//! Port of `Engines/*.js`: template engines.

pub mod custom;
pub mod html;
pub mod liquid;
pub mod markdown;
pub mod nunjucks;
pub mod nunjucks_value;
pub mod template_engine_manager;

use std::sync::Arc;

use anyhow::Result;

use crate::value::Value;

/// A compiled template: renders with data
pub type RenderFn = Arc<dyn Fn(&Value) -> Result<String> + Send + Sync>;

/// How a permalink string should be handled by an engine
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PermalinkCompilation {
    /// Render the permalink with this engine
    Compile,
    /// Use the permalink string as-is
    Raw,
    /// Treat the permalink as `false` (no output)
    Disabled,
}

/// Port of `Engines/TemplateEngine.js`
pub trait TemplateEngine: Send + Sync {
    fn name(&self) -> &str;

    fn cacheable(&self) -> bool {
        false
    }

    /// Wait for any one-time engine setup (e.g. a custom engine’s `init`)
    fn ready(&self) -> Result<()> {
        Ok(())
    }

    /// Compile a template string. `input_path` is the file the string came from.
    fn compile(&self, src: &str, input_path: Option<&str>) -> Result<RenderFn>;

    fn needs_compilation(&self, _src: &str) -> bool {
        true
    }

    fn permalink_needs_compilation(&self, src: &str) -> PermalinkCompilation {
        if self.needs_compilation(src) {
            PermalinkCompilation::Compile
        } else {
            PermalinkCompilation::Raw
        }
    }

    fn default_template_file_extension(&self) -> String {
        "html".into()
    }

    fn use_layouts(&self) -> bool {
        true
    }

    fn needs_to_read_file_contents(&self) -> bool {
        true
    }

    /// Extra data supplied by the engine (e.g. custom engines' `getData`)
    fn get_extra_data_from_file(&self, _input_path: &str) -> Result<Value> {
        Ok(Value::object())
    }

    /// Variables referenced by a template string (used for computed data ordering)
    fn parse_for_symbols(&self, _src: &str) -> Option<Vec<String>> {
        None
    }

    /// Display name for custom engines (`addExtension` `name` option)
    fn custom_name(&self) -> Option<String> {
        None
    }

    fn is_custom(&self) -> bool {
        false
    }
}

/// Extract variable references from `{{ … }}` and `{% … %}` blocks with a regex
/// (shared approximation of `parseForSymbols`).
pub fn find_symbols(src: &str) -> Vec<String> {
    static RE_BLOCK: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?s)\{\{(.*?)\}\}|\{%(.*?)%\}").unwrap()
    });
    static RE_IDENT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#""[^"]*"|'[^']*'|([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$-]*|\[(?:"[^"]*"|'[^']*'|\d+)\])*)"#).unwrap()
    });
    const KEYWORDS: &[&str] = &[
        "if", "else", "elif", "elsif", "endif", "for", "endfor", "in", "and", "or", "not", "true",
        "false", "null", "nil", "none", "None", "True", "False", "set", "endset", "assign",
        "capture", "endcapture", "unless", "endunless", "case", "when", "endcase", "include",
        "render", "with", "as", "limit", "offset", "reversed", "empty", "blank", "contains",
        "is", "defined", "undefined", "macro", "endmacro", "block", "endblock", "extends",
        "import", "from", "raw", "endraw", "comment", "endcomment", "echo", "liquid", "break",
        "continue", "cycle", "increment", "decrement", "tablerow", "endtablerow", "filter",
        "endfilter", "call", "endcall",
    ];
    let mut out: Vec<String> = vec![];
    for caps in RE_BLOCK.captures_iter(src) {
        let inner = caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str()).unwrap_or("");
        // Drop filter names: identifiers right after `|`
        let mut after_pipe = false;
        let mut last_end = 0;
        for ident in RE_IDENT.captures_iter(inner) {
            let whole = ident.get(0).unwrap();
            let between = &inner[last_end..whole.start()];
            last_end = whole.end();
            if between.contains('|') {
                after_pipe = true;
            }
            let Some(m) = ident.get(1) else {
                continue;
            };
            let prev_char = inner[..m.start()].trim_end().chars().last();
            if after_pipe && prev_char == Some('|') {
                after_pipe = false;
                continue;
            }
            after_pipe = false;
            // object keys in filter args (e.g. `key: value`) and assignments
            let next = inner[m.end()..].trim_start();
            if next.starts_with('(') {
                continue;
            }
            let name = m.as_str();
            let root = name.split(['.', '[']).next().unwrap_or("");
            if KEYWORDS.contains(&root) {
                continue;
            }
            // normalize brackets with string keys to dot notation
            let normalized = name
                .replace("[\"", ".")
                .replace("\"]", "")
                .replace("['", ".")
                .replace("']", "");
            if !out.contains(&normalized) {
                out.push(normalized);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols() {
        let s = find_symbols("{{ page.url | url }} {% if collections.posts %}{{ title }}{% endif %}");
        assert!(s.contains(&"page.url".to_string()));
        assert!(s.contains(&"collections.posts".to_string()));
        assert!(s.contains(&"title".to_string()));
        assert!(!s.contains(&"url".to_string()));
    }
}
