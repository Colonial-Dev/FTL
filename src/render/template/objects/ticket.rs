use crossbeam::queue::SegQueue;
use inkjet::formatter::Html;
use minijinja::value::*;
use minijinja::{context, Environment, State};
use minijinja_stack_ref::scope;
use serde::Serialize;

use super::*;
use crate::db::*;
use crate::parse::{Content, Shortcode};
use crate::prelude::*;

/// A rendering ticket, i.e. a discrete unit of rendering work that needs to be done.
///
/// Implements [`Object`]/[`StructObject`], which forwards in-engine interactions to the `inner` field
/// while also allowing hooked-in Rust functions to downcast it from [`Value`] and access the same data
/// in a well-typed manner.
#[derive(Debug)]
pub struct Ticket {
    pub dependencies : SegQueue<(Relation, String)>,
    pub rev_id       : RevisionID,
    pub source       : String,
    pub ctx          : Context,
    pub page         : Page,
    inner            : Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Header {
    pub level: u8,
    pub slug: String,
    pub name: String,
    pub link: String,
    pub children: Vec<Header>
}

#[derive(Serialize)]
struct SerTicket<'a> {
    #[serde(flatten)]
    page: &'a Page,
}

impl Ticket {
    pub fn new(ctx: &Context, rev_id: &RevisionID, page: Page, source: &str) -> Self {
        // Slice off the page's frontmatter.
        let source = source[page.offset as usize..].to_owned();

        let inner = Value::from_serializable(&SerTicket {
            page: &page,
        });

        Self {
            dependencies: SegQueue::new(),
            rev_id: rev_id.clone(),
            ctx: ctx.clone(),
            source,
            page,
            inner,
        }
    }

    pub fn build(&self, env: &Environment) -> Result<String> {
        let name = match &self.page.template {
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

        let out = scope(|scope| {
            template
                .render(context!(
                    page => scope.object_ref(self)
                ))
                .map_err(Wrap::flatten)
        })?;

        self.register_dependency(Relation::PageTemplate, name)?;

        Ok(out)
    }

    fn render(&self, state: &State) -> Result<Value> {
        let buffer = self.preprocess(state)?;
        let buffer = self.render_markdown(buffer)?;
        let buffer = self.postprocess(buffer)?;

        Ok(Value::from_safe_string(buffer))
    }

    fn toc(&self) -> Result<Value> {
        // Credit to Zola for this algorithm.
        fn try_insert(parent: Option<&mut Header>, child: &Header) -> bool {
            let Some(parent) = parent else {
                return false;
            };

            if child.level <= parent.level {
                // Heading is the same level or higher, so don't insert.
                return false;
            }

            if child.level + 1 == parent.level {
                // Direct child of the parent.
                parent.children.push(child.clone());
                return true;
            }

            if !try_insert(parent.children.last_mut(), child) {
                parent.children.push(child.clone());
            }

            true
        }
        
        let mut headers: Vec<Header> = Vec::new();

        Content::parse_many(&self.source)?
            .into_iter()
            .filter_map(|f| match f {
                Content::Header(header) => Some(header),
                _ => None
            })
            .map(|h| {
                let anchor = h.ident.unwrap_or(h.title);
                let anchor = slug::slugify(anchor);

                Header {
                    level: h.level,
                    name: h.title.to_owned(),
                    link: format!("#{anchor}"),
                    slug: anchor,
                    children: Vec::new()
                }
            })
            .for_each(|h| {
                if headers.is_empty() || !try_insert(headers.last_mut(), &h) {
                    headers.push(h)
                }
            });
        
        Ok(Value::from_serializable(&headers))
    }

    #[inline(always)]
    fn preprocess(&self, state: &State) -> Result<String> {
        use std::cell::RefCell;
        use inkjet::{Highlighter, Language};
        use Content::*;

        std::thread_local! {
            static HIGHLIGHTER: RefCell<Highlighter> = RefCell::new(Highlighter::new())
        };

        let mut buffer = String::new();

        for fragment in Content::parse_many(&self.source)? {
            // Behold: the match to end all matches
            match fragment {
                Plaintext(text) => buffer += text,
                Emojicode(code) => {
                    if !self.ctx.build.render_emoji {
                        buffer += ":";
                        buffer += code;
                        buffer += ":";
                        
                        continue;
                    }

                    match gh_emoji::get(code) {
                        Some(emoji) => buffer += emoji,
                        None => {
                            warn!("Encountered an invalid emoji shortcode ('{code}').");
                            buffer += ":";
                            buffer += code;
                            buffer += ":";
                        }
                    }
                },
                Shortcode(code) => buffer += &self.eval_shortcode(state, code)?,
                Codeblock(block) => {                    
                    if !self.ctx.build.highlight_code {
                        buffer += "```";
                        buffer += block.token.unwrap_or("");
                        buffer += "\n";

                        buffer += block.body;

                        buffer += "```";
                        buffer += "\n";

                        continue;
                    }
                    
                    let format = |code| {
                        if let Some(name) = &self.ctx.build.code_template {
                            let Ok(template) = state.env().get_template(name) else {
                                bail!("Could not find specified codeblock template \"{name}\".");
                            };
                            
                            self.register_dependency(Relation::PageTemplate, name)?;

                            return Ok(template.render(context! {
                                code => code,
                            })?)
                        }

                        // Default codeblock template.
                        // Note that the empty line between the <div> and <pre> tags is important!
                        // Without it, the Markdown parser will incorrectly add <p> tags into the highlighted
                        // code.
                        Ok(indoc::formatdoc! {r#"
                            <div class="code-block">

                            <pre class="code-block-inner">
                            {code}
                            </pre>

                            </div>
                        "#})
                    };

                    if block.token.is_none() {
                        buffer += &format(block.body)?;
                    }
                    else if let Some(lang) = block.token.and_then(Language::from_token) {
                        let highlighted = HIGHLIGHTER.with(|cell| {
                            cell.borrow_mut().highlight_to_string(
                                lang,
                                &Html,
                                block.body
                            )
                        })?;

                        buffer += &format(&highlighted)?;
                    }
                    else {
                        let token = block.token.unwrap();
                        let err = eyre!("A codeblock had a language token ('{token}'), but FTL could not find a matching language definition.")
                            .note("Your codeblock's language token may just be malformed, or it could specify a language not bundled with FTL.")
                            .suggestion("Provide a valid language token, or remove it to format the block as plain text.");

                        bail!(err)
                    }
                },
                Header(header) => {
                    let anchor = header.ident.unwrap_or(header.title);
                    let anchor = slug::slugify(anchor);

                    if let Some(name) = &self.ctx.build.anchor_template {
                        let Ok(template) = state.env().get_template(name) else {
                            bail!("Could not find specified anchor template \"{name}\".");
                        };
                        
                        self.register_dependency(Relation::PageTemplate, name)?;

                        buffer += &template.render(context! {
                            level => header.level,
                            title => header.title,
                            anchor => anchor,
                            classes => header.classes
                        })?;
                    } 
                    else {
                        let level = header.level;
                        let classes = {
                            let mut buffer = String::new();
    
                            for class in header.classes {
                                buffer += class;
                                buffer += " ";
                            }
                            
                            // Integer overflow moment
                            if !buffer.is_empty() {
                                buffer.truncate(buffer.len() - 1);
                            }

                            buffer
                        };
                        
                        let anchor = indoc::formatdoc!("
                            <h{level} class=\"{classes}\">
                                <a id=\"{anchor}\" class=\"anchor\" href=\"#{anchor}\">
                                {}
                                </a>
                            </h{level}>
                        ",
                            header.title
                        );
    
                        buffer += &anchor;
                    }
                }
            }
        }

        Ok(buffer)
    }

    #[inline(always)]
    fn render_markdown(&self, buffer: String) -> Result<String> {
        use pulldown_cmark::{html, Options, Parser};
        
        let mut options = Options::all();

        if !self.ctx.build.smart_punctuation {
            options.remove(Options::ENABLE_SMART_PUNCTUATION);
        }

        let parser = Parser::new_ext(&buffer, options);
        let mut html_buffer = String::new();

        html::push_html(&mut html_buffer, parser);

        Ok(html_buffer)
    }

    #[inline(always)]
    fn postprocess(&self, buffer: String) -> Result<String> {
        use lol_html::{element, HtmlRewriter, Settings};

        let mut output = Vec::new();
        {   
            let element_content_handlers = vec![
                element!("img", |el| {
                    let conn = self.ctx.db.get_ro()?;

                    let src = el.get_attribute("src")
                        .context("Tried to cachebust an img element without a src attribute")?;
                    
                    let lookup_targets = vec![
                        format!("{}{}", &self.page.path.trim_end_matches("index.md"), src),
                        format!("{}{src}", SITE_ASSET_PATH),
                        format!("{}{src}", SITE_CONTENT_PATH),
                        src.to_owned(),
                    ];

                    let mut query = conn.prepare("
                        SELECT input_files.* FROM input_files
                        JOIN revision_files ON revision_files.id = input_files.id
                        WHERE revision_files.revision = ?1
                        AND input_files.path IN (?2, ?3, ?4, ?5)
                    ")?;

                    let parameters = [
                        self.rev_id.as_ref(),
                        &*lookup_targets[0],
                        &*lookup_targets[1],
                        &*lookup_targets[2],
                        &*lookup_targets[3],
                    ];

                    match query
                        .query_and_then(parameters, InputFile::from_row)?
                        .next() 
                    {
                        Some(file) => {
                            let file: InputFile = file?;
                    
                            el.set_attribute("src", &file.cachebust())
                                .context("Failed to set img element src attribute")?;
                        }
                        None => {
                            Err(eyre!("Could not cachebust image tag with src attribute {src}"))?
                        }
                    }

                    Ok(())
                }),
                element!("a", |el| {
                    const NO_OPENER: &str = "noopener ";
                    const NO_FOLLOW: &str = "nofollow ";
                    const NO_REFERRER: &str = "noreferrer";
                                        
                    let target = el.get_attribute("href")
                        .context("No target (href) found when trying to update hyperlink rel attribute.")
                        .suggestion("Do you have a typo or missing link?")?;

                    if !target.starts_with("http:") && !target.starts_with("https:") {
                        // Not an external link, skip.
                        return Ok(());
                    }

                    let mut rel_attribute = String::new();

                    if self.ctx.build.external_links_new_tab {
                        el.set_attribute("target", "_blank").unwrap();
                        rel_attribute += NO_OPENER;
                    }

                    if self.ctx.build.external_links_no_follow {
                        rel_attribute += NO_FOLLOW;
                    }

                    if self.ctx.build.external_links_no_referrer {
                        rel_attribute += NO_REFERRER;
                    }

                    if !rel_attribute.is_empty() {
                        el.set_attribute(
                            "rel",
                            rel_attribute.trim()
                        ).unwrap()
                    }

                    Ok(())
                })
            ];

            let mut rewriter = HtmlRewriter::new(
                Settings {
                    element_content_handlers,
                    ..Settings::default()
                },
                |c: &[u8]| output.extend_from_slice(c)
            );

            rewriter.write(buffer.as_bytes())?;
        }

        let buffer = String::from_utf8(output)?;

        Ok(buffer)
    }

    fn eval_shortcode(&self, state: &State, code: Shortcode) -> Result<String> {
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

    pub fn register_dependency(&self, relation: Relation, child: impl Into<String>) -> Result<()> {
        let child = child.into();

        if matches!(relation, Relation::PageTemplate) {
            // Stupid (but effective) way to keep builtins out of the dependencies table.
            // Because builtins are anonymous and don't appear in revision_files, a page
            // that has them as a dependency will always be rebuilt.
            if matches!(
                &*child,
                "eval.html" | "ftl_default.html"
            ) {
                return Ok(());
            }

            let conn = self.ctx.db.get_ro()?;

            let mut query = conn.prepare_cached("
                SELECT child FROM dependencies
                WHERE parent = ?1
                AND relation = 1
            ")?;

            query
                .query_and_then([&*child], |row| row.get(0))?
                .try_for_each(|child| -> Result<_> {
                    self.dependencies.push((relation, child?));
                    Ok(())
                })?;
        } 
        else {
            self.dependencies.push((relation, child));
        }

        Ok(())
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

    fn call_method(&self, state: &State, name: &str, _args: &[Value]) -> MJResult {
        match name {
            "render" => self.render(state),
            "toc" => self.toc(),
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
        Some(&["id", "path", "template", "draft", "attributes", "extra"])
    }
}