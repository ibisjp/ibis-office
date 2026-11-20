// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! How a justified line takes in one more character (GitHub issues #164 and #165, Word's PDFs of the reporter's
//! `justify-squeeze-or-break.docx` and `justify-distribute-amounts.docx` as the Windows PC's Word set them, `tasks/doc/m112-docx-issues-160-166.md`):
//! the line is taken to the next break by squeezing when the part of the squeeze that it needs, of what it could give, is no more than the part
//! of the room left on the line before it that spreading would use (decided in `Layouter::justify_choice`), and the squeeze is shared out
//! as the line needs it, not stop by stop in halves.

use super::flow_layout::Item;
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::justify164_line_takes_the_next_break_by_squeezing_or_breaks_before", line_takes_the_next_break_by_squeezing_or_breaks_before),
        ("docx::justify165_squeeze_and_spread_amounts_follow_word", squeeze_and_spread_amounts_follow_word),
        ("docx::justify164_a_number_and_an_indent_keep_their_spaces", a_number_and_an_indent_keep_their_spaces),
        ("docx::justify164_colons_are_squeezed_only_in_mode_15", colons_are_squeezed_only_in_mode_15),
        ("docx::justify164_without_the_balance_setting_a_leading_space_is_squeezed", without_the_balance_setting_a_leading_space_is_squeezed),
    ]
}

/// Tools (not checks): `--ignored`.
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![("docx::justify164_dump", dump)]
}

/// The first line of each of the paragraphs of `file`: each character with its x and its advance.
fn first_lines(file: &str) -> Vec<Vec<(char, f32, f32)>> {
    let mut s = Scenario::open(file);
    s.with(|e, _| {
        let pages = super::flow_layout::pages(e.flow().expect("a document"), &e.resources);
        let mut glyphs: Vec<(f32, f32, char, f32)> = Vec::new();
        for page in &pages.pages {
            for it in &page.items {
                if let Item::Text { x, y, text, offsets, font, size, .. } = it {
                    for (k, c) in text.chars().enumerate() {
                        glyphs.push((*y, x + offsets[k], c, e.resources.text_width(&c.to_string(), font, *size)));
                    }
                }
            }
        }
        glyphs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut lines: Vec<(f32, Vec<(char, f32, f32)>)> = Vec::new();
        for (y, x, c, w) in glyphs {
            if lines.last().is_none_or(|l| (y - l.0).abs() > 1.0) {
                lines.push((y, Vec::new()));
            }
            lines.last_mut().unwrap().1.push((c, x, w));
        }
        // A paragraph's first line: the one that comes after the space between paragraphs (the pitch of lines inside one is the smallest step).
        let pitch = lines.windows(2).map(|w| w[1].0 - w[0].0).fold(f32::MAX, f32::min);
        let mut firsts = Vec::new();
        for (n, l) in lines.iter().enumerate() {
            if n == 0 || l.0 - lines[n - 1].0 > pitch * 1.2 {
                firsts.push(l.1.clone());
            }
        }
        firsts
    })
}

/// The characters on a line, without its spaces.
fn count(line: &[(char, f32, f32)]) -> usize {
    line.iter().filter(|g| !g.0.is_whitespace()).count()
}

/// The room after glyph `k` beyond its own advance (what the glyph's right edge leaves before the next one).
fn gap(line: &[(char, f32, f32)], k: usize) -> f32 {
    line[k + 1].1 - line[k].1 - line[k].2
}

fn dump() {
    if let Some(f) = std::env::var_os("PROBE_FILE") {
        let f = f.to_string_lossy().to_string();
        let mut s = Scenario::open(&f);
        let pages = s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
        for it in &pages.pages[0].items {
            if let Item::Text { y, text, .. } = it {
                println!("PROBE y={y:.1} {text}");
            }
        }
        return;
    }
    for f in ["test/editing/justify-squeeze-or-break.docx", "test/editing/justify-squeeze-or-break-compat14.docx", "test/editing/justify-distribute-amounts.docx"] {
        for (n, l) in first_lines(f).iter().enumerate() {
            let text: String = l.iter().map(|g| g.0).collect();
            let gaps: Vec<String> = (0..l.len() - 1).filter(|&k| gap(l, k).abs() > 0.2).map(|k| format!("{}{}:{:+.2}", l[k].0, k, gap(l, k))).collect();
            println!("{f} #{}: {} chars ({} without spaces), end {:.2}  {text}\n    {}", n + 1, l.len(), count(l), l.last().map_or(0.0, |g| g.1 + g.2), gaps.join(" "));
        }
    }
}

/// #164: how many characters the first line of each paragraph has, in Word (compatibility mode 15): 46, 45, 46, 46, 46, 46, 46 and
/// (counting the full-width space) 46. Paragraphs 1 and 2 differ only in the overflow, 4.0 and 4.5 pt, of the 46th character: the squeeze
/// of 4.0 is 0.76 of the stop's 5.25 pt and 0.90 of the room the line before has to spread, 4.5 is 0.86 and 0.83.
fn line_takes_the_next_break_by_squeezing_or_breaks_before() {
    let lines = first_lines("test/editing/justify-squeeze-or-break.docx");
    let word = [46usize, 45, 46, 46, 46, 46, 46, 45];
    for (n, &w) in word.iter().enumerate() {
        assert_eq!(count(&lines[n]), w, "paragraph {}: {} characters on the first line, Word has {w}", n + 1, count(&lines[n]));
    }
}

/// #165: the amounts. Word's first lines (compatibility mode 15, ＭＳ-free Yu Gothic 10.5 pt): the squeeze is what the line needs, shared by the stops
/// (3.0 pt on one 、; 1.5 pt on each of two), a closing 。 that the line cannot hold whole keeps the half em inside the line and the other half past
/// the edge, and a line that is spread and not squeezed keeps the stops as they are.
fn squeeze_and_spread_amounts_follow_word() {
    let lines = first_lines("test/editing/justify-distribute-amounts.docx");
    let near = |a: f32, b: f32, tol: f32, what: &str| assert!((a - b).abs() <= tol, "{what}: {a:.2} against {b:.2}");
    let stop = |l: &[(char, f32, f32)], n: usize| (0..l.len() - 1).filter(|&k| l[k].0 == '\u{3001}').nth(n).unwrap_or_else(|| panic!("a stop"));
    // 1: one 、, 3.0 pt over: it is squeezed by 3.0 and the others stay.
    let l = &lines[0];
    near(gap(l, stop(l, 0)), -3.0, 0.3, "paragraph 1: the stop");
    // 2: two 、, 3.0 pt over: 1.5 pt each.
    let l = &lines[1];
    near(gap(l, stop(l, 0)), -1.56, 0.3, "paragraph 2: the first stop");
    near(gap(l, stop(l, 1)), -1.56, 0.3, "paragraph 2: the second stop");
    // 3: two 、 and a 。 that ends the line, 2.5 pt over: the stops keep their room; the 。 starts half an em before the right edge (529.31 pt).
    let l = &lines[2];
    near(gap(l, stop(l, 0)), 0.0, 0.3, "paragraph 3: the first stop");
    near(gap(l, stop(l, 1)), 0.0, 0.3, "paragraph 3: the second stop");
    let stop_end = l.iter().rposition(|g| g.0 == '\u{3002}').expect("a full stop");
    near(l[stop_end].1, 529.31, 0.4, "paragraph 3: where the closing full stop starts");
    // 4: a line broken before a long Latin word keeps no squeeze: 「「 are not squeezed more than the kerning gives (Word: the gaps are 0.78 pt).
    let l = &lines[3];
    let open = l.iter().position(|g| g.0 == '\u{300c}').expect("an opening bracket");
    near(gap(l, open - 1), 0.78, 0.3, "paragraph 4: kana to the first bracket");
    near(gap(l, open), 0.78, 0.3, "paragraph 4: the first bracket to the second");
    // 5: っ1 and the line broken before the long word: the space beside the 1 is widened before the others (Word: +5.86 and +5.80 from the kana).
    let l = &lines[4];
    let one = l.iter().position(|g| g.0 == '1').expect("a 1");
    near(gap(l, one - 1), 5.86, 0.5, "paragraph 5: small tsu to the 1");
    near(gap(l, one), 5.80, 0.5, "paragraph 5: the 1 to the next kana");
}

/// With `w:balanceSingleByteDoubleByteWidth` (145 of 150 corpus documents) no space of a line is squeezed to take one more character, wherever it
/// stands (Windows Word, 2026-11-14, `tasks/doc/requests/2026-11-14-many-patterns-1-result.md`: 0 of 128 lines per case, a number's space, indent
/// spaces, the 10th character's space alike; documents 072, 123, 028, 147 of the corpus); the stop in the line gives what the line needs (3.0 pt).
fn a_number_and_an_indent_keep_their_spaces() {
    let lines = first_lines("test/editing/justify-leading-space.docx");
    // (The file's third paragraph, ⅰ and a half-width space, has a shorter line: its space is a quarter of the full-width one's; not checked here.)
    for (n, l) in lines.iter().take(2).enumerate() {
        assert_eq!(l.len(), 46, "paragraph {}: {} characters on the first line", n + 1, l.len());
        let stop = l.iter().position(|g| g.0 == '\u{3001}').expect("a stop");
        assert!((gap(l, stop) + 3.0).abs() < 0.3, "paragraph {}: the stop is squeezed {:.2} pt", n + 1, gap(l, stop));
        for k in 0..2 {
            assert!(gap(l, k).abs() < 0.3, "paragraph {}: the gap after character {k} is {:.2} pt", n + 1, gap(l, k));
        }
    }
}

/// ： and ； give up their half em in compatibility mode 15 only: paragraph 6 of the compat 14 file keeps 44 characters, as Word's PDF does.
fn colons_are_squeezed_only_in_mode_15() {
    let lines = first_lines("test/editing/justify-squeeze-or-break-compat14.docx");
    assert_eq!(count(&lines[5]), 44, "paragraph 6 of the mode 14 file");
}

/// Without the setting every space is squeezed, a line's first ones too (the same file without `balanceSingleByteDoubleByteWidth`): the stop gives
/// less than the 3.0 pt it gave when the spaces kept theirs, and the space after the first character is narrower than the full-width one.
fn without_the_balance_setting_a_leading_space_is_squeezed() {
    let lines = first_lines("test/editing/justify-leading-space-unbalanced.docx");
    for (n, l) in lines.iter().take(2).enumerate() {
        let stop = l.iter().position(|g| g.0 == '\u{3001}').expect("a stop");
        let squeezed_spaces: f32 = (0..2).map(|k| gap(l, k)).sum();
        assert!(squeezed_spaces < -0.5, "paragraph {}: the leading spaces are squeezed by {:.2} pt in all", n + 1, -squeezed_spaces);
        assert!(gap(l, stop) > -3.0 + 0.3, "paragraph {}: the stop is squeezed {:.2} pt", n + 1, gap(l, stop));
    }
}
