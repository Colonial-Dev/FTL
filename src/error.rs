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