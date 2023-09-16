mod hook;
mod output;
mod page;
mod revision;

pub use hook::*;
pub use output::*;
pub use page::*;
pub use revision::*;
use sqlite::{Statement, Value};

use crate::prelude::*;

/// An interface for types that can be serialized and inserted into the database.
pub trait Insertable {
    const TABLE_NAME: &'static str;
    const INSERT_TYPE: &'static str = "INSERT OR IGNORE INTO";
    const COLUMN_NAMES: &'static [&'static str];

    fn bind_query(&self, stmt: &mut Statement<'_>) -> Result<()>;

    fn default_query() -> String {
        let mut query = format!("{} {}\n VALUES(", Self::INSERT_TYPE, Self::TABLE_NAME);
        let mut columns = Self::COLUMN_NAMES.iter().peekable();

        while let Some(column) = columns.next() {
            let value = match columns.peek() {
                Some(_) => format!(":{column}, "),
                None => format!(":{column});"),
            };

            query.push_str(&value);
        }

        query
    }
}

/// An interface for types that can be deserialized from database query results.
pub trait Queryable: Sized {
    fn read_query(stmt: &Statement<'_>) -> Result<Self>;
}

impl Queryable for String {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(stmt.read(0)?)
    }
}

pub trait StatementExt {
    fn read_value(&self, column: &str) -> Result<Value>;
    fn read_string(&self, column: &str) -> Result<String>;
    fn read_i64(&self, column: &str) -> Result<i64>;
    fn read_bool(&self, column: &str) -> Result<bool>;
    fn read_bytes(&self, column: &str) -> Result<Vec<u8>>;
    fn read_optional_str(&self, column: &str) -> Result<Option<String>>;
}

impl StatementExt for Statement<'_> {
    fn read_value(&self, column: &str) -> Result<Value> {
        Ok(self.read::<Value, _>(column)?)
    }

    fn read_string(&self, column: &str) -> Result<String> {
        Ok(self.read::<String, _>(column)?)
    }

    fn read_i64(&self, column: &str) -> Result<i64> {
        Ok(self.read::<i64, _>(column)?)
    }

    fn read_bool(&self, column: &str) -> Result<bool> {
        self.read_i64(column).map(|n| matches!(n, 1..))
    }

    fn read_bytes(&self, column: &str) -> Result<Vec<u8>> {
        Ok(self.read::<Vec<u8>, _>(column)?)
    }

    fn read_optional_str(&self, column: &str) -> Result<Option<String>> {
        Ok(self.read::<Option<String>, _>(column)?)
    }
}

#[cfg(test)]
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
}
