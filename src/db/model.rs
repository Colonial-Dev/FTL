use std::path::{Path, PathBuf};

use exemplar::{
    Model, sql_enum,
    BindResult, ExtrResult
};

use rusqlite::types::ValueRef;

use serde::{
    Deserialize,
    Serialize,
    de::DeserializeOwned
};

use crate::prelude::*;

/// A high-speed map of strings and TOML values.
pub type TomlMap = ahash::AHashMap<String, toml::Value>;

/// Represents a file discovered by FTL's walking algorithm.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Model)]
#[table("input_files")]
#[check("sql/prime_up.sql")]
pub struct InputFile {
    /// The file's ID value.
    /// Computed as the hash of the file's `hash` and `path` concatenated together,
    /// and formatted as a 16-character hexadecimal string.
    pub id        : String,
    /// The hash of the file's contents, formatted as a 16-character hexadecimal string.
    pub hash      : String,
    /// The site-root-relative path to the file.
    #[bind(bind_path)]
    #[extr(extr_path)]
    pub path      : PathBuf,
    /// The file's extension, if any.
    pub extension : Option<String>,
    /// The file's contents, if it is inline.
    pub contents  : Option<String>,
    /// Whether or not the file's contents are stored in the database.
    /// - When `true`, the file's contents are written to the database as UTF-8 TEXT.
    /// - When `false`, the file is copied to `.ftl/cache` and renamed to its ID.
    pub inline    : bool,
}

impl InputFile {
    /// Given a path and input file ID, this function generates its cachebusted link
    /// in the format "/static/{filename}.{ext (if it exists)}?v={id}".
    pub fn cachebust(&self) -> String {
        use std::ffi::OsStr;

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

#[derive(Debug, Clone, Model)]
#[table("revisions")]
#[check("sql/prime_up.sql")]
pub struct Revision {
    pub id     : String,
    pub name   : Option<String>,
    pub time   : Option<String>,
    pub pinned : bool,
    pub stable : bool,
}

#[derive(Debug, Clone, Model)]
#[table("revision_files")]
#[check("sql/prime_up.sql")]
/// Represents a revision and file ID pair.
pub struct RevisionFile {
    /// The file's ID value.
    pub id: String,
    /// The revision ID associated with the file.
    pub revision: String,
}

#[derive(Debug, Clone, Model)]
#[table("hooks")]
#[check("sql/prime_up.sql")]
pub struct Hook {
    pub id       : String,
    pub paths    : String,
    pub template : String,
    pub headers  : String,
    pub cache    : bool,
}

/// Represents a URL route to a file.
#[derive(Debug, Clone, Model)]
#[table("routes")]
#[check("sql/prime_up.sql")]
pub struct Route {
    /// The ID of the file this route points to.
    pub id       : String,
    /// The ID of the revision this route is associated with.
    pub revision : String,
    /// The URL this route qualifies.
    /// 
    /// Example: `/img/banner.png`, which points to `assets/img/banner.png`.
    pub route    : String,
    /// What type of asset this route points to.
    pub kind     : RouteKind,
}

sql_enum! {
    Name => RouteKind,
    Asset,
    Hook,
    Page,
    Stylesheet,
    RedirectPage,
    RedirectAsset,
}

#[derive(Serialize, Deserialize, Debug, Clone, Model)]
#[table("pages")]
#[check("sql/prime_up.sql")]
pub struct Page {
    pub id         : String,
    pub path       : String,
    pub template   : Option<String>,
    pub offset     : i64,
    pub draft      : bool,
    #[bind(bind_cbor)]
    #[extr(extr_cbor)]
    pub attributes : TomlMap,
    #[bind(bind_cbor)]
    #[extr(extr_cbor)]
    pub extra      : TomlMap,
}

impl Page {
    pub fn flatten_attrs(&self) -> Vec<Attribute> {
        use toml::Value;
        let mut attrs = Vec::new();

        let mut push_attr = |kind: &String, property: &Value| {
            attrs.push(Attribute {
                id: self.id.clone(),
                kind: kind.to_owned(),
                property: property
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| property.to_string()),
            })
        };

        for (key, value) in self.attributes.iter() {
            match value {
                Value::Array(arr) => {
                    for value in arr {
                        push_attr(key, value)
                    }
                }
                _ => push_attr(key, value),
            }
        }

        attrs
    }
}

#[derive(Debug, Clone, Model)]
#[table("attributes")]
#[check("sql/prime_up.sql")]
pub struct Attribute {
    pub id       : String,
    pub kind     : String,
    pub property : String,
}

sql_enum! {
    Name => Relation,
    Intertemplate,
    PageAsset,
    PageTemplate
}

sql_enum! {
    Name => OutputKind,
    Page,
    Stylesheet,
}

#[derive(Debug, Clone, Model)]
#[table("dependencies")]
#[check("sql/prime_up.sql")]
pub struct Dependency {
    pub relation : Relation,
    pub parent   : String,
    pub child    : String,
}

#[derive(Debug, Clone, Model)]
#[table("output_hot")]
#[check("sql/prime_up.sql")]
pub struct Output {
    pub id      : Option<String>,
    pub kind    : OutputKind,
    pub content : String,
}

fn bind_cbor<T: Serialize>(value: &T) -> BindResult {
    use rusqlite::Error;
    use rusqlite::types::ToSqlOutput;

    serde_cbor::to_vec(value)
        .map(|vec| {
            let value = vec.into();
            ToSqlOutput::Owned(value)
        })
        .map_err(|err| {
            let err = Box::new(err);
            Error::ToSqlConversionFailure(err)
        })
}

fn extr_cbor<T: DeserializeOwned>(value: &ValueRef) -> ExtrResult<T> {
    use rusqlite::types::FromSqlError;

    let bytes = value.as_bytes()?;

    serde_cbor::from_slice(bytes)
        .map_err(|err| {
            let err = Box::new(err);
            FromSqlError::Other(err)
        })
}

fn bind_path(value: &Path) -> BindResult {
    use rusqlite::types::ToSqlOutput;
    
    let path = value.to_string_lossy().into_owned();
    let path = path.into();

    Ok(
        ToSqlOutput::Owned(path)
    )
}

fn extr_path(value: &ValueRef) -> ExtrResult<PathBuf> {
    Ok(
        value.as_str()?.into()
    )
}