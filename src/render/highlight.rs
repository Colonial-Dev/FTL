use std::hash::{Hash, Hasher};
use std::path::Path;

use itertools::Itertools;
use serde::{Serialize, Deserialize};
use syntect::{
    parsing::{SyntaxSet, SyntaxDefinition},
    highlighting::{ThemeSet, Theme},
    html::highlighted_html_for_string as higlight_html
};

use crate::db::{InputFile, Queryable, Statement, StatementExt};
use crate::prelude::*;
use crate::parse::Codeblock;

const HIGHLIGHTER_DUMP_PATH: &str = ".ftl/cache/highlighter.bin";

#[derive(Debug, Serialize, Deserialize)]
pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
    hash: u64,
}

impl Highlighter {
    pub fn new(state: &State) -> Result<Self> {
        match Path::new(HIGHLIGHTER_DUMP_PATH).exists() {
            false => Self::load_new(state),
            true => {
                debug!("Highlighter dump exists, attempting to load...");

                let old = Self::load_from_disk()?;
                let hash = load_hash(state)?;

                if old.hash == hash {
                    debug!("Hashes matched, using prebuilt highlighter!");
                    Ok(old)
                } else {
                    debug!("Hashes did NOT match, building new highlighter.");
                    Self::load_new(state)
                }
            }
        }
    }

    pub fn highlight(&self, block: Codeblock) -> Result<String> {
        let syntax = match block.token {
            Some(token) => match self.syntaxes.find_syntax_by_token(token) {
                Some(syntax) => syntax,
                None => {
                    let err = eyre!("A codeblock had a language token ('{token}'), but FTL could not find a matching syntax definition.")
                    .suggestion("Your codeblock's language token may just be malformed, or it could specify a language not bundled with FTL.");
                    bail!(err)
                }
            },
            None => self.syntaxes.find_syntax_plain_text()
        };

        higlight_html(
            block.body,
            &self.syntaxes,
            syntax,
            &self.theme
        ).wrap_err("An error occurred in the syntax highlighting engine.")
    }

    fn dump_to_disk(&self) -> Result<()> {
        std::fs::write(
            HIGHLIGHTER_DUMP_PATH,
            bincode::serialize(self)?
        )?;
        Ok(())
    }

    fn load_from_disk() -> Result<Self> {
        let bytes = std::fs::read(HIGHLIGHTER_DUMP_PATH)?;
        Ok(bincode::deserialize(&bytes)?)
    }

    fn load_new(state: &State) -> Result<Self> {
        warn!("Building syntax and theme sets from scratch - this might take a hot second!");

        let syntaxes = load_syntaxes(state)?;
        let theme_set = load_themes(state)?;
        let hash = load_hash(state)?;

        let Some(theme_name) = &state.config.render.highlight_theme else {
            bail!("Syntax highlighting is enabled, but no theme has been specified.")
        };

        let theme = match theme_set.themes.get(theme_name) {
            Some(theme) => theme.to_owned(),
            None => {
                let err = eyre!("Syntax highlighting theme \"{theme_name}\" does not exist.")
                    .note("This error occurred because FTL could not resolve your specified syntax highlighting theme from its name.")
                    .suggestion("Make sure your theme name is spelled correctly, and double-check that the corresponding theme file exists.");
                bail!(err)
            }
        };
        
        let new = Self {
            syntaxes,
            theme,
            hash
        };

        debug!("New higlighter created with id {hash:016x}.");
        new.dump_to_disk()?;
        Ok(new)
    }
}

fn load_syntaxes(state: &State) -> Result<SyntaxSet> {
    let conn = state.db.get_ro()?;
    let rev_id = state.get_working_rev();

    let query = "
        SELECT input_files.* FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND path LIKE 'src/config/highlighting/%'
        AND extension = 'sublime-syntax'
    ";
    let params = (1, rev_id.as_str()).into();

    let mut syntax_builder = SyntaxSet::load_defaults_newlines().into_builder();

    for syntax in conn.prepare_reader::<InputFile, _, _>(query, params)? {
        let def = SyntaxDefinition::load_from_str(
            &syntax?.contents.expect("Syntax contents should be Some."),
            true,
            None
        )?;
        syntax_builder.add(def);
    }

    Ok(syntax_builder.build())
}

fn load_themes(state: &State) -> Result<ThemeSet> {
    use std::io::Cursor;
    use std::ffi::OsStr;

    let conn = state.db.get_ro()?;
    let rev_id = state.get_working_rev();

    let query = "
        SELECT input_files.* FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND path LIKE 'src/config/highlighting/%'
        AND extension = 'tmTheme'
    ";
    let params = (1, rev_id.as_str()).into();

    let mut set = ThemeSet::load_defaults();

    for theme in conn.prepare_reader::<InputFile, _, _>(query, params)? {
        let theme = theme?;

        let bytes = theme.contents
            .expect("Theme contents should be Some.")
            .into_bytes();
        let mut cursor = Cursor::new(bytes);
        let def = ThemeSet::load_from_reader(&mut cursor)?;

        set.themes.insert(
            theme.path
                .file_stem()
                .and_then(OsStr::to_str)
                .map(str::to_owned)
                .expect("Theme path should be valid UTF-8."),
            def
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

fn load_hash(state: &State) -> Result<u64> {
    let conn = state.db.get_ro()?;
    let rev_id = state.get_working_rev();

    let query = "
        SELECT input_files.id FROM input_files
        JOIN revision_files ON revision_files.id = input_files.id
        WHERE revision_files.revision = ?1
        AND path LIKE 'src/config/highlighting/%'
        AND extension IN ('sublime-syntax', 'tmTheme')
    ";
    let params = (1, rev_id.as_str()).into();

    let hash = conn.prepare_reader(query, params)?
        .fold_ok(seahash::SeaHasher::new(), |mut hasher, row: Row| {
            row.hash(&mut hasher);
            hasher
        })?.finish();

    Ok(hash)
}