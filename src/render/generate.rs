use std::collections::HashMap;

use pulldown_cmark::{html, Options, Parser, Event, Tag};

use super::{Engine, RenderTicket, template};
use crate::prelude::*;

pub fn generate(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    // There are no possible worlds in which the HTML output is smaller
    // than the Markdown input, so a little preallocation can't hurt.
    let mut html_buffer = String::with_capacity(ticket.content.len());
    
    html::push_html(
        &mut html_buffer,
        Parser::new_ext(&ticket.content, Options::all())
    );
    
    ticket.content = html_buffer;
    template::templates(ticket, engine)?;
    Ok(())
}
