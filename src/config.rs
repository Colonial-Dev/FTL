use std::collections::HashMap;
use std::env;
use std::path::{PathBuf, Path};

use clap::{Args, Parser, Subcommand};
use once_cell::sync::OnceCell;
use serde::{Serialize, Deserialize};

use crate::prelude::*;

/// [`OnceCell`] that holds the global [`Config`] instance for the program.
/// Not intended to be accessed directly; an immutable reference to its contents can be obtained via [`Config::global()`].
static CONFIG_CELL: OnceCell<Config> = OnceCell::new();
static ARGS_CELL: OnceCell<Cli> = OnceCell::new();
const CONFIG_FILENAME: &str = "ftl.toml";

/// Represents the contents of (and a safe interface to) FTL's global configuration, 
/// which includes command line arguments and the contents of `ftl.toml`.
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub root_url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub on_non_fatal: Option<String>,
    pub extra: HashMap<String, toml::Value>
}

impl Config {
    /// Returns an immutable reference to the global FTL [`Config`].
    /// Panics if the containing [`OnceCell`] hasn't been initialized by [`Config::init()`].
    pub fn global() -> &'static Config {
        CONFIG_CELL.get().expect("Config cell has not been initialized!")
    }

    /// Returns an immutable reference to the global FTL [`Cli`].
    /// Panics if the containing [`OnceCell`] hasn't been initialized by [`Config::init()`].
    pub fn args() -> &'static Cli {
        ARGS_CELL.get().expect("Arguments cell has not been initialized!")
    }

    /// Attempts to build instances of [`Config`] and [`Cli`] and insert them into their respective cells.
    /// Panics if [`CONFIG_CELL`] and/or [`ARGS_CELL`] has already been initialized.
    pub fn init() -> Result<()> {
        if ARGS_CELL.get().is_some() { panic!("Args cell has already been initialized!") }
        if CONFIG_CELL.get().is_some() { panic!("Config cell has already been initialized!") }

        init_args();
        init_config()?;

        Ok(())
    }
}

fn init_args() {
    let args = Cli::parse();
    ARGS_CELL.set(args).expect("Failed to initialize Args cell.");
}

fn init_config() -> Result<()> {
    let dir = env::current_dir()?.join("test_site/");
    env::set_current_dir(&dir)?;

    let config_file = try_locate_config(&dir);

    let toml_raw = match config_file {
        Some(file) => {
            std::fs::read_to_string(file)
                .wrap_err("Could not read in configuration file.")
                .suggestion("The configuration file was found, but couldn't be read - try checking your file permissions.")?
        },
        None => bail!("Configuration file not found.")
    };

    let config: Config = toml::from_str(&toml_raw)?;
    CONFIG_CELL.set(config).expect("Failed to initialize Config cell.");

    Ok(())
}

fn try_locate_config(start: &Path) -> Option<PathBuf> {
    let mut path: PathBuf = start.into();
    let target = Path::new(CONFIG_FILENAME);

    loop {
        path.push(target);

        if path.is_file() { 
            break Some(path);
        }

        if !(path.pop() && path.pop()) {
            break None;
        }
    }
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init,
    Build(Build),
    Serve(Serve),
    #[command(subcommand)]
    Db(Db)
}

#[derive(Debug, Args)]
pub struct Build {

}

#[derive(Debug, Args)]
pub struct Serve {

}

#[derive(Debug, Subcommand)]
pub enum Db {
    Stat,
    Compress,
    Clear
}