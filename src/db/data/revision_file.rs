pub use super::dependencies::*;

/// Represents a revision and file ID pair.
/// Maps directly to and from rows in the `revision_files` table.
#[derive(Serialize, Deserialize, Debug)]
pub struct RevisionFile {
    /// The revision ID associated with the file.
    revision: String,
    /// The file's ID value. See `id` in [`InputFile`][super::input_file::InputFile].
    id: String,
}

// Database write methods
impl RevisionFile {
    /// Prepares an SQL statement to insert a new row into the `revision_files` table and returns a closure that wraps it.
    pub fn prepare_insert(
        conn: &Connection,
    ) -> Result<impl FnMut(&RevisionFileIn) -> Result<()> + '_> {
        let mut stmt = conn.prepare(
            "
            INSERT OR IGNORE INTO revision_files
            VALUES(:revision, :id);
        ",
        )?;

        let closure = move |input: &RevisionFileIn| {
            let _ = stmt.execute(input.to_params()?.to_slice().as_slice())?;
            Ok(())
        };

        Ok(closure)
    }
}

#[derive(Serialize, Debug)]
pub struct RevisionFileIn<'a> {
    pub revision: &'a str,
    pub id: &'a str,
}

impl<'a> RevisionFileIn<'a> {
    /// Serializes a [`RevisionFileIn`] instance to a [`ParameterSlice`] suitable for consumption by [`rusqlite`] queries.
    /// Returns a [`DbError::Serde`] if serialization fails.
    pub fn to_params(&self) -> Result<ParameterSlice> {
        let params = to_params_named(&self)?;
        Ok(params)
    }
}

impl<'a> From<&'a RevisionFile> for RevisionFileIn<'a> {
    fn from(source: &'a RevisionFile) -> Self {
        RevisionFileIn {
            revision: &source.revision,
            id: &source.id,
        }
    }
}
