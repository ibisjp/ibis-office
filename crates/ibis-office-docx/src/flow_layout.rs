// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Laying out a flowing document (Word) into pages: lines are broken with
//! the characters' widths in the typefaces the document asks for, and
//! paragraphs, table rows, headers and footers are placed on pages.
//!
//! ＭＳ 明朝 and ＭＳ ゴシック are fixed-width (a full-width character one em,
//! a half-width one half an em); where they are not installed the lines are
//! still broken by those widths, and each character is drawn in its place
//! with the substitute, so lines end where Word ends them.

use std::cell::RefCell;
use std::sync::Arc;

use gpui_kit::*;
use ibis_doc_model::{
    Block, CellAnchor, PreferredWidth, EMU_PER_POINT, Emu, FlowAlign, FlowBorder, FlowDocument, FlowParagraph, FlowRunKind, FlowRunStyle,
    FlowTable, LineHeight, ListSuffix, MediaId, Rgb, RowHeight, SectionBreak, ShadingPattern, TabAlign, TextStyle, UnderlineStyle, VerticalMerge, VerticalPosition,
};

use ibis_doc_model::flow::{CarryInput, FallbackCarry, FontSlot};
use crate::art_border::{self, ArtPart};
use crate::render::shape_view::{Resources, hsla};

/// Something drawn on a page, in points from the page's top-left corner.
#[derive(Clone, Debug)]
pub enum Item {
    /// Characters starting at `x` on the baseline `y`, each at its own
    /// offset from `x`.
    Text { x: f32, y: f32, text: SharedString, offsets: Arc<[f32]>, font: Font, size: f32, color: Hsla },
    /// Characters on a baseline from `x`, `y` turned a quarter (`turns`: 1
    /// clockwise, -1 anticlockwise) about that point (a cell's vertical
    /// text).
    /// `xscale` squeezes the glyph along its own advance before it is turned (1 when it is not squeezed).
    Turned { x: f32, y: f32, text: SharedString, font: Font, size: f32, color: Hsla, turns: i8, xscale: f32 },
    /// One character on the baseline `y` from `x`, its glyph `xscale` times
    /// as wide (`w:w`, `w:fitText`).
    Scaled { x: f32, y: f32, text: SharedString, font: Font, size: f32, color: Hsla, xscale: f32 },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Hsla },
    /// A picture of an art page border, `image_w` × `image_h` from (`x`,
    /// `y`), cut to the `w` × `h` there; `tint` recolours a grey one.
    Art { art: ibis_doc_model::ArtBorder, part: ArtPart, x: f32, y: f32, w: f32, h: f32, image_w: f32, image_h: f32, tint: Option<Rgb> },
    Picture { media: MediaId, x: f32, y: f32, w: f32, h: f32 },
    Shape { shape: Box<ibis_doc_model::Shape>, x: f32, y: f32, w: f32, h: f32 },
    /// A straight line from (`x`, `y`) to (`x2`, `y2`), `w` wide.
    Stroke { x: f32, y: f32, x2: f32, y2: f32, w: f32, color: Hsla },
    /// Not drawn: where the positions of a line of paragraph `para` are,
    /// for the caret and the selection. The line's box starts at `x`, `y`
    /// and is `h` high; `stops` are the positions on it and their distance
    /// from `x` (the last one is where the line's text ends).
    ///
    /// `list_tab` is where the tab after a list item's bullet or number is (from, to: the distance from `x`, the editing marks' →).
    /// `base` is the drawn baseline's distance from the line's top (`LineBox::baseline` and `text_dy`; a horizontal line), `avail` how far the
    /// line's text can run from its start (to the margin or the cell's edge), where text being composed wraps.
    ///
    /// `largest` is the largest size the line's characters are measured at (`Unit::Char::metric`; a script is smaller where it is drawn): the upright characters of a vertical line stand
    /// on the middle of that size's box, as Word's do.
    ///
    /// `pad` is the paragraph's space before (on its first line) and after (on its last line) that the line's box takes in Word's PDF (where the links are): the line's
    /// box is from `y - pad.0` to `y + h + pad.1`.
    Line { para: u32, x: f32, y: f32, h: f32, stops: Arc<[(usize, f32)]>, vertical: bool, list_tab: Option<(f32, f32)>, base: f32, avail: f32, pad: (f32, f32), largest: f32 },
    /// A two-color shading pattern across `(x, y, w, h)`, expanded into
    /// absolute page coordinates before rendering.
    Shading { pattern: ShadingPattern, color: Hsla, x: f32, y: f32, w: f32, h: f32 },
    /// A comment speech-bubble icon drawn outside the right edge of the paper.
    CommentBubble { x: f32, y: f32, size: f32 },
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub items: Vec<Item>,
    /// Number of items that belong to the document body (before headers and footers).
    pub(crate) body_items_count: usize,
    /// The paper's size (points), its section's.
    pub(crate) width: f32,
    pub height: f32,
    /// The section the page is in (its index), its page number, and whether
    /// it is the section's first page.
    pub(crate) section: usize,
    pub(crate) number: u32,
    pub(crate) first_in_section: bool,
    /// The footnotes whose marks are on the page (drawn at its bottom).
    pub(crate) notes: Vec<u32>,
    /// The rest of the notes of earlier pages that go on here: (the note, the first piece of it on this page).
    pub(crate) carry_in: Vec<(u32, usize)>,
    /// When the notes on the page are more than it holds: how many of their pieces (carried ones first) it takes, the rest go on to the next page.
    pub(crate) split_take: Option<usize>,
    /// The body's area on the paper (points): left, top, right, bottom. The text boundaries and the header and footer
    /// lines of the editing view are drawn at its edges.
    pub(crate) text_area: [f32; 4],
}

/// The pages of a document.
#[derive(Debug, Default)]
pub struct Pagination {
    pub pages: Vec<Page>,
}

pub(crate) fn pt(emu: i64) -> f32 {
    emu as f32 / EMU_PER_POINT as f32
}

/// How far right of where its margins put it Word draws a page's content:
/// none (the coverage tests' references, Word's PDFs: the leftmost text of
/// DBDY, DRUN, DCEL, DPAR and DTBB at 36.0 pt, the margin). It was 0.49 pt
/// (an earlier measurement on Word for Mac's PDF of one document), which
/// the coverage tests' strict matches show was wrong (357 → 405 at 0).
const TEXT_ORIGIN_X: f32 = 0.0;
/// The typeface of a run that has none (see `typeface_for_character_in_line`): Word's built-in default on a Japanese Windows, measured.
const DEFAULT_TYPEFACE: &str = "Yu Mincho";


// The markup display types are in the drawing crate (`Resources::markup_view` holds one): ibis-office-render/src/markup_view.rs.
pub use ibis_office_render::markup_view::{MarkupDisplayMode, MarkupView};

pub fn clear_layout_caches() {
    PIECES.with(|c| c.borrow_mut().clear());
    PAGINATIONS.with(|c| c.borrow_mut().clear());
}

/// The colour tracked changes are shown in (Word's first author's).
const REVISION_COLOR: ibis_doc_model::Rgb = ibis_doc_model::Rgb(0xC00000);
/// The colour tracked moves are shown in (Word's standard green).
pub(crate) const MOVE_REVISION_COLOR: ibis_doc_model::Rgb = ibis_doc_model::Rgb(0x008000);

/// The highlight colour for commented text ranges (Word-compatible pale amber/yellow).
pub(crate) const COMMENT_HIGHLIGHT_COLOR: ibis_doc_model::Color = ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0xfff2ac), alpha: 180, theme: None };
/// The background colour of the comment indicator bubble in the margin.
#[allow(dead_code)]
pub(crate) const COMMENT_BUBBLE_BG: ibis_doc_model::Rgb = ibis_doc_model::Rgb(0xfef7e0);
/// The border colour of the comment indicator bubble in the margin.
#[allow(dead_code)]
pub(crate) const COMMENT_BUBBLE_BORDER: ibis_doc_model::Rgb = ibis_doc_model::Rgb(0xf9ab00);
/// The icon colour inside the comment indicator bubble in the margin.
pub(crate) const COMMENT_BUBBLE_ICON: ibis_doc_model::Rgb = ibis_doc_model::Rgb(0xd9822b);

/// Whether a unit is the rest of a 組み文字 (a combined run: its first character holds the room of all of them, the others have none): a line
/// never breaks there, nor does it break back to it, even when the whole is wider than the line (Word's PDF of corpus document 020: a combined
/// run of eight characters in a 40 pt cell stays whole). Both line breakers (the plain one and the one beside a floating object) use it.
fn is_combined_rest(p: &FlowParagraph, unit: &Unit, width: f32) -> bool {
    width == 0.0 && matches!(unit, Unit::Char { style, .. } if *style != LABEL && p.runs.get(*style).is_some_and(|r| r.style.combine.is_some()))
}

/// What an inline box (a run Word sets as one thing in a line) a unit belongs to: a ruby's base (its run) or a span fitted to a width (`w:fitText`, its
/// id). Two neighbouring units of one box are not parted by a line (GitHub issue #180: Word sends a ruby's base and a fitted span whole to the next line
/// when they do not fit, and draws the ruby once).
fn inline_box_of(p: &FlowParagraph, unit: &Unit) -> Option<(u8, u32)> {
    let Unit::Char { style, .. } = unit else { return None };
    if *style == LABEL {
        return None;
    }
    let run = p.runs.get(*style)?;
    if matches!(run.kind, FlowRunKind::Ruby { .. }) {
        return Some((0, *style as u32));
    }
    run.style.fit_text.map(|(_, id)| (1, id))
}

/// Whether the unit `i` is inside an inline box (`inline_box_of`) that began after the line's first unit `start`: a line does not break before it. A box
/// that starts the line is not kept whole (one wider than the line has to be cut).
fn is_box_rest(p: &FlowParagraph, units: &[Unit], i: usize, start: usize) -> bool {
    let Some(key) = inline_box_of(p, &units[i]) else { return false };
    if i == 0 || inline_box_of(p, &units[i - 1]) != Some(key) {
        return false;
    }
    let mut first = i - 1;
    while first > start && first > 0 && inline_box_of(p, &units[first - 1]) == Some(key) {
        first -= 1;
    }
    first > start
}

/// The width of the whole inline box (`inline_box_of`) whose first unit is `i` (the unit's own width when it is not the first of a box).
fn box_width_from(p: &FlowParagraph, units: &[Unit], i: usize) -> f32 {
    let Some(key) = inline_box_of(p, &units[i]) else { return unit_width(&units[i]) };
    let mut end = i + 1;
    while end < units.len() && inline_box_of(p, &units[end]) == Some(key) {
        end += 1;
    }
    units[i..end].iter().map(unit_width).sum()
}

/// The size of the characters of a 組み文字 / 割注 line set in `size` (pt): half of it, in whole half points (Word keeps a character's size in half points:
/// 10.5 pt text gives 5.0 pt, Word's PDFs of GitHub issue #180 have 5.04 pt, which is 5.0 in its 0.12 pt steps; 14 pt gives 7.0).
fn combine_size(size: f32) -> f32 {
    ((size * 2.0).round() as u32 / 2) as f32 / 2.0
}

/// How much nearer each character of the base of a ruby at its right (`rubyAlign="rightVertical"`) is to the next than its natural advance, in ems of the base's size: 0.1
/// (Windows Word's XPS, `tasks/doc/requests/2026-11-14-180-192-rest-many-patterns-result.md`, A2: 12 kinds of base in ＭＳ 明朝, 游明朝 and Arial at 10.5 and 24 pt: the advance is the
/// character's natural one less 0.1 B, whole-width 1.0 -> 0.9, half-width 0.5 -> 0.4, proportional the same, ±0.01 B). A measured constant not in the exceptions
/// of `docs/rule/implimentationRule.md` yet: the user decides (board d-180-base-09).
const RIGHT_VERTICAL_BASE_TIGHTENING: f32 = 0.1;

/// A typeface whose characters are one em (full-width) or half an em wide.
fn fixed_width(name: &str) -> bool {
    matches!(name, "ＭＳ 明朝" | "ＭＳ ゴシック" | "MS Mincho" | "MS Gothic" | "ＭＳ明朝" | "ＭＳゴシック") // i18n-ok（フォント名の判定（固定幅フォント）。訳さない）
}

/// The added pitch of a section's character grid (when it sets the characters: 文字数と行数を指定).
fn char_space_of(page: &ibis_doc_model::PageSetup) -> Option<Emu> {
    matches!(page.doc_grid_type, ibis_doc_model::DocGridType::LinesAndChars | ibis_doc_model::DocGridType::SnapToChars).then_some(page.char_space).flatten()
}

/// Whether a section sets the characters on a grid (文字数と行数を指定, or 文字数のみ), whatever its added pitch.
fn char_grid_of(page: &ibis_doc_model::PageSetup) -> bool {
    matches!(page.doc_grid_type, ibis_doc_model::DocGridType::LinesAndChars | ibis_doc_model::DocGridType::SnapToChars)
}

/// What Word's line of an East Asian typeface has more than the typeface's Windows height `S = usWinAscent + usWinDescent` (font units): the 0.3 of its 1.3 is split in two halves
/// of `0.15 S`, each rounded to the nearest whole font unit, one above the ascent and one below the descent (`H = S + 2 round(0.15 S)`). A vertical column's pitch is the same
/// with `S' = S + upem / 32` (`vertical_extra_units`). Windows Word, 2026-11-16 (XPS, `tasks/doc/requests/2026-11-16-54-ms-pitch-result.md`): 27 typefaces of 12 different
/// (unitsPerEm, ascent, descent) sets (256, 1024, 2048), the line and the column within 0.03 unit of the integer, the line's ascent side `winA + round(0.15 S)` within 0.8 unit;
/// 1.3 S rounded as a sum, floor, ceil and as A and D apart, to even and to 1/64 em, all miss some of them. For the ＭＳ typefaces (256, 220 + 36) it is 256 + 76 = 332 and
/// 264 + 80 = 344, 332/256 em and 344/256 em.
fn cjk_extra_units(s: f32) -> f32 {
    2.0 * (0.15 * s).round()
}

/// The extra of a vertical column's pitch over a horizontal line's of the same typeface, in font units: `(S' + 2 round(0.15 S')) - (S + 2 round(0.15 S))`, `S' = S + upem / 32`.
fn vertical_extra_units(s: f32, upem: f32) -> f32 {
    let s2 = s + (upem / 32.0).round();
    s2 + cjk_extra_units(s2) - s - cjk_extra_units(s)
}

pub(crate) fn cjk_extra_units_for_test(s: f32) -> f32 {
    cjk_extra_units(s)
}

pub(crate) fn vertical_extra_units_for_test(s: f32, upem: f32) -> f32 {
    vertical_extra_units(s, upem)
}

/// How much more than its 1 em of Windows ascent and descent a line of the ＭＳ typefaces is, in ems (upem 256, `S` = 256): see `cjk_extra_units`, 76/256 em.
fn ms_room_em() -> f32 {
    const UPEM: f32 = 256.0;
    cjk_extra_units(UPEM) / UPEM
}

/// Which characters that stand in Word's vertical text go by the typeface's own width, not a whole em (Windows Word's PDF, GitHub issue #188, 5 typefaces, 10.5 and 100 pt,
/// 18 characters that stand, `tasks/doc/requests/2026-11-16-188-proportional-upright-result.md`): ‘ ’ “ ” in ＭＳ Ｐ明朝 and ＭＳ Ｐゴシック (0.5 em), and ‘ “ in メイリオ (0.259 and
/// 0.449 em; its ’ ” take 1 em). Every other character that stands takes 1 em in every typeface measured (ＭＳ 明朝, 游明朝 and the others', whatever the hmtx
/// width: † ‡ ‼ ⁇ ⁈ ⁉ § in the proportional faces, ‘ ’ “ ” in 游明朝). Meiryo UI and the faces not measured are taken as 1 em.
fn upright_keeps_face_width(name: Option<&str>, c: char) -> bool {
    let Some(n) = name else { return false };
    match c {
        '\u{2018}' | '\u{201C}' => proportional_ms(n) || matches!(n.trim(), "メイリオ" | "Meiryo"), // i18n-ok（フォント名の判定。訳さない）
        '\u{2019}' | '\u{201D}' => proportional_ms(n),
        // The ideographic space of ＭＳ Ｐ明朝 is its own advance (0.6625 em) in a column too (Word's PDF of `space-advance-mspmincho-v.docx`, 10.5 pt: 6.98 pt, as across; not one em).
        '\u{3000}' => proportional_ms(n),
        _ => false,
    }
}

/// ＭＳ Ｐ明朝・ＭＳ Ｐゴシック (the proportional ones).
fn proportional_ms(name: &str) -> bool {
    matches!(name, "ＭＳ Ｐ明朝" | "ＭＳ Ｐゴシック" | "MS PMincho" | "MS PGothic" | "ＭＳＰ明朝" | "ＭＳＰゴシック") // i18n-ok（フォント名の判定（プロポーショナルフォント）。訳さない）
}

/// A character that gives up a sixteenth of an em with `compressPunctuationAndJapaneseKana`: hiragana, katakana (not the long vowel mark ー) and full-width digits (Word's XPS,
/// `tasks/doc/requests/2026-11-17-54-compress-punctuation-result.md`, 1 and 3: `ああ` saves 2/16 em, `あーあ` 2/16, `１あ` 2/16, kanji none; at most half an em in all).
fn gives_kana_squeeze(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{3096}' | '\u{30A1}'..='\u{30FA}' | '\u{FF10}'..='\u{FF19}' | '\u{FF21}'..='\u{FF3A}' | '\u{FF41}'..='\u{FF5A}')
}

/// The proportional Japanese typefaces: ＭＳ Ｐ明朝・Ｐゴシック, Meiryo UI, BIZ UDP明朝・UDPゴシック (and ＨＧＰ…). Word does not squeeze their stops and brackets, whatever their widths
/// are on this PC (the file may be missing here; Windows Word's XPS, `tasks/doc/requests/2026-11-17-54-compress-punctuation-result.md`, 2: `（時間休憩）` saves 0.01 em in
/// them, 0.5 em in the fixed-width faces). The squeeze of the space between Japanese and Latin letters is the same in every typeface.
fn proportional_cjk(name: &str) -> bool {
    let n = name.trim();
    proportional_ms(n) || n.eq_ignore_ascii_case("Meiryo UI") || n.starts_with("BIZ UDP") || n.starts_with("ＨＧＰ") || n.starts_with("HGP") // i18n-ok（フォント名の判定。訳さない）
}

/// A character's advance in ＭＳ Ｐ明朝 / ＭＳ Ｐゴシック (`name`) for a PC that lacks the typeface: the font file's hmtx (`ms_p_advance_em`; Word's PDFs and XPS agree with it, and
/// no kerning applies). For a character the file lacks, what Word's PDFs showed before the table: kana and kanji a full em, the stops 0.664 em, brackets and letters about half an em.
fn proportional_ms_width(name: &str, c: char, size: f32) -> f32 {
    let gothic = name.contains("ゴシック") || name.contains("Gothic"); // i18n-ok（フォント名の判定。訳さない）
    if let Some(em) = crate::render::font_metrics::ms_p_advance_em(gothic, c) {
        return em * size;
    }
    let em = match c {
        '、' | '。' | '，' | '．' => 0.664,
        '（' | '）' | '「' | '」' | '『' | '』' | '［' | '］' | '｛' | '｝' | '【' | '】' => 0.5, // i18n-ok（約物の文字の集合）
        ' ' => 0.5,
        _ if c.is_ascii() => 0.508,
        _ if full_width(c) => 1.0,
        _ => 0.5,
    };
    em * size
}

/// The advance, in ems, of the spaces ＭＳ 明朝 and ＭＳ ゴシック lack (or give a half em only) and Word sets by a fixed fraction of the size, whatever the typeface and the text's
/// direction (Word's PDFs of GitHub issue #188, D3a, 10.5 pt in a column and in a line: U+202F 2.102 pt = 0.2 em; U+205F 2.330 pt = 4/18 em, the
/// medium mathematical space of Unicode; in ＭＳ 明朝, メイリオ, 游明朝 and ＭＳ Ｐ明朝 alike).
fn fixed_space_em(c: char) -> Option<f32> {
    match c {
        '\u{202f}' => Some(0.2),
        '\u{205f}' => Some(4.0 / 18.0),
        // The three-per-em, four-per-em and six-per-em spaces take that fraction of the half em these faces give every space (hmtx: 0.5 em for all
        // three; Word's PDFs, ＭＳ 明朝 10.5 pt: 1.754, 1.310 and 0.854 pt = 1/6, 1/8 and 1/12 em, in a column and in a line).
        '\u{2004}' => Some(0.5 / 3.0),
        '\u{2005}' => Some(0.5 / 4.0),
        '\u{2006}' => Some(0.5 / 6.0),
        _ => None,
    }
}

/// The advance (em) Word gives U+2002..U+2006 in a typeface that is not fixed-width: not the `hmtx` of the character itself but of the typeface's EN DASH (U+2013) and
/// EM DASH (U+2014): U+2002 is the EN DASH's advance, U+2003 the EM DASH's, U+2004, U+2005 and U+2006 a third, a quarter and a sixth of it. `dash` gives a character's advance
/// in the typeface. Word's XPS of 17 typefaces (GitHub issue #188, `tasks/doc/requests/2026-11-25-188-mechanism-result.md`): メイリオ U+2002 0.6208 em = 1272/2048 (EN DASH),
/// ＭＳ Ｐ明朝 U+2003 0.6674 = 171/256 (EM DASH), Calibri U+2004 0.3027 = 0.9053/3. (ＭＳ 明朝 and ＭＳ ゴシック are `fixed_space_em`: their EM DASH is half an em but U+2003 stays one em.)
fn dash_space_em(c: char, dash: impl Fn(char) -> f32) -> Option<f32> {
    match c {
        '\u{2002}' => Some(dash('\u{2013}')),
        '\u{2003}' => Some(dash('\u{2014}')),
        '\u{2004}' => Some(dash('\u{2014}') / 3.0),
        '\u{2005}' => Some(dash('\u{2014}') / 4.0),
        '\u{2006}' => Some(dash('\u{2014}') / 6.0),
        _ => None,
    }
}

/// The advance (em) of the spaces 游明朝 and 游ゴシック lack (their cmaps have no U+2000, U+2001, U+2004..U+200A, U+202F; U+205F too), which Word draws in a fallback typeface
/// at that typeface's `hmtx`: Times New Roman for 游明朝 (but U+205F), Cambria Math for 游ゴシック (and U+205F of both). Word's XPS of GitHub issue #188
/// (`tasks/doc/requests/2026-11-25-188-space-advance-result.md`, 3.1, the `FontUri` of each run; the fallback typefaces' `hmtx` over 2048): U+2000 0.5, U+2001 1, U+2004 1/3,
/// U+2005 1/4, U+2006 1/6, U+2007 0.5 (Cambria Math 0.5537), U+2008 1/4 (0.2050), U+2009 0.2002 (0.1665), U+200A 0.0835 (0.0556), U+202F 0.2002, U+205F 0.2222.
/// (A table of the stand-in typefaces' widths, which a PC without them needs: the exception of the rule against measured coefficients.)
fn yu_space_fallback_em(name: &str, c: char) -> Option<f32> {
    let n = name.trim();
    let gothic = n.starts_with("游ゴシック") || n.starts_with("Yu Gothic"); // i18n-ok（フォント名の判定。訳さない）
    if !gothic && !(n.starts_with("游明朝") || n.starts_with("Yu Mincho")) { // i18n-ok（フォント名の判定。訳さない）
        return None;
    }
    let math = |times: f32, cambria: f32| if gothic { cambria } else { times };
    Some(match c {
        // 游明朝's own space (hmtx 532/2048 of yumin.ttf; Word 2.724 pt of 10.5 pt text, section 3 of the Word result), for a PC without the face (Mac). 游明朝 Light is 682/2048, not this.
        ' ' | '\u{a0}' if !gothic && matches!(n, "游明朝" | "Yu Mincho") => 532.0 / 2048.0, // i18n-ok（フォント名の判定。訳さない）
        '\u{2000}' => 0.5,
        '\u{2001}' => 1.0,
        '\u{2004}' => 682.0 / 2048.0,
        '\u{2005}' => 0.25,
        '\u{2006}' => 341.0 / 2048.0,
        '\u{2007}' => math(0.5, 0.5537),
        '\u{2008}' => math(0.25, 0.2050),
        '\u{2009}' => math(410.0 / 2048.0, 0.1665),
        '\u{200a}' => math(0.0835, 0.0556),
        '\u{202f}' => 410.0 / 2048.0,
        '\u{205f}' => 4.0 / 18.0,
        _ => return None,
    })
}

/// The advance (em) of `c` in typeface `font` (`None`: the default), as this layout sets it: a fixed-width Japanese face is half
/// or one em, a stand-in for an uninstalled ＭＳ Ｐ face has Word's widths, any other face its own glyph. It is what 縦中横's
/// 行の幅に合わせる (`ibis_doc_model::tate_chu_yoko_fit_scale`) measures a run by.
pub(crate) fn advance_em(res: &Resources, font: Option<&str>, c: char) -> f32 {
    const EM: f32 = 100.0;
    match font {
        // The narrow no-break space is a fifth of an em (Unicode's width for it; the fixed-width fonts lack it): Word's PDF of GitHub issue #188 (D3a, ＭＳ 明朝
        // 10.5 pt in a column) sets it 2.1 pt, where `full_width` guessed an em.
        Some(n) if fixed_width(n) && fixed_space_em(c).is_some() => fixed_space_em(c).unwrap_or(0.5),
        Some(n) if fixed_width(n) => if fixed_full_width(c) { 1.0 } else { 0.5 },
        Some(n) if proportional_ms(n) && !res.has_typeface(n) => dash_space_em(c, |d| proportional_ms_width(n, d, EM) / EM).unwrap_or_else(|| proportional_ms_width(n, c, EM) / EM),
        _ => {
            let f = res.font(&TextStyle { font_size: (EM * 100.0) as u32, font: font.map(str::to_string), ..TextStyle::default() });
            dash_space_em(c, |d| res.text_width(&d.to_string(), &f, EM) / EM).unwrap_or_else(|| res.text_width(&c.to_string(), &f, EM) / EM)
        }
    }
}

/// The room `c` takes at `size` in typeface `name`: for ＭＳ 明朝 and ＭＳ ゴシック the advance their font files have (`fixed_full_width`: one em or half an em,
/// the same for every machine, as the text beside it is set), else `measured`, what this machine's face for the typeface gives it. A Mac that draws such a typeface
/// with another face (it has no ＭＳ 明朝 of its own, or its face lacks the character) measures a glyph the face lacks as half an em, and the enclosure's □ (EQ \o\ac) took
/// 5.25 pt where Word gives 10.5 (GitHub issue #180, D6b).
pub(crate) fn typeface_room(name: Option<&str>, c: char, size: f32, measured: f32) -> f32 {
    match name {
        Some(n) if fixed_width(n) => match fixed_space_em(c) {
            Some(em) => size * em,
            None if fixed_full_width(c) => size,
            None => size / 2.0,
        },
        _ => measured,
    }
}

/// A character one em wide in ＭＳ 明朝 / ＭＳ ゴシック: the advance their font files give it (hmtx: Greek, Cyrillic and Ǎ one em, the dashes, •, arrows and
/// the like half an em, GitHub issue #167); a character they lack takes `full_width`'s guess.
fn fixed_full_width(c: char) -> bool {
    match crate::render::font_metrics::ms_fixed_advance_em(c) {
        Some(em) => em > 0.75,
        None => full_width(c),
    }
}

/// A character drawn full-width in a Japanese fixed-width font (a guess by the Unicode range: for the characters the font files do not have, and the stand-in for ＭＳ Ｐ明朝).
fn full_width(c: char) -> bool {
    let u = c as u32;
    (u >= 0x2000 && !(0xff61..=0xff9f).contains(&u) && !(0x2000..=0x200b).contains(&u))
        || matches!(c, '§' | '¨' | '°' | '±' | '´' | '¶' | '×' | '÷')
}

/// The characters a line may not start with when `w:strictFirstAndLastChars` is set (高), and when it is not (標準), and those a line may not
/// end with: ECMA-376's lists (Annexes H, D and E), which Word's PDFs follow (`render::line_break`, GitHub issue #68).
pub(crate) const STRICT_NO_LINE_START: &str = crate::render::line_break::WORD_STRICT_NO_LINE_START;
pub(crate) const NORMAL_NO_LINE_START: &str = crate::render::line_break::WORD_NO_LINE_START;
pub(crate) const NO_LINE_END: &str = crate::render::line_break::WORD_NO_LINE_END;

fn is_cjk(c: char) -> bool {
    c as u32 >= 0x2e80
}

/// A letter of the Latin, Greek or Cyrillic script: the letters Word sets in the font of the text's `hAnsi` slot (the ones it spaces from Japanese text
/// with `autoSpaceDE`: é, α and а as the ASCII letters, Word's PDF of GitHub issue #166's `autospace-letters.docx`). The letters of the other scripts
/// (Hebrew, Arabic, Thai, …) are of the complex-script slot and are not measured.
///
/// Beyond those, the letters Word's PDF spaces (Windows Word, 2026-11-26, ＭＳ 明朝 10.5 pt, 1032 characters of the blocks from U+0080 to U+2FFF that hold Latin letters, `あ<c>い`
/// with `autoSpaceDE` on and off; 233 are spaced, 5.28 pt wider with it): Armenian (U+0531..0556, 0561..0587), the Latin phonetic letters that are not modifiers (U+1D00..1D2B,
/// 1D62..1D77, 1D79..1D9A), the superscript i and n (U+2071, 207F), the letters of the Letterlike Symbols block (ℂ ℇ ℊ..ℓ ℕ ℙ..ℝ ℤ Ω ℨ K Å ℬ..ℭ ℯ..ℴ ℹ ℼ..ⅉ ⅎ: the Å of GitHub issue
/// #188 is U+212B), Ↄ ↄ, and the ligatures ﬀ..ﬆ. Not the letters that are symbols there (℀ ℃ № ℮ ℵ..ℸ), the roman numerals, the Latin Extended-C and -D blocks, the modifier letters
/// (U+02B0.., 1D2C..1D61, 1D78, 2090..209C), the fullwidth letters (U+FF21.., a cell of their own) and the enclosed ones.
fn is_western_letter(c: char) -> bool {
    c.is_alphabetic()
        && matches!(c as u32,
            0x41..=0x5a | 0x61..=0x7a | 0xaa | 0xb5 | 0xba | 0xc0..=0x2af | 0x370..=0x52f | 0x1e00..=0x1fff
            | 0x531..=0x556 | 0x561..=0x587
            | 0x1d00..=0x1d2b | 0x1d62..=0x1d77 | 0x1d79..=0x1d9a
            | 0x2071 | 0x207f
            | 0x2102 | 0x2107 | 0x210a..=0x2113 | 0x2115 | 0x2119..=0x211d | 0x2124 | 0x2126 | 0x2128 | 0x212a..=0x212d | 0x212f..=0x2134 | 0x2139 | 0x213c..=0x213f | 0x2145..=0x2149 | 0x214e
            | 0x2183 | 0x2184
            | 0xfb00..=0xfb06)
}

/// A character of the Han, Hiragana or Katakana script: what Word takes for Japanese text in the spacing between Japanese and Latin letters
/// (`autoSpaceDE`, `autoSpaceDN`). Unicode's Script property tells them from the characters of the common script that the Japanese text uses and
/// that Word does not space: ー (U+30FC, as the middle dot ・ U+30FB, Word's PDFs of DTBA and DFLD: "auto・セル" and "2000・tcW" without the
/// space; GitHub issue #166: かー and nー have none), the combining voiced marks and the half-width ones (ﾞ ﾟ ｰ). The half-width katakana ｱ is
/// Japanese (n and ｱ have the space).
fn is_japanese_script(c: char) -> bool {
    matches!(c as u32,
        0x3041..=0x3098 | 0x309d..=0x309f // Hiragana
        | 0x30a1..=0x30fa | 0x30fd..=0x30ff // Katakana (not ゠ U+30A0, ・ U+30FB, ー U+30FC)
        | 0x31f0..=0x31ff | 0xff66..=0xff6f | 0xff71..=0xff9d // small ㇰ…, half-width katakana (not ｰ U+FF70)
        | 0x3400..=0x9fff | 0xf900..=0xfaff) // Han
}

impl Layouter<'_> {
    /// Whether a line that starts at `start` may break before `units[i]` (`i > start`): the characters' own rules (`may_break`), and not between full-width
    /// spaces that a closing character follows, nor after spaces that a character a line may not end with comes before.
    fn break_allowed(&self, p: &FlowParagraph, units: &[Unit], start: usize, i: usize) -> bool {
        let prev = unit_char(&units[i - 1]);
        let c = unit_char(&units[i]);
        // Not before full-width spaces that a closing character follows: they would start the line with it (Word's PDFs of
        // `spacecap-compat11-mincho-w41453.docx`: the character before the spaces goes down with them).
        let spaces_then_closer = c == Some('\u{3000}') && {
            let mut j = i;
            while j < units.len() && unit_char(&units[j]) == Some('\u{3000}') {
                j += 1;
            }
            units.get(j).and_then(unit_char).is_some_and(|ch| {
                "、。，．,）」』】］｝’”".contains(ch) // i18n-ok（閉じ括弧・句読点の文字の集合。文字の並び）
                    || (p.kinsoku && self.doc.no_line_start.as_deref().unwrap_or(if self.doc.strict_first_and_last_chars { STRICT_NO_LINE_START } else { NORMAL_NO_LINE_START }).contains(ch))
            })
        };
        // Nor after spaces that a character a line may not end with (「) comes before: Word looks past the spaces, and the 「 goes down with
        // them (Word's PDF of `kinsoku-across-space.docx`, GitHub issue #162).
        let spaces_after_opener = p.kinsoku && prev.is_some_and(|ch| ch.is_whitespace()) && {
            let mut j = i;
            while j > start && unit_char(&units[j - 1]).is_some_and(|ch| ch.is_whitespace()) {
                j -= 1;
            }
            j > start && unit_char(&units[j - 1]).is_some_and(|ch| self.doc.no_line_end.as_deref().unwrap_or(NO_LINE_END).contains(ch))
        };
        may_break(prev, c, p.kinsoku, p.word_wrap, self.doc) && !spaces_then_closer && !spaces_after_opener
    }
}

mod justify_choice;
use justify_choice::JustifyChoice;

/// One unit of a paragraph's text.
#[derive(Clone, Debug)]
enum Unit {
    /// `inset` centres the glyph in a fixed-width cell it does not fill.
    /// `squeezed`: the half em of a stop or bracket was already taken off its width (a pair of them kerned, or a line that took it in).
    /// `line` and `ascent` are the height a line of the character's typeface
    /// takes and its ascent (points), as Word measures them. `metric` is the size they are measured at: the run's own size, which a script (a superscript's `size` is smaller) keeps.
    Char { c: char, width: f32, inset: f32, font: Font, size: f32, style: usize, line: f32, plain_line: f32, ascent: f32, lead: f32, auto_space: f32, squeeze: f32, squeezed: bool, metric: f32 },
    Tab,
    PositionalTab { align: TabAlign, relative_to_margin: bool, leader: bool },
    /// The tab after a list item's number: to the hanging indent (the text's
    /// left edge) when that is ahead, else as a tab.
    ListTab,
    /// `ee_top` and `ee_bottom`: the room the picture's effects (`wp:effectExtent`) take above and below it, which sits on the baseline with them.
    Picture { media: MediaId, w: f32, h: f32, ee_top: f32, ee_bottom: f32 },
    Shape { shape: Box<ibis_doc_model::Shape>, w: f32, h: f32 },
    /// A typeset formula (`w`, `h` its width and height, `descent` the part of `h` below its baseline).
    Math { boxed: Arc<crate::render::math_view::MathBox>, w: f32, h: f32, descent: f32, font: Font, color: Hsla, display: bool },
    LineBreak,
    PageBreak,
    ColumnBreak,
}

/// Set by the tools that count what asking the table of a typeface's file first changes (`tests/docx/fallback_carry.rs`): the installed typeface's own cmap is asked first, as before.
pub(crate) static INSTALLED_FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Set by the tools that count what Word's carrying over of a fallback typeface changes (`tests/docx/fallback_carry.rs`): no character takes the typeface of the one before.
pub(crate) static CARRY_OFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A laid-out line: its height, where its baseline is, what is on it (x
/// from the paragraph's left edge) and whether a page break follows it.
#[derive(Clone, Debug, Default)]
struct LineBox {
    height: f32,
    baseline: f32,
    /// How far below `baseline` the text is drawn (see `text_dy`).
    text_dy: f32,
    /// The room the line grid adds below the text (half of what it adds: the rest is above): a line whose text fits the page stays on
    /// it though this hangs past the foot (Word's PDFs of a grid line at the page's end, Century 10.5 pt: 2 pt stays, 3 pt goes on).
    grid_below: f32,
    items: Vec<Item>,
    page_break: bool,
    column_break: bool,
    has_revision: bool,
    comment_marks: Vec<u32>,
}

#[derive(Clone, Debug)]
struct WrapObstacle {
    shape_left: f32,
    shape_right: f32,
    y_top: f32,
    y_bottom: f32,
    wrap_text: String,
    wrap_polygon: Option<(ibis_doc_model::WrapPolygon, f32, f32, f32, f32, f32, f32)>,
    /// A floating table's (stacked tables are one obstacle: a table that joins them takes the place of the earlier ones).
    float_table: bool,
}

#[derive(Clone, Debug)]
struct WrapObstacleInPara {
    shape_left: f32,
    shape_right: f32,
    rel_top: f32,
    rel_bottom: f32,
    wrap_text: String,
    wrap_polygon: Option<(ibis_doc_model::WrapPolygon, f32, f32, f32, f32, f32, f32)>,
    shape_to_emit: Option<(ibis_doc_model::Shape, f32, f32, f32, f32, bool)>,
}

/// The shapes of a paragraph that the text wraps round (a paragraph can anchor more than one).
fn square_shapes(p: &FlowParagraph) -> impl Iterator<Item = &ibis_doc_model::Shape> {
    p.runs.iter().filter_map(|r| match &r.kind {
        FlowRunKind::Shape(shape) if shape.floating.as_ref().is_some_and(|f| matches!(f.wrap.as_str(), "square" | "tight" | "through")) => Some(&**shape),
        _ => None,
    })
}

/// A drop cap's frame paragraph, as Word writes it (`w:framePr w:dropCap`): the paragraph is the capital, takes no height of its own, and the paragraph
/// after it goes round it. The editor makes its drop caps in this form too (`edit_drop_cap`).
fn is_cap_frame(p: &FlowParagraph) -> bool {
    p.drop_cap.is_some_and(|d| d.location != ibis_doc_model::DropCapLocation::None)
}

/// What Word's `w:sz` and `w:position` of a drop cap's capital are worked from, in ems of the typeface: its Windows ascent and descent (`OS/2`), GDI's external leading, and the
/// lowest and highest points of the capital's glyph (`yMin`, `yMax`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct DropCapFace {
    pub win_ascent: f32,
    pub win_descent: f32,
    pub external_leading: f32,
    pub ink_min: f32,
    pub ink_max: f32,
}

/// The size (points, in half-points) and the raise (points, in half-points) Word gives the capital of a drop cap of `lines` lines over a body of `body` points (Windows Word,
/// 2026-11-16, 3790 drop caps in 26 typefaces: `w:sz` within 0.5 pt in 98.8%, `w:position` within one half-point in 99.7%, none by a typeface's constant;
/// `tasks/doc/requests/2026-11-16-191-dropcap-sz-result.md`). `p_dc` is the drop cap's line pitch, `e` how far the body line's foot is below its baseline (on a line grid: of the line
/// without the grid), `shift` how far the grid lowers the capital's baseline (0 off a grid). The capital's ink runs from the line top
/// `max(P_dc - e - (1 - winD) body, (winH + EL - 1) body + shift)` (the top of the em square, or the line's internal and external leading, whichever is lower) to
/// `lines P_dc - e` (the foot of its last line), so the size is that over the glyph's height, and the raise puts the glyph's lowest point on that foot:
/// `e + shift - 0.2 lines P_dc - yMin S`. Both are rounded to a half-point; the size does not pass 1638 pt (`w:sz` 3276).
pub(crate) fn drop_cap_size_and_raise(face: &DropCapFace, body: f32, lines: u32, p_dc: f32, e: f32, shift: f32) -> Option<(f32, f32)> {
    let h = face.ink_max - face.ink_min;
    if h < 0.05 || lines == 0 || body <= 0.0 {
        return None;
    }
    let line = lines as f32 * p_dc;
    let bottom = line - e;
    let em_top = p_dc - e - (1.0 - face.win_descent) * body;
    let leading_top = (face.win_ascent + face.win_descent + face.external_leading - 1.0) * body + shift;
    let half = |v: f32| (v * 2.0).round() / 2.0;
    let size = half((bottom - em_top.max(leading_top)) / h).min(1638.0);
    if size < 1.0 {
        return None;
    }
    Some((size, half(e + shift - 0.2 * line - face.ink_min * size)))
}

/// The `DropCapFace` of the capital `c` in typeface `name`, from the typeface's file and what the layout takes for the typeface's Windows ascent and descent. `None` when `c` has no glyph.
pub(crate) fn drop_cap_face(res: &Resources, name: &str, c: char) -> Option<DropCapFace> {
    let (ink_min, ink_max) = res.glyph_ink_em(name, c)?;
    let font = res.font(&TextStyle { font: Some(name.into()), ..TextStyle::default() });
    let (win_ascent, win_descent) = Layouter::win_ascent_descent(Some(name), name, || {
        let (a, d, _) = res.line_metrics(&font);
        (a, d)
    });
    // GDI's external leading from the typeface's file where it is installed, else the table of the typefaces Office ships (0 for MS: their hhea gap is 0).
    let external_leading = res
        .win_row_metrics(name)
        .map(|m| m.external_leading())
        .or_else(|| crate::render::font_metrics::office_external_leading(name))
        .unwrap_or(0.0);
    Some(DropCapFace { win_ascent, win_descent, external_leading, ink_min, ink_max })
}

/// A shape set from the page (not from its paragraph or line): its place does not depend on where the paragraph is.
fn is_page_shape(shape: &ibis_doc_model::Shape) -> bool {
    shape.floating.as_ref().is_some_and(|f| f.relative_v != "paragraph" && f.relative_v != "line")
}

/// The page's measures a floating shape is set from.
struct FloatGeometry {
    page_w: f32,
    page_h: f32,
    margin_l: f32,
    margin_r: f32,
    margin_t: f32,
    margin_b: f32,
    /// The width of the column the text is in (what `column` and `margin` mean horizontally).
    content_w: f32,
    even: bool,
}

impl FloatGeometry {
    fn new(page: &ibis_doc_model::PageSetup, content_w: f32, page_number: u32) -> Self {
        Self {
            page_w: pt(page.size.width),
            page_h: pt(page.size.height),
            margin_l: pt(page.left),
            margin_r: pt(page.right),
            margin_t: pt(page.top),
            margin_b: pt(page.bottom),
            content_w,
            even: page_number % 2 == 0,
        }
    }
}

/// The size an alignment places and the offset kept from it: a shape that follows a frame (`frame_size`) is aligned as the frame is, and lies its offset from the frame's
/// corner; any other shape is aligned by its own size.
fn aligned_size(f: &ibis_doc_model::FloatingAnchor, sw: f32, sh: f32) -> (f32, f32, f32, f32) {
    match f.frame_size {
        Some((w, h)) => {
            let (ax, ay) = (f.align_h.is_some(), f.align_v.is_some());
            (if ax { pt(w) } else { sw }, if ay { pt(h) } else { sh }, if ax { pt(f.offset_x) } else { 0.0 }, if ay { pt(f.offset_y) } else { 0.0 })
        }
        None => (sw, sh, 0.0, 0.0),
    }
}

/// Where a floating shape's left edge is on the page, from its anchor's horizontal position (the one place this is worked out: the text round it, the
/// obstacle it leaves on the page and the drawing of it all take it from here).
fn float_x(f: &ibis_doc_model::FloatingAnchor, sw: f32, g: &FloatGeometry, char_x: f32) -> f32 {
    let (aw, _, rx, _) = aligned_size(f, sw, 0.0);
    let (base_x0, base_w) = match f.relative_h.as_str() {
        "page" => (0.0, g.page_w),
        "leftMargin" => (0.0, g.margin_l),
        "rightMargin" => (g.page_w - g.margin_r, g.margin_r),
        "insideMargin" => if g.even { (g.page_w - g.margin_r, g.margin_r) } else { (0.0, g.margin_l) },
        "outsideMargin" => if g.even { (0.0, g.margin_l) } else { (g.page_w - g.margin_r, g.margin_r) },
        _ /* "column" | "margin" */ => (g.margin_l, g.content_w),
    };
    // From the anchor character (`char_x`: its distance from the column's left edge in the line it is in, as the text would be set without the shape;
    // Word's PDF of the issue #191 file: an anchor after 19 characters of 10.5 pt has the box at 72 + 199.5 = 271.5, and 60 pt of offset 331.5).
    let abs_x = if f.relative_h == "character" {
        let at = g.margin_l + char_x;
        match f.align_h.as_deref() {
            Some("center") => at - aw / 2.0,
            Some("right") => at - aw,
            Some("left") => at,
            _ => at + pt(f.offset_x),
        }
    } else {
        match f.align_h.as_deref() {
            Some("left") => base_x0,
            Some("center") => base_x0 + (base_w - aw) / 2.0,
            Some("right") => base_x0 + base_w - aw,
            Some("inside") => if g.even { base_x0 + base_w - aw } else { base_x0 },
            Some("outside") => if g.even { base_x0 } else { base_x0 + base_w - aw },
            _ => base_x0 + pt(f.offset_x),
        }
    };
    abs_x.clamp(0.0, (g.page_w - aw).max(0.0)) + rx
}

/// Where a floating shape's top is on the page, from its anchor's vertical position: `para_top` is the top of its paragraph, `line_h` the height of
/// its line (what `paragraph` and `line` mean).
fn float_y(f: &ibis_doc_model::FloatingAnchor, sh: f32, g: &FloatGeometry, para_top: f32, line_h: f32) -> f32 {
    let (_, sh, _, ry) = aligned_size(f, 0.0, sh);
    let (base_y0, base_h) = match f.relative_v.as_str() {
        "page" => (0.0, g.page_h),
        "margin" => (g.margin_t, (g.page_h - g.margin_t - g.margin_b).max(0.0)),
        "topMargin" => (0.0, g.margin_t),
        "bottomMargin" => (g.page_h - g.margin_b, g.margin_b),
        "insideMargin" => if g.even { (g.page_h - g.margin_b, g.margin_b) } else { (0.0, g.margin_t) },
        "outsideMargin" => if g.even { (0.0, g.margin_t) } else { (g.page_h - g.margin_b, g.margin_b) },
        "line" => (para_top, line_h),
        _ /* "paragraph" */ => (para_top, 0.0),
    };
    let below = |at: f32| if base_h > 0.0 { at + base_h - sh } else { at };
    ry + match f.align_v.as_deref() {
        Some("top") => base_y0,
        Some("center") => if base_h > 0.0 { base_y0 + (base_h - sh) / 2.0 } else { base_y0 },
        Some("bottom") => below(base_y0),
        Some("inside") => if g.even { below(base_y0) } else { base_y0 },
        Some("outside") => if g.even { base_y0 } else { below(base_y0) },
        _ => base_y0 + pt(f.offset_y),
    }
}

/// A shape that wraps the text: where it is (`sx` from the left margin, `abs_y` on the page), its size and the room the text keeps from it.
struct FloatBox<'a> {
    shape: &'a ibis_doc_model::Shape,
    sx: f32,
    abs_y: f32,
    sw: f32,
    sh: f32,
    dl: f32,
    dr: f32,
    dt: f32,
    db: f32,
    wrap_text: String,
    /// The outline the text follows (`tight` and `through`).
    polygon: Option<ibis_doc_model::WrapPolygon>,
}

impl FloatBox<'_> {
    /// What stays on the page for the paragraphs after it: in the page's y.
    fn obstacle(&self) -> WrapObstacle {
        WrapObstacle {
            shape_left: self.sx - self.dl,
            shape_right: self.sx + self.sw + self.dr,
            y_top: self.abs_y - self.dt,
            y_bottom: self.abs_y + self.sh + self.db,
            wrap_text: self.wrap_text.clone(),
            wrap_polygon: self.polygon.clone().map(|poly| (poly, self.sx, self.abs_y, self.sw, self.sh, self.dl, self.dr)),
            float_table: false,
        }
    }

    /// What the paragraph's lines go round, in the paragraph's y (`para_top` is the page's y of its top); the shape is drawn with the paragraph.
    fn in_paragraph(&self, para_top: f32) -> WrapObstacleInPara {
        let sy = self.abs_y - para_top;
        WrapObstacleInPara {
            shape_left: self.sx - self.dl,
            shape_right: self.sx + self.sw + self.dr,
            rel_top: sy - self.dt,
            rel_bottom: sy + self.sh + self.db,
            wrap_text: self.wrap_text.clone(),
            wrap_polygon: self.polygon.clone().map(|poly| (poly, self.sx, sy, self.sw, self.sh, self.dl, self.dr)),
            shape_to_emit: Some((self.shape.clone(), self.sx, sy, self.sw, self.sh, self.shape.floating.as_ref().is_some_and(|f| f.behind_doc))),
        }
    }
}

impl WrapObstacle {
    /// What the paragraph whose top is at `para_top` on the page goes round (the shape itself was drawn with the paragraph it is anchored in).
    fn in_paragraph(&self, para_top: f32) -> WrapObstacleInPara {
        WrapObstacleInPara {
            shape_left: self.shape_left,
            shape_right: self.shape_right,
            rel_top: self.y_top - para_top,
            rel_bottom: self.y_bottom - para_top,
            wrap_text: self.wrap_text.clone(),
            wrap_polygon: self.wrap_polygon.as_ref().map(|(poly, sx, abs_y, sw, sh, dl, dr)| (poly.clone(), *sx, *abs_y - para_top, *sw, *sh, *dl, *dr)),
            shape_to_emit: None,
        }
    }
}

pub(crate) struct Layouter<'r> {
    res: &'r Resources,
    doc: &'r FlowDocument,
    /// The numbers and bullets of list items, by paragraph.
    labels: std::collections::HashMap<u32, String>,
    /// How page numbers are written where laying out is (the section's).
    page_format: std::cell::RefCell<Option<String>>,
    /// The chapter number and its separator in front of the page number of the page whose header and
    /// footer are being laid out (`1-`; see `page_chapter`).
    chapter_prefix: std::cell::RefCell<String>,
    /// The line grid's pitch for the current section.
    line_pitch: std::cell::Cell<Option<Emu>>,
    /// The character grid's added pitch (`w:docGrid/@w:charSpace`) of the current section, where the grid sets characters.
    char_space: std::cell::Cell<Option<Emu>>,
    /// The section sets the characters on a grid (`char_grid_of`).
    char_grid: std::cell::Cell<bool>,
    /// The section's grid is `snapToChars` (characters only): it does not squeeze, and hangs (Windows Word, `2026-11-18-54-grid-result.md`).
    snap_chars: std::cell::Cell<bool>,
    /// The current section's text runs down the page (`tbRl`): its upright characters take a cell of the font size. (Not the document's last section's, `doc.page`:
    /// the bold heading of a horizontal first page took four ems for あとがき, GitHub issue #54, because the last section of that document is vertical.)
    vertical: std::cell::Cell<bool>,
    /// The width (points) of the line the paragraph being laid out breaks into (`f32::MAX` where it is not known).
    line_room: std::cell::Cell<f32>,
    /// The number of pages of the document and of the current section, and
    /// the section's number, for `NUMPAGES`, `SECTIONPAGES` and `SECTION`,
    /// once the body is paginated (the headers and footers); before, the
    /// fields show their saved result.
    page_counts: std::cell::Cell<Option<(u32, u32, u32)>>,
    /// The body's: the number of pages and of each section's pages of a
    /// first laying out (for `NUMPAGES` and `SECTIONPAGES` in the body),
    /// and the section being laid out.
    known_pages: Option<(u32, Vec<u32>)>,
    /// The page (its index) each paragraph that anchors a shape set from the page was laid on in an earlier pass, by block: a shape makes its hole
    /// from the top of its own page only (`None` in the first pass, which has none until the shapes' paragraphs are reached).
    anchor_pages: Option<std::collections::HashMap<usize, usize>>,
    /// The same, as this pass lays them out.
    page_anchors: std::cell::RefCell<std::collections::HashMap<usize, usize>>,
    body_section: std::cell::Cell<usize>,
    /// The footnotes' and endnotes' numbers (by the order of their marks),
    /// as shown.
    note_numbers: std::collections::HashMap<(bool, u32), String>,
    /// The number of the note being laid out, for its own mark.
    current_note: std::cell::RefCell<String>,
    /// 同じスタイルの段落間にスペースを追加しない: the paragraphs whose
    /// space before (first) or after (second) is not added, by paragraph.
    contextual: std::collections::HashMap<u32, (bool, bool)>,
    /// The table and cell coordinate (table_id, row_index, grid_col_index) being laid out, for formula evaluation.
    current_cell: std::cell::Cell<Option<(u32, usize, usize)>>,
    /// The paragraph whose fields are being made into text (a `STYLEREF` in the body looks back from it).
    field_paragraph: std::cell::Cell<Option<u32>>,
    /// Active comment IDs covering run `(para_id, run_index)` in the document.
    comment_runs: std::collections::HashMap<(u32, usize), Vec<u32>>,
}

fn map_comment_runs(doc: &FlowDocument) -> std::collections::HashMap<(u32, usize), Vec<u32>> {
    let mut map: std::collections::HashMap<(u32, usize), Vec<u32>> = std::collections::HashMap::new();
    let mut active: Vec<u32> = Vec::new();
    for p in ibis_doc_model::all_paragraphs(doc) {
        for (idx, r) in p.runs.iter().enumerate() {
            match r.kind {
                FlowRunKind::CommentStart(id) => {
                    active.push(id);
                }
                FlowRunKind::CommentEnd(id) => {
                    active.retain(|&c| c != id);
                }
                FlowRunKind::CommentRef(_) => {}
                _ => {
                    if !active.is_empty() {
                        map.insert((p.id, idx), active.clone());
                    }
                }
            }
        }
    }
    map
}

/// The run index of a list item's number (drawn in `list_style`).
const LABEL: usize = usize::MAX;

/// The typeface of a list item's number or bullet `c`: the list level's own, else (a bullet in the geometric shapes (●, ■, ◆…) defaults to
/// Arial in Word's PDF of DWID, not the Japanese font's) the run style's for the character.
///
/// A bullet in the symbol blocks (arrows, the geometric shapes, dingbats: U+2190 to U+27BF; and •) is not East Asian text to Word (no `w:hint` on a list's mark): it takes the Latin typeface of the non-ASCII
/// characters, `w:rFonts/@w:hAnsi` (Word's PDF of a document whose Normal style has `ascii` ＭＳ 明朝 and `hAnsi` Courier New: ■ □ in Courier New, ◆ ◇ in Cambria
/// Math and the other symbols in Segoe UI Symbol where Courier New has no glyph, all at the size of the paragraph mark, GitHub issue #142; ours drew them in the
/// East Asian typeface at its full em, twice as high).
fn label_font_name(style: &FlowRunStyle, c: char) -> Option<&str> {
    if matches!(c as u32, 0x2022 | 0x2190..=0x27BF) {
        // (Arial where the mark has no Latin typeface at all, not the East Asian one.)
        if style.h_ansi_font.is_none() && style.font.is_none() {
            return Some("Arial");
        }
        return style.font_for_with_hint(c, false);
    }
    style.font.as_deref().or_else(|| style.font_for(c))
}
#[derive(Clone, Copy)]
struct UnderlineBox {
    style: UnderlineStyle,
    size: f32,
    x: f32,
    w: f32,
    baseline: f32,
    color: Hsla,
    /// Where Word puts the underline from the typefaces' files (`Layouter::underline_band_of`): the single line's top below the baseline, its thickness, and whether
    /// the typeface is an East Asian one; `None`: the older fixed fractions of the size.
    band: Option<(f32, f32, bool)>,
}

impl UnderlineBox {
    /// Goes on to `ux + uw`: one line for the runs that follow one another, at the mean of their lines' tops and thicknesses weighted by their widths (Word's XPS
    /// of GitHub issue #192: 24 pt ＭＳ 明朝 after 10.5 pt, the line at the advance-weighted mean, not the mean of the characters).
    fn extend(&mut self, ux: f32, uw: f32, band: Option<(f32, f32, bool)>) {
        if let (Some(a), Some(b)) = (self.band, band) {
            let total = self.w + uw;
            if total > 0.0 {
                self.band = Some(((a.0 * self.w + b.0 * uw) / total, (a.1 * self.w + b.1 * uw) / total, a.2));
            }
        }
        self.w = (ux + uw) - self.x;
    }
}

#[derive(Clone, Copy)]
struct StrikeBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: Hsla,
}

#[derive(Clone, Copy)]
struct BorderBox {
    border: FlowBorder,
    x_start: f32,
    x_end: f32,
    y_top: f32,
    y_bottom: f32,
}

fn run_border_items(
    b: &FlowBorder,
    x_left: f32,
    y_top: f32,
    x_right: f32,
    y_bottom: f32,
) -> Vec<Item> {
    // A wave is a line along each side, as a paragraph's.
    if matches!(b.style, ibis_doc_model::BorderStyle::Wave | ibis_doc_model::BorderStyle::DoubleWave) {
        let half = pt(b.total()) / 2.0;
        let mut out = border_items(b, true, true, y_top + half, x_left, x_right, true, true, false);
        out.extend(border_items(b, true, false, y_bottom - half, x_left, x_right, true, true, false));
        out.extend(border_items(b, false, true, x_left + half, y_top, y_bottom, true, true, false));
        out.extend(border_items(b, false, false, x_right - half, y_top, y_bottom, true, true, false));
        return out;
    }
    let color = hsla(ibis_doc_model::Color::opaque(b.color));
    let mut out = Vec::new();
    let dashes = b.dashes().map(|d| d.into_iter().map(pt).collect::<Vec<f32>>());
    let mut offset = 0.0;
    for (width, drawn) in b.bands() {
        let t = pt(width).max(0.25);
        if drawn {
            let xl = x_left + offset;
            let xr = (x_right - offset).max(xl);
            let yt = y_top + offset;
            let yb = (y_bottom - offset).max(yt);
            let w = xr - xl;
            let h = yb - yt;
            if w > 0.0 && h > 0.0 {
                match &dashes {
                    Some(pattern) if pattern.iter().sum::<f32>() > 0.1 => {
                        let mut draw_dashed = |horizontal: bool, at: f32, start: f32, end: f32| {
                            let mut a = start;
                            let mut i = 0;
                            while a < end {
                                let len = pattern[i % pattern.len()];
                                if i % 2 == 0 {
                                    let z = (a + len).min(end);
                                    if horizontal {
                                        out.push(Item::Rect { x: a, y: at, w: z - a, h: t, color });
                                    } else {
                                        out.push(Item::Rect { x: at, y: a, w: t, h: z - a, color });
                                    }
                                }
                                a += len;
                                i += 1;
                            }
                        };
                        draw_dashed(true, yt, xl, xr);
                        draw_dashed(true, yb - t, xl, xr);
                        if h > 2.0 * t {
                            draw_dashed(false, xl, yt + t, yb - t);
                            draw_dashed(false, xr - t, yt + t, yb - t);
                        }
                    }
                    _ => {
                        out.push(Item::Rect { x: xl, y: yt, w, h: t, color });
                        out.push(Item::Rect { x: xl, y: yb - t, w, h: t, color });
                        if h > 2.0 * t {
                            out.push(Item::Rect { x: xl, y: yt + t, w: t, h: h - 2.0 * t, color });
                            out.push(Item::Rect { x: xr - t, y: yt + t, w: t, h: h - 2.0 * t, color });
                        }
                    }
                }
            }
        }
        offset += pt(width);
    }
    out
}

/// The height of a line with Hangul (ems) and how much of what it adds to a Latin or Japanese line is above the text.
const HANGUL_LINE: f32 = 1.51;
const HANGUL_ASCENT_SHARE: f32 = 0.8;

/// Where the characters that stand past the edge of a line (`..end`) begin: where the splitting first let one hang, `end` for none (one
/// that hung before a break the line then took earlier is not on the line).
/// The state of splitting one paragraph's lines that outlives a line: the line (its `start`) whose overflowing character was taken in by squeezing stops and brackets
/// (`compress_punctuation`), and the automatic spaces already squeezed off the line that starts at `.0` (`.1` pt), for the limit of the next character's squeeze.
struct BreakState {
    compressed_line: Option<usize>,
    /// How much the line that starts at `.0` has squeezed off its stops, brackets, spaces and kana so far (`.1` pt, below mode 15): the limit of the whole line is Word's
    /// (`total_cap`), not each character's.
    squeezed_so_far: (usize, f32),
    gaps_cut: (usize, f32),
    /// The units that already took the room of a border (`w:bdr`) cut by a line's edge: on their left (the line's first unit of a bordered run) and on their right (its last).
    border_left: std::collections::HashSet<usize>,
    border_right: std::collections::HashSet<usize>,
}

impl Default for BreakState {
    fn default() -> Self {
        BreakState { compressed_line: None, squeezed_so_far: (usize::MAX, 0.0), gaps_cut: (usize::MAX, 0.0), border_left: Default::default(), border_right: Default::default() }
    }
}

/// Where one part of a line lies: its start (`origin`), the room it has from there, and the edges a positional tab (`w:ptab`) aligns to. Word's measurements (the issue #196
/// file, `ＭＳ 明朝` 10.5 pt, A4, margins 72 pt, a figure 80 pt wide with 9 pt kept off the text; the column's right edge 523.3 pt):
/// - a left-aligned tab stops at the line's left indent (the margin's or the indent's, no difference: 111.98 pt for an indent of 40 pt) or the gap's left edge beside a figure
///   on the left (161.06 pt), whichever is further right, never at the margin;
/// - a centred tab is centred between the margin (or the indent) and the column's right edge, whatever the right indent and a figure: 271.37 pt for the five full-width
///   characters after it in every case; the line's own start (the gap's left edge) is the only thing that can push it right;
/// - a right-aligned tab ends at the column's right edge less the right indent (434.3 pt for 89 pt), or at the gap's right edge beside a figure on the right (the figure's
///   left edge less its 9 pt, 434.3 pt), and the left indent or a figure on the left do not move it.
#[derive(Clone, Copy)]
struct SegmentEdges {
    origin: f32,
    room: f32,
    /// The left edge of the line's gap, where a left-aligned tab stops at least.
    left_stop: f32,
    /// Where a centred tab's span starts, relative to the margin and to the indent, and where it ends (the column's right edge).
    centre: (f32, f32, f32),
    /// Where a right-aligned tab ends: the line's right edge, or the gap's when a figure ends the line.
    right_stop: f32,
}

impl SegmentEdges {
    fn new(width: f32, left: f32, right: f32, slot_x: f32, slot_w: f32, _line_start: f32, line_end: f32) -> Self {
        let slot_r = slot_x + slot_w;
        SegmentEdges {
            origin: slot_x,
            room: slot_w,
            left_stop: left.max(slot_x),
            centre: (0.0, left, width),
            right_stop: if slot_r < line_end - 0.01 { slot_r } else { width - right },
        }
    }

    /// The edges a tab of `align` aligns to: a left tab uses the first (its stop), a centred tab both (its span), a right tab the second (its end).
    fn span(&self, align: TabAlign, relative_to_margin: bool) -> (f32, f32) {
        match align {
            TabAlign::Center => (if relative_to_margin { self.centre.0 } else { self.centre.1 }, self.centre.2),
            _ => (self.left_stop, self.right_stop),
        }
    }
}

fn hang_start(hang: Option<usize>, end: usize) -> usize {
    hang.filter(|&h| h < end).unwrap_or(end)
}

fn is_hangul(c: char) -> bool {
    matches!(c, '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}' | '\u{ac00}'..='\u{d7af}')
}

/// The text items from `from` on drawn each character on its own with its
/// glyph `squeeze` times as wide (not at all when it is 1).
fn squeeze_items(items: &mut Vec<Item>, from: usize, squeeze: f32) {
    if (squeeze - 1.0).abs() < 0.001 {
        return;
    }
    let drawn: Vec<Item> = items.drain(from..).collect();
    for it in drawn {
        let Item::Text { x, y, text, offsets, font, size, color } = &it else {
            items.push(it);
            continue;
        };
        for ((i, c), dx) in text.char_indices().zip(offsets.iter()) {
            if c.is_whitespace() {
                continue;
            }
            items.push(Item::Scaled { x: x + dx, y: *y, text: text[i..i + c.len_utf8()].to_string().into(), font: font.clone(), size: *size, color: *color, xscale: squeeze });
        }
    }
}

fn push_styled_text_items(
    items: &mut Vec<Item>,
    s: &ibis_doc_model::FlowRunStyle,
    x0: f32,
    y_base: f32,
    text: String,
    offsets: Vec<f32>,
    font: Font,
    size: f32,
    color: Hsla,
) {
    // Word 2010's effects: a glow and a shadow behind, the outline around (a ring of copies), a gradient fill
    // by the character.
    if let Some(e) = &s.effects {
        let rgba = |rgb: ibis_doc_model::Rgb, alpha: u8| {
            let mut c = hsla(ibis_doc_model::Color::opaque(rgb));
            c.a *= alpha as f32 / 255.0;
            c
        };
        let copy = |items: &mut Vec<Item>, dx: f32, dy: f32, color: Hsla| items.push(Item::Text { x: x0 + dx, y: y_base + dy, text: text.clone().into(), offsets: offsets.clone().into(), font: font.clone(), size, color });
        if let Some((radius, rgb, alpha)) = e.glow {
            let r = (radius as f32 / 12_700.0).min(1.5);
            if r > 0.0 {
                for ring in 1..=3 {
                    let d = r * ring as f32 / 3.0;
                    for k in 0..12 {
                        let angle = k as f32 * std::f32::consts::TAU / 12.0;
                        copy(items, d * angle.cos(), d * angle.sin(), rgba(rgb, (alpha as f32 * 0.25) as u8));
                    }
                }
            }
        }
        if let Some(sh) = e.shadow {
            let dist = sh.distance as f32 / 12_700.0;
            let angle = (sh.direction as f32 / 60_000.0).to_radians();
            let (dx, dy) = (dist * angle.cos(), dist * angle.sin());
            let blur = sh.blur as f32 / 12_700.0;
            if blur > 0.5 {
                for (ox, oy) in [(-blur / 3.0, 0.0), (blur / 3.0, 0.0), (0.0, -blur / 3.0), (0.0, blur / 3.0)] {
                    copy(items, dx + ox, dy + oy, rgba(sh.color, (sh.alpha as f32 * 0.25) as u8));
                }
            } else {
                copy(items, dx, dy, rgba(sh.color, sh.alpha));
            }
        }
        if let Some((width, rgb)) = e.outline {
            let r = (width as f32 / 12_700.0 / 2.0).max(0.25);
            for k in 0..8 {
                let angle = k as f32 * std::f32::consts::TAU / 8.0;
                copy(items, r * angle.cos(), r * angle.sin(), rgba(rgb, 255));
            }
        }
        if let Some((stops, _angle)) = &e.gradient {
            // The colour at a position across the run (left to right).
            let total = offsets.last().copied().unwrap_or(0.0).max(0.001);
            let at = |t: f32| -> ibis_doc_model::Rgb {
                let t = (t * 100_000.0).clamp(0.0, 100_000.0);
                let upper = stops.iter().position(|(p, _)| *p as f32 >= t).unwrap_or(stops.len() - 1);
                if upper == 0 {
                    return stops[0].1;
                }
                let (p0, c0) = stops[upper - 1];
                let (p1, c1) = stops[upper];
                let f = if p1 > p0 { ((t - p0 as f32) / (p1 - p0) as f32).clamp(0.0, 1.0) } else { 0.0 };
                let mix = |a: u32, b: u32| ((a as f32) * (1.0 - f) + (b as f32) * f).round() as u32;
                ibis_doc_model::Rgb((mix(c0.0 >> 16 & 255, c1.0 >> 16 & 255) << 16) | (mix(c0.0 >> 8 & 255, c1.0 >> 8 & 255) << 8) | mix(c0.0 & 255, c1.0 & 255))
            };
            for ((i, c), start) in text.char_indices().zip(offsets.iter()) {
                let _ = i;
                let width_of_char = offsets.get(text[..i].chars().count() + 1).map_or(0.0, |next| next - start);
                let colour = rgba(at((start + width_of_char / 2.0) / total), 255);
                items.push(Item::Text { x: x0 + start, y: y_base, text: c.to_string().into(), offsets: vec![0.0].into(), font: font.clone(), size, color: colour });
            }
            return;
        }
    }
    if s.shadow {
        let dx = (size * 0.05).max(0.6);
        let dy = (size * 0.05).max(0.6);
        let shadow_color = Hsla { h: color.h, s: color.s * 0.2, l: 0.5, a: 0.4 };
        items.push(Item::Text { x: x0 + dx, y: y_base + dy, text: text.clone().into(), offsets: offsets.clone().into(), font: font.clone(), size, color: shadow_color });
    }
    if s.outline {
        let od = (size * 0.03).max(0.5);
        for (ox, oy) in [(-od, 0.0), (od, 0.0), (0.0, -od), (0.0, od)] {
            items.push(Item::Text { x: x0 + ox, y: y_base + oy, text: text.clone().into(), offsets: offsets.clone().into(), font: font.clone(), size, color });
        }
        items.push(Item::Text { x: x0, y: y_base, text: text.into(), offsets: offsets.into(), font, size, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffffff))) });
    } else if s.emboss || s.imprint {
        // Word's PDF: the text in its colour with a copy of it in 60 % grey (#999999) a point down and right (an
        // emboss) or up and left (an imprint), behind it.
        let ed = if s.emboss { 1.0f32 } else { -1.0f32 };
        items.push(Item::Text { x: x0 + ed, y: y_base + ed, text: text.clone().into(), offsets: offsets.clone().into(), font: font.clone(), size, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x999999))) });
        // The strokes of the text itself are heavier than plain text's (a third more black on the page).
        for (ox, oy) in [(-0.3f32, -0.3f32), (0.3, -0.3), (-0.3, 0.3)] {
            items.push(Item::Text { x: x0 + ox, y: y_base + oy, text: text.clone().into(), offsets: offsets.clone().into(), font: font.clone(), size, color });
        }
        items.push(Item::Text { x: x0, y: y_base, text: text.into(), offsets: offsets.into(), font, size, color });
    } else {
        items.push(Item::Text { x: x0, y: y_base, text: text.into(), offsets: offsets.into(), font, size, color });
    }
}

#[derive(Default)]
struct LineDecorations {
    highlights: Vec<Item>,
    underlines: Vec<UnderlineBox>,
    strikes: Vec<StrikeBox>,
    borders: Vec<BorderBox>,
}

impl LineDecorations {
    /// `vertical`: the line is of a `tbRl` section's across layout; the page is turned a quarter afterwards, which would put an underline
    /// (below the baseline) on the left of the column. Word's is on the right: the underline is mirrored in the characters' em box
    /// (0.85 em above the baseline, 0.15 em below, as `turn_cell_text` places an upright character).
    fn finish(self, line: &mut LineBox, vertical: bool) {
        let mut b_items = self.highlights;
        for u in self.underlines {
            let baseline = if vertical {
                let (top, thick) = underline_band(u.style, u.size);
                u.baseline - 0.7 * u.size - 2.0 * top - thick
            } else {
                u.baseline
            };
            b_items.extend(underline_items_banded(u.style, u.size, u.x, u.w, baseline, u.color, u.band));
        }
        for b in self.borders {
            // Across a line the room of the border is in the characters' advances (`border_room`): the box is their extent. A vertical column keeps the old padding.
            let extra_bw = (pt(b.border.total()) - pt(b.border.width)).max(0.0);
            let pad_h = if vertical { pt(b.border.space) + 0.75 + extra_bw } else { 0.0 };
            let x_left = b.x_start - pad_h;
            let x_right = b.x_end + pad_h;
            let y_top = b.y_top;
            let y_bottom = b.y_bottom;
            b_items.extend(run_border_items(&b.border, x_left, y_top, x_right, y_bottom));
        }
        for s in self.strikes {
            b_items.push(Item::Rect { x: s.x, y: s.y, w: s.w, h: s.h, color: s.color });
        }
        let pos = line.items.iter().position(|it| matches!(it, Item::Text { .. })).unwrap_or(line.items.len());
        line.items.splice(pos..pos, b_items);
    }
}

impl<'r> Layouter<'r> {
    /// The default tab stops' spacing (points): the settings', else 42.55 pt (851 twips).
    fn default_tab_pt(&self) -> f32 {
        self.doc.default_tab_stop.map_or(851.0 / 20.0, pt)
    }

    /// Effective markup display mode for this layout pass (honours print_markup during printing DOCX-SEC-014).
    fn markup_mode(&self) -> MarkupDisplayMode {
        if self.res.print && !self.res.print_markup {
            MarkupDisplayMode::None
        } else {
            self.res.markup_view.mode
        }
    }

    /// The shapes of an inline SmartArt (`wp:inline`) lie where its frame does: the reader gives them offsets from their paragraph, but the frame sits in its line, at the
    /// baseline (and in the middle of the grids a line grid takes for it: `picture_grid_slack`), so each is moved to its offset from the frame's place (Word's PDF
    /// `docx-smartart-word-inline.pdf`: on an 18 pt grid the figure of 248.04 pt is in a line of 252 pt, and its shapes' text is 1.98 pt lower than the line's top gives).
    fn inline_diagram_members(&self, pagination: &mut Pagination) {
        for page in &mut pagination.pages {
            let frames: std::collections::HashMap<_, _> = page
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Shape { shape, y, .. } if shape.floating.is_none() && matches!(shape.kind, ibis_doc_model::ShapeKind::Placeholder(ibis_doc_model::Placeholder::Diagram)) => Some((shape.id, *y)),
                    _ => None,
                })
                .collect();
            if frames.is_empty() {
                continue;
            }
            for group in &self.doc.shape_groups {
                let Some(ibis_doc_model::GroupMember::Shape(frame)) = group.children.first() else { continue };
                let Some(&frame_y) = frames.get(frame) else { continue };
                let members: std::collections::HashSet<_> = group.children.iter().skip(1).filter_map(|m| if let ibis_doc_model::GroupMember::Shape(id) = m { Some(*id) } else { None }).collect();
                for item in &mut page.items {
                    if let Item::Shape { shape, y, .. } = item
                        && members.contains(&shape.id)
                        && let Some(f) = &shape.floating
                    {
                        *y = frame_y + pt(f.offset_y);
                    }
                }
            }
        }
    }

    /// The lines of the text in text boxes on the line grid (`w:docGrid type="lines"`), as Word puts
    /// them: each paragraph's line is as many `linePitch`es high as its text needs (a 14 pt 游明朝
    /// line in two grids of 20 pt), as the lines of the body text are.
    fn grid_text_boxes(&self, pagination: &mut Pagination) {
        if let Some((edited_id, _, Some(composing))) = &self.res.edited_box {
            for page in &mut pagination.pages {
                for item in &mut page.items {
                    let Item::Shape { shape, .. } = item else { continue };
                    if shape.id != *edited_id {
                        continue;
                    }
                    match &mut shape.kind {
                        ibis_doc_model::ShapeKind::Auto(a) => a.text = Some(composing.clone()),
                        ibis_doc_model::ShapeKind::TextBox(body) => *body = composing.clone(),
                        _ => {}
                    }
                }
            }
        }
        let Some(pitch) = self.doc.page.line_pitch.filter(|p| *p > 0).map(pt) else { return };
        // The text of a SmartArt's shapes is not on the grid (Word's PDF `docx-smartart-word-inline.pdf`: the text is in its shape as the shape's own text rules put it,
        // under a `linePitch` of 18 pt a 29 pt line would take 2 pitches, 5.9 pt lower than Word's).
        let diagram_frames: std::collections::HashSet<_> = pagination
            .pages
            .iter()
            .flat_map(|page| page.items.iter())
            .filter_map(|item| match item {
                Item::Shape { shape, .. } if matches!(shape.kind, ibis_doc_model::ShapeKind::Placeholder(ibis_doc_model::Placeholder::Diagram)) => Some(shape.id),
                _ => None,
            })
            .collect();
        let diagram_members: std::collections::HashSet<_> = self
            .doc
            .shape_groups
            .iter()
            .filter(|g| matches!(g.children.first(), Some(ibis_doc_model::GroupMember::Shape(id)) if diagram_frames.contains(id)))
            .flat_map(|g| g.children.iter().skip(1))
            .filter_map(|m| match m {
                ibis_doc_model::GroupMember::Shape(id) => Some(*id),
                _ => None,
            })
            .collect();
        for page in &mut pagination.pages {
            for item in &mut page.items {
                let Item::Shape { shape, .. } = item else { continue };
                if diagram_members.contains(&shape.id) {
                    continue;
                }
                let body = match &mut shape.kind {
                    ibis_doc_model::ShapeKind::Auto(a) => a.text.as_mut(),
                    ibis_doc_model::ShapeKind::TextBox(body) => Some(body),
                    _ => None,
                };
                let Some(body) = body else { continue };
                // The first paragraph's baseline: where the text box draws it in an exact line, and
                // where Word does in a grid line (see `baseline_in_line`); the text is moved by the
                // difference.
                let mut shift: Option<f32> = None;
                let vertical = body.vertical;
                for paragraph in &mut body.paragraphs {
                    if matches!(paragraph.line_spacing, Some(ibis_doc_model::LineSpacing::Points(_))) {
                        continue;
                    }
                    let multiple = match paragraph.line_spacing {
                        Some(ibis_doc_model::LineSpacing::Percent(n)) => n as f32 / 100_000.0,
                        _ => 1.0,
                    };
                    // The tallest line of the runs: its ascent, its line, the room Word leaves below it
                    // (游明朝: 1.43 em in all) and the size.
                    let mut tall = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                    for r in paragraph.runs.iter().filter(|r| !r.text.is_empty()) {
                        let size = r.style.font_size.unwrap_or(body.style.font_size) as f32 / 100.0;
                        let cjk = r.text.chars().any(crate::render::rich_text::is_cjk);
                        let name = if cjk { r.style.font.as_deref() } else { r.style.fallback_font.as_deref().or(r.style.font.as_deref()) };
                        let font = self.res.font(&TextStyle { font_size: (size * 100.0) as u32, bold: r.style.bold.unwrap_or(false), italic: r.style.italic.unwrap_or(false), font: name.map(str::to_string), ..TextStyle::default() });
                        let (ascent, line) = self.line_metrics(name, &font, size);
                        let lead = self.lead_em(name) * size;
                        if line + lead > tall.1 + tall.2 {
                            tall = (ascent, line, lead, size);
                        }
                    }
                    let (ascent, line, lead, size) = tall;
                    if line <= 0.0 {
                        continue;
                    }
                    let snapped = if paragraph.no_snap_to_grid {
                        (line + lead) * multiple
                    } else {
                        (((line + lead) * multiple - 0.01) / pitch).floor() * pitch + pitch
                    };
                    paragraph.line_spacing = Some(ibis_doc_model::LineSpacing::Points((snapped * 100.0).round() as u32));
                    if vertical {
                        // A vertical box's lines are columns: whole pitches wide, the text in the middle of each (Word's PDF); the baseline
                        // of a horizontal line has no part in it.
                        paragraph.grid_pitch = (!paragraph.no_snap_to_grid).then(|| (pitch * 100.0).round() as u32);
                        continue;
                    }
                    if shift.is_none() {
                        let grid = if paragraph.no_snap_to_grid { 0.0 } else { snapped - line * multiple };
                        let word = (snapped - grid - line) * 0.58 + grid * 0.5 + ascent;
                        let drawn = crate::render::font_metrics::powerpoint_baseline(snapped, crate::render::font_metrics::POWERPOINT_SINGLE * size, ascent, line - ascent);
                        shift = Some(word - drawn);
                    }
                }
                if let Some(shift) = shift {
                    body.insets.top += (shift * 12_700.0) as i64;
                }
            }
        }
    }

    /// Where a shape that the text wraps round is on the page and the room the text keeps from it (`para_top`: the page's y of its paragraph; `line_h`:
    /// the height of its line): the one place this is worked out.
    /// The distance of the anchor of `shape` (in `p`) from the left of its column: the left edge of the character just before the anchor in the paragraph
    /// as its lines are set without the paragraph's shapes (the anchor is attached to that character, which the shape pushes right; Word's XPS, issue
    /// #191 request 5: after three "あ" of 10.5 pt the shape is 21 pt in, after the 20th 199.5 pt, a tab before it is the tab's stop), and, where
    /// nothing comes before it, the left edge of the character after it (the text's start). Taken from the lines the layout itself sets, so that the
    /// indents, a list item's number, tabs and where the line breaks are as in the text. 0 where the paragraph has no characters.
    fn anchor_x(&self, p: &FlowParagraph, shape: &ibis_doc_model::Shape, page_number: u32, width: f32) -> f32 {
        let (units, _) = self.units(p, page_number);
        let mut before = 0usize;
        let mut found = false;
        for u in &units {
            match u {
                Unit::Shape { shape: s, .. } if s.id == shape.id => {
                    found = true;
                    break;
                }
                Unit::Char { .. } => before += 1,
                _ => {}
            }
        }
        if !found {
            return 0.0;
        }
        // The paragraph without its floating shapes (they are not set in the line).
        let mut bare = p.clone();
        for run in &mut bare.runs {
            if matches!(&run.kind, FlowRunKind::Shape(sh) if sh.floating.as_ref().is_some_and(|f| matches!(f.wrap.as_str(), "square" | "tight" | "through"))) {
                run.kind = FlowRunKind::Text(String::new().into());
            }
        }
        let boxes = self.paragraph(&bare, width, page_number, &[], false);
        let mut at: Vec<f32> = Vec::new();
        for line in &boxes {
            for it in &line.items {
                if let Item::Text { x, text, offsets, .. } = it {
                    at.extend(text.chars().zip(offsets.iter()).map(|(_, o)| x + o));
                }
            }
        }
        if at.len() < units.iter().filter(|u| matches!(u, Unit::Char { .. })).count() {
            return 0.0;
        }
        let x = if before > 0 { at.get(before - 1) } else { at.first() };
        x.copied().filter(|x| *x <= width).unwrap_or(0.0)
    }

    fn float_box<'a>(&self, p: &FlowParagraph, page_number: u32, shape: &'a ibis_doc_model::Shape, g: &FloatGeometry, para_top: f32, line_h: f32) -> FloatBox<'a> {
        let f = shape.floating.as_ref().expect("a floating shape");
        let sw = pt(shape.frame.width);
        let sh = self.fitted_shape_height(shape);
        let char_x = if f.relative_h == "character" { self.anchor_x(p, shape, page_number, g.content_w) } else { 0.0 };
        let abs_x = float_x(f, sw, g, char_x);
        let abs_y = float_y(f, sh, g, para_top, line_h);
        FloatBox {
            shape,
            sx: abs_x - g.margin_l,
            abs_y,
            sw,
            sh,
            dl: pt(f.dist_l),
            dr: pt(f.dist_r),
            dt: pt(f.dist_t),
            db: pt(f.dist_b),
            wrap_text: f.wrap_text.clone().unwrap_or_else(|| "bothSides".to_string()),
            polygon: matches!(f.wrap.as_str(), "tight" | "through").then(|| f.wrap_polygon.clone()).flatten(),
        }
    }

    /// A floating text box with `spAutoFit` is as high as its text where it keeps the text off the lines (the height the file
    /// saved can be Word's default for a box, 110 pt, of which the text fills one line: corpus 063, whose date line
    /// wrapped round the room that is not there).
    fn fitted_shape_height(&self, shape: &ibis_doc_model::Shape) -> f32 {
        let h = pt(shape.frame.height);
        let body = match &shape.kind {
            ibis_doc_model::ShapeKind::Auto(a) => a.text.as_ref(),
            ibis_doc_model::ShapeKind::TextBox(body) => Some(body),
            _ => None,
        };
        let Some(body) = body else { return h };
        if body.autofit != ibis_doc_model::TextAutofit::ResizeShape || body.vertical || body.sideways != 0 || shape.rotation != 0 {
            return h;
        }
        let inner = pt(shape.frame.width) - (body.insets.left + body.insets.right) as f32 / 12_700.0;
        if inner <= 1.0 {
            return h;
        }
        h.min(self.text_body_height(body, inner) + (body.insets.top + body.insets.bottom) as f32 / 12_700.0)
    }

    /// 図形を文字に合わせる (`a:spAutoFit`): Word fits the box to its text (its lines
    /// and the insets above and below), where the size the file saved can be one from
    /// before the text changed. A box saved much taller than its text needs is
    /// drawn as high as the text (a white fill would hide what lies under the
    /// room that is not there).
    fn fit_text_boxes(&self, pagination: &mut Pagination) {
        for page in &mut pagination.pages {
            for item in &mut page.items {
                let Item::Shape { shape, w, h, .. } = item else { continue };
                let body = match &shape.kind {
                    ibis_doc_model::ShapeKind::Auto(a) => a.text.as_ref(),
                    ibis_doc_model::ShapeKind::TextBox(body) => Some(body),
                    _ => None,
                };
                let Some(body) = body else { continue };
                if body.autofit != ibis_doc_model::TextAutofit::ResizeShape || body.vertical || body.sideways != 0 || shape.rotation != 0 {
                    continue;
                }
                let inner = *w - (body.insets.left + body.insets.right) as f32 / 12_700.0;
                if inner <= 1.0 {
                    continue;
                }
                let needed = self.text_body_height(body, inner) + (body.insets.top + body.insets.bottom) as f32 / 12_700.0;
                // Word fits a box with `spAutoFit` to its text: the saved height can be from before the text changed.
                if *h > needed {
                    *h = needed;
                }
            }
        }
    }

    /// The height (points) of a text body's lines wrapped at `width`, with the room
    /// between its paragraphs (an estimate: characters wrapped one by one).
    fn text_body_height(&self, body: &ibis_doc_model::TextBody, width: f32) -> f32 {
        let mut total = 0.0f32;
        for paragraph in &body.paragraphs {
            let default_size = body.style.font_size.max(100) as f32 / 100.0;
            let mut tall = 0.0f32;
            let mut lines = 1usize;
            let mut at = 0.0f32;
            let mut any = false;
            for r in &paragraph.runs {
                let size = r.style.font_size.map_or(default_size, |s| s as f32 / 100.0);
                let name = r.style.font.as_deref();
                let font = self.res.font(&TextStyle { font_size: (size * 100.0) as u32, bold: r.style.bold.unwrap_or(false), italic: r.style.italic.unwrap_or(false), font: name.map(str::to_string), ..TextStyle::default() });
                let (_, line) = self.line_metrics(name, &font, size);
                tall = tall.max(line + self.lead_em(name) * size);
                for c in r.text.chars() {
                    any = true;
                    let cw = self.res.text_width(&c.to_string(), &font, size);
                    if at + cw > width && at > 0.0 {
                        lines += 1;
                        at = 0.0;
                    }
                    at += cw;
                }
            }
            if !any {
                let size = paragraph.end_style.font_size.map_or(default_size, |s| s as f32 / 100.0);
                tall = tall.max(size * 1.2);
            }
            let line = match paragraph.line_spacing {
                Some(ibis_doc_model::LineSpacing::Percent(n)) => tall * n as f32 / 100_000.0,
                Some(ibis_doc_model::LineSpacing::Points(p)) => p as f32 / 100.0,
                None => tall,
            };
            total += line * lines as f32 + (paragraph.space_before + paragraph.space_after) as f32 / 100.0;
        }
        total
    }

    /// A shape in a line (`Unit::Shape`): a floating one at the place its anchor says (its
    /// `x`, `y` from the margin and the line), one in the line at `x` on the baseline, which
    /// moves on by its width.
    fn emit_unit_shape(&self, shape: &Box<ibis_doc_model::Shape>, w: &f32, h: &f32, x_at: &mut f32, width: f32, line: &mut LineBox, page_number: u32) {
        let mut x = *x_at;
            if let Some(f) = &shape.floating {
                if f.wrap == "topAndBottom" {
                    return;
                }
                let sw = pt(shape.frame.width);
                let sh = self.fitted_shape_height(shape);
                let page_w = pt(self.doc.page.size.width);
                let margin_l = pt(self.doc.page.left);
                let margin_r = pt(self.doc.page.right);
                let is_even = page_number % 2 == 0;
                let (aw, ah, rx, ry) = aligned_size(f, sw, sh);

                let (base_x0, base_w) = match f.relative_h.as_str() {
                    "page" => (0.0, page_w),
                    "leftMargin" => (0.0, margin_l),
                    "rightMargin" => (page_w - margin_r, margin_r),
                    "insideMargin" => if is_even { (page_w - margin_r, margin_r) } else { (0.0, margin_l) },
                    "outsideMargin" => if is_even { (0.0, margin_l) } else { (page_w - margin_r, margin_r) },
                    "character" => (margin_l + x, (width - x).max(0.0)),
                    _ /* "column" | "margin" */ => (margin_l, width),
                };

                let abs_x = if f.relative_h == "character" {
                    match f.align_h.as_deref() {
                        Some("center") => margin_l + x - aw / 2.0,
                        Some("right") => margin_l + x - aw,
                        Some("left") => margin_l + x,
                        _ => margin_l + x + pt(f.offset_x),
                    }
                } else {
                    match f.align_h.as_deref() {
                        Some("left") => base_x0,
                        Some("center") => base_x0 + (base_w - aw) / 2.0,
                        Some("right") => base_x0 + base_w - aw,
                        Some("inside") => if is_even { base_x0 + base_w - aw } else { base_x0 },
                        Some("outside") => if is_even { base_x0 } else { base_x0 + base_w - aw },
                        _ => base_x0 + pt(f.offset_x),
                    }
                };
                // A box anchored in a table cell with its place counted from the column (`layoutInCell`) is placed from
                // the cell's left edge, whatever the offset's sign (it may lie to the left of the cell, past the page).
                let in_cell = f.layout_in_cell && self.current_cell.get().is_some() && f.align_h.is_none() && matches!(f.relative_h.as_str(), "column" | "margin");
                let abs_x = if in_cell { margin_l + pt(f.offset_x) } else { abs_x.clamp(0.0, (page_w - aw).max(0.0)) + rx };
                let sx = abs_x - margin_l;
                let page_h = pt(self.doc.page.size.height);
                let margin_t = pt(self.doc.page.top);
                let margin_b = pt(self.doc.page.bottom);
                let sy = if f.relative_v == "line" {
                    match f.align_v.as_deref() {
                        Some("center") => ry + (line.height - ah) / 2.0,
                        Some("bottom") => ry + line.height - ah,
                        Some("top") => ry,
                        _ => pt(f.offset_y),
                    }
                } else if f.relative_v == "paragraph" {
                    match f.align_v.as_deref() {
                        Some("center") => ry + (line.height - ah) / 2.0,
                        Some("bottom") => ry + line.height - ah,
                        Some("top") => ry,
                        _ => pt(f.offset_y),
                    }
                } else {
                    let (base_y0, base_h) = match f.relative_v.as_str() {
                        "page" => (0.0, page_h),
                        "margin" => (margin_t, (page_h - margin_t - margin_b).max(0.0)),
                        "topMargin" => (0.0, margin_t),
                        "bottomMargin" => (page_h - margin_b, margin_b),
                        "insideMargin" => if is_even { (page_h - margin_b, margin_b) } else { (0.0, margin_t) },
                        "outsideMargin" => if is_even { (0.0, margin_t) } else { (page_h - margin_b, margin_b) },
                        _ => (margin_t, page_h),
                    };
                    let abs_y = ry + match f.align_v.as_deref() {
                        Some("top") => base_y0,
                        Some("center") => if base_h > 0.0 { base_y0 + (base_h - ah) / 2.0 } else { base_y0 },
                        Some("bottom") => if base_h > 0.0 { base_y0 + base_h - ah } else { base_y0 },
                        Some("inside") => if is_even { if base_h > 0.0 { base_y0 + base_h - ah } else { base_y0 } } else { base_y0 },
                        Some("outside") => if is_even { base_y0 } else { if base_h > 0.0 { base_y0 + base_h - ah } else { base_y0 } },
                        _ => base_y0 + pt(f.offset_y),
                    };
                    abs_y - margin_t
                };
                let item = Item::Shape {
                    shape: shape.clone(),
                    x: sx,
                    y: sy,
                    w: sw,
                    h: sh,
                };
                if f.behind_doc {
                    line.items.insert(0, item);
                } else {
                    line.items.push(item);
                }
            } else {
                // A turned picture's own frame is centred in the room its turn takes.
                let (fw, fh) = (pt(shape.frame.width), pt(shape.frame.height));
                let turned = matches!(shape.kind, ibis_doc_model::ShapeKind::Picture(_)) && shape.rotation != 0;
                let (fx, fy, fw, fh) = if turned { (x + (*w - fw) / 2.0, line.baseline - h + (*h - fh) / 2.0, fw, fh) } else { (x, line.baseline - h, *w, *h) };
                line.items.push(Item::Shape { shape: shape.clone(), x: fx, y: fy, w: fw, h: fh });
                x += w;
            }
        *x_at = x;
    }

    /// How much a line's part above the baseline grows for the inline pictures in it: a picture (its `wp:effectExtent` room too) sits on the baseline, and
    /// the line is the text's line with its part above the baseline as high as the tallest picture (Word's PDFs of `n2-line-height.docx`, GitHub issue #179:
    /// 14 / 20 / 40 pt pictures in 10.5 pt ＭＳ 明朝 are 17.0 / 23.0 / 43.0 pt lines, the text's 13.6 and its 3.0 pt below the baseline kept; effect extents of
    /// 5 pt above and below add 10 pt; in a line of three lines' spacing the two lines the multiple adds stay below, 70.3 pt = 40 + 3.0 + 2 x 13.6; in an
    /// exact line the line stays as high as it is and the baseline is where it is without a picture, 0.8 of the line). `None` where the picture is not
    /// told this way: on a line grid (the grids the line takes are those of the picture, `line_height_around`).
    fn picture_growth(&self, p: &FlowParagraph, text_line: f32, text_ascent: f32, lead: f32, in_table: bool, ms_room: f32, pictures: f32) -> Option<f32> {
        if self.line_height_parts(p, text_line, lead, in_table).1 > 0.0 {
            return None;
        }
        if matches!(p.line, LineHeight::Exact(_)) {
            return Some(0.0);
        }
        // A multiple under 1 puts the picture on the single line's baseline (a = max(A, the picture)), and shrinks the line after.
        let above = if self.multiple_below_one(p, in_table) { ms_room * 0.5 + text_ascent } else { self.baseline_in_line(p, text_line, lead, text_line, text_ascent, in_table, ms_room, 0.0) };
        Some((pictures - above).max(0.0))
    }

    /// What a line of nothing but pictures (no character: the paragraph mark's room only) has below its pictures that Word does not give it: the picture sits on the
    /// baseline and the line ends there, with none of the room below the baseline of the paragraph mark's typeface that a line with text in it keeps (Word's PDFs
    /// of corpus 023: a 722.5 pt picture alone in its paragraph (mark 14 pt ＭＳ ゴシック, no grid) is a 722.5 pt line, the next line's baseline 10.6 pt below the
    /// picture's bottom, the text line's own 10.56; a 710.0 pt one with a 10.5 pt mark the same, 10.3 pt; ibisOffice's was 5.8 and 3.1 pt more). A line with text keeps it
    /// (`picture_growth`: 14 / 20 / 40 pt pictures in 10.5 pt text, picture + 3.0 pt). Single spacing, off a line grid (where `growth` is told); 0 otherwise.
    fn picture_only_cut(&self, p: &FlowParagraph, units: &[Unit], text_line: f32, text_ascent: f32, lead: f32, in_table: bool, ms_room: f32, growth: f32) -> f32 {
        if growth <= 0.0 || !matches!(p.line, LineHeight::Auto(240)) {
            return 0.0;
        }
        if units.iter().any(|u| matches!(u, Unit::Char { c, .. } if !c.is_whitespace())) {
            return 0.0;
        }
        let above = self.baseline_in_line(p, text_line, lead, text_line, text_ascent, in_table, ms_room, 0.0);
        (text_line - above).max(0.0)
    }

    /// A line's height with a picture (or a box) in it: a line spacing of several lines (`w:lineRule="auto"`
    /// above single) multiplies the text's line, not the picture (Word's PDF of a 3-line spacing around a
    /// 45 pt inline box: the box's height, not three times it).
    fn line_height_around(&self, p: &FlowParagraph, text_line: f32, picture: f32, natural: f32, lead: f32, in_table: bool, single: f32) -> f32 {
        // On a line grid a line that a picture makes is the picture's height rounded up to whole grids, and one that is exactly k grids is k grids (not k + 1 as a line of
        // text that high is): Word's PDFs of an inline picture of 18 to 300 pt on an 18 pt grid, at single spacing and at 1.079 lines (36 / 54 / 72 / 90 pt are 2 / 3 / 4 / 5
        // grids, 72.01 pt is 4, 72.5 pt is 5; a SmartArt's 248.04 pt, the line is 252 pt = 14 grids, ibisOffice's was 248.04 pt there and 270 pt at single spacing; GitHub issue #208).
        if picture > text_line
            && matches!(p.line, LineHeight::Auto(_))
            && self.on_grid(p, in_table)
            && let Some(pitch) = self.line_pitch.get().map(pt).filter(|g| *g > 0.0)
        {
            return (((picture - 0.01) / pitch).ceil() * pitch).max(self.resolve_line_height(p, text_line, 0.0, in_table));
        }
        if picture > text_line && matches!(p.line, LineHeight::Auto(m) if m > 240) {
            return self.resolve_line_height(p, text_line, 0.0, in_table).max(picture);
        }
        self.resolve_line_height(p, natural, lead, in_table) - self.multiple_shrink(p, natural, lead, in_table, single)
    }

    /// How far above the line's foot a picture sits on a line grid: the picture is centred in the grids its line takes (`line_height_around`), the room half above and half below
    /// (Word's PDFs of an inline picture of 30 to 300 pt on an 18 pt grid: the next line's top is 20.2 pt + half the room under the picture's foot, the picture's top the half
    /// of it below the line's top). `line` is the line's height; 0 where the line is not a grid's.
    fn picture_grid_slack(&self, p: &FlowParagraph, text_line: f32, picture: f32, in_table: bool, line: f32) -> f32 {
        if picture > text_line && matches!(p.line, LineHeight::Auto(_)) && self.on_grid(p, in_table) { ((line - picture) / 2.0).max(0.0) } else { 0.0 }
    }

    /// Whether the line spacing is a multiple of 1 or less of the line (`w:lineRule="auto"`, `w:line` under 240) off a line grid: the lines of it are Word's `H' = H - (1 - m) L`.
    fn multiple_below_one(&self, p: &FlowParagraph, in_table: bool) -> bool {
        matches!(p.line, LineHeight::Auto(m) if m < 240) && !self.on_grid(p, in_table)
    }

    /// What a multiple `m` under 1 takes off a line beyond `m` times it: Word's line is `H' = H - (1 - m) L` (`H` the single line, with the raise, lowering and pictures in it;
    /// `L` the line of its characters' typefaces and sizes with no raise, `single`, 0 where it is `H`), so `H' = m H + (1 - m) (H - L)`, and `line_height_parts` gives the `m H`.
    /// Windows Word, 2026-11-14 (XPS, `tasks/doc/requests/2026-11-14-179-rest-many-patterns-result.md`, 218 pages).
    fn multiple_shrink(&self, p: &FlowParagraph, natural: f32, lead: f32, in_table: bool, single: f32) -> f32 {
        let LineHeight::Auto(m) = p.line else { return 0.0 };
        if m >= 240 || single <= 0.0 || self.on_grid(p, in_table) {
            return 0.0;
        }
        -(1.0 - m as f32 / 240.0) * (natural + lead - single)
    }

    /// The line of the largest of the characters in `chars`, with no raise or lowering (`Unit::Char::plain_line`) and the room below it: Word's `L` of a multiple under 1.
    /// 0 where there is none.
    fn single_line_of<'u>(&self, p: &FlowParagraph, chars: impl Iterator<Item = &'u Unit>) -> f32 {
        chars
            .filter_map(|u| if let Unit::Char { c, plain_line, ascent, lead, style, size, .. } = u { if c.is_whitespace() { return None; } Some(self.mark_room(p, *c, *style, *size, *plain_line, *ascent).0 + *lead) } else { None })
            .fold(0.0f32, f32::max)
    }

    /// Whether the paragraph's lines are on the line grid (`line_height_parts`).
    fn on_grid(&self, p: &FlowParagraph, in_table: bool) -> bool {
        let in_table = in_table && !self.doc.adjust_line_height_in_table && self.doc.compat_mode.is_none_or(|m| m >= 15);
        !in_table && p.snap_to_grid && self.line_pitch.get().is_some_and(|pitch| pitch > 0) && !matches!(p.line, LineHeight::Exact(_) | LineHeight::AtLeast(0))
    }

    fn resolve_line_height(&self, p: &FlowParagraph, base: f32, lead: f32, in_table: bool) -> f32 {
        self.line_height_parts(p, base, lead, in_table).0
    }

    /// A line's height, and the room of it the line grid adds (`w:docGrid`
    /// `type="lines"`: the height rounded up to whole `linePitch`es).
    fn line_height_parts(&self, p: &FlowParagraph, base: f32, lead: f32, in_table: bool) -> (f32, f32) {
        // In a table's cell the lines are on the grid only with `adjustLineHeightInTable`.
        // Word 2003-era files (compatibility mode below 15) line the cells' lines up on the grid as well.
        let snapping = self.on_grid(p, in_table);
        // `lead`: the room below the text off a grid (see `leading`).
        let base = if snapping { base } else { base + lead };
        let h = match p.line {
            LineHeight::Exact(exact) => return (pt(exact), 0.0),
            LineHeight::AtLeast(min) => pt(min).max(base),
            // The lines a multiple adds are Word's line of the typeface (13.62 pt for ＭＳ 明朝 10.5 pt, `ms_room`: Word's PDFs of GitHub issue #192, the feed of a line
            // of 1.5 / 2 / 3 lines is 20.42 / 27.26 / 40.82 pt; the extra is below the text). Off a grid only; the typeface's own line otherwise.
            LineHeight::Auto(m) if m > 240 && !snapping => base * m as f32 / 240.0,
            LineHeight::Auto(m) => base * m as f32 / 240.0,
        };
        // A line whose height is the 1.3 times of an East Asian typeface (`lead` > 0) that is no more than 0.01 pt over k grids is k grids: 26 pt 游明朝 is
        // 43.504 pt (1.67324 em) and Word puts it in 3 grids of 14.5 pt = 43.5 pt on all multiples (Word's PDF of `line-multiple-grid-sweep-yumincho-g290.docx`);
        // the ＭＳ line that is exactly a grid takes the next (below).
        let tie_of = |p: &FlowParagraph, lead: f32| if matches!(p.line, LineHeight::AtLeast(_)) || lead > 0.0 { -0.01 } else { 0.001 };
        if snapping && let Some(pitch) = self.line_pitch.get() {
            let grid_pitch = pt(pitch);
            if grid_pitch > 0.0 {
                // How many grids the line takes counts the room Word leaves below the text
                // (游ゴシック・游明朝: 1.43 em, so a 28 pt line is 3 grids of 20 pt, not 2:
                // measured on Word's PDF of the title of the editing report), but where the
                // text sits in them does not (see `baseline_in_line`).
                let needed = match p.line {
                    LineHeight::Exact(_) => h,
                    LineHeight::AtLeast(min) => pt(min).max(base + lead),
                    LineHeight::Auto(m) => (base + lead).max((base + lead) * m as f32 / 240.0),
                };
                // A multiple (1.5 lines) is of the line the grid makes of the text, not rounded up to the grid again.
                if let LineHeight::Auto(m) = p.line
                    && m != 240
                {
                    let single = ((base + lead + tie_of(p, lead)) / grid_pitch).floor() * grid_pitch + grid_pitch;
                    // The larger of the grids the single line takes and the grid times the multiple (Word's PDF of
                    // `line-multiple-h-grid-probe.docx`, 18 pt grid: 10.5 pt at 1.15 / 1.5 / 2 / 3 lines 20.76 / 27 / 36 / 54 pt;
                    // 40 pt ＭＳ 明朝, 54 pt single, stays 54 pt at 1.5, 2 and 3 lines; 12 pt in a cell on a 14.05 pt grid at 3 lines 42.15 pt).
                    let total = single.max(grid_pitch * m as f32 / 240.0);
                    return (total, total - h);
                }
                // A line of text that is exactly a grid high takes the next grid too (Word's PDFs: a 14 pt ＭＳ 明朝 line, 9/7 of 14 = 18 pt, on an
                // 18 pt grid takes two); a line spacing's minimum equal to the grid takes one.
                let tie = tie_of(p, lead);
                let snapped = ((needed + tie) / grid_pitch).floor() * grid_pitch + grid_pitch;
                return (snapped, snapped - h);
            }
        }
        (h, 0.0)
    }

    /// The typeface Word draws `c` in when the typeface `name` of its font slot has no glyph for it (`ibis_doc_model::flow::fallback_typeface`), where that
    /// is known and installed; else `name` (the typeface's own, or what the system gives a character it lacks). Whether `name` has the glyph: the installed
    /// typeface's own cmap; not installed (a stand-in draws it), the table of its font file (`office_has_glyph`), and where there is none the character is taken
    /// as in it (no typeface is looked for). `c` stands apart from its neighbours (a list's mark).
    fn typeface_for_character<'a>(&self, style: &FlowRunStyle, name: Option<&'a str>, c: char, size: f32) -> Option<&'a str> {
        self.typeface_for_character_in_line(style, name, c, size, &mut FallbackCarry::default(), false).0
    }

    /// `typeface_for_character` for a character of a line of text: `carry` is the state of the characters before it (Word draws a character that its typeface
    /// lacks in the fallback typeface of the character before, where that has it: `FallbackCarry`), `line_start` whether it begins a wrapped line. The second
    /// value is whether the typeface depends on the carrying over (so that a line beginning with the character, where Word cuts it, is drawn in another).
    fn typeface_for_character_in_line<'a>(&self, style: &FlowRunStyle, name: Option<&'a str>, c: char, size: f32, carry: &mut FallbackCarry, line_start: bool) -> (Option<&'a str>, bool) {
        // A run with no typeface at all (no `w:rFonts` anywhere, not even in the styles) is drawn by Word in its built-in default, 游明朝 on this Word (Windows, Japanese): Word's PDF of
        // 2026-11-26 (`tasks/doc/reports/2026-11-26-k0066-fe0e-typefaces.md`) has "A", é, Ω, ★, →, 亜 and ⚽ of a document with no fonts all in YuMincho, so for
        // any slot (not Times New Roman for the Latin ones). Its glyphs decide whether a fallback typeface is looked for (❤ is in none of its blocks: Segoe UI
        // Symbol, not the colour emoji typeface the system's fallback gives a typeface-less run). The run's own `name` stays `None` when no fallback is chosen.
        let have = name.unwrap_or(DEFAULT_TYPEFACE);
        let slot = ibis_doc_model::flow::font_slot(c, style.east_asia_hint);
        // Whether `have` has the glyph matters only where a fallback typeface could be chosen (the character is in the table, or one is waiting to be carried).
        let looked_at = slot == FontSlot::HAnsi && (ibis_doc_model::flow::fallback_typeface(c).is_some() || carry.is_carrying());
        // Whether `have` has the glyph is the question Word asks of its own (Windows') typeface, so the table of the typeface's file (`office_has_glyph`, the cmap of the
        // Windows font) answers it where it has the typeface; an installed typeface of another version (a Mac's Century has a glyph of ‒ the Windows one lacks) is asked only
        // where the table has none, and where there is none either the character is taken as in it (no typeface is looked for).
        let by_table = if looked_at && !INSTALLED_FIRST.load(std::sync::atomic::Ordering::Relaxed) { crate::render::font_metrics::office_has_glyph(have, c) } else { None };
        let has_glyph = if !looked_at {
            true
        } else if let Some(has) = by_table {
            has
        } else if self.res.has_typeface(have) {
            let probe = self.font(style, name);
            self.res.text_system().advance(self.res.text_system().resolve_font(&probe), px(size), c).is_ok()
        } else {
            crate::render::font_metrics::office_has_glyph(have, c).unwrap_or(true)
        };
        // (`CARRY_OFF`, a switch of the tools that count what the carrying over changes: every character is then a line's first.)
        let line_start = line_start || CARRY_OFF.load(std::sync::atomic::Ordering::Relaxed);
        let input = CarryInput { c, slot, typeface: have, bold: style.bold, italic: style.italic, line_start, has_glyph };
        let choice = carry.next(input, crate::render::font_metrics::office_has_glyph);
        match choice.fallback {
            Some(fallback) if !has_glyph && !have.eq_ignore_ascii_case(fallback) && self.res.has_typeface(fallback) => (Some(fallback), choice.carried),
            _ => (name, false),
        }
    }

    /// How many times a run's character spacing (`w:spacing`) counts for the character `c`. With `balanceSingleByteDoubleByteWidth` the spacing of a
    /// double-byte character and of a half-width space counts twice, whatever the document grid and whether the paragraph is on it; digits and Latin
    /// letters count once; without the setting every character counts once (Word's PDFs, Microsoft 365 16.0.20527: `spacing-charspace-size-probe.docx`
    /// (no setting), `spacing-charspace-balance-probe.docx`, `spacing-charspace-compress-probe.docx`, `spacing-charspace-balance-compress-probe.docx`:
    /// +5 and +10 pt on ＭＳ ゴシック digits, Century and ＭＳ ゴシック Latin letters, spaces, かな and 漢字, in nine settings of `w:charSpace`
    /// (none, 321 ... 6651, -1000, -4096): the digits and letters move 5.0 / 10.0 pt each, the spaces and the かな and 漢字 10.0 / 20.0 pt with the
    /// setting; `spacing-small-values-*-probe.docx`: -100 to +100 twips, the same ratios (a space 2.0 x); 019: 労働時間 at +7.9 pt is 15.8 pt apart, a digit
    /// at +5.25 pt 5.25; earlier `spacing-balance-grid-probe.docx`, `spacing-grid-flags-probe.docx`, `gridspacing-halfwidth-probe.docx`).
    fn spacing_factor(&self, _p: &FlowParagraph, c: char) -> f32 {
        if self.doc.balance_single_double_byte_width && (is_cjk(c) || c == ' ') { 2.0 } else { 1.0 }
    }

    /// The size a character takes its line's room at: the size it is drawn in, but for a superscript or subscript the full size of the run (the line takes the room of the run's
    /// own size, `line_metrics` at `metric_size`).
    fn room_size(&self, p: &FlowParagraph, style: usize, c: char, size: f32) -> f32 {
        match p.runs.get(style) {
            Some(r) if style != LABEL && r.style.position != VerticalPosition::Baseline => r.style.size_for(c) as f32 / 100.0,
            _ => size,
        }
    }

    /// The room `ms_room` put into the line of the largest of `chars` (ＭＳ 明朝 / ＭＳ ゴシック), or 0.
    fn ms_room_of<'u>(&self, p: &FlowParagraph, chars: impl Iterator<Item = &'u Unit>) -> f32 {
        chars
            .filter_map(|u| {
                let Unit::Char { c, line, style, size, .. } = u else { return None };
                // A list item's number or bullet is in the line too (with the typeface `units` gave it).
                let name = if *style == LABEL { label_font_name(&p.list_style, *c) } else { p.runs.get(*style).and_then(|r| r.style.font_for(*c)) };
                Some((*line, self.ms_room(name, self.room_size(p, *style, *c, *size))))
            })
            .fold((0.0f32, 0.0f32), |best, x| if x.0 > best.0 { x } else { best })
            .1
    }

    /// The text's ascent of a line that mixes ＭＳ 明朝 / ＭＳ ゴシック (whose line has `ms_room` more, half of it above the baseline) with a Latin typeface of a larger ascent than their 1 em
    /// (Arial's 1854/2048 + 67/2048 = 0.9375 em against ＭＳ's 220/256 = 0.859 em): each character has its own ascent, ＭＳ's with half its room, and the line's is the largest of them, not
    /// the largest bare ascent plus half the room. Returned as the bare ascent that `ms_room_of`'s half added gives that: `max(a_i + r_i / 2) - R / 2`. Windows Word, 2026-11-14 (XPS of
    /// `blank-paras-after-wrapsquare.docx`: the line of ＭＳ ゴシック 10.5 pt with an Arial "A" has its baseline 10.6 pt down, 0.859 x 10.5 + 1.56, not 9.85 + 1.56).
    fn ms_mixed_ascent<'u>(&self, p: &FlowParagraph, chars: impl Iterator<Item = &'u Unit> + Clone, text_ascent: f32) -> f32 {
        if self.vertical.get() || p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::Ruby { .. })) {
            return text_ascent;
        }
        let (mut best, mut any) = (0.0f32, false);
        for u in chars.clone() {
            let Unit::Char { c, ascent, style, size, .. } = u else { continue };
            if c.is_whitespace() {
                continue;
            }
            let name = if *style == LABEL { label_font_name(&p.list_style, *c) } else { p.runs.get(*style).and_then(|r| r.style.font_for(*c)) };
            let room = self.ms_room(name, self.room_size(p, *style, *c, *size));
            any |= room > 0.0;
            best = best.max(*ascent + room * 0.5);
        }
        if !any {
            return text_ascent;
        }
        (best - self.ms_room_of(p, chars) * 0.5).max(0.0)
    }

    /// The room a line's 傍点 (`w:em`) take, and where it moves the baseline: `(grow, baseline_dy)`. Windows Word, 2026-11-14 (XPS, `tasks/doc/requests/2026-11-14-54-rest-many-patterns-result.md`,
    /// 746 patterns of 6 typefaces from 8 to 72 pt): the room `e` is a quarter of the line of the largest character of the line (`H`: the typeface's own line, the one a single
    /// line of it has; for a multiple under 1 the quarter of the multiple of it), whatever the dots (dot, comma, circle over the line, underDot under it) and the characters.
    /// The line spacing's room `s = height - H` takes it: `s >= e` leaves the line as high as it was, else the line is `height + e`. An exact line does not move at all.
    /// The baseline: the dots over the line lower it by `e` (the room is above), unless the room is taken: a multiple's room is below the baseline (still `e`), an `atLeast`
    /// line's is above it (0; and underDot lifts it `e`). On a line grid the taken room is placed by `H + 2e` centred in the grid line (below).
    fn emphasis_adjust<'u>(&self, p: &FlowParagraph, units: impl Iterator<Item = &'u Unit>, height: f32, grid: f32) -> (f32, f32) {
        use ibis_doc_model::EmphasisMark;
        if matches!(p.line, LineHeight::Exact(_)) {
            return (0.0, 0.0);
        }
        let (mut h, mut above, mut below) = (0.0f32, false, false);
        for u in units {
            let Unit::Char { c, line, lead, style, .. } = u else { continue };
            if c.is_whitespace() {
                continue;
            }
            h = h.max(line + lead);
            // (No marks over a ruby's base: see `is_ruby_base`.)
            match p.runs.get(*style).filter(|_| !is_ruby_base(p, *style)).map(|r| r.style.emphasis) {
                Some(EmphasisMark::Dot | EmphasisMark::Comma | EmphasisMark::Circle) => above = true,
                Some(EmphasisMark::UnderDot) => below = true,
                _ => {}
            }
        }
        if !above && !below || h <= 0.0 {
            return (0.0, 0.0);
        }
        let multiple = match p.line {
            LineHeight::Auto(m) if m < 240 => m as f32 / 240.0,
            _ => 1.0,
        };
        let e = 0.25 * h * multiple;
        let need = e * (above as u32 + below as u32) as f32;
        let absorbed = height - h >= need - 0.001;
        let grow = if absorbed { 0.0 } else { need };
        let at_least = matches!(p.line, LineHeight::AtLeast(_)) && grid <= 0.0;
        if absorbed && grid > 0.0 {
            // On a line grid Word reserves `n e` above and `n e` below the line (`H + 2 n e`, `n` the number of sides that have marks: dot, comma, circle over, underDot under) and centres that in the grid line `height`, but never above the line's top: the line's
            // top is `p = max(0, (height - H - 2 n e) / 2)` below the grid line's, where a line without marks has `(height - H) / 2`. The baseline of a dot over the line is `e + p`
            // below its top, an underDot's `p` (Word, 2026-11-14: 8 to 20 pt, grids 18 and 36 pt, 20 patterns, within 0.12 pt: 10.5 pt dot +1.20 / underDot -2.28,
            // 14 pt 0 / -4.44; `dot - underDot = e`).
            let half = (height - h) / 2.0;
            let top = ((height - h - 2.0 * need) / 2.0).max(0.0);
            return (grow, if above { e + top - half } else { top - half });
        }
        let mut dy = 0.0;
        if above {
            dy += if absorbed && at_least { 0.0 } else { e };
        }
        if below && absorbed && at_least {
            dy -= e;
        }
        (grow, dy)
    }

    /// The room under the text of a line that need not fit the page: the line grid's half of what it adds.
    fn grid_room_below(&self, p: &FlowParagraph, natural: f32, lead: f32, in_table: bool, _height: f32, _baseline: f32, _text_line: f32, _text_ascent: f32) -> f32 {
        let (height, grid) = self.line_height_parts(p, natural, lead, in_table);
        // What a multiple (1.5, 2 lines) adds is below the text and need not fit the page either, in the last line of the page's text: Word's page of corpus 056 (ＭＳ 明朝
        // 11 pt, 1.5 lines, a last empty paragraph) ends with the line's extra below the bottom margin (Windows Word, 2026-11-14, many-patterns-2-result.md).
        let multiple = if grid == 0.0 && matches!(p.line, LineHeight::Auto(m) if m > 240) { (height - (natural + lead)).max(0.0) } else { 0.0 };
        grid / 2.0 + multiple
    }

    /// Where the baseline is in a line `height` high whose text is
    /// `text_line` high with `text_ascent`: 58 % of the room the line
    /// spacing gives beyond the text above it, and 0.87 em below that
    /// (measured on Word's PDF: 8 pt text in an exact 8.5 pt line, 12 pt in
    /// 18 pt); the room the line grid adds is split evenly above and below
    /// (Word's PDF of DBDY.p01: a label of 9 pt Arial and 游ゴシック, 11.58 pt
    /// high, in an 18 pt grid line has its baseline 12.0 pt down).
    fn baseline_in_line(&self, p: &FlowParagraph, natural: f32, lead: f32, text_line: f32, text_ascent: f32, in_table: bool, ms_room: f32, single: f32) -> f32 {
        let (height, grid) = self.line_height_parts(p, natural, lead, in_table);
        // An exact line puts its baseline 0.8 of the line down, whatever the typeface and size (Word's PDF of exact-baseline-sweep.docx: Century and
        // ＭＳ 明朝, 8 / 10.5 / 12 pt text in exact 10 / 12 / 14 / 18 / 24 pt lines: 0.797 to 0.813 of the height; 游ゴシック 10 pt in exact 15 pt: 11.98).
        if matches!(p.line, LineHeight::Exact(_)) {
            return height * 0.8;
        }
        // A multiple of lines on a line grid: the room the grid line has beyond the text, the extra of the multiple included, is split evenly above
        // and below the text (Word's PDFs of cellgrid-multiple-10pt-grid290.docx: from a line of 1.15 lines to the next of 1.5 the baselines are
        // 19.20 pt apart = 16.68 + (21.75 - 16.68) / 2, and so for the other multiples; the first baseline of a line is 10.83 pt down in a
        // single 14.5 pt line and 11.91 pt in a 1.15 one, half of the 2.18 pt more).
        // The text's box in the grid line is its 1 em (ascent and descent), not the line `ms_room` makes of it: a single 14.5 pt line of 10 pt ＭＳ ゴシック has its
        // baseline 10.83 to 10.94 pt down (Word's PDFs of cellgrid-empty-multiple and cellrow-valign-exact: (14.5 - 10) / 2 + 0.87 x 10 = 10.95).
        if grid > 0.0 && matches!(p.line, LineHeight::Auto(m) if m != 240 || ms_room > 0.0) {
            return (height - (text_line - ms_room)).max(0.0) * 0.5 + text_ascent;
        }
        // A multiple of 1 or less off a grid: the part below the text is `m` times the single line's (`d' = m d`), the line `H' = H - (1 - m) L`, and the text's baseline
        // `a' = H' - d'`, which for a line of text only is `m A` (Windows Word, 2026-11-14, 218 pages of XPS, `…-179-rest-many-patterns-result.md`: MS 明朝 8 to 72 pt, five
        // other typefaces, mixed sizes, raised and lowered text (`A + r` and `D - r` of the single line, neither under 0), 0.13 pt). `text_dy` stays what it was.
        if let LineHeight::Auto(m) = p.line && m < 240 && grid <= 0.0 {
            let dy = self.text_dy(p, natural, lead, in_table);
            let single_baseline = ms_room * 0.5 + text_ascent + dy;
            let d = (natural + lead - single_baseline).max(0.0);
            let h = height - self.multiple_shrink(p, natural, lead, in_table, single);
            return h - m as f32 / 240.0 * d - dy;
        }
        // Off a grid a single line of ＭＳ 明朝 / ＭＳ ゴシック puts the room `ms_room` adds to the text's 1 em half above and half below it too (Word's PDF of 019: a 10.29 pt line of 8 pt
        // text has its baseline 8.0 pt down, 0.87 em + 0.143 em, so that the next, an exact 12 pt line, is 11.9 pt under it, not the 12.9 pt of a baseline at 0.87 em).
        // A multiple has it where the single line has (Word's PDFs of GitHub issue #192: the baseline of the line of 1.5, 2 and 3 lines is 10.56 to 10.68 pt down, as the single line's).
        if grid <= 0.0 && ms_room > 0.0 && matches!(p.line, LineHeight::Auto(m) if m >= 240) {
            return ms_room * 0.5 + text_ascent;
        }
        // An exact line spacing leaves no room for the leading (DSEC-00011:
        // 10 pt 游ゴシック in an exact 15 pt line).
        let text_line = if grid > 0.0 || matches!(p.line, LineHeight::Exact(_)) { text_line } else { text_line + lead };
        // A multiple (`w:lineRule="auto"`) puts all of its extra room below the text
        // (DRUX-00730: 8 pt text in a double line sits at the top of it).
        let above = if matches!(p.line, LineHeight::Auto(_)) { 0.0 } else { 0.58 };
        // A line spacing's minimum (`atLeast`) puts all the room it adds above the text, the text's part below the baseline staying as it is in a single line
        // (Word's PDF of `n2-line-height.docx` of GitHub issue #179: 10.5 pt ＭＳ 明朝 in an `atLeast` 36 pt line has its baseline 33.0 pt down = 36 - 3.0, where a
        // single line's 13.5 pt has it 10.6 pt down; the half of `ms_room` that a single line has below the baseline counts there).
        if matches!(p.line, LineHeight::AtLeast(_)) && grid <= 0.0 {
            return (height - text_line).max(0.0) + ms_room * 0.5 + text_ascent;
        }
        // An exact line of 游ゴシック puts all the room above the text, the descent flush with the line's bottom
        // (DCHP: 10 pt text in an exact 15 pt line has its baseline 11.98 pt down = 15 − 0.3022 em; 0.58 of the room gives 11.08).
        let above = if lead > 0.0 && grid <= 0.0 && matches!(p.line, LineHeight::Exact(_)) { 1.0 } else { above };
        (height - grid - text_line).max(0.0) * above + grid * 0.5 + text_ascent
    }

    /// How far below `baseline_in_line` the text is drawn (inline pictures and shapes stay on it): off a grid, the
    /// room 游ゴシック's lines have beyond its ascent and descent is split above and below the text (DSEC-00001,
    /// DHDR-00001: the baseline of 10 pt and 14 pt text is 0.5 × 0.143 em below its ascent, within 0.07 pt of Word's PDF).
    fn text_dy(&self, p: &FlowParagraph, natural: f32, lead: f32, in_table: bool) -> f32 {
        let (_, grid) = self.line_height_parts(p, natural, lead, in_table);
        // A line with ruby keeps the text on `baseline_in_line` (DW14: Word's ruby and its base text stay where they were).
        let ruby = p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::Ruby { .. }));
        if grid <= 0.0 && matches!(p.line, LineHeight::Auto(_)) && !ruby { lead * 0.5 } else { 0.0 }
    }

    fn font_for_size(&self, style: &FlowRunStyle, name: Option<&str>, size: u32) -> Font {
        self.res.font(&TextStyle {
            font_size: size,
            bold: style.bold,
            italic: style.italic,
            font: name.map(str::to_string),
            ..TextStyle::default()
        })
    }

    /// Where Word puts the single line of an underline for a run of `size` points in typeface `name` (`UnderlineBox::band`): from the files of the typeface (GitHub issue #192,
    /// Word's XPS). An East Asian typeface (`OS/2` ulCodePageRange1 bit 17 to 20) is `size / 20` thick, and its top is `0.7 usWinDescent × size + 0.5 pt` below the baseline.
    /// **A measured value the user allowed as an exception (implimentationRule.md, 2026-11-27, board d-192-underline-const; keep it in this one place)**: Word's PDFs of ＭＳ 明朝・ＭＳ ゴシック・ＭＳ Ｐ明朝・游ゴシック・游明朝・メイリオ・BIZ UDゴシック・BIZ UD明朝 Medium at 10.5, 14, 24 and 48 pt (32 points) are within 0.1 pt; the origin of 0.7 and 0.5 pt is not known. The others have `post`'s underlinePosition and underlineThickness, the line cut back to usWinDescent. A vertical column keeps the older lines.
    fn underline_band_of(&self, name: Option<&str>, bold: bool, size: f32) -> Option<(f32, f32, bool)> {
        if self.vertical.get() {
            return None;
        }
        let m = self.res.decoration_metrics(name?.trim(), bold)?;
        if m.east_asian {
            return Some((0.7 * m.win_descent * size + 0.5, size / 20.0, true));
        }
        if m.underline_thickness <= 0.0 {
            return None;
        }
        let t = m.underline_thickness * size;
        let top = (-m.underline_position * size).min(m.win_descent * size - t);
        Some((top, t, false))
    }

    /// The strike lines (top, thickness) of a run of `size` points in typeface `name` whose baseline is at `y_base`: `OS/2`'s yStrikeoutPosition (the top of the stroke above the
    /// baseline) and yStrikeoutSize as they are, a double strike two lines of that size, one above and one below the single one's by its thickness (Word's XPS of GitHub issue
    /// #192). The older fractions of the size for a typeface whose file is not known.
    fn strike_lines_of(&self, name: Option<&str>, bold: bool, size: f32, double: bool, y_base: f32) -> Vec<(f32, f32)> {
        if let Some(m) = name.and_then(|n| self.res.decoration_metrics(n.trim(), bold)).filter(|m| m.strike_size > 0.0) {
            let (top, t) = (y_base - m.strike_position * size, m.strike_size * size);
            return if double { vec![(top - t, t), (top + t, t)] } else { vec![(top, t)] };
        }
        if double {
            let sh = (size / 13.0).max(0.4);
            vec![(y_base - size * 0.3 - size * 0.08, sh), (y_base - size * 0.3 + size * 0.08, sh)]
        } else {
            vec![(y_base - size * 0.3, (size / 18.0).max(0.4))]
        }
    }

    fn font(&self, style: &FlowRunStyle, name: Option<&str>) -> Font {
        self.font_for_size(style, name, style.size)
    }

    /// The top and bottom of a run's border box on the page (`y_base`: the baseline the run's text is drawn on, `raise`: how far that is below the line's baseline, negative
    /// above it, `w:position` and the script both; `run_ascent`, `run_line`, `s_size`: the text's own). The box is the run's own box (`border_extent`) on its baseline; a run
    /// raised or lowered (`w:position`) also holds the band of the line's baseline, the border's room (`W`) above and below it (Word's XPS of GitHub issue #192, 0.5 pt line,
    /// W 0.48). A superscript or subscript (`w:vertAlign`) shrinks the text but not the box, and does not lift it: the box is that of the run's full size on the line's baseline,
    /// as high and as low for both (-11.16 and +3.50 pt for 10.5 pt ＭＳ 明朝, -24.74 and +7.44 for 24 pt, space 2 and sz 12 the same).
    #[allow(clippy::too_many_arguments)]
    fn border_box_y(&self, s: &FlowRunStyle, font_name: Option<&str>, y_base: f32, raise: f32, run_ascent: f32, run_line: f32, s_size: f32, b: &FlowBorder) -> (f32, f32) {
        let vertical = self.vertical.get();
        let script = s.position != VerticalPosition::Baseline && !vertical;
        let line_base = y_base - raise;
        let (own_base, ascent, line, size) = if script {
            let full = s.size as f32 / 100.0;
            let (a, l) = self.line_metrics(font_name, &self.font(s, font_name), full);
            (line_base - s.raise as f32 / 2.0, a, l, full)
        } else {
            (y_base, run_ascent, run_line, s_size)
        };
        let (above, below) = self.border_extent(font_name, ascent, line, size, b);
        let y_top = if vertical { (own_base - above).max(0.0) } else { own_base - above };
        let y_bottom = own_base + below;
        if s.raise != 0 && !vertical {
            let band = if script { line_base } else { y_base + s.raise as f32 / 2.0 };
            let w = pt(b.total()) + pt(b.space);
            return (y_top.min(band - w), y_bottom.max(band + w));
        }
        (y_top, y_bottom)
    }

    /// How far a run's border (`w:bdr`) goes above and below the baseline, in points (a line of text; a vertical column keeps the older fixed padding).
    ///
    /// Across a line the box is the line box of the run's typeface, the ascent and descent with the room Word leaves beyond them split evenly above and below
    /// (`line_metrics`, `ms_room`, `lead_em`), grown by the border's whole width and its distance from the text on each side (Word's PDFs of GitHub issue #192: ＭＳ 明朝 10.5 pt,
    /// a 0.5 pt line, the box from 11.16 pt above the baseline to 3.48 below, a double line 0.96 pt further out on each side, 游ゴシック 12.96 above and 5.76 below).
    fn border_extent(&self, name: Option<&str>, ascent: f32, line: f32, size: f32, b: &FlowBorder) -> (f32, f32) {
        let pad_v = pt(b.space);
        if self.vertical.get() {
            let extra_bw = (pt(b.total()) - pt(b.width)).max(0.0);
            return (ascent + 1.2 + pad_v, (line - ascent) + 2.8 + pad_v + 2.0 * extra_bw);
        }
        let pad = pt(b.total()) + pad_v;
        let ms_room = self.ms_room(name, size);
        let room = ms_room + self.lead_em(name) * size;
        (ascent + room / 2.0 + pad, (line - ms_room - ascent) + room / 2.0 + pad)
    }

    /// The Windows ascent and descent (ems) Word works from for typeface `name` (`family`: the font drawn's; `drawn`: its own when Office gives the typeface none).
    fn win_ascent_descent(name: Option<&str>, family: &str, drawn: impl FnOnce() -> (f32, f32)) -> (f32, f32) {
        match name.map(str::trim) {
            // ＭＳ 明朝's and ＭＳ ゴシック's Windows ascent and descent, 220 and 36 of the 256 units of the em (`usWinAscent`, `usWinDescent`). Word's first
            // baseline is 220/256 em + half the room of its 332/256 em line (`ms_room_em`) from the top, 1.0078 em (2026-11-14, Windows Word, 8 to 72 pt,
            // many-patterns-2-result.md).
            Some(n) if fixed_width(n) || n.starts_with("ＭＳ") || n.starts_with("MS ") => (220.0 / 256.0, 36.0 / 256.0), // i18n-ok（フォント名の判定（ＭＳ 系）。訳さない）

            Some(n) => crate::render::font_metrics::office_metrics(n).or_else(|| crate::render::font_metrics::office_metrics(family)).unwrap_or_else(drawn),
            None => crate::render::font_metrics::office_metrics(family).unwrap_or_else(drawn),
        }
    }

    /// The ascent and the line height (points) of typeface `name` at `size`:
    /// Word's, from the font's Windows ascent and descent (`font_metrics`):
    /// the font drawn's when it is the typeface, else those Office gives the
    /// typeface (another font stands in for it), else the font drawn's.
    fn line_metrics(&self, name: Option<&str>, font: &Font, size: f32) -> (f32, f32) {
        let drawn = || {
            let (a, d, _) = self.res.line_metrics(font);
            (a, d)
        };
        let (a, d) = Self::win_ascent_descent(name, &font.family, drawn);
        // Word's line of a Latin typeface is the Windows ascent and descent plus GDI's external leading (`TEXTMETRIC.tmExternalLeading`, `external_leading_em`: 67/2048 em of Arial,
        // 87/2048 of Times New Roman, the 2355/2048 em that Windows Word's XPS shows for Arial: a 10.5 pt empty line is 12.07 pt, its first baseline 9.85 pt down = the gap
        // above the ascent; `blank-paras-after-wrapsquare.docx`, `tasks/doc/requests/2026-11-14-54-rest-many-patterns-result.md`).
        let gap = self.external_leading_em(name);
        // ＭＳ 明朝 / ＭＳ ゴシック: Word's line is 332/256 em, against the 1 em of their ascent and descent, in a table's cell as well (see `ms_room_em`).
        ((a + gap) * size, (a + d + gap) * size + self.ms_room(name, size))
    }

    /// GDI's external leading of typeface `name` in ems (`font_metrics::office_external_leading`, from the files' OS/2 and hhea). 0 where it is not known, and for the typefaces whose
    /// line is Word's 1.3 times the Windows height (the East Asian ones, `lead_em`, and ＭＳ, `ms_room_em`): 游明朝's file would say 645/2048 and Word's line is 1.3 x 2636/2048 without it.
    fn external_leading_em(&self, name: Option<&str>) -> f32 {
        let Some(n) = name.map(str::trim) else { return 0.0 };
        if fixed_width(n) || proportional_ms(n) || self.lead_em(Some(n)) > 0.0 {
            return 0.0;
        }
        crate::render::font_metrics::office_external_leading(n).unwrap_or(0.0)
    }

    /// Whether a superscript or subscript of typeface `n` is Word's Japanese kind: 0.6 of the body, rising by the body less its size. By the typeface's file where it is here (a Japanese code
    /// page whose `OS/2` script offsets are 0 or the ascent, `WinRowMetrics::japanese_without_script_offsets`: 47 of 47 in Word's 294 typefaces); by its name where it is not (ＭＳ and the
    /// `lead_em` typefaces: 游, BIZ UD ...; not メイリオ, whose offsets are real).
    /// The typeface's Windows metrics: its file's where it is here, else the table of the typefaces Office ships (`font_metrics::office_font_face`, written by
    /// `scripts/gen-office-font-metrics.py` from the files on a Windows PC) for what GDI gives it at a size.
    fn row_metrics(&self, n: &str) -> Option<std::rc::Rc<crate::render::font_metrics::WinRowMetrics>> {
        self.res.win_row_metrics(n).or_else(|| crate::render::font_metrics::office_font_face(n, false, false).map(std::rc::Rc::new))
    }

    fn japanese_script(&self, n: &str) -> bool {
        match self.row_metrics(n) {
            Some(m) => m.japanese_without_script_offsets(),
            None => fixed_width(n) || proportional_ms(n) || (self.lead_em(Some(n)) > 0.0 && n != "メイリオ" && n != "Meiryo"), // i18n-ok（フォント名の判定。訳さない）
        }
    }

    /// The size of a superscript or subscript (`w:vertAlign`) of `size` points in typeface `name`: Word's, in 0.5 pt units (Windows Word's XPS, `tasks/doc/requests/2026-11-14-180-192-rest-many-patterns-result.md`:
    /// 15 sizes from 6 to 72 pt of 5 typefaces, 294 typefaces at 20 and 24 pt). The typeface's `OS/2` ySuperscriptYSize (ySubscriptYSize for a subscript) over its em times the size, rounded
    /// to half a point: 218 of the 294 typefaces. The typefaces with a Japanese code page (ＭＳ and the `lead_em` ones: 游, BIZ UD ...) are 0.6 whatever their `OS/2` says (0.5 in ＭＳ 明朝, 0.76 to 0.81 in
    /// 游ゴシック); so are 76 of the 294 in all (22 of them Latin: Arial Black, Garamond ... not explained). Where the typeface's file is not here, 0.65 (Arial, Times New Roman, Verdana, Calibri,
    /// Segoe UI, Century and メイリオ are 0.65 in the result).
    fn script_size(&self, name: Option<&str>, size: f32, position: VerticalPosition) -> f32 {
        let factor = (|| {
            let n = name?.trim();
            if self.japanese_script(n) {
                return Some(0.6);
            }
            let m = self.row_metrics(n)?;
            let y = if position == VerticalPosition::Subscript { m.y_subscript_size } else { m.y_superscript_size };
            (y > 0.0).then_some((y / m.units_per_em) as f32)
        })()
        .unwrap_or(0.65);
        (size * factor * 2.0).round() / 2.0
    }

    /// How far a superscript (up) or subscript (down) of `body` points is moved off the line's baseline, in points, positive; `script` is its size (`script_size`). Windows Word's
    /// (`tasks/doc/requests/2026-11-16-192-rise-result.md`, 8 typefaces x 87 sizes, all 696 rises and 696 falls; `2026-11-18-192-gdi-metrics-*.tsv`): everything is in pixels of a 600 dpi
    /// device (`ppem = round(half points x 25 / 6)`) and each pixel value goes to half points (`round(px x 0.24)`) before the two are subtracted.
    /// A superscript rises by the body's GDI ascent and external leading less the script's ascent (`WinRowMetrics::gdi_ascent_descent`: the font's `VDMX` rows where it has them,
    /// else `usWinAscent` scaled), a subscript falls by the body's GDI descent less the script's, and neither more than the typeface's `OS/2` ySuperscriptYOffset (ySubscriptYOffset)
    /// of the body, rounded to half points. A typeface with a Japanese code page (size 0.6, `japanese_script`) rises by the body less the script, and falls by the descent plus
    /// the room Word's line gives it below the text (the East Asian line is 1.3 times `usWinAscent + usWinDescent`: 0.15 of it below, as `cjk_extra_units`), the body's less the
    /// script's, with no cap (the 47 Japanese typefaces of the 294 at 20 and 24 pt, and ＭＳ 明朝's 87 sizes). Where the typeface's file is not here (and for the Japanese
    /// subscript with it) the old 0.35 and 0.1 of the body stay.
    fn script_shift(&self, name: Option<&str>, body: f32, script: f32, position: VerticalPosition) -> f32 {
        let sup = position == VerticalPosition::Superscript;
        let old = if sup { body * 0.35 } else { body * 0.1 };
        let Some(n) = name.map(str::trim) else { return old };
        let japanese = self.japanese_script(n);
        if japanese && sup {
            return (body - script).max(0.0);
        }
        let Some(m) = self.row_metrics(n) else { return old };
        let (body_hp, script_hp) = ((body * 2.0).round() as i64, (script * 2.0).round() as i64);
        let ppem = |hp: i64| ((hp as f64) * 25.0 / 6.0 + 0.5).floor() as u32;
        // A pixel of 600 dpi is 0.12 pt = 0.24 half points.
        let half_points = |px: i32| (f64::from(px) * 0.24 + 0.5).floor() as i64;
        let (body_px, script_px) = (ppem(body_hp), ppem(script_hp));
        let (body_ascent, body_descent) = m.gdi_ascent_descent(body_px);
        let (script_ascent, script_descent) = m.gdi_ascent_descent(script_px);
        let shift = if japanese {
            // 0.15 is one side of Word's East Asian line (1.3 x (usWinAscent + usWinDescent): `cjk_extra_units` = 2 x 0.15 S), here below the text, at the pixel.
            let room = |p: u32| ((0.15 * (m.win_ascent + m.win_descent) * f64::from(p) / m.units_per_em) + 0.5).floor() as i32;
            half_points(body_descent + room(body_px)) - half_points(script_descent + room(script_px))
        } else {
            let (by_gdi, offset) = if sup {
                (half_points(body_ascent + m.gdi_external_leading(body_px)) - half_points(script_ascent), m.y_superscript_offset)
            } else {
                (half_points(body_descent) - half_points(script_descent), m.y_subscript_offset)
            };
            by_gdi.min((offset / m.units_per_em * body_hp as f64 + 0.5).floor() as i64)
        };
        shift.max(0) as f32 / 2.0
    }

    /// The room Word leaves in a line of text (an empty paragraph has it too) of ＭＳ 明朝, ＭＳ ゴシック, ＭＳ Ｐ明朝 and ＭＳ Ｐゴシック beyond their 1 em of ascent
    /// and descent, in a table's cell as in the body (see `ms_room_em`).
    fn ms_room(&self, name: Option<&str>, size: f32) -> f32 {
        let ms = name.map(str::trim).is_some_and(|n| fixed_width(n) || proportional_ms(n));
        if ms { ms_room_em() * size } else { 0.0 }
    }

    /// The room below the text a line of paragraph `p` in typeface `name` counts, in ems: `lead_em`, on the line grid as off it (`line_height_parts`). Word's lines of an East
    /// Asian typeface are 1.3 times its Windows ascent plus descent on the grid too, and the number of grids a line takes counts all of it (Word's PDFs
    /// of `ratio-mincho.docx` and `ratio-gothic.docx`, GitHub issue #54, Microsoft 365 16.0.20527: 游明朝 and 游ゴシック at 20 / 40 / 60 pt on grids of 16.3 to
    /// 17 pt step from k to k - 1 grids at 16.7 / 16.7 / 16.7 pt = 1.67 to 1.675 em of the line (1.2871 x 1.3 = 1.6732), at 12 pt and 36 pt on grids of 20.0
    /// to 20.1 pt at 20.05 pt and 60.15 pt; メイリオ (1.5 x 1.3 = 1.95 em) at 19.5 pt, BIZ UDP ゴシック (1.3 em) at 17.3 pt). A 24 pt 游明朝 line is 40.2 pt,
    /// 3 grids of 18 pt, not the 2 that 1.2871 em or 1.43 em give.
    fn lead_in(&self, p: &FlowParagraph, name: Option<&str>) -> f32 {
        self.lead_em(name) + self.vertical_pitch_em(p, name)
    }

    /// What a column of vertical text without a line grid has more than a horizontal line of the same typeface, in ems of its size. An East Asian typeface's column is the line's
    /// rule (`cjk_extra_units`) on `S' = S + upem / 32` (`vertical_extra_units`: ＭＳ 344/256 em, 游 and メイリオ 1.3/32 em within a unit); a Latin typeface's is its line and
    /// `upem / 32` units. Windows Word, 2026-11-16 (`tasks/doc/requests/2026-11-16-54-ms-pitch-result.md`: 27 East Asian typefaces, 15 Latin ones; before it, 2026-11-14,
    /// `2026-11-14-54-vertical-pitch-result.md`: the difference is in proportion to the size, intercept 0).
    fn vertical_pitch_em(&self, p: &FlowParagraph, name: Option<&str>) -> f32 {
        let snapping = p.snap_to_grid && self.line_pitch.get().is_some_and(|pitch| pitch > 0) && !matches!(p.line, LineHeight::Exact(_) | LineHeight::AtLeast(0));
        if !self.vertical.get() || snapping || matches!(p.line, LineHeight::Exact(_)) {
            return 0.0;
        }
        let ms = name.map(str::trim).is_some_and(|n| fixed_width(n) || proportional_ms(n));
        if ms {
            vertical_extra_units(256.0, 256.0) / 256.0
        } else if let Some((s, upem)) = name.and_then(|n| self.cjk_line_units(n.trim())) {
            vertical_extra_units(s, upem) / upem
        } else {
            // A Latin typeface's column is its line and `upem / 32` units (Word's XPS of 15 typefaces, same result): 1/32 em of the 2048 ones.
            let upem = name.and_then(|n| self.res.win_row_metrics(n.trim())).map_or(2048.0, |m| m.units_per_em as f32);
            (upem / 32.0).round() / upem
        }
    }

    /// The room Word leaves in a line of typeface `name` beyond its ascent and descent when the line is not on a line grid, in ems
    /// (split evenly above and below the text, see `text_dy`).
    ///
    /// Word's lines of an East Asian typeface are 1.3 times its Windows ascent plus descent (`OS/2` usWinAscent + usWinDescent): the 1.3 is
    /// Word's constant for them. In its PDFs (`lineheight-*-mode15-nogrid-*.docx` and `lineheight2-*`, 8 / 10.5 / 14 pt, in and out of a table's
    /// cell, kana and Latin letters or Latin letters only; Microsoft 365 16.0.20527.20088) the lines of メイリオ (Windows height 1.5 em) are 1.95 em
    /// apart, Meiryo UI (1.2701) 1.65, 游ゴシック and 游明朝 (1.2871) 1.673, Yu Gothic UI and Malgun Gothic (1.3301) 1.729, ＭＳ ゴシック,
    /// HGGothicM and BIZ UDGothic (1.0) 1.3, all within 0.03 pt. Arial, Segoe UI and Century keep their own height. All the typefaces that get it
    /// say in their font that they support the Japanese code page (`OS/2` ulCodePageRange1 bit 17), Malgun Gothic (a Korean typeface) too, and
    /// the three that do not get it do not: the font decides, not the text (Latin letters only in Meiryo, 游ゴシック or ＭＳ ゴシック: 1.3 as well).
    /// In a line of two typefaces the room above and below the text is that of the tallest of each (Segoe UI with ＭＳ 明朝's kana: 1.079 above,
    /// 0.291 below: 1.370 em; measured 1.372).
    ///
    /// A typeface that is not installed (another font stands in for it, whose file says nothing about it) is told by its name, from the list
    /// of the Japanese typefaces Office ships (AGENTS.md: the fallback table for typefaces the PC lacks). ＭＳ 明朝 and ＭＳ ゴシック have
    /// their own line (`ms_room_em`, 332/256 em).
    fn lead_em(&self, name: Option<&str>) -> f32 {
        let Some(n) = name.map(str::trim) else { return 0.0 };
        if fixed_width(n) || proportional_ms(n) {
            return 0.0;
        }
        self.cjk_line_units(n).map_or(0.0, |(s, upem)| cjk_extra_units(s) / upem)
    }

    /// The Windows height `S = usWinAscent + usWinDescent` of an East Asian typeface (not ＭＳ) in font units, and its units per em (the installed file's, else 2048 of the typefaces
    /// Office ships: 游, メイリオ, BIZ UD, Yu Gothic UI), or `None` where the typeface is not one whose line Word makes 1.3 times its height (`lead_em`).
    fn cjk_line_units(&self, n: &str) -> Option<(f32, f32)> {
        let japanese = self.res.is_japanese_typeface(n).unwrap_or_else(|| {
            ["游ゴシック", "Yu Gothic", "YuGothic", "游明朝", "Yu Mincho", "YuMincho", "メイリオ", "Meiryo", "BIZ UD"].iter().any(|p| n.starts_with(p)) // i18n-ok（フォント名の判定。訳さない）
        });
        if !japanese {
            return None;
        }
        let (a, d) = crate::render::font_metrics::office_metrics(n).unwrap_or_else(|| {
            let font = self.res.font(&TextStyle { font: Some(n.to_string()), ..TextStyle::default() });
            let (a, d, _) = self.res.line_metrics(&font);
            (a, d)
        });
        let upem = self.res.win_row_metrics(n).map_or(2048.0, |m| m.units_per_em as f32);
        let s = ((a + d) * upem).round();
        (s > 0.0).then_some((s, upem))
    }

    /// How far a 縦中横 run goes down a vertical line: as far as a line of its typeface is high, in points. Word's PDFs (a run of 2 to 8
    /// digits between kana, with and without a line grid, 行の幅に合わせる on and off; 9 / 10.5 / 12 / 16 / 24 pt ＭＳ ゴシック and ＭＳ 明朝, 9 / 10.5 / 12 /
    /// 16 / 24 pt 游明朝) have the kana after the run 1.29 to 1.30 em (ＭＳ), 1.67 em (游明朝) below the end of the kana before it, whatever the
    /// digits: the height of a line of the typeface (the same 1.3 times its Windows ascent plus descent as `lead_em`; the digits, horizontal
    /// text, take the height of their line down the line and lie across it). ＭＳ's is this layout's line (`ms_room_em`).
    fn tate_chu_yoko_cell(&self, name: Option<&str>, font: &Font, size: f32) -> f32 {
        self.line_metrics(name, font, size).1 + self.lead_em(name) * size
    }

    /// The horizontal scale Word draws a 縦中横 run's digits with for its `w:w`: `floor(w / 100 * X) / X`, `X` the face's average character width (OS/2 `xAvgCharWidth`: 0.5 em of
    /// ＭＳ 明朝 and ＭＳ ゴシック, 1984/2048 of 游明朝) at the size in pixels of 600 dpi, rounded (Windows Word's XPS of 180 runs and 153 more of `w:w` 10 to 200, within 5e-6:
    /// `tasks/doc/requests/2026-11-18-188-tate-chu-yoko-fit-result.md` 2; why 600 dpi is a guess). A face whose average width is not known takes `w / 100`.
    fn tate_chu_yoko_scale(&self, style: &ibis_doc_model::FlowRunStyle, name: Option<&str>, size: f32) -> f32 {
        let plain = style.scale as f32 / 100.0;
        let avg_em = match name.map(str::trim) {
            Some(n) if fixed_width(n) || n.starts_with("ＭＳ") || n.starts_with("MS ") => 0.5, // i18n-ok（フォント名の判定（ＭＳ 系）。訳さない）
            Some(n) if n.starts_with("游明朝") || n.starts_with("Yu Mincho") => 1984.0 / 2048.0, // i18n-ok（フォント名の判定（游明朝）。訳さない）
            _ => return plain,
        };
        let x = (avg_em * size * 600.0 / 72.0).round();
        if x < 1.0 { plain } else { (style.scale as f32 / 100.0 * x).floor() / x }
    }

    /// The width of a 縦中横 run's digits across the line: their advances at the size, times the run's `w:w`.
    fn tate_chu_yoko_across(&self, style: &ibis_doc_model::FlowRunStyle, units: &[Unit]) -> f32 {
        let name = units.iter().find_map(unit_char).and_then(|c| style.font_for(c));
        let size = units.iter().find_map(|u| if let Unit::Char { size, .. } = u { Some(*size) } else { None }).unwrap_or(0.0);
        units.iter().filter_map(|u| match u {
            Unit::Char { c, size, .. } => Some(advance_em(self.res, style.font_for(*c), *c) * size),
            _ => None,
        }).sum::<f32>() * self.tate_chu_yoko_scale(style, name, size)
    }

    /// A 縦中横 run on an across line (the page is turned a quarter afterwards, `turn_page_items`): its digits turned anticlockwise, one
    /// by one when `w:w` squeezes them (each takes its advance times the squeeze). They are centred on the kana's box (Word's PDFs: the same
    /// middle, 行の幅に合わせる's squeezed digits and 行を広げる's wider ones alike) and in the middle of the run's cell down the line.
    fn tate_chu_yoko_items(&self, style: &ibis_doc_model::FlowRunStyle, text: &str, x0: f32, y_base: f32, size: f32, font: &Font, color: Hsla) -> Vec<Item> {
        let xscale = self.tate_chu_yoko_scale(style, text.chars().next().and_then(|c| style.font_for(c)), size);
        let advances: Vec<f32> = text.chars().map(|c| advance_em(self.res, style.font_for(c), c) * size * xscale).collect();
        let across: f32 = advances.iter().sum();
        let cell = self.tate_chu_yoko_cell(text.chars().next().and_then(|c| style.font_for(c)), font, size);
        // The kana's box is `0.35` of its size right of the baseline's place on the page (`turn_cell_text`).
        let mut y = y_base - 0.35 * size + across / 2.0;
        let x = x0 + (cell - size) / 2.0 + size * 0.88;
        // One item to a digit, squeezed or not: the page is turned a quarter afterwards and a text item that comes of a turned one keeps one
        // offset, so the digits of a run in one item lost all but the first (GitHub issue #124: 10 came out as 1).
        text.chars().zip(advances).map(|(c, advance)| {
            let item = Item::Turned { x, y, text: c.to_string().into(), font: font.clone(), size, color, turns: -1, xscale };
            y -= advance;
            item
        }).collect()
    }

    /// The page counts and section number for fields where laying out is
    /// (a header's or footer's page, or the body's section).
    fn counts(&self) -> Option<(u32, u32, u32)> {
        self.page_counts.get().or_else(|| {
            let s = self.body_section.get();
            self.known_pages.as_ref().map(|(total, per)| (*total, per.get(s).copied().unwrap_or(0), s as u32 + 1))
        })
    }

    /// 囲い文字 (an `EQ \o` field): its character, smaller, in the middle
    /// of its enclosure drawn from `x0` (`width` wide) on baseline `y`.
    #[allow(clippy::too_many_arguments)]
    fn enclosed_items(&self, s: &FlowRunStyle, run: &ibis_doc_model::FlowRun, x0: f32, y: f32, width: f32, size: f32, color: Hsla) -> Vec<Item> {
        if run.is_field_code_shown(self.doc.show_field_codes) {
            return Vec::new();
        }
        let FlowRunKind::Field { instruction, .. } = &run.kind else { return Vec::new() };
        let Some((_, inner)) = ibis_doc_model::field_format::enclosed(instruction.as_str()) else { return Vec::new() };
        let count = inner.chars().count();
        let small = match s.enclosure_sizes {
            Some((_, inner_size)) => inner_size as f32 / 100.0,
            None if count <= 1 => size * 0.7,
            None => size * 0.5,
        };
        let w: f32 = inner.chars().map(|c| typeface_room(s.font_for(c), c, small, self.res.text_width(&c.to_string(), &self.font(s, s.font_for(c)), small))).sum();
        let font = self.font(s, s.font_for(inner.chars().next().unwrap_or('あ')));
        let offsets: Vec<f32> = inner.chars().scan(0.0f32, |at, c| {
            let x = *at;
            *at += typeface_room(s.font_for(c), c, small, self.res.text_width(&c.to_string(), &self.font(s, s.font_for(c)), small));
            Some(x)
        }).collect();
        // Centred on the enclosure, on its baseline (Word's PDFs of GitHub issue #180: the character's baseline is the enclosure's, whatever the two sizes are).
        vec![Item::Text { x: x0 + (width - w) / 2.0, y, text: inner.into(), offsets: offsets.into(), font, size: small, color }]
    }

    /// The blank room of a 重ね書き (EQ \o\ad, \o\al, \o\ar): the second
    /// argument's width when it spreads the first, else the widest
    /// argument's (as spaces of about that width).
    fn overstrike_room(&self, align: ibis_doc_model::field_format::OverstrikeAlign, args: &[(String, f32)]) -> String {
        use ibis_doc_model::field_format::OverstrikeAlign;
        let blank = |text: &str| -> String { text.chars().map(|c| if c.is_whitespace() { c } else if (c as u32) >= 0x2E80 { '\u{3000}' } else { ' ' }).collect() };
        let pick = match align {
            OverstrikeAlign::Distribute => args.get(1).or(args.first()),
            _ => args.iter().max_by_key(|(t, _)| t.chars().count()),
        };
        pick.map(|(t, _)| blank(t)).unwrap_or_default()
    }

    /// The width of each argument of a stack with no switch (`EQ \o(…,…)`) at its size: the sizes the code's runs give the first two (`enclosure_sizes`), else `size`.
    fn overstrike_widths(&self, s: &FlowRunStyle, args: &[(String, f32)], size: f32) -> Vec<f32> {
        args.iter()
            .enumerate()
            .map(|(k, (text, _))| {
                let at = match (k, s.enclosure_sizes) {
                    (0, Some((a, _))) => a as f32 / 100.0,
                    (1, Some((_, b))) => b as f32 / 100.0,
                    _ => size,
                };
                text.chars().map(|c| self.res.text_width(&c.to_string(), &self.font(s, s.font_for(c)), at)).sum()
            })
            .collect()
    }

    /// A 重ね書き's arguments drawn from `x0` on baseline `y` (`width` wide):
    /// the first spread over the room (\ad), or each from the left (\al) or
    /// the right (\ar), raised by its `\s\up n`.
    #[allow(clippy::too_many_arguments)]
    fn overstrike_items(&self, s: &FlowRunStyle, run: &ibis_doc_model::FlowRun, x0: f32, y: f32, width: f32, size: f32, color: Hsla) -> Vec<Item> {
        use ibis_doc_model::field_format::OverstrikeAlign;
        if run.is_field_code_shown(self.doc.show_field_codes) {
            return Vec::new();
        }
        let FlowRunKind::Field { instruction, .. } = &run.kind else { return Vec::new() };
        let Some((align, args)) = ibis_doc_model::field_format::overstrike(instruction.as_str()) else { return Vec::new() };
        let mut out = Vec::new();
        let shown: &[(String, f32)] = if align == OverstrikeAlign::Distribute { &args[..args.len().min(1)] } else { &args };
        for (k, (text, raise)) in shown.iter().enumerate() {
            // A stack with no switch sets each argument at the size its run of the code has.
            let size = match (align, k, s.enclosure_sizes) {
                (OverstrikeAlign::Center, 0, Some((a, _))) => a as f32 / 100.0,
                (OverstrikeAlign::Center, 1, Some((_, b))) => b as f32 / 100.0,
                _ => size,
            };
            let widths: Vec<f32> = text.chars().map(|c| self.res.text_width(&c.to_string(), &self.font(s, s.font_for(c)), size)).collect();
            let total: f32 = widths.iter().sum();
            let n = widths.len();
            if n == 0 {
                continue;
            }
            let (start, gap) = match align {
                OverstrikeAlign::Distribute if n > 1 && width > total => (0.0, (width - total) / (n as f32 - 1.0)),
                OverstrikeAlign::Distribute | OverstrikeAlign::Left => (0.0, 0.0),
                OverstrikeAlign::Right => ((width - total).max(0.0), 0.0),
                OverstrikeAlign::Center => (((width - total) / 2.0).max(0.0), 0.0),
            };
            let mut offsets = Vec::with_capacity(n);
            let mut at = start;
            for w in &widths {
                offsets.push(at);
                at += w + gap;
            }
            let font = self.font(s, s.font_for(text.chars().next().unwrap_or('あ')));
            out.push(Item::Text { x: x0, y: y - raise, text: text.clone().into(), offsets: offsets.into(), font, size, color });
        }
        out
    }

    /// A 組み文字's bracket drawn in the run's East Asian typeface, whatever the bracket is (Word's PDF of 020: the round brackets of a combined
    /// run are drawn in ＭＳ 明朝 at 14 pt, 7.0 pt wide; the Latin face would give 8.75 pt): the typeface's own advance, which for a fixed-width
    /// face is its half-width cell.
    fn bracket_width(&self, s: &FlowRunStyle, ch: char, size: f32) -> f32 {
        let name = s.font_for('\u{3042}');
        if name.is_some_and(|n| fixed_width(n.trim())) {
            return size / 2.0;
        }
        self.res.text_width(&ch.to_string(), &self.font(s, name), size)
    }

    /// 組み文字's room: its characters in two lines of half `size` (the
    /// first half, rounded up, on the first), the wider of them, and its
    /// brackets at `size`.
    fn combine_width(&self, s: &FlowRunStyle, chars: &[char], size: f32, c: ibis_doc_model::Combine) -> f32 {
        self.combine_part_width(s, chars, size, c, true, true)
    }

    /// `combine_width` of a part of a 割注 that a line parts: its opening bracket is on the first part (`open`), its closing one on the last (`close`).
    fn combine_part_width(&self, s: &FlowRunStyle, chars: &[char], size: f32, c: ibis_doc_model::Combine, open: bool, close: bool) -> f32 {
        let measure = |list: &[char], at: f32| list.iter().map(|ch| self.res.text_width(&ch.to_string(), &self.font(s, s.font_for(*ch)), at)).sum::<f32>();
        let first = chars.len().div_ceil(2);
        let block = measure(&chars[..first], combine_size(size)).max(measure(&chars[first..], combine_size(size)));
        block + c.brackets().map_or(0.0, |(a, b)| if open { self.bracket_width(s, a, size) } else { 0.0 } + if close { self.bracket_width(s, b, size) } else { 0.0 })
    }

    /// Where a 割注 (a combined run of eight characters or more, `units[i]` its first and as wide as all of it) that does not fit the `left` of the line is
    /// parted: the opening bracket and the most columns of two characters that fit go on this line, no more than leave four characters for the next, and the
    /// closing bracket is on the next line (Word's XPS, GitHub issue #180: five columns of 5 pt and the bracket of 5.25 pt in 30.2 pt; thirteen
    /// characters in 41.8 pt are 5 and 4 on the first line, 2 and 2 on the second). Fewer than four characters fit: the whole goes to the next line (`false`).
    /// Otherwise the parts' widths are put on the units that start them, which a line may now break between.
    fn warichu_split(&self, p: &FlowParagraph, units: &mut [Unit], i: usize, left: f32) -> bool {
        let Unit::Char { style, size, width: total, .. } = &units[i] else { return false };
        let (style, size, total) = (*style, *size, *total);
        let Some(run) = p.runs.get(style).filter(|_| style != LABEL) else { return false };
        let Some(c) = run.style.combine else { return false };
        if total <= 0.0 || total <= left + 0.01 {
            return false;
        }
        let open = i == 0 || !matches!(&units[i - 1], Unit::Char { style: before, .. } if *before == style);
        let end = (i..units.len()).find(|&k| k > i && !matches!(&units[k], Unit::Char { style: s, width, .. } if *s == style && *width == 0.0)).unwrap_or(units.len());
        let chars: Vec<char> = units[i..end].iter().filter_map(unit_char).collect();
        if chars.len() != end - i || chars.len() < 8 {
            return false;
        }
        for n1 in (4..=chars.len() - 4).rev() {
            // An odd count is a part only where it is what four characters left over allow.
            if n1 % 2 == 1 && n1 != chars.len() - 4 {
                continue;
            }
            if self.combine_part_width(&run.style, &chars[..n1], size, c, open, false) <= left + 0.01 {
                let first = self.combine_part_width(&run.style, &chars[..n1], size, c, open, false);
                let rest = self.combine_part_width(&run.style, &chars[n1..], size, c, false, true);
                if let Unit::Char { width, inset, .. } = &mut units[i] {
                    *width = first;
                    *inset = 0.0;
                }
                if let Unit::Char { width, inset, .. } = &mut units[i + n1] {
                    *width = rest;
                    *inset = 0.0;
                }
                return true;
            }
        }
        false
    }

    /// Which brackets of a 割注 a part of it drawn (`text`) has: the opening one if the part starts the run, the closing one if it ends it.
    fn combine_brackets_of(&self, p: &FlowParagraph, style: usize, text: &str) -> (bool, bool) {
        let Some(FlowRunKind::Text(whole)) = p.runs.get(style).map(|r| &r.kind) else { return (true, true) };
        let whole: String = whole.chars().filter(|ch| *ch != '\u{200b}').collect();
        let part: String = text.chars().filter(|ch| *ch != '\u{200b}').collect();
        if part.len() >= whole.len() || part.is_empty() {
            (true, true)
        } else if whole.starts_with(&part) {
            (true, false)
        } else if whole.ends_with(&part) {
            (false, true)
        } else {
            (false, false)
        }
    }

    /// 組み文字 drawn from `x0` on baseline `y`: the first line's baseline
    /// 0.517 em above, the second's 0.186 em below (Word's PDF of DW14 at
    /// 14 pt), between its brackets at the text's size.
    #[allow(clippy::too_many_arguments)]
    fn combine_items(&self, s: &FlowRunStyle, c: ibis_doc_model::Combine, text: &str, x0: f32, y: f32, size: f32, color: Hsla, (with_open, with_close): (bool, bool)) -> Vec<Item> {
        let chars: Vec<char> = text.chars().filter(|ch| *ch != '\u{200b}').collect();
        let half = combine_size(size);
        let first = chars.len().div_ceil(2);
        let width = |ch: char, at: f32| self.res.text_width(&ch.to_string(), &self.font(s, s.font_for(ch)), at);
        let mut x = x0;
        let line_w = |list: &[char]| list.iter().map(|ch| width(*ch, half)).sum::<f32>();
        let block = line_w(&chars[..first]).max(line_w(&chars[first..]));
        let mut out: Vec<Item> = Vec::new();
        let mut put = |ch: char, x: f32, y: f32, at: f32, name: Option<&str>| {
            let font = self.font(s, name);
            out.push(Item::Text { x, y, text: ch.to_string().into(), offsets: Arc::from([0.0f32]), font, size: at, color });
        };
        // The brackets are drawn in the East Asian typeface, the characters in the typeface of each.
        let bracket_face = s.font_for('\u{3042}');
        if let Some((open, _)) = c.brackets().filter(|_| with_open) {
            put(open, x, y, size, bracket_face);
            x += self.bracket_width(s, open, size);
        }
        for (list, dy) in [(&chars[..first], -0.517), (&chars[first..], 0.186)] {
            // Each line from the block's left: the shorter one of an odd count is not centred (Word's PDFs of GitHub issue #180: five characters,
            // the second line's left edge is at the block's, 108.9 pt after the bracket at 103.5 pt).
            let mut lx = x;
            for ch in list {
                put(*ch, lx, y + dy * size, half, s.font_for(*ch));
                lx += width(*ch, half);
            }
        }
        if let Some((_, close)) = c.brackets().filter(|_| with_close) {
            put(close, x + block, y, size, bracket_face);
        }
        out
    }

    /// The width of the column a ruby at its base's right (`rubyAlign="rightVertical"`) takes after the base: 4/3 of the ruby's size times the Windows height of the ruby's typeface
    /// (`usWinAscent + usWinDescent` in ems: 1.0 for ＭＳ and BIZ UD, 1.2871 for 游, 1.5 for メイリオ), the ruby's characters in the middle of it (Word's XPS of GitHub issue #180
    /// and many-patterns-3, 1,350 cases in 6 typefaces: 4/3 of the Windows height times the size within 0.15 pt in 1,000 of them, 0.31 pt at most).
    fn right_vertical_column(&self, s: &FlowRunStyle, size: u32) -> f32 {
        let style = FlowRunStyle { size, ..s.clone() };
        let name = style.font_for('あ'); // i18n-ok（全角の文字）
        let font = self.font(&style, name);
        let (a, d) = Self::win_ascent_descent(name, &font.family, || {
            let (a, d, _) = self.res.line_metrics(&font);
            (a, d)
        });
        4.0 / 3.0 * (a + d) * size as f32 / 100.0
    }

    /// The ruby of `rubyAlign="rightVertical"` across a line: its characters stacked at the right of the base (`x0` + `width` less the column), `size` each way, the stack's middle at the
    /// middle of the base's line: `(usWinAscent - usWinHeight / 2) x base size` above the base's baseline less `(sTypoAscender - 1/2) x ruby size` (the middle of the ruby's box
    /// is that much above its baseline), the characters in the middle of the column (Word's XPS of GitHub issue #180 and many-patterns-3, 1,341 of 1,350 cases within 0.25 pt).
    fn right_vertical_items(&self, s: &FlowRunStyle, text: &str, size: u32, x0: f32, y: f32, width: f32, color: Hsla) -> Vec<Item> {
        let rs = size as f32 / 100.0;
        let style = FlowRunStyle { size, ..s.clone() };
        let name = style.font_for('あ'); // i18n-ok（全角の文字）
        let font = self.font(&style, name);
        let column = self.right_vertical_column(s, size);
        let parent_font = self.font(s, s.font_for('あ')); // i18n-ok（全角の文字）
        let (win_a, win_d) = Self::win_ascent_descent(s.font_for('あ'), &parent_font.family, || { // i18n-ok（全角の文字）
            let (a, d, _) = self.res.line_metrics(&parent_font);
            (a, d)
        });
        let typo_a = name.and_then(crate::render::font_metrics::office_typo_ascender).unwrap_or_else(|| Self::win_ascent_descent(name, &font.family, || (0.0, 0.0)).0);
        let centre_above = (win_a - (win_a + win_d) / 2.0) * s.size as f32 / 100.0 - (typo_a - 0.5) * rs;
        let chars: Vec<char> = text.chars().collect();
        let m = chars.len() as f32;
        let left = x0 + width - column + (column - rs) / 2.0;
        chars
            .iter()
            .enumerate()
            .map(|(k, c)| {
                let w = self.res.text_width(&c.to_string(), &font, rs);
                let offsets: Arc<[f32]> = Arc::from([0.0f32]);
                Item::Text { x: left + (rs - w) / 2.0, y: y - centre_above + (k as f32 - (m - 1.0) / 2.0) * rs, text: c.to_string().into(), offsets, font: font.clone(), size: rs, color }
            })
            .collect()
    }

    /// The width of a ruby's text (set at its own size, in the typeface of its first character).
    fn ruby_text_width(&self, s: &FlowRunStyle, text: &str, size: u32) -> f32 {
        let style = FlowRunStyle { size, ..s.clone() };
        let font = self.font(&style, style.font_for(text.chars().next().unwrap_or('あ'))); // i18n-ok（全角の文字）
        text.chars().map(|c| self.res.text_width(&c.to_string(), &font, size as f32 / 100.0)).sum()
    }

    /// ルビ over its base drawn from `x0` (`width` wide) on baseline `y`:
    /// its baseline `raise` above, centred, from the left or right, or
    /// spread over the base (均等割り付け: the first character at the
    /// base's left and the last at its right; 1-2-1: equal room around
    /// each), as Word's PDF of DW14 places them.
    fn ruby_items(&self, s: &FlowRunStyle, kind: &FlowRunKind, x0: f32, y: f32, width: f32, color: Hsla) -> Vec<Item> {
        use ibis_doc_model::RubyAlign;
        let FlowRunKind::Ruby { text, size, raise, align, .. } = kind else { return Vec::new() };
        if text.is_empty() {
            return Vec::new();
        }
        if *align == RubyAlign::RightVertical && !self.vertical.get() {
            return self.right_vertical_items(s, text, *size, x0, y, width, color);
        }
        let style = FlowRunStyle { size: *size, ..s.clone() };
        let rs = *size as f32 / 100.0;
        let font = self.font(&style, style.font_for(text.chars().next().unwrap_or('あ')));
        let widths: Vec<f32> = text.chars().map(|c| self.res.text_width(&c.to_string(), &font, rs)).collect();
        let total: f32 = widths.iter().sum();
        let n = widths.len() as f32;
        let room = width - total;
        let (start, gap) = match align {
            RubyAlign::DistributeLetter if room > 0.0 && n > 1.0 => (0.0, room / (n - 1.0)),
            RubyAlign::DistributeSpace if room > 0.0 => (room / n / 2.0, room / n),
            RubyAlign::Left => (0.0, 0.0),
            RubyAlign::Right => (room, 0.0),
            _ => (room / 2.0, 0.0),
        };
        let mut offsets = Vec::with_capacity(widths.len());
        let mut at = start;
        for w in &widths {
            offsets.push(at);
            at += w + gap;
        }
        // Across, the ruby's baseline is `raise` above the base's. In a column (the page is turned a quarter afterwards, the upright characters' box
        // centred 0.35 em of their size from the line's baseline) Word puts the ruby's centre `floor(raise)` pt to the right of the base's centre, whatever the
        // ruby's size, its alignment and the base's length (Word's PDFs of GitHub issue #188, D5a, ＭＳ 明朝 10.5 pt: hpsRaise 0..36 half points of hps 8..21:
        // 0, 4.9, 9.0, 13.0 and 18.0 pt; 17, 19, 25 and 35 half points 8, 9, 12 and 17 pt), so the baseline is lifted by that less the difference of the two
        // boxes' centres from their baselines.
        let lift = if self.vertical.get() { (*raise as f32 / 100.0).floor() - 0.35 * (rs - s.size as f32 / 100.0) } else { *raise as f32 / 100.0 };
        vec![Item::Text { x: x0, y: y - lift, text: text.clone().into(), offsets: offsets.into(), font, size: rs, color }]
    }

    /// 傍点 (`w:em`) over (or under) the characters of a run drawn at `x0`
    /// on baseline `y` (`offsets`: each character's start, then the end), as
    /// Word draws them (its PDF of DRUN, 14 pt): the mark is a character
    /// of the run's size — ． for a dot, 、 for a comma (their baseline
    /// 1.011 em above the text's), ˚ for a circle (0.291 em above), ． for a
    /// dot under (0.377 em below) — placed from each character's middle
    /// (0.166, 0.154 and 0.199 em to the left), none over spaces.
    #[allow(clippy::too_many_arguments)]
    fn emphasis_marks(&self, s: &FlowRunStyle, label: bool, x0: f32, y: f32, size: f32, text: &str, offsets: &[f32], color: Hsla) -> Vec<Item> {
        use ibis_doc_model::EmphasisMark;
        // In a column (the page is turned afterwards: `up` is then to the right of the base's centre, `left` back along the column from the base's
        // middle) Word's PDFs (XPS) of GitHub issue #188 D5c and many-patterns 3 (6 faces x 8..36 pt) put a mark's box centre `up` right of the column's
        // centre — dot and comma 0.3955, circle 0.352, underDot -0.889 of the size (±0.01; the vertical forms of the dot and the comma and the ring
        // are the font's, Word sets each by the box's vertical origin) — and its box top 0.535, 0.385 and 0.565 of the size above the base's baseline,
        // which is 0.176, 0.026 and 0.206 of the size back from the base's middle. 游明朝 and 游ゴシック sit 0.0276 and 0.0384 of the size nearer the
        // centre (no formula found: not applied).
        let vertical = self.vertical.get();
        let (mark, up, left) = match s.emphasis {
            _ if label => return Vec::new(),
            EmphasisMark::None => return Vec::new(),
            EmphasisMark::Dot => ('．', if vertical { 0.3955 } else { 1.0114 }, if vertical { 0.176 } else { 0.166 }),
            EmphasisMark::Comma => ('、', if vertical { 0.3955 } else { 1.0114 }, if vertical { 0.176 } else { 0.166 }),
            EmphasisMark::Circle => ('˚', if vertical { 0.352 } else { 0.2914 }, if vertical { 0.026 } else { 0.154 }),
            EmphasisMark::UnderDot => ('．', if vertical { -0.889 } else { -0.377 }, if vertical { 0.206 } else { 0.199 }),
        };
        let font = self.font(s, s.font_for(if mark == '˚' { 'a' } else { 'あ' }));
        let one: Arc<[f32]> = Arc::from([0.0f32]);
        text.chars()
            .enumerate()
            .filter(|(_, c)| !c.is_whitespace() && *c != '\u{200b}')
            .filter_map(|(i, _)| {
                let (a, b) = (*offsets.get(i)?, *offsets.get(i + 1)?);
                let middle = x0 + (a + b) / 2.0;
                Some(Item::Text { x: middle - left * size, y: y - up * size, text: mark.to_string().into(), offsets: one.clone(), font: font.clone(), size, color })
            })
            .collect()
    }

    /// Look up a table by ID in the document's body, headers and footers.
    fn find_table_by_id<'a>(doc: &'a FlowDocument, table_id: u32) -> Option<&'a FlowTable> {
        fn search<'a>(blocks: &'a [Block], id: u32) -> Option<&'a FlowTable> {
            for b in blocks {
                match b {
                    Block::Table(t) => {
                        if t.id == id {
                            return Some(t);
                        }
                        for row in &t.rows {
                            for cell in &row.cells {
                                if let Some(found) = search(&cell.blocks, id) {
                                    return Some(found);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        search(&doc.body, table_id)
            .or_else(|| search(&doc.header, table_id))
            .or_else(|| search(&doc.footer, table_id))
            .or_else(|| search(&doc.header_first, table_id))
            .or_else(|| search(&doc.footer_first, table_id))
            .or_else(|| search(&doc.header_even, table_id))
            .or_else(|| search(&doc.footer_even, table_id))
    }

    /// What the field `instruction` with the saved result `shown` shows on page `page_number`: a `HYPERLINK` field shows its saved result (the text of the link; the address
    /// only when it has none), as Word does; another field is `field_text`.
    fn shown_field_text(&self, instruction: &str, shown: &str, page_number: u32) -> Option<String> {
        let t = instruction.trim_start();
        if t.get(..9).is_some_and(|w| w.eq_ignore_ascii_case("HYPERLINK")) && t[9..].starts_with(char::is_whitespace) {
            return Some(self.doc.hyperlink_text(instruction.trim(), shown));
        }
        self.field_text(instruction, page_number)
    }

    /// What a field the layout computes shows on page `page_number` (`None`:
    /// its saved result): `PAGE` with its switches, the page counts and the
    /// section once known, a formula over them, `DATE` and `TIME`.
    fn field_text(&self, instruction: &str, page_number: u32) -> Option<String> {
        use ibis_doc_model::field_format::{compare_field_text, format_field_number, formula_text, if_field_text};
        let expanded_storage;
        let t = if instruction.contains('{') && instruction.contains('}') {
            expanded_storage = ibis_doc_model::expand_nested_fields(instruction, |inner| {
                match inner.split_whitespace().next() {
                    Some("PAGE") if !inner.contains('\\') => Some(page_number.to_string()),
                    _ => self.field_text(inner, page_number).or_else(|| {
                        Some(ibis_doc_model::flow_command::evaluate_field_instruction(inner, self.doc, ibis_doc_model::FlowPos::new(0, 0)))
                    }),
                }
            });
            expanded_storage.trim()
        } else {
            instruction.trim()
        };
        if t.starts_with('=') {
            if let Some((table_id, row, col)) = self.current_cell.get() {
                if let Some(table) = Self::find_table_by_id(self.doc, table_id) {
                    let resolver = ibis_doc_model::FlowTableCellResolver { table, row, col };
                    return ibis_doc_model::formula_text_with_resolver(t, Some(&resolver));
                }
            }
            return formula_text(t);
        }
        let t_upper = t.to_ascii_uppercase();
        if t_upper.starts_with("IF ") || t_upper == "IF" {
            return if_field_text(t);
        }
        if t_upper.starts_with("COMPARE ") || t_upper == "COMPARE" {
            return compare_field_text(t);
        }
        let counts = self.counts();
        let n = match t.split_whitespace().next()? {
            "PAGE" => {
                let text = format_field_number(page_number.into(), t).unwrap_or_else(|| match &*self.page_format.borrow() {
                    Some(f) => ibis_doc_model::format_page_number(page_number, f),
                    None => page_number.to_string(),
                });
                return Some(format!("{}{text}", self.chapter_prefix.borrow()));
            }
            "NUMPAGES" => counts?.0,
            "SECTIONPAGES" => counts?.1,
            "SECTION" => counts.map_or(self.body_section.get() as u32 + 1, |c| c.2),
            // DATE and TIME show the time now, as Word updates them when it
            // shows or prints the document.
            "DATE" | "TIME" => return ibis_doc_model::date_field::date_field_text(t, &crate::clock::local_now()),
            "CREATEDATE" => return self.doc.created_date_text(t),
            "SAVEDATE" => return self.doc.saved_date_text(t),
            "PRINTDATE" => return self.doc.print_date_text(t),
            "AUTHOR" => return Some(ibis_doc_model::apply_instruction_text_switches(self.doc.author_text(t), t)),
            "TITLE" => return Some(ibis_doc_model::apply_instruction_text_switches(self.doc.title_text(t), t)),
            "SUBJECT" => return Some(ibis_doc_model::apply_instruction_text_switches(self.doc.subject_text(t), t)),
            "COMMENTS" => return Some(ibis_doc_model::apply_instruction_text_switches(self.doc.comments_text(t), t)),
            "DOCPROPERTY" => return self.doc.doc_property_text(t).map(|v| ibis_doc_model::apply_instruction_text_switches(v, t)),
            "NOTEREF" => return self.doc.noteref_text(t).map(|v| ibis_doc_model::apply_instruction_text_switches(v, t)),
            "STYLEREF" => return self.doc.styleref_text_at(t, self.field_paragraph.get()).map(|v| ibis_doc_model::apply_instruction_text_switches(v, t)),
            "FILESIZE" => return Some(self.doc.file_size_text(t)),
            "DOCVARIABLE" => return Some(self.doc.doc_variable_text(t)),
            "USERNAME" => {
                let name = self.doc.properties.creator.clone().unwrap_or_else(crate::shell::user_name::user_name);
                return Some(ibis_doc_model::format_user_name(&name, t));
            }
            "USERINITIALS" => {
                let inits = self.doc.properties.creator.as_deref().map(ibis_doc_model::field_format::initials_from_name).unwrap_or_else(crate::shell::user_name::user_initials);
                return Some(ibis_doc_model::format_user_initials(&inits, t));
            }
            "USERADDRESS" => return Some(self.doc.user_address_text(t)),
            "QUOTE" => return Some(self.doc.quote_text(t)),
            "SYMBOL" => return Some(self.doc.symbol_text(t)),
            "SET" => return Some(String::new()),
            "IF" => return if_field_text(t),
            "COMPARE" => return compare_field_text(t),
            "HYPERLINK" => return Some(self.doc.hyperlink_text(t, "")),
            "TEMPLATE" => return Some(self.doc.template_text(t)),
            "ADVANCE" => return Some(String::new()),
            "INFO" => return self.doc.info_text(t),
            "KEYWORDS" | "LASTSAVEDBY" | "NUMCHARS" | "NUMWORDS" | "REVNUM" => return self.doc.bare_info_text(t),
            "ASK" => return Some(String::new()),
            "FILLIN" | "FILL-IN" => return Some(self.doc.fillin_text(t)),
            "AUTOTEXT" => return Some(ibis_doc_model::field_format::format_autotext(t)),
            "AUTOTEXTLIST" => return Some(ibis_doc_model::field_format::format_autotextlist(t)),
            "CITATION" => return Some(ibis_doc_model::field_format::format_citation(t)),
            "BIBLIOGRAPHY" => return Some(ibis_doc_model::field_format::format_bibliography(t)),
            _ => return None,
        };
        Some(format_field_number(n.into(), t).unwrap_or_else(|| n.to_string()))
    }

    /// A text box in the text that does not wrap its text (`wrap="none"`) is as wide as its longest line (and its
    /// margins) when that is less than its width, as Word draws it (DTXB: a 2-character box is 33 pt wide
    /// whatever the extent says); a longer line wraps at the extent.
    fn fit_unwrapped_text_box(&self, shape: &mut ibis_doc_model::Shape) {
        let frame_width = shape.frame.width;
        let body = match &mut shape.kind {
            ibis_doc_model::ShapeKind::Auto(a) => a.text.as_mut(),
            ibis_doc_model::ShapeKind::TextBox(body) => Some(body),
            _ => None,
        };
        let Some(body) = body else { return };
        if body.wrap || body.vertical || body.sideways != 0 {
            return;
        }
        let mut widest = 0.0f32;
        for paragraph in &body.paragraphs {
            let mut width = 0.0f32;
            for r in paragraph.runs.iter().filter(|r| !r.text.is_empty()) {
                let size = r.style.font_size.unwrap_or(body.style.font_size) as f32 / 100.0;
                let cjk = r.text.chars().any(crate::render::rich_text::is_cjk);
                let name = if cjk { r.style.font.as_deref() } else { r.style.fallback_font.as_deref().or(r.style.font.as_deref()) };
                let font = self.res.font(&TextStyle { font_size: (size * 100.0) as u32, bold: r.style.bold.unwrap_or(false), italic: r.style.italic.unwrap_or(false), font: name.map(str::to_string), ..TextStyle::default() });
                width += self.res.text_width(&r.text, &font, size);
            }
            widest = widest.max(width);
        }
        let natural = ((widest * 12_700.0).round() as i64) + body.insets.left + body.insets.right;
        if natural > 0 && natural < frame_width {
            shape.frame.width = natural;
        } else {
            // Too long for the box: it grows to the line it is on (the cell's width: DTXB, a 78.7 pt box in an
            // 89.4 pt cell is 89.4 pt wide) and wraps there.
            let room = self.line_room.get();
            if room < f32::MAX && pt(frame_width) < room {
                shape.frame.width = ((room * 12_700.0) as i64).min(natural);
            }
            body.wrap = true;
        }
    }

    /// A formula as a unit: typeset at the run's size in the run's typeface, as wide and as high as it comes. Word's math typeface
    /// is Cambria Math (the shared alias table finds a substitute for it).
    fn math_unit(&self, math: &ibis_doc_model::math::Math, display: bool, style: &ibis_doc_model::FlowRunStyle, size: f32) -> Unit {
        let font = self.font(style, Some("Cambria Math"));
        let mut measure = |text: &str, size: f32, italic: bool| {
            let mut f = font.clone();
            if italic {
                f.style = FontStyle::Italic;
            }
            let (ascent, descent, _) = self.res.line_metrics(&f);
            (self.res.text_width(text, &f, size), ascent * size, descent * size)
        };
        let boxed = crate::render::math_view::typeset(math, size, &mut measure);
        let (w, h, descent) = (boxed.width, boxed.ascent + boxed.descent, boxed.descent);
        let color = hsla(ibis_doc_model::Color::opaque(style.color));
        Unit::Math { boxed: Arc::new(boxed), w, h, descent, font, color, display }
    }

    /// A typeset formula as page items: its text where its pieces are (from `x` on the baseline `baseline`), its rules and lines.
    fn math_items(&self, boxed: &crate::render::math_view::MathBox, x: f32, baseline: f32, font: &Font, color: Hsla) -> Vec<Item> {
        use crate::render::math_view::Piece;
        boxed
            .pieces
            .iter()
            .map(|piece| match piece {
                Piece::Text { x: dx, y, text, size, italic, color: own } => {
                    let color = own.map_or(color, |c| hsla(ibis_doc_model::Color::opaque(c)));
                    let mut f = font.clone();
                    if *italic {
                        f.style = FontStyle::Italic;
                    }
                    let mut offsets = Vec::with_capacity(text.chars().count());
                    let mut at = 0.0;
                    for c in text.chars() {
                        offsets.push(at);
                        at += self.res.text_width(&c.to_string(), &f, *size);
                    }
                    Item::Text { x: x + dx, y: baseline + y, text: SharedString::from(text.clone()), offsets: offsets.into(), font: f, size: *size, color }
                }
                Piece::Rule { x: dx, y, w, h, color: own } => Item::Rect { x: x + dx, y: baseline + y, w: *w, h: *h, color: own.map_or(color, |c| hsla(ibis_doc_model::Color::opaque(c))) },
                Piece::Line { x0, y0, x1, y1, t, color: own } => Item::Stroke { x: x + x0, y: baseline + y0, x2: x + x1, y2: baseline + y1, w: *t, color: own.map_or(color, |c| hsla(ibis_doc_model::Color::opaque(c))) },
            })
            .collect()
    }

    /// The paragraph's text as units, with the page number for `PAGE`.
    fn units(&self, p: &FlowParagraph, page_number: u32) -> (Vec<Unit>, Vec<(usize, bool)>) {
        let (units, positions, _) = self.units_cut(p, page_number, &[]);
        (units, positions)
    }

    /// `units`, where the units at `line_starts` (indices into the units) begin a wrapped line, so that Word's carrying over of a fallback typeface is cut there
    /// (`FallbackCarry`); and the indices of the units whose typeface depends on the carrying over (a line that begins with one draws it in another typeface).
    fn units_cut(&self, p: &FlowParagraph, page_number: u32, line_starts: &[usize]) -> (Vec<Unit>, Vec<(usize, bool)>, Vec<usize>) {
        let carried_units: RefCell<Vec<usize>> = RefCell::new(Vec::new());
        let carry: RefCell<FallbackCarry> = RefCell::new(FallbackCarry::default());
        self.field_paragraph.set(Some(p.id));
        let mut out = Vec::new();
        // The position of each unit in the paragraph, and whether it is the
        // first unit of its position (a page number's digits are one).
        let mut positions: Vec<(usize, bool)> = Vec::new();
        let mut position = 0;
        // A list item's number or bullet, and what follows it: no positions.
        if let (Some(label), Some(list)) = (self.labels.get(&p.id), p.list) {
            let style = &p.list_style;
            let size = style.size as f32 / 100.0;
            // A picture bullet (`w:lvlPicBulletId`): the picture, as high as the text
            // (or as its shape says), not the bullet's character.
            let numbering = &self.doc.styles.numbering;
            let picture = numbering.level(list.num_id, list.level).and_then(|l| l.pic_bullet).and_then(|id| numbering.pic_bullets.get(&id));
            if let Some(&(media, shape)) = picture {
                let (w, h) = shape.map_or((size, size), |s| (pt(s.width), pt(s.height)));
                out.push(Unit::Picture { media, w, h, ee_top: 0.0, ee_bottom: 0.0 });
                positions.push((0, false));
            }
            // A bullet of a symbol typeface takes the room the typeface gives it, where it is installed (Word's PDF of 072: Wingdings'
            // check mark, 0.786 em, ends before the hanging indent, which the stand-in ✓ of a Japanese face, an em, passes).
            let symbol = self.doc.styles.numbering.level(list.num_id, list.level).and_then(|l| l.symbol.clone()).filter(|(f, _)| self.res.has_typeface(f));
            // (The text presentation selector the reader adds to a stand-in that is an emoji by default is no glyph of the typeface.)
            let glyphs = label.chars().filter(|&c| c != '\u{fe0e}').count();
            for c in label.chars().filter(|&c| picture.is_none() && !(symbol.is_some() && c == '\u{fe0e}')) {
                // A bullet in the geometric shapes (●, ■, ◆…) defaults to Arial in
                // Word's PDF of DWID (small) if no bullet font was specified, not the Japanese font's.
                let mut name = label_font_name(style, c);
                // A symbol the typeface has no glyph for is drawn in the typeface Word's font linking gives it (Word's PDF, bullets with no typeface of their
                // own in a document whose Latin typeface is Courier New: ■ □ in Courier New, ◆ ◇ in Cambria Math, ★ ☆ ✓ ✔ ❖ ✧ ➔ ➢ in Segoe UI Symbol): not whatever the
                // system picks (Segoe UI Emoji's heavy ✔, GitHub issue #142). Only where those typefaces are installed.
                // The characters Word's table of fallback typefaces has (`fallback_typeface`: ■ ▲ ▼ Courier New, ◆ ◇ Cambria Math, ★ ☆ Segoe UI Symbol…) go to it first.
                let by_table = if picture.is_none() && symbol.is_none() { self.typeface_for_character(style, name, c, size) } else { name };
                if by_table != name {
                    name = by_table;
                } else if picture.is_none() && symbol.is_none() && matches!(c as u32, 0x2190..=0x27bf) {
                    let probe = self.font(style, name);
                    if self.res.text_system().advance(self.res.text_system().resolve_font(&probe), px(size), c).is_err() {
                        let chain: &[&str] = if matches!(c as u32, 0x25a0..=0x25ff) { &["Cambria Math", "Segoe UI Symbol"] } else { &["Segoe UI Symbol"] };
                        if let Some(found) = chain.iter().find(|f| self.res.has_typeface(f)) {
                            name = Some(*found);
                        }
                    }
                }
                let stand_in_font = self.font(style, name);
                // Where the typeface is installed its own glyph is drawn (the character is the code, U+F000 and it), as Word does: the
                // glyph is the size and shape the typeface gives it, not those of the character that stands in for it elsewhere.
                let (drawn, font, width) = match &symbol {
                    Some((face, code)) if glyphs == 1 => {
                        let font = self.font(style, Some(face.as_str()));
                        let width = self.res.text_width(&code.to_string(), &font, size);
                        (*code, font, width)
                    }
                    _ => (c, stand_in_font.clone(), self.res.text_width(&c.to_string(), &stand_in_font, size)),
                };
                let (mut ascent, mut line) = self.line_metrics(name, &stand_in_font, size);
                if is_hangul(c) && line < size * HANGUL_LINE {
                    let extra = size * HANGUL_LINE - line;
                    ascent += extra * HANGUL_ASCENT_SHARE;
                    line += extra;
                }
                out.push(Unit::Char { c: drawn, width, inset: 0.0, font, size, style: LABEL, line, plain_line: line, ascent, lead: self.lead_in(p, name) * size, auto_space: 0.0, squeeze: 1.0, squeezed: false, metric: size });
                positions.push((0, false));
            }
            let suffix = self.doc.styles.numbering.level(list.num_id, list.level).map(|l| l.suffix).unwrap_or_default();
            match suffix {
                ListSuffix::Tab => {
                    out.push(Unit::ListTab);
                    positions.push((0, false));
                }
                ListSuffix::Space => {
                    let font = self.font(style, style.font_for(' '));
                    let width = self.res.text_width(" ", &font, size);
                    let (ascent, line) = self.line_metrics(style.font_for(' '), &font, size);
                    out.push(Unit::Char { c: ' ', width, inset: 0.0, font, size, style: LABEL, line, plain_line: line, ascent, lead: self.lead_em(style.font_for(' ')) * size, auto_space: 0.0, squeeze: 1.0, squeezed: false, metric: size });
                    positions.push((0, false));
                }
                ListSuffix::Nothing => {}
            }
        }
        if let Some(sub) = &p.subdoc {
            let style = &p.mark;
            let size = (style.size as f32 / 100.0).max(9.0);
            let lock_str = if sub.locked { " [ロック]" } else { "" }; // i18n-ok
            let expand_str = if sub.expanded && !sub.content_paragraphs.is_empty() {
                format!(" ({})", sub.content_paragraphs.join(" / "))
            } else {
                String::new()
            };
            let badge = if p.runs.is_empty() {
                format!("[サブ文書: {}{}{}]", sub.target, lock_str, expand_str) // i18n-ok
            } else {
                format!("[サブ文書: {}{}] ", sub.target, lock_str) // i18n-ok
            };
            for c in badge.chars() {
                let name = style.font_for(c);
                let font = self.font(style, name);
                let width = self.res.text_width(&c.to_string(), &font, size);
                let (ascent, line) = self.line_metrics(name, &font, size);
                out.push(Unit::Char { c, width, inset: 0.0, font, size, style: LABEL, line, plain_line: line, ascent, lead: self.lead_in(p, name) * size, auto_space: 0.0, squeeze: 1.0, squeezed: false, metric: size });
                positions.push((0, false));
            }
        }
        // The paragraph's text and where each run's begins in it (the neighbours of a space may be in another run).
        let para_chars: Vec<char> = p.runs.iter().filter_map(|r| if let FlowRunKind::Text(t) = &r.kind { Some(t.chars()) } else { None }).flatten().collect();
        let run_offsets: Vec<usize> = p.runs.iter().scan(0usize, |sum, r| { let start = *sum; if let FlowRunKind::Text(t) = &r.kind { *sum += t.chars().count(); } Some(start) }).collect();
        for (index, run) in p.runs.iter().enumerate() {
            let before = out.len();
            let style = &run.style;
            let size = style.size as f32 / 100.0; // (a superscript or subscript is scaled below, `script_size`, once its typeface is known)
            let half_points = (style.size + 25) / 50;
            let push_text = |text: &str, out: &mut Vec<Unit>| {
                let all: Vec<char> = text.chars().collect();
                // Text after a tab, a line break, a picture or the start of the paragraph has no character before it to carry a typeface from.
                let no_char_before = !matches!(out.last(), Some(Unit::Char { style, .. }) if *style != LABEL);
                if no_char_before {
                    carry.borrow_mut().reset();
                }
                for (at, &c) in all.iter().enumerate() {
                    // The optional hyphen of `w:softHyphen` is U+00AD to the rest of this layout. A U+00AD written in `w:t` is no optional hyphen: Word draws it as a hyphen
                    // of the typeface in its own cell (its PDFs of `fullwidth-symbols-all.docx`, #M125 and #M136: 5.28 pt of ＭＳ 明朝's 10.56 pt text, 7.5 pt of 游明朝's, in a line and
                    // in a vertical column, where the characters after it move), so it is laid out and drawn as the hyphen-minus.
                    let c = match c {
                        ibis_doc_model::OPTIONAL_HYPHEN => '\u{ad}',
                        '\u{ad}' => '-',
                        _ => c,
                    };
                    let char_style_size = style.size_for(c);
                    let (base_size, char_half_points) = if char_style_size == style.size {
                        (size, half_points)
                    } else {
                        let s = char_style_size as f32 / 100.0;
                        let hp = (char_style_size + 25) / 50;
                        (s, hp)
                    };
                    let (c, char_size) = if style.caps {
                        (c.to_uppercase().next().unwrap_or(c), base_size)
                    } else if style.small_caps && c.is_lowercase() {
                        let small_hp = (char_half_points * 8 + 5) / 10;
                        let small_sz = (small_hp * 50) as f32 / 100.0;
                        (c.to_uppercase().next().unwrap_or(c), small_sz)
                    } else {
                        (c, base_size)
                    };
                    // The typeface of the character's font slot: by its Unicode range and the run's `w:hint` only, not by the paragraph's text or the neighbours (Word's PDFs, #M76).
                    // The characters of an emoji cluster after its first take no part in the carrying over (they are drawn in the emoji typeface, with it).
                    let (cluster_start, cluster_end) = crate::render::rich_text::cluster_bounds(&all, at);
                    let in_cluster = at > cluster_start;
                    let name = if in_cluster {
                        style.font_for(c)
                    } else {
                        let (name, carried) = self.typeface_for_character_in_line(style, style.font_for(c), c, char_style_size as f32 / 100.0, &mut carry.borrow_mut(), line_starts.contains(&out.len()));
                        if carried {
                            carried_units.borrow_mut().push(out.len());
                        }
                        name
                    };
                    // The stand-in for ＭＳ 明朝 / ＭＳ ゴシック (Hiragino Mincho ProN / Hiragino Sans) has half-width
                    // (0.5 em) ASCII glyphs under its OpenType `hwid` (`Half Widths`) feature, matching Word's 0.5 em
                    // cell without overflowing into the neighbour (`w2_ms_halfwidth_latin_drawn_half_em`).
                    let installed = name.is_some_and(|n| self.res.has_typeface(n));
                    // A superscript or subscript is a size of the typeface it is in (`script_size`).
                    let (base_size, char_size) = if style.position == VerticalPosition::Baseline {
                        (base_size, char_size)
                    } else {
                        let scaled = self.script_size(name, char_style_size as f32 / 100.0, style.position);
                        (scaled, char_size * scaled / base_size.max(0.01))
                    };
                    let mut font = self.font_for_size(style, name, char_style_size);
                    // An emoji (a pictograph, or a character with the emoji selector, or a joined sequence) is drawn in the OS's emoji
                    // typeface whatever the run's font (Word forces Segoe UI Emoji); the selectors and joined characters of a cluster have its
                    // font too, so that the cluster is one item to shape into one glyph, and take no room (#66, #79).
                    let emoji = crate::render::rich_text::is_emoji_cluster(&all[cluster_start..cluster_end]);
                    if emoji {
                        font = crate::render::rich_text::emoji_font(&font);
                    }
                    // A character that is an emoji by default, asked for in its text presentation (U+FE0E), is not drawn in the colour emoji typeface. Word's PDFs of 2026-11-26
                    // (17 characters of U+2300..2BFF, in 游明朝 and in a run with no typeface; tasks/doc/reports/2026-11-26-k0066-fe0e-typefaces.md): the run's typeface where it has the
                    // glyph (⚽ in 游明朝), else the first of Cambria Math and Segoe UI Symbol that has it (⌚ ⌛ ◽ in Cambria Math, the rest in Segoe UI Symbol, which has all of them).
                    if !emoji && crate::render::rich_text::asks_text_presentation(&all[cluster_start..cluster_end]) {
                        let first = all[cluster_start];
                        let own = crate::render::font_metrics::office_has_glyph(name.unwrap_or(DEFAULT_TYPEFACE), first).unwrap_or(false);
                        let text = ["Cambria Math", "Segoe UI Symbol"].into_iter().find(|t| self.res.has_typeface(t) && crate::render::font_metrics::office_has_glyph(t, first) == Some(true));
                        if let (false, Some(text)) = (own, text) {
                            font.family = text.into();
                        }
                    }
                    // A combining mark ＭＳ 明朝 / ＭＳ ゴシック have a glyph for (U+0300..036F, 0.5 em in the font file) is a character of its own in Word: it takes the font's half
                    // em like any other character, one to a cell, where the zero-advance marks of an OpenType font join their base (Word's PDF of `fullwidth-symbols-all.docx`, #M125:
                    // after έ the 24 marks of JIS X 0213 row 11 each advance 5.28 pt of the 10.56 pt text, in a horizontal and a vertical line).
                    let own_cell_mark = in_cluster && !emoji && name.is_some_and(fixed_width) && crate::render::rich_text::is_font_spaced_mark(c) && !crate::render::rich_text::mark_composes(&all[cluster_start..=at]);
                    // The ideographic tone marks U+302A..302D (Mn) have no glyph in ＭＳ 明朝 and no advance of their own wherever they stand, with a base before them or
                    // none (Word's PDF of `m138-nobase-probe.docx`, #M138: 0 pt after 亜 and at a paragraph's start, in a line and in a vertical column). The Hangul tone marks
                    // U+302E and U+302F (Mc, spacing marks) are characters of their own, one em in a vertical column and after 亜 as well (a mark with an advance does not join
                    // its base). Word's horizontal advance for those two is 1.24 em (13.10 pt of 10.56), which no font or Unicode property tells: this layout sets them one em.
                    let fixed = name.is_some_and(fixed_width) && !emoji;
                    let zero_tone_mark = fixed && matches!(c as u32, 0x302A..=0x302D);
                    let own_cell_mark = own_cell_mark || (in_cluster && fixed && matches!(c as u32, 0x302E | 0x302F));
                    // Marks with no character before them at the start of a vertical line (the start of the paragraph, after a line break, a tab) share one em cell of the
                    // column when a symbol that stays upright and is not full width follows them (`vertical_text::baseless_marks_share_a_cell`).
                    let baseless_run = self.vertical.get() && no_char_before && fixed && crate::render::rich_text::is_font_spaced_mark(all[0]) && {
                        let marks = all.iter().take_while(|m| crate::render::rich_text::is_font_spaced_mark(**m)).count();
                        at < marks && all.get(marks).is_some_and(|n| crate::render::vertical_text::baseless_marks_share_a_cell(*n))
                    };
                    if !installed && name.is_some_and(fixed_width) && c.is_ascii() && !c.is_whitespace() {
                        let mut features: Vec<(String, u32)> = font.features.0.iter().cloned().collect();
                        features.push(("hwid".into(), 1));
                        font.features = FontFeatures(Arc::new(features));
                    }
                    let mut actual = self.res.text_width(&c.to_string(), &font, char_size);
                                        if matches!(c, '☐' | '☑' | '☒') && actual < char_size * 0.85 {
                        let box_w = self.res.text_width("□", &font, char_size);
                        if box_w >= char_size * 0.9 {
                            actual = box_w;
                        }
                    }
                    // A Latin letter's advance is its glyph's own width at the size, not rounded to a pixel: Word's PDFs of
                    // `glyph-advance-ascii-probe.docx` (Times New Roman and Century, 9 / 10 / 10.5 / 11 / 12 pt, the 93 printable ASCII letters,
                    // 930 pairs) have the advances within 0.1 px (rms) of hmtx x size and 0.3 px of the same rounded to a 600 dpi pixel (GDI's width,
                    // which this layout used before, `281e5d53b`); on the 150 corpus documents the pages and the lines that match Word's are the same either way.
                    // An emoji takes its own glyph's advance in the emoji typeface, not the cell of the run's font.
                    let (cell, inset) = match name.filter(|_| !emoji) {
                        Some(n) if proportional_ms(n) => {
                            // The installed font's own advance; a stand-in's (the font is not installed) is set to the advance the
                            // font has (`proportional_ms_width`).
                            // `installed` is whether the typeface is installed under the name the file gives (its Japanese name or its English one,
                            // `has_typeface`): the drawn family is named one or the other by the OS, so it is not compared with the file's name
                            // (GitHub issue #56).
                            let cell = if installed { actual } else { proportional_ms_width(n, c, char_size) };
                            (cell, 0.0)
                        }
                        // Meiryo and Meiryo UI (the typeface not installed): the advance the font file has (`meiryo_advance_em`; Word's XPS agree with it, #54), the glyph of the stand-in
                        // drawn as it is.
                        Some(n) if !installed && crate::render::font_metrics::meiryo_face(n).is_some() => {
                            let ui = crate::render::font_metrics::meiryo_face(n).unwrap_or(false);
                            let cell = crate::render::font_metrics::meiryo_advance_em(ui, style.bold, c).map_or(actual, |em| em * char_size);
                            (cell, 0.0)
                        }
                        // Century (installed or not: the Mac's Century.ttf of Office has other advances than Windows', "TOE" at 10 pt 19.44 pt for 21.70): the advance Windows' font file has (`century_advance_em`; Word's PDF agrees with it within 0.1%, #54), for the ASCII letters,
                        // digits and signs (the `w:hint="eastAsia"` keeps them in Century too), the glyph of the stand-in drawn as it is.
                        Some(n) if n.trim() == "Century" && c.is_ascii() && c != ' ' && crate::render::font_metrics::century_advance_em(c).is_some() => {
                            (crate::render::font_metrics::century_advance_em(c).map_or(actual, |em| em * char_size), 0.0)
                        }
                        Some(n) if fixed_width(n) => {
                            let half = char_size / 2.0;
                            let cell = if let Some(em) = fixed_space_em(c) { char_size * em } else if fixed_full_width(c) { char_size } else { half }; // the narrow no-break space: `advance_em`
                            let inset = if c.is_whitespace() || own_cell_mark { 0.0 } else { ((cell - actual) / 2.0).max(0.0) };
                            (cell, inset)
                        }
                        // A space beside East Asian text is half an em, and so is one that is not alone between two Latin characters: one of two or more in a
                        // row, one that begins or ends the paragraph. A single space between two Latin words keeps its width (Word's PDFs of an English text in
                        // a Japanese document, and of `spaces-count-matrix-century.docx`: 1 space between "A" and "X" is Century's own, 2 to 6 are 5.25 pt
                        // each, one at a paragraph's start or end is 5.25 pt, whatever `w:hint` and the Latin font). Spaces of neighbouring runs count together.
                        _ if c == ' ' && self.doc.balance_single_double_byte_width && {
                            // The text the space is in: the paragraph's (the neighbours may be in another run), or its own (a list mark, which is no run of the paragraph).
                            let (chars, at_all): (&[char], usize) = match run_offsets.get(index) {
                                Some(base) if p.runs.get(index).is_some_and(|r| matches!(&r.kind, FlowRunKind::Text(_))) && para_chars.get(base + at) == Some(&' ') => (&para_chars, base + at),
                                _ => (&all, at),
                            };
                            let mut first = at_all;
                            while first > 0 && chars[first - 1] == ' ' {
                                first -= 1;
                            }
                            let mut last = at_all;
                            while last + 1 < chars.len() && chars[last + 1] == ' ' {
                                last += 1;
                            }
                            let before = first.checked_sub(1).map(|i| chars[i]);
                            let after = chars.get(last + 1).copied();
                            last > first || before.is_none() || after.is_none() || before.is_some_and(is_cjk) || after.is_some_and(is_cjk)
                        } => (char_size / 2.0, 0.0),
                        // (A lone space between Latin letters of Century is Windows' Century's own.)
                        Some(n) if c == ' ' && n.trim() == "Century" => (crate::render::font_metrics::century_advance_em(c).map_or(actual, |em| em * char_size), 0.0),
                        _ => (actual, 0.0),
                    };
                    // U+2002..U+2006 take the advance of the typeface's EN DASH / EM DASH (`dash_space_em`), in an installed typeface and in a stand-in alike.
                    let cell = if emoji || name.is_some_and(fixed_width) {
                        cell
                    } else {
                        let meiryo = match name {
                            Some(n) if !installed => crate::render::font_metrics::meiryo_face(n),
                            _ => None,
                        };
                        let dash = |d: char| -> f32 {
                            match name {
                                Some(n) if proportional_ms(n) && !installed => proportional_ms_width(n, d, char_size),
                                _ => match meiryo {
                                    Some(ui) => crate::render::font_metrics::meiryo_advance_em(ui, style.bold, d).map_or_else(|| self.res.text_width(&d.to_string(), &font, char_size), |em| em * char_size),
                                    None => self.res.text_width(&d.to_string(), &font, char_size),
                                },
                            }
                        };
                        // U+202F and U+205F are Times New Roman's and Cambria Math's whatever the typeface (no typeface of Word's has them); 游明朝 and 游ゴシック lack the others too.
                        match (name.and_then(|n| yu_space_fallback_em(n, c)), c) {
                            (Some(em), _) => em * char_size,
                            (None, '\u{202f}' | '\u{205f}') => fixed_space_em(c).map_or(cell, |em| em * char_size),
                            _ => dash_space_em(c, dash).unwrap_or(cell),
                        }
                    };
                    // A comment's mark (U+200B) takes no room.
                    // The character grid's pitch (文字数と行数を指定): a full-width character takes the grid's added pitch (Word's PDFs keep the Latin letters of a proportional font as they are, and give those of a fixed-width one (ＭＳ 明朝, ＭＳ ゴシック) the added pitch too: a half-width 8 pt letter 4.0 → 3.0 pt, and the characters in a paragraph on the grid).
                    // A run's character spacing (`w:spacing`) counts twice in a paragraph on the character grid (Word's PDF of
                    // gridspacing-halfwidth-probe.docx: +0.4 / -0.4 / -1.0 pt moves every character, Latin, digit, space or kana, in a fixed-width
                    // or proportional face, by +0.8 / -0.8 / -2.0 pt on charSpace +1.624 and -1.0 pt).
                    let grid_spacing_factor = self.spacing_factor(p, c);
                    let grid_added = if !p.snap_to_grid || self.snap_chars.get() {
                        0.0
                    } else {
                        self.char_space.get().map_or(0.0, |cs| {
                            if is_cjk(c) {
                                pt(cs)
                            } else if name.is_some_and(fixed_width) {
                                // A half-width character of a fixed-width font sits in half a grid cell, so it takes half of the added pitch
                                // (Word's PDFs: ＭＳ 明朝 8 pt on charSpace +0.503 pt, the digits, brackets and spaces advance 4.25 pt = 4.0 + 0.25
                                // (103); 11 pt on charSpace -1.0 pt, each space 5.02 pt = 5.52 - 0.5 (121)).
                                if fixed_full_width(c) { pt(cs) } else { pt(cs) / 2.0 }
                            } else if name.is_some_and(|n| n.trim() == "Century") && !self.snap_chars.get() {
                                // Century's letters and spaces take the added pitch on a `linesAndChars` grid, whole, or half when the file's compatibility settings have
                                // `balanceSingleByteDoubleByteWidth`; none on a grid of `snapToChars` (Windows Word's PDFs, 576 paragraphs: `adv = hmtx + k * charSpace / 4096`,
                                // `tasks/doc/requests/2026-11-18-century-grid-result.md` 1 and 2; corpus 127 has the flag).
                                if self.doc.balance_single_double_byte_width { pt(cs) / 2.0 } else { pt(cs) }
                            } else {
                                0.0
                            }
                        })
                    };
                    // A character that stands in vertical text takes a whole em, except the few that go by the face's own width (`upright_keeps_face_width`).
                    let cell=if self.vertical.get() && crate::render::vertical_text::upright(c) && !upright_keeps_face_width(name, c) {char_size}else{cell};
                    let cell = if baseless_run && at == 0 { char_size } else { cell };
                    let own_cell_mark = own_cell_mark && !baseless_run;
                    let width = if c == '\u{200b}' || c == '\u{ad}' || zero_tone_mark || (in_cluster && !own_cell_mark) { 0.0 } else { cell * style.scale as f32 / 100.0 + pt(style.spacing) * grid_spacing_factor + grid_added };
                    // A substitute's glyph wider than the cell Word's font gives it is squeezed into it (it would
                    // overlap its neighbour); a space has no ink to overlap (and a squeezed one is drawn as nothing: GitHub issue #188, U+202F).
                    let fits = if !emoji && !c.is_whitespace() && name.is_some_and(|n| fixed_width(n) || proportional_ms(n)) && actual > cell + 0.05 && cell > 0.0 { cell / actual } else { 1.0 };
                    // A superscript or subscript takes the room of its full size on the line, not of the smaller size it is drawn in (Word's PDFs of 2026-11-26, ＭＳ 明朝 in a
                    // column and in a line: after 9 pt text a superscript of sz 24 makes the line as thick as 12 pt text, 21 as 10.5 pt text, and one of 18 changes nothing, the
                    // same for a subscript: 16.1, 14.1 and 12.1 pt of a column. A footnote of 9 pt text after a 10.5 pt reference mark is 14.1 pt thick, not 12.1, #188).
                    let metric_size = if style.position == VerticalPosition::Baseline { base_size } else { char_style_size as f32 / 100.0 };
                    let (mut ascent, mut line) = self.line_metrics(name, &font, metric_size);
                    // A line with Hangul is 1.72 em high in Word (18.1 pt of 10.5 pt text, DNUM: its fallback font for
                    // Hangul), the text sitting low in it.
                    if is_hangul(c) && line < base_size * HANGUL_LINE {
                        let extra = base_size * HANGUL_LINE - line;
                        ascent += extra * HANGUL_ASCENT_SHARE;
                        line += extra;
                    }
                    // `w:position`: the text is raised (lowered) by half the value in points. In a line that is not of exact height the line grows above the
                    // raised text and below the lowered text by that much (Word's PDFs of GitHub issue #192, `S5a`/`S5b`: +12 pt above and +12 pt below for 12 pt);
                    // in an exact line the height stays and only the text moves (`S5h`/`S5i`).
                    // Word's rule (its XPS of GitHub issue #192): a run's room above the baseline is `A + r` and below `D - r` (`A` and `D` the run's own, `r` the raise in
                    // points, negative when lowered), neither under 0; the line takes the largest of each. `A` here is the ascent with half the room the line leaves beyond
                    // the text (`ms_room`, which `text_dy` puts above it), `D` the rest.
                    // The line the run has with no raise or lowering: a multiple under 1 shrinks the line by that (`multiple_shrink`).
                    let plain_line = line;
                    if style.raise != 0 && !matches!(p.line, LineHeight::Exact(_)) {
                        let r = style.raise as f32 / 2.0;
                        let half = self.ms_room(name, base_size) / 2.0;
                        let (above, below) = ((ascent + half + r).max(0.0), (line - ascent - half - r).max(0.0));
                        ascent = above - half;
                        line = above + below;
                    }
                    // A border across a line (`w:bdr`) takes its room `W` (its whole width and its distance from the text, the room it takes at the ends of the characters) above
                    // and below the run as well: the line is `W` higher above and below (Word's PDFs of GitHub issue #192, ＭＳ 明朝 10.5 pt: the room above the text and the next
                    // paragraph's below it grow by 0.62 and 0.36 for a 0.5 pt line, 4.44 and 4.56 at 4 pt apart, 2.88 and 3.15 for 3 pt, 1.44 and 1.56 for a double line; 游ゴシック 0.45 and 0.50).
                    if let Some(b) = style.border.as_ref().filter(|_| !self.vertical.get()) {
                        let w = pt(b.total()) + pt(b.space);
                        ascent += w;
                        line += 2.0 * w;
                    }
                    if let Some(b) = style.border.as_ref().filter(|_| self.vertical.get()) {
                        let pad_v = pt(b.space);
                        let extra_bw = (pt(b.total()) - pt(b.width)).max(0.0);
                        ascent += 1.2 + pad_v;
                        line += 1.2 + 2.8 + 2.0 * pad_v + 2.0 * extra_bw;
                    }
                    // An optional hyphen (`w:softHyphen`) takes no room and is not shown, unless a line ends after it: then it is a hyphen. Its `auto_space` is the
                    // hyphen's width, negative: taking the gap off the width at the end of a line (`end_of_line`) adds it.
                    let auto_space = if c == '\u{00ad}' { -self.res.text_width("-", &font, char_size) } else { 0.0 };
                    out.push(Unit::Char { c, width, inset, font, size: char_size, style: index, line, plain_line, ascent, lead: self.lead_in(p, name) * metric_size, auto_space, squeeze: style.scale as f32 / 100.0 * fits, squeezed: false, metric: metric_size });
                }
            };
            // Tracked changes visibility: All shows both; Simple and None hide deleted text; Original hides inserted text.
            let show_hidden = if self.res.print {
                self.res.print_hidden_text
            } else {
                self.res.markup_view.show_hidden_text
            };
            let rev_hidden = match run.revision.as_ref() {
                Some(r) if self.res.markup_view.is_reviewer_shown(&r.author) => match self.markup_mode() {
                    MarkupDisplayMode::All => false,
                    MarkupDisplayMode::Simple | MarkupDisplayMode::None => r.kind.is_delete(),
                    MarkupDisplayMode::Original => r.kind.is_insert(),
                },
                Some(r) => {
                    // 非表示校閲者の変更は確定テキスト（挿入は表示、削除は非表示）として扱う
                    r.kind.is_delete()
                }
                None => false,
            };
            let hidden = (run.style.hidden && !show_hidden) || rev_hidden;
            let show_code = run.is_field_code_shown(self.doc.show_field_codes);
            match &run.kind {
                FlowRunKind::Text(t) if hidden => push_text(&"\u{200b}".repeat(t.chars().count()), &mut out),
                FlowRunKind::Text(t) => push_text(t, &mut out),
                FlowRunKind::CheckBox { checked, checked_char, unchecked_char } => {
                    let ch = if *checked { *checked_char } else { *unchecked_char };
                    push_text(&ch.to_string(), &mut out);
                }
                FlowRunKind::DropDownList { items, selected, .. } => {
                    let text = items.get(*selected).map(|(d, _)| d.as_str()).unwrap_or("");
                    if hidden {
                        push_text(&"\u{200b}".repeat(text.chars().count()), &mut out);
                    } else {
                        push_text(text, &mut out);
                    }
                }
                FlowRunKind::DatePicker { text, .. }
                | FlowRunKind::RichText { text, .. }
                | FlowRunKind::PlainText { text, .. }
                | FlowRunKind::BuildingBlockGallery { text, .. }
                | FlowRunKind::Group { text, .. } => {
                    if hidden {
                        push_text(&"\u{200b}".repeat(text.chars().count()), &mut out);
                    } else {
                        push_text(text, &mut out);
                    }
                }
                FlowRunKind::RepeatingSection { items, selected, .. } => {
                    let text = items.get(*selected).map(|s| s.as_str()).unwrap_or("");
                    if hidden {
                        push_text(&"\u{200b}".repeat(text.chars().count()), &mut out);
                    } else {
                        push_text(text, &mut out);
                    }
                }
                FlowRunKind::FormField(field) => match &field.kind {
                    ibis_doc_model::FormFieldKind::TextInput { value, default, .. } => {
                        let text = if value.is_empty() {
                            if !default.is_empty() {
                                default.as_str()
                            } else {
                                "\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}"
                            }
                        } else {
                            value.as_str()
                        };
                        if hidden {
                            push_text(&"\u{200b}".repeat(text.chars().count()), &mut out);
                        } else {
                            push_text(text, &mut out);
                        }
                    }
                    ibis_doc_model::FormFieldKind::CheckBox { checked, .. } => {
                        let ch = if *checked { '☒' } else { '☐' };
                        push_text(&ch.to_string(), &mut out);
                    }
                    ibis_doc_model::FormFieldKind::DropDown { items, selected, .. } => {
                        let text = items.get(*selected).map(|s| s.as_str()).unwrap_or("");
                        if hidden {
                            push_text(&"\u{200b}".repeat(text.chars().count()), &mut out);
                        } else {
                            push_text(text, &mut out);
                        }
                    }
                },
                FlowRunKind::PageNumber if show_code => push_text("{ PAGE }", &mut out),
                FlowRunKind::PageNumber => {
                    let text = match &*self.page_format.borrow() {
                        Some(f) => ibis_doc_model::format_page_number(page_number, f),
                        None => page_number.to_string(),
                    };
                    push_text(&format!("{}{text}", self.chapter_prefix.borrow()), &mut out)
                }
                FlowRunKind::FileName(_) if show_code => push_text("{ FILENAME }", &mut out),
                FlowRunKind::FileName(saved) => push_text(self.doc.file_name.as_deref().unwrap_or(saved), &mut out),
                FlowRunKind::Field { instruction, .. } if show_code => {
                    let code = format!("{{ {} }}", instruction.trim());
                    push_text(&code, &mut out);
                }
                FlowRunKind::Field { instruction, shown } => match self.shown_field_text(instruction, shown, page_number) {
                    Some(text) => push_text(&text, &mut out),
                    // 囲い文字: its enclosure takes the room (the character
                    // is drawn over it).
                    None => match ibis_doc_model::field_format::enclosed(instruction) {
                        Some((enclosure, _)) => {
                            let ring = enclosure.chars().next().unwrap_or('○');
                            match style.enclosure_sizes {
                                // The enclosure drawn at the size its own run of the code has.
                                Some((ring_size, _)) => {
                                    let ring_pt = ring_size as f32 / 100.0;
                                    let name = style.font_for(ring);
                                    let font = self.font_for_size(style, name, ring_size);
                                    let width = typeface_room(name, ring, ring_pt, self.res.text_width(&ring.to_string(), &font, ring_pt));
                                    let (ascent, line) = self.line_metrics(name, &font, ring_pt);
                                    out.push(Unit::Char { c: ring, width, inset: 0.0, font, size: ring_pt, style: index, line, plain_line: line, ascent, lead: self.lead_in(p, name) * ring_pt, auto_space: 0.0, squeeze: 1.0, squeezed: false, metric: ring_pt });
                                }
                                None => push_text(&ring.to_string(), &mut out),
                            }
                        }
                        None => match ibis_doc_model::field_format::overstrike(instruction) {
                            // 重ね書き (EQ \o\ad, \o\al): room as wide as the second
                            // argument (spread) or the widest one, left blank (the
                            // arguments are drawn over it).
                            // A stack with no switch: one cell as wide as the widest argument at its own size (the sizes the runs of the code give).
                            Some((ibis_doc_model::field_format::OverstrikeAlign::Center, args)) => {
                                let before = out.len();
                                push_text("\u{3000}", &mut out);
                                let room = self.overstrike_widths(style, &args, size).into_iter().fold(0.0f32, f32::max);
                                if let Some(Unit::Char { width, inset, .. }) = out.get_mut(before) {
                                    *width = room;
                                    *inset = 0.0;
                                }
                            }
                            Some((align, args)) => push_text(&self.overstrike_room(align, &args), &mut out),
                            // An EQ field of fractions, roots, scripts…: typeset as a formula.
                            None => match ibis_doc_model::eq_field::eq_math(instruction) {
                                Some(math) => out.push(self.math_unit(&math, false, style, size)),
                                None => push_text(shown, &mut out),
                            },
                        },
                    },
                },
                FlowRunKind::PageRef { bookmark, .. } if show_code => {
                    let code = format!("{{ PAGEREF {} }}", bookmark);
                    push_text(&code, &mut out);
                }
                FlowRunKind::PageRef { shown, .. } => push_text(shown, &mut out),
                FlowRunKind::Ruby { base, .. } => push_text(base, &mut out),
                FlowRunKind::NoteRef { endnote, id } => {
                    let n = self.note_numbers.get(&(*endnote, *id)).cloned().unwrap_or_default();
                    push_text(&n, &mut out)
                }
                FlowRunKind::NoteMark => {
                    let n = self.current_note.borrow().clone();
                    push_text(&n, &mut out)
                }
                // A comment's marks take a position but no room.
                FlowRunKind::CommentStart(_) | FlowRunKind::CommentEnd(_) | FlowRunKind::CommentRef(_) => push_text("\u{200b}", &mut out),
                FlowRunKind::Tab => out.push(Unit::Tab),
                FlowRunKind::PositionalTab { align, relative_to_margin, leader } => out.push(Unit::PositionalTab { align: *align, relative_to_margin: *relative_to_margin, leader: *leader }),
                FlowRunKind::LineBreak => out.push(Unit::LineBreak),
                // A page break in a table's cell does nothing (Word draws the header row of a table that starts with one as one
                // line: the row of the cell holding it is not a line taller).
                FlowRunKind::PageBreak if self.current_cell.get().is_some() => {}
                FlowRunKind::PageBreak => out.push(Unit::PageBreak),
                FlowRunKind::ColumnBreak => out.push(Unit::ColumnBreak),
                FlowRunKind::Picture { media, size, look } | FlowRunKind::PictureControl { media, size, look, .. } if look.is_visual_default() => out.push(Unit::Picture { media: *media, w: pt(size.width), h: pt(size.height), ee_top: look.effect_extent[1] as f32 / 12700.0, ee_bottom: look.effect_extent[3] as f32 / 12700.0 }),
                // A picture turned, flipped, cut or outlined: a shape (drawn as the pictures of a
                // presentation are) whose room in the line is the box around it turned.
                FlowRunKind::Picture { media, size, look } | FlowRunKind::PictureControl { media, size, look, .. } => {
                    let angle = look.rotation as f32 / 60_000.0 * std::f32::consts::PI / 180.0;
                    let (sin, cos) = (angle.sin().abs(), angle.cos().abs());
                    let (w, h) = (pt(size.width), pt(size.height));
                    let picture = ibis_doc_model::Picture { media: *media, crop: look.crop, stroke: look.stroke.clone(), corner: 0, duotone: look.duotone, clear: look.clear, luminance: look.luminance, clip: None, alt_text: look.alt_text.clone(), decorative: look.decorative, name: look.name.clone(), shadow: look.shadow, reflection: look.reflection.clone(), soft_edge: look.soft_edge, geometry: look.geometry.clone(), effects: look.effects.clone(), tile: None, fill_rect: None, artistic_effect: None };
                    let shape = ibis_doc_model::Shape {
                        id: ibis_doc_model::ShapeId(0),
                        frame: ibis_doc_model::EmuRect::new(0, 0, size.width, size.height),
                        rotation: look.rotation,
                        flip_h: look.flip_h,
                        flip_v: look.flip_v,
                        kind: ibis_doc_model::ShapeKind::Picture(picture),
                        floating: None,
                    };
                    out.push(Unit::Shape { shape: Box::new(shape), w: w * cos + h * sin, h: w * sin + h * cos });
                }
                FlowRunKind::Math { math, display } => out.push(self.math_unit(math, *display, style, size)),
                FlowRunKind::Shape(shape) => {
                    let is_floating = shape.floating.is_some();
                    let mut shape = shape.clone();
                    if !is_floating {
                        self.fit_unwrapped_text_box(&mut shape);
                    }
                    let (w, h) = if is_floating { (0.0, 0.0) } else { (pt(shape.frame.width), pt(shape.frame.height)) };
                    out.push(Unit::Shape { shape, w, h });
                }
            }
            // 組み文字: the run's room is its two half-size lines' (and
            // its brackets'), on its first character.
            if let Some(c) = style.combine
                && out.len() > before
            {
                let chars: Vec<char> = out[before..].iter().filter_map(unit_char).collect();
                let total = self.combine_width(style, &chars, size, c);
                for (k, u) in out[before..].iter_mut().enumerate() {
                    if let Unit::Char { width, inset, .. } = u {
                        *width = if k == 0 { total } else { 0.0 };
                        *inset = 0.0;
                    }
                }
            }
            // A ruby longer than its base: the box is the ruby's width and the base stands in the middle of it (Word's PDFs of GitHub issue #180: a base of 1
            // character (10.5 pt) under a ruby of 4 characters (5 pt each) is a box of 20 pt, the base 4.7 pt from its left, the text after it 20 pt on). The
            // ruby is drawn over the box from its left (`ruby_items`), so the room is shared by the first and the last character of the base.
            // A ruby at the base's right (`rubyAlign="rightVertical"`, across a line): the base's characters are 0.9 em apart and the column of the ruby's stacked characters
            // (`right_vertical_column`) follows the last (Word's XPS, many-patterns-3: the base's advance 0.8955 to 0.9000 of its size in 6 typefaces at 8 to 24 pt).
            if let FlowRunKind::Ruby { size: ruby_size, align: ibis_doc_model::RubyAlign::RightVertical, .. } = &run.kind
                && out.len() > before
                && !self.vertical.get()
            {
                let column = self.right_vertical_column(style, *ruby_size);
                for u in out[before..].iter_mut() {
                    if let Unit::Char { width, size, .. } = u {
                        *width -= RIGHT_VERTICAL_BASE_TIGHTENING * *size;
                    }
                }
                if let Some(Unit::Char { width, .. }) = out.last_mut() {
                    *width += column;
                }
            } else if let FlowRunKind::Ruby { text, size: ruby_size, .. } = &run.kind
                && out.len() > before
            {
                let base: f32 = out[before..].iter().map(unit_width).sum();
                let pad = self.ruby_text_width(style, text, *ruby_size) - base;
                if pad > 0.0 {
                    if let Some(Unit::Char { width, inset, .. }) = out.get_mut(before) {
                        *width += pad / 2.0;
                        *inset += pad / 2.0;
                    }
                    if let Some(Unit::Char { width, .. }) = out.last_mut() {
                        *width += pad / 2.0;
                    }
                }
            }
            // 文字の囲み線 (`w:bdr`) takes room across the line: its whole width and its distance from the text, before the run's first character and after its last (the box
            // runs from the end of the text before it to the start of the text after it; Word's PDFs of GitHub issue #192: 0.48 pt for a 0.5 pt line, 4.44 pt for a 0.5 pt line
            // 4 pt from the text, 3.00 pt for a 3 pt line, before and after). Runs in a row with the same border are one box: only its ends take the room.
            if let Some(b) = &style.border
                && out.len() > before
                && !self.vertical.get()
                && style.combine.is_none()
                && !style.tate_chu_yoko
                && out[before..].iter().filter_map(unit_char).any(|c| !c.is_whitespace())
            {
                let room = pt(b.total()) + pt(b.space);
                let continues = |other: Option<&ibis_doc_model::FlowRun>| other.is_some_and(|r| r.style.border.as_ref() == Some(b));
                if !continues(index.checked_sub(1).and_then(|i| p.runs.get(i)))
                    && let Some(Unit::Char { width, inset, .. }) = out[before..].iter_mut().find(|u| matches!(u, Unit::Char { .. }))
                {
                    *width += room;
                    *inset += room;
                }
                if !continues(p.runs.get(index + 1))
                    && let Some(Unit::Char { width, .. }) = out[before..].iter_mut().rev().find(|u| matches!(u, Unit::Char { .. }))
                {
                    *width += room;
                }
            }
            // 縦中横・文字の回転 (`w:eastAsianLayout w:vert`).
            if style.tate_chu_yoko && out.len() > before {
                // The run is one cell down the line: as long as a line of its typeface is high (Word's PDFs of
                // `tate-chu-yoko-*`, ＭＳ ゴシック / ＭＳ 明朝 15.62 pt and 游明朝 20.06 pt at 12 pt, whatever the digits and whether
                // 行の幅に合わせる is on: `tate_chu_yoko_cell`), shared out between the characters so that the caret goes through it one by one.
                let n = (out.len() - before) as f32;
                let first = out[before..].iter().find_map(unit_char);
                let name = first.and_then(|c| style.font_for(c));
                let (cell, across) = match &out[before] {
                    Unit::Char { font, size, .. } => (self.tate_chu_yoko_cell(name, font, *size), self.tate_chu_yoko_across(style, &out[before..])),
                    _ => (size, 0.0),
                };
                for u in out[before..].iter_mut() {
                    if let Unit::Char { width, inset, line, ascent, size, .. } = u {
                        *width = cell / n;
                        *inset = 0.0;
                        // 行を広げる (off): the digits lie across the line, so the line is as thick as they are wide, the characters of the
                        // line in the middle of it (Word's PDFs: the lines before and after the digits move apart by half of what they
                        // are wider than the line, each). 行の幅に合わせる squeezes them to fit the line, so it never widens it.
                        // The line is as thick as the digits are wide, nothing more (Word's 12 pt ＭＳ: 33.96 pt for 3 digits of 18 pt, 15.96 + 18): the
                        // `vertical_pitch_em` that the characters' `lead` adds to a vertical line is part of the 18 pt, not on top of it.
                        let room = self.vertical_pitch_em(p, name) * *size;
                        // (Whether 行の幅に合わせる, `vertCompress`, is on makes no difference: Word draws by `w:w` alone, and a `vertCompress` run with no `w:w` is as wide as one
                        // without it, so its line widens too; Windows Word's XPS of `scripts/word-check/tcy_fit.py`, `tasks/doc/requests/2026-11-18-188-tate-chu-yoko-fit-result.md` 1 and 3.
                        // A run `w:w` squeezed is no wider than the line, and the line stays.)
                        if across - room > *line {
                            *ascent += (across - room - *line) / 2.0;
                            *line = across - room;
                        }
                    }
                }
            }
            let text = matches!(run.kind, FlowRunKind::Text(_));
            for k in 0..out.len() - before {
                if text {
                    positions.push((position + k, true));
                } else {
                    positions.push((position, k == 0));
                }
            }
            position += run.len();
        }
        // Pair kerning (`w:kern`): from that size up (half-points), between
        // two characters of a run in the same font, the first one's width
        // takes the pair's kerning (Word's line breaks in DBDY-00007 follow
        // the kerned widths).
        for i in 1..out.len() {
            let (left, right) = out.split_at_mut(i);
            let (Unit::Char { c: a, width, font, size, style, .. }, Unit::Char { c: b, font: next_font, size: next_size, style: next_style, .. }) = (&mut left[i - 1], &right[0]) else { continue };
            if style != next_style || font != next_font || size != next_size {
                continue;
            }
            let Some(run) = p.runs.get(*style) else { continue };
            // Not a pair with Japanese text: Word's PDFs keep Japanese letters and stops (すc。) at their full width
            // (DIMG: 游ゴシック kerns す。 by 0.115 em, and the text after it ends 1.14 pt early).
            if run.style.kern.is_some_and(|k| *size * 2.0 >= k as f32) && !is_cjk(*a) && !is_cjk(*b) && (*a as u32) < 0x3000 && (*b as u32) < 0x3000 {
                *width += self.res.pair_kerning(font, *size, *a, *b) * run.style.scale as f32 / 100.0;
            }
        }
        // A quarter em between Japanese and Latin letters or digits.
        if p.auto_space_letters || p.auto_space_digits {
            let spaced = |c: char| (p.auto_space_letters && is_western_letter(c)) || (p.auto_space_digits && c.is_ascii_digit());
            let japanese = is_japanese_script;
            for i in 1..out.len() {
                let (Some(a), Some(b)) = (unit_char(&out[i - 1]), unit_char(&out[i])) else { continue };
                // Field delimiters (`w:fldChar` 0x0013/0x0014/0x0015) separate a field's result run
                // from the text around it in Word's character stream, so auto-spacing is not added across the boundary.
                let field_boundary = match (&out[i - 1], &out[i]) {
                    (Unit::Char { style: sa, .. }, Unit::Char { style: sb, .. }) if sa != sb => {
                        p.runs.get(*sa).is_some_and(|r| r.is_field()) || p.runs.get(*sb).is_some_and(|r| r.is_field())
                    }
                    _ => false,
                };
                // Not beside a character of a 組み文字 (Word's PDF of `combine-neighbours-mincho-w41453.docx`: the brackets of a combined run touch the
                // text before and after it, Japanese, digits or Latin letters alike). Inside the run Word puts a quarter em of the half size between a
                // Japanese character and a digit or letter (1.74 pt at 6.96 pt); not done yet: the first character holds the room of the whole run
                // (`combine_width`), the others have none, so a width added to one of them would give it a place to break the run.
                let combined = |u: &Unit| matches!(u, Unit::Char { style, .. } if *style != LABEL && p.runs.get(*style).is_some_and(|r| r.style.combine.is_some()));
                // Nor beside a 縦中横 run (Word's PDFs of `tate-chu-yoko-*.docx`: the kana after the digits are a line's height from the kana before them,
                // no quarter em more on either side, whether the page's `w:autoSpaceDN` is on or not).
                let tate = |u: &Unit| matches!(u, Unit::Char { style, .. } if *style != LABEL && p.runs.get(*style).is_some_and(|r| r.style.tate_chu_yoko));
                // Nor beside a span fitted to a width (`w:fitText`; Word's PDFs of GitHub issue #180: the span is as wide as its width says, 63.0 pt for
                // four Latin letters, and the text after it starts there, with no quarter em before the letters or after them).
                let fitted = |u: &Unit| matches!(u, Unit::Char { style, .. } if *style != LABEL && p.runs.get(*style).is_some_and(|r| r.style.fit_text.is_some()));
                // Nor beside a footnote's or an endnote's reference mark (the mark in the text, and the mark at the start of the note: Word's PDF of `n5-vertical.docx`, #188, the note's mark takes its 3.25 pt and the 9 pt text starts there) in a vertical line (Word's XPS of `vertical-footnote-base.docx`, ＭＳ 明朝 10.5 pt: the 1 after 。 starts where
                // the 。 ends and the kana after it starts where the 1 ends, 3.24 pt later, no quarter em; the mark is one special character of the paragraph, not a digit). Not
                // measured in a line across, which keeps the quarter em.
                let note_mark = |u: &Unit| matches!(u, Unit::Char { style, .. } if *style != LABEL && p.runs.get(*style).is_some_and(|r| matches!(r.kind, FlowRunKind::NoteRef { .. } | FlowRunKind::NoteMark)));
                let in_combined = combined(&out[i - 1]) || combined(&out[i]) || tate(&out[i - 1]) || tate(&out[i]) || fitted(&out[i - 1]) || fitted(&out[i])
                    || (self.vertical.get() && (note_mark(&out[i - 1]) || note_mark(&out[i])));
                // Nor beside a full-width bracket in Meiryo: none before （ or after ） (Windows Word's XPS of Meiryo and Meiryo UI, `1時`, `時1`, `A時` take a quarter em and `（1`, `1）`, `）A`, `A（`
                // none; the 072 of the corpus has `）A` a quarter em shorter than the advances and the quarter em add up to, #54).
                // (Measured in Meiryo and Meiryo UI only: the other typefaces keep the quarter em there until they are measured.)
                let bracket = |u: &Unit| match u {
                    Unit::Char { c, style, .. } if matches!(c, '（' | '）') && *style != LABEL => p.runs.get(*style).and_then(|r| r.style.font_for(*c)).is_some_and(|n| crate::render::font_metrics::meiryo_face(n).is_some()),
                    _ => false,
                };
                if !field_boundary && !in_combined && !bracket(&out[i - 1]) && !bracket(&out[i])
                    && ((japanese(a) && spaced(b)) || (spaced(a) && japanese(b)))
                    && let Unit::Char { width, size, auto_space, style, .. } = &mut out[i - 1]
                {
                    // A quarter of the scaled em (`w:w`): Word's XPS/PDF of a Meiryo 11 pt line at 80% (corpus 016, a cell) puts 2.16 pt between 「1」 and the
                    // kanji, a quarter of the 8.8 pt the character is set at, not of 11 pt (2.75).
                    // Only the run's own `w:w`, not the squeeze that fits a substitute's wider glyph into Word's cell (`fits`, in `squeeze`): that exists only where the typeface is missing.
                    let scale = if *style == LABEL { 1.0 } else { p.runs.get(*style).map_or(1.0, |r| r.style.scale as f32 / 100.0) };
                    let space = *size * scale / 4.0;
                    *width += space;
                    *auto_space = space;
                }
            }
        }
        // A closing bracket or a stop followed by an opening bracket (、「 ）（ 。『) is squeezed: the first one takes half an em
        // (Word does so in DIDX-05 and -07 even with `doNotCompress`).
        for i in 1..out.len() {
            let (Some(a), Some(b)) = (unit_char(&out[i - 1]), unit_char(&out[i])) else { continue };
            // Only where the run's kerning is on (`w:kern`; with 0 Word keeps the full width).
            // Word's PDF of a line with （「 … （…）」 in a left aligned paragraph (compatibility mode 15, `compressPunctuation`): the first of each of the pairs
            // （「 and ）」 is 4.5 pt (half of 9 pt) short of its em, two pairs, 9 pt in all; so an opening bracket before another and a closing one before
            // another are squeezed as the closing one before an opening is. A stop or a closing bracket before another one (、。，． in a row, 。」)
            // is squeezed too: Word's PDF of issue #54's (and #120's) document, compressPunctuation and w:kern 2 in the defaults, sets 、。，．
            // half an em each down its vertical line, where ibisOffice gave them an em (corpus 150 docx: ink 0.8114 -> 0.8118, page counts as before).
            let closing = |ch: char| "、。，．）」』】〕〉》］｝".contains(ch); // i18n-ok（行頭禁則の文字の集合。文字の並び）
            let opening = |ch: char| "「『（【〔〈《［｛".contains(ch); // i18n-ok（行末禁則の文字の集合。文字の並び）
            // ・ ； ： give up their half em before an opening bracket as a closing one does (・「 ・〈 ；「 ：「), and a stop or a closing bracket gives it up before ・
            // (．・ 。・ ）・ 」・ 〉・): Word's PDF of `pair-middle-dot-probe.docx` (ＭＳ 明朝 14 pt, w:kern 2 in the defaults; the first one of the pair 7.1 pt); not ・ before a
            // closing bracket or a stop, ！ ？ before anything, ・・, ー or 々 before a bracket. The vertical page of #54 (stops 、。，．・) has the four stops 6 pt each.
            let middle = |ch: char| "・；：".contains(ch); // i18n-ok（約物の文字の集合。文字の並び）
            // (Not in Meiryo UI: its `）（` is two ems, Word's XPS of `時間）（時間` in it is 66.12 pt, six ems; BIZ UDPゴシック and ＭＳ Ｐ明朝 pair them, 55.08 pt, five.)
            if matches!(&out[i - 1], Unit::Char { style, font, .. } if p.runs.get(*style).is_some_and(|r| r.style.kern.is_some()) && !p.runs.get(*style).and_then(|r| r.style.font_for(a)).unwrap_or(font.family.as_ref()).trim().eq_ignore_ascii_case("Meiryo UI"))
                && (((closing(a) || middle(a)) && opening(b)) || (opening(a) && opening(b)) || (closing(a) && (closing(b) || b == '・')))
                && let Unit::Char { width, size, style, squeezed, .. } = &mut out[i - 1]
                // The em the character is set at: the run's own `w:w` scales it (Word's XPS of 「時間）（時間」 at 80%, Meiryo 11 pt, is 4.4 pt shorter, half of 8.8 pt),
                // not the squeeze that fits a substitute's wider glyph into Word's cell.
                && let em = *size * if *style == LABEL { 1.0 } else { p.runs.get(*style).map_or(1.0, |r| r.style.scale as f32 / 100.0) }
                && *width >= em * 0.99
            {
                *width -= em / 2.0;
                *squeezed = true;
            }
        }
        // A span of characters fitted to a width (`w:fitText`; its runs share
        // an id): wider, the room is between its characters (not after the
        // last); narrower, the glyphs are squeezed. Word sets the span from
        // the characters' natural widths, not the saved spacing and scale.
        let fit = |u: &Unit| -> Option<(u32, u32)> {
            match u {
                Unit::Char { style, .. } if *style != LABEL => p.runs.get(*style).and_then(|r| r.style.fit_text),
                _ => None,
            }
        };
        let mut i = 0;
        while i < out.len() {
            let Some((val, id)) = fit(&out[i]) else {
                i += 1;
                continue;
            };
            let mut j = i + 1;
            while j < out.len() && fit(&out[j]).is_some_and(|(_, other)| other == id) {
                j += 1;
            }
            let natural: Vec<f32> = out[i..j]
                .iter()
                .map(|u| match u {
                    Unit::Char { c, width, style, .. } => {
                        let st = &p.runs[*style].style;
                        ((*width - pt(st.spacing) * self.spacing_factor(p, *c)) / (st.scale as f32 / 100.0).max(0.01)).max(0.0)
                    }
                    _ => 0.0,
                })
                .collect();
            let total: f32 = natural.iter().sum();
            let goal = val as f32 / 20.0;
            if total > 0.0 && goal > 0.0 {
                let n = natural.len();
                let (gap, k) = if goal >= total { (if n > 1 { (goal - total) / (n - 1) as f32 } else { 0.0 }, 1.0) } else { (0.0, goal / total) };
                for (m, u) in out[i..j].iter_mut().enumerate() {
                    if let Unit::Char { width, squeeze, .. } = u {
                        *width = natural[m] * k + if m + 1 < n { gap } else { 0.0 };
                        *squeeze = k;
                    }
                }
            }
            i = j;
        }
        self.snap_to_cells(p, &mut out);
        (out, positions, carried_units.into_inner())
    }

    /// A section of `snapToChars` sets each character on the grid's cells (Windows Word's PDFs, ＭＳ 明朝, 2026-11-26: 16 documents of 4 grids x 4 sizes, 61 widths each; the
    /// pitch `P` is the Normal style's size plus the grid's added pitch): a character takes `ceil(advance / P)` cells, `advance` its own width without the added pitch, and stands
    /// in the middle of them (12 pt kanji on a 10.5 pt grid: 21 pt apart, 4.5 pt in; on a 9 pt grid 18 pt; 9 pt kanji on a 12 pt grid 12 pt). A run of Latin letters, digits and
    /// spaces is one thing: it takes `ceil(width / P)` cells and stands in the middle of them (`ABC` of 18 pt on 10.5 pt cells: 21 pt, the letters 1.5 pt in; ` 12345 `, 42 pt,
    /// exactly 4 cells); the quarter em of `autoSpaceDE` and `autoSpaceDN` is not added; a half-width kana takes a cell by itself. A line is as many cells as `W / P` holds.
    fn snap_to_cells(&self, p: &FlowParagraph, out: &mut [Unit]) {
        if !self.snap_chars.get() || !p.snap_to_grid {
            return;
        }
        let base = self.doc.styles.paragraph_base(None, None).1.size.unwrap_or(1000) as f32 / 100.0;
        let pitch = base + self.char_space.get().map_or(0.0, pt);
        if pitch < 1.0 {
            return;
        }
        let latin = |u: &Unit| matches!(u, Unit::Char { c, style, .. } if *style != LABEL && (c.is_ascii() || is_western_letter(*c)));
        let mut i = 0;
        while i < out.len() {
            let Unit::Char { style, .. } = &out[i] else {
                i += 1;
                continue;
            };
            if *style == LABEL {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            if latin(&out[i]) && !self.vertical.get() {
                while j < out.len() && latin(&out[j]) {
                    j += 1;
                }
            }
            let natural: f32 = out[i..j].iter().map(|u| if let Unit::Char { width, auto_space, .. } = u { *width - *auto_space } else { 0.0 }).sum();
            let cells = (natural / pitch - 0.01).ceil().max(1.0);
            let pad = cells * pitch - natural;
            if let Unit::Char { width, inset, auto_space, .. } = &mut out[i] {
                *width += pad / 2.0 - *auto_space;
                *inset += pad / 2.0;
                *auto_space = 0.0;
            }
            for u in out[i + 1..j].iter_mut() {
                if let Unit::Char { width, auto_space, .. } = u {
                    *width -= *auto_space;
                    *auto_space = 0.0;
                }
            }
            if let Unit::Char { width, .. } = &mut out[j - 1] {
                *width += pad / 2.0;
            }
            i = j;
        }
    }

    /// The characters of the lines after the first, drawn again as Word draws them (#M102): a line's first character takes the typeface it would have standing
    /// apart, and the carrying over starts again from it. `units` were laid out with the typefaces carried over along the runs and the lines were broken by their
    /// widths; the lines are not broken again by the widths of the new typefaces (Word's lines are the ones of the carried widths: a character that is wider
    /// now stands past the line's end, as it does in Word's PDFs). `starts` are the first units of the lines.
    fn redraw_lines_after_the_first(&self, p: &FlowParagraph, page_number: u32, width: f32, units: &mut [Unit], starts: &[usize]) {
        let cuts: Vec<usize> = starts.iter().copied().filter(|&s| s > 0).collect();
        if cuts.is_empty() {
            return;
        }
        self.line_room.set(width - pt(p.left) - pt(p.right));
        let (fresh, _, _) = self.units_cut(p, page_number, &cuts);
        self.line_room.set(f32::MAX);
        // (A drop cap took a unit off: the indices do not agree, and nothing is redrawn.)
        if fresh.len() != units.len() {
            return;
        }
        for (unit, new) in units.iter_mut().zip(fresh) {
            if let (Unit::Char { font, width, inset, line, ascent, lead, squeeze, .. }, Unit::Char { font: new_font, width: new_width, inset: new_inset, line: new_line, ascent: new_ascent, lead: new_lead, squeeze: new_squeeze, .. }) = (unit, new)
                && *font != new_font
            {
                (*font, *width, *inset, *line, *ascent, *lead, *squeeze) = (new_font, new_width, new_inset, new_line, new_ascent, new_lead, new_squeeze);
            }
        }
    }

    /// Break a paragraph `width` points wide into lines.
    ///
    /// Word draws a character whose typeface lacks it in the fallback typeface of the character before (`FallbackCarry`), and breaks the lines by the widths of
    /// that, wherever the lines break; then it draws the first character of each line after the first as standing apart, and carries on from it
    /// (`redraw_lines_after_the_first`, #M102).
    /// Splits one line, or the part of it that lies in one of the gaps an obstacle leaves, off `units` from `*next`: where the line ends, whether it ends by force (a break or
    /// the paragraph's end), and where the characters that stand past its edge begin. The body's lines (one gap, the column) and the lines of a paragraph that goes round a
    /// shape (a gap beside it) are split by this alone, so that stops and brackets, the squeezing, kinsoku, hanging, tabs and the justified line's last word are decided in
    /// one place (GitHub issue #196: the lines beside a figure were split by a shorter copy of this and differed from the same text set in the same width without it).
    fn break_segment(&self, p: &FlowParagraph, units: &mut [Unit], next: &mut usize, edges: &SegmentEdges, left: f32, tab_default: f32, state: &mut BreakState) -> (usize, usize, bool, usize) {
        let seg = self.break_segment_whole_runs(p, units, next, edges, left, tab_default, state);
        // A border (`w:bdr`) cut by the line's end takes its room (the line's width and its distance from the text, `W`) there and at the start of the next line: each
        // line's fragment of the run has `W` at both ends (Word's measurements of GitHub issue #192, a run wrapped across lines).
        let end = seg.1;
        if end > 0 && end < units.len() && !seg.2 {
            if let Some(w) = self.border_room_between(p, units, end - 1)
                && state.border_right.insert(end - 1)
                && let Unit::Char { width, .. } = &mut units[end - 1]
            {
                *width += w;
            }
            if let Some(w) = self.border_room_between(p, units, end - 1)
                && state.border_left.insert(end)
                && let Unit::Char { width, inset, .. } = &mut units[end]
            {
                *width += w;
                *inset += w;
            }
        }
        seg
    }

    /// The room `W` of the border of unit `i` when the unit and the one after it are in the same bordered box (a run, or runs in a row with the same border; the box's
    /// two ends have their room already, `units_cut`), else none.
    fn border_room_between(&self, p: &FlowParagraph, units: &[Unit], i: usize) -> Option<f32> {
        let border_of = |u: &Unit| -> Option<&ibis_doc_model::FlowBorder> {
            let Unit::Char { style, .. } = u else { return None };
            if *style == LABEL {
                return None;
            }
            let st = &p.runs.get(*style)?.style;
            if st.combine.is_some() || st.tate_chu_yoko {
                return None;
            }
            st.border.as_ref()
        };
        if self.vertical.get() {
            return None;
        }
        let b = border_of(units.get(i)?)?;
        (border_of(units.get(i + 1)?) == Some(b)).then(|| pt(b.total()) + pt(b.space))
    }

    fn break_segment_whole_runs(&self, p: &FlowParagraph, units: &mut [Unit], next: &mut usize, edges: &SegmentEdges, left: f32, tab_default: f32, state: &mut BreakState) -> (usize, usize, bool, usize) {
        let mut hang: Option<usize> = None;
        let mut i = *next;
        let start = i;
        let mut x = 0.0;
        let mut last_break: Option<usize> = None;
        // How far a right-aligned tab stop beyond the right indent lets the line go on (Word sets the text after it there).
        let mut tab_extra = 0.0f32;
        while i < units.len() {
            let room = edges.room + tab_extra;
            match &units[i] {
                Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak => {
                    let seg = (start, i + 1, true, hang_start(hang.take(), i + 1));
                    if i > 0 && let Unit::Char { width, auto_space, .. } = &mut units[i - 1] {
                        *width -= *auto_space;
                        *auto_space = 0.0;
                    }
                    *next = i + 1;
                    return seg;
                }
                Unit::ListTab if left > edges.origin + x + 0.01 => {
                    x = left - edges.origin;
                    i += 1;
                    continue;
                }
                Unit::Tab | Unit::ListTab => {
                    let at = edges.origin + x;
                    let tab = p.tabs.iter().find(|t| pt(t.position) > at + 0.01);
                    let stop = tab.map(|t| pt(t.position)).unwrap_or(((at / tab_default).floor() + 1.0) * tab_default);
                    // Text after a right or centre stop ends at (or centres on) it.
                    let rest: f32 = units[i + 1..].iter().take_while(|u| !matches!(u, Unit::Tab | Unit::PositionalTab { .. } | Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak)).map(unit_width).sum();
                    let stop = match tab.map(|t| t.align) {
                        Some(TabAlign::Right) => (stop - rest).max(at),
                        Some(TabAlign::Center) => (stop - rest / 2.0).max(at),
                        _ => stop,
                    };
                    x = stop - edges.origin;
                    if let Some(t) = tab.filter(|t| t.align == TabAlign::Right) {
                        tab_extra = tab_extra.max(pt(t.position) - (edges.origin + edges.room));
                    }
                    i += 1;
                    continue;
                }
                Unit::PositionalTab { align, relative_to_margin, .. } => {
                    let at = edges.origin + x;
                    let rest: f32 = units[i + 1..].iter().take_while(|u| !matches!(u, Unit::Tab | Unit::PositionalTab { .. } | Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak)).map(unit_width).sum();
                    let (span_l, span_r) = edges.span(*align, *relative_to_margin);
                    // A left-aligned tab after text that is already past the stop goes to the next line's start, the text after it with it (Word's PDF of the
                    // issue #196 file, beside a figure on the left: "ＡＢ" is on the first line and the text after the tab at the gap's left edge on the second).
                    if *align == TabAlign::Left && i > start && span_l < at - 0.01 {
                        let seg = (start, i, false, hang_start(hang.take(), i));
                        *next = i;
                        return seg;
                    }
                    let stop = match align {
                        TabAlign::Right => (span_r - rest).max(at),
                        TabAlign::Center => ((span_l + span_r - rest) / 2.0).max(at),
                        _ => span_l.max(at),
                    };
                    x = stop - edges.origin;
                    i += 1;
                    continue;
                }
                _ => {}
            }
            if i > start {
                self.warichu_split(p, units, i, room - x);
            }
            let w = unit_width(&units[i]);
            let c = unit_char(&units[i]);
            let combined_rest = is_combined_rest(p, &units[i], w) || is_box_rest(p, &units, i, start);
            // A break is allowed before this unit?
            if i > start && !combined_rest {
                if self.break_allowed(p, &units, start, i) {
                    last_break = Some(i);
                }
            }
            // The space after a character between Japanese and Latin (`auto_space`) is not needed where the line ends after it: the character fits when
            // it does without it (Word's PDFs of `kinsoku-*-probe.docx`: a kana before a Latin word ends a line when the kana fits, GitHub issue #68 D3).
            let trailing_space = if let Unit::Char { auto_space, .. } = &units[i] { *auto_space } else { 0.0 };
            // The room of a border that the line would cut after this unit (it is added when the line is cut there, `break_segment`): the unit must fit with it.
            let border_after = if state.border_right.contains(&i) { 0.0 } else { self.border_room_between(p, units, i).unwrap_or(0.0) };
            if x + box_width_from(p, &units, i) + border_after - trailing_space > room + 0.01 && i > start && !combined_rest {
                // 約物を詰める: a line a little too long takes in its stops and brackets (half an em each) before it
                // breaks.
                // Word squeezes a line to take in one more character than fits (Word's PDF of `compresspunct-mincho-w41453.docx`, with the
                // stops 、 。, brackets and the dot ・: ＭＳ 明朝 10.5 pt in 414.45 pt, 39 characters fit and the lines of text with stops and
                // brackets have 40, however many they have, and 41 only when the 41st cannot start a line (、 ） and the like): the squeeze is
                // shared by all the line's stops and brackets, as much as the line needs. A second overflowing character is taken in only when
                // it cannot start a line.
                let must_stay = i > start && !may_break(unit_char(&units[i - 1]), c, p.kinsoku, p.word_wrap, self.doc);
                // Compatibility mode 15, a justified or distributed line (GitHub issues #164 and #165, `justify_choice.rs`): the line takes in the next
                // break by squeezing only when the share of what it can give that it needs is no more than the share of the room it would spread,
                // and squeezes as little as it needs, in Word's order; else it breaks before and spreads. `decided`: the squeezing below is done.
                let (mut forced_break, mut decided) = (false, false);
                // (Not a distributed line: Word's PDF of `kinsoku-squeeze-distribute.docx`, case W018, keeps 51 characters where this takes 52, so its own rule below stays.)
                if self.doc.compat_mode.is_some_and(|m| m >= 15) && p.align == FlowAlign::Justify && (!self.char_grid.get() || (!self.snap_chars.get() && p.snap_to_grid && self.current_cell.get().is_none() && self.char_space.get().is_some_and(|c| c != Emu::default()))) {
                    match self.justify_choice(p, &units, start, i, last_break, room) {
                        JustifyChoice::BreakBefore => forced_break = true,
                        JustifyChoice::TakeIn(plan) => {
                            decided = true;
                            let mut any = false;
                            for sq in plan {
                                if let Unit::Char { c, width, inset, auto_space, squeezed, .. } = &mut units[sq.k] {
                                    *width -= sq.width;
                                    *inset -= sq.inset;
                                    *auto_space -= sq.auto_space;
                                    if *c != ' ' && sq.auto_space == 0.0 {
                                        *squeezed = true;
                                    }
                                    any = true;
                                }
                            }
                            if any {
                                x = (start..i).map(|k| unit_width(&units[k])).sum();
                                state.compressed_line = Some(start);
                                continue;
                            }
                        }
                        JustifyChoice::Leave => {}
                    }
                }
                let squeezes_allowed = !forced_break && !decided;
                // Compatibility mode 15: only in a justified or distributed paragraph (Word's PDFs of `squeezealign-mincho-w41453.docx` and
                // `squeezedist-mincho-w41453.docx`: the stops are squeezed whatever the line needs, one more character or the one that cannot start a
                // line; a left aligned line squeezes nothing, in all the grids). Below 15, or without a `compatibilityMode` (Word's PDF of
                // `squeezenocompat-mincho-w41453.docx` is that of mode 14 and 11), every alignment squeezes, but by no more than one stop's half em
                // (below).
                let below_15 = self.doc.compat_mode.is_none_or(|m| m < 15);
                let squeezes = below_15 || matches!(p.align, FlowAlign::Justify | FlowAlign::Distribute);
                // Full-width spaces before a closing character that must stay with them (Word's PDFs of `spacehang-compat11/14/15-mincho-w41453.docx`
                // and `spacecap-compat11/15-…`: "あ"×n, s full-width spaces, "）", a line of 20 ems): the spaces are squeezed so that the closing
                // character fits inside the line, each down to a quarter of an em and the run as a whole to a whole number of half ems (s = 1, 2: half an em,
                // 3, 4: one em, 5, 6: one and a half, 7, 8: two); when that is not enough the run is squeezed all the way and the closing character
                // hangs past the edge (its start is no further than the edge). Where the closing character is squeezed or hangs is up to the hang below.
                if squeezes_allowed && self.doc.compress_punctuation && squeezes && unit_char(&units[i]) == Some('\u{3000}') && !matches!(&units[i], Unit::Char { squeezed: true, .. }) {
                    let (mut j0, mut j1) = (i, i);
                    while j0 > start && unit_char(&units[j0 - 1]) == Some('\u{3000}') {
                        j0 -= 1;
                    }
                    while j1 + 1 < units.len() && unit_char(&units[j1 + 1]) == Some('\u{3000}') {
                        j1 += 1;
                    }
                    let closes = units.get(j1 + 1).and_then(unit_char).is_some_and(|ch| "、。，．,）」』】］｝’”".contains(ch)); // i18n-ok（閉じ括弧・句読点の文字の集合。文字の並び）
                    let all_spaces = (j0..=j1).all(|k| matches!(&units[k], Unit::Char { style, squeezed: false, .. } if *style != LABEL));
                    if closes && all_spaces {
                        let before: f32 = (j0..i).map(|k| unit_width(&units[k])).sum();
                        let nat: f32 = (j0..=j1).map(|k| unit_width(&units[k])).sum();
                        let em = unit_width(&units[j0]);
                        let min_total = ((j1 - j0 + 1) as f32 * 0.5).ceil() * 0.5 * em;
                        let x_run = x - before;
                        let need = x_run + nat + unit_width(&units[j1 + 1]) - room;
                        let amount = need.clamp(0.0, (nat - min_total).max(0.0));
                        if x_run + nat - amount <= room + 0.01 && amount > 0.01 {
                            let each = amount / (j1 - j0 + 1) as f32;
                            for k in j0..=j1 {
                                if let Unit::Char { width, squeezed, .. } = &mut units[k] {
                                    *width -= each;
                                    *squeezed = true;
                                }
                            }
                            x = x_run + (j0..i).map(|k| unit_width(&units[k])).sum::<f32>();
                            continue;
                        }
                    }
                }
                if squeezes_allowed && self.doc.compress_punctuation && squeezes && (state.compressed_line != Some(start) || must_stay) {
                    // A stop or bracket gives up the half em it has beside its glyph, once: one that was squeezed (a kerned pair, or on an
                    // earlier pass of this line) has no more, and one narrower than that half has none to give. List numbering labels (`LABEL`)
                    // are not squeezed (`i1_leftchars_zero_keeps_numbering_indent`).
                    // A face whose stops are narrower than an em (a proportional one: Meiryo UI and ＭＳ Ｐ明朝 set 、 at 0.664 em) squeezes
                    // nothing, its brackets neither (Word's PDFs of `squeeze-proportional-*-probe.docx`, 8 pt: Meiryo UI and ＭＳ Ｐ明朝 keep 52 and
                    // 53 characters in the justified lines of mode 15 as in the left aligned ones, 游明朝 takes in one more there, 52 to 51).
                    let full_stops = |font: &Font, style: usize, c: char| {
                        // (The typeface the run asks for: the one drawn may be a stand-in on this PC.)
                        let asked = p.runs.get(style).and_then(|r| r.style.font_for(c)).unwrap_or(font.family.as_ref());
                        !proportional_cjk(asked) && advance_em(self.res, Some(font.family.as_ref()), '、') > 0.99
                    }; // i18n-ok（読点の幅で書体を見分ける）
                    let squeezable = |u: &Unit| matches!(u, Unit::Char { c, width, size, style, squeeze, squeezed, font, .. } if *style != LABEL && (can_give_up_half(*c, *width, *size * *squeeze, *squeezed) || (below_15 && (colon_gives_up_half(*c, *width, *size * *squeeze, *squeezed) || (*c == '\u{3000}' && !*squeezed && *width > *size * *squeeze / 2.0)))) && full_stops(font, *style, *c));
                    // The closing brackets that must stay with this character count too (a stop may hang past the edge).
                    let mut end = i;
                    while end + 1 < units.len() && unit_char(&units[end + 1]).is_some_and(|ch| "）」』】〕〉》］｝".contains(ch)) { // i18n-ok（閉じ括弧の文字の集合。文字の並び）
                        end += 1;
                    }
                    // What does not fit is the unit that goes to the next line with the closing brackets that must stay with it, a Latin word whole (Word's XPS of
                    // `（1時間休憩）ABC`: the line takes in the whole of ABC by squeezing, not only its first letter).
                    let word_rest: f32 = if unit_char(&units[i]).is_some_and(|c| c.is_ascii_alphanumeric()) {
                        (i..units.len()).take_while(|&k| unit_char(&units[k]).is_some_and(|c| c.is_ascii_alphanumeric())).map(|k| unit_width(&units[k])).sum()
                    } else {
                        0.0
                    };
                    let tail = (i..=end).map(|k| unit_width(&units[k])).sum::<f32>().max(box_width_from(p, &units, i) - trailing_space).max(word_rest);
                    // A closing character that follows hangs past the edge (below mode 15, or in a justified paragraph of mode 15, in the body, with `w:overflowPunct`): the unit
                    // does not have to make room for it (Word's `（1時間休憩）`: the quarter em between 1 and 時 is squeezed to take 憩 in, and the bracket hangs).
                    let hang_w = if end > i
                        && p.overflow_punct
                        && self.current_cell.get().is_none()
                        && (self.doc.compat_mode.is_none_or(|m| m < 15) || matches!(p.align, FlowAlign::Justify | FlowAlign::Distribute))
                        && unit_char(&units[end]).is_some_and(|ch| "、。，．）」』】］｝".contains(ch)) // i18n-ok（ぶら下げる文字の集合。文字の並び）
                    {
                        unit_width(&units[end])
                    } else {
                        0.0
                    };
                    let over = x + tail - hang_w - room;
                    // The first character of the line that is seen, with nothing but half-width spaces before it (a first line's indent made with spaces), counts as the start of the
                    // line (Word's PDF of corpus 047 page 2: the 【 after two spaces and a first line indent is where the line's start puts it, not 5.25 pt to the left).
                    // An opening bracket that starts the line keeps its half em (Word's PDFs of `squeeze-line-start-bracket-mode*-probe.docx`,
                    // ＭＳ 明朝 10.5 pt on a line of 40.55 em, 0.45 em short of a 41st character: a line that starts with （ or 「 keeps 40 in modes 11
                    // and 15, left aligned or justified; one that starts with （（ takes in the 41st, the second bracket giving up its half).
                    let opens_line = |k: usize| (start..k).all(|j| unit_char(&units[j]) == Some(' ')) && unit_char(&units[k]).is_some_and(|ch| "（「『【〔〈《［｛".contains(ch)); // i18n-ok（始め括弧の文字の集合。文字の並び）
                    // (A full-width space that starts the line gives half an em in Windows Word's XPS of `　あいう`, `tasks/doc/requests/2026-11-17-54-punct-2-result.md` C2; not taken in:
                    // the lines of the corpus whose paragraphs start with one, the indent of a paragraph, are 12 documents farther from Word's PDFs with it. `…-punct-3.md`.)
                    // (Windows Word, `2026-11-18-54-grid-result.md`: `linesAndChars` squeezes as without a grid, every amount scaled by the grid's pitch, the font size and the added
                    // pitch; `snapToChars` squeezes nothing but hangs.)
                    let on_char_grid = self.snap_chars.get() && p.snap_to_grid && self.current_cell.get().is_none();
                    let pitch_add = if below_15 && self.char_grid.get() && p.snap_to_grid && self.current_cell.get().is_none() { self.char_space.get().map_or(0.0, pt) } else { 0.0 };
                    // (The closing character that hangs is not squeezed: that would not move the text before it.)
                    let candidates: Vec<usize> = (start..=end).filter(|&k| squeezable(&units[k]) && !opens_line(k) && !(hang_w > 0.0 && k == end)
                        // (A ： or ； that would end the line is not squeezed, Word's PDF of `justify-squeeze-or-break-compat14.docx` paragraph 6: 44 characters, GitHub issue #164.)
                        && !(k == end && unit_char(&units[k]).is_some_and(|ch| "：；".contains(ch)))).collect(); // i18n-ok（コロン・セミコロンの文字。文字の並び）
                    let halves = candidates.iter().map(|&k| if let Unit::Char { size, squeeze, .. } = &units[k] { (*size + pitch_add) * *squeeze / 2.0 } else { 0.0 });
                    // Below 15 the line gives up no more than one stop's half em to take in a character that may start a line; one that
                    // cannot is taken in as in mode 15. Word's PDFs of mode 11 (ＭＳ 明朝, 414.45 pt wide, left aligned, justified and distributed,
                    // with no grid, a line grid or a grid of lines and characters, in the body or in a table's cell): at 10.5 pt the 40th kanji
                    // needs 0.53 em and the lines keep 39 however many stops they have, at 12 pt the 35th needs 0.5 em and is taken in; 028
                    // (12 pt, 489.05 pt) takes in its 41st, 0.25 em over; a ） as the 40th is taken in at 10.5 pt. A line on a character grid
                    // (a section's `linesAndChars`, the paragraph on the grid, not in a table's cell) takes in none: the 12 pt file with that
                    // grid keeps 34 a line, in a cell 35; 122 (12 pt, `linesAndChars`) keeps its lines 0.21 em short of the next character.
                    // What a line below mode 15 may give in all (stops, brackets, spaces and kana together) is decided by the unit that goes to the next line, and by what
                    // the line has to give (Word's XPS, `tasks/doc/requests/2026-11-17-54-punct-2-result.md`, A1 to A4; em of the unit's size):
                    // - a kana, a kanji, a full-width digit: half an em, a closing bracket or a stop being the line's only point; after an opening bracket (the line has no closing
                    //   one) half an em and the unit's width, but no more than the points give: the opening brackets are a half em each; `あい` after it, half an em;
                    // - a Latin word: its width less its first letter, at least half an em and the space before it (`A` 0.64, `ABC` 1.35); after an opening bracket (the line has no
                    //   closing one) half an em and the word's width;
                    // - a closing bracket or a stop: the points of the line, two ems at most (an opening bracket before it, one and a half);
                    // - an opening bracket: one em with opening brackets only, a quarter em with closing ones only.
                    let em_unit = |k: usize| if let Unit::Char { size, style, .. } = &units[k] { pitch_add + *size * if *style == LABEL { 1.0 } else { p.runs.get(*style).map_or(1.0, |r| r.style.scale as f32 / 100.0) } } else { 0.0 };
                    let opening_char = |ch: char| "（「『【〔〈《［｛".contains(ch); // i18n-ok（始め括弧の文字の集合。文字の並び）
                    let closing_char = |ch: char| "、。，．）」』】〕〉》］｝".contains(ch); // i18n-ok（閉じ括弧・句読点の文字の集合。文字の並び）
                    let sum_halves: f32 = candidates.iter().map(|&k| if let Unit::Char { size, squeeze, .. } = &units[k] { (*size + pitch_add) * *squeeze / 2.0 } else { 0.0 }).sum();
                    let used_before = if state.squeezed_so_far.0 == start { state.squeezed_so_far.1 } else { 0.0 };
                    let total_cap: f32 = {
                        let em = em_unit(i);
                        let is_open = |k: usize| unit_char(&units[k]).is_some_and(opening_char);
                        let n_open = candidates.iter().filter(|&&k| is_open(k)).count();
                        let n_close = candidates.len() - n_open;
                        // `w:kinsoku` 0: the line may end on an opening bracket, so no unit stays with the one before it; `w:wordWrap` 0: a Latin word breaks anywhere, the unit that goes
                        // to the next line is its last letter, and the limit is half of that letter's width (Windows Word's XPS, `tasks/doc/requests/2026-11-17-54-punct-3-result.md` 1).
                        let (ki_off, ww_off) = (!p.kinsoku, !p.word_wrap);
                        let after_opener = |k: usize| !ki_off && k > start && is_open(k - 1);
                        match unit_char(&units[i]) {
                            
                            Some(ch) if closing_char(ch) => if ki_off { em } else if n_open == 0 { 2.0 * em } else if n_close == 0 { 1.5 * em } else { f32::INFINITY },
                            Some(ch) if opening_char(ch) => {
                                // After a closing bracket (the pair kerned: the closing one is squeezed already) a quarter em, `w:kinsoku` 0 or not; else with opening brackets only an em at most.
                                if i > start && matches!(&units[i - 1], Unit::Char { c, .. } if closing_char(*c)) {
                                    em / 4.0
                                } else if n_close == 0 {
                                    sum_halves.min(em).max(em / 2.0)
                                } else {
                                    em / 2.0
                                }
                            }
                            Some(ch) if ch.is_ascii_alphanumeric() && ww_off => unit_width(&units[i]) / 2.0,
                            Some(ch) if ch.is_ascii_alphanumeric() => {
                                let word = |k: usize| unit_char(&units[k]).is_some_and(|c| c.is_ascii_alphanumeric());
                                let mut first = i;
                                while first > start && word(first - 1) {
                                    first -= 1;
                                }
                                let mut last = i;
                                while last + 1 < units.len() && word(last + 1) {
                                    last += 1;
                                }
                                let width: f32 = (first..=last).map(|k| unit_width(&units[k])).sum();
                                let space_before = if first > start { if let Unit::Char { auto_space, .. } = &units[first - 1] { *auto_space / 2.0 } else { 0.0 } } else { 0.0 };
                                if n_open > 0 && after_opener(first) { em / 2.0 + width } else { (em / 2.0 + space_before).max(width - unit_width(&units[first])) }
                            }
                            _ => if n_open > 0 && after_opener(i) { if n_close == 0 { em / 2.0 + unit_width(&units[i]) } else { f32::INFINITY } } else { em / 2.0 },
                        }
                    };
                    let total_cap = if below_15 { (total_cap - used_before).max(0.0) } else { total_cap };
                    let possible: f32 = match (below_15 && !must_stay, on_char_grid) {
                        (true, true) => 0.0,
                        // (A unit that may start a line is taken in by one stop's half em at most, Word's PDFs of mode 11; one that must stay with the one before it, by the points.)
                        (true, false) => halves.fold(0.0, f32::max).min(total_cap),
                        // (Below 15 a unit that must stay with the one before it, after an opening bracket, takes in the points too, by the same limit.)
                        _ if below_15 => sum_halves.min(total_cap),
                        _ => halves.sum(),
                    };
                    // Below 15 a line also narrows the automatic spaces between Japanese and Latin letters or digits, down to half of a quarter em each, after the stops and
                    // brackets have given what they can (Word's XPS of corpus 016's cell, Meiryo 11 pt, mode 14, `compressPunctuation`: 「（1時間休憩）」 is one line in
                    // 1100 twips, the closing bracket's half em and half of the quarter em between 1 and 時; 「第1章」 two such spaces, 「1時間休憩」 one, whatever else is on
                    // the line; with `none` or `doNotCompress`, and in mode 15, nothing is squeezed: `tasks/doc/requests/2026-11-17-54-compress-punctuation-result.md`).
                    let gap_cap = |u: &Unit| match u {
                        Unit::Char { auto_space, size, style, .. } if *auto_space > 0.0 && *style != LABEL => {
                            let scale = p.runs.get(*style).map_or(1.0, |r| r.style.scale as f32 / 100.0);
                            (*auto_space - *size * scale / 8.0).max(0.0)
                        }
                        _ => 0.0,
                    };
                    let gap_room: f32 = if below_15 { (start..i).map(|k| gap_cap(&units[k])).sum() } else { 0.0 };
                    let gap_room = gap_room.min((total_cap - over.min(possible)).max(0.0));
                    // With the kana too (`compressPunctuationAndJapaneseKana`) the line then gives a sixteenth of an em after each kana, half an em at most in all.
                    let kana_cap = |u: &Unit| match u {
                        // (A kana with a quarter em after it, before Latin letters or digits, gives nothing: the space is what narrows there. Word's `あA` and `ですABC`.)
                        Unit::Char { c, size, style, squeezed: false, auto_space, .. }
                            if *style != LABEL && gives_kana_squeeze(*c) && *auto_space <= 0.0 && !p.runs.get(*style).and_then(|r| r.style.font_for(*c)).is_some_and(proportional_cjk) =>
                        {
                            *size * p.runs.get(*style).map_or(1.0, |r| r.style.scale as f32 / 100.0) / 16.0
                        }
                        _ => 0.0,
                    };
                    let kana_room: f32 = if below_15 && self.doc.compress_kana {
                        let used = over.min(possible) + gap_room.min((over - possible).max(0.0));
                        (start..=i).map(|k| kana_cap(&units[k])).sum::<f32>().min(em_unit(i) / 2.0).min((total_cap - used).max(0.0))
                    } else {
                        0.0
                    };
                    if possible + gap_room + kana_room >= over - 0.01 {
                        let (mut left, mut before) = (over.min(possible), 0.0);
                        let mut squeezed_any = false;
                        let mut taken = 0.0;
                        for k in candidates {
                            if left <= 0.01 {
                                break;
                            }
                            if let Unit::Char { c, width, size, inset, squeeze, squeezed, .. } = &mut units[k] {
                                let half = *size * *squeeze / 2.0;
                                *width -= half;
                                *squeezed = true;
                                squeezed_any = true;
                                if "（「『【〔〈《［｛".contains(*c) { // i18n-ok（始め括弧の文字の集合。文字の並び）
                                    *inset -= half;
                                } else if "・：；".contains(*c) { // i18n-ok（中点・コロン・セミコロンの文字の集合。文字の並び）
                                    // The dot is in the middle of its em: the squeeze takes the same from both sides (Word's PDF).
                                    *inset -= half / 2.0;
                                }
                                left -= half;
                                taken += half;
                                if k < i {
                                    before += half;
                                }
                            }
                        }
                        // What the stops and brackets could not give is taken from the automatic spaces, an equal share of each up to what it has.
                        let mut rest = over - taken;
                        if rest > 0.01 && gap_room > 0.0 {
                            let mut open: Vec<usize> = (start..i).filter(|&k| gap_cap(&units[k]) > 0.0).collect();
                            while rest > 0.001 && !open.is_empty() {
                                let share = rest / open.len() as f32;
                                let mut next = Vec::new();
                                for &k in &open {
                                    let cap = gap_cap(&units[k]);
                                    let give = share.min(cap);
                                    if let Unit::Char { width, auto_space, .. } = &mut units[k] {
                                        *width -= give;
                                        *auto_space -= give;
                                    }
                                    rest -= give;
                                    before += give;
                                    squeezed_any = true;
                                    if give < cap - 0.001 {
                                        next.push(k);
                                    }
                                }
                                if next.len() == open.len() && share <= 0.0 {
                                    break;
                                }
                                open = next;
                            }
                        }
                        if rest > 0.01 && kana_room > 0.0 {
                            let mut open: Vec<usize> = (start..=i).filter(|&k| kana_cap(&units[k]) > 0.0).collect();
                            let mut budget = kana_room.min(rest);
                            while budget > 0.001 && !open.is_empty() {
                                let share = budget / open.len() as f32;
                                let mut next = Vec::new();
                                for &k in &open {
                                    let cap = kana_cap(&units[k]);
                                    let give = share.min(cap);
                                    if let Unit::Char { width, squeezed, .. } = &mut units[k] {
                                        *width -= give;
                                        *squeezed = true;
                                    }
                                    budget -= give;
                                    rest -= give;
                                    if k < i {
                                        before += give;
                                    }
                                    squeezed_any = true;
                                    if give < cap - 0.001 {
                                        next.push(k);
                                    }
                                }
                                open = next;
                            }
                        }
                        if below_15 {
                            state.squeezed_so_far = (start, used_before + (over - rest.max(0.0)).max(taken));
                        }
                        x -= before;
                        state.compressed_line = Some(start);
                        // Nothing was squeezed (`over` is not positive: the overflow was found by the width a soft hyphen's `auto_space`
                        // holds back, which `over` does not count, GitHub issue #161): going on from here with nothing changed would loop for ever.
                        if squeezed_any {
                            continue;
                        }
                    }
                }
                // A justified or distributed line of mode 15 squeezes the automatic spaces between Japanese and Latin letters or digits to take one more
                // character, by as much as the line is short, and by the same limit as it narrows its spaces (`justify_overflow_room`) with half of the
                // automatic spaces in place of a quarter of the spaces: the smaller of half of them all and 0.35 of the last word with the space before it.
                // Word's PDFs of `justify-autospace-squeeze-{b*-g*,gaps1,gaps3,gaps4}-probe.docx` (ＭＳ ゴシック 10 pt, a line of 「あ」 and 「第…条」 with
                // 1, 2, 3 and 4 spaces of 2.5 pt, narrowed by 0.5 pt a paragraph, all the same with balance and a grid or not): the last 「条」 is taken
                // when the line is short by 1.0 (not 1.5), 2.5 (not 3.0), 3.5 (not 4.0) and 4.0 (not 4.5) pt, between min(1.25, 4.375), min(2.5, 4.375),
                // min(3.75, 4.375) and min(5.0, 4.375) with the last word 「条」 and its space 12.5 pt. The spaces are narrowed by as much as the line is short.
                // In modes 11 and 14 (`…-compat11-probe.docx`, `…-compat14-…`) Word squeezes nothing: only the line that fits is taken. The 0.35 and the
                // half are Word's, as those of `justify_overflow_room`; why they are, is not known. 150 corpus documents: 142 +159, 143 +40 lines.
                if squeezes_allowed && self.doc.compat_mode.is_some_and(|m| m >= 15) && matches!(p.align, FlowAlign::Justify | FlowAlign::Distribute) {
                    // The overflowing character's own space after it ends the line and is not needed (as above); the limit is of the line's spaces
                    // as they were before the characters already taken squeezed them (GitHub issue #68 D2, Word's PDF of `kinsoku-both-normal-probe.docx`
                    // W050 and W083: 「…る1990–2000」 and 「…ろe-mail」 end the justified line, 1.2 pt over, with the one space before the word).
                    let over = x + w - trailing_space - room;
                    let gap_of = |k: usize| if let Unit::Char { auto_space, .. } = &units[k] { *auto_space } else { 0.0 };
                    let gaps: Vec<usize> = (start..i).filter(|&k| gap_of(k) > 0.0).collect();
                    let all: f32 = gaps.iter().map(|&k| gap_of(k)).sum();
                    let cut_before = if state.gaps_cut.0 == start { state.gaps_cut.1 } else { 0.0 };
                    // The last word: what follows the last space on the line, with that space (the whole line when there is none before it).
                    let last_word = match (start..i).rev().find(|&k| gap_of(k) > 0.0) {
                        Some(g) => units[g + 1..=i].iter().enumerate().map(|(n, u)| unit_width(u) - gap_of(g + 1 + n)).sum::<f32>() + gap_of(g),
                        None => f32::MAX,
                    };
                    let limit = ((all + cut_before) * 0.5).min(last_word * 0.35);
                    // A distributed line squeezes them only for the word that follows the last space: Japanese after it, which may break between its
                    // characters, is not taken (Word's PDF of `kinsoku-distribute-normal-probe.docx`: 「…89あいうえ」 breaks before お, 2.2 pt over;
                    // 「…67.89あ」 takes あ, as much over; a justified line takes both, `kinsoku-both-normal-probe.docx`). Of its 115 cases one
                    // still differs (W056: 「a/b/c/dあい」 is taken).
                    let in_last_word = p.align != FlowAlign::Distribute || gaps.last().is_none_or(|&g| {
                        (g + 2..=i).all(|k| !(unit_char(&units[k - 1]).is_some_and(crate::render::line_break::is_ideographic) || unit_char(&units[k]).is_some_and(crate::render::line_break::is_ideographic)))
                    });
                    if all > 0.01 && in_last_word && limit >= over + cut_before - 0.01 {
                        let factor = (over / all).clamp(0.0, 1.0);
                        let mut before = 0.0;
                        for k in gaps {
                            if let Unit::Char { width, auto_space, .. } = &mut units[k] {
                                let cut = *auto_space * factor;
                                *width -= cut;
                                *auto_space -= cut;
                                before += cut;
                            }
                        }
                        state.gaps_cut = (start, cut_before + before);
                        x -= before;
                        continue;
                    }
                }
                // Spaces at the end of a line hang past the edge, but the first of them, a half em wide (an East Asian space, see where the widths are balanced), has to fit
                // after the Latin word it follows: "Date:" with 39 spaces before it stays on the line, with 40 it goes down to the next line though the word itself would fit
                // (Word's PDF of `date-wrap-variants.docx`; a word of 80 half-width characters that fills the line to a twip stays, the space after it hanging).
                let first_space_must_fit = i > start
                    // An East Asian space: not a single one between two Latin words (the next unit is another space or a character of East Asian text; a single space that ends the paragraph hangs: Word's PDF of corpus 124).
                    && self.doc.balance_single_double_byte_width
                    && units.get(i + 1).is_some_and(|u| unit_char(u).is_some_and(|n| n == ' ' || is_cjk(n)))
                    && unit_char(&units[i - 1]).is_some_and(|ch| ch != ' ' && !is_cjk(ch) && !ch.is_whitespace())
                    && x + w > room + 0.1;
                if !forced_break && c == Some(' ') && !first_space_must_fit {
                    hang.get_or_insert(i);
                    x += w;
                    i += 1;
                    continue;
                }
                // Full-width spaces that end the paragraph hang too.
                if !forced_break && c == Some('\u{3000}') && units[i..].iter().all(|u| matches!(unit_char(u), Some('\u{3000}'))) {
                    hang.get_or_insert(i);
                    x += w;
                    i += 1;
                    continue;
                }
                // Punctuation hangs past the edge if overflow_punct is enabled.
                // Which characters hang past the edge, and where (Word's PDFs of `hang-line-end-closers-compat14-probe.docx` and `…compat15-probe.docx`, 20
                // full-width characters fill the line, then one closing character: a line of one such paragraph at a time): ） 」 』 】 ］ ｝ 、 。 ， ． ’ ” hang;
                // 〕 〉 》 do not (the character before them goes down with them), nor do ー ぁ っ (they may start a line). Below mode 15 a left aligned line
                // hangs them as well as a justified one; in mode 15 only a justified one does (a left aligned line puts the character, and the one before
                // it, on the next line, a 、 or 。 too).
                // (Latin closers in a justified line of mode 15, `justify-hang-closer-compat15-probe.docx`, Times New Roman 10.5 pt, 8 words, the last with one
                // closer: the largest overflow taken grows by the closer's width for ” ’ . ,; for " ' ) ] } ; : ! ? it does not grow, and 、 。 ） 」 take 12 pt and more.)
                // Not in a table's cell: its text does not hang past the cell's edge, the closing character takes the one before it down (Word's PDF of corpus 023
                // page 10: "（※変更箇所の概要及び大学等確認変更届）" in a cell with room for 19 characters breaks before 届, where hanging gave one line). Of 150 corpus
                // documents, 9 have more lines like Word's with it and none fewer (59 lines), and the page count of 143 is Word's. Only the body (and a text box)
                // hangs, as the probes (paragraphs of the body) say.
                // A paragraph without kinsoku (`w:kinsoku w:val="0"`) hangs nothing, and the half-width ｡ ､ hang as 、 。 do (Word's PDFs of
                // `kinsoku-*-off-probe.docx` and `kinsoku-*-normal-probe.docx`, GitHub issue #68 D5 and D10).
                // `w:kinsoku` does not decide what hangs (Windows Word's XPS of a paragraph of the body, `w:kinsoku` 0 or not, `w:wordWrap` 0 or not, the one line and the line that
                // wraps: 「時間休憩）」 hangs 1 em, `tasks/doc/requests/2026-11-17-54-punct-3-result.md` 4): below mode 15 every paragraph hangs, in mode 15 the paragraphs that are
                // justified (the corpus's `Normal` styles have `jc both`) as in mode 14 (`2026-11-18-54-grid-result.md` 4), a left aligned, centred or right aligned one nothing;
                // `w:overflowPunct` 0 keeps everything in.
                let hangs = self.doc.compat_mode.is_none_or(|m| m < 15) || matches!(p.align, FlowAlign::Justify | FlowAlign::Distribute);
                let hanging = if hangs { "、。，．,.）」』】］｝’”｡､" } else { "" }; // i18n-ok（ぶら下げる文字の集合。文字の並び）
                // A justified line (mode 15) that took its last word by narrowing its spaces (below) has its closing punctuation hang as well: the
                // line's own overflow is within what the spaces give (Word's PDF of corpus 124: the curly quote that closes "This Content" ends a line).
                let hang_allow = if matches!(p.align, FlowAlign::Justify) && self.doc.compat_mode.is_none_or(|m| m >= 15) && p.overflow_punct && c.is_some_and(|ch| hanging.contains(ch)) {
                    let we = (i..units.len()).find(|&k| unit_char(&units[k]).is_none_or(|ch| ch == ' ')).unwrap_or(units.len());
                    justify_overflow_room(&units, start, i, we)
                } else {
                    0.0
                };
                // The text before a hanging character is brought into the line by the spaces between Japanese and Latin letters, half of each at most, when it is over by no more than they give
                // (Word's XPS of `（1時間休憩）` in ＭＳ 明朝, mode 14, `compressPunctuation`: the closing bracket hangs its em and the quarter em between 1 and 時 gives its half).
                if !forced_break
                    && self.doc.compress_punctuation
                    && self.doc.compat_mode.is_none_or(|m| m < 15)
                    && p.overflow_punct
                    && self.current_cell.get().is_none()
                    && x > room + 0.01
                    && c.is_some_and(|ch| hanging.contains(ch) && !keeps_width_at_end(&units, i, ch, p.align, self.doc.compress_punctuation))
                {
                    let cap_of = |u: &Unit| match u {
                        Unit::Char { auto_space, size, style, .. } if *auto_space > 0.0 && *style != LABEL => {
                            (*auto_space - *size * p.runs.get(*style).map_or(1.0, |r| r.style.scale as f32 / 100.0) / 8.0).max(0.0)
                        }
                        _ => 0.0,
                    };
                    let room_gaps: f32 = (start..i).map(|k| cap_of(&units[k])).sum();
                    if x - room <= room_gaps + 0.01 {
                        let mut need = x - room;
                        let mut open: Vec<usize> = (start..i).filter(|&k| cap_of(&units[k]) > 0.0).collect();
                        while need > 0.001 && !open.is_empty() {
                            let share = need / open.len() as f32;
                            let mut next = Vec::new();
                            for &k in &open {
                                let cap = cap_of(&units[k]);
                                let give = share.min(cap);
                                if let Unit::Char { width, auto_space, .. } = &mut units[k] {
                                    *width -= give;
                                    *auto_space -= give;
                                }
                                need -= give;
                                x -= give;
                                if give < cap - 0.001 {
                                    next.push(k);
                                }
                            }
                            open = next;
                        }
                    }
                }
                // (An ASCII , or . and a curly quote of a narrow Latin glyph hang only where text follows them: the last character of a paragraph keeps its width, Word's XPS of `ABC,`: 22.3 pt and the comma's width; a curly quote that is full width, set in the East Asian typeface, hangs as a Japanese closer does: Word's PDFs of `hang-line-end-closers-compat14-probe.docx`, ’ and ” last in the paragraph after 20 kana, 21 characters in the line.)
                if !forced_break && p.overflow_punct && self.current_cell.get().is_none() && c.is_some_and(|ch| hanging.contains(ch) && !keeps_width_at_end(&units, i, ch, p.align, self.doc.compress_punctuation)) && x <= room + 0.01 + hang_allow { // i18n-ok（オーバーフロー句読点・閉じ括弧の文字の集合。文字の並び。Word の PDF では行末の「】」もぶら下がる）
                    hang.get_or_insert(i);
                    x += w;
                    i += 1;
                    continue;
                }
                if !forced_break
                    && matches!(p.align, FlowAlign::Justify | FlowAlign::Distribute)
                    && self.doc.compat_mode.is_none_or(|m| m >= 15)
                    && c.is_some_and(|ch| !ch.is_whitespace() && !is_cjk(ch))
                {
                    let mut word_end = i + 1;
                    while word_end < units.len()
                        && !matches!(units[word_end], Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak | Unit::Tab | Unit::ListTab)
                        && unit_char(&units[word_end]) != Some(' ')
                        && !may_break(unit_char(&units[word_end - 1]), unit_char(&units[word_end]), p.kinsoku, p.word_wrap, self.doc)
                    {
                        word_end += 1;
                    }
                    let word_rest: f32 = units[i..word_end].iter().map(unit_width).sum();
                    let space_shrink = justify_overflow_room(&units, start, i, word_end);
                    // Closing punctuation at the word's end hangs past the edge and takes none of the overflow (Word's PDF of corpus 124: a line ends
                    // with the closing curly quote of "This Content", which is not counted).
                    let hung: f32 = if p.overflow_punct && hangs { units[i..word_end].iter().rev().take_while(|u| unit_char(u).is_some_and(|ch| hanging.contains(ch))).map(unit_width).sum() } else { 0.0 };
                    if x + word_rest - hung <= room + space_shrink + 0.01 {
                        x += w;
                        i += 1;
                        continue;
                    }
                }
                let at = match last_break {
                    Some(b) if b > start => b,
                    _ => i,
                };
                let seg = (start, at, false, hang_start(hang.take(), at));
                // A line that ends at an optional hyphen (`w:softHyphen`) shows it as a hyphen (Word's PDFs of `kinsoku-latin-*-probe.docx`:
                // "supercali-" ends a line, GitHub issue #68 D12).
                if at > 0 && let Unit::Char { c, width, font, size, .. } = &mut units[at - 1] && *c == '\u{ad}' {
                    *c = '-';
                    *width = self.res.text_width("-", font, *size);
                }
                if at > 0 && let Unit::Char { width, auto_space, .. } = &mut units[at - 1] {
                    *width -= *auto_space;
                    *auto_space = 0.0;
                }
                *next = at;
                return seg;
            }
            x += w;
            i += 1;
        }
        let seg = (start, units.len(), true, hang_start(hang.take(), units.len()));
        if let Some(Unit::Char { width, auto_space, .. }) = units.last_mut() {
            *width -= *auto_space;
            *auto_space = 0.0;
        }
        *next = units.len();
        seg
    }

    /// What the cells of a `linesAndChars` grid leave of a line's width unused: `W - floor(W / pitch) * pitch` (below mode 15, in a section on the grid, whatever the paragraph's `snapToGrid`, not in a table's cell; a vertical section too: Word's PDFs of `tbRl` pages cut at the same `floor(W / pitch) * pitch`).
    fn char_grid_cut(&self, p: &FlowParagraph, width: f32, in_table: bool) -> f32 {
        let below_15 = self.doc.compat_mode.is_none_or(|m| m < 15);
        if !below_15 || !self.char_grid.get() || self.snap_chars.get() || in_table || self.current_cell.get().is_some() {
            return 0.0;
        }
        let base = self.doc.styles.paragraph_base(None, None).1.size.unwrap_or(1000) as f32 / 100.0;
        let pitch = base + self.char_space.get().map_or(0.0, pt);
        if pitch < 1.0 {
            return 0.0;
        }
        (width - ((width / pitch + 0.001).floor() * pitch)).max(0.0)
    }

    fn paragraph(&self, p: &FlowParagraph, width: f32, page_number: u32, obstacles: &[WrapObstacleInPara], in_table: bool) -> Vec<LineBox> {
        self.line_room.set(width - pt(p.left) - pt(p.right));
        let (mut units, mut positions, carried) = self.units_cut(p, page_number, &[]);
        self.line_room.set(f32::MAX);
        let left = pt(p.left);
        // Below mode 15 the text of a section's `linesAndChars` grid goes only as far as the last whole cell (Windows Word, 2026-11-26, 1210 widths of ＭＳ 明朝 and 游明朝,
        // `tasks/doc/handoff-docx.md`): the line may be `floor(W / pitch) * pitch` long, `pitch` the Normal style's size plus the grid's added pitch, whatever the size of the
        // text (12 pt text on a 14 pt grid: 29 cells, 406 pt, in 414.45 pt: 33 characters); the indents come off that line, not off `W`.
        let right = pt(p.right) + self.char_grid_cut(p, width, in_table);
        // A list item's number centred on, or ending at, its position.
        let label: f32 = units.iter().take_while(|u| matches!(u, Unit::Char { style: LABEL, .. })).map(unit_width).sum();
        let label_align = p.list.and_then(|l| self.doc.styles.numbering.level(l.num_id, l.level)).map(|l| l.align);
        let shift = match label_align {
            Some(FlowAlign::Right) => label,
            Some(FlowAlign::Center) => label / 2.0,
            _ => 0.0,
        };
        // A count of characters (not 0) is the first line's indent, the length beside it ignored: one character is a full-width character of the
        // paragraph's first text (its size, its character spacing as the line counts it, and on a character grid the grid's added pitch), the
        // product in whole twips. Word's PDF of `distribute-first-line-chars-probe.docx` (ＭＳ 明朝 10.5 and 12 pt, firstLine 500): firstLineChars 100
        // and 250 start 1 and 2.5 characters in, 0 or none 25 pt. The lengths Word writes beside the counts in the corpus are that sum: 202 twips
        // for 100 at 10.5 pt and charSpace -1730 (128: 210 - 8.45 = 201.55), 1053 for 500 at charSpace 117 (062: 5 x 210.57), 180 for 100 in 9 pt
        // text under a paragraph mark of the default 10.5 pt (021: the first run's size, not the mark's), 640 for 250 at 10 pt with a character
        // spacing of 1.4 pt counted twice (021: 2.5 x 256).
        let first_line = match p.first_line_chars {
            Some(chars) if chars != 0 => {
                let run = p.runs.iter().find(|r| matches!(&r.kind, FlowRunKind::Text(t) if !t.is_empty())).map(|r| &r.style);
                let size = run.map_or(p.mark.size, |s| s.size) as f32 / 100.0;
                let spacing = run.map_or(0.0, |s| pt(s.spacing)) * self.spacing_factor(p, 'あ'); // i18n-ok（全角の文字）
                let grid = if p.snap_to_grid { self.char_space.get().map_or(0.0, pt) } else { 0.0 };
                (chars as f32 / 100.0 * (size + spacing + grid) * 20.0).round() / 20.0
            }
            _ => pt(p.first_line),
        };
        let first = first_line - shift;
        let tab_default = self.default_tab_pt();

        let line_start_x = |first_line: bool| left + if first_line { first } else { 0.0 };

        let mark_size = p.mark.size as f32 / 100.0;
        let mark_metrics = {
            // A header's or a footer's paragraphs are laid out as a cell's (`in_table`), but an empty one of them is as high as in the body (Word's PDFs of
            // `footer-trailing-empty-probe.docx`: a footer's empty ＭＳ 明朝 10.5 pt line takes 13.6 pt, as its text line does).
            let in_cell = in_table && self.current_cell.get().is_some();
            let name = p.mark.font_for(if in_cell { 'あ' } else { 'A' });
            let font = self.font(&p.mark, name);
            let (ascent, line) = self.line_metrics(name, &font, mark_size);
            // An empty paragraph is as high as a line of text, in a cell too (Word's PDFs of `emptycell-mincho-w41453.docx` and
            // `emptysize-*`: ＭＳ 明朝 10.5 pt, 13.62 pt in the cell and in the body).
            (line, ascent, self.lead_in(p, name) * mark_size)
        };
        let default_line_h = self.resolve_line_height(p, mark_metrics.0, mark_metrics.2, in_table);

        // The shapes this paragraph anchors, when the caller has not worked out what the text goes round (a cell's or a header's paragraph).
        let default_obs: Vec<WrapObstacleInPara> = if obstacles.is_empty() {
            let geometry = FloatGeometry::new(&self.doc.page, width, page_number);
            square_shapes(p).map(|shape| {
                // The paragraph's top is taken as the top margin: a shape set from the page is then in the paragraph's y as `abs_y - margin_t`.
                self.float_box(p, page_number, shape, &geometry, geometry.margin_t, default_line_h).in_paragraph(geometry.margin_t)
            }).collect()
        } else {
            Vec::new()
        };
        let effective_obs: Vec<&WrapObstacleInPara> = if !obstacles.is_empty() { obstacles.iter().collect() } else if !default_obs.is_empty() { default_obs.iter().collect() } else { Vec::new() };

        if !effective_obs.is_empty() {

            #[derive(Clone, Copy)]
            struct Segment {
                s: usize,
                e: usize,
                slot_x: f32,
                slot_w: f32,
                /// The part's edges, as its splitting saw them (a positional tab aligns to them).
                edges: SegmentEdges,
                forced: bool,
                /// Where the characters that stand past the slot's edge begin (`e` for none): decided where the line is split, as in the body's lines.
                hang: usize,
            }

            let mut physical_lines: Vec<Vec<Segment>> = Vec::new();
            // A line the obstacle leaves no room in is not text: the text goes on below the obstacle (its height is how far
            // that is, not a whole line).
            let mut skipped: Vec<Option<f32>> = Vec::new();
            // Room an empty line is pushed down by an obstacle it cannot sit beside (the line itself comes below it).
            let mut extras: Vec<f32> = Vec::new();
            let mut i = 0;
            let mut cur_y = 0.0;
            // What follows a page break is on another page: the obstacle (a
            // floating table's wrap) does not reach it.
            let mut past_page_break = false;
            let mut state = BreakState::default();

            while i < units.len() || physical_lines.is_empty() {
                let first_line = i == 0;
                let line_start = line_start_x(first_line);
                let line_end = width - right;
                let full_room = (line_end - line_start).max(0.0);

                // The obstacles this line's band meets, and where each is on it (a polygon's span for the band), the foot of them.
                let band = (cur_y, cur_y + default_line_h);
                let hits: Vec<(f32, f32, &WrapObstacleInPara)> = if past_page_break {
                    Vec::new()
                } else {
                    effective_obs
                        .iter()
                        .filter(|obs| band.0 < obs.rel_bottom - 0.01 && band.1 > obs.rel_top + 0.01)
                        .filter_map(|obs| match &obs.wrap_polygon {
                            Some((poly, sx, sy, sw, sh, dl, dr)) => poly.x_span_for_y_band(*sx, *sy, *sw, *sh, band.0, band.1).map(|(min_x, max_x)| (min_x - dl, max_x + dr, *obs)),
                            None => Some((obs.shape_left, obs.shape_right, *obs)),
                        })
                        // A shape that lies wholly outside the column does not touch the line, whatever side `wrapText` keeps the text on (Word's
                        // PDF of the issue #191 file: a box in the margin with `wrapText` left leaves the column's first line where it is, as
                        // `bothSides` does, where this sent the line below the box).
                        .filter(|&(l, r, _)| r > line_start + 0.01 && l < line_end - 0.01)
                        .collect()
                };
                let is_intersecting = !hits.is_empty();
                let shape_bottom = hits.iter().map(|h| h.2.rel_bottom).fold(f32::MIN, f32::max);
                // The room the line has between them: what each obstacle leaves (by its `wrapText`) cut down by the others, the gaps that
                // can hold a character kept.
                let slots: Vec<(f32, f32)> = if is_intersecting {
                    let min_slot_w = 15.0;
                    let mut free: Vec<(f32, f32)> = vec![(line_start, line_end)];
                    for &(l, r, obs) in &hits {
                        let left_w = (l - line_start).max(0.0);
                        let right_w = (line_end - r).max(0.0);
                        let allowed: Vec<(f32, f32)> = match obs.wrap_text.as_str() {
                            "left" => vec![(f32::MIN, l)],
                            "right" => vec![(r, f32::MAX)],
                            "largest" => {
                                if right_w >= left_w && right_w >= min_slot_w {
                                    vec![(r, f32::MAX)]
                                } else if left_w >= min_slot_w {
                                    vec![(f32::MIN, l)]
                                } else {
                                    vec![]
                                }
                            }
                            _ /* "bothSides" */ => vec![(f32::MIN, l), (r, f32::MAX)],
                        };
                        free = free
                            .iter()
                            .flat_map(|&(a, b)| allowed.iter().filter_map(move |&(c, d)| (a.max(c) < b.min(d)).then(|| (a.max(c), b.min(d)))))
                            .collect();
                    }
                    free.into_iter().map(|(a, b)| (a, b - a)).filter(|&(_, w)| w >= min_slot_w).collect()
                } else {
                    vec![(line_start, full_room)]
                };

                let mut phys_line: Vec<Segment> = Vec::new();
                let mut line_forced_break = false;

                for (slot_n, &(slot_x, slot_w)) in slots.iter().enumerate() {
                    // The first line's indent counts from the edge of the gap the line starts in: a figure on the left leaves the line's start at the figure's
                    // right edge, and the first line begins the indent from there (Word's PDF of the issue #196 file, `firstLine` 42 pt beside a figure on the
                    // left: the line begins 42 pt right of the gap's edge, 203.1 pt for the gap's 161.1; the same paragraph with the indent that narrows it, 203.0).
                    let (slot_x, slot_w) = if first_line && slot_n == 0 && is_intersecting && slot_x > line_start + 0.01 {
                        let indent = first.max(0.0).min(slot_w);
                        (slot_x + indent, slot_w - indent)
                    } else {
                        (slot_x, slot_w)
                    };
                    if i >= units.len() {
                        break;
                    }
                    // A gap beside the shape that cannot hold even the next character is left
                    // empty (as Word does): the text goes to the other side, or below the shape.
                    if is_intersecting
                        && !matches!(units[i], Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak | Unit::Tab | Unit::ListTab)
                        && unit_width(&units[i]) > slot_w + 0.01
                    {
                        continue;
                    }
                    // A word (letters up to a space) that does not fit the gap whole, where a later gap of the line holds it, goes to that gap whole: Word does not cut it
                    // at the first gap's edge (its PDF of `docx-smartart-word-square.docx`, "SmartArt process (square)" with a figure beside: "SmartArt" is on the right
                    // of the figure on the first line, "process" on its left on the second).
                    if is_intersecting && !matches!(units[i], Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak | Unit::Tab | Unit::ListTab) {
                        let word: f32 = units[i..].iter().map_while(|u| unit_char(u).filter(|c| !c.is_whitespace() && !is_cjk(*c)).map(|_| unit_width(u))).sum();
                        if word > slot_w + 0.01 && slots[slot_n + 1..].iter().any(|&(_, w)| w + 0.01 >= word) {
                            continue;
                        }
                    }
                    let edges = SegmentEdges::new(width, left, right, slot_x, slot_w, line_start, line_end);
                    let (s, e, forced, hang) = self.break_segment(p, &mut units, &mut i, &edges, left, tab_default, &mut state);
                    phys_line.push(Segment { s, e, slot_x, slot_w, edges, forced, hang });
                    if forced {
                        line_forced_break = true;
                    }

                    if line_forced_break {
                        break;
                    }
                }

                let mut extra_above = 0.0;
                if units.is_empty() && is_intersecting && slots.is_empty() && shape_bottom > cur_y + 0.01 {
                    extra_above = shape_bottom - cur_y;
                    cur_y = shape_bottom;
                }
                if phys_line.is_empty() && units.is_empty() {
                    phys_line.push(Segment { s: 0, e: units.len(), slot_x: line_start, slot_w: full_room, edges: SegmentEdges::new(width, left, right, line_start, full_room, line_start, line_end), forced: true, hang: units.len() });
                }

                if phys_line.is_empty() && is_intersecting && i < units.len() && shape_bottom > cur_y + 0.01 {
                    // Text that goes round an outline (`tight`, `through`) comes below it on the first line, counted from the paragraph's top in
                    // whole lines, whose top is not above the outline's foot and its distance below (Word's XPS, issue #191 request 7a: a figure
                    // 100 pt high with no distance, its paragraph's top at 81.2 and lines of 13.56 pt: the first line's top is at 81.2 + 8 lines, not
                    // at the figure's foot, 181.2). `wrapSquare` goes straight under.
                    let reach = hits
                        .iter()
                        .filter(|h| h.2.rel_bottom > cur_y + 0.01)
                        .map(|h| match &h.2.wrap_polygon {
                            Some((poly, _, _, _, sh, _, _)) if *sh > 0.0 => {
                                let foot = poly.all_points().iter().map(|pt| pt.y as f32).fold(f32::MIN, f32::max) / 21_600.0 * sh;
                                (h.2.rel_bottom - sh + foot, true)
                            }
                            _ => (h.2.rel_bottom, false),
                        })
                        .fold((f32::MIN, false), |(a, sa), (b, sb)| (a.max(b), sa || sb));
                    let stop = if reach.1 && default_line_h > 0.5 { cur_y + ((reach.0 - cur_y) / default_line_h - 0.001).ceil().max(1.0) * default_line_h } else { shape_bottom };
                    extras.push(0.0);
                    skipped.push(Some(stop - cur_y));
                    cur_y = stop;
                    physical_lines.push(phys_line);
                    continue;
                }
                extras.push(extra_above);
                skipped.push(None);
                let (text_line, lead) = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Char { c, line, lead, .. } = u { if !c.is_whitespace() { Some((*line, *lead)) } else { None } } else { None }).fold((0.0f32, 0.0f32), |(l, d), (l2, d2)| (l.max(l2), d.max(d2)));
                let (text_line, lead) = if text_line > 0.0 { (text_line, lead) } else { (mark_metrics.0, mark_metrics.2) };
                let picture = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| match u { Unit::Math { h, .. } => Some(*h), _ => None }).fold(0.0, f32::max);
                let pictures = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| match u { Unit::Picture { h, ee_top, ee_bottom, .. } => Some(h + ee_top + ee_bottom), Unit::Shape { h, .. } => Some(*h), _ => None }).fold(0.0, f32::max);
                let mut cut = 0.0;
                let growth = if picture > text_line || pictures <= 0.0 {
                    None
                } else {
                    let ascent = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Char { c, ascent, .. } = u { if !c.is_whitespace() { Some(*ascent) } else { None } } else { None }).fold(0.0f32, f32::max);
                    let ms_room = self.ms_room_of(p, phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]));
                    let growth = self.picture_growth(p, text_line, ascent, lead, in_table, ms_room, pictures);
                    if let Some(g) = growth {
                        let line_units: Vec<Unit> = phys_line.iter().flat_map(|seg| units[seg.s..seg.e].iter().cloned()).collect();
                        cut = self.picture_only_cut(p, &line_units, text_line, ascent, lead, in_table, ms_room, g);
                    }
                    growth
                };
                let picture = if growth.is_some() { 0.0 } else { picture.max(pictures) };
                let growth = growth.unwrap_or(0.0);
                let natural = text_line.max(picture);
                let lead = if picture > text_line { 0.0 } else { lead };
                let single = if picture > text_line { 0.0 } else { self.single_line_of(p, phys_line.iter().flat_map(|seg| &units[seg.s..seg.e])) };
                let height = self.line_height_around(p, text_line, picture, natural, lead, in_table, single) + growth - cut;
                cur_y += height;
                if phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).any(|u| matches!(u, Unit::PageBreak)) {
                    past_page_break = true;
                }

                physical_lines.push(phys_line);
                if i >= units.len() {
                    break;
                }
            }

            if !carried.is_empty() {
                let starts: Vec<usize> = physical_lines.iter().filter_map(|l| l.first().map(|seg| seg.s)).collect();
                self.redraw_lines_after_the_first(p, page_number, width, &mut units, &starts);
            }
            let mut out = Vec::new();
            for (n, phys_line) in physical_lines.iter().enumerate() {
                let (text_line, text_ascent, lead) = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Char { c, line, ascent, lead, style, size, .. } = u { if c.is_whitespace() { return None; } let (line, ascent) = self.mark_room(p, *c, *style, *size, *line, *ascent); Some((line, ascent, *lead)) } else { None }).fold((0.0f32, 0.0f32, 0.0f32), |(l, a, d), (l2, a2, d2)| (l.max(l2), a.max(a2), d.max(d2)));
            // A line with raised or lowered text (`w:position`): its room below is the largest of the runs' own, not the largest line less the largest ascent (a raised run's
            // is short of its raise, the line's others' are not): Word's `D' = max(D - r)`.
            let (text_line, text_ascent) = if phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).any(|u| matches!(u, Unit::Char { style, .. } if *style != LABEL && p.runs.get(*style).is_some_and(|r| r.style.raise != 0))) {
                let rooms = || phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Char { c, line, ascent, style, size, .. } = u { if c.is_whitespace() { return None; } let (line, ascent) = self.mark_room(p, *c, *style, *size, *line, *ascent); Some((ascent, line - ascent)) } else { None });
                // (A line's room above can be under 0 here: a lowered run's, which Word clamps to 0 at the run and puts its text by the lowering from there.)
                let above = rooms().map(|r| r.0).fold(f32::NEG_INFINITY, f32::max);
                (above + rooms().map(|r| r.1).fold(0.0f32, f32::max), above)
            } else {
                (text_line, self.ms_mixed_ascent(p, phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]), text_ascent))
            };
                let (text_line, text_ascent, lead) = if text_line > 0.0 { (text_line, text_ascent, lead) } else { mark_metrics };
                let picture = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| match u { Unit::Math { h, .. } => Some(*h), _ => None }).fold(0.0, f32::max);
                let pictures = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| match u { Unit::Picture { h, ee_top, ee_bottom, .. } => Some(h + ee_top + ee_bottom), Unit::Shape { h, .. } => Some(*h), _ => None }).fold(0.0, f32::max);
                let mut cut = 0.0;
                let growth = if picture > text_line || pictures <= 0.0 {
                    None
                } else {
                    let ms_room = self.ms_room_of(p, phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]));
                    let growth = self.picture_growth(p, text_line, text_ascent, lead, in_table, ms_room, pictures);
                    if let Some(g) = growth {
                        let line_units: Vec<Unit> = phys_line.iter().flat_map(|seg| units[seg.s..seg.e].iter().cloned()).collect();
                        cut = self.picture_only_cut(p, &line_units, text_line, text_ascent, lead, in_table, ms_room, g);
                    }
                    growth
                };
                let picture = if growth.is_some() { 0.0 } else { picture.max(pictures) };
                let growth = growth.unwrap_or(0.0);
                let natural = text_line.max(picture);
                let lead = if picture > text_line { 0.0 } else { lead };
                let single = if picture > text_line { 0.0 } else { self.single_line_of(p, phys_line.iter().flat_map(|seg| &units[seg.s..seg.e])) };
                let height = self.line_height_around(p, text_line, picture, natural, lead, in_table, single) + growth - cut;
                let grid_slack = self.picture_grid_slack(p, text_line, picture, in_table, height);
                let (mark_grow, mark_dy) = self.emphasis_adjust(p, phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]), height, self.line_height_parts(p, natural, lead, in_table).1);
                let height = height + mark_grow;
                let height = skipped.get(n).copied().flatten().unwrap_or(height + extras.get(n).copied().unwrap_or(0.0));
                let math_below = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Math { descent, .. } = u { Some(*descent) } else { None }).fold(0.0f32, f32::max);
                let baseline = if picture > text_line { height - math_below - grid_slack } else { self.baseline_in_line(p, natural, lead, text_line, text_ascent, in_table, self.ms_room_of(p, phys_line.iter().flat_map(|seg| &units[seg.s..seg.e])), single) + growth + mark_dy };
                let page_break = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).any(|u| matches!(u, Unit::PageBreak));
                let column_break = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).any(|u| matches!(u, Unit::ColumnBreak));
                let has_revision = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).any(|u| {
                    if let Unit::Char { style, .. } = u {
                        if *style != LABEL {
                            p.runs.get(*style).is_some_and(|r| r.revision.as_ref().is_some_and(|rev| self.res.markup_view.is_reviewer_shown(&rev.author)))
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                });
                let mut comment_marks = Vec::new();
                for u in phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]) {
                    if let Unit::Char { style, .. } = u {
                        if *style != LABEL {
                            if let Some(c_ids) = self.comment_runs.get(&(p.id, *style)) {
                                for &id in c_ids {
                                    if !comment_marks.contains(&id) {
                                        comment_marks.push(id);
                                    }
                                }
                            }
                            if let Some(r) = p.runs.get(*style) {
                                match r.kind {
                                    FlowRunKind::CommentStart(id) | FlowRunKind::CommentRef(id) => {
                                        if !comment_marks.contains(&id) {
                                            comment_marks.push(id);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                let mut line = LineBox { height, baseline, text_dy: if picture > text_line { 0.0 } else { self.text_dy(p, natural, lead, in_table) }, grid_below: if picture > text_line { 0.0 } else { self.grid_room_below(p, natural, lead, in_table, height, baseline, text_line, text_ascent) }, items: Vec::new(), page_break, column_break, has_revision, comment_marks };

                if n == 0 {
                    for obs in &effective_obs {
                        if let Some((shape, sx, sy, sw, sh, behind_doc)) = &obs.shape_to_emit {
                            let item = Item::Shape {
                                shape: Box::new((*shape).clone()),
                                x: *sx,
                                y: *sy,
                                w: *sw,
                                h: *sh,
                            };
                            if *behind_doc {
                                line.items.insert(0, item);
                            } else {
                                line.items.push(item);
                            }
                        }
                    }
                }

                for (seg_idx, seg) in phys_line.iter().enumerate() {
                    let slice = &units[seg.s..seg.e];
                    let visible_end = slice[..seg.hang - seg.s].iter().rposition(|u| unit_char(u).is_none_or(|c| c != ' ')).map_or(0, |p| p + 1);
                    let used: f32 = slice[..visible_end].iter().map(unit_width).sum();
                    let last = (n + 1 == physical_lines.len() && seg_idx + 1 == phys_line.len()) || seg.forced;
                    let justify = match p.align {
                        // The last line of a paragraph is not spread, but one that took its last word by narrowing its spaces (above) is drawn narrowed (Word's PDF of corpus 124:
                        // "… The License Original page URL)" ends the paragraph at the right margin, its spaces 0.35 pt short each).
                        FlowAlign::Justify => !last || used > seg.slot_w + 0.01,
                        FlowAlign::Distribute => true,
                        _ => false,
                    };
                    let has_tab = slice.iter().any(|u| matches!(u, Unit::Tab | Unit::PositionalTab { .. }));
                    let has_ptab = slice.iter().any(|u| matches!(u, Unit::PositionalTab { .. }));
                    let marks = slice[..visible_end].iter().filter(|u| unit_char(u) == Some('\u{200b}')).count();
                    let room = seg.slot_w;
                    let extras = if justify && !has_tab && (used < room || p.align == FlowAlign::Justify) { spread_extras_past_number(&slice[..visible_end], seg.slot_x, left, room, used, p.align == FlowAlign::Distribute, marks) } else { Vec::new() };
                    // A displayed formula alone on its line is centred.
                    let align = if slice.iter().any(|u| matches!(u, Unit::Math { display: true, .. })) && slice.iter().all(|u| !matches!(u, Unit::Char { c, .. } if !c.is_whitespace())) { FlowAlign::Center } else { p.align };
                    let shift = if has_ptab {
                        0.0
                    } else {
                        match align {
                            FlowAlign::Center => ((room - used) / 2.0).max(0.0),
                            FlowAlign::Right => (room - if last { slice.iter().map(unit_width).sum::<f32>() } else { used }).max(0.0),
                            FlowAlign::Left if p.bidi => (room - used).max(0.0),
                            _ => 0.0,
                        }
                    };
                    let mut x = seg.slot_x + shift;
                    let line_x = x;
                    let mut stops: Vec<(usize, f32)> = Vec::new();
                    let mut list_tab: Option<(f32, f32)> = None;
                    let mut run: Option<OpenRun> = None;
                    let mut dec = LineDecorations::default();
                    let flush = |run: &mut Option<OpenRun>, line: &mut LineBox, dec: &mut LineDecorations| {
                        if let Some((x0, text, offsets, font, size, style, squeeze)) = run.take() {
                            let s = if style == LABEL { &p.list_style } else { &p.runs[style].style };
                            let revised;
                            let is_all_markup = self.markup_mode() == MarkupDisplayMode::All;
                            let is_hidden_dotted = p.runs.get(style).is_some_and(|r| r.style.hidden && !self.res.print && self.res.markup_view.show_hidden_text && style != LABEL);
                            let active_rev = p.runs.get(style).and_then(|r| r.revision.as_ref()).filter(|r| self.res.markup_view.is_reviewer_shown(&r.author));
                            let s = match active_rev.map(|r| &r.kind).filter(|_| is_all_markup) {
                                Some(ibis_doc_model::RevisionKind::Insert) if style != LABEL => {
                                    revised = ibis_doc_model::FlowRunStyle { underline: true, underline_style: ibis_doc_model::UnderlineStyle::Single, color: REVISION_COLOR, ..s.clone() };
                                    &revised
                                }
                                Some(ibis_doc_model::RevisionKind::Delete) if style != LABEL => {
                                    revised = ibis_doc_model::FlowRunStyle { strike: true, color: REVISION_COLOR, ..s.clone() };
                                    &revised
                                }
                                Some(ibis_doc_model::RevisionKind::MoveTo) if style != LABEL => {
                                    revised = ibis_doc_model::FlowRunStyle { underline: true, underline_style: ibis_doc_model::UnderlineStyle::Single, color: MOVE_REVISION_COLOR, ..s.clone() };
                                    &revised
                                }
                                Some(ibis_doc_model::RevisionKind::MoveFrom) if style != LABEL => {
                                    revised = ibis_doc_model::FlowRunStyle { strike: true, color: MOVE_REVISION_COLOR, ..s.clone() };
                                    &revised
                                }
                                _ if is_hidden_dotted => {
                                    revised = ibis_doc_model::FlowRunStyle { underline: true, underline_style: ibis_doc_model::UnderlineStyle::Dotted, ..s.clone() };
                                    &revised
                                }
                                _ => s,
                            };
                            let s_size = { let full = s.enclosure_sizes.map_or(s.size, |(ring, _)| ring) as f32 / 100.0; if s.position == VerticalPosition::Baseline { full } else { self.script_size(text.chars().next().and_then(|c| s.font_for(c)).or_else(|| s.font_for('あ')), full, s.position) } };
                            let raise = match s.position {
                                VerticalPosition::Superscript => -self.script_shift(text.chars().next().and_then(|c| s.font_for(c)).or_else(|| s.font_for('あ')), s.size as f32 / 100.0, s_size, s.position),
                                VerticalPosition::Subscript => self.script_shift(text.chars().next().and_then(|c| s.font_for(c)).or_else(|| s.font_for('あ')), s.size as f32 / 100.0, s_size, s.position),
                                VerticalPosition::Baseline => 0.0,
                            } - s.raise as f32 / 2.0;
                            let font_name = text.chars().next().and_then(|c| s.font_for(c)).or_else(|| s.font_for('あ'));
                            let (run_ascent, run_line) = self.line_metrics(font_name, &font, s_size);
                            let run_descent = run_line - run_ascent;
                            let align_shift = match p.text_alignment {
                                ibis_doc_model::FontAlign::Top => run_ascent - text_ascent,
                                ibis_doc_model::FontAlign::Center => ((run_ascent - run_descent) - (text_ascent - (text_line - text_ascent))) / 2.0,
                                ibis_doc_model::FontAlign::Bottom => (text_line - text_ascent) - run_descent,
                                ibis_doc_model::FontAlign::Baseline | ibis_doc_model::FontAlign::Auto => 0.0,
                            };
                            let y_base = line.baseline + line.text_dy + raise + align_shift;
                            let end = offsets.last().copied().unwrap_or(0.0);
                            let show_comments = self.markup_mode() != MarkupDisplayMode::None && self.markup_mode() != MarkupDisplayMode::Original;
                            if show_comments && style != LABEL && end > 0.0 {
                                if let Some(c_ids) = self.comment_runs.get(&(p.id, style)) && !c_ids.is_empty() {
                                    let color = hsla(COMMENT_HIGHLIGHT_COLOR);
                                    if let Some(Item::Rect { x: last_x, w: last_w, color: last_c, .. }) = dec.highlights.last_mut()
                                        && *last_c == color
                                        && (*last_x + *last_w - x0).abs() < 0.02
                                    {
                                        *last_w = (x0 + end) - *last_x;
                                    } else {
                                        dec.highlights.push(Item::Rect { x: x0, y: 0.0, w: end, h: line.height, color });
                                    }
                                    let comment_ucolor = hsla(ibis_doc_model::Color::opaque(COMMENT_BUBBLE_ICON));
                                    if let Some(last_u) = dec.underlines.last_mut()
                                        && last_u.style == UnderlineStyle::Dash
                                        && last_u.color == comment_ucolor
                                        && (last_u.baseline - y_base).abs() < 0.01
                                        && (last_u.size - s_size).abs() < 0.01
                                        && (last_u.x + last_u.w - x0).abs() < 0.02
                                    {
                                        last_u.w = (x0 + end) - last_u.x;
                                    } else {
                                        dec.underlines.push(UnderlineBox {
                                            style: UnderlineStyle::Dash,
                                            size: s_size,
                                            x: x0,
                                            w: end,
                                            baseline: y_base,
                                            color: comment_ucolor,
                                        band: None,
                                        });
                                    }
                                }
                            }
                            if let Some(h) = s.highlight && end > 0.0 {
                                let color = hsla(ibis_doc_model::Color::opaque(h));
                                if let Some(Item::Rect { x: last_x, w: last_w, color: last_c, .. }) = dec.highlights.last_mut()
                                    && *last_c == color
                                    && (*last_x + *last_w - x0).abs() < 0.02
                                {
                                    *last_w = (x0 + end) - *last_x;
                                } else {
                                    dec.highlights.push(Item::Rect { x: x0, y: 0.0, w: end, h: line.height, color });
                                }
                                // A stripe or a grid over the fill.
                                if let Some((pattern, pattern_color)) = s.shd_pattern {
                                    dec.highlights.push(Item::Shading { pattern, color: hsla(ibis_doc_model::Color::opaque(pattern_color)), x: x0, y: 0.0, w: end, h: line.height });
                                }
                            }
                            if let Some(ref b) = s.border && end > 0.0 && !text.chars().all(char::is_whitespace) {
                                let run_start = x0;
                                let run_end = x0 + end;
                                let (y_top, y_bottom) = self.border_box_y(s, font_name, y_base, raise, run_ascent, run_line, s_size, b);
                                if let Some(last) = dec.borders.last_mut()
                                    && last.border == *b
                                    && (run_start - last.x_end).abs() < 2.0
                                {
                                    last.x_end = run_end;
                                    last.y_top = last.y_top.min(y_top);
                                    last.y_bottom = last.y_bottom.max(y_bottom);
                                } else {
                                    dec.borders.push(BorderBox {
                                        border: *b,
                                        x_start: run_start,
                                        x_end: run_end,
                                        y_top,
                                        y_bottom,
                                    });
                                }
                            }
                            let color = hsla(ibis_doc_model::Color::opaque(s.color));
                            if s.underline && end > 0.0 {
                                let ucolor = s.underline_color.map_or(color, |c| hsla(ibis_doc_model::Color::opaque(c)));
                                let mut spans = Vec::new();
                                if s.underline_style == UnderlineStyle::Words {
                                    let mut start: Option<f32> = None;
                                    for (i, c) in text.chars().enumerate() {
                                        let (a, z) = (offsets[i], offsets[i + 1]);
                                        match (c.is_whitespace(), start) {
                                            (false, None) => start = Some(a),
                                            (true, Some(st)) => {
                                                spans.push((st, a));
                                                start = None;
                                            }
                                            _ => {}
                                        }
                                        if i + 2 == offsets.len() && let Some(st) = start {
                                            spans.push((st, z));
                                        }
                                    }
                                } else {
                                    spans.push((0.0, end));
                                }
                                let u_size = s_size;
                let u_band = self.underline_band_of(font_name, s.bold, u_size);
                                for (a, z) in spans {
                                    let ux = x0 + a;
                                    let uw = z - a;
                                    if let Some(last_u) = dec.underlines.last_mut()
                                        && last_u.style == s.underline_style
                                        && last_u.color == ucolor
                                        && (last_u.baseline - y_base).abs() < 0.01
                                        && u_band.is_some() == last_u.band.is_some()
                                        && (u_band.is_some() || (last_u.size - u_size).abs() < 0.01)
                                        && (last_u.x + last_u.w - ux).abs() < 0.02
                                    {
                                        last_u.extend(ux, uw, u_band);
                                    } else {
                                        dec.underlines.push(UnderlineBox {
                                            style: s.underline_style,
                                            size: u_size,
                                            x: ux,
                                            w: uw,
                                            baseline: y_base,
                                            color: ucolor,
                                            band: u_band,
                                        });
                                    }
                                }
                            }
                            if (s.strike || s.double_strike) && end > 0.0 {
                                let strike_size = s_size;
                                let lines: Vec<(f32, f32)> = self.strike_lines_of(font_name, s.bold, strike_size, s.double_strike, y_base);
                                for (sy, sh) in lines {
                                    if let Some(last_s) = dec.strikes.iter_mut().rev().find(|l| (l.y - sy).abs() < 0.01)
                                        && last_s.color == color
                                        && (last_s.h - sh).abs() < 0.01
                                        && (last_s.x + last_s.w - x0).abs() < 0.02
                                    {
                                        last_s.w = (x0 + end) - last_s.x;
                                    } else {
                                        dec.strikes.push(StrikeBox { x: x0, y: sy, w: end, h: sh, color });
                                    }
                                }
                            }
                            line.items.extend(self.emphasis_marks(s, style == LABEL || is_ruby_base(p, style), x0, y_base, s_size, &text, &offsets, color));
                            if let Some(run) = p.runs.get(style).filter(|_| style != LABEL) {
                                line.items.extend(self.enclosed_items(s, run, x0, y_base, end, s_size, color));
                                line.items.extend(self.overstrike_items(s, run, x0, y_base, end, s_size, color));
                            }
                            if let Some(run) = p.runs.get(style).filter(|_| style != LABEL) {
                                line.items.extend(self.ruby_items(s, &run.kind, x0, y_base, end, color));
                            }
                            let combined = p.runs.get(style).filter(|_| style != LABEL).and_then(|r| r.style.combine);
                            if let Some(c) = combined {
                                line.items.extend(self.combine_items(s, c, &text, x0, y_base, s_size, color, self.combine_brackets_of(p, style, &text)));
                            }
                            let is_tate = p.runs.get(style).filter(|_| style != LABEL).is_some_and(|r| r.style.tate_chu_yoko);
                            if is_tate && let Some(run) = p.runs.get(style) {
                                line.items.extend(self.tate_chu_yoko_items(&run.style, &text, x0, y_base, s_size, &font, color));
                            }
                            let mut offsets = offsets;
                            offsets.pop();
                            if combined.is_none() && !is_tate {
                                let before = line.items.len();
                                push_styled_text_items(&mut line.items, s, x0, y_base, text, offsets, font, size, color);
                                squeeze_items(&mut line.items, before, squeeze);
                            }
                        }
                    };
                    for (k, u) in slice.iter().enumerate() {
                        if let Some(&(position, true)) = positions.get(seg.s + k) {
                            stops.push((position, x - line_x));
                        }
                        match u {
                            Unit::Char { c, width, inset, font, size, style, squeeze, .. } => {
                                // A 縦中横 run is drawn whole, centred as one (its digits may be squeezed one by one to their cell, which would split it).
                                let tate_run = *style != LABEL && p.runs.get(*style).is_some_and(|r| r.style.tate_chu_yoko);
                                // A combining mark with a cell of its own (`own_cell_mark`) is its own item, drawn where its cell is, not with its base.
                                let own_cell = *width > 0.0 && crate::render::rich_text::is_font_spaced_mark(*c);
                                let same = !own_cell && run.as_ref().is_some_and(|r| r.5 == *style && (tate_run || (r.3 == *font && r.4 == *size && r.6 == *squeeze)));
                                if !same {
                                    flush(&mut run, &mut line, &mut dec);
                                    run = Some((x, String::new(), vec![0.0], font.clone(), *size, *style, *squeeze));
                                }
                                let r = run.as_mut().expect("a run");
                                r.1.push(*c);
                                let at = x - r.0 + inset;
                                r.2.pop();
                                r.2.push(at);
                                x += width + if *c != '\u{200b}' && *style != LABEL { extras.get(k).copied().unwrap_or(0.0) } else { 0.0 };
                                r.2.push(x - r.0);
                            }
                            Unit::ListTab if left > x + 0.01 => {
                                flush(&mut run, &mut line, &mut dec);
                                list_tab = Some((x - line_x, left - line_x));
                                x = left;
                            }
                            Unit::Tab | Unit::ListTab => {
                                flush(&mut run, &mut line, &mut dec);
                                let stop_list: Vec<_> = p.tabs.iter().filter(|t| pt(t.position) > x + 0.01).collect();
                                let from = x;
                                match stop_list.first() {
                                    Some(t) if matches!(t.align, TabAlign::Right | TabAlign::Center) => {
                                        let rest: f32 = slice[k + 1..].iter().take_while(|u| !matches!(u, Unit::Tab)).map(unit_width).sum();
                                        let stop = pt(t.position);
                                        x = if t.align == TabAlign::Right { stop - rest } else { stop - rest / 2.0 }.max(x);
                                    }
                                    Some(t) => x = pt(t.position),
                                    None => x = ((x / tab_default).floor() + 1.0) * tab_default,
                                }
                                if matches!(u, Unit::ListTab) {
                                    list_tab = Some((from - line_x, x - line_x));
                                }
                                if stop_list.first().is_some_and(|t| t.leader.is_some()) {
                                    let size = p.runs.first().map_or(10.5, |r| r.style.size as f32 / 100.0);
                                    let (step, dot) = (size * 0.3, (size * 0.08).max(0.5));
                                    let color = hsla(ibis_doc_model::Color::opaque(p.runs.first().map_or(ibis_doc_model::Rgb::BLACK, |r| r.style.color)));
                                    let mut d = (from / step).ceil() * step + step / 2.0;
                                    while d + dot < x - step / 2.0 {
                                        line.items.push(Item::Rect { x: d, y: line.baseline - dot, w: dot, h: dot, color });
                                        d += step;
                                    }
                                }
                            }
                            Unit::PositionalTab { align, relative_to_margin, leader } => {
                                flush(&mut run, &mut line, &mut dec);
                                let from = x;
                                let rest: f32 = slice[k + 1..].iter().take_while(|u| !matches!(u, Unit::Tab | Unit::PositionalTab { .. })).map(unit_width).sum();
                                let (span_l, span_r) = seg.edges.span(*align, *relative_to_margin);
                                let target = match align {
                                    TabAlign::Right => span_r - rest,
                                    TabAlign::Center => (span_l + span_r - rest) / 2.0,
                                    _ => span_l,
                                };
                                x = target.max(x);
                                if *leader {
                                    let size = p.runs.first().map_or(10.5, |r| r.style.size as f32 / 100.0);
                                    let (step, dot) = (size * 0.3, (size * 0.08).max(0.5));
                                    let color = hsla(ibis_doc_model::Color::opaque(p.runs.first().map_or(ibis_doc_model::Rgb::BLACK, |r| r.style.color)));
                                    let mut d = (from / step).ceil() * step + step / 2.0;
                                    while d + dot < x - step / 2.0 {
                                        line.items.push(Item::Rect { x: d, y: line.baseline - dot, w: dot, h: dot, color });
                                        d += step;
                                    }
                                }
                            }
                            Unit::Picture { media, w, h, ee_bottom, .. } => {
                                flush(&mut run, &mut line, &mut dec);
                                line.items.push(Item::Picture { media: *media, x, y: line.baseline - h - ee_bottom, w: *w, h: *h });
                                x += w;
                            }
                            Unit::Math { boxed, w, font, color, .. } => {
                                flush(&mut run, &mut line, &mut dec);
                                let items = self.math_items(boxed, x, line.baseline, font, *color);
                                line.items.extend(items);
                                x += w;
                            }
                            Unit::Shape { shape, w, h } => {
                                flush(&mut run, &mut line, &mut dec);
                                // A shape the text goes round is drawn once, with the paragraph's first line (`shape_to_emit`); drawing it here as
                                // well put it on the page twice (GitHub issue #191).
                                let drawn_with_paragraph = shape.floating.is_some() && effective_obs.iter().any(|o| o.shape_to_emit.as_ref().is_some_and(|(s, ..)| s.id == shape.id));
                                if !drawn_with_paragraph {
                                    self.emit_unit_shape(shape, w, h, &mut x, width, &mut line, page_number);
                                }
                            }
                            Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak => {}
                        }
                    }
                    flush(&mut run, &mut line, &mut dec);
                    dec.finish(&mut line, self.vertical.get());
                    let end = positions.get(seg.e).map_or(p.len(), |&(position, _)| position);
                    let avail = (seg.slot_x + seg.slot_w - line_x).max(0.0);
                    stops.push((end, if matches!(slice.last(), Some(Unit::PageBreak | Unit::ColumnBreak)) { avail.max(x - line_x) } else { x - line_x }));
                    line.items.push(Item::Line { para: p.id, x: line_x, y: 0.0, h: line.height, stops: stops.into(), vertical: false, list_tab, base: line.baseline + line.text_dy, avail, pad: (0.0, 0.0), largest: slice.iter().filter_map(|u| if let Unit::Char { c, metric, .. } = u { (!c.is_whitespace()).then_some(*metric) } else { None }).fold(0.0, f32::max) });
                }
                out.push(line);
            }
            return out;
        }

        // Split into lines: (start, end, forced end, hang start). The splitting alone decides which characters of the line stand past the
        // right edge (a hanging space or stop, below): the line keeps where they start (`hang_start`, `end` when none) and the drawing
        // below counts the line up to there; it never judges by a character's kind again (E6: the two had parted, a justified line
        // ending in full-width spaces was drawn narrowed).
        let mut lines: Vec<(usize, usize, bool, usize)> = Vec::new();
        let mut i = 0;
        let mut state = BreakState::default();
        while i < units.len() {
            let origin = line_start_x(lines.is_empty());
            let line_end = width - right;
            let edges = SegmentEdges::new(width, left, right, origin, line_end - origin, origin, line_end);
            let seg = self.break_segment(p, &mut units, &mut i, &edges, left, tab_default, &mut state);
            lines.push(seg);
        }
        let start = i;
        let mut hang: Option<usize> = None;
        // A paragraph that ends with a page break has its mark on the next page, an empty line there, with `splitPgBreakAndParaMark` below
        // mode 15 (Word's PDFs of `split-page-break-mark-*-probe.docx`: a page break then an empty paragraph that ends a section leaves a blank
        // page in mode 11 with the flag, as Word lays out the same document saved as a `.doc`, and none without it or in mode 15).
        let mark_after_break = self.doc.split_pg_break_and_para_mark
            && self.doc.compat_mode.is_none_or(|m| m < 15)
            && start == units.len()
            && matches!(units.last(), Some(Unit::PageBreak));
        // A paragraph that ends with a line break has its mark on a line of its own below (Word: the caret goes there, the mark with it).
        let mark_after_line_break = start == units.len() && matches!(units.last(), Some(Unit::LineBreak));
        if start < units.len() || lines.is_empty() || mark_after_break || mark_after_line_break {
            lines.push((start, units.len(), true, hang_start(hang.take(), units.len())));
            if let Some(Unit::Char { width, auto_space, .. }) = units.last_mut() {
                *width -= *auto_space;
                *auto_space = 0.0;
            }
        }

        let mark_size = p.mark.size as f32 / 100.0;
        let mark_metrics = {
            // A header's or a footer's paragraphs are laid out as a cell's (`in_table`), but an empty one of them is as high as in the body (Word's PDFs of
            // `footer-trailing-empty-probe.docx`: a footer's empty ＭＳ 明朝 10.5 pt line takes 13.6 pt, as its text line does).
            let in_cell = in_table && self.current_cell.get().is_some();
            let name = p.mark.font_for(if in_cell { 'あ' } else { 'A' });
            let font = self.font(&p.mark, name);
            let (ascent, line) = self.line_metrics(name, &font, mark_size);
            // An empty paragraph is as high as a line of text, in a cell too (Word's PDFs of `emptycell-mincho-w41453.docx` and
            // `emptysize-*`: ＭＳ 明朝 10.5 pt, 13.62 pt in the cell and in the body).
            (line, ascent, self.lead_in(p, name) * mark_size)
        };
        if !carried.is_empty() {
            let starts: Vec<usize> = lines.iter().map(|l| l.0).collect();
            self.redraw_lines_after_the_first(p, page_number, width, &mut units, &starts);
        }
        let mut out = Vec::new();
        for (n, &(s, e, forced, hang_at)) in lines.iter().enumerate() {
            let slice = &units[s..e];
            // The line's text height and ascent: the tallest typeface's (the
            // paragraph mark's on an empty line).
            let (text_line, text_ascent, lead) = slice.iter().filter_map(|u| if let Unit::Char { c, line, ascent, lead, style, size, .. } = u { if c.is_whitespace() { return None; } let (line, ascent) = self.mark_room(p, *c, *style, *size, *line, *ascent); Some((line, ascent, *lead)) } else { None }).fold((0.0f32, 0.0f32, 0.0f32), |(l, a, d), (l2, a2, d2)| (l.max(l2), a.max(a2), d.max(d2)));
            // A line with raised or lowered text (`w:position`): its room below is the largest of the runs' own, not the largest line less the largest ascent (a raised run's
            // is short of its raise, the line's others' are not): Word's `D' = max(D - r)`.
            let (text_line, text_ascent) = if slice.iter().any(|u| matches!(u, Unit::Char { style, .. } if *style != LABEL && p.runs.get(*style).is_some_and(|r| r.style.raise != 0))) {
                let rooms = || slice.iter().filter_map(|u| if let Unit::Char { c, line, ascent, style, size, .. } = u { if c.is_whitespace() { return None; } let (line, ascent) = self.mark_room(p, *c, *style, *size, *line, *ascent); Some((ascent, line - ascent)) } else { None });
                // (A line's room above can be under 0 here: a lowered run's, which Word clamps to 0 at the run and puts its text by the lowering from there.)
                let above = rooms().map(|r| r.0).fold(f32::NEG_INFINITY, f32::max);
                (above + rooms().map(|r| r.1).fold(0.0f32, f32::max), above)
            } else {
                (text_line, self.ms_mixed_ascent(p, slice.iter(), text_ascent))
            };
            let (text_line, text_ascent, lead) = if text_line > 0.0 { (text_line, text_ascent, lead) } else { mark_metrics };
            let picture = slice.iter().filter_map(|u| match u { Unit::Math { h, .. } => Some(*h), _ => None }).fold(0.0, f32::max);
            let pictures = slice.iter().filter_map(|u| match u { Unit::Picture { h, ee_top, ee_bottom, .. } => Some(h + ee_top + ee_bottom), Unit::Shape { h, .. } => Some(*h), _ => None }).fold(0.0, f32::max);
            let mut cut = 0.0;
            let growth = if picture > text_line || pictures <= 0.0 {
                None
            } else {
                let ms_room = self.ms_room_of(p, slice.iter());
                let growth = self.picture_growth(p, text_line, text_ascent, lead, in_table, ms_room, pictures);
                if let Some(g) = growth {
                    cut = self.picture_only_cut(p, slice, text_line, text_ascent, lead, in_table, ms_room, g);
                }
                growth
            };
            let picture = if growth.is_some() { 0.0 } else { picture.max(pictures) };
            let growth = growth.unwrap_or(0.0);
            let natural = text_line.max(picture);
            let lead = if picture > text_line { 0.0 } else { lead };
            let single = if picture > text_line { 0.0 } else { self.single_line_of(p, slice.iter()) };
            let height = self.line_height_around(p, text_line, picture, natural, lead, in_table, single) + growth - cut;
            let grid_slack = self.picture_grid_slack(p, text_line, picture, in_table, height);
            let (mark_grow, mark_dy) = self.emphasis_adjust(p, slice.iter(), height, self.line_height_parts(p, natural, lead, in_table).1);
            let height = height + mark_grow;
            // Where Word puts the baseline in a line: see `baseline_in_line`.
            let math_below = slice.iter().filter_map(|u| if let Unit::Math { descent, .. } = u { Some(*descent) } else { None }).fold(0.0f32, f32::max);
            let baseline = if picture > text_line { height - math_below - grid_slack } else { self.baseline_in_line(p, natural, lead, text_line, text_ascent, in_table, self.ms_room_of(p, slice.iter()), single) + growth + mark_dy };
            let start_x = line_start_x(n == 0);
            let room = width - right - start_x;
            // Width used (without trailing spaces), and the gaps to spread
            // justification over.
            // What counts of the line: up to where the characters standing past the edge start, without its trailing spaces.
            let visible_end = slice[..hang_at - s].iter().rposition(|u| unit_char(u).is_none_or(|c| c != ' ')).map_or(0, |p| p + 1);
            let used: f32 = slice[..visible_end].iter().map(unit_width).sum();
            let last = n + 1 == lines.len() || forced;
            let justify = match p.align {
                // The last line of a paragraph is not spread, but one that took its last word by narrowing its spaces (above) is drawn narrowed (Word's PDF of corpus 124:
                // "… The License Original page URL)" ends the paragraph at the right margin, its spaces 0.35 pt short each).
                FlowAlign::Justify => !last || used > room + 0.01,
                FlowAlign::Distribute => true,
                _ => false,
            };
            let has_tab = slice.iter().any(|u| matches!(u, Unit::Tab | Unit::PositionalTab { .. }));
            let has_ptab = slice.iter().any(|u| matches!(u, Unit::PositionalTab { .. }));
            let marks = slice[..visible_end].iter().filter(|u| unit_char(u) == Some('\u{200b}')).count();
            // The characters that hang past the edge of the line (the splitting's `hang`) are not spread over the room: the characters before
            // them are, and they follow the last of them past the edge (Word's PDFs of `kinsoku-both-normal-probe.docx`: 46 kana and a 。
            // that hangs keep their pitch, GitHub issue #68 D4).
            let extras = if justify && !has_tab && (used < room || p.align == FlowAlign::Justify) { spread_extras_past_number(&slice[..visible_end], line_start_x(n == 0), left, room, used, p.align == FlowAlign::Distribute, marks) } else { Vec::new() };
            // A displayed formula alone on its line is centred.
            let align = if slice.iter().any(|u| matches!(u, Unit::Math { display: true, .. })) && slice.iter().all(|u| !matches!(u, Unit::Char { c, .. } if !c.is_whitespace())) { FlowAlign::Center } else { p.align };
            let shift = if has_ptab {
                0.0
            } else {
                match align {
                    FlowAlign::Center => ((room - used) / 2.0).max(0.0),
                    FlowAlign::Right => (room - if last { slice.iter().map(unit_width).sum::<f32>() } else { used }).max(0.0),
                    FlowAlign::Left if p.bidi => (room - used).max(0.0),
                    _ => 0.0,
                }
            };
            let has_revision = slice.iter().any(|u| {
                if let Unit::Char { style, .. } = u {
                    if *style != LABEL {
                        p.runs.get(*style).is_some_and(|r| r.revision.as_ref().is_some_and(|rev| self.res.markup_view.is_reviewer_shown(&rev.author)))
                    } else {
                        false
                    }
                } else {
                    false
                }
            });
            let mut comment_marks = Vec::new();
            for u in slice {
                if let Unit::Char { style, .. } = u {
                    if *style != LABEL {
                        if let Some(c_ids) = self.comment_runs.get(&(p.id, *style)) {
                            for &id in c_ids {
                                if !comment_marks.contains(&id) {
                                    comment_marks.push(id);
                                }
                            }
                        }
                        if let Some(r) = p.runs.get(*style) {
                            match r.kind {
                                FlowRunKind::CommentStart(id) | FlowRunKind::CommentRef(id) => {
                                    if !comment_marks.contains(&id) {
                                        comment_marks.push(id);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            let mut line = LineBox { height, baseline, text_dy: if picture > text_line { 0.0 } else { self.text_dy(p, natural, lead, in_table) }, grid_below: if picture > text_line { 0.0 } else { self.grid_room_below(p, natural, lead, in_table, height, baseline, text_line, text_ascent) }, items: Vec::new(), page_break: slice.iter().any(|u| matches!(u, Unit::PageBreak)), column_break: slice.iter().any(|u| matches!(u, Unit::ColumnBreak)), has_revision, comment_marks };
            let mut x = start_x + shift;
            let line_x = x;
            let mut stops: Vec<(usize, f32)> = Vec::new();
            let mut list_tab: Option<(f32, f32)> = None;
            let mut run: Option<OpenRun> = None;
            let mut dec = LineDecorations::default();
            let is_all_markup = self.markup_mode() == MarkupDisplayMode::All;
            let flush = |run: &mut Option<OpenRun>, line: &mut LineBox, dec: &mut LineDecorations| {
                if let Some((x0, text, offsets, font, size, style, squeeze)) = run.take() {
                    let s = if style == LABEL { &p.list_style } else { &p.runs[style].style };
                    // Tracked changes are shown: insertions underlined and
                    // deletions struck through, in the changes' colour.
                    let revised;
                    let is_hidden_dotted = p.runs.get(style).is_some_and(|r| r.style.hidden && !self.res.print && self.res.markup_view.show_hidden_text && style != LABEL);
                    let active_rev = p.runs.get(style).and_then(|r| r.revision.as_ref()).filter(|r| self.res.markup_view.is_reviewer_shown(&r.author));
                    let s = match active_rev.map(|r| &r.kind).filter(|_| is_all_markup) {
                        Some(ibis_doc_model::RevisionKind::Insert) if style != LABEL => {
                            revised = ibis_doc_model::FlowRunStyle { underline: true, underline_style: ibis_doc_model::UnderlineStyle::Single, color: REVISION_COLOR, ..s.clone() };
                            &revised
                        }
                        Some(ibis_doc_model::RevisionKind::Delete) if style != LABEL => {
                            revised = ibis_doc_model::FlowRunStyle { strike: true, color: REVISION_COLOR, ..s.clone() };
                            &revised
                        }
                        Some(ibis_doc_model::RevisionKind::MoveTo) if style != LABEL => {
                            revised = ibis_doc_model::FlowRunStyle { underline: true, underline_style: ibis_doc_model::UnderlineStyle::Single, color: MOVE_REVISION_COLOR, ..s.clone() };
                            &revised
                        }
                        Some(ibis_doc_model::RevisionKind::MoveFrom) if style != LABEL => {
                            revised = ibis_doc_model::FlowRunStyle { strike: true, color: MOVE_REVISION_COLOR, ..s.clone() };
                            &revised
                        }
                        _ if is_hidden_dotted => {
                            revised = ibis_doc_model::FlowRunStyle { underline: true, underline_style: ibis_doc_model::UnderlineStyle::Dotted, ..s.clone() };
                            &revised
                        }
                        _ => s,
                    };
                    let s_size = { let full = s.enclosure_sizes.map_or(s.size, |(ring, _)| ring) as f32 / 100.0; if s.position == VerticalPosition::Baseline { full } else { self.script_size(text.chars().next().and_then(|c| s.font_for(c)).or_else(|| s.font_for('あ')), full, s.position) } };
                    let raise = match s.position {
                        VerticalPosition::Superscript => -self.script_shift(text.chars().next().and_then(|c| s.font_for(c)).or_else(|| s.font_for('あ')), s.size as f32 / 100.0, s_size, s.position),
                        VerticalPosition::Subscript => self.script_shift(text.chars().next().and_then(|c| s.font_for(c)).or_else(|| s.font_for('あ')), s.size as f32 / 100.0, s_size, s.position),
                        VerticalPosition::Baseline => 0.0,
                    } - s.raise as f32 / 2.0;
                    let font_name = text.chars().next().and_then(|c| s.font_for(c)).or_else(|| s.font_for('あ'));
                    let (run_ascent, run_line) = self.line_metrics(font_name, &font, s_size);
                    let run_descent = run_line - run_ascent;
                    let align_shift = match p.text_alignment {
                        ibis_doc_model::FontAlign::Top => run_ascent - text_ascent,
                        ibis_doc_model::FontAlign::Center => ((run_ascent - run_descent) - (text_ascent - (text_line - text_ascent))) / 2.0,
                        ibis_doc_model::FontAlign::Bottom => (text_line - text_ascent) - run_descent,
                        ibis_doc_model::FontAlign::Baseline | ibis_doc_model::FontAlign::Auto => 0.0,
                    };
                    let y_base = line.baseline + line.text_dy + raise + align_shift;
                    let end = offsets.last().copied().unwrap_or(0.0);
                    let show_comments = self.markup_mode() != MarkupDisplayMode::None && self.markup_mode() != MarkupDisplayMode::Original;
                    if show_comments && style != LABEL && end > 0.0 {
                        if let Some(c_ids) = self.comment_runs.get(&(p.id, style)) && !c_ids.is_empty() {
                            let color = hsla(COMMENT_HIGHLIGHT_COLOR);
                            if let Some(Item::Rect { x: last_x, w: last_w, color: last_c, .. }) = dec.highlights.last_mut()
                                && *last_c == color
                                && (*last_x + *last_w - x0).abs() < 0.02
                            {
                                *last_w = (x0 + end) - *last_x;
                            } else {
                                dec.highlights.push(Item::Rect { x: x0, y: 0.0, w: end, h: line.height, color });
                            }
                            let comment_ucolor = hsla(ibis_doc_model::Color::opaque(COMMENT_BUBBLE_ICON));
                            if let Some(last_u) = dec.underlines.last_mut()
                                && last_u.style == UnderlineStyle::Dash
                                && last_u.color == comment_ucolor
                                && (last_u.baseline - y_base).abs() < 0.01
                                && (last_u.size - s_size).abs() < 0.01
                                && (last_u.x + last_u.w - x0).abs() < 0.02
                            {
                                last_u.w = (x0 + end) - last_u.x;
                            } else {
                                dec.underlines.push(UnderlineBox {
                                    style: UnderlineStyle::Dash,
                                    size: s_size,
                                    x: x0,
                                    w: end,
                                    baseline: y_base,
                                    color: comment_ucolor,
                                band: None,
                                });
                            }
                        }
                    }
                    if let Some(h) = s.highlight && end > 0.0 {
                        let color = hsla(ibis_doc_model::Color::opaque(h));
                        if let Some(Item::Rect { x: last_x, w: last_w, color: last_c, .. }) = dec.highlights.last_mut()
                            && *last_c == color
                            && (*last_x + *last_w - x0).abs() < 0.02
                        {
                            *last_w = (x0 + end) - *last_x;
                        } else {
                            dec.highlights.push(Item::Rect { x: x0, y: 0.0, w: end, h: line.height, color });
                        }
                        // A stripe or a grid over the fill.
                        if let Some((pattern, pattern_color)) = s.shd_pattern {
                            dec.highlights.push(Item::Shading { pattern, color: hsla(ibis_doc_model::Color::opaque(pattern_color)), x: x0, y: 0.0, w: end, h: line.height });
                        }
                    }
                    if let Some(ref b) = s.border && end > 0.0 && !text.chars().all(char::is_whitespace) {
                        let run_start = x0;
                        let run_end = x0 + end;
                        let (y_top, y_bottom) = self.border_box_y(s, font_name, y_base, raise, run_ascent, run_line, s_size, b);
                        if let Some(last) = dec.borders.last_mut()
                            && last.border == *b
                            && (run_start - last.x_end).abs() < 2.0
                        {
                            last.x_end = run_end;
                            last.y_top = last.y_top.min(y_top);
                            last.y_bottom = last.y_bottom.max(y_bottom);
                        } else {
                            dec.borders.push(BorderBox {
                                border: *b,
                                x_start: run_start,
                                x_end: run_end,
                                y_top,
                                y_bottom,
                            });
                        }
                    }
                    let color = hsla(ibis_doc_model::Color::opaque(s.color));
                    if s.underline && end > 0.0 {
                        let ucolor = s.underline_color.map_or(color, |c| hsla(ibis_doc_model::Color::opaque(c)));
                        // The spans underlined: all of it, or the words.
                        let mut spans = Vec::new();
                        if s.underline_style == UnderlineStyle::Words {
                            let mut start: Option<f32> = None;
                            for (i, c) in text.chars().enumerate() {
                                let (a, z) = (offsets[i], offsets[i + 1]);
                                match (c.is_whitespace(), start) {
                                    (false, None) => start = Some(a),
                                    (true, Some(st)) => {
                                        spans.push((st, a));
                                        start = None;
                                    }
                                    _ => {}
                                }
                                if i + 2 == offsets.len() && let Some(st) = start {
                                    spans.push((st, z));
                                }
                            }
                        } else {
                            spans.push((0.0, end));
                        }
                        let u_size = s_size;
                let u_band = self.underline_band_of(font_name, s.bold, u_size);
                        // The underline of the spaces that end a line: Word draws it, up to the right edge, when the document underlines trailing spaces (`ulTrailSpace`),
                        // and not at all otherwise (Word's PDFs of `trailing-spaces-hang-ms*.docx`, `underlined-trailing-spaces-signature*.docx`). Those that hang past the
                        // edge are cut at it.
                        let trailing_spaces = text.chars().all(|c| c == ' ') && x0 + 0.01 >= line_x + used;
                        let right_edge = start_x + room;
                        for (a, z) in spans {
                            let ux = x0 + a;
                            let mut uw = z - a;
                            if trailing_spaces {
                                if !self.doc.ul_trail_space {
                                    continue;
                                }
                                uw = uw.min((right_edge - ux).max(0.0));
                            }
                            if let Some(last_u) = dec.underlines.last_mut()
                                && last_u.style == s.underline_style
                                && last_u.color == ucolor
                                && (last_u.baseline - y_base).abs() < 0.01
                                && u_band.is_some() == last_u.band.is_some()
                                        && (u_band.is_some() || (last_u.size - u_size).abs() < 0.01)
                                && (last_u.x + last_u.w - ux).abs() < 0.02
                            {
                                last_u.extend(ux, uw, u_band);
                            } else {
                                dec.underlines.push(UnderlineBox {
                                    style: s.underline_style,
                                    size: u_size,
                                    x: ux,
                                    w: uw,
                                    baseline: y_base,
                                    color: ucolor,
                                    band: u_band,
                                });
                            }
                        }
                    }
                    if (s.strike || s.double_strike) && end > 0.0 {
                        let strike_size = s_size;
                        let lines: Vec<(f32, f32)> = self.strike_lines_of(font_name, s.bold, strike_size, s.double_strike, y_base);
                        for (sy, sh) in lines {
                            if let Some(last_s) = dec.strikes.iter_mut().rev().find(|l| (l.y - sy).abs() < 0.01)
                                && last_s.color == color
                                && (last_s.h - sh).abs() < 0.01
                                && (last_s.x + last_s.w - x0).abs() < 0.02
                            {
                                last_s.w = (x0 + end) - last_s.x;
                            } else {
                                dec.strikes.push(StrikeBox { x: x0, y: sy, w: end, h: sh, color });
                            }
                        }
                    }
                    line.items.extend(self.emphasis_marks(s, style == LABEL || is_ruby_base(p, style), x0, y_base, s_size, &text, &offsets, color));
                    if let Some(run) = p.runs.get(style).filter(|_| style != LABEL) {
                        line.items.extend(self.enclosed_items(s, run, x0, y_base, end, s_size, color));
                        line.items.extend(self.overstrike_items(s, run, x0, y_base, end, s_size, color));
                    }
                    if let Some(run) = p.runs.get(style).filter(|_| style != LABEL) {
                        line.items.extend(self.ruby_items(s, &run.kind, x0, y_base, end, color));
                    }
                    let combined = p.runs.get(style).filter(|_| style != LABEL).and_then(|r| r.style.combine);
                    if let Some(c) = combined {
                        line.items.extend(self.combine_items(s, c, &text, x0, y_base, s_size, color, self.combine_brackets_of(p, style, &text)));
                    }
                    let is_tate = p.runs.get(style).filter(|_| style != LABEL).is_some_and(|r| r.style.tate_chu_yoko);
                    if is_tate && let Some(run) = p.runs.get(style) {
                        line.items.extend(self.tate_chu_yoko_items(&run.style, &text, x0, y_base, s_size, &font, color));
                    }
                    let mut offsets = offsets;
                    offsets.pop();
                    if combined.is_none() && !is_tate {
                        let before = line.items.len();
                        push_styled_text_items(&mut line.items, s, x0, y_base, text, offsets, font, size, color);
                        squeeze_items(&mut line.items, before, squeeze);
                    }
                }
            };
            for (k, u) in slice.iter().enumerate() {
                if let Some(&(position, true)) = positions.get(s + k) {
                    stops.push((position, x - line_x));
                }
                match u {
                    Unit::Char { c, width, inset, font, size, style, squeeze, .. } => {
                // A 縦中横 run is drawn whole, centred as one (its digits may be squeezed one by one to their cell, which would split it).
                        let tate_run = *style != LABEL && p.runs.get(*style).is_some_and(|r| r.style.tate_chu_yoko);
                        let own_cell = *width > 0.0 && crate::render::rich_text::is_font_spaced_mark(*c);
                        let same = !own_cell && run.as_ref().is_some_and(|r| r.5 == *style && (tate_run || (r.3 == *font && r.4 == *size && r.6 == *squeeze)));
                        if !same {
                            flush(&mut run, &mut line, &mut dec);
                            run = Some((x, String::new(), vec![0.0], font.clone(), *size, *style, *squeeze));
                        }
                        let r = run.as_mut().expect("a run");
                        // A glyph wider than its fixed-width cell is centred
                        // in it (the substitute's Latin letters).
                        // An optional hyphen inside a line is not drawn.
                        r.1.push(if *c == '\u{ad}' { '\u{200b}' } else { *c });
                        let at = x - r.0 + inset;
                        r.2.pop();
                        r.2.push(at);
                        x += width + if *c != '\u{200b}' && *style != LABEL { extras.get(k).copied().unwrap_or(0.0) } else { 0.0 };
                        r.2.push(x - r.0);
                    }
                    Unit::ListTab if left > x + 0.01 => {
                        flush(&mut run, &mut line, &mut dec);
                        list_tab = Some((x - line_x, left - line_x));
                        x = left;
                    }
                    Unit::Tab | Unit::ListTab => {
                        flush(&mut run, &mut line, &mut dec);
                        let stop_list: Vec<_> = p.tabs.iter().filter(|t| pt(t.position) > x + 0.01).collect();
                        let from = x;
                        match stop_list.first() {
                            Some(t) if matches!(t.align, TabAlign::Right | TabAlign::Center) => {
                                // The text after the tab ends at (or centres on) the stop.
                                let rest: f32 = slice[k + 1..].iter().take_while(|u| !matches!(u, Unit::Tab)).map(unit_width).sum();
                                let stop = pt(t.position);
                                x = if t.align == TabAlign::Right { stop - rest } else { stop - rest / 2.0 }.max(x);
                            }
                            Some(t) => x = pt(t.position),
                            None => x = ((x / tab_default).floor() + 1.0) * tab_default,
                        }
                        if matches!(u, Unit::ListTab) {
                            list_tab = Some((from - line_x, x - line_x));
                        }
                        // A leader: dots from the text before to the text after.
                        if stop_list.first().is_some_and(|t| t.leader.is_some()) {
                            let size = p.runs.first().map_or(10.5, |r| r.style.size as f32 / 100.0);
                            let (step, dot) = (size * 0.3, (size * 0.08).max(0.5));
                            let color = hsla(ibis_doc_model::Color::opaque(p.runs.first().map_or(ibis_doc_model::Rgb::BLACK, |r| r.style.color)));
                            let mut d = (from / step).ceil() * step + step / 2.0;
                            while d + dot < x - step / 2.0 {
                                line.items.push(Item::Rect { x: d, y: line.baseline - dot, w: dot, h: dot, color });
                                d += step;
                            }
                        }
                    }
                    Unit::PositionalTab { align, relative_to_margin, leader } => {
                        flush(&mut run, &mut line, &mut dec);
                        let from = x;
                        let rest: f32 = slice[k + 1..].iter().take_while(|u| !matches!(u, Unit::Tab | Unit::PositionalTab { .. })).map(unit_width).sum();
                        let (span_l, span_r) = SegmentEdges::new(width, left, right, start_x, room, start_x, width - right).span(*align, *relative_to_margin);
                        let target = match align {
                            TabAlign::Right => span_r - rest,
                            TabAlign::Center => (span_l + span_r - rest) / 2.0,
                            _ => span_l,
                        };
                        x = target.max(x);
                        if *leader {
                            let size = p.runs.first().map_or(10.5, |r| r.style.size as f32 / 100.0);
                            let (step, dot) = (size * 0.3, (size * 0.08).max(0.5));
                            let color = hsla(ibis_doc_model::Color::opaque(p.runs.first().map_or(ibis_doc_model::Rgb::BLACK, |r| r.style.color)));
                            let mut d = (from / step).ceil() * step + step / 2.0;
                            while d + dot < x - step / 2.0 {
                                line.items.push(Item::Rect { x: d, y: line.baseline - dot, w: dot, h: dot, color });
                                d += step;
                            }
                        }
                    }
                    Unit::Picture { media, w, h, ee_bottom, .. } => {
                        flush(&mut run, &mut line, &mut dec);
                        line.items.push(Item::Picture { media: *media, x, y: line.baseline - h - ee_bottom, w: *w, h: *h });
                        x += w;
                    }
                    Unit::Math { boxed, w, font, color, .. } => {
                        flush(&mut run, &mut line, &mut dec);
                        let items = self.math_items(boxed, x, line.baseline, font, *color);
                        line.items.extend(items);
                        x += w;
                    }
                    Unit::Shape { shape, w, h } => {
                        flush(&mut run, &mut line, &mut dec);
                        self.emit_unit_shape(shape, w, h, &mut x, width, &mut line, page_number);
                    }
                    Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak => {}
                }
            }
            flush(&mut run, &mut line, &mut dec);
            dec.finish(&mut line, self.vertical.get());
            // Where the line's text ends: the next line's first position.
            let end = positions.get(e).map_or(p.len(), |&(position, _)| position);
            // The caret after a page or column break stands where its dashes end (Word: the break's mark is the line's last, with the paragraph's end after it).
            let avail = (width - right - line_x).max(0.0);
            stops.push((end, if matches!(slice.last(), Some(Unit::PageBreak | Unit::ColumnBreak)) { avail.max(x - line_x) } else { x - line_x }));
            line.items.push(Item::Line { para: p.id, x: line_x, y: 0.0, h: line.height, stops: stops.into(), vertical: false, list_tab, base: line.baseline + line.text_dy, avail, pad: (0.0, 0.0), largest: slice.iter().filter_map(|u| if let Unit::Char { c, metric, .. } = u { (!c.is_whitespace()).then_some(*metric) } else { None }).fold(0.0, f32::max) });
            out.push(line);
        }
        out
    }
}

/// A block's pieces as last laid out, and what they were laid out from.
struct CachedPieces {
    block: Block,
    labels: Vec<(u32, String)>,
    page: u32,
    counts: Option<(u32, u32, u32)>,
    pieces: Vec<Piece>,
}

thread_local! {
    /// Laid-out blocks, by (id, width, styles, in_table, line_pitch, print, print_hidden, print_markup, markup_view, the settings that shape the lines): see `Layouter::pieces`.
    static PIECES: std::cell::RefCell<std::collections::HashMap<(u32, u32, usize, bool, Option<Emu>, bool, bool, bool, MarkupView, LineSettings), CachedPieces>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// What, besides the block itself, decides how its lines are set: the document's settings (kinsoku and squeezing of stops, the compatibility mode, the
/// grid, whether field codes show) and the section's character grid. The one list that a laid-out block is kept by: a setting that shapes the lines is
/// added here, not in the places that read it.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct LineSettings {
    compat_mode: Option<u32>,
    show_field_codes: bool,
    compress_punctuation: bool,
    balance_single_double_byte_width: bool,
    adjust_line_height_in_table: bool,
    do_not_use_html_paragraph_auto_spacing: bool,
    split_pg_break_and_para_mark: bool,
    default_tab_stop: Option<Emu>,
    char_grid: bool,
    char_space: Option<Emu>,
}

impl Layouter<'_> {
    fn line_settings(&self) -> LineSettings {
        LineSettings {
            compat_mode: self.doc.compat_mode,
            show_field_codes: self.doc.show_field_codes,
            compress_punctuation: self.doc.compress_punctuation,
            balance_single_double_byte_width: self.doc.balance_single_double_byte_width,
            adjust_line_height_in_table: self.doc.adjust_line_height_in_table,
            do_not_use_html_paragraph_auto_spacing: self.doc.do_not_use_html_paragraph_auto_spacing,
            split_pg_break_and_para_mark: self.doc.split_pg_break_and_para_mark,
            default_tab_stop: self.doc.default_tab_stop,
            char_grid: self.char_grid.get(),
            char_space: self.char_space.get(),
        }
    }
}

/// Whether field `instruction` is one whose text depends on the page
/// (`PAGE` with a switch, `NUMPAGES`, `SECTIONPAGES`, `SECTION`, a formula
/// over them).
fn page_field(instruction: &str) -> bool {
    let t = instruction.trim_start();
    t.starts_with('=') || matches!(t.split_whitespace().next(), Some("PAGE" | "NUMPAGES" | "SECTIONPAGES" | "SECTION"))
}

/// Whether a block has fields whose text depends on the page or the file.
fn has_fields(block: &Block) -> bool {
    match block {
        Block::Paragraph(p) => p.runs.iter().any(|r| match &r.kind {
            FlowRunKind::PageNumber | FlowRunKind::FileName(_) | FlowRunKind::NoteRef { .. } | FlowRunKind::NoteMark => true,
            FlowRunKind::Field { instruction, .. } => page_field(instruction),
            _ => false,
        }),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).any(has_fields),
    }
}

/// Characters of one run being gathered on a line: where it starts, its
/// text, each character's offset (and the end), font, size and run index.
type OpenRun = (f32, String, Vec<f32>, Font, f32, usize, f32);

/// How far a justified line (mode 15) may run over the edge and still take the word it is at (`units[i]` is in the word, which ends at `word_end`; the line
/// starts at `start`): the spaces narrow by a quarter at most (of their own width), and the overflow is at most 0.35 of the word and the space before it,
/// whichever is less. The two constants are Word's, read from its PDFs, not fitted to documents: `justify-shrink-mono2-compat15-probe.docx` (Courier New, 7 pt,
/// 17 counts of words x words of 3, 6 and 15 letters, the overflow in 0.4 % steps: per space 22 to 25 % of the space for every count of words until the word
/// cap is reached), `justify-shrink-lastword-compat15-probe.docx` (17 words, the last of 2 to 20 letters, steps of a tenth of a letter: the largest overflow
/// taken is 0.35 x (the letters + 1) letters, whatever the words before it), `justify-shrink-mono-compat15-probe.docx` (10.5 and 12 pt: the same ratios) and
/// `justify-shrink-prop-compat15-probe.docx` (Times New Roman and Century 10.5 pt, 8 and 17 words, the last word 2 to 12 letters with and without a bracket
/// first: a quarter of the spaces alone sorts 8-word lines with no error, and with 0.34 to 0.36 of the last word the 17-word lines to 5 to 8 of 176).
/// Why a quarter and 0.35, and how the two are one rule in Word, is not known.
fn justify_overflow_room(units: &[Unit], start: usize, i: usize, word_end: usize) -> f32 {
    let mut word_start = i;
    while word_start > start && unit_char(&units[word_start - 1]).is_some_and(|ch| ch != ' ') {
        word_start -= 1;
    }
    let last_word: f32 = units[word_start..word_end].iter().map(unit_width).sum::<f32>() + if word_start > start { unit_width(&units[word_start - 1]) } else { 0.0 };
    let spaces: f32 = units[start..i].iter().filter(|u| unit_char(u) == Some(' ')).map(unit_width).sum();
    (spaces * 0.25).min(last_word * 0.35)
}

/// Whether a stop or bracket (full width: a half em of glyph and a half em of room beside it) can still give up that half em to
/// let a line take one more character (約物の詰め, `compressPunctuation`): it must be one of those characters, not squeezed yet (a
/// pair of them kerned, or an earlier pass of the line, has taken it), and wider than the half em it gives (a narrower one, a
/// character grid's tight pitch, has no room to give). `em` is the glyph's size with its fitText squeeze.
fn can_give_up_half(c: char, width: f32, em: f32, squeezed: bool) -> bool {
    "、。，．・（）「」『』【】〔〕〈〉《》［］｛｝".contains(c) && !squeezed && width > em / 2.0 // i18n-ok（約物の文字の集合。文字の並び）
}

/// The same for ： and ； in compatibility mode 15, where Word takes a quarter em from each side of them as of a dot (GitHub issue #164, paragraphs 5 to 7 of
/// `justify-squeeze-or-break.docx`). In compatibility mode 14 and below the line squeezes them in the same way as the dots (Word's PDF of `justify-squeeze-or-break-compat14.docx`, paragraphs 5 and 7: 46 characters; paragraph 6's ： is past the break: 44).
fn colon_gives_up_half(c: char, width: f32, em: f32, squeezed: bool) -> bool {
    "：；".contains(c) && !squeezed && width > em / 2.0 // i18n-ok（コロン・セミコロンの文字。文字の並び）
}

#[cfg(test)]
mod can_give_up_half_tests {
    use super::{can_give_up_half, colon_gives_up_half};

    #[test]
    fn a_fresh_stop_or_bracket_gives_up_its_half_em() {
        for c in "、。，．・（）「」『』【】〔〕〈〉《》［］｛｝".chars() {
            assert!(can_give_up_half(c, 10.5, 10.5, false), "{c}");
        }
    }

    #[test]
    fn the_colon_and_the_semicolon_give_up_their_half_em_as_the_dot_does() {
        // Word's PDF of GitHub issue #164's `justify-squeeze-or-break.docx` (paragraphs 5 to 7): ： ； take a quarter em from each side.
        // (only in compatibility mode 15: `colon_gives_up_half`; below it Word's PDF of `justify-squeeze-or-break-compat14.docx` keeps them whole)
        for c in "：；".chars() {
            assert!(!can_give_up_half(c, 10.5, 10.5, false), "{c}");
            assert!(colon_gives_up_half(c, 10.5, 10.5, false), "{c}");
        }
    }

    #[test]
    fn other_characters_do_not() {
        for c in "あ漢A1 ,.()ー…".chars() {
            assert!(!can_give_up_half(c, 10.5, 10.5, false), "{c}");
        }
    }

    #[test]
    fn one_squeezed_gives_nothing_more() {
        // Marked (a kerned pair, an earlier pass), or already down to the half em it keeps: not squeezed twice.
        assert!(!can_give_up_half('、', 10.5, 10.5, true));
        assert!(!can_give_up_half('」', 5.25, 10.5, true));
        assert!(!can_give_up_half('」', 5.25, 10.5, false));
        assert!(!can_give_up_half('、', 5.0, 10.5, false));
    }

    #[test]
    fn with_a_character_grid_the_width_is_the_pitch() {
        // The pitch makes a stop a little narrower or wider than the glyph's em: it still has its half em to give...
        assert!(can_give_up_half('、', 10.5 - 0.6, 10.5, false));
        assert!(can_give_up_half('、', 10.5 + 1.0, 10.5, false));
        assert!(can_give_up_half('。', 7.4, 10.5, false), "0.70 em: the old threshold's edge");
        // ...unless the grid squeezed it below that (a pitch under 0.5 em).
        assert!(!can_give_up_half('。', 5.0, 10.5, false));
    }

    #[test]
    fn a_fitted_glyph_halves_its_squeezed_em() {
        // fitText (`squeeze` 0.5): the glyph is 5.25 pt, its half em 2.625.
        assert!(can_give_up_half('、', 5.25, 5.25, false));
        assert!(can_give_up_half('、', 3.0, 5.25, false));
        assert!(!can_give_up_half('、', 2.6, 5.25, false));
    }
}

/// Where the part of a piece that has to fit the page ends, from `y` where it starts: its height without the space after it and
/// the room the line grid adds under its text (Word keeps a line on the page whose text fits it: Century 10.5 pt in a 17.15 pt grid
/// stays with 2 pt of its line past the foot, goes on with 3; ＭＳ 明朝 stays with 1 pt, goes with 2), unless a paragraph kept with
/// the next (`kept` is the height of the next block's first line) is to go with it.
fn page_foot(y: f32, height: f32, space_after: f32, grid_below: f32, kept: f32) -> f32 {
    y + height - if kept > 0.0 { 0.0 } else { space_after + grid_below } + kept
}

#[cfg(test)]
mod page_foot_tests {
    use super::page_foot;

    #[test]
    fn a_line_off_the_grid_needs_its_whole_height() {
        assert_eq!(page_foot(100.0, 12.0, 0.0, 0.0, 0.0), 112.0);
    }

    #[test]
    fn the_space_after_does_not_need_room() {
        assert_eq!(page_foot(100.0, 12.0 + 6.0, 6.0, 0.0, 0.0), 112.0);
    }

    #[test]
    fn the_grids_room_under_the_text_does_not_need_room() {
        // A 17.15 pt grid line of 12.15 pt of text: 2.5 pt above and 2.5 below (the grid adds 5 pt, half under).
        assert!((page_foot(100.0, 17.15, 0.0, 2.5, 0.0) - 114.65).abs() < 1e-4);
        // Century 10.5 pt (a 12.1 pt line): 2.525 pt under; 2 pt past the foot fits, 3 pt does not.
        let foot = page_foot(0.0, 17.15, 0.0, (17.15 - 12.1) / 2.0, 0.0);
        assert!(foot <= 17.15 - 2.0 && foot > 17.15 - 3.0, "{foot}");
        // ＭＳ 明朝 10.5 pt (a 13.58 pt line, A2's 1.2935 em): 1.785 pt under; 1 pt fits, 2 pt does not.
        let foot = page_foot(0.0, 17.15, 0.0, (17.15 - 13.58) / 2.0, 0.0);
        assert!(foot <= 17.15 - 1.0 && foot > 17.15 - 2.0, "{foot}");
    }

    #[test]
    fn a_paragraph_kept_with_the_next_needs_all_of_it() {
        assert_eq!(page_foot(100.0, 20.0, 6.0, 2.5, 15.0), 135.0);
    }

    #[test]
    fn a_line_without_a_grid_or_space_is_its_height() {
        for h in [8.0f32, 10.5, 17.15, 40.0] {
            assert_eq!(page_foot(0.0, h, 0.0, 0.0, 0.0), h);
        }
    }
}

/// Whether justifying widens the gap after unit `k` of a line: Word spreads a justified line over its
/// spaces and over the East Asian characters (between each of them and the next), never between the
/// letters of a Latin word (`Distribute` spreads everything; see `spread_extras`).
fn expands_after(slice: &[Unit], k: usize) -> bool {
    match (slice.get(k), slice.get(k + 1)) {
        (Some(Unit::Char { c, .. }), Some(Unit::Char { c: next, .. })) => {
            !crate::render::rich_text::joins_previous(Some(*c), *next) && (c.is_whitespace() || is_cjk(*c) || is_cjk(*next) || *next == '\u{200b}')
        }
        _ => true,
    }
}

/// The automatic space (和欧間) after unit `k` of a line, when there is one between it and the next.
fn auto_space_after(slice: &[Unit], k: usize) -> f32 {
    match (slice.get(k), slice.get(k + 1)) {
        (Some(Unit::Char { c, auto_space, .. }), Some(Unit::Char { c: next, .. })) if *auto_space > 0.0 && !crate::render::rich_text::joins_previous(Some(*c), *next) => *auto_space,
        _ => 0.0,
    }
}

/// `spread_extras` for a line that starts with a list number: the number and the tab after it are not justified, the tab ends at the text's start (`left`, the paragraph's
/// left indent: ECMA-376 17.9.7 `suff` tab, the text starts at the next tab stop, which is the hanging indent's end) so the text spreads over the room from there: `x0` is where
/// the line starts, `room` its width from `x0`, `used` the width of the units before the tab and after it. Without a tab to `left`, as `spread_extras`.
#[allow(clippy::too_many_arguments)]
fn spread_extras_past_number(slice: &[Unit], x0: f32, left: f32, room: f32, used: f32, distribute: bool, marks: usize) -> Vec<f32> {
    let Some(t) = slice.iter().position(|u| matches!(u, Unit::ListTab)) else {
        return spread_extras(slice, room - used, distribute, marks);
    };
    let before: f32 = slice[..t].iter().map(unit_width).sum();
    if left <= x0 + before + 0.01 || !slice[..t].iter().all(|u| matches!(u, Unit::Char { style: LABEL, .. })) {
        return spread_extras(slice, room - used, distribute, marks);
    }
    let rest = &slice[t + 1..];
    let rest_used: f32 = rest.iter().map(unit_width).sum();
    let mut extras = vec![0.0; t + 1];
    extras.extend(spread_extras(rest, room - (left - x0) - rest_used, distribute, marks));
    extras
}

/// The room spreading a line of units `slice` adds after each of them (the last takes none), `slack` the room the line
/// has left over (`marks` of its characters are comments' marks, which take none).
///
/// Distributed, every gap takes an equal share. Justified, the automatic spaces between Japanese and Latin text (和欧間,
/// a quarter em) widen first, up to half an em (JIS X 4051's 和欧間 is a quarter em, at most half an em, and is widened
/// before the gaps between the characters), and what is still left goes to every gap that widens (`expands_after`),
/// those spaces too: Word's PDF of GitHub issue #67's `justify-repro.docx` (10.5 pt Yu Gothic, 32.4 pt left) widens
/// the two spaces beside 「LIVE」 by a quarter em and then every gap of the line by 1.39 pt, and that of
/// `justify-autospace.docx` (12 pt, 3 pt left) gives all of it to the two spaces before 「10」, the kana as they are.
/// A line narrowed to take its last word (`slack` below zero) is narrowed in its spaces and its automatic spaces when
/// it has these, else in every gap that widens.
fn spread_extras(slice: &[Unit], slack: f32, distribute: bool, marks: usize) -> Vec<f32> {
    let n = slice.len();
    let mut extras = vec![0.0; n];
    let last = n.saturating_sub(1);
    if distribute {
        let share = slack / last.saturating_sub(marks).max(1) as f32;
        extras[..last].fill(share);
        return extras;
    }
    let autos: Vec<(usize, f32)> = (0..last).map(|k| (k, auto_space_after(slice, k))).filter(|a| a.1 > 0.0).collect();
    let widen: Vec<usize> = if slack < 0.0 && !autos.is_empty() {
        (0..last).filter(|&k| auto_space_after(slice, k) > 0.0 || matches!(&slice[k], Unit::Char { c, .. } if c.is_whitespace())).collect()
    } else {
        (0..last).filter(|&k| expands_after(slice, k)).collect()
    };
    let mut rest = slack;
    if slack > 0.0 {
        let room: f32 = autos.iter().map(|a| a.1).sum();
        let first = slack.min(room);
        for &(k, space) in &autos {
            extras[k] += first * space / room;
        }
        rest -= first;
    }
    if rest != 0.0 {
        let share = rest / widen.len().max(1) as f32;
        for k in widen {
            extras[k] += share;
        }
    }
    extras
}


/// Whether the last unit of a paragraph, an ASCII comma or full stop or a curly quote, keeps its width instead of hanging. A narrow Latin glyph (narrower than the em) keeps it. A full-width one (the
/// East Asian typeface's) hangs, except in a left aligned (or centred, right) paragraph of a document with `compressPunctuation`: Word's PDFs of `hang-line-end-closers-compat14-probe.docx` (no
/// compression: 20 kana and a ’ or ” fill 21 characters of a 20 em line, left aligned or justified), and Word's least widths of `時間休憩”` in ＭＳ 明朝 10.5 pt with `compressPunctuation`
/// (mode 14 and 15, no grid and a grid of lines and characters, `2026-11-18-54-grid-result.tsv`): 840 twips (four ems, the quote hung) justified and 1050 (all five) left aligned; the stops 。、 hang
/// in both.
fn keeps_width_at_end(units: &[Unit], i: usize, ch: char, align: FlowAlign, compress_punctuation: bool) -> bool {
    if i + 1 != units.len() || !",.’”".contains(ch) {
        return false;
    }
    match &units[i] {
        Unit::Char { size, width, .. } => *width < *size * 0.9 || (compress_punctuation && !matches!(align, FlowAlign::Justify | FlowAlign::Distribute)),
        _ => true,
    }
}

fn unit_width(u: &Unit) -> f32 {
    match u {
        Unit::Char { width, .. } => *width,
        Unit::Picture { w, .. } | Unit::Shape { w, .. } | Unit::Math { w, .. } => *w,
        _ => 0.0,
    }
}

fn unit_char(u: &Unit) -> Option<char> {
    match u {
        Unit::Char { c, .. } => Some(*c),
        _ => None,
    }
}

impl Layouter<'_> {
    /// A character's line height and ascent with room for its ruby above it (`raise`; Word's PDF of DW14). The room of a 傍点 (`w:em`) is not here: it goes
    /// by the line's height and line spacing (`emphasis_adjust`).
    ///
    /// The ruby's top is `raise` above the base's baseline plus the ruby's own line ascent, and the line's ascent is the base's: both with the half of the room
    /// above the baseline that a line of ＭＳ 明朝 / ＭＳ ゴシック has beyond its 1 em (`ms_room`, as `baseline_in_line` puts the baseline), so that a ruby of 5 pt over
    /// a base of 10.5 pt makes the line `raise + (5 + room/2) - (10.5 + room/2)` higher (Word's PDFs of GitHub issue #179, `n2-line-height.docx`, ＭＳ 明朝 10.5 pt: lines 3.45,
    /// 5.95 higher for rubies of 5 and 7.5 pt raised 9.77 and 9.42 pt, 9.48 for a ruby of 10.5 pt over 14 pt text raised 13.5 pt).
    fn mark_room(&self, p: &FlowParagraph, c: char, style: usize, size: f32, line: f32, ascent: f32) -> (f32, f32) {
        if let Some(FlowRunKind::Ruby { size: rs, raise, align, .. }) = p.runs.get(style).map(|r| &r.kind)
            && size > 0.0
        {
            let vertical = self.vertical.get();
            // A ruby at the base's right (`rightVertical`, across a line) is no higher than its base: the line keeps the base's own (Word's XPS, many-patterns-3).
            if !vertical && *align == ibis_doc_model::RubyAlign::RightVertical {
                return (line, ascent);
            }
            let ruby_size = *rs as f32 / 100.0;
            let name = p.runs.get(style).and_then(|r| r.style.font_for(c));
            let (room, ruby_room) = if vertical { (0.0, 0.0) } else { (self.ms_room(name, size), self.ms_room(name, ruby_size)) };
            let base_ascent = ascent + room * 0.5;
            let raised = base_ascent.max(*raise as f32 / 100.0 + (ascent / size * ruby_size) + ruby_room * 0.5);
            return (line + raised - base_ascent, raised - room * 0.5);
        }
        (line, ascent)
    }
}

/// Whether run `style` of `p` is the base of a ruby. Word draws no 傍点 (`w:em`) over it, and leaves no room for one in its line (Word's PDF of GitHub issue #179,
/// `n2-line-height.docx` H2c: a ruby base with dots has 0 marks, its line 17.04 pt from the one above as that of a ruby without them).
fn is_ruby_base(p: &FlowParagraph, style: usize) -> bool {
    p.runs.get(style).is_some_and(|r| matches!(r.kind, FlowRunKind::Ruby { .. }))
}

/// Whether a line may break between `before` and `after`: `render::line_break::may_break` with Word's kinsoku (the lists the document
/// sets, 禁則文字の設定, replace the standard and strict ones) when `kinsoku` is on. A hyphen breaks a Latin word only with `word_wrap`.
pub(crate) fn may_break(before: Option<char>, after: Option<char>, kinsoku: bool, word_wrap: bool, doc: &FlowDocument) -> bool {
    use crate::render::line_break::{self, Kinsoku, LatinBreaks};
    let (Some(b), Some(a)) = (before, after) else { return true };
    if b == '-' && !word_wrap && !line_break::is_ideographic(a) {
        return false;
    }
    let no_start = doc.no_line_start.as_deref().unwrap_or(if doc.strict_first_and_last_chars { STRICT_NO_LINE_START } else { NORMAL_NO_LINE_START });
    let no_end = doc.no_line_end.as_deref().unwrap_or(NO_LINE_END);
    line_break::may_break(b, a, kinsoku.then_some(Kinsoku { no_start, no_end }), LatinBreaks::default())
}

/// Where on the page the body is.
struct Frame {
    left: f32,
    top: f32,
    width: f32,
    bottom: f32,
}

/// A block laid out: its lines (paragraph) or rows (table), each a unit
/// that goes on one page, `x`/`y` relative to the block.
#[derive(Clone, Default)]
struct Piece {
    height: f32,
    /// The space before (of a paragraph's first line) and after (of its
    /// last) in `height`: next to each other the larger one is kept
    /// (Word's PDF of DWID).
    space_before: f32,
    space_after: f32,
    /// The line grid's room under the text of its last line (`LineBox::grid_below`): it need not fit the page.
    grid_below: f32,
    items: Vec<Item>,
    page_break: bool,
    /// Continue in the next column (the next page without columns).
    column_break: bool,
    keep_with_next: bool,
    /// A table's heading row, repeated at the top of each page the table
    /// continues on.
    header: bool,
    /// A table row that may be divided where a page ends (its cells and
    /// their content, to draw each part).
    split: Option<Arc<RowSplit>>,
    /// A table row's horizontal span `(left, right)` from the table's grid columns, used even when the table has no borders or fill (`c018_b1`).
    table_bounds: Option<(f32, f32)>,
    /// Cells merged down from this row whose text is not set from the top (centred, at the foot, turned): placed once the pages are known (`AnchoredCell`).
    anchored: Vec<Arc<AnchoredCell>>,
}

/// The text of the cells merged down of a table that is on one page (in a cell, a header, a floating table), placed in the rows they cover.
fn place_anchored_on_one_page(pieces: &mut [Piece]) {
    for i in 0..pieces.len() {
        for cell in std::mem::take(&mut pieces[i].anchored) {
            let rows_h: f32 = pieces[i..].iter().take(cell.rows.len()).map(|row| row.height).sum();
            let items = cell.place(rows_h);
            pieces[i].items.extend(items);
        }
    }
}

/// A cell merged down over several rows with its text centred, at the foot or turned: where the text goes depends on how many of its rows are on the page it starts on
/// (Word's PDF of `vmerge-aligned-across-pages.docx` and corpus 128: the text is set within the part of the cell on its first page, not within the whole cell).
#[derive(Clone, Debug)]
struct AnchoredCell {
    /// What the cell holds, laid out from its top left (before any anchoring).
    items: Vec<Item>,
    content: f32,
    across: f32,
    direction: ibis_doc_model::CellDirection,
    anchor: CellAnchor,
    left: f32,
    right: f32,
    cell_top: f32,
    border: f32,
    /// Half the width of the left and the right border (a vertical cell's text is across the room between the borders' middles, see `turn_cell_text`'s caller).
    side_borders: [f32; 2],
    /// The cell's margins: top, right, bottom, left.
    margins: [f32; 4],
    spacing: f32,
    /// The heights of the rows it covers.
    rows: Vec<f32>,
}

impl AnchoredCell {
    /// The cell's text in a cell `rows_h` high (the rows of it on the page it starts on), from the top of its first row.
    fn place(&self, rows_h: f32) -> Vec<Item> {
        let cell_h = if self.spacing > 0.0 { (rows_h - self.spacing).max(0.0) } else { rows_h };
        let [m0, m1, m2, m3] = self.margins;
        let dx = self.left + m3 + self.side_borders[0];
        if self.direction.vertical() {
            let by = self.cell_top + self.border + m0;
            let bw = (self.right - self.left - m1 - m3 - self.side_borders[0] - self.side_borders[1]).max(0.0);
            let bh = (cell_h - m0 - m2 - self.border).max(0.0);
            turn_cell_text(&self.items, self.direction, self.anchor, (dx, by, bw, bh), self.across, &|_, _, size| size)
        } else {
            let room = cell_h - m0 - m2 - self.border;
            let dy = self.cell_top
                + self.border
                + m0
                + match self.anchor {
                    CellAnchor::Top => 0.0,
                    CellAnchor::Center => ((room - self.content) / 2.0).max(0.0),
                    CellAnchor::Bottom => (room - self.content).max(0.0),
                };
            if self.direction == ibis_doc_model::CellDirection::LrTbV {
                turn_japanese(self.items.iter().cloned().map(|it| offset(it, dx, dy)))
            } else {
                self.items.iter().cloned().map(|it| offset(it, dx, dy)).collect()
            }
        }
    }
}

/// A row that may go on to the next page, as its cells are drawn.
#[derive(Clone, Debug)]
struct RowSplit {
    cells: Vec<SplitCell>,
    /// What is left of the row's minimum height (`w:trHeight` atLeast).
    min_height: f32,
}

#[derive(Clone, Debug)]
struct SplitCell {
    left: f32,
    right: f32,
    fill: Option<Hsla>,
    /// The content, placed in the row (inside the borders and margins) from the top of the cell's text, not shifted by the cell's vertical
    /// alignment: Word divides a row by where the lines fall counted from the top, then aligns each part within its own height (GitHub issue
    /// #178, Word's PDF: a centred cell of one line stays on the first page with the first lines of its taller neighbour, centred in that part).
    content: Vec<Item>,
    anchor: CellAnchor,
    /// From the row's top to the content (top border and margin), and from
    /// the content to the row's bottom (margin).
    top: f32,
    bottom: f32,
    /// Top, right, bottom and left, as drawn (after the neighbours').
    borders: [Option<FlowBorder>; 4],
}

/// A vertical cell's text laid out across (`items`, lines `across` thick
/// in all) turned into the cell's box (`x`, `y`, `w`, `h`): down with the
/// lines right to left (`TbRlV`; Japanese upright, the rest turned
/// clockwise; `TbRl` turns all of it), down with the lines left to right (`TbLrV`), or up with
/// them left to right (`BtLr`, all turned anticlockwise); its vertical
/// alignment is across the lines (Word's PDF of DCTD: 上 is right for
/// `TbRl` and `TbRlV`, left for the others).
fn turn_cell_text(items: &[Item], direction: ibis_doc_model::CellDirection, anchor: CellAnchor, (bx, by, bw, bh): (f32, f32, f32, f32), across: f32, advance: &dyn Fn(char, &Font, f32) -> f32) -> Vec<Item> {
    turn_cell_text_in_lines(items, &|_, _| None, direction, anchor, (bx, by, bw, bh), across, advance)
}

/// `turn_cell_text` where `largest(u, v)` may give the largest size of the line the text at (`u` along, baseline `v`) is the main text of, which the upright characters of the
/// line stand by (the page's lines, `turn_page_items`); `None` keeps them by their own size.
#[allow(clippy::too_many_arguments)]
fn turn_cell_text_in_lines(items: &[Item], largest: &dyn Fn(f32, f32) -> Option<f32>, direction: ibis_doc_model::CellDirection, anchor: CellAnchor, (bx, by, bw, bh): (f32, f32, f32, f32), across: f32, advance: &dyn Fn(char, &Font, f32) -> f32) -> Vec<Item> {
    use ibis_doc_model::CellDirection;
    let room = (bw - across).max(0.0);
    let shift = match anchor {
        CellAnchor::Top => 0.0,
        // Lines that are thicker than the room are still centred, so they stand out on both sides (Word's PDF of the issue #271 form: two columns of 10 pt in a
        // room of 17 pt are centred on the room's middle, 50.17 pt; they were put against the edge, 2.0 pt off).
        CellAnchor::Center => (bw - across) / 2.0,
        CellAnchor::Bottom => room,
    };
    let mut out = Vec::new();
    let single_offset = [0.0f32];
    for it in items {
        let (x, y, text, offsets, font, size, color, xscale) = match it {
            Item::Text { x, y, text, offsets, font, size, color } => (x, y, text, offsets.as_ref(), font, size, color, &1.0f32),
            Item::Scaled { x, y, text, font, size, color, xscale } => (x, y, text, &single_offset[..], font, size, color, xscale),
            _ => continue,
        };
        for ((_, c), dx) in text.char_indices().zip(offsets.iter()) {
            // A space is a character of the column as any other (Word's PDFs of GitHub issue #188, D3a: U+0020, U+00A0, U+2007, U+202F and U+3000 are all
            // drawn into the PDF, where they are not seen but are in the text taken out of it); only a control and the zero width space are not.
            if c.is_control() || c == '\u{200b}' {
                continue;
            }
            let (u, v) = (x + dx, *y);
            let s: SharedString = c.to_string().into();
            match direction {
                CellDirection::TbRl | CellDirection::TbRlV | CellDirection::TbLrV => {
                    // Down: along the cell from its top; across from its
                    // right (lines right to left) or left.
                    let line_x = match direction {
                        CellDirection::TbRl | CellDirection::TbRlV => bx + bw - shift - v,
                        _ => bx + shift + (across - v),
                    };
                    // ˚ (U+02DA) stands too: it is the circle of an 圏点, which Word draws upright by its box's vertical origin like the dot's (the same
                    // character in a text is UAX #50's R, turned, and is rare).
                    // `tbRl` of a cell turns all of it (Word for Windows: the Japanese lies on its side too; `tbRlV` keeps it upright).
                    if direction != CellDirection::TbRl && (crate::render::vertical_text::upright(c) || c == '\u{2da}') {
                        // OpenType vert/vrt2 selects the font’s vertical glyph.
                        let font=if crate::render::vertical_text::vertical_glyph(c){crate::render::vertical_text::font(font)}else{font.clone()};
                        // Upright, centred on the line. The centre of the upright box stands 0.35 of its size from the baseline: in a line of larger characters (a note's 9 pt text
                        // after a mark of 10.5 pt) a smaller one would stand off the line's middle, and Word puts every upright character of the line's text on the middle of the
                        // largest one's box (Word's PDF of GitHub issue #188, `n5-vertical.docx` D6c: the note's 9 pt さ is 0.61 pt right of where its own size puts it).
                        let mid = line_x + largest(u, v).map_or(*size, |l| l.max(*size)) * 0.35;
                        // 、。 take their vertical forms: the upper right of the box.
                        // An upright character the font sets narrower than the em (the proportional ‘ ’ “ ” of Hiragino, 0.29 and 0.49 em) has its advance
                        // centred on the column: Word's Hiragino quotation marks stand in the middle of a vertical line, where the glyph set at the em
                        // box's left edge stood left of it. A character of the full em (the usual case) moves by nothing.
                        let (dx, dy) = (((*size - advance(c, &font, *size)) / 2.0).max(0.0), 0.0);
                        // A squeezed one keeps its squeeze across the line.
                        if (*xscale - 1.0).abs() > 0.001 {
                            out.push(Item::Scaled { x: mid - size / 2.0 + dx, y: by + u + size * 0.88 + dy, text: s, font: font.clone(), size: *size, color: *color, xscale: *xscale });
                        } else {
                            out.push(Item::Text { x: mid - size / 2.0 + dx, y: by + u + size * 0.88 + dy, text: s, offsets: Arc::from([0.0f32]), font: font.clone(), size: *size, color: *color });
                        }
                    } else {
                        // A turned letter's baseline is not a fixed 0.35 em left of the upright characters' centre: Word centres the face's own box (its Windows
                        // ascent and descent, Yu's typographic ones) on the column's centre, the centre of the upright あ (`vertical_text::turned_baseline_from_centre`).
                        // The upright character's centre stands 0.35 of its size right of `line_x`, so the baseline moves by the difference. TbRlV only (a page, or a
                        // cell of `tbRlV`): `tbRl` cells lay the Japanese down too and were not measured.
                        let line_x = match (direction, crate::render::vertical_text::turned_baseline_from_centre(&font.family)) {
                            (CellDirection::TbRlV, Some(below)) => line_x + (*size) * (0.35 - below),
                            _ => line_x,
                        };
                        out.push(Item::Turned { x: line_x, y: by + u, text: s, font: font.clone(), size: *size, color: *color, turns: 1, xscale: *xscale });
                    }
                }
                CellDirection::BtLr => {
                    out.push(Item::Turned { x: bx + shift + v, y: by + bh - u, text: s, font: font.clone(), size: *size, color: *color, turns: -1, xscale: *xscale });
                }
                CellDirection::LrTb | CellDirection::LrTbV => {}
            }
        }
    }
    out
}

/// `lrTbV`'s text: across, its Japanese characters turned anticlockwise
/// in their places (Word's PDF of DCTD).
fn turn_japanese(items: impl Iterator<Item = Item>) -> Vec<Item> {
    let mut out = Vec::new();
    for it in items {
        // A squeezed character (a bold fixed-width font, `w:w`, `fitText`) is its own item; it turns like the others.
        if let Item::Scaled { x, y, text, font, size, color, .. } = &it {
            if text.chars().next().is_some_and(is_cjk) {
                out.push(Item::Turned { x: x + size * 0.88, y: y + size * 0.12, text: text.clone(), font: font.clone(), size: *size, color: *color, turns: -1, xscale: 1.0 });
                continue;
            }
        }
        let Item::Text { x, y, text, offsets, font, size, color } = &it else {
            out.push(it);
            continue;
        };
        let mut rest = String::new();
        let mut rest_offsets = Vec::new();
        for ((_, c), dx) in text.char_indices().zip(offsets.iter()) {
            if is_cjk(c) {
                // Its em box where the upright one's was.
                out.push(Item::Turned { x: x + dx + size * 0.88, y: y + size * 0.12, text: c.to_string().into(), font: font.clone(), size: *size, color: *color, turns: -1, xscale: 1.0 });
            } else {
                rest.push(c);
                rest_offsets.push(*dx);
            }
        }
        if !rest.is_empty() {
            out.push(Item::Text { x: *x, y: *y, text: rest.into(), offsets: rest_offsets.into(), font: font.clone(), size: *size, color: *color });
        }
    }
    out
}

/// Where an item starts, downward (a character's top, from its baseline).
fn item_top(item: &Item) -> f32 {
    match item {
        Item::Text { y, size, .. } | Item::Turned { y, size, .. } | Item::Scaled { y, size, .. } => y - size * 0.8,
        Item::Rect { y, .. } | Item::Art { y, .. } | Item::Picture { y, .. } | Item::Shape { y, .. } | Item::Line { y, .. } | Item::Shading { y, .. } | Item::CommentBubble { y, .. } => *y,
        Item::Stroke { y, y2, .. } => y.min(*y2),
    }
}

impl RowSplit {
    /// The row's cells drawn `height` high with `contents`.
    fn draw(&self, contents: &[Vec<Item>], height: f32) -> Vec<Item> {
        let mut items = Vec::new();
        for (cell, content) in self.cells.iter().zip(contents) {
            if let Some(fill) = cell.fill {
                items.push(Item::Rect { x: cell.left, y: 0.0, w: cell.right - cell.left, h: height, color: fill });
            }
            items.extend(content.iter().cloned());
        }
        for cell in &self.cells {
            let [top, rgt, bottom, lft] = cell.borders;
            for (b, horizontal, outer_first, at, from, to) in [(top, true, true, 0.0, cell.left, cell.right), (bottom, true, false, height, cell.left, cell.right), (lft, false, true, cell.left, 0.0, height), (rgt, false, false, cell.right, 0.0, height)] {
                if let Some(b) = b {
                    items.extend(border_items(&b, horizontal, outer_first, at, from, to, false, false, true));
                }
            }
        }
        items
    }

    /// The row divided so that its first part fits in `room`: no line is
    /// cut, each cell's text goes on at its top on the next page. `None`
    /// when not even a line of each cell fits.
    fn split(&self, room: f32) -> Option<(Piece, Piece)> {
        let lines = |c: &SplitCell| -> Vec<(f32, f32)> { c.content.iter().filter_map(|it| if let Item::Line { y, h, .. } = it { Some((*y, y + h)) } else { None }).collect() };
        let mut cut = f32::INFINITY;
        for cell in &self.cells {
            let ls = lines(cell);
            if let Some(i) = ls.iter().position(|(_, b)| *b > room - cell.bottom + 0.01) {
                if i == 0 {
                    return None;
                }
                cut = cut.min(ls[i].0);
            }
        }
        // All of the text fits, but the row is higher by its minimum height.
        if !cut.is_finite() && self.min_height <= room + 0.01 {
            return None;
        }
        let mut first = Vec::new();
        let mut rest = Vec::new();
        let mut first_h = 0.0f32;
        let mut rest_h = 0.0f32;
        for cell in &self.cells {
            let mut a = Vec::new();
            let mut b = Vec::new();
            for it in cell.content.iter().cloned() {
                if let Item::Rect { x, y, w, h, color } = it
                    && y + h > cut + 0.5
                {
                    if y < cut - 1.0 {
                        a.push(Item::Rect { x, y, w, h: cut - y, color });
                        b.push(Item::Rect { x, y: cut, w, h: y + h - cut, color });
                    } else {
                        b.push(it);
                    }
                } else if item_top(&it) < cut - 0.01 {
                    a.push(it);
                } else {
                    b.push(it);
                }
            }
            let bottom_of = |items: &[Item]| items.iter().filter_map(|it| if let Item::Line { y, h, .. } = it { Some(y + h) } else { None }).fold(0.0f32, f32::max);
            first_h = first_h.max(bottom_of(&a).max(cell.top) + cell.bottom);
            let shift = cell.top - cut;
            let b: Vec<Item> = b.into_iter().map(|it| offset(it, 0.0, shift)).collect();
            rest_h = rest_h.max(bottom_of(&b).max(cell.top) + cell.bottom);
            first.push(a);
            rest.push(b);
        }
        let first_h = first_h.min(room).max(self.min_height.min(room));
        // The minimum height goes on over the pages.
        let remaining = (self.min_height - first_h).max(0.0);
        let rest_h = rest_h.max(remaining);
        // Each part's text is aligned (centre, foot) within the part's own height.
        let align = |parts: &[Vec<Item>], h: f32| -> Vec<Vec<Item>> {
            self.cells.iter().zip(parts).map(|(cell, items)| {
                let bottom = items.iter().filter_map(|it| if let Item::Line { y, h, .. } = it { Some(y + h) } else { None }).fold(cell.top, f32::max);
                let (room, content) = (h - cell.bottom - cell.top, bottom - cell.top);
                let dy = match cell.anchor {
                    CellAnchor::Top => 0.0,
                    CellAnchor::Center => ((room - content) / 2.0).max(0.0),
                    CellAnchor::Bottom => (room - content).max(0.0),
                };
                items.iter().cloned().map(|it| offset(it, 0.0, dy)).collect()
            }).collect()
        };
        let (first_aligned, rest_aligned) = (align(&first, first_h), align(&rest, rest_h));
        let piece = |items: Vec<Item>, height: f32, split: Option<Arc<RowSplit>>| Piece { height, items, page_break: false, column_break: false, keep_with_next: false, header: false, split, ..Default::default() };
        let rest_split = RowSplit { cells: self.cells.iter().zip(&rest).map(|(c, r)| SplitCell { content: r.clone(), ..c.clone() }).collect(), min_height: remaining };
        Some((piece(self.draw(&first_aligned, first_h), first_h, None), piece(self.draw(&rest_aligned, rest_h), rest_h, Some(Arc::new(rest_split)))))
    }
}

impl<'r> Layouter<'r> {
    /// The pieces of a header's or a footer's block: its lines are not on the document's line grid (Word's PDFs: the footer of corpus 142, on a 14.5 pt grid,
    /// has its two lines 12.74 pt apart; `footer-empty-134-variants-probe.docx`: an empty footer line in a 17.3 pt grid takes about 16 pt, not 17.3).
    fn header_footer_pieces(&self, block: &Block, width: f32, page_number: u32) -> Vec<Piece> {
        let saved = self.line_pitch.replace(None);
        let pieces = self.pieces(block, width, page_number, &[], true);
        self.line_pitch.set(saved);
        pieces
    }

    /// The pieces of a block `width` wide: a paragraph's lines (the space
    /// before goes with the first, after with the last) or a table's rows.
    /// The pieces of a block, laid out again only when the block (or what
    /// it depends on: the width, the list numbers, the page number for a
    /// page-number field) changed since it was laid out last: an edit lays
    /// out only the blocks it changed.
    fn pieces(&self, block: &Block, width: f32, page_number: u32, obstacles: &[WrapObstacleInPara], in_table: bool) -> Vec<Piece> {
        if !obstacles.is_empty() {
            return self.lay_out_block(block, width, page_number, obstacles, in_table);
        }
        let id = match block {
            Block::Paragraph(p) => p.id,
            Block::Table(t) => t.id,
        };
        let mut label = self.block_labels(block);
        // Its notes' numbers too (they change with the notes' settings).
        label.extend(self.block_note_numbers(block));
        // The number a note's own mark (`FlowRunKind::NoteMark`) shows is the note being laid out's: the block alone does not say it, so a block laid out for
        // another note, or another document, must not be taken from the cache.
        if let Block::Paragraph(p) = block
            && p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::NoteMark))
        {
            label.push((u32::MAX, self.current_note.borrow().clone()));
        }
        let fields = has_fields(block);
        let styles = Arc::as_ptr(&self.doc.styles) as usize;
        let key = (id, width.to_bits(), styles, in_table, self.line_pitch.get(), self.res.print, self.res.print_hidden_text, self.res.print_markup, self.res.markup_view.clone(), self.line_settings());
        let hit = PIECES.with(|cache| {
            let cache = cache.borrow();
            cache.get(&key).filter(|c| c.block == *block && c.labels == label && (!fields || (c.page == page_number && c.counts == self.counts()))).map(|c| c.pieces.clone())
        });
        if let Some(pieces) = hit {
            return pieces;
        }
        let pieces = self.lay_out_block(block, width, page_number, &[], in_table);
        PIECES.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() > 200_000 {
                cache.clear();
            }
            cache.insert(key, CachedPieces { block: block.clone(), labels: label, page: page_number, counts: self.counts(), pieces: pieces.clone() });
        });
        pieces
    }

    /// The numbers of the notes marked in a block, as (a key apart from the
    /// paragraphs' ids, the number shown).
    fn block_note_numbers(&self, block: &Block) -> Vec<(u32, String)> {
        fn walk(b: &Block, numbers: &std::collections::HashMap<(bool, u32), String>, out: &mut Vec<(u32, String)>) {
            match b {
                Block::Paragraph(p) => {
                    for r in &p.runs {
                        if let FlowRunKind::NoteRef { endnote, id } = r.kind {
                            let key = u32::MAX - (id << 1) - u32::from(endnote);
                            out.push((key, numbers.get(&(endnote, id)).cloned().unwrap_or_default()));
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).for_each(|b| walk(b, numbers, out)),
            }
        }
        let mut out = Vec::new();
        walk(block, &self.note_numbers, &mut out);
        out
    }

    /// The list labels of a block's paragraphs (a table's too).
    fn block_labels(&self, block: &Block) -> Vec<(u32, String)> {
        fn walk(b: &Block, labels: &std::collections::HashMap<u32, String>, out: &mut Vec<(u32, String)>) {
            match b {
                Block::Paragraph(p) => {
                    if let Some(l) = labels.get(&p.id) {
                        out.push((p.id, l.clone()));
                    }
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            for b in &cell.blocks {
                                walk(b, labels, out);
                            }
                        }
                    }
                }
            }
        }
        let mut out = Vec::new();
        if !self.labels.is_empty() {
            walk(block, &self.labels, &mut out);
        }
        out
    }

    fn lay_out_block(&self, block: &Block, width: f32, page_number: u32, obstacles: &[WrapObstacleInPara], in_table: bool) -> Vec<Piece> {
        let mut pieces = self.lay_out_block_pieces(block, width, page_number, obstacles, in_table);
        // A table that is not laid on the pages (in a cell, a header, a note) is on one page.
        if in_table {
            place_anchored_on_one_page(&mut pieces);
        }
        pieces
    }

    fn lay_out_block_pieces(&self, block: &Block, width: f32, page_number: u32, obstacles: &[WrapObstacleInPara], in_table: bool) -> Vec<Piece> {
        match block {
            Block::Paragraph(p) => {
                let lines = self.paragraph(p, width, page_number, obstacles, in_table);
                let count = lines.len();
                // Borders and shading: the box around the text, its borders
                // `space` away from it (outside the indents, and inside the
                // spacing before and after).
                let [top, right, bottom, left] = p.borders;
                let pad = |b: Option<FlowBorder>| b.map_or(0.0, |b| pt(b.space) + pt(b.total()));
                let (pad_top, pad_bottom) = (pad(top), pad(bottom));
                let boxed = p.shading.is_some() || p.shading_pattern.is_some() || p.borders.iter().any(Option::is_some);
                let text_left = pt(p.left.min(p.left + p.first_line));
                let text_right = width - pt(p.right);
                let inner_left = text_left - left.map_or(0.0, |b| pt(b.space));
                let inner_right = text_right + right.map_or(0.0, |b| pt(b.space));
                let top_shapes: Vec<&Box<ibis_doc_model::Shape>> = p.runs.iter().filter_map(|r| match &r.kind {
                    FlowRunKind::Shape(shape) if shape.floating.as_ref().is_some_and(|f| f.wrap == "topAndBottom") => Some(shape),
                    _ => None,
                }).collect();
                if top_shapes.is_empty() {
                    // Where each line's top is from the paragraph's top (the space before it, the border's room, the lines above): a shape placed
                    // from its paragraph (`relativeFrom="paragraph"`) is placed from the paragraph's top, the space before it included, and
                    // not from the line it is anchored in (Word's PDF of line-anchored-to-heading: a line anchored 4.4 pt below a paragraph with
                    // 6 pt before it is above the first line's text; placed from the line, it crossed the text). Below compatibility mode 15 it is the line's top
                    // (Word's PDF of corpus 049, mode 11: the lines of a 4.8 pt space before stand where the line's top puts them; 5 of the 6 documents of mode 15 with such shapes
                    // came nearer to Word's PDF, 007, 011, 046, 083, 091, and the mode 11 one went away from it).
                    let line_tops: Vec<f32> = lines.iter().scan(0.0f32, |sum, l| { let top = *sum; *sum += l.height; Some(top) }).collect();
                    let first_line_top = self.space_before(p) + pad_top;
                    return lines
                        .into_iter()
                        .enumerate()
                        .map(|(i, l)| {
                            let before = if i == 0 { self.space_before(p) } else { 0.0 };
                            let after = if i + 1 == count { self.space_after(p) } else { 0.0 };
                            let above = if i == 0 { pad_top } else { 0.0 };
                            let below = if i + 1 == count { pad_bottom } else { 0.0 };
                            let mut items: Vec<Item> = Vec::new();
                            if boxed {
                                // This line's band of the box, from `y0` to `y1`.
                                let y0 = before + if i == 0 { top.map_or(0.0, |b| pt(b.total())) } else { 0.0 };
                                let y1 = before + above + l.height + below - if i + 1 == count { bottom.map_or(0.0, |b| pt(b.total())) } else { 0.0 };
                                if p.shading.is_some() || p.shading_pattern.is_some() {
                                    let bg = if p.shading_pattern.is_some() { p.shading_fill } else { p.shading };
                                    items.extend(shading_items(p.shading_pattern, bg, p.shading_color, inner_left, y0, inner_right - inner_left, y1 - y0));
                                }
                                let lx = inner_left - left.map_or(0.0, |b| pt(b.total()) / 2.0);
                                let rx = inner_right + right.map_or(0.0, |b| pt(b.total()) / 2.0);
                                if let (Some(b), 0) = (top, i) {
                                    items.extend(border_items(&b, true, true, before + pt(b.total()) / 2.0, lx, rx, left.is_some(), right.is_some(), false));
                                }
                                if let Some(b) = bottom.filter(|_| i + 1 == count) {
                                    items.extend(border_items(&b, true, false, y1 + pt(b.total()) / 2.0, lx, rx, left.is_some(), right.is_some(), false));
                                }
                                // The side borders go over the space before and after
                                // when no top or bottom border bounds it (Word's PDF of
                                // DWID: one rectangle per paragraph, end to end).
                                let ty = if i == 0 { if top.is_none() { 0.0 } else { before } } else { 0.0 };
                                let by = if i + 1 == count { y1 + bottom.map_or(after, |b| pt(b.total())) } else { before + above + l.height };
                                if let Some(b) = left {
                                    items.extend(border_items(&b, false, true, lx, ty, by, false, false, false));
                                }
                                if let Some(b) = right {
                                    items.extend(border_items(&b, false, false, rx, ty, by, false, false, false));
                                }
                            }
                            let from_paragraph_top = if i == 0 { 0.0 } else { first_line_top + line_tops[i] };
                            items.extend(l.items.into_iter().map(|it| match &it {
                                Item::Shape { shape, .. } if self.doc.compat_mode.is_some_and(|m| m >= 15) && shape.floating.as_ref().is_some_and(|f| f.relative_v == "paragraph" && f.align_v.is_none()) => offset(it, 0.0, -from_paragraph_top),
                                _ => offset(it, 0.0, before + above),
                            }));
                            for it in items.iter_mut() {
                                if let Item::Line { pad, .. } = it {
                                    *pad = (before, after);
                                }
                            }
                            let mode = self.markup_mode();
                            if (mode == MarkupDisplayMode::All || mode == MarkupDisplayMode::Simple) && l.has_revision && !in_table {
                                let margin_l = pt(self.doc.page.left);
                                let bar_x = (inner_left - 10.0).max(-margin_l + 8.0);
                                items.push(Item::Rect {
                                    x: bar_x,
                                    y: before + above,
                                    w: 1.5,
                                    h: l.height,
                                    color: hsla(ibis_doc_model::Color::opaque(REVISION_COLOR)),
                                });
                            }
                            if (mode == MarkupDisplayMode::All || mode == MarkupDisplayMode::Simple) && !l.comment_marks.is_empty() && !in_table {
                                let margin_l = pt(self.doc.page.left);
                                let bubble_size = 16.0;
                                let bubble_x = pt(self.doc.page.size.width) - margin_l + 8.0;
                                let bubble_y = before + above + (l.height - bubble_size).max(0.0) / 2.0;
                                items.push(Item::CommentBubble {
                                    x: bubble_x,
                                    y: bubble_y,
                                    size: bubble_size,
                                });
                            }
                            let page_break = l.page_break || (matches!(p.section_break, SectionBreak::NextPage | SectionBreak::OddPage | SectionBreak::EvenPage) && i + 1 == count);
                            let column_break = l.column_break || (p.section_break == SectionBreak::NextColumn && i + 1 == count);
                            // An empty paragraph whose mark is hidden takes no room (Word joins it to the next one when hidden text is not shown).
                            let hidden_mark = p.runs.is_empty() && p.mark.hidden && !(if self.res.print { self.res.print_hidden_text } else { self.res.markup_view.show_hidden_text });
                            let height = if hidden_mark || (p.framed && in_table && self.current_cell.get().is_none()) { 0.0 } else { before + above + l.height + below + after };
                            let (before, after) = if hidden_mark { (0.0, 0.0) } else { (before, after) };
                            Piece { height, items, page_break, column_break, keep_with_next: p.keep_with_next && i + 1 == count, header: false, split: None, table_bounds: None, anchored: Vec::new(), space_before: before, space_after: after, grid_below: if below > 0.0 || hidden_mark { 0.0 } else { l.grid_below } }
                        })
                        .collect();
                }

                let mut shape_pieces = Vec::new();
                let has_lines = count > 0;
                let page_w = pt(self.doc.page.size.width);
                let margin_l = pt(self.doc.page.left);
                let margin_r = pt(self.doc.page.right);
                let is_even = page_number % 2 == 0;
                for (si, shape) in top_shapes.iter().enumerate() {
                    let sw = pt(shape.frame.width);
                    let sh = self.fitted_shape_height(shape);
                    let f = shape.floating.as_ref().unwrap();
                    let (aw, _, rx, _) = aligned_size(f, sw, sh);
                    let (base_x0, base_w) = match f.relative_h.as_str() {
                        "page" => (0.0, page_w),
                        "leftMargin" => (0.0, margin_l),
                        "rightMargin" => (page_w - margin_r, margin_r),
                        "insideMargin" => if is_even { (page_w - margin_r, margin_r) } else { (0.0, margin_l) },
                        "outsideMargin" => if is_even { (0.0, margin_l) } else { (page_w - margin_r, margin_r) },
                        "character" => (margin_l, width),
                        _ /* "column" | "margin" */ => (margin_l, width),
                    };

                    let abs_x = match f.align_h.as_deref() {
                        Some("left") => base_x0,
                        Some("center") => base_x0 + (base_w - aw) / 2.0,
                        Some("right") => base_x0 + base_w - aw,
                        Some("inside") => if is_even { base_x0 + base_w - aw } else { base_x0 },
                        Some("outside") => if is_even { base_x0 } else { base_x0 + base_w - aw },
                        _ => base_x0 + pt(f.offset_x),
                    };
                    let abs_x = abs_x.clamp(0.0, (page_w - aw).max(0.0)) + rx;
                    let sx = abs_x - margin_l;
                    let before = if si == 0 { self.space_before(p) } else { 0.0 };
                    let above = if si == 0 { pad_top } else { 0.0 };
                    let after = if !has_lines && si + 1 == top_shapes.len() { self.space_after(p) } else { 0.0 };
                    let below = if !has_lines && si + 1 == top_shapes.len() { pad_bottom } else { 0.0 };
                    let dt = pt(f.dist_t);
                    let db = pt(f.dist_b);
                    let items = vec![Item::Shape {
                        shape: (*shape).clone(),
                        x: sx,
                        y: before + above + dt,
                        w: sw,
                        h: sh,
                    }];
                    shape_pieces.push(Piece {
                        height: before + above + dt + sh + db + below + after,
                        items,
                        page_break: !has_lines && si + 1 == top_shapes.len() && matches!(p.section_break, SectionBreak::NextPage | SectionBreak::OddPage | SectionBreak::EvenPage),
                        column_break: !has_lines && si + 1 == top_shapes.len() && p.section_break == SectionBreak::NextColumn,
                        keep_with_next: !has_lines && si + 1 == top_shapes.len() && p.keep_with_next,
                        header: false,
                        split: None,
                        ..Default::default()
                    });
                }

                let text_pieces: Vec<Piece> = lines
                    .into_iter()
                    .enumerate()
                    .map(|(i, l)| {
                        let after = if i + 1 == count { self.space_after(p) } else { 0.0 };
                        let below = if i + 1 == count { pad_bottom } else { 0.0 };
                        let mut items: Vec<Item> = Vec::new();
                        if boxed {
                            let y0 = 0.0;
                            let y1 = l.height + below - if i + 1 == count { bottom.map_or(0.0, |b| pt(b.total())) } else { 0.0 };
                            if p.shading.is_some() || p.shading_pattern.is_some() {
                                let bg = if p.shading_pattern.is_some() { p.shading_fill } else { p.shading };
                                items.extend(shading_items(p.shading_pattern, bg, p.shading_color, inner_left, y0, inner_right - inner_left, y1 - y0));
                            }
                            let lx = inner_left - left.map_or(0.0, |b| pt(b.total()) / 2.0);
                            let rx = inner_right + right.map_or(0.0, |b| pt(b.total()) / 2.0);
                            if let Some(b) = bottom.filter(|_| i + 1 == count) {
                                items.extend(border_items(&b, true, false, y1 + pt(b.total()) / 2.0, lx, rx, left.is_some(), right.is_some(), false));
                            }
                            let (ty, by) = (0.0, if i + 1 == count { y1 + bottom.map_or(0.0, |b| pt(b.total())) } else { l.height });
                            if let Some(b) = left {
                                items.extend(border_items(&b, false, true, lx, ty, by, false, false, false));
                            }
                            if let Some(b) = right {
                                items.extend(border_items(&b, false, false, rx, ty, by, false, false, false));
                            }
                        }
                        items.extend(l.items);
                        let mode = self.markup_mode();
                        if (mode == MarkupDisplayMode::All || mode == MarkupDisplayMode::Simple) && l.has_revision && !in_table {
                            let bar_x = (inner_left - 10.0).max(-margin_l + 8.0);
                            items.push(Item::Rect {
                                x: bar_x,
                                y: 0.0,
                                w: 1.5,
                                h: l.height,
                                color: hsla(ibis_doc_model::Color::opaque(REVISION_COLOR)),
                            });
                        }
                        if (mode == MarkupDisplayMode::All || mode == MarkupDisplayMode::Simple) && !l.comment_marks.is_empty() && !in_table {
                            let bubble_size = 16.0;
                            let bubble_x = pt(self.doc.page.size.width) - margin_l + 8.0;
                            let bubble_y = (l.height - bubble_size).max(0.0) / 2.0;
                            items.push(Item::CommentBubble {
                                x: bubble_x,
                                y: bubble_y,
                                size: bubble_size,
                            });
                        }
                        let page_break = l.page_break || (matches!(p.section_break, SectionBreak::NextPage | SectionBreak::OddPage | SectionBreak::EvenPage) && i + 1 == count);
                        let column_break = l.column_break || (p.section_break == SectionBreak::NextColumn && i + 1 == count);
                        let height = if p.framed && in_table && self.current_cell.get().is_none() { 0.0 } else { l.height + below + after };
                        Piece { height, items, page_break, column_break, keep_with_next: p.keep_with_next && i + 1 == count, header: false, split: None, table_bounds: None, anchored: Vec::new(), space_before: 0.0, space_after: after, grid_below: l.grid_below }
                    })
                    .collect();

                // A box that wraps text above and below and is placed from the paragraph's top
                // (`posOffset`): Word puts it that far down, the lines that end above it stay
                // over it, the rest go below.
                let from_top = (top_shapes.len() == 1 && shape_pieces.len() == 1 && has_lines)
                    .then(|| top_shapes[0].floating.as_ref())
                    .flatten()
                    .filter(|f| f.relative_v == "paragraph" && f.align_v.is_none() && f.offset_y > 0)
                    .map(|f| pt(f.offset_y));
                if let Some(offset_y) = from_top {
                    // The room above the first line (the spacing before, the border's).
                    let pre = self.space_before(p) + pad_top;
                    let mut lead = Vec::new();
                    let mut rest = Vec::new();
                    let mut used = 0.0f32;
                    for piece in text_pieces {
                        if rest.is_empty() && used + piece.height <= offset_y + 0.5 {
                            used += piece.height;
                            lead.push(piece);
                        } else {
                            rest.push(piece);
                        }
                    }
                    let mut gap = (offset_y - used).max(0.0);
                    if let Some(first) = lead.first_mut() {
                        first.height += pre;
                        first.items = std::mem::take(&mut first.items).into_iter().map(|it| offset(it, 0.0, pre)).collect();
                    } else {
                        gap += pre;
                    }
                    let mut shape_piece = shape_pieces.remove(0);
                    let dist_t = pt(top_shapes[0].floating.as_ref().map_or(0, |f| f.dist_t));
                    let old_top = pre + dist_t;
                    for it in &mut shape_piece.items {
                        if let Item::Shape { y, .. } = it {
                            *y = gap;
                        }
                    }
                    shape_piece.height += gap - old_top;
                    lead.push(shape_piece);
                    lead.extend(rest);
                    return lead;
                }
                shape_pieces.extend(text_pieces);
                shape_pieces
            }
            Block::Table(t) => self.table(t, width, page_number),
        }
    }

    /// How thick the lines of a vertical cell's paragraphs are together (points): the paragraphs laid out along an unbounded length, each one line of its own.
    fn vertical_thickness(&self, blocks: &[Block], page_number: u32) -> f32 {
        blocks.iter().filter(|b| matches!(b, Block::Paragraph(_))).flat_map(|b| self.pieces(b, 10_000.0, page_number, &[], true)).map(|q| q.height).sum()
    }

    /// The narrowest and widest a cell's content can be laid out (points,
    /// without the cell's margins): its longest word (or character, in
    /// Japanese), and its longest line unbroken.
    fn content_widths(&self, blocks: &[Block], page_number: u32) -> (f32, f32, f32) {
        let (mut least, mut most, mut held) = (0.0f32, 0.0f32, 0.0f32);
        for block in blocks {
            match block {
                Block::Paragraph(p) => {
                    let (units, _) = self.units(p, page_number);
                    // `held`: the least a column holds, its longest word with the paragraph's indents beside it (a word can sit no nearer the cell's edge than they allow:
                    // Word's PDF of a narrow cell's right aligned 「m」 and a superscript 「2」, an indent of 14 pt: the column is wider than its width, so as to hold them on one line).
                    let indent = pt(p.left) + pt(p.right);
                    // The first word of a paragraph sits on its first line, where the first line's indent puts it (a hanging indent brings it to the edge); the others can be
                    // on the lines after it (Word's PDFs of `autofit-b-*.docx`: a first line indent, a hanging indent and indents at both sides).
                    let first_indent = (pt(p.left) + pt(p.first_line)).max(0.0) + pt(p.right).max(0.0);
                    // A list item's indent is the room of its number, and a paragraph with no text has no word to put beside its indent (Word's PDF of
                    // `autofit-indent-narrow-col.docx`: a cell with only a number, or an empty one with a left indent of 42 pt, keeps its 25.5 pt column).
                    let has_text = p.list.is_none() && units.iter().any(|u| matches!(u, Unit::Char { c, .. } if !c.is_whitespace() && *c != '\u{200b}'));
                    let mut starts = true;
                    let first = pt(p.first_line).max(0.0);
                    let (mut line, mut word, mut prev) = (first, 0.0f32, None);
                    let mut trailing = 0.0f32;
                    for u in &units {
                        match u {
                            Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak => {
                                most = most.max(indent + line - trailing);
                                least = least.max(word);
                                if has_text && word > 0.0 {
                                    held = held.max(if starts { first_indent } else { indent.max(0.0) } + word);
                                }
                                (line, word, prev, trailing) = (0.0, 0.0, None, 0.0);
                                starts = true;
                            }
                            Unit::Tab | Unit::ListTab => {
                                // A tab to the next default stop.
                                let stop = self.default_tab_pt();
                                line = ((line / stop).floor() + 1.0) * stop;
                                least = least.max(word);
                                if has_text && word > 0.0 {
                                    held = held.max(if starts { first_indent } else { indent.max(0.0) } + word);
                                    starts = false;
                                }
                                (word, prev, trailing) = (0.0, None, 0.0);
                            }
                            _ => {
                                let c = unit_char(u);
                                let w = unit_width(u);
                                // A paragraph that does not wrap by words (`w:wordWrap` 0) cuts a word that is longer than the line (Word's PDF of corpus 072: an address of 288 pt in a
                                // cell of 149 pt is on two lines and the cell keeps its width), so a Latin word is no least width of the cell (one that fits the line is not cut: it goes down
                                // whole, as the lines are broken).
                                let cuttable = !p.word_wrap && prev.is_some_and(|b: char| b.is_ascii_alphabetic()) && c.is_some_and(|a: char| a.is_ascii_alphabetic());
                                if cuttable || may_break(prev, c, p.kinsoku, p.word_wrap, self.doc) {
                                    least = least.max(word);
                                if has_text && word > 0.0 {
                                    held = held.max(if starts { first_indent } else { indent.max(0.0) } + word);
                                    starts = false;
                                }
                                    word = 0.0;
                                }
                                if c == Some(' ') {
                                    trailing += w;
                                } else {
                                    trailing = 0.0;
                                    word += w;
                                }
                                line += w;
                                prev = c;
                            }
                        }
                    }
                    most = most.max(indent + line - trailing);
                    least = least.max(word);
                                if has_text && word > 0.0 {
                                    held = held.max(if starts { first_indent } else { indent.max(0.0) } + word);
                                }
                }
                Block::Table(t) => {
                    let w: f32 = match self.fitted_columns(t, f32::MAX / 4.0, page_number) {
                        Some(c) => c.iter().sum(),
                        None => t.columns.iter().map(|c| pt(*c)).sum(),
                    };
                    least = least.max(w);
                    held = held.max(w);
                    most = most.max(w);
                }
            }
        }
        (least, most.max(least), held.max(least))
    }

    /// An automatically fitted table's columns (points) in `available`
    /// points, as Word's 自動調整 lays them out: each column the width its
    /// cells prefer, else its content's unbroken width (at least its
    /// longest word); then the table's preferred width shared out (in
    /// proportion), or the columns narrowed towards their longest words
    /// to fit. `None` to use the grid.
    fn fitted_columns(&self, t: &FlowTable, available: f32, page_number: u32) -> Option<Vec<f32>> {
        let fit = t.autofit.as_ref()?;
        if fit.cells.len() != t.rows.len() || fit.cells.iter().zip(&t.rows).any(|(w, r)| w.len() != r.cells.len()) {
            return None;
        }
        let n = t.columns.len();
        // The text's width, with the cell margins a table of the older compatibility modes reaches out over.
        let outdent = self.table_outdent(t);
        let available = if available < f32::MAX / 8.0 { available + outdent } else { available };
        // A width that is the text's (a percent, or the room a wide table is narrowed to) is of the table's outer edges in mode 15: its end borders
        // are inside it, half of each on the grid's side (GitHub issue #178, Word's PDFs: tables of 100 % and of the text's width have their end
        // lines' middles 450.9 pt apart in 451.3 pt of text, with borders of 0.5 pt).
        let borders_in = self.borders_inside_width(t);
        let target = match fit.width {
            PreferredWidth::Fixed(w) => Some(pt(w)),
            PreferredWidth::Percent(p) if available < f32::MAX / 8.0 => Some(available * p as f32 / 5000.0 - borders_in),
            _ => None,
        };
        // A cell's preferred width in points (a percent of the table's: its width, else what the grid adds up to when it gives the
        // widths, else the text's; GitHub issue #178, Word's PDFs: 66.7 % of a table of 360 pt is 240 pt, of the 451 pt text 301).
        let grid_total: f32 = t.columns.iter().map(|c| pt(*c)).sum();
        let base = target.unwrap_or(if fit.grid_given { grid_total } else { available });
        let prefer = |w: PreferredWidth, grid: f32| match w {
            PreferredWidth::Fixed(w) => Some(pt(w)),
            PreferredWidth::Percent(p) if base < f32::MAX / 8.0 => Some(base * p as f32 / 5000.0),
            // `nil`: the grid's width.
            PreferredWidth::Nil => Some(grid),
            _ => None,
        };
        let (mut least, mut most) = (vec![0.0f32; n], vec![0.0f32; n]);
        let mut held_least = vec![0.0f32; n];
        let mut preferred: Vec<Option<f32>> = vec![None; n];
        let mut spanning = Vec::new();
        for (r_idx, (row, widths)) in t.rows.iter().zip(&fit.cells).enumerate() {
            let mut col = 0;
            for (cell, w) in row.cells.iter().zip(widths) {
                let span = (cell.span.max(1) as usize).min(n.saturating_sub(col)).max(1);
                if col >= n {
                    break;
                }
                let margins = pt(cell.margins[1]) + pt(cell.margins[3]);
                self.current_cell.set(Some((t.id, r_idx, col)));
                let (lo, hi, held) = if cell.merge == VerticalMerge::Continue {
                    (0.0, 0.0, 0.0)
                } else if cell.direction.vertical() {
                    // A cell of vertical text is as wide as its lines are thick, not as long as its text (Word's PDF of the issue #271 form: a column of 22.2 pt
                    // holds a cell of nine characters, `noWrap`, in one line of 12 pt; the text's length is the height's).
                    let thickness = self.vertical_thickness(&cell.blocks, page_number);
                    (thickness, thickness, thickness)
                } else {
                    self.content_widths(&cell.blocks, page_number)
                };
                self.current_cell.set(None);
                // Per ECMA-376 §17.4.42 (`w:noWrap`), a cell with an explicit width (`Fixed` / `Percent`) wraps at that width.
                let lo = if cell.no_wrap && matches!(*w, PreferredWidth::Auto) { hi } else { lo };
                let (lo, hi, held) = (lo + margins, hi + margins, held + margins);
                if span == 1 {
                    least[col] = least[col].max(lo);
                    held_least[col] = held_least[col].max(held);
                    most[col] = most[col].max(hi);
                    if let Some(p) = prefer(*w, pt(t.columns[col])) {
                        preferred[col] = Some(preferred[col].map_or(p, |q: f32| q.max(p)));
                    }
                } else {
                    spanning.push((col, span, lo, prefer(*w, t.columns[col..col + span].iter().map(|c| pt(*c)).sum()).unwrap_or(hi).max(lo)));
                }
                col += span;
            }
        }
        // A column no cell of its own sits in (only cells spanning it do) is as wide as the grid says.
        let mut widths: Vec<f32> = (0..n).map(|c| preferred[c].unwrap_or(if most[c] > 0.0 { most[c] } else { pt(t.columns[c]) }).max(least[c])).collect();
        // A column that cannot hold its longest word beside the paragraph's indents is widened to it, and is narrowed no further than that (Word's PDFs of `autofit-a-*.docx`).
        let held: Vec<f32> = (0..n).map(|c| widths[c].max(held_least[c])).collect();
        // A grid that says the widths is kept while every column holds its least width (a grid Word wrote is that of its own layout).
        if fit.grid_trusted && (0..n).all(|c| held_least[c] <= pt(t.columns[c]) + 0.5) {
            return None;
        }
        widths = held;
        // Columns only spanned: their share of the spanning cells' widths.
        for &(col, span, lo, want) in &spanning {
            let have: f32 = widths[col..col + span].iter().sum();
            let need = want.max(lo);
            if need > have {
                let empty: Vec<usize> = (col..col + span).filter(|&c| widths[c] == 0.0).collect();
                let share = if empty.is_empty() { (col..col + span).collect() } else { empty };
                for &c in &share {
                    widths[c] += (need - have) / share.len() as f32;
                }
            }
            for c in col..col + span {
                least[c] = least[c].max(lo / span as f32);
            }
        }
        let total: f32 = widths.iter().sum();
        let goal = match target {
            Some(goal) => goal,
            None if total > available => available - borders_in,
            None => return Some(widths.into_iter().map(|w| (w / 0.24 - 0.01).ceil() * 0.24).collect()),
        };
        if total < goal {
            // Widened: the columns without a preferred width take the room
            // (in proportion to their widths), else all of them.
            let free: Vec<usize> = (0..n).filter(|&c| preferred[c].is_none()).collect();
            let grow = if free.is_empty() { (0..n).collect() } else { free };
            let sum: f32 = grow.iter().map(|&c| widths[c]).sum();
            for &c in &grow {
                widths[c] += (goal - total) * if sum > 0.0 { widths[c] / sum } else { 1.0 / grow.len() as f32 };
            }
        } else if total > goal {
            // Narrowed towards the longest words, in proportion to how much
            // narrower each can be.
            // (What a column holds at the least is its longest word with the paragraph's indents beside it, `held_least`.)
            let floors: Vec<f32> = (0..n).map(|c| least[c].max(held_least[c])).collect();
            let floor: f32 = floors.iter().sum();
            let k = if total > floor { ((goal - floor) / (total - floor)).max(0.0) } else { 0.0 };
            for c in 0..n {
                widths[c] = floors[c] + (widths[c] - floors[c]).max(0.0) * k;
            }
        }
        // Fitted to a width (the preferred one, or the room): the columns add up to it. Each is not rounded up to Word's 1/300 inch here as the natural
        // ones are above (that made the table up to 0.24 pt a column wider than its width, GitHub issue #178): `table` rounds the edges.
        Some(widths)
    }

    /// Half of the first cell's left border and half of the last cell's right border of a table's first row (0 before mode 15 and in a cell),
    /// which a width of the text's is less by in mode 15 (`fitted_columns`).
    fn borders_inside_width(&self, t: &FlowTable) -> f32 {
        if !self.doc.compat_mode.is_some_and(|m| m >= 15) || self.current_cell.get().is_some() {
            return 0.0;
        }
        let Some(row) = t.rows.first() else { return 0.0 };
        let half = |b: Option<FlowBorder>| b.map_or(0.0, |b| pt(b.total()) / 2.0);
        half(row.cells.first().and_then(|c| c.borders[3])) + half(row.cells.last().and_then(|c| c.borders[1]))
    }

    /// How far a table reaches out over the text's width on both sides in the compatibility modes before Word 2013's (15), where its width
    /// and its text's left edge are those of the text and the table's cell margins hang outside them: the first row's first cell's left margin
    /// and last cell's right margin (GitHub issue #178, Word's PDFs: a table of the text's width, 100 % or too wide for it, is wider by the
    /// two margins, 10.8 pt at 5.4 pt each, and starts a margin out to the left, as the reader already puts it by `w:tblInd`).
    fn table_outdent(&self, t: &FlowTable) -> f32 {
        // (A table in a cell does not: its percent is of the cell's text width, Word's XPS of nested tables, #178.)
        if self.doc.compat_mode.is_some_and(|m| m >= 15) || self.current_cell.get().is_some() {
            return 0.0;
        }
        let Some(row) = t.rows.first() else { return 0.0 };
        row.cells.first().map_or(0.0, |c| pt(c.margins[3])) + row.cells.last().map_or(0.0, |c| pt(c.margins[1]))
    }

    /// The grid's columns cut down in proportion to fit `width` when they are wider (as Word does: measured on its PDFs of tables
    /// of 5000×3 twips in 9638: 160.8 pt each), no column less than its content's least width.
    fn shrunk_columns(&self, t: &FlowTable, width: f32, page_number: u32) -> Vec<Emu> {
        let total: Emu = t.columns.iter().sum();
        // A table up to its cell margins wider than the text is left as it is (it reaches that far out on both sides: Word's PDF of a table
        // 492.7 pt wide in 481.9 pt of text stays 492.7 wide).
        let outdent = t.rows.first().and_then(|r| r.cells.first()).map_or(0.0, |c| pt(c.margins[1]) + pt(c.margins[3]));
        if pt(total) <= width + outdent + 0.5 || t.columns.is_empty() {
            return t.columns.clone();
        }
        // (A table of the older compatibility modes is cut down to the text's width and the margins it reaches out over, #178.)
        let width = width + self.table_outdent(t);
        let n = t.columns.len();
        let mut least = vec![0.0f32; n];
        for (r, row) in t.rows.iter().enumerate() {
            let mut col = 0;
            for cell in &row.cells {
                let span = cell.span.max(1) as usize;
                if span == 1 && col < n && cell.merge != VerticalMerge::Continue {
                    self.current_cell.set(Some((t.id, r, col)));
                    let (lo, _, _) = self.content_widths(&cell.blocks, page_number);
                    self.current_cell.set(None);
                    least[col] = least[col].max(lo + pt(cell.margins[1]) + pt(cell.margins[3]));
                }
                col += span;
            }
        }
        let want: Vec<f32> = t.columns.iter().map(|c| pt(*c)).collect();
        let mut fixed = vec![false; n];
        let mut out = want.clone();
        for _ in 0..n {
            let used: f32 = (0..n).filter(|&c| fixed[c]).map(|c| out[c]).sum();
            let rest: f32 = (0..n).filter(|&c| !fixed[c]).map(|c| want[c]).sum();
            if rest <= 0.0 {
                break;
            }
            let k = ((width - used) / rest).max(0.0);
            let mut changed = false;
            for c in 0..n {
                if !fixed[c] {
                    out[c] = want[c] * k;
                    if out[c] < least[c].min(want[c]) {
                        out[c] = least[c].min(want[c]);
                        fixed[c] = true;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        out.iter().map(|w| (*w as f64 * EMU_PER_POINT as f64) as Emu).collect()
    }

    /// A table's rows as pieces. Vertically merged cells are drawn in the
    /// row they start, as high as the rows they cover.
    fn table(&self, t: &FlowTable, width: f32, page_number: u32) -> Vec<Piece> {
        // The columns' edges from the table's left edge, rounded to Word's
        // 1/300 inch (measured on its PDF of DTBB: edges at 160, 280 and
        // 1731 twips are at 7.92, 13.92 and 86.64 pt).
        let mut xs = vec![0.0f32];
        // The same edges not rounded: the text's room is by them (a line that
        // fits the cell's width is not broken by the rounding).
        let mut exact = vec![0.0f32];
        let mut edge = 0;
        let columns: Vec<Emu> = match self.fitted_columns(t, width, page_number) {
            Some(fitted) => fitted.iter().map(|w| (*w as f64 * EMU_PER_POINT as f64) as Emu).collect(),
            None if t.fit_to_page && width < f32::MAX / 8.0 => self.shrunk_columns(t, width, page_number),
            // A fixed table's width in percent of the text's (the columns cut or stretched in proportion, GitHub issue #178).
            None if t.fixed_percent.is_some() && width < f32::MAX / 8.0 && !t.columns.is_empty() => {
                let sum: Emu = t.columns.iter().sum();
                let goal = ((width + self.table_outdent(t)) as f64 * f64::from(t.fixed_percent.unwrap_or(0)) / 5000.0 - self.borders_inside_width(t) as f64).max(0.0) * EMU_PER_POINT as f64;
                if sum > 0 { t.columns.iter().map(|c| (*c as f64 * goal / sum as f64) as Emu).collect() } else { t.columns.clone() }
            }
            None => t.columns.clone(),
        };
        for c in &columns {
            edge += *c;
            // 3 048 EMU: 1/300 inch.
            xs.push((edge as f64 / 3048.0).round() as f32 * 0.24);
            exact.push((edge as f64 / EMU_PER_POINT as f64) as f32);
        }
        let total = *xs.last().unwrap_or(&0.0);
        // The indent is to the table's left edge.
        let x0 = match t.align {
            FlowAlign::Center => (width - total) / 2.0,
            FlowAlign::Right => width - total,
            // Word puts the outer edge of the table's left border, not its middle, at the left edge: the grid starts half the border in
            // (GitHub issue #178, Word's PDFs: borders of 3, 1 and 0.5 pt have their middles at 73.5, 72.5 and 72.25 pt for 72).
            // (Before mode 15 it does not: the border's middle is on the cell's edge, whatever its width. Word's PDFs of `n1-table-compat14.docx` with borders of
            // 0.5, 1.5 and 3 pt: the left line's middle at 66.62, 66.62 and 66.56 for 72 less the margin 5.4.)
            _ if !self.doc.compat_mode.is_some_and(|m| m >= 15) => pt(t.indent),
            _ => pt(t.indent) + t.rows.first().and_then(|r| r.cells.first()).and_then(|c| c.borders[3]).map_or(0.0, |b| pt(b.total()) / 2.0),
        };
        // Lay out each cell: its column, span, content and content height.
        struct Laid {
            col: usize,
            span: usize,
            items: Vec<Item>,
            content: f32,
            /// Vertical text: its lines' total thickness (across the cell).
            across: f32,
        }
        impl HasCol for Laid {
            fn col(&self) -> usize {
                self.col
            }
        }
        impl HasSpan for Laid {
            fn col_end(&self) -> usize {
                self.col + self.span
            }
        }
        // A cell's blocks laid out `inner` wide (a vertical cell's: with
        // its paragraphs left aligned, to measure its natural length).
        let lay_cell = |cell: &ibis_doc_model::FlowCell, r_idx: usize, col: usize, inner: f32, natural: bool| -> (Vec<Item>, f32) {
            let mut y = 0.0;
            let mut items = Vec::new();
            // A floating table in the cell: (top, bottom, left, right) of what the text keeps off, and how low it reaches.
            let mut float_obs: Option<(f32, f32, f32, f32)> = None;
            let mut float_bottom = 0.0f32;
            let mut float_top = 0.0f32;
            if cell.merge != VerticalMerge::Continue {
                self.current_cell.set(Some((t.id, r_idx, col)));
                // The space after a paragraph of the cell that the next one's space before takes in (the larger of the two, as in the body).
                let mut prev_after = 0.0f32;
                for (b, block) in cell.blocks.iter().enumerate() {
                    // The empty paragraph that ends a cell after a
                    // nested table takes no room (as Word prints it).
                    let after_table = b > 0 && b + 1 == cell.blocks.len() && matches!(cell.blocks[b - 1], Block::Table(_));
                    if after_table && matches!(block, Block::Paragraph(p) if p.runs.is_empty()) {
                        continue;
                    }
                    let left_aligned;
                    let block = match block {
                        Block::Paragraph(p) if natural && p.align != FlowAlign::Left => {
                            let mut p = p.clone();
                            p.align = FlowAlign::Left;
                            left_aligned = Block::Paragraph(p);
                            &left_aligned
                        }
                        _ => block,
                    };
                    // A floating table in the cell sits where it says, the text round it (Word: the cell is as high as the
                    // text or the table's bottom, whichever is lower; the lines it leaves no room for go below it).
                    if let Block::Table(inner_table) = block
                        && let Some(f) = inner_table.floating
                    {
                        use ibis_doc_model::TableAlign;
                        let pieces = self.pieces(block, inner, page_number, &[], true);
                        let h: f32 = pieces.iter().map(|q| q.height).sum();
                        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                        for (a, b) in pieces.iter().filter_map(|q| q.table_bounds) {
                            lo = lo.min(a);
                            hi = hi.max(b);
                        }
                        for it in pieces.iter().flat_map(|q| &q.items) {
                            let (x0, x1) = match it {
                                Item::Rect { x, w, .. } | Item::Shading { x, w, .. } => (*x, x + w),
                                Item::Stroke { x, x2, w, .. } => (x.min(*x2) - w / 2.0, x.max(*x2) + w / 2.0),
                                _ => continue,
                            };
                            lo = lo.min(x0);
                            hi = hi.max(x1);
                        }
                        if lo > hi {
                            (lo, hi) = (0.0, 0.0);
                        }
                        let w = hi - lo;
                        let left = match f.x_align {
                            Some(TableAlign::Center) => (inner - w) / 2.0,
                            Some(TableAlign::End) => inner - w,
                            Some(TableAlign::Start) => 0.0,
                            _ => pt(f.x),
                        };
                        let mut top = y + pt(f.y);
                        let [dl, dr, dt, db] = f.distance.map(pt);
                        // Under a floating table it would overlap (Word stacks them).
                        if let Some((_, bottom, l, r)) = float_obs
                            && top < bottom
                            && top + h > float_top
                            && left < r
                            && left + w > l
                        {
                            top = bottom + dt;
                        }
                        let mut at = top;
                        for piece in pieces {
                            items.extend(piece.items.into_iter().map(|it| offset(it, left - lo, at)));
                            at += piece.height;
                        }
                        float_obs = Some((top - dt, top + h + db, left - dl, left + w + dr));
                        float_top = top;
                        float_bottom = float_bottom.max(top + h);
                        continue;
                    }
                    let obstacle = float_obs.filter(|(_, bottom, _, _)| *bottom > y + 0.01).map(|(top, bottom, l, r)| WrapObstacleInPara {
                        shape_left: l,
                        shape_right: r,
                        rel_top: top - y,
                        rel_bottom: bottom - y,
                        wrap_text: "bothSides".into(),
                        wrap_polygon: None,
                        shape_to_emit: None,
                                });
                    let pieces = if natural || obstacle.is_some() { self.lay_out_block(block, inner, page_number, obstacle.as_slice(), true) } else { self.pieces(block, inner, page_number, &[], true) };
                    for mut piece in pieces {
                        let own = matches!(block, Block::Paragraph(_));
                        // Between two paragraphs of a cell too, Windows Word takes the larger of the space after one and before the next, not both: 27 pt between lines of 18 pt
                        // with 9 pt before and after, on a grid or off one (its XPS of Meiryo 11 pt cells, `tasks/doc/requests/2026-11-16-54-acc-result.md`).
                        if !self.doc.do_not_use_html_paragraph_auto_spacing && prev_after > 0.0 && piece.space_before > 0.0 {
                            let taken = prev_after.min(piece.space_before);
                            y -= taken;
                            for it in piece.items.iter_mut() {
                                if let Item::Line { pad, .. } = it {
                                    pad.0 = (pad.0 - taken).max(0.0);
                                }
                            }
                        }
                        prev_after = piece.space_after;
                        items.extend(piece.items.into_iter().map(|it| if own { in_cell_shape(offset(it, 0.0, y)) } else { offset(it, 0.0, y) }));
                        y += piece.height;
                    }
                }
                self.current_cell.set(None);
            }
            (items, y.max(float_bottom))
        };
        let mut grid: Vec<Vec<Laid>> = Vec::new();
        for (r_idx, row) in t.rows.iter().enumerate() {
            let mut col = 0;
            let mut cells = Vec::new();
            for cell in &row.cells {
                let span = cell.span.max(1) as usize;
                let (l, r) = (exact[col.min(exact.len() - 1)], exact[(col + span).min(exact.len() - 1)]);
                let inner = (r - l - pt(cell.margins[1]) - pt(cell.margins[3])).max(1.0);
                // Vertical text's lines run the row's height; which is not
                // known yet, so it is laid out at its natural length (left
                // aligned) and laid out again at the rows' height below.
                let vertical = cell.direction.vertical();
                let inner = if vertical { 10_000.0 } else { inner };
                let (items, y) = lay_cell(cell, r_idx, col, inner, vertical);
                // Vertical text: its length is its height in the row.
                let (content, across) = if vertical {
                    let length = items.iter().filter_map(|it| match it {
                        Item::Text { x, offsets, size, .. } => Some(x + offsets.last().copied().unwrap_or(0.0) + size),
                        _ => None,
                    }).fold(0.0f32, f32::max);
                    (length, y)
                } else {
                    (y, 0.0)
                };
                cells.push(Laid { col, span, items, content, across });
                col += span;
            }
            grid.push(cells);
        }
        let spacing = t.cell_spacing.map_or(0.0, pt);
        let half_sp = spacing / 2.0;
        // Row heights: the content of cells that are not merged down, the
        // rule's height; then merged cells stretch the last row they cover.
        // The rule between two rows takes the width of the thickest border at that joint, whichever cell it is of: the bottom borders of the row above and
        // the top borders of the row below, the cells of a merge's continuation too (Word 16 PDFs of a 4-row table of 10.5 pt: the pitch of a row is its
        // content and the joint below it, 41.4 / 43.9 pt for joints of 0.5 / 3 pt, a thick border on a one-line cell of a three-line row included, a thick bottom
        // border of the row above the same as a thick top border of the row below; the first row's text is its table's top border from the top).
        let joint_above: Vec<f32> = (0..t.rows.len())
            .map(|r| {
                let top = t.rows[r].cells.iter().filter_map(|c| c.borders[0]).map(|b| pt(b.width)).fold(0.0, f32::max);
                let above = if r == 0 { 0.0 } else { t.rows[r - 1].cells.iter().filter_map(|c| c.borders[2]).map(|b| pt(b.width)).fold(0.0, f32::max) };
                top.max(above)
            })
            .collect();
        let mut heights: Vec<f32> = t
            .rows
            .iter()
            .zip(&grid)
            .enumerate()
            .map(|(r, (row, cells))| {
                let joint = joint_above[r];
                // Word counts the width of the joint above into its row.
                let content = row
                    .cells
                    .iter()
                    .zip(cells)
                    .filter(|(c, _)| c.merge == VerticalMerge::None)
                    .map(|(c, l)| l.content + pt(c.margins[0]) + pt(c.margins[2]) + joint + spacing)
                    .fold(0.0, f32::max);
                // A minimum height does not count the top border either.
                let border = joint;
                match row.height {
                    // An exact row is its height and the cells' bottom margin; the top margin and the border are not added (Word's XPS, GitHub issue #179: the
                    // rules' middles 20.04, 22.08, 25.08 and 30.12 pt apart for a height of 20 pt and bottom margins of 0, 2, 5 and 10 pt, whatever the top margin).
                    RowHeight::Exact(h) => pt(h) + row.cells.iter().map(|c| pt(c.margins[2])).fold(0.0, f32::max),
                    // An at least row is the larger of its height and the content, and the margins and the border on top of that (20 pt with margins of 5 pt and
                    // a border of 0.5 pt is 30.5 pt; with a content of three lines 40.9 pt and the margins and the border).
                    RowHeight::AtLeast(h) => {
                        let with_margins = row
                            .cells
                            .iter()
                            .zip(cells)
                            .filter(|(c, _)| c.merge == VerticalMerge::None)
                            .map(|(c, l)| pt(h).max(l.content) + pt(c.margins[0]) + pt(c.margins[2]) + joint + spacing)
                            .fold(pt(h) + border + spacing, f32::max);
                        with_margins.max(content)
                    }
                    RowHeight::Auto => content,
                }
            })
            .collect();
        // The last row's bottom border takes room below the table too.
        if let (Some(last), Some(h)) = (t.rows.last(), heights.last_mut()) {
            *h += last.cells.iter().filter_map(|c| c.borders[2]).map(|b| pt(b.width)).fold(0.0, f32::max);
        }
        // How many rows each merged cell covers.
        let spans_down = |grid: &Vec<Vec<Laid>>, r: usize, col: usize| -> usize {
            let mut n = 1;
            while r + n < t.rows.len() {
                let below = grid[r + n].iter().zip(&t.rows[r + n].cells).find(|(l, _)| l.col == col);
                match below {
                    Some((_, c)) if c.merge == VerticalMerge::Continue => n += 1,
                    _ => break,
                }
            }
            n
        };
        for r in 0..t.rows.len() {
            for (laid, cell) in grid[r].iter().zip(&t.rows[r].cells) {
                if cell.merge != VerticalMerge::Start {
                    continue;
                }
                let n = spans_down(&grid, r, laid.col);
                let need = laid.content + pt(cell.margins[0]) + pt(cell.margins[2]);
                let have: f32 = heights[r..r + n].iter().sum();
                // A vertical cell's text wraps in rows of exact heights;
                // in the others the rows grow to hold it.
                let fixed = cell.direction.vertical() && t.rows[r..r + n].iter().all(|row| matches!(row.height, RowHeight::Exact(_)));
                if need > have && !fixed {
                    heights[r + n - 1] += need - have;
                }
            }
        }
        // Vertical text again, as long as the rows it covers are high (so
        // that its alignment and wrapping are to that length).
        let mut again: Vec<(usize, usize, Vec<Item>, f32)> = Vec::new();
        for r in 0..t.rows.len() {
            for (k, (laid, cell)) in grid[r].iter().zip(&t.rows[r].cells).enumerate() {
                if !cell.direction.vertical() || cell.merge == VerticalMerge::Continue {
                    continue;
                }
                let n = if cell.merge == VerticalMerge::Start { spans_down(&grid, r, laid.col) } else { 1 };
                let h: f32 = heights[r..(r + n).min(heights.len())].iter().sum();
                let border = joint_above[r];
                let length = (h - pt(cell.margins[0]) - pt(cell.margins[2]) - border).max(1.0);
                let (items, across) = lay_cell(cell, r, laid.col, length, false);
                again.push((r, k, items, across));
            }
        }
        for (r, k, items, across) in again {
            grid[r][k].items = items;
            grid[r][k].across = across;
        }

        let mut pieces = Vec::new();
        // What of a cell merged down belongs to the rows below its first (by row and column).
        let mut carried: std::collections::HashMap<(usize, usize), Vec<Item>> = std::collections::HashMap::new();
        for (r, row) in t.rows.iter().enumerate() {
            let mut items = Vec::new();
            let mut borders = Vec::new();
            // A row Word may divide at a page's end: not kept whole, not a
            // heading, not of an exact height, without merged cells or
            // diagonals. Whatever the cells' vertical alignment (GitHub issue
            // #178: Word divides a row of centred or bottom-aligned cells too).
            let divisible = !row.cant_split
                && !row.header
                && !matches!(row.height, RowHeight::Exact(_))
                && row.lines.is_empty()
                && row.cells.iter().all(|c| c.merge == VerticalMerge::None && c.diagonals.iter().all(Option::is_none));
            let mut split_cells = Vec::new();
            let mut anchored: Vec<Arc<AnchoredCell>> = Vec::new();
            for (laid, cell) in grid[r].iter().zip(&row.cells) {
                let n = if cell.merge == VerticalMerge::Start { spans_down(&grid, r, laid.col) } else { 1 };
                let h: f32 = heights[r..(r + n).min(heights.len())].iter().sum();
                let raw_left = x0 + xs[laid.col.min(xs.len() - 1)];
                let raw_right = x0 + xs[(laid.col + laid.span).min(xs.len() - 1)];
                let (left, right) = if spacing > 0.0 {
                    (raw_left + half_sp, raw_right - half_sp)
                } else {
                    (raw_left, raw_right)
                };
                let cell_top = if spacing > 0.0 { half_sp } else { 0.0 };
                let cell_h = if spacing > 0.0 { (h - spacing).max(0.0) } else { h };
                let mut cell_items: Vec<Item> = Vec::new();
                if cell.merge != VerticalMerge::Continue {
                    // Its own fill, else its table style's.
                    let fill = cell.shading.or(cell.style_fill);
                    if fill.is_some() || cell.shading_pattern.is_some() {
                        let bg = if cell.shading_pattern.is_some() { cell.shading_fill } else { fill };
                        cell_items.extend(shading_items(cell.shading_pattern, bg, cell.shading_color, left, cell_top, right - left, cell_h));
                    }
                    // The top border's width is taken off the room first.
                    // An art border takes as much room as its `w:sz` in points (a 1.5 pt line, 12 pt of room:
                    // Word's text is 10 pt lower, DTBB).
                    let border = cell.borders[0].map_or(joint_above[r], |b| if b.art.is_some() { pt(b.width) * 8.0 } else { joint_above[r] });
                    let room = cell_h - pt(cell.margins[0]) - pt(cell.margins[2]) - border;
                    let dy = cell_top
                        + border
                        + pt(cell.margins[0])
                        + match cell.anchor {
                            CellAnchor::Top => 0.0,
                            CellAnchor::Center => ((room - laid.content) / 2.0).max(0.0),
                            CellAnchor::Bottom => (room - laid.content).max(0.0),
                        };
                    // The text starts a margin from the middle of the cell's left border (not from the border's inner edge: GitHub issue #178,
                    // Word's PDFs: 5.4 pt from the middle for borders of 3, 1 and 0.5 pt).
                    let dx = left + pt(cell.margins[3]);
                    // A row of an exact height shows only the lines that
                    // begin in it (Word cuts the rest off; DTST's wrapped
                    // cells).
                    let exact = matches!(row.height, RowHeight::Exact(_)) && n == 1;
                    if n > 1 && (cell.anchor != CellAnchor::Top || cell.direction.vertical()) {
                        // Its text is placed where the pages are known (`AnchoredCell`).
                        anchored.push(Arc::new(AnchoredCell {
                            items: laid.items.clone(),
                            content: laid.content,
                            across: laid.across,
                            direction: cell.direction,
                            anchor: cell.anchor,
                            left,
                            right,
                            cell_top,
                            border,
                            side_borders: if cell.direction.vertical() { [cell.borders[3].map_or(0.0, |b| pt(b.total()) / 2.0), cell.borders[1].map_or(0.0, |b| pt(b.total()) / 2.0)] } else { [0.0; 2] },
                            margins: [pt(cell.margins[0]), pt(cell.margins[1]), pt(cell.margins[2]), pt(cell.margins[3])],
                            spacing,
                            rows: heights[r..(r + n).min(heights.len())].to_vec(),
                        }));
                    } else if cell.direction.vertical() {
                        // The cell's box inside its margins.
                        // The lines stand between the borders' middles (the borders are drawn across the cell's edges): Word's PDF of the issue #271 form, a
                        // cell of borders of 1.5 pt left and 3 pt right: its two columns' middle is the middle of 36.72 to 63.62, 1.5 pt in from the cell's right edge.
                        let (border_left, border_right) = (cell.borders[3].map_or(0.0, |b| pt(b.total()) / 2.0), cell.borders[1].map_or(0.0, |b| pt(b.total()) / 2.0));
                        let bx = left + pt(cell.margins[3]) + border_left;
                        let by = cell_top + border + pt(cell.margins[0]);
                        let bw = (right - left - pt(cell.margins[1]) - pt(cell.margins[3]) - border_left - border_right).max(0.0);
                        let bh = (cell_h - pt(cell.margins[0]) - pt(cell.margins[2]) - border).max(0.0);
                        cell_items.extend(turn_cell_text(&laid.items, cell.direction, cell.anchor, (bx, by, bw, bh), laid.across, &|_, _, size| size));
                    } else if cell.direction == ibis_doc_model::CellDirection::LrTbV {
                        cell_items.extend(turn_japanese(laid.items.iter().cloned().map(|it| offset(it, dx, dy))));
                    } else {
                    cell_items.extend(laid.items.iter().cloned().map(|it| offset(it, dx, dy)).filter(|it| {
                        !exact
                            || match it {
                                Item::Text { y, size, .. } => *y - size * 0.8 < h,
                                Item::Line { y, .. } => *y < h,
                                _ => true,
                            }
                    }));
                    if divisible {
                        let dy_top = dy - match cell.anchor {
                            CellAnchor::Top => 0.0,
                            CellAnchor::Center => ((room - laid.content) / 2.0).max(0.0),
                            CellAnchor::Bottom => (room - laid.content).max(0.0),
                        };
                        split_cells.push(SplitCell {
                            left,
                            right,
                            fill: cell.shading.or(cell.style_fill).map(|f| hsla(ibis_doc_model::Color::opaque(f))),
                            content: laid.items.iter().cloned().map(|it| offset(it, dx, dy_top)).collect(),
                            anchor: cell.anchor,
                            top: dy_top,
                            bottom: pt(cell.margins[2]),
                            borders: [None; 4],
                        });
                    }
                    }
                }
                // A cell merged down is laid out over all the rows it covers; each row has the part that falls in it, so that the rows can go on
                // different pages (E3: the lines were all drawn with the first row, past the page's end, and the rows on the next page were empty).
                if n > 1 {
                    let mut at = 0.0f32;
                    let bounds: Vec<(f32, f32)> = (r..r + n).map(|k| { let b = (at, at + heights[k]); at = b.1; b }).collect();
                    for (k, part) in split_over_rows(cell_items, &bounds, true).into_iter().enumerate() {
                        if k == 0 { items.extend(part) } else { carried.insert((r + k, laid.col), part); }
                    }
                } else {
                    items.extend(cell_items);
                }
                if let Some(part) = carried.remove(&(r, laid.col)) {
                    items.extend(part);
                }
                // Borders: a continued cell draws only its sides and bottom.
                let row_h = if spacing > 0.0 { heights[r] - half_sp } else { heights[r] };
                let row_top = if spacing > 0.0 { half_sp } else { 0.0 };
                let [top, rgt, bottom, lft] = cell.borders;
                // Where two cells' borders meet, the heavier one is drawn
                // (Word's rule: the width in eighths of a point times the
                // style's number; the one above or left on a tie).
                let (top, bottom, lft, rgt) = if spacing > 0.0 {
                    (top, bottom, lft, rgt)
                } else {
                    let neighbour = |row: usize, col: usize| grid.get(row).and_then(|cells| cells.iter().position(|l| l.col == col)).map(|i| t.rows[row].cells[i].borders);
                    let beats = |a: Option<FlowBorder>, b: Option<FlowBorder>| weight(a) > weight(b);
                    let top = if r > 0 && neighbour(r - 1, laid.col).is_some_and(|n| !beats(top, n[2])) { None } else { top };
                    let bottom = if neighbour(r + 1, laid.col).is_some_and(|n| beats(n[0], bottom)) { None } else { bottom };
                    let lft = if laid.col > 0 && cells_left_of(&grid[r], laid.col).is_some_and(|i| !beats(lft, t.rows[r].cells[i].borders[1])) { None } else { lft };
                    let rgt = if grid[r].iter().position(|l| l.col == laid.col + laid.span).is_some_and(|i| beats(t.rows[r].cells[i].borders[3], rgt)) { None } else { rgt };
                    (top, bottom, lft, rgt)
                };
                let is_table_top = r == 0;
                let is_table_bottom = r + 1 == t.rows.len();
                let is_table_left = laid.col == 0;
                let is_table_right = laid.col + laid.span == t.columns.len();
                if cell.merge != VerticalMerge::Continue {
                    // The table's top border starts at its top edge and goes down into the row (Word counts it in the
                    // row and does not draw half of it above the table: DTBB).
                    let top_at = if is_table_top { row_top + top.map_or(0.0, |b| pt(b.total()) / 2.0) } else { row_top };
                    borders.push((top, true, true, top_at, left, right, is_table_top && is_table_left, is_table_top && is_table_right));
                    // Diagonal lines across the cell (as high as it spans).
                    let n = if cell.merge == VerticalMerge::Start { spans_down(&grid, r, laid.col) } else { 1 };
                    let h: f32 = heights[r..(r + n).min(heights.len())].iter().sum();
                    let diag_bottom = if spacing > 0.0 { h - half_sp } else { h };
                    for (i, d) in cell.diagonals.iter().enumerate() {
                        if let Some(d) = d {
                            let color = hsla(ibis_doc_model::Color::opaque(d.color));
                            let (x, x2) = if i == 0 { (left, right) } else { (right, left) };
                            items.push(Item::Stroke { x, y: row_top, x2, y2: diag_bottom, w: pt(d.width).max(0.25), color });
                        }
                    }
                }
                if divisible && let Some(c) = split_cells.last_mut() {
                    c.borders = [top, rgt, bottom, lft];
                }
                if cell.merge == VerticalMerge::None || cell.merge == VerticalMerge::Continue && !continues_below(t, &grid, r, laid.col) {
                    // At the table's bottom edge the border ends where the row does (it is inside the room the row
                    // reserves for it), as the top one starts at the top edge.
                    let bottom_at = if is_table_bottom { row_h - bottom.map_or(0.0, |b| pt(b.total()) / 2.0) } else { row_h };
                    borders.push((bottom, true, false, bottom_at, left, right, is_table_bottom && is_table_left, is_table_bottom && is_table_right));
                }
                // The sides meet the top border where it starts (at the table's top edge, see above).
                let side_top = if is_table_top { row_top + top.map_or(0.0, |b| pt(b.total()) / 2.0) } else { row_top };
                let side_bottom = if is_table_bottom { row_h - bottom.map_or(0.0, |b| pt(b.total()) / 2.0) } else { row_h };
                borders.push((lft, false, true, left, side_top, side_bottom, is_table_left && is_table_top, is_table_left && is_table_bottom));
                borders.push((rgt, false, false, right, side_top, side_bottom, is_table_right && is_table_top, is_table_right && is_table_bottom));
            }
            for (b, horizontal, outer_first, at, from, to, extend_start, extend_end) in borders {
                if let Some(b) = b {
                    items.extend(border_items(&b, horizontal, outer_first, at, from, to, extend_start, extend_end, true));
                }
            }
            // Extra display-line fragments use normalized row coordinates.
            // They travel with the unsplittable row and are rotated with a
            // vertical page by the same transform as all other strokes.
            for line in &row.lines {
                let ((x1, y1), (x2, y2)) = line.points(total, heights[r]);
                if x1 == x2 || y1 == y2 {
                    let horizontal = y1 == y2;
                    let (at, from, to) = if horizontal { (y1, x0 + x1.min(x2), x0 + x1.max(x2)) }
                        else { (x0 + x1, y1.min(y2), y1.max(y2)) };
                    // Partial rules have the same line-type semantics as
                    // cell borders, including dashed and compound strokes.
                    items.extend(border_items(&line.border, horizontal, true, at, from, to, false, false, true));
                    continue;
                }
                items.push(Item::Stroke {
                    x: x0 + x1, y: y1, x2: x0 + x2, y2,
                    w: pt(line.border.width).max(0.25),
                    color: hsla(ibis_doc_model::Color::opaque(line.border.color)),
                });
            }
            let min_height = if let RowHeight::AtLeast(h) = row.height { pt(h) + joint_above[r] + spacing } else { 0.0 };
            let split = (divisible && split_cells.len() == row.cells.len()).then(|| Arc::new(RowSplit { cells: split_cells, min_height }));
            pieces.push(Piece { height: heights[r], items, page_break: false, column_break: false, keep_with_next: false, header: row.header && t.rows[..r].iter().all(|r| r.header), split, table_bounds: Some((x0, x0 + total)), anchored, ..Default::default() });
        }
        pieces
    }

    /// A paragraph's space before and after, less what contextual spacing
    /// takes away.
    fn space_before(&self, p: &FlowParagraph) -> f32 {
        if self.contextual.get(&p.id).is_some_and(|c| c.0) { 0.0 } else { self.spacing_length(p.space_before, p.space_before_lines) }
    }

    fn space_after(&self, p: &FlowParagraph) -> f32 {
        if self.contextual.get(&p.id).is_some_and(|c| c.1) { 0.0 } else { self.spacing_length(p.space_after, p.space_after_lines) }
    }

    /// A space before or after: the length, or, when the paragraph gives it in lines (`w:beforeLines`, `w:afterLines`; they win over the
    /// length, ECMA-376 17.3.1.33), the lines times the unit of a line. The unit is the line grid's pitch when the section has one (a
    /// paragraph that does not snap to the grid still spaces by it); without a grid it is 12 pt, which is what Word's PDF of the
    /// issue #191 file shows (`beforeLines` 100 and 200 are 12 and 24 pt in text of 10.5 pt with exact lines of 14 pt; the font size and
    /// the line height do not enter). How it depends on the document's default font is not measured (tasks/doc/requests/2026-11-14-191-word.md).
    fn spacing_length(&self, length: Emu, lines: Option<i32>) -> f32 {
        match lines {
            Some(n) => {
                let unit = self.line_pitch.get().filter(|p| *p > 0).map_or(12.0, pt);
                n as f32 / 100.0 * unit
            }
            None => pt(length),
        }
    }

    fn lay_out(&self) -> Pagination {
        use ibis_doc_model::flow_command;
        // The sections: their setup and what their headers and footers show.
        let ids = flow_command::sections(self.doc);
        let mut setups: Vec<ibis_doc_model::SectionSetup> = ids.iter().map(|id| flow_command::section_setup(self.doc, *id).unwrap_or_default()).collect();
        // 縦書き: a section of vertical writing is laid out as across text on
        // a page turned a quarter (see `turn_page_items`).
        let directions: Vec<ibis_doc_model::CellDirection> = setups.iter().map(|s| s.page.direction).collect();
        // The paper's left margin, and the right margin with the gutter (which a vertical section has on the right), before they are turned with the page: the
        // running heads and feet of a vertical section stay across and are laid out in the paper (see below).
        let paper_margins: Vec<(f32, f32)> = setups.iter().map(|s| (pt(s.page.left), pt(s.page.right + s.page.gutter))).collect();
        for setup in &mut setups {
            use ibis_doc_model::CellDirection;
            let page = &mut setup.page;
            let (t, r, b, l) = (page.top, page.right, page.bottom, page.left);
            match page.direction {
                // The gutter of a vertical page is on the right (the top of the page turned), or on the top of the paper (the left of the page turned) with `gutter_at_top`
                // (Word, Windows, 2026-11-25, `tasks/doc/requests/2026-11-24-287-gridlines-word-result.md`: the body's right edge is the right margin and the gutter).
                CellDirection::TbRl => {
                    let (to_right, to_top) = if page.gutter_at_top { (0, page.gutter) } else { (page.gutter, 0) };
                    (page.top, page.right, page.bottom, page.left) = (r + to_right, b, l, t + to_top)
                }
                CellDirection::BtLr => (page.top, page.right, page.bottom, page.left) = (l, t, r, b),
                _ => continue,
            }
            page.size = ibis_doc_model::EmuSize { width: page.size.height, height: page.size.width };
            page.borders = [None; 4];
            page.gutter = 0;
        }
        let parts: Vec<[Vec<Block>; 6]> = ids.iter().map(|id| std::array::from_fn(|k| flow_command::resolved_part(self.doc, *id, ibis_doc_model::PartKind::ALL[k]))).collect();
        // A header taller than the top margin (or a footer than the bottom
        // one) pushes the body down (up), as Word does: the body starts at
        // the larger of the margin and where the header ends (GitHub issue
        // #10). A negative margin is exact and stays. The header meant is the
        // ones the section's pages show (the default, the even pages' with 奇数/偶数, the first page's).
        // How far lower (or higher) the body of the first page of a section with its own first page header starts than the other pages' does.
        let mut first_page_drop = vec![0.0f32; setups.len()];
        // How much more room above the footer the body of a page has than the section's most demanding footer leaves: the pages of a section do not all
        // have the same footer (the default one, the even pages' with 奇数/偶数, the first page's), and a page has the room its own footer leaves (Word's PDF of 023:
        // the even pages' footer has its page number in a frame, so it is one empty line high where the odd pages' is two, and the body of an even page ends 12.6 pt
        // lower). By page kind: 0 default (odd), 1 even, 2 the first page of a section with its own first page footer.
        let mut bottom_raise = vec![[0.0f32; 3]; setups.len()];
        for (s, setup) in setups.iter_mut().enumerate() {
            // A vertical section's running heads and feet stay across at the paper's top and bottom (laid out in the paper, `fixed_items` below): they do not
            // move the margins the turned page has (the paper's right and left, which are the across page's top and bottom).
            if directions[s].vertical() {
                continue;
            }
            let page = &mut setup.page;
            let (gl, _) = if page.gutter_at_top { (0, page.gutter) } else { (page.gutter, 0) };
            let width = pt(page.size.width) - pt(page.left + gl) - pt(page.right);
            if width <= 0.0 {
                continue;
            }
            // Every paragraph of a footer that is not framed takes its height off the body, an empty one too, also the trailing ones and a footer of
            // empty paragraphs only (Word's PDFs of `footer-trailing-empty-probe.docx` and `footer-empty-134-variants-probe.docx`: with a text paragraph
            // and an empty one the body is one line shorter than with the text alone, and an empty footer takes a line, whether its distance from the
            // edge is more than the bottom margin or less (the footer then does not reach the margin and leaves the body alone)). A header's is the same: 024.
            // (The empty line of corpus 134 that was taken for the body's last line, 789 pt down, is the footer's own.)
            let is_flow_content = |b: &Block| !matches!(b, Block::Paragraph(p) if p.framed);
            let measure = |blocks: &[Block]| -> f32 { blocks.iter().flat_map(|b| self.header_footer_pieces(b, width, 1)).map(|p| p.height).sum() };
            let measure_footer = |blocks: &[Block]| -> f32 {
                let end = blocks.iter().rposition(is_flow_content).map_or(blocks.len(), |i| i + 1);
                measure(&blocks[..end])
            };
            let mut kinds = vec![(0usize, 1usize)];
            if self.doc.even_and_odd {
                kinds.push((4, 5));
            }
            // The pages after the first have the default (and the even pages') header; the first page of a section with
            // 先頭ページのみ別指定 has its own, so its body starts where that header ends (Word's PDF of 023: a section whose first page header has two
            // lines and the default header one has the body of its later pages at the margin, 42.5 pt, not 64.3 pt).
            let first_kinds = if setup.title_page { Some((2usize, 3usize)) } else { None };
            let (mut header_end, mut footer_start, mut first_header_end, mut first_footer_start) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for (h, f) in kinds {
                if !parts[s][h].is_empty() {
                    header_end = header_end.max(pt(page.header) + measure(&parts[s][h]));
                }
                if !parts[s][f].is_empty() {
                    footer_start = footer_start.max(pt(page.footer) + measure_footer(&parts[s][f]));
                }
            }
            if let Some((h, f)) = first_kinds {
                if !parts[s][h].is_empty() {
                    first_header_end = pt(page.header) + measure(&parts[s][h]);
                }
                if !parts[s][f].is_empty() {
                    first_footer_start = pt(page.footer) + measure_footer(&parts[s][f]);
                }
            }
            let emu = |v: f32| (v / pt(1)).round() as i64;
            let margin_top = pt(page.top);
            let margin_bottom_before = pt(page.bottom);
            if page.top > 0 && header_end > pt(page.top) {
                page.top = emu(header_end);
            }
            if first_kinds.is_some() && margin_top > 0.0 {
                first_page_drop[s] = margin_top.max(first_header_end) - pt(page.top);
            }
            // The footer of the first page: the bottom margin of the section is the larger of both as before.
            footer_start = footer_start.max(first_footer_start);
            if page.bottom > 0 && footer_start > pt(page.bottom) {
                page.bottom = emu(footer_start);
            }
            if page.bottom > 0 {
                let margin_bottom = margin_bottom_before;
                let section_bottom = pt(page.bottom);
                let room = |part: &[Block], present: bool| -> f32 { if present && !part.is_empty() { pt(page.footer) + measure_footer(part) } else { 0.0 } };
                let default_start = room(&parts[s][1], true);
                let even_start = if self.doc.even_and_odd { room(&parts[s][5], true) } else { default_start };
                let first_start = if first_kinds.is_some() { room(&parts[s][3], true) } else { default_start };
                for (kind, start) in [default_start, even_start, first_start].into_iter().enumerate() {
                    bottom_raise[s][kind] = (section_bottom - margin_bottom.max(start)).max(0.0);
                }
            }
        }
        // The text area of a section's pages, and its columns (段組み).
        // とじしろ (gutter): on the left (the inside with 見開きページ), or
        // at the top.
        let gutters = |s: usize| -> (i64, i64) {
            let page = &setups[s].page;
            if page.gutter_at_top { (0, page.gutter) } else { (page.gutter, 0) }
        };
        let text_frame = |s: usize| {
            let page = &setups[s].page;
            let (w, h) = (pt(page.size.width), pt(page.size.height));
            let (gl, gt) = gutters(s);
            Frame { left: pt(page.left + gl), top: pt(page.top + gt), width: w - pt(page.left + gl) - pt(page.right), bottom: h - pt(page.bottom) }
        };
        let columns_of = |s: usize| -> Vec<(f32, f32)> {
            let page = &setups[s].page;
            page.columns.layout(page.size.width - page.left - gutters(s).0 - page.right).into_iter().map(|(x, w)| (pt(x), pt(w))).collect()
        };
        // 見開きページ・袋とじ: an even page's margins swapped (the inside on the
        // right): how far its text moves.
        let mirror_dx = |s: usize, number: u32| -> f32 {
            let page = &setups[s].page;
            if page.is_mirrored() && number % 2 == 0 { pt(page.right) - pt(page.left + gutters(s).0) } else { 0.0 }
        };
        let column_frame = |s: usize, c: usize| {
            let text = text_frame(s);
            let cols = columns_of(s);
            let (x, w) = cols.get(c).copied().unwrap_or((0.0, text.width));
            Frame { left: text.left + x, width: w, ..text }
        };
        let frame_of = |s: usize| column_frame(s, 0);
        // The frame of a page of section `s`: the first page of a section with its own first page header starts at that header's end.
        let raise_of = |s: usize, first: bool, number: u32| -> f32 {
            let kind = if first && setups[s].title_page { 2 } else if self.doc.even_and_odd && number % 2 == 0 { 1 } else { 0 };
            bottom_raise[s][kind]
        };
        let frame_for = |s: usize, first: bool, number: u32| {
            let mut f = frame_of(s);
            if first {
                f.top += first_page_drop[s];
            }
            f.bottom += raise_of(s, first, number);
            f
        };
        let page_of = |s: usize, number: u32, first: bool| Page {
            items: Vec::new(),
            body_items_count: 0,
            width: pt(setups[s].page.size.width),
            height: pt(setups[s].page.size.height),
            section: s,
            number,
            first_in_section: first,
            notes: Vec::new(),
            carry_in: Vec::new(),
            split_take: None,
            text_area: {
                let (f, dx) = (frame_for(s, first, number), mirror_dx(s, number));
                [f.left + dx, f.top, f.left + dx + f.width, text_frame(s).bottom]
            },
        };
        // The section each body block is in.
        let mut section = 0usize;
        let mut sections_of = Vec::with_capacity(self.doc.body.len());
        for block in &self.doc.body {
            sections_of.push(section);
            if matches!(block, Block::Paragraph(p) if p.section.is_some()) {
                section = (section + 1).min(setups.len() - 1);
            }
        }
        // The footnotes' marks, by paragraph (their positions), and the
        // notes laid out (each as wide as the text of the first section).
        let mut marks: std::collections::HashMap<u32, Vec<(usize, u32)>> = std::collections::HashMap::new();
        fn collect(blocks: &[Block], marks: &mut std::collections::HashMap<u32, Vec<(usize, u32)>>) {
            for b in blocks {
                match b {
                    Block::Paragraph(p) => {
                        let mut at = 0;
                        for r in &p.runs {
                            if let FlowRunKind::NoteRef { endnote: false, id } = r.kind {
                                marks.entry(p.id).or_default().push((at, id));
                            }
                            at += r.len();
                        }
                    }
                    Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| collect(&c.blocks, marks)),
                }
            }
        }
        collect(&self.doc.body, &mut marks);
        // The section each footnote's mark is in: its text is set as that section's is (across or down the page, its grid, its width).
        let mut note_section: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        fn note_sections(blocks: &[Block], s: usize, out: &mut std::collections::HashMap<u32, usize>) {
            for b in blocks {
                match b {
                    Block::Paragraph(p) => {
                        for r in &p.runs {
                            if let FlowRunKind::NoteRef { endnote: false, id } = r.kind {
                                out.entry(id).or_insert(s);
                            }
                        }
                    }
                    Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| note_sections(&c.blocks, s, out)),
                }
            }
        }
        for (b, block) in self.doc.body.iter().enumerate() {
            note_sections(std::slice::from_ref(block), sections_of[b], &mut note_section);
        }
        let note_pieces = |endnote: bool, id: u32| -> Vec<Piece> {
            let notes = if endnote { &self.doc.endnotes } else { &self.doc.footnotes };
            let Some(note) = notes.iter().find(|n| n.id == id) else { return Vec::new() };
            *self.current_note.borrow_mut() = self.note_numbers.get(&(endnote, id)).cloned().unwrap_or_default();
            // A footnote is set as the section it is in is: the width of its columns, and, in a section that runs down the page, the vertical rules of its characters
            // (`self.vertical` is the section's of the body only while the body is laid out, so it is set here for the notes laid out before it and after it).
            let at = if endnote { 0 } else { note_section.get(&id).copied().unwrap_or(0) };
            if !endnote {
                self.line_pitch.set(setups[at].page.line_pitch);
                self.char_space.set(char_space_of(&setups[at].page));
                self.char_grid.set(char_grid_of(&setups[at].page));
                self.snap_chars.set(setups[at].page.doc_grid_type == ibis_doc_model::DocGridType::SnapToChars);
                self.vertical.set(setups[at].page.direction.vertical());
            }
            let note_width = frame_of(at).width;
            // (Not as a table's cell: a note's lines go on the line grid, 18 pt apart in Word's PDFs of `vertical-footnote` `v-p-grid` and `h-p-grid`.)
            note.blocks.iter().flat_map(|b| self.pieces(b, note_width, 1, &[], false)).collect()
        };
        let note_width = frame_of(0).width;
        let mut note_heights: std::collections::HashMap<u32, f32> = std::collections::HashMap::new();
        // Each footnote's pieces (its lines), laid out once: a note longer than the room the page has is cut between them.
        let mut note_cache: std::collections::HashMap<u32, Vec<Piece>> = std::collections::HashMap::new();
        for ids in marks.values() {
            for (_, id) in ids {
                if !note_cache.contains_key(id) {
                    let v = note_pieces(false, *id);
                    note_heights.insert(*id, v.iter().map(|p| p.height).sum());
                    note_cache.insert(*id, v);
                }
            }
        }
        // The separator above the notes: a line of the normal style.
        let separator_h = {
            let mut p = FlowParagraph { direct: ibis_doc_model::ParaFormat { before: Some(0), after: Some(0), line: Some(ibis_doc_model::LineHeight::Auto(240)), ..Default::default() }, ..FlowParagraph::default() };
            p.resolve(&self.doc.styles);
            self.pieces(&Block::Paragraph(p), note_width, 1, &[], true).iter().map(|p| p.height).sum::<f32>()
        };
        // The notes a piece has marks of.
        let notes_of = |piece: &Piece| -> Vec<u32> {
            let mut out = Vec::new();
            for it in &piece.items {
                if let Item::Line { para, stops, .. } = it
                    && let Some(list) = marks.get(para)
                {
                    let (from, to) = (stops.first().map_or(0, |s| s.0), stops.last().map_or(0, |s| s.0));
                    for (at, id) in list {
                        if *at >= from && (*at < to || to == from) && !out.contains(id) {
                            out.push(*id);
                        }
                    }
                }
            }
            out
        };
        // The pieces the notes of a page (and `extra`, the ones a line about to be placed adds) have, carried ones first: (the note, the piece, its height).
        let note_entries = |page: &Page, extra: &[u32]| -> Vec<(u32, usize, f32)> {
            let mut v = Vec::new();
            for (id, from) in &page.carry_in {
                if let Some(ps) = note_cache.get(id) {
                    v.extend(ps.iter().enumerate().skip(*from).map(|(i, p)| (*id, i, p.height)));
                }
            }
            for id in page.notes.iter().chain(extra.iter().filter(|e| !page.notes.contains(e))) {
                if page.carry_in.iter().any(|(c, _)| c == id) {
                    continue;
                }
                if let Some(ps) = note_cache.get(id) {
                    v.extend(ps.iter().enumerate().map(|(i, p)| (*id, i, p.height)));
                }
            }
            v
        };
        // How much of the room a page's notes take, down the page, when they run in `n` columns as the body's do (Word sets them in the section's columns; the
        // columns are as even as the lines allow: the one that holds the most is as short as it can be).
        let thickness = |heights: &[f32], n: usize| -> f32 {
            if heights.is_empty() {
                return 0.0;
            }
            let total: f32 = heights.iter().sum();
            if n <= 1 {
                return total;
            }
            let fill = |limit: f32| -> (usize, f32) {
                let (mut cols, mut used, mut tallest) = (1usize, 0.0f32, 0.0f32);
                for h in heights {
                    if used + h > limit + 0.001 && used > 0.0 {
                        cols += 1;
                        used = 0.0;
                    }
                    used += h;
                    tallest = tallest.max(used);
                }
                (cols, tallest)
            };
            let (mut lo, mut hi) = (heights.iter().copied().fold(0.0f32, f32::max), total);
            for _ in 0..40 {
                let mid = (lo + hi) / 2.0;
                if fill(mid).0 <= n { hi = mid } else { lo = mid }
            }
            fill(hi).1
        };
        // How many of the pieces fit `cap` (the room the page leaves for them under the separator) in `n` columns each that high; at least one.
        let note_fit = |heights: &[f32], cap: f32, n: usize| -> usize {
            let (mut col, mut used, mut k) = (0usize, 0.0f32, 0usize);
            for h in heights {
                if used + h > cap + 0.01 {
                    if col + 1 < n && used > 0.0 {
                        col += 1;
                        used = 0.0;
                    } else {
                        break;
                    }
                }
                used += h;
                k += 1;
            }
            k.max(1).min(heights.len())
        };
        let notes_room = |page: &Page| -> f32 {
            let entries = note_entries(page, &[]);
            let entries = &entries[..page.split_take.map_or(entries.len(), |k| k.min(entries.len()))];
            if entries.is_empty() { 0.0 } else { separator_h + thickness(&entries.iter().map(|e| e.2).collect::<Vec<_>>(), columns_of(page.section).len()) }
        };
        // What goes on to the next page: the notes' pieces after the first `k` of `entries`, as (note, first piece).
        let note_rest = |entries: &[(u32, usize, f32)], k: usize| -> Vec<(u32, usize)> {
            let mut rest: Vec<(u32, usize)> = Vec::new();
            for (id, i, _) in &entries[k.min(entries.len())..] {
                if !rest.iter().any(|(r, _)| r == id) {
                    rest.push((*id, *i));
                }
            }
            rest
        };
        let pending_carry: std::cell::RefCell<Vec<(u32, usize)>> = std::cell::RefCell::new(Vec::new());
        let mut pages: Vec<Page> = vec![page_of(0, setups[0].page.first_page, true)];
        let mut frame = frame_for(0, true, setups[0].page.first_page);
        let mut y = frame.top;
        // Where the last piece placed ended (pages so far, column, y) and the
        // space after it that is in it.
        let mut prev_end: Option<(usize, usize, f32, f32)> = None;
        let mut number = setups[0].page.first_page;
        // A new page, in section `s`: numbered on, or again from the
        // section's start when it is the section's first page and restarts.
        // The section that began in the middle of a page (continuous): its
        // first page is that one, so no page it makes later is its first.
        let mid_started: std::cell::Cell<Option<usize>> = std::cell::Cell::new(None);
        let next_page = |pages: &mut Vec<Page>, frame: &mut Frame, y: &mut f32, number: &mut u32, s: usize| {
            let first = pages.last().is_some_and(|p| p.section != s) && mid_started.get() != Some(s);
            *number = if first && setups[s].restart_numbering { setups[s].page.first_page } else { *number + 1 };
            pages.push(page_of(s, *number, first));
            *frame = frame_for(s, first, *number);
            *y = frame.top;
            // The rest of a note the page before could not hold goes first on this one; if it is more than this page holds, the page takes what it can.
            let carry = std::mem::take(&mut *pending_carry.borrow_mut());
            if !carry.is_empty() {
                let page = pages.last_mut().expect("a page");
                page.carry_in = carry;
                let entries = note_entries(page, &[]);
                let heights: Vec<f32> = entries.iter().map(|e| e.2).collect();
                let cap = (frame.bottom - frame.top - separator_h).max(0.0);
                if separator_h + thickness(&heights, columns_of(s).len()) > frame.bottom - frame.top {
                    let k = note_fit(&heights, cap, columns_of(s).len());
                    page.split_take = Some(k);
                    *pending_carry.borrow_mut() = note_rest(&entries, k);
                }
            }
        };
        // What the shapes set from the page, whose paragraphs are on page `page_index`, leave on it from its top (so that the text before the paragraph
        // goes round them too). None until a first pass has found which page each paragraph is on.
        let find_page_obstacle = |s: usize, number: u32, frame: &Frame, page_index: usize| -> Vec<WrapObstacle> {
            let Some(anchor_pages) = &self.anchor_pages else { return Vec::new() };
            let geometry = FloatGeometry::new(&setups[s].page, frame.width, number);
            let mut found = Vec::new();
            for (b_idx, b) in self.doc.body.iter().enumerate() {
                if sections_of[b_idx] != s || anchor_pages.get(&b_idx) != Some(&page_index) {
                    continue;
                }
                if let Block::Paragraph(p) = b {
                    for shape in square_shapes(p).filter(|shape| is_page_shape(shape)) {
                        found.push(self.float_box(p, number, shape, &geometry, geometry.margin_t, 0.0).obstacle());
                    }
                }
            }
            found
        };
        let place_item = |it: Item, y_cursor: f32, s_idx: usize, pg_num: u32, frame_left: f32| -> Item {
            if let Item::Shape { shape, x, y: _, w, h } = &it
                && let Some(f) = &shape.floating
                && matches!(f.relative_v.as_str(), "page" | "margin" | "topMargin" | "bottomMargin" | "insideMargin" | "outsideMargin")
            {
                let abs_y = float_y(f, *h, &FloatGeometry::new(&setups[s_idx].page, 0.0, pg_num), pt(setups[s_idx].page.top), 0.0);
                Item::Shape {
                    shape: shape.clone(),
                    x: *x + frame_left,
                    y: abs_y,
                    w: *w,
                    h: *h,
                }
            } else {
                offset(it, frame_left, y_cursor)
            }
        };
        let mut active_obstacle: Vec<WrapObstacle> = find_page_obstacle(0, number, &frame, 0);
        // The last floating table: its page, edges and top and bottom.
        let mut last_float: Option<(usize, f32, f32, f32, f32)> = None;
        // The columns being filled on the page (段組み): the section's, the
        // column, where they start, how far each is filled, and the pieces
        // put in them (their items on the page, top, height and column; to
        // balance them where a section ends on the page).
        let mut region_s = 0usize;
        let mut col = 0usize;
        let mut col_top = frame.top;
        let mut col_fill: Vec<f32> = Vec::new();
        let mut placed: Vec<(std::ops::Range<usize>, f32, f32, usize)> = Vec::new();
        // The lines between the columns filled (w:sep).
        macro_rules! close_region {
            () => {{
                if setups[region_s].page.columns.separator {
                    let cols = columns_of(region_s);
                    let left = text_frame(region_s).left;
                    let bottom = col_fill.iter().copied().fold(col_top, f32::max);
                    let page = pages.last_mut().expect("a page");
                    for c in 1..cols.len() {
                        if col_fill.get(c).is_some_and(|&f| f > col_top + 0.01) {
                            let x = left + (cols[c - 1].0 + cols[c - 1].1 + cols[c].0) / 2.0;
                            page.items.push(Item::Rect { x: x - 0.25, y: col_top, w: 0.5, h: bottom - col_top, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::BLACK)) });
                        }
                    }
                }
            }};
        }
        macro_rules! new_page {
            ($s:expr) => {{
                close_region!();
                next_page(&mut pages, &mut frame, &mut y, &mut number, $s);
                region_s = $s;
                // Not read again where the page opened is the last one (the macro is expanded there too).
                #[allow(unused_assignments)]
                {
                    col = 0;
                }
                col_top = y;
                col_fill.clear();
                placed.clear();
            }};
        }
        // On in the next column, or the next page after the last.
        macro_rules! advance {
            ($s:expr) => {{
                if region_s == $s && col + 1 < columns_of($s).len() {
                    col += 1;
                    frame = column_frame($s, col);
                    if let Some(pg) = pages.last() {
                        frame.bottom += raise_of(pg.section, pg.first_in_section, pg.number);
                    }
                    y = col_top;
                } else {
                    new_page!($s);
                }
            }};
        }
        // Pieces are laid out with the number of the page they start on.
        for (b, block) in self.doc.body.iter().enumerate() {
            let s = sections_of[b];
            self.line_pitch.set(setups[s].page.line_pitch);
            self.char_space.set(char_space_of(&setups[s].page));
            self.char_grid.set(char_grid_of(&setups[s].page));
            self.snap_chars.set(setups[s].page.doc_grid_type == ibis_doc_model::DocGridType::SnapToChars);
            self.vertical.set(setups[s].page.direction.vertical());
            // A section that starts a page (after a page-breaking one) takes
            // over the page it starts on.
            if pages.last().is_some_and(|pg| pg.section != s && pg.items.iter().all(|it| matches!(it, Item::Line { .. }))) && y <= frame.top + 0.01 {
                let first = true;
                number = if setups[s].restart_numbering { setups[s].page.first_page } else { number };
                *pages.last_mut().expect("a page") = page_of(s, number, first);
                frame = frame_for(s, first, number);
                y = frame.top;
                active_obstacle = find_page_obstacle(s, number, &frame, pages.len() - 1);
                (region_s, col, col_top) = (s, 0, y);
                col_fill.clear();
                placed.clear();
            }
            // A section that starts on the page (continuous): the columns
            // before are balanced (as Word does where a section ends), and
            // its own start below them.
            if s != region_s {
                mid_started.set(Some(s));
                let cols = columns_of(region_s);
                if cols.len() > 1 && !placed.is_empty() {
                    let heights: Vec<f32> = placed.iter().map(|p| p.2).collect();
                    let total: f32 = heights.iter().sum();
                    let fits = |limit: f32| {
                        let (mut c, mut h) = (0usize, 0.0f32);
                        for &x in &heights {
                            if h + x > limit + 0.01 && h > 0.0 {
                                c += 1;
                                h = 0.0;
                            }
                            h += x;
                        }
                        c < cols.len()
                    };
                    let (mut lo, mut hi) = (heights.iter().copied().fold(0.0, f32::max), total);
                    for _ in 0..40 {
                        let mid = (lo + hi) / 2.0;
                        if fits(mid) { hi = mid } else { lo = mid }
                    }
                    let page = pages.last_mut().expect("a page");
                    let (mut c, mut h) = (0usize, 0.0f32);
                    col_fill = vec![col_top; cols.len()];
                    for (range, top, height, old) in placed.drain(..) {
                        if h + height > hi + 0.01 && h > 0.0 && c + 1 < cols.len() {
                            c += 1;
                            h = 0.0;
                        }
                        let (dx, dy) = (cols[c].0 - cols[old].0, col_top + h - top);
                        for it in &mut page.items[range] {
                            *it = offset(it.clone(), dx, dy);
                        }
                        h += height;
                        col_fill[c] = col_top + h;
                    }
                }
                close_region!();
                if cols.len() > 1 {
                    y = col_fill.iter().copied().fold(col_top, f32::max);
                }
                region_s = s;
                col = 0;
                col_top = y;
                col_fill.clear();
                placed.clear();
                frame = Frame { top: frame.top, ..column_frame(s, 0) };
                if let Some(pg) = pages.last() {
                    frame.bottom += raise_of(s, pg.first_in_section, pg.number);
                }
            }
            *self.page_format.borrow_mut() = setups[s].page_number_format.clone();
            self.body_section.set(s);
            if let Block::Paragraph(p) = block
                && p.page_break_before
                && !pages.last().is_some_and(|pg| pg.items.iter().all(|it| matches!(it, Item::Line { .. })))
            {
                new_page!(s);
                active_obstacle = find_page_obstacle(s, number, &frame, pages.len() - 1);
            }
            let obstacles_for_block: Vec<WrapObstacleInPara> = if let Block::Paragraph(p) = block {
                let pad = |b: Option<FlowBorder>| b.map_or(0.0, |b| pt(b.space) + pt(b.total()));
                let before_pad = self.space_before(p) + pad(p.borders[0]);
                let shapes: Vec<&ibis_doc_model::Shape> = square_shapes(p).collect();
                if shapes.is_empty() {
                    // What an earlier paragraph's shape or table leaves on the page, as far as it reaches this one.
                    let para_top = y + before_pad;
                    active_obstacle.retain(|obs| obs.y_bottom - para_top > 0.01);
                    active_obstacle.iter().map(|obs| obs.in_paragraph(para_top)).collect()
                } else {
                    let mark_size = p.mark.size as f32 / 100.0;
                    let mark_metrics = {
                        let name = p.mark.font_for('A');
                        let font = self.font(&p.mark, name);
                        let (ascent, line) = self.line_metrics(name, &font, mark_size);
                        (line, ascent, self.lead_in(p, name) * mark_size)
                    };
                    let line_h = self.resolve_line_height(p, mark_metrics.0, mark_metrics.2, false);
                    let mut para_top = y + before_pad;
                    let mut boxes: Vec<FloatBox> = shapes.iter().map(|shape| self.float_box(p, number, shape, &FloatGeometry::new(&setups[s].page, frame.width, number), para_top, line_h)).collect();
                    // A box set from its paragraph that does not fit below it on the page goes on to the next page with it (Word does
                    // not cut it off at the page's end).
                    let does_not_fit = boxes.iter().any(|bx| {
                        bx.shape.floating.as_ref().is_some_and(|f| matches!(f.relative_v.as_str(), "paragraph" | "line"))
                            && bx.abs_y + bx.sh > frame.bottom
                            && y > col_top + 0.01
                            && bx.sh < frame.bottom - frame.top
                    });
                    if does_not_fit {
                        active_obstacle.clear();
                        advance!(s);
                        para_top = y + before_pad;
                        boxes = shapes.iter().map(|shape| self.float_box(p, number, shape, &FloatGeometry::new(&setups[s].page, frame.width, number), para_top, line_h)).collect();
                    }
                    // The paragraph's own shapes are on the page from here on (one that was there from the page's top, as the first pass found
                    // this paragraph on it, is the same obstacle); the ones before them reach the paragraph too.
                    let own: Vec<WrapObstacle> = boxes.iter().map(FloatBox::obstacle).collect();
                    let same = |a: &WrapObstacle, b: &WrapObstacle| (a.shape_left - b.shape_left).abs() < 0.01 && (a.shape_right - b.shape_right).abs() < 0.01 && (a.y_top - b.y_top).abs() < 0.01 && (a.y_bottom - b.y_bottom).abs() < 0.01;
                    active_obstacle.retain(|obs| !own.iter().any(|n| same(obs, n)) && obs.y_bottom - para_top > 0.01);
                    let mut list: Vec<WrapObstacleInPara> = active_obstacle.iter().map(|obs| obs.in_paragraph(para_top)).collect();
                    list.extend(boxes.iter().map(|bx| bx.in_paragraph(para_top)));
                    active_obstacle.extend(own);
                    if shapes.iter().any(|shape| is_page_shape(shape)) {
                        // Where this paragraph is put is noted as its pieces are placed (below).
                        self.page_anchors.borrow_mut().entry(b).or_insert(usize::MAX);
                    }
                    list
                }
            } else {
                Vec::new()
            };
            // 浮動の表: placed on the page from its anchor (not in the
            // text's flow), the text after it round it.
            // A paragraph's text frame with a width of its own (`w:framePr w:w`) is placed as a floating table is: on the page from its anchor, the text after it round it.
            let para_frame = match block {
                Block::Paragraph(p) if p.frame.is_some() && !is_cap_frame(p) => p.frame,
                _ => None,
            };
            let floating = match block {
                Block::Table(t) => t.floating,
                _ => para_frame.map(|f| f.float),
            };
            if let Some(f) = floating {
                use ibis_doc_model::{TableAlign, TableAnchor};
                // (A frame without a width is as wide as its widest line, up to the text's width.)
                let frame_width = para_frame.map_or(frame.width, |f| if f.width > 0 { pt(f.width) } else { self.content_widths(std::slice::from_ref(block), number).1.min(frame.width) });
                let mut pieces = self.pieces(block, frame_width, number, &[], false);
                place_anchored_on_one_page(&mut pieces);
                // A frame's height (`w:h`): exact, the lines that start below it are not drawn; at least, the frame is as high as it says when its lines are lower.
                let mut frame_extra = 0.0f32;
                if let Some(tf) = para_frame && tf.height > 0 && tf.height_rule != ibis_doc_model::FrameHeightRule::Auto {
                    let (hh, content): (f32, f32) = (pt(tf.height), pieces.iter().map(|p| p.height).sum());
                    if tf.height_rule == ibis_doc_model::FrameHeightRule::Exact {
                        let mut top_of = 0.0f32;
                        pieces.retain(|p| {
                            let keep = top_of < hh;
                            top_of += p.height;
                            keep
                        });
                        frame_extra = hh - pieces.iter().map(|p| p.height).sum::<f32>();
                    } else {
                        frame_extra = (hh - content).max(0.0);
                    }
                }
                let h: f32 = pieces.iter().map(|p| p.height).sum::<f32>() + frame_extra;
                // Its anchor is the paragraph after it: when that goes on to
                // the next page, so does the table.
                if let Some(Block::Paragraph(_)) = self.doc.body.get(b + 1) {
                    let first = self.pieces(&self.doc.body[b + 1], frame.width, number, &[], false).into_iter().next().map_or(0.0, |q| q.height);
                    let room = notes_room(pages.last().expect("a page"));
                    if y + first > frame.bottom - room + 0.01 && (y > col_top + 0.01 || col_top > frame.top + 0.01 || col + 1 < columns_of(s).len()) {
                        active_obstacle.clear();
                        advance!(s);
                    }
                }
                let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                for (a, b) in pieces.iter().filter_map(|p| p.table_bounds) {
                    lo = lo.min(a);
                    hi = hi.max(b);
                }
                for it in pieces.iter().flat_map(|p| &p.items) {
                    let (a, b) = match it {
                        Item::Rect { x, w, .. } | Item::Shading { x, w, .. } => (*x, x + w),
                        Item::Stroke { x, x2, w, .. } => (x.min(*x2) - w / 2.0, x.max(*x2) + w / 2.0),
                        _ => continue,
                    };
                    lo = lo.min(a);
                    hi = hi.max(b);
                }
                if let Some(tf) = para_frame {
                    // The frame is as wide as it says, from its own left edge.
                    (lo, hi) = (0.0, frame_width);
                }
                if lo > hi {
                    (lo, hi) = (0.0, 0.0);
                }
                let w = hi - lo;
                let page_w = pt(setups[s].page.size.width);
                let page_h = pt(setups[s].page.size.height);
                let text = text_frame(s);
                let (base_x, base_w) = match f.horz_anchor {
                    TableAnchor::Page => (0.0, page_w),
                    TableAnchor::Margin => (text.left, text.width),
                    TableAnchor::Text => (frame.left, frame.width),
                };
                let odd = number % 2 == 1;
                // Measured on Word's PDF of DTFL: an offset puts the table's
                // edge 0.24 pt before it.
                let left = match f.x_align {
                    Some(TableAlign::Start) => base_x,
                    Some(TableAlign::Center) => base_x + (base_w - w) / 2.0,
                    Some(TableAlign::End) => base_x + base_w - w,
                    Some(TableAlign::Inside) => if odd { base_x } else { base_x + base_w - w },
                    Some(TableAlign::Outside) => if odd { base_x + base_w - w } else { base_x },
                    None => base_x + pt(f.x) - if para_frame.is_some() { 0.0 } else { 0.24 },
                };
                let (base_y, base_h) = match f.vert_anchor {
                    TableAnchor::Page => (0.0, page_h),
                    TableAnchor::Margin => (text.top, text.bottom - text.top),
                    TableAnchor::Text => (y, 0.0),
                };
                let top = match f.y_align {
                    Some(TableAlign::Start) => base_y,
                    Some(TableAlign::Center) => base_y + (base_h - h) / 2.0,
                    Some(TableAlign::End) => base_y + base_h - h,
                    _ => base_y + pt(f.y),
                };
                // A table placed on the page (not by the text) that the text already on the page runs into, and that does not
                // fit below it, goes on to the next page.
                if f.vert_anchor != TableAnchor::Text && y > top + 1.0 && y > col_top + 1.0 {
                    active_obstacle.clear();
                    advance!(s);
                }
                let [dl, dr, dt, db] = f.distance.map(pt);
                // Under a floating table it would overlap.
                let mut joined: Option<(f32, f32, f32)> = None;
                let top = match last_float {
                    Some((pg, a, z, from, bottom)) if pg + 1 == pages.len() && top < bottom && top + h > from && left < z && left + w > a => {
                        joined = Some((from, a, z));
                        bottom + dt
                    }
                    _ => top,
                };
                let mut at = top;
                let mut chunk_top = top;
                // The cells' text 0.9 pt further left than the borders' outer
                // edge would put it (DTFL: each placement's text 0.9 pt left
                // of where this layout's cell text falls).
                let dx = left - lo - if para_frame.is_some() { 0.0 } else { 0.9 };
                let mut queue: std::collections::VecDeque<Piece> = pieces.into();
                while let Some(mut piece) = queue.pop_front() {
                    // A row that does not fit is divided where the page ends (as in the text's flow), its rest going on at the
                    // top of the next page (Word: a floating table of one row as high as the page goes on to the next).
                    if piece.height > text.bottom - at + 0.01
                        && let Some(split) = piece.split.clone()
                        && let Some((first, rest)) = split.split(text.bottom - at)
                        && rest.height < piece.height - 1.0
                    {
                        queue.push_front(rest);
                        piece = first;
                        piece.page_break = false;
                    }
                    // A table longer than the page goes on at the top of the
                    // next one.
                    if at + piece.height > text.bottom + 0.01 && at > chunk_top + 0.01 {
                        active_obstacle.clear();
                        advance!(s);
                        at = text.top;
                        chunk_top = at;
                    }
                    pages.last_mut().expect("a page").items.extend(piece.items.into_iter().map(|it| offset(it, dx, at)));
                    at += piece.height;
                }
                at += frame_extra;
                let (top, h) = (chunk_top, at - chunk_top);
                // A frame the text does not go round (`w:wrap="none"`) is drawn over it: nothing keeps the text off.
                if para_frame.is_some_and(|f| f.wrap == ibis_doc_model::FrameWrap::None) {
                    continue;
                }
                // One it goes round only above and below (`notBeside`) takes the whole width of the text.
                let (left, w, dl, dr) = if para_frame.is_some_and(|f| f.wrap == ibis_doc_model::FrameWrap::NotBeside) { (text.left, text.width, 0.0, 0.0) } else { (left, w, dl, dr) };
                // Tables stacked one under the other are one obstacle to the text.
                let (from, a, z) = joined.map_or((top, left, left + w), |(from, a, z)| (from, a.min(left), z.max(left + w)));
                last_float = Some((pages.len() - 1, a, z, from, top + h + db));
                // Stacked tables are one obstacle: the one that joins them takes the place of the earlier ones. Shapes already on the page stay.
                if joined.is_some() {
                    active_obstacle.retain(|obs| !obs.float_table);
                }
                active_obstacle.push(WrapObstacle { shape_left: a - dl - text.left, shape_right: z + dr - text.left, y_top: from - dt, y_bottom: top + h + db, wrap_text: "bothSides".into(), wrap_polygon: None, float_table: true });
                continue;
            }
            let mut pieces = self.pieces(block, frame.width, number, &obstacles_for_block, false);
            // An empty paragraph that only carries a continuous section break takes no room: the next section starts on the line under the text before it (Word's PDF
            // of corpus 020 page 2: 54.0, 65.5, 77.1 where the two break paragraphs' lines (11.5 pt each, the line grid) would have given 54.0, 77.0, 100.0; 150 documents:
            // only 020 changes, to Word's 2 pages and 15 more lines).
            if let Block::Paragraph(p) = block && p.section_break == SectionBreak::Continuous && p.runs.is_empty() {
                for piece in pieces.iter_mut() {
                    piece.height = 0.0;
                    piece.space_after = 0.0;
                    piece.grid_below = 0.0;
                }
            }
            // A section ending with the document's last paragraph starts no page.
            if b + 1 == self.doc.body.len()
                && matches!(block, Block::Paragraph(p) if p.section_break != SectionBreak::None)
                && let Some(last) = pieces.last_mut()
            {
                last.page_break = false;
            }
            // A paragraph kept whole (段落を分割しない) starts the next page
            // when its lines do not all fit (and they fit on a page).
            if let Block::Paragraph(p) = block
                && p.keep_lines
            {
                let total: f32 = pieces.iter().map(|p| p.height).sum();
                if y + total > frame.bottom + 0.01 && (y > col_top + 0.01 || col_top > frame.top + 0.01 || col + 1 < columns_of(s).len()) && total <= frame.bottom - frame.top {
                    active_obstacle.clear();
                    advance!(s);
                    if !obstacles_for_block.is_empty() {
                        pieces = self.pieces(block, frame.width, number, &[], false);
                    }
                }
            }
            // Widow/orphan control: of a paragraph's lines, the page's end
            // does not leave the first alone (all of them go on) nor the last
            // alone (the one before goes on too).
            let mut break_before: Option<usize> = None;
            if let Block::Paragraph(p) = block
                && p.widow_control
                && !p.keep_lines
                && pieces.len() >= 2
                && pieces.iter().all(|q| q.split.is_none() && !q.header && !q.page_break && !q.column_break)
            {
                let page = pages.last().expect("a page");
                let limit = frame.bottom - notes_room(page) + 0.01;
                let mut used = y;
                let mut fit = 0usize;
                for q in &pieces {
                    if used + q.height > limit {
                        break;
                    }
                    used += q.height;
                    fit += 1;
                }
                let n = pieces.len();
                let can_move = y > col_top + 0.01 || col_top > frame.top + 0.01 || col + 1 < columns_of(s).len();
                if fit > 0 && fit < n && can_move {
                    if fit == 1 || (fit == n - 1 && n == 3) {
                        active_obstacle.clear();
                        advance!(s);
                        if !obstacles_for_block.is_empty() {
                            pieces = self.pieces(block, frame.width, number, &[], false);
                        }
                    } else if fit == n - 1 {
                        break_before = Some(n - 2);
                    }
                }
            }
            let headers: Vec<Piece> = pieces.iter().take_while(|p| p.header).cloned().collect();
            let mut queue: std::collections::VecDeque<Piece> = pieces.into();
            let mut popped = 0usize;
            let mut relaid = false;
            // Where each piece of the block was put (page, top, height), and the cells merged down whose text waits for that: (their first row's index, page, top, cell).
            let mut block_rows: Vec<(usize, f32, f32)> = Vec::new();
            let mut waiting: Vec<(usize, usize, f32, Arc<AnchoredCell>)> = Vec::new();
            while let Some(mut piece) = queue.pop_front() {
                if break_before == Some(popped) {
                    active_obstacle.clear();
                    advance!(s);
                    break_before = None;
                }
                popped += 1;
                // Paragraphs side by side: the larger of the space after one
                // and before the next, not both (unless `w:doNotUseHTMLParagraphAutoSpacing` sums them).
                if !self.doc.do_not_use_html_paragraph_auto_spacing
                    && let Some((pc, c, ye, after)) = prev_end
                    && pc == pages.len()
                    && c == col
                    && (y - ye).abs() < 0.01
                    && after > 0.0
                    && piece.space_before > 0.0
                {
                    let taken = after.min(piece.space_before);
                    y -= taken;
                    // The space the first line's box has above it in the PDF (its link) is the part of the space before the space after does not cover: the boxes of the
                    // paragraphs follow each other.
                    for it in piece.items.iter_mut() {
                        if let Item::Line { pad, .. } = it {
                            pad.0 = (pad.0 - taken).max(0.0);
                        }
                    }
                }
                // A table row that does not fit is divided where the page
                // ends (when a line of each cell fits there), its rest
                // going on at the top of the next page.
                if piece.height > frame.bottom - y + 0.01
                    && let Some(split) = piece.split.clone()
                    && let Some((first, rest)) = split.split(frame.bottom - y)
                {
                    queue.push_front(rest);
                    piece = first;
                    piece.page_break = false;
                    let items = piece.items.into_iter().map(|it| place_item(it, y, s, number, frame.left));
                    let page = pages.last_mut().expect("a page");
                    let start = page.items.len();
                    page.items.extend(items);
                    let end = page.items.len();
                    placed.push((start..end, y, frame.bottom - y, col));
                    if col_fill.len() <= col {
                        col_fill.resize(col + 1, col_top);
                    }
                    col_fill[col] = frame.bottom;
                    active_obstacle.clear();
                    advance!(s);
                    for h in &headers {
                        pages.last_mut().expect("a page").items.extend(h.items.iter().cloned().map(|it| place_item(it, y, s, number, frame.left)));
                        y += h.height;
                    }
                    continue;
                }
                // A paragraph kept with the next goes to the next page with
                // the next block's first line when that does not fit; a table's
                // heading row goes with the first body row after it.
                let kept = if piece.header {
                    let rest_headers: f32 = queue.iter().take_while(|q| q.header).map(|q| q.height).sum();
                    let first_body = queue.iter().find(|q| !q.header).map_or(0.0, |q| {
                        if let Some(s) = &q.split {
                            s.cells.iter().map(|c| c.content.iter().find_map(|it| if let Item::Line { y, h, .. } = it { Some(y + h + c.bottom) } else { None }).unwrap_or(0.0)).fold(0.0f32, f32::max)
                        } else {
                            q.height
                        }
                    });
                    rest_headers + first_body
                } else if piece.keep_with_next {
                    // A chain of paragraphs kept with the next goes together (Word moves a run of headings as one): what follows is the
                    // whole of each paragraph that is itself kept with the next, then the first line of the one that ends the chain
                    // (a chain longer than a page is not kept: the page's start takes it as it comes).
                    let mut kept = 0.0f32;
                    let mut k = b + 1;
                    while let Some(next) = self.doc.body.get(k) {
                        let next_s = sections_of[k];
                        let saved_pitch = self.line_pitch.get();
                        self.line_pitch.set(setups[next_s].page.line_pitch);
                        let next_pieces = self.pieces(next, frame.width, number, &[], false);
                        self.line_pitch.set(saved_pitch);
                        let chained = next_pieces.last().is_some_and(|n| n.keep_with_next) && k + 1 < self.doc.body.len() && next_s == s;
                        if chained && kept + next_pieces.iter().map(|n| n.height).sum::<f32>() < frame.bottom - frame.top {
                            kept += next_pieces.iter().map(|n| n.height).sum::<f32>();
                            k += 1;
                        } else {
                            // The lines of the paragraph that ends the chain which the page must hold with it: the ones the paragraph's own
                            // rules let it start the page with (widow/orphan control keeps its first two, a paragraph kept whole all of
                            // them), not just the first line, or the heading is left at the page's foot when that paragraph goes on whole.
                            let whole: f32 = next_pieces.iter().map(|n| n.height).sum();
                            let plain = next_pieces.iter().all(|q| q.split.is_none() && !q.header && !q.page_break && !q.column_break);
                            kept += match next {
                                Block::Paragraph(np) if np.keep_lines && whole <= frame.bottom - frame.top => whole,
                                Block::Paragraph(np) if np.widow_control && !np.keep_lines && next_pieces.len() >= 2 && plain => next_pieces[0].height + next_pieces[1].height,
                                _ => next_pieces.first().map_or(0.0, |n| n.height),
                            };
                            break;
                        }
                    }
                    kept
                } else {
                    0.0
                };
                // Room for the footnotes of the marks on it, at the page's
                // bottom.
                let page = pages.last().expect("a page");
                let new_notes: Vec<u32> = notes_of(&piece).into_iter().filter(|id| !page.notes.contains(id)).collect();
                // The notes of the line stay with it, but a note longer than the room is cut between its lines (Word's PDF of `vertical-footnote-continue.docx`: the body
                // goes on to the line with the mark and the notes take the rest of the page, the others go on to the next page); the line goes on when the room holds the
                // separator and the first line of each note it adds, whatever the rest of them needs.
                let columns = columns_of(page.section).len();
                let need_of = |entries: Vec<(u32, usize, f32)>| -> f32 {
                    if entries.is_empty() { 0.0 } else { separator_h + thickness(&entries.iter().map(|e| e.2).collect::<Vec<_>>(), columns) }
                };
                let need_full = need_of(note_entries(page, &new_notes));
                let need_least = need_of(note_entries(page, &[]).into_iter().chain(new_notes.iter().filter_map(|id| note_cache.get(id).and_then(|ps| ps.first()).map(|p| (*id, 0, p.height)))).collect());
                // An empty paragraph that only ends a section, or a line of a page break that text follows in its paragraph, stays on the page though it
                // does not fit (Word does not make a blank page of it: the corpus's repro-F-pagebreak-line-at-bottom). A page break that ends its paragraph
                // needs room like any line: when the page is full it is on the next page and breaks from there (Word, measured on Windows in
                // compatibility modes 14 and 15, GitHub issue #234 W8: a break after a full page makes 3 pages).
                let break_only = matches!(block, Block::Paragraph(p) if (p.section_break != SectionBreak::None || (piece.page_break && !queue.is_empty())))
                    && piece.items.iter().all(|it| matches!(it, Item::Line { .. }));
                // The space after a paragraph that ends the page does not need room (Word keeps the
                // paragraph there when its lines fit: 016, an empty paragraph after a table).
                let foot = page_foot(y, piece.height, piece.space_after, piece.grid_below, kept);
                let page_full = page.split_take.is_some();
                let overflow = page_full || (foot > frame.bottom - need_full + 0.01 && (new_notes.is_empty() || foot > frame.bottom - need_least + 0.01));
                if overflow && !break_only && (y > col_top + 0.01 || col_top > frame.top + 0.01 || col + 1 < columns_of(s).len()) {
                    active_obstacle.clear();
                    advance!(s);
                    // The first piece of a paragraph that was laid beside (or below) an obstacle of the page it leaves:
                    // on the new page that obstacle is not there, so its lines are laid again (otherwise the lines
                    // skipped down past the obstacle's foot move on with it as blank space).
                    if popped == 1 && !relaid && !obstacles_for_block.is_empty() && matches!(block, Block::Paragraph(_)) {
                        relaid = true;
                        queue = self.pieces(block, frame.width, number, &[], false).into();
                        popped = 0;
                        continue;
                    }
                    // A table's heading rows start the new page.
                    if !piece.header {
                        for h in &headers {
                            pages.last_mut().expect("a page").items.extend(h.items.iter().cloned().map(|it| place_item(it, y, s, number, frame.left)));
                            y += h.height;
                        }
                    }
                }
                // A paragraph that comes first in a page's column has no space before when it got there by the natural flow, a page-break
                // character, or (compatibility mode 15, or when the paragraph before has space after) `w:pageBreakBefore`; the document's first
                // column, the first paragraph of a section (whatever kind of break begins it) and a table's cells keep it (Word's XPS, issue #191
                // request 3: all of compatibility modes 12, 14 and 15 agree except `pageBreakBefore`, which is kept in 12 and 14).
                let first_of_section = b == 0 || sections_of[b - 1] != s;
                let by_own_accord = matches!(block, Block::Paragraph(p) if p.page_break_before);
                let after_before = prev_end.is_some_and(|e| e.3 > 0.0);
                if piece.space_before > 0.0
                    && !(pages.len() == 1 && col == 0)
                    && !first_of_section
                    && (y - col_top).abs() < 0.01
                    && (!by_own_accord || self.doc.compat_mode.is_some_and(|m| m >= 15) || after_before)
                {
                    // The piece starts that much above the column's top, so that its first line is at the top and the piece still ends where
                    // its height says.
                    y -= piece.space_before;
                    piece.space_before = 0.0;
                    for it in piece.items.iter_mut() {
                        if let Item::Line { pad, .. } = it {
                            pad.0 = 0.0;
                        }
                    }
                }
                // A drop cap's frame takes no room in the flow; the text after it goes round it: the width its characters advance and its
                // `hSpace`, over the lines whose tops are above its foot (Word's XPS, issue #191 request 4: the 31.5 pt "カ" in a line of 30 pt
                // exactly keeps 31.58 pt off, whatever `lines` says, and 3 lines of 13.6 pt).
                let cap_frame = match block {
                    Block::Paragraph(p) if is_cap_frame(p) => {
                        let (units, _) = self.units(p, number);
                        let start = pt(p.left + p.first_line).max(0.0);
                        let advance: f32 = units.iter().map(unit_width).sum();
                        let space = p.drop_cap.map_or(0.0, |d| pt(d.space));
                        Some((y + piece.space_before, piece.height - piece.space_before - piece.space_after, (start + advance).min(frame.width) + space))
                    }
                    _ => None,
                };
                let flow_height = if cap_frame.is_some() { 0.0 } else { piece.height };
                let page = pages.last_mut().expect("a page");
                for id in new_notes {
                    if !page.notes.contains(&id) {
                        page.notes.push(id);
                    }
                }
                // Notes that do not all fit under the line: the page takes the pieces that fit, the rest go on to the next page.
                if page.split_take.is_none() {
                    let entries = note_entries(page, &[]);
                    let heights: Vec<f32> = entries.iter().map(|e| e.2).collect();
                    let columns = columns_of(page.section).len();
                    let room = frame.bottom - (y + flow_height);
                    if !entries.is_empty() && separator_h + thickness(&heights, columns) > room + 0.01 {
                        let k = note_fit(&heights, (room - separator_h).max(0.0), columns);
                        page.split_take = Some(k);
                        *pending_carry.borrow_mut() = note_rest(&entries, k);
                    }
                }
                let anchored_here = std::mem::take(&mut piece.anchored);
                let items = piece.items.into_iter().map(|it| place_item(it, y, s, number, frame.left));
                let start = page.items.len();
                page.items.extend(items);
                placed.push((start..page.items.len(), y, flow_height, col));
                waiting.extend(anchored_here.into_iter().map(|c| (block_rows.len(), pages.len() - 1, y, c)));
                block_rows.push((pages.len() - 1, y, flow_height));
                if let Some((top, height, right)) = cap_frame {
                    active_obstacle.push(WrapObstacle { shape_left: 0.0, shape_right: right, y_top: top, y_bottom: top + height, wrap_text: "right".into(), wrap_polygon: None, float_table: false });
                    piece.space_after = 0.0;
                }
                if popped == 1 && let Some(at) = self.page_anchors.borrow_mut().get_mut(&b) {
                    *at = pages.len() - 1;
                }
                y += flow_height;
                prev_end = Some((pages.len(), col, y, piece.space_after));
                if col_fill.len() <= col {
                    col_fill.resize(col + 1, col_top);
                }
                col_fill[col] = col_fill[col].max(y);
                // 段区切り (and a section starting in the next column).
                if piece.column_break && !piece.page_break {
                    advance!(s);
                }
                // A page break right before an empty paragraph that ends a
                // section on a new page: that paragraph stays on this page
                // (Word's PDF of DSEC makes one page of them, not two).
                let ends_section = matches!(block, Block::Paragraph(p) if p.section_break != SectionBreak::None) && queue.is_empty();
                let breaks_again = !ends_section
                    && queue.is_empty()
                    && matches!(self.doc.body.get(b + 1), Some(Block::Paragraph(next)) if next.runs.is_empty() && next.section.is_some() && matches!(next.section_break, SectionBreak::NextPage | SectionBreak::OddPage | SectionBreak::EvenPage));
                // A page break that ends the document's last paragraph, with nothing after it, begins no page (Word: the caret is behind the break
                // on the page the break is on, and its PDF has no blank page after it; GitHub issue #234). The paragraph's mark stays on the
                // break's page; a paragraph after the break (even an empty one) is what makes the new page.
                let ends_document = queue.is_empty() && b + 1 == self.doc.body.len() && matches!(block, Block::Paragraph(p) if p.section_break == SectionBreak::None);
                if piece.page_break && !breaks_again && !ends_document {
                    // The page after a section's end is the next section's.
                    let s_next = if ends_section { (s + 1).min(setups.len() - 1) } else { s };
                    new_page!(s_next);
                    active_obstacle = find_page_obstacle(s_next, number, &frame, pages.len() - 1);
                    // An odd- or even-page section break: a blank page
                    // between when the next page has the other parity.
                    // (Only at the section's end: a page break inside the section's last paragraph is an ordinary one.)
                    let wanted = match block {
                        Block::Paragraph(p) if ends_section && p.section_break == SectionBreak::OddPage => Some(1),
                        Block::Paragraph(p) if ends_section && p.section_break == SectionBreak::EvenPage => Some(0),
                        _ => None,
                    };
                    // (A section that restarts its numbering (`w:pgNumType w:start`) gets no blank page: Word's PDFs of GitHub issue #223, an
                    // even-page section restarting at 7 and an odd-page one both stay on the next page, while without a restart an odd-page
                    // break after one page makes a blank page and an even-page one does not.)
                    // Word (measured on Windows, 150 documents, tasks/doc/requests/2026-11-15-223-section-start-parity-result.md): the blank page is
                    // decided by the parity of the continued number (the previous section's last number + 1), not the physical page; a section with
                    // a start number adds none but rounds the start up to the parity (even 7 -> 8, odd 2 -> 3).
                    if setups[s_next].restart_numbering {
                        if wanted.is_some_and(|w| number % 2 != w) {
                            number += 1;
                            let first = pages.last().is_some_and(|p| p.first_in_section);
                            *pages.last_mut().expect("a page") = page_of(s_next, number, first);
                            frame = frame_for(s_next, first, number);
                            y = frame.top;
                            col_top = y;
                            active_obstacle = find_page_obstacle(s_next, number, &frame, pages.len() - 1);
                        }
                    } else if wanted.is_some_and(|w| number % 2 != w) {
                        // The page just begun stays blank, the section's
                        // before; the next section starts on the one after.
                        let last = pages.last_mut().expect("a page");
                        last.section = s;
                        last.first_in_section = false;
                        new_page!(s_next);
                    }
                }
            }
            // The text of the cells merged down that is not set from the top, in the part of each on the page its first row is on.
            for (first, page_index, top, cell) in waiting.drain(..) {
                let rows_h: f32 = block_rows.iter().skip(first).take(cell.rows.len()).take_while(|row| row.0 == page_index).map(|row| row.2).sum();
                let number = pages[page_index].number;
                let items = cell.place(rows_h).into_iter().map(|it| place_item(it, top, s, number, frame.left)).collect::<Vec<_>>();
                pages[page_index].items.extend(items);
            }
            active_obstacle.retain(|obs| y < obs.y_bottom - 0.01);
        }
        close_region!();
        // Notes that go on past the last page: pages of their own.
        for _ in 0..1000 {
            if pending_carry.borrow().is_empty() {
                break;
            }
            let s = pages.last().map_or(0, |p| p.section);
            new_page!(s);
        }
        // The endnotes: either at each section's end or at the document's end.
        let endnotes_with_section: Vec<(usize, u32)> = {
            let mut all: Vec<(usize, u32)> = Vec::new();
            for (b, block) in self.doc.body.iter().enumerate() {
                let s = sections_of[b];
                fn walk(b: &Block, s: usize, out: &mut Vec<(usize, u32)>) {
                    match b {
                        Block::Paragraph(p) => {
                            for r in &p.runs {
                                if let FlowRunKind::NoteRef { endnote: true, id } = r.kind
                                    && !out.iter().any(|(_, i)| *i == id)
                                {
                                    out.push((s, id));
                                }
                            }
                        }
                        Block::Table(t) => {
                            for row in &t.rows {
                                for cell in &row.cells {
                                    for cb in &cell.blocks {
                                        walk(cb, s, out);
                                    }
                                }
                            }
                        }
                    }
                }
                walk(block, s, &mut all);
            }
            all
        };
        let mut sect_endnotes: std::collections::HashMap<usize, Vec<u32>> = std::collections::HashMap::new();
        let mut doc_endnotes: Vec<u32> = Vec::new();
        for (s, id) in endnotes_with_section {
            let pos = setups.get(s).map_or(self.doc.page.endnotes.position, |st| st.page.endnotes.position);
            if pos == ibis_doc_model::NotePosition::SectionEnd {
                sect_endnotes.entry(s).or_default().push(id);
            } else {
                doc_endnotes.push(id);
            }
        }
        // A section's endnotes go under its last page's text; the ones that do not fit go on in pages of their own after it (E4: they were
        // laid on, past the paper's end, and cut off). Later sections first, so that the pages of the earlier ones keep their places.
        let mut sect_endnotes: Vec<(usize, Vec<u32>)> = sect_endnotes.into_iter().filter(|(_, ids)| !ids.is_empty()).collect();
        sect_endnotes.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
        for (s, ids) in sect_endnotes {
            if let Some(mut pg_idx) = pages.iter().rposition(|p| p.section == s) {
                let frame = frame_of(s);
                let body_bottom = pages[pg_idx]
                    .items
                    .iter()
                    .filter_map(|it| match it {
                        Item::Line { y, h, .. } => Some(y + h),
                        Item::Picture { y, h, .. } | Item::Shape { y, h, .. } | Item::Rect { y, h, .. } => Some(y + h),
                        _ => None,
                    })
                    .fold(frame.top, f32::max);
                // The notes follow the text at once: the separator's own line (its height, `separator_h`) and no gap before it (Word's PDF of
                // `endnotes-overflow.docx`: the last text line's box ends at 712.1, the first note's line starts 13.6 below, the separator's line's height).
                let mut note_y = body_bottom.min(frame.bottom);
                if let Some(w) = separator_width(self.doc.note_separators[0], frame.width) {
                    pages[pg_idx].items.push(Item::Rect { x: frame.left, y: note_y + separator_h / 2.0, w, h: 0.5, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::BLACK)) });
                }
                note_y += separator_h;
                for id in ids {
                    // A note's lines go on the next page together unless two or more stay: no first line alone at the foot and no last line alone
                    // at the top (widow and orphan control, as the body's paragraphs have: Word's PDF of `endnotes-overflow.docx`: a note of two
                    // lines that does not fit whole is moved whole, page 1 ends with the second note's lines both on page 2).
                    let mut rest: std::collections::VecDeque<Piece> = note_pieces(true, id).into();
                    let n = rest.len();
                    let mut first_chunk = true;
                    while !rest.is_empty() {
                        let mut fit = 0;
                        let mut at = note_y;
                        while fit < rest.len() && at + rest[fit].height <= frame.bottom + 0.01 {
                            at += rest[fit].height;
                            fit += 1;
                        }
                        let can_move = note_y > frame.top + 0.01;
                        let take = if fit == rest.len() {
                            fit
                        } else if !can_move {
                            fit.max(1)
                        } else if first_chunk && (fit <= 1 || (fit + 1 == n && n == 3)) {
                            0
                        } else if first_chunk && fit + 1 == n {
                            fit - 1
                        } else {
                            fit
                        };
                        for piece in rest.drain(..take) {
                            pages[pg_idx].items.extend(piece.items.into_iter().map(|it| offset(it, frame.left, note_y)));
                            note_y += piece.height;
                        }
                        first_chunk = false;
                        if !rest.is_empty() {
                            let number = pages[pg_idx].number + 1;
                            for later in pages.iter_mut().skip(pg_idx + 1) {
                                if later.first_in_section && setups[later.section].restart_numbering {
                                    break;
                                }
                                later.number += 1;
                            }
                            pages.insert(pg_idx + 1, page_of(s, number, false));
                            pg_idx += 1;
                            note_y = frame.top;
                        }
                    }
                }
            }
        }
        if !doc_endnotes.is_empty() {
            let s = setups.len() - 1;
            let mut queue: Vec<Piece> = Vec::new();
            let end_separator: Vec<Item> = separator_width(self.doc.note_separators[1], frame.width).map(|w| Item::Rect { x: 0.0, y: separator_h / 2.0, w, h: 0.5, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::BLACK)) }).into_iter().collect();
            queue.push(Piece { height: separator_h, items: end_separator, page_break: false, column_break: false, keep_with_next: true, header: false, split: None, ..Default::default() });
            for id in &doc_endnotes {
                queue.extend(note_pieces(true, *id));
            }
            for piece in queue {
                if y + piece.height > frame.bottom - notes_room(pages.last().expect("a page")) + 0.01 && y > frame.top + 0.01 {
                    new_page!(s);
                }
                let items = piece.items.into_iter().map(|it| offset(it, frame.left, y));
                pages.last_mut().expect("a page").items.extend(items);
                y += piece.height;
            }
        }
        // The footnotes under a separator: either at page bottom or beneath text.
        for pg in pages.iter_mut() {
            let entries = note_entries(pg, &[]);
            let entries = entries[..pg.split_take.map_or(entries.len(), |k| k.min(entries.len()))].to_vec();
            if entries.is_empty() {
                continue;
            }
            let frame = frame_of(pg.section);
            let pos = setups.get(pg.section).map_or(self.doc.page.footnotes.position, |s| s.page.footnotes.position);
            let mut y = match pos {
                ibis_doc_model::NotePosition::BeneathText => {
                    let body_bottom = pg
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            Item::Line { y, h, .. } => Some(y + h),
                            Item::Picture { y, h, .. } | Item::Shape { y, h, .. } | Item::Rect { y, h, .. } => Some(y + h),
                            _ => None,
                        })
                        .fold(frame.top, f32::max);
                    // Right after the text: Word's PDF of `vertical-footnote` `v-p-beneath` has the separator's column against the text's last column (0.1 pt), no space between.
                    let spacing = 0.0;
                    (body_bottom + spacing).min(frame.bottom - notes_room(pg))
                }
                _ => frame.bottom - notes_room(pg),
            };
            if let Some(w) = separator_width(self.doc.note_separators[0], frame.width) {
                pg.items.push(Item::Rect { x: frame.left, y: y + separator_h / 2.0, w, h: 0.5, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::BLACK)) });
            }
            y += separator_h;
            // The notes' pieces fill the columns of the section (the same as the body's) one after another, each as high as the tallest of them needs.
            let columns = columns_of(pg.section).len();
            let heights: Vec<f32> = entries.iter().map(|e| e.2).collect();
            let limit = thickness(&heights, columns);
            let (mut col, mut used) = (0usize, 0.0f32);
            for (id, i, h) in &entries {
                if used + h > limit + 0.001 && used > 0.0 && col + 1 < columns {
                    col += 1;
                    used = 0.0;
                }
                if let Some(piece) = note_cache.get(id).and_then(|ps| ps.get(*i)) {
                    let x = column_frame(pg.section, col).left;
                    pg.items.extend(piece.items.iter().cloned().map(|it| offset(it, x, y + used)));
                }
                used += h;
            }
        }
        if std::env::var_os("IBIS_DEBUG_PAGES").is_some() {
            for (n, pg) in pages.iter().enumerate() {
                let first: String = pg.items.iter().filter_map(|it| if let Item::Text { text, .. } = it { Some(text.to_string()) } else { None }).collect::<String>().chars().take(16).collect();
                eprintln!("{}\t{}\t{first}", n + 1, pg.number);
            }
        }
        // 垂直方向の配置 (vAlign): the body moved down to the middle or the
        // bottom of the text area; 見開きページ: an even page's body moved
        // to its swapped margins.
        for pg in &mut pages {
            let page = &setups[pg.section].page;
            let frame = text_frame(pg.section);
            let used = pg
                .items
                .iter()
                .filter_map(|it| match it {
                    Item::Line { y, h, .. } => Some(y + h),
                    Item::Picture { y, h, .. } | Item::Shape { y, h, .. } | Item::Rect { y, h, .. } => Some(y + h),
                    _ => None,
                })
                .fold(f32::MIN, f32::max);
            let room = frame.bottom - used;
            let dy = match page.v_align {
                ibis_doc_model::PageVAlign::Center if used > f32::MIN && room > 0.0 => room / 2.0,
                ibis_doc_model::PageVAlign::Bottom if used > f32::MIN && room > 0.0 => room,
                _ => 0.0,
            };
            let dx = mirror_dx(pg.section, pg.number);
            if dx != 0.0 || dy != 0.0 {
                pg.items = std::mem::take(&mut pg.items).into_iter().map(|it| offset(it, dx, dy)).collect();
            }
        }
        // 行番号: the body's lines (not tables', not suppressed, not an
        // empty paragraph that only ends a section) counted by their
        // section's numbering, every count_by'th numbered left of the text
        // (Word's PDF of DSEC: right-aligned a quarter inch, or the
        // distance, from the text; "continuous" goes on from the section
        // before when that had numbers, whatever its restarting).
        {
            use ibis_doc_model::LineNumberRestart;
            // Each numbered paragraph's section.
            let mut numbered: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
            for (b, block) in self.doc.body.iter().enumerate() {
                if let Block::Paragraph(p) = block
                    && !p.suppress_line_numbers
                    && !(p.runs.is_empty() && p.section.is_some())
                {
                    numbered.insert(p.id, sections_of[b]);
                }
            }
            let (_, base) = self.doc.styles.paragraph_base(None, None);
            let style = base.style();
            let size = style.size as f32 / 100.0;
            let font = self.font(&style, style.font_for('1'));
            let mut count = 0u32;
            // The section of the last line counted, and whether it was numbered.
            let mut last: Option<(usize, bool)> = None;
            for (pg_index, pg) in pages.iter_mut().enumerate() {
                let lines: Vec<(f32, f32, usize)> = pg
                    .items
                    .iter()
                    .filter_map(|it| match it {
                        Item::Line { para, y, h, .. } => numbered.get(para).map(|s| (*y, *h, *s)),
                        _ => None,
                    })
                    .collect();
                let mut seen: Vec<f32> = Vec::new();
                let mut new_page = true;
                let mut labels = Vec::new();
                for (y, h, s) in lines {
                    // A line split round an obstacle counts once.
                    if seen.iter().any(|t| (t - y).abs() < 0.01) {
                        continue;
                    }
                    seen.push(y);
                    let Some(n) = setups[s].page.line_numbers else {
                        last = Some((s, false));
                        continue;
                    };
                    let new_section = last.is_none_or(|(ls, _)| ls != s);
                    let continues = n.restart == LineNumberRestart::Continuous && last.is_some_and(|(_, numbered)| numbered);
                    if (new_section && !continues) || (new_page && n.restart == LineNumberRestart::NewPage) {
                        count = n.start - 1;
                    }
                    new_page = false;
                    last = Some((s, true));
                    count += 1;
                    if count % n.count_by != 0 {
                        continue;
                    }
                    let distance = if n.distance > 0 { pt(n.distance) } else { 18.0 };
                    labels.push((y, h, count, text_frame(s).left - distance));
                }
                let _ = pg_index;
                for (y, h, n, right) in labels {
                    // On the line's baseline (its text's).
                    let baseline = pg.items.iter().find_map(|it| match it {
                        Item::Text { y: by, .. } if *by > y && *by <= y + h + 0.01 => Some(*by),
                        _ => None,
                    }).unwrap_or(y + h * 0.8);
                    let label = n.to_string();
                    let w = self.res.text_width(&label, &font, size);
                    let mut offsets = Vec::new();
                    let mut at = 0.0;
                    for c in label.chars() {
                        offsets.push(at);
                        at += self.res.text_width(&c.to_string(), &font, size);
                    }
                    pg.items.push(Item::Text { x: right - w, y: baseline, text: label.into(), offsets: offsets.into(), font: font.clone(), size, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::BLACK)) });
                }
            }
        }
        // Headers and footers, with each page's number: the first page's
        // own, the even pages' own, or the default, of the page's section.
        let total = pages.len() as u32;
        let section_pages: Vec<u32> = pages.iter().map(|pg| pages.iter().filter(|q| q.section == pg.section).count() as u32).collect();
        // 章番号つきのページ番号: each page's chapter, the number of the first heading of the chapter
        // style that starts on it, else the one before (`w:pgNumType/@w:chapStyle`).
        let heading_levels: std::collections::HashMap<u32, u32> = self.doc.body.iter().filter_map(|b| match b {
            Block::Paragraph(p) => ibis_doc_model::flow_command::heading_level(&self.doc.styles, p).map(|l| (p.id, l)),
            _ => None,
        }).collect();
        // The number part only (第1章 -> 1), from the numbering levels' formats, not the label's text. Before the first chapter heading of the
        // level, the page carries the number of the first one after it (Word 365: 1.1-1 on a page before the first heading 2).
        let chapter_numbers = ibis_doc_model::chapter_numbers(&self.doc);
        let mut chapter_now: Option<String> = pages.iter().find_map(|pg| {
            let (level, _) = setups[pg.section].page_chapter.as_ref()?;
            self.doc.body.iter().find_map(|b| match b {
                Block::Paragraph(p) if heading_levels.get(&p.id) == Some(level) => chapter_numbers.get(&p.id).cloned(),
                _ => None,
            })
        });
        let chapter_prefixes: Vec<String> = pages.iter().map(|pg| {
            let Some((level, sep)) = &setups[pg.section].page_chapter else { return String::new() };
            let starts = pg.items.iter().find_map(|item| match item {
                Item::Line { para, stops, .. } if stops.first().is_some_and(|s| s.0 == 0) && heading_levels.get(para) == Some(level) => Some(*para),
                _ => None,
            });
            if let Some(id) = starts {
                chapter_now = chapter_numbers.get(&id).cloned();
            }
            let separator = match sep.as_str() {
                "period" => '.',
                "colon" => ':',
                "emDash" => '\u{ff0d}',
                _ => '-',
            };
            chapter_now.as_ref().map_or(String::new(), |l| format!("{l}{separator}"))
        }).collect();
        // The running heads and feet of the vertical sections' pages, in paper coordinates (Word's PDFs of GitHub issue #188, D6a/D6b, ＭＳ 明朝 10.5 pt: the head's
        // baseline is `header` + 10.56 pt from the paper's top and the foot's `footer` + 3.00 above its bottom, as in a section across; a head or foot stays
        // across, even with `w:textDirection tbRl`; the text's box is the paper's width between the left margin and the right one with the gutter).
        let mut fixed_items: Vec<Vec<Item>> = (0..pages.len()).map(|_| Vec::new()).collect();
        for (page_index, ((pg, &in_section), chapter)) in pages.iter_mut().zip(&section_pages).zip(&chapter_prefixes).enumerate() {
            pg.body_items_count = pg.items.len();
            *self.chapter_prefix.borrow_mut() = chapter.clone();
            self.page_counts.set(Some((total, in_section, pg.section as u32 + 1)));
            let s = pg.section;
            let setup = &setups[s];
            let mut frame = text_frame(s);
            let n = pg.number;
            frame.left += mirror_dx(s, n);
            *self.page_format.borrow_mut() = setup.page_number_format.clone();
            self.line_pitch.set(setup.page.line_pitch);
            self.char_space.set(char_space_of(&setup.page));
            self.char_grid.set(char_grid_of(&setup.page));
            self.snap_chars.set(setup.page.doc_grid_type == ibis_doc_model::DocGridType::SnapToChars);
            self.vertical.set(setup.page.direction.vertical());
            let (header, footer) = if setup.title_page && pg.first_in_section {
                (&parts[s][2], &parts[s][3])
            } else if self.doc.even_and_odd && n % 2 == 0 {
                (&parts[s][4], &parts[s][5])
            } else {
                (&parts[s][0], &parts[s][1])
            };
            if directions[s].vertical() {
                self.vertical.set(false);
                let (paper_w, paper_h) = (pg.height, pg.width);
                let (left, right) = paper_margins[s];
                // 見開きページ: an even page's margins are swapped, the gutter with the right one (Word's PDFs of GitHub issue #188, A11: left 72, right 108, gutter 0 or 36 in a
                // vertical section: the feet's box is [72, 487.32 - gutter] on an odd page and [108 + gutter, 523.32] on an even one).
                let (left, right) = if setup.page.is_mirrored() && n % 2 == 0 { (right, left) } else { (left, right) };
                let width = paper_w - left - right;
                let mut y = pt(setup.page.header);
                for block in header {
                    for piece in self.header_footer_pieces(block, width, n) {
                        fixed_items[page_index].extend(piece.items.into_iter().map(|it| offset(it, left, y)));
                        y += piece.height;
                    }
                }
                let footer: Vec<Piece> = footer.iter().flat_map(|b| self.header_footer_pieces(b, width, n)).collect();
                let height: f32 = footer.iter().map(|p| p.height).sum();
                let mut y = paper_h - pt(setup.page.footer) - height;
                for piece in footer {
                    fixed_items[page_index].extend(piece.items.into_iter().map(|it| offset(it, left, y)));
                    y += piece.height;
                }
                continue;
            }
            let mut y = pt(setup.page.header);
            for block in header {
                for piece in self.header_footer_pieces(block, frame.width, n) {
                    pg.items.extend(piece.items.into_iter().map(|it| offset(it, frame.left, y)));
                    y += piece.height;
                }
            }
            let footer: Vec<Piece> = footer.iter().flat_map(|b| self.header_footer_pieces(b, frame.width, n)).collect();
            let height: f32 = footer.iter().map(|p| p.height).sum();
            let mut y = pg.height - pt(setup.page.footer) - height;
            for piece in footer {
                pg.items.extend(piece.items.into_iter().map(|it| offset(it, frame.left, y)));
                y += piece.height;
            }
        }
        self.chapter_prefix.borrow_mut().clear();
        for pg in &mut pages {
            pg.items = std::mem::take(&mut pg.items).into_iter().map(|it| offset(it, TEXT_ORIGIN_X, 0.0)).collect();
        }
        // Page borders: `space` from the page's edges to their outer side,
        // or from the text area (the margins) to their inner side.
        for pg in &mut pages {
            let page = &setups[pg.section].page;
            if !page.borders.iter().any(Option::is_some) {
                continue;
            }
            let frame = text_frame(pg.section);
            let (page_w, page_h) = (pg.width, pg.height);
            let [top, right, bottom, left] = page.borders;
            let total = |b: Option<FlowBorder>| b.map_or(0.0, |b| pt(b.total()));
            let space = |b: Option<FlowBorder>| b.map_or(0.0, |b| pt(b.space));
            let (t, r, b, l) = if page.borders_from_text {
                (
                    frame.top - space(top) - total(top) / 2.0,
                    page_w - pt(page.right) + space(right) + total(right) / 2.0,
                    frame.bottom + space(bottom) + total(bottom) / 2.0,
                    frame.left - space(left) - total(left) / 2.0,
                )
            } else {
                (space(top) + total(top) / 2.0, page_w - space(right) - total(right) / 2.0, page_h - space(bottom) - total(bottom) / 2.0, space(left) + total(left) / 2.0)
            };
            // An art border's corner: the top or bottom border's picture
            // (else the left or right one's), sz × sz about the corner.
            let corner = |cx: f32, cy: f32, b1: Option<&FlowBorder>, b2: Option<&FlowBorder>, part: ArtPart, items: &mut Vec<Item>| {
                let art_border = b1.and_then(|b| b.art.map(|a| (b, a))).or_else(|| b2.and_then(|b| b.art.map(|a| (b, a))));
                if let Some((fb, art)) = art_border
                    && art_border_drawn(art)
                    && art_border::picture(art, part).is_some()
                {
                    let sz = pt(fb.width);
                    let tint = art_border::tint(art, fb.color);
                    // Fitted in sz × sz, keeping its proportions (heartBalloon's
                    // corner is 330 × 660: half as wide as it is high).
                    let (pw, ph) = art_border::picture(art, part).map_or((1.0, 1.0), |p| (p.width as f32, p.height as f32));
                    let k = sz / pw.max(ph) * art_border::scale(art, sz);
                    let (iw, ih) = (pw * k, ph * k);
                    // Against the outer corner for the kinds made smaller there.
                    let (x, y) = if art_border::outer(art) {
                        let left = matches!(part, ArtPart::TopLeft | ArtPart::BottomLeft);
                        let top = matches!(part, ArtPart::TopLeft | ArtPart::TopRight);
                        (if left { cx - sz / 2.0 } else { cx + sz / 2.0 - iw }, if top { cy - sz / 2.0 } else { cy + sz / 2.0 - ih })
                    } else {
                        (cx - iw / 2.0, cy - ih / 2.0)
                    };
                    art_border::warm(art, part, tint, iw.min(ih));
                    items.push(Item::Art { art, part, x, y, w: iw, h: ih, image_w: iw, image_h: ih, tint });
                }
            };
            corner(l, t, top.as_ref(), left.as_ref(), ArtPart::TopLeft, &mut pg.items);
            corner(r, t, top.as_ref(), right.as_ref(), ArtPart::TopRight, &mut pg.items);
            corner(l, b, bottom.as_ref(), left.as_ref(), ArtPart::BottomLeft, &mut pg.items);
            corner(r, b, bottom.as_ref(), right.as_ref(), ArtPart::BottomRight, &mut pg.items);
            if let Some(x) = top {
                if let Some(art) = x.art {
                    draw_art_edge(&x, art, ArtPart::Top, l, r, t, true, &mut pg.items);
                } else {
                    pg.items.extend(border_items(&x, true, true, t, l, r, true, true, false));
                }
            }
            if let Some(x) = bottom {
                if let Some(art) = x.art {
                    draw_art_edge(&x, art, ArtPart::Bottom, l, r, b, true, &mut pg.items);
                } else {
                    pg.items.extend(border_items(&x, true, false, b, l, r, true, true, false));
                }
            }
            if let Some(x) = left {
                if let Some(art) = x.art {
                    draw_art_edge(&x, art, ArtPart::Left, t, b, l, false, &mut pg.items);
                } else {
                    pg.items.extend(border_items(&x, false, true, l, t, b, true, true, false));
                }
            }
            if let Some(x) = right {
                if let Some(art) = x.art {
                    draw_art_edge(&x, art, ArtPart::Right, t, b, r, false, &mut pg.items);
                } else {
                    pg.items.extend(border_items(&x, false, false, r, t, b, true, true, false));
                }
            }
        }
        for pg in &mut pages {
            let mut expanded = Vec::with_capacity(pg.items.len());
            for it in pg.items.drain(..) {
                match it {
                    Item::Shading { pattern, color, x, y, w, h } => {
                        expanded.extend(pattern_items(pattern, color, x, y, w, h));
                    }
                    other => expanded.push(other),
                }
            }
            pg.items = expanded;
        }
        for pg in &mut pages {
            if matches!(directions[pg.section], ibis_doc_model::CellDirection::TbRl | ibis_doc_model::CellDirection::BtLr) {
                let direction = directions[pg.section];
                // The paper: the across page's height is its width.
                let (w, h) = (pg.height, pg.width);
                pg.items = turn_page_items(std::mem::take(&mut pg.items), direction, w, h, &|c, font, size| self.res.text_width(&c.to_string(), font, size));
                (pg.width, pg.height) = (w, h);
                // The marks of the text boundaries are in the paper too (#289): the body's corners turned with the page.
                let [l, t, r, b] = pg.text_area;
                let at = |x: f32, y: f32| if direction == ibis_doc_model::CellDirection::TbRl { (w - y, x) } else { (y, h - x) };
                let ((ax, ay), (bx, by)) = (at(l, t), at(r, b));
                pg.text_area = [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)];
            }
        }
        for (pg, fixed) in pages.iter_mut().zip(fixed_items) {
            for it in fixed {
                match it {
                    Item::Shading { pattern, color, x, y, w, h } => pg.items.extend(pattern_items(pattern, color, x, y, w, h)),
                    other => pg.items.push(other),
                }
            }
        }
        Pagination { pages }
    }
}

/// Whether the cell at `col` in the row below `r` continues a vertical merge.
fn continues_below(t: &FlowTable, grid: &[Vec<impl HasCol>], r: usize, col: usize) -> bool {
    let Some(row) = t.rows.get(r + 1) else { return false };
    grid[r + 1].iter().zip(&row.cells).any(|(l, c)| l.col() == col && c.merge == VerticalMerge::Continue)
}

trait HasCol {
    fn col(&self) -> usize;
}

/// Whether Word draws this art border at all. The model keeps every `w:val`
/// (and saving writes it back); only drawing stops.
///
/// candyCorn・checkedBarColor・confettiOutline・cornerTriangles・decoArchColor・
/// pushPinNote1 は、Mac の Word（2026-10-05 ユーザーが印刷レイアウトの画面で確認）が
/// 描かない。PDF の書き出しにも出ない（網羅試験 DPGB の正解 PDF、9/30 に Mac の Word で作成）。
/// Windows の Word では未確認。
fn art_border_drawn(art: ibis_doc_model::ArtBorder) -> bool {
    use ibis_doc_model::ArtBorder::*;
    !matches!(art, CandyCorn | CheckedBarColor | ConfettiOutline | CornerTriangles | DecoArchColor | PushPinNote1)
}

/// The pictures of art border `art` along an edge, `part`, from `from` to
/// `to` (the corners' centres) at `across` (the border's middle line). A
/// picture as long as the page (a strip in the zip) is drawn once, sz thick
/// and cut at the corners; a shorter one is repeated, sz thick and as many as
/// fit at its own proportions, stretched to fill the edge evenly.
fn draw_art_edge(fb: &FlowBorder, art: ibis_doc_model::ArtBorder, part: ArtPart, from: f32, to: f32, across: f32, horizontal: bool, items: &mut Vec<Item>) {
    if !art_border_drawn(art) {
        return;
    }
    let Some(picture) = art_border::picture(art, part) else { return };
    let sz = pt(fb.width);
    if sz <= 0.0 {
        return;
    }
    let tint = art_border::tint(art, fb.color);
    // From corner to corner; the kinds made smaller against the outer side
    // have smaller corners there, which the edge meets.
    let inset = if art_border::outer(art) { sz * art_border::scale(art, sz) - sz / 2.0 } else { sz / 2.0 };
    let (start, end) = (from.min(to) + inset, from.max(to) - inset);
    let span = end - start;
    if span <= 0.0 {
        return;
    }
    // The pictures are drawn to one scale: the corner's longer side is sz
    // (Word's references: moons' edge 469 × 240 with a 240 × 240 corner
    // repeats every 1.9 to 2.0 sz, DPGB-00243/00244; heartBalloon's 330 × 660
    // with a 330 × 660 corner every sz, half as wide, DPGB-00211/00212;
    // confetti's 313 × 400 with a 400 × 400 corner every 0.75 to 0.79 sz,
    // DPGB-00125/00126).
    let (cw, ch) = art_border::picture(art, ArtPart::TopLeft).map_or((picture.width.min(picture.height), picture.width.min(picture.height)), |c| (c.width, c.height));
    let k = sz / cw.max(ch) as f32;
    // Their spacing goes by the corner's width along the top and bottom,
    // its height along the sides: heartBalloon (corner 330 × 660) repeats
    // every sz along the top too, its balloons half as wide (DPGB-00212),
    // paperClips (corner 280 × 560, top 172 × 560) every 0.62 sz (DPGB-00260).
    let cell = if horizontal { picture.width as f32 / cw as f32 } else { picture.height as f32 / ch as f32 } * sz;
    let (along, thick) = if horizontal { (picture.width as f32 * k, picture.height as f32 * k) } else { (picture.height as f32 * k, picture.width as f32 * k) };
    // Drawn smaller than its cell for some kinds, centred in it.
    let s = art_border::scale(art, sz);
    // Where the picture's band starts across the edge: about the middle
    // line, or from the outer side (the top and left edges' outer side is
    // their start).
    let first = matches!(part, ArtPart::Top | ArtPart::Left);
    let band = |t: f32| if !art_border::outer(art) { across - t / 2.0 } else if first { across - thick / 2.0 } else { across + thick / 2.0 - t };
    let place = |a: f32, len: f32, image_len: f32, strip: bool| {
        let (len, image_len, a, t) = if strip { (len, image_len * s, a, thick * s) } else { (len * s, image_len * s, a + len * (1.0 - s) / 2.0, thick * s) };
        let (x, y, w, h, image_w, image_h) =
            if horizontal { (a, band(t), len, t, image_len, t) } else { (band(t), a, t, len, t, image_len) };
        art_border::warm(art, part, tint, image_w.min(image_h));
        Item::Art { art, part, x, y, w, h, image_w, image_h, tint }
    };
    // A strip as long as the page (20 000 pixels in the zip): once, cut.
    if along >= span.max(thick * 4.0) {
        items.push(place(start, span, along, true));
        return;
    }
    // Repeated as many times as fit, stretched to fill the edge evenly,
    // each centred in its cell.
    if art_border::outer(art) {
        // Made smaller, they stay side by side (birds' band is unbroken in
        // Word, DPGB-00090): the cells shrink with them.
        let n = ((span / (cell * s)).round() as usize).max(1);
        let step = span / n as f32;
        let len = along * s * step / (cell * s);
        items.extend((0..n).map(|i| place(start + i as f32 * step + (step - len) / 2.0, len, len / s, true)));
        return;
    }
    // Some kinds Word sets farther apart than their proportions (the
    // picture keeps its length, the cell grows).
    let wide = cell * art_border::pitch_scale(art, sz);
    let n = ((span / wide).round() as usize).max(1);
    let step = span / n as f32;
    let len = along * step / wide;
    items.extend((0..n).map(|i| place(start + i as f32 * step + (step - len) / 2.0, len, len, false)));
}

/// The rectangles of border `b` along a cell's or paragraph's edge: at
/// `at` across, from `from` to `to` along, `horizontal` or not; `outer_first`
/// when the border's outer side is the top or left one. The border is
/// centred on the edge; its lines are extended by half its width at the
/// ends, so the corners meet.
/// The width (points) of the line above the notes: the short line is 2 inches, the full one the text's width; none for no line.
fn separator_width(kind: ibis_doc_model::NoteSeparator, text_width: f32) -> Option<f32> {
    match kind {
        ibis_doc_model::NoteSeparator::Standard => Some(144.0),
        ibis_doc_model::NoteSeparator::Full => Some(text_width),
        ibis_doc_model::NoteSeparator::Hidden => None,
    }
}

/// A table border's drawn thickness (pt): Word keeps the width in whole twips (1/20 pt, the fraction of `w:sz`'s eighths of a point cut
/// off) and draws it as a filled rectangle a whole number of the printer's pixels (1/600 inch = 0.12 pt) thick, the pixels cut off too,
/// at least one (Word's XPS of tables, issue #189: `w:sz` 1…8 give 0.12, 0.24, 0.24, 0.48, 0.60, 0.72, 0.84, 0.96 pt; `w:sz` 3 is
/// 0.375 pt = 7.5 twips → 7 twips = 0.35 pt = 2.9 pixels → 2).
fn printer_pixels_pt(width: f32) -> f32 {
    const PIXEL: f32 = 72.0 / 600.0;
    let twips = (width * 20.0 + 1e-3).floor();
    (twips / 20.0 / PIXEL + 1e-3).floor().max(1.0) * PIXEL
}

fn border_items(
    b: &FlowBorder,
    horizontal: bool,
    outer_first: bool,
    at: f32,
    from: f32,
    to: f32,
    extend_start: bool,
    extend_end: bool,
    table: bool,
) -> Vec<Item> {
    let color = hsla(ibis_doc_model::Color::opaque(b.color));
    // Word draws the bands of an uneven double line in the same order on
    // every side, from the top or the left (measured on its PDF of DTBB:
    // thinThickSmallGap has its thick line first, top to bottom and left to
    // right, on all four sides; thickThinSmallGap its thin one), not from
    // the outer side.
    let bands = b.bands();
    let total = pt(bands.iter().map(|b| b.0).sum());
    let dashes = b.dashes().map(|d| d.into_iter().map(pt).collect::<Vec<f32>>());
    let mut out = Vec::new();
    let mut across = at - total / 2.0;
    let mut depth_from_top = 0.0f32;
    let mut drawn_so_far = 0usize;
    for (width, drawn) in bands {
        let t = if table { printer_pixels_pt(pt(width)) } else { pt(width).max(0.25) };
        // The lines meet at the corners as nested rectangles do (a mitre): a band reaches past the edge's end by
        // half the border's width less its own distance from the border's outer side (an inner line stops at
        // the inner line of the border across).
        let depth = if outer_first { depth_from_top } else { (total - depth_from_top - pt(width)).max(0.0) };
        let reach = total / 2.0 - depth;
        depth_from_top += pt(width);
        let start = if extend_start { from - reach } else { from };
        let end = if extend_end { to + reach } else { to };
        if drawn {
            let mut segments = Vec::new();
            match &dashes {
                Some(pattern) if pattern.iter().sum::<f32>() > 0.1 => {
                    let mut a = start;
                    let mut i = 0;
                    while a < end {
                        let len = pattern[i % pattern.len()];
                        if i % 2 == 0 {
                            segments.push((a, (a + len).min(end)));
                        }
                        a += len;
                        i += 1;
                    }
                }
                _ => segments.push((start, end)),
            }
            // A wave is a sine line (Word's: 2.5 pt high, 5.75 pt long, measured on its PDF of DBRD); a double
            // wave is two of them, a half turn apart, one above the other.
            if matches!(b.style, ibis_doc_model::BorderStyle::Wave | ibis_doc_model::BorderStyle::DoubleWave) {
                let (line, amplitude) = (0.5f32, (t - 0.75) / 2.0);
                let length = 5.75f32;
                let centre = across + t / 2.0;
                let phase = if drawn_so_far % 2 == 1 { std::f32::consts::PI } else { 0.0 };
                drawn_so_far += 1;
                let steps = ((end - start) / length * 8.0).ceil().max(1.0) as usize;
                let point = |i: usize| {
                    let a = start + (end - start) * i as f32 / steps as f32;
                    (a, centre + amplitude * ((a - start) / length * std::f32::consts::TAU + phase).sin())
                };
                for i in 0..steps {
                    let (a0, c0) = point(i);
                    let (a1, c1) = point(i + 1);
                    out.push(if horizontal {
                        Item::Stroke { x: a0, y: c0, x2: a1, y2: c1, w: line, color }
                    } else {
                        Item::Stroke { x: c0, y: a0, x2: c1, y2: a1, w: line, color }
                    });
                }
                across += pt(width);
                continue;
            }
            // dashDotStroked: short lines slanted across the band, in pairs, over the band's white strip.
            if b.style == ibis_doc_model::BorderStyle::DashDotStroked {
                let bg = hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::WHITE));
                out.push(if horizontal {
                    Item::Rect { x: start, y: across, w: (end - start).max(0.0), h: t, color: bg }
                } else {
                    Item::Rect { x: across, y: start, w: t, h: (end - start).max(0.0), color: bg }
                });
                let (pitch, slant) = (2.5f32, t.min(3.5));
                let mut a = start;
                let mut n = 0usize;
                while a + slant < end {
                    // Two close lines, then a gap.
                    let from = a;
                    let (c0, c1) = (across + t, across);
                    out.push(if horizontal {
                        Item::Stroke { x: from, y: c0, x2: from + slant, y2: c1, w: 0.9, color }
                    } else {
                        Item::Stroke { x: c0, y: from, x2: c1, y2: from + slant, w: 0.9, color }
                    });
                    a += if n % 3 == 2 { pitch * 2.0 } else { pitch };
                    n += 1;
                }
                across += pt(width);
                continue;
            }
            for (a, z) in segments {
                out.push(if horizontal { Item::Rect { x: a, y: across, w: z - a, h: t, color } } else { Item::Rect { x: across, y: a, w: t, h: z - a, color } });
            }
        }
        across += pt(width);
    }
    out
}

/// A shape in a table cell with `layoutInCell`: Word measures its offsets from
/// the cell, not the page or the margin, so a shape placed from the page's or the
/// margin's edge lies from the cell's top and left edge (a position already
/// measured from the cell, with an alignment, or from the paragraph is kept).
fn in_cell_shape(item: Item) -> Item {
    let Item::Shape { shape, x, y, w, h } = &item else { return item };
    let Some(f) = shape.floating.as_ref().filter(|f| f.layout_in_cell && f.wrap != "topAndBottom") else { return item };
    let from_cell_x = f.align_h.is_none() && matches!(f.relative_h.as_str(), "page" | "margin" | "leftMargin");
    let from_cell_y = f.align_v.is_none() && matches!(f.relative_v.as_str(), "page" | "margin" | "topMargin");
    if !from_cell_x && !from_cell_y {
        return item;
    }
    let nx = if from_cell_x { pt(f.offset_x) } else { *x };
    let ny = if from_cell_y { pt(f.offset_y) } else { *y };
    // Placed in the cell now: the page's placing of shapes from the page's or margin's edge must not
    // place it again.
    let mut shape = shape.clone();
    if let Some(f) = shape.floating.as_mut() {
        if from_cell_y {
            f.relative_v = "paragraph".to_string();
        }
        if from_cell_x {
            f.relative_h = "column".to_string();
        }
    }
    Item::Shape { shape, x: nx, y: ny, w: *w, h: *h }
}

/// A cell merged down laid out over its rows' whole height, divided among the rows (`bounds`: each row's top and bottom in the cell's y); `text_by_row`: the text is set from the top, so each line goes with the row it stands in. The items
/// that stand in a row (a text by its baseline, a line, a picture by its top) are moved up by the row's top and go with it; a fill is cut at the rows' edges.
fn split_over_rows(items: Vec<Item>, bounds: &[(f32, f32)], text_by_row: bool) -> Vec<Vec<Item>> {
    let mut parts: Vec<Vec<Item>> = bounds.iter().map(|_| Vec::new()).collect();
    let row_of = |y: f32| bounds.iter().position(|&(_, bottom)| y < bottom).unwrap_or(bounds.len() - 1);
    for item in items {
        match item {
            Item::Rect { x, y, w, h, color } => {
                for (k, &(top, bottom)) in bounds.iter().enumerate() {
                    let (a, b) = (y.max(top), (y + h).min(bottom));
                    if b > a || (k == 0 && h <= 0.0) {
                        parts[k].push(Item::Rect { x, y: a - top, w, h: b - a, color });
                    }
                }
            }
            Item::Shading { pattern, color, x, y, w, h } => {
                for (k, &(top, bottom)) in bounds.iter().enumerate() {
                    let (a, b) = (y.max(top), (y + h).min(bottom));
                    if b > a {
                        parts[k].push(Item::Shading { pattern, color, x, y: a - top, w, h: b - a });
                    }
                }
            }
            // Text that is not set from the top (centred, at the foot, turned) is placed over the whole of the cell, so it stays with the first row:
            // Word places it within the part of the cell on the page it starts on, which is known only when the pages are made (Word's PDF of corpus 128:
            // the turned heading of a cell merged over a page's end is centred in the cell's part on the first page).
            other if !text_by_row => parts[0].push(other),
            other => {
                let y = match &other {
                    Item::Text { y, .. } | Item::Turned { y, .. } | Item::Scaled { y, .. } | Item::Line { y, .. } | Item::Picture { y, .. } | Item::Shape { y, .. } | Item::Stroke { y, .. } | Item::Art { y, .. } | Item::CommentBubble { y, .. } => *y,
                    Item::Rect { .. } | Item::Shading { .. } => unreachable!(),
                };
                let k = row_of(y);
                parts[k].push(offset(other, 0.0, -bounds[k].0));
            }
        }
    }
    parts
}

fn offset(item: Item, dx: f32, dy: f32) -> Item {
    match item {
        Item::Text { x, y, text, offsets, font, size, color } => Item::Text { x: x + dx, y: y + dy, text, offsets, font, size, color },
        Item::Turned { x, y, text, font, size, color, turns, xscale } => Item::Turned { x: x + dx, y: y + dy, text, font, size, color, turns, xscale },
        Item::Scaled { x, y, text, font, size, color, xscale } => Item::Scaled { x: x + dx, y: y + dy, text, font, size, color, xscale },
        Item::Rect { x, y, w, h, color } => Item::Rect { x: x + dx, y: y + dy, w, h, color },
        Item::Art { art, part, x, y, w, h, image_w, image_h, tint } => Item::Art { art, part, x: x + dx, y: y + dy, w, h, image_w, image_h, tint },
        Item::Picture { media, x, y, w, h } => Item::Picture { media, x: x + dx, y: y + dy, w, h },
        Item::Shape { shape, x, y, w, h } => Item::Shape { shape, x: x + dx, y: y + dy, w, h },
        Item::Line { para, x, y, h, stops, vertical, list_tab, base, avail, pad, largest } => Item::Line { para, x: x + dx, y: y + dy, h, stops, vertical, list_tab, base, avail, pad, largest },
        Item::Stroke { x, y, x2, y2, w, color } => Item::Stroke { x: x + dx, y: y + dy, x2: x2 + dx, y2: y2 + dy, w, color },
        Item::Shading { pattern, color, x, y, w, h } => Item::Shading { pattern, color, x: x + dx, y: y + dy, w, h },
        Item::CommentBubble { x, y, size } => Item::CommentBubble { x: x + dx, y: y + dy, size },
    }
}

/// The notes' numbers as shown: footnotes 1, 2, 3 and endnotes i, ii, iii
/// (Word's defaults), in the order their marks come in the body.
fn note_numbers(doc: &FlowDocument) -> std::collections::HashMap<(bool, u32), String> {
    // The notes' marks in body order, with the section each is in.
    fn walk(blocks: &[Block], section: usize, out: &mut Vec<(bool, u32, usize)>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    for r in &p.runs {
                        if let FlowRunKind::NoteRef { endnote, id } = r.kind
                            && !out.iter().any(|(e, i, _)| (*e, *i) == (endnote, id))
                        {
                            out.push((endnote, id, section));
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, section, out)),
            }
        }
    }
    let ids = ibis_doc_model::flow_command::sections(doc);
    let settings: Vec<(ibis_doc_model::NoteSettings, ibis_doc_model::NoteSettings)> = ids
        .iter()
        .map(|id| ibis_doc_model::flow_command::section_setup(doc, *id).map_or((doc.page.footnotes, doc.page.endnotes), |s| (s.page.footnotes, s.page.endnotes)))
        .collect();
    let mut order = Vec::new();
    let mut section = 0usize;
    for b in &doc.body {
        walk(std::slice::from_ref(b), section, &mut order);
        // (A section ends at a paragraph with a setup of its own, as
        // `flow_command::sections` counts them.)
        if let Block::Paragraph(p) = b
            && p.section.is_some()
        {
            section += 1;
        }
    }
    // Numbered from each kind's start, again at each section when asked
    // (again at each page is counted on, as continuous). A note whose reference has a mark of its own (`w:customMarkFollows`) shows no number and takes none of the count (Word:
    // 1, *, 2; the mark is the text after the reference).
    let (mut f, mut e) = (0u32, 0u32);
    let (mut f_section, mut e_section) = (usize::MAX, usize::MAX);
    let custom = |endnote: bool, id: u32| (if endnote { &doc.endnotes } else { &doc.footnotes }).iter().any(|n| n.id == id && n.custom_mark);
    order
        .into_iter()
        .map(|(endnote, id, s)| {
            if custom(endnote, id) {
                return ((endnote, id), String::new());
            }
            let (foot, end) = settings.get(s).or(settings.last()).copied().unwrap_or((doc.page.footnotes, doc.page.endnotes));
            let (set, count, seen) = if endnote { (end, &mut e, &mut e_section) } else { (foot, &mut f, &mut f_section) };
            if *seen == usize::MAX || (set.restart == ibis_doc_model::NoteRestart::EachSection && *seen != s) {
                *count = set.start.max(1) - 1;
            }
            *seen = s;
            *count += 1;
            ((endnote, id), set.format.number(*count))
        })
        .collect()
}

/// Which paragraphs lose their space before and after to
/// `w:contextualSpacing`: a paragraph with it, next to one of the same style
/// (in the same body, header, footer or cell).
fn contextual_spacing(doc: &FlowDocument) -> std::collections::HashMap<u32, (bool, bool)> {
    // `prev` is the style of the paragraph before in the text's order, the last paragraph of the cell before included: Word takes a paragraph's
    // space before away when it has `w:contextualSpacing` and that one has its style (Windows Word, #54: the first paragraph of a cell after a cell of the same style loses it).
    fn visit(blocks: &[Block], in_cell: bool, prev: &mut Option<Option<String>>, out: &mut std::collections::HashMap<u32, (bool, bool)>) {
        for (i, block) in blocks.iter().enumerate() {
            match block {
                Block::Paragraph(p) => {
                    // The automatic space (`w:beforeAutospacing`) is none at the start of a cell, nor after at its end.
                    let (mut before, mut after) = (in_cell && i == 0 && p.auto_before, in_cell && i + 1 == blocks.len() && p.auto_after);
                    if p.contextual_spacing {
                        let same = |q: Option<&Block>| matches!(q, Some(Block::Paragraph(q)) if q.style == p.style);
                        before |= prev.as_ref() == Some(&p.style);
                        after |= same(blocks.get(i + 1));
                    }
                    *prev = Some(p.style.clone());
                    if before || after {
                        out.insert(p.id, (before, after));
                    }
                }
                Block::Table(t) => {
                    for cell in t.rows.iter().flat_map(|r| &r.cells) {
                        visit(&cell.blocks, true, prev, out);
                    }
                }
            }
        }
    }
    let mut out = std::collections::HashMap::new();
    for blocks in [&doc.body, &doc.header, &doc.footer, &doc.header_first, &doc.footer_first, &doc.header_even, &doc.footer_even] {
        visit(blocks, false, &mut None, &mut out);
    }
    out
}

/// Whether any section or the document has footnotes restarting each page.
pub(crate) fn has_each_page_notes(doc: &FlowDocument) -> bool {
    if doc.page.footnotes.restart == ibis_doc_model::NoteRestart::EachPage {
        return true;
    }
    let ids = ibis_doc_model::flow_command::sections(doc);
    for id in ids {
        if let Some(s) = ibis_doc_model::flow_command::section_setup(doc, id) {
            if s.page.footnotes.restart == ibis_doc_model::NoteRestart::EachPage {
                return true;
            }
        }
    }
    false
}

/// The notes' numbers updated with the pages footnotes are on, restarting
/// each page where `EachPage` is set.
pub(crate) fn note_numbers_with_pages(
    doc: &FlowDocument,
    pages: &[Page],
) -> std::collections::HashMap<(bool, u32), String> {
    let ids = ibis_doc_model::flow_command::sections(doc);
    let settings: Vec<(ibis_doc_model::NoteSettings, ibis_doc_model::NoteSettings)> = ids
        .iter()
        .map(|id| ibis_doc_model::flow_command::section_setup(doc, *id).map_or((doc.page.footnotes, doc.page.endnotes), |s| (s.page.footnotes, s.page.endnotes)))
        .collect();

    let mut map = note_numbers(doc);
    for pg in pages {
        let (foot_set, _) = settings.get(pg.section).or(settings.last()).copied().unwrap_or((doc.page.footnotes, doc.page.endnotes));
        if foot_set.restart == ibis_doc_model::NoteRestart::EachPage {
            let mut count = foot_set.start.max(1) - 1;
            for &note_id in &pg.notes {
                if doc.footnotes.iter().any(|n| n.id == note_id && n.custom_mark) {
                    continue;
                }
                count += 1;
                map.insert((false, note_id), foot_set.format.number(count));
            }
        }
    }
    map
}

/// Lay out `doc` into pages.
pub(crate) fn paginate(doc: &FlowDocument, res: &Resources) -> Pagination {
    // 差し込み印刷's 結果のプレビュー: the merge fields show a record's values.
    if let Some(values) = &res.merge_values {
        let merged = ibis_doc_model::merge::merged_view(doc, values);
        return paginate_collapsed(&merged, res);
    }
    paginate_collapsed(doc, res)
}

/// The document as the markup mode shows it: a paragraph whose mark is deleted is joined to the next one where the deleted text is not
/// shown (シンプルな変更履歴 and 変更履歴なし; all markup keeps the paragraphs, the text struck out), as Word shows it. The joined paragraph
/// is named by the paragraph whose mark stays, and has its formatting. `None` when nothing is joined.
fn with_deleted_marks_joined(doc: &FlowDocument, res: &Resources) -> Option<FlowDocument> {
    fn any(blocks: &[Block]) -> bool {
        blocks.iter().any(|b| match b {
            Block::Paragraph(p) => p.mark_revision.is_some(),
            Block::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| any(&c.blocks))),
        })
    }
    let mode = if res.print && !res.print_markup { MarkupDisplayMode::None } else { res.markup_view.mode };
    let joins = |p: &ibis_doc_model::FlowParagraph| -> bool {
        match p.mark_revision.as_ref() {
            Some(r) if r.kind.is_delete() => !res.markup_view.is_reviewer_shown(&r.author) || matches!(mode, MarkupDisplayMode::Simple | MarkupDisplayMode::None),
            _ => false,
        }
    };
    fn join(blocks: &mut Vec<Block>, joins: &dyn Fn(&ibis_doc_model::FlowParagraph) -> bool, styles: &std::sync::Arc<ibis_doc_model::FlowStyles>) {
        let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
        let mut chain: Vec<ibis_doc_model::FlowParagraph> = Vec::new();
        for block in std::mem::take(blocks) {
            match block {
                Block::Paragraph(p) => {
                    let joined = joins(&p);
                    chain.push(p);
                    if !joined {
                        flush(&mut chain, &mut out, styles);
                    }
                }
                Block::Table(mut t) => {
                    // A table ends a run of joined paragraphs: the last of them keeps its mark.
                    flush(&mut chain, &mut out, styles);
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            join(&mut cell.blocks, joins, styles);
                        }
                    }
                    out.push(Block::Table(t));
                }
            }
        }
        flush(&mut chain, &mut out, styles);
        *blocks = out;
    }
    fn flush(chain: &mut Vec<ibis_doc_model::FlowParagraph>, out: &mut Vec<Block>, styles: &std::sync::Arc<ibis_doc_model::FlowStyles>) {
        match chain.len() {
            0 => {}
            1 => out.push(Block::Paragraph(chain.remove(0))),
            _ => {
                let refs: Vec<&ibis_doc_model::FlowParagraph> = chain.iter().collect();
                let mut joined = ibis_doc_model::flow_command::join_marked(&refs, false);
                joined.resolve(styles);
                joined.normalize();
                out.push(Block::Paragraph(joined));
                chain.clear();
            }
        }
    }
    if !any(&doc.body) || !doc.body.iter().any(|_| true) {
        return None;
    }
    let mut view = doc.clone();
    let styles = view.styles.clone();
    join(&mut view.body, &joins, &styles);
    (view.body != doc.body).then_some(view)
}

/// The document with the text the input method is composing typed in (`Resources::ime_marked`), in the formatting it will have; `None` when
/// nothing is being composed. Only the layout sees it.
fn with_ime_text(doc: &FlowDocument, res: &Resources) -> Option<FlowDocument> {
    let (para, offset, text) = res.ime_marked.as_ref()?;
    let mut view = doc.clone();
    let at = ibis_doc_model::FlowPos::new(*para, *offset);
    ibis_doc_model::flow_command::apply_to(&mut view, ibis_doc_model::FlowCommand::InsertText { at, text: text.to_string(), format: None }).ok()?;
    Some(view)
}

fn paginate_collapsed(doc: &FlowDocument, res: &Resources) -> Pagination {
    if let Some(view) = with_ime_text(doc, res) {
        return paginate_collapsed_marks(&view, res);
    }
    paginate_collapsed_marks(doc, res)
}

fn paginate_collapsed_marks(doc: &FlowDocument, res: &Resources) -> Pagination {
    if let Some(view) = with_deleted_marks_joined(doc, res) {
        return paginate_collapsed_view(&view, res);
    }
    paginate_collapsed_view(doc, res)
}

fn paginate_collapsed_view(doc: &FlowDocument, res: &Resources) -> Pagination {
    // Headings shown collapsed: what is under them is not laid out.
    let any_collapsed = !res.collapsed_headings.is_empty() || doc.body.iter().any(|b| matches!(b, Block::Paragraph(p) if p.collapsed));
    if any_collapsed
        && let Some(view) = super::outline_ops::collapsed_view(doc, &res.collapsed_headings)
    {
        return paginate_whole(&view, res);
    }
    paginate_whole(doc, res)
}

fn paginate_whole(doc: &FlowDocument, res: &Resources) -> Pagination {
    let ids = ibis_doc_model::flow_command::sections(doc);
    let first_pitch = ids.first().and_then(|id| ibis_doc_model::flow_command::section_setup(doc, *id)).and_then(|s| s.page.line_pitch);
    let lay_out = |known_pages: Option<(u32, Vec<u32>)>, known_notes: Option<std::collections::HashMap<(bool, u32), String>>, anchors: Option<std::collections::HashMap<usize, usize>>| {
        let layouter = Layouter {
            res,
            doc,
            labels: ibis_doc_model::list_labels(doc),
            page_format: std::cell::RefCell::new(doc.page_number_format.clone()),
            chapter_prefix: std::cell::RefCell::new(String::new()),
            line_pitch: std::cell::Cell::new(first_pitch),
            char_space: std::cell::Cell::new(ids.first().and_then(|id| ibis_doc_model::flow_command::section_setup(doc, *id)).and_then(|s| char_space_of(&s.page))),
            snap_chars: std::cell::Cell::new(ids.first().and_then(|id| ibis_doc_model::flow_command::section_setup(doc, *id)).is_some_and(|s| s.page.doc_grid_type == ibis_doc_model::DocGridType::SnapToChars)),
            char_grid: std::cell::Cell::new(ids.first().and_then(|id| ibis_doc_model::flow_command::section_setup(doc, *id)).is_some_and(|s| char_grid_of(&s.page))),
            vertical: std::cell::Cell::new(ids.first().and_then(|id| ibis_doc_model::flow_command::section_setup(doc, *id)).map_or(doc.page.direction.vertical(), |s| s.page.direction.vertical())),
            line_room: std::cell::Cell::new(f32::MAX),
            page_counts: std::cell::Cell::new(None),
            known_pages,
            anchor_pages: anchors,
            page_anchors: std::cell::RefCell::new(std::collections::HashMap::new()),
            body_section: std::cell::Cell::new(0),
            note_numbers: known_notes.unwrap_or_else(|| note_numbers(doc)),
            current_note: std::cell::RefCell::new(String::new()),
            contextual: contextual_spacing(doc),
            current_cell: std::cell::Cell::new(None),
            field_paragraph: std::cell::Cell::new(None),
            comment_runs: map_comment_runs(doc),
        };
        let mut pagination = layouter.lay_out();
        layouter.inline_diagram_members(&mut pagination);
        layouter.grid_text_boxes(&mut pagination);
        layouter.fit_text_boxes(&mut pagination);
        (pagination, layouter.page_anchors.take())
    };
    let (first, page_anchors) = lay_out(None, None, None);
    let need_page_counts = doc.body.iter().any(has_page_counts);
    let need_each_page_notes = has_each_page_notes(doc);
    // A shape set from the page makes its hole on the page its paragraph is on, which the first pass found out.
    let need_anchor_pages = !page_anchors.is_empty();
    if !need_page_counts && !need_each_page_notes && !need_anchor_pages {
        return first;
    }

    let notes = if need_each_page_notes {
        Some(note_numbers_with_pages(doc, &first.pages))
    } else {
        None
    };

    let counts = if need_page_counts {
        let sections = first.pages.iter().map(|p| p.section + 1).max().unwrap_or(1);
        let per: Vec<u32> = (0..sections).map(|s| first.pages.iter().filter(|p| p.section == s).count() as u32).collect();
        Some((first.pages.len() as u32, per))
    } else {
        None
    };

    lay_out(counts, notes, need_anchor_pages.then_some(page_anchors)).0
}

/// Whether a block has `NUMPAGES` or `SECTIONPAGES` fields (or formulas
/// over them).
fn has_page_counts(block: &Block) -> bool {
    match block {
        Block::Paragraph(p) => p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Field { instruction, .. } if instruction.contains("NUMPAGES") || instruction.contains("SECTIONPAGES"))),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).any(has_page_counts),
    }
}

thread_local! {
    /// The fonts' generation (`fonts_generation`) the layout caches were made for.
    static FONTS_SEEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Pagination of the documents open, by the document (held so its
    /// address is not reused).
    #[allow(clippy::type_complexity)]
    static PAGINATIONS: std::cell::RefCell<Vec<(Arc<FlowDocument>, Arc<Pagination>, u64, bool, bool, bool, Arc<std::collections::BTreeSet<u32>>, Option<Arc<std::collections::HashMap<String, String>>>, MarkupView, Option<(u32, usize, Arc<str>)>, Option<(ibis_doc_model::ShapeId, ibis_doc_model::TextBody)>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The pages of `doc`, laid out once.
pub fn pages(doc: &Arc<FlowDocument>, res: &Resources) -> Arc<Pagination> {
    let edited_ref = res.edited_box.as_ref().and_then(|(id, _, body)| body.as_ref().map(|b| (*id, b)));
    PAGINATIONS.with(|cache| {
        let mut cache = cache.borrow_mut();
        // Laid out again when art borders' pictures have come since (the
        // web version fetches them).
        // Or when fonts came since (the browser version reads the downloaded fonts behind the first drawing, GitHub issue #157): the widths changed.
        let fonts = ibis_office_render::shape_view::fonts_generation();
        if FONTS_SEEN.replace(fonts) != fonts {
            PIECES.with(|c| c.borrow_mut().clear());
        }
        let generation = crate::art_border::generation().wrapping_add(fonts.wrapping_mul(1 << 32));
        cache.retain(|(_, _, g, _, _, _, _, _, _, _, _)| *g == generation);
        if let Some((_, p, _, _, _, _, _, _, _, _, _)) = cache.iter().find(|(d, _, _, pr, ph, pm, co, mv, mk, ime, eb)| Arc::ptr_eq(d, doc) && *ime == res.ime_marked && eb.as_ref().map(|(id, b)| (*id, b)) == edited_ref && *pr == res.print && *ph == res.print_hidden_text && *pm == res.print_markup && **co == *res.collapsed_headings && *mv == res.merge_values && *mk == res.markup_view) {
            return p.clone();
        }
        let started = crate::debug_timing::start();
        let mut p = paginate(doc, res);
        order_floating_shapes(&mut p);
        add_watermark(doc, &mut p);
        add_genko(doc, &mut p);
        let p = Arc::new(p);
        crate::debug_timing::report(&format!("paginate ({} pages)", p.pages.len()), started);
        if cache.len() >= 4 {
            cache.remove(0);
        }
        let edited_owned = edited_ref.map(|(id, b)| (id, b.clone()));
        cache.push((doc.clone(), p.clone(), generation, res.print, res.print_hidden_text, res.print_markup, res.collapsed_headings.clone(), res.merge_values.clone(), res.markup_view.clone(), res.ime_marked.clone(), edited_owned));
        p
    })
}

/// Floating shapes are drawn in `relativeHeight` order (a larger one in front),
/// not in the order they come in the document: each page's floating shapes
/// are put in that order among the slots they take (a stable sort, so equal
/// heights keep the document order).
fn order_floating_shapes(pagination: &mut Pagination) {
    for page in &mut pagination.pages {
        let slots: Vec<usize> = page.items.iter().enumerate().filter(|(_, i)| matches!(i, Item::Shape { shape, .. } if shape.floating.is_some())).map(|(n, _)| n).collect();
        if slots.len() < 2 {
            continue;
        }
        let height = |item: &Item| match item {
            Item::Shape { shape, .. } => shape.floating.as_ref().map_or(0, |f| f.relative_height),
            _ => 0,
        };
        let mut shapes: Vec<Item> = slots.iter().map(|&n| page.items[n].clone()).collect();
        if shapes.windows(2).all(|w| height(&w[0]) <= height(&w[1])) {
            continue;
        }
        shapes.sort_by_key(height);
        for (&n, item) in slots.iter().zip(shapes) {
            page.items[n] = item;
        }
    }
}

/// 原稿用紙設定 (K0063): the squares, underlines or frame of the document's 原稿用紙, behind every page's text, at the corner of the
/// text area (Word draws them as a shape in the header, anchored to the page there).
fn add_genko(doc: &FlowDocument, pagination: &mut Pagination) {
    let Some(genko) = &doc.genko else { return };
    let m = &doc.page;
    let vertical = m.direction == ibis_doc_model::CellDirection::TbRl;
    let left = m.left + if m.gutter_at_top { 0 } else { m.gutter };
    let top = m.top + if m.gutter_at_top { m.gutter } else { 0 };
    let (width, height) = ((m.size.width - left - m.right).max(1), (m.size.height - top - m.bottom).max(1));
    let parts = genko.parts(width, height, vertical);
    for page in &mut pagination.pages {
        let (ox, oy) = (pt(left), pt(top));
        let mut items = Vec::new();
        for (n, part) in parts.iter().enumerate() {
            let (x, y, w, h, thick, is_line) = match *part {
                ibis_doc_model::GenkoPart::Rect { x, y, w, h, thick } => (x, y, w, h, thick, false),
                ibis_doc_model::GenkoPart::Line { x, y, w, h } => (x, y, w, h, false, true),
            };
            let stroke = ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(genko.color), if thick { 12_700 } else { 6_350 });
            let kind = if is_line { ibis_doc_model::ShapeKind::Line(stroke) } else { ibis_doc_model::ShapeKind::Auto(ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, ibis_doc_model::Fill::None, Some(stroke))) };
            let shape = ibis_doc_model::Shape::new(ibis_doc_model::ShapeId(n as u64), ibis_doc_model::EmuRect::new(0, 0, w, h), kind);
            items.push(Item::Shape { shape: Box::new(shape), x: ox + pt(x), y: oy + pt(y), w: pt(w), h: pt(h) });
        }
        page.items.splice(0..0, items);
    }
}

/// 透かし (fix): the watermark behind every page's text, centred on the
/// margins, as Word draws its WordArt (a semi-transparent one drawn half
/// way to white, as on white paper).
fn add_watermark(doc: &FlowDocument, pagination: &mut Pagination) {
    if doc.watermark.is_none() && doc.section_watermarks.is_empty() {
        return;
    }
    let ids = ibis_doc_model::flow_command::sections(doc);
    let m = &doc.page;
    for page in &mut pagination.pages {
        let own = ids.get(page.section).and_then(|id| doc.section_watermarks.iter().find(|(s, _)| s == id)).map(|(_, w)| w.as_ref());
        let Some(w) = own.unwrap_or(doc.watermark.as_ref()) else { continue };
        if w.text.trim().is_empty() {
            continue;
        }
        let (left, top) = (pt(m.left + m.gutter), pt(m.top));
        let area = (page.width - left - pt(m.right), page.height - top - pt(m.bottom));
        let (sw, sh) = w.size_in(area.0, area.1);
        // Word stretches the text's ink to fill the shape (fitshape). One
        // size here: the largest whose ink fits; wide (CJK) characters each
        // in their own share of the width, as Word's stretched ones sit.
        let chars: Vec<char> = w.text.chars().collect();
        let wide = chars.iter().all(|&c| (c as u32) >= 0x2E80);
        let ink = if wide { 0.88 } else if chars.iter().all(|c| !c.is_lowercase()) { 0.66 } else { 0.75 };
        let ems = chars.iter().map(|&c| if (c as u32) >= 0x2E80 { 1.0 } else if c.is_ascii_uppercase() { 0.56 } else { 0.48 }).sum::<f32>().max(1.0);
        let pieces: Vec<String> = if wide && chars.len() > 1 { chars.iter().map(|c| c.to_string()).collect() } else { vec![w.text.clone()] };
        let slot = sw / pieces.len() as f32;
        let size = if pieces.len() > 1 { (sh / ink).min(slot) } else { (sw / ems).min(sh / ink) };
        let color = if w.semitransparent {
            let half = |c: u32| (c + 255) / 2;
            let c = w.color.0;
            Rgb(half((c >> 16) & 0xFF) << 16 | half((c >> 8) & 0xFF) << 8 | half(c & 0xFF))
        } else {
            w.color
        };
        let turn = w.rotation.rem_euclid(360);
        let (sin, cos) = (turn as f32).to_radians().sin_cos();
        let centre = (left + area.0 / 2.0, top + area.1 / 2.0);
        let mut items = Vec::new();
        for (i, piece) in pieces.iter().enumerate() {
            let style = ibis_doc_model::TextStyle { font_size: (size * 100.0) as u32, color, font: (!w.font.is_empty()).then(|| w.font.clone()), ..Default::default() };
            let mut body = ibis_doc_model::TextBody::plain(piece, style);
            body.paragraphs.iter_mut().for_each(|p| p.align = Some(ibis_doc_model::TextAlign::Center));
            body.anchor = ibis_doc_model::VerticalAnchor::Middle;
            // The ink's middle, not the line's: CJK glyphs sit low in the
            // line (Latin capitals about at its middle).
            let down = if wide { 0.2 } else { 0.0 };
            body.insets = ibis_doc_model::Insets { left: 0, top: (size * down * EMU_PER_POINT as f32) as i64, right: 0, bottom: 0 };
            body.wrap = false;
            let emu = |v: f32| (v * EMU_PER_POINT as f32).round() as i64;
            let mut shape = ibis_doc_model::Shape::new(ibis_doc_model::ShapeId(0), ibis_doc_model::EmuRect::new(0, 0, emu(slot), emu(sh)), ibis_doc_model::ShapeKind::TextBox(body));
            shape.rotation = turn * 60_000;
            shape.floating = Some(ibis_doc_model::FloatingAnchor {
                offset_x: 0,
                offset_y: 0,
                align_h: Some("center".into()),
                align_v: Some("center".into()),
                relative_h: "margin".into(),
                relative_v: "margin".into(),
                wrap: "none".into(),
                behind_doc: true,
                dist_t: 0,
                dist_b: 0,
                dist_l: 0,
                dist_r: 0,
                wrap_text: None,
                locked: false,
                allow_overlap: true,
                layout_in_cell: true,
                relative_height: ibis_doc_model::FloatingAnchor::DEFAULT_RELATIVE_HEIGHT,
                wrap_polygon: None,
                glue: Default::default(),
                frame_size: None,
            });
            // The piece's centre along the turned text.
            let along = (i as f32 + 0.5) * slot - sw / 2.0;
            let (cx, cy) = (centre.0 + along * cos, centre.1 + along * sin);
            items.push(Item::Shape { shape: Box::new(shape), x: cx - slot / 2.0, y: cy - sh / 2.0, w: slot, h: sh });
        }
        page.items.splice(0..0, items);
    }
}

/// A document's captions' numbers, cross-references and the like brought up to date as it is opened or exported (Word
/// does so when it opens the document: a figure added at the front is not 1-1 twice). Not an edit: no undo step.
pub fn update_fields_on_open(presentation: &mut ibis_doc_model::Presentation, res: &Resources) {
    let Some(doc) = presentation.flow.as_mut() else { return };
    let commands = ibis_doc_model::flow_command::updated_fields(doc);
    if !commands.is_empty() {
        let doc = Arc::make_mut(doc);
        for command in commands {
            let _ = ibis_doc_model::flow_command::apply_to(doc, command);
        }
    }
    make_marked_tocs(doc, res);
    // The index (INDEX) of the XE fields: with the pages they are laid out on, twice (the index moves the pages after it).
    for _ in 0..2 {
        let Some((first, instruction)) = super::index_fields::index_field(doc) else { break };
        let pagination = pages(doc, res);
        let mut numbers: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        for page in &pagination.pages {
            for item in &page.items {
                if let Item::Line { para, .. } = item {
                    numbers.entry(*para).or_insert(page.number);
                }
            }
        }
        let page_of = |id: u32| numbers.get(&id).copied();
        let commands = super::index_fields::index_commands(doc, first, true, &instruction, &page_of);
        let doc = Arc::make_mut(doc);
        for command in commands {
            let _ = ibis_doc_model::flow_command::apply_to(doc, command);
        }
    }
}

/// The tables of contents (TOC) a file marks for making again, made as Word makes them as it opens the file (measured on
/// Word 16 with its alerts off: a TOC field marked `w:dirty="true"` is made again, empty or with a placeholder result,
/// inside a content control or not; with `w:updateFields` in the settings every TOC is; one with neither is left as it is,
/// even if its result is empty). The pages are those the document lays out, twice: the table moves the pages after it.
/// Not an edit: no undo step.
fn make_marked_tocs(doc: &mut Arc<ibis_doc_model::FlowDocument>, res: &Resources) {
    use ibis_doc_model::flow_command::{self, FlowCommand};
    let all = doc.update_fields;
    let wanted: Vec<(u32, String)> = doc
        .body
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) if all || p.field_dirty => p.field_start.as_ref().filter(|f| f.trim_start().to_ascii_uppercase().starts_with("TOC")).map(|f| (p.id, f.clone())),
            _ => None,
        })
        .collect();
    for (first, instruction) in wanted {
        let figures = flow_command::toc_caption_label(&instruction);
        let instr = flow_command::parse_toc_instruction(&instruction);
        let missing = match &figures {
            Some(_) => flow_command::missing_table_of_figures_style(&doc.styles).into_iter().collect(),
            None => flow_command::missing_toc_styles_for_levels(&doc.styles, instr.all_levels()),
        };
        {
            let d = Arc::make_mut(doc);
            for (id, style) in missing {
                let _ = flow_command::apply_to(d, FlowCommand::SetStyle { id, style: Some(style) });
            }
        }
        let mut at = first;
        for _ in 0..2 {
            let pagination = pages(doc, res);
            let mut numbers: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
            for page in &pagination.pages {
                for item in &page.items {
                    if let Item::Line { para, .. } = item {
                        numbers.entry(*para).or_insert(page.number);
                    }
                }
            }
            let page_of = |id: u32| numbers.get(&id).copied();
            let width = doc.page.size.width - doc.page.left - doc.page.right;
            let new_first = doc.new_id();
            let (entries, bookmarks) = match &figures {
                Some(label) => flow_command::table_of_figures_entries(doc, label, &page_of, new_first, width),
                None => flow_command::toc_entries_with_instruction(doc, &instr, &page_of, new_first, width),
            };
            let d = Arc::make_mut(doc);
            if flow_command::apply_to(d, FlowCommand::SetTableOfContents { before: at, replace: true, entries, bookmarks }).is_err() {
                break;
            }
            at = new_first;
        }
    }
}

/// Where a hyperlink of a page of a flowing document goes in the PDF: an address, or a place of the document (a page, from 0, and the point on it, in points from its top left).
#[derive(Clone, Debug, PartialEq)]
pub enum PageLinkTarget {
    Url(String),
    Place { page: usize, x: f32, y: f32 },
}

/// A linked stretch of a line on a page, in points from the page's top left.
#[derive(Clone, Debug, PartialEq)]
pub struct PageLinkRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub target: PageLinkTarget,
}

/// A pixel of the 96 dpi page Word lays a line's characters out on, in points: the left and right of a link's rectangle are on its grid.
const LINK_PIXEL: f32 = 0.75;
/// How far Word's PDF has a link's rectangle reach out of the first and last linked character of a line: 3 pixels.
const LINK_REACH: f32 = 3.0 * LINK_PIXEL;

/// What a linked stretch of a paragraph is linked to.
#[derive(Clone)]
enum LinkTo {
    Link(ibis_doc_model::flow::Link),
    /// The reference to a footnote or an endnote (Word links it to the note without a `w:hyperlink`).
    Note { endnote: bool, id: u32 },
}

/// The hyperlinks of each page of `doc` (`w:hyperlink`: an address or a bookmark; a footnote's reference to its note; a picture's or a shape's link) as Word's PDF holds
/// them (#227): for each line of a linked stretch of text, a rectangle over the box of the line (from the first linked character's position 3 pixels out to the last one's end 3
/// pixels out, on the pixel grid when the line starts on it; the line's whole height, the space before a paragraph's first line and the one after its last line included);
/// for a picture or a shape the box that holds it. A bookmark's or a note's place is the point 3 pixels left of its paragraph's first line, at the paragraph's top.
pub fn page_links(doc: &Arc<FlowDocument>, res: &Resources) -> Vec<Vec<PageLinkRect>> {
    use ibis_doc_model::flow::Link;
    let pagination = pages(doc, res);
    use std::collections::HashMap;
    let mut spans: HashMap<u32, Vec<(usize, usize, LinkTo)>> = HashMap::new();
    // The pictures and shapes that are links: (the paragraph, the run's kind, the link).
    let mut objects: Vec<(u32, FlowRunKind, Link)> = Vec::new();
    // The bookmarks: (the paragraph, the character it starts at).
    let mut anchors: HashMap<String, (u32, usize)> = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    for item in pagination.pages.iter().flat_map(|p| &p.items) {
        let Item::Line { para, .. } = item else { continue };
        if !seen.insert(*para) {
            continue;
        }
        let Some(p) = ibis_doc_model::flow_command::paragraph(doc, *para) else { continue };
        for (k, name) in p.bookmarks.iter().enumerate() {
            anchors.entry(name.clone()).or_insert((*para, p.bookmark_at.get(k).copied().unwrap_or(0).min(p.len())));
        }
        let mut at = 0;
        let mut found: Vec<(usize, usize, LinkTo)> = Vec::new();
        for (index, r) in p.runs.iter().enumerate() {
            let len = r.len();
            match (&r.kind, &r.link) {
                // A picture or a shape in a link that has text beside it is a part of the stretch of the link (Word's PDF: one rectangle over the line for "text <picture> more");
                // one that is the whole of its link has the box that holds it.
                (FlowRunKind::Picture { .. } | FlowRunKind::Shape(_), Some(link)) => {
                    let joins_before = matches!(found.last(), Some((_, end, LinkTo::Link(last))) if *end == at && last == link);
                    let joins_after = p.runs.get(index + 1).is_some_and(|n| n.link.as_ref() == Some(link) && !matches!(n.kind, FlowRunKind::Picture { .. } | FlowRunKind::Shape(_)));
                    if joins_before {
                        if let Some((_, end, _)) = found.last_mut() {
                            *end = at + len;
                        }
                    } else if joins_after {
                        found.push((at, at + len, LinkTo::Link(link.clone())));
                    } else {
                        objects.push((*para, r.kind.clone(), link.clone()));
                    }
                }
                (FlowRunKind::NoteRef { endnote, id }, None) => found.push((at, at + len, LinkTo::Note { endnote: *endnote, id: *id })),
                (_, Some(link)) => match found.last_mut() {
                    Some((_, end, LinkTo::Link(last))) if *end == at && last == link => *end = at + len,
                    _ => found.push((at, at + len, LinkTo::Link(link.clone()))),
                },
                _ => {}
            }
            at += len;
        }
        if !found.is_empty() {
            spans.insert(*para, found);
        }
    }
    // Where `offset` characters into a paragraph is: the page, and the point of the PDF's destination, 3 pixels before the character, in the line it is in (at the line's top,
    // the space before the paragraph's first line included).
    let place_of_para = |para: u32, offset: usize| -> Option<PageLinkTarget> {
        let all: Vec<_> = pagination
            .pages
            .iter()
            .enumerate()
            .flat_map(|(page, pg)| {
                pg.items.iter().filter_map(move |i| match i {
                    Item::Line { para: q, x, y, h, stops, vertical, pad, .. } if *q == para => Some((page, *x, *y, *h, stops, *vertical, *pad)),
                    _ => None,
                })
            })
            .collect();
        // The line the character is in: the first whose last position is past it (the paragraph's last line otherwise).
        let (page, x, y, h, stops, vertical, pad) = *all.iter().find(|l| l.4.last().is_some_and(|&(end, _)| offset < end)).or(all.last())?;
        let along = stops.iter().rev().find(|&&(index, _)| index <= offset).or(stops.first()).map_or(0.0, |s| s.1);
        Some(if vertical { PageLinkTarget::Place { page, x: x + h, y: y + along - LINK_REACH } } else { PageLinkTarget::Place { page, x: x + along - LINK_REACH, y: y - pad.0 } })
    };
    let target_of = |to: &LinkTo| -> Option<PageLinkTarget> {
        match to {
            LinkTo::Link(Link::Url(u)) if !u.is_empty() => Some(PageLinkTarget::Url(u.clone())),
            LinkTo::Link(Link::Anchor(name)) => {
                let (para, offset) = *anchors.get(name.as_str())?;
                place_of_para(para, offset)
            }
            LinkTo::Link(_) => None,
            LinkTo::Note { endnote, id } => {
                let notes = if *endnote { &doc.endnotes } else { &doc.footnotes };
                let first = notes.iter().find(|n| n.id == *id)?.blocks.iter().find_map(|b| match b {
                    ibis_doc_model::flow::Block::Paragraph(p) => Some(p.id),
                    _ => None,
                })?;
                place_of_para(first, 0)
            }
        }
    };
    let mut out: Vec<Vec<PageLinkRect>> = vec![Vec::new(); pagination.pages.len()];
    for (page, pg) in pagination.pages.iter().enumerate() {
        for item in &pg.items {
            let Item::Line { para, x, y, h, stops, vertical, avail, pad, .. } = item else { continue };
            let Some(list) = spans.get(para) else { continue };
            for (start, end, to) in list {
                let Some(target) = target_of(to) else { continue };
                let (mut lo, mut hi) = (f32::MAX, f32::MIN);
                for pair in stops.windows(2) {
                    let ((a, ax), (b, bx)) = (pair[0], pair[1]);
                    if a < b && a >= *start && b <= *end {
                        lo = lo.min(ax.min(bx));
                        hi = hi.max(ax.max(bx));
                    }
                }
                if lo >= hi {
                    continue;
                }
                // A link that goes on in the next line: Word's rectangle takes the line as far as its text can run (the margin, or past it by the space the line ends in), not only
                // what is written on it.
                if stops.last().is_some_and(|&(last, _)| *end > last) {
                    hi = hi.max(*avail);
                }
                // Along the line: 3 pixels out of the first and last characters; on the pixel grid when the line starts on it (the right end of a line that runs to the margin
                // is where the layout put it).
                let origin = if *vertical { *y } else { *x };
                let on_grid = ((origin / LINK_PIXEL) - (origin / LINK_PIXEL).round()).abs() < 0.02;
                let floor_to_grid = |v: f32| ((v / LINK_PIXEL) + 0.001).floor() * LINK_PIXEL;
                let (mut from, mut to_end) = (origin + lo - LINK_REACH, origin + hi + LINK_REACH);
                if on_grid {
                    from = floor_to_grid(from);
                    if hi < *avail - 0.5 {
                        to_end = floor_to_grid(to_end);
                    }
                }
                out[page].push(if *vertical {
                    PageLinkRect { x: *x, y: from, w: *h, h: to_end - from, target }
                } else {
                    PageLinkRect { x: from, y: y - pad.0, w: to_end - from, h: h + pad.0 + pad.1, target }
                });
            }
        }
    }
    // A picture or a shape: the box that holds it. A shape is found by its id (a floating one is not in a line); a picture by the line it stands in.
    // The shapes of a group (`wpg:wgp`) are read as shapes one after another with the link of the group (the `docPr`): Word's PDF has one rectangle over the union of their
    // turned boxes (#227), so shapes next to each other with the same link in a paragraph share one rectangle, by page.
    let mut shared: Option<(u32, Link, std::collections::HashMap<usize, usize>)> = None;
    for (para, kind, link) in &objects {
        // A shape that holds text (a `wps:txbx`, even an empty paragraph or one letter) and is neither turned nor flipped has no link in Word's PDF; with no text, or turned or flipped
        // by any angle, it has the box that holds it (Windows Word, `tasks/doc/requests/2026-11-17-227-word-shape-links-result.md`, 1: 54 pages of 3 shapes x 3 texts x 3 anchors x fill,
        // 36 with text none, 18 without all there; the reason is not known).
        if let FlowRunKind::Shape(shape) = kind {
            let holds_text = match &shape.kind {
                ibis_doc_model::ShapeKind::TextBox(_) => true,
                ibis_doc_model::ShapeKind::Auto(auto) => auto.text.is_some(),
                _ => false,
            };
            if holds_text && shape.rotation == 0 && !shape.flip_h && !shape.flip_v {
                continue;
            }
        }
        let to = LinkTo::Link(link.clone());
        let Some(target) = target_of(&to) else { continue };
        let joins = matches!(kind, FlowRunKind::Shape(_)) && shared.as_ref().is_some_and(|(p, l, _)| p == para && l == link);
        if !joins {
            shared = matches!(kind, FlowRunKind::Shape(_)).then(|| (*para, link.clone(), Default::default()));
        }
        for (page, pg) in pagination.pages.iter().enumerate() {
            for item in &pg.items {
                let rect = match (kind, item) {
                    (FlowRunKind::Shape(shape), Item::Shape { shape: placed, x, y, w, h }) if placed.id == shape.id => {
                        let emu = |v: f32| (v * EMU_PER_POINT as f32).round() as i64;
                        let [l, t, r, b] = placed.link_box(ibis_doc_model::EmuRect::new(emu(*x), emu(*y), emu(*w), emu(*h)));
                        let pt = |v: f64| v as f32 / EMU_PER_POINT as f32;
                        Some((pt(l), pt(t), pt(r - l), pt(b - t)))
                    }
                    (FlowRunKind::Picture { media, .. }, Item::Picture { media: placed, x, y, w, h })
                        if placed == media
                            && pg.items.iter().any(|l| matches!(l, Item::Line { para: q, x: lx, y: ly, h: lh, vertical: false, .. } if q == para && *x + w / 2.0 >= *lx && (*y + h / 2.0 >= *ly - 0.5) && (*y + h / 2.0) <= ly + lh + 0.5)) =>
                    {
                        Some((*x, *y, *w, *h))
                    }
                    _ => None,
                };
                if let Some((x, y, w, h)) = rect {
                    let at = shared.as_ref().and_then(|(_, _, pages)| pages.get(&page).copied());
                    match at {
                        Some(i) if joins => {
                            let r = &mut out[page][i];
                            let (right, bottom) = ((r.x + r.w).max(x + w), (r.y + r.h).max(y + h));
                            (r.x, r.y) = (r.x.min(x), r.y.min(y));
                            (r.w, r.h) = (right - r.x, bottom - r.y);
                        }
                        _ => {
                            out[page].push(PageLinkRect { x, y, w, h, target: target.clone() });
                            if let Some((_, _, pages)) = shared.as_mut() {
                                pages.insert(page, out[page].len() - 1);
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// Give a presentation read from a flowing document one slide per page.
/// `file_name` is shown by `FILENAME` fields.
pub fn expand_pages(presentation: &mut ibis_doc_model::Presentation, res: &Resources, file_name: Option<&str>) {
    let Some(mut doc) = presentation.flow.take() else { return };
    if file_name.is_some() {
        Arc::make_mut(&mut doc).file_name = file_name.map(str::to_string);
    }
    presentation.flow = Some(doc.clone());
    let pagination = pages(&doc, res);
    let count = pagination.pages.len().max(1);
    // The pages are the paper's size (after a page setup, too): each its
    // section's.
    let size_of = |i: usize| {
        pagination.pages.get(i).filter(|p| p.width > 0.0).map_or(doc.page.size, |p| ibis_doc_model::EmuSize::new((p.width * EMU_PER_POINT as f32).round() as i64, (p.height * EMU_PER_POINT as f32).round() as i64))
    };
    presentation.slide_size = size_of(0);
    let mut slides = Vec::with_capacity(count);
    for index in 0..count {
        let id = presentation.new_slide_id();
        let mut slide = ibis_doc_model::Slide::new(id);
        let size = size_of(index);
        if size != presentation.slide_size {
            slide.size = Some(size);
        }
        let shape = presentation.new_shape_id();
        slide.shapes.push(ibis_doc_model::Shape {
            id: shape,
            frame: ibis_doc_model::EmuRect::new(0, 0, size.width, size.height),
            rotation: 0,
            flip_h: false,
            flip_v: false,
            kind: ibis_doc_model::ShapeKind::Page { document: doc.clone(), index },
            floating: None,
        });
        slides.push(slide);
    }
    presentation.slides = slides;
}


/// An underline's distance below the baseline and its thickness (points).
fn underline_band(style: UnderlineStyle, size: f32) -> (f32, f32) {
    use UnderlineStyle::*;
    if style.heavy() || style == Double || style == WavyDouble { (0.0875 * size + 0.25, 0.025 * size + 1.25) } else { (0.1367 * size, 0.0684 * size) }
}

/// The rectangles of an underline `w` long from `x`, under text of `size`
/// points whose baseline is at `baseline` (measured on Word's PDF: a single
/// line 0.05 em + 0.25 pt thick, its top 0.1125 em + 0.5 pt below the
/// baseline; heavy ones 0.025 em + 1.25 pt from 0.0875 em + 0.25 pt).
pub fn underline_items(style: UnderlineStyle, size: f32, x: f32, w: f32, baseline: f32, color: Hsla) -> Vec<Item> {
    underline_items_banded(style, size, x, w, baseline, color, None)
}

/// `underline_items` with the single line's band from the typeface's file when it is known (`band`: top below the baseline, thickness, East Asian typeface), as Word's XPS of
/// GitHub issue #192 has it: the line `thick` thick at `top`; a double one two lines of `thick` and a gap of `thick` (East Asian typefaces) or two of half the thickness (the others,
/// in the band from `top / 2` to `top + thick`); a heavy one twice as thick (East Asian) or from `top / 2` to the double one's bottom.
pub fn underline_items_banded(style: UnderlineStyle, size: f32, x: f32, w: f32, baseline: f32, color: Hsla, band: Option<(f32, f32, bool)>) -> Vec<Item> {
    use UnderlineStyle::*;
    let (top, thick) = underline_band(style, size);
    if matches!(style, Wave | WavyHeavy | WavyDouble) {
        // Word's reference PDF uses roughly 0.24 pt strokes (0.96 pt for
        // heavy), with 1.2 pt half-waves. Flatten smooth cubic half-waves
        // to the existing Stroke primitive so screen and export agree.
        let stroke = if style == WavyHeavy { 0.96 } else { 0.24 };
        let amplitude = if style == WavyDouble { 0.24 } else { 0.72 };
        let bands = if style == WavyDouble { 2 } else { 1 };
        let mut out = Vec::new();
        for band in 0..bands {
            let y0 = baseline + top + stroke / 2.0 + band as f32 * (0.024 * size + 0.24);
            let mut a = 0.0;
            let mut half = 0;
            while a < w {
                let (from, to) = if half % 2 == 0 { (0.0, amplitude) } else { (amplitude, 0.0) };
                let mut prev = (a, from);
                for step in 1..=8 {
                    let t = step as f32 / 8.0;
                    let u = 1.0 - t;
                    let dx = 3.0*u*u*t*0.48 + 3.0*u*t*t*0.48 + t*t*t*1.2;
                    let dy = u*u*u*from + 3.0*u*u*t*from + 3.0*u*t*t*to + t*t*t*to;
                    let end = (a + dx).min(w);
                    let fraction = if a + dx > w { (w-prev.0)/(a+dx-prev.0) } else { 1.0 };
                    let y = prev.1 + (dy-prev.1)*fraction;
                    out.push(Item::Stroke { x: x+prev.0, y: y0+prev.1, x2: x+end, y2: y0+y, w: stroke, color });
                    prev = (a+dx, dy);
                    if end >= w { break; }
                }
                a += 1.2;
                half += 1;
            }
        }
        return out;
    }
    // Bands across: (offset from `top`, thickness).
    let mut top = top;
    let bands: Vec<(f32, f32)> = match (style, band) {
        (Double, Some((t0, t, true))) => {
            top = t0;
            vec![(0.0, t), (2.0 * t, t)]
        }
        (Double, Some((t0, t, false))) => {
            top = t0 / 2.0;
            vec![(0.0, t / 2.0), (t0 / 2.0 + t / 2.0, t / 2.0)]
        }
        (Double, None) => {
            let t = 0.025 * size + 0.25;
            vec![(0.0, t), (t + 1.0, t)]
        }
        (_, Some((t0, t, true))) => {
            top = t0;
            vec![(0.0, if style.heavy() { 2.0 * t } else { t })]
        }
        (_, Some((t0, t, false))) => {
            if style.heavy() {
                top = t0 / 2.0;
                vec![(0.0, (t0 + t) / 2.0)]
            } else {
                top = t0;
                vec![(0.0, t)]
            }
        }
        _ => vec![(0.0, thick)],
    };
    let mut out = Vec::new();
    for (dy, t) in bands {
        let y = baseline + top + dy;
        match style.dashes() {
            Some(pattern) => {
                let (mut a, mut i) = (x, 0);
                while a < x + w {
                    let len = pattern[i % pattern.len()];
                    if i % 2 == 0 {
                        out.push(Item::Rect { x: a, y, w: len.min(x + w - a), h: t, color });
                    }
                    a += len;
                    i += 1;
                }
            }
            None => out.push(Item::Rect { x, y, w, h: t, color }),
        }
    }
    out
}


/// The items drawing a shading: background fill and two-color pattern
/// lines/stripes across rectangle `(x, y, w, h)`.
fn shading_items(
    pattern: Option<ShadingPattern>,
    fill: Option<Rgb>,
    pat_color: Option<Rgb>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Vec<Item> {
    let mut items = Vec::new();
    if fill.is_none() && pattern.is_none() {
        return items;
    }
    // 1. Background fill
    if let Some(f) = fill {
        items.push(Item::Rect { x, y, w, h, color: hsla(ibis_doc_model::Color::opaque(f)) });
    }
    // 2. Pattern placeholder (expanded into absolute page coordinates before rendering)
    if let Some(pat) = pattern {
        let c = pat_color.unwrap_or(Rgb::BLACK);
        items.push(Item::Shading {
            pattern: pat,
            color: hsla(ibis_doc_model::Color::opaque(c)),
            x,
            y,
            w,
            h,
        });
    }
    items
}

/// Expand two-color shading pattern across absolute page rectangle `(x, y, w, h)`.
///
/// Word's own pattern (read from its PDFs of the coverage tests DCEL and
/// DCMB): a tiling pattern of 8 × 8 pixels of 0.24 pt (300 dpi), so it
/// repeats every 1.92 pt, on a grid of 1.92 pt from the page's top left
/// corner (the tile holding the rectangle's top left corner). Drawn here
/// every 2 pt instead: the reference PNGs of the tests are made by pdftoppm
/// at 144 dpi, which draws that tile every 4 px (2 pt), and drawing every
/// 1.92 pt made more of them differ (tried 2026-10-05: 5 newly passing,
/// 33 newly failing). Where pdftoppm starts the stripes does not follow a
/// simple rule from the tile's origin, so some cells still differ in phase
/// (DCMB-01342 and 9 others); that is not copied here.
fn pattern_items(
    pat: ShadingPattern,
    pcolor: Hsla,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Vec<Item> {
    use ShadingPattern::*;
    let mut items = Vec::new();
    let step = 2.0_f32; // 4px at 144 DPI
    match pat {
        ThinHorzStripe => {
            // In Word 144 DPI: thinHorzStripe covers y%4 == 3 (height 0.5pt spanning [-0.5, 0.0] before 2.0k).
            let k_start = ((y + 0.5) / step).floor() as i64;
            let mut k = k_start;
            loop {
                let y_stroke = k as f32 * step - 0.5;
                if y_stroke >= y + h {
                    break;
                }
                let top = y_stroke.max(y);
                let bottom = (y_stroke + 0.5).min(y + h);
                if bottom > top {
                    items.push(Item::Rect { x, y: top, w, h: bottom - top, color: pcolor });
                }
                k += 1;
            }
        }
        HorzStripe => {
            let mut y0 = y;
            while y0 < y + h {
                let lh = 1.0_f32.min(y + h - y0);
                items.push(Item::Rect { x, y: y0, w, h: lh, color: pcolor });
                y0 += step;
            }
        }
        ThinVertStripe => {
            // In Word 144 DPI: thinVertStripe covers x%4 == 0 (width 0.5pt spanning [0.0, 0.5] after 2.0k).
            let k_start = (x / step).floor() as i64;
            let mut k = k_start;
            loop {
                let x_stroke = k as f32 * step;
                if x_stroke >= x + w {
                    break;
                }
                let left = x_stroke.max(x);
                let right = (x_stroke + 0.5).min(x + w);
                if right > left {
                    items.push(Item::Rect { x: left, y, w: right - left, h, color: pcolor });
                }
                k += 1;
            }
        }
        VertStripe => {
            let mut x0 = x;
            while x0 < x + w {
                let lw = 1.0_f32.min(x + w - x0);
                items.push(Item::Rect { x: x0, y, w: lw, h, color: pcolor });
                x0 += step;
            }
        }
        ThinHorzCross => {
            // Horizontal stroke at y%4 == 1 (k*step + 0.5) and vertical stroke at x%4 == 3 (k*step + 1.5).
            let k_y_start = ((y - 0.5) / step).floor() as i64;
            let mut ky = k_y_start;
            loop {
                let y_stroke = ky as f32 * step + 0.5;
                if y_stroke >= y + h { break; }
                let top = y_stroke.max(y);
                let bottom = (y_stroke + 0.5).min(y + h);
                if bottom > top {
                    items.push(Item::Rect { x, y: top, w, h: bottom - top, color: pcolor });
                }
                ky += 1;
            }
            let k_x_start = ((x - 1.5) / step).floor() as i64;
            let mut kx = k_x_start;
            loop {
                let x_stroke = kx as f32 * step + 1.5;
                if x_stroke >= x + w { break; }
                let left = x_stroke.max(x);
                let right = (x_stroke + 0.5).min(x + w);
                if right > left {
                    items.push(Item::Rect { x: left, y, w: right - left, h, color: pcolor });
                }
                kx += 1;
            }
        }
        HorzCross => {
            items.extend(pattern_items(HorzStripe, pcolor, x, y, w, h));
            items.extend(pattern_items(VertStripe, pcolor, x, y, w, h));
        }
        // `\` from top left to bottom right: reverseDiagStripe (Word's PDF:
        // row r of its tile dark at columns r..r+3).
        ReverseDiagStripe | ThinReverseDiagStripe => {
            let thin = matches!(pat, ThinReverseDiagStripe);
            let diag_stroke: f32 = if thin { 0.36 } else { 0.71 };
            let mut d = -h;
            while d <= w {
                let (x1, y1) = if d >= 0.0 { (x + d, y) } else { (x, y - d) };
                let (x2, y2) = if d + h <= w { (x + d + h, y + h) } else { (x + w, y + w - d) };
                if (x2 - x1).abs() > 0.01 || (y2 - y1).abs() > 0.01 {
                    items.push(Item::Stroke { x: x1, y: y1, x2, y2, w: diag_stroke, color: pcolor });
                }
                d += step;
            }
        }
        // `/` from bottom left to top right: diagStripe (Word's PDF: row r of
        // its tile dark at columns 4−r..7−r).
        DiagStripe | ThinDiagStripe => {
            let thin = matches!(pat, ThinDiagStripe);
            let diag_stroke: f32 = if thin { 0.36 } else { 0.71 };
            let mut d = 0.0;
            while d <= w + h {
                let (x1, y1) = if d <= h { (x, y + d) } else { (x + d - h, y + h) };
                let (x2, y2) = if d <= w { (x + d, y) } else { (x + w, y + d - w) };
                if (x2 - x1).abs() > 0.01 || (y2 - y1).abs() > 0.01 {
                    items.push(Item::Stroke { x: x1, y: y1, x2, y2, w: diag_stroke, color: pcolor });
                }
                d += step;
            }
        }
        DiagCross | ThinDiagCross => {
            let thin = matches!(pat, ThinDiagCross);
            let dpat = if thin { ThinDiagStripe } else { DiagStripe };
            let rpat = if thin { ThinReverseDiagStripe } else { ReverseDiagStripe };
            items.extend(pattern_items(dpat, pcolor, x, y, w, h));
            items.extend(pattern_items(rpat, pcolor, x, y, w, h));
        }
    }
    items
}

/// The cell of a row that ends where column `col` starts.
fn cells_left_of(cells: &[impl HasSpan], col: usize) -> Option<usize> {
    cells.iter().position(|l| l.col_end() == col)
}

trait HasSpan {
    fn col_end(&self) -> usize;
}

/// A border's weight where two meet (ECMA-376 §17.4.66): its width in
/// eighths of a point times the style's number in `ST_Border`.
fn weight(b: Option<FlowBorder>) -> i64 {
    use ibis_doc_model::BorderStyle::*;
    let Some(b) = b else { return 0 };
    let number = match b.style {
        Single => 1,
        Thick => 2,
        Double => 3,
        Dotted => 4,
        Dashed => 5,
        DotDash => 6,
        DotDotDash => 7,
        Triple => 8,
        ThinThickSmallGap => 9,
        ThickThinSmallGap => 10,
        ThinThickThinSmallGap => 11,
        ThinThickMediumGap => 12,
        ThickThinMediumGap => 13,
        ThinThickThinMediumGap => 14,
        ThinThickLargeGap => 15,
        ThickThinLargeGap => 16,
        ThinThickThinLargeGap => 17,
        Wave => 18,
        DoubleWave => 19,
        DashSmallGap => 20,
        DashDotStroked => 21,
        ThreeDEmboss => 22,
        ThreeDEngrave => 23,
        Outset => 24,
        Inset => 25,
    };
    (b.width * 8 / 12_700).max(1) * number
}


/// A section of vertical writing (縦書き, `w:textDirection` `tbRl`; or `btLr`)
/// is laid out as across text on a page turned a quarter (its width the
/// paper's height, its margins moved round with it), then each page's items
/// are turned back into the paper `(w, h)` (points): the characters of
/// `tbRl` upright when Japanese and turned clockwise otherwise (as a
/// vertical cell's text, `turn_cell_text`), those of `btLr` all turned
/// anticlockwise.
fn turn_page_items(items: Vec<Item>, direction: ibis_doc_model::CellDirection, w: f32, h: f32, advance: &dyn Fn(char, &Font, f32) -> f32) -> Vec<Item> {
    use ibis_doc_model::CellDirection;
    let tb = direction == CellDirection::TbRl;
    // A vertical page sets its Japanese upright, as a `tbRlV` cell does (a `tbRl` cell lays it down).
    let page_as_cell = if tb { CellDirection::TbRlV } else { direction };
    // A point of the across page in the paper.
    let at = |x: f32, y: f32| if tb { (w - y, x) } else { (y, h - x) };
    // A rectangle's corner nearest the paper's top-left, and its size.
    let rect = |x: f32, y: f32, rw: f32, rh: f32| {
        let (ax, ay) = at(x, y);
        let (bx, by) = at(x + rw, y + rh);
        (ax.min(bx), ay.min(by), rh, rw)
    };
    // The lines of the across page: (start along, across top, drawn baseline from the top, thickness, end along, largest size). The main text of a line is what stands on its baseline
    // (a ruby or an emphasis mark does not).
    let lines: Vec<(f32, f32, f32, f32, f32)> = items.iter().filter_map(|it| if let Item::Line { x, y, base, stops, largest, .. } = it { Some((*x, *y + *base, *x + stops.last().map_or(0.0, |s| s.1), *largest, *base)) } else { None }).collect();
    let largest = |u: f32, v: f32| lines.iter().find(|(lx, base_at, end, l, base)| *l > 0.0 && *base > 0.0 && (v - base_at).abs() < 0.05 && u >= lx - 0.01 && u <= end + 0.5).map(|line| line.3);
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        match it {
            // A squeezed character (a glyph wider than its cell, as a bold ＭＳ 明朝's, or `w:w`) is turned as the others are, upright at its
            // cell's size: left as it was, it stayed across at the page's top left (GitHub issue #54, the bold ＭＳ headings of a `tbRl` page).
            Item::Text { .. } | Item::Scaled { .. } => out.extend(turn_cell_text_in_lines(std::slice::from_ref(&it), &largest, page_as_cell, CellAnchor::Top, (0.0, 0.0, w, h), w, advance)),
            Item::Rect { x, y, w: rw, h: rh, color } => {
                let (x, y, w, h) = rect(x, y, rw, rh);
                out.push(Item::Rect { x, y, w, h, color });
            }
            Item::Stroke { x, y, x2, y2, w: sw, color } => {
                let ((x, y), (x2, y2)) = (at(x, y), at(x2, y2));
                out.push(Item::Stroke { x, y, x2, y2, w: sw, color });
            }
            Item::Picture { media, x, y, w: pw, h: ph } => {
                let (cx, cy) = at(x + pw / 2.0, y + ph / 2.0);
                out.push(Item::Picture { media, x: cx - pw / 2.0, y: cy - ph / 2.0, w: pw, h: ph });
            }
            Item::Shape { shape, x, y, w: sw, h: sh } => {
                let (cx, cy) = at(x + sw / 2.0, y + sh / 2.0);
                out.push(Item::Shape { shape, x: cx - sw / 2.0, y: cy - sh / 2.0, w: sw, h: sh });
            }
            Item::Shading { pattern, color, x, y, w: sw, h: sh } => {
                let (x, y, w, h) = rect(x, y, sw, sh);
                out.push(Item::Shading { pattern, color, x, y, w, h });
            }
            Item::Turned { x, y, text, font, size, color, turns, xscale } => {
                let (ax, ay) = at(x, y);
                let new_turns = if tb { turns + 1 } else { turns - 1 };
                if new_turns == 0 && (xscale - 1.0).abs() > 0.001 {
                    // 縦中横's squeezed digits (`w:w`), one item each.
                    out.push(Item::Scaled { x: ax, y: ay, text, font, size, color, xscale });
                } else if new_turns == 0 {
                    out.push(Item::Text { x: ax, y: ay, text, offsets: Arc::from([0.0f32]), font, size, color });
                } else {
                    out.push(Item::Turned { x: ax, y: ay, text, font, size, color, turns: new_turns, xscale });
                }
            }
            Item::Line { para, x, y, h: lh, stops, avail, list_tab, largest, .. } => {
                if tb {
                    out.push(Item::Line { para, x: w - y - lh, y: x, h: lh, stops, vertical: true, list_tab, base: 0.0, avail, pad: (0.0, 0.0), largest });
                } else {
                    out.push(Item::Line { para, x: y, y: h - x - lh, h: lh, stops, vertical: false, list_tab, base: 0.0, avail, pad: (0.0, 0.0), largest });
                }
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod turn_japanese_tests {
    use super::{Item, turn_japanese};
    use std::sync::Arc;

    // `lrTbV` turns its Japanese characters left, including the ones squeezed one by one (bold fixed-width font, `w:w`, `fitText`).
    #[test]
    fn a_squeezed_japanese_character_turns_like_the_others() {
        let scaled = |text: &str| Item::Scaled { x: 10.0, y: 20.0, text: text.to_string().into(), font: gpui_kit::font("Test"), size: 10.0, color: gpui_kit::black(), xscale: 0.5 };
        let plain = Item::Text { x: 30.0, y: 20.0, text: "和".into(), offsets: Arc::from([0.0f32]), font: gpui_kit::font("Test"), size: 10.0, color: gpui_kit::black() };
        let out = turn_japanese(vec![scaled("和"), scaled("A"), plain].into_iter());
        let turned: Vec<_> = out.iter().filter(|i| matches!(i, Item::Turned { turns: -1, .. })).collect();
        assert_eq!(turned.len(), 2, "both Japanese characters turn");
        assert!(matches!(&out[0], Item::Turned { x, y, .. } if (*x - 18.8).abs() < 1e-4 && (*y - 21.2).abs() < 1e-4), "the em box where the upright one's was");
        assert!(out.iter().any(|i| matches!(i, Item::Scaled { text, .. } if text.as_ref() == "A")), "a Latin letter stays across");
    }

    // A squeezed character of a vertical page (a fixed-width font's Å wider than its half cell) is turned with the
    // page like the others; left as it was it was drawn across the page, apart from its column (issue 54).
    #[test]
    fn a_squeezed_character_of_a_vertical_page_turns_with_the_page() {
        let scaled = Item::Scaled { x: 10.0, y: 20.0, text: "Å".into(), font: gpui_kit::font("Test"), size: 14.0, color: gpui_kit::black(), xscale: 0.5 };
        let out = super::turn_page_items(vec![scaled], ibis_doc_model::CellDirection::TbRl, 800.0, 600.0, &|_, _, size| size);
        assert!(matches!(&out[..], [Item::Turned { xscale, .. }] if (*xscale - 0.5).abs() < 1e-6), "{out:?}");
    }
    // Hiragino sets ‘ ’ “ ” proportionally (0.29 and 0.49 em): upright in a vertical line, Word centres the advance on the column (the user's Mac Word
    // with 'ヒラギノ明朝 ProN' / 'ヒラギノ角ゴ ProN'), where the glyph set at the em box's left edge stood left of it.
    #[test]
    fn a_narrow_upright_character_of_a_vertical_page_is_centred_on_the_column() {
        let text = |c: &str| Item::Text { x: 10.0, y: 20.0, text: c.to_string().into(), offsets: Arc::from([0.0f32]), font: gpui_kit::font("Test"), size: 20.0, color: gpui_kit::black() };
        let place = |c: &str, advance: f32| match &super::turn_page_items(vec![text(c)], ibis_doc_model::CellDirection::TbRl, 800.0, 600.0, &move |_, _, _| advance)[..] {
            [Item::Text { x, .. }] => *x,
            other => panic!("{other:?}"),
        };
        let full = place("\u{201c}", 20.0);
        assert!((place("\u{201c}", 9.8) - full - 5.1).abs() < 1e-4, "half an em less the advance, each side");
        assert!((place("\u{2019}", 5.8) - full - 7.1).abs() < 1e-4);
        assert!((place("\u{3042}", 20.0) - full).abs() < 1e-4, "a full-width character stays where it was");
    }
}

#[cfg(test)]
mod table_border_thickness_tests {
    use super::{Item, border_items};
    use ibis_doc_model::{BorderStyle, FlowBorder, Rgb};

    fn thickness(sz: i64, table: bool) -> f32 {
        // `w:sz` is in eighths of a point: 1/8 pt = 1587.5 EMU.
        let b = FlowBorder { width: sz * 12_700 / 8, color: Rgb(0), style: BorderStyle::Single, space: 0, art: None };
        match border_items(&b, true, true, 10.0, 0.0, 100.0, false, false, table).as_slice() {
            [Item::Rect { h, .. }] => *h,
            other => panic!("one rectangle: {}", other.len()),
        }
    }

    /// A table's border is a rectangle a whole number of the printer's pixels (1/600 inch = 0.12 pt), at least one, thick (Word's XPS of tables, issue #189:
    /// `w:sz` 0 and 1 give 0.12, 2 gives 0.24, 4 gives 0.48, 8 gives 0.96); a paragraph's keeps its own width.
    #[test]
    fn a_table_border_is_a_whole_number_of_printer_pixels_thick() {
        for (sz, want) in [(0, 0.12f32), (1, 0.12), (2, 0.24), (3, 0.24), (4, 0.48), (5, 0.60), (6, 0.72), (7, 0.84), (8, 0.96)] {
            assert!((thickness(sz, true) - want).abs() < 1e-4, "w:sz {sz}: {}", thickness(sz, true));
        }
        assert!((thickness(8, false) - 1.0).abs() < 1e-4);
    }
}

#[cfg(test)]
mod ms_room_tests {
    use super::ms_room_em;

    /// Windows Word, 2026-11-14: a line of ＭＳ 明朝 / ＭＳ ゴシック is 332/256 em (10.372 pt at 8, 13.612 at 10.5, 18.167 at 14, 31.127 at 24, 93.375 at 72).
    #[test]
    fn a_line_of_ms_is_332_of_256_em() {
        for (size, word) in [(8.0f32, 10.372f32), (10.5, 13.612), (14.0, 18.167), (24.0, 31.127), (72.0, 93.375)] {
            let line = (1.0 + ms_room_em()) * size;
            assert!((line - word).abs() < 0.03, "{size} pt: {line} against Word's {word}");
        }
    }
}
