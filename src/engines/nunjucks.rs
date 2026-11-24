//! Port of `Engines/Nunjucks.js`.
//!
//! Nunjucks is implemented with `minijinja` (a Jinja2 engine, close to Nunjucks
//! syntax). A small source compatibility pass translates Nunjucks-specific tags
//! (shortcodes, `for k, v in obj`, `elseif`, `asyncEach`, …) to minijinja syntax.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{Result, anyhow};
use minijinja::value::Rest;
use minijinja::{AutoEscape, Environment, Error, ErrorKind, State, UndefinedBehavior, Value as MjValue};

use super::nunjucks_value::{from_mj, to_mj};
use super::{RenderFn, TemplateEngine};
use crate::global_dependency_map::GlobalDependencyMap;
use crate::template_config::Config;
use crate::user_config::CallContext;
use crate::util::project_directories::ProjectDirectories;
use crate::util::template_path;
use crate::value::Value;

const COMPILED_PREFIX: &str = "__twelvety_compiled__/";

pub struct NunjucksEngine {
    name: String,
    env: Arc<Environment<'static>>,
    sources: Arc<RwLock<HashMap<String, String>>>,
    shortcodes: Arc<HashSet<String>>,
    paired_shortcodes: Arc<HashSet<String>>,
}

fn mj_err(e: anyhow::Error) -> Error {
    Error::new(ErrorKind::InvalidOperation, format!("{e:#}"))
}

fn state_lookup<'a>(state: &'a State<'_, '_>) -> impl Fn(&str) -> Value + 'a {
    move |key: &str| match state.lookup(key) {
        Some(v) => from_mj(&v),
        None => Value::Undefined,
    }
}

impl NunjucksEngine {
    pub fn new(
        name: &str,
        config: &Config,
        dirs: &ProjectDirectories,
        uses: Arc<Mutex<GlobalDependencyMap>>,
    ) -> Self {
        let mut env: Environment<'static> = Environment::new();
        env.set_keep_trailing_newline(true);
        env.set_undefined_behavior(UndefinedBehavior::Chainable);
        let autoescape = config
            .nunjucks_environment_options
            .get("autoescape")
            .as_bool()
            .unwrap_or(true);
        env.set_auto_escape_callback(move |_| {
            if autoescape {
                AutoEscape::Html
            } else {
                AutoEscape::None
            }
        });
        if config.nunjucks_environment_options.get("trimBlocks").is_truthy() {
            env.set_trim_blocks(true);
        }
        if config.nunjucks_environment_options.get("lstripBlocks").is_truthy() {
            env.set_lstrip_blocks(true);
        }

        let sources: Arc<RwLock<HashMap<String, String>>> = Arc::new(RwLock::new(HashMap::new()));
        let shortcodes: Arc<HashSet<String>> = Arc::new(
            config
                .nunjucks
                .shortcodes
                .keys()
                .cloned()
                .collect(),
        );
        // Raw paired shortcodes (body passed unrendered) are marked with a `raw:` entry
        let paired: Arc<HashSet<String>> = Arc::new(
            config
                .nunjucks
                .paired_shortcodes
                .keys()
                .cloned()
                .chain(
                    config
                        .raw_paired_shortcodes
                        .iter()
                        .filter(|n| config.nunjucks.paired_shortcodes.contains_key(*n))
                        .map(|n| format!("raw:{n}")),
                )
                .collect(),
        );

        // FileSystemLoader([includes, cwd])
        let search_paths = vec![dirs.includes(), ".".to_string()];
        let (src_map, sc, psc) = (sources.clone(), shortcodes.clone(), paired.clone());
        env.set_loader(move |name: &str| {
            if let Some(src) = src_map.read().unwrap().get(name) {
                return Ok(Some(src.clone()));
            }
            let candidates: Vec<String> = if name.starts_with('/') {
                vec![name.to_string()]
            } else {
                search_paths
                    .iter()
                    .map(|p| template_path::join(&[p, name]))
                    .collect()
            };
            for candidate in candidates {
                let path = std::path::Path::new(&candidate);
                if path.is_file() {
                    let contents = std::fs::read_to_string(path).map_err(|e| {
                        Error::new(ErrorKind::InvalidOperation, format!("{candidate}: {e}"))
                    })?;
                    return Ok(Some(nunjucks_compat(&contents, &sc, &psc)));
                }
            }
            Ok(None)
        });
        // Relative paths (`./`, `../`) resolve relative to the parent template
        let uses_for_join = uses.clone();
        let engine_name = name.to_string();
        env.set_path_join_callback(move |name, parent| {
            let parent_path = parent
                .strip_prefix(COMPILED_PREFIX)
                .map(|p| p.rsplit_once('#').map(|(a, _)| a).unwrap_or(p))
                .unwrap_or(parent);
            let resolved = if name.starts_with("./") || name.starts_with("../") {
                let dir = template_path::get_dir_from_file_path(parent_path);
                template_path::join(&[&dir, name])
            } else {
                name.to_string()
            };
            let from = if parent_path.is_empty() { engine_name.as_str() } else { parent_path };
            uses_for_join
                .lock()
                .unwrap()
                .add_dependency(from, &[resolved.clone()]);
            resolved.into()
        });

        // Nunjucks output: undefined/null print nothing, values print like JavaScript
        // and escaping only covers `& < > " '`.
        // `throwOnUndefined` makes printing `undefined`/`null` an error (Nunjucks’ `ensureDefined`)
        let throw_on_undefined = config
            .nunjucks_environment_options
            .get("throwOnUndefined")
            .is_truthy();
        env.set_formatter(move |out, state, value| {
            if value.is_undefined() || value.is_none() {
                if throw_on_undefined {
                    return Err(Error::new(
                        ErrorKind::UndefinedError,
                        "attempted to output null or undefined value",
                    ));
                }
                return Ok(());
            }
            let s = js_string(value);
            if matches!(state.auto_escape(), AutoEscape::Html) && !value.is_safe() {
                out.write_str(&njk_escape(&s))
                    .map_err(|_| Error::new(ErrorKind::WriteFailure, "write failed"))
            } else {
                out.write_str(&s)
                    .map_err(|_| Error::new(ErrorKind::WriteFailure, "write failed"))
            }
        });
        register_nunjucks_filters(&mut env);
        // JavaScript methods on strings and arrays (`tags.includes('x')`, `name.toUpperCase()`)
        env.set_unknown_method_callback(|_state, value, method, args| {
            let args: Vec<Value> = args.iter().map(from_mj).collect();
            js_method(&from_mj(value), method, &args)
                .map(|v| to_mj(&v))
                .ok_or_else(|| Error::from(ErrorKind::UnknownMethod))
        });

        // `__njk_items`: `for k, v in obj` iterates object entries in Nunjucks
        env.add_filter("__njk_items", |v: MjValue| -> Result<MjValue, Error> {
            match from_mj(&v).resolve() {
                Value::Object(o) => Ok(MjValue::from(
                    o.iter()
                        .map(|(k, v)| MjValue::from(vec![MjValue::from(k.as_str()), to_mj(v)]))
                        .collect::<Vec<_>>(),
                )),
                _ => Ok(v),
            }
        });

        for (name, f) in &config.nunjucks.filters {
            let f = f.clone();
            let fname = name.clone();
            env.add_filter(
                name.clone(),
                move |state: &State, value: MjValue, args: Rest<MjValue>| -> Result<MjValue, Error> {
                    let lookup = state_lookup(state);
                    let call = CallContext::new("njk", &lookup);
                    let args: Vec<Value> = args.iter().map(from_mj).collect();
                    let input = from_mj(&value);
                    let result = f(&call, input.clone(), &args).map_err(|e| {
                        let page = from_mj(&state.lookup("page").unwrap_or_default());
                        let suffix = if page.is_truthy() {
                            format!(" ({})", page.get("inputPath").to_output_string())
                        } else {
                            String::new()
                        };
                        mj_err(e.context(format!("Error in Nunjucks Filter `{fname}`{suffix}")))
                    })?;
                    // Returning the (safe) input unchanged keeps it safe, like returning
                    // the same `SafeString` object in Nunjucks
                    if value.is_safe() && result.is_string() && result.strict_equals(&input) {
                        return Ok(MjValue::from_safe_string(result.to_js_string()));
                    }
                    Ok(to_mj(&result))
                },
            );
        }
        for (name, f) in &config.nunjucks.shortcodes {
            let f = f.clone();
            let sname = name.clone();
            env.add_function(
                format!("__sc_{}", sanitize_name(name)),
                move |state: &State, args: Rest<MjValue>| -> Result<MjValue, Error> {
                    let lookup = state_lookup(state);
                    let call = CallContext::new("njk", &lookup);
                    let args: Vec<Value> = args.iter().map(from_mj).collect();
                    let out = f(&call, &args).map_err(|e| {
                        mj_err(e.context(format!("Error with Nunjucks shortcode `{sname}`")))
                    })?;
                    Ok(MjValue::from_safe_string(out))
                },
            );
        }
        for (name, f) in &config.nunjucks.paired_shortcodes {
            let f = f.clone();
            let sname = name.clone();
            env.add_filter(
                format!("__psc_{}", sanitize_name(name)),
                move |state: &State, body: MjValue, args: Rest<MjValue>| -> Result<MjValue, Error> {
                    let lookup = state_lookup(state);
                    let call = CallContext::new("njk", &lookup);
                    let args: Vec<Value> = args.iter().map(from_mj).collect();
                    let content = body.as_str().map(String::from).unwrap_or_else(|| body.to_string());
                    let out = f(&call, &content, &args).map_err(|e| {
                        mj_err(e.context(format!("Error with Nunjucks paired shortcode `{sname}`")))
                    })?;
                    Ok(MjValue::from_safe_string(out))
                },
            );
        }
        for (name, value) in &config.nunjucks_globals {
            env.add_global(name.clone(), to_mj(value));
        }
        for amendment in &config.nunjucks_amendments {
            amendment(&mut env);
        }

        NunjucksEngine {
            name: name.into(),
            env: Arc::new(env),
            sources,
            shortcodes,
            paired_shortcodes: paired,
        }
    }

    pub fn render_string(&self, src: &str, data: &Value) -> Result<String> {
        let f = self.compile(src, None)?;
        f(data)
    }
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

impl TemplateEngine for NunjucksEngine {
    fn name(&self) -> &str {
        &self.name
    }

    fn cacheable(&self) -> bool {
        true
    }

    fn needs_compilation(&self, src: &str) -> bool {
        src.contains("{%") || src.contains("{{") || src.contains("{#")
    }

    fn parse_for_symbols(&self, src: &str) -> Option<Vec<String>> {
        Some(parse_symbols(src))
    }

    fn compile(&self, src: &str, input_path: Option<&str>) -> Result<RenderFn> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        src.hash(&mut hasher);
        let path = match input_path {
            None | Some("njk") | Some("md") => String::new(),
            Some(p) => p.to_string(),
        };
        let name = format!("{COMPILED_PREFIX}{path}#{:x}", hasher.finish());
        {
            let mut sources = self.sources.write().unwrap();
            if !sources.contains_key(&name) {
                sources.insert(
                    name.clone(),
                    nunjucks_compat(src, &self.shortcodes, &self.paired_shortcodes),
                );
            }
        }
        // compile eagerly to surface syntax errors
        self.env
            .get_template(&name)
            .map_err(|e| anyhow!(format_mj_error(&e)))?;
        let env = self.env.clone();
        Ok(Arc::new(move |data: &Value| {
            let tmpl = env.get_template(&name).map_err(|e| anyhow!(format_mj_error(&e)))?;
            tmpl.render(to_mj(data))
                .map_err(|e| anyhow!(format_mj_error(&e)))
        }))
    }
}

fn format_mj_error(e: &Error) -> String {
    let mut msg = e.to_string();
    let mut source = std::error::Error::source(e);
    while let Some(s) = source {
        msg.push_str(&format!("\n  caused by: {s}"));
        source = s.source();
    }
    msg
}

/// Translate Nunjucks-only syntax to minijinja (Jinja2) syntax.
pub fn nunjucks_compat(src: &str, shortcodes: &HashSet<String>, paired: &HashSet<String>) -> String {
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        let rest = &src[i..];
        let Some(start) = rest.find("{%").or_else(|| rest.find("{{")) else {
            out.push_str(rest);
            break;
        };
        // handle the earliest of `{%` and `{{`
        let tag_pos = rest.find("{%");
        let out_pos = rest.find("{{");
        let (pos, is_tag) = match (tag_pos, out_pos) {
            (Some(t), Some(o)) => {
                if t < o {
                    (t, true)
                } else {
                    (o, false)
                }
            }
            (Some(t), None) => (t, true),
            (None, Some(o)) => (o, false),
            (None, None) => (start, true),
        };
        out.push_str(&rest[..pos]);
        let abs = i + pos;
        let close = if is_tag { "%}" } else { "}}" };
        let Some(end_rel) = find_close(&src[abs + 2..], close) else {
            out.push_str(&src[abs..]);
            break;
        };
        let end = abs + 2 + end_rel + 2;
        let inner = &src[abs + 2..end - 2];
        let (lmod, inner_trim, rmod) = strip_modifiers(inner);
        if !is_tag {
            out.push_str(&format!("{{{{{lmod} {} {rmod}}}}}", js_operators(inner_trim)));
            i = end;
            continue;
        }
        let trimmed = inner_trim.trim();
        let (name, args) = match trimmed.find(char::is_whitespace) {
            Some(p) => (&trimmed[..p], trimmed[p..].trim()),
            None => (trimmed, ""),
        };
        if name == "raw" || name == "verbatim" {
            // copy verbatim until endraw/endverbatim
            match find_end_tag(src, end, &format!("end{name}")) {
                Some((s, e)) => {
                    out.push_str(&format!("{{%{lmod} raw {rmod}%}}"));
                    out.push_str(&src[end..s]);
                    let (l2, _, r2) = strip_modifiers(&src[s + 2..e - 2]);
                    out.push_str(&format!("{{%{l2} endraw {r2}%}}"));
                    i = e;
                }
                None => {
                    out.push_str(&src[abs..]);
                    i = src.len();
                }
            }
            continue;
        }
        if paired.contains(name) && paired.contains(&format!("raw:{name}")) {
            if let Some((s, e)) = find_end_tag(src, end, &format!("end{name}")) {
                let args = js_operators(&strip_trailing_comma(args));
                let call = if args.is_empty() {
                    format!("__psc_{}", sanitize_name(name))
                } else {
                    format!("__psc_{}({})", sanitize_name(name), args)
                };
                let (l2, _, r2) = strip_modifiers(&src[s + 2..e - 2]);
                out.push_str(&format!("{{%{lmod} filter {call} {rmod}%}}{{% raw %}}"));
                out.push_str(&src[end..s]);
                out.push_str(&format!("{{% endraw %}}{{%{l2} endfilter {r2}%}}"));
                i = e;
                continue;
            }
        }
        let translated = if shortcodes.contains(name) {
            format!(
                "{{{{{lmod} __sc_{}({}) {rmod}}}}}",
                sanitize_name(name),
                js_operators(&strip_trailing_comma(args))
            )
        } else if paired.contains(name) {
            let args = js_operators(&strip_trailing_comma(args));
            if args.is_empty() {
                format!("{{%{lmod} filter __psc_{} {rmod}%}}", sanitize_name(name))
            } else {
                format!("{{%{lmod} filter __psc_{}({}) {rmod}%}}", sanitize_name(name), args)
            }
        } else if let Some(base) = name.strip_prefix("end").filter(|b| paired.contains(*b)) {
            let _ = base;
            format!("{{%{lmod} endfilter {rmod}%}}")
        } else {
            match name {
                "elseif" => format!("{{%{lmod} elif {} {rmod}%}}", js_operators(args)),
                "asyncEach" | "asyncAll" | "for" => {
                    format!("{{%{lmod} for {} {rmod}%}}", translate_for(args))
                }
                "endeach" | "endall" => format!("{{%{lmod} endfor {rmod}%}}"),
                "ifAsync" => format!("{{%{lmod} if {} {rmod}%}}", js_operators(args)),
                "endifAsync" => format!("{{%{lmod} endif {rmod}%}}"),
                "setAsync" => format!("{{%{lmod} set {} {rmod}%}}", args),
                "endsetAsync" => format!("{{%{lmod} endset {rmod}%}}"),
                _ => format!("{{%{lmod} {} {rmod}%}}", js_operators(inner_trim.trim())),
            }
        };
        out.push_str(&translated);
        i = end;
    }
    out
}

/// Find `{% end_name %}` starting at `from`: returns (start, end) byte offsets
fn find_end_tag(src: &str, from: usize, end_name: &str) -> Option<(usize, usize)> {
    let mut search = from;
    while let Some(p) = src[search..].find("{%") {
        let s = search + p;
        let c = find_close(&src[s + 2..], "%}")?;
        let e = s + 2 + c + 2;
        let (_, t, _) = strip_modifiers(&src[s + 2..e - 2]);
        if t.trim() == end_name {
            return Some((s, e));
        }
        search = e;
    }
    None
}

fn strip_trailing_comma(args: &str) -> String {
    args.trim().trim_end_matches(',').to_string()
}

fn find_close(s: &str, close: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
        } else if c == b'"' || c == b'\'' {
            quote = Some(c);
        } else if s[i..].starts_with(close) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn strip_modifiers(inner: &str) -> (&'static str, &str, &'static str) {
    let mut s = inner;
    let mut l = "";
    let mut r = "";
    if let Some(rest) = s.strip_prefix('-') {
        l = "-";
        s = rest;
    } else if let Some(rest) = s.strip_prefix('+') {
        l = "+";
        s = rest;
    }
    if let Some(rest) = s.strip_suffix('-') {
        r = "-";
        s = rest;
    } else if let Some(rest) = s.strip_suffix('+') {
        r = "+";
        s = rest;
    }
    (l, s, r)
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Expression syntax differences (outside strings):
/// - `===`/`!==` to `==`/`!=`
/// - bare object literal keys (`{ key: value }`) are quoted, as Nunjucks treats them as names
fn js_operators(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut quote: Option<char> = None;
    let mut brackets: Vec<char> = vec![];
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '{' | '(' | '[' => brackets.push(c),
            '}' | ')' | ']' => {
                brackets.pop();
            }
            _ => {}
        }
        if (c == '=' || c == '!') && i + 2 < chars.len() && chars[i + 1] == '=' && chars[i + 2] == '=' {
            out.push(c);
            out.push('=');
            i += 3;
            continue;
        }
        if is_ident_start(c) && (i == 0 || !is_ident_char(chars[i - 1])) {
            let mut j = i;
            while j < chars.len() && is_ident_char(chars[j]) {
                j += 1;
            }
            let ident: String = chars[i..j].iter().collect();
            let in_object = brackets.last() == Some(&'{');
            let after_separator = matches!(out.trim_end().chars().last(), Some('{') | Some(','));
            let before_colon = chars[j..].iter().find(|c| !c.is_whitespace()) == Some(&':');
            if in_object && after_separator && before_colon {
                out.push_str(&format!("\"{ident}\""));
            } else {
                out.push_str(&ident);
            }
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn translate_for(args: &str) -> String {
    let args = js_operators(args);
    // `k, v in obj` -> `k, v in obj|__njk_items`
    if let Some(in_pos) = find_keyword(&args, " in ") {
        let targets = &args[..in_pos];
        if targets.contains(',') {
            let iter = args[in_pos + 4..].trim();
            return format!("{} in ({})|__njk_items", targets.trim(), iter);
        }
    }
    args
}

fn find_keyword(s: &str, kw: &str) -> Option<usize> {
    s.find(kw)
}

/// The subset of JavaScript `String.prototype`/`Array.prototype` methods callable from
/// templates (Nunjucks calls methods on the underlying JavaScript values).
fn js_method(value: &Value, method: &str, args: &[Value]) -> Option<Value> {
    let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Undefined);
    // relative index (negative counts from the end) clamped to `0..=len`
    let index = |v: Value, len: usize, default: usize| -> usize {
        if v.is_undefined() {
            return default;
        }
        let n = v.to_number();
        let n = if n.is_nan() { 0.0 } else { n.trunc() };
        if n < 0.0 {
            (len as f64 + n).max(0.0) as usize
        } else {
            (n as usize).min(len)
        }
    };
    match value {
        Value::Array(items) => {
            let position = |needle: &Value| items.iter().position(|v| v.strict_equals(needle));
            Some(match method {
                "includes" => Value::Bool(position(&arg(0)).is_some()),
                "indexOf" => Value::Number(position(&arg(0)).map(|p| p as f64).unwrap_or(-1.0)),
                "join" => {
                    let sep = arg(0);
                    let sep = if sep.is_undefined() { ",".to_string() } else { sep.to_js_string() };
                    Value::str(
                        items
                            .iter()
                            .map(|v| if v.is_nullish() { String::new() } else { v.to_js_string() })
                            .collect::<Vec<_>>()
                            .join(&sep),
                    )
                }
                "slice" => {
                    let start = index(arg(0), items.len(), 0);
                    let end = index(arg(1), items.len(), items.len());
                    Value::array(items.get(start..end.max(start)).unwrap_or_default().to_vec())
                }
                "concat" => {
                    let mut all = items.as_ref().clone();
                    for a in args {
                        match a {
                            Value::Array(more) => all.extend(more.iter().cloned()),
                            other => all.push(other.clone()),
                        }
                    }
                    Value::array(all)
                }
                "at" => {
                    let i = arg(0).to_number().trunc();
                    let i = if i < 0.0 { items.len() as f64 + i } else { i };
                    if i < 0.0 {
                        Value::Undefined
                    } else {
                        items.get(i as usize).cloned().unwrap_or(Value::Undefined)
                    }
                }
                "toString" => Value::str(value.to_js_string()),
                _ => return None,
            })
        }
        Value::String(s) => {
            let chars: Vec<char> = s.chars().collect();
            let len = chars.len();
            let substring = |a: usize, b: usize| -> Value {
                Value::str(chars[a.min(b)..b.max(a)].iter().collect::<String>())
            };
            let needle = arg(0).to_js_string();
            let char_index = |byte: usize| s[..byte].chars().count() as f64;
            Some(match method {
                "includes" => Value::Bool(s.contains(needle.as_str())),
                "startsWith" => Value::Bool(s.starts_with(needle.as_str())),
                "endsWith" => Value::Bool(s.ends_with(needle.as_str())),
                "indexOf" => Value::Number(s.find(needle.as_str()).map(char_index).unwrap_or(-1.0)),
                "lastIndexOf" => {
                    Value::Number(s.rfind(needle.as_str()).map(char_index).unwrap_or(-1.0))
                }
                "toUpperCase" | "toLocaleUpperCase" => Value::str(s.to_uppercase()),
                "toLowerCase" | "toLocaleLowerCase" => Value::str(s.to_lowercase()),
                "trim" => Value::str(s.trim()),
                "trimStart" => Value::str(s.trim_start()),
                "trimEnd" => Value::str(s.trim_end()),
                "slice" => substring(index(arg(0), len, 0), index(arg(1), len, len).max(index(arg(0), len, 0))),
                "substring" => {
                    let clamp = |v: Value, default: usize| -> usize {
                        if v.is_undefined() {
                            return default;
                        }
                        let n = v.to_number();
                        if n.is_nan() || n < 0.0 { 0 } else { (n as usize).min(len) }
                    };
                    substring(clamp(arg(0), 0), clamp(arg(1), len))
                }
                "charAt" => {
                    let i = arg(0).to_number();
                    let i = if i.is_nan() { 0.0 } else { i.trunc() };
                    if i < 0.0 || i as usize >= len {
                        Value::str("")
                    } else {
                        substring(i as usize, i as usize + 1)
                    }
                }
                "split" => {
                    let sep = arg(0);
                    if sep.is_undefined() {
                        Value::array(vec![value.clone()])
                    } else if needle.is_empty() {
                        Value::array(chars.iter().map(|c| Value::str(c.to_string())).collect())
                    } else {
                        Value::array(s.split(needle.as_str()).map(Value::str).collect())
                    }
                }
                "replace" => Value::str(s.replacen(needle.as_str(), &arg(1).to_js_string(), 1)),
                "replaceAll" => Value::str(s.replace(needle.as_str(), &arg(1).to_js_string())),
                "repeat" => Value::str(s.repeat(arg(0).to_number().max(0.0) as usize)),
                "padStart" | "padEnd" => {
                    let width = arg(0).to_number().max(0.0) as usize;
                    let fill = arg(1);
                    let fill = if fill.is_undefined() { " ".to_string() } else { fill.to_js_string() };
                    if width <= len || fill.is_empty() {
                        return Some(value.clone());
                    }
                    let pad: String = fill.chars().cycle().take(width - len).collect();
                    if method == "padStart" {
                        Value::str(pad + s)
                    } else {
                        Value::str(s.to_string() + &pad)
                    }
                }
                "toString" | "valueOf" => value.clone(),
                _ => return None,
            })
        }
        _ => None,
    }
}

/// `parseForSymbols`: variable references in the order Nunjucks’ parser finds `Symbol`
/// nodes. Filter names count as symbols (`{{ a | f }}` → `["f", "a"]`, a filter node lists
/// its name before its arguments), tag names and member access by index are left out.
fn parse_symbols(src: &str) -> Vec<String> {
    static RE_COMMENT: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?s)\{#.*?#\}").unwrap());
    let src = RE_COMMENT.replace_all(src, "");
    let mut out: Vec<String> = vec![];
    let mut rest: &str = &src;
    while let Some(start) = rest.find(['{']) {
        let after = &rest[start..];
        let close = if after.starts_with("{{") {
            "}}"
        } else if after.starts_with("{%") {
            "%}"
        } else {
            rest = &rest[start + 1..];
            continue;
        };
        let Some(end) = find_close(&after[2..], close) else {
            break;
        };
        let (_, inner, _) = strip_modifiers(&after[2..2 + end]);
        let expression = if close == "%}" {
            // drop the tag name
            let inner = inner.trim_start();
            inner.find(char::is_whitespace).map(|p| &inner[p..]).unwrap_or("")
        } else {
            inner
        };
        for symbol in expression_symbols(expression) {
            if !out.contains(&symbol) {
                out.push(symbol);
            }
        }
        rest = &after[2 + end + 2..];
    }
    out
}

fn expression_symbols(expr: &str) -> Vec<String> {
    // split on top-level `|` (filters)
    let chars: Vec<char> = expr.chars().collect();
    let mut segments: Vec<String> = vec![String::new()];
    let mut quote: Option<char> = None;
    let mut depth = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            if c == '\\' {
                segments.last_mut().unwrap().push(c);
                i += 1;
            } else if c == q {
                quote = None;
            }
        } else {
            match c {
                '"' | '\'' => quote = Some(c),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                '|' if depth == 0 => {
                    segments.push(String::new());
                    i += 1;
                    continue;
                }
                _ => {}
            }
        }
        if let Some(&ch) = chars.get(i) {
            segments.last_mut().unwrap().push(ch);
        }
        i += 1;
    }
    let mut out = vec![];
    let mut filter_args = vec![];
    for segment in segments[1..].iter().rev() {
        let segment = segment.trim_start();
        let name_len = segment.find(|c: char| !is_ident_char(c)).unwrap_or(segment.len());
        if name_len > 0 {
            out.push(segment[..name_len].to_string());
        }
        filter_args.push(&segment[name_len..]);
    }
    out.extend(identifiers(&segments[0]));
    for args in filter_args.into_iter().rev() {
        out.extend(identifiers(args));
    }
    out
}

/// Names (with `.member` chains) referenced in an expression, skipping strings and keywords
fn identifiers(expr: &str) -> Vec<String> {
    const KEYWORDS: &[&str] = &[
        "and", "or", "not", "in", "is", "if", "else", "true", "false", "none", "null", "True",
        "False", "None", "as", "import", "with", "without", "context", "ignore", "missing",
    ];
    let chars: Vec<char> = expr.chars().collect();
    let mut out = vec![];
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            if c == '\\' {
                i += 1;
            } else if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            quote = Some(c);
            i += 1;
            continue;
        }
        let member = i > 0 && chars[..i].iter().rev().find(|c| !c.is_whitespace()) == Some(&'.');
        if is_ident_start(c) && (i == 0 || !is_ident_char(chars[i - 1])) {
            let start = i;
            while i < chars.len()
                && (is_ident_char(chars[i])
                    || (chars[i] == '.' && chars.get(i + 1).is_some_and(|c| is_ident_start(*c))))
            {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            if !member && !KEYWORDS.contains(&name.as_str()) {
                out.push(name);
            }
            continue;
        }
        if c.is_ascii_digit() {
            // skip number literals (`1.5`, `1e3`)
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '.') {
                i += 1;
            }
            continue;
        }
        i += 1;
    }
    out
}

/// JavaScript `String(value)` for a minijinja value
fn js_string(value: &MjValue) -> String {
    use minijinja::value::ValueKind;
    match value.kind() {
        ValueKind::Undefined | ValueKind::None => String::new(),
        ValueKind::String => value.as_str().unwrap_or("").to_string(),
        ValueKind::Seq | ValueKind::Map | ValueKind::Iterable => from_mj(value).to_js_string(),
        _ => value.to_string(),
    }
}

/// Nunjucks’ `escape` (lib.escape)
pub fn njk_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

fn register_nunjucks_filters(env: &mut Environment<'static>) {
    for name in ["escape", "e"] {
        env.add_filter(name, |v: MjValue| -> MjValue {
            if v.is_safe() {
                return v;
            }
            MjValue::from_safe_string(njk_escape(&js_string(&v)))
        });
    }
    env.add_filter("dump", |v: MjValue, spaces: Option<MjValue>| -> String {
        let value = from_mj(&v);
        match spaces.and_then(|s| i64::try_from(s).ok()) {
            Some(n) if n > 0 => value.to_json_string_pretty(n as usize),
            _ => value.to_json_string(),
        }
    });
    env.add_filter("striptags", |v: String| -> String {
        let re = regex::Regex::new(r"(?s)<[^>]*>").unwrap();
        let s = re.replace_all(&v, "");
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    });
    env.add_filter(
        "truncate",
        |v: String, length: Option<usize>, killwords: Option<bool>, end: Option<String>| -> String {
            let length = length.unwrap_or(255);
            let end = end.unwrap_or_else(|| "...".into());
            let chars: Vec<char> = v.chars().collect();
            if chars.len() <= length {
                return v;
            }
            if killwords.unwrap_or(false) {
                return chars[..length].iter().collect::<String>() + &end;
            }
            let truncated: String = chars[..length].iter().collect();
            let idx = truncated.rfind(' ').unwrap_or(truncated.len());
            truncated[..idx].to_string() + &end
        },
    );
    env.add_filter("wordcount", |v: String| -> usize { v.split_whitespace().count() });
    env.add_filter("nl2br", |v: MjValue| -> MjValue {
        let s = v.to_string().replace("\r\n", "\n").replace('\n', "<br />\n");
        if v.is_safe() {
            MjValue::from_safe_string(s)
        } else {
            MjValue::from(s)
        }
    });
    env.add_filter("center", |v: String, width: Option<usize>| -> String {
        let width = width.unwrap_or(80);
        let len = v.chars().count();
        if len >= width {
            return v;
        }
        let spaces = width - len;
        let pre = spaces / 2;
        let post = spaces - pre;
        format!("{}{}{}", " ".repeat(pre), v, " ".repeat(post))
    });
    env.add_filter("forceescape", |v: MjValue| -> MjValue {
        MjValue::from_safe_string(njk_escape(&js_string(&v)))
    });
    env.add_filter("random", |v: MjValue| -> MjValue {
        let items: Vec<MjValue> = v.try_iter().map(|i| i.collect()).unwrap_or_default();
        if items.is_empty() {
            return MjValue::UNDEFINED;
        }
        let idx = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0) as usize)
            % items.len();
        items[idx].clone()
    });
    env.add_filter("urlize", |v: String| -> MjValue {
        let re = regex::Regex::new(r"(https?://[^\s<]+)").unwrap();
        MjValue::from_safe_string(re.replace_all(&v, "<a href=\"$1\">$1</a>").to_string())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compat() {
        let sc: HashSet<String> = ["youtube".to_string()].into_iter().collect();
        let psc: HashSet<String> = ["note".to_string()].into_iter().collect();
        assert_eq!(
            nunjucks_compat("{% youtube \"abc\", 1 %}", &sc, &psc),
            "{{ __sc_youtube(\"abc\", 1) }}"
        );
        assert_eq!(
            nunjucks_compat("{% note %}x{% endnote %}", &sc, &psc),
            "{% filter __psc_note %}x{% endfilter %}"
        );
        assert_eq!(
            nunjucks_compat("{% for k, v in obj %}{% endfor %}", &sc, &psc),
            "{% for k, v in (obj)|__njk_items %}{% endfor %}"
        );
        assert_eq!(
            nunjucks_compat("{% if a === 'b' %}{% elseif c %}{% endif %}", &sc, &psc),
            "{% if a == 'b' %}{% elif c %}{% endif %}"
        );
        assert_eq!(
            nunjucks_compat("{% youtube { id: x, \"t\": 1, n: {a: 'b:c'} } %}", &sc, &psc),
            "{{ __sc_youtube({ \"id\": x, \"t\": 1, \"n\": {\"a\": 'b:c'} }) }}"
        );
        assert_eq!(nunjucks_compat("{{a if b else c}}", &sc, &psc), "{{ a if b else c }}");
    }

    #[test]
    fn symbols() {
        assert_eq!(parse_symbols("{{ a | f(b) | g }}{% for x in y.z %}{{ 'q' }}{% endfor %}"), [
            "g", "f", "a", "b", "x", "y.z"
        ]);
        assert_eq!(parse_symbols("{# {{ hidden }} #}{{ obj.method().prop }}"), ["obj.method"]);
    }

    #[test]
    fn js_methods() {
        let s = Value::str("Hello World");
        assert_eq!(js_method(&s, "slice", &[Value::Number(-5.0)]), Some(Value::str("World")));
        assert_eq!(js_method(&s, "indexOf", &[Value::str("o")]), Some(Value::Number(4.0)));
        let a = Value::array(vec![Value::str("a"), Value::Number(1.0)]);
        assert_eq!(js_method(&a, "join", &[Value::str("-")]), Some(Value::str("a-1")));
        assert_eq!(js_method(&a, "includes", &[Value::Number(1.0)]), Some(Value::Bool(true)));
        assert_eq!(js_method(&a, "nope", &[]), None);
    }
}
