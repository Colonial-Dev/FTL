use minijinja::Source;
use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{params, Connection};
use serde_rusqlite::from_rows;

use crate::{db, prelude::*};

pub const FTL_BUILTIN_NAME: &str = "FTL_BUILTIN.html";
const FTL_BUILTIN_CONTENT: &str = "{{ page | render }}";

pub fn load_templates(conn: &mut Connection, rev_id: &str) -> Result<Source> {
    let rows = query_templates(conn, rev_id)?;
    let mut source = Source::new();

    rows.iter()
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

    compute_ids(rows.as_slice(), conn).wrap_err("Failed to compute template dependency IDs.")?;
    
    source.add_template(FTL_BUILTIN_NAME, FTL_BUILTIN_CONTENT)
        .expect("FTL builtin template is invalid!");
    
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
static MJ_INCLUDE_REGEXP: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\{%\s?include "(.*?)".*\s?%\}"#).unwrap());
// Example input: {% import "macros.html" as macros %}
// The first capture: macros.html
static MJ_FULL_IMPORT_REGEXP: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\{%\s?import "(.*?)" as .*\s?%\}"#).unwrap());
// Example input: {% from "macros.html" import macro_a, macro_b %}
// The first capture: macros.html
static MJ_SELECTIVE_IMPORT_REGEXP: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\{%\s?from "(.*?)" import .*\s?%\}"#).unwrap());
// Example input: {% extends "base.html" %}
// The first capture: base.html
static MJ_EXTENDS_REGEXP: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\{%\s?extends "(.*?)"\s?%\}"#).unwrap());

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
fn compute_ids<'a>(templates: &'a [Row], conn: &mut Connection) -> Result<()> {
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

#[cfg(test)]
mod dependency_resolution {
    use super::*;

    fn get_capture<'a>(regexp: &Regex, haystack: &'a str) -> Option<&'a str> {
        regexp
            .captures_iter(haystack)
            .filter_map(|cap| cap.get(1))
            .map(|found| found.as_str())
            .next()
    }

    // Minijinja does not consider spacing when parsing delimiters, so we need to check that our
    // regexes treat them the same way.
    #[test]
    fn include_regexp() {
        let double_spaced = get_capture(&MJ_INCLUDE_REGEXP, "{% include \"test.html\" %}");
        let left_spaced = get_capture(&MJ_INCLUDE_REGEXP, "{% include \"test.html\"%}");
        let right_spaced = get_capture(&MJ_INCLUDE_REGEXP, "{%include \"test.html\" %}");
        let no_spaced = get_capture(&MJ_INCLUDE_REGEXP, "{%include \"test.html\"%}");

        assert_eq!(double_spaced, Some("test.html"));
        assert_eq!(left_spaced, Some("test.html"));
        assert_eq!(right_spaced, Some("test.html"));
        assert_eq!(no_spaced, Some("test.html"));
    }

    #[test]
    fn full_import_regexp() {
        let double_spaced = get_capture(&MJ_FULL_IMPORT_REGEXP, "{% import \"test.html\" as test %}");
        let left_spaced = get_capture(&MJ_FULL_IMPORT_REGEXP, "{% import \"test.html\" as test%}");
        let right_spaced = get_capture(&MJ_FULL_IMPORT_REGEXP, "{%import \"test.html\" as test %}");
        let no_spaced = get_capture(&MJ_FULL_IMPORT_REGEXP, "{%import \"test.html\" as test%}");

        assert_eq!(double_spaced, Some("test.html"));
        assert_eq!(left_spaced, Some("test.html"));
        assert_eq!(right_spaced, Some("test.html"));
        assert_eq!(no_spaced, Some("test.html"));
    }

    #[test]
    fn selective_import_regexp() {
        let double_spaced = get_capture(&MJ_SELECTIVE_IMPORT_REGEXP, "{% from \"test.html\" import macro %}");
        let left_spaced = get_capture(&MJ_SELECTIVE_IMPORT_REGEXP, "{% from \"test.html\" import macro%}");
        let right_spaced = get_capture(&MJ_SELECTIVE_IMPORT_REGEXP, "{%from \"test.html\" import macro %}");
        let no_spaced = get_capture(&MJ_SELECTIVE_IMPORT_REGEXP, "{%from \"test.html\" import macro%}");

        assert_eq!(double_spaced, Some("test.html"));
        assert_eq!(left_spaced, Some("test.html"));
        assert_eq!(right_spaced, Some("test.html"));
        assert_eq!(no_spaced, Some("test.html"));
    }

    #[test]
    fn extends_regexp() {
        let double_spaced = get_capture(&MJ_EXTENDS_REGEXP, "{% extends \"test.html\" %}");
        let left_spaced = get_capture(&MJ_EXTENDS_REGEXP, "{% extends \"test.html\"%}");
        let right_spaced = get_capture(&MJ_EXTENDS_REGEXP, "{%extends \"test.html\" %}");
        let no_spaced = get_capture(&MJ_EXTENDS_REGEXP, "{%extends \"test.html\"%}");

        assert_eq!(double_spaced, Some("test.html"));
        assert_eq!(left_spaced, Some("test.html"));
        assert_eq!(right_spaced, Some("test.html"));
        assert_eq!(no_spaced, Some("test.html"));
    }

    #[derive(serde::Deserialize, Debug)]
    struct Template {
        pub name: String,
        pub id: String,
    }

    #[test]
    #[allow(clippy::needless_collect)]
    fn sanity_check() {
        let mut conn = Connection::open_in_memory().unwrap();
        db::PRIME_MIGRATIONS.to_latest(&mut conn).unwrap();

        let alpha_row = Row {
            id: "ALPHA_ID".to_string(),
            path: "alpha.html".to_string(),
            contents: r#"{% include "beta.html" %}"#.to_string(),
        };

        let beta_row = Row {
            id: "BETA_ID".to_string(),
            path: "beta.html".to_string(),
            contents: "{% include \"gamma.html\" %}\n{% include \"delta.html\" %}".to_string(),
        };

        let gamma_row = Row {
            id: "GAMMA_ID".to_string(),
            path: "gamma.html".to_string(),
            contents: String::new(),
        };

        let delta_row = Row {
            id: "DELTA_ID".to_string(),
            path: "delta.html".to_string(),
            contents: String::new(),
        };

        let rows = vec![alpha_row, beta_row, gamma_row, delta_row];
        compute_ids(&rows, &mut conn).unwrap();

        let mut stmt = conn
            .prepare(
                "
            SELECT * FROM templates
            WHERE name = ?1
        ",
            )
            .unwrap();

        let alpha_deps: Vec<Template> =
            from_rows::<Template>(stmt.query(params![rows[0].path]).unwrap())
                .map(|x| x.unwrap())
                .collect();

        assert_eq!(alpha_deps.len(), 4);
        assert_eq!(alpha_deps[0].id, "ALPHA_ID");
        assert_eq!(alpha_deps[1].name, "alpha.html");
        assert_eq!(alpha_deps[1].id, "BETA_ID");
        assert_eq!(alpha_deps[2].id, "DELTA_ID");
        assert_eq!(alpha_deps[3].id, "GAMMA_ID");

        let beta_deps: Vec<Template> =
            from_rows::<Template>(stmt.query(params![rows[1].path]).unwrap())
                .map(|x| x.unwrap())
                .collect();

        assert_eq!(beta_deps.len(), 3);
        assert_eq!(beta_deps[0].id, "BETA_ID");
        assert_eq!(beta_deps[0].name, "beta.html");
        assert_eq!(beta_deps[1].id, "DELTA_ID");
        assert_eq!(beta_deps[2].id, "GAMMA_ID");

        let gamma_deps: Vec<Template> =
            from_rows::<Template>(stmt.query(params![rows[2].path]).unwrap())
                .map(|x| x.unwrap())
                .collect();

        assert_eq!(gamma_deps.len(), 1);
        assert_eq!(gamma_deps[0].id, "GAMMA_ID");
        assert_eq!(gamma_deps[0].name, "gamma.html");

        let delta_deps: Vec<Template> =
            from_rows::<Template>(stmt.query(params![rows[3].path]).unwrap())
                .map(|x| x.unwrap())
                .collect();

        assert_eq!(delta_deps.len(), 1);
        assert_eq!(delta_deps[0].id, "DELTA_ID");
        assert_eq!(delta_deps[0].name, "delta.html");
    }
}
