use std::{collections::HashMap, ops::Range};

use nom::{bytes::complete::take_until, character::complete::char, sequence::terminated, IResult};
use serde::Serialize;

use super::{
    delimit::{Delimited, DelimiterKind},
    trim_quotes, Ranged,
};
use crate::prelude::*;

#[derive(Serialize, Debug)]
pub struct Shortcode<'a> {
    pub name: &'a str,
    pub args: HashMap<&'a str, &'a str>,
    pub content: Option<&'a str>,
    pub range: Range<usize>,
}

impl<'a> TryFrom<Delimited<'a>> for Shortcode<'a> {
    type Error = Report;

    fn try_from(value: Delimited<'a>) -> Result<Self, Self::Error> {
        match value.kind {
            DelimiterKind::Inline => parse_inline(value),
            DelimiterKind::Multiline => parse_multiline(value),
            DelimiterKind::Raw => unimplemented!(),
        }
    }
}

impl<'a> Ranged for Shortcode<'a> {
    fn range(&self) -> Range<usize> {
        self.range.clone()
    }
}

fn parse_inline<'a>(source: Delimited<'a>) -> Result<Shortcode> {
    let (name, args) = source
        .contents
        .split_once(' ')
        .unwrap_or((source.contents, ""));

    let (_, args) = parse_kwargs(args)
        .map_err(|ierr| {
            eyre!("Encountered a shortcode with malformed kwargs. (Source: {source:?})")
            .note("This error occurred because FTL found a shortcode invocation with improperly formatted arguments.")
            .suggestion("Make sure your shortcode invocation is well-formed.")
            .section(ierr.to_owned())
        })?;

    Ok(Shortcode {
        name,
        args,
        content: None,
        range: source.range,
    })
}

fn parse_multiline<'a>(source: Delimited<'a>) -> Result<Shortcode<'a>> {
    let token = source
        .token
        .expect("Multiline token was None!")
        .trim_end_matches(" %}")
        .trim();

    let (name, args) = token.split_once(' ').unwrap_or((token, ""));

    let (_, args) = parse_kwargs(args)
        .map_err(|ierr| {
            eyre!("Encountered a shortcode with malformed kwargs. (Source: {source:?})")
            .note("This error occurred because FTL found a shortcode invocation with improperly formatted arguments.")
            .suggestion("Make sure your shortcode invocation is well-formed.")
            .section(ierr.to_owned())
        })?;

    Ok(Shortcode {
        name,
        args,
        content: Some(source.contents),
        range: source.range,
    })
}

fn parse_kwargs(i: &str) -> IResult<&str, HashMap<&str, &str>> {
    let mut kwarg_split = terminated(take_until("="), char('='));

    if i == "" {
        return Ok((i, HashMap::new()));
    }

    let kwargs: Vec<&str> = i.split(',').map(|x| x.trim()).collect();

    let mut map = HashMap::with_capacity(kwargs.len());
    for pair in kwargs {
        let (value, key) = kwarg_split(pair)?;
        let (_, value) = trim_quotes(value);
        map.insert(key, value);
    }

    Ok((i, map))
}

#[cfg(test)]
mod inline {
    use once_cell::sync::Lazy;

    use super::*;
    use crate::parse::delimit::Delimiters;

    static DELIMS: Lazy<Delimiters> =
        Lazy::new(|| Delimiters::new("{% sci ", " %}", DelimiterKind::Inline));

    #[test]
    fn with_args() {
        let source = "{% sci youtube id=\"foo\", x=500, y=250 %}";
        let code = &DELIMS.parse_into::<Shortcode>(source).unwrap()[0];

        assert_eq!(code.name, "youtube");
        assert_eq!(code.args.get("id").unwrap(), &"foo");
        assert_eq!(code.args.get("x").unwrap(), &"500");
        assert_eq!(code.args.get("y").unwrap(), &"250");
    }

    #[test]
    fn no_args() {
        let source = "{% sci noargs %}";
        let code = &DELIMS.parse_into::<Shortcode>(source).unwrap()[0];

        assert_eq!(code.name, "noargs");
        assert_eq!(code.args.keys().count(), 0);
    }

    #[test]
    fn malformed_args() {
        let source = "{% sci malformed id+foo %}";
        let result = DELIMS.parse_into::<Shortcode>(source);

        assert!(result.is_err())
    }
}

#[cfg(test)]
mod block {
    use once_cell::sync::Lazy;

    use super::*;
    use crate::parse::delimit::Delimiters;

    static DELIMS: Lazy<Delimiters> =
        Lazy::new(|| Delimiters::new("{% sc ", "{% endsc %}", DelimiterKind::Multiline));

    #[test]
    fn with_args() {
        let source = "{% sc code lang=\"rs\", dark=1 %}\npanic!()\n{% endsc %}";
        let code = &DELIMS.parse_into::<Shortcode>(source).unwrap()[0];

        assert_eq!(code.name, "code");
        assert_eq!(code.args.get("lang").unwrap(), &"rs");
        assert_eq!(code.args.get("dark").unwrap(), &"1");
        assert_eq!(code.content, Some("panic!()"));
    }

    #[test]
    fn no_args() {
        let source = "{% sc code %}\npanic!()\n{% endsc %}";
        let code = &DELIMS.parse_into::<Shortcode>(source).unwrap()[0];

        assert_eq!(code.name, "code");
        assert_eq!(code.args.keys().count(), 0);
        assert_eq!(code.content, Some("panic!()"));
    }

    #[test]
    fn malformed_args() {
        let source = "{% sc code lang+\"rs\" %}\npanic!()\n{% endsc %}";
        let result = DELIMS.parse_into::<Shortcode>(source);

        assert!(result.is_err())
    }
}
