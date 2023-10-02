use std::path::PathBuf;

use serde::Serialize;

use super::*;

use crate::{model, enum_sql};

/// Represents a file discovered by FTL's walking algorithm.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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
    /// - When `false`, the file is copied to `.ftl/cache` and renamed to its ID.
    pub inline: bool,
}

impl InputFile {
    /// Given a path and input file ID, this function generates its cachebusted link
    /// in the format "/static/{filename}.{ext (if it exists)}?v={id}".
    pub fn cachebust(&self) -> String {
        use std::ffi::OsStr;
        use std::path::Path;

        let filename = Path::new(&self.path)
            .file_stem()
            .map(OsStr::to_str)
            .map(Option::unwrap)
            .unwrap();

        let ext = Path::new(&self.path)
            .extension()
            .map(OsStr::to_str)
            .map(Option::unwrap_or_default);   

        match ext {
                Some(ext) => format!("/static/{filename}.{ext}?v={}", &self.id),
                None => format!("/static/{filename}?=v{}", &self.id),
        }
    }
}

impl Model for InputFile {
    const TABLE_NAME: &'static str = "input_files";
    const COLUMNS: &'static [&'static str] =
        &["id", "hash", "path", "extension", "contents", "inline"];

    fn execute_insert(&self, sql: &str, conn: &impl Deref<Target = Connection>) -> Result<()> {
        conn
            .prepare_cached(sql)?
            .execute(rusqlite::named_params! {
                ":id"        : self.id,
                ":hash"      : self.hash,
                // TODO this isn't free - camino?
                ":path"      : self.path.to_string_lossy().as_ref(),
                ":extension" : self.extension,
                ":contents"  : self.contents,
                ":inline"    : self.inline
            })?;

        Ok(())
    }

    fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            id        : row.get("id")?,
            hash      : row.get("hash")?,
            path      : row.get::<_, String>("path")?.into(),
            extension : row.get("extension")?,
            contents  : row.get("contents")?,
            inline    : row.get("inline")?
        })
    }
}

model! {
    /// Represents metadata about a revision.
    Name   => Revision,
    Table  => "revisions",
    id     => String,
    name   => Option<String>,
    time   => Option<String>,
    pinned => bool,
    stable => bool
}

model! {
    /// Represents a revision and file ID pair.
    Name     => RevisionFile,
    Table    => "revision_files",
    /// The file's ID value.
    id       => String,
    /// The revision ID associated with the file.
    revision => String
}

model! {
    Name     => Hook,
    Table    => "hooks",
    id       => String,
    paths    => String,
    template => String,
    headers  => String,
    cache    => bool
}

model! {
    /// Represents a URL route to a file.
    Name     => Route,
    Table    => "routes",
    /// The ID of the file this route points to.
    id       => String,
    /// The ID of the revision this route is associated with.
    revision => String,
    /// The URL this route qualifies.
    /// 
    /// Example: `/img/banner.png`, which points to `assets/img/banner.png`.
    route    => String,
    /// What type of asset this route points to.
    kind     => RouteKind
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RouteKind {
    Asset = 1,
    Hook = 2,
    Page = 3,
    Stylesheet = 4,
    RedirectPage = 5,
    RedirectAsset = 6,
}

impl From<i64> for RouteKind {
    fn from(value: i64) -> Self {
        use RouteKind::*;
        match value {
            1 => Asset,
            2 => Hook,
            3 => Page,
            4 => Stylesheet,
            5 => RedirectPage,
            6 => RedirectAsset,
            _ => panic!("Encountered an unknown RouteKind discriminant ({value}).")
        }
    }
}

enum_sql!(RouteKind);
