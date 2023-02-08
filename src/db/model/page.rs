use ahash::AHashMap;
use serde::{Serialize, Deserialize};
use sqlite::Statement;

use super::*;

/// A high-speed map of strings and TOML values.
pub type TomlMap = AHashMap<String, toml::Value>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Page {
    pub id: String,
    pub template: Option<String>,
    pub offset: i64,
    pub draft: bool,
    pub attributes: TomlMap,
    pub extra: TomlMap
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
                    .unwrap_or_else(|| property.to_string())
            })
        };

        for (key, value) in self.attributes.iter() {
            match value {
                Value::Array(arr) => for value in arr {
                    push_attr(key, value)
                },
                _ => push_attr(key, value)
            }
        }

        attrs
    }
}

impl Insertable for Page {
    const TABLE_NAME: &'static str = "pages";
    const COLUMN_NAMES: &'static [&'static str] = &[
        "id",
        "template",
        "offset",
        "draft",
        "attributes",
        "extra"
    ];

    fn bind_query(&self, stmt: &mut Statement<'_>) -> Result<()> {
        stmt.bind((":id", self.id.as_str()))?;
        stmt.bind((":template", self.template.as_deref()))?;
        stmt.bind((":offset", self.offset))?;
        stmt.bind((":draft", self.draft as i64))?;

        let attributes = serde_cbor::to_vec(&self.attributes)?;
        let extra = serde_cbor::to_vec(&self.extra)?;

        stmt.bind((":attributes", &attributes[..]))?;
        stmt.bind((":extra", &extra[..]))?;

        Ok(())
    }
}

impl Queryable for Page {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            id: stmt.read_string("id")?,
            template: stmt.read_optional_str("template")?,
            offset: stmt.read_i64("offset")?,
            draft: stmt.read_bool("draft")?,
            attributes: {
                let bytes = stmt.read_bytes("attributes")?;
                serde_cbor::from_slice(&bytes)
            }?,
            extra: {
                let bytes = stmt.read_bytes("extra")?;
                serde_cbor::from_slice(&bytes)
            }?
        })
    }
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub id: String,
    pub kind: String,
    pub property: String
}

impl Insertable for Attribute {
    const TABLE_NAME: &'static str = "attributes";
    const COLUMN_NAMES: &'static [&'static str] = &[
        "id",
        "kind",
        "property"
    ];

    fn bind_query(&self, stmt: &mut Statement<'_>) -> Result<()> {
        stmt.bind((":id", self.id.as_str()))?;
        stmt.bind((":kind", self.kind.as_str()))?;
        stmt.bind((":property", self.property.as_str()))?;

        Ok(())
    }
}

impl Queryable for Attribute {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            id: stmt.read_string("id")?,
            kind: stmt.read_string("kind")?,
            property: stmt.read_string("property")?
        })
    }
}