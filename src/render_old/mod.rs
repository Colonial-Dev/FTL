mod rewrite;
mod stylesheet;
mod template;

use std::sync::{Arc, RwLock};

use minijinja::{Environment, value::{Value, Object}};
use rayon::prelude::*;
use rusqlite::{params, Connection};
use serde_aux::serde_introspection::serde_introspect;
use serde_rusqlite::from_rows;

use crate::{
    db::{
        self,
        data::{Dependency, Page, Route, RouteIn},
        DbPool, PooledConnection,
    },
    prelude::*, render::rewrite::rewrite,
};

/// Represents a "render ticket" - a discrete unit of rendering work that needs to be done.
/// 
/// Implements [`minijinja::value::Object`], which forwards in-engine interactions to the `inner` field
/// and allows hooked-in Rust functions to downcast from [`Value`] to access the `page` and `source` fields.
#[derive(Debug)]
pub struct Ticket {
    /// A serialized copy of the `page` field. In-engine interactions with Tickets
    /// are forwarded here.
    pub inner: Value,
    /// The original Page corresponding to this ticket. Not accessible in-engine.
    pub page: Page,
    /// The source text of the ticket, from the `input_files` table. Not accessible in-engine.
    /// Wrapped in a [`RwLock`] to allow in-place mutation as rendering progresses.
    pub source: RwLock<String>,
}

impl Ticket {
    pub fn new(page: Page, mut source: String) -> Self {
        // Drain away the page's frontmatter.
        source.drain(..(page.offset as usize)).for_each(drop);

        Ticket {
            inner: Value::from_serializable(&page),
            page,
            source: RwLock::new(source),
        }
    }
}

impl std::fmt::Display for Ticket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl Object for Ticket {
    fn get_attr(&self, name: &str) -> Option<Value> {
        // Forward requests for attributes to the `inner` field.
        self.inner.get_attr(name).ok()
    }

    fn attributes(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        // serde_introspect returns an iterator over the field names of
        // Page - less brittle than hardcoding a static array.
        Box::new(serde_introspect::<Page>().iter().copied())
    }
}

#[derive(Debug)]
pub enum Message {
    Ticket(Arc<Ticket>),
    Dependency(String, Dependency),
    Route(Route),
}

impl From<(String, Dependency)> for Message {
    fn from(source: (String, Dependency)) -> Self {
        Self::Dependency(source.0, source.1)
    }
}

impl From<Route> for Message {
    fn from(route: Route) -> Self {
        Self::Route(route)
    }
}

/// A shared bridge into the database. Contains a connection pool, the revision ID
/// and write consumer.
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

/// A rendering engine. Wraps a database bridge and templating environment.
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

    /// Attempt to evaluate the provided ticket's template. Uses the builtin template
    /// if the ticket's page does not specify one.
    pub fn eval_ticket_template(&self, ticket: &Arc<Ticket>) -> Result<String> {
        let name = match &ticket.page.template {
            Some(name) => name,
            None => template::FTL_BUILTIN
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

        template.render(minijinja::context!(page => page))
            .map_err(template::flatten_err)
    }

    /// Finalizes the engine; this drops the consumer and attempts to flush/commit the database bridge.
    /// Panics if more than one strong reference is held to the bridge [`Arc`].
    pub fn finalize(engine: Arc<Engine>) -> Result<()> {
        let bridge = Arc::clone(&engine.bridge);
        drop(engine);

        Arc::try_unwrap(bridge)
            .expect("Lingering reference to bridge Arc!")
            .consumer
            .finalize()
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
            let rendered = engine.eval_ticket_template(&ticket)?; 

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
    
    Engine::finalize(engine)?;
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
            SELECT 1 FROM output, dependencies
            WHERE output.id = pages.id
            OR dependencies.page_id = pages.id
        )
        OR EXISTS (
            SELECT 1 FROM dependencies
            WHERE dependencies.page_id = pages.id
            AND dependencies.asset_id NOT IN (
                SELECT id FROM revision_files
                WHERE revision = ?1
            )
        )
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
        let mut insert_route = Route::prepare_insert(&conn)?;

        while let Ok(message) = stream.recv() {
            match message {
                Message::Ticket(ticket) => {
                    let output = ticket.source.read().unwrap();
                    debug!("Consumer received hypertext output: {}", output);
                    insert_hypertext.execute(params![ticket.page.id, rev_id, &*output])?;
                }
                Message::Dependency(page_id, dependency) => {
                    debug!("Consumer received dependency: {page_id} : {dependency:?}");
                    insert_dep(&page_id, &dependency)?;
                }
                Message::Route(route) => {
                    debug!("Consumer received route: {route:?}");
                    insert_route(&RouteIn {
                        id: route.id.as_deref(),
                        revision: &route.revision,
                        route: &route.route,
                        parent_route: route.parent_route.as_deref(),
                        kind: route.kind
                    })?;
                }
            }
        }

        drop(insert_hypertext);
        drop(insert_dep);
        drop(insert_route);

        conn.commit()?;

        Ok(())
    })
}
