// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > ソルバー (Excel's Solver): values for the variable cells that
//! make the objective cell greatest, least or a given value while the
//! constraints hold.
//!
//! The model is tried on a copy of the book: the variable cells set, the
//! book calculated, the objective and constraint cells read. A model that
//! turns out to be linear (probed at the start and at another point) is
//! solved exactly by the simplex method (with branch and bound for whole
//! numbers); the others by a penalty search (GRG Nonlinear) or by differential
//! evolution (Evolutionary).

use crate::command::{Command, Document};
use crate::model::{Presentation, SlideId};
use crate::sheet::CellValue;
use crate::sheet_command::{CellEdit, CellInput, SheetCommand};

/// What to do with the objective cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Goal {
    Max,
    Min,
    Value(f64),
}

/// The relation of a constraint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rel {
    Le,
    Eq,
    Ge,
    Int,
    Bin,
    /// All different (whole numbers, each once).
    Dif,
}

/// The right side of a constraint.
#[derive(Clone, Debug, PartialEq)]
pub enum Rhs {
    Number(f64),
    /// A cell, or a range of the same size as the left side.
    Cells(Vec<(u32, u32)>),
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Constraint {
    /// The left side's cells (row, column), in order.
    pub lhs: Vec<(u32, u32)>,
    pub rel: Rel,
    pub rhs: Rhs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// GRG Nonlinear.
    Grg,
    /// Simplex LP.
    Simplex,
    Evolutionary,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub objective: (u32, u32),
    pub goal: Goal,
    /// The variable cells (row, column), in order.
    pub variables: Vec<(u32, u32)>,
    pub constraints: Vec<Constraint>,
    /// Make unconstrained variables non-negative.
    pub non_negative: bool,
    pub method: Method,
    /// How close a constraint must be met.
    pub precision: f64,
    /// The stopping change of the nonlinear search.
    pub convergence: f64,
    /// Branch and bound stops within this fraction of the best.
    pub integer_tolerance: f64,
    /// The most steps (0: no limit).
    pub max_iterations: u32,
    /// Seed of the evolutionary search.
    pub seed: u64,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            objective: (0, 0),
            goal: Goal::Max,
            variables: Vec::new(),
            constraints: Vec::new(),
            non_negative: true,
            method: Method::Grg,
            precision: 1e-6,
            convergence: 1e-4,
            integer_tolerance: 0.01,
            max_iterations: 0,
            seed: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// A solution; the constraints and optimality hold.
    Optimal,
    /// The search converged on this solution; the constraints hold.
    Converged,
    /// No values meet the constraints.
    Infeasible,
    /// The objective has no limit.
    Unbounded,
    /// Stopped at the limit of steps; the constraints hold.
    IterationLimit,
    /// Stopped by the caller (time).
    Stopped,
    /// The Simplex method on a model that is not linear.
    NotLinear,
    /// The Evolutionary method needs a lower and an upper bound on every variable.
    MissingBounds,
    /// A cell the model uses is not a number, or the model is incomplete.
    BadModel,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Solution {
    pub status: Status,
    /// The values of the variable cells.
    pub values: Vec<f64>,
    /// The objective cell's value there.
    pub objective: f64,
}

/// One setting of the variable cells and what it gives.
#[derive(Clone, Debug)]
struct Point {
    objective: f64,
    /// For each scalar constraint, left minus right.
    gaps: Vec<f64>,
}

/// A constraint of one left cell.
#[derive(Clone, Copy, Debug)]
struct Scalar {
    rel: Rel,
    lhs: (u32, u32),
    rhs_cell: Option<(u32, u32)>,
    rhs_number: f64,
}

struct Harness {
    doc: Document,
    slide: SlideId,
    model: Model,
    scalars: Vec<Scalar>,
    /// For each variable: whole numbers only.
    integer: Vec<bool>,
    /// For each variable: bounds from the constraints that name it alone.
    bounds: Vec<(f64, f64)>,
    evaluations: usize,
    all_different: Vec<Vec<usize>>,
}

fn read(doc: &Document, slide: SlideId, (row, column): (u32, u32)) -> f64 {
    let sheet = doc.presentation().slide(slide).and_then(crate::sheet_calc::sheet_of_slide);
    match sheet.and_then(|s| s.cell(row, column)).map(|c| &c.value) {
        Some(CellValue::Number(n)) => *n,
        Some(CellValue::Bool(b)) => f64::from(*b),
        Some(CellValue::Empty) | None => 0.0,
        _ => f64::NAN,
    }
}

impl Harness {
    fn new(presentation: &Presentation, slide: SlideId, model: &Model) -> Option<Self> {
        let mut scalars = Vec::new();
        let mut integer = vec![false; model.variables.len()];
        let mut bounds = vec![(if model.non_negative { 0.0 } else { f64::NEG_INFINITY }, f64::INFINITY); model.variables.len()];
        let mut all_different = Vec::new();
        let index_of = |cell: &(u32, u32)| model.variables.iter().position(|v| v == cell);
        for c in &model.constraints {
            match c.rel {
                Rel::Int | Rel::Bin => {
                    for cell in &c.lhs {
                        if let Some(i) = index_of(cell) {
                            integer[i] = true;
                            if c.rel == Rel::Bin {
                                bounds[i] = (0.0, 1.0);
                            }
                        }
                    }
                }
                Rel::Dif => {
                    let members: Vec<usize> = c.lhs.iter().filter_map(index_of).collect();
                    for &i in &members {
                        integer[i] = true;
                    }
                    all_different.push(members);
                }
                Rel::Le | Rel::Eq | Rel::Ge => {
                    for (k, cell) in c.lhs.iter().enumerate() {
                        let (rhs_cell, rhs_number) = match &c.rhs {
                            Rhs::Number(n) => (None, *n),
                            Rhs::Cells(cells) if cells.len() == 1 => (Some(cells[0]), 0.0),
                            Rhs::Cells(cells) => (Some(*cells.get(k)?), 0.0),
                            Rhs::None => return None,
                        };
                        scalars.push(Scalar { rel: c.rel, lhs: *cell, rhs_cell, rhs_number });
                        // A variable named alone against a number is a bound.
                        if let (Some(i), None) = (index_of(cell), rhs_cell) {
                            let (lo, hi) = bounds[i];
                            match c.rel {
                                Rel::Le => bounds[i].1 = hi.min(rhs_number),
                                Rel::Ge => bounds[i].0 = lo.max(rhs_number),
                                _ => bounds[i] = (rhs_number, rhs_number),
                            }
                        }
                    }
                }
            }
        }
        Some(Self { doc: Document::new(presentation.clone()), slide, model: model.clone(), scalars, integer, bounds, evaluations: 0, all_different })
    }

    fn set(&mut self, x: &[f64]) -> bool {
        let edits = self.model.variables.iter().zip(x).map(|(&(row, column), v)| CellEdit { row, column, input: CellInput::Value(CellValue::Number(*v)) }).collect();
        self.doc.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: self.slide, edits })]).is_ok()
    }

    fn eval(&mut self, x: &[f64]) -> Option<Point> {
        self.evaluations += 1;
        if !self.set(x) {
            return None;
        }
        let objective = read(&self.doc, self.slide, self.model.objective);
        if objective.is_nan() {
            return None;
        }
        let mut gaps = Vec::with_capacity(self.scalars.len());
        for s in &self.scalars {
            let rhs = s.rhs_cell.map_or(s.rhs_number, |c| read(&self.doc, self.slide, c));
            let gap = read(&self.doc, self.slide, s.lhs) - rhs;
            if gap.is_nan() {
                return None;
            }
            gaps.push(gap);
        }
        Some(Point { objective, gaps })
    }

    /// How far the point is from meeting the constraints (0: it does).
    fn violation(&self, p: &Point, x: &[f64]) -> f64 {
        let mut v = 0.0;
        for (s, g) in self.scalars.iter().zip(&p.gaps) {
            v += match s.rel {
                Rel::Le => g.max(0.0),
                Rel::Ge => (-g).max(0.0),
                _ => g.abs(),
            };
        }
        for (i, xi) in x.iter().enumerate() {
            let (lo, hi) = self.bounds[i];
            v += (lo - xi).max(0.0) + (xi - hi).max(0.0);
            if self.integer[i] {
                v += (xi - xi.round()).abs();
            }
        }
        for group in &self.all_different {
            for (a, &i) in group.iter().enumerate() {
                for &j in &group[a + 1..] {
                    if (x[i].round() - x[j].round()).abs() < 0.5 {
                        v += 1.0;
                    }
                }
            }
        }
        v
    }

    fn tolerance(&self, p: &Point) -> f64 {
        self.model.precision * (1.0 + p.gaps.iter().fold(0.0f64, |m, g| m.max(g.abs())).min(1e6))
    }
}

// ---------------------------------------------------------------------- LP

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rel3 {
    Le,
    Eq,
    Ge,
}

#[derive(Clone, Debug)]
struct LpRow {
    a: Vec<f64>,
    rel: Rel3,
    b: f64,
}

#[derive(Clone, Debug)]
struct Lp {
    /// Minimized.
    c: Vec<f64>,
    rows: Vec<LpRow>,
    lo: Vec<f64>,
    hi: Vec<f64>,
    int: Vec<bool>,
}

#[derive(Clone, Debug)]
enum LpOutcome {
    Optimal(Vec<f64>, f64),
    Infeasible,
    Unbounded,
}

const EPS: f64 = 1e-9;

/// Two-phase simplex (Bland's rule) on the continuous problem.
fn simplex(lp: &Lp) -> LpOutcome {
    let n = lp.c.len();
    // Columns: each variable as shift (x = lo + y), negation (x = hi - y) or a split pair.
    struct Col {
        var: usize,
        sign: f64,
    }
    let mut cols: Vec<Col> = Vec::new();
    let mut offset = vec![0.0; n];
    let mut extra: Vec<LpRow> = Vec::new();
    for j in 0..n {
        let (lo, hi) = (lp.lo[j], lp.hi[j]);
        if lo.is_finite() {
            offset[j] = lo;
            cols.push(Col { var: j, sign: 1.0 });
            if hi.is_finite() {
                let mut a = vec![0.0; n];
                a[j] = 1.0;
                extra.push(LpRow { a, rel: Rel3::Le, b: hi });
            }
        } else if hi.is_finite() {
            offset[j] = hi;
            cols.push(Col { var: j, sign: -1.0 });
        } else {
            cols.push(Col { var: j, sign: 1.0 });
            cols.push(Col { var: j, sign: -1.0 });
        }
    }
    let m_cols = cols.len();
    let rows: Vec<&LpRow> = lp.rows.iter().chain(extra.iter()).collect();
    let m = rows.len();
    // Each row: coefficients on the columns, the right side with the offsets taken out.
    let mut a: Vec<Vec<f64>> = Vec::with_capacity(m);
    let mut b: Vec<f64> = Vec::with_capacity(m);
    let mut rel: Vec<Rel3> = Vec::with_capacity(m);
    for r in &rows {
        let mut row: Vec<f64> = cols.iter().map(|c| r.a[c.var] * c.sign).collect();
        let mut rhs = r.b - (0..n).map(|j| r.a[j] * offset[j]).sum::<f64>();
        let mut kind = r.rel;
        if rhs < 0.0 {
            for v in &mut row {
                *v = -*v;
            }
            rhs = -rhs;
            kind = match kind {
                Rel3::Le => Rel3::Ge,
                Rel3::Ge => Rel3::Le,
                Rel3::Eq => Rel3::Eq,
            };
        }
        a.push(row);
        b.push(rhs);
        rel.push(kind);
    }
    // Slack (Le), surplus (Ge) and artificial (Ge, Eq) columns.
    let mut slack_of = vec![None; m];
    let mut artificial_of = vec![None; m];
    let mut total = m_cols;
    for i in 0..m {
        if rel[i] != Rel3::Eq {
            slack_of[i] = Some(total);
            total += 1;
        }
    }
    let first_artificial = total;
    for i in 0..m {
        if rel[i] != Rel3::Le {
            artificial_of[i] = Some(total);
            total += 1;
        }
    }
    let mut t: Vec<Vec<f64>> = vec![vec![0.0; total + 1]; m];
    let mut basis = vec![0usize; m];
    for i in 0..m {
        t[i][..m_cols].copy_from_slice(&a[i]);
        if let Some(s) = slack_of[i] {
            t[i][s] = if rel[i] == Rel3::Le { 1.0 } else { -1.0 };
        }
        if let Some(art) = artificial_of[i] {
            t[i][art] = 1.0;
            basis[i] = art;
        } else {
            basis[i] = slack_of[i].unwrap_or(0);
        }
        t[i][total] = b[i];
    }
    let limit = 20_000 + 50 * (m + total);
    // Phase 1: minimize the artificials.
    if first_artificial < total {
        let mut cost = vec![0.0; total];
        for c in &mut cost[first_artificial..] {
            *c = 1.0;
        }
        match run_simplex(&mut t, &mut basis, &cost, total, limit, total) {
            Some(value) if value > 1e-7 * (1.0 + b.iter().sum::<f64>()) => return LpOutcome::Infeasible,
            None => return LpOutcome::Infeasible,
            _ => {}
        }
        // Artificials left in the basis at zero are pivoted out where possible.
        for i in 0..m {
            if basis[i] >= first_artificial {
                if let Some(j) = (0..first_artificial).find(|&j| t[i][j].abs() > 1e-9) {
                    pivot(&mut t, &mut basis, i, j, total);
                }
            }
        }
    }
    // Phase 2.
    let mut cost = vec![0.0; total];
    for (k, col) in cols.iter().enumerate() {
        cost[k] = lp.c[col.var] * col.sign;
    }
    match run_simplex(&mut t, &mut basis, &cost, total, limit, first_artificial) {
        None => return LpOutcome::Unbounded,
        Some(_) => {}
    }
    let mut y = vec![0.0; total];
    for i in 0..m {
        y[basis[i]] = t[i][total];
    }
    let mut x = offset.clone();
    for (k, col) in cols.iter().enumerate() {
        x[col.var] += col.sign * y[k];
    }
    let value = (0..n).map(|j| lp.c[j] * x[j]).sum();
    LpOutcome::Optimal(x, value)
}

fn pivot(t: &mut [Vec<f64>], basis: &mut [usize], row: usize, col: usize, total: usize) {
    let p = t[row][col];
    for v in &mut t[row][..=total] {
        *v /= p;
    }
    let pivot_row = t[row].clone();
    for (i, r) in t.iter_mut().enumerate() {
        if i != row {
            let f = r[col];
            if f.abs() > 0.0 {
                for (x, pv) in r.iter_mut().zip(&pivot_row) {
                    *x -= f * pv;
                }
            }
        }
    }
    basis[row] = col;
}

/// Minimize `cost` over the columns before `allowed`; `None` if unbounded
/// (or out of steps). Gives the optimal value.
fn run_simplex(t: &mut [Vec<f64>], basis: &mut [usize], cost: &[f64], total: usize, limit: usize, allowed: usize) -> Option<f64> {
    let m = t.len();
    for _ in 0..limit {
        // Reduced costs.
        let mut enter = None;
        for j in 0..allowed {
            if basis.contains(&j) {
                continue;
            }
            let reduced = cost[j] - (0..m).map(|i| cost[basis[i]] * t[i][j]).sum::<f64>();
            if reduced < -EPS {
                enter = Some(j);
                break;
            }
        }
        let Some(j) = enter else {
            return Some((0..m).map(|i| cost[basis[i]] * t[i][total]).sum());
        };
        // Ratio test (ties: the lowest basis column, Bland).
        let mut leave: Option<(usize, f64)> = None;
        for i in 0..m {
            if t[i][j] > EPS {
                let ratio = t[i][total] / t[i][j];
                match leave {
                    Some((k, best)) if ratio > best + 1e-12 || (ratio > best - 1e-12 && basis[i] > basis[k]) => {}
                    _ => leave = Some((i, ratio)),
                }
            }
        }
        let (i, _) = leave?;
        pivot(t, basis, i, j, total);
    }
    None
}

/// Branch and bound over the whole-number variables.
fn solve_lp(lp: &Lp, _tolerance: f64, node_limit: usize) -> LpOutcome {
    let root = simplex(lp);
    if !lp.int.iter().any(|&i| i) {
        return root;
    }
    let LpOutcome::Optimal(..) = root else { return root };
    let mut best: Option<(Vec<f64>, f64)> = None;
    let mut stack = vec![lp.clone()];
    let mut nodes = 0;
    let mut unbounded = false;
    while let Some(node) = stack.pop() {
        nodes += 1;
        if nodes > node_limit {
            break;
        }
        match simplex(&node) {
            LpOutcome::Infeasible => continue,
            LpOutcome::Unbounded => {
                unbounded = true;
                continue;
            }
            LpOutcome::Optimal(x, value) => {
                if let Some((_, b)) = &best
                    && value >= *b - 1e-9
                {
                    continue;
                }
                // The first whole-number variable that is not whole.
                let fractional = (0..x.len()).find(|&j| node.int[j] && (x[j] - x[j].round()).abs() > 1e-6);
                match fractional {
                    None => {
                        let mut x = x;
                        for j in 0..x.len() {
                            if node.int[j] {
                                x[j] = x[j].round();
                            }
                        }
                        best = Some((x, value));
                    }
                    Some(j) => {
                        let mut down = node.clone();
                        down.hi[j] = down.hi[j].min(x[j].floor());
                        let mut up = node.clone();
                        up.lo[j] = up.lo[j].max(x[j].ceil());
                        if up.lo[j] <= up.hi[j] {
                            stack.push(up);
                        }
                        if down.lo[j] <= down.hi[j] {
                            stack.push(down);
                        }
                    }
                }
            }
        }
    }
    match best {
        Some((x, v)) => LpOutcome::Optimal(x, v),
        None if unbounded => LpOutcome::Unbounded,
        None => LpOutcome::Infeasible,
    }
}

// ------------------------------------------------------------- the search

/// A small generator (SplitMix64).
struct Rng(u64);

impl Rng {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The model as linear: the coefficients of the objective and of each
/// constraint's gap, if the book behaves linearly.
struct Linear {
    x0: Vec<f64>,
    base: Point,
    /// `slope[j]`: the change of the objective and of each gap per unit of variable `j`.
    slope: Vec<Point>,
}

fn probe_linear(h: &mut Harness, x0: &[f64]) -> Option<Linear> {
    let base = h.eval(x0)?;
    let mut slope = Vec::with_capacity(x0.len());
    for j in 0..x0.len() {
        let mut x = x0.to_vec();
        x[j] += 1.0;
        let p = h.eval(&x)?;
        slope.push(Point { objective: p.objective - base.objective, gaps: p.gaps.iter().zip(&base.gaps).map(|(a, b)| a - b).collect() });
    }
    // Another point: the prediction must hold.
    let mut rng = Rng(0xABCDEF);
    for round in 0..2 {
        let y: Vec<f64> = x0.iter().map(|v| v + (rng.unit() * 6.0 - 2.0) + f64::from(round)).collect();
        let p = h.eval(&y)?;
        let predict = |f: &dyn Fn(&Point) -> f64| f(&base) + y.iter().zip(x0).enumerate().map(|(j, (a, b))| f(&slope[j]) * (a - b)).sum::<f64>();
        let close = |a: f64, b: f64| (a - b).abs() <= 1e-7 * (1.0 + a.abs().max(b.abs()));
        if !close(predict(&|q| q.objective), p.objective) {
            return None;
        }
        for k in 0..base.gaps.len() {
            if !close(predict(&|q| q.gaps[k]), p.gaps[k]) {
                return None;
            }
        }
    }
    Some(Linear { x0: x0.to_vec(), base, slope })
}

fn solve_linear(h: &Harness, lin: &Linear) -> Solution {
    let n = lin.x0.len();
    let sign = match h.model.goal {
        Goal::Max => -1.0,
        _ => 1.0,
    };
    let mut c: Vec<f64> = (0..n).map(|j| sign * lin.slope[j].objective).collect();
    if matches!(h.model.goal, Goal::Value(_)) {
        c = vec![0.0; n];
    }
    let mut rows = Vec::new();
    for (k, s) in h.scalars.iter().enumerate() {
        let a: Vec<f64> = (0..n).map(|j| lin.slope[j].gaps[k]).collect();
        let b = -lin.base.gaps[k] + (0..n).map(|j| a[j] * lin.x0[j]).sum::<f64>();
        rows.push(LpRow { a, rel: match s.rel { Rel::Le => Rel3::Le, Rel::Ge => Rel3::Ge, _ => Rel3::Eq }, b });
    }
    if let Goal::Value(v) = h.model.goal {
        let a: Vec<f64> = (0..n).map(|j| lin.slope[j].objective).collect();
        let b = v - lin.base.objective + (0..n).map(|j| a[j] * lin.x0[j]).sum::<f64>();
        rows.push(LpRow { a, rel: Rel3::Eq, b });
    }
    let lp = Lp { c, rows, lo: h.bounds.iter().map(|b| b.0).collect(), hi: h.bounds.iter().map(|b| b.1).collect(), int: h.integer.clone() };
    match solve_lp(&lp, h.model.integer_tolerance, 20_000) {
        LpOutcome::Optimal(x, _) => {
            let objective = lin.base.objective + (0..n).map(|j| lin.slope[j].objective * (x[j] - lin.x0[j])).sum::<f64>();
            Solution { status: Status::Optimal, values: x, objective }
        }
        LpOutcome::Infeasible => Solution { status: Status::Infeasible, values: lin.x0.clone(), objective: lin.base.objective },
        LpOutcome::Unbounded => Solution { status: Status::Unbounded, values: lin.x0.clone(), objective: lin.base.objective },
    }
}

/// The penalized objective (to be minimized).
fn penalized(h: &mut Harness, x: &[f64], weight: f64) -> f64 {
    let Some(p) = h.eval(x) else { return f64::INFINITY };
    let base = match h.model.goal {
        Goal::Max => -p.objective,
        Goal::Min => p.objective,
        Goal::Value(v) => (p.objective - v).abs(),
    };
    base + weight * h.violation(&p, x)
}

/// Nelder-Mead from `start`; the best point and its value.
fn nelder_mead(h: &mut Harness, start: &[f64], weight: f64, step: f64, convergence: f64, budget: &mut i64, stop: &dyn Fn() -> bool) -> (Vec<f64>, f64) {
    let n = start.len();
    let mut simplex: Vec<(Vec<f64>, f64)> = Vec::with_capacity(n + 1);
    simplex.push((start.to_vec(), penalized(h, start, weight)));
    for j in 0..n {
        let mut x = start.to_vec();
        x[j] += if x[j].abs() > 1e-9 { x[j].abs() * step } else { step };
        let v = penalized(h, &x, weight);
        simplex.push((x, v));
    }
    while *budget > 0 && !stop() {
        *budget -= 1;
        simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (best, worst) = (simplex[0].1, simplex[n].1);
        let spread: f64 = simplex.iter().map(|(x, _)| x.iter().zip(&simplex[0].0).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max)).fold(0.0, f64::max);
        if (worst - best).abs() <= convergence * 1e-4 * (1.0 + best.abs()) && spread <= convergence * 1e-2 * (1.0 + simplex[0].0.iter().fold(0.0f64, |m, v| m.max(v.abs()))) {
            break;
        }
        let centroid: Vec<f64> = (0..n).map(|j| simplex[..n].iter().map(|(x, _)| x[j]).sum::<f64>() / n as f64).collect();
        let along = |t: f64| -> Vec<f64> { (0..n).map(|j| centroid[j] + t * (simplex[n].0[j] - centroid[j])).collect() };
        let reflected = along(-1.0);
        let fr = penalized(h, &reflected, weight);
        if fr < simplex[0].1 {
            let expanded = along(-2.0);
            let fe = penalized(h, &expanded, weight);
            simplex[n] = if fe < fr { (expanded, fe) } else { (reflected, fr) };
        } else if fr < simplex[n - 1].1 {
            simplex[n] = (reflected, fr);
        } else {
            let (inside, outside) = (along(0.5), along(-0.5));
            let (candidate, fc) = if fr < simplex[n].1 {
                let f = penalized(h, &outside, weight);
                (outside, f)
            } else {
                let f = penalized(h, &inside, weight);
                (inside, f)
            };
            if fc < simplex[n].1.min(fr) {
                simplex[n] = (candidate, fc);
            } else {
                let best_x = simplex[0].0.clone();
                for k in 1..=n {
                    let x: Vec<f64> = (0..n).map(|j| best_x[j] + 0.5 * (simplex[k].0[j] - best_x[j])).collect();
                    let v = penalized(h, &x, weight);
                    simplex[k] = (x, v);
                }
            }
        }
    }
    simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
    simplex.swap_remove(0)
}

fn clamp_to_bounds(h: &Harness, x: &mut [f64]) {
    for (i, v) in x.iter_mut().enumerate() {
        *v = v.clamp(h.bounds[i].0, h.bounds[i].1);
    }
}

fn solve_grg(h: &mut Harness, x0: &[f64], stop: &dyn Fn() -> bool) -> Solution {
    let mut x = x0.to_vec();
    clamp_to_bounds(h, &mut x);
    let mut budget: i64 = if h.model.max_iterations > 0 { i64::from(h.model.max_iterations) } else { 4000 };
    let started_budget = budget;
    let mut weight = 10.0;
    for round in 0..8 {
        let step = if round == 0 { 0.5 } else { 0.05 };
        let (best, _) = nelder_mead(h, &x, weight, step, h.model.convergence, &mut budget, stop);
        x = best;
        clamp_to_bounds(h, &mut x);
        if let Some(p) = h.eval(&x)
            && h.violation(&p, &x) <= h.tolerance(&p)
            && round >= 3
        {
            break;
        }
        weight *= 100.0;
        if budget <= 0 || stop() {
            break;
        }
    }
    // Whole numbers: rounded and the others settled.
    if h.integer.iter().any(|&i| i) {
        for (i, v) in x.iter_mut().enumerate() {
            if h.integer[i] {
                *v = v.round();
            }
        }
    }
    finish(h, x, budget <= 0 && started_budget > 0, stop())
}

fn finish(h: &mut Harness, x: Vec<f64>, out_of_steps: bool, stopped: bool) -> Solution {
    let Some(p) = h.eval(&x) else { return Solution { status: Status::BadModel, values: x, objective: f64::NAN } };
    let feasible = h.violation(&p, &x) <= h.tolerance(&p);
    let status = if !feasible {
        Status::Infeasible
    } else if stopped {
        Status::Stopped
    } else if out_of_steps {
        Status::IterationLimit
    } else {
        Status::Converged
    };
    Solution { status, values: x, objective: p.objective }
}

fn solve_evolutionary(h: &mut Harness, x0: &[f64], stop: &dyn Fn() -> bool) -> Solution {
    let n = x0.len();
    if h.bounds.iter().any(|b| !b.0.is_finite() || !b.1.is_finite()) {
        return Solution { status: Status::MissingBounds, values: x0.to_vec(), objective: f64::NAN };
    }
    let mut rng = Rng(h.model.seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
    let size = (10 * n).clamp(20, 100);
    let weight = 1e4;
    let make = |h: &Harness, rng: &mut Rng| -> Vec<f64> {
        (0..n)
            .map(|j| {
                let (lo, hi) = h.bounds[j];
                let v = lo + (hi - lo) * rng.unit();
                if h.integer[j] { v.round() } else { v }
            })
            .collect()
    };
    let mut pop: Vec<(Vec<f64>, f64)> = Vec::with_capacity(size);
    let mut start = x0.to_vec();
    clamp_to_bounds(h, &mut start);
    let f0 = penalized(h, &start, weight);
    pop.push((start, f0));
    while pop.len() < size {
        let x = make(h, &mut rng);
        let f = penalized(h, &x, weight);
        pop.push((x, f));
    }
    let generations = if h.model.max_iterations > 0 { h.model.max_iterations as usize } else { 400 };
    let mut out_of_steps = true;
    let mut last_best = f64::INFINITY;
    let mut still = 0;
    for _ in 0..generations {
        if stop() {
            let best = pop.iter().min_by(|a, b| a.1.total_cmp(&b.1)).map(|b| b.0.clone()).unwrap_or_default();
            return finish(h, best, false, true);
        }
        for i in 0..size {
            let pick = |rng: &mut Rng| (rng.unit() * size as f64) as usize % size;
            let (a, b, c) = (pick(&mut rng), pick(&mut rng), pick(&mut rng));
            let forced = (rng.unit() * n as f64) as usize % n;
            let trial: Vec<f64> = (0..n)
                .map(|j| {
                    let v = if rng.unit() < 0.7 || j == forced { pop[a].0[j] + 0.6 * (pop[b].0[j] - pop[c].0[j]) } else { pop[i].0[j] };
                    let v = v.clamp(h.bounds[j].0, h.bounds[j].1);
                    if h.integer[j] { v.round() } else { v }
                })
                .collect();
            let f = penalized(h, &trial, weight);
            if f <= pop[i].1 {
                pop[i] = (trial, f);
            }
        }
        let best = pop.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        if (last_best - best).abs() <= h.model.convergence * 1e-3 * (1.0 + best.abs()) {
            still += 1;
        } else {
            still = 0;
        }
        last_best = best;
        if still >= 30 {
            out_of_steps = false;
            break;
        }
    }
    let best = pop.iter().min_by(|a, b| a.1.total_cmp(&b.1)).map(|b| b.0.clone()).unwrap_or_default();
    finish(h, best, out_of_steps, false)
}

/// Solve `model` on a copy of the book. `stop` is asked now and then: true
/// ends the search (the time limit).
pub fn solve(presentation: &Presentation, slide: SlideId, model: &Model, stop: &dyn Fn() -> bool) -> Solution {
    let bad = |values: Vec<f64>| Solution { status: Status::BadModel, values, objective: f64::NAN };
    if model.variables.is_empty() {
        return bad(Vec::new());
    }
    let Some(mut h) = Harness::new(presentation, slide, model) else { return bad(Vec::new()) };
    let x0: Vec<f64> = model.variables.iter().map(|&c| read(&h.doc, slide, c)).collect();
    if x0.iter().any(|v| v.is_nan()) {
        return bad(x0);
    }
    if h.eval(&x0).is_none() {
        return bad(x0);
    }
    let linear = if model.method == Method::Evolutionary { None } else { probe_linear(&mut h, &x0) };
    match (model.method, linear) {
        (Method::Simplex, None) => Solution { status: Status::NotLinear, values: x0, objective: f64::NAN },
        (_, Some(lin)) => solve_linear(&h, &lin),
        (Method::Grg, None) => solve_grg(&mut h, &x0, stop),
        (Method::Evolutionary, None) => solve_evolutionary(&mut h, &x0, stop),
    }
}

/// The values of `cells` with the `variables` set to `x`.
pub fn values_with(presentation: &Presentation, slide: SlideId, variables: &[(u32, u32)], x: &[f64], cells: &[(u32, u32)]) -> Vec<f64> {
    let mut doc = Document::new(presentation.clone());
    let edits = variables.iter().zip(x).map(|(&(row, column), v)| CellEdit { row, column, input: CellInput::Value(CellValue::Number(*v)) }).collect();
    let _ = doc.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: slide, edits })]);
    cells.iter().map(|&c| read(&doc, slide, c)).collect()
}

/// The values of the cells in the book as it is.
pub fn values_of(presentation: &Presentation, slide: SlideId, cells: &[(u32, u32)]) -> Vec<f64> {
    let doc = Document::new(presentation.clone());
    cells.iter().map(|&c| read(&doc, slide, c)).collect()
}

/// Sensitivity of one variable cell at the solution.
#[derive(Clone, Debug, PartialEq)]
pub struct VariableSensitivity {
    pub cell: (u32, u32),
    pub final_value: f64,
    pub reduced_cost: f64,
    pub objective_coefficient: f64,
    pub allowable_increase: f64,
    pub allowable_decrease: f64,
}

/// Sensitivity of one scalar constraint at the solution.
#[derive(Clone, Debug, PartialEq)]
pub struct ConstraintSensitivity {
    pub cell: (u32, u32),
    pub final_value: f64,
    pub shadow_price: f64,
    pub constraint_rhs: f64,
    pub allowable_increase: f64,
    pub allowable_decrease: f64,
}

/// Sensitivity report values for variables and constraints.
#[derive(Clone, Debug, PartialEq)]
pub struct Sensitivity {
    pub variables: Vec<VariableSensitivity>,
    pub constraints: Vec<ConstraintSensitivity>,
}

/// Lower and upper limits of one variable cell (holding the other variables fixed).
#[derive(Clone, Debug, PartialEq)]
pub struct VariableLimit {
    pub cell: (u32, u32),
    pub value: f64,
    pub lower_limit: Option<f64>,
    pub lower_objective: Option<f64>,
    pub upper_limit: Option<f64>,
    pub upper_objective: Option<f64>,
}

const INF_ALLOWABLE: f64 = 1e30;

fn clean_zero(v: f64) -> f64 {
    if v.abs() < 1e-12 { 0.0 } else { v }
}

/// Compute the Sensitivity Report (variable reduced costs / allowable ranges and
/// constraint shadow prices / allowable ranges) at `solution`.
pub fn sensitivity(presentation: &Presentation, slide: SlideId, model: &Model, solution: &Solution) -> Sensitivity {
    let Some(mut h) = Harness::new(presentation, slide, model) else {
        return Sensitivity { variables: Vec::new(), constraints: Vec::new() };
    };
    let x = &solution.values;
    let Some(base) = h.eval(x) else {
        return Sensitivity { variables: Vec::new(), constraints: Vec::new() };
    };
    let n = model.variables.len();
    let m = h.scalars.len();
    let constraint_cells_and_rhs: Vec<((u32, u32), f64, f64)> = h
        .scalars
        .iter()
        .enumerate()
        .map(|(k, s)| {
            let rhs = s.rhs_cell.map_or(s.rhs_number, |c| read(&h.doc, slide, c));
            let lhs = base.gaps[k] + rhs;
            (s.lhs, lhs, rhs)
        })
        .collect();

    if let Some(lin) = probe_linear(&mut h, x)
        && let Some(sens) = linear_sensitivity(&h, &lin, x, &constraint_cells_and_rhs)
    {
        return sens;
    }

    // Nonlinear fallback: finite-difference gradients and slack-based ranges.
    let eps = 1e-6;
    let mut variables = Vec::with_capacity(n);
    for (j, &cell) in model.variables.iter().enumerate() {
        let mut x1 = x.clone();
        x1[j] += eps;
        let grad = h.eval(&x1).map_or(0.0, |p| clean_zero((p.objective - base.objective) / eps));
        let (lo, hi) = h.bounds[j];
        let at_bound = (x[j] - lo).abs() <= model.precision || (hi - x[j]).abs() <= model.precision;
        variables.push(VariableSensitivity {
            cell,
            final_value: x[j],
            reduced_cost: if at_bound { grad } else { 0.0 },
            objective_coefficient: grad,
            allowable_increase: INF_ALLOWABLE,
            allowable_decrease: INF_ALLOWABLE,
        });
    }
    let mut constraints = Vec::with_capacity(m);
    for (k, s) in h.scalars.iter().enumerate() {
        let (cell, final_value, constraint_rhs) = constraint_cells_and_rhs[k];
        let gap = base.gaps[k];
        let slack = match s.rel {
            Rel::Le => (-gap).max(0.0),
            Rel::Ge => gap.max(0.0),
            _ => gap.abs(),
        };
        let binding = slack <= model.precision * (1.0 + constraint_rhs.abs());
        let (allowable_increase, allowable_decrease) = if binding {
            (INF_ALLOWABLE, INF_ALLOWABLE)
        } else {
            match s.rel {
                Rel::Le => (INF_ALLOWABLE, slack),
                Rel::Ge => (slack, INF_ALLOWABLE),
                _ => (0.0, 0.0),
            }
        };
        constraints.push(ConstraintSensitivity {
            cell,
            final_value,
            shadow_price: 0.0,
            constraint_rhs,
            allowable_increase,
            allowable_decrease,
        });
    }
    Sensitivity { variables, constraints }
}

fn linear_sensitivity(
    h: &Harness,
    lin: &Linear,
    x_sol: &[f64],
    constraint_cells_and_rhs: &[((u32, u32), f64, f64)],
) -> Option<Sensitivity> {
    let n = lin.x0.len();
    let goal_sign = match h.model.goal {
        Goal::Max => -1.0,
        _ => 1.0,
    };
    let mut c: Vec<f64> = (0..n).map(|j| goal_sign * lin.slope[j].objective).collect();
    if matches!(h.model.goal, Goal::Value(_)) {
        c = vec![0.0; n];
    }
    let mut rows = Vec::with_capacity(h.scalars.len());
    for (k, s) in h.scalars.iter().enumerate() {
        let a: Vec<f64> = (0..n).map(|j| lin.slope[j].gaps[k]).collect();
        let b = -lin.base.gaps[k] + (0..n).map(|j| a[j] * lin.x0[j]).sum::<f64>();
        rows.push(LpRow {
            a,
            rel: match s.rel {
                Rel::Le => Rel3::Le,
                Rel::Ge => Rel3::Ge,
                _ => Rel3::Eq,
            },
            b,
        });
    }
    let m_user = rows.len();
    if let Goal::Value(v) = h.model.goal {
        let a: Vec<f64> = (0..n).map(|j| lin.slope[j].objective).collect();
        let b = v - lin.base.objective + (0..n).map(|j| a[j] * lin.x0[j]).sum::<f64>();
        rows.push(LpRow { a, rel: Rel3::Eq, b });
    }
    // Keep only non-negativity in `lo` so explicit bound constraints stay solely in `rows`.
    let lo = vec![if h.model.non_negative { 0.0 } else { f64::NEG_INFINITY }; n];
    struct Col {
        var: usize,
        sign: f64,
    }
    let mut cols: Vec<Col> = Vec::new();
    let mut offset = vec![0.0; n];
    for j in 0..n {
        if lo[j].is_finite() {
            offset[j] = lo[j];
            cols.push(Col { var: j, sign: 1.0 });
        } else {
            cols.push(Col { var: j, sign: 1.0 });
            cols.push(Col { var: j, sign: -1.0 });
        }
    }
    let m_cols = cols.len();
    let m = rows.len();
    let mut a: Vec<Vec<f64>> = Vec::with_capacity(m);
    let mut b: Vec<f64> = Vec::with_capacity(m);
    let mut rel: Vec<Rel3> = Vec::with_capacity(m);
    let mut row_sign = vec![1.0; m];
    for (i, r) in rows.iter().enumerate() {
        let mut row: Vec<f64> = cols.iter().map(|col| r.a[col.var] * col.sign).collect();
        let mut rhs = r.b - (0..n).map(|j| r.a[j] * offset[j]).sum::<f64>();
        let mut kind = r.rel;
        if rhs < 0.0 {
            for v in &mut row {
                *v = -*v;
            }
            rhs = -rhs;
            row_sign[i] = -1.0;
            kind = match kind {
                Rel3::Le => Rel3::Ge,
                Rel3::Ge => Rel3::Le,
                Rel3::Eq => Rel3::Eq,
            };
        }
        a.push(row);
        b.push(rhs);
        rel.push(kind);
    }
    let mut slack_of = vec![None; m];
    let mut artificial_of = vec![None; m];
    let mut total = m_cols;
    for i in 0..m {
        if rel[i] != Rel3::Eq {
            slack_of[i] = Some(total);
            total += 1;
        }
    }
    let first_artificial = total;
    for i in 0..m {
        if rel[i] != Rel3::Le {
            artificial_of[i] = Some(total);
            total += 1;
        }
    }
    let mut t: Vec<Vec<f64>> = vec![vec![0.0; total + 1]; m];
    let mut basis = vec![0usize; m];
    let mut unit_col = vec![0usize; m];
    for i in 0..m {
        t[i][..m_cols].copy_from_slice(&a[i]);
        if let Some(s) = slack_of[i] {
            t[i][s] = if rel[i] == Rel3::Le { 1.0 } else { -1.0 };
        }
        if let Some(art) = artificial_of[i] {
            t[i][art] = 1.0;
            basis[i] = art;
            unit_col[i] = art;
        } else {
            let s = slack_of[i].unwrap_or(0);
            basis[i] = s;
            unit_col[i] = s;
        }
        t[i][total] = b[i];
    }
    let limit = 20_000 + 50 * (m + total);
    if first_artificial < total {
        let mut cost1 = vec![0.0; total];
        for c1 in &mut cost1[first_artificial..] {
            *c1 = 1.0;
        }
        match run_simplex(&mut t, &mut basis, &cost1, total, limit, total) {
            Some(v) if v > 1e-7 * (1.0 + b.iter().sum::<f64>()) => return None,
            None => return None,
            _ => {}
        }
        for i in 0..m {
            if basis[i] >= first_artificial
                && let Some(j) = (0..first_artificial).find(|&j| t[i][j].abs() > 1e-9)
            {
                pivot(&mut t, &mut basis, i, j, total);
            }
        }
    }
    let mut cost = vec![0.0; total];
    for (k, col) in cols.iter().enumerate() {
        cost[k] = c[col.var] * col.sign;
    }
    run_simplex(&mut t, &mut basis, &cost, total, limit, first_artificial)?;

    let reduced: Vec<f64> = (0..first_artificial)
        .map(|q| (cost[q] - (0..m).map(|r| cost[basis[r]] * t[r][q]).sum::<f64>()).max(0.0))
        .collect();

    let mut variables = Vec::with_capacity(n);
    for (j, &cell) in h.model.variables.iter().enumerate() {
        let col_pos = cols.iter().position(|col| col.var == j && col.sign > 0.0).unwrap_or(j);
        let col_neg = cols.iter().position(|col| col.var == j && col.sign < 0.0);
        let basic_row = basis
            .iter()
            .position(|&b| b == col_pos)
            .map(|r| (r, 1.0))
            .or_else(|| col_neg.and_then(|cn| basis.iter().position(|&b| b == cn).map(|r| (r, -1.0))));
        let (reduced_cost, allowable_increase, allowable_decrease) = match basic_row {
            Some((r0, col_sign)) => {
                let mut inc = INF_ALLOWABLE;
                let mut dec = INF_ALLOWABLE;
                for (q, &dq) in reduced.iter().enumerate() {
                    if basis.contains(&q) || cols.get(q).is_some_and(|col| col.var == j) {
                        continue;
                    }
                    let beta = goal_sign * col_sign * t[r0][q];
                    if beta > 1e-9 {
                        inc = inc.min(dq / beta);
                    } else if beta < -1e-9 {
                        dec = dec.min(dq / (-beta));
                    }
                }
                (0.0, clean_zero(inc), clean_zero(dec))
            }
            None => {
                let dj = cost[col_pos] - (0..m).map(|r| cost[basis[r]] * t[r][col_pos]).sum::<f64>();
                let rc = clean_zero(goal_sign * dj);
                if goal_sign < 0.0 {
                    (rc, clean_zero(dj.max(0.0)), INF_ALLOWABLE)
                } else {
                    (rc, INF_ALLOWABLE, clean_zero(dj.max(0.0)))
                }
            }
        };
        variables.push(VariableSensitivity {
            cell,
            final_value: x_sol[j],
            reduced_cost,
            objective_coefficient: clean_zero(lin.slope[j].objective),
            allowable_increase,
            allowable_decrease,
        });
    }

    let mut constraints = Vec::with_capacity(m_user);
    for (i, &(cell, final_value, constraint_rhs)) in constraint_cells_and_rhs.iter().enumerate().take(m_user) {
        let u = unit_col[i];
        let sp = clean_zero(goal_sign * row_sign[i] * (0..m).map(|r| cost[basis[r]] * t[r][u]).sum::<f64>());
        let mut inc = INF_ALLOWABLE;
        let mut dec = INF_ALLOWABLE;
        for r in 0..m {
            let alpha = t[r][u] * row_sign[i];
            let rhs_r = t[r][total].max(0.0);
            if alpha < -1e-9 {
                inc = inc.min(rhs_r / (-alpha));
            } else if alpha > 1e-9 {
                dec = dec.min(rhs_r / alpha);
            }
        }
        constraints.push(ConstraintSensitivity {
            cell,
            final_value,
            shadow_price: sp,
            constraint_rhs,
            allowable_increase: clean_zero(inc),
            allowable_decrease: clean_zero(dec),
        });
    }
    Some(Sensitivity { variables, constraints })
}

/// Compute the Limits Report (lower and upper limits for each variable holding the
/// other variables at `solution.values`, together with the objective value at each limit).
pub fn limits(presentation: &Presentation, slide: SlideId, model: &Model, solution: &Solution) -> Vec<VariableLimit> {
    let Some(mut h) = Harness::new(presentation, slide, model) else {
        return Vec::new();
    };
    let x = &solution.values;
    let Some(base) = h.eval(x) else {
        return Vec::new();
    };
    let lin = probe_linear(&mut h, x);
    let mut out = Vec::with_capacity(model.variables.len());
    for (j, &cell) in model.variables.iter().enumerate() {
        let xj = x[j];
        let (mut lo, mut hi) = h.bounds[j];
        if let Some(lin) = &lin {
            for (k, s) in h.scalars.iter().enumerate() {
                let a = lin.slope[j].gaps[k];
                let g0 = base.gaps[k];
                if a.abs() <= 1e-9 {
                    continue;
                }
                let boundary = xj - g0 / a;
                match s.rel {
                    Rel::Le => {
                        if a > 0.0 {
                            hi = hi.min(boundary);
                        } else {
                            lo = lo.max(boundary);
                        }
                    }
                    Rel::Ge => {
                        if a > 0.0 {
                            lo = lo.max(boundary);
                        } else {
                            hi = hi.min(boundary);
                        }
                    }
                    Rel::Eq => {
                        lo = xj;
                        hi = xj;
                    }
                    _ => {}
                }
            }
        } else {
            // Coordinate search along variable j for nonlinear models.
            let feasible_at = |h: &mut Harness, val: f64| -> bool {
                let mut trial = x.clone();
                trial[j] = val;
                h.eval(&trial).is_some_and(|p| h.violation(&p, &trial) <= h.tolerance(&p))
            };
            let search_dir = |h: &mut Harness, dir: f64, bound: f64| -> f64 {
                let mut step = 1.0;
                let mut ok = xj;
                let mut far = bound;
                for _ in 0..30 {
                    let cand = (xj + dir * step).clamp(lo, hi);
                    if feasible_at(h, cand) {
                        ok = cand;
                        if (cand - bound).abs() <= 1e-9 {
                            return bound;
                        }
                        step *= 2.0;
                    } else {
                        far = cand;
                        break;
                    }
                }
                if !far.is_finite() {
                    return far;
                }
                let (mut a, mut b) = (ok, far);
                for _ in 0..40 {
                    let mid = 0.5 * (a + b);
                    if feasible_at(h, mid) {
                        a = mid;
                    } else {
                        b = mid;
                    }
                }
                a
            };
            let new_lo = search_dir(&mut h, -1.0, lo);
            let new_hi = search_dir(&mut h, 1.0, hi);
            (lo, hi) = (new_lo, new_hi);
        }
        if h.integer[j] {
            if lo.is_finite() {
                lo = lo.ceil();
            }
            if hi.is_finite() {
                hi = hi.floor();
            }
        }
        lo = lo.min(xj);
        hi = hi.max(xj);
        let obj_at = |h: &mut Harness, val: f64| -> Option<f64> {
            if !val.is_finite() {
                return None;
            }
            let mut trial = x.clone();
            trial[j] = val;
            h.eval(&trial).map(|p| clean_zero(p.objective))
        };
        let lower_limit = lo.is_finite().then(|| clean_zero(lo));
        let lower_objective = lower_limit.and_then(|v| obj_at(&mut h, v));
        let upper_limit = hi.is_finite().then(|| clean_zero(hi));
        let upper_objective = upper_limit.and_then(|v| obj_at(&mut h, v));
        out.push(VariableLimit {
            cell,
            value: xj,
            lower_limit,
            lower_objective,
            upper_limit,
            upper_objective,
        });
    }
    out
}

// ------------------------------------------------- the model as written down

/// The model as the user typed it and as Excel keeps it in the sheet's
/// hidden names (`solver_opt`, `solver_adj`, `solver_lhs1`, …): the cells
/// as text (`$B$5`, `$A$1:$A$3`, `A1,A3`).
#[derive(Clone, Debug, PartialEq)]
pub struct Description {
    pub objective: String,
    pub goal: Goal,
    pub variables: String,
    /// Left side, relation, right side (a number or a cell; empty for int, bin, dif).
    pub constraints: Vec<(String, Rel, String)>,
    pub non_negative: bool,
    pub method: Method,
    pub precision: f64,
    pub convergence: f64,
    /// In percent (Excel's "Integer Optimality").
    pub integer_tolerance: f64,
    /// 0: no limit.
    pub max_iterations: u32,
    /// In seconds; 0: no limit.
    pub max_time: u32,
}

impl Default for Description {
    fn default() -> Self {
        Self { objective: String::new(), goal: Goal::Max, variables: String::new(), constraints: Vec::new(), non_negative: true, method: Method::Grg, precision: 1e-6, convergence: 1e-4, integer_tolerance: 1.0, max_iterations: 0, max_time: 0 }
    }
}

/// `$B$5`, `Sheet1!$A$1:$A$3`, `A1,A3:A4`: the cells in order.
pub fn parse_cells(text: &str) -> Option<Vec<(u32, u32)>> {
    let mut cells = Vec::new();
    for area in text.trim().trim_matches(|c| c == '(' || c == ')').split(',') {
        let area = area.trim();
        let area = area.rsplit_once('!').map_or(area, |(_, a)| a);
        let mut ends = area.split(':');
        let a = ibis_calc::formula::parser::parse_cell(ends.next()?.trim())?;
        let b = match ends.next() {
            Some(e) => ibis_calc::formula::parser::parse_cell(e.trim())?,
            None => a.clone(),
        };
        if ends.next().is_some() || cells.len() > 100_000 {
            return None;
        }
        for row in a.row.min(b.row)..=a.row.max(b.row) {
            for column in a.column.min(b.column)..=a.column.max(b.column) {
                cells.push((row, column));
            }
        }
    }
    (!cells.is_empty()).then_some(cells)
}

fn absolute(sheet: &str, text: &str) -> String {
    // Each area as `Sheet!$A$1:$B$2`.
    let quoted = ibis_calc::formula::quote_sheet(sheet);
    text.split(',')
        .map(|area| {
            let area = area.trim();
            let area = area.rsplit_once('!').map_or(area, |(_, a)| a);
            let dollar = |cell: &str| {
                let cell = cell.replace('$', "");
                let split = cell.find(|c: char| c.is_ascii_digit()).unwrap_or(cell.len());
                format!("${}${}", &cell[..split], &cell[split..])
            };
            format!("{quoted}!{}", area.split(':').map(dollar).collect::<Vec<_>>().join(":"))
        })
        .collect::<Vec<_>>()
        .join(",")
}

impl Description {
    /// The cells and settings, for [`solve`]; `Err` names what is wrong.
    pub fn to_model(&self) -> Result<Model, &'static str> {
        let objective = parse_cells(&self.objective).filter(|c| c.len() == 1).ok_or("objective")?[0];
        let variables = parse_cells(&self.variables).ok_or("variables")?;
        let mut constraints = Vec::new();
        for (lhs, rel, rhs) in &self.constraints {
            let lhs = parse_cells(lhs).ok_or("constraint")?;
            let rhs = match rel {
                Rel::Int | Rel::Bin | Rel::Dif => Rhs::None,
                _ => match rhs.trim().parse::<f64>() {
                    Ok(n) => Rhs::Number(n),
                    Err(_) => {
                        let cells = parse_cells(rhs).ok_or("constraint")?;
                        if cells.len() != 1 && cells.len() != lhs.len() {
                            return Err("constraint");
                        }
                        Rhs::Cells(cells)
                    }
                },
            };
            constraints.push(Constraint { lhs, rel: *rel, rhs });
        }
        Ok(Model {
            objective,
            goal: self.goal,
            variables,
            constraints,
            non_negative: self.non_negative,
            method: self.method,
            precision: self.precision,
            convergence: self.convergence,
            integer_tolerance: self.integer_tolerance / 100.0,
            max_iterations: self.max_iterations,
            seed: 1,
        })
    }

    /// The hidden names Excel keeps the model in, for the sheet called `sheet`.
    pub fn to_names(&self, sheet: &str) -> Vec<(String, String)> {
        let n = |v: f64| ibis_calc::number_to_xml(v);
        let mut names = vec![
            ("solver_opt".to_string(), absolute(sheet, &self.objective)),
            ("solver_adj".to_string(), absolute(sheet, &self.variables)),
            ("solver_typ".to_string(), match self.goal { Goal::Max => "1", Goal::Min => "2", Goal::Value(_) => "3" }.to_string()),
            ("solver_val".to_string(), n(if let Goal::Value(v) = self.goal { v } else { 0.0 })),
            ("solver_num".to_string(), self.constraints.len().to_string()),
            ("solver_eng".to_string(), match self.method { Method::Grg => "1", Method::Simplex => "2", Method::Evolutionary => "3" }.to_string()),
            ("solver_neg".to_string(), if self.non_negative { "1" } else { "2" }.to_string()),
            ("solver_lin".to_string(), if self.method == Method::Simplex { "1" } else { "2" }.to_string()),
            ("solver_pre".to_string(), n(self.precision)),
            ("solver_cvg".to_string(), n(self.convergence)),
            ("solver_mip".to_string(), n(self.integer_tolerance)),
            ("solver_itr".to_string(), if self.max_iterations == 0 { "2147483647".to_string() } else { self.max_iterations.to_string() }),
            ("solver_tim".to_string(), if self.max_time == 0 { "2147483647".to_string() } else { self.max_time.to_string() }),
            ("solver_ver".to_string(), "3".to_string()),
        ];
        for (i, (lhs, rel, rhs)) in self.constraints.iter().enumerate() {
            let k = i + 1;
            names.push((format!("solver_lhs{k}"), absolute(sheet, lhs)));
            names.push((format!("solver_rel{k}"), match rel { Rel::Le => "1", Rel::Eq => "2", Rel::Ge => "3", Rel::Int => "4", Rel::Bin => "5", Rel::Dif => "6" }.to_string()));
            names.push((
                format!("solver_rhs{k}"),
                match rel {
                    Rel::Int => "integer".to_string(),
                    Rel::Bin => "binary".to_string(),
                    Rel::Dif => "alldifferent".to_string(),
                    _ if rhs.trim().parse::<f64>().is_ok() => rhs.trim().to_string(),
                    _ => absolute(sheet, rhs),
                },
            ));
        }
        names
    }

    /// The model kept in the names (`(name, formula)` of the sheet), if there is one.
    pub fn from_names(names: &[(String, String)]) -> Option<Self> {
        let get = |k: &str| names.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, f)| f.trim().to_string());
        let number = |k: &str| get(k).and_then(|v| v.trim_matches('"').parse::<f64>().ok());
        let objective = get("solver_opt")?;
        let variables = get("solver_adj")?;
        let strip = |text: String| -> String { text.rsplit_once('!').map_or(text.clone(), |(_, a)| a.to_string()).replace('$', "") };
        let list = |text: String| -> String { text.split(',').map(|a| strip(a.trim().trim_matches(|c| c == '(' || c == ')').to_string())).collect::<Vec<_>>().join(",") };
        let goal = match number("solver_typ").unwrap_or(1.0) as i32 {
            2 => Goal::Min,
            3 => Goal::Value(number("solver_val").unwrap_or(0.0)),
            _ => Goal::Max,
        };
        let count = number("solver_num").unwrap_or(0.0) as usize;
        let mut constraints = Vec::new();
        for k in 1..=count {
            let lhs = get(&format!("solver_lhs{k}"))?;
            let rel = match number(&format!("solver_rel{k}")).unwrap_or(1.0) as i32 {
                2 => Rel::Eq,
                3 => Rel::Ge,
                4 => Rel::Int,
                5 => Rel::Bin,
                6 => Rel::Dif,
                _ => Rel::Le,
            };
            let rhs = get(&format!("solver_rhs{k}")).unwrap_or_default();
            let rhs = if matches!(rel, Rel::Int | Rel::Bin | Rel::Dif) { String::new() } else if rhs.trim_matches('"').parse::<f64>().is_ok() { rhs.trim_matches('"').to_string() } else { strip(rhs) };
            constraints.push((list(lhs), rel, rhs));
        }
        let limit = |k: &str| number(k).filter(|v| *v < 1e9).map_or(0, |v| v as u32);
        Some(Self {
            objective: list(objective),
            goal,
            variables: list(variables),
            constraints,
            non_negative: number("solver_neg").unwrap_or(1.0) as i32 != 2,
            method: match number("solver_eng").unwrap_or(1.0) as i32 {
                2 => Method::Simplex,
                3 => Method::Evolutionary,
                _ => Method::Grg,
            },
            precision: number("solver_pre").unwrap_or(1e-6),
            convergence: number("solver_cvg").unwrap_or(1e-4),
            integer_tolerance: number("solver_mip").unwrap_or(1.0),
            max_iterations: limit("solver_itr"),
            max_time: limit("solver_tim"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{EmuRect, EmuSize};
    use crate::model::{Shape, ShapeKind, Slide};
    use crate::sheet::{Sheet, Workbook};
    use std::sync::Arc;

    fn book(cells: &[(u32, u32, CellInput)]) -> (Presentation, SlideId) {
        let mut p = Presentation::empty(EmuSize::WIDESCREEN);
        let id = p.new_slide_id();
        let mut slide = Slide::new(id);
        slide.name = Some("Sheet1".into());
        let shape = p.new_shape_id();
        let sheet = Sheet { columns: vec![100; 10], rows: vec![100; 20], grid_lines: true, ..Sheet::default() };
        slide.shapes.push(Shape { id: shape, frame: EmuRect::new(0, 0, 1000, 2000), rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Sheet(Arc::new(sheet)), floating: None });
        p.slides.push(slide);
        p.workbook = Some(Arc::new(Workbook {
            sheets: vec![crate::sheet::WorkbookSheet { name: "Sheet1".into(), state: Default::default(), slide: Some(id), hidden: None, other: false }],
            ..Default::default()
        }));
        let mut d = Document::new(p);
        let edits = cells.iter().map(|(row, column, input)| CellEdit { row: *row, column: *column, input: input.clone() }).collect();
        d.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: id, edits })]).unwrap();
        (d.presentation().clone(), id)
    }

    fn n(v: f64) -> CellInput {
        CellInput::Value(CellValue::Number(v))
    }

    fn f(s: &str) -> CellInput {
        CellInput::Formula(s.into())
    }

    fn never() -> bool {
        false
    }

    /// Maximize 3x + 2y (B1) with x (A1), y (A2) under x + y <= 4 (B2),
    /// x + 3y <= 6 (B3), x <= 3.
    fn production() -> (Presentation, SlideId, Model) {
        let (p, s) = book(&[(0, 0, n(0.0)), (1, 0, n(0.0)), (0, 1, f("3*A1+2*A2")), (1, 1, f("A1+A2")), (2, 1, f("A1+3*A2"))]);
        let model = Model {
            objective: (0, 1),
            goal: Goal::Max,
            variables: vec![(0, 0), (1, 0)],
            constraints: vec![
                Constraint { lhs: vec![(1, 1)], rel: Rel::Le, rhs: Rhs::Number(4.0) },
                Constraint { lhs: vec![(2, 1)], rel: Rel::Le, rhs: Rhs::Number(6.0) },
                Constraint { lhs: vec![(0, 0)], rel: Rel::Le, rhs: Rhs::Number(3.0) },
            ],
            method: Method::Simplex,
            ..Model::default()
        };
        (p, s, model)
    }

    #[test]
    fn a_linear_program_is_solved_exactly() {
        let (p, s, model) = production();
        let sol = solve(&p, s, &model, &never);
        assert_eq!(sol.status, Status::Optimal);
        assert!((sol.values[0] - 3.0).abs() < 1e-9 && (sol.values[1] - 1.0).abs() < 1e-9, "{:?}", sol.values);
        assert!((sol.objective - 11.0).abs() < 1e-9);
        // The default method finds the same for a linear book.
        let grg = solve(&p, s, &Model { method: Method::Grg, ..model.clone() }, &never);
        assert_eq!(grg.status, Status::Optimal);
        assert!((grg.objective - 11.0).abs() < 1e-9);
    }

    #[test]
    fn minimizing_and_value_of_goals() {
        let (p, s) = book(&[(0, 0, n(5.0)), (1, 0, n(5.0)), (0, 1, f("A1+A2")), (1, 1, f("A1-A2"))]);
        let mut model = Model { objective: (0, 1), goal: Goal::Min, variables: vec![(0, 0), (1, 0)], constraints: vec![Constraint { lhs: vec![(1, 1)], rel: Rel::Ge, rhs: Rhs::Number(2.0) }], method: Method::Simplex, ..Model::default() };
        let sol = solve(&p, s, &model, &never);
        assert_eq!(sol.status, Status::Optimal);
        assert!((sol.objective - 2.0).abs() < 1e-9, "{sol:?}");
        model.goal = Goal::Value(10.0);
        let sol = solve(&p, s, &model, &never);
        assert_eq!(sol.status, Status::Optimal);
        assert!((sol.objective - 10.0).abs() < 1e-9);
        assert!(sol.values[0] - sol.values[1] >= 2.0 - 1e-9);
    }

    #[test]
    fn whole_numbers_are_branched_on() {
        // Maximize 5x + 4y with 6x + 4y <= 24 and x + 2y <= 6: the relaxed optimum (3, 1.5) is not whole.
        let (p, s) = book(&[(0, 0, n(0.0)), (1, 0, n(0.0)), (0, 1, f("5*A1+4*A2")), (1, 1, f("6*A1+4*A2")), (2, 1, f("A1+2*A2"))]);
        let model = Model {
            objective: (0, 1),
            goal: Goal::Max,
            variables: vec![(0, 0), (1, 0)],
            constraints: vec![
                Constraint { lhs: vec![(1, 1)], rel: Rel::Le, rhs: Rhs::Number(24.0) },
                Constraint { lhs: vec![(2, 1)], rel: Rel::Le, rhs: Rhs::Number(6.0) },
                Constraint { lhs: vec![(0, 0), (1, 0)], rel: Rel::Int, rhs: Rhs::None },
            ],
            method: Method::Simplex,
            ..Model::default()
        };
        let sol = solve(&p, s, &model, &never);
        assert_eq!(sol.status, Status::Optimal);
        assert_eq!(sol.values, vec![4.0, 0.0]);
        assert_eq!(sol.objective, 20.0);
    }

    #[test]
    fn a_binary_choice_and_cell_right_sides() {
        // Pick items (A1:A3 binary) of weight 2, 3, 4 and value 3, 4, 5 within a capacity in C1.
        let (p, s) = book(&[(0, 0, n(0.0)), (1, 0, n(0.0)), (2, 0, n(0.0)), (0, 1, f("3*A1+4*A2+5*A3")), (1, 1, f("2*A1+3*A2+4*A3")), (0, 2, n(5.0))]);
        let model = Model {
            objective: (0, 1),
            goal: Goal::Max,
            variables: vec![(0, 0), (1, 0), (2, 0)],
            constraints: vec![Constraint { lhs: vec![(1, 1)], rel: Rel::Le, rhs: Rhs::Cells(vec![(0, 2)]) }, Constraint { lhs: vec![(0, 0), (1, 0), (2, 0)], rel: Rel::Bin, rhs: Rhs::None }],
            method: Method::Simplex,
            ..Model::default()
        };
        let sol = solve(&p, s, &model, &never);
        assert_eq!(sol.status, Status::Optimal);
        assert_eq!(sol.values, vec![1.0, 1.0, 0.0]);
        assert_eq!(sol.objective, 7.0);
    }

    #[test]
    fn infeasible_and_unbounded_models_are_told() {
        let (p, s, mut model) = production();
        model.constraints.push(Constraint { lhs: vec![(0, 0)], rel: Rel::Ge, rhs: Rhs::Number(5.0) });
        assert_eq!(solve(&p, s, &model, &never).status, Status::Infeasible);
        let (p, s) = book(&[(0, 0, n(1.0)), (0, 1, f("A1"))]);
        let model = Model { objective: (0, 1), goal: Goal::Max, variables: vec![(0, 0)], method: Method::Simplex, ..Model::default() };
        assert_eq!(solve(&p, s, &model, &never).status, Status::Unbounded);
    }

    #[test]
    fn the_simplex_method_refuses_a_curved_model() {
        let (p, s) = book(&[(0, 0, n(1.0)), (0, 1, f("A1*A1"))]);
        let model = Model { objective: (0, 1), goal: Goal::Min, variables: vec![(0, 0)], method: Method::Simplex, non_negative: false, ..Model::default() };
        assert_eq!(solve(&p, s, &model, &never).status, Status::NotLinear);
    }

    #[test]
    fn a_nonlinear_model_is_searched() {
        // Minimize (x - 1)^2 + (y - 2)^2 with x + y <= 2: at (0.5, 1.5).
        let (p, s) = book(&[(0, 0, n(0.0)), (1, 0, n(0.0)), (0, 1, f("(A1-1)^2+(A2-2)^2")), (1, 1, f("A1+A2"))]);
        let model = Model {
            objective: (0, 1),
            goal: Goal::Min,
            variables: vec![(0, 0), (1, 0)],
            constraints: vec![Constraint { lhs: vec![(1, 1)], rel: Rel::Le, rhs: Rhs::Number(2.0) }],
            method: Method::Grg,
            non_negative: false,
            ..Model::default()
        };
        let sol = solve(&p, s, &model, &never);
        assert!(matches!(sol.status, Status::Converged | Status::Optimal), "{sol:?}");
        assert!((sol.values[0] - 0.5).abs() < 1e-3 && (sol.values[1] - 1.5).abs() < 1e-3, "{:?}", sol.values);
        // Unconstrained maximum of a hill.
        let (p, s) = book(&[(0, 0, n(0.0)), (0, 1, f("10-(A1-3)^2"))]);
        let model = Model { objective: (0, 1), goal: Goal::Max, variables: vec![(0, 0)], method: Method::Grg, non_negative: false, ..Model::default() };
        let sol = solve(&p, s, &model, &never);
        assert!((sol.values[0] - 3.0).abs() < 1e-3, "{:?}", sol.values);
    }

    #[test]
    fn the_evolutionary_method_needs_bounds_and_finds_a_good_point() {
        let (p, s) = book(&[(0, 0, n(0.0)), (0, 1, f("10-(A1-3)^2"))]);
        let mut model = Model { objective: (0, 1), goal: Goal::Max, variables: vec![(0, 0)], method: Method::Evolutionary, non_negative: false, ..Model::default() };
        assert_eq!(solve(&p, s, &model, &never).status, Status::MissingBounds);
        model.constraints = vec![Constraint { lhs: vec![(0, 0)], rel: Rel::Le, rhs: Rhs::Number(10.0) }, Constraint { lhs: vec![(0, 0)], rel: Rel::Ge, rhs: Rhs::Number(-10.0) }];
        let sol = solve(&p, s, &model, &never);
        assert!((sol.values[0] - 3.0).abs() < 0.05, "{sol:?}");
    }

    #[test]
    fn the_model_is_kept_in_excels_names_and_read_back() {
        let d = Description {
            objective: "$B$1".into(),
            goal: Goal::Value(7.5),
            variables: "$A$1:$A$2".into(),
            constraints: vec![("$B$2".into(), Rel::Le, "4".into()), ("$B$3".into(), Rel::Ge, "$C$1".into()), ("$A$1:$A$2".into(), Rel::Int, String::new())],
            non_negative: false,
            method: Method::Simplex,
            precision: 1e-5,
            convergence: 1e-3,
            integer_tolerance: 2.0,
            max_iterations: 100,
            max_time: 30,
        };
        let names = d.to_names("My Sheet");
        let get = |k: &str| names.iter().find(|(n, _)| n == k).unwrap().1.clone();
        assert_eq!(get("solver_opt"), "'My Sheet'!$B$1");
        assert_eq!(get("solver_adj"), "'My Sheet'!$A$1:$A$2");
        assert_eq!(get("solver_typ"), "3");
        assert_eq!(get("solver_num"), "3");
        assert_eq!(get("solver_rhs2"), "'My Sheet'!$C$1");
        assert_eq!(get("solver_rhs3"), "integer");
        assert_eq!(get("solver_eng"), "2");
        let back = Description::from_names(&names).unwrap();
        assert_eq!(back.goal, d.goal);
        assert_eq!(back.constraints[1], ("B3".to_string(), Rel::Ge, "C1".to_string()));
        assert_eq!(back.variables, "A1:A2");
        assert_eq!((back.max_iterations, back.max_time, back.method, back.non_negative), (100, 30, Method::Simplex, false));
        let model = back.to_model().unwrap();
        assert_eq!(model.variables, vec![(0, 0), (1, 0)]);
        assert_eq!(model.constraints[1].rhs, Rhs::Cells(vec![(0, 2)]));
        assert!(Description { objective: "A1:A2".into(), ..Description::default() }.to_model().is_err());
    }

    #[test]
    fn sensitivity_and_limits_reports_match_lp_theory() {
        // Maximize 3*A1 + 2*A2 subject to 2*A1 + A2 <= 10 (B2) and A1 + 2*A2 <= 8 (B3).
        let (p, s) = book(&[(0, 0, n(0.0)), (1, 0, n(0.0)), (0, 1, f("3*A1+2*A2")), (1, 1, f("2*A1+A2")), (2, 1, f("A1+2*A2"))]);
        let model = Model {
            objective: (0, 1),
            goal: Goal::Max,
            variables: vec![(0, 0), (1, 0)],
            constraints: vec![
                Constraint { lhs: vec![(1, 1)], rel: Rel::Le, rhs: Rhs::Number(10.0) },
                Constraint { lhs: vec![(2, 1)], rel: Rel::Le, rhs: Rhs::Number(8.0) },
            ],
            method: Method::Simplex,
            ..Model::default()
        };
        let sol = solve(&p, s, &model, &never);
        assert_eq!(sol.status, Status::Optimal);
        assert!((sol.values[0] - 4.0).abs() < 1e-9 && (sol.values[1] - 2.0).abs() < 1e-9);
        assert!((sol.objective - 16.0).abs() < 1e-9);

        let sens = sensitivity(&p, s, &model, &sol);
        assert_eq!(sens.variables.len(), 2);
        assert!((sens.variables[0].final_value - 4.0).abs() < 1e-9);
        assert!((sens.variables[0].reduced_cost).abs() < 1e-9);
        assert!((sens.variables[0].objective_coefficient - 3.0).abs() < 1e-9);
        assert!((sens.variables[0].allowable_increase - 1.0).abs() < 1e-6);
        assert!((sens.variables[0].allowable_decrease - 2.0).abs() < 1e-6);
        assert!((sens.variables[1].final_value - 2.0).abs() < 1e-9);
        assert!((sens.variables[1].reduced_cost).abs() < 1e-9);
        assert!((sens.variables[1].objective_coefficient - 2.0).abs() < 1e-9);
        assert!((sens.variables[1].allowable_increase - 4.0).abs() < 1e-6);
        assert!((sens.variables[1].allowable_decrease - 0.5).abs() < 1e-6);

        assert_eq!(sens.constraints.len(), 2);
        assert!((sens.constraints[0].final_value - 10.0).abs() < 1e-9);
        assert!((sens.constraints[0].shadow_price - 4.0 / 3.0).abs() < 1e-6);
        assert!((sens.constraints[0].constraint_rhs - 10.0).abs() < 1e-9);
        assert!((sens.constraints[0].allowable_increase - 6.0).abs() < 1e-6);
        assert!((sens.constraints[0].allowable_decrease - 6.0).abs() < 1e-6);
        assert!((sens.constraints[1].final_value - 8.0).abs() < 1e-9);
        assert!((sens.constraints[1].shadow_price - 1.0 / 3.0).abs() < 1e-6);
        assert!((sens.constraints[1].constraint_rhs - 8.0).abs() < 1e-9);
        assert!((sens.constraints[1].allowable_increase - 12.0).abs() < 1e-6);
        assert!((sens.constraints[1].allowable_decrease - 3.0).abs() < 1e-6);

        let lim = limits(&p, s, &model, &sol);
        assert_eq!(lim.len(), 2);
        // Holding A2 = 2: A1 in [0, 4], objective in [4, 16].
        assert_eq!(lim[0].lower_limit, Some(0.0));
        assert_eq!(lim[0].lower_objective, Some(4.0));
        assert_eq!(lim[0].upper_limit, Some(4.0));
        assert_eq!(lim[0].upper_objective, Some(16.0));
        // Holding A1 = 4: A2 in [0, 2], objective in [12, 16].
        assert_eq!(lim[1].lower_limit, Some(0.0));
        assert_eq!(lim[1].lower_objective, Some(12.0));
        assert_eq!(lim[1].upper_limit, Some(2.0));
        assert_eq!(lim[1].upper_objective, Some(16.0));
    }
}
