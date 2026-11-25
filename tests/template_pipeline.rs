//! Ported from upstream `test/TemplatePipelineTest.js`.

use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use twelvety::core::{Twelvety, Options};
use twelvety::template::PageResult;
use twelvety::testing::json;
use twelvety::user_config::{CompiledFn, ExtensionOptions, UserConfig};

fn options<F>(config: F) -> Options
where
    F: Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
{
    Options {
        // No configuration file (JS configuration files are not supported in the port)
        config_path: Some(String::new()),
        ..Default::default()
    }
    .with_config(config)
}

fn get_result<F>(config: F) -> PageResult
where
    F: Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
{
    let mut elev = Twelvety::new(Some("./test/stubs-virtual/"), None, options(config));
    elev.disable_logger();
    elev.to_json().unwrap().into_iter().next().expect("no results")
}

#[test]
fn permalinks_on_njk_md_pages_render_with_nunjucks() {
    let result = get_result(|cfg| {
        cfg.add_template(
            "index.md",
            r#"# {{ "abc" | replace("b", "x") }}"#,
            json(r#"{"templateEngineOverride": "njk,md", "permalink": "/{{ \"abc\" | replace(\"b\", \"x\") }}/"}"#),
        )
    });

    assert_eq!(result.url.as_str(), Some("/axc/"));
    assert_eq!(result.content.trim(), "<h1>axc</h1>");
}

#[test]
fn permalinks_on_md_pages_render_with_markdown_template_engine_446() {
    let result = get_result(|cfg| {
        cfg.add_template(
            "about.md",
            "# {{ title }}",
            json(r#"{"title": "Raw", "templateEngineOverride": "md", "permalink": "/{{ page.fileSlug }}/"}"#),
        )
    });

    assert_eq!(result.url.as_str(), Some("/about/"));
    assert_eq!(result.content.trim(), "<h1>{{ title }}</h1>");
}

#[test]
fn liquid_md_override_on_an_html_file() {
    let result = get_result(|cfg| {
        cfg.add_template(
            "index.html",
            "# {{ title }}",
            json(r#"{"title": "Hi", "templateEngineOverride": "liquid,md"}"#),
        )
    });

    assert_eq!(result.content.trim(), "<h1>Hi</h1>");
}

#[test]
fn template_syntax_reflects_the_pipeline() {
    let syntaxes: Arc<Mutex<IndexMap<String, String>>> = Arc::default();
    let collected = syntaxes.clone();
    let mut elev = Twelvety::new(
        Some("./test/stubs-virtual/"),
        None,
        options(move |cfg| {
            cfg.add_template("a.md", "", json("{}"))?;
            cfg.add_template("b.md", "", json(r#"{"templateEngineOverride": "njk,md"}"#))?;
            cfg.add_template("c.md", "", json(r#"{"templateEngineOverride": "md"}"#))?;
            cfg.add_template("d.html", "", json("{}"))?;
            cfg.add_template("e.njk", "", json("{}"))?;
            let collected = collected.clone();
            cfg.add_transform("syntax", move |ctx, content| {
                let slug = ctx.page.get("fileSlug").as_str().unwrap_or_default().to_string();
                let syntax = ctx.page.get("templateSyntax").as_str().unwrap_or_default().to_string();
                collected.lock().unwrap().insert(slug, syntax);
                Ok(content)
            });
            Ok(())
        }),
    );
    elev.disable_logger();
    elev.to_json().unwrap();

    let mut syntaxes = syntaxes.lock().unwrap().clone();
    syntaxes.sort_keys();
    let expected: IndexMap<String, String> = [("a", "liquid,md"), ("b", "njk,md"), ("c", "md"), ("d", "liquid"), ("e", "njk")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    assert_eq!(syntaxes, expected);
}

#[test]
fn a_custom_md_engine_receives_preprocessed_content() {
    let result = get_result(|cfg| {
        cfg.add_extension(
            "md",
            ExtensionOptions {
                compile: Some(Arc::new(|s: &str, _: &str| {
                    let s = s.to_string();
                    Ok(Arc::new(move |_: &twelvety::Value| Ok(format!("<custom>{s}</custom>"))) as CompiledFn)
                })),
                ..Default::default()
            },
        )?;
        cfg.add_template("index.md", "{{ title }}", json(r#"{"title": "Hi"}"#))
    });

    assert_eq!(result.content, "<custom>Hi</custom>");
}
