mod output;
mod page;
mod revision;

pub use output::*;
pub use page::*;
pub use revision::*;
use sqlite::{Statement, Value};

use crate::prelude::*;

/// An interface for types that can be serialized and inserted into the database.
pub trait Insertable {
    const TABLE_NAME: &'static str;
    const COLUMN_NAMES: &'static [&'static str];

    fn bind_query(&self, stmt: &mut Statement<'_>) -> Result<()>;

    fn default_query() -> String {
        let mut query = format!("INSERT OR IGNORE INTO {}\n VALUES(", Self::TABLE_NAME);
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

// TODO: auto-derive insertion and retrieval tests for all queryable types.
