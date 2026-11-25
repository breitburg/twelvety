//! Ported from upstream `test/TemplateRenderMarkdownPluginTest.js`.
//!
//! Upstream installs the plugin on a markdown-it instance passed to `setLibrary`; the port
//! installs it with `amendLibrary("md", …)`. markdown-it’s `state.env` (the render data) is
//! available to rules via `twelvety::engines::markdown::render_env()`.

use std::sync::Mutex;

use markdown_it::parser::core::CoreRule;
use markdown_it::parser::inline::builtin::InlineParserRule;
use markdown_it::plugins::cmark::inline::link::Link;
use markdown_it::{MarkdownIt, Node};
use twelvety::Value;
use twelvety::engines::markdown::render_env;
use twelvety::template_render::TemplateRender;
use twelvety::testing::{TestOptions, get_context, json};

/// The last `state.env` seen by the plugin
static ENVIRONMENT: Mutex<Option<Value>> = Mutex::new(None);

/// Appends `?data=${env.some}` to link URLs (runs after the inline parser)
struct ReplaceLink;

impl CoreRule for ReplaceLink {
    fn run(root: &mut Node, _: &MarkdownIt) {
        let env = render_env();
        let some = env.get("some").as_str().unwrap_or_default().to_string();
        *ENVIRONMENT.lock().unwrap() = Some(env);
        root.walk_mut(|node, _| {
            if let Some(link) = node.cast_mut::<Link>() {
                link.url = format!("{}?data={some}", link.url);
            }
        });
    }
}

fn test_markdown_plugin(md: &mut MarkdownIt) {
    md.add_rule::<ReplaceLink>().after::<InlineParserRule>();
}

fn get_new_template_render() -> TemplateRender {
    let options = TestOptions::default().with_config(|cfg| {
        cfg.amend_markdown_library(test_markdown_plugin);
        Ok(())
    });
    TemplateRender::new(get_context(options).unwrap(), "md").unwrap()
}

/// Both tests share `ENVIRONMENT`, so they run serially
static SERIAL: Mutex<()> = Mutex::new(());

fn assert_sends_context_data(tr: &TemplateRender) {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    *ENVIRONMENT.lock().unwrap() = None;
    let data = json(r#"{"some": "data"}"#);

    let f = tr
        .get_compiled_template("[link text](http://link.com)", false, None)
        .unwrap();
    let result = f(&data).unwrap();
    assert_eq!(ENVIRONMENT.lock().unwrap().clone(), Some(data));
    assert_eq!(
        result,
        "<p><a href=\"http://link.com?data=data\">link text</a></p>\n"
    );
}

#[test]
fn with_html_prerender_sends_context_data_to_the_markdown_library() {
    assert_sends_context_data(&get_new_template_render());
}

#[test]
fn without_html_prerender_sends_context_data_to_the_markdown_library() {
    let mut tr = get_new_template_render();
    tr.set_preprocessing_engine(None).unwrap();
    assert_sends_context_data(&tr);
}

// Skipped: "renderer that only implements the render function" passes a plain JS object with a
// `render` method to `setLibrary`; swapping in a JS library object is not supported in the port.
