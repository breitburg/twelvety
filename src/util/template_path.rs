//! Port of `TemplatePath` from `@11ty/eleventy-utils`.

use super::node_path;
use std::path::Path;

pub fn get_working_dir() -> String {
    normalize(&node_path::resolve(&["."]))
}

pub fn get_dir(path: &str) -> String {
    if is_directory_sync(path) {
        return path.to_string();
    }
    get_dir_from_file_path(path)
}

pub fn get_dir_from_file_path(file_path: &str) -> String {
    let dir = node_path::parse(file_path).dir;
    if dir.is_empty() { ".".into() } else { dir }
}

pub fn get_last_path_segment(path: &str) -> String {
    if !path.contains('/') {
        return path.to_string();
    }
    let path = path.strip_suffix('/').unwrap_or(path);
    match path.rfind('/') {
        Some(i) => path[i + 1..].to_string(),
        None => path.to_string(),
    }
}

pub fn get_all_dirs(path: &str) -> Vec<String> {
    let path = path.strip_suffix('/').unwrap_or(path);
    if !path.contains('/') {
        return vec![path.to_string()];
    }
    let segments: Vec<&str> = path.split('/').collect();
    let mut out: Vec<String> = (0..segments.len())
        .map(|i| segments[..=i].join("/"))
        .filter(|p| p != ".")
        .collect();
    out.reverse();
    out
}

pub fn normalize(the_path: &str) -> String {
    let file_path = node_path::normalize(the_path);
    if file_path != "/" && file_path.ends_with('/') {
        return file_path[..file_path.len() - 1].to_string();
    }
    file_path
}

pub fn join(paths: &[&str]) -> String {
    normalize(&node_path::join(paths))
}

pub fn normalize_url_path(url_paths: &[&str]) -> String {
    let url_path = node_path::join(url_paths);
    let trimmed = url_path.trim_end_matches('/');
    if trimmed.len() != url_path.len() {
        format!("{trimmed}/")
    } else {
        url_path
    }
}

pub fn absolute_path(paths: &[&str]) -> String {
    if let Some(first) = paths.first() {
        if node_path::is_absolute(first) {
            return join(paths);
        }
    }
    let cwd = get_working_dir();
    let mut all = vec![cwd.as_str()];
    all.extend_from_slice(paths);
    join(&all)
}

pub fn relative_path(absolute_path: &str) -> String {
    strip_leading_sub_path(absolute_path, &get_working_dir())
}

pub fn add_leading_dot_slash(path: &str) -> String {
    if path == "." || path == ".." {
        return format!("{path}/");
    }
    if node_path::is_absolute(path) || path.starts_with("./") || path.starts_with("../") {
        return path.to_string();
    }
    format!("./{path}")
}

pub fn add_leading_dot_slash_array(paths: &[String]) -> Vec<String> {
    paths.iter().map(|p| add_leading_dot_slash(p)).collect()
}

pub fn strip_leading_dot_slash(path: &str) -> String {
    path.strip_prefix("./").unwrap_or(path).to_string()
}

fn get_sub_path_remainder(file_path: &str, sub_path: &str) -> Option<String> {
    if file_path == sub_path {
        return Some(String::new());
    }
    if sub_path == "." {
        let is_outside =
            file_path == ".." || file_path.starts_with("../") || node_path::is_absolute(file_path);
        return if is_outside { None } else { Some(file_path.to_string()) };
    }
    let prefix = if sub_path.ends_with('/') {
        sub_path.to_string()
    } else {
        format!("{sub_path}/")
    };
    file_path.strip_prefix(&prefix).map(|s| s.to_string())
}

pub fn starts_with_sub_path(path: &str, sub_path: &str) -> bool {
    get_sub_path_remainder(&normalize(path), &normalize(sub_path)).is_some()
}

pub fn strip_leading_sub_path(path: &str, sub_path: &str) -> String {
    let path = normalize(path);
    let sub_path = normalize(sub_path);
    if sub_path == "." {
        return path;
    }
    get_sub_path_remainder(&path, &sub_path).unwrap_or(path)
}

pub fn is_directory_sync(path: &str) -> bool {
    Path::new(path).is_dir()
}

pub fn exists(path: &str) -> bool {
    Path::new(path).exists()
}

pub fn convert_to_recursive_glob_sync(path: &str) -> String {
    if path.is_empty() {
        return "./**".into();
    }
    let path = add_leading_dot_slash(path);
    if is_directory_sync(&path) {
        return format!("{}{}**", path, if path.ends_with('/') { "" } else { "/" });
    }
    path
}

pub fn get_extension(the_path: &str) -> String {
    let ext = node_path::extname(the_path);
    ext.strip_prefix('.').unwrap_or(&ext).to_string()
}

pub fn remove_extension(path: &str, extension: Option<&str>) -> String {
    let Some(extension) = extension else {
        return path.to_string();
    };
    let path_extension = get_extension(path);
    if !path_extension.is_empty() && extension.ends_with(&path_extension) {
        if let Some(i) = path.rfind(&path_extension) {
            return path[..i.saturating_sub(1)].to_string();
        }
    }
    path.to_string()
}

pub fn standardize_file_path(file_path: &str) -> String {
    add_leading_dot_slash(&file_path.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_leading_sub_path() {
        assert_eq!(strip_leading_sub_path("./src/test.md", "./src/"), "test.md");
        assert_eq!(strip_leading_sub_path("./test.md", "."), "test.md");
        assert_eq!(strip_leading_sub_path("./other/test.md", "./src"), "other/test.md");
    }

    #[test]
    fn test_add_leading_dot_slash() {
        assert_eq!(add_leading_dot_slash("a"), "./a");
        assert_eq!(add_leading_dot_slash("."), "./");
        assert_eq!(add_leading_dot_slash("/a"), "/a");
    }

    #[test]
    fn test_get_all_dirs() {
        assert_eq!(get_all_dirs("./a/b/c"), vec!["./a/b/c", "./a/b", "./a"]);
    }

    #[test]
    fn test_last_segment() {
        assert_eq!(get_last_path_segment("./src/posts/"), "posts");
    }
}
