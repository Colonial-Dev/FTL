use serde::{Serialize, Deserialize};
use syntect::{
    parsing::SyntaxSet,
    highlighting::{ThemeSet, Theme},
    html::highlighted_html_for_string as higlight_html
};

use crate::prelude::*;

const HIGHLIGHTER_DUMP_PATH: &str = ".ftl/cache/highlighter.bin";

#[derive(Debug, Serialize, Deserialize)]
pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
}

// TODO: implement loading from disk when possible - linking a syntax set is SLOOOOOW
impl Highlighter {
    pub fn new(state: &State) -> Result<Self> {
        let Some(theme_name) = &state.config.render.highlight_theme else {
            bail!("Syntax highlighting is enabled, but no theme has been specified.")
        };

        let mut syntax_builder = SyntaxSet::load_defaults_newlines().into_builder();
        syntax_builder.add_from_folder("src/cfg/highlighting/", true)?;

        let mut theme_set = ThemeSet::load_defaults();
        theme_set.add_from_folder("src/cfg/highlighting/")?;

        let theme = match theme_set.themes.remove(theme_name) {
            Some(theme) => theme,
            None => {
                let err = eyre!("Syntax highlighting theme \"{theme_name}\" does not exist.")
                    .note("This error occurred because FTL could not resolve your specified syntax highlighting theme from its name.")
                    .suggestion("Make sure your theme name is spelled correctly, and double-check that the corresponding theme file exists.");
                bail!(err)
            }
        };
        let syntaxes = syntax_builder.build();
    
        debug!("Highlighting syntaxes and themes loaded.");

        Ok(Self {
            syntaxes,
            theme
        })
    }

    pub fn highlight(&self, token: Option<&str>, body: &str) -> Result<String> {
        let syntax = match token {
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
            body,
            &self.syntaxes,
            syntax,
            &self.theme
        ).wrap_err("An error occurred in the syntax highlighting engine.")
    }
}