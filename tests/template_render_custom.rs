//! Ported from upstream `test/TemplateRenderCustomTest.js`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use twelvety::Value;
use twelvety::template_render::TemplateRender;
use twelvety::testing::{TestOptions, get_context, json};
use twelvety::user_config::{CompiledFn, CustomCompileFn, CustomCompileWithDefaultFn, ExtensionOptions, UserConfig};

fn get_new_template_render<F>(name: &str, config: F) -> TemplateRender
where
    F: Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
{
    TemplateRender::new(get_context(TestOptions::default().with_config(config)).unwrap(), name).unwrap()
}

fn render(tr: &TemplateRender, src: &str, data: &Value) -> String {
    tr.get_compiled_template(src, false, None).unwrap()(data).unwrap()
}

fn compile<F>(f: F) -> Option<CustomCompileFn>
where
    F: Fn(&str, &str) -> CompiledFn + Send + Sync + 'static,
{
    Some(Arc::new(move |content: &str, input_path: &str| Ok(f(content, input_path))))
}

/// `compile` that wraps `this.defaultRenderer(data)`
fn compile_wrapping_default<F>(wrap: F) -> Option<CustomCompileWithDefaultFn>
where
    F: Fn(&str) -> String + Send + Sync + Clone + 'static,
{
    Some(Arc::new(move |_: &str, _: &str, default_renderer: Option<CompiledFn>| {
        let default_renderer = default_renderer.expect("missing defaultRenderer");
        let wrap = wrap.clone();
        Ok(Arc::new(move |data: &Value| Ok(wrap(&default_renderer(data)?))) as CompiledFn)
    }))
}

fn counter() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

fn count(c: &Arc<AtomicUsize>) -> usize {
    c.load(Ordering::SeqCst)
}

#[test]
fn custom_plaintext_render() {
    let tr = get_new_template_render("txt", |cfg| {
        cfg.add_extension(
            "txt",
            ExtensionOptions {
                compile: compile(|s, _| {
                    let s = s.to_string();
                    Arc::new(move |_| Ok(s.clone()))
                }),
                ..Default::default()
            },
        )
    });
    assert_eq!(render(&tr, "<p>Paragraph</p>", &Value::Undefined), "<p>Paragraph</p>");
    assert_eq!(render(&tr, "<p>Paragraph</p>", &Value::object()), "<p>Paragraph</p>");
}

#[test]
fn custom_markdown_render_with_compile_override() {
    let tr = get_new_template_render("md", |cfg| {
        cfg.add_extension(
            "md",
            ExtensionOptions {
                compile: compile(|s, _| {
                    let s = s.to_string();
                    Arc::new(move |_| Ok(format!("<not-markdown>{}</not-markdown>", s.trim())))
                }),
                ..Default::default()
            },
        )
    });

    let f = tr.get_compiled_template("# Markdown?", false, None).unwrap();
    assert_eq!(f(&Value::Undefined).unwrap().trim(), "<not-markdown># Markdown?</not-markdown>");
    assert_eq!(f(&Value::object()).unwrap().trim(), "<not-markdown># Markdown?</not-markdown>");
}

#[test]
fn custom_markdown_render_without_compile_override() {
    let init_called = counter();
    let ic = init_called.clone();
    let tr = get_new_template_render("md", move |cfg| {
        let ic = ic.clone();
        cfg.add_extension(
            "md",
            ExtensionOptions {
                init: Some(Arc::new(move || {
                    ic.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })),
                ..Default::default()
            },
        )
    });

    let f = tr.get_compiled_template("# Header", false, None).unwrap();
    assert!(count(&init_called) > 0);
    assert_eq!(f(&Value::Undefined).unwrap().trim(), "<h1>Header</h1>");
    assert_eq!(f(&Value::object()).unwrap().trim(), "<h1>Header</h1>");
}

#[test]
fn custom_markdown_render_with_compile_override_and_call_to_default_compiler() {
    let tr = get_new_template_render("md", |cfg| {
        cfg.add_extension(
            "md",
            ExtensionOptions {
                compile_with_default_renderer: compile_wrapping_default(|result| {
                    format!("<custom-wrapper>{}</custom-wrapper>", result.trim())
                }),
                ..Default::default()
            },
        )
    });

    let f = tr.get_compiled_template("Hey {{name}}", false, None).unwrap();
    assert_eq!(f(&Value::Undefined).unwrap().trim(), "<custom-wrapper><p>Hey</p></custom-wrapper>");
    assert_eq!(
        f(&json(r#"{"name": "Zach"}"#)).unwrap().trim(),
        "<custom-wrapper><p>Hey Zach</p></custom-wrapper>"
    );
}

// Skipped (JS-only):
// - "Custom Vue Render" and "Custom Sass Render": depend on the `vue` and `sass` npm packages.
// - "JavaScript functions should not be mutable but not *that* mutable": exercises
//   `getInstanceFromInputPath`, `getData: [keys]` and `this.config.javascriptFunctions`.
// - "Return undefined in compile to ignore #2267": a Rust `compile` callback always returns a
//   render function; there is no `undefined` return value.
// - The three `test.skip` "Breaking Change (3.0)" tests (skipped upstream too; `12ty.js`).

#[test]
fn simple_alias_to_markdown_render() {
    let tr = get_new_template_render("mdx", |cfg| {
        cfg.add_extension(
            "mdx",
            ExtensionOptions {
                key: "md".into(),
                ..Default::default()
            },
        )
    });

    assert_eq!(render(&tr, "# Header", &Value::Undefined).trim(), "<h1>Header</h1>");
    assert_eq!(render(&tr, "# Header", &Value::object()).trim(), "<h1>Header</h1>");
}

#[test]
fn double_override_one_simple_alias_to_custom_works_fine() {
    let inits = counter();
    let compiles = counter();
    let (ic, cc) = (inits.clone(), compiles.clone());
    let tr = get_new_template_render("customhtml", move |cfg| {
        let (ic, cc) = (ic.clone(), cc.clone());
        cfg.add_extension(
            "12ty.possum",
            ExtensionOptions {
                init: Some(Arc::new(move || {
                    ic.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })),
                compile: compile(move |content, _| {
                    cc.fetch_add(1, Ordering::SeqCst);
                    let content = content.to_string();
                    Arc::new(move |_| Ok(format!("Appended {content}")))
                }),
                ..Default::default()
            },
        )?;
        cfg.add_extension(
            "customhtml",
            ExtensionOptions {
                key: "12ty.possum".into(),
                ..Default::default()
            },
        )?;
        cfg.set_template_formats(&["customhtml"]);
        Ok(())
    });

    assert_eq!(render(&tr, "Template content", &Value::object()), "Appended Template content");
    assert_eq!(count(&inits), 1);
    assert_eq!(count(&compiles), 1);
}

#[test]
fn double_override_two_simple_aliases() {
    let tr = get_new_template_render("customhtml", |cfg| {
        cfg.add_extension(
            "12ty.possum",
            ExtensionOptions {
                key: "html".into(),
                ..Default::default()
            },
        )?;
        cfg.add_extension(
            "customhtml",
            ExtensionOptions {
                key: "12ty.possum".into(),
                ..Default::default()
            },
        )?;
        cfg.set_template_formats(&["customhtml"]);
        Ok(())
    });

    assert_eq!(render(&tr, "Template content", &Value::object()), "Template content");
}

#[test]
fn double_override_two_complex_aliases_is_supported_as_of_3_0() {
    let inits = counter();
    let ic = inits.clone();
    let tr = get_new_template_render("12ty.custom", move |cfg| {
        let (ic1, ic2) = (ic.clone(), ic.clone());
        cfg.add_extension(
            "possum",
            ExtensionOptions {
                key: "html".into(),
                init: Some(Arc::new(move || {
                    ic1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })),
                compile_with_default_renderer: compile_wrapping_default(|result| format!("possum|{result}")),
                ..Default::default()
            },
        )?;
        cfg.add_extension(
            "12ty.custom",
            ExtensionOptions {
                key: "possum".into(),
                init: Some(Arc::new(move || {
                    ic2.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })),
                compile_with_default_renderer: compile_wrapping_default(|result| format!("12ty.custom|{result}")),
                ..Default::default()
            },
        )?;
        cfg.set_template_formats(&["possum", "12ty.custom"]);
        Ok(())
    });

    assert_eq!(
        render(&tr, "Template content", &Value::object()),
        "12ty.custom|possum|Template content"
    );
    assert_eq!(count(&inits), 2);
}

#[test]
fn double_override_not_aliases_throws_an_error() {
    let plaintext = || ExtensionOptions {
        compile: compile(|s, _| {
            let s = s.to_string();
            Arc::new(move |_| Ok(s.clone()))
        }),
        ..Default::default()
    };
    let result = get_context(TestOptions::default().with_config(move |cfg| {
        cfg.add_extension("md", plaintext())?;
        cfg.add_extension("md", plaintext())?;
        Ok(())
    }));
    let err = result.err().expect("config should fail");
    assert!(
        format!("{err:#}").contains(
            "An attempt was made to override the \"md\" template syntax twice (via the `addExtension` configuration API). A maximum of one override is currently supported."
        ),
        "{err:#}"
    );
}
