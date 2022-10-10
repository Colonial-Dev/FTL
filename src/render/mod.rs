mod pulldown;
mod rewrite;
mod stylesheet;
mod template;

use std::borrow::Cow;

use rayon::prelude::*;
use rusqlite::params;
use serde_rusqlite::from_rows;
use tera::{Context, Tera};

use crate::{db::{*, data::Page, self}, share::ERROR_CHANNEL};
use crate::prelude::*;

#[derive(Debug)]
pub struct RenderTicket<'a> {
    pub page: Page,
    pub content: Cow<'a, str>,
    pub context: Context
}

impl<'a> RenderTicket<'a> {
    pub fn new(page: Page, mut source: String) -> Self {
        source
            .drain((page.offset as usize)..)
            .for_each(drop);

        let mut context = Context::new();
        context.insert("page", &page);
        context.insert("config", Config::global());

        RenderTicket {
            content: Cow::Owned(source),
            page,
            context
        }
    }
}

pub struct Engine<'a> {
    pub rev_id: &'a str,
    pub pool: DbPool,
    pub tera: Tera,
    pub sink: flume::Sender<Result<RenderTicket<'a>>>,
}

impl<'a> Engine<'a> {
    pub fn build(conn: &mut Connection, rev_id: &'a str) -> Result<(Self, flume::Receiver<Result<RenderTicket<'a>>>)> {
        let pool = db::make_pool()?;
        let tera = template::make_engine(conn, rev_id)?;
        let (sink, stream) = flume::unbounded();

        let engine = Engine {
            rev_id,
            pool,
            tera,
            sink,
        };

        Ok((engine, stream))
    }
}

/// Executes the render pipeline for the provided revision, inserting the results into the database.
pub fn render<'a>(conn: &mut Connection, rev_id: &str) -> Result<()> {
    info!("Starting render stage...");

    let (engine, stream) = Engine::build(conn, rev_id)?;
    let tickets = query_tickets(conn, rev_id)?;

    tickets
        .into_par_iter()
        .map(|mut ticket| -> Result<RenderTicket<'a>> {
            template::shortcodes(&mut ticket, &engine)?;
            pulldown::process(&mut ticket, &engine);
            template::templates(&mut ticket, &engine)?;
            rewrite::rewrite(&mut ticket, &engine)?;
            Ok(ticket)
        })
        .for_each(|x| {
            engine.sink.send(x).expect("Rendering output sink closed unexpectedly!");
        });

    drop(engine);

    let mut stmt = conn.prepare("
        INSERT OR IGNORE INTO hypertext 
        VALUES (?1, ?2, 
            (SELECT template_id FROM dependencies WHERE kind = 1 AND template_name = ?3),
            ?4
        )
    ")?;
    
    for ticket in stream.into_iter() {
        let ticket = ticket?;
        stmt.execute(params![rev_id, ticket.page.id, ticket.page.template, ticket.content])?;
    }

    stylesheet::compile_stylesheet(conn, rev_id)?;
    
    Ok(())
}

/// Queries the database for all pages that need to be rendered for a revision and packages the results into a [`Vec<RenderTicket>`].'
/// 
/// N.B. a page will be rendered/re-rendered if any of the following criteria are met:
/// - The page is marked as dynamic in its frontmatter.
/// - The page's ID is not in the hypertext table (i.e. it's a new or changed page.)
/// - The page itself is unchanged, but one of the templates/shortcodes it relies upon has changed (expressed via a templating ID, see [`template::dependency::compute_ids`].)
/// - The page itself is unchanged, but one of the cachebusted assets it relies upon has changed (captured during cachebusting, see [`rewrite::prepare_cachebust`].)
fn query_tickets<'a>(conn: &Connection, rev_id: &str) -> Result<Vec<RenderTicket<'a>>> {
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
                FROM dependencies, hypertext
                WHERE hypertext.input_id = pages.id
                AND hypertext.templating_id NOT IN (
                    SELECT template_id FROM dependencies
                )
            )
            OR EXISTS (
                SELECT 1
                FROM dependencies, hypertext
                WHERE hypertext.input_id = pages.id
                AND dependencies.kind = 2
                AND dependencies.page_id = pages.id
                AND dependencies.asset_id NOT IN (
                    SELECT id FROM revision_files
                    WHERE revision = ?1
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
            
            debug!("Generated render ticket for page \"{}\" ({}).", x.title, x.id);
            let ticket = RenderTicket::new(x, source);

            Ok(ticket)
        })
        .filter_map(|x| ERROR_CHANNEL.filter_error(x))
        .collect();

    Ok(pages)
}