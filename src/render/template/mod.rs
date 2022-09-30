use std::borrow::Cow;

use anyhow::{Result, anyhow, Context};
use rusqlite::params;
use serde_rusqlite::from_rows;
use tera::{Tera};
use serde::{Serialize, Deserialize};

mod dependency;
mod shortcode;

pub use shortcode::evaluate_shortcodes as shortcodes;

use crate::db::{*, data::Page};

#[derive(Deserialize, Debug)]
pub struct Row {
    pub id: String,
    pub path: String,
    pub contents: String,
}

/// Create a standard [`Tera`] instance, and register all known filters, functions, tests and templates with it.
pub fn make_engine(conn: &mut Connection, rev_id: &str) -> Result<Tera> {
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

/// Query the database for all relevant templates (via [`query_templates`]), then:
/// 1. Add every template to the provided [`Tera`] instance.
/// 2. Use the [`dependency`] module to compute and cache templating IDs for the provided revision.
fn parse_templates(conn: &mut Connection, rev_id: &str, mut tera: Tera) -> Result<Tera> {
    let rows = query_templates(conn, rev_id)?;
    // Collect row path/contents into a Vec of references.
    // This is necessary because Tera needs to ingest every template at once to allow for dependency resolution.
    let templates: Vec<(&str, &str)> = rows.iter()
        .map(|x| (x.path.as_str().trim_start_matches(crate::prepare::SITE_SRC_DIRECTORY).trim_end_matches(".tera"), x.contents.as_str()) )
        .collect();
    
    if let Err(e) = tera.add_raw_templates(templates) { return Err(anyhow!(e)); }

    dependency::compute_ids(&rows, conn, rev_id)
        .context("Failed to compute template dependency IDs.")?;
    
    Ok(tera)
}

/// Queries the database for the `id`, `path` and `contents` tables of all `.tera` files in the provided revision,
/// then packages the results into a [`Result<Vec<Row>>`].
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

    let rows: Vec<Row> = from_rows::<Row>(stmt.query(params![&rev_id])?)
        .filter_map(|x| x.ok() )
        .collect();

    log::trace!("Query for templates complete, found {} entries.", rows.len());

    Ok(rows)
}

/// Uses the provided [`Tera`] instance to evaluate a page's template, if one was specified.
pub fn templates<'a>(input: Cow<'a, str>, page: &Page, tera: &Tera) -> Result<Cow<'a, str>> {
    #[derive(Serialize, Debug)]
    struct IntoContext<'a> {
        page: &'a Page
    }
    if let Some(template) = &page.template {
        if tera.get_template_names().any(|name| name == template) {
            let wrap = IntoContext { page };
            let mut ctx = tera::Context::from_serialize(wrap)?;
            ctx.insert("markup", &input);
            let out = tera.render(template, &ctx)?;
            Ok(Cow::Owned(out))
        }
        else { Ok(input) }
    }
    else { Ok(input) }
}