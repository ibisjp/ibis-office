// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Parsing a formula into an expression tree.
//!
//! Precedence, tightest first, as Excel: `:` (range), ` ` (intersection),
//! `,` (union, inside parentheses), unary `-` and `+`, `%`, `^`, `*` `/`,
//! `+` `-`, `&`, comparisons. So `-2^2` is 4, as in Excel.

use super::lexer::{Op, Spanned, Token, tokenize};
use crate::value::CellError;

/// The last row and column of a worksheet (0-based).
pub const MAX_ROW: u32 = 1_048_575;
pub const MAX_COLUMN: u32 = 16_383;

/// One corner of a reference: a row and a column (0-based), each fixed
/// (`$`) or relative.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Corner {
    pub row: u32,
    pub column: u32,
    pub row_fixed: bool,
    pub column_fixed: bool,
}

/// Which sheet(s) a reference is on, as written.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SheetRef {
    pub first: String,
    /// The last sheet of a 3D reference (`Sheet1:Sheet3!A1`).
    pub last: Option<String>,
    /// An external workbook (`[1]`).
    pub book: Option<String>,
    /// The directory path of an external workbook (`'C:\dir\[Book.xlsx]Sheet1'!A1`).
    pub path: Option<String>,
}

/// A rectangle of cells as written: `A1`, `A1:B2`, `A:C` (whole columns,
/// rows 0 to [`MAX_ROW`]), `1:3` (whole rows).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AreaRef {
    pub sheet: Option<SheetRef>,
    pub start: Corner,
    pub end: Corner,
    /// Whole columns (`A:C`) or rows (`1:3`), for writing back.
    pub whole_columns: bool,
    pub whole_rows: bool,
}

impl AreaRef {
    pub fn is_cell(&self) -> bool {
        self.start.row == self.end.row && self.start.column == self.end.column && !self.whole_columns && !self.whole_rows
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
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
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Number(f64),
    Text(String),
    Bool(bool),
    Error(CellError),
    /// An array constant, by rows.
    Array(Vec<Vec<Expr>>),
    Ref(AreaRef),
    /// A defined name (or, unknown, `#NAME?`), maybe on a sheet.
    Name { sheet: Option<SheetRef>, name: String },
    /// A structured reference: the table (empty for `[@Col]` inside a
    /// table) and the brackets as written.
    Structured { table: String, spec: String },
    /// A function, its name in upper case without `_xlfn.`.
    Call { name: String, args: Vec<Expr> },
    /// A call of a value, such as a LAMBDA: `LAMBDA(x, x+1)(2)`.
    Invoke { callee: Box<Expr>, args: Vec<Expr> },
    /// An argument left out (`IF(A1,,2)`).
    Missing,
    Neg(Box<Expr>),
    Plus(Box<Expr>),
    Percent(Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    /// `a:b` between expressions that are not both plain cells (such as
    /// `A1:INDEX(…)`).
    Range(Box<Expr>, Box<Expr>),
    Intersect(Box<Expr>, Box<Expr>),
    Union(Vec<Expr>),
    /// `A1#`: the range spilled from a cell.
    Spill(Box<Expr>),
    /// `@expr`: implicit intersection.
    Implicit(Box<Expr>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub at: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Parse a formula (without the leading `=`).
pub fn parse(text: &str) -> Result<Expr, ParseError> {
    parse_with_implicit_spans(text).map(|(expr, _)| expr)
}

pub(super) fn implicit_spans(text: &str) -> Result<Vec<(usize, usize)>, ParseError> {
    parse_with_implicit_spans(text).map(|(_, spans)| spans)
}

fn parse_with_implicit_spans(text: &str) -> Result<(Expr, Vec<(usize, usize)>), ParseError> {
    let tokens = tokenize(text).map_err(|e| ParseError { at: e.at, message: e.message.to_string() })?;
    let mut p = Parser { tokens, i: 0, end: text.len(), implicit_spans: Vec::new() };
    if p.tokens.is_empty() {
        return Err(ParseError { at: 0, message: "数式がありません".into() });
    }
    let e = p.expr()?;
    if p.i < p.tokens.len() {
        return Err(p.error("数式の終わりに余分なものがあります"));
    }
    Ok((e, p.implicit_spans))
}

struct Parser {
    tokens: Vec<Spanned>,
    i: usize,
    end: usize,
    implicit_spans: Vec<(usize, usize)>,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.i).map(|s| &s.token)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.i).map(|s| s.token.clone());
        self.i += 1;
        t
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError { at: self.tokens.get(self.i).map_or(self.end, |s| s.start), message: message.to_string() }
    }

    fn expect(&mut self, token: Token, message: &str) -> Result<(), ParseError> {
        if self.peek() == Some(&token) {
            self.i += 1;
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        self.comparison()
    }

    fn comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.concat()?;
        loop {
            let op = match self.peek() {
                Some(Token::Op(Op::Eq)) => BinaryOp::Eq,
                Some(Token::Op(Op::Ne)) => BinaryOp::Ne,
                Some(Token::Op(Op::Lt)) => BinaryOp::Lt,
                Some(Token::Op(Op::Le)) => BinaryOp::Le,
                Some(Token::Op(Op::Gt)) => BinaryOp::Gt,
                Some(Token::Op(Op::Ge)) => BinaryOp::Ge,
                _ => return Ok(left),
            };
            self.i += 1;
            let right = self.concat()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn concat(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.additive()?;
        while self.peek() == Some(&Token::Op(Op::Concat)) {
            self.i += 1;
            let right = self.additive()?;
            left = Expr::Binary(BinaryOp::Concat, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.multiplicative()?;
        loop {
            let op = match self.peek() {
                Some(Token::Op(Op::Add)) => BinaryOp::Add,
                Some(Token::Op(Op::Sub)) => BinaryOp::Sub,
                _ => return Ok(left),
            };
            self.i += 1;
            let right = self.multiplicative()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.power()?;
        loop {
            let op = match self.peek() {
                Some(Token::Op(Op::Mul)) => BinaryOp::Mul,
                Some(Token::Op(Op::Div)) => BinaryOp::Div,
                _ => return Ok(left),
            };
            self.i += 1;
            let right = self.power()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn power(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.percent()?;
        while self.peek() == Some(&Token::Op(Op::Pow)) {
            self.i += 1;
            let right = self.percent()?;
            left = Expr::Binary(BinaryOp::Pow, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn percent(&mut self) -> Result<Expr, ParseError> {
        let mut e = self.unary()?;
        while self.peek() == Some(&Token::Op(Op::Percent)) {
            self.i += 1;
            e = Expr::Percent(Box::new(e));
        }
        Ok(e)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some(Token::Op(Op::Sub)) => {
                self.i += 1;
                Ok(Expr::Neg(Box::new(self.unary()?)))
            }
            Some(Token::Op(Op::Add)) => {
                self.i += 1;
                Ok(Expr::Plus(Box::new(self.unary()?)))
            }
            _ => self.intersection(),
        }
    }

    fn intersection(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.range()?;
        while self.peek() == Some(&Token::Op(Op::Intersect)) {
            self.i += 1;
            let right = self.range()?;
            left = Expr::Intersect(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn range(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.postfix()?;
        while let Some(&Token::Op(op)) = self.peek()
            && op.is_range()
        {
            self.i += 1;
            let right = self.postfix()?;
            left = combine_range(left, right);
            // `a:.b`, `a.:b`, `a.:.b` are TRIMRANGE(a:b, mode, mode): 2 trailing, 1 leading, 3 both.
            let mode = match op {
                Op::RangeTrimEnd => 2.0,
                Op::RangeTrimStart => 1.0,
                Op::RangeTrimBoth => 3.0,
                _ => continue,
            };
            left = Expr::Call { name: "TRIMRANGE".to_string(), args: vec![left, Expr::Number(mode), Expr::Number(mode)] };
        }
        Ok(left)
    }

    fn postfix(&mut self) -> Result<Expr, ParseError> {
        let mut e = self.primary()?;
        loop {
            match self.peek() {
                Some(Token::Spill) => {
                    self.i += 1;
                    e = Expr::Spill(Box::new(e));
                }
                // A value called: LAMBDA(x,x)(1), or a name bound by LET.
                Some(Token::LParen) if matches!(e, Expr::Call { .. } | Expr::Invoke { .. }) => {
                    self.i += 1;
                    let args = self.arguments()?;
                    e = Expr::Invoke { callee: Box::new(e), args };
                }
                _ => return Ok(e),
            }
        }
    }

    /// Arguments after `(`, up to and including `)`.
    fn arguments(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        if self.peek() == Some(&Token::RParen) {
            self.i += 1;
            return Ok(args);
        }
        loop {
            if matches!(self.peek(), Some(Token::Comma | Token::RParen)) {
                args.push(Expr::Missing);
            } else {
                args.push(self.expr()?);
            }
            match self.next() {
                Some(Token::Comma) => continue,
                Some(Token::RParen) => return Ok(args),
                _ => {
                    self.i -= 1;
                    return Err(self.error("関数の引数の区切りが正しくありません"));
                }
            }
        }
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let Some(token) = self.next() else {
            return Err(self.error("数式が途中で終わっています"));
        };
        match token {
            Token::Number(n) => Ok(number_or_rows(n, self)),
            Token::Text(s) => Ok(Expr::Text(s)),
            Token::Error(e) => Ok(Expr::Error(e)),
            Token::At => {
                let start = self.tokens[self.i - 1].start;
                let operand = self.range()?;
                let end = self.tokens[self.i - 1].end;
                self.implicit_spans.push((start, end));
                Ok(Expr::Implicit(Box::new(operand)))
            },
            Token::LParen => {
                let first = self.expr()?;
                let mut items = vec![first];
                while self.peek() == Some(&Token::Comma) {
                    self.i += 1;
                    items.push(self.expr()?);
                }
                self.expect(Token::RParen, ") がありません")?;
                Ok(if items.len() == 1 { items.pop().unwrap() } else { Expr::Union(items) })
            }
            Token::LBrace => self.array(),
            Token::Sheet { first, last, book, path } => {
                let sheet = SheetRef { first, last, book, path };
                match self.next() {
                    Some(Token::Word(w)) => Ok(word_on_sheet(&w, Some(sheet), self)),
                    Some(Token::Number(n)) => {
                        let e = number_or_rows(n, self);
                        Ok(with_sheet(e, sheet))
                    }
                    Some(Token::Error(e)) => Ok(Expr::Error(e)),
                    _ => {
                        self.i -= 1;
                        Err(self.error("シート名の後に参照がありません"))
                    }
                }
            }
            Token::Brackets(spec) => Ok(Expr::Structured { table: String::new(), spec }),
            Token::Word(w) => {
                if self.peek() == Some(&Token::LParen) {
                    self.i += 1;
                    let args = self.arguments()?;
                    return Ok(Expr::Call { name: function_name(&w), args });
                }
                if let Some(Token::Brackets(_)) = self.peek() {
                    let Some(Token::Brackets(spec)) = self.next() else { unreachable!() };
                    return Ok(Expr::Structured { table: w, spec });
                }
                Ok(word_on_sheet(&w, None, self))
            }
            _ => {
                self.i -= 1;
                Err(self.error("ここに値や参照が必要です"))
            }
        }
    }

    fn array(&mut self) -> Result<Expr, ParseError> {
        let mut rows = vec![Vec::new()];
        loop {
            let negative = if self.peek() == Some(&Token::Op(Op::Sub)) {
                self.i += 1;
                true
            } else {
                if self.peek() == Some(&Token::Op(Op::Add)) {
                    self.i += 1;
                }
                false
            };
            let item = match self.next() {
                Some(Token::Number(n)) => Expr::Number(if negative { -n } else { n }),
                Some(Token::Text(s)) if !negative => Expr::Text(s),
                Some(Token::Error(e)) if !negative => Expr::Error(e),
                Some(Token::Word(w)) if !negative && w.eq_ignore_ascii_case("TRUE") => Expr::Bool(true),
                Some(Token::Word(w)) if !negative && w.eq_ignore_ascii_case("FALSE") => Expr::Bool(false),
                _ => {
                    self.i -= 1;
                    return Err(self.error("配列の定数には数値・文字列・論理値・エラー値だけが使えます"));
                }
            };
            rows.last_mut().unwrap().push(item);
            match self.next() {
                Some(Token::Comma) => {}
                Some(Token::Semicolon) => rows.push(Vec::new()),
                Some(Token::RBrace) => break,
                _ => {
                    self.i -= 1;
                    return Err(self.error("配列の定数が正しくありません"));
                }
            }
        }
        let width = rows[0].len();
        if rows.iter().any(|r| r.len() != width) {
            return Err(self.error("配列の定数の行の長さがそろっていません"));
        }
        Ok(Expr::Array(rows))
    }
}

/// A function's name as the engine knows it: upper case, without the
/// prefixes a file adds (`_xlfn.`, `_xlws.`).
pub fn function_name(word: &str) -> String {
    let mut w = word;
    for prefix in ["_xlfn.", "_xlws.", "_xlfn._xlws."] {
        if w.as_bytes().get(..prefix.len()).is_some_and(|p| p.eq_ignore_ascii_case(prefix.as_bytes())) {
            w = &w[prefix.len()..];
        }
    }
    if w.as_bytes().get(..6).is_some_and(|p| p.eq_ignore_ascii_case(b"_xlws.")) {
        w = &w[6..];
    }
    w.to_uppercase()
}

/// A number that may be the start of a row range (`1:3`).
fn number_or_rows(n: f64, p: &mut Parser) -> Expr {
    let after_range = p.i >= 2 && matches!(p.tokens.get(p.i - 2).map(|s| &s.token), Some(Token::Op(o)) if o.is_range());
    if n.fract() == 0.0 && n >= 1.0 && n <= (MAX_ROW + 1) as f64 && (matches!(p.peek(), Some(Token::Op(o)) if o.is_range()) || after_range) {
        let row = n as u32 - 1;
        return Expr::Name { sheet: None, name: format!("\u{0}row:{row}:0:{n}") };
    }
    Expr::Number(n)
}

fn with_sheet(e: Expr, sheet: SheetRef) -> Expr {
    match e {
        Expr::Ref(mut a) => {
            a.sheet = Some(sheet);
            Expr::Ref(a)
        }
        Expr::Name { name, .. } => Expr::Name { sheet: Some(sheet), name },
        other => other,
    }
}

/// A word: TRUE/FALSE, a cell, a column or row (for a range), or a name.
fn word_on_sheet(word: &str, sheet: Option<SheetRef>, p: &mut Parser) -> Expr {
    if sheet.is_none() && word.eq_ignore_ascii_case("TRUE") {
        return Expr::Bool(true);
    }
    if sheet.is_none() && word.eq_ignore_ascii_case("FALSE") {
        return Expr::Bool(false);
    }
    if let Some(corner) = parse_cell(word) {
        return Expr::Ref(AreaRef { sheet, start: corner, end: corner, whole_columns: false, whole_rows: false });
    }
    // Part of a whole column or row range: kept as a marker name that
    // `combine_range` turns into a range.
    if matches!(p.peek(), Some(Token::Op(o)) if o.is_range()) || matches!(p.tokens.get(p.i.wrapping_sub(2)).map(|s| &s.token), Some(Token::Op(o)) if o.is_range()) {
        if let Some((column, fixed)) = parse_column(word) {
            return Expr::Name { sheet, name: format!("\u{0}col:{column}:{}:{word}", fixed as u8) };
        }
        if let Some((row, fixed)) = parse_row(word) {
            return Expr::Name { sheet, name: format!("\u{0}row:{row}:{}:{word}", fixed as u8) };
        }
    }
    Expr::Name { sheet, name: word.to_string() }
}

/// Parts of a whole column or row range, as marked by `word_on_sheet`.
fn marker(e: &Expr) -> Option<(Option<SheetRef>, bool, u32, bool)> {
    let Expr::Name { sheet, name } = e else { return None };
    let rest = name.strip_prefix('\u{0}')?;
    let (kind, rest) = rest.split_once(':')?;
    let (index, rest) = rest.split_once(':')?;
    let (fixed, _) = rest.split_once(':')?;
    Some((sheet.clone(), kind == "col", index.parse().ok()?, fixed == "1"))
}

/// `left:right`.
fn combine_range(left: Expr, right: Expr) -> Expr {
    // Whole columns or rows.
    if let (Some((sheet, lc, a, af)), Some((sheet2, rc, b, bf))) = (marker(&left), marker(&right))
        && lc == rc
        && sheet2.is_none()
    {
        let (lo, hi, lo_fixed, hi_fixed) = if a <= b { (a, b, af, bf) } else { (b, a, bf, af) };
        let (start, end) = if lc {
            (Corner { row: 0, column: lo, row_fixed: true, column_fixed: lo_fixed }, Corner { row: MAX_ROW, column: hi, row_fixed: true, column_fixed: hi_fixed })
        } else {
            (Corner { row: lo, column: 0, row_fixed: lo_fixed, column_fixed: true }, Corner { row: hi, column: MAX_COLUMN, row_fixed: hi_fixed, column_fixed: true })
        };
        return Expr::Ref(AreaRef { sheet, start, end, whole_columns: lc, whole_rows: !lc });
    }
    // A1:B2 → one area on the first's sheet.
    if let (Expr::Ref(a), Expr::Ref(b)) = (&left, &right)
        && a.is_cell()
        && b.is_cell()
        && b.sheet.is_none()
    {
        let (s, e) = (a.start, b.start);
        let start = Corner {
            row: s.row.min(e.row),
            column: s.column.min(e.column),
            row_fixed: if s.row <= e.row { s.row_fixed } else { e.row_fixed },
            column_fixed: if s.column <= e.column { s.column_fixed } else { e.column_fixed },
        };
        let end = Corner {
            row: s.row.max(e.row),
            column: s.column.max(e.column),
            row_fixed: if s.row <= e.row { e.row_fixed } else { s.row_fixed },
            column_fixed: if s.column <= e.column { e.column_fixed } else { s.column_fixed },
        };
        return Expr::Ref(AreaRef { sheet: a.sheet.clone(), start, end, whole_columns: false, whole_rows: false });
    }
    Expr::Range(Box::new(unmark(left)), Box::new(unmark(right)))
}

/// A marker that did not become a range is what it was written as again:
/// a number or a name.
fn unmark(e: Expr) -> Expr {
    if marker(&e).is_none() {
        return e;
    }
    let Expr::Name { sheet, name } = e else { unreachable!() };
    let word = name.splitn(4, ':').nth(3).unwrap_or("").to_string();
    match word.parse::<f64>() {
        Ok(n) if sheet.is_none() => Expr::Number(n),
        _ => Expr::Name { sheet, name: word },
    }
}

/// A cell reference such as `A1`, `$B$2`, `xfd1048576` (any case).
pub fn parse_cell(word: &str) -> Option<Corner> {
    let b = word.as_bytes();
    let mut i = 0;
    let column_fixed = b.first() == Some(&b'$');
    if column_fixed {
        i += 1;
    }
    let letters_start = i;
    while i < b.len() && b[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i == letters_start || i - letters_start > 3 {
        return None;
    }
    let letters = &word[letters_start..i];
    let row_fixed = b.get(i) == Some(&b'$');
    if row_fixed {
        i += 1;
    }
    let digits_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start || i != b.len() || b[digits_start] == b'0' {
        return None;
    }
    let column = column_index(letters)?;
    let row: u32 = word[digits_start..].parse().ok()?;
    if row == 0 || row > MAX_ROW + 1 {
        return None;
    }
    Some(Corner { row: row - 1, column, row_fixed, column_fixed })
}

/// Column letters (`A`, `XFD`) as a 0-based index.
pub fn column_index(letters: &str) -> Option<u32> {
    let mut column = 0u32;
    for c in letters.bytes() {
        if !c.is_ascii_alphabetic() {
            return None;
        }
        column = column * 26 + (c.to_ascii_uppercase() - b'A' + 1) as u32;
    }
    (column >= 1 && column <= MAX_COLUMN + 1).then(|| column - 1)
}

fn parse_column(word: &str) -> Option<(u32, bool)> {
    let (fixed, letters) = match word.strip_prefix('$') {
        Some(rest) => (true, rest),
        None => (false, word),
    };
    if letters.is_empty() || letters.len() > 3 || !letters.bytes().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some((column_index(letters)?, fixed))
}

fn parse_row(word: &str) -> Option<(u32, bool)> {
    let (fixed, digits) = match word.strip_prefix('$') {
        Some(rest) => (true, rest),
        None => (false, word),
    };
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let row: u32 = digits.parse().ok()?;
    (row >= 1 && row <= MAX_ROW + 1).then(|| (row - 1, fixed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_name_with_multibyte_text_does_not_panic() {
        assert_eq!(function_name("xあい"), "Xあい");
        assert_eq!(function_name("_xlfn.sum"), "SUM");
        let _ = parse("xあい(A1)");
    }

    fn cell(row: u32, column: u32) -> Corner {
        Corner { row, column, row_fixed: false, column_fixed: false }
    }

    #[test]
    fn precedence() {
        // 1. Unary minus vs power (Excel specifies unary minus binds tighter than power: -2^2 == (-2)^2 == 4)
        let e = parse("-2^2").unwrap();
        assert_eq!(e, Expr::Binary(BinaryOp::Pow, Box::new(Expr::Neg(Box::new(Expr::Number(2.0)))), Box::new(Expr::Number(2.0))));

        // Parentheses override unary precedence: -(2^2)
        let e = parse("-(2^2)").unwrap();
        assert_eq!(e, Expr::Neg(Box::new(Expr::Binary(BinaryOp::Pow, Box::new(Expr::Number(2.0)), Box::new(Expr::Number(2.0))))));

        // 2. Power is left-associative: 2^3^2 == (2^3)^2
        let e = parse("2^3^2").unwrap();
        assert_eq!(e, Expr::Binary(BinaryOp::Pow, Box::new(Expr::Binary(BinaryOp::Pow, Box::new(Expr::Number(2.0)), Box::new(Expr::Number(3.0)))), Box::new(Expr::Number(2.0))));

        // 3. Multiplication/Division vs Power: 2*3^2 == 2*(3^2)
        let e = parse("2*3^2").unwrap();
        assert_eq!(e, Expr::Binary(BinaryOp::Mul, Box::new(Expr::Number(2.0)), Box::new(Expr::Binary(BinaryOp::Pow, Box::new(Expr::Number(3.0)), Box::new(Expr::Number(2.0))))));

        // 4. Addition/Subtraction vs Multiplication/Division: 1+2*3 == 1+(2*3)
        let e = parse("1+2*3").unwrap();
        assert_eq!(e, Expr::Binary(BinaryOp::Add, Box::new(Expr::Number(1.0)), Box::new(Expr::Binary(BinaryOp::Mul, Box::new(Expr::Number(2.0)), Box::new(Expr::Number(3.0))))));

        // 5. Percent operator: 50%*2 == (50%)*2
        let e = parse("50%*2").unwrap();
        assert_eq!(e, Expr::Binary(BinaryOp::Mul, Box::new(Expr::Percent(Box::new(Expr::Number(50.0)))), Box::new(Expr::Number(2.0))));

        // 6. Concatenation vs Addition: 1+2&3+4 == (1+2)&(3+4)
        let e = parse("1+2&3+4").unwrap();
        assert_eq!(e, Expr::Binary(BinaryOp::Concat, Box::new(Expr::Binary(BinaryOp::Add, Box::new(Expr::Number(1.0)), Box::new(Expr::Number(2.0)))), Box::new(Expr::Binary(BinaryOp::Add, Box::new(Expr::Number(3.0)), Box::new(Expr::Number(4.0))))));

        // 7. Comparison vs Concatenation: 1&2="12" == (1&2)="12"
        let e = parse("1&2=\"12\"").unwrap();
        assert_eq!(e, Expr::Binary(BinaryOp::Eq, Box::new(Expr::Binary(BinaryOp::Concat, Box::new(Expr::Number(1.0)), Box::new(Expr::Number(2.0)))), Box::new(Expr::Text("12".into()))));

        // 8. Comparison vs Arithmetic: 1+2<2*2 == (1+2)<(2*2)
        let e = parse("1+2<2*2").unwrap();
        assert_eq!(e, Expr::Binary(BinaryOp::Lt, Box::new(Expr::Binary(BinaryOp::Add, Box::new(Expr::Number(1.0)), Box::new(Expr::Number(2.0)))), Box::new(Expr::Binary(BinaryOp::Mul, Box::new(Expr::Number(2.0)), Box::new(Expr::Number(2.0))))));
    }

    #[test]
    fn references() {
        let Expr::Ref(a) = parse("Sheet1!$A$1:b3").unwrap() else { panic!() };
        assert_eq!(a.sheet.as_ref().unwrap().first, "Sheet1");
        assert_eq!((a.start.row, a.start.column, a.end.row, a.end.column), (0, 0, 2, 1));
        assert!(a.start.row_fixed && !a.end.row_fixed);
        let Expr::Ref(a) = parse("A:C").unwrap() else { panic!() };
        assert!(a.whole_columns);
        assert_eq!((a.start.column, a.end.column, a.end.row), (0, 2, MAX_ROW));
        let Expr::Ref(a) = parse("$2:5").unwrap() else { panic!() };
        assert!(a.whole_rows);
        assert_eq!((a.start.row, a.end.row), (1, 4));
        let Expr::Ref(a) = parse("'My Sheet'!3:3").unwrap() else { panic!() };
        assert_eq!(a.sheet.unwrap().first, "My Sheet");
        assert_eq!(parse("A1").unwrap(), Expr::Ref(AreaRef { sheet: None, start: cell(0, 0), end: cell(0, 0), whole_columns: false, whole_rows: false }));
        assert!(matches!(parse("LOG10(A1)").unwrap(), Expr::Call { .. }));
        assert!(matches!(parse("A1:INDEX(B:B,2)").unwrap(), Expr::Range(_, _)));
        assert!(matches!(parse("(A1,B2)").unwrap(), Expr::Union(_)));
        assert!(matches!(parse("A1:B2 B1:C3").unwrap(), Expr::Intersect(_, _)));
    }

    #[test]
    fn calls_and_arrays() {
        let Expr::Call { name, args } = parse("_xlfn.XLOOKUP(1,A1:A3,B1:B3,,0)").unwrap() else { panic!() };
        assert_eq!(name, "XLOOKUP");
        assert_eq!(args.len(), 5);
        assert_eq!(args[3], Expr::Missing);
        let Expr::Array(rows) = parse("{1,-2;\"a\",TRUE}").unwrap() else { panic!() };
        assert_eq!(rows[0][1], Expr::Number(-2.0));
        assert_eq!(rows[1][1], Expr::Bool(true));
        assert!(parse("{1,2;3}").is_err());
        assert!(matches!(parse("PI()").unwrap(), Expr::Call { ref args, .. } if args.is_empty()));
        assert!(matches!(parse("LAMBDA(x,x+1)(2)").unwrap(), Expr::Invoke { .. }));
        assert!(parse("SUM(1,").is_err());
        assert!(parse("1+").is_err());
    }
}
