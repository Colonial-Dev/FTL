
use once_cell::sync::Lazy;
use regex::Regex;

use crate::prelude::*;

static URL_SCHEMA: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[0-9A-Za-z\-]+:").unwrap());

#[derive(Debug, Clone, Copy)]
pub enum Root {
    Absolute,
    Contents,
    Assets,
}

#[derive(Debug)]
pub enum Link<'a> {
    Relative(&'a str),
    Internal(&'a str, Root),
    External(&'a str),
}

impl<'a> Link<'a> {
    pub fn parse(source: &'a str) -> Result<Self> {
        if URL_SCHEMA.is_match(source) {
            return Ok(Link::External(source))
        }

        match source.chars().next().context("Cannot parse an empty link.")? {
            '@' => {
                let source = source.trim_start_matches("@/");
                let source = Link::Internal(source, Root::Contents);
                Ok(source)
            }
            '$' => {
                let source = source.trim_start_matches("$/");
                let source = Link::Internal(source, Root::Assets);
                Ok(source)
            }
            '/' => {
                let source = Link::Internal(source, Root::Absolute);
                Ok(source)
            }
            _ => Ok(Link::Relative(source))
        }
    }
}