//! Port of `TemplateFileSlug.js`.

use std::sync::LazyLock;

use regex::Regex;

use crate::extension_map::ExtensionMap;
use crate::util::{node_path, template_path};

static DATE_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d{4}-\d{2}-\d{2}-(.*)").unwrap());

pub struct TemplateFileSlug {
    dirs: Vec<String>,
    filename_no_ext: String,
}

impl TemplateFileSlug {
    pub fn new(input_path: &str, extension_map: &ExtensionMap, input_dir: &str) -> Self {
        let input_path = if input_dir.is_empty() {
            input_path.to_string()
        } else {
            template_path::strip_leading_sub_path(input_path, input_dir)
        };
        let clean = template_path::strip_leading_dot_slash(&input_path);
        let mut dirs: Vec<String> = clean.split('/').map(String::from).collect();
        dirs.pop();
        let parsed = node_path::parse(&input_path);
        TemplateFileSlug {
            dirs,
            filename_no_ext: extension_map.remove_template_extension(&parsed.base),
        }
    }

    pub fn get_full_path_without_extension(&self) -> String {
        let mut parts: Vec<&str> = self.dirs.iter().map(|s| s.as_str()).collect();
        let raw = self.raw_slug();
        parts.push(&raw);
        format!("/{}", template_path::join(&parts))
    }

    fn raw_slug(&self) -> String {
        Self::strip_date_from_slug(&self.filename_no_ext)
    }

    fn strip_date_from_slug(slug: &str) -> String {
        match DATE_PREFIX.captures(slug) {
            Some(c) => c[1].to_string(),
            None => slug.to_string(),
        }
    }

    pub fn get_slug(&self) -> String {
        let raw = self.raw_slug();
        if raw == "index" {
            return match self.dirs.last() {
                Some(last) => Self::strip_date_from_slug(last),
                None => String::new(),
            };
        }
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slug(path: &str, input: &str) -> (String, String) {
        let mut m = ExtensionMap::new(&[]);
        m.set_formats(&["md".into(), "njk".into(), "html".into(), "liquid".into()]);
        let s = TemplateFileSlug::new(path, &m, input);
        (s.get_slug(), s.get_full_path_without_extension())
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("./test.md", "."), ("test".into(), "/test".into()));
        assert_eq!(
            slug("./src/2018-01-01-test.md", "./src"),
            ("test".into(), "/test".into())
        );
        assert_eq!(
            slug("./src/posts/index.njk", "./src"),
            ("posts".into(), "/posts/index".into())
        );
        assert_eq!(
            slug("./src/index.njk", "./src"),
            ("".into(), "/index".into())
        );
    }
}
