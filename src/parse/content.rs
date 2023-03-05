//! This module contains a `nom` parser for page content, such as shortcodes and codeblocks.

use ahash::AHashMap;

use nom::{
    branch::alt,
    combinator::{recognize, not, eof, opt},
    number::complete::double,
    character::is_alphanumeric,
};

use nom::bytes::complete::*;
use nom::character::complete::*;

use nom::multi::{
    many0_count,
    separated_list0,
    many1,
    many_m_n
};

use nom::sequence::{
    pair,
    delimited,
    terminated,
    preceded,
    tuple
};

use serde::Serialize;

use super::{Input, Result, EyreResult, trim, to_report};

/// Type alias for a collection of (flattened) [`Kwarg`]s.
pub type Kwargs<'i> = AHashMap<&'i str, Literal<'i>>;

/// Type alias for a vector of [`Literal`]s.
pub type Literals<'i> = Vec<Literal<'i>>;

#[derive(Debug, PartialEq, Serialize)]
/// An identifier for kwargs.
/// Conforms to the same rules as Rust identifiers.
pub struct Ident<'i>(pub &'i str);

#[derive(Debug, PartialEq, Serialize)]
// Tags show up in templating output!
#[serde(untagged)]
/// A literal value - found in shortcode invocations,
/// used as the arg half of kwargs.
pub enum Literal<'i> {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    String(&'i str),
    Vector(Literals<'i>)
}

#[derive(Debug, PartialEq, Serialize)]
/// A keyword argument. Consists of an identifier and a literal value.
pub struct Kwarg<'i> {
    pub ident: Ident<'i>,
    pub value: Literal<'i>
}

#[derive(Debug, PartialEq, Serialize)]
/// A parsed shortcode invocation, including name, body and arguments.
pub struct Shortcode<'i> {
    /// The name of the shortcode.
    pub name: &'i str,
    /// The body of the shortcode - only present for block invocations.
    pub body: Option<&'i str>,
    /// The keyword arguments provided in the invocation, if any.
    pub args: Kwargs<'i>
}

#[derive(Debug, PartialEq, Serialize)]
/// A parsed Markdown codeblock, including language token and body.
pub struct Codeblock<'i> {
    /// The codeblock's language token, if any. 
    /// (Example: ```rs)
    pub token: Option<&'i str>,
    /// The codeblock's body.
    pub body: &'i str
}

#[derive(Debug, PartialEq, Serialize)]
/// A parsed Markdown header, including level, title, anchor ident and CSS classes.
pub struct Header<'i> {
    /// The header level, 1-6.
    pub level: u8,
    /// The title of the header.
    /// 
    /// Example: `### Hello, World` - the title is "Hello, World".
    pub title: &'i str,
    /// The user-provided anchor identifier, if any.
    /// 
    /// Example: `# Foo {#foo_header}` - the identifier is "foo_header".
    pub ident: Option<&'i str>,
    /// The user-provided CSS class(es), if any.
    /// 
    /// Example: `# Bar {#bar .italic .xl}` - the classes are "italic" and "xl".
    pub classes: Vec<&'i str>
}

#[derive(Debug, PartialEq)]
/// Enum representing the different content/"structures" parsed from a page.
pub enum Content<'i> {
    /// Regular Markdown text, with no special features. Captured as-is.
    Plaintext(&'i str),
    /// An emoji shortcode, such as `:smile:`.
    Emojicode(&'i str),
    /// A parsed shortcode invocation.
    Shortcode(Shortcode<'i>),
    /// A parsed Markdown codeblock.
    Codeblock(Codeblock<'i>),
    /// A parsed Markdown header.
    Header(Header<'i>)
}

impl<'i> Ident<'i> {
    /// Parses Rust-style identifiers; taken from the Nom recipes document.
    /// <https://github.com/rust-bakery/nom/blob/main/doc/nom_recipes.md>
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

impl std::fmt::Display for Ident<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<'i> Literal<'i> {
    /// Attempts to parse a single literal from the provided input.
    pub fn parse(input: Input<'i>) -> Result<Self> {
        alt((
            Self::parse_number,
            Self::parse_boolean,
            Self::parse_string,
            Self::parse_vector
        ))(input)
    }

    /// Attempts to parse a numeric literal from the provided input.
    /// 
    /// Numeric literals are always parsed as double-precision floats.
    /// Literals with no fractional component are demoted to integers post-hoc.
    fn parse_number(input: Input<'i>) -> Result<Self> {
        double(input).map(|(i, o)| {
            if o.fract() == 0.0 {
                (i, Self::Integer(o as i64))
            } else {
                (i, Self::Float(o))
            }
        })
    }

    /// Attempts to parse a boolean literal from the provided input.
    /// 
    /// This parser is quite strict - it only accepts the (case-insensitive) 
    /// literal strings "true" and "false".
    fn parse_boolean(input: Input<'i>) -> Result<Self> {
        alt((
            tag_no_case("true"),
            tag_no_case("false")
        ))(input)
        .map(|(i, o)| {
            let boolean = o
                .parse::<bool>()
                .map(Self::Boolean)
                .expect("Literal should be a valid boolean.");
            (i, boolean)
        })
    }

    /// Attempts to parse a string literal from the provided input.
    /// 
    /// String literals are delimited with single or double quotes.
    /// Delimiter escaping is supported.
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

    /// Attempts to parse a vector of literals from the provided input.
    /// 
    /// Vectors take the form of `[value, value, value]`. This parser will
    /// parse any literal as part of a vector - they need not be homogenous.
    /// Nested vectors are also supported.
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
    /// Attempts to parse a kwarg from the provided input.
    /// 
    /// Kwargs take the form of `ident = literal` (whitespace-insensitive.)
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

    /// Attempts to parse many kwargs from the provided input.
    /// 
    /// Kwarg sets take the form of `kwarg, kwarg, kwarg` (whitespace-insensitive.)
    /// 
    /// This parser does not directly return a collection of [`Kwarg`]s, but instead
    /// flattens their data into a hashmap.
    pub fn parse_many(input: Input<'i>) -> Result<Kwargs> {
        separated_list0(
            tag(","),
            trim(Self::parse)
        )(input)
        .map(|(i, o)| {
            (
                i,
                o.into_iter().map(|kwarg| (kwarg.ident.0, kwarg.value)).collect()
            )
        })
    }
}

impl<'i> Shortcode<'i> {
    /// Attempts to parse a single shortcode (inline or block) from the provided input.
    /// 
    /// Inline shortcodes take the form of `{{ ident(kwargs) }}`.
    /// 
    /// Block shortcodes take the form of `{% ident(kwargs) %} /* ... body ... */ {% end %}`.
    pub fn parse(input: Input<'i>) -> Result<Self> {
        alt((
            Self::parse_inline,
            Self::parse_block
        ))(input)
    }

    /// Attempts to parse a shortcode invocation from the provided input.
    /// 
    /// Invocations are where the name and arguments are specified; they do not include
    /// delimiters like `{{`.
    fn parse_invocation(input: Input<'i>) -> Result<(&'i str, Kwargs)> {
        tuple((
            take_until("("),
            delimited(
                trim(tag("(")),
                Kwarg::parse_many,
                trim(tag(")"))
            )
        ))(input)
    }

    /// Attempts to parse a single inline shortcode from the provided input.
    /// 
    /// See [`Shortcode::parse`].
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
                    name: o.0,
                    body: None,
                    args: o.1
                }
            )
        })
    }

    /// Attempts to parse a single block shortcode from the provided input.
    /// 
    /// See [`Shortcode::parse`].
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
                    name: invocation.0,
                    body: Some(body),
                    args: invocation.1
                }
            )
        })
    }
}

impl<'i> Codeblock<'i> {
    /// Attempts to parse a single Markdown codeblock from the provided input.
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

impl<'i> Header<'i> {
    /// Attempts to parse a single Markdown header from the provided input.
    pub fn parse(input: Input<'i>) -> Result<Self> {
        tuple((
            Self::parse_level,
            Self::parse_title,
            opt(Self::parse_extra)
        ))(input)
        .map(|(i, (level, title, extra))| {
            (i, match extra {
                Some((ident, classes)) => Self {
                    level,
                    title,
                    ident,
                    classes: classes.unwrap_or_default()
                },
                None => Self {
                    level,
                    title,
                    ident: None,
                    classes: Vec::new()
                }
            })
        })
    }

    /// Attempts to parse the "level" of a Markdown header from the provided input.
    /// 
    /// This is simply how many pound signs are used - between one and six.
    fn parse_level(input: Input<'i>) -> Result<u8> {
        many_m_n(1, 6, char('#'))(input)
            .map(|(i, o)| {
                (i, o.len() as u8)
            })
    }

    /// Attempts to parse the title of a Markdown header from the provided input.
    fn parse_title(input: Input<'i>) -> Result<&'i str> {
        let munch_plain = tuple((
            anychar,
            not(alt((
                |i| Self::parse_extra(i).map(|(i, _)| (i, "")),
                is_a("\r\n"),
                eof
            )))
        ));
        
        let (_, count) = many0_count(munch_plain)(input)?;

        take(count + 1)(input).map(|(i, o)| (i, o.trim()))
    }

    /// Attempts to parse "extra" details from a Markdown header, such as its anchor ident and/or classes.
    fn parse_extra(input: Input<'i>) -> Result<(Option<&'i str>, Option<Vec<&'i str>>)> {
        delimited(
            tag("{"),
            pair(
                opt(preceded(
                    char('#'),
                    is_not("}.")
                )),
                opt(preceded(
                    char('.'),
                    separated_list0(char('.'), trim(is_not("}. ")))
                ))
            ),
            tag("}")
        )(input)
        .map(|(i, (ident, classes))| {
            (i, (ident.map(str::trim), classes))
        })
    }
}

impl<'i> Content<'i> {
    /// Attempts to parse content from the provided input until it is exhausted.
    pub fn parse_many(input: Input<'i>) -> EyreResult<Vec<Self>> {
        many1(Self::parse_one)(input)   
            .map(|(_, o)| o)
            .map_err(to_report)
    }

    /// Attempts to parse a single piece of content from the provided input.
    fn parse_one(input: Input<'i>) -> Result<Self> {
        alt((
            Self::parse_structure,
            Self::parse_plaintext
        ))(input)
    }

    /// Attempts to parse a "structure" from the provided input, such as a shortcode
    /// invocation or codeblock.
    fn parse_structure(input: Input<'i>) -> Result<Self> {
        alt((
            Self::parse_emojicode,
            |i| Shortcode::parse(i).map(|(i, o)| (i, Self::Shortcode(o))),
            |i| Codeblock::parse(i).map(|(i, o)| (i, Self::Codeblock(o))),
            |i| Header::parse(i).map(|(i, o)| (i, Self::Header(o))),
        ))(input)
    }

    /// Attempts to parse (really, capture) "plaintext" from the provided input.
    /// 
    /// Plaintext is just regular Markdown text, with nothing of particular note in it.
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

    /// Attempts to parse an emoji shortcode from the provided input.
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

        assert_eq!(kwargs["answer"], Literal::Integer(42));
        assert_eq!(kwargs["sky"], Literal::String("blue"));
        assert_eq!(kwargs["arr"], Literal::Vector(vec![Literal::Integer(1), Literal::Integer(2)]));
    }
}

#[cfg(test)]
mod test_shortcodes {
    use super::*;

    #[test]
    fn inline() {
        let code = "{{ invoke(answer = 42, text=\"Hello!\") }}";

        let (_, code) = Shortcode::parse(code).unwrap();

        assert_eq!(code.name, "invoke");
        assert_eq!(code.body,  None);
        assert_eq!(code.args["answer"], Literal::Integer(42));
        assert_eq!(code.args["text"], Literal::String("Hello!"));
    }

    #[test]
    fn block() {
        let code = indoc::indoc! {"
            {% invoke(answer = 42) %}
            Block shortcode body text.
            {% end %}
        "};

        let (_, code) = Shortcode::parse(code).unwrap();

        assert_eq!(code.name, "invoke");
        assert_eq!(code.body, Some("Block shortcode body text."));
        assert_eq!(code.args["answer"], Literal::Integer(42));
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
mod test_headers {
    use super::*;

    #[test]
    fn simple() {
        let input = "## I'm a header!";

        let (_, output) = Header::parse(input).unwrap();

        assert_eq!(output, Header {
            level: 2,
            title: "I'm a header!",
            ident: None,
            classes: Vec::new()
        })
    }

    #[test]
    fn complex() {
        let input = "# I'm a header! {#header .blue .bold}";

        let (_, output) = Header::parse(input).unwrap();

        assert_eq!(output, Header {
            level: 1,
            title: "I'm a header!",
            ident: Some("header"),
            classes: vec!["blue", "bold"]
        })
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
            ## Let's exhaust the possibilities! {#exhaust .some_class .another_class}
        
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

        let header = Header {
            level: 2,
            title: "Let's exhaust the possibilities!",
            ident: Some("exhaust"),
            classes: vec!["some_class", "another_class"]
        };

        let inline_sc = Shortcode {
            name: "invoke",
            body: None,
            args: AHashMap::from([("answer", Literal::Integer(42))])
        };

        let block_sc = Shortcode {
            name: "invoke",
            body: Some("Block shortcode body."),
            args: AHashMap::from([("block", Literal::Boolean(true))])
        };

        let codeblock = Codeblock {
            token: Some("rs"),
            body: "panic!(\"oh no\");"
        };

        let page = Content::parse_many(page).unwrap();

        assert_eq!(page.len(), 10);
        assert_eq!(page[0], Content::Header(header));
        assert_eq!(page[1], Content::Plaintext("\n\nSome plaintext here...\n\n"));
        assert_eq!(page[2], Content::Shortcode(inline_sc));
        assert_eq!(page[3], Content::Plaintext("\n\nSome more plaintext here...\n\n"));
        assert_eq!(page[4], Content::Shortcode(block_sc));
        assert_eq!(page[5], Content::Plaintext("\n\nA codeblock:\n"));
        assert_eq!(page[6], Content::Codeblock(codeblock));
        assert_eq!(page[7], Content::Plaintext("\n\nAn emoji - "));
        assert_eq!(page[8], Content::Emojicode("eagle"));
        assert_eq!(page[9], Content::Plaintext("\n"));
    }
}