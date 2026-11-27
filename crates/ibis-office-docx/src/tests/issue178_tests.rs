// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #178 (a table's width, margins, border position and the break of a row at a page's end differ from Word's), the document (DOCX)
//! side. Each test lays out a table of one row (MS Mincho 10.5 pt, A4 with 72 pt margins) and measures the vertical lines and the first
//! character's x; the values are the issue's measurements of Word's PDFs (tolerance 0.25 pt for a line or a character, 1 pt for a width). The documents carry the
//! styles.xml Word writes (its default table style gives the cells their 108 twips side margins: #227).

use super::flow_layout::{Item, Pagination};
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue178_cell_margin_types", cell_margin_types),
        ("docx::issue178_fixed_table_widths", fixed_table_widths),
        ("docx::issue178_autofit_cell_widths", autofit_cell_widths),
        ("docx::issue178_compat14_tables_reach_out_over_their_margins", compat14_tables_reach_out_over_their_margins),
        ("docx::issue178_compat14_left_line_is_on_the_cell_edge_whatever_its_width", compat14_left_line_is_on_the_cell_edge_whatever_its_width),
        ("docx::issue178_borders_and_text_from_the_middle_of_the_line", borders_and_text_from_the_middle_of_the_line),
        ("docx::issue178_rows_divide_whatever_the_vertical_alignment", rows_divide_whatever_the_vertical_alignment),
        ("docx::issue178_a_centred_cell_beside_a_taller_one_stays_in_the_first_part", a_centred_cell_beside_a_taller_one_stays_in_the_first_part),
        ("docx::issue178_a_width_of_the_texts_has_the_end_borders_inside", a_width_of_the_texts_has_the_end_borders_inside),
        ("docx::issue178_nested_table_percent_is_of_the_cells_text", nested_table_percent_is_of_the_cells_text),
        ("docx::issue178_fixed_table_widened_by_tcw_and_cut_inside_the_margins", fixed_table_widened_by_tcw_and_cut_inside_the_margins),
    ]
}

/// MS Mincho (the typeface's own advances whatever is installed), two hiragana and one.
const MINCHO: &str = "\u{ff2d}\u{ff33} \u{660e}\u{671d}";
const KANA2: &str = "\u{3042}\u{3044}";
const KANA1: &str = "\u{3042}";

const BORDERS: &str = r#"<w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:left w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:right w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="000000"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="000000"/></w:tblBorders>"#;

/// A document of `body` (the XML of its blocks) in the compatibility mode `mode`; the path of the file.
fn make(name: &str, mode: u32, body: &str) -> String {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let settings = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{w}"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat></w:settings>"#);
    // The styles every file Word writes has: its Normal paragraph style and the default table style, 標準の表 (Normal Table), whose cell margins are 108 twips. A table's cells
    // have margins only where the file has a default table style (#227: Windows Word's XPS of one-factor tests, `tasks/doc/requests/2026-11-18-227-margin-s1-result.md`); the
    // measurements of this issue are of Word's own files, which have it.
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{w}"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style><w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:uiPriority w:val="99"/><w:semiHidden/><w:unhideWhenUsed/><w:tblPr><w:tblInd w:w="0" w:type="dxa"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style></w:styles>"#);
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body>{body}<w:p/><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/settings.xml", EntryMeta::new(), settings.as_bytes()).unwrap();
    z.add("word/styles.xml", EntryMeta::new(), styles.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("issue178-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue178-{name}.docx")
}

/// A paragraph of the text, MS Mincho 10.5 pt.
fn para(text: &str) -> String {
    format!(r#"<w:p><w:r><w:rPr><w:rFonts w:ascii="{MINCHO}" w:hAnsi="{MINCHO}" w:eastAsia="{MINCHO}"/><w:sz w:val="21"/></w:rPr><w:t>{text}</w:t></w:r></w:p>"#,
    )
}

/// A one row table: `tbl_pr` its properties, `grid` the grid's columns (twips), `tc_prs` each cell's properties.
fn table(tbl_pr: &str, grid: &[u32], tc_prs: &[&str]) -> String {
    let cols: String = grid.iter().map(|g| format!(r#"<w:gridCol w:w="{g}"/>"#)).collect();
    let cells: String = tc_prs.iter().map(|p| format!("<w:tc><w:tcPr>{p}</w:tcPr>{}</w:tc>", para(KANA2))).collect();
    format!("<w:tbl><w:tblPr>{tbl_pr}</w:tblPr><w:tblGrid>{cols}</w:tblGrid><w:tr>{cells}</w:tr></w:tbl>")
}

fn pages_of(file: &str) -> std::sync::Arc<Pagination> {
    let mut s = Scenario::open(file);
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

/// The middles of the page's vertical lines (thin rectangles taller than wide), sorted, the ones of one x once.
fn verticals(p: &Pagination) -> Vec<f32> {
    let mut xs: Vec<f32> = p.pages[0].items.iter().filter_map(|it| match it {
        Item::Rect { x, w, h, .. } if *w <= 3.5 && *h > 5.0 && *h > *w * 2.0 => Some(x + w / 2.0),
        _ => None,
    }).collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    xs
}

/// The x of the first character of the table's text.
fn text_x(p: &Pagination) -> f32 {
    p.pages[0].items.iter().filter_map(|it| if let Item::Text { x, text, .. } = it { text.starts_with(KANA2).then_some(*x) } else { None }).fold(f32::MAX, f32::min)
}

fn near(a: f32, b: f32, tol: f32, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} against {b}");
}

fn widths(xs: &[f32]) -> Vec<f32> {
    xs.windows(2).map(|w| w[1] - w[0]).collect()
}

/// Item 1: a margin in `pct` is 0, `nil` and `auto` leave the table's (5.4 pt), a margin without a type is twips.
fn cell_margin_types() {
    let pct = r#"<w:tblCellMar><w:top w:w="250" w:type="pct"/><w:left w:w="250" w:type="pct"/><w:bottom w:w="250" w:type="pct"/><w:right w:w="250" w:type="pct"/></w:tblCellMar>"#;
    let fixed = r#"<w:tblLayout w:type="fixed"/>"#;
    let p = pages_of(&make("margin-pct", 15, &table(&format!(r#"<w:tblW w:w="6000" w:type="dxa"/>{fixed}{pct}"#), &[3000, 3000], &["", ""])));
    near(text_x(&p), 72.0, 0.25, "pct margins are 0");
    for ty in ["nil", "auto"] {
        let tc = format!(r#"<w:tcMar><w:left w:w="0" w:type="{ty}"/></w:tcMar>"#);
        let p = pages_of(&make(&format!("margin-{ty}"), 15, &table(&format!(r#"<w:tblW w:w="6000" w:type="dxa"/>{fixed}"#), &[3000, 3000], &[&tc, ""])));
        near(text_x(&p), 77.4, 0.25, &format!("a {ty} margin leaves the default"));
    }
    let tc = r#"<w:tcMar><w:left w:w="200" w:type="dxa"/></w:tcMar>"#;
    let p = pages_of(&make("margin-dxa", 15, &table(&format!(r#"<w:tblW w:w="6000" w:type="dxa"/>{fixed}"#), &[3000, 3000], &[tc, ""])));
    near(text_x(&p), 82.0, 0.25, "twips");
}

/// Item 2: a fixed table takes `w:tcW` over a grid that disagrees, and `w:tblW` stretches or cuts the columns in proportion (in percent: of the text's width).
fn fixed_table_widths() {
    let fixed = r#"<w:tblLayout w:type="fixed"/>"#;
    let cell = |w: u32| format!(r#"<w:tcW w:w="{w}" w:type="dxa"/>"#);
    let t = |tbl_w: &str, tcs: [u32; 3]| table(&format!("{tbl_w}{fixed}{BORDERS}"), &[2400, 2400, 2400], &[&cell(tcs[0]), &cell(tcs[1]), &cell(tcs[2])]);
    // tblW 210 pt, grid and cells 120 pt each: 70 pt each.
    let w = widths(&verticals(&pages_of(&make("fixed-a", 15, &t(r#"<w:tblW w:w="4200" w:type="dxa"/>"#, [2400; 3])))));
    assert_eq!(w.len(), 3, "{w:?}");
    for c in &w {
        near(*c, 70.0, 0.3, "tblW cuts the columns");
    }
    // No tblW, cells 90, 150 and 120 pt: the cells'.
    let w = widths(&verticals(&pages_of(&make("fixed-b", 15, &t("", [1800, 3000, 2400])))));
    for (c, want) in w.iter().zip([90.0, 150.0, 120.0]) {
        near(*c, want, 0.3, "tcW over a grid that disagrees");
    }
    // tblW 100 %: the text's width (451.3 pt; Word's PDF 450.9).
    let xs = verticals(&pages_of(&make("fixed-c", 15, &t(r#"<w:tblW w:w="5000" w:type="pct"/>"#, [2400; 3]))));
    near(xs[xs.len() - 1] - xs[0], 451.3, 1.0, "100 % of the text");
}

/// Item 3: a cell width in percent is of the grid's total when the table has no width; `nil` is the grid's width (`auto` the content's).
fn autofit_cell_widths() {
    let t = |tcs: &[&str]| table(BORDERS, &[2400, 4800], tcs);
    let w = widths(&verticals(&pages_of(&make("auto-a", 15, &t(&[r#"<w:tcW w:w="2400" w:type="dxa"/>"#, r#"<w:tcW w:w="3333" w:type="pct"/>"#])))));
    near(w[0], 120.0, 0.5, "dxa");
    near(w[1], 240.0, 0.5, "66.7 % of the grid's 360 pt");
    let w = widths(&verticals(&pages_of(&make("auto-b", 15, &t(&[r#"<w:tcW w:w="0" w:type="nil"/>"#, r#"<w:tcW w:w="4800" w:type="dxa"/>"#])))));
    near(w[0], 120.0, 0.5, "nil is the grid's width");
    near(w[1], 240.0, 0.5, "dxa");
}

/// Item 4: before Word 2013's mode (15) the table starts a margin out to the left, and a table of the text's width is wider by both margins.
fn compat14_tables_reach_out_over_their_margins() {
    let t = |extra: &str, tc: &str| table(&format!("{extra}{BORDERS}"), &[2400, 2400], &[tc, ""]);
    let full = r#"<w:tblW w:w="5000" w:type="pct"/>"#;
    let xs = verticals(&pages_of(&make("compat14-a", 14, &t(full, ""))));
    near(xs[xs.len() - 1] - xs[0], 451.3 + 10.8, 1.0, "100 % in mode 14: the text's width and both margins");
    near(xs[0], 66.6, 0.6, "a margin out to the left");
    let xs = verticals(&pages_of(&make("compat15-a", 15, &t(full, ""))));
    near(xs[xs.len() - 1] - xs[0], 451.3, 1.0, "100 % in mode 15");
    near(xs[0], 72.0, 0.6, "mode 15: at the text's edge");
    // A left margin of type nil is the table's 5.4 pt: out too.
    let xs = verticals(&pages_of(&make("compat14-c", 14, &t("", r#"<w:tcMar><w:left w:w="0" w:type="nil"/></w:tcMar>"#))));
    near(xs[0], 66.6, 0.6, "nil margin in mode 14");
}

/// Item 5: the outer edge of the left border is at the text's edge (its middle half the border in) and the text a margin from that middle.
fn borders_and_text_from_the_middle_of_the_line() {
    for (sz, width) in [(24u32, 3.0f32), (8, 1.0), (4, 0.5)] {
        let borders = BORDERS.replace(r#"w:sz="4""#, &format!(r#"w:sz="{sz}""#));
        let tbl = table(&format!(r#"<w:tblLayout w:type="fixed"/><w:tblW w:w="6000" w:type="dxa"/>{borders}"#), &[3000, 3000], &["", ""]);
        let p = pages_of(&make(&format!("border-{sz}"), 15, &tbl));
        let xs = verticals(&p);
        near(xs[0], 72.0 + width / 2.0, 0.25, &format!("the left line of {width} pt"));
        near(text_x(&p), xs[0] + 5.4, 0.25, &format!("the text beside a line of {width} pt"));
    }
}

/// Item 6: a row of centred or bottom aligned cells divides at the page's end as one of top aligned cells does.
fn rows_divide_whatever_the_vertical_alignment() {
    let filler = r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="400" w:lineRule="exact"/></w:pPr></w:p>"#.repeat(31);
    let rows_on_first_page = |name: &str, valign: &str| -> (usize, usize) {
        let lines: String = (0..8).map(|_| para(KANA1)).collect();
        let tbl = format!(r#"<w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/>{valign}</w:tcPr>{lines}</w:tc></w:tr></w:tbl>"#);
        let p = pages_of(&make(name, 15, &format!("{filler}{tbl}")));
        let count = |n: usize| p.pages.get(n).map_or(0, |pg| pg.items.iter().filter(|it| matches!(it, Item::Text { text, .. } if text.starts_with(KANA1))).count());
        (count(0), count(1))
    };
    let top = rows_on_first_page("split-top", "");
    assert!(top.0 > 0 && top.1 > 0 && top.0 + top.1 == 8, "a top aligned row is divided: {top:?}");
    for (name, v) in [("split-center", r#"<w:vAlign w:val="center"/>"#), ("split-bottom", r#"<w:vAlign w:val="bottom"/>"#)] {
        let got = rows_on_first_page(name, v);
        assert!(got.0 > 0 && got.1 > 0 && got.0 + got.1 == 8, "{name}: the row is divided: {got:?}");
    }
}

/// A row of a cell of 8 lines and a cell of one line centred, at a page's end (Word's PDF of `n1-table-widths.docx`, GitHub issue #178 T6a: three lines of the tall cell in the
/// first part, the centred cell's line centred in them (baseline 740.55 beside 720.5, 740.5, 760.5), the rest of the tall cell's lines on the next page): the row is divided by where the
/// lines fall from the cells' tops, and each part's text is aligned in the part's own height.
fn a_centred_cell_beside_a_taller_one_stays_in_the_first_part() {
    let filler = r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="400" w:lineRule="exact"/></w:pPr></w:p>"#.repeat(31);
    let line = |t: &str| format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="400" w:lineRule="exact"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="{MINCHO}" w:hAnsi="{MINCHO}" w:eastAsia="{MINCHO}"/><w:sz w:val="21"/></w:rPr><w:t>{t}</w:t></w:r></w:p>"#);
    let tall: String = (0..8).map(|_| line(KANA1)).collect();
    let tbl = format!(r#"<w:tbl><w:tblPr><w:tblW w:w="7200" w:type="dxa"/>{BORDERS}<w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3600"/><w:gridCol w:w="3600"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3600" w:type="dxa"/><w:vAlign w:val="center"/></w:tcPr>{tall}</w:tc><w:tc><w:tcPr><w:tcW w:w="3600" w:type="dxa"/><w:vAlign w:val="center"/></w:tcPr>{}</w:tc></w:tr></w:tbl>"#, line(KANA2));
    let p = pages_of(&make("split-two-cells", 15, &format!("{filler}{tbl}")));
    let ys = |n: usize, text: &str| -> Vec<f32> { p.pages.get(n).map_or(vec![], |pg| pg.items.iter().filter_map(|it| match it { Item::Text { y, text: t, .. } if &**t == text => Some(*y), _ => None }).collect()) };
    let (first, rest) = (ys(0, KANA1), ys(1, KANA1));
    assert!(!first.is_empty() && !rest.is_empty() && first.len() + rest.len() == 8, "the tall cell is divided: {} and {}", first.len(), rest.len());
    let side = ys(0, KANA2);
    assert_eq!(side.len(), 1, "the one line of the centred cell is in the first part: {side:?}; the next page has {:?}", ys(1, KANA2));
    assert!(ys(1, KANA2).is_empty(), "and not in the second");
    // Centred in the lines beside it: the middle of the first part's lines.
    let middle = (first[0] + first[first.len() - 1]) / 2.0;
    near(side[0], middle, 0.5, "the centred line is in the middle of the part");
}

/// A table of the text's width (`w:tblW` 100 % or auto, wide cells) in mode 15 has its end borders inside that width, half of each (Word's PDF of `n1-table-widths.docx`, #178 T4a and
/// T4b, borders of 0.5 pt, text of 451.3 pt: the lines' middles at 72.26, 297.65, 523.18 and 72.26, 491.62, 523.18; within 0.25 pt).
fn a_width_of_the_texts_has_the_end_borders_inside() {
    let tbl = |w: &str, text: &str| format!(r#"<w:tbl><w:tblPr><w:tblW {w}/>{BORDERS}</w:tblPr><w:tblGrid><w:gridCol w:w="4513"/><w:gridCol w:w="4513"/></w:tblGrid><w:tr><w:tc><w:tcPr></w:tcPr>{}</w:tc><w:tc><w:tcPr></w:tcPr>{}</w:tc></w:tr></w:tbl>"#, para(text), para(KANA2));
    let a = verticals(&pages_of(&make("autofit-pct", 15, &tbl(r#"w:w="5000" w:type="pct""#, KANA2))));
    assert_eq!(a.len(), 3, "{a:?}");
    near(a[0], 72.26, 0.25, "T4a: the left line");
    near(a[1], 297.65, 0.25, "T4a: the middle line");
    near(a[2], 523.18, 0.25, "T4a: the right line");
    let long = "あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわをんあいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわをんあいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめも";
    let b = verticals(&pages_of(&make("autofit-auto", 15, &tbl(r#"w:w="0" w:type="auto""#, long))));
    assert_eq!(b.len(), 3, "{b:?}");
    near(b[2], 523.18, 0.25, "T4b: the right line");
}

/// A nested table's percent is of the outer cell's width less its margins, in every compatibility mode, and it does not reach out to the left
/// (Word's XPS, MS Mincho 10.5 pt: outer cells of 200 pt, margins 5.4 pt, a nested table of two columns of 50 pt in a tblW of 100 % or 50 %;
/// the lines' middles are of a width of 189.2 pt or 94.6 pt from the outer cell's text, 77.9 in mode 15 and 72.3 in mode 14).
fn nested_table_percent_is_of_the_cells_text() {
    let nested = |pct: u32| format!(r#"<w:tbl><w:tblPr><w:tblW w:w="{pct}" w:type="pct"/>{BORDERS}</w:tblPr><w:tblGrid><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/></w:tblGrid><w:tr><w:tc><w:tcPr/>{}</w:tc><w:tc><w:tcPr/>{}</w:tc></w:tr></w:tbl><w:p/>"#, para(KANA2), para(KANA2));
    for (mode, left) in [(15u32, 77.904f32), (14, 72.264)] {
        for (pct, width) in [(5000u32, 189.21f32), (2500, 94.8)] {
            let outer = format!(r#"<w:tbl><w:tblPr><w:tblW w:w="8000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr>{}</w:tc><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>"#, nested(pct));
            let xs = verticals(&pages_of(&make(&format!("nested-{mode}-{pct}"), mode, &outer)));
            assert_eq!(xs.len(), 3, "mode {mode} {pct}: {xs:?}");
            near(xs[0], left, 0.3, &format!("mode {mode} {pct}: the left line"));
            near(xs[2] - xs[0], width - 0.48, 1.0, &format!("mode {mode} {pct}: the width"));
            near(xs[1] - xs[0], (width - 0.48) / 2.0, 0.6, &format!("mode {mode} {pct}: the middle line"));
        }
    }
}

/// A fixed table's `w:tcW` (a gridSpan cell's is its whole width) over a grid that disagrees, `w:tblW` in twips: more than the cells make them
/// wider in proportion to the cells' widths, less cuts what is inside the margins (Word's XPS, grid 50 pt three times, margins 5.4 pt: widths of
/// the lines' middles, to 0.6 pt).
fn fixed_table_widened_by_tcw_and_cut_inside_the_margins() {
    let fixed = r#"<w:tblLayout w:type="fixed"/>"#;
    let cell = |span: u32, w: u32| format!(r#"<w:tcW w:w="{w}" w:type="dxa"/>{}"#, if span > 1 { format!(r#"<w:gridSpan w:val="{span}"/>"# ) } else { String::new() });
    let case = |name: &str, tbl_w: u32, grid: &[u32], cells: &[(u32, u32)], want: &[f32]| {
        let tcs: Vec<String> = cells.iter().map(|(s, w)| cell(*s, *w)).collect();
        let tcs: Vec<&str> = tcs.iter().map(String::as_str).collect();
        let tbl = table(&format!(r#"<w:tblW w:w="{tbl_w}" w:type="dxa"/>{fixed}{BORDERS}"#), grid, &tcs);
        let w = widths(&verticals(&pages_of(&make(name, 15, &tbl))));
        assert_eq!(w.len(), want.len(), "{name}: {w:?}");
        for (got, want) in w.iter().zip(want) {
            near(*got, *want, 0.6, &format!("{name}: {w:?}"));
        }
    };
    case("tcw-a", 6000, &[1000; 3], &[(2, 2000), (1, 1000)], &[200.0, 100.0]);
    case("tcw-b", 6000, &[1000; 3], &[(2, 1500), (1, 1000)], &[180.0, 120.0]);
    case("tcw-c", 3000, &[1000; 3], &[(2, 2000), (1, 1000)], &[100.0, 50.0]);
    case("tcw-d", 3000, &[1000; 3], &[(2, 1000), (1, 2000)], &[50.0, 100.0]);
    case("tcw-e", 2000, &[1000; 3], &[(2, 2000), (1, 1000)], &[65.16, 34.80]);
    case("tcw-f", 1500, &[1000; 3], &[(2, 2000), (1, 1000)], &[47.64, 27.36]);
    case("tcw-g", 3000, &[1000; 3], &[(2, 3000), (1, 1500)], &[98.52, 51.51]);
    case("tcw-h", 2000, &[2000, 1000], &[(1, 2000), (1, 1000)], &[65.16, 34.80]);
    case("tcw-i", 2400, &[2000, 1000], &[(1, 2000), (1, 1000)], &[79.08, 40.95]);
    case("tcw-j", 6000, &[2000, 1000], &[(1, 2000), (1, 1000)], &[200.0, 100.0]);
    case("tcw-k", 3000, &[1500, 1500], &[(1, 2000), (1, 1000)], &[100.0, 50.0]);
}

/// Before mode 15 the left line's middle is on the cell's edge (the text's edge less the margin), as thick as the line is (Word's PDFs, 0.5/1.5/3 pt: 66.62, 66.62, 66.56).
fn compat14_left_line_is_on_the_cell_edge_whatever_its_width() {
    for sz in [4u32, 12, 24] {
        let borders = BORDERS.replace(r#"w:sz="4""#, &format!(r#"w:sz="{sz}""#));
        let xs = verticals(&pages_of(&make(&format!("compat14-border-{sz}"), 14, &table(&format!(r#"<w:tblW w:w="6000" w:type="dxa"/>{borders}"#), &[3000, 3000], &["", ""]))));
        near(xs[0], 66.6, 0.15, &format!("the left line of sz {sz}"));
    }
}
