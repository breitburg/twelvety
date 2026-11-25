//! Parser for Liquid templates (AST construction).

use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow, bail};

use super::lexer::{ExprToken, Token, lex_expression, tokenize_with};
use crate::value::Value;

#[derive(Debug, Clone)]
pub enum PathSeg {
    Key(String),
    Index(Expr),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Literal(Value),
    Range(Box<Expr>, Box<Expr>),
    Path(String, Vec<PathSeg>),
    Empty,
    Blank,
}

#[derive(Debug, Clone)]
pub enum FilterArg {
    Positional(Expr),
    Keyword(String, Expr),
}

#[derive(Debug, Clone)]
pub struct FilterCall {
    pub name: String,
    pub args: Vec<FilterArg>,
}

#[derive(Debug, Clone)]
pub struct FilteredExpr {
    pub base: Expr,
    pub filters: Vec<FilterCall>,
}

#[derive(Debug, Clone)]
pub enum Cond {
    Value(Expr),
    Compare(String, Expr, Expr),
    Not(Box<Cond>),
    And(Box<Cond>, Box<Cond>),
    Or(Box<Cond>, Box<Cond>),
}

#[derive(Debug, Clone)]
pub struct ForTag {
    pub var: String,
    pub collection: Expr,
    pub limit: Option<Expr>,
    pub offset: Option<Expr>,
    pub reversed: bool,
    pub body: Vec<Node>,
    pub else_body: Vec<Node>,
}

#[derive(Debug, Clone)]
pub enum PartialName {
    Static(String),
    Dynamic(Expr),
}

#[derive(Debug, Clone)]
pub struct IncludeTag {
    pub name: PartialName,
    pub with: Option<(Expr, Option<String>)>,
    pub for_loop: Option<(Expr, Option<String>)>,
    pub hash: Vec<(String, Expr)>,
    pub isolated: bool,
}

#[derive(Debug, Clone)]
pub struct ShortcodeTag {
    pub name: String,
    pub args: Vec<ShortcodeArg>,
    pub body: Option<Vec<Node>>,
}

#[derive(Debug, Clone)]
pub enum Node {
    Text(String),
    Output(FilteredExpr, usize),
    Assign(String, FilteredExpr),
    Capture(String, Vec<Node>),
    If(Vec<(Cond, Vec<Node>)>, Vec<Node>),
    Unless(Cond, Vec<Node>, Vec<(Cond, Vec<Node>)>, Vec<Node>),
    Case(Expr, Vec<(Vec<Expr>, Vec<Node>)>, Vec<Node>),
    For(Box<ForTag>),
    TableRow(Box<ForTag>, Option<Expr>),
    Break,
    Continue,
    Cycle(Option<Expr>, Vec<Expr>),
    Increment(String),
    Decrement(String),
    Include(Box<IncludeTag>),
    Echo(FilteredExpr),
    Layout(Option<PartialName>, Vec<(String, Expr)>, Vec<Node>),
    Block(String, Vec<Node>),
    Shortcode(Box<ShortcodeTag>, usize),
    Raw(String),
}

#[derive(Debug, Clone)]
pub struct Template {
    pub nodes: Vec<Node>,
    pub file: Option<String>,
}

/// Which tag names are registered as shortcodes (and whether they're paired)
pub trait ShortcodeRegistry {
    fn is_shortcode(&self, name: &str) -> bool;
    fn is_paired_shortcode(&self, name: &str) -> bool;
    /// Paired shortcodes that receive their body unparsed
    fn is_raw_paired_shortcode(&self, _name: &str) -> bool {
        false
    }
    /// liquidjs `dynamicPartials`: when false, partial names are unquoted literal paths
    fn dynamic_partials(&self) -> bool {
        true
    }
}

pub struct NoShortcodes;
impl ShortcodeRegistry for NoShortcodes {
    fn is_shortcode(&self, _: &str) -> bool {
        false
    }
    fn is_paired_shortcode(&self, _: &str) -> bool {
        false
    }
}

pub fn parse_template(
    src: &str,
    file: Option<&str>,
    shortcodes: &dyn ShortcodeRegistry,
) -> Result<Arc<Template>> {
    let tokens = tokenize_with(src, &|name| shortcodes.is_raw_paired_shortcode(name))?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        shortcodes,
    };
    let (nodes, end) = parser.parse_until(&[])?;
    if let Some((name, line)) = end {
        bail!("unexpected tag {{% {name} %}}, line:{line}");
    }
    Ok(Arc::new(Template {
        nodes,
        file: file.map(|s| s.to_string()),
    }))
}

struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    shortcodes: &'a dyn ShortcodeRegistry,
}

type EndTag = Option<(String, usize)>;

impl<'a> Parser<'a> {
    /// Parse nodes until one of the `ends` tags is reached. Returns the end tag found.
    fn parse_until(&mut self, ends: &[&str]) -> Result<(Vec<Node>, EndTag)> {
        let mut nodes = Vec::new();
        while self.pos < self.tokens.len() {
            let token = self.tokens[self.pos].clone();
            self.pos += 1;
            match token {
                Token::Text(t) => nodes.push(Node::Text(t)),
                Token::Raw(t) => nodes.push(Node::Raw(t)),
                Token::Output { content, line } => {
                    let expr = parse_filtered(&content).with_context(|| {
                        format!("invalid output {{{{ {content} }}}}, line:{line}")
                    })?;
                    nodes.push(Node::Output(expr, line));
                }
                Token::Tag { name, args, line } => {
                    if ends.contains(&name.as_str()) {
                        // put back args for the caller through the end tag
                        self.pos -= 1;
                        return Ok((nodes, Some((name, line))));
                    }
                    if let Some(node) = self
                        .parse_tag(&name, &args, line)
                        .with_context(|| format!("tag {{% {name} {args} %}}, line:{line}"))?
                    {
                        nodes.push(node);
                    }
                }
            }
        }
        if !ends.is_empty() {
            return Ok((nodes, None));
        }
        Ok((nodes, None))
    }

    fn current_args(&self) -> String {
        match &self.tokens[self.pos] {
            Token::Tag { args, .. } => args.clone(),
            _ => String::new(),
        }
    }

    fn expect_block(&mut self, tag: &str, ends: &[&str]) -> Result<(Vec<Node>, String, String)> {
        let (body, end) = self.parse_until(ends)?;
        let Some((end_name, _)) = end else {
            bail!("tag {{% {tag} %}} not closed");
        };
        let args = self.current_args();
        self.pos += 1;
        Ok((body, end_name, args))
    }

    fn parse_tag(&mut self, name: &str, args: &str, line: usize) -> Result<Option<Node>> {
        Ok(Some(match name {
            "#" => return Ok(None),
            "comment" => {
                // skip everything until endcomment (allow nesting)
                let mut depth = 1;
                while self.pos < self.tokens.len() {
                    if let Token::Tag { name, .. } = &self.tokens[self.pos] {
                        if name == "comment" {
                            depth += 1;
                        } else if name == "endcomment" {
                            depth -= 1;
                            if depth == 0 {
                                self.pos += 1;
                                return Ok(None);
                            }
                        }
                    }
                    self.pos += 1;
                }
                bail!("tag {{% comment %}} not closed");
            }
            "assign" => {
                let eq = args
                    .find('=')
                    .ok_or_else(|| anyhow!("illegal assign expression: {args}"))?;
                let var = args[..eq].trim().to_string();
                let value = parse_filtered(args[eq + 1..].trim())?;
                Node::Assign(var, value)
            }
            "capture" => {
                let var = args
                    .trim()
                    .trim_matches(|c| c == '"' || c == '\'')
                    .to_string();
                let (body, _, _) = self.expect_block("capture", &["endcapture"])?;
                Node::Capture(var, body)
            }
            "echo" => Node::Echo(parse_filtered(if args.is_empty() { "''" } else { args })?),
            "liquid" => {
                // each line is a tag
                let mut tokens = Vec::new();
                for l in args.lines() {
                    let l = l.trim();
                    if l.is_empty() {
                        continue;
                    }
                    let end = l.find(char::is_whitespace).unwrap_or(l.len());
                    tokens.push(Token::Tag {
                        name: l[..end].to_string(),
                        args: l[end..].trim().to_string(),
                        line,
                    });
                }
                let mut sub = Parser {
                    tokens,
                    pos: 0,
                    shortcodes: self.shortcodes,
                };
                let (nodes, _) = sub.parse_until(&[])?;
                Node::If(
                    vec![(Cond::Value(Expr::Literal(Value::Bool(true))), nodes)],
                    vec![],
                )
            }
            "if" => {
                let mut branches = vec![];
                let mut cond = parse_condition(args)?;
                let mut else_body = vec![];
                loop {
                    let (body, end, end_args) =
                        self.expect_block("if", &["elsif", "else", "endif"])?;
                    match end.as_str() {
                        "elsif" => {
                            branches.push((cond, body));
                            cond = parse_condition(&end_args)?;
                        }
                        "else" => {
                            branches.push((cond, body));
                            let (body, _, _) = self.expect_block("if", &["endif"])?;
                            else_body = body;
                            break;
                        }
                        _ => {
                            branches.push((cond, body));
                            break;
                        }
                    }
                }
                Node::If(branches, else_body)
            }
            "unless" => {
                let cond = parse_condition(args)?;
                let mut elsifs = vec![];
                let mut else_body = vec![];
                let (body, mut end, mut end_args) =
                    self.expect_block("unless", &["elsif", "else", "endunless"])?;
                loop {
                    match end.as_str() {
                        "elsif" => {
                            let c = parse_condition(&end_args)?;
                            let (b, e, a) =
                                self.expect_block("unless", &["elsif", "else", "endunless"])?;
                            elsifs.push((c, b));
                            end = e;
                            end_args = a;
                        }
                        "else" => {
                            let (b, _, _) = self.expect_block("unless", &["endunless"])?;
                            else_body = b;
                            break;
                        }
                        _ => break,
                    }
                }
                Node::Unless(cond, body, elsifs, else_body)
            }
            "case" => {
                let subject = parse_expr_str(args)?;
                let mut whens = vec![];
                let mut else_body = vec![];
                // skip anything before the first when
                let (_, mut end, mut end_args) =
                    self.expect_block("case", &["when", "else", "endcase"])?;
                loop {
                    match end.as_str() {
                        "when" => {
                            let values = parse_when_values(&end_args)?;
                            let (body, e, a) =
                                self.expect_block("case", &["when", "else", "endcase"])?;
                            whens.push((values, body));
                            end = e;
                            end_args = a;
                        }
                        "else" => {
                            let (body, e, a) =
                                self.expect_block("case", &["when", "else", "endcase"])?;
                            else_body = body;
                            end = e;
                            end_args = a;
                        }
                        _ => break,
                    }
                }
                Node::Case(subject, whens, else_body)
            }
            "for" | "tablerow" => {
                let (tag, cols) = parse_for_args(args, name == "tablerow")?;
                let end_name = format!("end{name}");
                let (body, end, _) = self.expect_block(name, &["else", &end_name])?;
                let mut tag = tag;
                tag.body = body;
                if end == "else" {
                    let (else_body, _, _) = self.expect_block(name, &[&end_name])?;
                    tag.else_body = else_body;
                }
                if name == "tablerow" {
                    Node::TableRow(Box::new(tag), cols)
                } else {
                    Node::For(Box::new(tag))
                }
            }
            "break" => Node::Break,
            "continue" => Node::Continue,
            "cycle" => {
                let toks = lex_expression(args)?;
                let mut group = None;
                let mut rest: &[ExprToken] = &toks;
                if let Some(colon) = toks.iter().position(|t| *t == ExprToken::Colon) {
                    let mut p = ExprParser::new(toks[..colon].to_vec());
                    group = Some(p.parse_value()?);
                    rest = &toks[colon + 1..];
                }
                let mut values = vec![];
                let mut p = ExprParser::new(rest.to_vec());
                while !p.done() {
                    values.push(p.parse_value()?);
                    p.eat(&ExprToken::Comma);
                }
                Node::Cycle(group, values)
            }
            "increment" => Node::Increment(args.trim().to_string()),
            "decrement" => Node::Decrement(args.trim().to_string()),
            "include" | "render" => Node::Include(Box::new(parse_include_args(
                args,
                name == "render",
                self.shortcodes.dynamic_partials(),
            )?)),
            "layout" => {
                let (name_part, hash) =
                    parse_partial_name_and_hash(args, self.shortcodes.dynamic_partials())?;
                let (body, _) = self.parse_until(&[])?;
                Node::Layout(name_part, hash, body)
            }
            "block" => {
                let block_name = args
                    .trim()
                    .trim_matches(|c| c == '"' || c == '\'')
                    .to_string();
                let (body, _, _) = self.expect_block("block", &["endblock"])?;
                Node::Block(block_name, body)
            }
            _ => {
                if self.shortcodes.is_paired_shortcode(name) {
                    let end_name = format!("end{name}");
                    let (body, _, _) = self.expect_block(name, &[&end_name])?;
                    Node::Shortcode(
                        Box::new(ShortcodeTag {
                            name: name.to_string(),
                            args: parse_shortcode_args(args)?,
                            body: Some(body),
                        }),
                        line,
                    )
                } else if self.shortcodes.is_shortcode(name) {
                    Node::Shortcode(
                        Box::new(ShortcodeTag {
                            name: name.to_string(),
                            args: parse_shortcode_args(args)?,
                            body: None,
                        }),
                        line,
                    )
                } else {
                    bail!("tag \"{name}\" not found");
                }
            }
        }))
    }
}

/* Expression parsing */

pub struct ExprParser {
    tokens: Vec<ExprToken>,
    pos: usize,
}

impl Iterator for ExprParser {
    type Item = ExprToken;

    fn next(&mut self) -> Option<ExprToken> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }
}

impl ExprParser {
    pub fn new(tokens: Vec<ExprToken>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn done(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    pub fn peek(&self) -> Option<&ExprToken> {
        self.tokens.get(self.pos)
    }

    pub fn peek_at(&self, offset: usize) -> Option<&ExprToken> {
        self.tokens.get(self.pos + offset)
    }

    pub fn eat(&mut self, t: &ExprToken) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub fn parse_value(&mut self) -> Result<Expr> {
        match self.next() {
            Some(ExprToken::Str(s)) => Ok(Expr::Literal(Value::str(s))),
            Some(ExprToken::Number(n)) => Ok(Expr::Literal(Value::Number(n))),
            Some(ExprToken::LParen) => {
                let start = self.parse_value()?;
                if !self.eat(&ExprToken::Range) {
                    bail!("expected range");
                }
                let end = self.parse_value()?;
                if !self.eat(&ExprToken::RParen) {
                    bail!("expected )");
                }
                Ok(Expr::Range(Box::new(start), Box::new(end)))
            }
            Some(ExprToken::LBracket) => {
                // `[key]` lookup on the root scope
                let key = self.parse_value()?;
                if !self.eat(&ExprToken::RBracket) {
                    bail!("expected ]");
                }
                let segs = self.parse_path_segments(vec![])?;
                match key {
                    Expr::Literal(Value::String(s)) => Ok(Expr::Path(s.to_string(), segs)),
                    other => {
                        let mut all = vec![PathSeg::Index(other)];
                        all.extend(segs);
                        Ok(Expr::Path(String::new(), all))
                    }
                }
            }
            Some(ExprToken::Ident(name)) => {
                match name.as_str() {
                    "true" => return Ok(Expr::Literal(Value::Bool(true))),
                    "false" => return Ok(Expr::Literal(Value::Bool(false))),
                    "nil" | "null" => return Ok(Expr::Literal(Value::Null)),
                    "empty" => return Ok(Expr::Empty),
                    "blank" => return Ok(Expr::Blank),
                    _ => {}
                }
                let segs = self.parse_path_segments(vec![])?;
                Ok(Expr::Path(name, segs))
            }
            other => bail!("unexpected token {other:?}"),
        }
    }

    fn parse_path_segments(&mut self, mut segs: Vec<PathSeg>) -> Result<Vec<PathSeg>> {
        loop {
            match self.peek() {
                Some(ExprToken::Dot) => {
                    self.pos += 1;
                    match self.next() {
                        Some(ExprToken::Ident(k)) => segs.push(PathSeg::Key(k)),
                        Some(ExprToken::Number(n)) => {
                            segs.push(PathSeg::Key(crate::value::format_number(n)))
                        }
                        other => bail!("unexpected token after '.': {other:?}"),
                    }
                }
                Some(ExprToken::LBracket) => {
                    self.pos += 1;
                    let key = self.parse_value()?;
                    if !self.eat(&ExprToken::RBracket) {
                        bail!("expected ]");
                    }
                    segs.push(PathSeg::Index(key));
                }
                _ => break,
            }
        }
        Ok(segs)
    }

    pub fn parse_filters(&mut self) -> Result<Vec<FilterCall>> {
        let mut filters = vec![];
        while self.eat(&ExprToken::Pipe) {
            let name = match self.next() {
                Some(ExprToken::Ident(n)) => n,
                other => bail!("expected filter name, found {other:?}"),
            };
            let mut args = vec![];
            if self.eat(&ExprToken::Colon) {
                loop {
                    args.push(self.parse_filter_arg()?);
                    if !self.eat(&ExprToken::Comma) {
                        break;
                    }
                }
            }
            filters.push(FilterCall { name, args });
        }
        Ok(filters)
    }

    pub fn parse_filter_arg(&mut self) -> Result<FilterArg> {
        if let (Some(ExprToken::Ident(k)), Some(ExprToken::Colon)) = (self.peek(), self.peek_at(1))
        {
            let k = k.clone();
            self.pos += 2;
            return Ok(FilterArg::Keyword(k, self.parse_value()?));
        }
        Ok(FilterArg::Positional(self.parse_value()?))
    }
}

pub fn parse_filtered(src: &str) -> Result<FilteredExpr> {
    let tokens = lex_expression(src)?;
    let mut p = ExprParser::new(tokens);
    if p.done() {
        return Ok(FilteredExpr {
            base: Expr::Literal(Value::Undefined),
            filters: vec![],
        });
    }
    let base = p.parse_value()?;
    let filters = p.parse_filters()?;
    if !p.done() {
        bail!("unexpected token {:?} in: {src}", p.peek());
    }
    Ok(FilteredExpr { base, filters })
}

pub fn parse_expr_str(src: &str) -> Result<Expr> {
    let mut p = ExprParser::new(lex_expression(src)?);
    p.parse_value()
}

fn parse_when_values(src: &str) -> Result<Vec<Expr>> {
    let mut p = ExprParser::new(lex_expression(src)?);
    let mut values = vec![];
    while !p.done() {
        values.push(p.parse_value()?);
        if !p.eat(&ExprToken::Comma) {
            if let Some(ExprToken::Ident(i)) = p.peek()
                && i == "or"
            {
                p.pos += 1;
                continue;
            }
            break;
        }
    }
    Ok(values)
}

/// Parse a condition with liquidjs operator precedence (comparisons > not > and/or)
pub fn parse_condition(src: &str) -> Result<Cond> {
    let tokens = lex_expression(src)?;
    let mut p = ExprParser::new(tokens);
    let cond = parse_or(&mut p)?;
    Ok(cond)
}

fn parse_or(p: &mut ExprParser) -> Result<Cond> {
    // and/or share precedence and are right-associative (as in Shopify Liquid)
    let left = parse_not(p)?;
    match p.peek() {
        Some(ExprToken::Ident(op)) if op == "and" => {
            p.pos += 1;
            Ok(Cond::And(Box::new(left), Box::new(parse_or(p)?)))
        }
        Some(ExprToken::Ident(op)) if op == "or" => {
            p.pos += 1;
            Ok(Cond::Or(Box::new(left), Box::new(parse_or(p)?)))
        }
        _ => Ok(left),
    }
}

fn parse_not(p: &mut ExprParser) -> Result<Cond> {
    if let Some(ExprToken::Ident(op)) = p.peek()
        && op == "not"
    {
        p.pos += 1;
        return Ok(Cond::Not(Box::new(parse_not(p)?)));
    }
    parse_comparison(p)
}

fn parse_comparison(p: &mut ExprParser) -> Result<Cond> {
    let left = p.parse_value()?;
    if let Some(ExprToken::Op(op)) = p.peek() {
        let op = op.clone();
        p.pos += 1;
        let right = p.parse_value()?;
        return Ok(Cond::Compare(op, left, right));
    }
    Ok(Cond::Value(left))
}

fn parse_for_args(args: &str, is_tablerow: bool) -> Result<(ForTag, Option<Expr>)> {
    let mut p = ExprParser::new(lex_expression(args)?);
    let var = match p.next() {
        Some(ExprToken::Ident(v)) => v,
        other => bail!("illegal for tag variable: {other:?}"),
    };
    match p.next() {
        Some(ExprToken::Ident(i)) if i == "in" => {}
        _ => bail!("expected `in` in for tag: {args}"),
    }
    let collection = p.parse_value()?;
    let mut tag = ForTag {
        var,
        collection,
        limit: None,
        offset: None,
        reversed: false,
        body: vec![],
        else_body: vec![],
    };
    let mut cols = None;
    while let Some(tok) = p.next() {
        match tok {
            ExprToken::Ident(i) if i == "reversed" => tag.reversed = true,
            ExprToken::Ident(i) if i == "limit" || i == "offset" || i == "cols" => {
                p.eat(&ExprToken::Colon);
                let v = p.parse_value()?;
                match i.as_str() {
                    "limit" => tag.limit = Some(v),
                    "offset" => tag.offset = Some(v),
                    _ => cols = Some(v),
                }
            }
            ExprToken::Comma => {}
            other => {
                if !is_tablerow {
                    bail!("unexpected token in for tag: {other:?}");
                }
            }
        }
    }
    Ok((tag, cols))
}

/// With `dynamicPartials: false` the file name is everything up to the first
/// whitespace or comma (quotes are stripped), as in liquidjs `readFileName`.
/// Returns the name and the remaining arguments.
fn split_static_file_name(args: &str) -> (String, &str) {
    let args = args.trim_start();
    let end = args
        .find(|c: char| c.is_whitespace() || c == ',')
        .unwrap_or(args.len());
    let name = args[..end].trim_matches(|c| c == '"' || c == '\'');
    (name.to_string(), &args[end..])
}

fn parse_partial_name_and_hash(
    args: &str,
    dynamic: bool,
) -> Result<(Option<PartialName>, Vec<(String, Expr)>)> {
    let args = args.trim();
    if args.is_empty() {
        return Ok((None, vec![]));
    }
    if !dynamic {
        let (name, rest) = split_static_file_name(args);
        let hash = parse_hash(&mut ExprParser::new(lex_expression(rest)?))?;
        let name = (name != "none").then_some(PartialName::Static(name));
        return Ok((name, hash));
    }
    let mut p = ExprParser::new(lex_expression(args)?);
    let name = match p.peek() {
        Some(ExprToken::Ident(n)) if n == "none" => {
            p.pos += 1;
            None
        }
        _ => Some(parse_partial_name(&mut p)?),
    };
    let hash = parse_hash(&mut p)?;
    Ok((name, hash))
}

fn parse_partial_name(p: &mut ExprParser) -> Result<PartialName> {
    match p.peek().cloned() {
        Some(ExprToken::Str(s)) => {
            p.pos += 1;
            Ok(PartialName::Static(s))
        }
        _ => Ok(PartialName::Dynamic(p.parse_value()?)),
    }
}

fn parse_hash(p: &mut ExprParser) -> Result<Vec<(String, Expr)>> {
    let mut hash = vec![];
    while !p.done() {
        p.eat(&ExprToken::Comma);
        match (p.peek().cloned(), p.peek_at(1).cloned()) {
            (Some(ExprToken::Ident(k)), Some(ExprToken::Colon))
            | (Some(ExprToken::Ident(k)), Some(ExprToken::Assign)) => {
                p.pos += 2;
                hash.push((k, p.parse_value()?));
            }
            (None, _) => break,
            (other, _) => bail!("unexpected token in hash arguments: {other:?}"),
        }
    }
    Ok(hash)
}

fn parse_include_args(args: &str, isolated: bool, dynamic: bool) -> Result<IncludeTag> {
    if !dynamic {
        let (name, rest) = split_static_file_name(args);
        let p = ExprParser::new(lex_expression(rest)?);
        return parse_include_options(PartialName::Static(name), p, isolated);
    }
    let tokens = lex_expression(args)?;
    let mut p = ExprParser::new(tokens);
    // Unquoted file names (Jekyll style) are treated as static paths
    let name = match p.peek().cloned() {
        Some(ExprToken::Str(s)) => {
            p.pos += 1;
            PartialName::Static(s)
        }
        _ => {
            // Could be a variable or an unquoted file name like header.liquid
            let raw: String = args
                .split(|c: char| c.is_whitespace() || c == ',')
                .next()
                .unwrap_or("")
                .to_string();
            if raw.contains('/') || raw.ends_with(".liquid") || raw.ends_with(".html") {
                // consume tokens of the raw name
                let consumed = lex_expression(&raw).map(|t| t.len()).unwrap_or(1);
                p.pos += consumed;
                PartialName::Static(raw)
            } else {
                PartialName::Dynamic(p.parse_value()?)
            }
        }
    };
    parse_include_options(name, p, isolated)
}

/// The `with`/`for`/hash arguments following an include/render file name
fn parse_include_options(
    name: PartialName,
    mut p: ExprParser,
    isolated: bool,
) -> Result<IncludeTag> {
    let mut tag = IncludeTag {
        name,
        with: None,
        for_loop: None,
        hash: vec![],
        isolated,
    };
    loop {
        match p.peek().cloned() {
            Some(ExprToken::Ident(k))
                if (k == "with" || k == "for") && p.peek_at(1) != Some(&ExprToken::Colon) =>
            {
                p.pos += 1;
                let value = p.parse_value()?;
                let mut alias = None;
                if let Some(ExprToken::Ident(a)) = p.peek()
                    && a == "as"
                {
                    p.pos += 1;
                    if let Some(ExprToken::Ident(n)) = p.next() {
                        alias = Some(n);
                    }
                }
                if k == "with" {
                    tag.with = Some((value, alias));
                } else {
                    tag.for_loop = Some((value, alias));
                }
            }
            Some(ExprToken::Comma) => {
                p.pos += 1;
            }
            None => break,
            _ => {
                tag.hash = parse_hash(&mut p)?;
                break;
            }
        }
    }
    Ok(tag)
}

#[derive(Debug, Clone)]
pub enum ShortcodeArg {
    Positional(FilteredExpr),
    Keyword(String, Expr),
}

/// Shortcode arguments: values separated by commas/whitespace, `key: value` pairs become keyword args.
pub fn parse_shortcode_args(args: &str) -> Result<Vec<ShortcodeArg>> {
    let mut p = ExprParser::new(lex_expression(args)?);
    let mut out = vec![];
    while !p.done() {
        if p.eat(&ExprToken::Comma) {
            continue;
        }
        match p.parse_filter_arg()? {
            FilterArg::Positional(base) => {
                let filters = if p.peek() == Some(&ExprToken::Pipe) {
                    p.parse_filters()?
                } else {
                    vec![]
                };
                out.push(ShortcodeArg::Positional(FilteredExpr { base, filters }));
            }
            FilterArg::Keyword(k, v) => out.push(ShortcodeArg::Keyword(k, v)),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let t = parse_template(
            "{% if a == 1 and b %}x{% elsif c %}y{% else %}z{% endif %}{{ a | plus: 1 }}",
            None,
            &NoShortcodes,
        )
        .unwrap();
        assert_eq!(t.nodes.len(), 2);
    }

    #[test]
    fn for_args() {
        let t = parse_template(
            "{% for x in (1..5) limit:2 reversed %}{{x}}{% else %}none{% endfor %}",
            None,
            &NoShortcodes,
        )
        .unwrap();
        assert!(matches!(t.nodes[0], Node::For(_)));
    }

    #[test]
    fn static_partial_names() {
        let tag =
            parse_include_args("./dir/included.html, myVariable: 'myValue'", false, false).unwrap();
        assert!(matches!(&tag.name, PartialName::Static(n) if n == "./dir/included.html"));
        assert_eq!(tag.hash.len(), 1);
        let tag = parse_include_args("included", false, false).unwrap();
        assert!(matches!(&tag.name, PartialName::Static(n) if n == "included"));
        let tag = parse_include_args("included", false, true).unwrap();
        assert!(matches!(tag.name, PartialName::Dynamic(_)));
    }
}
