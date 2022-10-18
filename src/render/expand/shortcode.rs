use std::borrow::Cow;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::db::data::Dependency;
use crate::render::{RenderTicket, Engine};
use crate::parse::shortcode::{Block, Inline};
use crate::prelude::*;

use super::regexp_expand;

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
        ticket.context.insert("code", &shortcode);
        expand_shortcode(shortcode.name, ticket, engine)
    })?;
    
    Ok(())
}

fn expand_block<'a>(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    ticket.content = regexp_expand(ticket.content.clone(), &BLOCK_REGEX, |shortcode: &str| {
        let shortcode = Block::parse(shortcode)?;
        ticket.context.insert("code", &shortcode);
        expand_shortcode(shortcode.name, ticket, engine)
    })?;
    
    Ok(())
}

/// Checks that a shortcode of the given name exists, and evaluates it if it does.
fn expand_shortcode(name: &str, ticket: &mut RenderTicket, engine: &Engine) -> Result<String> {
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

    ticket.dependencies.push(
        Dependency::Template(name.to_owned())
    );
    
    Ok(engine.tera.render(name, &ticket.context)?)
}