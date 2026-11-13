// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Splitting a formula into tokens.
//!
//! Formulas are in the form a file stores them (A1 references, `,` between
//! arguments, `.` for decimals), without the leading `=`. Each token keeps
//! its place in the text so that a formula can be rewritten piece by piece
//! (references moved when rows are inserted, prefixes added for the file)
//! without touching the rest of what was typed.

use crate::value::CellError;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Number(f64),
    Text(String),
    Error(CellError),
    /// A name, a cell reference, a function name (when `(` follows), `TRUE`
    /// or `FALSE`, or a part of a range such as `A` of `A:C` or `$1` of
    /// `$1:$3`: classified by the parser.
    Word(String),
    /// `Sheet1!`, `'My sheet'!`, `Sheet1:Sheet3!`, `[1]Sheet1!`, `'C:\dir\[Book.xlsx]Sheet1'!`: the sheet
    /// names (first and last), the external workbook, and directory path prefix, unquoted.
    Sheet { first: String, last: Option<String>, book: Option<String>, path: Option<String> },
    /// A structured reference's brackets after a table name, or on their
    /// own (`[@Price]`), as written, with the brackets.
    Brackets(String),
    Op(Op),
    LParen,
    RParen,
    LBrace,
    RBrace,
    /// `,`: between arguments, array columns, or areas of a union.
    Comma,
    /// `;`: between the rows of an array constant.
    Semicolon,
    /// `#` after a reference (the spill range of `A1#`).
    Spill,
    /// `@` before an expression (implicit intersection).
    At,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Percent,
    Range,
    /// `:.` (trailing blanks trimmed), `.:` (leading) and `.:.` (both): TRIMRANGE's operators.
    RangeTrimEnd,
    RangeTrimStart,
    RangeTrimBoth,
    /// A space between two references.
    Intersect,
}

impl Op {
    /// `:` and the trim operators (`:.` `.:` `.:.`), which join two references.
    pub fn is_range(self) -> bool {
        matches!(self, Op::Range | Op::RangeTrimEnd | Op::RangeTrimStart | Op::RangeTrimBoth)
    }
}

/// A token and where it is in the formula (byte offsets).
#[derive(Clone, Debug, PartialEq)]
pub struct Spanned {
    pub token: Token,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    pub at: usize,
    pub message: &'static str,
}

/// Characters of a name (and of a cell reference, which looks like one).
pub(crate) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '?' | '$') || (c as u32 > 0x7f && !c.is_whitespace() && !is_fullwidth_operator(c))
}

fn is_fullwidth_operator(c: char) -> bool {
    matches!(c, '\u{3000}')
}

fn is_word_start(c: char) -> bool {
    c.is_alphabetic() || matches!(c, '_' | '\\' | '$') || (c as u32 > 0x7f && !c.is_whitespace() && !is_fullwidth_operator(c))
}

pub fn tokenize(text: &str) -> Result<Vec<Spanned>, LexError> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let len = text.len();
    let at = |i: usize| chars.get(i).map(|c| c.1);
    let pos = |i: usize| chars.get(i).map_or(len, |c| c.0);
    let mut out: Vec<Spanned> = Vec::new();
    let mut i = 0;
    // Whitespace since the last token, which may be the intersection operator.
    let mut space_before = false;
    while i < chars.len() {
        let c = chars[i].1;
        let start = chars[i].0;
        if c == ' ' || c == '\n' || c == '\r' || c == '\t' || c == '\u{3000}' {
            space_before = true;
            i += 1;
            continue;
        }
        let had_space = std::mem::take(&mut space_before);
        let push = |out: &mut Vec<Spanned>, token: Token, end_index: usize| {
            out.push(Spanned { token, start, end: pos(end_index) });
        };
        // A space between the end of one operand and the start of another
        // is the intersection operator.
        if had_space && ends_operand(out.last().map(|s| &s.token)) && starts_operand(c) {
            out.push(Spanned { token: Token::Op(Op::Intersect), start: pos(i - 1), end: pos(i) });
        }
        match c {
            '"' => {
                let mut s = String::new();
                let mut j = i + 1;
                loop {
                    match at(j) {
                        None => return Err(LexError { at: start, message: "文字列の \" が閉じていません" }),
                        Some('"') if at(j + 1) == Some('"') => {
                            s.push('"');
                            j += 2;
                        }
                        Some('"') => break,
                        Some(ch) => {
                            s.push(ch);
                            j += 1;
                        }
                    }
                }
                push(&mut out, Token::Text(s), j + 1);
                i = j + 1;
            }
            '\'' => {
                // A quoted sheet name, then `!` (or `:'Other'!` for 3D).
                let (name, j) = quoted(&chars, i).ok_or(LexError { at: start, message: "シート名の ' が閉じていません" })?;
                let (book, path, sheet) = split_book_path(&name);
                if at(j) == Some('!') {
                    let (first, last) = if let Some((f, l)) = sheet.split_once(':') {
                        (f.to_string(), Some(l.to_string()))
                    } else {
                        (sheet, None)
                    };
                    push(&mut out, Token::Sheet { first, last, book, path }, j + 1);
                    i = j + 1;
                } else if at(j) == Some(':') {
                    // 'A':'B'! or 'A':B!
                    let (last, next_idx) = if at(j + 1) == Some('\'') {
                        let (last_name, k) = quoted(&chars, j + 1).ok_or(LexError { at: start, message: "シート名の ' が閉じていません" })?;
                        (last_name, k)
                    } else {
                        let mut k = j + 1;
                        let word_start = k;
                        while at(k).is_some_and(is_word_char) {
                            k += 1;
                        }
                        if k > word_start {
                            (chars[word_start..k].iter().map(|c| c.1).collect(), k)
                        } else {
                            return Err(LexError { at: start, message: "シート名の後に ! がありません" });
                        }
                    };
                    if at(next_idx) == Some('!') {
                        let (_, _, last) = split_book_path(&last);
                        push(&mut out, Token::Sheet { first: sheet, last: Some(last), book, path }, next_idx + 1);
                        i = next_idx + 1;
                    } else {
                        return Err(LexError { at: start, message: "シート名の後に ! がありません" });
                    }
                } else {
                    return Err(LexError { at: start, message: "シート名の後に ! がありません" });
                }
            }
            '#' => {
                // An error value, or the spill operator after a reference.
                let rest = &text[start..];
                if let Some(e) = CellError::ALL.iter().copied().filter(|e| rest.as_bytes().get(..e.as_str().len()).is_some_and(|p| p.eq_ignore_ascii_case(e.as_str().as_bytes()))).max_by_key(|e| e.as_str().len()) {
                    let n = e.as_str().chars().count();
                    push(&mut out, Token::Error(e), i + n);
                    i += n;
                } else if ends_operand(out.last().map(|s| &s.token)) && !had_space {
                    push(&mut out, Token::Spill, i + 1);
                    i += 1;
                } else {
                    return Err(LexError { at: start, message: "# の使い方が正しくありません" });
                }
            }
            '[' => {
                // An external workbook (`[1]Sheet1!A1`) or a structured reference.
                let j = brackets_end(&chars, i).ok_or(LexError { at: start, message: "[ が閉じていません" })?;
                let inner: String = chars[i + 1..j].iter().map(|c| c.1).collect();
                // `[1]Sheet!` / `[Book.xlsx]Sheet!`
                let mut k = j + 1;
                let word_start = k;
                while at(k).is_some_and(is_word_char) {
                    k += 1;
                }
                if k > word_start && at(k) == Some('!') {
                    let sheet: String = chars[word_start..k].iter().map(|c| c.1).collect();
                    push(&mut out, Token::Sheet { first: sheet, last: None, book: Some(inner), path: None }, k + 1);
                    i = k + 1;
                } else if k > word_start && at(k) == Some(':') {
                    // [1]A:B! 3D in another book.
                    let mut m = k + 1;
                    let s2 = m;
                    while at(m).is_some_and(is_word_char) {
                        m += 1;
                    }
                    if m > s2 && at(m) == Some('!') {
                        let first: String = chars[word_start..k].iter().map(|c| c.1).collect();
                        let last: String = chars[s2..m].iter().map(|c| c.1).collect();
                        push(&mut out, Token::Sheet { first, last: Some(last), book: Some(inner), path: None }, m + 1);
                        i = m + 1;
                    } else {
                        let raw: String = chars[i..=j].iter().map(|c| c.1).collect();
                        push(&mut out, Token::Brackets(raw), j + 1);
                        i = j + 1;
                    }
                } else if k == word_start && at(k) == Some('\'') {
                    if let Some((sheet_name, next_q)) = quoted(&chars, k) && at(next_q) == Some('!') {
                        let (first, last) = if let Some((f, l)) = sheet_name.split_once(':') {
                            (f.to_string(), Some(l.to_string()))
                        } else {
                            (sheet_name, None)
                        };
                        push(&mut out, Token::Sheet { first, last, book: Some(inner), path: None }, next_q + 1);
                        i = next_q + 1;
                    } else {
                        let raw: String = chars[i..=j].iter().map(|c| c.1).collect();
                        push(&mut out, Token::Brackets(raw), j + 1);
                        i = j + 1;
                    }
                } else {
                    let raw: String = chars[i..=j].iter().map(|c| c.1).collect();
                    push(&mut out, Token::Brackets(raw), j + 1);
                    i = j + 1;
                }
            }
            '0'..='9' | '.' if c != '.' || at(i + 1).is_some_and(|d| d.is_ascii_digit()) => {
                // A number, unless digits run into letters (a name such as
                // `1st`? not allowed) or form a row range (`1:3` is read by
                // the parser from two numbers).
                let mut j = i;
                while at(j).is_some_and(|d| d.is_ascii_digit()) {
                    j += 1;
                }
                if at(j) == Some('.') {
                    j += 1;
                    while at(j).is_some_and(|d| d.is_ascii_digit()) {
                        j += 1;
                    }
                }
                if matches!(at(j), Some('e' | 'E')) {
                    let mut k = j + 1;
                    if matches!(at(k), Some('+' | '-')) {
                        k += 1;
                    }
                    if at(k).is_some_and(|d| d.is_ascii_digit()) {
                        while at(k).is_some_and(|d| d.is_ascii_digit()) {
                            k += 1;
                        }
                        j = k;
                    }
                }
                // Digits followed by letters: a name like `1Q`? Excel names
                // cannot start with a digit, but a sheet can (`2024!A1`).
                if at(j).is_some_and(|d| is_word_char(d) && d != '.') {
                    let mut k = j;
                    while at(k).is_some_and(is_word_char) {
                        k += 1;
                    }
                    if at(k) == Some('!') {
                        let name: String = chars[i..k].iter().map(|c| c.1).collect();
                        push(&mut out, Token::Sheet { first: name, last: None, book: None, path: None }, k + 1);
                        i = k + 1;
                        continue;
                    }
                    // A 3D range of sheets named with digits, e.g. 2023:2024!A1.
                    return Err(LexError { at: start, message: "数値の後に文字があります" });
                }
                let s: String = chars[i..j].iter().map(|c| c.1).collect();
                // A sheet named with digits only, before `!` (`2024!A1`) or `:2025!`.
                if at(j) == Some('!') {
                    push(&mut out, Token::Sheet { first: s, last: None, book: None, path: None }, j + 1);
                    i = j + 1;
                    continue;
                }
                if at(j) == Some(':') && let Some((last, k)) = sheet_after_colon(&chars, j) {
                    push(&mut out, Token::Sheet { first: s, last: Some(last), book: None, path: None }, k + 1);
                    i = k + 1;
                    continue;
                }
                let truncated = crate::eval::convert::truncate_to_15_digits(&s);
                let n: f64 = truncated.parse().map_err(|_| LexError { at: start, message: "数値が正しくありません" })?;
                push(&mut out, Token::Number(n), j);
                i = j;
            }
            c if is_word_start(c) || c.is_ascii_digit() => {
                let mut j = i;
                while at(j).is_some_and(is_word_char) {
                    j += 1;
                }
                let word: String = chars[i..j].iter().map(|c| c.1).collect();
                if at(j) == Some(':') && word.len() > 1 && word.ends_with('.') && !word.ends_with("..") {
                    // `D.:D` and `D.:.D`: the dot before the colon is the trim operator's, not the name's.
                    let both = at(j + 1) == Some('.') && !at(j + 2).is_some_and(|d| d.is_ascii_digit());
                    out.push(Spanned { token: Token::Word(word[..word.len() - 1].to_string()), start, end: pos(j - 1) });
                    let (op, next) = if both { (Op::RangeTrimBoth, j + 2) } else { (Op::RangeTrimStart, j + 1) };
                    out.push(Spanned { token: Token::Op(op), start: pos(j - 1), end: pos(next) });
                    i = next;
                } else if at(j) == Some('!') {
                    push(&mut out, Token::Sheet { first: word, last: None, book: None, path: None }, j + 1);
                    i = j + 1;
                } else if at(j) == Some(':') && let Some((last, k)) = sheet_after_colon(&chars, j) {
                    push(&mut out, Token::Sheet { first: word, last: Some(last), book: None, path: None }, k + 1);
                    i = k + 1;
                } else if at(j) == Some('[') {
                    // A table's structured reference: Table1[Column].
                    let k = brackets_end(&chars, j).ok_or(LexError { at: start, message: "[ が閉じていません" })?;
                    push(&mut out, Token::Word(word), j);
                    let raw: String = chars[j..=k].iter().map(|c| c.1).collect();
                    out.push(Spanned { token: Token::Brackets(raw), start: pos(j), end: pos(k + 1) });
                    i = k + 1;
                } else {
                    push(&mut out, Token::Word(word), j);
                    i = j;
                }
            }
            '+' => {
                push(&mut out, Token::Op(Op::Add), i + 1);
                i += 1;
            }
            '-' => {
                push(&mut out, Token::Op(Op::Sub), i + 1);
                i += 1;
            }
            '*' => {
                push(&mut out, Token::Op(Op::Mul), i + 1);
                i += 1;
            }
            '/' => {
                push(&mut out, Token::Op(Op::Div), i + 1);
                i += 1;
            }
            '^' => {
                push(&mut out, Token::Op(Op::Pow), i + 1);
                i += 1;
            }
            '&' => {
                push(&mut out, Token::Op(Op::Concat), i + 1);
                i += 1;
            }
            '%' => {
                push(&mut out, Token::Op(Op::Percent), i + 1);
                i += 1;
            }
            ':' => {
                // `:.` trims the trailing blanks; not `:.5` (a number).
                if at(i + 1) == Some('.') && !at(i + 2).is_some_and(|d| d.is_ascii_digit()) {
                    push(&mut out, Token::Op(Op::RangeTrimEnd), i + 2);
                    i += 2;
                } else {
                    push(&mut out, Token::Op(Op::Range), i + 1);
                    i += 1;
                }
            }
            '=' => {
                push(&mut out, Token::Op(Op::Eq), i + 1);
                i += 1;
            }
            '<' => match at(i + 1) {
                Some('=') => {
                    push(&mut out, Token::Op(Op::Le), i + 2);
                    i += 2;
                }
                Some('>') => {
                    push(&mut out, Token::Op(Op::Ne), i + 2);
                    i += 2;
                }
                _ => {
                    push(&mut out, Token::Op(Op::Lt), i + 1);
                    i += 1;
                }
            },
            '>' => {
                if at(i + 1) == Some('=') {
                    push(&mut out, Token::Op(Op::Ge), i + 2);
                    i += 2;
                } else {
                    push(&mut out, Token::Op(Op::Gt), i + 1);
                    i += 1;
                }
            }
            '(' => {
                push(&mut out, Token::LParen, i + 1);
                i += 1;
            }
            ')' => {
                push(&mut out, Token::RParen, i + 1);
                i += 1;
            }
            '{' => {
                push(&mut out, Token::LBrace, i + 1);
                i += 1;
            }
            '}' => {
                push(&mut out, Token::RBrace, i + 1);
                i += 1;
            }
            ',' => {
                push(&mut out, Token::Comma, i + 1);
                i += 1;
            }
            ';' => {
                push(&mut out, Token::Semicolon, i + 1);
                i += 1;
            }
            '@' => {
                push(&mut out, Token::At, i + 1);
                i += 1;
            }
            _ => return Err(LexError { at: start, message: "使えない文字があります" }),
        }
    }
    Ok(out)
}

/// `'…'` from `i`: the name (`''` for a quote) and the index after it.
fn quoted(chars: &[(usize, char)], i: usize) -> Option<(String, usize)> {
    let mut s = String::new();
    let mut j = i + 1;
    loop {
        match chars.get(j)?.1 {
            '\'' if chars.get(j + 1).map(|c| c.1) == Some('\'') => {
                s.push('\'');
                j += 2;
            }
            '\'' => return Some((s, j + 1)),
            ch => {
                s.push(ch);
                j += 1;
            }
        }
    }
}

/// `[Book]Sheet` or `dir\[Book]Sheet` in a quoted name: the book, optional path prefix, and the sheet.
fn split_book_path(name: &str) -> (Option<String>, Option<String>, String) {
    if let Some(open) = name.find('[')
        && let Some(close_rel) = name[open..].find(']')
    {
        let close = open + close_rel;
        let path_prefix = &name[..open];
        let book = &name[open + 1..close];
        let sheet = &name[close + 1..];
        let path = if path_prefix.is_empty() { None } else { Some(path_prefix.to_string()) };
        return (Some(book.to_string()), path, sheet.to_string());
    }
    (None, None, name.to_string())
}

/// The matching `]` of the `[` at `i` (brackets nest; `'` escapes one).
fn brackets_end(chars: &[(usize, char)], i: usize) -> Option<usize> {
    let mut depth = 0;
    let mut j = i;
    while j < chars.len() {
        match chars[j].1 {
            '\'' => j += 1,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// After `first:` at `colon`, a sheet name and `!`: `Sheet3!` of `Sheet1:Sheet3!A1`.
fn sheet_after_colon(chars: &[(usize, char)], colon: usize) -> Option<(String, usize)> {
    let mut k = colon + 1;
    let start = k;
    while chars.get(k).is_some_and(|c| is_word_char(c.1)) {
        k += 1;
    }
    (k > start && chars.get(k).map(|c| c.1) == Some('!')).then(|| (chars[start..k].iter().map(|c| c.1).collect(), k))
}

fn ends_operand(token: Option<&Token>) -> bool {
    matches!(token, Some(Token::Word(_) | Token::Number(_) | Token::RParen | Token::Brackets(_) | Token::Spill | Token::Text(_) | Token::Error(_) | Token::RBrace))
}

fn starts_operand(c: char) -> bool {
    is_word_start(c) || c.is_ascii_digit() || matches!(c, '(' | '\'' | '[' | '$')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_value_next_to_multibyte_text_does_not_panic() {
        assert!(tokenize("#REF!=\"いいえ\"").is_ok());
        assert!(tokenize("#N/A&\"あいうえお\"").is_ok());
        assert!(tokenize("A1\u{3000}B1").is_ok());
    }

    fn kinds(text: &str) -> Vec<Token> {
        tokenize(text).unwrap().into_iter().map(|s| s.token).collect()
    }

    #[test]
    fn basics() {
        assert_eq!(
            kinds("SUM(A1:B2, 3.5)&\"a\"\"b\""),
            vec![
                Token::Word("SUM".into()),
                Token::LParen,
                Token::Word("A1".into()),
                Token::Op(Op::Range),
                Token::Word("B2".into()),
                Token::Comma,
                Token::Number(3.5),
                Token::RParen,
                Token::Op(Op::Concat),
                Token::Text("a\"b".into()),
            ]
        );
    }

    #[test]
    fn sheets_and_errors() {
        assert_eq!(kinds("'My Sheet'!$A$1")[0], Token::Sheet { first: "My Sheet".into(), last: None, book: None, path: None });
        assert_eq!(kinds("Sheet1:Sheet3!A1")[0], Token::Sheet { first: "Sheet1".into(), last: Some("Sheet3".into()), book: None, path: None });
        assert_eq!(kinds("'Sheet1:Sheet3'!A1")[0], Token::Sheet { first: "Sheet1".into(), last: Some("Sheet3".into()), book: None, path: None });
        assert_eq!(kinds("'Sheet 1:Sheet 3'!A1")[0], Token::Sheet { first: "Sheet 1".into(), last: Some("Sheet 3".into()), book: None, path: None });
        assert_eq!(kinds("'Sheet1':'Sheet3'!A1")[0], Token::Sheet { first: "Sheet1".into(), last: Some("Sheet3".into()), book: None, path: None });
        assert_eq!(kinds("[1]Data!A1")[0], Token::Sheet { first: "Data".into(), last: None, book: Some("1".into()), path: None });
        assert_eq!(kinds(r"'C:\dir\[Book.xlsx]Sheet1'!A1")[0], Token::Sheet { first: "Sheet1".into(), last: None, book: Some("Book.xlsx".into()), path: Some(r"C:\dir\".into()) });
        assert_eq!(kinds("#N/A+#div/0!"), vec![Token::Error(CellError::NA), Token::Op(Op::Add), Token::Error(CellError::Div0)]);
        assert_eq!(kinds("A1#")[1], Token::Spill);
        assert_eq!(kinds("2024!A1")[0], Token::Sheet { first: "2024".into(), last: None, book: None, path: None });
        assert_eq!(kinds("売上!A1")[0], Token::Sheet { first: "売上".into(), last: None, book: None, path: None });
    }

    #[test]
    fn intersection_and_spaces() {
        assert_eq!(kinds("A1:B2 B1:C3")[3], Token::Op(Op::Intersect));
        assert_eq!(kinds("A1 + B1"), vec![Token::Word("A1".into()), Token::Op(Op::Add), Token::Word("B1".into())]);
        assert_eq!(kinds("Table1[[#This Row],[Price]]")[1], Token::Brackets("[[#This Row],[Price]]".into()));
        assert_eq!(kinds("1E+3")[0], Token::Number(1000.0));
        assert_eq!(kinds(".5")[0], Token::Number(0.5));
    }

    /// Issue #21: multi-byte characters within 13 bytes after `#` (conditional formats of sheets with Japanese names).
    #[test]
    fn a_hash_before_multibyte_text_does_not_panic() {
        assert_eq!(kinds("#REF!+'売上'!$B$2>0")[0], Token::Error(CellError::Ref));
        assert_eq!(kinds("$A1#&\"円\"=\"x\"")[1], Token::Spill);
        assert!(tokenize("ISERROR(#あいう)").is_err());
    }
}
