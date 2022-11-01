use rusqlite::params;
use serde::Deserialize;
use serde_rusqlite::from_rows;
use tera::Tera;

mod dependency;

use super::{Engine, RenderTicket};
use crate::{db::*, prelude::*};

static TEMPLATE_PRELUDE: &str = include_str!("ftl.tera");

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

    parse_templates(conn, rev_id, tera)
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
    let mut rows = query_templates(conn, rev_id)?;
    // Collect row path/contents into a Vec of references.
    // This is necessary because Tera needs to ingest every template at once to allow for dependency resolution.
    let mut templates: Vec<(&str, &str)> = rows
        .iter_mut()
        .map(|x| {
            // Every template implicitly imports a special FTL prelude.
            // The prelude includes various macros for tasks like cachebusting.
            let prelude = String::from("{% import \"ftl\" as ftl %}\n");
            x.contents = prelude + &x.contents;
            x
        })
        .map(|x| {
            (
                x.path
                    .as_str()
                    .trim_start_matches(SITE_SRC_DIRECTORY)
                    .trim_start_matches(SITE_TEMPLATE_DIRECTORY)
                    .trim_end_matches(".tera"),
                x.contents.as_str(),
            )
        })
        .collect();
    
    // Add the prelude to the template set.
    templates.push(("ftl", TEMPLATE_PRELUDE));

    tera.add_raw_templates(templates)?;

    dependency::compute_ids(&rows, conn, rev_id)
        .wrap_err("Failed to compute template dependency IDs.")?;

    Ok(tera)
}

/// Queries the database for the `id`, `path` and `contents` tables of all `.tera` files in the provided revision,
/// then packages the results into a [`Result<Vec<Row>>`].
fn query_templates(conn: &Connection, rev_id: &str) -> Result<Vec<Row>> {
    let mut stmt = conn.prepare(
        "
        SELECT input_files.id, path, contents FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.extension = 'tera'
        AND input_files.contents NOT NULL;
    ",
    )?;

    let rows: Result<Vec<Row>> = from_rows::<Row>(stmt.query(params![&rev_id])?)
        .map(|x| x.wrap_err("SQLite deserialization error!"))
        .collect();

    let rows = rows?;

    debug!(
        "Query for templates complete, found {} entries.",
        rows.len()
    );

    Ok(rows)
}

/// Uses the provided [`Tera`] instance to evaluate a page's template, if one was specified.
pub fn templates(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    if let Some(template) = &ticket.page.template {
        if engine
            .tera
            .get_template_names()
            .any(|name| name == template)
        {
            ticket.context.insert("markup", &ticket.content);
            ticket.content = engine.tera.render(template, &ticket.context)?;
        } else {
            let error = eyre!(
                "Page {} has a template specified ({}), but it can't be resolved.",
                ticket.page.title,
                template
            )
            .note("This error occurred because a page had a template specified in its frontmatter that FTL couldn't find at build time.")
            .suggestion("Double check the page's frontmatter for spelling and path mistakes, and make sure the template is where you think it is.");
            bail!(error)
        }
    }
    Ok(())
}
