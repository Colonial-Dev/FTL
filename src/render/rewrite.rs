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
    cachebust(ticket, engine)?;
    ticket.content = lazy_load(&ticket.content)?;
    Ok(())
}

fn cachebust(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    let conn = engine.pool.get()?;
    let page_path = Path::new(&ticket.page.path);
    let assets_path = Path::new("src/assets/NULL");

    let deps = RefCell::new(Vec::new());
    let count = Cell::new(0_u32);
    let cachebust = Link::prepare_cachebust(&conn, engine.rev_id)?;
    let cachebust = RefCell::new(cachebust);

    macro_rules! cachebust {
        ($e:literal, $a:literal) => {
            element!($e, |el| {
                let src = el.get_attribute($a);

                if src.is_none() {
                    return Ok(())
                }

                let src = src.unwrap();
                let link = Link::parse(&src)?;
    
                if let Link::External(_) = link {
                    return Ok(())
                }
                
                let mut cb = cachebust.borrow_mut();
                let (busted, id) = match cb(&link, Some(page_path))? {
                    Some(file) => file,
                    None => match cb(&link, Some(assets_path))? {
                        Some(file) => file,
                        None => {
                            let err = eyre!("Could not cachebust file \"{link:?}\"");
                            return Err(err.into())
                        }
                    },
                };
    
                el.set_attribute($a, &busted)?;
                deps.borrow_mut().push(Dependency::Id(id));
                count.set(count.get() + 1);
    
                Ok(())
            })
        };
    }

    let mut output = vec![];
    {
        let mut rewriter = HtmlRewriter::new(
            Settings {
                element_content_handlers: vec![
                    cachebust!("audio", "src"),
                    cachebust!("embed", "src"),
                    cachebust!("img", "src"),
                    cachebust!("input", "src"),
                    cachebust!("script", "src"),
                    cachebust!("track", "src"),
                    cachebust!("video", "src"),
                    cachebust!("link", "href")
                ],
                ..Settings::default()
            },
            |c: &[u8]| output.extend_from_slice(c),
        );
        rewriter.write(ticket.content.as_bytes())?;
    }

    if count.get() > 0 {
        ticket.content = String::from_utf8(output)?;
        ticket.dependencies.append(&mut *deps.borrow_mut());
    }

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
