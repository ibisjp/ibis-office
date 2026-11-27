// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Values of M, and the lazy cells (a `let` name, a record field, a list item are evaluated when first used, once).
use crate::ast::{Expr, Param, Ty, TypeSpec};
use crate::error::MError;
use crate::eval::Interp;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub enum Value {
    Null,
    Logical(bool),
    /// One numeric type (design 9.11): a double. `Currency.Type` rounds in its own function.
    Number(f64),
    Text(Rc<str>),
    /// Days since 0001-01-01 (day 0 = 0001-01-01).
    Date(i64),
    /// Days since 0001-01-01 and seconds in the day.
    DateTime(i64, f64),
    /// A datetime with its offset from UTC in minutes: the days since 0001-01-01 and seconds of the day *as written* (local to the offset). Equal and
    /// ordered by the instant (Excel, K0039 second measurement).
    DateTimeZone(i64, f64, i32),
    /// Seconds in the day.
    Time(f64),
    /// Seconds (negative allowed).
    Duration(f64),
    List(Rc<Vec<Lazy>>),
    Record(Rc<Vec<(String, Lazy)>>),
    Table(Rc<Table>),
    Function(Rc<Func>),
    Type(TypeSpec),
    /// A binary that is a file not read yet (`File.Contents(path)`): the path. `Csv.Document` and `Excel.Workbook` read it through the host.
    Binary(Rc<str>),
    /// A list made as it is asked for (`List.Generate`: it may be endless, `List.FirstN(…, 3)` takes three).
    Gen(Rc<Generator>),
    /// A cell of a table that holds an error (a failed type change...). It only lives in `Table::rows`; reading the cell raises the error.
    Failed(Rc<MError>),
}

/// A table value: column names and rows of values (column types are kept in `types`).
#[derive(Clone)]
pub struct Table {
    pub columns: Vec<String>,
    pub types: Vec<TypeSpec>,
    pub rows: Vec<Vec<Value>>,
}

pub type Builtin = fn(&Interp, &[Value]) -> Result<Value, MError>;

pub enum Func {
    Closure { params: Vec<Param>, ret: Option<TypeSpec>, body: Rc<Expr>, env: Rc<Env> },
    Builtin { name: &'static str, min: usize, max: usize, f: Builtin },
    /// A function made at run time by a built-in (`Splitter.SplitTextByDelimiter(",")`...).
    Native { name: &'static str, min: usize, max: usize, f: NativeFn },
}

pub type NativeFn = Rc<dyn Fn(&Interp, &[Value]) -> Result<Value, MError>>;

/// The names of a scope. Lookup goes outward through `parent`.
pub struct Env {
    pub vars: RefCell<Vec<(String, Lazy)>>,
    pub parent: Option<Rc<Env>>,
}

impl Env {
    pub fn child(parent: &Rc<Env>) -> Rc<Env> {
        Rc::new(Env { vars: RefCell::new(Vec::new()), parent: Some(parent.clone()) })
    }
    pub fn root() -> Rc<Env> {
        Rc::new(Env { vars: RefCell::new(Vec::new()), parent: None })
    }
    pub fn define(&self, name: &str, v: Lazy) {
        self.vars.borrow_mut().push((name.to_string(), v));
    }
    pub fn lookup(&self, name: &str) -> Option<Lazy> {
        if let Some((_, v)) = self.vars.borrow().iter().rev().find(|(n, _)| n == name) {
            return Some(v.clone());
        }
        self.parent.as_ref().and_then(|p| p.lookup(name))
    }
}

enum State {
    Pending(Rc<Expr>, Rc<Env>),
    Running,
    Done(Result<Value, MError>),
}

/// A value computed on first use (once). Cloning shares the cell.
#[derive(Clone)]
pub struct Lazy(Rc<RefCell<State>>);

impl Lazy {
    pub fn pending(e: Rc<Expr>, env: Rc<Env>) -> Lazy {
        Lazy(Rc::new(RefCell::new(State::Pending(e, env))))
    }
    pub fn failed(e: Rc<MError>) -> Lazy {
        Lazy(Rc::new(RefCell::new(State::Done(Err((*e).clone())))))
    }
    /// A cell of a table as a lazy value: a failed cell raises its error when read.
    pub fn of_cell(v: Value) -> Lazy {
        match v {
            Value::Failed(e) => Lazy::failed(e),
            v => Lazy::done(v),
        }
    }
    pub fn done(v: Value) -> Lazy {
        Lazy(Rc::new(RefCell::new(State::Done(Ok(v)))))
    }
    pub fn force(&self, it: &Interp) -> Result<Value, MError> {
        let taken = std::mem::replace(&mut *self.0.borrow_mut(), State::Running);
        match taken {
            State::Done(r) => {
                *self.0.borrow_mut() = State::Done(r.clone());
                r
            }
            State::Running => Err(MError::expr("A cyclic reference was encountered during evaluation.")),
            State::Pending(e, env) => {
                let r = it.eval(&e, &env);
                *self.0.borrow_mut() = State::Done(r.clone());
                r
            }
        }
    }
}

impl Value {
    pub fn text(s: impl AsRef<str>) -> Value {
        Value::Text(Rc::from(s.as_ref()))
    }
    pub fn list(items: Vec<Value>) -> Value {
        Value::List(Rc::new(items.into_iter().map(Lazy::done).collect()))
    }
    pub fn record(fields: Vec<(String, Value)>) -> Value {
        Value::Record(Rc::new(fields.into_iter().map(|(n, v)| (n, Lazy::done(v))).collect()))
    }
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Logical(_) => "logical",
            Value::Number(_) => "number",
            Value::Text(_) => "text",
            Value::Date(_) => "date",
            Value::DateTime(..) => "datetime",
            Value::DateTimeZone(..) => "datetimezone",
            Value::Time(_) => "time",
            Value::Duration(_) => "duration",
            Value::List(_) => "list",
            Value::Record(_) => "record",
            Value::Table(_) => "table",
            Value::Function(_) => "function",
            Value::Type(_) => "type",
            Value::Binary(_) => "binary",
            Value::Gen(_) => "list",
            Value::Failed(_) => "error",
        }
    }
    /// Whether the value is of the type (`is`, and the check of `as`).
    pub fn conforms(&self, t: &TypeSpec) -> bool {
        if matches!(self, Value::Failed(_)) {
            return false;
        }
        if matches!(self, Value::Null) {
            return t.nullable || matches!(t.ty, Ty::Any | Ty::Null);
        }
        match t.ty {
            Ty::Any => true,
            Ty::AnyNonNull => true,
            Ty::None | Ty::Null => false,
            Ty::Logical => matches!(self, Value::Logical(_)),
            Ty::Number | Ty::Currency | Ty::Percentage => matches!(self, Value::Number(_)),
            Ty::Int64 | Ty::Int32 => matches!(self, Value::Number(n) if n.fract() == 0.0),
            Ty::Text => matches!(self, Value::Text(_)),
            Ty::Date => matches!(self, Value::Date(_)),
            Ty::DateTime => matches!(self, Value::DateTime(..)),
            Ty::DateTimeZone => matches!(self, Value::DateTimeZone(..)),
            Ty::Time => matches!(self, Value::Time(_)),
            Ty::Duration => matches!(self, Value::Duration(_)),
            Ty::Binary => matches!(self, Value::Binary(_)),
            Ty::List => matches!(self, Value::List(_) | Value::Gen(_)),
            Ty::Record => matches!(self, Value::Record(_)),
            Ty::Table => matches!(self, Value::Table(_)),
            Ty::Function => matches!(self, Value::Function(_)),
            Ty::Type => matches!(self, Value::Type(_)),
        }
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Logical(b) => write!(f, "{b}"),
            Value::Number(n) => write!(f, "{n}"),
            Value::Text(s) => write!(f, "{s:?}"),
            Value::Date(d) => write!(f, "date({d})"),
            Value::DateTime(d, s) => write!(f, "datetime({d},{s})"),
            Value::DateTimeZone(d, s, o) => write!(f, "datetimezone({d},{s},{o})"),
            Value::Time(s) => write!(f, "time({s})"),
            Value::Duration(s) => write!(f, "duration({s})"),
            Value::List(_) => write!(f, "list"),
            Value::Record(_) => write!(f, "record"),
            Value::Table(t) => write!(f, "table({} rows)", t.rows.len()),
            Value::Function(_) => write!(f, "function"),
            Value::Type(t) => write!(f, "type {:?}", t.ty),
            Value::Failed(e) => write!(f, "error({})", e.message),
            Value::Binary(p) => write!(f, "binary({p})"),
            Value::Gen(_) => write!(f, "list"),
        }
    }
}

/// Days since 0001-01-01 of a calendar date (proleptic Gregorian).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    // Howard Hinnant's algorithm: days since 1970-01-01, shifted to 0001-01-01.
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468 + 719162
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z - 719162 + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The state of a lazy list.
struct GenState {
    items: Vec<Value>,
    /// The state to try next (None before the first call of the initial function).
    current: Option<Value>,
    finished: bool,
}

/// `List.Generate`: the items are made when asked for (and kept), each from the state before it.
pub struct Generator {
    initial: Value,
    cond: Value,
    next: Value,
    select: Option<Value>,
    state: RefCell<GenState>,
}

impl Generator {
    pub fn new(initial: Value, cond: Value, next: Value, select: Option<Value>) -> Generator {
        Generator { initial, cond, next, select, state: RefCell::new(GenState { items: Vec::new(), current: None, finished: false }) }
    }

    /// Make items until there are `n` or the list ends.
    pub fn ensure(&self, it: &Interp, n: usize) -> Result<(), MError> {
        loop {
            let (have, finished, current) = {
                let st = self.state.borrow();
                (st.items.len(), st.finished, st.current.clone())
            };
            if have >= n || finished {
                return Ok(());
            }
            if have >= crate::eval::MAX_LIST {
                return Err(MError::limit("The list is too long."));
            }
            let state = match current {
                Some(s) => s,
                None => it.call(&self.initial, vec![])?,
            };
            let go = match it.call(&self.cond, vec![state.clone()])? {
                Value::Logical(b) => b,
                r => return Err(MError::expr(format!("We cannot convert a value of type {} to type Logical.", r.type_name()))),
            };
            if !go {
                self.state.borrow_mut().finished = true;
                return Ok(());
            }
            let item = match &self.select {
                Some(f) => it.call(f, vec![state.clone()])?,
                None => state.clone(),
            };
            let next = it.call(&self.next, vec![state])?;
            let mut st = self.state.borrow_mut();
            st.items.push(item);
            st.current = Some(next);
        }
    }

    /// The first `n` items (fewer if the list ends sooner).
    pub fn take(&self, it: &Interp, n: usize) -> Result<Vec<Value>, MError> {
        self.ensure(it, n)?;
        Ok(self.state.borrow().items.iter().take(n).cloned().collect())
    }

    /// All the items (an endless list stops at the longest list there may be, with an error).
    pub fn all(&self, it: &Interp) -> Result<Vec<Value>, MError> {
        self.ensure(it, crate::eval::MAX_LIST + 1)?;
        Ok(self.state.borrow().items.clone())
    }
}
