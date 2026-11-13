// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The shared parts of the panels, driven through the real window: the TextField's caret placement by a click (every
//! kind of field), the panel header area with its tab control, the one-line / two-line rows (toolbar, colour, text
//! field with a unit), and the equal-width button group.
use super::{Scenario, button_probe as buttons};
use crate::shell::chrome::{
    self, ButtonGroupItem, FORCED_SIDEBAR, SidebarPanel, ToolbarItem, button_group, color_row, text_field, text_field_row,
    toolbar_row,
};
use crate::shell::dialog::DialogMode;
use crate::shell::editor::SlideEditor;
use gpui_kit::component::input::InputState;
use gpui_kit::component::v_flex;
use gpui_kit::*;
use std::{cell::{Cell, RefCell}, collections::BTreeMap, rc::Rc};
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::text_field_caret_by_click_every_kind", text_field_caret_every_kind),
        ("shell::text_field_caret_toolbar_and_xlsx_bars", text_field_caret_xlsx_bars),
        ("shell::text_field_ime_clauses_every_kind", text_field_ime_clauses),
        ("shell::panel_header_tabs_sidebar", header_tabs_sidebar),
        ("shell::panel_header_tabs_dialog", header_tabs_dialog),
        ("shell::adaptive_rows_one_or_two_lines", adaptive_rows),
        ("shell::button_group_equal_width", button_group_equal_width),
        ("shell::data_table_group_rows_drag_drop_actions", group_rows),
    ]
}

type Records = Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>;
thread_local! {
    static RECORDS: RefCell<Records> = RefCell::new(Rc::new(RefCell::new(BTreeMap::new())));
    static FIELDS: RefCell<Vec<(&'static str, Entity<InputState>)>> = const { RefCell::new(Vec::new()) };
    static TAB: Cell<usize> = const { Cell::new(0) };
    static CLICKS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        FORCED_SIDEBAR.with(|slot| slot.set(None));
        FIELDS.with(|f| f.borrow_mut().clear());
        RECORDS.with(|r| r.borrow().borrow_mut().clear());
        CLICKS.with(|c| c.borrow_mut().clear());
        TAB.with(|t| t.set(0));
    }
}
fn records() -> Records { RECORDS.with(|r| r.borrow().clone()) }
/// `child` with its laid-out bounds recorded under `key`.
fn recorded(key: &str, child: impl IntoElement) -> AnyElement {
    let (records, key) = (records(), key.to_string());
    div().relative().w_full().child(child)
        .child(canvas(|_, _, _| (), move |b, _, _, _| { records.borrow_mut().insert(key.clone(), b); }).absolute().inset_0())
        .into_any_element()
}
fn rec(key: &str) -> Bounds<Pixels> {
    records().borrow().get(key).copied().unwrap_or_else(|| panic!("no bounds for {key}: {:?}", records().borrow().keys().collect::<Vec<_>>()))
}
fn force(s: &mut Scenario, panel: fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) -> Option<SidebarPanel>, key: &'static str, mode: DialogMode) {
    FORCED_SIDEBAR.with(|slot| slot.set(Some((panel, key, mode))));
    s.update(|e, _, cx| { e.sidebar_open = true; cx.notify(); });
}

// ---------------------------------------------------------------------------------------------------------------
// 1. The caret by a click, in every kind of TextField.

/// Click in the left quarter / the right quarter of every character of `input` (whose text is on screen) and check
/// the caret goes to the boundary that is nearest to the click: before the character in its left half, after it in its
/// right half. Returns the number of clicks checked.
fn check_caret_by_click(s: &mut Scenario, input: &Entity<InputState>, name: &str, right_inset: Pixels) -> usize {
    let text = s.with(|_, cx| input.read(cx).value().to_string());
    let boundaries: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
    let mut checked = 0;
    for pair in boundaries.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let geometry = s.with(|_, cx| {
            let state = input.read(cx);
            // What is on screen: the text area's own size where it is without the scroll (the text's bounds are
            // shifted by it), less what the field draws over its right end (the clear button).
            let text = state.text_bounds()?;
            let visible = Bounds::new(text.origin - state.scroll_offset(), text.size);
            let from = state.range_to_bounds(&(a..a))?;
            let to = state.range_to_bounds(&(b..b))?;
            Some((visible, from.origin.x, to.origin.x, from.origin.y + from.size.height / 2.))
        });
        let Some((visible, xa, xb, y)) = geometry else { continue };
        let width = xb - xa;
        if width <= px(1.) { continue; }
        for (fraction, expected) in [(0.25_f32, a), (0.75, b)] {
            let x = xa + width * fraction;
            if x <= visible.left() + px(1.) || x >= visible.right() - right_inset { continue; }
            s.click((f32::from(x), f32::from(y)));
            let caret = s.with(|_, cx| input.read(cx).cursor());
            assert_eq!(caret, expected, "{name}: {text:?}, click at {fraction} of the character {:?} ({a}..{b}), x={x:?}", &text[a..b]);
            checked += 1;
        }
    }
    checked
}

fn fields_panel(_: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
    let fields = FIELDS.with(|f| f.borrow().clone());
    let mut body = v_flex().gap_2().p(px(12.));
    for (name, input) in fields {
        let field = match name {
            "unit" => text_field(&input).unit("pt", px(20.)).no_clear(),
            "unit-spinner" => text_field(&input).unit("pt", px(20.)).spinner("caret-spin", chrome::SpinnerSpec::new(1.0).range(1., 400.), cx, |_, _, _, _| {}),
            "no-clear" => text_field(&input).no_clear(),
            "full-width" => text_field(&input).full_width(),
            _ => text_field(&input),
        };
        body = body.child(text_field_row(name, field));
    }
    Some(SidebarPanel { title: "Fields".into(), body: body.into_any_element() })
}

fn text_field_caret_every_kind() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // Dropped before the scenario, so no entity is held when it is torn down.
    let _reset = Reset;
    s.size(1200., 900.);
    let samples: [(&'static str, &str); 9] = [
        ("ascii", "Hello world, 12345"),
        ("japanese", "こんにちは世界、全角の文字"), // i18n-ok（キャレット位置の試験に使う日本語の入力例）
        ("mixed", "ABCあいうDEF１２３ｱｲｳxyz"), // i18n-ok（キャレット位置の試験に使う日本語の入力例）
        ("long-scrolls", "The quick brown fox jumps over the lazy dog 日本語の長い文字列を入力しています 0123456789"), // i18n-ok（キャレット位置の試験に使う日本語の入力例）
        ("unit", "12.5"),
        ("unit-spinner", "3.25"),
        ("no-clear", "iiiiilllll MMMMM wwww"),
        ("full-width", "Wide field with 日本語 text"), // i18n-ok（キャレット位置の試験に使う日本語の入力例）
        ("narrow-glyphs", "1 l i I | ! . , : ; 'ʼ"),
    ];
    s.update(|_, w, cx| {
        for (name, text) in samples {
            let input = cx.new(|cx| InputState::new(w, cx).default_value(text));
            FIELDS.with(|f| f.borrow_mut().push((name, input)));
        }
    });
    force(&mut s, fields_panel, "caret-fields", DialogMode::Sidebar);
    s.settle();
    let fields = FIELDS.with(|f| f.borrow().clone());
    for (name, input) in &fields {
        // A long text is scrolled: put the caret at its end first (a click right of the text), then check what is visible.
        if *name == "long-scrolls" {
            let (visible, y) = s.with(|_, cx| { let t = input.read(cx).text_bounds().unwrap(); (t, t.origin.y + t.size.height / 2.) });
            s.click((f32::from(visible.right() - px(2.)), f32::from(y)));
            s.key("end");
        }
        let n = check_caret_by_click(&mut s, input, name, if matches!(*name, "no-clear" | "unit" | "unit-spinner") { px(1.) } else { px(28.) });
        assert!(n >= 6, "{name}: only {n} clicks could be checked");
    }
    s.screenshot("text-field-caret-kinds");
}

/// GitHub issue #138: the TextField (gpui-base's input) underlines the composing text as the input method says (its clauses: thick, dotted,
/// ...), as the page does (`render::ime_text`); with no clauses, one thin solid line. Every kind of TextField is gpui-base's, so each is
/// given composing text and the clauses, drawn in the real window, and the lines the input draws are read back.
fn text_field_ime_clauses() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // Dropped before the scenario, so no entity is held when it is torn down.
    let _reset = Reset;
    s.size(1200., 900.);
    s.update(|_, w, cx| {
        for name in ["plain", "unit", "no-clear", "full-width"] {
            let input = cx.new(|cx| InputState::new(w, cx).default_value("ab"));
            FIELDS.with(|f| f.borrow_mut().push((name, input)));
        }
    });
    force(&mut s, fields_panel, "ime-fields", DialogMode::Sidebar);
    s.settle();
    let fields = FIELDS.with(|f| f.borrow().clone());
    // "ここでは" being converted: the converting clause thick, the part still typed dotted, the converted one thin (what Word shows).
    let composing = "ここでは"; // i18n-ok（IME の変換中の文字列の試験データ、issue #138）
    let clauses = vec![
        ImeClause { range: 0..3, thick: true, line: ImeLine::Solid },
        ImeClause { range: 3..4, thick: false, line: ImeLine::Dotted },
    ];
    for (name, input) in &fields {
        // Nothing composed: no line.
        assert!(s.with(|_, cx| input.read(cx).ime_underlines().is_empty()), "{name}: nothing composed");
        s.update(|_, w, cx| input.update(cx, |st, cx| {
            // Composed after "ab".
            st.replace_and_mark_text_in_range(Some(2..2), composing, Some(4..4), w, cx);
        }));
        s.settle();
        // The platform said nothing yet (Linux, the web): one thin solid line over all of it.
        let n = composing.len();
        assert_eq!(s.with(|_, cx| input.read(cx).ime_underlines()), vec![(2..2 + n, false, ImeLine::Solid)], "{name}: no clauses");
        s.update(|_, w, cx| input.update(cx, |st, cx| st.set_ime_clauses(clauses.clone(), w, cx)));
        s.settle();
        assert_eq!(
            s.with(|_, cx| input.read(cx).ime_underlines()),
            vec![(2..11, true, ImeLine::Solid), (11..2 + n, false, ImeLine::Dotted)],
            "{name}: the clauses decide the lines"
        );
        // Committed: the lines go away.
        s.update(|_, w, cx| input.update(cx, |st, cx| st.replace_text_in_range(None, "\u{5b8c}\u{4e86}", w, cx)));
        s.settle();
        assert!(s.with(|_, cx| input.read(cx).ime_underlines()).is_empty(), "{name}: committed");
    }
    s.screenshot("text-field-ime-clauses");
}

fn text_field_caret_xlsx_bars() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // Dropped before the scenario, so no entity is held when it is torn down.
    let _reset = Reset;
    s.size(1400., 900.);
    s.settle();
    // The cell-name box, left of the formula bar.
    let name_box = s.with(|e, _| e.wb().jump.name_box.clone());
    s.update(|_, w, cx| name_box.update(cx, |i, cx| i.set_value("Sheet1!A1:C10", w, cx)));
    s.settle();
    // The first press on the name box selects all, as Excel's does; the caret is placed by the presses after it.
    let at = s.with(|_, cx| name_box.read(cx).text_bounds().unwrap());
    s.click((f32::from(at.origin.x + px(4.)), f32::from(at.origin.y + at.size.height / 2.)));
    let n = check_caret_by_click(&mut s, &name_box, "name box", px(28.));
    assert!(n >= 6, "name box: only {n} clicks checked");
    // The toolbar's font size field.
    let size = s.with(|e, _| e.font_size_edit_input.clone());
    // The first press on the font size box selects all too.
    let at = s.with(|_, cx| size.read(cx).text_bounds().unwrap());
    s.click((f32::from(at.origin.x + px(2.)), f32::from(at.origin.y + at.size.height / 2.)));
    let n = check_caret_by_click(&mut s, &size, "toolbar font size", px(1.));
    assert!(n >= 2, "toolbar font size: only {n} clicks checked");
    // The formula bar: typing into a cell, then clicking inside the bar's text.
    s.update(|e, _, cx| {
        e.begin_cell_edit(crate::xlsx::cell_edit::EditMode::Edit, false, cx);
        if let Some(edit) = e.cell_edit_mut() {
            edit.text = "=SUM(あいう,ABC)+1２３".to_string(); // i18n-ok（キャレット位置の試験に使う日本語の入力例）
            edit.caret = 0;
            edit.anchor = None;
        }
        cx.notify();
    });
    s.settle();
    let (rect, text) = s.with(|e, _| (e.sheet_state().unwrap().bar_bounds.get().unwrap(), e.cell_edit().unwrap().text.clone()));
    // Where each character begins, from the same font and size the bar draws with.
    let boundaries: Vec<(usize, Pixels)> = s.update(|_, window, _| {
        let style = window.text_style();
        // The bar's own font (the window's, with the family of Settings > Spreadsheet > formula bar's alphanumeric font, #149).
        let mut font = style.font();
        if let Some(family) = crate::shell::settings_dialog::formula_bar_font() { font.family = family.into(); }
        let run = TextRun { len: text.len(), font, color: style.color, background_color: None, underline: None, strikethrough: None };
        let shaped = window.text_system().shape_line(SharedString::from(text.clone()), window.rem_size() * 0.875, &[run], None);
        text.char_indices().map(|(i, _)| i).chain([text.len()]).map(|i| (i, shaped.x_for_index(i))).collect()
    });
    let y = f32::from(rect.origin.y + rect.size.height / 2.);
    let mut checked = 0;
    for pair in boundaries.windows(2) {
        let ((a, xa), (b, xb)) = (pair[0], pair[1]);
        for (fraction, expected) in [(0.25_f32, a), (0.75, b)] {
            // The text starts after the field's 8 px padding.
            let x = f32::from(rect.origin.x + px(8.) + xa + (xb - xa) * fraction);
            s.click((x, y));
            let caret = s.with(|e, _| e.cell_edit().unwrap().caret);
            let expected_units = text[..expected].encode_utf16().count();
            assert_eq!(caret, expected_units, "formula bar {text:?}: click at {fraction} of {:?}", &text[a..b]);
            checked += 1;
        }
    }
    assert!(checked >= 20);
    s.screenshot("formula-bar-caret");
}

// ---------------------------------------------------------------------------------------------------------------
// 2. The panel header area and its tab control.

fn tabbed_panel(_: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
    let tab = TAB.with(|t| t.get());
    let names = ["Format", "Border", "Protection"];
    let body = v_flex().gap_2().p(px(12.))
        .child(recorded(&format!("body-{tab}"), div().h(px(900.)).child(format!("Body of {}", names[tab]))))
        .into_any_element();
    let panel = SidebarPanel { title: "Tabbed".into(), body };
    Some(panel.header_tabs("panel-tabs", names.iter().map(|n| SharedString::from(*n)), tab, |_, i, cx| { TAB.with(|t| t.set(i)); cx.notify(); }, cx))
}
fn tab_key(index: usize) -> String {
    buttons::all().into_iter().find(|k| k.contains("panel-tabs") && k.ends_with(&format!(", {index})")))
        .unwrap_or_else(|| panic!("tab {index} not rendered: {:?}", buttons::all()))
}
fn check_header_tabs(s: &mut Scenario, scroll_at: (f32, f32)) {
    let header_before = buttons::bounds(&tab_key(0)).unwrap();
    for (i, name) in ["Format", "Border", "Protection"].iter().enumerate() {
        let key = tab_key(i);
        buttons::click(s, &key);
        assert_eq!(TAB.with(|t| t.get()), i, "tab {i} pressed");
        s.settle();
        let body = rec(&format!("body-{i}"));
        let header = buttons::bounds(&tab_key(i)).unwrap();
        assert!(header.bottom() <= body.top() + px(1.), "{name}: the header is above the body ({header:?} / {body:?})");
        assert_eq!(header.origin.y, header_before.origin.y, "the header does not move when the tab changes");
    }
    // Scrolling the body does not move the header.
    let body_before = rec("body-2");
    s.wheel(scroll_at, -120.);
    s.settle();
    let body_after = rec("body-2");
    assert!(body_after.origin.y < body_before.origin.y, "the body scrolls ({body_before:?} -> {body_after:?})");
    assert_eq!(buttons::bounds(&tab_key(2)).unwrap().origin.y, header_before.origin.y, "the header stays when the body scrolls");
}
fn header_tabs_sidebar() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // Dropped before the scenario, so no entity is held when it is torn down.
    let _reset = Reset;
    s.size(1200., 700.);
    force(&mut s, tabbed_panel, "tabbed-sidebar", DialogMode::Sidebar);
    s.settle();
    let body = rec("body-0");
    check_header_tabs(&mut s, (f32::from(body.origin.x + px(40.)), f32::from(body.origin.y + px(100.))));
    s.screenshot("panel-header-tabs-sidebar");
}
fn header_tabs_dialog() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // Dropped before the scenario, so no entity is held when it is torn down.
    let _reset = Reset;
    s.size(1200., 700.);
    force(&mut s, tabbed_panel, "tabbed-dialog", DialogMode::Modal);
    s.settle();
    let body = rec("body-0");
    check_header_tabs(&mut s, (f32::from(body.origin.x + px(40.)), f32::from(body.origin.y + px(100.))));
    let dialog = s.with(|e, _| e.dialog.bounds).expect("a dialog");
    let header = buttons::bounds(&tab_key(2)).unwrap();
    assert!(header.top() >= dialog.top() + px(40.) && header.bottom() <= dialog.bottom(), "the header is inside the dialog");
    s.screenshot("panel-header-tabs-dialog");
}

// ---------------------------------------------------------------------------------------------------------------
// 3. Rows: one line when they fit, two when they do not.

fn rows_panel(_: &mut SlideEditor, _: &mut Window, _: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
    let size = FIELDS.with(|f| f.borrow()[0].1.clone());
    let toolbar = |key: &'static str| {
        let items = || {
            ["b", "i", "u"].into_iter().zip(["format_bold", "format_italic", "format_underlined"]).enumerate().map(|(n, (id, icon))| {
                ToolbarItem::icon(SharedString::from(format!("{key}-{id}")), icon).on(n == 0).on_click(move |_, _, _| CLICKS.with(|c| c.borrow_mut().push(format!("{key}-{id}"))))
            })
        };
        toolbar_row("Font style", items())
    };
    let mut body = v_flex().gap_3().p(px(12.));
    for (key, width) in [("wide", 420.), ("narrow", 190.)] {
        let w = px(width);
        body = body.child(v_flex().w(w).gap_2().child(
            div().w(w).child(recorded(&format!("{key}-unit"), text_field_row("Size", text_field(&size).unit("pt", px(20.)).no_clear()))))
            .child(div().w(w).child(recorded(&format!("{key}-toolbar"), toolbar(if key == "wide" { "wide-tb" } else { "narrow-tb" }))))
            .child(div().w(w).child(recorded(&format!("{key}-color"), color_row("Fill color", div().h(px(24.)).w(px(124.)).bg(rgb(0xff0000))))))
            .child(recorded(&format!("{key}-segments"), chrome::SegmentControlRow::new(chrome::segment_control::Mode::Intrinsic).label("Align").available_width(w)
                .children(["Left", "Center", "Right"].into_iter().enumerate().map(|(i, l)| chrome::segment_control::SegmentItem::label((if key == "wide" { "wide-seg" } else { "narrow-seg" }, i), l, i == 0))))));
    }
    Some(SidebarPanel { title: "Rows".into(), body: body.into_any_element() })
}

fn adaptive_rows() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // Dropped before the scenario, so no entity is held when it is torn down.
    let _reset = Reset;
    s.size(1200., 900.);
    s.update(|_, w, cx| {
        let input = cx.new(|cx| InputState::new(w, cx).default_value("11"));
        FIELDS.with(|f| f.borrow_mut().push(("size", input)));
    });
    force(&mut s, rows_panel, "rows", DialogMode::Sidebar);
    s.settle();
    // For a row whose control is taller than the label, one line means the control starts where the row starts.
    for kind in ["unit", "toolbar", "color", "segments"] {
        let wide = rec(&format!("wide-{kind}"));
        let narrow = rec(&format!("narrow-{kind}"));
        assert!(narrow.size.height > wide.size.height + px(4.), "{kind}: narrower, the row takes two lines ({wide:?} / {narrow:?})");
    }
    // The wrapped control is under the label, not beside it, and nothing sticks out of the pane's width.
    let narrow_tb = buttons::bounds("narrow-tb-b").unwrap();
    let wide_tb = buttons::bounds("wide-tb-b").unwrap();
    assert!(narrow_tb.origin.x < wide_tb.origin.x, "the narrow group starts at the left edge, the wide one after the label");
    assert!(buttons::bounds("narrow-tb-u").unwrap().right() <= rec("narrow-toolbar").right() + px(0.5));
    // The unit is inside the field: the row is as wide as the same row without a unit.
    // The toolbar buttons work.
    CLICKS.with(|c| c.borrow_mut().clear());
    buttons::click(&mut s, "wide-tb-i");
    buttons::click(&mut s, "narrow-tb-u");
    assert_eq!(CLICKS.with(|c| c.borrow().clone()), vec!["wide-tb-i".to_string(), "narrow-tb-u".to_string()]);
    s.screenshot("adaptive-rows");
}

// ---------------------------------------------------------------------------------------------------------------
// 4. The equal-width button group.

fn group_panel(_: &mut SlideEditor, _: &mut Window, _: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
    let items = |prefix: &'static str| ["OK", "A much longer label", "Mid"].into_iter().enumerate()
        .map(move |(i, label)| ButtonGroupItem::new((prefix, i), label).on_click(move |_, _, _| CLICKS.with(|c| c.borrow_mut().push(format!("{prefix}-{i}")))));
    let body = v_flex().gap_3().p(px(12.))
        .child(div().w(px(560.)).child(recorded("group-wide", button_group(items("gw")))))
        .child(div().w(px(220.)).child(recorded("group-narrow", button_group(items("gn")))))
        .child(div().w(px(200.)).child(recorded("group-stacked", button_group(items("gs")).stacked())));
    Some(SidebarPanel { title: "Group".into(), body: body.into_any_element() })
}
fn group_key(prefix: &str, i: usize) -> String {
    buttons::all().into_iter().find(|k| k.contains(prefix) && k.ends_with(&format!(", {i})"))).unwrap_or_else(|| panic!("{prefix} {i}: {:?}", buttons::all()))
}
fn button_group_equal_width() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // Dropped before the scenario, so no entity is held when it is torn down.
    let _reset = Reset;
    s.size(1400., 900.);
    force(&mut s, group_panel, "group", DialogMode::Sidebar);
    s.settle();
    for prefix in ["gw", "gn", "gs"] {
        let widths: Vec<Pixels> = (0..3).map(|i| buttons::bounds(&group_key(prefix, i)).unwrap().size.width).collect();
        assert!(widths.iter().all(|w| (*w - widths[0]).abs() < px(0.5)), "{prefix}: equal widths {widths:?}");
    }
    // The width is the widest label's own (the same in every group that fits), not the pane's.
    let natural = buttons::bounds(&group_key("gw", 0)).unwrap().size.width;
    assert!((buttons::bounds(&group_key("gn", 0)).unwrap().size.width - natural).abs() < px(0.5));
    let wide = (0..3).map(|i| buttons::bounds(&group_key("gw", i)).unwrap()).collect::<Vec<_>>();
    assert!(wide[0].origin.y == wide[2].origin.y, "wide: one line");
    let narrow = (0..3).map(|i| buttons::bounds(&group_key("gn", i)).unwrap()).collect::<Vec<_>>();
    assert!(narrow[2].origin.y > narrow[0].origin.y, "narrow: the last button wraps to the next line ({narrow:?})");
    let stacked = (0..3).map(|i| buttons::bounds(&group_key("gs", i)).unwrap()).collect::<Vec<_>>();
    assert!(stacked[1].origin.y > stacked[0].origin.y && stacked[0].origin.x == stacked[1].origin.x, "stacked");
    CLICKS.with(|c| c.borrow_mut().clear());
    buttons::click(&mut s, &group_key("gn", 1));
    assert_eq!(CLICKS.with(|c| c.borrow().clone()), vec!["gn-1".to_string()]);
    s.screenshot("button-group-equal-width");
}

// ---------------------------------------------------------------------------------------------------------------
// 4. The shared table's group rows: a heading row in the same rows (folds, counts), rows that are dragged and take
// drops (between two rows, onto a group row), selection, and the buttons at the row end.

thread_local! {
    static GROUP_STATE: (crate::shell::chrome::virtual_table::TableState, crate::shell::chrome::row_dnd::RowDnd, Cell<bool>, Cell<Option<u64>>) =
        (Default::default(), Default::default(), Cell::new(false), Cell::new(None));
}

fn table_group_panel(_: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
    use crate::shell::chrome::data_table::DataTable;
    use crate::shell::chrome::virtual_table::{TableColumn, TableRow};
    let (state, dnd) = GROUP_STATE.with(|g| (g.0.clone(), g.1.clone()));
    let collapsed = GROUP_STATE.with(|g| g.2.get());
    // (id, kind): 0 group, 1 item, 2 placeholder of an empty group.
    let mut specs: Vec<(u64, u8, &'static str)> = vec![(100, 0, "Group A")];
    if !collapsed {
        specs.push((1, 1, "alpha"));
        specs.push((2, 1, "beta"));
    }
    specs.push((200, 0, "Group B"));
    specs.push((200, 2, "drop here"));
    let editor = cx.entity().downgrade();
    let count = specs.len();
    let build = move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
        range.filter_map(|i| specs.get(i).copied()).map(|(id, kind, label)| match kind {
            0 => {
                let mut row = TableRow::group(Some("filter_alt"), label).count(if id == 100 { 2 } else { 0 }).drop_into(id).selected(GROUP_STATE.with(|g| g.3.get()) == Some(id));
                if id == 100 {
                    row = row.collapsible(collapsed).on_click(|_, _, _| GROUP_STATE.with(|g| g.2.set(!g.2.get())));
                }
                row
            }
            1 => {
                let del = editor.clone();
                TableRow::new(vec![div().child(label).into_any_element()])
                    .draggable(id, label)
                    .drop_between(id)
                    .selected(GROUP_STATE.with(|g| g.3.get()) == Some(id))
                    .on_click(move |_, _, cx| { GROUP_STATE.with(|g| g.3.set(Some(id))); let _ = del.update(cx, |_, cx| cx.notify()); })
                    .action("row-delete", "delete", "Delete", move |_, _, _| CLICKS.with(|c| c.borrow_mut().push(format!("delete {id}"))))
            }
            _ => TableRow::new(vec![div().child(label).into_any_element()]).drop_into(id),
        }).collect()
    };
    let table = DataTable::virtualized("group-table", vec![TableColumn::flex("name")], vec![div().child("Name").into_any_element()], state, count, build, cx)
        .row_height(30.)
        .row_actions(1)
        .row_dnd(dnd, |_, ev, _, _| CLICKS.with(|c| c.borrow_mut().push(format!("{}->{} {:?}", ev.from, ev.target, ev.place))));
    let body = v_flex().size_full().child(div().flex_1().min_h_0().child(table)).into_any_element();
    Some(SidebarPanel { title: "Groups".into(), body }.fill())
}

fn group_rows() {
    use crate::shell::chrome::row_dnd::DropPlace;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let _reset = Reset;
    s.size(1200., 700.);
    force(&mut s, table_group_panel, "group-sidebar", DialogMode::Sidebar);
    s.settle();
    let dnd = GROUP_STATE.with(|g| g.1.clone());
    let state = GROUP_STATE.with(|g| g.0.clone());
    let mid = |i: usize| { let b = dnd.row_bounds("group-table", i).unwrap_or_else(|| panic!("row {i}")); (f32::from(b.origin.x + b.size.width / 2.), f32::from(b.origin.y + b.size.height / 2.), f32::from(b.origin.y), f32::from(b.size.height)) };
    s.screenshot("data-table-group-rows");
    assert_eq!(state.last_built(), 0..5, "a heading row is a row of the same table");
    // A group row folds when pressed: its rows go.
    let (x, y, ..) = mid(0);
    s.click((x - 40., y));
    s.settle();
    assert_eq!(state.last_built(), 0..3, "folded");
    s.click((x - 40., y));
    s.settle();
    assert_eq!(state.last_built(), 0..5, "unfolded");
    // A click selects an item row.
    let (x, y, ..) = mid(1);
    s.click((x - 40., y));
    assert_eq!(GROUP_STATE.with(|g| g.3.get()), Some(1));
    // The button at the row end acts without selecting or dragging.
    let b = dnd.row_bounds("group-table", 2).unwrap();
    s.click((f32::from(b.right()) - 14., f32::from(b.origin.y + b.size.height / 2.)));
    assert_eq!(CLICKS.with(|c| c.borrow().clone()), vec!["delete 2".to_string()]);
    assert_eq!(GROUP_STATE.with(|g| g.3.get()), Some(1), "the button did not select its row");
    CLICKS.with(|c| c.borrow_mut().clear());
    // An item dropped onto a group row (as a whole), onto the placeholder, and above/below another item.
    let from = mid(1);
    let to = mid(3);
    s.drag_through(&[(from.0 - 40., from.1), (from.0 - 40., from.1 + 8.), (to.0, to.1), (to.0, to.1 + 0.5)]);
    s.settle();
    let to = mid(4);
    let from = mid(2);
    s.drag_through(&[(from.0 - 40., from.1), (from.0 - 40., from.1 + 8.), (to.0, to.1), (to.0, to.1 + 0.5)]);
    s.settle();
    let from = mid(2);
    let to = mid(1);
    // The upper half of alpha: before it. Then the lower half: after it.
    s.drag_through(&[(from.0 - 40., from.1), (from.0 - 40., from.1 - 8.), (to.0, to.2 + 3.), (to.0, to.2 + 3.5)]);
    s.settle();
    let from = mid(2);
    let to = mid(1);
    s.drag_through(&[(from.0 - 40., from.1), (from.0 - 40., from.1 - 8.), (to.0, to.2 + to.3 - 3.), (to.0, to.2 + to.3 - 3.5)]);
    s.settle();
    assert_eq!(
        CLICKS.with(|c| c.borrow().clone()),
        vec!["1->200 Into".to_string(), "2->200 Into".to_string(), "2->1 Before".to_string(), "2->1 After".to_string()],
    );
    let _ = DropPlace::Before;
    // While a drag is in progress, the row shows where it will go; Escape gives it up.
    CLICKS.with(|c| c.borrow_mut().clear());
    let from = mid(1);
    let to = mid(3);
    s.dispatch(vec![
        PlatformInput::MouseMove(MouseMoveEvent { position: point(px(from.0 - 40.), px(from.1)), ..Default::default() }),
        PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: point(px(from.0 - 40.), px(from.1)), click_count: 1, ..Default::default() }),
        PlatformInput::MouseMove(MouseMoveEvent { position: point(px(to.0), px(to.1)), pressed_button: Some(MouseButton::Left), ..Default::default() }),
    ]);
    assert_eq!(dnd.dragging(), Some(("group-table", 1)));
    assert_eq!(dnd.hover().map(|h| (h.1, h.2)), Some((3, DropPlace::Into)));
    s.settle();
    s.screenshot("data-table-group-rows-dragging");
    s.key("escape");
    assert_eq!(dnd.dragging(), None);
    s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: point(px(to.0), px(to.1)), click_count: 1, ..Default::default() })]);
    assert!(CLICKS.with(|c| c.borrow().is_empty()), "nothing dropped after Escape");
    GROUP_STATE.with(|g| { g.2.set(false); g.3.set(None); });
}
