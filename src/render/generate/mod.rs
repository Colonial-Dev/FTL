mod template;

use minijinja::context;
use pulldown_cmark::{Parser, Options, html};

use super::{Engine, Ticket};
use crate::prelude::*;

pub use template::make_environment;

pub fn generate(ticket: &mut Ticket, engine: &Engine) -> Result<()> {
    // There are no possible worlds in which the HTML output is smaller
    // than the Markdown input, so a little preallocation can't hurt.
    let mut html_buffer = String::with_capacity(ticket.content.len());
    
    html::push_html(
        &mut html_buffer,
        Parser::new_ext(&ticket.content, Options::all())
    );
    
    ticket.content = html_buffer;
    eval_page_template(ticket, engine)?;
    Ok(())
}

fn eval_page_template(ticket: &mut Ticket, engine: &Engine) -> Result<()> {
    let Some(name) = &ticket.page.template else {
        warn!(
            "Tried to evaluate template for page {} (\"{}\"), but none was specified.",
            ticket.page.id,
            ticket.page.title
        );

        // This isn't *technically* an error, so we just silently yield.
        return Ok(())
    };

    let Some(template) = engine.get_template(name) else {
        let error = eyre!(
            "Tried to resolve a nonexistent template (\"{}\").",
            name,
        )
        .note("This error occurred because a page had a template specified in its frontmatter that FTL couldn't find at build time.")
        .suggestion("Double check the page's frontmatter for spelling and path mistakes, and make sure the template is where you think it is.");
        bail!(error)
    };

    ticket.content = template.render(context!(page => &ticket.page, markup => &ticket.content))
        .wrap_err("Minijinja encountered an error when rendering a template.")?;
    
    Ok(())
}
