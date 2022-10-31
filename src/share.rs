use once_cell::sync::Lazy;

use crate::prelude::*;
pub use crate::prepare::{
    SITE_ASSET_DIRECTORY, SITE_CONTENT_DIRECTORY, SITE_SRC_DIRECTORY, SITE_TEMPLATE_DIRECTORY,
};

pub static ERROR_CONSUMER: Lazy<Consumer<Error>> = Lazy::new(|| {
    Consumer::new(|error: Error| {
        error!("Error sunk: {}", error);
        Ok(())
    })
});

/// The number of threads available on the system.
/// *Defaults to 4 if the true value cannot be determined.*
pub static THREADS: Lazy<u8> = Lazy::new(|| {
    let threads = std::thread::available_parallelism();
    match threads {
        Ok(num) => num.get() as u8,
        Err(e) => {
            warn!("Couldn't determine available parallelism (error: {e:?}) - defaulting to 4 threads.");
            4
        },
    }
});