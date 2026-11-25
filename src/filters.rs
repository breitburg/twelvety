//! Port of `Filters/*.js`.

use crate::util::template_path;
use crate::value::Value;

fn is_valid_url(url: &str) -> bool {
    // `new URL(url)` succeeds for absolute URLs with a scheme
    let Some(colon) = url.find(':') else {
        return false;
    };
    let scheme = &url[..colon];
    !scheme.is_empty()
        && scheme
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false)
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        && colon < url.len()
}

/// `Filters/Url.js`
pub fn url_filter(url: &str, path_prefix: &str) -> String {
    if is_valid_url(url) || (url.starts_with("//") && url != "//") {
        return url.to_string();
    }
    let norm_url = template_path::normalize_url_path(&[url]);
    let norm_root_dir = template_path::normalize_url_path(&["/", path_prefix]);
    let norm_full = template_path::normalize_url_path(&["/", path_prefix, url]);
    let is_root_dir_trailing_slash = norm_root_dir.ends_with('/');
    if norm_url == "/" || norm_url == norm_root_dir {
        return format!(
            "{}{}",
            norm_root_dir,
            if !is_root_dir_trailing_slash { "/" } else { "" }
        );
    } else if norm_url.starts_with('/') {
        return norm_full;
    }
    norm_url
}

/// `findCollectionItemIndex`
pub fn find_collection_item_index(collection: &Value, page: &Value) -> Option<usize> {
    let items = collection.to_vec();
    let input_path = page.get("inputPath");
    let output_path = page.get("outputPath");
    let url = page.get("url");
    items.iter().position(|item| {
        item.get("inputPath").strict_equals(&input_path)
            && (item.get("outputPath").strict_equals(&output_path)
                || item.get("url").strict_equals(&url))
    })
}

/// `getCollectionItem(collection, page, modifier)`
pub fn get_collection_item(collection: &Value, page: &Value, modifier: i64) -> Value {
    let items = collection.to_vec();
    if let Some(index) = find_collection_item_index(collection, page) {
        let target = index as i64 + modifier;
        if target >= 0 && (target as usize) < items.len() {
            return items[target as usize].clone();
        }
    }
    Value::Undefined
}

/// `Filters/GetParentDirectory.js`
pub fn get_parent_directory(input: &Value) -> Value {
    let Some(s) = input.as_str() else {
        return Value::Undefined;
    };
    if let Some(rest) = s
        .strip_prefix("http://")
        .or_else(|| s.strip_prefix("https://"))
    {
        let scheme = if s.starts_with("https") {
            "https://"
        } else {
            "http://"
        };
        let (host, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let path = path.split(['?', '#']).next().unwrap_or("/");
        let parent = if let Some(trimmed) = path.strip_suffix('/') {
            match trimmed.rfind('/') {
                Some(i) => &trimmed[..=i],
                None => "/",
            }
        } else {
            match path.rfind('/') {
                Some(i) => &path[..=i],
                None => "/",
            }
        };
        return Value::str(format!("{scheme}{host}{parent}"));
    }
    let path = if s.len() > 1 && s.ends_with('/') {
        &s[..s.len() - 1]
    } else {
        s
    };
    match path.rfind('/') {
        Some(i) => Value::str(&path[..=i]),
        None => Value::Undefined,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url() {
        assert_eq!(url_filter("/", "/"), "/");
        assert_eq!(url_filter("/test/", "/"), "/test/");
        assert_eq!(url_filter("/test/", "/rootdir/"), "/rootdir/test/");
        assert_eq!(url_filter("/", "/rootdir/"), "/rootdir/");
        assert_eq!(url_filter("test/", "/rootdir/"), "test/");
        assert_eq!(
            url_filter("https://example.com/", "/rootdir/"),
            "https://example.com/"
        );
        assert_eq!(url_filter("//example.com/", "/rootdir/"), "//example.com/");
        assert_eq!(url_filter("", "/rootdir/"), ".");
    }

    #[test]
    fn parent_dir() {
        assert_eq!(
            get_parent_directory(&Value::str("/a/b/")),
            Value::str("/a/")
        );
        assert_eq!(
            get_parent_directory(&Value::str("/a/b.html")),
            Value::str("/a/")
        );
        assert_eq!(
            get_parent_directory(&Value::str("https://example.com/a/b/")),
            Value::str("https://example.com/a/")
        );
    }
}
