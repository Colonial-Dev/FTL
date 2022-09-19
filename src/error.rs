use lazy_static::lazy_static;
use flume::{Sender, Receiver};

lazy_static!(
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

#[derive(Debug)]
pub enum DbError {
    Pool(r2d2::Error),
    Db(rusqlite::Error),
    Io(std::io::Error),
    Serde(serde_rusqlite::Error),
}

impl From<r2d2::Error> for DbError {
    fn from(item: r2d2::Error) -> Self {
        DbError::Pool(item)
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(item: rusqlite::Error) -> Self {
        DbError::Db(item)
    }
}

impl From<std::io::Error> for DbError {
    fn from(item: std::io::Error) -> Self {
        DbError::Io(item)
    }
}

impl From<serde_rusqlite::Error> for DbError {
    fn from(item: serde_rusqlite::Error) -> Self {
        DbError::Serde(item)
    }
}

#[derive(Debug)]
pub enum ParseError {
    Toml(toml::de::Error)
}

impl From<toml::de::Error> for ParseError {
    fn from(item: toml::de::Error) -> Self {
        ParseError::Toml(item)
    }
}