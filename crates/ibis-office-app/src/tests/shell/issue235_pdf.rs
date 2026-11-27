// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #235: a PDF is opened for viewing (pure Rust: `ibis-pdf` reads it, `ibis-office-pdf` draws its pages), as a screen of its own.
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::issue235_a_pdf_opens_as_a_screen_of_its_own_with_its_pages", opens),
        ("shell::issue235_the_menus_of_a_pdf_are_those_of_a_file_that_is_looked_at", menus),
        ("shell::issue235_a_copy_and_an_export_of_a_pdf_are_the_file_as_it_is", copies),
        ("shell::issue235_text_is_picked_by_dragging_and_copied", pick_and_copy),
        ("shell::issue235_all_the_text_is_selected_and_copied_page_by_page", select_all_and_copy),
        ("shell::issue235_the_text_is_searched_on_all_the_pages", search),
        ("shell::issue235_a_page_is_gone_to_by_its_number", go_to_page),
    ]
}

fn opens() {
    let mut s = Scenario::open("test/editing/mac-office-shape.pdf");
    assert_eq!(s.with(|e, _| e.kind()), ibis_formats::Kind::Pdf);
    let pages = s.with(|e, _| e.presentation().slides.len());
    assert!(pages >= 1, "the file's pages are the screen's pages: {pages}");
    assert!(s.with(|e, _| e.presentation().pdf_bytes.is_some()));
    // The file's own name and kind are shown, and the page count is in the status bar.
    let texts = s.status_texts();
    assert!(!texts.is_empty(), "the status bar says something: {texts:?}");
    s.screenshot("pdf-issue235-opened");
}

/// The menu bar and the toolbar are the document screen's, whole: what a PDF cannot do is grey, not left out.
fn menus() {
    use crate::shell::chrome::{MenuEntry, MenuId, ToolItem};
    fn enabled(entries: &[MenuEntry], out: &mut Vec<&'static str>, all: &mut usize) {
        for entry in entries {
            match entry {
                MenuEntry::Item(item) => {
                    *all += 1;
                    if !item.disabled && item.handler.is_some() {
                        out.push(item.id);
                    }
                }
                MenuEntry::Submenu(sub) => enabled(&sub.items, out, all),
                _ => {}
            }
        }
    }
    let mut s = Scenario::open("test/editing/mac-office-shape.pdf");
    let (ids, on, all, buttons) = s.with(|e, _| {
        let ids: Vec<MenuId> = crate::shell::menu_layout::layout(ibis_formats::Kind::Pdf).iter().map(|m| m.id).collect();
        let bar = e.menu_bar_menus().menus;
        let (mut on, mut all) = (Vec::new(), 0);
        for (_, entries) in &bar {
            enabled(entries, &mut on, &mut all);
        }
        let buttons: Vec<(&'static str, bool)> = e.toolbar_items().iter().filter_map(|t| if let ToolItem::Button(b) = t { Some((b.id, b.disabled || b.handler.is_none())) } else { None }).collect();
        (ids, on, all, buttons)
    });
    // The document screen's menus, whole.
    assert_eq!(ids.len(), 11, "{ids:?}");
    for menu in [MenuId::Insert, MenuId::Layout, MenuId::Review] {
        assert!(ids.contains(&menu), "{menu:?} stays, grey");
    }
    assert!(all > 150, "the document's lines are all there: {all}");
    // What works: opening, closing, a copy, the PDF written out, printing, the other application, the zoom, the windows, the help.
    for id in ["file.open", "file.close", "file.copy", "file.export.pdf", "file.print", "window.open-in-office", "view.zoom-in", "view.zoom-fit", "window.list", "help.about"] {
        assert!(on.contains(&id), "{id} works: {on:?}");
    }
    // What does not is grey, not left out: the document's own commands, saving, the pictures of the pages.
    for id in ["file.save", "file.save-as", "file.export.png", "file.export.gif", "file.protect"] {
        assert!(!on.contains(&id), "{id} is grey");
    }
    assert!(on.iter().all(|id| !id.starts_with("docx.") || matches!(*id, "docx.never")), "a document's command works on a PDF: {on:?}");
    // A PDF with no text (this one is pictures and shapes) has nothing to pick or search: Select all and Find are grey; Go to a page works.
    assert!(!on.contains(&"edit.select-all") && !on.contains(&"edit.find-replace"), "no text: Select all and Find are grey: {on:?}");
    assert!(on.contains(&"edit.go-to"), "a page is gone to without any text: {on:?}");
    let working: Vec<&str> = buttons.iter().filter(|b| !b.1).map(|b| b.0).collect();
    assert!(!working.contains(&"formatting-marks"), "the document's buttons are grey: {working:?}");
    eprintln!("enabled buttons: {working:?}");
    use crate::shell::format::FormatEditor;
    let info = ibis_office_pdf::editor::PdfDocument::INFO;
    // The "open in" button exists on macOS and Windows only (`OfficeApp::open_key`).
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let open_in = info.office.open_key;
        assert!(open_in.starts_with("window.open-pdf-in-"), "the PDF's own words, not Word's: {open_in}");
    }
    assert_eq!(info.save_formats.len(), 0, "a PDF is not saved as another kind");
    s.screenshot("pdf-issue235-menus");
    // (One screen at a time.)
    drop(s);
    // One with text (a file of the scenarios below): they work.
    let dir = crate::shell::scenario::repo_root().join("target/scenario/issue235-menus-src");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("text.pdf"), text_pdf()).unwrap();
    let mut with_text = Scenario::open("target/scenario/issue235-menus-src/text.pdf");
    let on_text = with_text.with(|e, _| {
        let (mut on, mut all) = (Vec::new(), 0);
        for (_, entries) in &e.menu_bar_menus().menus {
            enabled(entries, &mut on, &mut all);
        }
        on
    });
    assert!(on_text.contains(&"edit.select-all") && on_text.contains(&"edit.find-replace") && on_text.contains(&"edit.go-to"), "text: they work: {on_text:?}");
}

/// A copy saved, and the PDF written by 書き出し, are the bytes of the file that was opened.
fn copies() {
    let mut s = Scenario::open("test/editing/mac-office-shape.pdf");
    let original = std::fs::read(crate::shell::scenario::repo_root().join("test/editing/mac-office-shape.pdf")).unwrap();
    let dir = crate::shell::scenario::repo_root().join("target/scenario/issue235-copies");
    std::fs::create_dir_all(&dir).unwrap();
    let copy = dir.join("copy.pdf");
    s.with(|e, _| e.copy_to(&copy)).expect("a copy");
    assert_eq!(std::fs::read(&copy).unwrap(), original);
    let out = dir.join("export.pdf");
    let job = s.with(|e, _| e.write_pdf(out.clone())).expect("a job");
    job().expect("written");
    assert_eq!(std::fs::read(&out).unwrap(), original);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A PDF of two pages with text in a standard font (what the scenarios of the text work on): the first has two lines, the second one.
fn text_pdf() -> String {
    let pages = ["BT /F1 24 Tf 72 720 Td (Hello PDF world) Tj ET\nBT /F1 24 Tf 72 680 Td (Search the Text here) Tj ET", "BT /F1 24 Tf 72 720 Td (Second page text) Tj ET"];
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 7 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{}\nendstream", pages[0].len(), pages[0]),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << /Font << /F1 7 0 R >> >> >>".into(),
        format!("<< /Length {} >>\nstream\n{}\nendstream", pages[1].len(), pages[1]),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.drain(..).enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
    }
    let xref = out.len();
    out.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1));
    for o in &offsets {
        out.push_str(&format!("{o:010} 00000 n \n"));
    }
    out.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1));
    out
}

fn open_text_pdf() -> Scenario {
    let dir = crate::shell::scenario::repo_root().join("target/scenario/issue235-text-src");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("text.pdf"), text_pdf()).unwrap();
    Scenario::open("target/scenario/issue235-text-src/text.pdf")
}

fn clipboard(s: &mut Scenario) -> Option<String> {
    s.with_app(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

fn position(s: &mut Scenario, page: usize, index: usize) -> (f32, f32) {
    s.with(|e, _| ibis_office_pdf::editor::char_position(e, page, index)).expect("the page was drawn")
}

/// Pressing on a character and dragging to another picks the text between them; the copy puts it on the clipboard; a click picks nothing.
fn pick_and_copy() {
    let mut s = open_text_pdf();
    assert_eq!(s.with(|e, _| e.kind()), ibis_formats::Kind::Pdf);
    // Nothing is picked: the copy is grey.
    assert_eq!(s.menu_state("edit.copy").map(|m| m.0), Some(false), "nothing picked: copy is grey");
    let (a, b) = (position(&mut s, 0, 0), position(&mut s, 0, 4));
    s.drag(a, b);
    assert_eq!(s.with(|e, _| ibis_office_pdf::editor::selected_text(e)), "Hello");
    s.menu("edit.copy");
    assert_eq!(clipboard(&mut s).as_deref(), Some("Hello"));
    // Dragging down to the second line picks to the end of the first and the start of the second.
    let (a, c) = (position(&mut s, 0, 6), position(&mut s, 0, 18));
    s.drag(a, c);
    let picked = s.with(|e, _| ibis_office_pdf::editor::selected_text(e));
    assert!(picked.starts_with("PDF world\nSear"), "{picked:?}");
    // A click without a drag picks nothing.
    s.click(a);
    assert_eq!(s.with(|e, _| ibis_office_pdf::editor::selected_text(e)), "");
    s.screenshot("pdf-issue235-selection");
}

/// 編集 > すべて選択 picks the text of every page; the copy has a line break between lines and a blank line between pages.
fn select_all_and_copy() {
    let mut s = open_text_pdf();
    s.menu("edit.select-all");
    s.menu("edit.copy");
    assert_eq!(clipboard(&mut s).as_deref(), Some("Hello PDF world\nSearch the Text here\n\nSecond page text"));
}

/// ⌘F opens the search bar; what is typed is looked for on all the pages without regard to case; Enter goes to the next place (and shows its
/// page), Shift+Enter to the previous one.
fn search() {
    let mut s = open_text_pdf();
    // The pointer is on the page (as when a person reads it): the keys go to the screen.
    let p = position(&mut s, 0, 3);
    s.click(p);
    s.key("cmd-f");
    assert_eq!(s.with(|e, _| ibis_office_pdf::editor::find_status(e)), Some((0, None)), "the bar is open");
    s.type_text("text");
    assert_eq!(s.with(|e, _| ibis_office_pdf::editor::find_status(e)), Some((2, None)), "two places, on two pages");
    s.key("enter");
    assert_eq!(s.with(|e, _| ibis_office_pdf::editor::find_status(e)), Some((2, Some(0))));
    s.key("enter");
    assert_eq!(s.with(|e, _| ibis_office_pdf::editor::find_status(e)), Some((2, Some(1))));
    assert!(s.status_texts()[0].contains("2 / 2"), "the second place is on the second page: {:?}", s.status_texts());
    s.key("shift-enter");
    assert_eq!(s.with(|e, _| ibis_office_pdf::editor::find_status(e)), Some((2, Some(0))));
    assert!(s.status_texts()[0].contains("1 / 2"), "{:?}", s.status_texts());
    s.screenshot("pdf-issue235-search");
    // Nothing found.
    s.type_text("zzz");
    assert_eq!(s.with(|e, _| ibis_office_pdf::editor::find_status(e)), Some((0, None)));
}

/// ジャンプ… takes the number of a page: the page is shown and the status bar says it.
fn go_to_page() {
    let mut s = open_text_pdf();
    assert!(s.status_texts()[0].contains("1 / 2"), "{:?}", s.status_texts());
    let p = position(&mut s, 0, 3);
    s.click(p);
    s.menu("edit.go-to");
    s.type_text("2");
    s.key("enter");
    assert!(s.status_texts()[0].contains("2 / 2"), "{:?}", s.status_texts());
    // The page before it, by the page keys.
    s.key("pageup");
    assert!(s.status_texts()[0].contains("1 / 2"), "{:?}", s.status_texts());
}
