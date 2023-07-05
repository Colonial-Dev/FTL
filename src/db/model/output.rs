use sqlite::Statement;

use super::*;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Relation {
    Unknown = 0,
    Intertemplate = 1,
    PageAsset = 2,
    PageTemplate = 3,
}

impl From<i64> for Relation {
    fn from(value: i64) -> Self {
        use Relation::*;
        match value {
            1 => Intertemplate,
            2 => PageAsset,
            3 => PageTemplate,
            _ => {
                error!("Encountered an unknown Relation discriminant ({value}).");
                Unknown
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub relation: Relation,
    pub parent: String,
    pub child: String,
}

impl Insertable for Dependency {
    const TABLE_NAME: &'static str = "dependencies";
    const COLUMN_NAMES: &'static [&'static str] = &[
        "relation",
        "parent",
        "child"
    ];

    fn bind_query(&self, stmt: &mut Statement<'_>) -> Result<()> {
        stmt.bind((":relation", self.relation as i64))?;
        stmt.bind((":parent", self.parent.as_str()))?;
        stmt.bind((":child", self.child.as_str()))?;
        
        Ok(())
    }
}

impl Queryable for Dependency {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            relation: stmt.read_i64("relation").map(Relation::from)?,
            parent: stmt.read_string("parent")?,
            child: stmt.read_string("child")?
        })
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum OutputKind {
    Unknown = 0,
    Page = 1,
    Stylesheet = 2
}

impl From<i64> for OutputKind {
    fn from(value: i64) -> Self {
        use OutputKind::*;
        match value {
            1 => Page,
            2 => Stylesheet,
            _ => {
                error!("Encountered an unknown OutputKind discriminant ({value}).");
                Unknown
            }
        }
    }
}

#[derive(Debug)]
pub struct Output {
    pub id: Option<String>,
    pub kind: OutputKind,
    pub content: String,
}

impl Insertable for Output {
    const TABLE_NAME: &'static str = "output";
    const COLUMN_NAMES: &'static [&'static str] = &[
        "id",
        "kind",
        "content"
    ];

    fn bind_query(&self, stmt: &mut Statement<'_>) -> Result<()> {
        stmt.bind((":id", self.id.as_deref()))?;
        stmt.bind((":kind", self.kind as i64))?;
        stmt.bind((":content", self.content.as_str()))?;

        Ok(())
    }
}

impl Queryable for Output {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            id: stmt.read_optional_str("id")?,
            kind: stmt.read_i64("kind").map(OutputKind::from)?,
            content: stmt.read_string("content")?
        })
    }
}