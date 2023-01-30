//! This module contains a nom parser for page content, such as shortcodes and codeblocks.
// TODO: Document everything here.

use nom::{
    branch::alt,
    combinator::{recognize, not, eof},
    number::complete::double,
    character::is_alphanumeric,
};

use nom::bytes::complete::*;
use nom::character::complete::*;

use nom::multi::{
    many0_count,
    separated_list0,
    many1
};

use nom::sequence::{
    pair,
    delimited,
    terminated,
    tuple
};

use super::{Input, Result, trim};

pub type Kwargs<'i> = Vec<Kwarg<'i>>;
pub type Literals<'i> = Vec<Literal<'i>>;

#[derive(Debug, PartialEq)]
pub struct Ident<'i>(&'i str);

#[derive(Debug, PartialEq)]
pub enum Literal<'i> {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    String(&'i str),
    Vector(Literals<'i>)
}

#[derive(Debug, PartialEq)]
pub struct Kwarg<'i> {
    pub ident: Ident<'i>,
    pub value: Literal<'i>
}

#[derive(Debug, PartialEq)]
pub struct Shortcode<'i> {
    pub ident: Ident<'i>,
    pub body: Option<&'i str>,
    pub args: Kwargs<'i>
}

#[derive(Debug, PartialEq)]
pub struct Codeblock<'i> {
    pub token: Option<&'i str>,
    pub body: &'i str
}

#[derive(Debug, PartialEq)]
pub struct Header<'i> {
    pub level: u8,
    pub ident: Option<&'i str>,
    pub classes: Vec<&'i str>
}

#[derive(Debug, PartialEq)]
pub enum Content<'i> {
    Plaintext(&'i str),
    Emojicode(&'i str),
    Shortcode(Shortcode<'i>),
    Codeblock(Codeblock<'i>),
    Header(Header<'i>)
}

impl<'i> Ident<'i> {
    /// Parses Rust-style identifiers; taken from the Nom recipes document.
    /// https://github.com/rust-bakery/nom/blob/main/doc/nom_recipes.md
    pub fn parse(input: Input<'i>) -> Result<Self> {
        recognize(
            pair(
                alt((
                    alpha1,
                    tag("_")
                )),
                many0_count(alt((
                    alphanumeric1,
                    tag("_")
                )))
            )
        )(input)
        .map(|(i, o)| {
            (i, Self(o))
        })
    }
}

impl<'i> Literal<'i> {
    pub fn parse(input: Input<'i>) -> Result<Self> {
        alt((
            Self::parse_number,
            Self::parse_boolean,
            Self::parse_string,
            Self::parse_vector
        ))(input)
    }

    fn parse_number(input: Input<'i>) -> Result<Self> {
        double(input).map(|(i, o)| {
            if o.fract() == 0.0 {
                (i, Self::Integer(o as i64))
            } else {
                (i, Self::Float(o))
            }
        })
    }

    fn parse_boolean(input: Input<'i>) -> Result<Self> {
        alt((
            tag("true"),
            tag("false")
        ))(input)
        .map(|(i, o)| {
            let boolean = o
                .parse::<bool>()
                .map(Self::Boolean)
                .expect("Literal should be a valid boolean.");
            (i, boolean)
        })
    }

    fn parse_string(input: Input<'i>) -> Result<Self> {
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
            (i, Self::String(o))
        })
    }

    fn parse_vector(input: Input<'i>) -> Result<Self> {
        delimited(
            tag("["),
            separated_list0(tag(","), trim(Self::parse)),
            tag("]")
        )(input)
        .map(|(i, o)| {
            (i, Self::Vector(o))
        })
    }
}

impl<'i> Kwarg<'i> {
    pub fn parse(input: Input<'i>) -> Result<Self> {
        tuple((
            Ident::parse,
            trim(tag("=")),
            Literal::parse
        ))(input)
        .map(|(i, (ident, _, value))| {
            (i, Self { ident, value })
        })
    }

    pub fn parse_many(input: Input<'i>) -> Result<Kwargs> {
        separated_list0(tag(","), trim(Self::parse))(input)
    }
}

impl<'i> Shortcode<'i> {
    pub fn parse(input: Input<'i>) -> Result<Self> {
        alt((
            Self::parse_inline,
            Self::parse_block
        ))(input)
    }

    fn parse_invocation(input: Input<'i>) -> Result<(Ident, Kwargs)> {
        tuple((
            Ident::parse,
            delimited(
                trim(tag("(")),
                Kwarg::parse_many,
                trim(tag(")"))
            )
        ))(input)
    }

    fn parse_inline(input: Input<'i>) -> Result<Self> {
        delimited(
            tag("{{"),
            trim(Self::parse_invocation),
            tag("}}")
        )(input)
        .map(|(i, o)| {
            (
                i,
                Self {
                    ident: o.0,
                    body: None,
                    args: o.1
                }
            )
        })
    }

    fn parse_block(input: Input<'i>) -> Result<Self> {
        tuple((
            delimited(
                tag("{%"),
                trim(Self::parse_invocation),
                tag("%}")
            ),
            terminated(
                trim(take_until("\n{% end %}")),
                tag("{% end %}")
            )
        ))(input)
        .map(|(i, (invocation, body))| {
            (
                i,
                Self {
                    ident: invocation.0,
                    body: Some(body),
                    args: invocation.1
                }
            )
        })
    }
}

impl<'i> Codeblock<'i> {
    pub fn parse(input: Input<'i>) -> Result<Self> {
        delimited(
            tag("```"),
            tuple((
                take_until("\n"),
                take_until("\n```")
            )),
            tag("\n```")
        )(input)
        .map(|(i, o)| {
            (
                i,
                Self {
                    token: match o.0.is_empty() {
                        false => Some(o.0),
                        true => None
                    },
                    body: o.1.trim()
                }
            )
        })
    }
}

impl<'i> Content<'i> {
    pub fn parse(input: Input<'i>) -> Result<Self> {
        alt((
            Self::parse_structure,
            Self::parse_plaintext
        ))(input)
    }

    pub fn parse_many(input: Input<'i>) -> Result<Vec<Self>> {
        many1(Self::parse)(input)
    }

    fn parse_structure(input: Input<'i>) -> Result<Self> {
        alt((
            Self::parse_emojicode,
            |i| Shortcode::parse(i).map(|(i, o)| (i, Self::Shortcode(o))),
            |i| Codeblock::parse(i).map(|(i, o)| (i, Self::Codeblock(o))),
            //|i| Header::parse(i).map(|(i, o)| (i, Self::Header(o))),
        ))(input)
    }

    fn parse_plaintext(input: Input<'i>) -> Result<Self> {
        let munch_plain = tuple((
            anychar,
            not(alt((
                |i| Self::parse_structure(i).map(|(i, _)| (i, "")),
                eof
            )))
        ));
        
        let (_, count) = many0_count(munch_plain)(input)?;

        take(count + 1)(input).map(|(i, o)| {
            (i, Self::Plaintext(o))
        })
    }

    fn parse_emojicode(input: Input<'i>) -> Result<Self> {
        delimited(
            tag(":"),
            take_till1(|c| {
                !is_alphanumeric(c as u8) && !"+-_".contains(c)
            }),
            tag(":")
        )(input)
        .map(|(i, o)| {
            (i, Self::Emojicode(o))
        })
    }
}

#[cfg(test)]
mod test_literals {
    use super::*;

    #[test]
    fn numbers() {
        let case_a = "1.5";
        let case_b = "0.42";
        let case_c = "2";

        let (_, case_a) = Literal::parse_number(case_a).unwrap();
        let (_, case_b) = Literal::parse_number(case_b).unwrap();
        let (_, case_c) = Literal::parse_number(case_c).unwrap();

        assert_eq!(case_a, Literal::Float(1.5));
        assert_eq!(case_b, Literal::Float(0.42));
        assert_eq!(case_c, Literal::Integer(2));
    }
    
    #[test]
    fn booleans() {
        let false_case = "false";
        let true_case = "true";

        let (_, false_case) = Literal::parse_boolean(false_case).unwrap();
        let (_, true_case) = Literal::parse_boolean(true_case).unwrap();

        assert_eq!(false_case, Literal::Boolean(false));
        assert_eq!(true_case, Literal::Boolean(true));
    }

    #[test]
    fn strings() {
        let double_quoted = r#""Hello, world!""#;
        let single_quoted = r#"'Hello, world!'"#;
        let with_escaped = r#""Hello, \" world!""#;

        let (_, double_quoted) = Literal::parse_string(double_quoted).unwrap();
        let (_, single_quoted) = Literal::parse_string(single_quoted).unwrap();
        let (_, with_escaped) = Literal::parse_string(with_escaped).unwrap();

        assert_eq!(double_quoted, Literal::String("Hello, world!"));
        assert_eq!(single_quoted, Literal::String("Hello, world!"));
        assert_eq!(with_escaped, Literal::String("Hello, \\\" world!"));
    }

    #[test]
    fn vectors() {
        let case_a = "[1, 2, 3]";
        let case_b = "[1, 3.5, \"Hola!\"]";

        let(_, case_a) = Literal::parse_vector(case_a).unwrap();
        let(_, case_b) = Literal::parse_vector(case_b).unwrap();

        let case_a_cmp = vec![1, 2, 3].into_iter().map(Literal::Integer).collect();
        let case_b_cmp = vec![Literal::Integer(1), Literal::Float(3.5), Literal::String("Hola!")];

        assert_eq!(case_a, Literal::Vector(case_a_cmp));
        assert_eq!(case_b, Literal::Vector(case_b_cmp));
    }
}

#[cfg(test)]
mod test_kwargs {
    use super::*;

    #[test]
    fn single() {
        let kwarg = "answer = 42";

        let (_, kwarg) = Kwarg::parse(kwarg).unwrap();

        assert_eq!(kwarg.ident.0, "answer");
        assert_eq!(kwarg.value, Literal::Integer(42));
    }

    #[test]
    fn many() {
        let kwargs = "answer = 42, sky = \"blue\", arr = [1, 2]";

        let (_, kwargs) = Kwarg::parse_many(kwargs).unwrap();

        assert_eq!(kwargs[0].ident.0, "answer");
        assert_eq!(kwargs[0].value, Literal::Integer(42));

        assert_eq!(kwargs[1].ident.0, "sky");
        assert_eq!(kwargs[1].value, Literal::String("blue"));

        assert_eq!(kwargs[2].ident.0, "arr");
        assert_eq!(kwargs[2].value, Literal::Vector(vec![Literal::Integer(1), Literal::Integer(2)]));
    }
}

#[cfg(test)]
mod test_shortcodes {
    use super::*;

    #[test]
    fn inline() {
        let code = "{{ invoke(answer = 42, text=\"Hello!\") }}";

        let (_, code) = Shortcode::parse(code).unwrap();

        assert_eq!(code.ident.0, "invoke");
        assert_eq!(code.body,  None);
        assert_eq!(code.args[0].ident.0, "answer");
        assert_eq!(code.args[0].value, Literal::Integer(42));
        assert_eq!(code.args[1].ident.0, "text");
        assert_eq!(code.args[1].value, Literal::String("Hello!"));
    }

    #[test]
    fn block() {
        let code = indoc::indoc! {"
            {% invoke(answer = 42) %}
            Block shortcode body text.
            {% end %}
        "};

        let (_, code) = Shortcode::parse(code).unwrap();

        assert_eq!(code.ident.0, "invoke");
        assert_eq!(code.body, Some("Block shortcode body text."));
        assert_eq!(code.args[0].ident.0, "answer");
        assert_eq!(code.args[0].value, Literal::Integer(42));
    }
}

#[cfg(test)]
mod test_codeblocks {
    use super::*;

    #[test]
    fn with_token() {
        let block = indoc::indoc! {"```rs
        panic!(\"oh no\");
        ```"};

        let (_, block) = Codeblock::parse(block).unwrap();

        assert_eq!(block.token, Some("rs"));
        assert_eq!(block.body, "panic!(\"oh no\");");
    }
    
    #[test]
    fn without_token() {
        let block = indoc::indoc! {"```
        Some boring plain text.
        ```"};

        let (_, block) = Codeblock::parse(block).unwrap();

        assert_eq!(block.token, None);
        assert_eq!(block.body, "Some boring plain text.");
    }
}

#[cfg(test)]
mod test_content {
    use super::*;

    #[test]
    fn emojicode() {
        let valid = ":eagle:";
        let invalid = ":hello there$:";

        let (_, valid) = Content::parse_emojicode(valid).unwrap();
        let _err = Content::parse_emojicode(invalid).unwrap_err();

        assert_eq!(valid, Content::Emojicode("eagle"));
    }

    #[test]
    fn exhaustive() {
        let page = indoc::indoc! {"
            Some plaintext here...

            {{ invoke(answer = 42) }}

            Some more plaintext here...

            {% invoke(block = true) %}
            Block shortcode body.
            {% end %}

            A codeblock:
            ```rs
            panic!(\"oh no\");
            ```

            An emoji - :eagle:
        "};

        let inline_sc = Shortcode {
            ident: Ident("invoke"),
            body: None,
            args: vec![Kwarg { ident: Ident("answer"), value: Literal::Integer(42)}]
        };

        let block_sc = Shortcode {
            ident: Ident("invoke"),
            body: Some("Block shortcode body."),
            args: vec![Kwarg { ident: Ident("block"), value: Literal::Boolean(true)}]
        };

        let codeblock = Codeblock {
            token: Some("rs"),
            body: "panic!(\"oh no\");"
        };

        let (out, page) = Content::parse_many(page).unwrap();

        assert!(out.is_empty());

        assert_eq!(page.len(), 9);
        assert_eq!(page[0], Content::Plaintext("Some plaintext here...\n\n"));
        assert_eq!(page[1], Content::Shortcode(inline_sc));
        assert_eq!(page[2], Content::Plaintext("\n\nSome more plaintext here...\n\n"));
        assert_eq!(page[3], Content::Shortcode(block_sc));
        assert_eq!(page[4], Content::Plaintext("\n\nA codeblock:\n"));
        assert_eq!(page[5], Content::Codeblock(codeblock));
        assert_eq!(page[6], Content::Plaintext("\n\nAn emoji - "));
        assert_eq!(page[7], Content::Emojicode("eagle"));
        assert_eq!(page[8], Content::Plaintext("\n"));
    }
}