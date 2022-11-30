mod content;

use nom::{
    IResult,
    error::{ErrorKind, ParseError}
};

pub use content::*;
pub use template::find_dependencies;
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

mod template {
    use pest::Parser;

    use crate::prelude::*;

    pub fn find_dependencies(template: &str) -> Result<impl Iterator<Item = &str>> {
        let iterator = DependencyFinder::parse(Rule::template, template)?
            .filter(|pair| pair.as_rule() == Rule::string)
            .map(|pair| {
                pair
                    .as_str()
                    .trim_start_matches('"')
                    .trim_start_matches('\'')
                    .trim_end_matches('"')
                    .trim_end_matches('\'')
            });

        Ok(iterator)
    }

    #[derive(pest_derive::Parser)]
    #[grammar = "parse/grammars/template.pest"]
    struct DependencyFinder;

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn dependency_finding() {
            let test_template = indoc! {r#"
                {% extends "base.html" %}
                {% include 'header.html' %}
                {% include 'customization.html' ignore missing %}
                {% include ['page_detailed.html', 'page.html'] %}
                {% include ['special_sidebar.html', 'sidebar.html'] ignore missing %}
                {% from "my_template.html" import my_macro, my_variable %}
                {% from "my_template.html" import my_macro as other_name %}
                {% import "my_template.html" as helpers %}
            "#};

            let found: Vec<_> = find_dependencies(test_template).unwrap().collect();

            assert_eq!("base.html", found[0]);
            assert_eq!("header.html", found[1]);
            assert_eq!("customization.html", found[2]);
            assert_eq!("page_detailed.html", found[3]);
            assert_eq!("page.html", found[4]);
            assert_eq!("special_sidebar.html", found[5]);
            assert_eq!("sidebar.html", found[6]);
            assert_eq!("my_template.html", found[7]);
            assert_eq!("my_template.html", found[8]);
            assert_eq!("my_template.html", found[9]);
        }
    }
}
