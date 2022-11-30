mod error;
mod loading;
mod objects;
mod routines;

use minijinja::{
    Environment, 
    value::Value
};

use crate::prelude::*;

pub fn setup_environment(state: &State) -> Result<Environment<'_>> {
    let rev_id = state.get_working_rev();
    let stylesheet = format!("{rev_id}/style.css");

    let mut env = Environment::new();
    let source = loading::setup_source(state)?;

    env.set_source(source);
    env.add_global("CONFIG", Value::from_serializable(&state.config));
    env.add_global("REVISION_ID", Value::from_serializable(&*rev_id));
    env.add_global("STYLESHEET", Value::from_safe_string(stylesheet));
    routines::register(state, &mut env)?;
    
    Ok(env)
}