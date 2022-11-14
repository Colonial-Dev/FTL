use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::Regex;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_rusqlite::from_rows;
use toml::value::Datetime;

use crate::{
    db::{data::Page, *},
    parse::delimit::TOML_DELIM,
    prelude::*,
};

static EXT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new("[.][^.]+$").unwrap());

#[derive(Serialize, Deserialize, Debug)]
struct Frontmatter {
    pub title: String,
    pub date: Option<Datetime>,
    pub description: Option<String>,
    pub summary: Option<String>,
    pub template: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub dynamic: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub collections: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct Row {
    pub id: String,
    pub path: String,
    pub contents: String,
}

pub fn parse_frontmatters(conn: &Connection, rev_id: &str) -> Result<()> {
    info!("Starting frontmatter parsing for revision {}...", rev_id);
    let mut insert_page = Page::prepare_insert(conn)?;
    let rows = query_new_pages(conn, rev_id)?;

    let pages: Vec<Result<Page>> = rows.into_par_iter().map(extract_frontmatter).collect();

    for page in pages {
        let page = page?;
        insert_page(&page)?;
    }

    info!("Done parsing frontmatters for revision {}", rev_id);
    Ok(())
}

fn query_new_pages(conn: &Connection, rev_id: &str) -> Result<Vec<Row>> {
    let mut stmt = conn.prepare(
        "
        SELECT id, path, contents
        FROM input_files
        WHERE EXISTS (
                SELECT 1
                FROM revision_files
                WHERE revision_files.id = input_files.id
                AND revision_files.revision = ?1
            EXCEPT
                SELECT 1 
                FROM pages 
                WHERE pages.id = input_files.id
        )
        AND input_files.extension = 'md';
    ",
    )?;

    let rows: Result<Vec<Row>> = from_rows::<Row>(stmt.query(params![&rev_id])?)
        .map(|x| x.wrap_err("SQLite deserialization error!"))
        .collect();

    let rows = rows?;

    debug!(
        "Query for new pages complete, found {} entries.",
        rows.len()
    );
    Ok(rows)
}

fn extract_frontmatter(item: Row) -> Result<Page> {
    debug!("Extracting frontmatter for file {}...", item.id);

    let frontmatter = TOML_DELIM
        .parse_iter(&item.contents)
        .next()
        .context("Could not find frontmatter.")?;

    match toml::from_str::<Frontmatter>(frontmatter.contents) {
        Ok(fm) => {
            debug!("Parsed frontmatter for page \"{}\"", fm.title);
            let page = to_page(item.id, item.path, frontmatter.range.end as i64, fm);
            Ok(page)
        }
        Err(e) => {
            bail!("Error when parsing frontmatter for file {}: {}", item.id, e);
        }
    }
}

fn to_page(id: String, path: String, offset: i64, fm: Frontmatter) -> Page {
    Page {
        id,
        route: to_route(&path),
        path,
        offset,
        title: fm.title,
        date: fm.date.map(|x| x.to_string()),
        description: fm.description,
        summary: fm.summary,
        template: fm.template,
        draft: fm.draft,
        dynamic: fm.dynamic,
        tags: fm.tags,
        collections: fm.collections,
        aliases: fm.aliases,
    }
}

fn to_route(path: &str) -> String {
    let route_path = path
        .trim_start_matches(SITE_SRC_DIRECTORY)
        .trim_start_matches(SITE_CONTENT_DIRECTORY)
        .trim_end_matches("/index.md")
        .trim_start_matches('/');

    EXT_REGEX.replace(route_path, "").to_string()
}
