// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Web build issues (#73 half-width text in a typeface the browser has) and text-box columns (#77).
use crate::shell::scenario::Scenario;
use ibis_doc_model::TextStyle;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue73_browser_fallback_names_put_requested_typefaces_before_stand_ins", fallback_names),
        ("pptx::issue77_text_columns_flow_line_by_line_edit_and_save", text_columns),
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
        let list = e.pptx().unwrap().positions.paragraphs.borrow().clone();
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
    let paragraph_count = s.with(|e, _| e.pptx().unwrap().positions.paragraphs.borrow().len());
    assert!(paragraph_count < lines.len());
    // A click in the second column's first line puts the caret on it.
    let (n, line) = s.with(|e, _| {
        let list = e.pptx().unwrap().positions.paragraphs.borrow().clone();
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
    let last_of_first = s.with(|e, _| e.pptx().unwrap().positions.paragraphs.borrow()[0].borrow().lines[first_of_second - 1]);
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
