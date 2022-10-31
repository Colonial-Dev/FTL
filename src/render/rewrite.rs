use std::{
    borrow::Cow,
    path::Path, cell::{RefCell, Cell}
};

use lol_html::{element, HtmlRewriter, Settings};

use super::{Engine, RenderTicket};
use crate::{
    db::data::Dependency,
    parse::link::Link,
    prelude::*,
};



pub fn rewrite(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    ticket.content = lazy_load(&ticket.content)?;
    Ok(())
}

/// Rewrites the `loading` attribute of all `<img>` and `<video>` tags to be `lazy`.
fn lazy_load(hypertext: &str) -> Result<String> {
    let mut output = vec![];
    {
        let mut rewriter = HtmlRewriter::new(
            Settings {
                element_content_handlers: vec![
                    // Unwrap justification: hardcoded values should never trip any of lol_html's error conditions.
                    element!("img", |el| {
                        el.set_attribute("loading", "lazy").unwrap();
                        Ok(())
                    }),
                    element!("video", |el| {
                        el.set_attribute("loading", "lazy").unwrap();
                        Ok(())
                    }),
                ],
                ..Settings::default()
            },
            |c: &[u8]| output.extend_from_slice(c),
        );
        rewriter.write(hypertext.as_bytes())?;
    }
    let hypertext = String::from_utf8(output)?;
    Ok(hypertext)
}

/// Based on user configuration, rewrites the `rel` and `target` attributes of `<a>` tags.
/// - If `external_links_new_tab` is true, then `rel="noopener"` and `target="_blank"`.
/// - If `external_links_no_follow` is true, then `rel="nofollow"`.
/// - If `external_links_no_referrer` is true, then `rel="noreferrer"`.
fn link_targets<'a>(hypertext: Cow<'a, str>) -> Result<Cow<'a, str>> {
    let config = Config::global();
    let mut output = vec![];
    {
        let mut rewriter = HtmlRewriter::new(
            Settings {
                element_content_handlers: vec![element!("a", |el| { Ok(()) })],
                ..Settings::default()
            },
            |c: &[u8]| output.extend_from_slice(c),
        );
        rewriter.write(hypertext.as_bytes())?;
    }
    let hypertext = String::from_utf8(output)?;
    Ok(Cow::Owned(hypertext))
}
