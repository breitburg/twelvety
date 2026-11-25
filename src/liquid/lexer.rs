//! Tokenizer for Liquid templates and the expressions inside tags/outputs.

use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Text(String),
    Output {
        content: String,
        line: usize,
    },
    Tag {
        name: String,
        args: String,
        line: usize,
    },
    /// Raw content of `{% raw %}…{% endraw %}`
    Raw(String),
}

/// Split template source into text/output/tag tokens, applying whitespace control.
pub fn tokenize(src: &str) -> Result<Vec<Token>> {
    tokenize_with(src, &|_| false)
}

/// Tokenize; tags for which `is_raw_block` returns true keep their body unparsed
/// (e.g. the Render plugin’s `renderTemplate`).
pub fn tokenize_with(src: &str, is_raw_block: &dyn Fn(&str) -> bool) -> Result<Vec<Token>> {
    let mut tokens: Vec<Token> = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut text_start = 0;
    let mut trim_next_left = false;
    let mut line = 1;

    let push_text = |tokens: &mut Vec<Token>, text: &str, trim_left: bool, trim_right: bool| {
        let mut t = text;
        if trim_left {
            t = t.trim_start();
        }
        if trim_right {
            t = t.trim_end();
        }
        if !t.is_empty() {
            tokens.push(Token::Text(t.to_string()));
        }
    };

    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && (bytes[i + 1] == b'{' || bytes[i + 1] == b'%')
        {
            let is_output = bytes[i + 1] == b'{';
            let open_end = i + 2;
            let trim_left = open_end < bytes.len() && bytes[open_end] == b'-';
            let content_start = if trim_left { open_end + 1 } else { open_end };
            let close = if is_output { "}}" } else { "%}" };
            // find the closing delimiter, skipping quoted strings
            let mut j = content_start;
            let mut quote: Option<u8> = None;
            let mut found = None;
            while j < bytes.len() {
                let c = bytes[j];
                if let Some(q) = quote {
                    if c == q {
                        quote = None;
                    }
                } else if c == b'"' || c == b'\'' {
                    quote = Some(c);
                } else if src[j..].starts_with(close) {
                    found = Some(j);
                    break;
                }
                j += 1;
            }
            let Some(close_pos) = found else {
                bail!(
                    "{} \"{}\" not closed, line:{}",
                    if is_output { "output" } else { "tag" },
                    &src[i..(i + 20).min(src.len())],
                    line
                );
            };
            let trim_right = close_pos > content_start && bytes[close_pos - 1] == b'-';
            let content_end = if trim_right { close_pos - 1 } else { close_pos };
            // text before this token
            push_text(&mut tokens, &src[text_start..i], trim_next_left, trim_left);
            let token_line = line + src[text_start..i].matches('\n').count();
            let content = src[content_start..content_end.max(content_start)].trim();
            let after = close_pos + 2;
            line = token_line + src[i..after].matches('\n').count();
            if is_output {
                tokens.push(Token::Output {
                    content: content.to_string(),
                    line: token_line,
                });
                i = after;
                text_start = i;
                trim_next_left = trim_right;
            } else {
                let (name, args) = split_tag(content);
                let is_raw = name == "raw";
                if is_raw || is_raw_block(&name) {
                    // capture until {% endraw %} (or {% end<name> %})
                    let end_name = format!("end{name}");
                    let (raw, end, endraw_trim_left, endraw_trim_right) =
                        find_end_raw(src, after, token_line, &end_name)?;
                    let mut raw_text = raw.as_str();
                    if trim_right {
                        raw_text = raw_text.trim_start();
                    }
                    if endraw_trim_left {
                        raw_text = raw_text.trim_end();
                    }
                    if !is_raw {
                        tokens.push(Token::Tag {
                            name: name.clone(),
                            args,
                            line: token_line,
                        });
                    }
                    tokens.push(Token::Raw(raw_text.to_string()));
                    if !is_raw {
                        tokens.push(Token::Tag {
                            name: end_name,
                            args: String::new(),
                            line: token_line,
                        });
                    }
                    line += src[after..end].matches('\n').count();
                    i = end;
                    text_start = i;
                    trim_next_left = endraw_trim_right;
                } else {
                    tokens.push(Token::Tag {
                        name,
                        args,
                        line: token_line,
                    });
                    i = after;
                    text_start = i;
                    trim_next_left = trim_right;
                }
            }
            continue;
        }
        i += 1;
    }
    push_text(&mut tokens, &src[text_start..], trim_next_left, false);
    Ok(tokens)
}

fn split_tag(content: &str) -> (String, String) {
    let content = content.trim();
    if let Some(rest) = content.strip_prefix('#') {
        return ("#".into(), rest.trim().to_string());
    }
    let end = content
        .find(|c: char| c.is_whitespace())
        .unwrap_or(content.len());
    (
        content[..end].to_string(),
        content[end..].trim().to_string(),
    )
}

/// Find `{% endraw %}` starting at `start`; returns (raw text, index after endraw, trim flags)
fn find_end_raw(
    src: &str,
    start: usize,
    line: usize,
    end_name: &str,
) -> Result<(String, usize, bool, bool)> {
    let mut search = start;
    while let Some(pos) = src[search..].find("{%") {
        let abs = search + pos;
        let rest = &src[abs + 2..];
        let trim_left = rest.starts_with('-');
        let inner_start = if trim_left { 1 } else { 0 };
        if let Some(close) = rest.find("%}") {
            let inner = rest[inner_start..close].trim_end_matches('-').trim();
            if inner == end_name {
                let trim_right = rest[..close].ends_with('-');
                return Ok((
                    src[start..abs].to_string(),
                    abs + 2 + close + 2,
                    trim_left,
                    trim_right,
                ));
            }
        }
        search = abs + 2;
    }
    bail!("tag {{% {} %}} not closed, line:{line}", &end_name[3..])
}

/* Expression lexer */

#[derive(Debug, Clone, PartialEq)]
pub enum ExprToken {
    Ident(String),
    Number(f64),
    Str(String),
    Dot,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Range,
    Pipe,
    Colon,
    Comma,
    Op(String),
    Assign,
}

pub fn lex_expression(src: &str) -> Result<Vec<ExprToken>> {
    let chars: Vec<char> = src.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => {
                let quote = c;
                let mut s = String::new();
                i += 1;
                while i < chars.len() && chars[i] != quote {
                    s.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    bail!("unterminated string in: {src}");
                }
                i += 1;
                tokens.push(ExprToken::Str(s));
            }
            '.' => {
                if i + 1 < chars.len() && chars[i + 1] == '.' {
                    tokens.push(ExprToken::Range);
                    i += 2;
                } else {
                    tokens.push(ExprToken::Dot);
                    i += 1;
                }
            }
            '[' => {
                tokens.push(ExprToken::LBracket);
                i += 1;
            }
            ']' => {
                tokens.push(ExprToken::RBracket);
                i += 1;
            }
            '(' => {
                tokens.push(ExprToken::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(ExprToken::RParen);
                i += 1;
            }
            '|' => {
                tokens.push(ExprToken::Pipe);
                i += 1;
            }
            ':' => {
                tokens.push(ExprToken::Colon);
                i += 1;
            }
            ',' => {
                tokens.push(ExprToken::Comma);
                i += 1;
            }
            '=' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(ExprToken::Op("==".into()));
                    i += 2;
                } else {
                    tokens.push(ExprToken::Assign);
                    i += 1;
                }
            }
            '!' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(ExprToken::Op("!=".into()));
                    i += 2;
                } else {
                    bail!("unexpected character '!' in: {src}");
                }
            }
            '<' | '>' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(ExprToken::Op(format!("{c}=")));
                    i += 2;
                } else if c == '<' && i + 1 < chars.len() && chars[i + 1] == '>' {
                    tokens.push(ExprToken::Op("!=".into()));
                    i += 2;
                } else {
                    tokens.push(ExprToken::Op(c.to_string()));
                    i += 1;
                }
            }
            _ if c.is_ascii_digit()
                || (c == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) =>
            {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                // identifiers can start with digits in liquid (e.g. `posts.2021-review`)
                if i < chars.len() && (chars[i].is_alphabetic() || chars[i] == '_') {
                    while i < chars.len() && is_ident_char(chars[i]) {
                        i += 1;
                    }
                    let s: String = chars[start..i].iter().collect();
                    tokens.push(ExprToken::Ident(s));
                    continue;
                }
                let s: String = chars[start..i].iter().collect();
                tokens.push(ExprToken::Number(s.parse().unwrap_or(0.0)));
            }
            _ if c.is_alphabetic() || c == '_' || c == '$' || c == '@' => {
                let start = i;
                while i < chars.len() && is_ident_char(chars[i]) {
                    i += 1;
                }
                // trailing `?` allowed in identifiers
                if i < chars.len() && chars[i] == '?' {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                if s == "contains" {
                    tokens.push(ExprToken::Op(s));
                } else {
                    tokens.push(ExprToken::Ident(s));
                }
            }
            _ => bail!("unexpected character '{c}' in: {src}"),
        }
    }
    Ok(tokens)
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '$' || c == '@'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_control() {
        let t = tokenize("a  {%- if x -%}  b  {%- endif %}").unwrap();
        assert_eq!(t[0], Token::Text("a".into()));
        assert_eq!(t[2], Token::Text("b".into()));
    }

    #[test]
    fn raw() {
        let t = tokenize("{% raw %}{{ x }}{% endraw %}").unwrap();
        assert_eq!(t[0], Token::Raw("{{ x }}".into()));
    }
}
