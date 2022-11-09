mod config;
mod consumer;

use once_cell::sync::Lazy;

use crate::prelude::*;

pub use config::*;
pub use consumer::Consumer;

pub const SITE_SRC_DIRECTORY: &str = "src/";
pub const SITE_ASSET_DIRECTORY: &str = "assets/";
pub const SITE_CONTENT_DIRECTORY: &str = "content/";
pub const SITE_TEMPLATE_DIRECTORY: &str = "templates/";

pub static ERROR_CONSUMER: Lazy<Consumer<Error>> = Lazy::new(|| {
    Consumer::new(|error: Error| {
        error!("Error sunk: {}", error);
        Ok(())
    })
});

/// The number of threads available on the system.
/// *Defaults to 1 if the true value cannot be determined.*
pub static THREADS: Lazy<u8> = Lazy::new(|| {
    let threads = std::thread::available_parallelism();
    match threads {
        Ok(num) => num.get() as u8,
        Err(e) => {
            warn!("Couldn't determine available parallelism (error: {e}) - defaulting to 1 thread.");
            1
        },
    }
});