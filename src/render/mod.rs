// mod expand;
mod rewrite;
mod stylesheet;
mod template;

use std::sync::Arc;

use color_eyre::eyre::Context;
use minijinja::Environment;
use rayon::prelude::*;
use rusqlite::{params, Connection};
use serde_rusqlite::from_rows;

use self::template::Ticket;
use crate::{
    db::{
        self,
        data::{Dependency, Page},
        DbPool, PooledConnection,
    },
    prelude::*, render::rewrite::rewrite,
};

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Message {
    Ticket(Arc<Ticket>),
    Dependency(String, Dependency),
}

impl From<(String, Dependency)> for Message {
    fn from(source: (String, Dependency)) -> Self {
        Self::Dependency(source.0, source.1)
    }
}

#[derive(Debug)]
pub struct Bridge {
    pub pool: DbPool,
    pub rev_id: String,
    pub consumer: Consumer<Message>
}

impl Bridge {
    pub fn build(rev_id: &str) -> Arc<Self> {
        let pool = db::make_pool_sync().expect("Could not initialize pool.");
        let consumer_conn = pool.get().expect("Could not retrieve consumer connection.");
        let consumer = make_consumer(consumer_conn, rev_id);
        Arc::new(Self {
            pool,
            rev_id: rev_id.to_owned(),
            consumer
        })
    }

    pub fn send(&self, msg: impl Into<Message>) {
        self.consumer.send(msg)
    }
}

#[derive(Debug)]
pub struct Engine {
    pub bridge: Arc<Bridge>,
    pub environment: Environment<'static>,
}

impl Engine {
    pub fn build(conn: &mut Connection, rev_id: &str) -> Result<Arc<Self>> {
        let bridge = Bridge::build(rev_id);
        let environment = template::make_environment(conn, &bridge)?;
        let arc = Arc::new(Self {
            bridge,
            environment
        });

        Ok(arc)
    }

    pub fn eval_page_template(&self, ticket: &Arc<Ticket>) -> Result<String> {
        let name = match &ticket.page.template {
            Some(name) => name,
            None => template::FTL_BUILTIN_NAME
        };

        let Ok(template) = self.environment.get_template(name) else {
            let error = eyre!(
                "Tried to resolve a nonexistent template (\"{}\").",
                name,
            )
            .note("This error occurred because a page had a template specified in its frontmatter that FTL couldn't find at build time.")
            .suggestion("Double check the page's frontmatter for spelling and path mistakes, and make sure the template is where you think it is.");
            bail!(error)
        };

        let page = minijinja::value::Value::from_object(Arc::clone(ticket));

        let buffer = template.render(minijinja::context!(page => page))
            .wrap_err("Minijinja encountered an error when rendering a template.")?;

        Ok(buffer)
    } 
}

/// Executes the render pipeline for the provided revision, inserting the results into the database.
/// Page rendering happens in two distinct stages: template evaluation and rewriting.
///
/// During template evaluation, each page is evaluated against its specified [`minijinja`] template
/// (or the basic FTL-provided template, if one wasn't specified.) This is where the majority of rendering takes place;
/// FTL exposes a `render` filter within the engine that does several passes over the page's source, performing operations
/// like syntax highlighting before feeding it into [`pulldown_cmark`] to get the final output value.
/// 
/// Rewriting takes place after template evaluation; the [`lol_html`] crate is used to post-process the generated
/// HTML, taking care of tasks like redirecting external links to new tabs and setting media to load lazily.
/// 
/// Once rendering is complete, the final output is dispatched to a [`Consumer`] for writing to the database.
/// 
/// Stylesheet compilation is performed once page rendering is complete.
pub fn render(conn: &mut Connection, rev_id: &str) -> Result<()> {
    info!("Starting render stage...");

    let engine = Engine::build(conn, rev_id)?;

    query_tickets(conn, rev_id)?
        .into_par_iter()
        .map(Arc::new)
        .map(|ticket| -> Result<Arc<Ticket>> {
            let rendered = engine.eval_page_template(&ticket)?; 

            *ticket
                .source
                .write()
                .unwrap() = rendered;

            Ok(ticket)
        })
        .map(|ticket| -> Result<Arc<Ticket>> {
            let ticket = ticket?;
            let rewritten = rewrite(&ticket, &engine)?;
            *ticket
                .source
                .write()
                .unwrap() = rewritten;
            
            Ok(ticket)
        })
        .try_for_each(|ticket| -> Result<()> {
            let ticket = Message::Ticket(ticket?);
            engine.bridge.send(ticket);
            Ok(())
        })?;
    
    let bridge = Arc::clone(&engine.bridge);
    drop(engine);

    Arc::try_unwrap(bridge)
        .expect("Lingering reference to bridge Arc!")
        .consumer
        .finalize()?;

    stylesheet::compile_stylesheet(conn, rev_id)?;
    
    info!("Render stage complete.");
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

        let mut insert_hypertext = conn.prepare(
            "
            INSERT OR IGNORE INTO output
            VALUES (?1, ?2, 1, ?3)
        ",
        )?;
        let mut insert_dep = Dependency::prepare_insert(&conn)?;

        while let Ok(message) = stream.recv() {
            match message {
                Message::Ticket(ticket) => {
                    let output = ticket.source.read().unwrap();
                    debug!("Hypertext: {}", output);
                    insert_hypertext.execute(params![ticket.page.id, rev_id, &*output])?;
                }
                Message::Dependency(page_id, dependency) => {
                    debug!("Dependency: {page_id} : {dependency:?}");
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
