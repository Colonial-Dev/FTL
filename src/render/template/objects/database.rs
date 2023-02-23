use std::{
    collections::HashMap,
    sync::Arc,
};

use minijinja::{
    value::*,
    State as MJState
};

use once_cell::sync::Lazy;
use sqlite::{Bindable, Value as SQLValue};

use crate::{
    prelude::*, 
    db::{
        InputFile, Pool, NO_PARAMS, Queryable, Statement, StatementExt
    },
};

use super::*;

static ASSETS_PATH: Lazy<String> = Lazy::new(|| {
    format!("{SITE_SRC_PATH}{SITE_ASSET_PATH}")
});

static CONTENT_PATH: Lazy<String> = Lazy::new(|| {
    format!("{SITE_SRC_PATH}{SITE_CONTENT_PATH}")
});

/// Dynamic object wrapper around a database connection pool.
/// Used to enable access to a database from within templates.
#[derive(Debug)]
pub struct DbHandle {
    state: State,
    pool: Arc<Pool>,
    rev_id: Arc<String>
}

// Public methods, mainly those called from within the engine.
impl DbHandle {
    pub fn new(state: &State) -> Self {
        Self {
            state: Arc::clone(state),
            pool: Arc::clone(&state.db.ro_pool),
            rev_id: state.get_rev()
        }
    }

    pub fn query(&self, sql: String, params: Option<Value>) -> MJResult {
        match params {
            Some(params) => self.query_with_params(sql, params),
            None => self.query_core(sql, NO_PARAMS)
        }.map_err(Wrap::wrap)
    }

    pub fn get_resource(&self, state: &MJState, path: String) -> Result<Value> {
        let conn = self.pool.get()?;
        let rev_id = self.rev_id.as_str();
        let mut lookup_targets = Vec::with_capacity(4);

        if let Some(value) = state.lookup("page") {
            if let Some(ticket) = value.downcast_object_ref::<Arc<Ticket>>() {
                lookup_targets.push(
                    format!(
                        "{}{}",
                        &ticket.page.path.trim_end_matches("index.md"),
                        path
                    )
                )
            }
        }

        lookup_targets.extend([
            format!("{}{path}", &*ASSETS_PATH),
            format!("{}{path}", &*CONTENT_PATH),
            path.to_owned()
        ].into_iter());

        let query = "
            SELECT input_files.* FROM input_files
            JOIN revision_files ON revision_files.id = input_files.id
            WHERE revision_files.revision = ?1
            AND input_files.path = ?2
        ";

        let mut query = conn.prepare(query)?;
        let mut get_source = move |path: &str| -> Result<_> {
            use sqlite::State;
            query.reset()?;
            query.bind((1, rev_id))?;
            query.bind((2, path))?;
            match query.next()? {
                State::Row => Ok(Some(InputFile::read_query(&query)?)),
                State::Done => Ok(None)
            }
        };

        for target in &lookup_targets {
            dbg!(target);
            if let Some(file) = get_source(target)? {
                return Ok(Resource {
                    inner: Value::from_serializable(&file),
                    base: file,
                    state: Arc::clone(&self.state)
                }).map(Value::from_object)
            }
        }

        bail!("Could not resolve resource at path \"{path}\".")
    }
}

// Internal methods (kept separate for readability/organization.)
impl DbHandle {
    /// Query the database using the provided SQL and parameters.
    /// 
    /// Parameters must be of the following form:
    /// - A sequence/array of valid types (see [`DbHandle::map_value`].)
    /// - A string-keyed map of valid types.
    /// - A single valid type (assumed to be bound to index 1.)
    fn query_with_params(&self, sql: String, params: Value) -> Result<Value> {
        match params.kind() {
            ValueKind::Seq => {
                let parameters = params
                    .try_iter()?
                    .map(Self::map_value)
                    .enumerate()
                    .try_fold(Vec::new(), |mut acc, (i, param)| -> Result<_> {
                        // SQLite parameter indices start at 1, not 0.
                        acc.push((i + 1, param?));
                        Ok(acc)
                    })?;

                self.query_core(sql, Some(&parameters[..]))
            },
            ValueKind::Map => {
                if params.try_iter()?.any(|key| !matches!(key.kind(), ValueKind::String)) {
                    bail!("When using a map for SQL parameters, all keys must be strings.");
                }
                
                let len = params.len().unwrap();
                let mut parameters = Vec::with_capacity(len);
    
                for key in params.try_iter()? {
                    let param = params.get_item(&key)?;
                    let key = String::try_from(key).unwrap();
                    parameters.push((key, Self::map_value(param)?))
                }
    
                let params_bindable: Vec<_> = parameters
                    .iter()
                    .map(|(key, val)| (key.as_str(), val))
                    .collect();
    
                self.query_core(sql, Some(&params_bindable[..]))
            }
            _ => {
                let parameters = [(1, Self::map_value(params)?)];
                self.query_core(sql, Some(&parameters[..]))
            }
        }
    }
    
    /// Query the database using the provided SQL and optional parameters, converting the resulting
    /// rows into [`ValueMap`]s for use inside of Minijinja.
    fn query_core(&self, sql: String, params: Option<impl Bindable>) -> Result<Value> {
        self.pool.get()?.prepare_reader(sql, params)?
            .try_fold(Vec::new(), |mut acc, map| -> Result<_> {
                let map: ValueMap = map?;
                acc.push(Value::from_struct_object(map));
                Ok(acc)
            })
            .map(Value::from)
    }
    
    /// Attempts to convert the provided Minijinja value into an SQLite value,
    /// bailing with an error if an unsupported type is passed.
    /// 
    /// Currently only these mappings are supported:
    /// - Integers/floats -> SQLite `REAL`s (f64s)
    /// - Strings -> SQLite `TEXT`
    /// - Booleans -> SQLite `INTEGER`s, 0 for false, 1 for true
    /// - None/Undefined -> SQLite `NULL`
    fn map_value(value: Value) -> Result<SQLValue> {
        match value.kind() {
            ValueKind::Number => {
                Ok(SQLValue::Float(
                    f64::try_from(value.clone()).or_else(|_| {
                        i64::try_from(value).map(|x| x as f64)
                    })?
                ))
            }
            ValueKind::String => {
                Ok(SQLValue::String(
                    String::try_from(value)?
                ))
            },
            ValueKind::Bool => {
                Ok(SQLValue::Integer(
                    bool::try_from(value)? as i64
                ))
            },
            ValueKind::None | ValueKind::Undefined => Ok(SQLValue::Null),
            _ => bail!(
                "Unsupported SQL parameter type ({}) - only strings, booleans, numbers and null/undefined are supported.",
                value.kind()
            )
        }
    }
}

impl std::fmt::Display for DbHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "<Database Handle Object>")
    }
}

impl Object for DbHandle {
    fn call_method(&self, state: &MJState, name: &str, args: &[Value]) -> MJResult {
        match name {
            "query" => {
                let (sql, params) = from_args(args)?;
                self.query(sql, params)
            },
            "get_resource" => {
                let (path,) = from_args(args)?;
                self.get_resource(state, path).map_err(Wrap::wrap)
            }
            _ => Err(MJError::new(
                MJErrorKind::UnknownMethod,
                format!("object has no method named {name}")
            ))
        }
    }
}

/// Dynamic object wrapper around a [`HashMap<String, Value>`], necessary to obey the orphan rule.
/// Used to store database query results, skipping the potentially expensive serialization step.
#[derive(Debug)]
pub struct ValueMap(HashMap<String, Value>);

impl std::fmt::Display for ValueMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl StructObject for ValueMap {
    fn get_field(&self, name: &str) -> Option<Value> {
        self.0.get(name).map(|x| x.to_owned())
    }
    
    fn fields(&self) -> Vec<Arc<String>> {
        self.0
            .keys()
            .map(String::as_str)
            .map(intern)
            .collect::<Vec<_>>()
    }
}

impl Queryable for ValueMap {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        let mut map = HashMap::with_capacity(stmt.column_count());

        for column in stmt.column_names() {
            let value = match stmt.read_value(column)? {
                SQLValue::Binary(bytes) => Value::from(bytes),
                SQLValue::Float(float) => Value::from(float),
                SQLValue::Integer(int) => Value::from(int),
                SQLValue::Null => Value::from(()),
                SQLValue::String(str) => Value::from(str)
            };

            map.insert(column.to_owned(), value);
        }

        Ok(Self(map))
    }
}