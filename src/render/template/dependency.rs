use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{params, Connection};

use super::Row;
use crate::{db, prelude::*};

// Example input: {% include "included.html" %}
// The first capture: included.html
static TERA_INCLUDE_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\{% include "(.*?)"(?: ignore missing |\s)%\}"#).unwrap() );
// Example input: {% import "macros.html" as macros %}
// The first capture: macros.html
static TERA_IMPORT_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\{% import "(.*?)" as .* %\}"#).unwrap() );
// Example input: {% extends "base.html" %}
// The first capture: base.html
static TERA_EXTENDS_REGEXP: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\{% extends "(.*?)" %\}"#).unwrap() );

// TODO: Could this be parallelized, at least in part?
/// Maps out the dependency set of each template in the given slice, hashes the sets into templating IDs, and inserts them into the templates table.
///
/// The procedure goes roughly like this:
/// - Attach a new in-memory database and initialize a few tables.
/// - Insert each template's name and ID into one of the tables.
/// - Match the contents of each template against a set of regular expressions to extract its immediate dependencies.
/// - Insert each template's direct dependencies into another table, where one column is the dependents's ID and the other is the dependency's ID.
/// - Using a recursive Common Table Expression, map out each template's dependency set (deduplicated using `UNION` and sorted by `id ASC`.)
/// - Fold the results of each recursive CTE query into a hasher, and insert the resulting hash into the on-disk templates table.
/// - Detach the in-memory database, deallocating its contents.
pub fn compute_ids<'a>(templates: &'a [Row], conn: &mut Connection, _rev_id: &str) -> Result<()> {
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
        
        INSERT OR REPLACE INTO templates
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
            .trim_end_matches(".tera");

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

    // For each row in the templates slice,
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

/// Parse the contents of the given [`Row`] for its direct dependencies using the `TERA_INCLUDE_*` regular expressions.
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

    capture(&TERA_INCLUDE_REGEXP);
    capture(&TERA_IMPORT_REGEXP);
    capture(&TERA_EXTENDS_REGEXP);

    dependencies.sort_unstable();
    dependencies.dedup();

    dependencies.into_iter()
}
