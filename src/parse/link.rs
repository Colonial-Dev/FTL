
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{Connection, params, OptionalExtension};

use crate::prelude::*;

static URL_SCHEMA: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[0-9A-Za-z\-]+:").unwrap());

#[derive(Debug, Clone, Copy)]
pub enum Root {
    Absolute,
    Contents,
    Assets,
}

#[derive(Debug)]
pub enum Link<'a> {
    Relative(&'a str),
    Internal(&'a str, Root),
    External(&'a str),
}

impl<'a> Link<'a> {
    pub fn parse(source: &'a str) -> Result<Self> {
        if URL_SCHEMA.is_match(source) {
            return Ok(Link::External(source))
        }

        match source.chars().next().context("Cannot parse an empty link.")? {
            '@' => {
                let source = source.trim_start_matches("@/");
                let source = Link::Internal(source, Root::Contents);
                Ok(source)
            }
            '$' => {
                let source = source.trim_start_matches("$/");
                let source = Link::Internal(source, Root::Assets);
                Ok(source)
            }
            '/' => {
                let source = Link::Internal(source, Root::Absolute);
                Ok(source)
            }
            _ => Ok(Link::Relative(source))
        }
    }

    pub fn prepare_cachebust(conn: &'a Connection, rev_id: &'a str) -> Result<impl FnMut(&Link, Option<&Path>) -> Result<Option<(String, String)>> + 'a> {
        let mut query_id = conn.prepare("
            SELECT input_files.id FROM input_files
            JOIN revision_files ON revision_files.id = input_files.id
            WHERE revision_files.revision = ?1
            AND input_files.path = ?2
        ")?;

        let mut cachebust = move |path: &Path| -> Result<Option<(String, String)>> {
            let name = path
                .file_name()
                .context("Cannot cachebust a path without a file name.")?
                .to_string_lossy();

            let maybe_id: Option<String> =
                query_id.query_row(params![rev_id, path.to_string_lossy()], |row| row.get(0))
                .optional()?;
            
            debug!("{maybe_id:?}");

            if let Some(id) = maybe_id {
                let busted = match name.contains('.') {
                    true => name.replace('.', &format!(".{}.", id)),
                    false => {
                        let mut name = name.to_string();
                        name.push_str(&id);
                        name
                    }
                };
                Ok(Some((busted, id)))
            } 
            else {
                Ok(None)
            }
        };

        let check_relative = |link: &str, path: Option<&Path>| -> Result<PathBuf> {
            let path = path.context("Cannot cachebust a relative link without a path.")?;
            let mut path = PathBuf::from(path);
            path.pop();
            path.push(link);

            Ok(path)
        };

        let check_internal = |link: &str, root: Root| -> Result<PathBuf> {
            let mut path = PathBuf::new();
            match root {
                Root::Absolute => path.push(link),
                Root::Contents => {
                    path.push("src/content");
                    path.push(link);
                }
                Root::Assets => {
                    path.push("src/assets");
                    path.push(link);
                }
            };

            Ok(path)
        };

        let closure = move |link: &Link, path: Option<&Path>| -> Result<Option<(String, String)>> {
            debug!("{link:?}");
            debug!("{path:?}");
            let path = match link {
                Link::Relative(link) => check_relative(link, path)?,
                Link::Internal(link, root) => check_internal(link, *root)?,
                Link::External(_) => bail!("Cannot cachebust an external link.")
            };

            cachebust(&path)
        };

        Ok(closure)
    }
}