// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The parser of M (recursive descent over the tokens). The precedence, from the loosest: `??`, `or`, `and`, `is`/`as`, equality,
//! relational, additive (`+ - &`), multiplicative, unary. `let`, `if`, `each`, `try`, `error` and functions are whole expressions.
use crate::ast::*;
use crate::error::MError;
use crate::lex::{Tok, Token, lex};
use std::rc::Rc;

pub fn parse(src: &str) -> Result<Rc<Expr>, MError> {
    let toks = lex(src)?;
    let mut p = Parser { toks, pos: 0 };
    let e = p.expr()?;
    match p.peek() {
        Tok::Eof => Ok(e),
        _ => Err(p.err("Token Eof expected.")),
    }
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

type R<T> = Result<T, MError>;

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }
    fn line(&self) -> u32 {
        self.toks[self.pos].line
    }
    fn bump(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }
    fn err(&self, msg: &str) -> MError {
        MError::syntax(self.line(), msg)
    }
    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Tok::Op(o) if *o == op)
    }
    fn is_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Tok::Kw(k) if *k == kw)
    }
    fn eat_op(&mut self, op: &str) -> bool {
        if self.is_op(op) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.is_kw(kw) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn expect_op(&mut self, op: &str) -> R<()> {
        if self.eat_op(op) { Ok(()) } else { Err(self.err(&format!("Token {op} expected."))) }
    }
    fn expect_kw(&mut self, kw: &str) -> R<()> {
        if self.eat_kw(kw) { Ok(()) } else { Err(self.err(&format!("Token {kw} expected."))) }
    }
    fn ident(&mut self) -> R<String> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                self.bump();
                Ok(s)
            }
            _ => Err(self.err("Token Identifier expected.")),
        }
    }

    fn expr(&mut self) -> R<Rc<Expr>> {
        let line = self.line();
        match self.peek().clone() {
            Tok::Kw("let") => {
                self.bump();
                let mut binds = Vec::new();
                loop {
                    let name = self.ident()?;
                    self.expect_op("=")?;
                    binds.push((name, self.expr()?));
                    if !self.eat_op(",") {
                        break;
                    }
                }
                self.expect_kw("in")?;
                Ok(Rc::new(Expr::Let(binds, self.expr()?)))
            }
            Tok::Kw("if") => {
                self.bump();
                let c = self.expr()?;
                self.expect_kw("then")?;
                let t = self.expr()?;
                self.expect_kw("else")?;
                let e = self.expr()?;
                Ok(Rc::new(Expr::If { c, t, e }))
            }
            Tok::Kw("each") => {
                self.bump();
                Ok(Rc::new(Expr::Each(self.expr()?)))
            }
            Tok::Kw("error") => {
                self.bump();
                Ok(Rc::new(Expr::Error(self.expr()?, line)))
            }
            Tok::Kw("try") => {
                self.bump();
                let e = self.expr()?;
                let otherwise = if self.eat_kw("otherwise") { Some(self.expr()?) } else { None };
                Ok(Rc::new(Expr::Try { e, otherwise }))
            }
            Tok::Op("(") => {
                if let Some(f) = self.try_function()? {
                    return Ok(f);
                }
                self.coalesce()
            }
            _ => self.coalesce(),
        }
    }

    /// At '(': a function expression `(a, optional b as number) as text => body`, or None (a parenthesized expression).
    fn try_function(&mut self) -> R<Option<Rc<Expr>>> {
        let save = self.pos;
        self.bump();
        let mut params = Vec::new();
        let ok = (|| -> R<bool> {
            if !self.is_op(")") {
                loop {
                    let optional = self.eat_kw("optional");
                    let name = match self.peek().clone() {
                        Tok::Ident(s) => {
                            self.bump();
                            s
                        }
                        _ => return Ok(false),
                    };
                    let ty = if self.eat_kw("as") { Some(self.type_spec()?) } else { None };
                    params.push(Param { name, optional, ty });
                    if !self.eat_op(",") {
                        break;
                    }
                }
            }
            if !self.eat_op(")") {
                return Ok(false);
            }
            Ok(true)
        })();
        let ret_and_arrow = match ok {
            Ok(true) => {
                let ret = if self.eat_kw("as") { self.type_spec().ok() } else { None };
                if self.eat_op("=>") { Some(ret) } else { None }
            }
            _ => None,
        };
        match ret_and_arrow {
            Some(ret) => {
                let body = self.expr()?;
                Ok(Some(Rc::new(Expr::Func { params, ret, body })))
            }
            None => {
                self.pos = save;
                Ok(None)
            }
        }
    }

    fn type_spec(&mut self) -> R<TypeSpec> {
        let nullable = self.eat_kw("nullable");
        let ty = match self.peek().clone() {
            Tok::Ident(s) => {
                self.bump();
                let t = match s.as_str() {
                    "any" => Ty::Any,
                    "anynonnull" => Ty::AnyNonNull,
                    "none" => Ty::None,
                    "logical" => Ty::Logical,
                    "number" => Ty::Number,
                    "text" => Ty::Text,
                    "date" => Ty::Date,
                    "datetime" => Ty::DateTime,
                    "datetimezone" => Ty::DateTimeZone,
                    "time" => Ty::Time,
                    "duration" => Ty::Duration,
                    "binary" => Ty::Binary,
                    "list" => Ty::List,
                    "record" => Ty::Record,
                    "table" => Ty::Table,
                    "function" => Ty::Function,
                    "type" => Ty::Type,
                    _ => return Err(self.err(&format!("The type '{s}' is not known."))),
                };
                // `table [a = number]` / `record [...]`: the field list is read and not kept.
                if matches!(t, Ty::Table | Ty::Record) && self.is_op("[") {
                    self.skip_group("[", "]")?;
                }
                t
            }
            Tok::Kw("null") => {
                self.bump();
                Ty::Null
            }
            Tok::Op("{") => {
                self.skip_group("{", "}")?;
                Ty::List
            }
            Tok::Op("[") => {
                self.skip_group("[", "]")?;
                Ty::Record
            }
            _ => return Err(self.err("A type is expected.")),
        };
        Ok(TypeSpec { ty, nullable: nullable || matches!(ty, Ty::Null | Ty::Any) })
    }

    fn skip_group(&mut self, open: &str, close: &str) -> R<()> {
        let mut depth = 0;
        loop {
            if self.is_op(open) {
                depth += 1;
            } else if self.is_op(close) {
                depth -= 1;
            } else if matches!(self.peek(), Tok::Eof) {
                return Err(self.err(&format!("Token {close} expected.")));
            }
            self.bump();
            if depth == 0 {
                return Ok(());
            }
        }
    }

    fn bin(op: BinOp, l: Rc<Expr>, r: Rc<Expr>) -> Rc<Expr> {
        Rc::new(Expr::Bin { op, l, r })
    }

    fn coalesce(&mut self) -> R<Rc<Expr>> {
        let l = self.or()?;
        if self.eat_op("??") {
            // right associative
            let r = self.coalesce_rhs()?;
            return Ok(Self::bin(BinOp::Coalesce, l, r));
        }
        Ok(l)
    }
    fn coalesce_rhs(&mut self) -> R<Rc<Expr>> {
        let l = self.or()?;
        if self.eat_op("??") {
            let r = self.coalesce_rhs()?;
            return Ok(Self::bin(BinOp::Coalesce, l, r));
        }
        Ok(l)
    }
    fn or(&mut self) -> R<Rc<Expr>> {
        let mut l = self.and()?;
        while self.eat_kw("or") {
            l = Self::bin(BinOp::Or, l, self.and()?);
        }
        Ok(l)
    }
    fn and(&mut self) -> R<Rc<Expr>> {
        let mut l = self.is_as()?;
        while self.eat_kw("and") {
            l = Self::bin(BinOp::And, l, self.is_as()?);
        }
        Ok(l)
    }
    fn is_as(&mut self) -> R<Rc<Expr>> {
        let mut l = self.equality()?;
        loop {
            if self.eat_kw("is") {
                l = Rc::new(Expr::Is(l, self.type_spec()?));
            } else if self.eat_kw("as") {
                l = Rc::new(Expr::As(l, self.type_spec()?));
            } else {
                return Ok(l);
            }
        }
    }
    fn equality(&mut self) -> R<Rc<Expr>> {
        let mut l = self.relational()?;
        loop {
            let op = if self.is_op("=") {
                BinOp::Eq
            } else if self.is_op("<>") {
                BinOp::Ne
            } else {
                return Ok(l);
            };
            self.bump();
            l = Self::bin(op, l, self.relational()?);
        }
    }
    fn relational(&mut self) -> R<Rc<Expr>> {
        let mut l = self.additive()?;
        loop {
            let op = if self.is_op("<") {
                BinOp::Lt
            } else if self.is_op(">") {
                BinOp::Gt
            } else if self.is_op("<=") {
                BinOp::Le
            } else if self.is_op(">=") {
                BinOp::Ge
            } else {
                return Ok(l);
            };
            self.bump();
            l = Self::bin(op, l, self.additive()?);
        }
    }
    fn additive(&mut self) -> R<Rc<Expr>> {
        let mut l = self.multiplicative()?;
        loop {
            let op = if self.is_op("+") {
                BinOp::Add
            } else if self.is_op("-") {
                BinOp::Sub
            } else if self.is_op("&") {
                BinOp::Concat
            } else {
                return Ok(l);
            };
            self.bump();
            l = Self::bin(op, l, self.multiplicative()?);
        }
    }
    fn multiplicative(&mut self) -> R<Rc<Expr>> {
        let mut l = self.unary()?;
        loop {
            let op = if self.is_op("*") {
                BinOp::Mul
            } else if self.is_op("/") {
                BinOp::Div
            } else {
                return Ok(l);
            };
            self.bump();
            l = Self::bin(op, l, self.unary()?);
        }
    }
    fn unary(&mut self) -> R<Rc<Expr>> {
        if self.eat_op("-") {
            return Ok(Rc::new(Expr::Neg(self.unary()?)));
        }
        if self.eat_op("+") {
            return Ok(Rc::new(Expr::Pos(self.unary()?)));
        }
        if self.eat_kw("not") {
            return Ok(Rc::new(Expr::Not(self.unary()?)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> R<Rc<Expr>> {
        let mut e = self.primary()?;
        loop {
            let line = self.line();
            if self.eat_op("(") {
                let mut args = Vec::new();
                if !self.is_op(")") {
                    loop {
                        args.push(self.expr()?);
                        if !self.eat_op(",") {
                            break;
                        }
                    }
                }
                self.expect_op(")")?;
                e = Rc::new(Expr::Call { f: e, args, line });
            } else if self.eat_op("{") {
                let index = self.expr()?;
                self.expect_op("}")?;
                let optional = self.eat_op("?");
                e = Rc::new(Expr::Index { target: e, index, optional, line });
            } else if self.is_op("[") {
                self.bump();
                let name = self.field_name()?;
                self.expect_op("]")?;
                let optional = self.eat_op("?");
                e = Rc::new(Expr::Field { target: e, name, optional, line });
            } else {
                return Ok(e);
            }
        }
    }

    /// A field name inside `[...]`: an identifier, or a generalized identifier of words separated by blanks (`[Sales Amount]`,
    /// `[Column 1]`; a keyword is a word too). The blanks are read back as one blank each.
    fn field_name(&mut self) -> R<String> {
        let mut parts: Vec<String> = Vec::new();
        loop {
            match self.peek().clone() {
                Tok::Ident(s) => {
                    self.bump();
                    parts.push(s);
                }
                Tok::Kw(k) => {
                    self.bump();
                    parts.push(k.to_string());
                }
                Tok::Num(n) if !parts.is_empty() => {
                    self.bump();
                    parts.push(crate::builtin::number_to_text(n));
                }
                _ => break,
            }
        }
        if parts.is_empty() { Err(self.err("A field name is expected.")) } else { Ok(parts.join(" ")) }
    }

    fn primary(&mut self) -> R<Rc<Expr>> {
        let line = self.line();
        match self.bump() {
            Tok::Num(n) => Ok(Rc::new(Expr::Num(n))),
            Tok::Text(s) => Ok(Rc::new(Expr::Text(s))),
            Tok::Kw("true") => Ok(Rc::new(Expr::Bool(true))),
            Tok::Kw("false") => Ok(Rc::new(Expr::Bool(false))),
            Tok::Kw("null") => Ok(Rc::new(Expr::Null)),
            Tok::Kw("type") => Ok(Rc::new(Expr::TypeOf(self.type_spec()?))),
            Tok::Op("@") => {
                let name = self.ident()?;
                Ok(Rc::new(Expr::Ident { name, line }))
            }
            Tok::Ident(name) => Ok(Rc::new(Expr::Ident { name, line })),
            Tok::Op("(") => {
                let e = self.expr()?;
                self.expect_op(")")?;
                Ok(e)
            }
            Tok::Op("{") => {
                let mut items = Vec::new();
                if !self.is_op("}") {
                    loop {
                        let a = self.expr()?;
                        if self.eat_op("..") {
                            let b = self.expr()?;
                            items.push(ListItem::Range(a, b));
                        } else {
                            items.push(ListItem::One(a));
                        }
                        if !self.eat_op(",") {
                            break;
                        }
                    }
                }
                self.expect_op("}")?;
                Ok(Rc::new(Expr::List(items)))
            }
            Tok::Op("[") => {
                if self.eat_op("]") {
                    if self.eat_op("?") {
                        return Err(self.err("A field name is expected."));
                    }
                    return Ok(Rc::new(Expr::Record(Vec::new())));
                }
                let first = self.field_name()?;
                if self.is_op("]") {
                    self.bump();
                    let optional = self.eat_op("?");
                    return Ok(Rc::new(Expr::ImplicitField { name: first, optional, line }));
                }
                let mut fields = Vec::new();
                let mut name = first;
                loop {
                    self.expect_op("=")?;
                    fields.push((name, self.expr()?));
                    if !self.eat_op(",") {
                        break;
                    }
                    name = self.field_name()?;
                }
                self.expect_op("]")?;
                Ok(Rc::new(Expr::Record(fields)))
            }
            _ => {
                self.pos = self.pos.saturating_sub(1);
                Err(self.err("An expression is expected."))
            }
        }
    }
}
