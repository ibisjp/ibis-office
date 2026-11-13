// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The page setup dialog of the sheet (segment rows, the aligned label column, the custom header editor that comes
//! back) and the Margins menu (Excel's presets, 「ユーザー設定の余白…」 opens the dialog).

use crate::shell::i18n::{self, Language};
use crate::shell::scenario::{Scenario, button_probe as buttons};
use crate::ext::*;

const BOOK: &str = "test/editing/xlsx-sales.xlsx";

/// Excel's Margins presets in EMU (914 400 per inch): Normal 0.7 / 0.7 / 0.75 / 0.75 in, Wide 1 in all round,
/// Narrow 0.25 / 0.25 / 0.75 / 0.75 in (left, right, top, bottom).
const NORMAL: [i64; 4] = [640_080, 640_080, 685_800, 685_800];
const WIDE: [i64; 4] = [914_400; 4];
const NARROW: [i64; 4] = [228_600, 228_600, 685_800, 685_800];

pub(crate) fn margin_menu_presets() {
    let mut s = Scenario::open(BOOK);
    for (id, want) in [("xlsx.margins-wide", WIDE), ("xlsx.margins-narrow", NARROW), ("xlsx.margins-normal", NORMAL)] {
        s.menu(id);
        assert_eq!(s.with(|e, _| e.current_print().unwrap().margins), want, "{id}");
        assert!(!s.with(|e, _| e.wb().page_setup), "{id} sets the margins; it does not open the dialog");
    }
    // 「ユーザー設定の余白…」 opens the dialog (and leaves the margins).
    s.menu("xlsx.margins-custom");
    assert!(s.with(|e, _| e.wb().page_setup));
    assert_eq!(s.with(|e, _| e.current_print().unwrap().margins), NORMAL);
}

/// Every control of the form starts at one x, and the label column is the widest unwrapped label (to a whole pixel).
fn assert_aligned() {
    let n = buttons::all().iter().filter(|k| k.starts_with("page-setup-control-")).count();
    assert!(n >= 10, "the form's rows are measured: {:?}", buttons::all());
    let gap = crate::shell::chrome::label_form::LABEL_GAP;
    let mut widths = Vec::new();
    let mut xs = Vec::new();
    for i in 0..n {
        let text = buttons::bounds(&format!("page-setup-text-{i}")).unwrap();
        assert!(text.size.height < gpui_kit::px(30.), "label {i} is on one line");
        widths.push(f32::from(text.size.width));
        xs.push(f32::from(buttons::bounds(&format!("page-setup-control-{i}")).unwrap().origin.x));
    }
    assert!(xs.iter().all(|x| (x - xs[0]).abs() < 0.5), "the controls start at one x: {xs:?}");
    let label = buttons::bounds("page-setup-label-0").unwrap();
    let column = xs[0] - f32::from(label.origin.x) - gap;
    let widest = crate::shell::chrome::label_form::label_column_width(&widths);
    assert!((column - widest).abs() < 0.5, "label column {column} = the widest label {widest} ({widths:?})");
}

pub(crate) fn dialog_layout_and_header_editor() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(BOOK);
    s.size(1500., 1300.);
    for (language, name) in [(Language::Japanese, "ja"), (Language::English, "en")] {
        i18n::show(language);
        s.menu("file.page-setup");
        s.settle();
        s.settle();
        // +30 % of the default 520 px.
        let width = s.with(|e, _| e.dialog.bounds).unwrap().size.width;
        assert!((f32::from(width) - 676.).abs() < 2., "dialog width {width:?}");
        assert_aligned();
        // Commands are buttons, choices are segments of the same row (no radio buttons).
        for id in ["title-cols-set", "titles-set", "area-set", "header-custom", "footer-custom", "row-break"] {
            assert!(buttons::bounds(id).is_some(), "{id} is a button");
        }
        s.screenshot(&format!("page-setup-{name}"));
        s.with(|e, _| assert!(e.dialog.bounds.is_some()));
        // Header options: one per line, indented from the label.
        let a = buttons::bounds("hf-diff-first").unwrap();
        let b = buttons::bounds("hf-diff-odd-even").unwrap();
        assert!(b.origin.y > a.origin.y + a.size.height - gpui_kit::px(1.) && (a.origin.x - b.origin.x).abs() < gpui_kit::px(1.), "one check box per line");
        // The custom button is on its own line under the presets, indented.
        let presets = buttons::bounds("header-docx.page-number").unwrap();
        let custom = buttons::bounds("header-custom").unwrap();
        assert!(custom.origin.y >= presets.origin.y + presets.size.height - gpui_kit::px(1.));
        // A segment is chosen with a click; the sheet follows.
        buttons::click(&mut s, "landscape");
        s.settle();
        assert!(s.with(|e, _| e.current_print().unwrap().landscape));
        buttons::click(&mut s, "portrait");
        s.settle();
        assert!(!s.with(|e, _| e.current_print().unwrap().landscape));
        // The custom header: its editor takes the dialog's place and one click goes back, the dialog intact.
        buttons::click(&mut s, "header-custom");
        s.settle();
        assert!(s.with(|e, _| e.wb().header_footer_edit.is_some()), "the editor is open");
        s.screenshot(&format!("header-editor-{name}"));
        buttons::click(&mut s, "hf-back-to-page-setup");
        s.settle();
        assert!(!s.with(|e, _| e.wb().header_footer_edit.is_some()), "back to the page setup");
        assert!(s.with(|e, _| e.wb().page_setup), "the page setup is still open");
        assert!(buttons::bounds("header-custom").is_some(), "the dialog is drawn again");
        s.update(|e, w, cx| e.close_dialog(w, cx));
        s.settle();
    }
    i18n::show(Language::Japanese);
}

/// The segment rows of the dialog take the arrow keys (radio-group keys): the selection follows the focus, and the sheet follows
/// the selection; Home/End go to the ends, the ends stop, and Tab leaves the row.
pub(crate) fn segment_arrow_keys() {
    use crate::print::PAPERS;
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(BOOK);
    s.size(1500., 1300.);
    s.menu("file.page-setup");
    s.settle();
    s.settle();
    let paper = |s: &mut Scenario| s.with(|e, _| e.current_print().unwrap().paper);
    let landscape = |s: &mut Scenario| s.with(|e, _| e.current_print().unwrap().landscape);
    // Orientation: Right selects 横 and moves the focus; at the end it stops; Left/Home/End.
    buttons::focus(&mut s, "portrait");
    assert!(!landscape(&mut s));
    s.key("right");
    assert!(landscape(&mut s));
    assert_eq!(buttons::focused(&mut s).as_deref(), Some("landscape"));
    s.key("right");
    assert!(landscape(&mut s), "stops at the end");
    assert_eq!(buttons::focused(&mut s).as_deref(), Some("landscape"));
    s.key("left");
    assert!(!landscape(&mut s));
    s.key("end");
    assert!(landscape(&mut s));
    s.key("home");
    assert!(!landscape(&mut s));
    // Up/Down are the same as Left/Right.
    s.key("down");
    assert!(landscape(&mut s));
    s.key("up");
    assert!(!landscape(&mut s));
    // Tab leaves the row: the focus goes to a control that is not one of its segments.
    s.key("tab");
    let after = buttons::focused(&mut s);
    assert!(!matches!(after.as_deref(), Some("portrait" | "landscape")), "Tab leaves the row: {after:?}");
    // Paper: the neighbours in order, Home/End at the ends of the whole row (the row wraps; the order is the item order).
    let (first, last) = (PAPERS[0], PAPERS[PAPERS.len() - 1]);
    buttons::focus(&mut s, &format!("paper-{}", first.0));
    s.key("right");
    assert_eq!(paper(&mut s), PAPERS[1].1);
    assert_eq!(buttons::focused(&mut s), Some(format!("paper-{}", PAPERS[1].0)));
    s.key("end");
    assert_eq!(paper(&mut s), last.1);
    s.key("home");
    assert_eq!(paper(&mut s), first.1);
    // Scale: 100 % -> 80 %.
    buttons::focus(&mut s, "scale-100");
    s.key("right");
    assert_eq!(s.with(|e, _| e.current_print().unwrap().scale), 80);
    assert!(s.with(|e, _| e.current_print().unwrap().fit_to).is_none());
    s.key("end");
    assert_eq!(buttons::focused(&mut s).as_deref(), Some("fit-height-1"));
    // Print what: active sheet -> whole workbook -> selection.
    buttons::focus(&mut s, "print-active-sheet");
    s.key("right");
    assert_eq!(s.with(|e, _| e.print_what()), crate::print::PrintWhat::EntireWorkbook);
    s.key("right");
    assert_eq!(s.with(|e, _| e.print_what()), crate::print::PrintWhat::Selection);
    s.key("home");
    assert_eq!(s.with(|e, _| e.print_what()), crate::print::PrintWhat::ActiveSheet);
    s.update(|e, w, cx| e.close_dialog(w, cx));
}
