mod error;
mod loading;
mod objects;

pub use error::*;
use error::{MJResult, WrappedReport as Wrap};
use minijinja::value::Value;
use minijinja::{context, Environment, State as MJState};
pub use objects::*;
use objects::{DbHandle, Highlighter};

use super::stylesheet;
use crate::prelude::*;

pub fn setup_environment(ctx: &Context, rev_id: &RevisionID) -> Result<Environment<'static>> {
    let stylesheet = format!("static/style.{}.css", stylesheet::load_hash(ctx, rev_id)?);
    let db = DbHandle::new(ctx, rev_id);

    let mut env = Environment::new();
    loading::setup_templates(ctx, rev_id, &mut env)?;

    env.add_global("CONFIG", Value::from_serializable(&ctx.config));
    env.add_global("REVISION_ID", Value::from_serializable(&rev_id.as_ref()));
    env.add_global("STYLESHEET", Value::from_safe_string(stylesheet));
    env.add_global("DB", Value::from_object(db));
    register_routines(ctx, rev_id, &mut env)?;

    Ok(env)
}

pub fn register_routines(ctx: &Context, rev_id: &RevisionID, env: &mut Environment<'_>) -> Result<()> {
    env.add_function("eval", eval);
    env.add_function("raise", raise);

    env.add_filter("eval", eval);
    env.add_filter("timefmt", timefmt);
    env.add_filter("slug", slug::slugify::<String>);

    let hili = Highlighter::new(ctx, rev_id)?;
    env.add_filter("highlight", move |body, token| {
        hili.highlight(body, token)
            .map(Value::from_safe_string)
            .map_err(Wrap::wrap)
    });

    let db = DbHandle::new(ctx, rev_id);
    env.add_filter("query", move |sql, params| db.query(sql, params));

    Ok(())
}

fn eval(state: &MJState, template: String) -> MJResult {
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
