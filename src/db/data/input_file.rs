use super::dependencies::*;

/// Represents a file discovered by FTL's walking algorithm;
/// maps directly to and from rows in the `input_files` table.
#[derive(Serialize, Deserialize, Debug, Eq)]
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
    pub inline: bool
}

// Because the walking algorithm operates in parallel, we implement
// Ord based on the `hash` value as a way to smooth over any variations
// between program runs.
impl Ord for InputFile {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl PartialOrd for InputFile {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for InputFile {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

// Database write methods
impl InputFile {
    /// Serializes an [`InputFile`] instance to a [`ParameterSlice`] suitable for consumption by [`rusqlite`] queries.
    /// Returns a [`DbError::Serde`] if serialization fails.
    pub fn to_params(&self) -> Result<ParameterSlice, DbError> {
        let params = to_params_named(&self)?;
        Ok(params)
    }

    /// Prepares an SQL statement to insert a new row into the `input_files` table and returns a closure that wraps it.
    pub fn prepare_insert(conn: &Connection) -> Result<impl FnMut(&InputFile) -> Result<(), DbError> + '_, DbError> {        
        let mut stmt = conn.prepare("
            INSERT OR IGNORE INTO input_files
            VALUES(:id, :path, :hash, :extension, :contents, :inline);
        ")?;

        let closure = move |input: &InputFile| {
            let _ = stmt.execute(input.to_params()?.to_slice().as_slice())?;
            Ok(())
        };

        Ok(closure)
    }
}

// Database read methods
impl InputFile {
    /// Attempts to query the `input_files` table for a row with the given `id` value,
    /// then tries to deserializes the result into an [`InputFile`] instance.
    /// 
    /// Returns a [`DbError`] if:
    /// - Something goes wrong when trying to use the database
    /// - Deserialization fails because the row does not exist or is malformed.
    pub fn from_id(conn: &Connection, id: &str) -> Result<Self, DbError> {
        let mut stmt = conn.prepare("
            SELECT * FROM input_files
            WHERE id = ?1;
        ")?;
        let mut result = from_rows::<Self>(stmt.query(params![id])?);
        let row = result.next();

        match row {
            Some(row) => Ok(row?),
            None => {
                let error = serde_rusqlite::Error::Deserialization(String::from("Entry does not exist or is malformed."));
                Err(error.into())
            },
        }
    }

    /// Attempts to query the `input_files` table for all rows corresponding to the given revision ID,
    /// then tries to deserialize the results into a [`Vec<InputFile>`].
    /// 
    /// Returns a [`DbError`] if:
    /// - Something goes wrong when trying to use the database
    /// 
    /// An error value is NOT returned if no rows are found or if deserialization fails.
    pub fn for_revision(conn: &Connection, rev_id: &str) -> Result<Vec<InputFile>, DbError> {
        let mut stmt = conn.prepare("
            SELECT * FROM input_files
            WHERE EXISTS (
                SELECT 1
                FROM revision_files
                WHERE revision_files.id = input_files.id
                AND revision_files.revision = ?1
            );
        ")?;

        let results = from_rows::<Self>(stmt.query(params![rev_id])?)
            .filter_map(|x| {
                match x {
                    Ok(x) => Some(x),
                    Err(_) => None
                }
            })
            .collect();

        Ok(results)
    }
}