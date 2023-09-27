//! Types, functions, constants and other items that are globally relevant throughout the FTL codebase.
#![allow(dead_code)]

pub mod const_fmt;

mod args;
mod cli;
mod config;
mod context;

use std::fmt::Display;
use std::sync::Arc;

use once_cell::sync::Lazy;

pub use args::*;
pub use cli::*;
pub use config::*;
pub use context::*;

use crate::prelude::*;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

pub const CONFIG_FILENAME: &str = "ftl.toml";

pub const SITE_INTERNAL_PATH: &str = ".ftl/";
pub const SITE_DB_PATH: &str = ".ftl/ftl.db";
pub const SITE_CACHE_PATH: &str = ".ftl/cache/";

pub const SITE_ASSET_PATH: &str = "assets/";
pub const SITE_SASS_PATH: &str = "assets/sass/";
pub const SITE_HOOKS_PATH: &str = "hooks/";
pub const SITE_CONTENT_PATH: &str = "content/";
pub const SITE_TEMPLATE_PATH: &str = "templates/";

/// The number of threads available on the system.
/// *Defaults to 1 if the true value cannot be determined.*
pub static THREADS: Lazy<u16> = Lazy::new(|| match std::thread::available_parallelism() {
    Ok(num) => num.get() as u16,
    Err(e) => {
        warn!("Couldn't determine available parallelism (error: {e}) - defaulting to 1 thread.");
        1
    }
});

/// The number of blocking threads available to the program in an asynchronous context.
pub const BLOCKING_THREADS: u16 = 512;

#[derive(Clone, Debug)]
pub struct RevisionID(Arc<String>);

impl RevisionID {
    #[allow(clippy::wrong_self_convention)]
    pub fn into_inner(&self) -> Arc<String> {
        self.0.clone()
    }

    pub fn as_inner(&self) -> &Arc<String> {
        &self.0
    }
}

impl Display for RevisionID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<S> From<S> for RevisionID
where
    S: Into<String>,
{
    fn from(value: S) -> Self {
        let value = value.into();
        let value = Arc::new(value);
        Self(value)
    }
}

impl AsRef<str> for RevisionID {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
