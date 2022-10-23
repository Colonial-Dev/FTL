use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use lol_html::{element, HtmlRewriter, Settings};
use rusqlite::params;

use super::{Engine, RenderTicket};
use crate::{
    db::{data::Page, Connection},
    prelude::*,
};

pub fn rewrite<'a>(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    let conn = engine.pool.get()?;

    ticket.content = cachebust_img(&ticket.content, &ticket.page, &conn, engine.rev_id)?;
    ticket.content = lazy_load(&ticket.content)?;

    Ok(())
}

/// Cachebusts `<img>` tags with a relative `src` attribute.
/// This turns, say, `bar.png` into `content/pages/foo/bar.ffde185cab76e0a6.png`.
///
/// This function will query for the image's ID in two places:
/// - The directory of the page itself.
///   - Example: A page is located at `src/content/articles/article-01/index.md`.
///   - If the image path is `cover.png`, we query by `src/content/articles/article-01/cover.png`.
/// - The assets directory.
///   - If the image path is `background.png`, we query by `src/assets/background.png`.
///
/// A match relative to the page takes priority over a match in the assets directory.
/// If no match is found, we leave the tag untouched.
fn cachebust_img<'a>(
    hypertext: &'a str,
    page: &Page,
    conn: &Connection,
    rev_id: &str,
) -> Result<String> {
    let mut cachebust = prepare_cachebust(conn, page, rev_id)?;
    let mut output = vec![];
    {
        let mut rewriter = HtmlRewriter::new(
            Settings {
                element_content_handlers: vec![
                    // Cachebust <img> tags with a relative src attribute.
                    element!("img", |el| {
                        let src = el.get_attribute("src").unwrap();

                        // First character being / means the src attribute isn't relative,
                        // so we skip rewriting this tag.
                        if let Some('/') = src.chars().next() {
                            return Ok(());
                        }

                        let asset = PathBuf::from(&src);
                        let page_relative =
                            PathBuf::from(&page.path).parent().unwrap().join(&asset);
                        let assets_relative = PathBuf::from(SITE_SRC_DIRECTORY)
                            .join(SITE_ASSET_DIRECTORY)
                            .join(&asset);

                        //if cachebust(&page_relative, el) { return Ok(()) }
                        //if cachebust(&assets_relative, el) { return Ok(()) }
                        return Ok(());
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

/// Prepares and returns a closure that wraps cachebusting and ID caching logic.
/// Returns `true` if the element was cachebusted successfully; false otherwise.
fn prepare_cachebust<'a>(
    conn: &'a Connection,
    page: &'a Page,
    rev_id: &'a str,
) -> Result<impl FnMut(&Path) -> Result<Option<(String, String)>> + 'a> {
    let mut try_query_id = conn.prepare(
        "
        SELECT id FROM input_files
        WHERE path = ?1
        AND EXISTS (
            SELECT 1 FROM revision_files
            WHERE input_files.id = revision_files.id
            AND revision = ?2
        )
    ",
    )?;

    let closure = move |path: &Path| -> Result<Option<(String, String)>> {
        let maybe_id: Option<String> =
            try_query_id.query_row(params![path.to_string_lossy(), rev_id], |row| row.get(0))?;

        if let None = maybe_id {
            return Ok(None);
        }

        let id = maybe_id.unwrap();

        if let Some(name) = path.file_name() {
            let name = name.to_string_lossy();
            let busted = name.replace(".", &format!(".{}.", id));

            let busted = path.to_string_lossy().replace(&*name, &busted);

            Ok(Some((busted, id)))
        } else {
            let err = eyre!("");

            bail!(err)
        }
    };

    Ok(closure)
}

/// Rewrites the `loading` attribute of all `<img>` and `<video>` tags to be `lazy`.
fn lazy_load<'a>(hypertext: &'a str) -> Result<String> {
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
