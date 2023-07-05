mod content;
mod template;

pub use content::*;
use nom::error::{ErrorKind, ParseError};
use nom::{Err, IResult};
pub use template::*;

use crate::prelude::{Report, Result as EyreResult};

type Input<'i> = &'i str;
type Result<'i, T> = IResult<Input<'i>, T, (Input<'i>, ErrorKind)>;

/// Wraps the given parser, consuming all whitespace before and after it.
/// Taken from the Nom recipes document.
fn trim<'a, F, O, E>(inner: F) -> impl FnMut(&'a str) -> IResult<&'a str, O, E>
where
    E: ParseError<&'a str>,
    F: FnMut(&'a str) -> IResult<&'a str, O, E>,
{
    use nom::character::complete::multispace0;
    use nom::sequence::delimited;

    delimited(multispace0, inner, multispace0)
}

/// Converts a nom Err into an eyre Report.
fn to_report(err: Err<(Input<'_>, ErrorKind)>) -> Report {
    Report::from(err.to_owned())
}
