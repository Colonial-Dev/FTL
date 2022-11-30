//! Types, functions, constants and other items that are globally relevant throughout the FTL codebase.

mod args;
mod config;
mod state;

pub use args::*;
pub use config::*;
pub use state::*;

use once_cell::sync::Lazy;

use crate::prelude::*;

/// Type alias for a `Result<Vec<T>>`, intended for use as a shorthand
/// when collecting iterators over `Result`s.
pub type MaybeVec<T> = Result<Vec<T>>;

pub const CONFIG_FILENAME: &str = "ftl.toml";
pub const SITE_DB_PATH: &str = ".ftl/content.db";
pub const SITE_CACHE_PATH: &str = ".ftl/cache";

pub const SITE_SRC_PATH: &str = "src/";
pub const SITE_ASSET_PATH: &str = "assets/";
pub const SITE_CONTENT_PATH: &str = "content/";
pub const SITE_TEMPLATE_PATH: &str = "templates/";

/// The number of threads available on the system.
/// *Defaults to 1 if the true value cannot be determined.*
pub static THREADS: Lazy<u16> = Lazy::new(|| {
    let threads = std::thread::available_parallelism();
    match threads {
        Ok(num) => num.get() as u16,
        Err(e) => {
            warn!(
                "Couldn't determine available parallelism (error: {e}) - defaulting to 1 thread."
            );
            1
        }
    }
});

/// The number of blocking threads available to the program in an asynchronous context.
/// Evaluates to `THREADS * 64` or `512`, whichever is larger.
pub static BLOCKING_THREADS: Lazy<u16> = Lazy::new(|| std::cmp::max(*THREADS * 64, 512));