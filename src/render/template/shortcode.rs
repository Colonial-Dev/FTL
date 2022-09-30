use std::borrow::Cow;

use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use regex::{Regex, Captures};
use tera::{Tera, Context};

use crate::{share::ERROR_CHANNEL};

lazy_static! {
    static ref INLINE_SC_REGEX: Regex = Regex::new(r#"\{% sci (.*?) (.*)?%\}"#).unwrap();
    static ref BLOCK_SC_REGEX: Regex = Regex::new(r#"(?s)\{% sc (.*) %\}\n?(.*?)\n?\{% endsc %\}"#).unwrap();
}

/// Parses the provided input for inline and block shortcodes, then evaluates them (in that order) using a [`Tera`] instance.
pub fn evaluate_shortcodes<'a>(input: Cow<'a, str>, tera: &Tera) -> Cow<'a, str> {
    let result = expand_inline(&input, tera);
    let result = expand_block(&result, tera);
    Cow::Owned(result.to_string())
}

/// Finds all inline shortcodes in the provided input using [`INLINE_SC_REGEX`],
/// then executes a [`Regex::replace_all`] operation that evaluates and replaces each one.
fn expand_inline<'a>(input: &'a str, tera: &Tera) -> Cow<'a, str> {
    INLINE_SC_REGEX.replace_all(input, |caps: &Captures| {
        if check_validity(caps, tera) {
            let ctx = split_args(&caps[2]);
            let out = tera.render(&caps[1], &ctx);

            match out {
                Ok(out) => format!("{out}"),
                Err(e) => {
                    ERROR_CHANNEL.sink_error(e.into());
                    format!("{}", &caps[0])
                }
            }
        }
        else {
            let err = anyhow!(
                "Invalid inline shortcode detected, skipping. Captures were:\n{:#?}", caps
            );
            ERROR_CHANNEL.sink_error(err);
            format!("{}", &caps[0]) 
        }
    })
}

/// Finds all block shortcodes in the provided input using [`BLOCK_SC_REGEX`],
/// then executes a [`Regex::replace_all`] operation that evaluates and replaces each one.
fn expand_block<'a>(input: &'a str, tera: &Tera) -> Cow<'a, str> {
    BLOCK_SC_REGEX.replace_all(input, |caps: &Captures| {
        if check_validity(caps, tera) {
            let mut ctx = Context::new();
            ctx.insert("block", &caps[2]);
            let out = tera.render(&caps[1], &ctx);

            match out {
                Ok(out) => format!("{out}"),
                Err(e) => {
                    ERROR_CHANNEL.sink_error(e.into());
                    format!("{}", &caps[0])
                }
            }
        }
        else {
            let err = anyhow!(
                "Invalid block shortcode detected, skipping. Captures were:\n{:#?}", caps
            );
            ERROR_CHANNEL.sink_error(err);
            format!("{}", &caps[0]) 
        }
    })
}

/// Takes arguments from an inline shortcode (of the format `arg_a="data", arg_b=...`),
/// and parses them into a [`Context`] instance for use when evaluating the inline shortcode.
fn split_args<'a>(args: &'a str) -> Context {
    let mut ctx = Context::new();
    if args == "" { return ctx; }

    args.split(',')
        .map(|arg| {
            let arg = 
            arg
                .trim_start_matches(' ')
                .trim_end_matches(' ');
            arg
        })
        .for_each(|arg| {
            let kv: Vec<&str> = arg.split('=').collect();
            let key = kv.get(0);
            let val = kv.get(1);

            if key.is_none() || val.is_none() {
                let err = anyhow!(
                    "Inline shortcode key/value pair malformed, skipping.
                    \nOriginal pair: {arg}, K: {:?}, V:{:?}", key, val
                );
                ERROR_CHANNEL.sink_error(err); 
                return; 
            }
            else {
                let key = 
                key.unwrap()
                    .trim_start_matches(' ')
                    .trim_end_matches(' ');
                
                let val = 
                val.unwrap()
                    .trim_start_matches(' ')
                    .trim_end_matches(' ')
                    .trim_start_matches('"')
                    .trim_end_matches('"'); 
                
                ctx.insert(key, val);
            }
        });

    ctx
}

/// Checks to see if a shortcode capture is valid and therefore safe to evaluate.
/// To return true, the input must meet the following conditions:
/// - The shortcode name is present (i.e. there is a capture group at index 1)
/// - The shortcode name corresponds to a template name in the provided Tera instance.
/// - The shortcode arguments or block exist (i.e. there is a capture group at index 2.)
fn check_validity(caps: &Captures, tera: &Tera) -> bool {
    let template_name = match caps.get(1) {
        Some(name) => name.as_str(),
        None => {
            return false;
        }
    };

    let template_exists = tera.get_template_names()
        .any(|n| n == template_name);
    
    let args_exist = caps.get(2).is_some();
    
    template_exists && args_exist
}