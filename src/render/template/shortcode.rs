use std::borrow::Cow;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::render::{RenderTicket, Engine};
use crate::prelude::*;

use super::parser::{Inline, Block};

static INLINE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\{%\s?sci.*?%\}"#).unwrap() );
static BLOCK_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?s)\{%\s?sc.*%\}.*\{%\s?endsc\s?%\}"#).unwrap() );

/// Parses the provided input for inline and block shortcodes, then evaluates them (in that order) using a [`Tera`] instance.
pub fn evaluate_shortcodes<'a>(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    expand_inline(ticket, engine)?;
    expand_block(ticket, engine)?;
    Ok(())
}

fn expand_inline<'a>(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    ticket.content = regexp_expand(ticket.content.clone(), &INLINE_REGEX, |shortcode: &str| {
        let shortcode = Inline::parse(shortcode)?;
        ticket.context.insert("shortcode", &shortcode);
        expand_shortcode(shortcode.name, ticket, engine)
    })?;
    
    Ok(())
}

fn expand_block<'a>(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    ticket.content = regexp_expand(ticket.content.clone(), &BLOCK_REGEX, |shortcode: &str| {
        let shortcode = Block::parse(shortcode)?;
        ticket.context.insert("shortcode", &shortcode);
        expand_shortcode(shortcode.name, ticket, engine)
    })?;
    
    Ok(())
}

fn expand_shortcode(name: &str, ticket: &RenderTicket, engine: &Engine) -> Result<String> {
    if !engine.tera.get_template_names().any(|t| t == name) {
        let err = eyre!(
            "Page {} contains a shortcode invoking template {}, which does not exist.",
            ticket.page.title,
            name
        )
        .note("This error occurred because a shortcode referenced a template that FTL couldn't find at build time.")
        .suggestion("Double check the shortcode invocation for spelling and path mistakes, and make sure the template is where you think it is.");
        
        bail!(err)
    }

    Ok(engine.tera.render(name, &ticket.context)?)
}

fn regexp_expand<'a>(source: Cow<'a, str>, expression: &Lazy<Regex>, mut replacer: impl FnMut(&str) -> Result<String>) -> Result<Cow<'a, str>> {
    let mut matches = expression.find_iter(&source).peekable();
    if matches.peek().is_none() {
        return Ok(source);
    }

    let mut buffer = String::with_capacity(source.len());
    let mut last_match = 0;
    for m in matches {
        let replacement = replacer(m.as_str())?;
        buffer.push_str(&source[last_match..m.start()]);
        buffer.push_str(&replacement);
        last_match = m.end();
    }
    buffer.push_str(&source[last_match..]);

    Ok(Cow::Owned(buffer))
}