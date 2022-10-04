use std::borrow::Cow;
use std::path::{PathBuf, Path};

use anyhow::Result;
use lol_html::html_content::Element;
use lol_html::{element, HtmlRewriter, Settings};
use rusqlite::params;

use crate::db::Connection;
use crate::db::data::Page;
use crate::share::*;

pub fn rewrite<'a>(hypertext: Cow<'a, str>) -> Cow<'a, str> {
    hypertext
}

/// Cachebusts `<img>` tags with a relative `src` attribute.
/// This turns, say, `bar.png` into `content/pages/foo/bar.ffde185cab76e0a6.png`.
/// 
/// FTL will try to query for the image's ID at a few locations:
/// - The directory of the page itself.
///   - Example: A page is located at `src/content/articles/article-01/index.md`.
///   - If the image path is `cover.png`, we query by `src/content/articles/article-01/cover.png`.
///   - If the image path is `../cover.png`, we query by `src/content/articles/cover.png`.
/// - The assets and static directories.
///   - If the image path is `background.png`, we query by `src/[assets][static]/background.png`.
/// 
/// A match relative to the page takes priority over a match in the assets or static directory.
/// If no match is found, we leave the tag untouched.
pub fn cachebust_img<'a>(hypertext: String, page: &Page, conn: &Connection) -> Result<String> {
    let mut stmt = conn.prepare("
        SELECT id FROM input_files
        WHERE path = ?1
    ")?;

    let mut check_relative = |path: &Path, el: &mut Element, src: &Path| {
        let id: Option<String> =
        stmt.query_row(params![path.to_string_lossy()], |row| row.get(0))
            .unwrap_or(None);

        if let Some(id) = id {
            let file_name = src.file_name().unwrap_or_default().to_str().unwrap_or_default();
            let busted = file_name.replace(".", &format!(".{}.", id));
            el.set_attribute("src", &busted).unwrap();
        }
    };
    
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

                        // This does work, but could be cleaned up a little, especially check_relative (see above.)
                        // Should also skip over any remaining check_relative calls if one evaluates successfully.
                        let asset = PathBuf::from(&src);
                        let page_relative = PathBuf::from(&page.path);
                        let asset_relative = PathBuf::from(SITE_SRC_DIRECTORY).join(SITE_ASSET_DIRECTORY).join(&asset);
                        let static_relative = PathBuf::from(SITE_SRC_DIRECTORY).join(SITE_STATIC_DIRECTORY).join(&asset);

                        let page_relative = match page_relative.parent() {
                            Some(path) => path.join(&asset),
                            None => page_relative,
                        };

                        check_relative(&page_relative, el, &asset);
                        check_relative(&asset_relative, el, &asset);
                        check_relative(&static_relative, el, &asset);

                        // TODO - store page ID / cachebusted image ID pairs in the database somehow
                        // so we can re-render pages based on changes to its assets.

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
    Ok(hypertext)
}

/// Rewrites the `loading` attribute of all `<img>` and `<video>` tags to be `lazy`.
fn lazy_load<'a>(hypertext: String) -> Result<String> {
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
    Ok(hypertext)
}

