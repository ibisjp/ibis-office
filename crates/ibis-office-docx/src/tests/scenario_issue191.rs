// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #191 (the vertical space of paragraphs, the breaks of pages, frames and floating figures differ from Word's), the document (DOCX)
//! side. Each test lays out paragraphs (MS Mincho 10.5 pt, A4 with 72 pt margins, compatibility mode 15, exact lines of 14 pt as the issue's
//! file) and measures where lines are; the values are the issue's measurements of Word's PDFs (tolerance 0.25 pt; Word's PDFs have 0.12 pt steps).

use super::flow_layout::{Item, Pagination};
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue191_keep_next_heading_goes_with_a_paragraph_widow_control_moves", keep_next_heading_goes_with_a_paragraph_widow_control_moves),
        ("docx::issue191_spacing_in_lines_is_a_length", spacing_in_lines_is_a_length),
        ("docx::issue191_space_before_is_dropped_at_the_top_of_a_page", space_before_is_dropped_at_the_top_of_a_page),
        ("docx::issue191_a_shape_the_text_goes_round_is_drawn_once", a_shape_the_text_goes_round_is_drawn_once),
        ("docx::issue191_a_shape_set_from_its_character_is_at_the_character", a_shape_set_from_its_character_is_at_the_character),
        ("docx::issue191_a_shape_outside_the_column_leaves_the_text_alone", a_shape_outside_the_column_leaves_the_text_alone),
        ("docx::issue191_space_before_at_the_top_of_a_page_by_how_it_got_there", space_before_at_the_top_of_a_page_by_how_it_got_there),
        ("docx::issue191_a_shape_from_its_character_follows_the_line_and_the_alignment", a_shape_from_its_character_follows_the_line_and_the_alignment),
        ("docx::issue191_text_round_a_tight_outline_comes_below_it_in_whole_lines", text_round_a_tight_outline_comes_below_it_in_whole_lines),
        ("docx::issue191_a_drop_cap_keeps_the_text_off_by_its_advance_and_hspace", a_drop_cap_keeps_the_text_off_by_its_advance_and_hspace),
        ("docx::issue191_drop_cap_size_and_raise_are_words", drop_cap_size_and_raise_are_words),
        ("docx::issue191_the_editors_drop_cap_has_words_size_and_raise", the_editors_drop_cap_has_words_size_and_raise),
        ("docx::issue191_dump", dump),
    ]
}

/// A document of `body` (the XML of its paragraphs); the path of the file.
pub(crate) fn make(name: &str, body: &str) -> String {
    make_in(name, body, "")
}

/// The same, with `sect_extra` (XML) in its section's properties, after the page margins.
pub(crate) fn make_in(name: &str, body: &str, sect_extra: &str) -> String {
    make_mode(name, body, sect_extra, 15)
}

/// The same, in compatibility mode `mode`.
pub(crate) fn make_mode(name: &str, body: &str, sect_extra: &str, mode: u32) -> String {
    make_control(name, body, sect_extra, mode, "doNotCompress")
}

/// The same, with the document's `w:characterSpacingControl` (`doNotCompress` or `compressPunctuation`).
pub(crate) fn make_control(name: &str, body: &str, sect_extra: &str, mode: u32, control: &str) -> String {
    let direction = sect_extra;
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let settings = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{w}"><w:characterSpacingControl w:val="{control}"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{mode}"/></w:compat></w:settings>"#);
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{w}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="ＭＳ 明朝" w:hAnsi="ＭＳ 明朝" w:eastAsia="ＭＳ 明朝" w:cs="ＭＳ 明朝"/><w:sz w:val="21"/><w:szCs w:val="21"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#);
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>{direction}</w:sectPr></w:body></w:document>"#
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
    std::fs::write(dir.join(format!("issue191-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue191-{name}.docx")
}


pub(crate) fn pages_of(file: &str) -> std::sync::Arc<Pagination> {
    let mut s = Scenario::open(file);
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}


const GOTHIC: &str = r#"<w:rFonts w:ascii="游ゴシック" w:hAnsi="游ゴシック" w:eastAsia="游ゴシック" w:cs="游ゴシック"/>"#;

/// A paragraph of `text` with `ppr` (XML) in its properties and `rpr` (XML) in its run's.
pub(crate) fn p(ppr: &str, rpr: &str, text: &str) -> String {
    format!(r#"<w:p><w:pPr>{ppr}</w:pPr><w:r><w:rPr>{rpr}</w:rPr><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

pub(crate) fn near(a: f32, b: f32, tol: f32, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} against {b}");
}

/// The baselines of the text items of page `n` (one per line, in order) with their text.
pub(crate) fn lines(p: &Pagination, n: usize) -> Vec<(f32, String)> {
    let mut v: Vec<(f32, String)> = Vec::new();
    for it in &p.pages[n].items {
        if let Item::Text { y, text, .. } = it {
            match v.last_mut() {
                Some(last) if (last.0 - *y).abs() < 0.01 => last.1.push_str(text),
                _ => v.push((*y, text.to_string())),
            }
        }
    }
    v
}

/// The line of page `n` whose text starts with `start`: its baseline.
pub(crate) fn y_of(p: &Pagination, n: usize, start: &str) -> f32 {
    lines(p, n).into_iter().find(|l| l.1.starts_with(start)).unwrap_or_else(|| panic!("no line {start} on page {n}: {:?}", lines(p, n))).0
}

/// Item 1 (the issue's first page): a heading kept with the next paragraph, whose two lines widow/orphan control keeps together, goes to the next page
/// with it when only the heading and one line would fit (Word's PDF: the page ends with the 37th line at 716.74, the next starts with the heading at 84.38).
fn keep_next_heading_goes_with_a_paragraph_widow_control_moves() {
    let ppr = r#"<w:widowControl w:val="1"/><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/>"#;
    let mut body = String::new();
    for i in 1..=37 {
        body += &p(ppr, GOTHIC, &format!("本文の文{i:02}"));
    }
    body += &p(&format!("<w:keepNext w:val=\"1\"/>{ppr}"), GOTHIC, "見出し");
    body += &format!(r#"<w:p><w:pPr>{ppr}</w:pPr><w:r><w:rPr>{GOTHIC}</w:rPr><w:t>一行目</w:t></w:r><w:r><w:br/></w:r><w:r><w:rPr>{GOTHIC}</w:rPr><w:t>二行目</w:t></w:r></w:p>"#);
    body += &p(ppr, GOTHIC, "後ろの文");
    let pg = pages_of(&make("keepnext", &body));
    assert_eq!(pg.pages.len(), 2, "two pages");
    near(y_of(&pg, 0, "本文の文37"), 716.74, 0.25, "the last line of the first page");
    assert!(lines(&pg, 0).iter().all(|l| !l.1.starts_with("見出し")), "the heading is not at the foot of the first page");
    near(y_of(&pg, 1, "見出し"), 84.38, 0.25, "the heading starts the next page");
}

/// Item 2: `beforeLines` and `afterLines` are lengths (1 line = 12 pt without a line grid; Word's PDF of the issue's third page, lines of 14 pt exactly:
/// the distance of one line's baseline from the one before is 25.94 for `beforeLines` 100, 38.04 for 200, and the line after one with `afterLines` 100 is 25.92 from it;
/// the same lengths in `before` and `after` give 26.04, 37.92 and 26.04).
fn spacing_in_lines_is_a_length() {
    let base = r#"<w:widowControl w:val="0"/>"#;
    let sp = |attrs: &str| format!(r#"{base}<w:spacing {attrs} w:line="280" w:lineRule="exact"/>"#);
    let mut body = p(&sp(r#"w:before="0" w:after="0""#), "", "基準零");
    let cases = [
        ("甲", r#"w:beforeLines="100" w:after="0""#),
        ("乙", r#"w:before="240" w:after="0""#),
        ("丙", r#"w:beforeLines="200" w:after="0""#),
        ("丁", r#"w:before="480" w:after="0""#),
    ];
    for (name, attrs) in cases {
        body += &p(&sp(attrs), "", name);
        body += &p(&sp(r#"w:before="0" w:after="0""#), "", &format!("基準{name}"));
    }
    body += &p(&sp(r#"w:before="0" w:afterLines="100""#), "", "戊");
    body += &p(&sp(r#"w:before="0" w:after="0""#), "", "基準戊");
    body += &p(&sp(r#"w:before="0" w:after="240""#), "", "己");
    body += &p(&sp(r#"w:before="0" w:after="0""#), "", "基準己");
    let pg = pages_of(&make("lines", &body));
    // The line before each one is the base one before it.
    let gap = |name: &str, before: &str| y_of(&pg, 0, name) - y_of(&pg, 0, before);
    near(gap("甲", "基準零"), 25.94, 0.25, "beforeLines 100");
    near(gap("乙", "基準甲"), 26.04, 0.25, "before 240");
    near(gap("丙", "基準乙"), 38.04, 0.25, "beforeLines 200");
    near(gap("丁", "基準丙"), 37.92, 0.25, "before 480");
    near(gap("基準戊", "戊"), 25.92, 0.25, "afterLines 100");
    near(gap("基準己", "己"), 26.04, 0.25, "after 240");
    // A line grid's pitch is the unit where the section has one: 1.5 lines of 18 pt (the `lines` grid) are 27 pt.
    let grid = make_in("lines-grid", &(p(&sp(r#"w:before="0" w:after="0""#), "", "基準") + &p(&sp(r#"w:beforeLines="150" w:after="0""#), "", "後")), r#"<w:docGrid w:type="lines" w:linePitch="360"/>"#);
    let pg = pages_of(&grid);
    near(y_of(&pg, 0, "後") - y_of(&pg, 0, "基準"), 14.0 + 27.0, 0.25, "beforeLines in a line grid");
}

/// A text box (`wps`) of `w` x `h` EMU anchored in the run, set `rel_h` from its place with `off_x` EMU, with the wrap element `wrap`.
pub(crate) fn text_box(id: u32, rel_h: &str, off_x: i64, wrap: &str, w: i64, h: i64) -> String {
    format!(r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="{id}" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="{rel_h}"><wp:posOffset>{off_x}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="{w}" cy="{h}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>{wrap}<wp:docPr id="{id}" name="fig{id}"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>アイウ</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr rot="0" vert="horz" wrap="square" anchor="t" anchorCtr="0"><a:noAutofit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#)
}

const SQUARE: &str = r#"<wp:wrapSquare wrapText="bothSides"/>"#;
const PT: i64 = 12_700;
const KANA: &str = "あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわをんあいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよ";

fn run(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

/// The shapes of page `n`: (x, y).
fn shapes(p: &Pagination, n: usize) -> Vec<(f32, f32)> {
    p.pages[n].items.iter().filter_map(|it| match it { Item::Shape { x, y, .. } => Some((*x, *y)), _ => None }).collect()
}

/// Item 5 (F1a): a text box the text goes round (`wrapSquare`) is drawn once (Word: 1; this drew it twice on the same place).
fn a_shape_the_text_goes_round_is_drawn_once() {
    let sp = r#"<w:spacing w:before="0" w:after="0"/>"#;
    let body = format!("{}<w:p><w:pPr>{sp}</w:pPr>{}{}</w:p>", p(sp, "", "#F1a"), text_box(101, "column", 3_810_000, SQUARE, 200 * PT, 80 * PT), run(KANA));
    let pg = pages_of(&make("once", &body));
    assert_eq!(shapes(&pg, 0).len(), 1, "one box: {:?}", shapes(&pg, 0));
}

/// Item 6 (W4a, W4b): a box set from its character (after the 20th character of 10.5 pt: the box is at the 20th's left edge): 72 + 199.5 = 271.5 pt (Word), and
/// with an offset of 60 pt 331.5; one box, not two.
fn a_shape_set_from_its_character_is_at_the_character() {
    let sp = r#"<w:spacing w:before="0" w:after="0"/>"#;
    for (name, off, want) in [("w4a", 0, 271.5), ("w4b", 60, 331.5)] {
        let body = format!("{}<w:p><w:pPr>{sp}</w:pPr>{}{}{}</w:p>", p(sp, "", "#W4"), run(&KANA.chars().take(20).collect::<String>()), text_box(102, "character", off * PT, SQUARE, 120 * PT, 60 * PT), run(KANA));
        let pg = pages_of(&make(name, &body));
        let s = shapes(&pg, 0);
        assert_eq!(s.len(), 1, "{name}: one box: {s:?}");
        near(s[0].0, want, 0.25, &format!("{name}: the box's left"));
    }
}

/// Item 7b (W3d, W3e): a box wholly in the margin (60 pt wide, 72 pt left of the column, with the 9 pt the text keeps off it) whose `wrapText` is `left`
/// leaves the column's text where it is, as `bothSides` does (Word: the first line at 91.82 pt in both).
fn a_shape_outside_the_column_leaves_the_text_alone() {
    let sp = r#"<w:spacing w:before="0" w:after="0"/>"#;
    let mut first = Vec::new();
    for (name, wrap) in [("w3d", r#"<wp:wrapSquare wrapText="left"/>"#), ("w3e", r#"<wp:wrapSquare wrapText="bothSides"/>"#)] {
        let body = format!("{}<w:p><w:pPr>{sp}</w:pPr>{}{}</w:p>", p(sp, "", "#W3"), text_box(103, "column", -72 * PT, wrap, 60 * PT, 100 * PT), run(KANA));
        let pg = pages_of(&make(name, &body));
        first.push(y_of(&pg, 0, "あいう"));
    }
    near(first[0], first[1], 0.01, "the first line with `left` and with `bothSides`");
    assert!(first[0] < 100.0, "the first line is beside the box: {}", first[0]);
}

/// Item 3 (the issue's fifth and sixth pages): in compatibility mode 15 a paragraph that comes first on a page has no space before, after the natural
/// break (the page is full) and after a page-break character (Word: the line at 83.16 pt with 20 pt before, as with none); one in the middle of a
/// page keeps it (33.96 from the line before: 14 + 20). A box set from the paragraph (`wrapNone`) in the paragraph after goes up with it (86.00).
fn space_before_is_dropped_at_the_top_of_a_page() {
    let base = r#"<w:widowControl w:val="0"/><w:spacing w:before="0" w:after="0" w:line="280" w:lineRule="exact"/>"#;
    let spaced = r#"<w:widowControl w:val="0"/><w:spacing w:before="400" w:after="0" w:line="280" w:lineRule="exact"/>"#;
    // A page of 697.9 pt holds 49 lines of 14 pt.
    let mut body = String::new();
    for i in 1..=49 {
        body += &p(base, "", &format!("行{i:02}"));
    }
    body += &p(spaced, "", "自然");
    let wrap_none = r#"<wp:wrapNone/>"#;
    body += &format!("<w:p><w:pPr>{base}</w:pPr>{}{}</w:p>", text_box(104, "column", 350 * PT, wrap_none, 100 * PT, 30 * PT), run("次の段落"));
    // Then a page-break character and the spaced paragraph again, and a spaced one in the middle of the page.
    body += &format!(r#"<w:p><w:pPr>{base}</w:pPr><w:r><w:br w:type="page"/></w:r></w:p>"#);
    body += &p(spaced, "", "改頁");
    body += &p(base, "", "途前");
    body += &p(spaced, "", "途後");
    let pg = pages_of(&make("top", &body));
    assert_eq!(pg.pages.len(), 3, "three pages");
    near(y_of(&pg, 1, "自然"), 83.16, 0.25, "the paragraph the natural break leaves first on a page");
    near(shapes(&pg, 1)[0].1, 86.0, 0.25, "the box of the paragraph after it");
    near(y_of(&pg, 2, "改頁"), 83.16, 0.25, "the paragraph after a page-break character");
    near(y_of(&pg, 2, "途後") - y_of(&pg, 2, "途前"), 33.96, 0.25, "a paragraph in the middle of the page keeps its space before");
}

/// The left of the first text of the line of page `n` whose text starts with `start`.
fn x_of(p: &Pagination, n: usize, start: &str) -> f32 {
    let y = y_of(p, n, start);
    p.pages[n].items.iter().filter_map(|it| match it { Item::Text { x, y: ty, .. } if (ty - y).abs() < 0.01 => Some(*x), _ => None }).fold(f32::MAX, f32::min)
}

/// Item 3, by how a paragraph came first in a page's column (Word's XPS, compatibility modes 12, 14, 15; a paragraph with 20 pt before is at 83.16 pt
/// when it loses it and 103.16 when it keeps it): `pageBreakBefore` loses it in mode 15 only (and in 12 and 14 when the paragraph before has space
/// after); the first paragraph of a section and of the document keeps it; the top of a second column loses it.
fn space_before_at_the_top_of_a_page_by_how_it_got_there() {
    let base = r#"<w:widowControl w:val="0"/><w:spacing w:before="0" w:after="0" w:line="280" w:lineRule="exact"/>"#;
    let after = r#"<w:widowControl w:val="0"/><w:spacing w:before="0" w:after="240" w:line="280" w:lineRule="exact"/>"#;
    let spaced = r#"<w:widowControl w:val="0"/><w:spacing w:before="400" w:after="0" w:line="280" w:lineRule="exact"/>"#;
    let paged = format!("<w:pageBreakBefore/>{spaced}");
    for (mode, prev, want) in [(15, base, 83.16), (14, base, 103.16), (12, base, 103.16), (14, after, 83.16)] {
        let body = p(prev, "", "前") + &p(&paged, "", "頁");
        let pg = pages_of(&make_mode(&format!("pbb{mode}"), &body, "", mode));
        near(y_of(&pg, 1, "頁"), want, 0.25, &format!("pageBreakBefore in mode {mode}"));
    }
    // The document's first paragraph keeps it; so does the first one of a section that starts a page.
    let pg = pages_of(&make("first", &p(spaced, "", "最初")));
    near(y_of(&pg, 0, "最初"), 103.16, 0.25, "the document's first paragraph");
    let sect = r#"<w:p><w:pPr><w:sectPr><w:type w:val="nextPage"/><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:pPr></w:p>"#;
    let body = p(base, "", "前") + sect + &p(spaced, "", "節");
    let pg = pages_of(&make("section", &body));
    near(y_of(&pg, 1, "節"), 103.16, 0.25, "the first paragraph of a section");
    // A second column filled to its end: its top loses it.
    let mut body = String::new();
    for i in 1..=49 {
        body += &p(base, "", &format!("行{i:02}"));
    }
    body += &p(spaced, "", "次段");
    let pg = pages_of(&make_in("column", &body, r#"<w:cols w:num="2" w:space="720"/>"#));
    near(y_of(&pg, 0, "次段"), 83.16, 0.25, "the top of the second column");
}

/// Item 6 (Word's XPS, issue #191 request 5): a box set from its character is at the left of the character before the anchor (three "あ" of 10.5 pt:
/// 72 + 21; fifty: in the second line, 72 + 7 x 10.5), `align` centres it on that (the box's width halved) or ends at it (a box of 100 pt reaching past the
/// page's left stays at 0).
fn a_shape_from_its_character_follows_the_line_and_the_alignment() {
    let sp = r#"<w:spacing w:before="0" w:after="0"/>"#;
    let one = |name: &str, id: u32, count: usize, fix: &dyn Fn(String) -> String, w: i64| {
        let body = format!("{}<w:p><w:pPr>{sp}</w:pPr>{}{}{}</w:p>", p(sp, "", "#C"), run(&"あ".repeat(count)), fix(text_box(id, "character", 0, SQUARE, w * PT, 40 * PT)), run(KANA));
        shapes(&pages_of(&make(name, &body)), 0)[0].0
    };
    near(one("chr3", 111, 3, &|s| s, 50), 93.0, 0.25, "after three characters");
    // An indent moves it with the text (left 720: 108.02 + 21); a tab before the anchor is the tab's stop (a stop at 42 pt: 114.0).
    let sp_ind = r#"<w:ind w:left="720"/><w:spacing w:before="0" w:after="0"/>"#;
    let body = format!("{}<w:p><w:pPr>{sp_ind}</w:pPr>{}{}{}</w:p>", p(sp, "", "#C"), run("あああ"), text_box(116, "character", 0, SQUARE, 50 * PT, 40 * PT), run(KANA));
    near(shapes(&pages_of(&make("chrind", &body)), 0)[0].0, 129.0, 0.3, "in an indent");
    let body = format!("{}<w:p><w:pPr><w:tabs><w:tab w:val=\"left\" w:pos=\"840\"/></w:tabs>{sp}</w:pPr>{}<w:r><w:tab/></w:r>{}{}{}</w:p>", p(sp, "", "#C"), run("あ"), run("あ"), text_box(117, "character", 0, SQUARE, 50 * PT, 40 * PT), run(KANA));
    near(shapes(&pages_of(&make("chrtab", &body)), 0)[0].0, 114.0, 0.3, "after a tab");
    near(one("chr50", 112, 50, &|s| s, 50), 145.5, 0.25, "in the second line");
    let align = |a: &'static str| move |s: String| s.replace("<wp:posOffset>0</wp:posOffset></wp:positionH>", &format!("<wp:align>{a}</wp:align></wp:positionH>"));
    near(one("chrc", 113, 3, &align("center"), 50), 68.0, 0.25, "centred");
    near(one("chrr", 114, 3, &align("right"), 50), 43.0, 0.25, "at the right");
    near(one("chrr100", 115, 3, &align("right"), 100), 0.0, 0.25, "at the right, past the page's left");
}

/// Item 7a (Word's XPS): text that goes round a `wrapTight` outline, which leaves it no room beside, comes below it on the first line, counted from the
/// paragraph's top in whole lines, whose top is not above the outline's foot: a figure 100 pt high puts the first baseline 8.88 pt lower than
/// `wrapSquare` does (200.66 against 191.78), and with 9 pt below (`distB`) a line more.
fn text_round_a_tight_outline_comes_below_it_in_whole_lines() {
    let sp = r#"<w:spacing w:before="0" w:after="0"/>"#;
    let tight = r#"<wp:wrapTight wrapText="left"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="0" y="21600"/><wp:lineTo x="21600" y="21600"/><wp:lineTo x="21600" y="0"/><wp:lineTo x="0" y="0"/></wp:wrapPolygon></wp:wrapTight>"#;
    let square = r#"<wp:wrapSquare wrapText="left"/>"#;
    let label = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsia="Arial" w:cs="Arial"/><w:sz w:val="16"/><w:szCs w:val="16"/>"#;
    let first = |name: &str, wrap: &str| {
        let body = format!("{}<w:p><w:pPr>{sp}</w:pPr>{}{}</w:p>", p(sp, label, "#T"), text_box(121, "column", 0, wrap, 150 * PT, 100 * PT), run(KANA));
        let pg = pages_of(&make(name, &body));
        let l = lines(&pg, 0);
        (y_of(&pg, 0, "あいう"), l[2].0 - l[1].0)
    };
    let ((sq, _), (ti, pitch)) = (first("sq", square), first("ti", tight));
    let lines_down = ((ti - sq + 100.0) / pitch).round();
    near(ti - sq + 100.0, lines_down * pitch, 0.05, "the first line is whole lines below the paragraph's top");
    assert!(ti - sq > 0.0 && ti - sq < pitch, "tight is below square by less than a line: {}", ti - sq);
    // Word's: 8.88 (its lines are 13.6 pt); here the lines are 13.5 pt, and 8 of them from the paragraph's top make 8.0 (the same 8 lines).
    near(ti - sq, 8.0 * pitch - 100.0, 0.05, "tight below square, 8 lines");
}

/// Item 4, the drop cap (Word's XPS): the capital's paragraph takes no height; the text after it keeps off the width its characters advance and `hSpace`
/// (a 31.5 pt "カ": 31.58 pt, "A" in MS Mincho 15.72, two characters 63.0; `hSpace` 14.15 pt more), on the lines whose tops are above the frame's foot (a line
/// of 30 pt exactly, 13.6 pt lines of text: three).
fn a_drop_cap_keeps_the_text_off_by_its_advance_and_hspace() {
    let cap = |name: &str, text: &str, h_space: &str| {
        let ppr = format!(r#"<w:framePr w:dropCap="drop" w:lines="3" w:hSpace="{h_space}" w:wrap="around" w:vAnchor="text" w:hAnchor="text"/><w:spacing w:before="0" w:after="0" w:line="600" w:lineRule="exact"/>"#);
        let body = p(&ppr, r#"<w:sz w:val="63"/><w:szCs w:val="63"/>"#, text) + &p(r#"<w:spacing w:before="0" w:after="0"/>"#, "", &"本".repeat(130));
        pages_of(&make(name, &body))
    };
    let pg = cap("cap1", "カ", "0");
    let l = lines(&pg, 0);
    let body: Vec<&(f32, String)> = l.iter().filter(|l| l.1.starts_with('本')).collect();
    let x = |y: f32| pg.pages[0].items.iter().filter_map(|it| match it { Item::Text { x, y: ty, text, .. } if (ty - y).abs() < 0.01 && text.starts_with('本') => Some(*x), _ => None }).fold(f32::MAX, f32::min);
    near(x(body[0].0), 72.0 + 31.58, 0.3, "the first line beside the capital");
    near(x(body[2].0), 72.0 + 31.58, 0.3, "the third line beside it");
    near(x(body[3].0), 72.0, 0.3, "the fourth line below it");
    let pg2 = cap("cap2", "ふふ", "0");
    let y2 = y_of(&pg2, 0, "本");
    near(x_of(&pg2, 0, "本"), 72.0 + 63.0, 0.3, &format!("two characters ({y2})"));
    let pg3 = cap("cap3", "カ", "283");
    near(x_of(&pg3, 0, "本"), 72.0 + 31.58 + 14.15, 0.3, "with hSpace");
}

/// Word's `w:sz` and `w:position` of a drop cap's capital (Windows Word, 3790 drop caps; `tasks/doc/requests/2026-11-16-191-dropcap-sz-result.md`): ＭＳ 明朝 10.5 pt, "あ"
/// (yMin -13/256, yMax 201/256 em), 3 lines of the 13.62 pt pitch with a line's foot 3.04 pt below its baseline: 43.5 pt, raised -3.0 pt (`w:position` -6). Where the em
/// square's top is the higher (メイリオ, whose descent is 0.44 em), the size is `((lines - 1) P_dc + (1 - winD) body) / h`, whatever the line's foot is; where the line's leading is
/// (a typeface with a large external leading, 游明朝 0.315 em), the lower of the two limits is taken, and a capital of 1 line is smaller than the body.
fn drop_cap_size_and_raise_are_words() {
    use super::flow_layout::{drop_cap_size_and_raise, DropCapFace};
    let ms = DropCapFace { win_ascent: 220.0 / 256.0, win_descent: 36.0 / 256.0, external_leading: 0.0, ink_min: -13.0 / 256.0, ink_max: 201.0 / 256.0 };
    let (size, raise) = drop_cap_size_and_raise(&ms, 10.5, 3, 13.62, 3.04, 0.0).expect("a size");
    near(size, 43.5, 0.01, "MS Mincho 10.5 pt, 3 lines: the size");
    near(raise, -3.0, 0.01, "its raise");
    let meiryo = DropCapFace { win_ascent: 2171.0 / 2048.0, win_descent: 901.0 / 2048.0, external_leading: 0.0, ink_min: 0.0, ink_max: 0.736 };
    for foot in [3.0, 4.0, 5.0] {
        let (size, _) = drop_cap_size_and_raise(&meiryo, 12.0, 4, 20.0, foot, 0.0).expect("a size");
        near(size, ((3.0f32 * 20.0 + (1.0 - 901.0 / 2048.0) * 12.0) / 0.736 * 2.0).round() / 2.0, 0.01, "Meiryo: the em square's top, whatever the foot");
    }
    // 游明朝: the leading's limit (0.602 body) is the lower at 1 line: ink from 0.602 x 10.5 down to the foot, 17.3 - 3 pt over 0.79 + 0.031.
    let yu = DropCapFace { win_ascent: 2038.0 / 2048.0, win_descent: 598.0 / 2048.0, external_leading: 0.315, ink_min: -0.031, ink_max: 0.790 };
    let (size, _) = drop_cap_size_and_raise(&yu, 10.5, 1, 17.0, 3.0, 0.0).expect("a size");
    assert!(size < 10.5, "a capital of 1 line is smaller than the body: {size}");
    assert!(drop_cap_size_and_raise(&ms, 10.5, 0, 13.62, 3.04, 0.0).is_none() && drop_cap_size_and_raise(&DropCapFace { ink_max: 0.0, ink_min: 0.0, ..ms }, 10.5, 3, 13.62, 3.04, 0.0).is_none());
    // No more than 1638 pt (`w:sz` 3276).
    let (size, _) = drop_cap_size_and_raise(&meiryo, 144.0, 10, 220.0, 20.0, 0.0).expect("a size");
    assert_eq!(size, 1638.0);
}

/// The editor's drop cap of "あ" in ＭＳ 明朝 10.5 pt, 3 lines, off a grid, has Word's `w:sz` 87 (43.5 pt) and `w:position` -6, and its lines 817 twips (Windows Word's drop cap of the
/// same: line 817, sz 87, position -6). 4 lines: the size grows by P_dc over h (13.62 / 0.836 = 16.3 pt), 59.5 pt.
fn the_editors_drop_cap_has_words_size_and_raise() {
    let body = p(r#"<w:spacing w:before="0" w:after="0"/>"#, "", &format!("あ{}", "本".repeat(120)));
    use super::scenarios::DocSteps;
    let mut s = Scenario::open(&make("cap-words", &body));
    let id = s.with(|e, _| e.flow().unwrap().body.iter().find_map(|b| match b { ibis_doc_model::Block::Paragraph(p) => Some(p.id), _ => None }).unwrap());
    s.click_text(ibis_doc_model::FlowPos::new(id, 1));
    s.menu("docx.drop-cap.drop");
    let frame = |s: &mut Scenario| {
        s.with(|e, _| {
            let ibis_doc_model::Block::Paragraph(f) = &e.flow().unwrap().body[0] else { panic!("the frame") };
            (f.runs[0].format.size, f.runs[0].format.raise, match f.direct.line { Some(ibis_doc_model::LineHeight::Exact(l)) => l as f32 / 12700.0, other => panic!("{other:?}") })
        })
    };
    let (size, raise, line) = frame(&mut s);
    // The glyph's ink is the drawn font's: ＭＳ 明朝 is not on every PC, and the stand-in's "あ" is up to a point higher or lower than its own (42.5 pt and -3.5 here, against 43.5 and -3.0).
    near(size.expect("a size") as f32 / 100.0, 43.5, 1.5, "3 lines: the size");
    assert!(raise.is_some_and(|r| (r + 6).abs() <= 2), "3 lines: the raise {raise:?}");
    near(line, 3.0 * 13.62, 0.3, "its line");
    s.menu("docx.drop-cap.lines.4");
    let (size, _, _) = frame(&mut s);
    near(size.unwrap() as f32 / 100.0, 59.5, 1.5, "4 lines");
}

/// Not a test: prints what a file's pages hold (I191_FILE).
fn dump() {
    let Ok(file) = std::env::var("I191_FILE") else { return };
    let p = pages_of(&file);
    for (n, page) in p.pages.iter().enumerate() {
        println!("--- page {n}");
        for it in &page.items {
            match it {
                Item::Text { x, y, text, offsets, size, .. } => {
                    let xs: Vec<String> = offsets.iter().take(3).map(|o| format!("{:.1}", x + o)).collect();
                    println!("T y={y:.2} x={x:.2} sz={size:.2} {text} [{}]", xs.join(","));
                }
                Item::Picture { x, y, w, h, .. } => println!("PIC x={x:.2} y={y:.2} w={w:.1} h={h:.1}"),
                Item::Shape { x, y, w, h, .. } => println!("SHAPE x={x:.2} y={y:.2} w={w:.1} h={h:.1}"),
                Item::Rect { x, y, w, h, .. } => println!("RECT x={x:.2} y={y:.2} w={w:.1} h={h:.1}"),
                _ => {}
            }
        }
    }
}
