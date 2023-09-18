mod common;
mod db;
mod parse;
mod render;
mod serve;
mod watch;

mod prelude {
    pub use color_eyre::eyre::{
        bail,
        ensure,
        eyre,
        Context as EyreContext,
        ContextCompat
    };

    pub use color_eyre::{
        Report,
        Result, 
        Section
    };

    pub use indoc::indoc;

    pub use tracing::{
        instrument,
        trace,
        debug,
        info,
        warn,
        error
    };

    pub use crate::common::*;
}

use crate::prelude::*;
use crate::render::Renderer;
use crate::serve::InnerServer;

fn main() -> Result<()> {
    use common::{
        Command::*,
        RevisionSubcommand::*,
        DatabaseSubcommand::*,
    };
    
    let ctx = InnerContext::init()?;

    match ctx.args.verbose {
        0 => std::env::set_var("RUST_LOG", "none"),
        1 => std::env::set_var("RUST_LOG", "info"),
        2 => std::env::set_var("RUST_LOG", "debug"),
        _ => std::env::set_var("RUST_LOG", "trace"),
    }
    
    install_logging();
    info!("FTL v{VERSION} by {AUTHORS}");
    info!("This program is licensed under the GNU Affero General Public License, version 3.");
    info!("See {REPOSITORY} for more information.");
    
    ctx.db.clear()?;

    match &ctx.args.command {
        Build { watch, serve, full, .. } => {

        },
        Serve => {
            let renderer = Renderer::new_prepare(&ctx)?;
            
            renderer.render()?;

            InnerServer::new(&ctx, renderer).serve()?;
        }
        Revision(subcommand) => todo!(),
        Db(subcommand) => match subcommand {
            Stat => todo!(),
            Compress => ctx.db.compress()?,
            Clear => ctx.db.clear()?,
        },
        Init { .. } => unreachable!()
    }

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
            Consider reporting the bug at \"https://github.com/Colonial-Dev/FTL\"."
        })
        .capture_span_trace_by_default(true)
        //.display_env_section(false)
        //.display_location_section(false)
        .install()
        .expect("Could not install Eyre hooks!");
}
