use std::sync::Arc;

use minijinja::{context, State, value::Value};
use pulldown_cmark::{html, Options, Parser};
use syntect::{
    highlighting::{Theme, ThemeSet},
    html::highlighted_html_for_string as highlight_html,
    parsing::SyntaxSet,
};

use super::{Bridge, Ticket};
use crate::{
    db::data::{Dependency, Page},
    parse::{delimit::*, shortcode::Shortcode},
    prelude::*,
};

type ExpansionFn = Box<dyn Fn(&State, &mut String, &Page) -> Result<()> + Send + Sync>;

pub fn prepare_pipeline(
    bridge_arc: &Arc<Bridge>,
) -> Result<impl Fn(&State, &Ticket) -> Result<String>> {
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
        INLINE_SC_DELIM.expand(source, |code: Delimited| {
            let code = Shortcode::try_from(code)?;
            eval_shortcode(code, state, page, &bridge)
        })?;

        BLOCK_SC_DELIM.expand(source, |code: Delimited| {
            let code = Shortcode::try_from(code)?;
            eval_shortcode(code, state, page, &bridge)
        })?;

        Ok(())
    };

    let bridge = Arc::clone(bridge_arc);
    let markdown = prepare_markdown_stateful(bridge);

    expansions.push(Box::new(expand_shortcodes));
    expansions.push(Box::new(expand_emoji));
    expansions.push(Box::new(highlight_code));
    expansions.push(markdown);

    Ok(move |state: &State, ticket: &Ticket| -> Result<String> {
        let mut output = ticket.source.read().unwrap().clone();

        for expansion in &expansions {
            expansion(state, &mut output, &ticket.page)?
        }

        Ok(output)
    })
}

pub fn prepare_markdown_stateful(bridge: Arc<Bridge>) -> ExpansionFn {
    use pulldown_cmark::{Event, Tag};
    
    Box::new(move |_: &State, source: &mut String, page: &Page| -> Result<()> {
        let mut buffer = String::with_capacity(source.len());
        let parser = Parser::new_ext(source, Options::all());
        let parser = parser.map(|mut event| match event {
            Event::Start(Tag::Image(_, ref mut target, _)) => {
                let busted = bridge.cachebust_link(target, Some(page), true)?;
                *target = busted.into();
                Ok(event)
            }
            _ => Ok(event)
        });

        let parser: Result<Vec<Event>> = parser.collect();
        
        html::push_html(&mut buffer, parser?.into_iter());
        *source = buffer;
        Ok(())
    })
}

pub fn prepare_markdown_stateless() -> impl Fn(String) -> Value {
    |input: String| -> Value {
        let mut buffer = String::with_capacity(input.len());
        let parser = Parser::new_ext(&input, Options::all());
        html::push_html(&mut buffer, parser);
        Value::from_safe_string(buffer)
    }
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

    let mut syntax_builder = SyntaxSet::load_defaults_newlines().into_builder();
    syntax_builder.add_from_folder("cfg/highlighting/", true)?;

    let mut theme_set = ThemeSet::load_defaults();
    theme_set.add_from_folder("cfg/highlighting/")?;

    let theme = match theme_set.themes.remove(theme_name) {
        Some(theme) => theme,
        None => {
            let err = eyre!("Syntax highlighting theme \"{theme_name}\" does not exist.")
                .note("This error occurred because FTL could not resolve your specified syntax highlighting theme from its name.")
                .suggestion("Make sure your theme name is spelled correctly, and double-check that the corresponding theme file exists.");
            bail!(err)
        }
    };
    let syntaxes = syntax_builder.build();

    debug!("Highlighting syntaxes and themes loaded.");
    Ok((syntaxes, theme))
}

/// Checks that a shortcode of the given name exists, and evaluates it if it does.
fn eval_shortcode(
    code: Shortcode,
    state: &State,
    page: &Page,
    bridge: &Arc<Bridge>,
) -> Result<String> {
    let name = String::from(code.name);
    let name = name + ".html";

    let Ok(template) = state.env().get_template(&name) else {
        let err = eyre!(
            "Page \"{}\" contains a shortcode invoking template \"{}\", which does not exist.",
            page.title,
            code.name
        )
        .note("This error occurred because a shortcode referenced a template that FTL couldn't find at build time.")
        .suggestion("Double check the shortcode invocation for spelling and path mistakes, and make sure the template is where you think it is.");

        bail!(err)
    };

    let dependency = Dependency::Template(name);
    bridge.consumer.send((page.id.clone(), dependency));

    Ok(template.render(context!(code => code))?)
}
