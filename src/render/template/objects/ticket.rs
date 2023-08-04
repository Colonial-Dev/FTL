use std::sync::Arc;

use crossbeam::queue::SegQueue;
use minijinja::value::*;
use minijinja::{context, Environment, State as MJState};
use serde::Serialize;

use super::*;
use crate::db::{Page, Relation};
use crate::parse::{Content, Shortcode};
use crate::prelude::*;

#[derive(Debug)]
pub enum Metadata {
    Dependency { relation: Relation, child: String },
    Rendered(String),
}

/// A rendering ticket, i.e. a discrete unit of rendering work that needs to be done.
///
/// Implements [`Object`]/[`StructObject`], which forwards in-engine interactions to the `inner` field
/// while also allowing hooked-in Rust functions to downcast it from [`Value`] and access the same data
/// in a well-typed manner.
#[derive(Debug)]
pub struct Ticket {
    pub metadata: SegQueue<Metadata>,
    pub source: String,
    pub state: Context,
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
    pub fn new(ctx: &Context, page: Page, source: &str) -> Self {
        // Slice off the page's frontmatter.
        let source = source[page.offset as usize..].to_string();
        let inner = Value::from_serializable(&SerTicket {
            source: &source,
            page: &page,
        });

        Self {
            metadata: SegQueue::new(),
            state: Arc::clone(ctx),
            source,
            page,
            inner,
        }
    }

    pub fn build(self, env: &Environment) -> Result<Self> {
        let arc_self = Arc::new(self);

        let name = match &arc_self.page.template {
            Some(name) => name,
            None => "ftl_default.html",
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

        let out = template
            .render(context!(
                page => Value::from_object(Arc::clone(&arc_self))
            ))
            .map_err(Wrap::flatten)?;

        // TODO insert HTML rewriting here

        arc_self.register_dependency(Relation::PageTemplate, name)?;
        arc_self.metadata.push(Metadata::Rendered(out));

        Ok(Arc::into_inner(arc_self)
            .expect("There should only be one strong reference to the ticket."))
    }

    pub fn register_dependency(&self, relation: Relation, child: impl Into<String>) -> Result<()> {
        let child = child.into();

        if matches!(relation, Relation::PageTemplate) {
            // Stupid (but effective) way to keep builtins out of the dependencies table.
            // Because builtins are anonymous and don't appear in revision_files, a page
            // that has them as a dependency will always be rebuilt.
            if matches!(
                &*child,
                "ftl_codeblock.html" | "eval.html" | "ftl_default.html"
            ) {
                return Ok(());
            }

            let conn = self.state.db.get_ro()?;

            let query = "
                SELECT child FROM dependencies
                WHERE parent = ?1
                AND relation = 1
            ";
            let params = Some((1, &*child));

            conn.prepare_reader(query, params)?
                .try_for_each(|child| -> Result<_> {
                    self.metadata.push(Metadata::Dependency {
                        relation,
                        child: child?,
                    });

                    Ok(())
                })?;
        } else {
            self.metadata.push(Metadata::Dependency { relation, child });
        }

        Ok(())
    }

    fn render(&self, state: &MJState) -> Result<Value> {
        use pulldown_cmark::{html, Options, Parser};
        use Content::*;

        let mut buffer = String::new();

        for fragment in Content::parse_many(&self.source)? {
            match fragment {
                Plaintext(text) => buffer += text,
                Emojicode(code) => match gh_emoji::get(code) {
                    Some(emoji) => buffer += emoji,
                    None => {
                        buffer += ":";
                        buffer += code;
                        buffer += ":";
                    }
                },
                Shortcode(code) => buffer += &self.eval_shortcode(state, code)?,
                Codeblock(block) => {
                    buffer += {
                        &state
                            .env()
                            .get_template("ftl_codeblock.html")
                            .expect("Codeblock template should be built-in.")
                            .render(context!(
                                body => block.body,
                                token => block.token
                            ))
                            .map_err(Wrap::flatten)?
                    }
                }
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

        self.register_dependency(Relation::PageTemplate, name)?;

        template
            .render(context!(
                args => code.args,
                body => code.body,
                page => state.lookup("page")
            ))
            .map_err(Wrap::flatten)
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
            _ => Err(eyre!("object has no method named {name}")),
        }
        .map_err(Wrap::wrap)
    }
}

impl StructObject for Ticket {
    fn get_field(&self, name: &str) -> Option<Value> {
        match name {
            // Convenience shorthand for attributes.
            "attrs" => self.inner.get_attr("attributes").ok(),
            _ => self.inner.get_attr(name).ok(),
        }
    }

    fn static_fields(&self) -> Option<&'static [&'static str]> {
        Some(&["source", "id", "template", "draft", "attributes", "extra"])
    }
}
