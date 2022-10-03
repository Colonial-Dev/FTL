use std::borrow::Cow;

use pulldown_cmark::{Parser, Event, Options, html};

/// Initializes a [`Parser`] instance with the given Markdown input and all available extensions.
pub fn init<'a>(input: &'a str) -> Parser<'a, 'a> {
    let options = Options::all();
    Parser::new_ext(input, options)
}

/// Consume a [`Parser`] instance, buffering the HTML output into a final [`String`].
pub fn write<'a>(parser: Parser) -> Cow<'a, str> {
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    Cow::Owned(html_output)
}

pub fn map<'a>(parser: Parser<'a, 'a>) -> Parser<'a, 'a> {
    parser
    // Syntax highlighting
    // Anchors/deep linking
    // ...internal linking?
}