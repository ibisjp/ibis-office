// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #54, corpus 016: how much a line squeezes its punctuation (`compressPunctuation`) and the automatic space between Japanese and Latin
//! letters before it breaks. The numbers are Word's: `tasks/doc/requests/2026-11-17-54-compress-punctuation-result.tsv` has, for each face, size, compatibility
//! mode, `characterSpacingControl`, `w:kern`, alignment and string, the least text width in twips at which Word (its XPS export) sets the string in one line, the
//! string alone in a one-cell table (cell margins 57 and 57, a right indent of 34). Each scenario here sets the same strings in the same cell and finds the least
//! width at which ibisOffice sets one line, and compares it with Word's.

use super::flow_layout::Item;
use crate::ext::*;
use crate::shell::scenario::Scenario;
use super::issue191_tests::make_control;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue54_punct_016_is_one_line_at_the_width_word_sets_it", the_016_cell_is_one_line_at_words_width),
        ("docx::issue54_punct_least_widths_of_words_meiryo_strings", least_widths_of_words_meiryo_strings),
        ("docx::issue54_punct_when_a_line_squeezes", when_a_line_squeezes),
        ("docx::issue54_punct_proportional_faces_squeeze_only_their_spaces", proportional_faces_squeeze_only_their_spaces),
        ("docx::issue54_punct_kana_are_squeezed_with_the_kana_setting", kana_are_squeezed_with_the_kana_setting),
        ("docx::issue54_punct_the_total_follows_the_last_unit", the_total_follows_the_last_unit),
        ("docx::issue54_punct_the_hanging_and_the_start_of_a_line", the_hanging_and_the_start_of_a_line),
        ("docx::issue54_punct_the_paragraph_settings_decide_the_total", the_paragraph_settings_decide_the_total),
        ("docx::issue54_punct_a_justified_paragraph_of_mode_15_is_one_of_mode_14", a_justified_paragraph_of_mode_15_is_one_of_mode_14),
        ("docx::issue54_punct_lines_and_chars_squeezes_by_the_pitch", lines_and_chars_squeezes_by_the_pitch),
        ("docx::issue54_punct_report", report),
    ]
}

/// One row of Word's table: the configuration, the string and the least width in twips.
struct Row {
    set: String,
    layout: String,
    config: String,
    string: String,
    word: i64,
}

fn rows() -> Vec<Row> {
    // The first round's table, then the second's (`2026-11-17-54-punct-2-result.tsv`): the same columns.
    let mut out = Vec::new();
    for file in ["2026-11-17-54-compress-punctuation-result.tsv", "2026-11-17-54-punct-2-result.tsv", "2026-11-17-54-punct-3-result.tsv", "2026-11-18-54-grid-result.tsv"] {
        let path = crate::shell::scenario::repo_root().join("tasks/doc/requests").join(file);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut lines = text.lines();
        let header: Vec<&str> = lines.next().expect("a header").split('\t').collect();
        let col = |name: &str| header.iter().position(|h| *h == name).unwrap_or_else(|| panic!("no column {name}"));
        let (set, config, string, min, layout) = (col("set"), col("config"), col("string"), col("min_text_width_twips"), col("layout"));
        out.extend(lines.filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            Some(Row { set: f.get(set)?.to_string(), layout: f.get(layout)?.to_string(), config: f.get(config)?.to_string(), string: f.get(string)?.to_string(), word: f.get(min)?.parse().ok()? })
        }));
    }
    out
}

/// The `w:docGrid` of a name's `__<grid>` (`lcx_360`: linesAndChars with no charSpace, `lc4068_360`, `lcm1730_360` the same with a charSpace of 4068, −1730; `sc…` snapToChars; `l360`: lines;
/// `none`): `None` for what is not made here (`_sn0`, an indent in the name).
fn grid_xml(tag: &str) -> Option<String> {
    if tag == "none" {
        return Some(String::new());
    }
    if let Some(pitch) = tag.strip_prefix('l').and_then(|p| p.parse::<u32>().ok()) {
        return Some(format!(r#"<w:docGrid w:type="lines" w:linePitch="{pitch}"/>"#));
    }
    let (kind, pitch) = tag.split_once('_')?;
    let pitch: u32 = pitch.parse().ok()?;
    let (ty, rest) = if let Some(r) = kind.strip_prefix("lc") { ("linesAndChars", r) } else if let Some(r) = kind.strip_prefix("sc") { ("snapToChars", r) } else { return None };
    let space = match rest {
        "x" => String::new(),
        r if r.starts_with('m') => format!(r#" w:charSpace="-{}""#, r.trim_start_matches('m').parse::<u32>().ok()?),
        r => format!(r#" w:charSpace="{}""#, r.parse::<u32>().ok()?),
    };
    Some(format!(r#"<w:docGrid w:type="{ty}" w:linePitch="{pitch}"{space}/>"#))
}

/// A configuration of Word's table, `meiryo_11_c14_cp_k2_center_on_w80`: face, size, compatibility mode, `characterSpacingControl`, `w:kern`, `jc`, the automatic
/// spaces, `w:w`.
struct Config {
    face: &'static str,
    size_half_points: u32,
    compat: u32,
    control: &'static str,
    kern: bool,
    jc: String,
    auto_space: bool,
    /// `autospace` `de0` / `dn0` (one of `w:autoSpaceDE`, `w:autoSpaceDN` off; `auto_space` false is both).
    auto_de: bool,
    auto_dn: bool,
    scale: u32,
    /// `ov0`, `ki0`, `ww0` in the name: `w:overflowPunct`, `w:kinsoku`, `w:wordWrap` switched off.
    overflow_off: bool,
    kinsoku_off: bool,
    word_wrap_off: bool,
    /// The section's `w:docGrid` (XML), from the `__<grid>` of the name.
    grid: String,
    /// The string is in a paragraph of the body (its right indent makes the width), not in a one-cell table.
    body: bool,
}

fn parse(config: &str) -> Option<Config> {
    let head = config.split("__").next()?;
    let grid_tag = config.split("__").nth(1).unwrap_or("none");
    let grid = grid_xml(grid_tag)?;
    let p: Vec<&str> = head.split('_').collect();
    if p.len() < 8 {
        return None;
    }
    let tags = &p[8..];
    let face = match p[0] {
        "meiryo" => "メイリオ",
        "meiryoui" => "Meiryo UI",
        "yugothic" => "游ゴシック",
        "yumincho" => "游明朝",
        "bizudg" => "BIZ UDゴシック",
        "bizudpg" => "BIZ UDPゴシック",
        "msmincho" => "ＭＳ 明朝",
        "mspmincho" => "ＭＳ Ｐ明朝",
        "msgothic" => "ＭＳ ゴシック",
        "mspgothic" => "ＭＳ Ｐゴシック",
        "bizudm" => "BIZ UD明朝 Medium",
        "bizudpm" => "BIZ UDP明朝 Medium",
        "hgmincho" => "HG明朝E",
        "hgpmincho" => "HGP明朝E",
        "hggothic" => "HGｺﾞｼｯｸM",
        "hgpgothic" => "HGPｺﾞｼｯｸM",
        _ => return None,
    };
    let control = match p[3] {
        "none" => "none",
        "dnc" => "doNotCompress",
        "cp" => "compressPunctuation",
        "cpk" => "compressPunctuationAndJapaneseKana",
        _ => return None,
    };
    Some(Config {
        face,
        size_half_points: (p[1].parse::<f32>().ok()? * 2.0) as u32,
        compat: p[2].strip_prefix('c')?.parse().ok()?,
        control,
        kern: p[4] != "k0",
        jc: p[5].to_string(),
        auto_space: p[6] == "on" || p[6] == "de0" || p[6] == "dn0",
        auto_de: p[6] != "de0",
        auto_dn: p[6] != "dn0",
        scale: p[7].strip_prefix('w')?.parse().ok()?,
        overflow_off: tags.contains(&"ov0"),
        kinsoku_off: tags.contains(&"ki0"),
        word_wrap_off: tags.contains(&"ww0"),
        grid,
        body: false,
    })
}

/// How many lines the string takes in a one-cell table whose text is `text_width` twips wide (the cell's margins 57 and 57, the paragraph's right indent 34).
fn lines_at(cfg: &Config, text: &str, text_width: i64) -> usize {
    let cell = text_width + 57 + 57 + 34;
    let face = cfg.face;
    let kern = if cfg.kern { r#"<w:kern w:val="2"/>"# } else { "" };
    let scale = if cfg.scale != 100 { format!(r#"<w:w w:val="{}"/>"#, cfg.scale) } else { String::new() };
    let mut ppr = String::new();
    if cfg.kinsoku_off {
        ppr.push_str(r#"<w:kinsoku w:val="0"/>"#);
    }
    if cfg.word_wrap_off {
        ppr.push_str(r#"<w:wordWrap w:val="0"/>"#);
    }
    if cfg.overflow_off {
        ppr.push_str(r#"<w:overflowPunct w:val="0"/>"#);
    }
    if !cfg.auto_space || !cfg.auto_de {
        ppr.push_str(r#"<w:autoSpaceDE w:val="0"/>"#);
    }
    if !cfg.auto_space || !cfg.auto_dn {
        ppr.push_str(r#"<w:autoSpaceDN w:val="0"/>"#);
    }
    let jc = &cfg.jc;
    let sz = cfg.size_half_points;
    // A paragraph of the body is as wide as the page's text (9026 twips) less its right indent; a cell's has its margins 57 and 57 and a right indent of 34.
    let indent = if cfg.body { (9026 - text_width).max(0) } else { 34 };
    let para = format!(
        r#"<w:p><w:pPr>{ppr}<w:spacing w:before="0" w:after="0"/><w:ind w:right="{indent}"/><w:jc w:val="{jc}"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="{face}" w:eastAsia="{face}" w:hAnsi="{face}" w:cs="{face}"/>{kern}{scale}<w:sz w:val="{sz}"/><w:szCs w:val="{sz}"/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#
    );
    let body = if cfg.body {
        format!("{para}<w:p/>")
    } else {
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="{cell}" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="57" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="57" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="{cell}"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="{cell}" w:type="dxa"/></w:tcPr>{para}</w:tc></w:tr></w:tbl><w:p/>"#
        )
    };
    // One file for each scenario: they may run side by side.
    let name = format!("punct-{:?}", std::thread::current().id()).replace(['(', ')'], "");
    let file = make_control(&name, &body, &cfg.grid, cfg.compat, cfg.control);
    let mut s = Scenario::open(&file);
    let pages = s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
    let mut ys: Vec<f32> = Vec::new();
    for it in &pages.pages[0].items {
        if let Item::Text { y, text, .. } | Item::Scaled { y, text, .. } = it
            && text.chars().any(|c| !c.is_whitespace())
            && !ys.iter().any(|v| (v - y).abs() < 0.05)
        {
            ys.push(*y);
        }
    }
    ys.len()
}

/// The least text width (twips) at which the string is one line, by bisection between a width that is certainly two lines and one that is certainly one.
fn least_width(cfg: &Config, text: &str) -> i64 {
    let (mut lo, mut hi) = (100i64, 5000i64);
    if lines_at(cfg, text, hi) != 1 {
        return -1;
    }
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if lines_at(cfg, text, mid) == 1 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

/// Word's least widths against ibisOffice's for the strings of `set`s in `configs`: (strings that agree within `tolerance`, all, the differences).
/// The string every grid configuration of Word's table has as its control (ten kana).
const GRID_CONTROL: &str = "あいうえおかきくけこ"; // i18n-ok（試験の入力）

fn compare(sets: &[&str], configs: &[&str], tolerance: i64) -> (usize, usize, Vec<String>) {
    let (mut same, mut all, mut out) = (0, 0, Vec::new());
    let every: usize = std::env::var("IBIS_PUNCT_EVERY").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let mut count = 0usize;
    for r in rows() {
        if !sets.contains(&r.set.as_str()) || !(configs.is_empty() || configs.contains(&r.config.as_str())) {
            continue;
        }
        if r.config.contains("_cNone_") || r.config.contains("_sn0") || r.config.contains("_wfirst") {
            continue; // (a document with no compatibility setting, a paragraph with `snapToGrid` 0 and an indent are not made here)
        }
        count += 1;
        if every > 1 && count % every != 0 {
            continue;
        }
        let Some(mut cfg) = parse(&r.config) else { continue };
        cfg.body = r.layout.starts_with("indent");
        let mut ours = least_width(&cfg, &r.string);
        let mut word = r.word;
        // A grid's least width has an extra of its own (Word's control string, ten kana, is wider than its natural width): the strings are compared with the control's.
        if !cfg.grid.is_empty() {
            let Some(control) = rows().into_iter().find(|c| c.set == r.set && c.config == r.config && c.string == GRID_CONTROL) else { continue };
            if r.string == GRID_CONTROL {
                continue;
            }
            ours -= least_width(&cfg, GRID_CONTROL);
            word -= control.word;
        }
        all += 1;
        if (ours - word).abs() <= tolerance {
            same += 1;
        } else {
            out.push(format!("{}\t{}\tWord {}\tours {}\t({:+})", r.config, r.string, word, ours, ours - word));
        }
    }
    (same, all, out)
}

/// Corpus 016's cell: 「（1時間休憩）」 in Meiryo 11 pt, `w:w` 80, compatibility mode 14, `compressPunctuation`, `w:kern` 2, centred, is one line in a text width of
/// 1100 twips in Word (the cell of 1248 twips with its margins and indent; Word set 016 in 1250).
fn the_016_cell_is_one_line_at_words_width() {
    let cfg = parse("meiryo_11_c14_cp_k2_center_on_w80").unwrap();
    let ours = least_width(&cfg, "（1時間休憩）");
    let word = rows().into_iter().find(|r| r.config == "meiryo_11_c14_cp_k2_center_on_w80" && r.string == "（1時間休憩）" && r.set == "p1_cell").expect("Word's row").word;
    assert!((ours - word).abs() <= 2, "the least width of 「（1時間休憩）」: ours {ours}, Word {word}");
}

/// Word's first set of strings (closing brackets, stops, the space between Japanese and Latin letters at the end of a line, brackets in the middle of one) in Meiryo 11 pt at
/// 80% (`w:w`), compatibility mode 14, `compressPunctuation`: every one agrees with Word's least width, the 016 cell's.
fn least_widths_of_words_meiryo_strings() {
    let (same, all, diffs) = compare(&["p1_cell"], &["meiryo_11_c14_cp_k2_center_on_w80"], 4);
    assert!(all >= 30 && same == all, "{same} of {all} agree with Word's:\n{}", diffs.join("\n"));
}

/// When a line squeezes at all: in compatibility modes 12 and 14 with `compressPunctuation` (and with the kana too), not with `none` or `doNotCompress`, not in mode 15
/// (Word's table, the same strings in each configuration).
fn when_a_line_squeezes() {
    const STRINGS: [&str; 8] = ["（1時間休憩）", "1時間休憩", "第1章", "（1時間休憩）ABC", "時間（1休憩）時間", "時間）（時間", "（1時間休憩）あ", "1時間休憩（"];
    let configs = [
        "meiryo_11_c14_cp_k2_center_on_w100",
        "meiryo_11_c12_cp_k2_center_on_w100",
        "meiryo_11_c14_cpk_k2_center_on_w100",
        "meiryo_11_c14_none_k2_center_on_w100",
        "meiryo_11_c14_dnc_k2_center_on_w100",
        "meiryo_11_c15_cp_k2_center_on_w100",
    ];
    let mut diffs = Vec::new();
    let mut all = 0;
    for r in rows() {
        if r.set != "p3_cell" || !configs.contains(&r.config.as_str()) || !STRINGS.contains(&r.string.as_str()) {
            continue;
        }
        let cfg = parse(&r.config).unwrap();
        let ours = least_width(&cfg, &r.string);
        all += 1;
        if (ours - r.word).abs() > 4 {
            diffs.push(format!("{} {}: Word {}, ours {ours}", r.config, r.string, r.word));
        }
    }
    assert!(all >= 40 && diffs.is_empty(), "{} of {all} differ from Word's:\n{}", diffs.len(), diffs.join("\n"));
}

/// The proportional Japanese faces (Meiryo UI, BIZ UDPゴシック) squeeze no stop and no bracket, only the space between Japanese and Latin letters (Word's table, the first set
/// of strings in each: `（時間休憩）` and `（ABC）` save almost nothing, `（1時間休憩）` and `1時間休憩（` half of a quarter em).
fn proportional_faces_squeeze_only_their_spaces() {
    const STRINGS: [&str; 12] = ["（1時間休憩）", "（時間休憩）", "（ABC）", "1時間休憩", "第1章", "（1時間休憩）あ", "1時間休憩（", "時間（1休憩）時間", "時間）（時間", "時間、1時間", "（1時間休憩）ABC", "1時間休憩、、"];
    let mut diffs = Vec::new();
    let mut all = 0;
    for r in rows() {
        if r.set != "p2_cell" || !(r.config == "meiryoui_11_c14_cp_k2_center_on_w100" || r.config == "bizudpg_11_c14_cp_k2_center_on_w100") || !STRINGS.contains(&r.string.as_str()) {
            continue;
        }
        let ours = least_width(&parse(&r.config).unwrap(), &r.string);
        all += 1;
        if (ours - r.word).abs() > 4 {
            diffs.push(format!("{} {}: Word {}, ours {ours}", r.config, r.string, r.word));
        }
    }
    assert!(all >= 20 && diffs.is_empty(), "{} of {all} differ from Word's:\n{}", diffs.len(), diffs.join("\n"));
}

/// With `compressPunctuationAndJapaneseKana` a line gives a sixteenth of an em after each kana (and full-width digit), half an em at most, after the stops, brackets and
/// spaces (Word's table, set k1, `compressPunctuation` against the same strings: `ああ` 2/16 em, ten kana 0.5 em, kanji none, the long vowel mark none).
fn kana_are_squeezed_with_the_kana_setting() {
    let mut diffs = Vec::new();
    let mut all = 0;
    for r in rows() {
        if r.set != "k1_cell" || r.config != "meiryo_11_c14_cpk_k2_center_on_w100" || r.string.contains('ー') {
            continue;
        }
        let ours = least_width(&parse(&r.config).unwrap(), &r.string);
        all += 1;
        if (ours - r.word).abs() > 4 {
            diffs.push(format!("{}: Word {}, ours {ours}", r.string, r.word));
        }
    }
    assert!(all >= 20 && diffs.is_empty(), "{} of {all} differ from Word's:\n{}", diffs.len(), diffs.join("\n"));
}

/// What a line may give up in all, brackets, spaces and kana together, follows the unit that goes to the next line (Word's second table, sets a1 to a4: n opening brackets, n closing
/// ones, brackets of both kinds and the pairs `）（`, a line with the spaces and the kana, each with the last unit a kana, a kanji, a digit, a Latin word or a bracket): a
/// sample of every eighth row of the four sets agrees with Word's least width, but for the rows named in `known`, where the last unit is two closing brackets, a pair of them
/// after six more, or a line whose total Word's table does not explain yet.
fn the_total_follows_the_last_unit() {
    let known = |s: &str| s.ends_with("））") || s.ends_with("（）") || s.ends_with("）（") || s.contains("）（）（）（）（）（）") || s.contains("、（、（、（、（、（、（") || s.contains("。「。「。「。「。「。「") || s.contains("）「）「）「）「）「）「") || s.contains("」（」（」（」（」（」（") || s.contains("い2う");
    let mut diffs = Vec::new();
    let mut all = 0;
    let mut n = 0;
    for r in rows() {
        if !["a1_cell", "a2_cell", "a3_cell", "a4_cell"].contains(&r.set.as_str()) || !(r.config == "meiryo_11_c14_cp_k2_center_on_w100" || r.config == "meiryo_11_c14_cpk_k2_center_on_w100") {
            continue;
        }
        n += 1;
        if n % 8 != 0 || known(&r.string) {
            continue;
        }
        let ours = least_width(&parse(&r.config).unwrap(), &r.string);
        all += 1;
        if (ours - r.word).abs() > 4 {
            diffs.push(format!("{} {}: Word {}, ours {ours}", r.config, r.string, r.word));
        }
    }
    assert!(all >= 40 && diffs.is_empty(), "{} of {all} differ from Word's:\n{}", diffs.len(), diffs.join("\n"));
}

/// Hanging punctuation and the start of a line (Word's second table, sets c1 and c2, a paragraph of the body with a right indent): below mode 15 a full-width closing bracket or stop at
/// the end of a paragraph hangs its whole width, whatever `w:wordWrap` says, and not with `w:overflowPunct` off (with `w:kinsoku` off Word's one-line paragraph hangs too, the corpus and the older
/// probes do not: not taken in); an ASCII comma or full stop at the paragraph's end keeps its
/// width; in mode 15 nothing hangs in a left aligned paragraph; a closing bracket or a stop that starts the line gives half an em. A sample of every fifth row of
/// the two sets (the rows with a colon, with `overflowPunct` and `kinsoku` both off, with `kinsoku` off in a document, with two opening brackets or a full-width space at the start of the line are not taken in).
fn the_hanging_and_the_start_of_a_line() {
    let known = |config: &str, s: &str| config.contains("ki0") || s.contains('：') || s.contains('；') || s.starts_with("（（") || s.starts_with("「（") || s.starts_with('\u{3000}');
    let mut diffs = Vec::new();
    let (mut all, mut n) = (0, 0);
    for r in rows() {
        if !(r.set == "c1_indent" || r.set == "c2_indent") {
            continue;
        }
        n += 1;
        if n % 5 != 0 || known(&r.config, &r.string) {
            continue;
        }
        let Some(mut cfg) = parse(&r.config) else { continue };
        cfg.body = true;
        let ours = least_width(&cfg, &r.string);
        all += 1;
        if (ours - r.word).abs() > 4 {
            diffs.push(format!("{} {}: Word {}, ours {ours}", r.config, r.string, r.word));
        }
    }
    assert!(all >= 40 && diffs.is_empty(), "{} of {all} differ from Word's:\n{}", diffs.len(), diffs.join("\n"));
}

/// The paragraph's own settings decide the total (Word's third table, set a_one, a paragraph of the body, `jc` left and both alike): `w:kinsoku` 0 takes the unit-after-an-opening-bracket
/// limit away, `w:wordWrap` 0 makes the last letter of a Latin word the unit that goes to the next line, `w:autoSpaceDE` and `w:autoSpaceDN` take away the spaces of letters and of
/// digits (each alone), `w:overflowPunct` 0 the hanging. A sample of every fortieth row of the set, but for the rows that end on a closing bracket (they hang and squeeze together:
/// the total of the closing brackets that follow each other is not explained), the long lines of a real document, `（（` at the start of a line and the family of `あ1あ、い2う`
/// (the limit one sixteenth short before a digit: not explained).
fn the_paragraph_settings_decide_the_total() {
    let known = |s: &str| {
        s.ends_with(['）', '」', '。', '、', '，', '．', '』', '】']) || s.starts_with("（（") || s.contains("い2う") || s.contains("ついて") || s.contains("障害者") || s.contains("あ1あ、い")
    };
    let mut diffs = Vec::new();
    let (mut all, mut n) = (0, 0);
    for r in rows() {
        if r.set != "a_one" || r.config.contains("__") || r.config.contains("_cNone_") || !(r.config.starts_with("meiryo_11_c14_cp_k2_left") || r.config.starts_with("meiryo_11_c14_cp_k2_both")) {
            continue;
        }
        n += 1;
        if n % 40 != 0 || known(&r.string) {
            continue;
        }
        let Some(mut cfg) = parse(&r.config) else { continue };
        cfg.body = true;
        let ours = least_width(&cfg, &r.string);
        all += 1;
        if (ours - r.word).abs() > 4 {
            diffs.push(format!("{} {}: Word {}, ours {ours}", r.config, r.string, r.word));
        }
    }
    assert!(all >= 40 && diffs.is_empty(), "{} of {all} differ from Word's:\n{}", diffs.len(), diffs.join("\n"));
}

/// Mode 15 squeezes and hangs only in a justified paragraph, as mode 14 does in every one (Word's fourth table, sets g1_ms_one and g15_ms_one, ＭＳ 明朝 10.5 pt, no grid and a lines grid):
/// the same strings in `jc` both give the same least widths in the two modes, and in `jc` left mode 15 gives the natural width. A sample of every eighth row, but for the rows of a
/// paragraph with `w:kinsoku`, `w:wordWrap` and the spaces all off (the one-sixteenth limits) and the full-width spaces in a row (the share rule of the justified line of mode 15).
fn a_justified_paragraph_of_mode_15_is_one_of_mode_14() {
    let known = |config: &str, s: &str| config.contains("ki0") || s.contains("\u{3000}\u{3000}") || s.contains("：") || s.contains("；") || s.contains("！") || s.ends_with('!') || s.contains("ABC）");
    let mut diffs = Vec::new();
    let (mut all, mut n) = (0, 0);
    for r in rows() {
        let grid_ok = r.config.ends_with("__none") || r.config.ends_with("__l360");
        if !(r.set == "g15_ms_one" || r.set == "g1_ms_one") || !grid_ok || !r.config.contains("_left_") && !r.config.contains("_both_") {
            continue;
        }
        n += 1;
        if n % 8 != 0 || known(&r.config, &r.string) {
            continue;
        }
        let Some(mut cfg) = parse(&r.config) else { continue };
        cfg.body = true;
        let ours = least_width(&cfg, &r.string);
        all += 1;
        if (ours - r.word).abs() > 4 {
            diffs.push(format!("{} {}: Word {}, ours {ours}", r.config, r.string, r.word));
        }
    }
    assert!(all >= 40 && diffs.is_empty(), "{} of {all} differ from Word's:\n{}", diffs.len(), diffs.join("\n"));
}

/// A section of `linesAndChars` (no added pitch, or +1.0 / -0.4 pt) squeezes and hangs as one without a grid does, the amounts times the grid's pitch (Word's table of ＭＳ 明朝
/// 10.5 pt, mode 14, g1_ms_one): the strings' least widths less the control's (ten kana, whose least width has an extra of one pitch) are the same as Word's. A sample of every
/// third string of `compress` on and the default settings (`w:kinsoku`, `w:wordWrap`, `autoSpace` on).
fn lines_and_chars_squeezes_by_the_pitch() {
    let mut diffs = Vec::new();
    let (mut all, mut n) = (0, 0);
    for r in rows() {
        if r.set != "g1_ms_one" || !(r.config.ends_with("__lcx_360") || r.config.ends_with("__lc0_360")) || !r.config.contains("_left_on_") {
            continue;
        }
        n += 1;
        // (Left out: the colon and the semicolon, whose half em of mode 14 is not explained, a lone full-width space and the 7/16 em before a digit: they differ without a grid too.)
        if n % 3 != 0 || r.string == GRID_CONTROL || r.string.contains(['：', '；', '\u{3000}']) || r.string.contains("あ1あ") {
            continue;
        }
        let Some(control) = rows().into_iter().find(|c| c.set == r.set && c.config == r.config && c.string == GRID_CONTROL) else { continue };
        let Some(mut cfg) = parse(&r.config) else { continue };
        cfg.body = true;
        let ours = least_width(&cfg, &r.string) - least_width(&cfg, GRID_CONTROL);
        let word = r.word - control.word;
        all += 1;
        if (ours - word).abs() > 4 {
            diffs.push(format!("{} {}: Word {word}, ours {ours}", r.config, r.string));
        }
    }
    assert!(all >= 10 && diffs.is_empty(), "{} of {all} differ from Word's:\n{}", diffs.len(), diffs.join("\n"));
}

/// A report of how far ibisOffice is from Word on the sets of the table (stderr): not a test of anything; it runs only with `IBIS_PUNCT_REPORT` set.
fn report() {
    if std::env::var("IBIS_PUNCT_REPORT").is_err() {
        return; // (it takes minutes: `IBIS_PUNCT_REPORT=1 IBIS_PUNCT_SETS=p2_cell IBIS_PUNCT_ALL=1`)
    }
    let configs = [
        "meiryo_11_c14_cp_k2_center_on_w100",
        "meiryo_11_c14_cp_k2_center_on_w80",
        "meiryo_11_c12_cp_k2_center_on_w100",
        "meiryo_11_c15_cp_k2_center_on_w100",
        "meiryo_11_c14_none_k2_center_on_w100",
        "meiryo_11_c14_dnc_k2_center_on_w100",
        "meiryo_11_c14_cpk_k2_center_on_w100",
    ];
    let sets: Vec<String> = std::env::var("IBIS_PUNCT_SETS").unwrap_or_else(|_| "p1_cell,p3_cell".into()).split(',').map(str::to_string).collect();
    let all_configs = std::env::var("IBIS_PUNCT_ALL").is_ok();
    for set in &sets {
        let (same, all, diffs) = compare(&[set.as_str()], if all_configs { &[] } else { &configs }, 4);
        eprintln!("{set}: {same} of {all} within 4 twips of Word");
        for d in diffs {
            eprintln!("  {d}");
        }
    }
}
