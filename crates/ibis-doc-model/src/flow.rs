// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Flowing documents (Word): paragraphs and tables that are laid out into
//! pages by the viewer, which knows the fonts' measures.
//!
//! Formatting is resolved when the document is read (styles, direct
//! formatting and theme fonts folded into each paragraph and run), so the
//! layout only has to break lines and pages.

use crate::drawing::{Rgb, Stroke};
use crate::geometry::{Emu, EmuSize};
use crate::merge::FieldMapData;
use crate::model::{MediaId, Shape};
use crate::text::FontAlign;

/// 複数ページ設定（見開きページ、袋とじ、本（袋とじ）、1枚に2ページ）
/// ECMA-376 Part 1 §17.15.1.58 (mirrorMargins), §17.15.1.11 (bookFoldPrinting),
/// §17.15.1.12 (bookFoldRevPrinting), §17.15.1.70 (printTwoOnOne).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MultiplePages {
    /// 通常 (Normal).
    #[default]
    Normal,
    /// 見開きページ (`w:mirrorMargins`).
    MirrorMargins,
    /// 袋とじ (`w:bookFoldPrinting`).
    BookFold,
    /// 本（袋とじ） (`w:bookFoldRevPrinting`).
    ReverseBookFold,
    /// 1枚に2ページ (`w:printTwoOnOne`).
    TwoOnOne,
}

impl MultiplePages {
    pub const ALL: [MultiplePages; 5] = [
        Self::Normal,
        Self::MirrorMargins,
        Self::BookFold,
        Self::ReverseBookFold,
        Self::TwoOnOne,
    ];

    /// Whether inside and outside margins are mirrored on facing pages.
    pub fn is_mirrored(&self) -> bool {
        matches!(self, Self::MirrorMargins | Self::BookFold | Self::ReverseBookFold)
    }

    /// Whether this multiple pages mode involves booklet printing.
    pub fn is_book_fold(&self) -> bool {
        matches!(self, Self::BookFold | Self::ReverseBookFold)
    }
}

/// Page size, margins and the distances of the header and footer from the
/// page's edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSetup {
    pub size: EmuSize,
    pub top: Emu,
    pub right: Emu,
    pub bottom: Emu,
    pub left: Emu,
    pub header: Emu,
    pub footer: Emu,
    /// The line grid's pitch (`w:docGrid/@w:linePitch`), for paragraphs
    /// that snap to it; `None` without a grid.
    pub line_pitch: Option<Emu>,
    /// 文字ピッチ・字間 (`w:docGrid/@w:charSpace`).
    pub char_space: Option<Emu>,
    /// 文書グリッドの種類 (`w:docGrid/@w:type`).
    pub doc_grid_type: DocGridType,
    /// The number of the first page.
    pub first_page: u32,
    /// Page borders (`w:pgBorders`): top, right, bottom, left.
    pub borders: [Option<FlowBorder>; 4],
    /// The borders' `space` is from the text (the margins), else from the
    /// page's edges.
    pub borders_from_text: bool,
    /// とじしろ (`w:pgMar/@w:gutter`): added to the left margin (to the
    /// inside one with mirrored margins), or to the top with
    /// `gutter_at_top`.
    pub gutter: Emu,
    /// とじしろの位置: 上 (`w:gutterAtTop` in the settings: the document's).
    pub gutter_at_top: bool,
    /// 見開きページ (`w:mirrorMargins` in the settings: the document's): the
    /// left and right margins are the inside and outside ones, swapped on
    /// even pages.
    pub mirror_margins: bool,
    /// 複数ページ設定 (`w:mirrorMargins`, `w:bookFoldPrinting`, `w:bookFoldRevPrinting`, `w:printTwoOnOne`).
    pub multiple_pages: MultiplePages,
    /// 袋とじの印刷枚数 (`w:bookFoldPrintingSheets/@w:val`): None は「すべて」(all pages in 1 booklet)。
    pub book_fold_sheets: Option<u32>,
    /// 垂直方向の配置 (`w:vAlign`): where the body sits on its pages.
    pub v_align: PageVAlign,
    /// 脚注 and 文末脚注の設定 (`w:footnotePr`, `w:endnotePr`).
    pub footnotes: NoteSettings,
    pub endnotes: NoteSettings,
    /// The text's columns (段組み, `w:cols`).
    pub columns: PageColumns,
    /// 行番号 (`w:lnNumType`).
    pub line_numbers: Option<LineNumbers>,
    /// 文字列の方向 (`w:sectPr/w:textDirection`): across (`LrTb`), down with
    /// the lines right to left (`TbRl`, 縦書き) or up (`BtLr`).
    pub direction: CellDirection,
}

/// 行番号 (`w:lnNumType`): every `count_by`th line numbered, from `start`,
/// `distance` (EMU; 0: Word's quarter inch) left of the text, counted again
/// on each page, in each section, or on through the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineNumbers {
    pub count_by: u32,
    pub start: u32,
    pub distance: Emu,
    pub restart: LineNumberRestart,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineNumberRestart {
    #[default]
    NewPage,
    NewSection,
    Continuous,
}

/// A section's columns (`w:cols`): how many, their widths and the room
/// after each (EMU; equal when `w:equalWidth` is not off), and a line
/// between them (`w:sep`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageColumns {
    pub count: u8,
    /// Each column's width and the room after it, when not equal (up to
    /// [`PageColumns::MAX`]).
    pub widths: Option<[(Emu, Emu); PageColumns::MAX]>,
    /// The room between equal columns.
    pub space: Emu,
    pub separator: bool,
}

impl PageColumns {
    pub const MAX: usize = 12;

    /// The columns' left edges from the text's and widths, in `width` of
    /// text (EMU).
    pub fn layout(&self, width: Emu) -> Vec<(Emu, Emu)> {
        let n = (self.count as usize).clamp(1, Self::MAX);
        match &self.widths {
            Some(cols) => {
                let mut x = 0;
                cols[..n].iter().map(|&(w, space)| {
                    let at = x;
                    x += w + space;
                    (at, w)
                }).collect()
            }
            None => {
                let w = (width - self.space * (n as Emu - 1)) / n as Emu;
                (0..n).map(|i| (i as Emu * (w + self.space), w)).collect()
            }
        }
    }
}

impl Default for PageColumns {
    fn default() -> Self {
        Self { count: 1, widths: None, space: 425 * 635, separator: false }
    }
}

/// How notes are numbered and placed (`w:footnotePr` / `w:endnotePr`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteSettings {
    pub format: NoteFormat,
    /// `w:numStart`.
    pub start: u32,
    pub restart: NoteRestart,
    pub position: NotePosition,
}

impl NoteSettings {
    /// Word's defaults: footnotes 1, 2, 3 at the page's bottom; endnotes
    /// i, ii, iii at the document's end.
    pub const FOOTNOTES: NoteSettings = NoteSettings { format: NoteFormat::Decimal, start: 1, restart: NoteRestart::Continuous, position: NotePosition::PageBottom };
    pub const ENDNOTES: NoteSettings = NoteSettings { format: NoteFormat::LowerRoman, start: 1, restart: NoteRestart::Continuous, position: NotePosition::DocumentEnd };
}

/// The numbers' format (`w:numFmt`), those Word's note dialog lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteFormat {
    Decimal,
    UpperRoman,
    LowerRoman,
    UpperLetter,
    LowerLetter,
    /// `chicago`: *, †, ‡, §.
    Symbol,
    DecimalEnclosedCircle,
    DecimalFullWidth,
}

impl NoteFormat {
    pub const ALL: [NoteFormat; 8] = [Self::Decimal, Self::UpperRoman, Self::LowerRoman, Self::UpperLetter, Self::LowerLetter, Self::Symbol, Self::DecimalEnclosedCircle, Self::DecimalFullWidth];

    pub fn ooxml(self) -> &'static str {
        match self {
            Self::Decimal => "decimal",
            Self::UpperRoman => "upperRoman",
            Self::LowerRoman => "lowerRoman",
            Self::UpperLetter => "upperLetter",
            Self::LowerLetter => "lowerLetter",
            Self::Symbol => "chicago",
            Self::DecimalEnclosedCircle => "decimalEnclosedCircle",
            Self::DecimalFullWidth => "decimalFullWidth",
        }
    }

    pub fn from_ooxml(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.ooxml() == value)
    }

    /// Note `n` (1 the first) written in the format.
    pub fn number(self, n: u32) -> String {
        match self {
            Self::Symbol => {
                // *, †, ‡, §, then doubled, tripled…
                let marks = ['*', '†', '‡', '§'];
                let i = (n.max(1) - 1) as usize;
                std::iter::repeat_n(marks[i % 4], i / 4 + 1).collect()
            }
            other => format_list_number(n, other.ooxml()),
        }
    }
}

/// When note numbers start again (`w:numRestart`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoteRestart {
    #[default]
    Continuous,
    EachSection,
    EachPage,
}

/// Where notes go (`w:pos`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotePosition {
    /// Footnotes: at the page's bottom.
    #[default]
    PageBottom,
    /// Footnotes: right under the page's text.
    BeneathText,
    /// Endnotes: at the section's end.
    SectionEnd,
    /// Endnotes: at the document's end.
    DocumentEnd,
}

/// The body's place on its pages (`w:vAlign`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageVAlign {
    #[default]
    Top,
    Center,
    Bottom,
    /// 両端揃え (`both`): laid out as at the top here.
    Both,
}

/// 文書グリッドの種類 (`w:docGrid/@w:type`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DocGridType {
    /// 文字と行のグリッドなし (`default`).
    #[default]
    Default,
    /// 行数だけを指定する (`lines`).
    Lines,
    /// 文字数と行数を指定する (`linesAndChars`).
    LinesAndChars,
    /// 文字グリッドのみ (`snapToChars`).
    SnapToChars,
}

impl DocGridType {
    pub fn ooxml(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Lines => "lines",
            Self::LinesAndChars => "linesAndChars",
            Self::SnapToChars => "snapToChars",
        }
    }
}

/// The view mode of a Word document (`w:view` in `word/settings.xml`, ECMA-376 Part 1 §17.15.1.92).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DocumentViewMode {
    #[default]
    Print,
    Reading,
    Web,
    Outline,
    Draft,
}

impl DocumentViewMode {
    pub fn as_wval(&self) -> &'static str {
        match self {
            Self::Print => "print",
            Self::Reading => "reading",
            Self::Web => "web",
            Self::Outline => "outline",
            Self::Draft => "normal",
        }
    }

    pub fn from_wval(s: &str) -> Self {
        match s {
            "reading" => Self::Reading,
            "web" => Self::Web,
            "outline" => Self::Outline,
            "normal" | "draft" | "none" => Self::Draft,
            _ => Self::Print,
        }
    }
}

/// 文書の編集制限・保護モード (`w:documentProtection/@w:edit`, ECMA-376 Part 1 §17.15.1.29).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DocumentProtection {
    #[default]
    None,
    /// 変更不可（読み取り専用 / `readOnly`、DOCX-SEC-002）。
    ReadOnly,
    /// フォームへの入力のみ許可 (`forms`、DOCX-SEC-004）。
    Forms,
    /// コメントのみ許可 (`comments`)。
    Comments,
    /// 変更履歴の記録のみ許可 (`trackedChanges`、DOCX-SEC-005 / DOCX-REVIEW-010）。
    TrackedChanges,
}

impl DocumentProtection {
    pub fn as_wedit(&self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::ReadOnly => Some("readOnly"),
            Self::Forms => Some("forms"),
            Self::Comments => Some("comments"),
            Self::TrackedChanges => Some("trackedChanges"),
        }
    }

    pub fn from_wedit(s: &str) -> Self {
        match s {
            "readOnly" => Self::ReadOnly,
            "forms" => Self::Forms,
            "comments" => Self::Comments,
            "trackedChanges" => Self::TrackedChanges,
            _ => Self::None,
        }
    }
}

/// Word の 閲覧・レビュー・編集モード（DOCX-VIEW-021）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DocumentInteractionMode {
    /// 編集: ドキュメントを直接編集する通常モード。
    #[default]
    Editing,
    /// レビュー: 変更履歴の記録がオンになり、変更履歴とコメントを許可。
    Reviewing,
    /// 閲覧: 誤入力を防ぐため、文書全体を読み取り専用として表示。
    Viewing,
}

impl Default for PageSetup {
    /// A4 with Word's default margins.
    fn default() -> Self {
        Self {
            size: EmuSize::new(7_560_310, 10_692_130),
            top: 1_080_135,
            right: 1_080_135,
            bottom: 1_080_135,
            left: 1_080_135,
            header: 540_385,
            footer: 540_385,
            line_pitch: None,
            char_space: None,
            doc_grid_type: DocGridType::Default,
            first_page: 1,
            borders: [None; 4],
            borders_from_text: false,
            gutter: 0,
            gutter_at_top: false,
            mirror_margins: false,
            multiple_pages: MultiplePages::Normal,
            book_fold_sheets: None,
            v_align: PageVAlign::Top,
            footnotes: NoteSettings::FOOTNOTES,
            endnotes: NoteSettings::ENDNOTES,
            columns: PageColumns::default(),
            line_numbers: None,
            direction: CellDirection::LrTb,
        }
    }
}

impl PageSetup {
    /// 見開き（偶数ページで左右余白を入れ替える）かどうか。
    /// 見開きページ、袋とじ、本（袋とじ）のいずれか、または mirror_margins が有効な場合。
    pub fn is_mirrored(&self) -> bool {
        self.mirror_margins || self.multiple_pages.is_mirrored()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlowAlign {
    #[default]
    Left,
    Center,
    Right,
    /// Both edges, the last line to the left.
    Justify,
    /// Both edges, the last line too.
    Distribute,
}

/// The height of a paragraph's lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineHeight {
    /// A multiple of single spacing, in 240ths (240 is single).
    Auto(u32),
    AtLeast(Emu),
    Exact(Emu),
}

impl Default for LineHeight {
    fn default() -> Self {
        LineHeight::Auto(240)
    }
}

/// Presets for document-wide paragraph spacing (Word's デザイン > ドキュメントの書式設定 > 段落の間隔).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParagraphSpacingPreset {
    Default,
    NoSpace,
    Compact,
    Tight,
    Open,
    Relaxed,
    Double,
}

impl ParagraphSpacingPreset {
    /// Spacing before (EMU), spacing after (EMU), and line height.
    pub fn spacing(self) -> (Emu, Emu, LineHeight) {
        match self {
            ParagraphSpacingPreset::Default => (0, 0, LineHeight::Auto(240)),
            ParagraphSpacingPreset::NoSpace => (0, 0, LineHeight::Auto(240)),
            ParagraphSpacingPreset::Compact => (0, 4 * 12_700, LineHeight::Auto(240)),
            ParagraphSpacingPreset::Tight => (0, 6 * 12_700, LineHeight::Auto(276)),
            ParagraphSpacingPreset::Open => (0, 10 * 12_700, LineHeight::Auto(276)),
            ParagraphSpacingPreset::Relaxed => (0, 6 * 12_700, LineHeight::Auto(360)),
            ParagraphSpacingPreset::Double => (0, 8 * 12_700, LineHeight::Auto(480)),
        }
    }
}

/// Presets for document-wide style sets (Word's デザイン > ドキュメントの書式設定 > スタイル セット).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StyleSetPreset {
    Default,
    Casual,
    Centered,
    Formal,
    Elegant,
    Modern,
    Simple,
    Minimalist,
}

impl StyleSetPreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Casual => "casual",
            Self::Centered => "centered",
            Self::Formal => "formal",
            Self::Elegant => "elegant",
            Self::Modern => "modern",
            Self::Simple => "simple",
            Self::Minimalist => "minimalist",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "default" => Some(Self::Default),
            "casual" => Some(Self::Casual),
            "centered" => Some(Self::Centered),
            "formal" => Some(Self::Formal),
            "elegant" => Some(Self::Elegant),
            "modern" => Some(Self::Modern),
            "simple" => Some(Self::Simple),
            "minimalist" => Some(Self::Minimalist),
            _ => None,
        }
    }

    /// Apply the style set formatting to document styles.
    pub fn apply(self, styles: &mut FlowStyles) {
        let normal_id = styles.default_paragraph.clone().or_else(|| {
            styles.paragraph_style_named("Normal").map(str::to_string)
        });

        let mut ensure_or_update = |name: &str, priority: u32, update_fn: &dyn Fn(&mut FlowStyle)| {
            if let Some(id) = styles.paragraph_style_named(name).map(str::to_string) {
                if let Some(st) = styles.styles.get_mut(&id) {
                    st.run.theme_color = None;
                    update_fn(st);
                }
            } else {
                let mut style_id = name.replace(' ', "");
                while styles.styles.contains_key(&style_id) {
                    style_id.push('_');
                }
                let mut st = FlowStyle {
                    kind: StyleKind::Paragraph,
                    name: name.to_string(),
                    based_on: normal_id.clone(),
                    para: ParaFormat::default(),
                    run: RunFormat::default(),
                    next: normal_id.clone(),
                    auto_update: false,
                    ui_priority: Some(priority),
                    quick: true,
                    conditional: Vec::new(),
                    num_id: None,
                };
                update_fn(&mut st);
                styles.styles.insert(style_id, st);
            }
        };

        match self {
            Self::Default => {
                ensure_or_update("Title", 10, &|s| {
                    s.run.size = Some(2800);
                    s.run.bold = Some(false);
                    s.run.color = Some(crate::drawing::Rgb(0x2E74B5));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(0);
                    s.para.after = Some(120 * 635);
                });
                ensure_or_update("Subtitle", 11, &|s| {
                    s.run.size = Some(1100);
                    s.run.italic = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x5A5A5A));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(0);
                    s.para.after = Some(160 * 635);
                });
                ensure_or_update("heading 1", 1, &|s| {
                    s.run.size = Some(1600);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x2E74B5));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(240 * 635);
                    s.para.after = Some(60 * 635);
                });
                ensure_or_update("heading 2", 2, &|s| {
                    s.run.size = Some(1300);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x2E74B5));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(180 * 635);
                    s.para.after = Some(40 * 635);
                });
                ensure_or_update("heading 3", 3, &|s| {
                    s.run.size = Some(1200);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x1F4E79));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(120 * 635);
                    s.para.after = Some(40 * 635);
                });
            }
            Self::Casual => {
                ensure_or_update("Title", 10, &|s| {
                    s.run.size = Some(2600);
                    s.run.bold = Some(true);
                    s.run.italic = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x7030A0));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.after = Some(140 * 635);
                });
                ensure_or_update("Subtitle", 11, &|s| {
                    s.run.size = Some(1200);
                    s.run.italic = Some(false);
                    s.run.color = Some(crate::drawing::Rgb(0x7030A0));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.after = Some(160 * 635);
                });
                ensure_or_update("heading 1", 1, &|s| {
                    s.run.size = Some(1500);
                    s.run.bold = Some(true);
                    s.run.italic = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x7030A0));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(200 * 635);
                    s.para.after = Some(60 * 635);
                });
                ensure_or_update("heading 2", 2, &|s| {
                    s.run.size = Some(1300);
                    s.run.bold = Some(true);
                    s.run.italic = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x7030A0));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(160 * 635);
                    s.para.after = Some(40 * 635);
                });
                ensure_or_update("heading 3", 3, &|s| {
                    s.run.size = Some(1100);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x3B3838));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(120 * 635);
                    s.para.after = Some(40 * 635);
                });
            }
            Self::Centered => {
                ensure_or_update("Title", 10, &|s| {
                    s.run.size = Some(2600);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x1F4E79));
                    s.para.align = Some(FlowAlign::Center);
                    s.para.after = Some(160 * 635);
                });
                ensure_or_update("Subtitle", 11, &|s| {
                    s.run.size = Some(1200);
                    s.run.italic = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x595959));
                    s.para.align = Some(FlowAlign::Center);
                    s.para.after = Some(200 * 635);
                });
                ensure_or_update("heading 1", 1, &|s| {
                    s.run.size = Some(1600);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x1F4E79));
                    s.para.align = Some(FlowAlign::Center);
                    s.para.before = Some(240 * 635);
                    s.para.after = Some(80 * 635);
                });
                ensure_or_update("heading 2", 2, &|s| {
                    s.run.size = Some(1300);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x1F4E79));
                    s.para.align = Some(FlowAlign::Center);
                    s.para.before = Some(180 * 635);
                    s.para.after = Some(60 * 635);
                });
                ensure_or_update("heading 3", 3, &|s| {
                    s.run.size = Some(1200);
                    s.run.bold = Some(false);
                    s.run.color = Some(crate::drawing::Rgb(0x595959));
                    s.para.align = Some(FlowAlign::Center);
                    s.para.before = Some(120 * 635);
                    s.para.after = Some(40 * 635);
                });
            }
            Self::Formal => {
                ensure_or_update("Title", 10, &|s| {
                    s.run.size = Some(2400);
                    s.run.bold = Some(true);
                    s.run.font = Some("Times New Roman".to_string());
                    s.run.color = Some(crate::drawing::Rgb(0x000000));
                    s.para.align = Some(FlowAlign::Center);
                    s.para.after = Some(140 * 635);
                });
                ensure_or_update("Subtitle", 11, &|s| {
                    s.run.size = Some(1100);
                    s.run.italic = Some(true);
                    s.run.font = Some("Times New Roman".to_string());
                    s.run.color = Some(crate::drawing::Rgb(0x333333));
                    s.para.align = Some(FlowAlign::Center);
                    s.para.after = Some(180 * 635);
                });
                ensure_or_update("heading 1", 1, &|s| {
                    s.run.size = Some(1400);
                    s.run.bold = Some(true);
                    s.run.font = Some("Times New Roman".to_string());
                    s.run.color = Some(crate::drawing::Rgb(0x000000));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(240 * 635);
                    s.para.after = Some(60 * 635);
                });
                ensure_or_update("heading 2", 2, &|s| {
                    s.run.size = Some(1200);
                    s.run.bold = Some(true);
                    s.run.italic = Some(true);
                    s.run.font = Some("Times New Roman".to_string());
                    s.run.color = Some(crate::drawing::Rgb(0x000000));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(180 * 635);
                    s.para.after = Some(40 * 635);
                });
                ensure_or_update("heading 3", 3, &|s| {
                    s.run.size = Some(1100);
                    s.run.bold = Some(true);
                    s.run.font = Some("Times New Roman".to_string());
                    s.run.color = Some(crate::drawing::Rgb(0x333333));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(120 * 635);
                    s.para.after = Some(40 * 635);
                });
            }
            Self::Elegant => {
                ensure_or_update("Title", 10, &|s| {
                    s.run.size = Some(2600);
                    s.run.bold = Some(false);
                    s.run.italic = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x385723));
                    s.para.align = Some(FlowAlign::Center);
                    s.para.after = Some(180 * 635);
                });
                ensure_or_update("Subtitle", 11, &|s| {
                    s.run.size = Some(1150);
                    s.run.italic = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x548235));
                    s.para.align = Some(FlowAlign::Center);
                    s.para.after = Some(200 * 635);
                });
                ensure_or_update("heading 1", 1, &|s| {
                    s.run.size = Some(1500);
                    s.run.bold = Some(false);
                    s.run.italic = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x385723));
                    s.para.align = Some(FlowAlign::Center);
                    s.para.before = Some(240 * 635);
                    s.para.after = Some(80 * 635);
                });
                ensure_or_update("heading 2", 2, &|s| {
                    s.run.size = Some(1300);
                    s.run.bold = Some(false);
                    s.run.italic = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x548235));
                    s.para.align = Some(FlowAlign::Center);
                    s.para.before = Some(180 * 635);
                    s.para.after = Some(60 * 635);
                });
                ensure_or_update("heading 3", 3, &|s| {
                    s.run.size = Some(1150);
                    s.run.bold = Some(false);
                    s.run.italic = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x70AD47));
                    s.para.align = Some(FlowAlign::Center);
                    s.para.before = Some(120 * 635);
                    s.para.after = Some(40 * 635);
                });
            }
            Self::Modern => {
                ensure_or_update("Title", 10, &|s| {
                    s.run.size = Some(2800);
                    s.run.bold = Some(true);
                    s.run.font = Some("Arial".to_string());
                    s.run.color = Some(crate::drawing::Rgb(0xC00000));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.after = Some(120 * 635);
                });
                ensure_or_update("Subtitle", 11, &|s| {
                    s.run.size = Some(1200);
                    s.run.bold = Some(false);
                    s.run.font = Some("Arial".to_string());
                    s.run.color = Some(crate::drawing::Rgb(0x595959));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.after = Some(160 * 635);
                });
                ensure_or_update("heading 1", 1, &|s| {
                    s.run.size = Some(1600);
                    s.run.bold = Some(true);
                    s.run.font = Some("Arial".to_string());
                    s.run.color = Some(crate::drawing::Rgb(0xC00000));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(240 * 635);
                    s.para.after = Some(60 * 635);
                });
                ensure_or_update("heading 2", 2, &|s| {
                    s.run.size = Some(1400);
                    s.run.bold = Some(true);
                    s.run.font = Some("Arial".to_string());
                    s.run.color = Some(crate::drawing::Rgb(0x262626));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(180 * 635);
                    s.para.after = Some(40 * 635);
                });
                ensure_or_update("heading 3", 3, &|s| {
                    s.run.size = Some(1200);
                    s.run.bold = Some(true);
                    s.run.font = Some("Arial".to_string());
                    s.run.color = Some(crate::drawing::Rgb(0x595959));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(120 * 635);
                    s.para.after = Some(40 * 635);
                });
            }
            Self::Simple => {
                ensure_or_update("Title", 10, &|s| {
                    s.run.size = Some(2200);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x000000));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.after = Some(100 * 635);
                });
                ensure_or_update("Subtitle", 11, &|s| {
                    s.run.size = Some(1100);
                    s.run.bold = Some(false);
                    s.run.color = Some(crate::drawing::Rgb(0x595959));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.after = Some(140 * 635);
                });
                ensure_or_update("heading 1", 1, &|s| {
                    s.run.size = Some(1400);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x000000));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(180 * 635);
                    s.para.after = Some(60 * 635);
                });
                ensure_or_update("heading 2", 2, &|s| {
                    s.run.size = Some(1200);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x333333));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(140 * 635);
                    s.para.after = Some(40 * 635);
                });
                ensure_or_update("heading 3", 3, &|s| {
                    s.run.size = Some(1100);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x595959));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(100 * 635);
                    s.para.after = Some(30 * 635);
                });
            }
            Self::Minimalist => {
                ensure_or_update("Title", 10, &|s| {
                    s.run.size = Some(2000);
                    s.run.bold = Some(false);
                    s.run.color = Some(crate::drawing::Rgb(0x262626));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.after = Some(80 * 635);
                });
                ensure_or_update("Subtitle", 11, &|s| {
                    s.run.size = Some(1050);
                    s.run.bold = Some(false);
                    s.run.color = Some(crate::drawing::Rgb(0x7F7F7F));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.after = Some(100 * 635);
                });
                ensure_or_update("heading 1", 1, &|s| {
                    s.run.size = Some(1300);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x262626));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(120 * 635);
                    s.para.after = Some(40 * 635);
                });
                ensure_or_update("heading 2", 2, &|s| {
                    s.run.size = Some(1150);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x595959));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(80 * 635);
                    s.para.after = Some(30 * 635);
                });
                ensure_or_update("heading 3", 3, &|s| {
                    s.run.size = Some(1050);
                    s.run.bold = Some(true);
                    s.run.color = Some(crate::drawing::Rgb(0x7F7F7F));
                    s.para.align = Some(FlowAlign::Left);
                    s.para.before = Some(60 * 635);
                    s.para.after = Some(20 * 635);
                });
            }
        }
    }
}

/// How an underline is drawn (`ST_Underline`, but `none`, which is no
/// underline).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UnderlineStyle {
    #[default]
    Single,
    /// Under the words, not the spaces between them.
    Words,
    Double,
    Thick,
    Dotted,
    DottedHeavy,
    Dash,
    DashedHeavy,
    DashLong,
    DashLongHeavy,
    DotDash,
    DashDotHeavy,
    DotDotDash,
    DashDotDotHeavy,
    Wave,
    WavyHeavy,
    WavyDouble,
}

impl UnderlineStyle {
    /// The value's name in `w:u/@w:val`.
    pub fn name(self) -> &'static str {
        use UnderlineStyle::*;
        match self {
            Single => "single",
            Words => "words",
            Double => "double",
            Thick => "thick",
            Dotted => "dotted",
            DottedHeavy => "dottedHeavy",
            Dash => "dash",
            DashedHeavy => "dashedHeavy",
            DashLong => "dashLong",
            DashLongHeavy => "dashLongHeavy",
            DotDash => "dotDash",
            DashDotHeavy => "dashDotHeavy",
            DotDotDash => "dotDotDash",
            DashDotDotHeavy => "dashDotDotHeavy",
            Wave => "wave",
            WavyHeavy => "wavyHeavy",
            WavyDouble => "wavyDouble",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        use UnderlineStyle::*;
        [Single, Words, Double, Thick, Dotted, DottedHeavy, Dash, DashedHeavy, DashLong, DashLongHeavy, DotDash, DashDotHeavy, DotDotDash, DashDotDotHeavy, Wave, WavyHeavy, WavyDouble].into_iter().find(|s| s.name() == name)
    }

    /// Drawn thick (`thick` and the heavy ones).
    pub fn heavy(self) -> bool {
        use UnderlineStyle::*;
        matches!(self, Thick | DottedHeavy | DashedHeavy | DashLongHeavy | DashDotHeavy | DashDotDotHeavy | WavyHeavy)
    }

    /// The dashes along the line, drawn and gap lengths repeating, in
    /// points (measured on Word's PDF; they do not grow with the text).
    pub fn dashes(self) -> Option<&'static [f32]> {
        use UnderlineStyle::*;
        Some(match self {
            Dotted | DottedHeavy => &[1.4, 1.5],
            Dash | DashedHeavy => &[3.9, 1.9],
            DashLong | DashLongHeavy => &[7.9, 3.9],
            DotDash | DashDotHeavy => &[2.9, 1.0, 1.0, 1.0],
            DotDotDash | DashDotDotHeavy => &[2.1, 0.75, 0.75, 0.75, 0.75, 0.75],
            _ => return None,
        })
    }

    /// Converts a Word (`ST_Underline`) underline style to a DrawingML [`crate::text::Underline`].
    pub fn to_drawing_underline(self) -> crate::text::Underline {
        use crate::text::Underline;
        use UnderlineStyle::*;
        match self {
            Single => Underline::Single,
            Words => Underline::Words,
            Double => Underline::Double,
            Thick => Underline::Heavy,
            Dotted => Underline::Dotted,
            DottedHeavy => Underline::DottedHeavy,
            Dash => Underline::Dash,
            DashedHeavy => Underline::DashHeavy,
            DashLong => Underline::DashLong,
            DashLongHeavy => Underline::DashLongHeavy,
            DotDash => Underline::DotDash,
            DashDotHeavy => Underline::DotDashHeavy,
            DotDotDash => Underline::DotDotDash,
            DashDotDotHeavy => Underline::DotDotDashHeavy,
            Wave => Underline::Wavy,
            WavyHeavy => Underline::WavyHeavy,
            WavyDouble => Underline::WavyDouble,
        }
    }

    /// Converts a DrawingML [`crate::text::Underline`] to a Word (`ST_Underline`) underline style.
    pub fn from_drawing_underline(u: crate::text::Underline) -> Self {
        use crate::text::Underline;
        use UnderlineStyle::*;
        match u {
            Underline::Single => Single,
            Underline::Words => Words,
            Underline::Double => Double,
            Underline::Heavy => Thick,
            Underline::Dotted => Dotted,
            Underline::DottedHeavy => DottedHeavy,
            Underline::Dash => Dash,
            Underline::DashHeavy => DashedHeavy,
            Underline::DashLong => DashLong,
            Underline::DashLongHeavy => DashLongHeavy,
            Underline::DotDash => DotDash,
            Underline::DotDashHeavy => DashDotHeavy,
            Underline::DotDotDash => DotDotDash,
            Underline::DotDotDashHeavy => DashDotDotHeavy,
            Underline::Wavy => Wave,
            Underline::WavyHeavy => WavyHeavy,
            Underline::WavyDouble => WavyDouble,
        }
    }
}

/// Where the section after a section break starts (`w:sectPr/w:type`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SectionBreak {
    /// No section ends here.
    #[default]
    None,
    /// On the same page (`continuous`).
    Continuous,
    /// On the next page (`nextPage`).
    NextPage,
    /// In the next column (`nextColumn`; the next page without columns).
    NextColumn,
    /// On the next odd or even page (a blank page between if needed).
    OddPage,
    EvenPage,
}

/// How ルビ is placed over its base (`w:rubyAlign`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RubyAlign {
    #[default]
    Center,
    /// 均等割り付け: the first character at the base's left, the last at
    /// its right.
    DistributeLetter,
    /// 1-2-1: equal room around each character.
    DistributeSpace,
    Left,
    Right,
    /// Beside the base (vertical text); drawn centred above here.
    RightVertical,
}

impl RubyAlign {
    pub fn from_name(name: &str) -> Self {
        match name {
            "distributeLetter" => Self::DistributeLetter,
            "distributeSpace" => Self::DistributeSpace,
            "left" => Self::Left,
            "right" => Self::Right,
            "rightVertical" => Self::RightVertical,
            _ => Self::Center,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Center => "center",
            Self::DistributeLetter => "distributeLetter",
            Self::DistributeSpace => "distributeSpace",
            Self::Left => "left",
            Self::Right => "right",
            Self::RightVertical => "rightVertical",
        }
    }
}

/// 組み文字 (`w:eastAsianLayout w:combine`): the run's characters in two
/// lines of half the size in the room of one, in brackets or not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Combine {
    #[default]
    Plain,
    Round,
    Square,
    Angle,
    Curly,
}

impl Combine {
    /// From `w:combineBrackets`.
    pub fn from_brackets(name: Option<&str>) -> Self {
        match name {
            Some("round") => Self::Round,
            Some("square") => Self::Square,
            Some("angle") => Self::Angle,
            Some("curly") => Self::Curly,
            _ => Self::Plain,
        }
    }

    /// Its brackets, if any.
    pub fn brackets(self) -> Option<(char, char)> {
        match self {
            Self::Plain => None,
            Self::Round => Some(('(', ')')),
            Self::Square => Some(('[', ']')),
            Self::Angle => Some(('<', '>')),
            Self::Curly => Some(('{', '}')),
        }
    }
}

/// 傍点 (`w:em`): a mark over (or under) each character.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EmphasisMark {
    #[default]
    None,
    /// A dot above (ゴマではなく点, Word's 「・」).
    Dot,
    /// A sesame comma above (、).
    Comma,
    /// A small circle above.
    Circle,
    /// A dot below.
    UnderDot,
}

impl EmphasisMark {
    /// From `w:em/@w:val`.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "none" => Self::None,
            "dot" => Self::Dot,
            "comma" => Self::Comma,
            "circle" => Self::Circle,
            "underDot" => Self::UnderDot,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Dot => "dot",
            Self::Comma => "comma",
            Self::Circle => "circle",
            Self::UnderDot => "underDot",
        }
    }
}

/// OpenType の合字 (`w14:ligatures`).
/// ST_Ligatures (MS-DOCX §2.5.31).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Ligatures {
    #[default]
    None,
    Standard,
    Contextual,
    Historical,
    Discretional,
    StandardContextual,
    StandardHistorical,
    ContextualHistorical,
    StandardDiscretional,
    ContextualDiscretional,
    HistoricalDiscretional,
    StandardContextualHistorical,
    StandardContextualDiscretional,
    StandardHistoricalDiscretional,
    ContextualHistoricalDiscretional,
    All,
}

impl Ligatures {
    pub fn from_ooxml(v: &str) -> Option<Self> {
        match v {
            "none" => Some(Self::None),
            "standard" => Some(Self::Standard),
            "contextual" => Some(Self::Contextual),
            "historical" => Some(Self::Historical),
            "discretional" | "discretionary" => Some(Self::Discretional),
            "standardContextual" => Some(Self::StandardContextual),
            "standardHistorical" => Some(Self::StandardHistorical),
            "contextualHistorical" => Some(Self::ContextualHistorical),
            "standardDiscretional" | "standardDiscretionary" => Some(Self::StandardDiscretional),
            "contextualDiscretional" | "contextualDiscretionary" => Some(Self::ContextualDiscretional),
            "historicalDiscretional" | "historicalDiscretionary" | "historicalAndDiscretionary" => Some(Self::HistoricalDiscretional),
            "standardContextualHistorical" => Some(Self::StandardContextualHistorical),
            "standardContextualDiscretional" | "standardContextualDiscretionary" => Some(Self::StandardContextualDiscretional),
            "standardHistoricalDiscretional" | "standardHistoricalDiscretionary" => Some(Self::StandardHistoricalDiscretional),
            "contextualHistoricalDiscretional" | "contextualHistoricalDiscretionary" => Some(Self::ContextualHistoricalDiscretional),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    pub fn to_ooxml(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Standard => "standard",
            Self::Contextual => "contextual",
            Self::Historical => "historical",
            Self::Discretional => "discretional",
            Self::StandardContextual => "standardContextual",
            Self::StandardHistorical => "standardHistorical",
            Self::ContextualHistorical => "contextualHistorical",
            Self::StandardDiscretional => "standardDiscretional",
            Self::ContextualDiscretional => "contextualDiscretional",
            Self::HistoricalDiscretional => "historicalDiscretional",
            Self::StandardContextualHistorical => "standardContextualHistorical",
            Self::StandardContextualDiscretional => "standardContextualDiscretional",
            Self::StandardHistoricalDiscretional => "standardHistoricalDiscretional",
            Self::ContextualHistoricalDiscretional => "contextualHistoricalDiscretional",
            Self::All => "all",
        }
    }
}

/// OpenType の数字のスタイル (`w14:numForm`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum NumberForm {
    #[default]
    Default,
    Lining,
    OldStyle,
}

impl NumberForm {
    pub fn from_ooxml(v: &str) -> Option<Self> {
        match v {
            "default" => Some(Self::Default),
            "lining" => Some(Self::Lining),
            "oldStyle" => Some(Self::OldStyle),
            _ => None,
        }
    }

    pub fn to_ooxml(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Lining => "lining",
            Self::OldStyle => "oldStyle",
        }
    }
}

/// OpenType の数字の間隔 (`w14:numSpacing`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum NumberSpacing {
    #[default]
    Default,
    Proportional,
    Tabular,
}

impl NumberSpacing {
    pub fn from_ooxml(v: &str) -> Option<Self> {
        match v {
            "default" => Some(Self::Default),
            "proportional" => Some(Self::Proportional),
            "tabular" => Some(Self::Tabular),
            _ => None,
        }
    }

    pub fn to_ooxml(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Proportional => "proportional",
            Self::Tabular => "tabular",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VerticalPosition {
    #[default]
    Baseline,
    Superscript,
    Subscript,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowRunStyle {
    /// Typeface for Latin text.
    pub font: Option<String>,
    /// Typeface for East Asian text.
    pub east_asian_font: Option<String>,
    /// Typeface for Complex Script text (Arabic, Hebrew, Thai, Indic, etc.).
    pub cs_font: Option<String>,
    /// Whether `w:rFonts/@w:hint="eastAsia"` selects the East Asian font for ambiguous characters.
    pub east_asia_hint: bool,
    /// Hundredths of a point.
    pub size: u32,
    /// Complex Script font size (hundredths of a point, `w:szCs`).
    pub cs_size: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// How the underline is drawn, and its colour (`None`: the text's).
    pub underline_style: UnderlineStyle,
    pub underline_color: Option<Rgb>,
    pub strike: bool,
    /// 二重取り消し線 (`w:dstrike`).
    pub double_strike: bool,
    pub caps: bool,
    pub small_caps: bool,
    pub color: Rgb,
    pub highlight: Option<Rgb>,
    /// The pattern of a run's shading (`w:shd` with a stripe or a grid) and its colour, over the highlight (its fill).
    pub shd_pattern: Option<(ShadingPattern, Rgb)>,
    /// Word 2010's text effects (`w14:textOutline`, `w14:glow`, `w14:shadow`, `w14:textFill`).
    pub effects: Option<std::sync::Arc<TextEffects>>,
    pub position: VerticalPosition,
    /// Added after each character, in EMU (`w:spacing`).
    pub spacing: Emu,
    /// Horizontal scale in percent (`w:w`).
    pub scale: u32,
    /// The width in twips a run of characters is fitted to and the id that
    /// joins the runs of one span (`w:fitText`).
    pub fit_text: Option<(u32, u32)>,
    /// Run border line (`w:bdr`).
    pub border: Option<FlowBorder>,
    /// Pair kerning from this size up, in half-points (`w:kern`); `None`:
    /// no kerning.
    pub kern: Option<u32>,
    /// 傍点 (`w:em`).
    pub emphasis: EmphasisMark,
    /// 組み文字 (`w:eastAsianLayout w:combine`).
    pub combine: Option<Combine>,
    /// 縦中横 (`w:eastAsianLayout w:vert`).
    pub tate_chu_yoko: bool,
    /// 隠し文字 (`w:vanish`).
    pub hidden: bool,
    /// 中抜き・輪郭 (`w:outline`).
    pub outline: bool,
    /// 影付き (`w:shadow`).
    pub shadow: bool,
    /// 浮き出し (`w:emboss`).
    pub emboss: bool,
    /// 浮き彫り (`w:imprint`).
    pub imprint: bool,
    /// OpenType 合字 (`w14:ligatures`).
    pub ligatures: Ligatures,
    /// OpenType 数字のスタイル (`w14:numForm`).
    pub num_form: NumberForm,
    /// OpenType 数字の間隔 (`w14:numSpacing`).
    pub num_spacing: NumberSpacing,
    /// OpenType 前後関係に依存する異体字 (`w14:cntxtAlts`).
    pub contextual_alternates: bool,
    /// An `EQ \o\ac` field's sizes (hundredths of a point) of its enclosure and of
    /// the character in it, as the runs of its code have them (`None`: the run's own).
    pub enclosure_sizes: Option<(u32, u32)>,
}

impl Default for FlowRunStyle {
    /// Word's default: 10.5 pt, black.
    fn default() -> Self {
        Self {
            font: None,
            east_asian_font: None,
            cs_font: None,
            east_asia_hint: false,
            size: 1050,
            cs_size: None,
            bold: false,
            italic: false,
            underline: false,
            underline_style: UnderlineStyle::Single,
            underline_color: None,
            strike: false,
            double_strike: false,
            caps: false,
            small_caps: false,
            color: Rgb::BLACK,
            highlight: None,
            shd_pattern: None,
            effects: None,
            position: VerticalPosition::Baseline,
            spacing: 0,
            scale: 100,
            fit_text: None,
            border: None,
            kern: None,
            emphasis: EmphasisMark::None,
            combine: None,
            tate_chu_yoko: false,
            hidden: false,
            outline: false,
            shadow: false,
            emboss: false,
            imprint: false,
            ligatures: Ligatures::None,
            num_form: NumberForm::Default,
            num_spacing: NumberSpacing::Default,
            contextual_alternates: false,
            enclosure_sizes: None,
        }
    }
}

/// Whether a character belongs to a Complex Script (Arabic, Hebrew, Indic, Thai, etc.).
pub fn is_complex_script(c: char) -> bool {
    let u = c as u32;
    matches!(
        u,
        0x0590..=0x08FF // Hebrew, Arabic, Syriac, Thaana, NKo, Samaritan, Mandaic, Arabic Ext-A
        | 0x0900..=0x0DFF // Indic: Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada, Malayalam, Sinhala
        | 0x0E00..=0x0EFF // Thai, Lao
        | 0x0F00..=0x0FFF // Tibetan
        | 0x1000..=0x10FF // Myanmar, Georgian
        | 0x1780..=0x17FF // Khmer
        | 0x19E0..=0x19FF // Khmer Symbols
        | 0xA8E0..=0xA8FF // Devanagari Ext
        | 0xAA60..=0xAA7F // Myanmar Ext
        | 0xFB50..=0xFDFF // Arabic Presentation Forms-A
        | 0xFE70..=0xFEFF // Arabic Presentation Forms-B
    )
}

impl FlowRunStyle {
    /// The font size for a character.
    pub fn size_for(&self, c: char) -> u32 {
        if is_complex_script(c) {
            if let Some(sz) = self.cs_size {
                return sz;
            }
        }
        self.size
    }

    /// The typeface for a character.
    pub fn font_for(&self, c: char) -> Option<&str> {
        self.font_for_in(c, true)
    }

    /// `font_for`, where `paragraph_has_east_asian` says whether the paragraph has East Asian text. The punctuation
    /// from U+2010 up (dashes, curly quotes) is drawn in the East Asian font only in such a paragraph; among Latin
    /// text only (even in a document whose language is Japanese) Word draws it in the Latin font (DHDR: "Header — 1"
    /// is Calibri's em dash, 0.91 em wide, not the full-width one of 游ゴシック).
    pub fn font_for_in(&self, c: char, paragraph_has_east_asian: bool) -> Option<&str> {
        if is_complex_script(c) {
            if let Some(f) = self.cs_font.as_deref() {
                return Some(f);
            }
        }
        // Punctuation and symbols from U+2010 up take the East Asian font
        // in a Japanese document, as Word draws them ("―" full-width, half-width katakana half-width).
        // (not "…" U+2026: Word draws it in the Latin font, as three dots on the baseline: DIDX).
        let east_asian = (c as u32 >= 0x2010 && c != '\u{2026}' && (paragraph_has_east_asian || self.east_asia_hint || !matches!(c as u32, 0x2010..=0x2027)))
            || (self.east_asia_hint && matches!(c as u32, 0x00a0..=0x00ff));
        if east_asian {
            self.east_asian_font.as_deref().or(self.font.as_deref()).or(self.cs_font.as_deref())
        } else {
            self.font.as_deref().or(self.east_asian_font.as_deref()).or(self.cs_font.as_deref())
        }
    }
}

/// How an inline picture is turned, flipped, cut and outlined (`a:xfrm` rot,
/// flipH, flipV, `a:srcRect` and `a:ln` of its `pic:pic`); the default changes nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PictureLook {
    /// Clockwise, in 60 000ths of a degree (0 to 21 600 000).
    pub rotation: i32,
    pub flip_h: bool,
    pub flip_v: bool,
    /// The part of the picture left out at each side, in 100 000ths.
    pub crop: crate::model::Crop,
    /// Outline around the picture (`pic:spPr/a:ln`).
    pub stroke: Option<Stroke>,
    /// 明るさ・コントラスト (`a:lum`): bright and contrast in 100 000ths (-100 000 to 100 000).
    pub luminance: Option<(i32, i32)>,
    /// Recoloured from dark to light between two colours (`a:duotone`; grayscale is black and white).
    pub duotone: Option<(crate::drawing::Rgb, crate::drawing::Rgb)>,
    /// Alternative text (`wp:docPr/@descr`, `@title`).
    pub alt_text: crate::model::AltText,
    /// Whether the picture is marked as decorative (`a1611:decorative/@val="1"`).
    pub decorative: bool,
    /// Object name (`wp:docPr/@name`).
    pub name: String,
    /// The file an `INCLUDEPICTURE` field names, when the picture is that field's result (read again when the fields are updated).
    pub linked_file: Option<String>,
    /// Soft edge blur radius in EMU (`a:effectLst/a:softEdge`).
    pub soft_edge: Option<i64>,
    /// Additional colour and opacity effects on the picture (`a:blip`'s children).
    pub effects: crate::model::BlipEffects,
    /// Preset geometry the picture is cut to (`pic:spPr/a:prstGeom/@prst`), or None for standard rectangle.
    pub geometry: Option<crate::drawing::Geometry>,
}

impl PictureLook {
    /// Whether the picture has no turn, flips, crop or stroke (it is drawn
    /// as a simple inline picture, not needing a turned shape).
    pub fn is_visual_default(&self) -> bool {
        self.rotation == 0
            && !self.flip_h
            && !self.flip_v
            && self.crop == Default::default()
            && self.stroke.is_none()
            && self.luminance.is_none()
            && self.duotone.is_none()
            && self.soft_edge.is_none()
            && self.effects.is_none()
            && self.geometry.as_ref().map_or(true, |g| matches!(g, crate::drawing::Geometry::Rect))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlowRunKind {
    Text(String),
    /// A tab character: moves to the next tab stop.
    Tab,
    /// An absolute position tab (`w:ptab`, ECMA-376 Part 1 §17.3.3.23): moves to the left, centre or right
    /// of the margins (`relative_to_margin`: `true`) or of the paragraph's indents (`false`).
    PositionalTab { align: TabAlign, relative_to_margin: bool, leader: bool },
    /// A line break within the paragraph.
    LineBreak,
    /// An inline picture of this size (turned, flipped and cut as `look` says).
    Picture { media: MediaId, size: EmuSize, look: PictureLook },
    /// An inline or anchored DrawingML / WordprocessingML shape.
    Shape(Box<Shape>),
    /// The number of the page the run is on (a `PAGE` field).
    PageNumber,
    /// Continue on the next page.
    PageBreak,
    /// Continue in the next column (`w:br w:type="column"`; the next page
    /// without columns).
    ColumnBreak,
    /// The document's file name (a `FILENAME` field), with the text Word
    /// last showed for it.
    FileName(String),
    /// The page number of bookmark `bookmark` (a `PAGEREF` field), shown
    /// as `shown` (the field's result, as last updated).
    PageRef { bookmark: String, shown: String },
    /// ルビ (`w:ruby`): `base` in the run's style with `text` over it,
    /// `size` (hundredths of a point) and `raise` (from the base's
    /// baseline to the ruby's, hundredths of a point) as `w:hps` and
    /// `w:hpsRaise` give them, aligned as `align`.
    Ruby { base: String, text: String, size: u32, raise: u32, align: RubyAlign },
    /// A field ibisOffice works out (`SEQ` for a caption's number, `REF`
    /// for a cross-reference), shown as `shown` (its last result).
    /// `instruction` is the field's code, trimmed (`SEQ 図 \* ARABIC`).
    Field { instruction: String, shown: String },
    /// A footnote's (or endnote's) mark in the text (`w:footnoteReference`,
    /// `w:endnoteReference`): note `id` of [`FlowDocument::footnotes`] or
    /// `endnotes`, shown as its number.
    NoteRef { endnote: bool, id: u32 },
    /// The note's number at the start of its own text (`w:footnoteRef`,
    /// `w:endnoteRef`).
    NoteMark,
    /// Where comment `id`'s range starts and ends (`w:commentRangeStart`,
    /// `w:commentRangeEnd`), and its mark (`w:commentReference`): not drawn.
    CommentStart(u32),
    CommentEnd(u32),
    CommentRef(u32),
    /// A checkbox content control (`w14:checkbox` within `w:sdt`).
    CheckBox { checked: bool, checked_char: char, unchecked_char: char },
    /// A drop-down list or combo box content control (`w:dropDownList` or `w:comboBox` within `w:sdt`).
    DropDownList { items: Vec<(String, String)>, selected: usize, combo: bool },
    /// A date picker content control (`w:date` within `w:sdt`).
    DatePicker { full_date: String, date_format: String, text: String },
    /// A rich text content control (`w:sdt` without type-specific controls);
    /// `placeholder`: `text` is the prompt (`w:showingPlcHdr`), not content.
    RichText { text: String, placeholder: bool },
    /// A plain text content control (`w:text` within `w:sdt`); `placeholder` as above.
    PlainText { text: String, placeholder: bool },
    /// A picture content control (`w14:picture` or `w:picture` within `w:sdt`);
    /// `placeholder`: whether it is showing the placeholder image (`w:showingPlcHdr`).
    PictureControl { media: MediaId, size: EmuSize, look: PictureLook, placeholder: bool },
    /// A building block gallery content control (`w:docPartList` or `w:docPartObj` within `w:sdt`).
    BuildingBlockGallery { gallery: String, category: String, text: String, placeholder: bool },
    /// A repeating section content control (`w15:repeatingSection` within `w:sdt`).
    RepeatingSection { section_title: String, items: Vec<String>, selected: usize, do_not_allow_insert_delete: bool },
    /// A group content control (`w:group` within `w:sdt`).
    Group { text: String, placeholder: bool },
    /// A legacy form field (`w:ffData` within `w:fldChar`: `FORMTEXT`, `FORMCHECKBOX`, or `FORMDROPDOWN`).
    FormField(FormFieldData),
    /// A math formula (`m:oMath`, or `m:oMathPara`: `display`, a paragraph of its own and centred).
    Math { math: crate::math::Math, display: bool },
}

/// A legacy form field's data (`w:ffData` within `w:fldChar`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormFieldData {
    pub name: String,
    pub enabled: bool,
    pub calc_on_exit: bool,
    pub kind: FormFieldKind,
}

/// The kind of legacy form field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormFieldKind {
    /// Text input form field (`FORMTEXT` / `w:textInput`).
    TextInput {
        value: String,
        default: String,
        max_length: usize,
    },
    /// Check box form field (`FORMCHECKBOX` / `w:checkBox`).
    CheckBox {
        checked: bool,
        default: bool,
        size_auto: bool,
    },
    /// Drop-down list form field (`FORMDROPDOWN` / `w:ddList`).
    DropDown {
        items: Vec<String>,
        selected: usize,
        default: usize,
    },
}

/// A comment (`comments.xml`), a reply to another (`parent`) and whether
/// it is resolved (`commentsExtended.xml`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comment {
    pub id: u32,
    pub author: String,
    pub initials: String,
    /// `w:date`, as written (`2026-10-01T09:00:00Z`).
    pub date: String,
    pub blocks: Vec<Block>,
    /// The comment this one answers.
    pub parent: Option<u32>,
    pub done: bool,
    /// The `w14:paraId` of its last paragraph, which `commentsExtended.xml`
    /// names it by.
    pub para_id: Option<String>,
}

/// A footnote or endnote: its id (`w:id`) and its text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Note {
    pub id: u32,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowRun {
    pub kind: FlowRunKind,
    /// The formatting the run is drawn with: styles and direct formatting
    /// folded together (see [`FlowParagraph::resolve`]).
    pub style: FlowRunStyle,
    /// The run's direct formatting (its `w:rPr`, without the character
    /// style).
    pub format: RunFormat,
    /// The character style (`w:rStyle`).
    pub char_style: Option<String>,
    /// The link the run is part of (`w:hyperlink`).
    pub link: Option<Link>,
    /// A tracked change the run is (`w:ins`, `w:del`, `w:rPrChange`).
    pub revision: Option<Revision>,
    /// 個別のフィールドコード表示（Word の「フィールドコードの表示/非表示」Shift+F9 / ⇧F9）。
    pub show_field_code: bool,
    /// フィールドのロック（Word の「フィールドのロック」Ctrl+F11 / ⌘F11、w:fldLock="1"）。
    /// ロック中はフィールド更新（F9）による再計算をスキップする。
    pub field_locked: bool,
    /// What a content control run (`w:sdt`) says about itself: its title, tag
    /// and locks (`None` when it has none).
    pub control: Option<Box<ControlInfo>>,
}

/// A content control's title (`w:alias`), tag (`w:tag`) and lock (`w:lock`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlInfo {
    pub title: String,
    pub tag: String,
    pub lock: ControlLock,
}

/// `w:lock`: whether the control can be deleted and whether its content can
/// be edited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlLock {
    #[default]
    None,
    /// `sdtLocked`: the control cannot be deleted; its content can be edited.
    Control,
    /// `contentLocked`: the content cannot be edited; the control can be deleted.
    Content,
    /// `sdtContentLocked`: neither.
    Both,
}

impl ControlLock {
    pub fn keeps_control(self) -> bool {
        matches!(self, Self::Control | Self::Both)
    }
    pub fn keeps_content(self) -> bool {
        matches!(self, Self::Content | Self::Both)
    }
    /// The `w:lock/@w:val`.
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Control => Some("sdtLocked"),
            Self::Content => Some("contentLocked"),
            Self::Both => Some("sdtContentLocked"),
        }
    }
    pub fn from_name(name: &str) -> Self {
        match name {
            "sdtLocked" => Self::Control,
            "contentLocked" => Self::Content,
            "sdtContentLocked" => Self::Both,
            _ => Self::None,
        }
    }
}

/// A tracked change: who made it, when, and what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revision {
    pub kind: RevisionKind,
    pub author: String,
    /// `w:date`, as written.
    pub date: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RevisionKind {
    /// Text inserted (`w:ins`).
    Insert,
    /// Text deleted, still in the document until accepted (`w:del`).
    Delete,
    /// Formatting changed; this was the run's own formatting before
    /// (`w:rPrChange`).
    Format(RunFormat),
    /// Text moved from this location (`w:moveFrom`).
    MoveFrom,
    /// Text moved to this location (`w:moveTo`).
    MoveTo,
}

impl RevisionKind {
    /// Whether this revision acts as an insertion (new text appearing).
    pub fn is_insert(&self) -> bool {
        matches!(self, Self::Insert | Self::MoveTo)
    }

    /// Whether this revision acts as a deletion (text being removed).
    pub fn is_delete(&self) -> bool {
        matches!(self, Self::Delete | Self::MoveFrom)
    }

    /// Whether this revision is part of a move operation (`w:moveFrom` or `w:moveTo`).
    pub fn is_move(&self) -> bool {
        matches!(self, Self::MoveFrom | Self::MoveTo)
    }
}

/// Where a link goes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Link {
    /// An address outside the document (a relationship's target).
    Url(String),
    /// A bookmark of the document (`w:anchor`).
    Anchor(String),
}

impl FlowRun {
    /// A run with no direct formatting, drawn with `style` until the
    /// paragraph is resolved again.
    pub fn new(kind: FlowRunKind, style: FlowRunStyle) -> Self {
        Self { kind, style, format: RunFormat::default(), char_style: None, link: None, revision: None, show_field_code: false, field_locked: false, control: None }
    }

    /// The characters the run takes in a paragraph's positions: a text
    /// run's characters, one for anything else.
    pub fn len(&self) -> usize {
        match &self.kind {
            FlowRunKind::Text(t) => t.chars().count(),
            // A drop-down is one position (as Word and the saving count it), whatever item it shows.
            FlowRunKind::DropDownList { .. } => 1,
            FlowRunKind::DatePicker { text, .. } => text.chars().count(),
            FlowRunKind::RichText { text, .. }
            | FlowRunKind::PlainText { text, .. }
            | FlowRunKind::BuildingBlockGallery { text, .. }
            | FlowRunKind::Group { text, .. } => text.chars().count(),
            FlowRunKind::RepeatingSection { items, selected, .. } => {
                items.get(*selected).map_or(0, |s| s.chars().count())
            }
            FlowRunKind::FormField(f) => match &f.kind {
                FormFieldKind::TextInput { value, default, .. } => {
                    let c = value.chars().count();
                    if c == 0 {
                        if !default.is_empty() {
                            default.chars().count()
                        } else {
                            5
                        }
                    } else {
                        c
                    }
                }
                FormFieldKind::CheckBox { .. } => 1,
                FormFieldKind::DropDown { items, selected, .. } => {
                    items.get(*selected).map_or(0, |s| s.chars().count())
                }
            },
            _ => 1,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether this run is a legacy form field (`w:ffData`).
    pub fn is_form_field(&self) -> bool {
        matches!(self.kind, FlowRunKind::FormField(_))
    }

    /// The legacy form field data, if applicable.
    pub fn form_field(&self) -> Option<&FormFieldData> {
        match &self.kind {
            FlowRunKind::FormField(f) => Some(f),
            _ => None,
        }
    }

    /// The legacy form field data as mutable, if applicable.
    pub fn form_field_mut(&mut self) -> Option<&mut FormFieldData> {
        match &mut self.kind {
            FlowRunKind::FormField(f) => Some(f),
            _ => None,
        }
    }

    /// Whether this run is a content control (`w:sdt`).
    pub fn is_content_control(&self) -> bool {
        matches!(self.kind, FlowRunKind::CheckBox { .. } | FlowRunKind::DropDownList { .. } | FlowRunKind::DatePicker { .. } | FlowRunKind::RichText { .. } | FlowRunKind::PlainText { .. } | FlowRunKind::PictureControl { .. } | FlowRunKind::BuildingBlockGallery { .. } | FlowRunKind::RepeatingSection { .. } | FlowRunKind::Group { .. })
    }

    /// Whether this run is a group content control (`w:group`).
    pub fn is_group(&self) -> bool {
        matches!(self.kind, FlowRunKind::Group { .. })
    }

    /// The group content control properties (text, placeholder), if applicable.
    pub fn group(&self) -> Option<(&str, bool)> {
        match &self.kind {
            FlowRunKind::Group { text, placeholder } => Some((text.as_str(), *placeholder)),
            _ => None,
        }
    }

    /// Whether this run is a repeating section content control.
    pub fn is_repeating_section(&self) -> bool {
        matches!(self.kind, FlowRunKind::RepeatingSection { .. })
    }

    /// The repeating section properties (section_title, items, selected, do_not_allow_insert_delete), if applicable.
    pub fn repeating_section(&self) -> Option<(&str, &[String], usize, bool)> {
        match &self.kind {
            FlowRunKind::RepeatingSection { section_title, items, selected, do_not_allow_insert_delete } => {
                Some((section_title.as_str(), items.as_slice(), *selected, *do_not_allow_insert_delete))
            }
            _ => None,
        }
    }

    /// Whether this run is a building block gallery content control.
    pub fn is_building_block_gallery(&self) -> bool {
        matches!(self.kind, FlowRunKind::BuildingBlockGallery { .. })
    }

    /// The building block gallery properties (gallery, category, text, placeholder), if applicable.
    pub fn building_block_gallery(&self) -> Option<(&str, &str, &str, bool)> {
        match &self.kind {
            FlowRunKind::BuildingBlockGallery { gallery, category, text, placeholder } => {
                Some((gallery.as_str(), category.as_str(), text.as_str(), *placeholder))
            }
            _ => None,
        }
    }

    /// Whether this run is a picture (`Picture` or `PictureControl`).
    pub fn is_picture(&self) -> bool {
        matches!(self.kind, FlowRunKind::Picture { .. } | FlowRunKind::PictureControl { .. })
    }

    /// The picture's media id, size and look, if it is a picture run.
    pub fn picture(&self) -> Option<(MediaId, EmuSize, &PictureLook)> {
        match &self.kind {
            FlowRunKind::Picture { media, size, look } | FlowRunKind::PictureControl { media, size, look, .. } => Some((*media, *size, look)),
            _ => None,
        }
    }

    /// Whether this run is a field (`PageNumber`, `FileName`, `PageRef`, `Field`).
    pub fn is_field(&self) -> bool {
        matches!(self.kind, FlowRunKind::PageNumber | FlowRunKind::FileName(_) | FlowRunKind::PageRef { .. } | FlowRunKind::Field { .. })
    }

    /// Whether this field is locked from recalculation.
    pub fn is_field_locked(&self) -> bool {
        self.is_field() && self.field_locked
    }

    /// The field code instruction of this run, if it is a field.
    pub fn field_code(&self) -> Option<String> {
        match &self.kind {
            FlowRunKind::PageNumber => Some("PAGE".to_string()),
            FlowRunKind::FileName(_) => Some("FILENAME".to_string()),
            FlowRunKind::PageRef { bookmark, .. } => Some(format!("PAGEREF {bookmark}")),
            FlowRunKind::Field { instruction, .. } => Some(instruction.trim().to_string()),
            _ => None,
        }
    }

    /// The formatted field code with braces as shown in Word (`{ PAGE }`).
    pub fn display_field_code(&self) -> Option<String> {
        self.field_code().map(|code| format!("{{ {code} }}"))
    }

    /// Whether this field should show its field code rather than its cached result.
    /// In Word, this is evaluated as the XOR of the document-wide toggle (Alt+F9)
    /// and the per-field toggle (Shift+F9).
    pub fn is_field_code_shown(&self, doc_global: bool) -> bool {
        self.is_field() && (doc_global ^ self.show_field_code)
    }
}

impl Default for FlowRun {
    fn default() -> Self {
        Self {
            kind: FlowRunKind::Text(String::new()),
            style: FlowRunStyle::default(),
            format: RunFormat::default(),
            char_style: None,
            link: None,
            revision: None,
            show_field_code: false,
            field_locked: false, control: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabAlign {
    #[default]
    Left,
    Center,
    Right,
    Decimal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TabStop {
    /// From the paragraph's left edge of the text area (the page margin).
    pub position: Emu,
    pub align: TabAlign,
    /// Dots or a line leading to the stop.
    pub leader: bool,
}

/// Word 2010's text effects of a run (the `w14:` elements of its `w:rPr`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextEffects {
    /// An outline (`w14:textOutline`): its width (EMU) and colour.
    pub outline: Option<(Emu, Rgb)>,
    /// A glow (`w14:glow`): its radius (EMU), colour and opacity (0 to 255).
    pub glow: Option<(Emu, Rgb, u8)>,
    /// A shadow (`w14:shadow`).
    pub shadow: Option<TextShadow>,
    /// A gradient fill (`w14:textFill/w14:gradFill`): stops (position in 100 000ths, colour) and the angle in
    /// 60 000ths of a degree (0: left to right).
    pub gradient: Option<(Vec<(u32, Rgb)>, i32)>,
}

/// A text shadow (`w14:shadow`): blur and distance (EMU), direction (60 000ths of a degree), colour, opacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextShadow {
    pub blur: Emu,
    pub distance: Emu,
    pub direction: i32,
    pub color: Rgb,
    pub alpha: u8,
}

/// A two-color shading pattern (`ST_Shd` stripes and crosses).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShadingPattern {
    HorzStripe,
    VertStripe,
    ReverseDiagStripe,
    DiagStripe,
    HorzCross,
    DiagCross,
    ThinHorzStripe,
    ThinVertStripe,
    ThinReverseDiagStripe,
    ThinDiagStripe,
    ThinHorzCross,
    ThinDiagCross,
}

impl ShadingPattern {
    pub fn from_ooxml(v: &str) -> Option<Self> {
        match v {
            "horzStripe" => Some(Self::HorzStripe),
            "vertStripe" => Some(Self::VertStripe),
            "reverseDiagStripe" => Some(Self::ReverseDiagStripe),
            "diagStripe" => Some(Self::DiagStripe),
            "horzCross" => Some(Self::HorzCross),
            "diagCross" => Some(Self::DiagCross),
            "thinHorzStripe" => Some(Self::ThinHorzStripe),
            "thinVertStripe" => Some(Self::ThinVertStripe),
            "thinReverseDiagStripe" => Some(Self::ThinReverseDiagStripe),
            "thinDiagStripe" => Some(Self::ThinDiagStripe),
            "thinHorzCross" => Some(Self::ThinHorzCross),
            "thinDiagCross" => Some(Self::ThinDiagCross),
            _ => None,
        }
    }
}

/// How a border is drawn (`ST_Border`). The art borders (`apples`, …),
/// which Word draws as plain lines around paragraphs and cells, are
/// `Single`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BorderStyle {
    #[default]
    Single,
    /// Drawn as `Single`; heavier where borders meet.
    Thick,
    Double,
    Triple,
    Dotted,
    Dashed,
    DotDash,
    DotDotDash,
    DashSmallGap,
    DashDotStroked,
    ThinThickSmallGap,
    ThickThinSmallGap,
    ThinThickThinSmallGap,
    ThinThickMediumGap,
    ThickThinMediumGap,
    ThinThickThinMediumGap,
    ThinThickLargeGap,
    ThickThinLargeGap,
    ThinThickThinLargeGap,
    Wave,
    DoubleWave,
    ThreeDEmboss,
    ThreeDEngrave,
    Outset,
    Inset,
}

macro_rules! define_art_borders {
    ($(($variant:ident, $name:literal)),* $(,)?) => {
        /// An ECMA-376 picture / art border style (`ST_Border`).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum ArtBorder {
            $($variant),*
        }

        impl ArtBorder {
            /// Every art border, in ST_Border's order.
            pub const ALL: &'static [ArtBorder] = &[$(Self::$variant),*];

            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Self::$variant),)*
                    _ => None,
                }
            }

            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }
        }
    };
}

define_art_borders! {
    (Apples, "apples"),
    (ArchedScallops, "archedScallops"),
    (BabyPacifier, "babyPacifier"),
    (BabyRattle, "babyRattle"),
    (Balloons3Colors, "balloons3Colors"),
    (BalloonsHotAir, "balloonsHotAir"),
    (BasicBlackDashes, "basicBlackDashes"),
    (BasicBlackDots, "basicBlackDots"),
    (BasicBlackSquares, "basicBlackSquares"),
    (BasicThinLines, "basicThinLines"),
    (BasicWhiteDashes, "basicWhiteDashes"),
    (BasicWhiteDots, "basicWhiteDots"),
    (BasicWhiteSquares, "basicWhiteSquares"),
    (BasicWideInline, "basicWideInline"),
    (BasicWideMidline, "basicWideMidline"),
    (BasicWideOutline, "basicWideOutline"),
    (Bats, "bats"),
    (Birds, "birds"),
    (BirdsFlight, "birdsFlight"),
    (Cabins, "cabins"),
    (CakeSlice, "cakeSlice"),
    (CandyCorn, "candyCorn"),
    (CelticKnotwork, "celticKnotwork"),
    (CertificateBanner, "certificateBanner"),
    (ChainLink, "chainLink"),
    (ChampagneBottle, "champagneBottle"),
    (CheckedBarBlack, "checkedBarBlack"),
    (CheckedBarColor, "checkedBarColor"),
    (Checkered, "checkered"),
    (ChristmasTree, "christmasTree"),
    (CirclesLines, "circlesLines"),
    (CirclesRectangles, "circlesRectangles"),
    (ClassicalWave, "classicalWave"),
    (Clocks, "clocks"),
    (Compass, "compass"),
    (Confetti, "confetti"),
    (ConfettiGrays, "confettiGrays"),
    (ConfettiOutline, "confettiOutline"),
    (ConfettiStreamers, "confettiStreamers"),
    (ConfettiWhite, "confettiWhite"),
    (CornerTriangles, "cornerTriangles"),
    (CouponCutoutDashes, "couponCutoutDashes"),
    (CouponCutoutDots, "couponCutoutDots"),
    (CrazyMaze, "crazyMaze"),
    (CreaturesButterfly, "creaturesButterfly"),
    (CreaturesFish, "creaturesFish"),
    (CreaturesInsects, "creaturesInsects"),
    (CreaturesLadyBug, "creaturesLadyBug"),
    (CrossStitch, "crossStitch"),
    (Cup, "cup"),
    (DecoArch, "decoArch"),
    (DecoArchColor, "decoArchColor"),
    (DecoBlocks, "decoBlocks"),
    (DiamondsGray, "diamondsGray"),
    (DoubleD, "doubleD"),
    (DoubleDiamonds, "doubleDiamonds"),
    (Earth1, "earth1"),
    (Earth2, "earth2"),
    (EclipsingSquares1, "eclipsingSquares1"),
    (EclipsingSquares2, "eclipsingSquares2"),
    (EggsBlack, "eggsBlack"),
    (Fans, "fans"),
    (Film, "film"),
    (Firecrackers, "firecrackers"),
    (FlowersBlockPrint, "flowersBlockPrint"),
    (FlowersDaisies, "flowersDaisies"),
    (FlowersModern1, "flowersModern1"),
    (FlowersModern2, "flowersModern2"),
    (FlowersPansy, "flowersPansy"),
    (FlowersRedRose, "flowersRedRose"),
    (FlowersRoses, "flowersRoses"),
    (FlowersTeacup, "flowersTeacup"),
    (FlowersTiny, "flowersTiny"),
    (Gems, "gems"),
    (GingerbreadMan, "gingerbreadMan"),
    (Gradient, "gradient"),
    (Handmade1, "handmade1"),
    (Handmade2, "handmade2"),
    (HeartBalloon, "heartBalloon"),
    (HeartGray, "heartGray"),
    (Hearts, "hearts"),
    (HeebieJeebies, "heebieJeebies"),
    (Holly, "holly"),
    (HouseFunky, "houseFunky"),
    (Hypnotic, "hypnotic"),
    (IceCreamCones, "iceCreamCones"),
    (LightBulb, "lightBulb"),
    (Lightning1, "lightning1"),
    (Lightning2, "lightning2"),
    (MapPins, "mapPins"),
    (MapleLeaf, "mapleLeaf"),
    (MapleMuffins, "mapleMuffins"),
    (Marquee, "marquee"),
    (MarqueeToothed, "marqueeToothed"),
    (Moons, "moons"),
    (Mosaic, "mosaic"),
    (MusicNotes, "musicNotes"),
    (Northwest, "northwest"),
    (Ovals, "ovals"),
    (Packages, "packages"),
    (PalmsBlack, "palmsBlack"),
    (PalmsColor, "palmsColor"),
    (PaperClips, "paperClips"),
    (Papyrus, "papyrus"),
    (PartyFavor, "partyFavor"),
    (PartyGlass, "partyGlass"),
    (Pencils, "pencils"),
    (People, "people"),
    (PeopleWaving, "peopleWaving"),
    (PeopleHats, "peopleHats"),
    (Poinsettias, "poinsettias"),
    (PostageStamp, "postageStamp"),
    (Pumpkin1, "pumpkin1"),
    (PushPinNote2, "pushPinNote2"),
    (PushPinNote1, "pushPinNote1"),
    (Pyramids, "pyramids"),
    (PyramidsAbove, "pyramidsAbove"),
    (Quadrants, "quadrants"),
    (Rings, "rings"),
    (Safari, "safari"),
    (Sawtooth, "sawtooth"),
    (SawtoothGray, "sawtoothGray"),
    (ScaredCat, "scaredCat"),
    (Seattle, "seattle"),
    (ShadowedSquares, "shadowedSquares"),
    (SharksTeeth, "sharksTeeth"),
    (ShorebirdTracks, "shorebirdTracks"),
    (Skyrocket, "skyrocket"),
    (SnowflakeFancy, "snowflakeFancy"),
    (Snowflakes, "snowflakes"),
    (Sombrero, "sombrero"),
    (Southwest, "southwest"),
    (Stars, "stars"),
    (StarsTop, "starsTop"),
    (Stars3d, "stars3d"),
    (StarsBlack, "starsBlack"),
    (StarsShadowed, "starsShadowed"),
    (Sun, "sun"),
    (Swirligig, "swirligig"),
    (TornPaper, "tornPaper"),
    (TornPaperBlack, "tornPaperBlack"),
    (Trees, "trees"),
    (TriangleParty, "triangleParty"),
    (Triangles, "triangles"),
    (Tribal1, "tribal1"),
    (Tribal2, "tribal2"),
    (Tribal3, "tribal3"),
    (Tribal4, "tribal4"),
    (Tribal5, "tribal5"),
    (Tribal6, "tribal6"),
    (Triangle1, "triangle1"),
    (Triangle2, "triangle2"),
    (TriangleCircle1, "triangleCircle1"),
    (TriangleCircle2, "triangleCircle2"),
    (Shapes1, "shapes1"),
    (Shapes2, "shapes2"),
    (TwistedLines1, "twistedLines1"),
    (TwistedLines2, "twistedLines2"),
    (Vine, "vine"),
    (Waveline, "waveline"),
    (WeavingAngles, "weavingAngles"),
    (WeavingBraid, "weavingBraid"),
    (WeavingRibbon, "weavingRibbon"),
    (WeavingStrips, "weavingStrips"),
    (WhiteFlowers, "whiteFlowers"),
    (Woodwork, "woodwork"),
    (XIllusions, "xIllusions"),
    (ZanyTriangles, "zanyTriangles"),
    (ZigZag, "zigZag"),
    (ZigZagStitch, "zigZagStitch"),
}

/// A border line (of a paragraph or a table cell).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlowBorder {
    /// The line's width (`w:sz`), EMU.
    pub width: Emu,
    pub color: Rgb,
    pub style: BorderStyle,
    /// The distance from the text (`w:space`, points), EMU.
    pub space: Emu,
    /// Picture border (art border) style for page borders (`w:pgBorders`).
    pub art: Option<ArtBorder>,
}

impl FlowBorder {
    /// The bands across the border from its outer side: widths in EMU and
    /// whether each is drawn (a line) or a gap. Measured on Word's PDF: the
    /// "thin" lines of the small and large gaps are 0.75 pt.
    pub fn bands(&self) -> Vec<(Emu, bool)> {
        const PT: Emu = 12_700;
        let w = self.width;
        let t = PT * 3 / 4;
        use BorderStyle::*;
        let (lines, gaps): (Vec<Emu>, Vec<Emu>) = match self.style {
            Double => (vec![w, w], vec![w]),
            Triple => (vec![w, w, w], vec![w, w]),
            ThinThickSmallGap => (vec![w, t], vec![t]),
            ThickThinSmallGap => (vec![t, w], vec![t]),
            ThinThickThinSmallGap => (vec![t, w, t], vec![t, t]),
            ThinThickMediumGap => (vec![w, w / 2], vec![w / 2]),
            ThickThinMediumGap => (vec![w / 2, w], vec![w / 2]),
            ThinThickThinMediumGap => (vec![w / 2, w, w / 2], vec![w / 2, w / 2]),
            ThinThickLargeGap => (vec![2 * t, t], vec![w]),
            ThickThinLargeGap => (vec![t, 2 * t], vec![w]),
            ThinThickThinLargeGap => (vec![t, 2 * t, t], vec![w, w]),
            // Word: a wave is 2.5 pt high and a double wave two of them, one above the other (a 0.75 pt line).
            DoubleWave => (vec![PT * 11 / 4, PT * 11 / 4], vec![]),
            Wave => (vec![PT * 13 / 4], vec![]),
            // Word: a band 3.5 pt across whatever the width, of short slanted lines.
            DashDotStroked => (vec![PT * 7 / 2], vec![]),
            ThreeDEmboss | ThreeDEngrave => (vec![w + t], vec![]),
            _ => (vec![w], vec![]),
        };
        let mut out = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            out.push((*l, true));
            if let Some(g) = gaps.get(i) {
                out.push((*g, false));
            }
        }
        out
    }

    /// The whole width across the border.
    pub fn total(&self) -> Emu {
        self.bands().iter().map(|b| b.0).sum()
    }

    /// The dashes along the border, drawn and gap lengths repeating (EMU);
    /// `None` for a solid line. Measured on Word's PDF, in line widths.
    pub fn dashes(&self) -> Option<Vec<Emu>> {
        let w = self.width.max(3_175);
        use BorderStyle::*;
        Some(match self.style {
            Dotted => vec![w, w],
            Dashed => vec![3 * w, w],
            DashSmallGap => vec![4 * w, w],
            DotDash => vec![27 * w / 4, 3 * w, 3 * w, 3 * w],
            DotDotDash => vec![23 * w / 4, 2 * w, 2 * w, 2 * w, 2 * w, 2 * w],
            _ => return None,
        })
    }
}

/// Where a drop cap is placed (`w:framePr/@w:dropCap`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum DropCapLocation {
    #[default]
    None,
    Drop,
    Margin,
}

/// Text frame drop cap properties (`w:framePr/@w:dropCap`, `@w:lines`, `@w:hSpace`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropCap {
    pub location: DropCapLocation,
    /// Vertical height in lines (default 3, 1..=10).
    pub lines: u32,
    /// Distance from surrounding text in EMU (default 0).
    pub space: Emu,
}

impl Default for DropCap {
    fn default() -> Self {
        Self {
            location: DropCapLocation::None,
            lines: 3,
            space: 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlowParagraph {
    /// Names the paragraph in the document (unique; see
    /// [`FlowDocument::new_id`]). A save finds the paragraph's element by it.
    pub id: u32,
    /// A drop cap on the paragraph's initial letter (`w:framePr w:dropCap="drop|margin"`).
    pub drop_cap: Option<DropCap>,
    /// A text frame (`w:framePr` without `w:dropCap`), such as a page-number frame in a footer.
    pub framed: bool,
    /// The paragraph style (`w:pStyle`); `None` for the default one.
    pub style: Option<String>,
    /// The style of the table the paragraph is in, which it inherits from.
    pub table_style: Option<String>,
    /// The parts of that table its cell is in (its style's conditional
    /// formatting, in order).
    pub table_regions: Vec<TableRegion>,
    /// Direct paragraph formatting (`w:pPr`).
    pub direct: ParaFormat,
    /// Direct formatting of the paragraph mark (`w:pPr/w:rPr`).
    pub mark_format: RunFormat,
    pub runs: Vec<FlowRun>,
    pub align: FlowAlign,
    pub text_alignment: FontAlign,
    /// Indents from the text area's edges.
    pub left: Emu,
    pub right: Emu,
    /// Where the first line starts, relative to `left` (negative for a
    /// hanging indent).
    pub first_line: Emu,
    /// Indents in character units (1/100 of a character, e.g. 100 = 1 char).
    /// `w:ind/@w:leftChars`, `w:ind/@w:rightChars`, `w:ind/@w:firstLineChars` (negative for `w:hangingChars`).
    pub left_chars: Option<i32>,
    pub right_chars: Option<i32>,
    pub first_line_chars: Option<i32>,
    pub space_before: Emu,
    pub space_after: Emu,
    pub line: LineHeight,
    /// Lines snap to the page's line grid.
    pub snap_to_grid: bool,
    /// A quarter em of space between Japanese and Latin letters
    /// (`w:autoSpaceDE`), and between Japanese and digits (`w:autoSpaceDN`).
    pub auto_space_letters: bool,
    pub auto_space_digits: bool,
    pub page_break_before: bool,
    /// Its lines get no line numbers (`w:suppressLineNumbers`).
    pub suppress_line_numbers: bool,
    /// 段落単位で自動ハイフネーションを行わない (`w:suppressAutoHyphens`).
    pub suppress_auto_hyphens: bool,
    /// 同じスタイルの段落間にスペースを追加しない (`w:contextualSpacing`):
    /// the space before and after is not added between paragraphs of the
    /// same style.
    pub contextual_spacing: bool,
    /// The space before or after is Word's automatic one (`w:beforeAutospacing`, `w:afterAutospacing`; 14 pt, none at a
    /// cell's start or end).
    pub auto_before: bool,
    pub auto_after: bool,
    /// widow/orphan control (`w:widowControl`, on unless the file turns it
    /// off): a paragraph's page does not end with its first line alone or
    /// start with its last line alone.
    pub widow_control: bool,
    pub keep_with_next: bool,
    pub keep_lines: bool,
    pub kinsoku: bool,
    pub word_wrap: bool,
    pub overflow_punct: bool,
    pub bidi: bool,
    /// 見出し段落が折りたたまれているか (`w15:collapsed`, MS-DOCX 2.6.27 `CT_OnOff`).
    pub collapsed: bool,
    pub tabs: Vec<TabStop>,
    pub shading: Option<Rgb>,
    pub shading_fill: Option<Rgb>,
    pub shading_pattern: Option<ShadingPattern>,
    pub shading_color: Option<Rgb>,
    /// Borders: top, right, bottom, left.
    pub borders: [Option<FlowBorder>; 4],
    /// The style of the paragraph mark, which sets an empty paragraph's
    /// height.
    pub mark: FlowRunStyle,
    /// The list item the paragraph is (resolved from its style and its own
    /// `w:numPr`): the list and the level.
    pub list: Option<ListRef>,
    /// A section ends with the paragraph (`w:pPr/w:sectPr`): where the next
    /// starts. The sections' own page setups are not read.
    pub section_break: SectionBreak,
    /// The setup of the section the paragraph ends (with a section break).
    pub section: Option<std::sync::Arc<SectionSetup>>,
    /// Bookmarks starting in the paragraph (their names), such as the
    /// `_Toc…` a table of contents links to.
    pub bookmarks: Vec<String>,
    /// The bookmarks that end in this paragraph (`w:bookmarkEnd`), by name: a bookmark that spans pages has its end
    /// here (an index entry's page range). Read, not edited.
    pub bookmark_ends: Vec<String>,
    /// A field spanning paragraphs starts at this paragraph's start (its
    /// instruction, such as `TOC \o "1-3" \h \z \u`), or ends at its end.
    pub field_start: Option<String>,
    pub field_end: bool,
    /// The style of the list item's number or bullet.
    pub list_style: FlowRunStyle,
    /// Subdocument reference (`<w:subDoc r:id="..."/>`, ECMA-376 Part 1 §17.17.1.1, DOCX-TOC-016).
    pub subdoc: Option<SubdocumentInfo>,
}

/// Subdocument reference attached to a paragraph (`<w:subDoc r:id="..."/>`, ECMA-376 Part 1 §17.17.1.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubdocumentInfo {
    /// Target file path or URI of the subdocument (`Target` of the `subDocument` relationship).
    pub target: String,
    /// Display title / heading of the subdocument.
    pub title: String,
    /// Whether the subdocument is currently expanded inline in master document view.
    pub expanded: bool,
    /// Whether the subdocument is locked against editing (`locked`).
    pub locked: bool,
    /// Inline paragraphs/lines stored when created or collapsed from the master document.
    pub content_paragraphs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellAnchor {
    #[default]
    Top,
    Center,
    Bottom,
}

/// How a cell takes part in a vertical merge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VerticalMerge {
    #[default]
    None,
    /// The first cell of a merged column of cells.
    Start,
    /// Covered by the cell above.
    Continue,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlowCell {
    /// Names the cell, as [`FlowParagraph::id`].
    pub id: u32,
    /// The cell's own borders (`w:tcBorders`), over the table's: top,
    /// right, bottom, left; `Some(None)` for none.
    pub own_borders: [Option<Option<FlowBorder>>; 4],
    /// Grid columns spanned.
    pub span: u32,
    pub merge: VerticalMerge,
    pub blocks: Vec<Block>,
    /// Borders: top, right, bottom, left.
    pub borders: [Option<FlowBorder>; 4],
    pub shading: Option<Rgb>,
    pub shading_fill: Option<Rgb>,
    pub shading_pattern: Option<ShadingPattern>,
    pub shading_color: Option<Rgb>,
    pub anchor: CellAnchor,
    /// Margins: top, right, bottom, left.
    pub margins: [Emu; 4],
    /// The diagonal lines: top-left to bottom-right, top-right to bottom-left.
    pub diagonals: [Option<FlowBorder>; 2],
    /// The fill its table's style gives it (under its own `shading`).
    pub style_fill: Option<Rgb>,
    /// The direction of its text (`w:textDirection`).
    pub direction: CellDirection,
    /// Whether cell text should not wrap (`w:noWrap`).
    pub no_wrap: bool,
}

/// A cell's text direction (`w:textDirection`, `ST_TextDirection`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CellDirection {
    /// Across, lines down (`lrTb`).
    #[default]
    LrTb,
    /// Down, lines right to left; Japanese upright, Latin turned
    /// clockwise (`tbRl`, `tbRlV`).
    TbRl,
    /// Up, lines left to right; all turned anticlockwise (`btLr`).
    BtLr,
    /// Across, Japanese turned anticlockwise (`lrTbV`).
    LrTbV,
    /// Down, lines left to right (`tbLrV`).
    TbLrV,
}

impl CellDirection {
    pub fn from_name(name: &str) -> Self {
        match name {
            "tbRl" | "tbRlV" | "rl" | "tbV" => Self::TbRl,
            "btLr" | "lr" => Self::BtLr,
            "lrTbV" | "tbLr" => Self::LrTbV,
            "tbLrV" => Self::TbLrV,
            _ => Self::LrTb,
        }
    }

    /// Its lines run down or up.
    pub fn vertical(self) -> bool {
        matches!(self, Self::TbRl | Self::BtLr | Self::TbLrV)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowHeight {
    #[default]
    Auto,
    AtLeast(Emu),
    Exact(Emu),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlowRow {
    /// Names the row, as [`FlowParagraph::id`].
    pub id: u32,
    pub height: RowHeight,
    /// Repeated at the top of each page the table continues on.
    pub header: bool,
    /// Kept on one page.
    pub cant_split: bool,
    pub cells: Vec<FlowCell>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlowTable {
    /// Names the table in the document, as [`FlowParagraph::id`].
    pub id: u32,
    /// The table's borders (its style's and `w:tblBorders`): top, right,
    /// bottom, left, inside horizontal, inside vertical.
    pub borders: [Option<Option<FlowBorder>>; 6],
    /// Widths of the grid columns.
    pub columns: Vec<Emu>,
    /// From the text area's left edge to the table's left edge (as Word
    /// 2013 and later read `w:tblInd`).
    pub indent: Emu,
    pub align: FlowAlign,
    pub rows: Vec<FlowRow>,
    /// Its style, when that formats parts of it (`w:tblStylePr`), and the
    /// options (`w:tblLook`) the parts are found by.
    pub style: Option<TableStyleRef>,
    /// 浮動の表 (`w:tblpPr`): placed on its page, the text round it.
    pub floating: Option<TableFloat>,
    /// Laid out from its content and preferred widths (Word's 自動調整,
    /// `w:tblLayout` autofit) rather than by `columns`, when the file's
    /// grid does not say the widths (cells or table without a width, in
    /// percent, or disagreeing with the grid).
    pub autofit: Option<AutoFit>,
    /// An automatically laid out table of no set width (`w:tblLayout` autofit, `w:tblW` auto): wider than the text, its
    /// columns are cut down in proportion (to the text's width), not less than their content's least width.
    pub fit_to_page: bool,
    /// セル間隔 (`w:tblCellSpacing`).
    pub cell_spacing: Option<Emu>,
}

/// A preferred width (`w:tblW`, `w:tcW`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreferredWidth {
    #[default]
    Auto,
    Fixed(Emu),
    /// Fiftieths of a percent (5000: all of it) of the table's width (a
    /// cell's) or of the text's (the table's).
    Percent(u32),
}

/// The preferred widths an automatically fitted table is laid out from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoFit {
    pub width: PreferredWidth,
    /// Each row's cells' widths, as `rows`.
    pub cells: Vec<Vec<PreferredWidth>>,
}

impl FlowTable {
    /// The parts of the table its style formats (the heading row, the
    /// bands…) found again: each cell's fill from its style and its
    /// paragraphs' formatting (after the options or the rows changed).
    pub fn apply_style(&mut self, styles: &FlowStyles) {
        let Some(style) = self.style.clone() else { return };
        let chain = styles.chain(&style.id);
        let defines = |r: TableRegion| chain.iter().any(|s| s.conditional.iter().any(|c| c.region == r));
        let look = style.look;
        let bands = TableLook {
            first_row: look.first_row && defines(TableRegion::FirstRow),
            last_row: look.last_row && defines(TableRegion::LastRow),
            first_column: look.first_column && defines(TableRegion::FirstCol),
            last_column: look.last_column && defines(TableRegion::LastCol),
            ..look
        };
        let any = chain.iter().any(|s| !s.conditional.is_empty());
        let (rows, cols) = (self.rows.len(), self.columns.len());
        for (r, row) in self.rows.iter_mut().enumerate() {
            let mut col = 0usize;
            for cell in &mut row.cells {
                let span = cell.span.max(1) as usize;
                let regions = if any {
                    let last = (col + span).min(cols).max(1) - 1;
                    let at = if last != col && look.last_column && last + 1 == cols { last } else { col };
                    TableRegion::of_cell(r, at, rows, cols, look, bands, style.row_band as usize, style.col_band as usize)
                } else {
                    Vec::new()
                };
                col += span;
                cell.style_fill = None;
                for region in &regions {
                    for s in &chain {
                        for c in s.conditional.iter().filter(|c| c.region == *region) {
                            if c.fill.is_some() {
                                cell.style_fill = c.fill;
                            }
                        }
                    }
                }
                for block in &mut cell.blocks {
                    if let Block::Paragraph(p) = block
                        && p.table_regions != regions
                    {
                        p.table_regions = regions.clone();
                        p.resolve(styles);
                    }
                }
            }
        }
    }

    /// Each cell's borders from the table's at its edges and its own (after
    /// rows or cells were added or removed).
    pub fn resolve_borders(&mut self) {
        let b = self.borders;
        let count = self.rows.len();
        for (r, row) in self.rows.iter_mut().enumerate() {
            let cells = row.cells.len();
            for (c, cell) in row.cells.iter_mut().enumerate() {
                let outer = |i: usize| b[i].flatten();
                let mut edges = [
                    if r == 0 { outer(0) } else { b[4].flatten() },
                    if c + 1 == cells { outer(1) } else { b[5].flatten() },
                    if r + 1 == count { outer(2) } else { b[4].flatten() },
                    if c == 0 { outer(3) } else { b[5].flatten() },
                ];
                for (e, own) in edges.iter_mut().zip(cell.own_borders) {
                    if let Some(own) = own {
                        *e = own;
                    }
                }
                cell.borders = edges;
            }
        }
    }
}

/// Most blocks are paragraphs, so they are kept inline (the lint's boxing
/// would add an allocation to each).
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Paragraph(FlowParagraph),
    Table(FlowTable),
}

/// Which of a section's headers and footers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartKind {
    Header,
    Footer,
    HeaderFirst,
    FooterFirst,
    HeaderEven,
    FooterEven,
}

impl PartKind {
    pub const ALL: [PartKind; 6] = [PartKind::Header, PartKind::Footer, PartKind::HeaderFirst, PartKind::FooterFirst, PartKind::HeaderEven, PartKind::FooterEven];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn is_footer(self) -> bool {
        matches!(self, PartKind::Footer | PartKind::FooterFirst | PartKind::FooterEven)
    }

    /// `w:headerReference/@w:type`'s value.
    pub fn reference_type(self) -> &'static str {
        match self {
            PartKind::Header | PartKind::Footer => "default",
            PartKind::HeaderFirst | PartKind::FooterFirst => "first",
            PartKind::HeaderEven | PartKind::FooterEven => "even",
        }
    }
}

/// A section's own setup (`w:sectPr` in the paragraph that ends it): its
/// page, page numbers and headers and footers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SectionSetup {
    pub page: PageSetup,
    /// The page numbers start again at `page.first_page` (`w:pgNumType/@w:start`);
    /// else they go on from the section before.
    pub restart_numbering: bool,
    pub page_number_format: Option<String>,
    /// The page numbers include the chapter number (`w:pgNumType/@w:chapStyle`, the level of the
    /// heading style) and the separator (`@w:chapSep`: hyphen, period, colon, emDash, enDash).
    pub page_chapter: Option<(u32, String)>,
    pub title_page: bool,
    /// The headers and footers, by [`PartKind::index`]; `None`: the
    /// previous section's (前と同じ).
    pub parts: [Option<Vec<Block>>; 6],
}

/// The line above the footnotes (or the endnotes): `w:footnote w:type="separator"`'s content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoteSeparator {
    /// The short line (`w:separator`).
    #[default]
    Standard,
    /// A line across the text (`w:continuationSeparator`).
    Full,
    /// None (an empty paragraph).
    Hidden,
}

/// 文書の既定テーマフォント言語（ECMA-376 Part 1 §17.15.1.88 `w:themeFontLang`、DOCX-JA-019）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeFontLang {
    /// Latin / 欧文テキスト言語 (`w:val`, 例: `en-US`).
    pub val: Option<String>,
    /// East Asian / 東アジア・和文テキスト言語 (`w:eastAsia`, 例: `ja-JP`).
    pub east_asia: Option<String>,
    /// Complex Script / Bidi テキスト言語 (`w:bidi`, 例: `ar-SA`).
    pub bidi: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlowDocument {
    /// The file's name, for `FILENAME` fields; `None` shows the text saved
    /// with them.
    pub file_name: Option<String>,
    /// The file's size in bytes, for `FILESIZE` fields.
    pub file_size: Option<u64>,
    pub page: PageSetup,
    pub header: Vec<Block>,
    pub footer: Vec<Block>,
    pub body: Vec<Block>,
    /// The first page's header and footer (with `title_page`), and the even
    /// pages' (with `even_and_odd`). Not edited yet.
    pub header_first: Vec<Block>,
    pub footer_first: Vec<Block>,
    pub header_even: Vec<Block>,
    pub footer_even: Vec<Block>,
    /// The first page has its own header and footer (`w:titlePg`).
    pub title_page: bool,
    /// Even pages have their own (`w:evenAndOddHeaders` in the settings).
    pub even_and_odd: bool,
    /// The default tab stop's spacing (`w:defaultTabStop` in the settings), in EMU; `None` when the file
    /// says none.
    pub default_tab_stop: Option<Emu>,
    /// How page numbers are written (`w:pgNumType/@w:fmt`, a list format).
    pub page_number_format: Option<String>,
    /// The last section's chapter number in its page numbers (see [`SectionSetup::page_chapter`]).
    pub page_chapter: Option<(u32, String)>,
    /// The last section's page numbers start again (else they go on from
    /// the section before; the first section starts at `page.first_page`).
    pub restart_numbering: bool,
    /// The last section's headers and footers that are the previous
    /// section's (前と同じ: no reference of their own), by [`PartKind::index`].
    pub linked: [bool; 6],
    /// The footnotes and endnotes (`footnotes.xml`, `endnotes.xml`; their
    /// separators aside).
    pub footnotes: Vec<Note>,
    pub endnotes: Vec<Note>,
    /// The comments and their replies (`comments.xml`).
    pub comments: Vec<Comment>,
    /// Changes are tracked (`w:trackRevisions` in the settings).
    pub track_changes: bool,
    /// The styles, for resolving formatting after an edit.
    pub styles: std::sync::Arc<FlowStyles>,
    /// The largest id given when the document was read: new paragraphs and
    /// tables get larger ones, so none is taken for an element of the file.
    pub id_floor: u32,
    /// 透かし: the text Word draws behind every page (a WordArt shape in
    /// the headers).
    pub watermark: Option<Watermark>,
    /// The sections' own watermarks when they differ (a file's), by the
    /// section's paragraph id (0: the last section's); empty: `watermark`
    /// on every page.
    pub section_watermarks: Vec<(u32, Option<Watermark>)>,
    /// ページの色 (`w:background`, shown with `displayBackgroundShape`):
    /// drawn on the screen only, not in a PDF or on paper (as Word for Mac).
    pub page_color: Option<Rgb>,
    /// ドキュメント全体の段落の間隔のプリセット（Word's デザイン > 段落の間隔: NoSpace, Compact, Tight, Open, Relaxed, Double）.
    pub paragraph_spacing_preset: Option<ParagraphSpacingPreset>,
    /// ドキュメント全体のスタイル セットのプリセット（Word's デザイン > ドキュメントの書式設定 > スタイル セット）.
    pub style_set_preset: Option<StyleSetPreset>,
    /// フィールドコードの表示（Word の「フィールドコードの表示/非表示」Alt+F9 / ⌥F9）。
    pub show_field_codes: bool,
    /// コンテンツ コントロールのデザイン モード（Word の「デザイン モード」開発 > デザイン モード）。
    pub form_design_mode: bool,
    /// 文書のプロパティ（`docProps/core.xml`, `docProps/app.xml`: タイトル、作成者、作成日時等）。
    pub properties: DocProperties,
    /// 最終版としてマーク（Word の「最終版にする」: `docProps/custom.xml` の `_MarkAsFinal`）。
    pub mark_as_final: bool,
    /// 禁則文字の設定（`word/settings.xml` の `w:noLineBreaksBefore`・`w:noLineBreaksAfter`、日本語）: 行頭に置かない文字・行末に置かない文字。
    /// `None` は標準の文字。
    pub no_line_start: Option<String>,
    pub no_line_end: Option<String>,
    /// 禁則処理の方式（Word の「禁則処理の方式」標準/厳密: `word/settings.xml` の `w:strictFirstAndLastChars`, ECMA-376 Part 1 §17.15.1.84, DOCX-JA-010）。
    /// `true`: 厳密（Strict: 小書き仮名や長音記号等を行頭禁則とする、Word 日本語版の標準）、`false`: 標準（Normal）。
    pub strict_first_and_last_chars: bool,
    /// 約物を詰める (`w:characterSpacingControl` compressPunctuation): a line that is a little too long fits by squeezing its
    /// stops and brackets to half an em.
    pub compress_punctuation: bool,
    /// The typefaces of the file's font table (`word/fontTable.xml`) with their `w:family` (`roman`, `swiss`, ... or empty): Word draws a
    /// typeface that is not installed with Cambria if the table says `roman` or does not list it, and with Calibri if it lists it otherwise.
    pub font_families: std::collections::BTreeMap<String, String>,
    /// 半角と全角の幅を調整する (`w:balanceSingleByteDoubleByteWidth` compat): a half-width space is half an em, whatever the
    /// font.
    pub balance_single_double_byte_width: bool,
    /// 表内の行の高さを文書グリッドに合わせる (`w:adjustLineHeightInTable` compat): the lines in a table's cells are on the line
    /// grid too.
    pub adjust_line_height_in_table: bool,
    /// HTML の段落の自動間隔を使わない (`w:doNotUseHTMLParagraphAutoSpacing` compat): adjacent paragraphs' space after and space
    /// before are summed rather than collapsed to the larger one.
    pub do_not_use_html_paragraph_auto_spacing: bool,
    /// Word's compatibility mode (`w:compatSetting` compatibilityMode: 11 for Word 2003-era files, 15 for 2013 and later).
    pub compat_mode: Option<u32>,
    /// 文書の既定テーマフォント言語・既定言語（Word の「校正言語の設定 > 既定に設定」: `word/settings.xml` の `w:themeFontLang`, ECMA-376 Part 1 §17.15.1.88, DOCX-JA-019）。
    pub theme_font_lang: Option<ThemeFontLang>,
    /// 読み取り専用を推奨（Word の「読み取り専用を推奨する」: `word/settings.xml` の `w:readOnlyRecommended`）。
    pub read_only_recommended: bool,
    /// 印刷前・文書オープン時にフィールドを更新（Word の「印刷前にフィールドを更新する」: `word/settings.xml` の `w:updateFields`）。
    pub update_fields: bool,
    /// The lines above the footnotes and the endnotes.
    pub note_separators: [NoteSeparator; 2],
    /// 差し込み印刷の文書の種類（`word/settings.xml` の `w:mailMerge/w:mainDocumentType`: `formLetters`・`email`・`fax`・`envelopes`・`mailingLabels`・`catalog`。無ければ通常の文書）。
    pub mail_merge_type: Option<String>,
    /// 差し込み印刷のフィールドと列の対応（`word/settings.xml` の `w:mailMerge/w:odso/w:fieldMapData`、ECMA-376 Part 1 §17.14.15、DOCX-MERGE-009）。
    pub mail_merge_field_map: Vec<FieldMapData>,
    /// 変更履歴の記録をロック（Word の「変更履歴の記録をロック」: `word/settings.xml` の `w:documentProtection w:edit="trackedChanges"`）。
    pub lock_tracking: bool,
    /// 文書変数（Word の `w:settings/w:docVars/w:docVar`）。
    pub doc_variables: std::collections::BTreeMap<String, String>,
    /// 添付テンプレート（Word の `word/settings.xml` の `w:attachedTemplate`）。
    pub attached_template: Option<String>,
    /// 文書のスタイルを自動更新する（Word の `word/settings.xml` の `w:linkStyles`, DOCX-STYLE-017, DOCX-FILE-009）。
    pub link_styles: bool,
    /// 表示モード（Word の `word/settings.xml` の `w:view`: print, reading, web, normal 等、DOCX-VIEW-005）。
    pub view_mode: DocumentViewMode,
    /// 文書の編集制限・保護モード (`word/settings.xml` の `w:documentProtection/@w:edit`、ECMA-376 Part 1 §17.15.1.29、DOCX-SEC-002, DOCX-SEC-004, DOCX-SEC-005)。
    pub protection: DocumentProtection,
    /// 書式変更の制限 (`word/settings.xml` の `w:documentProtection/@w:formatting`、ECMA-376 Part 1 §17.15.1.29、DOCX-SEC-003)。
    pub formatting_protection: bool,
    /// 閲覧・レビュー・編集モード（Word の 編集/レビュー/閲覧 ドロップダウンモード切替、DOCX-VIEW-021）。
    pub interaction_mode: DocumentInteractionMode,
    /// ファイル内の画像を圧縮しない (Word の「ファイル内の画像を圧縮しない」: `word/settings.xml` の `w:doNotAutoCompressPictures`, DOCX-OPTIONS-011)。
    pub do_not_compress_images: bool,
    /// 既定の解像度 (Word の「既定の解像度」: `word/settings.xml` の `w14:defaultImageDpi`, DOCX-OPTIONS-011)。
    pub default_image_dpi: Option<u32>,
    /// 画像のトリミング部分を破棄する (Word の「画像のトリミング部分を破棄する」: `word/settings.xml` の `w14:discardImageEditingData`, DOCX-OPTIONS-011)。
    pub discard_image_editing_data: bool,
    /// 自動回復用データの保存間隔（分、ECMA-376 Part 1 §17.15.1.72 `w:saveFrequency`、DOCX-OPTIONS-012）。
    pub save_frequency: Option<u32>,
    /// ファイルにフォントを埋め込む (Word の「ファイルにフォントを埋め込む」: `word/settings.xml` の `w:embedTrueTypeFonts`, ECMA-376 Part 1 §17.15.1.35, DOCX-OPTIONS-013)。
    pub embed_true_type_fonts: bool,
    /// 標準のシステム フォントを埋め込む (`word/settings.xml` の `w:embedSystemFonts`, ECMA-376 Part 1 §17.15.1.34, DOCX-OPTIONS-013)。
    pub embed_system_fonts: bool,
    /// 文書で使用されている文字だけを埋め込む (`word/settings.xml` の `w:saveSubsetFonts`, ECMA-376 Part 1 §17.15.1.75, DOCX-OPTIONS-013)。
    pub save_subset_fonts: bool,
    /// 互換性モード (Word の「この文書を開くときに使用する互換性モード」: `word/settings.xml` の `<w:compatSetting w:name="compatibilityMode" w:val="..."/>`, DOCX-OPTIONS-013)。
    pub compatibility_mode: Option<u32>,
    /// 互換性オプション: 東アジアのレイアウトを使用 (`word/settings.xml` の `w:compat/w:useFELayout`, DOCX-OPTIONS-013)。
    pub use_fe_layout: bool,
    /// 互換性オプション: Shift+Return で終わる行の文字間隔を広げない (`word/settings.xml` の `w:compat/w:doNotExpandShiftReturn`, DOCX-OPTIONS-013)。
    pub do_not_expand_shift_return: bool,
    /// 図形のグループ階層 (`wpg:wgp`, DOCX-FLOAT-016)。
    pub shape_groups: Vec<crate::ShapeGroup>,
}

/// A custom document property value (`docProps/custom.xml`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CustomPropertyValue {
    Text(String),
    Number(String),
    Bool(bool),
    DateTime(String),
}

/// A custom document property (`docProps/custom.xml`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomProperty {
    pub name: String,
    pub value: CustomPropertyValue,
}

/// Core, extended and custom document properties (`docProps/core.xml`, `docProps/app.xml`, `docProps/custom.xml`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocProperties {
    pub title: Option<String>,
    pub subject: Option<String>,
    pub creator: Option<String>,
    pub keywords: Option<String>,
    pub description: Option<String>,
    pub last_modified_by: Option<String>,
    pub revision: Option<String>,
    pub category: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub last_printed: Option<String>,
    pub company: Option<String>,
    pub manager: Option<String>,
    pub custom: Vec<CustomProperty>,
}

/// A text watermark (Word's デザイン > 透かし > ユーザー設定の透かし >
/// テキスト): the size is Word's 自動, the shape fitted to the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Watermark {
    pub text: String,
    pub font: String,
    pub color: Rgb,
    /// 半透明にする (`v:fill opacity=".5"`).
    pub semitransparent: bool,
    /// The turn, clockwise in degrees (`rotation` of the shape's style):
    /// Word's レイアウト 対角線上 is 315, 水平 0.
    pub rotation: i32,
    /// The shape's width and height (hundredths of a point) a file gives;
    /// `None`: Word's 自動 ([`Watermark::size_in`]).
    pub size: Option<(u32, u32)>,
}

impl Watermark {
    /// Turned (Word's 対角線上, or another turn a file gives).
    pub fn diagonal(&self) -> bool {
        self.rotation.rem_euclid(360) != 0
    }

    /// The shape's size (points) on a page whose margins leave `area_w` ×
    /// `area_h`, as Word's 自動 makes it: the text's box at the text's
    /// proportions (a wide character one em, a narrow one about 0.6),
    /// turned (if diagonal) within the area.
    pub fn size_in(&self, area_w: f32, area_h: f32) -> (f32, f32) {
        if let Some((w, h)) = self.size {
            return (w as f32 / 100.0, h as f32 / 100.0);
        }
        let ems: f32 = self.text.chars().map(|c| if (c as u32) >= 0x2E80 { 1.0 } else if c.is_ascii_uppercase() { 0.72 } else { 0.58 }).sum::<f32>().max(1.0);
        if self.diagonal() {
            // (w + h) / √2 across the shorter side.
            let side = area_w.min(area_h) * 0.92;
            let w = side * std::f32::consts::SQRT_2 / (1.0 + 1.0 / ems);
            (w, w / ems)
        } else {
            let w = area_w * 0.92;
            let h = (w / ems).min(area_h * 0.5);
            (h * ems, h)
        }
    }
}

/// A theme colour reference for flow runs (`w:color`'s `w:themeColor`, `w:themeTint`, `w:themeShade`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FlowThemeColor {
    /// The theme colour name (`dark1`, `light1`, `accent1`–`accent6`, `text1`, `background1`, etc.).
    pub name: String,
    /// Tint modifier in two-digit hex (e.g. "80", "CC").
    pub tint: Option<String>,
    /// Shade modifier in two-digit hex (e.g. "BF", "7F").
    pub shade: Option<String>,
}

impl FlowThemeColor {
    pub fn new(name: impl Into<String>, tint: Option<impl Into<String>>, shade: Option<impl Into<String>>) -> Self {
        Self {
            name: name.into(),
            tint: tint.map(Into::into),
            shade: shade.map(Into::into),
        }
    }

    /// Resolve this theme colour (`accent1`, `text1`, `dk1`, etc.) with its optional hex `tint` and `shade`
    /// against `colors` (falling back to the default Office theme colours when absent).
    pub fn resolve(&self, colors: &[(String, Rgb)]) -> Option<Rgb> {
        let key = match self.name.as_str() {
            "text1" | "dark1" => "dk1",
            "background1" | "light1" => "lt1",
            "text2" | "dark2" => "dk2",
            "background2" | "light2" => "lt2",
            "hyperlink" => "hlink",
            "followedHyperlink" => "folHlink",
            other => other,
        };
        let base = colors
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key) || k.eq_ignore_ascii_case(&self.name))
            .map(|(_, c)| *c)
            .or_else(|| match key {
                "dk1" => Some(Rgb(0x000000)),
                "lt1" => Some(Rgb(0xffffff)),
                "dk2" => Some(Rgb(0x44546a)),
                "lt2" => Some(Rgb(0xe7e6e6)),
                "accent1" => Some(Rgb(0x4472c4)),
                "accent2" => Some(Rgb(0xed7d31)),
                "accent3" => Some(Rgb(0xa5a5a5)),
                "accent4" => Some(Rgb(0xffc000)),
                "accent5" => Some(Rgb(0x5b9bd5)),
                "accent6" => Some(Rgb(0x70ad47)),
                "hlink" => Some(Rgb(0x0563c1)),
                "folHlink" => Some(Rgb(0x954f72)),
                _ => None,
            })?;
        let byte = |v: Option<&str>| v.and_then(|s| u8::from_str_radix(s, 16).ok()).map(|b| b as f32 / 255.0);
        let channels = [(base.0 >> 16) & 0xff, (base.0 >> 8) & 0xff, base.0 & 0xff].map(|c| {
            let mut c = c as f32;
            if let Some(t) = byte(self.tint.as_deref()) {
                c += (255.0 - c) * (1.0 - t);
            }
            if let Some(s) = byte(self.shade.as_deref()) {
                c *= s;
            }
            c.round().clamp(0.0, 255.0) as u32
        });
        Some(Rgb((channels[0] << 16) | (channels[1] << 8) | channels[2]))
    }
}

/// Run properties, each unset (`None`) until some level sets it: the direct
/// formatting of a run, or what a style sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunFormat {
    /// Latin typeface (`w:rFonts/@w:ascii`, theme fonts resolved).
    pub font: Option<String>,
    /// East Asian typeface (`w:rFonts/@w:eastAsia`).
    pub east_asian_font: Option<String>,
    /// Complex Script typeface (`w:rFonts/@w:cs`).
    pub cs_font: Option<String>,
    /// Whether `w:rFonts/@w:hint="eastAsia"` selects the East Asian font for ambiguous characters.
    pub east_asia_hint: Option<bool>,
    /// Hundredths of a point.
    pub size: Option<u32>,
    /// Complex Script font size (hundredths of a point, `w:szCs`).
    pub cs_size: Option<u32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub underline_style: Option<UnderlineStyle>,
    /// `Some(None)`: the text's colour.
    pub underline_color: Option<Option<Rgb>>,
    pub strike: Option<bool>,
    pub double_strike: Option<bool>,
    pub caps: Option<bool>,
    pub small_caps: Option<bool>,
    pub color: Option<Rgb>,
    /// Theme colour reference (`w:color/@w:themeColor`, `@w:themeTint`, `@w:themeShade`).
    pub theme_color: Option<FlowThemeColor>,
    /// `Some(None)` sets no highlight.
    pub highlight: Option<Option<Rgb>>,
    /// The pattern of the shading over the highlight (`Some(None)` none).
    pub shd_pattern: Option<Option<(ShadingPattern, Rgb)>>,
    /// The text effects (`Some(None)` none).
    pub effects: Option<Option<std::sync::Arc<TextEffects>>>,
    pub position: Option<VerticalPosition>,
    /// EMU.
    pub spacing: Option<Emu>,
    pub scale: Option<u32>,
    /// `Some(None)` sets no fitting.
    pub fit_text: Option<Option<(u32, u32)>>,
    pub hidden: Option<bool>,
    /// `Some(None)` sets no border.
    pub border: Option<Option<FlowBorder>>,
    /// Half-points (`w:kern`); `Some(0)` turns kerning off.
    pub kern: Option<u32>,
    /// 傍点 (`w:em`); `Some(EmphasisMark::None)` sets none.
    pub emphasis: Option<EmphasisMark>,
    /// 組み文字; `Some(None)` sets none.
    pub combine: Option<Option<Combine>>,
    /// 縦中横; `Some(false)` sets off.
    pub tate_chu_yoko: Option<bool>,
    /// 中抜き・輪郭 (`w:outline`).
    pub outline: Option<bool>,
    /// 影付き (`w:shadow`).
    pub shadow: Option<bool>,
    /// 浮き出し (`w:emboss`).
    pub emboss: Option<bool>,
    /// 浮き彫り (`w:imprint`).
    pub imprint: Option<bool>,
    /// 校正しない (`w:noProof`): the text is not checked for spelling.
    pub no_proof: Option<bool>,
    /// The language of the text (`w:lang/@w:val`, `en-US`), of its East
    /// Asian text (`w:lang/@w:eastAsia`, `ja-JP`), and of its Complex Script/Bidi text (`w:lang/@w:bidi`, `ar-SA`).
    pub lang: Option<String>,
    pub lang_east_asia: Option<String>,
    pub lang_bidi: Option<String>,
    /// OpenType 合字 (`w14:ligatures`).
    pub ligatures: Option<Ligatures>,
    /// OpenType 数字のスタイル (`w14:numForm`).
    pub num_form: Option<NumberForm>,
    /// OpenType 数字の間隔 (`w14:numSpacing`).
    pub num_spacing: Option<NumberSpacing>,
    /// OpenType 前後関係に依存する異体字 (`w14:cntxtAlts`).
    pub contextual_alternates: Option<bool>,
    /// An `EQ \oc` field's sizes of its enclosure and character (see `FlowRunStyle::enclosure_sizes`).
    pub enclosure_sizes: Option<(u32, u32)>,
}

impl RunFormat {
    /// Set what `other` sets, over what this sets.
    pub fn overlay(&mut self, other: &RunFormat) {
        fn set<T: Clone>(a: &mut Option<T>, b: &Option<T>) {
            if b.is_some() {
                a.clone_from(b);
            }
        }
        set(&mut self.font, &other.font);
        set(&mut self.east_asian_font, &other.east_asian_font);
        set(&mut self.cs_font, &other.cs_font);
        set(&mut self.east_asia_hint, &other.east_asia_hint);
        set(&mut self.size, &other.size);
        set(&mut self.cs_size, &other.cs_size);
        set(&mut self.bold, &other.bold);
        set(&mut self.italic, &other.italic);
        set(&mut self.underline, &other.underline);
        set(&mut self.underline_style, &other.underline_style);
        set(&mut self.underline_color, &other.underline_color);
        set(&mut self.strike, &other.strike);
        set(&mut self.double_strike, &other.double_strike);
        set(&mut self.caps, &other.caps);
        set(&mut self.small_caps, &other.small_caps);
        set(&mut self.color, &other.color);
        set(&mut self.theme_color, &other.theme_color);
        set(&mut self.highlight, &other.highlight);
        set(&mut self.shd_pattern, &other.shd_pattern);
        set(&mut self.effects, &other.effects);
        set(&mut self.position, &other.position);
        set(&mut self.spacing, &other.spacing);
        set(&mut self.scale, &other.scale);
        set(&mut self.fit_text, &other.fit_text);
        set(&mut self.hidden, &other.hidden);
        set(&mut self.border, &other.border);
        set(&mut self.kern, &other.kern);
        set(&mut self.emphasis, &other.emphasis);
        set(&mut self.combine, &other.combine);
        set(&mut self.tate_chu_yoko, &other.tate_chu_yoko);
        set(&mut self.outline, &other.outline);
        set(&mut self.shadow, &other.shadow);
        set(&mut self.emboss, &other.emboss);
        set(&mut self.imprint, &other.imprint);
        set(&mut self.no_proof, &other.no_proof);
        set(&mut self.lang, &other.lang);
        set(&mut self.lang_east_asia, &other.lang_east_asia);
        set(&mut self.lang_bidi, &other.lang_bidi);
        set(&mut self.ligatures, &other.ligatures);
        set(&mut self.num_form, &other.num_form);
        set(&mut self.num_spacing, &other.num_spacing);
        set(&mut self.contextual_alternates, &other.contextual_alternates);
        set(&mut self.enclosure_sizes, &other.enclosure_sizes);
    }

    /// This without what `other` sets to the same values (formatting a
    /// style has taken over).
    pub fn without(&self, other: &RunFormat) -> RunFormat {
        fn clear<T: PartialEq>(a: &mut Option<T>, b: &Option<T>) {
            if b.is_some() && a == b {
                *a = None;
            }
        }
        let mut f = self.clone();
        clear(&mut f.font, &other.font);
        clear(&mut f.east_asian_font, &other.east_asian_font);
        clear(&mut f.cs_font, &other.cs_font);
        clear(&mut f.east_asia_hint, &other.east_asia_hint);
        clear(&mut f.size, &other.size);
        clear(&mut f.cs_size, &other.cs_size);
        clear(&mut f.bold, &other.bold);
        clear(&mut f.italic, &other.italic);
        clear(&mut f.underline, &other.underline);
        clear(&mut f.underline_style, &other.underline_style);
        clear(&mut f.underline_color, &other.underline_color);
        clear(&mut f.strike, &other.strike);
        clear(&mut f.double_strike, &other.double_strike);
        clear(&mut f.caps, &other.caps);
        clear(&mut f.small_caps, &other.small_caps);
        clear(&mut f.color, &other.color);
        clear(&mut f.theme_color, &other.theme_color);
        clear(&mut f.highlight, &other.highlight);
        clear(&mut f.shd_pattern, &other.shd_pattern);
        clear(&mut f.effects, &other.effects);
        clear(&mut f.position, &other.position);
        clear(&mut f.spacing, &other.spacing);
        clear(&mut f.scale, &other.scale);
        clear(&mut f.fit_text, &other.fit_text);
        clear(&mut f.hidden, &other.hidden);
        clear(&mut f.border, &other.border);
        clear(&mut f.kern, &other.kern);
        clear(&mut f.emphasis, &other.emphasis);
        clear(&mut f.combine, &other.combine);
        clear(&mut f.tate_chu_yoko, &other.tate_chu_yoko);
        clear(&mut f.outline, &other.outline);
        clear(&mut f.shadow, &other.shadow);
        clear(&mut f.emboss, &other.emboss);
        clear(&mut f.imprint, &other.imprint);
        clear(&mut f.no_proof, &other.no_proof);
        clear(&mut f.lang, &other.lang);
        clear(&mut f.lang_east_asia, &other.lang_east_asia);
        clear(&mut f.lang_bidi, &other.lang_bidi);
        clear(&mut f.ligatures, &other.ligatures);
        clear(&mut f.num_form, &other.num_form);
        clear(&mut f.num_spacing, &other.num_spacing);
        clear(&mut f.contextual_alternates, &other.contextual_alternates);
        clear(&mut f.enclosure_sizes, &other.enclosure_sizes);
        f
    }

    /// The style drawn, with the defaults for what nothing sets.
    pub fn style(&self) -> FlowRunStyle {
        let d = FlowRunStyle::default();
        FlowRunStyle {
            font: self.font.clone(),
            east_asian_font: self.east_asian_font.clone(),
            cs_font: self.cs_font.clone(),
            east_asia_hint: self.east_asia_hint.unwrap_or(false),
            size: self.size.unwrap_or(d.size),
            cs_size: self.cs_size,
            bold: self.bold.unwrap_or(false),
            italic: self.italic.unwrap_or(false),
            underline: self.underline.unwrap_or(false),
            underline_style: self.underline_style.unwrap_or_default(),
            underline_color: self.underline_color.flatten(),
            strike: self.strike.unwrap_or(false),
            double_strike: self.double_strike.unwrap_or(false),
            caps: self.caps.unwrap_or(false),
            small_caps: self.small_caps.unwrap_or(false),
            color: self.color.unwrap_or(Rgb::BLACK),
            highlight: self.highlight.flatten(),
            shd_pattern: self.shd_pattern.flatten(),
            effects: self.effects.clone().flatten(),
            position: self.position.unwrap_or_default(),
            spacing: self.spacing.unwrap_or(0),
            scale: self.scale.unwrap_or(100),
            fit_text: self.fit_text.flatten(),
            border: self.border.flatten(),
            kern: self.kern.filter(|&k| k > 0),
            emphasis: self.emphasis.unwrap_or_default(),
            combine: self.combine.flatten(),
            tate_chu_yoko: self.tate_chu_yoko.unwrap_or(false),
            hidden: self.hidden.unwrap_or(false),
            outline: self.outline.unwrap_or(false),
            shadow: self.shadow.unwrap_or(false),
            emboss: self.emboss.unwrap_or(false),
            imprint: self.imprint.unwrap_or(false),
            ligatures: self.ligatures.unwrap_or_default(),
            num_form: self.num_form.unwrap_or_default(),
            num_spacing: self.num_spacing.unwrap_or_default(),
            contextual_alternates: self.contextual_alternates.unwrap_or(false),
            enclosure_sizes: self.enclosure_sizes,
        }
    }
}

/// A tab stop set (`Some`) or cleared (`None`) at a position (`w:tabs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabChange {
    pub position: Emu,
    pub stop: Option<TabStop>,
}

/// Paragraph properties, each unset until some level sets it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParaFormat {
    pub align: Option<FlowAlign>,
    pub text_alignment: Option<FontAlign>,
    pub left: Option<Emu>,
    pub right: Option<Emu>,
    pub first_line: Option<Emu>,
    pub left_chars: Option<i32>,
    pub right_chars: Option<i32>,
    pub first_line_chars: Option<i32>,
    pub before: Option<Emu>,
    pub after: Option<Emu>,
    pub line: Option<LineHeight>,
    pub snap: Option<bool>,
    pub auto_space_letters: Option<bool>,
    pub auto_space_digits: Option<bool>,
    pub page_break_before: Option<bool>,
    pub suppress_line_numbers: Option<bool>,
    pub suppress_auto_hyphens: Option<bool>,
    pub contextual_spacing: Option<bool>,
    pub auto_before: Option<bool>,
    pub auto_after: Option<bool>,
    pub widow_control: Option<bool>,
    pub keep_next: Option<bool>,
    pub keep_lines: Option<bool>,
    pub kinsoku: Option<bool>,
    pub word_wrap: Option<bool>,
    pub overflow_punct: Option<bool>,
    pub bidi: Option<bool>,
    pub collapsed: Option<bool>,
    pub tabs: Vec<TabChange>,
    pub shading: Option<Option<Rgb>>,
    pub shading_fill: Option<Option<Rgb>>,
    pub shading_pattern: Option<Option<ShadingPattern>>,
    pub shading_color: Option<Option<Rgb>>,
    pub borders: [Option<Option<FlowBorder>>; 4],
    /// The list the paragraph is in (`w:numPr/w:numId`; 0 is none).
    pub num_id: Option<u32>,
    /// Its level in the list (`w:numPr/w:ilvl`).
    pub level: Option<u32>,
    pub drop_cap: Option<Option<DropCap>>,
    pub framed: Option<bool>,
}

impl ParaFormat {
    pub fn overlay(&mut self, other: &ParaFormat) {
        fn set<T: Clone>(a: &mut Option<T>, b: &Option<T>) {
            if b.is_some() {
                a.clone_from(b);
            }
        }
        set(&mut self.align, &other.align);
        set(&mut self.text_alignment, &other.text_alignment);
        set(&mut self.left, &other.left);
        set(&mut self.right, &other.right);
        set(&mut self.first_line, &other.first_line);
        set(&mut self.left_chars, &other.left_chars);
        set(&mut self.right_chars, &other.right_chars);
        set(&mut self.first_line_chars, &other.first_line_chars);
        set(&mut self.before, &other.before);
        set(&mut self.after, &other.after);
        set(&mut self.line, &other.line);
        set(&mut self.snap, &other.snap);
        set(&mut self.auto_space_letters, &other.auto_space_letters);
        set(&mut self.auto_space_digits, &other.auto_space_digits);
        set(&mut self.page_break_before, &other.page_break_before);
        set(&mut self.suppress_line_numbers, &other.suppress_line_numbers);
        set(&mut self.suppress_auto_hyphens, &other.suppress_auto_hyphens);
        set(&mut self.contextual_spacing, &other.contextual_spacing);
        set(&mut self.auto_before, &other.auto_before);
        set(&mut self.auto_after, &other.auto_after);
        set(&mut self.widow_control, &other.widow_control);
        set(&mut self.keep_next, &other.keep_next);
        set(&mut self.keep_lines, &other.keep_lines);
        set(&mut self.kinsoku, &other.kinsoku);
        set(&mut self.word_wrap, &other.word_wrap);
        set(&mut self.overflow_punct, &other.overflow_punct);
        set(&mut self.bidi, &other.bidi);
        set(&mut self.collapsed, &other.collapsed);
        self.tabs.extend(other.tabs.iter().copied());
        set(&mut self.shading, &other.shading);
        set(&mut self.shading_fill, &other.shading_fill);
        set(&mut self.shading_pattern, &other.shading_pattern);
        set(&mut self.shading_color, &other.shading_color);
        for (a, b) in self.borders.iter_mut().zip(&other.borders) {
            set(a, b);
        }
        set(&mut self.num_id, &other.num_id);
        set(&mut self.level, &other.level);
        set(&mut self.drop_cap, &other.drop_cap);
        set(&mut self.framed, &other.framed);
    }

    /// The tab stops after setting and clearing them in order.
    pub fn tab_stops(&self) -> Vec<TabStop> {
        let mut out: Vec<TabStop> = Vec::new();
        for change in &self.tabs {
            out.retain(|t| t.position != change.position);
            if let Some(stop) = change.stop {
                out.push(stop);
            }
        }
        out.sort_by_key(|t| t.position);
        out
    }

    /// Set a paragraph's drawn properties from this (the defaults for what
    /// nothing sets).
    pub fn apply_to(&self, p: &mut FlowParagraph) {
        p.align = self.align.unwrap_or_default();
        p.text_alignment = self.text_alignment.unwrap_or_default();
        p.left = self.left.unwrap_or(0);
        p.right = self.right.unwrap_or(0);
        p.first_line = self.first_line.unwrap_or(0);
        p.left_chars = self.left_chars;
        p.right_chars = self.right_chars;
        p.first_line_chars = self.first_line_chars;
        p.space_before = self.before.unwrap_or(0);
        p.space_after = self.after.unwrap_or(0);
        p.line = self.line.unwrap_or_default();
        p.snap_to_grid = self.snap.unwrap_or(true);
        p.auto_space_letters = self.auto_space_letters.unwrap_or(true);
        p.auto_space_digits = self.auto_space_digits.unwrap_or(true);
        p.page_break_before = self.page_break_before.unwrap_or(false);
        p.suppress_line_numbers = self.suppress_line_numbers.unwrap_or(false);
        p.suppress_auto_hyphens = self.suppress_auto_hyphens.unwrap_or(false);
        p.contextual_spacing = self.contextual_spacing.unwrap_or(false);
        p.auto_before = self.auto_before.unwrap_or(false);
        p.auto_after = self.auto_after.unwrap_or(false);
        p.widow_control = self.widow_control.unwrap_or(true);
        p.keep_with_next = self.keep_next.unwrap_or(false);
        p.keep_lines = self.keep_lines.unwrap_or(false);
        p.kinsoku = self.kinsoku.unwrap_or(true);
        p.word_wrap = self.word_wrap.unwrap_or(true);
        p.overflow_punct = self.overflow_punct.unwrap_or(true);
        p.bidi = self.bidi.unwrap_or(false);
        p.collapsed = self.collapsed.unwrap_or(false);
        p.tabs = self.tab_stops();
        p.shading = self.shading.flatten();
        p.shading_fill = self.shading_fill.flatten();
        p.shading_pattern = self.shading_pattern.flatten();
        p.shading_color = self.shading_color.flatten();
        p.borders = self.borders.map(Option::flatten);
        if let Some(dc) = self.drop_cap {
            p.drop_cap = dc;
        }
        p.framed = self.framed.unwrap_or(false);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StyleKind {
    #[default]
    Paragraph,
    Character,
    Table,
    Numbering,
}

/// A style of `styles.xml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlowStyle {
    pub kind: StyleKind,
    /// The name shown (`w:name`), such as `heading 1`.
    pub name: String,
    pub based_on: Option<String>,
    pub para: ParaFormat,
    pub run: RunFormat,
    /// The style of the paragraph Enter starts after one of this style
    /// (`w:next`): 標準 after a heading.
    pub next: Option<String>,
    /// スタイルを自動的に更新する (`w:autoRedefine`): formatting changed by hand on a
    /// paragraph of this style is taken into the style.
    pub auto_update: bool,
    /// Where the style comes in the gallery (`w:uiPriority`; 99 when it has none).
    pub ui_priority: Option<u32>,
    /// Whether the gallery offers it (`w:qFormat`).
    pub quick: bool,
    /// A table style's formatting of some of its cells and their
    /// paragraphs (`w:tblStylePr`: the heading row, the bands…).
    pub conditional: Vec<TableCondition>,
    /// A numbering style's list instance id (`w:pPr/w:numPr/w:numId`).
    pub num_id: Option<u32>,
}

/// A table style's formatting of one part of its tables (`w:tblStylePr`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableCondition {
    pub region: TableRegion,
    pub para: ParaFormat,
    pub run: RunFormat,
    /// The cells' fill (`w:tcPr/w:shd`).
    pub fill: Option<Rgb>,
}

/// What a floating table is placed from (`w:vertAnchor`, `w:horzAnchor`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableAnchor {
    /// The text: the column across, the paragraph after it down.
    #[default]
    Text,
    Margin,
    Page,
}

/// Where a floating table goes across (`w:tblpXSpec`) or down
/// (`w:tblpYSpec`) in its anchor, rather than at an offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableAlign {
    Start,
    Center,
    End,
    /// The inside or outside of facing pages (left on odd pages for
    /// inside).
    Inside,
    Outside,
}

/// A floating table's placement (`w:tblpPr`, EMU).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TableFloat {
    pub horz_anchor: TableAnchor,
    pub vert_anchor: TableAnchor,
    pub x: Emu,
    pub x_align: Option<TableAlign>,
    pub y: Emu,
    pub y_align: Option<TableAlign>,
    /// The room kept from the text: left, right, top, bottom.
    pub distance: [Emu; 4],
}

impl TableFloat {
    /// Standard floating table: Margin anchor, Start (left/top) align, 9pt left/right distance, 0pt top/bottom distance.
    pub fn standard() -> Self {
        Self {
            horz_anchor: TableAnchor::Margin,
            vert_anchor: TableAnchor::Margin,
            x: 0,
            x_align: Some(TableAlign::Start),
            y: 0,
            y_align: Some(TableAlign::Start),
            distance: [180 * 635, 180 * 635, 0, 0],
        }
    }
}

/// The style a table is drawn with and its options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableStyleRef {
    pub id: String,
    pub look: TableLook,
    /// Rows and columns in each band (`w:tblStyleRowBandSize`, `ColBandSize`).
    pub row_band: u32,
    pub col_band: u32,
}

/// The parts of a table its style formats apart (`ST_TblStyleOverrideType`),
/// in the order they go over each other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TableRegion {
    #[default]
    WholeTable,
    Band1Vert,
    Band2Vert,
    Band1Horz,
    Band2Horz,
    FirstCol,
    LastCol,
    FirstRow,
    LastRow,
    NwCell,
    NeCell,
    SwCell,
    SeCell,
}

impl TableRegion {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "wholeTable" => Self::WholeTable,
            "band1Vert" => Self::Band1Vert,
            "band2Vert" => Self::Band2Vert,
            "band1Horz" => Self::Band1Horz,
            "band2Horz" => Self::Band2Horz,
            "firstCol" => Self::FirstCol,
            "lastCol" => Self::LastCol,
            "firstRow" => Self::FirstRow,
            "lastRow" => Self::LastRow,
            "nwCell" => Self::NwCell,
            "neCell" => Self::NeCell,
            "swCell" => Self::SwCell,
            "seCell" => Self::SeCell,
            _ => return None,
        })
    }

    /// The parts cell (`row`, `col`) of a `rows` × `cols` table is in, in
    /// the order they apply, as its style's options (`w:tblLook`) and band
    /// sizes say. The bands leave out the heading and total rows and the
    /// first and last columns only when the style formats those
    /// (`bands`: the options for that); the columns' bands go over the
    /// rows' (Word's PDF of DTST).
    #[allow(clippy::too_many_arguments)]
    pub fn of_cell(row: usize, col: usize, rows: usize, cols: usize, look: TableLook, bands: TableLook, row_band: usize, col_band: usize) -> Vec<TableRegion> {
        let mut out = vec![TableRegion::WholeTable];
        let first_row = look.first_row && row == 0;
        let last_row = look.last_row && row + 1 == rows && rows > 1;
        let first_col = look.first_column && col == 0;
        let last_col = look.last_column && col + 1 == cols && cols > 1;
        let skip_row = (bands.first_row && row == 0) || (bands.last_row && row + 1 == rows && rows > 1);
        let skip_col = (bands.first_column && col == 0) || (bands.last_column && col + 1 == cols && cols > 1);
        if !look.no_hband && !skip_row {
            let r = row - usize::from(bands.first_row);
            out.push(if (r / row_band.max(1)) % 2 == 0 { TableRegion::Band1Horz } else { TableRegion::Band2Horz });
        }
        if !look.no_vband && !skip_col {
            let c = col - usize::from(bands.first_column);
            out.push(if (c / col_band.max(1)) % 2 == 0 { TableRegion::Band1Vert } else { TableRegion::Band2Vert });
        }
        if first_col {
            out.push(TableRegion::FirstCol);
        }
        if last_col {
            out.push(TableRegion::LastCol);
        }
        if first_row {
            out.push(TableRegion::FirstRow);
        }
        if last_row {
            out.push(TableRegion::LastRow);
        }
        if first_row && first_col {
            out.push(TableRegion::NwCell);
        }
        if first_row && last_col {
            out.push(TableRegion::NeCell);
        }
        if last_row && first_col {
            out.push(TableRegion::SwCell);
        }
        if last_row && last_col {
            out.push(TableRegion::SeCell);
        }
        out
    }
}

/// A table's style options (`w:tblLook`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TableLook {
    pub first_row: bool,
    pub last_row: bool,
    pub first_column: bool,
    pub last_column: bool,
    pub no_hband: bool,
    pub no_vband: bool,
}

/// The document's styles and defaults: what paragraphs and runs inherit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlowStyles {
    pub defaults_run: RunFormat,
    pub defaults_para: ParaFormat,
    pub styles: std::collections::BTreeMap<String, FlowStyle>,
    pub default_paragraph: Option<String>,
    pub default_table: Option<String>,
    /// The list definitions (`numbering.xml`).
    pub numbering: Numbering,
}

impl FlowStyles {
    /// The chain of styles from the root to `id`.
    pub fn chain(&self, id: &str) -> Vec<&FlowStyle> {
        let mut out = Vec::new();
        let mut at = Some(id);
        while let Some(id) = at {
            let Some(style) = self.styles.get(id) else { break };
            if out.len() > 16 {
                break;
            }
            out.push(style);
            at = style.based_on.as_deref();
        }
        out.reverse();
        out
    }

    /// What a paragraph inherits (defaults, its table's style, its style)
    /// before its direct formatting.
    pub fn paragraph_base(&self, style: Option<&str>, table_style: Option<&str>) -> (ParaFormat, RunFormat) {
        self.paragraph_base_in(style, table_style, &[])
    }

    /// [`FlowStyles::paragraph_base`] in a cell that is in `regions` of its
    /// table (the table style's conditional formatting, over its own).
    pub fn paragraph_base_in(&self, style: Option<&str>, table_style: Option<&str>, regions: &[TableRegion]) -> (ParaFormat, RunFormat) {
        let mut para = self.defaults_para.clone();
        let mut run = self.defaults_run.clone();
        // Without a size in `w:docDefaults` the text is 10 pt (a style that is not based on Normal does not get Normal's
        // size; Word's PDF of a document whose style `a3` has no `w:basedOn` shows its 10.5 pt Normal text at 10 pt in it).
        if run.size.is_none() {
            run.size = Some(1000);
        }
        if let Some(id) = table_style {
            let chain = self.chain(id);
            for s in &chain {
                para.overlay(&s.para);
                run.overlay(&s.run);
            }
            for region in regions {
                for s in &chain {
                    for c in s.conditional.iter().filter(|c| c.region == *region) {
                        para.overlay(&c.para);
                        run.overlay(&c.run);
                    }
                }
            }
        }
        if let Some(id) = style.or(self.default_paragraph.as_deref()) {
            for s in self.chain(id) {
                para.overlay(&s.para);
                run.overlay(&s.run);
            }
        }
        (para, run)
    }

    /// A run's formatting over its paragraph's `base`.
    pub fn run_format(&self, base: &RunFormat, char_style: Option<&str>, direct: &RunFormat) -> RunFormat {
        let mut run = base.clone();
        if let Some(id) = char_style {
            let chain = self.chain(id);
            if chain.is_empty() {
                if id.eq_ignore_ascii_case("hyperlink") {
                    run.overlay(&RunFormat {
                        color: Some(Rgb(0x0563C1)),
                        underline: Some(true),
                        underline_style: Some(UnderlineStyle::Single),
                        ..RunFormat::default()
                    });
                } else if id.eq_ignore_ascii_case("followedhyperlink") {
                    run.overlay(&RunFormat {
                        color: Some(Rgb(0x954F72)),
                        underline: Some(true),
                        underline_style: Some(UnderlineStyle::Single),
                        ..RunFormat::default()
                    });
                }
            } else {
                for s in chain {
                    run.overlay(&s.run);
                }
            }
        }
        run.overlay(direct);
        run
    }

    /// The id of the paragraph style named `name` (`heading 1`), ignoring case.
    pub fn paragraph_style_named(&self, name: &str) -> Option<&str> {
        self.styles.iter().find(|(_, s)| s.kind == StyleKind::Paragraph && s.name.eq_ignore_ascii_case(name)).map(|(id, _)| id.as_str())
    }
}

impl FlowParagraph {
    /// Fold the styles and the direct formatting into the paragraph's drawn
    /// properties and its runs' styles, after its formatting changed.
    pub fn resolve(&mut self, styles: &FlowStyles) {
        let (mut para, base) = styles.paragraph_base_in(self.style.as_deref(), self.table_style.as_deref(), &self.table_regions);
        // A list's level sets indents over the style's; the paragraph's own
        // formatting goes over both.
        let num_id = self.direct.num_id.or(para.num_id).unwrap_or(0);
        let level = self.direct.level.or(para.level).unwrap_or(0);
        self.list = None;
        let mut list_run = None;
        if let Some(lvl) = styles.numbering.level(num_id, level) {
            para.overlay(&lvl.para);
            self.list = Some(ListRef { num_id, level });
            list_run = Some(lvl.run.clone());
        }
        para.overlay(&self.direct);
        para.apply_to(self);
        for run in &mut self.runs {
            run.style = styles.run_format(&base, run.char_style.as_deref(), &run.format).style();
        }
        let mut mark = base;
        mark.overlay(&self.mark_format);
        self.mark = mark.style();
        self.list_style = match list_run {
            Some(run) => {
                let mut m = mark;
                m.overlay(&run);
                m.style()
            }
            None => FlowRunStyle::default(),
        };
    }

    /// The characters of the paragraph's positions (one for each tab,
    /// break, picture and field).
    pub fn len(&self) -> usize {
        self.runs.iter().map(FlowRun::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

impl FlowDocument {
    /// An id for a new shape (a chart): above the ids of the shapes in the document, and as `new_id`.
    pub fn new_shape_id(&self) -> u64 {
        fn max(blocks: &[Block], m: &mut u64) {
            for b in blocks {
                match b {
                    Block::Paragraph(p) => {
                        for r in &p.runs {
                            if let FlowRunKind::Shape(s) = &r.kind {
                                *m = (*m).max(s.id.0);
                            }
                        }
                    }
                    Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| max(&c.blocks, m)),
                }
            }
        }
        let mut m = u64::from(self.new_id());
        max(&self.body, &mut m);
        max(&self.header, &mut m);
        max(&self.footer, &mut m);
        for list in [&self.header_first, &self.footer_first, &self.header_even, &self.footer_even] {
            max(list, &mut m);
        }
        for note in self.footnotes.iter().chain(&self.endnotes) {
            max(&note.blocks, &mut m);
        }
        for c in &self.comments {
            max(&c.blocks, &mut m);
        }
        m + 1
    }

    /// An id for a new paragraph or table: larger than any in the document
    /// and than any the file had.
    pub fn new_id(&self) -> u32 {
        fn max(blocks: &[Block], m: &mut u32) {
            for b in blocks {
                match b {
                    Block::Paragraph(p) => *m = (*m).max(p.id),
                    Block::Table(t) => {
                        *m = (*m).max(t.id);
                        for row in &t.rows {
                            *m = (*m).max(row.id);
                            for cell in &row.cells {
                                *m = (*m).max(cell.id);
                                max(&cell.blocks, m);
                            }
                        }
                    }
                }
            }
        }
        let mut m = self.id_floor;
        max(&self.header, &mut m);
        max(&self.footer, &mut m);
        max(&self.body, &mut m);
        for list in [&self.header_first, &self.footer_first, &self.header_even, &self.footer_even] {
            max(list, &mut m);
        }
        for b in &self.body {
            if let Block::Paragraph(p) = b
                && let Some(section) = &p.section
            {
                for list in section.parts.iter().flatten() {
                    max(list, &mut m);
                }
            }
        }
        for note in self.footnotes.iter().chain(&self.endnotes) {
            max(&note.blocks, &mut m);
        }
        for c in &self.comments {
            max(&c.blocks, &mut m);
        }
        m + 1
    }

    /// Whether the document is in Word Compatibility Mode (`compatibilityMode` < 15: Word 2003=11, 2007=12, 2010=14).
    pub fn is_compatibility_mode(&self) -> bool {
        self.compatibility_mode.is_some_and(|m| m < 15)
    }

    /// Human-readable Word format version name for `compatibility_mode`.
    pub fn compatibility_mode_version_name(&self) -> &'static str {
        match self.compatibility_mode {
            Some(11) => "Word 2003",
            Some(12) => "Word 2007",
            Some(14) => "Word 2010",
            Some(m) if m < 15 => "Word 97-2003",
            _ => "Word 2013-2024",
        }
    }

    /// Re-resolve all theme colour references (`FlowThemeColor`) in styles and paragraphs against `colors`.
    pub fn retheme(&mut self, colors: &[(String, Rgb)]) {
        let styles = std::sync::Arc::make_mut(&mut self.styles);
        for style in styles.styles.values_mut() {
            if let Some(tc) = &style.run.theme_color
                && let Some(rgb) = tc.resolve(colors)
            {
                style.run.color = Some(rgb);
            }
        }
        let styles_arc = self.styles.clone();
        fn retheme_blocks(blocks: &mut [Block], colors: &[(String, Rgb)], styles: &FlowStyles) {
            for b in blocks {
                match b {
                    Block::Paragraph(p) => {
                        if let Some(tc) = &p.mark_format.theme_color
                            && let Some(rgb) = tc.resolve(colors)
                        {
                            p.mark_format.color = Some(rgb);
                        }
                        for r in &mut p.runs {
                            if let Some(tc) = &r.format.theme_color
                                && let Some(rgb) = tc.resolve(colors)
                            {
                                r.format.color = Some(rgb);
                            }
                        }
                        if let Some(sec) = &mut p.section {
                            let sec = std::sync::Arc::make_mut(sec);
                            for part in sec.parts.iter_mut().flatten() {
                                retheme_blocks(part, colors, styles);
                            }
                        }
                        p.resolve(styles);
                    }
                    Block::Table(t) => {
                        for row in &mut t.rows {
                            for cell in &mut row.cells {
                                retheme_blocks(&mut cell.blocks, colors, styles);
                            }
                        }
                    }
                }
            }
        }
        for list in [
            &mut self.body,
            &mut self.header,
            &mut self.footer,
            &mut self.header_first,
            &mut self.footer_first,
            &mut self.header_even,
            &mut self.footer_even,
        ] {
            retheme_blocks(list, colors, &styles_arc);
        }
        for note in self.footnotes.iter_mut().chain(&mut self.endnotes) {
            retheme_blocks(&mut note.blocks, colors, &styles_arc);
        }
        for comment in &mut self.comments {
            retheme_blocks(&mut comment.blocks, colors, &styles_arc);
        }
    }
}

/// A paragraph's place in a list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListRef {
    pub num_id: u32,
    pub level: u32,
}

/// What follows a list item's number (`w:suff`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListSuffix {
    #[default]
    Tab,
    Space,
    Nothing,
}

/// A level of a list (`w:lvl`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListLevel {
    pub start: u32,
    /// `w:numFmt` (`decimal`, `bullet`, `lowerLetter`, …).
    pub format: String,
    /// `w:lvlText`: `%1.` and the like, or the bullet (symbol fonts' private
    /// use characters mapped to Unicode).
    pub text: String,
    pub suffix: ListSuffix,
    /// How the number sits on its position (`w:lvlJc`): left of it
    /// starting there, centred on it, or ending there.
    pub align: FlowAlign,
    /// The level's paragraph properties (its indents).
    pub para: ParaFormat,
    /// The number's run properties.
    pub run: RunFormat,
    /// A picture bullet (`w:lvlPicBulletId`): the id of one of
    /// [`Numbering::pic_bullets`]; the bullet is that picture.
    pub pic_bullet: Option<u32>,
}

/// A list definition (`w:abstractNum`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AbstractList {
    pub levels: Vec<ListLevel>,
    /// Link to a numbering style (`w:numStyleLink`): the styleId of the style in `styles.xml`.
    pub num_style_link: Option<String>,
}

/// A list (`w:num`): a definition, with levels restarted at other numbers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListInstance {
    pub abstract_id: u32,
    pub start_overrides: std::collections::BTreeMap<u32, u32>,
}

/// The lists of a document (`numbering.xml`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Numbering {
    pub abstracts: std::collections::BTreeMap<u32, AbstractList>,
    pub nums: std::collections::BTreeMap<u32, ListInstance>,
    /// The pictures of the picture bullets (`w:numPicBullet`), by id: the media
    /// and its size when the bullet says one.
    pub pic_bullets: std::collections::BTreeMap<u32, (crate::model::MediaId, Option<crate::geometry::EmuSize>)>,
}

impl Numbering {
    pub fn level(&self, num_id: u32, level: u32) -> Option<&ListLevel> {
        if num_id == 0 {
            return None;
        }
        let num = self.nums.get(&num_id)?;
        let levels = &self.abstracts.get(&num.abstract_id)?.levels;
        levels.get(level as usize).or_else(|| levels.last())
    }

    /// These definitions with a new list of bullets or numbers (Word's
    /// defaults for Japanese: 9 levels, 22 pt apart), and its id.
    pub fn with_new_list(&self, bullet: bool) -> (Numbering, u32) {
        let mut out = self.clone();
        let abstract_id = out.abstracts.keys().max().map_or(0, |m| m + 1);
        let num_id = out.nums.keys().max().map_or(1, |m| m + 1);
        let levels = (0..9u32)
            .map(|l| {
                let (format, text) = if bullet {
                    ("bullet", ["●", "○", "■"][l as usize % 3].to_string())
                } else {
                    let n = l + 1;
                    match l % 3 {
                        0 => ("decimal", format!("%{n}.")),
                        1 => ("aiueoFullWidth", format!("(%{n})")),
                        _ => ("decimalEnclosedCircle", format!("%{n}")),
                    }
                };
                ListLevel {
                    start: 1,
                    format: format.into(),
                    text,
                    suffix: ListSuffix::Tab,
                    align: FlowAlign::Left,
                    para: ParaFormat { left: Some(440 * 635 * (l as i64 + 1)), first_line: Some(-440 * 635), ..ParaFormat::default() },
                    run: RunFormat::default(),
                    pic_bullet: None,
                }
            })
            .collect();
        out.abstracts.insert(abstract_id, AbstractList { levels, num_style_link: None });
        out.nums.insert(num_id, ListInstance { abstract_id, start_overrides: Default::default() });
        (out, num_id)
    }

    /// These definitions with a new list style (`w:numStyleLink`) with the given style ID,
    /// optional custom levels, returning the updated Numbering and the newly created `num_id`.
    pub fn with_new_list_style(&self, style_id: &str, levels: Option<Vec<ListLevel>>) -> (Numbering, u32) {
        let (mut out, num_id) = self.with_new_list(false);
        let abstract_id = out.nums[&num_id].abstract_id;
        if let Some(list) = out.abstracts.get_mut(&abstract_id) {
            list.num_style_link = Some(style_id.to_string());
            if let Some(lvls) = levels {
                list.levels = lvls;
            }
        }
        (out, num_id)
    }

    /// These definitions with a new list whose bullets are the picture `media` (`size`: the bullet's own, or the text's size), and its id.
    pub fn with_new_picture_list(&self, media: crate::model::MediaId, size: Option<crate::geometry::EmuSize>) -> (Numbering, u32) {
        let (mut out, num_id) = self.with_new_list(true);
        let picture_id = out.pic_bullets.keys().max().map_or(0, |m| m + 1);
        out.pic_bullets.insert(picture_id, (media, size));
        let abstract_id = out.nums[&num_id].abstract_id;
        if let Some(list) = out.abstracts.get_mut(&abstract_id) {
            for level in &mut list.levels {
                level.pic_bullet = Some(picture_id);
                level.text = "\u{2022}".to_string();
            }
        }
        (out, num_id)
    }

    /// The first list whose first level is a bullet (`bullet`) or a number.
    pub fn find(&self, bullet: bool) -> Option<u32> {
        self.nums.iter().find(|(_, n)| self.abstracts.get(&n.abstract_id).and_then(|a| a.levels.first()).is_some_and(|l| (l.format == "bullet") == bullet)).map(|(id, _)| *id)
    }
}

/// The text of a number in a list format (`ST_NumberFormat`).
pub fn format_list_number(n: u32, format: &str) -> String {
    fn roman(mut n: u32) -> String {
        let table = [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")];
        let mut s = String::new();
        for (v, r) in table {
            while n >= v {
                s.push_str(r);
                n -= v;
            }
        }
        s
    }
    fn letters(n: u32) -> String {
        // a…z, then aa…zz, as Word counts.
        if n == 0 {
            return String::new();
        }
        let c = char::from(b'a' + ((n - 1) % 26) as u8);
        std::iter::repeat_n(c, ((n - 1) / 26 + 1) as usize).collect()
    }
    fn kanji(n: u32) -> String {
        let digits = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
        if n == 0 {
            return "〇".into();
        }
        let mut s = String::new();
        let (th, h, t, o) = (n / 1000 % 10, n / 100 % 10, n / 10 % 10, n % 10);
        for (d, unit) in [(th, "千"), (h, "百"), (t, "十")] {
            if d > 0 {
                if d > 1 {
                    s.push(digits[d as usize]);
                }
                s.push_str(unit);
            }
        }
        if o > 0 {
            s.push(digits[o as usize]);
        }
        s
    }
    fn full_width(s: &str) -> String {
        s.chars().map(|c| if c.is_ascii_digit() { char::from_u32(c as u32 - '0' as u32 + 0xff10).unwrap_or(c) } else { c }).collect()
    }
    match format {
        "upperRoman" => roman(n).to_uppercase(),
        "lowerRoman" => roman(n),
        "upperLetter" => letters(n).to_uppercase(),
        "lowerLetter" => letters(n),
        "decimalZero" => format!("{n:02}"),
        "decimalEnclosedCircle" | "decimalEnclosedCircleChinese" if (1..=20).contains(&n) => char::from_u32(0x2460 + n - 1).map(String::from).unwrap_or_default(),
        "decimalEnclosedCircle" | "decimalEnclosedCircleChinese" => n.to_string(),
        "decimalFullWidth" | "decimalFullWidth2" => full_width(&n.to_string()),
        "decimalEnclosedParen" => format!("({n})"),
        "decimalEnclosedFullstop" => format!("{n}."),
        "japaneseCounting" | "chineseCounting" | "chineseCountingThousand" => kanji(n),
        // 이십칠: the Sino-Korean numerals, with no 일 before 십・백・천 (measured on Word's PDF of DNUM).
        "koreanCounting" => {
            let digits = ['영', '일', '이', '삼', '사', '오', '육', '칠', '팔', '구'];
            if n == 0 {
                return "영".into();
            }
            let mut s = String::new();
            for (d, unit) in [(n / 1000 % 10, "천"), (n / 100 % 10, "백"), (n / 10 % 10, "십")] {
                if d > 0 {
                    if d > 1 {
                        s.push(digits[d as usize]);
                    }
                    s.push_str(unit);
                }
            }
            if n % 10 > 0 {
                s.push(digits[(n % 10) as usize]);
            }
            s
        }
        // 이칠: the digits one by one.
        "koreanDigital" => n.to_string().chars().map(|c| ['영', '일', '이', '삼', '사', '오', '육', '칠', '팔', '구'][c.to_digit(10).unwrap_or(0) as usize]).collect(),
        // 스물일곱: the native Korean numerals.
        "koreanLegal" => {
            const ONES: [&str; 10] = ["", "하나", "둘", "셋", "넷", "다섯", "여섯", "일곱", "여덟", "아홉"];
            const TENS: [&str; 10] = ["", "열", "스물", "서른", "마흔", "쉰", "예순", "일흔", "여든", "아흔"];
            if n == 0 || n >= 100 {
                return n.to_string();
            }
            format!("{}{}", TENS[(n / 10) as usize], ONES[(n % 10) as usize])
        }
        // 贰拾柒 / 貳拾柒: the legal digits, with the units (a digit before 拾 and 佰 always).
        "chineseLegalSimplified" | "ideographLegalTraditional" => {
            let d: [char; 10] = if format == "chineseLegalSimplified" { ['零', '壹', '贰', '叁', '肆', '伍', '陆', '柒', '捌', '玖'] } else { ['零', '壹', '貳', '參', '肆', '伍', '陸', '柒', '捌', '玖'] };
            let mut s = String::new();
            for (v, unit) in [(n / 1000 % 10, '仟'), (n / 100 % 10, '佰'), (n / 10 % 10, '拾')] {
                if v > 0 {
                    s.push(d[v as usize]);
                    s.push(unit);
                }
            }
            if n % 10 > 0 || s.is_empty() {
                s.push(d[(n % 10) as usize]);
            }
            s
        }
        // 庚寅: the stem and the branch of the sexagenary cycle.
        "ideographZodiacTraditional" => {
            let i = (n.max(1) - 1) as usize;
            format!("{}{}", "甲乙丙丁戊己庚辛壬癸".chars().nth(i % 10).unwrap_or('甲'), "子丑寅卯辰巳午未申酉戌亥".chars().nth(i % 12).unwrap_or('子'))
        }
        "hindiNumbers" => n.to_string().chars().map(|c| ['०', '१', '२', '३', '४', '५', '६', '७', '८', '९'][c.to_digit(10).unwrap_or(0) as usize]).collect(),
        "aiueoFullWidth" | "aiueo" => {
            const A: &str = "アイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワヲン";
            A.chars().nth(((n.max(1) - 1) % 46) as usize).map(String::from).unwrap_or_default()
        }
        "iroha" | "irohaFullWidth" => {
            const I: &str = "イロハニホヘトチリヌルヲワカヨタレソツネナラムウヰノオクヤマケフコエテアサキユメミシヱヒモセス";
            I.chars().nth(((n.max(1) - 1) % 47) as usize).map(String::from).unwrap_or_default()
        }
        "none" | "bullet" => String::new(),
        "hex" => format!("{n:X}"),
        "ordinal" => {
            let suffix = match (n % 10, n % 100) {
                (_, 11..=13) => "th",
                (1, _) => "st",
                (2, _) => "nd",
                (3, _) => "rd",
                _ => "th",
            };
            format!("{n}{suffix}")
        }
        "cardinalText" | "ordinalText" => {
            const ONES: [&str; 20] = ["Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten", "Eleven", "Twelve", "Thirteen", "Fourteen", "Fifteen", "Sixteen", "Seventeen", "Eighteen", "Nineteen"];
            const ORD: [&str; 20] = ["Zeroth", "First", "Second", "Third", "Fourth", "Fifth", "Sixth", "Seventh", "Eighth", "Ninth", "Tenth", "Eleventh", "Twelfth", "Thirteenth", "Fourteenth", "Fifteenth", "Sixteenth", "Seventeenth", "Eighteenth", "Nineteenth"];
            let table = if format == "ordinalText" { &ORD } else { &ONES };
            table.get(n as usize).map_or_else(|| n.to_string(), |s| s.to_string())
        }
        "chicago" => {
            const SYMBOLS: [char; 4] = ['*', '†', '‡', '§'];
            let i = (n.max(1) - 1) as usize;
            std::iter::repeat_n(SYMBOLS[i % 4], i / 4 + 1).collect()
        }
        "ideographDigital" | "japaneseDigitalTenThousand" => n.to_string().chars().map(|c| ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'][c.to_digit(10).unwrap_or(0) as usize]).collect(),
        "japaneseLegal" => {
            // 壱弐参 for the digits, with the units.
            let d = ['〇', '壱', '弐', '参', '四', '伍', '六', '七', '八', '九'];
            let (h, t, o) = (n / 100 % 10, n / 10 % 10, n % 10);
            let mut s = String::new();
            if h > 0 {
                s.push(d[h as usize]);
                s.push('百');
            }
            if t > 0 {
                s.push(d[t as usize]);
                s.push('拾');
            }
            if o > 0 || s.is_empty() {
                s.push(d[o as usize]);
            }
            s
        }
        "ideographEnclosedCircle" if (1..=10).contains(&n) => char::from_u32(0x3280 + n - 1).map(String::from).unwrap_or_default(),
        "ideographTraditional" => "甲乙丙丁戊己庚辛壬癸".chars().nth(((n.max(1) - 1) % 10) as usize).map(String::from).unwrap_or_default(),
        "ideographZodiac" => "子丑寅卯辰巳午未申酉戌亥".chars().nth(((n.max(1) - 1) % 12) as usize).map(String::from).unwrap_or_default(),
        "decimalHalfWidth" => n.to_string(),
        "numberInDash" => format!("- {n} -"),
        "russianLower" | "russianUpper" => {
            const R: &str = "абвгдежзиклмнопрстуфхцчшщэюя";
            let c = R.chars().nth(((n.max(1) - 1) % 28) as usize).unwrap_or('а');
            let times = ((n.max(1) - 1) / 28 + 1) as usize;
            let s: String = std::iter::repeat_n(c, times).collect();
            if format == "russianUpper" { s.to_uppercase() } else { s }
        }
        "ganada" => "가나다라마바사아자차카타파하".chars().nth(((n.max(1) - 1) % 14) as usize).map(String::from).unwrap_or_default(),
        "chosung" => "ㄱㄴㄷㄹㅁㅂㅅㅇㅈㅊㅋㅌㅍㅎ".chars().nth(((n.max(1) - 1) % 14) as usize).map(String::from).unwrap_or_default(),
        _ => n.to_string(),
    }
}

/// Format a page number according to Word's page number format rules.
/// Word renders `cardinalText` and `ordinalText` in lowercase for page numbers
/// (e.g. "five", "fifth"), whereas list numbering renders them in title case (e.g. "Five", "Fifth").
pub fn format_page_number(n: u32, format: &str) -> String {
    let s = format_list_number(n, format);
    if format == "cardinalText" || format == "ordinalText" {
        s.to_lowercase()
    } else {
        s
    }
}

/// The labels of a document's list items, by paragraph id, counted in
/// reading order (the body with its tables, then header and footer).
pub fn list_labels(doc: &FlowDocument) -> std::collections::HashMap<u32, String> {
    use std::collections::HashMap;
    let numbering = &doc.styles.numbering;
    // Counters per list definition (or per list, when it restarts levels).
    let mut counters: HashMap<(u32, bool), [u32; 9]> = HashMap::new();
    let mut out = HashMap::new();
    fn walk(blocks: &[Block], numbering: &Numbering, counters: &mut HashMap<(u32, bool), [u32; 9]>, out: &mut HashMap<u32, String>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    let Some(list) = p.list else { continue };
                    let Some(num) = numbering.nums.get(&list.num_id) else { continue };
                    let Some(abs) = numbering.abstracts.get(&num.abstract_id) else { continue };
                    let key = if num.start_overrides.is_empty() { (num.abstract_id, false) } else { (list.num_id, true) };
                    let level = (list.level as usize).min(8);
                    let start = |l: usize| num.start_overrides.get(&(l as u32)).copied().unwrap_or_else(|| abs.levels.get(l).map_or(1, |x| x.start));
                    let c = counters.entry(key).or_insert([0; 9]);
                    c[level] = if c[level] == 0 { start(level) } else { c[level] + 1 };
                    for deeper in c.iter_mut().skip(level + 1) {
                        *deeper = 0;
                    }
                    let Some(lvl) = abs.levels.get(level) else { continue };
                    let mut text = String::new();
                    let mut chars = lvl.text.chars().peekable();
                    while let Some(ch) = chars.next() {
                        if ch == '%'
                            && let Some(d) = chars.peek().and_then(|d| d.to_digit(10))
                        {
                            chars.next();
                            let l = (d as usize).saturating_sub(1).min(8);
                            let n = if c[l] == 0 { start(l) } else { c[l] };
                            let format = abs.levels.get(l).map_or("decimal", |x| x.format.as_str());
                            text.push_str(&format_list_number(n, format));
                        } else {
                            text.push(ch);
                        }
                    }
                    out.insert(p.id, text);
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            // The cell a merge goes on in is not shown: Word does not count its paragraphs.
                            if cell.merge == VerticalMerge::Continue {
                                continue;
                            }
                            walk(&cell.blocks, numbering, counters, out);
                        }
                    }
                }
            }
        }
    }
    walk(&doc.body, numbering, &mut counters, &mut out);
    walk(&doc.header, numbering, &mut counters, &mut out);
    walk(&doc.footer, numbering, &mut counters, &mut out);
    out
}

impl FlowDocument {
    /// What `CREATEDATE` field `instruction` shows.
    pub fn created_date_text(&self, instruction: &str) -> Option<String> {
        let t = self
            .properties
            .created
            .as_deref()
            .and_then(crate::date_field::parse_iso8601)
            .unwrap_or(crate::date_field::LocalTime { year: 2026, month: 10, day: 8, hour: 0, minute: 0, second: 0, weekday: 4 });
        crate::date_field::date_field_text(instruction, &t)
    }

    /// What `SAVEDATE` field `instruction` shows.
    pub fn saved_date_text(&self, instruction: &str) -> Option<String> {
        let t = self
            .properties
            .modified
            .as_deref()
            .and_then(crate::date_field::parse_iso8601)
            .unwrap_or(crate::date_field::LocalTime { year: 2026, month: 10, day: 8, hour: 0, minute: 0, second: 0, weekday: 4 });
        crate::date_field::date_field_text(instruction, &t)
    }

    /// What `PRINTDATE` field `instruction` shows.
    pub fn print_date_text(&self, instruction: &str) -> Option<String> {
        let t = self
            .properties
            .last_printed
            .as_deref()
            .and_then(crate::date_field::parse_iso8601)
            .unwrap_or(crate::date_field::LocalTime { year: 2026, month: 10, day: 8, hour: 0, minute: 0, second: 0, weekday: 4 });
        crate::date_field::date_field_text(instruction, &t)
    }

    /// What `DOCPROPERTY` field `instruction` shows.
    pub fn doc_property_text(&self, instruction: &str) -> Option<String> {
        let trimmed = instruction.trim();
        let words = crate::flow_command::field_words(trimmed);
        let prop_name = words.get(1).copied().unwrap_or("");
        let name_upper = prop_name.trim_matches('"').to_ascii_uppercase();
        let val = match name_upper.as_str() {
            "TITLE" => self.properties.title.clone().unwrap_or_default(),
            "SUBJECT" => self.properties.subject.clone().unwrap_or_default(),
            "AUTHOR" => self.properties.creator.clone().unwrap_or_else(|| "Author".to_string()),
            "KEYWORDS" => self.properties.keywords.clone().unwrap_or_default(),
            "COMMENTS" => self.properties.description.clone().unwrap_or_default(),
            "LASTSAVEDBY" => self.properties.last_modified_by.clone().unwrap_or_default(),
            "REVISION" => self.properties.revision.clone().unwrap_or_else(|| "1".to_string()),
            "CATEGORY" => self.properties.category.clone().unwrap_or_default(),
            "COMPANY" => self.properties.company.clone().unwrap_or_default(),
            "MANAGER" => self.properties.manager.clone().unwrap_or_default(),
            "CREATETIME" | "CREATED" => return self.created_date_text(instruction).or_else(|| self.properties.created.clone()),
            "LASTSAVETIME" | "LASTSAVED" | "SAVED" => return self.saved_date_text(instruction).or_else(|| self.properties.modified.clone()),
            "LASTPRINTDATE" => return self.print_date_text(instruction).or_else(|| self.properties.last_printed.clone()),
            "PAGES" => "1".to_string(),
            "WORDS" => self.word_count().to_string(),
            "CHARACTERS" => self.character_count().to_string(),
            _ => {
                let raw_name = prop_name.trim_matches('"');
                self.properties
                    .custom
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(raw_name) || p.name == raw_name)
                    .map(|p| match &p.value {
                        CustomPropertyValue::Text(s) => s.clone(),
                        CustomPropertyValue::Number(s) => s.clone(),
                        CustomPropertyValue::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
                        CustomPropertyValue::DateTime(s) => s.clone(),
                    })
                    .unwrap_or_default()
            }
        };
        Some(val)
    }

    pub fn author_text(&self, instruction: &str) -> String {
        let words = crate::flow_command::field_words(instruction);
        if let Some(arg) = words.get(1) {
            if !arg.starts_with('\\') {
                return arg.trim_matches('"').to_string();
            }
        }
        self.properties.creator.clone().unwrap_or_else(|| "Author".to_string())
    }

    pub fn title_text(&self, instruction: &str) -> String {
        let words = crate::flow_command::field_words(instruction);
        if let Some(arg) = words.get(1) {
            if !arg.starts_with('\\') {
                return arg.trim_matches('"').to_string();
            }
        }
        self.properties.title.clone().unwrap_or_else(|| "Title".to_string())
    }

    pub fn subject_text(&self, instruction: &str) -> String {
        let words = crate::flow_command::field_words(instruction);
        if let Some(arg) = words.get(1) {
            if !arg.starts_with('\\') {
                return arg.trim_matches('"').to_string();
            }
        }
        self.properties.subject.clone().unwrap_or_default()
    }

    pub fn comments_text(&self, instruction: &str) -> String {
        let words = crate::flow_command::field_words(instruction);
        if let Some(arg) = words.get(1) {
            if !arg.starts_with('\\') {
                return arg.trim_matches('"').to_string();
            }
        }
        self.properties.description.clone().unwrap_or_default()
    }

    pub fn file_size_text(&self, instruction: &str) -> String {
        crate::field_format::format_filesize(self.file_size.unwrap_or(0), instruction)
    }

    pub fn doc_variable_text(&self, instruction: &str) -> String {
        crate::field_format::format_doc_variable(instruction, &self.doc_variables)
    }

    pub fn user_name_text(&self, instruction: &str) -> String {
        let name = self.properties.creator.as_deref().unwrap_or("ibisOffice");
        crate::field_format::format_user_name(name, instruction)
    }

    pub fn user_initials_text(&self, instruction: &str) -> String {
        let name = self.properties.creator.as_deref().unwrap_or("ibisOffice");
        let inits = crate::field_format::initials_from_name(name);
        crate::field_format::format_user_initials(&inits, instruction)
    }

    pub fn user_address_text(&self, instruction: &str) -> String {
        crate::field_format::format_user_address("", instruction)
    }

    pub fn quote_text(&self, instruction: &str) -> String {
        crate::field_format::format_quote(instruction)
    }

    pub fn symbol_text(&self, instruction: &str) -> String {
        crate::field_format::format_symbol(instruction)
    }

    /// What a `TEMPLATE` field shows for this document.
    pub fn template_text(&self, instruction: &str) -> String {
        crate::field_format::format_template_field(instruction, self.attached_template.as_deref())
    }

    /// What a `HYPERLINK` field shows for this document.
    pub fn hyperlink_text(&self, instruction: &str, current_shown: &str) -> String {
        crate::field_format::format_hyperlink_field(instruction, current_shown)
    }

    /// What an `ADVANCE` field shows for this document (always empty string).
    pub fn advance_text(&self, _instruction: &str) -> String {
        String::new()
    }

    /// What an `ASK` field shows for this document (always empty string).
    pub fn ask_text(&self, _instruction: &str) -> String {
        String::new()
    }

    /// What a `FILLIN` / `FILL-IN` field shows for this document (ECMA-376 Part 1 §17.16.5.24).
    pub fn fillin_text(&self, instruction: &str) -> String {
        crate::field_format::format_fillin(instruction)
    }

    /// What the document-information fields `KEYWORDS`, `LASTSAVEDBY`, `NUMCHARS`, `NUMWORDS` and `REVNUM` show (ECMA-376
    /// Part 1 §17.16.5.35, .36, .44, .45, .53): what `INFO` shows for the same property. `None` for another field.
    pub fn bare_info_text(&self, instruction: &str) -> Option<String> {
        let words = crate::flow_command::field_words(instruction);
        let first = words.first()?.to_ascii_uppercase();
        let property = match first.as_str() {
            "KEYWORDS" | "LASTSAVEDBY" | "NUMCHARS" | "NUMWORDS" => first.as_str(),
            "REVNUM" => "REVISION",
            _ => return None,
        };
        self.info_text(&format!("INFO {property} {}", words[1..].join(" ")))
    }

    /// What an `INFO` field shows for this document (ECMA-376 Part 1 §17.16.5.30).
    pub fn info_text(&self, instruction: &str) -> Option<String> {
        let prop = crate::field_format::parse_info_field(instruction)?;
        let prop_upper = prop.to_ascii_uppercase();
        let words = crate::flow_command::field_words(instruction);
        let new_value = words.get(2).and_then(|w| if !w.starts_with('\\') { Some(w.trim_matches('"').to_string()) } else { None });
        let val = if let Some(nv) = new_value {
            nv
        } else {
            match prop_upper.as_str() {
                "AUTHOR" => self.properties.creator.clone().unwrap_or_else(|| "Author".to_string()),
                "TITLE" => self.properties.title.clone().unwrap_or_else(|| "Title".to_string()),
                "SUBJECT" => self.properties.subject.clone().unwrap_or_default(),
                "COMMENTS" => self.properties.description.clone().unwrap_or_default(),
                "KEYWORDS" => self.properties.keywords.clone().unwrap_or_default(),
                "LASTSAVEDBY" => self.properties.last_modified_by.clone().unwrap_or_default(),
                "REVISION" => self.properties.revision.clone().unwrap_or_else(|| "1".to_string()),
                "CATEGORY" => self.properties.category.clone().unwrap_or_default(),
                "NUMPAGES" | "PAGES" => "1".to_string(),
                "NUMWORDS" | "WORDS" => self.word_count().to_string(),
                "NUMCHARS" | "CHARACTERS" => self.character_count().to_string(),
                "CREATEDATE" | "CREATED" => self.created_date_text(instruction).unwrap_or_else(|| "2026/10/8".to_string()),
                "SAVEDATE" | "SAVED" => self.saved_date_text(instruction).unwrap_or_else(|| "2026/10/8".to_string()),
                "PRINTDATE" => self.print_date_text(instruction).unwrap_or_else(|| "2026/10/8".to_string()),
                "FILENAME" => self.file_name.clone().unwrap_or_else(|| "Document.docx".to_string()),
                "FILESIZE" => self.file_size_text(instruction),
                "TEMPLATE" => self.template_text(instruction),
                _ => self.doc_property_text(&format!("DOCPROPERTY {prop} {instruction}")).unwrap_or_default(),
            }
        };
        Some(crate::field_format::apply_instruction_text_switches(val, instruction))
    }

    /// Count words across all paragraphs in the document body.
    pub fn word_count(&self) -> usize {
        let mut count = 0;
        for b in &self.body {
            if let crate::Block::Paragraph(p) = b {
                let text = p.text();
                count += text.split_whitespace().count();
            }
        }
        count
    }

    /// Count characters across all paragraphs in the document body.
    pub fn character_count(&self) -> usize {
        let mut count = 0;
        for b in &self.body {
            if let crate::Block::Paragraph(p) = b {
                count += p.text().chars().count();
            }
        }
        count
    }

    /// Resolve a `NOTEREF` field instruction to its referenced footnote or endnote number.
    /// ECMA-376 Part 1 §17.16.5.42: `{ NOTEREF bookmark [\f] [\h] [\p] }`
    pub fn noteref_text(&self, instruction: &str) -> Option<String> {
        let words = crate::flow_command::field_words(instruction);
        let first = words.first()?.to_ascii_uppercase();
        if first != "NOTEREF" {
            return None;
        }
        let bookmark = words.get(1)?.trim_matches('"');
        let has_p = words.iter().any(|&w| w.eq_ignore_ascii_case("\\p"));

        if has_p {
            // Relative position switch: defaults to "上" / "above"
            return Some("上".to_string());
        }

        // 1. Search in footnotes
        for (i, note) in self.footnotes.iter().enumerate() {
            let note_num = self.page.footnotes.start.saturating_add(i as u32);
            for b in &note.blocks {
                if let crate::Block::Paragraph(p) = b {
                    if p.bookmarks.iter().any(|bm| bm.eq_ignore_ascii_case(bookmark)) {
                        return Some(self.page.footnotes.format.number(note_num));
                    }
                }
            }
        }

        // 2. Search in endnotes
        for (i, note) in self.endnotes.iter().enumerate() {
            let note_num = self.page.endnotes.start.saturating_add(i as u32);
            for b in &note.blocks {
                if let crate::Block::Paragraph(p) = b {
                    if p.bookmarks.iter().any(|bm| bm.eq_ignore_ascii_case(bookmark)) {
                        return Some(self.page.endnotes.format.number(note_num));
                    }
                }
            }
        }

        // 3. Search in body paragraphs for the bookmark and check for NoteRef runs
        for b in &self.body {
            if let crate::Block::Paragraph(p) = b {
                if p.bookmarks.iter().any(|bm| bm.eq_ignore_ascii_case(bookmark)) {
                    // Check if this paragraph contains a NoteRef run
                    for r in &p.runs {
                        if let FlowRunKind::NoteRef { endnote, id } = &r.kind {
                            if *endnote {
                                if let Some(idx) = self.endnotes.iter().position(|n| n.id == *id) {
                                    let num = self.page.endnotes.start.saturating_add(idx as u32);
                                    return Some(self.page.endnotes.format.number(num));
                                }
                            } else {
                                if let Some(idx) = self.footnotes.iter().position(|n| n.id == *id) {
                                    let num = self.page.footnotes.start.saturating_add(idx as u32);
                                    return Some(self.page.footnotes.format.number(num));
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4. Fallback: if bookmark matches a note number or reference pattern, or default to first note
        if !self.footnotes.is_empty() {
            Some(self.page.footnotes.format.number(self.page.footnotes.start))
        } else if !self.endnotes.is_empty() {
            Some(self.page.endnotes.format.number(self.page.endnotes.start))
        } else {
            Some("1".to_string())
        }
    }

    /// Resolve a `STYLEREF` field instruction to the text or paragraph number
    /// of the nearest paragraph formatted with the specified style.
    /// ECMA-376 Part 1 §17.16.5.56: `{ STYLEREF style-identifier [\l] [\n] [\p] [\r] [\t] [\w] }`
    pub fn styleref_text(&self, instruction: &str) -> Option<String> {
        self.styleref_text_at(instruction, None)
    }

    /// [`FlowDocument::styleref_text`] for a field in paragraph `anchor` of the body: the nearest paragraph of the style
    /// before it (itself included), else after it (Word's STYLEREF in the body text).
    pub fn styleref_text_at(&self, instruction: &str, anchor: Option<u32>) -> Option<String> {
        let mut words = Vec::new();
        let mut chars = instruction.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
                continue;
            }
            if c == '"' {
                chars.next();
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    chars.next();
                    if ch == '"' {
                        break;
                    }
                    s.push(ch);
                }
                words.push(s);
            } else {
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_whitespace() {
                        break;
                    }
                    chars.next();
                    s.push(ch);
                }
                words.push(s);
            }
        }
        let first = words.first()?.to_ascii_uppercase();
        if first != "STYLEREF" {
            return None;
        }
        let target_style = words.get(1)?.as_str();
        let has_l = words.iter().any(|w| w.eq_ignore_ascii_case("\\l"));
        let has_r = words.iter().any(|w| w.eq_ignore_ascii_case("\\r"));
        let has_n = words.iter().any(|w| w.eq_ignore_ascii_case("\\n") || w.eq_ignore_ascii_case("\\s"));
        let has_w = words.iter().any(|w| w.eq_ignore_ascii_case("\\w"));
        let has_p = words.iter().any(|w| w.eq_ignore_ascii_case("\\p"));

        if has_p {
            return Some("上".to_string());
        }

        let norm = |s: &str| s.chars().filter(|c| !c.is_whitespace() && *c != '_' && *c != '-').collect::<String>().to_lowercase();
        let target_norm = norm(target_style);

        let matches_style = |p_style: &str| -> bool {
            if p_style.eq_ignore_ascii_case(target_style) {
                return true;
            }
            let p_norm = norm(p_style);
            if p_norm == target_norm {
                return true;
            }
            for s in self.styles.chain(p_style) {
                if s.name.eq_ignore_ascii_case(target_style) {
                    return true;
                }
                let s_norm = norm(&s.name);
                if s_norm == target_norm {
                    return true;
                }
                if target_norm == "1" && (s_norm == "heading1" || s_norm == "見出し1") {
                    return true;
                }
                if target_norm == "2" && (s_norm == "heading2" || s_norm == "見出し2") {
                    return true;
                }
                if target_norm == "3" && (s_norm == "heading3" || s_norm == "見出し3") {
                    return true;
                }
                if (target_norm == "heading1" && s_norm == "見出し1") || (target_norm == "見出し1" && s_norm == "heading1") {
                    return true;
                }
                if (target_norm == "heading2" && s_norm == "見出し2") || (target_norm == "見出し2" && s_norm == "heading2") {
                    return true;
                }
                if (target_norm == "heading3" && s_norm == "見出し3") || (target_norm == "見出し3" && s_norm == "heading3") {
                    return true;
                }
                if (target_norm == "title" && s_norm == "タイトル") || (target_norm == "タイトル" && s_norm == "title") {
                    return true;
                }
            }
            if target_norm == "1" && (p_norm == "heading1" || p_norm == "見出し1") {
                return true;
            }
            if target_norm == "2" && (p_norm == "heading2" || p_norm == "見出し2") {
                return true;
            }
            if target_norm == "3" && (p_norm == "heading3" || p_norm == "見出し3") {
                return true;
            }
            if (target_norm == "heading1" && p_norm == "見出し1") || (target_norm == "見出し1" && p_norm == "heading1") {
                return true;
            }
            if (target_norm == "heading2" && p_norm == "見出し2") || (target_norm == "見出し2" && p_norm == "heading2") {
                return true;
            }
            if (target_norm == "heading3" && p_norm == "見出し3") || (target_norm == "見出し3" && p_norm == "heading3") {
                return true;
            }
            if (target_norm == "title" && p_norm == "タイトル") || (target_norm == "タイトル" && p_norm == "title") {
                return true;
            }
            false
        };

        fn collect_blocks<'a>(blocks: &'a [crate::Block], rev: bool, ps: &mut Vec<&'a FlowParagraph>) {
            let iter: Box<dyn Iterator<Item = &'a crate::Block>> = if rev {
                Box::new(blocks.iter().rev())
            } else {
                Box::new(blocks.iter())
            };
            for b in iter {
                match b {
                    crate::Block::Paragraph(p) => ps.push(p),
                    crate::Block::Table(t) => {
                        let rows: Box<dyn Iterator<Item = &FlowRow>> = if rev { Box::new(t.rows.iter().rev()) } else { Box::new(t.rows.iter()) };
                        for row in rows {
                            let cells: Box<dyn Iterator<Item = &FlowCell>> = if rev { Box::new(row.cells.iter().rev()) } else { Box::new(row.cells.iter()) };
                            for cell in cells {
                                collect_blocks(&cell.blocks, rev, ps);
                            }
                        }
                    }
                }
            }
        }

        let mut ps: Vec<&FlowParagraph> = Vec::new();
        collect_blocks(&self.body, has_l, &mut ps);
        if let Some(anchor) = anchor.filter(|_| !has_l) && let Some(at) = ps.iter().position(|p| p.id == anchor) {
            // From the field's paragraph back to the start, then on to the end.
            let mut order: Vec<&FlowParagraph> = ps[..=at].iter().rev().copied().collect();
            order.extend(ps[at + 1..].iter().copied());
            ps = order;
        }

        for p in ps {
            let matched = p.style.as_deref().is_some_and(matches_style);
            if matched {
                if has_r || has_n || has_w {
                    let labels = crate::flow::list_labels(self);
                    if let Some(lbl) = labels.get(&p.id) {
                        if has_n {
                            return Some(lbl.trim_end_matches('.').to_string());
                        } else {
                            return Some(lbl.clone());
                        }
                    }
                }
                let text = p.text();
                return Some(text);
            }
        }

        Some(String::new())
    }
}


/// The prompt a text content control shows when it has no content and its
/// document does not give one (`w:placeholder`'s building block is not read).
pub const DEFAULT_CONTROL_PROMPT: &str = "クリックまたはタップしてテキストを入力してください。"; // i18n-ok（Word の既定の案内文）

/// Word 標準の画像コンテンツ コントロールの初期プレースホルダー PNG 画像 (120×90 ピクセル、4:3 比率)。
pub const DEFAULT_PICTURE_CONTROL_PLACEHOLDER_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x78, 0x00, 0x00, 0x00, 0x5a, 0x08, 0x02, 0x00, 0x00, 0x00, 0xfc, 0x62, 0x05,
    0xb8, 0x00, 0x00, 0x01, 0x3d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0xed, 0xdb, 0x31, 0x0e, 0xc2,
    0x20, 0x18, 0x80, 0xd1, 0xde, 0x7f, 0xe8, 0xe2, 0x3d, 0x3a, 0x38, 0x79, 0x8d, 0x2e, 0xde, 0xa0,
    0x0e, 0x9d, 0x1b, 0x67, 0x9b, 0x34, 0x71, 0xd0, 0x68, 0x0a, 0x08, 0xa2, 0xbc, 0xe4, 0x73, 0x75,
    0x78, 0x31, 0x04, 0x7e, 0xa4, 0x1b, 0xc7, 0xb3, 0x0a, 0xd4, 0xad, 0x9f, 0x65, 0xb9, 0x2a, 0x6b,
    0xa0, 0x41, 0x83, 0x16, 0x68, 0xd0, 0xa0, 0x41, 0x83, 0x06, 0x2d, 0xd0, 0xa0, 0x41, 0x83, 0x06,
    0x0d, 0x5a, 0xa0, 0x41, 0x83, 0x06, 0x0d, 0x1a, 0xb4, 0x40, 0x83, 0x06, 0x0d, 0x1a, 0x34, 0x68,
    0xd5, 0x0c, 0xdd, 0xf7, 0x87, 0x3a, 0xfb, 0x43, 0xe8, 0x69, 0x9a, 0x6b, 0x0b, 0x34, 0x68, 0xd0,
    0xa0, 0x41, 0x83, 0x06, 0x0d, 0x1a, 0x74, 0x16, 0xe8, 0xcb, 0xa9, 0xdb, 0x02, 0x9d, 0x11, 0xfa,
    0xae, 0x5c, 0xde, 0xba, 0x21, 0xe8, 0x07, 0xe5, 0xc2, 0xd6, 0xa0, 0x41, 0x83, 0xb6, 0x46, 0x83,
    0xb6, 0xeb, 0xb0, 0x8f, 0x06, 0x0d, 0x1a, 0x34, 0x68, 0xd0, 0xa0, 0x7f, 0xe3, 0xce, 0x70, 0x18,
    0x8e, 0x99, 0x02, 0x5d, 0xc2, 0xda, 0x2f, 0xba, 0x84, 0xb5, 0xa5, 0xa3, 0x84, 0xb5, 0x35, 0x1a,
    0x74, 0x35, 0x7f, 0xa0, 0xf9, 0xa0, 0x32, 0xe8, 0x8c, 0xd6, 0xad, 0xec, 0xa3, 0xbf, 0x6b, 0xdd,
    0xd6, 0x81, 0x05, 0x74, 0xb9, 0x93, 0x61, 0x3a, 0xf4, 0xfa, 0x25, 0xe9, 0xca, 0x4d, 0x1c, 0xc1,
    0x13, 0x95, 0xb7, 0x12, 0x95, 0x5b, 0x99, 0x75, 0x24, 0x2a, 0xef, 0xb7, 0x36, 0x54, 0x9a, 0x13,
    0x95, 0x41, 0x07, 0x4c, 0xef, 0x52, 0x94, 0xf7, 0x58, 0x1b, 0x93, 0x86, 0x59, 0xbf, 0xf7, 0x8a,
    0x53, 0x6e, 0x71, 0x1e, 0x9d, 0xa2, 0xfc, 0xca, 0xda, 0xe0, 0x3f, 0xcc, 0x7a, 0xe7, 0x08, 0x1f,
    0x74, 0xc0, 0x0d, 0x4b, 0xb4, 0xf2, 0xb3, 0xb5, 0xab, 0xac, 0x00, 0xeb, 0x88, 0xab, 0xa9, 0x20,
    0xe5, 0xd6, 0xef, 0x0c, 0xa3, 0x95, 0xdd, 0x19, 0x7a, 0x67, 0xe8, 0xe5, 0xac, 0x27, 0xca, 0xa0,
    0x05, 0x1a, 0x34, 0x68, 0xd0, 0xa0, 0x41, 0x0b, 0x34, 0x68, 0xd0, 0xa0, 0x41, 0x83, 0x16, 0x68,
    0xd0, 0xa0, 0x41, 0x83, 0x06, 0x2d, 0xd0, 0xa0, 0x41, 0x83, 0x06, 0x0d, 0x5a, 0x31, 0xd0, 0x2a,
    0xd0, 0x0d, 0x2a, 0x3f, 0x0c, 0xf3, 0xd4, 0x1a, 0x9b, 0xad, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// 画像コンテンツ コントロールの既定の幅 (120 pt = 1,524,000 EMU)。
pub const DEFAULT_PICTURE_CONTROL_WIDTH_EMU: i64 = 1_524_000;
/// 画像コンテンツ コントロールの既定の高さ (90 pt = 1,143,000 EMU)。
pub const DEFAULT_PICTURE_CONTROL_HEIGHT_EMU: i64 = 1_143_000;

/// Word 標準の文書パーツ ギャラリー コンテンツ コントロールの初期プレースホルダー案内文。
pub const DEFAULT_BUILDING_BLOCK_GALLERY_PROMPT: &str = "ここをクリックまたはタップして、文書パーツ ギャラリーの項目を挿入します。"; // i18n-ok（Word の既定の案内文）
/// 文書パーツ ギャラリーの既定のギャラリー名。
pub const DEFAULT_BUILDING_BLOCK_GALLERY_NAME: &str = "Custom";
/// 文書パーツ ギャラリーの既定のカテゴリ名。
pub const DEFAULT_BUILDING_BLOCK_GALLERY_CATEGORY: &str = "General";

/// Word 標準の繰返しセクション コンテンツ コントロールの既定のセクション名。
pub const DEFAULT_REPEATING_SECTION_TITLE: &str = "セクション";
/// Word 標準の繰返しセクション コンテンツ コントロールの初期項目案内文。
pub const DEFAULT_REPEATING_SECTION_ITEM_PROMPT: &str = "ここをクリックまたはタップして、セクションの内容を入力します。"; // i18n-ok（Word の既定の案内文）

/// Word 標準のグループ化コンテンツ コントロールの初期プレースホルダー案内文。
pub const DEFAULT_GROUP_PROMPT: &str = "ここをクリックまたはタップして、グループ化されたテキストを入力します。"; // i18n-ok（Word の既定の案内文）

