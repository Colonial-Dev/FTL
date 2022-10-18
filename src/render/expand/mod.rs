mod emoji;
mod highlight;
mod shortcode;

pub use emoji::expand_emoji;
pub use shortcode::evaluate_shortcodes as shortcodes;
pub use highlight::Highlighter;
pub use highlight::highlight_code;

use std::borrow::Cow;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::prelude::*;

use crate::parse::Ranged;

/// Fine-tuned version of [`Regex::replace_all()`].
/// - Uses the fast (match only, no capture) path.
/// - Takes and returns a [`Cow<str>`].
/// - Error-aware; the replacement closure has a return type of [`Result<&str>`].
fn regexp_expand<'a>(source: Cow<'a, str>, expression: &Lazy<Regex>, mut replacer: impl FnMut(&str) -> Result<String>) -> Result<Cow<'a, str>> {
    let mut matches = expression.find_iter(&source).peekable();
    if matches.peek().is_none() {
        return Ok(source);
    }

    let mut buffer = String::with_capacity(source.len());
    let mut last_match = 0;
    for m in matches {
        let replacement = replacer(m.as_str())?;
        buffer.push_str(&source[last_match..m.start()]);
        buffer.push_str(&replacement);
        last_match = m.end();
    }
    buffer.push_str(&source[last_match..]);
    buffer.shrink_to_fit();

    Ok(Cow::Owned(buffer))
}

fn ranged_expand<'a, T: Ranged>(source: Cow<'a, str>, targets: Vec<T>, mut replacer: impl FnMut(T) -> Result<String>) -> Result<Cow<'a, str>> {
    if targets.is_empty() {
        return Ok(source);
    }

    let mut buffer = String::with_capacity(source.len());
    let mut last_match = 0;
    for target in targets {
        let range = target.range();
        let replacement = replacer(target)?;

        buffer.push_str(&source[last_match..range.start]);
        buffer.push_str(&replacement);

        last_match = range.end;
    }
    buffer.push_str(&source[last_match..]);
    buffer.shrink_to_fit();

    Ok(Cow::Owned(buffer))
}