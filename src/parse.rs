use crate::{*, db::*, dbdata::*};
use lazy_static::lazy_static;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use serde_rusqlite::from_rows;
use rusqlite::params;

lazy_static! {
    static ref TOML_FRONTMATTER: regex::Regex = regex::Regex::new(r#"(\+\+\+)(.|\n)*(\+\+\+)"#).unwrap();
}

#[derive(Serialize, Deserialize, Debug)]
struct TomlPage {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub series: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub publish_date: String,
    #[serde(default)]
    pub expire_date: String,
}

// TODO - there must be a better way than having an intermediate TOML representation for Page
impl Into<Page> for TomlPage {
    fn into(self) -> Page {
        Page {
            id: self.id,
            offset: self.offset,
            title: self.title,
            date: self.date,
            description: self.description,
            summary: self.summary,
            tags: self.tags,
            series: self.series,
            aliases: self.aliases,
            template: self.template,
            draft: self.draft,
            publish_date: self.publish_date,
            expire_date: self.expire_date,
        }
    }
}

#[derive(Deserialize, Debug)]
struct Row {
    pub id: String,
    pub contents: String,
}

pub fn parse_markdown(pool: &DbPool, rev_id: &str) -> Result<Vec<Page>, DbError> {
    log::info!("Starting frontmatter parsing for revision {}...", rev_id);

    let rows = query_new_pages(pool, rev_id)?;
    let output: Vec<Page> = rows.par_iter()
        .filter_map(extract_frontmatter)
        .collect();

    log::info!("Done parsing frontmatters for revision {}, processed {} pages.", rev_id, output.len());
    Ok(output)
}

fn query_new_pages(pool: &DbPool, rev_id: &str) -> Result<Vec<Row>, DbError> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("
        SELECT input_files.id, input_files.contents
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

    let mut result = from_rows::<Row>(stmt.query(params![&rev_id])?);
    let mut rows  = Vec::new();
    while let Some(row) = result.next() {
        rows.push(row?);
    }

    log::trace!("Query for new pages complete, found {} entries.", rows.len());

    Ok(rows)
}

fn extract_frontmatter(item: &Row) -> Option<Page> {   
    log::trace!("Extracting frontmatter for file {}...", item.id);

    let captures = TOML_FRONTMATTER.captures(&item.contents);
    match captures {
        Some(captures) => {
            // Unwrap justification: if captures is Some, it must have at least one item.
            let fm_raw = captures.get(0).unwrap();
            let fm_terminus = fm_raw.end();
            let fm = parse_frontmatter(fm_raw.as_str());

            match fm {
                Ok(mut fm) => {
                    fm.id = item.id.clone();
                    fm.offset = fm_terminus as i64;
                    return Some(fm);
                }
                Err(error) => ERROR_CHANNEL.sink_error(ParseError::from(error))
            }
        }
        None => log::error!("Could not locate frontmatter for file {}.", item.id)
    }

    None
}

fn parse_frontmatter(raw: &str) -> Result<Page, toml::de::Error> {
    let raw = raw.replace("+++", "");
    let fm: TomlPage = toml::from_str(&raw)?;
    
    log::trace!("Parsed frontmatter for page \"{}\"", fm.title);
    Ok(fm.into())
}