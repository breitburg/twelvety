//! Port of `Util/GlobMatcher.js` (picomatch semantics via `globset`, plus an extglob matcher).

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};

use super::template_path;

pub fn build_glob(pattern: &str, case_insensitive: bool) -> Option<Glob> {
    GlobBuilder::new(pattern)
        .case_insensitive(case_insensitive)
        .literal_separator(true)
        .backslash_escape(true)
        .empty_alternates(true)
        .build()
        .ok()
}

/// A set of globs. `globset` doesn’t support extglobs (`+(a|b)`, `!(x)`, …), so patterns using
/// them are matched by [`ExtGlob`] instead.
#[derive(Clone, Debug, Default)]
pub struct PatternSet {
    globs: Option<GlobSet>,
    /// Index into the original patterns for each glob in `globs`
    glob_indexes: Vec<usize>,
    extglobs: Vec<(usize, ExtGlob)>,
}

impl PatternSet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_match(&self, path: impl AsRef<str>) -> bool {
        let path = path.as_ref();
        self.globs.as_ref().is_some_and(|g| g.is_match(path))
            || self.extglobs.iter().any(|(_, e)| e.is_match(path))
    }

    /// Indexes of the matching patterns
    pub fn matches(&self, path: impl AsRef<str>) -> Vec<usize> {
        let path = path.as_ref();
        let mut out: Vec<usize> = self
            .globs
            .as_ref()
            .map(|g| g.matches(path).into_iter().map(|i| self.glob_indexes[i]).collect())
            .unwrap_or_default();
        out.extend(self.extglobs.iter().filter(|(_, e)| e.is_match(path)).map(|(i, _)| *i));
        out.sort_unstable();
        out
    }
}

/// A single compiled pattern
pub fn build_matcher(pattern: &str) -> Option<PatternSet> {
    let set = build_glob_set(&[pattern.to_string()]);
    (set.globs.is_some() || !set.extglobs.is_empty()).then_some(set)
}

pub fn build_glob_set(patterns: &[String]) -> PatternSet {
    let mut builder = GlobSetBuilder::new();
    let mut set = PatternSet::default();
    for (i, p) in patterns.iter().enumerate() {
        let p = template_path::strip_leading_dot_slash(p);
        if has_extglob(&p) {
            if let Some(e) = ExtGlob::new(&p) {
                set.extglobs.push((i, e));
            }
        } else if let Some(g) = build_glob(&p, true) {
            builder.add(g);
            set.glob_indexes.push(i);
        }
    }
    if !set.glob_indexes.is_empty() {
        set.globs = builder.build().ok();
    }
    set
}

/// The pattern a negated glob (`!pattern`, but not an extglob like `!(a|b)`) excludes
pub fn strip_negation(pattern: &str) -> Option<&str> {
    pattern.strip_prefix('!').filter(|_| !pattern.starts_with("!("))
}

/// Whether a pattern uses extglob syntax (`@(…)`, `?(…)`, `+(…)`, `*(…)`, `!(…)`)
pub fn has_extglob(pattern: &str) -> bool {
    let chars: Vec<char> = pattern.chars().collect();
    chars
        .windows(2)
        .enumerate()
        .any(|(i, w)| matches!(w[0], '@' | '?' | '+' | '*' | '!') && w[1] == '(' && (i == 0 || chars[i - 1] != '\\'))
}

#[derive(Clone, Debug)]
enum Node {
    Lit(char),
    /// `?`
    Any,
    /// `*` (within a path segment)
    Star,
    /// `**/`: zero or more whole segments
    GlobstarSlash,
    /// `/**` at the end of a pattern (or a lone `**`): anything below
    GlobstarEnd,
    Class { negated: bool, ranges: Vec<(char, char)> },
    /// `{a,b}` or `@(a|b)`
    Alt(Vec<Vec<Node>>),
    /// `kind(a|b)` with kind one of `?`, `+`, `*`, `!`
    Ext(char, Vec<Vec<Node>>),
}

/// A (case-insensitive) glob with extglob support, following picomatch semantics
#[derive(Clone, Debug)]
pub struct ExtGlob {
    nodes: Vec<Node>,
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    /// Parse a sequence until one of `terminators` (at nesting level 0) or the end
    fn sequence(&mut self, terminators: &[char]) -> Option<Vec<Node>> {
        let mut nodes = vec![];
        while let Some(c) = self.peek(0) {
            if terminators.contains(&c) {
                break;
            }
            let at_segment_start = self.pos == 0 || self.chars[self.pos - 1] == '/';
            match c {
                '\\' => {
                    self.pos += 1;
                    nodes.push(Node::Lit(self.peek(0)?.to_ascii_lowercase()));
                    self.pos += 1;
                }
                '@' | '?' | '+' | '*' | '!' if self.peek(1) == Some('(') => {
                    self.pos += 2;
                    let alts = self.alternatives('|', ')')?;
                    nodes.push(if c == '@' { Node::Alt(alts) } else { Node::Ext(c, alts) });
                }
                '*' if self.peek(1) == Some('*') && at_segment_start => {
                    match self.peek(2) {
                        Some('/') => {
                            self.pos += 3;
                            nodes.push(Node::GlobstarSlash);
                        }
                        None => {
                            self.pos += 2;
                            if matches!(nodes.last(), Some(Node::Lit('/'))) {
                                nodes.pop();
                            }
                            nodes.push(Node::GlobstarEnd);
                        }
                        _ => {
                            self.pos += 2;
                            nodes.push(Node::Star);
                        }
                    }
                }
                '*' => {
                    self.pos += 1;
                    if !matches!(nodes.last(), Some(Node::Star)) {
                        nodes.push(Node::Star);
                    }
                }
                '?' => {
                    self.pos += 1;
                    nodes.push(Node::Any);
                }
                '[' => nodes.push(self.class()?),
                '{' => {
                    self.pos += 1;
                    nodes.push(Node::Alt(self.alternatives(',', '}')?));
                }
                _ => {
                    self.pos += 1;
                    nodes.push(Node::Lit(c.to_ascii_lowercase()));
                }
            }
        }
        Some(nodes)
    }

    fn alternatives(&mut self, separator: char, close: char) -> Option<Vec<Vec<Node>>> {
        let mut alts = vec![];
        loop {
            alts.push(self.sequence(&[separator, close])?);
            match self.peek(0)? {
                c if c == separator => self.pos += 1,
                _ => {
                    self.pos += 1;
                    return Some(alts);
                }
            }
        }
    }

    fn class(&mut self) -> Option<Node> {
        self.pos += 1;
        let negated = matches!(self.peek(0), Some('!' | '^'));
        if negated {
            self.pos += 1;
        }
        let mut ranges = vec![];
        let mut first = true;
        loop {
            let c = self.peek(0)?;
            if c == ']' && !first {
                self.pos += 1;
                break;
            }
            first = false;
            let start = c.to_ascii_lowercase();
            if self.peek(1) == Some('-') && self.peek(2).is_some_and(|e| e != ']') {
                ranges.push((start, self.peek(2)?.to_ascii_lowercase()));
                self.pos += 3;
            } else {
                ranges.push((start, start));
                self.pos += 1;
            }
        }
        Some(Node::Class { negated, ranges })
    }
}

fn segment_end(s: &[char], i: usize) -> usize {
    s[i..].iter().position(|c| *c == '/').map(|p| i + p).unwrap_or(s.len())
}

fn concat(a: &[Node], b: &[Node]) -> Vec<Node> {
    a.iter().chain(b.iter()).cloned().collect()
}

impl ExtGlob {
    pub fn new(pattern: &str) -> Option<Self> {
        let mut parser = Parser { chars: pattern.chars().collect(), pos: 0 };
        let nodes = parser.sequence(&[])?;
        (parser.pos == parser.chars.len()).then_some(ExtGlob { nodes })
    }

    pub fn is_match(&self, path: &str) -> bool {
        let s: Vec<char> = path.chars().map(|c| c.to_ascii_lowercase()).collect();
        match_nodes(&self.nodes, &s, 0)
    }
}

fn match_nodes(nodes: &[Node], s: &[char], i: usize) -> bool {
    let Some((node, rest)) = nodes.split_first() else {
        return i == s.len();
    };
    match node {
        Node::Lit(c) => s.get(i) == Some(c) && match_nodes(rest, s, i + 1),
        Node::Any => s.get(i).is_some_and(|c| *c != '/') && match_nodes(rest, s, i + 1),
        Node::Star => (i..=segment_end(s, i)).any(|j| match_nodes(rest, s, j)),
        Node::GlobstarSlash => {
            match_nodes(rest, s, i) || (i..s.len()).any(|j| s[j] == '/' && match_nodes(rest, s, j + 1))
        }
        Node::GlobstarEnd => i == s.len() || i == 0 || s[i] == '/',
        Node::Class { negated, ranges } => s.get(i).is_some_and(|c| {
            *c != '/' && ranges.iter().any(|(a, b)| a <= c && c <= b) != *negated
        }) && match_nodes(rest, s, i + 1),
        Node::Alt(alts) => alts.iter().any(|alt| match_nodes(&concat(alt, rest), s, i)),
        Node::Ext('?', alts) => {
            match_nodes(rest, s, i) || alts.iter().any(|alt| match_nodes(&concat(alt, rest), s, i))
        }
        Node::Ext('+', alts) => {
            let repeat = Node::Ext('*', alts.clone());
            alts.iter().any(|alt| {
                !alt.is_empty() && match_nodes(&concat(alt, &concat(std::slice::from_ref(&repeat), rest)), s, i)
            })
        }
        Node::Ext('*', alts) => {
            match_nodes(rest, s, i)
                || alts.iter().any(|alt| {
                    !alt.is_empty() && match_nodes(&concat(alt, nodes), s, i)
                })
        }
        Node::Ext(_, alts) => {
            // `!(…)`: anything within the segment that doesn’t match the alternatives. Like
            // picomatch’s negative lookahead, when more pattern follows no alternative may match
            // at this position at all (`!(index)*.md` doesn’t match `index2.md`).
            let end = segment_end(s, i);
            if rest.is_empty() {
                return end == s.len()
                    && !alts.iter().any(|alt| match_nodes(alt, &s[..end], i));
            }
            let segment = &s[..end];
            let blocked = alts.iter().any(|alt| {
                (i..=end).any(|j| match_nodes(alt, &segment[..j], i))
            });
            !blocked && (i..=end).any(|j| match_nodes(rest, s, j))
        }
    }
}

/// `isGlobMatch(filepath, globs)`
pub fn is_glob_match(filepath: &str, globs: &[String]) -> bool {
    if filepath.is_empty() || globs.is_empty() {
        return false;
    }
    let input_path = template_path::strip_leading_dot_slash(filepath);
    let mut positives = vec![];
    let mut negatives = vec![];
    for g in globs {
        if let Some(neg) = strip_negation(g) {
            negatives.push(neg.to_string());
        } else {
            positives.push(g.clone());
        }
    }
    build_glob_set(&positives).is_match(&input_path) && !build_glob_set(&negatives).is_match(&input_path)
}

/// `isDynamicPattern` (picomatch.scan isGlob || negated)
pub fn is_dynamic_pattern(pattern: &str) -> bool {
    if pattern.starts_with('!') {
        return true;
    }
    let mut escaped = false;
    for c in pattern.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '*' | '?' | '[' | '{' | '(' => return true,
            _ => {}
        }
    }
    false
}

/// The static base directory of a glob (picomatch.scan().base)
pub fn glob_base(pattern: &str) -> String {
    let mut parts = vec![];
    for seg in pattern.split('/') {
        if is_dynamic_pattern(seg) {
            break;
        }
        parts.push(seg);
    }
    if parts.len() == pattern.split('/').count() {
        // not a glob: the base is the parent dir
        parts.pop();
    }
    parts.join("/")
}

pub fn has_negated_pattern(patterns: &[String]) -> bool {
    patterns.iter().any(|p| strip_negation(p).is_some())
}

struct IgnoreRule {
    negated: bool,
    glob: String,
    git: bool,
    base: String,
    matcher: Option<PatternSet>,
}

/// `createIgnoreMatcher` (ignore rules with negation support)
pub struct IgnoreMatcher {
    rules: Vec<IgnoreRule>,
}

fn is_overlapping_base(a: &str, b: &str) -> bool {
    a.is_empty()
        || b.is_empty()
        || a == b
        || a.starts_with(&format!("{b}/"))
        || b.starts_with(&format!("{a}/"))
}

impl IgnoreMatcher {
    pub fn new(patterns: &[String], git_patterns: &[String]) -> Self {
        let git: Vec<String> = git_patterns
            .iter()
            .map(|p| template_path::strip_leading_dot_slash(p))
            .collect();
        let rules = patterns
            .iter()
            .map(|entry| {
                let negated = strip_negation(entry).is_some();
                let glob = template_path::strip_leading_dot_slash(strip_negation(entry).unwrap_or(entry));
                IgnoreRule {
                    negated,
                    git: !negated && git.contains(&glob),
                    base: glob_base(&glob).to_lowercase(),
                    matcher: build_matcher(&glob),
                    glob,
                }
            })
            .collect();
        Self { rules }
    }

    fn has_later_negation(&self, index: usize, base: &str) -> bool {
        self.rules[index + 1..]
            .iter()
            .any(|r| r.negated && is_overlapping_base(&r.base, base))
    }

    fn get_last_match(&self, input_path: &str) -> Option<(usize, &IgnoreRule)> {
        self.rules
            .iter()
            .enumerate()
            .rev()
            .find(|(_, r)| r.matcher.as_ref().map(|m| m.is_match(input_path)).unwrap_or(false))
    }

    pub fn get_prunable_ignores(&self) -> Vec<String> {
        self.rules
            .iter()
            .enumerate()
            .filter(|(i, r)| !r.negated && !self.has_later_negation(*i, &r.base))
            .map(|(_, r)| r.glob.clone())
            .collect()
    }

    pub fn is_ignored(&self, filepath: &str, is_directory: bool) -> bool {
        let input_path = template_path::strip_leading_dot_slash(filepath);
        let segments: Vec<&str> = input_path.split('/').collect();
        for depth in 1..segments.len() {
            if let Some((_, rule)) = self.get_last_match(&segments[..depth].join("/")) {
                if rule.git {
                    return true;
                }
            }
        }
        match self.get_last_match(&input_path) {
            None => false,
            Some((_, rule)) if rule.negated => false,
            Some((index, rule)) => {
                rule.git
                    || !(is_directory
                        && self.has_later_negation(index, &input_path.to_lowercase()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches() {
        assert!(is_glob_match("./src/test.md", &["src/**/*.md".into()]));
        assert!(is_glob_match("./src/a/test.MD", &["./src/**/*.md".into()]));
        assert!(!is_glob_match("./src/test.njk", &["src/**/*.md".into()]));
        assert!(is_glob_match("./test.md", &["**/*.md".into()]));
    }

    #[test]
    fn dynamic() {
        assert!(is_dynamic_pattern("src/*.md"));
        assert!(!is_dynamic_pattern("src/index.md"));
        assert_eq!(glob_base("src/posts/**/*.md"), "src/posts");
    }
}
