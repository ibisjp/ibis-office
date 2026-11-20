// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The expressions of a custom column: a small part of M. Column references
//! in brackets (`[単価]`), numbers, text in quotes, `null`, `true`, `false`,
//! `+ - * / &`, comparisons, `and`, `or`, `not`, `if … then … else`, and
//! calls of the `Text.`, `Number.` and `Date.` functions listed in
//! [`FUNCTIONS`].
use ibis_calc::{CellError, CellValue};

use super::{compare, value_text};

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Null,
    Number(f64),
    Text(String),
    Bool(bool),
    Column(String),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Binary(Box<Expr>, Bin, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bin {
    Add,
    Sub,
    Mul,
    Div,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// The functions an expression may call.
pub const FUNCTIONS: &[&str] = &[
    "Text.Upper",
    "Text.Lower",
    "Text.Proper",
    "Text.Trim",
    "Text.Length",
    "Text.Start",
    "Text.End",
    "Text.Middle",
    "Text.Replace",
    "Text.Contains",
    "Text.From",
    "Text.PadStart",
    "Number.From",
    "Number.Round",
    "Number.RoundUp",
    "Number.RoundDown",
    "Number.Abs",
    "Number.Mod",
    "Number.IntegerDivide",
    "Date.Year",
    "Date.Month",
    "Date.Day",
    "Date.DayOfWeek",
    "Date.AddDays",
    "Date.AddMonths",
];

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Col(String),
    Ident(String),
    Sym(&'static str),
}

fn lex(text: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '"' {
            let mut s = String::new();
            i += 1;
            while i < chars.len() {
                if chars[i] == '"' {
                    if chars.get(i + 1) == Some(&'"') {
                        s.push('"');
                        i += 2;
                        continue;
                    }
                    break;
                }
                s.push(chars[i]);
                i += 1;
            }
            if i >= chars.len() {
                return Err("文字列が閉じていません".into());
            }
            i += 1;
            out.push(Tok::Str(s));
        } else if c == '[' && chars.get(i + 1) == Some(&'#') && chars.get(i + 2) == Some(&'"') {
            // `[#"Any name"]`
            let mut name = String::new();
            let mut k = i + 3;
            loop {
                match chars.get(k) {
                    Some('"') if chars.get(k + 1) == Some(&'"') => {
                        name.push('"');
                        k += 2;
                    }
                    Some('"') => break,
                    Some(c) => {
                        name.push(*c);
                        k += 1;
                    }
                    None => return Err("文字列が閉じていません".into()),
                }
            }
            if chars.get(k + 1) != Some(&']') {
                return Err("] がありません".into());
            }
            out.push(Tok::Col(name));
            i = k + 2;
        } else if c == '[' {
            let end = chars[i..].iter().position(|&x| x == ']').ok_or("] がありません")? + i;
            out.push(Tok::Col(chars[i + 1..end].iter().collect()));
            i = end + 1;
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit())) {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let t: String = chars[start..i].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| format!("数 {t} を読めません"))?));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.') {
                i += 1;
            }
            out.push(Tok::Ident(chars[start..i].iter().collect()));
        } else {
            let two: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let sym = match two.as_str() {
                "<>" => Some("<>"),
                "<=" => Some("<="),
                ">=" => Some(">="),
                _ => None,
            };
            if let Some(s) = sym {
                out.push(Tok::Sym(s));
                i += 2;
                continue;
            }
            let s = match c {
                '+' => "+",
                '-' => "-",
                '*' => "*",
                '/' => "/",
                '&' => "&",
                '=' => "=",
                '<' => "<",
                '>' => ">",
                '(' => "(",
                ')' => ")",
                ',' => ",",
                other => return Err(format!("{other} は式に使えません")),
            };
            out.push(Tok::Sym(s));
            i += 1;
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(i)) if i == w)
    }

    fn sym(&self, s: &str) -> bool {
        matches!(self.peek(), Some(Tok::Sym(x)) if *x == s)
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut e = self.and()?;
        while self.word("or") {
            self.at += 1;
            e = Expr::Binary(Box::new(e), Bin::Or, Box::new(self.and()?));
        }
        Ok(e)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut e = self.not()?;
        while self.word("and") {
            self.at += 1;
            e = Expr::Binary(Box::new(e), Bin::And, Box::new(self.not()?));
        }
        Ok(e)
    }

    fn not(&mut self) -> Result<Expr, String> {
        if self.word("not") {
            self.at += 1;
            return Ok(Expr::Not(Box::new(self.not()?)));
        }
        self.cmp()
    }

    fn cmp(&mut self) -> Result<Expr, String> {
        let e = self.concat()?;
        let op = match self.peek() {
            Some(Tok::Sym("=")) => Bin::Eq,
            Some(Tok::Sym("<>")) => Bin::Ne,
            Some(Tok::Sym("<")) => Bin::Lt,
            Some(Tok::Sym("<=")) => Bin::Le,
            Some(Tok::Sym(">")) => Bin::Gt,
            Some(Tok::Sym(">=")) => Bin::Ge,
            _ => return Ok(e),
        };
        self.at += 1;
        Ok(Expr::Binary(Box::new(e), op, Box::new(self.concat()?)))
    }

    fn concat(&mut self) -> Result<Expr, String> {
        let mut e = self.add()?;
        while self.sym("&") {
            self.at += 1;
            e = Expr::Binary(Box::new(e), Bin::Concat, Box::new(self.add()?));
        }
        Ok(e)
    }

    fn add(&mut self) -> Result<Expr, String> {
        let mut e = self.mul()?;
        loop {
            let op = if self.sym("+") {
                Bin::Add
            } else if self.sym("-") {
                Bin::Sub
            } else {
                return Ok(e);
            };
            self.at += 1;
            e = Expr::Binary(Box::new(e), op, Box::new(self.mul()?));
        }
    }

    fn mul(&mut self) -> Result<Expr, String> {
        let mut e = self.unary()?;
        loop {
            let op = if self.sym("*") {
                Bin::Mul
            } else if self.sym("/") {
                Bin::Div
            } else {
                return Ok(e);
            };
            self.at += 1;
            e = Expr::Binary(Box::new(e), op, Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.sym("-") {
            self.at += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, String> {
        let tok = self.peek().cloned().ok_or("式が途中で終わっています")?;
        self.at += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Number(n)),
            Tok::Str(s) => Ok(Expr::Text(s)),
            Tok::Col(c) => Ok(Expr::Column(c)),
            Tok::Sym("(") => {
                let e = self.or()?;
                if !self.sym(")") {
                    return Err(") がありません".into());
                }
                self.at += 1;
                Ok(e)
            }
            Tok::Ident(w) => match w.as_str() {
                "null" => Ok(Expr::Null),
                "true" => Ok(Expr::Bool(true)),
                "false" => Ok(Expr::Bool(false)),
                "each" => self.or(),
                "if" => {
                    let c = self.or()?;
                    if !self.word("then") {
                        return Err("then がありません".into());
                    }
                    self.at += 1;
                    let t = self.or()?;
                    if !self.word("else") {
                        return Err("else がありません".into());
                    }
                    self.at += 1;
                    let e = self.or()?;
                    Ok(Expr::If(Box::new(c), Box::new(t), Box::new(e)))
                }
                _ => {
                    if !self.sym("(") {
                        return Err(format!("{w} は使えません"));
                    }
                    if !FUNCTIONS.contains(&w.as_str()) {
                        return Err(format!("関数 {w} は使えません"));
                    }
                    self.at += 1;
                    let mut args = Vec::new();
                    if !self.sym(")") {
                        loop {
                            args.push(self.or()?);
                            if self.sym(",") {
                                self.at += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    if !self.sym(")") {
                        return Err(") がありません".into());
                    }
                    self.at += 1;
                    Ok(Expr::Call(w, args))
                }
            },
            Tok::Sym(s) => Err(format!("{s} の位置が正しくありません")),
        }
    }
}

fn num(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Number(n) => Some(*n),
        CellValue::Bool(b) => Some(f64::from(*b)),
        CellValue::Text(t) => t.trim().parse().ok(),
        _ => None,
    }
}

fn truthy(v: &CellValue) -> Option<bool> {
    match v {
        CellValue::Bool(b) => Some(*b),
        _ => None,
    }
}

impl Expr {
    pub fn parse(text: &str) -> Result<Expr, String> {
        let toks = lex(text)?;
        if toks.is_empty() {
            return Err("式が空です".into());
        }
        let mut p = Parser { toks, at: 0 };
        let e = p.or()?;
        if p.at < p.toks.len() {
            return Err("式の終わりに余分なものがあります".into());
        }
        Ok(e)
    }

    /// The columns the expression reads.
    pub fn columns(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<String>) {
        match self {
            Expr::Column(c) => out.push(c.clone()),
            Expr::Neg(e) | Expr::Not(e) => e.collect(out),
            Expr::Binary(a, _, b) => {
                a.collect(out);
                b.collect(out);
            }
            Expr::If(a, b, c) => {
                a.collect(out);
                b.collect(out);
                c.collect(out);
            }
            Expr::Call(_, args) => args.iter().for_each(|a| a.collect(out)),
            _ => {}
        }
    }

    /// The value on a row (`names` are the columns' names).
    pub fn eval(&self, names: &[String], row: &[CellValue]) -> CellValue {
        let err = || CellValue::Error(CellError::Value);
        match self {
            Expr::Null => CellValue::Empty,
            Expr::Number(n) => CellValue::Number(*n),
            Expr::Text(t) => CellValue::Text(t.clone()),
            Expr::Bool(b) => CellValue::Bool(*b),
            Expr::Column(c) => names.iter().position(|n| n == c).map_or_else(err, |i| row[i].clone()),
            Expr::Neg(e) => match e.eval(names, row) {
                CellValue::Empty => CellValue::Empty,
                v => num(&v).map_or_else(err, |n| CellValue::Number(-n)),
            },
            Expr::Not(e) => truthy(&e.eval(names, row)).map_or_else(err, |b| CellValue::Bool(!b)),
            Expr::If(c, t, e) => match truthy(&c.eval(names, row)) {
                Some(true) => t.eval(names, row),
                Some(false) => e.eval(names, row),
                None => err(),
            },
            Expr::Binary(a, op, b) => {
                let (x, y) = (a.eval(names, row), b.eval(names, row));
                if let (CellValue::Error(e), _) | (_, CellValue::Error(e)) = (&x, &y) {
                    return CellValue::Error(*e);
                }
                match op {
                    Bin::And => match (truthy(&x), truthy(&y)) {
                        (Some(p), Some(q)) => CellValue::Bool(p && q),
                        _ => err(),
                    },
                    Bin::Or => match (truthy(&x), truthy(&y)) {
                        (Some(p), Some(q)) => CellValue::Bool(p || q),
                        _ => err(),
                    },
                    Bin::Concat => {
                        if matches!(x, CellValue::Empty) && matches!(y, CellValue::Empty) {
                            return CellValue::Empty;
                        }
                        if matches!(x, CellValue::Empty) || matches!(y, CellValue::Empty) {
                            // Power Query: null & "a" is null.
                            return CellValue::Empty;
                        }
                        CellValue::Text(format!("{}{}", value_text(&x), value_text(&y)))
                    }
                    Bin::Eq => CellValue::Bool(super::same(&x, &y)),
                    Bin::Ne => CellValue::Bool(!super::same(&x, &y)),
                    Bin::Lt | Bin::Le | Bin::Gt | Bin::Ge => {
                        if matches!(x, CellValue::Empty) || matches!(y, CellValue::Empty) {
                            return CellValue::Empty;
                        }
                        let o = compare(&x, &y);
                        CellValue::Bool(match op {
                            Bin::Lt => o.is_lt(),
                            Bin::Le => o.is_le(),
                            Bin::Gt => o.is_gt(),
                            _ => o.is_ge(),
                        })
                    }
                    Bin::Add | Bin::Sub | Bin::Mul | Bin::Div => {
                        if matches!(x, CellValue::Empty) || matches!(y, CellValue::Empty) {
                            return CellValue::Empty;
                        }
                        let (Some(p), Some(q)) = (num(&x), num(&y)) else { return err() };
                        match op {
                            Bin::Add => CellValue::Number(p + q),
                            Bin::Sub => CellValue::Number(p - q),
                            Bin::Mul => CellValue::Number(p * q),
                            _ => {
                                if q == 0.0 {
                                    CellValue::Error(CellError::Div0)
                                } else {
                                    CellValue::Number(p / q)
                                }
                            }
                        }
                    }
                }
            }
            Expr::Call(name, args) => {
                let v: Vec<CellValue> = args.iter().map(|a| a.eval(names, row)).collect();
                call(name, &v)
            }
        }
    }
}

pub(super) fn call(name: &str, a: &[CellValue]) -> CellValue {
    let err = || CellValue::Error(CellError::Value);
    if let Some(CellValue::Error(e)) = a.iter().find(|v| matches!(v, CellValue::Error(_))) {
        return CellValue::Error(*e);
    }
    let text = |i: usize| a.get(i).map(value_text);
    let number = |i: usize| a.get(i).and_then(num);
    let null_in = matches!(a.first(), Some(CellValue::Empty)) && !matches!(name, "Text.From" | "Number.From");
    if null_in {
        return CellValue::Empty;
    }
    match name {
        "Text.Upper" => text(0).map_or_else(err, |t| CellValue::Text(t.to_uppercase())),
        "Text.Lower" => text(0).map_or_else(err, |t| CellValue::Text(t.to_lowercase())),
        "Text.Proper" => text(0).map_or_else(err, |t| {
            let mut out = String::new();
            let mut start = true;
            for c in t.chars() {
                if c.is_alphabetic() {
                    if start {
                        out.extend(c.to_uppercase());
                    } else {
                        out.extend(c.to_lowercase());
                    }
                    start = false;
                } else {
                    out.push(c);
                    start = true;
                }
            }
            CellValue::Text(out)
        }),
        "Text.Trim" => text(0).map_or_else(err, |t| CellValue::Text(t.trim().to_string())),
        "Text.Length" => text(0).map_or_else(err, |t| CellValue::Number(t.chars().count() as f64)),
        "Text.Start" => match (text(0), number(1)) {
            (Some(t), Some(n)) if n >= 0.0 => CellValue::Text(t.chars().take(n as usize).collect()),
            _ => err(),
        },
        "Text.End" => match (text(0), number(1)) {
            (Some(t), Some(n)) if n >= 0.0 => {
                let c: Vec<char> = t.chars().collect();
                CellValue::Text(c[c.len().saturating_sub(n as usize)..].iter().collect())
            }
            _ => err(),
        },
        "Text.Middle" => match (text(0), number(1), number(2)) {
            (Some(t), Some(s), n) if s >= 0.0 => {
                let it = t.chars().skip(s as usize);
                CellValue::Text(match n {
                    Some(n) if n >= 0.0 => it.take(n as usize).collect(),
                    _ => it.collect(),
                })
            }
            _ => err(),
        },
        "Text.Replace" => match (text(0), text(1), text(2)) {
            (Some(t), Some(f), Some(w)) if !f.is_empty() => CellValue::Text(t.replace(&f, &w)),
            _ => err(),
        },
        "Text.Contains" => match (text(0), text(1)) {
            (Some(t), Some(s)) => CellValue::Bool(t.contains(&s)),
            _ => err(),
        },
        "Text.From" => match a.first() {
            Some(CellValue::Empty) => CellValue::Empty,
            Some(CellValue::Bool(b)) => CellValue::Text(if *b { "true" } else { "false" }.into()),
            Some(v) => CellValue::Text(value_text(v)),
            None => err(),
        },
        "Text.PadStart" => match (text(0), number(1), text(2)) {
            (Some(t), Some(n), pad) if n >= 0.0 => {
                let pad = pad.and_then(|p| p.chars().next()).unwrap_or(' ');
                let len = t.chars().count();
                let mut out: String = std::iter::repeat_n(pad, (n as usize).saturating_sub(len)).collect();
                out.push_str(&t);
                CellValue::Text(out)
            }
            _ => err(),
        },
        "Number.From" => match a.first() {
            Some(CellValue::Empty) => CellValue::Empty,
            Some(v) => number_from(v),
            None => err(),
        },
        "Number.Round" => match (number(0), a.get(1).map(|_| number(1))) {
            (Some(n), None) => CellValue::Number(round_half_even(n, 0)),
            (Some(n), Some(Some(d))) => CellValue::Number(round_half_even(n, d as i32)),
            _ => err(),
        },
        "Number.RoundUp" => number(0).map_or_else(err, |n| CellValue::Number(n.ceil())),
        "Number.RoundDown" => number(0).map_or_else(err, |n| CellValue::Number(n.floor())),
        "Number.Abs" => number(0).map_or_else(err, |n| CellValue::Number(n.abs())),
        "Number.Mod" => match (number(0), number(1)) {
            (Some(n), Some(d)) if d != 0.0 => CellValue::Number(n - d * (n / d).floor()),
            (Some(_), Some(_)) => CellValue::Error(CellError::Div0),
            _ => err(),
        },
        "Number.IntegerDivide" => match (number(0), number(1)) {
            (Some(n), Some(d)) if d != 0.0 => CellValue::Number((n / d).trunc()),
            (Some(_), Some(_)) => CellValue::Error(CellError::Div0),
            _ => err(),
        },
        "Date.Year" | "Date.Month" | "Date.Day" | "Date.DayOfWeek" => match number(0) {
            Some(serial) => {
                let days = serial.floor() as i64 - 25569;
                let (y, m, d) = ibis_calc::eval::convert::civil_from_days(days);
                CellValue::Number(match name {
                    "Date.Year" => y as f64,
                    "Date.Month" => f64::from(m),
                    "Date.Day" => f64::from(d),
                    // Sunday is 0.
                    _ => (days + 4).rem_euclid(7) as f64,
                })
            }
            None => err(),
        },
        "Date.AddDays" => match (number(0), number(1)) {
            (Some(s), Some(n)) => CellValue::Number(s + n.trunc()),
            _ => err(),
        },
        "Date.AddMonths" => match (number(0), number(1)) {
            (Some(s), Some(n)) => {
                let days = s.floor() as i64 - 25569;
                let (y, m, d) = ibis_calc::eval::convert::civil_from_days(days);
                let total = y * 12 + i64::from(m) - 1 + n.trunc() as i64;
                let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) + 1);
                let last = last_day(ny, nm);
                let serial = ibis_calc::eval::convert::days_from_civil(ny, nm, i64::from(d).min(last)) + 25569;
                CellValue::Number(serial as f64 + (s - s.floor()))
            }
            _ => err(),
        },
        _ => err(),
    }
}

fn last_day(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

fn number_from(v: &CellValue) -> CellValue {
    match v {
        CellValue::Number(_) => v.clone(),
        CellValue::Bool(b) => CellValue::Number(f64::from(*b)),
        CellValue::Text(t) => t.trim().parse().map_or(CellValue::Error(CellError::Value), CellValue::Number),
        other => other.clone(),
    }
}

/// Power Query rounds halves to the even digit (`Number.Round`'s default).
fn round_half_even(n: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    let x = n * f;
    let r = x.round();
    let r = if (x - x.trunc()).abs() == 0.5 && r % 2.0 != 0.0 { r - x.signum() } else { r };
    r / f
}
