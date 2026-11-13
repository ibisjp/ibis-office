// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Every item of the menu bar is bound and does something (the menu audit of 2026-11-10, `tasks/doc/reports/2026-11-10-menu-audit.md`).
//!
//! The menu bar's layout names commands by id and takes each one's words and handler from the screens' own menu builders
//! (`menu_layout.rs`), so a wrong id (a line that runs another command), an id the table does not have (the line silently
//! disappears) or an item with no handler is not a compile error. Two kinds of scenario check it, for each of the three screens:
//!
//! * `menu_audit_static_*`: the layout's ids are in the command table of some state of the screen, the same id never has two
//!   different words in the old menus (the table keeps the first), the words do not read like another command's, no enabled line
//!   lacks a handler, no line says it is not made yet.
//! * `menu_every_item_is_bound_and_acts_*`: in each of the typical states of the screen (nothing selected, everything, a table,
//!   a picture, a shape, a chart...) every enabled item of the bar (its menus, the context menus of the selection and the Draw menu) is
//!   run and something must be observable afterwards: the window's pixels, the document, the clipboard, the windows. An item that
//!   does nothing in every state it is enabled in is listed in `target/menu-audit/<screen>.tsv` and fails the scenario unless it is
//!   in the screen's allow list with its reason.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};

use ibis_doc_model::FlowPos;

use super::Scenario;
use crate::docx::scenarios::DocSteps;
use crate::xlsx::scenarios::SheetSteps;
use crate::shell::chrome::{Handler, MenuEntry};
use crate::shell::menu_layout::{layout, layout_context, LayoutItem, DRAW_MENU};
use ibis_pptx::Kind;
use crate::pptx::ext::*;
use crate::docx::ext::*;
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::menu_audit_static_docx", static_docx),
        ("shell::menu_audit_static_xlsx", static_xlsx),
        ("shell::menu_audit_static_pptx", static_pptx),
        // The same audit on what is distributed (the planned lines hidden, `menu_layout::show_planned`).
        ("shell::menu_audit_static_hidden_docx", || crate::shell::menu_layout::with_planned(false, static_docx)),
        ("shell::menu_audit_static_hidden_xlsx", || crate::shell::menu_layout::with_planned(false, static_xlsx)),
        ("shell::menu_audit_static_hidden_pptx", || crate::shell::menu_layout::with_planned(false, static_pptx)),
        ("shell::menu_every_item_is_bound_and_acts_docx", acts_docx),
        ("shell::menu_every_item_is_bound_and_acts_xlsx", acts_xlsx),
        ("shell::menu_every_item_is_bound_and_acts_pptx", acts_pptx),
        ("shell::menu_states_enable_hard_items_pptx", states_enable_hard_items_pptx),
        ("shell::menu_states_of_the_made_files", states_of_the_made_files),
        ("shell::menu_suspect_docx_select_cell", suspect_docx_select_cell),
        ("shell::menu_suspect_pptx_shadow", suspect_pptx_shadow),
        ("shell::menu_suspect_pptx_palette_selected", suspect_pptx_palette_selected),
        ("shell::menu_suspect_pptx_table_borders", suspect_pptx_table_borders),
    ]
}

const DOCX: &str = "test/editing/docx-report.docx";
const XLSX: &str = "test/editing/xlsx-compatibility-v1.xlsx";
const PPTX: &str = "test/editing/pptx-plan.pptx";

/// A 1 x 1 PNG (a picture to put in a document).
const PNG: [u8; 69] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xff, 0xff, 0x3f, 0x00, 0x05, 0xfe, 0x02, 0xfe, 0xa7, 0x35, 0x81, 0x84, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

// ---- The lines of the bar ----

/// One line of the bar, with where it is.
#[derive(Clone)]
struct Row {
    path: String,
    id: &'static str,
    label: String,
    enabled: bool,
    checked: bool,
    has_handler: bool,
    hint: String,
    /// The group of lines (between separators) of its menu: lines with the same words in one group cannot be told apart.
    group: usize,
}

fn walk(entries: &[MenuEntry], path: &str, out: &mut Vec<Row>) {
    let mut group = 0;
    for entry in entries {
        match entry {
            MenuEntry::Item(i) => out.push(Row {
                path: format!("{path} > {}", i.label),
                id: i.id,
                label: crate::shell::chrome::menu_item_display_label(i).to_string(),
                enabled: i.enabled(),
                checked: i.checked,
                has_handler: i.handler.is_some(),
                hint: i.hint.as_ref().map(|h| h.to_string()).unwrap_or_default(),
                group,
            }),
            MenuEntry::Submenu(s) => walk(&s.items, &format!("{path} > {}", s.label), out),
            MenuEntry::Tiles(items) => {
                for i in items {
                    out.push(Row { path: format!("{path} > {}", i.label), id: i.id, label: i.label.to_string(), enabled: i.enabled(), checked: i.checked, has_handler: i.handler.is_some(), hint: String::new(), group });
                }
            }
            MenuEntry::Separator => group += 1,
            MenuEntry::Palette(_) => {}
        }
    }
}

/// Every line of the bar now: its menus, the selection's context menus, the Draw menu.
fn bar_rows(s: &mut Scenario) -> Vec<Row> {
    s.with(|e, _| {
        let bar = e.menu_bar_all();
        let mut out = Vec::new();
        for (id, entries) in &bar.statics.menus {
            walk(entries, &id.label().to_string(), &mut out);
        }
        for (ctx, entries) in &bar.contexts {
            walk(entries, &crate::shell::i18n::text(ctx.key()).to_string(), &mut out);
        }
        walk(&bar.draw, "draw", &mut out);
        out
    })
}

/// The handler of the enabled line `id` of the bar now.
fn handler_of(s: &mut Scenario, id: &str) -> Option<Handler> {
    s.with(|e, _| {
        let bar = e.menu_bar_all();
        let mut found = None;
        fn find(entries: &[MenuEntry], id: &str, found: &mut Option<Handler>) {
            for entry in entries {
                match entry {
                    MenuEntry::Item(i) if i.id == id && i.enabled() && found.is_none() => *found = i.handler.clone(),
                    MenuEntry::Submenu(s) => find(&s.items, id, found),
                    MenuEntry::Tiles(items) => {
                        for i in items {
                            if i.id == id && i.enabled() && found.is_none() {
                                *found = i.handler.clone();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        for (_, entries) in &bar.statics.menus {
            find(entries, id, &mut found);
        }
        for (_, entries) in &bar.contexts {
            find(entries, id, &mut found);
        }
        find(&bar.draw, id, &mut found);
        found
    })
}

// ---- The states of each screen ----

type Setup = fn(&mut Scenario);

/// A state of a screen: its name, the file it opens (the screen's own when `None`) and how it is brought about.
type State = (&'static str, Option<&'static str>, Setup);

fn none(_: &mut Scenario) {}

fn select_all(s: &mut Scenario) {
    s.key("cmd-a");
}

fn doc_caret(s: &mut Scenario) {
    let (id, _) = s.paragraph(2);
    s.click_text(FlowPos::new(id, 0));
}

fn doc_table(s: &mut Scenario) {
    doc_caret(s);
    s.menu("docx.table.insert-2x2");
    // Text in the cell: 表のセルを選択 selects the cell's text (an empty cell has none to show).
    s.type_text("abc");
}

fn doc_picture(s: &mut Scenario) {
    doc_caret(s);
    let at = s.with(|e, _| e.text_selection().expect("a caret").1);
    s.update(|e, _, cx| e.doc_insert_picture(PNG.to_vec(), ibis_doc_model::MediaFormat::Png, cx));
    s.update(move |e, _, cx| {
        e.set_caret(at, false, cx);
        e.set_caret(FlowPos::new(at.para, at.offset + 1), true, cx);
    });
}

fn doc_text_selected(s: &mut Scenario) {
    doc_caret(s);
    s.key("shift-end");
}

fn insert_shape(s: &mut Scenario) {
    s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let frame = ibis_doc_model::EmuRect::new(1_000_000, 1_000_000, 1_500_000, 800_000);
        let slide = e.current_slide;
        e.run(vec![ibis_doc_model::Command::InsertShape { slide, index: e.slide().shapes.len(), shape: ibis_doc_model::Shape::text_box(id, frame, "x") }], cx);
        e.set_selection(vec![id]);
        cx.notify();
    });
}

fn insert_chart(s: &mut Scenario) {
    s.menu("pptx.insert-chart.column");
}


fn first_shape(s: &mut Scenario) {
    s.select_shape(0);
}

/// The id of a menu item that puts something in the document and selects it, or the state is left as it is.
fn try_menu(s: &mut Scenario, id: &str) {
    if s.menu_state(id).is_some_and(|(enabled, _)| enabled) {
        s.menu(id);
    }
}

/// A text box set in two columns: 1 段 (図形の書式 > 段組み) has something to undo here. In a shape of one column it does nothing (as in
/// PowerPoint, where the line is only checked), so the other states cannot show it acting.
fn pptx_two_columns(s: &mut Scenario) {
    insert_shape(s);
    s.menu("pptx.text.columns.2");
}

/// The same box with the columns 1 cm apart (2 columns start 0.5 cm apart): the 0.5 cm line of 段の間隔 has a change to make here, as
/// the 1 cm line has in the state above.
fn pptx_two_columns_wide_gap(s: &mut Scenario) {
    pptx_two_columns(s);
    s.menu("pptx.text.columns.spacing.1cm");
}

fn pptx_table(s: &mut Scenario) {
    s.menu("table.new.3x3");
    let last = s.with(|e, _| e.slide().shapes.len()) - 1;
    s.select_shape(last);
}

fn doc_split(s: &mut Scenario) {
    doc_caret(s);
    try_menu(s, "docx.split");
}

fn sheet_rectangle(s: &mut Scenario) {
    try_menu(s, "xlsx.shape.rect");
}

fn pivot_cell(s: &mut Scenario) {
    s.click_cell("A5");
}

fn pptx_picture(s: &mut Scenario) {
    // A slide's picture, if the deck has one.
    let found = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Picture { .. })));
    if let Some(i) = found {
        s.select_shape(i);
    }
}

/// A rectangle (an auto shape: outer shadow, glow... are for it) selected.
fn pptx_rectangle(s: &mut Scenario) {
    s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let frame = ibis_doc_model::EmuRect::new(1_000_000, 1_000_000, 1_500_000, 800_000);
        let slide = e.current_slide;
        let kind = ibis_doc_model::ShapeKind::Auto(ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, ibis_doc_model::Fill::None, None));
        e.run(vec![ibis_doc_model::Command::InsertShape { slide, index: e.slide().shapes.len(), shape: ibis_doc_model::Shape::new(id, frame, kind) }], cx);
        e.set_selection(vec![id]);
        cx.notify();
    });
}

// ---- The states of the files made by tools/testfiles (the lines that the plain files cannot enable) ----

const DOCX_STATES_FILE: &str = "test/editing/docx-menu-states.docx";
const DOCX_PROTECTED_FILE: &str = "test/editing/docx-protected-range.docx";
const XLSX_STATES_FILE: &str = "test/editing/xlsx-menu-states.xlsx";
const PPTX_STATES_FILE: &str = "test/editing/pptx-menu-states.pptx";

fn doc_caret_in(s: &mut Scenario, n: usize) {
    let (id, _) = s.paragraph(n);
    s.click_text(FlowPos::new(id, 0));
}

/// A caption (a figure's number): the caret is in it, and the cross-reference lines have something to point at.
fn doc_caption(s: &mut Scenario) {
    doc_caret_in(s, 8);
}

/// A document with a bibliography and a table of authorities that are out of date: the bibliography was made from one citation and a
/// second one was inserted after it; the table from one marked citation and a second was marked after it. The update lines of the
/// 参考資料 tab (文献目録の更新, 引用文献目録の更新) then have something to make again (a table that is already current does not change).
fn doc_references(s: &mut Scenario) {
    // A citation (the default tag) at the start of the third paragraph, a citation marked in the fourth.
    doc_caret_in(s, 2);
    s.menu("docx.reference.insert-citation");
    doc_caret_in(s, 3);
    s.key("shift-end");
    s.menu("docx.toa.mark-category-1");
    // The bibliography before the seventh paragraph, the table of authorities before the fifth.
    doc_caret_in(s, 6);
    s.menu("docx.reference.bibliography");
    doc_caret_in(s, 4);
    s.menu("docx.toa");
    // Now one more of each, so that both tables are out of date.
    doc_caret_in(s, 2);
    s.menu("docx.reference.insert-citation");
    doc_caret_in(s, 3);
    s.key("shift-end");
    s.menu("docx.toa.mark-category-2");
    doc_caret_in(s, 2);
}

/// The paragraph of the second section, in two columns with a line between them.
fn doc_columns(s: &mut Scenario) {
    doc_caret_in(s, 18);
}

/// The paragraph of the line-numbered first section.
fn doc_numbered(s: &mut Scenario) {
    doc_caret_in(s, 2);
}

/// The paragraph of a document whose editing is restricted (a range may be edited).
fn doc_protected(s: &mut Scenario) {
    doc_caret_in(s, 2);
}

fn sheet_commented(s: &mut Scenario) {
    s.click_cell("C2");
}

fn sheet_table_cell(s: &mut Scenario) {
    s.click_cell("B4");
}

/// The group of two shapes.
fn sheet_group(s: &mut Scenario) {
    s.select_shape(0);
}

fn slide_shape(s: &mut Scenario, slide: usize, shape: usize) {
    let id = s.with(|e, _| e.presentation().slides[slide].id);
    s.update(move |e, w, cx| e.select_slide(id, w, cx));
    s.select_shape(shape);
}

/// The chart that appears on a click.
fn pptx_animated_chart(s: &mut Scenario) {
    slide_shape(s, 0, 2);
}

/// The list whose paragraphs come one by one.
fn pptx_animated_list(s: &mut Scenario) {
    slide_shape(s, 0, 1);
}

/// The button that starts an effect of another shape (a trigger).
fn pptx_trigger_button(s: &mut Scenario) {
    slide_shape(s, 0, 3);
}

/// The group of two shapes, on the third slide (in the second section).
fn pptx_group_in_section(s: &mut Scenario) {
    slide_shape(s, 2, 2);
}

/// Two rectangles, both selected (the lines that join and group shapes need more than one).
fn pptx_two_rectangles(s: &mut Scenario) {
    pptx_rectangle(s);
    let first = s.with(|e, _| e.selected_ids());
    s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let frame = ibis_doc_model::EmuRect::new(1_800_000, 1_200_000, 1_500_000, 800_000);
        let slide = e.current_slide;
        let kind = ibis_doc_model::ShapeKind::Auto(ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, ibis_doc_model::Fill::None, None));
        e.run(vec![ibis_doc_model::Command::InsertShape { slide, index: e.slide().shapes.len(), shape: ibis_doc_model::Shape::new(id, frame, kind) }], cx);
        let mut ids: Vec<_> = first.iter().copied().collect();
        ids.push(id);
        e.set_selection(ids);
        cx.notify();
    });
}

/// The two rectangles grouped (the group is selected: the line that takes a group apart is on).
fn pptx_group(s: &mut Scenario) {
    pptx_two_rectangles(s);
    try_menu(s, "pptx.group");
}

/// A table with the cells of its top left corner chosen (a cell range, as Shift and a press in another cell make).
fn pptx_table_range(s: &mut Scenario) {
    pptx_table(s);
    // A new table has every border already; with none, each border item has something to set.
    s.menu("table.borders.none");
    s.update(|e, w, cx| {
        let shape = *e.selected_ids().first().expect("the table");
        e.begin_table_cell_edit(shape, (0, 0), w, cx);
        e.shape_edit.edit.as_mut().unwrap().cell_anchor = Some((1, 1));
    });
}

/// Two slides chosen in the list (the first two).
fn pptx_two_slides(s: &mut Scenario) {
    let ids: Vec<_> = s.with(|e, _| e.presentation().slides.iter().map(|x| x.id).collect());
    s.update(move |e, w, cx| {
        e.select_slide(ids[0], w, cx);
        e.click_slide(ids[1], &gpui_kit::Modifiers { shift: true, ..Default::default() }, w, cx);
    });
}

fn pptx_all_slides(s: &mut Scenario) {
    s.key("cmd-a");
}

/// Every shape of the slide selected (a merge, a group, an alignment need several).
fn select_every_shape(s: &mut Scenario) {
    s.update(|e, _, cx| {
        let ids: Vec<_> = e.slide().shapes.iter().map(|sh| sh.id).collect();
        e.set_selection(ids);
        cx.notify();
    });
}

/// The format of the selection copied (書式の貼り付け has something to paste).
fn format_copied_doc(s: &mut Scenario) {
    doc_text_selected(s);
    try_menu(s, "edit.copy-format");
}

fn format_copied_sheet(s: &mut Scenario) {
    try_menu(s, "edit.copy-format");
}

fn format_copied_slide(s: &mut Scenario) {
    s.select_shape(0);
    try_menu(s, "edit.copy-format");
}

/// Everything copied, and the caret moved (so 貼り付け has something to paste).
fn copied(s: &mut Scenario) {
    s.key("cmd-a");
    s.key("cmd-c");
    s.key("right");
}

/// A change made and undone (so that やり直し has something to do).
fn undone_doc(s: &mut Scenario) {
    doc_caret(s);
    s.type_text("x");
    s.key("cmd-z");
}

fn undone_sheet(s: &mut Scenario) {
    s.type_text("1");
    s.key("enter");
    s.key("cmd-z");
}

fn undone_slide(s: &mut Scenario) {
    s.select_shape(0);
    s.key("delete");
    s.key("cmd-z");
}

/// A shape with an animation on it, selected.
fn animated(s: &mut Scenario) {
    s.select_shape(0);
    try_menu(s, "add.pptx.anim.in.fade");
    s.select_shape(0);
}

/// A second sheet, hidden.
fn hidden_sheet(s: &mut Scenario) {
    try_menu(s, "xlsx.insert-sheet");
    try_menu(s, "xlsx.hide-sheet");
}

fn sheet_cell(s: &mut Scenario) {
    s.key("right");
    s.key("down");
}

fn sheet_row(s: &mut Scenario) {
    s.key("shift-space");
}

fn sheet_column(s: &mut Scenario) {
    s.key("ctrl-space");
}

// ---- The fingerprint of what a command may change ----

fn hash_of(x: &impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    x.hash(&mut h);
    h.finish()
}

/// Everything a menu item may change that a scenario can see: the window's pixels (a dialog, a panel, the selection, the zoom, the
/// status bar, a toast), the document, the status texts, the clipboard, the windows.
#[derive(PartialEq, Eq, Debug)]
struct Fingerprint {
    pixels: u64,
    state: u64,
    windows: usize,
}

fn fingerprint(s: &mut Scenario) -> Fingerprint {
    let windows = s.window_count();
    // The item closed the window (ファイル > 閉じる): nothing to draw, and the count of windows says it.
    if windows == 0 {
        return Fingerprint { pixels: 0, state: 0, windows };
    }
    let (w, px) = s.picture();
    let pixels = hash_of(&(w, px));
    // The lines of the bar: which are grey and which are checked (a toggle, a copied format that makes 書式の貼り付け available).
    let lines: Vec<(&'static str, bool, bool)> = bar_rows(s).into_iter().map(|r| (r.id, r.enabled, r.checked)).collect();
    let state = s.with(|e, app| {
        let clipboard = app.read_from_clipboard().map(|c| format!("{c:?}"));
        hash_of(&(
            (lines, format!("{:?}", e.tool)),
            // A drawing tool the item armed (挿入 ▸ 図形 of a document: drag on the page to draw).
            e.doc_state().map(|s| format!("{:?}", (s.draw_shape, s.draw_table_mode))),
            format!("{:?}", e.presentation()),
            (e.is_dirty(), e.status.as_ref().map(|t| t.to_string()), e.menu_notice.as_ref().map(|t| t.to_string()), e.save_note.as_ref().map(|t| t.to_string())),
            e.status_texts().into_iter().map(|t| t.to_string()).collect::<Vec<_>>(),
            (e.open_menu.is_some(), e.sidebar_open, e.zoom.map(|z| z.to_bits())),
            clipboard,
        ))
    });
    Fingerprint { pixels, state, windows }
}

// ---- The scenario of a screen ----

/// What running every enabled item found, by id.
#[derive(Default)]
struct Outcome {
    label: String,
    path: String,
    /// The states in which the item was enabled and ran.
    ran: Vec<&'static str>,
    /// The state in which it did something.
    acted: Option<&'static str>,
    /// What changed: `pixels` (the window), `state` (the document, the texts, the clipboard), `windows`.
    changed: String,
}

struct Screen {
    name: &'static str,
    file: &'static str,
    states: &'static [State],
    /// Items that are enabled and do nothing observable by design (or that only an operating system's dialog answers), with the reason.
    kind: Kind,
}

fn open_state(screen: &Screen, state: &State) -> Scenario {
    let (_, file, setup) = *state;
    let mut s = Scenario::open(file.unwrap_or(screen.file));
    s.size(1400., 900.);
    setup(&mut s);
    s.settle();
    s
}

/// The fingerprint of the window once it has stopped changing by itself: it is drawn again (with nothing run) until two pictures in a
/// row are the same, so what changes after an item is the item's and not the window settling (the caret, the selection handles, a
/// layout that takes a second frame). A window that never settles (a running timer) fails the scenario, as every item would "act".
fn steady(s: &mut Scenario, what: &str) -> Fingerprint {
    steady_or(s, what, true)
}

/// `steady`, or the last picture when the window keeps changing and `strict` is not set (after an item: a window that goes on changing
/// by itself, as a document laying itself out again does, is a change the item made).
fn steady_or(s: &mut Scenario, what: &str, strict: bool) -> Fingerprint {
    let mut fp = fingerprint(s);
    for _ in 0..8 {
        if fp.windows == 0 {
            return fp;
        }
        s.update(|_, _, _| {});
        let next = fingerprint(s);
        if next == fp {
            return fp;
        }
        fp = next;
    }
    assert!(!strict, "{what}: the window keeps changing by itself");
    fp
}

/// Close what the item opened. `true` when the state is back as it was.
fn restore(s: &mut Scenario, baseline: &Fingerprint) -> bool {
    if s.window_count() == 0 {
        return false;
    }
    for _ in 0..3 {
        s.key("escape");
    }
    s.update(|e, _, cx| e.close_menu(cx));
    s.settle();
    &steady_or(s, "restore", false) == baseline
}

fn run_screen(screen: &Screen) {
    let mut outcomes: BTreeMap<&'static str, Outcome> = BTreeMap::new();
    let mut disabled_everywhere: BTreeMap<&'static str, (String, String, String)> = BTreeMap::new();
    let mut no_handler: BTreeSet<String> = BTreeSet::new();
    for state in screen.states {
        let state_name = state.0;
        let mut s = open_state(screen, state);
        let baseline = steady(&mut s, &format!("{}: the state {state_name}", screen.name));
        let rows = bar_rows(&mut s);
        let mut seen = HashSet::new();
        let mut ids: Vec<Row> = Vec::new();
        for row in rows {
            if !row.has_handler {
                no_handler.insert(format!("{}\t{}\t{}", row.id, row.path, row.hint));
            }
            if !row.enabled {
                disabled_everywhere.entry(row.id).or_insert_with(|| (row.label.clone(), row.path.clone(), row.hint.clone()));
                continue;
            }
            if seen.insert(row.id) {
                ids.push(row);
            }
        }
        let mut baseline = baseline;
        for row in ids {
            // Done in an earlier state.
            if outcomes.get(row.id).is_some_and(|o| o.acted.is_some()) {
                continue;
            }
            // Shows a file in the operating system's file manager, which the headless platform has none (it panics).
            if row.id == "help.update-log" {
                continue;
            }
            let Some(handler) = handler_of(&mut s, row.id) else { continue };
            eprintln!("[{}:{}] {}", screen.name, state_name, row.id);
            s.update(|e, w, cx| handler(e, w, cx));
            let after = steady_or(&mut s, "", false);
            let acted = after != baseline;
            let o = outcomes.entry(row.id).or_default();
            o.label = row.label.clone();
            o.path = row.path.clone();
            o.ran.push(state_name);
            if acted {
                o.acted = Some(state_name);
                o.changed = [(after.pixels != baseline.pixels, "pixels"), (after.state != baseline.state, "state"), (after.windows != baseline.windows, "windows")]
                    .iter().filter(|(c, _)| *c).map(|(_, n)| *n).collect::<Vec<_>>().join("+");
            }
            if !restore(&mut s, &baseline) {
                // The item changed the document or left a window: a fresh copy of the state.
                drop(s);
                s = open_state(screen, state);
                baseline = steady(&mut s, "a fresh state");
            }
        }
    }
    // The items that never ran (disabled in every state) and the ones that did nothing.
    let ran: HashSet<&str> = outcomes.keys().copied().collect();
    disabled_everywhere.retain(|id, _| !ran.contains(id));
    let idle: Vec<_> = outcomes.iter().filter(|(id, o)| o.acted.is_none() && !IDLE_OK.iter().any(|(prefix, why)| id.starts_with(prefix) && !why.is_empty())).collect();
    let mut report = String::from("id\tstatus\tlabel\tpath\tstates\n");
    for (id, o) in &outcomes {
        report.push_str(&format!("{id}\t{}\t{}\t{}\t{}\t{}\n", if o.acted.is_some() { "acts" } else { "NO EFFECT" }, o.label, o.path, o.ran.join(","), o.changed));
    }
    for (id, (label, path, hint)) in &disabled_everywhere {
        report.push_str(&format!("{id}\tnever enabled\t{label}\t{path}\t{hint}\n"));
    }
    for line in &no_handler {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        report.push_str(&format!("{}\tNO HANDLER\t\t{}\t{}\t\n", parts[0], parts[1], parts[2]));
    }
    let dir = crate::shell::scenario::repo_root().join("target/menu-audit");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join(format!("{}.tsv", screen.name)), &report).ok();
    let idle_ids: Vec<String> = idle.iter().map(|(id, o)| format!("{id} ({})", o.path)).collect();
    assert!(idle_ids.is_empty(), "{}: enabled items that did nothing in every state they were enabled in:\n{}", screen.name, idle_ids.join("\n"));
    let _ = screen.kind;
}

// ---- The screens ----

const DOCX_STATES: &[State] = &[
    ("nothing", None, none),
    ("caret", None, doc_caret),
    ("text selected", None, doc_text_selected),
    ("all", None, select_all),
    ("table", None, doc_table),
    ("references out of date", None, doc_references),
    ("picture", None, doc_picture),
    ("split", None, doc_split),
    ("copied", None, copied),
    ("format copied", None, format_copied_doc),
    ("after undo", None, undone_doc),
    ("numbered lines", Some(DOCX_STATES_FILE), doc_numbered),
    ("caption", Some(DOCX_STATES_FILE), doc_caption),
    ("columns", Some(DOCX_STATES_FILE), doc_columns),
    ("protected", Some(DOCX_PROTECTED_FILE), doc_protected),
];

const XLSX_STATES: &[State] = &[
    ("cell", None, none),
    ("moved", None, sheet_cell),
    ("all", None, select_all),
    ("row", None, sheet_row),
    ("column", None, sheet_column),
    ("shape", None, insert_shape),
    ("chart", None, insert_chart),
    ("rectangle", None, sheet_rectangle),
    ("pivot", Some("test/editing/xlsx-pivot-not-refreshed.xlsx"), pivot_cell),
    ("hidden sheet", None, hidden_sheet),
    ("copied", None, copied),
    ("format copied", None, format_copied_sheet),
    ("after undo", None, undone_sheet),
    ("commented cell", Some(XLSX_STATES_FILE), sheet_commented),
    ("table cell", Some(XLSX_STATES_FILE), sheet_table_cell),
    ("grouped shapes", Some(XLSX_STATES_FILE), sheet_group),
];

const PPTX_STATES: &[State] = &[
    ("nothing", None, none),
    ("shape", None, first_shape),
    ("all", None, pptx_all_slides),
    ("text box", None, insert_shape),
    ("two columns", None, pptx_two_columns),
    ("two columns, wide gap", None, pptx_two_columns_wide_gap),
    ("chart", None, insert_chart),
    ("table", None, pptx_table),
    ("rectangle", None, pptx_rectangle),
    ("table cells", None, pptx_table_range),
    ("two slides", None, pptx_two_slides),
    ("picture", Some("test/editing/pptx-picture-color-effects.pptx"), pptx_picture),
    ("every shape", None, select_every_shape),
    ("animated", None, animated),
    ("two rectangles", None, pptx_two_rectangles),
    ("group", None, pptx_group),
    ("animated chart", Some(PPTX_STATES_FILE), pptx_animated_chart),
    ("animated list", Some(PPTX_STATES_FILE), pptx_animated_list),
    ("trigger button", Some(PPTX_STATES_FILE), pptx_trigger_button),
    ("group in a section", Some(PPTX_STATES_FILE), pptx_group_in_section),
    ("smartart deck", Some("test/editing/pptx-smartart.pptx"), select_every_shape),
    ("copied", None, copied),
    ("format copied", None, format_copied_slide),
    ("after undo", None, undone_slide),
];

/// Enabled items that do nothing the scenario can see, each with the reason (an id, or the start of one). An item that does something in
/// any state is not here. A reason that starts with suspect is not decided: the item may be a bug (listed in the audit report for the user).
const IDLE_OK: &[(&str, &str)] = &[
    // The answer is the operating system's dialog or a file on the disk (the headless window shows neither).
    ("file.copy", "native save dialog"),
    ("file.open", "native open dialog"),
    ("file.save-as", "native save dialog"),
    ("file.export.", "native save dialog (PDF, PNG, GIF, SVG)"),
    ("docx.file.export.", "native save dialog"),
    ("xlsx.export-", "native save dialog"),
    ("pptx.export-", "native save dialog"),
    ("docx.new-from-template", "opens the template picker window"),
    ("xlsx.expand", "the sample's outline groups are shown already"),
    ("xlsx.validation.circle-invalid", "the sample has no invalid data to circle"),
    ("xlsx.validation.clear-circles", "no circles are drawn in the sample"),
    ("docx.combine", "native open dialog (the file to combine)"),
    ("docx.compare", "native open dialog (the file to compare)"),
    ("docx.merge.export-pdf", "native save dialog"),
    ("docx.page.margins-normal", "the document's margins are the normal ones already"),
    ("docx.page.portrait", "the document is upright already"),
    ("file.recent.item", "opens the file in a window of its own (the headless run shows none)"),
    ("window.open-in-", "starts the Microsoft application (not run in a test)"),
    ("xlsx.new-from-template", "opens the template picker window"),
    ("pptx.new-from-template", "opens the template picker window"),
    ("image.file", "native open dialog"),
    ("docx.image.file", "native open dialog"),
    ("docx.picture.replace", "native open dialog"),
    ("insert.audio", "native open dialog"),
    ("insert.video", "native open dialog"),
    ("xlsx.3d-model", "native open dialog"),
    ("pptx.3d-model", "native open dialog"),
    ("xlsx.insert-picture-", "native open dialog"),
    ("xlsx.screenshot.", "reads the system clipboard's screenshot (none)"),
    ("pptx.insert.screenshot.", "reads the system clipboard's screenshot (none)"),
    ("pptx.theme.save-theme", "native save dialog"),
    ("table.borders.1pt", "the line weight of a new table's borders is 1 pt already"),
    ("help.check-update", "a network request (the answer comes later, the headless run shows nothing)"),
    ("pptx.review.compare", "native open dialog"),
    ("pptx.insert-object", "native open dialog"),
    ("view.fullscreen", "the platform window's full screen (the headless window has none)"),
    // The window list's line of the window that is already in front, and what needs a second window.
    ("window.open.", "chooses a window that is already in front"),
    ("docx.view-side-by-side", "needs a second window"),
    ("xlsx.view-side-by-side", "needs a second window"),
    // The state already has the value the item sets.
    ("docx.cell.direction.lrtb", "already horizontal"),
    ("docx.cell.margins.normal", "already the normal margins"),
    ("docx.columns.1", "already one column"),
    ("docx.interaction.editing", "already in editing mode"),
    ("docx.line-numbers.none", "no line numbers"),
    ("docx.options.paste-default.keep-source", "already the default"),
    ("docx.page-movement.vertical", "already vertical"),
    ("docx.page.borders.", "no page borders; from the edge of the paper is the default"),
    ("docx.page.direction.horizontal", "already horizontal"),
    ("view.zoom-100", "already 100%"),
    ("view.zoom-fit", "already fitted"),
    ("xlsx.normal-view", "already the normal view"),
    ("xlsx.paper-a4", "already A4"),
    ("xlsx.scale-", "already 100% / automatic"),
    ("xlsx.shape-effect.shadow-none", "no shadow yet"),
    ("pptx.normal-view", "already the normal view"),
    ("pptx.slide-size.", "already wide and landscape"),
    ("pptx.shadow.none", "no shadow yet"),
    ("pptx.text.warp.none", "no warp yet"),
    ("pptx.picture.artistic.none", "no effect yet"),
    ("pptx.wrap.none", "no wrapping yet"),
    ("pptx.anim.after.hold", "the default"),
    ("pptx.anim.build.whole", "the default"),
    ("pptx.anim.delay.0", "the default"),
    ("pptx.anim.duration.500", "the default"),
    ("pptx.anim.repeat.none", "the default"),
    ("pptx.anim.smooth.none", "the default"),
    ("pptx.anim.start.click", "the default"),
    ("pptx.reset-placement", "no placeholder was moved"),
    ("pptx.order.forward", "one object only"),
    ("xlsx.order-forward", "one object only"),
    ("xlsx.order-front", "one object only"),
    ("xlsx.move-sheet-first", "one sheet only"),
    ("xlsx.move-sheet-left", "one sheet only"),
    ("table.distribute-", "the new table's rows and columns are equal already"),
    ("sidebar.pivot-fields", "the fields pane is already open when a pivot table cell is selected"),
    // The sample has nothing for the item to act on.
    ("docx.comments.", "no comments in the sample"),
    ("docx.fields.", "no fields in the sample"),
    ("docx.footnote.previous", "no footnotes in the sample"),
    ("docx.form.", "no content controls in the sample"),
    ("docx.notes.show", "no footnotes or endnotes in the sample"),
    ("docx.toc.update-", "no table of contents in the sample"),
    ("docx.track.", "no tracked changes in the sample"),
    ("docx.find.", "no search text was typed"),
    ("docx.merge.match-fields", "no data source for the mail merge"),
    ("edit.paste-format", "no format was copied (the format-copied state runs it)"),
    ("xlsx.clear-comments", "no comments in the sample"),
    ("xlsx.collapse", "no outline groups in the sample"),
    ("xlsx.remove-", "no trace arrows"),
    ("xlsx.trace-", "the cell has no references"),
    ("pptx.replace-one", "no search text was typed"),
];

fn screen_of(name: &str) -> Screen {
    match name {
        "docx" => Screen { name: "docx", file: DOCX, states: DOCX_STATES, kind: Kind::Document },
        "xlsx" => Screen { name: "xlsx", file: XLSX, states: XLSX_STATES, kind: Kind::Workbook },
        _ => Screen { name: "pptx", file: PPTX, states: PPTX_STATES, kind: Kind::Presentation },
    }
}

fn acts_docx() {
    run_screen(&screen_of("docx"));
}

fn acts_xlsx() {
    run_screen(&screen_of("xlsx"));
}

fn acts_pptx() {
    run_screen(&screen_of("pptx"));
}

// ---- The static audit ----

fn layout_ids(items: &[LayoutItem], out: &mut Vec<&'static str>) {
    for item in items {
        match item {
            LayoutItem::Cmd(id) | LayoutItem::CmdAs { id, .. } => out.push(id),
            LayoutItem::Sub { id, items, .. } => {
                if let Some(id) = id {
                    out.push(id);
                }
                layout_ids(items, out);
            }
            LayoutItem::Sep | LayoutItem::Dynamic(_) | LayoutItem::Planned { .. } | LayoutItem::CmdOrPlanned { .. } => {}
        }
    }
}

/// The words the layout gives a command besides its own, by id: a `CmdAs` line shows the command under the words of its `label_key`
/// in that menu (the review menu's "new comment" line and the insert menu's "comment" line both run `insert.comment`).
fn layout_aliases(items: &[LayoutItem], out: &mut HashMap<&'static str, BTreeSet<String>>) {
    for item in items {
        match item {
            LayoutItem::CmdAs { id, label_key } => {
                out.entry(*id).or_default().insert(crate::shell::i18n::text(label_key).to_string());
            }
            LayoutItem::Sub { items, .. } => layout_aliases(items, out),
            LayoutItem::Cmd(_) | LayoutItem::Sep | LayoutItem::Dynamic(_) | LayoutItem::Planned { .. } | LayoutItem::CmdOrPlanned { .. } => {}
        }
    }
}

/// Ids of the old menus' items (`menus_unpruned`) with the words each had.
fn old_labels(entries: &[MenuEntry], out: &mut HashMap<&'static str, Vec<String>>) {
    for entry in entries {
        match entry {
            MenuEntry::Item(i) => out.entry(i.id).or_default().push(i.label.to_string()),
            MenuEntry::Submenu(s) => {
                out.entry(s.id).or_default().push(s.label.to_string());
                old_labels(&s.items, out);
            }
            _ => {}
        }
    }
}

/// Problems of the static audit that are known and decided (a problem line starting with the text): the reason is the decision.
const STATIC_OK: &[(&str, &str)] = &[
    ("draw.", "the Draw menu's ink lines are greyed with not-ready as their reason (the ink tools are not made yet; the Draw menu of redesign stage 5 says so on purpose): listed in tasks/doc/reports/2026-11-10-menu-audit.md for the user to decide"),
    ("slideshow.custom-shows", "the submenu exists only when the deck has custom shows (`shell/chrome.rs`)"),
    ("edit.paste-match", "the document's Paste Special submenu has the same line as `docx.paste-special.match-format` (the same words)"),
    ("view.slideshow", "the Slide Show menu's From Current Slide line (`slideshow.from-current`, the same action and shortcut) stands for it in the bar; `view.slideshow` stays the toolbar button and the key's command"),
];

/// The old menus' groups the redesign takes out of the menu bar on purpose (`tasks/doc/menu-redesign/moved-and-removed.md`, 「0. メニューから消えるもの」):
/// the group and everything under it stays in the toolbar, the shortcut keys and the command search.
const REMOVED_GROUPS: &[(&str, &[&str])] = &[
    ("docx", &["format.text", "format.align", "format.spacing", "format.lists", "format.borders", "format.options", "format.clear", "docx.format-brush.lock", "docx.heading-numbers"]),
    ("xlsx", &["format.text", "format.align", "format.borders", "format.clear", "edit.duplicate", "xlsx.comma-style", "xlsx.phonetic-edit", "xlsx.phonetic-toggle", "xlsx.view-code", "xlsx.xml-map-properties", "xlsx.xml-source"]),
    ("pptx", &["format.text", "format.align", "format.spacing", "format.lists", "format.borders", "format.clear", "pptx.motion", "pptx.paragraph.settings", "pptx.text.shadow"]),
];

/// Commands (by id prefix) the spec lists by id or words that no state of the audit shows in the bar, with why. Not an accident.
const UNREACHABLE_OK: &[(&str, &str)] = &[
    // The context needs an object no state of the audit has.
    ("docx.chart.type-none", "state-limited: the chart's context (no chart in the document states)"),
    ("docx.math.struct.", "state-limited: the equation's context (no equation in the document states)"),
    ("docx.section.", "state-limited: the header and footer mode's context"),
    ("docx.shape.", "state-limited: the shape's context (no drawing shape in the document states)"),
    ("pptx.chart.data-quick", "state-limited: the chart's context"),
    ("pptx.clip.", "state-limited: the video's context (no video in the sample)"),
    ("pptx.picture.convert-to-shape", "state-limited: the SmartArt's context"),
    ("xlsx.picture.", "state-limited: the picture's context (no picture in the sheet states)"),
    ("xlsx.slicer-settings", "state-limited: the slicer's context"),
    ("xlsx.table-", "state-limited: the table design context (no table in the sheet states); placed in the layout by the unreachable audit of 2026-11-10"),
    // The same words as another line of the spec (a word like 標準, 左揃え, 平均), not the command the spec means.
    ("docx.kinsoku.mode.", "same words as another line"),
    ("docx.page-number.", "the position submenu's lines: the spec's words (left, right align) are other lines"),
    ("docx.page.", "same words as other lines of the layout (vertical, letter, page setup)"),
    ("pptx.icon.", "icon names (same words as other lines)"),
    ("window.split", "the spec's split line is xlsx.split / transition.split"),
    ("insert.equation", "the spec's equation lines are other commands"),
    ("xlsx.cell-style.", "same words (normal, note) as other lines"),
    ("xlsx.status-summary-", "the status bar's own menu (not a line of the bar)"),
    ("xlsx.text-vertical.", "same words (top align ...) as other lines"),
    ("xlsx.text.warp.", "same words (circle, button) as other lines"),
    ("xlsx.pivot-clear-all", "same words as Edit > Clear > Clear All"),
];

/// The lines of the old menus themselves (not inside a submenu) that the spec leaves out (neither the id nor the words are in
/// `menu-items.tsv`), by screen. They stay in the toolbar, the shortcut keys and the command search (`moved-and-removed.md`); the list is the
/// 11/12 review of what the menus no longer show. A new line here (a command that goes from the menus) fails the audit until it is decided.
const LEFT_OUT: &[(&str, &[&str])] = &[
    ("docx", &[
        // Taken out of the design by the user (tasks/doc/menu-redesign/removed-from-design.md, 2026-11-10).
        "edit.duplicate",
        "docx.caption.equation",
        // 参考資料 ▸ 図表番号の挿入… is the caption dialog (docx.caption.insert, 2026-11-11), which chooses the label 図／表／数式.
        "docx.caption.figure",
        "docx.caption.insert",
        "docx.caption.table",
        "docx.comments.show",
        "docx.customize-toolbar",
        "docx.edit.autocorrect",
        "docx.file.convert",
        "docx.file.repair",
        "docx.format-brush.apply",
        "docx.format-brush.cancel",
        "docx.inspect-document",
        "docx.notes.to-endnotes",
        "docx.notes.to-footnotes",
        "docx.options.user-info",
        "docx.outline.collapse",
        "docx.outline.collapse-all",
        "docx.outline.demote",
        "docx.outline.expand-all",
        "docx.outline.move-down",
        "docx.outline.move-up",
        "docx.outline.promote",
        "docx.outline.to-body",
        "docx.outline.to-heading",
        "docx.picture.allow-overlap",
        "docx.picture.crop-mode",
        "docx.picture.decorative",
        "docx.picture.layout-in-cell",
        "docx.picture.lock-anchor",
        "docx.picture.remove-background",
        "docx.print-settings",
        "docx.read-aloud",
        "docx.reference.bibliography.update",
        "docx.reset-window-position",
        "docx.screen-reader",
        "docx.sync-scrolling",
        "docx.template.attach",
        "docx.textbox.edit",
        "docx.toc.from-entries",
        "docx.toc.manual",
        "docx.view.mini-toolbar",
        "edit.paste-match",
        "edit.paste-plain",
        "edit.replace-fonts",
        "file.start-window",
        "insert.repeating-section",
        "tools.accessibility",
        "window.open-in-office",
    ]),
    ("xlsx", &[
        // Taken out of the design by the user (tasks/doc/menu-redesign/removed-from-design.md, 2026-11-10).
        "file.language",
        "edit.paste-match",
        "edit.paste-plain",
        "edit.replace-fonts",
        "tools.accessibility",
        "tools.word-count",
        "window.open-in-office",
        "xlsx.arrange-all",
        "xlsx.autorecover-settings",
        "xlsx.backup-save-settings",
        "xlsx.goto-special",
        "xlsx.hide-window",
        "xlsx.insert-cell-checkbox",
        "xlsx.named-ranges",
        "xlsx.new-window",
        "xlsx.pivot-col-headers",
        "xlsx.pivot-row-headers",
        "xlsx.precision-as-displayed",
        "xlsx.protect",
        "xlsx.reset-window-position",
        "xlsx.split-text",
        "xlsx.sync-scrolling",
        "xlsx.theme-colors",
        "xlsx.unhide-window",
        "xlsx.validation.circle-invalid",
        "xlsx.validation.clear-circles",
    ]),
    ("pptx", &[
        "edit.paste-match",
        "edit.paste-plain",
        "file.page-setup",
        "pptx.audio-link",
        "pptx.audio-record",
        "pptx.clip.caption.clear",
        "pptx.edit-points.corner",
        "pptx.edit-points.smooth",
        "pptx.edit-points.straight",
        "pptx.edit-theme",
        "pptx.smartart-to-shapes",
        "pptx.smartart.add-assistant",
        "pptx.trust-center",
        "pptx.video-link",
        "tools.accessibility",
        "tools.dictionary",
        "window.open-in-office",
    ]),
];

/// Ids whose words change with the state on purpose.
const LABEL_VARIES: &[&str] = &[
    // 日時の挿入 (docx.date.0-7): each line shows the current date or time in one form (`docx/toolbar.rs` DATE_IDS), so its words
    // change when the minute (the time forms) or the day (the date forms) turns while the test runs. The words follow the clock on purpose.
    "docx.date.0",
    "docx.date.1",
    "docx.date.2",
    "docx.date.3",
    "docx.date.4",
    "docx.date.5",
    "docx.date.6",
    "docx.date.7",
    // One line for each hidden sheet, named by the sheet (`xlsx/toolbar.rs`); each runs its own `unhide_sheet(i)`.
    "xlsx.unhide-sheet",
    // One line for each shape of the slide, named by the shape (the animation menu's trigger lines).
    "pptx.anim.trigger.shape-item",
    // One line for each layout of the deck, named by the layout (the new slide's gallery): a deck made by another program names its own.
    "pptx.new-slide.layout",
    // One line for each layout of the deck, named by the layout (編集 ▸ スライド ▸ レイアウト: apply it to the slide, #84 #85 #62).
    "pptx.layout.apply",
];

/// Ids of the items of `entries` (and their submenus), with their words.
fn entry_ids(entries: &[MenuEntry], out: &mut BTreeMap<&'static str, String>) {
    for entry in entries {
        match entry {
            MenuEntry::Item(i) => {
                out.entry(i.id).or_insert_with(|| i.label.to_string());
            }
            MenuEntry::Submenu(s) => {
                out.entry(s.id).or_insert_with(|| s.label.to_string());
                entry_ids(&s.items, out);
            }
            MenuEntry::Tiles(items) => {
                for i in items {
                    out.entry(i.id).or_insert_with(|| i.label.to_string());
                }
            }
            MenuEntry::Palette(p) => {
                for i in p.none.iter().chain(p.more.iter()).chain(p.theme.iter().flatten().chain(p.standard.iter()).map(|sw| &sw.item)) {
                    out.entry(i.id).or_insert_with(|| i.label.to_string());
                }
            }
            MenuEntry::Separator => {}
        }
    }
}

/// The ids of the submenus above each item of the old menus (first seen), outermost first.
fn old_ancestors(entries: &[MenuEntry], above: &mut Vec<&'static str>, out: &mut HashMap<&'static str, Vec<&'static str>>) {
    for entry in entries {
        match entry {
            MenuEntry::Item(i) => {
                out.entry(i.id).or_insert_with(|| above.clone());
            }
            MenuEntry::Submenu(s) => {
                out.entry(s.id).or_insert_with(|| above.clone());
                above.push(s.id);
                old_ancestors(&s.items, above, out);
                above.pop();
            }
            _ => {}
        }
    }
}

/// Ids a toolbar can run: its buttons, the lines of their menus, its boxes' menus.
fn toolbar_ids(items: &[crate::shell::chrome::ToolItem], out: &mut BTreeMap<&'static str, String>) {
    use crate::shell::chrome::ToolItem;
    for item in items {
        match item {
            ToolItem::Button(b) => {
                out.entry(b.id).or_insert_with(|| b.tooltip.to_string());
                if let Some(menu) = &b.menu {
                    entry_ids(menu, out);
                }
            }
            ToolItem::Select { id, menu, tooltip, .. } => {
                out.entry(id).or_insert_with(|| tooltip.to_string());
                entry_ids(menu, out);
            }
            _ => {}
        }
    }
}

fn static_audit(screen: &Screen) {
    let mut problems: Vec<String> = Vec::new();
    // The other way: the commands of the table (the old menus' items) that no line of the bar (its menus, its context menus, the Draw menu)
    // and no toolbar button or menu of any state of the screen has: a function that went from the menus without a word.
    let mut table_cmds: BTreeMap<&'static str, String> = BTreeMap::new();
    let mut toolbar: BTreeMap<&'static str, String> = BTreeMap::new();
    let mut ancestors: HashMap<&'static str, Vec<String>> = HashMap::new();
    // The ids the layout names must be in the table of some state of the screen.
    let mut named: Vec<&'static str> = Vec::new();
    for spec in layout(screen.kind) {
        layout_ids(spec.items, &mut named);
    }
    for spec in layout_context(screen.kind) {
        layout_ids(spec.items, &mut named);
    }
    layout_ids(DRAW_MENU, &mut named);
    let mut in_table: HashSet<&'static str> = HashSet::new();
    let mut old: HashMap<&'static str, Vec<String>> = HashMap::new();
    let mut in_bar: HashSet<&'static str> = HashSet::new();
    let mut idle_labels: BTreeSet<String> = BTreeSet::new();
    let mut all_rows: HashMap<&'static str, BTreeSet<String>> = HashMap::new();
    let mut by_label: HashMap<String, BTreeSet<&'static str>> = HashMap::new();
    for state in screen.states {
        let mut s = open_state(screen, state);
        s.with(|e, _| {
            let menus = e.menus_unpruned();
            for (menu, entries) in &menus.menus {
                let mut above: Vec<&'static str> = Vec::new();
                let mut paths = HashMap::new();
                old_ancestors(entries, &mut above, &mut paths);
                for (id, chain) in paths {
                    ancestors.entry(id).or_insert_with(|| std::iter::once(menu.label().to_string()).chain(chain.into_iter().map(String::from)).collect());
                }
                old_labels(entries, &mut old);
            }
            let (_, reg) = e.menu_bar_menus_and_registry();
            for c in &reg.commands {
                table_cmds.entry(c.id).or_insert_with(|| c.label.to_string());
            }
            toolbar_ids(&e.toolbar_items(), &mut toolbar);
            // The right-click menus of the screen's own places (they are not in the bar's context menus).
            let mut popups: Vec<Vec<MenuEntry>> = Vec::new();
            match screen.kind {
                Kind::Document => popups.push(e.doc_body_popup_entries()),
                Kind::Workbook => {
                    popups.push(e.cell_menu());
                    popups.push(e.tab_menu());
                    popups.push(e.shape_menu());
                    popups.push(e.header_menu(ibis_doc_model::sheet_command::Axis::Columns));
                    popups.push(e.header_menu(ibis_doc_model::sheet_command::Axis::Rows));
                }
                Kind::Presentation => {
                    popups.push(e.slide_context_menu_entries());
                    popups.push(e.shape_context_menu_entries());
                    popups.push(e.smartart_shape_menu_entries());
                    popups.push(e.outline_context_menu_entries());
                    popups.push(crate::pptx::chrome::chart_context_menu_items(e));
                    popups.push(crate::pptx::chrome::zoom_context_menu_items(e));
                }
            }
            for entries in &popups {
                entry_ids(entries, &mut toolbar);
            }
            // The bar's own lines, submenus included (a submenu the layout builds from its own lines has its id too).
            let bar = e.menu_bar_all();
            for (_, entries) in &bar.statics.menus {
                entry_ids(entries, &mut toolbar);
            }
            for (_, entries) in &bar.contexts {
                entry_ids(entries, &mut toolbar);
            }
            entry_ids(&bar.draw, &mut toolbar);
            for id in &named {
                if reg.entry(id).is_some() {
                    in_table.insert(*id);
                }
            }
        });
        let rows = bar_rows(&mut s);
        // In one submenu, two lines with the same words and different ids, or the same id with different words.
        let mut by_place: HashMap<(String, usize, String), BTreeSet<&'static str>> = HashMap::new();
        for row in &rows {
            let (parent, _) = row.path.rsplit_once(" > ").unwrap_or(("", ""));
            by_place.entry((parent.to_string(), row.group, row.label.clone())).or_default().insert(row.id);
            all_rows.entry(row.id).or_default().insert(row.label.clone());
            by_label.entry(row.label.clone()).or_default().insert(row.id);
        }
        for ((parent, _, label), ids) in &by_place {
            if ids.len() > 1 {
                problems.push(format!("{parent}: the words 「{label}」 are on lines with different ids {ids:?}")); // i18n-ok (Japanese text under test or a name table that matches labels)
            }
        }
        for row in rows {
            in_bar.insert(row.id);
            let words = format!("{} {}", row.label, row.hint);
            if words.contains("準備中") || words.contains("未実装") || words.to_lowercase().contains("todo") { // i18n-ok (Japanese text under test or a name table that matches labels)
                idle_labels.insert(format!("{} ({}) says it is not made yet", row.id, row.path));
            }
            if row.enabled && !row.has_handler {
                problems.push(format!("{}: enabled and has no handler ({})", row.id, row.path));
            }
        }
    }
    // The lines the redesign's table (`menu-items.tsv`, from the appendix) lists, by their place and words, that no state's bar has.
    if let Ok(table) = std::fs::read_to_string(crate::shell::scenario::repo_root().join("tasks/doc/menu-redesign/menu-items.tsv")) {
        let screen_key = match screen.kind {
            Kind::Document => "doc",
            Kind::Workbook => "sheet",
            Kind::Presentation => "slide",
        };
        let norm = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace() && !"　…・（）()「」、。:：／/-.,▸>".contains(*c)).collect() }; // i18n-ok (Japanese text under test or a name table that matches labels)
        let mut missing: Vec<String> = Vec::new();
        for line in table.lines().skip(1) {
            let c: Vec<&str> = line.split('\t').collect();
            if c.len() < 6 || c[0] != screen_key || c[5] != "item" || c[1].contains("色付き") || c[2].starts_with("最近使ったファイル") { // i18n-ok (Japanese text under test or a name table that matches labels)
                continue;
            }
            let _ = norm;
            // A line with an id (a command the appendix means) that no state of the screen has in its bar.
            if !c[4].is_empty() && !in_bar.contains(c[4]) {
                missing.push(format!("{}\t{}\t{}", c[1], c[2], c[4]));
            }
        }
        let dir = crate::shell::scenario::repo_root().join("target/menu-audit");
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(dir.join(format!("{}-appendix-missing.tsv", screen.name)), missing.join("\n")).ok();
    }
    {
        let mut unreachable: Vec<String> = Vec::new();
        let dump: Vec<String> = table_cmds.iter().map(|(id, label)| format!("{id}\t{label}\t{}", if in_bar.contains(id) || toolbar.contains_key(id) { "reach" } else { "no" })).collect();
        let dir = crate::shell::scenario::repo_root().join("target/menu-audit");
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(dir.join(format!("{}-table.tsv", screen.name)), dump.join("\n")).ok();
        // What the redesign's spec (the appendix, `menu-items.tsv`) lists for the screen: ids and words. A command the spec does not list
        // (by id or by words) was left out on purpose (`moved-and-removed.md`: kept in the toolbar, the shortcut, the command search).
        let screen_key = match screen.kind {
            Kind::Document => "doc",
            Kind::Workbook => "sheet",
            Kind::Presentation => "slide",
        };
        let norm = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace() && !"　…・（）()「」、。:：／/-.,▸>".contains(*c)).collect() }; // i18n-ok (Japanese text under test or a name table that matches labels)
        let mut spec_ids: HashSet<String> = HashSet::new();
        let mut spec_words: HashSet<String> = HashSet::new();
        if let Ok(table) = std::fs::read_to_string(crate::shell::scenario::repo_root().join("tasks/doc/menu-redesign/menu-items.tsv")) {
            for line in table.lines().skip(1) {
                let c: Vec<&str> = line.split('\t').collect();
                if c.len() >= 6 && c[0] == screen_key {
                    spec_words.insert(norm(c[3]));
                    if !c[4].is_empty() {
                        spec_ids.insert(c[4].to_string());
                    }
                }
            }
        }
        let removed_groups: &[&str] = REMOVED_GROUPS.iter().find(|(k, _)| *k == screen.name).map_or(&[], |(_, g)| *g);
        let mut left_out = 0usize;
        let mut left_out_top: Vec<String> = Vec::new();
        for (id, label) in &table_cmds {
            if in_bar.contains(id) || toolbar.contains_key(id) || crate::shell::sidebars::pane_only_commands(screen.kind).contains(id) {
                continue;
            }
            let chain = ancestors.get(id).cloned().unwrap_or_default();
            // The spec leaves it out, or takes the whole group out of the bar.
            if removed_groups.iter().any(|g| id == g || chain.iter().any(|c| c == g)) {
                left_out += 1;
                continue;
            }
            if !spec_ids.contains(*id) && !spec_words.contains(&norm(label)) {
                left_out += 1;
                // A line of an old menu itself (not inside a submenu) that the spec leaves out: listed (LEFT_OUT) so that a new one is noticed.
                if chain.len() <= 1 {
                    left_out_top.push(format!("{id}\t{label}\t{}", chain.join(" > ")));
                    if !LEFT_OUT.iter().any(|(k, ids)| *k == screen.name && ids.contains(id)) {
                        problems.push(format!("{id}: an old menu's line that the spec leaves out and LEFT_OUT does not list (a function that went from the menus: decide, then list it or put it in the layout): {label}"));
                    }
                }
                continue;
            }
            if UNREACHABLE_OK.iter().any(|(prefix, _)| id.starts_with(prefix)) {
                continue;
            }
            unreachable.push(format!("{id}\t{label}\t{}", chain.join(" > ")));
        }
        eprintln!("{}: {left_out} commands the spec leaves out of the bar (by design)", screen.name);
        std::fs::write(dir.join(format!("{}-left-out-top.tsv", screen.name)), left_out_top.join("\n")).ok();
        std::fs::write(dir.join(format!("{}-unreachable.tsv", screen.name)), unreachable.join("\n")).ok();
        for line in unreachable {
            let id = line.split('\t').next().unwrap_or("");
            problems.push(format!("{id}: the spec lists it (by id or words) and no line of the bar, no toolbar button and no right-click menu of any state has it: {}", line.split('\t').nth(1).unwrap_or("")));
        }
    }
    for id in &named {
        // The windows and the sidebars are made when the menu is built; the lines under them are not named in the layout.
        // Lines only some builds register (help.register-desktop: a Linux AppImage's own menu; the update lines: not in the browser).
        const PLATFORM_ONLY: &[&str] = &["help.register-desktop"];
        if !in_table.contains(id) && !PLATFORM_ONLY.contains(&&**id) && !crate::shell::sidebars::pane_only_commands(screen.kind).contains(id) {
            problems.push(format!("{id}: the layout names it and no state's command table has it (the line never shows)"));
        }
    }
    // The same id with different words in the old menus: the table keeps the first, so the layout's line runs that one.
    for (id, labels) in &old {
        let distinct: BTreeSet<&String> = labels.iter().collect();
        if distinct.len() > 1 && named.contains(id) && !LABEL_VARIES.contains(id) {
            problems.push(format!("{id}: the old menus give it different words {distinct:?}"));
        }
    }
    // The words a `CmdAs` line of the layout gives a command are a second name on purpose, and the layout is the one place that names
    // them (the generator's `LABEL_AS`), so they are not counted here: only what is left of an id's words must be one. A wording no
    // `CmdAs` line names still fails.
    let mut aliases: HashMap<&'static str, BTreeSet<String>> = HashMap::new();
    for spec in layout(screen.kind) {
        layout_aliases(spec.items, &mut aliases);
    }
    for spec in layout_context(screen.kind) {
        layout_aliases(spec.items, &mut aliases);
    }
    layout_aliases(DRAW_MENU, &mut aliases);
    for (id, labels) in &all_rows {
        let own = labels.iter().filter(|label| !aliases.get(id).is_some_and(|words| words.contains(*label))).count();
        // A toggle names its two states (表示 / 非表示) or a document's own name (the window list); anything else is a line that runs one command under two names.
        if own > 1 && !LABEL_VARIES.contains(id) && !id.starts_with("window.") && !id.starts_with("file.recent") {
            problems.push(format!("{id}: the bar gives the id different words {labels:?}"));
        }
    }
    // For review: the same words on different ids in the whole bar (written to a file; many are right, such as a submenu and its first line).
    let mut same_words: Vec<String> = by_label.iter().filter(|(_, ids)| ids.len() > 1).map(|(l, ids)| format!("{l}\t{ids:?}")).collect();
    same_words.sort();
    let dir = crate::shell::scenario::repo_root().join("target/menu-audit");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join(format!("{}-same-words.tsv", screen.name)), same_words.join("\n")).ok();
    problems.extend(idle_labels);
    problems.retain(|p| !STATIC_OK.iter().any(|(prefix, _)| p.starts_with(prefix)));
    problems.sort();
    problems.dedup();
    let dir = crate::shell::scenario::repo_root().join("target/menu-audit");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join(format!("{}-static.txt", screen.name)), problems.join("\n")).ok();
    assert!(problems.is_empty(), "{}: {} problems:\n{}", screen.name, problems.len(), problems.join("\n"));
}

fn static_docx() {
    static_audit(&screen_of("docx"));
}

fn static_xlsx() {
    static_audit(&screen_of("xlsx"));
}

fn static_pptx() {
    static_audit(&screen_of("pptx"));
}

// ---- The four items the audit could not decide (2026-11-10) ----

/// 編集 ▸ 選択 ▸ セルを選択: the caret in a cell with text selects the cell's text.
fn suspect_docx_select_cell() {
    let mut s = Scenario::open(DOCX);
    s.size(1400., 900.);
    doc_caret(&mut s);
    s.menu("docx.table.insert-2x2");
    s.type_text("abc");
    s.settle();
    let before = s.with(|e, _| e.text_selection().expect("a caret"));
    assert_eq!(before.0, before.1, "a caret to begin with");
    s.menu("docx.table.select.cell");
    s.settle();
    let after = s.with(|e, _| e.text_selection().expect("a selection"));
    assert_ne!(after.0, after.1, "the cell is selected: {after:?}");
}

/// An auto shape (a rectangle) selected: each outer shadow sets the shape's shadow.
fn suspect_pptx_shadow() {
    let mut s = Scenario::open(PPTX);
    s.size(1400., 900.);
    s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let frame = ibis_doc_model::EmuRect::new(1_000_000, 1_000_000, 1_500_000, 800_000);
        let slide = e.current_slide;
        let kind = ibis_doc_model::ShapeKind::Auto(ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, ibis_doc_model::Fill::None, None));
        e.run(vec![ibis_doc_model::Command::InsertShape { slide, index: e.slide().shapes.len(), shape: ibis_doc_model::Shape::new(id, frame, kind) }], cx);
        e.set_selection(vec![id]);
        cx.notify();
    });
    s.settle();
    let shadow = |s: &mut Scenario| {
        s.with(|e, _| match &e.slide().shapes.last().unwrap().kind {
            ibis_doc_model::ShapeKind::Auto(a) => format!("{:?}", a.shadow),
            _ => panic!("not an auto shape"),
        })
    };
    let mut seen = vec![shadow(&mut s)];
    for id in ["pptx.shadow.right", "pptx.shadow.diagonal", "pptx.shadow.soft", "pptx.shadow.none"] {
        s.menu(id);
        s.settle();
        let now = shadow(&mut s);
        assert_ne!(&now, seen.last().unwrap(), "{id} changed nothing");
        seen.push(now);
    }
}

/// Slides chosen in the list: the palette goes to each of them and to no other. Only the third slide of the sample has shapes that take their
/// colours from the theme (the palette changes it alone and no other slide alone), so the third slide is chosen first and the second
/// is added to it with Shift: the slide shown is then the second one, and the palette must still reach the third.
fn suspect_pptx_palette_selected() {
    let mut s = Scenario::open(PPTX);
    s.size(1400., 900.);
    s.settle();
    let ids: Vec<_> = s.with(|e, _| e.presentation().slides.iter().map(|x| x.id).collect());
    let slides = |s: &mut Scenario| s.with(|e, _| e.presentation().slides.iter().map(|x| format!("{x:?}")).collect::<Vec<_>>());
    let changed_by = |s: &mut Scenario| {
        let before = slides(s);
        s.menu("pptx.theme.palette-selected.aspect");
        s.settle();
        let after = slides(s);
        (0..before.len()).filter(|k| before[*k] != after[*k]).collect::<Vec<usize>>()
    };
    // One slide chosen: that slide only.
    for i in [1, 2] {
        let id = ids[i];
        s.update(move |e, w, cx| e.select_slide(id, w, cx));
        let changed = changed_by(&mut s);
        assert_eq!(changed, if i == 2 { vec![2] } else { vec![] }, "slide {i} chosen alone");
        s.key("cmd-z");
    }
    // The third and the second: both.
    let (second, third) = (ids[1], ids[2]);
    s.update(move |e, w, cx| {
        e.select_slide(third, w, cx);
        e.click_slide(second, &gpui_kit::Modifiers { shift: true, ..Default::default() }, w, cx);
    });
    assert_eq!(s.with(|e, _| (e.current_slide, e.selected_slide_ids().len())), (second, 2));
    assert_eq!(changed_by(&mut s), vec![2], "the third slide is chosen with the second");
}

/// A table of the slide: each border item changes the cells' borders, for the whole table and for a cell range.
fn suspect_pptx_table_borders() {
    let mut s = Scenario::open(PPTX);
    s.size(1400., 900.);
    s.menu("table.new.3x3");
    let last = s.with(|e, _| e.slide().shapes.len()) - 1;
    s.select_shape(last);
    s.settle();
    let borders = |s: &mut Scenario| {
        s.with(|e, _| match &e.slide().shapes.last().unwrap().kind {
            ibis_doc_model::ShapeKind::Table(t) => t.rows.iter().map(|r| r.cells.iter().map(|c| format!("{:?}", c.borders)).collect::<Vec<_>>()).collect::<Vec<_>>(),
            _ => panic!("not a table"),
        })
    };
    let ids = ["table.borders.all", "table.borders.outside", "table.borders.inside", "table.borders.top", "table.borders.bottom", "table.borders.left", "table.borders.right"];
    // The whole table (no cell range): every border item changes the borders of a table that has none.
    for id in ids {
        s.menu("table.borders.none");
        let before = borders(&mut s);
        s.menu(id);
        s.settle();
        assert_ne!(before, borders(&mut s), "{id}: nothing changed in the whole table");
    }
    // A cell range (the two cells at the top left): the cells of the range change, the cell at the bottom right does not.
    for id in ids {
        s.menu("table.borders.none");
        s.update(|e, w, cx| {
            let shape = *e.selected_ids().first().unwrap();
            e.begin_table_cell_edit(shape, (0, 0), w, cx);
            e.shape_edit.edit.as_mut().unwrap().cell_anchor = Some((1, 1));
        });
        let before = borders(&mut s);
        s.menu(id);
        s.settle();
        let after = borders(&mut s);
        assert_ne!(before, after, "{id}: nothing changed in the cell range");
        assert_eq!(before[2][2], after[2][2], "{id}: the cell outside the range changed");
        s.update(|e, w, cx| e.end_text_edit(w, cx));
        let last = s.with(|e, _| e.slide().shapes.len()) - 1;
        s.select_shape(last);
    }
}

/// The states of the audit that make the lines enabled that the plain states cannot: two shapes (the join lines) and a group (the line that
/// takes it apart). A line that is enabled here and does something is no longer left to `IDLE_OK`.
fn states_enable_hard_items_pptx() {
    let screen = screen_of("pptx");
    for (state_name, ids) in [
        ("two rectangles", &["pptx.merge.union", "pptx.merge.combine", "pptx.merge.intersect", "pptx.merge.subtract", "pptx.merge.fragment", "pptx.group"][..]),
        ("group", &["pptx.ungroup"][..]),
    ] {
        let state = screen.states.iter().find(|(name, _, _)| *name == state_name).expect("the state");
        let mut s = open_state(&screen, state);
        for id in ids {
            let on = s.menu_state(id).is_some_and(|(enabled, _)| enabled);
            assert!(on, "{state_name}: {id} is not enabled");
        }
    }
}

/// The states of the files made by `tools/testfiles` enable lines that no plain state does: for each, the lines that are enabled in it and
/// in none of the screen's plain states are listed (`target/menu-audit/<screen>-made-files.txt`), and there must be some.
fn states_of_the_made_files() {
    for name in ["docx", "xlsx", "pptx"] {
        let screen = screen_of(name);
        let enabled_in = |state: &State| -> BTreeSet<&'static str> {
            let mut s = open_state(&screen, state);
            bar_rows(&mut s).into_iter().filter(|r| r.enabled).map(|r| r.id).collect()
        };
        let mut plain: BTreeSet<&'static str> = BTreeSet::new();
        for state in screen.states.iter().filter(|(_, file, _)| file.is_none_or(|f| !f.contains("menu-states") && !f.contains("protected-range"))) {
            plain.extend(enabled_in(state));
        }
        let mut report = String::new();
        for state in screen.states.iter().filter(|(_, file, _)| file.is_some_and(|f| f.contains("menu-states") || f.contains("protected-range"))) {
            let new: Vec<_> = enabled_in(state).into_iter().filter(|id| !plain.contains(id)).collect();
            report.push_str(&format!("{}\t{}\n", state.0, new.join(" ")));
            assert!(!new.is_empty(), "{name}: the state {:?} enables no line that the plain states do not", state.0);
        }
        let dir = crate::shell::scenario::repo_root().join("target/menu-audit");
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(dir.join(format!("{name}-made-files.txt")), report).ok();
    }
}
