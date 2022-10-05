use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use once_cell::sync::OnceCell;

/// [`OnceCell`] that holds the global [`Config`] instance for the program.
/// Not intended to be accessed directly; an immutable reference to its contents can be obtained via [`Config::global()`].
static CONFIG_CELL: OnceCell<Config> = OnceCell::new();

/// Represents the contents of (and a safe interface to) FTL's global configuration, 
/// which includes command line arguments and the contents of `ftl.toml`.
#[derive(Debug)]
pub struct Config {
    pub args: Cli,
    _private: ()
}

impl Config {
    /// Returns an immutable reference to the global FTL [`Config`].
    /// Panics if the containing [`OnceCell`] ([`CONFIG_CELL`]) hasn't been initialized by [`Config::build()`].
    pub fn global() -> &'static Config {
        CONFIG_CELL.get().expect("Config instance has not been initialized!")
    }

    /// Attempts to build a [`Config`] instance and insert it into [`CONFIG_CELL`].
    /// 
    /// This function handles:
    /// - Parsing command line arguments.
    /// - Searching for and parsing `ftl.toml`.
    /// - Bundling the resulting data into a single [`Config`] instance.
    /// 
    /// Panics if [`CONFIG_CELL`] has already been initialized.
    pub fn build() -> Result<()> {
        if CONFIG_CELL.get().is_some() { 
            panic!("Config instance has already been initialized!") 
        }

        

        todo!()
    }
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init(Init),
    Build(Build),
    Serve(Serve),
    Db(Db)
}

#[derive(Debug, Args)]
pub struct Init {

}

#[derive(Debug, Args)]
pub struct Build {

}

#[derive(Debug, Args)]
pub struct Serve {

}

#[derive(Debug, Args)]
pub struct Db {

}