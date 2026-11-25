//! Shared helpers for the `Template*` test ports (mirrors the helpers at the top of
//! upstream `test/TemplateTest.js` and `test/_getRenderedTemplates.js`).
#![allow(dead_code)]

use std::sync::Arc;

use anyhow::Result;
use twelvety::page_entry::PageEntry;
use twelvety::template::Template;
use twelvety::testing::{TestOptions, get_new_template};
use twelvety::{UserConfig, Value};

pub type Templates = Vec<(String, String, Value)>;

/// Virtual templates (keys are relative to the input directory)
pub fn templates(list: &[(&str, &str)]) -> Templates {
    list.iter()
        .map(|(p, c)| (p.to_string(), c.to_string(), Value::object()))
        .collect()
}

/// `getVirtualConfig(dirs, templates, configCallback)`
pub fn virtual_options<F>(
    input: &str,
    output: &str,
    templates: Templates,
    callback: F,
) -> TestOptions
where
    F: Fn(&mut UserConfig) -> Result<()> + Send + Sync + 'static,
{
    TestOptions::new(input, output).with_config(move |cfg| {
        callback(cfg)?;
        for (path, content, data) in &templates {
            cfg.add_template(path, content, data.clone())?;
        }
        Ok(())
    })
}

/// `getVirtualTemplate(path, inputDir, outputDir, templates)`
pub fn virtual_template(
    path: &str,
    input: &str,
    output: &str,
    list: &[(&str, &str)],
) -> Arc<Template> {
    get_new_template(
        path,
        virtual_options(input, output, templates(list), |_| Ok(())),
    )
    .unwrap()
}

/// `getRenderedData(tmpl, pageNumber)`
pub fn rendered_data(tmpl: &Arc<Template>, page_number: usize) -> Value {
    let mut data = tmpl.get_data().unwrap();
    let pages = tmpl.get_templates(&mut data).unwrap();
    pages[page_number].data()
}

/// `getRenderedTemplates(tmpl, data)`: page entries with `templateContent` set
/// (layouts are applied to `templateContent`, as in the upstream test helper)
pub fn rendered_templates(tmpl: &Arc<Template>) -> Result<Vec<Arc<PageEntry>>> {
    let mut data = tmpl.get_data()?;
    let pages = tmpl.get_templates(&mut data)?;
    for page in &pages {
        let content = render_entry(tmpl, page)?;
        page.set_template_content(content);
    }
    Ok(pages)
}

/// `renderTemplate(tmpl, data)` for a page entry: content with layouts (no transforms)
fn render_entry(tmpl: &Arc<Template>, page: &Arc<PageEntry>) -> Result<String> {
    let content = tmpl.render_page_entry_without_layout(page)?;
    let layout_key = page.data().get("layout");
    if layout_key.is_truthy() && tmpl.templates_uses_layouts(&page.data()) {
        let layout = tmpl.get_layout(&layout_key.to_output_string())?;
        return layout.render_layout_page_entry(page, content);
    }
    Ok(content)
}

/// `renderTemplate(tmpl, await tmpl.getData())` (first page)
pub fn render(tmpl: &Arc<Template>) -> Result<String> {
    let mut data = tmpl.get_data()?;
    let pages = tmpl.get_templates(&mut data)?;
    render_entry(tmpl, &pages[0])
}

/// `_testCompleteRender(tmpl)`: layouts, linters and transforms for every page
pub fn complete_render(tmpl: &Arc<Template>) -> Result<Vec<String>> {
    let mut data = tmpl.get_data()?;
    let pages = tmpl.get_templates(&mut data)?;
    pages
        .iter()
        .map(|page| {
            let content = tmpl.render_page_entry_without_layout(page)?;
            page.set_template_content(content);
            tmpl.render_page_entry(page)
        })
        .collect()
}

pub fn output_path(tmpl: &Template, data: &Value) -> Value {
    tmpl.get_output_locations(data, "").unwrap().path
}

pub fn output_href(tmpl: &Template, data: &Value) -> Value {
    tmpl.get_output_locations(data, "").unwrap().href
}

pub fn raw_output_path(tmpl: &Template, data: &Value) -> Option<String> {
    tmpl.get_output_locations(data, "").unwrap().raw_path
}

/// Collapse whitespace between tags (stands in for `pretty()` + `normalizeNewLines`)
pub fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn strs(v: &Value) -> Vec<String> {
    v.to_vec().iter().map(|v| v.to_js_string()).collect()
}

pub fn sorted_keys(v: &Value) -> Vec<String> {
    let mut k = v.keys();
    k.sort();
    k
}

/// Copy a fixture directory into a fresh scratch directory (relative to the crate root,
/// like upstream fixture paths) and add `files` (e.g. JSON stand-ins for JavaScript data
/// files, which the port does not support). Returns the new directory path (`./…`).
pub fn fixture_copy(src: &str, name: &str, files: &[(&str, &str)]) -> String {
    let dest = format!("{}/{name}", env!("CARGO_TARGET_TMPDIR"));
    let dest = pathdiff(&dest);
    let _ = std::fs::remove_dir_all(&dest);
    copy_dir(std::path::Path::new(src), std::path::Path::new(&dest));
    for (path, content) in files {
        let p = std::path::Path::new(&dest).join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    dest
}

/// `CARGO_TARGET_TMPDIR` relative to the crate root (the test working directory)
fn pathdiff(abs: &str) -> String {
    let cwd = std::env::current_dir().unwrap();
    let cwd: Vec<_> = cwd.components().collect();
    let target: Vec<_> = std::path::Path::new(abs).components().collect();
    let common = cwd.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".into(); cwd.len() - common];
    parts.extend(
        target[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    let rel = parts.join("/");
    if rel.starts_with("..") {
        rel
    } else {
        format!("./{rel}")
    }
}

fn copy_dir(src: &std::path::Path, dest: &std::path::Path) {
    std::fs::create_dir_all(dest).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dest.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to);
        } else if !matches!(
            entry.path().extension().and_then(|e| e.to_str()),
            Some("js" | "cjs" | "mjs")
        ) {
            std::fs::copy(entry.path(), to).unwrap();
        }
    }
}
