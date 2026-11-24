//! Liquid renderer (context, scopes, and tag semantics after liquidjs).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result, anyhow, bail};

use super::filters;
use super::parser::{
    Cond, Expr, FilterArg, FilteredExpr, ForTag, IncludeTag, Node, PartialName, PathSeg,
    ShortcodeArg, ShortcodeRegistry, Template, parse_template,
};
use crate::user_config::{CallContext, FilterFn, PairedShortcodeFn, ShortcodeFn};
use crate::value::{Map, Value};

/// Resolves a partial name (from the including file) to a file path and its contents
pub type PartialLoader =
    Arc<dyn Fn(&str, Option<&str>) -> Result<(String, String)> + Send + Sync>;

#[derive(Clone)]
pub struct LiquidOptions {
    /// Timezone offset in minutes used by the `date` filter (0 = UTC)
    pub timezone_offset: i32,
    /// Named time zone (overrides `timezone_offset`)
    pub timezone: Option<String>,
    /// When true, unknown filters throw
    pub strict_filters: bool,
    pub js_truthy: bool,
    /// liquidjs `dynamicPartials`: partial names are expressions (quoted strings) when true
    pub dynamic_partials: bool,
}

impl Default for LiquidOptions {
    fn default() -> Self {
        LiquidOptions {
            timezone_offset: 0,
            timezone: None,
            strict_filters: true,
            js_truthy: true,
            dynamic_partials: true,
        }
    }
}

/// A Liquid environment: filters, shortcodes and partial lookup
pub struct Liquid {
    pub options: LiquidOptions,
    pub filters: HashMap<String, FilterFn>,
    pub shortcodes: HashMap<String, ShortcodeFn>,
    pub paired_shortcodes: HashMap<String, PairedShortcodeFn>,
    pub raw_paired_shortcodes: std::collections::HashSet<String>,
    pub partial_loader: Option<PartialLoader>,
    partial_cache: Mutex<HashMap<String, Arc<Template>>>,
    /// Called with (including file, included file) for dependency tracking
    pub on_partial: Option<Arc<dyn Fn(Option<&str>, &str) + Send + Sync>>,
}

impl ShortcodeRegistry for Liquid {
    fn is_shortcode(&self, name: &str) -> bool {
        self.shortcodes.contains_key(name)
    }
    fn is_paired_shortcode(&self, name: &str) -> bool {
        self.paired_shortcodes.contains_key(name)
    }
    fn is_raw_paired_shortcode(&self, name: &str) -> bool {
        self.raw_paired_shortcodes.contains(name) && self.paired_shortcodes.contains_key(name)
    }
    fn dynamic_partials(&self) -> bool {
        self.options.dynamic_partials
    }
}

enum Flow {
    Normal,
    Break,
    Continue,
}

pub struct RenderContext {
    scopes: Vec<Map>,
    environments: Value,
    globals: Value,
    counters: HashMap<String, f64>,
    cycles: HashMap<String, usize>,
    blocks: HashMap<String, Vec<Node>>,
    file: Option<String>,
    depth: usize,
}

impl RenderContext {
    pub fn get(&self, key: &str) -> Value {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(key) {
                return v.clone();
            }
        }
        let v = self.environments.get(key);
        if !v.is_undefined() {
            return v;
        }
        self.globals.get(key)
    }

    fn set_bottom(&mut self, key: &str, value: Value) {
        self.scopes[0].insert(key.to_string(), value);
    }

    /// All variables visible in this context (`this.ctx` / `getAll()`)
    pub fn get_all(&self) -> Value {
        let mut map = Map::new();
        if let Value::Object(g) = &self.globals {
            for (k, v) in g.iter() {
                map.insert(k.clone(), v.clone());
            }
        }
        if let Value::Object(e) = &self.environments {
            for (k, v) in e.iter() {
                map.insert(k.clone(), v.clone());
            }
        }
        for scope in &self.scopes {
            for (k, v) in scope.iter() {
                map.insert(k.clone(), v.clone());
            }
        }
        Value::from_map(map)
    }
}

/// liquidjs `readProperty` semantics
pub fn read_property(obj: &Value, key: &Value) -> Value {
    if obj.is_nullish() {
        return Value::Undefined;
    }
    match key {
        Value::Number(n) => {
            if let Value::Array(a) = obj {
                let idx = if *n < 0.0 { a.len() as f64 + n } else { *n };
                if idx < 0.0 {
                    return Value::Undefined;
                }
                return a.get(idx as usize).cloned().unwrap_or_default();
            }
            obj.get(&crate::value::format_number(*n))
        }
        _ => {
            let k = key.to_js_string();
            let v = obj.get(&k);
            if !v.is_undefined() {
                return v;
            }
            match k.as_str() {
                "size" => match obj {
                    Value::Array(_) | Value::String(_) | Value::Object(_) => {
                        Value::Number(obj.len() as f64)
                    }
                    Value::Dynamic(d) => Value::Number(d.keys().len() as f64),
                    _ => Value::Undefined,
                },
                "first" => match obj {
                    Value::Array(a) => a.first().cloned().unwrap_or_default(),
                    _ => Value::Undefined,
                },
                "last" => match obj {
                    Value::Array(a) => a.last().cloned().unwrap_or_default(),
                    _ => Value::Undefined,
                },
                _ => Value::Undefined,
            }
        }
    }
}

/// liquidjs `toEnumerable`
pub fn to_enumerable(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.as_ref().clone(),
        Value::String(s) if !s.is_empty() => vec![v.clone()],
        Value::Object(o) => o
            .iter()
            .map(|(k, v)| Value::array(vec![Value::str(k), v.clone()]))
            .collect(),
        Value::Dynamic(_) => match v.resolve() {
            Value::Array(a) => a.as_ref().clone(),
            other @ Value::Object(_) => to_enumerable(&other),
            _ => vec![],
        },
        _ => vec![],
    }
}

/// liquidjs `stringify` for output
pub fn stringify(v: &Value) -> String {
    match v {
        Value::Undefined | Value::Null => String::new(),
        Value::Array(a) => a.iter().map(stringify).collect::<Vec<_>>().join(""),
        other => other.to_js_string(),
    }
}

fn is_empty_value(v: &Value) -> bool {
    match v {
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

fn is_blank_value(v: &Value) -> bool {
    match v {
        Value::Undefined | Value::Null => true,
        Value::Bool(b) => !b,
        Value::String(s) => s.trim().is_empty(),
        _ => is_empty_value(v),
    }
}

impl Liquid {
    pub fn new(options: LiquidOptions) -> Self {
        Liquid {
            options,
            filters: HashMap::new(),
            shortcodes: HashMap::new(),
            paired_shortcodes: HashMap::new(),
            raw_paired_shortcodes: Default::default(),
            partial_loader: None,
            partial_cache: Mutex::new(HashMap::new()),
            on_partial: None,
        }
    }

    pub fn parse(&self, src: &str, file: Option<&str>) -> Result<Arc<Template>> {
        parse_template(src, file, self)
    }

    pub fn clear_cache(&self) {
        self.partial_cache.lock().unwrap().clear();
    }

    /// Render a parsed template with the given scope data and globals
    pub fn render(&self, tmpl: &Template, data: &Value, globals: &Value) -> Result<String> {
        let mut ctx = RenderContext {
            scopes: vec![Map::new()],
            environments: data.clone(),
            globals: globals.clone(),
            counters: HashMap::new(),
            cycles: HashMap::new(),
            blocks: HashMap::new(),
            file: tmpl.file.clone(),
            depth: 0,
        };
        let mut out = String::new();
        self.render_nodes(&tmpl.nodes, &mut ctx, &mut out)?;
        Ok(out)
    }

    pub fn parse_and_render(&self, src: &str, data: &Value) -> Result<String> {
        let t = self.parse(src, None)?;
        self.render(&t, data, &Value::object())
    }

    fn load_partial(&self, name: &str, from: Option<&str>) -> Result<Arc<Template>> {
        let loader = self
            .partial_loader
            .as_ref()
            .ok_or_else(|| anyhow!("Partial lookup is not configured: {name}"))?;
        let (path, contents) = loader(name, from)?;
        if let Some(cb) = &self.on_partial {
            cb(from, &path);
        }
        if let Some(t) = self.partial_cache.lock().unwrap().get(&path) {
            return Ok(t.clone());
        }
        let t = self
            .parse(&contents, Some(&path))
            .with_context(|| format!("Problem parsing partial {path}"))?;
        self.partial_cache.lock().unwrap().insert(path, t.clone());
        Ok(t)
    }

    pub fn eval_expr(&self, expr: &Expr, ctx: &RenderContext) -> Result<Value> {
        Ok(match expr {
            Expr::Literal(v) => v.clone(),
            Expr::Empty => Value::str(""),
            Expr::Blank => Value::str(""),
            Expr::Range(a, b) => {
                let start = self.eval_expr(a, ctx)?.to_number() as i64;
                let end = self.eval_expr(b, ctx)?.to_number() as i64;
                Value::array((start..=end).map(|n| Value::Number(n as f64)).collect())
            }
            Expr::Path(root, segs) => {
                let mut current = if root.is_empty() {
                    Value::Undefined
                } else {
                    ctx.get(root)
                };
                for (i, seg) in segs.iter().enumerate() {
                    let key = match seg {
                        PathSeg::Key(k) => Value::str(k),
                        PathSeg::Index(e) => self.eval_expr(e, ctx)?,
                    };
                    if i == 0 && root.is_empty() {
                        current = ctx.get(&key.to_js_string());
                        continue;
                    }
                    current = read_property(&current, &key);
                }
                current
            }
        })
    }

    pub fn eval_filtered(&self, expr: &FilteredExpr, ctx: &RenderContext) -> Result<Value> {
        let mut value = self.eval_expr(&expr.base, ctx)?;
        for f in &expr.filters {
            let mut args = Vec::with_capacity(f.args.len());
            for arg in &f.args {
                match arg {
                    FilterArg::Positional(e) => args.push(self.eval_expr(e, ctx)?),
                    FilterArg::Keyword(k, e) => {
                        args.push(Value::array(vec![Value::str(k), self.eval_expr(e, ctx)?]))
                    }
                }
            }
            value = self.apply_filter(&f.name, value, &args, ctx)?;
        }
        Ok(value)
    }

    fn apply_filter(
        &self,
        name: &str,
        input: Value,
        args: &[Value],
        ctx: &RenderContext,
    ) -> Result<Value> {
        if let Some(f) = self.filters.get(name) {
            let lookup = |key: &str| -> Value {
                if key == "__all__" {
                    return ctx.get_all();
                }
                ctx.get(key)
            };
            let call = CallContext::new("liquid", &lookup);
            return f(&call, input, args)
                .with_context(|| format!("Error in Liquid filter `{name}`"));
        }
        if let Some(result) = filters::apply_builtin(name, &input, args, &self.options)? {
            return Ok(result);
        }
        if self.options.strict_filters {
            bail!("undefined filter: {name}");
        }
        Ok(input)
    }

    fn eval_cond(&self, cond: &Cond, ctx: &RenderContext) -> Result<bool> {
        Ok(match cond {
            Cond::Value(e) => self.is_truthy(&self.eval_expr(e, ctx)?),
            Cond::Not(c) => !self.eval_cond(c, ctx)?,
            Cond::And(a, b) => self.eval_cond(a, ctx)? && self.eval_cond(b, ctx)?,
            Cond::Or(a, b) => self.eval_cond(a, ctx)? || self.eval_cond(b, ctx)?,
            Cond::Compare(op, l, r) => {
                // empty/blank comparisons
                if matches!(r, Expr::Empty | Expr::Blank) || matches!(l, Expr::Empty | Expr::Blank)
                {
                    let (other, special) = if matches!(r, Expr::Empty | Expr::Blank) {
                        (l, r)
                    } else {
                        (r, l)
                    };
                    let v = self.eval_expr(other, ctx)?;
                    let is = if matches!(special, Expr::Empty) {
                        is_empty_value(&v)
                    } else {
                        is_blank_value(&v)
                    };
                    return Ok(match op.as_str() {
                        "==" => is,
                        "!=" => !is,
                        _ => false,
                    });
                }
                let a = self.eval_expr(l, ctx)?;
                let b = self.eval_expr(r, ctx)?;
                compare(op, &a, &b)
            }
        })
    }

    fn is_truthy(&self, v: &Value) -> bool {
        if self.options.js_truthy {
            v.is_truthy()
        } else {
            !matches!(v, Value::Undefined | Value::Null | Value::Bool(false))
        }
    }

    fn render_nodes(&self, nodes: &[Node], ctx: &mut RenderContext, out: &mut String) -> Result<Flow> {
        for node in nodes {
            match self.render_node(node, ctx, out)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn render_node(&self, node: &Node, ctx: &mut RenderContext, out: &mut String) -> Result<Flow> {
        match node {
            Node::Text(t) | Node::Raw(t) => out.push_str(t),
            Node::Output(expr, line) => {
                let v = self
                    .eval_filtered(expr, ctx)
                    .with_context(|| format!("line:{line}"))?;
                out.push_str(&stringify(&v));
            }
            Node::Echo(expr) => {
                let v = self.eval_filtered(expr, ctx)?;
                out.push_str(&stringify(&v));
            }
            Node::Assign(var, expr) => {
                let v = self.eval_filtered(expr, ctx)?;
                ctx.set_bottom(var, v);
            }
            Node::Capture(var, body) => {
                let mut buf = String::new();
                self.render_nodes(body, ctx, &mut buf)?;
                ctx.set_bottom(var, Value::str(buf));
            }
            Node::If(branches, else_body) => {
                for (cond, body) in branches {
                    if self.eval_cond(cond, ctx)? {
                        return self.render_nodes(body, ctx, out);
                    }
                }
                return self.render_nodes(else_body, ctx, out);
            }
            Node::Unless(cond, body, elsifs, else_body) => {
                if !self.eval_cond(cond, ctx)? {
                    return self.render_nodes(body, ctx, out);
                }
                for (c, b) in elsifs {
                    if self.eval_cond(c, ctx)? {
                        return self.render_nodes(b, ctx, out);
                    }
                }
                return self.render_nodes(else_body, ctx, out);
            }
            Node::Case(subject, whens, else_body) => {
                let v = self.eval_expr(subject, ctx)?;
                let mut matched = false;
                for (values, body) in whens {
                    for value in values {
                        let w = self.eval_expr(value, ctx)?;
                        if compare("==", &v, &w) {
                            matched = true;
                            break;
                        }
                    }
                    if matched {
                        return self.render_nodes(body, ctx, out);
                    }
                }
                return self.render_nodes(else_body, ctx, out);
            }
            Node::For(tag) => return self.render_for(tag, ctx, out),
            Node::TableRow(tag, cols) => self.render_tablerow(tag, cols.as_ref(), ctx, out)?,
            Node::Break => return Ok(Flow::Break),
            Node::Continue => return Ok(Flow::Continue),
            Node::Cycle(group, values) => {
                let key = match group {
                    Some(g) => self.eval_expr(g, ctx)?.to_js_string(),
                    None => format!("{values:?}"),
                };
                let idx = ctx.cycles.entry(key).or_insert(0);
                if !values.is_empty() {
                    let v = &values[*idx % values.len()];
                    *idx += 1;
                    let v = self.eval_expr(v, ctx)?;
                    out.push_str(&stringify(&v));
                }
            }
            Node::Increment(name) => {
                let counter = ctx.counters.entry(name.clone()).or_insert(0.0);
                out.push_str(&crate::value::format_number(*counter));
                *counter += 1.0;
            }
            Node::Decrement(name) => {
                let counter = ctx.counters.entry(name.clone()).or_insert(0.0);
                *counter -= 1.0;
                out.push_str(&crate::value::format_number(*counter));
            }
            Node::Include(tag) => self.render_include(tag, ctx, out)?,
            Node::Layout(name, hash, body) => {
                // Render the body to collect blocks, then render the layout file
                let Some(name) = name else {
                    return self.render_nodes(body, ctx, out);
                };
                let mut sink = String::new();
                let mut blocks = HashMap::new();
                for n in body {
                    if let Node::Block(block_name, block_body) = n {
                        blocks.insert(block_name.clone(), block_body.clone());
                    }
                }
                let _ = self.render_nodes(body, ctx, &mut sink);
                let file_name = match name {
                    PartialName::Static(s) => s.clone(),
                    PartialName::Dynamic(e) => self.eval_expr(e, ctx)?.to_js_string(),
                };
                let tmpl = self.load_partial(&file_name, ctx.file.as_deref())?;
                let mut scope = Map::new();
                for (k, e) in hash {
                    scope.insert(k.clone(), self.eval_expr(e, ctx)?);
                }
                let saved_blocks = std::mem::replace(&mut ctx.blocks, blocks);
                ctx.scopes.push(scope);
                let r = self.render_nodes(&tmpl.nodes, ctx, out);
                ctx.scopes.pop();
                ctx.blocks = saved_blocks;
                r?;
            }
            Node::Block(name, body) => {
                if let Some(over) = ctx.blocks.get(name).cloned() {
                    self.render_nodes(&over, ctx, out)?;
                } else {
                    self.render_nodes(body, ctx, out)?;
                }
            }
            Node::Shortcode(tag, line) => {
                let mut args = vec![];
                let mut kwargs: Option<Map> = None;
                for arg in &tag.args {
                    match arg {
                        ShortcodeArg::Positional(e) => args.push(self.eval_filtered(e, ctx)?),
                        ShortcodeArg::Keyword(k, e) => {
                            kwargs
                                .get_or_insert_with(Map::new)
                                .insert(k.clone(), self.eval_expr(e, ctx)?);
                        }
                    }
                }
                if let Some(mut kw) = kwargs {
                    kw.insert("__keywords".into(), Value::Bool(true));
                    args.push(Value::from_map(kw));
                }
                let lookup = |key: &str| -> Value {
                    if key == "__all__" {
                        return ctx.get_all();
                    }
                    ctx.get(key)
                };
                let result = if let Some(body) = &tag.body {
                    let mut content = String::new();
                    // body needs a mutable ctx: render it first
                    let _ = lookup;
                    self.render_nodes(body, ctx, &mut content)?;
                    let lookup = |key: &str| -> Value {
                        if key == "__all__" {
                            return ctx.get_all();
                        }
                        ctx.get(key)
                    };
                    let call = CallContext::new("liquid", &lookup);
                    let f = self.paired_shortcodes.get(&tag.name).unwrap();
                    f(&call, &content, &args)
                } else {
                    let call = CallContext::new("liquid", &lookup);
                    let f = self.shortcodes.get(&tag.name).unwrap();
                    f(&call, &args)
                };
                let result = result.with_context(|| {
                    format!("Error with Liquid shortcode `{}`, line:{line}", tag.name)
                })?;
                out.push_str(&result);
            }
        }
        Ok(Flow::Normal)
    }

    fn loop_items(&self, tag: &ForTag, ctx: &RenderContext) -> Result<Vec<Value>> {
        let collection = self.eval_expr(&tag.collection, ctx)?;
        let mut items = to_enumerable(&collection);
        let offset = match &tag.offset {
            Some(e) => self.eval_expr(e, ctx)?.to_number().max(0.0) as usize,
            None => 0,
        };
        if offset > 0 {
            items = items.into_iter().skip(offset).collect();
        }
        if let Some(e) = &tag.limit {
            let limit = self.eval_expr(e, ctx)?.to_number().max(0.0) as usize;
            items.truncate(limit);
        }
        if tag.reversed {
            items.reverse();
        }
        Ok(items)
    }

    fn render_for(&self, tag: &ForTag, ctx: &mut RenderContext, out: &mut String) -> Result<Flow> {
        let items = self.loop_items(tag, ctx)?;
        if items.is_empty() {
            return self.render_nodes(&tag.else_body, ctx, out).map(|_| Flow::Normal);
        }
        let length = items.len();
        let parent = ctx.get("forloop");
        for (i, item) in items.into_iter().enumerate() {
            let mut scope = Map::new();
            let mut forloop = Map::new();
            forloop.insert("name".into(), Value::str(format!("{}-{}", tag.var, "loop")));
            forloop.insert("length".into(), Value::from(length));
            forloop.insert("index".into(), Value::from(i + 1));
            forloop.insert("index0".into(), Value::from(i));
            forloop.insert("rindex".into(), Value::from(length - i));
            forloop.insert("rindex0".into(), Value::from(length - i - 1));
            forloop.insert("first".into(), Value::Bool(i == 0));
            forloop.insert("last".into(), Value::Bool(i == length - 1));
            if !parent.is_undefined() {
                forloop.insert("parentloop".into(), parent.clone());
            }
            scope.insert("forloop".into(), Value::from_map(forloop));
            scope.insert(tag.var.clone(), item);
            ctx.scopes.push(scope);
            let flow = self.render_nodes(&tag.body, ctx, out);
            // keep assigns made at the bottom scope; drop the loop scope
            ctx.scopes.pop();
            match flow? {
                Flow::Break => break,
                Flow::Continue | Flow::Normal => {}
            }
        }
        Ok(Flow::Normal)
    }

    fn render_tablerow(
        &self,
        tag: &ForTag,
        cols: Option<&Expr>,
        ctx: &mut RenderContext,
        out: &mut String,
    ) -> Result<()> {
        let items = self.loop_items(tag, ctx)?;
        let cols = match cols {
            Some(e) => self.eval_expr(e, ctx)?.to_number() as usize,
            None => items.len(),
        }
        .max(1);
        let length = items.len();
        out.push_str("<tr class=\"row1\">");
        for (i, item) in items.into_iter().enumerate() {
            let row = i / cols;
            let col = i % cols;
            if col == 0 && i != 0 {
                out.push_str(&format!("</tr><tr class=\"row{}\">", row + 1));
            }
            let mut scope = Map::new();
            let mut tablerowloop = Map::new();
            tablerowloop.insert("length".into(), Value::from(length));
            tablerowloop.insert("index".into(), Value::from(i + 1));
            tablerowloop.insert("index0".into(), Value::from(i));
            tablerowloop.insert("col".into(), Value::from(col + 1));
            tablerowloop.insert("col0".into(), Value::from(col));
            tablerowloop.insert("row".into(), Value::from(row + 1));
            tablerowloop.insert("first".into(), Value::Bool(i == 0));
            tablerowloop.insert("last".into(), Value::Bool(i == length - 1));
            tablerowloop.insert("col_first".into(), Value::Bool(col == 0));
            tablerowloop.insert("col_last".into(), Value::Bool(col == cols - 1));
            scope.insert("tablerowloop".into(), Value::from_map(tablerowloop));
            scope.insert(tag.var.clone(), item);
            ctx.scopes.push(scope);
            out.push_str(&format!("<td class=\"col{}\">", col + 1));
            let r = self.render_nodes(&tag.body, ctx, out);
            out.push_str("</td>");
            ctx.scopes.pop();
            r?;
        }
        out.push_str("</tr>");
        Ok(())
    }

    fn render_include(&self, tag: &IncludeTag, ctx: &mut RenderContext, out: &mut String) -> Result<()> {
        if ctx.depth > 100 {
            bail!("Maximum include depth exceeded");
        }
        let file_name = match &tag.name {
            PartialName::Static(s) => s.clone(),
            PartialName::Dynamic(e) => {
                let v = self.eval_expr(e, ctx)?;
                if v.is_nullish() {
                    bail!("illegal file path for include: {e:?}");
                }
                v.to_js_string()
            }
        };
        let tmpl = self.load_partial(&file_name, ctx.file.as_deref())?;
        let mut hash = Map::new();
        for (k, e) in &tag.hash {
            hash.insert(k.clone(), self.eval_expr(e, ctx)?);
        }
        let base_name = file_name
            .rsplit('/')
            .next()
            .unwrap_or(&file_name)
            .split('.')
            .next()
            .unwrap_or("")
            .to_string();

        if tag.isolated {
            // `render`: isolated scope (globals only)
            let render_one = |scope: Map, out: &mut String| -> Result<()> {
                let mut child = RenderContext {
                    scopes: vec![scope],
                    environments: Value::object(),
                    globals: ctx.globals.clone(),
                    counters: HashMap::new(),
                    cycles: HashMap::new(),
                    blocks: HashMap::new(),
                    file: tmpl.file.clone(),
                    depth: ctx.depth + 1,
                };
                self.render_nodes(&tmpl.nodes, &mut child, out)?;
                Ok(())
            };
            if let Some((e, alias)) = &tag.for_loop {
                let items = to_enumerable(&self.eval_expr(e, ctx)?);
                let length = items.len();
                for (i, item) in items.into_iter().enumerate() {
                    let mut scope = hash.clone();
                    scope.insert(alias.clone().unwrap_or_else(|| base_name.clone()), item);
                    let mut forloop = Map::new();
                    forloop.insert("length".into(), Value::from(length));
                    forloop.insert("index".into(), Value::from(i + 1));
                    forloop.insert("index0".into(), Value::from(i));
                    forloop.insert("first".into(), Value::Bool(i == 0));
                    forloop.insert("last".into(), Value::Bool(i == length - 1));
                    scope.insert("forloop".into(), Value::from_map(forloop));
                    render_one(scope, out)?;
                }
                return Ok(());
            }
            let mut scope = hash;
            if let Some((e, alias)) = &tag.with {
                scope.insert(
                    alias.clone().unwrap_or_else(|| base_name.clone()),
                    self.eval_expr(e, ctx)?,
                );
            }
            return render_one(scope, out);
        }

        let mut scope = hash;
        if let Some((e, alias)) = &tag.with {
            scope.insert(
                alias.clone().unwrap_or_else(|| base_name.clone()),
                self.eval_expr(e, ctx)?,
            );
        }
        let saved_file = ctx.file.replace(tmpl.file.clone().unwrap_or_default());
        ctx.scopes.push(scope);
        ctx.depth += 1;
        let r = self.render_nodes(&tmpl.nodes, ctx, out);
        ctx.depth -= 1;
        ctx.scopes.pop();
        ctx.file = saved_file;
        r?;
        Ok(())
    }
}

/// Liquid comparison operators
pub fn compare(op: &str, a: &Value, b: &Value) -> bool {
    match op {
        "==" => liquid_equals(a, b),
        "!=" | "<>" => !liquid_equals(a, b),
        "<" => matches!(order(a, b), Some(std::cmp::Ordering::Less)),
        ">" => matches!(order(a, b), Some(std::cmp::Ordering::Greater)),
        "<=" => matches!(
            order(a, b),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        ),
        ">=" => matches!(
            order(a, b),
            Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
        ),
        "contains" => match a {
            Value::String(s) => s.contains(&b.to_js_string()),
            Value::Array(arr) => arr.iter().any(|x| liquid_equals(x, b)),
            Value::Object(o) => o.contains_key(&b.to_js_string()),
            Value::Dynamic(_) => a.has(&b.to_js_string()),
            _ => false,
        },
        _ => false,
    }
}

fn order(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    if a.is_nullish() || b.is_nullish() {
        return None;
    }
    a.js_compare(b)
}

fn liquid_equals(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undefined | Value::Null, Value::Undefined | Value::Null) => true,
        (Value::Dynamic(_), _) | (_, Value::Dynamic(_)) => a.strict_equals(b),
        _ => a.strict_equals(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(src: &str, data: serde_json::Value) -> String {
        let lib = Liquid::new(LiquidOptions::default());
        lib.parse_and_render(src, &Value::from_json(data)).unwrap()
    }

    #[test]
    fn basics() {
        assert_eq!(render("Hello {{ name }}", serde_json::json!({"name": "Zach"})), "Hello Zach");
        assert_eq!(render("{{ missing }}", serde_json::json!({})), "");
        assert_eq!(
            render("{% if a > 1 %}big{% else %}small{% endif %}", serde_json::json!({"a": 2})),
            "big"
        );
        assert_eq!(
            render("{% for i in (1..3) %}{{ i }}{% unless forloop.last %},{% endunless %}{% endfor %}", serde_json::json!({})),
            "1,2,3"
        );
        assert_eq!(
            render("{% assign x = 'a,b' | split: ',' %}{{ x | join: '-' }}", serde_json::json!({})),
            "a-b"
        );
        assert_eq!(
            render("{% capture c %}hi {{ n }}{% endcapture %}{{ c | upcase }}", serde_json::json!({"n": 1})),
            "HI 1"
        );
        assert_eq!(render("{{ arr.size }} {{ arr.first }}", serde_json::json!({"arr": [3, 4]})), "2 3");
        assert_eq!(
            render("{% case x %}{% when 1, 2 %}a{% else %}b{% endcase %}", serde_json::json!({"x": 2})),
            "a"
        );
        assert_eq!(render("{% if '' %}t{% else %}f{% endif %}", serde_json::json!({})), "f");
        assert_eq!(render("{% if arr == empty %}e{% endif %}", serde_json::json!({"arr": []})), "e");
    }

    #[test]
    fn whitespace() {
        assert_eq!(render("a {%- if true -%} b {%- endif -%} c", serde_json::json!({})), "abc");
    }
}
