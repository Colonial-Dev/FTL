mod db;
mod error;
mod walking;
mod parse;

use error::*;
use flume::{Receiver, Sender};
use std::path::Path;

#[derive(Clone)]
pub struct BuildSinks {
    error_sink: Sender<BuildError>,
    error_stream: Receiver<BuildError>,
}

impl BuildSinks {
    pub fn default() -> Self {
        let (error_sink, error_stream) = flume::unbounded();
        BuildSinks {
            error_sink,
            error_stream,
        }
    }

    pub fn sink_error(&self, error: impl Into<BuildError> + std::fmt::Debug) {
        log::error!("Error sunk: {:#?}", error);
        // Expect justification: channel should never close while this method is callable
        self.error_sink
            .send(error.into())
            .expect("Build error sink has been closed!");
    }

    pub fn stream_errors(&self) -> flume::TryIter<'_, BuildError> {
        self.error_stream.try_iter()
    }
}

fn initialize() -> (BuildSinks, db::DbPool) {
    pretty_env_logger::init();

    let build_sinks = BuildSinks::default();
    let db_pool = db::make_db_pool(Path::new(".ftl/content.db")).unwrap();

    (build_sinks, db_pool)
}

fn main() {
    let (build_sinks, db_pool) = initialize();
    let items = walking::walk_src(&build_sinks);
    db::update_input_files(&db_pool, &items).unwrap();
    let rev_id = db::update_revision_files(&db_pool, &items).unwrap();

    let items = parse::parse_markdown(&db_pool, &build_sinks, &rev_id).unwrap();
    db::update_pages(&db_pool, &items).unwrap();

    // Parse markdown for frontmatter and content offset [DONE]
    // Update pages table with above [DONE]
    // Compute routes 
    // Compile stylesheets
    // Render markdown to HTML
       // - Evaluate shortcodes
       // - Render HTML and evalaute templates
       // - Post-process HTML (cache-busting etcetera)
    // Done?
}