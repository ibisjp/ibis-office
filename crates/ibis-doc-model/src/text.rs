// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Text in shapes: paragraphs, runs and their formatting.

use crate::drawing::Rgb;
use crate::geometry::Emu;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    /// 両端揃え (`just`): the lines but the last spread to the width.
    Justify,
    /// 均等割り付け (`dist`): every line spread to the width.
    Distributed,
}

/// The formatting of a whole text body, as the ribbon shows and edits it.
///
/// Runs and paragraphs may override parts of it; see [`RunStyle`] and
/// [`Paragraph::align`]. Changing a field here with
/// [`TextBody::apply_style`] drops those overrides, so the change shows on all
/// of the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextStyle {
    /// Font size in hundredths of a point, as OOXML's `sz` attribute (1800 = 18 pt).
    pub font_size: u32,
    pub bold: bool,
    pub italic: bool,
    pub color: Rgb,
    pub align: TextAlign,
    /// Font family; `None` is the UI's default font.
    pub font: Option<String>,
    /// The run's other script's font, used when `font` is not installed.
    pub fallback_font: Option<String>,
    /// Kerning from this size on, in hundredths of a point (`a:rPr/@kern`: 1200 kerns text of 12 pt and larger); `None` or 0: none.
    pub kern: KernMin,
}

/// The size from which text is kerned (`a:rPr/@kern`, in hundredths of a point; `None` or 0: not kerned), as the file says it with its
/// inheritance. It only tells the renderer: a run made in the editor has none until the file is read again, so it takes no part in
/// comparing styles (two styles are equal whatever their `KernMin`).
#[derive(Clone, Copy, Debug, Default)]
pub struct KernMin(pub Option<u32>);

impl PartialEq for KernMin {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for KernMin {}

impl TextStyle {
    pub const MIN_FONT_SIZE: u32 = 100;
    pub const MAX_FONT_SIZE: u32 = 400_000;

    pub fn font_size_pt(&self) -> f32 {
        self.font_size as f32 / 100.0
    }

    pub fn with_font_size(mut self, font_size: u32) -> Self {
        self.font_size = font_size.clamp(Self::MIN_FONT_SIZE, Self::MAX_FONT_SIZE);
        self
    }

    /// This style with the overrides of `run` applied.
    pub fn with_run(&self, run: &RunStyle) -> Self {
        Self {
            font_size: run.font_size.unwrap_or(self.font_size),
            bold: run.bold.unwrap_or(self.bold),
            italic: run.italic.unwrap_or(self.italic),
            color: run.color.unwrap_or(self.color),
            align: self.align,
            font: run.font.clone().or_else(|| self.font.clone()),
            fallback_font: run.fallback_font.clone().or_else(|| self.fallback_font.clone()),
            kern: KernMin(run.kern.0.or(self.kern.0)),
        }
    }
}

impl Default for TextStyle {
    /// PowerPoint's default for a new text box: 18 pt, black, left-aligned.
    fn default() -> Self {
        Self {
            font_size: 1800,
            bold: false,
            italic: false,
            color: Rgb::BLACK,
            align: TextAlign::Left,
            font: None,
            fallback_font: None,
            kern: KernMin(None),
        }
    }
}

/// Font sizes offered by PowerPoint's size list, in hundredths of a point.
pub const FONT_SIZE_STEPS: &[u32] = &[
    800, 900, 1000, 1050, 1100, 1200, 1400, 1600, 1800, 2000, 2400, 2800, 3200, 3600, 4000,
    4400, 4800, 5400, 6000, 6600, 7200, 8000, 8800, 9600,
];

/// The size of text PowerPoint draws in a shape whose text is shrunk by `a:normAutofit fontScale` (`font_scale`, 100000 = 100%): the size times the
/// scale rounded to a whole point, a half up. Windows PowerPoint's PDFs: 009's titles (16 pt at 90% is 14 pt, 13 pt at 90% is 12 pt), 032 (18 pt at 90% is
/// 16 pt), 005 (12 pt at 92.5% is 11 pt), and autofit-size-rounding.pptx, where all 44 boxes agree: 10 pt at 85% = 8.5 is 9, 7 pt at 50% = 3.5 is 4,
/// 20 pt at 92.5% = 18.5 is 19, 44 pt at 62.5% = 27.5 is 28, 4 pt at 85% = 3.4 is 3 (no lower limit). Computed in decimals (a scale as a float is not
/// exactly 0.925). Without a shrink the size is as it is.
pub fn autofit_font_size_pt(size_pt: f32, font_scale: u32) -> f32 {
    if font_scale >= 100_000 {
        return size_pt;
    }
    let scaled = f64::from(size_pt) * f64::from(font_scale) / 100_000.0;
    ((scaled + 0.5).floor() as f32).max(1.0)
}

/// The next larger (`grow = true`) or smaller size in [`FONT_SIZE_STEPS`],
/// like PowerPoint's "Increase/Decrease Font Size".
pub fn step_font_size(current: u32, grow: bool) -> u32 {
    let next = if grow {
        FONT_SIZE_STEPS.iter().copied().find(|&s| s > current)
    } else {
        FONT_SIZE_STEPS.iter().rev().copied().find(|&s| s < current)
    };
    next.unwrap_or(if grow { current + 1000 } else { current })
        .clamp(TextStyle::MIN_FONT_SIZE, TextStyle::MAX_FONT_SIZE)
}

/// Whether a character is drawn with the East Asian font of a font set
/// (as Office picks: Enclosed Alphanumerics U+2460..=U+24FF, Geometric
/// Shapes U+25A0..=U+25FF, and from CJK radicals, U+2E80, up).
pub fn is_east_asian(c: char) -> bool {
    matches!(c as u32, 0x2460..=0x24ff | 0x25a0..=0x25ff) || c as u32 >= 0x2e80
}

/// Typefaces by script: a theme's heading or body fonts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontSet {
    pub latin: Option<String>,
    pub east_asian: Option<String>,
    /// Complex scripts (Arabic, Hebrew, Thai, …).
    pub complex: Option<String>,
}

impl FontSet {
    /// The typeface for `text`: the East Asian one when it has East Asian
    /// characters, otherwise the Latin one (each falling back to the other).
    pub fn for_text(&self, text: &str) -> Option<&str> {
        let (first, second) = if text.chars().any(is_east_asian) { (&self.east_asian, &self.latin) } else { (&self.latin, &self.east_asian) };
        first.as_deref().or(second.as_deref())
    }

    /// The typeface of the other script than [`for_text`](Self::for_text) picks.
    pub fn other_for_text(&self, text: &str) -> Option<&str> {
        if text.chars().any(is_east_asian) { self.latin.as_deref() } else { self.east_asian.as_deref() }
    }
}

/// A document's theme fonts: text with no typeface of its own uses them
/// (Office's `+mj-lt`, `+mn-ea`, …; Word's `majorHAnsi`, …).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeFonts {
    /// Headings (titles).
    pub major: FontSet,
    /// Body text.
    pub minor: FontSet,
}

/// Text a presentation computes rather than stores (`a:fld`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextField {
    /// The number of the slide the text is on (`a:fld type="slidenum"`);
    /// see [`crate::Presentation::renumber_slides`].
    SlideNumber,
    /// The date and time, updated (`a:fld type="datetime"`, `datetime1` to
    /// `datetime13`, `datetimeyyyy`: `format` 0 to 14), in East Asian form if its language
    /// is one (`a:rPr/@lang` ja, zh, ko); see [`crate::Presentation::refresh_dates`].
    DateTime { format: u8, east_asian: bool },
}

/// Where a hyperlink (ハイパーリンク, `a:hlinkClick`) goes: a web page or a
/// file (`Url`, its relationship external), a slide of the presentation, or
/// a move in the show.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SlideLink {
    Url(String),
    Slide(crate::model::SlideId),
    NextSlide,
    PreviousSlide,
    FirstSlide,
    LastSlide,
    LastSlideViewed,
    EndShow,
}

/// Character formatting of a run that differs from its body's [`TextStyle`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunStyle {
    pub font_size: Option<u32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: bool,
    /// Background colour behind the text (`a:highlight`).
    pub highlight: Option<Rgb>,
    /// Outline around the characters (`a:ln`): colour and width in EMU.
    pub outline: Option<(Rgb, i64)>,
    /// The characters painted with a gradient or a pattern (`a:gradFill`,
    /// `a:pattFill`) instead of `color` (which keeps its first colour, for
    /// what draws in one colour).
    pub fill: Option<crate::drawing::Fill>,
    /// Drawn transparent: the text of a paragraph an animation has not
    /// brought in yet (set on the copy a slide show draws, never saved).
    pub invisible: bool,
    /// Raised (positive) or lowered (negative) as superscript or subscript,
    /// by this fraction of the font size in 100 000ths (OOXML's `baseline`).
    pub baseline: i32,
    /// Space added after each character, in hundredths of a point
    /// (negative to tighten; OOXML's `spc`).
    pub spacing: i32,
    pub color: Option<Rgb>,
    pub font: Option<String>,
    /// The other script's font, for when `font` is not installed.
    pub fallback_font: Option<String>,
    /// `a:rPr/@kern` (see `KernMin`).
    pub kern: KernMin,
    /// The run's text is computed (a slide number), not typed.
    pub field: Option<TextField>,
    /// A line through the characters (`a:rPr/@strike`).
    pub strike: Strike,
    /// Letters drawn as capitals (`a:rPr/@cap`).
    pub caps: Caps,
    /// How the underline is drawn when `underline` is set (`a:rPr/@u`).
    pub underline_kind: Underline,
    /// The run is a hyperlink (`a:hlinkClick`).
    pub link: Option<SlideLink>,
    /// Tooltip / ScreenTip of the hyperlink (`a:hlinkClick/@tooltip`).
    pub link_tooltip: Option<String>,
    /// Shadow effect on characters (`a:outerShdw`).
    pub shadow: Option<crate::drawing::Shadow>,
    /// A soft glow around the characters (`a:effectLst/a:glow`): colour and radius in EMU.
    pub glow: Option<(crate::drawing::Color, i64)>,
    /// Proofing language code (`a:rPr/@lang`, e.g. "ja-JP", "en-US").
    pub lang: Option<String>,
    /// The run is a math formula (`a14:m`), drawn typeset; its text is the
    /// formula in one line.
    pub math: Option<Box<crate::math::Math>>,
    /// The formula is in the line with the text round it (`a14:m` holding `m:oMath`), not a display formula of its own
    /// (`m:oMathPara`).
    pub math_inline: bool,
    /// Run border line (`w:bdr` inside a Word text box).
    pub border: Option<crate::flow::FlowBorder>,
}

/// The kind of an underline (OOXML's `ST_TextUnderlineType`, but `none`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Underline {
    /// `sng`.
    #[default]
    Single,
    /// `words`: only under the words (PowerPoint for Mac draws it whole).
    Words,
    Double,
    Heavy,
    Dotted,
    DottedHeavy,
    Dash,
    DashHeavy,
    DashLong,
    DashLongHeavy,
    DotDash,
    DotDashHeavy,
    DotDotDash,
    DotDotDashHeavy,
    Wavy,
    WavyHeavy,
    WavyDouble,
}

impl Underline {
    /// The kind for OOXML's `u` value (`None` for `none` and unknown values).
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "sng" => Self::Single,
            "words" => Self::Words,
            "dbl" => Self::Double,
            "heavy" => Self::Heavy,
            "dotted" => Self::Dotted,
            "dottedHeavy" => Self::DottedHeavy,
            "dash" => Self::Dash,
            "dashHeavy" => Self::DashHeavy,
            "dashLong" => Self::DashLong,
            "dashLongHeavy" => Self::DashLongHeavy,
            "dotDash" => Self::DotDash,
            "dotDashHeavy" => Self::DotDashHeavy,
            "dotDotDash" => Self::DotDotDash,
            "dotDotDashHeavy" => Self::DotDotDashHeavy,
            "wavy" => Self::Wavy,
            "wavyHeavy" => Self::WavyHeavy,
            "wavyDbl" => Self::WavyDouble,
            _ => return None,
        })
    }

    /// The kind's OOXML `u` value (what `parse` reads).
    pub fn value(self) -> &'static str {
        match self {
            Self::Single => "sng",
            Self::Words => "words",
            Self::Double => "dbl",
            Self::Heavy => "heavy",
            Self::Dotted => "dotted",
            Self::DottedHeavy => "dottedHeavy",
            Self::Dash => "dash",
            Self::DashHeavy => "dashHeavy",
            Self::DashLong => "dashLong",
            Self::DashLongHeavy => "dashLongHeavy",
            Self::DotDash => "dotDash",
            Self::DotDashHeavy => "dotDashHeavy",
            Self::DotDotDash => "dotDotDash",
            Self::DotDotDashHeavy => "dotDotDashHeavy",
            Self::Wavy => "wavy",
            Self::WavyHeavy => "wavyHeavy",
            Self::WavyDouble => "wavyDbl",
        }
    }
}

/// A line through the characters (OOXML's `ST_TextStrikeType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Strike {
    #[default]
    None,
    /// `sngStrike`.
    Single,
    /// `dblStrike`.
    Double,
}

/// Letters drawn as capitals (OOXML's `ST_TextCapsType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Caps {
    #[default]
    None,
    /// `small`: lower-case letters as smaller capitals.
    Small,
    /// `all`: every letter a capital.
    All,
}

/// A stretch of text with one character format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub style: RunStyle,
}

/// The mark in front of a bulleted paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bullet {
    pub text: String,
    /// A picture drawn as the bullet instead of `text` (`a:buBlip`).
    pub picture: Option<crate::MediaId>,
    /// Size relative to the paragraph's first run, in 100 000ths.
    pub size: u32,
    /// `None` uses the colour of the paragraph's first run.
    pub color: Option<Rgb>,
    /// A numbered bullet (`a:buAutoNum`): `text` is the label it shows now.
    pub auto: Option<AutoNumber>,
    /// A bullet drawn with a symbol typeface (`a:buFont` Wingdings, Symbol): `text` is the Unicode character that stands in
    /// for its glyph where the typeface is not installed; this is what the file says.
    pub symbol: Option<SymbolBullet>,
}

/// The glyph of a symbol typeface (Wingdings, Symbol) a bullet is drawn with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolBullet {
    /// The typeface (`a:buFont/@typeface`).
    pub font: String,
    /// The character as the file writes it (`a:buChar/@char`): a code of the typeface, or in the private use range U+F020..=U+F0FF.
    pub code: String,
    /// The percentage of the bullet's size the stand-in character is drawn smaller (`Bullet::size` is already scaled by it): the
    /// typeface's own glyph is drawn at the full size.
    pub stand_in_percent: u32,
}

/// How a numbered bullet counts (`a:buAutoNum`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoNumber {
    /// `ST_TextAutonumberScheme`, such as `arabicPeriod`.
    pub scheme: String,
    pub start_at: u32,
}

/// How runs of different sizes in a line line up (`a:pPr/@fontAlgn`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontAlign {
    /// Preserve the automatic OOXML alignment independently of an explicit baseline.
    Auto,
    /// On a shared baseline.
    #[default]
    Baseline,
    Top,
    /// Their middles on the line's middle.
    Center,
    Bottom,
}

/// Alignment of a tab stop in a paragraph (`a:tab/@algn`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextTabAlign {
    #[default]
    Left,
    Center,
    Right,
    Decimal,
}

/// A tab stop on a slide's paragraph (`a:tab`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextTabStop {
    pub position: Emu,
    pub align: TextTabAlign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineSpacing {
    /// Multiple of single spacing, in 100 000ths (100 000 = single).
    Percent(u32),
    /// Exact line pitch, in hundredths of a point.
    Points(u32),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Paragraph {
    pub runs: Vec<Run>,
    /// `None` uses the body's alignment.
    pub align: Option<TextAlign>,
    /// Left margin of the text, in EMU.
    pub margin_left: Emu,
    /// Offset of the first line (and the bullet) from `margin_left`, in EMU;
    /// negative for a hanging indent.
    pub indent: Emu,
    pub bullet: Option<Bullet>,
    pub line_spacing: Option<LineSpacing>,
    pub font_align: FontAlign,
    /// Space above and below the paragraph, in hundredths of a point.
    pub space_before: u32,
    pub space_after: u32,
    /// Whether `space_before` is drawn as it is. Not so (the default) for a space before given in points: PowerPoint draws it rounded to a whole
    /// point (Windows PowerPoint's PDF of paragraph-spacing-before-values: 3.75 is 4, 4.8 is 5, 5.5 is 6, 6.4 is 6, 7.2 is 7; the reason is not
    /// known). A space before given as a percentage of the line (`a:spcPct`) is not rounded, nor is Word's.
    pub space_before_exact: bool,
    /// Formatting of the paragraph mark; sets the height of an empty paragraph.
    pub end_style: RunStyle,
    /// The paragraph is a line after a line break (Shift+Enter, `a:br`) of
    /// the one before it: it shares that one's alignment, margin and line
    /// spacing, and has no bullet, first-line indent or space above.
    pub line_break: bool,
    /// The outline level, 0 to 8 (`a:pPr/@lvl`): numbering counts within a
    /// level.
    pub level: u8,
    /// Tab stops defined on this paragraph (`a:pPr/a:tabLst`).
    pub tabs: Vec<TextTabStop>,
    /// Default tab stop interval (`a:pPr/@defTabSz`), in EMU (`None`: 914 400 EMU = 1 inch).
    pub default_tab: Option<Emu>,
    /// Whether `w:snapToGrid` is explicitly disabled (`false`) on this paragraph in a Word text box.
    pub no_snap_to_grid: bool,
}

impl Paragraph {
    fn char_count(&self) -> usize {
        self.runs.iter().map(|r| r.text.chars().count()).sum()
    }

    /// Remove the characters from `start` to `end`, keeping the runs.
    fn remove_chars(&mut self, start: usize, end: usize) {
        let mut at = 0;
        for run in &mut self.runs {
            let len = run.text.chars().count();
            let (from, to) = (start.clamp(at, at + len) - at, end.clamp(at, at + len) - at);
            if from < to {
                run.text = run.text.chars().take(from).chain(run.text.chars().skip(to)).collect();
            }
            at += len;
        }
    }

    /// Insert `text` (one line) at character `offset`: into the run it
    /// follows, or the first run at the start, or a new run with the
    /// paragraph mark's format in an empty paragraph.
    fn insert_chars(&mut self, offset: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.runs.is_empty() {
            self.runs.push(Run { text: text.to_string(), style: self.end_style.clone() });
            return;
        }
        let mut at = 0;
        let count = self.runs.len();
        for (i, run) in self.runs.iter_mut().enumerate() {
            let len = run.text.chars().count();
            if offset <= at + len && (offset > at || at == 0 || i + 1 == count || len == 0) {
                let local = offset - at;
                let byte = run.text.char_indices().nth(local).map_or(run.text.len(), |(b, _)| b);
                run.text.insert_str(byte, text);
                return;
            }
            at += len;
        }
    }

    /// Cut the paragraph at character `offset`; the part after it is
    /// returned as a paragraph with the same format (bullet included, as a
    /// new paragraph gets in PowerPoint and Word).
    fn split_off(&mut self, offset: usize) -> Paragraph {
        let mut tail_runs = Vec::new();
        let mut at = 0;
        let mut keep = Vec::new();
        for run in std::mem::take(&mut self.runs) {
            let len = run.text.chars().count();
            if at + len <= offset {
                keep.push(run);
            } else if at >= offset {
                tail_runs.push(run);
            } else {
                let local = offset - at;
                let byte = run.text.char_indices().nth(local).map_or(run.text.len(), |(b, _)| b);
                let (head, tail) = run.text.split_at(byte);
                tail_runs.push(Run { text: tail.to_string(), style: run.style.clone() });
                keep.push(Run { text: head.to_string(), style: run.style });
            }
            at += len;
        }
        self.runs = keep;
        Paragraph { runs: tail_runs, line_break: false, ..self.clone() }
    }

    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// A change of character format for part of a text (each `Some` field is
/// set, the others are left as they are).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CharacterFormat {
    pub font_size: Option<u32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    /// How the underline is drawn (`a:rPr/@u`); set with `underline: Some(true)`.
    pub underline_kind: Option<Underline>,
    pub color: Option<Rgb>,
    /// `Some(None)` returns to the theme's font.
    pub font: Option<Option<String>>,
    /// Latin and East Asian families, chosen for each target run by its script.
    pub script_fonts: Option<(Option<String>, Option<String>)>,
    pub highlight: Option<Option<Rgb>>,
    /// Some(None) clears the outline; the inner value is colour and width in EMU.
    pub outline: Option<Option<(Rgb, i64)>>,
    pub baseline: Option<i32>,
    pub spacing: Option<i32>,
    pub strike: Option<Strike>,
    pub caps: Option<Caps>,
    /// `Some(None)` takes the hyperlink away.
    pub link: Option<Option<SlideLink>>,
    /// `Some(None)` takes the tooltip away; inner value is the tooltip text.
    pub link_tooltip: Option<Option<String>>,
    /// Some(None) clears the shadow effect; inner value is the shadow.
    pub shadow: Option<Option<crate::drawing::Shadow>>,
    /// `Some(None)` clears the proofing language.
    pub lang: Option<Option<String>>,
}

impl CharacterFormat {
    fn apply(&self, run: &mut RunStyle, text: &str) {
        if let Some(v) = self.font_size {
            run.font_size = Some(v.clamp(TextStyle::MIN_FONT_SIZE, TextStyle::MAX_FONT_SIZE));
        }
        if let Some(v) = self.bold {
            run.bold = Some(v);
        }
        if let Some(v) = self.italic {
            run.italic = Some(v);
        }
        if let Some(v) = self.underline {
            run.underline = v;
        }
        if let Some(v) = self.underline_kind {
            run.underline_kind = v;
        }
        if let Some(v) = self.color {
            run.color = Some(v);
        }
        if let Some(v) = &self.font {
            run.font = v.clone();
            run.fallback_font = None;
        }
        if let Some((latin, east_asian)) = &self.script_fonts {
            let (font, fallback) = if text.chars().any(is_east_asian) { (east_asian, latin) } else { (latin, east_asian) };
            run.font = font.clone(); run.fallback_font = fallback.clone();
        }
        if let Some(v) = self.highlight {
            run.highlight = v;
        }
        if let Some(v) = self.outline {
            run.outline = v;
        }
        if let Some(v) = self.shadow {
            run.shadow = v;
        }
        if let Some(v) = self.baseline {
            run.baseline = v;
        }
        if let Some(v) = self.spacing {
            run.spacing = v;
        }
        if let Some(v) = self.strike {
            run.strike = v;
        }
        if let Some(v) = self.caps {
            run.caps = v;
        }
        if let Some(v) = &self.link {
            run.link = v.clone();
            if v.is_none() {
                run.link_tooltip = None;
            }
        }
        if let Some(v) = &self.link_tooltip {
            run.link_tooltip = v.clone();
        }
        if let Some(v) = &self.lang {
            run.lang = v.clone();
        }
    }
}

/// The kind of mark a paragraph gets from 箇条書き / 番号付きリスト.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    None,
    /// A bullet character (`•`).
    Bullet,
    /// Numbers `1.`, `2.`, … (`arabicPeriod`).
    Numbered,
    /// Another bullet character.
    Symbol(char),
    /// Numbers (or letters) of another scheme.
    Scheme(NumberScheme),
}

/// The numbering schemes 番号付きリスト offers (`ST_TextAutonumberScheme`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberScheme {
    ArabicPeriod,
    ArabicParenR,
    AlphaLcPeriod,
    AlphaUcPeriod,
    AlphaLcParenR,
    RomanLcPeriod,
    RomanUcPeriod,
}

impl NumberScheme {
    pub const ALL: [NumberScheme; 7] = [
        NumberScheme::ArabicPeriod,
        NumberScheme::ArabicParenR,
        NumberScheme::AlphaLcPeriod,
        NumberScheme::AlphaUcPeriod,
        NumberScheme::AlphaLcParenR,
        NumberScheme::RomanLcPeriod,
        NumberScheme::RomanUcPeriod,
    ];

    /// The scheme's name in the file.
    pub fn name(self) -> &'static str {
        match self {
            Self::ArabicPeriod => "arabicPeriod",
            Self::ArabicParenR => "arabicParenR",
            Self::AlphaLcPeriod => "alphaLcPeriod",
            Self::AlphaUcPeriod => "alphaUcPeriod",
            Self::AlphaLcParenR => "alphaLcParenR",
            Self::RomanLcPeriod => "romanLcPeriod",
            Self::RomanUcPeriod => "romanUcPeriod",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    /// The label of item `n` (1, 2, …).
    pub fn label(self, n: u32) -> String {
        let alpha = |upper: bool| {
            let mut s = String::new();
            let mut k = n;
            while k > 0 {
                s.insert(0, (((k - 1) % 26) as u8 + if upper { b'A' } else { b'a' }) as char);
                k = (k - 1) / 26;
            }
            s
        };
        let roman = |upper: bool| {
            let table = [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")];
            let (mut s, mut k) = (String::new(), n);
            for (value, digits) in table {
                while k >= value {
                    s.push_str(digits);
                    k -= value;
                }
            }
            if upper { s.to_uppercase() } else { s }
        };
        match self {
            Self::ArabicPeriod => format!("{n}."),
            Self::ArabicParenR => format!("{n})"),
            Self::AlphaLcPeriod => format!("{}.", alpha(false)),
            Self::AlphaUcPeriod => format!("{}.", alpha(true)),
            Self::AlphaLcParenR => format!("{})", alpha(false)),
            Self::RomanLcPeriod => format!("{}.", roman(false)),
            Self::RomanUcPeriod => format!("{}.", roman(true)),
        }
    }
}

impl ListKind {
    /// The list a bullet makes a paragraph (Numbered for an arabicPeriod one).
    pub fn of(bullet: Option<&Bullet>) -> ListKind {
        match bullet {
            None => ListKind::None,
            Some(b) => match &b.auto {
                Some(a) => match NumberScheme::from_name(&a.scheme) {
                    Some(NumberScheme::ArabicPeriod) | None => ListKind::Numbered,
                    Some(s) => ListKind::Scheme(s),
                },
                None if b.text.ends_with('.') && b.text.len() > 1 && b.text[..b.text.len() - 1].chars().all(|c| c.is_ascii_digit()) => ListKind::Numbered,
                None => match b.text.chars().next() {
                    Some('•') | None => ListKind::Bullet,
                    Some(c) => ListKind::Symbol(c),
                },
            },
        }
    }
}

/// A change of paragraph format for the paragraphs of part of a text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParagraphFormat {
    pub font_align: Option<FontAlign>,
    pub align: Option<TextAlign>,
    pub line_spacing: Option<Option<LineSpacing>>,
    pub space_before: Option<u32>,
    pub space_after: Option<u32>,
    pub list: Option<ListKind>,
    /// The outline level, 0 to 8 (アウトライン表示の昇格・降格).
    pub level: Option<u8>,
    /// Added to the left margin (インデントを増やす・減らす), in EMU; the
    /// margin does not go below 0.
    pub indent_by: Option<Emu>,
    /// Explicit left margin, in EMU.
    pub margin_left: Option<Emu>,
    /// First-line indent offset from `margin_left`, in EMU (negative for hanging).
    pub indent: Option<Emu>,
    /// Tab stops defined on this paragraph.
    pub tabs: Option<Vec<TextTabStop>>,
    /// Numbered list starting number (1..=32767).
    pub start_at: Option<u32>,
}

/// A kind of vertical writing (`a:bodyPr/@vert`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VerticalKind {
    /// `eaVert`: lines from right to left, East Asian characters upright and
    /// the others turned a quarter turn.
    #[default]
    EastAsian,
    /// `mongolianVert`: as `EastAsian`, but lines from left to right.
    Mongolian,
    /// `wordArtVert`: every character upright, stacked; lines from left to
    /// right.
    WordArt,
    /// `wordArtVertRtl`: as `WordArt`, lines from right to left.
    WordArtRtl,
}

impl VerticalKind {
    /// The kind for OOXML's `vert` value (`None` for the horizontal and
    /// turned ones).
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "eaVert" => Self::EastAsian,
            "mongolianVert" => Self::Mongolian,
            "wordArtVert" => Self::WordArt,
            "wordArtVertRtl" => Self::WordArtRtl,
            _ => return None,
        })
    }

    pub fn ooxml(self) -> &'static str {
        match self {
            Self::EastAsian => "eaVert",
            Self::Mongolian => "mongolianVert",
            Self::WordArt => "wordArtVert",
            Self::WordArtRtl => "wordArtVertRtl",
        }
    }
}

/// Where the text sits vertically in its shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerticalAnchor {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// Space between a shape's frame and its text, in EMU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Insets {
    pub left: Emu,
    pub top: Emu,
    pub right: Emu,
    pub bottom: Emu,
}

impl Default for Insets {
    /// OOXML's default: 0.1 in left and right, 0.05 in top and bottom.
    fn default() -> Self {
        Self { left: 91_440, top: 45_720, right: 91_440, bottom: 45_720 }
    }
}

/// How text fits within its shape or frame (`a:bodyPr`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAutofit {
    /// Do not autofit text (`a:noAutofit`).
    #[default]
    None,
    /// Shrink text on overflow (`a:normAutofit`).
    Shrink,
    /// Resize shape to fit text (`a:spAutoFit`).
    ResizeShape,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextBody {
    pub paragraphs: Vec<Paragraph>,
    pub style: TextStyle,
    pub insets: Insets,
    pub anchor: VerticalAnchor,
    /// Wrap lines at the frame's width.
    pub wrap: bool,
    /// How text fits within its shape or frame (`a:bodyPr`).
    pub autofit: TextAutofit,
    /// "Shrink text on overflow": font sizes are drawn at this fraction, in
    /// 100 000ths (100 000 = unscaled).
    pub font_scale: u32,
    /// Vertical writing (`vert="eaVert"` and the others of
    /// [`VerticalKind`]): characters run top to bottom.
    pub vertical: bool,
    /// Which vertical writing, when `vertical`.
    pub vertical_kind: VerticalKind,
    /// The text turned within its shape (`bodyPr/@rot`), clockwise, in
    /// 60 000ths of a degree; laid out in the unturned text rectangle.
    pub rotation: i32,
    /// The whole text turned a quarter turn (`vert="vert"`: 5 400 000,
    /// `vert="vert270"`: 16 200 000; 0 otherwise), laid out in the text
    /// rectangle turned with it (its width and height swapped).
    pub sideways: i32,
    /// The text kept upright when the shape is turned or flipped
    /// (`bodyPr/@upright`).
    pub upright: bool,
    /// Number of columns (`a:bodyPr/@numCol`), 1 to 16.
    pub columns: u32,
    /// Space between columns (`a:bodyPr/@spcCol`), in EMU.
    pub column_spacing: Emu,
    /// Preset text warp shape (`a:bodyPr/a:prstTxWarp/@prst`), e.g. "textArchUp", "textWave1".
    pub warp: Option<String>,
    /// The warp's own adjust values (`a:prstTxWarp/a:avLst/a:gd`: name and the number of `val N`).
    pub warp_adjust: Vec<(String, i64)>,
    /// The text is cut at the shape's edge (a Word text box: what does not fit is not seen).
    pub clip: bool,
}

impl Default for TextBody {
    fn default() -> Self {
        Self {
            paragraphs: vec![Paragraph::default()],
            style: TextStyle::default(),
            insets: Insets::default(),
            anchor: VerticalAnchor::Top,
            wrap: true,
            autofit: TextAutofit::None,
            font_scale: 100_000,
            vertical: false,
            vertical_kind: VerticalKind::EastAsian,
            rotation: 0,
            sideways: 0,
            upright: false,
            columns: 1,
            column_spacing: 0,
            warp: None,
            warp_adjust: Vec::new(),
            clip: false,
        }
    }
}

impl TextBody {
    /// A body of unformatted paragraphs, one per `\n`-separated line.
    pub fn plain(text: &str, style: TextStyle) -> Self {
        let mut body = Self { style, ..Self::default() };
        body.set_text(text);
        body
    }

    /// The text, paragraphs separated by `\n`.
    pub fn text(&self) -> String {
        self.paragraphs.iter().map(Paragraph::text).collect::<Vec<_>>().join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.paragraphs.iter().all(|p| p.runs.iter().all(|r| r.text.is_empty()))
    }

    /// Replace the text, one paragraph per `\n`-separated line.
    ///
    /// Plain text carries no formatting, so each new paragraph takes the
    /// paragraph and character format of the old first paragraph.
    pub fn set_text(&mut self, text: &str) {
        let template = self.paragraphs.first().cloned().unwrap_or_default();
        let run_style = template.runs.first().map(|r| r.style.clone()).unwrap_or_else(|| template.end_style.clone());
        self.paragraphs = text
            .split('\n')
            .map(|line| Paragraph {
                runs: if line.is_empty() {
                    Vec::new()
                } else {
                    vec![Run { text: line.to_string(), style: run_style.clone() }]
                },
                end_style: run_style.clone(),
                ..template.clone()
            })
            .collect();
    }

    /// Replace `remove` characters at character `at` of [`text`](Self::text)
    /// with `insert`, keeping the runs and their formatting: text typed
    /// into a run takes its format, and a `\n` splits a paragraph (both
    /// halves keeping its format). Runs are kept even when they become
    /// empty, so that an edit inside one run changes nothing else (and a
    /// writer can put it back where it came from). Positions past the end
    /// are clamped.
    pub fn splice(&mut self, at: usize, remove: usize, insert: &str) {
        if self.paragraphs.is_empty() {
            self.paragraphs.push(Paragraph::default());
        }
        let total = self.text().chars().count();
        let at = at.min(total);
        let end = (at + remove).min(total);
        let (p1, o1) = self.locate(at);
        let (p2, o2) = self.locate(end);
        // Remove: within one paragraph, or joining the first and the last.
        if p1 == p2 {
            self.paragraphs[p1].remove_chars(o1, o2);
        } else {
            let tail = self.paragraphs[p2].split_off(o2);
            self.paragraphs[p1].split_off(o1);
            if self.paragraphs[p1].runs.is_empty() {
                self.paragraphs[p1].end_style = tail.end_style;
            }
            self.paragraphs[p1].runs.extend(tail.runs);
            self.paragraphs.drain(p1 + 1..=p2);
        }
        // Insert, splitting the paragraph at each line break.
        let mut paragraph = p1;
        let mut offset = o1;
        for (i, line) in insert.split('\n').enumerate() {
            if i > 0 {
                let tail = self.paragraphs[paragraph].split_off(offset);
                paragraph += 1;
                self.paragraphs.insert(paragraph, tail);
                offset = 0;
            }
            self.paragraphs[paragraph].insert_chars(offset, line);
            offset += line.chars().count();
        }
        self.unify_lines();
        self.refresh_math();
    }

    /// A formula run whose text was edited is read again from its text (typed in linear form).
    fn refresh_math(&mut self) {
        for run in self.paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()) {
            // (An emptied run keeps its formula, so that what is typed into it is a formula again.)
            if let Some(math) = &run.style.math
                && !run.text.is_empty()
                && run.text != math.linear()
            {
                run.style.math = Some(Box::new(crate::math::parse_linear(&run.text)));
            }
        }
    }

    /// Replace a match, giving inserted characters the match's first run format.
    /// Ordinary typing uses `splice` and keeps its caret-boundary semantics.
    pub fn replace_text(&mut self, at: usize, remove: usize, insert: &str) {
        if self.paragraphs.is_empty() { self.paragraphs.push(Paragraph::default()); }
        let at = at.min(self.text().chars().count());
        let (paragraph, offset) = self.locate(at);
        let mut pos = 0;
        let style = self.paragraphs[paragraph].runs.iter().find_map(|run| {
            let end = pos + run.text.chars().count();
            let found = (offset < end).then(|| run.style.clone());
            pos = end;
            found
        }).unwrap_or_else(|| self.paragraphs[paragraph].end_style.clone());
        self.splice(at, remove, insert);
        let end = at + insert.chars().count();
        if at == end { return; }
        self.split_runs_at(end);
        self.split_runs_at(at);
        let mut pos = 0;
        for paragraph in &mut self.paragraphs {
            for run in &mut paragraph.runs {
                let n = run.text.chars().count();
                if n > 0 && pos >= at && pos + n <= end { run.style = style.clone(); }
                pos += n;
            }
            pos += 1;
        }
    }

    /// The paragraph and the character in it at character `at` of the text.
    fn locate(&self, at: usize) -> (usize, usize) {
        let mut rest = at;
        for (i, p) in self.paragraphs.iter().enumerate() {
            let len = p.char_count();
            if rest <= len {
                return (i, rest);
            }
            rest -= len + 1;
        }
        let last = self.paragraphs.len() - 1;
        (last, self.paragraphs[last].char_count())
    }

    /// Keep lines after line breaks in step with the paragraph they belong
    /// to (see [`Paragraph::line_break`]): the same alignment, margin and
    /// line spacing, no bullet, indent or space above, and the space below
    /// on the last line only.
    pub fn unify_lines(&mut self) {
        if let Some(first) = self.paragraphs.first_mut() {
            first.line_break = false;
        }
        for i in 1..self.paragraphs.len() {
            if !self.paragraphs[i].line_break {
                continue;
            }
            let (head, tail) = self.paragraphs.split_at_mut(i);
            let before = head.last_mut().expect("a paragraph before");
            let line = &mut tail[0];
            line.align = before.align;
            line.margin_left = before.margin_left;
            line.line_spacing = before.line_spacing;
            line.font_align = before.font_align;
            line.level = before.level;
            line.bullet = None;
            line.indent = 0;
            line.space_before = 0;
            // The space below goes to the group's last line.
            line.space_after = line.space_after.max(before.space_after);
            before.space_after = 0;
        }
    }

    /// Cut runs at character `at` of [`text`](Self::text) (so a format
    /// can be given from there).
    pub fn split_runs_at(&mut self, at: usize) {
        let (p, offset) = self.locate(at);
        let paragraph = &mut self.paragraphs[p];
        let mut pos = 0;
        for i in 0..paragraph.runs.len() {
            let len = paragraph.runs[i].text.chars().count();
            if offset > pos && offset < pos + len {
                let byte = paragraph.runs[i].text.char_indices().nth(offset - pos).map_or(paragraph.runs[i].text.len(), |(b, _)| b);
                let tail = paragraph.runs[i].text.split_off(byte);
                let style = paragraph.runs[i].style.clone();
                paragraph.runs.insert(i + 1, Run { text: tail, style });
                return;
            }
            pos += len;
        }
    }

    /// Give characters `start..end` of the text the format `format`,
    /// splitting runs at the ends. With an empty range, the format goes to
    /// the paragraph mark of the paragraph there (for the text typed next).
    pub fn format_runs(&mut self, start: usize, end: usize, format: &CharacterFormat) {
        if self.paragraphs.is_empty() {
            self.paragraphs.push(Paragraph::default());
        }
        let total = self.text().chars().count();
        let (start, end) = (start.min(total), end.min(total).max(start.min(total)));
        if start == end {
            let (p, _) = self.locate(start);
            format.apply(&mut self.paragraphs[p].end_style, "");
            return;
        }
        self.split_runs_at(end);
        self.split_runs_at(start);
        if format.script_fonts.is_some() {
            let mut position = 0;
            for paragraph in &mut self.paragraphs {
                paragraph.runs = std::mem::take(&mut paragraph.runs).into_iter().flat_map(|run| {
                    let begin = position;
                    position += run.text.chars().count();
                    if run.text.is_empty() || begin < start || position > end { return vec![run]; }
                    let mut parts: Vec<Run> = Vec::new();
                    for c in run.text.chars() {
                        let same_script = parts.last().is_some_and(|p| p.text.chars().last().is_some_and(|last| is_east_asian(last) == is_east_asian(c)));
                        if same_script { parts.last_mut().unwrap().text.push(c); }
                        else { parts.push(Run { text: c.to_string(), style: run.style.clone() }); }
                    }
                    parts
                }).collect();
                position += 1;
            }
        }
        let mut at = 0;
        for paragraph in &mut self.paragraphs {
            let len = paragraph.char_count();
            let mut pos = at;
            for run in &mut paragraph.runs {
                let n = run.text.chars().count();
                if pos >= start && pos + n <= end && n > 0 {
                    format.apply(&mut run.style, &run.text);
                }
                pos += n;
            }
            // A paragraph wholly inside keeps the format for its mark too.
            if at >= start && at + len <= end && len > 0 {
                format.apply(&mut paragraph.end_style, "");
            }
            at += len + 1;
        }
    }

    /// 書式をクリア: characters `start..end` lose their own format (size,
    /// bold, colour, font, highlight, …) and take the text's.
    pub fn clear_run_formats(&mut self, start: usize, end: usize) {
        let total = self.text().chars().count();
        let (start, end) = (start.min(total), end.min(total).max(start.min(total)));
        let plain = |style: &RunStyle| RunStyle { field: style.field.clone(), ..RunStyle::default() };
        if start == end {
            if self.paragraphs.is_empty() {
                return;
            }
            let (p, _) = self.locate(start);
            self.paragraphs[p].end_style = plain(&self.paragraphs[p].end_style);
            return;
        }
        self.split_runs_at(end);
        self.split_runs_at(start);
        let mut at = 0;
        for paragraph in &mut self.paragraphs {
            let len = paragraph.char_count();
            let mut pos = at;
            for run in &mut paragraph.runs {
                let n = run.text.chars().count();
                if pos >= start && pos + n <= end && n > 0 {
                    run.style = plain(&run.style);
                }
                pos += n;
            }
            if at >= start && at + len <= end && len > 0 {
                paragraph.end_style = plain(&paragraph.end_style);
            }
            at += len + 1;
        }
    }

    /// The paragraphs characters `start..end` touch, by index.
    pub fn paragraphs_in(&self, start: usize, end: usize) -> std::ops::RangeInclusive<usize> {
        let (a, _) = self.locate(start);
        let last = if end > start { end - 1 } else { start };
        let (b, _) = self.locate(last);
        a..=b
    }

    /// Give the paragraphs characters `start..end` touch the format
    /// `format`: whole paragraphs, lines after line breaks included (see
    /// [`Paragraph::line_break`]).
    pub fn format_paragraphs(&mut self, start: usize, end: usize, format: &ParagraphFormat) {
        if self.paragraphs.is_empty() {
            self.paragraphs.push(Paragraph::default());
        }
        let range = self.paragraphs_in(start, end);
        let (mut first, mut last) = (*range.start(), *range.end());
        while first > 0 && self.paragraphs[first].line_break {
            first -= 1;
        }
        while last + 1 < self.paragraphs.len() && self.paragraphs[last + 1].line_break {
            last += 1;
        }
        let explicit_start = format.start_at.map(|s| s.clamp(1, 32767));
        let mut number = explicit_start.unwrap_or(1).saturating_sub(1);
        let mut level: Option<u8> = None;
        for paragraph in &mut self.paragraphs[first..=last] {
            if let Some(value) = format.font_align { paragraph.font_align = value; }
            if let Some(a) = format.align {
                paragraph.align = Some(a);
            }
            if let Some(v) = format.line_spacing {
                paragraph.line_spacing = v;
            }
            if let Some(l) = format.level {
                paragraph.level = l.min(8);
            }
            if let Some(d) = format.indent_by {
                paragraph.margin_left = (paragraph.margin_left + d).max(0);
            }
            if let Some(m) = format.margin_left {
                paragraph.margin_left = m.max(0);
            }
            if let Some(i) = format.indent {
                paragraph.indent = i;
            }
            if let Some(ref t) = format.tabs {
                paragraph.tabs = t.clone();
            }
            if paragraph.line_break {
                continue;
            }
            if let Some(v) = format.space_before {
                paragraph.space_before = v;
                // A space before set here is in points (`a:spcPts`), which PowerPoint rounds; only a percentage read from a file is exact.
                paragraph.space_before_exact = false;
            }
            if let Some(v) = format.space_after {
                paragraph.space_after = v;
            }
            match format.list {
                Some(ListKind::None) => {
                    paragraph.bullet = None;
                    paragraph.margin_left = 0;
                    paragraph.indent = 0;
                }
                Some(kind) => {
                    // An empty paragraph shows the number the next one gets
                    // and does not count (as PowerPoint numbers).
                    // A paragraph at another level starts again from start_for_level (as the
                    // reader numbers).
                    let has_text = paragraph.runs.iter().any(|r| !r.text.is_empty());
                    let start_for_level = explicit_start.unwrap_or(1);
                    if level != Some(paragraph.level) {
                        number = start_for_level.saturating_sub(1);
                    }
                    let shown = number + 1;
                    if has_text {
                        number += 1;
                        level = Some(paragraph.level);
                    }
                    let scheme = match kind {
                        ListKind::Numbered => Some(NumberScheme::ArabicPeriod),
                        ListKind::Scheme(s) => Some(s),
                        _ => None,
                    };
                    let text = match (scheme, kind) {
                        (Some(s), _) => s.label(shown),
                        (None, ListKind::Symbol(c)) => c.to_string(),
                        _ => "•".to_string(),
                    };
                    let auto = scheme.map(|s| AutoNumber {
                        scheme: s.name().to_string(),
                        start_at: if shown == start_for_level { start_for_level } else { 1 },
                    });
                    paragraph.bullet = Some(Bullet { text, picture: None, size: 100_000, color: None, auto, symbol: None });
                    // A hanging indent the width of the mark, as PowerPoint sets.
                    if paragraph.indent >= 0 {
                        let hang = if scheme.is_some() { 342_900 } else { 228_600 };
                        paragraph.margin_left = paragraph.margin_left.max(hang);
                        paragraph.indent = -hang;
                    }
                }
                None => {
                    if let Some(start_for_level) = explicit_start {
                        let kind = ListKind::of(paragraph.bullet.as_ref());
                        if matches!(kind, ListKind::Numbered | ListKind::Scheme(_)) {
                            let has_text = paragraph.runs.iter().any(|r| !r.text.is_empty());
                            if level != Some(paragraph.level) {
                                number = start_for_level.saturating_sub(1);
                            }
                            let shown = number + 1;
                            if has_text {
                                number += 1;
                                level = Some(paragraph.level);
                            }
                            let scheme = match kind {
                                ListKind::Numbered => Some(NumberScheme::ArabicPeriod),
                                ListKind::Scheme(s) => Some(s),
                                _ => None,
                            };
                            if let Some(s) = scheme {
                                let text = s.label(shown);
                                let auto = Some(AutoNumber {
                                    scheme: s.name().to_string(),
                                    start_at: if shown == start_for_level { start_for_level } else { 1 },
                                });
                                let (picture, size, color) = paragraph.bullet.as_ref()
                                    .map(|b| (b.picture.clone(), b.size, b.color))
                                    .unwrap_or((None, 100_000, None));
                                paragraph.bullet = Some(Bullet { text, picture, size, color, auto, symbol: None });
                            }
                        }
                    }
                }
            }
        }
        // The margin a bullet set goes to the lines after it too.
        self.unify_lines();
    }

    /// Refresh date fields to `t` (those with a known form), without
    /// changing their character formatting; whether any changed. A field is
    /// the runs in a row with it (split where the script changes:
    /// `segments` splits text so, each piece East Asian or not), its new
    /// text split the same way, each piece with the format of a run of its
    /// script.
    pub fn set_dates(&mut self, t: &crate::date_field::LocalTime, segments: &dyn Fn(&str) -> Vec<(String, bool)>) -> bool {
        let mut changed = false;
        for paragraph in &mut self.paragraphs {
            let mut i = 0;
            while i < paragraph.runs.len() {
                let Some(field @ TextField::DateTime { format, east_asian }) = paragraph.runs[i].style.field else {
                    i += 1;
                    continue;
                };
                let end = (i..paragraph.runs.len()).find(|&k| paragraph.runs[k].style.field != Some(field)).unwrap_or(paragraph.runs.len());
                let old: String = paragraph.runs[i..end].iter().map(|r| r.text.as_str()).collect();
                match crate::date_field::slide_date_text(format, east_asian, t) {
                    Some(text) if text != old => {
                        let style_of = |ea: bool| {
                            paragraph.runs[i..end]
                                .iter()
                                .find(|r| segments(&r.text).first().is_some_and(|s| s.1 == ea))
                                .unwrap_or(&paragraph.runs[i])
                                .style
                                .clone()
                        };
                        let runs: Vec<Run> = segments(&text).into_iter().map(|(text, ea)| Run { text, style: style_of(ea) }).collect();
                        let n = runs.len();
                        paragraph.runs.splice(i..end, runs);
                        changed = true;
                        i += n.max(1);
                    }
                    _ => i = end,
                }
            }
        }
        changed
    }

    /// Refresh slide-number fields without changing their character formatting.
    pub fn set_slide_number(&mut self, number: usize) {
        let number = number.to_string();
        for run in self.paragraphs.iter_mut().flat_map(|p| &mut p.runs) {
            if run.style.field == Some(TextField::SlideNumber) { run.text = number.clone(); }
        }
    }

    /// Set the body's style. Run and paragraph overrides of every field that
    /// changes are dropped, so the new value applies to all of the text.
    pub fn apply_style(&mut self, style: TextStyle) {
        let old = std::mem::replace(&mut self.style, style);
        let new = &self.style;
        for paragraph in &mut self.paragraphs {
            if new.align != old.align {
                paragraph.align = None;
            }
            let runs = paragraph.runs.iter_mut().map(|r| &mut r.style);
            for run in runs.chain(std::iter::once(&mut paragraph.end_style)) {
                if new.font_size != old.font_size {
                    run.font_size = None;
                }
                if new.bold != old.bold {
                    run.bold = None;
                }
                if new.italic != old.italic {
                    run.italic = None;
                }
                if new.color != old.color {
                    run.color = None;
                }
                if new.font != old.font {
                    run.font = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_autofit_shrink_gives_a_whole_point_size() {
        // PowerPoint's PDFs (009, 032, 005): 16 pt and 13 pt at 90% are 14 pt and 12 pt, 18 pt at 90% is 16 pt, 12 pt at 92.5% is 11 pt.
        assert_eq!(autofit_font_size_pt(16.0, 90_000), 14.0);
        assert_eq!(autofit_font_size_pt(13.0, 90_000), 12.0);
        assert_eq!(autofit_font_size_pt(18.0, 90_000), 16.0);
        assert_eq!(autofit_font_size_pt(12.0, 92_500), 11.0);
        // The halves go up.
        assert_eq!(autofit_font_size_pt(10.0, 85_000), 9.0);
        assert_eq!(autofit_font_size_pt(20.0, 92_500), 19.0);
        assert_eq!(autofit_font_size_pt(20.0, 62_500), 13.0);
        assert_eq!(autofit_font_size_pt(28.0, 62_500), 18.0);
        assert_eq!(autofit_font_size_pt(4.0, 85_000), 3.0);
        assert_eq!(autofit_font_size_pt(16.0, 100_000), 16.0);
        assert_eq!(autofit_font_size_pt(10.5, 100_000), 10.5);
    }

    #[test]
    fn paragraph_format_excludes_next_paragraph_at_range_end() {
        let mut body = TextBody::plain("first\nsecond", TextStyle::default());
        body.format_paragraphs(0, 6, &ParagraphFormat { align: Some(TextAlign::Center), ..Default::default() });
        assert_eq!(body.paragraphs[0].align, Some(TextAlign::Center));
        assert_eq!(body.paragraphs[1].align, None);
        body.format_paragraphs(6, 6, &ParagraphFormat { space_before: Some(325), ..Default::default() });
        assert_eq!(body.paragraphs[0].space_before, 0);
        assert_eq!(body.paragraphs[1].space_before, 325);
    }


    #[test]
    fn script_fonts_preserve_unselected_and_empty_runs() {
        let outer = Run { text: "外A".into(), style: RunStyle { bold: Some(true), ..Default::default() } };
        let empty = Run { text: String::new(), style: RunStyle { field: Some(TextField::SlideNumber), ..Default::default() } };
        let mut body = TextBody::plain("", TextStyle::default());
        body.paragraphs[0].runs = vec![outer.clone(), empty.clone(), Run { text: "AB日本".into(), style: RunStyle::default() }, outer.clone()];
        body.format_runs(2, 6, &CharacterFormat { script_fonts: Some((Some("Latin".into()), Some("East".into()))), ..Default::default() });
        assert_eq!(body.text(), "外AAB日本外A");
        assert_eq!(body.paragraphs[0].runs[0], outer);
        assert_eq!(body.paragraphs[0].runs[1], empty);
        assert_eq!(body.paragraphs[0].runs.last(), Some(&outer));
        assert_eq!(body.paragraphs[0].runs[2].text, "AB");
        assert_eq!(body.paragraphs[0].runs[2].style.font.as_deref(), Some("Latin"));
        assert_eq!(body.paragraphs[0].runs[3].text, "日本");
        assert_eq!(body.paragraphs[0].runs[3].style.font.as_deref(), Some("East"));
        assert_eq!(body.paragraphs[0].runs[3].style.fallback_font.as_deref(), Some("Latin"));
    }


    #[test]
    fn font_size_steps_follow_powerpoint_list() {
        assert_eq!(step_font_size(1800, true), 2000);
        assert_eq!(step_font_size(1800, false), 1600);
        assert_eq!(step_font_size(1900, false), 1800);
        assert_eq!(step_font_size(9600, true), 10600);
        assert_eq!(step_font_size(800, false), 800);
    }

    #[test]
    fn plain_text_round_trips() {
        let body = TextBody::plain("一行目\n\n三行目", TextStyle::default());
        assert_eq!(body.paragraphs.len(), 3);
        assert_eq!(body.text(), "一行目\n\n三行目");
    }

    #[test]
    fn set_text_keeps_first_paragraph_format() {
        let mut body = TextBody {
            paragraphs: vec![Paragraph {
                runs: vec![Run { text: "a".into(), style: RunStyle { bold: Some(true), ..Default::default() } }],
                align: Some(TextAlign::Center),
                ..Default::default()
            }],
            ..TextBody::default()
        };
        body.set_text("x\ny");
        assert_eq!(body.paragraphs.len(), 2);
        assert!(body.paragraphs.iter().all(|p| p.align == Some(TextAlign::Center)));
        assert_eq!(body.paragraphs[1].runs[0].style.bold, Some(true));
    }

    fn two_runs() -> TextBody {
        let bold = RunStyle { bold: Some(true), ..Default::default() };
        TextBody {
            paragraphs: vec![
                Paragraph { runs: vec![Run { text: "ab".into(), style: RunStyle::default() }, Run { text: "cd".into(), style: bold }], ..Default::default() },
                Paragraph { runs: vec![Run { text: "ef".into(), style: RunStyle::default() }], ..Default::default() },
            ],
            ..TextBody::default()
        }
    }

    #[test]
    fn splice_inside_a_run_keeps_the_runs() {
        let mut body = two_runs();
        body.splice(3, 1, "X");
        assert_eq!(body.text(), "abcX\nef");
        assert_eq!(body.paragraphs[0].runs.len(), 2);
        assert_eq!(body.paragraphs[0].runs[1].text, "cX");
        // Typed at the end of a run, the text takes that run's format.
        body.splice(2, 0, "Y");
        assert_eq!(body.paragraphs[0].runs[0].text, "abY");
        // Deleting a whole run keeps it, empty.
        body.splice(3, 2, "");
        assert_eq!(body.paragraphs[0].runs.len(), 2);
        assert_eq!(body.text(), "abY\nef");
    }

    #[test]
    fn splice_splits_and_joins_paragraphs() {
        let mut body = two_runs();
        body.splice(1, 0, "1\n2");
        assert_eq!(body.text(), "a1\n2bcd\nef");
        assert_eq!(body.paragraphs.len(), 3);
        assert_eq!(body.paragraphs[1].runs.last().unwrap().style.bold, Some(true));
        body.splice(2, 1, "");
        assert_eq!(body.text(), "a12bcd\nef");
        body.splice(5, 3, "");
        assert_eq!(body.text(), "a12bcf");
        assert_eq!(body.paragraphs.len(), 1);
        let mut empty = TextBody::default();
        empty.splice(0, 0, "new");
        assert_eq!(empty.text(), "new");
    }

    #[test]
    fn apply_style_drops_changed_overrides_only() {
        let mut body = TextBody::default();
        body.paragraphs[0].runs = vec![Run {
            text: "a".into(),
            style: RunStyle { font_size: Some(4000), bold: Some(true), ..Default::default() },
        }];
        let style = TextStyle { bold: true, ..body.style.clone() };
        body.apply_style(style);
        let run = &body.paragraphs[0].runs[0].style;
        assert_eq!(run.bold, None);
        assert_eq!(run.font_size, Some(4000));
    }

    #[test]
    fn numbered_list_format_with_custom_start_at() {
        let mut body = TextBody::default();
        body.set_text("First\nSecond\nThird");
        // Apply numbered list starting at 5.
        body.format_paragraphs(0, body.text().chars().count(), &ParagraphFormat {
            list: Some(ListKind::Numbered),
            start_at: Some(5),
            ..Default::default()
        });
        assert_eq!(body.paragraphs[0].bullet.as_ref().unwrap().text, "5.");
        assert_eq!(body.paragraphs[0].bullet.as_ref().unwrap().auto.as_ref().unwrap().start_at, 5);
        assert_eq!(body.paragraphs[1].bullet.as_ref().unwrap().text, "6.");
        assert_eq!(body.paragraphs[1].bullet.as_ref().unwrap().auto.as_ref().unwrap().start_at, 1);
        assert_eq!(body.paragraphs[2].bullet.as_ref().unwrap().text, "7.");
        assert_eq!(body.paragraphs[2].bullet.as_ref().unwrap().auto.as_ref().unwrap().start_at, 1);

        // Update start_at only to 10 without re-specifying list kind.
        body.format_paragraphs(0, body.text().chars().count(), &ParagraphFormat {
            start_at: Some(10),
            ..Default::default()
        });
        assert_eq!(body.paragraphs[0].bullet.as_ref().unwrap().text, "10.");
        assert_eq!(body.paragraphs[0].bullet.as_ref().unwrap().auto.as_ref().unwrap().start_at, 10);
        assert_eq!(body.paragraphs[1].bullet.as_ref().unwrap().text, "11.");
        assert_eq!(body.paragraphs[2].bullet.as_ref().unwrap().text, "12.");
    }
}

