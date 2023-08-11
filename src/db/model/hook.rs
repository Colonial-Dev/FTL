use sqlite::Statement;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hook {
    pub id: String,
    pub revision: String,
    pub template: String,
    pub headers: String,
    pub cache: bool
}

impl Insertable for Hook {
    const TABLE_NAME: &'static str = "hooks";
    const COLUMN_NAMES: &'static [&'static str] = &[
        "id",
        "revision",
        "template",
        "headers",
        "cache"
    ];

    fn bind_query(&self, stmt: &mut Statement<'_>) -> Result<()> {
        stmt.bind((":id", self.id.as_str()))?;
        stmt.bind((":revision", self.revision.as_str()))?;
        stmt.bind((":template", self.template.as_str()))?;
        stmt.bind((":headers", self.headers.as_str()))?;
        stmt.bind((":cache", self.cache as i64))?;

        Ok(())
    }
}

impl Queryable for Hook {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            id: stmt.read_string("id")?,
            revision: stmt.read_string("revision")?,
            template: stmt.read_string("template")?,
            headers: stmt.read_string("headers")?,
            cache: stmt.read_bool("cache")?
        })
    }
}