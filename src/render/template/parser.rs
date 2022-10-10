use std::collections::HashMap;

use nom::IResult;
use nom::branch::alt;
use nom::bytes::complete::{tag, take_until};
use nom::character::complete::char;
use nom::combinator::rest;
use nom::sequence::{preceded, terminated};
use serde::Serialize;

use crate::prelude::*;

#[derive(Serialize, Debug)]
pub struct Inline<'a> {
    pub name: &'a str,
    pub args: HashMap<&'a str, &'a str>,
}

impl<'a> Inline<'a> {
    pub fn parse(source: &'a str) -> Result<Inline> {
        let (_, inline) = parse_inline(source)
            .map_err(|ierr| {
                eyre!("Inline shortcode parsing error.")
                    .section(ierr.to_owned())
            })?;

        Ok(inline)
    }
}

#[derive(Serialize, Debug)]
pub struct Block<'a> {
    pub name: &'a str,
    pub args: HashMap<&'a str, &'a str>,
    pub block: &'a str
}

impl<'a> Block<'a> {
    pub fn parse(source: &'a str) -> Result<Block> {
        let (_, inline) = parse_block(source)
            .map_err(|ierr| {
                eyre!("Block shortcode parsing error.")
                    .section(ierr.to_owned())
            })?;

        Ok(inline)
    }
}

fn parse_inline(i: &str) -> IResult<&str, Inline> {
    let mut extract_name = alt((
        take_until(" "),
        rest
    ));

    let (_, i) = strip_inline_delimiters(i)?;
    let (i, name) = extract_name(i)?;
    let (i, args) = parse_kwargs(i)?;

    let inline = Inline {
        name,
        args
    };

    Ok((i, inline))
}

fn strip_inline_delimiters(i: &str) -> IResult<&str, &str> {
    let start_delimiter = alt((
        tag("{% sci"),
        tag("{%sci")
    ));
    let mut strip_delimiters = preceded(start_delimiter, take_until("%}"));

    let (_, o) = strip_delimiters(i)?;
    Ok(trim_whitespace(o))
}

fn parse_block(i: &str) -> IResult<&str, Block> {
    let (i, (name, args)) = parse_block_header(i)?;
    let (_, args) = parse_kwargs(args)?;
    let (i, block) = parse_block_contents(i)?;
    
    let block = Block {
        name,
        args,
        block
    };

    Ok((i, block))
}

fn parse_block_header(i: &str) -> IResult<&str, (&str, &str)> {
    let mut trim_start_delimiter = alt((
        tag("{% sc"),
        tag("{%sc")
    ));
    let mut extract_name = alt((
        take_until(" "),
        take_until("%}"),
        rest
    ));
    let extract_args = take_until("%}");

    let (i, _) = trim_start_delimiter(i)?;
    let (_, i) = trim_whitespace(i);

    let (i, name) = extract_name(i)?; 
    let (_, i) = trim_whitespace(i);
    let (i, args) = extract_args(i)?;

    let (i, _) = tag("%}")(i)?;
    let (_, i) = trim_whitespace(i);
    
    Ok((i, (name, args)))
}

fn parse_block_contents(i: &str) -> IResult<&str, &str> {
    let mut extract_content = alt((
        take_until("{% endsc %}"),
        take_until("{%endsc %}"),
        take_until("{% endsc%}"),
        take_until("{%endsc%}")
    ));

    let (i, block) = extract_content(i)?;
    let (_, block) = trim_whitespace(block);

    Ok((i, block))
}

fn parse_kwargs(i: &str) -> IResult<&str, HashMap<&str, &str>> {
    let mut kwarg_split = terminated(take_until("="), char('='));
    
    if i == "" { return Ok((i, HashMap::new())) }

    let kwargs: Vec<&str> = i
        .split(',')
        .map(|x| x.trim())
        .collect();

    let mut map = HashMap::with_capacity(kwargs.len());    
    for pair in kwargs {
        let (value, key) = kwarg_split(pair)?;
        let (_, value) = trim_quotes(value);
        map.insert(key, value);
    }
    
    Ok((i, map))
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inline_with_args() {
        let input = "{% sci youtube id=\"foo\", x=500, y=250 %}";
        let (_, result) = parse_inline(input).unwrap();

        assert_eq!(result.name, "youtube");
        assert_eq!(result.args.get("id").unwrap(), &"foo");
        assert_eq!(result.args.get("x").unwrap(), &"500");
        assert_eq!(result.args.get("y").unwrap(), &"250");
    }

    #[test]
    fn inline_no_args() {
        let input = "{% sci noargs %}";
        let (_, result) = parse_inline(input).unwrap();

        assert_eq!(result.name, "noargs");
        assert_eq!(result.args.keys().count(), 0);
    }

    #[test]
    fn inline_malformed_arg() {
        let input = "{% sci malformed id+foo %}";
        let result = parse_inline(input);
        assert!(result.is_err())
    }

    #[test]
    fn inline_malspaced_with_args() {
        let input = "{%scimalformed arg=1%}";
        let result = parse_inline(input);
        assert!(result.is_ok())
    }

    #[test]
    fn inline_malspaced_no_args() {
        let input = "{%scimalformed%}";
        let result = parse_inline(input);
        assert!(result.is_ok())
    }

    #[test]
    fn block_with_args() {
        let input = "{% sc code lang=\"rs\", dark=1 %}\npanic!()\n{% endsc %}";
        let (_, result) = parse_block(input).unwrap();

        assert_eq!(result.name, "code");
        assert_eq!(result.args.get("lang").unwrap(), &"rs");
        assert_eq!(result.args.get("dark").unwrap(), &"1");
        assert_eq!(result.block, "panic!()");
    }

    #[test]
    fn block_no_args() {
        let input = "{% sc code %}\npanic!()\n{% endsc %}";
        let (_, result) = parse_block(input).unwrap();

        assert_eq!(result.name, "code");
        assert_eq!(result.args.keys().count(), 0);
        assert_eq!(result.block, "panic!()");
    }

    #[test]
    fn block_malformed_arg() {
        let input = "{% sc code lang+\"rs\" %}\npanic!()\n{% endsc %}";
        let result = parse_block(input);
        assert!(result.is_err())
    }

    #[test]
    fn block_malspaced_with_args() {
        let input = "{%sccode lang=\"rs\"%}panic!(){%endsc%}";
        let result = parse_block(input);
        assert!(result.is_ok())
    }

    #[test]
    fn block_malspaced_no_args() {
        let input = "{%sccode%}panic!(){%endsc%}";
        let result = parse_block(input);
        assert!(result.is_ok())
    }
}
