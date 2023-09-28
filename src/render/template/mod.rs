mod error;
mod loading;
mod objects;

use base64::engine::general_purpose;
use error::WrappedReport as Wrap;
use minijinja::value::{Value, ValueKind};
use minijinja::{context, Environment, State};

pub use error::*;
pub use objects::*;

use super::stylesheet;

use crate::db::*;
use crate::prelude::*;

pub fn setup_environment(ctx: &Context, rev_id: &RevisionID) -> Result<Environment<'static>> {
    let db = DbHandle::new(ctx, rev_id);

    let mut env = Environment::new();
    loading::setup_templates(ctx, rev_id, &mut env)?;

    env.add_global("CONFIG", Value::from_serializable(&ctx.config));
    env.add_global("REVISION_ID", Value::from_serializable(&rev_id.as_ref()));
    env.add_global("DB", Value::from_object(db));
    register_routines(ctx, rev_id, &mut env)?;

    Ok(env)
}

pub fn register_routines(
    ctx: &Context,
    rev_id: &RevisionID,
    env: &mut Environment<'_>,
) -> Result<()> {
    env.add_function("eval", eval);
    env.add_function("raise", raise);

    let ids = stylesheet::load_all_ids(ctx, rev_id)?;
    let path = format!(
        "/static/style.css?v={}",
        stylesheet::load_hash(ctx, rev_id)?
    );

    env.add_function("stylesheet_path", move |state: &State| {
        try_with_ticket(state, |page| {
            for id in &ids {
                // Unwrap justification: register_dependency can only fail
                // if you're registering a template dependency
                page.register_dependency(
                    Relation::PageAsset,
                    id.to_owned()
                ).unwrap();
            }
        });

        Ok(path.to_owned())
    });

    env.add_function("dbg", dbg);
    env.add_function("info", info);
    env.add_function("warn", warn);
    env.add_function("emojify", emojify);
    env.add_function("getenv", getenv);
    env.add_function("shell", shell);
    env.add_function("base64_enc", base64_enc);
    env.add_function("base64_dec", base64_dec);

    env.add_filter("eval", eval);
    env.add_filter("timefmt", timefmt);
    env.add_filter("slug", slug::slugify::<String>);
    env.add_filter("emojify", emojify);
    env.add_filter("base64_enc", base64_enc);
    env.add_filter("base64_dec", base64_dec);

    let db = DbHandle::new(ctx, rev_id);
    env.add_filter("query", move |sql, params| db.query(sql, params));

    Ok(())
}

fn eval(state: &State, template: String) -> MJResult {
    state
        .env()
        .render_named_str("<eval>", &template, context!(page => state.lookup("page")))
        .map(Value::from_safe_string)
}

fn raise(message: String) -> MJResult {
    Err(MJError::new(MJErrorKind::InvalidOperation, message))
}

fn timefmt(input: String, format: String) -> MJResult {
    use chrono::DateTime;

    let datetime = DateTime::parse_from_rfc3339(&input)
        .map_err(|err| eyre!("Datetime input parsing error: {err}"))
        .map_err(Wrap::wrap)?;

    // Workaround to avoid panicking when the user-provided format string is invalid.
    // Will be obsolete once https://github.com/chronotope/chrono/pull/902 is merged.
    std::panic::set_hook(Box::new(|_| ()));
    let formatted = std::panic::catch_unwind(|| datetime.format(&format).to_string())
        .map_err(|_| Wrap::wrap(eyre!("Invalid datetime format string ({format})")))?;
    let _ = std::panic::take_hook();

    Ok(Value::from(formatted))
}

// Attempt to convert a given emoji shortcode (such as :smile:) into the corresponding Unicode character.
//
// Returns the shortcode unchanged if no match was found.
fn emojify(input: String) -> String {
    let code = input
        .trim_start_matches(':')
        .trim_end_matches(':');

    match gh_emoji::get(code) {
        Some(emoji) => emoji.to_owned(),
        None => input
    }
}

// Attempt to look up the specified environment variable.
//
// Returns an empty string if the variable is not found.
fn getenv(key: String) -> String {
    std::env::var(key).unwrap_or_else(|e| {
        warn!("Failed to get environment variable - {e:?}");
        String::new()
    })
}

fn dbg(state: &State, msg: String) {
    debug!(
        "<{}> {}",
        state.name(),
        msg
    )
}

fn info(state: &State, msg: String) {
    info!(
        "<{}> {}",
        state.name(),
        msg
    )
}

fn warn(state: &State, msg: String) {
    warn!(
        "<{}> {}",
        state.name(),
        msg
    )
}

fn shell(script: String) -> MJResult {
    use std::process::Command;

    #[cfg(target_family = "windows")]
    const SH_NAME: &str = "cmd";
    #[cfg(target_family = "windows")]
    const SH_ARG: &str = "/C";
    #[cfg(target_family = "unix")]
    const SH_NAME: &str = "sh";
    #[cfg(target_family = "unix")]
    const SH_ARG: &str = "-c";

    let output = Command::new(SH_NAME)
        .args([SH_ARG, &script])
        .output()
        .map_err(Wrap::wrap)?;

    String::from_utf8(output.stdout)
        .map(Value::from)
        .map_err(Wrap::wrap)
}

fn base64_enc(input: Value) -> MJResult {
    use base64::Engine;

    let engine = general_purpose::STANDARD_NO_PAD;

    match input.kind() {
        ValueKind::String => Ok(Value::from(
            engine.encode(input.as_str().unwrap())
        )),
        ValueKind::Bytes => Ok(Value::from(
            engine.encode(input.as_bytes().unwrap())
        )),
        _ => Err(MJError::new(
            MJErrorKind::InvalidOperation,
            format!("Base64 encoding can only accept strings and bytes (got {})", input.kind())
        ))
    }
}

fn base64_dec(input: String) -> MJResult {
    use base64::Engine;

    let engine = general_purpose::STANDARD_NO_PAD;

    engine.decode(input)
        .map(Value::from)
        .map_err(Wrap::wrap)
}

// markdown
// hashing
// now (time)
// regex?

/// Attempts to fetch the "page" variable from the engine state and downcast it into
/// a [`Ticket`].
///
/// - If successful, it then executes the provided closure against the downcasted [`Ticket`]
/// and returns its output.
/// - If unsuccessful, it immediately returns [`None`].
fn try_with_ticket<F, R>(state: &State, op: F) -> Option<R>
where
    F: FnOnce(&Ticket) -> R,
{
    use minijinja_stack_ref::StackHandle as Handle;

    if let Some(value) = state.lookup("page") {
        if let Some(ticket) = value.downcast_object_ref::<Handle<Ticket>>() {
            return Some(ticket.with(op))
        }
    }

    None
}
