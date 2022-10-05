use anyhow::{Result};
use rusqlite::params;
use serde_rusqlite::{from_rows};
use serde::Deserialize;

use crate::db::Connection;
use crate::db::data::{Page, Route, RouteIn, RouteKind};



pub fn create_static_asset_routes(conn: &Connection, rev_id: &str) -> Result<()> {
    #[derive(Deserialize, Debug)]
    struct Row {
        id: String,
        path: String,
    }

    let mut insert_route = Route::prepare_insert(conn)?;

    let mut stmt = conn.prepare("
        SELECT id, path FROM input_files
        WHERE EXISTS (
            SELECT 1
            FROM revision_files
            WHERE revision_files.id = input_files.id
            AND revision_files.revision = ?1
        )
        AND input_files.extension != 'md'
        AND input_files.extension != 'sass'
        AND input_files.extension != 'scss'
    ")?;

    let rows = from_rows::<Row>(stmt.query(params![&rev_id])?);
    for row in rows {
        let row = row?;
        insert_route(&RouteIn {
            revision: rev_id,
            id: &row.id,
            route: row.path.trim_start_matches("src/assets/"),
            parent_route: None,
            kind: RouteKind::StaticAsset,
        })?;
    }

    log::info!("Computed static asset routes.");
    Ok(())
}

pub fn create_page_routes(conn: &Connection, rev_id: &str) -> Result<()> {
    let mut insert_route = Route::prepare_insert(conn)?;

    let pages = Page::for_revision(conn, rev_id)?;
    for page in pages {
        insert_route(&RouteIn {
            revision: rev_id,
            id: &page.id,
            route: &page.route,
            parent_route: Some(to_parent_path(&page.route)),
            kind: RouteKind::Page,
        })?;
    }

    log::info!("Computed page routes.");
    Ok(())
}

pub fn create_alias_routes(conn: &Connection, rev_id: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct Row {
        id: String,
        path: String,
    }
    
    let mut insert_route = Route::prepare_insert(conn)?;

    let mut stmt = conn.prepare("
        SELECT page_id, alias FROM page_attributes
        WHERE EXISTS (
            SELECT 1 FROM revision_files
            WHERE revision_files.revision = ?1
            AND revision_files.id = page_attributes.page_id
        )
    ")?;

    let rows = from_rows::<Row>(stmt.query(params![&rev_id])?);
    for row in rows {
        let row = row?;
        insert_route(&RouteIn{
            revision: rev_id,
            id: &row.id,
            route: &row.path,
            parent_route: None,
            kind: RouteKind::Redirect,
        })?;
    }

    log::info!("Computed alias routes.");
    Ok(())
}

fn to_parent_path(route_path: &str) -> &str {
    let (prefix, _) = route_path.split_once('/').unwrap_or(("", ""));

    prefix
}