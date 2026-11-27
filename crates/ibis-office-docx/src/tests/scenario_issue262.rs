// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #262: the formatting marks of a document window. They are shown from the start (as the settings' defaults say), each
//! of Word's items (タブ・スペース・段落記号・改行・隠し文字・任意指定のハイフン・アンカー) is chosen in ツール > 設定… > ドキュメント, a change reaches the
//! windows already open, and the marks are a bright light blue, not grey.
use crate::shell::scenario::Scenario;
use crate::ext::*;
use crate::shell::settings_dialog::DOCX_MARKS;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue262_the_marks_are_shown_from_the_start_each_item_by_its_setting", defaults_and_items),
        ("docx::issue262_a_change_of_the_settings_reaches_the_open_window", change_reaches_open_window),
        ("docx::issue262_the_marks_are_painted_in_light_blue_and_only_the_chosen_ones", painted),
    ]
}

/// Every mark's setting taken away (the defaults), or set to `value`.
fn set_all(value: Option<&str>) {
    for (key, _, _) in DOCX_MARKS {
        match value {
            Some(v) => crate::shell::settings::set(key, v),
            None => crate::shell::settings::remove(key),
        }
    }
}

/// Settings put back when the test ends (a failed assertion too).
struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        set_all(None);
    }
}

fn options(s: &mut Scenario) -> crate::prefs::FormattingMarksOptions {
    s.with(|e, _| e.doc_prefs().formatting_marks_options)
}

fn defaults_and_items() {
    let _restore = Restore;
    set_all(None);
    let o = {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        let o = options(&mut s);
        assert!(!s.with(|e, _| e.doc_prefs().formatting_marks_open), "the ¶ button is off: the items alone show the marks");
        o
    };
    // Shown from the start: all but 隠し文字 (it changes what the page shows, as in Word).
    assert!(o.show_tabs && o.show_spaces && o.show_paragraph_marks && o.show_line_breaks && o.show_optional_hyphens && o.show_object_anchors, "{o:?}");
    assert!(!o.show_hidden_text, "hidden text is not shown by default");
    // Each item by its own setting: a window opened after a setting is 0 does not show that mark, and the others stay.
    for (i, (key, _, default)) in DOCX_MARKS.iter().enumerate() {
        set_all(None);
        crate::shell::settings::set(key, if *default { "0" } else { "1" });
        let mut s = Scenario::open("test/editing/docx-report.docx");
        let o = options(&mut s);
        let flags = [o.show_tabs, o.show_spaces, o.show_paragraph_marks, o.show_line_breaks, o.show_hidden_text, o.show_optional_hyphens, o.show_object_anchors];
        for (j, (_, _, d)) in DOCX_MARKS.iter().enumerate() {
            assert_eq!(flags[j], if i == j { !*d } else { *d }, "setting {key}: item {j} of {flags:?}");
        }
    }
}

fn change_reaches_open_window() {
    let _restore = Restore;
    set_all(None);
    let mut s = Scenario::open("test/editing/docx-report.docx");
    assert!(options(&mut s).show_paragraph_marks);
    crate::shell::settings::set("docx.marks.paragraph", "0");
    crate::shell::settings_dialog::note_docx_marks_changed();
    s.settle();
    assert!(!options(&mut s).show_paragraph_marks, "the window read the new setting");
    // Hidden text changes the layout: the window shows it as soon as the setting says so.
    assert!(!s.with(|e, _| e.showing_hidden_text()));
    crate::shell::settings::set("docx.marks.hidden-text", "1");
    crate::shell::settings_dialog::note_docx_marks_changed();
    s.settle();
    assert!(s.with(|e, _| e.showing_hidden_text()), "hidden text is shown");
}

/// The pixels of `image` within `tolerance` (per channel) of the mark colour.
fn mark_pixels(path: &std::path::Path) -> usize {
    let c = crate::prefs::MARK_COLOR;
    let want = [(c >> 16) as i32 & 0xff, (c >> 8) as i32 & 0xff, c as i32 & 0xff];
    let image = image::open(path).expect("the screenshot").to_rgba8();
    // Only the document's area: the app's own logo, toolbar icons and zoom slider are light blue too (the window's chrome is not the marks).
    let (w, h) = image.dimensions();
    let (top, bottom) = (h * 7 / 40, h - h / 16);
    (top..bottom).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| {
        let p = image.get_pixel(x, y);
        (0..3).all(|i| (p.0[i] as i32 - want[i]).abs() <= 24)
    }).count()
}

fn painted() {
    let _restore = Restore;
    // The colour itself: a bright light blue (blue full, green high, red low).
    let c = crate::prefs::MARK_COLOR;
    assert!((c & 0xff) >= 0xe0 && ((c >> 8) & 0xff) >= 0xa0 && ((c >> 16) & 0xff) <= 0x80, "{c:06x} is a bright light blue");
    set_all(None);
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1200., 800.);
    s.settle();
    let shown = mark_pixels(&s.screenshot("marks-default"));
    assert!(shown >= 20, "the marks are painted from the start, in light blue ({shown} pixels)");
    // Every item off: no mark colour in the page.
    set_all(Some("0"));
    crate::shell::settings_dialog::note_docx_marks_changed();
    s.settle();
    let none = mark_pixels(&s.screenshot("marks-none"));
    assert!(none < shown / 10, "no marks with every item off ({none} against {shown})");
    // The paragraph mark alone.
    crate::shell::settings::set("docx.marks.paragraph", "1");
    crate::shell::settings_dialog::note_docx_marks_changed();
    s.settle();
    let paragraph_only = mark_pixels(&s.screenshot("marks-paragraph"));
    assert!(paragraph_only > none && paragraph_only < shown, "the paragraph marks alone ({none} < {paragraph_only} < {shown})");
}
