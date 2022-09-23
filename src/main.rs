// Masochism
#![warn(clippy::pedantic, clippy::perf, clippy::style, clippy::cargo, warnings)]

mod db;
mod error;
mod walking;
mod parse;
mod route;

fn initialize() -> db::DbPool {
    pretty_env_logger::init();
    
    let db_pool = db::make_pool(std::path::Path::new(".ftl/content.db")).unwrap();

    db_pool
}

fn main() {
    let db_pool = initialize();
    let conn = &mut *db_pool.get().unwrap();

    let rev_id = walking::walk_src(conn).unwrap();
    parse::parse_markdown(conn, &rev_id).unwrap();
    route::create_static_asset_routes(conn, &rev_id).unwrap();
    route::create_page_routes(conn, &rev_id).unwrap();
}