mod error;
mod loading;
mod objects;

use std::cell::RefCell;

use error::WrappedReport as Wrap;
use inkjet::formatter::Html;
use inkjet::{Highlighter, Language};
use minijinja::value::Value;
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
    let path = format!("/static/style.css?v={}", stylesheet::load_hash(ctx, rev_id)?);

    env.add_function("stylesheet_path", move |state: &State| {
        try_with_page(state, |page| {
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

    env.add_filter("eval", eval);
    env.add_filter("timefmt", timefmt);
    env.add_filter("slug", slug::slugify::<String>);

    env.add_filter("highlight", move |body: String, token: String| {
        std::thread_local! {
            static HIGHLIGHTER: RefCell<Highlighter> = RefCell::new(Highlighter::new())
        };

        // No token means the block should be formatted as plain text
        if token.is_empty() {
            return Ok(Value::from(body));
        }

        let Some(lang) = Language::from_token(&token) else {
            let err = eyre!("A codeblock had a language token ('{token}'), but FTL could not find a matching language definition.")
                .note("Your codeblock's language token may just be malformed, or it could specify a language not bundled with FTL.")
                .suggestion("Provide a valid language token, or remove it to format the block as plain text.");
            
            return Err(Wrap::wrap(err))
        };

        let output = HIGHLIGHTER.with(|cell| {
            let mut highlighter = cell.borrow_mut();

            highlighter.highlight_to_string(
                lang,
                &Html,
                &body
            )
        }).map_err(Wrap::wrap)?;

        Ok(Value::from_safe_string(output))
    });

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

/// Attempts to fetch the "page" variable from the engine state and downcast it into
/// a [`Ticket`].
///
/// - If successful, it then executes the provided closure against the downcasted [`Ticket`]
/// and returns its output.
/// - If unsuccessful, it immediately returns [`None`].
fn try_with_page<F, R>(state: &State, op: F) -> Option<R>
where
    F: FnOnce(&Ticket) -> R,
{
    use std::sync::Arc;

    if let Some(value) = state.lookup("page") {
        if let Some(ticket) = value.downcast_object_ref::<Arc<Ticket>>() {
            return op(ticket).into();
        }
    }

    None
}
