//! Template loading procedures, including validation and dependency resolution.

use itertools::Itertools;
use minijinja::Environment;

use crate::record;
use crate::db::{Connection, AUX_DOWN, AUX_UP};
use crate::parse::Dependency;
use crate::prelude::*;

const BUILTINS: &[&str] = &[
    include_str!("builtins/ftl_default.html"),
    include_str!("builtins/eval.html"),
];

record! {
    Name     => Row,
    id       => String,
    path     => String,
    contents => String
}

/// Loads all user-provided and builtin templates into a [`Source`]
pub fn setup_templates(ctx: &Context, rev_id: &RevisionID, env: &mut Environment) -> Result<()> {
    let mut conn = ctx.db.get_rw()?;
    
    let mut query = conn.prepare("
        SELECT input_files.id, path, contents FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.extension = 'html'
        AND input_files.contents NOT NULL;
    ")?;

    let rows: Vec<_> = query
        .query_and_then([rev_id.as_ref()], Row::from_row)?
        .try_collect()?;

    query.finalize()?;

    compute_dependencies(&mut conn, &rows)?;

    rows.into_iter()
        .map(|row| {
            (
                row.path.trim_start_matches(SITE_TEMPLATE_PATH).to_owned(),
                row.contents,
            )
        })
        .chain(load_builtins())
        .try_for_each(|(name, contents)| env.add_template_owned(name, contents))?;

    Ok(())
}

fn load_builtins() -> impl Iterator<Item = (String, String)> {
    BUILTINS
        .iter()
        .map(|template| {
            template
                .split_once('\n')
                .expect("FTL builtin should have content.")
        })
        .map(|(name, content)| (name.to_owned(), content.to_owned()))
}

fn compute_dependencies(conn: &mut Connection, templates: &[Row]) -> Result<()> {
    let txn = conn.transaction()?;

    // Open and setup an in-memory database for use as our working space.
    txn.execute_batch(AUX_UP)?;

    // Purge old template dependencies from the on-disk database.
    //
    // We *could* differentiate them based on revision, but that would
    // be pointless since we only care about the current one.
    txn.execute("DELETE FROM dependencies WHERE relation = 1;", [])?;

    // Prepare all the necessary statements for dependency mapping.
    let insert_template = "
        INSERT OR IGNORE INTO map.templates
        VALUES (?1, ?2);
    ";

    let insert_dependency = "
        INSERT OR IGNORE INTO map.dependencies
        VALUES (?1, (SELECT id FROM map.templates WHERE name = ?2));
    ";

    let query_set = "
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

        INSERT OR IGNORE INTO dependencies
        SELECT 1, template_name.name, transitives.id
        FROM template_name, transitives;
    ";

    let mut insert_template = txn.prepare(insert_template)?;
    let mut insert_dependency = txn.prepare(insert_dependency)?;
    let mut query_set = txn.prepare(query_set)?;

    // For each template:
    // 1. Trim its path to be relative to SITE_TEMPLATE_PATH.
    // 2. Insert the trimmed path and the template's ID into the map.templates table.
    for row in templates {
        let trimmed_path = row.path.trim_start_matches(SITE_TEMPLATE_PATH);

        insert_template.execute(
            [trimmed_path, row.id.as_str()]
        )?;
    }

    // For each template:
    // 1. Scan for its direct dependencies.
    // 2. Insert them into the map.dependencies table.
    for row in templates {
        for dependency in Dependency::parse_many(&row.contents)? {
            insert_dependency.execute(
                [row.id.as_str(), dependency]
            )?;
        }
    }

    // For each row in the templates slice, recurse (in SQL)
    // over its transitive dependencies and insert them into the
    // on-disk templates table.
    for row in templates {
        query_set.execute([row.id.as_str()])?;
    }

    insert_template.finalize()?;
    insert_dependency.finalize()?;
    query_set.finalize()?;

    // Commit the above changes, then detatch/destroy the in-memory database.
    txn.commit()?;
    conn.execute_batch(AUX_DOWN)?;

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::db::{IN_MEMORY, PRIME_UP};

    #[test]
    fn builtin_templates() {
        let mut env = Environment::new();

        for (name, data) in load_builtins() {
            env.add_template_owned(name, data).unwrap();
        }
    }

    record! {
        Name => Template,
        name => String,
        id   => String
    }

    #[test]
    #[allow(clippy::needless_collect)]
    /// Imperative "sanity check" that ensures dependency mapping works as expected.
    fn sanity_check() -> Result<()> {
        let mut conn = Connection::open(IN_MEMORY).unwrap();
        conn.execute_batch(PRIME_UP).unwrap();

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
        compute_dependencies(&mut conn, &rows).unwrap();

        let mut query = conn.prepare("
            SELECT parent AS name, child AS id FROM dependencies
            WHERE parent = ?1
            AND relation = 1
        ")?;

        let alpha_deps: Vec<Template> = query
            .query_and_then([rows[0].path.as_str()], Template::from_row)?
            .map(Result::unwrap)
            .collect();

        assert_eq!(alpha_deps.len(), 4);
        assert_eq!(alpha_deps[0].id, "ALPHA_ID");
        assert_eq!(alpha_deps[1].name, "alpha.html");
        assert_eq!(alpha_deps[1].id, "BETA_ID");
        assert_eq!(alpha_deps[2].id, "DELTA_ID");
        assert_eq!(alpha_deps[3].id, "GAMMA_ID");

        let beta_deps: Vec<Template> = query
            .query_and_then([rows[1].path.as_str()], Template::from_row)?
            .map(Result::unwrap)
            .collect();

        assert_eq!(beta_deps.len(), 3);
        assert_eq!(beta_deps[0].id, "BETA_ID");
        assert_eq!(beta_deps[0].name, "beta.html");
        assert_eq!(beta_deps[1].id, "DELTA_ID");
        assert_eq!(beta_deps[2].id, "GAMMA_ID");

        let gamma_deps: Vec<Template> = query
            .query_and_then([rows[2].path.as_str()], Template::from_row)?
            .map(Result::unwrap)
            .collect();

        assert_eq!(gamma_deps.len(), 1);
        assert_eq!(gamma_deps[0].id, "GAMMA_ID");
        assert_eq!(gamma_deps[0].name, "gamma.html");

        let delta_deps: Vec<Template> = query
            .query_and_then([rows[3].path.as_str()], Template::from_row)?
            .map(Result::unwrap)
            .collect();

        assert_eq!(delta_deps.len(), 1);
        assert_eq!(delta_deps[0].id, "DELTA_ID");
        assert_eq!(delta_deps[0].name, "delta.html");

        Ok(())
    }
}
