mod output;
mod page;
mod revision;

use std::ops::Deref;

pub use output::*;
pub use page::*;
pub use revision::*;

use rusqlite::{Connection, Row};

use crate::{
    prelude::*,
    common::const_fmt::*
};

/// An interface for types that model tables in the database.
pub trait Model: Sized {
    const TABLE_NAME: &'static str;
    const COLUMNS: &'static [&'static str];

    const INSERT: ConstStr<255> = format_query::<Self>(None);
    const INSERT_IGNORE: ConstStr<255> = format_query::<Self>(Some("IGNORE"));
    const INSERT_REPLACE: ConstStr<255> = format_query::<Self>(Some("REPLACE"));

    fn execute_insert(&self, sql: &str, conn: &impl Deref<Target = Connection>) -> Result<()>;

    fn insert(&self, conn: &impl Deref<Target = Connection>) -> Result<()> {
        self.execute_insert(Self::INSERT.as_ref(), conn)
    }

    fn insert_or_ignore(&self, conn: &impl Deref<Target = Connection>) -> Result<()> {
        self.execute_insert(Self::INSERT_IGNORE.as_ref(), conn)
    }

    fn insert_or_update(&self, conn: &impl Deref<Target = Connection>) -> Result<()> {
        self.execute_insert(Self::INSERT_REPLACE.as_ref(), conn)
    }

    fn from_row(row: &Row) -> Result<Self>;

    // const TABLE_NAME and COLUMN_NAMES remain
    // new consts: QUERY, QUERY_IGNORE, QUERY_REPLACE (ConstStr)
    //
    // Default methods:
    // query_template (generates INSERT $MODE INTO ... string in const fn and stores as a ConstStr)
    //
    // Un-impl'd methods:
    // insert - takes &self and &mut Connection, and inserts self into the database
    // insert_or - allows the caller to specify e.g. OR IGNORE INTO/OR REPLACE INTO on an insert call.
    // from_row - takes an &Row and attempts to extract an instance of Self from it.
}

// Rust doesn't seem to consider invocations in associated constants a "use".
#[allow(dead_code)]
const fn format_query<T: Model>(conflict_clause: Option<&'static str>) -> ConstStr<255> {
    let mut i = 0;
    let mut buf = [0_u8; 255];

    (buf, i) = const_copy(b"INSERT ", buf, i);

    if let Some(clause) = conflict_clause {
        (buf, i) = const_copy(b"OR ", buf, i);
        (buf, i) = const_copy(clause.as_bytes(), buf, i);
        (buf, i) = const_copy(b" ", buf, i);
    }

    (buf, i) = const_copy(b"INTO ", buf, i);
    (buf, i) = const_copy(T::TABLE_NAME.as_bytes(), buf, i);
    (buf, i) = const_copy(b" VALUES(", buf, i);

    let mut idx = 0;

    while idx < T::COLUMNS.len() {
        (buf, i) = const_copy(b":", buf, i);
        (buf, i) = const_copy(T::COLUMNS[idx].as_bytes(), buf, i);

        if idx + 1 != T::COLUMNS.len() {
            (buf, i) = const_copy(b", ", buf, i);
        }
        else {
            (buf, i) = const_copy(b");", buf, i);
        }

        idx += 1;
    }

    ConstStr::new(buf, i)
}

#[macro_export]
macro_rules! model {
    ($(#[$struct_doc:meta])* Name => $name:ident, Table => $table:literal, $($(#[$field_doc:meta])* $fname:ident => $ftype:ty),*) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        $(#[$struct_doc])*
        pub struct $name {
            $($(#[$field_doc])* pub $fname : $ftype),*
        }

        impl $crate::db::Model for $name {
            const TABLE_NAME: &'static str = $table;
            const COLUMNS: &'static [&'static str] = &[$(stringify!($fname)),*];

            fn execute_insert(&self, sql: &str, conn: &impl std::ops::Deref<Target = rusqlite::Connection>) -> color_eyre::Result<()> {
                let mut stmt = conn.prepare_cached(sql)?;
                
                let params = [
                    $((concat!(":", stringify!($fname)), &self.$fname as &dyn rusqlite::ToSql)),*
                ];

                stmt.execute(&params)?;

                Ok(())
            }

            fn from_row(row: &rusqlite::Row) -> color_eyre::Result<Self> {
                Ok(Self {
                    $($fname : row.get::<_, $ftype>(stringify!($fname))?.into()),*
                })
            }
        }
    };
}

#[macro_export]
macro_rules! record {
    (Name => $name:ident, $($fname:ident => $ftype:ty),*) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        /// Automatically generated record type for storing query results.
        pub struct $name {
            $(pub $fname : $ftype),*
        }
        
        impl $name {
            fn from_row(row: &rusqlite::Row) -> color_eyre::Result<Self> {
                Ok(Self {
                    $($fname : row.get(stringify!($fname))?),*
                })
            }
        }
    };
    ($($fname:ident => $ftype:ty),*) => {
        record!(Name => Record, $($fname => $ftype),*);
    };
}

#[macro_export]
macro_rules! enum_sql {
    ($enum:ty) => {
        impl rusqlite::ToSql for $enum {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                let value = rusqlite::types::Value::Integer(*self as i64);
                let value = rusqlite::types::ToSqlOutput::Owned(value);
                Ok(value)
            }
        }

        impl rusqlite::types::FromSql for $enum {
            fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
                value.as_i64().map(<$enum>::from)
            }
        }
    };
}

// TODO fixme
/*#[cfg(test)]
mod test_roundtrip {
    use std::fmt::Debug;
    use std::path::PathBuf;

    use super::*;
    use crate::db::{Connection, DEFAULT_QUERY, NO_PARAMS};

    fn test_insert<T>(conn: &Connection, data: T) -> Result<()>
    where
        T: Insertable,
    {
        let mut insert_data = conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?;
        insert_data(&data)?;
        Ok(())
    }

    fn test_query<T>(conn: &Connection, data: T) -> Result<()>
    where
        T: Insertable + Queryable + Eq + Debug,
    {
        let mut query_data =
            conn.prepare_reader(format!("SELECT * FROM {}", T::TABLE_NAME), NO_PARAMS)?;

        let queried = query_data.next().unwrap()?;

        assert_eq!(data, queried);

        Ok(())
    }

    macro_rules! derive_test {
        ($name:ident, $data:expr) => {
            paste::paste! {
                #[test]
                fn $name() {
                    use crate::db::{IN_MEMORY, PRIME_UP};

                    let conn = Connection::open(IN_MEMORY).unwrap();
                    conn.execute("PRAGMA foreign_keys = OFF;").unwrap();
                    conn.execute(PRIME_UP).unwrap();

                    test_insert(&conn, { $data }).unwrap();
                    test_query(&conn, { $data }).unwrap();
                }
            }
        };
    }

    derive_test!(
        input_file,
        InputFile {
            id: format!("{:016x}", 0xF),
            hash: format!("{:016x}", 0xF),
            path: PathBuf::from("/path/to/file"),
            extension: String::from("foo").into(),
            contents: String::from("bar").into(),
            inline: true
        }
    );

    derive_test!(
        revision,
        Revision {
            id: format!("{:016x}", 0xF),
            name: String::from("A revision").into(),
            time: String::from("A timestamp").into(),
            pinned: true,
            stable: true,
        }
    );

    derive_test!(
        revision_file,
        RevisionFile {
            id: format!("{:016x}", 0xF),
            revision: format!("{:016x}", 0xF)
        }
    );

    derive_test!(
        route,
        Route {
            id: format!("{:016x}", 0xF),
            revision: format!("{:016x}", 0xF),
            route: String::from("A route"),
            kind: RouteKind::Page,
        }
    );

    derive_test!(
        hook,
        Hook {
            id: format!("{:016x}", 0xF),
            paths: String::from("a path"),
            revision: format!("{:016x}", 0xF),
            template: String::from("a template"),
            headers: String::from("some headers"),
            cache: true
        }
    );

    // TODO figure out how to apply this to Page and Attribute (TomlMap doesn't implement Eq)

    derive_test!(
        dependency,
        Dependency {
            relation: Relation::PageAsset,
            parent: format!("{:016x}", 0xF),
            child: format!("{:016x}", 0xF),
        }
    );

    derive_test!(
        output,
        Output {
            id: format!("{:016x}", 0xF).into(),
            kind: OutputKind::Stylesheet,
            content: String::from("content")
        }
    );
}*/
