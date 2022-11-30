use std::path::{Path, PathBuf};

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

fn compile(conn: &Connection, rev_id: &str, temp_dir: &Path) -> Result<()> {
    #[derive(Deserialize, Debug)]
    struct Row {
        path: String,
        contents: String,
    }

    let mut stmt = conn.prepare(
        "
        SELECT path, contents FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND extension IN ('sass', 'scss')
    ",
    )?;

    let rows = from_rows::<Row>(stmt.query(params![&rev_id])?);
    for row in rows {
        let row = row?;

        let mut target = temp_dir.to_path_buf();
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

    let style_file = temp_dir.join("src/sass/style.scss");

    if !style_file.exists() {
        warn!("Trying to build SCSS but style file does not exist - skipping.");
        return Ok(());
    }

    let style_file = style_file.to_string_lossy();

    let output = grass::from_path(&style_file, &grass::Options::default())?;

    let mut insert_sheet = Stylesheet::prepare_insert(conn, rev_id)?;
    insert_sheet(&StylesheetIn {
        revision: rev_id,
        content: &output,
    })?;

    let mut insert_route = Route::prepare_insert(conn)?;
    insert_route(&RouteIn {
        revision: rev_id,
        id: None,
        route: "style.css",
        parent_route: None,
        kind: RouteKind::Stylesheet,
    })?;

    insert_route(&RouteIn {
        revision: rev_id,
        id: None,
        route: &format!("style.{}.css", rev_id),
        parent_route: None,
        kind: RouteKind::Redirect,
    })?;

    Ok(())
}
