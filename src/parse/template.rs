use nom::{
    branch::alt,
    combinator::{not, eof},
};

use nom::bytes::complete::*;
use nom::character::complete::*;

use nom::multi::{
    many0_count,
    separated_list0,
};

use nom::sequence::{
    pair,
    delimited,
    preceded,
    tuple
};

use super::{Input, Result, trim};

pub type Paths<'i> = Vec<Dependency<'i>>;

#[derive(Debug, PartialEq)]
pub enum Dependency<'i> {
    Single(&'i str),
    Vector(Paths<'i>)
}

impl<'i> Dependency<'i> {
    pub fn parse_many(_input: Input<'i>) -> Result<Vec<&'i str>> {
        todo!()
    }

    fn parse_path(input: Input<'i>) -> Result<Self> {
        alt((
            Self::parse_vector,
            Self::parse_single
        ))(input)
    }

    fn parse_single(input: Input<'i>) -> Result<Self> {
        alt((
            delimited(
                tag("\""),
                escaped(is_not(r#"\""#), '\\', one_of(r#"""#)),
            tag("\"")
            ),
            delimited(
                tag("'"),
                escaped(is_not(r#"\'"#), '\\', one_of(r#"'"#)),
                tag("'")
            )
        ))(input)
        .map(|(i, o)| {
            (i, Self::Single(o))
        })
    }

    fn parse_vector(input: Input<'i>) -> Result<Self> {
        delimited(
            tag("["),
            separated_list0(tag(","), trim(Self::parse_single)),
            tag("]")
        )(input)
        .map(|(i, o)| {
            (i, Self::Vector(o))
        })
    }

    fn parse_dep(input: Input<'i>) -> Result<Self> {
        alt((
            Self::parse_by_keyword("extends"),
            Self::parse_by_keyword("include"),
            Self::parse_by_keyword("import"),
            Self::parse_by_keyword("from")
        ))(input)
    }

    fn skip_ignored(input: Input<'i>) -> Result<()> {
        let munch_plain = tuple((
            anychar,
            not(alt((
                |i| Self::parse_path(i).map(|(i, _)| (i, "")),
                eof
            )))
        ));
        
        let (_, count) = many0_count(munch_plain)(input)?;

        take(count + 1)(input).map(|(i, _)| {
            (i, ())
        })
    }

    fn parse_by_keyword(kw: &'static str) -> impl FnMut(Input<'i>) -> Result<Dependency<'i>> {
        move |i| {
            delimited(
                pair(trim(tag("{%")), trim(tag(kw))),
                Self::parse_path,
                rest
            )(i)
        }
    }
}

fn rest(input: Input<'_>) -> Result<&str> {
    preceded(
        take_until("%}"),
        tag("%}")
    )(input)
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

        let input_a = r#"{% include 'header.html' %}"#;
        let input_b = r#"{% include 'customization.html' ignore missing %}"#;
        let input_c = r#"{% include ['page_detailed.html', 'page.html'] %}"#;
        let input_d = r#"{% include ['special_sidebar.html', 'sidebar.html'] ignore missing %}"#;
        
        let (_, out_a) = parser(input_a).unwrap();
        let (_, out_b) = parser(input_b).unwrap();
        let (_, out_c) = parser(input_c).unwrap();
        let (_, out_d) = parser(input_d).unwrap();

        let cmp_c = vec![Dependency::Single("page_detailed.html"), Dependency::Single("page.html")];
        let cmp_d = vec![Dependency::Single("special_sidebar.html"), Dependency::Single("sidebar.html")];

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
}