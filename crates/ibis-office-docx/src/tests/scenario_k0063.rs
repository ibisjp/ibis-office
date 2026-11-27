// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0063: the ページ設定 dialog in Word's four tabs (文字数と行数・余白・用紙・その他). One OK sets the direction, the orientation, the grid and
//! the columns together as one step of undo; the grid's counts and pitches follow each other; the sectPr's `w:textDirection`, `w:pgSz`
//! (`w:orient`), `w:docGrid` and `w:cols` are read, shown and written back.

use crate::ext::*;
use crate::page_setup::{count_in, grid_area};
use crate::shell::scenario::Scenario;
use ibis_doc_model::{CellDirection, DocGridType, EMU_PER_POINT};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::k0063_one_dialog_sets_vertical_landscape_and_the_grid_at_once", one_dialog_sets_everything),
        ("docx::k0063_the_grids_counts_and_pitches_follow_each_other", counts_and_pitches_follow),
        ("docx::k0063_a_sectpr_of_a_vertical_landscape_grid_document_is_shown_and_kept", sectpr_roundtrip),
        ("docx::k0063_genko_paper_style_sets_word_margins_grid_and_the_header_drawing", genko_paper),
    ]
}

const REPORT: &str = "test/editing/docx-report.docx";

fn tab(s: &mut Scenario, tab: usize) {
    s.update(|e, _, cx| {
        e.doc_state_mut().unwrap().helper.page_setup.as_mut().expect("the dialog").tab = tab;
        cx.notify();
    });
}

fn click(s: &mut Scenario, id: &str) {
    let id = id.to_string();
    let b = s.with(|e, _| *e.doc_state().unwrap().helper.page_setup.as_ref().expect("the dialog").ui.borrow().get(&id).unwrap_or_else(|| panic!("{id}")));
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
}

/// Type `value` in the grid field `which` (`chars`, `char-space`, `lines`, `line-pitch`).
fn type_in(s: &mut Scenario, which: &str, value: &str) {
    s.update(|e, window, cx| {
        let p = e.doc_state().unwrap().helper.page_setup.as_ref().expect("the dialog");
        let f = match which {
            "chars" => p.chars.clone(),
            "char-space" => p.char_space.clone(),
            "lines" => p.lines.clone(),
            _ => p.line_pitch.clone(),
        };
        f.update(cx, |i, cx| i.focus(window, cx));
    });
    s.key("cmd-a");
    s.type_text(value);
    s.settle();
}

fn field(s: &mut Scenario, which: &str) -> String {
    s.with(|e, cx| {
        let p = e.doc_state().unwrap().helper.page_setup.as_ref().expect("the dialog");
        let f = match which {
            "chars" => &p.chars,
            "char-space" => &p.char_space,
            "lines" => &p.lines,
            _ => &p.line_pitch,
        };
        f.read(cx).value().to_string()
    })
}

fn page(s: &mut Scenario) -> ibis_doc_model::PageSetup {
    s.with(|e, _| e.caret_page().expect("a page"))
}

fn base_pitch(s: &mut Scenario) -> i64 {
    s.with(|e, _| e.flow().unwrap().styles.paragraph_base(None, None).1.size.unwrap_or(1000) as i64 * (EMU_PER_POINT / 100))
}

/// The number of characters per line and of lines a page makes.
fn counts(page: &ibis_doc_model::PageSetup, base: i64) -> (i64, i64) {
    let (along, across) = grid_area(page.size.width, page.size.height, page.top, page.bottom, page.left, page.right, page.gutter, page.gutter_at_top, page.direction == CellDirection::TbRl);
    (count_in(along, base + page.char_space.unwrap_or(0)), count_in(across, page.line_pitch.unwrap_or(18 * EMU_PER_POINT)))
}

/// 縦書き, 横向き and 30 characters by 20 lines are set in the one dialog; one OK, one step of undo; saved and read back.
fn one_dialog_sets_everything() {
    let mut s = Scenario::open(REPORT);
    s.size(1200.0, 1000.0);
    let before = page(&mut s);
    let base = base_pitch(&mut s);
    s.menu("docx.page.doc-grid");
    // The grid tab is the first: the direction, the grid type; the orientation is in 余白. The paper first, then the counts (they follow the text area).
    click(&mut s, "direction-vertical");
    tab(&mut s, 1);
    click(&mut s, "orientation-landscape");
    tab(&mut s, 0);
    click(&mut s, "grid-lines-chars");
    type_in(&mut s, "chars", "30");
    type_in(&mut s, "lines", "20");
    click(&mut s, "apply");
    let after = page(&mut s);
    assert_eq!(after.direction, CellDirection::TbRl);
    assert!(after.size.width > after.size.height, "landscape: {:?}", after.size);
    assert_eq!(after.doc_grid_type, DocGridType::LinesAndChars);
    assert_eq!(counts(&after, base), (30, 20), "the pitches make the counts typed: {after:?}");
    // The paper is the portrait one turned (the same sheet).
    // (The dialog shows millimetres to a tenth: within 0.1 mm.)
    assert!((after.size.width - before.size.height).abs() <= 3_600 && (after.size.height - before.size.width).abs() <= 3_600, "{:?} of {:?}", after.size, before.size);

    // One step of undo takes it all back.
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    let undone = page(&mut s);
    assert_eq!((undone.direction, undone.size, undone.doc_grid_type), (before.direction, before.size, before.doc_grid_type));

    // Saved and read back (the dialog applied once more: redo).
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.save_as("k0063-vertical-landscape.docx");
    let saved = s.reopen();
    let saved_page = saved.flow.as_deref().expect("a document").page;
    assert_eq!(saved_page.direction, CellDirection::TbRl);
    assert!(saved_page.size.width > saved_page.size.height);
    assert_eq!(saved_page.doc_grid_type, DocGridType::LinesAndChars);
    assert_eq!(counts(&saved_page, base), (30, 20));
}

/// The number of characters (lines) and their pitch give each other: `pitch = area / count`, `count = floor(area / pitch)`.
fn counts_and_pitches_follow() {
    let mut s = Scenario::open(REPORT);
    s.size(1200.0, 1000.0);
    let p = page(&mut s);
    let (along, across) = grid_area(p.size.width, p.size.height, p.top, p.bottom, p.left, p.right, p.gutter, p.gutter_at_top, p.direction == CellDirection::TbRl);
    s.menu("docx.page.doc-grid");
    click(&mut s, "grid-lines-chars");
    // Characters: 25 per line gives the pitch area / 25.
    type_in(&mut s, "chars", "25");
    let pitch: f64 = field(&mut s, "char-space").parse().unwrap();
    let want = along as f64 / 25.0 / EMU_PER_POINT as f64;
    assert!((pitch - want).abs() < 0.006, "{pitch} vs {want}");
    // The pitch: 12 pt gives floor(area / 12 pt) characters, and the number just typed stays.
    type_in(&mut s, "char-space", "12");
    assert_eq!(field(&mut s, "char-space"), "12");
    assert_eq!(field(&mut s, "chars"), count_in(along, 12 * EMU_PER_POINT).to_string());
    // Lines the same way.
    type_in(&mut s, "lines", "30");
    let pitch: f64 = field(&mut s, "line-pitch").parse().unwrap();
    let want = across as f64 / 30.0 / EMU_PER_POINT as f64;
    assert!((pitch - want).abs() < 0.006, "{pitch} vs {want}");
    type_in(&mut s, "line-pitch", "24");
    assert_eq!(field(&mut s, "line-pitch"), "24");
    assert_eq!(field(&mut s, "lines"), count_in(across, 24 * EMU_PER_POINT).to_string());
    // 行数だけを指定する leaves the characters alone (their fields are off); 標準 shows no pitch changes at the page.
    click(&mut s, "grid-lines");
    click(&mut s, "apply");
    let applied = page(&mut s);
    assert_eq!((applied.doc_grid_type, applied.line_pitch, applied.char_space), (DocGridType::Lines, Some(24 * EMU_PER_POINT), None));
}

fn make(name: &str, sect: &str) -> String {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{w}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="ＭＳ 明朝" w:hAnsi="ＭＳ 明朝" w:eastAsia="ＭＳ 明朝" w:cs="ＭＳ 明朝"/><w:sz w:val="21"/><w:szCs w:val="21"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#);
    let document = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body><w:p><w:r><w:t>原稿用紙の試験</w:t></w:r></w:p>{sect}</w:body></w:document>"#);
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/styles.xml", EntryMeta::new(), styles.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("k0063-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/k0063-{name}.docx")
}

/// A document of a 縦書き, 横向き (`w:orient`), two-column section on a 原稿用紙 grid (`snapToChars`, 20 pt pitches): the dialog shows its numbers,
/// and OK with nothing changed writes the same back.
fn sectpr_roundtrip() {
    let file = make(
        "snap",
        r#"<w:sectPr><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/><w:pgMar w:top="1134" w:right="1134" w:bottom="1134" w:left="1134" w:header="851" w:footer="992" w:gutter="0"/><w:cols w:num="2" w:space="425"/><w:textDirection w:val="tbRl"/><w:docGrid w:type="snapToChars" w:linePitch="400" w:charSpace="-2048"/></w:sectPr>"#,
    );
    let mut s = Scenario::open(&file);
    s.size(1200.0, 1000.0);
    let p = page(&mut s);
    assert_eq!(p.direction, CellDirection::TbRl);
    assert_eq!((p.size.width, p.size.height), (16838 * 635, 11906 * 635));
    assert_eq!((p.doc_grid_type, p.line_pitch, p.columns.count), (DocGridType::SnapToChars, Some(400 * 635), 2));
    assert_eq!(p.char_space, Some(-2048 * 12_700 / 4096));
    let base = base_pitch(&mut s);
    s.menu("docx.page.doc-grid");
    // The dialog is on the grid tab, with the file's values.
    let (chars, lines) = counts(&p, base);
    assert_eq!(field(&mut s, "chars"), chars.to_string());
    assert_eq!(field(&mut s, "lines"), lines.to_string());
    assert_eq!(field(&mut s, "line-pitch"), "20");
    assert_eq!(s.with(|e, cx| e.doc_state().unwrap().helper.page_setup.as_ref().unwrap().columns.read(cx).value().to_string()), "2");
    click(&mut s, "apply");
    let after = page(&mut s);
    assert_eq!((after.direction, after.size, after.doc_grid_type, after.line_pitch, after.char_space, after.columns.count), (p.direction, p.size, p.doc_grid_type, p.line_pitch, p.char_space, 2));
    s.save_as("k0063-snap.docx");
    let saved = s.reopen();
    let q = saved.flow.as_deref().expect("a document").page;
    assert_eq!((q.direction, q.size, q.doc_grid_type, q.line_pitch, q.char_space, q.columns.count), (p.direction, p.size, p.doc_grid_type, p.line_pitch, p.char_space, 2));
}

/// 原稿用紙のスタイル (マス目付き) in the dialog's first tab: Word's margins (1440 twips above and below, 1700 at the sides), 20 × 20, the
/// `snapToChars` grid with Word's pitches (`linePitch` = the whole twips of the text area's share, `charSpace` the share less the characters'
/// size), 縦書き across the paper; one step of undo; saved with the drawing in the header and read back.
fn genko_paper() {
    use ibis_doc_model::GenkoStyle;
    let mut s = Scenario::open(REPORT);
    s.size(1200.0, 1000.0);
    let before = page(&mut s);
    let base = base_pitch(&mut s);
    s.menu("docx.page.doc-grid");
    click(&mut s, "direction-vertical");
    click(&mut s, "genko-grid");
    click(&mut s, "genko-color-red");
    click(&mut s, "apply");
    let after = page(&mut s);
    assert_eq!((after.top, after.bottom, after.left, after.right), (1440 * 635, 1440 * 635, 1700 * 635, 1700 * 635));
    assert!(after.size.width > after.size.height, "a 縦書き 原稿用紙 lies across the paper");
    assert_eq!((after.direction, after.doc_grid_type), (CellDirection::TbRl, DocGridType::SnapToChars));
    // 縦書き: the lines across the width, the characters down the height.
    let (along, across) = grid_area(after.size.width, after.size.height, after.top, after.bottom, after.left, after.right, after.gutter, after.gutter_at_top, true);
    assert_eq!(after.line_pitch, Some(across / 635 / 20 * 635));
    let units = ((along as f64 / 20.0 - base as f64) * 4096.0 / EMU_PER_POINT as f64).round() as i64;
    assert_eq!(after.char_space, Some(units * EMU_PER_POINT / 4096));
    let genko = s.with(|e, _| e.flow().unwrap().genko.clone()).expect("the 原稿用紙");
    assert_eq!((genko.style, genko.chars, genko.lines, genko.color), (GenkoStyle::Grid, 20, 20, ibis_doc_model::Rgb(0xFF0000)));
    // One step of undo.
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    assert!(s.with(|e, _| e.flow().unwrap().genko.is_none()));
    let undone = page(&mut s);
    assert_eq!((undone.top, undone.size, undone.doc_grid_type), (before.top, before.size, before.doc_grid_type));
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    // Saved and read back.
    s.save_as("k0063-genko.docx");
    let saved = s.reopen();
    let doc = saved.flow.as_deref().expect("a document");
    assert_eq!(doc.genko.as_ref().map(|g| (g.style, g.chars, g.lines)), Some((GenkoStyle::Grid, 20, 20)));
    assert_eq!(counts(&doc.page, base), (20, 20));
}
