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

    pub fn into_context(&self) -> Result<tera::Context> {
        Ok(tera::Context::from_serialize(self)?)
    }
}

pub fn render<'a>(conn: &mut Connection, rev_id: &str) -> Result<()> {
    let tera = template::make_engine_instance(conn, rev_id).unwrap();
    let tickets = query_tickets(conn, rev_id)?;
    let (tx, rx) = flume::unbounded();

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    

    tickets.into_par_iter()
        .for_each(|ticket| {
            let expanded = expand_shortcodes(ticket.offset_source(), &ticket.page, &tera);

            let parser = init_pulldown(&expanded);
            let parser = map_pulldown(parser);

            let hypertext = write_pulldown(parser);
            let hypertext = evaluate_templating(hypertext, &ticket.page, &tera);
            let hypertext = rewrite_hypertext(hypertext);

            // Justification: neither end of the channel pair is dropped until after this iterator is evaluated.
            #[allow(unused_must_use)]
            tx.send(hypertext);
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

/// Parses the given Markdown input for shortcodes and attempts to evaluate them against the provided Tera instance.
/// The resulting "expanded source" will be returned as a [`Cow<'a, str>`].
fn expand_shortcodes<'a>(input: &'a str, page: &Page, tera: &Tera) -> Cow<'a, str> {
    todo!()
}

/// Initializes a [`Parser`] instance with the given Markdown input and all available extensions.
fn init_pulldown<'a>(input: &'a str) -> Parser<'a, 'a> {
    let options = Options::all();
    Parser::new_ext(input, options)
}

/// Maps a [`Parser`] instance over an arbitrary number of enabled parser maps. 
fn map_pulldown<'a>(parser: Parser) -> Parser<'a, 'a> {
    todo!()
}

/// Consume a [`Parser`] instance, buffering the HTML output into a final [`String`].
fn write_pulldown(parser: Parser) -> String {
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

/// If the given [Page] instance has a [`Some`] template value, attempt to evaluate it using the provided hypertext and Tera instance.
fn evaluate_templating(hypertext: String, page: &Page, tera: &Tera) -> String {
    todo!()
}

fn rewrite_hypertext(hypertext: String) -> String {
    todo!()
}