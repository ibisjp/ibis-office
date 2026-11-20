// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #188 (vertical writing: 縦中横, orientation of characters, ruby/underline/emphasis side, running heads differ from Word's), the
//! document (DOCX) side. Each test lays out one left aligned paragraph of a `tbRl` section (MS Mincho 10.5 pt, A4 with 72 pt margins) and measures
//! where characters are; the values are the issue's measurements of Word's PDFs (tolerance 0.25 pt; Word's PDFs have 0.12 pt steps).

use super::flow_layout::{Item, Pagination};
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue188_tate_chu_yoko_draws_every_digit", tate_chu_yoko_draws_every_digit),
        ("docx::issue188_characters_Word_lays_down_are_turned", characters_word_lays_down_are_turned),
        ("docx::issue188_tate_chu_yoko_squeeze_is_floor_of_w_over_100_x_in_600_dpi_pixels", tate_chu_yoko_squeeze),
        ("docx::issue188_spaces_are_drawn_in_a_column", spaces_are_drawn_in_a_column),
        ("docx::issue188_cell_direction_names", cell_direction_names),
        ("docx::issue188_ruby_centre_in_a_column_is_the_raise_from_the_base", ruby_centre_in_a_column),
        ("docx::issue188_running_head_of_a_vertical_section_stays_across", running_head_stays_across),
        ("docx::issue188_u205f_is_four_eighteenths_of_an_em", medium_mathematical_space),
        ("docx::issue188_emphasis_marks_in_a_column", emphasis_marks_in_a_column),
        ("docx::issue188_tbrl_cell_turns_japanese_and_tbrlv_keeps_it_upright", tbrl_cell_turns_all_tbrlv_keeps_japanese_upright),
        ("docx::issue188_quotes_in_a_proportional_face_go_by_its_width", quotes_in_ms_p_mincho),
        ("docx::issue188_word_orientation_of_5_typefaces", word_orientation_of_5_typefaces),
        ("docx::issue188_dump", dump),
    ]
}

/// A document of `body` (the XML of its paragraphs); the path of the file.
fn make(name: &str, body: &str) -> String {
    make_in(name, body, true)
}

/// The same, in a section of vertical writing (`tbRl`) or of across writing.
fn make_in(name: &str, body: &str, vertical: bool) -> String {
    make_with_head(name, body, vertical, None)
}

/// The same with a running head of the paragraph XML `head` (36 pt from the paper's top, 72 pt margins).
fn make_with_head(name: &str, body: &str, vertical: bool, head: Option<&str>) -> String {
    let direction = if vertical { r#"<w:textDirection w:val="tbRl"/>"# } else { "" };
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let settings = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{w}"><w:characterSpacingControl w:val="doNotCompress"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#);
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{w}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="ＭＳ 明朝" w:hAnsi="ＭＳ 明朝" w:eastAsia="ＭＳ 明朝" w:cs="ＭＳ 明朝"/><w:sz w:val="21"/><w:szCs w:val="21"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#);
    let header_ref = if head.is_some() { r#"<w:headerReference w:type="default" r:id="rId3" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/>"# } else { "" };
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body>{body}<w:sectPr>{header_ref}<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>{direction}<w:docGrid w:linePitch="360"/></w:sectPr></w:body></w:document>"#
    );
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/settings.xml", EntryMeta::new(), settings.as_bytes()).unwrap();
    z.add("word/styles.xml", EntryMeta::new(), styles.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    if let Some(head) = head {
        let header = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr xmlns:w="{w}">{head}</w:hdr>"#);
        z.add("word/header1.xml", EntryMeta::new(), header.as_bytes()).unwrap();
    }
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("issue188-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue188-{name}.docx")
}

/// A left aligned paragraph of `before` (plain text), the run `middle` (XML), and `after`.
fn para(before: &str, middle: &str, after: &str) -> String {
    format!(r#"<w:p><w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:t xml:space="preserve">{before}</w:t></w:r>{middle}<w:r><w:t xml:space="preserve">{after}</w:t></w:r></w:p>"#)
}

fn pages_of(file: &str) -> std::sync::Arc<Pagination> {
    let mut s = Scenario::open(file);
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

/// Every text item of the first page: (baseline, size, text, the x of each character).
fn texts(p: &Pagination) -> Vec<(f32, f32, String, Vec<f32>)> {
    p.pages[0].items.iter().filter_map(|it| match it {
        Item::Text { x, y, text, offsets, size, .. } => Some((*y, *size, text.to_string(), offsets.iter().map(|o| x + o).collect())),
        _ => None,
    }).collect()
}

/// The items whose text is exactly `text`.
fn of<'a>(t: &'a [(f32, f32, String, Vec<f32>)], text: &str) -> Vec<&'a (f32, f32, String, Vec<f32>)> {
    t.iter().filter(|i| i.2 == text).collect()
}

/// The x of the first character of the item that starts with `text`.
fn start_x(t: &[(f32, f32, String, Vec<f32>)], text: &str) -> f32 {
    t.iter().find(|i| i.2.starts_with(text)).unwrap_or_else(|| panic!("no item {text}: {t:?}")).3[0]
}

fn near(a: f32, b: f32, tol: f32, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} against {b}");
}

fn tcy(id: u32, text: &str) -> String {
    format!(r#"<w:r><w:rPr><w:eastAsianLayout w:id="{id}" w:vert="1"/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

/// The page-1 items as (x, y, text, size) of every drawn text on the first page (in paper points).
fn drawn(p: &Pagination) -> Vec<(f32, f32, String)> {
    let mut v = Vec::new();
    for it in &p.pages[0].items {
        match it {
            Item::Text { x, y, text, offsets, .. } => {
                for (c, o) in text.chars().zip(offsets.iter()) { v.push((x + o, *y, c.to_string())); }
            }
            Item::Scaled { x, y, text, .. } | Item::Turned { x, y, text, .. } => v.push((*x, *y, text.to_string())),
            _ => {}
        }
    }
    v
}

/// Item 2 (D2b, D2c): a 縦中横 run of two or more digits is drawn whole (each digit, side by side across the column).
fn tate_chu_yoko_draws_every_digit() {
    for (name, digits) in [("d2b", "12"), ("d2c", "2026")] {
        let d = drawn(&pages_of(&make(name, &para("あいう", &tcy(100, digits), "たちつ"))));
        for c in digits.chars() {
            assert!(d.iter().any(|i| i.2 == c.to_string()), "{name}: the digit {c} is drawn: {d:?}");
        }
        // The digits are side by side across the column, in one cell: the same place down it. The kana after is 45.14 pt (Word) from the first.
        let first = d.iter().find(|i| i.2 == digits[..1]).unwrap();
        let last = d.iter().rev().find(|i| i.2 == digits[digits.len() - 1..]).unwrap();
        near(first.1, last.1, 0.01, &format!("{name}: one cell"));
        assert!(last.0 > first.0, "{name}: in order across the column");
        let (a, ta) = (d.iter().find(|i| i.2 == "あ").unwrap(), d.iter().find(|i| i.2 == "た").unwrap());
        near(ta.1 - a.1, 45.14, 0.25, &format!("{name}: the kana after"));
    }
}

/// A 縦中横 run's `w:w` squeezes by `floor(w / 100 * X) / X`, `X` the average character width of the face (0.5 em of ＭＳ 明朝) in pixels of 600 dpi, rounded (Windows Word's XPS,
/// `tasks/doc/requests/2026-11-18-188-tate-chu-yoko-fit-result.md` 2: ＭＳ 12 pt X 50 gives 0.86 0.64 0.50 0.42 0.32 for w 86 64 51 43 32; 10.5 pt X 44 gives 0.8409 0.6364 0.5 0.4091 0.3182
/// for the same w).
fn tate_chu_yoko_squeeze() {
    for (half_points, cases) in [(24, [(86u32, 0.86f32), (64, 0.64), (51, 0.50), (43, 0.42), (32, 0.32)]), (21, [(86, 0.8409), (64, 0.6364), (51, 0.5), (43, 0.4091), (32, 0.3182)])] {
        for (w, want) in cases {
            let run = format!(r#"<w:r><w:rPr><w:eastAsianLayout w:id="1" w:vert="1" w:vertCompress="1"/><w:w w:val="{w}"/><w:sz w:val="{half_points}"/></w:rPr><w:t>12345</w:t></w:r>"#);
            let p = pages_of(&make(&format!("tcy-w-{half_points}-{w}"), &para("あいう", &run, "たちつ")));
            let scales: Vec<f32> = p.pages[0].items.iter().filter_map(|it| match it { Item::Scaled { xscale, text, .. } if text.chars().all(|c| c.is_ascii_digit()) => Some(*xscale), _ => None }).collect();
            assert_eq!(scales.len(), 5, "{half_points} {w}: {scales:?}");
            near(scales[0], want, 0.0006, &format!("{half_points} half-points, w {w}"));
        }
    }
}

/// Item 1: the characters the issue's table has Word laying down (UAX #50 R, the `(b)` ones, and the Latin-1 and Number Forms ones Word lays down
/// though UAX #50 stands them) are turned; Å, the fullwidth － ＜ ＝ ＞ and ‘ ’ “ ” stand.
fn characters_word_lays_down_are_turned() {
    use crate::render::vertical_text::upright;
    for c in "‐–—―•․‥…‧′″‵‾€←↑→↓↔↖↗↘↙⇧∀∂∃∇∈∉∋√∝∟∠∥∦∧∨∩∪∫∬∮∽≈≒≠≡≢≦≧≪≫⊂⊃⊄⊅⊆⊇⊕⊥⊿☞©®±¼½¾×÷⅓⅔⅕∞∴∵".chars() {
        assert!(!upright(c), "U+{:04X} lies", c as u32);
    }
    for c in "\u{212B}－＜＝＞‘’“”⁄⁋⁕⁜".chars() {
        assert!(upright(c), "U+{:04X} stands", c as u32);
    }
}

/// Windows Word's PDF of 5 typefaces (GitHub issue #188, `2026-11-16-188-proportional-upright-result.md`): ‘ ’ “ ” ⇒ ⇔ § ‖ † ‡ ‰ ※ ‼ ⁇ ⁈ ⁉ ℃ № stand, and ¨ © « ® ° ± ´ ¶ » ¼ ½ ¾ × ÷ ′ ″ ™
/// ← ↑ → ↓ ∞ lie, whatever the typeface and the hint.
fn word_orientation_of_5_typefaces() {
    use crate::render::vertical_text::upright;
    for c in "‘’“”⇒⇔§‖†‡‰※‼⁇⁈⁉℃№".chars() {
        assert!(upright(c), "U+{:04X} stands", c as u32);
    }
    for c in "¨©«®°±´¶»¼½¾×÷′″™←↑→↓∞".chars() {
        assert!(!upright(c), "U+{:04X} lies", c as u32);
    }
}

/// Item 3 (D3a): U+0020, U+00A0, U+2007, U+202F and U+3000 are characters of the column (Word draws all five into its PDF) and each takes its room.
fn spaces_are_drawn_in_a_column() {
    let spaces = ['\u{20}', '\u{a0}', '\u{2007}', '\u{202f}', '\u{3000}'];
    let middle: String = spaces.iter().map(|c| format!("え{c}お")).collect();
    let d = drawn(&pages_of(&make_in("d3a", &para("あいう", &format!(r#"<w:r><w:t xml:space="preserve">{middle}</w:t></w:r>"#), "か"), true)));
    for c in spaces {
        assert!(d.iter().any(|i| i.2 == c.to_string()), "U+{:04X} is drawn: {d:?}", c as u32);
    }
    // The kana before and after U+202F (D3a: 12.60 pt apart in Word, the em of え and the 2.1 pt of the narrow space).
    let ys: Vec<f32> = d.iter().filter(|i| i.2 == "え" || i.2 == "お").map(|i| i.1).collect();
    // え お え お ...: the 4th pair has U+202F between.
    near(ys[7] - ys[6], 12.60, 0.25, "え to お across U+202F");
}

/// Item 7 (D7b-D7d): `rlV`, `lrV` and `tbV` read as Word reads them.
fn cell_direction_names() {
    use ibis_doc_model::CellDirection as D;
    assert_eq!(D::from_name("tbRlV"), D::TbRlV);
    assert_eq!(D::from_name("tbRl"), D::TbRl);
    assert_eq!(D::from_name("rlV"), D::TbRlV, "rlV is as tbRlV");
    assert_eq!(D::from_name("lrV"), D::TbLrV, "lrV is as tbLrV: down, lines left to right");
    assert_eq!(D::from_name("tbV"), D::LrTbV, "tbV is across, the Japanese turned anticlockwise");
}

/// #222 (Word for Windows, D6a): in a cell `tbRl` lays all of it on its side, `tbRlV` keeps the Japanese upright.
fn tbrl_cell_turns_all_tbrlv_keeps_japanese_upright() {
    for (direction, upright) in [("tbRl", false), ("tbRlV", true)] {
        let body = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="5000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="3000"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="5000" w:type="dxa"/><w:textDirection w:val="{direction}"/></w:tcPr><w:p><w:r><w:t>あい</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
        );
        let pages = pages_of(&make_in(&format!("cell-{direction}"), &body, false));
        let turned = pages.pages[0].items.iter().any(|it| matches!(it, Item::Turned { text, .. } if text.as_ref() == "あ"));
        assert_eq!(turned, !upright, "{direction}: the あ lies on its side: {turned}");
    }
}

/// Item 5 (D5a): in a column the ruby's box centre is `floor(raise)` pt right of the base's centre (Word 9.00 for `hpsRaise` 18, 18.00 for 36, 4.92 for 10;
/// 8, 9, 12 and 17 pt for 17, 19, 25 and 35 half points), whatever `hps`.
fn ruby_centre_in_a_column() {
    for (name, hps, raise, expected) in [("d5a-18", 10, 18, 9.0), ("d5a-36", 10, 36, 18.0), ("d5a-10", 8, 10, 5.0), ("d5a-26", 14, 26, 13.0), ("d5a-19", 10, 19, 9.0)] {
        let ruby = format!(
            r#"<w:r><w:ruby><w:rubyPr><w:rubyAlign w:val="center"/><w:hps w:val="{hps}"/><w:hpsRaise w:val="{raise}"/><w:hpsBaseText w:val="21"/><w:lid w:val="ja-JP"/></w:rubyPr><w:rt><w:r><w:t>さし</w:t></w:r></w:rt><w:rubyBase><w:r><w:t>かき</w:t></w:r></w:rubyBase></w:ruby></w:r>"#
        );
        let t = texts(&pages_of(&make(name, &para("", &ruby, ""))));
        let centre = |ch: &str| {
            let it = t.iter().find(|i| i.2 == ch).unwrap_or_else(|| panic!("{name}: no {ch}: {t:?}"));
            it.3[0] + it.1 / 2.0
        };
        let (ruby_centre, base_centre) = (centre("さ"), centre("か"));
        near(ruby_centre - base_centre, expected, 0.3, &format!("{name}: the ruby's centre from the base's"));
    }
}

/// Item 6 (D6a): a running head of a section with `tbRl` is across, from the paper's left margin, its baseline `header` + 10.56 pt from the top.
fn running_head_stays_across() {
    let head = r#"<w:p><w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:t>ふへほ</w:t></w:r></w:p>"#;
    let t = texts(&pages_of(&make_with_head("d6a", &para("あいう", "", ""), true, Some(head))));
    let item = t.iter().find(|i| i.2.contains('ふ')).unwrap_or_else(|| panic!("no running head: {t:?}"));
    near(item.3[0], 72.0, 0.3, "the head starts at the left margin");
    near(item.0, 36.0 + 10.56, 0.3, "the head's baseline");
    assert!(item.3.windows(2).all(|w| w[1] > w[0]), "across: its characters go right");
}

/// Item 3 (D3a): U+205F is 4/18 em wide (2.330 pt of 10.5 pt text), U+202F 0.2 em, in a line and in a column.
fn medium_mathematical_space() {
    for vertical in [false, true] {
        let d = drawn(&pages_of(&make_in("d3a-205f", &para("あ", r#"<w:r><w:t xml:space="preserve">え&#x205F;お</w:t></w:r>"#, ""), vertical)));
        let (e, o) = (d.iter().find(|i| i.2 == "え").unwrap(), d.iter().find(|i| i.2 == "お").unwrap());
        let along = if vertical { o.1 - e.1 } else { o.0 - e.0 };
        near(along, 10.56 + 10.5 * 4.0 / 18.0, 0.25, &format!("え to お across U+205F (vertical: {vertical})"));
    }
}

/// 圏点 (`w:em`) in a column (many-patterns 3, Word's XPS of ＭＳ 明朝 at 8..36 pt): the mark's box centre is 0.3955 (dot, comma), 0.352 (circle) and
/// -0.889 (underDot) of the size right of the column's centre, its baseline 0.324, 0.474 and 0.294 of the size below the base's baseline (box top
/// 0.535, 0.385 and 0.565 above it). The ring's box centre depends on the face's ring (Word's has half the em as its advance), so only its place along
/// the column is checked.
fn emphasis_marks_in_a_column() {
    for (half_points, size) in [(21, 10.5f32), (28, 14.0), (48, 24.0)] {
        for (em, mark, across, below) in [("dot", "．", Some(0.3955f32), 0.324f32), ("comma", "、", Some(0.3955), 0.324), ("circle", "˚", None, 0.474), ("underDot", "．", Some(-0.889), 0.294)] {
            let run = |t: &str, em: &str| format!(r#"<w:r><w:rPr>{em}<w:sz w:val="{half_points}"/></w:rPr><w:t>{t}</w:t></w:r>"#);
            let body = format!(
                r#"<w:p><w:pPr><w:jc w:val="left"/></w:pPr>{}{}{}</w:p>"#,
                run("あ", ""),
                run("い", &format!(r#"<w:em w:val="{em}"/>"#)),
                run("う", "")
            );
            let d = drawn(&pages_of(&make(&format!("em-{em}-{half_points}"), &body)));
            let find = |c: &str| d.iter().find(|i| i.2 == c).unwrap_or_else(|| panic!("{em} {size}: no {c}: {d:?}")).clone();
            let (a, i, m) = (find("あ"), find("い"), find(mark));
            near(m.1 - i.1, below * size, 0.2 + 0.01 * size, &format!("{em} {size} pt: the mark's baseline below the base's"));
            if let Some(across) = across {
                near((m.0 + size / 2.0) - (a.0 + size / 2.0), across * size, 0.2 + 0.01 * size, &format!("{em} {size} pt: the mark's centre from the column's"));
            }
        }
    }
}

/// What stands in a column of vertical text goes by a whole em, except ‘ ’ “ ” in ＭＳ Ｐ明朝 and ＭＳ Ｐゴシック and ‘ “ in メイリオ, which go by the face's width
/// (Windows Word's PDF, 10.5 pt: ＭＳ Ｐ明朝 5.3 pt between the quotes, あ to い 31.2 pt; メイリオ ‘ 2.72 pt and ’ 10.5 pt; 游明朝 and ＭＳ 明朝 10.5 pt each).
fn quotes_in_ms_p_mincho() {
    let run = |font: &str, text: &str| format!(r#"<w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:eastAsia="{font}" w:hint="eastAsia"/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#);
    // (the typeface, the character, the advance in em: None is a whole em)
    let cases: [(&str, char, Option<f32>); 14] = [
        ("ＭＳ Ｐ明朝", '‘', Some(0.5)), ("ＭＳ Ｐ明朝", '”', Some(0.5)), ("ＭＳ Ｐゴシック", '“', Some(0.5)), ("ＭＳ Ｐゴシック", '’', Some(0.5)),
        ("ＭＳ Ｐ明朝", '†', None), ("ＭＳ Ｐ明朝", '‼', None), ("ＭＳ Ｐ明朝", '§', None),
        ("メイリオ", '‘', Some(0.259)), ("メイリオ", '“', Some(0.449)), ("メイリオ", '’', None), ("メイリオ", '”', None),
        ("游明朝", '‘', None), ("游明朝", '”', None), ("ＭＳ 明朝", '’', None),
    ];
    for (n, (font, c, em)) in cases.iter().enumerate() {
        let body = format!(r#"<w:p><w:pPr><w:jc w:val="left"/></w:pPr>{}</w:p>"#, run(font, &format!("あ{c}{c}{c}{c}い")));
        let d = drawn(&pages_of(&make(&format!("up{n}"), &body)));
        let y = |ch: &str, k: usize| d.iter().filter(|i| i.2 == ch).nth(k).unwrap_or_else(|| panic!("{ch}: {d:?}")).1;
        let cs = c.to_string();
        let step = (y(&cs, 3) - y(&cs, 0)) / 3.0;
        near(step, em.unwrap_or(1.0) * 10.5, 0.3, &format!("{font} {c}: the advance"));
    }
}

fn dump() {
    let Ok(file) = std::env::var("I188_FILE") else { return };
    let p = pages_of(&file);
    for (n, page) in p.pages.iter().enumerate() {
        println!("--- page {n}");
        for it in &page.items {
            match it {
                Item::Text { x, y, text, offsets, size, .. } => {
                    let xs: Vec<String> = offsets.iter().map(|o| format!("{:.1}", x + o)).collect();
                    println!("T x={x:.1} y={y:.1} sz={size:.2} {text} [{}]", xs.join(","));
                }
                Item::Turned { x, y, text, turns, size, .. } => println!("R x={x:.1} y={y:.1} sz={size:.2} turns={turns} {text}"),
                Item::Scaled { x, y, text, size, xscale, .. } => println!("S x={x:.1} y={y:.1} sz={size:.2} xs={xscale} {text}"),
                Item::Stroke { x, y, x2, y2, w, .. } => println!("L ({x:.1},{y:.1})-({x2:.1},{y2:.1}) w={w}"),
                _ => {}
            }
        }
    }
}
