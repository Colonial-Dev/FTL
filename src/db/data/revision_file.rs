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
    pub fn prepare_insert(conn: &Connection) -> Result<impl FnMut(&RevisionFileIn) -> Result<()> + '_> {        
        let mut stmt = conn.prepare("
            INSERT OR IGNORE INTO revision_files
            VALUES(:revision, :id);
        ")?;

        let closure = move |input: &RevisionFileIn| {
            let _ = stmt.execute(input.to_params()?.to_slice().as_slice())?;
            Ok(())
        };

        Ok(closure)
    }
}

// Database read methods
impl RevisionFile {
    /// Attempts to query the `revision_files` table for all rows corresponding to the given file ID,
    /// then tries to deserialize the results into a [`Vec<RevisionFile>`].
    /// 
    /// Returns a [`DbError`] if:
    /// - Something goes wrong when trying to use the database
    /// 
    /// An error value is NOT returned if no rows are found or if deserialization fails.
    pub fn for_id(conn: &Connection, id: &str) -> Result<Vec<RevisionFile>> {
        let mut stmt = conn.prepare("
            SELECT * FROM revision_files
            WHERE id = ?1;
        ")?;
        let results = from_rows::<Self>(stmt.query(params![id])?)
            .filter_map(|x| {
                match x {
                    Ok(x) => Some(x),
                    Err(_) => None,
                }
            })
            .collect();
        
        Ok(results)
    }

    /// Attempts to query the `revision_files` table for all rows corresponding to the given revision ID,
    /// then tries to deserialize the results into a [`Vec<RevisionFile>`].
    /// 
    /// Returns a [`DbError`] if:
    /// - Something goes wrong when trying to use the database
    /// 
    /// An error value is NOT returned if no rows are found or if deserialization fails.
    pub fn for_revision(conn: &Connection, rev_id: &str) -> Result<Vec<RevisionFile>> {
        let mut stmt = conn.prepare("
            SELECT * FROM revision_files
            WHERE revision = ?1;
        ")?;
        let results = from_rows::<Self>(stmt.query(params![rev_id])?)
            .filter_map(|x| {
                match x {
                    Ok(x) => Some(x),
                    Err(_) => None,
                }
            })
            .collect();
        
        Ok(results)
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
            id: &source.id
        }
    }
}