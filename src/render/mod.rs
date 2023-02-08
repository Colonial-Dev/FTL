// We need:
// A high-level Renderer struct that wraps all the assorted crap needed to render a revision
// A module for objects used in the templating engine
//   - "Resource" enum?
// A module for functions/filters/etc used in the templating engine
// A module for setting up the templating engine environment (possibly super of the previous two?)
// A module for post-render rewriting
// A module for stylesheet compilation

mod highlight;
mod prepare;
mod template;
mod stylesheet;

use std::sync::{Arc, Weak};

use itertools::Itertools;
use minijinja::Environment;

use template::{Ticket, WrappedReport as Wrap};
use crate::db::Page;
use crate::prelude::*;

#[derive(Debug)]
pub struct Renderer {
    pub env: Environment<'static>,
    pub loopback: Weak<Self>,
    pub state: State,
}

impl Renderer {
    pub fn new(state: &State) -> Result<Arc<Self>> {
        prepare::prepare(state)?;
        
        let env = template::setup_environment(state)?;
        let state = Arc::clone(state);

        Ok(Arc::new_cyclic(move |weak| Self {
            env,
            loopback: Weak::clone(weak),
            state,
        }))
    }

    pub fn render_revision(&self) -> Result<()> {
        info!("Starting render for revision {}...", self.state.get_rev());

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

        let conn = self.state.db.get_rw()?;
        let rev_id = self.state.get_rev();
        let params = (1, rev_id.as_str()).into();

        let mut source_query = conn.prepare(source_query)?;
        let mut get_source = move |id: &str| {
            use sqlite::State;
            source_query.reset()?;
            source_query.bind((1, id))?;
            match source_query.next()? {
                State::Row => {
                    source_query.read::<String, _>("contents")
                        .map_err(Report::from)
                },
                State::Done => {
                    bail!("Could not find source for page with id {id}.")
                }
            }
        };

        let tickets: Vec<_> = conn.prepare_reader(page_query, params)?
            .map_ok(|page: Page| -> Result<_> {
                let source = get_source(&page.id)?;
                Ok(Ticket::new(
                    &self.state,
                    page,
                    &source
                ))
            })
            .flatten()
            .map_ok(Arc::new)
            .try_collect()?;
        
        for ticket in tickets {
            let name = match &ticket.page.template {
                Some(name) => name,
                None => "ftl_default.html"
            };
    
            let Ok(template) = self.env.get_template(name) else {
                let error = eyre!(
                    "Tried to resolve a nonexistent template (\"{}\").",
                    name,
                )
                .note("This error occurred because a page had a template specified in its frontmatter that FTL couldn't find at build time.")
                .suggestion("Double check the page's frontmatter for spelling and path mistakes, and make sure the template is where you think it is.");
    
                bail!(error)
            };
    
            let page = minijinja::value::Value::from_object(Arc::clone(&ticket));

            let out = template.render(minijinja::context!(page => page))
                .map_err(Wrap::flatten)?;

            println!("{out}")
        }

        stylesheet::compile(&self.state)?;

        Ok(())
    }
}