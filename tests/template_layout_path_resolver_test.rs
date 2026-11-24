//! Port of upstream `test/TemplateLayoutPathResolverTest.js`.
//!
//! Instance aliases (`resolver.addLayoutAlias()` + `init()`) are configured with
//! `addLayoutAlias` on the configuration instead.

use std::sync::Arc;

use twelvety::UserConfig;
use twelvety::template_layout::TemplateLayoutPathResolver;
use twelvety::testing::{TestOptions, get_context};
use twelvety::user_config::ExtensionOptions;

fn resolver_with<F>(path: &str, input: &str, config: F) -> TemplateLayoutPathResolver
where
    F: Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
{
    let ctx = get_context(TestOptions::input(input).with_config(config)).unwrap();
    TemplateLayoutPathResolver::new(path, &ctx)
}

fn file_name(path: &str) -> String {
    resolver_with(path, "./test/stubs", |_| Ok(())).get_file_name().unwrap()
}

fn liquid_with<F>(path: &str, dirs: F) -> String
where
    F: Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
{
    resolver_with(path, "test/stubs", move |cfg| {
        cfg.set_template_formats(&["liquid"]);
        dirs(cfg)
    })
    .get_file_name()
    .unwrap()
}

#[test]
fn layout() {
    assert_eq!(file_name("default"), "default.liquid");
}

#[test]
fn layout_already_has_extension() {
    assert_eq!(file_name("default.liquid"), "default.liquid");
}

#[test]
fn layout_uses_empty_string_includes_folder() {
    for path in ["includesemptystring", "includesemptystring.liquid"] {
        assert_eq!(liquid_with(path, |cfg| cfg.set_includes_directory("")), "includesemptystring.liquid");
    }
}

#[test]
fn layout_uses_layouts_folder() {
    let name = liquid_with("layoutsdefault", |cfg| {
        cfg.set_layouts_directory("_layouts")?;
        cfg.set_includes_directory("_includes")
    });
    assert_eq!(name, "layoutsdefault.liquid");
    let name = liquid_with("layoutsdefault.liquid", |cfg| cfg.set_layouts_directory("_layouts"));
    assert_eq!(name, "layoutsdefault.liquid");
}

#[test]
fn layout_uses_empty_string_layouts_folder() {
    for path in ["layoutsemptystring", "layoutsemptystring.liquid"] {
        assert_eq!(liquid_with(path, |cfg| cfg.set_layouts_directory("")), "layoutsemptystring.liquid");
    }
}

#[test]
fn layout_uses_empty_string_layouts_folder_no_template_resolution() {
    let res = resolver_with("layoutsemptystring", "test/stubs", |cfg| {
        cfg.set_layouts_directory("")?;
        cfg.set_layout_resolution(false);
        Ok(())
    });
    assert_eq!(
        res.get_file_name().unwrap_err().to_string(),
        "You’re trying to use a layout that does not exist: test/stubs/layoutsemptystring (via `layout: layoutsemptystring`)"
    );
}

#[test]
fn layout_subdir() {
    assert_eq!(file_name("layouts/inasubdir"), "layouts/inasubdir.njk");
    assert_eq!(file_name("layouts/inasubdir.njk"), "layouts/inasubdir.njk");
}

#[test]
fn multiple_layouts_exist_with_the_same_file_base_pick_one() {
    // pick the first one if multiple exist.
    assert_eq!(file_name("multiple"), "multiple.liquid");
}

#[test]
fn multiple_layouts_exist_but_explicit_extension() {
    assert_eq!(file_name("multiple.liquid"), "multiple.liquid");
    assert_eq!(file_name("multiple.md"), "multiple.md");
}

fn aliased(path: &str, from: &'static str) -> TemplateLayoutPathResolver {
    resolver_with(path, "./test/stubs", move |cfg| {
        cfg.add_layout_alias(from, "layouts/post.liquid");
        Ok(())
    })
}

#[test]
fn layout_is_aliased_to_a_new_location() {
    assert_eq!(aliased("post", "post").get_file_name().unwrap(), "layouts/post.liquid");
}

#[test]
fn global_default_with_empty_string_alias() {
    assert_eq!(aliased("", "").get_file_name().unwrap(), "layouts/post.liquid");
}

#[test]
fn global_default_with_empty_string_alias_but_no_alias_exists_for_this_instance() {
    assert!(aliased("layout.liquid", "").get_file_name().is_err());
}

#[test]
fn layout_has_no_alias_and_does_not_exist() {
    let res = aliased("shouldnotexist", "post");
    assert!(res.get_file_name().is_err());
    assert!(res.get_full_path().is_err());
}

#[test]
fn layout_with_mixed_case_extensions_and_folders_4224() {
    let ctx = get_context(TestOptions::input("test/_issues/4224").with_config(|cfg| {
        cfg.add_template_formats(&["vZome"]);
        cfg.add_extension(
            "vZome",
            ExtensionOptions {
                compile: Some(Arc::new(|_, _| Ok(Arc::new(|_: &twelvety::Value| Ok(String::new()))))),
                ..Default::default()
            },
        )
    }))
    .unwrap();
    let name = |p: &str| TemplateLayoutPathResolver::new(p, &ctx).get_file_name().unwrap();
    assert_eq!(name("base"), "base.vZome");
    assert_eq!(name("Base.VZOME"), "base.vZome");
    assert_eq!(name("Partials/Inner"), "partials/inner.vZome");
}
