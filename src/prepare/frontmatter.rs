use crate::{db::*, db::data::{Page, PageIn}};

use rayon::prelude::*;
use anyhow::{Result, Context};
use lazy_static::lazy_static;
use regex::Captures;
use serde::{Deserialize, Serialize};
use serde_rusqlite::from_rows;
use rusqlite::params;
use toml::value::Datetime;
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

lazy_static! {
    static ref TOML_FRONTMATTER: regex::Regex = regex::Regex::new(r#"(\+\+\+)(.|\n)*(\+\+\+)"#).unwrap();
    static ref EXT_REGEX: regex::Regex = regex::Regex::new("[.][^.]+$").unwrap();
}

#[derive(Serialize, Deserialize, Debug)]
struct TomlFrontmatter {
    pub title: String,
    pub date: Option<Datetime>,
    pub publish_date: Option<Datetime>,
    pub expire_date: Option<Datetime>,
    pub description: Option<String>,
    pub summary: Option<String>,
    pub template: Option<String>,
    #[serde(default)]
    pub draft: bool,
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
    log::info!("Starting frontmatter parsing for revision {}...", rev_id);
    let mut insert_page = Page::prepare_insert(conn)?;
    let rows = query_new_pages(conn, rev_id)?;

    // TODO: refactor to avoid collecting into a Vec.
    // Probably just need to send the Pages into a channel,
    // consume the parallel iterator and then iterate 
    // serially over the channel's rx.
    //
    // Also, into_par_iter to avoid clones in parse_frontmatter?
    let num_pages = rows
        .par_iter()
        .filter_map(try_extract_frontmatter)
        .filter_map(parse_frontmatter)
        .collect::<Vec<Page>>()
        .into_iter() // Convert to serial iterator, because rusqlite is Not Thread Safe (TM)
        .map(|x| {
            let page = PageIn::from(&x);
            insert_page(&page)
        })
        .map(|x| if let Err(e) = x {
            log::error!("Error when inserting Page: {:#?}", e);
        })
        .count();

    log::info!("Done parsing frontmatters for revision {}, processed {} pages.", rev_id, num_pages);
    Ok(())
}

fn query_new_pages(conn: &Connection, rev_id: &str) -> Result<Vec<Row>> {
    let mut stmt = conn.prepare("
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
    ")?;

    let result = from_rows::<Row>(stmt.query(params![&rev_id])?);
    let mut rows  = Vec::new();
    for row in result {
        rows.push(row?);
    }

    log::trace!("Query for new pages complete, found {} entries.", rows.len());

    Ok(rows)
}

fn try_extract_frontmatter(item: &Row) -> Option<(&Row, Captures)> {   
    log::trace!("Extracting frontmatter for file {}...", item.id);

    let captures = TOML_FRONTMATTER.captures(&item.contents);
    match captures {
        Some(cap) => Some((item, cap)),
        None => {
            log::error!("Could not locate frontmatter for file {}.", item.id);
            None
        }
    }
}

fn parse_frontmatter(bundle: (&Row, Captures)) -> Option<Page> {
    let (item, capture) = bundle;

    let capture = capture.get(0).unwrap();
    let raw = capture.as_str();
    let raw = raw.replace("+++", "");

    match toml::from_str::<TomlFrontmatter>(&raw) {
        Ok(fm) => {
            log::trace!("Parsed frontmatter for page \"{}\"", fm.title);
            let page = to_page(
                item.id.clone(),
                to_route(&item.path),
                capture.end() as i64,
                fm
            );
            Some(page)
        }
        Err(e) => {
            log::error!("Error when parsing frontmatter for file {}: {}", item.id, e);
            None
        }
    }
}

fn to_route(path: &str) -> String {
    let route_path = path
        .trim_start_matches("src/")
        .trim_start_matches("content")
        .trim_end_matches("/index.md")
        .trim_start_matches('/');
    
    EXT_REGEX.replace(route_path, "").to_string()
}

fn to_page(id: String, route: String, offset: i64, fm: TomlFrontmatter) -> Page {
    Page {
        id,
        route,
        offset,
        title: fm.title,
        date: unwrap_datetime(fm.date),
        publish_date: unwrap_datetime(fm.publish_date),
        expire_date: unwrap_datetime(fm.expire_date),
        description: fm.description,
        summary: fm.summary,
        template: fm.template,
        draft: fm.draft,
        tags: fm.tags,
        collections: fm.collections,
        aliases: fm.aliases
    }
}

fn unwrap_datetime(value: Option<Datetime>) -> Option<OffsetDateTime> {
    match value {
        Some(dt) => {
            let dt = dt.to_string();
            let odt = OffsetDateTime::parse(&dt, &Iso8601::DEFAULT);
            match odt {
                Ok(odt) => Some(odt),
                // TODO error handling
                Err(_) => None
            }
        }
        None => None
    }
}