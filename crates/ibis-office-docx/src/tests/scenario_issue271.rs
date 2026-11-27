// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #271 (a document of vertical and across writing mixed: the vertical text showed across). The report's form
//! (`yousiki15_r8.4.1-3.docx`) has twelve `w:textDirection w:val="tbRlV"`, every one in a table cell's `w:tcPr` (none in a section or a paragraph): narrow cells
//! (444 and 583 twips) that begin a vertical merge (`w:vMerge`), `w:noWrap`, `w:vAlign` centre, beside across cells. The tests build that shape and
//! check that the characters of a vertical cell are drawn down a column (the same x, y growing; ECMA-376 17.18.93 `ST_TextDirection`: `tbRlV` and
//! `tbRl` go down with the lines from the right, `tbLrV` down from the left, `btLr` up), and the across cell beside it is still across.

use super::flow_layout::{Item, Pagination};
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue271_a_tbrlv_cell_of_a_form_is_drawn_down_a_column_and_its_neighbour_across", form_cell_is_a_column),
        ("docx::issue271_each_cell_direction_runs_along_its_own_axis", each_direction_runs_along_its_axis),
        ("docx::issue271_a_vertical_cell_does_not_widen_its_column_by_its_text_length", vertical_cell_keeps_its_column),
        ("docx::issue271_a_vertical_cell_sets_its_lines_between_the_borders_middles", vertical_cell_lines_between_borders),
        ("docx::issue271_lines_thicker_than_a_vertical_cells_room_are_centred", thick_lines_are_centred),
        ("docx::issue271_a_rows_pitch_is_its_content_and_the_thickest_border_of_the_joint_below", row_pitch_is_content_and_joint),
    ]
}

fn make(name: &str, body: &str) -> String {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{w}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="ＭＳ 明朝" w:hAnsi="ＭＳ 明朝" w:eastAsia="ＭＳ 明朝" w:cs="ＭＳ 明朝"/><w:sz w:val="21"/><w:szCs w:val="21"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#);
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720" w:header="397" w:footer="397" w:gutter="0"/><w:docGrid w:type="lines" w:linePitch="360"/></w:sectPr></w:body></w:document>"#
    );
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/styles.xml", EntryMeta::new(), styles.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("issue271-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue271-{name}.docx")
}

fn pages_of(file: &str) -> std::sync::Arc<Pagination> {
    let mut s = Scenario::open(file);
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

/// Every drawn character of the first page: (x of its box's left edge, baseline y, the character).
fn drawn(p: &Pagination) -> Vec<(f32, f32, char)> {
    let mut v = Vec::new();
    for it in &p.pages[0].items {
        match it {
            Item::Text { x, y, text, offsets, .. } => v.extend(text.chars().zip(offsets.iter()).map(|(c, o)| (x + o, *y, c))),
            Item::Scaled { x, y, text, .. } | Item::Turned { x, y, text, .. } => v.extend(text.chars().map(|c| (*x, *y, c))),
            _ => {}
        }
    }
    v
}

/// The characters of `text` found in the drawing, in the order of `text`.
fn of(d: &[(f32, f32, char)], text: &str) -> Vec<(f32, f32)> {
    text.chars().map(|c| d.iter().find(|i| i.2 == c).map(|i| (i.0, i.1)).unwrap_or_else(|| panic!("{c} is not drawn: {d:?}"))).collect()
}

/// The report's shape: a 583 twips cell of `tbRlV` (`vMerge` restart, `noWrap`, `vAlign` centre) over a continuing one, an across cell beside them.
fn form_cell_is_a_column() {
    let cell = |w: u32, merge: &str, text: &str, direction: &str| {
        let para = if text.is_empty() { "<w:p/>".to_string() } else { format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>") };
        format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/>{merge}<w:noWrap/>{direction}<w:vAlign w:val="center"/></w:tcPr>{para}</w:tc>"#)
    };
    let vertical = r#"<w:textDirection w:val="tbRlV"/>"#;
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="9000" w:type="dxa"/><w:tblBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/><w:insideH w:val="single" w:sz="4"/><w:insideV w:val="single" w:sz="4"/></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="583"/><w:gridCol w:w="8417"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="1200"/></w:trPr>{}{}</w:tr><w:tr><w:trPr><w:trHeight w:val="1200"/></w:trPr>{}{}</w:tr></w:tbl><w:p/>"#,
        cell(583, r#"<w:vMerge w:val="restart"/>"#, "申請者届出情", vertical),
        cell(8417, "", "電子メールアドレス", ""),
        cell(583, "<w:vMerge/>", "", vertical),
        cell(8417, "", "施設の所在地", ""),
    );
    let d = drawn(&pages_of(&make("form", &body)));
    let col = of(&d, "申請者届出情");
    // Down a column: every character at the same x (the column's), the y growing by about a line each.
    for w in col.windows(2) {
        assert!((w[1].0 - w[0].0).abs() < 1.0, "one column: {col:?}");
        assert!(w[1].1 > w[0].1 + 5.0, "going down: {col:?}");
    }
    // The across cell: the characters of one line, the y the same, the x growing.
    let across = of(&d, "電子メールアドレス");
    for w in across.windows(2) {
        assert!((w[1].1 - w[0].1).abs() < 0.5, "one line: {across:?}");
        assert!(w[1].0 > w[0].0 + 5.0, "going right: {across:?}");
    }
    // The vertical column is inside its own narrow cell (583 twips = 29.15 pt, from the left margin 36 pt), left of the across cell.
    assert!(col.iter().all(|c| c.0 > 36.0 - 1.0 && c.0 < 36.0 + 29.15 + 1.0), "inside the narrow cell: {col:?}");
    assert!(across[0].0 > 36.0 + 29.15, "the across text is right of it: {across:?}");
}

/// One cell of 1000 x 3000 twips with four characters, in each direction: `tbRlV`, `tbRl` and `tbLrV` go down, `btLr` goes up.
fn each_direction_runs_along_its_axis() {
    for (direction, down) in [("tbRlV", true), ("tbRl", true), ("tbLrV", true), ("btLr", false)] {
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="1000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="1000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="3000"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="1000" w:type="dxa"/><w:textDirection w:val="{direction}"/></w:tcPr><w:p><w:r><w:t>あいうえ</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
        );
        let d = drawn(&pages_of(&make(direction, &body)));
        let col = of(&d, "あいうえ");
        for w in col.windows(2) {
            assert!((w[1].0 - w[0].0).abs() < 1.0, "{direction}: one column: {col:?}");
            let step = w[1].1 - w[0].1;
            assert!(if down { step > 5.0 } else { step < -5.0 }, "{direction}: goes {}: {col:?}", if down { "down" } else { "up" });
        }
    }
}

const CELL_MARGINS: &str = r#"<w:tblCellMar><w:left w:w="99" w:type="dxa"/><w:right w:w="99" w:type="dxa"/></w:tblCellMar>"#;

/// An autofit table (no `w:tblLayout`) as the report's second table is: a column of 444 twips (22.2 pt) holds a cell of vertical text of nine characters (`noWrap`, one
/// line of 12 pt exactly), beside a column of across text. Word keeps the 22.2 pt (its PDF: the next column's text 27.15 pt from the table's left, 22.2 + the margin
/// of 4.95 pt); the text's length is the cell's height, so it must not widen the column (it did, to 30 pt, and the rows' heights grew until a third page was needed).
fn vertical_cell_keeps_its_column() {
    let vertical = |w: u32, text: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/><w:noWrap/><w:textDirection w:val="tbRlV"/><w:vAlign w:val="center"/></w:tcPr><w:p><w:pPr><w:spacing w:line="240" w:lineRule="exact"/><w:jc w:val="center"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>"#);
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="8444" w:type="dxa"/>{CELL_MARGINS}</w:tblPr><w:tblGrid><w:gridCol w:w="444"/><w:gridCol w:w="8000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="2000"/></w:trPr>{}<w:tc><w:tcPr><w:tcW w:w="8000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>ABC</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#,
        vertical(444, "申請者・届出者情報"),
    );
    let d = drawn(&pages_of(&make("column", &body)));
    let col = of(&d, "申請者・届出者情報");
    let a = of(&d, "A")[0].0;
    // The column's glyphs are 10.5 pt wide: the centre of one is 5.25 pt from its box's left edge; the cell's middle is 11.1 pt from the table's left.
    let centre = col[0].0 + 5.25;
    let gap = a - centre;
    assert!((gap - (11.1 + 4.95)).abs() < 0.8, "the next column's text is 16.05 pt right of the vertical column's middle (it was 19.95 in a 30 pt column): {gap}");
}

/// A cell of the report's form (583 twips, margins of 99 twips, borders of 1.5 pt left and 3 pt right) with its lines top and bottom aligned: the lines stand between
/// the borders' middles (Word's PDF: the inner room is 36.72 to 63.62 pt, the cell's 36 to 65.15 less half of each border) after the margins, so the room is
/// 29.15 - 9.9 - 0.75 - 1.5 = 17.0 pt, and a line of 10 pt put at the top (right) and one at the bottom (left) have their boxes 7.0 pt apart (9.15 with the whole left border
/// taken and the right one not).
fn vertical_cell_lines_between_borders() {
    let cell = |anchor: &str| format!(
        r#"<w:tc><w:tcPr><w:tcW w:w="583" w:type="dxa"/><w:tcBorders><w:left w:val="single" w:sz="12" w:space="0" w:color="auto"/><w:right w:val="single" w:sz="24" w:space="0" w:color="auto"/></w:tcBorders><w:textDirection w:val="tbRlV"/><w:vAlign w:val="{anchor}"/></w:tcPr><w:p><w:pPr><w:spacing w:line="200" w:lineRule="exact"/></w:pPr><w:r><w:t>あいう</w:t></w:r></w:p></w:tc>"#
    );
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="583" w:type="dxa"/><w:tblLayout w:type="fixed"/>{CELL_MARGINS}</w:tblPr><w:tblGrid><w:gridCol w:w="583"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="1500"/></w:trPr>{}</w:tr></w:tbl><w:p/><w:tbl><w:tblPr><w:tblW w:w="583" w:type="dxa"/><w:tblLayout w:type="fixed"/>{CELL_MARGINS}</w:tblPr><w:tblGrid><w:gridCol w:w="583"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="1500"/></w:trPr>{}</w:tr></w:tbl><w:p/>"#,
        cell("top"),
        cell("bottom"),
    );
    let d = drawn(&pages_of(&make("borders", &body)));
    // The two tables hold the same text: every drawn "あ".
    let all: Vec<f32> = d.iter().filter(|i| i.2 == 'あ').map(|i| i.0).collect();
    assert_eq!(all.len(), 2, "{d:?}");
    let apart = (all[0] - all[1]).abs();
    assert!((apart - 7.0).abs() < 0.8, "top and bottom aligned lines are 7.0 pt apart: {apart}");
}

/// Lines of 10 pt that are thicker than the cell's room (17 pt) are centred on the room's middle, standing out on both sides, as one line is (Word's PDF of the form,
/// two lines: the columns' glyph centres are 55.0 and 45.2, their middle 50.1, the room's middle 50.17; they were put against the right edge, 2.0 pt off).
fn thick_lines_are_centred() {
    let cell = |paragraphs: &str| format!(
        r#"<w:tc><w:tcPr><w:tcW w:w="583" w:type="dxa"/><w:tcBorders><w:left w:val="single" w:sz="12" w:space="0" w:color="auto"/><w:right w:val="single" w:sz="24" w:space="0" w:color="auto"/></w:tcBorders><w:textDirection w:val="tbRlV"/><w:vAlign w:val="center"/></w:tcPr>{paragraphs}</w:tc>"#
    );
    let line = |t: &str| format!(r#"<w:p><w:pPr><w:spacing w:line="200" w:lineRule="exact"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let table = |c: String| format!(r#"<w:tbl><w:tblPr><w:tblW w:w="583" w:type="dxa"/><w:tblLayout w:type="fixed"/>{CELL_MARGINS}</w:tblPr><w:tblGrid><w:gridCol w:w="583"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="1500"/></w:trPr>{c}</w:tr></w:tbl>"#);
    let body = format!("{}<w:p/>{}<w:p/>", table(cell(&line("あ"))), table(cell(&(line("い") + &line("う") + &line("え")))));
    let d = drawn(&pages_of(&make("centred", &body)));
    let at = |c: char| d.iter().find(|i| i.2 == c).unwrap_or_else(|| panic!("{c}: {d:?}")).0;
    let one = at('あ');
    let three = (at('い') + at('え')) / 2.0;
    assert!((one - three).abs() < 0.6, "one line {one} and three lines {three} have the same middle");
}

/// Word 16 (Windows) PDFs of a 4-row table of 10.5 pt, cells of 0.5 pt borders: the pitch of a row (first line to first line) is its content and the
/// thickest border at the joint below it, whichever cell it is of: the bottom borders of the row and the top borders of the next, the cells of a merge's
/// continuation too. A 3 pt border on the one-line cell B beside a three-line cell (rows 2 and 3 top: pitches 43.94 / 43.80 / 41.40 against 41.42 / 41.28 / 41.40
/// thin; rows 2 and 3 bottom: 41.42 / 43.80 / 43.92; the merge's continuation of 3 pt: +2.5 on each pitch it touches).
fn row_pitch_is_content_and_joint() {
    let tc = |text: &str, merge: &str, top: u32, bottom: u32| {
        let para = if text.is_empty() { "<w:p/>".to_string() } else { format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>") };
        format!(r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/>{merge}<w:tcBorders><w:top w:val="single" w:sz="{top}"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="{bottom}"/><w:right w:val="single" w:sz="4"/></w:tcBorders></w:tcPr>{para}</w:tc>"#)
    };
    // `b`: the (top, bottom) of cell B in each of the four rows; cell A has the three-line text.
    let table = |name: &str, b: [(u32, u32); 4], a_merge: bool| {
        let rows: String = (0..4)
            .map(|i| {
                let a = if a_merge {
                    if i == 0 { tc("PP", r#"<w:vMerge w:val="restart"/>"#, 4, 4) } else { tc("", "<w:vMerge/>", b[i].0, b[i].1) }
                } else {
                    tc(["PP", "QQ", "RR", "SS"][i], "", 4, 4)
                };
                format!("<w:tr>{a}{}</w:tr>", tc(["bb", "cc", "dd", "ee"][i], "", if a_merge { 4 } else { b[i].0 }, if a_merge { 4 } else { b[i].1 }))
            })
            .collect();
        let body = format!(r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid>{rows}</w:tbl><w:p/>"#);
        drawn(&pages_of(&make(name, &body)))
    };
    let rows_y = |d: &[(f32, f32, char)]| -> Vec<f32> { ["b", "c", "d", "e"].iter().map(|t| of(d, t)[0].1).collect() };
    let pitch = |d: &[(f32, f32, char)]| -> Vec<f32> { let y = rows_y(d); y.windows(2).map(|w| w[1] - w[0]).collect() };
    let thin = [(4, 4); 4];
    let base = pitch(&table("joint-thin", thin, false));
    let near = |got: &[f32], want: [f32; 3], what: &str| {
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 0.3, "{what}: pitches {got:?}, expected {want:?} (thin {base:?})");
        }
    };
    let (u, v, w) = (base[0], base[1], base[2]);
    // A 3 pt top border on the cells of rows 2 and 3: the pitch to each of them is 2.5 pt more.
    near(&pitch(&table("joint-top", [(4, 4), (24, 4), (24, 4), (4, 4)], false)), [u + 2.5, v + 2.5, w], "top of rows 2 and 3");
    // The same as a bottom border of rows 1 and 2.
    near(&pitch(&table("joint-bottom", [(4, 24), (4, 24), (4, 4), (4, 4)], false)), [u + 2.5, v + 2.5, w], "bottom of rows 1 and 2");
    // The borders of a merge's continuation count (cell A of rows 2 to 4 continues row 1's).
    near(&pitch(&table("joint-merge", [(24, 24); 4], true)), [u + 2.5, v + 2.5, w + 2.5], "the continuation cells' borders");
}
