mod rendering;
mod querying;

use std::sync::Arc;

use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{params, Connection};
use serde_aux::serde_introspection::serde_introspect;
use serde_rusqlite::from_rows;
use minijinja::{Source, Environment, value::{Value, Object}, State, ErrorKind};

use crate::{db::{self, DbPool, data::Page}, prelude::*};
use super::{Message};

type TResult<T> = Result<T, minijinja::Error>;

#[derive(Debug)]
pub struct Ticket {
    pub inner: Value,
    pub page: Page,
    pub source: String,
}

impl Ticket {
    pub fn new(page: Page, mut source: String) -> Self {
        source.drain(..(page.offset as usize)).for_each(drop);

        Ticket {
            inner: Value::from_serializable(&page),
            page,
            source
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
        Box::new(
            serde_introspect::<Page>()
                .iter()
                .copied()
        )
    }
}

#[derive(Debug)]
pub struct DatabaseBridge {
    pub pool: DbPool,
    pub rev_id: String,
    pub consumer: Consumer<Message>,
}

impl DatabaseBridge {
    pub fn build(rev_id: &str, consumer: Consumer<Message>) -> Result<Arc<Self>> {
        let arc = Arc::new(
            Self {
                pool: db::make_pool()?,
                rev_id: rev_id.to_owned(),
                consumer
            }
        );
        Ok(arc)
    }
}

/// Create a standard [`minijinja::Environment`] instance, and register all known globals, filters, functions, tests and templates with it.
pub fn make_environment(conn: &mut Connection, bridge_arc: &Arc<DatabaseBridge>) -> Result<Environment<'static>> {
    let mut environment = Environment::new();    
    environment.set_source(load_templates(conn, &bridge_arc.rev_id)?);
    environment.add_global(
        "config", 
        Value::from_serializable(Config::global())
    );
    register_routines(&mut environment, bridge_arc)?;
    Ok(environment)
}

fn register_routines(environment: &mut Environment, bridge_arc: &Arc<DatabaseBridge>) -> Result<()> {
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
                    "An error was encountered during page rendering."
                ).with_source(e))
            }
        }
    };

    environment.add_function("query", query_fn);
    environment.add_filter("query", query_filter);
    environment.add_filter("render", render_filter);

    Ok(())
}

fn load_templates(conn: &mut Connection, rev_id: &str) -> Result<Source> {
    let rows = query_templates(conn, rev_id)?;
    let mut source = Source::new();
    
    rows
        .iter()
        .map(|row| {
            (
                row.path
                    .trim_start_matches(SITE_SRC_DIRECTORY)
                    .trim_start_matches(SITE_TEMPLATE_DIRECTORY),
                row.contents.as_str(),
            )
        })
        .try_for_each(|(name, contents)| -> Result<()> {
            source.add_template(name, contents)?;
            Ok(())
        })?;

    compute_ids(rows.as_slice(), conn, rev_id)
        .wrap_err("Failed to compute template dependency IDs.")?;

    Ok(source)
}

#[derive(serde::Deserialize, Debug)]
struct Row {
    pub id: String,
    pub path: String,
    pub contents: String,
}

/// Queries the database for the `id`, `path` and `contents` tables of all templates in the specified revision,
/// then packages the results into a [`Result<Vec<Row>>`].
fn query_templates(conn: &Connection, rev_id: &str) -> Result<Vec<Row>> {
    let mut stmt = conn.prepare(
        "
        SELECT input_files.id, path, contents FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.extension = 'html'
        AND input_files.contents NOT NULL;
    ",
    )?;

    let rows: Result<Vec<Row>> = from_rows::<Row>(stmt.query(params![&rev_id])?)
        .map(|x| x.wrap_err("SQLite deserialization error!"))
        .collect();

    rows
}

// Example input: {% include "included.html" %}
// The first capture: included.html
static MJ_INCLUDE_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\{% include "(.*?)" .* %\}"#).unwrap() );
// Example input: {% import "macros.html" as macros %}
// The first capture: macros.html
static MJ_FULL_IMPORT_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\{% import "(.*?)" as .* %\}"#).unwrap() );
// Example input: {% from "macros.html" import macro_a, macro_b %}
// The first capture: macros.html
static MJ_SELECTIVE_IMPORT_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\{% from "(.*?)" import .* %\}"#).unwrap() );
// Example input: {% extends "base.html" %}
// The first capture: base.html
static MJ_EXTENDS_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\{% extends "(.*?)" %\}"#).unwrap() );

/// Maps out the dependency set of each template in the given slice, hashes the sets into templating IDs, and inserts them into the templates table.
///
/// The procedure goes roughly like this:
/// - Attach a new in-memory database and initialize a few tables.
/// - Insert each template's name and ID into one of the tables.
/// - Match the contents of each template against a set of regular expressions to extract its immediate dependencies.
/// - Insert each template's direct dependencies into another table, where one column is the dependents's ID and the other is the dependency's ID.
/// - Using a recursive Common Table Expression, map out each template's dependency set (deduplicated using `UNION` and sorted by `id ASC`.)
/// - Insert the results into the on-disk templates table, which maps each templates name to many dependency IDs.
/// - Detach the in-memory database, deallocating its contents.
fn compute_ids<'a>(templates: &'a [Row], conn: &mut Connection, _rev_id: &str) -> Result<()> {
    // Attach and setup a new in-memory database for mapping dependency relations.
    let txn = conn.transaction()?;
    db::attach_mapping_database(&txn)?;

    // Prepare necessary statements for dependency mapping.
    let mut insert_template = txn.prepare(
        "
        INSERT OR IGNORE INTO map.templates 
        VALUES (?1, ?2);
    ",
    )?;

    let mut insert_dependency = txn.prepare(
        "
        INSERT OR IGNORE INTO map.dependencies 
        VALUES (?1, (SELECT id FROM map.templates WHERE name = ?2));
    ",
    )?;

    let mut query_set = txn.prepare(
        "
        WITH RECURSIVE 
            transitives (id) AS (
                SELECT id FROM map.templates
                WHERE id = ?1
                
                UNION
                
                SELECT dependency_id FROM map.dependencies
                JOIN transitives ON transitives.id = dependencies.parent_id
                LIMIT 255
            ),
            template_name (name) AS (
                SELECT name FROM map.templates
                WHERE id = ?1
            )
        
        INSERT OR IGNORE INTO templates
        SELECT template_name.name, transitives.id
        FROM template_name, transitives;
    ",
    )?;

    // Purge old template dependencies.
    // We *could* differentiate them based on revision,
    // but that would be pointless since we only care about the IDs for the current one.
    txn.execute("DELETE FROM templates;", [])?;

    // For each row in the templates slice:
    // 1. Trim its path to be relative to SITE_TEMPLATE_DIRECTORY.
    // 2. Insert the trimmed path and ID into the map.templates table.
    for row in templates {
        let trimmed_path = row
            .path
            .trim_start_matches(SITE_SRC_DIRECTORY)
            .trim_start_matches(SITE_TEMPLATE_DIRECTORY);
        
        insert_template.execute(params![trimmed_path, row.id])?;
    }

    // For each row in the templates slice:
    // 1. Match for the row's direct dependencies.
    // 2. Insert them into the map.dependencies table.
    for row in templates {
        for dependency in find_direct_dependencies(row) {
            insert_dependency.execute(params![row.id, dependency])?;
        }
    }

    // For each row in the templates slice, recurse (in SQL)
    // over its transitive dependencies and insert them into the
    // on-disk templates table.
    for row in templates {
        query_set.execute(params![&row.id])?;
    }

    // Drop prepared statements so the borrow checker will shut
    insert_template.finalize()?;
    insert_dependency.finalize()?;
    query_set.finalize()?;

    // Commit the above changes, then detatch (i.e. destroy) the in-memory mapping table.
    txn.commit()?;
    db::detach_mapping_database(conn)?;

    Ok(())
}

/// Parse the contents of the given [`Row`] for its direct dependencies using the `MJ_INCLUDE_*` regular expressions.
fn find_direct_dependencies(item: &'_ Row) -> impl Iterator<Item = &'_ str> {
    let mut dependencies: Vec<&str> = Vec::new();

    let mut capture = |regexp: &Regex| {
        regexp
            .captures_iter(&item.contents)
            .filter_map(|cap| cap.get(1))
            .map(|found| found.as_str())
            .map(|text| dependencies.push(text))
            .for_each(drop)
    };

    capture(&MJ_INCLUDE_REGEXP);
    capture(&MJ_FULL_IMPORT_REGEXP);
    capture(&MJ_SELECTIVE_IMPORT_REGEXP);
    capture(&MJ_EXTENDS_REGEXP);

    dependencies.sort_unstable();
    dependencies.dedup();

    dependencies.into_iter()
}