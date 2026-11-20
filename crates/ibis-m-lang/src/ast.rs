// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The syntax tree of M.
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ty {
    Any,
    AnyNonNull,
    None,
    Null,
    Logical,
    Number,
    /// `Int64.Type`, `Int32.Type`... (a number whose value is whole).
    Int64,
    Int32,
    Currency,
    Percentage,
    Text,
    Date,
    DateTime,
    DateTimeZone,
    Time,
    Duration,
    Binary,
    List,
    Record,
    Table,
    Function,
    Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeSpec {
    pub ty: Ty,
    pub nullable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Concat,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    Coalesce,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub optional: bool,
    pub ty: Option<TypeSpec>,
}

#[derive(Debug, Clone)]
pub enum ListItem {
    One(Rc<Expr>),
    Range(Rc<Expr>, Rc<Expr>),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Null,
    Bool(bool),
    Num(f64),
    Text(String),
    Ident { name: String, line: u32 },
    List(Vec<ListItem>),
    Record(Vec<(String, Rc<Expr>)>),
    /// `target[name]` / `target[name]?`.
    Field { target: Rc<Expr>, name: String, optional: bool, line: u32 },
    /// `[name]` alone: the field of `_`.
    ImplicitField { name: String, optional: bool, line: u32 },
    /// `target{index}` / `target{[k=v]}`.
    Index { target: Rc<Expr>, index: Rc<Expr>, optional: bool, line: u32 },
    Call { f: Rc<Expr>, args: Vec<Rc<Expr>>, line: u32 },
    Neg(Rc<Expr>),
    Pos(Rc<Expr>),
    Not(Rc<Expr>),
    Bin { op: BinOp, l: Rc<Expr>, r: Rc<Expr> },
    Is(Rc<Expr>, TypeSpec),
    As(Rc<Expr>, TypeSpec),
    Let(Vec<(String, Rc<Expr>)>, Rc<Expr>),
    If { c: Rc<Expr>, t: Rc<Expr>, e: Rc<Expr> },
    Each(Rc<Expr>),
    Func { params: Vec<Param>, ret: Option<TypeSpec>, body: Rc<Expr> },
    Error(Rc<Expr>, u32),
    Try { e: Rc<Expr>, otherwise: Option<Rc<Expr>> },
    TypeOf(TypeSpec),
}
