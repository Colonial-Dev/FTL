use std::collections::HashMap;
use std::sync::Arc;

use minijinja::value::*;
use minijinja::State;
use rusqlite::{
    Row,
    Params,
    ToSql,
    params_from_iter,
    types::Value as SQLValue
};

use super::*;
use crate::db::Model;
use crate::db::{InputFile, Pool};
use crate::prelude::*;

/// Dynamic object wrapper around a database connection pool.
/// Used to enable access to a database from within templates.
#[derive(Debug)]
pub struct DbHandle {
    ctx: Context,
    pool: Arc<Pool>,
    rev_id: RevisionID,
}

// Public methods, mainly those called from within the engine.
impl DbHandle {
    pub fn new(ctx: &Context, rev_id: &RevisionID) -> Self {
        Self {
            ctx: Arc::clone(ctx),
            pool: Arc::clone(&ctx.db.ro_pool),
            rev_id: rev_id.clone(),
        }
    }

    pub fn query(&self, sql: String, params: Option<Value>) -> MJResult {
        match params {
            Some(params) => self.query_with_params(sql, params),
            None => self.query_core(sql, []),
        }
        .map_err(Wrap::wrap)
    }

    pub fn get_resource(&self, state: &State, path: String) -> Result<Value> {
        let conn = self.pool.get()?;
        let rev_id = self.rev_id.as_ref();
        let mut lookup_targets = Vec::with_capacity(4);

        try_with_ticket(state, |ticket| {
            let target = format!("{}{}", &ticket.page.path.trim_end_matches("index.md"), path);
            lookup_targets.push(target);
        });

        lookup_targets.extend(
            [
                format!("{}{path}", SITE_ASSET_PATH),
                format!("{}{path}", SITE_CONTENT_PATH),
                path.to_owned(),
            ],
        );

        let mut get_source = conn.prepare("
            SELECT input_files.* FROM input_files
            JOIN revision_files ON revision_files.id = input_files.id
            WHERE revision_files.revision = ?1
            AND input_files.path IN (?2, ?3, ?4, ?5)
        ")?;

        get_source.raw_bind_parameter(1, rev_id)?;

        for (i, target) in lookup_targets.iter().enumerate() {
            get_source.raw_bind_parameter(i + 2, target)?;
        }

        let file = match get_source
            .raw_query()
            .and_then(InputFile::from_row)
            .find(|res| res.is_ok()) 
        {
            Some(Ok(file)) => file,
            _ => bail!("Could not resolve resource at path \"{path}\".")
        };

        Ok(Resource {
            inner: Value::from_serializable(&file),
            base: file,
            ctx: Arc::clone(&self.ctx),
            rev_id: self.rev_id.clone(),
        })
        .map(Value::from_object)
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
                    .try_fold(Vec::new(), |mut acc, param| -> Result<_> {
                        // SQLite parameter indices start at 1, not 0.
                        acc.push(param?);
                        Ok(acc)
                    })?;

                self.query_core(sql, params_from_iter(parameters))
            }
            ValueKind::Map => {
                if params
                    .try_iter()?
                    .any(|key| !matches!(key.kind(), ValueKind::String))
                {
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
                    .map(|(key, val)| (key.as_str(), val as &dyn ToSql))
                    .collect();

                self.query_core(sql, &params_bindable[..])
            }
            _ => {
                let parameters = [(&Self::map_value(params)? as &dyn ToSql)];
                self.query_core(sql, &parameters[..])
            }
        }
    }

    /// Query the database using the provided SQL and optional parameters, converting the resulting
    /// rows into [`ValueMap`]s for use inside of Minijinja.
    fn query_core(&self, sql: String, params: impl Params) -> Result<Value> {
        self.pool
            .get()?
            .prepare(&sql)?
            .query_and_then(params, ValueMap::from_row)?
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
                Ok(SQLValue::Real(
                    f64::try_from(value.clone()).or_else(|_| {
                        i64::try_from(value).map(|x| x as f64)
                    })?
                ))
            }
            ValueKind::String => {
                Ok(SQLValue::Text(
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
                "Unsupported SQL parameter type ({}) - only strings, booleans, numbers and none/undefined are supported.",
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
    fn call_method(&self, state: &State, name: &str, args: &[Value]) -> MJResult {
        match name {
            "query" => {
                let (sql, params) = from_args(args)?;
                self.query(sql, params)
            }
            "get_resource" => {
                let (path,) = from_args(args)?;
                self.get_resource(state, path).map_err(Wrap::wrap)
            }
            _ => Err(MJError::new(
                MJErrorKind::UnknownMethod,
                format!("object has no method named {name}"),
            )),
        }
    }
}

/// Dynamic object wrapper around a [`HashMap<String, Value>`], necessary to obey the orphan rule.
/// Used to store database query results, skipping the potentially expensive serialization step.
#[derive(Debug)]
pub struct ValueMap(HashMap<String, Value>);

impl ValueMap {
    pub fn from_row(row: &Row) -> Result<Self> {
        let stmt = row.as_ref();
        let mut map = HashMap::with_capacity(stmt.column_count());

        for column in stmt.column_names() {
            let value = match row.get(column)? {
                SQLValue::Blob(bytes) => Value::from(bytes),
                SQLValue::Real(float) => Value::from(float),
                SQLValue::Integer(int) => Value::from(int),
                SQLValue::Null => Value::from(()),
                SQLValue::Text(str) => Value::from(str)
            };

            map.insert(column.to_owned(), value);
        }

        Ok(Self(map))
    }
}

impl std::fmt::Display for ValueMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl StructObject for ValueMap {
    fn get_field(&self, name: &str) -> Option<Value> {
        self.0.get(name).map(|x| x.to_owned())
    }

    fn fields(&self) -> Vec<Arc<str>> {
        self.0
            .keys()
            .map(String::as_str)
            .map(intern)
            .collect::<Vec<_>>()
    }
}