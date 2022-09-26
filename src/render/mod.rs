mod hypertext;
mod shortcode;
mod template;

use crate::db::*;

pub fn prepare(conn: &Connection, rev_id: &str) {
    template::make_engine_instance(conn, rev_id);
}

pub fn render(conn: &Connection, rev_id: &str) {
    // Query for page ID to render (all pages whose ID is not in the hypertext table, OR whose dynamic column is true (tentative))
    // Build liquid global
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