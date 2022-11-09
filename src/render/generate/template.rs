use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{params, Connection};
use serde_rusqlite::from_rows;
use minijinja::{Source, Environment};

use crate::{db, prelude::*};

#[derive(serde::Deserialize, Debug)]
struct Row {
    pub id: String,
    pub path: String,
    pub contents: String,
}

/// Create a standard [`minijinja::Environment`] instance, and register all known globals, filters, functions, tests and templates with it.
pub fn make_environment(conn: &mut Connection, rev_id: &str) -> Result<Environment<'static>> {
    let mut environment = Environment::new();
    let templates = load_templates(conn, rev_id)?;
    
    environment.set_source(load_templates(conn, rev_id)?);
    // register_filters(&mut tera);
    // register_functions(&mut tera);
    // register_tests(&mut tera);

    Ok(environment)
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
                    .trim_start_matches(SITE_TEMPLATE_DIRECTORY)
                    .trim_end_matches(".html"),
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
static MJ_INCLUDE_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\{% include "(.*?)"(?: ignore missing |\s)%\}"#).unwrap() );
// Example input: {% import "macros.html" as macros %}
// The first capture: macros.html
static MJ_IMPORT_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\{% import "(.*?)" as .* %\}"#).unwrap() );
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
            .trim_start_matches(SITE_TEMPLATE_DIRECTORY)
            .trim_end_matches(".html");

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
    capture(&MJ_IMPORT_REGEXP);
    capture(&MJ_EXTENDS_REGEXP);

    dependencies.sort_unstable();
    dependencies.dedup();

    dependencies.into_iter()
}
