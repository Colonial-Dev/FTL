use anyhow::anyhow;
use serde::{de::DeserializeOwned, Serializer};

use crate::share::ERROR_CHANNEL;

use super::dependencies::*;

/// Represents a Markdown page and frontmatter.
/// Maps directly to and from rows in the `pages` table.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Page {
    /// The ID of the file associated with this Page.
    /// See [`InputFile`][crate::db::data::InputFile].
    pub id: String,
    /// The path to the page's source file.
    pub path: String,
    /// The URL route associated with this Page.
    pub route: String,
    /// The byte offset in the `content` column of this Page's corresponding 
    /// `input_file` row at which its content begins.
    pub offset: i64,
    /// The title of this Page.
    pub title: String,
    /// The date associated with this Page in ISO 8601 format, if any.
    pub date: Option<String>,
    /// The publish date associated with this Page in ISO 8601 format, if any.
    pub publish_date: Option<String>,
    /// The expiration date associated with this Page in ISO 8601 format, if any.
    pub expire_date: Option<String>,
    /// The description associated with this Page, if any.
    pub description: Option<String>,
    /// The summary associated with this Page, if any.
    pub summary: Option<String>,
    /// The template associated with this Page, if any.
    pub template: Option<String>,
    /// Whether or not this Page is a draft.
    pub draft: bool,
    /// Whether or not this Page is dynamic (should always be re-rendered).
    pub dynamic: bool,
    /// The tags associated with this Page.
    #[serde(serialize_with = "serialize_slice", deserialize_with="deserialize_vec")]
    pub tags: Vec<String>,
    /// The collections associated with this Page.
    #[serde(serialize_with = "serialize_slice", deserialize_with="deserialize_vec")]
    pub collections: Vec<String>,
    /// The aliases (redirects) associated with this Page.
    #[serde(serialize_with = "serialize_slice", deserialize_with="deserialize_vec")]
    pub aliases: Vec<String>,
}

impl Page {
    /// Serializes a [`RevisionFile`] instance to a [`ParameterSlice`] suitable for consumption by [`rusqlite`] queries.
    /// Returns a [`DbError::Serde`] if serialization fails.
    pub fn to_params(&self) -> Result<ParameterSlice> {
        let params = to_params_named(&self)?;
        Ok(params)
    }
    
    /// Prepares an SQL statement to insert a new row into the `pages` table and returns a closure that wraps it.
    pub fn prepare_insert(conn: &Connection) -> Result<impl FnMut(&Page) -> Result<()> + '_> {        
        let mut stmt = conn.prepare("
            INSERT OR IGNORE INTO pages
            VALUES(:id, :path, :route, :offset, :title, :date, :publish_date, 
                :expire_date, :description, :summary, :template, :draft, 
                :dynamic, :tags, :collections, :aliases);
        ")?;

        let closure = move |input: &Page| {
            stmt.execute(input.to_params()?.to_slice().as_slice())?;
            Ok(())
        };

        Ok(closure)
    }

    /// Attempts to query the `pages` table for all rows corresponding to the given revision ID,
    /// then tries to deserialize the results into a [`Vec<Page>`].
    /// 
    /// Returns a [`DbError`] if:
    /// - Something goes wrong when trying to use the database
    /// 
    /// An error value is NOT returned if no rows are found or if deserialization fails.
    pub fn for_revision(conn: &Connection, rev_id: &str) -> Result<Vec<Page>> {
        let mut stmt = conn.prepare("
            SELECT * FROM pages
            WHERE EXISTS (
                SELECT 1
                FROM revision_files
                WHERE revision_files.id = pages.id
                AND revision_files.revision = ?1
            );
        ")?;

        let results: Vec<Page> = from_rows::<Page>(stmt.query(params![rev_id])?)
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

/// Serializes a slice into JSON for storage in the database as `TEXT`.
fn serialize_slice<T, S>(x: &[T], s: S) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: Serializer,
{
    let json = serde_json::to_string(&x).unwrap_or_else(|err| {
        let err = anyhow!("Error when serializing a vector: {err}");
        ERROR_CHANNEL.sink_error(err);
        String::from("[]")
    });
    s.serialize_str(&json)
}

/// Deserialization override for [`Vec`]/slice values being retrieved from the database.
/// Parses the in-database JSON `TEXT` representation back into the original [`Vec`].
fn deserialize_vec<'de, T,  D>(d: D) -> Result<Vec<T>, D::Error>
where
	D: serde::Deserializer<'de>,
    T: DeserializeOwned
{
    let s: std::borrow::Cow<'de, str> = Deserialize::deserialize(d)?;
    let vec: Vec<T> = serde_json::from_str(&s).unwrap_or_else(|err| {
        let err = anyhow!("Error when deserializing a vector: {err}");
        ERROR_CHANNEL.sink_error(err);
        Vec::new()
    });
    Ok(vec)
}