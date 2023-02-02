use std::{
    path::Path,
    ffi::OsStr
};

use once_cell::sync::Lazy;
use regex::Regex;

use crate::{
    db::{
        Route,
        RouteKind,
        Queryable,
        Statement,
        StatementExt
    },
    prelude::*,
};

#[derive(Debug)]
struct Row {
    id: String,
    path: String,
}

impl Queryable for Row {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            id: stmt.read_string("id")?,
            path: stmt.read_string("path")?,
        })
    }
}

pub fn create_routes(state: &State, rev_id: &str) -> Result<()> {
    let conn = state.db.get_rw()?;

    let query_static = "
        SELECT input_files.id, path FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.extension NOT IN ('md', 'html', 'in', 'sass', 'scss')
    ";

    let query_pages = "
        SELECT input_files.id, path FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.extension = 'md'
    ";

    let query_alias = "
        SELECT attributes.id, property AS path FROM attributes
        JOIN revision_files ON revision_files.id = attributes.id
        WHERE revision_files.revision = ?1
        AND attributes.kind = 'aliases'
    ";

    let params = (1, rev_id).into();

    let static_routes = conn.prepare_reader(query_static, params)?
        .map(|row| -> Result<_> {
            let row: Row = row?;
            let route = row
                .path
                .trim_start_matches(SITE_SRC_PATH)
                .trim_start_matches(SITE_ASSET_PATH)
                .trim_start_matches(SITE_CONTENT_PATH)
                .to_string();

            Ok(Route {
                id: Some(row.id),
                revision: rev_id.to_owned(),
                route,
                kind: RouteKind::StaticAsset
            })
        });

    let cachebust_routes = conn.prepare_reader(query_static, params)?
        .map(|row| -> Result<_> {
            let row: Row = row?;

            let ext = Path::new(&row.path)
                .extension()
                .map(OsStr::to_str)
                .map(Option::unwrap_or_default);
            
            let route = match ext {
                Some(ext) => format!("static/{}.{ext}", row.id),
                None => format!("static/{}", row.id)
            };

            Ok(Route {
                id: Some(row.id),
                revision: rev_id.to_owned(),
                route,
                kind: RouteKind::Redirect
            })
        });

    let page_routes = conn.prepare_reader(query_pages, params)?
        .map(|row| {
            let row: Row = row?;
            Ok(Route {
                id: Some(row.id),
                revision: rev_id.to_owned(),
                route: to_route(&row.path),
                kind: RouteKind::Page
            })
        });

    let alias_routes = conn.prepare_reader(query_alias, params)?
        .map(|row| {
            let row: Row = row?;
            Ok(Route {
                id: Some(row.id),
                revision: rev_id.to_owned(),
                route: row.path,
                kind: RouteKind::Redirect
            })
        });

    let txn = conn.open_transaction()?;
    let mut insert_route = conn.prepare_writer(None::<&str>, None::<&[()]>)?;
    
    static_routes
        .chain(cachebust_routes)
        .chain(page_routes)
        .chain(alias_routes)
        .try_for_each(|route| {
            insert_route(&route?)
        })?;

    txn.commit()?;
    info!("Done computing routes.");
    Ok(())
}

static EXT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new("[.][^.]+$").unwrap());

fn to_route(path: &str) -> String {
    let route_path = path
        .trim_start_matches(SITE_SRC_PATH)
        .trim_start_matches(SITE_CONTENT_PATH)
        .trim_end_matches("index.md")
        .trim_start_matches('/')
        .trim_end_matches('/');

    EXT_REGEX.replace(route_path, "").to_string()
}