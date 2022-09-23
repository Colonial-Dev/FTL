use serde::Deserialize;
use rusqlite::params;
use serde_rusqlite::from_rows;

use crate::db::*;

#[derive(Deserialize, Debug)]
struct Row {
    pub path: String,
    pub contents: String,
}

pub fn parse_templates(conn: &Connection, rev_id: &str) -> Result<(), DbError> {

    Ok(())
}

fn query_templates(conn: &Connection, rev_id: &str) -> Result<Vec<Row>, DbError> {
    let mut stmt = conn.prepare("
        SELECT path, contents
        FROM input_files
        WHERE EXISTS (
                SELECT 1
                FROM revision_files
                WHERE revision_files.id = input_files.id
                AND revision_files.revision = ?1
            EXCEPT
                SELECT 1 
                FROM pages 
                WHERE pages.id = input_files.id
        )
        AND input_files.extension = 'liquid';
    ")?;

    let result = from_rows::<Row>(stmt.query(params![&rev_id])?);
    let mut rows  = Vec::new();
    for row in result {
        rows.push(row?);
    }

    log::trace!("Query for templates complete, found {} entries.", rows.len());

    Ok(rows)
}
