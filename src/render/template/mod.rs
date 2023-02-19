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

use objects::Highlighter;
use crate::prelude::*;

pub fn setup_environment(state: &State) -> Result<Environment<'static>> {
    let rev_id = state.get_rev();
    let stylesheet = format!("{rev_id}/style.css");
    let handle = DbHandle::new(state);

    let mut env = Environment::new();
    let source = loading::setup_source(state)?;

    env.set_source(source);
    env.add_global("CONFIG", Value::from_serializable(&state.config));
    env.add_global("REVISION_ID", Value::from_serializable(&*rev_id));
    env.add_global("STYLESHEET", Value::from_safe_string(stylesheet));
    env.add_global("DB", Value::from_object(handle));
    register_routines(state, &mut env)?;
    
    Ok(env)
}

pub fn register_routines(state: &State, env: &mut Environment<'_>) -> Result<()> {
    env.add_function("eval", eval);

    env.add_filter("eval", eval);
    env.add_filter("slug", slug::slugify::<String>);

    let hili = Highlighter::new(state)?;
    env.add_filter("highlight", move |body, token| {
        hili.highlight(body, token)
            .map(Value::from)
            .map_err(Wrap::wrap)
    });

    Ok(())
}

fn eval(state: &MJState, template: String) -> MJResult {
    state.env().render_str(
        &template,
        context!(page => state.lookup("page"))
    ).map(Value::from_safe_string)
}