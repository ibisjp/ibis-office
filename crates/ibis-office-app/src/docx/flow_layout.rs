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

use std::sync::Arc;

use gpui_kit::*;
use ibis_doc_model::{
    Block, CellAnchor, PreferredWidth, EMU_PER_POINT, Emu, FlowAlign, FlowBorder, FlowDocument, FlowParagraph, FlowRunKind, FlowRunStyle,
    FlowTable, LineHeight, ListSuffix, MediaId, Rgb, RowHeight, SectionBreak, ShadingPattern, TabAlign, TextStyle, UnderlineStyle, VerticalMerge, VerticalPosition,
};

use crate::docx::art_border::{self, ArtPart};
use crate::render::shape_view::{Resources, hsla};

/// Something drawn on a page, in points from the page's top-left corner.
#[derive(Clone, Debug)]
pub(crate) enum Item {
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
    Line { para: u32, x: f32, y: f32, h: f32, stops: Arc<[(usize, f32)]>, vertical: bool },
    /// A two-color shading pattern across `(x, y, w, h)`, expanded into
    /// absolute page coordinates before rendering.
    Shading { pattern: ShadingPattern, color: Hsla, x: f32, y: f32, w: f32, h: f32 },
    /// A comment speech-bubble icon drawn outside the right edge of the paper.
    CommentBubble { x: f32, y: f32, size: f32 },
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Page {
    pub(crate) items: Vec<Item>,
    /// Number of items that belong to the document body (before headers and footers).
    pub(crate) body_items_count: usize,
    /// The paper's size (points), its section's.
    pub(crate) width: f32,
    pub(crate) height: f32,
    /// The section the page is in (its index), its page number, and whether
    /// it is the section's first page.
    pub(crate) section: usize,
    pub(crate) number: u32,
    pub(crate) first_in_section: bool,
    /// The footnotes whose marks are on the page (drawn at its bottom).
    pub(crate) notes: Vec<u32>,
}

/// The pages of a document.
#[derive(Debug, Default)]
pub(crate) struct Pagination {
    pub(crate) pages: Vec<Page>,
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


/// Tracked changes display mode (Word's Display for Review: Simple, All, None, Original).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub(crate) enum MarkupDisplayMode {
    #[default]
    All = 0,        // すべての変更履歴 (All Markup)
    Simple = 1,     // シンプルな変更履歴 (Simple Markup)
    None = 2,       // 変更履歴なし (No Markup / 最終版)
    Original = 3,   // 元の文書 (Original / 原文)
}

pub(crate) fn clear_layout_caches() {
    PIECES.with(|c| c.borrow_mut().clear());
    PAGINATIONS.with(|c| c.borrow_mut().clear());
}

/// What one window shows of tracked changes and hidden text: the markup mode, whether hidden text is shown, and the reviewers
/// whose changes are shown (`None`: all). It belongs to the window's own `Resources` (`Resources::markup_view`), not to the
/// process: two documents open at once each keep their own. It is part of the layout caches' keys, so a change of it lays
/// the pages out again by itself.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub(crate) struct MarkupView {
    pub(crate) mode: MarkupDisplayMode,
    pub(crate) show_hidden_text: bool,
    pub(crate) shown_reviewers: Option<Arc<std::collections::BTreeSet<String>>>,
}

impl MarkupView {
    /// Whether tracked changes are drawn (すべての変更履歴 or シンプルな変更履歴).
    pub(crate) fn showing_markup(&self) -> bool {
        matches!(self.mode, MarkupDisplayMode::All | MarkupDisplayMode::Simple)
    }

    pub(crate) fn is_reviewer_shown(&self, author: &str) -> bool {
        self.shown_reviewers.as_ref().map_or(true, |set| set.contains(author))
    }
}

/// The colour tracked changes are shown in (Word's first author's).
const REVISION_COLOR: ibis_doc_model::Rgb = ibis_doc_model::Rgb(0xC00000);
/// The colour tracked moves are shown in (Word's standard green).
pub(crate) const MOVE_REVISION_COLOR: ibis_doc_model::Rgb = ibis_doc_model::Rgb(0x008000);

/// The highlight colour for commented text ranges (Word-compatible pale amber/yellow).
pub(crate) const COMMENT_HIGHLIGHT_COLOR: ibis_doc_model::Color = ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0xfff2ac), alpha: 180 };
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

/// ＭＳ Ｐ明朝・ＭＳ Ｐゴシック (the proportional ones).
fn proportional_ms(name: &str) -> bool {
    matches!(name, "ＭＳ Ｐ明朝" | "ＭＳ Ｐゴシック" | "MS PMincho" | "MS PGothic" | "ＭＳＰ明朝" | "ＭＳＰゴシック") // i18n-ok（フォント名の判定（プロポーショナルフォント）。訳さない）
}

/// A character's advance in ＭＳ Ｐ明朝 / ＭＳ Ｐゴシック, as Word's PDFs show it: kana and kanji a full em, the stops
/// 0.664 em, brackets and letters about half an em (ＭＳ Ｐ明朝「（」0.5, ＭＳ Ｐゴシック o 0.508).
fn proportional_ms_width(c: char, size: f32) -> f32 {
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

/// The advance (em) of `c` in typeface `font` (`None`: the default), as this layout sets it: a fixed-width Japanese face is half
/// or one em, a stand-in for an uninstalled ＭＳ Ｐ face has Word's widths, any other face its own glyph. It is what 縦中横's
/// 行の幅に合わせる (`ibis_doc_model::tate_chu_yoko_fit_scale`) measures a run by.
pub(crate) fn advance_em(res: &Resources, font: Option<&str>, c: char) -> f32 {
    const EM: f32 = 100.0;
    match font {
        Some(n) if fixed_width(n) => if full_width(c) { 1.0 } else { 0.5 },
        Some(n) if proportional_ms(n) && !res.has_typeface(n) => proportional_ms_width(c, EM) / EM,
        _ => {
            let f = res.font(&TextStyle { font_size: (EM * 100.0) as u32, font: font.map(str::to_string), ..TextStyle::default() });
            res.text_width(&c.to_string(), &f, EM) / EM
        }
    }
}

/// A character drawn full-width in a Japanese fixed-width font.
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

/// One unit of a paragraph's text.
#[derive(Clone, Debug)]
enum Unit {
    /// `inset` centres the glyph in a fixed-width cell it does not fill.
    /// `squeezed`: the half em of a stop or bracket was already taken off its width (a pair of them kerned, or a line that took it in).
    /// `line` and `ascent` are the height a line of the character's typeface
    /// takes and its ascent (points), as Word measures them.
    Char { c: char, width: f32, inset: f32, font: Font, size: f32, style: usize, line: f32, ascent: f32, lead: f32, auto_space: f32, squeeze: f32, squeezed: bool },
    Tab,
    PositionalTab { align: TabAlign, relative_to_margin: bool, leader: bool },
    /// The tab after a list item's number: to the hanging indent (the text's
    /// left edge) when that is ahead, else as a tab.
    ListTab,
    Picture { media: MediaId, w: f32, h: f32 },
    Shape { shape: Box<ibis_doc_model::Shape>, w: f32, h: f32 },
    /// A typeset formula (`w`, `h` its width and height, `descent` the part of `h` below its baseline).
    Math { boxed: Arc<crate::render::math_view::MathBox>, w: f32, h: f32, descent: f32, font: Font, color: Hsla, display: bool },
    LineBreak,
    PageBreak,
    ColumnBreak,
}

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
    drop_cap_to_emit: Option<Item>,
}

/// The shapes of a paragraph that the text wraps round (a paragraph can anchor more than one).
fn square_shapes(p: &FlowParagraph) -> impl Iterator<Item = &ibis_doc_model::Shape> {
    p.runs.iter().filter_map(|r| match &r.kind {
        FlowRunKind::Shape(shape) if shape.floating.as_ref().is_some_and(|f| matches!(f.wrap.as_str(), "square" | "tight" | "through")) => Some(&**shape),
        _ => None,
    })
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

/// Where a floating shape's left edge is on the page, from its anchor's horizontal position (the one place this is worked out: the text round it, the
/// obstacle it leaves on the page and the drawing of it all take it from here).
fn float_x(f: &ibis_doc_model::FloatingAnchor, sw: f32, g: &FloatGeometry) -> f32 {
    let (base_x0, base_w) = match f.relative_h.as_str() {
        "page" => (0.0, g.page_w),
        "leftMargin" => (0.0, g.margin_l),
        "rightMargin" => (g.page_w - g.margin_r, g.margin_r),
        "insideMargin" => if g.even { (g.page_w - g.margin_r, g.margin_r) } else { (0.0, g.margin_l) },
        "outsideMargin" => if g.even { (0.0, g.margin_l) } else { (g.page_w - g.margin_r, g.margin_r) },
        _ /* "column" | "margin" */ => (g.margin_l, g.content_w),
    };
    let abs_x = if f.relative_h == "character" {
        match f.align_h.as_deref() {
            Some("center") => g.margin_l - sw / 2.0,
            Some("right") => g.margin_l - sw,
            Some("left") => g.margin_l,
            _ => g.margin_l + pt(f.offset_x),
        }
    } else {
        match f.align_h.as_deref() {
            Some("left") => base_x0,
            Some("center") => base_x0 + (base_w - sw) / 2.0,
            Some("right") => base_x0 + base_w - sw,
            Some("inside") => if g.even { base_x0 + base_w - sw } else { base_x0 },
            Some("outside") => if g.even { base_x0 } else { base_x0 + base_w - sw },
            _ => base_x0 + pt(f.offset_x),
        }
    };
    abs_x.clamp(0.0, (g.page_w - sw).max(0.0))
}

/// Where a floating shape's top is on the page, from its anchor's vertical position: `para_top` is the top of its paragraph, `line_h` the height of
/// its line (what `paragraph` and `line` mean).
fn float_y(f: &ibis_doc_model::FloatingAnchor, sh: f32, g: &FloatGeometry, para_top: f32, line_h: f32) -> f32 {
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
    match f.align_v.as_deref() {
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
            drop_cap_to_emit: None,
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
            drop_cap_to_emit: None,
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
fn label_font_name(style: &FlowRunStyle, c: char) -> Option<&str> {
    style.font.as_deref().or_else(|| if matches!(c as u32, 0x2022 | 0x25A0..=0x25FF) { Some("Arial") } else { style.font_for(c) })
}
#[derive(Clone, Copy)]
struct UnderlineBox {
    style: UnderlineStyle,
    size: f32,
    x: f32,
    w: f32,
    baseline: f32,
    color: Hsla,
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
        let mut out = border_items(b, true, true, y_top + half, x_left, x_right, true, true);
        out.extend(border_items(b, true, false, y_bottom - half, x_left, x_right, true, true));
        out.extend(border_items(b, false, true, x_left + half, y_top, y_bottom, true, true));
        out.extend(border_items(b, false, false, x_right - half, y_top, y_bottom, true, true));
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
    fn finish(self, line: &mut LineBox) {
        let mut b_items = self.highlights;
        for u in self.underlines {
            b_items.extend(underline_items(u.style, u.size, u.x, u.w, u.baseline, u.color));
        }
        for b in self.borders {
            let extra_bw = (pt(b.border.total()) - pt(b.border.width)).max(0.0);
            let pad_h = pt(b.border.space) + 0.75 + extra_bw;
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
        for page in &mut pagination.pages {
            for item in &mut page.items {
                let Item::Shape { shape, .. } = item else { continue };
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
                        let lead = if paragraph.no_snap_to_grid { self.lead_em(name) } else { Self::grid_lead_em(name) } * size;
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
    fn float_box<'a>(&self, shape: &'a ibis_doc_model::Shape, g: &FloatGeometry, para_top: f32, line_h: f32) -> FloatBox<'a> {
        let f = shape.floating.as_ref().expect("a floating shape");
        let sw = pt(shape.frame.width);
        let sh = self.fitted_shape_height(shape);
        let abs_x = float_x(f, sw, g);
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
                        Some("center") => margin_l + x - sw / 2.0,
                        Some("right") => margin_l + x - sw,
                        Some("left") => margin_l + x,
                        _ => margin_l + x + pt(f.offset_x),
                    }
                } else {
                    match f.align_h.as_deref() {
                        Some("left") => base_x0,
                        Some("center") => base_x0 + (base_w - sw) / 2.0,
                        Some("right") => base_x0 + base_w - sw,
                        Some("inside") => if is_even { base_x0 + base_w - sw } else { base_x0 },
                        Some("outside") => if is_even { base_x0 } else { base_x0 + base_w - sw },
                        _ => base_x0 + pt(f.offset_x),
                    }
                };
                // A box anchored in a table cell with its place counted from the column (`layoutInCell`) is placed from
                // the cell's left edge, whatever the offset's sign (it may lie to the left of the cell, past the page).
                let in_cell = f.layout_in_cell && self.current_cell.get().is_some() && f.align_h.is_none() && matches!(f.relative_h.as_str(), "column" | "margin");
                let abs_x = if in_cell { margin_l + pt(f.offset_x) } else { abs_x.clamp(0.0, (page_w - sw).max(0.0)) };
                let sx = abs_x - margin_l;
                let page_h = pt(self.doc.page.size.height);
                let margin_t = pt(self.doc.page.top);
                let margin_b = pt(self.doc.page.bottom);
                let sy = if f.relative_v == "line" {
                    match f.align_v.as_deref() {
                        Some("center") => (line.height - sh) / 2.0,
                        Some("bottom") => line.height - sh,
                        Some("top") => 0.0,
                        _ => pt(f.offset_y),
                    }
                } else if f.relative_v == "paragraph" {
                    match f.align_v.as_deref() {
                        Some("center") => (line.height - sh) / 2.0,
                        Some("bottom") => line.height - sh,
                        Some("top") => 0.0,
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
                    let abs_y = match f.align_v.as_deref() {
                        Some("top") => base_y0,
                        Some("center") => if base_h > 0.0 { base_y0 + (base_h - sh) / 2.0 } else { base_y0 },
                        Some("bottom") => if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 },
                        Some("inside") => if is_even { if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 } } else { base_y0 },
                        Some("outside") => if is_even { base_y0 } else { if base_h > 0.0 { base_y0 + base_h - sh } else { base_y0 } },
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

    /// A line's height with a picture (or a box) in it: a line spacing of several lines (`w:lineRule="auto"`
    /// above single) multiplies the text's line, not the picture (Word's PDF of a 3-line spacing around a
    /// 45 pt inline box: the box's height, not three times it).
    fn line_height_around(&self, p: &FlowParagraph, text_line: f32, picture: f32, natural: f32, lead: f32, in_table: bool) -> f32 {
        if picture > text_line && matches!(p.line, LineHeight::Auto(m) if m > 240) {
            return self.resolve_line_height(p, text_line, 0.0, in_table).max(picture);
        }
        self.resolve_line_height(p, natural, lead, in_table)
    }

    fn resolve_line_height(&self, p: &FlowParagraph, base: f32, lead: f32, in_table: bool) -> f32 {
        self.line_height_parts(p, base, lead, in_table).0
    }

    /// A line's height, and the room of it the line grid adds (`w:docGrid`
    /// `type="lines"`: the height rounded up to whole `linePitch`es).
    fn line_height_parts(&self, p: &FlowParagraph, base: f32, lead: f32, in_table: bool) -> (f32, f32) {
        // In a table's cell the lines are on the grid only with `adjustLineHeightInTable`.
        // Word 2003-era files (compatibility mode below 15) line the cells' lines up on the grid as well.
        let in_table = in_table && !self.doc.adjust_line_height_in_table && self.doc.compat_mode.is_none_or(|m| m >= 15);
        let snapping = !in_table && p.snap_to_grid && self.line_pitch.get().is_some_and(|pitch| pitch > 0) && !matches!(p.line, LineHeight::Exact(_) | LineHeight::AtLeast(0));
        // `lead`: the room below the text off a grid (see `leading`).
        let base = if snapping { base } else { base + lead };
        let h = match p.line {
            LineHeight::Exact(exact) => return (pt(exact), 0.0),
            LineHeight::AtLeast(min) => pt(min).max(base),
            LineHeight::Auto(m) => base * m as f32 / 240.0,
        };
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
                    let single = ((base + lead + 0.001) / grid_pitch).floor() * grid_pitch + grid_pitch;
                    // The larger of the grids the single line takes and the grid times the multiple (Word's PDF of
                    // `line-multiple-h-grid-probe.docx`, 18 pt grid: 10.5 pt at 1.15 / 1.5 / 2 / 3 lines 20.76 / 27 / 36 / 54 pt;
                    // 40 pt ＭＳ 明朝, 54 pt single, stays 54 pt at 1.5, 2 and 3 lines; 12 pt in a cell on a 14.05 pt grid at 3 lines 42.15 pt).
                    let total = single.max(grid_pitch * m as f32 / 240.0);
                    return (total, total - h);
                }
                // A line of text that is exactly a grid high takes the next grid too (Word's PDFs: a 14 pt ＭＳ 明朝 line, 9/7 of 14 = 18 pt, on an
                // 18 pt grid takes two); a line spacing's minimum equal to the grid takes one.
                let tie = if matches!(p.line, LineHeight::AtLeast(_)) { -0.01 } else { 0.001 };
                let snapped = ((needed + tie) / grid_pitch).floor() * grid_pitch + grid_pitch;
                return (snapped, snapped - h);
            }
        }
        (h, 0.0)
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

    /// The room `ms_room` put into the line of the largest of `chars` (ＭＳ 明朝 / ＭＳ ゴシック), or 0.
    fn ms_room_of<'u>(&self, p: &FlowParagraph, chars: impl Iterator<Item = &'u Unit>) -> f32 {
        chars
            .filter_map(|u| {
                let Unit::Char { c, line, style, size, .. } = u else { return None };
                // A list item's number or bullet is in the line too (with the typeface `units` gave it).
                let name = if *style == LABEL { label_font_name(&p.list_style, *c) } else { p.runs.get(*style).and_then(|r| r.style.font_for(*c)) };
                Some((*line, self.ms_room(name, *size)))
            })
            .fold((0.0f32, 0.0f32), |best, x| if x.0 > best.0 { x } else { best })
            .1
    }

    /// The room under the text of a line that need not fit the page: the line grid's half of what it adds.
    fn grid_room_below(&self, p: &FlowParagraph, natural: f32, lead: f32, in_table: bool, _height: f32, _baseline: f32, _text_line: f32, _text_ascent: f32) -> f32 {
        self.line_height_parts(p, natural, lead, in_table).1 / 2.0
    }

    /// Where the baseline is in a line `height` high whose text is
    /// `text_line` high with `text_ascent`: 58 % of the room the line
    /// spacing gives beyond the text above it, and 0.87 em below that
    /// (measured on Word's PDF: 8 pt text in an exact 8.5 pt line, 12 pt in
    /// 18 pt); the room the line grid adds is split evenly above and below
    /// (Word's PDF of DBDY.p01: a label of 9 pt Arial and 游ゴシック, 11.58 pt
    /// high, in an 18 pt grid line has its baseline 12.0 pt down).
    fn baseline_in_line(&self, p: &FlowParagraph, natural: f32, lead: f32, text_line: f32, text_ascent: f32, in_table: bool, ms_room: f32) -> f32 {
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
        // Off a grid a single line of ＭＳ 明朝 / ＭＳ ゴシック puts the room `ms_room` adds to the text's 1 em half above and half below it too (Word's PDF of 019: a 10.29 pt line of 8 pt
        // text has its baseline 8.0 pt down, 0.87 em + 0.143 em, so that the next, an exact 12 pt line, is 11.9 pt under it, not the 12.9 pt of a baseline at 0.87 em).
        if grid <= 0.0 && ms_room > 0.0 && matches!(p.line, LineHeight::Auto(240)) {
            return ms_room * 0.5 + text_ascent;
        }
        // An exact line spacing leaves no room for the leading (DSEC-00011:
        // 10 pt 游ゴシック in an exact 15 pt line).
        let text_line = if grid > 0.0 || matches!(p.line, LineHeight::Exact(_)) { text_line } else { text_line + lead };
        // A multiple (`w:lineRule="auto"`) puts all of its extra room below the text
        // (DRUX-00730: 8 pt text in a double line sits at the top of it).
        let above = if matches!(p.line, LineHeight::Auto(_)) { 0.0 } else { 0.58 };
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

    fn font(&self, style: &FlowRunStyle, name: Option<&str>) -> Font {
        self.font_for_size(style, name, style.size)
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
        let (a, d) = match name.map(str::trim) {
            // Word puts ＭＳ 明朝's and ＭＳ ゴシック's baseline 0.87 of the
            // line down, not at their Windows ascent (0.859): measured on
            // Word's PDF. Whether PowerPoint differs (an app's difference)
            // is being checked by the owner of `font_metrics`.
            Some(n) if fixed_width(n) || n.starts_with("ＭＳ") || n.starts_with("MS ") => (0.87, 0.13), // i18n-ok（フォント名の判定（ＭＳ 系）。訳さない）

            Some(n) => crate::render::font_metrics::office_metrics(n)
                .or_else(|| crate::render::font_metrics::office_metrics(&font.family))
                .unwrap_or_else(drawn),
            None => crate::render::font_metrics::office_metrics(&font.family).unwrap_or_else(drawn),
        };
        // ＭＳ 明朝 / ＭＳ ゴシック: Word's lines at 10.5 pt alternate 13.5 and 13.75 pt (mean 13.625, 1.298 em against the 1 em of
        // their ascent and descent), in a table's cell as well (see `ms_room`).
        (a * size, (a + d) * size + self.ms_room(name, size))
    }

    /// ＭＳ 明朝 / ＭＳ ゴシック: the room Word leaves in a line of text beyond their ascent and descent (an empty paragraph has it too).
    ///
    /// Body text and the cells of a document without a line grid: 2/7 em, an exception kept until its source is found (user decision 2026-10-21;
    /// A2): the 0.2857 em is measured, not derived. Word's PDFs (a line of one 10.5 / 12 / 14 pt ＭＳ 明朝 or ＭＳ ゴシック, no grid) give 1.2935 em
    /// per line (13.57 / 15.55 / 18.1 pt), where the tables of the typeface (hhea 220 / -36 / gap 0 on 256 upm, OS/2 win and typo), DirectWrite
    /// (LineSpacing), GDI+ (GetLineSpacing) and GDI's TEXTMETRIC (height 1 em, external leading 0) all say 1.0 em. Century (1.2021 em) and
    /// Arial Unicode MS (about 1.15 em) agree with the tables; 游明朝 (Word 1.669 em, tables 1.602 em) does not either.
    ///
    /// The cells of a document with a line grid whose lines are not on it (no `adjustLineHeightInTable`, mode 15): 0.3 em, the 1.3 times the size
    /// that `lead_em` gives the other East Asian typefaces (1.3 times the Windows ascent plus descent, 1 em for these). Word's PDFs of
    /// `smallsize-grid230-mincho-w41453.docx` (ＭＳ 明朝 5.04 / 6.00 / 6.96 / 8.04 / 9.00 / 9.96 pt in a cell of a 11.5 pt grid) have the lines
    /// 6.48 / 7.68-7.80 / 9.0-9.12 / 10.32-10.44 / 11.64-11.76 / 12.96 pt apart, 1.3 times the size to the pixel they are drawn on; the rules of the
    /// tables of corpus documents 020, 053, 055 and 062 are then where Word's are (within 0.3 pt).
    fn ms_room(&self, name: Option<&str>, size: f32) -> f32 {
        let ms = name.map(str::trim).is_some_and(|n| fixed_width(n) || proportional_ms(n));
        // In a table's cell the line is as high as in the body (Word's PDFs: ＭＳ ゴシック 10.5 pt, 13.62 pt in the cell and in the body, in a
        // document without a line grid and with `w:linePitch` without a type); where the document has a line grid and the cell's lines are not on
        // it (no `adjustLineHeightInTable`, mode 15) they are 1.3 times the size apart, whatever the size (see above).
        let cell_off_grid = self.current_cell.get().is_some()
            && self.line_pitch.get().is_some_and(|p| p > 0)
            && !(self.doc.adjust_line_height_in_table || self.doc.compat_mode.is_some_and(|m| m < 15));
        if ms { if cell_off_grid { 0.3 * size } else { 2.0 / 7.0 * size } } else { 0.0 }
    }

    /// The room below the text a line of paragraph `p` in typeface `name` counts, in ems: `lead_em` where the line is not on the line grid, and
    /// `grid_lead_em` where it is (see `line_height_parts` for when it is). The 1.3 times of `lead_em` is measured off the grid (Word's PDFs
    /// `lineheight-*-mode15-nogrid-*`); on it Word's lines take no more than 1.667 em of 游明朝 and 游ゴシック (1.2871 em tall): a 36 pt line is 3
    /// grids of 20 pt, not 4 (issue 54), a 12 pt line of the report one grid of 20 pt (1.673 em would be 20.08 pt), and more than 1.4286 em (a 28 pt
    /// line of 游ゴシック is 3 grids, 40.04 pt).
    fn lead_in(&self, p: &FlowParagraph, name: Option<&str>) -> f32 {
        let in_cell = self.current_cell.get().is_some() && !self.doc.adjust_line_height_in_table && self.doc.compat_mode.is_none_or(|m| m >= 15);
        let on_grid = !in_cell && p.snap_to_grid && self.line_pitch.get().is_some_and(|pitch| pitch > 0) && !matches!(p.line, LineHeight::Exact(_) | LineHeight::AtLeast(0));
        if on_grid { Self::grid_lead_em(name) } else { self.lead_em(name) }
    }

    /// `lead_em` on the line grid: 游ゴシック 0.143 em (its lines count 1.43 em there: Word's PDF of the editing report's title), the others none.
    fn grid_lead_em(name: Option<&str>) -> f32 {
        match name.map(str::trim) {
            Some(n) if (n.starts_with("游ゴシック") || n.starts_with("Yu Gothic") || n.starts_with("YuGothic")) && !n.contains("UI") => 0.143, // i18n-ok（フォント名の判定（游ゴシック）。訳さない）
            _ => 0.0,
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
    /// of the Japanese typefaces Office ships (AGENTS.md: the fallback table for typefaces the PC lacks). ＭＳ 明朝 and ＭＳ ゴシック keep their
    /// room in `ms_room` (0.3 em measured 1.297 em in Word for ＭＳ ゴシック 10.5 pt, but it makes 056 and 076 a page longer than Word's).
    fn lead_em(&self, name: Option<&str>) -> f32 {
        let Some(n) = name.map(str::trim) else { return 0.0 };
        if fixed_width(n) || proportional_ms(n) {
            return 0.0;
        }
        let japanese = self.res.is_japanese_typeface(n).unwrap_or_else(|| {
            ["游ゴシック", "Yu Gothic", "YuGothic", "游明朝", "Yu Mincho", "YuMincho", "メイリオ", "Meiryo"].iter().any(|p| n.starts_with(p)) // i18n-ok（フォント名の判定。訳さない）
        });
        if !japanese {
            return 0.0;
        }
        let (a, d) = crate::render::font_metrics::office_metrics(n).unwrap_or_else(|| {
            let font = self.res.font(&TextStyle { font: Some(n.to_string()), ..TextStyle::default() });
            let (a, d, _) = self.res.line_metrics(&font);
            (a, d)
        });
        0.3 * (a + d)
    }

    /// How far a 縦中横 run goes down a vertical line: as far as a line of its typeface is high, in points. Word's PDFs (a run of 2 to 8
    /// digits between kana, with and without a line grid, 行の幅に合わせる on and off; 9 / 10.5 / 12 / 16 / 24 pt ＭＳ ゴシック and ＭＳ 明朝, 9 / 10.5 / 12 /
    /// 16 / 24 pt 游明朝) have the kana after the run 1.29 to 1.30 em (ＭＳ), 1.67 em (游明朝) below the end of the kana before it, whatever the
    /// digits: the height of a line of the typeface (the same 1.3 times its Windows ascent plus descent as `lead_em`; the digits, horizontal
    /// text, take the height of their line down the line and lie across it). ＭＳ's is this layout's line (`ms_room`'s 2/7 em, 1.2857 em
    /// where Word's PDFs have 1.2935 to 1.3017), not another constant.
    fn tate_chu_yoko_cell(&self, name: Option<&str>, font: &Font, size: f32) -> f32 {
        self.line_metrics(name, font, size).1 + self.lead_em(name) * size
    }

    /// The width of a 縦中横 run's digits across the line: their advances at the size, times the run's `w:w`.
    fn tate_chu_yoko_across(&self, style: &ibis_doc_model::FlowRunStyle, units: &[Unit]) -> f32 {
        units.iter().filter_map(|u| match u {
            Unit::Char { c, size, .. } => Some(advance_em(self.res, style.font_for(*c), *c) * size),
            _ => None,
        }).sum::<f32>() * style.scale as f32 / 100.0
    }

    /// A 縦中横 run on an across line (the page is turned a quarter afterwards, `turn_page_items`): its digits turned anticlockwise, one
    /// by one when `w:w` squeezes them (each takes its advance times the squeeze). They are centred on the kana's box (Word's PDFs: the same
    /// middle, 行の幅に合わせる's squeezed digits and 行を広げる's wider ones alike) and in the middle of the run's cell down the line.
    fn tate_chu_yoko_items(&self, style: &ibis_doc_model::FlowRunStyle, text: &str, x0: f32, y_base: f32, size: f32, font: &Font, color: Hsla) -> Vec<Item> {
        let xscale = style.scale as f32 / 100.0;
        let advances: Vec<f32> = text.chars().map(|c| advance_em(self.res, style.font_for(c), c) * size * xscale).collect();
        let across: f32 = advances.iter().sum();
        let cell = self.tate_chu_yoko_cell(text.chars().next().and_then(|c| style.font_for(c)), font, size);
        // The kana's box is `0.35` of its size right of the baseline's place on the page (`turn_cell_text`).
        let mut y = y_base - 0.35 * size + across / 2.0;
        let x = x0 + (cell - size) / 2.0 + size * 0.88;
        if (xscale - 1.0).abs() < 0.001 {
            return vec![Item::Turned { x, y, text: text.to_string().into(), font: font.clone(), size, color, turns: -1, xscale: 1.0 }];
        }
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
        let w: f32 = inner.chars().map(|c| self.res.text_width(&c.to_string(), &self.font(s, s.font_for(c)), small)).sum();
        let font = self.font(s, s.font_for(inner.chars().next().unwrap_or('あ')));
        let offsets: Vec<f32> = inner.chars().scan(0.0f32, |at, c| {
            let x = *at;
            *at += self.res.text_width(&c.to_string(), &self.font(s, s.font_for(c)), small);
            Some(x)
        }).collect();
        // Centred on the enclosure: its middle 0.35 em above the baseline.
        vec![Item::Text { x: x0 + (width - w) / 2.0, y: y - size * 0.35 + small * 0.35, text: inner.into(), offsets: offsets.into(), font, size: small, color }]
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
        for (text, raise) in shown {
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
        let measure = |list: &[char], at: f32| list.iter().map(|ch| self.res.text_width(&ch.to_string(), &self.font(s, s.font_for(*ch)), at)).sum::<f32>();
        let first = chars.len().div_ceil(2);
        let block = measure(&chars[..first], size / 2.0).max(measure(&chars[first..], size / 2.0));
        block + c.brackets().map_or(0.0, |(a, b)| self.bracket_width(s, a, size) + self.bracket_width(s, b, size))
    }

    /// 組み文字 drawn from `x0` on baseline `y`: the first line's baseline
    /// 0.517 em above, the second's 0.186 em below (Word's PDF of DW14 at
    /// 14 pt), between its brackets at the text's size.
    #[allow(clippy::too_many_arguments)]
    fn combine_items(&self, s: &FlowRunStyle, c: ibis_doc_model::Combine, text: &str, x0: f32, y: f32, size: f32, color: Hsla) -> Vec<Item> {
        let chars: Vec<char> = text.chars().filter(|ch| *ch != '\u{200b}').collect();
        let half = size / 2.0;
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
        if let Some((open, _)) = c.brackets() {
            put(open, x, y, size, bracket_face);
            x += self.bracket_width(s, open, size);
        }
        for (list, dy) in [(&chars[..first], -0.517), (&chars[first..], 0.186)] {
            // Each line centred in the block.
            let mut lx = x + (block - line_w(list)) / 2.0;
            for ch in list {
                put(*ch, lx, y + dy * size, half, s.font_for(*ch));
                lx += width(*ch, half);
            }
        }
        if let Some((_, close)) = c.brackets() {
            put(close, x + block, y, size, bracket_face);
        }
        out
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
        vec![Item::Text { x: x0, y: y - *raise as f32 / 100.0, text: text.clone().into(), offsets: offsets.into(), font, size: rs, color }]
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
        let (mark, up, left) = match s.emphasis {
            _ if label => return Vec::new(),
            EmphasisMark::None => return Vec::new(),
            EmphasisMark::Dot => ('．', 1.0114, 0.166),
            EmphasisMark::Comma => ('、', 1.0114, 0.166),
            EmphasisMark::Circle => ('˚', 0.2914, 0.154),
            EmphasisMark::UnderDot => ('．', -0.377, 0.199),
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
                Piece::Text { x: dx, y, text, size, italic } => {
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
                Piece::Rule { x: dx, y, w, h } => Item::Rect { x: x + dx, y: baseline + y, w: *w, h: *h, color },
                Piece::Line { x0, y0, x1, y1, t } => Item::Stroke { x: x + x0, y: baseline + y0, x2: x + x1, y2: baseline + y1, w: *t, color },
            })
            .collect()
    }

    /// The paragraph's text as units, with the page number for `PAGE`.
    fn units(&self, p: &FlowParagraph, page_number: u32) -> (Vec<Unit>, Vec<(usize, bool)>) {
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
                out.push(Unit::Picture { media, w, h });
                positions.push((0, false));
            }
            // A bullet of a symbol typeface takes the room the typeface gives it, where it is installed (Word's PDF of 072: Wingdings'
            // check mark, 0.786 em, ends before the hanging indent, which the stand-in ✓ of a Japanese face, an em, passes).
            let symbol = self.doc.styles.numbering.level(list.num_id, list.level).and_then(|l| l.symbol.clone()).filter(|(f, _)| self.res.has_typeface(f));
            for c in label.chars().filter(|_| picture.is_none()) {
                // A bullet in the geometric shapes (●, ■, ◆…) defaults to Arial in
                // Word's PDF of DWID (small) if no bullet font was specified, not the Japanese font's.
                let name = label_font_name(style, c);
                let font = self.font(style, name);
                let width = match &symbol {
                    Some((face, code)) if label.chars().count() == 1 => self.res.text_width(&code.to_string(), &self.font(style, Some(face.as_str())), size),
                    _ => self.res.text_width(&c.to_string(), &font, size),
                };
                let (mut ascent, mut line) = self.line_metrics(name, &font, size);
                if is_hangul(c) && line < size * HANGUL_LINE {
                    let extra = size * HANGUL_LINE - line;
                    ascent += extra * HANGUL_ASCENT_SHARE;
                    line += extra;
                }
                out.push(Unit::Char { c, width, inset: 0.0, font, size, style: LABEL, line, ascent, lead: self.lead_in(p, name) * size, auto_space: 0.0, squeeze: 1.0, squeezed: false });
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
                    out.push(Unit::Char { c: ' ', width, inset: 0.0, font, size, style: LABEL, line, ascent, lead: self.lead_em(style.font_for(' ')) * size, auto_space: 0.0, squeeze: 1.0, squeezed: false });
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
                out.push(Unit::Char { c, width, inset: 0.0, font, size, style: LABEL, line, ascent, lead: self.lead_in(p, name) * size, auto_space: 0.0, squeeze: 1.0, squeezed: false });
                positions.push((0, false));
            }
        }
        let has_east_asian = p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Text(t) if t.chars().any(is_cjk)));
        // The paragraph's text and where each run's begins in it (the neighbours of a space may be in another run).
        let para_chars: Vec<char> = p.runs.iter().filter_map(|r| if let FlowRunKind::Text(t) = &r.kind { Some(t.chars()) } else { None }).flatten().collect();
        let run_offsets: Vec<usize> = p.runs.iter().scan(0usize, |sum, r| { let start = *sum; if let FlowRunKind::Text(t) = &r.kind { *sum += t.chars().count(); } Some(start) }).collect();
        for (index, run) in p.runs.iter().enumerate() {
            let before = out.len();
            let style = &run.style;
            let size = style.size as f32 / 100.0 * if style.position == VerticalPosition::Baseline { 1.0 } else { 0.65 };
            let half_points = (style.size + 25) / 50;
            let small_half_points = (half_points * 8 + 5) / 10;
            let _small_size = (small_half_points * 50) as f32 / 100.0 * if style.position == VerticalPosition::Baseline { 1.0 } else { 0.65 };
            let push_text = |text: &str, out: &mut Vec<Unit>| {
                let all: Vec<char> = text.chars().collect();
                for (at, &c) in all.iter().enumerate() {
                    let char_style_size = style.size_for(c);
                    let (base_size, char_half_points) = if char_style_size == style.size {
                        (size, half_points)
                    } else {
                        let s = char_style_size as f32 / 100.0 * if style.position == VerticalPosition::Baseline { 1.0 } else { 0.65 };
                        let hp = (char_style_size + 25) / 50;
                        (s, hp)
                    };
                    let (c, char_size) = if style.caps {
                        (c.to_uppercase().next().unwrap_or(c), base_size)
                    } else if style.small_caps && c.is_lowercase() {
                        let small_hp = (char_half_points * 8 + 5) / 10;
                        let small_sz = (small_hp * 50) as f32 / 100.0 * if style.position == VerticalPosition::Baseline { 1.0 } else { 0.65 };
                        (c.to_uppercase().next().unwrap_or(c), small_sz)
                    } else {
                        (c, base_size)
                    };
                    // Curly quotes are in the East Asian font only beside East Asian text (without `w:hint`).
                    let east_asian_here = if matches!(c, '‘' | '’' | '“' | '”') { (at > 0 && is_cjk(all[at - 1])) || all.get(at + 1).is_some_and(|n| is_cjk(*n)) } else { has_east_asian }; // i18n-ok（引用符の文字の集合）
                    let name = style.font_for_in(c, east_asian_here);
                    // The stand-in for ＭＳ 明朝 / ＭＳ ゴシック (Hiragino Mincho ProN / Hiragino Sans) has half-width
                    // (0.5 em) ASCII glyphs under its OpenType `hwid` (`Half Widths`) feature, matching Word's 0.5 em
                    // cell without overflowing into the neighbour (`w2_ms_halfwidth_latin_drawn_half_em`).
                    let installed = name.is_some_and(|n| self.res.has_typeface(n));
                    let mut font = self.font_for_size(style, name, char_style_size);
                    // An emoji (a pictograph, or a character with the emoji selector, or a joined sequence) is drawn in the OS's emoji
                    // typeface whatever the run's font (Word forces Segoe UI Emoji); the selectors and joined characters of a cluster have its
                    // font too, so that the cluster is one item to shape into one glyph, and take no room (#66, #79).
                    let (cluster_start, cluster_end) = crate::render::rich_text::cluster_bounds(&all, at);
                    let in_cluster = at > cluster_start;
                    let emoji = crate::render::rich_text::is_emoji_cluster(&all[cluster_start..cluster_end]);
                    if emoji {
                        font = crate::render::rich_text::emoji_font(&font);
                    }
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
                            let cell = if installed { actual } else { proportional_ms_width(c, char_size) };
                            (cell, 0.0)
                        }
                        Some(n) if fixed_width(n) => {
                            let half = char_size / 2.0;
                            let cell = if full_width(c) { char_size } else { half };
                            let inset = if c.is_whitespace() { 0.0 } else { ((cell - actual) / 2.0).max(0.0) };
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
                        _ => (actual, 0.0),
                    };
                    // A comment's mark (U+200B) takes no room.
                    // The character grid's pitch (文字数と行数を指定): a full-width character takes the grid's added pitch (Word's PDFs keep the Latin letters of a proportional font as they are, and give those of a fixed-width one (ＭＳ 明朝, ＭＳ ゴシック) the added pitch too: a half-width 8 pt letter 4.0 → 3.0 pt, and the characters in a paragraph on the grid).
                    // A run's character spacing (`w:spacing`) counts twice in a paragraph on the character grid (Word's PDF of
                    // gridspacing-halfwidth-probe.docx: +0.4 / -0.4 / -1.0 pt moves every character, Latin, digit, space or kana, in a fixed-width
                    // or proportional face, by +0.8 / -0.8 / -2.0 pt on charSpace +1.624 and -1.0 pt).
                    let grid_spacing_factor = self.spacing_factor(p, c);
                    let grid_added = if !p.snap_to_grid {
                        0.0
                    } else {
                        self.char_space.get().map_or(0.0, |cs| {
                            if is_cjk(c) {
                                pt(cs)
                            } else if name.is_some_and(fixed_width) {
                                // A half-width character of a fixed-width font sits in half a grid cell, so it takes half of the added pitch
                                // (Word's PDFs: ＭＳ 明朝 8 pt on charSpace +0.503 pt, the digits, brackets and spaces advance 4.25 pt = 4.0 + 0.25
                                // (103); 11 pt on charSpace -1.0 pt, each space 5.02 pt = 5.52 - 0.5 (121)).
                                if full_width(c) { pt(cs) } else { pt(cs) / 2.0 }
                            } else {
                                0.0
                            }
                        })
                    };
                    let cell=if self.doc.page.direction.vertical() && crate::render::vertical_text::upright(c){char_size}else{cell};
                    let width = if c == '\u{200b}' || c == '\u{ad}' || in_cluster { 0.0 } else { cell * style.scale as f32 / 100.0 + pt(style.spacing) * grid_spacing_factor + grid_added };
                    // A substitute's glyph wider than the cell Word's font gives it is squeezed into it (it would
                    // overlap its neighbour).
                    let fits = if !emoji && name.is_some_and(|n| fixed_width(n) || proportional_ms(n)) && actual > cell + 0.05 && cell > 0.0 { cell / actual } else { 1.0 };
                    let (mut ascent, mut line) = self.line_metrics(name, &font, base_size);
                    // A line with Hangul is 1.72 em high in Word (18.1 pt of 10.5 pt text, DNUM: its fallback font for
                    // Hangul), the text sitting low in it.
                    if is_hangul(c) && line < base_size * HANGUL_LINE {
                        let extra = base_size * HANGUL_LINE - line;
                        ascent += extra * HANGUL_ASCENT_SHARE;
                        line += extra;
                    }
                    if let Some(b) = &style.border {
                        let pad_v = pt(b.space);
                        let extra_bw = (pt(b.total()) - pt(b.width)).max(0.0);
                        ascent += 1.2 + pad_v;
                        line += 1.2 + 2.8 + 2.0 * pad_v + 2.0 * extra_bw;
                    }
                    // An optional hyphen (`w:softHyphen`) takes no room and is not shown, unless a line ends after it: then it is a hyphen. Its `auto_space` is the
                    // hyphen's width, negative: taking the gap off the width at the end of a line (`end_of_line`) adds it.
                    let auto_space = if c == '\u{00ad}' { -self.res.text_width("-", &font, char_size) } else { 0.0 };
                    out.push(Unit::Char { c, width, inset, font, size: char_size, style: index, line, ascent, lead: self.lead_in(p, name) * base_size, auto_space, squeeze: style.scale as f32 / 100.0 * fits, squeezed: false });
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
                FlowRunKind::Field { instruction, shown } => match self.field_text(instruction, page_number) {
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
                                    let width = self.res.text_width(&ring.to_string(), &font, ring_pt);
                                    let (ascent, line) = self.line_metrics(name, &font, ring_pt);
                                    out.push(Unit::Char { c: ring, width, inset: 0.0, font, size: ring_pt, style: index, line, ascent, lead: self.lead_in(p, name) * ring_pt, auto_space: 0.0, squeeze: 1.0, squeezed: false });
                                }
                                None => push_text(&ring.to_string(), &mut out),
                            }
                        }
                        None => match ibis_doc_model::field_format::overstrike(instruction) {
                            // 重ね書き (EQ \o\ad, \o\al): room as wide as the second
                            // argument (spread) or the widest one, left blank (the
                            // arguments are drawn over it).
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
                FlowRunKind::Picture { media, size, look } | FlowRunKind::PictureControl { media, size, look, .. } if look.is_visual_default() => out.push(Unit::Picture { media: *media, w: pt(size.width), h: pt(size.height) }),
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
                    if let Unit::Char { width, inset, line, ascent, .. } = u {
                        *width = cell / n;
                        *inset = 0.0;
                        // 行を広げる (off): the digits lie across the line, so the line is as thick as they are wide, the characters of the
                        // line in the middle of it (Word's PDFs: the lines before and after the digits move apart by half of what they
                        // are wider than the line, each). 行の幅に合わせる squeezes them to fit the line, so it never widens it.
                        if !style.tate_chu_yoko_fit && across > *line {
                            *ascent += (across - *line) / 2.0;
                            *line = across;
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
            let spaced = |c: char| (p.auto_space_letters && c.is_ascii_alphabetic()) || (p.auto_space_digits && c.is_ascii_digit());
            // Not the middle dot ・ (Word's PDFs of DTBA and DFLD: "auto・セル"
            // and "2000・tcW" without the space).
            let japanese = |c: char| c != '・' && matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x9fff | 0xf900..=0xfaff);
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
                let in_combined = combined(&out[i - 1]) || combined(&out[i]) || tate(&out[i - 1]) || tate(&out[i]);
                if !field_boundary && !in_combined
                    && ((japanese(a) && spaced(b)) || (spaced(a) && japanese(b)))
                    && let Unit::Char { width, size, auto_space, .. } = &mut out[i - 1]
                {
                    let space = *size / 4.0;
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
            // another are squeezed as the closing one before an opening is.
            let closing = |ch: char| "、。，．）」』】〕〉》］｝".contains(ch); // i18n-ok（行頭禁則の文字の集合。文字の並び）
            let opening = |ch: char| "「『（【〔〈《［｛".contains(ch); // i18n-ok（行末禁則の文字の集合。文字の並び）
            let closing_bracket = |ch: char| "）」』】〕〉》］｝".contains(ch); // i18n-ok（閉じ括弧の文字の集合。文字の並び）
            if matches!(&out[i - 1], Unit::Char { style, .. } if p.runs.get(*style).is_some_and(|r| r.style.kern.is_some()))
                && ((closing(a) && opening(b)) || (opening(a) && opening(b)) || (closing_bracket(a) && closing_bracket(b)))
                && let Unit::Char { width, size, squeezed, .. } = &mut out[i - 1]
                && *width >= *size * 0.99
            {
                *width -= *size / 2.0;
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
        (out, positions)
    }

    /// Break a paragraph `width` points wide into lines.
    fn paragraph(&self, p: &FlowParagraph, width: f32, page_number: u32, obstacles: &[WrapObstacleInPara], in_table: bool) -> Vec<LineBox> {
        self.line_room.set(width - pt(p.left) - pt(p.right));
        let (mut units, mut positions) = self.units(p, page_number);
        self.line_room.set(f32::MAX);
        let left = pt(p.left);
        let right = pt(p.right);
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
                self.float_box(shape, &geometry, geometry.margin_t, default_line_h).in_paragraph(geometry.margin_t)
            }).collect()
        } else {
            Vec::new()
        };
        let drop_cap_obs = if obstacles.is_empty() && default_obs.is_empty() {
            if let Some(dc) = p.drop_cap.filter(|d| d.location != ibis_doc_model::DropCapLocation::None) {
                let first_char_idx = units.iter().position(|u| matches!(u, Unit::Char { c, .. } if !c.is_whitespace() && *c != '\u{200b}'));
                if let Some(idx) = first_char_idx {
                    let (c, font, style) = match &units[idx] {
                        Unit::Char { c, font, style, .. } => (*c, font.clone(), *style),
                        _ => unreachable!(),
                    };
                    units.remove(idx);
                    if idx < positions.len() {
                        positions.remove(idx);
                    }
                    let lines_count = (dc.lines as f32).clamp(1.0, 10.0);
                    let cap_h = default_line_h * lines_count;
                    let cap_font_size = default_line_h * lines_count * 0.78;
                    let cap_w = self.res.text_width(&c.to_string(), &font, cap_font_size);
                    let space_pt = pt(dc.space);
                    let cap_baseline = (lines_count - 1.0) * default_line_h + mark_metrics.1;
                    let cap_color = p.runs.get(style).map_or_else(
                        || hsla(ibis_doc_model::Color::opaque(p.list_style.color)),
                        |r| hsla(ibis_doc_model::Color::opaque(r.style.color)),
                    );
                    let start_x = line_start_x(true);
                    let (cap_x, obs_right) = if dc.location == ibis_doc_model::DropCapLocation::Margin {
                        (start_x - cap_w - space_pt, start_x)
                    } else {
                        (start_x, start_x + cap_w + space_pt)
                    };
                    let item = Item::Text {
                        x: cap_x,
                        y: cap_baseline,
                        text: c.to_string().into(),
                        offsets: Arc::from([0.0]),
                        font,
                        size: cap_font_size,
                        color: cap_color,
                    };
                    Some(WrapObstacleInPara {
                        shape_left: cap_x,
                        shape_right: obs_right,
                        rel_top: 0.0,
                        rel_bottom: cap_h,
                        wrap_text: "right".to_string(),
                        wrap_polygon: None,
                        shape_to_emit: None,
                        drop_cap_to_emit: Some(item),
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        let effective_obs: Vec<&WrapObstacleInPara> = if !obstacles.is_empty() { obstacles.iter().collect() } else if !default_obs.is_empty() { default_obs.iter().collect() } else { drop_cap_obs.iter().collect() };

        if !effective_obs.is_empty() {

            #[derive(Clone, Copy)]
            struct Segment {
                s: usize,
                e: usize,
                slot_x: f32,
                slot_w: f32,
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

                for &(slot_x, slot_w) in &slots {
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
                    let start = i;
                    let mut x = 0.0;
                    let mut last_break: Option<usize> = None;
                    let mut slot_ended = false;
                    let mut hang: Option<usize> = None;

                    while i < units.len() {
                        match &units[i] {
                            Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak => {
                                phys_line.push(Segment { s: start, e: i + 1, slot_x, slot_w, forced: true, hang: hang_start(hang.take(), i + 1) });
                                if i > 0 && let Unit::Char { width, auto_space, .. } = &mut units[i - 1] {
                                    *width -= *auto_space;
                                    *auto_space = 0.0;
                                }
                                i += 1;
                                slot_ended = true;
                                line_forced_break = true;
                                break;
                            }
                            Unit::ListTab if left > slot_x + x + 0.01 => {
                                x = left - slot_x;
                                i += 1;
                                continue;
                            }
                            Unit::Tab | Unit::ListTab => {
                                let at = slot_x + x;
                                let tab = p.tabs.iter().find(|t| pt(t.position) > at + 0.01);
                                let stop = tab.map(|t| pt(t.position)).unwrap_or(((at / tab_default).floor() + 1.0) * tab_default);
                                let rest: f32 = units[i + 1..].iter().take_while(|u| !matches!(u, Unit::Tab | Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak)).map(unit_width).sum();
                                let stop = match tab.map(|t| t.align) {
                                    Some(TabAlign::Right) => (stop - rest).max(at),
                                    Some(TabAlign::Center) => (stop - rest / 2.0).max(at),
                                    _ => stop,
                                };
                                x = stop - slot_x;
                                i += 1;
                                continue;
                            }
                            _ => {}
                        }
                        let w = unit_width(&units[i]);
                        let c = unit_char(&units[i]);
                        let combined_rest = is_combined_rest(p, &units[i], w);
                        if i > start && !combined_rest {
                            let prev = unit_char(&units[i - 1]);
                            if may_break(prev, c, p.kinsoku, p.word_wrap, self.doc) {
                                last_break = Some(i);
                            }
                        }
                        if x + w > slot_w + 0.01 && i > start && !combined_rest {
                            if c == Some(' ') {
                                hang.get_or_insert(i);
                                x += w;
                                i += 1;
                                continue;
                            }
                            // Full-width spaces that end the paragraph hang too (as in the body's lines).
                            if c == Some('\u{3000}') && units[i..].iter().all(|u| matches!(unit_char(u), Some('\u{3000}'))) {
                                hang.get_or_insert(i);
                                x += w;
                                i += 1;
                                continue;
                            }
                            if p.overflow_punct && p.kinsoku && c.is_some_and(|ch| "、。，．,".contains(ch)) && x <= slot_w + 0.01 { // i18n-ok（オーバーフロー句読点の文字の集合。文字の並び）
                                hang.get_or_insert(i);
                                x += w;
                                i += 1;
                                continue;
                            }
                            let at = match last_break {
                                Some(b) if b > start => b,
                                _ => i,
                            };
                            phys_line.push(Segment { s: start, e: at, slot_x, slot_w, forced: false, hang: hang_start(hang.take(), at) });
                            if at > 0 && let Unit::Char { c, width, font, size, .. } = &mut units[at - 1] && *c == '\u{ad}' {
                                *c = '-';
                                *width = self.res.text_width("-", font, *size);
                            }
                            if at > 0 && let Unit::Char { width, auto_space, .. } = &mut units[at - 1] {
                                *width -= *auto_space;
                                *auto_space = 0.0;
                            }
                            i = at;
                            slot_ended = true;
                            break;
                        }
                        x += w;
                        i += 1;
                    }

                    if !slot_ended && start < units.len() {
                        phys_line.push(Segment { s: start, e: units.len(), slot_x, slot_w, forced: true, hang: hang_start(hang.take(), units.len()) });
                        if let Some(Unit::Char { width, auto_space, .. }) = units.last_mut() {
                            *width -= *auto_space;
                            *auto_space = 0.0;
                        }
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
                    phys_line.push(Segment { s: 0, e: units.len(), slot_x: line_start, slot_w: full_room, forced: true, hang: units.len() });
                }

                if phys_line.is_empty() && is_intersecting && i < units.len() && shape_bottom > cur_y + 0.01 {
                    extras.push(0.0);
                    skipped.push(Some(shape_bottom - cur_y));
                    cur_y = shape_bottom;
                    physical_lines.push(phys_line);
                    continue;
                }
                extras.push(extra_above);
                skipped.push(None);
                let spaces_only = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).all(|u| !matches!(u, Unit::Char { c, .. } if !c.is_whitespace()));
                let (text_line, lead) = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Char { c, line, lead, .. } = u { if spaces_only || !c.is_whitespace() { Some((*line, *lead)) } else { None } } else { None }).fold((0.0f32, 0.0f32), |(l, d), (l2, d2)| (l.max(l2), d.max(d2)));
                let (text_line, lead) = if text_line > 0.0 { (text_line, lead) } else { (mark_metrics.0, mark_metrics.2) };
                let picture = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| match u { Unit::Picture { h, .. } | Unit::Shape { h, .. } | Unit::Math { h, .. } => Some(*h), _ => None }).fold(0.0, f32::max);
                let natural = text_line.max(picture);
                let lead = if picture > text_line { 0.0 } else { lead };
                let height = self.line_height_around(p, text_line, picture, natural, lead, in_table);
                cur_y += height;
                if phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).any(|u| matches!(u, Unit::PageBreak)) {
                    past_page_break = true;
                }

                physical_lines.push(phys_line);
                if i >= units.len() {
                    break;
                }
            }

            let mut out = Vec::new();
            for (n, phys_line) in physical_lines.iter().enumerate() {
                let largest = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Char { size, .. } = u { Some(*size) } else { None }).fold(0.0, f32::max);
                let largest = if largest > 0.0 { largest } else { mark_size };
                // A superscript or subscript is raised by its full size (DNOTE: a 10.5 pt number is 3.59 pt up = 0.342 em).
                let largest_raise = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Char { size, style, .. } = u { Some(if p.runs.get(*style).is_some_and(|r| *style != LABEL && r.style.position != VerticalPosition::Baseline) { *size / 0.65 } else { *size }) } else { None }).fold(0.0, f32::max).max(largest);
                let spaces_only = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).all(|u| !matches!(u, Unit::Char { c, .. } if !c.is_whitespace()));
                let (text_line, text_ascent, lead) = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Char { c, line, ascent, lead, style, size, .. } = u { if !spaces_only && c.is_whitespace() { return None; } let (line, ascent) = mark_room(p, *c, *style, *size, *line, *ascent); Some((line, ascent, *lead)) } else { None }).fold((0.0f32, 0.0f32, 0.0f32), |(l, a, d), (l2, a2, d2)| (l.max(l2), a.max(a2), d.max(d2)));
                let (text_line, text_ascent, lead) = if text_line > 0.0 { (text_line, text_ascent, lead) } else { mark_metrics };
                let picture = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| match u { Unit::Picture { h, .. } | Unit::Shape { h, .. } | Unit::Math { h, .. } => Some(*h), _ => None }).fold(0.0, f32::max);
                let natural = text_line.max(picture);
                let lead = if picture > text_line { 0.0 } else { lead };
                let height = self.line_height_around(p, text_line, picture, natural, lead, in_table);
                let drop_cap_bottom = effective_obs.iter().filter(|obs| obs.drop_cap_to_emit.is_some()).map(|obs| obs.rel_bottom).fold(f32::MIN, f32::max);
                let height = if physical_lines.len() <= 1 && drop_cap_bottom > f32::MIN {
                    height.max(drop_cap_bottom)
                } else {
                    height
                };
                let height = skipped.get(n).copied().flatten().unwrap_or(height + extras.get(n).copied().unwrap_or(0.0));
                let math_below = phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]).filter_map(|u| if let Unit::Math { descent, .. } = u { Some(*descent) } else { None }).fold(0.0f32, f32::max);
                let baseline = if picture > text_line { height - math_below } else { self.baseline_in_line(p, natural, lead, text_line, text_ascent, in_table, self.ms_room_of(p, phys_line.iter().flat_map(|seg| &units[seg.s..seg.e]))) };
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
                        if let Some(item) = &obs.drop_cap_to_emit {
                            line.items.push(item.clone());
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
                    let has_tab = slice.iter().any(|u| matches!(u, Unit::Tab));
                    let marks = slice[..visible_end].iter().filter(|u| unit_char(u) == Some('\u{200b}')).count();
                    let room = seg.slot_w;
                    let extras = if justify && !has_tab && (used < room || p.align == FlowAlign::Justify) { spread_extras(&slice[..visible_end], room - used, p.align == FlowAlign::Distribute, marks) } else { Vec::new() };
                    // A displayed formula alone on its line is centred.
                    let align = if slice.iter().any(|u| matches!(u, Unit::Math { display: true, .. })) && slice.iter().all(|u| !matches!(u, Unit::Char { c, .. } if !c.is_whitespace())) { FlowAlign::Center } else { p.align };
                    let shift = match align {
                        FlowAlign::Center => ((room - used) / 2.0).max(0.0),
                        FlowAlign::Right => (room - if last { slice.iter().map(unit_width).sum::<f32>() } else { used }).max(0.0),
                        FlowAlign::Left if p.bidi => (room - used).max(0.0),
                        _ => 0.0,
                    };
                    let mut x = seg.slot_x + shift;
                    let line_x = x;
                    let mut stops: Vec<(usize, f32)> = Vec::new();
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
                            let s_size = s.enclosure_sizes.map_or(s.size, |(ring, _)| ring) as f32 / 100.0 * if s.position == VerticalPosition::Baseline { 1.0 } else { 0.65 };
                            let raise = match s.position {
                                VerticalPosition::Superscript => -largest_raise * 0.35,
                                VerticalPosition::Subscript => largest_raise * 0.1,
                                VerticalPosition::Baseline => 0.0,
                            };
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
                                let pad_v = pt(b.space);
                                let extra_bw = (pt(b.total()) - pt(b.width)).max(0.0);
                                let y_top = (y_base - run_ascent - 1.2 - pad_v).max(0.0);
                                let y_bottom = y_base + (run_line - run_ascent) + 2.8 + pad_v + 2.0 * extra_bw;
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
                                for (a, z) in spans {
                                    let ux = x0 + a;
                                    let uw = z - a;
                                    if let Some(last_u) = dec.underlines.last_mut()
                                        && last_u.style == s.underline_style
                                        && last_u.color == ucolor
                                        && (last_u.baseline - y_base).abs() < 0.01
                                        && (last_u.size - u_size).abs() < 0.01
                                        && (last_u.x + last_u.w - ux).abs() < 0.02
                                    {
                                        last_u.w = (ux + uw) - last_u.x;
                                    } else {
                                        dec.underlines.push(UnderlineBox {
                                            style: s.underline_style,
                                            size: u_size,
                                            x: ux,
                                            w: uw,
                                            baseline: y_base,
                                            color: ucolor,
                                        });
                                    }
                                }
                            }
                            if (s.strike || s.double_strike) && end > 0.0 {
                                let strike_size = s_size;
                                // A double one: two thinner lines about the single one.
                                let lines: Vec<(f32, f32)> = if s.double_strike {
                                    let sh = (strike_size / 13.0).max(0.4);
                                    vec![(y_base - strike_size * 0.3 - strike_size * 0.08, sh), (y_base - strike_size * 0.3 + strike_size * 0.08, sh)]
                                } else {
                                    vec![(y_base - strike_size * 0.3, (strike_size / 18.0).max(0.4))]
                                };
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
                            line.items.extend(self.emphasis_marks(s, style == LABEL, x0, y_base, s_size, &text, &offsets, color));
                            if let Some(run) = p.runs.get(style).filter(|_| style != LABEL) {
                                line.items.extend(self.enclosed_items(s, run, x0, y_base, end, s_size, color));
                                line.items.extend(self.overstrike_items(s, run, x0, y_base, end, s_size, color));
                            }
                            if let Some(run) = p.runs.get(style).filter(|_| style != LABEL) {
                                line.items.extend(self.ruby_items(s, &run.kind, x0, y_base, end, color));
                            }
                            let combined = p.runs.get(style).filter(|_| style != LABEL).and_then(|r| r.style.combine);
                            if let Some(c) = combined {
                                line.items.extend(self.combine_items(s, c, &text, x0, y_base, s_size, color));
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
                                let same = run.as_ref().is_some_and(|r| r.5 == *style && (tate_run || (r.3 == *font && r.4 == *size && r.6 == *squeeze)));
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
                                let (span_l, span_r) = if *relative_to_margin { (0.0, width) } else { (left, width - right) };
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
                            Unit::Picture { media, w, h } => {
                                flush(&mut run, &mut line, &mut dec);
                                line.items.push(Item::Picture { media: *media, x, y: line.baseline - h, w: *w, h: *h });
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
                    dec.finish(&mut line);
                    let end = positions.get(seg.e).map_or(p.len(), |&(position, _)| position);
                    stops.push((end, x - line_x));
                    line.items.push(Item::Line { para: p.id, x: line_x, y: 0.0, h: line.height, stops: stops.into(), vertical: false });
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
        let mut hang: Option<usize> = None;
        let mut start = 0;
        let mut x = 0.0;
        let mut i = 0;
        let mut last_break: Option<usize> = None;
        // The line (its `start`) whose overflowing character was taken in by squeezing stops and brackets (`compress_punctuation`).
        let mut compressed_line: Option<usize> = None;
        // The automatic spaces already squeezed off the line that starts at `.0` (`.1` pt), for the limit of the next character's squeeze.
        let mut gaps_cut: (usize, f32) = (usize::MAX, 0.0);
        // How far a right-aligned tab stop beyond the right indent lets the line go on (Word sets the text after it there).
        let mut tab_extra = 0.0f32;
        while i < units.len() {
            let first_line = lines.is_empty();
            let room = width - right - line_start_x(first_line) + tab_extra;
            match &units[i] {
                Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak => {
                    lines.push((start, i + 1, true, hang_start(hang.take(), i + 1)));
                    if i > 0 && let Unit::Char { width, auto_space, .. } = &mut units[i - 1] {
                        *width -= *auto_space;
                        *auto_space = 0.0;
                    }
                    start = i + 1;
                    x = 0.0;
                    tab_extra = 0.0;
                    last_break = None;
                    i += 1;
                    continue;
                }
                Unit::ListTab if left > line_start_x(first_line) + x + 0.01 => {
                    x = left - line_start_x(first_line);
                    i += 1;
                    continue;
                }
                Unit::Tab | Unit::ListTab => {
                    let at = line_start_x(first_line) + x;
                    let tab = p.tabs.iter().find(|t| pt(t.position) > at + 0.01);
                    let stop = tab.map(|t| pt(t.position)).unwrap_or(((at / tab_default).floor() + 1.0) * tab_default);
                    // Text after a right or centre stop ends at (or centres on) it.
                    let rest: f32 = units[i + 1..].iter().take_while(|u| !matches!(u, Unit::Tab | Unit::PositionalTab { .. } | Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak)).map(unit_width).sum();
                    let stop = match tab.map(|t| t.align) {
                        Some(TabAlign::Right) => (stop - rest).max(at),
                        Some(TabAlign::Center) => (stop - rest / 2.0).max(at),
                        _ => stop,
                    };
                    x = stop - line_start_x(first_line);
                    if let Some(t) = tab.filter(|t| t.align == TabAlign::Right) {
                        tab_extra = tab_extra.max(pt(t.position) - (width - right));
                    }
                    i += 1;
                    continue;
                }
                Unit::PositionalTab { align, relative_to_margin, .. } => {
                    let at = line_start_x(first_line) + x;
                    let rest: f32 = units[i + 1..].iter().take_while(|u| !matches!(u, Unit::Tab | Unit::PositionalTab { .. } | Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak)).map(unit_width).sum();
                    let (span_l, span_r) = if *relative_to_margin { (0.0, width) } else { (left, width - right) };
                    let stop = match align {
                        TabAlign::Right => (span_r - rest).max(at),
                        TabAlign::Center => ((span_l + span_r - rest) / 2.0).max(at),
                        _ => span_l.max(at),
                    };
                    x = stop - line_start_x(first_line);
                    if *align == TabAlign::Right && *relative_to_margin {
                        tab_extra = tab_extra.max(right);
                    }
                    i += 1;
                    continue;
                }
                _ => {}
            }
            let w = unit_width(&units[i]);
            let c = unit_char(&units[i]);
            let combined_rest = is_combined_rest(p, &units[i], w);
            // A break is allowed before this unit?
            if i > start && !combined_rest {
                let prev = unit_char(&units[i - 1]);
                // Not before full-width spaces that a closing character follows: they would start the line with it (Word's PDFs of
                // `spacecap-compat11-mincho-w41453.docx`: the character before the spaces goes down with them).
                let spaces_then_closer = c == Some('\u{3000}') && {
                    let mut j = i;
                    while j < units.len() && unit_char(&units[j]) == Some('\u{3000}') {
                        j += 1;
                    }
                    units.get(j).and_then(unit_char).is_some_and(|ch| "、。，．,）」』】］｝’”".contains(ch)) // i18n-ok（閉じ括弧・句読点の文字の集合。文字の並び）
                };
                if may_break(prev, c, p.kinsoku, p.word_wrap, self.doc) && !spaces_then_closer {
                    last_break = Some(i);
                }
            }
            // The space after a character between Japanese and Latin (`auto_space`) is not needed where the line ends after it: the character fits when
            // it does without it (Word's PDFs of `kinsoku-*-probe.docx`: a kana before a Latin word ends a line when the kana fits, GitHub issue #68 D3).
            let trailing_space = if let Unit::Char { auto_space, .. } = &units[i] { *auto_space } else { 0.0 };
            if x + w - trailing_space > room + 0.01 && i > start && !combined_rest {
                // 約物を詰める: a line a little too long takes in its stops and brackets (half an em each) before it
                // breaks.
                // Word squeezes a line to take in one more character than fits (Word's PDF of `compresspunct-mincho-w41453.docx`, with the
                // stops 、 。, brackets and the dot ・: ＭＳ 明朝 10.5 pt in 414.45 pt, 39 characters fit and the lines of text with stops and
                // brackets have 40, however many they have, and 41 only when the 41st cannot start a line (、 ） and the like): the squeeze is
                // shared by all the line's stops and brackets, as much as the line needs. A second overflowing character is taken in only when
                // it cannot start a line.
                let must_stay = i > start && !may_break(unit_char(&units[i - 1]), c, p.kinsoku, p.word_wrap, self.doc);
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
                if self.doc.compress_punctuation && squeezes && unit_char(&units[i]) == Some('\u{3000}') && !matches!(&units[i], Unit::Char { squeezed: true, .. }) {
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
                if self.doc.compress_punctuation && squeezes && (compressed_line != Some(start) || must_stay) {
                    // A stop or bracket gives up the half em it has beside its glyph, once: one that was squeezed (a kerned pair, or on an
                    // earlier pass of this line) has no more, and one narrower than that half has none to give. List numbering labels (`LABEL`)
                    // are not squeezed (`i1_leftchars_zero_keeps_numbering_indent`).
                    // A face whose stops are narrower than an em (a proportional one: Meiryo UI and ＭＳ Ｐ明朝 set 、 at 0.664 em) squeezes
                    // nothing, its brackets neither (Word's PDFs of `squeeze-proportional-*-probe.docx`, 8 pt: Meiryo UI and ＭＳ Ｐ明朝 keep 52 and
                    // 53 characters in the justified lines of mode 15 as in the left aligned ones, 游明朝 takes in one more there, 52 to 51).
                    let full_stops = |font: &Font| advance_em(self.res, Some(font.family.as_ref()), '、') > 0.99; // i18n-ok（読点の幅で書体を見分ける）
                    let squeezable = |u: &Unit| matches!(u, Unit::Char { c, width, size, style, squeeze, squeezed, font, .. } if *style != LABEL && can_give_up_half(*c, *width, *size * *squeeze, *squeezed) && full_stops(font));
                    // The closing brackets that must stay with this character count too (a stop may hang past the edge).
                    let mut end = i;
                    while end + 1 < units.len() && unit_char(&units[end + 1]).is_some_and(|ch| "）」』】〕〉》］｝".contains(ch)) { // i18n-ok（閉じ括弧の文字の集合。文字の並び）
                        end += 1;
                    }
                    let over = x + (i..=end).map(|k| unit_width(&units[k])).sum::<f32>() - room;
                    // The first character of the line that is seen, with nothing but half-width spaces before it (a first line's indent made with spaces), counts as the start of the
                    // line (Word's PDF of corpus 047 page 2: the 【 after two spaces and a first line indent is where the line's start puts it, not 5.25 pt to the left).
                    // An opening bracket that starts the line keeps its half em (Word's PDFs of `squeeze-line-start-bracket-mode*-probe.docx`,
                    // ＭＳ 明朝 10.5 pt on a line of 40.55 em, 0.45 em short of a 41st character: a line that starts with （ or 「 keeps 40 in modes 11
                    // and 15, left aligned or justified; one that starts with （（ takes in the 41st, the second bracket giving up its half).
                    let opens_line = |k: usize| (start..k).all(|j| unit_char(&units[j]) == Some(' ')) && unit_char(&units[k]).is_some_and(|ch| "（「『【〔〈《［｛".contains(ch)); // i18n-ok（始め括弧の文字の集合。文字の並び）
                    let candidates: Vec<usize> = (start..=end).filter(|&k| squeezable(&units[k]) && !opens_line(k)).collect();
                    let halves = candidates.iter().map(|&k| if let Unit::Char { size, squeeze, .. } = &units[k] { *size * *squeeze / 2.0 } else { 0.0 });
                    // Below 15 the line gives up no more than one stop's half em to take in a character that may start a line; one that
                    // cannot is taken in as in mode 15. Word's PDFs of mode 11 (ＭＳ 明朝, 414.45 pt wide, left aligned, justified and distributed,
                    // with no grid, a line grid or a grid of lines and characters, in the body or in a table's cell): at 10.5 pt the 40th kanji
                    // needs 0.53 em and the lines keep 39 however many stops they have, at 12 pt the 35th needs 0.5 em and is taken in; 028
                    // (12 pt, 489.05 pt) takes in its 41st, 0.25 em over; a ） as the 40th is taken in at 10.5 pt. A line on a character grid
                    // (a section's `linesAndChars`, the paragraph on the grid, not in a table's cell) takes in none: the 12 pt file with that
                    // grid keeps 34 a line, in a cell 35; 122 (12 pt, `linesAndChars`) keeps its lines 0.21 em short of the next character.
                    let on_char_grid = self.char_grid.get() && p.snap_to_grid && self.current_cell.get().is_none();
                    let possible: f32 = match (below_15 && !must_stay, on_char_grid) {
                        (true, true) => 0.0,
                        (true, false) => halves.fold(0.0, f32::max),
                        _ => halves.sum(),
                    };
                    if possible >= over - 0.01 {
                        let (mut left, mut before) = (over, 0.0);
                        for k in candidates {
                            if left <= 0.01 {
                                break;
                            }
                            if let Unit::Char { c, width, size, inset, squeeze, squeezed, .. } = &mut units[k] {
                                let half = *size * *squeeze / 2.0;
                                *width -= half;
                                *squeezed = true;
                                if "（「『【〔〈《［｛".contains(*c) { // i18n-ok（始め括弧の文字の集合。文字の並び）
                                    *inset -= half;
                                } else if *c == '・' {
                                    // The dot is in the middle of its em: the squeeze takes the same from both sides (Word's PDF).
                                    *inset -= half / 2.0;
                                }
                                left -= half;
                                if k < i {
                                    before += half;
                                }
                            }
                        }
                        x -= before;
                        compressed_line = Some(start);
                        continue;
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
                if self.doc.compat_mode.is_some_and(|m| m >= 15) && matches!(p.align, FlowAlign::Justify | FlowAlign::Distribute) {
                    // The overflowing character's own space after it ends the line and is not needed (as above); the limit is of the line's spaces
                    // as they were before the characters already taken squeezed them (GitHub issue #68 D2, Word's PDF of `kinsoku-both-normal-probe.docx`
                    // W050 and W083: 「…る1990–2000」 and 「…ろe-mail」 end the justified line, 1.2 pt over, with the one space before the word).
                    let over = x + w - trailing_space - room;
                    let gap_of = |k: usize| if let Unit::Char { auto_space, .. } = &units[k] { *auto_space } else { 0.0 };
                    let gaps: Vec<usize> = (start..i).filter(|&k| gap_of(k) > 0.0).collect();
                    let all: f32 = gaps.iter().map(|&k| gap_of(k)).sum();
                    let cut_before = if gaps_cut.0 == start { gaps_cut.1 } else { 0.0 };
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
                        gaps_cut = (start, cut_before + before);
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
                if c == Some(' ') && !first_space_must_fit {
                    hang.get_or_insert(i);
                    x += w;
                    i += 1;
                    continue;
                }
                // Full-width spaces that end the paragraph hang too.
                if c == Some('\u{3000}') && units[i..].iter().all(|u| matches!(unit_char(u), Some('\u{3000}'))) {
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
                let hangs = p.kinsoku && (self.doc.compat_mode.is_none_or(|m| m < 15) || matches!(p.align, FlowAlign::Justify | FlowAlign::Distribute));
                let hanging = if hangs { "、。，．,.）」』】］｝’”｡､" } else { "" }; // i18n-ok（ぶら下げる文字の集合。文字の並び）
                // A justified line (mode 15) that took its last word by narrowing its spaces (below) has its closing punctuation hang as well: the
                // line's own overflow is within what the spaces give (Word's PDF of corpus 124: the curly quote that closes "This Content" ends a line).
                let hang_allow = if matches!(p.align, FlowAlign::Justify) && self.doc.compat_mode.is_none_or(|m| m >= 15) && p.overflow_punct && c.is_some_and(|ch| hanging.contains(ch)) {
                    let we = (i..units.len()).find(|&k| unit_char(&units[k]).is_none_or(|ch| ch == ' ')).unwrap_or(units.len());
                    justify_overflow_room(&units, start, i, we)
                } else {
                    0.0
                };
                if p.overflow_punct && self.current_cell.get().is_none() && c.is_some_and(|ch| hanging.contains(ch)) && x <= room + 0.01 + hang_allow { // i18n-ok（オーバーフロー句読点・閉じ括弧の文字の集合。文字の並び。Word の PDF では行末の「】」もぶら下がる）
                    hang.get_or_insert(i);
                    x += w;
                    i += 1;
                    continue;
                }
                if matches!(p.align, FlowAlign::Justify | FlowAlign::Distribute)
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
                    let has_more = units[word_end..]
                        .iter()
                        .take_while(|u| !matches!(u, Unit::LineBreak | Unit::PageBreak | Unit::ColumnBreak))
                        .any(|u| unit_char(u).is_some_and(|ch| ch != ' '));
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
                lines.push((start, at, false, hang_start(hang.take(), at)));
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
                start = at;
                x = 0.0;
                tab_extra = 0.0;
                last_break = None;
                i = at;
                continue;
            }
            x += w;
            i += 1;
        }
        // A paragraph that ends with a page break has its mark on the next page, an empty line there, with `splitPgBreakAndParaMark` below
        // mode 15 (Word's PDFs of `split-page-break-mark-*-probe.docx`: a page break then an empty paragraph that ends a section leaves a blank
        // page in mode 11 with the flag, as Word lays out the same document saved as a `.doc`, and none without it or in mode 15).
        let mark_after_break = self.doc.split_pg_break_and_para_mark
            && self.doc.compat_mode.is_none_or(|m| m < 15)
            && start == units.len()
            && matches!(units.last(), Some(Unit::PageBreak));
        if start < units.len() || lines.is_empty() || mark_after_break {
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
        let mut out = Vec::new();
        for (n, &(s, e, forced, hang_at)) in lines.iter().enumerate() {
            let slice = &units[s..e];
            let largest = slice.iter().filter_map(|u| if let Unit::Char { size, .. } = u { Some(*size) } else { None }).fold(0.0, f32::max);
            let largest = if largest > 0.0 { largest } else { mark_size };
            let largest_raise = slice.iter().filter_map(|u| if let Unit::Char { size, style, .. } = u { Some(if p.runs.get(*style).is_some_and(|r| *style != LABEL && r.style.position != VerticalPosition::Baseline) { *size / 0.65 } else { *size }) } else { None }).fold(0.0, f32::max).max(largest);
            // The line's text height and ascent: the tallest typeface's (the
            // paragraph mark's on an empty line).
            let spaces_only = slice.iter().all(|u| !matches!(u, Unit::Char { c, .. } if !c.is_whitespace()));
            let (text_line, text_ascent, lead) = slice.iter().filter_map(|u| if let Unit::Char { c, line, ascent, lead, style, size, .. } = u { if !spaces_only && c.is_whitespace() { return None; } let (line, ascent) = mark_room(p, *c, *style, *size, *line, *ascent); Some((line, ascent, *lead)) } else { None }).fold((0.0f32, 0.0f32, 0.0f32), |(l, a, d), (l2, a2, d2)| (l.max(l2), a.max(a2), d.max(d2)));
            let (text_line, text_ascent, lead) = if text_line > 0.0 { (text_line, text_ascent, lead) } else { mark_metrics };
            let picture = slice.iter().filter_map(|u| match u { Unit::Picture { h, .. } | Unit::Shape { h, .. } | Unit::Math { h, .. } => Some(*h), _ => None }).fold(0.0, f32::max);
            let natural = text_line.max(picture);
            let lead = if picture > text_line { 0.0 } else { lead };
                let height = self.line_height_around(p, text_line, picture, natural, lead, in_table);
            // Where Word puts the baseline in a line: see `baseline_in_line`.
            let math_below = slice.iter().filter_map(|u| if let Unit::Math { descent, .. } = u { Some(*descent) } else { None }).fold(0.0f32, f32::max);
            let baseline = if picture > text_line { height - math_below } else { self.baseline_in_line(p, natural, lead, text_line, text_ascent, in_table, self.ms_room_of(p, slice.iter())) };
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
            let extras = if justify && !has_tab && (used < room || p.align == FlowAlign::Justify) { spread_extras(&slice[..visible_end], room - used, p.align == FlowAlign::Distribute, marks) } else { Vec::new() };
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
                    let s_size = s.enclosure_sizes.map_or(s.size, |(ring, _)| ring) as f32 / 100.0 * if s.position == VerticalPosition::Baseline { 1.0 } else { 0.65 };
                    let raise = match s.position {
                        VerticalPosition::Superscript => -largest_raise * 0.35,
                        VerticalPosition::Subscript => largest_raise * 0.1,
                        VerticalPosition::Baseline => 0.0,
                    };
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
                        let pad_v = pt(b.space);
                        let extra_bw = (pt(b.total()) - pt(b.width)).max(0.0);
                        let y_top = (y_base - run_ascent - 1.2 - pad_v).max(0.0);
                        let y_bottom = y_base + (run_line - run_ascent) + 2.8 + pad_v + 2.0 * extra_bw;
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
                                && (last_u.size - u_size).abs() < 0.01
                                && (last_u.x + last_u.w - ux).abs() < 0.02
                            {
                                last_u.w = (ux + uw) - last_u.x;
                            } else {
                                dec.underlines.push(UnderlineBox {
                                    style: s.underline_style,
                                    size: u_size,
                                    x: ux,
                                    w: uw,
                                    baseline: y_base,
                                    color: ucolor,
                                });
                            }
                        }
                    }
                    if (s.strike || s.double_strike) && end > 0.0 {
                        let strike_size = s_size;
                        let lines: Vec<(f32, f32)> = if s.double_strike {
                            let sh = (strike_size / 13.0).max(0.4);
                            vec![(y_base - strike_size * 0.3 - strike_size * 0.08, sh), (y_base - strike_size * 0.3 + strike_size * 0.08, sh)]
                        } else {
                            vec![(y_base - strike_size * 0.3, (strike_size / 18.0).max(0.4))]
                        };
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
                    line.items.extend(self.emphasis_marks(s, style == LABEL, x0, y_base, s_size, &text, &offsets, color));
                    if let Some(run) = p.runs.get(style).filter(|_| style != LABEL) {
                        line.items.extend(self.enclosed_items(s, run, x0, y_base, end, s_size, color));
                        line.items.extend(self.overstrike_items(s, run, x0, y_base, end, s_size, color));
                    }
                    if let Some(run) = p.runs.get(style).filter(|_| style != LABEL) {
                        line.items.extend(self.ruby_items(s, &run.kind, x0, y_base, end, color));
                    }
                    let combined = p.runs.get(style).filter(|_| style != LABEL).and_then(|r| r.style.combine);
                    if let Some(c) = combined {
                        line.items.extend(self.combine_items(s, c, &text, x0, y_base, s_size, color));
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
                        let same = run.as_ref().is_some_and(|r| r.5 == *style && (tate_run || (r.3 == *font && r.4 == *size && r.6 == *squeeze)));
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
                        let (span_l, span_r) = if *relative_to_margin { (0.0, width) } else { (left, width - right) };
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
                    Unit::Picture { media, w, h } => {
                        flush(&mut run, &mut line, &mut dec);
                        line.items.push(Item::Picture { media: *media, x, y: line.baseline - h, w: *w, h: *h });
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
            dec.finish(&mut line);
            // Where the line's text ends: the next line's first position.
            let end = positions.get(e).map_or(p.len(), |&(position, _)| position);
            stops.push((end, x - line_x));
            line.items.push(Item::Line { para: p.id, x: line_x, y: 0.0, h: line.height, stops: stops.into(), vertical: false });
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

#[cfg(test)]
mod can_give_up_half_tests {
    use super::can_give_up_half;

    #[test]
    fn a_fresh_stop_or_bracket_gives_up_its_half_em() {
        for c in "、。，．・（）「」『』【】〔〕〈〉《》［］｛｝".chars() {
            assert!(can_give_up_half(c, 10.5, 10.5, false), "{c}");
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

/// A character's line height and ascent with room for its 傍点 above
/// (`w:em` dot, comma, circle): Word's line grows by 0.36 em over Japanese
/// and 0.309 em over Latin letters (its PDF of DRUN at 14 pt: 5.04 pt for
/// 傍点の試験, 4.32 pt for Emphasis).
fn mark_room(p: &FlowParagraph, c: char, style: usize, size: f32, line: f32, ascent: f32) -> (f32, f32) {
    use ibis_doc_model::EmphasisMark;
    // ルビ: room for it over its base (its ascent, as the base's in its
    // size, `raise` above the baseline; Word's PDF of DW14).
    if let Some(FlowRunKind::Ruby { size: rs, raise, .. }) = p.runs.get(style).map(|r| &r.kind)
        && size > 0.0
    {
        let raised = ascent.max(*raise as f32 / 100.0 + ascent * (*rs as f32 / 100.0) / size);
        return (line + raised - ascent, raised);
    }
    let above = p.runs.get(style).is_some_and(|r| matches!(r.style.emphasis, EmphasisMark::Dot | EmphasisMark::Comma | EmphasisMark::Circle));
    if !above {
        return (line, ascent);
    }
    let extra = if is_cjk(c) { 0.36 } else { 0.3086 } * size;
    (line + extra, ascent + extra)
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
    left_border: f32,
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
        let dx = self.left + m3 + self.left_border;
        if self.direction.vertical() {
            let by = self.cell_top + self.border + m0;
            let bw = (self.right - self.left - m1 - m3).max(0.0);
            let bh = (cell_h - m0 - m2 - self.border).max(0.0);
            turn_cell_text(&self.items, self.direction, self.anchor, (dx, by, bw, bh), self.across)
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
    /// The content, placed in the row (inside the borders and margins).
    content: Vec<Item>,
    /// From the row's top to the content (top border and margin), and from
    /// the content to the row's bottom (margin).
    top: f32,
    bottom: f32,
    /// Top, right, bottom and left, as drawn (after the neighbours').
    borders: [Option<FlowBorder>; 4],
}

/// A vertical cell's text laid out across (`items`, lines `across` thick
/// in all) turned into the cell's box (`x`, `y`, `w`, `h`): down with the
/// lines right to left (`TbRl`; Japanese upright, the rest turned
/// clockwise), down with the lines left to right (`TbLrV`), or up with
/// them left to right (`BtLr`, all turned anticlockwise); its vertical
/// alignment is across the lines (Word's PDF of DCTD: 上 is right for
/// `TbRl`, left for the others).
fn turn_cell_text(items: &[Item], direction: ibis_doc_model::CellDirection, anchor: CellAnchor, (bx, by, bw, bh): (f32, f32, f32, f32), across: f32) -> Vec<Item> {
    use ibis_doc_model::CellDirection;
    let room = (bw - across).max(0.0);
    let shift = match anchor {
        CellAnchor::Top => 0.0,
        CellAnchor::Center => room / 2.0,
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
            if c.is_whitespace() || c == '\u{200b}' {
                continue;
            }
            let (u, v) = (x + dx, *y);
            let s: SharedString = c.to_string().into();
            match direction {
                CellDirection::TbRl | CellDirection::TbLrV => {
                    // Down: along the cell from its top; across from its
                    // right (lines right to left) or left.
                    let line_x = match direction {
                        CellDirection::TbRl => bx + bw - shift - v,
                        _ => bx + shift + (across - v),
                    };
                    if crate::render::vertical_text::upright(c) {
                        // OpenType vert/vrt2 selects the font’s vertical glyph.
                        let font=if crate::render::vertical_text::vertical_glyph(c){crate::render::vertical_text::font(font)}else{font.clone()};
                        // Upright, centred on the line.
                        let mid = line_x + size * 0.35;
                        // 、。 take their vertical forms: the upper right of the box.
                        let (dx, dy) = (0.0, 0.0);
                        // A squeezed one keeps its squeeze across the line.
                        if (*xscale - 1.0).abs() > 0.001 {
                            out.push(Item::Scaled { x: mid - size / 2.0 + dx, y: by + u + size * 0.88 + dy, text: s, font: font.clone(), size: *size, color: *color, xscale: *xscale });
                        } else {
                            out.push(Item::Text { x: mid - size / 2.0 + dx, y: by + u + size * 0.88 + dy, text: s, offsets: Arc::from([0.0f32]), font: font.clone(), size: *size, color: *color });
                        }
                    } else {
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
                    items.extend(border_items(&b, horizontal, outer_first, at, from, to, false, false));
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
        let piece = |items: Vec<Item>, height: f32, split: Option<Arc<RowSplit>>| Piece { height, items, page_break: false, column_break: false, keep_with_next: false, header: false, split, ..Default::default() };
        let rest_split = RowSplit { cells: self.cells.iter().zip(&rest).map(|(c, r)| SplitCell { content: r.clone(), ..c.clone() }).collect(), min_height: remaining };
        Some((piece(self.draw(&first, first_h), first_h, None), piece(self.draw(&rest, rest_h), rest_h, Some(Arc::new(rest_split)))))
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
                                    items.extend(border_items(&b, true, true, before + pt(b.total()) / 2.0, lx, rx, left.is_some(), right.is_some()));
                                }
                                if let Some(b) = bottom.filter(|_| i + 1 == count) {
                                    items.extend(border_items(&b, true, false, y1 + pt(b.total()) / 2.0, lx, rx, left.is_some(), right.is_some()));
                                }
                                // The side borders go over the space before and after
                                // when no top or bottom border bounds it (Word's PDF of
                                // DWID: one rectangle per paragraph, end to end).
                                let ty = if i == 0 { if top.is_none() { 0.0 } else { before } } else { 0.0 };
                                let by = if i + 1 == count { y1 + bottom.map_or(after, |b| pt(b.total())) } else { before + above + l.height };
                                if let Some(b) = left {
                                    items.extend(border_items(&b, false, true, lx, ty, by, false, false));
                                }
                                if let Some(b) = right {
                                    items.extend(border_items(&b, false, false, rx, ty, by, false, false));
                                }
                            }
                            let from_paragraph_top = if i == 0 { 0.0 } else { first_line_top + line_tops[i] };
                            items.extend(l.items.into_iter().map(|it| match &it {
                                Item::Shape { shape, .. } if self.doc.compat_mode.is_some_and(|m| m >= 15) && shape.floating.as_ref().is_some_and(|f| f.relative_v == "paragraph" && f.align_v.is_none()) => offset(it, 0.0, -from_paragraph_top),
                                _ => offset(it, 0.0, before + above),
                            }));
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
                        Some("center") => base_x0 + (base_w - sw) / 2.0,
                        Some("right") => base_x0 + base_w - sw,
                        Some("inside") => if is_even { base_x0 + base_w - sw } else { base_x0 },
                        Some("outside") => if is_even { base_x0 } else { base_x0 + base_w - sw },
                        _ => base_x0 + pt(f.offset_x),
                    };
                    let abs_x = abs_x.clamp(0.0, (page_w - sw).max(0.0));
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
                                items.extend(border_items(&b, true, false, y1 + pt(b.total()) / 2.0, lx, rx, left.is_some(), right.is_some()));
                            }
                            let (ty, by) = (0.0, if i + 1 == count { y1 + bottom.map_or(0.0, |b| pt(b.total())) } else { l.height });
                            if let Some(b) = left {
                                items.extend(border_items(&b, false, true, lx, ty, by, false, false));
                            }
                            if let Some(b) = right {
                                items.extend(border_items(&b, false, false, rx, ty, by, false, false));
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
                                    starts = false;
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
                                    starts = false;
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
        let target = match fit.width {
            PreferredWidth::Fixed(w) => Some(pt(w)),
            PreferredWidth::Percent(p) if available < f32::MAX / 8.0 => Some(available * p as f32 / 5000.0),
            _ => None,
        };
        // A cell's preferred width in points (a percent of the table's).
        let base = target.unwrap_or(available);
        let prefer = |w: PreferredWidth| match w {
            PreferredWidth::Fixed(w) => Some(pt(w)),
            PreferredWidth::Percent(p) if base < f32::MAX / 8.0 => Some(base * p as f32 / 5000.0),
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
                let (lo, hi, held) = if cell.merge == VerticalMerge::Continue { (0.0, 0.0, 0.0) } else { self.content_widths(&cell.blocks, page_number) };
                self.current_cell.set(None);
                // Per ECMA-376 §17.4.42 (`w:noWrap`), a cell with an explicit width (`Fixed` / `Percent`) wraps at that width.
                let lo = if cell.no_wrap && matches!(*w, PreferredWidth::Auto) { hi } else { lo };
                // (A cell of vertical text has its indents along its length, not across: the room of its width is its lines'.)
                let held = if cell.direction.vertical() { lo } else { held };
                let (lo, hi, held) = (lo + margins, hi + margins, held + margins);
                if span == 1 {
                    least[col] = least[col].max(lo);
                    held_least[col] = held_least[col].max(held);
                    most[col] = most[col].max(hi);
                    if let Some(p) = prefer(*w) {
                        preferred[col] = Some(preferred[col].map_or(p, |q: f32| q.max(p)));
                    }
                } else {
                    spanning.push((col, span, lo, prefer(*w).unwrap_or(hi).max(lo)));
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
            None if total > available => available,
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
        // Up to Word's 1/300 inch (the edges are rounded to it), so that
        // what fits is not broken by the rounding.
        Some(widths.into_iter().map(|w| (w / 0.24 - 0.01).ceil() * 0.24).collect())
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
            _ => pt(t.indent),
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
                        drop_cap_to_emit: None,
                    });
                    let pieces = if natural || obstacle.is_some() { self.lay_out_block(block, inner, page_number, obstacle.as_slice(), true) } else { self.pieces(block, inner, page_number, &[], true) };
                    for piece in pieces {
                        let own = matches!(block, Block::Paragraph(_));
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
        let mut heights: Vec<f32> = t
            .rows
            .iter()
            .zip(&grid)
            .map(|(row, cells)| {
                // Word counts the width of a cell's top border into its row.
                let content = row
                    .cells
                    .iter()
                    .zip(cells)
                    .filter(|(c, _)| c.merge == VerticalMerge::None)
                    .map(|(c, l)| l.content + pt(c.margins[0]) + pt(c.margins[2]) + c.borders[0].map_or(0.0, |b| pt(b.width)) + spacing)
                    .fold(0.0, f32::max);
                // A minimum height does not count the top border either.
                let border = row.cells.iter().filter_map(|c| c.borders[0]).map(|b| pt(b.width)).fold(0.0, f32::max);
                match row.height {
                    RowHeight::Exact(h) => pt(h),
                    RowHeight::AtLeast(h) => (pt(h) + border + spacing).max(content),
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
                let border = cell.borders[0].map_or(0.0, |b| pt(b.width));
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
            // diagonals.
            let divisible = !row.cant_split
                && !row.header
                && !matches!(row.height, RowHeight::Exact(_))
                && row.cells.iter().all(|c| c.merge == VerticalMerge::None && c.diagonals.iter().all(Option::is_none) && c.anchor == CellAnchor::Top);
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
                    let border = cell.borders[0].map_or(0.0, |b| if b.art.is_some() { pt(b.width) * 8.0 } else { pt(b.width) });
                    let room = cell_h - pt(cell.margins[0]) - pt(cell.margins[2]) - border;
                    let dy = cell_top
                        + border
                        + pt(cell.margins[0])
                        + match cell.anchor {
                            CellAnchor::Top => 0.0,
                            CellAnchor::Center => ((room - laid.content) / 2.0).max(0.0),
                            CellAnchor::Bottom => (room - laid.content).max(0.0),
                        };
                    // The text starts inside the cell's left border.
                    let dx = left + pt(cell.margins[3]) + cell.borders[3].map_or(0.0, |b| pt(b.width));
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
                            left_border: cell.borders[3].map_or(0.0, |b| pt(b.width)),
                            margins: [pt(cell.margins[0]), pt(cell.margins[1]), pt(cell.margins[2]), pt(cell.margins[3])],
                            spacing,
                            rows: heights[r..(r + n).min(heights.len())].to_vec(),
                        }));
                    } else if cell.direction.vertical() {
                        // The cell's box inside its margins.
                        let bx = left + pt(cell.margins[3]) + cell.borders[3].map_or(0.0, |b| pt(b.width));
                        let by = cell_top + border + pt(cell.margins[0]);
                        let bw = (right - left - pt(cell.margins[1]) - pt(cell.margins[3])).max(0.0);
                        let bh = (cell_h - pt(cell.margins[0]) - pt(cell.margins[2]) - border).max(0.0);
                        cell_items.extend(turn_cell_text(&laid.items, cell.direction, cell.anchor, (bx, by, bw, bh), laid.across));
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
                        split_cells.push(SplitCell {
                            left,
                            right,
                            fill: cell.shading.or(cell.style_fill).map(|f| hsla(ibis_doc_model::Color::opaque(f))),
                            content: laid.items.iter().cloned().map(|it| offset(it, dx, dy)).collect(),
                            top: dy,
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
                    items.extend(border_items(&b, horizontal, outer_first, at, from, to, extend_start, extend_end));
                }
            }
            let min_height = if let RowHeight::AtLeast(h) = row.height { pt(h) + row.cells.iter().filter_map(|c| c.borders[0]).map(|b| pt(b.width)).fold(0.0, f32::max) + spacing } else { 0.0 };
            let split = (divisible && split_cells.len() == row.cells.len()).then(|| Arc::new(RowSplit { cells: split_cells, min_height }));
            pieces.push(Piece { height: heights[r], items, page_break: false, column_break: false, keep_with_next: false, header: row.header && t.rows[..r].iter().all(|r| r.header), split, table_bounds: Some((x0, x0 + total)), anchored, ..Default::default() });
        }
        pieces
    }

    /// A paragraph's space before and after, less what contextual spacing
    /// takes away.
    fn space_before(&self, p: &FlowParagraph) -> f32 {
        if self.contextual.get(&p.id).is_some_and(|c| c.0) { 0.0 } else { pt(p.space_before) }
    }

    fn space_after(&self, p: &FlowParagraph) -> f32 {
        if self.contextual.get(&p.id).is_some_and(|c| c.1) { 0.0 } else { pt(p.space_after) }
    }

    fn lay_out(&self) -> Pagination {
        use ibis_doc_model::flow_command;
        // The sections: their setup and what their headers and footers show.
        let ids = flow_command::sections(self.doc);
        let mut setups: Vec<ibis_doc_model::SectionSetup> = ids.iter().map(|id| flow_command::section_setup(self.doc, *id).unwrap_or_default()).collect();
        // 縦書き: a section of vertical writing is laid out as across text on
        // a page turned a quarter (see `turn_page_items`).
        let directions: Vec<ibis_doc_model::CellDirection> = setups.iter().map(|s| s.page.direction).collect();
        for setup in &mut setups {
            use ibis_doc_model::CellDirection;
            let page = &mut setup.page;
            let (t, r, b, l) = (page.top, page.right, page.bottom, page.left);
            match page.direction {
                CellDirection::TbRl => (page.top, page.right, page.bottom, page.left) = (r, b, l, t),
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
        for (s, setup) in setups.iter_mut().enumerate() {
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
        let frame_for = |s: usize, first: bool| {
            let mut f = frame_of(s);
            if first {
                f.top += first_page_drop[s];
            }
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
        let note_width = frame_of(0).width;
        let note_pieces = |endnote: bool, id: u32| -> Vec<Piece> {
            let notes = if endnote { &self.doc.endnotes } else { &self.doc.footnotes };
            let Some(note) = notes.iter().find(|n| n.id == id) else { return Vec::new() };
            *self.current_note.borrow_mut() = self.note_numbers.get(&(endnote, id)).cloned().unwrap_or_default();
            note.blocks.iter().flat_map(|b| self.pieces(b, note_width, 1, &[], true)).collect()
        };
        let mut note_heights: std::collections::HashMap<u32, f32> = std::collections::HashMap::new();
        for ids in marks.values() {
            for (_, id) in ids {
                if !note_heights.contains_key(id) {
                    let h = note_pieces(false, *id).iter().map(|p| p.height).sum();
                    note_heights.insert(*id, h);
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
        let notes_room = |page: &Page| -> f32 { if page.notes.is_empty() { 0.0 } else { separator_h + page.notes.iter().map(|id| note_heights.get(id).copied().unwrap_or(0.0)).sum::<f32>() } };
        let mut pages: Vec<Page> = vec![page_of(0, setups[0].page.first_page, true)];
        let mut frame = frame_for(0, true);
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
            *frame = frame_for(s, first);
            *y = frame.top;
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
                        found.push(self.float_box(shape, &geometry, geometry.margin_t, 0.0).obstacle());
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
                col = 0;
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
            // A section that starts a page (after a page-breaking one) takes
            // over the page it starts on.
            if pages.last().is_some_and(|pg| pg.section != s && pg.items.iter().all(|it| matches!(it, Item::Line { .. }))) && y <= frame.top + 0.01 {
                let first = true;
                number = if setups[s].restart_numbering { setups[s].page.first_page } else { number };
                *pages.last_mut().expect("a page") = page_of(s, number, first);
                frame = frame_for(s, first);
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
                    let mut boxes: Vec<FloatBox> = shapes.iter().map(|shape| self.float_box(shape, &FloatGeometry::new(&setups[s].page, frame.width, number), para_top, line_h)).collect();
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
                        boxes = shapes.iter().map(|shape| self.float_box(shape, &FloatGeometry::new(&setups[s].page, frame.width, number), para_top, line_h)).collect();
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
            if let Block::Table(t) = block
                && let Some(f) = t.floating
            {
                use ibis_doc_model::{TableAlign, TableAnchor};
                let mut pieces = self.pieces(block, frame.width, number, &[], false);
                place_anchored_on_one_page(&mut pieces);
                let h: f32 = pieces.iter().map(|p| p.height).sum();
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
                    None => base_x + pt(f.x) - 0.24,
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
                let dx = left - lo - 0.9;
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
                let (top, h) = (chunk_top, at - chunk_top);
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
                    y -= after.min(piece.space_before);
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
                            kept += next_pieces.first().map_or(0.0, |n| n.height);
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
                let room = notes_room(page);
                let more = new_notes.iter().map(|id| note_heights.get(id).copied().unwrap_or(0.0)).sum::<f32>() + if page.notes.is_empty() && !new_notes.is_empty() { separator_h } else { 0.0 };
                // A line with only a page break, or an empty paragraph that
                // only ends a section, stays on the page though it does not
                // fit (Word does not make a blank page of it).
                let break_only = matches!(block, Block::Paragraph(p) if (p.section_break != SectionBreak::None || piece.page_break))
                    && piece.items.iter().all(|it| matches!(it, Item::Line { .. }));
                // The space after a paragraph that ends the page does not need room (Word keeps the
                // paragraph there when its lines fit: 016, an empty paragraph after a table).
                if page_foot(y, piece.height, piece.space_after, piece.grid_below, kept) > frame.bottom - room - more + 0.01 && !break_only && (y > col_top + 0.01 || col_top > frame.top + 0.01 || col + 1 < columns_of(s).len()) {
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
                let page = pages.last_mut().expect("a page");
                for id in new_notes {
                    if !page.notes.contains(&id) {
                        page.notes.push(id);
                    }
                }
                let anchored_here = std::mem::take(&mut piece.anchored);
                let items = piece.items.into_iter().map(|it| place_item(it, y, s, number, frame.left));
                let start = page.items.len();
                page.items.extend(items);
                placed.push((start..page.items.len(), y, piece.height, col));
                waiting.extend(anchored_here.into_iter().map(|c| (block_rows.len(), pages.len() - 1, y, c)));
                block_rows.push((pages.len() - 1, y, piece.height));
                if popped == 1 && let Some(at) = self.page_anchors.borrow_mut().get_mut(&b) {
                    *at = pages.len() - 1;
                }
                y += piece.height;
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
                if piece.page_break && !breaks_again {
                    // The page after a section's end is the next section's.
                    let s_next = if ends_section { (s + 1).min(setups.len() - 1) } else { s };
                    new_page!(s_next);
                    active_obstacle = find_page_obstacle(s_next, number, &frame, pages.len() - 1);
                    // An odd- or even-page section break: a blank page
                    // between when the next page has the other parity.
                    let wanted = match block {
                        Block::Paragraph(p) if p.section_break == SectionBreak::OddPage => Some(1),
                        Block::Paragraph(p) if p.section_break == SectionBreak::EvenPage => Some(0),
                        _ => None,
                    };
                    if wanted.is_some_and(|w| number % 2 != w) {
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
            if pg.notes.is_empty() {
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
                    let spacing = 12.0;
                    (body_bottom + spacing).min(frame.bottom - notes_room(pg))
                }
                _ => frame.bottom - notes_room(pg),
            };
            if let Some(w) = separator_width(self.doc.note_separators[0], frame.width) {
                pg.items.push(Item::Rect { x: frame.left, y: y + separator_h / 2.0, w, h: 0.5, color: hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::BLACK)) });
            }
            y += separator_h;
            for id in pg.notes.clone() {
                for piece in note_pieces(false, id) {
                    pg.items.extend(piece.items.into_iter().map(|it| offset(it, frame.left, y)));
                    y += piece.height;
                }
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
        let mut chapter_now: Option<String> = None;
        let chapter_prefixes: Vec<String> = pages.iter().map(|pg| {
            let Some((level, sep)) = &setups[pg.section].page_chapter else { return String::new() };
            let starts = pg.items.iter().find_map(|item| match item {
                Item::Line { para, stops, .. } if stops.first().is_some_and(|s| s.0 == 0) && heading_levels.get(para) == Some(level) => Some(*para),
                _ => None,
            });
            if let Some(id) = starts {
                chapter_now = self.labels.get(&id).cloned();
            }
            let separator = match sep.as_str() {
                "period" => '.',
                "colon" => ':',
                "emDash" => '\u{ff0d}',
                _ => '-',
            };
            chapter_now.as_ref().map_or(String::new(), |l| format!("{l}{separator}"))
        }).collect();
        for ((pg, &in_section), chapter) in pages.iter_mut().zip(&section_pages).zip(&chapter_prefixes) {
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
            let (header, footer) = if setup.title_page && pg.first_in_section {
                (&parts[s][2], &parts[s][3])
            } else if self.doc.even_and_odd && n % 2 == 0 {
                (&parts[s][4], &parts[s][5])
            } else {
                (&parts[s][0], &parts[s][1])
            };
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
                    pg.items.extend(border_items(&x, true, true, t, l, r, true, true));
                }
            }
            if let Some(x) = bottom {
                if let Some(art) = x.art {
                    draw_art_edge(&x, art, ArtPart::Bottom, l, r, b, true, &mut pg.items);
                } else {
                    pg.items.extend(border_items(&x, true, false, b, l, r, true, true));
                }
            }
            if let Some(x) = left {
                if let Some(art) = x.art {
                    draw_art_edge(&x, art, ArtPart::Left, t, b, l, false, &mut pg.items);
                } else {
                    pg.items.extend(border_items(&x, false, true, l, t, b, true, true));
                }
            }
            if let Some(x) = right {
                if let Some(art) = x.art {
                    draw_art_edge(&x, art, ArtPart::Right, t, b, r, false, &mut pg.items);
                } else {
                    pg.items.extend(border_items(&x, false, false, r, t, b, true, true));
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
                pg.items = turn_page_items(std::mem::take(&mut pg.items), direction, w, h);
                (pg.width, pg.height) = (w, h);
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

fn border_items(
    b: &FlowBorder,
    horizontal: bool,
    outer_first: bool,
    at: f32,
    from: f32,
    to: f32,
    extend_start: bool,
    extend_end: bool,
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
        let t = pt(width).max(0.25);
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
        Item::Line { para, x, y, h, stops, vertical } => Item::Line { para, x: x + dx, y: y + dy, h, stops, vertical },
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
    // (again at each page is counted on, as continuous).
    let (mut f, mut e) = (0u32, 0u32);
    let (mut f_section, mut e_section) = (usize::MAX, usize::MAX);
    order
        .into_iter()
        .map(|(endnote, id, s)| {
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
    fn visit(blocks: &[Block], in_cell: bool, out: &mut std::collections::HashMap<u32, (bool, bool)>) {
        for (i, block) in blocks.iter().enumerate() {
            match block {
                Block::Paragraph(p) => {
                    // The automatic space (`w:beforeAutospacing`) is none at the start of a cell, nor after at its end.
                    let (mut before, mut after) = (in_cell && i == 0 && p.auto_before, in_cell && i + 1 == blocks.len() && p.auto_after);
                    if p.contextual_spacing {
                        let same = |q: Option<&Block>| matches!(q, Some(Block::Paragraph(q)) if q.style == p.style);
                        before |= i > 0 && same(blocks.get(i - 1));
                        after |= same(blocks.get(i + 1));
                    }
                    if before || after {
                        out.insert(p.id, (before, after));
                    }
                }
                Block::Table(t) => {
                    for cell in t.rows.iter().flat_map(|r| &r.cells) {
                        visit(&cell.blocks, true, out);
                    }
                }
            }
        }
    }
    let mut out = std::collections::HashMap::new();
    for blocks in [&doc.body, &doc.header, &doc.footer, &doc.header_first, &doc.footer_first, &doc.header_even, &doc.footer_even] {
        visit(blocks, false, &mut out);
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

fn paginate_collapsed(doc: &FlowDocument, res: &Resources) -> Pagination {
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
            char_grid: std::cell::Cell::new(ids.first().and_then(|id| ibis_doc_model::flow_command::section_setup(doc, *id)).is_some_and(|s| char_grid_of(&s.page))),
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
    /// Pagination of the documents open, by the document (held so its
    /// address is not reused).
    static PAGINATIONS: std::cell::RefCell<Vec<(Arc<FlowDocument>, Arc<Pagination>, u64, bool, bool, bool, Arc<std::collections::BTreeSet<u32>>, Option<Arc<std::collections::HashMap<String, String>>>, MarkupView)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The pages of `doc`, laid out once.
pub(crate) fn pages(doc: &Arc<FlowDocument>, res: &Resources) -> Arc<Pagination> {
    if res.edited_box.as_ref().is_some_and(|(_, _, c)| c.is_some()) {
        let mut p = paginate(doc, res);
        order_floating_shapes(&mut p);
        add_watermark(doc, &mut p);
        return Arc::new(p);
    }
    PAGINATIONS.with(|cache| {
        let mut cache = cache.borrow_mut();
        // Laid out again when art borders' pictures have come since (the
        // web version fetches them).
        let generation = crate::docx::art_border::generation();
        cache.retain(|(_, _, g, _, _, _, _, _, _)| *g == generation);
        if let Some((_, p, _, _, _, _, _, _, _)) = cache.iter().find(|(d, _, _, pr, ph, pm, co, mv, mk)| Arc::ptr_eq(d, doc) && *pr == res.print && *ph == res.print_hidden_text && *pm == res.print_markup && **co == *res.collapsed_headings && *mv == res.merge_values && *mk == res.markup_view) {
            return p.clone();
        }
        let started = crate::debug_timing::start();
        let mut p = paginate(doc, res);
        order_floating_shapes(&mut p);
        add_watermark(doc, &mut p);
        let p = Arc::new(p);
        crate::debug_timing::report(&format!("paginate ({} pages)", p.pages.len()), started);
        if cache.len() >= 4 {
            cache.remove(0);
        }
        cache.push((doc.clone(), p.clone(), generation, res.print, res.print_hidden_text, res.print_markup, res.collapsed_headings.clone(), res.merge_values.clone(), res.markup_view.clone()));
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
pub(crate) fn update_fields_on_open(presentation: &mut ibis_doc_model::Presentation, res: &Resources) {
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
            if flow_command::apply_to(d, FlowCommand::SetToc { before: at, replace: true, entries, bookmarks }).is_err() {
                break;
            }
            at = new_first;
        }
    }
}

/// Give a presentation read from a flowing document one slide per page.
/// `file_name` is shown by `FILENAME` fields.
pub(crate) fn expand_pages(presentation: &mut ibis_doc_model::Presentation, res: &Resources, file_name: Option<&str>) {
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


/// The rectangles of an underline `w` long from `x`, under text of `size`
/// points whose baseline is at `baseline` (measured on Word's PDF: a single
/// line 0.05 em + 0.25 pt thick, its top 0.1125 em + 0.5 pt below the
/// baseline; heavy ones 0.025 em + 1.25 pt from 0.0875 em + 0.25 pt).
fn underline_items(style: UnderlineStyle, size: f32, x: f32, w: f32, baseline: f32, color: Hsla) -> Vec<Item> {
    use UnderlineStyle::*;
    let (top, thick) = if style.heavy() || style == Double || style == WavyDouble { (0.0875 * size + 0.25, 0.025 * size + 1.25) } else { (0.1367 * size, 0.0684 * size) };
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
    let bands: Vec<(f32, f32)> = match style {
        Double => {
            let t = 0.025 * size + 0.25;
            vec![(0.0, t), (t + 1.0, t)]
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
fn turn_page_items(items: Vec<Item>, direction: ibis_doc_model::CellDirection, w: f32, h: f32) -> Vec<Item> {
    use ibis_doc_model::CellDirection;
    let tb = direction == CellDirection::TbRl;
    // A point of the across page in the paper.
    let at = |x: f32, y: f32| if tb { (w - y, x) } else { (y, h - x) };
    // A rectangle's corner nearest the paper's top-left, and its size.
    let rect = |x: f32, y: f32, rw: f32, rh: f32| {
        let (ax, ay) = at(x, y);
        let (bx, by) = at(x + rw, y + rh);
        (ax.min(bx), ay.min(by), rh, rw)
    };
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        match it {
            // A squeezed character (a glyph wider than its cell, as a bold ＭＳ 明朝's, or `w:w`) is turned as the others are, upright at its
            // cell's size: left as it was, it stayed across at the page's top left (GitHub issue #54, the bold ＭＳ headings of a `tbRl` page).
            Item::Text { .. } | Item::Scaled { .. } => out.extend(turn_cell_text(&[it], direction, CellAnchor::Top, (0.0, 0.0, w, h), w)),
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
            Item::Line { para, x, y, h: lh, stops, .. } => {
                if tb {
                    out.push(Item::Line { para, x: w - y - lh, y: x, h: lh, stops, vertical: true });
                } else {
                    out.push(Item::Line { para, x: y, y: h - x - lh, h: lh, stops, vertical: false });
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
        let out = super::turn_page_items(vec![scaled], ibis_doc_model::CellDirection::TbRl, 800.0, 600.0);
        assert!(matches!(&out[..], [Item::Turned { xscale, .. }] if (*xscale - 0.5).abs() < 1e-6), "{out:?}");
    }
}
