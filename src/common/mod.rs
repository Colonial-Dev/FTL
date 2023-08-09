//! Types, functions, constants and other items that are globally relevant throughout the FTL codebase.

mod args;
mod config;
mod context;

use std::fmt::Display;
use std::sync::Arc;

pub use args::*;
pub use config::*;
pub use context::*;
use once_cell::sync::Lazy;

use crate::prelude::*;

pub const CONFIG_FILENAME: &str = "ftl.toml";

pub const SITE_INTERNAL_PATH: &str = ".ftl/";
pub const SITE_DB_PATH: &str = ".ftl/content.db";
pub const SITE_CACHE_PATH: &str = ".ftl/cache/";

pub const SITE_ASSET_PATH: &str = "assets/";
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
/// Evaluates to `THREADS * 64` or `512`, whichever is larger.
pub static BLOCKING_THREADS: Lazy<u16> = Lazy::new(|| std::cmp::max(*THREADS * 64, 512));

#[derive(Clone, Debug)]
pub struct RevisionID(Arc<String>);

impl RevisionID {
    #[allow(clippy::wrong_self_convention)]
    pub fn into_inner(&self) -> Arc<String> {
        self.0.clone()
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

impl sqlite::BindableWithIndex for &RevisionID {
    fn bind<T: sqlite::ParameterIndex>(
        self,
        stmt: &mut sqlite::Statement,
        index: T,
    ) -> sqlite::Result<()> {
        stmt.bind((index, self.0.as_str()))
    }
}

// Safety: we're just forwarding to the implementations on Arc,
// so the RevisionID newtype can be used directly in an ArcSwap.
unsafe impl arc_swap::RefCnt for RevisionID {
    type Base = String;

    fn into_ptr(me: Self) -> *mut Self::Base {
        arc_swap::RefCnt::into_ptr(me.0)
    }

    fn as_ptr(me: &Self) -> *mut Self::Base {
        arc_swap::RefCnt::as_ptr(&me.0)
    }

    unsafe fn from_ptr(ptr: *const Self::Base) -> Self {
        Self(Arc::from_raw(ptr))
    }
}
