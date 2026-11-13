// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue #102: the name manager's list on the shared scrolling table (`DataTable::virtualized`): the heading row
//! stays, only the value rows scroll, the pane has no scrollbar of its own, only the rows in view are made, and the
//! clicks reach the rows, the pencil and the trash can. Every move here is a real pointer event.

use crate::shell::chrome::virtual_table::{TableColumn, TableState};
use crate::shell::scenario::Scenario;
use crate::ext::*;

const BOOK: &str = "test/editing/xlsx-sales.xlsx";
const ROW: f32 = 32.;

fn with_many(count: usize) -> Scenario {
    let mut s = Scenario::open(BOOK);
    // Straight into the workbook (10 000 SetName commands would each keep a copy of the book for Undo).
    s.update(|e, _, _| {
        let mut wb = e.presentation().workbook.as_deref().cloned().unwrap();
        for i in 0..count {
            let formula = if i % 5 == 0 { format!("1+{i}") } else { format!("{i}") };
            wb.names.push(ibis_doc_model::DefinedName { name: format!("N{i:05}"), sheet: None, formula, hidden: false });
        }
        e.document.ids().workbook = Some(std::sync::Arc::new(wb));
    });
    s.size(1400., 800.);
    s.update(|e, w, cx| e.open_names(w, cx));
    s.settle();
    s
}

fn table(s: &mut Scenario) -> TableState {
    s.with(|e, _| e.sheet_state().unwrap().names.as_ref().unwrap().table.clone())
}

/// The rows' viewport: left, top, right, bottom in window coordinates.
fn viewport(s: &mut Scenario) -> (f32, f32, f32, f32) {
    let t = table(s);
    let b = t.scroll.0.borrow().base_handle.bounds();
    (f32::from(b.left()), f32::from(b.top()), f32::from(b.right()), f32::from(b.bottom()))
}

fn sort_name_ascending(s: &mut Scenario) -> Option<bool> {
    s.with(|e, _| e.sheet_state().unwrap().names.as_ref().unwrap().sort.direction(crate::names_list::SortColumn::Name))
}

fn selected(s: &mut Scenario) -> Option<(String, Option<usize>)> {
    s.with(|e, _| e.sheet_state().unwrap().names.as_ref().unwrap().selected.clone())
}

fn count(s: &mut Scenario) -> usize {
    s.with(|e, _| e.presentation().workbook.as_ref().unwrap().names.iter().filter(|n| !n.name.starts_with("_xl")).count())
}

/// The heading row does not move when the rows scroll, the wheel over the rows moves only the rows, and the pane
/// has no scrollbar of its own.
pub(super) fn header_stays_rows_scroll() {
    let mut s = with_many(200);
    let (left, top, right, bottom) = viewport(&mut s);
    let t = table(&mut s);
    s.screenshot("issue102-table-top");
    // The rows are far more than fit; they fill the pane to its bottom, and the pane itself does not scroll.
    assert!(t.max_scroll() > 1000., "{}", t.max_scroll());
    assert_eq!(t.scrolled(), 0.);
    assert!(bottom > 600. && bottom <= 800., "the table fills the pane: {bottom}");
    let (pane_max, pane_offset) = s.with(|e, _| (f32::from(e.sidebar_scroll.max_offset().y), f32::from(e.sidebar_scroll.offset().y)));
    assert_eq!((pane_max, pane_offset), (0., 0.), "the pane has no scroll of its own");
    // The wheel over the heading row moves nothing.
    let heading_y = top - ROW / 2.;
    s.wheel((left + 60., heading_y), -300.);
    assert_eq!(table(&mut s).scrolled(), 0.);
    // The wheel over the rows moves the rows, and the viewport (and so the heading above it) stays.
    s.wheel((left + 60., top + 100.), -300.);
    s.settle();
    let scrolled = table(&mut s).scrolled();
    assert!(scrolled > 100., "{scrolled}");
    assert_eq!(viewport(&mut s), (left, top, right, bottom));
    assert_eq!(s.with(|e, _| f32::from(e.sidebar_scroll.offset().y)), 0.);
    s.screenshot("issue102-table-scrolled");
    // The heading is where it was: pressing 名前 there sorts (it was ascending, now descending).
    assert_eq!(sort_name_ascending(&mut s), Some(true));
    s.click((left + 4. + 28. + 20., heading_y));
    assert_eq!(sort_name_ascending(&mut s), Some(false));
    // The scrollbar of the rows is dragged to the end: the last row is in view.
    let x = right - 5.;
    s.drag((x, top + 10.), (x, bottom + 50.));
    s.settle();
    let t = table(&mut s);
    assert!((t.scrolled() - t.max_scroll()).abs() < 1., "{} {}", t.scrolled(), t.max_scroll());
    assert_eq!(t.last_built().end, 200);
    s.screenshot("issue102-table-end");
}

/// Ten thousand names: only the rows in view are made, wherever the list is scrolled.
pub(super) fn only_visible_rows_are_made() {
    let mut s = with_many(10_000);
    let (left, top, right, bottom) = viewport(&mut s);
    let t = table(&mut s);
    let visible = ((bottom - top) / ROW).ceil() as usize;
    let built = t.last_built();
    assert!(!built.is_empty() && built.len() <= visible + 4, "{built:?} for {visible} rows in view");
    assert!(t.largest_batch() <= visible + 4, "{} rows made at once while the list opened", t.largest_batch());
    assert!(t.max_scroll() > 10_000. * ROW - (bottom - top) - 2., "{}", t.max_scroll());
    // Scroll with the wheel, then jump to the end and the middle with the scrollbar: still a screenful.
    s.wheel((left + 60., top + 100.), -3000.);
    s.settle();
    let t = table(&mut s);
    assert!(t.last_built().start > 50 && t.last_built().len() <= visible + 4, "{:?}", t.last_built());
    let x = right - 5.;
    s.drag((x, top + 10.), (x, bottom + 100.));
    s.settle();
    let t = table(&mut s);
    assert_eq!(t.last_built().end, 10_000);
    assert!(t.last_built().len() <= visible + 4, "{:?}", t.last_built());
    s.screenshot("issue102-table-10000-end");
    // The names are all in the list and the keys move through it (End, then Up scrolls back into view).
    assert_eq!(count(&mut s), 10_000);
    s.update(|e, w, cx| {
        let first = e.names_shown(cx)[0].key();
        e.names_select(first, false, w, cx);
    });
    s.key("end");
    s.settle();
    let t = table(&mut s);
    assert_eq!(t.last_built().end, 10_000);
    assert!(selected(&mut s).is_some_and(|k| k.0 == "n09999"), "{:?}", selected(&mut s));
    s.key("home");
    s.settle();
    assert_eq!(table(&mut s).last_built().start, 0);
    assert!(table(&mut s).largest_batch() <= visible + 4, "the most rows made at once: {}", table(&mut s).largest_batch());
}

/// A click on a row selects it; the pencil opens the dialog for that name; the trash can deletes it.
pub(super) fn clicks_reach_rows_and_buttons() {
    let mut s = with_many(60);
    let (left, top, right, _) = viewport(&mut s);
    // Row 3 (the fourth from the top; the order is the name, ascending).
    let rows = s.with(|e, cx| e.names_shown(cx));
    let y = |i: usize| top + ROW * i as f32 + ROW / 2.;
    s.click((left + 80., y(3)));
    assert_eq!(selected(&mut s), Some(rows[3].key()));
    s.screenshot("issue102-table-selected");
    // The pencil of row 5.
    s.click((right - 4. - 60. + 14., y(5)));
    assert!(s.with(|e, _| e.sheet_state().unwrap().names.as_ref().unwrap().dialog.is_some()));
    assert_eq!(s.with(|e, _| e.sheet_state().unwrap().names.as_ref().unwrap().dialog.as_ref().unwrap().editing.clone()), Some(rows[5].key()));
    s.key("escape");
    assert!(s.with(|e, _| e.sheet_state().unwrap().names.as_ref().unwrap().dialog.is_none()));
    // The trash can of row 4: the name goes, one undo brings it back.
    let before = count(&mut s);
    s.click((right - 4. - 60. + 42., y(4)));
    assert_eq!(count(&mut s), before - 1);
    assert!(!s.with(|e, cx| e.names_shown(cx)).iter().any(|r| r.key() == rows[4].key()));
    s.key("cmd-z");
    assert_eq!(count(&mut s), before);
    // After the rows have scrolled, the click still lands on the row under the pointer.
    s.wheel((left + 60., top + 100.), -64.);
    s.settle();
    let scrolled = table(&mut s).scrolled();
    let first = (scrolled / ROW).floor() as usize;
    let row_top = top + first as f32 * ROW - scrolled;
    let target = first + 3;
    s.click((left + 80., row_top + 3. * ROW + ROW / 2.));
    assert_eq!(selected(&mut s), Some(rows[target].key()), "scrolled by {scrolled}");
}

/// The columns are dragged by the border of their headings, within their limits; the width stays.
pub(super) fn columns_are_dragged() {
    let mut s = with_many(30);
    let (left, top, _, _) = viewport(&mut s);
    let name = TableColumn::resizable("name", 60., 118., 260.);
    let value = TableColumn::resizable("value", 50., 110., 300.);
    let heading_y = top - ROW / 2.;
    assert_eq!(table(&mut s).width(&name), 118.);
    // The name column's border: 4 (padding) + 28 (the kind column) + 118 - 3.
    let border = left + 4. + 28. + 118. - 3.;
    s.drag((border, heading_y), (border + 40., heading_y));
    s.settle();
    assert_eq!(table(&mut s).width(&name), 158.);
    s.drag((border + 40., heading_y), (border - 400., heading_y));
    s.settle();
    assert_eq!(table(&mut s).width(&name), 60., "no narrower than the least");
    s.drag((left + 4. + 28. + 60. - 3., heading_y), (left + 900., heading_y));
    s.settle();
    assert_eq!(table(&mut s).width(&name), 260., "no wider than the most");
    // Another column: the pane is made wide so that the value column is there.
    s.update(|e, _, cx| {
        e.sidebar_width = 640.;
        cx.notify();
    });
    s.settle();
    let (left, top, _, _) = viewport(&mut s);
    let heading_y = top - ROW / 2.;
    let border = left + 4. + 28. + 260. + 110. - 3.;
    s.drag((border, heading_y), (border - 30., heading_y));
    s.settle();
    assert_eq!(table(&mut s).width(&value), 80.);
    assert_eq!(table(&mut s).width(&name), 260.);
    assert!(!table(&mut s).dragging());
    s.screenshot("issue102-table-columns");
}

/// Thousands of names: opening the list, sorting by each heading, searching, filtering and scrolling each take a
/// bounded time (every step runs the real window frames), and only the rows in view are ever made.
pub(super) fn thousands_of_names_are_quick() {
    use crate::clock::Instant;
    use crate::names_list::{NameKind, SortColumn};
    const LIMIT_MS: u128 = 4000;
    let start = Instant::now();
    let mut s = with_many(10_000);
    let mut steps: Vec<(&str, u128)> = vec![("open", start.elapsed().as_millis())];
    let (left, top, right, bottom) = viewport(&mut s);
    let visible = ((bottom - top) / ROW).ceil() as usize;
    let mut timed = |s: &mut Scenario, what: &'static str, f: &mut dyn FnMut(&mut Scenario)| {
        let at = Instant::now();
        f(s);
        steps.push((what, at.elapsed().as_millis()));
    };
    for (what, column) in [("sort-name", SortColumn::Name), ("sort-value", SortColumn::Value), ("sort-reference", SortColumn::Reference), ("sort-scope", SortColumn::Scope), ("sort-kind", SortColumn::Kind)] {
        timed(&mut s, what, &mut |s| {
            s.update(|e, _, cx| {
                let p = e.sheet_state_mut().unwrap().names.as_mut().unwrap();
                p.sort = p.sort.pressed(column);
                cx.notify();
            });
            s.settle();
        });
        let t = table(&mut s);
        assert!(t.last_built().len() <= visible + 4, "{what}: {:?}", t.last_built());
    }
    timed(&mut s, "search", &mut |s| {
        s.update(|e, w, cx| {
            let input = e.sheet_state().unwrap().names.as_ref().unwrap().search.clone();
            input.update(cx, |i, cx| i.set_value("N0123", w, cx));
        });
        s.settle();
    });
    assert_eq!(s.with(|e, cx| e.names_shown(cx)).len(), 10, "N01230..N01239");
    timed(&mut s, "search-clear", &mut |s| {
        s.update(|e, w, cx| {
            let input = e.sheet_state().unwrap().names.as_ref().unwrap().search.clone();
            input.update(cx, |i, cx| i.set_value("", w, cx));
        });
        s.settle();
    });
    timed(&mut s, "filter-kind", &mut |s| {
        s.update(|e, _, cx| {
            e.sheet_state_mut().unwrap().names.as_mut().unwrap().filter.kinds = vec![NameKind::Constant];
            cx.notify();
        });
        s.settle();
    });
    let constants = s.with(|e, cx| e.names_shown(cx)).len();
    assert_eq!(constants, 8000, "the constants (every fifth name is a formula)");
    timed(&mut s, "filter-clear", &mut |s| {
        s.update(|e, _, cx| {
            e.sheet_state_mut().unwrap().names.as_mut().unwrap().filter.kinds = Vec::new();
            cx.notify();
        });
        s.settle();
    });
    timed(&mut s, "scroll-50-times", &mut |s| {
        for _ in 0..50 {
            s.wheel((left + 60., top + 100.), -400.);
        }
        s.settle();
    });
    let t = table(&mut s);
    assert!(t.last_built().len() <= visible + 4 && t.largest_batch() <= visible + 4, "{:?} {}", t.last_built(), t.largest_batch());
    let _ = right;
    for (what, ms) in &steps {
        assert!(*ms < LIMIT_MS, "{what} took {ms} ms");
    }
}
