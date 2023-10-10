use itertools::Itertools;
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

record! {
    Name     => Row,
    id       => String,
    contents => String
}

pub fn create_hooks(ctx: &Context, rev_id: &RevisionID) -> Result<()> {
    let mut conn = ctx.db.get_rw()?;
    let txn = conn.transaction()?;

    let mut get_hooks = txn.prepare("
        SELECT input_files.id, contents FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND input_files.path LIKE 'hooks/%'
        AND input_files.extension = 'toml'
        AND NOT EXISTS (
            SELECT 1
            FROM hooks
            WHERE hooks.id = input_files.id
        )
    ")?;
    
    get_hooks.query_and_then([rev_id.as_ref()], Row::from_row)?
        .map_ok(|row| -> Result<_> {
            let id = row.id;
            let hook: TomlHook = toml::from_str(&row.contents)?;

            Ok((id, hook))
        })
        .flatten()
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

            Hook {
                id,
                paths,
                template: hook.template,
                headers,
                cache: hook.cache,
            }.insert_or(&txn, OnConflict::Ignore)?;

            Ok(())
        })?;

    get_hooks.finalize()?;
    txn.commit()?;
    Ok(())
}