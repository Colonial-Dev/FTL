use serde::de::DeserializeOwned;
use time::{OffsetDateTime, format_description::well_known::Iso8601};

use super::dependencies::*;

/// Represents a Markdown page and frontmatter.
/// Maps directly to and from rows in the `pages` table.
#[derive(Deserialize, Debug)]
pub struct Page {
    /// The ID of the file associated with this Page.
    /// See [InputFile][crate::db::data::InputFile].
    pub id: String,
    /// The URL route associated with this Page.
    pub route: String,
    /// The byte offset in the `content` column of this Page's corresponding 
    /// `input_file` row at which its content begins.
    pub offset: i64,
    /// The title of this Page.
    pub title: String,
    /// The date associated with this Page, if any.
    #[serde(deserialize_with="wrap_datetime")]
    pub date: Option<OffsetDateTime>,
    /// The publish date associated with this Page, if any.
    #[serde(deserialize_with="wrap_datetime")]
    pub publish_date: Option<OffsetDateTime>,
    /// The expiration date associated with this Page, if any.
    #[serde(deserialize_with="wrap_datetime")]
    pub expire_date: Option<OffsetDateTime>,
    /// The description associated with this Page, if any.
    pub description: Option<String>,
    /// The summary associated with this Page, if any.
    pub summary: Option<String>,
    /// The template associated with this Page, if any.
    pub template: Option<String>,
    /// Whether or not this Page is a draft.
    pub draft: bool,
    /// The tags associated with this Page.
    #[serde(deserialize_with="deserialize_vec")]
    pub tags: Vec<String>,
    /// The collections associated with this Page.
    #[serde(deserialize_with="deserialize_vec")]
    pub collections: Vec<String>,
    /// The aliases (redirects) associated with this Page.
    #[serde(deserialize_with="deserialize_vec")]
    pub aliases: Vec<String>,
}

impl Page {
    /// Prepares an SQL statement to insert a new row into the `pages` table and returns a closure that wraps it.
    pub fn prepare_insert(conn: &Connection) -> Result<impl FnMut(&PageIn) -> Result<(), DbError> + '_, DbError> {        
        let mut stmt = conn.prepare("
            INSERT OR IGNORE INTO pages
            VALUES(:id, :route, :offset, :title, :date, :publish_date, 
                :expire_date, :description, :summary, :template, :draft, 
                :tags, :collections, :aliases);
        ")?;

        let closure = move |input: &PageIn| {
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
    pub fn for_revision(conn: &Connection, rev_id: &str) -> Result<Vec<Page>, DbError> {
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

/// Representation of [`Page`] for database insertion. Reference-and-[`Copy`] where possible.
/// 
/// Certain information ([`OffsetDateTime`]s and [`Vec<String>`]s) is mutated into a database-friendly format, requiring allocations.
#[derive(Serialize, Debug)]
pub struct PageIn<'a> {
    pub id: &'a str,
    pub route: &'a str,
    pub offset: i64,
    pub title: &'a str,
    pub date: Option<String>,
    pub publish_date: Option<String>,
    pub expire_date: Option<String>,
    pub description: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub template: Option<&'a str>,
    pub draft: bool,
    pub tags: String,
    pub collections: String,   
    pub aliases: String,
}

impl<'a> PageIn<'a> {
    pub fn to_params(&self) -> Result<ParameterSlice, DbError> {
        let params = to_params_named(&self)?;
        Ok(params)
    }
}

impl<'a> From<&'a Page> for PageIn<'a> {
    fn from(source: &'a Page) -> Self {
        PageIn {
            id: &source.id,
            route: &source.route,
            offset: source.offset,
            title: &source.title,
            date: unwrap_datetime(&source.date),
            publish_date: unwrap_datetime(&source.publish_date),
            expire_date: unwrap_datetime(&source.expire_date),
            description: source.description.as_deref(),
            summary: source.summary.as_deref(),
            template: source.summary.as_deref(),
            draft: source.draft,
            tags: serialize_slice(&source.tags),
            collections: serialize_slice(&source.collections),
            aliases: serialize_slice(&source.aliases),
        }
    }
}

/// Deserialization override for [`OffsetDateTime`] values being retrieved from the database.
/// Parses the in-database `TEXT` representation back into the original [`OffsetDateTime`].
fn wrap_datetime<'de, D>(d: D) -> Result<Option<OffsetDateTime>, D::Error>
where
	D: serde::Deserializer<'de>,
{
    let maybe_dt: Option<String> = Deserialize::deserialize(d)?;
    let dt = match maybe_dt {
        Some(maybe) => {
            let parsed = OffsetDateTime::parse(&maybe, &Iso8601::DEFAULT);
            match parsed {
                Ok(val) => Some(val),
                // TODO error handling
                Err(_) => None
            }
        },
        None => None
    };
    Ok(dt)
}

/// Converts potential [`OffsetDateTime`] values into [`String`]s (for storage in the database as `TEXT`) where applicable.
fn unwrap_datetime(value: &Option<OffsetDateTime>) -> Option<String> {
    value.map(|dt| dt.to_string())
}

/// Serializes a slice into JSON for storage in the database as `TEXT`.
fn serialize_slice<T>(input: &[T]) -> String where T: Serialize {
    serde_json::to_string(&input).unwrap_or_else(|err| {
        let msg = format!("Error when serializing a vector: {}", err);
        let error = serde_rusqlite::Error::Serialization(msg);
        let error = DbError::Serde(error);
        ERROR_CHANNEL.sink_error(error);
        String::from("[]")
    })
}

/// Deserialization override for [`Vec`]/slice values being retrieved from the database.
/// Parses the in-database JSON `TEXT` representation back into the original [`Vec`].
fn deserialize_vec<'de, T,  D>(d: D) -> Result<Vec<T>, D::Error>
where
	D: serde::Deserializer<'de>,
    T: DeserializeOwned
{
    let s: std::borrow::Cow<'de, str> = Deserialize::deserialize(d)?;
    let vec: Vec<T> = serde_json::from_str(&s).unwrap_or_else(|_| Vec::new());
    Ok(vec)
}