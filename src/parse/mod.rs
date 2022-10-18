mod codeblock;
pub mod shortcode;

use std::ops::Range;

pub use codeblock::Codeblock;

pub type Span<'a> = nom_locate::LocatedSpan<&'a str>;

pub trait Ranged {
    fn range(&self) -> Range<usize>;

    fn start(&self) -> usize {
        self.range().start
    }

    fn end(&self) -> usize {
        self.range().end
    }
}

fn trim_whitespace(i: &str) -> (&str, &str) {
    (i, i.trim())
}

fn trim_quotes(i: &str) -> (&str, &str) {
    let trimmed = i
        .trim_start_matches('"')
        .trim_end_matches('"');
    
    (i, trimmed)
}