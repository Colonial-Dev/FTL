use std::{
    collections::HashMap,
    sync::Arc,
};

use crossbeam::queue::SegQueue;
use minijinja::{
    context,
    value::*,
    State as MJState
};
use serde::Serialize;
use sqlite::{Bindable, Value as SQLValue};

use crate::{
    prelude::*, 
    db::{
        Pool, Page, Relation, InputFile, NO_PARAMS,
        Queryable, Statement, StatementExt
    },
    parse::{Content, Shortcode}
};

use super::error::{
    MJResult,
    MJError,
    MJErrorKind,
    WrappedReport as Wrap
};

#[derive(Debug)]
pub enum Metadata {
    Dependency {
        relation: Relation,
        child: String
    },
    Rendered(String)
}

/// A rendering ticket, i.e. a discrete unit of rendering work that needs to be done.
/// 
/// Implements [`Object`]/[`StructObject`], which forwards in-engine interactions to the `inner` field
/// while also allowing hooked-in Rust functions to downcast it from [`Value`] and access the same data 
/// in a well-typed manner.
#[derive(Debug)]
pub struct Ticket {
    pub metadata: SegQueue<Metadata>,
    pub state: State,
    pub source: String,
    pub page: Page,
    inner: Value,
}

#[derive(Serialize)]
struct SerTicket<'a> {
    source: &'a str,
    #[serde(flatten)]
    page: &'a Page,
}

impl Ticket {
    pub fn new(state: &State, page: Page, source: &str) -> Self {
        // Slice off the page's frontmatter.
        let source = source[page.offset as usize..].to_string();
        let inner = Value::from_serializable(&SerTicket {
            source: &source,
            page: &page
        });

        Self {
            metadata: SegQueue::new(),
            state: Arc::clone(state),
            source,
            page,
            inner,
        }
    }

    fn render(&self, state: &MJState) -> Result<Value> {
        use Content::*;
        use pulldown_cmark::{Parser, Options, html};

        let mut buffer = String::with_capacity(self.source.len());

        for fragment in Content::parse_many(&self.source)? {
            match fragment {
                Plaintext(text) => buffer += text,
                Emojicode(code) => {
                    match gh_emoji::get(code) {
                        Some(emoji) => buffer += emoji,
                        None => {
                            buffer.push(':');
                            buffer.push_str(code);
                            buffer.push(':');
                        }
                    }
                },
                Shortcode(code) => buffer += &self.eval_shortcode(state, code)?,
                Codeblock(block) => buffer += {
                    &state.env().render_str(
                        "{{ body | highlight(token) }}",
                        context!(
                            body => block.body,
                            token => block.token
                        )
                    )?
                },
                Header(header) => {
                    for _ in 0..header.level {
                        buffer.push('#')
                    }

                    buffer += " ";
                    buffer += header.title;
                    buffer += " ";

                    // TODO: Actually handle anchors and classes
                }
            }
        }

        let options = Options::all();
        let parser = Parser::new_ext(&buffer, options);

        let mut html_buffer = String::with_capacity(buffer.len());
        html::push_html(&mut html_buffer, parser);

        Ok(Value::from_safe_string(html_buffer))
    }

    fn eval_shortcode(&self, state: &MJState, code: Shortcode) -> Result<String> {
        let name = format!("{}.html", code.ident);

        let Ok(template) = state.env().get_template(&name) else {
            let err = eyre!(
                "Page {} contains a shortcode invoking template \"{}\", which does not exist.",
                self.page.id,
                code.ident.0
            )
            .note("This error occurred because a shortcode referenced a template that FTL couldn't find at build time.")
            .suggestion("Double check the shortcode invocation for spelling and path mistakes, and make sure the template is where you think it is.");
    
            bail!(err);
        };

        Ok(template.render(context!(
            code => code,
            page => state.lookup("page")
        ))?)
    }

    fn toc(&self) -> Result<Value> {
        todo!()
    }
}

impl std::fmt::Display for Ticket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl Object for Ticket {
    fn kind(&self) -> ObjectKind<'_> {
        ObjectKind::Struct(self)
    }
    
    fn call_method(&self, state: &MJState, name: &str, _args: &[Value]) -> MJResult {
        match name {
            "render" => self.render(state),
            "toc" => self.toc(),
            _ => Err(eyre!("object has no method named {name}"))
        }.map_err(Wrap::wrap)
    }
}

impl StructObject for Ticket {
    fn get_field(&self, name: &str) -> Option<Value> {
        match name {
            // Convenience shorthand for attributes.
            "attrs" => self.inner.get_attr("attributes").ok(),
            _ => self.inner.get_attr(name).ok()
        }
    }

    fn static_fields(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "source",
            "id",
            "template",
            "draft",
            "attributes",
            "extra"
        ])
    }
}

/// A resource known to FTL, such as an image or page.
/// 
/// Stores relatively little state, with more complex information being 
/// gated behind method calls that lazily compute the result.
#[derive(Debug)]
pub struct Resource {
    pub base: InputFile,
    pub inner: Value,
    pub state: State,
}

impl Resource {
    // Given a path, look in the following places (in order) to try and resolve it to an input file:
    // - If a page is in scope, its directory.
    // - The assets directory.
    // - The content directory.
    // - Attempt to resolve it exactly as provided.
    // Special sigils?:
    // - '.' (only look in the page directory)
    // - '@' (only look in the assets directory)
    // - '~' (only look in the content directory)
    // 
    // permalink
    // bustedlink
    // MIME (full/top/sub)
    // contents
    // base64?
}

impl std::fmt::Display for Resource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl Object for Resource {
    fn kind(&self) -> ObjectKind<'_> {
        ObjectKind::Struct(self)
    }
}

impl StructObject for Resource {
    fn get_field(&self, name: &str) -> Option<Value> {
        self.inner.get_attr(name).ok()
    }

    fn static_fields(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "id",
            "hash",
            "path",
            "extension",
            "contents",
            "inline"
        ])
    }
}

/// Dynamic object wrapper around a database connection pool.
/// Used to enable access to a database from within templates.
#[derive(Debug)]
pub struct DbHandle(Arc<Pool>);

impl DbHandle {
    pub fn new(state: &State) -> Self {
        Self(Arc::clone(&state.db.ro_pool))
    }

    fn query(&self, sql: String, params: Option<Value>) -> MJResult {
        match params {
            Some(params) => self.query_with_params(sql, params),
            None => self.query_core(sql, NO_PARAMS)
        }.map_err(Wrap::wrap)
    }

    fn query_with_params(&self, sql: String, params: Value) -> Result<Value> {
        match params.kind() {
            ValueKind::Seq => {
                let parameters = params
                    .try_iter()?
                    .map(Self::map_value)
                    .enumerate()
                    .try_fold(Vec::new(), |mut acc, (i, param)| -> Result<_> {
                        acc.push((i, param?));
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
            _ => bail!(
                "SQL parameters mut be passed as a sequence or a string-keyed map. (Received {} instead.)",
                params.kind()
            )
        }
    }
    
    fn query_core(&self, sql: String, params: Option<impl Bindable>) -> Result<Value> {
        self.0.get()?.prepare_reader(sql, params)?
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
}

impl std::fmt::Display for DbHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "<Database Handle Object>")
    }
}

impl Object for DbHandle {
    fn call_method(&self, _: &MJState, name: &str, args: &[Value]) -> MJResult {
        match name {
            "query" => {
                let (sql, params) = from_args(args)?;
                self.query(sql, params)
            },
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