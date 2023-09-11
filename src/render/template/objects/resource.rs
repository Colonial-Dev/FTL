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
    pub ctx: Context,
    pub rev_id: RevisionID,
}

impl Resource {
    fn route(&self) -> Result<Value> {
        let conn = self.ctx.db.get_ro()?;

        let query = "
            SELECT route FROM routes
            WHERE id = ?1
            AND revision = ?2
            AND kind IN (1, 3, 4)
        ";

        let parameters = &[
            (1_usize, &*self.base.id),
            (2_usize, self.rev_id.as_inner()),
        ];

        let mut query = conn.prepare_reader::<String, _, _>(
            query,
            parameters.as_slice().into()
        )?;

        Ok(match query.next() {
            Some(route) => Value::from(route?),
            None => Value::from(())
        })
    }

    fn cachebusted(&self) -> MJResult {
        // Only non-inline files have cachebusted routes.
        if self.base.inline {
            return Ok(Value::from(()));
        }
        
        Ok(Value::from(self.base.cachebust()))
    }
    
    fn contents_bytes(&self) -> Result<Value> {
        Ok(match &self.base.contents {
            Some(contents) => Value::from(contents.as_bytes()),
            None => {
                let path = format!("{SITE_CACHE_PATH}{}", self.base.id);
                Value::from(std::fs::read(path)?)
            },
        })    
    }

    fn contents_string(&self) -> MJResult {
        Ok(match &self.base.contents {
            Some(contents) => Value::from(contents.to_owned()),
            None => Value::from(()),
        })
    }

    // Returns estimated time in minutes, rounded up. (May update to use a better API later.)
    fn time_to_read(&self) -> MJResult {
        Ok(match &self.base.contents {
            Some(contents) => {
                let words = contents.split_whitespace().count();
                // Average adult reads at ~240 WPM (source: first hit on Google)
                let minutes = (words as f64 / 240.0).ceil() as i64;
                Value::from(minutes)
            }
            None => Value::from(0),
        })
    }

    // Naive word count using whitespace splitting.
    fn word_count(&self) -> MJResult {
        Ok(match &self.base.contents {
            Some(contents) => Value::from(contents.split_whitespace().count()),
            None => Value::from(0),
        })
    }

    fn is_page(&self) -> MJResult {
        let value = Value::from(self.base.inline);
        Ok(value)
    }

    fn is_asset(&self) -> MJResult {
        let value = Value::from(!self.base.inline);
        Ok(value)    
    }
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

    fn call_method(&self, _: &State, name: &str, _: &[Value]) -> MJResult {
        match name {
            "route" => self.route().map_err(Wrap::wrap),
            "cachebusted" => self.cachebusted(),
            "contents_bytes" => self.contents_bytes().map_err(Wrap::wrap),
            "contents_string" => self.contents_string(),
            "word_count" => self.word_count(),
            "time_to_read" => self.time_to_read(),
            "is_page" => self.is_page(),
            "is_asset" => self.is_asset(),
            _ => Err(MJError::new(
                MJErrorKind::UnknownMethod,
                format!("object has no method named {name}"),
            )),
        }
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
