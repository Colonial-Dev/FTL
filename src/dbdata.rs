use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use serde_rusqlite::{to_params_named, NamedParamSlice, from_rows};
use rusqlite::params;
use crate::{error::*, db::DbConn};

#[derive(Serialize, Deserialize, Debug, Eq)]
pub struct InputFile {
    pub id: String,
    pub hash: String,
    pub path: PathBuf,
    pub extension: String,
    pub contents: String,
    pub inline: bool
}

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

impl InputFile {
    pub fn to_params(&self) -> Result<NamedParamSlice, DbError> {
        let params = to_params_named(&self)?;
        Ok(params)
    }

    pub fn from_id(conn: &DbConn, id: &str) -> Result<Self, DbError> {
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

    pub fn for_revision(conn: &DbConn, rev_id: &str) -> Result<Vec<InputFile>, DbError> {
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


#[derive(Serialize, Deserialize, Debug)]
pub struct RevisionFile {
    revision: String,
    id: String,
}

impl RevisionFile {
    pub fn to_params(&self) -> Result<NamedParamSlice, DbError> {
        let params = to_params_named(&self)?;
        Ok(params)
    }

    pub fn by_id(conn: &DbConn, id: &str) -> Result<RevisionFile, DbError> {
        let mut stmt = conn.prepare("
            SELECT * FROM revision_files
            WHERE id = ?1;
        ")?;
        let mut  result = from_rows::<Self>(stmt.query(params![id])?);
        let row = result.next();

        match row {
            Some(row) => Ok(row?),
            None => {
                let error = serde_rusqlite::Error::Deserialization(String::from("Entry does not exist or is malformed."));
                Err(error.into())
            },
        }
    }

    pub fn for_revision(conn: &DbConn, rev_id: &str) -> Result<Vec<RevisionFile>, DbError> {
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

#[derive(Serialize, Deserialize, Debug)]
pub struct Page {
    pub id: String,
    pub offset: i64,
    pub title: String,
    pub date: String,
    pub description: String,
    pub summary: String,
    #[serde(serialize_with="vec_to_json", deserialize_with="vec_from_json")]
    pub tags: Vec<String>,
    #[serde(serialize_with="vec_to_json", deserialize_with="vec_from_json")]
    pub series: Vec<String>,
    #[serde(serialize_with="vec_to_json", deserialize_with="vec_from_json")]
    pub aliases: Vec<String>,
    pub template: String,
    pub draft: bool,
    pub publish_date: String,
    pub expire_date: String,
}

pub fn vec_to_json<S>(input: &Vec<String>, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let json = serde_json::to_string(&input).unwrap_or_else(|_| String::new());
    s.serialize_str(&json)
}


fn vec_from_json<'de, D>(d: D) -> Result<Vec<String>, D::Error>
where
	D: serde::Deserializer<'de>,
{
    let s: std::borrow::Cow<'de, str> = Deserialize::deserialize(d)?;
    let vec: Vec<String> = serde_json::from_str(&s).unwrap_or_else(|_| Vec::new());
    Ok(vec)
}

impl Page {
    pub fn to_params(&self) -> Result<NamedParamSlice, DbError> {
        let params = to_params_named(&self).unwrap();
        Ok(params)
    }

    pub fn by_id(conn: &DbConn, id: &str) -> Result<Page, DbError> {
        let mut stmt = conn.prepare("
            SELECT * FROM pages
            WHERE id = ?1;
        ")?;
        let mut  result = from_rows::<Self>(stmt.query(params![id])?);
        let row = result.next();

        match row {
            Some(row) => Ok(row?),
            None => {
                let error = serde_rusqlite::Error::Deserialization(String::from("Entry does not exist or is malformed."));
                Err(error.into())
            },
        }
    }

    pub fn for_revision(conn: &DbConn, rev_id: &str) -> Result<Vec<Page>, DbError> {
        let mut stmt = conn.prepare("
            SELECT * FROM pages
            WHERE EXISTS (
                SELECT 1 FROM revision_files
                WHERE revision = ?1
                AND revision_files.id = pages.id
            )
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