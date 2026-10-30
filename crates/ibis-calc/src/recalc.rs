// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Calculating again the cells a change affects.
//!
//! After cells change, the formulas that depend on them (directly, or
//! through other formulas, or through defined names) are calculated again,
//! each after the cells it refers to. Cells no change reaches keep the
//! values Excel last calculated: the engine never calculates more than it
//! must, so a function it does not have yet changes nothing that did not
//! change.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use crate::eval::{Area, Array, Book, Cells, Ctx};
use crate::formula::{Expr, parse};
use crate::value::{CellError, CellValue};

/// A cell of the book: sheet, row, column.
pub type CellId = (usize, u32, u32);

/// A cell calculated again.
#[derive(Clone, Debug, PartialEq)]
pub struct Recalculated {
    pub cell: CellId,
    pub value: CellValue,
    /// The formula used a function the engine does not have (or a random
    /// or time function): the value is only a stand-in.
    pub unsupported: bool,
    /// A dynamic array formula's result of more than one value: rows,
    /// columns and the values row by row (the first is `value`), to spill.
    pub spill: Option<(usize, usize, Vec<CellValue>)>,
    /// The picture IMAGE asked for (its address and sizing), for the cell to show.
    pub image: Option<crate::eval::ImageSpec>,
}

/// What a formula refers to, for finding what depends on what.
#[derive(Clone, Debug, Default)]
pub struct Dependencies {
    /// Rectangles (sheet, first row, first column, last row, last column).
    pub areas: Vec<(usize, usize, u32, u32, u32, u32)>,
    /// Depends on everything (volatile functions, INDIRECT, OFFSET, names
    /// that could not be followed).
    pub volatile: bool,
}

fn dependencies(book: &dyn Book, sheet: usize, expr: &Expr, out: &mut Dependencies, depth: u32) {
    let resolve = |s: &Option<crate::formula::SheetRef>| -> Option<(usize, usize)> {
        match s {
            None => Some((sheet, sheet)),
            Some(r) if r.book.is_some() => None,
            Some(r) => {
                let a = book.sheet_index(&r.first)?;
                let b = r.last.as_ref().map_or(Some(a), |l| book.sheet_index(l))?;
                Some((a.min(b), a.max(b)))
            }
        }
    };
    match expr {
        Expr::Ref(r) => {
            if let Some((a, b)) = resolve(&r.sheet) {
                out.areas.push((a, b, r.start.row, r.start.column, r.end.row, r.end.column));
            }
        }
        Expr::Name { sheet: s, name } => {
            let scope = match s {
                Some(r) => book.sheet_index(&r.first),
                None => Some(sheet),
            };
            match book.defined_name(name, scope) {
                Some(f) if depth < 32 => match parse(f.trim_start_matches('=')) {
                    Ok(e) => dependencies(book, scope.unwrap_or(sheet), &e, out, depth + 1),
                    Err(_) => {}
                },
                Some(_) => out.volatile = true,
                // An unknown name (maybe bound by LET): nothing to follow.
                None => {}
            }
        }
        Expr::Call { name, args } => {
            if matches!(name.as_str(), "INDIRECT" | "OFFSET" | "NOW" | "TODAY" | "RAND" | "RANDBETWEEN" | "RANDARRAY" | "CELL" | "INFO") {
                out.volatile = true;
            }
            if crate::functions::lookup(name).is_none() && crate::functions::table::function_info(name).is_none() {
                // A defined name holding a LAMBDA.
                dependencies(book, sheet, &Expr::Name { sheet: None, name: name.clone() }, out, depth);
            }
            for a in args {
                dependencies(book, sheet, a, out, depth);
            }
        }
        Expr::Invoke { callee, args } => {
            dependencies(book, sheet, callee, out, depth);
            for a in args {
                dependencies(book, sheet, a, out, depth);
            }
        }
        Expr::Array(rows) => rows.iter().flatten().for_each(|e| dependencies(book, sheet, e, out, depth)),
        Expr::Neg(x) | Expr::Plus(x) | Expr::Percent(x) | Expr::Implicit(x) => dependencies(book, sheet, x, out, depth),
        Expr::Spill(inner) => {
            out.volatile = true;
            dependencies(book, sheet, inner, out, depth);
        }
        Expr::Structured { .. } => out.volatile = true,
        Expr::Binary(_, a, b) | Expr::Range(a, b) | Expr::Intersect(a, b) => {
            dependencies(book, sheet, a, out, depth);
            dependencies(book, sheet, b, out, depth);
        }
        Expr::Union(items) => items.iter().for_each(|e| dependencies(book, sheet, e, out, depth)),
        Expr::Number(_) | Expr::Text(_) | Expr::Bool(_) | Expr::Error(_) | Expr::Missing => {}
    }
}

/// A book's formulas, parsed once for a recalculation.
struct Formulas {
    parsed: HashMap<CellId, Option<Arc<Expr>>>,
}

/// The cells being calculated again, and their new values.
struct Recalc<'b> {
    dirty: HashSet<CellId>,
    formulas: RefCell<Formulas>,
    results: RefCell<HashMap<CellId, (CellValue, bool)>>,
    spills: RefCell<HashMap<CellId, (usize, usize, Vec<CellValue>)>>,
    in_progress: RefCell<HashSet<CellId>>,
    iter_values: RefCell<HashMap<CellId, CellValue>>,
    cache: RefCell<HashMap<String, Option<Arc<Expr>>>>,
    /// Ranges made into arrays, with the spill version they were made at.
    arrays: RefCell<HashMap<Area, (u64, Rc<Array>)>>,
    spill_version: std::cell::Cell<u64>,
    cycle_detected: std::cell::Cell<bool>,
    _book: std::marker::PhantomData<&'b ()>,
    /// What IMAGE asked for in each cell calculated.
    images: RefCell<HashMap<CellId, crate::eval::ImageSpec>>,
}

impl Recalc<'_> {
    fn expr(&self, book: &dyn Book, cell: CellId) -> Option<Arc<Expr>> {
        if let Some(e) = self.formulas.borrow().parsed.get(&cell) {
            return e.clone();
        }
        let text = book.formula(cell.0, cell.1, cell.2)?.to_string();
        let e = self.cache.borrow_mut().entry(text.clone()).or_insert_with(|| parse(&text).ok().map(Arc::new)).clone();
        self.formulas.borrow_mut().parsed.insert(cell, e.clone());
        e
    }

    fn calculate_eval(&self, book: &dyn Book, cell: CellId) -> (CellValue, bool) {
        match self.expr(book, cell) {
            None if book.formula(cell.0, cell.1, cell.2).is_some() => (CellValue::Error(CellError::Name), false),
            None => (book.value(cell.0, cell.1, cell.2), false),
            Some(expr) => {
                self.in_progress.borrow_mut().insert(cell);
                let prev_cycle = self.cycle_detected.replace(false);
                let mut ctx = Ctx::new(book, self, cell.0, cell.1, cell.2);
                let v = ctx.eval(&expr);
                let had_cycle = self.cycle_detected.get();
                self.cycle_detected.set(prev_cycle || had_cycle);
                let is_dynamic = book.dynamic(cell.0, cell.1, cell.2);
                if is_dynamic
                    && let Some(spill) = ctx.spill_result(&v)
                {
                    self.spills.borrow_mut().insert(cell, spill);
                    self.spill_version.set(self.spill_version.get() + 1);
                }
                let value = if had_cycle && !book.iterate() {
                    CellValue::Number(0.0)
                } else {
                    ctx.cell_result(v)
                };
                let unsupported = ctx.unsupported;
                match ctx.image.take() {
                    Some(image) => self.images.borrow_mut().insert(cell, image),
                    None => self.images.borrow_mut().remove(&cell),
                };
                self.in_progress.borrow_mut().remove(&cell);
                (value, unsupported)
            }
        }
    }

    fn calculate(&self, book: &dyn Book, cell: CellId) -> (CellValue, bool) {
        if let Some(r) = self.results.borrow().get(&cell) {
            return r.clone();
        }
        let result = self.calculate_eval(book, cell);
        self.results.borrow_mut().insert(cell, result.clone());
        result
    }
}

impl Cells for Recalc<'_> {
    fn fresh(&self, book: &dyn Book, sheet: usize, row: u32, column: u32) -> Option<CellValue> {
        let cell = (sheet, row, column);
        if let Some(v) = self.iter_values.borrow().get(&cell) {
            return Some(v.clone());
        }
        if !self.dirty.contains(&cell) {
            for (&(s, ar, ac), (rows, cols, values)) in self.spills.borrow().iter() {
                if s == sheet && row >= ar && row < ar + *rows as u32 && column >= ac && column < ac + *cols as u32 {
                    let idx = ((row - ar) as usize) * cols + ((column - ac) as usize);
                    if let Some(v) = values.get(idx) {
                        return Some(v.clone());
                    }
                }
            }
            return None;
        }
        if let Some(r) = self.results.borrow().get(&cell) {
            if r.1 {
                return Some(book.value(sheet, row, column));
            }
            return Some(r.0.clone());
        }
        if self.in_progress.borrow().contains(&cell) {
            // A circular reference: Excel shows 0 (and warns).
            self.cycle_detected.set(true);
            return Some(CellValue::Number(0.0));
        }
        let (value, unsupported) = self.calculate(book, cell);
        if unsupported {
            // Keep what Excel calculated for a formula we cannot.
            return Some(book.value(sheet, row, column));
        }
        Some(value)
    }

    fn cached_array(&self, area: &Area) -> Option<Rc<Array>> {
        if !self.iter_values.borrow().is_empty() {
            return None;
        }
        let arrays = self.arrays.borrow();
        let (version, array) = arrays.get(area)?;
        (*version == self.spill_version.get()).then(|| array.clone())
    }

    fn store_array(&self, area: &Area, array: &Rc<Array>) {
        // Only when every cell is final: not while a cell in the range is
        // being calculated (a circular reference) or while iterating.
        if !self.iter_values.borrow().is_empty() {
            return;
        }
        let busy = self.in_progress.borrow().iter().any(|&(s, r, c)| {
            s >= area.sheet && s <= area.last_sheet && r >= area.first_row && r <= area.last_row && c >= area.first_column && c <= area.last_column
        });
        if !busy {
            self.arrays.borrow_mut().insert(*area, (self.spill_version.get(), array.clone()));
        }
    }

    fn spill_area(&self, sheet: usize, row: u32, column: u32) -> Option<(u32, u32, u32, u32)> {
        let cell = (sheet, row, column);
        if let Some((rows, cols, _)) = self.spills.borrow().get(&cell) {
            return Some((row, column, row + *rows as u32 - 1, column + *cols as u32 - 1));
        }
        None
    }

    fn for_each_cell(&self, sheet: usize, (r0, c0, r1, c1): (u32, u32, u32, u32), f: &mut dyn FnMut(u32, u32, &CellValue)) {
        let results = self.results.borrow();
        // A small area is looked up cell by cell; scanning all the results
        // for each SUM of a few cells made a big workbook's calculation
        // take minutes (the results are as many as the formulas).
        let cells = (r1.saturating_sub(r0) as u64 + 1) * (c1.saturating_sub(c0) as u64 + 1);
        if cells <= results.len() as u64 {
            for r in r0..=r1 {
                for c in c0..=c1 {
                    if let Some((v, _)) = results.get(&(sheet, r, c)) {
                        f(r, c, v);
                    }
                }
            }
        } else {
            for (&(s, r, c), (v, _)) in results.iter() {
                if s == sheet && r >= r0 && r <= r1 && c >= c0 && c <= c1 {
                    f(r, c, v);
                }
            }
        }
        drop(results);
        for (&(s, ar, ac), (rows, cols, values)) in self.spills.borrow().iter() {
            if s == sheet {
                for dr in 0..*rows as u32 {
                    for dc in 0..*cols as u32 {
                        let r = ar + dr;
                        let c = ac + dc;
                        if r >= r0 && r <= r1 && c >= c0 && c <= c1 {
                            let idx = (dr as usize) * cols + (dc as usize);
                            if let Some(v) = values.get(idx) {
                                f(r, c, v);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn find_sccs(nodes: &[CellId], adj: &HashMap<CellId, Vec<CellId>>) -> Vec<Vec<CellId>> {
    struct State<'a> {
        adj: &'a HashMap<CellId, Vec<CellId>>,
        index: usize,
        indices: HashMap<CellId, usize>,
        lowlink: HashMap<CellId, usize>,
        on_stack: HashSet<CellId>,
        stack: Vec<CellId>,
        sccs: Vec<Vec<CellId>>,
    }

    fn strongconnect(u: CellId, st: &mut State) {
        st.indices.insert(u, st.index);
        st.lowlink.insert(u, st.index);
        st.index += 1;
        st.stack.push(u);
        st.on_stack.insert(u);

        if let Some(neighbors) = st.adj.get(&u) {
            for &v in neighbors {
                if !st.indices.contains_key(&v) {
                    strongconnect(v, st);
                    let v_low = st.lowlink[&v];
                    let u_low = st.lowlink.get_mut(&u).unwrap();
                    *u_low = (*u_low).min(v_low);
                } else if st.on_stack.contains(&v) {
                    let v_idx = st.indices[&v];
                    let u_low = st.lowlink.get_mut(&u).unwrap();
                    *u_low = (*u_low).min(v_idx);
                }
            }
        }

        if st.lowlink[&u] == st.indices[&u] {
            let mut scc = Vec::new();
            while let Some(w) = st.stack.pop() {
                st.on_stack.remove(&w);
                scc.push(w);
                if w == u {
                    break;
                }
            }
            st.sccs.push(scc);
        }
    }

    let mut state = State {
        adj,
        index: 0,
        indices: HashMap::new(),
        lowlink: HashMap::new(),
        on_stack: HashSet::new(),
        stack: Vec::new(),
        sccs: Vec::new(),
    };

    for &node in nodes {
        if !state.indices.contains_key(&node) {
            strongconnect(node, &mut state);
        }
    }

    state.sccs
}

/// Precomputed dependencies per referenced sheet.
#[derive(Clone, Debug, Default)]
pub struct SheetDependencies {
    /// Formulas referencing wide column ranges (> 32 cols) or the whole sheet
    pub wide_formulas: Vec<CellId>,
    /// Formulas referencing specific column: col -> Vec<CellId>
    pub by_col: HashMap<u32, Vec<CellId>>,
}

/// Precomputed dependencies and parsed ASTs for workbook formulas,
/// allowing sub-millisecond recalculation on cell edits.
#[derive(Clone, Debug, Default)]
pub struct DependencyIndex {
    /// Formula expressions per cell: CellId -> Option<Arc<Expr>>
    pub parsed: HashMap<CellId, Option<Arc<Expr>>>,
    /// Dependencies per cell: CellId -> Dependencies
    pub deps: HashMap<CellId, Dependencies>,
    /// List of volatile formula cells
    pub volatile: Vec<CellId>,
    /// For each sheet index S: dependencies touching sheet S
    pub sheets: Vec<SheetDependencies>,
    /// AST cache by formula string
    pub ast_cache: HashMap<String, Option<Arc<Expr>>>,
}

/// The largest sheet index among `cells` (an area's sheet range is cut to it).
fn sheet_count_hint(cells: &std::collections::BTreeSet<CellId>) -> usize {
    cells.iter().next_back().map_or(0, |c| c.0)
}

impl DependencyIndex {
    /// Build index from all formula cells of the book.
    pub fn build(book: &dyn Book, all_formulas: &[CellId]) -> Self {
        Self::build_with_cache(book, all_formulas, HashMap::new())
    }

    /// The parsed formulas by their text, to give to [`DependencyIndex::build_with_cache`]
    /// of the book after a change (what did not change is not parsed again).
    pub fn take_cache(&mut self) -> HashMap<String, Option<Arc<Expr>>> {
        std::mem::take(&mut self.ast_cache)
    }

    /// `build`, with formulas parsed earlier (`previous`, from `take_cache`).
    pub fn build_with_cache(book: &dyn Book, all_formulas: &[CellId], previous: HashMap<String, Option<Arc<Expr>>>) -> Self {
        let sheet_count = book.sheet_count();
        let mut parsed = HashMap::with_capacity(all_formulas.len());
        let mut deps = HashMap::with_capacity(all_formulas.len());
        let mut volatile = Vec::new();
        let mut sheets: Vec<SheetDependencies> = (0..sheet_count).map(|_| SheetDependencies::default()).collect();
        let mut ast_cache: HashMap<String, Option<Arc<Expr>>> = HashMap::new();

        for &cell in all_formulas {
            let Some(text) = book.formula(cell.0, cell.1, cell.2) else { continue };
            let expr = match ast_cache.get(text) {
                Some(e) => e.clone(),
                None => {
                    let e = previous.get(text).cloned().unwrap_or_else(|| parse(text).ok().map(Arc::new));
                    ast_cache.insert(text.to_string(), e.clone());
                    e
                }
            };
            let mut d = Dependencies::default();
            if let Some(e) = &expr {
                dependencies(book, cell.0, e, &mut d, 0);
            }
            if d.volatile {
                volatile.push(cell);
            }
            for &(s0, s1, _r0, c0, _r1, c1) in &d.areas {
                for s in s0..=s1 {
                    if s < sheet_count {
                        let col_count = c1.saturating_sub(c0) + 1;
                        if col_count > 32 {
                            sheets[s].wide_formulas.push(cell);
                        } else {
                            for c in c0..=c1 {
                                sheets[s].by_col.entry(c).or_default().push(cell);
                            }
                        }
                    }
                }
            }
            parsed.insert(cell, expr);
            deps.insert(cell, d);
        }

        Self {
            parsed,
            deps,
            volatile,
            sheets,
            ast_cache,
        }
    }

    /// Update or add a formula cell in the index.
    pub fn update_formula(&mut self, book: &dyn Book, cell: CellId) {
        self.remove_formula(&cell);
        let Some(text) = book.formula(cell.0, cell.1, cell.2) else { return };
        let expr = self.ast_cache.entry(text.to_string()).or_insert_with(|| parse(text).ok().map(Arc::new)).clone();
        let mut d = Dependencies::default();
        if let Some(e) = &expr {
            dependencies(book, cell.0, e, &mut d, 0);
        }
        if d.volatile {
            self.volatile.push(cell);
        }
        let sheet_count = book.sheet_count();
        if self.sheets.len() < sheet_count {
            self.sheets.resize_with(sheet_count, SheetDependencies::default);
        }
        for &(s0, s1, _r0, c0, _r1, c1) in &d.areas {
            for s in s0..=s1 {
                if s < self.sheets.len() {
                    let col_count = c1.saturating_sub(c0) + 1;
                    if col_count > 32 {
                        self.sheets[s].wide_formulas.push(cell);
                    } else {
                        for c in c0..=c1 {
                            self.sheets[s].by_col.entry(c).or_default().push(cell);
                        }
                    }
                }
            }
        }
        self.parsed.insert(cell, expr);
        self.deps.insert(cell, d);
    }

    /// Remove a formula cell from the index.
    pub fn remove_formula(&mut self, cell: &CellId) {
        if let Some(d) = self.deps.remove(cell) {
            self.parsed.remove(cell);
            self.volatile.retain(|c| c != cell);
            for &(s0, s1, _r0, c0, _r1, c1) in &d.areas {
                for s in s0..=s1 {
                    if s < self.sheets.len() {
                        let col_count = c1.saturating_sub(c0) + 1;
                        if col_count > 32 {
                            self.sheets[s].wide_formulas.retain(|c| c != cell);
                        } else {
                            for c in c0..=c1 {
                                if let Some(cols) = self.sheets[s].by_col.get_mut(&c) {
                                    cols.retain(|c| c != cell);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Identify dirty formula cells affected by `changed` cells.
    pub fn find_dirty(&self, changed: &[CellId]) -> HashSet<CellId> {
        let mut dirty: HashSet<CellId> = changed.iter().copied().filter(|c| self.parsed.contains_key(c)).collect();
        let mut frontier: Vec<CellId> = changed.to_vec();
        let mut first_round = true;

        while !frontier.is_empty() {
            let mut next = Vec::new();
            // The changed cells by (sheet, column, row), to ask whether any lies in an area.
            let by_column: std::collections::BTreeSet<(usize, u32, u32)> = frontier.iter().map(|&(s, r, c)| (s, c, r)).collect();

            let mut candidate_cells: Vec<CellId> = Vec::new();
            if first_round {
                candidate_cells.extend_from_slice(&self.volatile);
            }

            // What refers to a column is listed once per column, not once per
            // changed cell (a sort changes tens of thousands of cells).
            let mut sheets_seen: HashSet<usize> = HashSet::new();
            let mut columns_seen: HashSet<(usize, u32)> = HashSet::new();
            for &(s, _r, c) in &frontier {
                if s < self.sheets.len() {
                    let s_deps = &self.sheets[s];
                    if sheets_seen.insert(s) {
                        candidate_cells.extend_from_slice(&s_deps.wide_formulas);
                    }
                    if columns_seen.insert((s, c))
                        && let Some(cols) = s_deps.by_col.get(&c)
                    {
                        candidate_cells.extend_from_slice(cols);
                    }
                }
            }

            for cell in candidate_cells {
                if dirty.contains(&cell) {
                    continue;
                }
                let Some(d) = self.deps.get(&cell) else { continue };
                let hit = (first_round && d.volatile)
                    || d.areas.iter().any(|&(s0, s1, r0, c0, r1, c1)| {
                        if (c1 - c0) < 64 && (s1 - s0) < 64 {
                            (s0..=s1).any(|s| (c0..=c1).any(|c| by_column.range((s, c, r0)..=(s, c, r1)).next().is_some()))
                        } else {
                            frontier.iter().any(|&(s, r, c)| (s0..=s1).contains(&s) && (r0..=r1).contains(&r) && (c0..=c1).contains(&c))
                        }
                    });
                if hit {
                    dirty.insert(cell);
                    next.push(cell);
                }
            }
            first_round = false;
            frontier = next;
        }

        dirty
    }

    /// Calculate again what depends on `changed` cells using the precomputed index.
    pub fn recalculate(&self, book: &dyn Book, changed: &[CellId]) -> Vec<Recalculated> {
        let dirty = self.find_dirty(changed);
        self.recalculate_set(book, dirty)
    }

    /// Calculate exactly `cells` (each after the cells it refers to), not what
    /// depends on them: for cells whose formulas have no value yet, the rest
    /// of the book being as the file has it.
    pub fn calculate_only(&self, book: &dyn Book, cells: &[CellId]) -> Vec<Recalculated> {
        let dirty: HashSet<CellId> = cells.iter().copied().filter(|c| self.parsed.contains_key(c)).collect();
        self.recalculate_set(book, dirty)
    }

    fn recalculate_set(&self, book: &dyn Book, dirty: HashSet<CellId>) -> Vec<Recalculated> {
        if dirty.is_empty() {
            return Vec::new();
        }

        let recalc = Recalc {
            dirty: dirty.clone(),
            formulas: RefCell::new(Formulas { parsed: self.parsed.clone() }),
            results: RefCell::new(HashMap::new()),
            spills: RefCell::new(HashMap::new()),
            in_progress: RefCell::new(HashSet::new()),
            iter_values: RefCell::new(HashMap::new()),
            cache: RefCell::new(self.ast_cache.clone()),
            arrays: RefCell::new(HashMap::new()),
            spill_version: std::cell::Cell::new(0),
            cycle_detected: std::cell::Cell::new(false),
            _book: std::marker::PhantomData,
            images: Default::default(),
        };

        let mut dirty_cells: Vec<CellId> = dirty.iter().copied().collect();
        dirty_cells.sort();

        // Construct adjacency graph among dirty cells only
        // (Each area looks up the dirty cells inside it in an ordered index, rather
        // than testing every dirty cell: a workbook of tens of thousands of formulas
        // took quadratic time.)
        let ordered: std::collections::BTreeSet<CellId> = dirty_cells.iter().copied().collect();
        let mut adj: HashMap<CellId, Vec<CellId>> = HashMap::with_capacity(dirty_cells.len());
        for &cell in &dirty_cells {
            let mut neighbors: Vec<CellId> = Vec::new();
            if let Some(d) = self.deps.get(&cell) {
                for &(s0, s1, r0, c0, r1, c1) in &d.areas {
                    for sheet in s0..=s1.min(sheet_count_hint(&ordered)) {
                        for &other in ordered.range((sheet, r0, 0)..=(sheet, r1, u32::MAX)) {
                            if (c0..=c1).contains(&other.2) {
                                neighbors.push(other);
                            }
                        }
                    }
                }
                neighbors.sort();
                neighbors.dedup();
            }
            adj.insert(cell, neighbors);
        }

        let sccs = find_sccs(&dirty_cells, &adj);

        for mut scc in sccs {
            scc.sort();
            let is_cycle = scc.len() > 1 || (scc.len() == 1 && adj.get(&scc[0]).map_or(false, |nbrs| nbrs.contains(&scc[0])));

            if !is_cycle {
                let cell = scc[0];
                recalc.calculate(book, cell);
            } else if !book.iterate() {
                for &cell in &scc {
                    recalc.calculate(book, cell);
                }
            } else {
                // Circular reference with iterative calculation enabled: Gauss-Seidel iterations
                for &cell in &scc {
                    let initial = match book.value(cell.0, cell.1, cell.2) {
                        CellValue::Number(n) => CellValue::Number(n),
                        _ => CellValue::Number(0.0),
                    };
                    recalc.iter_values.borrow_mut().insert(cell, initial);
                }

                let max_iters = book.max_iterations().max(1);
                let max_change = book.max_change();

                for _ in 0..max_iters {
                    let mut max_diff: f64 = 0.0;
                    for &cell in &scc {
                        let (new_val, unsupported) = recalc.calculate_eval(book, cell);

                        let old_num = match recalc.iter_values.borrow().get(&cell) {
                            Some(CellValue::Number(n)) => Some(*n),
                            _ => None,
                        };
                        let new_num = match &new_val {
                            CellValue::Number(n) => Some(*n),
                            _ => None,
                        };
                        if let (Some(o), Some(n)) = (old_num, new_num) {
                            let diff = (n - o).abs();
                            if diff > max_diff {
                                max_diff = diff;
                            }
                        } else if old_num != new_num {
                            max_diff = f64::INFINITY;
                        }

                        recalc.iter_values.borrow_mut().insert(cell, new_val.clone());
                        recalc.results.borrow_mut().insert(cell, (new_val, unsupported));
                    }

                    if max_diff <= max_change {
                        break;
                    }
                }
            }
        }

        let mut cells: Vec<CellId> = recalc.dirty.iter().copied().collect();
        cells.sort();
        cells
            .into_iter()
            .map(|cell| {
                let (value, unsupported) = recalc.calculate(book, cell);
                let spill = recalc.spills.borrow().get(&cell).cloned();
                let image = recalc.images.borrow().get(&cell).cloned();
                Recalculated { cell, value, unsupported, spill, image }
            })
            .collect()
    }
}

/// Calculate again what depends on the `changed` cells (whose new values
/// or formulas are already in the book), and the changed cells that have
/// formulas. `all_formulas` lists every formula cell of the book.
pub fn recalculate(book: &dyn Book, changed: &[CellId], all_formulas: &[CellId]) -> Vec<Recalculated> {
    let index = DependencyIndex::build(book, all_formulas);
    index.recalculate(book, changed)
}

/// Calculate one formula for a cell as things stand (no recalculation of
/// what it refers to), for checking the engine against Excel's values.
pub fn evaluate(book: &dyn Book, cell: CellId, formula: &str) -> (CellValue, bool) {
    match parse(formula) {
        Ok(expr) => {
            let mut ctx = Ctx::new(book, &crate::eval::Stored, cell.0, cell.1, cell.2);
            let v = ctx.eval(&expr);
            let value = ctx.cell_result(v);
            (value, ctx.unsupported)
        }
        Err(_) => (CellValue::Error(CellError::Name), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockBook {
        sheets: Vec<String>,
        formulas: HashMap<CellId, String>,
        values: HashMap<CellId, CellValue>,
        iterate: bool,
        max_iterations: u32,
        max_change: f64,
        dynamic: bool,
        mock_pivots: HashMap<String, CellValue>,
        external_sheets: Vec<(&'static str, &'static str, usize)>,
    }

    impl MockBook {
        fn new(iterate: bool) -> Self {
            Self {
                sheets: vec!["Sheet1".to_string()],
                formulas: HashMap::new(),
                values: HashMap::new(),
                iterate,
                max_iterations: 100,
                max_change: 0.001,
                dynamic: false,
                mock_pivots: HashMap::new(),
                external_sheets: Vec::new(),
            }
        }
    }

    impl Book for MockBook {
        fn sheet_count(&self) -> usize {
            self.sheets.len()
        }
        fn sheet_name(&self, sheet: usize) -> &str {
            self.sheets.get(sheet).map(|s| s.as_str()).unwrap_or("")
        }
        fn value(&self, sheet: usize, row: u32, column: u32) -> CellValue {
            self.values.get(&(sheet, row, column)).cloned().unwrap_or(CellValue::Empty)
        }
        fn formula(&self, sheet: usize, row: u32, column: u32) -> Option<&str> {
            self.formulas.get(&(sheet, row, column)).map(|s| s.as_str())
        }
        fn for_each_cell(&self, sheet: usize, (r0, c0, r1, c1): (u32, u32, u32, u32), f: &mut dyn FnMut(u32, u32, &CellValue)) {
            for r in r0..=r1 {
                for c in c0..=c1 {
                    let v = self.value(sheet, r, c);
                    if !v.is_empty() {
                        f(r, c, &v);
                    }
                }
            }
        }
        fn extent(&self, _sheet: usize) -> (u32, u32) {
            (100, 100)
        }
        fn defined_name(&self, _name: &str, _sheet: Option<usize>) -> Option<String> {
            None
        }
        fn iterate(&self) -> bool {
            self.iterate
        }
        fn max_iterations(&self) -> u32 {
            self.max_iterations
        }
        fn max_change(&self) -> f64 {
            self.max_change
        }
        fn dynamic(&self, _sheet: usize, _row: u32, _column: u32) -> bool {
            self.dynamic
        }
        fn pivot_data(
            &self,
            _sheet: usize,
            _row: u32,
            _column: u32,
            data_field: &str,
            fields: &[(&str, &str)],
        ) -> Option<CellValue> {
            let key = format!("{}:{}", data_field, fields.iter().map(|(f, it)| format!("{f}={it}")).collect::<Vec<_>>().join(";"));
            self.mock_pivots.get(&key).cloned()
        }
        fn external_sheet(&self, book: &str, sheet: &str) -> Option<usize> {
            let raw = if let Some(p) = book.find('[') && let Some(e) = book[p + 1..].find(']') {
                &book[p + 1..p + 1 + e]
            } else {
                book
            };
            self.external_sheets.iter().find(|(b, s, _)| (b.eq_ignore_ascii_case(book) || b.eq_ignore_ascii_case(raw)) && s.eq_ignore_ascii_case(sheet)).map(|(_, _, idx)| *idx)
        }
        fn external_sheets(&self, book: &str, first: &str, last: &str) -> Option<(usize, usize)> {
            let f = self.external_sheet(book, first)?;
            let l = self.external_sheet(book, last)?;
            let min = f.min(l);
            let max = f.max(l);
            let raw = if let Some(p) = book.find('[') && let Some(e) = book[p + 1..].find(']') {
                &book[p + 1..p + 1 + e]
            } else {
                book
            };
            for i in min..=max {
                if !self.external_sheets.iter().any(|(b, _, idx)| (b.eq_ignore_ascii_case(book) || b.eq_ignore_ascii_case(raw)) && *idx == i) {
                    return None;
                }
            }
            Some((min, max))
        }
    }

    #[test]
    fn github_44_45_46_50_array_limits_and_reference_values() {
        let mut book=MockBook::new(false);
        book.dynamic=true;
        book.values.insert((0,0,0),CellValue::Number(4.));
        book.values.insert((0,1,0),CellValue::Number(6.));
        let cases=[
            ("SEQUENCE(2^20+1)",CellValue::Error(CellError::Num)),
            ("SEQUENCE(1,2^14+1)",CellValue::Error(CellError::Num)),
            ("ISREF(TAKE(A1,1))",CellValue::Bool(true)),
            ("SUM(TAKE(A1:A2,-1))",CellValue::Number(6.)),
            ("LAMBDA(a,[b],a+b)(10)",CellValue::Number(10.)),
            ("LAMBDA(a,[b],a+b)(10,20)",CellValue::Number(30.)),
            ("LAMBDA(a,[b],ISOMITTED(b))(10)",CellValue::Bool(true)),
            ("ISREF(IF(1,A1,A1))",CellValue::Bool(true)),
            ("SUM(LET(a,IF(1,A1,A1),a:A2))",CellValue::Number(10.)),
        ];
        for (i,(formula,_)) in cases.iter().enumerate() {book.formulas.insert((0,i as u32,3),formula.to_string());}
        let cells:Vec<_>=(0..cases.len()).map(|i|(0,i as u32,3)).collect();
        let result=recalculate(&book,&cells,&cells);
        for (i,(formula,want)) in cases.iter().enumerate() {
            assert_eq!(&result.iter().find(|r|r.cell==(0,i as u32,3)).unwrap().value,want,"{formula}");
        }
    }

    #[test]
    fn a_lookup_table_shared_within_a_recalculation_is_not_stale_after_an_edit() {
        let mut book = MockBook::new(false);
        // A1:A100 keys 1..100, B1:B100 = A * 2 (formulas, so the table is made of results).
        let mut formulas = Vec::new();
        for r in 0..100u32 {
            book.values.insert((0, r, 0), CellValue::Number((r + 1) as f64));
            book.formulas.insert((0, r, 1), format!("A{}*2", r + 1));
            formulas.push((0, r, 1));
        }
        // D1..D100 look up the same range cell by cell; E1 goes through INDIRECT, F1 through OFFSET.
        for r in 0..100u32 {
            book.formulas.insert((0, r, 3), format!("VLOOKUP(A{},$A$1:$B$100,2,FALSE)", r + 1));
            formulas.push((0, r, 3));
        }
        book.formulas.insert((0, 0, 4), "VLOOKUP(5,INDIRECT(\"A1:B100\"),2,FALSE)".to_string());
        book.formulas.insert((0, 0, 5), "VLOOKUP(5,OFFSET(A1,0,0,100,2),2,FALSE)".to_string());
        formulas.push((0, 0, 4));
        formulas.push((0, 0, 5));
        let get = |res: &[Recalculated], cell: (usize, u32, u32)| res.iter().find(|r| r.cell == cell).map(|r| r.value.clone()).unwrap();

        let all = recalculate(&book, &formulas, &formulas);
        assert_eq!(get(&all, (0, 4, 3)), CellValue::Number(10.0));
        assert_eq!(get(&all, (0, 0, 4)), CellValue::Number(10.0));
        // The cached values are what the book holds now.
        for r in &all {
            book.values.insert(r.cell, r.value.clone());
        }

        // Change A5 (the key of row 5 becomes 500): only what depends on it is calculated again,
        // and the lookups must see the new range, not the table made before.
        book.values.insert((0, 4, 0), CellValue::Number(500.0));
        let part = recalculate(&book, &[(0, 4, 0)], &formulas);
        assert_eq!(get(&part, (0, 4, 1)), CellValue::Number(1000.0));
        assert_eq!(get(&part, (0, 4, 3)), CellValue::Number(1000.0));
        assert_eq!(get(&part, (0, 0, 4)), CellValue::Error(CellError::NA));
        assert_eq!(get(&part, (0, 0, 5)), CellValue::Error(CellError::NA));
    }

    #[test]
    fn test_circular_without_iterative() {
        let mut book = MockBook::new(false);
        // B1 (0, 0, 1) = B1 + 1
        book.formulas.insert((0, 0, 1), "B1+1".to_string());
        let res = recalculate(&book, &[(0, 0, 1)], &[(0, 0, 1)]);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].value, CellValue::Number(0.0));
    }

    #[test]
    fn test_iterative_self_addition() {
        let mut book = MockBook::new(true);
        // B1 (0, 0, 1) = B1 + 1 (100 iterations)
        book.formulas.insert((0, 0, 1), "B1+1".to_string());
        let res = recalculate(&book, &[(0, 0, 1)], &[(0, 0, 1)]);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].value, CellValue::Number(100.0));
    }

    #[test]
    fn test_iterative_if_counter() {
        let mut book = MockBook::new(true);
        // B3 (0, 2, 1) = IF(B3<10, B3+1, B3)
        book.formulas.insert((0, 2, 1), "IF(B3<10, B3+1, B3)".to_string());
        let res = recalculate(&book, &[(0, 2, 1)], &[(0, 2, 1)]);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].value, CellValue::Number(10.0));
    }

    #[test]
    fn test_iterative_linear_system_and_downstream() {
        let mut book = MockBook::new(true);
        // B5 (0, 4, 1) = (100-C5)/2
        // C5 (0, 4, 2) = (80-B5)/3
        // D5 (0, 4, 3) = B5 + C5  (downstream non-cyclic formula)
        book.formulas.insert((0, 4, 1), "(100-C5)/2".to_string());
        book.formulas.insert((0, 4, 2), "(80-B5)/3".to_string());
        book.formulas.insert((0, 4, 3), "B5+C5".to_string());

        let formulas = vec![(0, 4, 1), (0, 4, 2), (0, 4, 3)];
        let res = recalculate(&book, &formulas, &formulas);
        assert_eq!(res.len(), 3);

        let b5_val = res.iter().find(|r| r.cell == (0, 4, 1)).unwrap().value.as_number().unwrap();
        let c5_val = res.iter().find(|r| r.cell == (0, 4, 2)).unwrap().value.as_number().unwrap();
        let d5_val = res.iter().find(|r| r.cell == (0, 4, 3)).unwrap().value.as_number().unwrap();

        // Exact analytical solution:
        // 2*B5 + C5 = 100
        // B5 + 3*C5 = 80
        // 6*B5 + 3*C5 = 300 => 5*B5 = 220 => B5 = 44, C5 = 12
        assert!((b5_val - 44.0).abs() < 0.01, "B5 was {}", b5_val);
        assert!((c5_val - 12.0).abs() < 0.01, "C5 was {}", c5_val);
        assert!((d5_val - 56.0).abs() < 0.01, "D5 was {}", d5_val);
    }

    #[test]
    fn test_array_constants() {
        let mut book = MockBook::new(false);
        book.dynamic = true;
        // Direct spill: A1 (0, 0, 0) = {1, 2; 3, 4}
        book.formulas.insert((0, 0, 0), "{1, 2; 3, 4}".to_string());
        // SUM with array constant: C1 (0, 0, 2) = SUM({1, 2; 3, 4})
        book.formulas.insert((0, 0, 2), "SUM({1, 2; 3, 4})".to_string());
        // AVERAGE with 1D array constant: C2 (0, 1, 2) = AVERAGE({2, 4, 6})
        book.formulas.insert((0, 1, 2), "AVERAGE({2, 4, 6})".to_string());
        // INDEX with array constant: C3 (0, 2, 2) = INDEX({10, 20; 30, 40}, 2, 1)
        book.formulas.insert((0, 2, 2), "INDEX({10, 20; 30, 40}, 2, 1)".to_string());
        // VLOOKUP with array constant: C4 (0, 3, 2) = VLOOKUP(2, {1, "A"; 2, "B"; 3, "C"}, 2, FALSE)
        book.formulas.insert((0, 3, 2), "VLOOKUP(2, {1, \"A\"; 2, \"B\"; 3, \"C\"}, 2, FALSE)".to_string());
        // Array arithmetic broadcast: C5 (0, 4, 2) = {1, 2} + 10 (spill 1x2)
        book.formulas.insert((0, 4, 2), "{1, 2} + 10".to_string());

        let cells = vec![(0, 0, 0), (0, 0, 2), (0, 1, 2), (0, 2, 2), (0, 3, 2), (0, 4, 2)];
        let res = recalculate(&book, &cells, &cells);

        // A1 spills 2x2
        let a1 = res.iter().find(|r| r.cell == (0, 0, 0)).unwrap();
        assert_eq!(a1.value, CellValue::Number(1.0));
        assert_eq!(a1.spill, Some((2, 2, vec![CellValue::Number(1.0), CellValue::Number(2.0), CellValue::Number(3.0), CellValue::Number(4.0)])));

        // C1 = SUM({1, 2; 3, 4}) = 10
        let c1 = res.iter().find(|r| r.cell == (0, 0, 2)).unwrap();
        assert_eq!(c1.value, CellValue::Number(10.0));
        assert_eq!(c1.spill, None);

        // C2 = AVERAGE({2, 4, 6}) = 4
        let c2 = res.iter().find(|r| r.cell == (0, 1, 2)).unwrap();
        assert_eq!(c2.value, CellValue::Number(4.0));
        assert_eq!(c2.spill, None);

        // C3 = INDEX({10, 20; 30, 40}, 2, 1) = 30
        let c3 = res.iter().find(|r| r.cell == (0, 2, 2)).unwrap();
        assert_eq!(c3.value, CellValue::Number(30.0));

        // C4 = VLOOKUP(2, {1, "A"; 2, "B"; 3, "C"}, 2, FALSE) = "B"
        let c4 = res.iter().find(|r| r.cell == (0, 3, 2)).unwrap();
        assert_eq!(c4.value, CellValue::Text("B".to_string()));

        // C5 = {1, 2} + 10 spills 1x2: [11, 12]
        let c5 = res.iter().find(|r| r.cell == (0, 4, 2)).unwrap();
        assert_eq!(c5.value, CellValue::Number(11.0));
        assert_eq!(c5.spill, Some((1, 2, vec![CellValue::Number(11.0), CellValue::Number(12.0)])));

        // Mixed types: C6 (0, 5, 2) = {"hello", TRUE; 100, #N/A}
        book.formulas.insert((0, 5, 2), "{\"hello\", TRUE; 100, #N/A}".to_string());
        let res2 = recalculate(&book, &[(0, 5, 2)], &[(0, 5, 2)]);
        let c6 = res2.iter().find(|r| r.cell == (0, 5, 2)).unwrap();
        assert_eq!(c6.value, CellValue::Text("hello".to_string()));
        assert_eq!(c6.spill, Some((2, 2, vec![
            CellValue::Text("hello".to_string()),
            CellValue::Bool(true),
            CellValue::Number(100.0),
            CellValue::Error(CellError::NA),
        ])));

        // Syntax error: unequal row widths
        assert!(parse("{1, 2; 3}").is_err());
        assert!(parse("{1, 2; 3, 4, 5}").is_err());
    }

    #[test]
    fn test_3d_references() {
        let mut book = MockBook::new(false);
        book.sheets = vec!["Sheet1".to_string(), "Sheet2".to_string(), "Sheet3".to_string()];

        // Values across 3 sheets:
        // Sheet1 (0): A1=10, B1=5
        book.values.insert((0, 0, 0), CellValue::Number(10.0));
        book.values.insert((0, 0, 1), CellValue::Number(5.0));

        // Sheet2 (1): A1=20, B1=15
        book.values.insert((1, 0, 0), CellValue::Number(20.0));
        book.values.insert((1, 0, 1), CellValue::Number(15.0));

        // Sheet3 (2): A1=30, B1=25
        book.values.insert((2, 0, 0), CellValue::Number(30.0));
        book.values.insert((2, 0, 1), CellValue::Number(25.0));

        // Formulas in Sheet1:
        // C1 (0, 0, 2) = SUM(Sheet1:Sheet3!A1) -> 60
        book.formulas.insert((0, 0, 2), "SUM(Sheet1:Sheet3!A1)".to_string());
        // C2 (0, 1, 2) = AVERAGE(Sheet1:Sheet3!A1) -> 20
        book.formulas.insert((0, 1, 2), "AVERAGE(Sheet1:Sheet3!A1)".to_string());
        // C3 (0, 2, 2) = COUNT(Sheet1:Sheet3!A1) -> 3
        book.formulas.insert((0, 2, 2), "COUNT(Sheet1:Sheet3!A1)".to_string());
        // C4 (0, 3, 2) = MAX(Sheet1:Sheet3!A1) -> 30
        book.formulas.insert((0, 3, 2), "MAX(Sheet1:Sheet3!A1)".to_string());
        // C5 (0, 4, 2) = MIN(Sheet1:Sheet3!A1) -> 10
        book.formulas.insert((0, 4, 2), "MIN(Sheet1:Sheet3!A1)".to_string());
        // C6 (0, 5, 2) = SUM(Sheet1:Sheet3!A1:B1) -> 105
        book.formulas.insert((0, 5, 2), "SUM(Sheet1:Sheet3!A1:B1)".to_string());
        // C7 (0, 6, 2) = SUM(Sheet3:Sheet1!A1) -> 60 (reverse sheet order)
        book.formulas.insert((0, 6, 2), "SUM(Sheet3:Sheet1!A1)".to_string());

        let formulas = vec![
            (0, 0, 2),
            (0, 1, 2),
            (0, 2, 2),
            (0, 3, 2),
            (0, 4, 2),
            (0, 5, 2),
            (0, 6, 2),
        ];

        let res = recalculate(&book, &formulas, &formulas);
        assert_eq!(res.iter().find(|r| r.cell == (0, 0, 2)).unwrap().value, CellValue::Number(60.0));
        assert_eq!(res.iter().find(|r| r.cell == (0, 1, 2)).unwrap().value, CellValue::Number(20.0));
        assert_eq!(res.iter().find(|r| r.cell == (0, 2, 2)).unwrap().value, CellValue::Number(3.0));
        assert_eq!(res.iter().find(|r| r.cell == (0, 3, 2)).unwrap().value, CellValue::Number(30.0));
        assert_eq!(res.iter().find(|r| r.cell == (0, 4, 2)).unwrap().value, CellValue::Number(10.0));
        assert_eq!(res.iter().find(|r| r.cell == (0, 5, 2)).unwrap().value, CellValue::Number(105.0));
        assert_eq!(res.iter().find(|r| r.cell == (0, 6, 2)).unwrap().value, CellValue::Number(60.0));

        // Cascade recalculation test:
        // Change Sheet2 A1 from 20 to 50
        book.values.insert((1, 0, 0), CellValue::Number(50.0));
        let changed = vec![(1, 0, 0)];
        let res2 = recalculate(&book, &changed, &formulas);
        let c1_new = res2.iter().find(|r| r.cell == (0, 0, 2)).unwrap();
        assert_eq!(c1_new.value, CellValue::Number(90.0), "C1 recalculated from 60 to 90");
    }

    #[test]
    fn test_external_3d_and_path_references() {
        let mut book = MockBook::new(false);
        book.sheets = vec!["Main".to_string()];

        // External sheets past sheet_count (1):
        // Budget.xlsx has Sheet1(1), Sheet2(2), Sheet3(3)
        book.external_sheets.push(("Budget.xlsx", "Sheet1", 1));
        book.external_sheets.push(("Budget.xlsx", "Sheet2", 2));
        book.external_sheets.push(("Budget.xlsx", "Sheet3", 3));

        // Budget 2026.xlsx has same sheets
        book.external_sheets.push(("Budget 2026.xlsx", "Sheet1", 1));
        book.external_sheets.push(("Budget 2026.xlsx", "Sheet2", 2));
        book.external_sheets.push(("Budget 2026.xlsx", "Sheet3", 3));

        // Other.xlsx has SheetX(4)
        book.external_sheets.push(("Other.xlsx", "SheetX", 4));

        // Values in external sheets:
        book.values.insert((1, 0, 0), CellValue::Number(10.0));
        book.values.insert((1, 0, 1), CellValue::Number(5.0));

        book.values.insert((2, 0, 0), CellValue::Number(20.0));
        book.values.insert((2, 0, 1), CellValue::Number(15.0));

        book.values.insert((3, 0, 0), CellValue::Number(30.0));
        book.values.insert((3, 0, 1), CellValue::Number(25.0));

        book.values.insert((4, 0, 0), CellValue::Number(100.0));

        let cases = vec![
            ((0, 0, 0), "SUM([Budget.xlsx]Sheet1:Sheet3!A1)", CellValue::Number(60.0)),
            ((0, 1, 0), "AVERAGE([Budget.xlsx]Sheet1:Sheet3!A1)", CellValue::Number(20.0)),
            ((0, 2, 0), "COUNT([Budget.xlsx]Sheet1:Sheet3!A1)", CellValue::Number(3.0)),
            ((0, 3, 0), "MAX([Budget.xlsx]Sheet1:Sheet3!A1)", CellValue::Number(30.0)),
            ((0, 4, 0), "MIN([Budget.xlsx]Sheet1:Sheet3!A1)", CellValue::Number(10.0)),
            ((0, 5, 0), "SUM([Budget.xlsx]Sheet1:Sheet3!A1:B1)", CellValue::Number(105.0)),
            ((0, 6, 0), "SUM('[Budget 2026.xlsx]Sheet1:Sheet3'!A1)", CellValue::Number(60.0)),
            ((0, 7, 0), "SUM('C:\\dir\\[Budget.xlsx]Sheet1:Sheet3'!A1)", CellValue::Number(60.0)),
            ((0, 8, 0), "SUM('C:\\dir\\[Budget.xlsx]Sheet1'!A1)", CellValue::Number(10.0)),
            ((0, 9, 0), "SUM([Budget.xlsx]Sheet1:Missing!A1)", CellValue::Error(CellError::Ref)),
            ((0, 10, 0), "SUM([Budget.xlsx]Sheet1:SheetX!A1)", CellValue::Error(CellError::Ref)),
        ];

        let mut formulas = Vec::new();
        for (cell, formula, _) in &cases {
            book.formulas.insert(*cell, formula.to_string());
            formulas.push(*cell);
        }

        let res = recalculate(&book, &formulas, &formulas);
        for (cell, formula, expected) in &cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_operator_precedence() {
        let mut book = MockBook::new(false);

        // Cells for reference-based calculations
        // A1 (0, 0, 0) = 10, B1 (0, 0, 1) = 2, C1 (0, 0, 2) = 3
        book.values.insert((0, 0, 0), CellValue::Number(10.0));
        book.values.insert((0, 0, 1), CellValue::Number(2.0));
        book.values.insert((0, 0, 2), CellValue::Number(3.0));

        let cases = vec![
            // 1. Basic arithmetic and parentheses
            ((0, 1, 0), "1+2*3", CellValue::Number(7.0)),
            ((0, 1, 1), "(1+2)*3", CellValue::Number(9.0)),
            ((0, 1, 2), "10-4/2", CellValue::Number(8.0)),
            ((0, 1, 3), "16/4/2", CellValue::Number(2.0)),
            ((0, 1, 4), "10-5-2", CellValue::Number(3.0)),

            // 2. Power and unary minus
            ((0, 2, 0), "2*3^2", CellValue::Number(18.0)),
            ((0, 2, 1), "(2*3)^2", CellValue::Number(36.0)),
            ((0, 2, 2), "2^3^2", CellValue::Number(64.0)), // left-associative in Excel: (2^3)^2 = 64
            ((0, 2, 3), "-2^2", CellValue::Number(4.0)),   // Excel unary minus binds tighter than power: (-2)^2 = 4
            ((0, 2, 4), "-(2^2)", CellValue::Number(-4.0)),
            ((0, 2, 5), "10-2^3", CellValue::Number(2.0)),

            // 3. Percent operator
            ((0, 3, 0), "50%*200", CellValue::Number(100.0)),
            ((0, 3, 1), "10+50%", CellValue::Number(10.5)),
            ((0, 3, 2), "20%+30%", CellValue::Number(0.5)),

            // 4. String concatenation
            ((0, 4, 0), "1+2&3+4", CellValue::Text("37".into())),
            ((0, 4, 1), "\"Val:\"&10*2", CellValue::Text("Val:20".into())),

            // 5. Comparison operators
            ((0, 5, 0), "1+2=3", CellValue::Bool(true)),
            ((0, 5, 1), "1+2*3=7", CellValue::Bool(true)),
            ((0, 5, 2), "1+2*3=9", CellValue::Bool(false)),
            ((0, 5, 3), "1&2=\"12\"", CellValue::Bool(true)),
            ((0, 5, 4), "1+2<2*2", CellValue::Bool(true)),
            ((0, 5, 5), "5>=2+3", CellValue::Bool(true)),
            ((0, 5, 6), "10<>5*2", CellValue::Bool(false)),
            ((0, 5, 7), "1<2=TRUE", CellValue::Bool(true)),

            // 6. Cell references with operator precedence
            ((0, 6, 0), "A1+B1*C1", CellValue::Number(16.0)),
            ((0, 6, 1), "(A1+B1)*C1", CellValue::Number(36.0)),
            ((0, 6, 2), "-B1^2", CellValue::Number(4.0)), // (-2)^2 = 4
            ((0, 6, 3), "A1&B1", CellValue::Text("102".into())),
            ((0, 6, 4), "A1>B1*C1", CellValue::Bool(true)), // 10 > 6 = true
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in &cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in &cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_implicit_intersection_operator() {
        let mut book = MockBook::new(false);
        book.dynamic = true; // Dynamic array environment

        // Data cells:
        // Column range A1:A5 (rows 0..=4, column 0)
        book.values.insert((0, 0, 0), CellValue::Number(10.0));
        book.values.insert((0, 1, 0), CellValue::Number(20.0));
        book.values.insert((0, 2, 0), CellValue::Number(30.0));
        book.values.insert((0, 3, 0), CellValue::Number(40.0));
        book.values.insert((0, 4, 0), CellValue::Number(50.0));

        // Row range C1:E1 (row 0, columns 2..=4)
        book.values.insert((0, 0, 2), CellValue::Number(100.0));
        book.values.insert((0, 0, 3), CellValue::Number(200.0));
        book.values.insert((0, 0, 4), CellValue::Number(300.0));

        let cases = vec![
            // 1. Column range intersection by row:
            ((0, 0, 1), "@A1:A5", CellValue::Number(10.0)), // row 0 intersects with A1
            ((0, 1, 1), "@A1:A5", CellValue::Number(20.0)), // row 1 intersects with A2
            ((0, 2, 1), "@A1:A5", CellValue::Number(30.0)), // row 2 intersects with A3
            ((0, 4, 1), "@A1:A5", CellValue::Number(50.0)), // row 4 intersects with A5
            ((0, 9, 1), "@A1:A5", CellValue::Error(CellError::Value)), // row 9 does not intersect -> #VALUE!

            // 2. Row range intersection by column:
            ((0, 2, 2), "@C1:E1", CellValue::Number(100.0)), // col 2 intersects with C1
            ((0, 2, 3), "@C1:E1", CellValue::Number(200.0)), // col 3 intersects with D1
            ((0, 2, 4), "@C1:E1", CellValue::Number(300.0)), // col 4 intersects with E1
            ((0, 2, 5), "@C1:E1", CellValue::Error(CellError::Value)), // col 5 does not intersect -> #VALUE!

            // 3. Array constants and dynamic formulas (suppress spill, extract first element):
            ((0, 0, 6), "@{1000, 2000; 3000, 4000}", CellValue::Number(1000.0)),
            ((0, 1, 6), "@SEQUENCE(5)", CellValue::Number(1.0)),

            // 4. Combined with arithmetic:
            ((0, 1, 7), "10+@A1:A5", CellValue::Number(30.0)), // 10 + 20 (row 1) = 30
            ((0, 2, 7), "@A1:A5*2", CellValue::Number(60.0)),  // 30 * 2 (row 2) = 60
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in &cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in &cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }

        // 5. Cascade recalculation: change A2 from 20 to 99
        book.values.insert((0, 1, 0), CellValue::Number(99.0));
        let changed = vec![(0, 1, 0)];
        let res2 = recalculate(&book, &changed, &formula_cells);

        // B2 (row 1, col 1) should update to 99
        let b2 = res2.iter().find(|r| r.cell == (0, 1, 1)).unwrap();
        assert_eq!(b2.value, CellValue::Number(99.0), "B2 recalculated to 99");

        // H2 (row 1, col 7) = 10+@A1:A5 should update to 109
        let h2 = res2.iter().find(|r| r.cell == (0, 1, 7)).unwrap();
        assert_eq!(h2.value, CellValue::Number(109.0), "H2 recalculated to 109");
    }

    #[test]
    fn test_text_delimiter_functions() {
        let mut book = MockBook::new(false);

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Fractional match_mode and match_end truncated
            ((0, 0, 0), "TEXTBEFORE(\"aB|ab\", \"ab\", 1, 0.5, 0, \"missing\")", CellValue::Text("aB|".into())),
            ((0, 0, 1), "TEXTBEFORE(\"abc\", \"x\", 1, 0, 0.5, \"missing\")", CellValue::Text("missing".into())),
            ((0, 0, 2), "TEXTAFTER(\"aB|ab\", \"ab\", 1, 0.5, 0, \"missing\")", CellValue::Text("".into())),
            ((0, 0, 3), "TEXTAFTER(\"abc\", \"x\", 1, 0, 0.5, \"missing\")", CellValue::Text("missing".into())),

            // 2. Negative instance out of bounds -> #VALUE!
            ((0, 1, 0), "TEXTBEFORE(\"abc\", \"x\", -1000, 0, 0, \"missing\")", CellValue::Error(CellError::Value)),
            ((0, 1, 1), "TEXTBEFORE(\"abc\", \"x\", -4, 0, 0, \"missing\")", CellValue::Error(CellError::Value)),
            ((0, 1, 2), "TEXTAFTER(\"abc\", \"x\", -1000, 0, 0, \"missing\")", CellValue::Error(CellError::Value)),
            ((0, 1, 3), "TEXTAFTER(\"abc\", \"x\", -4, 0, 0, \"missing\")", CellValue::Error(CellError::Value)),

            // 3. Empty text -> #VALUE!
            ((0, 2, 0), "TEXTBEFORE(\"\", \"x\", 1, 0, 0, \"missing\")", CellValue::Error(CellError::Value)),
            ((0, 2, 1), "TEXTAFTER(\"\", \"x\", 1, 0, 0, \"missing\")", CellValue::Error(CellError::Value)),

            // 4. Empty delimiter
            ((0, 3, 0), "TEXTBEFORE(\"abc\", \"\", -2, 0, 0, \"missing\")", CellValue::Text("abc".into())),
            ((0, 3, 1), "TEXTBEFORE(\"abc\", \"\", 2, 0, 0, \"missing\")", CellValue::Text("".into())),
            ((0, 3, 2), "TEXTAFTER(\"abc\", \"\", -2, 0, 0, \"missing\")", CellValue::Text("".into())),
            ((0, 3, 3), "TEXTAFTER(\"abc\", \"\", 2, 0, 0, \"missing\")", CellValue::Text("abc".into())),

            // 5. Fallback as string type
            ((0, 4, 0), "TEXTBEFORE(\"abc\", \"x\", 1, 0, 0, TRUE)", CellValue::Text("TRUE".into())),
            ((0, 4, 1), "TEXTBEFORE(\"abc\", \"x\", 1, 0, 0, 0)", CellValue::Text("0".into())),
            ((0, 4, 2), "TEXTAFTER(\"abc\", \"x\", 1, 0, 0, TRUE)", CellValue::Text("TRUE".into())),

            // 6. TEXTSPLIT edge cases
            ((0, 5, 0), "TEXTSPLIT(\"\", \",\")", CellValue::Error(CellError::Value)),
            ((0, 5, 1), "TEXTSPLIT(\"Hello World\", \"Hello World\", \"Hello World\", TRUE)", CellValue::Error(CellError::Value)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_char_and_code_functions() {
        let mut book = MockBook::new(false);

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 0, 0), "CHAR(255)", CellValue::Text("ÿ".into())),
            ((0, 0, 1), "CHAR(3)", CellValue::Text("\u{3}".into())),
            ((0, 0, 2), "CHAR(10)", CellValue::Text("\n".into())),
            ((0, 0, 3), "CHAR(0)", CellValue::Error(CellError::Value)),
            ((0, 0, 4), "CHAR(256)", CellValue::Error(CellError::Value)),
            ((0, 1, 0), "CODE(\"日本語テキスト\")", CellValue::Number(18044.0)),
            ((0, 1, 1), "CODE(\"abc\")", CellValue::Number(97.0)),
            ((0, 1, 2), "CODE(\"ｱｲｳ ABC\")", CellValue::Number(177.0)),
            ((0, 1, 3), "CODE(\"\")", CellValue::Error(CellError::Value)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_acot_acoth_and_type_functions() {
        let mut book = MockBook::new(false);
        book.values.insert((0, 0, 0), CellValue::Number(1.0)); // A1 = 1
        book.values.insert((0, 1, 0), CellValue::Number(2.0)); // A2 = 2

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 0, 1), "ACOT(10)", CellValue::Number(0.09966865249116204)),
            ((0, 0, 2), "ACOT(100)", CellValue::Number(0.009999666686665238)),
            ((0, 0, 3), "ACOT(1E15)", CellValue::Number(1e-15)),
            ((0, 0, 4), "ACOT(-1)", CellValue::Number(std::f64::consts::PI * 0.75)),
            ((0, 1, 1), "ACOTH(100)", CellValue::Number(0.010000333353334763)),
            ((0, 1, 2), "ACOTH(1E15)", CellValue::Number(1e-15)),
            ((0, 1, 3), "ACOTH(1)", CellValue::Error(CellError::Num)),
            ((0, 1, 4), "ACOTH(0.5)", CellValue::Error(CellError::Num)),
            // TYPE: array constant is 64
            ((0, 2, 0), "TYPE({1, 2; 3, 4})", CellValue::Number(64.0)),
            // TYPE: single cell reference is cell type (1 for number)
            ((0, 0, 5), "TYPE(A1)", CellValue::Number(1.0)),
            // TYPE: out-of-intersection range reference is #VALUE! -> type 16
            ((0, 10, 0), "TYPE(A1:A2)", CellValue::Number(16.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_spill_hash_and_anchorarray() {
        let mut book = MockBook::new(false);
        book.dynamic = true;
        book.formulas.insert((0, 0, 1), "SEQUENCE(4, 1, 10, 5)".to_string());
        book.formulas.insert((0, 0, 3), "SUM(B1#)".to_string());
        book.formulas.insert((0, 1, 3), "INDEX(B1#, 2)".to_string());
        book.formulas.insert((0, 0, 4), "SUM(ANCHORARRAY(B1))".to_string());
        book.formulas.insert((0, 1, 4), "INDEX(ANCHORARRAY(B1), 2)".to_string());

        let formula_cells = vec![(0, 0, 1), (0, 0, 3), (0, 1, 3), (0, 0, 4), (0, 1, 4)];
        let res = recalculate(&book, &formula_cells, &formula_cells);
        let val = |cell| res.iter().find(|r| r.cell == cell).map(|r| r.value.clone()).unwrap();
        assert_eq!(val((0, 0, 1)), CellValue::Number(10.0));
        assert_eq!(val((0, 0, 3)), CellValue::Number(70.0));
        assert_eq!(val((0, 1, 3)), CellValue::Number(15.0));
        assert_eq!(val((0, 0, 4)), CellValue::Number(70.0));
        assert_eq!(val((0, 1, 4)), CellValue::Number(15.0));
    }

    #[test]
    fn test_n_and_t_range_functions() {
        let mut book = MockBook::new(false);
        // Set up cells in sheet 0
        // A1: 10
        book.values.insert((0, 0, 0), CellValue::Number(10.0));
        // A2: "hello"
        book.values.insert((0, 1, 0), CellValue::Text("hello".to_string()));
        // A3: TRUE
        book.values.insert((0, 2, 0), CellValue::Bool(true));
        // A4: Empty (implicit)
        // A5: #DIV/0!
        book.values.insert((0, 4, 0), CellValue::Error(CellError::Div0));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // N function: evaluates top-left cell of range
            ((0, 10, 0), "N(A1:A5)", CellValue::Number(10.0)),
            ((0, 10, 1), "N(A2:A5)", CellValue::Number(0.0)),
            ((0, 10, 2), "N(A3)", CellValue::Number(1.0)),
            ((0, 10, 3), "N(A4)", CellValue::Number(0.0)),
            ((0, 10, 4), "N(A5)", CellValue::Error(CellError::Div0)),
            ((0, 10, 5), "N({42, 2; 3, 4})", CellValue::Number(42.0)),
            // T function: evaluates top-left cell of range
            ((0, 11, 0), "T(A1:A5)", CellValue::Text("".to_string())),
            ((0, 11, 1), "T(A2:A5)", CellValue::Text("hello".to_string())),
            ((0, 11, 2), "T(A3)", CellValue::Text("".to_string())),
            ((0, 11, 3), "T(A4)", CellValue::Text("".to_string())),
            ((0, 11, 4), "T(A5)", CellValue::Error(CellError::Div0)),
            ((0, 11, 5), "T({1, 2; 3, 4})", CellValue::Text("".to_string())),
            ((0, 11, 6), "T({\"world\", 2; 3, 4})", CellValue::Text("world".to_string())),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_factdouble_boolean_error() {
        let mut book = MockBook::new(false);
        book.values.insert((0, 0, 0), CellValue::Bool(true));
        book.values.insert((0, 1, 0), CellValue::Bool(false));
        book.values.insert((0, 2, 0), CellValue::Number(6.0));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 10, 0), "FACTDOUBLE(6)", CellValue::Number(48.0)),
            ((0, 10, 1), "FACTDOUBLE(7)", CellValue::Number(105.0)),
            ((0, 10, 2), "FACTDOUBLE(0)", CellValue::Number(1.0)),
            ((0, 10, 3), "FACTDOUBLE(-1)", CellValue::Number(1.0)),
            ((0, 10, 4), "FACTDOUBLE(-2)", CellValue::Error(CellError::Num)),
            ((0, 10, 5), "FACTDOUBLE(\"2\")", CellValue::Number(2.0)),
            ((0, 10, 6), "FACTDOUBLE(TRUE)", CellValue::Error(CellError::Value)),
            ((0, 10, 7), "FACTDOUBLE(FALSE)", CellValue::Error(CellError::Value)),
            ((0, 10, 8), "FACTDOUBLE(A1)", CellValue::Error(CellError::Value)),
            ((0, 10, 9), "FACTDOUBLE(A2)", CellValue::Error(CellError::Value)),
            ((0, 10, 10), "FACTDOUBLE(A3)", CellValue::Number(48.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_imlog2_precision() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 20, 0), "IMLOG2(\"12:30\")", CellValue::Text("-0.941106310946432".to_string())),
            ((0, 20, 1), "IMLOG2(123)", CellValue::Text("6.94251450533924".to_string())),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_dist_boolean_argument() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 30, 0), "EXPON.DIST(100, 0.5, \"TRUE\")", CellValue::Number(1.0)),
            ((0, 30, 1), "NORM.S.DIST(1000, \"TRUE\")", CellValue::Number(1.0)),
            ((0, 30, 2), "EXPON.DIST(0, 0.5, \"FALSE\")", CellValue::Number(0.5)),
            ((0, 30, 3), "EXPON.DIST(100, 0.5, \"x\")", CellValue::Error(CellError::Value)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_error_type_calc_filter() {
        let mut book = MockBook::new(false);
        book.dynamic = true;
        book.values.insert((0, 0, 0), CellValue::Number(1.0)); // A1
        book.values.insert((0, 1, 0), CellValue::Number(3.0)); // A2

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 44, 1), "ERROR.TYPE(_xlfn._xlws.FILTER(A1:A2, A1:A2>100))", CellValue::Number(14.0)),
            ((0, 44, 2), "ERROR.TYPE(1/0)", CellValue::Number(2.0)),
            ((0, 44, 3), "ERROR.TYPE(NA())", CellValue::Number(7.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_fisherinv_precision() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 0, 0), "FISHERINV(1E-15)", CellValue::Number(9.992007221626401e-16)),
            ((0, 0, 1), "FISHERINV(0)", CellValue::Number(0.0)),
            ((0, 0, 2), "FISHERINV(100)", CellValue::Number(1.0)),
            ((0, 0, 3), "FISHERINV(1E15)", CellValue::Number(1.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_gammaln_precision() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 0, 0), "GAMMALN(2.5)", CellValue::Number(0.2846828704729194)),
            ((0, 0, 1), "_xlfn.GAMMALN.PRECISE(2.5)", CellValue::Number(0.2846828704729194)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            if let (CellValue::Number(act), CellValue::Number(exp)) = (actual, expected) {
                assert!((act - exp).abs() < 1e-15, "formula `{formula}` precision mismatch: {act} vs {exp}");
            } else {
                panic!("expected number");
            }
        }
    }

    #[test]
    fn test_intercept_precision() {
        let mut book = MockBook::new(false);
        // D!A1:B5 and D!J1:J10 simulation
        // A1:A5 = 1, 2, 3, 4, 5
        // B1:B5 = -2.5, 0, 3.7, 12, -7
        // J1:J10 = 2.1, 3.9, 6.2, 7.8, 10.1, 12.2, 13.8, 16.1, 18.0, 20.3
        for i in 0..5 {
            book.values.insert((0, i, 0), CellValue::Number((i + 1) as f64)); // A1..A5
        }
        let b_vals = [-2.5, 0.0, 3.7, 12.0, -7.0, 100.0, 0.25, 8.0, 1e5, 42.0];
        for (i, &v) in b_vals.iter().enumerate() {
            book.values.insert((0, i as u32, 1), CellValue::Number(v)); // B1..B10
        }
        let j_vals = [2.1, 3.9, 6.2, 7.8, 10.1, 12.2, 13.8, 16.1, 18.0, 20.3];
        for (i, &v) in j_vals.iter().enumerate() {
            book.values.insert((0, i as u32, 9), CellValue::Number(v)); // J1..J10
        }

        let cases: &[((usize, u32, u32), &str, f64)] = &[
            ((0, 0, 20), "INTERCEPT(A1:B5, J1:J10)", 0.7248121627076085),
            ((0, 1, 20), "_xlfn.FORECAST.LINEAR(2.5, J1:J10, B1:B10)", 10.27591956519375),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            if let CellValue::Number(act) = actual {
                assert!((act - expected).abs() < 1e-15, "formula `{formula}` precision mismatch: {act} vs {expected}");
            } else {
                panic!("expected number for {formula}");
            }
        }
    }

    #[test]
    fn test_math_and_stat_excel_compliance() {
        let mut book = MockBook::new(false);
        // L1 is empty (default in MockBook)
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 50, 0), "ASINH(1E-15)", CellValue::Number(1.1102230246251559e-15)),
            ((0, 50, 1), "_xlfn.SECH(10)", CellValue::Number(9.0799859337817237e-5)),
            ((0, 50, 2), "_xlfn.BASE(0, 10, 0)", CellValue::Text(String::new())),
            ((0, 50, 3), "_xlfn.BASE(0, 2)", CellValue::Text("0".to_string())),
            ((0, 50, 4), "MOD(1E+15, 100)", CellValue::Error(CellError::Num)),
            ((0, 50, 5), "QUOTIENT(TRUE, 0.5)", CellValue::Error(CellError::Value)),
            ((0, 50, 6), "QUOTIENT(1000, FALSE)", CellValue::Error(CellError::Value)),
            ((0, 50, 7), "QUOTIENT(10, 3)", CellValue::Number(3.0)),
            ((0, 50, 8), "AVEDEV(L1)", CellValue::Error(CellError::Num)),
            ((0, 50, 9), "POWER(100, -2.5)", CellValue::Number(9.99999999999998e-6)),
            ((0, 50, 10), "POWER(0.05, 100)", CellValue::Number(7.888609052210203e-131)),
            ((0, 50, 11), "POWER(0.05, 2.5)", CellValue::Number(0.0005590169943749478)),
            ((0, 50, 12), "POWER({1,2,3}, -2.5)", CellValue::Number(1.0)),
            ((0, 50, 13), "SQRTPI(TRUE)", CellValue::Error(CellError::Value)),
            ((0, 50, 14), "MROUND(TRUE, 5)", CellValue::Error(CellError::Value)),
            ((0, 50, 15), "GCD(TRUE)", CellValue::Error(CellError::Value)),
            ((0, 50, 16), "GCD(L1)", CellValue::Error(CellError::Value)),
            ((0, 50, 17), "LCM(TRUE)", CellValue::Error(CellError::Value)),
            ((0, 50, 18), "LCM(L1)", CellValue::Error(CellError::Value)),
            ((0, 50, 19), "_xlfn.PERCENTOF(\"abc\", 10)", CellValue::Error(CellError::Value)),
            ((0, 50, 20), "MULTINOMIAL(2, 2, 2)", CellValue::Number(89.999999999999915)),
            ((0, 50, 21), "MULTINOMIAL(TRUE)", CellValue::Error(CellError::Value)),
            ((0, 50, 22), "MULTINOMIAL(255)", CellValue::Error(CellError::Num)),
            ((0, 50, 23), "DATEDIF(DATE(2020,1,31),DATE(2021,3,1),\"MD\")", CellValue::Number(-2.0)),
            ((0, 50, 24), "DATEVALUE(\"令和6年1月15日\")", CellValue::Number(45306.0)),
            ((0, 50, 25), "DATEVALUE(\"R6.1.15\")", CellValue::Number(45306.0)),
            ((0, 50, 26), "DATEVALUE(\"H31.4.30\")", CellValue::Number(43585.0)),
            ((0, 50, 27), "DATEVALUE(\"Jan 15, 2024\")", CellValue::Number(45306.0)),
            ((0, 50, 28), "DATEVALUE(\"1900/2/29\")", CellValue::Number(60.0)),
            ((0, 50, 29), "DATEVALUE(\"明治45年7月30日\")", CellValue::Number(4595.0)),
            ((0, 50, 30), "DATEVALUE(\"大正元年7月30日\")", CellValue::Error(CellError::Value)),
            ((0, 50, 31), "SUBTOTAL(9, Sheet1:Sheet2!A1:A3)", CellValue::Error(CellError::Value)),
            ((0, 50, 32), "_xlfn.AGGREGATE(14, 6, A1:A3*2, 1)", CellValue::Number(60.0)),
            ((0, 50, 33), "_xlfn.AGGREGATE(15, 6, A1:A3*2, 1)", CellValue::Number(20.0)),
            ((0, 50, 34), "DEVSQ(L1)", CellValue::Error(CellError::Num)),
            ((0, 50, 35), "HARMEAN(L1)", CellValue::Error(CellError::NA)),
            ((0, 50, 36), "SMALL({10, 20, 30}, 2.9)", CellValue::Number(20.0)),
            ((0, 50, 37), "_xlfn.MAXIFS(A1:A3, C1:C3, \"=\")", CellValue::Number(0.0)),
            ((0, 50, 38), "_xlfn.CHISQ.DIST.RT(1E-15, 2)", CellValue::Number(0.99999999999999956)),
            ((0, 50, 39), "SUBSTITUTE(\"123\",\"12:30\",\"A1\",TRUE)", CellValue::Error(CellError::Value)),
            ((0, 50, 40), "DBCS(\"ｶﾞﾊﾟｳﾞ\")", CellValue::Text("ガパウ゛".to_string())),
            ((0, 50, 41), "LARGE({10, 20, 30}, 2.9)", CellValue::Number(10.0)),
            ((0, 50, 42), "_xlfn.SWITCH({1,2;3,4}, 1, \"one\", 2, \"two\", \"default\")", CellValue::Text("default".to_string())),
            ((0, 50, 43), "COUPDAYSNC(DATE(2021,1,1), DATE(2021,2,28), 2, 4)", CellValue::Number(57.0)),
            ((0, 50, 44), "COUPDAYBS(0, DATE(2021,2,28), 2, 0)", CellValue::Number(0.0)),
            ((0, 50, 45), "_xlfn.TRIMRANGE(L1:L10)", CellValue::Error(CellError::Ref)),
            ((0, 50, 46), "LOOKUP(1, \"abc\")", CellValue::Error(CellError::NA)),
            ((0, 50, 47), "MATCH(2, {1, 3, 2}, 1)", CellValue::Number(1.0)),
            ((0, 50, 48), "_xlfn.ARRAYTOTEXT(_xlfn.DROP(F1:F3, 2, 0), 1)", CellValue::Error(CellError::Value)),
            ((0, 50, 49), "_xlfn.ARRAYTOTEXT(_xlfn.ANCHORARRAY(F1), 1)", CellValue::Error(CellError::NA)),
            ((0, 50, 50), "_xlfn.ARRAYTOTEXT(_xlfn.EXPAND(A1:A3, 3, 2, L1), 1)", CellValue::Text("{10,;20,;30,}".to_string())),
            ((0, 50, 51), "ISPMT(10, 3, 2.9, 0.25)", CellValue::Number(0.08620689655172421)),
            ((0, 50, 52), "NOMINAL(123456789, 5)", CellValue::Number(202.62182322772847)),
            ((0, 50, 53), "PV(3.7, 2, 10, 0.5, 0)", CellValue::Number(-2.602987777274785)),
            ((0, 50, 54), "_xlfn.ARRAYTOTEXT(_xlfn.SCAN(A1:A3, A1:A3, LAMBDA(a, v, a+v)), 1)", CellValue::Text("{#VALUE!;#VALUE!;#VALUE!}".to_string())),
            ((0, 50, 55), "LEFTB(\"日本語テキスト\", 3)", CellValue::Text("日 ".to_string())),
            ((0, 50, 56), "RIGHTB(\"日本語テキスト\", 10)", CellValue::Text("語テキスト".to_string())),
            ((0, 50, 57), "MIDB(\"日本語テキスト\", 2, 3)", CellValue::Text(" 本".to_string())),
            ((0, 50, 58), "REPLACEB(\"日本語テキスト\", 10, 5, \"Hello World\")", CellValue::Text("日本語テ Hello World".to_string())),
            ((0, 50, 59), "LOGINV(1E-15, 3.7, 0.5)", CellValue::Number(0.76286616979737698)),
            ((0, 50, 60), "DISC(-1, 47119, 2, 3.7)", CellValue::Error(CellError::Num)),
            ((0, 50, 61), "INTRATE(-1, 47119, 10, 100)", CellValue::Error(CellError::Num)),
            ((0, 50, 62), "PRICEDISC(-1, 44197, 3.7, 10)", CellValue::Error(CellError::Num)),
            ((0, 50, 63), "YIELDDISC(-1, 47119, 100, 0.25)", CellValue::Error(CellError::Num)),
            ((0, 50, 64), "AMORLINC(1, -1, 46000, 1, 1, 0.25)", CellValue::Error(CellError::Num)),
            ((0, 50, 65), "ACCRINTM(45000, 45000.75, 0.25, 10)", CellValue::Number(0.0)),
            ((0, 50, 66), "ACCRINTM(-1, 46000, 0.25, 10)", CellValue::Error(CellError::Num)),
            ((0, 50, 67), "AND(\"x\", TRUE, TRUE)", CellValue::Bool(true)),
            ((0, 50, 68), "OR(\"x\", TRUE, TRUE)", CellValue::Bool(true)),
            ((0, 50, 69), "XOR(\"x\", TRUE, TRUE)", CellValue::Bool(false)),
            ((0, 50, 70), "AND(\"x\")", CellValue::Error(CellError::Value)),
            ((0, 50, 71), "TEXT(1, \"Banana\")", CellValue::Error(CellError::Value)),
            ((0, 50, 72), "TEXT(1, \"ABC-def_ghi\")", CellValue::Text("A43C-133f 0i".to_string())),
            ((0, 50, 73), "EXPONDIST(1E-15, 0.25, TRUE)", CellValue::Number(2.4999999999999997e-16)),
            ((0, 50, 74), "BINOMDIST(0, 1, 1, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 50, 75), "BINOMDIST(1, 3, 1, FALSE)", CellValue::Number(0.0)),
            ((0, 50, 76), "GAMMA(0.5)", CellValue::Number(std::f64::consts::PI.sqrt())),
            ((0, 50, 77), "MAKEARRAY(2, 2, LAMBDA(r, c, {1, 2}))", CellValue::Error(CellError::Value)),
            ((0, 50, 78), "VAR(\"5\", 3.7, 3.7)", CellValue::Number(0.56333333333333613)),
            ((0, 50, 79), "VAR(3.7, 3.7, 3.7)", CellValue::Number(2.9582283945787943e-31)),
            ((0, 50, 80), "_xlfn.VAR.P(2.5, 3.7, 3.7)", CellValue::Number(0.32000000000000106)),
            ((0, 50, 81), "VARP(\"5\", 3.7, 3.7)", CellValue::Number(0.3755555555555582)),
            ((0, 50, 82), "_xlfn.STDEV.P(2.5, 3.7, 3.7)", CellValue::Number(0.56568542494923901)),
            ((0, 50, 83), "STDEVP(3.7, 2.5, 3.7)", CellValue::Number(0.56568542494923901)),
            ((0, 50, 84), "_xlfn.NORM.DIST(-1, 10, 5, FALSE)", CellValue::Number(7.0949185692462842e-3)),
            ((0, 50, 85), "COUPNUM(0, 47119, 4)", CellValue::Error(CellError::Num)),
            ((0, 50, 86), "COUPNUM(1, 47119, 4)", CellValue::Number(516.0)),
            ((0, 50, 87), "COUPPCD(1, 45000, 2)", CellValue::Number(0.0)),
            ((0, 50, 88), "TBILLEQ(39538, 39600, 10)", CellValue::Error(CellError::Num)),
            ((0, 50, 89), "XNPV(0, A1:A3, A1:A3)", CellValue::Error(CellError::Num)),
            ((0, 50, 90), "XNPV(0.05, \"abc\", A1:A3)", CellValue::Error(CellError::Value)),
            ((0, 50, 91), "PMT(1E-15, 5, 12, -2.5, 0)", CellValue::Number(-1.9000000000000081)),
            ((0, 50, 92), "PMT(0.05, \"2\", 1)", CellValue::Number(-0.53780487804878041)),
            ((0, 50, 93), "FV(-2.5, 0, 10, 0.25, 0)", CellValue::Number(-0.25)),
            ((0, 50, 94), "FV(2, 2.9, 0.05)", CellValue::Number(-0.57977196039251466)),
            ((0, 50, 95), "IPMT(-1, 1, 1, 10, 0.25, 1)", CellValue::Number(0.0)),
            ((0, 50, 96), "_xlfn.NUMBERVALUE(\"2024/1/31\", \"a,b,c\", \"abc\")", CellValue::Error(CellError::Value)),
            ((0, 50, 97), "_xlfn.NUMBERVALUE(\"2024/1/31\", \"a,b,c\", \"\")", CellValue::Error(CellError::Value)),
            ((0, 50, 98), "_xlfn.NUMBERVALUE(\"2024/1/31\", \"a,b,c\", \"123\")", CellValue::Error(CellError::Value)),
            ((0, 50, 99), "GEOMEAN(1E+15)", CellValue::Number(1e15)),
            ((0, 50, 100), "GEOMEAN(2.5, 3.7, 3.7)", CellValue::Number(3.2467422977985749)),
            ((0, 50, 101), "_xlfn.XMATCH(\"a.c\", {\"abc\", \"a.c\"}, 3, 1)", CellValue::Number(1.0)),
            ((0, 50, 102), "_xlfn.XMATCH(\"a.c\", {\"abc\", \"a.c\"}, 3, -1)", CellValue::Number(2.0)),
            ((0, 50, 103), "_xlfn.XMATCH(35, {10, 20, 30, 40, 50, 60, 70, 80, 90, 100}, -1, -2)", CellValue::Error(CellError::NA)),
            ((0, 50, 104), "IPMT(3.7, 2, 2, 3.7, 3.7)", CellValue::Number(-8.8864912280701773)),
            ((0, 50, 105), "EFFECT(1, 255)", CellValue::Number(1.712970952642269)),
            ((0, 50, 106), "EFFECT(1, 1000)", CellValue::Number(1.7169239322355203)),
            ((0, 50, 107), "DB(123456789, 3.7, 10, 5)", CellValue::Number(99725.880122494535)),
            ((0, 50, 108), "DB(10, 1E-15, 10, 5)", CellValue::Number(3.8085937500000032e-6)),
            ((0, 50, 109), "INFO(\"origin\")", CellValue::Text("$A:$A$1".into())),
            ((0, 50, 110), "CONFIDENCE(0.25, 100, 3)", CellValue::Number(66.415452442220769)),
            ((0, 50, 111), "_xlfn.CONFIDENCE.NORM(0.25, 100, 2)", CellValue::Number(81.341984759761857)),
            ((0, 50, 112), "IMPOWER(\"123\", -2.5)", CellValue::Text("5.95987596448829E-06".into())),
            ((0, 50, 113), "IMPOWER(123, -2.5)", CellValue::Text("5.95987596448829E-06".into())),
            ((0, 50, 114), "IMPOWER(\"2024/1/31\", -1)", CellValue::Text("0.0000220643396143153".into())),
            ((0, 50, 115), "IMPOWER(\"2024/1/31\", 2.5)", CellValue::Text("437293141712.532".into())),
            ((0, 50, 116), "SERIESSUM(5, -2.5, 0.5, {2, 1, 0; 1, 3, 1; 0, 1, 4})", CellValue::Number(52.33877742413565)),
            ((0, 50, 117), "NORMSDIST(-2.5)", CellValue::Number(0.006209665325776135)),
            ((0, 50, 118), "ERF(-1, 100)", CellValue::Number(1.842700792949715)),
            ((0, 50, 119), "ERF(0.05, -1)", CellValue::Number(-0.8990727707467315)),
            ((0, 50, 120), "_xlfn.T.INV(0.05, 1)", CellValue::Number(-6.3137515146750438)),
            ((0, 50, 121), "_xlfn.T.INV.2T(0.5, 2)", CellValue::Number(0.816496580927726)),
            ((0, 50, 122), "_xlfn.T.INV.2T(1E-15, 2)", CellValue::Number(31622776.601683773)),
            ((0, 50, 123), "CHIINV(0.25, 1)", CellValue::Number(1.3233036969314653)),
            ((0, 50, 124), "CHIDIST(2.5, 3)", CellValue::Number(0.47529108334302028)),
            ((0, 50, 125), "_xlfn.CHISQ.INV(0.25, 255)", CellValue::Number(239.42646554141962)),
            ((0, 50, 126), "TINV(0, 1)", CellValue::Error(CellError::Div0)),
            ((0, 50, 127), "TINV(0.25, 255)", CellValue::Number(1.1529756638999835)),
            ((0, 50, 128), "NORMDIST(0.25, 100, 10, TRUE)", CellValue::Number(9.8050726443977616e-24)),
            ((0, 50, 129), "LOGNORMDIST(1E-15, 0.25, 3.7)", CellValue::Number(2.6673315017894328e-21)),
            ((0, 50, 130), "_xlfn.BINOM.DIST(2, 255, 0.25, TRUE)", CellValue::Number(5.0930868280557947e-29)),
            ((0, 50, 131), "_xlfn.CHISQ.DIST(3.7, 2, TRUE)", CellValue::Number(0.84276283368637239)),
            ((0, 50, 132), "NEGBINOMDIST(0, 1, 0)", CellValue::Error(CellError::Num)),
            ((0, 50, 133), "NEGBINOMDIST(0, 1, 1)", CellValue::Error(CellError::Num)),
            ((0, 50, 134), "_xlfn.NEGBINOM.DIST(3, 2, 0.05, FALSE)", CellValue::Number(8.5737499999999998e-3)),
            ((0, 50, 135), "POISSON(5, 2.5, FALSE)", CellValue::Number(6.6800942890542642e-2)),
            ((0, 50, 136), "_xlfn.POISSON.DIST(3, 100, TRUE)", CellValue::Number(6.3898877022381386e-39)),
            ((0, 50, 137), "NPV(3.7, 3.7, 3.7, 3.7)", CellValue::Number(0.99036822284079651)),
            ((0, 50, 138), "MIRR(\"abc\", 3.7, 5)", CellValue::Error(CellError::Div0)),
            ((0, 50, 139), "NPER(1E-15, 12, 5, 2, 1)", CellValue::Number(-0.50000000000000044)),
            ((0, 50, 140), "SKEW(5, 3.7, 3.7)", CellValue::Number(1.7320508075688847)),
            ((0, 50, 141), "KURT(3.7, 3.7, 1, 2, 3)", CellValue::Number(-1.0732818569596123)),
            ((0, 50, 142), "_xlfn.SKEW.P(5, 3.7, 3.7)", CellValue::Number(0.70710678118655046)),
            ((0, 50, 143), "PERCENTRANK(C1:C3, 0.25)", CellValue::Error(CellError::NA)),
            ((0, 50, 144), "_xlfn.PERCENTRANK.EXC(C1:C3, 1)", CellValue::Error(CellError::NA)),
            ((0, 50, 145), "_xlfn.PERCENTRANK.INC(A1:A3, 20, 1000)", CellValue::Error(CellError::Div0)),
            ((0, 50, 146), "_xlfn.BINOM.DIST.RANGE(255, 0.5, 2)", CellValue::Number(5.593646373134651e-73)),
            ((0, 50, 147), "_xlfn.CONFIDENCE.T(0.25, 10, 3)", CellValue::Number(9.2582009977255151)),
            ((0, 50, 148), "_xlfn.GAMMA.DIST(0.05, 0.05, 5, FALSE)", CellValue::Number(0.80782854739359444)),
            ((0, 50, 149), "MODE(\"abc\")", CellValue::Error(CellError::NA)),
            ((0, 50, 150), "MODE(L1)", CellValue::Error(CellError::Value)),
            ((0, 50, 151), "MODE(\"abc\", 3.7, 3.7)", CellValue::Number(3.7)),
            ((0, 50, 152), "_xlfn.MODE.SNGL(L1)", CellValue::Error(CellError::Value)),
            ((0, 50, 153), "_xlfn.CHISQ.TEST(C1:C3, C1:C3)", CellValue::Error(CellError::Div0)),
            ((0, 50, 154), "_xlfn.F.TEST(C1:C3, A1:A3)", CellValue::Error(CellError::Div0)),
            ((0, 50, 155), "_xlfn.T.DIST(0.5, 0, FALSE)", CellValue::Error(CellError::Div0)),
            ((0, 50, 156), "_xlfn.T.DIST(0.5, -1, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 50, 157), "_xlfn.T.DIST.RT(-1, 3)", CellValue::Number(0.80449889052211465)),
            ((0, 50, 158), "_xlfn.T.DIST.2T(12, 2)", CellValue::Number(0.006872933677158459)),
            ((0, 50, 159), "CRITBINOM(10, 0, 0.05)", CellValue::Error(CellError::Num)),
            ((0, 50, 160), "CRITBINOM(10, 0.05, 0)", CellValue::Error(CellError::Num)),
            ((0, 50, 161), "CRITBINOM(10, 0.5, 0.05)", CellValue::Number(2.0)),
            ((0, 50, 162), "_xlfn.BINOM.INV(1, 0, 0.5)", CellValue::Error(CellError::Num)),
            ((0, 50, 163), "_xlfn.BINOM.INV(1, 0.5, 0.5)", CellValue::Number(1.0)),
            ((0, 50, 164), "HYPGEOMDIST(0, 1, 3, 3)", CellValue::Number(0.0)),
            ((0, 50, 165), "HYPGEOMDIST(1, 1, 3, 3)", CellValue::Number(1.0)),
            ((0, 50, 166), "HYPGEOMDIST(1, 1, 3, 255)", CellValue::Number(0.011764705882352941)),
            ((0, 50, 167), "_xlfn.HYPGEOM.DIST(0, 2, 0, 10, TRUE)", CellValue::Number(1.0)),
            ((0, 50, 168), "_xlfn.HYPGEOM.DIST(-1, 2, 0, 10, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 50, 169), "WEIBULL(0, 2, 100, FALSE)", CellValue::Number(0.0)),
            ((0, 50, 170), "WEIBULL(3.7, 1E+15, 100, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 50, 171), "_xlfn.WEIBULL.DIST(1000, 1E+15, 1000, TRUE)", CellValue::Number(0.63212055882855767)),
            ((0, 50, 172), "ZTEST(C1:C3, 0)", CellValue::Error(CellError::NA)),
            ((0, 50, 173), "ZTEST(A1, 0)", CellValue::Error(CellError::Div0)),
            ((0, 50, 174), "ZTEST(A1:A3, 0, -1)", CellValue::Error(CellError::Num)),
            ((0, 50, 175), "BETADIST(0.05, 1, 0.05)", CellValue::Number(0.0025613787765302811)),
            ((0, 50, 176), "BETAINV(0, 1, 5)", CellValue::Error(CellError::Num)),
            ((0, 50, 177), "BETAINV(0.25, 1, 123456789)", CellValue::Number(2.3302248053492527e-9)),
            ((0, 50, 178), "_xlfn.BETA.DIST(1000, 100, 1, FALSE, 0.05, 1000)", CellValue::Number(0.0)),
            ((0, 50, 179), "_xlfn.BETA.INV(0, 12, 12)", CellValue::Error(CellError::Num)),
            ((0, 50, 180), "_xlfn.BETA.INV(0.25, 1, 12)", CellValue::Number(0.0236884242226068)),
            ((0, 50, 181), "_xlfn.F.DIST(1, 5, 10, FALSE)", CellValue::Number(0.49547978348663885)),
            ((0, 50, 182), "_xlfn.F.INV(1E-15, 1, 5)", CellValue::Number(1.7348913986289857e-30)),
            ((0, 50, 183), "TTEST(A1:A3, A1:A3, 1, 1)", CellValue::Error(CellError::Div0)),
            ((0, 50, 184), "_xlfn.T.TEST(A1:A3, A1:A3, 1, 1)", CellValue::Error(CellError::Div0)),
            ((0, 50, 185), "_xlfn.LOGNORM.DIST(5, 0.05, 0.05, FALSE)", CellValue::Number(9.4505694153986851e-212)),
            ((0, 50, 186), "_xlfn.LOGNORM.INV(0.05, 5, 10)", CellValue::Number(1.0665073978308865e-5)),
            ((0, 50, 187), "_xlfn.ARRAYTOTEXT(TREND(5), 1)", CellValue::Text("{5}".into())),
            ((0, 50, 188), "_xlfn.ARRAYTOTEXT(GROWTH(5), 1)", CellValue::Text("{5}".into())),
            ((0, 50, 189), "_xlfn.ARRAYTOTEXT(LINEST(5), 1)", CellValue::Text("{0,5}".into())),
            ((0, 50, 190), "_xlfn.ARRAYTOTEXT(LOGEST(5), 1)", CellValue::Text("{1,5}".into())),
            ((0, 50, 191), "TREND(A1:A3, \"abc\")", CellValue::Error(CellError::Ref)),
            ((0, 50, 192), "TREND(\"abc\", A1:A3)", CellValue::Error(CellError::Value)),
            ((0, 50, 193), "CUMIPMT(5, 10, 2, 2, 10, 0)", CellValue::Number(-88.00000165381719)),
            ((0, 50, 194), "CUMIPMT(5, 1000, 2, 2, 10, 0)", CellValue::Number(-90.0)),
            ((0, 50, 195), "CUMIPMT(5, 10, 2, 2, 10, 1)", CellValue::Number(-14.666666942302866)),
            ((0, 50, 196), "CUMPRINC(5, 10, 2, 2, 10, 0)", CellValue::Number(-1.9999998346182803)),
            ((0, 50, 197), "CUMIPMT(1E-15, 10, 2, 2, 10, 0)", CellValue::Number(-8.881784197001252e-15)),
            ((0, 50, 198), "PPMT(2.5, 5, 5, 2)", CellValue::Number(-1.4312965722801787)),
            ((0, 50, 199), "PPMT(1E+15, 5, 5, 2)", CellValue::Number(-2.0000000000000027)),
            ((0, 50, 200), "PPMT(123456789, 5, 5, 2)", CellValue::Number(-1.9999999838000018)),
            ((0, 50, 201), "PPMT(1, 5, 5, 1, 0.05, 1)", CellValue::Number(-0.2709677419354839)),
            ((0, 50, 202), "PPMT(0.5, 5, 5, 1, 1, 1)", CellValue::Number(-0.51184834123222744)),
            ((0, 50, 203), "PPMT(0.5, 5, 5, 1, -1, 1)", CellValue::Number(0.0)),
            ((0, 50, 204), "PPMT(1E+15, 5, 5, 1, 0.05, 1)", CellValue::Number(-1.0500000000000025e-15)),
            ((0, 50, 205), "ACCRINT(43831, 43831, 46000, 0.05, 0.05, 4, 0, TRUE)", CellValue::Number(1.4847222222222225e-2)),
            ((0, 50, 206), "ACCRINT(43831, 43831, 46000, 0.05, 0.05, 4, 0, FALSE)", CellValue::Number(1.4847222222222225e-2)),
            ((0, 50, 207), "ACCRINT(43831, 43831, 46000, 0.05, 0.05, 4, 0, \"TRUE\")", CellValue::Error(CellError::Value)),
            ((0, 50, 208), "ACCRINT(0, 44197, 45292, 100, 12, 4)", CellValue::Error(CellError::Num)),
            ((0, 50, 209), "PRICE(43831, 44197, 5, 5, 2, 2, 3)", CellValue::Number(92.0)),
            ((0, 50, 210), "PRICE(43831, 44197, 5, 5, 2, 2, 2)", CellValue::Number(92.0)),
            ((0, 50, 211), "PRICE(0, 44197, 5, 5, 2, 2, 3)", CellValue::Error(CellError::Num)),
            ((0, 50, 212), "PRICEMAT(1, 39551, 39397, 0.061, 0.061)", CellValue::Error(CellError::Num)),
            ((0, 50, 213), "YIELDMAT(39522, 39755, 45000.75, 0.0625, 100.0123)", CellValue::Error(CellError::Num)),
            ((0, 50, 214), "VDB(12, 0.5, 10, 0.25, 5)", CellValue::Number(7.430976000000001)),
            ((0, 50, 215), "VDB(12, 0.5, 10, 0.25, 1)", CellValue::Number(1.7100000000000002)),
            ((0, 50, 216), "VDB(12, 100, 10, 0.25, 5)", CellValue::Number(-88.0)),
            ((0, 50, 217), "VDB(-1, 0.5, 10, 0.25, 5)", CellValue::Error(CellError::Num)),
            ((0, 50, 218), "FISHER(0.5)", CellValue::Number(0.54930614433405489)),
            ((0, 50, 219), "FISHER(1)", CellValue::Error(CellError::Num)),
            ((0, 50, 220), "FISHER(-1)", CellValue::Error(CellError::Num)),
            ((0, 50, 221), "_xlfn.ARRAYTOTEXT(_xlfn.GROUPBY({1;2;1}, {10;20;30}, _xleta.SUM), 1)", CellValue::Text("{1,40;2,20;\"合計\",60}".into())),
            ((0, 50, 222), "_xlfn.ARRAYTOTEXT(_xlfn.GROUPBY({1;2;1}, {10;20;30}, _xleta.SUM, 0, 0, -1), 1)", CellValue::Text("{2,20;1,40}".into())),
            ((0, 50, 223), "_xlfn.ARRAYTOTEXT(_xlfn.PIVOTBY({1;2;1}, {5;5;6}, {10;20;30}, _xleta.SUM), 1)", CellValue::Text("{\"\",5,6,\"合計\";1,10,30,40;2,20,\"\",20;\"合計\",30,30,60}".into())),
            ((0, 50, 224), "_xlfn.ARRAYTOTEXT(_xlfn.BYROW({1,2;3,4}, _xleta.SUM), 1)", CellValue::Text("{3;7}".into())),
            ((0, 50, 226), "_xlfn.ARRAYTOTEXT(_xlfn.GROUPBY({\"大阪\",\"み\";\"大阪\",\"り\";\"東京\",\"り\"}, {1;2;3}, _xleta.SUM, 0, 2), 1)", CellValue::Text("{\"大阪\",\"み\",1;\"大阪\",\"り\",2;\"大阪\",\"\",3;\"東京\",\"り\",3;\"東京\",\"\",3;\"総計\",\"\",6}".into())),
            ((0, 50, 227), "_xlfn.ARRAYTOTEXT(_xlfn.XLOOKUP(_xlfn.SEQUENCE(16, 1, 0), _xlfn.SEQUENCE(8), _xlfn.SEQUENCE(8) * 10, \"none\"), 1)", CellValue::Text("{\"none\";10;20;30;40;50;60;70;80;\"none\";\"none\";\"none\";\"none\";\"none\";\"none\";\"none\"}".into())),
            ((0, 50, 228), "_xlfn.ARRAYTOTEXT(_xlfn.XLOOKUP(_xlfn.SEQUENCE(16, 1, 0) * 0 + 1, _xlfn.SEQUENCE(8), _xlfn.SEQUENCE(8) * 10, 0), 1)", CellValue::Text("{10;10;10;10;10;10;10;10;10;10;10;10;10;10;10;10}".into())),
            ((0, 50, 229), "AMORDEGRC(2400, 1, 39813, 300, 1, 0.15)", CellValue::Number(113.0)),
            ((0, 50, 230), "AMORDEGRC(2400, 39679, 39813, 300, 1, 0.5)", CellValue::Error(CellError::Num)),
            ((0, 50, 231), "AMORDEGRC(10, 43831, 45000, \"5\", 2, 0.05, 4)", CellValue::Number(1.0)),
            ((0, 50, 232), "VDB(1, 2, 5, 3.7, 5, 5, FALSE)", CellValue::Number(0.0)),
            ((0, 50, 233), "_xlfn.ARRAYTOTEXT(ROUND(LINEST({1;3;2;5;4}, {1;2;3;4;5}, TRUE, TRUE), 4), 1)", CellValue::Text("{0.8,0.6;0.3464,1.1489;0.64,1.0954;5.3333,3;6.4,3.6}".into())),
            ((0, 50, 234), "_xlfn.ARRAYTOTEXT(ROUND(LINEST({1;3;2;5;4}, {1;2;3;4;5}, FALSE, TRUE), 4), 1)", CellValue::Text("{0.9636,0;0.1336,#N/A;0.9286,0.9909;52.0185,4;51.0727,3.9273}".into())),
            ((0, 50, 235), "BESSELI(1, 0)", CellValue::Number(1.2660658480342601)),
            ((0, 50, 236), "BESSELK(1, 3)", CellValue::Number(7.1012627693358237)),
            ((0, 50, 237), "BESSELI(10, 10)", CellValue::Number(21.8917064462442)),
            ((0, 50, 238), "ROUND(_xlfn.FORECAST.ETS(46249, {2;4;6;8;10;12}, {46037;46068;46096;46127;46157;46188}), 1)", CellValue::Number(16.0)),
            ((0, 50, 239), "_xlfn.FORECAST.ETS.SEASONALITY({2;4;6;8;10;12}, {46037;46068;46096;46127;46157;46188})", CellValue::Number(0.0)),
            ((0, 50, 240), "ROUND(_xlfn.FORECAST.ETS(13, {2.5;5.5;6.5;9.5;10.5;13.5;14.5;17.5;18.5;21.5}, {1;2;3;4;5;6;7;8;9;10}), 1)", CellValue::Number(26.5)),
            ((0, 50, 250), "CELL(\"filename\", A1)", CellValue::Text(String::new())),
            ((0, 50, 251), "CELL(\"type\", C1)", CellValue::Text("l".into())),
            ((0, 50, 252), "CELL(\"type\", L1)", CellValue::Text("b".into())),
            ((0, 50, 253), "CELL(\"type\", A1)", CellValue::Text("v".into())),
            ((0, 50, 254), "CELL(\"type\", A51)", CellValue::Text("v".into())),
        ];

        book.sheets.push("Sheet2".to_string());
        book.values.insert((0, 0, 0), CellValue::Number(10.0));
        book.values.insert((0, 1, 0), CellValue::Number(20.0));
        book.values.insert((0, 2, 0), CellValue::Number(30.0));
        book.values.insert((0, 0, 2), CellValue::Text("apple".to_string()));
        book.values.insert((0, 1, 2), CellValue::Text("".to_string()));
        book.values.insert((0, 2, 2), CellValue::Text("banana".to_string()));
        book.values.insert((0, 0, 5), CellValue::Error(CellError::NA));
        book.values.insert((0, 1, 5), CellValue::Error(CellError::Div0));
        book.values.insert((0, 2, 5), CellValue::Error(CellError::Value));

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            // A number may differ by what is below Excel's 15 shown digits: exp, ln and pow come from the platform's libm
            // (CHISQ.DIST.RT(1E-15, 2) is the half-way case between two doubles, which Windows' libm rounds the other way, and
            // EFFECT(1, 1000) = (1 + 1/1000)^1000 comes out 4e-14 of itself apart (the rounding of 1 + 1/1000 is raised to the 1000th power)).
            if let (CellValue::Number(a), CellValue::Number(e)) = (actual, expected) {
                assert!((a - e).abs() <= 1e-13 * e.abs().max(1e-300) || (a - e).abs() <= 4e-16, "formula `{formula}` evaluated incorrectly: {a:?} vs {e:?}");
            } else {
                assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
            }
        }
    }

    #[test]
    fn test_excel_15_digits_and_zero_subtraction() {
        let mut book = MockBook::new(false);
        book.sheets.push("Sheet1".to_string());

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // Comparison operands rounded to 15 digits
            ((0, 0, 0), "0.1+0.2=0.3", CellValue::Bool(true)),
            ((0, 0, 1), "1-0.9=0.1", CellValue::Bool(true)),
            // Root subtraction/addition near-zero compensation
            ((0, 0, 2), "0.1+0.2-0.3", CellValue::Number(0.0)),
            ((0, 0, 3), "1-0.9-0.1", CellValue::Number(0.0)),
            ((0, 0, 4), "(0.1+0.2)-0.3", CellValue::Number(0.0)),
            ((0, 0, 5), "0.3-(0.1+0.2)", CellValue::Number(0.0)),
            // Non-root subtraction preserved as IEEE-754: 5.55e-17 != 0
            ((0, 0, 6), "(0.1+0.2)-0.3=0", CellValue::Bool(false)),
            // Lexer 15-digit precision capping for literals
            ((0, 0, 7), "12345678901234567", CellValue::Number(12345678901234500.0)),
            ((0, 0, 8), "123456789012345678+1", CellValue::Number(123456789012345000.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_accrint_compliance() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 60, 0), "ACCRINT(43831,60,45292,100,12,4)", CellValue::Number(4800.0)),
            ((0, 60, 1), "ACCRINT(43831,44197,2958465,100,12,4)", CellValue::Error(CellError::Num)),
            ((0, 60, 2), "ACCRINT(43831,45351,45292,100,12,4)", CellValue::Number(4796.666666666667)),
            ((0, 60, 3), "ACCRINT(43831,43831,46000,0.05,0.05,4,2,TRUE)", CellValue::Number(0.014854166666666668)),
            ((0, 60, 4), "ACCRINT(43831,43831,46000,0.05,0.05,4,3,TRUE)", CellValue::Number(0.01484760273972603)),
            ((0, 60, 5), "ACCRINT(60,44197,45292,100,12,4)", CellValue::Number(148606.66666666666)),
            ((0, 60, 6), "INFO(\"release\")", CellValue::Text("16.0".to_string())),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() <= 1e-9 * (1.0 + e.abs()), "formula `{formula}` evaluated to {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_bessel_compliance() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // BESSELJ
            ((0, 70, 0), "BESSELJ(0.5,1)", CellValue::Number(0.24226845767957006)),
            ((0, 70, 1), "BESSELJ(0,1)", CellValue::Number(0.0)),
            ((0, 70, 2), "BESSELJ(1,1)", CellValue::Number(0.44005058567713012)),
            ((0, 70, 3), "BESSELJ(-1,1)", CellValue::Number(-0.44005058567713012)),
            ((0, 70, 4), "BESSELJ(2.5,1)", CellValue::Number(0.49709410250442176)),
            ((0, 70, 5), "BESSELJ(-2.5,1)", CellValue::Number(-0.49709410250442176)),
            ((0, 70, 6), "BESSELJ(\"5\",1)", CellValue::Number(-0.32757913856636323)),
            ((0, 70, 7), "BESSELJ(0.5,0)", CellValue::Number(0.93846980742354058)),
            ((0, 70, 8), "BESSELJ(0.5,3)", CellValue::Number(2.5637299945872449e-3)),
            ((0, 70, 9), "BESSELJ(0.5,2.9)", CellValue::Number(3.0604023458698646e-2)),
            ((0, 70, 10), "BESSELJ(0.5,10)", CellValue::Number(2.6131773608228023e-13)),
            ((0, 70, 11), "BESSELJ(10,1)", CellValue::Number(0.043472249652254151)),
            ((0, 70, 12), "BESSELJ(0.5,-1)", CellValue::Error(CellError::Num)),
            // BESSELY
            ((0, 70, 13), "BESSELY(100,3)", CellValue::Number(2.3445786618508869e-2)),
            ((0, 70, 14), "BESSELY(0,3)", CellValue::Error(CellError::Num)),
            ((0, 70, 15), "BESSELY(1,3)", CellValue::Number(-5.8215176322626707)),
            ((0, 70, 16), "BESSELY(-1,3)", CellValue::Error(CellError::Num)),
            ((0, 70, 17), "BESSELY(0.5,3)", CellValue::Number(-42.059494277784459)),
            ((0, 70, 18), "BESSELY(2.5,3)", CellValue::Number(-0.75605549501237423)),
            ((0, 70, 19), "BESSELY(10,3)", CellValue::Number(-0.25136265710573502)),
            ((0, 70, 20), "BESSELY(100,0)", CellValue::Number(-7.72443133567247e-2)),
            ((0, 70, 21), "BESSELY(100,1)", CellValue::Number(-2.0372311933786909e-2)),
            ((0, 70, 22), "BESSELY(100,2.9)", CellValue::Number(7.6836867118048963e-2)),
            ((0, 70, 23), "BESSELY(100,10)", CellValue::Number(5.833157426221653e-2)),
            ((0, 70, 24), "BESSELY(100,1000)", CellValue::Error(CellError::Num)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() <= 1e-12 * (1.0 + e.abs()), "formula `{formula}` evaluated to {a:.17e}, expected {e:.17e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_yield_compliance() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 80, 1), "YIELD(46000, 47119, 10, 10, 100, 2, 2)", CellValue::Number(14.226343812645517)),
            ((0, 80, 2), "YIELD(46000, 47119, 100, 10, 100, 2, 2)", CellValue::Error(CellError::Num)),
            ((0, 80, 3), "YIELD(46000, 47119, 1E+15, 10, 100, 2, 2)", CellValue::Error(CellError::Num)),
            ((0, 80, 4), "YIELD(1, 47119, 10, 10, 100, 2, 2)", CellValue::Error(CellError::Num)),
            ((0, 80, 5), "YIELD(46000, 47119, 0, 10, 100, 2, 2)", CellValue::Number(0.9161862304290855)),
            ((0, 80, 6), "YIELD(46000, 47119, 1, 10, 100, 2, 2)", CellValue::Number(7.056906353958439)),
            ((0, 80, 7), "YIELD(46000, 47119, 0.5, 10, 100, 2, 2)", CellValue::Number(4.356741958739219)),
            ((0, 80, 8), "YIELD(46000, 47119, 2.5, 10, 100, 2, 2)", CellValue::Number(10.793478864682273)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() <= 1e-12 * (1.0 + e.abs()), "formula `{formula}` evaluated to {a:.17e}, expected {e:.17e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_getpivotdata_calculation() {
        let mut book = MockBook::new(false);
        book.mock_pivots.insert("Sales:".to_string(), CellValue::Number(1000.0));
        book.mock_pivots.insert("Sales:Region=East".to_string(), CellValue::Number(450.0));
        book.mock_pivots.insert("Sales:Region=East;Year=2023".to_string(), CellValue::Number(200.0));
        book.mock_pivots.insert("Sales:Region=West;Year=2023".to_string(), CellValue::Number(300.0));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            ((0, 90, 1), "GETPIVOTDATA(\"Sales\", A1)", CellValue::Number(1000.0)),
            ((0, 90, 2), "GETPIVOTDATA(\"Sales\", $A$1, \"Region\", \"East\")", CellValue::Number(450.0)),
            ((0, 90, 3), "GETPIVOTDATA(\"Sales\", A1, \"Region\", \"East\", \"Year\", 2023)", CellValue::Number(200.0)),
            ((0, 90, 4), "GETPIVOTDATA(A1, \"Sales Region East\")", CellValue::Number(450.0)),
            ((0, 90, 5), "GETPIVOTDATA(\"Sales\", A1, \"Region\", \"North\")", CellValue::Error(CellError::Ref)),
            ((0, 90, 6), "GETPIVOTDATA(\"Profit\", A1)", CellValue::Error(CellError::Ref)),
            ((0, 90, 7), "GETPIVOTDATA(\"Sales\", \"NotARef\")", CellValue::Error(CellError::Ref)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_empty_cell_and_cross_type_semantics() {
        let mut book = MockBook::new(false);
        // Cell (0, 0, 0) (A1) and (0, 0, 1) (B1) are unassigned / empty.
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Empty cell comparisons with literals
            ((0, 100, 0), "A1=0", CellValue::Bool(true)),
            ((0, 100, 1), "A1=\"\"", CellValue::Bool(true)),
            ((0, 100, 2), "A1=FALSE", CellValue::Bool(false)),
            ((0, 100, 3), "A1=TRUE", CellValue::Bool(false)),
            ((0, 100, 4), "A1<FALSE", CellValue::Bool(true)),
            ((0, 100, 5), "A1>FALSE", CellValue::Bool(false)),
            ((0, 100, 6), "A1<TRUE", CellValue::Bool(true)),
            ((0, 100, 7), "A1=B1", CellValue::Bool(true)), // two empty cells are equal
            ((0, 100, 8), "A1<B1", CellValue::Bool(false)),
            ((0, 100, 9), "A1<=B1", CellValue::Bool(true)),

            // 2. Cross-type literal comparisons: Number < Text < Bool
            ((0, 101, 0), "0=FALSE", CellValue::Bool(false)),
            ((0, 101, 1), "1=TRUE", CellValue::Bool(false)),
            ((0, 101, 2), "\"\"=FALSE", CellValue::Bool(false)),
            ((0, 101, 3), "\"\"=0", CellValue::Bool(false)),
            ((0, 101, 4), "12345<\"0\"", CellValue::Bool(true)), // Number < Text
            ((0, 101, 5), "99999<\"a\"", CellValue::Bool(true)),
            ((0, 101, 6), "\"zzz\"<FALSE", CellValue::Bool(true)), // Text < Bool
            ((0, 101, 7), "\"true\"<TRUE", CellValue::Bool(true)),
            ((0, 101, 8), "FALSE<TRUE", CellValue::Bool(true)),
            ((0, 101, 9), "100<FALSE", CellValue::Bool(true)),

            // 3. Arithmetic and concatenation with empty cells
            ((0, 102, 0), "A1+10", CellValue::Number(10.0)),
            ((0, 102, 1), "10-A1", CellValue::Number(10.0)),
            ((0, 102, 2), "A1-10", CellValue::Number(-10.0)),
            ((0, 102, 3), "A1*5", CellValue::Number(0.0)),
            ((0, 102, 4), "A1/5", CellValue::Number(0.0)),
            ((0, 102, 5), "5/A1", CellValue::Error(CellError::Div0)),
            ((0, 102, 6), "A1^0", CellValue::Error(CellError::Num)), // In Excel, 0^0 produces #NUM!
            ((0, 102, 7), "A1^2", CellValue::Number(0.0)),
            ((0, 102, 8), "A1&\"hello\"", CellValue::Text("hello".into())),
            ((0, 102, 9), "\"world\"&A1", CellValue::Text("world".into())),
            ((0, 102, 10), "A1&B1", CellValue::Text("".into())),
            ((0, 102, 11), "-A1", CellValue::Number(0.0)),
            ((0, 102, 12), "+A1", CellValue::Number(0.0)),
            ((0, 102, 13), "A1%", CellValue::Number(0.0)),

            // 4. Function behavior with empty cells
            ((0, 103, 0), "ISBLANK(A1)", CellValue::Bool(true)),
            ((0, 103, 1), "ISNUMBER(A1)", CellValue::Bool(false)),
            ((0, 103, 2), "ISTEXT(A1)", CellValue::Bool(false)),
            ((0, 103, 3), "ISLOGICAL(A1)", CellValue::Bool(false)),
            ((0, 103, 4), "N(A1)", CellValue::Number(0.0)),
            ((0, 103, 5), "T(A1)", CellValue::Text("".into())),
            ((0, 103, 6), "LEN(A1)", CellValue::Number(0.0)),
            ((0, 103, 7), "IF(A1, 1, 2)", CellValue::Number(2.0)),
            ((0, 103, 8), "IF(NOT(A1), 1, 2)", CellValue::Number(1.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_1900_leap_year_boundary_dates() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // Boundary cases around 1900 leap day (serial 60 = 1900-02-29)
            ((0, 110, 0), "DATE(1900, 1, 1)", CellValue::Number(1.0)),
            ((0, 110, 1), "DATE(1900, 2, 28)", CellValue::Number(59.0)),
            ((0, 110, 2), "DATE(1900, 2, 29)", CellValue::Number(60.0)),
            ((0, 110, 3), "DATE(1900, 3, 1)", CellValue::Number(61.0)),
            ((0, 110, 4), "YEAR(60)", CellValue::Number(1900.0)),
            ((0, 110, 5), "MONTH(60)", CellValue::Number(2.0)),
            ((0, 110, 6), "DAY(60)", CellValue::Number(29.0)),
            ((0, 110, 7), "YEAR(59)", CellValue::Number(1900.0)),
            ((0, 110, 8), "MONTH(59)", CellValue::Number(2.0)),
            ((0, 110, 9), "DAY(59)", CellValue::Number(28.0)),
            ((0, 110, 10), "YEAR(61)", CellValue::Number(1900.0)),
            ((0, 110, 11), "MONTH(61)", CellValue::Number(3.0)),
            ((0, 110, 12), "DAY(61)", CellValue::Number(1.0)),
            ((0, 110, 13), "WEEKDAY(1)", CellValue::Number(1.0)), // 1900-01-01 is Sunday (type 1 default: 1 = Sun)
            ((0, 110, 14), "WEEKDAY(60)", CellValue::Number(4.0)), // 1900-02-29 is Wednesday
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_conditional_short_circuit_and_error_handling() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. IF short-circuit
            ((0, 120, 0), "IF(TRUE, 42, 1/0)", CellValue::Number(42.0)),
            ((0, 120, 1), "IF(FALSE, 1/0, 99)", CellValue::Number(99.0)),
            ((0, 120, 2), "IF(1=1, \"ok\", SQRT(-1))", CellValue::Text("ok".into())),
            ((0, 120, 3), "IF(1=2, SQRT(-1), \"ok\")", CellValue::Text("ok".into())),

            // 2. CHOOSE short-circuit
            ((0, 120, 4), "CHOOSE(1, 10, 1/0, SQRT(-1))", CellValue::Number(10.0)),
            ((0, 120, 5), "CHOOSE(2, 1/0, 20, SQRT(-1))", CellValue::Number(20.0)),
            ((0, 120, 6), "CHOOSE(3, 1/0, SQRT(-1), 30)", CellValue::Number(30.0)),

            // 3. IFERROR and IFNA
            ((0, 120, 7), "IFERROR(100, 1/0)", CellValue::Number(100.0)),
            ((0, 120, 8), "IFERROR(1/0, \"fallback\")", CellValue::Text("fallback".into())),
            ((0, 120, 9), "IFERROR(SQRT(-1), 0)", CellValue::Number(0.0)),
            ((0, 120, 10), "IFNA(NA(), \"was_na\")", CellValue::Text("was_na".into())),
            ((0, 120, 11), "IFNA(1/0, \"was_na\")", CellValue::Error(CellError::Div0)), // non-#N/A errors pass through
            ((0, 120, 12), "IFNA(50, 1/0)", CellValue::Number(50.0)),

            // 4. IFS short-circuit
            ((0, 120, 13), "IFS(TRUE, 1, TRUE, 1/0)", CellValue::Number(1.0)),
            ((0, 120, 14), "IFS(FALSE, 1/0, TRUE, 2)", CellValue::Number(2.0)),

            // 5. SWITCH short-circuit
            ((0, 120, 15), "SWITCH(1, 1, \"one\", 2, 1/0)", CellValue::Text("one".into())),
            ((0, 120, 16), "SWITCH(2, 1, 1/0, 2, \"two\")", CellValue::Text("two".into())),
            ((0, 120, 17), "SWITCH(9, 1, 1/0, \"default\")", CellValue::Text("default".into())),

            // 6. IF short-circuit with unexecuted circular references (must not be treated as active cycles)
            ((0, 120, 18), "IF(TRUE, 100, S121+1)", CellValue::Number(100.0)),
            ((0, 120, 19), "IF(FALSE, T121+1, 200)", CellValue::Number(200.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_xlookup_and_xmatch_modes() {
        let mut book = MockBook::new(false);
        book.dynamic = true;

        // Keys in row 0: A1:E1 -> [10, 20, 30, 40, 50]
        // Values in row 1: A2:E2 -> ["A", "B", "C", "D", "E"]
        book.values.insert((0, 0, 0), CellValue::Number(10.0));
        book.values.insert((0, 0, 1), CellValue::Number(20.0));
        book.values.insert((0, 0, 2), CellValue::Number(30.0));
        book.values.insert((0, 0, 3), CellValue::Number(40.0));
        book.values.insert((0, 0, 4), CellValue::Number(50.0));

        book.values.insert((0, 1, 0), CellValue::Text("A".into()));
        book.values.insert((0, 1, 1), CellValue::Text("B".into()));
        book.values.insert((0, 1, 2), CellValue::Text("C".into()));
        book.values.insert((0, 1, 3), CellValue::Text("D".into()));
        book.values.insert((0, 1, 4), CellValue::Text("E".into()));

        // Duplicate keys for reverse search testing:
        // A3:E3 -> ["apple", "banana", "cherry", "banana", "date"]
        // A4:E4 -> [1, 2, 3, 4, 5]
        book.values.insert((0, 2, 0), CellValue::Text("apple".into()));
        book.values.insert((0, 2, 1), CellValue::Text("banana".into()));
        book.values.insert((0, 2, 2), CellValue::Text("cherry".into()));
        book.values.insert((0, 2, 3), CellValue::Text("banana".into()));
        book.values.insert((0, 2, 4), CellValue::Text("date".into()));

        book.values.insert((0, 3, 0), CellValue::Number(1.0));
        book.values.insert((0, 3, 1), CellValue::Number(2.0));
        book.values.insert((0, 3, 2), CellValue::Number(3.0));
        book.values.insert((0, 3, 3), CellValue::Number(4.0));
        book.values.insert((0, 3, 4), CellValue::Number(5.0));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Basic exact match
            ((0, 10, 0), "XLOOKUP(30, A1:E1, A2:E2)", CellValue::Text("C".into())),
            ((0, 10, 1), "XMATCH(30, A1:E1)", CellValue::Number(3.0)),

            // 2. Not found with if_not_found fallback
            ((0, 10, 2), "XLOOKUP(99, A1:E1, A2:E2, \"missing\")", CellValue::Text("missing".into())),
            ((0, 10, 3), "XLOOKUP(99, A1:E1, A2:E2)", CellValue::Error(CellError::NA)),
            ((0, 10, 4), "XMATCH(99, A1:E1)", CellValue::Error(CellError::NA)),

            // 3. Approximate match: match_mode = -1 (exact or next smaller)
            ((0, 10, 5), "XLOOKUP(25, A1:E1, A2:E2, \"none\", -1)", CellValue::Text("B".into())), // 20 <= 25 -> B
            ((0, 10, 6), "XMATCH(25, A1:E1, -1)", CellValue::Number(2.0)),
            ((0, 10, 7), "XLOOKUP(5, A1:E1, A2:E2, \"none\", -1)", CellValue::Text("none".into())), // nothing smaller

            // 4. Approximate match: match_mode = 1 (exact or next larger)
            ((0, 10, 8), "XLOOKUP(25, A1:E1, A2:E2, \"none\", 1)", CellValue::Text("C".into())), // 30 >= 25 -> C
            ((0, 10, 9), "XMATCH(25, A1:E1, 1)", CellValue::Number(3.0)),
            ((0, 10, 10), "XLOOKUP(55, A1:E1, A2:E2, \"none\", 1)", CellValue::Text("none".into())), // nothing larger

            // 5. Wildcard match: match_mode = 2 vs match_mode = 0 (exact default)
            ((0, 10, 11), "XLOOKUP(\"ch*\", A3:E3, A4:E4, 0, 2)", CellValue::Number(3.0)), // wildcard matches cherry
            ((0, 10, 12), "XLOOKUP(\"ch*\", A3:E3, A4:E4, 0, 0)", CellValue::Number(0.0)), // exact match fails, returns 0
            ((0, 10, 13), "XMATCH(\"d?te\", A3:E3, 2)", CellValue::Number(5.0)), // ? wildcard matches date
            ((0, 10, 14), "XMATCH(\"d?te\", A3:E3, 0)", CellValue::Error(CellError::NA)),

            // 6. Reverse search: search_mode = -1 (last to first)
            ((0, 10, 15), "XLOOKUP(\"banana\", A3:E3, A4:E4, 0, 0, 1)", CellValue::Number(2.0)), // first occurrence
            ((0, 10, 16), "XLOOKUP(\"banana\", A3:E3, A4:E4, 0, 0, -1)", CellValue::Number(4.0)), // last occurrence
            ((0, 10, 17), "XMATCH(\"banana\", A3:E3, 0, 1)", CellValue::Number(2.0)),
            ((0, 10, 18), "XMATCH(\"banana\", A3:E3, 0, -1)", CellValue::Number(4.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly");
        }
    }

    #[test]
    fn test_lambda_helper_functions() {
        let mut book = MockBook::new(false);
        book.dynamic = true;

        // Cells for testing:
        // A1:B2 -> [1, 2; 3, 4]
        book.values.insert((0, 0, 0), CellValue::Number(1.0));
        book.values.insert((0, 0, 1), CellValue::Number(2.0));
        book.values.insert((0, 1, 0), CellValue::Number(3.0));
        book.values.insert((0, 1, 1), CellValue::Number(4.0));

        // 1. MAKEARRAY: 2x3 grid
        book.formulas.insert((0, 10, 0), "MAKEARRAY(2, 3, LAMBDA(r, c, r*10+c))".to_string());
        // 2. MAP: double each element of A1:B2
        book.formulas.insert((0, 15, 0), "MAP(A1:B2, LAMBDA(x, x*2))".to_string());
        // 3. REDUCE: sum of 1..5
        book.formulas.insert((0, 20, 0), "REDUCE(0, {1, 2, 3, 4, 5}, LAMBDA(acc, val, acc+val))".to_string());
        // 4. SCAN: running sum of {1, 2, 3, 4}
        book.formulas.insert((0, 25, 0), "SCAN(0, {1, 2, 3, 4}, LAMBDA(acc, val, acc+val))".to_string());
        // 5. BYROW: row sums of A1:B2
        book.formulas.insert((0, 30, 0), "BYROW(A1:B2, LAMBDA(r, SUM(r)))".to_string());
        // 6. BYCOL: column sums of A1:B2
        book.formulas.insert((0, 35, 0), "BYCOL(A1:B2, LAMBDA(c, SUM(c)))".to_string());

        let cells = vec![(0, 10, 0), (0, 15, 0), (0, 20, 0), (0, 25, 0), (0, 30, 0), (0, 35, 0)];
        let res = recalculate(&book, &cells, &cells);

        // 1. MAKEARRAY: top-left is 11, spill is 2x3
        let m = res.iter().find(|r| r.cell == (0, 10, 0)).unwrap();
        assert_eq!(m.value, CellValue::Number(11.0));
        assert_eq!(m.spill, Some((2, 3, vec![
            CellValue::Number(11.0), CellValue::Number(12.0), CellValue::Number(13.0),
            CellValue::Number(21.0), CellValue::Number(22.0), CellValue::Number(23.0),
        ])));

        // 2. MAP: top-left is 2, spill is 2x2: [2, 4; 6, 8]
        let mp = res.iter().find(|r| r.cell == (0, 15, 0)).unwrap();
        assert_eq!(mp.value, CellValue::Number(2.0));
        assert_eq!(mp.spill, Some((2, 2, vec![
            CellValue::Number(2.0), CellValue::Number(4.0),
            CellValue::Number(6.0), CellValue::Number(8.0),
        ])));

        // 3. REDUCE: scalar 15
        let rd = res.iter().find(|r| r.cell == (0, 20, 0)).unwrap();
        assert_eq!(rd.value, CellValue::Number(15.0));
        assert_eq!(rd.spill, None);

        // 4. SCAN: top-left is 1, spill is 1x4: [1, 3, 6, 10]
        let sc = res.iter().find(|r| r.cell == (0, 25, 0)).unwrap();
        assert_eq!(sc.value, CellValue::Number(1.0));
        assert_eq!(sc.spill, Some((1, 4, vec![
            CellValue::Number(1.0), CellValue::Number(3.0), CellValue::Number(6.0), CellValue::Number(10.0),
        ])));

        // 5. BYROW: top-left is 3 (1+2), spill is 2x1: [3; 7]
        let br = res.iter().find(|r| r.cell == (0, 30, 0)).unwrap();
        assert_eq!(br.value, CellValue::Number(3.0));
        assert_eq!(br.spill, Some((2, 1, vec![
            CellValue::Number(3.0), CellValue::Number(7.0),
        ])));

        // 6. BYCOL: top-left is 4 (1+3), spill is 1x2: [4, 6]
        let bc = res.iter().find(|r| r.cell == (0, 35, 0)).unwrap();
        assert_eq!(bc.value, CellValue::Number(4.0));
        assert_eq!(bc.spill, Some((1, 2, vec![
            CellValue::Number(4.0), CellValue::Number(6.0),
        ])));
    }

    #[test]
    fn test_array_reshaping_and_manipulation_functions() {
        let mut book = MockBook::new(false);
        book.dynamic = true;

        // Populate base 3x3 matrix in A1:C3
        // [ 1, 2, 3;
        //   4, 5, 6;
        //   7, 8, 9 ]
        for r in 0..3 {
            for c in 0..3 {
                book.values.insert((0, r, c), CellValue::Number((r * 3 + c + 1) as f64));
            }
        }

        // Populate 2x3 matrix with empty cells and errors in D1:F2
        // [ 10,        Empty, 30;
        //   #DIV/0!,   50,    Empty ]
        book.values.insert((0, 0, 3), CellValue::Number(10.0));
        book.values.insert((0, 0, 4), CellValue::Empty);
        book.values.insert((0, 0, 5), CellValue::Number(30.0));
        book.values.insert((0, 1, 3), CellValue::Error(CellError::Div0));
        book.values.insert((0, 1, 4), CellValue::Number(50.0));
        book.values.insert((0, 1, 5), CellValue::Empty);

        // Define test cases for dynamic array reshaping formulas
        let cases: &[((usize, u32, u32), &str, CellValue, Option<(usize, usize, Vec<CellValue>)>)] = &[
            // 1. TAKE: positive rows and columns
            ((0, 10, 0), "TAKE(A1:C3, 2, 2)", CellValue::Number(1.0), Some((2, 2, vec![
                CellValue::Number(1.0), CellValue::Number(2.0),
                CellValue::Number(4.0), CellValue::Number(5.0),
            ]))),
            // TAKE: negative rows and columns (from end)
            ((0, 10, 5), "TAKE(A1:C3, -2, -1)", CellValue::Number(6.0), Some((2, 1, vec![
                CellValue::Number(6.0),
                CellValue::Number(9.0),
            ]))),
            // TAKE: rows exceeding size (clamped to max available)
            ((0, 10, 10), "TAKE(A1:C3, 5, 1)", CellValue::Number(1.0), Some((3, 1, vec![
                CellValue::Number(1.0),
                CellValue::Number(4.0),
                CellValue::Number(7.0),
            ]))),
            // TAKE: 0 rows or cols yields #VALUE! (Excel specification)
            ((0, 10, 15), "TAKE(A1:C3, 0, 1)", CellValue::Error(CellError::Value), None),
            ((0, 10, 20), "TAKE(A1:C3, 1, 0)", CellValue::Error(CellError::Value), None),

            // 2. DROP: drop 1 row and 1 column
            ((0, 15, 0), "DROP(A1:C3, 1, 1)", CellValue::Number(5.0), Some((2, 2, vec![
                CellValue::Number(5.0), CellValue::Number(6.0),
                CellValue::Number(8.0), CellValue::Number(9.0),
            ]))),
            // DROP: negative drop (from end)
            ((0, 15, 5), "DROP(A1:C3, -1, -2)", CellValue::Number(1.0), Some((2, 1, vec![
                CellValue::Number(1.0),
                CellValue::Number(4.0),
            ]))),
            // DROP: dropping all rows or columns yields #VALUE! (Excel specification)
            ((0, 15, 10), "DROP(A1:C3, 3, 1)", CellValue::Error(CellError::Value), None),
            ((0, 15, 15), "DROP(A1:C3, 1, 3)", CellValue::Error(CellError::Value), None),

            // 3. EXPAND: with default padding (#N/A)
            ((0, 20, 0), "EXPAND(A1:B2, 3, 3)", CellValue::Number(1.0), Some((3, 3, vec![
                CellValue::Number(1.0), CellValue::Number(2.0), CellValue::Error(CellError::NA),
                CellValue::Number(4.0), CellValue::Number(5.0), CellValue::Error(CellError::NA),
                CellValue::Error(CellError::NA), CellValue::Error(CellError::NA), CellValue::Error(CellError::NA),
            ]))),
            // EXPAND: custom scalar padding (number and text)
            ((0, 20, 5), "EXPAND(A1:B2, 3, 3, 0)", CellValue::Number(1.0), Some((3, 3, vec![
                CellValue::Number(1.0), CellValue::Number(2.0), CellValue::Number(0.0),
                CellValue::Number(4.0), CellValue::Number(5.0), CellValue::Number(0.0),
                CellValue::Number(0.0), CellValue::Number(0.0), CellValue::Number(0.0),
            ]))),
            ((0, 20, 10), "EXPAND(A1:B2, 2, 3, \"pad\")", CellValue::Number(1.0), Some((2, 3, vec![
                CellValue::Number(1.0), CellValue::Number(2.0), CellValue::Text("pad".into()),
                CellValue::Number(4.0), CellValue::Number(5.0), CellValue::Text("pad".into()),
            ]))),
            // EXPAND: dimensions smaller than source yield #VALUE!
            ((0, 20, 15), "EXPAND(A1:B2, 1, 2)", CellValue::Error(CellError::Value), None),
            ((0, 20, 20), "EXPAND(A1:B2, 2, 1)", CellValue::Error(CellError::Value), None),

            // 4. TOCOL: ignore flags (0=none, 1=blanks, 2=errors, 3=both) and scan order
            // ignore=1 (skip empty cells E1, F2)
            ((0, 25, 0), "TOCOL(D1:F2, 1)", CellValue::Number(10.0), Some((4, 1, vec![
                CellValue::Number(10.0),
                CellValue::Number(30.0),
                CellValue::Error(CellError::Div0),
                CellValue::Number(50.0),
            ]))),
            // ignore=2 (skip error cell D2, empty cells display as 0.0 in spill)
            ((0, 25, 5), "TOCOL(D1:F2, 2)", CellValue::Number(10.0), Some((5, 1, vec![
                CellValue::Number(10.0),
                CellValue::Number(0.0),
                CellValue::Number(30.0),
                CellValue::Number(50.0),
                CellValue::Number(0.0),
            ]))),
            // ignore=3 (skip blanks and errors, row-wise)
            ((0, 25, 10), "TOCOL(D1:F2, 3)", CellValue::Number(10.0), Some((3, 1, vec![
                CellValue::Number(10.0),
                CellValue::Number(30.0),
                CellValue::Number(50.0),
            ]))),
            // ignore=3 with by_col=TRUE (column-wise scan)
            ((0, 25, 15), "TOCOL(D1:F2, 3, TRUE)", CellValue::Number(10.0), Some((3, 1, vec![
                CellValue::Number(10.0),
                CellValue::Number(50.0),
                CellValue::Number(30.0),
            ]))),
            // invalid ignore flag yields #VALUE!
            ((0, 25, 20), "TOCOL(D1:F2, 5)", CellValue::Error(CellError::Value), None),

            // 5. TOROW: row vector output
            ((0, 30, 0), "TOROW(D1:F2, 3)", CellValue::Number(10.0), Some((1, 3, vec![
                CellValue::Number(10.0), CellValue::Number(30.0), CellValue::Number(50.0),
            ]))),
            ((0, 30, 5), "TOROW(D1:F2, 3, TRUE)", CellValue::Number(10.0), Some((1, 3, vec![
                CellValue::Number(10.0), CellValue::Number(50.0), CellValue::Number(30.0),
            ]))),

            // 6. WRAPROWS: wrapping vector into rows
            ((0, 35, 0), "WRAPROWS({1, 2, 3, 4, 5}, 2)", CellValue::Number(1.0), Some((3, 2, vec![
                CellValue::Number(1.0), CellValue::Number(2.0),
                CellValue::Number(3.0), CellValue::Number(4.0),
                CellValue::Number(5.0), CellValue::Error(CellError::NA),
            ]))),
            ((0, 35, 5), "WRAPROWS({1, 2, 3, 4, 5}, 2, 99)", CellValue::Number(1.0), Some((3, 2, vec![
                CellValue::Number(1.0), CellValue::Number(2.0),
                CellValue::Number(3.0), CellValue::Number(4.0),
                CellValue::Number(5.0), CellValue::Number(99.0),
            ]))),
            ((0, 35, 10), "WRAPROWS({1, 2, 3, 4}, 2)", CellValue::Number(1.0), Some((2, 2, vec![
                CellValue::Number(1.0), CellValue::Number(2.0),
                CellValue::Number(3.0), CellValue::Number(4.0),
            ]))),
            // wrap_count < 1 yields #NUM!
            ((0, 35, 15), "WRAPROWS({1, 2, 3}, 0)", CellValue::Error(CellError::Num), None),
            // 2D input yields #VALUE!
            ((0, 35, 20), "WRAPROWS(A1:B2, 2)", CellValue::Error(CellError::Value), None),

            // 7. WRAPCOLS: wrapping vector into columns
            ((0, 40, 0), "WRAPCOLS({1, 2, 3, 4, 5}, 2)", CellValue::Number(1.0), Some((2, 3, vec![
                CellValue::Number(1.0), CellValue::Number(3.0), CellValue::Number(5.0),
                CellValue::Number(2.0), CellValue::Number(4.0), CellValue::Error(CellError::NA),
            ]))),
            ((0, 40, 5), "WRAPCOLS({1, 2, 3, 4, 5}, 2, \"fill\")", CellValue::Number(1.0), Some((2, 3, vec![
                CellValue::Number(1.0), CellValue::Number(3.0), CellValue::Number(5.0),
                CellValue::Number(2.0), CellValue::Number(4.0), CellValue::Text("fill".into()),
            ]))),

            // 8. CHOOSEROWS & CHOOSECOLS: indexing and negative indexing
            ((0, 45, 0), "CHOOSEROWS(A1:C3, 3, 1)", CellValue::Number(7.0), Some((2, 3, vec![
                CellValue::Number(7.0), CellValue::Number(8.0), CellValue::Number(9.0),
                CellValue::Number(1.0), CellValue::Number(2.0), CellValue::Number(3.0),
            ]))),
            ((0, 45, 5), "CHOOSEROWS(A1:C3, -1)", CellValue::Number(7.0), Some((1, 3, vec![
                CellValue::Number(7.0), CellValue::Number(8.0), CellValue::Number(9.0),
            ]))),
            ((0, 45, 10), "CHOOSEROWS(A1:C3, 0)", CellValue::Error(CellError::Value), None),
            ((0, 45, 15), "CHOOSEROWS(A1:C3, 4)", CellValue::Error(CellError::Value), None),
            ((0, 45, 20), "CHOOSECOLS(A1:C3, 3, 2)", CellValue::Number(3.0), Some((3, 2, vec![
                CellValue::Number(3.0), CellValue::Number(2.0),
                CellValue::Number(6.0), CellValue::Number(5.0),
                CellValue::Number(9.0), CellValue::Number(8.0),
            ]))),
            ((0, 45, 25), "CHOOSECOLS(A1:C3, -1)", CellValue::Number(3.0), Some((3, 1, vec![
                CellValue::Number(3.0),
                CellValue::Number(6.0),
                CellValue::Number(9.0),
            ]))),

            // 9. HSTACK & VSTACK: stacking with different dimensions
            ((0, 50, 0), "HSTACK({1, 2; 3, 4}, {5; 6})", CellValue::Number(1.0), Some((2, 3, vec![
                CellValue::Number(1.0), CellValue::Number(2.0), CellValue::Number(5.0),
                CellValue::Number(3.0), CellValue::Number(4.0), CellValue::Number(6.0),
            ]))),
            ((0, 50, 5), "HSTACK({1; 2}, {3, 4})", CellValue::Number(1.0), Some((2, 3, vec![
                CellValue::Number(1.0), CellValue::Number(3.0), CellValue::Number(4.0),
                CellValue::Number(2.0), CellValue::Error(CellError::NA), CellValue::Error(CellError::NA),
            ]))),
            ((0, 50, 10), "VSTACK({1, 2}, {3, 4; 5, 6})", CellValue::Number(1.0), Some((3, 2, vec![
                CellValue::Number(1.0), CellValue::Number(2.0),
                CellValue::Number(3.0), CellValue::Number(4.0),
                CellValue::Number(5.0), CellValue::Number(6.0),
            ]))),
            ((0, 50, 15), "VSTACK({1, 2}, {3})", CellValue::Number(1.0), Some((2, 2, vec![
                CellValue::Number(1.0), CellValue::Number(2.0),
                CellValue::Number(3.0), CellValue::Error(CellError::NA),
            ]))),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected_val, expected_spill) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).expect(formula);
            assert_eq!(&actual.value, expected_val, "formula `{formula}` value mismatch");
            assert_eq!(&actual.spill, expected_spill, "formula `{formula}` spill mismatch");
        }
    }

    #[test]
    fn test_database_functions() {
        let mut book = MockBook::new(false);

        // Database table in A1:E7
        // Header: Tree (col 0), Height (col 1), Age (col 2), Yield (col 3), Profit (col 4)
        let headers = ["Tree", "Height", "Age", "Yield", "Profit"];
        for (c, &h) in headers.iter().enumerate() {
            book.values.insert((0, 0, c as u32), CellValue::Text(h.into()));
        }

        // Data rows (A2:E7)
        let data: &[(&str, f64, f64, f64, f64)] = &[
            ("Apple", 18.0, 20.0, 14.0, 105.0),
            ("Pear", 12.0, 12.0, 10.0, 96.0),
            ("Cherry", 13.0, 14.0, 9.0, 105.0),
            ("Apple", 14.0, 15.0, 10.0, 75.0),
            ("Pear", 9.0, 8.0, 8.0, 77.0),
            ("Apple", 8.0, 9.0, 6.0, 45.0),
        ];
        for (r, &(tree, h, a, y, p)) in data.iter().enumerate() {
            let row = (r + 1) as u32;
            book.values.insert((0, row, 0), CellValue::Text(tree.into()));
            book.values.insert((0, row, 1), CellValue::Number(h));
            book.values.insert((0, row, 2), CellValue::Number(a));
            book.values.insert((0, row, 3), CellValue::Number(y));
            book.values.insert((0, row, 4), CellValue::Number(p));
        }

        // Criteria 1: G1:H2 -> Tree = "Apple", Height > 10 (matches rows 1, 4: Yields 14, 10; Profits 105, 75)
        book.values.insert((0, 0, 6), CellValue::Text("Tree".into()));
        book.values.insert((0, 0, 7), CellValue::Text("Height".into()));
        book.values.insert((0, 1, 6), CellValue::Text("Apple".into()));
        book.values.insert((0, 1, 7), CellValue::Text(">10".into()));

        // Criteria 2: G3:H4 -> Tree = "Pear", Age < 10 (matches single row 5: Yield 8, Profit 77)
        book.values.insert((0, 2, 6), CellValue::Text("Tree".into()));
        book.values.insert((0, 2, 7), CellValue::Text("Age".into()));
        book.values.insert((0, 3, 6), CellValue::Text("Pear".into()));
        book.values.insert((0, 3, 7), CellValue::Text("<10".into()));

        // Criteria 3: G5:H6 -> Tree = "Orange", Height > 0 (matches 0 rows)
        book.values.insert((0, 4, 6), CellValue::Text("Tree".into()));
        book.values.insert((0, 4, 7), CellValue::Text("Height".into()));
        book.values.insert((0, 5, 6), CellValue::Text("Orange".into()));
        book.values.insert((0, 5, 7), CellValue::Text(">0".into()));

        // Criteria 4: G7:H9 -> (Tree = "Apple" AND Height > 15) OR (Tree = "Pear" AND Height > 10) (matches rows 1, 2)
        book.values.insert((0, 6, 6), CellValue::Text("Tree".into()));
        book.values.insert((0, 6, 7), CellValue::Text("Height".into()));
        book.values.insert((0, 7, 6), CellValue::Text("Apple".into()));
        book.values.insert((0, 7, 7), CellValue::Text(">15".into()));
        book.values.insert((0, 8, 6), CellValue::Text("Pear".into()));
        book.values.insert((0, 8, 7), CellValue::Text(">10".into()));

        // Criteria 5: G10:G11 -> Tree = "Ap*" (wildcard prefix, matches rows 1, 4, 6)
        book.values.insert((0, 9, 6), CellValue::Text("Tree".into()));
        book.values.insert((0, 10, 6), CellValue::Text("Ap*".into()));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. DSUM
            ((0, 20, 0), "DSUM(A1:E7, \"Yield\", G1:H2)", CellValue::Number(24.0)),
            ((0, 20, 1), "DSUM(A1:E7, 4, G1:H2)", CellValue::Number(24.0)), // field as 1-based column index (col 4 is Yield)
            ((0, 20, 2), "DSUM(A1:E7, \"Profit\", G7:H9)", CellValue::Number(201.0)), // 105 + 96
            ((0, 20, 3), "DSUM(A1:E7, \"Yield\", G5:H6)", CellValue::Number(0.0)), // 0 matches

            // 2. DAVERAGE
            ((0, 20, 4), "DAVERAGE(A1:E7, \"Yield\", G1:H2)", CellValue::Number(12.0)), // (14 + 10) / 2
            ((0, 20, 5), "DAVERAGE(A1:E7, \"Yield\", G5:H6)", CellValue::Error(CellError::Div0)), // 0 matches yields #DIV/0!

            // 3. DCOUNT and DCOUNTA
            ((0, 20, 6), "DCOUNT(A1:E7, \"Yield\", G1:H2)", CellValue::Number(2.0)),
            ((0, 20, 7), "DCOUNT(A1:E7, , G1:H2)", CellValue::Number(2.0)), // omitted field counts rows
            ((0, 20, 8), "DCOUNTA(A1:E7, \"Yield\", G1:H2)", CellValue::Number(2.0)),
            ((0, 20, 9), "DCOUNTA(A1:E7, , G1:H2)", CellValue::Number(2.0)),
            ((0, 20, 10), "DCOUNT(A1:E7, \"Yield\", G10:G11)", CellValue::Number(3.0)), // wildcard "Ap*"

            // 4. DMAX and DMIN
            ((0, 20, 11), "DMAX(A1:E7, \"Profit\", G1:H2)", CellValue::Number(105.0)),
            ((0, 20, 12), "DMIN(A1:E7, \"Profit\", G1:H2)", CellValue::Number(75.0)),

            // 5. DPRODUCT
            ((0, 20, 13), "DPRODUCT(A1:E7, \"Yield\", G1:H2)", CellValue::Number(140.0)), // 14 * 10
            ((0, 20, 14), "DPRODUCT(A1:E7, \"Yield\", G5:H6)", CellValue::Number(0.0)), // empty yields 0

            // 6. DSTDEV, DSTDEVP, DVAR, DVARP
            ((0, 20, 15), "DSTDEV(A1:E7, \"Yield\", G1:H2)", CellValue::Number(std::f64::consts::SQRT_2 * 2.0)), // sqrt(8) = 2.8284271247461903
            ((0, 20, 16), "DSTDEVP(A1:E7, \"Yield\", G1:H2)", CellValue::Number(2.0)), // sqrt(4) = 2
            ((0, 20, 17), "DVAR(A1:E7, \"Yield\", G1:H2)", CellValue::Number(8.0)),
            ((0, 20, 18), "DVARP(A1:E7, \"Yield\", G1:H2)", CellValue::Number(4.0)),

            // 7. DGET
            ((0, 20, 19), "DGET(A1:E7, \"Yield\", G3:H4)", CellValue::Number(8.0)), // exactly 1 matching row
            ((0, 20, 20), "DGET(A1:E7, \"Profit\", G3:H4)", CellValue::Number(77.0)),
            ((0, 20, 21), "DGET(A1:E7, \"Yield\", G1:H2)", CellValue::Error(CellError::Num)), // multiple matches yield #NUM!
            ((0, 20, 22), "DGET(A1:E7, \"Yield\", G5:H6)", CellValue::Error(CellError::Value)), // 0 matches yield #VALUE!

            // 8. Field validation errors
            ((0, 20, 23), "DSUM(A1:E7, \"NonExistent\", G1:H2)", CellValue::Error(CellError::Value)),
            ((0, 20, 24), "DSUM(A1:E7, 99, G1:H2)", CellValue::Error(CellError::Value)),
            ((0, 20, 25), "DSUM(A1:E7, 0, G1:H2)", CellValue::Error(CellError::Value)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).map(|r| &r.value).expect(formula);
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_textsplit_dynamic_spill_and_modes() {
        let mut book = MockBook::new(false);
        book.dynamic = true;

        let cases: &[((usize, u32, u32), &str, CellValue, Option<(usize, usize, Vec<CellValue>)>)] = &[
            // 1. Column-only delimiter -> horizontal 1x3 spill
            ((0, 10, 0), "TEXTSPLIT(\"apple,banana,cherry\", \",\")", CellValue::Text("apple".into()), Some((1, 3, vec![
                CellValue::Text("apple".into()),
                CellValue::Text("banana".into()),
                CellValue::Text("cherry".into()),
            ]))),

            // 2. Row-only delimiter -> vertical 3x1 spill
            ((0, 10, 5), "TEXTSPLIT(\"one;two;three\", , \";\")", CellValue::Text("one".into()), Some((3, 1, vec![
                CellValue::Text("one".into()),
                CellValue::Text("two".into()),
                CellValue::Text("three".into()),
            ]))),

            // 3. Both row and column delimiters -> 2D 2x2 spill
            ((0, 10, 10), "TEXTSPLIT(\"1,2;3,4\", \",\", \";\")", CellValue::Text("1".into()), Some((2, 2, vec![
                CellValue::Text("1".into()), CellValue::Text("2".into()),
                CellValue::Text("3".into()), CellValue::Text("4".into()),
            ]))),

            // 4. Ragged 2D split with default padding (#N/A) -> 2x3 spill
            ((0, 10, 15), "TEXTSPLIT(\"a,b;c,d,e\", \",\", \";\")", CellValue::Text("a".into()), Some((2, 3, vec![
                CellValue::Text("a".into()), CellValue::Text("b".into()), CellValue::Error(CellError::NA),
                CellValue::Text("c".into()), CellValue::Text("d".into()), CellValue::Text("e".into()),
            ]))),

            // 5. Ragged 2D split with custom scalar padding ("pad") -> 2x3 spill
            ((0, 10, 20), "TEXTSPLIT(\"a,b;c,d,e\", \",\", \";\", , , \"pad\")", CellValue::Text("a".into()), Some((2, 3, vec![
                CellValue::Text("a".into()), CellValue::Text("b".into()), CellValue::Text("pad".into()),
                CellValue::Text("c".into()), CellValue::Text("d".into()), CellValue::Text("e".into()),
            ]))),

            // 6. ignore_empty = TRUE vs FALSE
            ((0, 10, 25), "TEXTSPLIT(\"x,,y\", \",\", , TRUE)", CellValue::Text("x".into()), Some((1, 2, vec![
                CellValue::Text("x".into()),
                CellValue::Text("y".into()),
            ]))),
            ((0, 10, 30), "TEXTSPLIT(\"x,,y\", \",\", , FALSE)", CellValue::Text("x".into()), Some((1, 3, vec![
                CellValue::Text("x".into()),
                CellValue::Text("".into()),
                CellValue::Text("y".into()),
            ]))),

            // 7. match_mode: 0 (case-sensitive) vs 1 (case-insensitive)
            ((0, 10, 35), "TEXTSPLIT(\"FooXBar\", \"x\", , , 1)", CellValue::Text("Foo".into()), Some((1, 2, vec![
                CellValue::Text("Foo".into()),
                CellValue::Text("Bar".into()),
            ]))),
            ((0, 10, 40), "TEXTSPLIT(\"FooXBar\", \"x\", , , 0)", CellValue::Text("FooXBar".into()), None), // single element, no spill

            // 8. Delimiter array constants: multiple delimiters {",", ";"}
            ((0, 10, 45), "TEXTSPLIT(\"cat,dog;bird\", {\",\", \";\"})", CellValue::Text("cat".into()), Some((1, 3, vec![
                CellValue::Text("cat".into()),
                CellValue::Text("dog".into()),
                CellValue::Text("bird".into()),
            ]))),

            // 9. Error cases
            ((0, 10, 50), "TEXTSPLIT(\"test\", \"\")", CellValue::Error(CellError::Value), None),
            ((0, 10, 55), "TEXTSPLIT(\"test\", , \"\")", CellValue::Error(CellError::Value), None),
            ((0, 10, 60), "TEXTSPLIT(\"test\", , , , 2)", CellValue::Error(CellError::Value), None), // match_mode 2 is invalid
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected_val, expected_spill) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).expect(formula);
            assert_eq!(&actual.value, expected_val, "formula `{formula}` value mismatch");
            assert_eq!(&actual.spill, expected_spill, "formula `{formula}` spill mismatch");
        }
    }

    #[test]
    fn test_dynamic_array_core_functions() {
        let mut book = MockBook::new(false);
        book.dynamic = true;

        // Populate mock table data in sheet 0
        // A1:C6 - Employee score dataset with duplicate row (Alice at row 1 and row 5)
        // Row indices: 0-based in MockBook: row 0 is header, 1..=5 are data rows
        let data_rows = [
            ("Alice", 85.0, "Sales"),
            ("Bob", 92.0, "Tech"),
            ("Carol", 85.0, "Sales"),
            ("Dave", 70.0, "Tech"),
            ("Alice", 85.0, "Sales"), // Duplicate of row 1
        ];
        book.values.insert((0, 0, 0), CellValue::Text("Name".into()));
        book.values.insert((0, 0, 1), CellValue::Text("Score".into()));
        book.values.insert((0, 0, 2), CellValue::Text("Dept".into()));
        for (i, (name, score, dept)) in data_rows.iter().enumerate() {
            let r = (i + 1) as u32;
            book.values.insert((0, r, 0), CellValue::Text((*name).into()));
            book.values.insert((0, r, 1), CellValue::Number(*score));
            book.values.insert((0, r, 2), CellValue::Text((*dept).into()));
        }

        // D1:D6 - Mixed numbers, empty cell, and error
        // D1=30, D2=10, D3=empty, D4=20, D5=10, D6=#DIV/0!
        book.values.insert((0, 0, 3), CellValue::Number(30.0));
        book.values.insert((0, 1, 3), CellValue::Number(10.0));
        // (0, 2, 3) left empty
        book.values.insert((0, 3, 3), CellValue::Number(20.0));
        book.values.insert((0, 4, 3), CellValue::Number(10.0));
        book.values.insert((0, 5, 3), CellValue::Error(CellError::Div0));

        // E1:I1 - Horizontal row vector
        // E1=50, F1=20, G1=40, H1=10, I1=20
        book.values.insert((0, 0, 4), CellValue::Number(50.0));
        book.values.insert((0, 0, 5), CellValue::Number(20.0));
        book.values.insert((0, 0, 6), CellValue::Number(40.0));
        book.values.insert((0, 0, 7), CellValue::Number(10.0));
        book.values.insert((0, 0, 8), CellValue::Number(20.0));

        let cases: &[((usize, u32, u32), &str, CellValue, Option<(usize, usize, Vec<CellValue>)>)] = &[
            // 1. SORT: 1D ascending order with empty cell and duplicate
            // D1:D5 = {30; 10; empty; 20; 10} -> sorted: {10; 10; 20; 30; 0}
            ((0, 10, 0), "SORT(D1:D5)", CellValue::Number(10.0), Some((5, 1, vec![
                CellValue::Number(10.0),
                CellValue::Number(10.0),
                CellValue::Number(20.0),
                CellValue::Number(30.0),
                CellValue::Number(0.0), // Empty cell in spill displays as 0.0
            ]))),
            // SORT: 1D descending order (empty cell remains at the end in Excel)
            ((0, 10, 5), "SORT(D1:D5, 1, -1)", CellValue::Number(30.0), Some((5, 1, vec![
                CellValue::Number(30.0),
                CellValue::Number(20.0),
                CellValue::Number(10.0),
                CellValue::Number(10.0),
                CellValue::Number(0.0),
            ]))),
            // SORT: 2D table sorted by column 2 (Score) descending
            // A2:B5 = {Alice, 85; Bob, 92; Carol, 85; Dave, 70}
            // sorted desc: Bob(92), Alice(85), Carol(85), Dave(70)
            ((0, 10, 10), "SORT(A2:B5, 2, -1)", CellValue::Text("Bob".into()), Some((4, 2, vec![
                CellValue::Text("Bob".into()), CellValue::Number(92.0),
                CellValue::Text("Alice".into()), CellValue::Number(85.0),
                CellValue::Text("Carol".into()), CellValue::Number(85.0),
                CellValue::Text("Dave".into()), CellValue::Number(70.0),
            ]))),
            // SORT: by_column = TRUE (horizontal sort of row vector E1:I1 {50, 20, 40, 10, 20})
            ((0, 10, 15), "SORT(E1:I1, 1, 1, TRUE)", CellValue::Number(10.0), Some((1, 5, vec![
                CellValue::Number(10.0),
                CellValue::Number(20.0),
                CellValue::Number(20.0),
                CellValue::Number(40.0),
                CellValue::Number(50.0),
            ]))),
            // SORT: error cases
            ((0, 10, 20), "SORT(A2:B5, 0)", CellValue::Error(CellError::Value), None),
            ((0, 10, 25), "SORT(A2:B5, 3)", CellValue::Error(CellError::Value), None), // Column index out of bounds
            ((0, 10, 30), "SORT(A2:B5, 1, 0)", CellValue::Error(CellError::Value), None), // Invalid order (not 1 or -1)
            ((0, 10, 35), "SORT(A2:B5, 1, 2)", CellValue::Error(CellError::Value), None),

            // 2. SORTBY: multiple sort keys (Score desc, then Name asc)
            // A2:A5 sorted by B2:B5 (-1) and A2:A5 (1)
            // Scores: Bob(92), Alice(85), Carol(85), Dave(70)
            ((0, 15, 0), "SORTBY(A2:A5, B2:B5, -1, A2:A5, 1)", CellValue::Text("Bob".into()), Some((4, 1, vec![
                CellValue::Text("Bob".into()),
                CellValue::Text("Alice".into()),
                CellValue::Text("Carol".into()),
                CellValue::Text("Dave".into()),
            ]))),
            // SORTBY: horizontal vector sort
            ((0, 15, 5), "SORTBY(E1:I1, E1:I1, 1)", CellValue::Number(10.0), Some((1, 5, vec![
                CellValue::Number(10.0),
                CellValue::Number(20.0),
                CellValue::Number(20.0),
                CellValue::Number(40.0),
                CellValue::Number(50.0),
            ]))),
            // SORTBY: dimension mismatch yields #VALUE!
            ((0, 15, 10), "SORTBY(A2:A5, B2:B4, 1)", CellValue::Error(CellError::Value), None),
            // SORTBY: invalid order yields #VALUE!
            ((0, 15, 15), "SORTBY(A2:A5, B2:B5, 0)", CellValue::Error(CellError::Value), None),

            // 3. UNIQUE: 1D deduplication (A2:A6 contains Alice twice)
            ((0, 20, 0), "UNIQUE(A2:A6)", CellValue::Text("Alice".into()), Some((4, 1, vec![
                CellValue::Text("Alice".into()),
                CellValue::Text("Bob".into()),
                CellValue::Text("Carol".into()),
                CellValue::Text("Dave".into()),
            ]))),
            // UNIQUE: 2D whole row deduplication (row 1 and row 5 are identical)
            ((0, 20, 5), "UNIQUE(A2:C6)", CellValue::Text("Alice".into()), Some((4, 3, vec![
                CellValue::Text("Alice".into()), CellValue::Number(85.0), CellValue::Text("Sales".into()),
                CellValue::Text("Bob".into()), CellValue::Number(92.0), CellValue::Text("Tech".into()),
                CellValue::Text("Carol".into()), CellValue::Number(85.0), CellValue::Text("Sales".into()),
                CellValue::Text("Dave".into()), CellValue::Number(70.0), CellValue::Text("Tech".into()),
            ]))),
            // UNIQUE: column-wise deduplication (E1:I1 {50, 20, 40, 10, 20})
            ((0, 20, 10), "UNIQUE(E1:I1, TRUE)", CellValue::Number(50.0), Some((1, 4, vec![
                CellValue::Number(50.0),
                CellValue::Number(20.0),
                CellValue::Number(40.0),
                CellValue::Number(10.0),
            ]))),
            // UNIQUE: exactly_once = TRUE (returns elements that appear exactly once)
            // Alice appears twice, Bob, Carol, Dave appear once
            ((0, 20, 15), "UNIQUE(A2:A6, FALSE, TRUE)", CellValue::Text("Bob".into()), Some((3, 1, vec![
                CellValue::Text("Bob".into()),
                CellValue::Text("Carol".into()),
                CellValue::Text("Dave".into()),
            ]))),
            // UNIQUE: exactly_once when no elements appear once yields #CALC!
            ((0, 20, 20), "UNIQUE({1; 1; 2; 2}, FALSE, TRUE)", CellValue::Error(CellError::Calc), None),

            // 4. FILTER: 1D filter (Score > 80)
            // B2:B5 > 80 is true for Alice(85), Bob(92), Carol(85)
            ((0, 25, 0), "FILTER(A2:A5, B2:B5 > 80)", CellValue::Text("Alice".into()), Some((3, 1, vec![
                CellValue::Text("Alice".into()),
                CellValue::Text("Bob".into()),
                CellValue::Text("Carol".into()),
            ]))),
            // FILTER: 2D table filter (Dept = "Tech")
            // rows 2 (Bob) and 4 (Dave)
            ((0, 25, 5), "FILTER(A2:B5, C2:C5 = \"Tech\")", CellValue::Text("Bob".into()), Some((2, 2, vec![
                CellValue::Text("Bob".into()), CellValue::Number(92.0),
                CellValue::Text("Dave".into()), CellValue::Number(70.0),
            ]))),
            // FILTER: horizontal vector filter (E1:I1 > 25 -> 50, 40)
            ((0, 25, 10), "FILTER(E1:I1, E1:I1 > 25)", CellValue::Number(50.0), Some((1, 2, vec![
                CellValue::Number(50.0),
                CellValue::Number(40.0),
            ]))),
            // FILTER: no matches with fallback value
            ((0, 25, 15), "FILTER(A2:A5, B2:B5 > 100, \"None\")", CellValue::Text("None".into()), None),
            // FILTER: no matches without fallback value yields #CALC!
            ((0, 25, 20), "FILTER(A2:A5, B2:B5 > 100)", CellValue::Error(CellError::Calc), None),
            // FILTER: criteria dimension mismatch yields #VALUE!
            ((0, 25, 25), "FILTER(A2:A5, B2:B4 > 80)", CellValue::Error(CellError::Value), None),

            // 5. SEQUENCE: basic vectors and matrices
            // 4x1 column vector
            ((0, 30, 0), "SEQUENCE(4)", CellValue::Number(1.0), Some((4, 1, vec![
                CellValue::Number(1.0),
                CellValue::Number(2.0),
                CellValue::Number(3.0),
                CellValue::Number(4.0),
            ]))),
            // 1x5 row vector
            ((0, 30, 5), "SEQUENCE(1, 5)", CellValue::Number(1.0), Some((1, 5, vec![
                CellValue::Number(1.0),
                CellValue::Number(2.0),
                CellValue::Number(3.0),
                CellValue::Number(4.0),
                CellValue::Number(5.0),
            ]))),
            // 2x3 matrix with custom start and step: start=10, step=5
            ((0, 30, 10), "SEQUENCE(2, 3, 10, 5)", CellValue::Number(10.0), Some((2, 3, vec![
                CellValue::Number(10.0), CellValue::Number(15.0), CellValue::Number(20.0),
                CellValue::Number(25.0), CellValue::Number(30.0), CellValue::Number(35.0),
            ]))),
            // 3x1 column vector with negative step
            ((0, 30, 15), "SEQUENCE(3, 1, 10, -2)", CellValue::Number(10.0), Some((3, 1, vec![
                CellValue::Number(10.0),
                CellValue::Number(8.0),
                CellValue::Number(6.0),
            ]))),
            // 2x2 matrix with fractional start and step
            ((0, 30, 20), "SEQUENCE(2, 2, 0.5, 0.5)", CellValue::Number(0.5), Some((2, 2, vec![
                CellValue::Number(0.5), CellValue::Number(1.0),
                CellValue::Number(1.5), CellValue::Number(2.0),
            ]))),
            // SEQUENCE: error cases (< 1 yields #VALUE!)
            ((0, 30, 25), "SEQUENCE(0, 5)", CellValue::Error(CellError::Value), None),
            ((0, 30, 30), "SEQUENCE(5, 0)", CellValue::Error(CellError::Value), None),
            ((0, 30, 35), "SEQUENCE(-1, 5)", CellValue::Error(CellError::Value), None),
            ((0, 30, 40), "SEQUENCE(5, -1)", CellValue::Error(CellError::Value), None),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected_val, expected_spill) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).expect(formula);
            assert_eq!(&actual.value, expected_val, "formula `{formula}` value mismatch");
            assert_eq!(&actual.spill, expected_spill, "formula `{formula}` spill mismatch");
        }
    }

    #[test]
    fn test_groupby_and_pivotby_functions() {
        let mut book = MockBook::new(false);
        book.dynamic = true;

        // Populate table data:
        // A1: "Dept", B1: "Region", C1: "Sales"
        // A2: "Sales", B2: "East", C2: 300.0
        // A3: "Sales", B3: "West", C3: 400.0
        // A4: "Tech",  B4: "East", C4: 100.0
        // A5: "Tech",  B5: "West", C5: 200.0
        book.values.insert((0, 0, 0), CellValue::Text("Dept".into()));
        book.values.insert((0, 0, 1), CellValue::Text("Region".into()));
        book.values.insert((0, 0, 2), CellValue::Text("Sales".into()));

        let rows = [
            ("Sales", "East", 300.0),
            ("Sales", "West", 400.0),
            ("Tech", "East", 100.0),
            ("Tech", "West", 200.0),
        ];
        for (i, &(dept, region, sales)) in rows.iter().enumerate() {
            let r = (i + 1) as u32;
            book.values.insert((0, r, 0), CellValue::Text(dept.into()));
            book.values.insert((0, r, 1), CellValue::Text(region.into()));
            book.values.insert((0, r, 2), CellValue::Number(sales));
        }

        let cases: &[((usize, u32, u32), &str, CellValue, Option<(usize, usize, Vec<CellValue>)>)] = &[
            // 1. GROUPBY default: single key, default totals at bottom (3x2 spill)
            ((0, 10, 0), "GROUPBY(A2:A5, C2:C5, SUM)", CellValue::Text("Sales".into()), Some((3, 2, vec![
                CellValue::Text("Sales".into()), CellValue::Number(700.0),
                CellValue::Text("Tech".into()), CellValue::Number(300.0),
                CellValue::Text("合計".into()), CellValue::Number(1000.0),
            ]))),

            // 2. GROUPBY without totals: headers=0, total_depth=0 (2x2 spill)
            ((0, 10, 5), "GROUPBY(A2:A5, C2:C5, SUM, 0, 0)", CellValue::Text("Sales".into()), Some((2, 2, vec![
                CellValue::Text("Sales".into()), CellValue::Number(700.0),
                CellValue::Text("Tech".into()), CellValue::Number(300.0),
            ]))),

            // 3. GROUPBY with totals at top: total_depth=-1 (3x2 spill)
            ((0, 10, 10), "GROUPBY(A2:A5, C2:C5, SUM, 0, -1)", CellValue::Text("合計".into()), Some((3, 2, vec![
                CellValue::Text("合計".into()), CellValue::Number(1000.0),
                CellValue::Text("Sales".into()), CellValue::Number(700.0),
                CellValue::Text("Tech".into()), CellValue::Number(300.0),
            ]))),

            // 4. GROUPBY sorted by value ascending (sort_order = 2 -> Tech 300, Sales 700)
            ((0, 10, 15), "GROUPBY(A2:A5, C2:C5, SUM, 0, 0, 2)", CellValue::Text("Tech".into()), Some((2, 2, vec![
                CellValue::Text("Tech".into()), CellValue::Number(300.0),
                CellValue::Text("Sales".into()), CellValue::Number(700.0),
            ]))),

            // 5. GROUPBY sorted by value descending (sort_order = -2 -> Sales 700, Tech 300)
            ((0, 10, 20), "GROUPBY(A2:A5, C2:C5, SUM, 0, 0, -2)", CellValue::Text("Sales".into()), Some((2, 2, vec![
                CellValue::Text("Sales".into()), CellValue::Number(700.0),
                CellValue::Text("Tech".into()), CellValue::Number(300.0),
            ]))),

            // 6. GROUPBY two keys with subtotals and grand total (total_depth = 2) (7x3 spill)
            ((0, 10, 25), "GROUPBY(A2:B5, C2:C5, SUM, 0, 2)", CellValue::Text("Sales".into()), Some((7, 3, vec![
                CellValue::Text("Sales".into()), CellValue::Text("East".into()), CellValue::Number(300.0),
                CellValue::Text("Sales".into()), CellValue::Text("West".into()), CellValue::Number(400.0),
                CellValue::Text("Sales".into()), CellValue::Text("".into()), CellValue::Number(700.0),
                CellValue::Text("Tech".into()), CellValue::Text("East".into()), CellValue::Number(100.0),
                CellValue::Text("Tech".into()), CellValue::Text("West".into()), CellValue::Number(200.0),
                CellValue::Text("Tech".into()), CellValue::Text("".into()), CellValue::Number(300.0),
                CellValue::Text("総計".into()), CellValue::Text("".into()), CellValue::Number(1000.0),
            ]))),

            // 7. GROUPBY with row filter: Sales > 150 (Tech East excluded)
            ((0, 10, 30), "GROUPBY(A2:A5, C2:C5, SUM, 0, 0, , C2:C5 > 150)", CellValue::Text("Sales".into()), Some((2, 2, vec![
                CellValue::Text("Sales".into()), CellValue::Number(700.0),
                CellValue::Text("Tech".into()), CellValue::Number(200.0),
            ]))),

            // 8. GROUPBY with AVERAGE function
            ((0, 10, 35), "GROUPBY(A2:A5, C2:C5, AVERAGE, 0, 0)", CellValue::Text("Sales".into()), Some((2, 2, vec![
                CellValue::Text("Sales".into()), CellValue::Number(350.0),
                CellValue::Text("Tech".into()), CellValue::Number(150.0),
            ]))),

            // 9. GROUPBY error cases
            ((0, 10, 40), "GROUPBY(A2:A4, C2:C5, SUM)", CellValue::Error(CellError::Value), None), // row mismatch
            ((0, 10, 45), "GROUPBY(A2:A5, C2:C5, SUM, 0, 2)", CellValue::Error(CellError::Value), None), // subtotals require >= 2 keys
            ((0, 10, 50), "GROUPBY(A2:A5, C2:C5, SUM, 0, 0, , C2:C5 > 1000)", CellValue::Error(CellError::Calc), None), // no filtered rows

            // 10. PIVOTBY default: row key Dept, col key Region, val Sales (4x4 spill)
            ((0, 20, 0), "PIVOTBY(A2:A5, B2:B5, C2:C5, SUM)", CellValue::Text("".into()), Some((4, 4, vec![
                CellValue::Text("".into()), CellValue::Text("East".into()), CellValue::Text("West".into()), CellValue::Text("合計".into()),
                CellValue::Text("Sales".into()), CellValue::Number(300.0), CellValue::Number(400.0), CellValue::Number(700.0),
                CellValue::Text("Tech".into()), CellValue::Number(100.0), CellValue::Number(200.0), CellValue::Number(300.0),
                CellValue::Text("合計".into()), CellValue::Number(400.0), CellValue::Number(600.0), CellValue::Number(1000.0),
            ]))),

            // 11. PIVOTBY without totals: row_depth=0, col_depth=0 (3x3 spill)
            ((0, 20, 5), "PIVOTBY(A2:A5, B2:B5, C2:C5, SUM, 0, 0, , 0)", CellValue::Text("".into()), Some((3, 3, vec![
                CellValue::Text("".into()), CellValue::Text("East".into()), CellValue::Text("West".into()),
                CellValue::Text("Sales".into()), CellValue::Number(300.0), CellValue::Number(400.0),
                CellValue::Text("Tech".into()), CellValue::Number(100.0), CellValue::Number(200.0),
            ]))),

            // 12. PIVOTBY with AVERAGE function without totals
            ((0, 20, 10), "PIVOTBY(A2:A5, B2:B5, C2:C5, AVERAGE, 0, 0, , 0)", CellValue::Text("".into()), Some((3, 3, vec![
                CellValue::Text("".into()), CellValue::Text("East".into()), CellValue::Text("West".into()),
                CellValue::Text("Sales".into()), CellValue::Number(300.0), CellValue::Number(400.0),
                CellValue::Text("Tech".into()), CellValue::Number(100.0), CellValue::Number(200.0),
            ]))),

            // 13. PIVOTBY error cases
            ((0, 20, 15), "PIVOTBY(A2:A4, B2:B5, C2:C5, SUM)", CellValue::Error(CellError::Value), None), // row key size mismatch
            ((0, 20, 20), "PIVOTBY(A2:A5, B2:B4, C2:C5, SUM)", CellValue::Error(CellError::Value), None), // col key size mismatch
            ((0, 20, 25), "PIVOTBY(A2:A5, B2:B5, A2:B5, SUM)", CellValue::Error(CellError::Value), None), // values has multiple columns
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected_val, expected_spill) in cases {
            let actual = res.iter().find(|r| r.cell == *cell).expect(formula);
            assert_eq!(&actual.value, expected_val, "formula `{formula}` value mismatch");
            assert_eq!(&actual.spill, expected_spill, "formula `{formula}` spill mismatch");
        }
    }

    #[test]
    fn test_date_and_workday_calculation_functions() {
        let mut book = MockBook::new(false);

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. WORKDAY: 5 business days forward from Mon 2024-01-01 -> Mon 2024-01-08 (45299)
            ((0, 10, 0), "WORKDAY(DATE(2024, 1, 1), 5)", CellValue::Number(45299.0)),
            // WORKDAY: 5 business days backward from Mon 2024-01-08 -> Mon 2024-01-01 (45292)
            ((0, 10, 1), "WORKDAY(DATE(2024, 1, 8), -5)", CellValue::Number(45292.0)),
            // WORKDAY: with holiday on Wed 2024-01-03 -> Tue 2024-01-09 (45300)
            ((0, 10, 2), "WORKDAY(DATE(2024, 1, 1), 5, DATE(2024, 1, 3))", CellValue::Number(45300.0)),
            // WORKDAY.INTL: weekend=11 (Sunday only holiday, Saturday is workday) -> Sat 2024-01-06 (45297)
            ((0, 10, 3), "WORKDAY.INTL(DATE(2024, 1, 1), 5, 11)", CellValue::Number(45297.0)),
            // WORKDAY.INTL: string weekend "0000011" (Sat+Sun) -> Mon 2024-01-08 (45299)
            ((0, 10, 4), "WORKDAY.INTL(DATE(2024, 1, 1), 5, \"0000011\")", CellValue::Number(45299.0)),
            // WORKDAY.INTL: string weekend "1111111" (all days weekend) -> #VALUE!
            ((0, 10, 5), "WORKDAY.INTL(DATE(2024, 1, 1), 5, \"1111111\")", CellValue::Error(CellError::Value)),
            // WORKDAY: boolean day argument rejected -> #VALUE!
            ((0, 10, 6), "WORKDAY(DATE(2024, 1, 1), TRUE)", CellValue::Error(CellError::Value)),
            // WORKDAY.INTL: invalid weekend number -> #NUM!
            ((0, 10, 7), "WORKDAY.INTL(DATE(2024, 1, 1), 5, 20)", CellValue::Error(CellError::Num)),

            // 2. NETWORKDAYS: 2 full weeks Mon 2024-01-01 to Sun 2024-01-14 -> 10 business days
            ((0, 15, 0), "NETWORKDAYS(DATE(2024, 1, 1), DATE(2024, 1, 14))", CellValue::Number(10.0)),
            // NETWORKDAYS: reverse start/end -> -10
            ((0, 15, 1), "NETWORKDAYS(DATE(2024, 1, 14), DATE(2024, 1, 1))", CellValue::Number(-10.0)),
            // NETWORKDAYS: with holiday on Mon 2024-01-08 -> 9
            ((0, 15, 2), "NETWORKDAYS(DATE(2024, 1, 1), DATE(2024, 1, 14), DATE(2024, 1, 8))", CellValue::Number(9.0)),
            // NETWORKDAYS.INTL: weekend=11 (Sunday only, 6 workdays/week) -> 12
            ((0, 15, 3), "NETWORKDAYS.INTL(DATE(2024, 1, 1), DATE(2024, 1, 14), 11)", CellValue::Number(12.0)),
            // NETWORKDAYS.INTL: all days weekend -> 0
            ((0, 15, 4), "NETWORKDAYS.INTL(DATE(2024, 1, 1), DATE(2024, 1, 14), \"1111111\")", CellValue::Number(0.0)),
            // NETWORKDAYS.INTL: invalid string length -> #VALUE!
            ((0, 15, 5), "NETWORKDAYS.INTL(DATE(2024, 1, 1), DATE(2024, 1, 14), \"000001\")", CellValue::Error(CellError::Value)),
            // NETWORKDAYS.INTL: invalid weekend number -> #NUM!
            ((0, 15, 6), "NETWORKDAYS.INTL(DATE(2024, 1, 1), DATE(2024, 1, 14), 99)", CellValue::Error(CellError::Num)),

            // 3. EDATE: month addition and clamp to end of month
            // 2024 is leap year: 2024-01-31 + 1 month -> 2024-02-29 (45351)
            ((0, 20, 0), "EDATE(DATE(2024, 1, 31), 1)", CellValue::Number(45351.0)),
            // 2023 is non-leap year: 2023-01-31 + 1 month -> 2023-02-28 (44985)
            ((0, 20, 1), "EDATE(DATE(2023, 1, 31), 1)", CellValue::Number(44985.0)),
            // 2024-03-31 - 1 month -> 2024-02-29 (45351)
            ((0, 20, 2), "EDATE(DATE(2024, 3, 31), -1)", CellValue::Number(45351.0)),
            // 2024-01-15 + 12 months -> 2025-01-15 (45672)
            ((0, 20, 3), "EDATE(DATE(2024, 1, 15), 12)", CellValue::Number(45672.0)),
            // EDATE error: logical argument rejected -> #VALUE!
            ((0, 20, 4), "EDATE(DATE(2024, 1, 1), TRUE)", CellValue::Error(CellError::Value)),

            // 4. EOMONTH: end of month calculation
            // Current month end: 2024-01-10 + 0 months -> 2024-01-31 (45322)
            ((0, 25, 0), "EOMONTH(DATE(2024, 1, 10), 0)", CellValue::Number(45322.0)),
            // Next month end in leap year: 2024-01-10 + 1 month -> 2024-02-29 (45351)
            ((0, 25, 1), "EOMONTH(DATE(2024, 1, 10), 1)", CellValue::Number(45351.0)),
            // Next month end in non-leap year: 2023-01-10 + 1 month -> 2023-02-28 (44985)
            ((0, 25, 2), "EOMONTH(DATE(2023, 1, 10), 1)", CellValue::Number(44985.0)),
            // 2024-05-10 - 2 months -> 2024-03-31 (45382)
            ((0, 25, 3), "EOMONTH(DATE(2024, 5, 10), -2)", CellValue::Number(45382.0)),
            // EOMONTH error: logical argument rejected -> #VALUE!
            ((0, 25, 4), "EOMONTH(DATE(2024, 1, 1), FALSE)", CellValue::Error(CellError::Value)),

            // 5. DATEDIF: interval calculation
            // Full years: 2020-01-15 to 2024-03-20 -> 4
            ((0, 30, 0), "DATEDIF(DATE(2020, 1, 15), DATE(2024, 3, 20), \"Y\")", CellValue::Number(4.0)),
            // Full months: 4 * 12 + 2 = 50
            ((0, 30, 1), "DATEDIF(DATE(2020, 1, 15), DATE(2024, 3, 20), \"M\")", CellValue::Number(50.0)),
            // Days: 14 days
            ((0, 30, 2), "DATEDIF(DATE(2024, 1, 1), DATE(2024, 1, 15), \"D\")", CellValue::Number(14.0)),
            // YM: months excluding years -> 2
            ((0, 30, 3), "DATEDIF(DATE(2020, 1, 15), DATE(2024, 3, 20), \"YM\")", CellValue::Number(2.0)),
            // MD: days excluding months and years (20 - 15) -> 5
            ((0, 30, 4), "DATEDIF(DATE(2020, 1, 15), DATE(2024, 3, 20), \"MD\")", CellValue::Number(5.0)),
            // DATEDIF error: start > end -> #NUM!
            ((0, 30, 5), "DATEDIF(DATE(2024, 1, 15), DATE(2020, 1, 15), \"Y\")", CellValue::Error(CellError::Num)),
            // DATEDIF error: invalid unit -> #NUM!
            ((0, 30, 6), "DATEDIF(DATE(2020, 1, 1), DATE(2024, 1, 1), \"INVALID\")", CellValue::Error(CellError::Num)),

            // 6. YEARFRAC: basis modes
            // Basis 0: 30/360 NASD -> 180 / 360 = 0.5
            ((0, 35, 0), "YEARFRAC(DATE(2024, 1, 1), DATE(2024, 7, 1), 0)", CellValue::Number(0.5)),
            // Basis 1: Actual/actual in leap year 2024 -> 366 / 366 = 1.0
            ((0, 35, 1), "YEARFRAC(DATE(2024, 1, 1), DATE(2025, 1, 1), 1)", CellValue::Number(1.0)),
            // YEARFRAC error: basis >= 5 -> #NUM!
            ((0, 35, 2), "YEARFRAC(DATE(2024, 1, 1), DATE(2024, 7, 1), 5)", CellValue::Error(CellError::Num)),
            // YEARFRAC error: basis < 0 -> #NUM!
            ((0, 35, 3), "YEARFRAC(DATE(2024, 1, 1), DATE(2024, 7, 1), -1)", CellValue::Error(CellError::Num)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_financial_loan_and_tvm_functions() {
        let mut book = MockBook::new(false);

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. PMT: regular end-of-period loan payment (5% annual, 24 months, 1,000,000 principal)
            ((0, 10, 0), "ROUND(PMT(0.05/12, 24, 1000000), 2)", CellValue::Number(-43871.39)),
            // PMT: beginning-of-period payment (type = 1)
            ((0, 10, 1), "ROUND(PMT(0.05/12, 24, 1000000, 0, 1), 2)", CellValue::Number(-43689.35)),
            // PMT: zero interest rate -> -pv / nper
            ((0, 10, 2), "ROUND(PMT(0, 24, 1000000), 2)", CellValue::Number(-41666.67)),
            // PMT error: nper = 0 -> #NUM!
            ((0, 10, 3), "PMT(0.05, 0, 1000)", CellValue::Error(CellError::Num)),
            // PMT error: rate <= -1 -> #NUM!
            ((0, 10, 4), "PMT(-1.5, 10, 1000)", CellValue::Error(CellError::Num)),

            // 2. PPMT & IPMT: principal and interest payments
            ((0, 15, 0), "ROUND(IPMT(0.05/12, 1, 24, 1000000), 2)", CellValue::Number(-4166.67)),
            ((0, 15, 1), "ROUND(PPMT(0.05/12, 1, 24, 1000000), 2)", CellValue::Number(-39704.72)),
            // IPMT: beginning-of-period payment has 0 interest in period 1
            ((0, 15, 2), "ROUND(IPMT(0.05/12, 1, 24, 1000000, 0, 1), 2)", CellValue::Number(0.0)),
            // PPMT & IPMT at rate = 0
            ((0, 15, 3), "ROUND(PPMT(0, 5, 24, 1000000), 2)", CellValue::Number(-41666.67)),
            ((0, 15, 4), "IPMT(0, 5, 24, 1000000)", CellValue::Number(0.0)),
            // Invariant: PPMT + IPMT == PMT
            ((0, 15, 5), "ROUND(PPMT(0.05/12, 12, 24, 1000000) + IPMT(0.05/12, 12, 24, 1000000), 2) = ROUND(PMT(0.05/12, 24, 1000000), 2)", CellValue::Bool(true)),
            // Errors: per < 1 -> #NUM!, per > nper -> #NUM!
            ((0, 15, 6), "IPMT(0.05, 0, 24, 1000)", CellValue::Error(CellError::Num)),
            ((0, 15, 7), "PPMT(0.05, 25, 24, 1000)", CellValue::Error(CellError::Num)),

            // 3. PV: present value
            ((0, 20, 0), "ROUND(PV(0.05/12, 24, -43871.38973407), 2)", CellValue::Number(1000000.0)),
            ((0, 20, 1), "ROUND(PV(0, 24, -41666.66666667), 2)", CellValue::Number(1000000.0)),

            // 4. FV: future value
            ((0, 25, 0), "ROUND(FV(0.05/12, 24, -43871.38973407), 2)", CellValue::Number(1104941.34)),
            // Fully amortized loan has FV = 0
            ((0, 25, 1), "ROUND(FV(0.05/12, 24, -43871.38973407, 1000000), 2)", CellValue::Number(0.0)),
            ((0, 25, 2), "ROUND(FV(0, 24, -1000, 20000), 2)", CellValue::Number(4000.0)),

            // 5. NPER: number of periods
            ((0, 30, 0), "ROUND(NPER(0.05/12, -43871.38973407, 1000000), 4)", CellValue::Number(24.0)),
            ((0, 30, 1), "ROUND(NPER(0, -50000, 1000000), 4)", CellValue::Number(20.0)),
            ((0, 30, 2), "NPER(0, 0, 1000)", CellValue::Error(CellError::Num)),

            // 6. RATE: interest rate solver
            ((0, 35, 0), "ROUND(RATE(24, -43871.38973407, 1000000) * 12, 4)", CellValue::Number(0.05)),
            ((0, 35, 1), "RATE(0, -100, 1000)", CellValue::Error(CellError::Num)),

            // 7. CUMPRINC & CUMIPMT: cumulative principal and interest
            // Total principal paid across all 24 periods equals full loan amount
            ((0, 40, 0), "ROUND(CUMPRINC(0.05/12, 24, 1000000, 1, 24, 0), 2)", CellValue::Number(-1000000.0)),
            // Errors: start > end -> #NUM!, start < 1 -> #NUM!, end > nper -> #NUM!, rate <= 0 -> #NUM!
            ((0, 40, 1), "CUMPRINC(0.05, 24, 1000, 5, 2, 0)", CellValue::Error(CellError::Num)),
            ((0, 40, 2), "CUMIPMT(0.05, 24, 1000, 0, 5, 0)", CellValue::Error(CellError::Num)),
            ((0, 40, 3), "CUMIPMT(0.05, 24, 1000, 1, 25, 0)", CellValue::Error(CellError::Num)),
            ((0, 40, 4), "CUMIPMT(-0.01, 24, 1000, 1, 5, 0)", CellValue::Error(CellError::Num)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_trigonometric_hyperbolic_and_distribution_edge_cases() {
        let mut book = MockBook::new(false);

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Angular constants and conversions
            ((0, 50, 0), "ROUND(PI(), 6)", CellValue::Number(3.141593)),
            ((0, 50, 1), "ROUND(RADIANS(180), 8) = ROUND(PI(), 8)", CellValue::Bool(true)),
            ((0, 50, 2), "RADIANS(0)", CellValue::Number(0.0)),
            ((0, 50, 3), "ROUND(RADIANS(-180), 8) = ROUND(-PI(), 8)", CellValue::Bool(true)),
            ((0, 50, 4), "ROUND(DEGREES(PI()), 6)", CellValue::Number(180.0)),
            ((0, 50, 5), "DEGREES(0)", CellValue::Number(0.0)),
            ((0, 50, 6), "ROUND(DEGREES(-PI()), 6)", CellValue::Number(-180.0)),

            // 2. Trigonometric functions & edge points
            ((0, 55, 0), "SIN(0)", CellValue::Number(0.0)),
            ((0, 55, 1), "ROUND(SIN(PI()), 10)", CellValue::Number(0.0)),
            ((0, 55, 2), "ROUND(SIN(PI()/2), 8)", CellValue::Number(1.0)),
            ((0, 55, 3), "ROUND(SIN(-PI()/2), 8)", CellValue::Number(-1.0)),
            ((0, 55, 4), "COS(0)", CellValue::Number(1.0)),
            ((0, 55, 5), "ROUND(COS(PI()/2), 10)", CellValue::Number(0.0)),
            ((0, 55, 6), "ROUND(COS(PI()), 8)", CellValue::Number(-1.0)),
            ((0, 55, 7), "ROUND(COS(-PI()), 8)", CellValue::Number(-1.0)),
            ((0, 55, 8), "TAN(0)", CellValue::Number(0.0)),
            ((0, 55, 9), "ROUND(TAN(PI()), 10)", CellValue::Number(0.0)),
            ((0, 55, 10), "ROUND(TAN(PI()/4), 8)", CellValue::Number(1.0)),

            // 3. Inverse trigonometric functions & domain errors
            ((0, 60, 0), "ASIN(0)", CellValue::Number(0.0)),
            ((0, 60, 1), "ROUND(ASIN(1), 8) = ROUND(PI()/2, 8)", CellValue::Bool(true)),
            ((0, 60, 2), "ROUND(ASIN(-1), 8) = ROUND(-PI()/2, 8)", CellValue::Bool(true)),
            ((0, 60, 3), "ASIN(1.0001)", CellValue::Error(CellError::Num)),
            ((0, 60, 4), "ASIN(-1.5)", CellValue::Error(CellError::Num)),
            ((0, 60, 5), "ACOS(1)", CellValue::Number(0.0)),
            ((0, 60, 6), "ROUND(ACOS(0), 8) = ROUND(PI()/2, 8)", CellValue::Bool(true)),
            ((0, 60, 7), "ROUND(ACOS(-1), 8) = ROUND(PI(), 8)", CellValue::Bool(true)),
            ((0, 60, 8), "ACOS(1.01)", CellValue::Error(CellError::Num)),
            ((0, 60, 9), "ACOS(-2)", CellValue::Error(CellError::Num)),
            ((0, 60, 10), "ATAN(0)", CellValue::Number(0.0)),
            ((0, 60, 11), "ROUND(ATAN(1), 8) = ROUND(PI()/4, 8)", CellValue::Bool(true)),
            ((0, 60, 12), "ROUND(ATAN(-1), 8) = ROUND(-PI()/4, 8)", CellValue::Bool(true)),

            // 4. ATAN2 quadrant selection & (0, 0) singularity
            ((0, 65, 0), "ROUND(ATAN2(1, 1), 8) = ROUND(PI()/4, 8)", CellValue::Bool(true)),
            ((0, 65, 1), "ROUND(ATAN2(-1, 1), 8) = ROUND(3*PI()/4, 8)", CellValue::Bool(true)),
            ((0, 65, 2), "ROUND(ATAN2(-1, -1), 8) = ROUND(-3*PI()/4, 8)", CellValue::Bool(true)),
            ((0, 65, 3), "ROUND(ATAN2(1, -1), 8) = ROUND(-PI()/4, 8)", CellValue::Bool(true)),
            ((0, 65, 4), "ATAN2(0, 0)", CellValue::Error(CellError::Div0)),

            // 5. Reciprocal trigonometric functions (COT, ACOT, CSC, SEC)
            ((0, 70, 0), "ROUND(COT(PI()/4), 8)", CellValue::Number(1.0)),
            ((0, 70, 1), "COT(0)", CellValue::Error(CellError::Div0)),
            ((0, 70, 2), "ROUND(ACOT(0), 8) = ROUND(PI()/2, 8)", CellValue::Bool(true)),
            ((0, 70, 3), "ROUND(ACOT(1), 8) = ROUND(PI()/4, 8)", CellValue::Bool(true)),
            ((0, 70, 4), "ROUND(ACOT(-1), 8) = ROUND(3*PI()/4, 8)", CellValue::Bool(true)),
            ((0, 70, 5), "SEC(0)", CellValue::Number(1.0)),
            ((0, 70, 6), "CSC(0)", CellValue::Error(CellError::Div0)),

            // 6. Hyperbolic & inverse hyperbolic functions
            ((0, 75, 0), "SINH(0)", CellValue::Number(0.0)),
            ((0, 75, 1), "ROUND(SINH(1) + SINH(-1), 10)", CellValue::Number(0.0)),
            ((0, 75, 2), "COSH(0)", CellValue::Number(1.0)),
            ((0, 75, 3), "ROUND(COSH(1) - COSH(-1), 10)", CellValue::Number(0.0)),
            ((0, 75, 4), "ROUND(COSH(1.5)^2 - SINH(1.5)^2, 10)", CellValue::Number(1.0)),
            ((0, 75, 5), "TANH(0)", CellValue::Number(0.0)),
            ((0, 75, 6), "ASINH(0)", CellValue::Number(0.0)),
            ((0, 75, 7), "ROUND(ASINH(2) + ASINH(-2), 10)", CellValue::Number(0.0)),
            ((0, 75, 8), "ACOSH(1)", CellValue::Number(0.0)),
            ((0, 75, 9), "ACOSH(0.999)", CellValue::Error(CellError::Num)),
            ((0, 75, 10), "ACOSH(-1)", CellValue::Error(CellError::Num)),
            ((0, 75, 11), "ATANH(0)", CellValue::Number(0.0)),
            ((0, 75, 12), "ATANH(1)", CellValue::Error(CellError::Num)),
            ((0, 75, 13), "ATANH(-1)", CellValue::Error(CellError::Num)),
            ((0, 75, 14), "ATANH(1.5)", CellValue::Error(CellError::Num)),
            ((0, 75, 15), "COTH(0)", CellValue::Error(CellError::Div0)),
            ((0, 75, 16), "CSCH(0)", CellValue::Error(CellError::Div0)),
            ((0, 75, 17), "SECH(0)", CellValue::Number(1.0)),
            ((0, 75, 18), "ACOTH(1)", CellValue::Error(CellError::Num)),
            ((0, 75, 19), "ACOTH(-1)", CellValue::Error(CellError::Num)),
            ((0, 75, 20), "ACOTH(0.5)", CellValue::Error(CellError::Num)),

            // 7. Exponential & logarithmic functions
            ((0, 80, 0), "EXP(0)", CellValue::Number(1.0)),
            ((0, 80, 1), "LN(1)", CellValue::Number(0.0)),
            ((0, 80, 2), "ROUND(LN(EXP(1)), 8)", CellValue::Number(1.0)),
            ((0, 80, 3), "LN(0)", CellValue::Error(CellError::Num)),
            ((0, 80, 4), "LN(-1)", CellValue::Error(CellError::Num)),
            ((0, 80, 5), "LOG10(1)", CellValue::Number(0.0)),
            ((0, 80, 6), "LOG10(100)", CellValue::Number(2.0)),
            ((0, 80, 7), "LOG10(0)", CellValue::Error(CellError::Num)),
            ((0, 80, 8), "LOG10(-5)", CellValue::Error(CellError::Num)),
            ((0, 80, 9), "LOG(8, 2)", CellValue::Number(3.0)),
            ((0, 80, 10), "LOG(1000)", CellValue::Number(3.0)),
            ((0, 80, 11), "LOG(10, 1)", CellValue::Error(CellError::Div0)),
            ((0, 80, 12), "LOG(0, 2)", CellValue::Error(CellError::Num)),
            ((0, 80, 13), "LOG(-10, 2)", CellValue::Error(CellError::Num)),
            ((0, 80, 14), "LOG(10, 0)", CellValue::Error(CellError::Num)),
            ((0, 80, 15), "LOG(10, -2)", CellValue::Error(CellError::Num)),

            // 8. Statistical & distribution edge cases (STANDARDIZE, NORM.DIST, NORM.INV, CONFIDENCE)
            ((0, 85, 0), "STANDARDIZE(10, 10, 2)", CellValue::Number(0.0)),
            ((0, 85, 1), "STANDARDIZE(14, 10, 2)", CellValue::Number(2.0)),
            ((0, 85, 2), "STANDARDIZE(6, 10, 2)", CellValue::Number(-2.0)),
            ((0, 85, 3), "STANDARDIZE(10, 10, 0)", CellValue::Error(CellError::Num)),
            ((0, 85, 4), "STANDARDIZE(10, 10, -2)", CellValue::Error(CellError::Num)),
            ((0, 85, 5), "ROUND(NORM.DIST(10, 10, 2, TRUE), 4)", CellValue::Number(0.5)),
            ((0, 85, 6), "ROUND(NORM.DIST(10, 10, 2, FALSE), 4)", CellValue::Number(0.1995)),
            ((0, 85, 7), "NORM.DIST(10, 10, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 85, 8), "NORM.DIST(10, 10, -1, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 85, 9), "ROUND(NORM.S.DIST(0, TRUE), 4)", CellValue::Number(0.5)),
            ((0, 85, 10), "ROUND(NORM.S.DIST(0, FALSE), 4)", CellValue::Number(0.3989)),
            ((0, 85, 11), "ROUND(NORM.INV(0.5, 10, 2), 4)", CellValue::Number(10.0)),
            ((0, 85, 12), "NORM.INV(0, 10, 2)", CellValue::Error(CellError::Num)),
            ((0, 85, 13), "NORM.INV(1, 10, 2)", CellValue::Error(CellError::Num)),
            ((0, 85, 14), "NORM.INV(-0.2, 10, 2)", CellValue::Error(CellError::Num)),
            ((0, 85, 15), "NORM.INV(1.2, 10, 2)", CellValue::Error(CellError::Num)),
            ((0, 85, 16), "NORM.INV(0.5, 10, 0)", CellValue::Error(CellError::Num)),
            ((0, 85, 17), "ROUND(NORM.S.INV(0.5), 4)", CellValue::Number(0.0)),
            ((0, 85, 18), "ROUND(NORM.S.INV(0.975), 2)", CellValue::Number(1.96)),
            ((0, 85, 19), "NORM.S.INV(0)", CellValue::Error(CellError::Num)),
            ((0, 85, 20), "NORM.S.INV(1)", CellValue::Error(CellError::Num)),
            ((0, 85, 21), "ROUND(CONFIDENCE.NORM(0.05, 2.5, 50), 4)", CellValue::Number(0.693)),
            ((0, 85, 22), "CONFIDENCE.NORM(0, 2.5, 50)", CellValue::Error(CellError::Num)),
            ((0, 85, 23), "CONFIDENCE.NORM(1, 2.5, 50)", CellValue::Error(CellError::Num)),
            ((0, 85, 24), "CONFIDENCE.NORM(0.05, 0, 50)", CellValue::Error(CellError::Num)),
            ((0, 85, 25), "CONFIDENCE.NORM(0.05, 2.5, 0)", CellValue::Error(CellError::Num)),
            ((0, 85, 26), "ROUND(CONFIDENCE.T(0.05, 2.5, 50), 4)", CellValue::Number(0.7105)),
            ((0, 85, 27), "CONFIDENCE.T(0, 2.5, 50)", CellValue::Error(CellError::Num)),
            ((0, 85, 28), "CONFIDENCE.T(1, 2.5, 50)", CellValue::Error(CellError::Num)),
            ((0, 85, 29), "CONFIDENCE.T(0.05, 0, 50)", CellValue::Error(CellError::Num)),
            ((0, 85, 30), "CONFIDENCE.T(0.05, 2.5, 1)", CellValue::Error(CellError::Div0)),
            ((0, 85, 31), "CONFIDENCE.T(0.05, 2.5, 0)", CellValue::Error(CellError::Num)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_text_and_string_manipulation_edge_cases() {
        let mut book = MockBook::new(false);

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Length & substring extraction (LEN, LENB, LEFT, RIGHT, MID)
            ((0, 90, 0), "LEN(\"\")", CellValue::Number(0.0)),
            ((0, 90, 1), "LEN(\"Hello\")", CellValue::Number(5.0)),
            ((0, 90, 2), "LEN(\"日本語\")", CellValue::Number(3.0)),
            ((0, 90, 3), "LEN(\"😀\")", CellValue::Number(2.0)), // Surrogate pair = 2 UTF-16 code units
            ((0, 90, 4), "LENB(\"ABC\")", CellValue::Number(3.0)),
            ((0, 90, 5), "LENB(\"アイウ\")", CellValue::Number(6.0)),
            ((0, 90, 6), "LENB(\"ｱｲｳ\")", CellValue::Number(3.0)), // Half-width katakana = 1 byte each

            ((0, 95, 0), "LEFT(\"ABCDE\", 3)", CellValue::Text("ABC".into())),
            ((0, 95, 1), "LEFT(\"ABCDE\")", CellValue::Text("A".into())),
            ((0, 95, 2), "LEFT(\"ABCDE\", 0)", CellValue::Text("".into())),
            ((0, 95, 3), "LEFT(\"ABCDE\", 10)", CellValue::Text("ABCDE".into())),
            ((0, 95, 4), "LEFT(\"ABCDE\", -1)", CellValue::Error(CellError::Value)),

            ((0, 100, 0), "RIGHT(\"ABCDE\", 2)", CellValue::Text("DE".into())),
            ((0, 100, 1), "RIGHT(\"ABCDE\")", CellValue::Text("E".into())),
            ((0, 100, 2), "RIGHT(\"ABCDE\", 0)", CellValue::Text("".into())),
            ((0, 100, 3), "RIGHT(\"ABCDE\", 10)", CellValue::Text("ABCDE".into())),
            ((0, 100, 4), "RIGHT(\"ABCDE\", -2)", CellValue::Error(CellError::Value)),

            ((0, 105, 0), "MID(\"ABCDE\", 2, 3)", CellValue::Text("BCD".into())),
            ((0, 105, 1), "MID(\"ABCDE\", 1, 1)", CellValue::Text("A".into())),
            ((0, 105, 2), "MID(\"ABCDE\", 6, 2)", CellValue::Text("".into())),
            ((0, 105, 3), "MID(\"ABCDE\", 0, 2)", CellValue::Error(CellError::Value)),
            ((0, 105, 4), "MID(\"ABCDE\", -1, 2)", CellValue::Error(CellError::Value)),
            ((0, 105, 5), "MID(\"ABCDE\", 2, -1)", CellValue::Error(CellError::Value)),
            ((0, 105, 6), "MID(\"\", 1, 5)", CellValue::Text("".into())),
            ((0, 105, 7), "MID(\"東京都千代田区\", 4, 3)", CellValue::Text("千代田".into())),

            // 2. Search & pattern matching (FIND, SEARCH)
            ((0, 110, 0), "FIND(\"c\", \"abcABC\")", CellValue::Number(3.0)),
            ((0, 110, 1), "FIND(\"C\", \"abcABC\")", CellValue::Number(6.0)),
            ((0, 110, 2), "FIND(\"a\", \"banana\", 3)", CellValue::Number(4.0)),
            ((0, 110, 3), "FIND(\"z\", \"abc\")", CellValue::Error(CellError::Value)),
            ((0, 110, 4), "FIND(\"a\", \"abc\", 0)", CellValue::Error(CellError::Value)),
            ((0, 110, 5), "FIND(\"a\", \"abc\", 5)", CellValue::Error(CellError::Value)),
            ((0, 110, 6), "FIND(\"\", \"abc\")", CellValue::Number(1.0)),
            ((0, 110, 7), "FIND(\"\", \"abc\", 2)", CellValue::Number(2.0)),

            ((0, 115, 0), "SEARCH(\"c\", \"abcABC\")", CellValue::Number(3.0)),
            ((0, 115, 1), "SEARCH(\"C\", \"abcABC\")", CellValue::Number(3.0)),
            ((0, 115, 2), "SEARCH(\"b?t\", \"batman\")", CellValue::Number(1.0)),
            ((0, 115, 3), "SEARCH(\"b*t\", \"basket\")", CellValue::Number(1.0)),
            ((0, 115, 4), "SEARCH(\"~?\", \"what?\")", CellValue::Number(5.0)),
            ((0, 115, 5), "SEARCH(\"xyz\", \"abcdef\")", CellValue::Error(CellError::Value)),

            // 3. String replacement & cleanup (REPLACE, SUBSTITUTE, TRIM, PROPER)
            ((0, 120, 0), "REPLACE(\"ABCDE\", 2, 3, \"123\")", CellValue::Text("A123E".into())),
            ((0, 120, 1), "REPLACE(\"ABCDE\", 2, 0, \"123\")", CellValue::Text("A123BCDE".into())),
            ((0, 120, 2), "REPLACE(\"ABCDE\", 2, 3, \"\")", CellValue::Text("AE".into())),
            ((0, 120, 3), "REPLACE(\"ABCDE\", 0, 2, \"X\")", CellValue::Error(CellError::Value)),
            ((0, 120, 4), "REPLACE(\"ABCDE\", 2, -1, \"X\")", CellValue::Error(CellError::Value)),

            ((0, 125, 0), "SUBSTITUTE(\"banana\", \"a\", \"o\")", CellValue::Text("bonono".into())),
            ((0, 125, 1), "SUBSTITUTE(\"banana\", \"a\", \"o\", 2)", CellValue::Text("banona".into())),
            ((0, 125, 2), "SUBSTITUTE(\"Apple apple\", \"apple\", \"orange\")", CellValue::Text("Apple orange".into())),
            ((0, 125, 3), "SUBSTITUTE(\"banana\", \"z\", \"o\")", CellValue::Text("banana".into())),
            ((0, 125, 4), "SUBSTITUTE(\"banana\", \"a\", \"o\", 0)", CellValue::Error(CellError::Value)),
            ((0, 125, 5), "SUBSTITUTE(\"banana\", \"a\", \"o\", -1)", CellValue::Error(CellError::Value)),

            ((0, 130, 0), "TRIM(\"  Hello   World  \")", CellValue::Text("Hello World".into())),
            ((0, 130, 1), "TRIM(\"    \")", CellValue::Text("".into())),
            ((0, 130, 2), "PROPER(\"hello world\")", CellValue::Text("Hello World".into())),
            ((0, 130, 3), "PROPER(\"HELLO WORLD\")", CellValue::Text("Hello World".into())),
            ((0, 130, 4), "PROPER(\"123hello-world\")", CellValue::Text("123Hello-World".into())),

            // 4. Joining & repetition (CONCAT, TEXTJOIN, REPT)
            ((0, 135, 0), "CONCAT(\"A\", 1, \"B\", 2)", CellValue::Text("A1B2".into())),
            ((0, 135, 1), "CONCAT(\"A\", \"\", \"B\")", CellValue::Text("AB".into())),
            ((0, 135, 2), "TEXTJOIN(\", \", TRUE, \"apple\", \"\", \"banana\", \"cherry\")", CellValue::Text("apple, banana, cherry".into())),
            ((0, 135, 3), "TEXTJOIN(\", \", FALSE, \"apple\", \"\", \"banana\")", CellValue::Text("apple, , banana".into())),
            ((0, 135, 4), "TEXTJOIN(\"\", TRUE, \"A\", \"B\", \"C\")", CellValue::Text("ABC".into())),
            ((0, 135, 5), "REPT(\"*-\", 3)", CellValue::Text("*-*-*-".into())),
            ((0, 135, 6), "REPT(\"abc\", 0)", CellValue::Text("".into())),
            ((0, 135, 7), "REPT(\"abc\", -1)", CellValue::Error(CellError::Value)),

            // 5. Modern text functions (TEXTBEFORE, TEXTAFTER, VALUETOTEXT, ARRAYTOTEXT, TEXTSPLIT)
            ((0, 140, 0), "TEXTBEFORE(\"Red-Green-Blue\", \"-\")", CellValue::Text("Red".into())),
            ((0, 140, 1), "TEXTBEFORE(\"Red-Green-Blue\", \"-\", 2)", CellValue::Text("Red-Green".into())),
            ((0, 140, 2), "TEXTBEFORE(\"Red-Green-Blue\", \"-\", -1)", CellValue::Text("Red-Green".into())),
            ((0, 140, 3), "TEXTBEFORE(\"Red-Green-Blue\", \"green\", 1, 1)", CellValue::Text("Red-".into())),
            ((0, 140, 4), "TEXTBEFORE(\"Red-Green-Blue\", \"Yellow\")", CellValue::Error(CellError::NA)),
            ((0, 140, 5), "TEXTBEFORE(\"Red-Green-Blue\", \"Yellow\", 1, 0, 0, \"Missing\")", CellValue::Text("Missing".into())),

            ((0, 145, 0), "TEXTAFTER(\"Red-Green-Blue\", \"-\")", CellValue::Text("Green-Blue".into())),
            ((0, 145, 1), "TEXTAFTER(\"Red-Green-Blue\", \"-\", 2)", CellValue::Text("Blue".into())),
            ((0, 145, 2), "TEXTAFTER(\"Red-Green-Blue\", \"-\", -1)", CellValue::Text("Blue".into())),
            ((0, 145, 3), "TEXTAFTER(\"Red-Green-Blue\", \"Yellow\", 1, 0, 0, \"Missing\")", CellValue::Text("Missing".into())),

            ((0, 150, 0), "VALUETOTEXT(12345, 0)", CellValue::Text("12345".into())),
            ((0, 150, 1), "VALUETOTEXT(\"Hello\", 0)", CellValue::Text("Hello".into())),
            ((0, 150, 2), "VALUETOTEXT(TRUE, 0)", CellValue::Text("TRUE".into())),
            ((0, 150, 3), "VALUETOTEXT(\"Hello\", 1)", CellValue::Text("\"Hello\"".into())),
            ((0, 150, 4), "VALUETOTEXT(123, 1)", CellValue::Text("123".into())),
            ((0, 150, 5), "VALUETOTEXT(\"Hello\", 2)", CellValue::Error(CellError::Value)),

            ((0, 155, 0), "ARRAYTOTEXT({\"A\", \"B\"; \"C\", \"D\"}, 0)", CellValue::Text("A, B, C, D".into())),
            ((0, 155, 1), "ARRAYTOTEXT({\"A\", \"B\"; \"C\", \"D\"}, 1)", CellValue::Text("{\"A\",\"B\";\"C\",\"D\"}".into())),
            ((0, 155, 2), "ARRAYTOTEXT(TEXTSPLIT(\"alpha,beta,gamma\", \",\"), 0)", CellValue::Text("alpha, beta, gamma".into())),

            // 6. Character codes and exact matching (CHAR, CODE, UNICHAR, UNICODE, EXACT)
            ((0, 160, 0), "CHAR(65)", CellValue::Text("A".into())),
            ((0, 160, 1), "CODE(\"A\")", CellValue::Number(65.0)),
            ((0, 160, 2), "CHAR(0)", CellValue::Error(CellError::Value)),
            ((0, 160, 3), "UNICHAR(9731)", CellValue::Text("☃".into())),
            ((0, 160, 4), "UNICODE(\"☃\")", CellValue::Number(9731.0)),
            ((0, 160, 5), "UNICHAR(0)", CellValue::Error(CellError::Value)),
            ((0, 160, 6), "EXACT(\"word\", \"word\")", CellValue::Bool(true)),
            ((0, 160, 7), "EXACT(\"word\", \"Word\")", CellValue::Bool(false)),
            ((0, 160, 8), "EXACT(\"123\", 123)", CellValue::Bool(true)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_date_and_time_edge_cases() {
        let mut book = MockBook::new(false);

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. DATE constructor boundaries and month/day rollovers
            ((0, 200, 0), "DATE(2024, 1, 15)", CellValue::Number(45306.0)),
            ((0, 200, 1), "DATE(0, 1, 1)", CellValue::Number(1.0)), // Year 0 -> 1900
            ((0, 200, 2), "DATE(100, 1, 1)", CellValue::Number(36526.0)), // Year 100 -> 2000
            ((0, 200, 3), "DATE(1900, 1, 0)", CellValue::Number(0.0)), // Serial 0
            ((0, 200, 15), "DATE(1900, 0, 31)", CellValue::Number(0.0)), // Month underflow to 1899-12-31 = serial 0
            ((0, 200, 16), "DATE(1899, 12, 31)", CellValue::Number(693962.0)), // Years 0..1899 map to +1900 (3799-12-31)
            ((0, 200, 4), "DATE(2023, 13, 1)", CellValue::Number(45292.0)), // 2024-01-01
            ((0, 200, 5), "DATE(2024, 0, 1)", CellValue::Number(45261.0)), // 2023-12-01
            ((0, 200, 6), "DATE(2024, -1, 1)", CellValue::Number(45231.0)), // 2023-11-01
            ((0, 200, 7), "DATE(2024, 1, 32)", CellValue::Number(45323.0)), // 2024-02-01
            ((0, 200, 8), "DATE(2024, 2, 0)", CellValue::Number(45322.0)), // 2024-01-31
            ((0, 200, 9), "DATE(2024, 2, 29)", CellValue::Number(45351.0)), // Leap year leap day
            ((0, 200, 10), "DATE(2023, 2, 29)", CellValue::Number(44986.0)), // 2023-03-01
            ((0, 200, 11), "DATE(2023, 3, 0)", CellValue::Number(44985.0)), // 2023-02-28
            ((0, 200, 12), "DATE(9999, 12, 31)", CellValue::Number(2958465.0)), // MAX_SERIAL
            ((0, 200, 13), "DATE(10000, 1, 1)", CellValue::Error(CellError::Num)),
            ((0, 200, 14), "DATE(-1, 1, 1)", CellValue::Error(CellError::Num)),

            // 2. TIME constructor overflow, wrapping, and limits
            ((0, 205, 0), "TIME(0, 0, 0)", CellValue::Number(0.0)),
            ((0, 205, 1), "TIME(12, 0, 0)", CellValue::Number(0.5)),
            ((0, 205, 2), "TIME(6, 0, 0)", CellValue::Number(0.25)),
            ((0, 205, 3), "TIME(18, 0, 0)", CellValue::Number(0.75)),
            ((0, 205, 4), "TIME(24, 0, 0)", CellValue::Number(0.0)),
            ((0, 205, 5), "TIME(25, 0, 0)", CellValue::Number(1.0 / 24.0)),
            ((0, 205, 6), "TIME(0, 60, 0)", CellValue::Number(1.0 / 24.0)),
            ((0, 205, 7), "TIME(0, 120, 0)", CellValue::Number(2.0 / 24.0)),
            ((0, 205, 8), "TIME(0, 0, 3600)", CellValue::Number(1.0 / 24.0)),
            ((0, 205, 9), "TIME(32767, 0, 0)", CellValue::Number(7.0 / 24.0)),
            ((0, 205, 10), "TIME(32768, 0, 0)", CellValue::Error(CellError::Num)),
            ((0, 205, 11), "TIME(0, 32768, 0)", CellValue::Error(CellError::Num)),
            ((0, 205, 12), "TIME(0, 0, 32768)", CellValue::Error(CellError::Num)),
            ((0, 205, 13), "TIME(-1, 0, 0)", CellValue::Error(CellError::Num)),
            ((0, 205, 14), "TIME(0, -1, 0)", CellValue::Error(CellError::Num)),

            // 3. Component extractors (YEAR, MONTH, DAY, HOUR, MINUTE, SECOND)
            ((0, 210, 0), "YEAR(0)", CellValue::Number(1900.0)),
            ((0, 210, 1), "MONTH(0)", CellValue::Number(1.0)),
            ((0, 210, 2), "DAY(0)", CellValue::Number(0.0)),
            ((0, 210, 3), "YEAR(60)", CellValue::Number(1900.0)), // Excel 1900 leap day bug
            ((0, 210, 4), "MONTH(60)", CellValue::Number(2.0)),
            ((0, 210, 5), "DAY(60)", CellValue::Number(29.0)),
            ((0, 210, 6), "YEAR(61)", CellValue::Number(1900.0)),
            ((0, 210, 7), "MONTH(61)", CellValue::Number(3.0)),
            ((0, 210, 8), "DAY(61)", CellValue::Number(1.0)),
            ((0, 210, 9), "YEAR(45306)", CellValue::Number(2024.0)),
            ((0, 210, 10), "MONTH(45306)", CellValue::Number(1.0)),
            ((0, 210, 11), "DAY(45306)", CellValue::Number(15.0)),
            ((0, 210, 12), "YEAR(-1)", CellValue::Error(CellError::Num)),

            ((0, 215, 0), "HOUR(0.0)", CellValue::Number(0.0)),
            ((0, 215, 1), "HOUR(0.5)", CellValue::Number(12.0)),
            ((0, 215, 2), "HOUR(0.75)", CellValue::Number(18.0)),
            ((0, 215, 3), "HOUR(45306.75)", CellValue::Number(18.0)),
            ((0, 215, 4), "MINUTE(0.0)", CellValue::Number(0.0)),
            ((0, 215, 5), "MINUTE(TIME(12, 30, 45))", CellValue::Number(30.0)),
            ((0, 215, 6), "SECOND(TIME(12, 30, 45))", CellValue::Number(45.0)),
            ((0, 215, 7), "HOUR(-0.5)", CellValue::Error(CellError::Num)),
            ((0, 215, 8), "MINUTE(-1)", CellValue::Error(CellError::Num)),
            ((0, 215, 9), "SECOND(-0.1)", CellValue::Error(CellError::Num)),

            // 4. Weekdays and week numbers (WEEKDAY, WEEKNUM, ISOWEEKNUM)
            ((0, 220, 0), "WEEKDAY(DATE(2024, 1, 1), 1)", CellValue::Number(2.0)), // Monday = 2 (Sun=1..Sat=7)
            ((0, 220, 1), "WEEKDAY(DATE(2024, 1, 1), 2)", CellValue::Number(1.0)), // Monday = 1 (Mon=1..Sun=7)
            ((0, 220, 2), "WEEKDAY(DATE(2024, 1, 1), 3)", CellValue::Number(0.0)), // Monday = 0 (Mon=0..Sun=6)
            ((0, 220, 3), "WEEKDAY(DATE(2024, 1, 1), 11)", CellValue::Number(1.0)),
            ((0, 220, 4), "WEEKDAY(DATE(2024, 1, 1), 12)", CellValue::Number(7.0)),
            ((0, 220, 5), "WEEKDAY(DATE(2024, 1, 1), 17)", CellValue::Number(2.0)),
            ((0, 220, 6), "WEEKDAY(DATE(2024, 1, 1), 0)", CellValue::Error(CellError::Num)),
            ((0, 220, 7), "WEEKDAY(DATE(2024, 1, 1), 4)", CellValue::Error(CellError::Num)),

            ((0, 225, 0), "WEEKNUM(DATE(2024, 1, 1), 1)", CellValue::Number(1.0)),
            ((0, 225, 1), "WEEKNUM(DATE(2024, 1, 7), 1)", CellValue::Number(2.0)), // Sunday
            ((0, 225, 2), "WEEKNUM(DATE(2024, 1, 7), 2)", CellValue::Number(1.0)), // Monday-start
            ((0, 225, 3), "WEEKNUM(DATE(2024, 1, 8), 2)", CellValue::Number(2.0)),
            ((0, 225, 4), "WEEKNUM(DATE(2024, 1, 1), 21)", CellValue::Number(1.0)), // ISO week
            ((0, 225, 5), "WEEKNUM(DATE(2024, 1, 1), 3)", CellValue::Error(CellError::Num)),

            ((0, 230, 0), "ISOWEEKNUM(DATE(2024, 1, 1))", CellValue::Number(1.0)),
            ((0, 230, 1), "ISOWEEKNUM(DATE(2023, 12, 31))", CellValue::Number(52.0)),
            ((0, 230, 2), "ISOWEEKNUM(DATE(2021, 1, 1))", CellValue::Number(53.0)),

            // 5. String parsing and formatting (DATEVALUE, TIMEVALUE, DATESTRING)
            ((0, 235, 0), "DATEVALUE(\"2024-01-15\")", CellValue::Number(45306.0)),
            ((0, 235, 1), "DATEVALUE(\"2024/1/15\")", CellValue::Number(45306.0)),
            ((0, 235, 2), "DATEVALUE(\"not_a_date\")", CellValue::Error(CellError::Value)),
            ((0, 235, 3), "DATEVALUE(\"\")", CellValue::Error(CellError::Value)),
            ((0, 235, 4), "TIMEVALUE(\"12:00:00\")", CellValue::Number(0.5)),
            ((0, 235, 5), "TIMEVALUE(\"6:00:00 PM\")", CellValue::Number(0.75)),
            ((0, 235, 6), "TIMEVALUE(\"06:00\")", CellValue::Number(0.25)),
            ((0, 235, 7), "TIMEVALUE(\"not_a_time\")", CellValue::Error(CellError::Value)),
            ((0, 235, 8), "DATESTRING(45306)", CellValue::Text("令和6年1月15日".into())),
            ((0, 235, 9), "DATESTRING(0)", CellValue::Text("明治33年1月0日".into())),
            ((0, 235, 10), "DATESTRING(-1)", CellValue::Error(CellError::Num)),

            // 6. Interval & date difference functions (DAYS, DAYS360, DATEDIF)
            ((0, 240, 0), "DAYS(DATE(2024, 2, 1), DATE(2024, 1, 1))", CellValue::Number(31.0)),
            ((0, 240, 1), "DAYS(DATE(2024, 1, 1), DATE(2024, 2, 1))", CellValue::Number(-31.0)),
            ((0, 240, 2), "DAYS(45306, 45306)", CellValue::Number(0.0)),
            ((0, 240, 3), "DAYS360(DATE(2024, 1, 1), DATE(2024, 1, 31), FALSE)", CellValue::Number(30.0)),
            ((0, 240, 4), "DAYS360(DATE(2024, 1, 1), DATE(2024, 1, 31), TRUE)", CellValue::Number(29.0)),
            ((0, 240, 5), "DAYS360(DATE(2024, 1, 31), DATE(2024, 2, 29), FALSE)", CellValue::Number(29.0)),
            ((0, 240, 6), "DAYS360(DATE(2024, 1, 31), DATE(2024, 2, 29), TRUE)", CellValue::Number(29.0)),
            ((0, 240, 7), "DATEDIF(DATE(2020, 1, 1), DATE(2024, 1, 1), \"Y\")", CellValue::Number(4.0)),
            ((0, 240, 8), "DATEDIF(DATE(2020, 1, 1), DATE(2024, 1, 1), \"M\")", CellValue::Number(48.0)),
            ((0, 240, 9), "DATEDIF(DATE(2020, 1, 1), DATE(2024, 1, 1), \"D\")", CellValue::Number(1461.0)),
            ((0, 240, 10), "DATEDIF(DATE(2020, 2, 29), DATE(2021, 2, 28), \"Y\")", CellValue::Number(0.0)),
            ((0, 240, 11), "DATEDIF(DATE(2020, 2, 29), DATE(2021, 3, 1), \"Y\")", CellValue::Number(1.0)),
            ((0, 240, 12), "DATEDIF(DATE(2024, 1, 1), DATE(2020, 1, 1), \"Y\")", CellValue::Error(CellError::Num)),
            ((0, 240, 13), "DATEDIF(DATE(2020, 1, 1), DATE(2024, 1, 1), \"Z\")", CellValue::Error(CellError::Num)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_lookup_and_reference_edge_cases() {
        let mut book = MockBook::new(false);
        book.sheets = vec!["Sheet1".to_string(), "Sheet2".to_string()];

        // 1. Grid A1:C3 (numeric matrix)
        // [10, 20, 30]
        // [40, 50, 60]
        // [70, 80, 90]
        let matrix = [
            (10.0, 20.0, 30.0),
            (40.0, 50.0, 60.0),
            (70.0, 80.0, 90.0),
        ];
        for (r, row) in matrix.iter().enumerate() {
            book.values.insert((0, r as u32, 0), CellValue::Number(row.0));
            book.values.insert((0, r as u32, 1), CellValue::Number(row.1));
            book.values.insert((0, r as u32, 2), CellValue::Number(row.2));
        }

        // 2. Vector D1:D4 (vertical text vector)
        book.values.insert((0, 0, 3), CellValue::Text("alpha".into()));
        book.values.insert((0, 1, 3), CellValue::Text("beta".into()));
        book.values.insert((0, 2, 3), CellValue::Text("gamma".into()));
        book.values.insert((0, 3, 3), CellValue::Text("delta".into()));

        // 3. Vector E1:H1 (horizontal numeric vector)
        book.values.insert((0, 0, 4), CellValue::Number(100.0));
        book.values.insert((0, 0, 5), CellValue::Number(200.0));
        book.values.insert((0, 0, 6), CellValue::Number(300.0));
        book.values.insert((0, 0, 7), CellValue::Number(400.0));

        // 4. Sorted lookup table J1:K4 (keys in J, values in K)
        let sorted_lookup = [
            (10.0, "Bronze"),
            (20.0, "Silver"),
            (30.0, "Gold"),
            (40.0, "Platinum"),
        ];
        for (r, &(k, v)) in sorted_lookup.iter().enumerate() {
            book.values.insert((0, r as u32, 9), CellValue::Number(k));
            book.values.insert((0, r as u32, 10), CellValue::Text(v.into()));
        }

        // 5. Text table M1:N4 (case sensitivity & wildcard lookup)
        let text_table = [
            ("Apple", 100.0),
            ("Banana", 200.0),
            ("Cherry", 300.0),
            ("Date?", 400.0),
        ];
        for (r, &(fruit, price)) in text_table.iter().enumerate() {
            book.values.insert((0, r as u32, 12), CellValue::Text(fruit.into()));
            book.values.insert((0, r as u32, 13), CellValue::Number(price));
        }

        // 6. Table P1:Q3 with an empty return cell
        book.values.insert((0, 0, 15), CellValue::Number(1.0));
        book.values.insert((0, 0, 16), CellValue::Text("Item1".into()));
        book.values.insert((0, 1, 15), CellValue::Number(2.0));
        // Cell Q2 is empty CellValue::Empty
        book.values.insert((0, 2, 15), CellValue::Number(3.0));
        book.values.insert((0, 2, 16), CellValue::Number(300.0));

        // 7. Horizontal lookup table in A31:D32 (row 30 and 31)
        book.values.insert((0, 30, 0), CellValue::Number(10.0));
        book.values.insert((0, 30, 1), CellValue::Number(20.0));
        book.values.insert((0, 30, 2), CellValue::Number(30.0));
        book.values.insert((0, 30, 3), CellValue::Number(40.0));
        book.values.insert((0, 31, 0), CellValue::Text("First".into()));
        book.values.insert((0, 31, 1), CellValue::Text("Second".into()));
        book.values.insert((0, 31, 2), CellValue::Text("Third".into()));
        book.values.insert((0, 31, 3), CellValue::Text("Fourth".into()));

        // 8. Sheet2!A1 cell
        book.values.insert((1, 0, 0), CellValue::Number(999.0));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // ==========================================
            // 1. INDEX function edge cases
            // ==========================================
            ((0, 100, 0), "INDEX(A1:C3, 2, 3)", CellValue::Number(60.0)),
            ((0, 100, 1), "INDEX(A1:C3, 1, 1)", CellValue::Number(10.0)),
            ((0, 100, 2), "INDEX(A1:C3, 3, 2)", CellValue::Number(80.0)),
            // String coerced indices
            ((0, 100, 3), "INDEX(A1:C3, \"2\", \"3\")", CellValue::Number(60.0)),
            // Single row range: row omitted, index selects column
            ((0, 100, 4), "INDEX(E1:H1, 3)", CellValue::Number(300.0)),
            // Single column range: row selected
            ((0, 100, 5), "INDEX(D1:D4, 2)", CellValue::Text("beta".into())),
            // Out of bounds checks
            ((0, 100, 6), "INDEX(A1:C3, 4, 1)", CellValue::Error(CellError::Ref)),
            ((0, 100, 7), "INDEX(A1:C3, 1, 4)", CellValue::Error(CellError::Ref)),
            ((0, 100, 8), "INDEX(A1:C3, -1, 1)", CellValue::Error(CellError::Value)),
            ((0, 100, 9), "INDEX(A1:C3, 1, -1)", CellValue::Error(CellError::Value)),
            ((0, 100, 10), "INDEX(D1:D4, 5)", CellValue::Error(CellError::Ref)),
            // 2D range with only 1 index argument yields #REF!
            ((0, 100, 11), "INDEX(A1:C3, 2)", CellValue::Error(CellError::Ref)),
            // Array constant indexing
            ((0, 100, 12), "INDEX({1, 2, 3; 4, 5, 6}, 2, 1)", CellValue::Number(4.0)),
            ((0, 100, 13), "INDEX({1, 2, 3; 4, 5, 6}, 1, 3)", CellValue::Number(3.0)),
            ((0, 100, 14), "INDEX({1, 2; 3, 4}, 3, 1)", CellValue::Error(CellError::Ref)),

            // ==========================================
            // 2. MATCH function edge cases
            // ==========================================
            // Exact match (0)
            ((0, 110, 0), "MATCH(\"Banana\", M1:M4, 0)", CellValue::Number(2.0)),
            // Case-insensitivity in exact match
            ((0, 110, 1), "MATCH(\"banana\", M1:M4, 0)", CellValue::Number(2.0)),
            ((0, 110, 2), "MATCH(\"APPLE\", M1:M4, 0)", CellValue::Number(1.0)),
            // Wildcard matching in exact mode
            ((0, 110, 3), "MATCH(\"B*a\", M1:M4, 0)", CellValue::Number(2.0)),
            ((0, 110, 4), "MATCH(\"Ch?rry\", M1:M4, 0)", CellValue::Number(3.0)),
            // Escaped wildcard ~?
            ((0, 110, 5), "MATCH(\"Date~?\", M1:M4, 0)", CellValue::Number(4.0)),
            // Not found exact
            ((0, 110, 6), "MATCH(\"Orange\", M1:M4, 0)", CellValue::Error(CellError::NA)),
            // Numeric exact match
            ((0, 110, 7), "MATCH(30, J1:J4, 0)", CellValue::Number(3.0)),
            ((0, 110, 8), "MATCH(25, J1:J4, 0)", CellValue::Error(CellError::NA)),
            // Approximate ascending (1 or omitted)
            ((0, 110, 9), "MATCH(25, J1:J4, 1)", CellValue::Number(2.0)),
            ((0, 110, 10), "MATCH(25, J1:J4)", CellValue::Number(2.0)),
            ((0, 110, 11), "MATCH(40, J1:J4, 1)", CellValue::Number(4.0)),
            ((0, 110, 12), "MATCH(5, J1:J4, 1)", CellValue::Error(CellError::NA)),
            // Approximate descending (-1)
            ((0, 110, 13), "MATCH(25, {40, 30, 20, 10}, -1)", CellValue::Number(2.0)),
            ((0, 110, 14), "MATCH(10, {40, 30, 20, 10}, -1)", CellValue::Number(4.0)),
            ((0, 110, 15), "MATCH(50, {40, 30, 20, 10}, -1)", CellValue::Error(CellError::NA)),
            // 2D range in MATCH yields #N/A
            ((0, 110, 16), "MATCH(10, A1:C3, 0)", CellValue::Error(CellError::NA)),

            // ==========================================
            // 3. VLOOKUP function edge cases
            // ==========================================
            // Exact match (FALSE / 0)
            ((0, 120, 0), "VLOOKUP(\"Banana\", M1:N4, 2, FALSE)", CellValue::Number(200.0)),
            ((0, 120, 1), "VLOOKUP(\"banana\", M1:N4, 2, 0)", CellValue::Number(200.0)),
            ((0, 120, 2), "VLOOKUP(\"Ch*\", M1:N4, 2, FALSE)", CellValue::Number(300.0)),
            ((0, 120, 3), "VLOOKUP(\"Grape\", M1:N4, 2, FALSE)", CellValue::Error(CellError::NA)),
            // Column index 1 returns matched lookup key
            ((0, 120, 4), "VLOOKUP(\"Apple\", M1:N4, 1, FALSE)", CellValue::Text("Apple".into())),
            // Column index out of bounds
            ((0, 120, 5), "VLOOKUP(\"Apple\", M1:N4, 3, FALSE)", CellValue::Error(CellError::Ref)),
            ((0, 120, 6), "VLOOKUP(\"Apple\", M1:N4, 0, FALSE)", CellValue::Error(CellError::Value)),
            ((0, 120, 7), "VLOOKUP(\"Apple\", M1:N4, -1, FALSE)", CellValue::Error(CellError::Value)),
            // Empty cell in return column evaluates to 0.0
            ((0, 120, 8), "VLOOKUP(2, P1:Q3, 2, FALSE)", CellValue::Number(0.0)),
            // Approximate match (TRUE / 1 or omitted)
            ((0, 120, 9), "VLOOKUP(25, J1:K4, 2, TRUE)", CellValue::Text("Silver".into())),
            ((0, 120, 10), "VLOOKUP(25, J1:K4, 2)", CellValue::Text("Silver".into())),
            ((0, 120, 11), "VLOOKUP(30, J1:K4, 2, TRUE)", CellValue::Text("Gold".into())),
            ((0, 120, 12), "VLOOKUP(5, J1:K4, 2, TRUE)", CellValue::Error(CellError::NA)),

            // ==========================================
            // 4. HLOOKUP function edge cases
            // ==========================================
            ((0, 130, 0), "HLOOKUP(30, A31:D32, 2, FALSE)", CellValue::Text("Third".into())),
            ((0, 130, 1), "HLOOKUP(25, A31:D32, 2, TRUE)", CellValue::Text("Second".into())),
            ((0, 130, 2), "HLOOKUP(25, A31:D32, 2)", CellValue::Text("Second".into())),
            ((0, 130, 3), "HLOOKUP(5, A31:D32, 2, TRUE)", CellValue::Error(CellError::NA)),
            ((0, 130, 4), "HLOOKUP(20, A31:D32, 3, FALSE)", CellValue::Error(CellError::Ref)),
            ((0, 130, 5), "HLOOKUP(20, A31:D32, 0, FALSE)", CellValue::Error(CellError::Value)),
            ((0, 130, 6), "HLOOKUP(99, A31:D32, 2, FALSE)", CellValue::Error(CellError::NA)),

            // ==========================================
            // 5. OFFSET function edge cases
            // ==========================================
            ((0, 140, 0), "OFFSET(A1, 1, 2)", CellValue::Number(60.0)),
            ((0, 140, 1), "SUM(OFFSET(A1, 0, 0, 2, 2))", CellValue::Number(120.0)),
            ((0, 140, 2), "OFFSET(C3, -1, -1)", CellValue::Number(50.0)),
            ((0, 140, 3), "SUM(OFFSET(B2, 0, 0, -2, -2))", CellValue::Number(120.0)),
            // Non-reference argument
            ((0, 140, 4), "OFFSET(123, 1, 1)", CellValue::Error(CellError::Value)),
            // Out of bounds coordinates
            ((0, 140, 5), "OFFSET(A1, -1, 0)", CellValue::Error(CellError::Ref)),
            ((0, 140, 6), "OFFSET(A1, 0, -1)", CellValue::Error(CellError::Ref)),
            // Zero height or width
            ((0, 140, 7), "OFFSET(A1, 1, 1, 0, 1)", CellValue::Error(CellError::Ref)),
            ((0, 140, 8), "OFFSET(A1, 1, 1, 1, 0)", CellValue::Error(CellError::Ref)),

            // ==========================================
            // 6. INDIRECT function edge cases
            // ==========================================
            ((0, 150, 0), "INDIRECT(\"A1\")", CellValue::Number(10.0)),
            ((0, 150, 1), "INDIRECT(\"B2\")", CellValue::Number(50.0)),
            ((0, 150, 2), "SUM(INDIRECT(\"A1:B2\"))", CellValue::Number(120.0)),
            ((0, 150, 3), "INDIRECT(\"Sheet2!A1\")", CellValue::Number(999.0)),
            ((0, 150, 4), "INDIRECT(\"'Sheet2'!A1\")", CellValue::Number(999.0)),
            // R1C1 reference notation
            ((0, 150, 5), "INDIRECT(\"R1C1\", FALSE)", CellValue::Number(10.0)),
            ((0, 150, 6), "INDIRECT(\"R2C3\", FALSE)", CellValue::Number(60.0)),
            ((0, 150, 7), "SUM(INDIRECT(\"R1C1:R2C2\", FALSE))", CellValue::Number(120.0)),
            // Invalid reference strings
            ((0, 150, 8), "INDIRECT(\"InvalidRef\")", CellValue::Error(CellError::Ref)),
            ((0, 150, 9), "INDIRECT(\"\")", CellValue::Error(CellError::Ref)),
            ((0, 150, 10), "INDIRECT(123)", CellValue::Error(CellError::Ref)),

            // ==========================================
            // 7. CHOOSE function edge cases
            // ==========================================
            ((0, 160, 0), "CHOOSE(2, \"Apple\", \"Banana\", \"Cherry\")", CellValue::Text("Banana".into())),
            ((0, 160, 1), "CHOOSE(3.8, 100, 200, 300, 400)", CellValue::Number(300.0)),
            ((0, 160, 2), "CHOOSE(\"1\", \"First\", \"Second\")", CellValue::Text("First".into())),
            ((0, 160, 3), "CHOOSE(0, \"A\", \"B\")", CellValue::Error(CellError::Value)),
            ((0, 160, 4), "CHOOSE(4, \"A\", \"B\", \"C\")", CellValue::Error(CellError::Value)),
            ((0, 160, 5), "CHOOSE(-1, \"A\", \"B\")", CellValue::Error(CellError::Value)),
            ((0, 160, 6), "CHOOSE(\"abc\", \"A\", \"B\")", CellValue::Error(CellError::Value)),

            // ==========================================
            // 8. LOOKUP function edge cases
            // ==========================================
            // Vector form
            ((0, 170, 0), "LOOKUP(25, J1:J4, K1:K4)", CellValue::Text("Silver".into())),
            ((0, 170, 1), "LOOKUP(30, J1:J4, K1:K4)", CellValue::Text("Gold".into())),
            ((0, 170, 2), "LOOKUP(5, J1:J4, K1:K4)", CellValue::Error(CellError::NA)),
            // Array form
            ((0, 170, 3), "LOOKUP(25, {10, \"A\"; 20, \"B\"; 30, \"C\"})", CellValue::Text("B".into())),

            // ==========================================
            // 9. ROW, COLUMN, ROWS, COLUMNS, AREAS, ADDRESS
            // ==========================================
            ((0, 180, 0), "ROW(C5)", CellValue::Number(5.0)),
            ((0, 180, 1), "COLUMN(C5)", CellValue::Number(3.0)),
            ((0, 180, 2), "ROWS(A1:C4)", CellValue::Number(4.0)),
            ((0, 180, 3), "COLUMNS(A1:C4)", CellValue::Number(3.0)),
            ((0, 180, 4), "AREAS((A1:B2, D1:E2, G1:G5))", CellValue::Number(3.0)),
            ((0, 180, 5), "AREAS(A1:B2)", CellValue::Number(1.0)),
            ((0, 180, 6), "AREAS(123)", CellValue::Error(CellError::Value)),
            ((0, 180, 7), "ADDRESS(1, 1)", CellValue::Text("$A$1".into())),
            ((0, 180, 8), "ADDRESS(2, 3, 2)", CellValue::Text("C$2".into())),
            ((0, 180, 9), "ADDRESS(4, 5, 3)", CellValue::Text("$E4".into())),
            ((0, 180, 10), "ADDRESS(6, 7, 4)", CellValue::Text("G6".into())),
            ((0, 180, 11), "ADDRESS(2, 3, 1, FALSE)", CellValue::Text("R2C3".into())),
            ((0, 180, 12), "ADDRESS(1, 1, 1, TRUE, \"Sales\")", CellValue::Text("Sales!$A$1".into())),
            ((0, 180, 13), "ADDRESS(0, 1)", CellValue::Error(CellError::Value)),
            ((0, 180, 14), "ADDRESS(1, 0)", CellValue::Error(CellError::Value)),
            ((0, 180, 15), "ADDRESS(1, 1, 5)", CellValue::Error(CellError::Value)),

            // ==========================================
            // 10. TRANSPOSE & Error Propagation
            // ==========================================
            ((0, 190, 0), "INDEX(TRANSPOSE({1, 2, 3; 4, 5, 6}), 2, 1)", CellValue::Number(2.0)),
            ((0, 190, 1), "INDEX(TRANSPOSE({1, 2, 3; 4, 5, 6}), 1, 2)", CellValue::Number(4.0)),
            ((0, 190, 2), "INDEX(A1:C3, #VALUE!, 1)", CellValue::Error(CellError::Value)),
            ((0, 190, 3), "INDEX(A1:C3, 1, #NAME?)", CellValue::Error(CellError::Name)),
            ((0, 190, 4), "MATCH(#NUM!, J1:J4, 0)", CellValue::Error(CellError::Num)),
            ((0, 190, 5), "VLOOKUP(#DIV/0!, J1:K4, 2, FALSE)", CellValue::Error(CellError::Div0)),
            ((0, 190, 6), "OFFSET(A1, #VALUE!, 1)", CellValue::Error(CellError::Value)),
            ((0, 190, 7), "CHOOSE(1, #N/A, 2)", CellValue::Error(CellError::NA)),
            ((0, 190, 8), "CHOOSE(2, 1, #DIV/0!)", CellValue::Error(CellError::Div0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_information_logical_and_error_handling_edge_cases() {
        let mut book = MockBook::new(false);

        // Populate base fixture cells in Sheet 0, Row 0
        // A1: (0, 0, 0) = Empty
        // B1: (0, 0, 1) = 123.0
        // C1: (0, 0, 2) = "hello"
        // D1: (0, 0, 3) = "" (empty string text)
        // E1: (0, 0, 4) = TRUE
        // F1: (0, 0, 5) = #DIV/0!
        // G1: (0, 0, 6) = #N/A
        // H1: (0, 0, 7) = formula ="calculated"
        book.values.insert((0, 0, 1), CellValue::Number(123.0));
        book.values.insert((0, 0, 2), CellValue::Text("hello".into()));
        book.values.insert((0, 0, 3), CellValue::Text("".into()));
        book.values.insert((0, 0, 4), CellValue::Bool(true));
        book.values.insert((0, 0, 5), CellValue::Error(CellError::Div0));
        book.values.insert((0, 0, 6), CellValue::Error(CellError::NA));
        book.formulas.insert((0, 0, 7), "\"calculated\"".to_string());
        book.values.insert((0, 0, 7), CellValue::Text("calculated".into()));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Type inspection & predicates (ISBLANK, ISNUMBER, ISTEXT, ISNONTEXT, ISLOGICAL)
            ((0, 300, 0), "ISBLANK(A1)", CellValue::Bool(true)),
            ((0, 300, 1), "ISBLANK(B1)", CellValue::Bool(false)),
            ((0, 300, 2), "ISBLANK(C1)", CellValue::Bool(false)),
            ((0, 300, 3), "ISBLANK(D1)", CellValue::Bool(false)), // Empty text is not blank
            ((0, 300, 4), "ISBLANK(\"\")", CellValue::Bool(false)),
            ((0, 300, 5), "ISBLANK(123)", CellValue::Bool(false)),

            ((0, 305, 0), "ISNUMBER(B1)", CellValue::Bool(true)),
            ((0, 305, 1), "ISNUMBER(123.45)", CellValue::Bool(true)),
            ((0, 305, 2), "ISNUMBER(\"123\")", CellValue::Bool(false)), // No text coercion
            ((0, 305, 3), "ISNUMBER(TRUE)", CellValue::Bool(false)),
            ((0, 305, 4), "ISNUMBER(A1)", CellValue::Bool(false)),

            ((0, 310, 0), "ISTEXT(C1)", CellValue::Bool(true)),
            ((0, 310, 1), "ISTEXT(D1)", CellValue::Bool(true)),
            ((0, 310, 2), "ISTEXT(\"test\")", CellValue::Bool(true)),
            ((0, 310, 3), "ISTEXT(123)", CellValue::Bool(false)),
            ((0, 310, 4), "ISTEXT(TRUE)", CellValue::Bool(false)),
            ((0, 310, 5), "ISTEXT(A1)", CellValue::Bool(false)),

            ((0, 315, 0), "ISNONTEXT(A1)", CellValue::Bool(true)), // Empty is non-text
            ((0, 315, 1), "ISNONTEXT(B1)", CellValue::Bool(true)),
            ((0, 315, 2), "ISNONTEXT(E1)", CellValue::Bool(true)),
            ((0, 315, 3), "ISNONTEXT(F1)", CellValue::Bool(true)),
            ((0, 315, 4), "ISNONTEXT(C1)", CellValue::Bool(false)),
            ((0, 315, 5), "ISNONTEXT(D1)", CellValue::Bool(false)),

            ((0, 320, 0), "ISLOGICAL(E1)", CellValue::Bool(true)),
            ((0, 320, 1), "ISLOGICAL(TRUE)", CellValue::Bool(true)),
            ((0, 320, 2), "ISLOGICAL(FALSE)", CellValue::Bool(true)),
            ((0, 320, 3), "ISLOGICAL(1)", CellValue::Bool(false)),
            ((0, 320, 4), "ISLOGICAL(\"TRUE\")", CellValue::Bool(false)),

            // 2. Error inspection & handling (ISERROR, ISERR, ISNA, NA, ERROR.TYPE, IFERROR, IFNA)
            ((0, 325, 0), "NA()", CellValue::Error(CellError::NA)),
            ((0, 325, 1), "ISERROR(F1)", CellValue::Bool(true)),
            ((0, 325, 2), "ISERROR(G1)", CellValue::Bool(true)),
            ((0, 325, 3), "ISERROR(1/0)", CellValue::Bool(true)),
            ((0, 325, 4), "ISERROR(NA())", CellValue::Bool(true)),
            ((0, 325, 5), "ISERROR(123)", CellValue::Bool(false)),

            ((0, 330, 0), "ISERR(F1)", CellValue::Bool(true)),
            ((0, 330, 1), "ISERR(G1)", CellValue::Bool(false)), // ISERR ignores #N/A
            ((0, 330, 2), "ISERR(1/0)", CellValue::Bool(true)),
            ((0, 330, 3), "ISERR(NA())", CellValue::Bool(false)),
            ((0, 330, 4), "ISERR(123)", CellValue::Bool(false)),

            ((0, 335, 0), "ISNA(G1)", CellValue::Bool(true)),
            ((0, 335, 1), "ISNA(NA())", CellValue::Bool(true)),
            ((0, 335, 2), "ISNA(F1)", CellValue::Bool(false)),
            ((0, 335, 3), "ISNA(1/0)", CellValue::Bool(false)),
            ((0, 335, 4), "ISNA(123)", CellValue::Bool(false)),

            ((0, 340, 0), "ERROR.TYPE(1/0)", CellValue::Number(2.0)), // #DIV/0! = 2
            ((0, 340, 1), "ERROR.TYPE(SQRT(-1))", CellValue::Number(6.0)), // #NUM! = 6
            ((0, 340, 2), "ERROR.TYPE(NA())", CellValue::Number(7.0)), // #N/A = 7
            ((0, 340, 3), "ERROR.TYPE(123)", CellValue::Error(CellError::NA)), // Non-error -> #N/A

            ((0, 345, 0), "IFERROR(1/0, \"err\")", CellValue::Text("err".into())),
            ((0, 345, 1), "IFERROR(NA(), \"err\")", CellValue::Text("err".into())),
            ((0, 345, 2), "IFERROR(42, \"err\")", CellValue::Number(42.0)),
            ((0, 345, 3), "IFNA(NA(), \"not_found\")", CellValue::Text("not_found".into())),
            ((0, 345, 4), "IFNA(1/0, \"not_found\")", CellValue::Error(CellError::Div0)), // IFNA ignores #DIV/0!
            ((0, 345, 5), "IFNA(42, \"not_found\")", CellValue::Number(42.0)),

            // 3. Reference & structure metadata (ISREF, ISFORMULA, SHEET, SHEETS, CELL, TYPE)
            ((0, 350, 0), "ISREF(A1)", CellValue::Bool(true)),
            ((0, 350, 1), "ISREF(A1:B2)", CellValue::Bool(true)),
            ((0, 350, 2), "ISREF(123)", CellValue::Bool(false)),
            ((0, 350, 3), "ISREF(\"A1\")", CellValue::Bool(false)),

            ((0, 355, 0), "ISFORMULA(H1)", CellValue::Bool(true)),
            ((0, 355, 1), "ISFORMULA(B1)", CellValue::Bool(false)),
            ((0, 355, 2), "ISFORMULA(A1)", CellValue::Bool(false)),
            ((0, 355, 3), "ISFORMULA(123)", CellValue::Error(CellError::Value)),

            ((0, 360, 0), "SHEET()", CellValue::Number(1.0)),
            ((0, 360, 1), "SHEET(A1)", CellValue::Number(1.0)),
            ((0, 360, 2), "SHEET(\"NoSuchSheet\")", CellValue::Error(CellError::NA)),
            ((0, 360, 3), "SHEETS()", CellValue::Number(1.0)),
            ((0, 360, 4), "SHEETS(A1)", CellValue::Number(1.0)),

            ((0, 365, 0), "CELL(\"row\", B5)", CellValue::Number(5.0)),
            ((0, 365, 1), "CELL(\"col\", B5)", CellValue::Number(2.0)),
            ((0, 365, 2), "CELL(\"address\", B5)", CellValue::Text("$B$5".into())),
            ((0, 365, 3), "CELL(\"invalid\", B5)", CellValue::Error(CellError::Value)),
            ((0, 365, 4), "CELL(\"row\", 123)", CellValue::Error(CellError::Value)),

            ((0, 370, 0), "TYPE(123)", CellValue::Number(1.0)),
            ((0, 370, 1), "TYPE(A1)", CellValue::Number(1.0)), // Empty cell treated as 1
            ((0, 370, 2), "TYPE(\"abc\")", CellValue::Number(2.0)),
            ((0, 370, 3), "TYPE(TRUE)", CellValue::Number(4.0)),
            ((0, 370, 4), "TYPE(1/0)", CellValue::Number(16.0)),
            ((0, 370, 5), "TYPE({1, 2; 3, 4})", CellValue::Number(64.0)),

            // 4. Parity checkers (ISEVEN, ISODD)
            ((0, 375, 0), "ISEVEN(0)", CellValue::Bool(true)),
            ((0, 375, 1), "ISEVEN(2)", CellValue::Bool(true)),
            ((0, 375, 2), "ISEVEN(3)", CellValue::Bool(false)),
            ((0, 375, 3), "ISEVEN(2.9)", CellValue::Bool(true)),
            ((0, 375, 4), "ISEVEN(-2.9)", CellValue::Bool(true)),
            ((0, 375, 5), "ISEVEN(-3.1)", CellValue::Bool(false)),
            ((0, 375, 6), "ISEVEN(\"4\")", CellValue::Bool(true)),
            ((0, 375, 7), "ISEVEN(\"abc\")", CellValue::Error(CellError::Value)),
            ((0, 375, 8), "ISEVEN(TRUE)", CellValue::Error(CellError::Value)),

            ((0, 380, 0), "ISODD(0)", CellValue::Bool(false)),
            ((0, 380, 1), "ISODD(1)", CellValue::Bool(true)),
            ((0, 380, 2), "ISODD(2)", CellValue::Bool(false)),
            ((0, 380, 3), "ISODD(3.9)", CellValue::Bool(true)),
            ((0, 380, 4), "ISODD(-3.1)", CellValue::Bool(true)),
            ((0, 380, 5), "ISODD(\"5\")", CellValue::Bool(true)),
            ((0, 380, 6), "ISODD(FALSE)", CellValue::Error(CellError::Value)),

            // 5. Conversion helpers (N, T)
            ((0, 385, 0), "N(123)", CellValue::Number(123.0)),
            ((0, 385, 1), "N(TRUE)", CellValue::Number(1.0)),
            ((0, 385, 2), "N(FALSE)", CellValue::Number(0.0)),
            ((0, 385, 3), "N(\"abc\")", CellValue::Number(0.0)),
            ((0, 385, 4), "N(1/0)", CellValue::Error(CellError::Div0)),
            ((0, 385, 5), "N(A1)", CellValue::Number(0.0)),

            ((0, 390, 0), "T(\"abc\")", CellValue::Text("abc".into())),
            ((0, 390, 1), "T(123)", CellValue::Text("".into())),
            ((0, 390, 2), "T(TRUE)", CellValue::Text("".into())),
            ((0, 390, 3), "T(1/0)", CellValue::Error(CellError::Div0)),
            ((0, 390, 4), "T(A1)", CellValue::Text("".into())),

            // 6. Logical operations & multi-condition branches (NOT, AND, OR, XOR, IF, IFS, SWITCH)
            ((0, 395, 0), "NOT(TRUE)", CellValue::Bool(false)),
            ((0, 395, 1), "NOT(FALSE)", CellValue::Bool(true)),
            ((0, 395, 2), "NOT(0)", CellValue::Bool(true)),
            ((0, 395, 3), "NOT(1)", CellValue::Bool(false)),
            ((0, 395, 4), "NOT(\"text\")", CellValue::Error(CellError::Value)),

            ((0, 400, 0), "AND(TRUE, 1, 2)", CellValue::Bool(true)),
            ((0, 400, 1), "AND(TRUE, 0)", CellValue::Bool(false)),
            ((0, 400, 2), "AND(\"text\")", CellValue::Error(CellError::Value)),
            ((0, 400, 3), "AND(\"TRUE\")", CellValue::Bool(true)),

            ((0, 405, 0), "OR(FALSE, 0, 1)", CellValue::Bool(true)),
            ((0, 405, 1), "OR(FALSE, 0, 0)", CellValue::Bool(false)),

            ((0, 410, 0), "XOR(TRUE, FALSE)", CellValue::Bool(true)),
            ((0, 410, 1), "XOR(TRUE, TRUE)", CellValue::Bool(false)),
            ((0, 410, 2), "XOR(TRUE, TRUE, TRUE)", CellValue::Bool(true)), // Parity of TRUE
            ((0, 410, 3), "XOR(FALSE, FALSE)", CellValue::Bool(false)),

            ((0, 415, 0), "IF(TRUE, 10, 20)", CellValue::Number(10.0)),
            ((0, 415, 1), "IF(FALSE, 10, 20)", CellValue::Number(20.0)),
            ((0, 415, 2), "IF(1, 10, 20)", CellValue::Number(10.0)),
            ((0, 415, 3), "IF(0, 10, 20)", CellValue::Number(20.0)),
            ((0, 415, 4), "IF(FALSE, 10)", CellValue::Bool(false)),
            ((0, 415, 5), "IF(\"text\", 10, 20)", CellValue::Error(CellError::Value)),

            ((0, 420, 0), "IFS(1=2, \"a\", 2=3, \"b\", 3=3, \"c\")", CellValue::Text("c".into())),
            ((0, 420, 1), "IFS(1=2, \"a\", 2=3, \"b\")", CellValue::Error(CellError::NA)),
            ((0, 420, 2), "IFS(TRUE, 1, 2)", CellValue::Error(CellError::Value)), // Odd arg count

            ((0, 425, 0), "SWITCH(2, 1, \"one\", 2, \"two\", 3, \"three\")", CellValue::Text("two".into())),
            ((0, 425, 1), "SWITCH(9, 1, \"one\", 2, \"two\", \"default\")", CellValue::Text("default".into())),
            ((0, 425, 2), "SWITCH(9, 1, \"one\", 2, \"two\")", CellValue::Error(CellError::NA)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_engineering_and_complex_functions_edge_cases() {
        let mut book = MockBook::new(false);

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Radix conversions (BIN, OCT, DEC, HEX)
            ((0, 500, 0), "BIN2DEC(\"1010\")", CellValue::Number(10.0)),
            ((0, 500, 1), "BIN2DEC(\"1111111111\")", CellValue::Number(-1.0)), // 10-bit two's complement negative
            ((0, 500, 2), "BIN2DEC(\"1000000000\")", CellValue::Number(-512.0)), // Minimum negative
            ((0, 500, 3), "BIN2DEC(\"0111111111\")", CellValue::Number(511.0)), // Maximum positive
            ((0, 500, 4), "BIN2DEC(\"10101010101\")", CellValue::Error(CellError::Num)), // > 10 chars
            ((0, 500, 5), "BIN2DEC(\"102\")", CellValue::Error(CellError::Num)), // Invalid binary char

            ((0, 505, 0), "DEC2BIN(10)", CellValue::Text("1010".into())),
            ((0, 505, 1), "DEC2BIN(10, 6)", CellValue::Text("001010".into())),
            ((0, 505, 2), "DEC2BIN(-1)", CellValue::Text("1111111111".into())),
            ((0, 505, 3), "DEC2BIN(-512)", CellValue::Text("1000000000".into())),
            ((0, 505, 4), "DEC2BIN(511)", CellValue::Text("111111111".into())),
            ((0, 505, 5), "DEC2BIN(512)", CellValue::Error(CellError::Num)), // Positive overflow
            ((0, 505, 6), "DEC2BIN(-513)", CellValue::Error(CellError::Num)), // Negative underflow
            ((0, 505, 7), "DEC2BIN(10, 2)", CellValue::Error(CellError::Num)), // Places too small

            ((0, 510, 0), "HEX2DEC(\"A\")", CellValue::Number(10.0)),
            ((0, 510, 1), "HEX2DEC(\"FFFFFFFFFF\")", CellValue::Number(-1.0)),
            ((0, 510, 2), "HEX2DEC(\"7FFFFFFFFF\")", CellValue::Number(549755813887.0)),
            ((0, 510, 3), "HEX2DEC(\"G\")", CellValue::Error(CellError::Num)),

            ((0, 515, 0), "DEC2HEX(10)", CellValue::Text("A".into())),
            ((0, 515, 1), "DEC2HEX(10, 4)", CellValue::Text("000A".into())),
            ((0, 515, 2), "DEC2HEX(-1)", CellValue::Text("FFFFFFFFFF".into())),

            ((0, 520, 0), "OCT2DEC(\"12\")", CellValue::Number(10.0)),
            ((0, 520, 1), "OCT2DEC(\"7777777777\")", CellValue::Number(-1.0)),
            ((0, 520, 2), "OCT2DEC(\"8\")", CellValue::Error(CellError::Num)),

            ((0, 525, 0), "DEC2OCT(10)", CellValue::Text("12".into())),
            ((0, 525, 1), "DEC2OCT(10, 4)", CellValue::Text("0012".into())),
            ((0, 525, 2), "DEC2OCT(-1)", CellValue::Text("7777777777".into())),

            ((0, 530, 0), "BIN2HEX(\"1010\")", CellValue::Text("A".into())),
            ((0, 530, 1), "BIN2OCT(\"1010\")", CellValue::Text("12".into())),
            ((0, 530, 2), "HEX2BIN(\"A\")", CellValue::Text("1010".into())),
            ((0, 530, 3), "HEX2OCT(\"A\")", CellValue::Text("12".into())),
            ((0, 530, 4), "OCT2BIN(\"12\")", CellValue::Text("1010".into())),
            ((0, 530, 5), "OCT2HEX(\"12\")", CellValue::Text("A".into())),

            // 2. Bitwise functions (BITAND, BITOR, BITXOR, BITLSHIFT, BITRSHIFT)
            ((0, 535, 0), "BITAND(6, 3)", CellValue::Number(2.0)), // 110 & 011 = 010
            ((0, 535, 1), "BITOR(6, 3)", CellValue::Number(7.0)), // 110 | 011 = 111
            ((0, 535, 2), "BITXOR(6, 3)", CellValue::Number(5.0)), // 110 ^ 011 = 101
            ((0, 535, 3), "BITLSHIFT(4, 2)", CellValue::Number(16.0)),
            ((0, 535, 4), "BITLSHIFT(16, -2)", CellValue::Number(4.0)), // Negative shift = right shift
            ((0, 535, 5), "BITRSHIFT(16, 2)", CellValue::Number(4.0)),
            ((0, 535, 6), "BITRSHIFT(4, -2)", CellValue::Number(16.0)), // Negative shift = left shift
            ((0, 535, 7), "BITAND(-1, 5)", CellValue::Error(CellError::Num)),
            ((0, 535, 8), "BITAND(1.5, 5)", CellValue::Error(CellError::Num)),
            ((0, 535, 9), "BITLSHIFT(1, 54)", CellValue::Error(CellError::Num)),

            // 3. Threshold & comparison functions (DELTA, GESTEP)
            ((0, 540, 0), "DELTA(5, 5)", CellValue::Number(1.0)),
            ((0, 540, 1), "DELTA(5, 4)", CellValue::Number(0.0)),
            ((0, 540, 2), "DELTA(5)", CellValue::Number(0.0)),
            ((0, 540, 3), "DELTA(0)", CellValue::Number(1.0)),
            ((0, 545, 0), "GESTEP(5, 4)", CellValue::Number(1.0)),
            ((0, 545, 1), "GESTEP(5, 5)", CellValue::Number(1.0)),
            ((0, 545, 2), "GESTEP(4, 5)", CellValue::Number(0.0)),
            ((0, 545, 3), "GESTEP(5)", CellValue::Number(1.0)),
            ((0, 545, 4), "GESTEP(-1)", CellValue::Number(0.0)),

            // 4. Error functions (ERF, ERF.PRECISE, ERFC, ERFC.PRECISE)
            ((0, 550, 0), "ERF(0)", CellValue::Number(0.0)),
            ((0, 550, 1), "ERF.PRECISE(0)", CellValue::Number(0.0)),
            ((0, 550, 2), "ERFC(0)", CellValue::Number(1.0)),
            ((0, 550, 3), "ERFC.PRECISE(0)", CellValue::Number(1.0)),
            ((0, 550, 4), "ERF(1, 1)", CellValue::Number(0.0)),

            // 5. Unit conversions (CONVERT)
            ((0, 555, 0), "CONVERT(1, \"m\", \"cm\")", CellValue::Number(100.0)),
            ((0, 555, 1), "CONVERT(1, \"km\", \"m\")", CellValue::Number(1000.0)),
            ((0, 555, 2), "CONVERT(1, \"in\", \"cm\")", CellValue::Number(2.54)),
            ((0, 555, 3), "CONVERT(100, \"C\", \"F\")", CellValue::Number(212.0)),
            ((0, 555, 4), "CONVERT(0, \"C\", \"F\")", CellValue::Number(32.0)),
            ((0, 555, 5), "CONVERT(0, \"C\", \"K\")", CellValue::Number(273.15)),
            ((0, 555, 6), "CONVERT(1, \"m\", \"kg\")", CellValue::Error(CellError::NA)), // Incompatible units
            ((0, 555, 7), "CONVERT(1, \"unknown_unit\", \"m\")", CellValue::Error(CellError::NA)),

            // 6. Complex numbers (COMPLEX, IMREAL, IMAGINARY, IMABS, IMCONJUGATE, IMARGUMENT, IMSUM, IMSUB, IMPRODUCT, IMDIV, IMSQRT, IMEXP, IMLN, IMPOWER)
            ((0, 560, 0), "COMPLEX(3, 4)", CellValue::Text("3+4i".into())),
            ((0, 560, 1), "COMPLEX(3, -4)", CellValue::Text("3-4i".into())),
            ((0, 560, 2), "COMPLEX(0, 1)", CellValue::Text("i".into())),
            ((0, 560, 3), "COMPLEX(0, -1)", CellValue::Text("-i".into())),
            ((0, 560, 4), "COMPLEX(3, 0)", CellValue::Text("3".into())),
            ((0, 560, 5), "COMPLEX(0, 0)", CellValue::Text("0".into())),
            ((0, 560, 6), "COMPLEX(3, 4, \"j\")", CellValue::Text("3+4j".into())),
            ((0, 560, 7), "COMPLEX(3, 4, \"k\")", CellValue::Error(CellError::Value)), // Invalid suffix

            ((0, 565, 0), "IMREAL(\"3+4i\")", CellValue::Number(3.0)),
            ((0, 565, 1), "IMREAL(\"4i\")", CellValue::Number(0.0)),
            ((0, 565, 2), "IMREAL(\"5\")", CellValue::Number(5.0)),
            ((0, 565, 3), "IMAGINARY(\"3+4i\")", CellValue::Number(4.0)),
            ((0, 565, 4), "IMAGINARY(\"3-4i\")", CellValue::Number(-4.0)),
            ((0, 565, 5), "IMAGINARY(\"i\")", CellValue::Number(1.0)),
            ((0, 565, 6), "IMAGINARY(\"-i\")", CellValue::Number(-1.0)),
            ((0, 565, 7), "IMAGINARY(\"5\")", CellValue::Number(0.0)),

            ((0, 570, 0), "IMABS(\"3+4i\")", CellValue::Number(5.0)),
            ((0, 570, 1), "IMABS(\"5\")", CellValue::Number(5.0)),
            ((0, 570, 2), "IMCONJUGATE(\"3+4i\")", CellValue::Text("3-4i".into())),
            ((0, 570, 3), "IMCONJUGATE(\"5\")", CellValue::Text("5".into())),

            ((0, 575, 0), "IMARGUMENT(\"1\")", CellValue::Number(0.0)),
            ((0, 575, 1), "IMARGUMENT(\"-1\")", CellValue::Number(std::f64::consts::PI)),
            ((0, 575, 2), "IMARGUMENT(\"0\")", CellValue::Error(CellError::Div0)),

            ((0, 580, 0), "IMSUM(\"1+2i\", \"3+4i\")", CellValue::Text("4+6i".into())),
            ((0, 580, 1), "IMSUB(\"3+4i\", \"1+2i\")", CellValue::Text("2+2i".into())),
            ((0, 580, 2), "IMPRODUCT(\"1+2i\", \"3+4i\")", CellValue::Text("-5+10i".into())),
            ((0, 580, 3), "IMDIV(\"-5+10i\", \"1+2i\")", CellValue::Text("3+4i".into())),
            ((0, 580, 4), "IMDIV(\"1+2i\", \"3+4j\")", CellValue::Error(CellError::Value)), // Mismatched suffixes
            ((0, 580, 5), "IMDIV(\"1+2i\", \"0\")", CellValue::Error(CellError::Num)), // Division by zero

            ((0, 585, 0), "IMREAL(IMSQRT(\"-1\"))", CellValue::Number(0.0)),
            ((0, 585, 1), "IMAGINARY(IMSQRT(\"-1\"))", CellValue::Number(1.0)),
            ((0, 585, 6), "IMSQRT(\"4\")", CellValue::Text("2".into())),
            ((0, 585, 2), "IMEXP(\"0\")", CellValue::Text("1".into())),
            ((0, 585, 3), "IMLN(\"1\")", CellValue::Text("0".into())),
            ((0, 585, 4), "IMLN(\"0\")", CellValue::Error(CellError::Num)),
            ((0, 585, 5), "IMPOWER(\"2+3i\", 2)", CellValue::Text("-5+12i".into())),

            // 7. Bessel functions (BESSELJ, BESSELI, BESSELY, BESSELK)
            ((0, 590, 0), "BESSELJ(0, 0)", CellValue::Number(1.0)),
            ((0, 590, 1), "BESSELJ(0, 1)", CellValue::Number(0.0)),
            ((0, 590, 2), "BESSELI(0, 0)", CellValue::Number(1.0)),
            ((0, 590, 3), "BESSELI(0, 1)", CellValue::Number(0.0)),
            ((0, 590, 4), "BESSELJ(1, -1)", CellValue::Error(CellError::Num)),
            ((0, 590, 5), "BESSELI(1, -1)", CellValue::Error(CellError::Num)),
            ((0, 590, 6), "BESSELY(-1, 0)", CellValue::Error(CellError::Num)),
            ((0, 590, 7), "BESSELK(0, 0)", CellValue::Error(CellError::Num)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_financial_advanced_tvm_cash_flows_and_depreciation() {
        let mut book = MockBook::new(false);

        // 1. Cash flow series in C1:C4
        book.values.insert((0, 0, 2), CellValue::Number(-10000.0));
        book.values.insert((0, 1, 2), CellValue::Number(3000.0));
        book.values.insert((0, 2, 2), CellValue::Number(4200.0));
        book.values.insert((0, 3, 2), CellValue::Number(6800.0));

        // 2. Dates series in D1:D4 (serials: 2024-01-01, 2024-03-01, 2024-06-01, 2024-12-01)
        book.values.insert((0, 0, 3), CellValue::Number(45292.0));
        book.values.insert((0, 1, 3), CellValue::Number(45352.0));
        book.values.insert((0, 2, 3), CellValue::Number(45444.0));
        book.values.insert((0, 3, 3), CellValue::Number(45627.0));

        // 3. All positive cash flows in E1:E4
        book.values.insert((0, 0, 4), CellValue::Number(100.0));
        book.values.insert((0, 1, 4), CellValue::Number(200.0));
        book.values.insert((0, 2, 4), CellValue::Number(300.0));
        book.values.insert((0, 3, 4), CellValue::Number(400.0));

        // 4. All negative cash flows in F1:F4
        book.values.insert((0, 0, 5), CellValue::Number(-100.0));
        book.values.insert((0, 1, 5), CellValue::Number(-200.0));
        book.values.insert((0, 2, 5), CellValue::Number(-300.0));
        book.values.insert((0, 3, 5), CellValue::Number(-400.0));

        // 5. Non-chronological dates series in G1:G3
        book.values.insert((0, 0, 6), CellValue::Number(45292.0));
        book.values.insert((0, 1, 6), CellValue::Number(45200.0));
        book.values.insert((0, 2, 6), CellValue::Number(45352.0));

        // 6. Interest rate schedule in H1:H3
        book.values.insert((0, 0, 7), CellValue::Number(0.05));
        book.values.insert((0, 1, 7), CellValue::Number(0.04));
        book.values.insert((0, 2, 7), CellValue::Number(0.03));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // ==========================================
            // 1. NPV (Net Present Value)
            // ==========================================
            // Simple sum at 0% rate
            ((0, 200, 0), "NPV(0, C2:C4)", CellValue::Number(14000.0)),
            // Discounting at 10% rate
            ((0, 200, 1), "ROUND(NPV(0.1, C2:C4), 2)", CellValue::Number(11307.29)),
            // Discounting with initial investment included in NPV
            ((0, 200, 2), "ROUND(NPV(0.1, C1:C4), 2)", CellValue::Number(1188.44)),
            // Negative discount rate (-5%)
            ((0, 200, 3), "ROUND(NPV(-0.05, 100, 200), 2)", CellValue::Number(326.87)),
            // Rate = -100% (-1.0) causes #DIV/0!
            ((0, 200, 4), "NPV(-1, 100, 200)", CellValue::Error(CellError::Div0)),
            // All positive series at 0% rate
            ((0, 200, 5), "NPV(0, E1:E4)", CellValue::Number(1000.0)),
            // Array constant arguments
            ((0, 200, 6), "ROUND(NPV(0.05, 1000, 2000, 3000), 2)", CellValue::Number(5357.95)),
            // Error propagation
            ((0, 200, 7), "NPV(0.05, 100, #DIV/0!)", CellValue::Error(CellError::Div0)),
            ((0, 200, 8), "NPV(0.05, #VALUE!, 100)", CellValue::Error(CellError::Value)),

            // ==========================================
            // 2. IRR (Internal Rate of Return)
            // ==========================================
            // Standard investment cash flow (-10000, 3000, 4200, 6800) -> ~16.34%
            ((0, 210, 0), "ROUND(IRR(C1:C4), 4)", CellValue::Number(0.1634)),
            // Explicit initial guess
            ((0, 210, 1), "ROUND(IRR(C1:C4, 0.2), 4)", CellValue::Number(0.1634)),
            // Simple doubling investment (-100, 110) -> 10%
            ((0, 210, 2), "ROUND(IRR({-100, 110}), 2)", CellValue::Number(0.10)),
            // Error: all positive cash flows (no sign change) yields #NUM!
            ((0, 210, 3), "IRR(E1:E4)", CellValue::Error(CellError::Num)),
            // Error: all negative cash flows yields #NUM!
            ((0, 210, 4), "IRR(F1:F4)", CellValue::Error(CellError::Num)),
            // Error: guess <= -1.0 yields #VALUE!
            ((0, 210, 5), "IRR(C1:C4, -1.0)", CellValue::Error(CellError::Value)),
            ((0, 210, 6), "IRR(C1:C4, -1.5)", CellValue::Error(CellError::Value)),
            // Error propagation in cash flows
            ((0, 210, 7), "IRR({-100, #REF!, 200})", CellValue::Error(CellError::Ref)),

            // ==========================================
            // 3. XNPV (Net Present Value for Non-Periodic Flows)
            // ==========================================
            // Normal irregular cash flows
            ((0, 220, 0), "ROUND(XNPV(0.1, C1:C4, D1:D4), 2)", CellValue::Number(3220.36)),
            // Zero discount rate yields #NUM!
            ((0, 220, 1), "XNPV(0, C1:C4, D1:D4)", CellValue::Error(CellError::Num)),
            // Negative discount rate
            ((0, 220, 2), "ROUND(XNPV(-0.02, C1:C4, D1:D4), 2)", CellValue::Number(4172.73)),
            // Error: dates not in chronological order (earlier date after start) yields #NUM!
            ((0, 220, 3), "XNPV(0.1, C1:C3, G1:G3)", CellValue::Error(CellError::Num)),
            // Error: mismatched array lengths yields #NUM!
            ((0, 220, 4), "XNPV(0.1, C1:C4, G1:G3)", CellValue::Error(CellError::Num)),
            // Error: fewer than 2 data points yields #NUM!
            ((0, 220, 5), "XNPV(0.1, C1:C1, D1:D1)", CellValue::Error(CellError::Num)),
            // Error: text values yield #VALUE!
            ((0, 220, 6), "XNPV(0.1, \"invalid\", D1:D4)", CellValue::Error(CellError::Value)),

            // ==========================================
            // 4. XIRR (Internal Rate of Return for Non-Periodic Flows)
            // ==========================================
            // Standard non-periodic investment -> positive rate (~79.23% annualized over 335 days)
            ((0, 230, 0), "ROUND(XIRR(C1:C4, D1:D4), 4)", CellValue::Number(0.7923)),
            // Simple exactly 1-year investment (-1000 at day 45292, 1100 at day 45657 = 365 days) -> 10%
            ((0, 230, 1), "ROUND(XIRR({-1000, 1100}, {45292, 45657}), 4)", CellValue::Number(0.1000)),
            // Error: all positive cash flows yields #NUM!
            ((0, 230, 2), "XIRR(E1:E4, D1:D4)", CellValue::Error(CellError::Num)),
            // Error: all negative cash flows yields #NUM!
            ((0, 230, 3), "XIRR(F1:F4, D1:D4)", CellValue::Error(CellError::Num)),
            // Error: mismatched lengths
            ((0, 230, 4), "XIRR(C1:C4, G1:G3)", CellValue::Error(CellError::Num)),

            // ==========================================
            // 5. MIRR (Modified Internal Rate of Return)
            // ==========================================
            // Investment with finance rate 10% and reinvestment rate 12%
            ((0, 240, 0), "ROUND(MIRR(C1:C4, 0.1, 0.12), 4)", CellValue::Number(0.1515)),
            // Higher reinvestment rate yields higher MIRR
            ((0, 240, 1), "ROUND(MIRR(C1:C4, 0.1, 0.18), 4)", CellValue::Number(0.168)),
            // Error: all positive cash flows yields #DIV/0!
            ((0, 240, 2), "MIRR(E1:E4, 0.1, 0.12)", CellValue::Error(CellError::Div0)),
            // Error: all negative cash flows yields #DIV/0!
            ((0, 240, 3), "MIRR(F1:F4, 0.1, 0.12)", CellValue::Error(CellError::Div0)),
            // Error: finance rate = -1.0 yields #DIV/0!
            ((0, 240, 4), "MIRR(C1:C4, -1, 0.12)", CellValue::Error(CellError::Div0)),

            // ==========================================
            // 6. TVM Functions Extreme Points & Due Variations
            // ==========================================
            // FV: linear relationship at rate = 0
            ((0, 250, 0), "FV(0, 10, -100, -1000)", CellValue::Number(2000.0)),
            // FV: due=1 vs due=0 (beginning of period compounds one extra period of payment)
            ((0, 250, 1), "ROUND(FV(0.05, 5, -200, -1000, 0), 2)", CellValue::Number(2381.41)),
            ((0, 250, 2), "ROUND(FV(0.05, 5, -200, -1000, 1), 2)", CellValue::Number(2436.66)),
            // FV: rate <= -1.0 and nper < 0 yields #NUM!
            ((0, 250, 3), "FV(-1.5, -2, -100, 0)", CellValue::Error(CellError::Num)),
            ((0, 250, 4), "FV(-1.0, 0, -100, 0)", CellValue::Error(CellError::Num)),

            // PV: linear relationship at rate = 0
            ((0, 250, 5), "PV(0, 10, -100, 1000)", CellValue::Number(0.0)),
            // PV: due=1 vs due=0
            ((0, 250, 6), "ROUND(PV(0.06, 5, -200, 1000, 0), 2)", CellValue::Number(95.21)),
            ((0, 250, 7), "ROUND(PV(0.06, 5, -200, 1000, 1), 2)", CellValue::Number(145.76)),
            // PV: (1+rate)^nper = 0 causes #DIV/0!
            ((0, 250, 8), "PV(-1.0, 2, 100, 0)", CellValue::Error(CellError::Div0)),

            // PMT: rate = 0 splits evenly
            ((0, 250, 9), "PMT(0, 12, 12000, 0)", CellValue::Number(-1000.0)),
            // PMT: due=1 vs due=0
            ((0, 250, 10), "ROUND(PMT(0.06, 5, 1000, 0, 0), 2)", CellValue::Number(-237.40)),
            ((0, 250, 11), "ROUND(PMT(0.06, 5, 1000, 0, 1), 2)", CellValue::Number(-223.96)),
            // PMT: nper = 0 yields #NUM!
            ((0, 250, 12), "PMT(0.05, 0, 1000)", CellValue::Error(CellError::Num)),
            // PMT: rate <= -1.0 yields #NUM!
            ((0, 250, 13), "PMT(-1.5, 10, 1000)", CellValue::Error(CellError::Num)),

            // PPMT & IPMT: due=1 gives zero interest in period 1
            ((0, 250, 14), "IPMT(0.05, 1, 10, 1000, 0, 1)", CellValue::Number(0.0)),
            // PPMT & IPMT: period out of range yields #NUM!
            ((0, 250, 15), "IPMT(0.05, 0, 10, 1000)", CellValue::Error(CellError::Num)),
            ((0, 250, 16), "IPMT(0.05, 11, 10, 1000)", CellValue::Error(CellError::Num)),
            ((0, 250, 17), "PPMT(0.05, -1, 10, 1000)", CellValue::Error(CellError::Num)),
            ((0, 250, 18), "PPMT(0.05, 15, 10, 1000)", CellValue::Error(CellError::Num)),

            // NPER: rate = 0
            ((0, 250, 19), "NPER(0, -100, 1000)", CellValue::Number(10.0)),
            // NPER: cannot pay off when payment is less than interest
            ((0, 250, 20), "NPER(0.05, -10, 1000)", CellValue::Error(CellError::Num)),
            // NPER: rate=0 and pmt=0 yields #NUM!
            ((0, 250, 21), "NPER(0, 0, 1000)", CellValue::Error(CellError::Num)),

            // RATE: convergence failure & parameter errors
            ((0, 250, 22), "RATE(0, -100, 1000)", CellValue::Error(CellError::Num)),
            ((0, 250, 23), "RATE(-5, -100, 1000)", CellValue::Error(CellError::Num)),
            ((0, 250, 24), "RATE(10, 100, 1000)", CellValue::Error(CellError::Num)),
            ((0, 250, 25), "RATE(10, -100, 1000, 0, 0, -1.0)", CellValue::Error(CellError::Value)),
            ((0, 250, 26), "RATE(10, -100, 1000, 0, 0, -1.5)", CellValue::Error(CellError::Value)),

            // ==========================================
            // 7. EFFECT & NOMINAL
            // ==========================================
            // Annual 12% compounded monthly
            ((0, 260, 0), "ROUND(EFFECT(0.12, 12), 6)", CellValue::Number(0.126825)),
            // Annual 12% compounded once
            ((0, 260, 1), "ROUND(EFFECT(0.12, 1), 6)", CellValue::Number(0.12)),
            // Compounded quarterly & daily
            ((0, 260, 2), "ROUND(EFFECT(0.05, 4), 6)", CellValue::Number(0.050945)),
            ((0, 260, 3), "ROUND(EFFECT(0.05, 365), 6)", CellValue::Number(0.051267)),
            // Errors: nominal_rate <= 0 or npery < 1 yields #NUM!
            ((0, 260, 4), "EFFECT(0, 12)", CellValue::Error(CellError::Num)),
            ((0, 260, 5), "EFFECT(-0.05, 12)", CellValue::Error(CellError::Num)),
            ((0, 260, 6), "EFFECT(0.12, 0)", CellValue::Error(CellError::Num)),
            ((0, 260, 7), "EFFECT(0.12, -4)", CellValue::Error(CellError::Num)),
            // NOMINAL roundtrip invariant
            ((0, 260, 8), "ROUND(NOMINAL(EFFECT(0.08, 4), 4), 6) = 0.08", CellValue::Bool(true)),
            ((0, 260, 9), "ROUND(NOMINAL(EFFECT(0.15, 12), 12), 6) = 0.15", CellValue::Bool(true)),
            // NOMINAL parameter errors
            ((0, 260, 10), "NOMINAL(0, 12)", CellValue::Error(CellError::Num)),
            ((0, 260, 11), "NOMINAL(-0.05, 12)", CellValue::Error(CellError::Num)),
            ((0, 260, 12), "NOMINAL(0.12, 0)", CellValue::Error(CellError::Num)),
            ((0, 260, 13), "NOMINAL(0.12, -2)", CellValue::Error(CellError::Num)),

            // ==========================================
            // 8. PDURATION & RRI
            // ==========================================
            // Rule of 72 check: doubling time at 7% is ~10.24 periods
            ((0, 270, 0), "ROUND(PDURATION(0.07, 1000, 2000), 2)", CellValue::Number(10.24)),
            ((0, 270, 1), "ROUND(PDURATION(0.10, 500, 5000), 2)", CellValue::Number(24.16)),
            // Errors: rate <= 0, pv <= 0, fv <= 0 yields #NUM!
            ((0, 270, 2), "PDURATION(0, 1000, 2000)", CellValue::Error(CellError::Num)),
            ((0, 270, 3), "PDURATION(-0.05, 1000, 2000)", CellValue::Error(CellError::Num)),
            ((0, 270, 4), "PDURATION(0.05, 0, 2000)", CellValue::Error(CellError::Num)),
            ((0, 270, 5), "PDURATION(0.05, -500, 2000)", CellValue::Error(CellError::Num)),
            ((0, 270, 6), "PDURATION(0.05, 1000, 0)", CellValue::Error(CellError::Num)),
            // RRI: CAGR calculation
            ((0, 270, 7), "ROUND(RRI(10, 1000, 2000), 4)", CellValue::Number(0.0718)),
            ((0, 270, 8), "ROUND(RRI(5, 1000, 1610.51), 4)", CellValue::Number(0.1000)),
            // RRI and PDURATION roundtrip invariant
            ((0, 270, 9), "ROUND(RRI(PDURATION(0.05, 500, 2500), 500, 2500), 6) = 0.05", CellValue::Bool(true)),
            // Errors: nper <= 0 or pv == 0 yields #NUM!
            ((0, 270, 10), "RRI(0, 1000, 2000)", CellValue::Error(CellError::Num)),
            ((0, 270, 11), "RRI(-2, 1000, 2000)", CellValue::Error(CellError::Num)),
            ((0, 270, 12), "RRI(10, 0, 2000)", CellValue::Error(CellError::Num)),

            // ==========================================
            // 9. FVSCHEDULE
            // ==========================================
            ((0, 280, 0), "ROUND(FVSCHEDULE(1000, H1:H3), 2)", CellValue::Number(1124.76)),
            ((0, 280, 1), "FVSCHEDULE(1000, {0.1, 0.2})", CellValue::Number(1320.0)),
            ((0, 280, 2), "FVSCHEDULE(1000, {0.1, #DIV/0!})", CellValue::Error(CellError::Div0)),

            // ==========================================
            // 10. Depreciation Functions (SLN, SYD, DDB, VDB)
            // ==========================================
            // SLN: straight line depreciation
            ((0, 290, 0), "SLN(10000, 1000, 5)", CellValue::Number(1800.0)),
            ((0, 290, 1), "SLN(50000, 5000, 10)", CellValue::Number(4500.0)),
            ((0, 290, 2), "SLN(10000, 1000, 0)", CellValue::Error(CellError::Div0)),

            // SYD: sum-of-years' digits depreciation
            ((0, 290, 3), "SYD(10000, 1000, 5, 1)", CellValue::Number(3000.0)),
            ((0, 290, 4), "SYD(10000, 1000, 5, 2)", CellValue::Number(2400.0)),
            ((0, 290, 5), "SYD(10000, 1000, 5, 3)", CellValue::Number(1800.0)),
            ((0, 290, 6), "SYD(10000, 1000, 5, 4)", CellValue::Number(1200.0)),
            ((0, 290, 7), "SYD(10000, 1000, 5, 5)", CellValue::Number(600.0)),
            // SYD total invariant: sum of all periods equals depreciable base (cost - salvage)
            ((0, 290, 8), "SYD(10000, 1000, 5, 1) + SYD(10000, 1000, 5, 2) + SYD(10000, 1000, 5, 3) + SYD(10000, 1000, 5, 4) + SYD(10000, 1000, 5, 5)", CellValue::Number(9000.0)),
            // SYD errors
            ((0, 290, 9), "SYD(10000, 1000, 5, 0)", CellValue::Error(CellError::Num)),
            ((0, 290, 10), "SYD(10000, 1000, 5, 6)", CellValue::Error(CellError::Num)),
            ((0, 290, 11), "SYD(10000, 1000, 0, 1)", CellValue::Error(CellError::Num)),
            ((0, 290, 12), "SYD(10000, -500, 5, 1)", CellValue::Error(CellError::Num)),

            // DDB: double-declining balance depreciation
            ((0, 290, 13), "ROUND(DDB(10000, 1000, 5, 1), 2)", CellValue::Number(4000.0)),
            ((0, 290, 14), "ROUND(DDB(10000, 1000, 5, 2), 2)", CellValue::Number(2400.0)),
            ((0, 290, 15), "ROUND(DDB(10000, 1000, 5, 3), 2)", CellValue::Number(1440.0)),
            ((0, 290, 16), "ROUND(DDB(10000, 1000, 5, 4), 2)", CellValue::Number(864.0)),
            ((0, 290, 17), "ROUND(DDB(10000, 1000, 5, 5), 2)", CellValue::Number(296.0)),
            // DDB errors
            ((0, 290, 18), "DDB(10000, 1000, 5, 0)", CellValue::Error(CellError::Num)),
            ((0, 290, 19), "DDB(10000, 1000, 5, 6)", CellValue::Error(CellError::Num)),
            ((0, 290, 20), "DDB(-1000, 100, 5, 1)", CellValue::Error(CellError::Num)),
            ((0, 290, 21), "DDB(10000, -100, 5, 1)", CellValue::Error(CellError::Num)),
            ((0, 290, 22), "DDB(10000, 1000, 5, 1, -1)", CellValue::Error(CellError::Num)),

            // VDB: variable declining balance depreciation
            ((0, 290, 23), "ROUND(VDB(10000, 1000, 5, 0, 1), 2)", CellValue::Number(4000.0)),
            ((0, 290, 24), "ROUND(VDB(10000, 1000, 5, 1, 2), 2)", CellValue::Number(2400.0)),
            // VDB errors
            ((0, 290, 25), "VDB(10000, 1000, 5, 2, 1)", CellValue::Error(CellError::Num)),
            ((0, 290, 26), "VDB(10000, 1000, 5, 0, 6)", CellValue::Error(CellError::Num)),
            ((0, 290, 27), "VDB(-1000, 100, 5, 0, 1)", CellValue::Error(CellError::Num)),

            // ==========================================
            // 11. DOLLARDE, DOLLARFR & ISPMT
            // ==========================================
            ((0, 300, 0), "DOLLARDE(1.02, 16)", CellValue::Number(1.125)),
            ((0, 300, 1), "DOLLARDE(1.04, 32)", CellValue::Number(1.125)),
            ((0, 300, 2), "DOLLARFR(1.125, 16)", CellValue::Number(1.02)),
            ((0, 300, 3), "DOLLARFR(1.125, 32)", CellValue::Number(1.04)),
            // Roundtrip invariant
            ((0, 300, 4), "ROUND(DOLLARFR(DOLLARDE(2.05, 8), 8), 2) = 2.05", CellValue::Bool(true)),
            // DOLLARDE & DOLLARFR errors
            ((0, 300, 5), "DOLLARDE(1.02, 0)", CellValue::Error(CellError::Div0)),
            ((0, 300, 6), "DOLLARDE(1.02, -16)", CellValue::Error(CellError::Num)),
            ((0, 300, 7), "DOLLARFR(1.125, 0)", CellValue::Error(CellError::Div0)),
            ((0, 300, 8), "DOLLARFR(1.125, -32)", CellValue::Error(CellError::Num)),
            // ISPMT: interest paid during a specific period
            ((0, 300, 9), "ISPMT(0.05, 1, 4, 10000000)", CellValue::Number(-375000.0)),
            ((0, 300, 10), "ISPMT(0.05, 1, 0, 1000)", CellValue::Error(CellError::Div0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_financial_functions_comprehensive_edge_cases() {
        let mut book = MockBook::new(false);

        // Seed cash flow and date streams for investment & return functions
        // Column A: A1..A4 for IRR, MIRR, NPV range: -100, 30, 40, 50
        book.values.insert((0, 0, 0), CellValue::Number(-100.0));
        book.values.insert((0, 1, 0), CellValue::Number(30.0));
        book.values.insert((0, 2, 0), CellValue::Number(40.0));
        book.values.insert((0, 3, 0), CellValue::Number(50.0));

        // Column B: B1..B4 all positive for IRR/MIRR domain error testing
        book.values.insert((0, 0, 1), CellValue::Number(100.0));
        book.values.insert((0, 1, 1), CellValue::Number(200.0));
        book.values.insert((0, 2, 1), CellValue::Number(300.0));
        book.values.insert((0, 3, 1), CellValue::Number(400.0));

        // Column C: C1..C4 all negative for IRR/MIRR domain error testing
        book.values.insert((0, 0, 2), CellValue::Number(-100.0));
        book.values.insert((0, 1, 2), CellValue::Number(-200.0));
        book.values.insert((0, 2, 2), CellValue::Number(-300.0));
        book.values.insert((0, 3, 2), CellValue::Number(-400.0));

        // Column D: D1..D3 cash flows for XNPV, XIRR: -1000, 500, 600
        book.values.insert((0, 0, 3), CellValue::Number(-1000.0));
        book.values.insert((0, 1, 3), CellValue::Number(500.0));
        book.values.insert((0, 2, 3), CellValue::Number(600.0));

        // Column E: E1..E3 serial dates for XNPV, XIRR: 44000, 44100, 44200
        book.values.insert((0, 0, 4), CellValue::Number(44000.0));
        book.values.insert((0, 1, 4), CellValue::Number(44100.0));
        book.values.insert((0, 2, 4), CellValue::Number(44200.0));

        // Column F: F1..F3 unordered dates for XNPV error testing: 44100, 44000, 44200
        book.values.insert((0, 0, 5), CellValue::Number(44100.0));
        book.values.insert((0, 1, 5), CellValue::Number(44000.0));
        book.values.insert((0, 2, 5), CellValue::Number(44200.0));

        // Column G: G1..G3 interest rates for FVSCHEDULE: 0.05, 0.04, 0.03
        book.values.insert((0, 0, 6), CellValue::Number(0.05));
        book.values.insert((0, 1, 6), CellValue::Number(0.04));
        book.values.insert((0, 2, 6), CellValue::Number(0.03));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Time value of money & Loan functions (PV, FV, PMT, NPER, RATE, IPMT, PPMT, ISPMT, CUMIPMT, CUMPRINC)
            ((0, 600, 0), "ROUND(PV(0.06/12, 120, -500), 2)", CellValue::Number(45036.73)),
            ((0, 600, 1), "ROUND(PV(0.05, 10, 0, -1000), 2)", CellValue::Number(613.91)),
            ((0, 600, 2), "ROUND(PV(0.05, 10, -100, -1000, 1), 2)", CellValue::Number(1424.70)),
            ((0, 600, 3), "PV(0, 10, -100, 0, 0)", CellValue::Number(1000.0)),
            ((0, 600, 4), "ROUND(FV(0.05, 10, -100, -1000, 0), 2)", CellValue::Number(2886.68)),
            ((0, 600, 5), "ROUND(FV(0.05, 10, -100, -1000, 1), 2)", CellValue::Number(2949.57)),
            ((0, 600, 6), "FV(0, 10, -100, -1000, 0)", CellValue::Number(2000.0)),
            ((0, 600, 7), "FV(-1.5, -2, 100, 100)", CellValue::Error(CellError::Num)),
            ((0, 600, 8), "FV(-1.0, 0, 100, 100)", CellValue::Error(CellError::Num)),

            ((0, 605, 0), "PMT(0, 10, 1000)", CellValue::Number(-100.0)),
            ((0, 605, 1), "PMT(0.05, 0, 1000)", CellValue::Error(CellError::Num)),
            ((0, 605, 2), "PMT(-1.5, 10, 1000)", CellValue::Error(CellError::Num)),
            ((0, 605, 3), "NPER(0, -100, 1000)", CellValue::Number(10.0)),
            ((0, 605, 4), "NPER(0, 0, 1000)", CellValue::Error(CellError::Num)),
            ((0, 605, 5), "NPER(0.05, 100, -3000)", CellValue::Error(CellError::Num)),
            ((0, 605, 6), "RATE(0, -100, 1000)", CellValue::Error(CellError::Num)),
            ((0, 605, 7), "RATE(10, -100, 1000, 0, 0, -1.5)", CellValue::Error(CellError::Value)),

            ((0, 610, 0), "IPMT(0.05, 1, 10, 1000, 0, 1)", CellValue::Number(0.0)),
            ((0, 610, 1), "ROUND(PPMT(0.05, 1, 10, 1000, 0, 1), 2) = ROUND(PMT(0.05, 10, 1000, 0, 1), 2)", CellValue::Bool(true)),
            ((0, 610, 2), "IPMT(0.05, 0, 10, 1000)", CellValue::Error(CellError::Num)),
            ((0, 610, 3), "IPMT(0.05, 11, 10, 1000)", CellValue::Error(CellError::Num)),
            ((0, 610, 4), "PPMT(0.05, 0, 10, 1000)", CellValue::Error(CellError::Num)),
            ((0, 610, 5), "PPMT(0.05, 11, 10, 1000)", CellValue::Error(CellError::Num)),
            ((0, 610, 6), "ISPMT(0.05, 2, 10, 1000)", CellValue::Number(-40.0)),
            ((0, 610, 7), "ISPMT(0.05, 10, 10, 1000)", CellValue::Number(0.0)),
            ((0, 610, 8), "ISPMT(0.05, 2, 0, 1000)", CellValue::Error(CellError::Div0)),

            ((0, 615, 0), "CUMIPMT(0.05, 10, 1000, 5, 2, 0)", CellValue::Error(CellError::Num)),
            ((0, 615, 1), "CUMIPMT(0.05, 10, 1000, 0, 5, 0)", CellValue::Error(CellError::Num)),
            ((0, 615, 2), "CUMIPMT(0.05, 10, 1000, 1, 11, 0)", CellValue::Error(CellError::Num)),
            ((0, 615, 3), "CUMIPMT(0.05, 10, 1000, 1, 5, 2)", CellValue::Error(CellError::Num)),
            ((0, 615, 4), "CUMIPMT(-0.01, 10, 1000, 1, 5, 0)", CellValue::Error(CellError::Num)),
            ((0, 615, 5), "CUMPRINC(0.05, 10, 1000, 5, 2, 0)", CellValue::Error(CellError::Num)),

            // 2. Cash flows & Investment returns (NPV, XNPV, IRR, XIRR, MIRR)
            ((0, 620, 0), "ROUND(NPV(0.1, 100, 200, 300), 4)", CellValue::Number(481.5928)),
            ((0, 620, 1), "NPV(-1, 100, 200)", CellValue::Error(CellError::Div0)),
            ((0, 620, 2), "ROUND(NPV(0.1, 100), 4)", CellValue::Number(90.9091)),
            ((0, 620, 3), "ROUND(NPV(0.1, A1:A4), 4)", CellValue::Number(-1.9124)),

            ((0, 625, 0), "ROUND(IRR(A1:A4), 4)", CellValue::Number(0.089)),
            ((0, 625, 1), "IRR(B1:B4)", CellValue::Error(CellError::Num)),
            ((0, 625, 2), "IRR(C1:C4)", CellValue::Error(CellError::Num)),
            ((0, 625, 3), "IRR(A1:A4, -1.5)", CellValue::Error(CellError::Value)),

            ((0, 630, 0), "ROUND(XNPV(0.09, D1:D3, E1:E3), 2)", CellValue::Number(60.66)),
            ((0, 630, 1), "XNPV(0, D1:D3, E1:E3)", CellValue::Error(CellError::Num)),
            ((0, 630, 2), "XNPV(0.09, D1:D2, E1:E3)", CellValue::Error(CellError::Num)),
            ((0, 630, 3), "XNPV(0.09, D1:D3, F1:F3)", CellValue::Error(CellError::Num)),

            ((0, 635, 0), "ROUND(XIRR(D1:D3, E1:E3), 4)", CellValue::Number(0.2539)),
            ((0, 635, 1), "XIRR(B1:B3, E1:E3)", CellValue::Error(CellError::Num)),
            ((0, 635, 2), "ROUND(MIRR(A1:A4, 0.1, 0.12), 4)", CellValue::Number(0.0982)),
            ((0, 635, 3), "MIRR(A1:A4, -1.0, 0.12)", CellValue::Error(CellError::Div0)),
            ((0, 635, 4), "MIRR(B1:B4, 0.1, 0.12)", CellValue::Error(CellError::Div0)),
            ((0, 635, 5), "MIRR(C1:C4, 0.1, 0.12)", CellValue::Error(CellError::Div0)),

            // 3. Rates, Conversions & Schedules (EFFECT, NOMINAL, PDURATION, RRI, FVSCHEDULE)
            ((0, 640, 0), "ROUND(EFFECT(0.05, 12), 6)", CellValue::Number(0.051162)),
            ((0, 640, 1), "EFFECT(0.05, 1)", CellValue::Number(0.05)),
            ((0, 640, 2), "EFFECT(0.05, 12.8) = EFFECT(0.05, 12)", CellValue::Bool(true)),
            ((0, 640, 3), "EFFECT(0, 12)", CellValue::Error(CellError::Num)),
            ((0, 640, 4), "EFFECT(-0.05, 12)", CellValue::Error(CellError::Num)),
            ((0, 640, 5), "EFFECT(0.05, 0)", CellValue::Error(CellError::Num)),

            ((0, 645, 0), "ROUND(NOMINAL(0.051161897881733, 12), 4)", CellValue::Number(0.05)),
            ((0, 645, 1), "NOMINAL(0.05, 1)", CellValue::Number(0.05)),
            ((0, 645, 2), "NOMINAL(0, 12)", CellValue::Error(CellError::Num)),
            ((0, 645, 3), "NOMINAL(0.05, 0)", CellValue::Error(CellError::Num)),
            ((0, 645, 4), "ROUND(NOMINAL(EFFECT(0.06, 4), 4), 6) = 0.06", CellValue::Bool(true)),

            ((0, 650, 0), "ROUND(PDURATION(0.05, 1000, 2000), 4)", CellValue::Number(14.2067)),
            ((0, 650, 1), "PDURATION(0, 1000, 2000)", CellValue::Error(CellError::Num)),
            ((0, 650, 2), "PDURATION(0.05, 0, 2000)", CellValue::Error(CellError::Num)),
            ((0, 650, 3), "PDURATION(0.05, 1000, 0)", CellValue::Error(CellError::Num)),

            ((0, 655, 0), "ROUND(RRI(10, 1000, 2000), 6)", CellValue::Number(0.071773)),
            ((0, 655, 1), "RRI(0, 1000, 2000)", CellValue::Error(CellError::Num)),
            ((0, 655, 2), "RRI(10, 0, 2000)", CellValue::Error(CellError::Num)),
            ((0, 655, 3), "ROUND(FVSCHEDULE(1000, G1:G3), 2)", CellValue::Number(1124.76)),
            ((0, 655, 4), "FVSCHEDULE(1000, G4:G4)", CellValue::Number(1000.0)),

            // 4. Depreciation functions (SLN, SYD, DB, DDB, VDB, AMORLINC, AMORDEGRC)
            ((0, 660, 0), "SLN(10000, 1000, 5)", CellValue::Number(1800.0)),
            ((0, 660, 1), "SLN(10000, 10000, 5)", CellValue::Number(0.0)),
            ((0, 660, 2), "SLN(10000, 1000, 0)", CellValue::Error(CellError::Div0)),

            ((0, 665, 0), "SYD(10000, 1000, 5, 1)", CellValue::Number(3000.0)),
            ((0, 665, 1), "SYD(10000, 1000, 5, 5)", CellValue::Number(600.0)),
            ((0, 665, 2), "SYD(10000, 1000, 5, 0)", CellValue::Error(CellError::Num)),
            ((0, 665, 3), "SYD(10000, 1000, 5, 6)", CellValue::Error(CellError::Num)),
            ((0, 665, 4), "SYD(10000, 1000, 0, 1)", CellValue::Error(CellError::Num)),
            ((0, 665, 5), "SYD(10000, -100, 5, 1)", CellValue::Error(CellError::Num)),

            ((0, 670, 0), "DB(10000, 1000, 5, 1)", CellValue::Number(3690.0)),
            ((0, 670, 1), "ROUND(DB(10000, 1000, 5, 2), 2)", CellValue::Number(2328.39)),
            ((0, 670, 2), "DB(10000, 1000, 5, 0)", CellValue::Error(CellError::Num)),
            ((0, 670, 3), "DB(10000, 1000, 5, 7)", CellValue::Error(CellError::Num)),
            ((0, 670, 4), "DB(10000, 1000, 5, 6, 12)", CellValue::Error(CellError::Num)),
            ((0, 670, 5), "DB(10000, 1000, 5, 1, 0)", CellValue::Error(CellError::Num)),
            ((0, 670, 6), "DB(10000, 1000, 5, 1, 13)", CellValue::Error(CellError::Num)),
            ((0, 670, 7), "DB(0, 0, 5, 1)", CellValue::Number(0.0)),

            ((0, 675, 0), "DDB(10000, 1000, 5, 1)", CellValue::Number(4000.0)),
            ((0, 675, 1), "DDB(10000, 1000, 5, 2)", CellValue::Number(2400.0)),
            ((0, 675, 2), "DDB(10000, 1000, 5, 1, 1.5)", CellValue::Number(3000.0)),
            ((0, 675, 3), "DDB(10000, 1000, 5, 0)", CellValue::Error(CellError::Num)),
            ((0, 675, 4), "DDB(10000, 1000, 5, 6)", CellValue::Error(CellError::Num)),
            ((0, 675, 5), "DDB(10000, 1000, 5, 1, 0)", CellValue::Error(CellError::Num)),
            ((0, 675, 6), "DDB(-1000, 100, 5, 1)", CellValue::Error(CellError::Num)),
            ((0, 675, 7), "DDB(10000, -100, 5, 1)", CellValue::Error(CellError::Num)),

            ((0, 680, 0), "VDB(10000, 1000, 5, 0, 1)", CellValue::Number(4000.0)),
            ((0, 680, 1), "VDB(10000, 1000, 5, 0, 1, 2, FALSE)", CellValue::Number(4000.0)),
            ((0, 680, 2), "VDB(10000, 1000, 5, 2, 1)", CellValue::Error(CellError::Num)),
            ((0, 680, 3), "VDB(10000, 1000, 5, -1, 1)", CellValue::Error(CellError::Num)),
            ((0, 680, 4), "VDB(10000, 1000, 5, 0, 6)", CellValue::Error(CellError::Num)),
            ((0, 680, 5), "VDB(10000, 1000, 5, 0, 1, -1)", CellValue::Error(CellError::Num)),
            ((0, 680, 6), "VDB(1000, 2000, 5, 0, 1)", CellValue::Number(-1000.0)),

            ((0, 685, 0), "ROUND(AMORLINC(10000, 44000, 44100, 1000, 0, 0.2, 0), 2)", CellValue::Number(544.44)),
            ((0, 685, 1), "AMORLINC(10000, 44000, 44100, 1000, 1, 0.2, 0)", CellValue::Number(2000.0)),
            ((0, 685, 2), "AMORLINC(10000, 44100, 44000, 1000, 1, 0.2, 0)", CellValue::Error(CellError::Num)),
            ((0, 685, 3), "AMORLINC(10000, 44000, 44100, 1000, 1, 0.2, 2)", CellValue::Error(CellError::Num)),
            ((0, 685, 4), "AMORLINC(10000, 44000, 44100, 1000, 1, 0.2, 5)", CellValue::Error(CellError::Num)),
            ((0, 685, 5), "AMORLINC(10000, 44000, 44100, 1000, 1, 0, 0)", CellValue::Error(CellError::Num)),
            ((0, 685, 6), "AMORDEGRC(10000, 44000, 44100, 1000, 1, 0.5, 0)", CellValue::Error(CellError::Num)),

            // 5. Securities, Bonds & Treasury Bills
            ((0, 690, 0), "DOLLARDE(1.02, 16)", CellValue::Number(1.125)),
            ((0, 690, 1), "DOLLARFR(1.125, 16)", CellValue::Number(1.02)),
            ((0, 690, 2), "DOLLARDE(1.02, 0)", CellValue::Error(CellError::Div0)),
            ((0, 690, 3), "DOLLARDE(1.02, -16)", CellValue::Error(CellError::Num)),
            ((0, 690, 4), "DOLLARFR(1.125, 0)", CellValue::Error(CellError::Div0)),
            ((0, 690, 5), "DOLLARFR(1.125, -16)", CellValue::Error(CellError::Num)),

            ((0, 695, 0), "TBILLPRICE(44000, 44090, 0.05)", CellValue::Number(98.75)),
            ((0, 695, 1), "ROUND(TBILLYIELD(44000, 44090, 98.75), 6)", CellValue::Number(0.050633)),
            ((0, 695, 2), "ROUND(TBILLEQ(44000, 44090, 0.05), 6)", CellValue::Number(0.051336)),
            ((0, 695, 3), "TBILLPRICE(44000, 44000, 0.05)", CellValue::Error(CellError::Num)),
            ((0, 695, 4), "TBILLPRICE(44000, 44400, 0.05)", CellValue::Error(CellError::Num)),
            ((0, 695, 5), "TBILLPRICE(44000, 44090, 0)", CellValue::Error(CellError::Num)),
            ((0, 695, 6), "TBILLYIELD(44000, 44090, 0)", CellValue::Error(CellError::Num)),

            ((0, 700, 0), "DISC(44000, 44360, 95, 100, 2)", CellValue::Number(0.05)),
            ((0, 700, 1), "INTRATE(44000, 44360, 1000, 1050, 2)", CellValue::Number(0.05)),
            ((0, 700, 2), "RECEIVED(44000, 44360, 950, 0.05, 2)", CellValue::Number(1000.0)),
            ((0, 700, 3), "PRICEDISC(44000, 44360, 0.05, 100, 2)", CellValue::Number(95.0)),
            ((0, 700, 4), "ROUND(YIELDDISC(44000, 44360, 95, 100, 2), 6)", CellValue::Number(0.052632)),
            ((0, 700, 5), "DISC(44000, 44000, 95, 100, 2)", CellValue::Error(CellError::Num)),
            ((0, 700, 6), "INTRATE(44000, 44000, 1000, 1050, 2)", CellValue::Error(CellError::Num)),
            ((0, 700, 7), "RECEIVED(44000, 44000, 950, 0.05, 2)", CellValue::Error(CellError::Num)),
            ((0, 700, 8), "PRICEDISC(44000, 44000, 0.05, 100, 2)", CellValue::Error(CellError::Num)),
            ((0, 700, 9), "YIELDDISC(44000, 44000, 95, 100, 2)", CellValue::Error(CellError::Num)),
            ((0, 700, 10), "DISC(44000, 44360, 95, 100, 5)", CellValue::Error(CellError::Num)),
            ((0, 700, 11), "RECEIVED(44000, 44360, 950, 1.5, 2)", CellValue::Error(CellError::Num)),

            ((0, 705, 0), "PRICEMAT(44000, 44360, 44000, 0.05, 0.05, 2)", CellValue::Number(100.0)),
            ((0, 705, 1), "ROUND(YIELDMAT(44000, 44360, 44000, 0.05, 100, 2), 6)", CellValue::Number(0.05)),
            ((0, 705, 2), "ACCRINTM(44000, 44360, 0.05, 1000, 2)", CellValue::Number(50.0)),
            ((0, 705, 3), "ACCRINTM(44000, 44000, 0.05, 1000, 2)", CellValue::Number(0.0)),
            ((0, 705, 4), "PRICEMAT(44000, 44000, 44000, 0.05, 0.05, 2)", CellValue::Error(CellError::Num)),
            ((0, 705, 5), "PRICEMAT(44000, 44360, 44050, 0.05, 0.05, 2)", CellValue::Error(CellError::Num)),
            ((0, 705, 6), "ACCRINTM(44050, 44000, 0.05, 1000, 2)", CellValue::Error(CellError::Num)),
            ((0, 705, 7), "ACCRINTM(44000, 44360, 0, 1000, 2)", CellValue::Error(CellError::Num)),
            ((0, 705, 8), "ACCRINTM(44000, 44360, 0.05, 0, 2)", CellValue::Error(CellError::Num)),

            ((0, 710, 0), "ROUND(PRICE(44000, 45828, 0.05, 0.05, 100, 2, 0), 2)", CellValue::Number(100.0)),
            ((0, 710, 1), "ROUND(YIELD(44000, 45828, 0.05, 100, 100, 2, 0), 2)", CellValue::Number(0.05)),
            ((0, 710, 2), "PRICE(44000, 44000, 0.05, 0.05, 100, 2, 0)", CellValue::Error(CellError::Num)),
            ((0, 710, 3), "PRICE(44000, 45828, -0.01, 0.05, 100, 2, 0)", CellValue::Error(CellError::Num)),
            ((0, 710, 4), "PRICE(44000, 45828, 0.05, -0.01, 100, 2, 0)", CellValue::Error(CellError::Num)),
            ((0, 710, 5), "PRICE(44000, 45828, 0.05, 0.05, 0, 2, 0)", CellValue::Error(CellError::Num)),
            ((0, 710, 6), "PRICE(44000, 45828, 0.05, 0.05, 100, 3, 0)", CellValue::Error(CellError::Num)),
            ((0, 710, 7), "PRICE(44000, 45828, 0.05, 0.05, 100, 2, 5)", CellValue::Error(CellError::Num)),
            ((0, 710, 8), "ROUND(MDURATION(44000, 45828, 0.05, 0.05, 2, 0) * (1 + 0.05/2), 6) = ROUND(DURATION(44000, 45828, 0.05, 0.05, 2, 0), 6)", CellValue::Bool(true)),
            ((0, 710, 9), "DURATION(44000, 45828, -0.01, 0.05, 2, 0)", CellValue::Error(CellError::Num)),
            ((0, 710, 10), "DURATION(44000, 45828, 0.05, -0.01, 2, 0)", CellValue::Error(CellError::Num)),

            ((0, 715, 0), "COUPDAYS(44002, 45828, 2, 0)", CellValue::Number(180.0)),
            ((0, 715, 1), "COUPDAYS(44002, 45828, 1, 0)", CellValue::Number(360.0)),
            ((0, 715, 2), "COUPDAYS(44002, 45828, 4, 0)", CellValue::Number(90.0)),
            ((0, 715, 3), "COUPDAYS(44002, 45828, 2, 3)", CellValue::Number(182.5)),
            ((0, 715, 4), "COUPNUM(44002, 45828, 2, 0)", CellValue::Number(10.0)),
            ((0, 715, 5), "COUPNUM(44002, 45828, 1, 0)", CellValue::Number(5.0)),
            ((0, 715, 6), "COUPNUM(44002, 45828, 4, 0)", CellValue::Number(20.0)),
            ((0, 715, 7), "COUPNUM(44002, 44002, 2, 0)", CellValue::Error(CellError::Num)),
            ((0, 715, 8), "COUPNUM(44002, 45828, 3, 0)", CellValue::Error(CellError::Num)),
            ((0, 715, 9), "COUPNUM(44002, 45828, 2, 5)", CellValue::Error(CellError::Num)),
            ((0, 715, 10), "COUPDAYBS(44002, 45828, 2, 0)", CellValue::Number(0.0)),
            ((0, 715, 11), "COUPDAYSNC(44002, 45828, 2, 0)", CellValue::Number(180.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_logical_information_and_metadata_functions() {
        let mut book = MockBook::new(false);

        // Sheet 0 setup:
        // A1: Number 100
        book.values.insert((0, 0, 0), CellValue::Number(100.0));
        // A2: Text "Hello"
        book.values.insert((0, 1, 0), CellValue::Text("Hello".into()));
        // A3: Logical TRUE
        book.values.insert((0, 2, 0), CellValue::Bool(true));
        // A4: Empty cell (CellValue::Empty)
        // A5: Error #DIV/0!
        book.values.insert((0, 4, 0), CellValue::Error(CellError::Div0));
        // A6: Error #N/A
        book.values.insert((0, 5, 0), CellValue::Error(CellError::NA));
        // A7: Error #VALUE!
        book.values.insert((0, 6, 0), CellValue::Error(CellError::Value));
        // A8: Empty string text ""
        book.values.insert((0, 7, 0), CellValue::Text("".into()));
        // A9: Number 0
        book.values.insert((0, 8, 0), CellValue::Number(0.0));
        // A10: Number -42
        book.values.insert((0, 9, 0), CellValue::Number(-42.0));

        // Column B setup:
        // B1: Number 1
        book.values.insert((0, 0, 1), CellValue::Number(1.0));
        // B2: Number 0
        book.values.insert((0, 1, 1), CellValue::Number(0.0));
        // B3: Number -1
        book.values.insert((0, 2, 1), CellValue::Number(-1.0));
        // B4: Text "TRUE"
        book.values.insert((0, 3, 1), CellValue::Text("TRUE".into()));
        // B5: Text "123"
        book.values.insert((0, 4, 1), CellValue::Text("123".into()));

        // Sheet 1 setup (Sheet2):
        book.sheets.push("Sheet2".to_string());
        book.values.insert((1, 0, 0), CellValue::Number(999.0));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // ==========================================
            // 1. Logical Functions: AND, OR, XOR, NOT
            // ==========================================
            // AND: all true / mixed
            ((0, 100, 0), "AND(TRUE, TRUE, TRUE)", CellValue::Bool(true)),
            ((0, 100, 1), "AND(TRUE, FALSE, TRUE)", CellValue::Bool(false)),
            ((0, 100, 2), "AND(1, 2, -5)", CellValue::Bool(true)),
            ((0, 100, 3), "AND(1, 0, 2)", CellValue::Bool(false)),
            ((0, 100, 4), "AND(A1, A3)", CellValue::Bool(true)),
            ((0, 100, 5), "AND(A1, A9)", CellValue::Bool(false)),
            // AND: ignores text and empty cells in arguments
            ((0, 100, 6), "AND(TRUE, \"test\", 1)", CellValue::Bool(true)),
            ((0, 100, 7), "AND(TRUE, A4, 1)", CellValue::Bool(true)),
            // AND: no valid logical values yields #VALUE!
            ((0, 100, 8), "AND(\"a\", \"b\")", CellValue::Error(CellError::Value)),
            // AND: error propagation
            ((0, 100, 9), "AND(TRUE, #DIV/0!)", CellValue::Error(CellError::Div0)),

            // OR: basic behavior
            ((0, 105, 0), "OR(FALSE, FALSE, FALSE)", CellValue::Bool(false)),
            ((0, 105, 1), "OR(FALSE, TRUE, FALSE)", CellValue::Bool(true)),
            ((0, 105, 2), "OR(0, 0, 0)", CellValue::Bool(false)),
            ((0, 105, 3), "OR(0, 5, 0)", CellValue::Bool(true)),
            ((0, 105, 4), "OR(A9, A1)", CellValue::Bool(true)),
            // OR: ignores text
            ((0, 105, 5), "OR(FALSE, \"text\", 0)", CellValue::Bool(false)),
            // OR: no valid logical values yields #VALUE!
            ((0, 105, 6), "OR(\"a\", \"b\")", CellValue::Error(CellError::Value)),

            // XOR: exclusive OR (true if odd number of true arguments)
            ((0, 110, 0), "XOR(TRUE, FALSE, FALSE)", CellValue::Bool(true)),
            ((0, 110, 1), "XOR(TRUE, TRUE, FALSE)", CellValue::Bool(false)),
            ((0, 110, 2), "XOR(TRUE, TRUE, TRUE)", CellValue::Bool(true)),
            ((0, 110, 3), "XOR(FALSE, FALSE, FALSE)", CellValue::Bool(false)),
            ((0, 110, 4), "XOR(1, 0, 1, 1)", CellValue::Bool(true)),

            // NOT: logical negation & type coercion
            ((0, 115, 0), "NOT(TRUE)", CellValue::Bool(false)),
            ((0, 115, 1), "NOT(FALSE)", CellValue::Bool(true)),
            ((0, 115, 2), "NOT(1)", CellValue::Bool(false)),
            ((0, 115, 3), "NOT(0)", CellValue::Bool(true)),
            ((0, 115, 4), "NOT(-10)", CellValue::Bool(false)),
            ((0, 115, 5), "NOT(\"TRUE\")", CellValue::Bool(false)),
            ((0, 115, 6), "NOT(\"FALSE\")", CellValue::Bool(true)),
            ((0, 115, 7), "NOT(\"invalid\")", CellValue::Error(CellError::Value)),

            // ==========================================
            // 2. Conditional & Branching: IF, IFERROR, IFNA, IFS, SWITCH
            // ==========================================
            // IF basic
            ((0, 120, 0), "IF(TRUE, 10, 20)", CellValue::Number(10.0)),
            ((0, 120, 1), "IF(FALSE, 10, 20)", CellValue::Number(20.0)),
            ((0, 120, 2), "IF(1, \"yes\", \"no\")", CellValue::Text("yes".into())),
            ((0, 120, 3), "IF(0, \"yes\", \"no\")", CellValue::Text("no".into())),
            // IF returning empty cell yields 0.0
            ((0, 120, 4), "IF(TRUE, A4, 99)", CellValue::Number(0.0)),
            // IF short-circuit: unexecuted branch with error is NOT evaluated
            ((0, 120, 5), "IF(TRUE, 100, 1/0)", CellValue::Number(100.0)),
            ((0, 120, 6), "IF(FALSE, 1/0, 200)", CellValue::Number(200.0)),
            // IF condition is error
            ((0, 120, 7), "IF(#DIV/0!, 1, 2)", CellValue::Error(CellError::Div0)),

            // IFERROR: catches all errors
            ((0, 125, 0), "IFERROR(100, 999)", CellValue::Number(100.0)),
            ((0, 125, 1), "IFERROR(#DIV/0!, 999)", CellValue::Number(999.0)),
            ((0, 125, 2), "IFERROR(#N/A, \"error\")", CellValue::Text("error".into())),
            ((0, 125, 3), "IFERROR(A4, 999)", CellValue::Number(0.0)),

            // IFNA: ONLY catches #N/A, passes through all other errors
            ((0, 130, 0), "IFNA(100, 999)", CellValue::Number(100.0)),
            ((0, 130, 1), "IFNA(#N/A, 999)", CellValue::Number(999.0)),
            ((0, 130, 2), "IFNA(#DIV/0!, 999)", CellValue::Error(CellError::Div0)),
            ((0, 130, 3), "IFNA(#VALUE!, \"na\")", CellValue::Error(CellError::Value)),

            // IFS: evaluates pairs in order
            ((0, 135, 0), "IFS(FALSE, 1, TRUE, 2, FALSE, 3)", CellValue::Number(2.0)),
            ((0, 135, 1), "IFS(A1>50, \"Big\", A1<=50, \"Small\")", CellValue::Text("Big".into())),
            // IFS: all false yields #N/A
            ((0, 135, 2), "IFS(FALSE, 1, FALSE, 2)", CellValue::Error(CellError::NA)),
            // IFS: odd number of arguments yields #VALUE!
            ((0, 135, 3), "IFS(TRUE, 1, FALSE)", CellValue::Error(CellError::Value)),

            // SWITCH: matches value to cases
            ((0, 140, 0), "SWITCH(2, 1, \"One\", 2, \"Two\", 3, \"Three\")", CellValue::Text("Two".into())),
            ((0, 140, 1), "SWITCH(\"B\", \"A\", 1, \"b\", 2, \"C\", 3)", CellValue::Number(2.0)),
            ((0, 140, 2), "SWITCH(5, 1, \"A\", 2, \"B\", \"Default\")", CellValue::Text("Default".into())),
            ((0, 140, 3), "SWITCH(5, 1, \"A\", 2, \"B\")", CellValue::Error(CellError::NA)),

            // ==========================================
            // 3. Information & Type Checking Functions
            // ==========================================
            // ISBLANK: only true for empty cells
            ((0, 150, 0), "ISBLANK(A4)", CellValue::Bool(true)),
            ((0, 150, 1), "ISBLANK(A1)", CellValue::Bool(false)),
            ((0, 150, 2), "ISBLANK(A8)", CellValue::Bool(false)),
            ((0, 150, 3), "ISBLANK(\"\")", CellValue::Bool(false)),

            // ISNUMBER
            ((0, 155, 0), "ISNUMBER(A1)", CellValue::Bool(true)),
            ((0, 155, 1), "ISNUMBER(B5)", CellValue::Bool(false)),
            ((0, 155, 2), "ISNUMBER(A3)", CellValue::Bool(false)),
            ((0, 155, 3), "ISNUMBER(A4)", CellValue::Bool(false)),

            // ISTEXT & ISNONTEXT
            ((0, 160, 0), "ISTEXT(A2)", CellValue::Bool(true)),
            ((0, 160, 1), "ISTEXT(A8)", CellValue::Bool(true)),
            ((0, 160, 2), "ISTEXT(B4)", CellValue::Bool(true)),
            ((0, 160, 3), "ISTEXT(A1)", CellValue::Bool(false)),
            ((0, 160, 4), "ISTEXT(A3)", CellValue::Bool(false)),
            ((0, 160, 5), "ISTEXT(A4)", CellValue::Bool(false)),
            ((0, 160, 6), "ISNONTEXT(A1)", CellValue::Bool(true)),
            ((0, 160, 7), "ISNONTEXT(A3)", CellValue::Bool(true)),
            ((0, 160, 8), "ISNONTEXT(A4)", CellValue::Bool(true)),
            ((0, 160, 9), "ISNONTEXT(A2)", CellValue::Bool(false)),

            // ISLOGICAL
            ((0, 165, 0), "ISLOGICAL(A3)", CellValue::Bool(true)),
            ((0, 165, 1), "ISLOGICAL(B4)", CellValue::Bool(false)),
            ((0, 165, 2), "ISLOGICAL(A1)", CellValue::Bool(false)),
            ((0, 165, 3), "ISLOGICAL(A4)", CellValue::Bool(false)),

            // ISERROR vs ISERR vs ISNA
            ((0, 170, 0), "ISERROR(A5)", CellValue::Bool(true)),
            ((0, 170, 1), "ISERROR(A6)", CellValue::Bool(true)),
            ((0, 170, 2), "ISERROR(A1)", CellValue::Bool(false)),
            ((0, 170, 3), "ISERR(A5)", CellValue::Bool(true)),
            ((0, 170, 4), "ISERR(A6)", CellValue::Bool(false)),
            ((0, 170, 5), "ISERR(A7)", CellValue::Bool(true)),
            ((0, 170, 6), "ISERR(A1)", CellValue::Bool(false)),
            ((0, 170, 7), "ISNA(A6)", CellValue::Bool(true)),
            ((0, 170, 8), "ISNA(A5)", CellValue::Bool(false)),
            ((0, 170, 9), "ISNA(A1)", CellValue::Bool(false)),

            // ISREF
            ((0, 175, 0), "ISREF(A1)", CellValue::Bool(true)),
            ((0, 175, 1), "ISREF(A1:B5)", CellValue::Bool(true)),
            ((0, 175, 2), "ISREF(123)", CellValue::Bool(false)),
            ((0, 175, 3), "ISREF(\"A1\")", CellValue::Bool(false)),
            ((0, 175, 4), "ISREF(INDIRECT(\"A1\"))", CellValue::Bool(true)),
            ((0, 175, 5), "ISREF(OFFSET(A1, 1, 1))", CellValue::Bool(true)),

            // ISEVEN & ISODD
            ((0, 180, 0), "ISEVEN(0)", CellValue::Bool(true)),
            ((0, 180, 1), "ISEVEN(2)", CellValue::Bool(true)),
            ((0, 180, 2), "ISEVEN(2.9)", CellValue::Bool(true)),
            ((0, 180, 3), "ISEVEN(-4)", CellValue::Bool(true)),
            ((0, 180, 4), "ISEVEN(3)", CellValue::Bool(false)),
            ((0, 180, 5), "ISODD(1)", CellValue::Bool(true)),
            ((0, 180, 6), "ISODD(3.8)", CellValue::Bool(true)),
            ((0, 180, 7), "ISODD(-5)", CellValue::Bool(true)),
            ((0, 180, 8), "ISODD(0)", CellValue::Bool(false)),
            ((0, 180, 9), "ISODD(2)", CellValue::Bool(false)),
            ((0, 180, 10), "ISEVEN(\"abc\")", CellValue::Error(CellError::Value)),
            ((0, 180, 11), "ISODD(\"abc\")", CellValue::Error(CellError::Value)),

            // ==========================================
            // 4. Metadata, Type Codes & Information: TYPE, ERROR.TYPE, N, NA, SHEET, SHEETS
            // ==========================================
            // TYPE: 1=number, 2=text, 4=bool, 16=error, 64=array
            ((0, 190, 0), "TYPE(A1)", CellValue::Number(1.0)),
            ((0, 190, 1), "TYPE(A4)", CellValue::Number(1.0)),
            ((0, 190, 2), "TYPE(A2)", CellValue::Number(2.0)),
            ((0, 190, 3), "TYPE(A8)", CellValue::Number(2.0)),
            ((0, 190, 4), "TYPE(A3)", CellValue::Number(4.0)),
            ((0, 190, 5), "TYPE(A5)", CellValue::Number(16.0)),
            ((0, 190, 6), "TYPE({1, 2, 3})", CellValue::Number(64.0)),

            // ERROR.TYPE: numeric error codes (1 to 7)
            ((0, 195, 0), "ERROR.TYPE(#NULL!)", CellValue::Number(1.0)),
            ((0, 195, 1), "ERROR.TYPE(#DIV/0!)", CellValue::Number(2.0)),
            ((0, 195, 2), "ERROR.TYPE(#VALUE!)", CellValue::Number(3.0)),
            ((0, 195, 3), "ERROR.TYPE(#REF!)", CellValue::Number(4.0)),
            ((0, 195, 4), "ERROR.TYPE(#NAME?)", CellValue::Number(5.0)),
            ((0, 195, 5), "ERROR.TYPE(#NUM!)", CellValue::Number(6.0)),
            ((0, 195, 6), "ERROR.TYPE(#N/A)", CellValue::Number(7.0)),
            // ERROR.TYPE on non-error value returns #N/A
            ((0, 195, 7), "ERROR.TYPE(A1)", CellValue::Error(CellError::NA)),
            ((0, 195, 8), "ERROR.TYPE(\"error\")", CellValue::Error(CellError::NA)),

            // N: converts value to number (text becomes 0)
            ((0, 200, 0), "N(A1)", CellValue::Number(100.0)),
            ((0, 200, 1), "N(A3)", CellValue::Number(1.0)),
            ((0, 200, 2), "N(FALSE)", CellValue::Number(0.0)),
            ((0, 200, 3), "N(A2)", CellValue::Number(0.0)),
            ((0, 200, 4), "N(A4)", CellValue::Number(0.0)),
            ((0, 200, 5), "N(#DIV/0!)", CellValue::Error(CellError::Div0)),

            // NA: returns #N/A error
            ((0, 205, 0), "NA()", CellValue::Error(CellError::NA)),

            // SHEET & SHEETS
            ((0, 210, 0), "SHEET()", CellValue::Number(1.0)),
            ((0, 210, 1), "SHEET(Sheet1!A1)", CellValue::Number(1.0)),
            ((0, 210, 2), "SHEET(Sheet2!A1)", CellValue::Number(2.0)),
            ((0, 210, 3), "SHEET(\"Sheet1\")", CellValue::Number(1.0)),
            ((0, 210, 4), "SHEET(\"Sheet2\")", CellValue::Number(2.0)),
            ((0, 210, 5), "SHEET(\"NonExistent\")", CellValue::Error(CellError::NA)),
            ((0, 210, 6), "SHEETS()", CellValue::Number(2.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_xml_web_and_lookup_reshaping_edge_cases() {
        let mut book = MockBook::new(false);

        // Seed 3x3 numeric grid at rows 800..802, cols 0..2
        // A801=1, B801=2, C801=3
        // A802=4, B802=5, C802=6
        // A803=7, B803=8, C803=9
        book.values.insert((0, 800, 0), CellValue::Number(1.0));
        book.values.insert((0, 800, 1), CellValue::Number(2.0));
        book.values.insert((0, 800, 2), CellValue::Number(3.0));
        book.values.insert((0, 801, 0), CellValue::Number(4.0));
        book.values.insert((0, 801, 1), CellValue::Number(5.0));
        book.values.insert((0, 801, 2), CellValue::Number(6.0));
        book.values.insert((0, 802, 0), CellValue::Number(7.0));
        book.values.insert((0, 802, 1), CellValue::Number(8.0));
        book.values.insert((0, 802, 2), CellValue::Number(9.0));

        // Column D (col 3): Text items for MATCH
        // D801="Apple", D802="Banana", D803="Cherry"
        book.values.insert((0, 800, 3), CellValue::Text("Apple".into()));
        book.values.insert((0, 801, 3), CellValue::Text("Banana".into()));
        book.values.insert((0, 802, 3), CellValue::Text("Cherry".into()));

        // Column E (col 4): Ascending numbers for MATCH type 1
        // E801=10, E802=20, E803=30
        book.values.insert((0, 800, 4), CellValue::Number(10.0));
        book.values.insert((0, 801, 4), CellValue::Number(20.0));
        book.values.insert((0, 802, 4), CellValue::Number(30.0));

        // Column F (col 5): Descending numbers for MATCH type -1
        // F801=30, F802=20, F803=10
        book.values.insert((0, 800, 5), CellValue::Number(30.0));
        book.values.insert((0, 801, 5), CellValue::Number(20.0));
        book.values.insert((0, 802, 5), CellValue::Number(10.0));

        // Column G (col 6): XML string in G801 for FILTERXML tests
        let xml_doc = "<root><a id='1'>Apple</a><a id='2'>Banana</a><b val='42'>Orange</b></root>";
        book.values.insert((0, 800, 6), CellValue::Text(xml_doc.into()));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Web & XML functions (FILTERXML, ENCODEURL, WEBSERVICE)
            ((0, 810, 0), "FILTERXML(G801, \"//b\")", CellValue::Text("Orange".into())),
            ((0, 810, 1), "FILTERXML(G801, \"//a[1]\")", CellValue::Text("Apple".into())),
            ((0, 810, 2), "FILTERXML(G801, \"//a[last()]\")", CellValue::Text("Banana".into())),
            ((0, 810, 3), "FILTERXML(G801, \"//b/@val\")", CellValue::Text("42".into())),
            ((0, 810, 4), "FILTERXML(G801, \"count(//a)\")", CellValue::Number(2.0)),
            ((0, 810, 5), "FILTERXML(G801, \"boolean(//b)\")", CellValue::Bool(true)),
            ((0, 810, 6), "FILTERXML(G801, \"boolean(//c)\")", CellValue::Bool(false)),
            ((0, 810, 7), "FILTERXML(G801, \"string-length(//b)\")", CellValue::Number(6.0)),
            ((0, 810, 8), "FILTERXML(\"<root><a>&lt;ok&gt;</a></root>\", \"//a\")", CellValue::Text("<ok>".into())),
            ((0, 810, 9), "FILTERXML(\"<root><a>&#65;&#66;&#67;</a></root>\", \"//a\")", CellValue::Text("ABC".into())),
            ((0, 810, 10), "FILTERXML(G801, \"//nonexistent\")", CellValue::Error(CellError::Value)),
            ((0, 810, 11), "FILTERXML(\"bad xml\", \"//a\")", CellValue::Error(CellError::Value)),
            ((0, 810, 12), "FILTERXML(\"\", \"//a\")", CellValue::Error(CellError::Value)),

            ((0, 815, 0), "ENCODEURL(\"Hello World\")", CellValue::Text("Hello%20World".into())),
            ((0, 815, 1), "ENCODEURL(\"a=1&b=2\")", CellValue::Text("a%3D1%26b%3D2".into())),
            ((0, 815, 2), "ENCODEURL(\"user@domain.com\")", CellValue::Text("user%40domain.com".into())),
            ((0, 815, 3), "ENCODEURL(\"test-1_2.3~4\")", CellValue::Text("test-1_2.3~4".into())),
            ((0, 815, 4), "ENCODEURL(\"東京\")", CellValue::Text("%E6%9D%B1%E4%BA%AC".into())),
            ((0, 815, 5), "ENCODEURL(\"\")", CellValue::Text("".into())),
            ((0, 815, 6), "ENCODEURL(123)", CellValue::Text("123".into())),
            ((0, 815, 7), "WEBSERVICE(\"ftp://example.com\")", CellValue::Error(CellError::Value)),
            ((0, 815, 8), "WEBSERVICE(\"file:///tmp/test\")", CellValue::Error(CellError::Value)),
            ((0, 815, 9), "WEBSERVICE(\"invalid_url\")", CellValue::Error(CellError::Value)),

            // 2. Cell Address, Structure & Indirection (ADDRESS, INDIRECT, OFFSET, AREAS, ROWS, COLUMNS, CHOOSE)
            ((0, 820, 0), "ADDRESS(1, 1)", CellValue::Text("$A$1".into())),
            ((0, 820, 1), "ADDRESS(1, 1, 1)", CellValue::Text("$A$1".into())),
            ((0, 820, 2), "ADDRESS(1, 1, 2)", CellValue::Text("A$1".into())),
            ((0, 820, 3), "ADDRESS(1, 1, 3)", CellValue::Text("$A1".into())),
            ((0, 820, 4), "ADDRESS(1, 1, 4)", CellValue::Text("A1".into())),
            ((0, 820, 5), "ADDRESS(5, 3)", CellValue::Text("$C$5".into())),
            ((0, 820, 6), "ADDRESS(1, 1, 1, FALSE)", CellValue::Text("R1C1".into())),
            ((0, 820, 7), "ADDRESS(1, 1, 4, FALSE)", CellValue::Text("R[1]C[1]".into())),
            ((0, 820, 8), "ADDRESS(1, 1, 1, TRUE, \"Sheet2\")", CellValue::Text("Sheet2!$A$1".into())),
            ((0, 820, 9), "ADDRESS(1, 1, 1, TRUE, \"My Sheet\")", CellValue::Text("'My Sheet'!$A$1".into())),
            ((0, 820, 10), "ADDRESS(0, 1)", CellValue::Error(CellError::Value)),
            ((0, 820, 11), "ADDRESS(1, 0)", CellValue::Error(CellError::Value)),
            ((0, 820, 12), "ADDRESS(1, 1, 0)", CellValue::Error(CellError::Value)),
            ((0, 820, 13), "ADDRESS(1, 1, 5)", CellValue::Error(CellError::Value)),
            ((0, 820, 14), "ADDRESS(1048577, 1)", CellValue::Error(CellError::Value)),
            ((0, 820, 15), "ADDRESS(1, 16385)", CellValue::Error(CellError::Value)),

            ((0, 825, 0), "ROWS(A801:C803)", CellValue::Number(3.0)),
            ((0, 825, 1), "COLUMNS(A801:C803)", CellValue::Number(3.0)),
            ((0, 825, 2), "ROWS({1, 2, 3; 4, 5, 6})", CellValue::Number(2.0)),
            ((0, 825, 3), "COLUMNS({1, 2, 3; 4, 5, 6})", CellValue::Number(3.0)),
            ((0, 825, 4), "ROWS(100)", CellValue::Number(1.0)),
            ((0, 825, 5), "COLUMNS(\"abc\")", CellValue::Number(1.0)),
            ((0, 825, 6), "AREAS(A801:C803)", CellValue::Number(1.0)),
            ((0, 825, 7), "AREAS((A801:B802, C801:C803))", CellValue::Number(2.0)),
            ((0, 825, 8), "AREAS((A801:B802, B802:C803, A803:C803))", CellValue::Number(3.0)),
            ((0, 825, 9), "AREAS(100)", CellValue::Error(CellError::Value)),

            ((0, 830, 0), "CHOOSE(1, \"gold\", \"silver\", \"bronze\")", CellValue::Text("gold".into())),
            ((0, 830, 1), "CHOOSE(2, \"gold\", \"silver\", \"bronze\")", CellValue::Text("silver".into())),
            ((0, 830, 2), "CHOOSE(3, \"gold\", \"silver\", \"bronze\")", CellValue::Text("bronze".into())),
            ((0, 830, 3), "CHOOSE(1.8, \"gold\", \"silver\", \"bronze\")", CellValue::Text("gold".into())),
            ((0, 830, 4), "CHOOSE(0, \"gold\", \"silver\", \"bronze\")", CellValue::Error(CellError::Value)),
            ((0, 830, 5), "CHOOSE(4, \"gold\", \"silver\", \"bronze\")", CellValue::Error(CellError::Value)),
            ((0, 830, 6), "CHOOSE(-1, \"gold\", \"silver\", \"bronze\")", CellValue::Error(CellError::Value)),

            ((0, 835, 0), "OFFSET(A801, 1, 2)", CellValue::Number(6.0)),
            ((0, 835, 1), "OFFSET(C803, -2, -2)", CellValue::Number(1.0)),
            ((0, 835, 2), "OFFSET(B802, -1, 1)", CellValue::Number(3.0)),
            ((0, 835, 3), "OFFSET(A1, -1, 0)", CellValue::Error(CellError::Ref)),
            ((0, 835, 4), "OFFSET(A1, 0, -1)", CellValue::Error(CellError::Ref)),
            ((0, 835, 5), "OFFSET(A801, 0, 0, 0, 1)", CellValue::Error(CellError::Ref)),
            ((0, 835, 6), "OFFSET(A801, 0, 0, 1, 0)", CellValue::Error(CellError::Ref)),
            ((0, 835, 7), "INDIRECT(\"B802\")", CellValue::Number(5.0)),
            ((0, 835, 8), "INDIRECT(\"C803\")", CellValue::Number(9.0)),
            ((0, 835, 9), "INDIRECT(\"R801C1\", FALSE)", CellValue::Number(1.0)),
            ((0, 835, 10), "INDIRECT(\"R802C2\", FALSE)", CellValue::Number(5.0)),
            ((0, 835, 11), "INDIRECT(\"invalid_cell_ref\")", CellValue::Error(CellError::Ref)),
            ((0, 835, 12), "INDIRECT(\"\")", CellValue::Error(CellError::Ref)),

            // 3. INDEX & MATCH
            ((0, 840, 0), "INDEX(A801:C803, 1, 1)", CellValue::Number(1.0)),
            ((0, 840, 1), "INDEX(A801:C803, 2, 3)", CellValue::Number(6.0)),
            ((0, 840, 2), "INDEX(A801:C803, 3, 2)", CellValue::Number(8.0)),
            ((0, 840, 3), "INDEX({10, 20; 30, 40}, 1, 2)", CellValue::Number(20.0)),
            ((0, 840, 4), "INDEX({10, 20; 30, 40}, 2, 1)", CellValue::Number(30.0)),
            ((0, 840, 5), "INDEX(A801:C803, 4, 1)", CellValue::Error(CellError::Ref)),
            ((0, 840, 6), "INDEX(A801:C803, 1, 4)", CellValue::Error(CellError::Ref)),
            ((0, 840, 7), "INDEX(A801:C803, -1, 1)", CellValue::Error(CellError::Value)),
            ((0, 840, 8), "MATCH(\"Banana\", D801:D803, 0)", CellValue::Number(2.0)),
            ((0, 840, 9), "MATCH(\"B*\", D801:D803, 0)", CellValue::Number(2.0)),
            ((0, 840, 10), "MATCH(\"?anana\", D801:D803, 0)", CellValue::Number(2.0)),
            ((0, 840, 11), "MATCH(25, E801:E803, 1)", CellValue::Number(2.0)),
            ((0, 840, 12), "MATCH(25, F801:F803, -1)", CellValue::Number(1.0)),
            ((0, 840, 13), "MATCH(\"Orange\", D801:D803, 0)", CellValue::Error(CellError::NA)),

            // 4. Array Reshaping & Dynamic Matrix Functions (WRAPROWS, WRAPCOLS, TOCOL, TOROW, EXPAND, TAKE, DROP, CHOOSEROWS, CHOOSECOLS, VSTACK, HSTACK)
            ((0, 845, 0), "INDEX(WRAPROWS({1, 2, 3, 4, 5, 6}, 3), 1, 2)", CellValue::Number(2.0)),
            ((0, 845, 1), "INDEX(WRAPROWS({1, 2, 3, 4, 5, 6}, 3), 2, 1)", CellValue::Number(4.0)),
            ((0, 845, 2), "INDEX(WRAPROWS({1, 2, 3, 4, 5}, 3, \"pad\"), 2, 3)", CellValue::Text("pad".into())),
            ((0, 845, 3), "WRAPROWS({1, 2, 3}, 0)", CellValue::Error(CellError::Num)),
            ((0, 845, 4), "WRAPROWS({1, 2, 3}, -1)", CellValue::Error(CellError::Num)),
            ((0, 845, 5), "WRAPROWS({1, 2; 3, 4}, 2)", CellValue::Error(CellError::Value)),

            ((0, 850, 0), "INDEX(WRAPCOLS({1, 2, 3, 4, 5, 6}, 2), 1, 2)", CellValue::Number(3.0)),
            ((0, 850, 1), "INDEX(WRAPCOLS({1, 2, 3, 4, 5}, 2, \"pad\"), 2, 3)", CellValue::Text("pad".into())),
            ((0, 850, 2), "WRAPCOLS({1, 2, 3}, 0)", CellValue::Error(CellError::Num)),
            ((0, 850, 3), "INDEX(TOCOL({1, 2; 3, 4}), 2, 1)", CellValue::Number(2.0)),
            ((0, 850, 4), "INDEX(TOCOL({1, 2; 3, 4}, 0, TRUE), 2, 1)", CellValue::Number(3.0)),
            ((0, 850, 5), "TOCOL({1, 2}, 4)", CellValue::Error(CellError::Value)),
            ((0, 850, 6), "INDEX(TOROW({1, 2; 3, 4}), 1, 2)", CellValue::Number(2.0)),
            ((0, 850, 7), "INDEX(TOROW({1, 2; 3, 4}, 0, TRUE), 1, 2)", CellValue::Number(3.0)),
            ((0, 850, 8), "TOROW({1, 2}, 4)", CellValue::Error(CellError::Value)),

            ((0, 855, 0), "INDEX(EXPAND({1, 2; 3, 4}, 3, 3, 0), 3, 3)", CellValue::Number(0.0)),
            ((0, 855, 1), "EXPAND({1, 2; 3, 4}, 1, 3)", CellValue::Error(CellError::Value)),
            ((0, 855, 2), "EXPAND({1, 2; 3, 4}, 3, 1)", CellValue::Error(CellError::Value)),
            ((0, 855, 3), "INDEX(TAKE({1, 2, 3; 4, 5, 6}, 1), 1, 3)", CellValue::Number(3.0)),
            ((0, 855, 4), "INDEX(TAKE({1, 2, 3; 4, 5, 6}, -1), 1, 1)", CellValue::Number(4.0)),
            ((0, 855, 5), "INDEX(TAKE({1, 2, 3; 4, 5, 6}, 2, 2), 2, 2)", CellValue::Number(5.0)),
            ((0, 855, 6), "TAKE({1, 2; 3, 4}, 0)", CellValue::Error(CellError::Value)),
            ((0, 855, 7), "INDEX(DROP({1, 2, 3; 4, 5, 6}, 1), 1, 1)", CellValue::Number(4.0)),
            ((0, 855, 8), "INDEX(DROP({1, 2, 3; 4, 5, 6}, -1), 1, 1)", CellValue::Number(1.0)),
            ((0, 855, 9), "DROP({1, 2; 3, 4}, 2)", CellValue::Error(CellError::Value)),

            ((0, 860, 0), "INDEX(CHOOSEROWS({10, 20; 30, 40; 50, 60}, 3, 1), 1, 1)", CellValue::Number(50.0)),
            ((0, 860, 1), "INDEX(CHOOSEROWS({10, 20; 30, 40; 50, 60}, -1), 1, 1)", CellValue::Number(50.0)),
            ((0, 860, 2), "CHOOSEROWS({10, 20}, 0)", CellValue::Error(CellError::Value)),
            ((0, 860, 3), "CHOOSEROWS({10, 20}, 2)", CellValue::Error(CellError::Value)),
            ((0, 860, 4), "INDEX(CHOOSECOLS({10, 20, 30; 40, 50, 60}, 3, 1), 1, 1)", CellValue::Number(30.0)),
            ((0, 860, 5), "INDEX(CHOOSECOLS({10, 20, 30; 40, 50, 60}, -1), 1, 1)", CellValue::Number(30.0)),
            ((0, 860, 6), "CHOOSECOLS({10, 20}, 0)", CellValue::Error(CellError::Value)),
            ((0, 860, 7), "CHOOSECOLS({10, 20}, 3)", CellValue::Error(CellError::Value)),
            ((0, 860, 8), "INDEX(VSTACK({1, 2}, {3, 4, 5}), 2, 3)", CellValue::Number(5.0)),
            ((0, 860, 9), "INDEX(VSTACK({1, 2}, {3, 4, 5}), 1, 3)", CellValue::Error(CellError::NA)),
            ((0, 860, 10), "INDEX(HSTACK({1; 2}, {3; 4; 5}), 3, 2)", CellValue::Number(5.0)),
            ((0, 860, 11), "INDEX(HSTACK({1; 2}, {3; 4; 5}), 3, 1)", CellValue::Error(CellError::NA)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_statistical_functions_comprehensive_edge_cases() {
        let mut book = MockBook::new(false);
        let sheet = 0;

        // Dataset A1:A5 = [10.0, 20.0, 30.0, 40.0, 50.0] (arithmetic sequence: sum=150, mean=30, var.p=200, var.s=250)
        book.values.insert((sheet, 0, 0), CellValue::Number(10.0));
        book.values.insert((sheet, 1, 0), CellValue::Number(20.0));
        book.values.insert((sheet, 2, 0), CellValue::Number(30.0));
        book.values.insert((sheet, 3, 0), CellValue::Number(40.0));
        book.values.insert((sheet, 4, 0), CellValue::Number(50.0));

        // Dataset B1:B5 = [10.0, 20.0, 20.0, 40.0, 50.0] (with duplicates: mode=20)
        book.values.insert((sheet, 0, 1), CellValue::Number(10.0));
        book.values.insert((sheet, 1, 1), CellValue::Number(20.0));
        book.values.insert((sheet, 2, 1), CellValue::Number(20.0));
        book.values.insert((sheet, 3, 1), CellValue::Number(40.0));
        book.values.insert((sheet, 4, 1), CellValue::Number(50.0));

        // Dataset C1:C5 = [10.0, 20.0, "text", TRUE, Empty] (mixed types)
        book.values.insert((sheet, 0, 2), CellValue::Number(10.0));
        book.values.insert((sheet, 1, 2), CellValue::Number(20.0));
        book.values.insert((sheet, 2, 2), CellValue::Text("text".to_string()));
        book.values.insert((sheet, 3, 2), CellValue::Bool(true));
        // C5 is left empty

        // Dataset D1:D5 = [1.0, 2.0, 3.0, 4.0, 5.0] (for regression with A1:A5, perfectly correlated)
        book.values.insert((sheet, 0, 3), CellValue::Number(1.0));
        book.values.insert((sheet, 1, 3), CellValue::Number(2.0));
        book.values.insert((sheet, 2, 3), CellValue::Number(3.0));
        book.values.insert((sheet, 3, 3), CellValue::Number(4.0));
        book.values.insert((sheet, 4, 3), CellValue::Number(5.0));

        // Dataset E1:E5 = [5.0, 4.0, 3.0, 2.0, 1.0] (inverse correlation with D1:D5)
        book.values.insert((sheet, 0, 4), CellValue::Number(5.0));
        book.values.insert((sheet, 1, 4), CellValue::Number(4.0));
        book.values.insert((sheet, 2, 4), CellValue::Number(3.0));
        book.values.insert((sheet, 3, 4), CellValue::Number(2.0));
        book.values.insert((sheet, 4, 4), CellValue::Number(1.0));

        // Dataset F1:F5 = [5.0, 5.0, 5.0, 5.0, 5.0] (zero variance)
        book.values.insert((sheet, 0, 5), CellValue::Number(5.0));
        book.values.insert((sheet, 1, 5), CellValue::Number(5.0));
        book.values.insert((sheet, 2, 5), CellValue::Number(5.0));
        book.values.insert((sheet, 3, 5), CellValue::Number(5.0));
        book.values.insert((sheet, 4, 5), CellValue::Number(5.0));

        // Dataset G1:G5 = [-10.0, 20.0, 30.0, 40.0, 50.0] (contains negative value)
        book.values.insert((sheet, 0, 6), CellValue::Number(-10.0));
        book.values.insert((sheet, 1, 6), CellValue::Number(20.0));
        book.values.insert((sheet, 2, 6), CellValue::Number(30.0));
        book.values.insert((sheet, 3, 6), CellValue::Number(40.0));
        book.values.insert((sheet, 4, 6), CellValue::Number(50.0));

        // Dataset H1:H5 = ["apple", "banana", "apple", "cherry", "APPLE"] (text for criteria)
        book.values.insert((sheet, 0, 7), CellValue::Text("apple".to_string()));
        book.values.insert((sheet, 1, 7), CellValue::Text("banana".to_string()));
        book.values.insert((sheet, 2, 7), CellValue::Text("apple".to_string()));
        book.values.insert((sheet, 3, 7), CellValue::Text("cherry".to_string()));
        book.values.insert((sheet, 4, 7), CellValue::Text("APPLE".to_string()));

        // Dataset I1:I5 = [100.0, 200.0, 300.0, 400.0, 500.0] (sum/average values)
        book.values.insert((sheet, 0, 8), CellValue::Number(100.0));
        book.values.insert((sheet, 1, 8), CellValue::Number(200.0));
        book.values.insert((sheet, 2, 8), CellValue::Number(300.0));
        book.values.insert((sheet, 3, 8), CellValue::Number(400.0));
        book.values.insert((sheet, 4, 8), CellValue::Number(500.0));

        // Dataset J1:J2 = [10.0, 20.0] (small sample: N=2)
        book.values.insert((sheet, 0, 9), CellValue::Number(10.0));
        book.values.insert((sheet, 1, 9), CellValue::Number(20.0));

        // Dataset K1 = [10.0] (single value: N=1)
        book.values.insert((sheet, 0, 10), CellValue::Number(10.0));

        // L1:L5 are left unpopulated (all empty)

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Basic statistics: AVERAGE, AVERAGEA, COUNT, COUNTA, COUNTBLANK, MAX, MAXA, MIN, MINA, MEDIAN
            ((0, 50, 0), "AVERAGE(A1:A5)", CellValue::Number(30.0)),
            ((0, 50, 1), "AVERAGE(C1:C5)", CellValue::Number(15.0)), // ignores text, bool, empty
            ((0, 50, 2), "AVERAGE(1, \"2\", TRUE)", CellValue::Number(4.0 / 3.0)), // direct args convert
            ((0, 50, 3), "AVERAGE(L1:L5)", CellValue::Error(CellError::Div0)),
            ((0, 50, 4), "AVERAGEA(A1:A5)", CellValue::Number(30.0)),
            ((0, 50, 5), "AVERAGEA(C1:C5)", CellValue::Number(31.0 / 4.0)), // 10+20+0+1 / 4 = 7.75
            ((0, 50, 6), "COUNT(A1:A5)", CellValue::Number(5.0)),
            ((0, 50, 7), "COUNT(C1:C5)", CellValue::Number(2.0)), // only numbers in range
            ((0, 50, 8), "COUNT(1, \"2\", \"abc\", TRUE, FALSE)", CellValue::Number(4.0)), // 1, "2", TRUE, FALSE count directly
            ((0, 50, 9), "COUNTA(A1:A5)", CellValue::Number(5.0)),
            ((0, 50, 10), "COUNTA(C1:C5)", CellValue::Number(4.0)), // non-empty cells
            ((0, 50, 11), "COUNTBLANK(C1:C5)", CellValue::Number(1.0)),
            ((0, 50, 12), "COUNTBLANK(L1:L5)", CellValue::Number(5.0)),
            ((0, 50, 13), "MAX(A1:A5)", CellValue::Number(50.0)),
            ((0, 50, 14), "MAX(C1:C5)", CellValue::Number(20.0)),
            ((0, 50, 15), "MAX(L1:L5)", CellValue::Number(0.0)), // empty range defaults to 0
            ((0, 50, 16), "MAXA(C1:C5)", CellValue::Number(20.0)),
            ((0, 50, 17), "MIN(A1:A5)", CellValue::Number(10.0)),
            ((0, 50, 18), "MIN(C1:C5)", CellValue::Number(10.0)),
            ((0, 50, 19), "MINA(C1:C5)", CellValue::Number(0.0)), // text counted as 0
            ((0, 50, 20), "MIN(L1:L5)", CellValue::Number(0.0)),
            ((0, 50, 21), "MEDIAN(A1:A5)", CellValue::Number(30.0)), // odd N: middle value
            ((0, 50, 22), "MEDIAN(1, 2, 3, 4)", CellValue::Number(2.5)), // even N: average of middle two
            ((0, 50, 23), "MEDIAN(C1:C5)", CellValue::Number(15.0)), // ignores non-numbers
            ((0, 50, 24), "MEDIAN(L1:L5)", CellValue::Error(CellError::Num)),

            // 2. Mode functions: MODE, MODE.SNGL
            ((0, 55, 0), "MODE(B1:B5)", CellValue::Number(20.0)),
            ((0, 55, 1), "_xlfn.MODE.SNGL(B1:B5)", CellValue::Number(20.0)),
            ((0, 55, 2), "MODE(A1:A5)", CellValue::Error(CellError::NA)), // no duplicates
            ((0, 55, 3), "MODE(1, 2, 2, 3, 3)", CellValue::Number(2.0)), // first occurring mode
            ((0, 55, 4), "MODE(L1:L5)", CellValue::Error(CellError::NA)), // empty range gives #N/A

            // 3. Rank, Position, and Quantiles: LARGE, SMALL, RANK, RANK.EQ, RANK.AVG, PERCENTILE, QUARTILE, PERCENTRANK
            ((0, 60, 0), "LARGE(A1:A5, 1)", CellValue::Number(50.0)),
            ((0, 60, 1), "LARGE(A1:A5, 5)", CellValue::Number(10.0)),
            ((0, 60, 2), "LARGE(A1:A5, 0)", CellValue::Error(CellError::Num)),
            ((0, 60, 3), "LARGE(A1:A5, 6)", CellValue::Error(CellError::Num)),
            ((0, 60, 4), "SMALL(A1:A5, 1)", CellValue::Number(10.0)),
            ((0, 60, 5), "SMALL(A1:A5, 5)", CellValue::Number(50.0)),
            ((0, 60, 6), "SMALL(A1:A5, 0)", CellValue::Error(CellError::Num)),
            ((0, 60, 7), "SMALL(A1:A5, 6)", CellValue::Error(CellError::Num)),
            ((0, 60, 8), "RANK(30, A1:A5)", CellValue::Number(3.0)), // descending by default
            ((0, 60, 9), "RANK(30, A1:A5, 0)", CellValue::Number(3.0)),
            ((0, 60, 10), "RANK(30, A1:A5, 1)", CellValue::Number(3.0)), // ascending
            ((0, 60, 11), "RANK(10, A1:A5, 0)", CellValue::Number(5.0)), // 10 is 5th in descending
            ((0, 60, 12), "RANK(10, A1:A5, 1)", CellValue::Number(1.0)), // 10 is 1st in ascending
            ((0, 60, 13), "RANK(25, A1:A5)", CellValue::Error(CellError::NA)), // not in array
            ((0, 60, 14), "_xlfn.RANK.EQ(20, B1:B5)", CellValue::Number(3.0)), // ties get top rank
            ((0, 60, 15), "_xlfn.RANK.AVG(20, B1:B5)", CellValue::Number(3.5)), // ties averaged: (3+4)/2 = 3.5
            ((0, 60, 16), "PERCENTILE(A1:A5, 0.0)", CellValue::Number(10.0)),
            ((0, 60, 17), "PERCENTILE(A1:A5, 1.0)", CellValue::Number(50.0)),
            ((0, 60, 18), "PERCENTILE(A1:A5, 0.5)", CellValue::Number(30.0)),
            ((0, 60, 19), "_xlfn.PERCENTILE.INC(A1:A5, 0.25)", CellValue::Number(20.0)),
            ((0, 60, 20), "PERCENTILE(A1:A5, -0.1)", CellValue::Error(CellError::Num)),
            ((0, 60, 21), "PERCENTILE(A1:A5, 1.1)", CellValue::Error(CellError::Num)),
            ((0, 60, 22), "_xlfn.PERCENTILE.EXC(A1:A5, 0.5)", CellValue::Number(30.0)),
            ((0, 60, 23), "_xlfn.PERCENTILE.EXC(A1:A5, 0.1)", CellValue::Error(CellError::Num)), // out of exc range
            ((0, 60, 24), "QUARTILE(A1:A5, 0)", CellValue::Number(10.0)),
            ((0, 60, 25), "QUARTILE(A1:A5, 1)", CellValue::Number(20.0)),
            ((0, 60, 26), "QUARTILE(A1:A5, 2)", CellValue::Number(30.0)),
            ((0, 60, 27), "QUARTILE(A1:A5, 3)", CellValue::Number(40.0)),
            ((0, 60, 28), "QUARTILE(A1:A5, 4)", CellValue::Number(50.0)),
            ((0, 60, 29), "QUARTILE(A1:A5, 5)", CellValue::Error(CellError::Num)),
            ((0, 60, 30), "_xlfn.QUARTILE.EXC(A1:A5, 0)", CellValue::Error(CellError::Num)),
            ((0, 60, 31), "_xlfn.QUARTILE.EXC(A1:A5, 2)", CellValue::Number(30.0)),
            ((0, 60, 32), "PERCENTRANK(A1:A5, 30)", CellValue::Number(0.5)),
            ((0, 60, 33), "PERCENTRANK(A1:A5, 10)", CellValue::Number(0.0)),
            ((0, 60, 34), "PERCENTRANK(A1:A5, 50)", CellValue::Number(1.0)),
            ((0, 60, 35), "PERCENTRANK(A1:A5, 5)", CellValue::Error(CellError::NA)), // out of bounds
            ((0, 60, 36), "PERCENTRANK(A1:A5, 55)", CellValue::Error(CellError::NA)),

            // 4. Dispersion, Variance, Standard Deviation, and Moments
            ((0, 70, 0), "VAR(A1:A5)", CellValue::Number(250.0)),
            ((0, 70, 1), "_xlfn.VAR.S(A1:A5)", CellValue::Number(250.0)),
            ((0, 70, 2), "VARP(A1:A5)", CellValue::Number(200.0)),
            ((0, 70, 3), "_xlfn.VAR.P(A1:A5)", CellValue::Number(200.0)),
            ((0, 70, 4), "_xlfn.VAR.S(K1)", CellValue::Error(CellError::Div0)), // N=1 gives #DIV/0!
            ((0, 70, 5), "_xlfn.VAR.P(K1)", CellValue::Number(0.0)), // N=1 population variance is 0
            ((0, 70, 6), "_xlfn.VAR.S(F1:F5)", CellValue::Number(0.0)), // zero variance
            ((0, 70, 7), "STDEV(A1:A5)", CellValue::Number(250.0f64.sqrt())),
            ((0, 70, 8), "_xlfn.STDEV.S(A1:A5)", CellValue::Number(250.0f64.sqrt())),
            ((0, 70, 9), "STDEVP(A1:A5)", CellValue::Number(200.0f64.sqrt())),
            ((0, 70, 10), "_xlfn.STDEV.P(A1:A5)", CellValue::Number(200.0f64.sqrt())),
            ((0, 70, 11), "_xlfn.STDEV.S(K1)", CellValue::Error(CellError::Div0)),
            ((0, 70, 12), "_xlfn.STDEV.P(K1)", CellValue::Number(0.0)),
            ((0, 70, 13), "AVEDEV(A1:A5)", CellValue::Number(12.0)),
            ((0, 70, 14), "AVEDEV(L1:L5)", CellValue::Error(CellError::Num)),
            ((0, 70, 15), "DEVSQ(A1:A5)", CellValue::Number(1000.0)),
            ((0, 70, 16), "DEVSQ(L1:L5)", CellValue::Error(CellError::Num)),
            ((0, 70, 17), "GEOMEAN(A1:A5)", CellValue::Number(12000000.0f64.powf(0.2))),
            ((0, 70, 18), "GEOMEAN(G1:G5)", CellValue::Error(CellError::Num)), // contains negative
            ((0, 70, 19), "GEOMEAN(L1:L5)", CellValue::Error(CellError::Num)),
            ((0, 70, 20), "HARMEAN(A1:A5)", CellValue::Number(5.0 / (0.1 + 0.05 + 1.0/30.0 + 0.025 + 0.02))),
            ((0, 70, 21), "HARMEAN(G1:G5)", CellValue::Error(CellError::Num)),
            ((0, 70, 22), "HARMEAN(L1:L5)", CellValue::Error(CellError::NA)), // HARMEAN on empty gives #N/A in Excel
            ((0, 70, 23), "SKEW(A1:A5)", CellValue::Number(0.0)), // symmetric arithmetic progression
            ((0, 70, 24), "SKEW(J1:J2)", CellValue::Error(CellError::Div0)), // N < 3
            ((0, 70, 25), "SKEW(F1:F5)", CellValue::Error(CellError::Div0)), // variance = 0
            ((0, 70, 26), "_xlfn.SKEW.P(A1:A5)", CellValue::Number(0.0)),
            ((0, 70, 27), "KURT(A1:A5)", CellValue::Number(-1.2)), // kurtosis of uniform progression
            ((0, 70, 28), "KURT(J1:J2)", CellValue::Error(CellError::Div0)), // N < 4
            ((0, 70, 29), "KURT(F1:F5)", CellValue::Error(CellError::Div0)), // variance = 0

            // 5. Bivariate Statistics, Correlation, Covariance, Regression
            ((0, 80, 0), "CORREL(D1:D5, A1:A5)", CellValue::Number(1.0)), // perfect positive correlation
            ((0, 80, 1), "CORREL(D1:D5, E1:E5)", CellValue::Number(-1.0)), // perfect negative correlation
            ((0, 80, 2), "CORREL(D1:D5, F1:F5)", CellValue::Error(CellError::Div0)), // zero variance in F
            ((0, 80, 3), "CORREL(D1:D5, J1:J2)", CellValue::Error(CellError::NA)), // mismatched array lengths
            ((0, 80, 4), "PEARSON(D1:D5, A1:A5)", CellValue::Number(1.0)),
            ((0, 80, 5), "RSQ(D1:D5, A1:A5)", CellValue::Number(1.0)),
            ((0, 80, 6), "RSQ(D1:D5, E1:E5)", CellValue::Number(1.0)), // (-1)^2 = 1
            ((0, 80, 7), "_xlfn.COVARIANCE.S(D1:D5, A1:A5)", CellValue::Number(25.0)),
            ((0, 80, 8), "_xlfn.COVARIANCE.P(D1:D5, A1:A5)", CellValue::Number(20.0)),
            ((0, 80, 9), "COVAR(D1:D5, A1:A5)", CellValue::Number(20.0)),
            ((0, 80, 10), "_xlfn.COVARIANCE.S(K1, K1)", CellValue::Error(CellError::Div0)), // N < 2
            ((0, 80, 11), "_xlfn.COVARIANCE.P(K1, K1)", CellValue::Number(0.0)),
            ((0, 80, 12), "SLOPE(A1:A5, D1:D5)", CellValue::Number(10.0)), // y=A, x=D: slope is 10
            ((0, 80, 13), "SLOPE(D1:D5, F1:F5)", CellValue::Error(CellError::Div0)), // x has zero variance
            ((0, 80, 14), "INTERCEPT(A1:A5, D1:D5)", CellValue::Number(0.0)),
            ((0, 80, 15), "INTERCEPT(E1:E5, D1:D5)", CellValue::Number(6.0)), // E = -1*D + 6
            ((0, 80, 16), "STEYX(A1:A5, D1:D5)", CellValue::Number(0.0)), // zero standard error for perfect fit
            ((0, 80, 17), "STEYX(J1:J2, J1:J2)", CellValue::Error(CellError::Div0)), // N < 3
            ((0, 80, 18), "FORECAST(6, A1:A5, D1:D5)", CellValue::Number(60.0)), // 10 * 6 = 60
            ((0, 80, 19), "_xlfn.FORECAST.LINEAR(0, A1:A5, D1:D5)", CellValue::Number(0.0)),

            // 6. Conditional Statistical Aggregations: COUNTIF, COUNTIFS, SUMIF, SUMIFS, AVERAGEIF, AVERAGEIFS, MAXIFS, MINIFS
            ((0, 90, 0), "COUNTIF(A1:A5, \">25\")", CellValue::Number(3.0)), // 30, 40, 50
            ((0, 90, 1), "COUNTIF(A1:A5, \"<=20\")", CellValue::Number(2.0)), // 10, 20
            ((0, 90, 2), "COUNTIF(A1:A5, \"<>30\")", CellValue::Number(4.0)),
            ((0, 90, 3), "COUNTIF(H1:H5, \"apple\")", CellValue::Number(3.0)), // case-insensitive: apple, apple, APPLE
            ((0, 90, 4), "COUNTIF(C1:C5, \"=\")", CellValue::Number(1.0)), // blank cells only
            ((0, 90, 5), "COUNTIF(C1:C5, \"<>\")", CellValue::Number(4.0)), // non-blank cells
            ((0, 90, 6), "COUNTIFS(A1:A5, \">15\", A1:A5, \"<45\")", CellValue::Number(3.0)), // 20, 30, 40
            ((0, 90, 7), "COUNTIFS(A1:A5, \">20\", D1:D5, \"<=4\")", CellValue::Number(2.0)), // A>20 and D<=4: (30,3) and (40,4)
            ((0, 90, 8), "SUMIF(H1:H5, \"apple\", I1:I5)", CellValue::Number(900.0)), // 100 + 300 + 500
            ((0, 90, 9), "SUMIF(A1:A5, \">20\")", CellValue::Number(120.0)), // 30 + 40 + 50
            ((0, 90, 10), "SUMIFS(I1:I5, H1:H5, \"apple\", A1:A5, \">=30\")", CellValue::Number(800.0)), // 300 + 500
            ((0, 90, 11), "AVERAGEIF(H1:H5, \"apple\", I1:I5)", CellValue::Number(300.0)), // 900 / 3
            ((0, 90, 12), "AVERAGEIF(A1:A5, \">100\")", CellValue::Error(CellError::Div0)), // no matching cells
            ((0, 90, 13), "AVERAGEIFS(I1:I5, H1:H5, \"apple\", A1:A5, \">=30\")", CellValue::Number(400.0)), // 800 / 2
            ((0, 90, 14), "_xlfn.MAXIFS(I1:I5, H1:H5, \"apple\")", CellValue::Number(500.0)),
            ((0, 90, 15), "_xlfn.MAXIFS(I1:I5, H1:H5, \"nonexistent\")", CellValue::Number(0.0)), // zero matching returns 0.0
            ((0, 90, 16), "_xlfn.MINIFS(I1:I5, H1:H5, \"apple\")", CellValue::Number(100.0)),
            ((0, 90, 17), "_xlfn.MINIFS(I1:I5, H1:H5, \"nonexistent\")", CellValue::Number(0.0)),

            // 7. Special and Transform Statistical Functions: STANDARDIZE, PERMUT, PERMUTATIONA, FISHER, FISHERINV, TRIMMEAN
            ((0, 100, 0), "STANDARDIZE(42, 30, 12)", CellValue::Number(1.0)),
            ((0, 100, 1), "STANDARDIZE(18, 30, 12)", CellValue::Number(-1.0)),
            ((0, 100, 2), "STANDARDIZE(42, 30, 0)", CellValue::Error(CellError::Num)), // stdev <= 0
            ((0, 100, 3), "STANDARDIZE(42, 30, -5)", CellValue::Error(CellError::Num)),
            ((0, 100, 4), "PERMUT(5, 2)", CellValue::Number(20.0)), // 5 * 4
            ((0, 100, 5), "PERMUT(5, 5)", CellValue::Number(120.0)),
            ((0, 100, 6), "PERMUT(5, 0)", CellValue::Number(1.0)),
            ((0, 100, 7), "PERMUT(5, 6)", CellValue::Error(CellError::Num)), // k > n
            ((0, 100, 8), "PERMUT(-1, 2)", CellValue::Error(CellError::Num)),
            ((0, 100, 9), "PERMUT(5, -1)", CellValue::Error(CellError::Num)),
            ((0, 100, 10), "_xlfn.PERMUTATIONA(5, 2)", CellValue::Number(25.0)), // 5^2
            ((0, 100, 11), "_xlfn.PERMUTATIONA(5, 0)", CellValue::Number(1.0)),
            ((0, 100, 12), "_xlfn.PERMUTATIONA(-1, 2)", CellValue::Error(CellError::Num)),
            ((0, 100, 13), "FISHER(0.75)", CellValue::Number(0.5 * (1.75f64 / 0.25f64).ln())),
            ((0, 100, 14), "FISHER(0.0)", CellValue::Number(0.0)),
            ((0, 100, 15), "FISHER(1.0)", CellValue::Error(CellError::Num)),
            ((0, 100, 16), "FISHER(-1.0)", CellValue::Error(CellError::Num)),
            ((0, 100, 17), "FISHER(1.5)", CellValue::Error(CellError::Num)),
            ((0, 100, 18), "FISHERINV(0.9729550745276566)", CellValue::Number(0.75)),
            ((0, 100, 19), "FISHERINV(0.0)", CellValue::Number(0.0)),
            ((0, 100, 20), "FISHERINV(400.0)", CellValue::Number(1.0)), // clamped to 1.0
            ((0, 100, 21), "FISHERINV(-400.0)", CellValue::Number(-1.0)), // clamped to -1.0
            ((0, 100, 22), "TRIMMEAN(A1:A5, 0.4)", CellValue::Number(30.0)), // floor(5*0.4/2) = 1 point trimmed from both sides -> A2:A4 mean = 30
            ((0, 100, 23), "TRIMMEAN(A1:A5, 0.0)", CellValue::Number(30.0)),
            ((0, 100, 24), "TRIMMEAN(A1:A5, 1.0)", CellValue::Error(CellError::Num)), // percent >= 1.0
            ((0, 100, 25), "TRIMMEAN(A1:A5, -0.1)", CellValue::Error(CellError::Num)), // percent < 0.0
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_probability_distributions_and_statistical_tests_edge_cases() {
        let mut book = MockBook::new(false);

        // Seed contingency table for CHISQ.TEST:
        // Observed frequencies in Sheet1!A900:B901 (rows 899..=900, cols 0..=1)
        book.values.insert((0, 899, 0), CellValue::Number(58.0));
        book.values.insert((0, 899, 1), CellValue::Number(35.0));
        book.values.insert((0, 900, 0), CellValue::Number(11.0));
        book.values.insert((0, 900, 1), CellValue::Number(25.0));
        // Expected frequencies in Sheet1!D900:E901 (rows 899..=900, cols 3..=4)
        book.values.insert((0, 899, 3), CellValue::Number(45.35));
        book.values.insert((0, 899, 4), CellValue::Number(47.65));
        book.values.insert((0, 900, 3), CellValue::Number(23.65));
        book.values.insert((0, 900, 4), CellValue::Number(24.85));

        // Sample data vectors for F.TEST, T.TEST, Z.TEST in Sheet1!A905:A909 and B905:B909 (rows 904..=908)
        // Sample 1: 3, 4, 5, 8, 9 (mean = 5.8, s^2 = 6.7)
        book.values.insert((0, 904, 0), CellValue::Number(3.0));
        book.values.insert((0, 905, 0), CellValue::Number(4.0));
        book.values.insert((0, 906, 0), CellValue::Number(5.0));
        book.values.insert((0, 907, 0), CellValue::Number(8.0));
        book.values.insert((0, 908, 0), CellValue::Number(9.0));

        // Sample 2: 5, 6, 7, 9, 10 (mean = 7.4, s^2 = 4.3)
        book.values.insert((0, 904, 1), CellValue::Number(5.0));
        book.values.insert((0, 905, 1), CellValue::Number(6.0));
        book.values.insert((0, 906, 1), CellValue::Number(7.0));
        book.values.insert((0, 907, 1), CellValue::Number(9.0));
        book.values.insert((0, 908, 1), CellValue::Number(10.0));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Normal, standard normal, lognormal distributions, PHI and GAUSS
            ((0, 910, 0), "ROUND(PHI(0), 4)", CellValue::Number(0.3989)),
            ((0, 910, 1), "ROUND(PHI(1), 4)", CellValue::Number(0.242)),
            ((0, 910, 2), "ROUND(PHI(-1), 4)", CellValue::Number(0.242)),
            ((0, 910, 3), "ROUND(GAUSS(0), 4)", CellValue::Number(0.0)),
            ((0, 910, 4), "ROUND(GAUSS(1), 4)", CellValue::Number(0.3413)),
            ((0, 910, 5), "ROUND(GAUSS(-1), 4)", CellValue::Number(-0.3413)),
            ((0, 910, 6), "ROUND(NORM.DIST(0, 0, 1, FALSE), 4)", CellValue::Number(0.3989)),
            ((0, 910, 7), "ROUND(NORM.DIST(0, 0, 1, TRUE), 4)", CellValue::Number(0.5)),
            ((0, 910, 8), "ROUND(NORM.DIST(1.96, 0, 1, TRUE), 4)", CellValue::Number(0.975)),
            ((0, 910, 9), "ROUND(NORM.DIST(-1.96, 0, 1, TRUE), 4)", CellValue::Number(0.025)),
            ((0, 910, 10), "ROUND(NORM.DIST(50, 50, 10, TRUE), 4)", CellValue::Number(0.5)),
            ((0, 910, 11), "ROUND(NORM.DIST(60, 50, 10, TRUE), 4)", CellValue::Number(0.8413)),
            ((0, 910, 12), "ROUND(NORM.DIST(40, 50, 10, TRUE), 4)", CellValue::Number(0.1587)),
            ((0, 910, 13), "ROUND(NORM.DIST(50, 50, 10, FALSE), 4)", CellValue::Number(0.0399)),
            ((0, 910, 14), "NORM.DIST(50, 50, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 910, 15), "NORM.DIST(50, 50, -1, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 910, 16), "NORMDIST(50, 50, 10, 1) = NORM.DIST(50, 50, 10, TRUE)", CellValue::Bool(true)),
            ((0, 910, 17), "ROUND(NORM.INV(0.5, 50, 10), 4)", CellValue::Number(50.0)),
            ((0, 910, 18), "ROUND(NORM.INV(0.8413447, 50, 10), 2)", CellValue::Number(60.0)),
            ((0, 910, 19), "ROUND(NORM.INV(0.1586553, 50, 10), 2)", CellValue::Number(40.0)),
            ((0, 910, 20), "NORM.INV(0, 50, 10)", CellValue::Error(CellError::Num)),
            ((0, 910, 21), "NORM.INV(1, 50, 10)", CellValue::Error(CellError::Num)),
            ((0, 910, 22), "NORM.INV(-0.01, 50, 10)", CellValue::Error(CellError::Num)),
            ((0, 910, 23), "NORM.INV(1.01, 50, 10)", CellValue::Error(CellError::Num)),
            ((0, 910, 24), "NORM.INV(0.5, 50, 0)", CellValue::Error(CellError::Num)),
            ((0, 910, 25), "NORM.INV(0.5, 50, -5)", CellValue::Error(CellError::Num)),
            ((0, 910, 26), "NORMINV(0.5, 50, 10) = NORM.INV(0.5, 50, 10)", CellValue::Bool(true)),
            ((0, 910, 27), "ROUND(NORM.S.DIST(0, TRUE), 4)", CellValue::Number(0.5)),
            ((0, 910, 28), "ROUND(NORM.S.DIST(0, FALSE), 4)", CellValue::Number(0.3989)),
            ((0, 910, 29), "ROUND(NORM.S.DIST(1, TRUE), 4)", CellValue::Number(0.8413)),
            ((0, 910, 30), "NORMSDIST(0) = NORM.S.DIST(0, TRUE)", CellValue::Bool(true)),
            ((0, 910, 31), "ROUND(NORM.S.INV(0.5), 4)", CellValue::Number(0.0)),
            ((0, 910, 32), "ROUND(NORM.S.INV(0.975), 4)", CellValue::Number(1.96)),
            ((0, 910, 33), "NORM.S.INV(0)", CellValue::Error(CellError::Num)),
            ((0, 910, 34), "NORM.S.INV(1)", CellValue::Error(CellError::Num)),
            ((0, 910, 35), "NORMSINV(0.5) = NORM.S.INV(0.5)", CellValue::Bool(true)),
            ((0, 910, 36), "ROUND(LOGNORM.DIST(1, 0, 1, TRUE), 4)", CellValue::Number(0.5)),
            ((0, 910, 37), "ROUND(LOGNORM.DIST(EXP(1), 0, 1, TRUE), 4)", CellValue::Number(0.8413)),
            ((0, 910, 38), "ROUND(LOGNORM.DIST(1, 0, 1, FALSE), 4)", CellValue::Number(0.3989)),
            ((0, 910, 39), "LOGNORM.DIST(0, 0, 1, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 910, 40), "LOGNORM.DIST(1, 0, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 910, 41), "ROUND(LOGNORM.INV(0.5, 0, 1), 4)", CellValue::Number(1.0)),

            // 2. Discrete distributions (Binomial, Negative Binomial, Poisson, Hypergeometric)
            ((0, 915, 0), "ROUND(BINOM.DIST(6, 10, 0.5, FALSE), 4)", CellValue::Number(0.2051)),
            ((0, 915, 1), "ROUND(BINOM.DIST(6, 10, 0.5, TRUE), 4)", CellValue::Number(0.8281)),
            ((0, 915, 2), "ROUND(BINOM.DIST(0, 10, 0.5, FALSE), 4)", CellValue::Number(0.001)),
            ((0, 915, 3), "BINOM.DIST(10, 10, 0.5, TRUE)", CellValue::Number(1.0)),
            ((0, 915, 4), "BINOM.DIST(11, 10, 0.5, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 5), "BINOM.DIST(-1, 10, 0.5, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 6), "BINOM.DIST(5, -10, 0.5, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 7), "BINOM.DIST(5, 10, -0.1, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 8), "BINOM.DIST(5, 10, 1.1, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 9), "BINOMDIST(6, 10, 0.5, 0) = BINOM.DIST(6, 10, 0.5, FALSE)", CellValue::Bool(true)),
            ((0, 915, 10), "ROUND(BINOM.DIST.RANGE(10, 0.5, 3, 6), 4)", CellValue::Number(0.7734)),
            ((0, 915, 11), "ROUND(BINOM.DIST.RANGE(10, 0.5, 6), 4)", CellValue::Number(0.2051)),
            ((0, 915, 12), "BINOM.DIST.RANGE(10, 0.5, 6, 3)", CellValue::Error(CellError::Num)),
            ((0, 915, 13), "BINOM.DIST.RANGE(10, 0.5, 3, 11)", CellValue::Error(CellError::Num)),
            ((0, 915, 14), "BINOM.INV(6, 0.5, 0.75)", CellValue::Number(4.0)),
            ((0, 915, 15), "BINOM.INV(1, 0.5, 0.5)", CellValue::Number(1.0)),
            ((0, 915, 16), "BINOM.INV(6, 0.5, 0)", CellValue::Error(CellError::Num)),
            ((0, 915, 17), "BINOM.INV(6, 0.5, 1)", CellValue::Error(CellError::Num)),
            ((0, 915, 18), "CRITBINOM(6, 0.5, 0.75) = BINOM.INV(6, 0.5, 0.75)", CellValue::Bool(true)),
            ((0, 915, 19), "ROUND(NEGBINOM.DIST(10, 3, 0.5, FALSE), 4)", CellValue::Number(0.0081)),
            ((0, 915, 20), "NEGBINOM.DIST(-1, 3, 0.5, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 21), "NEGBINOM.DIST(10, 0, 0.5, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 22), "NEGBINOM.DIST(10, 3, 0, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 23), "NEGBINOM.DIST(10, 3, 1, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 24), "NEGBINOMDIST(10, 3, 0.5) = NEGBINOM.DIST(10, 3, 0.5, FALSE)", CellValue::Bool(true)),
            ((0, 915, 25), "ROUND(POISSON.DIST(2, 5, FALSE), 4)", CellValue::Number(0.0842)),
            ((0, 915, 26), "ROUND(POISSON.DIST(2, 5, TRUE), 4)", CellValue::Number(0.1247)),
            ((0, 915, 27), "POISSON.DIST(-1, 5, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 28), "POISSON.DIST(2, -5, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 29), "POISSON(2, 5, TRUE) = POISSON.DIST(2, 5, TRUE)", CellValue::Bool(true)),
            ((0, 915, 30), "ROUND(HYPGEOM.DIST(1, 4, 8, 20, FALSE), 4)", CellValue::Number(0.3633)),
            ((0, 915, 31), "HYPGEOM.DIST(-1, 4, 8, 20, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 32), "HYPGEOM.DIST(5, 4, 8, 20, FALSE)", CellValue::Number(0.0)),
            ((0, 915, 33), "HYPGEOM.DIST(1, 4, 8, 0, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 34), "HYPGEOM.DIST(1, 25, 8, 20, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 915, 35), "HYPGEOMDIST(1, 4, 8, 20) = HYPGEOM.DIST(1, 4, 8, 20, FALSE)", CellValue::Bool(true)),

            // 3. Continuous distributions (Exponential, Gamma, Beta, Weibull)
            ((0, 920, 0), "ROUND(EXPON.DIST(2, 0.5, FALSE), 4)", CellValue::Number(0.1839)),
            ((0, 920, 1), "ROUND(EXPON.DIST(2, 0.5, TRUE), 4)", CellValue::Number(0.6321)),
            ((0, 920, 2), "EXPON.DIST(0, 0.5, TRUE)", CellValue::Number(0.0)),
            ((0, 920, 3), "EXPON.DIST(-1, 0.5, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 920, 4), "EXPON.DIST(2, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 920, 5), "EXPONDIST(2, 0.5, 1) = EXPON.DIST(2, 0.5, TRUE)", CellValue::Bool(true)),
            ((0, 920, 6), "ROUND(GAMMA(5), 4)", CellValue::Number(24.0)),
            ((0, 920, 7), "ROUND(GAMMA(0.5)^2, 4)", CellValue::Number(3.1416)),
            ((0, 920, 8), "GAMMA(0)", CellValue::Error(CellError::Num)),
            ((0, 920, 9), "GAMMA(-1)", CellValue::Error(CellError::Num)),
            ((0, 920, 10), "GAMMA(172)", CellValue::Error(CellError::Num)),
            ((0, 920, 11), "ROUND(GAMMALN(5), 4)", CellValue::Number(3.1781)),
            ((0, 920, 12), "ROUND(GAMMALN.PRECISE(5), 4)", CellValue::Number(3.1781)),
            ((0, 920, 13), "GAMMALN(0)", CellValue::Error(CellError::Num)),
            ((0, 920, 14), "GAMMALN(-2)", CellValue::Error(CellError::Num)),
            ((0, 920, 15), "GAMMA.DIST(-1, 2, 3, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 920, 16), "GAMMA.DIST(2, 0, 3, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 920, 17), "GAMMA.DIST(2, 2, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 920, 18), "GAMMA.INV(0, 2, 3)", CellValue::Number(0.0)),
            ((0, 920, 19), "GAMMA.INV(1, 2, 3)", CellValue::Error(CellError::Num)),
            ((0, 920, 20), "GAMMA.INV(-0.1, 2, 3)", CellValue::Error(CellError::Num)),
            ((0, 920, 21), "ROUND(BETA.DIST(0.5, 2, 2, TRUE), 4)", CellValue::Number(0.5)),
            ((0, 920, 22), "ROUND(BETA.DIST(0.5, 2, 2, FALSE), 4)", CellValue::Number(1.5)),
            ((0, 920, 23), "ROUND(BETA.DIST(2, 2, 2, TRUE, 1, 3), 4)", CellValue::Number(0.5)),
            ((0, 920, 24), "BETA.DIST(-0.1, 2, 2, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 920, 25), "BETA.DIST(1.1, 2, 2, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 920, 26), "BETA.DIST(0.5, 0, 2, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 920, 27), "BETA.DIST(0.5, 2, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 920, 28), "BETA.DIST(0.5, 2, 2, TRUE, 5, 2)", CellValue::Error(CellError::Num)),
            ((0, 920, 29), "BETADIST(0.5, 2, 2) = BETA.DIST(0.5, 2, 2, TRUE)", CellValue::Bool(true)),
            ((0, 920, 30), "ROUND(BETA.INV(0.5, 2, 2), 4)", CellValue::Number(0.5)),
            ((0, 920, 31), "ROUND(BETA.INV(0.5, 2, 2, 1, 3), 4)", CellValue::Number(2.0)),
            ((0, 920, 32), "BETA.INV(0, 2, 2)", CellValue::Error(CellError::Num)),
            ((0, 920, 33), "BETA.INV(1, 2, 2)", CellValue::Error(CellError::Num)),
            ((0, 920, 34), "ROUND(WEIBULL.DIST(2, 2, 3, FALSE), 4)", CellValue::Number(0.285)),
            ((0, 920, 35), "ROUND(WEIBULL.DIST(2, 2, 3, TRUE), 4)", CellValue::Number(0.3588)),
            ((0, 920, 36), "WEIBULL.DIST(0, 2, 3, FALSE)", CellValue::Number(0.0)),
            ((0, 920, 37), "ROUND(WEIBULL.DIST(0, 1, 3, FALSE), 4)", CellValue::Number(0.3333)),
            ((0, 920, 38), "WEIBULL.DIST(0, 0.5, 3, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 920, 39), "WEIBULL.DIST(-1, 2, 3, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 920, 40), "WEIBULL(2, 2, 3, 1) = WEIBULL.DIST(2, 2, 3, TRUE)", CellValue::Bool(true)),

            // 4. Sampling distributions (Chi-square, F, Student's t)
            ((0, 925, 0), "ROUND(CHISQ.DIST(2, 2, TRUE), 4)", CellValue::Number(0.6321)),
            ((0, 925, 1), "ROUND(CHISQ.DIST(2, 2, FALSE), 4)", CellValue::Number(0.1839)),
            ((0, 925, 2), "ROUND(CHISQ.DIST.RT(2, 2), 4)", CellValue::Number(0.3679)),
            ((0, 925, 3), "ROUND(CHISQ.DIST(2, 2, TRUE) + CHISQ.DIST.RT(2, 2), 4)", CellValue::Number(1.0)),
            ((0, 925, 4), "CHISQ.DIST(-1, 2, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 925, 5), "CHISQ.DIST(2, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 925, 6), "CHIDIST(2, 2) = CHISQ.DIST.RT(2, 2)", CellValue::Bool(true)),
            ((0, 925, 7), "CHISQ.INV(0, 2)", CellValue::Number(0.0)),
            ((0, 925, 8), "CHISQ.INV(1, 2)", CellValue::Error(CellError::Num)),
            ((0, 925, 9), "CHISQ.INV.RT(1, 2)", CellValue::Number(0.0)),
            ((0, 925, 10), "CHISQ.INV.RT(0, 2)", CellValue::Error(CellError::Num)),
            ((0, 925, 11), "ROUND(F.DIST(1, 2, 2, TRUE), 4)", CellValue::Number(0.5)),
            ((0, 925, 12), "ROUND(F.DIST(1, 2, 2, FALSE), 4)", CellValue::Number(0.25)),
            ((0, 925, 13), "ROUND(F.DIST.RT(1, 2, 2), 4)", CellValue::Number(0.5)),
            ((0, 925, 14), "F.DIST(-1, 2, 2, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 925, 15), "F.DIST(1, 0, 2, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 925, 16), "F.DIST(1, 2, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 925, 17), "FDIST(1, 2, 2) = F.DIST.RT(1, 2, 2)", CellValue::Bool(true)),
            ((0, 925, 18), "F.INV(0, 2, 2)", CellValue::Number(0.0)),
            ((0, 925, 19), "ROUND(F.INV(0.5, 2, 2), 4)", CellValue::Number(1.0)),
            ((0, 925, 20), "F.INV(1, 2, 2)", CellValue::Error(CellError::Num)),
            ((0, 925, 21), "F.INV.RT(1, 2, 2)", CellValue::Number(0.0)),
            ((0, 925, 22), "ROUND(F.INV.RT(0.5, 2, 2), 4)", CellValue::Number(1.0)),
            ((0, 925, 23), "F.INV.RT(0, 2, 2)", CellValue::Error(CellError::Num)),
            ((0, 925, 24), "ROUND(T.DIST(0, 5, TRUE), 4)", CellValue::Number(0.5)),
            ((0, 925, 25), "ROUND(T.DIST.RT(0, 5), 4)", CellValue::Number(0.5)),
            ((0, 925, 26), "ROUND(T.DIST.2T(0, 5), 4)", CellValue::Number(1.0)),
            ((0, 925, 27), "ROUND(T.DIST(1.5, 5, TRUE) + T.DIST.RT(1.5, 5), 4)", CellValue::Number(1.0)),
            ((0, 925, 28), "T.DIST(1, 0, TRUE)", CellValue::Error(CellError::Div0)),
            ((0, 925, 29), "T.DIST(1, -1, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 925, 30), "T.DIST.2T(-1, 5)", CellValue::Error(CellError::Num)),
            ((0, 925, 31), "TDIST(1.5, 5, 1) = T.DIST.RT(1.5, 5)", CellValue::Bool(true)),
            ((0, 925, 32), "TDIST(1.5, 5, 2) = T.DIST.2T(1.5, 5)", CellValue::Bool(true)),
            ((0, 925, 33), "TDIST(1.5, 5, 3)", CellValue::Error(CellError::Num)),
            ((0, 925, 34), "ROUND(T.INV(0.5, 5), 4)", CellValue::Number(0.0)),
            ((0, 925, 35), "ROUND(T.INV.2T(0.05, 5), 4)", CellValue::Number(2.5706)),
            ((0, 925, 36), "T.INV.2T(1, 5)", CellValue::Number(0.0)),
            ((0, 925, 37), "T.INV.2T(0, 5)", CellValue::Error(CellError::Num)),
            ((0, 925, 38), "TINV(0, 5)", CellValue::Error(CellError::Div0)),

            // 5. Statistical hypothesis tests and confidence intervals
            ((0, 930, 0), "ROUND(CHISQ.TEST(A900:B901, D900:E901), 4)", CellValue::Number(0.0002)),
            ((0, 930, 1), "CHISQ.TEST(A900:B900, D900:E901)", CellValue::Error(CellError::NA)),
            ((0, 930, 2), "CHISQ.TEST({1}, {1})", CellValue::Error(CellError::Div0)),
            ((0, 930, 3), "CHISQ.TEST({1, 2}, {0, 2})", CellValue::Error(CellError::Div0)),
            ((0, 930, 4), "CHITEST(A900:B901, D900:E901) = CHISQ.TEST(A900:B901, D900:E901)", CellValue::Bool(true)),
            ((0, 930, 5), "ROUND(F.TEST(A905:A909, B905:B909), 4)", CellValue::Number(0.6779)),
            ((0, 930, 6), "F.TEST(A905:A909, A905:A909)", CellValue::Number(1.0)),
            ((0, 930, 7), "F.TEST({1}, {2, 3})", CellValue::Error(CellError::Div0)),
            ((0, 930, 8), "F.TEST({1, 1, 1}, {2, 3, 4})", CellValue::Error(CellError::Div0)),
            ((0, 930, 9), "FTEST(A905:A909, B905:B909) = F.TEST(A905:A909, B905:B909)", CellValue::Bool(true)),
            ((0, 930, 10), "ROUND(T.TEST(A905:A909, B905:B909, 2, 1), 4)", CellValue::Number(0.0028)),
            ((0, 930, 11), "ROUND(T.TEST(A905:A909, B905:B909, 1, 1), 4)", CellValue::Number(0.0014)),
            ((0, 930, 12), "ROUND(T.TEST(A905:A909, B905:B909, 2, 2), 4)", CellValue::Number(0.3122)),
            ((0, 930, 13), "ROUND(T.TEST(A905:A909, B905:B909, 2, 3), 4)", CellValue::Number(0.3136)),
            ((0, 930, 14), "T.TEST(A905:A909, A905:A909, 2, 2)", CellValue::Number(1.0)),
            ((0, 930, 15), "T.TEST(A905:A909, A905:A909, 1, 2)", CellValue::Number(0.5)),
            ((0, 930, 16), "T.TEST({1, 2}, {1, 2, 3}, 1, 1)", CellValue::Error(CellError::NA)),
            ((0, 930, 17), "T.TEST({1, 2}, {1, 2}, 3, 1)", CellValue::Error(CellError::Num)),
            ((0, 930, 18), "T.TEST({1, 2}, {1, 2}, 1, 4)", CellValue::Error(CellError::Num)),
            ((0, 930, 19), "ROUND(Z.TEST(A905:A909, 5), 4)", CellValue::Number(0.2448)),
            ((0, 930, 20), "ROUND(Z.TEST(A905:A909, 5.8), 4)", CellValue::Number(0.5)),
            ((0, 930, 21), "Z.TEST({1}, 5)", CellValue::Error(CellError::Div0)),
            ((0, 930, 22), "Z.TEST(A905:A909, 5, 0)", CellValue::Error(CellError::Num)),
            ((0, 930, 23), "Z.TEST(A905:A909, 5, -2)", CellValue::Error(CellError::Num)),
            ((0, 930, 24), "ZTEST(A905:A909, 5) = Z.TEST(A905:A909, 5)", CellValue::Bool(true)),

            // 6. Direct array constant evaluations for hypothesis tests
            ((0, 935, 0), "ROUND(CHISQ.TEST({58, 35; 11, 25}, {45.35, 47.65; 23.65, 24.85}), 4)", CellValue::Number(0.0002)),
            ((0, 935, 1), "ROUND(F.TEST({3, 4, 5, 8, 9}, {5, 6, 7, 9, 10}), 4)", CellValue::Number(0.6779)),
            ((0, 935, 2), "ROUND(T.TEST({3, 4, 5, 8, 9}, {5, 6, 7, 9, 10}, 2, 1), 4)", CellValue::Number(0.0028)),
            ((0, 935, 3), "ROUND(T.TEST({3, 4, 5, 8, 9}, {5, 6, 7, 9, 10}, 2, 2), 4)", CellValue::Number(0.3122)),
            ((0, 935, 4), "ROUND(T.TEST({3, 4, 5, 8, 9}, {5, 6, 7, 9, 10}, 2, 3), 4)", CellValue::Number(0.3136)),
            ((0, 935, 5), "ROUND(Z.TEST({3, 4, 5, 8, 9}, 5), 4)", CellValue::Number(0.2448)),
            ((0, 935, 6), "ROUND(Z.TEST({3, 4, 5, 8, 9}, 5.8), 4)", CellValue::Number(0.5)),

            // 7. Confidence interval functions (CONFIDENCE, CONFIDENCE.NORM, CONFIDENCE.T)
            ((0, 940, 0), "ROUND(CONFIDENCE.NORM(0.05, 2.5, 50), 4)", CellValue::Number(0.693)),
            ((0, 940, 1), "CONFIDENCE.NORM(0, 2.5, 50)", CellValue::Error(CellError::Num)),
            ((0, 940, 2), "CONFIDENCE.NORM(1, 2.5, 50)", CellValue::Error(CellError::Num)),
            ((0, 940, 3), "CONFIDENCE.NORM(0.05, 0, 50)", CellValue::Error(CellError::Num)),
            ((0, 940, 4), "CONFIDENCE.NORM(0.05, -2, 50)", CellValue::Error(CellError::Num)),
            ((0, 940, 5), "CONFIDENCE.NORM(0.05, 2.5, 0)", CellValue::Error(CellError::Num)),
            ((0, 940, 6), "CONFIDENCE.NORM(0.05, 2.5, -5)", CellValue::Error(CellError::Num)),
            ((0, 940, 7), "CONFIDENCE(0.05, 2.5, 50) = CONFIDENCE.NORM(0.05, 2.5, 50)", CellValue::Bool(true)),
            ((0, 940, 8), "ROUND(CONFIDENCE.T(0.05, 2.5, 50), 4)", CellValue::Number(0.7105)),
            ((0, 940, 9), "CONFIDENCE.T(0.05, 2.5, 1)", CellValue::Error(CellError::Div0)),
            ((0, 940, 10), "CONFIDENCE.T(0.05, 2.5, 0)", CellValue::Error(CellError::Num)),
            ((0, 940, 11), "CONFIDENCE.T(0, 2.5, 50)", CellValue::Error(CellError::Num)),
            ((0, 940, 12), "CONFIDENCE.T(1, 2.5, 50)", CellValue::Error(CellError::Num)),
            ((0, 940, 13), "CONFIDENCE.T(0.05, 0, 50)", CellValue::Error(CellError::Num)),

            // 8. Cumulative and inverse relationships
            ((0, 945, 0), "BINOM.DIST.RANGE(10, 0.5, 0, 10)", CellValue::Number(1.0)),
            ((0, 945, 1), "BINOM.DIST(0, 0, 0.5, FALSE)", CellValue::Number(1.0)),
            ((0, 945, 2), "BINOM.DIST(0, 0, 0.5, TRUE)", CellValue::Number(1.0)),
            ((0, 945, 3), "ROUND(CHISQ.DIST(CHISQ.INV(0.75, 4), 4, TRUE), 4)", CellValue::Number(0.75)),
            ((0, 945, 4), "ROUND(CHISQ.DIST.RT(CHISQ.INV.RT(0.25, 4), 4), 4)", CellValue::Number(0.25)),
            ((0, 945, 5), "ROUND(F.DIST(F.INV(0.75, 5, 10), 5, 10, TRUE), 4)", CellValue::Number(0.75)),
            ((0, 945, 6), "ROUND(F.DIST.RT(F.INV.RT(0.25, 5, 10), 5, 10), 4)", CellValue::Number(0.25)),
            ((0, 945, 7), "ROUND(T.DIST(T.INV(0.75, 8), 8, TRUE), 4)", CellValue::Number(0.75)),
            ((0, 945, 8), "ROUND(T.DIST.2T(T.INV.2T(0.05, 8), 8), 4)", CellValue::Number(0.05)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_probability_distributions_comprehensive_edge_cases() {
        let mut book = MockBook::new(false);

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Normal and Standard Normal Distributions (NORM.DIST, NORM.S.DIST, NORM.INV, NORM.S.INV, PHI, GAUSS)
            ((0, 50, 0), "NORM.DIST(10, 10, 2, TRUE)", CellValue::Number(0.5)),
            ((0, 50, 1), "NORM.DIST(10, 10, 2, FALSE)", CellValue::Number(1.0 / (2.0 * (2.0 * std::f64::consts::PI).sqrt()))),
            ((0, 50, 2), "NORM.DIST(10, 10, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 50, 3), "NORM.DIST(10, 10, -2, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 50, 4), "NORM.S.DIST(0, TRUE)", CellValue::Number(0.5)),
            ((0, 50, 5), "NORM.S.DIST(0, FALSE)", CellValue::Number(1.0 / (2.0 * std::f64::consts::PI).sqrt())),
            ((0, 50, 6), "NORM.INV(0.5, 10, 2)", CellValue::Number(10.0)),
            ((0, 50, 7), "NORM.INV(0, 10, 2)", CellValue::Error(CellError::Num)),
            ((0, 50, 8), "NORM.INV(1, 10, 2)", CellValue::Error(CellError::Num)),
            ((0, 50, 9), "NORM.INV(0.5, 10, -2)", CellValue::Error(CellError::Num)),
            ((0, 50, 10), "NORM.S.INV(0.5)", CellValue::Number(0.0)),
            ((0, 50, 11), "NORM.S.INV(0)", CellValue::Error(CellError::Num)),
            ((0, 50, 12), "NORM.S.INV(1)", CellValue::Error(CellError::Num)),
            ((0, 50, 13), "PHI(0)", CellValue::Number(1.0 / (2.0 * std::f64::consts::PI).sqrt())),
            ((0, 50, 14), "GAUSS(0)", CellValue::Number(0.0)),

            // 2. Lognormal Distribution (LOGNORM.DIST, LOGNORM.INV)
            ((0, 55, 0), "LOGNORM.DIST(1, 0, 1, TRUE)", CellValue::Number(0.5)), // ln(1) = 0, mean = 0 -> CDF = 0.5
            ((0, 55, 1), "LOGNORM.DIST(0, 0, 1, TRUE)", CellValue::Error(CellError::Num)), // x <= 0 gives #NUM!
            ((0, 55, 2), "LOGNORM.DIST(-1, 0, 1, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 55, 3), "LOGNORM.DIST(1, 0, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 55, 4), "LOGNORM.DIST(1, 0, -1, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 55, 5), "LOGNORM.INV(0.5, 0, 1)", CellValue::Number(1.0)), // exp(0) = 1
            ((0, 55, 6), "LOGNORM.INV(0, 0, 1)", CellValue::Error(CellError::Num)),
            ((0, 55, 7), "LOGNORM.INV(1, 0, 1)", CellValue::Error(CellError::Num)),
            ((0, 55, 8), "LOGNORM.INV(0.5, 0, 0)", CellValue::Error(CellError::Num)),

            // 3. Student's T-Distribution (T.DIST, T.DIST.RT, T.DIST.2T, T.INV, T.INV.2T, TDIST, TINV)
            ((0, 60, 0), "T.DIST(0, 5, TRUE)", CellValue::Number(0.5)), // t=0 gives 0.5 for symmetric T-distribution
            ((0, 60, 1), "T.DIST(1, 0, TRUE)", CellValue::Error(CellError::Div0)), // df < 1 gives #DIV/0!
            ((0, 60, 2), "T.DIST(1, -1, TRUE)", CellValue::Error(CellError::Num)), // df < 0 gives #NUM!
            ((0, 60, 3), "T.DIST.RT(0, 5)", CellValue::Number(0.5)), // right tail at 0 is 0.5
            ((0, 60, 4), "T.DIST.RT(1, 0)", CellValue::Error(CellError::Num)),
            ((0, 60, 5), "T.DIST.2T(0, 5)", CellValue::Number(1.0)), // two-tailed at 0 is 1.0
            ((0, 60, 6), "T.DIST.2T(-1, 5)", CellValue::Error(CellError::Num)), // t < 0 gives #NUM!
            ((0, 60, 7), "T.DIST.2T(1, 0)", CellValue::Error(CellError::Num)),
            ((0, 60, 8), "T.INV(0.5, 5)", CellValue::Number(0.0)),
            ((0, 60, 9), "T.INV(0, 5)", CellValue::Error(CellError::Num)),
            ((0, 60, 10), "T.INV(1, 5)", CellValue::Error(CellError::Num)),
            ((0, 60, 11), "T.INV(0.5, 0)", CellValue::Error(CellError::Num)),
            ((0, 60, 12), "T.INV.2T(1.0, 5)", CellValue::Number(0.0)), // p=1 gives t=0
            ((0, 60, 13), "T.INV.2T(0.0, 5)", CellValue::Error(CellError::Num)),
            ((0, 60, 14), "T.INV.2T(1.1, 5)", CellValue::Error(CellError::Num)),
            ((0, 60, 15), "TDIST(0, 5, 2)", CellValue::Number(1.0)),
            ((0, 60, 16), "TDIST(0, 5, 1)", CellValue::Number(0.5)),
            ((0, 60, 17), "TDIST(-1, 5, 2)", CellValue::Error(CellError::Num)),
            ((0, 60, 18), "TDIST(1, 5, 3)", CellValue::Error(CellError::Num)), // tails not 1 or 2
            ((0, 60, 19), "TINV(1.0, 5)", CellValue::Number(0.0)),
            ((0, 60, 20), "TINV(0.0, 5)", CellValue::Error(CellError::Div0)), // p=0 in legacy TINV gives #DIV/0!

            // 4. F-Distribution (F.DIST, F.DIST.RT, F.INV, F.INV.RT, FDIST, FINV)
            ((0, 65, 0), "F.DIST(1, 5, 5, TRUE)", CellValue::Number(0.5)), // d1=d2 gives CDF=0.5 at x=1
            ((0, 65, 1), "F.DIST(0, 2, 2, FALSE)", CellValue::Number(1.0)),
            ((0, 65, 2), "F.DIST(-1, 5, 5, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 65, 3), "F.DIST(1, 0, 5, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 65, 4), "F.DIST(1, 5, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 65, 5), "F.DIST.RT(1, 5, 5)", CellValue::Number(0.5)),
            ((0, 65, 6), "F.DIST.RT(0, 5, 5)", CellValue::Number(1.0)),
            ((0, 65, 7), "F.DIST.RT(-1, 5, 5)", CellValue::Error(CellError::Num)),
            ((0, 65, 8), "F.INV(0.5, 5, 5)", CellValue::Number(1.0)),
            ((0, 65, 9), "F.INV(0, 5, 5)", CellValue::Number(0.0)),
            ((0, 65, 10), "F.INV(1, 5, 5)", CellValue::Error(CellError::Num)),
            ((0, 65, 11), "F.INV.RT(0.5, 5, 5)", CellValue::Number(1.0)),
            ((0, 65, 12), "F.INV.RT(1.0, 5, 5)", CellValue::Number(0.0)),
            ((0, 65, 13), "F.INV.RT(0.0, 5, 5)", CellValue::Error(CellError::Num)),
            ((0, 65, 14), "FDIST(1, 5, 5)", CellValue::Number(0.5)),
            ((0, 65, 15), "FINV(0.5, 5, 5)", CellValue::Number(1.0)),

            // 5. Chi-Square Distribution (CHISQ.DIST, CHISQ.DIST.RT, CHISQ.INV, CHISQ.INV.RT, CHIDIST, CHIINV)
            ((0, 70, 0), "CHISQ.DIST(0, 2, TRUE)", CellValue::Number(0.0)),
            ((0, 70, 1), "CHISQ.DIST(2, 2, TRUE)", CellValue::Number(1.0 - (-1.0f64).exp())), // 1 - exp(-1)
            ((0, 70, 2), "CHISQ.DIST(0, 2, FALSE)", CellValue::Number(0.5)),
            ((0, 70, 3), "CHISQ.DIST(-1, 2, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 70, 4), "CHISQ.DIST(1, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 70, 5), "CHISQ.DIST.RT(0, 2)", CellValue::Number(1.0)),
            ((0, 70, 6), "CHISQ.DIST.RT(2, 2)", CellValue::Number((-1.0f64).exp())),
            ((0, 70, 7), "CHISQ.DIST.RT(-1, 2)", CellValue::Error(CellError::Num)),
            ((0, 70, 8), "CHISQ.INV(0, 2)", CellValue::Number(0.0)),
            ((0, 70, 9), "CHISQ.INV(1, 2)", CellValue::Error(CellError::Num)),
            ((0, 70, 10), "CHISQ.INV.RT(1.0, 2)", CellValue::Number(0.0)),
            ((0, 70, 11), "CHISQ.INV.RT(0.0, 2)", CellValue::Error(CellError::Num)),
            ((0, 70, 12), "CHIDIST(0, 2)", CellValue::Number(1.0)),
            ((0, 70, 13), "CHIINV(1.0, 2)", CellValue::Number(0.0)),

            // 6. Binomial, Negative Binomial, and Hypergeometric (BINOM.DIST, BINOM.INV, BINOM.DIST.RANGE, NEGBINOM.DIST, HYPGEOM.DIST)
            ((0, 75, 0), "BINOM.DIST(2, 4, 0.5, FALSE)", CellValue::Number(0.375)), // 4C2 * (0.5)^4 = 6/16 = 0.375
            ((0, 75, 1), "BINOM.DIST(2, 4, 0.5, TRUE)", CellValue::Number(0.6875)), // (1 + 4 + 6) / 16 = 11/16 = 0.6875
            ((0, 75, 2), "BINOM.DIST(0, 4, 0.5, FALSE)", CellValue::Number(0.0625)), // 1/16
            ((0, 75, 3), "BINOM.DIST(5, 4, 0.5, FALSE)", CellValue::Error(CellError::Num)), // s > trials
            ((0, 75, 4), "BINOM.DIST(-1, 4, 0.5, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 75, 5), "BINOM.DIST(2, 4, 1.5, FALSE)", CellValue::Error(CellError::Num)), // p > 1
            ((0, 75, 6), "_xlfn.BINOM.DIST.RANGE(4, 0.5, 1, 3)", CellValue::Number(0.875)), // 14 / 16 = 0.875
            ((0, 75, 7), "_xlfn.BINOM.INV(4, 0.5, 0.5)", CellValue::Number(2.0)),
            ((0, 75, 8), "_xlfn.BINOM.INV(4, 0.5, 0.0)", CellValue::Error(CellError::Num)),
            ((0, 75, 9), "_xlfn.BINOM.INV(4, 0.5, 1.0)", CellValue::Error(CellError::Num)),
            ((0, 75, 10), "_xlfn.NEGBINOM.DIST(0, 1, 0.5, FALSE)", CellValue::Number(0.5)),
            ((0, 75, 11), "_xlfn.NEGBINOM.DIST(1, 1, 0.5, FALSE)", CellValue::Number(0.25)),
            ((0, 75, 12), "_xlfn.NEGBINOM.DIST(-1, 1, 0.5, FALSE)", CellValue::Error(CellError::Num)),
            ((0, 75, 13), "_xlfn.HYPGEOM.DIST(1, 4, 2, 10, FALSE)", CellValue::Number(8.0 / 15.0)), // (2C1 * 8C3) / 10C4 = (2 * 56) / 210 = 112/210 = 8/15
            ((0, 75, 14), "_xlfn.HYPGEOM.DIST(0, 4, 2, 10, FALSE)", CellValue::Number(1.0 / 3.0)), // (2C0 * 8C4) / 10C4 = 70 / 210 = 1/3
            ((0, 75, 15), "_xlfn.HYPGEOM.DIST(0, 4, 2, 10, TRUE)", CellValue::Number(1.0 / 3.0)),

            // 7. Poisson and Exponential Distributions (POISSON.DIST, EXPON.DIST)
            ((0, 80, 0), "POISSON.DIST(0, 2, FALSE)", CellValue::Number((-2.0f64).exp())),
            ((0, 80, 1), "POISSON.DIST(1, 2, FALSE)", CellValue::Number(2.0 * (-2.0f64).exp())),
            ((0, 80, 2), "POISSON.DIST(1, 2, TRUE)", CellValue::Number(3.0 * (-2.0f64).exp())),
            ((0, 80, 3), "POISSON.DIST(-1, 2, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 80, 4), "POISSON.DIST(0, -1, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 80, 5), "EXPON.DIST(0, 2, FALSE)", CellValue::Number(2.0)),
            ((0, 80, 6), "EXPON.DIST(0, 2, TRUE)", CellValue::Number(0.0)),
            ((0, 80, 7), "EXPON.DIST(1, 2, TRUE)", CellValue::Number(1.0 - (-2.0f64).exp())),
            ((0, 80, 8), "EXPON.DIST(-1, 2, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 80, 9), "EXPON.DIST(1, 0, TRUE)", CellValue::Error(CellError::Num)),

            // 8. Gamma, Beta, and Weibull Distributions (GAMMA, GAMMALN, GAMMA.DIST, GAMMA.INV, BETA.DIST, BETA.INV, WEIBULL.DIST)
            ((0, 85, 0), "_xlfn.GAMMA(1)", CellValue::Number(1.0)),
            ((0, 85, 1), "_xlfn.GAMMA(2)", CellValue::Number(1.0)),
            ((0, 85, 2), "_xlfn.GAMMA(3)", CellValue::Number(2.0)),
            ((0, 85, 3), "_xlfn.GAMMA(4)", CellValue::Number(6.0)),
            ((0, 85, 4), "_xlfn.GAMMA(5)", CellValue::Number(24.0)),
            ((0, 85, 5), "_xlfn.GAMMA(0)", CellValue::Error(CellError::Num)),
            ((0, 85, 6), "_xlfn.GAMMA(-1)", CellValue::Error(CellError::Num)),
            ((0, 85, 7), "GAMMALN(1)", CellValue::Number(0.0)),
            ((0, 85, 8), "GAMMALN(2)", CellValue::Number(0.0)),
            ((0, 85, 9), "GAMMALN(3)", CellValue::Number(2.0f64.ln())),
            ((0, 85, 10), "GAMMALN(0)", CellValue::Error(CellError::Num)),
            ((0, 85, 11), "GAMMALN(-1)", CellValue::Error(CellError::Num)),
            ((0, 85, 12), "GAMMA.DIST(1, 1, 1, FALSE)", CellValue::Number((-1.0f64).exp())),
            ((0, 85, 13), "GAMMA.DIST(1, 1, 1, TRUE)", CellValue::Number(1.0 - (-1.0f64).exp())),
            ((0, 85, 14), "GAMMA.DIST(0, 1, 1, FALSE)", CellValue::Number(1.0)),
            ((0, 85, 15), "GAMMA.DIST(-1, 1, 1, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 85, 16), "GAMMA.DIST(1, 0, 1, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 85, 17), "GAMMA.DIST(1, 1, 0, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 85, 18), "GAMMA.INV(0, 1, 1)", CellValue::Number(0.0)),
            ((0, 85, 19), "GAMMA.INV(1, 1, 1)", CellValue::Error(CellError::Num)),
            ((0, 85, 20), "_xlfn.BETA.DIST(0, 1, 1, TRUE)", CellValue::Number(0.0)),
            ((0, 85, 21), "_xlfn.BETA.DIST(1, 1, 1, TRUE)", CellValue::Number(1.0)),
            ((0, 85, 22), "_xlfn.BETA.DIST(0.5, 1, 1, TRUE)", CellValue::Number(0.5)),
            ((0, 85, 23), "_xlfn.BETA.DIST(0.5, 1, 1, FALSE)", CellValue::Number(1.0)),
            ((0, 85, 24), "_xlfn.BETA.DIST(-0.1, 1, 1, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 85, 25), "_xlfn.BETA.DIST(1.1, 1, 1, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 85, 26), "_xlfn.BETA.INV(0.5, 1, 1)", CellValue::Number(0.5)),
            ((0, 85, 27), "_xlfn.BETA.INV(0.0, 1, 1)", CellValue::Error(CellError::Num)),
            ((0, 85, 28), "_xlfn.BETA.INV(1.0, 1, 1)", CellValue::Error(CellError::Num)),
            ((0, 85, 29), "WEIBULL.DIST(0, 1, 1, FALSE)", CellValue::Number(1.0)),
            ((0, 85, 30), "WEIBULL.DIST(1, 1, 1, FALSE)", CellValue::Number((-1.0f64).exp())),
            ((0, 85, 31), "WEIBULL.DIST(1, 1, 1, TRUE)", CellValue::Number(1.0 - (-1.0f64).exp())),
            ((0, 85, 32), "WEIBULL.DIST(-1, 1, 1, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 85, 33), "WEIBULL.DIST(1, 0, 1, TRUE)", CellValue::Error(CellError::Num)),
            ((0, 85, 34), "WEIBULL.DIST(1, 1, 0, TRUE)", CellValue::Error(CellError::Num)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_descriptive_statistics_ranking_correlation_and_conditional_aggregation_edge_cases() {
        let mut book = MockBook::new(false);

        // Seed data for ranking (Sheet1!A950:A954 -> rows 949..=953, col 0)
        book.values.insert((0, 949, 0), CellValue::Number(10.0));
        book.values.insert((0, 950, 0), CellValue::Number(30.0));
        book.values.insert((0, 951, 0), CellValue::Number(20.0));
        book.values.insert((0, 952, 0), CellValue::Number(50.0));
        book.values.insert((0, 953, 0), CellValue::Number(40.0));

        // Seed data for ranking with ties (Sheet1!A955:A958 -> rows 954..=957, col 0)
        book.values.insert((0, 954, 0), CellValue::Number(10.0));
        book.values.insert((0, 955, 0), CellValue::Number(20.0));
        book.values.insert((0, 956, 0), CellValue::Number(20.0));
        book.values.insert((0, 957, 0), CellValue::Number(40.0));

        // Seed data for conditional aggregations (Sheet1!A960:B964 -> rows 959..=963)
        // Col A (text criteria)
        book.values.insert((0, 959, 0), CellValue::Text("apple".into()));
        book.values.insert((0, 960, 0), CellValue::Text("banana".into()));
        book.values.insert((0, 961, 0), CellValue::Text("apple".into()));
        book.values.insert((0, 962, 0), CellValue::Text("orange".into()));
        book.values.insert((0, 963, 0), CellValue::Text("banana".into()));
        // Col B (numeric values)
        book.values.insert((0, 959, 1), CellValue::Number(10.0));
        book.values.insert((0, 960, 1), CellValue::Number(20.0));
        book.values.insert((0, 961, 1), CellValue::Number(30.0));
        book.values.insert((0, 962, 1), CellValue::Number(40.0));
        book.values.insert((0, 963, 1), CellValue::Number(50.0));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Descriptive statistics, dispersion, and moments
            ((0, 970, 0), "COUNT(1, \"2\", TRUE, \"abc\")", CellValue::Number(3.0)),
            ((0, 970, 1), "COUNTA(1, \"\", \"abc\", TRUE)", CellValue::Number(4.0)),
            ((0, 970, 2), "AVERAGE(10, 20, 30)", CellValue::Number(20.0)),
            ((0, 970, 3), "AVERAGEA(10, 20, 30, FALSE, TRUE)", CellValue::Number(12.2)),
            ((0, 970, 4), "MEDIAN(1, 2, 100)", CellValue::Number(2.0)),
            ((0, 970, 5), "MEDIAN(1, 2, 3, 4)", CellValue::Number(2.5)),
            ((0, 970, 6), "MODE(1, 2, 2, 3, 3, 3)", CellValue::Number(3.0)),
            ((0, 970, 7), "MODE.SNGL(1, 2, 2, 3)", CellValue::Number(2.0)),
            ((0, 970, 8), "MODE(1, 2, 3)", CellValue::Error(CellError::NA)),
            ((0, 970, 9), "VAR.S(10, 20, 30)", CellValue::Number(100.0)),
            ((0, 970, 10), "ROUND(VAR.P(10, 20, 30), 4)", CellValue::Number(66.6667)),
            ((0, 970, 11), "VAR.S(10)", CellValue::Error(CellError::Div0)),
            ((0, 970, 12), "STDEV.S(10, 20, 30)", CellValue::Number(10.0)),
            ((0, 970, 13), "ROUND(STDEV.P(10, 20, 30), 4)", CellValue::Number(8.165)),
            ((0, 970, 14), "STDEV.S(10)", CellValue::Error(CellError::Div0)),
            ((0, 970, 15), "ROUND(AVEDEV(10, 20, 30), 4)", CellValue::Number(6.6667)),
            ((0, 970, 16), "DEVSQ(10, 20, 30)", CellValue::Number(200.0)),
            ((0, 970, 17), "ROUND(GEOMEAN(2, 8), 4)", CellValue::Number(4.0)),
            ((0, 970, 18), "ROUND(GEOMEAN(1, 2, 4), 4)", CellValue::Number(2.0)),
            ((0, 970, 19), "GEOMEAN(1, 0, 4)", CellValue::Error(CellError::Num)),
            ((0, 970, 20), "GEOMEAN(1, -2, 4)", CellValue::Error(CellError::Num)),
            ((0, 970, 21), "ROUND(HARMEAN(2, 4), 4)", CellValue::Number(2.6667)),
            ((0, 970, 22), "HARMEAN(1, 0, 4)", CellValue::Error(CellError::Num)),
            ((0, 970, 23), "KURT(1, 2, 3)", CellValue::Error(CellError::Div0)),
            ((0, 970, 24), "SKEW(1, 2)", CellValue::Error(CellError::Div0)),
            ((0, 970, 25), "ROUND(TRIMMEAN({1, 2, 3, 4, 5, 6, 7, 8, 9, 10}, 0.2), 4)", CellValue::Number(5.5)),
            ((0, 970, 26), "TRIMMEAN({1, 2, 3}, -0.1)", CellValue::Error(CellError::Num)),
            ((0, 970, 27), "TRIMMEAN({1, 2, 3}, 1.0)", CellValue::Error(CellError::Num)),
            ((0, 970, 28), "STANDARDIZE(15, 10, 2.5)", CellValue::Number(2.0)),
            ((0, 970, 29), "STANDARDIZE(10, 10, 0)", CellValue::Error(CellError::Num)),
            ((0, 970, 30), "STANDARDIZE(10, 10, -1)", CellValue::Error(CellError::Num)),

            // 2. Ranking and quantiles (LARGE, SMALL, PERCENTILE, QUARTILE, PERCENTRANK, RANK)
            ((0, 975, 0), "LARGE({10, 50, 20, 40, 30}, 1)", CellValue::Number(50.0)),
            ((0, 975, 1), "LARGE({10, 50, 20, 40, 30}, 5)", CellValue::Number(10.0)),
            ((0, 975, 2), "LARGE({10, 50, 20, 40, 30}, 0)", CellValue::Error(CellError::Num)),
            ((0, 975, 3), "LARGE({10, 50, 20, 40, 30}, 6)", CellValue::Error(CellError::Num)),
            ((0, 975, 4), "SMALL({10, 50, 20, 40, 30}, 1)", CellValue::Number(10.0)),
            ((0, 975, 5), "SMALL({10, 50, 20, 40, 30}, 5)", CellValue::Number(50.0)),
            ((0, 975, 6), "SMALL({10, 50, 20, 40, 30}, 0)", CellValue::Error(CellError::Num)),
            ((0, 975, 7), "SMALL({10, 50, 20, 40, 30}, 6)", CellValue::Error(CellError::Num)),
            ((0, 975, 8), "PERCENTILE.INC({10, 20, 30, 40, 50}, 0)", CellValue::Number(10.0)),
            ((0, 975, 9), "PERCENTILE.INC({10, 20, 30, 40, 50}, 0.5)", CellValue::Number(30.0)),
            ((0, 975, 10), "PERCENTILE.INC({10, 20, 30, 40, 50}, 1)", CellValue::Number(50.0)),
            ((0, 975, 11), "PERCENTILE.INC({10, 20, 30}, -0.1)", CellValue::Error(CellError::Num)),
            ((0, 975, 12), "PERCENTILE.INC({10, 20, 30}, 1.1)", CellValue::Error(CellError::Num)),
            ((0, 975, 13), "PERCENTILE.EXC({10, 20, 30, 40, 50}, 0.5)", CellValue::Number(30.0)),
            ((0, 975, 14), "PERCENTILE.EXC({10, 20, 30}, 0)", CellValue::Error(CellError::Num)),
            ((0, 975, 15), "PERCENTILE.EXC({10, 20, 30}, 1)", CellValue::Error(CellError::Num)),
            ((0, 975, 16), "PERCENTILE({10, 20, 30}, 0.5) = PERCENTILE.INC({10, 20, 30}, 0.5)", CellValue::Bool(true)),
            ((0, 975, 17), "QUARTILE.INC({10, 20, 30, 40, 50}, 0)", CellValue::Number(10.0)),
            ((0, 975, 18), "QUARTILE.INC({10, 20, 30, 40, 50}, 2)", CellValue::Number(30.0)),
            ((0, 975, 19), "QUARTILE.INC({10, 20, 30, 40, 50}, 4)", CellValue::Number(50.0)),
            ((0, 975, 20), "QUARTILE.INC({10, 20, 30}, -1)", CellValue::Error(CellError::Num)),
            ((0, 975, 21), "QUARTILE.INC({10, 20, 30}, 5)", CellValue::Error(CellError::Num)),
            ((0, 975, 22), "QUARTILE.EXC({10, 20, 30, 40, 50}, 2)", CellValue::Number(30.0)),
            ((0, 975, 23), "QUARTILE.EXC({10, 20, 30, 40, 50}, 0)", CellValue::Error(CellError::Num)),
            ((0, 975, 24), "QUARTILE.EXC({10, 20, 30, 40, 50}, 4)", CellValue::Error(CellError::Num)),
            ((0, 975, 25), "PERCENTRANK.INC({10, 20, 30, 40, 50}, 10)", CellValue::Number(0.0)),
            ((0, 975, 26), "PERCENTRANK.INC({10, 20, 30, 40, 50}, 30)", CellValue::Number(0.5)),
            ((0, 975, 27), "PERCENTRANK.INC({10, 20, 30, 40, 50}, 50)", CellValue::Number(1.0)),
            ((0, 975, 28), "PERCENTRANK.INC({10, 20, 30}, 5)", CellValue::Error(CellError::NA)),
            ((0, 975, 29), "PERCENTRANK.INC({10, 20, 30}, 35)", CellValue::Error(CellError::NA)),
            ((0, 975, 30), "PERCENTRANK.EXC({10, 20, 30, 40, 50}, 30)", CellValue::Number(0.5)),
            ((0, 975, 31), "RANK(50, A950:A954)", CellValue::Number(1.0)),
            ((0, 975, 32), "RANK(10, A950:A954)", CellValue::Number(5.0)),
            ((0, 975, 33), "RANK(10, A950:A954, 1)", CellValue::Number(1.0)),
            ((0, 975, 34), "RANK(50, A950:A954, 1)", CellValue::Number(5.0)),
            ((0, 975, 35), "RANK(99, A950:A954)", CellValue::Error(CellError::NA)),
            ((0, 975, 36), "RANK.EQ(30, A950:A954)", CellValue::Number(3.0)),
            ((0, 975, 37), "RANK.EQ(20, A955:A958, 1)", CellValue::Number(2.0)),
            ((0, 975, 38), "RANK.AVG(20, A955:A958, 1)", CellValue::Number(2.5)),

            // 3. Correlation, covariance, regression, and forecast
            ((0, 980, 0), "CORREL({1, 2, 3, 4}, {2, 4, 6, 8})", CellValue::Number(1.0)),
            ((0, 980, 1), "CORREL({1, 2, 3, 4}, {-2, -4, -6, -8})", CellValue::Number(-1.0)),
            ((0, 980, 2), "CORREL({1, 2, 3}, {1, 2})", CellValue::Error(CellError::NA)),
            ((0, 980, 3), "CORREL({1, 1, 1}, {2, 3, 4})", CellValue::Error(CellError::Div0)),
            ((0, 980, 4), "PEARSON({1, 2, 3, 4}, {2, 4, 6, 8}) = CORREL({1, 2, 3, 4}, {2, 4, 6, 8})", CellValue::Bool(true)),
            ((0, 980, 5), "RSQ({1, 2, 3, 4}, {2, 4, 6, 8})", CellValue::Number(1.0)),
            ((0, 980, 6), "RSQ({1, 2, 3, 4}, {-2, -4, -6, -8})", CellValue::Number(1.0)),
            ((0, 980, 7), "COVARIANCE.P({1, 2, 3, 4, 5}, {2, 4, 6, 8, 10})", CellValue::Number(4.0)),
            ((0, 980, 8), "COVARIANCE.S({1, 2, 3, 4, 5}, {2, 4, 6, 8, 10})", CellValue::Number(5.0)),
            ((0, 980, 9), "COVAR({1, 2, 3}, {2, 4, 6}) = COVARIANCE.P({1, 2, 3}, {2, 4, 6})", CellValue::Bool(true)),
            ((0, 980, 10), "COVARIANCE.S({1}, {2})", CellValue::Error(CellError::Div0)),
            ((0, 980, 11), "SLOPE({3, 5, 7, 9}, {1, 2, 3, 4})", CellValue::Number(2.0)),
            ((0, 980, 12), "INTERCEPT({3, 5, 7, 9}, {1, 2, 3, 4})", CellValue::Number(1.0)),
            ((0, 980, 13), "SLOPE({1, 2, 3}, {5, 5, 5})", CellValue::Error(CellError::Div0)),
            ((0, 980, 14), "FORECAST(5, {3, 5, 7, 9}, {1, 2, 3, 4})", CellValue::Number(11.0)),
            ((0, 980, 15), "FORECAST.LINEAR(5, {3, 5, 7, 9}, {1, 2, 3, 4})", CellValue::Number(11.0)),
            ((0, 980, 16), "STEYX({3, 5, 7, 9}, {1, 2, 3, 4})", CellValue::Number(0.0)),
            ((0, 980, 17), "STEYX({3, 5}, {1, 2})", CellValue::Error(CellError::Div0)),

            // 4. Combinatorics, Fisher transform, and probability intervals
            ((0, 985, 0), "PERMUT(5, 2)", CellValue::Number(20.0)),
            ((0, 985, 1), "PERMUT(5, 5)", CellValue::Number(120.0)),
            ((0, 985, 2), "PERMUT(5, 0)", CellValue::Number(1.0)),
            ((0, 985, 3), "PERMUT(5, 6)", CellValue::Error(CellError::Num)),
            ((0, 985, 4), "PERMUT(5, -1)", CellValue::Error(CellError::Num)),
            ((0, 985, 5), "PERMUT(-5, 2)", CellValue::Error(CellError::Num)),
            ((0, 985, 6), "PERMUTATIONA(5, 2)", CellValue::Number(25.0)),
            ((0, 985, 7), "PERMUTATIONA(5, 0)", CellValue::Number(1.0)),
            ((0, 985, 8), "PERMUTATIONA(0, 0)", CellValue::Number(1.0)),
            ((0, 985, 9), "PERMUTATIONA(5, -1)", CellValue::Error(CellError::Num)),
            ((0, 985, 10), "ROUND(FISHER(0), 4)", CellValue::Number(0.0)),
            ((0, 985, 11), "ROUND(FISHER(0.5), 4)", CellValue::Number(0.5493)),
            ((0, 985, 12), "ROUND(FISHER(-0.5), 4)", CellValue::Number(-0.5493)),
            ((0, 985, 13), "FISHER(1)", CellValue::Error(CellError::Num)),
            ((0, 985, 14), "FISHER(-1)", CellValue::Error(CellError::Num)),
            ((0, 985, 15), "FISHER(1.5)", CellValue::Error(CellError::Num)),
            ((0, 985, 16), "ROUND(FISHERINV(0), 4)", CellValue::Number(0.0)),
            ((0, 985, 17), "ROUND(FISHERINV(0.5493061), 4)", CellValue::Number(0.5)),
            ((0, 985, 18), "ROUND(FISHER(FISHERINV(0.75)), 4)", CellValue::Number(0.75)),
            ((0, 985, 19), "PROB({1, 2, 3, 4}, {0.1, 0.2, 0.3, 0.4}, 2, 3)", CellValue::Number(0.5)),
            ((0, 985, 20), "PROB({1, 2, 3, 4}, {0.1, 0.2, 0.3, 0.4}, 2)", CellValue::Number(0.2)),
            ((0, 985, 21), "PROB({1, 2, 3}, {0.1, 0.2, 0.3}, 2)", CellValue::Error(CellError::Num)),
            ((0, 985, 22), "PROB({1, 2}, {0.5, 0.5, 0.5}, 1)", CellValue::Error(CellError::NA)),

            // 5. Conditional aggregations (COUNTIF, COUNTIFS, SUMIF, SUMIFS, AVERAGEIF, MAXIFS, MINIFS)
            ((0, 990, 0), "COUNTIF(A960:A964, \"apple\")", CellValue::Number(2.0)),
            ((0, 990, 1), "COUNTIF(A960:A964, \"b*\")", CellValue::Number(2.0)),
            ((0, 990, 2), "COUNTIF(B960:B964, \">=30\")", CellValue::Number(3.0)),
            ((0, 990, 3), "COUNTIF(B960:B964, \"<>20\")", CellValue::Number(4.0)),
            ((0, 990, 4), "SUMIF(A960:A964, \"apple\", B960:B964)", CellValue::Number(40.0)),
            ((0, 990, 5), "SUMIF(B960:B964, \">20\")", CellValue::Number(120.0)),
            ((0, 990, 6), "AVERAGEIF(A960:A964, \"banana\", B960:B964)", CellValue::Number(35.0)),
            ((0, 990, 7), "COUNTIFS(A960:A964, \"apple\", B960:B964, \">10\")", CellValue::Number(1.0)),
            ((0, 990, 8), "SUMIFS(B960:B964, A960:A964, \"banana\", B960:B964, \">=20\")", CellValue::Number(70.0)),
            ((0, 990, 9), "MAXIFS(B960:B964, A960:A964, \"apple\")", CellValue::Number(30.0)),
            ((0, 990, 10), "MINIFS(B960:B964, A960:A964, \"banana\")", CellValue::Number(20.0)),

            // 6. Frequency distribution and dynamic regression arrays (FREQUENCY, TREND, GROWTH)
            ((0, 995, 0), "INDEX(FREQUENCY({1, 5, 8, 12, 18, 25}, {10, 20}), 1, 1)", CellValue::Number(3.0)),
            ((0, 995, 1), "INDEX(FREQUENCY({1, 5, 8, 12, 18, 25}, {10, 20}), 2, 1)", CellValue::Number(2.0)),
            ((0, 995, 2), "INDEX(FREQUENCY({1, 5, 8, 12, 18, 25}, {10, 20}), 3, 1)", CellValue::Number(1.0)),
            ((0, 995, 3), "ROUND(INDEX(TREND({3; 5; 7}, {1; 2; 3}, {4}), 1, 1), 4)", CellValue::Number(9.0)),
            ((0, 995, 4), "ROUND(INDEX(GROWTH({2; 4; 8}, {1; 2; 3}, {4}), 1, 1), 4)", CellValue::Number(16.0)),

            // 7. Subtotals and structured aggregations (SUBTOTAL, AGGREGATE)
            ((0, 1000, 0), "SUBTOTAL(9, B960:B964)", CellValue::Number(150.0)),
            ((0, 1000, 1), "SUBTOTAL(1, B960:B964)", CellValue::Number(30.0)),
            ((0, 1000, 2), "SUBTOTAL(4, B960:B964)", CellValue::Number(50.0)),
            ((0, 1000, 3), "SUBTOTAL(5, B960:B964)", CellValue::Number(10.0)),
            ((0, 1000, 4), "SUBTOTAL(0, B960:B964)", CellValue::Error(CellError::Value)),
            ((0, 1000, 5), "SUBTOTAL(9, {10, 20})", CellValue::Error(CellError::Value)),
            ((0, 1000, 6), "AGGREGATE(9, 4, B960:B964)", CellValue::Number(150.0)),
            ((0, 1000, 7), "AGGREGATE(1, 4, B960:B964)", CellValue::Number(30.0)),
            ((0, 1000, 8), "AGGREGATE(4, 4, B960:B964)", CellValue::Number(50.0)),
            ((0, 1000, 9), "AGGREGATE(5, 4, B960:B964)", CellValue::Number(10.0)),
            ((0, 1000, 10), "AGGREGATE(20, 4, B960:B964)", CellValue::Error(CellError::Value)),
            ((0, 1000, 11), "AGGREGATE(9, 8, B960:B964)", CellValue::Error(CellError::Value)),

            // 8. Extended conditional aggregation edge cases (unmatched, wildcards, and zero-match)
            ((0, 1005, 0), "COUNTIF(A960:A964, \"grape\")", CellValue::Number(0.0)),
            ((0, 1005, 1), "COUNTIF(A960:A964, \"*e\")", CellValue::Number(3.0)),
            ((0, 1005, 2), "SUMIF(A960:A964, \"grape\", B960:B964)", CellValue::Number(0.0)),
            ((0, 1005, 3), "AVERAGEIF(A960:A964, \"grape\", B960:B964)", CellValue::Error(CellError::Div0)),
            ((0, 1005, 4), "COUNTIFS(A960:A964, \"apple\", B960:B964, \">50\")", CellValue::Number(0.0)),
            ((0, 1005, 5), "MAXIFS(B960:B964, A960:A964, \"grape\")", CellValue::Number(0.0)),
            ((0, 1005, 6), "MINIFS(B960:B964, A960:A964, \"grape\")", CellValue::Number(0.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_database_matrix_and_advanced_math_edge_cases() {
        let mut book = MockBook::new(false);

        // Seed database table in Sheet1!A1010:D1015 (rows 1009..=1014, cols 0..=3)
        // Header row (row 1009)
        book.values.insert((0, 1009, 0), CellValue::Text("Tree".into()));
        book.values.insert((0, 1009, 1), CellValue::Text("Height".into()));
        book.values.insert((0, 1009, 2), CellValue::Text("Age".into()));
        book.values.insert((0, 1009, 3), CellValue::Text("Yield".into()));
        // Row 1: Apple, 18, 20, 14
        book.values.insert((0, 1010, 0), CellValue::Text("Apple".into()));
        book.values.insert((0, 1010, 1), CellValue::Number(18.0));
        book.values.insert((0, 1010, 2), CellValue::Number(20.0));
        book.values.insert((0, 1010, 3), CellValue::Number(14.0));
        // Row 2: Pear, 12, 12, 10
        book.values.insert((0, 1011, 0), CellValue::Text("Pear".into()));
        book.values.insert((0, 1011, 1), CellValue::Number(12.0));
        book.values.insert((0, 1011, 2), CellValue::Number(12.0));
        book.values.insert((0, 1011, 3), CellValue::Number(10.0));
        // Row 3: Cherry, 13, 14, 9
        book.values.insert((0, 1012, 0), CellValue::Text("Cherry".into()));
        book.values.insert((0, 1012, 1), CellValue::Number(13.0));
        book.values.insert((0, 1012, 2), CellValue::Number(14.0));
        book.values.insert((0, 1012, 3), CellValue::Number(9.0));
        // Row 4: Apple, 14, 15, 10
        book.values.insert((0, 1013, 0), CellValue::Text("Apple".into()));
        book.values.insert((0, 1013, 1), CellValue::Number(14.0));
        book.values.insert((0, 1013, 2), CellValue::Number(15.0));
        book.values.insert((0, 1013, 3), CellValue::Number(10.0));
        // Row 5: Pear, 9, 8, 8
        book.values.insert((0, 1014, 0), CellValue::Text("Pear".into()));
        book.values.insert((0, 1014, 1), CellValue::Number(9.0));
        book.values.insert((0, 1014, 2), CellValue::Number(8.0));
        book.values.insert((0, 1014, 3), CellValue::Number(8.0));

        // Criteria 1 in Sheet1!F1010:G1011: Tree=Apple AND Height>15 (single row 1 match)
        book.values.insert((0, 1009, 5), CellValue::Text("Tree".into()));
        book.values.insert((0, 1009, 6), CellValue::Text("Height".into()));
        book.values.insert((0, 1010, 5), CellValue::Text("Apple".into()));
        book.values.insert((0, 1010, 6), CellValue::Text(">15".into()));

        // Criteria 2 in Sheet1!F1013:F1014: Tree=Apple (two rows match: rows 1 and 4)
        book.values.insert((0, 1012, 5), CellValue::Text("Tree".into()));
        book.values.insert((0, 1013, 5), CellValue::Text("Apple".into()));

        // Criteria 3 in Sheet1!F1016:F1017: Tree=Banana (zero match)
        book.values.insert((0, 1015, 5), CellValue::Text("Tree".into()));
        book.values.insert((0, 1016, 5), CellValue::Text("Banana".into()));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Database functions (DSUM, DAVERAGE, DCOUNT, DCOUNTA, DMAX, DMIN, DPRODUCT, DSTDEV, DSTDEVP, DVAR, DVARP, DGET)
            ((0, 1020, 0), "DSUM(A1010:D1015, \"Yield\", F1013:F1014)", CellValue::Number(24.0)),
            ((0, 1020, 1), "DSUM(A1010:D1015, 4, F1013:F1014)", CellValue::Number(24.0)),
            ((0, 1020, 2), "DAVERAGE(A1010:D1015, \"Yield\", F1013:F1014)", CellValue::Number(12.0)),
            ((0, 1020, 3), "DCOUNT(A1010:D1015, \"Yield\", F1013:F1014)", CellValue::Number(2.0)),
            ((0, 1020, 4), "DCOUNTA(A1010:D1015, \"Yield\", F1013:F1014)", CellValue::Number(2.0)),
            ((0, 1020, 5), "DMAX(A1010:D1015, \"Yield\", F1013:F1014)", CellValue::Number(14.0)),
            ((0, 1020, 6), "DMIN(A1010:D1015, \"Yield\", F1013:F1014)", CellValue::Number(10.0)),
            ((0, 1020, 7), "DPRODUCT(A1010:D1015, \"Yield\", F1013:F1014)", CellValue::Number(140.0)),
            ((0, 1020, 8), "ROUND(DSTDEV(A1010:D1015, \"Yield\", F1013:F1014), 4)", CellValue::Number(2.8284)),
            ((0, 1020, 9), "DSTDEVP(A1010:D1015, \"Yield\", F1013:F1014)", CellValue::Number(2.0)),
            ((0, 1020, 10), "DVAR(A1010:D1015, \"Yield\", F1013:F1014)", CellValue::Number(8.0)),
            ((0, 1020, 11), "DVARP(A1010:D1015, \"Yield\", F1013:F1014)", CellValue::Number(4.0)),
            ((0, 1020, 12), "DGET(A1010:D1015, \"Yield\", F1010:G1011)", CellValue::Number(14.0)),
            ((0, 1020, 13), "DGET(A1010:D1015, \"Yield\", F1013:F1014)", CellValue::Error(CellError::Num)),
            ((0, 1020, 14), "DGET(A1010:D1015, \"Yield\", F1016:F1017)", CellValue::Error(CellError::Value)),
            ((0, 1020, 15), "DSUM(A1010:D1015, \"Unknown\", F1013:F1014)", CellValue::Error(CellError::Value)),
            ((0, 1020, 16), "DSUM(A1010:D1015, 5, F1013:F1014)", CellValue::Error(CellError::Value)),
            ((0, 1020, 17), "DSUM(A1010:D1015, 0, F1013:F1014)", CellValue::Error(CellError::Value)),

            // 2. Matrix operations (MMULT, MDETERM, MINVERSE, MUNIT)
            ((0, 1025, 0), "INDEX(MMULT({1, 2; 3, 4}, {5, 6; 7, 8}), 1, 1)", CellValue::Number(19.0)),
            ((0, 1025, 1), "INDEX(MMULT({1, 2; 3, 4}, {5, 6; 7, 8}), 1, 2)", CellValue::Number(22.0)),
            ((0, 1025, 2), "INDEX(MMULT({1, 2; 3, 4}, {5, 6; 7, 8}), 2, 1)", CellValue::Number(43.0)),
            ((0, 1025, 3), "INDEX(MMULT({1, 2; 3, 4}, {5, 6; 7, 8}), 2, 2)", CellValue::Number(50.0)),
            ((0, 1025, 4), "MMULT({1, 2}, {3, 4})", CellValue::Error(CellError::Value)),
            ((0, 1025, 5), "ROUND(MDETERM({1, 2; 3, 4}), 4)", CellValue::Number(-2.0)),
            ((0, 1025, 6), "ROUND(MDETERM({2, 0; 0, 3}), 4)", CellValue::Number(6.0)),
            ((0, 1025, 7), "MDETERM({1, 2, 3; 4, 5, 6})", CellValue::Error(CellError::Value)),
            ((0, 1025, 8), "INDEX(MINVERSE({1, 2; 3, 4}), 1, 1)", CellValue::Number(-2.0)),
            ((0, 1025, 9), "INDEX(MINVERSE({1, 2; 3, 4}), 1, 2)", CellValue::Number(1.0)),
            ((0, 1025, 10), "INDEX(MINVERSE({1, 2; 3, 4}), 2, 1)", CellValue::Number(1.5)),
            ((0, 1025, 11), "INDEX(MINVERSE({1, 2; 3, 4}), 2, 2)", CellValue::Number(-0.5)),
            ((0, 1025, 12), "MINVERSE({1, 2; 2, 4})", CellValue::Error(CellError::Num)),
            ((0, 1025, 13), "MINVERSE({1, 2, 3; 4, 5, 6})", CellValue::Error(CellError::Value)),
            ((0, 1025, 14), "INDEX(MUNIT(3), 1, 1)", CellValue::Number(1.0)),
            ((0, 1025, 15), "INDEX(MUNIT(3), 1, 2)", CellValue::Number(0.0)),
            ((0, 1025, 16), "INDEX(MUNIT(3), 2, 2)", CellValue::Number(1.0)),
            ((0, 1025, 17), "INDEX(MUNIT(3), 3, 3)", CellValue::Number(1.0)),
            ((0, 1025, 18), "MUNIT(0)", CellValue::Error(CellError::Value)),
            ((0, 1025, 19), "ROUND(INDEX(MMULT({1, 2; 3, 4}, MINVERSE({1, 2; 3, 4})), 1, 1), 4)", CellValue::Number(1.0)),
            ((0, 1025, 20), "ROUND(INDEX(MMULT({1, 2; 3, 4}, MINVERSE({1, 2; 3, 4})), 1, 2), 4)", CellValue::Number(0.0)),

            // 3. Dynamic array generation and series (SEQUENCE, SERIESSUM, SUMPRODUCT, SUMX*)
            ((0, 1030, 0), "INDEX(SEQUENCE(3), 2, 1)", CellValue::Number(2.0)),
            ((0, 1030, 1), "INDEX(SEQUENCE(2, 3), 1, 3)", CellValue::Number(3.0)),
            ((0, 1030, 2), "INDEX(SEQUENCE(2, 3), 2, 1)", CellValue::Number(4.0)),
            ((0, 1030, 3), "INDEX(SEQUENCE(2, 3, 10, 5), 1, 1)", CellValue::Number(10.0)),
            ((0, 1030, 4), "INDEX(SEQUENCE(2, 3, 10, 5), 1, 2)", CellValue::Number(15.0)),
            ((0, 1030, 5), "INDEX(SEQUENCE(2, 3, 10, 5), 2, 1)", CellValue::Number(25.0)),
            ((0, 1030, 6), "SEQUENCE(0)", CellValue::Error(CellError::Value)),
            ((0, 1030, 7), "SEQUENCE(-1)", CellValue::Error(CellError::Value)),
            ((0, 1030, 8), "SERIESSUM(2, 1, 1, {1, 2, 3})", CellValue::Number(34.0)),
            ((0, 1030, 9), "SERIESSUM(2, 0, 2, {1, 1, 1})", CellValue::Number(21.0)),
            ((0, 1030, 10), "SUMPRODUCT({1, 2; 3, 4}, {2, 0; 1, 2})", CellValue::Number(13.0)),
            ((0, 1030, 11), "SUMPRODUCT({1, 2}, {1, 2, 3})", CellValue::Error(CellError::Value)),
            ((0, 1030, 12), "SUMX2MY2({2, 3}, {1, 2})", CellValue::Number(8.0)),
            ((0, 1030, 13), "SUMX2PY2({2, 3}, {1, 2})", CellValue::Number(18.0)),
            ((0, 1030, 14), "SUMXMY2({2, 3}, {1, 2})", CellValue::Number(2.0)),

            // 4. Advanced math, number theory, and combinatorics (GCD, LCM, MULTINOMIAL, COMBIN, FACT, MOD, QUOTIENT)
            ((0, 1035, 0), "GCD(24, 36)", CellValue::Number(12.0)),
            ((0, 1035, 1), "GCD(24, 36, 60)", CellValue::Number(12.0)),
            ((0, 1035, 2), "GCD(7, 13)", CellValue::Number(1.0)),
            ((0, 1035, 3), "GCD(0, 5)", CellValue::Number(5.0)),
            ((0, 1035, 4), "GCD(0, 0)", CellValue::Number(0.0)),
            ((0, 1035, 5), "GCD(5, -10)", CellValue::Error(CellError::Num)),
            ((0, 1035, 6), "LCM(4, 6)", CellValue::Number(12.0)),
            ((0, 1035, 7), "LCM(4, 6, 8)", CellValue::Number(24.0)),
            ((0, 1035, 8), "LCM(0, 5)", CellValue::Number(0.0)),
            ((0, 1035, 9), "LCM(5, -10)", CellValue::Error(CellError::Num)),
            ((0, 1035, 10), "MULTINOMIAL(1, 2, 3)", CellValue::Number(60.0)),
            ((0, 1035, 11), "MULTINOMIAL(2, 2)", CellValue::Number(6.0)),
            ((0, 1035, 12), "MULTINOMIAL(-1, 2)", CellValue::Error(CellError::Num)),
            ((0, 1035, 13), "COMBIN(5, 3)", CellValue::Number(10.0)),
            ((0, 1035, 14), "COMBIN(5, 0)", CellValue::Number(1.0)),
            ((0, 1035, 15), "COMBIN(5, 5)", CellValue::Number(1.0)),
            ((0, 1035, 16), "COMBIN(5, 6)", CellValue::Error(CellError::Num)),
            ((0, 1035, 17), "COMBIN(5, -1)", CellValue::Error(CellError::Num)),
            ((0, 1035, 18), "COMBINA(4, 3)", CellValue::Number(20.0)),
            ((0, 1035, 19), "COMBINA(4, 0)", CellValue::Number(1.0)),
            ((0, 1035, 20), "COMBINA(0, 0)", CellValue::Number(1.0)),
            ((0, 1035, 21), "COMBINA(0, 2)", CellValue::Error(CellError::Num)),
            ((0, 1035, 22), "FACT(5)", CellValue::Number(120.0)),
            ((0, 1035, 23), "FACT(0)", CellValue::Number(1.0)),
            ((0, 1035, 24), "FACT(-1)", CellValue::Error(CellError::Num)),
            ((0, 1035, 25), "FACTDOUBLE(7)", CellValue::Number(105.0)),
            ((0, 1035, 26), "FACTDOUBLE(6)", CellValue::Number(48.0)),
            ((0, 1035, 27), "FACTDOUBLE(0)", CellValue::Number(1.0)),
            ((0, 1035, 28), "FACTDOUBLE(-1)", CellValue::Number(1.0)),
            ((0, 1035, 29), "FACTDOUBLE(-2)", CellValue::Error(CellError::Num)),
            ((0, 1035, 30), "QUOTIENT(10, 3)", CellValue::Number(3.0)),
            ((0, 1035, 31), "QUOTIENT(-10, 3)", CellValue::Number(-3.0)),
            ((0, 1035, 32), "QUOTIENT(10, 0)", CellValue::Error(CellError::Div0)),
            ((0, 1035, 33), "MOD(10, 3)", CellValue::Number(1.0)),
            ((0, 1035, 34), "MOD(-10, 3)", CellValue::Number(2.0)),
            ((0, 1035, 35), "MOD(10, -3)", CellValue::Number(-2.0)),
            ((0, 1035, 36), "MOD(10, 0)", CellValue::Error(CellError::Div0)),

            // 5. Rounding, truncation, and significance boundaries (ROUND, MROUND, CEILING, FLOOR, TRUNC, INT, SIGN, SQRTPI)
            ((0, 1040, 0), "ROUND(2.15, 1)", CellValue::Number(2.2)),
            ((0, 1040, 1), "ROUND(2.14, 1)", CellValue::Number(2.1)),
            ((0, 1040, 2), "ROUND(-2.15, 1)", CellValue::Number(-2.2)),
            ((0, 1040, 3), "ROUNDUP(2.11, 1)", CellValue::Number(2.2)),
            ((0, 1040, 4), "ROUNDUP(-2.11, 1)", CellValue::Number(-2.2)),
            ((0, 1040, 5), "ROUNDDOWN(2.19, 1)", CellValue::Number(2.1)),
            ((0, 1040, 6), "ROUNDDOWN(-2.19, 1)", CellValue::Number(-2.1)),
            ((0, 1040, 7), "MROUND(10, 3)", CellValue::Number(9.0)),
            ((0, 1040, 8), "MROUND(11, 3)", CellValue::Number(12.0)),
            ((0, 1040, 9), "MROUND(-10, -3)", CellValue::Number(-9.0)),
            ((0, 1040, 10), "MROUND(10, -3)", CellValue::Error(CellError::Num)),
            ((0, 1040, 11), "MROUND(10, 0)", CellValue::Number(0.0)),
            ((0, 1040, 12), "CEILING.MATH(2.15)", CellValue::Number(3.0)),
            ((0, 1040, 13), "CEILING.MATH(-2.15)", CellValue::Number(-2.0)),
            ((0, 1040, 14), "CEILING.MATH(-2.15, 1, 1)", CellValue::Number(-3.0)),
            ((0, 1040, 15), "CEILING.MATH(11, 5)", CellValue::Number(15.0)),
            ((0, 1040, 16), "FLOOR.MATH(2.85)", CellValue::Number(2.0)),
            ((0, 1040, 17), "FLOOR.MATH(-2.85)", CellValue::Number(-3.0)),
            ((0, 1040, 18), "FLOOR.MATH(-2.85, 1, 1)", CellValue::Number(-2.0)),
            ((0, 1040, 19), "FLOOR.MATH(14, 5)", CellValue::Number(10.0)),
            ((0, 1040, 20), "ISO.CEILING(4.3)", CellValue::Number(5.0)),
            ((0, 1040, 21), "ISO.CEILING(-4.3)", CellValue::Number(-4.0)),
            ((0, 1040, 22), "ISO.CEILING(4.3, 2)", CellValue::Number(6.0)),
            ((0, 1040, 23), "ISO.CEILING(-4.3, 2)", CellValue::Number(-4.0)),
            ((0, 1040, 24), "TRUNC(3.7)", CellValue::Number(3.0)),
            ((0, 1040, 25), "TRUNC(-3.7)", CellValue::Number(-3.0)),
            ((0, 1040, 26), "TRUNC(3.75, 1)", CellValue::Number(3.7)),
            ((0, 1040, 27), "INT(3.7)", CellValue::Number(3.0)),
            ((0, 1040, 28), "INT(-3.7)", CellValue::Number(-4.0)),
            ((0, 1040, 29), "SIGN(5)", CellValue::Number(1.0)),
            ((0, 1040, 30), "SIGN(-5)", CellValue::Number(-1.0)),
            ((0, 1040, 31), "SIGN(0)", CellValue::Number(0.0)),
            ((0, 1040, 32), "ROUND(SQRTPI(1), 4)", CellValue::Number(1.7725)),
            ((0, 1040, 33), "ROUND(SQRTPI(4), 4)", CellValue::Number(3.5449)),
            ((0, 1040, 34), "SQRTPI(0)", CellValue::Number(0.0)),
            ((0, 1040, 35), "SQRTPI(-1)", CellValue::Error(CellError::Num)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_engineering_functions_comprehensive_edge_cases() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Number base conversions (BIN2DEC, OCT2DEC, HEX2DEC, BIN2OCT, BIN2HEX, OCT2BIN, OCT2HEX, HEX2BIN, HEX2OCT, DEC2BIN, DEC2OCT, DEC2HEX)
            ((0, 86, 0), "BIN2DEC(\"1010\")", CellValue::Number(10.0)),
            ((0, 86, 1), "BIN2DEC(\"1111111111\")", CellValue::Number(-1.0)),
            ((0, 86, 2), "BIN2DEC(\"1000000000\")", CellValue::Number(-512.0)),
            ((0, 86, 3), "BIN2DEC(\"0111111111\")", CellValue::Number(511.0)),
            ((0, 86, 4), "BIN2DEC(\"11111111111\")", CellValue::Error(CellError::Num)),
            ((0, 86, 5), "BIN2DEC(\"102\")", CellValue::Error(CellError::Num)),
            ((0, 86, 6), "BIN2OCT(\"1010\", 3)", CellValue::Text("012".into())),
            ((0, 86, 7), "BIN2OCT(\"1010\", 1)", CellValue::Error(CellError::Num)),
            ((0, 86, 8), "BIN2HEX(\"1111111111\")", CellValue::Text("FFFFFFFFFF".into())),
            ((0, 86, 9), "OCT2DEC(\"77\")", CellValue::Number(63.0)),
            ((0, 86, 10), "OCT2DEC(\"7777777777\")", CellValue::Number(-1.0)),
            ((0, 86, 11), "OCT2BIN(\"77\")", CellValue::Text("111111".into())),
            ((0, 86, 12), "OCT2HEX(\"77\")", CellValue::Text("3F".into())),
            ((0, 86, 13), "OCT2DEC(\"88\")", CellValue::Error(CellError::Num)),
            ((0, 86, 14), "HEX2DEC(\"FF\")", CellValue::Number(255.0)),
            ((0, 86, 15), "HEX2DEC(\"FFFFFFFFFF\")", CellValue::Number(-1.0)),
            ((0, 86, 16), "HEX2BIN(\"1F\")", CellValue::Text("11111".into())),
            ((0, 86, 17), "HEX2OCT(\"FF\")", CellValue::Text("377".into())),
            ((0, 86, 18), "HEX2DEC(\"FG\")", CellValue::Error(CellError::Num)),
            ((0, 86, 19), "DEC2BIN(10, 6)", CellValue::Text("001010".into())),
            ((0, 86, 20), "DEC2BIN(-1)", CellValue::Text("1111111111".into())),
            ((0, 86, 21), "DEC2BIN(512)", CellValue::Error(CellError::Num)),
            ((0, 86, 22), "DEC2BIN(-513)", CellValue::Error(CellError::Num)),
            ((0, 86, 23), "DEC2OCT(63)", CellValue::Text("77".into())),
            ((0, 86, 24), "DEC2OCT(-1)", CellValue::Text("7777777777".into())),
            ((0, 86, 25), "DEC2HEX(255)", CellValue::Text("FF".into())),
            ((0, 86, 26), "DEC2HEX(-1)", CellValue::Text("FFFFFFFFFF".into())),
            ((0, 86, 27), "DEC2BIN(10, 0)", CellValue::Error(CellError::Num)),
            ((0, 86, 28), "DEC2BIN(10, 11)", CellValue::Error(CellError::Num)),

            // 2. Bitwise operations (BITAND, BITOR, BITXOR, BITLSHIFT, BITRSHIFT)
            ((0, 87, 0), "_xlfn.BITAND(13, 27)", CellValue::Number(9.0)),
            ((0, 87, 1), "_xlfn.BITOR(23, 10)", CellValue::Number(31.0)),
            ((0, 87, 2), "_xlfn.BITXOR(23, 10)", CellValue::Number(29.0)),
            ((0, 87, 3), "_xlfn.BITLSHIFT(4, 2)", CellValue::Number(16.0)),
            ((0, 87, 4), "_xlfn.BITLSHIFT(16, -2)", CellValue::Number(4.0)),
            ((0, 87, 5), "_xlfn.BITRSHIFT(16, 2)", CellValue::Number(4.0)),
            ((0, 87, 6), "_xlfn.BITRSHIFT(4, -2)", CellValue::Number(16.0)),
            ((0, 87, 7), "_xlfn.BITAND(-1, 5)", CellValue::Error(CellError::Num)),
            ((0, 87, 8), "_xlfn.BITAND(2.5, 5)", CellValue::Error(CellError::Num)),
            ((0, 87, 9), "_xlfn.BITAND(281474976710656, 1)", CellValue::Error(CellError::Num)),
            ((0, 87, 10), "_xlfn.BITLSHIFT(1, 54)", CellValue::Error(CellError::Num)),
            ((0, 87, 11), "_xlfn.BITLSHIFT(1, -54)", CellValue::Error(CellError::Num)),
            ((0, 87, 12), "_xlfn.BITLSHIFT(0, 10)", CellValue::Number(0.0)),

            // 3. Predicates, special, error and Bessel functions (DELTA, GESTEP, ERF, ERFC, BESSEL)
            ((0, 88, 0), "DELTA(5, 5)", CellValue::Number(1.0)),
            ((0, 88, 1), "DELTA(5, 4)", CellValue::Number(0.0)),
            ((0, 88, 2), "DELTA(0)", CellValue::Number(1.0)),
            ((0, 88, 3), "DELTA(5)", CellValue::Number(0.0)),
            ((0, 88, 4), "GESTEP(5, 4)", CellValue::Number(1.0)),
            ((0, 88, 5), "GESTEP(5, 5)", CellValue::Number(1.0)),
            ((0, 88, 6), "GESTEP(4, 5)", CellValue::Number(0.0)),
            ((0, 88, 7), "GESTEP(5)", CellValue::Number(1.0)),
            ((0, 88, 8), "GESTEP(-1)", CellValue::Number(0.0)),
            ((0, 88, 9), "ERF(0)", CellValue::Number(0.0)),
            ((0, 88, 10), "ERFC(0)", CellValue::Number(1.0)),
            ((0, 88, 11), "_xlfn.ERF.PRECISE(0)", CellValue::Number(0.0)),
            ((0, 88, 12), "_xlfn.ERFC.PRECISE(0)", CellValue::Number(1.0)),
            ((0, 88, 13), "BESSELJ(0, 0)", CellValue::Number(1.0)),
            ((0, 88, 14), "BESSELJ(0, 1)", CellValue::Number(0.0)),
            ((0, 88, 15), "BESSELJ(1, -1)", CellValue::Error(CellError::Num)),
            ((0, 88, 16), "BESSELI(0, 0)", CellValue::Number(1.0)),
            ((0, 88, 17), "BESSELI(0, 1)", CellValue::Number(0.0)),
            ((0, 88, 18), "BESSELY(0, 0)", CellValue::Error(CellError::Num)),
            ((0, 88, 19), "BESSELK(0, 0)", CellValue::Error(CellError::Num)),

            // 4. Complex number operations (COMPLEX, IMREAL, IMAGINARY, IMABS, IMARGUMENT, IMCONJUGATE, IMEXP, IMLN, IMSQRT, IMSIN, IMCOS, IMDIV, IMSUB, IMSUM, IMPRODUCT, IMPOWER)
            ((0, 89, 0), "COMPLEX(3, 4)", CellValue::Text("3+4i".into())),
            ((0, 89, 1), "COMPLEX(3, -4)", CellValue::Text("3-4i".into())),
            ((0, 89, 2), "COMPLEX(3, 0)", CellValue::Text("3".into())),
            ((0, 89, 3), "COMPLEX(0, 4)", CellValue::Text("4i".into())),
            ((0, 89, 4), "COMPLEX(0, 1)", CellValue::Text("i".into())),
            ((0, 89, 5), "COMPLEX(0, -1)", CellValue::Text("-i".into())),
            ((0, 89, 6), "COMPLEX(0, 0)", CellValue::Text("0".into())),
            ((0, 89, 7), "COMPLEX(3, 4, \"j\")", CellValue::Text("3+4j".into())),
            ((0, 89, 8), "COMPLEX(3, 4, \"k\")", CellValue::Error(CellError::Value)),
            ((0, 89, 9), "IMREAL(\"3+4i\")", CellValue::Number(3.0)),
            ((0, 89, 10), "IMREAL(\"4i\")", CellValue::Number(0.0)),
            ((0, 89, 11), "IMREAL(\"5\")", CellValue::Number(5.0)),
            ((0, 89, 12), "IMAGINARY(\"3+4i\")", CellValue::Number(4.0)),
            ((0, 89, 13), "IMAGINARY(\"5\")", CellValue::Number(0.0)),
            ((0, 89, 14), "IMAGINARY(\"i\")", CellValue::Number(1.0)),
            ((0, 89, 15), "IMABS(\"3+4i\")", CellValue::Number(5.0)),
            ((0, 89, 16), "IMABS(\"0\")", CellValue::Number(0.0)),
            ((0, 89, 17), "IMCONJUGATE(\"3+4i\")", CellValue::Text("3-4i".into())),
            ((0, 89, 18), "IMARGUMENT(\"1\")", CellValue::Number(0.0)),
            ((0, 89, 19), "IMARGUMENT(\"0\")", CellValue::Error(CellError::Div0)),
            ((0, 89, 20), "IMEXP(\"0\")", CellValue::Text("1".into())),
            ((0, 89, 21), "IMLN(\"1\")", CellValue::Text("0".into())),
            ((0, 89, 22), "IMLN(\"0\")", CellValue::Error(CellError::Num)),
            ((0, 89, 23), "IMSQRT(\"-1\")", CellValue::Text("6.12323399573677E-17+i".into())),
            ((0, 89, 24), "IMSQRT(\"4\")", CellValue::Text("2".into())),
            ((0, 89, 25), "IMDIV(\"1\", \"0\")", CellValue::Error(CellError::Num)),
            ((0, 89, 26), "IMSUM(\"1+2i\", \"3+4i\")", CellValue::Text("4+6i".into())),
            ((0, 89, 27), "IMSUB(\"3+4i\", \"1+2i\")", CellValue::Text("2+2i".into())),
            ((0, 89, 28), "IMPRODUCT(\"1+2i\", \"1-2i\")", CellValue::Text("5".into())),
            ((0, 89, 29), "IMPOWER(\"2\", 3)", CellValue::Text("8".into())),

            // 5. Unit conversions (CONVERT)
            ((0, 90, 0), "CONVERT(1, \"m\", \"cm\")", CellValue::Number(100.0)),
            ((0, 90, 1), "CONVERT(1, \"km\", \"m\")", CellValue::Number(1000.0)),
            ((0, 90, 2), "CONVERT(1, \"in\", \"cm\")", CellValue::Number(2.54)),
            ((0, 90, 3), "CONVERT(1, \"ft\", \"in\")", CellValue::Number(12.0)),
            ((0, 90, 4), "CONVERT(1, \"yd\", \"ft\")", CellValue::Number(3.0)),
            ((0, 90, 5), "CONVERT(1, \"kg\", \"g\")", CellValue::Number(1000.0)),
            ((0, 90, 6), "CONVERT(1, \"day\", \"hr\")", CellValue::Number(24.0)),
            ((0, 90, 7), "CONVERT(1, \"hr\", \"mn\")", CellValue::Number(60.0)),
            ((0, 90, 8), "CONVERT(1, \"mn\", \"sec\")", CellValue::Number(60.0)),
            ((0, 90, 9), "CONVERT(0, \"C\", \"F\")", CellValue::Number(32.0)),
            ((0, 90, 10), "CONVERT(100, \"C\", \"F\")", CellValue::Number(212.0)),
            ((0, 90, 11), "CONVERT(0, \"C\", \"K\")", CellValue::Number(273.15)),
            ((0, 90, 12), "CONVERT(1, \"m\", \"kg\")", CellValue::Error(CellError::NA)),
            ((0, 90, 13), "CONVERT(1, \"invalid_unit\", \"m\")", CellValue::Error(CellError::NA)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_math_trig_matrix_comprehensive_edge_cases() {
        let mut book = MockBook::new(false);
        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Rounding and truncation functions (ROUND, ROUNDDOWN, ROUNDUP, MROUND, INT, TRUNC, EVEN, ODD, CEILING, FLOOR, CEILING.MATH, FLOOR.MATH, CEILING.PRECISE, FLOOR.PRECISE, ISO.CEILING)
            ((0, 91, 0), "ROUND(2.15, 1)", CellValue::Number(2.2)),
            ((0, 91, 1), "ROUND(2.149, 1)", CellValue::Number(2.1)),
            ((0, 91, 2), "ROUND(-2.15, 1)", CellValue::Number(-2.2)),
            ((0, 91, 3), "ROUND(21.5, -1)", CellValue::Number(20.0)),
            ((0, 91, 4), "ROUNDDOWN(2.19, 1)", CellValue::Number(2.1)),
            ((0, 91, 5), "ROUNDDOWN(-2.19, 1)", CellValue::Number(-2.1)),
            ((0, 91, 6), "ROUNDUP(2.11, 1)", CellValue::Number(2.2)),
            ((0, 91, 7), "ROUNDUP(-2.11, 1)", CellValue::Number(-2.2)),
            ((0, 91, 8), "MROUND(10, 3)", CellValue::Number(9.0)),
            ((0, 91, 9), "MROUND(11, 3)", CellValue::Number(12.0)),
            ((0, 91, 10), "MROUND(-10, -3)", CellValue::Number(-9.0)),
            ((0, 91, 11), "MROUND(10, -3)", CellValue::Error(CellError::Num)),
            ((0, 91, 12), "MROUND(10, 0)", CellValue::Number(0.0)),
            ((0, 91, 13), "INT(8.9)", CellValue::Number(8.0)),
            ((0, 91, 14), "INT(-8.9)", CellValue::Number(-9.0)),
            ((0, 91, 15), "TRUNC(8.9)", CellValue::Number(8.0)),
            ((0, 91, 16), "TRUNC(-8.9)", CellValue::Number(-8.0)),
            ((0, 91, 17), "TRUNC(8.912, 2)", CellValue::Number(8.91)),
            ((0, 91, 18), "EVEN(1.5)", CellValue::Number(2.0)),
            ((0, 91, 19), "EVEN(-1.5)", CellValue::Number(-2.0)),
            ((0, 91, 20), "EVEN(0)", CellValue::Number(0.0)),
            ((0, 91, 21), "EVEN(2)", CellValue::Number(2.0)),
            ((0, 91, 22), "ODD(1.5)", CellValue::Number(3.0)),
            ((0, 91, 23), "ODD(-1.5)", CellValue::Number(-3.0)),
            ((0, 91, 24), "ODD(0)", CellValue::Number(1.0)),
            ((0, 91, 25), "ODD(1)", CellValue::Number(1.0)),
            ((0, 91, 26), "CEILING(2.5, 1)", CellValue::Number(3.0)),
            ((0, 91, 27), "CEILING(-2.5, -1)", CellValue::Number(-3.0)),
            ((0, 91, 28), "CEILING(-2.5, 1)", CellValue::Number(-2.0)),
            ((0, 91, 29), "FLOOR(2.5, 1)", CellValue::Number(2.0)),
            ((0, 91, 30), "FLOOR(-2.5, -1)", CellValue::Number(-2.0)),
            ((0, 91, 31), "FLOOR(-2.5, 1)", CellValue::Number(-3.0)),
            ((0, 91, 32), "_xlfn.CEILING.MATH(2.5)", CellValue::Number(3.0)),
            ((0, 91, 33), "_xlfn.CEILING.MATH(-2.5, 1, 0)", CellValue::Number(-2.0)),
            ((0, 91, 34), "_xlfn.CEILING.MATH(-2.5, 1, 1)", CellValue::Number(-3.0)),
            ((0, 91, 35), "_xlfn.FLOOR.MATH(2.5)", CellValue::Number(2.0)),
            ((0, 91, 36), "_xlfn.FLOOR.MATH(-2.5, 1, 0)", CellValue::Number(-3.0)),
            ((0, 91, 37), "_xlfn.FLOOR.MATH(-2.5, 1, 1)", CellValue::Number(-2.0)),
            ((0, 91, 38), "_xlfn.CEILING.PRECISE(2.5, 1)", CellValue::Number(3.0)),
            ((0, 91, 39), "_xlfn.CEILING.PRECISE(-2.5, 1)", CellValue::Number(-2.0)),
            ((0, 91, 40), "_xlfn.FLOOR.PRECISE(2.5, 1)", CellValue::Number(2.0)),
            ((0, 91, 41), "_xlfn.FLOOR.PRECISE(-2.5, 1)", CellValue::Number(-3.0)),
            ((0, 91, 42), "_xlfn.ISO.CEILING(2.5)", CellValue::Number(3.0)),
            ((0, 91, 43), "_xlfn.ISO.CEILING(-2.5)", CellValue::Number(-2.0)),

            // 2. Modulo, quotient, factorials, combinatorics, number theory (MOD, QUOTIENT, FACT, FACTDOUBLE, COMBIN, COMBINA, GCD, LCM, MULTINOMIAL)
            ((0, 92, 0), "MOD(3, 2)", CellValue::Number(1.0)),
            ((0, 92, 1), "MOD(-3, 2)", CellValue::Number(1.0)),
            ((0, 92, 2), "MOD(3, -2)", CellValue::Number(-1.0)),
            ((0, 92, 3), "MOD(-3, -2)", CellValue::Number(-1.0)),
            ((0, 92, 4), "MOD(3, 0)", CellValue::Error(CellError::Div0)),
            ((0, 92, 5), "QUOTIENT(5, 2)", CellValue::Number(2.0)),
            ((0, 92, 6), "QUOTIENT(-5, 2)", CellValue::Number(-2.0)),
            ((0, 92, 7), "QUOTIENT(5, -2)", CellValue::Number(-2.0)),
            ((0, 92, 8), "QUOTIENT(5, 0)", CellValue::Error(CellError::Div0)),
            ((0, 92, 9), "FACT(5)", CellValue::Number(120.0)),
            ((0, 92, 10), "FACT(0)", CellValue::Number(1.0)),
            ((0, 92, 11), "FACT(-1)", CellValue::Error(CellError::Num)),
            ((0, 92, 12), "FACTDOUBLE(5)", CellValue::Number(15.0)),
            ((0, 92, 13), "FACTDOUBLE(6)", CellValue::Number(48.0)),
            ((0, 92, 14), "FACTDOUBLE(0)", CellValue::Number(1.0)),
            ((0, 92, 15), "FACTDOUBLE(-1)", CellValue::Number(1.0)),
            ((0, 92, 16), "FACTDOUBLE(-2)", CellValue::Error(CellError::Num)),
            ((0, 92, 17), "COMBIN(5, 2)", CellValue::Number(10.0)),
            ((0, 92, 18), "COMBIN(5, 0)", CellValue::Number(1.0)),
            ((0, 92, 19), "COMBIN(5, 5)", CellValue::Number(1.0)),
            ((0, 92, 20), "COMBIN(5, 6)", CellValue::Error(CellError::Num)),
            ((0, 92, 21), "COMBIN(-1, 2)", CellValue::Error(CellError::Num)),
            ((0, 92, 22), "_xlfn.COMBINA(5, 2)", CellValue::Number(15.0)),
            ((0, 92, 23), "_xlfn.COMBINA(0, 0)", CellValue::Number(1.0)),
            ((0, 92, 24), "GCD(24, 36)", CellValue::Number(12.0)),
            ((0, 92, 25), "GCD(0, 5)", CellValue::Number(5.0)),
            ((0, 92, 26), "GCD(0, 0)", CellValue::Number(0.0)),
            ((0, 92, 27), "GCD(-5, 2)", CellValue::Error(CellError::Num)),
            ((0, 92, 28), "LCM(4, 6)", CellValue::Number(12.0)),
            ((0, 92, 29), "LCM(0, 5)", CellValue::Number(0.0)),
            ((0, 92, 30), "LCM(-4, 6)", CellValue::Error(CellError::Num)),
            ((0, 92, 31), "MULTINOMIAL(1, 2, 3)", CellValue::Number(60.0)),

            // 3. Trigonometric, inverse, and hyperbolic functions (SIN, COS, TAN, COT, CSC, SEC, SINH, COSH, TANH, COTH, CSCH, SECH, ASIN, ACOS, ATAN, ATAN2, ACOT, ACOTH, ASINH, ACOSH, ATANH, DEGREES, RADIANS)
            ((0, 93, 0), "SIN(0)", CellValue::Number(0.0)),
            ((0, 93, 1), "COS(0)", CellValue::Number(1.0)),
            ((0, 93, 2), "TAN(0)", CellValue::Number(0.0)),
            ((0, 93, 3), "COT(0)", CellValue::Error(CellError::Div0)),
            ((0, 93, 4), "_xlfn.CSC(0)", CellValue::Error(CellError::Div0)),
            ((0, 93, 5), "_xlfn.SEC(0)", CellValue::Number(1.0)),
            ((0, 93, 6), "SINH(0)", CellValue::Number(0.0)),
            ((0, 93, 7), "COSH(0)", CellValue::Number(1.0)),
            ((0, 93, 8), "TANH(0)", CellValue::Number(0.0)),
            ((0, 93, 9), "_xlfn.COTH(0)", CellValue::Error(CellError::Div0)),
            ((0, 93, 10), "_xlfn.CSCH(0)", CellValue::Error(CellError::Div0)),
            ((0, 93, 11), "_xlfn.SECH(0)", CellValue::Number(1.0)),
            ((0, 93, 12), "ASIN(0)", CellValue::Number(0.0)),
            ((0, 93, 13), "ASIN(1.0)", CellValue::Number(std::f64::consts::FRAC_PI_2)),
            ((0, 93, 14), "ASIN(1.1)", CellValue::Error(CellError::Num)),
            ((0, 93, 15), "ACOS(1)", CellValue::Number(0.0)),
            ((0, 93, 16), "ACOS(0)", CellValue::Number(std::f64::consts::FRAC_PI_2)),
            ((0, 93, 17), "ACOS(1.1)", CellValue::Error(CellError::Num)),
            ((0, 93, 18), "ATAN(0)", CellValue::Number(0.0)),
            ((0, 93, 19), "ATAN2(1, 1)", CellValue::Number(std::f64::consts::FRAC_PI_4)),
            ((0, 93, 20), "ATAN2(0, 0)", CellValue::Error(CellError::Div0)),
            ((0, 93, 21), "_xlfn.ACOT(0)", CellValue::Number(std::f64::consts::FRAC_PI_2)),
            ((0, 93, 22), "_xlfn.ASINH(0)", CellValue::Number(0.0)),
            ((0, 93, 23), "_xlfn.ACOSH(1)", CellValue::Number(0.0)),
            ((0, 93, 24), "_xlfn.ACOSH(0.9)", CellValue::Error(CellError::Num)),
            ((0, 93, 25), "_xlfn.ATANH(0)", CellValue::Number(0.0)),
            ((0, 93, 26), "_xlfn.ATANH(1)", CellValue::Error(CellError::Num)),
            ((0, 93, 27), "_xlfn.ACOTH(2)", CellValue::Number((0.5f64).atanh())),
            ((0, 93, 28), "_xlfn.ACOTH(1)", CellValue::Error(CellError::Num)),
            ((0, 93, 29), "DEGREES(PI())", CellValue::Number(180.0)),
            ((0, 93, 30), "RADIANS(180)", CellValue::Number(std::f64::consts::PI)),

            // 4. Exponential, logarithmic, power, root, sign, absolute, PI (EXP, LN, LOG10, LOG, SQRT, SQRTPI, POWER, SIGN, ABS, PI)
            ((0, 94, 0), "EXP(0)", CellValue::Number(1.0)),
            ((0, 94, 1), "LN(1)", CellValue::Number(0.0)),
            ((0, 94, 2), "LN(0)", CellValue::Error(CellError::Num)),
            ((0, 94, 3), "LN(-1)", CellValue::Error(CellError::Num)),
            ((0, 94, 4), "LOG10(100)", CellValue::Number(2.0)),
            ((0, 94, 5), "LOG10(1)", CellValue::Number(0.0)),
            ((0, 94, 6), "LOG10(0)", CellValue::Error(CellError::Num)),
            ((0, 94, 7), "LOG(8, 2)", CellValue::Number(3.0)),
            ((0, 94, 8), "LOG(100)", CellValue::Number(2.0)),
            ((0, 94, 9), "LOG(8, 1)", CellValue::Error(CellError::Div0)),
            ((0, 94, 10), "LOG(8, 0)", CellValue::Error(CellError::Num)),
            ((0, 94, 11), "LOG(0, 2)", CellValue::Error(CellError::Num)),
            ((0, 94, 12), "SQRT(4)", CellValue::Number(2.0)),
            ((0, 94, 13), "SQRT(0)", CellValue::Number(0.0)),
            ((0, 94, 14), "SQRT(-1)", CellValue::Error(CellError::Num)),
            ((0, 94, 15), "SQRTPI(0)", CellValue::Number(0.0)),
            ((0, 94, 16), "SQRTPI(1)", CellValue::Number(std::f64::consts::PI.sqrt())),
            ((0, 94, 17), "SQRTPI(-1)", CellValue::Error(CellError::Num)),
            ((0, 94, 18), "POWER(2, 3)", CellValue::Number(8.0)),
            ((0, 94, 19), "POWER(4, 0.5)", CellValue::Number(2.0)),
            ((0, 94, 20), "SIGN(10)", CellValue::Number(1.0)),
            ((0, 94, 21), "SIGN(-10)", CellValue::Number(-1.0)),
            ((0, 94, 22), "SIGN(0)", CellValue::Number(0.0)),
            ((0, 94, 23), "ABS(-5)", CellValue::Number(5.0)),
            ((0, 94, 24), "PI()", CellValue::Number(std::f64::consts::PI)),

            // 5. Linear algebra, matrices, series, bases, roman numerals (MDETERM, MINVERSE, MUNIT, SUMPRODUCT, SERIESSUM, BASE, DECIMAL, ROMAN, ARABIC)
            ((0, 95, 0), "MDETERM({1,2;3,4})", CellValue::Number(-2.0)),
            ((0, 95, 1), "MDETERM({1,2,3;4,5,6})", CellValue::Error(CellError::Value)),
            ((0, 95, 2), "MINVERSE({1,2;2,4})", CellValue::Error(CellError::Num)),
            ((0, 95, 3), "_xlfn.MUNIT(0)", CellValue::Error(CellError::Value)),
            ((0, 95, 4), "_xlfn.MUNIT(-1)", CellValue::Error(CellError::Value)),
            ((0, 95, 5), "SUMPRODUCT({1,2;3,4}, {5,6;7,8})", CellValue::Number(70.0)),
            ((0, 95, 6), "SUMPRODUCT({1,2}, {1,2,3})", CellValue::Error(CellError::Value)),
            ((0, 95, 7), "SERIESSUM(2, 0, 1, {1, 2, 3})", CellValue::Number(17.0)),
            ((0, 95, 8), "_xlfn.BASE(17, 16)", CellValue::Text("11".into())),
            ((0, 95, 9), "_xlfn.BASE(17, 16, 4)", CellValue::Text("0011".into())),
            ((0, 95, 10), "_xlfn.BASE(17, 1)", CellValue::Error(CellError::Num)),
            ((0, 95, 11), "_xlfn.BASE(17, 37)", CellValue::Error(CellError::Num)),
            ((0, 95, 12), "_xlfn.DECIMAL(\"11\", 16)", CellValue::Number(17.0)),
            ((0, 95, 13), "_xlfn.DECIMAL(\"G\", 16)", CellValue::Error(CellError::Num)),
            ((0, 95, 14), "_xlfn.DECIMAL(\"\", 10)", CellValue::Number(0.0)),
            ((0, 95, 15), "ROMAN(1)", CellValue::Text("I".into())),
            ((0, 95, 16), "ROMAN(4)", CellValue::Text("IV".into())),
            ((0, 95, 17), "ROMAN(9)", CellValue::Text("IX".into())),
            ((0, 95, 18), "ROMAN(40)", CellValue::Text("XL".into())),
            ((0, 95, 19), "ROMAN(90)", CellValue::Text("XC".into())),
            ((0, 95, 20), "ROMAN(400)", CellValue::Text("CD".into())),
            ((0, 95, 21), "ROMAN(900)", CellValue::Text("CM".into())),
            ((0, 95, 22), "ROMAN(2026)", CellValue::Text("MMXXVI".into())),
            ((0, 95, 23), "ROMAN(4000)", CellValue::Error(CellError::Value)),
            ((0, 95, 24), "ROMAN(-1)", CellValue::Error(CellError::Value)),
            ((0, 95, 25), "_xlfn.ARABIC(\"MMXXVI\")", CellValue::Number(2026.0)),
            ((0, 95, 26), "_xlfn.ARABIC(\"IV\")", CellValue::Number(4.0)),
            ((0, 95, 27), "_xlfn.ARABIC(\"\")", CellValue::Number(0.0)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }

    #[test]
    fn test_dynamic_array_lambda_ets_and_groupby_edge_cases() {
        let mut book = MockBook::new(false);
        book.dynamic = true;

        // Groupby / Pivotby sample data at rows 1120..1124 (A1121:B1125 in Excel):
        // A1121="A", B1121=10
        // A1122="B", B1122=20
        // A1123="A", B1123=30
        // A1124="B", B1124=40
        // A1125="A", B1125=50
        book.values.insert((0, 1120, 0), CellValue::Text("A".into()));
        book.values.insert((0, 1120, 1), CellValue::Number(10.0));
        book.values.insert((0, 1121, 0), CellValue::Text("B".into()));
        book.values.insert((0, 1121, 1), CellValue::Number(20.0));
        book.values.insert((0, 1122, 0), CellValue::Text("A".into()));
        book.values.insert((0, 1122, 1), CellValue::Number(30.0));
        book.values.insert((0, 1123, 0), CellValue::Text("B".into()));
        book.values.insert((0, 1123, 1), CellValue::Number(40.0));
        book.values.insert((0, 1124, 0), CellValue::Text("A".into()));
        book.values.insert((0, 1124, 1), CellValue::Number(50.0));

        let cases: &[((usize, u32, u32), &str, CellValue)] = &[
            // 1. Dynamic array functions (FILTER, SORT, SORTBY, UNIQUE)
            ((0, 1130, 0), "INDEX(FILTER({1, 10; 2, 20; 3, 30}, {TRUE; FALSE; TRUE}), 1, 1)", CellValue::Number(1.0)),
            ((0, 1130, 1), "INDEX(FILTER({1, 10; 2, 20; 3, 30}, {TRUE; FALSE; TRUE}), 2, 2)", CellValue::Number(30.0)),
            ((0, 1130, 2), "ROWS(FILTER({1, 10; 2, 20; 3, 30}, {TRUE; FALSE; TRUE}))", CellValue::Number(2.0)),
            ((0, 1130, 3), "COLUMNS(FILTER({1, 10; 2, 20; 3, 30}, {TRUE; FALSE; TRUE}))", CellValue::Number(2.0)),
            ((0, 1130, 4), "FILTER({1; 2; 3}, {FALSE; FALSE; FALSE}, \"None\")", CellValue::Text("None".into())),
            ((0, 1130, 5), "FILTER({1; 2; 3}, {FALSE; FALSE; FALSE}, -999)", CellValue::Number(-999.0)),
            ((0, 1130, 6), "FILTER({1; 2; 3}, {FALSE; FALSE; FALSE})", CellValue::Error(CellError::Calc)),
            ((0, 1130, 7), "FILTER({1; 2; 3}, {TRUE; FALSE})", CellValue::Error(CellError::Value)),
            ((0, 1130, 8), "INDEX(SORT({5; 2; 8; 1}), 1, 1)", CellValue::Number(1.0)),
            ((0, 1130, 9), "INDEX(SORT({5; 2; 8; 1}), 4, 1)", CellValue::Number(8.0)),
            ((0, 1130, 10), "INDEX(SORT({5; 2; 8; 1}, 1, -1), 1, 1)", CellValue::Number(8.0)),
            ((0, 1130, 11), "INDEX(SORT({5; 2; 8; 1}, 1, -1), 4, 1)", CellValue::Number(1.0)),
            ((0, 1130, 12), "INDEX(SORT({10, 30; 20, 10; 30, 20}, 2, 1), 1, 1)", CellValue::Number(20.0)),
            ((0, 1130, 13), "INDEX(SORT({10, 30; 20, 10; 30, 20}, 2, -1), 1, 1)", CellValue::Number(10.0)),
            ((0, 1130, 14), "INDEX(SORT({5, 1, 9}, 1, 1, TRUE), 1, 1)", CellValue::Number(1.0)),
            ((0, 1130, 15), "INDEX(SORT({5, 1, 9}, 1, 1, TRUE), 1, 3)", CellValue::Number(9.0)),
            ((0, 1130, 16), "SORT({1; 2}, 3)", CellValue::Error(CellError::Value)),
            ((0, 1130, 17), "SORT({1; 2}, 1, 0)", CellValue::Error(CellError::Value)),
            ((0, 1130, 18), "INDEX(SORTBY({\"A\"; \"B\"; \"C\"}, {3; 1; 2}), 1, 1)", CellValue::Text("B".into())),
            ((0, 1130, 19), "INDEX(SORTBY({\"A\"; \"B\"; \"C\"}, {3; 1; 2}, -1), 1, 1)", CellValue::Text("A".into())),
            ((0, 1130, 20), "INDEX(SORTBY({\"A\"; \"B\"; \"C\"}, {1; 1; 2}, 1, {2; 1; 1}, 1), 1, 1)", CellValue::Text("B".into())),
            ((0, 1130, 21), "SORTBY({\"A\"; \"B\"; \"C\"}, {1; 2})", CellValue::Error(CellError::Value)),
            ((0, 1130, 22), "ROWS(UNIQUE({1; 2; 2; 3; 3; 3}))", CellValue::Number(3.0)),
            ((0, 1130, 23), "INDEX(UNIQUE({1; 2; 2; 3; 3; 3}), 2, 1)", CellValue::Number(2.0)),
            ((0, 1130, 24), "ROWS(UNIQUE({1; 2; 2; 3; 3; 3}, FALSE, TRUE))", CellValue::Number(1.0)),
            ((0, 1130, 25), "INDEX(UNIQUE({1; 2; 2; 3; 3; 3}, FALSE, TRUE), 1, 1)", CellValue::Number(1.0)),
            ((0, 1130, 26), "UNIQUE({1; 1; 2; 2}, FALSE, TRUE)", CellValue::Error(CellError::Calc)),
            ((0, 1130, 27), "COLUMNS(UNIQUE({10, 20, 20, 30}, TRUE))", CellValue::Number(3.0)),

            // 2. LAMBDA and helper functions (MAKEARRAY, MAP, REDUCE, SCAN, BYROW, BYCOL)
            ((0, 1140, 0), "(LAMBDA(x, x * 3 + 1))(5)", CellValue::Number(16.0)),
            ((0, 1140, 1), "(LAMBDA(a, b, a^2 + b^2))(3, 4)", CellValue::Number(25.0)),
            ((0, 1140, 2), "(LAMBDA(x, y, z, x * 100 + y * 10 + z))(2, 4, 6)", CellValue::Number(246.0)),
            ((0, 1140, 3), "(LAMBDA(777))()", CellValue::Number(777.0)),
            ((0, 1140, 4), "(LAMBDA(x, LAMBDA(y, x * 10 + y)))(5)(8)", CellValue::Number(58.0)),
            ((0, 1140, 5), "(LAMBDA(x, y, IF(ISOMITTED(y), x * 2, x + y)))(10, )", CellValue::Number(20.0)),
            ((0, 1140, 6), "(LAMBDA(x, y, IF(ISOMITTED(y), x * 2, x + y)))(10, 3)", CellValue::Number(13.0)),
            ((0, 1140, 7), "INDEX(MAKEARRAY(2, 3, LAMBDA(r, c, r * 10 + c)), 1, 1)", CellValue::Number(11.0)),
            ((0, 1140, 8), "INDEX(MAKEARRAY(2, 3, LAMBDA(r, c, r * 10 + c)), 2, 3)", CellValue::Number(23.0)),
            ((0, 1140, 9), "MAKEARRAY(0, 3, LAMBDA(r, c, 1))", CellValue::Error(CellError::Value)),
            ((0, 1140, 10), "MAKEARRAY(2, -1, LAMBDA(r, c, 1))", CellValue::Error(CellError::Value)),
            ((0, 1140, 11), "MAKEARRAY(2.5, 3, LAMBDA(r, c, 1))", CellValue::Error(CellError::Value)),
            ((0, 1140, 12), "MAKEARRAY(2, 2, LAMBDA(r, c, {1, 2}))", CellValue::Error(CellError::Value)),
            ((0, 1140, 13), "MAKEARRAY(2, 2, LAMBDA(r, 1))", CellValue::Error(CellError::Value)),
            ((0, 1140, 14), "INDEX(MAP({1, 2; 3, 4}, LAMBDA(x, x * 10)), 1, 2)", CellValue::Number(20.0)),
            ((0, 1140, 15), "INDEX(MAP({1, 2; 3, 4}, LAMBDA(x, x * 10)), 2, 1)", CellValue::Number(30.0)),
            ((0, 1140, 16), "INDEX(MAP({1; 2}, {10; 20}, LAMBDA(a, b, a + b)), 1, 1)", CellValue::Number(11.0)),
            ((0, 1140, 17), "INDEX(MAP({1; 2}, {10; 20}, LAMBDA(a, b, a + b)), 2, 1)", CellValue::Number(22.0)),
            ((0, 1140, 18), "MAP({1; 2}, LAMBDA(a, b, a + b))", CellValue::Error(CellError::Value)),
            ((0, 1140, 19), "REDUCE(0, {1, 2, 3, 4, 5}, LAMBDA(acc, x, acc + x))", CellValue::Number(15.0)),
            ((0, 1140, 20), "REDUCE(1, {1, 2, 3, 4}, LAMBDA(acc, x, acc * x))", CellValue::Number(24.0)),
            ((0, 1140, 21), "REDUCE(\"A\", {\"B\", \"C\", \"D\"}, LAMBDA(acc, x, acc & x))", CellValue::Text("ABCD".into())),
            ((0, 1140, 22), "REDUCE(0, {1, 2}, LAMBDA(acc, acc + 1))", CellValue::Error(CellError::Value)),
            ((0, 1140, 23), "INDEX(SCAN(0, {1, 2, 3, 4}, LAMBDA(acc, x, acc + x)), 1, 1)", CellValue::Number(1.0)),
            ((0, 1140, 24), "INDEX(SCAN(0, {1, 2, 3, 4}, LAMBDA(acc, x, acc + x)), 1, 4)", CellValue::Number(10.0)),
            ((0, 1140, 25), "INDEX(SCAN(1, {2, 3, 4}, LAMBDA(acc, x, acc * x)), 1, 3)", CellValue::Number(24.0)),
            ((0, 1140, 26), "INDEX(BYROW({1, 2; 3, 4}, LAMBDA(r, SUM(r))), 1, 1)", CellValue::Number(3.0)),
            ((0, 1140, 27), "INDEX(BYROW({1, 2; 3, 4}, LAMBDA(r, SUM(r))), 2, 1)", CellValue::Number(7.0)),
            ((0, 1140, 28), "INDEX(BYROW({10, 2; 3, 40}, LAMBDA(r, MAX(r))), 1, 1)", CellValue::Number(10.0)),
            ((0, 1140, 29), "INDEX(BYROW({10, 2; 3, 40}, LAMBDA(r, MAX(r))), 2, 1)", CellValue::Number(40.0)),
            ((0, 1140, 30), "INDEX(BYCOL({1, 2; 3, 4}, LAMBDA(c, SUM(c))), 1, 1)", CellValue::Number(4.0)),
            ((0, 1140, 31), "INDEX(BYCOL({1, 2; 3, 4}, LAMBDA(c, SUM(c))), 1, 2)", CellValue::Number(6.0)),
            ((0, 1140, 32), "INDEX(BYCOL({10, 20; 30, 4}, LAMBDA(c, MIN(c))), 1, 1)", CellValue::Number(10.0)),
            ((0, 1140, 33), "INDEX(BYCOL({10, 20; 30, 4}, LAMBDA(c, MIN(c))), 1, 2)", CellValue::Number(4.0)),
            ((0, 1140, 34), "BYROW({1, 2; 3, 4}, LAMBDA(r, c, 1))", CellValue::Error(CellError::Value)),

            // 3. ETS forecasting functions (FORECAST.ETS, CONFINT, SEASONALITY, STAT)
            ((0, 1150, 0), "ROUND(FORECAST.ETS(6, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}), 1)", CellValue::Number(60.0)),
            ((0, 1150, 1), "ROUND(FORECAST.ETS(7, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}), 1)", CellValue::Number(70.0)),
            ((0, 1150, 2), "ROUND(FORECAST.ETS(6, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, 0), 1)", CellValue::Number(60.0)),
            ((0, 1150, 3), "ROUND(FORECAST.ETS(6, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, 1), 1)", CellValue::Number(60.0)),
            ((0, 1150, 4), "FORECAST.ETS.SEASONALITY({10; 20; 30; 40; 50}, {1; 2; 3; 4; 5})", CellValue::Number(0.0)),
            ((0, 1150, 5), "ROUND(FORECAST.ETS.STAT({10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, 8), 1)", CellValue::Number(1.0)),
            ((0, 1150, 6), "FORECAST.ETS(6, {10; 20; 30}, {1; 2; 3; 4})", CellValue::Error(CellError::NA)),
            ((0, 1150, 7), "FORECAST.ETS(6, {10; 20; 30; 40}, {1, 2; 3, 4})", CellValue::Error(CellError::Num)),
            ((0, 1150, 8), "FORECAST.ETS(6, {10; 20; 30; 40}, {1; \"bad\"; 3; 4})", CellValue::Error(CellError::Num)),
            ((0, 1150, 9), "FORECAST.ETS(6, {10; 20; 30; 40}, {4; 3; 2; 1})", CellValue::Error(CellError::Num)),
            ((0, 1150, 10), "FORECAST.ETS(6, {10; 20; 30; 40}, {\"a\"; \"b\"; \"c\"; \"d\"})", CellValue::Error(CellError::Div0)),
            ((0, 1150, 11), "FORECAST.ETS(6, {10; \"text\"; 30; 40}, {1; 2; 3; 4})", CellValue::Error(CellError::Num)),
            ((0, 1150, 12), "FORECAST.ETS(6, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, -1)", CellValue::Error(CellError::Num)),
            ((0, 1150, 13), "FORECAST.ETS(6, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, 8761)", CellValue::Error(CellError::Num)),
            ((0, 1150, 14), "FORECAST.ETS(6, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, 2.5)", CellValue::Error(CellError::Num)),
            ((0, 1150, 15), "FORECAST.ETS(6, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, 1, 2)", CellValue::Error(CellError::Num)),
            ((0, 1150, 16), "FORECAST.ETS(6, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, 1, 0, 8)", CellValue::Error(CellError::Num)),
            ((0, 1150, 17), "FORECAST.ETS.CONFINT(6, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, 0)", CellValue::Number(0.0)),
            ((0, 1150, 18), "FORECAST.ETS.CONFINT(6, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, 1)", CellValue::Error(CellError::Num)),
            ((0, 1150, 19), "FORECAST.ETS.CONFINT(6, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, -0.5)", CellValue::Error(CellError::Num)),
            ((0, 1150, 20), "FORECAST.ETS.CONFINT(6, {10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, 1.5)", CellValue::Error(CellError::Num)),
            ((0, 1150, 21), "FORECAST.ETS.CONFINT(6, {10; 20; 30}, {1; 2; 3; 4})", CellValue::Error(CellError::NA)),
            ((0, 1150, 22), "FORECAST.ETS.SEASONALITY({10; 20; 30}, {1; 2; 3; 4})", CellValue::Error(CellError::NA)),
            ((0, 1150, 23), "FORECAST.ETS.STAT({10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, 0)", CellValue::Error(CellError::Num)),
            ((0, 1150, 24), "FORECAST.ETS.STAT({10; 20; 30; 40; 50}, {1; 2; 3; 4; 5}, 9)", CellValue::Error(CellError::Num)),
            ((0, 1150, 25), "FORECAST.ETS.STAT({10; 20; 30}, {1; 2; 3; 4}, 1)", CellValue::Error(CellError::NA)),

            // 4. Dynamic aggregation & grouping (GROUPBY, PIVOTBY)
            ((0, 1160, 0), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 0), 1, 1)", CellValue::Text("A".into())),
            ((0, 1160, 1), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 0), 1, 2)", CellValue::Number(90.0)),
            ((0, 1160, 2), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 0), 2, 1)", CellValue::Text("B".into())),
            ((0, 1160, 3), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 0), 2, 2)", CellValue::Number(60.0)),
            ((0, 1160, 4), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 1), 3, 1)", CellValue::Text("合計".into())),
            ((0, 1160, 5), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 1), 3, 2)", CellValue::Number(150.0)),
            ((0, 1160, 6), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, -1), 1, 1)", CellValue::Text("合計".into())),
            ((0, 1160, 7), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, -1), 1, 2)", CellValue::Number(150.0)),
            ((0, 1160, 8), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, AVERAGE, 0, 0), 1, 2)", CellValue::Number(30.0)),
            ((0, 1160, 9), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, AVERAGE, 0, 0), 2, 2)", CellValue::Number(30.0)),
            ((0, 1160, 10), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, COUNT, 0, 0), 1, 2)", CellValue::Number(3.0)),
            ((0, 1160, 11), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, COUNT, 0, 0), 2, 2)", CellValue::Number(2.0)),
            ((0, 1160, 12), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, MAX, 0, 0), 1, 2)", CellValue::Number(50.0)),
            ((0, 1160, 13), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, MIN, 0, 0), 1, 2)", CellValue::Number(10.0)),
            ((0, 1160, 14), "GROUPBY(A1121:A1125, B1121:B1124, SUM)", CellValue::Error(CellError::Value)),
            ((0, 1160, 15), "GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 5)", CellValue::Error(CellError::Value)),
            ((0, 1160, 16), "GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 0, 10)", CellValue::Error(CellError::Value)),
            ((0, 1160, 17), "GROUPBY(A1121:A1125, B1121:B1125, 999)", CellValue::Error(CellError::Value)),
            ((0, 1160, 18), "GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 0, , {TRUE; FALSE})", CellValue::Error(CellError::Value)),
            ((0, 1160, 19), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 0, , B1121:B1125 > 25), 1, 1)", CellValue::Text("A".into())),
            ((0, 1160, 20), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 0, , B1121:B1125 > 25), 1, 2)", CellValue::Number(80.0)),
            ((0, 1160, 21), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 0, , B1121:B1125 > 25), 2, 1)", CellValue::Text("B".into())),
            ((0, 1160, 22), "INDEX(GROUPBY(A1121:A1125, B1121:B1125, SUM, 0, 0, , B1121:B1125 > 25), 2, 2)", CellValue::Number(40.0)),
            ((0, 1160, 23), "PIVOTBY(A1121:A1125, B1121:B1125, B1121:B1125, SUM, 0, 5)", CellValue::Error(CellError::Value)),

            // 5. Array slicing and reshaping functions (CHOOSEROWS, CHOOSECOLS, TAKE, DROP, EXPAND, TOCOL, TOROW, WRAPROWS, WRAPCOLS)
            ((0, 1170, 0), "INDEX(CHOOSEROWS({10; 20; 30; 40}, -1), 1, 1)", CellValue::Number(40.0)),
            ((0, 1170, 1), "INDEX(CHOOSEROWS({10; 20; 30; 40}, -2), 1, 1)", CellValue::Number(30.0)),
            ((0, 1170, 2), "INDEX(CHOOSEROWS({10; 20; 30; 40}, 1, -1), 2, 1)", CellValue::Number(40.0)),
            ((0, 1170, 3), "CHOOSEROWS({10; 20; 30}, 0)", CellValue::Error(CellError::Value)),
            ((0, 1170, 4), "CHOOSEROWS({10; 20; 30}, 4)", CellValue::Error(CellError::Value)),
            ((0, 1170, 5), "CHOOSEROWS({10; 20; 30}, -4)", CellValue::Error(CellError::Value)),
            ((0, 1170, 6), "INDEX(CHOOSECOLS({10, 20, 30, 40}, -1), 1, 1)", CellValue::Number(40.0)),
            ((0, 1170, 7), "INDEX(CHOOSECOLS({10, 20, 30, 40}, -3), 1, 1)", CellValue::Number(20.0)),
            ((0, 1170, 8), "CHOOSECOLS({10, 20, 30}, 0)", CellValue::Error(CellError::Value)),
            ((0, 1170, 9), "CHOOSECOLS({10, 20, 30}, 5)", CellValue::Error(CellError::Value)),
            ((0, 1170, 10), "CHOOSECOLS({10, 20, 30}, -5)", CellValue::Error(CellError::Value)),
            ((0, 1170, 11), "INDEX(TAKE({10; 20; 30; 40}, -2), 1, 1)", CellValue::Number(30.0)),
            ((0, 1170, 12), "INDEX(TAKE({10; 20; 30; 40}, -2), 2, 1)", CellValue::Number(40.0)),
            ((0, 1170, 13), "ROWS(TAKE({10; 20}, 5))", CellValue::Number(2.0)),
            ((0, 1170, 14), "TAKE({10; 20}, 0)", CellValue::Error(CellError::Value)),
            ((0, 1170, 15), "INDEX(DROP({10; 20; 30; 40}, -2), 2, 1)", CellValue::Number(20.0)),
            ((0, 1170, 16), "ROWS(DROP({10; 20; 30; 40}, -2))", CellValue::Number(2.0)),
            ((0, 1170, 17), "DROP({10; 20}, 2)", CellValue::Error(CellError::Value)),
            ((0, 1170, 18), "DROP({10; 20}, -2)", CellValue::Error(CellError::Value)),
            ((0, 1170, 19), "INDEX(EXPAND({1, 2}, 2, 3, 0), 2, 3)", CellValue::Number(0.0)),
            ((0, 1170, 20), "INDEX(EXPAND({1}, 2, 2, \"X\"), 2, 2)", CellValue::Text("X".into())),
            ((0, 1170, 21), "EXPAND({1, 2; 3, 4}, 1, 2)", CellValue::Error(CellError::Value)),
            ((0, 1170, 22), "EXPAND({1, 2; 3, 4}, 2, 1)", CellValue::Error(CellError::Value)),
            ((0, 1170, 23), "INDEX(TOCOL({1, 2; 3, 4}, , TRUE), 2, 1)", CellValue::Number(3.0)),
            ((0, 1170, 24), "INDEX(TOROW({1, 2; 3, 4}, , TRUE), 1, 2)", CellValue::Number(3.0)),
            ((0, 1170, 25), "INDEX(WRAPROWS({1, 2, 3, 4}, 2), 2, 1)", CellValue::Number(3.0)),
            ((0, 1170, 26), "INDEX(WRAPROWS({1, 2, 3}, 2, 99), 2, 2)", CellValue::Number(99.0)),
            ((0, 1170, 27), "INDEX(WRAPCOLS({1, 2, 3, 4}, 2), 1, 2)", CellValue::Number(3.0)),
            ((0, 1170, 28), "INDEX(WRAPCOLS({1, 2, 3}, 2, 99), 2, 2)", CellValue::Number(99.0)),
            ((0, 1170, 29), "WRAPROWS({1, 2, 3}, 0)", CellValue::Error(CellError::Num)),
            ((0, 1170, 30), "WRAPCOLS({1, 2, 3}, -1)", CellValue::Error(CellError::Num)),
        ];

        let mut formula_cells = Vec::new();
        for (cell, formula, _) in cases {
            book.formulas.insert(*cell, formula.to_string());
            formula_cells.push(*cell);
        }

        let res = recalculate(&book, &formula_cells, &formula_cells);
        for (cell, formula, expected) in cases {
            let actual = &res.iter().find(|r| r.cell == *cell).expect(formula).value;
            match (actual, expected) {
                (CellValue::Number(a), CellValue::Number(e)) => {
                    assert!((a - e).abs() < 1e-12, "formula `{formula}` mismatch: got {a}, expected {e}");
                }
                _ => assert_eq!(actual, expected, "formula `{formula}` evaluated incorrectly"),
            }
        }
    }
}



