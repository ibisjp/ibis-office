// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #54, the room of a 傍点 (`w:em`) in a horizontal line (`emphasis_adjust`). Windows Word's XPS of 746 patterns (Microsoft 365, 2026-11-14,
//! `tasks/doc/requests/2026-11-14-54-rest-many-patterns-result.md`): the room `e` is a quarter of the line of the typeface (`H`: ＭＳ 332/256 em, 游 1.3 x 2636/2048,
//! メイリオ 1.95, BIZ UDP 1.3), taken by the room the line spacing has beyond `H` (`s >= e`) or else added to the line; an exact line does not move.
//! Each document is three lines of one paragraph (`w:br`), the second with its three characters marked (or the third, or all), and a paragraph after it. The tolerance
//! is 0.2 pt (Word's XPS has 0.12 pt steps; the values are Word's feeds, in the result's tables A, C, D, D2).

use std::sync::Arc;

use super::flow_layout::{Item, Pagination};
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue54_room_single_dot_line_takes_a_quarter_of_the_typefaces_line", single_dot as fn()),
        ("docx::issue54_room_single_under_dot_takes_it_below_the_line", single_under_dot),
        ("docx::issue54_room_circle_and_comma_are_dots_over_the_line", circle_and_comma),
        ("docx::issue54_room_is_the_lines_of_the_largest_character_not_the_marked_one", smaller_marked_run),
        ("docx::issue54_room_a_multiple_of_1_25_or_more_takes_it", multiple_takes_it),
        ("docx::issue54_room_a_multiple_under_1_25_adds_it", multiple_adds_it),
        ("docx::issue54_room_at_least_line_takes_it_when_it_has_e_to_spare", at_least),
        ("docx::issue54_room_exact_line_does_not_move", exact),
        ("docx::issue54_room_a_multiple_of_0_8_is_a_quarter_of_the_multiple", multiple_below_one),
        ("docx::issue54_room_line_grid_takes_it_when_it_has_e_to_spare", grid),
        ("docx::issue54_room_line_grid_places_the_absorbed_line_by_h_plus_2e", grid_baseline),
        ("docx::issue54_room_a_line_with_dot_and_under_dot_takes_two_sides", both_sides),
        ("docx::issue54_room_under_dot_of_the_last_line_moves_the_next_paragraph", next_paragraph),
        ("docx::issue54_room_every_marked_line_adds_it", every_line),
    ]
}

/// A paragraph of three lines (the three kana ones of `first`) with the marks of `marks` on lines 1 to 3 (`dot`, `underDot`, ... or "" for none), and a paragraph after.
/// `font`, `size` (half-points), `spacing` (the attributes of `w:spacing`, "" for single), `grid` (a `w:docGrid` linePitch in twips, 0 for none).
fn make(name: &str, font: &str, half_points: u32, spacing: &str, grid: u32, marks: [&str; 3]) -> String {
    make_with(name, font, half_points, spacing, grid, marks, half_points)
}

/// The same with the marked run of `mark_half_points`.
fn make_with(name: &str, font: &str, half_points: u32, spacing: &str, grid: u32, marks: [&str; 3], mark_half_points: u32) -> String {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let settings = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{w}"><w:characterSpacingControl w:val="doNotCompress"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#);
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{w}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:eastAsia="{font}" w:cs="{font}"/><w:sz w:val="{half_points}"/><w:szCs w:val="{half_points}"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#);
    let run = |text: &str, mark: &str, last: bool| {
        // "dot+underDot": the first character has the first mark, the others the second (a line with both sides).
        if let Some((m1, m2)) = mark.split_once('+') {
            let mut cs = text.chars();
            let first = cs.next().map(String::from).unwrap_or_default();
            let rest: String = cs.collect();
            let rpr = |m: &str| format!(r#"<w:rPr><w:em w:val="{m}"/><w:sz w:val="{mark_half_points}"/><w:szCs w:val="{mark_half_points}"/></w:rPr>"#);
            return format!(r#"<w:r>{}<w:t>{first}</w:t></w:r><w:r>{}<w:t>{rest}</w:t></w:r>{}"#, rpr(m1), rpr(m2), if last { "" } else { "<w:r><w:br/></w:r>" });
        }
        let rpr = if mark.is_empty() { String::new() } else { format!(r#"<w:rPr><w:em w:val="{mark}"/><w:sz w:val="{mark_half_points}"/><w:szCs w:val="{mark_half_points}"/></w:rPr>"#) };
        format!(r#"<w:r>{rpr}<w:t>{text}</w:t></w:r>{}"#, if last { "" } else { "<w:r><w:br/></w:r>" })
    };
    let spacing_xml = if spacing.is_empty() { String::new() } else { format!(r#"<w:spacing {spacing}/>"#) };
    let lines = [("あ", "いうえ", "お"), ("か", "きくけ", "こ"), ("さ", "しすせ", "そ")];
    let mut para = String::new();
    for (k, (a, m, b)) in lines.iter().enumerate() {
        para.push_str(&format!(r#"<w:r><w:t>{a}</w:t></w:r>{}<w:r><w:t>{b}</w:t></w:r>{}"#, run(m, marks[k], true), if k < 2 { "<w:r><w:br/></w:r>" } else { "" }));
    }
    let snap = if grid > 0 { format!(r#"<w:docGrid w:type="lines" w:linePitch="{grid}"/>"#) } else { String::new() };
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body><w:p><w:pPr>{spacing_xml}</w:pPr>{para}</w:p><w:p><w:pPr>{spacing_xml}</w:pPr><w:r><w:t>たちつてと</w:t></w:r></w:p><w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>{snap}</w:sectPr></w:body></w:document>"#
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
    std::fs::write(dir.join(format!("issue54room-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue54room-{name}.docx")
}

fn pages_of(file: &str) -> Arc<Pagination> {
    let mut s = Scenario::open(file);
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

/// The baselines of the first kana of the three lines and of the paragraph after (あ か さ た).
fn baselines(file: &str) -> [f32; 4] {
    let p = pages_of(file);
    let mut out = [0.0f32; 4];
    for (k, c) in ['あ', 'か', 'さ', 'た'].into_iter().enumerate() {
        out[k] = p.pages[0].items.iter().find_map(|it| match it {
            Item::Text { y, text, .. } if text.starts_with(c) => Some(*y),
            _ => None,
        }).unwrap_or_else(|| panic!("no {c} in {file}"));
    }
    out
}

fn near(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() <= 0.2, "{what}: {a:.2} against Word's {b:.2}");
}

/// A line's `H` of the typeface at `size` pt, in pt.
const MS: f32 = 332.0 / 256.0;
const YU: f32 = 1.3 * 2636.0 / 2048.0;
const MEIRYO: f32 = 1.95;
const BIZ: f32 = 1.3;

const TYPEFACES: [(&str, f32); 4] = [("ＭＳ 明朝", MS), ("游明朝", YU), ("メイリオ", MEIRYO), ("BIZ UDP明朝", BIZ)];
const SIZES: [u32; 4] = [16, 21, 28, 48];

/// Table C: the second line marked with a dot over it: its baseline is `e` below where it was and the third line is `e` further down than without (single line,
/// every typeface and size: `e = H / 4`).
fn single_dot() {
    for (font, h) in TYPEFACES {
        for hp in SIZES {
            let size = hp as f32 / 2.0;
            let b = baselines(&make(&format!("sd-{font}-{hp}"), font, hp, "", 0, ["", "dot", ""]));
            let (h, e) = (h * size, 0.25 * h * size);
            near(b[1] - b[0], h + e, &format!("{font} {size} pt: line 1 to 2"));
            near(b[2] - b[1], h, &format!("{font} {size} pt: line 2 to 3"));
        }
    }
}

/// Table C: with the dot under the characters the line itself stays and the next line is `e` further down.
fn single_under_dot() {
    for (font, h) in TYPEFACES {
        for hp in SIZES {
            let size = hp as f32 / 2.0;
            let b = baselines(&make(&format!("su-{font}-{hp}"), font, hp, "", 0, ["", "underDot", ""]));
            let (h, e) = (h * size, 0.25 * h * size);
            near(b[1] - b[0], h, &format!("{font} {size} pt underDot: line 1 to 2"));
            near(b[2] - b[1], h + e, &format!("{font} {size} pt underDot: line 2 to 3"));
        }
    }
}

/// Table A: the four marks take the same room.
fn circle_and_comma() {
    for mark in ["comma", "circle"] {
        let b = baselines(&make(&format!("cc-{mark}"), "ＭＳ 明朝", 21, "", 0, ["", mark, ""]));
        near(b[1] - b[0], MS * 10.5 * 1.25, mark);
        near(b[2] - b[1], MS * 10.5, mark);
    }
}

/// Table E: an 8 pt marked run in a 10.5 pt line takes the quarter of the 10.5 pt line (the largest character of the line), 3.36 pt in Word.
fn smaller_marked_run() {
    let b = baselines(&make_with("small", "ＭＳ 明朝", 21, "", 0, ["", "dot", ""], 16));
    near(b[1] - b[0], MS * 10.5 + 0.25 * MS * 10.5, "line 1 to 2 (a marked 8 pt run in 10.5 pt text)");
}

/// Table D and D2: a multiple of 1.25 and more has `0.25 H` to spare: the line is as high as it was; the baseline of the dots is `e` lower (the room is
/// below it), underDot's stays. 10.5 pt ＭＳ 明朝: 1.5 lines, the dot line 3.48 lower, 2 lines 3.36.
fn multiple_takes_it() {
    let h = MS * 10.5;
    for (m, twips) in [(1.25, 300), (1.5, 360), (2.0, 480)] {
        let spacing = format!(r#"w:line="{twips}" w:lineRule="auto""#);
        let dot = baselines(&make(&format!("mt-{twips}"), "ＭＳ 明朝", 21, &spacing, 0, ["", "dot", ""]));
        near(dot[2] - dot[0], 2.0 * m * h, &format!("x{m}: the two lines are as high as without"));
        near(dot[1] - dot[0], m * h + 0.25 * h, &format!("x{m}: the dot line's baseline is e lower"));
        let under = baselines(&make(&format!("mu-{twips}"), "ＭＳ 明朝", 21, &spacing, 0, ["", "underDot", ""]));
        near(under[1] - under[0], m * h, &format!("x{m}: underDot keeps the baseline"));
        near(under[2] - under[0], 2.0 * m * h, &format!("x{m}: underDot takes it"));
    }
}

/// D2: 1.1 and 1.2 lines have less than `e` to spare: the line is `P + e` (10.5 pt: +3.48, +3.36).
fn multiple_adds_it() {
    let h = MS * 10.5;
    for (m, twips) in [(1.1, 264), (1.2, 288)] {
        let spacing = format!(r#"w:line="{twips}" w:lineRule="auto""#);
        let b = baselines(&make(&format!("ma-{twips}"), "ＭＳ 明朝", 21, &spacing, 0, ["", "dot", ""]));
        near(b[2] - b[0], 2.0 * m * h + 0.25 * h, &format!("x{m}: both lines and e"));
    }
}

/// D2: an `atLeast` line takes `e` when `V - H >= e` (10.5 pt: V 18 pt and more, not 12 to 16 pt), putting the dots' room above it
/// (the baseline stays) and lifting the baseline of underDot by `e`; 14 pt V 20 pt does not have it (+4.44 for both).
fn at_least() {
    let h = MS * 10.5;
    let e = 0.25 * h;
    let spacing = |v: u32| format!(r#"w:line="{}" w:lineRule="atLeast""#, v * 20);
    // V = 20 pt at 10.5 pt: V - H = 6.4 >= e.
    let dot = baselines(&make("al-20-dot", "ＭＳ 明朝", 21, &spacing(20), 0, ["", "dot", ""]));
    near(dot[1] - dot[0], 20.0, "atLeast 20, dot: the baseline stays");
    near(dot[2] - dot[1], 20.0, "atLeast 20, dot: the next line");
    let under = baselines(&make("al-20-under", "ＭＳ 明朝", 21, &spacing(20), 0, ["", "underDot", ""]));
    near(under[1] - under[0], 20.0 - e, "atLeast 20, underDot: the baseline is lifted by e");
    // V = 14 pt at 10.5 pt: V - H = 0.4 < e: the line is V + e.
    let dot = baselines(&make("al-14-dot", "ＭＳ 明朝", 21, &spacing(14), 0, ["", "dot", ""]));
    near(dot[2] - dot[0], 28.0 + e, "atLeast 14: e added");
    // 14 pt, V = 20 pt: H = 18.16, V - H = 1.84 < 4.54.
    let dot = baselines(&make("al-14p", "ＭＳ 明朝", 28, &spacing(20), 0, ["", "dot", ""]));
    near(dot[2] - dot[0], 40.0 + 0.25 * MS * 14.0, "atLeast 20 at 14 pt: e added");
}

/// D: an exact line does not move for 傍点 (20 pt line).
fn exact() {
    for mark in ["dot", "underDot"] {
        let b = baselines(&make(&format!("ex-{mark}"), "ＭＳ 明朝", 21, r#"w:line="400" w:lineRule="exact""#, 0, ["", mark, ""]));
        near(b[1] - b[0], 20.0, mark);
        near(b[2] - b[1], 20.0, mark);
    }
}

/// D2: a multiple of 0.8 takes `0.25 x 0.8 H` (10.5 pt: +2.76 in Word, 2.72 by the rule; 14 pt: +3.60, 3.63).
fn multiple_below_one() {
    for hp in [21u32, 28] {
        let h = MS * hp as f32 / 2.0;
        let b = baselines(&make(&format!("m8-{hp}"), "ＭＳ 明朝", hp, r#"w:line="192" w:lineRule="auto""#, 0, ["", "dot", ""]));
        near(b[2] - b[0], 2.0 * 0.8 * h + 0.25 * 0.8 * h, &format!("{hp} half-points"));
    }
}

/// D2 (grid 18 pt, snap on): 10.5 pt has 18 - 13.62 = 4.4 >= 3.4 to spare: the lines are 18 pt as without; 12 pt (18 - 15.6 = 2.4 < 3.9) is 18 + e (21.84).
/// (Where the baseline of the absorbed line is, `A_f + p`, is not explained yet and not tested.)
fn grid() {
    let b = baselines(&make("gr-10", "ＭＳ 明朝", 21, "", 360, ["", "dot", ""]));
    near(b[2] - b[0], 36.0, "10.5 pt on the grid: two lines of 18");
    let b = baselines(&make("gr-12", "ＭＳ 明朝", 24, "", 360, ["", "dot", ""]));
    near(b[2] - b[0], 36.0 + 0.25 * MS * 12.0, "12 pt on the grid: e added to the grid line");
    let b = baselines(&make("gr-11u", "ＭＳ 明朝", 22, "", 360, ["", "underDot", ""]));
    near(b[2] - b[0], 36.0, "11 pt underDot on the grid: absorbed (3.73 >= 3.57)");
}

/// B1 (Word's XPS, 2026-11-16, 96 patterns): a line with a dot over a character and an underDot under another has `n = 2` sides: off a grid H + 2e (the baseline e down, the next line 2e), on the
/// grid (18 pt) 14 pt ＭＳ 明朝 is absorbed (27.3 <= 36: the baseline -4.44 as the underDot's, `p` stays 0) and 10.5 pt is not (20.4 > 18: 18 + 2e, the baseline +e).
fn both_sides() {
    let e = 0.25 * MS * 14.0;
    let plain = baselines(&make("bs-p", "ＭＳ 明朝", 28, "", 0, ["", "", ""]));
    let b = baselines(&make("bs-0", "ＭＳ 明朝", 28, "", 0, ["", "dot+underDot", ""]));
    near(b[1] - plain[1], e, "the baseline of the marked line is e down");
    near(b[2] - plain[2], 2.0 * e, "the next line is 2e down");
    let plain = baselines(&make("bs-gp", "ＭＳ 明朝", 28, "", 360, ["", "", ""]));
    let b = baselines(&make("bs-g", "ＭＳ 明朝", 28, "", 360, ["", "dot+underDot", ""]));
    near(b[1] - plain[1], -4.44, "14 pt on the grid: absorbed, the baseline as the underDot's");
    near(b[2] - plain[2], 0.0, "14 pt on the grid: the next line does not move");
}

/// D: on the grid (360 = 18 pt) where the room is taken, the baseline of the marked line against the unmarked one (ＭＳ 明朝; Word's XPS: size, dot, underDot).
fn grid_baseline() {
    let rows: [(u32, f32, f32); 10] = [(16, 0.0, -2.64), (18, 0.0, -2.88), (20, 0.72, -2.52), (21, 1.20, -2.28), (22, 1.80, -1.80), (28, 0.0, -4.44), (30, 0.0, -4.92), (32, 0.0, -5.28), (36, 0.0, -5.88), (40, 1.44, -5.04)];
    for (hp, dot, under) in rows {
        let plain = baselines(&make(&format!("gb-{hp}"), "ＭＳ 明朝", hp, "", 360, ["", "", ""]));
        let d = baselines(&make(&format!("gb-{hp}d"), "ＭＳ 明朝", hp, "", 360, ["", "dot", ""]));
        let u = baselines(&make(&format!("gb-{hp}u"), "ＭＳ 明朝", hp, "", 360, ["", "underDot", ""]));
        near(d[1] - plain[1], dot, &format!("{hp} half-points dot"));
        near(u[1] - plain[1], under, &format!("{hp} half-points underDot"));
    }
}

/// C: a mark on the third line's last 3 characters (underDot) moves the next paragraph down by `e` (4.32 pt at 14 pt in Word).
fn next_paragraph() {
    for (font, h) in TYPEFACES {
        let b = baselines(&make(&format!("np-{font}"), font, 28, "", 0, ["", "", "underDot"]));
        near(b[3] - b[2], h * 14.0 + 0.25 * h * 14.0, font);
    }
}

/// C: marks on the three lines make the three of them `e` higher (3e in all).
fn every_line() {
    let h = MS * 14.0;
    let b = baselines(&make("all", "ＭＳ 明朝", 28, "", 0, ["dot", "dot", "dot"]));
    near(b[2] - b[0], 2.0 * (h + 0.25 * h), "lines 1 to 3");
    near(b[3] - b[2], h, "the paragraph after");
}

/// A tool (ignored; `--ignored`): the page count of each document of the list in the file `IBIS_PAGES_LIST` (a path a line), `pages\t<path>\t<n>`.
pub(crate) fn page_counts() {
    let list = std::fs::read_to_string(std::env::var("IBIS_PAGES_LIST").expect("IBIS_PAGES_LIST")).expect("the list");
    for path in list.lines().filter(|l| !l.trim().is_empty() && !l.contains("encrypted")) {
        let n = std::panic::catch_unwind(|| pages_of(path).pages.len());
        println!("pages\t{path}\t{}", n.map_or("panic".to_string(), |n| n.to_string()));
    }
}
