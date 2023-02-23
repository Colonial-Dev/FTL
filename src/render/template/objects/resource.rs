use minijinja::value::*;

use crate::{
    prelude::*, 
    db::InputFile,
};

use super::*;

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
    // permalink
    // bustedlink
    // MIME (full/top/sub)
    // contents/contents_bytes (?)/contents_string
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