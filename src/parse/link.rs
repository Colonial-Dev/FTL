use once_cell::sync::Lazy;
use regex::Regex;

use crate::prelude::*;

/// Matches a case-insensitive identifier followed by a colon (that is, a URL schema like `http:`.)
static URL_SCHEMA: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[0-9A-Za-z\-]+:").unwrap());

/// Unit enum representing the possible implied roots of an internal path/link.
#[derive(Debug, Clone, Copy)]
pub enum Root {
    /// Source roots start with `$/`, and are relative to the top-level `src` directory.
    /// Example: `$/assets/logo.png` will point to `src/assets/logo.png`.
    Source,
    /// Asset roots start with `@/`, and are relative to the `assets` directory.
    /// Example: `@/logo.png` will point to `src/assets/logo.png`.
    Assets,
    /// Content roots start with `~/` and are relative to the `contents` directory.
    /// Example: `~/articles/example/index.md` will point to `src/content/articles/example/index.md`.
    Contents,
}

/// Enum representing the different possible types of links.
#[derive(Debug)]
pub enum Link<'a> {
    Relative(&'a str),
    Internal(String, Root),
    External(&'a str),
}

impl<'a> Link<'a> {
    pub fn parse(source: &'a str) -> Result<Self> {
        if URL_SCHEMA.is_match(source) {
            return Ok(Link::External(source));
        }

        match source
            .chars()
            .next()
            .context("Cannot parse an empty link.")?
        {
            '@' => {
                let source = source.trim_start_matches("@/");
                let source = SITE_ASSET_DIRECTORY.to_string() + source;
                let source = Link::Internal(source, Root::Assets);
                Ok(source)
            }
            '~' => {
                let source = source.trim_start_matches("~/");
                let source = SITE_CONTENT_DIRECTORY.to_string() + source;
                let source = Link::Internal(source, Root::Contents);
                Ok(source)
            }
            '$' => {
                let source = source.trim_start_matches("$/");
                let source = SITE_SRC_DIRECTORY.to_string() + source;
                let source = Link::Internal(source, Root::Source);
                Ok(source)
            }
            _ => Ok(Link::Relative(source)),
        }
    }
}
