mod db;
mod dbdata;
mod error;
mod walking;
mod parse;

use error::*;
use std::path::Path;

fn initialize() -> (db::DbPool) {
    pretty_env_logger::init();

    let db_pool = db::make_db_pool(Path::new(".ftl/content.db")).unwrap();

    (db_pool)
}

fn main() {
    let (db_pool) = initialize();
    let items = walking::walk_src();
    db::update_input_files(&db_pool, &items).unwrap();
    let rev_id = db::update_revision_files(&db_pool, &items).unwrap();

    let items = parse::parse_markdown(&db_pool, &rev_id).unwrap();
    db::update_pages(&db_pool, &items).unwrap();

    // Parse markdown for frontmatter and content offset [DONE]
    // Update pages table with above [DONE]
    // Compute routes 
    // Parse templates
    // Compile stylesheets
    // Render markdown to HTML
       // - Evaluate shortcodes
       // - Render HTML and evalaute templates
       // - Post-process HTML (cache-busting etcetera)
    // Done?
}