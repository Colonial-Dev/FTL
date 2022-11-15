use std::{marker::PhantomData, ops::Range};

use once_cell::sync::Lazy;
use regex::{Match, Regex};

use super::Ranged;
use crate::prelude::*;

pub static EMOJI_DELIM: Lazy<Delimiters> = Lazy::new(|| {
    Delimiters::new_with_regex(
        ":",
        ":",
        DelimiterKind::Inline,
        r#":[a-z1238+-][a-z0-9_-]*:"#,
    )
});

pub static TOML_DELIM: Lazy<Delimiters> =
    Lazy::new(|| Delimiters::new("+++", "+++", DelimiterKind::Multiline));

pub static CODE_DELIM: Lazy<Delimiters> =
    Lazy::new(|| Delimiters::new("```", "```", DelimiterKind::Multiline));

pub static INLINE_SC_DELIM: Lazy<Delimiters> =
    Lazy::new(|| Delimiters::new("{% sci ", " %}", DelimiterKind::Inline));

pub static BLOCK_SC_DELIM: Lazy<Delimiters> =
    Lazy::new(|| Delimiters::new("{% sc ", "{% endsc %}", DelimiterKind::Multiline));

/// Unit enum representing the possible types of delimited structures in textual data.
#[derive(Debug, Clone, Copy)]
pub enum DelimiterKind {
    /// A delimited structure that does not cross a newline boundary.
    Inline,
    /// A delimited structure that can potentially cross newline boundaries.
    Multiline,
    /// Like [`DelimiterKind::Multiline`], but represents the additional
    /// property that delimiters should not be stripped.
    Raw,
}

/// A parsing template capable of extracting delimited structures from textual data.
///
/// Given a pair of starting/ending delimiters and a parsing mode (see [`DelimiterKind`]),
/// an appropriate regular expression is compiled and stored internally.
///
/// Input data fed into a [`Delimiters`] instance is first broken down into regex matches,
/// which are then appropriately post-processed into output data.
#[derive(Debug)]
pub struct Delimiters<'a> {
    /// The opening delimiter.
    start: &'static str,
    /// The closing delimiter.
    end: &'static str,
    /// The delimiter type (can also be thought of as "parsing mode".)
    kind: DelimiterKind,
    /// The regular expression used in parsing. Typically auto-generated by [`Delimiters::new()`].
    regex: Regex,
    /// PhantomData of type [`&'a ()`], required to allow zero-copy parsing from input data.
    phantom: PhantomData<&'a ()>,
}

impl<'a> Delimiters<'a> {
    /// Creates a new [`Delimiters`] instance using the provided parameters.
    ///
    /// Note that this method compiles a regex under the hood. This means:
    /// - It incurs a non-trivial computational cost.
    /// - It can panic, should the expression string be invalid.
    pub fn new(start: &'static str, end: &'static str, kind: DelimiterKind) -> Self {
        let s_escaped = regex::escape(start);
        let e_escaped = regex::escape(end);

        let regex = match kind {
            DelimiterKind::Inline => format!("{s_escaped}.*?{e_escaped}"),
            _ => format!("(?s){s_escaped}.*?{e_escaped}"),
        };
        let regex = Regex::new(&regex).expect("Failed to compile regular expression!");

        Delimiters {
            start,
            end,
            kind,
            regex,
            phantom: PhantomData,
        }
    }

    /// Same as [`Delimiters::new()`], but uses a user-provided regex instead of auto-generating one.
    pub fn new_with_regex(
        start: &'static str,
        end: &'static str,
        kind: DelimiterKind,
        regex: &'a str,
    ) -> Self {
        let regex = Regex::new(regex).expect("Failed to compile regular expression!");

        Delimiters {
            start,
            end,
            kind,
            regex,
            phantom: PhantomData,
        }
    }

    /// Parses the provided input into instances of [`Delimited`] using the "template"
    /// defined by the instance.
    ///
    /// Note that this parsing is infallible, and should always produce well-formed
    /// results *given that* the parsing regular expression is itself well-formed.
    #[allow(dead_code)]
    pub fn parse_from(&self, source: &'a str) -> Vec<Delimited> {
        self.parse_iter(source).collect()
    }

    /// Parses the provided input into instances of [`T`] by first parsing it into
    /// instances of [`Delimited`], then fallibly converting to [`T`].
    #[allow(dead_code)]
    pub fn parse_into<T>(&self, source: &'a str) -> Result<Vec<T>>
    where
        T: TryFrom<Delimited<'a>, Error = Report>,
    {
        self.parse_iter(source).map(|d| T::try_from(d)).collect()
    }

    /// Generates a parsing iterator for arbitrary consumption.
    pub fn parse_iter(&self, source: &'a str) -> impl Iterator<Item = Delimited<'a>> + '_ {
        self.regex.find_iter(source).map(|m| match self.kind {
            DelimiterKind::Inline => self.parse_inline(m),
            DelimiterKind::Multiline => self.parse_multiline(m),
            DelimiterKind::Raw => self.parse_raw(m),
        })
    }

    pub fn expand(
        &self,
        source: &mut String,
        mut replacer: impl FnMut(Delimited) -> Result<String>,
    ) -> Result<()> {
        let mut targets = self.parse_iter(source).peekable();

        if targets.peek().is_none() {
            return Ok(());
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

        source.replace_range(.., &buffer);
        source.shrink_to_fit();

        Ok(())
    }

    fn parse_inline(&self, m: Match<'a>) -> Delimited<'a> {
        // Inline structures are relatively simple;
        // they just need their surrounding delimiters trimmed,
        // followed by a regular whitespace trim for good measure.
        let contents = m
            .as_str()
            .trim_start_matches(self.start)
            .trim_end_matches(self.end)
            .trim();

        Delimited {
            range: m.start()..m.end(),
            token: None,
            contents,
            kind: self.kind,
        }
    }

    fn parse_multiline(&self, m: Match<'a>) -> Delimited<'a> {
        // Multiline structures are more complex than inline ones,
        // but we start out the same by trimming the surrounding delimiters.
        let contents = m
            .as_str()
            .trim_start_matches(self.start)
            .trim_end_matches(self.end);

        // Multiline structures can potentially have "tokens" - essentially
        // arbitrary text that exists after the opening delimiter
        // but before a newline.
        // (A good example is language extensions in Markdown codeblocks.)
        let (token, contents) = contents.split_once('\n').unwrap_or(("", contents));

        // Trimming the contents takes place after token extraction,
        // so no whitespace or newlines are missed.
        let contents = contents.trim();

        Delimited {
            range: m.start()..m.end(),
            token: Some(token),
            contents,
            kind: self.kind,
        }
    }

    fn parse_raw(&self, m: Match<'a>) -> Delimited<'a> {
        // Raw structures are very simple; they're just the match!
        Delimited {
            range: m.start()..m.end(),
            token: None,
            contents: m.as_str(),
            kind: self.kind,
        }
    }
}

/// Represents a delimited structure extracted from textual data.
#[derive(Debug)]
pub struct Delimited<'a> {
    /// The range the structure occupied in the original data.
    pub range: Range<usize>,
    /// The token (if applicable) present after the start delimiter,
    /// but before a newline.
    pub token: Option<&'a str>,
    /// The contents of the structure.
    pub contents: &'a str,
    /// The delimiter type.
    pub kind: DelimiterKind,
}

impl<'a> Ranged for Delimited<'a> {
    fn range(&self) -> Range<usize> {
        self.range.clone()
    }
}

#[cfg(test)]
mod inline {
    use super::*;

    #[test]
    fn one() {
        let source = r#"
            Here is some text with inline data: $`\frac{1}{2}`$.
        "#;

        let del = Delimiters::new("$`", "`$", DelimiterKind::Inline);
        let math = &del.parse_from(source)[0];

        assert_eq!(math.contents, r#"\frac{1}{2}"#);
    }

    #[test]
    fn many_one_line() {
        let source = r#"
            0.5 is equal to $`\frac{1}{2}`$, and 9 is equal to $`3^2`$.
        "#;

        let del = Delimiters::new("$`", "`$", DelimiterKind::Inline);
        let maths = del.parse_from(source);

        assert_eq!(maths.len(), 2);

        assert_eq!(maths[0].contents, r#"\frac{1}{2}"#);
        assert_eq!(maths[1].contents, r#"3^2"#);
    }

    #[test]
    fn many_multi_line() {
        let source = r#"
            0.5 is equal to $`\frac{1}{2}`$.
            9 is equal to $`3^2`$.
            2 is equal to $`\sqrt{4}`$.
        "#;

        let del = Delimiters::new("$`", "`$", DelimiterKind::Inline);
        let maths = del.parse_from(source);

        assert_eq!(maths.len(), 3);

        assert_eq!(maths[0].contents, r#"\frac{1}{2}"#);
        assert_eq!(maths[1].contents, r#"3^2"#);
        assert_eq!(maths[2].contents, r#"\sqrt{4}"#);
    }
}

#[cfg(test)]
mod multiline {
    use super::*;

    #[test]
    fn one() {
        let source = r#"
            Code block:
            ```rs
            println!("Hello, world!");
            ```
        "#;

        let del = Delimiters::new("```", "```", DelimiterKind::Multiline);
        let block = &del.parse_from(source)[0];

        assert_eq!(block.token, Some("rs"));
        assert_eq!(block.contents, "println!(\"Hello, world!\");");
    }

    #[test]
    fn many() {
        let source = r#"
            Code block 1:
            ```rs
            println!("Hello, world!");
            ```

            Code block 2:
            ```py
            print("Hello, world!")
            ```

            Code block 3:
            ```c
            printf("Hello, world!");
            ```
        "#;

        let del = Delimiters::new("```", "```", DelimiterKind::Multiline);
        let blocks = del.parse_from(source);

        assert_eq!(blocks.len(), 3);

        assert_eq!(blocks[0].token, Some("rs"));
        assert_eq!(blocks[0].contents, r#"println!("Hello, world!");"#);

        assert_eq!(blocks[1].token, Some("py"));
        assert_eq!(blocks[1].contents, r#"print("Hello, world!")"#);

        assert_eq!(blocks[2].token, Some("c"));
        assert_eq!(blocks[2].contents, r#"printf("Hello, world!");"#);
    }
}

#[cfg(test)]
mod raw {
    use super::*;

    #[test]
    fn one() {
        let source = r#"
            An inline shortcode:
            {% sci youtube id="foobar" %}
        "#;

        let del = Delimiters::new("{% sci ", " %}", DelimiterKind::Raw);
        let code = &del.parse_from(source)[0];

        assert_eq!(code.contents, r#"{% sci youtube id="foobar" %}"#)
    }

    #[test]
    fn many() {
        let source = r#"
            Some inline shortcodes:
            {% sci youtube id="foo" %}
            {% sci youtube id="bar" %}
            {% sci youtube id="baz" %}
        "#;

        let del = Delimiters::new("{% sci ", " %}", DelimiterKind::Raw);
        let codes = del.parse_from(source);

        assert_eq!(codes.len(), 3);

        assert_eq!(codes[0].contents, r#"{% sci youtube id="foo" %}"#);
        assert_eq!(codes[1].contents, r#"{% sci youtube id="bar" %}"#);
        assert_eq!(codes[2].contents, r#"{% sci youtube id="baz" %}"#);
    }

    #[test]
    fn complex() {
        // We can't use a raw string because it captures editor tabs, making assertions annoying.
        let source =
            "Here is a block shortcode:\n{% sc code %}\nprintln!(\"Hello, world!\");\n{% endsc %}";

        let del = Delimiters::new("{% sc ", "{% endsc %}", DelimiterKind::Raw);
        let code = &del.parse_from(source)[0];

        assert_eq!(
            code.contents,
            "{% sc code %}\nprintln!(\"Hello, world!\");\n{% endsc %}"
        )
    }
}
