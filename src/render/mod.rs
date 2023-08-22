mod prepare;
mod stylesheet;
mod template;

use crossbeam::channel::Receiver;
use itertools::Itertools;
use minijinja::Environment;
use rayon::prelude::*;
use template::Ticket;

use crate::db::*;
use crate::poll;
use crate::prelude::*;

pub use prepare::prepare;

#[derive(Debug)]
pub struct Renderer {
    pub env: Environment<'static>,
    pub ctx: Context,
    pub rev_id: RevisionID,
}

impl Renderer {
    pub fn new(ctx: &Context, rev_id: &RevisionID) -> Result<Self> {
        Ok(Self {
            env: template::setup_environment(ctx, rev_id)?,
            ctx: ctx.clone(),
            rev_id: rev_id.clone(),
        })
    }

    pub fn render_revision(&self) -> Result<()> {
        info!("Starting render for revision {}...", self.rev_id);

        let conn = self.ctx.db.get_rw()?;
        let tickets = self.get_tickets(&conn)?;
        let (handle, tx) = conn.prepare_consumer(consumer_handler);

        tickets
            .into_par_iter()
            .try_for_each(|ticket| -> Result<_> {
                let rendered = ticket.build(&self.env)?;
                
                tx.send((
                    ticket,
                    rendered
                ))?;

                Ok(())
            })?;

        drop(tx);

        handle
            .join()
            .expect("Database consumer thread should not panic.")?;

        stylesheet::compile(&self.ctx, &self.rev_id)?;

        info!("Finished rendering revison {}.", self.rev_id);
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
                Ok(Ticket::new(&self.ctx, page, &source))
            })
            .flatten()
            .try_collect()?;

        Ok(tickets)
    }
}

fn consumer_handler(conn: &Connection, rx: Receiver<(Ticket, String)>) -> Result<()> {
    let txn = conn.open_transaction()?;
    let mut insert_output = conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?;
    let mut insert_dep = conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?;

    let mut remove_deps = conn.prepare("
        DELETE FROM dependencies
        WHERE parent = ?1
    ")?;

    let mut remove_deps = move |id: &str| -> Result<_> {
        remove_deps.reset()?;
        remove_deps.bind((1, id))?;
        poll!(remove_deps);
        Ok(())
    };

    for (ticket, output) in rx {
        let id = ticket.page.id;
        
        remove_deps(&id)?;

        for (relation, child) in ticket.dependencies.into_iter() {
            insert_dep(&Dependency {
                relation,
                parent: id.clone(),
                child,
            })?
        }

        println!("{output}");
        insert_output(&Output {
            id: Some(id),
            kind: OutputKind::Page,
            content: output
        })?;
    }

    txn.commit()?;
    Ok(())
}
