#![warn(clippy::perf, clippy::style, clippy::cargo, warnings)]

mod db;
mod prepare;
mod render;
mod share;

fn initialize() -> db::DbPool {
    pretty_env_logger::init();
    
    let db_pool = db::make_pool(std::path::Path::new(".ftl/content.db")).unwrap();

    db_pool
}

fn main() {
    let db_pool = initialize();
    let conn = &mut *db_pool.get().unwrap();


    let rev_id = prepare::walk_src(conn).unwrap();
    prepare::parse_frontmatters(conn, &rev_id).unwrap();
    prepare::create_static_asset_routes(conn, &rev_id).unwrap();
    prepare::create_page_routes(conn, &rev_id).unwrap();
    prepare::create_alias_routes(conn, &rev_id).unwrap();

    render::render(conn, &rev_id).unwrap();
}