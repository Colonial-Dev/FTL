mod loading;
mod querying;
mod rendering;

use std::sync::{Arc, RwLock};

use loading::load_templates;
use minijinja::{
    value::{Object, Value},
    Environment, ErrorKind, State,
};
use rusqlite::Connection;
use serde_aux::serde_introspection::serde_introspect;

use super::Engine;
use crate::{db::data::Page, prelude::*};

type TResult<T> = Result<T, minijinja::Error>;

#[derive(Debug)]
pub struct Ticket {
    pub inner: Value,
    pub page: Page,
    pub source: RwLock<String>,
}

impl Ticket {
    pub fn new(page: Page, mut source: String) -> Self {
        source.drain(..(page.offset as usize)).for_each(drop);

        Ticket {
            inner: Value::from_serializable(&page),
            page,
            source: RwLock::new(source),
        }
    }
}

impl std::fmt::Display for Ticket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl Object for Ticket {
    fn get_attr(&self, name: &str) -> Option<Value> {
        self.inner.get_attr(name).ok()
    }

    fn attributes(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(serde_introspect::<Page>().iter().copied())
    }
}

/// Create a standard [`minijinja::Environment`] instance, and register all known globals, filters, functions, tests and templates with it.
pub fn make_environment(
    conn: &mut Connection,
    bridge_arc: &Arc<Engine>,
) -> Result<Environment<'static>> {
    let mut environment = Environment::new();
    environment.set_source(load_templates(conn, &bridge_arc.rev_id)?);
    environment.add_global("config", Value::from_serializable(Config::global()));
    register_routines(&mut environment, bridge_arc)?;
    Ok(environment)
}

fn register_routines(environment: &mut Environment, bridge_arc: &Arc<Engine>) -> Result<()> {
    let bridge = Arc::clone(bridge_arc);
    let query_fn = move |sql: String, params: Option<Value>| -> TResult<Value> {
        let query_result = bridge.query(sql, params)?;
        Ok(Value::from_serializable(&query_result))
    };

    let bridge = Arc::clone(bridge_arc);
    let query_filter = move |sql: String, params: Option<Value>| -> TResult<Value> {
        let query_result = bridge.query(sql, params)?;
        Ok(Value::from_serializable(&query_result))
    };

    let renderer = rendering::prepare_renderer(bridge_arc)?;
    let render_filter = move |state: &State, ticket: Value| -> TResult<Value> {
        let Some(ticket) = ticket.downcast_object_ref::<Arc<Ticket>>() else {
            return Err(minijinja::Error::new(
                ErrorKind::InvalidOperation,
                "The render filter only supports Page objects."
            ));
        };

        match renderer(state, ticket) {
            Ok(rendered) => Ok(Value::from_safe_string(rendered)),
            Err(e) => {
                let e: SizedReport = e.into();
                Err(minijinja::Error::new(
                    ErrorKind::UndefinedError,
                    "An error was encountered during page rendering.",
                )
                .with_source(e))
            }
        }
    };

    environment.add_function("query", query_fn);
    environment.add_filter("query", query_filter);
    environment.add_filter("render", render_filter);

    Ok(())
}
