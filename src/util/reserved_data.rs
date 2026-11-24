//! Port of `Util/ReservedData.js`.

use anyhow::{Result, bail};

use crate::value::Value;

pub const FULL_PROPERTIES: &[&str] = &["pkg", "buildawesome", "eleventy", "content", "collections"];
pub const PROPERTIES: &[&str] = &["content", "collections"];
pub const PAGE_PROPERTIES: &[&str] = &[
    "date",
    "inputPath",
    "fileSlug",
    "filePathStem",
    "relativeInputPath",
    "outputFileExtension",
    "templateSyntax",
    "url",
    "outputPath",
];

pub fn get_reserved_keys(data: &Value, global_properties: &[&str]) -> Vec<String> {
    if !data.is_object() {
        return vec![];
    }
    let mut keys: Vec<String> = global_properties
        .iter()
        .filter(|k| data.has(k))
        .map(|k| k.to_string())
        .collect();
    if data.has("page") {
        let page = data.get("page");
        if page.is_object() {
            for key in PAGE_PROPERTIES {
                if page.has(key) {
                    keys.push(format!("page.{key}"));
                }
            }
        } else {
            keys.push("page".into());
        }
    }
    keys
}

pub fn error_message(reserved: &[String], source: Option<&str>) -> String {
    format!(
        "You attempted to set one of Build Awesome’s reserved data property names{}{}. You can opt-out of this behavior with `$config.setFreezeReservedData(false)` or rename/remove the property in your data cascade that conflicts with reserved property names (e.g. `buildawesome`, `eleventy`, `pkg`, and others). Learn more: https://v3.11ty.dev/docs/data-eleventy-supplied/",
        if reserved.is_empty() {
            String::new()
        } else {
            format!(": {}", reserved.join(", "))
        },
        source.map(|s| format!(" (source: {s})")).unwrap_or_default()
    )
}

fn check_with(data: &Value, source: Option<&str>, props: &[&str]) -> Result<()> {
    let reserved = get_reserved_keys(data, props);
    if reserved.is_empty() {
        return Ok(());
    }
    bail!(error_message(&reserved, source))
}

pub fn check(data: &Value, source: Option<&str>) -> Result<()> {
    check_with(data, source, FULL_PROPERTIES)
}

pub fn check_subset(data: &Value, source: Option<&str>) -> Result<()> {
    check_with(data, source, PROPERTIES)
}

pub fn check_global_overrides(data: &Value, global_keys: &[&str], source: Option<&str>) -> Result<()> {
    let reserved: Vec<String> = global_keys
        .iter()
        .filter(|k| data.get(k).is_plain_object() || data.get(&format!("override:{k}")).is_plain_object())
        .map(|k| k.to_string())
        .collect();
    if reserved.is_empty() {
        return Ok(());
    }
    bail!(error_message(&reserved, source))
}
