
use gh_emoji as emoji;
use once_cell::sync::Lazy;

use crate::db::data::Dependency;
use crate::parse::{delimit::*, shortcode::*};
use crate::prelude::*;

use super::{RenderTicket, Engine};

static EMOJI_DELIM: Lazy<Delimiters> = Lazy::new(|| Delimiters::new(":", ":", DelimiterKind::Inline) );
static CODE_DELIM: Lazy<Delimiters> = Lazy::new(|| Delimiters::new("```", "```", DelimiterKind::Multiline) );
static INLINE_DELIM: Lazy<Delimiters> = Lazy::new(|| Delimiters::new("{% sci ", " %}", DelimiterKind::Inline) );
static BLOCK_DELIM: Lazy<Delimiters> = Lazy::new(|| Delimiters::new("{% sc ", "{% endsc %}", DelimiterKind::Multiline) );

pub fn expand_emoji(ticket: &mut RenderTicket, _engine: &Engine) -> Result<()> {
    EMOJI_DELIM.expand(&mut ticket.content, |tag: Delimited| {
        let name = tag.contents;

        match emoji::get(name) {
            Some(emoji) => Ok(emoji.to_owned()),
            None => Ok(format!(":{name}:"))
        }
    })?;

    Ok(())
}

pub fn highlight_code(ticket: &mut RenderTicket, _engine: &Engine) -> Result<()> {
    CODE_DELIM.expand(&mut ticket.content, |block: Delimited| {
        //Ok(engine.highlight(block)?)
        Ok("a".to_string())
    })?;

    Ok(())
}

pub fn expand_shortcodes(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    // Note: the borrow checker can't distinguish between multiple mutable
    // borrows to disjoint fields within the same struct. 
    // 
    // Unfortunately, we need to mutate both the content and context fields
    // of ticket, which violates this limitation. To get around this, we
    // take the content field from the ticket, operate on it, then
    // swap it back into the ticket when we're done.
    //
    // Technically this could be avoided via unsafe, but... no

    let mut content = std::mem::take(&mut ticket.content);
    INLINE_DELIM.expand(&mut content, |code: Delimited| {
        let code = Shortcode::try_from(code)?;
        ticket.context.insert("code", &code);
        render_shortcode(code.name, ticket, engine)
    })?;

    BLOCK_DELIM.expand(&mut content, |code: Delimited| {
        let code = Shortcode::try_from(code)?;
        ticket.context.insert("code", &code);
        render_shortcode(code.name, ticket, engine)
    })?;
    let _ = std::mem::replace(&mut ticket.content, content);

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