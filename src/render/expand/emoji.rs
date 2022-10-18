use gh_emoji as emoji;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::render::{RenderTicket, Engine};
use crate::prelude::*;

use super::regexp_expand;

static EMOJI_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#":[a-z1238+-][a-z0-9_-]*:"#).unwrap() );

pub fn expand_emoji(ticket: &mut RenderTicket, _engine: &Engine) -> Result<()> {
    ticket.content = regexp_expand(ticket.content.clone(), &EMOJI_REGEX, |tag: &str| {
        let name = tag
            .trim()
            .trim_start_matches(":")
            .trim_end_matches(":");
            
        match emoji::get(name) {
            Some(emoji) => Ok(emoji.to_owned()),
            None => Ok(tag.to_owned())
        }
    })?;

    Ok(())
}