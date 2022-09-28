mod pulldown;
mod template;

use anyhow::{anyhow, Result};
use rusqlite::params;
use serde_rusqlite::from_rows;

use crate::db::{*, data::Page};

struct RenderEngine {
    // Page iterator
    // Templating Engine
    // CMark maps
    // Other fun stuff
}

pub fn prepare(conn: &mut Connection, rev_id: &str) -> Result<()> {
    let engine = template::make_engine_instance(conn, rev_id).unwrap();

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

    from_rows::<Page>(get_pages.query(params![rev_id])?)
        .for_each(|x| println!("{:?}", x));

    Ok(())
}

pub fn render(conn: &Connection, rev_id: &str) {
    // Evaluate each page ID with the render template, which:
    //   - Queries the database for the page's markup
    //   - Parses it for shortcodes and evaluates them
    //   - Plug shortcode evaluation output into pulldown-cmark
    //   - Feed cmark event stream into 0 or more enabled maps (such as code highlighting)
    //   - Write cmark to HTML, and plug that into the page's template (if any)
    //   - Post process the HTML as needed (such as for cache busting)
    //   - Sink the final hypertext into a channel.
    // Drop the hypertext channel sender and iterate over the receiver, serially inserting the final hypertext into the database.
}