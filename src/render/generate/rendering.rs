use std::sync::Arc;

use minijinja::{State, context};
use pulldown_cmark::{html, Parser, Options};
use syntect::{
    highlighting::{Theme, ThemeSet},
    html::highlighted_html_for_string as highlight_html,
    parsing::SyntaxSet,
};
use super::{DatabaseBridge, Ticket};

use crate::{prelude::*, parse::{delimit::*, shortcode::Shortcode}, db::data::{Page, Dependency}};

type ExpansionFn = Box<dyn Fn(&State, &mut String, &Page) -> Result<()> + Send + Sync>;

pub fn prepare_renderer(bridge_arc: &Arc<DatabaseBridge>) -> Result<impl Fn(&State, &Ticket) -> Result<String>> {
    let mut expansions: Vec<ExpansionFn> = Vec::new();
    
    let (syntaxes, theme) = prepare_highlighter()?;
    let highlight_code = move |_: &State, source: &mut String, _: &Page| -> Result<()> {
        CODE_DELIM.expand(source, |block: Delimited| {
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
        })
    };

    let bridge = Arc::clone(bridge_arc);
    let expand_shortcodes = move |state: &State, source: &mut String, page: &Page| -> Result<()> {
        INLINE_DELIM.expand(source, |code: Delimited| {
            let code = Shortcode::try_from(code)?;
            eval_shortcode(code, state, page, &bridge)
        })?;

        BLOCK_DELIM.expand(source, |code: Delimited| {
            let code = Shortcode::try_from(code)?;
            eval_shortcode(code, state, page, &bridge)
        })?;
        
        Ok(())
    };

    let markdown = move |_: &State, source: &mut String, _: &Page| -> Result<()> {
        // There are no possible worlds in which the HTML output is smaller
        // than the Markdown input, so a little preallocation can't hurt.
        let mut html_buffer = String::with_capacity(source.len());
        
        html::push_html(
            &mut html_buffer,
            Parser::new_ext(source, Options::all())
        );
        
        *source = html_buffer;
        Ok(())
    };

    expansions.push(Box::new(expand_shortcodes));
    expansions.push(Box::new(expand_emoji));
    expansions.push(Box::new(highlight_code));
    expansions.push(Box::new(markdown));

    Ok(move |state: &State, ticket: &Ticket| -> Result<String> {
        let mut output = ticket.source.clone();
        for expansion in &expansions {
            expansion(state, &mut output, &ticket.page)?
        }
        Ok(output)
    })
}

fn expand_emoji(_: &State, source: &mut String, _: &Page) -> Result<()> {
    EMOJI_DELIM.expand(source, |tag: Delimited| {
        let name = tag.contents;

        match gh_emoji::get(name) {
            Some(emoji) => Ok(emoji.to_owned()),
            None => Ok(format!(":{name}:")),
        }
    })
}

fn prepare_highlighter() -> Result<(SyntaxSet, Theme)> {
    let theme_name = Config::global()
        .render
        .highlight_theme
        .as_ref()
        .expect("Syntax highlighting theme should be Some.");
    
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

/// Checks that a shortcode of the given name exists, and evaluates it if it does.
fn eval_shortcode(code: Shortcode, state: &State, page: &Page, bridge: &Arc<DatabaseBridge>) -> Result<String> {
    let name = String::from(code.name);
    let name = name + ".html";
    let Ok(template) = state.env().get_template(&name) else {
        let err = eyre!(
            "Page {} contains a shortcode invoking template {}, which does not exist.",
            page.title,
            code.name
        )
        .note("This error occurred because a shortcode referenced a template that FTL couldn't find at build time.")
        .suggestion("Double check the shortcode invocation for spelling and path mistakes, and make sure the template is where you think it is.");

        bail!(err)
    };
    
    // Yes, this cloning is inefficient, but if it turns out to be an issue
    // then we can apply a clever optimization like putting them behind an Arc.
    let dependency = Dependency::Template(code.name.to_owned());
    bridge.consumer.send((page.id.clone(), dependency));

    Ok(template.render(context!(code => code))?)
}

fn eval_template(state: &State, source: &mut String, page: &Page) -> Result<()> {
    let Some(name) = &page.template else {
        warn!(
            "Tried to evaluate template for page {} (\"{}\"), but none was specified.",
            page.id,
            page.title
        );

        // This isn't *technically* an error, so we just silently yield.
        return Ok(())
    };

    let Ok(template) = state.env().get_template(name) else {
        let error = eyre!(
            "Tried to resolve a nonexistent template (\"{}\").",
            name,
        )
        .note("This error occurred because a page had a template specified in its frontmatter that FTL couldn't find at build time.")
        .suggestion("Double check the page's frontmatter for spelling and path mistakes, and make sure the template is where you think it is.");
        bail!(error)
    };

    *source = template.render(context!(page => &page, markup => &source))
        .wrap_err("Minijinja encountered an error when rendering a template.")?;
    
    Ok(())
}