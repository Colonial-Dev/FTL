mod pulldown;
mod template;

use crate::db::*;

struct RenderEngine {
    // Page iterator
    // Templating Engine
    // CMark maps
    // Other fun stuff
}

pub fn prepare(conn: &Connection, rev_id: &str) {
    let engine = template::make_engine_instance(conn, rev_id).unwrap();
    let mut stmt_a = conn.prepare("
    SELECT id, path, contents FROM input_files
    WHERE EXISTS (
            SELECT 1
            FROM revision_files
            WHERE revision_files.id = input_files.id
            AND revision_files.revision = ?1
    )
    AND input_files.extension = 'md';
    ");
    let mut stmt_b = conn.prepare("
        SELECT * FROM pages
        WHERE NOT EXISTS (
            SELECT 1
            FROM hypertext
            WHERE ?1 = hypertext.input_id
        )
        OR dynamic = 1;
    ");
}

pub fn render(conn: &Connection, rev_id: &str) {
    // Build render engine
    // Query for page ID to render (all pages whose ID is not in the hypertext table, OR whose dynamic column is true (tentative))
    // Bridge query to parallel iterator
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