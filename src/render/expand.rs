use color_eyre::eyre::ContextCompat;
use gh_emoji as emoji;
use once_cell::sync::Lazy;
use syntect::{
    highlighting::{Theme, ThemeSet},
    html::highlighted_html_for_string as highlight_html,
    parsing::SyntaxSet,
};

use super::{Engine, RenderTicket};
use crate::{
    db::data::Dependency,
    parse::{delimit::*, shortcode::*},
    prelude::*,
};

static EMOJI_DELIM: Lazy<Delimiters> =
    Lazy::new(|| Delimiters::new(":", ":", DelimiterKind::Inline));
static CODE_DELIM: Lazy<Delimiters> =
    Lazy::new(|| Delimiters::new("```", "```", DelimiterKind::Multiline));
static INLINE_DELIM: Lazy<Delimiters> =
    Lazy::new(|| Delimiters::new("{% sci ", " %}", DelimiterKind::Inline));
static BLOCK_DELIM: Lazy<Delimiters> =
    Lazy::new(|| Delimiters::new("{% sc ", "{% endsc %}", DelimiterKind::Multiline));

pub fn expand_emoji(ticket: &mut RenderTicket, _engine: &Engine) -> Result<()> {
    EMOJI_DELIM.expand(&mut ticket.content, |tag: Delimited| {
        let name = tag.contents;

        match emoji::get(name) {
            Some(emoji) => Ok(emoji.to_owned()),
            None => Ok(format!(":{name}:")),
        }
    })?;

    Ok(())
}

pub fn highlight_code(ticket: &mut RenderTicket, _engine: &Engine) -> Result<()> {
    let theme_name = Config::global()
        .render
        .highlight_theme
        .as_ref()
        .expect("Syntax highlighting theme should be Some.");

    let (syntaxes, theme) = prepare_highlight(theme_name)?;

    CODE_DELIM.expand(&mut ticket.content, |block: Delimited| {
        let token = block.token.expect("Block token should be Some.");

        if let Some(syntax) = syntaxes.find_syntax_by_token(token) {
            highlight_html(
                block.contents,
                &syntaxes,
                 syntax,
                 &theme
            ).wrap_err("An error occurred in the syntax highlighting engine.")
        }
        else if token == "" {
            let syntax = syntaxes.find_syntax_plain_text();
            highlight_html(
                block.contents,
                &syntaxes,
                 syntax,
                 &theme
            ).wrap_err("An error occurred in the syntax highlighting engine.")
        }
        else {
            let err = eyre!("A codeblock had a language token ('{token}'), but FTL could not find a matching syntax definition.")
            .suggestion("Your codeblock's language token may just be malformed, or it could specify a language not bundled with FTL.");
            bail!(err)
        }
    })?;

    Ok(())
}

fn prepare_highlight(theme_name: &str) -> Result<(SyntaxSet, Theme)> {
    let syntaxes = SyntaxSet::load_defaults_newlines();
    let mut themes = ThemeSet::load_defaults().themes;

    let theme = match themes.remove(theme_name) {
        Some(theme) => theme,
        None => {
            let err = eyre!("Syntax highlighting theme {theme_name} does not exist.")
                .note("This error occurred because FTL could not resolve your specified syntax highlighting theme from its name.")
                .suggestion("Make sure your theme name is spelled correctly, and double-check that the corresponding theme file exists.");
            bail!(err)
        }
    };

    Ok((syntaxes, theme))
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

    ticket
        .dependencies
        .push(Dependency::Template(name.to_owned()));

    Ok(engine.tera.render(name, &ticket.context)?)
}
