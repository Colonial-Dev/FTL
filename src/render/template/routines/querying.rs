
use minijinja::value::{Value, ValueKind};

use sqlite::{Bindable, Value as SQLValue};

use super::objects::ValueMap;
use super::error::{MJResult, WrappedReport as Wrap};

use crate::{
    prelude::*,
    db::Connection
};

pub fn prepare_query(state: &State) -> impl Fn(String, Option<Value>) -> MJResult {
    let state = state.clone();
    move |query: String, params: Option<Value>| {
        let conn = state.db.get_ro().map_err(Wrap::wrap)?;

        match params {
            Some(params) => query_with_params(&conn, query, params),
            None => query_core(&conn, query, &[()][..])
        }.map_err(Wrap::wrap)
    }
}

fn query_with_params(conn: &Connection, query: String, params: Value) -> Result<Value> {
    match params.kind() {
        ValueKind::Seq => {
            let parameters = params
                .try_iter()?
                .map(map_value)
                .enumerate()
                .try_fold(Vec::new(), |mut acc, (i, param)| -> Result<_> {
                    acc.push((i, param?));
                    Ok(acc)
                })?;

            query_core(conn, query, &parameters[..])
        },
        ValueKind::Map => {
            if params.try_iter()?.any(|key| !matches!(key.kind(), ValueKind::String)) {
                bail!("When using a map for SQL parameters, all keys must be strings.");
            }

            let mut parameters = Vec::new();

            for key in params.try_iter()? {
                let param = params.get_item(&key)?;
                let key = String::try_from(key).unwrap();
                parameters.push((key, map_value(param)?))
            }

            let params_bindable: Vec<_> = parameters
                .iter()
                .map(|(key, val)| (key.as_str(), val))
                .collect();

            query_core(conn, query, &params_bindable[..])
        }
        _ => bail!(
            "SQL parameters mut be passed as a sequence or a string-keyed map. (Received {} instead.)",
            params.kind()
        )
    }
}

fn query_core(conn: &Connection, query: String, params: impl Bindable) -> Result<Value> {
    conn.prepare_reader(query, params)?
        .try_fold(Vec::new(), |mut acc, map| -> Result<_> {
            let map: ValueMap = map?;
            acc.push(Value::from_struct_object(map));
            Ok(acc)
        })
        .map(Value::from)
}

fn map_value(value: Value) -> Result<SQLValue> {
    match value.kind() {
        ValueKind::Number => {
            Ok(SQLValue::Float(
                f64::try_from(value)?
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
            "Unsupported SQL parameter type ({}) - only strings, booleans, numbers and NULL are supported.",
            value.kind()
        )
    }
}