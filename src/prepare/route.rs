use rusqlite::params;
use serde::Deserialize;
use serde_rusqlite::from_rows;

use crate::{
    db::{
        data::{Page, Route, RouteIn, RouteKind},
        Connection,
    },
    prelude::*,
};

pub fn create_static_asset_routes(conn: &Connection, rev_id: &str) -> Result<()> {
    #[derive(Deserialize, Debug)]
    struct Row {
        id: String,
        path: String,
    }

    let mut insert_route = Route::prepare_insert(conn)?;

    let mut stmt = conn.prepare(
        "
        SELECT input_files.id, path FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.extension NOT IN ('md', 'html', 'in', 'sass', 'scss')
    ",
    )?;

    let rows = from_rows::<Row>(stmt.query(params![&rev_id])?);
    for row in rows {
        let row = row?;
        let route = row
            .path
            .trim_start_matches(SITE_SRC_DIRECTORY)
            .trim_start_matches(SITE_ASSET_DIRECTORY)
            .trim_start_matches(SITE_CONTENT_DIRECTORY);

        insert_route(&RouteIn {
            revision: rev_id,
            id: Some(&row.id),
            route,
            parent_route: None,
            kind: RouteKind::StaticAsset,
        })?;
    }

    info!("Computed static asset routes.");
    Ok(())
}

pub fn create_page_routes(conn: &Connection, rev_id: &str) -> Result<()> {
    let mut insert_route = Route::prepare_insert(conn)?;

    let pages = Page::for_revision(conn, rev_id)?;
    for page in pages {
        insert_route(&RouteIn {
            revision: rev_id,
            id: Some(&page.id),
            route: &page.route,
            parent_route: Some(to_parent_path(&page.route)),
            kind: RouteKind::Page,
        })?;
    }

    info!("Computed page routes.");
    Ok(())
}

pub fn create_alias_routes(conn: &Connection, rev_id: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct Row {
        id: String,
        path: String,
    }

    let mut insert_route = Route::prepare_insert(conn)?;

    let mut stmt = conn.prepare(
        "
        SELECT page_attributes.id, alias FROM page_attributes
        JOIN revision_files ON revision_files.id = page_attributes.id
        WHERE revision_files.revision = ?1
    ",
    )?;

    let rows = from_rows::<Row>(stmt.query(params![&rev_id])?);
    for row in rows {
        let row = row?;
        insert_route(&RouteIn {
            revision: rev_id,
            id: Some(&row.id),
            route: &row.path,
            parent_route: None,
            kind: RouteKind::Redirect,
        })?;
    }

    info!("Computed alias routes.");
    Ok(())
}

fn to_parent_path(route_path: &str) -> &str {
    let (prefix, _) = route_path.split_once('/').unwrap_or(("", ""));

    prefix
}
