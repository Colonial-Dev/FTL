use flume::{Receiver, Sender};
use once_cell::sync::Lazy;

use crate::prelude::*;
pub use crate::prepare::{
    SITE_ASSET_DIRECTORY, SITE_CONTENT_DIRECTORY, SITE_SRC_DIRECTORY, SITE_TEMPLATE_DIRECTORY,
};

pub static ERROR_CHANNEL: Lazy<ErrorChannel> = Lazy::new(|| ErrorChannel::default() );
pub static THREADS: Lazy<u32> = Lazy::new(|| {
    let threads = std::thread::available_parallelism();
    match threads {
        Ok(num) => num.get() as u32,
        Err(_) => 4,
    }
});


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
        error!("Error sunk: {}", error);
        // Expect justification: channel should never close while this method is callable
        self.error_sink
            .send(error)
            .expect("Build error sink has been closed!");
    }

    pub fn stream_errors(&self) -> flume::TryIter<'_, Error> {
        self.error_stream.try_iter()
    }
}
