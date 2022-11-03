use lol_html::{element, HtmlRewriter, Settings};

use super::{Engine, RenderTicket};
use crate::prelude::*;

pub fn rewrite(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    lazy_load(ticket)?;
    link_targets(ticket)?;
    Ok(())
}

/// Rewrites the `loading` attribute of all `<img>` and `<video>` tags to be `lazy`.
fn lazy_load(ticket: &mut RenderTicket) -> Result<()> {
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
        rewriter.write(ticket.content.as_bytes())?;
    }
    ticket.content = String::from_utf8(output)?;
    Ok(())
}

const NO_OPENER: &str = "noopener ";
const NO_FOLLOW: &str = "nofollow ";
const NO_REFERRER: &str = "noreferrer";

/// Based on user configuration, rewrites the `rel` and `target` attributes of `<a>` tags.
/// - If `external_links_new_tab` is true, then `rel="noopener"` and `target="_blank"`.
/// - If `external_links_no_follow` is true, then `rel="nofollow"`.
/// - If `external_links_no_referrer` is true, then `rel="noreferrer"`.
fn link_targets(ticket: &mut RenderTicket) -> Result<()> {
    let config = Config::global();
    let mut output = vec![];
    {
        let mut rewriter = HtmlRewriter::new(
            Settings {
                element_content_handlers: vec![element!("a", |el| {
                    let mut rel_attribute = String::new();
                    
                    if config.build.external_links_new_tab {
                        el.set_attribute("target", "_blank").unwrap();
                        rel_attribute.push_str(NO_OPENER);
                    }
                    
                    if config.build.external_links_no_follow {
                        rel_attribute.push_str(NO_FOLLOW);
                    }

                    if config.build.external_links_no_referrer {
                        rel_attribute.push_str(NO_REFERRER);
                    }

                    if !rel_attribute.is_empty() {
                        el.set_attribute("rel", &rel_attribute).unwrap()
                    }
                    
                    Ok(()) 
                })],
                ..Settings::default()
            },
            |c: &[u8]| output.extend_from_slice(c),
        );
        rewriter.write(ticket.content.as_bytes())?;
    }
    ticket.content = String::from_utf8(output)?;
    Ok(())
}
