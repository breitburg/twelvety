//! Ported from upstream `test/TemplateEngineManagerTest.js`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use twelvety::Value;
use twelvety::testing::{TestOptions, get_context};
use twelvety::user_config::{CompiledFn, ExtensionOptions};

/// A plaintext `compile` callback: renders the template string as-is
fn plaintext(content: &str) -> CompiledFn {
    let content = content.to_string();
    Arc::new(move |_: &Value| Ok(content.clone()))
}

#[test]
fn unsupported_engine() {
    let ctx = get_context(TestOptions::default()).unwrap();
    assert!(ctx.engine_manager.get_engine("doesnotexist", &ctx.extension_map).is_err());
}

#[test]
fn supported_engine() {
    let ctx = get_context(TestOptions::default()).unwrap();
    assert!(ctx.engine_manager.has_engine("12ty.js"));
}

#[test]
fn supported_custom_engine() {
    let ctx = get_context(TestOptions::default().with_config(|cfg| {
        cfg.extension_map.push(ExtensionOptions {
            extension: "txt".into(),
            key: "txt".into(),
            compile: Some(Arc::new(|s: &str, _: &str| Ok(plaintext(s)))),
            ..Default::default()
        });
        Ok(())
    }))
    .unwrap();
    let tem = &ctx.engine_manager;

    assert!(tem.has_engine("txt"));
    let engine = tem.get_engine("txt", &ctx.extension_map).unwrap();
    let f = engine.compile("<p>This is plaintext</p>", None).unwrap();
    assert_eq!(f(&twelvety::testing::json(r#"{"author": "zach"}"#)).unwrap(), "<p>This is plaintext</p>");
}

#[test]
fn custom_engine_with_custom_init() {
    let init_count = Arc::new(AtomicUsize::new(0));
    let compile_count = Arc::new(AtomicUsize::new(0));
    let (ic, cc) = (init_count.clone(), compile_count.clone());
    let ctx = get_context(TestOptions::default().with_config(move |cfg| {
        let (ic, cc) = (ic.clone(), cc.clone());
        cfg.extension_map.push(ExtensionOptions {
            extension: "custom1".into(),
            key: "custom1".into(),
            // do custom things, but only once
            init: Some(Arc::new(move || {
                ic.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })),
            compile: Some(Arc::new(move |s: &str, _: &str| {
                cc.fetch_add(1, Ordering::SeqCst);
                Ok(plaintext(s))
            })),
            ..Default::default()
        });
        Ok(())
    }))
    .unwrap();
    let tem = &ctx.engine_manager;

    assert!(tem.has_engine("custom1"));
    let engine = tem.get_engine("custom1", &ctx.extension_map).unwrap();
    let f = engine.compile("<p>This is plaintext</p>", None).unwrap();
    assert_eq!(f(&Value::object()).unwrap(), "<p>This is plaintext</p>");

    let engine2 = tem.get_engine("custom1", &ctx.extension_map).unwrap();
    assert!(Arc::ptr_eq(&engine, &engine2));

    let f2 = engine2.compile("<p>This is plaintext</p>", None).unwrap();
    assert_eq!(f2(&Value::object()).unwrap(), "<p>This is plaintext</p>");

    assert_eq!(init_count.load(Ordering::SeqCst), 1, "Should have only run the init callback once");
    assert_eq!(compile_count.load(Ordering::SeqCst), 2, "Should have only run the compile callback twice");
}

// Skipped (JS-only):
// - "getEngineLib": returns the JS library instance (markdown-it); the Rust engines don’t expose one.
// - "Engine event listeners are removed when the engine cache is reset" and
//   "Nunjucks setLibrary does not add another templateModified listener": these count JS
//   `EventEmitter` listeners and use `setLibrary`, neither of which exists in the port.
