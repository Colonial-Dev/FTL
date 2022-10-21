
use std::borrow::Cow;

use gh_emoji as emoji;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::db::data::Dependency;
use crate::parse::{Ranged, delimit::*, shortcode::*};
use crate::prelude::*;

use super::{RenderTicket, Engine};

static EMOJI_DELIM: Lazy<Delimiters> = Lazy::new(|| Delimiters::new(":", ":", DelimiterKind::Inline) );
static CODE_DELIM: Lazy<Delimiters> = Lazy::new(|| Delimiters::new("```", "```", DelimiterKind::Multiline) );
static INLINE_DELIM: Lazy<Delimiters> = Lazy::new(|| Delimiters::new("{% sci ", " %}", DelimiterKind::Inline) );
static BLOCK_DELIM: Lazy<Delimiters> = Lazy::new(|| Delimiters::new("{% sc ", "{% endsc %}", DelimiterKind::Multiline) );

pub fn expand_emoji(ticket: &mut RenderTicket, _engine: &Engine) -> Result<()> {
    let emoji = EMOJI_DELIM.parse_from(&ticket.content);

    ticket.content = ranged_expand(ticket.content.clone(), emoji, |tag: Delimited| {
        let name = tag.contents;

        match emoji::get(name) {
            Some(emoji) => Ok(emoji.to_owned()),
            None => Ok(format!(":{name}:"))
        }
    })?;

    Ok(())
}

pub fn highlight_code(ticket: &mut RenderTicket, _engine: &Engine) -> Result<()> {
    let codeblocks = CODE_DELIM.parse_from(&ticket.content);

    ticket.content = ranged_expand(ticket.content.clone(), codeblocks, |block: Delimited| {
        //Ok(engine.highlight(block)?)
        Ok("a".to_string())
    })?;

    Ok(())
}

pub fn expand_shortcodes(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    // Workaround to avoid conflicting references.
    let source = ticket.content.clone();
    let inline_codes = INLINE_DELIM.parse_into::<Shortcode>(&source)?;
    
    ticket.content = ranged_expand(ticket.content.clone(), inline_codes, |code: Shortcode| {
        ticket.context.insert("code", &code);
        render_shortcode(code.name, ticket, engine)
    })?;

    let source = ticket.content.clone();
    let block_codes = BLOCK_DELIM.parse_into::<Shortcode>(&source)?;

    ticket.content = ranged_expand(ticket.content.clone(), block_codes, |code: Shortcode| {
        ticket.context.insert("code", &code);
        render_shortcode(code.name, ticket, engine)
    })?;

    Ok(())
}

/// Checks that a shortcode of the given name exists, and evaluates it if it does.
fn render_shortcode(name: &str, ticket: &mut RenderTicket, engine: &Engine) -> Result<String> {
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

fn ranged_expand<'a, T: Ranged>(source: Cow<'a, str>, targets: Vec<T>, mut replacer: impl FnMut(T) -> Result<String>) -> Result<Cow<'a, str>> {
    if targets.is_empty() {
        return Ok(source);
    }

    let mut buffer = String::with_capacity(source.len());
    let mut last_match = 0;
    for target in targets {
        let range = target.range();
        let replacement = replacer(target)?;

        buffer.push_str(&source[last_match..range.start]);
        buffer.push_str(&replacement);

        last_match = range.end;
    }
    buffer.push_str(&source[last_match..]);
    buffer.shrink_to_fit();

    Ok(Cow::Owned(buffer))
}