//! This module contains a `nom` parser for a small subset of the Minijinja language, designed to extract dependency filenames.

use nom::branch::alt;
use nom::bytes::complete::*;
use nom::character::complete::*;
use nom::combinator::{eof, not};
use nom::multi::{many0, many0_count, separated_list0};
use nom::sequence::{delimited, pair, preceded, tuple};

use super::{to_report, trim, EyreResult, Input, Result};

#[derive(Debug, PartialEq)]
/// One or many dependencies parsed from a Minijinja template.
pub enum Dependency<'i> {
    Single(&'i str),
    Vector(Vec<&'i str>),
    Ignored,
}

impl<'i> Dependency<'i> {
    /// Attempts to parse dependencies from the provided input until exhaustion.
    pub fn parse_many(input: Input<'i>) -> EyreResult<impl Iterator<Item = &'i str>> {
        many0(Self::parse_one)(input)
            .map(|(_, o)| {
                o.into_iter().flat_map(|dep| match dep {
                    Self::Single(path) => vec![path].into_iter(),
                    Self::Vector(vec) => vec.into_iter(),
                    Self::Ignored => Vec::new().into_iter(),
                })
            })
            .map_err(to_report)
    }

    fn parse_one(input: Input<'i>) -> Result<Self> {
        alt((Self::parse_dep, Self::skip_ignored))(input)
    }

    /// Attempts to parse a single dependency from the provided input.
    fn parse_dep(input: Input<'i>) -> Result<Self> {
        alt((
            Self::parse_by_keyword("extends"),
            Self::parse_by_keyword("include"),
            Self::parse_by_keyword("import"),
            Self::parse_by_keyword("from"),
        ))(input)
    }

    /// Attempts to parse a stand-alone dependency filename from the provided input.
    ///
    /// Example: `"footer.html"`.
    fn parse_single(input: Input<'i>) -> Result<&'i str> {
        alt((
            delimited(
                tag("\""),
                escaped(is_not(r#"\""#), '\\', one_of(r#"""#)),
                tag("\""),
            ),
            delimited(
                tag("'"),
                escaped(is_not(r"\'"), '\\', one_of(r#"'"#)),
                tag("'"),
            ),
        ))(input)
    }

    /// Attempts to parse an array/set of dependency filenames from the provided input.
    ///
    /// Example: `["a.html", "b.html", "c.html"]`.
    fn parse_vector(input: Input<'i>) -> Result<Self> {
        delimited(
            tag("["),
            separated_list0(tag(","), trim(Self::parse_single)),
            tag("]"),
        )(input)
        .map(|(i, o)| (i, Self::Vector(o)))
    }

    /// Attempts to skip over any text that does not contain a dependency.
    fn skip_ignored(input: Input<'i>) -> Result<Self> {
        let munch_plain = tuple((
            anychar,
            not(alt((|i| Self::parse_dep(i).map(|(i, _)| (i, "")), eof))),
        ));

        let (_, count) = many0_count(munch_plain)(input)?;

        take(count + 1)(input).map(|(i, _)| (i, Self::Ignored))
    }

    /// Higher-order parser for Minijinja statements.
    ///
    /// Because all the statements we're looking to parse share a lot of syntax in common (like delimiters),
    /// we can just pass in the keyword we're looking for and share all that logic, rather than
    /// duplicating it for each statement.
    fn parse_by_keyword(kw: &'static str) -> impl FnMut(Input<'i>) -> Result<Dependency<'i>> {
        move |i| {
            delimited(
                pair(trim(tag("{%")), trim(tag(kw))),
                alt((Self::parse_vector, |i| {
                    Self::parse_single(i).map(|(i, o)| (i, Self::Single(o)))
                })),
                rest,
            )(i)
        }
    }
}

/// Attempt to take from the provided input until the end of a Minijinja statement is reached
/// (indicated by `%}`.)
fn rest(input: Input<'_>) -> Result<&str> {
    preceded(take_until("%}"), tag("%}"))(input)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_extends() {
        let input = r#"{% extends "base.html" %}"#;

        let (_, o) = Dependency::parse_by_keyword("extends")(input).unwrap();

        assert_eq!(o, Dependency::Single("base.html"))
    }

    #[test]
    fn parse_include() {
        let mut parser = Dependency::parse_by_keyword("include");

        let input_a = r#"{% include "header.html" %}"#;
        let input_b = r#"{% include 'customization.html' ignore missing %}"#;
        let input_c = r#"{% include ['page_detailed.html', 'page.html'] %}"#;
        let input_d = r#"{% include ['special_sidebar.html', 'sidebar.html'] ignore missing %}"#;

        let (_, out_a) = parser(input_a).unwrap();
        let (_, out_b) = parser(input_b).unwrap();
        let (_, out_c) = parser(input_c).unwrap();
        let (_, out_d) = parser(input_d).unwrap();

        let cmp_c = vec!["page_detailed.html", "page.html"];
        let cmp_d = vec!["special_sidebar.html", "sidebar.html"];

        assert_eq!(out_a, Dependency::Single("header.html"));
        assert_eq!(out_b, Dependency::Single("customization.html"));
        assert_eq!(out_c, Dependency::Vector(cmp_c));
        assert_eq!(out_d, Dependency::Vector(cmp_d));
    }

    #[test]
    fn parse_import_sub() {
        let mut parser = Dependency::parse_by_keyword("from");

        let input_a = r#"{% from "my_template.html" import my_macro, my_variable %}"#;
        let input_b = r#"{% from "my_template.html" import my_macro as other_name %}"#;

        let (_, out_a) = parser(input_a).unwrap();
        let (_, out_b) = parser(input_b).unwrap();

        assert_eq!(out_a, Dependency::Single("my_template.html"));
        assert_eq!(out_b, Dependency::Single("my_template.html"));
    }

    #[test]
    fn parse_import_full() {
        let input = r#"{% import "my_template.html" as helpers %}"#;

        let (_, o) = Dependency::parse_by_keyword("import")(input).unwrap();

        assert_eq!(o, Dependency::Single("my_template.html"));
    }

    #[test]
    fn parse_many() {
        let test_template = indoc::indoc! {r#"
            Here is our test template!

            {% extends "base.html" %}
            {% include 'header.html' %}
            {% include 'customization.html' ignore missing %}

            ... Some other template stuff we don't care about ...

            {% include ['page_detailed.html', 'page.html'] %}
            {% include ['special_sidebar.html', 'sidebar.html'] ignore missing %}
            {% from "my_template.html" import my_macro, my_variable %}

            ... Some more template stuff we just skip over ...

            {% from "my_template.html" import my_macro as other_name %}
            {% import "my_template.html" as helpers %}
        "#};

        let found: Vec<_> = Dependency::parse_many(test_template).unwrap().collect();

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
