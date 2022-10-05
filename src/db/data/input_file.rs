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
    pub fn to_params(&self) -> Result<ParameterSlice> {
        let params = to_params_named(&self)?;
        Ok(params)
    }

    /// Prepares an SQL statement to insert a new row into the `input_files` table and returns a closure that wraps it.
    pub fn prepare_insert(conn: &Connection) -> Result<impl FnMut(&InputFile) -> Result<()> + '_> {        
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