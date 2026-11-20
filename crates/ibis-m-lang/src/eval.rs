// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The evaluator: lazy (a name is evaluated when used, once), errors as values (`try`), bounded in depth and in steps.
use crate::ast::*;
use crate::error::{ErrKind, MError};
use crate::value::*;
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;

/// How deep the evaluation may nest (a runaway recursion becomes an error instead of overflowing the stack).
pub const MAX_DEPTH: u32 = 160;
/// How many expressions one run may evaluate.
pub const MAX_STEPS: u64 = 50_000_000;
/// The longest list a generator makes.
pub const MAX_LIST: usize = 5_000_000;

const LIBRARY_PREFIXES: &[&str] = &[
    "Table.", "List.", "Text.", "Number.", "Date.", "DateTime.", "DateTimeZone.", "Time.", "Duration.", "Record.", "Value.", "Logical.", "Combiner.", "Splitter.", "Comparer.", "Replacer.", "Excel.", "Csv.", "File.", "Web.", "Sql.", "Json.", "Binary.", "Uri.", "Error.", "Function.", "Int64.", "Table", "Currency.", "Percentage.", "Character.", "Lines.", "Expression.", "Access.", "OData.", "Folder.", "Xml.", "Html.", "Odbc.", "OleDb.", "Parquet.", "Pdf.", "Guid.", "Variable.", "Diagnostics.",
];

pub struct Interp {
    pub(crate) lib: HashMap<&'static str, Value>,
    depth: Cell<u32>,
    steps: Cell<u64>,
    globals: std::cell::RefCell<HashMap<String, Value>>,
    culture: Cell<crate::culture::Culture>,
    now: Cell<Option<(i64, f64)>>,
    host: std::cell::RefCell<Option<Rc<dyn crate::host::Host>>>,
}

impl Default for Interp {
    fn default() -> Self {
        Self::new()
    }
}

impl Interp {
    pub fn new() -> Self {
        Interp { lib: crate::builtin::library(), depth: Cell::new(0), steps: Cell::new(0), globals: Default::default(), culture: Cell::new(crate::culture::Culture::Ja), now: Cell::new(None), host: Default::default() }
    }

    /// The culture the conversions use when a call names none (`Culture.Current`): ja-JP unless the host says otherwise.
    pub fn culture(&self) -> crate::culture::Culture {
        self.culture.get()
    }
    pub fn set_culture(&self, c: crate::culture::Culture) {
        self.culture.set(c);
    }

    /// The local date and time of the run (days since 0001-01-01 and seconds of the day), for `DateTime.LocalNow` and the `Date.IsIn…`
    /// functions. The host gives it (this crate reads no clock, so that it runs in the browser); it is the same all through a run, as
    /// `DateTime.FixedLocalNow` is.
    pub fn set_now(&self, day: i64, seconds: f64) {
        self.now.set(Some((day, seconds)));
    }
    pub fn now(&self) -> Result<(i64, f64), MError> {
        self.now.get().ok_or_else(|| MError::expr("The current time is not available."))
    }

    /// What reads files for `Csv.Document` and `Excel.Workbook`.
    pub fn set_host(&self, host: Rc<dyn crate::host::Host>) {
        *self.host.borrow_mut() = Some(host);
    }
    pub fn host(&self) -> Result<Rc<dyn crate::host::Host>, MError> {
        self.host.borrow().clone().ok_or_else(|| MError::expr("Files cannot be read here."))
    }

    /// Give the host's data a name the M can reach (`#workbook`: a table of Name and Content for `Excel.CurrentWorkbook()`).
    pub fn set_global(&self, name: &str, v: Value) {
        self.globals.borrow_mut().insert(name.to_string(), v);
    }
    pub fn global(&self, name: &str) -> Option<Value> {
        self.globals.borrow().get(name).cloned()
    }

    /// Parse and evaluate a whole M expression (a `let ... in ...`).
    pub fn run(&self, src: &str) -> Result<Value, MError> {
        let e = crate::parse::parse(src)?;
        self.depth.set(0);
        self.steps.set(0);
        self.eval(&e, &Env::root())
    }

    pub fn eval(&self, e: &Rc<Expr>, env: &Rc<Env>) -> Result<Value, MError> {
        let d = self.depth.get();
        if d >= MAX_DEPTH {
            return Err(MError::limit("The evaluation is nested too deeply."));
        }
        let s = self.steps.get() + 1;
        if s > MAX_STEPS {
            return Err(MError::limit("The evaluation took too many steps."));
        }
        self.steps.set(s);
        self.depth.set(d + 1);
        let r = self.eval_inner(e, env);
        self.depth.set(d);
        r
    }

    fn eval_inner(&self, e: &Rc<Expr>, env: &Rc<Env>) -> Result<Value, MError> {
        match &**e {
            Expr::Null => Ok(Value::Null),
            Expr::Bool(b) => Ok(Value::Logical(*b)),
            Expr::Num(n) => Ok(Value::Number(*n)),
            Expr::Text(s) => Ok(Value::text(s)),
            Expr::Ident { name, line } => self.ident(name, *line, env),
            Expr::List(items) => {
                let mut out = Vec::new();
                for it in items {
                    match it {
                        ListItem::One(x) => out.push(Lazy::pending(x.clone(), env.clone())),
                        ListItem::Range(a, b) => {
                            let (a, b) = (self.eval(a, env)?, self.eval(b, env)?);
                            match (a, b) {
                                (Value::Number(a), Value::Number(b)) => {
                                    if b - a > MAX_LIST as f64 {
                                        return Err(MError::limit("The list is too long."));
                                    }
                                    let mut x = a;
                                    while x <= b {
                                        out.push(Lazy::done(Value::Number(x)));
                                        x += 1.0;
                                    }
                                }
                                // A range of one-character texts, `{"a".."z"}` (the characters in code point order).
                                (Value::Text(a), Value::Text(b)) if a.chars().count() == 1 && b.chars().count() == 1 => {
                                    let (a, b) = (a.chars().next().unwrap_or('\0') as u32, b.chars().next().unwrap_or('\0') as u32);
                                    if b.saturating_sub(a) as usize > MAX_LIST {
                                        return Err(MError::limit("The list is too long."));
                                    }
                                    for c in a..=b {
                                        if let Some(ch) = char::from_u32(c) {
                                            out.push(Lazy::done(Value::text(ch.to_string())));
                                        }
                                    }
                                }
                                _ => return Err(MError::expr("The ends of a range must be numbers or one-character texts.")),
                            }
                        }
                    }
                }
                Ok(Value::List(Rc::new(out)))
            }
            Expr::Record(fields) => {
                let renv = Env::child(env);
                let mut cells = Vec::new();
                for (n, x) in fields {
                    if cells.iter().any(|(m, _): &(String, Lazy)| m == n) {
                        return Err(MError::expr(format!("The field '{n}' of the record was defined more than once.")));
                    }
                    let l = Lazy::pending(x.clone(), renv.clone());
                    renv.define(n, l.clone());
                    cells.push((n.clone(), l));
                }
                Ok(Value::Record(Rc::new(cells)))
            }
            Expr::Field { target, name, optional, .. } => {
                let t = self.eval(target, env)?;
                self.field(&t, name, *optional)
            }
            Expr::ImplicitField { name, optional, .. } => {
                let t = self.ident("_", 0, env)?;
                self.field(&t, name, *optional)
            }
            Expr::Index { target, index, optional, .. } => {
                let t = self.eval(target, env)?;
                let i = self.eval(index, env)?;
                self.index(&t, &i, *optional)
            }
            Expr::Call { f, args, line } => {
                let fv = self.eval(f, env)?;
                let args: Vec<Lazy> = args.iter().map(|a| Lazy::pending(a.clone(), env.clone())).collect();
                self.call_lazy(&fv, args, *line)
            }
            Expr::Neg(x) => match self.eval(x, env)? {
                Value::Number(n) => Ok(Value::Number(-n)),
                Value::Duration(s) => Ok(Value::Duration(-s)),
                Value::Null => Ok(Value::Null),
                v => Err(op_err("-", &v, None)),
            },
            Expr::Pos(x) => match self.eval(x, env)? {
                v @ (Value::Number(_) | Value::Duration(_) | Value::Null) => Ok(v),
                v => Err(op_err("+", &v, None)),
            },
            Expr::Not(x) => match self.eval(x, env)? {
                Value::Logical(b) => Ok(Value::Logical(!b)),
                Value::Null => Ok(Value::Null),
                v => Err(MError::expr(format!("We cannot apply operator not to type {}.", v.type_name()))),
            },
            Expr::Bin { op, l, r } => self.binary(*op, l, r, env),
            Expr::Is(x, t) => {
                let v = self.eval(x, env)?;
                Ok(Value::Logical(v.conforms(t)))
            }
            Expr::As(x, t) => {
                let v = self.eval(x, env)?;
                if v.conforms(t) { Ok(v) } else { Err(MError::expr(format!("We cannot convert the value of type {} to type {:?}.", v.type_name(), t.ty))) }
            }
            Expr::Let(binds, body) => {
                let lenv = Env::child(env);
                for (n, x) in binds {
                    lenv.define(n, Lazy::pending(x.clone(), lenv.clone()));
                }
                self.eval(body, &lenv)
            }
            Expr::If { c, t, e: el } => match self.eval(c, env)? {
                Value::Logical(true) => self.eval(t, env),
                Value::Logical(false) => self.eval(el, env),
                v => Err(MError::expr(format!("We cannot convert a value of type {} to type Logical.", v.type_name()))),
            },
            Expr::Each(body) => Ok(Value::Function(Rc::new(Func::Closure {
                params: vec![Param { name: "_".into(), optional: false, ty: None }],
                ret: None,
                body: body.clone(),
                env: env.clone(),
            }))),
            Expr::Func { params, ret, body } => Ok(Value::Function(Rc::new(Func::Closure {
                params: params.clone(),
                ret: ret.clone(),
                body: body.clone(),
                env: env.clone(),
            }))),
            Expr::Error(x, _) => {
                let v = self.eval(x, env)?;
                Err(self.error_from(&v)?)
            }
            Expr::Try { e: x, otherwise } => match self.eval(x, env) {
                Ok(v) => match otherwise {
                    Some(_) => Ok(v),
                    None => Ok(Value::record(vec![("HasError".into(), Value::Logical(false)), ("Value".into(), v)])),
                },
                Err(err) if matches!(err.kind, ErrKind::Raised) => match otherwise {
                    Some(o) => self.eval(o, env),
                    None => Ok(Value::record(vec![("HasError".into(), Value::Logical(true)), ("Error".into(), error_record(&err))])),
                },
                Err(err) => Err(err),
            },
            Expr::TypeOf(t) => Ok(Value::Type(t.clone())),
        }
    }

    fn ident(&self, name: &str, line: u32, env: &Rc<Env>) -> Result<Value, MError> {
        if let Some(l) = env.lookup(name) {
            return l.force(self);
        }
        // Names the host gave (the tables of the steps before, by their step names) come before the library.
        if let Some(v) = self.globals.borrow().get(name) {
            return Ok(v.clone());
        }
        if let Some(v) = self.lib.get(name) {
            return Ok(v.clone());
        }
        if LIBRARY_PREFIXES.iter().any(|p| name.starts_with(p)) && name.contains('.') || name.starts_with('#') {
            return Err(MError::unsupported(name, line));
        }
        Err(MError::expr(format!("The name '{name}' wasn't recognized.  Make sure it's spelled correctly.")))
    }

    fn error_from(&self, v: &Value) -> Result<MError, MError> {
        match v {
            Value::Text(s) => Ok(MError { kind: ErrKind::Raised, line: 0, reason: "Expression.Error".into(), message: s.to_string(), detail: Value::Null }),
            Value::Record(fields) => {
                let get = |n: &str| -> Result<Value, MError> {
                    match fields.iter().find(|(m, _)| m == n) {
                        Some((_, l)) => l.force(self),
                        None => Ok(Value::Null),
                    }
                };
                let text = |v: Value| match v {
                    Value::Text(s) => s.to_string(),
                    _ => String::new(),
                };
                Ok(MError { kind: ErrKind::Raised, line: 0, reason: text(get("Reason")?), message: text(get("Message")?), detail: get("Detail")? })
            }
            _ => Err(MError::expr("error expects a text or a record.")),
        }
    }

    pub fn field(&self, t: &Value, name: &str, optional: bool) -> Result<Value, MError> {
        match t {
            Value::Record(fields) => match fields.iter().find(|(n, _)| n == name) {
                Some((_, l)) => l.force(self),
                None if optional => Ok(Value::Null),
                None => Err(MError::expr(format!("The field '{name}' of the record wasn't found."))),
            },
            Value::Table(tb) => match tb.columns.iter().position(|c| c == name) {
                Some(i) => Ok(Value::List(Rc::new(tb.rows.iter().map(|r| Lazy::of_cell(r[i].clone())).collect()))),
                None if optional => Ok(Value::Null),
                None => Err(MError::expr(format!("The column '{name}' of the table wasn't found."))),
            },
            Value::Null if optional => Ok(Value::Null),
            v => Err(MError::expr(format!("We cannot access the field '{name}' of a value of type {}.", v.type_name()))),
        }
    }

    fn index(&self, t: &Value, i: &Value, optional: bool) -> Result<Value, MError> {
        let miss = |msg: String| if optional { Ok(Value::Null) } else { Err(MError::expr(msg)) };
        match (t, i) {
            (Value::List(items), Value::Number(n)) => {
                if *n < 0.0 || n.fract() != 0.0 {
                    return Err(MError::expr("The list index must be a non-negative whole number."));
                }
                match items.get(*n as usize) {
                    Some(l) => l.force(self),
                    None => miss(format!("There weren't enough elements in the enumeration to complete the operation (index {n}).")),
                }
            }
            (Value::Gen(g), Value::Number(n)) => {
                if *n < 0.0 || n.fract() != 0.0 {
                    return Err(MError::expr("The list index must be a non-negative whole number."));
                }
                let items = g.take(self, *n as usize + 1)?;
                match items.get(*n as usize) {
                    Some(v) => Ok(v.clone()),
                    None => miss(format!("There weren't enough elements in the enumeration to complete the operation (index {n}).")),
                }
            }
            (Value::Table(tb), Value::Number(n)) => match tb.rows.get(*n as usize) {
                Some(r) if *n >= 0.0 => Ok(row_record(tb, r)),
                _ => miss(format!("There weren't enough rows in the table (index {n}).")),
            },
            (Value::Table(tb), Value::Record(want)) => {
                for r in &tb.rows {
                    let mut ok = true;
                    for (k, l) in want.iter() {
                        let wv = l.force(self)?;
                        match tb.columns.iter().position(|c| c == k) {
                            Some(ci) if self.equals(&r[ci], &wv)? => {}
                            _ => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        return Ok(row_record(tb, r));
                    }
                }
                miss("The key didn't match any rows in the table.".into())
            }
            (Value::Null, _) if optional => Ok(Value::Null),
            (t, i) => Err(MError::expr(format!("We cannot index a value of type {} with a value of type {}.", t.type_name(), i.type_name()))),
        }
    }

    pub fn call(&self, f: &Value, args: Vec<Value>) -> Result<Value, MError> {
        self.call_lazy(f, args.into_iter().map(Lazy::done).collect(), 0)
    }

    pub fn call_lazy(&self, f: &Value, args: Vec<Lazy>, line: u32) -> Result<Value, MError> {
        let Value::Function(func) = f else {
            return Err(MError::expr(format!("We cannot call a value of type {}.", f.type_name())));
        };
        match &**func {
            Func::Builtin { name, min, max, f } => {
                if args.len() < *min || args.len() > *max {
                    return Err(MError::expr(format!("{name}: {} arguments were passed to a function which expects {}.", args.len(), if min == max { min.to_string() } else { format!("{min} to {max}") })));
                }
                let mut vs = Vec::with_capacity(args.len());
                for a in &args {
                    vs.push(a.force(self)?);
                }
                f(self, &vs).map_err(|mut e| {
                    if e.line == 0 {
                        e.line = line;
                    }
                    // An unsupported thing met inside the call is told with the line of the call.
                    if let ErrKind::Unsupported { line: at, .. } = &mut e.kind
                        && *at == 0
                    {
                        *at = line;
                    }
                    e
                })
            }
            Func::Native { name, min, max, f } => {
                if args.len() < *min || args.len() > *max {
                    return Err(MError::expr(format!("{name}: {} arguments were passed to a function which expects {min} to {max}.", args.len())));
                }
                let mut vs = Vec::with_capacity(args.len());
                for a in &args {
                    vs.push(a.force(self)?);
                }
                f(self, &vs)
            }
            Func::Closure { params, ret, body, env } => {
                let required = params.iter().filter(|p| !p.optional).count();
                if args.len() < required || args.len() > params.len() {
                    return Err(MError::expr(format!("{} arguments were passed to a function which expects {}.", args.len(), params.len())));
                }
                let fenv = Env::child(env);
                for (i, p) in params.iter().enumerate() {
                    let l = args.get(i).cloned().unwrap_or_else(|| Lazy::done(Value::Null));
                    if let Some(t) = &p.ty
                        && (i < args.len())
                    {
                        let v = l.force(self)?;
                        if !v.conforms(t) {
                            return Err(MError::expr(format!("We cannot convert the value of type {} to type {:?} (argument '{}').", v.type_name(), t.ty, p.name)));
                        }
                    }
                    fenv.define(&p.name, l);
                }
                let r = self.eval(body, &fenv)?;
                if let Some(t) = ret
                    && !r.conforms(t)
                {
                    return Err(MError::expr(format!("The function returned a value of type {}, not {:?}.", r.type_name(), t.ty)));
                }
                Ok(r)
            }
        }
    }

    fn binary(&self, op: BinOp, l: &Rc<Expr>, r: &Rc<Expr>, env: &Rc<Env>) -> Result<Value, MError> {
        match op {
            BinOp::And | BinOp::Or => {
                let a = self.eval(l, env)?;
                let is_and = op == BinOp::And;
                match a {
                    Value::Logical(x) if x != is_and => return Ok(Value::Logical(x)), // false and _ / true or _
                    Value::Logical(_) | Value::Null => {}
                    v => return Err(op_err(if is_and { "and" } else { "or" }, &v, None)),
                }
                let b = self.eval(r, env)?;
                match (a, b) {
                    (_, Value::Logical(y)) if y != is_and => Ok(Value::Logical(y)),
                    (Value::Logical(_), b @ Value::Logical(_)) => Ok(b),
                    (_, Value::Null) | (Value::Null, Value::Logical(_)) => Ok(Value::Null),
                    (_, b) => Err(op_err(if is_and { "and" } else { "or" }, &b, None)),
                }
            }
            BinOp::Coalesce => match self.eval(l, env)? {
                Value::Null => self.eval(r, env),
                v => Ok(v),
            },
            _ => {
                let a = self.eval(l, env)?;
                let b = self.eval(r, env)?;
                self.arith(op, a, b)
            }
        }
    }

    fn arith(&self, op: BinOp, a: Value, b: Value) -> Result<Value, MError> {
        use Value::*;
        match op {
            BinOp::Eq => return Ok(Logical(self.equals(&a, &b)?)),
            BinOp::Ne => return Ok(Logical(!self.equals(&a, &b)?)),
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                if matches!(a, Null) || matches!(b, Null) {
                    return Ok(Null);
                }
                let o = self.compare(&a, &b)?.ok_or_else(|| op_err("comparison", &a, Some(&b)))?;
                return Ok(Logical(match op {
                    BinOp::Lt => o == Ordering::Less,
                    BinOp::Gt => o == Ordering::Greater,
                    BinOp::Le => o != Ordering::Greater,
                    _ => o != Ordering::Less,
                }));
            }
            BinOp::Concat => {
                return match (&a, &b) {
                    (Null, _) | (_, Null) => Ok(Null),
                    (Text(x), Text(y)) => Ok(Value::text(format!("{x}{y}"))),
                    (List(x), List(y)) => Ok(List(Rc::new(x.iter().chain(y.iter()).cloned().collect()))),
                    (Record(x), Record(y)) => {
                        let mut out: Vec<(String, Lazy)> = x.to_vec();
                        for (n, l) in y.iter() {
                            match out.iter_mut().find(|(m, _)| m == n) {
                                Some(slot) => slot.1 = l.clone(),
                                None => out.push((n.clone(), l.clone())),
                            }
                        }
                        Ok(Record(Rc::new(out)))
                    }
                    (Date(d), Time(t)) => Ok(DateTime(*d, *t)),
                    _ => Err(op_err("&", &a, Some(&b))),
                };
            }
            _ => {}
        }
        if matches!(a, Null) || matches!(b, Null) {
            return Ok(Null);
        }
        const DAY: f64 = 86400.0;
        match (op, &a, &b) {
            (BinOp::Add, Number(x), Number(y)) => Ok(Number(x + y)),
            (BinOp::Sub, Number(x), Number(y)) => Ok(Number(x - y)),
            (BinOp::Mul, Number(x), Number(y)) => Ok(Number(x * y)),
            (BinOp::Div, Number(x), Number(y)) => Ok(Number(x / y)),
            (BinOp::Add, Duration(x), Duration(y)) => Ok(Duration(x + y)),
            (BinOp::Sub, Duration(x), Duration(y)) => Ok(Duration(x - y)),
            (BinOp::Mul, Duration(x), Number(y)) | (BinOp::Mul, Number(y), Duration(x)) => Ok(Duration(x * y)),
            (BinOp::Div, Duration(x), Number(y)) => Ok(Duration(x / y)),
            (BinOp::Div, Duration(x), Duration(y)) => Ok(Number(x / y)),
            (BinOp::Add, Date(d), Duration(s)) | (BinOp::Add, Duration(s), Date(d)) => Ok(Date(d + (s / DAY).floor() as i64)),
            (BinOp::Sub, Date(d), Duration(s)) => Ok(Date(d - (s / DAY).floor() as i64)),
            (BinOp::Sub, Date(x), Date(y)) => Ok(Duration((x - y) as f64 * DAY)),
            (BinOp::Add, DateTime(d, t), Duration(s)) | (BinOp::Add, Duration(s), DateTime(d, t)) => {
                let total = *t + s;
                Ok(DateTime(d + total.div_euclid(DAY) as i64, total.rem_euclid(DAY)))
            }
            (BinOp::Sub, DateTime(d, t), Duration(s)) => {
                let total = *t - s;
                Ok(DateTime(d + total.div_euclid(DAY) as i64, total.rem_euclid(DAY)))
            }
            (BinOp::Sub, DateTime(d1, t1), DateTime(d2, t2)) => Ok(Duration((d1 - d2) as f64 * DAY + t1 - t2)),
            (BinOp::Sub, Time(x), Time(y)) => Ok(Duration(x - y)),
            (BinOp::Add, Time(t), Duration(s)) | (BinOp::Add, Duration(s), Time(t)) => Ok(Time((t + s).rem_euclid(DAY))),
            (BinOp::Sub, Time(t), Duration(s)) => Ok(Time((t - s).rem_euclid(DAY))),
            _ => Err(op_err(
                match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    _ => "/",
                },
                &a,
                Some(&b),
            )),
        }
    }

    /// M equality (`=`): same type and equal; lists and records element by element (a record ignores the field order).
    pub fn equals(&self, a: &Value, b: &Value) -> Result<bool, MError> {
        use Value::*;
        Ok(match (a, b) {
            (Null, Null) => true,
            (Logical(x), Logical(y)) => x == y,
            (Number(x), Number(y)) => x == y,
            (Text(x), Text(y)) => x == y,
            (Date(x), Date(y)) => x == y,
            (DateTime(d1, t1), DateTime(d2, t2)) => d1 == d2 && t1 == t2,
            (Time(x), Time(y)) | (Duration(x), Duration(y)) => x == y,
            (List(x), List(y)) => {
                if x.len() != y.len() {
                    return Ok(false);
                }
                for (p, q) in x.iter().zip(y.iter()) {
                    if !self.equals(&p.force(self)?, &q.force(self)?)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Record(x), Record(y)) => {
                if x.len() != y.len() {
                    return Ok(false);
                }
                for (n, p) in x.iter() {
                    match y.iter().find(|(m, _)| m == n) {
                        Some((_, q)) if self.equals(&p.force(self)?, &q.force(self)?)? => {}
                        _ => return Ok(false),
                    }
                }
                true
            }
            (Table(x), Table(y)) => {
                if x.columns != y.columns || x.rows.len() != y.rows.len() {
                    return Ok(false);
                }
                for (r, s) in x.rows.iter().zip(y.rows.iter()) {
                    for (p, q) in r.iter().zip(s.iter()) {
                        if !self.equals(p, q)? {
                            return Ok(false);
                        }
                    }
                }
                true
            }
            (Type(x), Type(y)) => x == y,
            (Binary(x), Binary(y)) => x == y,
            (Gen(_), _) | (_, Gen(_)) => {
                let (x, y) = (crate::builtin::list_items(self, a)?, crate::builtin::list_items(self, b)?);
                return self.equals(&Value::list(x), &Value::list(y));
            }
            _ => false,
        })
    }

    /// The order of two values of the same type (text by ordinal comparison, as M's `<` does).
    pub fn compare(&self, a: &Value, b: &Value) -> Result<Option<Ordering>, MError> {
        use Value::*;
        Ok(match (a, b) {
            // NaN is smaller than every number (and equal to itself) in an order: `#nan < 1` is true, `#nan <= #nan` is true (Excel).
            (Number(x), Number(y)) => Some(match (x.is_nan(), y.is_nan()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => x.partial_cmp(y).unwrap_or(Ordering::Equal),
            }),
            (Text(x), Text(y)) => Some(x.encode_utf16().cmp(y.encode_utf16())),
            (Logical(x), Logical(y)) => Some(x.cmp(y)),
            (Date(x), Date(y)) => Some(x.cmp(y)),
            (DateTime(d1, t1), DateTime(d2, t2)) => Some(d1.cmp(d2).then(t1.partial_cmp(t2).unwrap_or(Ordering::Equal))),
            (Time(x), Time(y)) | (Duration(x), Duration(y)) => x.partial_cmp(y),
            _ => None,
        })
    }
}

fn op_err(op: &str, a: &Value, b: Option<&Value>) -> MError {
    match b {
        Some(b) => MError::expr(format!("Operator {op} cannot be applied to operands of type {} and {}.", a.type_name(), b.type_name())),
        None => MError::expr(format!("Operator {op} cannot be applied to an operand of type {}.", a.type_name())),
    }
}

pub fn error_record(e: &MError) -> Value {
    Value::record(vec![("Reason".into(), Value::text(&e.reason)), ("Message".into(), Value::text(&e.message)), ("Detail".into(), e.detail.clone())])
}

pub fn row_record(t: &Table, row: &[Value]) -> Value {
    Value::Record(Rc::new(t.columns.iter().cloned().zip(row.iter().cloned().map(Lazy::of_cell)).collect()))
}
