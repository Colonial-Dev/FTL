use std::collections::HashMap;

use pulldown_cmark::{html, Options, Parser, Event, Tag};

use super::{Engine, RenderTicket, template};
use crate::prelude::*;

pub fn generate(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    let parser = init(&ticket.content);

    template::templates(ticket, engine)?;

    Ok(())
}

fn link_headings<'a>(parser: Parser<'a, 'a>) -> impl Iterator<Item=Event<'a>> {
    let mut headings: HashMap<&str, (&str, usize)> = HashMap::new();
    parser.map(|event| match event {
        _ => event
    })
}

/// Initializes a [`Parser`] instance with the given Markdown input and all available extensions.
fn init(input: &'_ str) -> Parser<'_, '_> {
    let options = Options::all();
    Parser::new_ext(input, options)
}

/// Consume a [`Parser`] instance, buffering the HTML output into a final [`String`].
fn write(parser: Parser) -> String {
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}
