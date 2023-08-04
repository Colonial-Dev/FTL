use minijinja::value::*;

use super::*;
use crate::db::{InputFile, Insertable};
use crate::prelude::*;

/// A resource known to FTL, such as an image or page. Acquired inside the engine
/// through the [`DbHandle::get_resource`] method.
///
/// Stores relatively little data, with more complex information being
/// gated behind method calls that lazily compute the result.
#[derive(Debug)]
pub struct Resource {
    pub base: InputFile,
    pub inner: Value,
    pub state: Context,
}

impl Resource {
    fn permalink(&self, state: &MJState) {}

    fn cachebusted(&self, state: &MJState) {}
    // permalink (the canonical route to the asset - excludes redirects)
    // cachebusted (only for non-inline, returns none or error for inline?)
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
        Some(InputFile::COLUMN_NAMES)
    }
}
