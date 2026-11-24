//! Port of the `gray-matter` front matter parsing used by `TemplateContent.js`.

use anyhow::{Result, bail};

use crate::value::Value;

#[derive(Clone, Debug, Default)]
pub struct FrontMatterOptions {
    pub excerpt: bool,
    pub excerpt_separator: Option<String>,
    pub excerpt_alias: Option<String>,
    pub language: Option<String>,
}

impl FrontMatterOptions {
    pub fn from_value(v: &Value) -> Self {
        FrontMatterOptions {
            excerpt: v.get("excerpt").is_truthy(),
            excerpt_separator: v.get("excerpt_separator").as_str().map(String::from),
            excerpt_alias: v.get("excerpt_alias").as_str().map(String::from),
            language: v.get("language").as_str().map(String::from),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FrontMatter {
    pub data: Value,
    pub content: String,
    pub excerpt: Option<String>,
    /// Number of lines before the body (for error line offsets)
    pub body_line_offset: usize,
}

fn parse_data(language: &str, matter: &str, file: &str) -> Result<Value> {
    Ok(match language {
        "yaml" | "yml" | "" => {
            let parsed: serde_yaml::Value = serde_yaml::from_str(matter)?;
            let v = Value::from_yaml(parsed);
            if v.is_nullish() { Value::object() } else { v }
        }
        "json" => Value::from_json(serde_json::from_str(matter)?),
        "toml" => Value::from_toml(toml::from_str(matter)?),
        "js" | "javascript" => {
            bail!("JavaScript front matter is not supported in the Rust port ({file}).")
        }
        "node" => bail!(
            "The `node` front matter type was a 3.0.0-alpha.x only feature, removed for stable release. Rename to `js` or `javascript` instead!"
        ),
        other => bail!("Unsupported front matter language: {other} ({file})"),
    })
}

/// `matter(content, options)`
pub fn parse(input: &str, options: &FrontMatterOptions, file: &str) -> Result<FrontMatter> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let open = "---";
    let close = "\n---";
    let mut fm = FrontMatter {
        data: Value::object(),
        content: input.to_string(),
        excerpt: None,
        body_line_offset: 0,
    };
    if !input.starts_with(open) || input[open.len()..].starts_with('-') {
        apply_excerpt(&mut fm, options);
        return Ok(fm);
    }
    let mut s = &input[open.len()..];
    let len = s.len();
    // language
    let first_line_end = s.find('\n').unwrap_or(s.len());
    let lang_raw = &s[..first_line_end];
    let mut language = options.language.clone().unwrap_or_else(|| "yaml".into());
    if !lang_raw.trim().is_empty() {
        language = lang_raw.trim().to_lowercase();
        s = &s[lang_raw.len()..];
    }
    let close_index = s.find(close).unwrap_or(s.len());
    let matter = &s[..close_index];
    let block: String = matter
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    if block.trim().is_empty() {
        fm.data = Value::object();
    } else {
        fm.data = parse_data(&language, matter, file)?;
    }
    if close_index == len || close_index == s.len() {
        fm.content = String::new();
    } else {
        let mut content = &s[close_index + close.len()..];
        // the rest of the closing delimiter line (gray-matter keeps it, but trims one newline)
        if content.starts_with('\r') {
            content = &content[1..];
        }
        if content.starts_with('\n') {
            content = &content[1..];
        }
        fm.content = content.to_string();
    }
    if input.ends_with(&fm.content) {
        fm.body_line_offset = input[..input.len() - fm.content.len()].matches('\n').count();
    }
    apply_excerpt(&mut fm, options);
    Ok(fm)
}

fn apply_excerpt(fm: &mut FrontMatter, options: &FrontMatterOptions) {
    if !options.excerpt {
        return;
    }
    let sep = options.excerpt_separator.clone().unwrap_or_else(|| "---".into());
    if let Some(idx) = fm.content.find(&sep) {
        fm.excerpt = Some(fm.content[..idx].to_string());
    }
}

/// Eleventy-specific excerpt handling from `TemplateContent#read`
pub fn apply_eleventy_excerpt(fm: &mut FrontMatter, options: &FrontMatterOptions) {
    if !options.excerpt {
        return;
    }
    let Some(excerpt) = fm.excerpt.clone() else {
        return;
    };
    if excerpt.is_empty() {
        return;
    }
    let sep = options.excerpt_separator.clone().unwrap_or_else(|| "---".into());
    let excerpt_string = format!("{excerpt}{sep}");
    if let Some(rest) = fm.content.strip_prefix(&format!("{excerpt_string}\n")) {
        fm.content = format!("{}\n{}", excerpt.trim(), rest);
    } else if let Some(rest) = fm.content.strip_prefix(&excerpt_string) {
        fm.content = format!("{excerpt}{rest}");
    }
    let alias = options
        .excerpt_alias
        .clone()
        .unwrap_or_else(|| "page.excerpt".into());
    fm.data.set_path(&alias, Value::str(excerpt));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_yaml() {
        let fm = parse("---\ntitle: Hi\ntags: [a, b]\n---\n# Body\n", &Default::default(), "x").unwrap();
        assert_eq!(fm.data.get("title"), Value::str("Hi"));
        assert_eq!(fm.content, "# Body\n");
        assert_eq!(fm.body_line_offset, 4);
    }

    #[test]
    fn no_front_matter() {
        let fm = parse("Hello", &Default::default(), "x").unwrap();
        assert_eq!(fm.content, "Hello");
    }

    #[test]
    fn json_front_matter() {
        let fm = parse("---json\n{\"a\": 1}\n---\nbody", &Default::default(), "x").unwrap();
        assert_eq!(fm.data.get("a"), Value::Number(1.0));
        assert_eq!(fm.content, "body");
    }

    #[test]
    fn empty_front_matter() {
        let fm = parse("---\n---\nbody", &Default::default(), "x").unwrap();
        assert!(fm.data.is_object());
        assert_eq!(fm.content, "body");
    }

    #[test]
    fn excerpt() {
        let opts = FrontMatterOptions {
            excerpt: true,
            ..Default::default()
        };
        let mut fm = parse("---\na: 1\n---\nThis is an excerpt.\n---\nRest of the post", &opts, "x").unwrap();
        apply_eleventy_excerpt(&mut fm, &opts);
        assert_eq!(fm.data.get("page").get("excerpt"), Value::str("This is an excerpt.\n"));
        assert_eq!(fm.content, "This is an excerpt.\nRest of the post");
    }
}
