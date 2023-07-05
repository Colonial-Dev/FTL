use itertools::Itertools;
use regex::Regex;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use toml::Value;

use crate::{
    db::{
        Page,
        TomlMap,
        Queryable,
        Statement,
        StatementExt, DEFAULT_QUERY, NO_PARAMS
    },
    prelude::*,
};

static TOML_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?s)\+\+\+.*?\+\+\+"#).unwrap()
});

#[derive(Serialize, Deserialize, Debug)]
struct Frontmatter {
    #[serde(skip)]
    pub id: String,
    #[serde(skip)]
    pub path: String,
    #[serde(skip)]
    pub offset: i64,
    pub template: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub attributes: TomlMap,
    #[serde(default)]
    pub extra: TomlMap
}

impl Frontmatter {
    pub fn map_attrs(&mut self) -> Result<()> {
        for value in self.attributes.values_mut() {
            match value {
                Value::Array(arr) => {
                    *arr = arr.iter()
                        .map(Self::value_fmt)
                        .collect()
                },
                Value::Datetime(dt) => *value = dt.to_string().into(),
                Value::Table(_) => {
                    bail!("TOML tables within the [attributes] section are not supported.")
                },
                _ => *value = Self::value_fmt(value)
            }
        }

        Ok(())
    }

    pub fn map_extra(&mut self) {
        for value in self.extra.values_mut() {
            if let Value::Datetime(dt) = value {
                *value = dt.to_string().into()
            }
        }
    }

    /// Custom TOML value stringifer, because its display
    /// implementation adds quotes to strings for some reason.
    fn value_fmt(value: &Value) -> Value {
        value
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| value.to_string())
            .into()
    }
}

// This conversion is strictly one-way, and implementing From would mean
// that the db module would need to know about the Frontmatter type.
#[allow(clippy::from_over_into)]
impl Into<Page> for Frontmatter {
    fn into(self) -> Page {
        Page {
            id: self.id,
            path: self.path,
            offset: self.offset,
            template: self.template,
            draft: self.draft,
            attributes: self.attributes,
            extra: self.extra
        }
    }
}

#[derive(Debug)]
struct Row {
    pub id: String,
    pub path: String,
    pub contents: String
}

impl Queryable for Row {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            id: stmt.read_string("id")?,
            path: stmt.read_string("path")?,
            contents: stmt.read_string("contents")?
        })
    }
}

pub fn parse_frontmatters(state: &State, rev_id: &str) -> Result<()> {
    info!("Starting frontmatter parsing for revision {}...", rev_id);
    let conn = state.db.get_rw()?;

    let query = "
        SELECT id, path, contents FROM input_files
        WHERE EXISTS (
                SELECT 1
                FROM revision_files
                WHERE revision_files.id = input_files.id
                AND revision_files.revision = ?1
            EXCEPT
                SELECT 1
                FROM pages
                WHERE pages.id = input_files.id
        )
        AND input_files.extension = 'md';
    ";

    let params = (1, rev_id).into();
    let txn = conn.open_transaction()?;

    let mut insert_page = conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?;
    let mut insert_attr = conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?;

    conn.prepare_reader(query, params)?
        .map_ok(extract_frontmatter)
        .flatten()
        .try_for_each(|page| -> Result<_> {
            let page = page?;

            insert_page(&page)?;
            for attr in page.flatten_attrs() {
                insert_attr(&attr)?;
            }

            Ok(())
        })?;

    txn.commit()?;
    info!("Done parsing frontmatters for revision {}", rev_id);
    Ok(())
}

fn extract_frontmatter(item: Row) -> Result<Page> {
    debug!("Extracting frontmatter for page {}...", item.id);

    let capture = TOML_REGEX.captures(&item.contents)
        .with_context(||
            format!("Could not find frontmatter in page at \"{}\".", item.path)
        )?
        .get(0).unwrap();
    
    let range = capture.range();
    let body = capture
        .as_str()
        .trim_start_matches("+++")
        .trim_end_matches("+++");

    let mut fm = toml::from_str::<Frontmatter>(body)
        .with_context(||
            format!("Failed to parse frontmatter for page at \"{}\".", item.path)
        )?;

    debug!("Parsed frontmatter for page at \"{}\".", item.path);

    fm.id = item.id;
    fm.path = item.path;
    fm.offset = range.end as i64;
    fm.map_attrs()?;
    fm.map_extra();

    Ok(fm.into())
}