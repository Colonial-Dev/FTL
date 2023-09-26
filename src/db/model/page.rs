use ahash::AHashMap;
use serde::{Deserialize, Serialize};

use super::*;

use crate::model;

/// A high-speed map of strings and TOML values.
pub type TomlMap = AHashMap<String, toml::Value>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Page {
    pub id: String,
    pub path: String,
    pub template: Option<String>,
    pub offset: i64,
    pub draft: bool,
    pub attributes: TomlMap,
    pub extra: TomlMap,
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

impl Model for Page {
    const TABLE_NAME: &'static str = "pages";
    const COLUMNS: &'static [&'static str] = &[
        "id",
        "path",
        "template",
        "offset",
        "draft",
        "attributes",
        "extra",
    ];

    fn execute_insert(&self, sql: &str, conn: &impl Deref<Target = Connection>) -> Result<()> {
        conn
            .prepare_cached(sql)?
            .execute(rusqlite::named_params! {
                ":id"            : self.id,
                ":path"          : self.path,
                ":template"      : self.template,
                ":offset"        : self.offset,
                ":draft"         : self.draft,
                ":attributes"    : serde_cbor::to_vec(&self.attributes)?,
                ":extra"         : serde_cbor::to_vec(&self.extra)?
            })?;
        
        Ok(())
    }

    fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            id         : row.get("id")?,
            path       : row.get("path")?,
            template   : row.get("template")?,
            offset     : row.get("offset")?,
            draft      : row.get("draft")?,
            attributes : row.get("attributes").map(|b: Vec<u8>| serde_cbor::from_slice(&b))??,
            extra      : row.get("extra").map(|b: Vec<u8>| serde_cbor::from_slice(&b))??
        })
    }
}

model! {
    Name     => Attribute,
    Table    => "attributes",
    id       => String,
    kind     => String,
    property => String
}
