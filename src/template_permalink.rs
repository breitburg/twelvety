//! Port of `TemplatePermalink.js`.

use anyhow::{Result, bail};

use crate::user_config::UrlTransformFn;
use crate::util::{node_path, template_path};
use crate::value::Value;

#[derive(Clone)]
pub struct TemplatePermalink {
    build_link: Option<String>,
    write_to_file_system: bool,
    extra_pagination_subdir: String,
    url_transforms: Vec<UrlTransformFn>,
}

impl TemplatePermalink {
    /// `link` is a string permalink, or `false`
    pub fn new(link: &Value, extra_subdir: &str) -> Result<Self> {
        let link = Self::resolve_build_alias(link);
        let mut p = TemplatePermalink {
            build_link: None,
            write_to_file_system: true,
            extra_pagination_subdir: extra_subdir.to_string(),
            url_transforms: vec![],
        };
        match &link {
            Value::Bool(false) => p.write_to_file_system = false,
            Value::Bool(true) => {
                bail!("`permalink: true` is not a supported feature. Did you mean `permalink: false`?")
            }
            Value::String(s) => {
                if !s.is_empty() {
                    p.build_link = Some(s.to_string());
                }
            }
            v if !v.is_truthy() => {}
            other => bail!(
                "Expected permalink value to be a string. Received `{}`",
                other.type_of()
            ),
        }
        Ok(p)
    }

    pub fn resolve_build_alias(link: &Value) -> Value {
        if link.is_plain_object() && link.has("build") {
            return link.get("build");
        }
        link.clone()
    }

    pub fn set_url_transforms(&mut self, transforms: &[UrlTransformFn]) {
        self.url_transforms = transforms.to_vec();
    }

    pub fn writes_to_file_system(&self) -> bool {
        self.write_to_file_system
    }

    fn add_default_link_filename(link: &str) -> String {
        if link.ends_with('/') {
            format!("{link}index.html")
        } else {
            link.to_string()
        }
    }

    /// Relative output path (without the output directory), or `None` for `false`
    pub fn to_output_path(&self) -> Option<String> {
        let link = self.build_link.as_ref()?;
        let clean = Self::add_default_link_filename(link);
        let parsed = node_path::parse(&clean);
        Some(template_path::join(&[
            &parsed.dir,
            &self.extra_pagination_subdir,
            &parsed.base,
        ]))
    }

    pub fn get_url_stem(original: &str) -> String {
        let subject = original.strip_suffix(".html").unwrap_or(original);
        Self::normalize_path_to_url(subject)
    }

    pub fn normalize_path_to_url(original: &str) -> String {
        for (needle, keep) in [("/index.html", true), ("/index/", true), ("/index", true)] {
            if let Some(stripped) = original.strip_suffix(needle) {
                let _ = keep;
                return format!("{stripped}/");
            }
        }
        original.to_string()
    }

    pub fn to_href(&self) -> Option<String> {
        let transformed = self.to_output_path()?;
        let mut original = format!(
            "{}{}",
            if transformed.starts_with('/') { "" } else { "/" },
            transformed
        );
        let normalized = Self::normalize_path_to_url(&original);
        for transform in &self.url_transforms {
            if let Some(t) = transform(&normalized, &Self::get_url_stem(&original)) {
                original = t;
            }
        }
        Some(Self::normalize_path_to_url(&original))
    }

    pub fn to_path(&self, output_dir: &str) -> Option<String> {
        let uri = self.to_output_path()?;
        Some(template_path::add_leading_dot_slash(&template_path::normalize(
            &format!("{output_dir}/{uri}"),
        )))
    }

    pub fn to_path_from_root(&self) -> Option<String> {
        let uri = self.to_output_path()?;
        Some(template_path::add_leading_dot_slash(&template_path::normalize(&uri)))
    }

    /// `_hasDuplicateFolder`
    pub fn has_duplicate_folder(dir: &str, base: &str) -> bool {
        let mut folders: Vec<&str> = dir.split('/').collect();
        if folders.last() == Some(&"") {
            folders.pop();
        }
        folders.last() == Some(&base)
    }

    pub fn generate(
        dir: &str,
        filename_no_ext: &str,
        extra_subdir: &str,
        file_extension: &str,
        collapse_matching_folder: bool,
    ) -> Result<Self> {
        let path = if file_extension == "html" {
            let has_dupe = collapse_matching_folder && Self::has_duplicate_folder(dir, filename_no_ext);
            format!(
                "{}{}index.html",
                if dir.is_empty() { String::new() } else { format!("{dir}/") },
                if filename_no_ext != "index" && !has_dupe {
                    format!("{filename_no_ext}/")
                } else {
                    String::new()
                }
            )
        } else {
            format!(
                "{}{filename_no_ext}.{file_extension}",
                if dir.is_empty() { String::new() } else { format!("{dir}/") }
            )
        };
        Self::new(&Value::str(path), extra_subdir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn href(link: &str, subdir: &str) -> String {
        TemplatePermalink::new(&Value::str(link), subdir).unwrap().to_href().unwrap()
    }

    #[test]
    fn hrefs() {
        assert_eq!(href("permalinksubfolder/test.html", ""), "/permalinksubfolder/test.html");
        assert_eq!(href("./permalinksubfolder/test.html", ""), "/permalinksubfolder/test.html");
        assert_eq!(href("permalinksubfolder/index.html", ""), "/permalinksubfolder/");
        assert_eq!(href("permalinksubfolder/", ""), "/permalinksubfolder/");
        assert_eq!(href("permalinksubfolder/", "1"), "/permalinksubfolder/1/");
        assert_eq!(href("/", ""), "/");
    }

    #[test]
    fn paths() {
        let p = TemplatePermalink::new(&Value::str("permalinksubfolder/"), "").unwrap();
        assert_eq!(p.to_path("_site").unwrap(), "./_site/permalinksubfolder/index.html");
        let g = TemplatePermalink::generate("", "test", "", "html", false).unwrap();
        assert_eq!(g.to_href().unwrap(), "/test/");
        let g = TemplatePermalink::generate("posts", "index", "", "html", false).unwrap();
        assert_eq!(g.to_href().unwrap(), "/posts/");
        let g = TemplatePermalink::generate("", "style", "", "css", false).unwrap();
        assert_eq!(g.to_href().unwrap(), "/style.css");
    }
}
