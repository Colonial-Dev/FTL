use std::path::PathBuf;

use rusqlite::params;
use serde::Deserialize;
use serde_rusqlite::from_rows;

use crate::{
    db::{
        data::{Route, RouteIn, RouteKind, Stylesheet, StylesheetIn},
        Connection,
    },
    prelude::*,
};

/// Compile the stylesheet for this revision from `src/sass/style.scss`.
/// Dumps all Sass files to a temporary directory so partials can be resolved.
pub fn compile_stylesheet(conn: &Connection, rev_id: &str) -> Result<()> {
    let temp_dir = PathBuf::from(".ftl/cache/").join(format!("sass-tmp-{}", &rev_id));

    let result = compile(conn, rev_id, &temp_dir);

    if let Err(e) = std::fs::remove_dir_all(temp_dir) {
        warn!("Failed to drop SASS temporary directory: {e}");
    } else {
        debug!("SASS temporary directory dropped.")
    }

    result
}

fn compile(conn: &Connection, rev_id: &str, temp_dir: &PathBuf) -> Result<()> {
    #[derive(Deserialize, Debug)]
    struct Row {
        path: String,
        contents: String,
    }

    let mut stmt = conn.prepare(
        "
        SELECT path, contents FROM input_files
        WHERE extension = 'sass'
        OR extension = 'scss'
        AND EXISTS (
            SELECT 1 FROM revision_files
            WHERE revision_files.id = input_files.id
            AND revision_files.revision = ?1
        )
    ",
    )?;

    let mut rows = from_rows::<Row>(stmt.query(params![&rev_id])?);
    while let Some(row) = rows.next() {
        let row = row?;

        let mut target = temp_dir.clone();
        for chunk in row.path.split('/') {
            target.push(chunk);
        }

        std::fs::create_dir_all(target.parent().unwrap())?;
        std::fs::write(&target, &row.contents)?;
        debug!(
            "Wrote temporary SASS file {:?} to disk (full path: {:?}).",
            target.file_name(),
            target
        )
    }

    let style_file = temp_dir.join("src/assets/sass/style.scss");
    let style_file = style_file.to_str().unwrap();

    let output = grass::from_path(style_file, &grass::Options::default())?;

    let mut insert_sheet = Stylesheet::prepare_insert(conn)?;
    insert_sheet(&StylesheetIn {
        revision: rev_id,
        content: &output,
    })?;

    let mut insert_route = Route::prepare_insert(conn)?;
    insert_route(&RouteIn {
        revision: &rev_id,
        id: "style",
        route: "style.css",
        parent_route: None,
        kind: RouteKind::Stylesheet,
    })?;

    Ok(())
}
