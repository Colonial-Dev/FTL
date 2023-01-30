mod content;
mod template;

use nom::{
    IResult,
    error::{ErrorKind, ParseError}
};

pub use content::*;
pub use template::*;
pub use self::toml::find_toml;

type Input<'i> = &'i str;
type Result<'i, T> = IResult<Input<'i>, T, (Input<'i>, ErrorKind)>;

/// Wraps the given parser, consuming all whitespace before and after it.
/// Taken from the Nom recipes document.
fn trim<'a, F, O, E>(inner: F) -> impl FnMut(&'a str) -> IResult<&'a str, O, E> where
    E: ParseError<&'a str>,
    F: FnMut(&'a str) -> IResult<&'a str, O, E>,
{
    use nom::sequence::delimited;
    use nom::character::complete::multispace0;

    delimited(
        multispace0,
        inner,
        multispace0
    )
}

// The parsers below were too simply to get their own files, but due to how pest-derive works
// they still need to be in separate scopes.

mod toml {
    use std::ops::Range;
    use pest::Parser;
    use crate::prelude::*;

    pub fn find_toml(text: &str) -> Result<(Range<usize>, &str)> {
        let span = TomlFinder::parse(Rule::toml_block, text)?
            .next()
            .unwrap()
            .as_span();

        let range = span.start() .. span.end();
        let body = span
            .as_str()
            .trim_start_matches("+++")
            .trim_end_matches("+++");

        Ok((range, body))
    }

    #[derive(pest_derive::Parser)]
    #[grammar = "parse/grammars/toml.pest"]
    struct TomlFinder;

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn toml_finding() {
            let test_page = indoc! {r#"
                +++
                key = "value"
                setting = true
                +++
            "#};

            let (_, toml) = find_toml(test_page).unwrap();

            assert_eq!(toml, "\nkey = \"value\"\nsetting = true\n");
        }
    }
}