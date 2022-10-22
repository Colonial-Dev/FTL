use pulldown_cmark::{html, Options, Parser};

use super::{Engine, RenderTicket};

pub fn process<'a>(ticket: &mut RenderTicket, _engine: &Engine) {
    let parser = init(&ticket.content);
    let parser = map(parser);
    ticket.content = write(parser);
}

/// Initializes a [`Parser`] instance with the given Markdown input and all available extensions.
fn init<'a>(input: &'a str) -> Parser<'a, 'a> {
    let options = Options::all();
    Parser::new_ext(input, options)
}

/// Consume a [`Parser`] instance, buffering the HTML output into a final [`String`].
fn write<'a>(parser: Parser) -> String {
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

fn map<'a>(parser: Parser<'a, 'a>) -> Parser<'a, 'a> {
    parser
    // Anchors/deep linking
    // ...internal linking?
}
