use anyhow::{Result, anyhow, Context};
use serde::Deserialize;
use rusqlite::params;
use serde_rusqlite::from_rows;
use tera::Tera;

use crate::db::*;

mod dependency;

#[derive(Deserialize, Debug)]
pub struct Row {
    pub id: String,
    pub path: String,
    pub contents: String,
}

pub struct TemplatingEngine {
    pub tera: Tera,
}

pub fn make_engine_instance(conn: &mut Connection, rev_id: &str) -> Result<TemplatingEngine> {
    let mut tera = Tera::default();
    
    register_filters(&mut tera);
    register_functions(&mut tera);
    register_tests(&mut tera);

    Ok(parse_templates(conn, rev_id, tera)?)
}

fn register_filters(tera: &mut Tera) {
    _ = tera;
}

fn register_functions(tera: &mut Tera) {
    _ = tera;
}

fn register_tests(tera: &mut Tera) {
    _ = tera;
}

fn parse_templates(conn: &mut Connection, rev_id: &str, mut tera: Tera) -> Result<TemplatingEngine> {
    let rows = query_templates(conn, rev_id)?;
    // Collect row path/contents into a Vec of references.
    // This is necessary because Tera needs to ingest every template at once to allow for dependency resolution.
    let templates: Vec<(&str, &str)> = rows.iter()
        .map(|x| (x.path.as_str().trim_start_matches(crate::prepare::SITE_SRC_DIRECTORY), x.contents.as_str()) )
        .collect();
    
    if let Err(e) = tera.add_raw_templates(templates) { return Err(anyhow!(e)); }

    dependency::compute_ids(&rows, conn, rev_id)
        .context("Failed to compute template dependency IDs.")?;
    
    Ok(TemplatingEngine { tera })
}

fn query_templates(conn: &Connection, rev_id: &str) -> Result<Vec<Row>> {
    let mut stmt = conn.prepare("
        SELECT id, path, contents
        FROM input_files
        WHERE EXISTS (
                SELECT 1
                FROM revision_files
                WHERE revision_files.id = input_files.id
                AND revision_files.revision = ?1
        )
        AND input_files.extension = 'tera'
        AND input_files.contents NOT NULL;
    ")?;

    let result = from_rows::<Row>(stmt.query(params![&rev_id])?);
    let mut rows  = Vec::new();
    for row in result {
        rows.push(row?);
    }

    log::trace!("Query for templates complete, found {} entries.", rows.len());

    Ok(rows)
}