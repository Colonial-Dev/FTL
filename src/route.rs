use std::borrow::Cow;

use rusqlite::params;
use serde_repr::{Serialize_repr, Deserialize_repr};
use serde_rusqlite::{from_rows};
use num_enum::TryFromPrimitive;
use serde::Deserialize;

use crate::db::DbConn;
use crate::dbdata::*;
use crate::error::*;

#[derive(Serialize_repr, Deserialize_repr, TryFromPrimitive)]
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum RouteKind {
    Unknown = 0,
    StaticAsset = 1,
    Page = 3,
    Stylesheet = 4,
    Redirect = 5,
}

pub fn create_static_asset_routes(conn: &DbConn, rev_id: &str) -> Result<(), DbError> {
    log::info!("Computing static asset routes...");
    
    #[derive(Deserialize, Debug)]
    struct Row {
        id: String,
        path: String,
    }

    let mut insert_route = Route::prepare_insert(&conn)?;

    let mut stmt = conn.prepare("
        SELECT id, path FROM input_files
        WHERE EXISTS (
            SELECT 1
            FROM revision_files
            WHERE revision_files.id = input_files.id
            AND revision_files.revision = ?1
        )
        AND input_files.extension != 'md'
    ")?;

    let rows = from_rows::<Row>(stmt.query(params![&rev_id])?);
    for row in rows {
        let row = row?;
        insert_route(&InputRoute {
            revision: &rev_id,
            id: &row.id,
            path: &row.path.trim_start_matches("src/static/"),
            parent_path: None,
            kind: RouteKind::StaticAsset,
            template: None
        })?;
    }

    log::info!("Done computing static asset routes.");
    Ok(())
}

pub fn create_page_routes(conn: &DbConn, rev_id: &str) -> Result<(), DbError> {
    log::info!("Computing page routes...");

    let mut insert_route = Route::prepare_insert(&conn)?;

    let pages = Page::for_revision(&conn, rev_id)?;
    for page in &pages {
        insert_route(&InputRoute {
            revision: &rev_id,
            id: &page.id,
            path: &page.route,
            parent_path: Some(&to_parent_path(&page.route)),
            kind: RouteKind::Page,
            template: Some(&page.template)
        })?;
    }

    log::info!("Done computing page routes.");
    Ok(())
}

pub fn create_alias_routes(conn: &DbConn, rev_id: &str) -> Result<(), DbError> {
    #[derive(Deserialize)]
    struct Row {
        id: String,
        path: String,
    }
    
    log::info!("Computing alias routes...");

    let mut insert_route = Route::prepare_insert(&conn)?;

    let mut stmt = conn.prepare("
        SELECT * FROM page_aliases
        WHERE EXISTS (
            SELECT 1 FROM revision_files
            WHERE revision_files.revision = ?1
            AND revision_files.id = page_aliases.id
        )
    ")?;

    let rows = from_rows::<Row>(stmt.query(params![&rev_id])?);
    for row in rows {
        let row = row?;
        insert_route(&InputRoute{
            revision: &rev_id,
            id: &row.id,
            path: &row.path,
            parent_path: None,
            kind: RouteKind::Redirect,
            template: None
        })?;
    }

    log::info!("Done computing alias routes.");
    Ok(())
}

fn to_parent_path(route_path: &str) -> Cow<str> {
    let (prefix, _) = route_path.split_once("/").unwrap_or_else(|| ("", ""));

    Cow::from(prefix)
}