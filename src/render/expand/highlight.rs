use syntect::parsing::SyntaxSet;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::html::highlighted_html_for_string as highlight_html;

use crate::prelude::*;
use crate::render::{RenderTicket, Engine};
use crate::parse::Codeblock;

use super::ranged_expand;

pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: Theme
}

impl Highlighter {
    pub fn build(theme_name: &str) -> Result<Self> {
        let syntaxes = SyntaxSet::load_defaults_newlines();
        let theme = get_theme(theme_name)?;

        Ok(Highlighter {
            syntaxes,
            theme
        })
    }

    pub fn highlight(&self, block: Codeblock) -> Result<String> {
        match self.syntaxes.find_syntax_by_token(block.token) {
            Some(syntax) => {
                highlight_html(
                    block.code, 
                    &self.syntaxes, 
                    syntax, 
                    &self.theme
                ).wrap_err("")
            }
            None => {
                let err = eyre!("");
                bail!(err)
            }
        }
    }
}

fn get_theme(theme_name: &str) -> Result<Theme> {
    let mut themes = ThemeSet::load_defaults().themes;

    match themes.remove(theme_name) {
        Some(theme) => Ok(theme),
        None => {
            let err = eyre!("");
            bail!(err)
        }
    }
}

pub fn highlight_code(ticket: &mut RenderTicket, engine: &Engine) -> Result<()> {
    let codeblocks = Codeblock::parse_many(&ticket.content)?;

    ticket.content = ranged_expand(ticket.content.clone(), codeblocks, |block: Codeblock| {
        Ok(engine.highlighter.highlight(block)?)
    })?;

    Ok(())
}