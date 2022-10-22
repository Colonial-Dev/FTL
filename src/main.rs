#![warn(clippy::perf, clippy::style, clippy::cargo, warnings)]

mod config;
mod db;
mod parse;
mod prepare;
mod render;
mod share;

mod prelude {
    pub use color_eyre::{
        eyre::{bail, ensure, eyre, Context, Error},
        Report, Result, Section,
    };
    pub use tracing::{debug, error, info, warn};

    pub use crate::{config::*, share::*};
}

use prelude::*;

fn main() -> Result<()> {
    install_logging();
    Config::init()?;
    let mut conn = db::make_connection()?;

    let rev_id = prepare::walk_src(&mut conn)?;
    prepare::parse_frontmatters(&conn, &rev_id)?;
    prepare::create_static_asset_routes(&conn, &rev_id)?;
    prepare::create_page_routes(&conn, &rev_id)?;
    prepare::create_alias_routes(&conn, &rev_id)?;
    render::render(&mut conn, &rev_id)?;

    Ok(())
}

fn install_logging() {
    use color_eyre::config::HookBuilder;
    use tracing_error::ErrorLayer;
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let fmt_layer = fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(ErrorLayer::default())
        .init();

    HookBuilder::new()
        .panic_section("Well, this is embarassing. It appears FTL has crashed!\nConsider reporting the bug at \"https://github.com/SomewhereOutInSpace/FTL\".")
        .display_env_section(false)
        .display_location_section(false)
        .install().expect("Could not install Eyre hooks!");
}
