use std::ffi::OsStr;
use std::path::Path;

use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::db::*;
use crate::prelude::*;

record! {
    Name => Row,
    id   => String,
    path => String
}

pub fn create_routes(ctx: &Context, rev_id: &RevisionID) -> Result<()> {
    let mut conn = ctx.db.get_rw()?;
    let txn = conn.transaction()?;

    let mut query_static = txn.prepare("
        SELECT input_files.id, path FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.inline = FALSE
    ")?;

    let mut query_cachebust = txn.prepare("
        SELECT input_files.id, path FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.inline = FALSE
    ")?;

    let mut query_hooks = txn.prepare("
        SELECT * FROM hooks
        JOIN revision_files ON revision_files.id = hooks.id
        WHERE revision_files.revision = ?1
    ")?;

    let mut query_pages = txn.prepare("
        SELECT input_files.id, path FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.extension = 'md'
    ")?;

    let mut query_alias = txn.prepare("
        SELECT attributes.id, property AS path FROM attributes
        JOIN revision_files ON revision_files.id = attributes.id
        WHERE revision_files.revision = ?1
        AND attributes.kind = 'aliases'
    ")?;

    let static_routes = query_static
        .query_and_then([rev_id.as_ref()], Row::from_row)?
        .map_ok(|row| {
            let route = row
                .path
                .trim_start_matches(SITE_ASSET_PATH)
                .trim_start_matches(SITE_CONTENT_PATH);

            Ok(Route {
                id: row.id,
                revision: rev_id.to_string(),
                route: format!("/{route}"),
                kind: RouteKind::Asset,
            })
        })
        .flatten();

    let hook_routes = query_hooks
        .query_and_then([rev_id.as_ref()], Hook::from_row)?
        .map_ok(|hook| -> rusqlite::Result<_> {
            let mut routes = Vec::new();

            for path in hook.paths.split('\n') {
                routes.push(Route {
                    id: hook.id.to_owned(),
                    revision: rev_id.to_string(),
                    route: path.to_string(),
                    kind: RouteKind::Hook,
                });
            }

            Ok(routes)
        })
        .flatten_ok()
        .flatten_ok();
    
    let cachebust_routes = query_cachebust
        .query_and_then([rev_id.as_ref()], Row::from_row)?
        .map_ok(|row| {
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
                Some(ext) => format!("/static/{filename}.{ext}?v={}", row.id),
                None => format!("/static/{filename}?=v{}", row.id),
            };

            Ok(Route {
                id: row.id,
                revision: rev_id.to_string(),
                route,
                kind: RouteKind::RedirectAsset,
            })
        })
        .flatten();

    let page_routes = query_pages
        .query_and_then([rev_id.as_ref()], Row::from_row)?
        .map_ok(|row| {
            let route = to_route(&row.path);

            let filename = Path::new(&route)
                .file_stem()
                .map(OsStr::to_str)
                .map(Option::unwrap)
                .unwrap_or_default();

            let filepath = route.trim_end_matches(filename);

            Ok(Route {
                id: row.id,
                revision: rev_id.to_string(),
                route: format!("/{filepath}{}", slug::slugify(filename)),
                kind: RouteKind::Page,
            })
        })
        .flatten();

    let alias_routes = query_alias
        .query_and_then([rev_id.as_ref()], Row::from_row)?
        .map_ok(|row| {
            Ok(Route {
                id: row.id,
                revision: rev_id.to_string(),
                route: row.path,
                kind: RouteKind::RedirectPage,
            })
        })
        .flatten();

    static_routes
        .chain(cachebust_routes)
        .chain(hook_routes)
        .chain(page_routes)
        .chain(alias_routes)
        .try_for_each(|route| {
            route?.insert_or(&txn, OnConflict::Ignore)
        })?;
    
    
    query_static.finalize()?;
    query_cachebust.finalize()?;
    query_hooks.finalize()?;
    query_pages.finalize()?;
    query_alias.finalize()?;
    
    txn.commit()?;
    info!("Done computing routes.");
    Ok(())
}

static EXT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new("[.][^.]+$").unwrap());

fn to_route(path: &str) -> String {
    let route_path = path
        .trim_start_matches(SITE_CONTENT_PATH)
        .trim_end_matches("index.md")
        .trim_end_matches('/');

    EXT_REGEX.replace(route_path, "").to_string()
}
