//! Tests for the Liquid engine (ported from upstream `test/TemplateRenderLiquidTest.js`).
//!
//! Not ported (JavaScript-only APIs):
//! - "Liquid Custom Tag prefixWithZach", "Liquid Custom Tag postfixWithZach",
//!   "Liquid Custom Tag Unquoted String", "Liquid addTag errors", "Liquid addTags",
//!   "Issue 347: Liquid addTags with space in argument": `addTag`/`addCustomTags`
//!   take liquidjs tag objects (`parse`/`render` with liquidjs internals).
//! - "Liquid Render: with Library Override": `setLibrary(new Liquid())`.
//! - "Issue 611: Run a function": relies on a liquidjs `Drop` subclass instance.
//! - "Liquid Options Overrides": inspects the merged liquidjs options object
//!   (`getLiquidOptions()`); the `dynamicPartials: false` behaviour itself is
//!   covered by the unquoted include tests below.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use twelvety::template_render::TemplateRender;
use twelvety::testing::{TestOptions, get_context, json, render_string};
use twelvety::{Options, Twelvety, UserConfig, Value};

fn options(input: Option<&str>) -> TestOptions {
    match input {
        Some(dir) => TestOptions::input(dir),
        None => TestOptions::default(),
    }
}

fn no_dynamic_partials(cfg: &mut UserConfig) -> Result<()> {
    cfg.set_liquid_options(json(r#"{"dynamicPartials": false}"#));
    Ok(())
}

/// `getNewTemplateRender("liquid")` + `getCompiledTemplate(src)` + `fn(data)`
fn render(src: &str, data: Value) -> Result<String> {
    render_string("liquid", src, data, options(None))
}

/// Same, with `./test/stubs/` as the input directory
fn render_stubs(src: &str, data: Value) -> Result<String> {
    render_string("liquid", src, data, options(Some("./test/stubs/")))
}

fn render_stubs_static(src: &str) -> Result<String> {
    render_string(
        "liquid",
        src,
        Value::object(),
        TestOptions::input("./test/stubs/").with_config(no_dynamic_partials),
    )
}

fn render_with<F>(src: &str, data: Value, config: F) -> Result<String>
where
    F: Fn(&mut UserConfig) -> Result<()> + Send + Sync + 'static,
{
    render_string(
        "liquid",
        src,
        data,
        TestOptions::input("./test/stubs/").with_config(config),
    )
}

fn page_data(extra: &str) -> Value {
    let mut data = json(extra);
    data.set("page", json(r#"{"url": "/hi/"}"#));
    data
}

fn s(v: &Value) -> String {
    v.to_js_string()
}

/// `JSON.stringify(args)`; `__keywords` is a non-enumerable property upstream so it is omitted
fn args_to_json(args: &[Value]) -> String {
    let args = args
        .iter()
        .map(|arg| {
            let mut arg = arg.clone();
            if arg.is_plain_object() {
                arg.remove("__keywords");
            }
            arg
        })
        .collect();
    Value::array(args).to_json_string()
}

/// Build a virtual project in `./test/stubs-virtual/` and return the first page’s content
fn build_virtual<F>(input: &str, config: F) -> Result<String>
where
    F: Fn(&mut UserConfig) -> Result<()> + Send + Sync + 'static,
{
    let options = Options {
        config_path: Some(String::new()),
        quiet_mode: Some(true),
        ..Default::default()
    }
    .with_config(config);
    let mut elev = Twelvety::new(Some(input), None, options);
    elev.disable_logger();
    let results = elev.to_json()?;
    Ok(results.into_iter().next().expect("one result").content)
}

#[test]
fn liquid() {
    let ctx = get_context(options(None)).unwrap();
    let tr = TemplateRender::new(ctx, "liquid").unwrap();
    assert_eq!(tr.get_engine_by_name("liquid").unwrap().name(), "liquid");
}

#[test]
fn render_addition() {
    assert_eq!(
        render("<p>{{ number | plus: 1 }}</p>", json(r#"{"number": 1}"#)).unwrap(),
        "<p>2</p>"
    );
}

#[test]
fn render_raw() {
    assert_eq!(
        render(
            "<p>{% raw %}{{name}}{% endraw %}</p>",
            json(r#"{"name": "tim"}"#)
        )
        .unwrap(),
        "<p>{{name}}</p>"
    );
}

#[test]
fn render_raw_multiline() {
    assert_eq!(
        render(
            "<p>{% raw %}\n{{name}}\n{% endraw %}</p>",
            json(r#"{"name": "tim"}"#)
        )
        .unwrap(),
        "<p>\n{{name}}\n</p>"
    );
}

#[test]
fn render_with_helper() {
    assert_eq!(
        render("<p>{{name | capitalize}}</p>", json(r#"{"name": "tim"}"#)).unwrap(),
        "<p>Tim</p>"
    );
}

#[test]
fn render_include() {
    assert_eq!(
        render_stubs_static("<p>{% include included %}</p>").unwrap(),
        "<p>This is an include.</p>"
    );
}

#[test]
fn render_relative_include_dynamic_partials_off() {
    // When the input path is `liquid`, this *only* uses _includes relative paths
    assert_eq!(
        render_stubs_static("<p>{% include ./included %}</p>").unwrap(),
        "<p>This is an include.</p>"
    );
}

#[test]
fn render_relative_include_dynamic_partials_on() {
    assert_eq!(
        render_stubs("<p>{% include './included' %}</p>", Value::object()).unwrap(),
        "<p>This is an include.</p>"
    );
}

#[test]
fn render_relative_current_dir_include() {
    let out = render_string(
        "./test/stubs/relative-liquid/does_not_exist_and_thats_ok.liquid",
        "<p>{% include ./dir/included %}</p>",
        Value::object(),
        TestOptions::input("./test/stubs/").with_config(no_dynamic_partials),
    )
    .unwrap();
    assert_eq!(out, "<p>TIME IS RELATIVE.</p>");
}

#[test]
fn render_relative_parent_dir_include() {
    let out = render_string(
        "./test/stubs/relative-liquid/dir/does_not_exist_and_thats_ok.liquid",
        "<p>{% include ../dir/included %}</p>",
        Value::object(),
        TestOptions::input("./test/stubs/").with_config(no_dynamic_partials),
    )
    .unwrap();
    assert_eq!(out, "<p>TIME IS RELATIVE.</p>");
}

#[test]
fn render_relative_include_ignores_includes_dir() {
    let out = render_string(
        "./test/stubs/does_not_exist_and_thats_ok.liquid",
        "<p>{% include './included' %}</p>",
        Value::object(),
        options(Some("./test/stubs/")),
    )
    .unwrap();
    assert_eq!(out, "<p>This is not in the includes dir.</p>");
}

#[test]
fn render_include_with_liquid_suffix() {
    assert_eq!(
        render_stubs_static("<p>{% include included.liquid %}</p>").unwrap(),
        "<p>This is an include.</p>"
    );
}

#[test]
fn render_include_with_html_suffix() {
    assert_eq!(
        render_stubs_static("<p>{% include included.html %}</p>").unwrap(),
        "<p>This is an include.</p>"
    );
}

#[test]
fn render_include_with_html_suffix_and_data() {
    assert_eq!(
        render_stubs_static("{% include included-data.html, myVariable: 'myValue' %}")
            .unwrap()
            .trim(),
        "This is an include. myValue"
    );
}

#[test]
fn custom_filter() {
    let out = render_with("{{ 'test' | prefixWithZach }}", Value::object(), |cfg| {
        cfg.add_liquid_filter("prefixWithZach", |_, val, _| {
            Ok(Value::str(format!("Zach{}", s(&val))))
        });
        Ok(())
    })
    .unwrap();
    assert_eq!(out, "Zachtest");
}

#[test]
fn async_filter() {
    let out = render_with("{{ 'test' | myAsyncFilter }}", Value::object(), |cfg| {
        cfg.add_async_filter("myAsyncFilter", |_, val, _| {
            Ok(Value::str(format!("HI{}", s(&val))))
        });
        Ok(())
    })
    .unwrap();
    assert_eq!(out.trim(), "HItest");
}

#[test]
fn issue_3206_strict_variables_and_custom_filters_in_includes() {
    let out = render_with(
        "<p>{% render \"custom-filter\", name: \"Zach\" %}</p>",
        Value::object(),
        |cfg| {
            cfg.set_liquid_options(json(r#"{"strictVariables": true}"#));
            cfg.add_liquid_filter("makeItFoo", |_, _, _| Ok(Value::str("foo")));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(out, "<p>foo</p>");
}

/// Shortcode that records `this.page.url` (upstream’s in-callback assertions)
fn postfix_shortcode(
    seen: Arc<Mutex<Vec<String>>>,
) -> impl Fn(&mut UserConfig) -> Result<()> + Send + Sync {
    move |cfg| {
        let seen = seen.clone();
        cfg.add_shortcode("postfixWithZach", move |ctx, args| {
            seen.lock().unwrap().push(s(&ctx.page().get("url")));
            Ok(format!("{}Zach", s(&args[0])))
        });
        Ok(())
    }
}

#[test]
fn shortcode() {
    let seen = Arc::new(Mutex::new(vec![]));
    let out = render_with(
        "{% postfixWithZach name %}",
        page_data(r#"{"name": "test"}"#),
        postfix_shortcode(seen.clone()),
    )
    .unwrap();
    assert_eq!(out, "testZach");
    assert_eq!(*seen.lock().unwrap(), vec!["/hi/".to_string()]);
}

#[test]
fn shortcode_returns_promise() {
    // Async shortcodes are regular shortcodes in Rust (covers the three upstream promise variants)
    let seen = Arc::new(Mutex::new(vec![]));
    let s2 = seen.clone();
    let out = render_with(
        "{% postfixWithZach name %}",
        page_data(r#"{"name": "test"}"#),
        move |cfg| {
            let seen = s2.clone();
            cfg.add_async_shortcode("postfixWithZach", move |ctx, args| {
                seen.lock().unwrap().push(s(&ctx.page().get("url")));
                Ok(format!("{}Zach", s(&args[0])))
            });
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(out, "testZach");
    assert_eq!(*seen.lock().unwrap(), vec!["/hi/".to_string()]);
}

#[test]
fn shortcode_safe_output() {
    let out = render_with(
        "{% postfixWithZach name %}",
        page_data(r#"{"name": "test"}"#),
        |cfg| {
            cfg.add_shortcode("postfixWithZach", |ctx, args| {
                assert_eq!(s(&ctx.page().get("url")), "/hi/");
                Ok(format!("<span>{}</span>", s(&args[0])))
            });
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(out, "<span>test</span>");
}

#[test]
fn paired_shortcode() {
    let out = render_with(
        "{% postfixWithZach name %}Content{% endpostfixWithZach %}",
        page_data(r#"{"name": "test"}"#),
        |cfg| {
            cfg.add_paired_shortcode("postfixWithZach", |ctx, content, args| {
                assert_eq!(s(&ctx.page().get("url")), "/hi/");
                Ok(format!("{}{content}Zach", s(&args[0])))
            });
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(out, "testContentZach");
}

#[test]
fn async_paired_shortcode() {
    let out = render_with(
        "{% postfixWithZach name %}Content{% endpostfixWithZach %}",
        page_data(r#"{"name": "test"}"#),
        |cfg| {
            cfg.add_paired_async_shortcode("postfixWithZach", |ctx, content, args| {
                assert_eq!(s(&ctx.page().get("url")), "/hi/");
                Ok(format!("{}{content}Zach", s(&args[0])))
            });
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(out, "testContentZach");
}

#[test]
fn render_include_subfolder() {
    for src in [
        "<p>{% include subfolder/included.liquid %}</p>",
        "<p>{% include subfolder/included.html %}</p>",
        "<p>{% include subfolder/included %}</p>",
    ] {
        assert_eq!(
            render_stubs_static(src).unwrap(),
            "<p>This is an include.</p>",
            "{src}"
        );
    }
}

// Related to https://github.com/harttle/liquidjs/issues/61
// (dynamicPartials defaults to true since Twelvety 1.0)
#[test]
fn render_include_subfolder_quoted() {
    for src in [
        "<p>{% include 'subfolder/included.liquid' %}</p>",
        "<p>{% include \"subfolder/included.liquid\" %}</p>",
        "<p>{% include 'subfolder/included.html' %}</p>",
        "<p>{% include \"subfolder/included.html\" %}</p>",
        "<p>{% include 'subfolder/included' %}</p>",
        "<p>{% include \"subfolder/included\" %}</p>",
        "<p>{% include 'subfolder/included.html', myVariable: 'myValue' %}</p>",
        "<p>{% include \"subfolder/included.html\", myVariable: \"myValue\" %}</p>",
    ] {
        assert_eq!(
            render_stubs(src, Value::object()).unwrap(),
            "<p>This is an include.</p>",
            "{src}"
        );
    }
}

#[test]
fn render_include_subfolder_relative_current_dir_dynamic_partials() {
    let out = render_string(
        "./test/stubs/does_not_exist_and_thats_ok.liquid",
        "<p>{% include './relative-liquid/dir/included' %}</p>",
        Value::object(),
        options(Some("./test/stubs/")),
    )
    .unwrap();
    assert_eq!(out, "<p>TIME IS RELATIVE.</p>");
}

#[test]
fn render_include_subfolder_relative_parent_dir_dynamic_partials() {
    let out = render_string(
        "./test/stubs/subfolder/does_not_exist_and_thats_ok.liquid",
        "<p>{% include '../relative-liquid/dir/included' %}</p>",
        Value::object(),
        options(Some("./test/stubs/")),
    )
    .unwrap();
    assert_eq!(out, "<p>TIME IS RELATIVE.</p>");
}

fn paired_postfix(cfg: &mut UserConfig) -> Result<()> {
    cfg.add_paired_shortcode("postfixWithZach", |_, content, args| {
        Ok(format!("{}{content}Zach", s(&args[0])))
    });
    Ok(())
}

#[test]
fn paired_shortcode_with_tag_inside() {
    let out = render_with(
        "{% postfixWithZach name %}Content{% if tester %}If{% endif %}{% endpostfixWithZach %}",
        json(r#"{"name": "test", "tester": true}"#),
        paired_postfix,
    )
    .unwrap();
    assert_eq!(out, "testContentIfZach");
}

#[test]
fn nested_paired_shortcode() {
    let out = render_with(
        "{% postfixWithZach name %}Content{% postfixWithZach name2 %}Content{% endpostfixWithZach %}{% endpostfixWithZach %}",
        json(r#"{"name": "test", "name2": "test2"}"#),
        paired_postfix,
    )
    .unwrap();
    assert_eq!(out, "testContenttest2ContentZachZach");
}

#[test]
fn shortcode_multiple_args() {
    let out = render_with(
        "{% postfixWithZach name other %}",
        json(r#"{"name": "test", "other": "howdy"}"#),
        |cfg| {
            cfg.add_shortcode("postfixWithZach", |_, args| {
                Ok(format!("{}{}Zach", s(&args[0]), s(&args[1])))
            });
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(out, "testhowdyZach");
}

#[test]
fn shortcode_keyword_args() {
    let out = render_with(
        "{% postfixWithZach name prepend: 'string' append: other %}",
        json(r#"{"name": "test", "other": "howdy"}"#),
        |cfg| {
            cfg.add_shortcode("postfixWithZach", |_, args| {
                let kwargs = &args[1];
                assert_eq!(kwargs.get("__keywords"), Value::Bool(true));
                // `__keywords` is non-enumerable upstream
                let keys: Vec<String> = kwargs
                    .keys()
                    .into_iter()
                    .filter(|k| k != "__keywords")
                    .collect();
                assert_eq!(keys, vec!["prepend", "append"]);
                Ok(format!(
                    "{}{}Zach{}",
                    s(&kwargs.get("prepend")),
                    s(&args[0]),
                    s(&kwargs.get("append"))
                ))
            });
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(out, "stringtestZachhowdy");
}

#[test]
fn shortcode_only_keyword_args_comma_separated() {
    let out = render_with(
        "{% postfixWithZach prepend: 'string', append: name %}",
        json(r#"{"name": "test"}"#),
        |cfg| {
            cfg.add_shortcode("postfixWithZach", |_, args| {
                let kwargs = &args[0];
                Ok(format!(
                    "{}Zach{}",
                    s(&kwargs.get("prepend")),
                    s(&kwargs.get("append"))
                ))
            });
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(out, "stringZachtest");
}

fn args_to_json_shortcode(cfg: &mut UserConfig) -> Result<()> {
    cfg.add_shortcode("argsToJson", |_, args| Ok(args_to_json(args)));
    Ok(())
}

#[test]
fn shortcode_keyword_args_mixed_with_property_access_and_literals() {
    let out = render_with(
        "{% argsToJson page.url, arr[0], key: arr[1], true, -1, 'a:b' %}",
        json(r#"{"page": {"url": "/hi/"}, "arr": [1, 2]}"#),
        args_to_json_shortcode,
    )
    .unwrap();
    assert_eq!(out, r#"["/hi/",1,true,-1,"a:b",{"key":2}]"#);
}

#[test]
fn shortcode_without_keyword_args_has_no_trailing_object() {
    let out = render_with(
        "{% argsToJson name %}",
        json(r#"{"name": "test"}"#),
        args_to_json_shortcode,
    )
    .unwrap();
    assert_eq!(out, r#"["test"]"#);
}

#[test]
fn paired_shortcode_keyword_args() {
    let out = render_with(
        "{% postfixWithZach str: name %}Content{% postfixWithZach str: name2 %}Content{% if tester %}If{% endif %}{% endpostfixWithZach %}{% endpostfixWithZach %}",
        json(r#"{"name": "test", "name2": "test2", "tester": true}"#),
        |cfg| {
            cfg.add_paired_shortcode("postfixWithZach", |_, content, args| {
                Ok(format!("{}{content}Zach", s(&args[0].get("str"))))
            });
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(out, "testContenttest2ContentIfZachZach");
}

#[test]
fn include_scope_leak() {
    // By design, `include` assigns to its parent scope (use `render` for a clean scope)
    // see: https://github.com/harttle/liquidjs/issues/404#issuecomment-955660149
    assert_eq!(
        render_stubs(
            "<p>{% include 'scopeleak' %}{{ test }}</p>",
            json(r#"{"test": 1}"#)
        )
        .unwrap(),
        "<p>22</p>"
    );
}

#[test]
fn render_scope_leak() {
    assert_eq!(
        render_stubs(
            "<p>{% render 'scopeleak' %}-{{ test }}</p>",
            json(r#"{"test": 1}"#)
        )
        .unwrap(),
        "<p>2-1</p>"
    );
}

#[test]
fn missing_filter_issue_183_no_strict_filters() {
    let out = render_with("{{ 'test' | prefixWithZach }}", Value::object(), |cfg| {
        cfg.set_liquid_options(json(r#"{"strictFilters": false}"#));
        Ok(())
    });
    assert!(out.is_ok(), "{out:?}");
}

#[test]
fn missing_filter_issue_183() {
    assert!(render_stubs("{{ 'test' | prefixWithZach }}", Value::object()).is_err());
}

#[test]
fn issue_258_render_date() {
    let date = chrono::DateTime::parse_from_rfc3339("2016-01-01T00:00:00Z")
        .unwrap()
        .to_utc();
    let mut data = Value::object();
    data.set("myDate", Value::Date(date));
    let out = render("<p>{{ myDate }}</p>", data).unwrap();
    assert!(out.starts_with("<p>"));
    assert!(out.ends_with("</p>"));
    assert!(!out.contains('"'));
}

fn str_zach_shortcode(name: &'static str) -> impl Fn(&mut UserConfig) -> Result<()> + Send + Sync {
    move |cfg| {
        cfg.add_shortcode(name, |_, args| {
            let parts: String = args.iter().map(s).collect();
            Ok(format!("{parts}Zach"))
        });
        Ok(())
    }
}

#[test]
fn issue_347_shortcode_string_argument() {
    let data = json(r#"{"name": "alkdsjfkslja"}"#);
    assert_eq!(
        render_with(
            "{% issue347 'test' %}",
            data,
            str_zach_shortcode("issue347")
        )
        .unwrap(),
        "testZach"
    );
}

#[test]
fn issue_347_shortcode_string_argument_with_space_double_quotes() {
    let out = render_with(
        "{% issue347b \"test 2\" \"test 3\" %}",
        json(r#"{"name": "alkdsjfkslja"}"#),
        |cfg| {
            // Only uses the first argument
            cfg.add_shortcode("issue347b", |_, args| Ok(format!("{}Zach", s(&args[0]))));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(out, "test 2Zach");
}

#[test]
fn issue_347_shortcode_string_arguments() {
    let data = json(r#"{"name": "alkdsjfkslja"}"#);
    for (src, expected) in [
        ("{% issue347 'test 2' %}", "test 2Zach"),
        ("{% issue347 'test 2' \"test 3\" %}", "test 2test 3Zach"),
        ("{% issue347 'test 2', \"test 3\" %}", "test 2test 3Zach"),
        ("{% issue347 'test 2', 3 %}", "test 23Zach"),
        ("{% issue347 'test 2', 3.23 %}", "test 23.23Zach"),
    ] {
        assert_eq!(
            render_with(src, data.clone(), str_zach_shortcode("issue347")).unwrap(),
            expected,
            "{src}"
        );
    }
}

#[test]
fn issue_347_shortcode_boolean_argument() {
    let config = |cfg: &mut UserConfig| {
        cfg.add_shortcode("issue347", |_, args| {
            Ok(if args[0].is_truthy() {
                "Zach"
            } else {
                "Not Zach"
            }
            .to_string())
        });
        Ok(())
    };
    let data = json(r#"{"name": "alkdsjfkslja"}"#);
    assert_eq!(
        render_with("{% issue347 true %}", data.clone(), config).unwrap(),
        "Zach"
    );
    assert_eq!(
        render_with("{% issue347 false %}", data, config).unwrap(),
        "Not Zach"
    );
}

#[test]
fn issue_347_paired_shortcode_with_spaces() {
    let out = render_with(
        "{% postfixWithZach 'My Name', 1234, \"Other\" %}Content{% endpostfixWithZach %}",
        json(r#"{"name": "test"}"#),
        |cfg| {
            cfg.add_paired_shortcode("postfixWithZach", |_, content, args| {
                Ok(format!(
                    "{}{}{}{content}Zach",
                    s(&args[0]),
                    s(&args[1]),
                    s(&args[2])
                ))
            });
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(out, "My Name1234OtherContentZach");
}

#[test]
fn render_with_dash_variable_issue_567() {
    assert_eq!(
        render(
            "<p>{{ my-global-name }}</p>",
            json(r#"{"my-global-name": "Zach"}"#)
        )
        .unwrap(),
        "<p>Zach</p>"
    );
}

#[test]
fn issue_600_shortcode_argument_page_url() {
    let out = render_with(
        "{% issue600 page.url %}",
        json(r#"{"page": {"url": "alkdsjfkslja"}}"#),
        str_zach_shortcode("issue600"),
    )
    .unwrap();
    assert_eq!(out, "alkdsjfksljaZach");
}

#[test]
fn issue_600_shortcode_argument_with_dashes() {
    let out = render_with(
        "{% issue600b page-url %}",
        json(r#"{"page-url": "alkdsjfkslja"}"#),
        str_zach_shortcode("issue600b"),
    )
    .unwrap();
    assert_eq!(out, "alkdsjfksljaZach");
}

#[test]
fn issue_600_shortcode_argument_with_underscores() {
    let out = render_with(
        "{% issue600c page_url %}",
        json(r#"{"page_url": "alkdsjfkslja"}"#),
        str_zach_shortcode("issue600c"),
    )
    .unwrap();
    assert_eq!(out, "alkdsjfksljaZach");
}

#[test]
fn shortcode_error_throwing() {
    // Covers both the sync and async upstream variants
    let err = render_with(
        "{% postfixWithZach name %}",
        json(r#"{"name": "test"}"#),
        |cfg| {
            cfg.add_shortcode("postfixWithZach", |_, _| {
                anyhow::bail!("Liquid Shortcode (with sync function, error throwing)")
            });
            Ok(())
        },
    )
    .unwrap_err();
    assert!(
        format!("{err:#}").contains("Liquid Shortcode (with sync function, error throwing)"),
        "{err:#}"
    );
}

#[test]
fn render_a_false_1069() {
    assert_eq!(
        render("{{ falseValue }}", json(r#"{"falseValue": false}"#)).unwrap(),
        "false"
    );
}

#[test]
fn render_square_brackets_680() {
    let data = json(r#"{"test": {"hey-a": 1}}"#);
    for src in [
        "<p>{{ test['hey-a'] }}</p>",
        "<p>{{ test[ 'hey-a' ] }}</p>",
        "<p>{{ test[\"hey-a\"] }}</p>",
        "<p>{{ test[ \"hey-a\" ] }}</p>",
    ] {
        assert_eq!(render(src, data.clone()).unwrap(), "<p>1</p>", "{src}");
    }
}

#[test]
fn render_square_brackets_680_variable_reference() {
    assert_eq!(
        render(
            "<p>{{ test[ref] }}</p>",
            json(r#"{"test": {"hey-a": 1}, "ref": "hey-a"}"#)
        )
        .unwrap(),
        "<p>1</p>"
    );
    assert_eq!(
        render(
            "<p>{{ test[ref[0]] }}</p>",
            json(r#"{"test": {"hey-a": 1}, "ref": ["hey-a"]}"#)
        )
        .unwrap(),
        "<p>1</p>"
    );
}

fn engine() -> Arc<dyn twelvety::engines::TemplateEngine> {
    let ctx = get_context(options(None)).unwrap();
    TemplateRender::new(ctx, "liquid")
        .unwrap()
        .get_engine_by_name("liquid")
        .unwrap()
}

#[test]
fn bypass_compilation() {
    let engine = engine();
    assert!(engine.needs_compilation("<p>{{ me }}</p>"));
    assert!(engine.needs_compilation("<p>{% comment %}{% endcomment %}</p>"));
    assert!(!engine.needs_compilation("<p>test</p>"));
}

#[test]
fn reverse_filter_in_output() {
    // https://liquidjs.com/filters/reverse.html
    assert_eq!(
        render(
            "{{ test | reverse | join: ',' }}",
            json(r#"{"test": [1, 2, 3]}"#)
        )
        .unwrap(),
        "3,2,1"
    );
}

#[test]
fn reverse_filter_in_for() {
    // https://liquidjs.com/tags/for.html#reversed
    assert_eq!(
        render(
            "{% for num in test reversed %}{{ num }}{% endfor %}",
            json(r#"{"test": [1, 2, 3]}"#)
        )
        .unwrap(),
        "321"
    );
}

#[test]
fn parse_for_symbols() {
    let engine = engine();
    let symbols = |src: &str| engine.parse_for_symbols(src).unwrap();
    assert_eq!(symbols("<p>{{ name }}</p>"), vec!["name"]);
    assert_eq!(
        symbols("<p>{{ twelvety.deep.nested }}</p>"),
        vec!["twelvety.deep.nested"]
    );
    assert_eq!(symbols("<p>{{ a }} {{ b }}</p>"), vec!["a", "b"]);
    assert_eq!(symbols("<p>{% if true %}{{ c }}{% endif %}</p>"), vec!["c"]);
    assert_eq!(
        symbols("<p>{% if false %}{{ c }}{% endif %}</p>"),
        vec!["c"]
    );
    // Note that the Nunjucks parser returns collections.all
    assert_eq!(
        symbols("{{ collections.all[0] }}>"),
        vec!["collections.all[0]"]
    );
    assert_eq!(symbols("{{ collections.mine }}>"), vec!["collections.mine"]);
    assert_eq!(
        symbols("{{ collections.mine | test }}>"),
        vec!["collections.mine"]
    );
}

fn virtual_shortcode_test(src: &'static str, paired: bool) -> Result<String> {
    build_virtual("./test/stubs-virtual/", move |cfg| {
        if paired {
            cfg.add_paired_shortcode("test", |_, content, args| {
                let mut all = vec![Value::str(content)];
                all.extend_from_slice(args);
                Ok(args_to_json(&all))
            });
        } else {
            cfg.add_shortcode("test", |_, args| Ok(args_to_json(args)));
        }
        cfg.add_template("index.liquid", src, json(r#"{"abc": 123, "def": 456}"#))
    })
}

// `setLiquidParameterParsing("builtin")` is the only parsing mode in the port
#[test]
fn shortcode_builtin_argument_parsing() {
    for src in [
        "{% test abc def %}",
        "{% test abc, def %}",
        "{% test abc,def %}",
    ] {
        assert_eq!(
            virtual_shortcode_test(src, false).unwrap(),
            "[123,456]",
            "{src}"
        );
    }
}

#[test]
fn paired_shortcode_builtin_argument_parsing() {
    for src in [
        "{% test abc def %}hi{% endtest %}",
        "{% test abc, def %}hi{% endtest %}",
        "{% test abc,def %}hi{% endtest %}",
    ] {
        assert_eq!(
            virtual_shortcode_test(src, true).unwrap(),
            r#"["hi",123,456]"#,
            "{src}"
        );
    }
}

#[test]
fn js_truthy_default_changed_3507() {
    let out = build_virtual("./test/stubs-virtual/", |cfg| {
        cfg.add_template(
            "index.liquid",
            "{% if emptyString %}notempty{% endif %}-{% if zero %}notzero{% endif %}-{% if not emptyString %}empty{% endif %}-{% if not zero %}zero{% endif %}",
            json(r#"{"emptyString": "", "zero": 0}"#),
        )
    })
    .unwrap();
    assert_eq!(out, "--empty-zero");
}

#[test]
fn globals_inside_render_1541() {
    let out = build_virtual("./test/stubs/stubs-1541/", |cfg| {
        cfg.add_template(
            "index.liquid",
            "{% render \"render-source.liquid\" %}",
            Value::object(),
        )
    })
    .unwrap();
    assert_eq!(out, "/ via script collections.all size: 1");
}

#[test]
fn layout_tag_resolves_from_custom_layouts_directory_2233() {
    let out = build_virtual("./test/stubs/stubs-2233/", |cfg| {
        cfg.set_layouts_directory("layouts")?;
        cfg.add_template(
            "index.liquid",
            "{% layout \"base\" %}{% block content %}Hi{% endblock %}",
            Value::object(),
        )
    })
    .unwrap();
    assert_eq!(out, "Before Hi After");
}
