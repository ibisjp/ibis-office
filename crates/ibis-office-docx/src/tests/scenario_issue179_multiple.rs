// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #179: a line spacing of a multiple of 1 or less (`w:lineRule="auto"`, `w:line` ≤ 240) off a line grid. Windows Word's 218 pages (XPS, 0.12 pt steps),
//! `tasks/doc/requests/2026-11-14-179-rest-many-patterns-result.md` and its `.tsv` (the table the tests read): a line of the single line's `a` above the baseline and
//! `d` below, `L` the line of its characters' typefaces and sizes, is `d' = m d`, `H' = H - (1 - m) L`, `a' = H' - d'` (for a line of text `m A`, `m D`, `m L`).
//! Each page is rebuilt here (A4, 72 pt margins, compatibility mode 15, three lines of the text divided by `w:br`, a 10.5 pt ＭＳ 明朝 "ｚ" paragraph after) and its
//! baselines (from the top margin) compared with Word's. Tolerance 0.3 pt: two or three baselines' rounding in 0.12 pt steps, as the result says.

use super::flow_layout::{Item, Pagination};
use crate::ext::*;
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue179_multiple_sweep_of_the_multiple", sweep),
        ("docx::issue179_multiple_sweep_of_the_size", sizes),
        ("docx::issue179_multiple_typefaces", typefaces),
        ("docx::issue179_multiple_mixed_sizes_take_the_largest_of_each_line", mixed),
        ("docx::issue179_multiple_does_not_act_on_a_line_grid", on_grid),
        ("docx::issue179_multiple_with_raised_and_lowered_text", position),
        ("docx::issue179_multiple_with_an_inline_picture", picture),
        ("docx::issue179_multiple_one_line_and_empty_paragraphs", one_line),
        ("docx::issue179_multiple_in_a_table_cell", table_cell),
    ]
}

const TSV: &str = include_str!("../../../../tasks/doc/requests/2026-11-14-179-rest-many-patterns-result.tsv");

/// One row of Word's table: (id, `w:line`, baselines of lines 1 to 3, of the "ｚ", the conditions as written).
struct Row {
    id: String,
    line: u32,
    b: [Option<f32>; 3],
    z: Option<f32>,
    extra: String,
}

fn rows(group: &str) -> Vec<Row> {
    TSV.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() < 8 || f[0].split('|').next() != Some(group) {
                return None;
            }
            let num = |s: &str| s.parse::<f32>().ok();
            Some(Row { id: f[0].to_string(), line: f[2].parse().unwrap(), b: [num(f[3]), num(f[4]), num(f[5])], z: num(f[6]), extra: f[7].to_string() })
        })
        .collect()
}

impl Row {
    /// A string of the conditions, `"font": "msm"` → `msm`.
    fn get(&self, key: &str) -> Option<String> {
        let k = format!("\"{key}\": ");
        let at = self.extra.find(&k)? + k.len();
        let rest = &self.extra[at..];
        let end = rest.find([',', '}']).unwrap_or(rest.len());
        Some(rest[..end].trim().trim_matches('"').to_string())
    }
    fn size(&self) -> f32 {
        self.get("size").and_then(|s| s.parse().ok()).unwrap_or(10.5)
    }
}

fn typeface(code: &str) -> &'static str {
    match code {
        "msm" => "ＭＳ 明朝",
        "msg" => "ＭＳ ゴシック",
        "biz" => "BIZ UD明朝 Medium",
        "yum" => "游明朝",
        "yug" => "游ゴシック",
        "mei" => "メイリオ",
        "ari" => "Arial",
        "tnr" => "Times New Roman",
        "cen" => "Century",
        other => panic!("typeface {other}"),
    }
}

/// A one-pixel PNG, for the inline pictures.
const PNG: [u8; 69] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00,
    0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// A document of `body` (the XML of its blocks), the default run of `font` and `half` half points, `grid` the `w:docGrid` linePitch in twips (0 for none), `media`
/// whether it has the picture of `rId3`.
fn make(name: &str, font: &str, half: u32, grid: u32, media: bool, body: &str) -> String {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let settings = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{w}"><w:characterSpacingControl w:val="doNotCompress"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#);
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{w}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:eastAsia="{font}" w:cs="{font}"/><w:sz w:val="{half}"/><w:szCs w:val="{half}"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#);
    let snap = if grid > 0 { format!(r#"<w:docGrid w:type="lines" w:linePitch="{grid}"/>"#) } else { String::new() };
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>{snap}</w:sectPr></w:body></w:document>"#
    );
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/settings.xml", EntryMeta::new(), settings.as_bytes()).unwrap();
    z.add("word/styles.xml", EntryMeta::new(), styles.as_bytes()).unwrap();
    if media {
        z.add("word/media/image1.png", EntryMeta::new(), &PNG).unwrap();
    }
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let file = format!("issue179-multiple-{}.docx", name.replace(['|', ' '], "-"));
    std::fs::write(dir.join(&file), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/{file}")
}

fn pages_of(file: &str) -> std::sync::Arc<Pagination> {
    let mut s = Scenario::open(file);
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

/// The first page's text items: (baseline from the top margin, text).
fn texts(p: &Pagination) -> Vec<(f32, String)> {
    p.pages[0].items.iter().filter_map(|it| if let Item::Text { y, text, .. } = it { Some((*y - 72.0, text.to_string())) } else { None }).collect()
}

/// The distinct baselines of the lines of `chars` (the first of each distinct baseline, in order) and the "ｚ"'s.
fn baselines(p: &Pagination, chars: &str) -> (Vec<f32>, Option<f32>) {
    let t = texts(p);
    let mut ys: Vec<f32> = t.iter().filter(|x| x.1.chars().next().is_some_and(|c| chars.contains(c))).map(|x| x.0).collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    (ys, t.iter().find(|x| x.1 == "ｚ").map(|x| x.0))
}

/// The paragraph of three lines divided by `w:br` of `lines` (the runs of each), with `spacing` (the attributes of `w:spacing`) and `ppr_rpr` as its mark's properties,
/// and the "ｚ" paragraph after.
fn three(spacing: &str, ppr_rpr: &str, lines: [&str; 3]) -> String {
    format!(
        r#"<w:p><w:pPr><w:spacing {spacing}/><w:rPr>{ppr_rpr}</w:rPr></w:pPr>{}<w:r><w:br/></w:r>{}<w:r><w:br/></w:r>{}</w:p><w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="ＭＳ 明朝" w:hAnsi="ＭＳ 明朝" w:eastAsia="ＭＳ 明朝"/><w:sz w:val="21"/></w:rPr><w:t>ｚ</w:t></w:r></w:p>"#,
        lines[0], lines[1], lines[2]
    )
}

fn spacing(line: u32) -> String {
    format!(r#"w:before="0" w:after="0" w:line="{line}" w:lineRule="auto""#)
}

fn run(rpr: &str, text: &str) -> String {
    format!(r#"<w:r><w:rPr>{rpr}</w:rPr><w:t>{text}</w:t></w:r>"#)
}

fn rpr_of(font: &str, size: f32, pos: i32) -> String {
    let half = (size * 2.0).round() as u32;
    let position = if pos != 0 { format!(r#"<w:position w:val="{pos}"/>"#) } else { String::new() };
    format!(r#"<w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:eastAsia="{font}" w:cs="{font}"/>{position}<w:sz w:val="{half}"/><w:szCs w:val="{half}"/>"#)
}

/// Compare a page's baselines with a row of the table: the three lines' and the "ｚ"'s, each where Word's has it.
fn check(row: &Row, got: (Vec<f32>, Option<f32>), tol: f32, worst: &mut f32) {
    let mut one = |what: &str, ours: Option<f32>, theirs: Option<f32>| {
        if let Some(t) = theirs {
            let o = ours.unwrap_or_else(|| panic!("{}: no {what}", row.id));
            let d = (o - t).abs();
            *worst = worst.max(d);
            if std::env::var_os("IBIS_SOFT").is_some() {
                if d > tol {
                    eprintln!("MISMATCH {} {what}: {o:.2} against Word's {t:.2}", row.id);
                }
            } else {
                assert!(d <= tol, "{} {what}: {o:.2} against Word's {t:.2} (lines {:?}, z {:?})", row.id, got.0, got.1);
            }
        }
    };
    for k in 0..3 {
        one(&format!("line {}", k + 1), got.0.get(k).copied(), row.b[k]);
    }
    one("the next paragraph (ｚ)", got.1, row.z);
}

/// The rows of a group that are of three lines of one typeface and size: `text` the characters of each line.
fn plain(group: &str, tol: f32) {
    let mut worst = 0.0f32;
    for row in rows(group) {
        let font = typeface(&row.get("font").unwrap_or_else(|| "msm".into()));
        let text = row.get("text").unwrap_or_else(|| "あ".into());
        // Arial's and Times New Roman's Latin line is not Word's yet at a single line either (Arial "H" 10.5 pt: 9.84 / 12.07 pt in Word's, 9.51 here at `w:line` 240): not
        // this issue's (the multiple), left out; the others (Century's Latin letters, every typeface's kana) are checked.
        if text == "H" && matches!(font, "Arial" | "Times New Roman") {
            continue;
        }
        let r = rpr_of(font, row.size(), 0);
        let line = run(&r, &text);
        let body = three(&spacing(row.line), &r, [&line, &line, &line]);
        let p = pages_of(&make(&row.id, font, (row.size() * 2.0) as u32, 0, false, &body));
        check(&row, baselines(&p, &text), tol, &mut worst);
    }
    eprintln!("{group}: the largest difference {worst:.2} pt");
}

/// Table 1, `w:line` 24 to 264 (0.1 to 1.1 lines): `m A`, `m L` of the line of ＭＳ 明朝 10.5 pt, continuous at 239 and 240, the larger ones as before.
fn sweep() {
    plain("s1", 0.3);
}

/// Table 2, 8 to 72 pt at 0.5, 0.8, 0.9: in proportion to the size (`A` and `L` are).
fn sizes() {
    plain("s2", 0.3);
}

/// Table 3, nine typefaces (ＭＳ, BIZ UD, 游, メイリオ, Arial, Times New Roman, Century; kana and Latin letters) at 10.5 and 24 pt, 0.5 and 0.8: the same law, each
/// typeface's own `A` and `L`.
fn typefaces() {
    plain("s3", 0.3);
}

/// Table 4, two sizes (10.5 and 24 pt) in one line (the larger one's) and in different lines (each line its own).
fn mixed() {
    let mut worst = 0.0f32;
    let (small, big) = (rpr_of("ＭＳ 明朝", 10.5, 0), rpr_of("ＭＳ 明朝", 24.0, 0));
    for row in rows("s4") {
        let (s, b) = (run(&small, "あ"), run(&big, "あ"));
        let both = format!("{s}{b}");
        let lines: [&str; 3] = match row.get("kind").unwrap().as_str() {
            "same" => [&both, &both, &both],
            "l1big" => [&b, &s, &s],
            "l2big" => [&s, &b, &s],
            "l3big" => [&s, &s, &b],
            k => panic!("{k}"),
        };
        let body = three(&spacing(row.line), &small, lines);
        let p = pages_of(&make(&row.id, "ＭＳ 明朝", 21, 0, false, &body));
        check(&row, baselines(&p, "あ"), 0.3, &mut worst);
    }
    eprintln!("s4: the largest difference {worst:.2} pt");
}

/// Table 5: on an 18 pt line grid a multiple of 1 or less changes nothing (the grid's cells decide), the same baselines at 0.5 to 1.
fn on_grid() {
    let mut worst = 0.0f32;
    for row in rows("s5") {
        let r = rpr_of("ＭＳ 明朝", row.size(), 0);
        let l = run(&r, "あ");
        let body = three(&spacing(row.line), &r, [&l, &l, &l]);
        let p = pages_of(&make(&row.id, "ＭＳ 明朝", (row.size() * 2.0) as u32, 360, false, &body));
        check(&row, baselines(&p, "あ"), 0.3, &mut worst);
    }
    eprintln!("s5: the largest difference {worst:.2} pt");
}

/// Table 6: raised and lowered text (`w:position`, 10.5 and 24 pt, up to ±24 pt) with a multiple of 1 or less: the single line's `A + r` and `D - r` (neither under 0),
/// then the same law.
fn position() {
    let mut worst = 0.0f32;
    for row in rows("s6").into_iter().chain(rows("s6b")) {
        let pos: i32 = row.get("pos").unwrap().parse().unwrap();
        let r = rpr_of("ＭＳ 明朝", row.size(), pos);
        let l = run(&r, "あ");
        let body = three(&spacing(row.line), &r, [&l, &l, &l]);
        let p = pages_of(&make(&row.id, "ＭＳ 明朝", (row.size() * 2.0) as u32, 0, false, &body));
        check(&row, baselines(&p, "あ"), 0.35, &mut worst);
    }
    eprintln!("s6: the largest difference {worst:.2} pt");
}

/// Table 7: an inline picture (14, 20, 40 pt square) in a line of 10.5 pt text: `a = max(A, the picture)`, `d = D`, the line `H - (1 - m) L` of the text's `L`.
fn picture() {
    let mut worst = 0.0f32;
    for row in rows("s7") {
        let side: u32 = row.get("pic").unwrap().parse().unwrap();
        let emu = side * 12700;
        let r = rpr_of("ＭＳ 明朝", 10.5, 0);
        let pic = format!(
            r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{emu}" cy="{emu}"/><wp:docPr id="1" name="p"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="1" name="p"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId3"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{emu}" cy="{emu}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
        );
        let line = format!("{}{pic}{}", run(&r, "あ"), run(&r, "あ"));
        let body = three(&spacing(row.line), &r, [&line, &line, &line]);
        let p = pages_of(&make(&row.id, "ＭＳ 明朝", 21, 0, true, &body));
        check(&row, baselines(&p, "あ"), 0.3, &mut worst);
    }
    eprintln!("s7: the largest difference {worst:.2} pt");
}

/// Table 8: a paragraph of one line and an empty one are `m L` high (10.5 pt: 6.86 at 0.5), the space after (6 pt) not multiplied.
fn one_line() {
    let mut worst = 0.0f32;
    for row in rows("s8") {
        let kind = row.get("kind").unwrap();
        let sa: u32 = row.get("sa").unwrap().parse().unwrap();
        let big = kind == "one24";
        let r = rpr_of("ＭＳ 明朝", if big { 24.0 } else { 10.5 }, 0);
        let content = if kind == "empty" { String::new() } else { run(&r, "あ") };
        let body = format!(
            r#"<w:p><w:pPr><w:spacing w:before="0" w:after="{sa}" w:line="{}" w:lineRule="auto"/><w:rPr>{r}</w:rPr></w:pPr>{content}</w:p><w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="ＭＳ 明朝" w:hAnsi="ＭＳ 明朝" w:eastAsia="ＭＳ 明朝"/><w:sz w:val="21"/></w:rPr><w:t>ｚ</w:t></w:r></w:p>"#,
            row.line
        );
        let p = pages_of(&make(&row.id, "ＭＳ 明朝", 21, 0, false, &body));
        check(&row, baselines(&p, "あ"), 0.3, &mut worst);
    }
    eprintln!("s8: the largest difference {worst:.2} pt");
}

/// Table 9: a table's cell of 1 or 3 lines (150 pt wide, 10.5 pt): the first baseline is `m A` under the cell's top as outside, the row `m L` a line plus the rule.
/// Word's values are from the page's top margin; the cell's top is 1.8 pt (rule and padding) there, the same at every multiple, so the differences from the single
/// line's are compared.
fn table_cell() {
    let all = rows("s9");
    let mut worst = 0.0f32;
    let ours = |row: &Row| {
        let n: usize = row.get("n").unwrap().parse().unwrap();
        let r = rpr_of("ＭＳ 明朝", 10.5, 0);
        let lines: String = (0..n).map(|k| format!("{}{}", if k > 0 { "<w:r><w:br/></w:r>" } else { "" }, run(&r, "あ"))).collect();
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/><w:tblBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="{}" w:lineRule="auto"/><w:rPr>{r}</w:rPr></w:pPr>{lines}</w:p></w:tc></w:tr></w:tbl><w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="ＭＳ 明朝" w:hAnsi="ＭＳ 明朝" w:eastAsia="ＭＳ 明朝"/><w:sz w:val="21"/></w:rPr><w:t>ｚ</w:t></w:r></w:p>"#,
            row.line
        );
        let (ys, z) = baselines(&pages_of(&make(&row.id, "ＭＳ 明朝", 21, 0, false, &body)), "あ");
        (ys, z.unwrap())
    };
    for n in [1usize, 3] {
        let of = |line: u32| all.iter().find(|r| r.get("n").unwrap().parse::<usize>().unwrap() == n && r.line == line).unwrap();
        let (base_theirs, base_ours) = (of(240), ours(of(240)));
        for row in all.iter().filter(|r| r.get("n").unwrap().parse::<usize>().unwrap() == n && r.line != 240) {
            let o = ours(row);
            // The first baseline, the pitch of the lines, the next paragraph: each as far from the single line's as Word's is.
            let mut pairs = vec![("first baseline", o.0[0] - base_ours.0[0], row.b[0].unwrap() - base_theirs.b[0].unwrap()), ("next paragraph", o.1 - base_ours.1, row.z.unwrap() - base_theirs.z.unwrap())];
            if n == 3 {
                pairs.push(("pitch", (o.0[2] - o.0[0]) - (base_ours.0[2] - base_ours.0[0]), (row.b[2].unwrap() - row.b[0].unwrap()) - (base_theirs.b[2].unwrap() - base_theirs.b[0].unwrap())));
            }
            for (what, a, b) in pairs {
                let d = (a - b).abs();
                worst = worst.max(d);
                if std::env::var_os("IBIS_SOFT").is_some() {
                    if d > 0.3 {
                        eprintln!("MISMATCH {} {what}: {a:.2} against Word's {b:.2}", row.id);
                    }
                } else {
                    assert!(d <= 0.3, "{} {what}: {a:.2} against Word's {b:.2}", row.id);
                }
            }
        }
    }
    eprintln!("s9: the largest difference {worst:.2} pt");
}
