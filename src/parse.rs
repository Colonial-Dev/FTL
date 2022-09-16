use crate::{*, db::*};
use lazy_static::lazy_static;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use serde::{Serialize, Deserialize};
use serde_rusqlite::from_rows;
use rusqlite::params;

lazy_static! {
    static ref TOML_FRONTMATTER: regex::Regex = regex::Regex::new(r#"(\+\+\+)(.|\n)*(\+\+\+)"#).unwrap();
}

#[derive(Serialize, Debug)]
pub struct FmItem {
    pub hapa: String,
    pub fm: FrontMatter,
    pub offset: usize
}

#[derive(Serialize, Deserialize, Debug)]
#[allow(dead_code)]
pub struct FrontMatter {
    pub title: Option<String>,
    pub date: Option<String>,
    pub description: Option<String>,
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub series: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub build_cfg: Build,
}

#[derive(Serialize, Deserialize, Debug, Default)]
#[allow(dead_code)]
pub struct Build {
    pub template: Option<String>,
    #[serde(default)]
    pub draft: bool,
    pub publish_date: Option<String>,
    pub expire_date: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Row {
    pub hapa: String,
    pub contents: String,
}

pub fn parse_markdown(pool: &DbPool, sinks: &BuildSinks, rev_id: &str) -> Result<Vec<FmItem>, DbError> {
    log::info!("Starting frontmatter parsing for revision {}...", rev_id);

    let rows = query_new_pages(pool, rev_id)?;
    let output: Vec<FmItem> = rows.par_iter()
        .filter_map(|x| extract_frontmatter(sinks, x))
        .collect();

    log::info!("Done parsing frontmatters for revision {}, processed {} pages.", rev_id, output.len());
    Ok(output)
}

fn query_new_pages(pool: &DbPool, rev_id: &str) -> Result<Vec<Row>, DbError> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("
        SELECT input_files.hapa, input_files.contents
        FROM input_files
        WHERE EXISTS (
                SELECT 1
                FROM revision_files
                WHERE revision_files.hapa = input_files.hapa
                AND revision_files.revision = ?1
            EXCEPT
                SELECT 1 
                FROM pages 
                WHERE pages.hapa = input_files.hapa
        )
        AND input_files.extension = 'md';
    ")?;

    let mut result = from_rows::<Row>(stmt.query(params![&rev_id])?);
    let mut rows  = Vec::new();
    while let Some(row) = result.next() {
        rows.push(row?);
    }

    log::trace!("Query for new pages complete, found {} entries.", rows.len());

    Ok(rows)
}

fn extract_frontmatter(sinks: &BuildSinks, item: &Row) -> Option<FmItem> {   
    log::trace!("Extracting frontmatter for file {}...", item.hapa);

    let captures = TOML_FRONTMATTER.captures(&item.contents);
    match captures {
        Some(captures) => {
            // Unwrap justification: if captures is Some, it must have at least one item.
            let fm_raw = captures.get(0).unwrap();
            let fm_terminus = fm_raw.end();
            let fm = parse_frontmatter(fm_raw.as_str());

            match fm {
                Ok(fm) => {
                    let item = FmItem {
                        hapa: item.hapa.clone(),
                        fm,
                        offset: fm_terminus
                    };
                    return Some(item);
                }
                Err(error) => sinks.sink_error(ParseError::from(error))
            }
        }
        None => log::error!("Could not locate frontmatter for file {}.", item.hapa)
    }

    None
}

fn parse_frontmatter(raw: &str) -> Result<FrontMatter, toml::de::Error> {
    let raw = raw.replace("+++", "");
    let fm: FrontMatter = toml::from_str(&raw)?;

    log::trace!("Parsed frontmatter for page \"{}\"", fm.title.as_ref().unwrap_or(&"N/A".to_string()));
    Ok(fm)
}