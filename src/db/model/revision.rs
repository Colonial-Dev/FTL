use std::path::PathBuf;

use sqlite::Statement;

use super::*;

/// Represents a file discovered by FTL's walking algorithm.
#[derive(Debug, Clone)]
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

impl Insertable for InputFile {
    const TABLE_NAME: &'static str = "input_files";
    const COLUMN_NAMES: &'static [&'static str] = &[
        "id",
        "hash",
        "path",
        "extension",
        "contents",
        "inline"
    ];

    fn bind_query(&self, stmt: &mut Statement<'_>) -> Result<()> {
        stmt.bind((":id", self.id.as_str()))?;
        stmt.bind((":hash", self.hash.as_str()))?;
        stmt.bind((":path", self.path.to_string_lossy().as_ref()))?;
        stmt.bind((":extension", self.extension.as_deref()))?;
        stmt.bind((":contents", self.contents.as_deref()))?;
        stmt.bind((":inline", self.inline as i64))?;

        Ok(())
    }
}

impl Queryable for InputFile {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            id: stmt.read_string("id")?,
            hash: stmt.read_string("hash")?,
            path: stmt.read_string("path").map(PathBuf::from)?,
            extension: stmt.read_optional_str("extension")?,
            contents: stmt.read_optional_str("contents")?,
            inline: stmt.read_bool("inline")?
        })
    }
}

/// Represents metadata about a revision.
#[derive(Debug, Clone)]
pub struct Revision {
    pub id: String,
    pub name: Option<String>,
    pub time: Option<String>,
    pub pinned: bool,
    pub stable: bool,
}

impl Insertable for Revision {
    const TABLE_NAME: &'static str = "revisions";
    const COLUMN_NAMES: &'static [&'static str] = &[
        "id",
        "name",
        "time",
        "pinned",
        "stable"
    ];

    fn bind_query(&self, stmt: &mut Statement<'_>) -> Result<()> {
        stmt.bind((":id", self.id.as_str()))?;
        stmt.bind((":name", self.name.as_deref()))?;
        stmt.bind((":time", self.time.as_deref()))?;
        stmt.bind((":pinned", self.pinned as i64))?;
        stmt.bind((":stable", self.stable as i64))?;

        Ok(())
    }
}

impl Queryable for Revision {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            id: stmt.read_string("id")?,
            name: stmt.read_optional_str("name")?,
            time: stmt.read_optional_str("time")?,
            pinned: stmt.read_bool("pinned")?,
            stable: stmt.read_bool("stable")?
        })
    }
}

/// Represents a revision and file ID pair.
#[derive(Debug, Clone)]
pub struct RevisionFile {
    /// The file's ID value.
    pub id: String,
    /// The revision ID associated with the file.
    pub revision: String,
}

impl Insertable for RevisionFile {
    const TABLE_NAME: &'static str = "revision_files";
    const COLUMN_NAMES: &'static [&'static str] = &[
        "id",
        "revision",
    ];

    fn bind_query(&self, stmt: &mut Statement<'_>) -> Result<()> {
        stmt.bind((":id", self.id.as_str()))?;
        stmt.bind((":revision", self.revision.as_str()))?;

        Ok(())
    }
}

impl Queryable for RevisionFile {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            id: stmt.read_string("id")?,
            revision: stmt.read_string("revision")?,
        })
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum RouteKind {
    Unknown = 0,
    StaticAsset = 1,
    Hook = 2,
    Page = 3,
    Stylesheet = 4,
    Redirect = 5,
}

impl From<i64> for RouteKind {
    fn from(value: i64) -> Self {
        use RouteKind::*;
        match value {
            1 => StaticAsset,
            2 => Hook,
            3 => Page,
            4 => Stylesheet,
            5 => Redirect,
            _ => {
                warn!("Encountered an unknown RouteKind discriminant ({value}).");
                Unknown
            }
        }
    }
}

/// Represents a URL route to a file.
/// Maps directly to and from rows in the `routes` table.
#[derive(Debug)]
pub struct Route {
    /// The ID of the file this route points to.
    pub id: Option<String>,
    /// The ID of the revision this route is associated with.
    pub revision: String,
    /// The URL this route qualifies.
    /// Example: `/img/banner.png`, which points to `src/assets/img/banner.png`.
    pub route: String,
    /// What type of asset this route points to.
    pub kind: RouteKind,
}

impl Insertable for Route {
    const TABLE_NAME: &'static str = "routes";
    const COLUMN_NAMES: &'static[&'static str] = &[
        "id",
        "revision",
        "route",
        "kind"
    ];

    fn bind_query(&self, stmt: &mut Statement<'_>) -> Result<()> {
        stmt.bind((":id", self.id.as_deref()))?;
        stmt.bind((":revision", self.revision.as_str()))?;
        stmt.bind((":route", self.route.as_str()))?;
        stmt.bind((":kind", self.kind as i64))?;

        Ok(())
    }
}

impl Queryable for Route {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            id: stmt.read_optional_str("id")?,
            revision: stmt.read_string("revision")?,
            route: stmt.read_string("route")?,
            kind: stmt.read_i64("kind").map(RouteKind::from)?
        })
    }
}