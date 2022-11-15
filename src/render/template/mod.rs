mod loading;
mod querying;
mod pipelining;

use std::sync::Arc;

use loading::load_templates;
use minijinja::{
    value::Value,
    Environment, ErrorKind, State,
};
use rusqlite::Connection;

use super::{Bridge, Ticket};
use crate::prelude::*;

pub use loading::FTL_BUILTIN;

type TResult<T> = Result<T, minijinja::Error>;

/// Create a standard [`minijinja::Environment`] instance, and register all known globals, filters, functions, tests and templates with it.
pub fn make_environment(conn: &mut Connection, bridge_arc: &Arc<Bridge>) -> Result<Environment<'static>> {
    let mut environment = Environment::new();
    environment.set_source(load_templates(conn, &bridge_arc.rev_id)?);
    environment.add_global("config", Value::from_serializable(Config::global()));
    register_routines(&mut environment, bridge_arc)?;
    Ok(environment)
}

fn register_routines(environment: &mut Environment, bridge_arc: &Arc<Bridge>) -> Result<()> {
    let bridge = Arc::clone(bridge_arc);
    let query_fn = move |sql: String, params: Option<Value>| -> TResult<Value> {
        let query_result = bridge.query(sql, params)
            .map_err(|e| {
                let e: WrappedReport = e.into();
                minijinja::Error::new(
                    ErrorKind::UndefinedError,
                    "An error occurred when querying the database."
                ).with_source(e)
            })?;
        
        Ok(Value::from_serializable(&query_result))
    };

    let bridge = Arc::clone(bridge_arc);
    let query_filter = move |sql: String, params: Option<Value>| -> TResult<Value> {
        let query_result = bridge.query(sql, params)
            .map_err(|e| {
                let e: WrappedReport = e.into();
                minijinja::Error::new(
                    ErrorKind::UndefinedError,
                    "An error occurred when querying the database."
                ).with_source(e)
            })?;
        
        Ok(Value::from_serializable(&query_result))
    };

    let renderer = pipelining::prepare_pipeline(bridge_arc)?;
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
                let e: WrappedReport = e.into();
                Err(minijinja::Error::new(
                    ErrorKind::UndefinedError,
                    "An error was encountered during page rendering.",
                )
                .with_source(e))
            }
        }
    };

    let bridge = Arc::clone(bridge_arc);
    let resolve_link_fn = move |state: &State, path: String, optional: Option<bool>| -> TResult<Value> {
        let optional = optional.unwrap_or(false);
        let Some(value) = state.lookup("page") else {
            return Err(minijinja::Error::new(
                ErrorKind::InvalidOperation,
                "Link resolution must be called in the context of page rendering."
            ))
        };

        let ticket = value.downcast_object_ref::<Arc<Ticket>>()
            .expect("Page should be an instance of Ticket.");

        match bridge.cachebust_link(&path, Some(&ticket.page), true) {
            Ok(link) => Ok(Value::from_safe_string(link)),
            Err(e) => match optional {
                false => {
                    let e: WrappedReport = e.into();
                    Err(minijinja::Error::new(
                        ErrorKind::UndefinedError,
                        "An error occurred when resolving a non-optional link."
                    ).with_source(e))
                },
                true => Ok(Value::from(""))
            }
        }
    };

    environment.add_function("query", query_fn);
    environment.add_function("link", resolve_link_fn);

    environment.add_filter("query", query_filter);
    environment.add_filter("render", render_filter);
    #[allow(clippy::redundant_closure)] // Type inference fails without the closure.
    environment.add_filter("slugify", |input: String| slug::slugify(input) );
    environment.add_filter("markdown", pipelining::prepare_markdown_stateless());

    Ok(())
}

/// Flatten a [`minijinja::Error`] into a [`Report`], by either:
/// 1. Recursively downcasting its sources until a [`WrappedReport`] is found, which is then extracted and returned.
/// 2. Wrapping it in a new [`Report`] otherwise.
pub fn flatten_err(err: minijinja::Error) -> Report {
    let mut root_err = &err as &dyn std::error::Error;

    while let Some(next_err) = root_err.source() {
        if let Some(report) = next_err.downcast_ref::<WrappedReport>() {
            let message = format!(
                "An error was encountered during template evaluation (in template file \"{}\" at line no. {}).",
                err.name().unwrap_or("?"),
                err.line().unwrap_or(0)
            );

            return report.extract().wrap_err(message)
        }

        root_err = next_err;
    };

    Report::from(err)
        .wrap_err("An error was encountered during template evaluation.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_flattening() {
        let internal_error = minijinja::Error::new(
            ErrorKind::UndefinedError,
            "An error was generated by non-FTL code."
        );

        let report: WrappedReport = eyre!("An Eyre Report generated by FTL code.").into();
        let erased_error = minijinja::Error::new(
            ErrorKind::UndefinedError,
            "An error was generated by FTL code and obfuscated by MiniJinja."
        ).with_source(report);

        let internal_flattened = flatten_err(internal_error);
        let erased_flattened = flatten_err(erased_error);

        assert_eq!(internal_flattened.to_string(), "An error was encountered during template evaluation.");
        assert_eq!(erased_flattened.to_string(), "An error was encountered during template evaluation (in template file \"?\" at line no. 0).");
    }
}
