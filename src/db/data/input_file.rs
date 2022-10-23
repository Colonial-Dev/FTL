use std::path::Path;

use super::dependencies::*;

/// Represents a file discovered by FTL's walking algorithm;
/// maps directly to and from rows in the `input_files` table.
#[derive(Serialize, Deserialize, Debug)]
pub struct InputFile {
    /// The file's ID value.
    /// Computed as the hash of the file's `hash` and `path` concatenated together,
    /// and formatted as a 16-character hexadecimal string.
    pub id: String,
    /// The hash of the file's contents, formatted as a 16-character hexadecimal string.
    pub hash: String,
    /// The site-root-relative path to the file.
    pub path: PathBuf,
    /// The file's extension, if any.
    pub extension: Option<String>,
    /// The file's contents, if it is inline.
    pub contents: Option<String>,
    /// Whether or not the file's contents are stored in the database.
    /// - When `true`, the file's contents are written to the database as UTF-8 TEXT.
    /// - When `false`, the file is copied to `.ftl/cache` and renamed to its hash.
    pub inline: bool,
}

// Database write methods
impl InputFile {
    /// Serializes an [`InputFile`] instance to a [`ParameterSlice`] suitable for consumption by [`rusqlite`] queries.
    /// Returns a [`DbError::Serde`] if serialization fails.
    pub fn to_params(&self) -> Result<ParameterSlice> {
        let params = to_params_named(&self)?;
        Ok(params)
    }

    /// Prepares an SQL statement to insert a new row into the `input_files` table and returns a closure that wraps it.
    pub fn prepare_insert(conn: &Connection) -> Result<impl FnMut(&InputFile) -> Result<()> + '_> {
        let mut stmt = conn.prepare(
            "
            INSERT OR IGNORE INTO input_files
            VALUES(:id, :path, :hash, :extension, :contents, :inline);
        ",
        )?;

        let closure = move |input: &InputFile| {
            let _ = stmt.execute(input.to_params()?.to_slice().as_slice())?;
            Ok(())
        };

        Ok(closure)
    }
}

//Database read methods
impl InputFile {
    pub fn prepare_get_by_path<'a>(
        conn: &'a Connection,
        rev_id: &'a str,
    ) -> Result<impl FnMut(&Path) -> Result<Option<Self>> + 'a> {
        let mut stmt = conn.prepare(
            "
            SELECT * FROM input_files
            WHERE path = ?1
            AND EXISTS (
                SELECT 1
                FROM revision_files
                WHERE revision_files.id = input_files.id
                AND revision_files.revision = ?2
            )
        ",
        )?;

        let closure = move |path: &Path| -> Result<Option<Self>> {
            let row =
                from_rows::<Self>(stmt.query(params![&path.to_string_lossy(), &rev_id])?).next();
            match row {
                Some(result) => Ok(Some(result?)),
                None => Ok(None),
            }
        };

        Ok(closure)
    }
}
