use std::sync::Arc;

use crossbeam::queue::SegQueue;
use minijinja::{
    context,
    value::*,
    State as MJState,
    Environment
};
use serde::Serialize;

use crate::{
    prelude::*, 
    db::{Page, Relation},
    parse::{Content, Shortcode}
};

use super::*;

#[derive(Debug)]
pub enum Metadata {
    Dependency {
        relation: Relation,
        child: String
    },
    Rendered(String)
}

/// A rendering ticket, i.e. a discrete unit of rendering work that needs to be done.
/// 
/// Implements [`Object`]/[`StructObject`], which forwards in-engine interactions to the `inner` field
/// while also allowing hooked-in Rust functions to downcast it from [`Value`] and access the same data 
/// in a well-typed manner.
#[derive(Debug)]
pub struct Ticket {
    pub metadata: SegQueue<Metadata>,
    pub state: State,
    pub source: String,
    pub page: Page,
    inner: Value,
}

#[derive(Serialize)]
struct SerTicket<'a> {
    source: &'a str,
    #[serde(flatten)]
    page: &'a Page,
}

impl Ticket {
    pub fn new(state: &State, page: Page, source: &str) -> Self {
        // Slice off the page's frontmatter.
        let source = source[page.offset as usize..].to_string();
        let inner = Value::from_serializable(&SerTicket {
            source: &source,
            page: &page
        });

        Self {
            metadata: SegQueue::new(),
            state: Arc::clone(state),
            source,
            page,
            inner,
        }
    }

    pub fn build(self: &Arc<Self>, env: &Environment) -> Result<()> {
        let name = match &self.page.template {
            Some(name) => name,
            None => "ftl_default.html"
        };

        let Ok(template) = env.get_template(name) else {
            let error = eyre!(
                "Tried to build with a nonexistent template (\"{}\").",
                name,
            )
            .note("This error occurred because a page had a template specified in its frontmatter that FTL couldn't find at build time.")
            .suggestion("Double check the page's frontmatter for spelling and path mistakes, and make sure the template is where you think it is.");

            bail!(error)
        };

        let out = template.render(context!(
            page => Value::from_object(Arc::clone(self))
        )).map_err(Wrap::flatten)?;

        self.metadata.push(Metadata::Rendered(out));

        Ok(())
    }

    fn render(&self, state: &MJState) -> Result<Value> {
        use Content::*;
        use pulldown_cmark::{Parser, Options, html};

        let mut buffer = String::new();

        for fragment in Content::parse_many(&self.source)? {
            match fragment {
                Plaintext(text) => buffer += text,
                Emojicode(code) => {
                    match gh_emoji::get(code) {
                        Some(emoji) => buffer += emoji,
                        None => {
                            buffer += ":";
                            buffer += code;
                            buffer += ":";
                        }
                    }
                },
                Shortcode(code) => buffer += &self.eval_shortcode(state, code)?,
                Codeblock(block) => buffer += {
                    &state.env().render_str(
                        "{{ body | highlight(token) }}",
                        context!(
                            body => block.body,
                            token => block.token
                        )
                    )?
                },
                Header(header) => {
                    for _ in 0..header.level {
                        buffer.push('#')
                    }

                    buffer += " ";
                    buffer += header.title;
                    buffer += " ";

                    // TODO: Actually handle anchors and classes
                }
            }
        }

        let options = Options::all();
        let parser = Parser::new_ext(&buffer, options);

        let mut html_buffer = String::new();
        html::push_html(&mut html_buffer, parser);

        Ok(Value::from_safe_string(html_buffer))
    }

    fn eval_shortcode(&self, state: &MJState, code: Shortcode) -> Result<String> {
        let name = format!("{}.html", code.name);

        let Ok(template) = state.env().get_template(&name) else {
            let err = eyre!(
                "Page {} contains a shortcode invoking template \"{}\", which does not exist.",
                self.page.id,
                code.name
            )
            .note("This error occurred because a shortcode referenced a template that FTL couldn't find at build time.")
            .suggestion("Double check the shortcode invocation for spelling and path mistakes, and make sure the template is where you think it is.");
    
            bail!(err);
        };

        self.metadata.push(Metadata::Dependency {
            relation: Relation::PageTemplate,
            child: name
        });

        Ok(template.render(context!(
            args => code.args,
            body => code.body,
            page => state.lookup("page")
        ))?)
    }
}

impl std::fmt::Display for Ticket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl Object for Ticket {
    fn kind(&self) -> ObjectKind<'_> {
        ObjectKind::Struct(self)
    }
    
    fn call_method(&self, state: &MJState, name: &str, _args: &[Value]) -> MJResult {
        match name {
            "render" => self.render(state),
            _ => Err(eyre!("object has no method named {name}"))
        }.map_err(Wrap::wrap)
    }
}

impl StructObject for Ticket {
    fn get_field(&self, name: &str) -> Option<Value> {
        match name {
            // Convenience shorthand for attributes.
            "attrs" => self.inner.get_attr("attributes").ok(),
            _ => self.inner.get_attr(name).ok()
        }
    }

    fn static_fields(&self) -> Option<&'static [&'static str]> {
        Some(&[
            "source",
            "id",
            "template",
            "draft",
            "attributes",
            "extra"
        ])
    }
}