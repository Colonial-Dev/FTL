#![warn(clippy::perf, clippy::style, clippy::cargo, warnings)]

mod clap;
mod config;
mod db;
mod prepare;
mod render;
mod share;

fn main() {
    pretty_env_logger::init();
    let mut conn = db::make_connection().unwrap();

    let rev_id = prepare::walk_src(&mut conn).unwrap();
    prepare::parse_frontmatters(&conn, &rev_id).unwrap();
    prepare::create_static_asset_routes(&conn, &rev_id).unwrap();
    prepare::create_page_routes(&conn, &rev_id).unwrap();
    prepare::create_alias_routes(&conn, &rev_id).unwrap();

    render::render(&mut conn, &rev_id).unwrap();
}