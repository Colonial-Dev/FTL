use std::borrow::Cow;
use std::path::{PathBuf, Path};

use anyhow::Result;
use lol_html::html_content::Element;
use lol_html::{element, HtmlRewriter, Settings};
use rusqlite::params;

use crate::db::Connection;
use crate::db::data::Page;
use crate::share::*;

pub fn rewrite<'a>(conn: &Connection, page: &Page, hypertext: Cow<'a, str>, rev_id: &str) -> Result<Cow<'a, str>> {
    let hypertext = cachebust_img(hypertext, page, conn, rev_id)?;
    let hypertext = lazy_load(hypertext)?;
    Ok(hypertext)
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
fn cachebust_img<'a>(hypertext: Cow<'a, str>, page: &Page, conn: &Connection, rev_id: &str) -> Result<Cow<'a, str>> {
    let mut cachebust = prepare_cachebust(conn, page, rev_id)?;
    let mut output = vec![];
    {
        let mut rewriter = HtmlRewriter::new(
            Settings {
                element_content_handlers: vec![
                    // Cachebust <img> tags with a relative src attribute.
                    element!("img", |el| {
                        let src = el
                            .get_attribute("src")
                            .unwrap();
                        
                        // First character being / means the src attribute isn't relative,
                        // so we skip rewriting this tag.
                        if let Some('/') = src.chars().nth(0) {
                            return Ok(());
                        }

                        let asset = PathBuf::from(&src);
                        let page_relative = PathBuf::from(&page.path).parent().unwrap().join(&asset);
                        let assets_relative = PathBuf::from(SITE_SRC_DIRECTORY).join(SITE_ASSET_DIRECTORY).join(&asset);

                        if cachebust(&page_relative, el) { return Ok(()) }
                        if cachebust(&assets_relative, el) { return Ok(()) }
                        else { return Ok(()) }
                    })
                ],
                ..Settings::default()
            },
            |c: &[u8]| output.extend_from_slice(c)
        );
        rewriter.write(hypertext.as_bytes())?;
    }
    let hypertext = String::from_utf8(output)?;
    Ok(Cow::Owned(hypertext))
}

/// Prepares and returns a closure that wraps cachebusting and ID caching logic.
/// Returns `true` if the element was cachebusted successfully; false otherwise.
fn prepare_cachebust<'a>(conn: &'a Connection, page: &'a Page, rev_id: &'a str) -> Result<impl FnMut(&Path, &mut Element) -> bool + 'a> {
    conn.execute(
        "DELETE FROM dependencies WHERE kind = 2 AND page_id = ?1", 
        params![&page.id]
    )?;
    
    let mut try_query_id = conn.prepare("
        SELECT id FROM input_files
        WHERE path = ?1
        AND EXISTS (
            SELECT 1 FROM revision_files
            WHERE input_files.id = revision_files.id
            AND revision = ?2
        )
    ")?;

    let mut insert_dep = conn.prepare("
        INSERT OR REPLACE INTO dependencies (kind, page_id, asset_id) 
        VALUES (2, ?1, ?2);
    ")?;

    let closure = move |path: &Path, el: &mut Element| -> bool {
        let maybe_id: Option<String> = try_query_id
            .query_row(params![path.to_string_lossy(), rev_id], |row| row.get(0))
            .unwrap_or(None);

        if let Some(id) = maybe_id {
            match path.file_name() {
                Some(name) => {
                    let name = name.to_string_lossy();
                    let busted = name.replace(".", &format!(".{}.", id));
                    
                    let busted = path
                        .to_string_lossy()
                        .replace(&*name, &busted);

                    insert_dep.execute(params![&page.id, &id]).unwrap();
                    el.set_attribute("src", &busted).unwrap();

                    true
                }
                None => false,
            }
        }
        else { false }
    };

    Ok(closure)
}

/// Rewrites the `loading` attribute of all `<img>` and `<video>` tags to be `lazy`.
fn lazy_load<'a>(hypertext: Cow<'a, str>) -> Result<Cow<'a, str>> {
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
                    })
                ],
                ..Settings::default()
            },
            |c: &[u8]| output.extend_from_slice(c)
        );
        rewriter.write(hypertext.as_bytes())?;
    }
    let hypertext = String::from_utf8(output)?;
    Ok(Cow::Owned(hypertext))
}

