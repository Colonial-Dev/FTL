// mod expand;
mod generate;
mod rewrite;
mod stylesheet;

use std::sync::Arc;

use color_eyre::eyre::Context;
use rayon::prelude::*;
use rusqlite::params;
use serde_rusqlite::from_rows;
use minijinja::{Environment, Template};
use rusqlite::Connection;

use crate::{
    db::{
        data::{Dependency, Page},
        DbPool, PooledConnection,
        self
    },
    prelude::*,
};

use self::generate::{DatabaseBridge, Ticket};

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Message {
    Ticket(String, Arc<Ticket>),
    Dependency(String, Dependency),
}

impl From<(String, Dependency)> for Message {
    fn from(source: (String, Dependency)) -> Self {
        Self::Dependency(
            source.0, 
            source.1
        )
    }
}

#[derive(Debug)]
pub struct Engine<'a> {
    pub bridge: Arc<DatabaseBridge>,
    pub environment: Environment<'a>
}

impl<'a> Engine<'a> {
    pub fn build(conn: &mut Connection, rev_id: &'a str) -> Result<Self> {
        let pool = db::make_pool()?;
        let consumer = make_consumer(pool.get()?, rev_id);
        let bridge = DatabaseBridge::build(rev_id, consumer)?;
        let environment = generate::make_environment(conn, &bridge)?;

        let engine = Engine {
            bridge,
            environment
        };

        Ok(engine)
    }

    pub fn get_template(&self, name: &str) -> Option<Template> {
        // get_template is weird in that it returns a Result rather than an Option
        // to represent whether a template was found - so it's safe to erase the 
        // Err match here.
        match self.environment.get_template(name) {
            Ok(template) => Some(template),
            Err(_) => None
        }
    }

    pub fn send(&self, msg: impl Into<Message>) {
        self.bridge.consumer.send(msg)
    }

    pub fn finalize(self) -> Result<()> {
        Ok(())
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
    
    // TODO:
    // - Move file inclusion into MiniJinja via shortcodes.
    // - Figure out good SQLite connection pool size.
    // - Fold source expansion / markdown rendering into MiniJinja.

    query_tickets(conn, rev_id)?
        .into_par_iter()
        .map(|mut ticket| -> Result<(String, Arc<Ticket>)> {
            let ticket = Arc::new(ticket);
            // expand::expand(&mut ticket, &engine)?;
            // generate::generate(&mut ticket, &engine)?;
            // rewrite::rewrite(&mut ticket, &engine)?;

            let mut buffer = ticket.source.clone();
            let Some(name) = &ticket.page.template else {
                warn!(
                    "Tried to evaluate template for page {} (\"{}\"), but none was specified.",
                    ticket.page.id,
                    ticket.page.title
                );
        
                // This isn't *technically* an error, so we just silently yield.
                return Ok((buffer, ticket))
            };
        
            let Some(template) = engine.get_template(name) else {
                let error = eyre!(
                    "Tried to resolve a nonexistent template (\"{}\").",
                    name,
                )
                .note("This error occurred because a page had a template specified in its frontmatter that FTL couldn't find at build time.")
                .suggestion("Double check the page's frontmatter for spelling and path mistakes, and make sure the template is where you think it is.");
                bail!(error)
            };
            
            let page = minijinja::value::Value::from_object(Arc::clone(&ticket));
            buffer = template.render(minijinja::context!(page => page))
                .wrap_err("Minijinja encountered an error when rendering a template.")?;

            Ok((buffer, ticket))
        })
        .try_for_each(|ticket| -> Result<()> {
            let ticket = ticket?;
            let ticket = Message::Ticket(ticket.0, ticket.1);
            engine.send(ticket);
            Ok(())
        })?;
    
    engine.finalize()?;
    stylesheet::compile_stylesheet(conn, rev_id)?;

    Ok(())
}

/// Queries the database for all pages that need to be rendered for a revision and packages the results into a [`Vec<RenderTicket>`].'
///
/// N.B. a page will be rendered/re-rendered if any of the following criteria are met:
/// - The page is marked as dynamic in its frontmatter.
/// - The page's ID is not in the hypertext table (i.e. it's a new or changed page.)
/// - The page itself is unchanged, but one of its dependencies has.
fn query_tickets(conn: &Connection, rev_id: &str) -> Result<Vec<Ticket>> {
    let mut get_pages = conn.prepare(
        "
        WITH page_set AS (
            SELECT pages.* FROM pages
            WHERE EXISTS (
                    SELECT 1 FROM revision_files
                    WHERE revision_files.revision = ?1
                    AND revision_files.id = pages.id
            )
        )

        SELECT DISTINCT * FROM page_set AS pages
        WHERE NOT EXISTS (
            SELECT 1 FROM output
            WHERE output.id = pages.id
        )
        OR NOT EXISTS (
            SELECT 1 FROM dependencies
            WHERE dependencies.page_id = pages.id
        )
        OR EXISTS (
            SELECT 1 FROM dependencies
            WHERE dependencies.page_id = pages.id
            AND dependencies.asset_id NOT IN (
                SELECT id FROM revision_files
                WHERE revision = ?1
            )
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

    let mut sanitize = Dependency::prepare_sanitize(conn)?;

    let pages: Result<Vec<Ticket>> = from_rows::<Page>(get_pages.query(params![rev_id])?)
        .map(|page| -> Result<Ticket> {
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

            sanitize(&page.id)?;

            let ticket = Ticket::new(page, source);
            Ok(ticket)
        })
        .collect();
    
    pages
}

fn make_consumer(mut conn: PooledConnection, rev_id: &str) -> Consumer<Message> {
    let rev_id = rev_id.to_owned();

    Consumer::new_manual(move |stream: flume::Receiver<Message>| {
        let conn = conn.transaction()?;

        let mut insert_hypertext = conn.prepare("
            INSERT OR IGNORE INTO output
            VALUES (?1, ?2, 1, ?3)
        ")?;
        let mut insert_dep = Dependency::prepare_insert(&conn)?;

        while let Ok(message) = stream.recv() {
            match message {
                Message::Ticket(output, ticket) => {
                    debug!("Hypertext: {}", output);
                    insert_hypertext.execute(params![ticket.page.id, rev_id, output])?;
                }
                Message::Dependency(page_id, dependency) => {
                    insert_dep(&page_id, &dependency)?;
                }
            }
        }

        drop(insert_hypertext);
        drop(insert_dep);
        conn.commit()?;

        Ok(())
    })
}
