mod error;
mod loading;
mod objects;
    
pub use error::*;
pub use objects::*;

use minijinja::{
    context,
    Environment, 
    value::Value,
    State as MJState
};

use error::{MJResult, WrappedReport as Wrap};

use objects::{Highlighter, DbHandle};
use crate::prelude::*;
use super::stylesheet;

pub fn setup_environment(state: &State) -> Result<Environment<'static>> {
    let rev_id = state.get_rev();
    let stylesheet = format!(
        "static/style.{}.css",
        stylesheet::load_hash(state)?
    );

    let mut env = Environment::new();
    let source = loading::setup_source(state)?;

    env.set_source(source);
    env.add_global("CONFIG", Value::from_serializable(&state.config));
    env.add_global("REVISION_ID", Value::from_serializable(&*rev_id));
    env.add_global("STYLESHEET", Value::from_safe_string(stylesheet));
    register_routines(state, &mut env)?;
    
    Ok(env)
}

pub fn register_routines(state: &State, env: &mut Environment<'_>) -> Result<()> {
    env.add_function("eval", eval);

    env.add_filter("eval", eval);
    env.add_filter("timefmt", timefmt);
    env.add_filter("slug", slug::slugify::<String>);

    let hili = Highlighter::new(state)?;
    env.add_filter("highlight", move |body, token| {
        hili.highlight(body, token)
            .map(Value::from_safe_string)
            .map_err(Wrap::wrap)
    });

    let db = DbHandle::new(state);
    env.add_filter("query", move |sql, params| {
        db.query(sql, params)
    });

    Ok(())
}

fn eval(state: &MJState, template: String) -> MJResult {
    state.env().render_str(
        &template,
        context!(page => state.lookup("page"))
    ).map(Value::from_safe_string)
}

fn timefmt(input: String, format: String) -> MJResult {
    use chrono::DateTime;

    let datetime = DateTime::parse_from_rfc3339(&input)
        .map_err(|err| eyre!("Datetime input parsing error: {err}"))
        .map_err(Wrap::wrap)?;

    // Workaround to avoid panicking when the user-provided format string is invalid.
    // Will be obsolete once https://github.com/chronotope/chrono/pull/902 is merged.
    std::panic::set_hook(Box::new(|_| ()));
    let formatted = std::panic::catch_unwind(|| {
        datetime.format(&format).to_string()
    })
    .map_err(|_| {
        Wrap::wrap(eyre!("Invalid datetime format string ({format})"))
    })?;
    let _ = std::panic::take_hook();

    Ok(Value::from(formatted))
}