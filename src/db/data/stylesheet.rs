pub use super::dependencies::*;

/// Represents a revision and stylesheet pair.
/// Maps directly to and from rows in the `stylesheets` table.
#[derive(Serialize, Deserialize, Debug)]
pub struct Stylesheet {
    /// The revision ID associated with the stylesheet.
    revision: String,
    /// The stylesheet's contents.
    content: String,
}

// Database write methods
impl Stylesheet {
    /// Prepares an SQL statement to insert a new row into the `stylesheets` table and returns a closure that wraps it.
    pub fn prepare_insert<'a>(conn: &'a Connection, rev_id: &'a str) -> Result<impl FnMut(&StylesheetIn) -> Result<()> + 'a> {
        let mut stmt = conn.prepare(
            "
            INSERT OR IGNORE INTO output
            VALUES(NULL, ?1, 2, ?2);
        ",
        )?;

        let closure = move |input: &StylesheetIn| {
            let _ = stmt.execute(params![rev_id, input.content])?;
            Ok(())
        };

        Ok(closure)
    }
}

#[derive(Serialize, Debug)]
pub struct StylesheetIn<'a> {
    pub revision: &'a str,
    pub content: &'a str,
}

impl<'a> StylesheetIn<'a> {
    /// Serializes a [`StylesheetIn`] instance to a [`ParameterSlice`] suitable for consumption by [`rusqlite`] queries.
    /// Returns a [`DbError::Serde`] if serialization fails.
    pub fn to_params(&self) -> Result<ParameterSlice> {
        let params = to_params_named(&self)?;
        Ok(params)
    }
}

impl<'a> From<&'a Stylesheet> for StylesheetIn<'a> {
    fn from(source: &'a Stylesheet) -> Self {
        StylesheetIn {
            revision: &source.revision,
            content: &source.content,
        }
    }
}
