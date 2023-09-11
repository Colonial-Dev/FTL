use serde::Deserialize;

use crate::db::*;
use crate::prelude::*;

#[derive(Debug, Deserialize)]
struct TomlHook {
    paths: Vec<String>,
    template: String,
    #[serde(default)]
    headers: Vec<String>,
    cache: bool
}

#[derive(Debug)]
struct Row {
    id: String,
    contents: String,
}

impl Queryable for Row {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self {
            id: stmt.read_string("id")?,
            contents: stmt.read_string("contents")?,
        })
    }
}

pub fn create_hooks(ctx: &Context, rev_id: &RevisionID) -> Result<()> {
    let conn = ctx.db.get_rw()?;

    let query_reader = "
        SELECT input_files.id, contents FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.path LIKE 'hooks/%'
        AND input_files.extension = 'toml'
    ";

    let params_reader = (1, rev_id).into();
    
    let mut insert_hook = conn.prepare_writer(DEFAULT_QUERY, NO_PARAMS)?;

    conn.prepare_reader(query_reader, params_reader)?
        .map(|row| -> Result<_> {
            let row: Row = row?;

            let id = row.id;
            let hook: TomlHook = toml::from_str(&row.contents)?;

            Ok((id, hook))
        })
        .try_for_each(|hook| -> Result<_> {
            let (id, hook) = hook?;

            let mut paths = String::new();

            for path in hook.paths {
                paths += &path;
                paths += "\n";
            }

            if !paths.is_empty() {
                paths.truncate(paths.len() - 1)
            }

            let mut headers = String::new();

            for header in hook.headers {
                headers += &header;
                headers += "\n";
            }

            if !headers.is_empty() {
                headers.truncate(headers.len() - 1);
            }

            insert_hook(&Hook {
                id,
                paths,
                revision: rev_id.to_string(),
                template: hook.template,
                headers,
                cache: hook.cache,
            })?;

            Ok(())
        })?;

    Ok(())
}