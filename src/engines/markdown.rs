//! Port of `Engines/Markdown.js` (markdown-it via the `markdown-it` crate).

use std::sync::Arc;

use anyhow::Result;
use markdown_it::plugins::cmark::block::fence::CodeFence;
use markdown_it::{MarkdownIt, Node, NodeValue, Renderer};

use super::{RenderFn, TemplateEngine};
use crate::template_config::Config;
use crate::user_config::MarkdownHighlighterFn;
use crate::value::Value;

thread_local! {
    static RENDER_ENV: std::cell::RefCell<Value> = const { std::cell::RefCell::new(Value::Undefined) };
}

/// The data passed to the Markdown render currently in progress (markdown-it’s `state.env`),
/// for use by rules added via `amendLibrary("md", …)`. `Undefined` outside of a render.
pub fn render_env() -> Value {
    RENDER_ENV.with(|env| env.borrow().clone())
}

/// Parse and render with `data` available via [`render_env`]
fn render_with_env(
    md: &MarkdownIt,
    highlighter: Option<&MarkdownHighlighterFn>,
    src: &str,
    data: &Value,
) -> String {
    let previous = RENDER_ENV.with(|env| env.replace(data.clone()));
    let mut ast = md.parse(src);
    if let Some(h) = highlighter {
        apply_highlighter(&mut ast, h);
    }
    let html = ast.render();
    RENDER_ENV.with(|env| env.replace(previous));
    html
}

pub struct Markdown {
    name: String,
    md: Arc<MarkdownIt>,
}

/// A fence rendered with the user’s `addMarkdownHighlighter` callback
#[derive(Debug)]
struct HighlightedFence {
    html: String,
}

impl NodeValue for HighlightedFence {
    fn render(&self, _node: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
        fmt.text_raw(&self.html);
        fmt.cr();
    }
}

/// markdown-it’s default preset (with `html: true`, the 12ty default)
pub fn create_markdown_it(config: &Config) -> MarkdownIt {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::html::add(&mut md);
    // The markdown-it "default" preset enables tables and strikethrough
    markdown_it::plugins::extra::tables::add(&mut md);
    markdown_it::plugins::extra::strikethrough::add(&mut md);
    let options = &config.markdown_options;
    if options.get("linkify").is_truthy() {
        markdown_it::plugins::extra::linkify::add(&mut md);
    }
    if options.get("typographer").is_truthy() {
        markdown_it::plugins::extra::typographer::add(&mut md);
        markdown_it::plugins::extra::smartquotes::add(&mut md);
    }
    if options.get("html").as_bool() == Some(false) {
        md.block
            .remove_rule::<markdown_it::plugins::html::html_block::HtmlBlockScanner>();
        md.inline
            .remove_rule::<markdown_it::plugins::html::html_inline::HtmlInlineScanner>();
    }
    // Indented code blocks are disabled in Twelvety (`mdLib.disable("code")`)
    md.block
        .remove_rule::<markdown_it::plugins::cmark::block::code::CodeScanner>();
    for amendment in &config.markdown_amendments {
        amendment(&mut md);
    }
    md
}

fn apply_highlighter(root: &mut Node, highlighter: &MarkdownHighlighterFn) {
    root.walk_mut(|node, _| {
        if let Some(fence) = node.cast::<CodeFence>() {
            let lang = html_escape::decode_html_entities(&fence.info)
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            let highlighted = highlighter(&fence.content, &lang);
            let html = if highlighted.starts_with("<pre") {
                highlighted
            } else {
                let class = if lang.is_empty() {
                    String::new()
                } else {
                    format!(
                        " class=\"language-{}\"",
                        html_escape::encode_double_quoted_attribute(&lang)
                    )
                };
                format!("<pre><code{class}>{highlighted}</code></pre>")
            };
            node.replace(HighlightedFence { html });
        }
    });
}

impl Markdown {
    pub fn new(name: &str, config: &Config) -> Self {
        Markdown {
            name: name.into(),
            md: Arc::new(create_markdown_it(config)),
        }
    }

    pub fn with_highlighter(name: &str, config: &Config) -> (Self, Option<MarkdownHighlighterFn>) {
        (Self::new(name, config), config.markdown_highlighter.clone())
    }
}

pub struct MarkdownEngine {
    inner: Markdown,
    highlighter: Option<MarkdownHighlighterFn>,
}

impl MarkdownEngine {
    pub fn new(name: &str, config: &Config) -> Self {
        let (inner, highlighter) = Markdown::with_highlighter(name, config);
        MarkdownEngine { inner, highlighter }
    }

    pub fn render(&self, src: &str) -> String {
        render_with_env(
            &self.inner.md,
            self.highlighter.as_ref(),
            src,
            &Value::object(),
        )
    }
}

impl TemplateEngine for MarkdownEngine {
    fn name(&self) -> &str {
        &self.inner.name
    }

    fn cacheable(&self) -> bool {
        true
    }

    fn compile(&self, src: &str, _input_path: Option<&str>) -> Result<RenderFn> {
        let md = self.inner.md.clone();
        let highlighter = self.highlighter.clone();
        let src = src.to_string();
        Ok(Arc::new(move |data| {
            Ok(render_with_env(&md, highlighter.as_ref(), &src, data))
        }))
    }
}
