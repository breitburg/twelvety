//! Ported from upstream `test/ExtensionMapTest.js`.

use std::sync::Arc;

use twelvety::extension_map::ExtensionMap;
use twelvety::user_config::{ExtensionOptions, UserConfig};

fn get_extension_map(formats: &[&str], config: &UserConfig) -> ExtensionMap {
    let mut map = ExtensionMap::new(&config.extension_map);
    let formats: Vec<String> = formats.iter().map(|s| s.to_string()).collect();
    map.set_formats(&formats);
    map
}

fn map_for(formats: &[&str]) -> ExtensionMap {
    get_extension_map(formats, &UserConfig::new())
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// `cfg.userConfig.extensionMap.add({ key: "js", extension: "js" })`
fn config_with_js_extension() -> UserConfig {
    let mut cfg = UserConfig::new();
    cfg.extension_map.push(ExtensionOptions {
        key: "js".into(),
        extension: "js".into(),
        ..Default::default()
    });
    cfg
}

#[test]
fn empty_formats() {
    let map = map_for(&[]);
    assert_eq!(map.get_globs("."), Vec::<String>::new());
}

#[test]
fn single_format() {
    let map = map_for(&["liquid"]);
    assert_eq!(map.get_globs("."), strings(&["./**/*.liquid"]));
    assert_eq!(map.get_globs("src"), strings(&["./src/**/*.liquid"]));
}

#[test]
fn multiple_formats() {
    let map = map_for(&["njk", "liquid"]);
    assert_eq!(map.get_globs("."), strings(&["./**/*.{njk,liquid}"]));
    assert_eq!(map.get_globs("src"), strings(&["./src/**/*.{njk,liquid}"]));
}

#[test]
fn invalid_keys_are_filtered_using_passthrough_copy() {
    let map = map_for(&["lksdjfjlsk"]);
    assert_eq!(map.get_globs("."), strings(&["./**/*.lksdjfjlsk"]));
}

#[test]
fn keys_are_mapped_to_lower_case() {
    let map = map_for(&["LIQUID", "PUG", "NJK"]);
    assert_eq!(map.get_globs("."), strings(&["./**/*.{liquid,pug,njk}"]));
}

#[test]
fn pruned_globs() {
    let map = map_for(&["liquid", "njk", "png"]);
    assert_eq!(map.get_passthrough_copy_globs("."), strings(&["./**/*.png"]));
}

#[test]
fn empty_path_for_file_list() {
    let map = map_for(&["njk", "liquid"]);
    assert_eq!(map.get_file_list("", None), Vec::<String>::new());
}

#[test]
fn file_list() {
    let map = map_for(&["njk", "liquid"]);
    assert_eq!(map.get_file_list("filename", None), strings(&["filename.njk", "filename.liquid"]));
}

#[test]
fn file_list_with_dir() {
    let map = map_for(&["njk", "liquid"]);
    assert_eq!(
        map.get_file_list("filename", Some("_includes")),
        strings(&["_includes/filename.njk", "_includes/filename.liquid"])
    );
}

#[test]
fn file_list_with_dir_in_path() {
    let map = map_for(&["njk", "liquid"]);
    assert_eq!(
        map.get_file_list("layouts/filename", None),
        strings(&["layouts/filename.njk", "layouts/filename.liquid"])
    );
}

#[test]
fn file_list_with_dir_in_path_and_dir() {
    let map = map_for(&["njk", "liquid", "pug"]);
    assert_eq!(
        map.get_file_list("layouts/filename", Some("_includes")),
        strings(&["_includes/layouts/filename.njk", "_includes/layouts/filename.liquid"])
    );
}

#[test]
fn remove_template_extension() {
    let map = map_for(&["njk", "12ty.js"]);
    assert_eq!(map.remove_template_extension("component.njk"), "component");
    assert_eq!(map.remove_template_extension("component.12ty.js"), "component");

    assert_eq!(map.remove_template_extension(""), "");
    assert_eq!(map.remove_template_extension("component"), "component");
    assert_eq!(map.remove_template_extension("component.js"), "component.js");
}

#[test]
fn has_engine() {
    let map = map_for(&["liquid", "njk", "12ty.js"]);
    assert!(map.has_engine("default.liquid"));
    assert_eq!(map.get_key("default.liquid").as_deref(), Some("liquid"));
    assert_eq!(map.get_key(""), None);
    assert_eq!(map.get_key("LiQuid").as_deref(), Some("liquid"));
    assert!(map.has_engine("LiqUiD"));
    assert!(map.has_engine("liquid"));
    assert_eq!(map.get_key("sldkjfkldsj"), None);
    assert!(!map.has_engine("sldkjfkldsj"));

    assert_eq!(map.get_key("12ty.js").as_deref(), Some("12ty.js"));
    assert!(map.has_engine("12ty.js"));

    assert_eq!(map.get_key("md"), None);
    assert!(!map.has_engine("md"));
}

#[test]
fn has_engine_no_formats_passed_in() {
    let map = map_for(&[]);
    assert!(!map.has_engine("default.liquid"));
    assert_eq!(map.get_key("default.liquid"), None);
    assert_eq!(map.get_key(""), None);
    assert_eq!(map.get_key("LiQuid"), None);
    assert!(!map.has_engine("LiqUiD"));
    assert!(!map.has_engine("liquid"));
    assert_eq!(map.get_key("sldkjfkldsj"), None);
    assert!(!map.has_engine("sldkjfkldsj"));
    assert_eq!(map.get_key("12ty.js"), None);
    assert!(!map.has_engine("12ty.js"));
    assert_eq!(map.get_key("md"), None);
    assert!(!map.has_engine("md"));
}

#[test]
fn get_key() {
    let map = map_for(&["njk", "12ty.js", "md"]);
    assert_eq!(map.get_key("component.njk").as_deref(), Some("njk"));
    assert_eq!(map.get_key("component.12ty.js").as_deref(), Some("12ty.js"));
    assert_eq!(map.get_key("12ty.js").as_deref(), Some("12ty.js"));
    assert_eq!(map.get_key(".12ty.js").as_deref(), Some("12ty.js"));

    assert_eq!(map.get_key("sample.md").as_deref(), Some("md"));

    assert_eq!(map.get_key(""), None);
    assert_eq!(map.get_key("js"), None);
    assert_eq!(map.get_key("component"), None);
    assert_eq!(map.get_key("component.js"), None);
}

#[test]
fn is_full_template_file_path_not_a_passthrough_copy_extension() {
    let map = map_for(&["liquid", "njk", "12ty.js", "js", "css"]);
    assert!(map.is_full_template_file_path("template.liquid"));
    assert!(map.is_full_template_file_path("template.njk"));
    assert!(map.is_full_template_file_path("template.12ty.js"));
    assert!(!map.is_full_template_file_path("template.ejs"));
    assert!(!map.is_full_template_file_path("template.pug"));
    assert!(!map.is_full_template_file_path("passthrough.js"));
    assert!(!map.is_full_template_file_path("passthrough.css"));
}

#[test]
fn is_full_template_file_path_with_other_extensions_for_a_key() {
    let map = map_for(&["njk", "12ty.js"]);
    assert!(map.is_full_template_file_path("template.12ty.cjs"));
    assert!(map.is_full_template_file_path("template.12ty.mjs"));
    assert!(map.is_full_template_file_path("template.server.js"));
    assert!(map.is_full_template_file_path("template.server.mjs"));
    assert!(!map.is_full_template_file_path("template.js"));
    assert!(!map.is_full_template_file_path("template.mjs"));
}

#[test]
fn get_valid_extensions_for_path() {
    let cfg = config_with_js_extension();
    let map = get_extension_map(&["liquid", "njk", "12ty.js", "js"], &cfg);

    assert_eq!(map.get_valid_extensions_for_path("template.liquid"), strings(&["liquid"]));
    assert_eq!(map.get_valid_extensions_for_path("template.12ty.js"), strings(&["12ty.js", "js"]));
    assert_eq!(map.get_valid_extensions_for_path("template.pug"), Vec::<String>::new());
    assert_eq!(map.get_valid_extensions_for_path("template.liquid.js"), strings(&["js"]));
    assert_eq!(map.get_valid_extensions_for_path("njk.liquid.12ty.js"), strings(&["12ty.js", "js"]));
    assert_eq!(map.get_valid_extensions_for_path("template.server.js"), strings(&["server.js", "js"]));
    assert_eq!(map.get_valid_extensions_for_path("template.12ty.mjs"), strings(&["12ty.mjs"]));
}

// Skipped: `shouldSpiderJavaScriptDependencies` (JavaScript dependency spidering for
// `12ty.js` templates is a JS-only feature, not part of the Rust port).

#[test]
fn custom_add_extension_keys_are_mapped_to_lower_case_4224() {
    let mut cfg = UserConfig::new();
    cfg.add_extension(
        "vZome",
        ExtensionOptions {
            compile: Some(Arc::new(|content: &str, _: &str| {
                let content = content.to_string();
                Ok(Arc::new(move |_: &twelvety::Value| Ok(content.clone())) as _)
            })),
            ..Default::default()
        },
    )
    .unwrap();

    let map = get_extension_map(&["vZome"], &cfg);
    assert_eq!(map.get_valid_globs("."), strings(&["./**/*.vzome"]));
    assert_eq!(map.get_key("component.vZome").as_deref(), Some("vzome"));
    assert!(map.is_full_template_file_path("component.vZome"));
    assert_eq!(map.get_valid_extensions_for_path("component.vZome"), strings(&["vzome"]));
    assert_eq!(map.remove_template_extension("component.vZome"), "component");
}

#[test]
fn custom_add_extension_detects_mixed_case_duplicates_4224() {
    let mut cfg = UserConfig::new();
    cfg.add_extension("vZome", ExtensionOptions::default()).unwrap();
    let err = cfg.add_extension("VZOME", ExtensionOptions::default()).unwrap_err();
    assert!(err.to_string().contains("\"vzome\" template syntax twice"), "{err}");
}
