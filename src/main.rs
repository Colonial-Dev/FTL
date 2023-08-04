#![warn(clippy::all)]

mod common;
mod db;
mod parse;
mod render;
mod serve;

mod prelude {
    pub use color_eyre::eyre::{bail, ensure, eyre, Context as EyreContext, ContextCompat};
    pub use color_eyre::{Report, Result, Section};
    pub use indoc::indoc;
    pub use tracing::{debug, error, info, warn};

    pub use crate::common::*;
}

use prelude::*;

fn main() -> Result<()> {
    install_logging();

    let ctx = InnerContext::init()?;
    ctx.db.reinitialize()?;
    render::Renderer::new(&ctx)?.render_revision()?;

    Ok(())
}

fn install_logging() {
    use color_eyre::config::HookBuilder;
    use tracing_error::ErrorLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

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
        .panic_section(indoc! {
            "Well, this is embarassing. It appears FTL has crashed!
            Consider reporting the bug at \"https://github.com/SomewhereOutInSpace/FTL\"."
        })
        .display_env_section(false)
        .display_location_section(false)
        .install()
        .expect("Could not install Eyre hooks!");

    info!("Logging installed.")
}
