use std::collections::BTreeMap;
use minijinja::{value::{Value, ValueKind, Object}, ErrorKind};
use serde_rusqlite::from_rows;
use super::{DatabaseBridge, TResult};

const MUTATING_STATEMENTS: &[&str] = &["ALTER", "ATTACH", "BEGIN", "COMMIT", "CREATE", "DELETE", "DETACH", "DROP", "INSERT", "PRAGMA", "RELEASE", "REINDEX", "ROLLBACK", "SAVEPOINT", "UPDATE", "VACUUM"];

type QueryOutput = Vec<Value>;
type QueryParams = rusqlite::ParamsFromIter<Vec<Box<dyn rusqlite::ToSql>>>;

impl DatabaseBridge {
    /// Performs invariant checks on the provided SQL and parameters, and queries them against
    /// the database if they pass.
    /// 
    /// Returns an appropriate MiniJinja error in the following scenarios:
    /// - The query would perform a mutating operation on the database.
    /// - The parameters are not provided as a sequence or a map.
    /// - The parameters could not be serialized.
    /// - The SQL query is invalid in some form, or if its execution produces errors.
    pub fn query(&self, sql: String, params: Option<Value>) -> TResult<QueryOutput> {
        if !Self::is_query_safe(&sql) {
            return Err(minijinja::Error::new(
                ErrorKind::InvalidOperation,
                "Template queries that mutate the database are not supported."
            ))
        }

        let Some(params) = params else {
            return self.execute_query(sql, None)
        };

        match params.kind() {
            ValueKind::Seq => {
                let params: Vec<Value> = params.try_iter().unwrap().collect();
                let params = serde_rusqlite::to_params(params)
                    .map_err(|e| minijinja::Error::new(
                        ErrorKind::UndefinedError,
                        "Could not serialize query parameters."
                    ).with_source(e))?;
                
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
                let params = serde_rusqlite::to_params(params)
                    .map_err(|e| minijinja::Error::new(
                        ErrorKind::UndefinedError,
                        "Could not serialize query parameters."
                    ).with_source(e))?;
                
                self.execute_query(sql, Some(params))
            }
            _ => {
                Err(minijinja::Error::new(
                    ErrorKind::InvalidOperation,
                    "SQL query parameters must be passed as either a sequence or a map."
                ))
            }
        }
    }

    /// Executes the provided SQL query with optional parameters, and serializes the results
    /// into a [`Vec<ValueMap>`] for consumption by the calling template.
    fn execute_query(&self, sql: String, params: Option<QueryParams>) -> TResult<QueryOutput> {
        let conn = self.pool.get().expect("Unable to acquire connection from pool!");
        let mut stmt = conn.prepare(&sql)
            .map_err(|e| minijinja::Error::new(
                ErrorKind::InvalidOperation,
                "SQL query is invalid."
            ).with_source(e))?;

        let rows = match params {
            Some(params) => stmt.query(params),
            None => stmt.query([])
        };
        
        let rows = rows
            .map_err(|e| minijinja::Error::new(
                ErrorKind::UndefinedError,
                "An error occurred when executing an SQL query."
            ).with_source(e))?;

        from_rows::<BTreeMap<String, Value>>(rows)
            .map(|x| {
                x.map_err(|e| minijinja::Error::new(
                    ErrorKind::UndefinedError,
                    "An error occurred when deserializing a query's results."
                ).with_source(e))
            })
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
        let Some(opener) = sql.lines().next() else {
            return false;
        };

        let opener = opener.to_uppercase();

        !MUTATING_STATEMENTS
            .iter()
            .any(|stmt| opener.contains(stmt))
    }
}

/// Minijinja dynamic object wrapper around a [`BTreeMap<String, Value>`], necessary to obey the orphan rule.
/// Doing this allows us to skip the potentially expensive serialization of query results.
#[derive(Debug)]
pub struct ValueMap {
    pub map: BTreeMap<String, Value>
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
        self.map
            .get(name)
            .map(|x| x.to_owned())
    }

    fn attributes(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(
            self.map.keys().map(|key| key.as_ref())
        )
    }
}