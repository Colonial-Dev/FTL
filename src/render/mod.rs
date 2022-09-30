mod pulldown;
mod template;

use std::borrow::Cow;

use anyhow::{anyhow, Result};
use pulldown_cmark::{Parser, Options, html};
use rayon::prelude::*;
use rusqlite::params;
use serde::Serialize;
use serde_rusqlite::from_rows;
use tera::Tera;

use crate::{db::{*, data::Page}, share::ERROR_CHANNEL};

#[derive(Serialize, Debug)]
struct RenderTicket {
    pub page: Page,
    pub source: String,
}

impl RenderTicket {
    pub fn new(page: Page, source: String) -> Self {
        RenderTicket { page, source }
    }

    pub fn offset_source(&self) -> &str {
        &self.source[(self.page.offset as usize)..]
    }
}

pub fn render<'a>(conn: &mut Connection, rev_id: &str) -> Result<()> {
    let tera = template::make_engine(conn, rev_id).unwrap();
    let tickets = query_tickets(conn, rev_id)?;
    let (tx, rx) = flume::unbounded();

    tickets.into_par_iter()
        .for_each(|ticket| {
            let source = Cow::Borrowed(ticket.offset_source());
            let source = template::shortcodes(source, &tera);
            let parser = pulldown::init(&source);
            let parser = pulldown::map(parser);

            let hypertext = pulldown::write(parser);
            let hypertext = template::templates(hypertext, &ticket.page, &tera).unwrap();
            let hypertext = rewrite_hypertext(hypertext);
            println!("{hypertext}");
            drop(tx.send(hypertext));
        });

    drop(tx);

    Ok(())
}

/// Queries the database for all pages that need to be rendered for a revision and packages the results into a [`Vec<RenderTicket>`].'
/// 
/// N.B. a page will be rendered/re-rendered if any of the following criteria are met:
/// - The page is marked as dynamic in its frontmatter.
/// - The page's ID is not in the hypertext table (i.e. it's a new or changed page.)
/// - The page itself is unchanged, but one of the templates/shortcodes it relies upon has changed (expressed via a templating ID, see [`template::dependency::compute_ids`].)
fn query_tickets<'a>(conn: &Connection, rev_id: &str) -> Result<Vec<RenderTicket>> {
    let mut get_pages = conn.prepare("
        SELECT DISTINCT pages.* FROM pages, revision_files WHERE
        revision_files.revision = ?1
        AND pages.id = revision_files.id
        AND (
            NOT EXISTS (
                SELECT 1
                FROM hypertext WHERE
                hypertext.input_id = pages.id
            )
            OR EXISTS (
                SELECT 1 
                FROM template_ids, hypertext
                WHERE hypertext.input_id = pages.id
                AND hypertext.templating_id NOT IN (
                    SELECT id FROM template_ids WHERE
                    template_ids.revision = ?1
                )
            )
        )
        OR pages.dynamic = 1;
    ")?;

    let mut get_source_stmt = conn.prepare("
        SELECT contents FROM input_files
        WHERE id = ?1
    ")?;

    let pages: Vec<RenderTicket> = from_rows::<Page>(get_pages.query(params![rev_id])?)
        .filter_map(|x| ERROR_CHANNEL.filter_error(x) )
        .map(|x| -> Result<RenderTicket> {
            let source = from_rows::<Option<String>>(get_source_stmt.query(params![x.id])?)
                .filter_map(|x| ERROR_CHANNEL.filter_error(x) )
                .filter_map(|x| x)
                .collect();
            
            log::trace!("Generated render ticket for page \"{}\" ({}).", x.title, x.id);
            let ticket = RenderTicket::new(x, source);

            Ok(ticket)
        })
        .filter_map(|x| ERROR_CHANNEL.filter_error(x))
        .collect();

    Ok(pages)
}

fn rewrite_hypertext<'a>(hypertext: Cow<'a, str>) -> Cow<'a, str> {
    hypertext
}