use std::{
    collections::HashMap,
    sync::Arc,
};

use crossbeam::queue::SegQueue;
use minijinja::{
    value::*,
    State as MJState
};
use serde::Serialize;

use crate::{
    prelude::*, 
    db::{
        Page, Relation,
        Queryable, Statement, StatementExt
    },
    render::Renderer
};

use super::error::{
    MJResult,
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
    pub original: (String, Page),
    pub renderer: Arc<Renderer>,
    inner: Value,
}

#[derive(Serialize)]
struct SerTicket<'a> {
    source: &'a str,
    #[serde(flatten)]
    page: &'a Page,
}

impl Ticket {
    pub fn new(renderer: &Arc<Renderer>, page: Page, source: &str) -> Self {
        // Slice off the page's frontmatter.
        let source = source[page.offset as usize..].to_string();
        let inner = Value::from_serializable(&SerTicket {
            source: &source,
            page: &page
        });

        Self {
            metadata: SegQueue::new(),
            original: (source, page),
            renderer: Arc::clone(renderer),
            inner,
        }
    }

    fn render(&self) -> Result<Value> {
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
    
    fn call_method(&self, state: &MJState, name: &str, args: &[Value]) -> MJResult {
        match name {
            "render" => self.render(),
            _ => Err(eyre!("object has no method named {name}"))
        }.map_err(Wrap::wrap)
    }
}

impl StructObject for Ticket {
    fn get_field(&self, name: &str) -> Option<Value> {
        self.inner.get_attr(name).ok()
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
/// Stores relatively little state when first created, with more complex information being 
/// gated behind method calls that lazily compute and cache the result.
#[derive(Debug)]
pub struct Resource {
    id: String,
    inline: bool,
    contents: Option<String>
}

/// Minijinja dynamic object wrapper around a [`HashMap<String, Value>`], necessary to obey the orphan rule.
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
        use sqlite::Value as SQLValue;
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