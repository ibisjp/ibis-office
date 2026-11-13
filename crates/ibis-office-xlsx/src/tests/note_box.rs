// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! メモ (a cell's note) written in its own box: a new note's first text and caret, the box's handles (resize) and border
//! (move), the card the pointer shows (a second on the selected cell, a second kept), the formats of the text, and the
//! file (the text's runs and the box, read back).

use ibis_doc_model::{CellRun, CellUnderline, NoteRect, Rgb, SheetComment, StyleChange, sheet_calc::sheet_of_slide};

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use crate::ext::*;

const POINT: i64 = ibis_doc_model::EMU_PER_POINT;

fn note_on(s: &mut Scenario, row: u32, column: u32) -> Option<SheetComment> {
    s.with(move |e, _| e.current_sheet().and_then(|(_, sh)| sh.comment_at(row, column).cloned()))
}

/// A note on `row`, `column` made by the model (no box of its own yet).
fn put_note(s: &mut Scenario, row: u32, column: u32, text: &str) {
    let slide = s.with(|e, _| e.current_slide);
    let comment = SheetComment { row, column, author: "Tester".into(), text: text.into(), ..Default::default() };
    s.update(|e, _, _| {
        e.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetComment { slide, row, column, comment: Some(comment) })]).unwrap();
    });
}

/// The box on the screen: its top left corner, its size and the cell's top right corner.
fn screen_box(s: &mut Scenario) -> ((f32, f32), (f32, f32), (f32, f32)) {
    s.with(|e, _| {
        let c = e.cell_edit().expect("a note being typed into");
        let rect = c.note.as_ref().unwrap().rect;
        let (o, sz, tr) = e.note_screen_box(c.column, c.row, rect).unwrap();
        ((f32::from(o.x), f32::from(o.y)), (f32::from(sz.width), f32::from(sz.height)), (f32::from(tr.x), f32::from(tr.y)))
    })
}

fn rect_of(s: &mut Scenario) -> NoteRect {
    s.with(|e, _| e.cell_edit().and_then(|c| c.note.as_ref()).expect("a note being typed into").rect)
}

/// A new note: the user's name, a colon and a line break are there at once, the caret at the start of the second line,
/// and what is typed goes there; Esc keeps it, with the box; one undo step takes it off.
pub(super) fn new_note_text_and_caret() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    s.click_cell("C3");
    s.menu("xlsx.note-new");
    let name = crate::shell::user_name::user_name();
    let (text, caret, is_note) = s.with(|e, _| {
        let c = e.cell_edit().unwrap();
        (c.text.clone(), c.caret, c.note.is_some())
    });
    assert!(is_note);
    assert_eq!(text, format!("{name}:\n"));
    assert_eq!(caret, text.encode_utf16().count(), "the caret is at the start of the second line");
    // A square, yellow box: its place beside the cell, the eight handles drawn (the box is where the rect says).
    let ((x, y), (w, h), (trx, try_)) = screen_box(&mut s);
    assert!(x > trx && y < try_ + 1.0, "a little right of the cell's top right corner, from its top up: {x},{y} vs {trx},{try_}");
    assert!(w > 100. && h > 50., "{w}x{h}");
    s.screenshot("note-box-new");
    // Typing goes at once (no click): the text is on the second line.
    s.type_text("Check this");
    s.key("enter");
    s.type_text("Third line");
    assert_eq!(s.with(|e, _| e.cell_edit().unwrap().text.clone()), format!("{name}:\nCheck this\nThird line"));
    s.key("escape");
    assert!(s.with(|e, _| e.cell_edit().is_none()));
    let n = note_on(&mut s, 2, 2).expect("the note");
    assert_eq!(n.text, format!("{name}:\nCheck this\nThird line"));
    assert_eq!(n.author, name);
    assert!(n.rect.is_some());
    // The name is bold (as Excel writes it); the rest is not.
    assert_eq!(n.runs.first().map(|r| (r.text.clone(), r.bold)), Some((format!("{name}:"), Some(true))));
    assert!(n.runs.iter().skip(1).all(|r| r.bold != Some(true)));
    s.key("cmd-z");
    assert!(note_on(&mut s, 2, 2).is_none(), "one undo step");
    // The right pane has no panel for it.
    s.menu("xlsx.note-new");
    assert!(!s.with(|e, _| e.sidebar_open));
    s.key("escape");
}

/// The pointer on the selected cell that has a note for a second shows it; it stays a second after the pointer left the
/// cell, and while the pointer is on it; the pointer on a cell that is not selected shows nothing; a click on it edits it.
pub(super) fn hover_shows_the_note() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    put_note(&mut s, 2, 2, "Tester:\nHello");
    s.click_cell("C3");
    let (cx_, cy_) = s.with(|e, _| e.cell_corner(2, 2)).map(|p| (f32::from(p.x), f32::from(p.y))).unwrap();
    let tr = s.with(|e, _| e.cell_top_right(2, 2)).map(|p| (f32::from(p.x), f32::from(p.y))).unwrap();
    // Inside the cell: left of its right edge, below its top.
    let in_cell = (tr.0 - 12., tr.1 + 8.);
    let away = (tr.0 + 300., tr.1 + 300.);
    let shown = |s: &mut Scenario| s.with(|e, _| e.wb().note_card.shown);
    let _ = (cx_, cy_);
    s.hover(in_cell);
    assert_eq!(shown(&mut s), None, "just arrived");
    s.wait(900);
    assert_eq!(shown(&mut s), None, "0.9 s");
    s.wait(200);
    assert_eq!(shown(&mut s), Some((2, 2)), "1.1 s: shown");
    s.screenshot("note-box-hover");
    // Leaving the cell: it stays a second.
    s.hover(away);
    s.wait(900);
    assert_eq!(shown(&mut s), Some((2, 2)), "0.9 s after leaving");
    s.wait(200);
    assert_eq!(shown(&mut s), None, "1.1 s after leaving: gone");
    // Leaving before the second: never shown.
    s.hover(in_cell);
    s.wait(500);
    s.hover(away);
    s.wait(1500);
    assert_eq!(shown(&mut s), None, "left too soon");
    // Back in the cell within the grace time: it stays.
    s.hover(in_cell);
    s.wait(1100);
    s.hover(away);
    s.wait(600);
    s.hover(in_cell);
    s.wait(1500);
    assert_eq!(shown(&mut s), Some((2, 2)), "came back in the second");
    // On the card: it stays as long as the pointer is on it.
    let card = s.with(|e, _| e.wb().note_card.bounds.get()).expect("the card was drawn");
    let on_card = (f32::from(card.origin.x + card.size.width / 2.0), f32::from(card.origin.y + card.size.height / 2.0));
    s.hover(on_card);
    s.wait(5000);
    assert_eq!(shown(&mut s), Some((2, 2)), "on the card, long after");
    s.hover(away);
    s.wait(900);
    assert_eq!(shown(&mut s), Some((2, 2)), "just left the card");
    s.wait(200);
    assert_eq!(shown(&mut s), None, "a second after leaving the card");
    // A cell that is not the selected one: nothing.
    s.click_cell("A1");
    s.hover(in_cell);
    s.wait(1500);
    assert_eq!(shown(&mut s), None, "not the selected cell");
    // A click on the card edits it.
    s.click_cell("C3");
    s.hover(in_cell);
    s.wait(1100);
    let card = s.with(|e, _| e.wb().note_card.bounds.get()).expect("the card was drawn");
    s.click((f32::from(card.origin.x + card.size.width / 2.0), f32::from(card.origin.y + gpui_kit::px(6.0))));
    assert!(s.with(|e, _| e.note_edit().is_some()), "editing the note");
    assert_eq!(s.with(|e, _| e.wb().note_card.shown), None, "the card is the editing box now");
    s.type_text("X");
    assert!(s.with(|e, _| e.cell_edit().unwrap().text.contains('X')));
    s.key("escape");
    assert!(note_on(&mut s, 2, 2).unwrap().text.contains('X'));
}

/// The eight handles resize the box and its border moves it (the cell's corner does not move); the box is kept as a rect.
pub(super) fn resize_and_move() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    s.click_cell("C3");
    s.menu("xlsx.note-new");
    let start = rect_of(&mut s);
    let scale = s.with(|e, _| e.slide_layout().unwrap().scale);
    let emu = |px: f32| (px / scale).round() as i64;
    let ((x, y), (w, h), _) = screen_box(&mut s);
    // The bottom right handle: 40 px right and 30 px down grows the box by as much.
    s.drag((x + w, y + h), (x + w + 40., y + h + 30.));
    let r = rect_of(&mut s);
    assert!((r.width - start.width - emu(40.)).abs() <= 2 && (r.height - start.height - emu(30.)).abs() <= 2, "{r:?} from {start:?}");
    assert_eq!((r.dx, r.dy), (start.dx, start.dy), "the top left corner stays");
    // The top left handle: the box grows to the upper left, the bottom right corner stays.
    let ((x, y), (w, h), _) = screen_box(&mut s);
    s.drag((x, y), (x - 20., y + 10.));
    let r2 = rect_of(&mut s);
    assert!((r2.dx - (r.dx - emu(20.))).abs() <= 2 && (r2.dy - (r.dy + emu(10.))).abs() <= 2, "{r2:?}");
    assert!(((r2.dx + r2.width) - (r.dx + r.width)).abs() <= 2 && ((r2.dy + r2.height) - (r.dy + r.height)).abs() <= 2, "the far corner stays: {r2:?}");
    let _ = (w, h);
    // Each of the eight handles is a hit: the middle of the right edge widens it only.
    let ((x, y), (w, h), _) = screen_box(&mut s);
    s.drag((x + w, y + h / 2.), (x + w + 15., y + h / 2. + 25.));
    let r3 = rect_of(&mut s);
    assert!((r3.width - r2.width - emu(15.)).abs() <= 2 && r3.height == r2.height && (r3.dy, r3.dx) == (r2.dy, r2.dx), "{r3:?} from {r2:?}");
    // Too small is refused: at least a little box is kept.
    let ((x, y), (w, h), _) = screen_box(&mut s);
    s.drag((x + w, y + h), (x - 500., y - 500.));
    let r4 = rect_of(&mut s);
    assert!(r4.width >= 16 * POINT && r4.height >= 16 * POINT, "{r4:?}");
    s.screenshot("note-box-resized");
    // The border moves the box: a press on the top edge, away from the handles.
    let ((x, y), (w, _), tr) = screen_box(&mut s);
    let before = rect_of(&mut s);
    s.drag((x + w * 0.25, y), (x + w * 0.25 + 60., y + 45.));
    let moved = rect_of(&mut s);
    assert!((moved.dx - before.dx - emu(60.)).abs() <= 2 && (moved.dy - before.dy - emu(45.)).abs() <= 2, "{moved:?} from {before:?}");
    assert_eq!((moved.width, moved.height), (before.width, before.height), "the size stays");
    // The cell's corner is where it was; the box is joined to it.
    let ((_, _), (_, _), tr2) = screen_box(&mut s);
    assert_eq!(tr, tr2);
    // The typed text is not lost, and the box stays on Esc.
    s.key("escape");
    let n = note_on(&mut s, 2, 2).unwrap();
    assert_eq!(n.rect, Some(moved));
}

/// The text takes size, colour, bold, italic and underline: on the selected part, or from the caret on.
pub(super) fn text_formats() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    s.click_cell("B2");
    s.menu("xlsx.note-new");
    s.type_text("Plain bold red");
    // Select "bold" and make it bold, italic, underlined, red, 14 pt.
    s.update(|e, _, cx| {
        let name = crate::shell::user_name::user_name();
        let start = name.encode_utf16().count() + 2 + "Plain ".encode_utf16().count();
        let edit = e.cell_edit_mut().unwrap();
        edit.anchor = Some(start);
        edit.caret = start + 4;
        for change in [StyleChange::Bold(true), StyleChange::Italic(true), StyleChange::Underline(true), StyleChange::Color(Rgb(0xFF0000)), StyleChange::FontSize(1400)] {
            assert!(e.format_edit_selection(&change, cx), "{change:?}");
        }
    });
    s.screenshot("note-box-formats");
    s.key("escape");
    let n = note_on(&mut s, 1, 1).unwrap();
    let bold: &CellRun = n.runs.iter().find(|r| r.text == "bold").expect("the part");
    assert_eq!((bold.bold, bold.italic, bold.underline, bold.color, bold.size), (Some(true), Some(true), Some(CellUnderline::Single), Some(Rgb(0xFF0000)), Some(1400)));
    let plain = n.runs.iter().find(|r| r.text.contains("Plain")).expect("the rest");
    assert_eq!((plain.italic, plain.underline, plain.size), (Some(false), Some(CellUnderline::None), Some(900)));
    // Typing with the caret alone: Bold waits for the next text.
    s.menu("xlsx.note-edit");
    s.update(|e, _, cx| {
        assert!(e.format_edit_selection(&StyleChange::Bold(true), cx));
    });
    s.type_text("!");
    s.key("escape");
    let n = note_on(&mut s, 1, 1).unwrap();
    assert!(n.runs.iter().any(|r| r.text.ends_with('!') && r.bold == Some(true)), "{:?}", n.runs);
}

/// Saved: the runs (size, colour, bold, italic, underline) and the box (moved and resized) come back as they were;
/// the drawing (VML) holds the box's anchor and style.
pub(super) fn saved_and_read_back() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    s.click_cell("B2");
    s.menu("xlsx.note-new");
    s.type_text("Red bold");
    s.update(|e, _, cx| {
        let name = crate::shell::user_name::user_name();
        let start = name.encode_utf16().count() + 2;
        let edit = e.cell_edit_mut().unwrap();
        edit.anchor = Some(start);
        edit.caret = start + 3;
        for change in [StyleChange::Bold(true), StyleChange::Color(Rgb(0xFF0000)), StyleChange::Underline(true), StyleChange::FontSize(1200)] {
            assert!(e.format_edit_selection(&change, cx), "{change:?}");
        }
    });
    let ((x, y), (w, h), _) = screen_box(&mut s);
    s.drag((x + w, y + h), (x + w + 50., y + h + 20.));
    let ((x, y), (w, _), _) = screen_box(&mut s);
    s.drag((x + w * 0.25, y), (x + w * 0.25 + 30., y + 20.));
    s.key("escape");
    let n = note_on(&mut s, 1, 1).unwrap();
    let saved = s.save_as("note-box.xlsx");
    let part = |name: &str| String::from_utf8(std::process::Command::new("unzip").arg("-p").arg(&saved).arg(name).output().unwrap().stdout).unwrap_or_default();
    let listing = String::from_utf8(std::process::Command::new("unzip").arg("-l").arg(&saved).output().unwrap().stdout).unwrap();
    let vml_name = listing.split_whitespace().find(|w| w.ends_with(".vml")).expect("a VML drawing").to_string();
    let vml = part(&vml_name);
    assert!(vml.contains("<x:Anchor>") && vml.contains("margin-left:") && vml.contains("fillcolor=\"#ffffe1\""), "{vml}");
    let comments_name = listing.split_whitespace().find(|w| w.starts_with("xl/comments") && w.ends_with(".xml")).expect("comments.xml").to_string();
    let comments = part(&comments_name);
    assert!(comments.contains("<b/>") && comments.contains("<sz val=\"12\"/>") && comments.contains("FFFF0000") && comments.contains("<u/>"), "{comments}");
    let p = s.reopen();
    let sheet = sheet_of_slide(&p.slides[1]).unwrap();
    let back = sheet.comment_at(1, 1).expect("the note");
    assert_eq!(back.text, n.text);
    assert_eq!(back.author, n.author);
    let red = back.runs.iter().find(|r| r.text == "Red").expect("the part");
    assert_eq!((red.bold, red.color, red.size, red.underline), (Some(true), Some(Rgb(0xFF0000)), Some(1200), Some(CellUnderline::Single)));
    // The box comes back within a pixel of Excel's (the anchor's offsets are whole pixels).
    let (a, b) = (n.rect.unwrap(), back.rect.expect("the box"));
    let tolerance = 9525;
    assert!((a.dx - b.dx).abs() <= tolerance && (a.dy - b.dy).abs() <= tolerance && (a.width - b.width).abs() <= tolerance * 2 && (a.height - b.height).abs() <= tolerance * 2, "{a:?} vs {b:?}");
}

/// A long text typed into the box goes on below it: the lines and the caret are drawn (laid out) past the box's bottom
/// while typing, and the box cuts the text again once the edit is left (Esc); the font is a Gothic.
pub(super) fn long_text_past_the_box() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    s.click_cell("C3");
    s.menu("xlsx.note-new");
    for i in 0..9 {
        s.type_text(&format!("Line number {i}"));
        s.key("enter");
    }
    s.type_text("The last line");
    let ((_, y), (_, h), _) = screen_box(&mut s);
    // The caret's line is below the box's bottom edge, and it was laid out (so it is painted).
    let (caret_y, rect_h) = s.with(|e, _| {
        let c = e.cell_edit().unwrap();
        let layout = c.layout.borrow().clone().unwrap();
        let line = layout.lines.last().unwrap();
        (f32::from(c.rect.get().unwrap().origin.y) + line.baseline + line.descent, f32::from(c.rect.get().unwrap().size.height))
    });
    assert!(caret_y > y + h, "the last line ({caret_y}) is below the box's bottom ({}): box height {rect_h}", y + h);
    s.screenshot("note-box-long-typing");
    s.key("escape");
    s.click_cell("A1");
    s.click_cell("C3");
    let tr = s.with(|e, _| e.cell_top_right(2, 2)).map(|p| (f32::from(p.x), f32::from(p.y))).unwrap();
    s.hover((tr.0 - 12., tr.1 + 8.));
    s.wait(1100);
    assert_eq!(s.with(|e, _| e.wb().note_card.shown), Some((2, 2)));
    s.screenshot("note-box-long-clipped");
    // The font of a note is a Gothic.
    assert_eq!(s.with(|e, _| e.note_font()), "MS PGothic");
}
