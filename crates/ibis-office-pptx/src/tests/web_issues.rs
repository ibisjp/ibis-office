// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Web build issues (#73 half-width text in a typeface the browser has) and text-box columns (#77).
use crate::shell::scenario::Scenario;
use ibis_doc_model::TextStyle;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue73_browser_fallback_names_put_requested_typefaces_before_stand_ins", fallback_names),
        ("pptx::issue77_text_columns_flow_line_by_line_edit_and_save", text_columns),
        ("pptx::issue77_vertical_text_columns_flow_in_bands_and_save", vertical_text_columns),
        ("pptx::issue77_vertical_three_bands_fill_in_order_or_share_out_the_overflow", vertical_three_bands),
    ]
}

/// The web build draws half-width Latin text with the first of the names before the empty separator that the browser has:
/// only what the document asks for (and Windows/Linux aliases), never the Japanese stand-ins.
fn fallback_names() {
    let names = crate::render::shape_view::browser_fallback_names(&TextStyle { font: Some("MS Mincho".into()), ..TextStyle::default() });
    let end = names.iter().position(String::is_empty).expect("a separator before the East Asian stand-ins");
    assert_eq!(&names[..end], ["MS Mincho"]);
    assert!(names[end + 1..].iter().any(|n| n == "Hiragino Sans"));
    assert!(!names[..end].iter().any(|n| n.contains("Hiragino")));

    let names = crate::render::shape_view::browser_fallback_names(&TextStyle { font: Some("Arial".into()), ..TextStyle::default() });
    assert_eq!(names, ["Arial", "Liberation Sans", "Arimo"]);

    let names = crate::render::shape_view::browser_fallback_names(&TextStyle { font: Some("Times New Roman".into()), fallback_font: Some("MS Gothic".into()), ..TextStyle::default() });
    let end = names.iter().position(String::is_empty).expect("separator");
    assert_eq!(&names[..end], ["Times New Roman", "Liberation Serif", "Tinos", "MS Gothic"]);
}

/// A text box in two columns (`numCol`, `spcCol`): one long paragraph runs down the first column to its end and on from the top of
/// the second (line by line, not whole paragraphs), the caret goes on from one column to the other, a click in the second column
/// finds the line there, and the columns are saved and read back.
fn text_columns() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.select_shape(1);
    s.update(|e, _, cx| e.set_text_columns(2, 180_000, cx));
    s.double_click_shape(1);
    s.key("end");
    // Long enough for more lines than a column holds.
    let text = " words that run on and on and on to fill the box".repeat(40);
    s.type_text(&text);
    let lines = s.with(|e, _| {
        let list = e.shape_edit.positions.paragraphs.borrow().clone();
        list.iter().flat_map(|p| p.borrow().lines.clone()).collect::<Vec<_>>()
    });
    let columns: Vec<usize> = lines.iter().map(|l| l.column).collect();
    assert_eq!(columns.iter().copied().max(), Some(1), "two columns are used: {columns:?}");
    // Down a column, and then on from the top of the next.
    let first_of_second = lines.iter().position(|l| l.column == 1).unwrap();
    assert!(first_of_second > 1, "the first column has lines: {columns:?}");
    assert!(columns[..first_of_second].iter().all(|&c| c == 0) && columns[first_of_second..].iter().all(|&c| c == 1), "{columns:?}");
    assert!(lines[first_of_second].top < lines[first_of_second - 1].top, "the second column starts at the top");
    assert!(lines[first_of_second].left > lines[first_of_second - 1].left + 10.0, "and to the right");
    // The paragraph is split: the lines of both columns belong to its one paragraph.
    let paragraph_count = s.with(|e, _| e.shape_edit.positions.paragraphs.borrow().len());
    assert!(paragraph_count < lines.len());
    // A click in the second column's first line puts the caret on it.
    let (n, line) = s.with(|e, _| {
        let list = e.shape_edit.positions.paragraphs.borrow().clone();
        let p = list[0].borrow();
        (p.lines.len(), p.lines[first_of_second])
    });
    assert!(n > first_of_second);
    s.click((line.left + 5.0, line.top + line.height / 2.0));
    let caret = s.with(|e, _| e.text_editing().unwrap().caret);
    assert!(caret >= line.start && caret <= line.end, "{caret} in {}..{}", line.start, line.end);
    // The caret is drawn in that column.
    let at = s.with(|e, _| e.caret_bounds().unwrap());
    assert!(f32::from(at.origin.x) >= line.left, "{at:?} {}", line.left);
    // Down from the last line of the first column goes to the first of the second.
    let last_of_first = s.with(|e, _| e.shape_edit.positions.paragraphs.borrow()[0].borrow().lines[first_of_second - 1]);
    s.click((last_of_first.left + 5.0, last_of_first.top + last_of_first.height / 2.0));
    s.key("down");
    let down = s.with(|e, _| e.text_editing().unwrap().caret);
    assert!(down >= line.start && down <= line.end, "{down} in {}..{}", line.start, line.end);
    s.screenshot("text-columns-edited");
    s.key("escape");
    s.save_as("columns.pptx");
    let saved = s.reopen();
    let body = saved.slides[0].shapes[1].text_body().unwrap();
    assert_eq!((body.columns, body.column_spacing), (2, 180_000));
}

/// Vertical writing in columns (#77): the columns are bands along the box's height (the layout is turned, so they are the columns
/// of the turned layout: one long paragraph fills the first to its end and goes on in the second, from the start of its lines), and
/// the vertical writing and the columns are saved and read back.
fn vertical_text_columns() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.select_shape(1);
    s.update(|e, _, cx| {
        e.format_text_body(
            |body| {
                body.vertical = true;
                body.vertical_kind = ibis_doc_model::VerticalKind::EastAsian;
                body.columns = 2;
                body.column_spacing = 360_000;
            },
            cx,
        )
    });
    s.double_click_shape(1);
    s.key("end");
    s.type_text(&"a long text for the check of vertical columns, ".repeat(40));
    let lines = s.with(|e, _| {
        let list = e.shape_edit.positions.paragraphs.borrow().clone();
        list.iter().flat_map(|p| p.borrow().lines.clone()).collect::<Vec<_>>()
    });
    let columns: Vec<usize> = lines.iter().map(|l| l.column).collect();
    assert_eq!(columns.iter().copied().max(), Some(1), "two columns are used: {columns:?}");
    let first_of_second = lines.iter().position(|l| l.column == 1).unwrap();
    assert!(first_of_second > 1 && columns[..first_of_second].iter().all(|&c| c == 0) && columns[first_of_second..].iter().all(|&c| c == 1), "{columns:?}");
    s.key("escape");
    s.save_as("vertical-columns.pptx");
    let saved = s.reopen();
    let body = saved.slides[0].shapes[1].text_body().unwrap();
    assert_eq!((body.columns, body.column_spacing, body.vertical), (2, 360_000, true));
}

/// How many lines of `text` each of the 3 columns of a box has (a gap of 16 pt, 16 pt text, the side margins 7.2 pt, the top and bottom
/// ones `margin` pt: a column takes `floor(inner height / 19.25)` lines of 19.25 pt, as Windows PowerPoint does:
/// `tasks/doc/issue77-vertical-columns-measure-result.md`). A vertical box is 220 x 160 pt, a horizontal one 160 x 220 pt.
fn lines_per_band(text: &str, vertical: bool, margin: f64) -> [usize; 3] {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let id = e.document.ids().new_shape_id();
        let mut body = ibis_doc_model::TextBody::plain(text, TextStyle { font_size: 1600, ..TextStyle::default() });
        body.vertical = vertical;
        body.vertical_kind = ibis_doc_model::VerticalKind::EastAsian;
        body.columns = 3;
        body.column_spacing = 16 * 12_700;
        let pt = |v: f64| (v * 12_700.0) as ibis_doc_model::Emu;
        body.insets = ibis_doc_model::Insets { left: pt(7.2), top: pt(margin), right: pt(7.2), bottom: pt(margin) };
        let (w, h) = if vertical { (220, 160) } else { (160, 220) };
        let frame = ibis_doc_model::EmuRect::new(1_000_000, 1_000_000, w * 12_700, h * 12_700);
        let shape = ibis_doc_model::Shape::new(id, frame, ibis_doc_model::ShapeKind::TextBox(body));
        e.run(vec![ibis_doc_model::Command::InsertShape { slide, index: e.slide().shapes.len(), shape }], cx);
        e.set_selection(vec![id]);
        cx.notify();
    });
    let last = s.with(|e, _| e.slide().shapes.len()) - 1;
    s.double_click_shape(last);
    s.key("cmd-a");
    s.key("right");
    s.settle();
    let lines = s.with(|e, _| {
        let list = e.shape_edit.positions.paragraphs.borrow().clone();
        list.iter().flat_map(|p| p.borrow().lines.clone()).collect::<Vec<_>>()
    });
    let mut per_band = [0usize; 3];
    for l in &lines {
        per_band[l.column.min(2)] += 1;
    }
    per_band
}

/// Text in 3 columns: lines that fit fill the columns one after the other; lines that do not fit are given `ceil(lines / 3)` to each
/// column from the first, the last taking the rest (PowerPoint's measures: 20 lines 10, 10, 0; 31: 11, 11, 9; 35: 12, 12, 11; 45: 15, 15, 15;
/// at one character a line 40 lines: 14, 14, 12 and 62: 21, 21, 20; horizontal 35 lines: 12, 12, 11 and 20 lines: 11, 9, 0).
fn vertical_three_bands() {
    let a = |n: usize| "\u{3042}".repeat(n);
    // Vertical, 2 characters a line, 10 lines of room in each column.
    assert_eq!(lines_per_band(&a(40), true, 3.6), [10, 10, 0]);
    assert_eq!(lines_per_band(&a(60), true, 3.6), [10, 10, 10]);
    assert_eq!(lines_per_band(&a(62), true, 3.6), [11, 11, 9]);
    assert_eq!(lines_per_band(&a(64), true, 3.6), [11, 11, 10]);
    assert_eq!(lines_per_band(&a(66), true, 3.6), [11, 11, 11]);
    assert_eq!(lines_per_band(&a(70), true, 3.6), [12, 12, 11]);
    assert_eq!(lines_per_band(&a(90), true, 3.6), [15, 15, 15]);
    // Vertical, one character a line (the top and bottom margins 30 pt).
    assert_eq!(lines_per_band(&a(40), true, 30.0), [14, 14, 12]);
    assert_eq!(lines_per_band(&a(62), true, 30.0), [21, 21, 20]);
    // Horizontal (11 lines of room in each column).
    assert_eq!(lines_per_band(&a(70), false, 3.6), [12, 12, 11]);
    assert_eq!(lines_per_band(&a(40), false, 3.6), [11, 9, 0]);
}
