//! Port of `TemplatePipeline.js`: output-format chains (Markdown/HTML) and their
//! template-engine preprocessors.

use anyhow::{Result, bail};

use crate::template_config::Config;

const FORMAT_CHAINS: &[&[&str]] = &[&["md"], &["html"]];
pub const PLAINTEXT_FORMAT: &str = "html";

pub fn is_format(name: &str) -> bool {
    FORMAT_CHAINS.iter().any(|chain| chain.contains(&name))
}

pub fn get_chain(format: &str) -> Option<&'static [&'static str]> {
    FORMAT_CHAINS
        .iter()
        .find(|chain| chain.last() == Some(&format))
        .copied()
}

/// The template engine that preprocesses a format (e.g. `markdownTemplateEngine`)
pub fn get_preprocessor(chain: &[&str], config: &Config) -> Result<Option<String>> {
    let format = chain[0];
    let (key, name) = match format {
        "md" => (
            "markdownTemplateEngine",
            config.markdown_template_engine.clone(),
        ),
        "html" => ("htmlTemplateEngine", config.html_template_engine.clone()),
        _ => return Ok(None),
    };
    let Some(name) = name else {
        return Ok(None);
    };
    if name.is_empty() || name == "false" || name == format || name == PLAINTEXT_FORMAT {
        return Ok(None);
    }
    if is_format(&name) {
        bail!(
            "The `{key}` configuration property must be a template engine, received the output format: {name}"
        );
    }
    Ok(Some(name))
}

/// Parse a `templateEngineOverride` value into pipeline steps
pub fn parse(syntax: &str) -> Result<Vec<String>> {
    let mut names: Vec<String> = vec![];
    for name in syntax.split(',') {
        let n = name.trim().to_lowercase();
        if !n.is_empty() && n != PLAINTEXT_FORMAT && !names.contains(&n) {
            names.push(n);
        }
    }
    let templates: Vec<String> = names.iter().filter(|n| !is_format(n)).cloned().collect();
    let formats: Vec<String> = names.iter().filter(|n| is_format(n)).cloned().collect();
    if templates.len() > 1 {
        bail!(
            "Don’t mix multiple templating engines in your front matter overrides (exceptions for HTML and Markdown). You used: {syntax}"
        );
    }
    if formats.is_empty() {
        return Ok(templates);
    }
    let Some(chain) = FORMAT_CHAINS.iter().find(|chain| {
        chain.len() == formats.len() && formats.iter().all(|f| chain.contains(&f.as_str()))
    }) else {
        bail!("Unsupported output format combination in template syntax: {syntax}");
    };
    let mut out = templates;
    out.extend(chain.iter().map(|s| s.to_string()));
    Ok(out)
}

pub fn validate(steps: &[String]) -> Result<()> {
    let first_format = steps.iter().position(|s| is_format(s));
    let (templates, formats) = match first_format {
        None => (steps, &[][..]),
        Some(i) => (&steps[..i], &steps[i..]),
    };
    if templates.len() > 1 {
        bail!(
            "Only one template engine may run in a template pipeline, received: {}",
            steps.join(",")
        );
    }
    if !formats.is_empty()
        && !FORMAT_CHAINS.iter().any(|chain| {
            chain.len() == formats.len() && chain.iter().zip(formats).all(|(a, b)| a == b)
        })
    {
        bail!("Unsupported template pipeline: {}", steps.join(","));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(parse("njk,md").unwrap(), vec!["njk", "md"]);
        assert_eq!(parse("md").unwrap(), vec!["md"]);
        assert!(parse("html").unwrap().is_empty());
        assert!(parse("njk,liquid").is_err());
    }
}
