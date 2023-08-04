use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::Path;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use syntect::highlighting::{Theme, ThemeSet};
use syntect::html::highlighted_html_for_string as higlight_html;
use syntect::parsing::{SyntaxDefinition, SyntaxSet};

use crate::db::{InputFile, Queryable, Statement, StatementExt};
use crate::prelude::*;

const HIGHLIGHTER_DUMP_PATH: &str = ".ftl/cache/highlighter.bin";

#[derive(Debug, Serialize, Deserialize)]
pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme_set: ThemeSet,
    curr_theme: Theme,
    hash: u64,
}

impl Highlighter {
    pub fn new(ctx: &Context, rev_id: &RevisionID) -> Result<Self> {
        // If there's a highlighter dump on the disk, we load it and check its hash against the current revision.
        // If it matches, then we skip the expensive build step and just use the loaded dump.
        // If it doesn't match, or if a dump doesn't exist, we take the slow path and build a new one from scratch,
        // dumping the result to disk when finished.
        match Path::new(HIGHLIGHTER_DUMP_PATH).exists() {
            false => Self::load_new(ctx, rev_id),
            true => Self::load_from_disk(ctx, rev_id),
        }
    }

    pub fn highlight(&self, body: String, token: Option<String>) -> Result<String> {
        let syntax = match token {
            Some(token) => match self.syntaxes.find_syntax_by_token(&token) {
                Some(syntax) => syntax,
                None => {
                    let err = eyre!("A codeblock had a language token ('{token}'), but FTL could not find a matching syntax definition.")
                    .note("Your codeblock's language token may just be malformed, or it could specify a language not bundled with FTL.")
                    .suggestion("Provide a valid language token, or remove it to format the block as plain text.");
                    bail!(err)
                }
            },
            None => self.syntaxes.find_syntax_plain_text(),
        };

        higlight_html(&body, &self.syntaxes, syntax, &self.curr_theme)
            .wrap_err("An error occurred in the syntax highlighting engine.")
    }

    fn dump_to_disk(&self) -> Result<()> {
        std::fs::write(HIGHLIGHTER_DUMP_PATH, serde_cbor::to_vec(self)?)?;
        Ok(())
    }

    fn load_from_disk(ctx: &Context, rev_id: &RevisionID) -> Result<Self> {
        debug!("Loading highlighter dump from disk...");

        let bytes = std::fs::read(HIGHLIGHTER_DUMP_PATH)?;
        let mut loaded: Self = serde_cbor::from_slice(&bytes)?;
        let hash = load_hash(ctx, rev_id)?;

        if loaded.hash == hash {
            debug!("Hashes matched, using prebuilt highlighter.");
            loaded.curr_theme = Self::get_theme(ctx, &loaded.theme_set)?;
            Ok(loaded)
        } else {
            debug!("Hashes did NOT match, building new highlighter.");
            Self::load_new(ctx, rev_id)
        }
    }

    fn load_new(ctx: &Context, rev_id: &RevisionID) -> Result<Self> {
        warn!("Building syntax and theme sets from scratch - this might take a hot second!");

        let syntaxes = load_syntaxes(ctx, rev_id)?;
        info!("New syntax set loaded.");
        let theme_set = load_themes(ctx, rev_id)?;
        info!("New theme set loaded.");

        let hash = load_hash(ctx, rev_id)?;
        let curr_theme = Self::get_theme(ctx, &theme_set)?;

        let new = Self {
            syntaxes,
            theme_set,
            curr_theme,
            hash,
        };

        debug!("New higlighter created with id {hash:016x}.");
        new.dump_to_disk()?;
        Ok(new)
    }

    fn get_theme(ctx: &Context, set: &ThemeSet) -> Result<Theme> {
        let Some(theme_name) = &ctx.config.render.highlight_theme else {
            bail!("Syntax highlighting is enabled, but no theme has been specified.")
        };

        match set.themes.get(theme_name) {
            Some(theme) => Ok(theme.to_owned()),
            None => {
                let err = eyre!("Syntax highlighting theme \"{theme_name}\" does not exist.")
                    .note("This error occurred because FTL could not resolve your specified syntax highlighting theme from its name.")
                    .suggestion("Make sure your theme name is spelled correctly, and double-check that the corresponding theme file exists.");
                bail!(err)
            }
        }
    }
}

fn load_syntaxes(ctx: &Context, rev_id: &RevisionID) -> Result<SyntaxSet> {
    let conn = ctx.db.get_ro()?;

    let query = "
        SELECT input_files.* FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND path LIKE 'config/highlighting/%'
        AND extension = 'sublime-syntax'
    ";
    let params = (1, rev_id.as_ref()).into();

    let mut syntax_builder = SyntaxSet::load_defaults_newlines().into_builder();

    for syntax in conn.prepare_reader::<InputFile, _, _>(query, params)? {
        let def = SyntaxDefinition::load_from_str(
            &syntax?.contents.expect("Syntax contents should be Some."),
            true,
            None,
        )?;
        syntax_builder.add(def);
    }

    Ok(syntax_builder.build())
}

fn load_themes(ctx: &Context, rev_id: &RevisionID) -> Result<ThemeSet> {
    let conn = ctx.db.get_ro()?;

    let query = "
        SELECT input_files.* FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND path LIKE 'config/highlighting/%'
        AND extension = 'tmTheme'
    ";
    let params = (1, rev_id.as_ref()).into();

    let mut set = ThemeSet::load_defaults();

    for theme in conn.prepare_reader(query, params)? {
        let theme: InputFile = theme?;

        let bytes = theme
            .contents
            .expect("Theme contents should be Some.")
            .into_bytes();
        let mut cursor = Cursor::new(bytes);
        let def = ThemeSet::load_from_reader(&mut cursor)?;

        set.themes.insert(
            theme
                .path
                .file_stem()
                .and_then(OsStr::to_str)
                .map(str::to_owned)
                .expect("Theme path should be valid UTF-8."),
            def,
        );
    }

    Ok(set)
}

#[derive(Hash)]
struct Row(String);

impl Queryable for Row {
    fn read_query(stmt: &Statement<'_>) -> Result<Self> {
        Ok(Self(stmt.read_string("id")?))
    }
}

fn load_hash(ctx: &Context, rev_id: &RevisionID) -> Result<u64> {
    let conn = ctx.db.get_ro()?;

    let query = "
        SELECT input_files.id FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND path LIKE 'config/highlighting/%'
        AND extension IN ('sublime-syntax', 'tmTheme')
        ORDER BY input_files.id
    ";
    let params = (1, rev_id.as_ref()).into();

    let hash = conn
        .prepare_reader(query, params)?
        .fold_ok(seahash::SeaHasher::new(), |mut hasher, row: Row| {
            row.hash(&mut hasher);
            hasher
        })?
        .finish();

    Ok(hash)
}
