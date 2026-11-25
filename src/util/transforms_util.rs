//! Port of `Util/TransformsUtil.js`.

use anyhow::{Context, Result};
use indexmap::IndexMap;

use super::console_logger::ConsoleLogger;
use crate::user_config::{TransformContext, TransformFn};
use crate::value::Value;

pub fn run_all(
    content: String,
    page: &Value,
    transforms: &IndexMap<String, TransformFn>,
    base_href: Option<String>,
    logger: Option<&ConsoleLogger>,
) -> Result<String> {
    let input_path = page.get("inputPath").to_output_string();
    let output_path = page.get("outputPath");
    let ctx = TransformContext {
        input_path: input_path.clone(),
        output_path: output_path.clone(),
        url: page.get("url"),
        page: page.clone(),
        base_href,
    };
    let mut content = content;
    for (name, callback) in transforms {
        let had_content_before = !content.is_empty();
        content = callback(&ctx, content).with_context(|| {
            format!("Transform `{name}` encountered an error when transforming {input_path}.")
        })?;
        if had_content_before
            && content.is_empty()
            && let Some(logger) = logger
        {
            logger.warn(&format!(
                "Warning: Transform `{name}` returned empty when writing {} from {input_path}.",
                output_path.to_output_string()
            ));
        }
    }
    Ok(content)
}
