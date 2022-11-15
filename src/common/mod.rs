mod config;
mod consumer;

use std::sync::Mutex;

pub use config::*;
pub use consumer::Consumer;
use once_cell::sync::Lazy;

use crate::prelude::*;

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
            warn!(
                "Couldn't determine available parallelism (error: {e}) - defaulting to 1 thread."
            );
            1
        }
    }
});

/// A wrapped Eyre report, used to smuggle reports through third-party error types
/// such as MiniJinja's.
#[derive(Debug)]
pub struct WrappedReport(pub Mutex<Report>);

impl WrappedReport {
    /// Extracts the inner Report, replacing it with a dummy value.
    pub fn extract(&self) -> Report {
        let dummy = eyre!("N/A");
        let mut inner = self.0.lock().unwrap();
        std::mem::replace(&mut *inner, dummy)
    }
}

impl std::fmt::Display for WrappedReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl From<Report> for WrappedReport {
    fn from(report: Report) -> Self {
        Self(Mutex::new(report))
    }
}

impl std::error::Error for WrappedReport {

}
