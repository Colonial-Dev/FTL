/// Represents an error issued during database interactions.
#[derive(Debug)]
pub enum DbError {
    /// Error originating from the connection pool.
    Pool(r2d2::Error),
    /// Error originating from the database itself. 
    Db(rusqlite::Error),
    /// Error originating from database-related I/O.
    Io(std::io::Error),
    /// Error originating from the serialization or deserialization of database information.
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