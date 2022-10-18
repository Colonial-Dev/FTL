use std::ops::Range;

use nom::IResult;
use nom::bytes::complete::{tag, take_until};
use nom::multi::many0;
use nom_locate::position;

use crate::prelude::*;

use super::{Span, Ranged};

#[derive(Debug)]
pub struct Codeblock<'a> {
    pub token: &'a str,
    pub code: &'a str,
    pub range: Range<usize>,
}

impl<'a> Codeblock<'a> {
    pub fn parse_many(source: &'a str) -> Result<Vec<Codeblock>> {
        let source = Span::from(source);
        let (_, blocks) = parse_codeblocks(source)
            .map_err(|ierr| {
                eyre!("Codeblock parsing error.")
                    .section(ierr.to_string())
            })?;

        Ok(blocks)
    }
}

impl<'a> Ranged for Codeblock<'a> {
    fn range(&self) -> Range<usize> {
        self.range.clone()
    }
}

fn parse_codeblocks(s: Span) -> IResult<Span, Vec<Codeblock>> {
    many0(parse_codeblock)(s)
}

fn parse_codeblock(s: Span) -> IResult<Span, Codeblock> {
    let get_token = take_until("\n");
    let get_code = take_until("```");

    let (s, start) = opening_fence(s)?;
    let (s, token) = get_token(s)?;
    let (s, code) = get_code(s)?;
    let (s, end) = closing_fence(s)?;

    let range = (start.location_offset())..(end.location_offset());

    let block = Codeblock {
        token: token.fragment().trim(),
        code: code.fragment().trim(),
        range
    };

    Ok((s, block))
}

fn opening_fence(s: Span) -> IResult<Span, Span> {
    let (s, _) = take_until("```")(s)?;
    let (s, pos) = position(s)?;
    let (s, _) = tag("```")(s)?;
    Ok((s, pos))
}

fn closing_fence(s: Span) -> IResult<Span, Span> {
    let (s, _) = tag("```")(s)?;
    let (s, pos) = position(s)?;
    Ok((s, pos))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one() {
        let source = r#"
            Code block:
            ```rs
            println!("Hello, world!");
            ```
        "#;
        let block = Span::new(source);
        let (_, block) = parse_codeblock(block).unwrap();

        assert_eq!(block.token, "rs");
        assert_eq!(block.code, "println!(\"Hello, world!\");");
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

        let blocks = Span::new(source);
        let (_, blocks) = parse_codeblocks(blocks).unwrap();

        assert_eq!(blocks.len(), 3);

        assert_eq!(blocks[0].token, "rs");
        assert_eq!(blocks[0].code, r#"println!("Hello, world!");"#);

        assert_eq!(blocks[1].token, "py");
        assert_eq!(blocks[1].code, r#"print("Hello, world!")"#);
        
        assert_eq!(blocks[2].token, "c");
        assert_eq!(blocks[2].code, r#"printf("Hello, world!");"#);
    }
    
    #[test]
    fn range() {
        // The test material needs to be inlined like this to prevent tabs from interfering.
        let source = "Code block:\n```rs\nprintln!(\"Hello, world!\")\n```";
        let block = Span::new(source);
        let (_, block) = parse_codeblock(block).unwrap();

        assert_eq!(block.range, 12..source.len());
    }
}
