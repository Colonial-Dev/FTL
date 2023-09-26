use crate::{model, enum_sql};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Relation {
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
            _ => panic!("Encountered an unknown Relation discriminant ({value}).")
        }
    }
}

enum_sql!(Relation);

model! {
    Name     => Dependency,
    Table    => "dependencies",
    relation => Relation,
    parent   => String,
    child    => String
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OutputKind {
    Page = 1,
    Stylesheet = 2,
}

impl From<i64> for OutputKind {
    fn from(value: i64) -> Self {
        use OutputKind::*;
        match value {
            1 => Page,
            2 => Stylesheet,
            _ => panic!("Encountered an unknown OutputKind discriminant ({value}).")
        }
    }
}

enum_sql!(OutputKind);

model! {
    Name    => Output,
    Table   => "output",
    id      => Option<String>,
    kind    => OutputKind,
    content => String
}