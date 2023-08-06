use std::ffi::OsStr;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::db::{Queryable, Route, RouteKind, Statement, StatementExt, DEFAULT_QUERY, NO_PARAMS};
use crate::prelude::*;

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

pub fn create_routes(ctx: &Context, rev_id: &RevisionID) -> Result<()> {
    let conn = ctx.db.get_rw()?;

    let query_static = "
        SELECT input_files.id, path FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.inline = FALSE
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

    let static_routes = conn
        .prepare_reader(query_static, params)?
        .map(|row| -> Result<_> {
            let row: Row = row?;
            let route = row
                .path
                .trim_start_matches(SITE_ASSET_PATH)
                .trim_start_matches(SITE_CONTENT_PATH);

            Ok(Route {
                id: Some(row.id),
                revision: rev_id.to_string(),
                route: route.to_owned(),
                kind: RouteKind::Asset,
            })
        });

    let cachebust_routes = conn
        .prepare_reader(query_static, params)?
        .map(|row| -> Result<_> {
            let row: Row = row?;

            let filename = Path::new(&row.path)
                .file_stem()
                .map(OsStr::to_str)
                .map(Option::unwrap)
                .unwrap();

            let ext = Path::new(&row.path)
                .extension()
                .map(OsStr::to_str)
                .map(Option::unwrap_or_default);

            let route = match ext {
                Some(ext) => format!("static/{filename}.{ext}?v={}", rev_id),
                None => format!("static/{filename}?=v{}", row.id),
            };

            Ok(Route {
                id: Some(row.id),
                revision: rev_id.to_string(),
                route,
                kind: RouteKind::RedirectAsset,
            })
        });

    let page_routes = conn.prepare_reader(query_pages, params)?.map(|row| {
        let row: Row = row?;

        let route = to_route(&row.path);

        let filename = Path::new(&route)
            .file_stem()
            .map(OsStr::to_str)
            .map(Option::unwrap)
            .unwrap_or_default();

        let filepath = route.trim_end_matches(filename);

        Ok(Route {
            id: Some(row.id),
            revision: rev_id.to_string(),
            route: format!("{filepath}{}", slug::slugify(filename)),
            kind: RouteKind::Page,
        })
    });

    let alias_routes = conn.prepare_reader(query_alias, params)?.map(|row| {
        let row: Row = row?;
        Ok(Route {
            id: Some(row.id),
            revision: rev_id.to_string(),
            route: row.path.trim_start_matches('/').to_string(),
            kind: RouteKind::RedirectPage,
        })
    });

    let txn = conn.open_transaction()?;
    let mut insert_route = conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?;

    static_routes
        .chain(cachebust_routes)
        .chain(page_routes)
        .chain(alias_routes)
        .try_for_each(|route| insert_route(&route?))?;

    txn.commit()?;
    info!("Done computing routes.");
    Ok(())
}

static EXT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new("[.][^.]+$").unwrap());

fn to_route(path: &str) -> String {
    let route_path = path
        .trim_start_matches(SITE_CONTENT_PATH)
        .trim_end_matches("index.md")
        .trim_start_matches('/')
        .trim_end_matches('/');

    EXT_REGEX.replace(route_path, "").to_string()
}
