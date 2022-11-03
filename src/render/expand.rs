use std::path::PathBuf;

use gh_emoji as emoji;
use pulldown_cmark::{Options, Parser, Event, Tag};
use regex::Regex;
use serde::Deserialize;
use syntect::{
    highlighting::{Theme, ThemeSet},
    html::highlighted_html_for_string as highlight_html,
    parsing::SyntaxSet,
};

use super::{Engine, RenderTicket};
use crate::{
    db::data::{Dependency, InputFile},
    parse::{delimit::*, shortcode::*},
    prelude::*,
};

/// Applies expansions to Markdown source text according to user configuration.
pub fn expand(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    // Warning: order of execution matters here!
    // For example, if code highlighting were to be done before include expansion,
    // includes within code blocks would get mangled and cause parsing errors.
    expand_shortcodes(ticket, engine)?;
    expand_includes(ticket, engine)?;
    expand_emoji(ticket, engine)?;
    highlight_code(ticket, engine)?;
    Ok(())
}

#[derive(Deserialize, Debug)]
struct Include {
    pub path: PathBuf,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
}

impl Include {
    pub fn is_well_formed(&self) -> Result<()> {
        if self.start_at.is_none() && self.end_at.is_some() {
            bail!("Encountered an include block with an ending pattern but no start pattern.")
        };
        Ok(())
    }
}

fn expand_includes(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    let conn = engine.pool.get()?;
    let mut get_file = InputFile::prepare_get_by_path(&conn, engine.rev_id)?;
    let assets_path = {
        let mut path = PathBuf::from(SITE_SRC_DIRECTORY);
        path.push(SITE_ASSET_DIRECTORY);
        path
    };

    TOML_DELIM.expand(&mut ticket.content, |tag: Delimited| {
        let block: Include = toml::from_str(tag.contents)
            .wrap_err("A TOML parsing error occurred while reading an include block.")?;

        block.is_well_formed()?;

        let assets_relative = assets_path.join(&block.path);
        let page_relative = {
            let mut path = PathBuf::from(&ticket.page.path);
            path.pop();
            path.push(&block.path);
            path
        };

        // This is the most concise way I could come up
        // with to short-circuit the file retrieval.
        let file = match get_file(&page_relative)? {
            Some(file) => file,
            None => match get_file(&assets_relative)? {
                Some(file) => file,
                None => bail!("Tried to include a file, but it does not exist."),
            },
        };

        // Non-inlined files have no data to be read.
        if !file.inline {
            bail!("Cannot include non-inlined files.")
        }

        // An empty file isn't necessarily an error.
        let mut contents = match file.contents {
            Some(text) => text,
            None => "".to_string(),
        };

        // At this point we register this page's dependency on the included file.
        ticket.dependencies.push(Dependency::Id(file.id));

        // Unbounded includes just paste the entire referenced file in,
        // ala C's #include directive.
        if block.start_at.is_none() {
            return Ok(contents);
        }

        if let Some(start_at) = block.start_at {
            let start_regex = make_regex(start_at)?;
            let start_idx = start_regex
                .find_iter(&contents)
                .next()
                .context("Could not match opening delimiter when including file.")?
                .start();

            contents.drain(..start_idx);
        }

        if let Some(end_at) = block.end_at {
            let end_regex = make_regex(end_at)?;
            let end_idx = end_regex
                .find_iter(&contents)
                .next()
                .context("Could not match closing delimiter when including file.")?
                .end();

            contents.drain(end_idx..);
        }

        Ok(contents)
    })?;

    Ok(())
}

#[inline]
fn make_regex(expression: String) -> Result<Regex> {
    let expression = regex::escape(&expression);
    let regex = Regex::new(&expression)
        .wrap_err("Error while compiling include block regular expression.")?;
    Ok(regex)
}

fn expand_emoji(ticket: &mut RenderTicket, _engine: &Engine) -> Result<()> {
    EMOJI_DELIM.expand(&mut ticket.content, |tag: Delimited| {
        let name = tag.contents;

        match emoji::get(name) {
            Some(emoji) => Ok(emoji.to_owned()),
            None => Ok(format!(":{name}:")),
        }
    })?;

    Ok(())
}

fn highlight_code(ticket: &mut RenderTicket, _engine: &Engine) -> Result<()> {
    let theme_name = Config::global()
        .render
        .highlight_theme
        .as_ref()
        .expect("Syntax highlighting theme should be Some.");

    let (syntaxes, theme) = prepare_highlight(theme_name)?;

    CODE_DELIM.expand(&mut ticket.content, |block: Delimited| {
        let token = block.token.expect("Block token should be Some.");

        let syntax = if token.is_empty() {
            syntaxes.find_syntax_plain_text()
        }
        else {
            match syntaxes.find_syntax_by_token(token) {
                Some(syntax) => syntax,
                None => {
                    let err = eyre!("A codeblock had a language token ('{token}'), but FTL could not find a matching syntax definition.")
                    .suggestion("Your codeblock's language token may just be malformed, or it could specify a language not bundled with FTL.");
                    bail!(err)
                }
            }
        };

        highlight_html(
            block.contents,
            &syntaxes,
             syntax,
             &theme
        ).wrap_err("An error occurred in the syntax highlighting engine.")
    })?;

    Ok(())
}

fn prepare_highlight(theme_name: &str) -> Result<(SyntaxSet, Theme)> {
    let syntaxes = SyntaxSet::load_defaults_newlines();
    let mut themes = ThemeSet::load_defaults().themes;
    // TODO: load user-provided syntaxes and themes.
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

fn expand_shortcodes(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
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

/// Adds deep links to all Markdown headings (where they are not already present.)
fn link_anchors(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    Parser::new_ext(&ticket.content, Options::all())
        .into_offset_iter();

    todo!()
}
