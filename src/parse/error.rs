#[derive(Debug)]
pub enum ParseError {
    Toml(toml::de::Error),
    Tera(tera::Error)
}

impl From<toml::de::Error> for ParseError {
    fn from(item: toml::de::Error) -> Self {
        ParseError::Toml(item)
    }
}

impl From<tera::Error> for ParseError {
    fn from(item: tera::Error) -> Self {
        ParseError::Tera(item)
    }
}