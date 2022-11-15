use std::{collections::BTreeMap, path::{PathBuf, Path}};

use minijinja::{
    value::{Object, Value, ValueKind},
    ErrorKind,
};
use rusqlite::params;
use serde_rusqlite::from_rows;

use crate::{prelude::*, db::data::{Page, Dependency}, parse::link::Link};

use super::Bridge;

const MUTATING_STATEMENTS: &[&str] = &[
    "ALTER",
    "ATTACH",
    "BEGIN",
    "COMMIT",
    "CREATE",
    "DELETE",
    "DETACH",
    "DROP",
    "INSERT",
    "PRAGMA",
    "RELEASE",
    "REINDEX",
    "ROLLBACK",
    "SAVEPOINT",
    "UPDATE",
    "VACUUM",
];

type QueryOutput = Vec<Value>;
type QueryParams = rusqlite::ParamsFromIter<Vec<Box<dyn rusqlite::ToSql>>>;

impl Bridge {
    /// Performs invariant checks on the provided SQL and parameters, and queries them against
    /// the database if they pass.
    ///
    /// Returns an appropriate MiniJinja error in the following scenarios:
    /// - The query would perform a mutating operation on the database.
    /// - The parameters are not provided as a sequence or a map.
    /// - The parameters could not be serialized.
    /// - The SQL query is invalid in some form, or if its execution produces errors.
    pub fn query(&self, sql: String, params: Option<Value>) -> Result<QueryOutput> {
        if !Self::is_query_safe(&sql) {
            bail!("Template queries that mutate the database are not supported.");
        }

        let Some(params) = params else {
            return self.execute_query(sql, None)
        };

        match params.kind() {
            ValueKind::Seq => {
                let params: Vec<Value> = params.try_iter().unwrap().collect();
                let params = serde_rusqlite::to_params(params)
                    .wrap_err("Could not serialize query parameters.")?;

                self.execute_query(sql, Some(params))
            }
            ValueKind::Map => {
                let params: BTreeMap<String, Value> = params
                    .try_iter()
                    .unwrap()
                    .map(|key| {
                        let value = params.get_item(&key).unwrap();
                        (key.to_string(), value)
                    })
                    .collect();

                // TODO: to_params_named is probably the correct choice here,
                // but its output is of a different type, so we'll need to work with that.
                let params = serde_rusqlite::to_params(params).map_err(|e| {
                    minijinja::Error::new(
                        ErrorKind::UndefinedError,
                        "Could not serialize query parameters.",
                    )
                    .with_source(e)
                })?;

                self.execute_query(sql, Some(params))
            }
            _ => bail!("SQL query parameters must be passed as either a sequence or a map."),
        }
    }

    /// Executes the provided SQL query with optional parameters, and serializes the results
    /// into a [`Vec<ValueMap>`] for consumption by the calling template.
    fn execute_query(&self, sql: String, params: Option<QueryParams>) -> Result<QueryOutput> {
        let conn = self
            .pool
            .get()
            .expect("Unable to acquire connection from pool!");
        let mut stmt = conn.prepare(&sql).map_err(|e| {
            minijinja::Error::new(ErrorKind::InvalidOperation, "SQL query is invalid.")
                .with_source(e)
        })?;

        let rows = match params {
            Some(params) => stmt.query(params),
            None => stmt.query([]),
        };

        let rows = rows.map_err(|e| {
            minijinja::Error::new(
                ErrorKind::UndefinedError,
                "An error occurred when executing an SQL query.",
            )
            .with_source(e)
        })?;

        from_rows::<BTreeMap<String, Value>>(rows)
            .map(|x| x.wrap_err("An error occurred when deserializing a query's results.") )
            .map(|x| {
                x.map(|tree| {
                    let tree: ValueMap = tree.into();
                    Value::from_object(tree)
                })
            })
            .collect()
    }

    /// Cursory safety check for mutating queries.
    /// While concurrent modification of SQLite is *safe*, it can still cause faults like timeouts.
    ///
    /// This is a footgun guard and does not try to handle the possibility of malicious input.
    fn is_query_safe(sql: &str) -> bool {
        let sql = sql
            .trim()
            .lines()
            .next()
            .map(|x| x.to_uppercase());
        
        match sql {
            Some(opener) => !MUTATING_STATEMENTS.iter().any(|stmt| opener.contains(stmt)),
            None => true
        }
    }

    pub fn resolve_link(&self, path: &str, page: Option<&Page>) -> Result<String> {
        let link = Link::parse(path)?;

        let resolved = match link {
            Link::Relative(path) => {
                match page {
                    Some(page) => {
                        let mut root = PathBuf::from(&page.path);
                        root.pop();
                        root.push(path);
                        root.to_string_lossy().to_string()
                    },
                    None => path.to_owned()
                }
            },
            Link::Internal(path, _) => path,
            Link::External(path) => path.to_owned()
        };

        Ok(resolved)
    }

    pub fn cachebust_link(&self, path: &str, page: Option<&Page>, cachebust: bool) -> Result<String> {
        let resolved = self.resolve_link(path, page)?;

        if !cachebust {
            return Ok(resolved)
        }

        let conn = self.pool.get()?;
        let mut stmt = conn.prepare_cached("
            SELECT input_files.id FROM input_files
            JOIN revision_files ON revision_files.id = input_files.id
            WHERE revision_files.revision = ?1
            AND input_files.path = ?2
        ")?;

        let id = {
            let id: Result<String> = from_rows::<String>(stmt.query(params![self.rev_id, resolved])?)
                .map(|x| x.wrap_err("SQLite deserialization error!") )
                .collect();

            id?
        };

        if id.is_empty() {
            let err = eyre!("Tried to cachebust a file at {path} ({resolved}), but it has no ID.")
                .suggestion("Make sure the file you are referencing exists.");
            
            bail!(err)
        }

        let path = Path::new(&resolved);

        let stem = path
            .file_stem()
            .expect("File should have a filename.")
            .to_string_lossy();

        let ext = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy();

        let root = path
            .parent()
            .unwrap_or(path)
            .to_string_lossy();

        let root = root.trim_start_matches(SITE_SRC_DIRECTORY);
        
        let busted = format!("{root}/{stem}.{id}.{ext}");

        if let Some(page) = page {
            let dependency = Dependency::Id(id);
            self.send((page.id.to_owned(), dependency));
        }

        Ok(busted)
    }
}

/// Minijinja dynamic object wrapper around a [`BTreeMap<String, Value>`], necessary to obey the orphan rule.
/// Doing this allows us to skip the potentially expensive serialization of query results.
#[derive(Debug)]
pub struct ValueMap {
    pub map: BTreeMap<String, Value>,
}

impl From<BTreeMap<String, Value>> for ValueMap {
    fn from(map: BTreeMap<String, Value>) -> Self {
        Self { map }
    }
}

impl std::fmt::Display for ValueMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.map)
    }
}

impl Object for ValueMap {
    fn get_attr(&self, name: &str) -> Option<Value> {
        self.map.get(name).map(|x| x.to_owned())
    }

    fn attributes(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(self.map.keys().map(|key| key.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_safety_check() {
        assert!(Bridge::is_query_safe("SELECT * FROM input_files"));
        assert!(!Bridge::is_query_safe("DELETE FROM input_files"));
    }
}
