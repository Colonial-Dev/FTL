#[derive(Debug)]
pub enum ParseError {
    Toml(toml::de::Error)
}

impl From<toml::de::Error> for ParseError {
    fn from(item: toml::de::Error) -> Self {
        ParseError::Toml(item)
    }
}