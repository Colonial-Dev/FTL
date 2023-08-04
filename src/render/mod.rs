// We need:
// A high-level Renderer struct that wraps all the assorted crap needed to render a revision
// A module for objects used in the templating engine
//   - "Resource" enum?
// A module for functions/filters/etc used in the templating engine
// A module for setting up the templating engine environment (possibly super of the previous two?)
// A module for post-render rewriting
// A module for stylesheet compilation

mod prepare;
mod stylesheet;
mod template;

use std::sync::Arc;

use crossbeam::channel::Receiver;
use itertools::Itertools;
use minijinja::Environment;
use rayon::prelude::*;
use template::{Metadata, Ticket};

use crate::db::{
    Connection, Dependency, Output, OutputKind, Page, Queryable, DEFAULT_QUERY, NO_PARAMS,
};
use crate::poll;
use crate::prelude::*;

#[derive(Debug)]
pub struct Renderer {
    pub env: Environment<'static>,
    pub state: Context,
    pub rev_id: RevisionID
}

impl Renderer {
    pub fn new(ctx: &Context) -> Result<Self> {
        let rev_id = prepare::prepare(ctx)?;

        let env = template::setup_environment(ctx, &rev_id)?;
        let state = Arc::clone(ctx);

        Ok(Self { 
            env,
            state,
            rev_id: rev_id.clone() 
        })
    }

    pub fn render_revision(&self) -> Result<()> {
        info!("Starting render for revision {}...", self.rev_id);

        let conn = self.state.db.get_rw()?;
        let tickets = self.get_tickets(&conn)?;
        let (handle, tx) = conn.prepare_consumer(consumer_handler);

        tickets
            .into_par_iter()
            .try_for_each(|ticket| -> Result<_> {
                tx.send(ticket.build(&self.env)?)?;
                Ok(())
            })?;

        drop(tx);

        handle
            .join()
            .expect("Database consumer thread should not panic.")?;

        stylesheet::compile(&self.state, &self.rev_id)?;

        Ok(())
    }

    fn get_tickets(&self, conn: &Connection) -> Result<Vec<Ticket>> {
        let page_query = "
            SELECT pages.* FROM pages
            JOIN revision_files ON revision_files.id = pages.id
            WHERE revision_files.revision = ?1
            AND NOT EXISTS (
                SELECT 1 FROM output, dependencies
                WHERE output.id = pages.id
                OR dependencies.parent = pages.id
            )
            OR EXISTS (
                SELECT 1 FROM dependencies
                WHERE dependencies.parent = pages.id
                AND dependencies.child NOT IN (
                    SELECT id FROM revision_files
                    WHERE revision = ?1
                )
            )
        ";

        let source_query = "
            SELECT contents FROM input_files
            WHERE id = ?1
        ";

        let params = (1, self.rev_id.as_ref()).into();

        let mut source_query = conn.prepare(source_query)?;
        let mut get_source = move |id: &str| {
            use sqlite::State;
            source_query.reset()?;
            source_query.bind((1, id))?;
            match source_query.next()? {
                State::Row => String::read_query(&source_query),
                State::Done => bail!("Could not find source for page with ID {id}."),
            }
        };

        let tickets: Vec<_> = conn
            .prepare_reader(page_query, params)?
            .map_ok(|page: Page| -> Result<_> {
                let source = get_source(&page.id)?;
                Ok(Ticket::new(&self.state, page, &source))
            })
            .flatten()
            .try_collect()?;

        Ok(tickets)
    }
}

fn consumer_handler(conn: &Connection, rx: Receiver<Ticket>) -> Result<()> {
    let txn = conn.open_transaction()?;
    let mut insert_output = conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?;
    let mut insert_dep = conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?;

    let mut remove_deps = conn.prepare(
        "
        DELETE FROM dependencies
        WHERE parent = ?1
    ",
    )?;
    let mut remove_deps = move |id: &str| -> Result<_> {
        remove_deps.reset()?;
        remove_deps.bind((1, id))?;
        poll!(remove_deps);
        Ok(())
    };

    for ticket in rx {
        let id = ticket.page.id.to_owned();
        remove_deps(&id)?;

        for md in ticket.metadata.into_iter() {
            match md {
                Metadata::Rendered(output) => {
                    println!("{output}");
                    insert_output(&Output {
                        id: id.clone().into(),
                        kind: OutputKind::Page,
                        content: output,
                    })?
                }
                Metadata::Dependency { relation, child } => insert_dep(&Dependency {
                    relation,
                    parent: id.clone(),
                    child,
                })?,
            }
        }
    }

    txn.commit()?;
    Ok(())
}
