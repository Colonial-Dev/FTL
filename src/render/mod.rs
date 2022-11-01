mod expand;
mod generate;
mod rewrite;
mod stylesheet;
mod template;

use color_eyre::eyre::Context;
use rayon::prelude::*;
use rusqlite::params;
use serde_rusqlite::from_rows;
use tera::Tera;

use crate::{
    db::{
        data::{Dependency, Page},
        *,
    },
    prelude::*,
};

pub struct RenderTicket {
    pub page: Page,
    pub content: String,
    pub context: tera::Context,
    pub dependencies: Vec<Dependency>,
}

impl RenderTicket {
    pub fn new(page: Page, mut source: String) -> Self {
        source.drain(..(page.offset as usize)).for_each(drop);

        let mut context = tera::Context::new();
        context.insert("page", &page);
        context.insert("config", Config::global());

        RenderTicket {
            content: source,
            page,
            context,
            dependencies: Vec::new(),
        }
    }
}

pub struct Engine<'a> {
    pub rev_id: &'a str,
    pub pool: DbPool,
    pub tera: Tera,
}

impl<'a> Engine<'a> {
    pub fn build(conn: &mut Connection, rev_id: &'a str) -> Result<Self> {
        let pool = make_pool()?;
        let tera = template::make_engine(conn, rev_id)?;

        let engine = Engine {
            rev_id,
            pool,
            tera,
        };

        Ok(engine)
    }
}

/// Executes the render pipeline for the provided revision, inserting the results into the database.
/// Rendering is composed of three distinct stages:
/// 1. Source expansion.
/// 2. Hypertext generation.
/// 3. Hypertext rewriting.
///
/// During *source expansion*, each page's Markdown source is parsed for certain structures like
/// code blocks, shortcodes and emoji tags. These are then evaluated accordingly, with the
/// result replacing the original structure in the text.
///
/// *Hypertext generation* is actually broken down into two sub-steps.
/// - First, the page's expanded Markdown source is rendered into full HTML.
/// (A few other syntax expansions, such as `@`-preceded internal links, are also handled here.)
/// - Second, the generated hypertext is evaluated against the page's specified template, if any.
///
/// Finally, *hypertext rewriting* consists of applying various transformations to a page's HTML, such as
/// cachebusting images or setting external links to open in a new tab.
pub fn render(conn: &mut Connection, rev_id: &str) -> Result<()> {
    info!("Starting render stage...");

    let engine = Engine::build(conn, rev_id)?;

    let mut consumer_conn = engine.pool.get()?;
    let consumer_rev_id = rev_id.to_owned();
    let consumer = Consumer::new_manual(move |stream: flume::Receiver<Result<RenderTicket>>| {
        let conn = consumer_conn.transaction()?;

        let mut insert_hypertext = conn.prepare("
            INSERT OR IGNORE INTO output
            VALUES (?1, ?2, 1, ?3)
        ")?;
        let mut sanitize = Dependency::prepare_sanitize(&conn)?;
        let mut insert_dep = Dependency::prepare_insert(&conn)?;

        while let Ok(ticket) = stream.recv() {
            let mut ticket = ticket?;

            if let Some(template) = ticket.page.template {
                ticket.dependencies.push(Dependency::Template(template))
            }
    
            sanitize(&ticket.page.id)?;
            for dependency in &ticket.dependencies {
                insert_dep(&ticket.page.id, dependency)?;
            }
            debug!("Hypertext: {}", ticket.content);
            insert_hypertext.execute(params![ticket.page.id, consumer_rev_id, ticket.content])?;
        }

        drop(insert_hypertext);
        drop(sanitize);
        drop(insert_dep);

        conn.commit()?;
        Ok(())
    });

    query_tickets(conn, rev_id)?
        .into_par_iter()
        .map(|mut ticket| -> Result<RenderTicket> {
            expand::expand(&mut ticket, &engine)?;
            generate::generate(&mut ticket, &engine)?;
            rewrite::rewrite(&mut ticket, &engine)?;
            Ok(ticket)
        })
        .for_each(|x| consumer.send(x));
    
    consumer.finalize()?;
    stylesheet::compile_stylesheet(conn, rev_id)?;

    Ok(())
}

/// Queries the database for all pages that need to be rendered for a revision and packages the results into a [`Vec<RenderTicket>`].'
///
/// N.B. a page will be rendered/re-rendered if any of the following criteria are met:
/// - The page is marked as dynamic in its frontmatter.
/// - The page's ID is not in the hypertext table (i.e. it's a new or changed page.)
/// - The page itself is unchanged, but one of its dependencies has.
fn query_tickets(conn: &Connection, rev_id: &str) -> Result<Vec<RenderTicket>> {
    let mut get_pages = conn.prepare(
        "
        WITH page_set AS (
            SELECT pages.* FROM pages
            WHERE EXISTS (
                    SELECT 1 FROM revision_files
                    WHERE revision_files.revision = ?1
                    AND revision_files.id = pages.id
                EXCEPT
                    SELECT 1 FROM output
                    WHERE output.id = pages.id
            )
        )

        SELECT DISTINCT * FROM page_set AS pages
        WHERE EXISTS (
            SELECT 1 FROM dependencies
            WHERE dependencies.page_id = pages.id
            AND dependencies.asset_id NOT IN (
                SELECT id FROM revision_files
                WHERE revision = ?1
            )
        )
        OR NOT EXISTS (
            SELECT 1 FROM dependencies
            WHERE dependencies.page_id = pages.id
        )
        OR pages.dynamic = 1;
    ",
    )?;

    let mut get_source_stmt = conn.prepare(
        "
        SELECT contents FROM input_files
        WHERE id = ?1
    ",
    )?;

    let pages: Result<Vec<RenderTicket>> = from_rows::<Page>(get_pages.query(params![rev_id])?)
        .map(|page| -> Result<RenderTicket> {
            let page = page?;

            let source: Result<Option<String>> =
                from_rows::<Option<String>>(get_source_stmt.query(params![page.id])?)
                    .map(|x| x.wrap_err("SQLite deserialization error!"))
                    .collect();

            let source = match source? {
                Some(source) => source,
                None => "".to_string(),
            };

            debug!(
                "Generated render ticket for page \"{}\" ({}).",
                page.title, page.id
            );
            let ticket = RenderTicket::new(page, source);

            Ok(ticket)
        })
        .collect();

    pages
}
