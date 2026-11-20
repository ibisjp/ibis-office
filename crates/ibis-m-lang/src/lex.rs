// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The lexer of M: identifiers (with dots, `#"quoted"`), keywords, numbers, text with `#(lf)` escapes, operators, comments.
use crate::error::MError;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Kw(&'static str),
    Num(f64),
    Text(String),
    /// An operator or punctuation: + - * / & = <> < > <= >= => ( ) [ ] { } , .. ... ? ?? @
    Op(&'static str),
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    /// 1-based line of the first character.
    pub line: u32,
}

const KEYWORDS: &[&str] = &[
    "and", "as", "each", "else", "error", "false", "if", "in", "is", "let", "meta", "not", "null", "nullable", "optional", "or", "otherwise", "section", "shared", "then", "true", "try", "type", "catch",
];
const OPS: &[&str] = &["=>", "<>", "<=", ">=", "??", "...", "..", "+", "-", "*", "/", "&", "=", "<", ">", "(", ")", "[", "]", "{", "}", ",", "?", "@", ";", "!"];

fn is_id_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}
fn is_id_part(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

pub fn lex(src: &str) -> Result<Vec<Token>, MError> {
    let cs: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut line = 1u32;
    let mut out = Vec::new();
    while i < cs.len() {
        let c = cs[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() || c == '\u{feff}' {
            i += 1;
        } else if c == '/' && cs.get(i + 1) == Some(&'/') {
            while i < cs.len() && cs[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && cs.get(i + 1) == Some(&'*') {
            let start = line;
            i += 2;
            loop {
                if i + 1 >= cs.len() {
                    return Err(MError::syntax(start, "A comment is not closed."));
                }
                if cs[i] == '*' && cs[i + 1] == '/' {
                    i += 2;
                    break;
                }
                if cs[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
        } else if c == '"' {
            let start = line;
            let (s, ni, nl) = lex_text(&cs, i + 1, line)?;
            out.push(Token { tok: Tok::Text(s), line: start });
            i = ni;
            line = nl;
        } else if c == '#' {
            if cs.get(i + 1) == Some(&'"') {
                let start = line;
                let (s, ni, nl) = lex_text(&cs, i + 2, line)?;
                out.push(Token { tok: Tok::Ident(s), line: start });
                i = ni;
                line = nl;
            } else {
                let mut j = i + 1;
                while j < cs.len() && is_id_part(cs[j]) {
                    j += 1;
                }
                let w: String = cs[i..j].iter().collect();
                let tok = match w.as_str() {
                    "#nan" => Tok::Num(f64::NAN),
                    "#infinity" => Tok::Num(f64::INFINITY),
                    _ => Tok::Ident(w),
                };
                out.push(Token { tok, line });
                i = j;
            }
        } else if c.is_ascii_digit() || (c == '.' && cs.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let (n, ni) = lex_number(&cs, i, line)?;
            out.push(Token { tok: Tok::Num(n), line });
            i = ni;
        } else if is_id_start(c) {
            let mut j = i + 1;
            loop {
                if (j < cs.len() && is_id_part(cs[j])) || (j + 1 < cs.len() && cs[j] == '.' && is_id_start(cs[j + 1])) {
                    j += 1;
                } else {
                    break;
                }
            }
            let w: String = cs[i..j].iter().collect();
            let tok = match KEYWORDS.iter().find(|k| **k == w) {
                Some(k) => Tok::Kw(k),
                None => Tok::Ident(w),
            };
            out.push(Token { tok, line });
            i = j;
        } else {
            match OPS.iter().find(|op| op.chars().enumerate().all(|(k, oc)| cs.get(i + k) == Some(&oc))) {
                Some(op) => {
                    out.push(Token { tok: Tok::Op(op), line });
                    i += op.chars().count();
                }
                None => return Err(MError::syntax(line, &format!("The character '{c}' is not valid."))),
            }
        }
    }
    out.push(Token { tok: Tok::Eof, line });
    Ok(out)
}

fn lex_text(cs: &[char], mut i: usize, mut line: u32) -> Result<(String, usize, u32), MError> {
    let start = line;
    let mut s = String::new();
    loop {
        match cs.get(i) {
            None => return Err(MError::syntax(start, "A text literal is not closed.")),
            Some('"') => {
                if cs.get(i + 1) == Some(&'"') {
                    s.push('"');
                    i += 2;
                } else {
                    return Ok((s, i + 1, line));
                }
            }
            Some('#') if cs.get(i + 1) == Some(&'(') => {
                let mut j = i + 2;
                while j < cs.len() && cs[j] != ')' {
                    j += 1;
                }
                if j >= cs.len() {
                    return Err(MError::syntax(line, "An escape sequence is not closed."));
                }
                let body: String = cs[i + 2..j].iter().collect();
                // #(lf,cr) or #(0041) or #(#): a comma-separated list.
                for part in body.split(',') {
                    let p = part.trim();
                    match p {
                        "lf" => s.push('\n'),
                        "cr" => s.push('\r'),
                        "tab" => s.push('\t'),
                        "#" => s.push('#'),
                        _ => match u32::from_str_radix(p, 16).ok().and_then(char::from_u32) {
                            Some(ch) => s.push(ch),
                            None => return Err(MError::syntax(line, &format!("The escape sequence '#({p})' is not valid."))),
                        },
                    }
                }
                i = j + 1;
            }
            Some(&c) => {
                if c == '\n' {
                    line += 1;
                }
                s.push(c);
                i += 1;
            }
        }
    }
}

fn lex_number(cs: &[char], start: usize, line: u32) -> Result<(f64, usize), MError> {
    let mut i = start;
    if cs[i] == '0' && matches!(cs.get(i + 1), Some('x') | Some('X')) {
        let mut j = i + 2;
        while j < cs.len() && cs[j].is_ascii_hexdigit() {
            j += 1;
        }
        let h: String = cs[i + 2..j].iter().collect();
        return u64::from_str_radix(&h, 16).map(|v| (v as f64, j)).map_err(|_| MError::syntax(line, "A hexadecimal number is not valid."));
    }
    while i < cs.len() && cs[i].is_ascii_digit() {
        i += 1;
    }
    if cs.get(i) == Some(&'.') && cs.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
        i += 1;
        while i < cs.len() && cs[i].is_ascii_digit() {
            i += 1;
        }
    }
    if matches!(cs.get(i), Some('e') | Some('E')) {
        let mut j = i + 1;
        if matches!(cs.get(j), Some('+') | Some('-')) {
            j += 1;
        }
        if cs.get(j).is_some_and(|d| d.is_ascii_digit()) {
            while j < cs.len() && cs[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    let t: String = cs[start..i].iter().collect();
    t.parse::<f64>().map(|v| (v, i)).map_err(|_| MError::syntax(line, &format!("The number '{t}' is not valid.")))
}
