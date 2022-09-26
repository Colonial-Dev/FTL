use serde::Deserialize;
use rusqlite::params;
use serde_rusqlite::from_rows;
use tera::Tera;

use crate::db::*;

mod dependency;

#[derive(Deserialize, Debug)]
struct Row {
    pub id: String,
    pub path: String,
    pub contents: String,
}

pub fn make_engine_instance(conn: &Connection, rev_id: &str) -> Result<Tera, DbError> {
    let tera = Tera::default();
    
    register_filters(&tera);
    register_functions(&tera);
    register_tests(&tera);
    parse_templates(conn, rev_id, &tera)?;

    Ok(tera)
}

fn register_filters(tera: &Tera) {
    _ = tera;
}

fn register_functions(tera: &Tera) {
    _ = tera;
}

fn register_tests(tera: &Tera) {
    _ = tera;
}

fn parse_templates(conn: &Connection, rev_id: &str, tera: &Tera) -> Result<(), DbError> {
    Ok(())
}

fn query_templates(conn: &Connection, rev_id: &str) -> Result<Vec<Row>, DbError> {
    let mut stmt = conn.prepare("
        SELECT id, path, contents
        FROM input_files
        WHERE EXISTS (
                SELECT 1
                FROM revision_files
                WHERE revision_files.id = input_files.id
                AND revision_files.revision = ?1
        )
        AND input_files.extension = 'tera';
    ")?;

    let result = from_rows::<Row>(stmt.query(params![&rev_id])?);
    let mut rows  = Vec::new();
    for row in result {
        rows.push(row?);
    }

    log::trace!("Query for templates complete, found {} entries.", rows.len());

    Ok(rows)
}