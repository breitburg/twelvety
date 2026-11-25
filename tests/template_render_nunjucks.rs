//! Tests for the Nunjucks engine, ported from upstream `test/TemplateRenderNunjucksTest.js`
//! and `test/TwelvetyNunjucksTest.js`.
//!
//! Skipped upstream tests (JavaScript-only APIs):
//! - "Nunjucks getEngineLib", "Nunjucks Render: with Library Override": `setLibrary`/`getEngineLib`
//!   hand out a JavaScript `Nunjucks.Environment`.
//! - "Nunjucks Render with getGlobals Issue #567": relies on `this.getVariables()` of a
//!   JavaScript global function (the filter variant is ported through `CallContext::get`).
//! - "Throw an error when you return a promise in addFilter for Nunjucks": Rust filters
//!   cannot return promises.
//! - "Nunjucks Shortcode with this.env #3175": `this.env` is the JavaScript environment.
//! - "Use addNunjucksGlobal with async function": skipped upstream too.
//!
//! Async filters/shortcodes are plain Rust closures, so the async variants run the same
//! closures through the async registration methods (or the sync ones when the async
//! method does not exist in the port, e.g. `addNunjucksAsyncFilter`).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Result, anyhow};
use twelvety::template_render::TemplateRender;
use twelvety::testing::{TestOptions, get_context, json, render_string};
use twelvety::user_config::{CallContext, UserConfig};
use twelvety::{Options, Twelvety, Value};

fn input(dir: &str) -> TestOptions {
    TestOptions::input(dir)
}

fn config<F>(dir: Option<&str>, f: F) -> TestOptions
where
    F: Fn(&mut UserConfig) -> Result<()> + Send + Sync + 'static,
{
    let options = match dir {
        Some(d) => TestOptions::input(d),
        None => TestOptions::default(),
    };
    options.with_config(f)
}

fn render(src: &str, data: Value, options: TestOptions) -> Result<String> {
    render_string("njk", src, data, options)
}

/// Full error chain as one string (`err.message`, `err.cause.message`, …)
fn error_chain(e: &anyhow::Error) -> String {
    format!("{e:#}")
}

/// A shortcode closure asserting `this.page.url` and counting its calls (`t.plan`)
fn count_calls() -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let counter = Arc::new(AtomicUsize::new(0));
    (counter.clone(), counter)
}

fn assert_page_url(ctx: &CallContext, counter: &AtomicUsize) {
    assert_eq!(ctx.page().get("url").as_str(), Some("/hi/"));
    counter.fetch_add(1, Ordering::SeqCst);
}

fn hi_page(extra: &str) -> Value {
    json(&format!(r#"{{ {extra} "page": {{ "url": "/hi/" }} }}"#))
}

fn arg(args: &[Value], i: usize) -> String {
    args.get(i)
        .cloned()
        .unwrap_or(Value::Undefined)
        .to_js_string()
}

#[test]
fn nunjucks_engine_name() {
    let ctx = get_context(TestOptions::default()).unwrap();
    let tr = TemplateRender::new(ctx, "njk").unwrap();
    assert_eq!(tr.engine_name, "njk");
}

#[test]
fn nunjucks_render() {
    let out = render(
        "<p>{{ name }}</p>",
        json(r#"{"name": "Zach"}"#),
        TestOptions::default(),
    );
    assert_eq!(out.unwrap(), "<p>Zach</p>");
}

#[test]
fn nunjucks_render_addition() {
    let out = render(
        "<p>{{ number + 1 }}</p>",
        json(r#"{"number": 1}"#),
        TestOptions::default(),
    );
    assert_eq!(out.unwrap(), "<p>2</p>");
}

#[test]
fn nunjucks_render_extends() {
    let out = render(
        "{% extends 'base.njk' %}{% block content %}This is a child.{% endblock %}",
        Value::object(),
        input("test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>This is a child.</p>");
}

#[test]
fn nunjucks_render_relative_extends() {
    let out = render_string(
        "./test/stubs/njk-relative/dir/does_not_exist_and_thats_ok.njk",
        "{% extends '../dir/base.njk' %}{% block content %}This is a child.{% endblock %}",
        Value::object(),
        input("test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>This is a child.</p>");
}

#[test]
fn nunjucks_render_include() {
    let out = render(
        "<p>{% include 'included.njk' %}</p>",
        Value::object(),
        input("test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>This is an include.</p>");
}

#[test]
fn nunjucks_render_include_different_extension() {
    let out = render(
        "<p>{% include 'included.nunj' %}</p>",
        Value::object(),
        input("test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>Nunjabusiness</p>");
}

#[test]
fn nunjucks_render_include_different_extension_subdir() {
    let out = render(
        "<p>{% include 'subfolder/included.nunj' %}</p>",
        Value::object(),
        input("test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>Nunjabusiness2</p>");
}

#[test]
fn nunjucks_render_relative_include_issue_190() {
    let out = render_string(
        "./test/stubs/njk-relative/does_not_exist_and_thats_ok.njk",
        "<p>{% include './dir/included.njk' %}</p>",
        Value::object(),
        input("./test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>HELLO FROM THE OTHER SIDE.</p>");
}

#[test]
fn nunjucks_render_relative_include_parent_dir_issue_190() {
    let path = "./test/stubs/njk-relative/dir/does_not_exist_and_thats_ok.njk";
    let out = render_string(
        path,
        "<p>{% include '../dir/included.njk' %}</p>",
        Value::object(),
        input("./test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>HELLO FROM THE OTHER SIDE.</p>");

    // should look in _includes too, related to Issue #633
    let out = render_string(
        path,
        "<p>{% include 'included-relative.njk' %}</p>",
        Value::object(),
        input("./test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>akdlsjafkljdskl</p>");
}

#[test]
fn nunjucks_render_relative_include_current_dir_issue_190() {
    let out = render_string(
        "./test/stubs/njk-relative/dir/does_not_exist_and_thats_ok.njk",
        "<p>{% include './included.njk' %}</p>",
        Value::object(),
        input("./test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>HELLO FROM THE OTHER SIDE.</p>");
}

#[test]
fn nunjucks_render_relative_include_ambiguous_path_prefers_includes() {
    let out = render_string(
        "./test/stubs/njk-relative/dir/does_not_exist_and_thats_ok.njk",
        "<p>{% include 'included.njk' %}</p>",
        Value::object(),
        input("./test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>This is an include.</p>");
}

#[test]
fn nunjucks_async_filter() {
    let options = config(Some("test/stubs"), |cfg| {
        cfg.add_nunjucks_filter("myAsyncFilter", |_, value, _| {
            Ok(Value::str(format!("HI{}", value.to_js_string())))
        });
        Ok(())
    });
    let out = render("{{ 'test' | myAsyncFilter }}", Value::object(), options);
    assert_eq!(out.unwrap().trim(), "HItest");
}

#[test]
fn nunjucks_render_set_with_a_filter() {
    let options = config(Some("test/stubs"), |cfg| {
        cfg.add_nunjucks_filter("uppercase", |_, value, _| {
            Ok(Value::str(value.to_js_string().to_uppercase()))
        });
        Ok(())
    });
    let out = render(
        r#"{% set test = "hi" | uppercase %}{{ test }}"#,
        Value::object(),
        options,
    );
    assert_eq!(out.unwrap().trim(), "HI");
}

#[test]
fn nunjucks_render_include_a_js_file_issue_398() {
    let options = config(Some("test/stubs"), |cfg| {
        cfg.add_nunjucks_filter("jsmin", |_, value, _| Ok(value));
        Ok(())
    });
    let out = render(
        "{% set ga %}{% include 'test.js' %}{% endset %}{{ ga | safe | jsmin }}",
        Value::object(),
        options,
    );
    assert_eq!(
        out.unwrap().trim(),
        r#"/* THIS IS A COMMENT */ alert("Issue #398");"#
    );
}

#[test]
fn nunjucks_render_include_subfolder() {
    let out = render(
        "<p>{% include 'subfolder/included.html' %}</p>",
        Value::object(),
        input("test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>This is an include.</p>");
}

#[test]
fn nunjucks_render_include_double_quotes() {
    let out = render(
        r#"<p>{% include "included.njk" %}</p>"#,
        Value::object(),
        input("test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>This is an include.</p>");
}

#[test]
fn nunjucks_render_include_subfolder_double_quotes() {
    let out = render(
        r#"<p>{% include "subfolder/included.html" %}</p>"#,
        Value::object(),
        input("test/stubs"),
    );
    assert_eq!(out.unwrap(), "<p>This is an include.</p>");
}

#[test]
fn nunjucks_render_imports() {
    let out = render(
        "{% import 'imports.njk' as forms %}<div>{{ forms.label('Name') }}</div>",
        Value::object(),
        input("test/stubs"),
    );
    assert_eq!(out.unwrap(), "<div><label>Name</label></div>");
}

#[test]
fn nunjucks_render_relative_imports() {
    let out = render_string(
        "./test/stubs/njk-relative/dir/does_not_exist_and_thats_ok.njk",
        "{% import '../dir/imports.njk' as forms %}<div>{{ forms.label('Name') }}</div>",
        Value::object(),
        input("test/stubs"),
    );
    assert_eq!(out.unwrap(), "<div><label>Name</label></div>");
}

#[test]
fn nunjucks_render_imports_from() {
    let out = render(
        "{% from 'imports.njk' import label %}<div>{{ label('Name') }}</div>",
        Value::object(),
        input("test/stubs"),
    );
    assert_eq!(out.unwrap(), "<div><label>Name</label></div>");
}

#[test]
fn nunjucks_render_with_get_var_from_string_filter_issue_567() {
    let options = config(None, |cfg| {
        cfg.add_nunjucks_filter("getVarFromString", |ctx, value, _| {
            Ok(ctx.get(&value.to_js_string()))
        });
        Ok(())
    });
    let out = render(
        "<p>{{ 'my-global-name' | getVarFromString }}</p>",
        json(r#"{"my-global-name": "Zach"}"#),
        options,
    );
    assert_eq!(out.unwrap(), "<p>Zach</p>");
}

#[test]
fn nunjucks_shortcode_without_args_372() {
    let options = config(Some("./test/stubs/"), |cfg| {
        cfg.add_nunjucks_shortcode("postfixWithZach", |_, args| Ok(arg(args, 0) + "Zach"));
        Ok(())
    });
    let out = render("{% postfixWithZach %}", Value::object(), options);
    assert_eq!(out.unwrap(), "undefinedZach");
}

#[test]
fn nunjucks_shortcode() {
    let (calls, c) = count_calls();
    let options = config(Some("./test/stubs/"), move |cfg| {
        let c = c.clone();
        cfg.add_nunjucks_shortcode("postfixWithZach", move |ctx, args| {
            assert_page_url(ctx, &c);
            Ok(arg(args, 0) + "Zach")
        });
        Ok(())
    });
    let out = render(
        "{% postfixWithZach name %}",
        hi_page(r#""name": "test","#),
        options,
    );
    assert_eq!(out.unwrap(), "testZach");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn nunjucks_async_shortcode() {
    let (calls, c) = count_calls();
    let options = config(Some("./test/stubs/"), move |cfg| {
        let c = c.clone();
        cfg.add_async_shortcode("postfixWithZach", move |ctx, args| {
            assert_page_url(ctx, &c);
            Ok(arg(args, 0) + "Zach")
        });
        Ok(())
    });
    let out = render(
        "{% postfixWithZach name %}",
        hi_page(r#""name": "test","#),
        options,
    );
    assert_eq!(out.unwrap(), "testZach");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn nunjucks_sync_function_shortcode_error_throwing() {
    let options = config(Some("./test/stubs/"), |cfg| {
        cfg.add_nunjucks_shortcode("postfixWithZach", |_, _| {
            Err(anyhow!("Nunjucks sync function Shortcode (error throwing)"))
        });
        Ok(())
    });
    let err = render(
        "{% postfixWithZach name %}",
        json(r#"{"name": "test"}"#),
        options,
    )
    .unwrap_err();
    let msg = error_chain(&err);
    assert!(
        msg.contains("Error with Nunjucks shortcode `postfixWithZach`"),
        "{msg}"
    );
    assert!(
        msg.contains("Nunjucks sync function Shortcode (error throwing)"),
        "{msg}"
    );
}

#[test]
fn nunjucks_async_function_shortcode_error_throwing() {
    let options = config(Some("./test/stubs/"), |cfg| {
        cfg.add_async_shortcode("postfixWithZachError", |_, _| {
            Err(anyhow!(
                "Nunjucks Async function Shortcode (error throwing)"
            ))
        });
        Ok(())
    });
    let err = render(
        "{% postfixWithZachError name %}",
        json(r#"{"name": "test"}"#),
        options,
    )
    .unwrap_err();
    let msg = error_chain(&err);
    assert!(
        msg.contains("Error with Nunjucks shortcode `postfixWithZachError`"),
        "{msg}"
    );
    assert!(
        msg.contains("Nunjucks Async function Shortcode (error throwing)"),
        "{msg}"
    );
}

#[test]
fn nunjucks_sync_function_paired_shortcode_error_throwing() {
    let options = config(Some("./test/stubs/"), |cfg| {
        cfg.add_paired_nunjucks_shortcode("postfixWithZachError", |_, _, _| {
            Err(anyhow!(
                "Nunjucks sync function paired Shortcode (error throwing)"
            ))
        });
        Ok(())
    });
    let err = render(
        "{% postfixWithZachError name %}hi{% endpostfixWithZachError %}",
        json(r#"{"name": "test"}"#),
        options,
    )
    .unwrap_err();
    let msg = error_chain(&err);
    assert!(
        msg.contains("Error with Nunjucks paired shortcode `postfixWithZachError`"),
        "{msg}"
    );
    assert!(
        msg.contains("Nunjucks sync function paired Shortcode (error throwing)"),
        "{msg}"
    );
}

#[test]
fn nunjucks_async_function_paired_shortcode_error_throwing() {
    let options = config(Some("./test/stubs/"), |cfg| {
        cfg.add_paired_async_shortcode("postfixWithZachError", |_, _, _| {
            Err(anyhow!(
                "Nunjucks Async function paired Shortcode (error throwing)"
            ))
        });
        Ok(())
    });
    let err = render(
        "{% postfixWithZachError name %}hi{% endpostfixWithZachError %}",
        json(r#"{"name": "test"}"#),
        options,
    )
    .unwrap_err();
    let msg = error_chain(&err);
    assert!(
        msg.contains("Error with Nunjucks paired shortcode `postfixWithZachError`"),
        "{msg}"
    );
    assert!(
        msg.contains("Nunjucks Async function paired Shortcode (error throwing)"),
        "{msg}"
    );
}

#[test]
fn nunjucks_shortcode_safe_output() {
    let (calls, c) = count_calls();
    let options = config(Some("./test/stubs/"), move |cfg| {
        let c = c.clone();
        cfg.add_nunjucks_shortcode("postfixWithZach", move |ctx, args| {
            assert_page_url(ctx, &c);
            Ok(format!("<span>{}</span>", arg(args, 0)))
        });
        Ok(())
    });
    let out = render(
        "{% postfixWithZach name %}",
        hi_page(r#""name": "test","#),
        options,
    );
    assert_eq!(out.unwrap(), "<span>test</span>");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn nunjucks_shortcode_return_non_string_value() {
    let options = config(Some("./test/stubs/"), |cfg| {
        cfg.add_nunjucks_shortcode("getYear", |_, _| Ok(2022.to_string()));
        Ok(())
    });
    let out = render("{% getYear %}", Value::object(), options);
    assert_eq!(out.unwrap(), "2022");
}

fn paired_postfix_options(calls: Arc<AtomicUsize>, is_async: bool) -> TestOptions {
    config(Some("./test/stubs/"), move |cfg| {
        let c = calls.clone();
        let f = move |ctx: &CallContext, content: &str, args: &[Value]| {
            assert_page_url(ctx, &c);
            Ok(format!("{}{content}Zach", arg(args, 0)))
        };
        if is_async {
            cfg.add_paired_async_shortcode("postfixWithZach", f);
        } else {
            cfg.add_paired_nunjucks_shortcode("postfixWithZach", f);
        }
        Ok(())
    })
}

#[test]
fn nunjucks_paired_shortcode() {
    let (calls, c) = count_calls();
    let out = render(
        "{% postfixWithZach name %}Content{% endpostfixWithZach %}",
        hi_page(r#""name": "test","#),
        paired_postfix_options(c, false),
    );
    assert_eq!(out.unwrap(), "testContentZach");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn nunjucks_async_paired_shortcode() {
    let (calls, c) = count_calls();
    let out = render(
        "{% postfixWithZach name %}Content{% endpostfixWithZach %}",
        hi_page(r#""name": "test","#),
        paired_postfix_options(c, true),
    );
    assert_eq!(out.unwrap(), "testContentZach");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn nunjucks_nested_async_paired_shortcode() {
    let (calls, c) = count_calls();
    let out = render(
        "{% postfixWithZach name %}Content{% postfixWithZach name2 %}Content{% endpostfixWithZach %}{% endpostfixWithZach %}",
        hi_page(r#""name": "test", "name2": "test2","#),
        paired_postfix_options(c, true),
    );
    assert_eq!(out.unwrap(), "testContenttest2ContentZachZach");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn nunjucks_paired_shortcode_without_args_372() {
    let options = config(Some("./test/stubs/"), |cfg| {
        cfg.add_paired_nunjucks_shortcode("postfixWithZach", |ctx, content, args| {
            assert_eq!(ctx.page().get("url").as_str(), Some("/hi/"));
            Ok(format!("{content}{}Zach", arg(args, 0)))
        });
        Ok(())
    });
    let out = render(
        "{% postfixWithZach %}Content{% endpostfixWithZach %}",
        hi_page(r#""name": "test","#),
        options,
    );
    assert_eq!(out.unwrap(), "ContentundefinedZach");
}

#[test]
fn nunjucks_paired_shortcode_with_tag_inside() {
    let (calls, c) = count_calls();
    let out = render(
        "{% postfixWithZach name %}Content{% if tester %}If{% endif %}{% endpostfixWithZach %}",
        hi_page(r#""name": "test", "tester": true,"#),
        paired_postfix_options(c, false),
    );
    assert_eq!(out.unwrap(), "testContentIfZach");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn nunjucks_nested_paired_shortcode() {
    let (calls, c) = count_calls();
    let out = render(
        "{% postfixWithZach name %}Content{% postfixWithZach name2 %}Content{% endpostfixWithZach %}{% endpostfixWithZach %}",
        hi_page(r#""name": "test", "name2": "test2","#),
        paired_postfix_options(c, false),
    );
    assert_eq!(out.unwrap(), "testContenttest2ContentZachZach");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn nunjucks_shortcode_multiple_args() {
    let (calls, c) = count_calls();
    let options = config(Some("./test/stubs/"), move |cfg| {
        let c = c.clone();
        cfg.add_nunjucks_shortcode("postfixWithZach", move |ctx, args| {
            assert_page_url(ctx, &c);
            Ok(format!("{}{}Zach", arg(args, 0), arg(args, 1)))
        });
        Ok(())
    });
    let out = render(
        "{% postfixWithZach name, other %}",
        hi_page(r#""name": "test", "other": "howdy","#),
        options,
    );
    assert_eq!(out.unwrap(), "testhowdyZach");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn nunjucks_shortcode_multiple_args_comma_is_required() {
    let options = config(Some("./test/stubs/"), |cfg| {
        cfg.add_nunjucks_shortcode("postfixWithZach", |_, args| {
            Ok(format!("{}{}Zach", arg(args, 0), arg(args, 1)))
        });
        Ok(())
    });
    let out = render(
        "{% postfixWithZach name other %}",
        json(r#"{"name": "test", "other": "howdy"}"#),
        options,
    );
    assert!(out.is_err());
}

fn named_args_options(calls: Arc<AtomicUsize>) -> TestOptions {
    config(Some("./test/stubs/"), move |cfg| {
        let c = calls.clone();
        cfg.add_nunjucks_shortcode("postfixWithZach", move |ctx, args| {
            assert_page_url(ctx, &c);
            let a = args.first().cloned().unwrap_or(Value::Undefined);
            Ok(format!(
                "{}{}Zach",
                a.get("arg1").to_js_string(),
                a.get("arg2").to_js_string()
            ))
        });
        Ok(())
    })
}

#[test]
fn nunjucks_shortcode_named_args() {
    let (calls, c) = count_calls();
    let out = render(
        "{% postfixWithZach arg1=name, arg2=other %}",
        hi_page(r#""name": "test", "other": "howdy","#),
        named_args_options(c),
    );
    assert_eq!(out.unwrap(), "testhowdyZach");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn nunjucks_shortcode_named_args_reverse_order() {
    let (calls, c) = count_calls();
    let out = render(
        "{% postfixWithZach arg2=other, arg1=name %}",
        hi_page(r#""name": "test", "other": "howdy","#),
        named_args_options(c),
    );
    assert_eq!(out.unwrap(), "testhowdyZach");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn nunjucks_shortcode_named_args_are_a_trailing_object_marked_with_keywords() {
    let options = config(Some("./test/stubs/"), |cfg| {
        cfg.add_nunjucks_shortcode("argsToJson", |_, args| {
            Ok(Value::array(args.to_vec()).to_json_string())
        });
        Ok(())
    });
    let out = render(
        "{% argsToJson name, key=other %}",
        json(r#"{"name": "test", "other": "howdy"}"#),
        options,
    );
    assert_eq!(
        out.unwrap(),
        r#"["test",{"key":"howdy","__keywords":true}]"#
    );
}

#[test]
fn nunjucks_shortcode_named_args_js_notation() {
    let (calls, c) = count_calls();
    let out = render(
        "{% postfixWithZach { arg1: name, arg2: other } %}",
        hi_page(r#""name": "test", "other": "howdy","#),
        named_args_options(c),
    );
    assert_eq!(out.unwrap(), "testhowdyZach");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn nunjucks_if_statements_on_arrays_issue_524() {
    let tags = || json(r#"{"tags": ["first", "second"]}"#);
    let cases = [
        "{% if 'first' in tags %}Success.{% endif %}",
        "{% if 'sdfsdfs' in tags %}{% else %}Success.{% endif %}",
        "{% if false %}{% elseif 'first' in tags %}Success.{% endif %}",
        "{% if tags.includes('first') %}Success.{% endif %}",
        "{% if tags.includes('dsds') %}{% else %}Success.{% endif %}",
        "{% if false %}{% elseif tags.includes('first') %}Success.{% endif %}",
    ];
    for src in cases {
        let out = render(src, tags(), input("./test/stubs/"));
        assert_eq!(out.unwrap(), "Success.", "{src}");
    }
}

#[test]
fn issue_611_run_a_function() {
    let mut data = Value::object();
    data.set(
        "test",
        Value::function(|_| Ok(Value::str("alkdsjfksljaZach"))),
    );
    let out = render("{{ test() }}", data, input("./test/stubs/"));
    assert_eq!(out.unwrap(), "alkdsjfksljaZach");
}

#[test]
fn nunjucks_bypass_compilation() {
    let ctx = get_context(TestOptions::default()).unwrap();
    let tr = TemplateRender::new(ctx, "njk").unwrap();
    assert!(tr.engine.needs_compilation("<p>{{ me }}</p>"));
    assert!(tr.engine.needs_compilation("<p>{% tag %}{% endtag %}</p>"));
    assert!(!tr.engine.needs_compilation("<p>test</p>"));
}

#[test]
fn nunjucks_parse_for_symbols() {
    let ctx = get_context(TestOptions::default()).unwrap();
    let tr = TemplateRender::new(ctx, "njk").unwrap();
    let symbols = |src: &str| tr.engine.parse_for_symbols(src).unwrap();
    assert_eq!(symbols("<p>{{ name }}</p>"), ["name"]);
    assert_eq!(
        symbols("<p>{{ twelvety.deep.nested }}</p>"),
        ["twelvety.deep.nested"]
    );
    assert_eq!(symbols("<p>{{ a }} {{ b }}</p>"), ["a", "b"]);
    assert_eq!(symbols("<p>{% if true %}{{ c }}{% endif %}</p>"), ["c"]);
    assert_eq!(symbols("<p>{% if false %}{{ c }}{% endif %}</p>"), ["c"]);
    // Note that the Liquid parser returns collections.all[0]
    assert_eq!(symbols("{{ collections.all[0] }}>"), ["collections.all"]);
    assert_eq!(symbols("{{ collections.mine }}>"), ["collections.mine"]);
    // TODO (upstream) not ideal to have `test` in here?
    assert_eq!(
        symbols("{{ collections.mine | test }}>"),
        ["test", "collections.mine"]
    );
}

#[test]
fn nunjucks_parse_for_symbols_with_custom_block() {
    let options = config(None, |cfg| {
        cfg.add_nunjucks_shortcode("test", |_, _| Ok(String::new()));
        Ok(())
    });
    let ctx = get_context(options).unwrap();
    let tr = TemplateRender::new(ctx, "njk").unwrap();
    assert_eq!(
        tr.engine
            .parse_for_symbols("<p>{{ name }} {% test %}</p>")
            .unwrap(),
        ["name"]
    );
}

#[test]
fn use_add_nunjucks_global_with_function() {
    let options = config(None, |cfg| {
        cfg.add_nunjucks_global("fortytwo", Value::function(|_| Ok(Value::Number(42.0))));
        Ok(())
    });
    let out = render("<p>{{ fortytwo() }}</p>", Value::object(), options);
    assert_eq!(out.unwrap(), "<p>42</p>");
}

#[test]
fn use_add_nunjucks_global_with_literal() {
    let options = config(None, |cfg| {
        cfg.add_nunjucks_global("fortytwo", Value::Number(42.0));
        Ok(())
    });
    let out = render("<p>{{ fortytwo }}</p>", Value::object(), options);
    assert_eq!(out.unwrap(), "<p>42</p>");
}

#[test]
fn use_config_driven_nunjucks_environment_options_throw_on_undefined() {
    let options = config(None, |cfg| {
        cfg.set_nunjucks_environment_options(json(r#"{"throwOnUndefined": true}"#));
        Ok(())
    });
    let out = render("<p>   {{ test }}</p>", Value::object(), options);
    assert!(out.is_err());
}

#[test]
fn use_config_driven_nunjucks_environment_options_autoescape() {
    let options = config(None, |cfg| {
        cfg.set_nunjucks_environment_options(json(r#"{"autoescape": false}"#));
        Ok(())
    });
    let out = render(
        "<p>{{ test }}</p>",
        json(r#"{"test": "<b>Hi</b>"}"#),
        options,
    );
    assert_eq!(out.unwrap(), "<p><b>Hi</b></p>");
}

fn generic_shortcode_options() -> TestOptions {
    config(Some("./test/stubs-njk-async/"), |cfg| {
        cfg.add_nunjucks_shortcode("genericshortcode", |_, args| Ok(arg(args, 0)));
        Ok(())
    })
}

#[test]
fn nunjucks_shortcode_in_a_loop_everything_is_sync() {
    let out = render(
        "{% for item in list %}{% include 'loop.njk' %}{% endfor %}",
        json(r#"{"list": ["a", "b", "c"]}"#),
        generic_shortcode_options(),
    );
    assert_eq!(out.unwrap(), "included_a-aincluded_b-bincluded_c-c");
}

#[test]
fn weird_issue_with_number_arguments_in_a_loop() {
    let out = render(
        "{% for item in list %}{{item}}-{% genericshortcode item %}{% endfor %}",
        json(r#"{"list": [1, 2, 3]}"#),
        generic_shortcode_options(),
    );
    assert_eq!(out.unwrap(), "1-12-23-3");
}

fn fortytwo_filter(_: &CallContext, value: Value, args: &[Value]) -> Result<Value> {
    let other = args.first().cloned().unwrap_or(Value::Undefined);
    Ok(Value::Number(value.to_number() + other.to_number()))
}

#[test]
fn make_sure_add_filter_is_async_friendly_for_nunjucks() {
    let options = config(None, |cfg| {
        cfg.add_filter("fortytwo", fortytwo_filter);
        Ok(())
    });
    let out = render("<p>{{ 10 | fortytwo(2) }}</p>", Value::object(), options);
    assert_eq!(out.unwrap(), "<p>12</p>");
}

#[test]
fn add_async_filter_for_nunjucks() {
    let options = config(None, |cfg| {
        cfg.add_async_filter("fortytwo", fortytwo_filter);
        Ok(())
    });
    let out = render("<p>{{ 10 | fortytwo(2) }}</p>", Value::object(), options);
    assert_eq!(out.unwrap(), "<p>12</p>");
}

#[test]
fn rejected_async_filter_fails_the_nunjucks_render() {
    let options = config(None, |cfg| {
        cfg.add_async_filter("boom", |_, _, _| Err(anyhow!("Boom")));
        Ok(())
    });
    let err = render("<p>{{ 'hi' | boom }}</p>", Value::object(), options).unwrap_err();
    let msg = error_chain(&err);
    assert!(msg.contains("Error in Nunjucks Filter `boom`"), "{msg}");
    assert!(msg.contains("Boom"), "{msg}");
}

#[test]
fn asynchronous_filters_via_add_nunjucks_filter() {
    let options = config(None, |cfg| {
        cfg.add_nunjucks_filter("fortytwo", fortytwo_filter);
        Ok(())
    });
    let out = render("<p>{{ 10 | fortytwo(2) }}</p>", Value::object(), options);
    assert_eq!(out.unwrap(), "<p>12</p>");
}

/* TwelvetyNunjucksTest.js */

fn build(
    input: &str,
    output: &str,
    config: impl Fn(&mut UserConfig) -> Result<()> + Send + Sync + 'static,
) -> Vec<String> {
    let options = Options {
        config_path: Some(String::new()),
        quiet_mode: Some(true),
        ..Default::default()
    }
    .with_config(config);
    let mut elev = Twelvety::new(Some(input), Some(output), options);
    elev.to_json()
        .unwrap()
        .into_iter()
        .map(|r| r.content)
        .collect()
}

#[test]
fn paired_shortcodes_in_macros_2261_1749() {
    let results = build("./test/stubs-2261/", "./test/stubs-2261/_site", |cfg| {
        cfg.add_paired_shortcode("sample", |_, content, args| {
            Ok(format!("{content} {}", arg(args, 0)))
        });
        Ok(())
    });
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].trim(), "<div>HelloHello Manuel</div>");
}

fn crossover_config(
    template: &'static str,
) -> impl Fn(&mut UserConfig) -> Result<()> + Send + Sync + 'static {
    move |cfg| {
        cfg.add_paired_shortcode("sync", |_, content, _| Ok(format!("abc{content}")));
        cfg.add_paired_async_shortcode("async", |_, content, _| {
            Ok(format!("{}xyz", content.to_uppercase()))
        });
        cfg.add_paired_shortcode("pseudoasync", |_, content, _| {
            Ok(format!("{}xyz", content.to_uppercase()))
        });
        cfg.add_template("index.njk", template, Value::object())
    }
}

#[test]
fn async_to_sync_crossover_shortcodes_4305_2261() {
    let results = build(
        "./test/stubs-virtual",
        "./test/stubs-virtual/_site",
        crossover_config("{% async %}before{% sync %}test{% endsync %}after{% endasync %}"),
    );
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].trim(), "BEFOREABCTESTAFTERxyz");
}

#[test]
fn sync_to_async_crossover_shortcodes_4305_2261() {
    let results = build(
        "./test/stubs-virtual",
        "./test/stubs-virtual/_site",
        crossover_config("{% sync %}before{% async %}test{% endasync %}after{% endsync %}"),
    );
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].trim(), "abcbeforeTESTxyzafter");
}

#[test]
fn sync_to_pseudo_async_crossover_shortcodes_4305_2261() {
    let results = build(
        "./test/stubs-virtual",
        "./test/stubs-virtual/_site",
        crossover_config(
            "{% sync %}before{% pseudoasync %}test{% endpseudoasync %}after{% endsync %}",
        ),
    );
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].trim(), "abcbeforeTESTxyzafter");
}
