//! Ported from upstream `test/TemplateRenderMarkdownTest.js`.

use markdown_it::parser::inline::{InlineRule, InlineState, Text};
use markdown_it::{MarkdownIt, Node};
use twelvety::Value;
use twelvety::template_render::{PipelineStep, TemplateRender};
use twelvety::testing::{TestOptions, get_context, json};

fn get_new_template_render(name: &str, options: TestOptions) -> TemplateRender {
    TemplateRender::new(get_context(options).unwrap(), name).unwrap()
}

fn md() -> TemplateRender {
    get_new_template_render("md", TestOptions::default())
}

fn render(tr: &TemplateRender, src: &str, data: &Value) -> String {
    tr.get_compiled_template(src, false, None).unwrap()(data).unwrap()
}

/// A tiny stand-in for the `markdown-it-emoji` plugin (just the `:)` shortcut)
struct Smiley;

impl InlineRule for Smiley {
    const MARKER: char = ':';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        if state.src[state.pos..state.pos_max].starts_with(":)") {
            let node = Node::new(Text { content: "😃".into() });
            return Some((node, 2));
        }
        None
    }
}

fn md_emoji(md: &mut MarkdownIt) {
    md.inline.add_rule::<Smiley>();
}

fn enable_code(md: &mut MarkdownIt) {
    markdown_it::plugins::cmark::block::code::add(md);
}

fn disable_code(md: &mut MarkdownIt) {
    md.block.remove_rule::<markdown_it::plugins::cmark::block::code::CodeScanner>();
}

#[test]
fn markdown() {
    assert_eq!(md().get_engine_name(), "md");
}

#[test]
fn parses_base_markdown_no_data() {
    assert_eq!(render(&md(), "# My Title", &Value::Undefined).trim(), "<h1>My Title</h1>");
}

#[test]
fn markdown_should_work_with_html_too() {
    assert_eq!(render(&md(), "<h1>My Title</h1>", &Value::Undefined).trim(), "<h1>My Title</h1>");
}

#[test]
fn parses_markdown_using_liquid_engine_default_with_data() {
    let out = render(&md(), "# {{title}}", &json(r#"{"title": "My Title"}"#));
    assert_eq!(out.trim(), "<h1>My Title</h1>");
}

#[test]
fn ignore_markdown_use_only_preprocess_engine() {
    let f = md().get_compiled_template("{{title}}", true, None).unwrap();
    assert_eq!(f(&json(r#"{"title": "My Title"}"#)).unwrap().trim(), "My Title");
}

#[test]
fn skip_markdown_and_preprocess_engine_466() {
    let mut tr = md();
    tr.set_preprocessing_engine(None).unwrap();
    let f = tr.get_compiled_template("404.html", true, None).unwrap();
    assert_eq!(f(&json(r#"{"title": "My Title"}"#)).unwrap().trim(), "404.html");
}

#[test]
fn set_markdown_engine_to_false_dont_parse() {
    let mut tr = md();
    tr.set_preprocessing_engine(None).unwrap();
    assert_eq!(render(&tr, "# {{title}}", &Value::Undefined).trim(), "<h1>{{title}}</h1>");
}

#[test]
fn set_markdown_engine_to_false_dont_parse_with_html_input() {
    let mut tr = md();
    tr.set_preprocessing_engine(None).unwrap();
    assert_eq!(render(&tr, "<h1>{{title}}</h1>", &Value::Undefined).trim(), "<h1>{{title}}</h1>");
}

#[test]
fn pass_in_an_override_liquid() {
    let mut tr = md();
    tr.set_preprocessing_engine(Some("liquid".into())).unwrap();
    let out = render(&tr, "# {{title}}", &json(r#"{"title": "My Title"}"#));
    assert_eq!(out.trim(), "<h1>My Title</h1>");
}

#[test]
fn strikethrough() {
    assert_eq!(render(&md(), "~~No~~", &Value::Undefined).trim(), "<p><s>No</s></p>");
}

#[test]
fn strikethrough_in_a_header() {
    assert_eq!(render(&md(), "# ~~No~~", &Value::Undefined).trim(), "<h1><s>No</s></h1>");
}

/// Upstream overrides the library with a plain `md()` instance; the port can’t swap in a JS
/// library, so this checks the same expected output with the default library.
#[test]
fn with_library_override() {
    assert_eq!(render(&md(), ":)", &Value::Undefined).trim(), "<p>:)</p>");
}

/// Upstream: `md().use(mdEmoji)` via `setLibrary`; here via `amendLibrary`.
#[test]
fn with_library_override_and_a_plugin() {
    let tr = get_new_template_render(
        "md",
        TestOptions::default().with_config(|cfg| {
            cfg.amend_markdown_library(md_emoji);
            Ok(())
        }),
    );
    assert_eq!(render(&tr, ":)", &Value::Undefined).trim(), "<p>😃</p>");
}

/// Upstream sets markdown-it’s `highlight` option on a `setLibrary` instance; the port’s
/// equivalent is `addMarkdownHighlighter`.
#[test]
fn use_a_custom_highlighter() {
    let tr = get_new_template_render(
        "md",
        TestOptions::default().with_config(|cfg| {
            cfg.add_markdown_highlighter(|_str, _lang| "This is overrrrrrride".into());
            Ok(())
        }),
    );
    let out = render(&tr, "```\nThis is some code.\n```", &Value::Undefined);
    assert_eq!(out.trim(), "<pre><code>This is overrrrrrride</code></pre>");
}

// Skipped: the five "use prism highlighter" tests need `@11ty/eleventy-plugin-syntaxhighlight`
// (Prism), which has no Rust port.

fn shortcode_options(paired: bool, liquid: bool) -> TestOptions {
    TestOptions::default().with_config(move |cfg| {
        match (paired, liquid) {
            (false, true) => cfg.add_liquid_shortcode("testShortcode", |_, _| Ok("## My Other Title".into())),
            (false, false) => cfg.add_nunjucks_shortcode("testShortcode", |_, _| Ok("## My Other Title".into())),
            (true, true) => cfg.add_paired_liquid_shortcode("testShortcode", |_, content, _| Ok(content.into())),
            (true, false) => {
                cfg.add_paired_nunjucks_shortcode("testShortcode", |_, content, _| Ok(content.into()))
            }
        }
        Ok(())
    })
}

/// Upstream creates a standalone Liquid/Nunjucks engine with a shortcode and passes the
/// instance to `setPreprocessingEngine`; here the engine instance comes from the engine manager.
fn markdown_inside_of_a_shortcode(paired: bool, engine_name: &str) {
    let ctx = get_context(shortcode_options(paired, engine_name == "liquid")).unwrap();
    let mut tr = TemplateRender::new(ctx.clone(), "md").unwrap();
    let engine = ctx.engine_manager.get_engine(engine_name, &ctx.extension_map).unwrap();
    tr.set_preprocessing_engine(Some(PipelineStep::Engine(engine))).unwrap();

    let src = if paired {
        "# {{title}}\n{% testShortcode %}## My Other Title{% endtestShortcode %}"
    } else {
        "# {{title}}\n{% testShortcode %}"
    };
    let out = render(&tr, src, &json(r#"{"title": "My Title", "otherTitle": "My Other Title"}"#));
    assert_eq!(out.trim(), "<h1>My Title</h1>\n<h2>My Other Title</h2>");
}

#[test]
fn use_markdown_inside_of_a_liquid_shortcode_536() {
    markdown_inside_of_a_shortcode(false, "liquid");
}

#[test]
fn use_markdown_inside_of_a_nunjucks_shortcode_536() {
    markdown_inside_of_a_shortcode(false, "njk");
}

#[test]
fn use_markdown_inside_of_a_liquid_paired_shortcode_536() {
    markdown_inside_of_a_shortcode(true, "liquid");
}

#[test]
fn use_markdown_inside_of_a_nunjucks_paired_shortcode_536() {
    markdown_inside_of_a_shortcode(true, "njk");
}

#[test]
fn disable_indented_code_blocks_by_default_2438() {
    assert_eq!(render(&md(), "    This is a test", &Value::Undefined).trim(), "<p>This is a test</p>");
}

// Skipped: "setLibrary does not have disabled indented code blocks either" and
// "amendLibrary works with setLibrary to re-enable indented code blocks" (`setLibrary` with a
// JS markdown-it instance is not supported in the port).

#[test]
fn use_amend_library_to_reenable_indented_code_blocks_2438() {
    let tr = get_new_template_render(
        "md",
        TestOptions::default().with_config(|cfg| {
            cfg.amend_markdown_library(enable_code);
            Ok(())
        }),
    );
    let out = render(&tr, "    This is a test", &Value::Undefined);
    assert_eq!(out.trim(), "<pre><code>This is a test\n</code></pre>");
}

#[test]
fn multiple_amend_library_calls_2438() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let tr = get_new_template_render(
        "md",
        TestOptions::default().with_config(move |cfg| {
            let (a, b) = (counter.clone(), counter.clone());
            cfg.amend_markdown_library(move |lib| {
                a.fetch_add(1, Ordering::SeqCst);
                enable_code(lib);
            });
            cfg.amend_markdown_library(move |lib| {
                b.fetch_add(1, Ordering::SeqCst);
                disable_code(lib);
            });
            Ok(())
        }),
    );
    let out = render(&tr, "    This is a test", &Value::Undefined);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(out.trim(), "<p>This is a test</p>");
}

#[test]
fn use_amend_library_to_add_a_plugin() {
    let tr = get_new_template_render(
        "md",
        TestOptions::default().with_config(|cfg| {
            cfg.amend_markdown_library(md_emoji);
            Ok(())
        }),
    );
    assert_eq!(render(&tr, ":)", &Value::Undefined).trim(), "<p>😃</p>");
}
