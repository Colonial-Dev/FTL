mod stylesheet;
mod template;

use crossbeam::channel::Receiver;
use itertools::Itertools;
use minijinja::Environment;
use rayon::prelude::*;
use template::Ticket;

use crate::db::*;
use crate::prelude::*;
use crate::prepare;

#[derive(Debug)]
pub struct Renderer {
    pub env: Environment<'static>,
    pub ctx: Context,
    pub rev_id: RevisionID,
}

impl Renderer {
    pub fn new(ctx: &Context, rev_id: Option<&RevisionID>) -> Result<Self> {
        let rev_id = prepare::prepare(ctx, rev_id)?;
        let env = template::setup_environment(ctx, &rev_id)?;
        let ctx = ctx.clone();
        
        let new = Self {
            env,
            ctx,
            rev_id,
        };

        new.render()?;

        Ok(new)
    }

    fn render(&self) -> Result<()> {
        info!("Starting render for revision {}...", self.rev_id);

        let progressor = Progressor::new(Message::Rendering);

        if self.ctx.build.compile_sass {
            stylesheet::compile(&self.ctx, &self.rev_id)?;
        }

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

        // TODO handle the possibility of multiple errors occurring during rendering.

        drop(tx);

        handle
            .join()
            .expect("Database consumer thread should not panic.")?;

        self.finalize_revision()?;

        info!("Finished rendering revison {}.", self.rev_id);
        progressor.finish();

        Message::BuildOK.print();

        Ok(())
    }

    fn get_tickets(&self, conn: &Connection) -> Result<Vec<Ticket>> {
        let mut get_pages = conn.prepare("
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
        ")?;

        let mut get_source = conn.prepare("
            SELECT contents FROM input_files
            WHERE id = ?1
        ")?;

        let tickets: Vec<_> = get_pages
            .query_and_then([self.rev_id.as_ref()], Page::from_row)?
            .filter_ok(|page| {
                if self.ctx.drafts_enabled() {
                    true
                } else {
                    !page.draft
                }
            })
            .map_ok(|page| -> Result<_> {
                let source = get_source
                    .query_row([&page.id], |row| row.get::<_, String>(0))?;

                Ok(Ticket::new(&self.ctx, &self.rev_id, page, &source))
            })
            .flatten()
            .try_collect()?;

        Ok(tickets)
    }

    fn finalize_revision(&self) -> Result<()> {
        let conn = self.ctx.db.get_rw()?;

        conn.prepare("
            UPDATE revisions
            SET time = datetime('now', 'localtime'),
                stable = TRUE
            WHERE revisions.id = ?1
        ")?
        .execute([self.rev_id.as_ref()])?;

        Ok(())
    }
}

fn consumer_handler(conn: &mut Connection, rx: Receiver<(Ticket, String)>) -> Result<()> {
    let txn = conn.transaction()?;

    let mut remove_deps = txn.prepare("
        DELETE FROM dependencies
        WHERE parent = ?1
    ")?;

    for (ticket, output) in rx {
        let id = ticket.page.id;
        
        remove_deps.execute([&id])?;

        for (relation, child) in ticket.dependencies.into_iter() {
            Dependency {
                relation,
                parent: id.clone(),
                child,
            }.insert(&txn)?;
        }

        debug!("{output}");
        
        Output {
            id: Some(id),
            kind: OutputKind::Page,
            content: output
        }.insert_or(&txn, OnConflict::Replace)?;
    }

    remove_deps.finalize()?;
    txn.commit()?;
    Ok(())
}
