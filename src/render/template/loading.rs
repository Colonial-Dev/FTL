//! Template loading procedures, including validation and dependency resolution.

use minijinja::Environment;

use crate::{
    poll,
    parse::Dependency,
    prelude::*, 
    db::{
        AUX_UP, AUX_DOWN,
        Connection, Queryable,
        Statement, StatementExt
    }
};

const BUILTINS: &[&str] = &[
    include_str!("builtins/ftl_default.html"),
    include_str!("builtins/eval.html"),
    include_str!("builtins/ftl_codeblock.html"),
];

#[derive(Debug)]
struct Row {
    pub id: String,
    pub path: String,
    pub contents: String,
}

impl Queryable for Row {
    fn read_query(row: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            id: row.read_string("id")?,
            path: row.read_string("path")?,
            contents: row.read_string("contents")?
        })
    }
}

/// Loads all user-provided and builtin templates into a [`Source`]
pub fn setup_templates(state: &State, env: &mut Environment) -> Result<()> {
    let conn = state.db.get_rw()?;
    let rev_id = state.get_rev();

    let query = "
        SELECT input_files.id, path, contents FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.extension = 'html'
        AND input_files.contents NOT NULL;
    ";
    let params = Some((1, rev_id.as_str()));

    let rows = conn
        .prepare_reader(query, params)?
        .collect::<MaybeVec<Row>>()?;
    
    compute_dependencies(&conn, &rows)?;

    rows
        .into_iter()
        .map(|row| {
            (
                row.path
                    .trim_start_matches(SITE_SRC_PATH)
                    .trim_start_matches(SITE_TEMPLATE_PATH)
                    .to_owned(),
                row.contents
            )
        })
        .chain(load_builtins())
        .try_for_each(|(name, contents)| {
            env.add_template_owned(name, contents)
        })?;
    
    Ok(())
}

fn load_builtins() -> impl Iterator<Item = (String, String)> {
    BUILTINS.iter()
        .map(|template| {
            template.split_once('\n').expect("FTL builtin should have content.")
        })
        .map(|(name, content)| {
            (name.to_owned(), content.to_owned())
        })
}

fn compute_dependencies(conn: &Connection, templates: &[Row]) -> Result<()> {
    let txn = conn.open_transaction()?;

    // Open and setup an in-memory database for use as our working space.
    conn.execute(AUX_UP)?;

    // Purge old template dependencies from the on-disk database.
    //
    // We *could* differentiate them based on revision, but that would
    // be pointless since we only care about the current one.
    conn.execute("DELETE FROM dependencies WHERE relation = 1;")?;

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

    let mut insert_template = conn.prepare(insert_template)?;
    let mut insert_dependency = conn.prepare(insert_dependency)?;
    let mut query_set = conn.prepare(query_set)?;

    // For each template:
    // 1. Trim its path to be relative to SITE_TEMPLATE_PATH.
    // 2. Insert the trimmed path and the template's ID into the map.templates table.
    for row in templates {
        let trimmed_path = row
            .path
            .trim_start_matches(SITE_SRC_PATH)
            .trim_start_matches(SITE_TEMPLATE_PATH);

        insert_template.reset()?;
        insert_template.bind((1, trimmed_path))?;
        insert_template.bind((2, row.id.as_str()))?;
        poll!(insert_template)
    }

    // For each template:
    // 1. Scan for its direct dependencies.
    // 2. Insert them into the map.dependencies table.
    for row in templates {
        for dependency in Dependency::parse_many(&row.contents)? {
            insert_dependency.reset()?;
            insert_dependency.bind((1, row.id.as_str()))?;
            insert_dependency.bind((2, dependency))?;
            poll!(insert_dependency)
        }
    }

    // For each row in the templates slice, recurse (in SQL)
    // over its transitive dependencies and insert them into the
    // on-disk templates table.
    for row in templates {
        query_set.reset()?;
        query_set.bind((1, row.id.as_str()))?;
        poll!(query_set)
    }

    // Commit the above changes, then detatch/destroy the in-memory database.
    txn.commit()?;
    conn.execute(AUX_DOWN)?;

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

    #[derive(serde::Deserialize, Debug)]
    struct Template {
        pub name: String,
        pub id: String,
    }

    impl Queryable for Template {
        fn read_query(row: &Statement<'_>) -> Result<Self> {
            Ok(Self {
                name: row.read_string("name")?,
                id: row.read_string("id")?
            })
        }
    }

    #[test]
    #[allow(clippy::needless_collect)]
    /// Imperative "sanity check" that ensures dependency mapping works as expected.
    fn sanity_check() {
        let conn = Connection::open(IN_MEMORY).unwrap();
        conn.execute(PRIME_UP).unwrap();

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
            contents: String::new()
        };

        let delta_row = Row {
            id: "DELTA_ID".to_string(),
            path: "delta.html".to_string(),
            contents: String::new()
        };

        let rows = vec![alpha_row, beta_row, gamma_row, delta_row];
        compute_dependencies(&conn, &rows).unwrap();

        let query = "
            SELECT parent AS name, child AS id FROM dependencies
            WHERE parent = ?1
            AND relation = 1
        ";

        let alpha_deps: Vec<Template> = conn.prepare_reader(query, Some((1, rows[0].path.as_str())))
            .unwrap()
            .map(Result::unwrap)
            .collect();

        assert_eq!(alpha_deps.len(), 4);
        assert_eq!(alpha_deps[0].id, "ALPHA_ID");
        assert_eq!(alpha_deps[1].name, "alpha.html");
        assert_eq!(alpha_deps[1].id, "BETA_ID");
        assert_eq!(alpha_deps[2].id, "DELTA_ID");
        assert_eq!(alpha_deps[3].id, "GAMMA_ID");

        let beta_deps: Vec<Template> = conn.prepare_reader(query, Some((1, rows[1].path.as_str())))
            .unwrap()
            .map(Result::unwrap)
            .collect();

        assert_eq!(beta_deps.len(), 3);
        assert_eq!(beta_deps[0].id, "BETA_ID");
        assert_eq!(beta_deps[0].name, "beta.html");
        assert_eq!(beta_deps[1].id, "DELTA_ID");
        assert_eq!(beta_deps[2].id, "GAMMA_ID");

        let gamma_deps: Vec<Template> = conn.prepare_reader(query, Some((1, rows[2].path.as_str())))
            .unwrap()
            .map(Result::unwrap)
            .collect();

        assert_eq!(gamma_deps.len(), 1);
        assert_eq!(gamma_deps[0].id, "GAMMA_ID");
        assert_eq!(gamma_deps[0].name, "gamma.html");

        let delta_deps: Vec<Template> = conn.prepare_reader(query, Some((1, rows[3].path.as_str())))
            .unwrap()
            .map(Result::unwrap)
            .collect();

        assert_eq!(delta_deps.len(), 1);
        assert_eq!(delta_deps[0].id, "DELTA_ID");
        assert_eq!(delta_deps[0].name, "delta.html");
    }
}