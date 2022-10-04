use lazy_static::lazy_static;
use flume::{Sender, Receiver};
use anyhow::{anyhow, Error};

pub use crate::prepare::{SITE_SRC_DIRECTORY, SITE_ASSET_DIRECTORY, SITE_CONTENT_DIRECTORY, SITE_STATIC_DIRECTORY, SITE_TEMPLATE_DIRECTORY};

lazy_static!(
    // This seems wrong, but it's convenient and rustc/clippy are both fine with it sooo...
    pub static ref ERROR_CHANNEL: ErrorChannel = ErrorChannel::default();
);

#[derive(Clone)]
pub struct ErrorChannel {
    error_sink: Sender<Error>,
    error_stream: Receiver<Error>,
}

impl ErrorChannel {
    pub fn default() -> Self {
        let (error_sink, error_stream) = flume::unbounded();
        ErrorChannel {
            error_sink,
            error_stream,
        }
    }

    pub fn sink_error(&self, error: Error) {
        log::error!("Error sunk: {}", error);
        // Expect justification: channel should never close while this method is callable
        self.error_sink
            .send(error.into())
            .expect("Build error sink has been closed!");
    }

    pub fn filter_error<T, E>(&self, result: Result<T, E>) -> Option<T> 
    where 
        E: Into<anyhow::Error>, 
    {
        match result {
            Ok(val) => Some(val),
            Err(e) => {
                self.sink_error(anyhow!(e));
                None
            }
        }
    }

    pub fn stream_errors(&self) -> flume::TryIter<'_, Error> {
        self.error_stream.try_iter()
    }
}