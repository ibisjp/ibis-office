// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Whether a justified or distributed line of compatibility mode 15 takes in the next break by squeezing, or breaks before it and spreads
//! (GitHub issues #164 and #165), and how the squeeze is shared out.
//!
//! The rule is a measured reading of Windows Word's output (the Microsoft 365 Word, `tasks/doc/m112-docx-issues-160-166.md` and
//! `tasks/doc/requests/2026-11-14-many-patterns-1-result.md`: about 3,700 lines of compatibility mode 15 with 4 fonts, 4 sizes and many
//! indents, the model below agrees with all but 3), not Word's published rule. The sizes, with the measured range each has:
//!
//! * A is the last break where the line fits as it is, B the next one. `o` is how far line B is too long, `e` how much room line A has left.
//! * What line B can give (`S`): a Latin space a quarter of itself, a space between Japanese and Latin (和欧間) an eighth em (half of what it has),
//!   with the setting 約物を詰める a stop's or bracket's half em, a dot's or colon's half em (a quarter each side), a full-width space three
//!   quarters of an em, and the character the line ends with: its whole width when it can hang past the edge, else what it can give.
//! * What spreading line A would use (`T`, the measure of the room): a Latin space half of itself, a 和欧間 a quarter em (all it has), any other
//!   gap between characters a 64th of an em (the 64th is a measured number, between 1/64.5 and 1/63.7 em; why it is that is not known).
//! * Word takes line B when `o / S <= e / T`, else line A. A line B that cannot take `o` at all (`o > S`) is not taken.
//! * With `w:balanceSingleByteDoubleByteWidth` (the compatibility setting most documents have) no space is squeezed, so `S` has none; without it
//!   every space is, a line's first ones too.
//! * The squeeze is `o`, no more: with a stop at the end that can hang and `o` at most half an em, the stop gives its half em (the room left is
//!   spread); else the Latin spaces and full-width spaces first, the stops and brackets next, the 和欧間 last, each stage up to what it has and
//!   shared out by what each has (a stop of 5.25 pt and the two sides of a dot of 2.625 pt each take 2 : 1 : 1); the end stop hangs when the
//!   rest cannot give enough (`o` at most the stop's width and what the rest gives).

use super::*;

/// What a justified line does at the character that does not fit.
pub(super) enum JustifyChoice {
    /// The line breaks before the characters that do not fit, at the last break; nothing is squeezed.
    BreakBefore,
    /// The line takes in the characters up to the next break: the squeezes, each `(unit, width taken, inset taken, auto space taken)`.
    TakeIn(Vec<Squeeze>),
    /// Not decided here (a tab, a picture, a character grid, nothing to break before): the line is set as before.
    Leave,
}

/// One unit's share of the squeeze.
pub(super) struct Squeeze {
    pub k: usize,
    pub width: f32,
    pub inset: f32,
    pub auto_space: f32,
}

const OPENERS: &str = "（「『【〔〈《［｛"; // i18n-ok（始め括弧の文字の集合。文字の並び）
const DOTS: &str = "・：；"; // i18n-ok（中点・コロン・セミコロン。文字の並び）
const HANGING: &str = "、。，．,.）」』】］｝’”｡､"; // i18n-ok（ぶら下げる文字の集合。文字の並び）

impl Layouter<'_> {
    /// The decision at `units[i]`, the first character that does not fit the line that starts at `start` (`room` wide), `last_break` the last break
    /// before it (the end of line A).
    pub(super) fn justify_choice(&self, p: &FlowParagraph, units: &[Unit], start: usize, i: usize, last_break: Option<usize>, room: f32) -> JustifyChoice {
        let b_end = (i + 1..units.len()).find(|&k| self.break_allowed(p, units, start, k)).unwrap_or(units.len());
        // Characters only; an optional hyphen (`w:softHyphen`) shows as a hyphen at the end of a line, which this does not count (see `shy_hang_tests.rs`).
        if !(start..b_end).all(|k| matches!(&units[k], Unit::Char { c, .. } if *c != '\u{ad}')) {
            return JustifyChoice::Leave;
        }
        // Line B without the spaces that end it (they hang; so do the full-width spaces that end the paragraph), and without the space between
        // Japanese and Latin that the line's end does not need.
        let hangs_at_end = |k: usize| unit_char(&units[k - 1]) == Some(' ') || (unit_char(&units[k - 1]) == Some('\u{3000}') && units[k..].iter().all(|u| unit_char(u) == Some('\u{3000}')));
        let mut eb = b_end;
        while eb > start + 1 && hangs_at_end(eb) {
            eb -= 1;
        }
        let gap_of = |k: usize| if let Unit::Char { auto_space, .. } = &units[k] { *auto_space } else { 0.0 };
        let o = (start..eb).map(|k| unit_width(&units[k])).sum::<f32>() - gap_of(eb - 1) - room;
        if o <= 0.01 {
            return JustifyChoice::Leave;
        }
        let a_end = last_break.filter(|&b| b > start);
        let ec = eb - 1;
        // On a grid of lines and characters every amount is scaled by the grid's pitch: the font size and the added pitch (Windows Word,
        // `tasks/doc/requests/2026-11-18-54-wrap15-grid-result.md` 3: the squeeze of mode 15 on the grid is that of no grid, in ems of the pitch).
        let pa = if self.char_grid.get() && p.snap_to_grid && self.current_cell.get().is_none() { self.char_space.get().map_or(0.0, pt) } else { 0.0 };
        let em_of = |k: usize| if let Unit::Char { size, squeeze, .. } = &units[k] { (*size + pa) * *squeeze } else { 0.0 };
        let full_stops = |font: &Font| advance_em(self.res, Some(font.family.as_ref()), '、') > 0.99; // i18n-ok（読点の幅で書体を見分ける）
        let squeezable = |k: usize| matches!(&units[k], Unit::Char { c, width, size, style, squeeze, squeezed, font, .. } if *style != LABEL && (can_give_up_half(*c, *width, *size * *squeeze, *squeezed) || colon_gives_up_half(*c, *width, *size * *squeeze, *squeezed)) && full_stops(font));
        let opens_line = |k: usize| (start..k).all(|j| unit_char(&units[j]) == Some(' ')) && unit_char(&units[k]).is_some_and(|ch| OPENERS.contains(ch));
        let compress = self.doc.compress_punctuation;
        // The character the line ends with hangs past the edge when it is one that may (and a table's cell does not let anything hang).
        let hangs = p.kinsoku && p.overflow_punct && self.current_cell.get().is_none() && ec > start && unit_char(&units[ec]).is_some_and(|ch| HANGING.contains(ch));
        let end_squeezable = compress && squeezable(ec);
        // No hanging (`overflowPunct` off, a table's cell): the stop or closing bracket the line ends with takes what the line needs first, up to half the pitch (the half em of its own on a grid with an
        // added pitch), and the rest is shared out by the units of the stops of the line (Windows Word, `tasks/doc/requests/2026-11-18-54-squeeze-share-result.md` 0: 11,515 paragraphs of mode 15,
        // justified, no hanging, every gap between two characters within 0.27 pt; corpus 037's two lines within 0.1 pt, the end stop 0.16 to 0.5 pt).
        let end_first = !hangs && end_squeezable && ec > start && unit_char(&units[ec]).is_some_and(|ch| HANGING.contains(ch));

        // What line B can give, by stage, and who gives it.
        let mut spaces: Vec<(usize, f32)> = Vec::new();
        let mut stops: Vec<(usize, f32)> = Vec::new();
        let mut gaps: Vec<(usize, f32)> = Vec::new();
        for k in start..eb {
            let Unit::Char { c, width, size, squeeze, style, squeezed, auto_space, .. } = &units[k] else { continue };
            if *style == LABEL {
                continue;
            }
            let em = (*size + pa) * *squeeze;
            // With the compatibility setting 半角と全角の幅を調整する (`w:balanceSingleByteDoubleByteWidth`) Word squeezes no space of the line, a
            // half-width or a full-width one, wherever it stands in the line (Windows Word, 2026-11-14: 0 of 128 lines per case, in every position
            // and font, compatibility 14 and 15; the line's first characters make no difference); without it every space is squeezed, a line's
            // first ones too. 145 of the 150 corpus documents have the setting.
            let balanced = self.doc.balance_single_double_byte_width;
            // A half-width space between two Latin characters is still narrowed with the setting (Word's PDF of `justify-hang-closer-compat15-probe.docx`, which has
            // the setting: Times New Roman words, 4.5 pt over is taken in by narrowing the spaces). The many-patterns-1 measurements are all of spaces beside Japanese
            // text; a space between Latin characters was not measured there.
            // (the nearest character that is not a space on either side counts: the spaces of an indent are beside the text that follows them)
            let before = (start..k).rev().find_map(|j| unit_char(&units[j]).filter(|ch| *ch != ' '));
            let after = (k + 1..units.len()).find_map(|j| unit_char(&units[j]).filter(|ch| *ch != ' '));
            let beside_cjk = before.is_some_and(is_cjk) || after.is_some_and(is_cjk);
            if *c == ' ' {
                if !balanced || !beside_cjk {
                    spaces.push((k, *width / 4.0));
                }
            } else if *c == '\u{3000}' && compress && !*squeezed {
                if !balanced {
                    spaces.push((k, (em * 0.75).min(*width)));
                }
            } else if compress && squeezable(k) && !opens_line(k) && !(hangs && k == ec) && !(end_first && k == ec) {
                stops.push((k, em / 2.0));
            }
            // On a grid with an added pitch the space between Japanese and Latin is not narrowed (Windows Word, corpus 101: a line that would take in one more character by narrowing
            // four such spaces breaks before it, and its spare room is spread into those spaces, each twice its width).
            if *auto_space > 0.0 && pa == 0.0 {
                gaps.push((k, *auto_space / 2.0));
            }
        }
        let total = |v: &[(usize, f32)]| v.iter().map(|g| g.1).sum::<f32>();
        // The gap after the line's last character counts in what the line can give, but there is nothing in the line to take it from.
        let gap_end = gaps.iter().find(|g| g.0 == ec).map_or(0.0, |g| g.1);
        let have = total(&spaces) + total(&stops) + total(&gaps) + if end_first { em_of(ec) / 2.0 } else { 0.0 };
        let end_w = unit_width(&units[ec]);
        let s_all = have + if hangs { end_w } else { 0.0 };
        if o > s_all + 0.01 {
            return if a_end.is_some() { JustifyChoice::BreakBefore } else { JustifyChoice::Leave };
        }
        let Some(a_end) = a_end else { return JustifyChoice::Leave };

        // Line A: the room it has left, and the measure of what spreading would use.
        let mut ea = a_end;
        while ea > start + 1 && unit_char(&units[ea - 1]) == Some(' ') {
            ea -= 1;
        }
        let e = room - ((start..ea).map(|k| unit_width(&units[k])).sum::<f32>() - gap_of(ea - 1));
        let mut t = 0.0f32;
        for k in start..ea.saturating_sub(1) {
            let (Unit::Char { c, width, size, squeeze, auto_space, .. }, Some(next)) = (&units[k], unit_char(&units[k + 1])) else { continue };
            if *auto_space > 0.0 {
                t += *auto_space;
            } else if *c == ' ' {
                t += *width / 2.0;
                if is_cjk(next) {
                    t += *size * *squeeze / 64.0;
                }
            } else if is_cjk(*c) || is_cjk(next) {
                t += *size * *squeeze / 64.0;
            }
        }
        if t > 0.0 && o / s_all > e / t + 1e-4 {
            return JustifyChoice::BreakBefore;
        }

        // Line B is taken: what each gives.
        let half_end = em_of(ec) / 2.0;
        let mut need = o;
        let mut squeezes: Vec<Squeeze> = Vec::new();
        let interior = have;
        if hangs && end_squeezable && o <= half_end + 0.01 {
            // The stop at the end gives its half em; what is left of the room is spread.
            squeezes.push(Squeeze { k: ec, width: half_end, inset: 0.0, auto_space: 0.0 });
            need = 0.0;
        } else if o <= interior + 0.01 {
            if end_first {
                let first = need.min(half_end);
                squeezes.push(Squeeze { k: ec, width: first, inset: 0.0, auto_space: 0.0 });
                need -= first;
            }
        } else if hangs && end_squeezable && o <= half_end + interior + 0.01 {
            squeezes.push(Squeeze { k: ec, width: half_end, inset: 0.0, auto_space: 0.0 });
            need = o - half_end;
        } else if hangs {
            // The end stop hangs, whole.
            need = (o - end_w).max(0.0);
        }
        // Spaces first, then stops and brackets, then the 和欧間; the gap after the last character is not there to take from, its share goes to the others.
        let gaps_here: Vec<(usize, f32)> = gaps.iter().copied().filter(|g| g.0 != ec).collect();
        let stage_have = [total(&spaces), total(&stops), total(&gaps_here) + gap_end];
        let mut left = need;
        let mut taken = [0.0f32; 3];
        for (n, have) in stage_have.iter().enumerate() {
            let take = left.min(*have);
            taken[n] = take;
            left -= take;
        }
        let share = |v: &[(usize, f32)], take: f32, squeezes: &mut Vec<Squeeze>, kind: u8| {
            let sum = total(v);
            if take <= 0.0 || sum <= 0.0 {
                return;
            }
            for &(k, cap) in v {
                let cut = take * cap / sum;
                let Unit::Char { c, .. } = &units[k] else { continue };
                let inset = match kind {
                    1 if OPENERS.contains(*c) => cut,
                    1 if DOTS.contains(*c) => cut / 2.0,
                    _ => 0.0,
                };
                squeezes.push(Squeeze { k, width: cut, inset, auto_space: if kind == 2 { cut } else { 0.0 } });
            }
        };
        share(&spaces, taken[0], &mut squeezes, 0);
        share(&stops, taken[1], &mut squeezes, 1);
        share(&gaps_here, taken[2], &mut squeezes, 2);
        JustifyChoice::TakeIn(squeezes)
    }
}
