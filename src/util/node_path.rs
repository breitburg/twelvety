//! A small port of the Node.js `path` (posix) module semantics.

use std::path::Path;

pub fn is_absolute(p: &str) -> bool {
    p.starts_with('/') || Path::new(p).is_absolute()
}

/// `path.posix.normalize`
pub fn normalize(p: &str) -> String {
    if p.is_empty() {
        return ".".into();
    }
    let p = p.replace('\\', "/");
    let is_abs = p.starts_with('/');
    let trailing = p.ends_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if let Some(last) = out.last()
                    && *last != ".."
                {
                    out.pop();
                    continue;
                }
                if !is_abs {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    let mut result = out.join("/");
    if is_abs {
        result = format!("/{result}");
    }
    if result.is_empty() {
        result = if is_abs { "/".into() } else { ".".into() };
    }
    if trailing && !result.ends_with('/') {
        result.push('/');
    }
    result
}

/// `path.posix.join`
pub fn join(parts: &[&str]) -> String {
    let joined: Vec<&str> = parts.iter().copied().filter(|s| !s.is_empty()).collect();
    if joined.is_empty() {
        return ".".into();
    }
    normalize(&joined.join("/"))
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedPath {
    pub root: String,
    pub dir: String,
    pub base: String,
    pub ext: String,
    pub name: String,
}

/// `path.posix.parse`
pub fn parse(p: &str) -> ParsedPath {
    let mut parsed = ParsedPath::default();
    if p.is_empty() {
        return parsed;
    }
    let is_abs = p.starts_with('/');
    if is_abs {
        parsed.root = "/".into();
    }
    let trimmed = if p.len() > 1 {
        p.trim_end_matches('/')
    } else {
        p
    };
    let (dir, base) = match trimmed.rfind('/') {
        Some(0) => ("/".to_string(), trimmed[1..].to_string()),
        Some(i) => (trimmed[..i].to_string(), trimmed[i + 1..].to_string()),
        None => (String::new(), trimmed.to_string()),
    };
    parsed.dir = dir;
    parsed.ext = extname(&base);
    parsed.name = base[..base.len() - parsed.ext.len()].to_string();
    parsed.base = base;
    parsed
}

/// `path.posix.extname`
pub fn extname(p: &str) -> String {
    let base = match p.trim_end_matches('/').rfind('/') {
        Some(i) => &p[i + 1..],
        None => p,
    };
    match base.rfind('.') {
        Some(0) | None => String::new(),
        Some(i) => base[i..].to_string(),
    }
}

pub fn basename(p: &str) -> String {
    parse(p).base
}

pub fn dirname(p: &str) -> String {
    let d = parse(p).dir;
    if d.is_empty() { ".".into() } else { d }
}

pub fn cwd() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| ".".into())
}

/// `path.resolve`
pub fn resolve(parts: &[&str]) -> String {
    let mut resolved = String::new();
    for part in parts.iter().rev() {
        if part.is_empty() {
            continue;
        }
        resolved = if resolved.is_empty() {
            part.to_string()
        } else {
            format!("{part}/{resolved}")
        };
        if is_absolute(part) {
            break;
        }
    }
    if !is_absolute(&resolved) {
        resolved = if resolved.is_empty() {
            cwd()
        } else {
            format!("{}/{}", cwd(), resolved)
        };
    }
    let n = normalize(&resolved);
    if n.len() > 1 {
        n.trim_end_matches('/').to_string()
    } else {
        n
    }
}

/// `path.relative(from, to)`
pub fn relative(from: &str, to: &str) -> String {
    let from = resolve(&[from]);
    let to = resolve(&[to]);
    if from == to {
        return String::new();
    }
    let from_parts: Vec<&str> = from.split('/').filter(|s| !s.is_empty()).collect();
    let to_parts: Vec<&str> = to.split('/').filter(|s| !s.is_empty()).collect();
    let mut common = 0;
    while common < from_parts.len()
        && common < to_parts.len()
        && from_parts[common] == to_parts[common]
    {
        common += 1;
    }
    let mut out = vec![".."; from_parts.len() - common];
    out.extend_from_slice(&to_parts[common..]);
    out.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize() {
        assert_eq!(normalize("./a/b/../c"), "a/c");
        assert_eq!(normalize("a//b/"), "a/b/");
        assert_eq!(normalize("../a"), "../a");
        assert_eq!(normalize("/a/../.."), "/");
        assert_eq!(normalize("./"), "./");
    }

    #[test]
    fn test_parse() {
        let p = parse("./src/index.md");
        assert_eq!(p.dir, "./src");
        assert_eq!(p.base, "index.md");
        assert_eq!(p.name, "index");
        assert_eq!(p.ext, ".md");
        let p = parse("index.12ty.js");
        assert_eq!(p.ext, ".js");
        assert_eq!(p.name, "index.12ty");
        assert_eq!(parse(".htaccess").ext, "");
    }

    #[test]
    fn test_relative() {
        assert_eq!(relative("/a/b", "/a/c/d"), "../c/d");
    }
}
