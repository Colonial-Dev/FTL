use std::path::{Path, PathBuf};

use minijinja::{
    value::*,
    State as MJState
};
use once_cell::sync::Lazy;

use crate::{
    prelude::*, 
    db::InputFile,
};

use super::*;

static ASSETS_PATH: Lazy<String> = Lazy::new(|| {
    format!("{SITE_SRC_PATH}{SITE_ASSET_PATH}")
});

static CONTENT_PATH: Lazy<String> = Lazy::new(|| {
    format!("{SITE_SRC_PATH}{SITE_CONTENT_PATH}")
});

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
    fn new_from_path(ftl_state: &State, mj_state: &MJState, path: String) -> Result<Self> {
        let mut lookup_targets = Vec::with_capacity(4);
        let conn = ftl_state.db.get_ro()?;
        let rev_id = ftl_state.get_rev();

        if let Some(value) = mj_state.lookup("page") {
            if let Some(ticket) = value.downcast_object_ref::<Ticket>() {
                lookup_targets.push(
                    Path::new(&ticket.page.path).join(&path)
                )
            }
        }

        lookup_targets.extend([
            Path::new(&*ASSETS_PATH).join(&path),
            Path::new(&*CONTENT_PATH).join(&path),
            PathBuf::from(&path)
        ].into_iter());

        let query = "
            SELECT input_files.id, path FROM input_files
            JOIN revision_files ON revision_files.id = input_files.id
            WHERE revision_files.revision = ?1
            AND input_files.path = ?2
        ";

        todo!()
    }
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