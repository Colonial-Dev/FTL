use lazy_static::lazy_static;
use flume::{Sender, Receiver};

lazy_static!(
    // This seems wrong, but it's convenient and rustc/clippy are both fine with it sooo...
    pub static ref ERROR_CHANNEL: ErrorChannel = ErrorChannel::default();
);

#[derive(Clone)]
pub struct ErrorChannel {
    error_sink: Sender<BuildError>,
    error_stream: Receiver<BuildError>,
}

impl ErrorChannel {
    pub fn default() -> Self {
        let (error_sink, error_stream) = flume::unbounded();
        ErrorChannel {
            error_sink,
            error_stream,
        }
    }

    pub fn sink_error(&self, error: impl Into<BuildError> + std::fmt::Debug) {
        log::error!("Error sunk: {:#?}", error);
        // Expect justification: channel should never close while this method is callable
        self.error_sink
            .send(error.into())
            .expect("Build error sink has been closed!");
    }

    pub fn stream_errors(&self) -> flume::TryIter<'_, BuildError> {
        self.error_stream.try_iter()
    }
}

pub use crate::db::DbError;

#[derive(Debug)]
pub enum BuildError {
    Walk(WalkError),
    Db(DbError),
    Parse(ParseError),
    Boxed(Box<dyn std::error::Error + Send>),
}

impl From<WalkError> for BuildError {
    fn from(item: WalkError) -> Self {
        BuildError::Walk(item)
    }
}

impl From<DbError> for BuildError {
    fn from(item: DbError) -> Self {
        BuildError::Db(item)
    }
}

impl From<ParseError> for BuildError {
    fn from(item: ParseError) -> Self {
        BuildError::Parse(item)
    }
}

#[derive(Debug)]
pub enum WalkError {
    WalkDir(walkdir::Error),
    Io(std::io::Error),
}

pub use crate::parse::ParseError;