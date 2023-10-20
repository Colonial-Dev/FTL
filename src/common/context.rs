use std::collections::HashMap;
use std::env;
use std::fmt::Arguments as FmtArgs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Parser;
use dialoguer::Confirm;

use super::{Arguments, Config};
use crate::db::Database;
use crate::prelude::*;

/// Type alias for an atomically-refcounted instance of [`InnerContext`].
pub type Context = Arc<InnerContext>;

/// Inner representation of global program context.
#[derive(Debug)]
pub struct InnerContext {
    pub args: Arguments,
    pub config: Config,
    pub db: Database,
}

impl InnerContext {
    pub fn init() -> Result<Context> {
        let args = Arguments::parse();

        if let Command::Init { root_url } = &args.command {
            let mut cfg = Config {
                root_url: root_url.to_owned(),
                build: Build::default(),
                serve: Serve::default(),
                extra: HashMap::new()
            };

            std::fs::create_dir_all(SITE_SASS_PATH)?;
            std::fs::create_dir_all(SITE_HOOKS_PATH)?;
            std::fs::create_dir_all(SITE_CONTENT_PATH)?;
            std::fs::create_dir_all(SITE_TEMPLATE_PATH)?;
            std::fs::create_dir_all(SITE_CACHE_PATH)?;

            Database::create(SITE_DB_PATH)?;
            
            cfg.build.highlight_code = Confirm::new()
                .with_prompt("Enable codeblock syntax highlighting?")
                .default(true)
                .interact()?;
            
            cfg.build.compile_sass = Confirm::new()
                .with_prompt("Enable SASS compilation?")
                .default(true)
                .interact()?;

            cfg.build.smart_punctuation = Confirm::new()
                .with_prompt("Enable smart punctuation?")
                .default(false)
                .interact()?;

            cfg.build.render_emoji = Confirm::new()
                .with_prompt("Enable emoji shortcodes?")
                .default(true)
                .interact()?;

            std::fs::write(
                CONFIG_FILENAME,
                toml::to_string(&cfg)?
            )?;

            println!(
                "\nNew site {}",
                console::style("created.").green().bold().bright()
            );

            std::process::exit(0);
        }

        let dir = validate_env()?;

        let config = dir.join(CONFIG_FILENAME);
        let config = Config::from_path(&config)?;

        let db = dir.join(SITE_DB_PATH);
        let db = Database::open(db)?;

        let ctx = InnerContext { config, args, db };

        Ok(Arc::new(ctx))
    }

    pub fn progressor(&self, msg: Message) -> Progressor {
        Progressor::new(msg)
    }

    pub fn try_println(&self, msg: FmtArgs) {
        if self.pretty_output() {
            println!("{}", msg);
        }
    }

    pub fn drafts_enabled(&self) -> bool {
        match self.args.command {
            Command::Build { drafts, ..} => drafts,
            _ => false
        }
    }

    pub fn devel_mode(&self) -> bool {
        match self.args.command {
            Command::Serve { development } => development,
            Command::Build { serve, .. } => serve,
            _ => false
        }
    }

    pub fn pretty_output(&self) -> bool {
        !self.args.quiet && self.args.verbose == 0
    }
}

// Deref abuse to enable easy access to the configuration field.
impl Deref for InnerContext {
    type Target = Config;

    fn deref(&self) -> &Self::Target {
        &self.config
    }
}

/// Performs environment validation and setup - essentially, ensuring that everything is where and how it should be
/// before the program executes any further. If nothing is amiss, it returns the path of the current directory.
///
/// In no particular order, this function:
/// -
fn validate_env() -> Result<PathBuf> {
    // TODO extend this
    let mut current_dir = env::current_dir()?;

    if env::var("FTL_TEST_MODE").is_ok() {
        current_dir.push("test_site/");
        env::set_current_dir(&current_dir)?;
    }

    match try_locate_config(&current_dir) {
        Some(path) => {
            env::set_current_dir(&path)?;
            Ok(path)
        }
        None => bail!("Failed to locate FTL configuration."),
    }
}

fn try_locate_config(start: &Path) -> Option<PathBuf> {
    let mut path: PathBuf = start.into();
    let target = Path::new(CONFIG_FILENAME);

    loop {
        path.push(target);

        if path.is_file() {
            path.pop();
            break Some(path);
        }

        if !(path.pop() && path.pop()) {
            break None;
        }
    }
}
