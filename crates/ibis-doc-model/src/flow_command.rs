// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Editing commands of flowing documents — the document担当's file.
//!
//! A document is `Presentation::flow`, an `Arc<FlowDocument>` (changed with
//! `Arc::make_mut`); the viewer lays its pages out again after a change.
//! Each command applied returns the command that undoes it, as
//! [`crate::Command`]s do: a [`FlowCommand::ReplaceBlocks`] putting back the
//! blocks it changed.
//!
//! A position is a paragraph (by its id) and an offset in it: characters of
//! its text runs, and one for each tab, break, picture and field
//! ([`FlowParagraph::len`]).

use std::sync::Arc;

use crate::command::{Command, EditError};
use crate::drawing::Rgb;
use crate::flow::{
    Block, FlowAlign, FlowBorder, FlowCell, FlowDocument, FlowParagraph, FlowRow, FlowRun, FlowRunKind, FlowRunStyle, FlowStyles,
    FlowTable, LineHeight, RunFormat, VerticalMerge, VerticalPosition,
};
use crate::geometry::Emu;
use crate::model::{Presentation, ShapeKind};

/// A place in a document's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FlowPos {
    pub para: u32,
    pub offset: usize,
}

impl FlowPos {
    pub fn new(para: u32, offset: usize) -> Self {
        Self { para, offset }
    }
}

/// A list of blocks of a document: the body, the header, the footer or a
/// table cell's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Container {
    Body,
    Header,
    Footer,
    Cell { table: u32, row: usize, cell: usize },
    /// A section's header or footer: of the section paragraph `section`
    /// ends (its own), or of the last section (`section` 0: the first
    /// page's and the even pages'; its default ones are `Header`/`Footer`).
    Part { section: u32, kind: crate::flow::PartKind },
    /// A footnote's or endnote's text (note `id`).
    Note { endnote: bool, id: u32 },
    /// A comment's text.
    Comment(u32),
}

/// The formatting inserted text takes (otherwise that of the text before it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsertFormat {
    pub format: RunFormat,
    pub char_style: Option<String>,
}

/// A change of character formatting (direct formatting, `w:rPr`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunChange {
    /// The language of the text, of its East Asian text, and of its Complex Script/Bidi text (`None`: as it was).
    Language { lang: Option<String>, east_asia: Option<String>, bidi: Option<String> },
    Bold(bool),
    Italic(bool),
    Underline(bool),
    Strike(bool),
    DoubleStrike(bool),
    Caps(bool),
    SmallCaps(bool),
    /// The Latin typeface (`w:ascii`, `w:hAnsi`).
    Font(String),
    /// The East Asian typeface (`w:eastAsia`).
    EastAsianFont(String),
    /// The Complex Script typeface (`w:cs`).
    ComplexScriptFont(String),
    /// Both typefaces (a Japanese font chosen, as Word sets it).
    AllFonts(String),
    /// Hundredths of a point.
    Size(u32),
    /// Complex Script font size in hundredths of a point (`w:szCs`).
    ComplexScriptSize(u32),
    Color(Rgb),
    /// テーマ色による文字色 (`w:color` の `w:themeColor`, `w:themeTint`, `w:themeShade`).
    ThemeColor {
        color: Rgb,
        theme_color: crate::flow::FlowThemeColor,
    },
    Highlight(Option<Rgb>),
    Position(VerticalPosition),
    /// Character spacing in EMU (`w:spacing`), or `None` to reset to standard.
    Spacing(Option<Emu>),
    /// Character horizontal scale in percent (`w:w`), or `None` to reset to standard.
    Scale(Option<u32>),
    /// Fit a contiguous group of characters to (twips, group id).
    FitText(Option<(u32,u32)>),
    /// 傍点 (`w:em`).
    Emphasis(crate::flow::EmphasisMark),
    /// 組み文字 (`w:eastAsianLayout w:combine`), or none.
    Combine(Option<crate::flow::Combine>),
    /// 縦中横 (`w:eastAsianLayout w:vert`).
    TateChuYoko(bool),
    /// 文字の囲み線 (`w:bdr`).
    Border(Option<crate::flow::FlowBorder>),
    /// カーニング (`w:kern`, half-points; Some(0) turns off, None clears direct override).
    Kern(Option<u32>),
    /// 隠し文字 (`w:vanish`).
    Hidden(bool),
    /// 中抜き・輪郭 (`w:outline`).
    Outline(bool),
    /// 影付き (`w:shadow`).
    Shadow(bool),
    /// 浮き出し (`w:emboss`).
    Emboss(bool),
    /// 校正しない (`w:noProof`).
    NoProof(bool),
    /// 浮き彫り (`w:imprint`).
    Imprint(bool),
    /// OpenType 合字 (`w14:ligatures`).
    Ligatures(crate::flow::Ligatures),
    /// OpenType 数字のスタイル (`w14:numForm`).
    NumberForm(crate::flow::NumberForm),
    /// OpenType 数字の間隔 (`w14:numSpacing`).
    NumberSpacing(crate::flow::NumberSpacing),
    /// OpenType 前後関係に依存する異体字 (`w14:cntxtAlts`).
    ContextualAlternates(bool),
    /// Remove the direct formatting (the character style stays).
    Clear,
}

impl RunChange {
    pub fn apply(&self, f: &mut RunFormat) {
        match self {
            RunChange::Language { lang, east_asia, bidi } => {
                if lang.is_some() {
                    f.lang.clone_from(lang);
                }
                if east_asia.is_some() {
                    f.lang_east_asia.clone_from(east_asia);
                }
                if bidi.is_some() {
                    f.lang_bidi.clone_from(bidi);
                }
            }
            RunChange::NoProof(v) => f.no_proof = Some(*v),
            RunChange::Bold(v) => f.bold = Some(*v),
            RunChange::Italic(v) => f.italic = Some(*v),
            RunChange::Underline(v) => f.underline = Some(*v),
            RunChange::Strike(v) => {
                f.strike = Some(*v);
                if *v {
                    f.double_strike = Some(false);
                }
            }
            RunChange::DoubleStrike(v) => {
                f.double_strike = Some(*v);
                if *v {
                    f.strike = Some(false);
                }
            }
            RunChange::Caps(v) => {
                f.caps = Some(*v);
                if *v {
                    f.small_caps = Some(false);
                }
            }
            RunChange::SmallCaps(v) => {
                f.small_caps = Some(*v);
                if *v {
                    f.caps = Some(false);
                }
            }
            RunChange::Font(v) => f.font = Some(v.clone()),
            RunChange::EastAsianFont(v) => f.east_asian_font = Some(v.clone()),
            RunChange::ComplexScriptFont(v) => f.cs_font = Some(v.clone()),
            RunChange::AllFonts(v) => {
                f.font = Some(v.clone());
                f.east_asian_font = Some(v.clone());
            }
            // Word sets the complex scripts' size (w:szCs) with the size.
            RunChange::Size(v) => {
                f.size = Some(*v);
                f.cs_size = Some(*v);
            }
            RunChange::ComplexScriptSize(v) => f.cs_size = Some(*v),
            RunChange::Color(v) => {
                f.color = Some(*v);
                f.theme_color = None;
            }
            RunChange::ThemeColor { color, theme_color } => {
                f.color = Some(*color);
                f.theme_color = Some(theme_color.clone());
            }
            RunChange::Highlight(v) => f.highlight = Some(*v),
            RunChange::Position(v) => f.position = Some(*v),
            RunChange::Spacing(v) => f.spacing = *v,
            RunChange::Scale(v) => f.scale = *v,
            RunChange::FitText(v) => f.fit_text = Some(*v),
            RunChange::Emphasis(v) => f.emphasis = Some(*v),
            RunChange::Combine(v) => f.combine = Some(*v),
            RunChange::TateChuYoko(v) => f.tate_chu_yoko = Some(*v),
            RunChange::Border(v) => f.border = Some(*v),
            RunChange::Kern(v) => f.kern = *v,
            RunChange::Hidden(v) => f.hidden = Some(*v),
            RunChange::Outline(v) => {
                f.outline = Some(*v);
                if *v {
                    f.emboss = Some(false);
                    f.imprint = Some(false);
                }
            }
            RunChange::Shadow(v) => {
                f.shadow = Some(*v);
                if *v {
                    f.emboss = Some(false);
                    f.imprint = Some(false);
                }
            }
            RunChange::Emboss(v) => {
                f.emboss = Some(*v);
                if *v {
                    f.outline = Some(false);
                    f.shadow = Some(false);
                    f.imprint = Some(false);
                }
            }
            RunChange::Imprint(v) => {
                f.imprint = Some(*v);
                if *v {
                    f.outline = Some(false);
                    f.shadow = Some(false);
                    f.emboss = Some(false);
                }
            }
            RunChange::Ligatures(v) => f.ligatures = Some(*v),
            RunChange::NumberForm(v) => f.num_form = Some(*v),
            RunChange::NumberSpacing(v) => f.num_spacing = Some(*v),
            RunChange::ContextualAlternates(v) => f.contextual_alternates = Some(*v),
            RunChange::Clear => *f = RunFormat { hidden: f.hidden, ..RunFormat::default() },
        }
    }
}

/// 文字種の変換 (DOCX-JA-020: 全角半角・大文字小文字・ひらがなカタカナ).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseConversion {
    /// 文の先頭文字を大文字にする (Sentence case)
    Sentence,
    /// すべて小文字にする (lowercase)
    Lower,
    /// すべて大文字にする (UPPERCASE)
    Upper,
    /// 各単語の先頭文字を大文字にする (Capitalize Each Word)
    Title,
    /// 大文字と小文字を切り替える (tOGGLE cASE)
    Toggle,
    /// 半角 (Half-width)
    HalfWidth,
    /// 全角 (Full-width)
    FullWidth,
    /// カタカナ (Katakana: ひらがな → カタカナ)
    Katakana,
    /// ひらがな (Hiragana: カタカナ → ひらがな)
    Hiragana,
}

const KANA_TABLE: &[(char, &str)] = &[
    ('。', "｡"), ('「', "｢"), ('」', "｣"), ('、', "､"), ('・', "･"), ('ヲ', "ｦ"), ('ァ', "ｧ"), ('ィ', "ｨ"), ('ゥ', "ｩ"), ('ェ', "ｪ"), ('ォ', "ｫ"),
    ('ャ', "ｬ"), ('ュ', "ｭ"), ('ョ', "ｮ"), ('ッ', "ｯ"), ('ー', "ｰ"), ('ア', "ｱ"), ('イ', "ｲ"), ('ウ', "ｳ"), ('エ', "ｴ"), ('オ', "ｵ"), ('カ', "ｶ"),
    ('キ', "ｷ"), ('ク', "ｸ"), ('ケ', "ｹ"), ('コ', "ｺ"), ('サ', "ｻ"), ('シ', "ｼ"), ('ス', "ｽ"), ('セ', "ｾ"), ('ソ', "ｿ"), ('タ', "ﾀ"), ('チ', "ﾁ"),
    ('ツ', "ﾂ"), ('テ', "ﾃ"), ('ト', "ﾄ"), ('ナ', "ﾅ"), ('ニ', "ﾆ"), ('ヌ', "ﾇ"), ('ネ', "ﾈ"), ('ノ', "ﾉ"), ('ハ', "ﾊ"), ('ヒ', "ﾋ"), ('フ', "ﾌ"),
    ('ヘ', "ﾍ"), ('ホ', "ﾎ"), ('マ', "ﾏ"), ('ミ', "ﾐ"), ('ム', "ﾑ"), ('メ', "ﾒ"), ('モ', "ﾓ"), ('ヤ', "ﾔ"), ('ユ', "ﾕ"), ('ヨ', "ﾖ"), ('ラ', "ﾗ"),
    ('リ', "ﾘ"), ('ル', "ﾙ"), ('レ', "ﾚ"), ('ロ', "ﾛ"), ('ワ', "ﾜ"), ('ン', "ﾝ"), ('゛', "ﾞ"), ('゜', "ﾟ"), ('ガ', "ｶﾞ"), ('ギ', "ｷﾞ"), ('グ', "ｸﾞ"),
    ('ゲ', "ｹﾞ"), ('ゴ', "ｺﾞ"), ('ザ', "ｻﾞ"), ('ジ', "ｼﾞ"), ('ズ', "ｽﾞ"), ('ゼ', "ｾﾞ"), ('ゾ', "ｿﾞ"), ('ダ', "ﾀﾞ"), ('ヂ', "ﾁﾞ"), ('ヅ', "ﾂﾞ"),
    ('デ', "ﾃﾞ"), ('ド', "ﾄﾞ"), ('バ', "ﾊﾞ"), ('ビ', "ﾋﾞ"), ('ブ', "ﾌﾞ"), ('ベ', "ﾍﾞ"), ('ボ', "ﾎﾞ"), ('パ', "ﾊﾟ"), ('ピ', "ﾋﾟ"), ('プ', "ﾌﾟ"),
    ('ペ', "ﾍﾟ"), ('ポ', "ﾎﾟ"), ('ヴ', "ｳﾞ"),
];

fn to_half_width(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    for ch in t.chars() {
        let c = ch as u32;
        if (0xFF01..=0xFF5E).contains(&c) {
            out.push(char::from_u32(c - 0xFEE0).unwrap());
        } else if ch == '　' {
            out.push(' ');
        } else if ch == '￥' {
            out.push('\\');
        } else if let Some((_, h)) = KANA_TABLE.iter().find(|(f, _)| *f == ch) {
            out.push_str(h);
        } else {
            out.push(ch);
        }
    }
    out
}

fn to_full_width(t: &str) -> String {
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len() * 3);
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let c = ch as u32;
        if i + 1 < chars.len() && matches!(chars[i + 1], 'ﾞ' | 'ﾟ') {
            let pair: String = [ch, chars[i + 1]].iter().collect();
            if let Some((f, _)) = KANA_TABLE.iter().find(|(_, h)| *h == pair) {
                out.push(*f);
                i += 2;
                continue;
            }
        }
        if (0x21..=0x7E).contains(&c) {
            out.push(if ch == '\\' { '￥' } else { char::from_u32(c + 0xFEE0).unwrap() });
        } else if ch == ' ' {
            out.push('　');
        } else if let Some((f, _)) = KANA_TABLE.iter().find(|(_, h)| h.chars().count() == 1 && h.starts_with(ch)) {
            out.push(*f);
        } else {
            out.push(ch);
        }
        i += 1;
    }
    out
}

fn to_katakana(t: &str) -> String {
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len() * 2);
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if i + 1 < chars.len() && matches!(chars[i + 1], 'ﾞ' | 'ﾟ') {
            let pair: String = [ch, chars[i + 1]].iter().collect();
            if let Some((f, _)) = KANA_TABLE.iter().find(|(_, h)| *h == pair) {
                out.push(*f);
                i += 2;
                continue;
            }
        }
        if let Some((f, _)) = KANA_TABLE.iter().find(|(_, h)| h.chars().count() == 1 && h.starts_with(ch)) {
            out.push(*f);
        } else {
            let code = ch as u32;
            if (0x3041..=0x3096).contains(&code) {
                out.push(char::from_u32(code + 0x60).unwrap());
            } else {
                out.push(ch);
            }
        }
        i += 1;
    }
    out
}

fn to_hiragana(t: &str) -> String {
    let kata = to_katakana(t);
    kata.chars().map(|c| {
        let code = c as u32;
        if (0x30A1..=0x30F6).contains(&code) {
            char::from_u32(code - 0x60).unwrap()
        } else {
            c
        }
    }).collect()
}

fn to_lower_case(t: &str) -> String {
    t.chars().flat_map(char::to_lowercase).collect()
}

fn to_upper_case(t: &str) -> String {
    t.chars().flat_map(char::to_uppercase).collect()
}

fn to_toggle_case(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    for c in t.chars() {
        if c.is_uppercase() {
            out.extend(c.to_lowercase());
        } else if c.is_lowercase() {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn to_sentence_case(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut capitalize_next = true;
    for c in t.chars() {
        if capitalize_next && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            capitalize_next = false;
        } else if c.is_alphabetic() {
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
            if matches!(c, '.' | '!' | '?' | '。' | '\n') {
                capitalize_next = true;
            }
        }
    }
    out
}

fn to_title_case(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut start_of_word = true;
    for c in t.chars() {
        if c.is_alphabetic() {
            if start_of_word {
                out.extend(c.to_uppercase());
                start_of_word = false;
            } else {
                out.extend(c.to_lowercase());
            }
        } else {
            out.push(c);
            if c.is_whitespace() || matches!(c, '-' | '/' | '(' | '[' | '{' | '"' | '\'' | '、' | '。' | ',' | '.' | ':' | ';') {
                start_of_word = true;
            }
        }
    }
    out
}

impl CaseConversion {
    pub fn apply(&self, text: &str) -> String {
        match self {
            CaseConversion::Sentence => to_sentence_case(text),
            CaseConversion::Lower => to_lower_case(text),
            CaseConversion::Upper => to_upper_case(text),
            CaseConversion::Title => to_title_case(text),
            CaseConversion::Toggle => to_toggle_case(text),
            CaseConversion::HalfWidth => to_half_width(text),
            CaseConversion::FullWidth => to_full_width(text),
            CaseConversion::Katakana => to_katakana(text),
            CaseConversion::Hiragana => to_hiragana(text),
        }
    }
}


/// A change of paragraph formatting (`w:pPr`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParaChange {
    Align(FlowAlign),
    Line(LineHeight),
    SpaceBefore(Emu),
    SpaceAfter(Emu),
    /// Indents: left, right, and the first line relative to the left
    /// (negative for hanging).
    Left(Emu),
    Right(Emu),
    FirstLine(Emu),
    /// Indents in character units (1/100 of a character, e.g. 100 = 1 char).
    LeftChars(Option<i32>),
    RightChars(Option<i32>),
    FirstLineChars(Option<i32>),
    /// The paragraph style (`None`: the default style). A style the document
    /// does not have is not applied.
    Style(Option<String>),
    /// Make the paragraph an item of list `Some(num_id)` (at its level, or
    /// the first), or no list item (`None`).
    List(Option<u32>),
    /// The list level (0 to 8) of a list item.
    Level(u32),
    /// End a section with the paragraph (or not: `SectionBreak::None`).
    SectionBreak(crate::flow::SectionBreak),
    /// 次の段落と分離しない (`w:keepNext`).
    KeepNext(bool),
    /// 段落を分割しない (`w:keepLines`).
    KeepLines(bool),
    /// 段落前で改ページ (`w:pageBreakBefore`).
    PageBreakBefore(bool),
    /// ブックマーク: a bookmark named so starting at the paragraph (taken
    /// off any other paragraph), or taken off it.
    AddBookmark(String),
    RemoveBookmark(String),
    /// タブ位置の設定・クリア (`w:tabs`).
    Tabs(Vec<crate::flow::TabChange>),
    /// 同じスタイルの段落間にスペースを追加しない (`w:contextualSpacing`).
    ContextualSpacing(bool),
    /// 改行時 1 行残して段落を区切らない（孤立行の制御、`w:widowControl`）.
    WidowControl(bool),
    /// 行番号を表示しない (`w:suppressLineNumbers`).
    SuppressLineNumbers(bool),
    /// 段落単位で自動ハイフネーションを行わない (`w:suppressAutoHyphens`).
    SuppressAutoHyphens(bool),
    /// 句読点のぶら下げを行う (`w:overflowPunct`).
    OverflowPunct(bool),
    /// 日本語と英字の間隔を自動調整する (`w:autoSpaceDE`).
    AutoSpaceDE(bool),
    /// 日本語と数字の間隔を自動調整する (`w:autoSpaceDN`).
    AutoSpaceDN(bool),
    /// 禁則処理を行う (`w:kinsoku`).
    Kinsoku(bool),
    /// 単語の途中で改行しない (`w:wordWrap`).
    WordWrap(bool),
    /// 右から左へ記述する（段落の方向、`w:bidi`）.
    Bidi(bool),
    /// 行グリッド線に合わせる (`w:snapToGrid`).
    SnapToGrid(bool),
    /// ドロップ キャップ (`w:framePr w:dropCap="drop|margin"`).
    DropCap(Option<crate::flow::DropCap>),
    /// 見出し段落の折りたたみ (`w15:collapsed`).
    Collapsed(bool),
}

impl ParaChange {
    fn apply(&self, p: &mut FlowParagraph, styles: &FlowStyles) {
        let d = &mut p.direct;
        match self {
            ParaChange::Align(v) => d.align = Some(*v),
            ParaChange::Line(v) => d.line = Some(*v),
            ParaChange::SpaceBefore(v) => d.before = Some(*v),
            ParaChange::SpaceAfter(v) => d.after = Some(*v),
            ParaChange::Left(v) => d.left = Some(*v),
            ParaChange::Right(v) => d.right = Some(*v),
            ParaChange::FirstLine(v) => d.first_line = Some(*v),
            ParaChange::LeftChars(v) => d.left_chars = *v,
            ParaChange::RightChars(v) => d.right_chars = *v,
            ParaChange::FirstLineChars(v) => d.first_line_chars = *v,
            ParaChange::KeepNext(v) => d.keep_next = Some(*v),
            ParaChange::KeepLines(v) => d.keep_lines = Some(*v),
            ParaChange::PageBreakBefore(v) => d.page_break_before = Some(*v),
            ParaChange::Style(id) => {
                if id.as_ref().is_none_or(|id| styles.styles.contains_key(id)) {
                    p.style = id.clone();
                }
            }
            ParaChange::List(Some(id)) => {
                d.num_id = Some(*id);
                d.level = Some(p.list.map_or(0, |l| l.level));
            }
            ParaChange::List(None) => {
                // A style that makes the paragraph a list item is undone
                // with list 0, as Word does.
                let (base, _) = styles.paragraph_base_in(p.style.as_deref(), p.table_style.as_deref(), &p.table_regions);
                let from_style = base.num_id.is_some_and(|n| n != 0);
                d.num_id = from_style.then_some(0);
                d.level = None;
            }
            ParaChange::SectionBreak(kind) => p.section_break = *kind,
            ParaChange::AddBookmark(name) => {
                if !p.bookmarks.contains(name) {
                    p.bookmarks.push(name.clone());
                }
            }
            ParaChange::RemoveBookmark(name) => p.bookmarks.retain(|b| b != name),
            ParaChange::Level(level) => {
                if p.list.is_some() {
                    d.level = Some((*level).min(8));
                    if d.num_id.is_none() {
                        d.num_id = p.list.map(|l| l.num_id);
                    }
                }
            }
            ParaChange::Tabs(tabs) => d.tabs = tabs.clone(),
            ParaChange::ContextualSpacing(v) => d.contextual_spacing = Some(*v),
            ParaChange::WidowControl(v) => d.widow_control = Some(*v),
            ParaChange::SuppressLineNumbers(v) => d.suppress_line_numbers = Some(*v),
            ParaChange::SuppressAutoHyphens(v) => d.suppress_auto_hyphens = Some(*v),
            ParaChange::OverflowPunct(v) => d.overflow_punct = Some(*v),
            ParaChange::AutoSpaceDE(v) => d.auto_space_letters = Some(*v),
            ParaChange::AutoSpaceDN(v) => d.auto_space_digits = Some(*v),
            ParaChange::Kinsoku(v) => d.kinsoku = Some(*v),
            ParaChange::WordWrap(v) => d.word_wrap = Some(*v),
            ParaChange::Bidi(v) => d.bidi = Some(*v),
            ParaChange::SnapToGrid(v) => d.snap = Some(*v),
            ParaChange::DropCap(v) => d.drop_cap = Some(*v),
            ParaChange::Collapsed(v) => d.collapsed = Some(*v),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlowCommand {
    /// Insert `text` at `at`: `\n` ends a paragraph (the new one after it
    /// takes the paragraph's formatting), `\t` is a tab, `\u{b}` a line
    /// break and `\u{c}` a page break.
    InsertText { at: FlowPos, text: String, format: Option<InsertFormat> },
    /// Delete from `from` to `to` (in one list of blocks; paragraphs and
    /// tables between them go). The joined paragraph keeps the first one's
    /// formatting.
    Delete { from: FlowPos, to: FlowPos },
    /// Change the characters' formatting from `from` to `to` (in one list
    /// of blocks). A paragraph taken whole has its mark changed too.
    SetRunFormat { from: FlowPos, to: FlowPos, change: RunChange },
    /// Change the formatting of the paragraphs from `first` to `last` (in
    /// one list of blocks).
    SetParagraphFormat { first: u32, last: u32, change: ParaChange },
    /// Replace `remove` blocks at `index` of a list of blocks with `blocks`.
    ReplaceBlocks { container: Container, index: usize, remove: usize, blocks: Vec<Block> },
    /// Replace the list definitions (a list added); the paragraphs are
    /// resolved again.
    SetNumbering { numbering: crate::flow::Numbering },
    /// A table of `rows` × `columns` empty cells, `width` wide, at `at`: before
    /// the paragraph at its start, after it at its end, else between its two
    /// halves.
    InsertTable { at: FlowPos, rows: usize, columns: usize, width: Emu },
    /// Change a table's rows, columns or cells.
    EditTable { table: u32, edit: TableEdit },
    /// Remove a table.
    DeleteTable { table: u32 },
    /// An inline picture of `media` (already in the presentation's media)
    /// at `at`, `size` big, formatted as the text before it.
    InsertPicture { at: FlowPos, media: crate::model::MediaId, size: crate::geometry::EmuSize },
    /// The size of the picture at `at`.
    SetPictureSize { at: FlowPos, size: crate::geometry::EmuSize },
    /// How the picture at `at` is turned, flipped and cut.
    SetPictureLook { at: FlowPos, look: crate::flow::PictureLook },
    /// Replace the media of the picture at `at` (and optionally change its size).
    ReplacePicture { at: FlowPos, media: crate::model::MediaId, size: Option<crate::geometry::EmuSize> },
    /// Change the text wrapping style of the picture at `at` (inline or floating).
    SetPictureWrap { at: FlowPos, wrap: crate::model::PictureWrapType },
    /// Change the floating anchor properties of the picture at `at`.
    SetPictureFloating { at: FlowPos, anchor: Option<crate::model::FloatingAnchor> },
    /// Change the text wrapping polygon (折り返し点) of the picture at `at`.
    SetPictureWrapPolygon { at: FlowPos, polygon: Option<crate::model::WrapPolygon> },
    /// Set or clear the subdocument reference (`w:subDoc`, DOCX-TOC-016) on paragraph `para`.
    SetParagraphSubdocument { para: u32, subdoc: Option<crate::flow::SubdocumentInfo> },
    /// A text box (a floating rectangle, white with a thin black line)
    /// at `at`, `size` big, holding `text` (paragraphs split at `\n`).
    InsertTextBox { at: FlowPos, size: crate::geometry::EmuSize, text: String },
    /// A floating shape (a preset geometry; Word's 挿入 > 図形) at `at`, `size` big, filled with `fill` and
    /// outlined as `line` (none: no fill / no line).
    InsertShape { at: FlowPos, size: crate::geometry::EmuSize, geometry: crate::drawing::Geometry, fill: Option<crate::drawing::Rgb>, line: Option<(crate::drawing::Rgb, crate::geometry::Emu)> },
    /// An inline chart (Word's 挿入 > グラフ): `shape` (a `ShapeKind::Chart`) at `at`, formatted as the text before it.
    InsertChart { at: FlowPos, shape: Box<crate::model::Shape> },
    /// The chart at `at` changed to `chart` (データの編集, a look changed).
    SetChart { at: FlowPos, chart: Box<crate::chart::Chart> },
    /// A floating straight line (an arrow when `stroke` has ends) at `at`, `size` wide and high from its start.
    InsertLine { at: FlowPos, size: crate::geometry::EmuSize, stroke: crate::drawing::Stroke },
    /// A connector at `at` (DOCX-IMAGE-014): a straight line, or a shape of an elbow or curved connector's `geometry`, drawn
    /// from the frame's top left to its bottom right (mirrored by the flips), glued to the shapes in `glue`.
    InsertConnector { at: FlowPos, size: crate::geometry::EmuSize, stroke: crate::drawing::Stroke, geometry: Option<crate::Geometry>, flip_h: bool, flip_v: bool, glue: crate::model::Glue },
    /// The whole outline of the shape (or line) at `at`; `None` takes it away (a line cannot have none).
    SetShapeStroke { at: FlowPos, stroke: Option<crate::drawing::Stroke> },
    /// The fill and the line of the shape at `at`: `None` leaves one as it is, `Some(None)` takes it away.
    /// 図形の塗り（グラデーションを含む）と影: the fill and the shadow of a drawn shape (`None`: as it is; the shadow `Some(None)`: none).
    SetShapeEffects { at: FlowPos, fill: Option<crate::drawing::Fill>, shadow: Option<Option<crate::drawing::Shadow>>, glow: Option<Option<(crate::drawing::Color, i64)>>, soft_edge: Option<Option<i64>>, reflection: Option<Option<crate::drawing::Reflection>> },
    SetShapeStyle { at: FlowPos, fill: Option<Option<crate::drawing::Rgb>>, line: Option<Option<(crate::drawing::Rgb, crate::geometry::Emu)>> },
    /// The text of the text box (or any shape with text) at `at`; the
    /// first paragraph's and run's formatting stays.
    SetShapeText { at: FlowPos, text: String },
    /// Splice the plain text of the shape at `at`: replace `start..start+remove`
    /// (char indices) with `insert`, preserving per-run formatting.
    SpliceShapeText { at: FlowPos, start: usize, remove: usize, insert: String },
    /// Where the text sits vertically in the shape at `at` (DOCX-IMAGE-011, ribbon line 254: テキストの配置).
    SetShapeAnchor { at: FlowPos, anchor: crate::text::VerticalAnchor },
    /// The size and (for a floating shape) the offset from where it is anchored of the
    /// shape at `at`; `None` leaves it as it is.
    SetShapeGeometry { at: FlowPos, size: Option<crate::geometry::EmuSize>, offset: Option<(crate::geometry::Emu, crate::geometry::Emu)> },
    /// The connectors glued to shapes follow them to where the shapes are now (DOCX-IMAGE-014).
    RerouteConnectors,
    /// Put the floating shape at `at` where it was drawn: `size` and the position (`x`, `y`) from the top left of the page
    /// (DOCX-IMAGE-011, drawing a shape by dragging).
    PlaceShapeOnPage { at: FlowPos, size: crate::geometry::EmuSize, x: crate::geometry::Emu, y: crate::geometry::Emu },
    /// The formatting of characters `from..to` of the text of the shape at `at` (counted in the text, paragraphs separated by one
    /// character): only those characters change (DOCX-IMAGE-015).
    SetShapeRangeFormat { at: FlowPos, from: usize, to: usize, bold: Option<bool>, italic: Option<bool>, size: Option<u32>, color: Option<crate::drawing::Rgb> },
    /// The text formatting of the shape at `at`, all of its text; `None` leaves it as it is.
    SetShapeTextFormat { at: FlowPos, bold: Option<bool>, italic: Option<bool>, size: Option<u32>, color: Option<crate::drawing::Rgb> },
    /// Apply `format` to characters `range` (`Some((from, to))`) or to the whole text (`None`) of the shape at `at`.
    FormatShapeText { at: FlowPos, range: Option<(usize, usize)>, format: crate::text::CharacterFormat },
    /// The paper size and margins (of the document's last section).
    SetPage { page: crate::flow::PageSetup },
    /// A header (or footer) for a document without one: an empty paragraph
    /// of the header (footer) style.
    AddHeader { footer: bool },
    /// A page number (a `PAGE` field) at `at`.
    InsertPageNumber { at: FlowPos },
    /// A field (a cross-reference, a caption's number) at `at`.
    InsertField { at: FlowPos, kind: FlowRunKind },
    /// Toggle a checkbox content control at `at`.
    ToggleCheckBox { at: FlowPos },
    /// Select item `index` of a drop-down list or combo box at `at`.
    SelectDropDownItem { at: FlowPos, index: usize },
    /// Change the date and displayed text of a date picker content control (DOCX-FORM-007).
    SetDatePickerDate { at: FlowPos, full_date: String, text: String },
    /// Change the text of a content control (DOCX-FORM-001, DOCX-FORM-002).
    SetContentControlText { at: FlowPos, text: String, placeholder: bool },
    /// Change the text of a legacy text form field (DOCX-FORM-016).
    SetFormFieldText { at: FlowPos, text: String },
    /// Change the metadata properties of a legacy form field.
    SetFormFieldMeta { at: FlowPos, name: String, enabled: bool, calc_on_exit: bool },
    /// Insert a picture content control (DOCX-FORM-003).
    InsertPictureControl { at: FlowPos, media: crate::model::MediaId, size: crate::geometry::EmuSize, placeholder: bool },
    /// Reset a picture content control to placeholder (DOCX-FORM-003).
    ResetPictureControl { at: FlowPos, placeholder_media: crate::model::MediaId, size: crate::geometry::EmuSize },
    /// Change the gallery and category of a building block gallery content control (DOCX-FORM-008).
    SetBuildingBlockGalleryMeta { at: FlowPos, gallery: String, category: String },
    /// Insert an item into a repeating section content control (DOCX-FORM-009).
    InsertRepeatingSectionItem { at: FlowPos, index: usize, text: String, restore_selected: Option<usize> },
    /// Delete an item from a repeating section content control (DOCX-FORM-009).
    DeleteRepeatingSectionItem { at: FlowPos, index: usize, restore_selected: Option<usize> },
    /// Select the active item in a repeating section content control (DOCX-FORM-009).
    SelectRepeatingSectionItem { at: FlowPos, index: usize },
    /// Change the section title and insert/delete restriction of a repeating section (DOCX-FORM-009).
    SetRepeatingSectionMeta { at: FlowPos, section_title: String, do_not_allow_insert_delete: bool },
    /// Group the run at `at` into a group content control (DOCX-FORM-014).
    GroupControl { at: FlowPos, text: String, placeholder: bool, control: Option<Box<crate::flow::ControlInfo>> },
    /// Ungroup a group content control at `at`, converting it back to plain text (DOCX-FORM-014).
    UngroupControl { at: FlowPos, restore: Option<(String, bool, Option<Box<crate::flow::ControlInfo>>)> },
    /// The title, tag and lock of the content control at `at`.
    SetControlInfo { at: FlowPos, info: Option<Box<crate::flow::ControlInfo>> },
    /// Make the text from `from` to `to` (in one paragraph) a link, or no
    /// link (`None`).
    SetLink { from: FlowPos, to: FlowPos, link: Option<crate::flow::Link> },
    /// A table of contents (`entries`, made by [`toc_entries`]) before
    /// paragraph `before`, or in place of the table of contents that
    /// starts at `before` (an update); bookmarks put on the headings.
    SetToc { before: u32, replace: bool, entries: Vec<FlowParagraph>, bookmarks: Vec<(u32, String)> },
    /// Several commands as one (undone in reverse).
    Many(Vec<FlowCommand>),
    /// The character style of the run at position `at` (one position: a
    /// note's mark).
    SetNoteStyle { at: FlowPos, style: Option<String> },
    /// The footnotes and endnotes replaced.
    SetNotes { footnotes: Vec<crate::flow::Note>, endnotes: Vec<crate::flow::Note> },
    /// The comments replaced.
    SetComments(Vec<crate::flow::Comment>),
    /// The character style (`w:rStyle`) of the text from `from` to `to`, or
    /// none.
    SetCharStyle { from: FlowPos, to: FlowPos, style: Option<String> },
    /// The watermark (透かし), or none.
    /// The sections' own ones (see [`FlowDocument::section_watermarks`])
    /// replaced by `sections`.
    SetWatermark { watermark: Option<crate::flow::Watermark>, sections: Vec<(u32, Option<crate::flow::Watermark>)> },
    /// Whether changes are tracked (変更履歴の記録).
    SetTrackChanges(bool),
    /// The text from `from` to `to` marked as a tracked change, or not.
    MarkRevision { from: FlowPos, to: FlowPos, revision: Option<crate::flow::Revision> },
    /// Delete from `from` to `to` as a tracked change: the text is marked
    /// deleted (what was a tracked insertion goes); paragraphs are not joined.
    TrackDelete { from: FlowPos, to: FlowPos, author: String, date: String },
    /// Mark the text from `from` to `to` as formatted differently (keeping
    /// its formatting now as the formatting before), for a tracked change of
    /// formatting that follows.
    TrackFormat { from: FlowPos, to: FlowPos, author: String, date: String },
    /// The sections as `state` says (their breaks, setups, headers and
    /// footers, and the last section's).
    SetSections { state: Box<SectionState> },
    /// Define style `id` as `style` (a new style, or a changed one), or
    /// remove it (`None`); the paragraphs are drawn again with it.
    SetStyle { id: String, style: Option<crate::flow::FlowStyle> },
    /// ドキュメント全体の段落の間隔のプリセット（Word's デザイン > ドキュメントの書式設定 > 段落の間隔）.
    SetDocumentParagraphSpacing {
        preset: Option<crate::flow::ParagraphSpacingPreset>,
        styles: Option<crate::flow::FlowStyles>,
    },
    /// ドキュメント全体のスタイル セットのプリセット（Word's デザイン > ドキュメントの書式設定 > スタイル セット）.
    SetDocumentStyleSet {
        preset: Option<crate::flow::StyleSetPreset>,
        styles: Option<crate::flow::FlowStyles>,
    },
    /// 文字種の変換 (DOCX-JA-020: 全角半角・大文字小文字・ひらがなカタカナ).
    ConvertCase { from: FlowPos, to: FlowPos, conversion: CaseConversion },
    /// ページの色 (`w:background`), or none.
    SetPageColor(Option<Rgb>),
    /// フィールドコードの表示/非表示（文書全体、Word の Alt+F9 / ⌥F9）。
    SetShowFieldCodes(bool),
    /// コンテンツ コントロールのデザイン モード（Word の「デザイン モード」開発 > デザイン モード）。
    SetFormDesignMode(bool),
    /// 文書のプロパティ（タイトル、作成者、作成日時等）の変更。
    SetDocProperties(crate::flow::DocProperties),
    /// 最終版としてマーク（Word の「最終版にする」）。
    SetMarkAsFinal(bool),
    /// 読み取り専用を推奨（Word の「読み取り専用を推奨する」）。
    SetReadOnlyRecommended(bool),
    /// 禁則文字の設定: the characters a line may not start (`start`) and end with; `None` is the standard ones.
    SetKinsokuLists { start: Option<String>, end: Option<String> },
    /// 禁則処理の方式（Word の「禁則処理の方式」標準/厳密: `w:strictFirstAndLastChars`、DOCX-JA-010）。
    SetStrictFirstAndLastChars(bool),
    /// 文書の既定テーマフォント言語・既定言語（Word の「校正言語の設定 > 既定に設定」: `w:themeFontLang`、DOCX-JA-019）。
    SetThemeFontLang(Option<crate::flow::ThemeFontLang>),
    /// 印刷前・文書オープン時にフィールドを更新（Word の「印刷前にフィールドを更新する」）。
    SetUpdateFields(bool),
    /// 差し込み印刷の文書の種類（`w:mailMerge/w:mainDocumentType`）を設定する（`None`: 通常の文書）。
    SetMailMergeType(Option<String>),
    /// 差し込み印刷のフィールドと列の対応（`w:mailMerge/w:odso/w:fieldMapData`、DOCX-MERGE-009）。
    SetMailMergeFieldMap(Vec<crate::merge::FieldMapData>),
    /// 注の区切り線: the line above the footnotes (`endnote` false) or the endnotes.
    SetNoteSeparator { endnote: bool, kind: crate::flow::NoteSeparator },
    /// 文書変数（Word の ActiveDocument.Variables、w:settings の w:docVars）。
    SetDocVariables(std::collections::BTreeMap<String, String>),
    /// 変更履歴の記録をロック（Word の「変更履歴の記録をロック」）。
    SetLockTracking { locked: bool, track_changes: bool },
    /// 表示モードの変更（Word の `w:view`: print, reading, web, normal 等、DOCX-VIEW-005）。
    SetViewMode(crate::flow::DocumentViewMode),
    /// 閲覧・レビュー・編集モードの変更（Word の 編集/レビュー/閲覧、DOCX-VIEW-021）。
    SetInteractionMode(crate::flow::DocumentInteractionMode),
    /// 文書の編集制限・保護モードの変更（Word の `w:documentProtection/@w:edit`、DOCX-SEC-002, DOCX-SEC-004, DOCX-SEC-005）。
    SetDocumentProtection(crate::flow::DocumentProtection),
    /// 書式変更の制限の変更（Word の `w:documentProtection/@w:formatting`、DOCX-SEC-003）。
    SetFormattingProtection(bool),
    /// 画像の圧縮（メディアの置換およびトリミング破棄、DOCX-OPTIONS-011）。
    CompressPicture { at: FlowPos, media: crate::model::MediaId, reset_crop: bool },
    /// 画像の品質と圧縮設定の変更（Word の「画像のサイズと品質」、DOCX-OPTIONS-011）。
    SetImageQualitySettings { default_dpi: Option<u32>, discard_crop: bool, do_not_compress: bool },
    /// 自動回復用データの保存間隔の変更（Word の「自動回復用データの保存間隔」、DOCX-OPTIONS-012）。
    SetSaveFrequency(Option<u32>),
    /// フォントの埋め込み設定の変更（Word の「ファイルにフォントを埋め込む」、DOCX-OPTIONS-013）。
    SetFontEmbedding { embed_true_type_fonts: bool, embed_system_fonts: bool, save_subset_fonts: bool },
    /// 互換性オプションの変更（Word の「互換性オプション」、DOCX-OPTIONS-013）。
    SetCompatibilityOptions { compatibility_mode: Option<u32>, use_fe_layout: bool, do_not_expand_shift_return: bool },
    /// 添付テンプレートおよびスタイル自動更新の変更（Word の「テンプレートとアドイン」、DOCX-STYLE-017, DOCX-FILE-009）。
    AttachTemplate { template: Option<String>, link_styles: bool },
    /// 文書スタイルの一括更新（テンプレートからのスタイル適用、DOCX-STYLE-017, DOCX-FILE-009）。
    UpdateStyles(std::sync::Arc<crate::flow::FlowStyles>),
    /// 図形のグループ階層の変更（Word の `wpg:wgp` グループ化・グループ解除、DOCX-FLOAT-016）。
    SetShapeGroups(Vec<crate::ShapeGroup>),
}

/// A change of a table (rows and cells by index, columns by grid column).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableEdit {
    /// A row like row `row`, with empty cells, above or below it.
    InsertRow { row: usize, below: bool },
    /// Remove rows `first` to `last` (the table, if they are all its rows).
    DeleteRows { first: usize, last: usize },
    /// A column like grid column `column`, left or right of it; the columns
    /// are narrowed to keep the table's width.
    InsertColumn { column: usize, right: bool },
    /// Remove grid column `column` (the table, if it is the only one).
    DeleteColumn { column: usize },
    /// A cell inserted at row `row`, cell `cell`, shifting following cells right.
    InsertCellShiftRight { row: usize, cell: usize },
    /// A cell inserted at row `row`, cell `cell`, shifting cells below it down.
    InsertCellShiftDown { row: usize, cell: usize },
    /// The cell at row `row`, cell `cell` deleted, shifting following cells left.
    DeleteCellShiftLeft { row: usize, cell: usize },
    /// The cell at row `row`, cell `cell` deleted, shifting cells below it up.
    DeleteCellShiftUp { row: usize, cell: usize },
    /// The cells' fill (`None`: no fill).
    Shading { cells: Vec<(usize, usize)>, color: Option<Rgb> },
    /// The cells' vertical alignment.
    Anchor { cells: Vec<(usize, usize)>, anchor: crate::flow::CellAnchor },
    /// The cells' text direction (`w:textDirection`).
    Direction { cells: Vec<(usize, usize)>, direction: crate::flow::CellDirection },
    /// The cells' no-wrap setting (`w:noWrap`).
    NoWrap { cells: Vec<(usize, usize)>, no_wrap: bool },
    /// The cells' margins (top, right, bottom, left) in EMU (`w:tcMar`).
    Margins { cells: Vec<(usize, usize)>, margins: [Emu; 4] },
    /// A grid column's width.
    ColumnWidth { column: usize, width: Emu },
    /// タイトル行の繰り返し: rows 0 to `row` repeated at the top of each
    /// page the table goes on to (`on`), or no rows repeated.
    RepeatHeader { row: usize, on: bool },
    /// 行の途中で改ページしない: the rows kept on one page, or not.
    CantSplit { rows: Vec<usize>, on: bool },
    /// A row's height (`w:trHeight`).
    RowHeight { row: usize, height: crate::flow::RowHeight },
    /// 列の幅を揃える: the grid columns made as wide as each other, the
    /// table as wide as before.
    EqualColumns,
    /// 行の高さを揃える: the rows made as tall as each other, the
    /// table as tall as before.
    EqualRows,
    /// 文字列の幅に自動調整: size columns to fit cell contents.
    AutoFitContents,
    /// ウィンドウサイズに自動調整: scale columns to fit the available width.
    AutoFitWindow(crate::geometry::Emu),
    /// 列の幅を固定: fix columns to current widths.
    FixedColumns,
    /// 表スタイルのオプション (`w:tblLook`): the heading row, the bands…
    Look(crate::flow::TableLook),
    /// 表の配置 (`w:jc`).
    Align(FlowAlign),
    /// 表の配置（互換名）
    Alignment(FlowAlign),
    /// 表の字下げ (`w:tblInd`).
    Indent(Emu),
    /// セル間隔 (`w:tblCellSpacing`).
    CellSpacing(Option<Emu>),
    /// 表内の行の並べ替え (Sort table rows by column)
    SortRows { column: usize, ascending: bool, has_header: bool },
    /// 浮動表の配置 (`w:tblpPr`): `None` でインライン（行内）、`Some(f)` で浮動表設定。
    Floating(Option<crate::flow::TableFloat>),
    /// 浮動表の本文からの距離 (`w:tblpPr` の leftFromText, rightFromText, topFromText, bottomFromText)。
    FloatDistance([Emu; 4]),
    /// 表のスタイル (`w:tblStyle`): `None` でスタイル解除（Normal Table）、`Some(id)` でスタイル適用。
    Style(Option<String>),
}

pub(crate) fn apply(presentation: &mut Presentation, command: FlowCommand) -> Result<Command, EditError> {
    let Some(flow) = presentation.flow.as_mut() else { return Err(EditError::IndexOutOfRange) };
    let inverse = apply_to(Arc::make_mut(flow), command)?;
    // The pages show the new document until the viewer lays it out again.
    if let Some(flow) = presentation.flow.clone() {
        for slide in &mut presentation.slides {
            for shape in &mut slide.shapes {
                if let ShapeKind::Page { document, .. } = &mut shape.kind {
                    *document = flow.clone();
                }
            }
        }
    }
    Ok(Command::Flow(inverse))
}

/// Two positions in one list of blocks, in order: the list, the indexes of
/// their paragraphs, and the positions.
fn ordered(doc: &FlowDocument, a: FlowPos, b: FlowPos) -> Result<(Container, usize, usize, FlowPos, FlowPos), EditError> {
    let (ca, ia) = locate(doc, a.para).ok_or(EditError::IndexOutOfRange)?;
    let (cb, ib) = locate(doc, b.para).ok_or(EditError::IndexOutOfRange)?;
    if ca != cb {
        return Err(EditError::IndexOutOfRange);
    }
    let list = blocks(doc, ca).ok_or(EditError::IndexOutOfRange)?;
    let len = |i: usize| match &list[i] {
        Block::Paragraph(p) => Ok(p.len()),
        Block::Table(_) => Err(EditError::IndexOutOfRange),
    };
    if a.offset > len(ia)? || b.offset > len(ib)? {
        return Err(EditError::IndexOutOfRange);
    }
    Ok(if (ia, a.offset) <= (ib, b.offset) { (ca, ia, ib, a, b) } else { (ca, ib, ia, b, a) })
}

/// Apply a command to a document; return the command that undoes it. On
/// error the document is unchanged.
pub fn apply_to(doc: &mut FlowDocument, command: FlowCommand) -> Result<FlowCommand, EditError> {
    let styles = doc.styles.clone();
    match command {
        FlowCommand::ReplaceBlocks { container, index, remove, blocks } => {
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            if index + remove > list.len() {
                return Err(EditError::IndexOutOfRange);
            }
            let count = blocks.len();
            let old: Vec<Block> = list.splice(index..index + remove, blocks).collect();
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: count, blocks: old })
        }
        FlowCommand::SetNumbering { numbering } => {
            let styles = Arc::make_mut(&mut doc.styles);
            let old = std::mem::replace(&mut styles.numbering, numbering);
            let styles = doc.styles.clone();
            for list in [&mut doc.body, &mut doc.header, &mut doc.footer] {
                resolve_all(list, &styles);
            }
            Ok(FlowCommand::SetNumbering { numbering: old })
        }
        FlowCommand::SetStyle { id, style } => {
            let styles = Arc::make_mut(&mut doc.styles);
            let old = match style {
                Some(s) => styles.styles.insert(id.clone(), s),
                None => styles.styles.remove(&id),
            };
            let styles = doc.styles.clone();
            for list in [&mut doc.body, &mut doc.header, &mut doc.footer, &mut doc.header_first, &mut doc.footer_first, &mut doc.header_even, &mut doc.footer_even] {
                resolve_all(list, &styles);
            }
            Ok(FlowCommand::SetStyle { id, style: old })
        }
        FlowCommand::SetDocumentParagraphSpacing { preset, styles } => {
            let old_preset = doc.paragraph_spacing_preset;
            let old_styles = (*doc.styles).clone();
            doc.paragraph_spacing_preset = preset;
            if let Some(styles) = styles {
                doc.styles = std::sync::Arc::new(styles);
            } else if let Some(preset) = preset {
                let (before, after, line) = preset.spacing();
                let s = std::sync::Arc::make_mut(&mut doc.styles);
                let normal_id = s.default_paragraph.clone().or_else(|| {
                    s.paragraph_style_named("Normal").map(str::to_string)
                });
                if let Some(normal_id) = normal_id {
                    if let Some(normal_style) = s.styles.get_mut(&normal_id) {
                        normal_style.para.before = Some(before);
                        normal_style.para.after = Some(after);
                        normal_style.para.line = Some(line);
                    }
                } else {
                    s.defaults_para.before = Some(before);
                    s.defaults_para.after = Some(after);
                    s.defaults_para.line = Some(line);
                }
            }
            let styles = doc.styles.clone();
            for list in [&mut doc.body, &mut doc.header, &mut doc.footer, &mut doc.header_first, &mut doc.footer_first, &mut doc.header_even, &mut doc.footer_even] {
                resolve_all(list, &styles);
            }
            Ok(FlowCommand::SetDocumentParagraphSpacing {
                preset: old_preset,
                styles: Some(old_styles),
            })
        }
        FlowCommand::SetDocumentStyleSet { preset, styles } => {
            let old_preset = doc.style_set_preset;
            let old_styles = (*doc.styles).clone();
            doc.style_set_preset = preset;
            if let Some(styles) = styles {
                doc.styles = std::sync::Arc::new(styles);
            } else if let Some(preset) = preset {
                let s = std::sync::Arc::make_mut(&mut doc.styles);
                preset.apply(s);
            }
            let styles = doc.styles.clone();
            for list in [&mut doc.body, &mut doc.header, &mut doc.footer, &mut doc.header_first, &mut doc.footer_first, &mut doc.header_even, &mut doc.footer_even] {
                resolve_all(list, &styles);
            }
            Ok(FlowCommand::SetDocumentStyleSet {
                preset: old_preset,
                styles: Some(old_styles),
            })
        }
        FlowCommand::ConvertCase { from, to, conversion } => {
            let (container, first, last, from, to) = ordered(doc, from, to)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let old: Vec<Block> = list[first..=last].to_vec();
            for (i, block) in list[first..=last].iter_mut().enumerate() {
                let Block::Paragraph(p) = block else { continue };
                let start = if i == 0 { from.offset } else { 0 };
                let end = if first + i == last { to.offset } else { p.len() };
                if start < end {
                    p.convert_case_range(start, end, conversion);
                    p.resolve(&styles);
                    p.normalize();
                }
            }
            Ok(FlowCommand::ReplaceBlocks { container, index: first, remove: last - first + 1, blocks: old })
        }
        FlowCommand::InsertTable { at, rows, columns, width } => {
            if rows == 0 || columns == 0 {
                return Err(EditError::IndexOutOfRange);
            }
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let mut next = doc.new_id();
            let mut id = || {
                next += 1;
                next - 1
            };
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(p) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let line = FlowBorder { width: 12_700 / 2, color: Rgb::BLACK, style: crate::flow::BorderStyle::Single, space: 0, art: None };
            let style = Some(crate::flow::TableStyleRef {
                id: "TableGrid".to_string(),
                look: crate::flow::TableLook {
                    first_row: true,
                    last_row: false,
                    first_column: true,
                    last_column: false,
                    no_hband: false,
                    no_vband: true,
                },
                row_band: 1,
                col_band: 1,
            });
            let mut table = FlowTable { id: id(), borders: [Some(Some(line)); 6], columns: vec![width / columns as Emu / 635 * 635; columns], style, fit_to_page: true, ..FlowTable::default() };
            for _ in 0..rows {
                let mut row = FlowRow { id: id(), ..FlowRow::default() };
                for _ in 0..columns {
                    let mut para = FlowParagraph { id: id(), table_style: Some("TableGrid".to_string()), ..FlowParagraph::default() };
                    para.resolve(&styles);
                    row.cells.push(FlowCell { id: id(), span: 1, margins: [0, 108 * 635, 0, 108 * 635], blocks: vec![Block::Paragraph(para)], ..FlowCell::default() });
                }
                table.rows.push(row);
            }
            table.resolve_borders();
            let len = p.len();
            let in_cell = matches!(container, Container::Cell { .. });
            let (start, remove) = if at.offset == 0 { (index, 0) } else { (index, 1) };
            let (replaced, mut blocks) = if at.offset == 0 {
                (0, vec![Block::Table(table)])
            } else if at.offset >= len {
                (0, vec![Block::Paragraph(p.clone()), Block::Table(table)])
            } else {
                let mut first = FlowParagraph { runs: p.runs_before(at.offset), ..p.clone() };
                let mut second = FlowParagraph { id: id(), runs: p.runs_after(at.offset), section_break: crate::flow::SectionBreak::None, section: None, bookmarks: Vec::new(), field_start: None, field_end: false, subdoc: None, ..p.clone() };
                first.resolve(&styles);
                first.normalize();
                second.resolve(&styles);
                second.normalize();
                (1, vec![Block::Paragraph(first), Block::Table(table), Block::Paragraph(second)])
            };
            if in_cell && !matches!(blocks.last(), Some(Block::Paragraph(_))) && start + remove == list.len() {
                let mut trailing = FlowParagraph { id: id(), ..FlowParagraph::default() };
                trailing.resolve(&styles);
                blocks.push(Block::Paragraph(trailing));
            }
            let _ = replaced;
            let old: Vec<Block> = list[start..start + remove].to_vec();
            let count = blocks.len();
            list.splice(start..start + remove, blocks);
            Ok(FlowCommand::ReplaceBlocks { container, index: start, remove: count, blocks: old })
        }
        FlowCommand::InsertPicture { at, media, size } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let (format, char_style, style) = old.format_at(at.offset);
            let mut p = FlowParagraph { runs: old.runs_before(at.offset), ..old.clone() };
            p.runs.push(FlowRun { kind: FlowRunKind::Picture { media, size, look: Default::default() }, style, format, char_style, link: None, revision: None, show_field_code: false, field_locked: false, control: None });
            p.runs.extend(old.runs_after(at.offset));
            p.resolve(&styles);
            p.normalize();
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::InsertPictureControl { at, media, size, placeholder } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let (format, char_style, style) = old.format_at(at.offset);
            let mut p = FlowParagraph { runs: old.runs_before(at.offset), ..old.clone() };
            p.runs.push(FlowRun { kind: FlowRunKind::PictureControl { media, size, look: Default::default(), placeholder }, style, format, char_style, link: None, revision: None, show_field_code: false, field_locked: false, control: None });
            p.runs.extend(old.runs_after(at.offset));
            p.resolve(&styles);
            p.normalize();
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::ResetPictureControl { at, placeholder_media, size } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let run = p.runs.get_mut(i).ok_or(EditError::IndexOutOfRange)?;
            if let Some(c) = run.control.as_ref().filter(|_| run.is_content_control()) {
                if c.lock.keeps_content() {
                    return Err(EditError::ControlLocked);
                }
            }
            match &mut run.kind {
                FlowRunKind::PictureControl { media: m, size: s, look, placeholder } => {
                    *m = placeholder_media;
                    *s = size;
                    *look = Default::default();
                    *placeholder = true;
                }
                _ => return Err(EditError::IndexOutOfRange),
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetPictureSize { at, size } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            match p.runs.get_mut(i) {
                Some(FlowRun { kind: FlowRunKind::Picture { size: s, .. } | FlowRunKind::PictureControl { size: s, .. }, .. }) => *s = size,
                Some(FlowRun { kind: FlowRunKind::Shape(shape), .. }) => {
                    shape.frame.width = size.width;
                    shape.frame.height = size.height;
                }
                _ => return Err(EditError::IndexOutOfRange),
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetPictureLook { at, look } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            match p.runs.get_mut(i) {
                Some(FlowRun { kind: FlowRunKind::Picture { look: l, .. } | FlowRunKind::PictureControl { look: l, .. }, .. }) => *l = look,
                Some(FlowRun { kind: FlowRunKind::Shape(shape), .. }) => {
                    shape.rotation = look.rotation;
                    shape.flip_h = look.flip_h;
                    shape.flip_v = look.flip_v;
                    if let crate::model::ShapeKind::Picture(pic) = &mut shape.kind {
                        pic.crop = look.crop;
                        pic.stroke = look.stroke;
                        pic.luminance = look.luminance;
                        pic.duotone = look.duotone;
                        pic.alt_text = look.alt_text;
                        pic.decorative = look.decorative;
                        pic.name = look.name;
                        pic.effects = look.effects;
                        pic.soft_edge = look.soft_edge;
                        pic.geometry = look.geometry;
                    }
                }
                _ => return Err(EditError::IndexOutOfRange),
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::ReplacePicture { at, media, size } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let run = p.runs.get_mut(i).ok_or(EditError::IndexOutOfRange)?;
            if let Some(c) = run.control.as_ref().filter(|_| run.is_content_control()) {
                if c.lock.keeps_content() {
                    return Err(EditError::ControlLocked);
                }
            }
            match &mut run.kind {
                FlowRunKind::Picture { media: m, size: s, .. } => {
                    *m = media;
                    if let Some(new_size) = size {
                        *s = new_size;
                    }
                }
                FlowRunKind::PictureControl { media: m, size: s, placeholder, .. } => {
                    *m = media;
                    *placeholder = false;
                    if let Some(new_size) = size {
                        *s = new_size;
                    }
                }
                FlowRunKind::Shape(shape) => {
                    if let crate::model::ShapeKind::Picture(pic) = &mut shape.kind {
                        pic.media = media;
                    }
                    if let Some(new_size) = size {
                        shape.frame.width = new_size.width;
                        shape.frame.height = new_size.height;
                    }
                }
                _ => return Err(EditError::IndexOutOfRange),
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::InsertTextBox { at, size, text } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let shape_id = doc.new_shape_id();
            let relative_height = next_floating_relative_height(doc);
            // Centred across the text column, as the box of Insert > Text Box (not stuck at its left edge: GitHub issue #14).
            let centred = ((doc.page.size.width - doc.page.left - doc.page.right - size.width) / 2).max(0);
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let (format, char_style, style) = old.format_at(at.offset);
            let mut body = crate::text::TextBody::plain(&text, crate::text::TextStyle { font_size: (style.size).max(100), ..Default::default() });
            body.insets = crate::text::Insets { left: 91_440, top: 45_720, right: 91_440, bottom: 45_720 };
            let stroke = crate::drawing::Stroke::solid(crate::drawing::Color::BLACK, 12_700);
            let shape = crate::model::Shape {
                id: crate::model::ShapeId(shape_id as u64),
                frame: crate::geometry::EmuRect::new(0, 0, size.width, size.height),
                rotation: 0,
                flip_h: false,
                flip_v: false,
                kind: crate::model::ShapeKind::Auto(crate::model::AutoShape {
                    geometry: crate::drawing::Geometry::Rect,
                    fill: crate::drawing::Fill::Solid(crate::drawing::Color::WHITE),
                    stroke: Some(stroke),
                    text: Some(body),
                    tilt: None,
                    shadow: None,
                    glow: None,
                    inner_shadow: None,
                    soft_edge: None,
                    reflection: None,
                    bevel: None,
                }),
                floating: Some(crate::model::FloatingAnchor { offset_x: centred, relative_height, ..Default::default() }),
            };
            let mut p = FlowParagraph { runs: old.runs_before(at.offset), ..old.clone() };
            p.runs.push(FlowRun { kind: FlowRunKind::Shape(Box::new(shape)), style, format, char_style, link: None, revision: None, show_field_code: false, field_locked: false, control: None });
            p.runs.extend(old.runs_after(at.offset));
            p.resolve(&styles);
            p.normalize();
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::InsertShape { at, size, geometry, fill, line } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let shape_id = doc.new_shape_id();
            let relative_height = next_floating_relative_height(doc);
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let (format, char_style, style) = old.format_at(at.offset);
            let shape = crate::model::Shape {
                id: crate::model::ShapeId(shape_id as u64),
                frame: crate::geometry::EmuRect::new(0, 0, size.width, size.height),
                rotation: 0,
                flip_h: false,
                flip_v: false,
                kind: crate::model::ShapeKind::Auto(crate::model::AutoShape {
                    geometry,
                    fill: fill.map_or(crate::drawing::Fill::None, |c| crate::drawing::Fill::Solid(crate::drawing::Color::opaque(c))),
                    stroke: line.map(|(c, w)| crate::drawing::Stroke::solid(crate::drawing::Color::opaque(c), w)),
                    text: Some(centred_shape_text_body(style.size)),
                    tilt: None,
                    shadow: None,
                    glow: None,
                    inner_shadow: None,
                    soft_edge: None,
                    reflection: None,
                    bevel: None,
                }),
                floating: Some(crate::model::FloatingAnchor { wrap: "square".into(), relative_height, ..Default::default() }),
            };
            let mut p = FlowParagraph { runs: old.runs_before(at.offset), ..old.clone() };
            p.runs.push(FlowRun { kind: FlowRunKind::Shape(Box::new(shape)), style, format, char_style, link: None, revision: None, show_field_code: false, field_locked: false, control: None });
            p.runs.extend(old.runs_after(at.offset));
            p.resolve(&styles);
            p.normalize();
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::InsertLine { at, size, stroke } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let shape_id = doc.new_shape_id();
            let relative_height = next_floating_relative_height(doc);
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let (format, char_style, style) = old.format_at(at.offset);
            let shape = crate::model::Shape {
                id: crate::model::ShapeId(shape_id as u64),
                frame: crate::geometry::EmuRect::new(0, 0, size.width, size.height),
                rotation: 0,
                flip_h: false,
                flip_v: false,
                kind: crate::model::ShapeKind::Line(stroke),
                floating: Some(crate::model::FloatingAnchor { wrap: "none".into(), relative_height, ..Default::default() }),
            };
            let mut p = FlowParagraph { runs: old.runs_before(at.offset), ..old.clone() };
            p.runs.push(FlowRun { kind: FlowRunKind::Shape(Box::new(shape)), style, format, char_style, link: None, revision: None, show_field_code: false, field_locked: false, control: None });
            p.runs.extend(old.runs_after(at.offset));
            p.resolve(&styles);
            p.normalize();
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::InsertConnector { at, size, stroke, geometry, flip_h, flip_v, glue } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let shape_id = doc.new_shape_id();
            let relative_height = next_floating_relative_height(doc);
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let (format, char_style, style) = old.format_at(at.offset);
            let kind = match geometry {
                None => crate::model::ShapeKind::Line(stroke),
                Some(geometry) => crate::model::ShapeKind::Auto(crate::model::AutoShape {
                    geometry,
                    fill: crate::drawing::Fill::None,
                    stroke: Some(stroke),
                    text: None,
                    tilt: None,
                    shadow: None,
                    glow: None,
                    inner_shadow: None,
                    soft_edge: None,
                    reflection: None,
                    bevel: None,
                }),
            };
            let shape = crate::model::Shape {
                id: crate::model::ShapeId(shape_id as u64),
                frame: crate::geometry::EmuRect::new(0, 0, size.width, size.height),
                rotation: 0,
                flip_h,
                flip_v,
                kind,
                floating: Some(crate::model::FloatingAnchor { wrap: "none".into(), glue, relative_height, ..Default::default() }),
            };
            let mut p = FlowParagraph { runs: old.runs_before(at.offset), ..old.clone() };
            p.runs.push(FlowRun { kind: FlowRunKind::Shape(Box::new(shape)), style, format, char_style, link: None, revision: None, show_field_code: false, field_locked: false, control: None });
            p.runs.extend(old.runs_after(at.offset));
            p.resolve(&styles);
            p.normalize();
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::InsertChart { at, shape } => {
            if !matches!(shape.kind, crate::model::ShapeKind::Chart(_)) {
                return Err(EditError::IndexOutOfRange);
            }
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let (format, char_style, style) = old.format_at(at.offset);
            let mut p = FlowParagraph { runs: old.runs_before(at.offset), ..old.clone() };
            p.runs.push(FlowRun { kind: FlowRunKind::Shape(shape), style, format, char_style, link: None, revision: None, show_field_code: false, field_locked: false, control: None });
            p.runs.extend(old.runs_after(at.offset));
            p.resolve(&styles);
            p.normalize();
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetChart { at, chart } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            let FlowRunKind::Shape(shape) = &mut run.kind else { return Err(EditError::IndexOutOfRange) };
            let crate::model::ShapeKind::Chart(c) = &mut shape.kind else { return Err(EditError::IndexOutOfRange) };
            **c = *chart;
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetShapeStroke { at, stroke } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            let FlowRunKind::Shape(shape) = &mut run.kind else { return Err(EditError::IndexOutOfRange) };
            match &mut shape.kind {
                crate::model::ShapeKind::Auto(a) => a.stroke = stroke,
                crate::model::ShapeKind::Line(s) => *s = stroke.ok_or(EditError::IndexOutOfRange)?,
                _ => return Err(EditError::IndexOutOfRange),
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetShapeEffects { at, fill, shadow, glow, soft_edge, reflection } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            let FlowRunKind::Shape(shape) = &mut run.kind else { return Err(EditError::IndexOutOfRange) };
            let crate::model::ShapeKind::Auto(auto) = &mut shape.kind else { return Err(EditError::IndexOutOfRange) };
            if let Some(fill) = fill {
                auto.fill = fill;
            }
            if let Some(shadow) = shadow {
                auto.shadow = shadow;
            }
            if let Some(glow) = glow {
                auto.glow = glow;
            }
            if let Some(soft_edge) = soft_edge {
                auto.soft_edge = soft_edge;
            }
            if let Some(reflection) = reflection {
                auto.reflection = reflection;
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetShapeStyle { at, fill, line } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            let FlowRunKind::Shape(shape) = &mut run.kind else { return Err(EditError::IndexOutOfRange) };
            let crate::model::ShapeKind::Auto(auto) = &mut shape.kind else { return Err(EditError::IndexOutOfRange) };
            if let Some(fill) = fill {
                auto.fill = fill.map_or(crate::drawing::Fill::None, |c| crate::drawing::Fill::Solid(crate::drawing::Color::opaque(c)));
            }
            if let Some(line) = line {
                auto.stroke = line.map(|(c, w)| crate::drawing::Stroke::solid(crate::drawing::Color::opaque(c), w));
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetShapeText { at, text } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            let FlowRunKind::Shape(shape) = &mut run.kind else { return Err(EditError::IndexOutOfRange) };
            let body = match &mut shape.kind {
                crate::model::ShapeKind::Auto(a) => a.text.get_or_insert_with(Default::default),
                crate::model::ShapeKind::TextBox(body) => body,
                _ => return Err(EditError::IndexOutOfRange),
            };
            if body.is_empty() || text.is_empty() {
                body.set_text(&text);
            } else {
                let old_text = body.text();
                if old_text != text {
                    let (a, b): (Vec<char>, Vec<char>) = (old_text.chars().collect(), text.chars().collect());
                    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
                    let rest = a.len().min(b.len()) - prefix;
                    let suffix = a.iter().rev().zip(b.iter().rev()).take(rest).take_while(|(x, y)| x == y).count();
                    let insert: String = b[prefix..b.len() - suffix].iter().collect();
                    body.splice(prefix, a.len() - prefix - suffix, &insert);
                }
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SpliceShapeText { at, start, remove, insert } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            let FlowRunKind::Shape(shape) = &mut run.kind else { return Err(EditError::IndexOutOfRange) };
            let body = match &mut shape.kind {
                crate::model::ShapeKind::Auto(a) => a.text.get_or_insert_with(Default::default),
                crate::model::ShapeKind::TextBox(body) => body,
                _ => return Err(EditError::IndexOutOfRange),
            };
            body.splice(start, remove, &insert);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetShapeAnchor { at, anchor } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            let FlowRunKind::Shape(shape) = &mut run.kind else { return Err(EditError::IndexOutOfRange) };
            match &mut shape.kind {
                crate::model::ShapeKind::Auto(a) => a.text.get_or_insert_with(Default::default).anchor = anchor,
                crate::model::ShapeKind::TextBox(body) => body.anchor = anchor,
                _ => return Err(EditError::IndexOutOfRange),
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetShapeGeometry { at, size, offset } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            let FlowRunKind::Shape(shape) = &mut run.kind else { return Err(EditError::IndexOutOfRange) };
            if let Some(size) = size {
                shape.frame.width = size.width.max(12_700);
                shape.frame.height = size.height.max(12_700);
            }
            // A connector put somewhere by hand is no longer glued to the shapes it was attached to.
            if (size.is_some() || offset.is_some()) && crate::connector::is_connector(shape) && let Some(f) = shape.floating.as_mut() {
                f.glue = crate::model::Glue::default();
            }
            if let Some((x, y)) = offset
                && let Some(f) = shape.floating.as_mut()
            {
                f.offset_x = x;
                f.offset_y = y;
                f.align_h = None;
                f.align_v = None;
                shape.frame.x = x;
                shape.frame.y = y;
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::RerouteConnectors => {
            let moves = crate::connector::rerouted(doc);
            let mut undo = Vec::new();
            for (index, run, shape) in moves {
                let Some(Block::Paragraph(old)) = doc.body.get(index).cloned() else { continue };
                let mut p = old.clone();
                let Some(r) = p.runs.get_mut(run) else { continue };
                r.kind = FlowRunKind::Shape(Box::new(shape));
                doc.body[index] = Block::Paragraph(p);
                undo.push(FlowCommand::ReplaceBlocks { container: Container::Body, index, remove: 1, blocks: vec![Block::Paragraph(old)] });
            }
            undo.reverse();
            Ok(FlowCommand::Many(undo))
        }
        FlowCommand::PlaceShapeOnPage { at, size, x, y } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            let FlowRunKind::Shape(shape) = &mut run.kind else { return Err(EditError::IndexOutOfRange) };
            let Some(f) = shape.floating.as_mut() else { return Err(EditError::IndexOutOfRange) };
            shape.frame.width = size.width.max(12_700);
            shape.frame.height = size.height.max(12_700);
            f.relative_h = "page".into();
            f.relative_v = "page".into();
            f.align_h = None;
            f.align_v = None;
            f.offset_x = x;
            f.offset_y = y;
            shape.frame.x = x;
            shape.frame.y = y;
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetShapeRangeFormat { at, from, to, bold, italic, size, color } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            let FlowRunKind::Shape(shape) = &mut run.kind else { return Err(EditError::IndexOutOfRange) };
            let body = match &mut shape.kind {
                crate::model::ShapeKind::Auto(a) => a.text.as_mut(),
                crate::model::ShapeKind::TextBox(body) => Some(body),
                _ => None,
            }
            .ok_or(EditError::IndexOutOfRange)?;
            let format = crate::text::CharacterFormat {
                bold,
                italic,
                font_size: size.map(|v| v.clamp(crate::text::TextStyle::MIN_FONT_SIZE, crate::text::TextStyle::MAX_FONT_SIZE)),
                color,
                ..Default::default()
            };
            body.format_runs(from, to, &format);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetShapeTextFormat { at, bold, italic, size, color } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            let FlowRunKind::Shape(shape) = &mut run.kind else { return Err(EditError::IndexOutOfRange) };
            let body = match &mut shape.kind {
                crate::model::ShapeKind::Auto(a) => a.text.as_mut(),
                crate::model::ShapeKind::TextBox(body) => Some(body),
                _ => None,
            }
            .ok_or(EditError::IndexOutOfRange)?;
            if let Some(v) = bold {
                body.style.bold = v;
            }
            if let Some(v) = italic {
                body.style.italic = v;
            }
            if let Some(v) = size {
                body.style.font_size = v.clamp(crate::text::TextStyle::MIN_FONT_SIZE, crate::text::TextStyle::MAX_FONT_SIZE);
            }
            if let Some(v) = color {
                body.style.color = v;
            }
            for paragraph in &mut body.paragraphs {
                for r in &mut paragraph.runs {
                    if let Some(v) = bold {
                        r.style.bold = Some(v);
                    }
                    if let Some(v) = italic {
                        r.style.italic = Some(v);
                    }
                    if let Some(v) = size {
                        r.style.font_size = Some(v.clamp(crate::text::TextStyle::MIN_FONT_SIZE, crate::text::TextStyle::MAX_FONT_SIZE));
                    }
                    if let Some(v) = color {
                        r.style.color = Some(v);
                    }
                }
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::FormatShapeText { at, range, format } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            let FlowRunKind::Shape(shape) = &mut run.kind else { return Err(EditError::IndexOutOfRange) };
            let body = match &mut shape.kind {
                crate::model::ShapeKind::Auto(a) => a.text.as_mut(),
                crate::model::ShapeKind::TextBox(body) => Some(body),
                _ => None,
            }
            .ok_or(EditError::IndexOutOfRange)?;
            if let Some((from, to)) = range {
                body.format_runs(from, to, &format);
            } else {
                if let Some(v) = format.bold {
                    body.style.bold = v;
                }
                if let Some(v) = format.italic {
                    body.style.italic = v;
                }
                if let Some(v) = format.font_size {
                    body.style.font_size = v.clamp(crate::text::TextStyle::MIN_FONT_SIZE, crate::text::TextStyle::MAX_FONT_SIZE);
                }
                if let Some(v) = format.color {
                    body.style.color = v;
                }
                if let Some(ref f) = format.font {
                    body.style.font = f.clone();
                }
                let total = body.text().chars().count();
                body.format_runs(0, total, &format);
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetPictureWrap { at, wrap } => {
            use crate::model::PictureWrapType;
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let shape_id = doc.new_shape_id();
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            match &mut run.kind {
                FlowRunKind::Picture { media, size, look } => {
                    if wrap != PictureWrapType::Inline {
                        let (wrap_str, behind_doc) = match wrap {
                            PictureWrapType::Inline => unreachable!(),
                            PictureWrapType::Square => ("square", false),
                            PictureWrapType::Tight => ("tight", false),
                            PictureWrapType::Through => ("through", false),
                            PictureWrapType::TopAndBottom => ("topAndBottom", false),
                            PictureWrapType::BehindText => ("none", true),
                            PictureWrapType::InFrontOfText => ("none", false),
                        };
                        let anchor = crate::model::FloatingAnchor {
                            wrap: wrap_str.to_string(),
                            behind_doc,
                            wrap_polygon: matches!(wrap, PictureWrapType::Tight | PictureWrapType::Through)
                                .then(crate::model::WrapPolygon::default_rect_21600),
                            ..Default::default()
                        };
                        let pic = crate::model::Picture {
                            media: *media,
                            crop: look.crop,
                            stroke: look.stroke.clone(),
                            corner: 0,
                            duotone: look.duotone,
                            clear: None,
                            luminance: look.luminance,
                            clip: None,
                            shadow: None,
                            reflection: None,
                            soft_edge: look.soft_edge,
                            alt_text: look.alt_text.clone(),
                            decorative: look.decorative,
                            name: look.name.clone(),
                            geometry: look.geometry.clone(),
                            effects: look.effects, tile: None, fill_rect: None,
                            artistic_effect: None,
                        };
                        let shape = crate::model::Shape {
                            id: crate::model::ShapeId(shape_id as u64),
                            frame: crate::geometry::EmuRect::new(0, 0, size.width, size.height),
                            rotation: look.rotation,
                            flip_h: look.flip_h,
                            flip_v: look.flip_v,
                            kind: crate::model::ShapeKind::Picture(pic),
                            floating: Some(anchor),
                        };
                        run.kind = FlowRunKind::Shape(Box::new(shape));
                    }
                }
                FlowRunKind::Shape(shape) => {
                    // A drawn shape or a text box wraps as a picture does (not inline: it floats).
                    if !matches!(shape.kind, crate::model::ShapeKind::Picture(_)) {
                        if wrap == PictureWrapType::Inline || shape.floating.is_none() {
                            return Err(EditError::IndexOutOfRange);
                        }
                        let (wrap_str, behind_doc) = match wrap {
                            PictureWrapType::Inline => unreachable!(),
                            PictureWrapType::Square => ("square", false),
                            PictureWrapType::Tight => ("tight", false),
                            PictureWrapType::Through => ("through", false),
                            PictureWrapType::TopAndBottom => ("topAndBottom", false),
                            PictureWrapType::BehindText => ("none", true),
                            PictureWrapType::InFrontOfText => ("none", false),
                        };
                        let anchor = shape.floating.as_mut().unwrap();
                        anchor.wrap = wrap_str.to_string();
                        anchor.behind_doc = behind_doc;
                        if matches!(wrap, PictureWrapType::Tight | PictureWrapType::Through) && anchor.wrap_polygon.is_none() {
                            anchor.wrap_polygon = Some(crate::model::WrapPolygon::default_rect_21600());
                        }
                        list[index] = Block::Paragraph(p);
                        return Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] });
                    }
                    let crate::model::ShapeKind::Picture(pic) = &shape.kind else { return Err(EditError::IndexOutOfRange) };
                    if wrap == PictureWrapType::Inline {
                        let media = pic.media;
                        let size = crate::geometry::EmuSize::new(shape.frame.width, shape.frame.height);
                        let look = crate::flow::PictureLook {
                            rotation: shape.rotation,
                            flip_h: shape.flip_h,
                            flip_v: shape.flip_v,
                            crop: pic.crop,
                            stroke: pic.stroke.clone(),
                            luminance: pic.luminance,
                            duotone: pic.duotone,
                            alt_text: pic.alt_text.clone(),
                            decorative: pic.decorative,
                            name: pic.name.clone(),
                            linked_file: None,
                            soft_edge: pic.soft_edge,
                            effects: pic.effects,
                            geometry: pic.geometry.clone(),
                        };
                        run.kind = FlowRunKind::Picture { media, size, look };
                    } else {
                        let (wrap_str, behind_doc) = match wrap {
                            PictureWrapType::Inline => unreachable!(),
                            PictureWrapType::Square => ("square", false),
                            PictureWrapType::Tight => ("tight", false),
                            PictureWrapType::Through => ("through", false),
                            PictureWrapType::TopAndBottom => ("topAndBottom", false),
                            PictureWrapType::BehindText => ("none", true),
                            PictureWrapType::InFrontOfText => ("none", false),
                        };
                        let anchor = shape.floating.get_or_insert_with(Default::default);
                        anchor.wrap = wrap_str.to_string();
                        anchor.behind_doc = behind_doc;
                        if matches!(wrap, PictureWrapType::Tight | PictureWrapType::Through) && anchor.wrap_polygon.is_none() {
                            anchor.wrap_polygon = Some(crate::model::WrapPolygon::default_rect_21600());
                        }
                    }
                }
                _ => return Err(EditError::IndexOutOfRange),
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetPictureFloating { at, anchor } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let shape_id = doc.new_shape_id();
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            match &mut run.kind {
                FlowRunKind::Shape(shape) => {
                    if anchor.is_none() && matches!(&shape.kind, crate::model::ShapeKind::Picture(_)) {
                        let crate::model::ShapeKind::Picture(pic) = &shape.kind else { unreachable!() };
                        let media = pic.media;
                        let size = crate::geometry::EmuSize::new(shape.frame.width, shape.frame.height);
                        let look = crate::flow::PictureLook {
                            rotation: shape.rotation,
                            flip_h: shape.flip_h,
                            flip_v: shape.flip_v,
                            crop: pic.crop,
                            stroke: pic.stroke.clone(),
                            luminance: pic.luminance,
                            duotone: pic.duotone,
                            alt_text: pic.alt_text.clone(),
                            decorative: pic.decorative,
                            name: pic.name.clone(),
                            linked_file: None,
                            soft_edge: pic.soft_edge,
                            effects: pic.effects,
                            geometry: pic.geometry.clone(),
                        };
                        run.kind = FlowRunKind::Picture { media, size, look };
                    } else {
                        shape.floating = anchor;
                    }
                }
                FlowRunKind::Picture { media, size, look } if anchor.is_some() => {
                    let pic = crate::model::Picture {
                        media: *media,
                        crop: look.crop,
                        stroke: look.stroke.clone(),
                        corner: 0,
                        duotone: look.duotone,
                        clear: None,
                        luminance: look.luminance,
                        clip: None,
                        shadow: None,
                        reflection: None,
                        soft_edge: look.soft_edge,
                        alt_text: look.alt_text.clone(),
                        decorative: look.decorative,
                        name: look.name.clone(),
                        geometry: look.geometry.clone(),
                        effects: look.effects, tile: None, fill_rect: None,
                        artistic_effect: None,
                    };
                    let shape = crate::model::Shape {
                        id: crate::model::ShapeId(shape_id as u64),
                        frame: crate::geometry::EmuRect::new(0, 0, size.width, size.height),
                        rotation: look.rotation,
                        flip_h: look.flip_h,
                        flip_v: look.flip_v,
                        kind: crate::model::ShapeKind::Picture(pic),
                        floating: anchor,
                    };
                    run.kind = FlowRunKind::Shape(Box::new(shape));
                }
                _ => return Err(EditError::IndexOutOfRange),
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetPictureWrapPolygon { at, polygon } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let shape_id = doc.new_shape_id();
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { return Err(EditError::IndexOutOfRange) };
            match &mut run.kind {
                FlowRunKind::Shape(shape) => {
                    let anchor = shape.floating.get_or_insert_with(Default::default);
                    if anchor.wrap != "through" {
                        anchor.wrap = "tight".to_string();
                    }
                    anchor.behind_doc = false;
                    anchor.wrap_polygon = polygon;
                }
                FlowRunKind::Picture { media, size, look } => {
                    let anchor = crate::model::FloatingAnchor {
                        wrap: "tight".to_string(),
                        behind_doc: false,
                        wrap_polygon: polygon,
                        ..Default::default()
                    };
                    let pic = crate::model::Picture {
                        media: *media,
                        crop: look.crop,
                        stroke: look.stroke.clone(),
                        corner: 0,
                        duotone: look.duotone,
                        clear: None,
                        luminance: look.luminance,
                        clip: None,
                        shadow: None,
                        reflection: None,
                        soft_edge: look.soft_edge,
                        alt_text: look.alt_text.clone(),
                        decorative: look.decorative,
                        name: look.name.clone(),
                        geometry: look.geometry.clone(),
                        effects: look.effects, tile: None, fill_rect: None,
                        artistic_effect: None,
                    };
                    let shape = crate::model::Shape {
                        id: crate::model::ShapeId(shape_id as u64),
                        frame: crate::geometry::EmuRect::new(0, 0, size.width, size.height),
                        rotation: look.rotation,
                        flip_h: look.flip_h,
                        flip_v: look.flip_v,
                        kind: crate::model::ShapeKind::Picture(pic),
                        floating: Some(anchor),
                    };
                    run.kind = FlowRunKind::Shape(Box::new(shape));
                }
                _ => return Err(EditError::IndexOutOfRange),
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetParagraphSubdocument { para, subdoc } => {
            let (container, index) = locate(doc, para).ok_or(EditError::IndexOutOfRange)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(p) = &mut list[index] else { return Err(EditError::IndexOutOfRange) };
            let old = std::mem::replace(&mut p.subdoc, subdoc);
            Ok(FlowCommand::SetParagraphSubdocument { para, subdoc: old })
        }
        FlowCommand::AddHeader { footer } => {
            let container = if footer { Container::Footer } else { Container::Header };
            let style = styles.paragraph_style_named(if footer { "footer" } else { "header" }).map(str::to_string);
            let list = if footer { &doc.footer } else { &doc.header };
            if !list.is_empty() {
                return Err(EditError::IndexOutOfRange);
            }
            let old = section_state(doc, &section_paragraphs(doc));
            let mut p = FlowParagraph { id: doc.new_id(), style, ..FlowParagraph::default() };
            p.resolve(&styles);
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            list.push(Block::Paragraph(p));
            // The last section's own (no longer the previous section's).
            doc.linked[if footer { 1 } else { 0 }] = false;
            Ok(FlowCommand::SetSections { state: Box::new(old) })
        }
        FlowCommand::SetToc { before, replace, entries, bookmarks } => {
            let (container, index) = locate(doc, before).ok_or(EditError::IndexOutOfRange)?;
            if container != Container::Body || entries.is_empty() {
                return Err(EditError::IndexOutOfRange);
            }
            // The bookmarks first (on headings anywhere in the body).
            let old_body = doc.body.clone();
            for (id, name) in &bookmarks {
                let (c, i) = locate(doc, *id).ok_or(EditError::IndexOutOfRange)?;
                let list = blocks_mut(doc, c).ok_or(EditError::IndexOutOfRange)?;
                if let Block::Paragraph(p) = &mut list[i]
                    && !p.bookmarks.contains(name)
                {
                    p.bookmarks.push(name.clone());
                }
            }
            let list = &mut doc.body;
            let remove = if replace {
                // The old table of contents: to the paragraph its field ends in.
                let end = list[index..].iter().position(|b| matches!(b, Block::Paragraph(p) if p.field_end)).ok_or(EditError::IndexOutOfRange)?;
                end + 1
            } else {
                0
            };
            list.splice(index..index + remove, entries.into_iter().map(Block::Paragraph));
            let _ = container;
            Ok(FlowCommand::ReplaceBlocks { container: Container::Body, index: 0, remove: doc.body.len(), blocks: old_body })
        }
        FlowCommand::SetLink { from, to, link } => {
            let (container, index, last, from, to) = ordered(doc, from, to)?;
            if index != last || from.offset == to.offset {
                return Err(EditError::IndexOutOfRange);
            }
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let a = p.boundary(from.offset);
            let b = p.boundary(to.offset);
            for run in &mut p.runs[a..b] {
                run.link = link.clone();
            }
            p.normalize();
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::InsertPageNumber { at } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let (format, char_style, style) = old.format_at(at.offset);
            let mut p = FlowParagraph { runs: old.runs_before(at.offset), ..old.clone() };
            p.runs.push(FlowRun { kind: FlowRunKind::PageNumber, style, format, char_style, link: None, revision: None, show_field_code: false, field_locked: false, control: None });
            p.runs.extend(old.runs_after(at.offset));
            p.resolve(&styles);
            p.normalize();
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::InsertField { at, kind } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let (mut format, mut char_style, style) = old.format_at(at.offset);
            // Range markers are paragraph-level XML nodes, outside w:r; they
            // cannot carry inherited run properties such as w:rFonts/@w:hint.
            if matches!(kind,FlowRunKind::CommentStart(_)|FlowRunKind::CommentEnd(_)) {
                format=RunFormat::default();char_style=None;
            }
            // A text content control's prompt is grey.
            if matches!(kind, FlowRunKind::RichText { placeholder: true, .. } | FlowRunKind::PlainText { placeholder: true, .. } | FlowRunKind::BuildingBlockGallery { placeholder: true, .. } | FlowRunKind::Group { placeholder: true, .. })
                || matches!(&kind, FlowRunKind::RepeatingSection { items, selected, .. } if items.get(*selected).is_some_and(|s| s == crate::flow::DEFAULT_REPEATING_SECTION_ITEM_PROMPT)) {
                format.color = Some(crate::drawing::Rgb(0x808080));
            }
            let mut p = FlowParagraph { runs: old.runs_before(at.offset), ..old.clone() };
            p.runs.push(FlowRun { kind, style, format, char_style, link: None, revision: None, show_field_code: false, field_locked: false, control: None });
            p.runs.extend(old.runs_after(at.offset));
            p.resolve(&styles);
            p.normalize();
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::ToggleCheckBox { at } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut found = false;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    if let FlowRunKind::CheckBox { checked, .. } = &mut run.kind {
                        *checked = !*checked;
                        found = true;
                        break;
                    } else if let FlowRunKind::FormField(field) = &mut run.kind {
                        if let crate::flow::FormFieldKind::CheckBox { checked, .. } = &mut field.kind {
                            *checked = !*checked;
                            found = true;
                            break;
                        }
                    }
                }
                offset += len;
            }
            if !found {
                return Err(EditError::IndexOutOfRange);
            }
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ToggleCheckBox { at })
        }
        FlowCommand::SelectDropDownItem { at, index: new_index } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut old_selected = None;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    if let FlowRunKind::DropDownList { selected, items, .. } = &mut run.kind {
                        if new_index < items.len() {
                            old_selected = Some(*selected);
                            *selected = new_index;
                            break;
                        }
                    } else if let FlowRunKind::FormField(field) = &mut run.kind {
                        if let crate::flow::FormFieldKind::DropDown { selected, items, .. } = &mut field.kind {
                            if new_index < items.len() {
                                old_selected = Some(*selected);
                                *selected = new_index;
                                break;
                            }
                        }
                    }
                }
                offset += len;
            }
            let Some(old_idx) = old_selected else {
                return Err(EditError::IndexOutOfRange);
            };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::SelectDropDownItem { at, index: old_idx })
        }
        FlowCommand::SetDatePickerDate { at, full_date: new_full_date, text: new_text } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut old_info = None;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    if let FlowRunKind::DatePicker { full_date, text, .. } = &mut run.kind {
                        old_info = Some((full_date.clone(), text.clone()));
                        *full_date = new_full_date;
                        *text = new_text;
                        break;
                    }
                }
                offset += len;
            }
            let Some((prev_date, prev_text)) = old_info else {
                return Err(EditError::IndexOutOfRange);
            };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::SetDatePickerDate { at, full_date: prev_date, text: prev_text })
        }
        FlowCommand::SetContentControlText { at, text: new_text, placeholder: new_placeholder } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut old_text = None;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    if let Some(c) = run.control.as_ref().filter(|_| run.is_content_control()) {
                        if c.lock.keeps_content() {
                            return Err(EditError::ControlLocked);
                        }
                    }
                    if let FlowRunKind::RichText { text, placeholder }
                        | FlowRunKind::PlainText { text, placeholder }
                        | FlowRunKind::BuildingBlockGallery { text, placeholder, .. }
                        | FlowRunKind::Group { text, placeholder } = &mut run.kind {
                        old_text = Some((text.clone(), *placeholder));
                        *text = new_text;
                        *placeholder = new_placeholder;
                        break;
                    }
                    if let FlowRunKind::RepeatingSection { items, selected, .. } = &mut run.kind {
                        if let Some(item) = items.get_mut(*selected) {
                            old_text = Some((item.clone(), false));
                            *item = new_text;
                            break;
                        }
                    }
                }
                offset += len;
            }
            let Some((prev_text, prev_placeholder)) = old_text else {
                return Err(EditError::IndexOutOfRange);
            };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::SetContentControlText { at, text: prev_text, placeholder: prev_placeholder })
        }
        FlowCommand::SetFormFieldText { at, text: new_text } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut old_text = None;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    if let FlowRunKind::FormField(field) = &mut run.kind {
                        if let crate::flow::FormFieldKind::TextInput { value, max_length, .. } = &mut field.kind {
                            old_text = Some(value.clone());
                            let mut val = new_text;
                            if *max_length > 0 && val.chars().count() > *max_length {
                                val = val.chars().take(*max_length).collect();
                            }
                            *value = val;
                            break;
                        }
                    }
                }
                offset += len;
            }
            let Some(prev_text) = old_text else {
                return Err(EditError::IndexOutOfRange);
            };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::SetFormFieldText { at, text: prev_text })
        }
        FlowCommand::SetFormFieldMeta { at, name: new_name, enabled: new_enabled, calc_on_exit: new_calc_on_exit } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut old_meta = None;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    if let FlowRunKind::FormField(field) = &mut run.kind {
                        old_meta = Some((field.name.clone(), field.enabled, field.calc_on_exit));
                        field.name = new_name;
                        field.enabled = new_enabled;
                        field.calc_on_exit = new_calc_on_exit;
                        break;
                    }
                }
                offset += len;
            }
            let Some((prev_name, prev_enabled, prev_calc)) = old_meta else {
                return Err(EditError::IndexOutOfRange);
            };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::SetFormFieldMeta { at, name: prev_name, enabled: prev_enabled, calc_on_exit: prev_calc })
        }
        FlowCommand::SetBuildingBlockGalleryMeta { at, gallery: new_gallery, category: new_category } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut old_meta = None;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    if let FlowRunKind::BuildingBlockGallery { gallery, category, .. } = &mut run.kind {
                        old_meta = Some((gallery.clone(), category.clone()));
                        *gallery = new_gallery;
                        *category = new_category;
                        break;
                    }
                }
                offset += len;
            }
            let Some((prev_gallery, prev_category)) = old_meta else {
                return Err(EditError::IndexOutOfRange);
            };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::SetBuildingBlockGalleryMeta { at, gallery: prev_gallery, category: prev_category })
        }
        FlowCommand::InsertRepeatingSectionItem { at, index: target_idx, text: new_text, restore_selected } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut prev_selected = None;
            let mut actual_insert_at = None;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    if let Some(c) = run.control.as_ref().filter(|_| run.is_content_control()) {
                        if c.lock.keeps_content() {
                            return Err(EditError::ControlLocked);
                        }
                    }
                    if let FlowRunKind::RepeatingSection { items, selected, do_not_allow_insert_delete, .. } = &mut run.kind {
                        if *do_not_allow_insert_delete {
                            return Err(EditError::ControlLocked);
                        }
                        prev_selected = Some(*selected);
                        let insert_at = target_idx.min(items.len());
                        actual_insert_at = Some(insert_at);
                        items.insert(insert_at, new_text);
                        if let Some(sel) = restore_selected {
                            *selected = sel.min(items.len().saturating_sub(1));
                        } else {
                            *selected = insert_at;
                        }
                        break;
                    }
                }
                offset += len;
            }
            let (Some(prev_sel), Some(insert_at)) = (prev_selected, actual_insert_at) else {
                return Err(EditError::IndexOutOfRange);
            };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::DeleteRepeatingSectionItem { at, index: insert_at, restore_selected: Some(prev_sel) })
        }
        FlowCommand::DeleteRepeatingSectionItem { at, index: target_idx, restore_selected } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut removed_item = None;
            let mut prev_selected = None;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    if let Some(c) = run.control.as_ref().filter(|_| run.is_content_control()) {
                        if c.lock.keeps_content() {
                            return Err(EditError::ControlLocked);
                        }
                    }
                    if let FlowRunKind::RepeatingSection { items, selected, do_not_allow_insert_delete, .. } = &mut run.kind {
                        if *do_not_allow_insert_delete {
                            return Err(EditError::ControlLocked);
                        }
                        if items.len() <= 1 || target_idx >= items.len() {
                            return Err(EditError::IndexOutOfRange);
                        }
                        prev_selected = Some(*selected);
                        let removed = items.remove(target_idx);
                        if let Some(sel) = restore_selected {
                            *selected = sel.min(items.len().saturating_sub(1));
                        } else if *selected > target_idx {
                            *selected -= 1;
                        } else if *selected >= items.len() {
                            *selected = items.len().saturating_sub(1);
                        }
                        removed_item = Some(removed);
                        break;
                    }
                }
                offset += len;
            }
            let (Some(text), Some(prev_sel)) = (removed_item, prev_selected) else {
                return Err(EditError::IndexOutOfRange);
            };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::InsertRepeatingSectionItem { at, index: target_idx, text, restore_selected: Some(prev_sel) })
        }
        FlowCommand::SelectRepeatingSectionItem { at, index: target_idx } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut prev_selected = None;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    if let FlowRunKind::RepeatingSection { items, selected, .. } = &mut run.kind {
                        if target_idx < items.len() {
                            prev_selected = Some(std::mem::replace(selected, target_idx));
                            break;
                        }
                    }
                }
                offset += len;
            }
            let Some(prev) = prev_selected else {
                return Err(EditError::IndexOutOfRange);
            };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::SelectRepeatingSectionItem { at, index: prev })
        }
        FlowCommand::SetRepeatingSectionMeta { at, section_title: new_title, do_not_allow_insert_delete: new_flag } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut old_meta = None;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    if let FlowRunKind::RepeatingSection { section_title, do_not_allow_insert_delete, .. } = &mut run.kind {
                        old_meta = Some((section_title.clone(), *do_not_allow_insert_delete));
                        *section_title = new_title;
                        *do_not_allow_insert_delete = new_flag;
                        break;
                    }
                }
                offset += len;
            }
            let Some((prev_title, prev_flag)) = old_meta else {
                return Err(EditError::IndexOutOfRange);
            };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::SetRepeatingSectionMeta { at, section_title: prev_title, do_not_allow_insert_delete: prev_flag })
        }
        FlowCommand::GroupControl { at, text: new_text, placeholder: new_placeholder, control: new_control } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut replaced = None;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    let old_text = match &run.kind {
                        FlowRunKind::Text(t) => t.clone(),
                        FlowRunKind::Group { text, .. } => text.clone(),
                        _ => String::new(),
                    };
                    let old_placeholder = match &run.kind {
                        FlowRunKind::Group { placeholder, .. } => *placeholder,
                        _ => false,
                    };
                    let old_ctrl = run.control.clone();
                    run.kind = FlowRunKind::Group { text: new_text.clone(), placeholder: new_placeholder };
                    run.control = new_control.clone();
                    if new_placeholder {
                        run.format.color = Some(crate::drawing::Rgb(0x808080));
                    }
                    replaced = Some((old_text, old_placeholder, old_ctrl));
                    break;
                }
                offset += len;
            }
            if replaced.is_none() && p.runs.is_empty() {
                let (mut format, char_style, style) = old.format_at(at.offset);
                if new_placeholder {
                    format.color = Some(crate::drawing::Rgb(0x808080));
                }
                p.runs.push(FlowRun {
                    kind: FlowRunKind::Group { text: new_text.clone(), placeholder: new_placeholder },
                    style,
                    format,
                    char_style,
                    link: None,
                    revision: None,
                    show_field_code: false,
                    field_locked: false,
                    control: new_control.clone(),
                });
                replaced = Some((String::new(), false, None));
            } else if replaced.is_none() && !p.runs.is_empty() && at.offset >= p.len() {
                let run = p.runs.last_mut().unwrap();
                let old_text = match &run.kind {
                    FlowRunKind::Text(t) => t.clone(),
                    FlowRunKind::Group { text, .. } => text.clone(),
                    _ => String::new(),
                };
                let old_placeholder = match &run.kind {
                    FlowRunKind::Group { placeholder, .. } => *placeholder,
                    _ => false,
                };
                let old_ctrl = run.control.clone();
                run.kind = FlowRunKind::Group { text: new_text.clone(), placeholder: new_placeholder };
                run.control = new_control.clone();
                if new_placeholder {
                    run.format.color = Some(crate::drawing::Rgb(0x808080));
                }
                replaced = Some((old_text, old_placeholder, old_ctrl));
            }
            let Some((prev_text, prev_placeholder, prev_ctrl)) = replaced else {
                return Err(EditError::IndexOutOfRange);
            };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::UngroupControl { at, restore: Some((prev_text, prev_placeholder, prev_ctrl)) })
        }
        FlowCommand::UngroupControl { at, restore } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut un_grouped = None;
            for run in &mut p.runs {
                let len = run.len();
                if (at.offset >= offset && at.offset < offset + len) || (at.offset == offset && len == 0) {
                    let (g_text, g_placeholder) = match &run.kind {
                        FlowRunKind::Group { text, placeholder } => (text.clone(), *placeholder),
                        _ => {
                            offset += len;
                            continue;
                        }
                    };
                    if let Some(c) = run.control.as_ref() {
                        if c.lock.keeps_control() {
                            return Err(EditError::ControlLocked);
                        }
                    }
                    un_grouped = Some((g_text.clone(), g_placeholder, run.control.clone()));
                    let t = if let Some((ref r_text, _, _)) = restore {
                        r_text.clone()
                    } else if g_placeholder {
                        String::new()
                    } else {
                        g_text
                    };
                    let ctrl = if let Some((_, _, ref r_ctrl)) = restore {
                        r_ctrl.clone()
                    } else {
                        None
                    };
                    run.kind = FlowRunKind::Text(t);
                    run.control = ctrl;
                    if g_placeholder {
                        run.format.color = None;
                    }
                    break;
                }
                offset += len;
            }
            let Some((curr_text, curr_placeholder, curr_ctrl)) = un_grouped else {
                return Err(EditError::IndexOutOfRange);
            };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::GroupControl { at, text: curr_text, placeholder: curr_placeholder, control: curr_ctrl })
        }
        FlowCommand::SetControlInfo { at, info } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let mut offset = 0;
            let mut previous = None;
            for run in &mut p.runs {
                let len = run.len();
                if at.offset >= offset && (at.offset < offset + len || (at.offset == offset && len == 0)) && run.is_content_control() {
                    previous = Some(std::mem::replace(&mut run.control, info));
                    break;
                }
                offset += len;
            }
            let Some(previous) = previous else { return Err(EditError::IndexOutOfRange) };
            p.resolve(&styles);
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::SetControlInfo { at, info: previous })
        }
        FlowCommand::SetWatermark { watermark, sections } => {
            let old = std::mem::replace(&mut doc.watermark, watermark);
            let old_sections = std::mem::replace(&mut doc.section_watermarks, sections);
            Ok(FlowCommand::SetWatermark { watermark: old, sections: old_sections })
        }
        FlowCommand::SetPage { page } => {
            let old = std::mem::replace(&mut doc.page, page);
            Ok(FlowCommand::SetPage { page: old })
        }
        FlowCommand::DeleteTable { table } => {
            let (container, index) = locate(doc, table).ok_or(EditError::IndexOutOfRange)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            if !matches!(list[index], Block::Table(_)) {
                return Err(EditError::IndexOutOfRange);
            }
            let old = list.remove(index);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 0, blocks: vec![old] })
        }
        FlowCommand::EditTable { table, edit } => {
            let (container, index) = locate(doc, table).ok_or(EditError::IndexOutOfRange)?;
            let mut next = doc.new_id();
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Table(t) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let old = Block::Table(t.clone());
            let edited = edit_table(t, edit, &mut next, &styles)?;
            let blocks: Vec<Block> = edited.into_iter().map(Block::Table).collect();
            let count = blocks.len();
            list.splice(index..index + 1, blocks);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: count, blocks: vec![old] })
        }
        FlowCommand::InsertText { at, text, format } => {
            let form_design_mode = doc.form_design_mode;
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let mut next_id = doc.new_id();
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            if old.subdoc.as_ref().is_some_and(|s| s.locked) {
                return Err(EditError::ControlLocked);
            }
            // Text typed in a text content control is the control's text; the
            // first typing replaces the prompt (w:showingPlcHdr).
            if !text.is_empty() && !text.chars().any(char::is_control) {
                if let Some((run, start)) = old.control_at(at.offset) {
                    if old.runs[run].control.as_ref().is_some_and(|c| c.lock.keeps_content()) {
                        return Err(EditError::ControlLocked);
                    }
                    let mut p = old.clone();
                    let run_ref = &mut p.runs[run];
                    if let FlowRunKind::RichText { text: t, placeholder }
                        | FlowRunKind::PlainText { text: t, placeholder }
                        | FlowRunKind::BuildingBlockGallery { text: t, placeholder, .. }
                        | FlowRunKind::Group { text: t, placeholder } = &mut run_ref.kind {
                        if *placeholder {
                            if form_design_mode {
                                let byte = t.char_indices().nth(at.offset.saturating_sub(start)).map_or(t.len(), |(b, _)| b);
                                t.insert_str(byte, &text);
                            } else {
                                *t = text;
                                *placeholder = false;
                                // The prompt's look (PlaceholderText) is not the typed text's.
                                if run_ref.char_style.as_deref() == Some("PlaceholderText") {
                                    run_ref.char_style = None;
                                }
                                if run_ref.format.color == Some(crate::drawing::Rgb(0x808080)) {
                                    run_ref.format.color = None;
                                }
                            }
                        } else {
                            let byte = t.char_indices().nth(at.offset.saturating_sub(start)).map_or(t.len(), |(b, _)| b);
                            t.insert_str(byte, &text);
                        }
                    } else if let FlowRunKind::RepeatingSection { items, selected, .. } = &mut run_ref.kind {
                        if let Some(t) = items.get_mut(*selected) {
                            if t == crate::flow::DEFAULT_REPEATING_SECTION_ITEM_PROMPT {
                                if form_design_mode {
                                    let byte = t.char_indices().nth(at.offset.saturating_sub(start)).map_or(t.len(), |(b, _)| b);
                                    t.insert_str(byte, &text);
                                } else {
                                    *t = text;
                                    if run_ref.format.color == Some(crate::drawing::Rgb(0x808080)) {
                                        run_ref.format.color = None;
                                    }
                                }
                            } else {
                                let byte = t.char_indices().nth(at.offset.saturating_sub(start)).map_or(t.len(), |(b, _)| b);
                                t.insert_str(byte, &text);
                            }
                        }
                    }
                    p.resolve(&styles);
                    list[index] = Block::Paragraph(p);
                    return Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] });
                }
            }
            let (format, char_style, style) = match format {
                Some(f) => {
                    let (_, base) = styles.paragraph_base_in(old.style.as_deref(), old.table_style.as_deref(), &old.table_regions);
                    let style = styles.run_format(&base, f.char_style.as_deref(), &f.format).style();
                    (f.format, f.char_style, style)
                }
                None => old.format_at(at.offset),
            };
            // Text typed inside a link (not at its ends) is part of it.
            let link = old.link_inside(at.offset);
            let make = |kind: FlowRunKind| FlowRun { kind, style: style.clone(), format: format.clone(), char_style: char_style.clone(), link: link.clone(), revision: None, show_field_code: false, field_locked: false, control: None };
            let mut paragraphs = vec![FlowParagraph { runs: old.runs_before(at.offset), ..old.clone() }];
            // Enter at the end of a paragraph starts one of its style's next
            // style (w:next: 標準 after a heading); inside it, both halves
            // keep the style.
            let next_style = if at.offset == old.len() { old.style.as_ref().and_then(|s| styles.styles.get(s)).and_then(|s| s.next.clone()) } else { None };
            let mut buf = String::new();
            let flush = |buf: &mut String, p: &mut FlowParagraph| {
                if !buf.is_empty() {
                    p.runs.push(make(FlowRunKind::Text(std::mem::take(buf))));
                }
            };
            for c in text.chars() {
                match c {
                    '\n' | '\r' => {
                        flush(&mut buf, paragraphs.last_mut().unwrap());
                        // The new paragraph has none of the paragraph's marks
                        // (they stay with its element, the first part).
                        paragraphs.push(FlowParagraph {
                            id: next_id,
                            runs: Vec::new(),
                            section_break: crate::flow::SectionBreak::None,
                            section: None,
                            bookmarks: Vec::new(),
                            field_start: None,
                            field_end: false,
                            subdoc: None,
                            style: next_style.clone().or_else(|| old.style.clone()),
                            mark_format: format.clone(),
                            ..old.clone()
                        });
                        next_id += 1;
                    }
                    '\t' => {
                        let last = paragraphs.last_mut().unwrap();
                        flush(&mut buf, last);
                        last.runs.push(make(FlowRunKind::Tab));
                    }
                    '\u{b}' | '\u{2028}' => {
                        let last = paragraphs.last_mut().unwrap();
                        flush(&mut buf, last);
                        last.runs.push(make(FlowRunKind::LineBreak));
                    }
                    '\u{c}' => {
                        let last = paragraphs.last_mut().unwrap();
                        flush(&mut buf, last);
                        last.runs.push(make(FlowRunKind::PageBreak));
                    }
                    '\u{e}' => {
                        let last = paragraphs.last_mut().unwrap();
                        flush(&mut buf, last);
                        last.runs.push(make(FlowRunKind::ColumnBreak));
                    }
                    _ => buf.push(c),
                }
            }
            let last = paragraphs.last_mut().unwrap();
            flush(&mut buf, last);
            last.runs.extend(old.runs_after(at.offset));
            for p in &mut paragraphs {
                p.resolve(&styles);
                p.normalize();
            }
            let count = paragraphs.len();
            list.splice(index..index + 1, paragraphs.into_iter().map(Block::Paragraph));
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: count, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::Delete { from, to } => {
            let form_design_mode = doc.form_design_mode;
            let (container, first, last, from, to) = ordered(doc, from, to)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let old: Vec<Block> = list[first..=last].to_vec();
            for blk in &old {
                if let Block::Paragraph(p) = blk && p.subdoc.as_ref().is_some_and(|s| s.locked) {
                    return Err(EditError::ControlLocked);
                }
            }
            let (Block::Paragraph(a), Block::Paragraph(b)) = (&old[0], &old[old.len() - 1]) else { return Err(EditError::IndexOutOfRange) };
            // A locked content control is not deleted or edited.
            for (i, block) in old.iter().enumerate() {
                let Block::Paragraph(p) = block else { continue };
                let start = if i == 0 { from.offset } else { 0 };
                let end = if i + 1 == old.len() { to.offset } else { p.len() };
                let mut at = 0;
                for run in &p.runs {
                    let len = run.len();
                    if let Some(c) = run.control.as_ref().filter(|_| run.is_content_control()) {
                        let overlap = at.max(start) < (at + len).min(end);
                        let covers = start < end && start <= at && end >= at + len;
                        let inside = !covers && start < end && start >= at && end <= at + len;
                        let partial = !covers && !inside && overlap;
                        if (covers && c.lock.keeps_control()) || (inside && c.lock.keeps_content()) || (partial && c.lock != crate::flow::ControlLock::None) {
                            return Err(EditError::ControlLocked);
                        }
                    }
                    at += len;
                }
            }
            // Deleting inside a text content control shortens its text; when
            // nothing is left the prompt shows again.
            if first == last && from.offset < to.offset {
                if let Some((run, start)) = a.control_at(to.offset).filter(|&(run, start)| {
                    start <= from.offset && (
                        form_design_mode
                        || matches!(&a.runs[run].kind, FlowRunKind::RichText { placeholder: false, .. } | FlowRunKind::PlainText { placeholder: false, .. } | FlowRunKind::BuildingBlockGallery { placeholder: false, .. } | FlowRunKind::Group { placeholder: false, .. })
                        || matches!(&a.runs[run].kind, FlowRunKind::RepeatingSection { .. })
                    )
                }) {
                    let mut p = a.clone();
                    let run_ref = &mut p.runs[run];
                    if let FlowRunKind::RepeatingSection { items, selected, .. } = &mut run_ref.kind {
                        if let Some(t) = items.get_mut(*selected) {
                            let kept: String = t.chars().enumerate().filter(|&(i, _)| i + start < from.offset || i + start >= to.offset).map(|(_, c)| c).collect();
                            if kept.is_empty() {
                                if !form_design_mode {
                                    *t = crate::flow::DEFAULT_REPEATING_SECTION_ITEM_PROMPT.to_string();
                                    run_ref.format.color = Some(crate::drawing::Rgb(0x808080));
                                } else {
                                    *t = kept;
                                }
                            } else {
                                *t = kept;
                            }
                        }
                    } else {
                        let is_gallery = matches!(&run_ref.kind, FlowRunKind::BuildingBlockGallery { .. });
                        let is_group = matches!(&run_ref.kind, FlowRunKind::Group { .. });
                        if let FlowRunKind::RichText { text: t, placeholder }
                            | FlowRunKind::PlainText { text: t, placeholder }
                            | FlowRunKind::BuildingBlockGallery { text: t, placeholder, .. }
                            | FlowRunKind::Group { text: t, placeholder } = &mut run_ref.kind {
                            let kept: String = t.chars().enumerate().filter(|&(i, _)| i + start < from.offset || i + start >= to.offset).map(|(_, c)| c).collect();
                            if kept.is_empty() {
                                if !form_design_mode {
                                    *t = if is_gallery {
                                        crate::flow::DEFAULT_BUILDING_BLOCK_GALLERY_PROMPT.to_string()
                                    } else if is_group {
                                        crate::flow::DEFAULT_GROUP_PROMPT.to_string()
                                    } else {
                                        crate::flow::DEFAULT_CONTROL_PROMPT.to_string()
                                    };
                                    *placeholder = true;
                                    run_ref.format.color = Some(crate::drawing::Rgb(0x808080));
                                } else {
                                    *t = kept;
                                }
                            } else {
                                *t = kept;
                            }
                        }
                    }
                    p.resolve(&styles);
                    list[first] = Block::Paragraph(p);
                    return Ok(FlowCommand::ReplaceBlocks { container, index: first, remove: 1, blocks: old });
                }
            }
            let mut joined = a.clone();
            let mut runs = a.runs_before(from.offset);
            runs.extend(b.runs_after(to.offset));
            joined.runs = runs;
            joined.resolve(&styles);
            joined.normalize();
            list.splice(first..=last, [Block::Paragraph(joined)]);
            Ok(FlowCommand::ReplaceBlocks { container, index: first, remove: 1, blocks: old })
        }
        FlowCommand::SetRunFormat { from, to, change } => {
            let (container, first, last, from, to) = ordered(doc, from, to)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let old: Vec<Block> = list[first..=last].to_vec();
            for (i, block) in list[first..=last].iter_mut().enumerate() {
                let Block::Paragraph(p) = block else { continue };
                let start = if i == 0 { from.offset } else { 0 };
                let end = if first + i == last { to.offset } else { p.len() };
                p.format_range(start, end, &change);
                if start == 0 && end == p.len() {
                    change.apply(&mut p.mark_format);
                }
                p.resolve(&styles);
                p.normalize();
            }
            Ok(FlowCommand::ReplaceBlocks { container, index: first, remove: last - first + 1, blocks: old })
        }
        FlowCommand::SetCharStyle { from, to, style } => {
            let (container, first, last, from, to) = ordered(doc, from, to)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let old: Vec<Block> = list[first..=last].to_vec();
            for (i, block) in list[first..=last].iter_mut().enumerate() {
                let Block::Paragraph(p) = block else { continue };
                let start = if i == 0 { from.offset } else { 0 };
                let end = if first + i == last { to.offset } else { p.len() };
                if start < end {
                    let a = p.boundary(start);
                    let b = p.boundary(end);
                    for run in &mut p.runs[a..b] {
                        run.char_style = style.clone();
                    }
                }
                p.resolve(&styles);
                p.normalize();
            }
            Ok(FlowCommand::ReplaceBlocks { container, index: first, remove: last - first + 1, blocks: old })
        }
        FlowCommand::SetNoteStyle { at, style } => {
            let (container, index) = locate(doc, at.para).ok_or(EditError::IndexOutOfRange)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(p) = &mut list[index] else { return Err(EditError::IndexOutOfRange) };
            let old = p.clone();
            let mut pos = 0;
            for r in &mut p.runs {
                if pos == at.offset && r.len() == 1 {
                    r.char_style = style.clone();
                    break;
                }
                pos += r.len();
            }
            p.resolve(&styles);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::Many(commands) => {
            let mut inverses = Vec::new();
            for c in commands {
                match apply_to(doc, c) {
                    Ok(inv) => inverses.push(inv),
                    Err(e) => {
                        // Undo what was done before failing.
                        for inv in inverses.into_iter().rev() {
                            let _ = apply_to(doc, inv);
                        }
                        return Err(e);
                    }
                }
            }
            inverses.reverse();
            Ok(FlowCommand::Many(inverses))
        }
        FlowCommand::SetTrackChanges(on) => {
            if doc.lock_tracking && !on {
                return Err(EditError::TrackingLocked);
            }
            if on {
                if doc.interaction_mode == crate::flow::DocumentInteractionMode::Editing {
                    doc.interaction_mode = crate::flow::DocumentInteractionMode::Reviewing;
                }
            } else if doc.interaction_mode == crate::flow::DocumentInteractionMode::Reviewing {
                doc.interaction_mode = crate::flow::DocumentInteractionMode::Editing;
            }
            Ok(FlowCommand::SetTrackChanges(std::mem::replace(&mut doc.track_changes, on)))
        }
        FlowCommand::MarkRevision { from, to, revision } => {
            let revision = revision.clone();
            each_part(doc, from, to, &styles, move |p, a, b| p.set_revision(a, b, |_| Some(revision.clone()).flatten()))
        }
        FlowCommand::TrackDelete { from, to, author, date } => each_part(doc, from, to, &styles, move |p, a, b| {
            let a = p.boundary(a);
            let b = p.boundary(b);
            let mut kept = Vec::new();
            for (i, mut run) in std::mem::take(&mut p.runs).into_iter().enumerate() {
                if i >= a && i < b {
                    match run.revision.as_ref().map(|r| &r.kind) {
                        Some(crate::flow::RevisionKind::Insert | crate::flow::RevisionKind::MoveTo) => continue,
                        Some(crate::flow::RevisionKind::Delete | crate::flow::RevisionKind::MoveFrom) => {}
                        _ => run.revision = Some(crate::flow::Revision { kind: crate::flow::RevisionKind::Delete, author: author.clone(), date: date.clone() }),
                    }
                }
                kept.push(run);
            }
            p.runs = kept;
        }),
        FlowCommand::TrackFormat { from, to, author, date } => each_part(doc, from, to, &styles, move |p, a, b| {
            p.set_revision(a, b, |run| match &run.revision {
                None => Some(crate::flow::Revision { kind: crate::flow::RevisionKind::Format(run.format.clone()), author: author.clone(), date: date.clone() }),
                other => other.clone(),
            })
        }),
        FlowCommand::SetComments(comments) => Ok(FlowCommand::SetComments(std::mem::replace(&mut doc.comments, comments))),
        FlowCommand::SetNotes { footnotes, endnotes } => {
            let old_f = std::mem::replace(&mut doc.footnotes, footnotes);
            let old_e = std::mem::replace(&mut doc.endnotes, endnotes);
            Ok(FlowCommand::SetNotes { footnotes: old_f, endnotes: old_e })
        }
        FlowCommand::SetSections { state } => {
            let mut ids: Vec<u32> = state.breaks.iter().map(|(id, _, _)| *id).collect();
            ids.extend(section_paragraphs(doc));
            let old = section_state(doc, &ids);
            set_section_state(doc, &state, &ids, &styles);
            Ok(FlowCommand::SetSections { state: Box::new(old) })
        }
        FlowCommand::SetParagraphFormat { first, last, change: ParaChange::SectionBreak(kind) } => {
            // A section break changes the sections around it too.
            let mut new = section_state(doc, &section_paragraphs(doc));
            let ids: Vec<u32> = {
                let (container, a, b, _, _) = ordered(doc, FlowPos::new(first, 0), FlowPos::new(last, 0))?;
                if container != Container::Body {
                    return Err(EditError::IndexOutOfRange);
                }
                doc.body[a..=b].iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).collect()
            };
            for id in ids {
                with_section_break(doc, &mut new, id, kind);
            }
            apply_to(doc, FlowCommand::SetSections { state: Box::new(new) })
        }
        FlowCommand::SetParagraphFormat { first, last, change } => {
            let (container, a, b, _, _) = ordered(doc, FlowPos::new(first, 0), FlowPos::new(last, 0))?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let old: Vec<Block> = list[a..=b].to_vec();
            for block in &mut list[a..=b] {
                let Block::Paragraph(p) = block else { continue };
                change.apply(p, &styles);
                p.resolve(&styles);
            }
            Ok(FlowCommand::ReplaceBlocks { container, index: a, remove: b - a + 1, blocks: old })
        }
        FlowCommand::SetPageColor(color) => {
            let old = std::mem::replace(&mut doc.page_color, color);
            Ok(FlowCommand::SetPageColor(old))
        }
        FlowCommand::SetShowFieldCodes(show) => {
            let old = std::mem::replace(&mut doc.show_field_codes, show);
            Ok(FlowCommand::SetShowFieldCodes(old))
        }
        FlowCommand::SetFormDesignMode(mode) => {
            let old = std::mem::replace(&mut doc.form_design_mode, mode);
            Ok(FlowCommand::SetFormDesignMode(old))
        }
        FlowCommand::SetDocProperties(props) => {
            let old = std::mem::replace(&mut doc.properties, props);
            Ok(FlowCommand::SetDocProperties(old))
        }
        FlowCommand::SetMarkAsFinal(mark) => {
            let old = std::mem::replace(&mut doc.mark_as_final, mark);
            Ok(FlowCommand::SetMarkAsFinal(old))
        }
        FlowCommand::SetReadOnlyRecommended(ro) => {
            let old = std::mem::replace(&mut doc.read_only_recommended, ro);
            Ok(FlowCommand::SetReadOnlyRecommended(old))
        }
        FlowCommand::SetKinsokuLists { start, end } => {
            let old_start = std::mem::replace(&mut doc.no_line_start, start);
            let old_end = std::mem::replace(&mut doc.no_line_end, end);
            Ok(FlowCommand::SetKinsokuLists { start: old_start, end: old_end })
        }
        FlowCommand::SetStrictFirstAndLastChars(strict) => {
            let old = std::mem::replace(&mut doc.strict_first_and_last_chars, strict);
            Ok(FlowCommand::SetStrictFirstAndLastChars(old))
        }
        FlowCommand::SetThemeFontLang(theme_lang) => {
            let old = std::mem::replace(&mut doc.theme_font_lang, theme_lang);
            Ok(FlowCommand::SetThemeFontLang(old))
        }
        FlowCommand::SetUpdateFields(uf) => {
            let old = std::mem::replace(&mut doc.update_fields, uf);
            Ok(FlowCommand::SetUpdateFields(old))
        }
        FlowCommand::SetNoteSeparator { endnote, kind } => {
            let old = std::mem::replace(&mut doc.note_separators[usize::from(endnote)], kind);
            Ok(FlowCommand::SetNoteSeparator { endnote, kind: old })
        }
        FlowCommand::SetMailMergeType(kind) => {
            let old = std::mem::replace(&mut doc.mail_merge_type, kind);
            Ok(FlowCommand::SetMailMergeType(old))
        }
        FlowCommand::SetMailMergeFieldMap(field_map) => {
            let old = std::mem::replace(&mut doc.mail_merge_field_map, field_map);
            Ok(FlowCommand::SetMailMergeFieldMap(old))
        }
        FlowCommand::SetDocVariables(vars) => {
            let old = std::mem::replace(&mut doc.doc_variables, vars);
            Ok(FlowCommand::SetDocVariables(old))
        }
        FlowCommand::SetLockTracking { locked, track_changes } => {
            let old_locked = std::mem::replace(&mut doc.lock_tracking, locked);
            let old_tracked = std::mem::replace(&mut doc.track_changes, track_changes);
            doc.protection = if locked { crate::flow::DocumentProtection::TrackedChanges } else { crate::flow::DocumentProtection::None };
            Ok(FlowCommand::SetLockTracking { locked: old_locked, track_changes: old_tracked })
        }
        FlowCommand::SetViewMode(mode) => {
            let old = std::mem::replace(&mut doc.view_mode, mode);
            Ok(FlowCommand::SetViewMode(old))
        }
        FlowCommand::SetInteractionMode(mode) => {
            let old = doc.interaction_mode;
            doc.interaction_mode = mode;
            match mode {
                crate::flow::DocumentInteractionMode::Editing => {
                    if doc.view_mode == crate::flow::DocumentViewMode::Reading {
                        doc.view_mode = crate::flow::DocumentViewMode::Print;
                    }
                    if !doc.lock_tracking {
                        doc.track_changes = false;
                    }
                }
                crate::flow::DocumentInteractionMode::Reviewing => {
                    if doc.view_mode == crate::flow::DocumentViewMode::Reading {
                        doc.view_mode = crate::flow::DocumentViewMode::Print;
                    }
                    doc.track_changes = true;
                }
                crate::flow::DocumentInteractionMode::Viewing => {}
            }
            Ok(FlowCommand::SetInteractionMode(old))
        }
        FlowCommand::SetDocumentProtection(prot) => {
            let old = std::mem::replace(&mut doc.protection, prot);
            doc.lock_tracking = prot == crate::flow::DocumentProtection::TrackedChanges;
            if prot == crate::flow::DocumentProtection::TrackedChanges {
                doc.track_changes = true;
            }
            Ok(FlowCommand::SetDocumentProtection(old))
        }
        FlowCommand::SetFormattingProtection(prot) => {
            let old = std::mem::replace(&mut doc.formatting_protection, prot);
            Ok(FlowCommand::SetFormattingProtection(old))
        }
        FlowCommand::CompressPicture { at, media, reset_crop } => {
            let (container, index, _, at, _) = ordered(doc, at, at)?;
            let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
            let Block::Paragraph(old) = list[index].clone() else { return Err(EditError::IndexOutOfRange) };
            let mut p = old.clone();
            let i = p.boundary(at.offset);
            match p.runs.get_mut(i) {
                Some(FlowRun { kind: FlowRunKind::Picture { media: m, look, .. } | FlowRunKind::PictureControl { media: m, look, .. }, .. }) => {
                    *m = media;
                    if reset_crop {
                        look.crop = Default::default();
                    }
                }
                Some(FlowRun { kind: FlowRunKind::Shape(shape), .. }) => {
                    if let crate::model::ShapeKind::Picture(pic) = &mut shape.kind {
                        pic.media = media;
                        if reset_crop {
                            pic.crop = Default::default();
                        }
                    }
                }
                _ => return Err(EditError::IndexOutOfRange),
            }
            list[index] = Block::Paragraph(p);
            Ok(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(old)] })
        }
        FlowCommand::SetImageQualitySettings { default_dpi, discard_crop, do_not_compress } => {
            let old_dpi = std::mem::replace(&mut doc.default_image_dpi, default_dpi);
            let old_discard = std::mem::replace(&mut doc.discard_image_editing_data, discard_crop);
            let old_compress = std::mem::replace(&mut doc.do_not_compress_images, do_not_compress);
            Ok(FlowCommand::SetImageQualitySettings { default_dpi: old_dpi, discard_crop: old_discard, do_not_compress: old_compress })
        }
        FlowCommand::SetSaveFrequency(frequency) => {
            let old = std::mem::replace(&mut doc.save_frequency, frequency);
            Ok(FlowCommand::SetSaveFrequency(old))
        }
        FlowCommand::SetFontEmbedding { embed_true_type_fonts, embed_system_fonts, save_subset_fonts } => {
            let old_true_type = std::mem::replace(&mut doc.embed_true_type_fonts, embed_true_type_fonts);
            let old_system = std::mem::replace(&mut doc.embed_system_fonts, embed_system_fonts);
            let old_subset = std::mem::replace(&mut doc.save_subset_fonts, save_subset_fonts);
            Ok(FlowCommand::SetFontEmbedding {
                embed_true_type_fonts: old_true_type,
                embed_system_fonts: old_system,
                save_subset_fonts: old_subset,
            })
        }
        FlowCommand::SetCompatibilityOptions { compatibility_mode, use_fe_layout, do_not_expand_shift_return } => {
            let old_mode = std::mem::replace(&mut doc.compatibility_mode, compatibility_mode);
            let old_fe = std::mem::replace(&mut doc.use_fe_layout, use_fe_layout);
            let old_shift = std::mem::replace(&mut doc.do_not_expand_shift_return, do_not_expand_shift_return);
            Ok(FlowCommand::SetCompatibilityOptions {
                compatibility_mode: old_mode,
                use_fe_layout: old_fe,
                do_not_expand_shift_return: old_shift,
            })
        }
        FlowCommand::AttachTemplate { template, link_styles } => {
            let old_template = std::mem::replace(&mut doc.attached_template, template);
            let old_link = std::mem::replace(&mut doc.link_styles, link_styles);
            Ok(FlowCommand::AttachTemplate {
                template: old_template,
                link_styles: old_link,
            })
        }
        FlowCommand::UpdateStyles(new_styles) => {
            let old_styles = std::mem::replace(&mut doc.styles, new_styles);
            Ok(FlowCommand::UpdateStyles(old_styles))
        }
        FlowCommand::SetShapeGroups(new_groups) => {
            let old_groups = std::mem::replace(&mut doc.shape_groups, new_groups);
            Ok(FlowCommand::SetShapeGroups(old_groups))
        }
    }
}

/// The sections of a document: the paragraphs that end one (their break
/// and setup) and the last section's setup, headers and footers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SectionState {
    pub breaks: Vec<(u32, crate::flow::SectionBreak, Option<Arc<crate::flow::SectionSetup>>)>,
    pub page: crate::flow::PageSetup,
    /// The last section's headers and footers, by [`crate::flow::PartKind::index`].
    pub parts: [Vec<Block>; 6],
    pub linked: [bool; 6],
    pub title_page: bool,
    pub even_and_odd: bool,
    pub page_number_format: Option<String>,
    pub page_chapter: Option<(u32, String)>,
    pub restart_numbering: bool,
}

impl SectionState {
    /// The setup of the section that paragraph `id` ends (its index in
    /// `breaks`), if it ends one.
    fn index_of(&self, id: u32) -> Option<usize> {
        self.breaks.iter().position(|(i, _, _)| *i == id)
    }
}

/// The body paragraphs that end a section, in order.
pub fn section_paragraphs(doc: &FlowDocument) -> Vec<u32> {
    doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b && (p.section_break != crate::flow::SectionBreak::None || p.section.is_some()) { Some(p.id) } else { None }).collect()
}

/// The sections now, with the breaks of paragraphs `ids` (in body order).
pub fn section_state(doc: &FlowDocument, ids: &[u32]) -> SectionState {
    let mut breaks = Vec::new();
    for b in &doc.body {
        if let Block::Paragraph(p) = b
            && ids.contains(&p.id)
            && !breaks.iter().any(|(i, _, _)| *i == p.id)
        {
            breaks.push((p.id, p.section_break, p.section.clone()));
        }
    }
    SectionState {
        breaks,
        page: doc.page,
        parts: [doc.header.clone(), doc.footer.clone(), doc.header_first.clone(), doc.footer_first.clone(), doc.header_even.clone(), doc.footer_even.clone()],
        linked: doc.linked,
        title_page: doc.title_page,
        even_and_odd: doc.even_and_odd,
        page_number_format: doc.page_number_format.clone(),
        page_chapter: doc.page_chapter.clone(),
        restart_numbering: doc.restart_numbering,
    }
}

/// Give the document the sections of `state`; paragraphs of `ids` that
/// `state` does not name end no section.
fn set_section_state(doc: &mut FlowDocument, state: &SectionState, ids: &[u32], styles: &Arc<FlowStyles>) {
    for b in &mut doc.body {
        if let Block::Paragraph(p) = b {
            if let Some(i) = state.index_of(p.id) {
                let (_, kind, section) = &state.breaks[i];
                p.section_break = *kind;
                p.section = section.clone();
            } else if ids.contains(&p.id) {
                p.section_break = crate::flow::SectionBreak::None;
                p.section = None;
            }
        }
    }
    doc.page = state.page;
    let [h, f, hf, ff, he, fe] = state.parts.clone();
    doc.header = h;
    doc.footer = f;
    doc.header_first = hf;
    doc.footer_first = ff;
    doc.header_even = he;
    doc.footer_even = fe;
    doc.linked = state.linked;
    doc.title_page = state.title_page;
    doc.even_and_odd = state.even_and_odd;
    doc.page_number_format = state.page_number_format.clone();
    doc.page_chapter = state.page_chapter.clone();
    doc.restart_numbering = state.restart_numbering;
    let _ = styles;
}

/// The command clearing all headers and footers from the document and sections.
pub fn clear_all_headers_footers(doc: &FlowDocument) -> Option<FlowCommand> {
    let ids = section_paragraphs(doc);
    let mut state = section_state(doc, &ids);
    state.parts = [Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for (_, _, sec) in &mut state.breaks {
        if let Some(sec_ref) = sec {
            let mut s = (**sec_ref).clone();
            s.parts = [Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new()), Some(Vec::new())];
            *sec = Some(std::sync::Arc::new(s));
        }
    }
    Some(FlowCommand::SetSections { state: Box::new(state) })
}

/// Build a [`FlowCommand::ReplaceBlocks`] that mutates the [`crate::text::TextBody`] of the shape at `at`.
pub fn mutate_shape_text_body<F>(doc: &FlowDocument, at: FlowPos, f: F) -> Option<FlowCommand>
where
    F: FnOnce(&mut crate::text::TextBody),
{
    let (container, index, _, at, _) = ordered(doc, at, at).ok()?;
    let list = blocks(doc, container)?;
    let Block::Paragraph(mut p) = list.get(index)?.clone() else { return None };
    let i = p.boundary(at.offset);
    let run = p.runs.get_mut(i)?;
    let FlowRunKind::Shape(shape) = &mut run.kind else { return None };
    let body = match &mut shape.kind {
        crate::model::ShapeKind::Auto(a) => a.text.as_mut()?,
        crate::model::ShapeKind::TextBox(body) => body,
        _ => return None,
    };
    f(body);
    Some(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(p)] })
}

/// Build [`FlowCommand::ReplaceBlocks`] commands that mutate the [`crate::text::TextBody`] of all shapes at `positions`.
/// Shapes anchored in the same paragraph are updated together in a single `ReplaceBlocks` so earlier edits are not overwritten.
pub fn mutate_shapes_text_bodies<F>(doc: &FlowDocument, positions: &[FlowPos], mut f: F) -> Vec<FlowCommand>
where
    F: FnMut(&mut crate::text::TextBody),
{
    let mut groups: Vec<(Container, usize, Vec<FlowPos>)> = Vec::new();
    for &pos in positions {
        let Ok((container, index, _, at, _)) = ordered(doc, pos, pos) else { continue };
        if let Some((_, _, list)) = groups.iter_mut().find(|(c, idx, _)| *c == container && *idx == index) {
            if !list.contains(&at) {
                list.push(at);
            }
        } else {
            groups.push((container, index, vec![at]));
        }
    }
    let mut commands = Vec::new();
    for (container, index, ats) in groups {
        let Some(list) = blocks(doc, container) else { continue };
        let Some(Block::Paragraph(mut p)) = list.get(index).cloned() else { continue };
        let mut any = false;
        for at in ats {
            let i = p.boundary(at.offset);
            let Some(run) = p.runs.get_mut(i) else { continue };
            let FlowRunKind::Shape(shape) = &mut run.kind else { continue };
            let body = match &mut shape.kind {
                crate::model::ShapeKind::Auto(a) => a.text.as_mut(),
                crate::model::ShapeKind::TextBox(body) => Some(body),
                _ => None,
            };
            if let Some(body) = body {
                f(body);
                any = true;
            }
        }
        if any {
            commands.push(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(p)] });
        }
    }
    commands
}

/// `state` with paragraph `id` ending a section of `kind` (or none). A new
/// section takes the setup of the section it is cut from and its own
/// headers and footers (that section's become the same as the new one's,
/// 前と同じ, as Word writes it: both refer to the same parts).
pub fn with_section_break(doc: &FlowDocument, state: &mut SectionState, id: u32, kind: crate::flow::SectionBreak) {
    use crate::flow::SectionBreak;
    let order: Vec<u32> = doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).collect();
    let Some(at) = order.iter().position(|i| *i == id) else { return };
    match (state.index_of(id), kind) {
        (Some(i), SectionBreak::None) => {
            state.breaks.remove(i);
        }
        (Some(i), kind) => state.breaks[i].1 = kind,
        (None, SectionBreak::None) => {}
        (None, kind) => {
            // The section the paragraph is in: the next break after it, or the last.
            let next = order[at + 1..].iter().find_map(|i| state.index_of(*i));
            let mut setup = match next {
                Some(n) => state.breaks[n].2.as_deref().cloned().unwrap_or_default(),
                None => crate::flow::SectionSetup {
                    page: state.page,
                    restart_numbering: state.restart_numbering,
                    page_number_format: state.page_number_format.clone(),
                    page_chapter: state.page_chapter.clone(),
                    title_page: state.title_page,
                    parts: Default::default(),
                },
            };
            for k in 0..6 {
                match next {
                    Some(n) => {
                        let owner = state.breaks[n].2.as_mut().map(Arc::make_mut);
                        setup.parts[k] = owner.and_then(|o| o.parts[k].take());
                    }
                    None => {
                        if !state.linked[k] {
                            setup.parts[k] = Some(std::mem::take(&mut state.parts[k]));
                            state.linked[k] = true;
                        } else {
                            setup.parts[k] = None;
                        }
                    }
                }
            }
            // In order among the breaks.
            let index = state.breaks.iter().position(|(i, _, _)| order.iter().position(|o| o == i) > Some(at)).unwrap_or(state.breaks.len());
            state.breaks.insert(index, (id, kind, Some(Arc::new(setup))));
        }
    }
}

/// The section paragraph `id` is in: the paragraph that ends it, or 0 for
/// the last section.
pub fn section_of(doc: &FlowDocument, id: u32) -> u32 {
    fn contains(b: &Block, id: u32) -> bool {
        match b {
            Block::Paragraph(p) => p.id == id,
            Block::Table(t) => t.id == id || t.rows.iter().any(|r| r.cells.iter().any(|c| c.blocks.iter().any(|b| contains(b, id)))),
        }
    }
    let Some(at) = doc.body.iter().position(|b| contains(b, id)) else { return 0 };
    doc.body[at..].iter().find_map(|b| if let Block::Paragraph(p) = b && p.section.is_some() { Some(p.id) } else { None }).unwrap_or(0)
}

/// The sections in order (the paragraphs that end them, then 0).
pub fn sections(doc: &FlowDocument) -> Vec<u32> {
    let mut out: Vec<u32> = doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b && p.section.is_some() { Some(p.id) } else { None }).collect();
    out.push(0);
    out
}

/// A section's setup (the last one's from the document's fields).
pub fn section_setup(doc: &FlowDocument, section: u32) -> Option<crate::flow::SectionSetup> {
    if section == 0 {
        let lists = [&doc.header, &doc.footer, &doc.header_first, &doc.footer_first, &doc.header_even, &doc.footer_even];
        return Some(crate::flow::SectionSetup {
            page: doc.page,
            restart_numbering: doc.restart_numbering,
            page_number_format: doc.page_number_format.clone(),
            page_chapter: doc.page_chapter.clone(),
            title_page: doc.title_page,
            parts: std::array::from_fn(|k| (!doc.linked[k]).then(|| lists[k].clone())),
        });
    }
    paragraph(doc, section)?.section.as_deref().cloned()
}

/// What a section's header or footer of `kind` shows: its own, or the
/// nearest earlier section's.
pub fn resolved_part(doc: &FlowDocument, section: u32, kind: crate::flow::PartKind) -> Vec<Block> {
    let order = sections(doc);
    let at = order.iter().position(|s| *s == section).unwrap_or(order.len() - 1);
    for s in order[..=at].iter().rev() {
        if let Some(setup) = section_setup(doc, *s)
            && let Some(blocks) = &setup.parts[kind.index()]
        {
            return blocks.clone();
        }
    }
    Vec::new()
}

/// `blocks` with new ids from `next` on (a copy that is a list of its own).
pub fn renumbered(blocks: &[Block], next: &mut u32) -> Vec<Block> {
    blocks
        .iter()
        .map(|b| match b {
            Block::Paragraph(p) => {
                let mut p = p.clone();
                p.id = *next;
                *next += 1;
                p.bookmarks.clear();
                Block::Paragraph(p)
            }
            Block::Table(t) => {
                let mut t = t.clone();
                t.id = *next;
                *next += 1;
                for row in &mut t.rows {
                    row.id = *next;
                    *next += 1;
                    for cell in &mut row.cells {
                        cell.id = *next;
                        *next += 1;
                        cell.blocks = renumbered(&cell.blocks, next);
                    }
                }
                Block::Table(t)
            }
        })
        .collect()
}

/// The command giving section `section` the setup `edit` makes of its own
/// (its page, page numbers, first page, and headers and footers: `None`
/// for the previous section's).
pub fn set_section(doc: &FlowDocument, section: u32, edit: impl FnOnce(&mut crate::flow::SectionSetup)) -> Option<FlowCommand> {
    let mut setup = section_setup(doc, section)?;
    edit(&mut setup);
    let mut state = section_state(doc, &section_paragraphs(doc));
    if section == 0 {
        state.page = setup.page;
        state.restart_numbering = setup.restart_numbering;
        state.page_number_format = setup.page_number_format.clone();
        state.page_chapter = setup.page_chapter.clone();
        state.title_page = setup.title_page;
        for k in 0..6 {
            state.linked[k] = setup.parts[k].is_none();
            state.parts[k] = setup.parts[k].take().unwrap_or_default();
        }
    } else {
        let i = state.index_of(section)?;
        state.breaks[i].2 = Some(Arc::new(setup));
    }
    Some(FlowCommand::SetSections { state: Box::new(state) })
}

/// The command making a section's header or footer of `kind` its own (a
/// copy of what it shows), or the previous section's again (`own` false).
pub fn link_part(doc: &FlowDocument, section: u32, kind: crate::flow::PartKind, own: bool) -> Option<FlowCommand> {
    let mut next = doc.new_id();
    let copy = renumbered(&resolved_part(doc, section, kind), &mut next);
    let style = doc.styles.paragraph_style_named(if kind.is_footer() { "footer" } else { "header" }).map(str::to_string);
    set_section(doc, section, |s| {
        s.parts[kind.index()] = own.then(|| {
            if copy.is_empty() {
                let mut p = FlowParagraph { id: next, style, ..FlowParagraph::default() };
                p.resolve(&doc.styles);
                vec![Block::Paragraph(p)]
            } else {
                copy
            }
        });
    })
}

/// The command setting whether even pages have their own headers.
pub fn set_even_and_odd(doc: &FlowDocument, on: bool) -> FlowCommand {
    let mut state = section_state(doc, &section_paragraphs(doc));
    state.even_and_odd = on;
    FlowCommand::SetSections { state: Box::new(state) }
}

/// The commands putting a footnote (or an endnote) at `at`: its mark there
/// (superscript) and a new note of one paragraph, its number then a space,
/// in the note text style; and the note paragraph's id.
pub fn insert_note(doc: &FlowDocument, at: FlowPos, endnote: bool) -> Option<(Vec<FlowCommand>, u32)> {
    let (container, _) = locate(doc, at.para)?;
    if !matches!(container, Container::Body | Container::Cell { .. }) {
        return None;
    }
    let styles = &doc.styles;
    let notes = if endnote { &doc.endnotes } else { &doc.footnotes };
    let id = notes.iter().map(|n| n.id).max().map_or(1, |m| m + 1).max(1);
    let pid = doc.new_id();
    let (text_style, mark_style) = if endnote { ("endnote text", "endnote reference") } else { ("footnote text", "footnote reference") };
    let mark_style = styles.styles.iter().find(|(_, s)| s.kind == crate::flow::StyleKind::Character && s.name.eq_ignore_ascii_case(mark_style)).map(|(id, _)| id.clone());
    let superscript = RunFormat { position: Some(crate::flow::VerticalPosition::Superscript), ..RunFormat::default() };
    let mark_format = if mark_style.is_some() { RunFormat::default() } else { superscript.clone() };
    let mut p = FlowParagraph { id: pid, style: styles.paragraph_style_named(text_style).map(str::to_string), ..FlowParagraph::default() };
    p.runs.push(FlowRun { kind: FlowRunKind::NoteMark, style: FlowRunStyle::default(), format: mark_format.clone(), char_style: mark_style.clone(), link: None, revision: None, show_field_code: false, field_locked: false, control: None });
    p.runs.push(FlowRun::new(FlowRunKind::Text(" ".into()), FlowRunStyle::default()));
    p.resolve(styles);
    let (mut footnotes, mut endnotes) = (doc.footnotes.clone(), doc.endnotes.clone());
    let note = crate::flow::Note { id, blocks: vec![Block::Paragraph(p)] };
    if endnote { endnotes.push(note) } else { footnotes.push(note) }
    let mut commands = vec![FlowCommand::SetNotes { footnotes, endnotes }, FlowCommand::InsertField { at, kind: FlowRunKind::NoteRef { endnote, id } }];
    let to = FlowPos::new(at.para, at.offset + 1);
    if mark_style.is_none() {
        commands.push(FlowCommand::SetRunFormat { from: at, to, change: RunChange::Position(crate::flow::VerticalPosition::Superscript) });
    } else {
        commands.push(FlowCommand::SetNoteStyle { at, style: mark_style });
    }
    Some((commands, pid))
}

/// The command making every footnote an endnote (`to_endnotes`) or every
/// endnote a footnote: the notes move, their marks follow.
pub fn convert_notes(doc: &FlowDocument, to_endnotes: bool) -> Option<FlowCommand> {
    let (from, to) = if to_endnotes { (&doc.footnotes, &doc.endnotes) } else { (&doc.endnotes, &doc.footnotes) };
    if from.is_empty() {
        return None;
    }
    let mut next = to.iter().map(|n| n.id).max().map_or(1, |m| m + 1).max(1);
    let mut moved = to.clone();
    let mut ids = std::collections::HashMap::new();
    for note in from {
        ids.insert(note.id, next);
        moved.push(crate::flow::Note { id: next, blocks: note.blocks.clone() });
        next += 1;
    }
    let (footnotes, endnotes) = if to_endnotes { (Vec::new(), moved) } else { (moved, Vec::new()) };
    let mut commands = vec![FlowCommand::SetNotes { footnotes, endnotes }];
    for p in all_paragraphs(doc) {
        if !p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::NoteRef { endnote, .. } if endnote != to_endnotes)) {
            continue;
        }
        let mut q = p.clone();
        for r in &mut q.runs {
            if let FlowRunKind::NoteRef { endnote, id } = r.kind
                && endnote != to_endnotes
                && let Some(new) = ids.get(&id)
            {
                r.kind = FlowRunKind::NoteRef { endnote: to_endnotes, id: *new };
            }
        }
        let (container, index) = locate(doc, p.id)?;
        commands.push(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(q)] });
    }
    Some(FlowCommand::Many(commands))
}

/// Who writes a comment and when.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommentAuthor {
    pub author: String,
    pub initials: String,
    /// `2026-10-01T09:00:00Z`.
    pub date: String,
}

/// A new comment's paragraph: `text` in the comment text style.
fn comment_paragraph(doc: &FlowDocument, id: u32, text: &str) -> FlowParagraph {
    let mut p = FlowParagraph { id, style: doc.styles.paragraph_style_named("annotation text").map(str::to_string), ..FlowParagraph::default() };
    if !text.is_empty() {
        p.runs.push(FlowRun::new(FlowRunKind::Text(text.to_string()), FlowRunStyle::default()));
    }
    p.resolve(&doc.styles);
    p
}

/// The `w14:paraId` a new comment's paragraph gets (unique for its id).
fn comment_para_id(id: u32) -> String {
    format!("{:08X}", 0x1C00_0000u32.wrapping_add(id.wrapping_mul(0x0001_0101)) & 0x7FFF_FFFF)
}

/// Where comment `id`'s marks are: (paragraph, offset) of its start, end
/// and reference.
pub fn comment_marks(doc: &FlowDocument, id: u32) -> [Option<FlowPos>; 3] {
    let mut out = [None; 3];
    for p in all_paragraphs(doc) {
        let mut at = 0;
        for r in &p.runs {
            let k = match r.kind {
                FlowRunKind::CommentStart(i) if i == id => Some(0),
                FlowRunKind::CommentEnd(i) if i == id => Some(1),
                FlowRunKind::CommentRef(i) if i == id => Some(2),
                _ => None,
            };
            if let Some(k) = k {
                out[k] = Some(FlowPos::new(p.id, at));
            }
            at += r.len();
        }
    }
    out
}

/// The text range covered by comment `id` (`from` .. `to`), if marks exist.
pub fn comment_range(doc: &FlowDocument, id: u32) -> Option<(FlowPos, FlowPos)> {
    let [start, end, ref_pos] = comment_marks(doc, id);
    match (start, end) {
        (Some(s), Some(e)) => Some((s, e)),
        (Some(s), None) => Some((s, ref_pos.unwrap_or(s))),
        (None, Some(e)) => Some((e, e)),
        (None, None) => ref_pos.map(|r| (r, r)),
    }
}

/// The comment (if any) containing `pos`. If multiple comments cover `pos`, returns the most specific (innermost/latest).
pub fn comment_at_pos(doc: &FlowDocument, pos: FlowPos) -> Option<u32> {
    let mut active: Vec<u32> = Vec::new();
    for p in all_paragraphs(doc) {
        let mut at = 0;
        for r in &p.runs {
            let len = r.len();
            match r.kind {
                FlowRunKind::CommentStart(id) => {
                    active.push(id);
                    if p.id == pos.para && pos.offset >= at && (pos.offset < at + len || (len == 0 && pos.offset == at)) {
                        return Some(id);
                    }
                }
                FlowRunKind::CommentEnd(id) => {
                    if p.id == pos.para && pos.offset >= at && (pos.offset < at + len || (len == 0 && pos.offset == at)) {
                        return Some(id);
                    }
                    active.retain(|&c| c != id);
                }
                FlowRunKind::CommentRef(id) => {
                    if p.id == pos.para && pos.offset >= at && (pos.offset < at + len || (len == 0 && pos.offset == at)) {
                        return Some(id);
                    }
                }
                _ => {
                    if p.id == pos.para && pos.offset >= at && (pos.offset < at + len || (len == 0 && pos.offset == at)) {
                        if let Some(&id) = active.last() {
                            return Some(id);
                        }
                    }
                }
            }
            at += len;
        }
        if p.id == pos.para {
            if pos.offset == at {
                if let Some(&id) = active.last() {
                    return Some(id);
                }
            }
            break;
        }
    }
    None
}

/// The commands putting a comment (`text`) on the text from `from` to `to`:
/// its range's marks and its mark after the range.
pub fn add_comment(doc: &FlowDocument, from: FlowPos, to: FlowPos, who: &CommentAuthor, text: &str) -> Option<Vec<FlowCommand>> {
    if from == to {
        return None;
    }
    let id = doc.comments.iter().map(|c| c.id + 1).max().unwrap_or(0);
    let mut comments = doc.comments.clone();
    comments.push(crate::flow::Comment {
        id,
        author: who.author.clone(),
        initials: who.initials.clone(),
        date: who.date.clone(),
        blocks: vec![Block::Paragraph(comment_paragraph(doc, doc.new_id(), text))],
        parent: None,
        done: false,
        para_id: Some(comment_para_id(id)),
    });
    Some(vec![
        FlowCommand::SetComments(comments),
        FlowCommand::InsertField { at: to, kind: FlowRunKind::CommentEnd(id) },
        FlowCommand::InsertField { at: FlowPos::new(to.para, to.offset + 1), kind: FlowRunKind::CommentRef(id) },
        FlowCommand::InsertField { at: from, kind: FlowRunKind::CommentStart(id) },
    ])
}

/// The commands answering comment `parent` with `text` (返信): a comment
/// marked where it is, as Word writes replies.
pub fn reply_comment(doc: &FlowDocument, parent: u32, who: &CommentAuthor, text: &str) -> Option<Vec<FlowCommand>> {
    if !doc.comments.iter().any(|c| c.id == parent) {
        return None;
    }
    let id = doc.comments.iter().map(|c| c.id + 1).max().unwrap_or(0);
    let mut comments = doc.comments.clone();
    comments.push(crate::flow::Comment {
        id,
        author: who.author.clone(),
        initials: who.initials.clone(),
        date: who.date.clone(),
        blocks: vec![Block::Paragraph(comment_paragraph(doc, doc.new_id(), text))],
        parent: Some(parent),
        done: false,
        para_id: Some(comment_para_id(id)),
    });
    let mut commands = vec![FlowCommand::SetComments(comments)];
    let [start_opt, end_opt, ref_opt] = comment_marks(doc, parent);
    if let Some(pos) = start_opt.or(end_opt).or(ref_opt) {
        let start = start_opt.unwrap_or(pos);
        let end = end_opt.unwrap_or(pos);
        let reference = ref_opt.unwrap_or(pos);
        let mut marks = [(start, FlowRunKind::CommentStart(id)), (end, FlowRunKind::CommentEnd(id)), (reference, FlowRunKind::CommentRef(id))];
        marks.sort_by_key(|(p, _)| (all_paragraphs(doc).iter().position(|q| q.id == p.para), p.offset));
        for (pos, kind) in marks.into_iter().rev() {
            commands.push(FlowCommand::InsertField { at: FlowPos::new(pos.para, pos.offset + 1), kind });
        }
    }
    Some(commands)
}

/// The command resolving a comment (and its replies), or opening it again.
pub fn resolve_comment(doc: &FlowDocument, id: u32, done: bool) -> FlowCommand {
    let mut comments = doc.comments.clone();
    for c in &mut comments {
        if c.id == id || c.parent == Some(id) {
            c.done = done;
        }
    }
    FlowCommand::SetComments(comments)
}

/// The commands removing a comment, its replies and their marks.
pub fn delete_comment(doc: &FlowDocument, id: u32) -> Option<FlowCommand> {
    let gone: Vec<u32> = doc.comments.iter().filter(|c| c.id == id || c.parent == Some(id)).map(|c| c.id).collect();
    if gone.is_empty() {
        return None;
    }
    let comments: Vec<_> = doc.comments.iter().filter(|c| !gone.contains(&c.id)).cloned().collect();
    let mut commands = vec![FlowCommand::SetComments(comments)];
    for p in all_paragraphs(doc) {
        let marked = |r: &FlowRun| matches!(r.kind, FlowRunKind::CommentStart(i) | FlowRunKind::CommentEnd(i) | FlowRunKind::CommentRef(i) if gone.contains(&i));
        if !p.runs.iter().any(marked) {
            continue;
        }
        let mut q = p.clone();
        q.runs.retain(|r| !marked(r));
        q.normalize();
        let (container, index) = locate(doc, p.id)?;
        commands.push(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(q)] });
    }
    Some(FlowCommand::Many(commands))
}

/// The comments whose marks are gone (their text deleted) and their
/// replies: to remove with them, as Word does.
pub fn orphan_comments(doc: &FlowDocument) -> Vec<u32> {
    // A comment needs its start, its end and its mark.
    let mut marked: std::collections::HashMap<u32, [bool; 3]> = std::collections::HashMap::new();
    for p in all_paragraphs(doc) {
        for r in &p.runs {
            let (i, k) = match r.kind {
                FlowRunKind::CommentStart(i) => (i, 0),
                FlowRunKind::CommentEnd(i) => (i, 1),
                FlowRunKind::CommentRef(i) => (i, 2),
                _ => continue,
            };
            marked.entry(i).or_default()[k] = true;
        }
    }
    let gone: Vec<u32> = doc.comments.iter().filter(|c| marked.get(&c.id) != Some(&[true; 3])).map(|c| c.id).collect();
    doc.comments.iter().filter(|c| gone.contains(&c.id) || c.parent.is_some_and(|p| gone.contains(&p))).map(|c| c.id).collect()
}

/// The commands removing all comments and their marks across the document.
pub fn delete_all_comments(doc: &FlowDocument) -> Option<FlowCommand> {
    if doc.comments.is_empty() {
        return None;
    }
    let mut commands = vec![FlowCommand::SetComments(Vec::new())];
    for p in all_paragraphs_everywhere(doc) {
        let marked = |r: &FlowRun| matches!(r.kind, FlowRunKind::CommentStart(_) | FlowRunKind::CommentEnd(_) | FlowRunKind::CommentRef(_));
        if !p.runs.iter().any(marked) {
            continue;
        }
        let mut q = p.clone();
        q.runs.retain(|r| !marked(r));
        q.normalize();
        if let Some((container, index)) = locate(doc, p.id) {
            commands.push(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(q)] });
        }
    }
    Some(FlowCommand::Many(commands))
}

/// Apply `f` to the part of each paragraph from `from` to `to` (in one list
/// of blocks), with that part's start and end; the old paragraphs are the
/// inverse.
fn each_part(doc: &mut FlowDocument, from: FlowPos, to: FlowPos, styles: &Arc<FlowStyles>, f: impl Fn(&mut FlowParagraph, usize, usize)) -> Result<FlowCommand, EditError> {
    let (container, first, last, from, to) = ordered(doc, from, to)?;
    let list = blocks_mut(doc, container).ok_or(EditError::IndexOutOfRange)?;
    let old: Vec<Block> = list[first..=last].to_vec();
    for (i, block) in list[first..=last].iter_mut().enumerate() {
        let Block::Paragraph(p) = block else { continue };
        let start = if i == 0 { from.offset } else { 0 };
        let end = if first + i == last { to.offset } else { p.len() };
        if start < end {
            f(p, start, end);
        }
        p.resolve(styles);
        p.normalize();
    }
    Ok(FlowCommand::ReplaceBlocks { container, index: first, remove: last - first + 1, blocks: old })
}

/// Who made a tracked change and when.
pub type RevisionBy = (String, String);

/// 承諾 or 却下 of the tracked changes (all, or those from `from` to `to`):
/// the commands. Accepting keeps insertions and drops deletions; rejecting
/// does the opposite and puts the formatting before back.
pub fn resolve_revisions(doc: &FlowDocument, range: Option<(FlowPos, FlowPos)>, accept: bool) -> Vec<FlowCommand> {
    resolve_revisions_filter(doc, range, accept, None)
}

/// 承諾 or 却下 of the tracked changes with an optional author filter:
/// when `author_filter` is `Some(filter)`, only revisions by authors contained in `filter`
/// are resolved; other revisions are preserved.
pub fn resolve_revisions_filter(
    doc: &FlowDocument,
    range: Option<(FlowPos, FlowPos)>,
    accept: bool,
    author_filter: Option<&std::collections::HashSet<String>>,
) -> Vec<FlowCommand> {
    if doc.lock_tracking {
        return Vec::new();
    }
    use crate::flow::RevisionKind;
    let ps = all_paragraphs(doc);
    let order: Vec<u32> = ps.iter().map(|p| p.id).collect();
    let index = |id: u32| order.iter().position(|o| *o == id).unwrap_or(0);
    // When resolving revisions within a selection, identify any MoveTo or MoveFrom revisions
    // so that their paired counterparts (MoveFrom or MoveTo by the same author) are resolved together.
    let mut target_moves: Vec<(bool, String, String)> = Vec::new();
    if let Some((a, b)) = range {
        for p in &ps {
            let i = index(p.id);
            if i < index(a.para) || i > index(b.para) {
                continue;
            }
            let (lo, hi) = (if i == index(a.para) { a.offset } else { 0 }, if i == index(b.para) { b.offset } else { usize::MAX });
            let mut at = 0;
            for run in &p.runs {
                let len = run.len();
                let inside = at < hi && at + len > lo || (len == 0 && at >= lo && at <= hi) || (lo == hi && at <= lo && lo < at + len.max(1));
                at += len;
                if inside && let Some(r) = &run.revision {
                    if let Some(filter) = author_filter && !filter.contains(&r.author) {
                        continue;
                    }
                    match r.kind {
                        RevisionKind::MoveTo => target_moves.push((true, r.author.clone(), r.date.clone())),
                        RevisionKind::MoveFrom => target_moves.push((false, r.author.clone(), r.date.clone())),
                        _ => {}
                    }
                }
            }
        }
    }

    let mut commands = Vec::new();
    for p in ps {
        if !p.runs.iter().any(|r| r.revision.is_some()) {
            continue;
        }
        let i = index(p.id);
        let within_range = match range {
            None => true,
            Some((a, b)) => i >= index(a.para) && i <= index(b.para),
        };
        let (lo, hi) = match range {
            None => (0, usize::MAX),
            Some((a, b)) => {
                let has_paired_move = p.runs.iter().any(|r| {
                    if let Some(rev) = &r.revision {
                        if let Some(filter) = author_filter && !filter.contains(&rev.author) {
                            return false;
                        }
                        target_moves.iter().any(|(is_to, author, date)| {
                            let matches_opposite = match rev.kind {
                                RevisionKind::MoveFrom => *is_to,
                                RevisionKind::MoveTo => !*is_to,
                                _ => false,
                            };
                            matches_opposite && &rev.author == author && (date.is_empty() || rev.date.is_empty() || &rev.date == date)
                        })
                    } else {
                        false
                    }
                });
                if !within_range && !has_paired_move {
                    continue;
                }
                (if i == index(a.para) { a.offset } else { 0 }, if i == index(b.para) { b.offset } else { usize::MAX })
            }
        };
        let mut q = p.clone();
        let mut runs = Vec::new();
        let mut at = 0;
        for mut run in std::mem::take(&mut q.runs) {
            let len = run.len();
            let inside = range.is_none() || (within_range && (at < hi && at + len > lo || (len == 0 && at >= lo && at <= hi) || (lo == hi && at <= lo && lo < at + len.max(1))));
            let is_paired_move = if !inside && let Some(r) = &run.revision {
                if let Some(filter) = author_filter && !filter.contains(&r.author) {
                    false
                } else {
                    target_moves.iter().any(|(is_to, author, date)| {
                        let matches_opposite = match r.kind {
                            RevisionKind::MoveFrom => *is_to,
                            RevisionKind::MoveTo => !*is_to,
                            _ => false,
                        };
                        matches_opposite && &r.author == author && (date.is_empty() || r.date.is_empty() || &r.date == date)
                    })
                }
            } else {
                false
            };
            at += len;
            if (inside || is_paired_move) && let Some(r) = run.revision.take() {
                if let Some(filter) = author_filter && !filter.contains(&r.author) {
                    run.revision = Some(r);
                    runs.push(run);
                    continue;
                }
                match (r.kind, accept) {
                    (RevisionKind::Insert, false)
                    | (RevisionKind::Delete, true)
                    | (RevisionKind::MoveTo, false)
                    | (RevisionKind::MoveFrom, true) => continue,
                    (RevisionKind::Format(old), false) => run.format = old,
                    _ => {}
                }
            }
            runs.push(run);
        }
        q.runs = runs;
        q.resolve(&doc.styles);
        q.normalize();
        if q != *p
            && let Some((container, index)) = locate(doc, p.id)
        {
            commands.push(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(q)] });
        }
    }
    commands
}

/// The grid columns each cell of a row starts at.
fn cell_columns(row: &FlowRow) -> Vec<usize> {
    let mut at = 0;
    row.cells
        .iter()
        .map(|c| {
            let start = at;
            at += c.span.max(1) as usize;
            start
        })
        .collect()
}

/// An empty cell like `cell` (its formatting, its first paragraph's).
fn empty_like(cell: &FlowCell, next: &mut u32, styles: &FlowStyles) -> FlowCell {
    let mut id = || {
        *next += 1;
        *next - 1
    };
    let mut para = match cell.blocks.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.clone()) } else { None }) {
        Some(p) => FlowParagraph { id: id(), runs: Vec::new(), list: None, ..p },
        None => FlowParagraph { id: id(), ..FlowParagraph::default() },
    };
    para.resolve(styles);
    FlowCell { id: id(), blocks: vec![Block::Paragraph(para)], merge: VerticalMerge::None, diagonals: [None; 2], ..cell.clone() }
}

/// A table after `edit` (none when it removes the whole table).
fn edit_table(mut t: FlowTable, edit: TableEdit, next: &mut u32, styles: &FlowStyles) -> Result<Option<FlowTable>, EditError> {
    match edit {
        TableEdit::InsertRow { row, below } => {
            let source = t.rows.get(row).ok_or(EditError::IndexOutOfRange)?.clone();
            *next += 1;
            let mut new = FlowRow { id: *next - 1, height: source.height, header: false, cant_split: source.cant_split, cells: Vec::new() };
            for cell in &source.cells {
                new.cells.push(empty_like(cell, next, styles));
            }
            t.rows.insert(if below { row + 1 } else { row }, new);
            if let Some(fit) = &mut t.autofit
                && let Some(widths) = fit.cells.get(row).cloned()
            {
                fit.cells.insert((if below { row + 1 } else { row }).min(fit.cells.len()), widths);
            }
        }
        TableEdit::DeleteRows { first, last } => {
            if first > last || last >= t.rows.len() {
                return Err(EditError::IndexOutOfRange);
            }
            t.rows.drain(first..=last);
            if let Some(fit) = &mut t.autofit {
                fit.cells.drain(first.min(fit.cells.len())..(last + 1).min(fit.cells.len()));
            }
            if t.rows.is_empty() {
                return Ok(None);
            }
            // A merge down that lost its start starts again.
            if let Some(row) = t.rows.get_mut(first) {
                for c in &mut row.cells {
                    if c.merge == VerticalMerge::Continue && first == 0 {
                        c.merge = VerticalMerge::Start;
                    }
                }
            }
        }
        TableEdit::InsertColumn { column, right } => {
            if column >= t.columns.len() {
                return Err(EditError::IndexOutOfRange);
            }
            // The columns are the grid's from now on.
            t.autofit = None;
            let total: Emu = t.columns.iter().sum();
            let at = if right { column + 1 } else { column };
            t.columns.insert(at, t.columns[column]);
            // Keep the table's width.
            // (In whole twips, as the file keeps them.)
            let now: Emu = t.columns.iter().sum();
            if now > 0 {
                for w in &mut t.columns {
                    *w = (*w * total / now) / 635 * 635;
                }
            }
            for row in &mut t.rows {
                let starts = cell_columns(row);
                let Some(i) = starts.iter().rposition(|&s| s <= column) else { continue };
                let new = empty_like(&row.cells[i], next, styles);
                let new = FlowCell { span: 1, ..new };
                row.cells.insert(if right { i + 1 } else { i }, new);
            }
        }
        TableEdit::DeleteColumn { column } => {
            if column >= t.columns.len() {
                return Err(EditError::IndexOutOfRange);
            }
            t.columns.remove(column);
            t.autofit = None;
            if t.columns.is_empty() {
                return Ok(None);
            }
            for row in &mut t.rows {
                let starts = cell_columns(row);
                let Some(i) = starts.iter().rposition(|&s| s <= column) else { continue };
                if row.cells[i].span > 1 {
                    row.cells[i].span -= 1;
                } else {
                    row.cells.remove(i);
                }
            }
        }
        TableEdit::InsertCellShiftRight { row, cell } => {
            if row >= t.rows.len() {
                return Err(EditError::IndexOutOfRange);
            }
            let target_row = &mut t.rows[row];
            if cell > target_row.cells.len() {
                return Err(EditError::IndexOutOfRange);
            }
            let template_cell = target_row.cells.get(cell.saturating_sub(1))
                .or_else(|| target_row.cells.get(cell))
                .cloned()
                .unwrap_or_default();
            let new_cell = empty_like(&template_cell, next, styles);
            target_row.cells.insert(cell, new_cell);

            if let Some(fit) = &mut t.autofit {
                if let Some(widths) = fit.cells.get_mut(row) {
                    let pref = widths.get(cell.saturating_sub(1)).copied().unwrap_or(crate::flow::PreferredWidth::Auto);
                    widths.insert(cell.min(widths.len()), pref);
                }
            }

            let row_span: u32 = target_row.cells.iter().map(|c| c.span.max(1)).sum();
            while t.columns.len() < row_span as usize {
                let last_w = t.columns.last().copied().unwrap_or(360_000);
                t.columns.push(last_w);
            }
            t.resolve_borders();
            t.apply_style(styles);
        }
        TableEdit::InsertCellShiftDown { row, cell } => {
            if row >= t.rows.len() {
                return Err(EditError::IndexOutOfRange);
            }
            if cell >= t.rows[row].cells.len() {
                return Err(EditError::IndexOutOfRange);
            }

            let col_starts = cell_columns(&t.rows[row]);
            let target_col = col_starts[cell];

            let template_cell = t.rows[row].cells[cell].clone();
            let mut insert_cell = empty_like(&template_cell, next, styles);

            let row_count = t.rows.len();
            for r in row..row_count {
                let starts = cell_columns(&t.rows[r]);
                let idx = starts.iter().position(|&s| s == target_col)
                    .or_else(|| starts.iter().rposition(|&s| s <= target_col))
                    .unwrap_or(0);

                let prev_cell = std::mem::replace(&mut t.rows[r].cells[idx], insert_cell);
                insert_cell = prev_cell;
            }

            let last_row_template = t.rows.last().unwrap().clone();
            *next += 1;
            let mut new_row = crate::flow::FlowRow {
                id: *next - 1,
                height: last_row_template.height,
                header: false,
                cant_split: last_row_template.cant_split,
                cells: Vec::new(),
            };

            let col_count = t.columns.len().max(1);
            let mut cur_col = 0;
            while cur_col < col_count {
                if cur_col == target_col {
                    cur_col += insert_cell.span.max(1) as usize;
                    new_row.cells.push(insert_cell.clone());
                } else {
                    let dummy = empty_like(&template_cell, next, styles);
                    cur_col += dummy.span.max(1) as usize;
                    new_row.cells.push(dummy);
                }
            }
            t.rows.push(new_row);

            if let Some(fit) = &mut t.autofit {
                let widths = vec![crate::flow::PreferredWidth::Auto; t.rows.last().unwrap().cells.len()];
                fit.cells.push(widths);
            }

            t.resolve_borders();
            t.apply_style(styles);
        }
        TableEdit::DeleteCellShiftLeft { row, cell } => {
            if row >= t.rows.len() || cell >= t.rows[row].cells.len() {
                return Err(EditError::IndexOutOfRange);
            }
            t.rows[row].cells.remove(cell);
            if let Some(fit) = &mut t.autofit {
                if let Some(widths) = fit.cells.get_mut(row) {
                    if cell < widths.len() {
                        widths.remove(cell);
                    }
                }
            }
            if t.rows[row].cells.is_empty() {
                t.rows.remove(row);
                if let Some(fit) = &mut t.autofit {
                    if row < fit.cells.len() {
                        fit.cells.remove(row);
                    }
                }
            }
            if t.rows.is_empty() {
                return Ok(None);
            }
            t.resolve_borders();
            t.apply_style(styles);
        }
        TableEdit::DeleteCellShiftUp { row, cell } => {
            if row >= t.rows.len() || cell >= t.rows[row].cells.len() {
                return Err(EditError::IndexOutOfRange);
            }
            let col_starts = cell_columns(&t.rows[row]);
            let target_col = col_starts[cell];

            let row_count = t.rows.len();
            for r in row..row_count - 1 {
                let next_starts = cell_columns(&t.rows[r + 1]);
                let next_idx = next_starts.iter().position(|&s| s == target_col)
                    .or_else(|| next_starts.iter().rposition(|&s| s <= target_col));

                if let Some(n_idx) = next_idx {
                    let cur_starts = cell_columns(&t.rows[r]);
                    let cur_idx = cur_starts.iter().position(|&s| s == target_col)
                        .or_else(|| cur_starts.iter().rposition(|&s| s <= target_col))
                        .unwrap_or(0);
                    let moved_cell = t.rows[r + 1].cells.remove(n_idx);
                    if cur_idx < t.rows[r].cells.len() {
                        t.rows[r].cells[cur_idx] = moved_cell;
                    } else {
                        t.rows[r].cells.push(moved_cell);
                    }
                }
            }

            let last_idx = t.rows.len() - 1;
            if last_idx > row {
                let starts = cell_columns(&t.rows[last_idx]);
                if let Some(idx) = starts.iter().position(|&s| s == target_col) {
                    t.rows[last_idx].cells.remove(idx);
                }
                if t.rows[last_idx].cells.is_empty() {
                    t.rows.remove(last_idx);
                    if let Some(fit) = &mut t.autofit {
                        if last_idx < fit.cells.len() {
                            fit.cells.remove(last_idx);
                        }
                    }
                }
            } else {
                t.rows[row].cells.remove(cell);
                if t.rows[row].cells.is_empty() {
                    t.rows.remove(row);
                }
            }

            if t.rows.is_empty() {
                return Ok(None);
            }
            t.resolve_borders();
            t.apply_style(styles);
        }
        TableEdit::Shading { cells, color } => {
            for (r, c) in cells {
                let cell = t.rows.get_mut(r).and_then(|row| row.cells.get_mut(c)).ok_or(EditError::IndexOutOfRange)?;
                cell.shading = color;
            }
        }
        TableEdit::Anchor { cells, anchor } => {
            for (r, c) in cells {
                let cell = t.rows.get_mut(r).and_then(|row| row.cells.get_mut(c)).ok_or(EditError::IndexOutOfRange)?;
                cell.anchor = anchor;
            }
        }
        TableEdit::Direction { cells, direction } => {
            for (r, c) in cells {
                let cell = t.rows.get_mut(r).and_then(|row| row.cells.get_mut(c)).ok_or(EditError::IndexOutOfRange)?;
                cell.direction = direction;
            }
        }
        TableEdit::NoWrap { cells, no_wrap } => {
            for (r, c) in cells {
                let cell = t.rows.get_mut(r).and_then(|row| row.cells.get_mut(c)).ok_or(EditError::IndexOutOfRange)?;
                cell.no_wrap = no_wrap;
            }
        }
        TableEdit::Margins { cells, margins } => {
            for (r, c) in cells {
                let cell = t.rows.get_mut(r).and_then(|row| row.cells.get_mut(c)).ok_or(EditError::IndexOutOfRange)?;
                cell.margins = margins;
            }
        }
        TableEdit::ColumnWidth { column, width } => {
            t.autofit = None;
            *t.columns.get_mut(column).ok_or(EditError::IndexOutOfRange)? = (width / 635).max(1) * 635;
        }
        TableEdit::RowHeight { row, height } => {
            t.rows.get_mut(row).ok_or(EditError::IndexOutOfRange)?.height = height;
        }
        TableEdit::EqualColumns => {
            t.autofit = None;
            let n = t.columns.len() as i64;
            if n == 0 {
                return Err(EditError::IndexOutOfRange);
            }
            let total: i64 = t.columns.iter().sum();
            let each = (total / n / 635).max(1) * 635;
            for w in &mut t.columns {
                *w = each;
            }
            // What the rounding leaves goes to the last column.
            if let Some(last) = t.columns.last_mut() {
                *last = (total - each * (n - 1)).max(635);
            }
        }
        TableEdit::EqualRows => {
            let n = t.rows.len() as i64;
            if n == 0 {
                return Err(EditError::IndexOutOfRange);
            }
            let total: i64 = t.rows.iter().map(|r| match r.height {
                crate::flow::RowHeight::Exact(h) | crate::flow::RowHeight::AtLeast(h) if h > 0 => h,
                _ => 20 * 12700,
            }).sum();
            let each = (total / n / 635).max(1) * 635;
            let all_exact = !t.rows.is_empty() && t.rows.iter().all(|r| matches!(r.height, crate::flow::RowHeight::Exact(_)));
            for r in &mut t.rows {
                r.height = if all_exact {
                    crate::flow::RowHeight::Exact(each)
                } else {
                    crate::flow::RowHeight::AtLeast(each)
                };
            }
            if let Some(last) = t.rows.last_mut() {
                let remainder = (total - each * (n - 1)).max(635);
                last.height = if all_exact {
                    crate::flow::RowHeight::Exact(remainder)
                } else {
                    crate::flow::RowHeight::AtLeast(remainder)
                };
            }
        }
        TableEdit::AutoFitContents => {
            let n = t.columns.len();
            if n == 0 {
                return Err(EditError::IndexOutOfRange);
            }
            let mut col_widths = vec![20 * 12700; n];
            for row in &t.rows {
                let mut col = 0;
                for cell in &row.cells {
                    let span = cell.span.max(1) as usize;
                    if col + span <= n {
                        let mut max_chars = 0;
                        for b in &cell.blocks {
                            if let Block::Paragraph(p) = b {
                                for line in p.text().split('\n') {
                                    let chars = line.chars().count();
                                    if chars > max_chars {
                                        max_chars = chars;
                                    }
                                }
                            }
                        }
                        let needed = (max_chars as i64 * 133_350 + 140_000).max(20 * 12700);
                        if span == 1 {
                            col_widths[col] = col_widths[col].max(needed);
                        }
                    }
                    col += span;
                }
            }
            for (i, w) in col_widths.into_iter().enumerate() {
                t.columns[i] = (w / 635).max(1) * 635;
            }
            t.autofit = Some(crate::flow::AutoFit {
                width: crate::flow::PreferredWidth::Auto,
                cells: t.rows.iter().map(|r| r.cells.iter().map(|_| crate::flow::PreferredWidth::Auto).collect()).collect(),
            });
        }
        TableEdit::AutoFitWindow(available) => {
            let n = t.columns.len() as i64;
            if n == 0 {
                return Err(EditError::IndexOutOfRange);
            }
            let target = if available > 0 { available } else { 150 * 36000 };
            let total: i64 = t.columns.iter().sum();
            if total > 0 {
                let mut sum = 0;
                for (i, w) in t.columns.iter_mut().enumerate() {
                    if i as i64 == n - 1 {
                        *w = (target - sum).max(635);
                    } else {
                        let scaled = ((*w as f64 * target as f64 / total as f64).round() as i64 / 635).max(1) * 635;
                        *w = scaled;
                        sum += scaled;
                    }
                }
            } else {
                let each = (target / n / 635).max(1) * 635;
                for w in &mut t.columns {
                    *w = each;
                }
                if let Some(last) = t.columns.last_mut() {
                    *last = (target - each * (n - 1)).max(635);
                }
            }
            t.autofit = Some(crate::flow::AutoFit {
                width: crate::flow::PreferredWidth::Percent(5000),
                cells: t.rows.iter().map(|r| r.cells.iter().map(|_| crate::flow::PreferredWidth::Auto).collect()).collect(),
            });
        }
        TableEdit::FixedColumns => {
            t.autofit = None;
        }
        TableEdit::RepeatHeader { row, on } => {
            if row >= t.rows.len() {
                return Err(EditError::IndexOutOfRange);
            }
            for (i, r) in t.rows.iter_mut().enumerate() {
                r.header = on && i <= row;
            }
        }
        TableEdit::CantSplit { rows, on } => {
            for i in rows {
                t.rows.get_mut(i).ok_or(EditError::IndexOutOfRange)?.cant_split = on;
            }
        }
        TableEdit::Look(look) => {
            let Some(style) = t.style.as_mut() else { return Err(EditError::IndexOutOfRange) };
            style.look = look;
        }
        TableEdit::Align(align) | TableEdit::Alignment(align) => {
            t.align = align;
        }
        TableEdit::Indent(indent) => {
            t.indent = indent.max(0);
        }
        TableEdit::CellSpacing(spacing) => {
            t.cell_spacing = spacing.map(|s| s.max(0));
        }
        TableEdit::SortRows { column, ascending, has_header } => {
            if t.rows.len() <= 1 {
                return Ok(Some(t));
            }
            let header_count = t.rows.iter().take_while(|r| r.header).count();
            let start_row = if has_header { header_count.max(1) } else { 0 };
            if start_row >= t.rows.len() {
                return Ok(Some(t));
            }

            #[derive(Debug, Clone, PartialEq)]
            enum SortKey {
                Empty,
                Number(f64),
                Text(String),
            }

            impl SortKey {
                fn cmp_key(&self, other: &Self) -> std::cmp::Ordering {
                    match (self, other) {
                        (SortKey::Empty, SortKey::Empty) => std::cmp::Ordering::Equal,
                        (SortKey::Empty, _) => std::cmp::Ordering::Greater,
                        (_, SortKey::Empty) => std::cmp::Ordering::Less,
                        (SortKey::Number(a), SortKey::Number(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
                        (SortKey::Number(_), SortKey::Text(_)) => std::cmp::Ordering::Less,
                        (SortKey::Text(_), SortKey::Number(_)) => std::cmp::Ordering::Greater,
                        (SortKey::Text(a), SortKey::Text(b)) => a.cmp(b),
                    }
                }
            }

            let mut indexed_rows: Vec<(usize, FlowRow, Option<Vec<crate::flow::PreferredWidth>>, SortKey)> = Vec::new();
            for (idx, row) in t.rows.drain(start_row..).enumerate() {
                let fit_cells = if let Some(fit) = &mut t.autofit {
                    if start_row + idx < fit.cells.len() {
                        Some(fit.cells[start_row + idx].clone())
                    } else {
                        None
                    }
                } else {
                    None
                };

                let starts = cell_columns(&row);
                let text = starts
                    .iter()
                    .rposition(|&s| s <= column)
                    .and_then(|c_idx| row.cells.get(c_idx))
                    .map(|cell| {
                        let mut s = String::new();
                        for b in &cell.blocks {
                            if let Block::Paragraph(p) = b {
                                if !s.is_empty() {
                                    s.push(' ');
                                }
                                s.push_str(&p.text());
                            }
                        }
                        s.trim().to_string()
                    })
                    .unwrap_or_default();

                let cleaned = text.replace(',', "").trim().to_string();
                let key = if cleaned.is_empty() {
                    SortKey::Empty
                } else if let Ok(val) = cleaned.parse::<f64>() {
                    if val.is_finite() {
                        SortKey::Number(val)
                    } else {
                        SortKey::Text(text.to_lowercase())
                    }
                } else {
                    SortKey::Text(text.to_lowercase())
                };

                indexed_rows.push((idx, row, fit_cells, key));
            }

            indexed_rows.sort_by(|a, b| {
                match (&a.3, &b.3) {
                    (SortKey::Empty, SortKey::Empty) => std::cmp::Ordering::Equal,
                    (SortKey::Empty, _) => std::cmp::Ordering::Greater,
                    (_, SortKey::Empty) => std::cmp::Ordering::Less,
                    _ => {
                        let c = a.3.cmp_key(&b.3);
                        if ascending {
                            c
                        } else {
                            c.reverse()
                        }
                    }
                }
            });

            if let Some(fit) = &mut t.autofit {
                fit.cells.truncate(start_row);
                for (_, row, fit_cells, _) in indexed_rows {
                    t.rows.push(row);
                    if let Some(fc) = fit_cells {
                        fit.cells.push(fc);
                    }
                }
            } else {
                for (_, row, _, _) in indexed_rows {
                    t.rows.push(row);
                }
            }
        }
        TableEdit::Floating(floating) => {
            t.floating = floating;
        }
        TableEdit::FloatDistance(distance) => {
            if let Some(f) = &mut t.floating {
                f.distance = distance;
            } else {
                let mut f = crate::flow::TableFloat::standard();
                f.distance = distance;
                t.floating = Some(f);
            }
        }
        TableEdit::Style(style_id) => {
            t.style = style_id.clone().map(|id| {
                let look = t.style.as_ref().map(|s| s.look).unwrap_or(crate::flow::TableLook {
                    first_row: true,
                    last_row: false,
                    first_column: true,
                    last_column: false,
                    no_hband: false,
                    no_vband: true,
                });
                crate::flow::TableStyleRef {
                    id,
                    look,
                    row_band: 1,
                    col_band: 1,
                }
            });
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    for block in &mut cell.blocks {
                        if let Block::Paragraph(p) = block {
                            p.table_style = style_id.clone();
                            p.resolve(styles);
                        }
                    }
                }
            }
        }
    }
    // The style's parts found again (the rows or the options changed).
    t.apply_style(styles);
    t.resolve_borders();
    Ok(Some(t))
}

/// Resolve every paragraph of `blocks` again (after the definitions changed).
fn resolve_all(blocks: &mut [Block], styles: &FlowStyles) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => p.resolve(styles),
            Block::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        resolve_all(&mut cell.blocks, styles);
                    }
                }
            }
        }
    }
}

impl FlowParagraph {
    /// The formatting text typed at `offset` takes: that of the run before
    /// it (the first run at the start), else the paragraph mark's.
    pub fn format_at(&self, offset: usize) -> (RunFormat, Option<String>, FlowRunStyle) {
        let mut at = 0;
        let mut found = None;
        for run in &self.runs {
            let len = run.len();
            if len > 0 && (offset == 0 || (offset > at && offset <= at + len)) {
                found = Some(run);
                break;
            }
            at += len;
        }
        match found {
            Some(r) => (r.format.clone(), r.char_style.clone(), r.style.clone()),
            None => (self.mark_format.clone(), None, self.mark.clone()),
        }
    }

    /// The link on both sides of `offset` (text typed there is part of it).
    pub fn link_inside(&self, offset: usize) -> Option<crate::flow::Link> {
        let mut at = 0;
        let (mut before, mut after) = (None, None);
        for run in &self.runs {
            let len = run.len();
            if offset > at && offset <= at + len {
                before = Some(run.link.clone());
            }
            if offset >= at && offset < at + len {
                after = Some(run.link.clone());
            }
            at += len;
        }
        match (before, after) {
            (Some(a), Some(b)) if a == b => a,
            _ => None,
        }
    }

    /// The text content control (run index and its first position) that a
    /// caret at `offset` is in: after its first character up to its end, or,
    /// when it shows its prompt, anywhere in it.
    pub fn control_at(&self, offset: usize) -> Option<(usize, usize)> {
        let mut at = 0;
        for (i, run) in self.runs.iter().enumerate() {
            let len = run.len();
            if let FlowRunKind::RichText { placeholder, .. }
                | FlowRunKind::PlainText { placeholder, .. }
                | FlowRunKind::BuildingBlockGallery { placeholder, .. }
                | FlowRunKind::Group { placeholder, .. } = &run.kind {
                let inside = if *placeholder { offset >= at && offset <= at + len } else { offset > at && offset <= at + len };
                if inside {
                    return Some((i, at));
                }
            } else if let FlowRunKind::RepeatingSection { items, selected, .. } = &run.kind {
                let is_prompt = items.get(*selected).is_some_and(|s| s == crate::flow::DEFAULT_REPEATING_SECTION_ITEM_PROMPT);
                let inside = if is_prompt { offset >= at && offset <= at + len } else { offset > at && offset <= at + len };
                if inside {
                    return Some((i, at));
                }
            }
            at += len;
        }
        None
    }

    /// The runs before `offset`, a text run cut at it.
    pub fn runs_before(&self, offset: usize) -> Vec<FlowRun> {
        let mut p = self.clone();
        p.split_off_runs(offset);
        p.runs
    }

    /// The runs after `offset`.
    pub fn runs_after(&self, offset: usize) -> Vec<FlowRun> {
        let mut p = self.clone();
        p.split_off_runs(offset)
    }

    /// Cut the runs at `offset` (a text run in two); return the runs after it.
    pub fn split_off_runs(&mut self, offset: usize) -> Vec<FlowRun> {
        let index = self.boundary(offset);
        self.runs.split_off(index)
    }

    /// Make a run boundary at `offset`; return the index of the run that
    /// starts there.
    fn boundary(&mut self, offset: usize) -> usize {
        let mut at = 0;
        for i in 0..self.runs.len() {
            let len = self.runs[i].len();
            if offset == at {
                return i;
            }
            if offset < at + len {
                let FlowRunKind::Text(t) = &mut self.runs[i].kind else { return i + 1 };
                let byte = t.char_indices().nth(offset - at).map_or(t.len(), |(b, _)| b);
                let rest = t.split_off(byte);
                let mut second = self.runs[i].clone();
                second.kind = FlowRunKind::Text(rest);
                self.runs.insert(i + 1, second);
                return i + 1;
            }
            at += len;
        }
        self.runs.len()
    }

    /// Apply `change` to the runs from `start` to `end`.
    /// Set the tracked change of the runs from `start` to `end` (split at
    /// both ends) to what `f` gives for each.
    pub fn set_revision(&mut self, start: usize, end: usize, f: impl Fn(&FlowRun) -> Option<crate::flow::Revision>) {
        if start >= end {
            return;
        }
        let a = self.boundary(start);
        let b = self.boundary(end);
        for run in &mut self.runs[a..b] {
            run.revision = f(run);
        }
    }

    pub fn format_range(&mut self, start: usize, end: usize, change: &RunChange) {
        if start >= end {
            return;
        }
        let a = self.boundary(start);
        let b = self.boundary(end);
        for run in &mut self.runs[a..b] {
            change.apply(&mut run.format);
        }
    }

    /// Convert the character case of the text runs from `start` to `end`.
    pub fn convert_case_range(&mut self, start: usize, end: usize, conversion: CaseConversion) -> usize {
        if start >= end {
            return 0;
        }
        let a = self.boundary(start);
        let b = self.boundary(end);
        let mut new_len = 0;
        for run in &mut self.runs[a..b] {
            if let FlowRunKind::Text(t) = &mut run.kind {
                *t = conversion.apply(t);
                new_len += t.chars().count();
            } else {
                new_len += run.len();
            }
        }
        new_len
    }

    /// Join neighbouring text runs of the same formatting, and drop empty
    /// ones (as the reader does).
    pub fn normalize(&mut self) {
        let mut out: Vec<FlowRun> = Vec::with_capacity(self.runs.len());
        for run in self.runs.drain(..) {
            if let FlowRunKind::Text(t) = &run.kind {
                if t.is_empty() {
                    continue;
                }
                if let Some(FlowRun { kind: FlowRunKind::Text(prev), style, format, char_style, link, revision, .. }) = out.last_mut()
                    && *style == run.style
                    && *format == run.format
                    && *char_style == run.char_style
                    && *link == run.link
                    && *revision == run.revision
                {
                    prev.push_str(t);
                    continue;
                }
            }
            out.push(run);
        }
        self.runs = out;
    }

    /// Whether position `offset` holds a mark that is not drawn (a
    /// comment's), which the caret passes over.
    pub fn is_mark(&self, offset: usize) -> bool {
        let mut at = 0;
        for run in &self.runs {
            let len = run.len();
            if offset < at + len {
                return matches!(run.kind, FlowRunKind::CommentStart(_) | FlowRunKind::CommentEnd(_) | FlowRunKind::CommentRef(_));
            }
            at += len;
        }
        false
    }

    /// The paragraph's text: a tab for a tab, U+000B for a line break, U+000C
    /// for a page break, U+000E for a column break and U+FFFC for a picture or
    /// a field, so that each position is one character.
    pub fn text(&self) -> String {
        let mut s = String::new();
        for run in &self.runs {
            match &run.kind {
                FlowRunKind::Text(t) => s.push_str(t),
                FlowRunKind::Tab | FlowRunKind::PositionalTab { .. } => s.push('\t'),
                FlowRunKind::LineBreak => s.push('\u{b}'),
                FlowRunKind::PageBreak => s.push('\u{c}'),
                FlowRunKind::ColumnBreak => s.push('\u{e}'),
                FlowRunKind::CheckBox { checked, checked_char, unchecked_char } => {
                    s.push(if *checked { *checked_char } else { *unchecked_char });
                }
                // One position: an object character stands for the drop-down.
                FlowRunKind::DropDownList { .. } => s.push('\u{fffc}'),
                FlowRunKind::DatePicker { text, .. } => {
                    s.push_str(text);
                }
                FlowRunKind::RichText { text, .. }
                | FlowRunKind::PlainText { text, .. }
                | FlowRunKind::BuildingBlockGallery { text, .. }
                | FlowRunKind::Group { text, .. } => {
                    s.push_str(text);
                }
                FlowRunKind::RepeatingSection { items, selected, .. } => {
                    if let Some(item) = items.get(*selected) {
                        s.push_str(item);
                    }
                }
                FlowRunKind::FormField(field) => match &field.kind {
                    crate::flow::FormFieldKind::TextInput { value, default, .. } => {
                        if value.is_empty() {
                            if !default.is_empty() {
                                s.push_str(default);
                            } else {
                                s.push_str("\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}");
                            }
                        } else {
                            s.push_str(value);
                        }
                    }
                    crate::flow::FormFieldKind::CheckBox { checked, .. } => {
                        s.push(if *checked { '☒' } else { '☐' });
                    }
                    crate::flow::FormFieldKind::DropDown { items, selected, .. } => {
                        if let Some(item) = items.get(*selected) {
                            s.push_str(item);
                        }
                    }
                },
                _ => s.push('\u{fffc}'),
            }
        }
        s
    }
}

/// The blocks of a list.
pub fn blocks_mut(doc: &mut FlowDocument, container: Container) -> Option<&mut Vec<Block>> {
    fn find(blocks: &mut [Block], table: u32, row: usize, cell: usize) -> Option<&mut Vec<Block>> {
        for b in blocks {
            if let Block::Table(t) = b {
                if t.id == table {
                    return t.rows.get_mut(row)?.cells.get_mut(cell).map(|c| &mut c.blocks);
                }
                for r in &mut t.rows {
                    for c in &mut r.cells {
                        if let Some(found) = find(&mut c.blocks, table, row, cell) {
                            return Some(found);
                        }
                    }
                }
            }
        }
        None
    }
    match container {
        Container::Body => Some(&mut doc.body),
        Container::Header => Some(&mut doc.header),
        Container::Footer => Some(&mut doc.footer),
        Container::Part { section: 0, kind } => Some(match kind {
            crate::flow::PartKind::Header => &mut doc.header,
            crate::flow::PartKind::Footer => &mut doc.footer,
            crate::flow::PartKind::HeaderFirst => &mut doc.header_first,
            crate::flow::PartKind::FooterFirst => &mut doc.footer_first,
            crate::flow::PartKind::HeaderEven => &mut doc.header_even,
            crate::flow::PartKind::FooterEven => &mut doc.footer_even,
        }),
        Container::Part { section, kind } => doc.body.iter_mut().find_map(|b| match b {
            Block::Paragraph(p) if p.id == section => p.section.as_mut().and_then(|s| Arc::make_mut(s).parts[kind.index()].as_mut()),
            _ => None,
        }),
        Container::Note { endnote, id } => (if endnote { &mut doc.endnotes } else { &mut doc.footnotes }).iter_mut().find(|n| n.id == id).map(|n| &mut n.blocks),
        Container::Comment(id) => doc.comments.iter_mut().find(|c| c.id == id).map(|c| &mut c.blocks),
        Container::Cell { table, row, cell } => {
            if blocks(doc, Container::Cell { table, row, cell }).is_none() {
                return None;
            }
            let FlowDocument { body, header, footer, .. } = doc;
            for list in [body, header, footer] {
                if let Some(found) = find(list, table, row, cell) {
                    return Some(found);
                }
            }
            None
        }
    }
}

/// The blocks of a list, to read.
pub fn blocks(doc: &FlowDocument, container: Container) -> Option<&[Block]> {
    fn find(blocks: &[Block], table: u32, row: usize, cell: usize) -> Option<&[Block]> {
        for b in blocks {
            if let Block::Table(t) = b {
                if t.id == table {
                    return t.rows.get(row)?.cells.get(cell).map(|c| c.blocks.as_slice());
                }
                for r in &t.rows {
                    for c in &r.cells {
                        if let Some(found) = find(&c.blocks, table, row, cell) {
                            return Some(found);
                        }
                    }
                }
            }
        }
        None
    }
    match container {
        Container::Body => Some(&doc.body),
        Container::Header => Some(&doc.header),
        Container::Footer => Some(&doc.footer),
        Container::Part { section: 0, kind } => Some(match kind {
            crate::flow::PartKind::Header => &doc.header,
            crate::flow::PartKind::Footer => &doc.footer,
            crate::flow::PartKind::HeaderFirst => &doc.header_first,
            crate::flow::PartKind::FooterFirst => &doc.footer_first,
            crate::flow::PartKind::HeaderEven => &doc.header_even,
            crate::flow::PartKind::FooterEven => &doc.footer_even,
        }),
        Container::Part { section, kind } => doc.body.iter().find_map(|b| match b {
            Block::Paragraph(p) if p.id == section => p.section.as_ref().and_then(|s| s.parts[kind.index()].as_deref()),
            _ => None,
        }),
        Container::Note { endnote, id } => (if endnote { &doc.endnotes } else { &doc.footnotes }).iter().find(|n| n.id == id).map(|n| n.blocks.as_slice()),
        Container::Comment(id) => doc.comments.iter().find(|c| c.id == id).map(|c| c.blocks.as_slice()),
        Container::Cell { table, row, cell } => {
            find(&doc.body, table, row, cell).or_else(|| find(&doc.header, table, row, cell)).or_else(|| find(&doc.footer, table, row, cell))
        }
    }
}

/// The list a paragraph (or table) is in, and its index there.
pub fn locate(doc: &FlowDocument, id: u32) -> Option<(Container, usize)> {
    fn find(blocks: &[Block], id: u32, here: Container) -> Option<(Container, usize)> {
        for (i, b) in blocks.iter().enumerate() {
            match b {
                Block::Paragraph(p) if p.id == id => return Some((here, i)),
                Block::Table(t) if t.id == id => return Some((here, i)),
                Block::Table(t) => {
                    for (r, row) in t.rows.iter().enumerate() {
                        for (c, cell) in row.cells.iter().enumerate() {
                            if let Some(found) = find(&cell.blocks, id, Container::Cell { table: t.id, row: r, cell: c }) {
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
    find(&doc.body, id, Container::Body).or_else(|| find(&doc.header, id, Container::Header)).or_else(|| find(&doc.footer, id, Container::Footer)).or_else(|| {
        use crate::flow::PartKind;
        for (list, kind) in [(&doc.header_first, PartKind::HeaderFirst), (&doc.footer_first, PartKind::FooterFirst), (&doc.header_even, PartKind::HeaderEven), (&doc.footer_even, PartKind::FooterEven)] {
            if let Some(found) = find(list, id, Container::Part { section: 0, kind }) {
                return Some(found);
            }
        }
        for b in &doc.body {
            if let Block::Paragraph(p) = b
                && let Some(section) = &p.section
            {
                for kind in PartKind::ALL {
                    if let Some(list) = &section.parts[kind.index()]
                        && let Some(found) = find(list, id, Container::Part { section: p.id, kind })
                    {
                        return Some(found);
                    }
                }
            }
        }
        for (endnote, notes) in [(false, &doc.footnotes), (true, &doc.endnotes)] {
            for note in notes {
                if let Some(found) = find(&note.blocks, id, Container::Note { endnote, id: note.id }) {
                    return Some(found);
                }
            }
        }
        for c in &doc.comments {
            if let Some(found) = find(&c.blocks, id, Container::Comment(c.id)) {
                return Some(found);
            }
        }
        None
    })
}

/// The paragraph with this id.
pub fn paragraph(doc: &FlowDocument, id: u32) -> Option<&FlowParagraph> {
    let (container, index) = locate(doc, id)?;
    match blocks(doc, container)?.get(index)? {
        Block::Paragraph(p) => Some(p),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{FlowRunStyle, FlowStyle, StyleKind};

    #[test]
    fn test_case_conversion_text() {
        assert_eq!(CaseConversion::Sentence.apply("hello world. this is ibis. OK? yes!"), "Hello world. This is ibis. Ok? Yes!");
        assert_eq!(CaseConversion::Lower.apply("HeLLo WoRLD ＡＢＣ"), "hello world ａｂｃ");
        assert_eq!(CaseConversion::Upper.apply("hello world ａｂｃ"), "HELLO WORLD ＡＢＣ");
        assert_eq!(CaseConversion::Title.apply("hello world/sample-test"), "Hello World/Sample-Test");
        assert_eq!(CaseConversion::Toggle.apply("Hello World"), "hELLO wORLD");

        let full_kana = "アイウエオ　ガギグゲゴ　パピプペポ　ＡＢＣ１２３！";
        let half_kana = "ｱｲｳｴｵ ｶﾞｷﾞｸﾞｹﾞｺﾞ ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟ ABC123!";
        assert_eq!(CaseConversion::HalfWidth.apply(full_kana), half_kana);
        assert_eq!(CaseConversion::FullWidth.apply(half_kana), full_kana);

        assert_eq!(CaseConversion::Katakana.apply("あいうえお がぎぐげご ぱぴぷぺぽ ゔ"), "アイウエオ ガギグゲゴ パピプペポ ヴ");
        assert_eq!(CaseConversion::Hiragana.apply("アイウエオ ガギグゲゴ パピプペポ ヴ"), "あいうえお がぎぐげご ぱぴぷぺぽ ゔ");
    }

    #[test]
    fn test_flow_command_convert_case() {
        let mut doc = FlowDocument::default();
        let p_id = doc.new_id();
        let mut p = FlowParagraph { id: p_id, ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(FlowRunKind::Text("hello world".into()), FlowRunStyle::default()));
        doc.body.push(Block::Paragraph(p));

        // Convert "hello" (0..5) to Upper -> "HELLO world"
        let cmd = FlowCommand::ConvertCase {
            from: FlowPos::new(p_id, 0),
            to: FlowPos::new(p_id, 5),
            conversion: CaseConversion::Upper,
        };
        let undo = apply_to(&mut doc, cmd).unwrap();
        let Block::Paragraph(res) = &doc.body[0] else { panic!() };
        assert_eq!(res.text(), "HELLO world");

        // Undo -> "hello world"
        apply_to(&mut doc, undo).unwrap();
        let Block::Paragraph(res_undo) = &doc.body[0] else { panic!() };
        assert_eq!(res_undo.text(), "hello world");
    }

    #[test]
    fn test_comment_range_and_at_pos() {
        let mut doc = FlowDocument::default();
        let p_id = doc.new_id();
        let mut p = FlowParagraph { id: p_id, ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(FlowRunKind::Text("Hello ".into()), FlowRunStyle::default())); // 0..6
        p.runs.push(FlowRun::new(FlowRunKind::CommentStart(1), FlowRunStyle::default())); // 6..7
        p.runs.push(FlowRun::new(FlowRunKind::Text("world".into()), FlowRunStyle::default())); // 7..12
        p.runs.push(FlowRun::new(FlowRunKind::CommentEnd(1), FlowRunStyle::default())); // 12..13
        p.runs.push(FlowRun::new(FlowRunKind::CommentRef(1), FlowRunStyle::default())); // 13..14
        p.runs.push(FlowRun::new(FlowRunKind::Text("!".into()), FlowRunStyle::default())); // 14..15
        doc.body.push(Block::Paragraph(p));

        let range = comment_range(&doc, 1);
        assert_eq!(range, Some((FlowPos::new(p_id, 6), FlowPos::new(p_id, 12))));

        assert_eq!(comment_at_pos(&doc, FlowPos::new(p_id, 0)), None);
        assert_eq!(comment_at_pos(&doc, FlowPos::new(p_id, 5)), None);
        assert_eq!(comment_at_pos(&doc, FlowPos::new(p_id, 6)), Some(1));
        assert_eq!(comment_at_pos(&doc, FlowPos::new(p_id, 9)), Some(1));
        assert_eq!(comment_at_pos(&doc, FlowPos::new(p_id, 12)), Some(1));
        assert_eq!(comment_at_pos(&doc, FlowPos::new(p_id, 13)), Some(1));
        assert_eq!(comment_at_pos(&doc, FlowPos::new(p_id, 14)), None);
    }

    #[test]
    fn test_unlink_fields() {
        let mut doc = FlowDocument::default();
        let p_id = doc.new_id();
        let mut p = FlowParagraph { id: p_id, ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(FlowRunKind::Text("前文 ".into()), FlowRunStyle::default()));
        p.runs.push(FlowRun::new(FlowRunKind::Field { instruction: "SEQ 図 \\* ARABIC".into(), shown: "1".into() }, FlowRunStyle::default()));
        p.runs.push(FlowRun::new(FlowRunKind::Text(" 中間 ".into()), FlowRunStyle::default()));
        p.runs.push(FlowRun::new(FlowRunKind::PageRef { bookmark: "_Ref1".into(), shown: "10".into() }, FlowRunStyle::default()));
        p.runs.push(FlowRun::new(FlowRunKind::Text(" 後文".into()), FlowRunStyle::default()));
        doc.body.push(Block::Paragraph(p));

        // 1. 部分選択（SEQ のみ含む範囲: offset 3..4）でリンク解除
        let cmds = unlink_fields(&doc, Some((FlowPos::new(p_id, 3), FlowPos::new(p_id, 4))));
        assert_eq!(cmds.len(), 1);
        let undo = apply_to(&mut doc, cmds[0].clone()).unwrap();
        let Block::Paragraph(p_res) = &doc.body[0] else { panic!() };
        // SEQ フィールドはプレーンテキスト化され、PageRef は維持されている（未解除のフィールドは \u{fffc}）
        assert!(p_res.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::PageRef { .. })));
        assert_eq!(p_res.text(), "前文 1 中間 \u{fffc} 後文");

        // 2. Undo で元に戻る
        apply_to(&mut doc, undo).unwrap();
        let Block::Paragraph(p_undo) = &doc.body[0] else { panic!() };
        assert!(p_undo.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Field { .. })));
        assert_eq!(p_undo.text(), "前文 \u{fffc} 中間 \u{fffc} 後文");

        // 3. 全体選択（None）ですべてのフィールドを解除
        let cmds_all = unlink_fields(&doc, None);
        assert_eq!(cmds_all.len(), 1);
        apply_to(&mut doc, cmds_all[0].clone()).unwrap();
        let Block::Paragraph(p_all) = &doc.body[0] else { panic!() };
        // 全フィールドが Text になり、Field や PageRef は存在しない
        assert!(!p_all.runs.iter().any(|r| is_field_kind(&r.kind)));
        assert_eq!(p_all.text(), "前文 1 中間 10 後文");
    }

    #[test]
    fn test_toggle_field_codes() {
        let mut doc = FlowDocument::default();
        let p_id = doc.new_id();
        let mut p = FlowParagraph { id: p_id, ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(FlowRunKind::Text("前文 ".into()), FlowRunStyle::default()));
        p.runs.push(FlowRun::new(FlowRunKind::Field { instruction: "SEQ 図 \\* ARABIC".into(), shown: "1".into() }, FlowRunStyle::default()));
        p.runs.push(FlowRun::new(FlowRunKind::Text(" 中間 ".into()), FlowRunStyle::default()));
        p.runs.push(FlowRun::new(FlowRunKind::PageNumber, FlowRunStyle::default()));
        p.runs.push(FlowRun::new(FlowRunKind::Text(" 後文".into()), FlowRunStyle::default()));
        doc.body.push(Block::Paragraph(p));

        // 1. 文書全体トグル (Alt+F9)
        assert!(!doc.show_field_codes);
        let cmd_all = toggle_all_field_codes(&doc);
        let undo_all = apply_to(&mut doc, cmd_all).unwrap();
        assert!(doc.show_field_codes);
        // Undo
        let redo_all = apply_to(&mut doc, undo_all).unwrap();
        assert!(!doc.show_field_codes);
        // Redo
        apply_to(&mut doc, redo_all).unwrap();
        assert!(doc.show_field_codes);
        // Reset to false
        apply_to(&mut doc, FlowCommand::SetShowFieldCodes(false)).unwrap();
        assert!(!doc.show_field_codes);

        // 2. 個別フィールドトグル (Shift+F9, SEQ: offset 3..4)
        let cmds = toggle_field_codes_at(&doc, Some((FlowPos::new(p_id, 3), FlowPos::new(p_id, 4))));
        assert_eq!(cmds.len(), 1);
        let undo_field = apply_to(&mut doc, cmds[0].clone()).unwrap();
        let Block::Paragraph(p_res) = &doc.body[0] else { panic!() };
        assert!(p_res.runs[1].show_field_code);
        assert!(p_res.runs[1].is_field_code_shown(doc.show_field_codes));
        assert_eq!(p_res.runs[1].display_field_code(), Some("{ SEQ 図 \\* ARABIC }".into()));
        // PageNumber は未トグル
        assert!(!p_res.runs[3].show_field_code);
        assert!(!p_res.runs[3].is_field_code_shown(doc.show_field_codes));

        // 3. Undo で元に戻る
        let redo_field = apply_to(&mut doc, undo_field).unwrap();
        let Block::Paragraph(p_undo) = &doc.body[0] else { panic!() };
        assert!(!p_undo.runs[1].show_field_code);

        // 4. Redo で再適用
        apply_to(&mut doc, redo_field).unwrap();
        let Block::Paragraph(p_redo) = &doc.body[0] else { panic!() };
        assert!(p_redo.runs[1].show_field_code);
    }

    #[test]
    fn test_lock_fields() {
        let mut doc = FlowDocument::default();
        let p1_id = doc.new_id();
        let mut p1 = FlowParagraph { id: p1_id, ..FlowParagraph::default() };
        p1.runs.push(FlowRun::new(FlowRunKind::Field { instruction: "SEQ 図 \\* ARABIC".into(), shown: "old".into() }, FlowRunStyle::default()));
        doc.body.push(Block::Paragraph(p1));

        let p2_id = doc.new_id();
        let mut p2 = FlowParagraph { id: p2_id, ..FlowParagraph::default() };
        p2.runs.push(FlowRun::new(FlowRunKind::Field { instruction: "SEQ 図 \\* ARABIC".into(), shown: "locked_val".into() }, FlowRunStyle::default()));
        doc.body.push(Block::Paragraph(p2));

        // 1. p2 のフィールドをロック
        let cmds = lock_fields_at(&doc, Some((FlowPos::new(p2_id, 0), FlowPos::new(p2_id, 1))), true);
        assert_eq!(cmds.len(), 1);
        let undo_cmd = apply_to(&mut doc, cmds.into_iter().next().unwrap()).unwrap();
        let Block::Paragraph(p2_locked) = &doc.body[1] else { panic!() };
        assert!(p2_locked.runs[0].field_locked);

        // 2. フィールド更新を実行 -> p1 は "1" に更新されるが、p2 はロックされているので "locked_val" のまま
        let update_cmds = updated_fields(&doc);
        for cmd in update_cmds {
            apply_to(&mut doc, cmd).unwrap();
        }
        let Block::Paragraph(p1_cur) = &doc.body[0] else { panic!() };
        let Block::Paragraph(p2_cur) = &doc.body[1] else { panic!() };
        assert_eq!(p1_cur.runs[0].kind, FlowRunKind::Field { instruction: "SEQ 図 \\* ARABIC".into(), shown: "1".into() });
        assert_eq!(p2_cur.runs[0].kind, FlowRunKind::Field { instruction: "SEQ 図 \\* ARABIC".into(), shown: "locked_val".into() });

        // 3. ロック解除
        let unlock_cmds = lock_fields_at(&doc, Some((FlowPos::new(p2_id, 0), FlowPos::new(p2_id, 1))), false);
        for cmd in unlock_cmds {
            apply_to(&mut doc, cmd).unwrap();
        }
        let Block::Paragraph(p2_unlocked) = &doc.body[1] else { panic!() };
        assert!(!p2_unlocked.runs[0].field_locked);

        // 4. 再度更新 -> p2 も "2" に更新される
        let update_cmds2 = updated_fields(&doc);
        for cmd in update_cmds2 {
            apply_to(&mut doc, cmd).unwrap();
        }
        let Block::Paragraph(p2_updated) = &doc.body[1] else { panic!() };
        assert_eq!(p2_updated.runs[0].kind, FlowRunKind::Field { instruction: "SEQ 図 \\* ARABIC".into(), shown: "2".into() });

        // 5. Undo でロック状態が戻る
        apply_to(&mut doc, undo_cmd).unwrap();
        let Block::Paragraph(p2_undone) = &doc.body[1] else { panic!() };
        assert!(!p2_undone.runs[0].field_locked);
    }

    #[test]
    fn test_insert_and_edit_fields() {
        let mut doc = FlowDocument::default();
        let p_id = doc.new_id();
        let mut p = FlowParagraph { id: p_id, ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(FlowRunKind::Text("PAGE".into()), FlowRunStyle::default()));
        doc.body.push(Block::Paragraph(p));

        // 1. "PAGE" の範囲を選択してフィールドに変換（Word の ⌘F9 / Ctrl+F9）
        let (cmds, pos) = insert_field_at_range(&doc, Some((FlowPos::new(p_id, 0), FlowPos::new(p_id, 4))), "", true).unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(pos, FlowPos::new(p_id, 1));
        let undo_cmd = apply_to(&mut doc, cmds.into_iter().next().unwrap()).unwrap();

        let Block::Paragraph(p_cur) = &doc.body[0] else { panic!() };
        assert_eq!(p_cur.runs.len(), 1);
        assert_eq!(p_cur.runs[0].kind, FlowRunKind::Field { instruction: "PAGE".into(), shown: "1".into() });
        assert!(p_cur.runs[0].show_field_code);

        // 2. get_field_at でコード取得
        assert_eq!(get_field_at(&doc, FlowPos::new(p_id, 0)), Some("PAGE".into()));

        // 3. edit_field_at でコードを変更
        let edit_cmds = edit_field_at(&doc, FlowPos::new(p_id, 0), "NUMPAGES").unwrap();
        assert_eq!(edit_cmds.len(), 1);
        let undo_edit = apply_to(&mut doc, edit_cmds.into_iter().next().unwrap()).unwrap();

        let Block::Paragraph(p_edited) = &doc.body[0] else { panic!() };
        assert_eq!(p_edited.runs[0].kind, FlowRunKind::Field { instruction: "NUMPAGES".into(), shown: "1".into() });
        assert_eq!(get_field_at(&doc, FlowPos::new(p_id, 0)), Some("NUMPAGES".into()));

        // 4. Undo edit で "PAGE" に復元
        apply_to(&mut doc, undo_edit).unwrap();
        let Block::Paragraph(p_reverted) = &doc.body[0] else { panic!() };
        assert_eq!(p_reverted.runs[0].kind, FlowRunKind::Field { instruction: "PAGE".into(), shown: "1".into() });

        // 5. Undo insert で元のテキスト "PAGE" に復元
        apply_to(&mut doc, undo_cmd).unwrap();
        let Block::Paragraph(p_original) = &doc.body[0] else { panic!() };
        assert_eq!(p_original.runs[0].kind, FlowRunKind::Text("PAGE".into()));
    }

    #[test]
    fn test_insert_caption_detailed() {
        let mut doc = FlowDocument::default();
        let p_id = doc.new_id();
        let p = FlowParagraph { id: p_id, ..FlowParagraph::default() };
        doc.body.push(Block::Paragraph(p));

        // 1. 数式ラベル + ローマ数字大文字 (I, II, ...) + 説明文
        let (cmds, _bm) = insert_caption_detailed(&doc, p_id, "数式", "オイラーの等式", CaptionNumberFormat::UpperRoman, false).unwrap();
        for cmd in cmds {
            apply_to(&mut doc, cmd).unwrap();
        }

        let Block::Paragraph(p_cap) = &doc.body[1] else { panic!() };
        assert_eq!(p_cap.runs.len(), 3);
        assert_eq!(p_cap.runs[0].kind, FlowRunKind::Text("数式 ".into()));
        assert_eq!(p_cap.runs[1].kind, FlowRunKind::Field { instruction: "SEQ 数式 \\* ROMAN".into(), shown: "I".into() });
        assert_eq!(p_cap.runs[2].kind, FlowRunKind::Text(" オイラーの等式".into()));

        // 2. caption_labels で "数式" が取得できる
        let labels = caption_labels(&doc);
        assert!(labels.contains(&"数式".to_string()));

        // 3. ラベル除外 (exclude_label = true)
        let (cmds2, _) = insert_caption_detailed(&doc, p_cap.id, "数式", "", CaptionNumberFormat::Arabic, true).unwrap();
        for cmd in cmds2 {
            apply_to(&mut doc, cmd).unwrap();
        }
        let Block::Paragraph(p_cap2) = &doc.body[2] else { panic!() };
        assert_eq!(p_cap2.runs.len(), 1);
        assert_eq!(p_cap2.runs[0].kind, FlowRunKind::Field { instruction: "SEQ 数式 \\* ARABIC".into(), shown: "1".into() });
    }

    #[test]
    fn test_table_of_figures_entries() {
        let mut doc = FlowDocument::default();
        let p_id = doc.new_id();
        let p = FlowParagraph { id: p_id, ..FlowParagraph::default() };
        doc.body.push(Block::Paragraph(p));

        // 1. 図 1 と 図 2 を挿入
        let (cmds1, _) = insert_caption_detailed(&doc, p_id, "図", "猫", CaptionNumberFormat::Arabic, false).unwrap();
        for cmd in cmds1 { apply_to(&mut doc, cmd).unwrap(); }
        let last_id1 = match doc.body.last().unwrap() { Block::Paragraph(p) => p.id, _ => panic!() };
        let (cmds2, _) = insert_caption_detailed(&doc, last_id1, "図", "犬", CaptionNumberFormat::Arabic, false).unwrap();
        for cmd in cmds2 { apply_to(&mut doc, cmd).unwrap(); }
        // 2. 表 1 を挿入
        let last_id2 = match doc.body.last().unwrap() { Block::Paragraph(p) => p.id, _ => panic!() };
        let (cmds3, _) = insert_caption_detailed(&doc, last_id2, "表", "売上", CaptionNumberFormat::Arabic, false).unwrap();
        for cmd in cmds3 { apply_to(&mut doc, cmd).unwrap(); }

        let page_of = |_: u32| Some(1);
        let width = 5_000_000;

        // 3. 図の図表目次を生成
        let (entries, _bms) = table_of_figures_entries(&doc, "図", &page_of, 100, width);
        assert_eq!(entries.len(), 2, "2 figure captions found");
        assert!(entries[0].field_start.as_ref().unwrap().contains("TOC \\h \\z \\c \"図\""));
        assert!(entries[1].field_end);
        assert!(entries[0].runs[0].kind == FlowRunKind::Text("図 1 猫".into()) || entries[0].text().contains("猫"));
        assert_eq!(entries[0].runs[1].kind, FlowRunKind::Tab);
        assert!(matches!(entries[0].runs[2].kind, FlowRunKind::PageRef { .. }));

        // 4. toc_caption_label ヘルパー
        assert_eq!(toc_caption_label("TOC \\h \\z \\c \"図\""), Some("図".to_string()));
        assert_eq!(toc_caption_label("TOC \\o \"1-3\" \\h \\z"), None);
        assert_eq!(toc_caption_label("TOC \\c Figure"), Some("Figure".to_string()));

        // 5. 該当ラベルがない場合の図表目次
        let (entries_none, _) = table_of_figures_entries(&doc, "グラフ", &page_of, 200, width);
        assert_eq!(entries_none.len(), 1);
        assert!(entries_none[0].text().contains("見つかりません"));
    }

    #[test]
    fn test_toc_instruction_and_custom_styles() {
        // 1. パースと生成のテスト
        let instr_str = "TOC \\o \"1-3\" \\h \\z \\u";
        let parsed = parse_toc_instruction(instr_str);
        assert_eq!(parsed.heading_levels, Some((1, 3)));
        assert!(parsed.custom_styles.is_empty());
        assert!(parsed.hyperlinks);
        assert!(parsed.hide_in_web);
        assert!(parsed.use_outline_levels);
        assert_eq!(parsed.all_levels(), vec![1, 2, 3]);

        // 2. \t スイッチ（カスタムスタイル）のパース
        let t_str = "TOC \\t \"CustomTitle,1,SubSection,2\" \\h \\z";
        let parsed_t = parse_toc_instruction(t_str);
        assert_eq!(parsed_t.heading_levels, None);
        assert_eq!(parsed_t.custom_styles, vec![("CustomTitle".to_string(), 1), ("SubSection".to_string(), 2)]);
        assert!(parsed_t.hyperlinks);
        assert!(parsed_t.hide_in_web);
        assert!(!parsed_t.use_outline_levels);
        assert_eq!(parsed_t.all_levels(), vec![1, 2]);
        assert_eq!(parsed_t.to_field_instruction(), "TOC \\t \"CustomTitle,1,SubSection,2\" \\h \\z");

        // セミコロン区切り・空白あり
        let semi_str = "TOC \\o \"1-2\" \\t \"My Heading, 1 ; Special, 3\"";
        let parsed_semi = parse_toc_instruction(semi_str);
        assert_eq!(parsed_semi.heading_levels, Some((1, 2)));
        assert_eq!(parsed_semi.custom_styles, vec![("My Heading".to_string(), 1), ("Special".to_string(), 3)]);
        assert_eq!(parsed_semi.all_levels(), vec![1, 2, 3]);

        // 3. カスタムスタイルを使用した目次エントリー生成の検証
        let mut doc = FlowDocument::default();
        // スタイルを登録
        let styles = std::sync::Arc::make_mut(&mut doc.styles);
        styles.styles.insert("CustomStyle".into(), FlowStyle {
            kind: StyleKind::Paragraph,
            name: "CustomStyle".into(),
            ..FlowStyle::default()
        });
        styles.styles.insert("Heading1".into(), FlowStyle {
            kind: StyleKind::Paragraph,
            name: "heading 1".into(),
            ..FlowStyle::default()
        });

        // 段落1: heading 1
        let mut p1 = FlowParagraph { id: 10, style: Some("Heading1".into()), ..FlowParagraph::default() };
        p1.runs.push(FlowRun::new(FlowRunKind::Text("第一章 はじめに".into()), FlowRunStyle::default()));
        doc.body.push(Block::Paragraph(p1));

        // 段落2: CustomStyle
        let mut p2 = FlowParagraph { id: 20, style: Some("CustomStyle".into()), ..FlowParagraph::default() };
        p2.runs.push(FlowRun::new(FlowRunKind::Text("特別コラム".into()), FlowRunStyle::default()));
        doc.body.push(Block::Paragraph(p2));

        // 段落3: 通常本文
        let mut p3 = FlowParagraph { id: 30, style: None, ..FlowParagraph::default() };
        p3.runs.push(FlowRun::new(FlowRunKind::Text("これは本文です。".into()), FlowRunStyle::default()));
        doc.body.push(Block::Paragraph(p3));

        let page_of = |id: u32| match id {
            10 => Some(1),
            20 => Some(3),
            _ => None,
        };
        let width = 5_000_000;

        // A. \t のみ: CustomStyle (level 2) のみを目次化
        let instr_custom_only = TocInstruction {
            heading_levels: None,
            custom_styles: vec![("CustomStyle".to_string(), 2)],
            ..Default::default()
        };
        let (entries_custom, _) = toc_entries_with_instruction(&doc, &instr_custom_only, &page_of, 100, width);
        assert_eq!(entries_custom.len(), 1);
        assert!(entries_custom[0].text().contains("特別コラム"));
        assert_eq!(entries_custom[0].runs[2].kind, FlowRunKind::PageRef { bookmark: "_Toc100000020".into(), shown: "3".into() });

        // B. \o と \t の併用: heading 1 (level 1) と CustomStyle (level 2) の両方を目次化
        let instr_combined = TocInstruction {
            heading_levels: Some((1, 1)),
            custom_styles: vec![("CustomStyle".to_string(), 2)],
            ..Default::default()
        };
        let (entries_comb, _) = toc_entries_with_instruction(&doc, &instr_combined, &page_of, 100, width);
        assert_eq!(entries_comb.len(), 2);
        assert!(entries_comb[0].text().contains("第一章 はじめに"));
        assert!(entries_comb[1].text().contains("特別コラム"));

        // C. missing_toc_styles_for_levels
        let missing = missing_toc_styles_for_levels(&doc.styles, vec![1, 2]);
        assert_eq!(missing.len(), 2);
        assert_eq!(missing[0].1.name, "toc 1");
        assert_eq!(missing[1].1.name, "toc 2");
    }

    #[test]
    fn test_table_edit_equal_rows() {
        use crate::flow::{FlowRow, FlowTable, RowHeight};
        let mut doc = FlowDocument::default();
        let table_id = doc.new_id();
        let pt = |v: i64| v * 12700;
        let table = FlowTable {
            id: table_id,
            columns: vec![pt(100), pt(100)],
            rows: vec![
                FlowRow { id: 10, height: RowHeight::AtLeast(pt(30)), ..FlowRow::default() },
                FlowRow { id: 11, height: RowHeight::Auto, ..FlowRow::default() }, // Auto defaults to 20pt
                FlowRow { id: 12, height: RowHeight::Exact(pt(40)), ..FlowRow::default() },
            ],
            ..FlowTable::default()
        };
        doc.body.push(Block::Table(table.clone()));

        // Total effective height: 30 + 20 + 40 = 90pt -> each row = 30pt (AtLeast)
        let cmd = FlowCommand::EditTable { table: table_id, edit: TableEdit::EqualRows };
        let undo = apply_to(&mut doc, cmd).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.rows[0].height, RowHeight::AtLeast(pt(30)));
        assert_eq!(t.rows[1].height, RowHeight::AtLeast(pt(30)));
        assert_eq!(t.rows[2].height, RowHeight::AtLeast(pt(30)));

        // Undo -> original heights
        apply_to(&mut doc, undo).unwrap();
        let Block::Table(t_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_undo.rows[0].height, RowHeight::AtLeast(pt(30)));
        assert_eq!(t_undo.rows[1].height, RowHeight::Auto);
        assert_eq!(t_undo.rows[2].height, RowHeight::Exact(pt(40)));

        // When all rows are Exact, EqualRows preserves Exact rule:
        let mut doc2 = FlowDocument::default();
        let table_id2 = doc2.new_id();
        doc2.body.push(Block::Table(FlowTable {
            id: table_id2,
            columns: vec![pt(100)],
            rows: vec![
                FlowRow { id: 20, height: RowHeight::Exact(pt(10)), ..FlowRow::default() },
                FlowRow { id: 21, height: RowHeight::Exact(pt(30)), ..FlowRow::default() },
            ],
            ..FlowTable::default()
        }));
        let cmd2 = FlowCommand::EditTable { table: table_id2, edit: TableEdit::EqualRows };
        apply_to(&mut doc2, cmd2).unwrap();
        let Block::Table(t2) = &doc2.body[0] else { panic!() };
        assert_eq!(t2.rows[0].height, RowHeight::Exact(pt(20)));
        assert_eq!(t2.rows[1].height, RowHeight::Exact(pt(20)));
    }

    #[test]
    fn test_insert_column_break() {
        let mut doc = FlowDocument::default();
        let p_id = doc.new_id();
        let mut p = FlowParagraph { id: p_id, ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(FlowRunKind::Text("LeftRight".into()), FlowRunStyle::default()));
        doc.body.push(Block::Paragraph(p));

        // Insert column break '\u{e}' at offset 4 (between "Left" and "Right")
        let cmd = FlowCommand::InsertText { at: FlowPos::new(p_id, 4), text: "\u{e}".into(), format: None };
        let undo = apply_to(&mut doc, cmd).unwrap();
        let Block::Paragraph(res) = &doc.body[0] else { panic!() };
        assert_eq!(res.runs.len(), 3);
        assert_eq!(res.runs[0].kind, FlowRunKind::Text("Left".into()));
        assert_eq!(res.runs[1].kind, FlowRunKind::ColumnBreak);
        assert_eq!(res.runs[2].kind, FlowRunKind::Text("Right".into()));

        // Undo -> "LeftRight"
        apply_to(&mut doc, undo).unwrap();
        let Block::Paragraph(res_undo) = &doc.body[0] else { panic!() };
        assert_eq!(res_undo.text(), "LeftRight");
    }

    #[test]
    fn test_table_edit_autofit() {
        use crate::flow::{FlowCell, FlowRow, FlowTable, PreferredWidth};
        let mut doc = FlowDocument::default();
        let table_id = doc.new_id();
        let pt = |v: i64| v * 12700;
        let p_short = FlowParagraph { id: 101, runs: vec![FlowRun::new(FlowRunKind::Text("A".into()), FlowRunStyle::default())], ..FlowParagraph::default() };
        let p_long = FlowParagraph { id: 102, runs: vec![FlowRun::new(FlowRunKind::Text("VeryLongTextContentHere".into()), FlowRunStyle::default())], ..FlowParagraph::default() };
        let cell1 = FlowCell { blocks: vec![Block::Paragraph(p_short)], ..FlowCell::default() };
        let cell2 = FlowCell { blocks: vec![Block::Paragraph(p_long)], ..FlowCell::default() };
        let row = FlowRow { id: 10, cells: vec![cell1, cell2], ..FlowRow::default() };
        let table = FlowTable {
            id: table_id,
            columns: vec![pt(100), pt(100)],
            rows: vec![row],
            ..FlowTable::default()
        };
        doc.body.push(Block::Table(table));

        // 1. AutoFitContents
        let cmd = FlowCommand::EditTable { table: table_id, edit: TableEdit::AutoFitContents };
        let undo = apply_to(&mut doc, cmd).unwrap();
        let Block::Table(t_fit) = &doc.body[0] else { panic!() };
        assert!(t_fit.columns[1] > t_fit.columns[0], "longer text makes wider column");
        assert!(t_fit.autofit.is_some());

        // Undo -> restores initial 100pt widths
        apply_to(&mut doc, undo).unwrap();
        let Block::Table(t_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_undo.columns[0], pt(100));
        assert_eq!(t_undo.columns[1], pt(100));
        assert!(t_undo.autofit.is_none());

        // 2. AutoFitWindow
        let available = 5_400_000; // 150mm
        let cmd_win = FlowCommand::EditTable { table: table_id, edit: TableEdit::AutoFitWindow(available) };
        apply_to(&mut doc, cmd_win).unwrap();
        let Block::Table(t_win) = &doc.body[0] else { panic!() };
        let total_w: i64 = t_win.columns.iter().sum();
        assert_eq!(total_w, available);
        assert_eq!(t_win.autofit.as_ref().unwrap().width, PreferredWidth::Percent(5000));

        // 3. FixedColumns
        let cmd_fix = FlowCommand::EditTable { table: table_id, edit: TableEdit::FixedColumns };
        apply_to(&mut doc, cmd_fix).unwrap();
        let Block::Table(t_fix) = &doc.body[0] else { panic!() };
        assert!(t_fix.autofit.is_none());
        assert_eq!(t_fix.columns.iter().sum::<i64>(), available);
    }

    #[test]
    fn test_set_page_color() {
        let mut doc = FlowDocument::default();
        assert_eq!(doc.page_color, None);

        // Set to light yellow (#FFF2CC)
        let yellow = Rgb(0xFFF2CC);
        let cmd = FlowCommand::SetPageColor(Some(yellow));
        let undo = apply_to(&mut doc, cmd).unwrap();
        assert_eq!(doc.page_color, Some(yellow));

        // Undo -> None
        let redo = apply_to(&mut doc, undo).unwrap();
        assert_eq!(doc.page_color, None);

        // Redo -> yellow
        apply_to(&mut doc, redo).unwrap();
        assert_eq!(doc.page_color, Some(yellow));

        // Set to None
        let cmd_none = FlowCommand::SetPageColor(None);
        let undo_none = apply_to(&mut doc, cmd_none).unwrap();
        assert_eq!(doc.page_color, None);

        // Undo None -> yellow
        apply_to(&mut doc, undo_none).unwrap();
        assert_eq!(doc.page_color, Some(yellow));
    }

    #[test]
    fn test_table_edit_cell_spacing() {
        let mut doc = FlowDocument::default();
        let table_id = doc.new_id();
        let pt = |v: i64| v * 12700;
        let table = FlowTable {
            id: table_id,
            columns: vec![pt(100)],
            rows: vec![FlowRow { id: 10, ..FlowRow::default() }],
            cell_spacing: None,
            ..FlowTable::default()
        };
        doc.body.push(Block::Table(table));

        // Set cell spacing to 2 pt (25400 EMU)
        let cmd = FlowCommand::EditTable { table: table_id, edit: TableEdit::CellSpacing(Some(pt(2))) };
        let undo = apply_to(&mut doc, cmd).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        assert_eq!(t.cell_spacing, Some(pt(2)));

        // Undo -> restores None
        let redo = apply_to(&mut doc, undo).unwrap();
        let Block::Table(t_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_undo.cell_spacing, None);

        // Redo -> 2 pt
        apply_to(&mut doc, redo).unwrap();
        let Block::Table(t_redo) = &doc.body[0] else { panic!() };
        assert_eq!(t_redo.cell_spacing, Some(pt(2)));

        // Set to None
        let cmd_none = FlowCommand::EditTable { table: table_id, edit: TableEdit::CellSpacing(None) };
        let undo_none = apply_to(&mut doc, cmd_none).unwrap();
        let Block::Table(t_none) = &doc.body[0] else { panic!() };
        assert_eq!(t_none.cell_spacing, None);

        // Undo None -> 2 pt
        apply_to(&mut doc, undo_none).unwrap();
        let Block::Table(t_back) = &doc.body[0] else { panic!() };
        assert_eq!(t_back.cell_spacing, Some(pt(2)));
    }

    #[test]
    fn test_table_alignment_and_indent() {
        let mut doc = FlowDocument::default();
        let table_id = doc.new_id();
        let cell = FlowCell { blocks: vec![Block::Paragraph(FlowParagraph::default())], ..FlowCell::default() };
        let row = FlowRow { id: 10, cells: vec![cell], ..FlowRow::default() };
        let table = FlowTable {
            id: table_id,
            columns: vec![100 * 12700],
            rows: vec![row],
            align: FlowAlign::Left,
            indent: 0,
            ..FlowTable::default()
        };
        doc.body.push(Block::Table(table));

        // 1. Center alignment
        let cmd = FlowCommand::EditTable { table: table_id, edit: TableEdit::Alignment(FlowAlign::Center) };
        let undo = apply_to(&mut doc, cmd).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        assert_eq!(t.align, FlowAlign::Center);

        // Undo -> Left
        apply_to(&mut doc, undo).unwrap();
        let Block::Table(t_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_undo.align, FlowAlign::Left);

        // 2. Right alignment
        let cmd_right = FlowCommand::EditTable { table: table_id, edit: TableEdit::Alignment(FlowAlign::Right) };
        apply_to(&mut doc, cmd_right).unwrap();
        let Block::Table(t_right) = &doc.body[0] else { panic!() };
        assert_eq!(t_right.align, FlowAlign::Right);

        // 3. Indent 360,000 EMU (10 mm)
        let cmd_indent = FlowCommand::EditTable { table: table_id, edit: TableEdit::Indent(360_000) };
        let undo_indent = apply_to(&mut doc, cmd_indent).unwrap();
        let Block::Table(t_ind) = &doc.body[0] else { panic!() };
        assert_eq!(t_ind.indent, 360_000);

        // Undo -> 0
        apply_to(&mut doc, undo_indent).unwrap();
        let Block::Table(t_ind_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_ind_undo.indent, 0);
    }

    #[test]
    fn test_table_edit_no_wrap() {
        let mut doc = FlowDocument::default();
        let table_id = doc.new_id();
        let pt = |v: i64| v * 12700;
        let p = FlowParagraph { id: 10, runs: vec![FlowRun::new(FlowRunKind::Text("Hello World".into()), FlowRunStyle::default())], ..FlowParagraph::default() };
        doc.body.push(Block::Table(FlowTable {
            id: table_id,
            columns: vec![pt(100)],
            rows: vec![FlowRow {
                id: 20,
                cells: vec![FlowCell {
                    blocks: vec![Block::Paragraph(p)],
                    no_wrap: false,
                    ..FlowCell::default()
                }],
                ..FlowRow::default()
            }],
            ..FlowTable::default()
        }));

        // Set no_wrap = true
        let cmd = FlowCommand::EditTable { table: table_id, edit: TableEdit::NoWrap { cells: vec![(0, 0)], no_wrap: true } };
        let undo = apply_to(&mut doc, cmd).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        assert_eq!(t.rows[0].cells[0].no_wrap, true);

        // Undo -> restores no_wrap = false
        let redo = apply_to(&mut doc, undo).unwrap();
        let Block::Table(t_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_undo.rows[0].cells[0].no_wrap, false);

        // Redo -> no_wrap = true
        apply_to(&mut doc, redo).unwrap();
        let Block::Table(t_redo) = &doc.body[0] else { panic!() };
        assert_eq!(t_redo.rows[0].cells[0].no_wrap, true);
    }

    #[test]
    fn test_table_edit_insert_delete_cell_shift() {
        let mut doc = FlowDocument::default();
        let table_id = doc.new_id();
        let pt = |v: i64| v * 12700;
        let cell = |id: u32, text: &str| FlowCell {
            id,
            blocks: vec![Block::Paragraph(FlowParagraph {
                id: id + 100,
                runs: vec![FlowRun::new(FlowRunKind::Text(text.into()), FlowRunStyle::default())],
                ..FlowParagraph::default()
            })],
            span: 1,
            ..FlowCell::default()
        };
        let table = FlowTable {
            id: table_id,
            columns: vec![pt(100), pt(100)],
            rows: vec![
                FlowRow { id: 1, cells: vec![cell(10, "A"), cell(11, "B")], ..FlowRow::default() },
                FlowRow { id: 2, cells: vec![cell(20, "C"), cell(21, "D")], ..FlowRow::default() },
            ],
            ..FlowTable::default()
        };
        doc.body.push(Block::Table(table));

        // 1. InsertCellShiftRight at (0, 1) -> row 0 has 3 cells: A, empty, B
        let cmd = FlowCommand::EditTable { table: table_id, edit: TableEdit::InsertCellShiftRight { row: 0, cell: 1 } };
        let undo = apply_to(&mut doc, cmd).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        assert_eq!(t.rows[0].cells.len(), 3);
        assert_eq!(t.rows[1].cells.len(), 2);
        // Undo -> restores 2x2
        let redo = apply_to(&mut doc, undo).unwrap();
        let Block::Table(t_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_undo.rows[0].cells.len(), 2);
        // Redo
        apply_to(&mut doc, redo).unwrap();

        // Reset to original 2x2 table
        doc.body[0] = Block::Table(FlowTable {
            id: table_id,
            columns: vec![pt(100), pt(100)],
            rows: vec![
                FlowRow { id: 1, cells: vec![cell(10, "A"), cell(11, "B")], ..FlowRow::default() },
                FlowRow { id: 2, cells: vec![cell(20, "C"), cell(21, "D")], ..FlowRow::default() },
            ],
            ..FlowTable::default()
        });

        // 2. InsertCellShiftDown at (0, 0) -> rows count becomes 3
        let cmd_down = FlowCommand::EditTable { table: table_id, edit: TableEdit::InsertCellShiftDown { row: 0, cell: 0 } };
        let undo_down = apply_to(&mut doc, cmd_down).unwrap();
        let Block::Table(t_down) = &doc.body[0] else { panic!() };
        assert_eq!(t_down.rows.len(), 3);
        // Undo -> restores 2 rows
        apply_to(&mut doc, undo_down).unwrap();
        let Block::Table(t_down_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_down_undo.rows.len(), 2);

        // 3. DeleteCellShiftLeft at (0, 0)
        let cmd_del_left = FlowCommand::EditTable { table: table_id, edit: TableEdit::DeleteCellShiftLeft { row: 0, cell: 0 } };
        let undo_del_left = apply_to(&mut doc, cmd_del_left).unwrap();
        let Block::Table(t_del_left) = &doc.body[0] else { panic!() };
        assert_eq!(t_del_left.rows[0].cells.len(), 1);
        apply_to(&mut doc, undo_del_left).unwrap();
        let Block::Table(t_del_left_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_del_left_undo.rows[0].cells.len(), 2);

        // 4. DeleteCellShiftUp at (0, 0)
        let cmd_del_up = FlowCommand::EditTable { table: table_id, edit: TableEdit::DeleteCellShiftUp { row: 0, cell: 0 } };
        let undo_del_up = apply_to(&mut doc, cmd_del_up).unwrap();
        let Block::Table(t_del_up) = &doc.body[0] else { panic!() };
        // Row 0 cell 0 should now be C (moved from row 1)
        if let Block::Paragraph(p) = &t_del_up.rows[0].cells[0].blocks[0] {
            assert_eq!(p.text(), "C");
        }
        apply_to(&mut doc, undo_del_up).unwrap();
        let Block::Table(t_del_up_undo) = &doc.body[0] else { panic!() };
        if let Block::Paragraph(p) = &t_del_up_undo.rows[0].cells[0].blocks[0] {
            assert_eq!(p.text(), "A");
        }
    }

    #[test]
    fn test_nested_table_insertion() {
        let mut doc = FlowDocument::default();
        let table_id = doc.new_id();
        let para_id = 50;
        let pt = |v: i64| v * 12700;
        let cell = FlowCell {
            id: 20,
            blocks: vec![Block::Paragraph(FlowParagraph { id: para_id, ..FlowParagraph::default() })],
            span: 1,
            ..FlowCell::default()
        };
        let table = FlowTable {
            id: table_id,
            columns: vec![pt(200)],
            rows: vec![FlowRow { id: 10, cells: vec![cell], ..FlowRow::default() }],
            ..FlowTable::default()
        };
        doc.body.push(Block::Table(table));

        // Insert 2x2 nested table inside cell (0, 0)
        let cmd = FlowCommand::InsertTable {
            at: FlowPos::new(para_id, 0),
            rows: 2,
            columns: 2,
            width: pt(180),
        };
        let undo = apply_to(&mut doc, cmd).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        let blocks = &t.rows[0].cells[0].blocks;
        // There should be a table and a trailing paragraph (ECMA-376 requirement)
        assert_eq!(blocks.len(), 2);
        assert!(matches!(blocks[0], Block::Table(_)));
        assert!(matches!(blocks[1], Block::Paragraph(_)));

        // Undo -> restores single paragraph in cell
        apply_to(&mut doc, undo).unwrap();
        let Block::Table(t_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_undo.rows[0].cells[0].blocks.len(), 1);
        assert!(matches!(t_undo.rows[0].cells[0].blocks[0], Block::Paragraph(_)));
    }

    #[test]
    fn test_table_cell_paragraphs_and_lists() {
        let mut doc = FlowDocument::default();
        let (numbering, num_id) = doc.styles.numbering.with_new_list(true);
        std::sync::Arc::make_mut(&mut doc.styles).numbering = numbering;

        let table_id = doc.new_id();
        let para_id = 100;
        let pt = |v: i64| v * 12700;
        let cell = FlowCell {
            id: 20,
            blocks: vec![Block::Paragraph(FlowParagraph {
                id: para_id,
                runs: vec![FlowRun::new(FlowRunKind::Text("First".into()), FlowRunStyle::default())],
                ..FlowParagraph::default()
            })],
            span: 1,
            ..FlowCell::default()
        };
        let table = FlowTable {
            id: table_id,
            columns: vec![pt(200)],
            rows: vec![FlowRow { id: 10, cells: vec![cell], ..FlowRow::default() }],
            ..FlowTable::default()
        };
        doc.body.push(Block::Table(table));
        doc.id_floor = para_id;

        // 1. Insert newline to create a second paragraph inside cell
        let cmd_enter = FlowCommand::InsertText {
            at: FlowPos::new(para_id, 5),
            text: "\nSecond".into(),
            format: None,
        };
        let undo_enter = apply_to(&mut doc, cmd_enter).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        let blocks = &t.rows[0].cells[0].blocks;
        assert_eq!(blocks.len(), 2);
        let second_id = match (&blocks[0], &blocks[1]) {
            (Block::Paragraph(p1), Block::Paragraph(p2)) => {
                assert_eq!(p1.text(), "First");
                assert_eq!(p2.text(), "Second");
                p2.id
            }
            _ => panic!("Expected two paragraphs in cell"),
        };

        // 2. Apply bullet list to both paragraphs in the cell
        let cmd_list = FlowCommand::SetParagraphFormat {
            first: para_id,
            last: second_id,
            change: ParaChange::List(Some(num_id)),
        };
        let undo_list = apply_to(&mut doc, cmd_list).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        let blocks = &t.rows[0].cells[0].blocks;
        for b in blocks {
            let Block::Paragraph(p) = b else { panic!() };
            assert_eq!(p.list, Some(crate::flow::ListRef { num_id, level: 0 }));
        }

        // 3. Change level of second paragraph to level 1
        let cmd_level = FlowCommand::SetParagraphFormat {
            first: second_id,
            last: second_id,
            change: ParaChange::Level(1),
        };
        let undo_level = apply_to(&mut doc, cmd_level).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        let Block::Paragraph(p2) = &t.rows[0].cells[0].blocks[1] else { panic!() };
        assert_eq!(p2.list, Some(crate::flow::ListRef { num_id, level: 1 }));

        // 4. Insert another newline at the end of second paragraph -> inherits list and level
        let cmd_enter3 = FlowCommand::InsertText {
            at: FlowPos::new(second_id, 6),
            text: "\nThird".into(),
            format: None,
        };
        let undo_enter3 = apply_to(&mut doc, cmd_enter3).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        assert_eq!(t.rows[0].cells[0].blocks.len(), 3);
        let Block::Paragraph(p3) = &t.rows[0].cells[0].blocks[2] else { panic!() };
        assert_eq!(p3.text(), "Third");
        assert_eq!(p3.list, Some(crate::flow::ListRef { num_id, level: 1 }));

        // 5. Test undo steps
        apply_to(&mut doc, undo_enter3).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        assert_eq!(t.rows[0].cells[0].blocks.len(), 2);

        apply_to(&mut doc, undo_level).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        let Block::Paragraph(p2_restored) = &t.rows[0].cells[0].blocks[1] else { panic!() };
        assert_eq!(p2_restored.list, Some(crate::flow::ListRef { num_id, level: 0 }));

        apply_to(&mut doc, undo_list).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        for b in &t.rows[0].cells[0].blocks {
            let Block::Paragraph(p) = b else { panic!() };
            assert_eq!(p.list, None);
        }

        apply_to(&mut doc, undo_enter).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        assert_eq!(t.rows[0].cells[0].blocks.len(), 1);
        let Block::Paragraph(p1) = &t.rows[0].cells[0].blocks[0] else { panic!() };
        assert_eq!(p1.text(), "First");
    }

    #[test]
    fn test_table_sort_rows() {
        let mut doc = FlowDocument::default();
        let table_id = doc.new_id();
        let pt = |v: i64| v * 12700;

        let cell = |id: u32, text: &str| FlowCell {
            id,
            blocks: vec![Block::Paragraph(FlowParagraph {
                id: id + 100,
                runs: vec![FlowRun::new(FlowRunKind::Text(text.into()), FlowRunStyle::default())],
                ..FlowParagraph::default()
            })],
            span: 1,
            ..FlowCell::default()
        };

        // Table with header:
        // Header: [ "Item", "Count" ]
        // Row 1:  [ "Banana", "30" ]
        // Row 2:  [ "Apple",  "100" ]
        // Row 3:  [ "Orange", "2" ]
        let table = FlowTable {
            id: table_id,
            columns: vec![pt(100), pt(100)],
            rows: vec![
                FlowRow { id: 1, header: true, cells: vec![cell(10, "Item"), cell(11, "Count")], ..FlowRow::default() },
                FlowRow { id: 2, cells: vec![cell(20, "Banana"), cell(21, "30")], ..FlowRow::default() },
                FlowRow { id: 3, cells: vec![cell(30, "Apple"), cell(31, "100")], ..FlowRow::default() },
                FlowRow { id: 4, cells: vec![cell(40, "Orange"), cell(41, "2")], ..FlowRow::default() },
            ],
            ..FlowTable::default()
        };
        doc.body.push(Block::Table(table));

        // 1. Sort by Column 1 (Count: numerical) Ascending with header:
        // Expected order: Header -> Row 4 (2), Row 2 (30), Row 3 (100)
        let cmd_num_asc = FlowCommand::EditTable {
            table: table_id,
            edit: TableEdit::SortRows { column: 1, ascending: true, has_header: true },
        };
        let undo_num_asc = apply_to(&mut doc, cmd_num_asc).unwrap();
        let Block::Table(t) = &doc.body[0] else { panic!() };
        assert_eq!(t.rows.len(), 4);
        assert_eq!(t.rows[0].id, 1); // Header kept
        assert_eq!(t.rows[1].id, 4); // "Orange", "2"
        assert_eq!(t.rows[2].id, 2); // "Banana", "30"
        assert_eq!(t.rows[3].id, 3); // "Apple", "100"

        // Undo -> restores original order (1, 2, 3, 4)
        apply_to(&mut doc, undo_num_asc).unwrap();
        let Block::Table(t_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_undo.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 2, 3, 4]);

        // 2. Sort by Column 0 (Item: text) Ascending with header:
        // Expected order: Header -> Row 3 ("Apple"), Row 2 ("Banana"), Row 4 ("Orange")
        let cmd_text_asc = FlowCommand::EditTable {
            table: table_id,
            edit: TableEdit::SortRows { column: 0, ascending: true, has_header: true },
        };
        apply_to(&mut doc, cmd_text_asc).unwrap();
        let Block::Table(t_text) = &doc.body[0] else { panic!() };
        assert_eq!(t_text.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 3, 2, 4]);

        // 3. Sort by Column 0 Descending with header:
        // Expected order: Header -> Row 4 ("Orange"), Row 2 ("Banana"), Row 3 ("Apple")
        let cmd_text_desc = FlowCommand::EditTable {
            table: table_id,
            edit: TableEdit::SortRows { column: 0, ascending: false, has_header: true },
        };
        apply_to(&mut doc, cmd_text_desc).unwrap();
        let Block::Table(t_desc) = &doc.body[0] else { panic!() };
        assert_eq!(t_desc.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 4, 2, 3]);

        // 4. Sort with has_header = false (all rows sorted by column 0 ascending):
        // Rows: Item, Orange, Banana, Apple -> Apple (3), Banana (2), Item (1), Orange (4)
        let cmd_all_asc = FlowCommand::EditTable {
            table: table_id,
            edit: TableEdit::SortRows { column: 0, ascending: true, has_header: false },
        };
        apply_to(&mut doc, cmd_all_asc).unwrap();
        let Block::Table(t_all) = &doc.body[0] else { panic!() };
        assert_eq!(t_all.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![3, 2, 1, 4]);
    }

    #[test]
    fn test_table_edit_floating() {
        use crate::flow::{TableAlign, TableAnchor, TableFloat};

        let mut doc = FlowDocument { id_floor: 10, ..FlowDocument::default() };
        let table_id = 100;
        let pt = |v: i64| v * 12700;
        let cell = |id: u32, text: &str| FlowCell {
            id,
            blocks: vec![Block::Paragraph(FlowParagraph {
                id: id + 100,
                runs: vec![FlowRun::new(FlowRunKind::Text(text.into()), FlowRunStyle::default())],
                ..FlowParagraph::default()
            })],
            span: 1,
            ..FlowCell::default()
        };
        let table = FlowTable {
            id: table_id,
            columns: vec![pt(100), pt(100)],
            rows: vec![FlowRow { id: 1, cells: vec![cell(10, "A"), cell(11, "B")], ..FlowRow::default() }],
            floating: None,
            ..FlowTable::default()
        };
        doc.body.push(Block::Table(table));

        // 1. Initially inline (floating == None)
        let Block::Table(t0) = &doc.body[0] else { panic!() };
        assert_eq!(t0.floating, None);

        // 2. Set floating standard
        let float_std = TableFloat::standard();
        let cmd_float = FlowCommand::EditTable {
            table: table_id,
            edit: TableEdit::Floating(Some(float_std)),
        };
        let undo = apply_to(&mut doc, cmd_float).unwrap();
        let Block::Table(t1) = &doc.body[0] else { panic!() };
        assert_eq!(t1.floating, Some(float_std));
        assert_eq!(t1.floating.unwrap().horz_anchor, TableAnchor::Margin);
        assert_eq!(t1.floating.unwrap().x_align, Some(TableAlign::Start));
        assert_eq!(t1.floating.unwrap().distance, [180 * 635, 180 * 635, 0, 0]);

        // 3. Undo -> restores inline (floating == None)
        let redo = apply_to(&mut doc, undo).unwrap();
        let Block::Table(t_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_undo.floating, None);

        // 4. Redo -> restores floating
        apply_to(&mut doc, redo).unwrap();
        let Block::Table(t_redo) = &doc.body[0] else { panic!() };
        assert_eq!(t_redo.floating, Some(float_std));

        // 5. Change distance
        let new_dist = [200 * 635, 200 * 635, 100 * 635, 100 * 635];
        let cmd_dist = FlowCommand::EditTable {
            table: table_id,
            edit: TableEdit::FloatDistance(new_dist),
        };
        apply_to(&mut doc, cmd_dist).unwrap();
        let Block::Table(t2) = &doc.body[0] else { panic!() };
        assert_eq!(t2.floating.unwrap().distance, new_dist);

        // 6. Set back to None (inline)
        let cmd_none = FlowCommand::EditTable {
            table: table_id,
            edit: TableEdit::Floating(None),
        };
        apply_to(&mut doc, cmd_none).unwrap();
        let Block::Table(t3) = &doc.body[0] else { panic!() };
        assert_eq!(t3.floating, None);
    }

    #[test]
    fn test_table_edit_style() {
        let mut doc = FlowDocument { id_floor: 10, ..FlowDocument::default() };
        let table_id = 100;
        let pt = |v: i64| v * 12700;
        let cell = |id: u32, text: &str| FlowCell {
            id,
            blocks: vec![Block::Paragraph(FlowParagraph {
                id: id + 100,
                runs: vec![FlowRun::new(FlowRunKind::Text(text.into()), FlowRunStyle::default())],
                ..FlowParagraph::default()
            })],
            span: 1,
            ..FlowCell::default()
        };
        let table = FlowTable {
            id: table_id,
            columns: vec![pt(100), pt(100)],
            rows: vec![FlowRow { id: 1, cells: vec![cell(10, "A"), cell(11, "B")], ..FlowRow::default() }],
            style: None,
            ..FlowTable::default()
        };
        doc.body.push(Block::Table(table));

        // 1. Initially no style (style == None)
        let Block::Table(t0) = &doc.body[0] else { panic!() };
        assert_eq!(t0.style, None);

        // 2. Apply TableGrid style
        let cmd = FlowCommand::EditTable {
            table: table_id,
            edit: TableEdit::Style(Some("TableGrid".to_string())),
        };
        let undo = apply_to(&mut doc, cmd).unwrap();
        let Block::Table(t1) = &doc.body[0] else { panic!() };
        assert!(t1.style.is_some());
        assert_eq!(t1.style.as_ref().unwrap().id, "TableGrid");
        if let Block::Paragraph(p) = &t1.rows[0].cells[0].blocks[0] {
            assert_eq!(p.table_style.as_deref(), Some("TableGrid"));
        } else {
            panic!();
        }

        // 3. Undo -> restores style == None
        let redo = apply_to(&mut doc, undo).unwrap();
        let Block::Table(t_undo) = &doc.body[0] else { panic!() };
        assert_eq!(t_undo.style, None);
        if let Block::Paragraph(p) = &t_undo.rows[0].cells[0].blocks[0] {
            assert_eq!(p.table_style, None);
        } else {
            panic!();
        }

        // 4. Redo -> restores TableGrid
        apply_to(&mut doc, redo).unwrap();
        let Block::Table(t_redo) = &doc.body[0] else { panic!() };
        assert_eq!(t_redo.style.as_ref().unwrap().id, "TableGrid");

        // 5. Change to another style
        let cmd_change = FlowCommand::EditTable {
            table: table_id,
            edit: TableEdit::Style(Some("LightShading".to_string())),
        };
        apply_to(&mut doc, cmd_change).unwrap();
        let Block::Table(t2) = &doc.body[0] else { panic!() };
        assert_eq!(t2.style.as_ref().unwrap().id, "LightShading");
        if let Block::Paragraph(p) = &t2.rows[0].cells[0].blocks[0] {
            assert_eq!(p.table_style.as_deref(), Some("LightShading"));
        }

        // 6. Clear style (None)
        let cmd_clear = FlowCommand::EditTable {
            table: table_id,
            edit: TableEdit::Style(None),
        };
        apply_to(&mut doc, cmd_clear).unwrap();
        let Block::Table(t3) = &doc.body[0] else { panic!() };
        assert_eq!(t3.style, None);
        if let Block::Paragraph(p) = &t3.rows[0].cells[0].blocks[0] {
            assert_eq!(p.table_style, None);
        }
    }

    fn text_para(id: u32, text: &str) -> Block {
        Block::Paragraph(FlowParagraph { id, runs: vec![FlowRun::new(FlowRunKind::Text(text.into()), FlowRunStyle::default())], ..FlowParagraph::default() })
    }

    fn doc() -> FlowDocument {
        let mut d = FlowDocument { body: vec![text_para(1, "あいうえお"), text_para(2, "かきくけこ")], id_floor: 2, ..FlowDocument::default() };
        let styles = d.styles.clone();
        for b in &mut d.body {
            if let Block::Paragraph(p) = b {
                p.resolve(&styles);
            }
        }
        d
    }

    fn texts(d: &FlowDocument) -> Vec<String> {
        d.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect()
    }

    fn run(d: &mut FlowDocument, c: FlowCommand) {
        let before = d.clone();
        let inverse = apply_to(d, c).unwrap();
        let after = d.clone();
        let redo = apply_to(d, inverse).unwrap();
        assert_eq!(*d, before, "undo");
        apply_to(d, redo).unwrap();
        assert_eq!(*d, after, "redo");
    }

    #[test]
    fn insert_split_and_delete_join() {
        let mut d = doc();
        run(&mut d, FlowCommand::InsertText { at: FlowPos::new(1, 2), text: "X\nY".into(), format: None });
        assert_eq!(texts(&d), ["あいX", "Yうえお", "かきくけこ"]);
        let new = if let Block::Paragraph(p) = &d.body[1] { p.id } else { 0 };
        assert_eq!(new, 3);
        run(&mut d, FlowCommand::Delete { from: FlowPos::new(1, 3), to: FlowPos::new(2, 2) });
        assert_eq!(texts(&d), ["あいXくけこ"]);
    }

    #[test]
    fn delete_across_paragraphs() {
        let mut d = doc();
        run(&mut d, FlowCommand::Delete { from: FlowPos::new(1, 3), to: FlowPos::new(2, 2) });
        assert_eq!(texts(&d), ["あいうくけこ"]);
        run(&mut d, FlowCommand::Delete { from: FlowPos::new(1, 0), to: FlowPos::new(1, 6) });
        assert_eq!(texts(&d), [""]);
    }

    #[test]
    fn format_splits_runs_and_joins_again() {
        let mut d = doc();
        run(&mut d, FlowCommand::SetRunFormat { from: FlowPos::new(1, 1), to: FlowPos::new(1, 3), change: RunChange::Bold(true) });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert_eq!(p.runs.len(), 3);
        assert!(p.runs[1].style.bold && !p.runs[0].style.bold);
        run(&mut d, FlowCommand::SetRunFormat { from: FlowPos::new(1, 0), to: FlowPos::new(1, 5), change: RunChange::Bold(true) });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert_eq!(p.runs.len(), 1);
        assert_eq!(p.mark_format.bold, Some(true));
        // Typing after bold text is bold.
        run(&mut d, FlowCommand::InsertText { at: FlowPos::new(1, 5), text: "か".into(), format: None });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert_eq!(p.runs.len(), 1);
    }

    #[test]
    fn paragraph_format() {
        let mut d = doc();
        run(&mut d, FlowCommand::SetParagraphFormat { first: 2, last: 1, change: ParaChange::Align(FlowAlign::Center) });
        for b in &d.body {
            let Block::Paragraph(p) = b else { panic!() };
            assert_eq!(p.align, FlowAlign::Center);
        }
        let stop = crate::flow::TabStop { position: 72 * 12_700, align: crate::flow::TabAlign::Right, leader: true };
        run(&mut d, FlowCommand::SetParagraphFormat { first: 1, last: 1, change: ParaChange::Tabs(vec![crate::flow::TabChange { position: stop.position, stop: Some(stop) }]) });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert_eq!(p.tabs, vec![stop]);
        assert!(!p.contextual_spacing);
        run(&mut d, FlowCommand::SetParagraphFormat { first: 1, last: 1, change: ParaChange::ContextualSpacing(true) });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert!(p.contextual_spacing);
        assert!(p.widow_control);
        run(&mut d, FlowCommand::SetParagraphFormat { first: 1, last: 1, change: ParaChange::WidowControl(false) });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert!(!p.widow_control);
        assert!(!p.suppress_line_numbers);
        run(&mut d, FlowCommand::SetParagraphFormat { first: 1, last: 1, change: ParaChange::SuppressLineNumbers(true) });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert!(p.suppress_line_numbers);
    }

    #[test]
    fn bad_positions_are_errors() {
        let mut d = doc();
        let before = d.clone();
        assert!(apply_to(&mut d, FlowCommand::InsertText { at: FlowPos::new(1, 6), text: "x".into(), format: None }).is_err());
        assert!(apply_to(&mut d, FlowCommand::Delete { from: FlowPos::new(9, 0), to: FlowPos::new(1, 0) }).is_err());
        assert_eq!(d, before);
    }

    #[test]
    fn test_set_picture_wrap() {
        use crate::model::PictureWrapType;
        let mut d = doc();
        // Insert an inline picture
        run(&mut d, FlowCommand::InsertPicture { at: FlowPos::new(1, 0), media: crate::model::MediaId(42), size: crate::geometry::EmuSize::new(100, 200) });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert!(matches!(&p.runs[0].kind, FlowRunKind::Picture { .. }));

        // Convert to Square wrap
        run(&mut d, FlowCommand::SetPictureWrap { at: FlowPos::new(1, 0), wrap: PictureWrapType::Square });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Shape(shape) = &p.runs[0].kind else { panic!("expected shape") };
        assert_eq!(shape.floating.as_ref().unwrap().wrap, "square");
        assert!(!shape.floating.as_ref().unwrap().behind_doc);
        assert_eq!(shape.frame.width, 100);
        assert_eq!(shape.frame.height, 200);

        // Convert to BehindText
        run(&mut d, FlowCommand::SetPictureWrap { at: FlowPos::new(1, 0), wrap: PictureWrapType::BehindText });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Shape(shape) = &p.runs[0].kind else { panic!("expected shape") };
        assert_eq!(shape.floating.as_ref().unwrap().wrap, "none");
        assert!(shape.floating.as_ref().unwrap().behind_doc);

        // Convert to TopAndBottom
        run(&mut d, FlowCommand::SetPictureWrap { at: FlowPos::new(1, 0), wrap: PictureWrapType::TopAndBottom });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Shape(shape) = &p.runs[0].kind else { panic!("expected shape") };
        assert_eq!(shape.floating.as_ref().unwrap().wrap, "topAndBottom");
        assert!(!shape.floating.as_ref().unwrap().behind_doc);

        // Modify size while floating
        run(&mut d, FlowCommand::SetPictureSize { at: FlowPos::new(1, 0), size: crate::geometry::EmuSize::new(300, 400) });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Shape(shape) = &p.runs[0].kind else { panic!("expected shape") };
        assert_eq!(shape.frame.width, 300);
        assert_eq!(shape.frame.height, 400);

        // Convert back to Inline
        run(&mut d, FlowCommand::SetPictureWrap { at: FlowPos::new(1, 0), wrap: PictureWrapType::Inline });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Picture { media, size, .. } = &p.runs[0].kind else { panic!("expected picture") };
        assert_eq!(*media, crate::model::MediaId(42));
        assert_eq!(size.width, 300);
        assert_eq!(size.height, 400);
    }

    #[test]
    fn test_set_picture_wrap_polygon() {
        use crate::model::{WrapPoint, WrapPolygon};
        let mut d = doc();
        // Insert an inline picture
        run(&mut d, FlowCommand::InsertPicture { at: FlowPos::new(1, 0), media: crate::model::MediaId(99), size: crate::geometry::EmuSize::new(100, 200) });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert!(matches!(&p.runs[0].kind, FlowRunKind::Picture { .. }));

        // Apply custom wrap polygon to inline picture (converts to floating tight and verifies undo/redo automatically)
        let poly = WrapPolygon::new(
            WrapPoint::new(10, 10),
            vec![
                WrapPoint::new(10, 190),
                WrapPoint::new(90, 190),
                WrapPoint::new(90, 10),
                WrapPoint::new(10, 10),
            ],
            true,
        );
        run(&mut d, FlowCommand::SetPictureWrapPolygon { at: FlowPos::new(1, 0), polygon: Some(poly.clone()) });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Shape(shape) = &p.runs[0].kind else { panic!("expected shape") };
        let anchor = shape.floating.as_ref().unwrap();
        assert_eq!(anchor.wrap, "tight");
        assert_eq!(anchor.wrap_polygon.as_ref().unwrap(), &poly);

        // Modify wrap polygon with another point
        let mut poly2 = poly.clone();
        poly2.set_point(0, WrapPoint::new(20, 20));
        run(&mut d, FlowCommand::SetPictureWrapPolygon { at: FlowPos::new(1, 0), polygon: Some(poly2.clone()) });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Shape(shape) = &p.runs[0].kind else { panic!("expected shape") };
        let anchor = shape.floating.as_ref().unwrap();
        assert_eq!(anchor.wrap_polygon.as_ref().unwrap(), &poly2);
    }

    #[test]
    fn test_compress_picture_and_quality_settings_commands() {
        let mut d = doc();
        // Insert an inline picture with crop
        run(&mut d, FlowCommand::InsertPicture { at: FlowPos::new(1, 0), media: crate::model::MediaId(10), size: crate::geometry::EmuSize::new(100, 200) });
        let mut look = crate::flow::PictureLook::default();
        look.crop = crate::model::Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 };
        run(&mut d, FlowCommand::SetPictureLook { at: FlowPos::new(1, 0), look: look.clone() });

        // Verify initial state
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Picture { media, look: l, .. } = &p.runs[0].kind else { panic!() };
        assert_eq!(*media, crate::model::MediaId(10));
        assert_eq!(l.crop.left, 10_000);

        // Compress picture and reset crop (run checks undo and redo automatically)
        run(&mut d, FlowCommand::CompressPicture { at: FlowPos::new(1, 0), media: crate::model::MediaId(20), reset_crop: true });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Picture { media, look: l, .. } = &p.runs[0].kind else { panic!() };
        assert_eq!(*media, crate::model::MediaId(20));
        assert_eq!(l.crop, crate::model::Crop::default());

        // Test SetImageQualitySettings (run checks undo and redo automatically)
        assert!(!d.do_not_compress_images);
        assert_eq!(d.default_image_dpi, None);
        assert!(!d.discard_image_editing_data);

        run(&mut d, FlowCommand::SetImageQualitySettings {
            default_dpi: Some(220),
            discard_crop: true,
            do_not_compress: true,
        });
        assert!(d.do_not_compress_images);
        assert_eq!(d.default_image_dpi, Some(220));
        assert!(d.discard_image_editing_data);

        // Test SetSaveFrequency (run checks undo and redo automatically)
        assert_eq!(d.save_frequency, None);
        run(&mut d, FlowCommand::SetSaveFrequency(Some(15)));
        assert_eq!(d.save_frequency, Some(15));
        run(&mut d, FlowCommand::SetSaveFrequency(None));
        assert_eq!(d.save_frequency, None);
    }

    #[test]
    fn test_picture_control_commands_and_locking() {
        use crate::flow::{ControlInfo, ControlLock};
        let mut d = doc();
        // Insert picture control with placeholder (run tests undo and redo automatically)
        run(&mut d, FlowCommand::InsertPictureControl {
            at: FlowPos::new(1, 0),
            media: crate::model::MediaId(1),
            size: crate::geometry::EmuSize::new(100, 100),
            placeholder: true,
        });

        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert!(p.runs[0].is_content_control());
        assert!(p.runs[0].is_picture());
        let FlowRunKind::PictureControl { media, size, placeholder, .. } = &p.runs[0].kind else { panic!() };
        assert_eq!(*media, crate::model::MediaId(1));
        assert_eq!(*size, crate::geometry::EmuSize::new(100, 100));
        assert!(*placeholder);

        // Replace picture: sets new media, size, and clears placeholder (undo/redo verified)
        run(&mut d, FlowCommand::ReplacePicture {
            at: FlowPos::new(1, 0),
            media: crate::model::MediaId(2),
            size: Some(crate::geometry::EmuSize::new(200, 150)),
        });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::PictureControl { media, size, placeholder, .. } = &p.runs[0].kind else { panic!() };
        assert_eq!(*media, crate::model::MediaId(2));
        assert_eq!(*size, crate::geometry::EmuSize::new(200, 150));
        assert!(!*placeholder);

        // Reset picture control: restores placeholder media, size, and sets placeholder = true
        run(&mut d, FlowCommand::ResetPictureControl {
            at: FlowPos::new(1, 0),
            placeholder_media: crate::model::MediaId(1),
            size: crate::geometry::EmuSize::new(100, 100),
        });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::PictureControl { media, size, placeholder, .. } = &p.runs[0].kind else { panic!() };
        assert_eq!(*media, crate::model::MediaId(1));
        assert_eq!(*size, crate::geometry::EmuSize::new(100, 100));
        assert!(*placeholder);

        // Set control info with Content lock
        run(&mut d, FlowCommand::SetControlInfo {
            at: FlowPos::new(1, 0),
            info: Some(Box::new(ControlInfo {
                title: "Photo".into(),
                tag: "photo_tag".into(),
                lock: ControlLock::Content,
            })),
        });

        // Content lock must block replace picture
        assert_eq!(
            apply_to(&mut d, FlowCommand::ReplacePicture {
                at: FlowPos::new(1, 0),
                media: crate::model::MediaId(3),
                size: None,
            }),
            Err(EditError::ControlLocked)
        );
        // Content lock must block reset picture control
        assert_eq!(
            apply_to(&mut d, FlowCommand::ResetPictureControl {
                at: FlowPos::new(1, 0),
                placeholder_media: crate::model::MediaId(1),
                size: crate::geometry::EmuSize::new(100, 100),
            }),
            Err(EditError::ControlLocked)
        );

        // Set control lock to Control (cannot delete, but can edit content)
        run(&mut d, FlowCommand::SetControlInfo {
            at: FlowPos::new(1, 0),
            info: Some(Box::new(ControlInfo {
                title: "Photo".into(),
                tag: "photo_tag".into(),
                lock: ControlLock::Control,
            })),
        });
        // Deleting the picture control must be blocked
        assert_eq!(
            apply_to(&mut d, FlowCommand::Delete {
                from: FlowPos::new(1, 0),
                to: FlowPos::new(1, 1),
            }),
            Err(EditError::ControlLocked)
        );

        // Unlock control
        run(&mut d, FlowCommand::SetControlInfo {
            at: FlowPos::new(1, 0),
            info: None,
        });
        // Now replace succeeds
        run(&mut d, FlowCommand::ReplacePicture {
            at: FlowPos::new(1, 0),
            media: crate::model::MediaId(3),
            size: None,
        });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert_eq!(p.runs[0].picture().unwrap().0, crate::model::MediaId(3));
    }

    #[test]
    fn test_building_block_gallery_commands_and_locking() {
        use crate::flow::{ControlInfo, ControlLock, DEFAULT_BUILDING_BLOCK_GALLERY_PROMPT};
        let mut d = doc();
        // Insert building block gallery control with placeholder
        run(&mut d, FlowCommand::InsertField {
            at: FlowPos::new(1, 0),
            kind: FlowRunKind::BuildingBlockGallery {
                gallery: "Custom".into(),
                category: "General".into(),
                text: DEFAULT_BUILDING_BLOCK_GALLERY_PROMPT.into(),
                placeholder: true,
            },
        });

        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert!(p.runs[0].is_content_control());
        assert!(p.runs[0].is_building_block_gallery());
        let (gallery, category, text, placeholder) = p.runs[0].building_block_gallery().unwrap();
        assert_eq!(gallery, "Custom");
        assert_eq!(category, "General");
        assert_eq!(text, DEFAULT_BUILDING_BLOCK_GALLERY_PROMPT);
        assert!(placeholder);
        assert_eq!(p.runs[0].format.color, Some(crate::drawing::Rgb(0x808080)));

        // Update gallery and category meta
        run(&mut d, FlowCommand::SetBuildingBlockGalleryMeta {
            at: FlowPos::new(1, 0),
            gallery: "Quick Parts".into(),
            category: "Built-In".into(),
        });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let (gallery, category, _, _) = p.runs[0].building_block_gallery().unwrap();
        assert_eq!(gallery, "Quick Parts");
        assert_eq!(category, "Built-In");

        // Typing text replaces placeholder prompt and clears placeholder
        run(&mut d, FlowCommand::InsertText {
            at: FlowPos::new(1, 0),
            text: "MyPart".into(),
            format: None,
        });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let (_, _, text, placeholder) = p.runs[0].building_block_gallery().unwrap();
        assert_eq!(text, "MyPart");
        assert!(!placeholder);
        assert_eq!(p.runs[0].format.color, None);

        // Deleting all text restores the placeholder prompt
        run(&mut d, FlowCommand::Delete {
            from: FlowPos::new(1, 0),
            to: FlowPos::new(1, 6),
        });
        let text_len = {
            let Block::Paragraph(p) = &d.body[0] else { panic!() };
            let (_, _, text, placeholder) = p.runs[0].building_block_gallery().unwrap();
            assert_eq!(text, DEFAULT_BUILDING_BLOCK_GALLERY_PROMPT);
            assert!(placeholder);
            assert_eq!(p.runs[0].format.color, Some(crate::drawing::Rgb(0x808080)));
            text.chars().count()
        };

        // Set content lock
        run(&mut d, FlowCommand::SetControlInfo {
            at: FlowPos::new(1, 0),
            info: Some(Box::new(ControlInfo {
                title: "Gallery1".into(),
                tag: "tag1".into(),
                lock: ControlLock::Content,
            })),
        });

        // Content lock must block InsertText
        assert_eq!(
            apply_to(&mut d, FlowCommand::InsertText {
                at: FlowPos::new(1, 0),
                text: "Blocked".into(),
                format: None,
            }),
            Err(EditError::ControlLocked)
        );

        // Set control lock
        run(&mut d, FlowCommand::SetControlInfo {
            at: FlowPos::new(1, 0),
            info: Some(Box::new(ControlInfo {
                title: "Gallery1".into(),
                tag: "tag1".into(),
                lock: ControlLock::Control,
            })),
        });

        // Control lock must block deleting the control
        assert_eq!(
            apply_to(&mut d, FlowCommand::Delete {
                from: FlowPos::new(1, 0),
                to: FlowPos::new(1, text_len),
            }),
            Err(EditError::ControlLocked)
        );

        // Unlock and verify SetContentControlText
        run(&mut d, FlowCommand::SetControlInfo {
            at: FlowPos::new(1, 0),
            info: None,
        });
        run(&mut d, FlowCommand::SetContentControlText {
            at: FlowPos::new(1, 0),
            text: "SelectedPart".into(),
            placeholder: false,
        });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let (_, _, text, placeholder) = p.runs[0].building_block_gallery().unwrap();
        assert_eq!(text, "SelectedPart");
        assert!(!placeholder);
    }

    #[test]
    fn test_repeating_section_commands_and_locking() {
        use crate::flow::{ControlInfo, ControlLock, DEFAULT_REPEATING_SECTION_TITLE};
        let mut d = doc();
        // Insert repeating section control
        run(&mut d, FlowCommand::InsertField {
            at: FlowPos::new(1, 0),
            kind: FlowRunKind::RepeatingSection {
                section_title: DEFAULT_REPEATING_SECTION_TITLE.into(),
                items: vec!["Item 1".into()],
                selected: 0,
                do_not_allow_insert_delete: false,
            },
        });

        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert!(p.runs[0].is_content_control());
        assert!(p.runs[0].is_repeating_section());
        let (title, items, selected, no_edit) = p.runs[0].repeating_section().unwrap();
        assert_eq!(title, DEFAULT_REPEATING_SECTION_TITLE);
        assert_eq!(items, &["Item 1".to_string()]);
        assert_eq!(selected, 0);
        assert!(!no_edit);

        // Add a new item
        run(&mut d, FlowCommand::InsertRepeatingSectionItem {
            at: FlowPos::new(1, 0),
            index: 1,
            text: "Item 2".into(),
            restore_selected: None,
        });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let (_, items, selected, _) = p.runs[0].repeating_section().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1], "Item 2");
        assert_eq!(selected, 1);

        // Switch active item
        run(&mut d, FlowCommand::SelectRepeatingSectionItem {
            at: FlowPos::new(1, 0),
            index: 0,
        });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert_eq!(p.runs[0].repeating_section().unwrap().2, 0);

        // Set metadata
        run(&mut d, FlowCommand::SetRepeatingSectionMeta {
            at: FlowPos::new(1, 0),
            section_title: "InvoiceItem".into(),
            do_not_allow_insert_delete: true,
        });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let (title, _, _, no_edit) = p.runs[0].repeating_section().unwrap();
        assert_eq!(title, "InvoiceItem");
        assert!(no_edit);

        // do_not_allow_insert_delete blocks adding
        assert_eq!(
            apply_to(&mut d, FlowCommand::InsertRepeatingSectionItem {
                at: FlowPos::new(1, 0),
                index: 2,
                text: "Item 3".into(),
                restore_selected: None,
            }),
            Err(EditError::ControlLocked)
        );

        // Unlock metadata
        run(&mut d, FlowCommand::SetRepeatingSectionMeta {
            at: FlowPos::new(1, 0),
            section_title: "InvoiceItem".into(),
            do_not_allow_insert_delete: false,
        });

        // Set control info with Content lock
        run(&mut d, FlowCommand::SetControlInfo {
            at: FlowPos::new(1, 0),
            info: Some(Box::new(ControlInfo {
                title: "RepSec".into(),
                tag: "tag_rep".into(),
                lock: ControlLock::Content,
            })),
        });

        // Content lock must block SetContentControlText
        assert_eq!(
            apply_to(&mut d, FlowCommand::SetContentControlText {
                at: FlowPos::new(1, 0),
                text: "Blocked".into(),
                placeholder: false,
            }),
            Err(EditError::ControlLocked)
        );

        // Delete item
        run(&mut d, FlowCommand::SetControlInfo {
            at: FlowPos::new(1, 0),
            info: None,
        });
        run(&mut d, FlowCommand::DeleteRepeatingSectionItem {
            at: FlowPos::new(1, 0),
            index: 1,
            restore_selected: None,
        });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let (_, items, selected, _) = p.runs[0].repeating_section().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(selected, 0);
    }

    #[test]
    fn test_group_control_commands_and_locking() {
        use crate::flow::{ControlInfo, ControlLock, DEFAULT_GROUP_PROMPT};
        let mut d = doc();
        // 1. Insert group control with placeholder
        run(&mut d, FlowCommand::InsertField {
            at: FlowPos::new(1, 0),
            kind: FlowRunKind::Group {
                text: DEFAULT_GROUP_PROMPT.into(),
                placeholder: true,
            },
        });

        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        assert!(p.runs[0].is_content_control());
        assert!(p.runs[0].is_group());
        let (text, placeholder) = p.runs[0].group().unwrap();
        assert_eq!(text, DEFAULT_GROUP_PROMPT);
        assert!(placeholder);
        assert_eq!(p.runs[0].format.color, Some(crate::drawing::Rgb(0x808080)));

        // 2. Typing replaces placeholder prompt
        run(&mut d, FlowCommand::InsertText {
            at: FlowPos::new(1, 0),
            text: "GroupedSection".into(),
            format: None,
        });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let (text, placeholder) = p.runs[0].group().unwrap();
        assert_eq!(text, "GroupedSection");
        assert!(!placeholder);

        // 3. Deleting all characters restores prompt
        let count = "GroupedSection".chars().count();
        run(&mut d, FlowCommand::Delete {
            from: FlowPos::new(1, 0),
            to: FlowPos::new(1, count),
        });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let (text, placeholder) = p.runs[0].group().unwrap();
        assert_eq!(text, DEFAULT_GROUP_PROMPT);
        assert!(placeholder);
        assert_eq!(p.runs[0].format.color, Some(crate::drawing::Rgb(0x808080)));

        // 4. Content lock protects typing & SetContentControlText
        run(&mut d, FlowCommand::SetControlInfo {
            at: FlowPos::new(1, 0),
            info: Some(Box::new(ControlInfo {
                title: "Grp1".into(),
                tag: "GRP_TAG".into(),
                lock: ControlLock::Content,
            })),
        });
        assert_eq!(
            apply_to(&mut d, FlowCommand::InsertText {
                at: FlowPos::new(1, 0),
                text: "Blocked".into(),
                format: None,
            }),
            Err(EditError::ControlLocked)
        );
        assert_eq!(
            apply_to(&mut d, FlowCommand::SetContentControlText {
                at: FlowPos::new(1, 0),
                text: "Blocked".into(),
                placeholder: false,
            }),
            Err(EditError::ControlLocked)
        );

        // 5. Control lock protects Ungroup and Delete
        run(&mut d, FlowCommand::SetControlInfo {
            at: FlowPos::new(1, 0),
            info: Some(Box::new(ControlInfo {
                title: "Grp1".into(),
                tag: "GRP_TAG".into(),
                lock: ControlLock::Control,
            })),
        });
        assert_eq!(
            apply_to(&mut d, FlowCommand::UngroupControl {
                at: FlowPos::new(1, 0),
                restore: None,
            }),
            Err(EditError::ControlLocked)
        );
        let prompt_len = DEFAULT_GROUP_PROMPT.chars().count();
        assert_eq!(
            apply_to(&mut d, FlowCommand::Delete {
                from: FlowPos::new(1, 0),
                to: FlowPos::new(1, prompt_len),
            }),
            Err(EditError::ControlLocked)
        );

        // 6. Unlock and Ungroup (run internally tests both undo and redo)
        run(&mut d, FlowCommand::SetControlInfo {
            at: FlowPos::new(1, 0),
            info: None,
        });
        run(&mut d, FlowCommand::UngroupControl {
            at: FlowPos::new(1, 0),
            restore: None,
        });
        {
            let Block::Paragraph(p) = &d.body[0] else { panic!() };
            assert!(!p.runs[0].is_content_control());
            assert!(matches!(&p.runs[0].kind, FlowRunKind::Text(_)));
        }

        // 7. GroupControl on text and Undo/Redo symmetry (run internally tests both undo and redo)
        run(&mut d, FlowCommand::GroupControl {
            at: FlowPos::new(1, 0),
            text: "ImportantSection".into(),
            placeholder: false,
            control: Some(Box::new(ControlInfo {
                title: "ProtectedGroup".into(),
                tag: "GRP_PROT".into(),
                lock: ControlLock::Content,
            })),
        });
        {
            let Block::Paragraph(p) = &d.body[0] else { panic!() };
            assert!(p.runs[0].is_group());
            let (t, plc) = p.runs[0].group().unwrap();
            assert_eq!(t, "ImportantSection");
            assert!(!plc);
            assert_eq!(p.runs[0].control.as_deref().unwrap().title, "ProtectedGroup");
        }
    }

    #[test]
    fn test_set_save_frequency_command_undo_redo() {
        let mut d = doc();
        assert_eq!(d.save_frequency, None);
        run(&mut d, FlowCommand::SetSaveFrequency(Some(15)));
        assert_eq!(d.save_frequency, Some(15));
        run(&mut d, FlowCommand::SetSaveFrequency(Some(0)));
        assert_eq!(d.save_frequency, Some(0));
        run(&mut d, FlowCommand::SetSaveFrequency(None));
        assert_eq!(d.save_frequency, None);
    }

    #[test]
    fn test_picture_look_extended_effects_undo_redo() {
        let mut d = doc();
        run(&mut d, FlowCommand::InsertPicture { at: FlowPos::new(1, 0), media: crate::model::MediaId(1), size: crate::geometry::EmuSize::new(100, 200) });
        let mut look = crate::flow::PictureLook::default();
        look.effects.opacity = Some(50_000);
        look.effects.saturation = Some(200_000);
        look.effects.bilevel = Some(60_000);
        look.soft_edge = Some(25_400);
        run(&mut d, FlowCommand::SetPictureLook { at: FlowPos::new(1, 0), look: look.clone() });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Picture { look: l, .. } = &p.runs[0].kind else { panic!() };
        assert_eq!(l.effects.opacity, Some(50_000));
        assert_eq!(l.effects.saturation, Some(200_000));
        assert_eq!(l.effects.bilevel, Some(60_000));
        assert_eq!(l.soft_edge, Some(25_400));
    }

    #[test]
    fn test_picture_look_crop_to_shape_geometry_undo_redo() {
        let mut d = doc();
        run(&mut d, FlowCommand::InsertPicture { at: FlowPos::new(1, 0), media: crate::model::MediaId(1), size: crate::geometry::EmuSize::new(100, 200) });
        let mut look = crate::flow::PictureLook::default();
        look.geometry = Some(crate::drawing::Geometry::Ellipse);
        run(&mut d, FlowCommand::SetPictureLook { at: FlowPos::new(1, 0), look: look.clone() });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Picture { look: l, .. } = &p.runs[0].kind else { panic!() };
        assert_eq!(l.geometry, Some(crate::drawing::Geometry::Ellipse));

        // Test conversion to floating shape and back preserves geometry
        run(&mut d, FlowCommand::SetPictureFloating { at: FlowPos::new(1, 0), anchor: Some(Default::default()) });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Shape(shape) = &p.runs[0].kind else { panic!() };
        let crate::model::ShapeKind::Picture(pic) = &shape.kind else { panic!() };
        assert_eq!(pic.geometry, Some(crate::drawing::Geometry::Ellipse));

        // Convert back to inline picture
        run(&mut d, FlowCommand::SetPictureFloating { at: FlowPos::new(1, 0), anchor: None });
        let Block::Paragraph(p) = &d.body[0] else { panic!() };
        let FlowRunKind::Picture { look: l, .. } = &p.runs[0].kind else { panic!() };
        assert_eq!(l.geometry, Some(crate::drawing::Geometry::Ellipse));
    }

    #[test]
    fn test_font_embedding_and_compatibility_commands_undo_redo() {
        let mut d = doc();
        // Font embedding
        assert!(!d.embed_true_type_fonts);
        assert!(!d.embed_system_fonts);
        assert!(!d.save_subset_fonts);
        run(&mut d, FlowCommand::SetFontEmbedding {
            embed_true_type_fonts: true,
            embed_system_fonts: true,
            save_subset_fonts: true,
        });
        assert!(d.embed_true_type_fonts);
        assert!(d.embed_system_fonts);
        assert!(d.save_subset_fonts);
        run(&mut d, FlowCommand::SetFontEmbedding {
            embed_true_type_fonts: false,
            embed_system_fonts: false,
            save_subset_fonts: false,
        });
        assert!(!d.embed_true_type_fonts);

        // Compatibility options
        assert_eq!(d.compatibility_mode, None);
        assert!(!d.use_fe_layout);
        assert!(!d.do_not_expand_shift_return);
        run(&mut d, FlowCommand::SetCompatibilityOptions {
            compatibility_mode: Some(15),
            use_fe_layout: true,
            do_not_expand_shift_return: true,
        });
        assert_eq!(d.compatibility_mode, Some(15));
        assert!(d.use_fe_layout);
        assert!(d.do_not_expand_shift_return);
        run(&mut d, FlowCommand::SetCompatibilityOptions {
            compatibility_mode: Some(14),
            use_fe_layout: false,
            do_not_expand_shift_return: false,
        });
        assert_eq!(d.compatibility_mode, Some(14));
        assert!(!d.use_fe_layout);
        assert!(!d.do_not_expand_shift_return);
    }

    #[test]
    fn test_attach_template_and_update_styles_commands_undo_redo() {
        let mut d = doc();
        assert_eq!(d.attached_template, None);
        assert!(!d.link_styles);

        // Attach template with link_styles
        run(&mut d, FlowCommand::AttachTemplate {
            template: Some("MyTemplate.dotx".to_string()),
            link_styles: true,
        });
        assert_eq!(d.attached_template.as_deref(), Some("MyTemplate.dotx"));
        assert!(d.link_styles);

        // Detach template and link_styles
        run(&mut d, FlowCommand::AttachTemplate {
            template: None,
            link_styles: false,
        });
        assert_eq!(d.attached_template, None);
        assert!(!d.link_styles);

        // UpdateStyles
        let old_styles = d.styles.clone();
        let mut new_styles = (*old_styles).clone();
        new_styles.styles.insert("CustomStyle".to_string(), crate::flow::FlowStyle {
            name: "Custom Style".to_string(),
            ..crate::flow::FlowStyle::default()
        });
        let new_styles_arc = std::sync::Arc::new(new_styles);
        run(&mut d, FlowCommand::UpdateStyles(new_styles_arc.clone()));
        assert!(d.styles.styles.contains_key("CustomStyle"));
    }

    #[test]
    fn test_strict_first_and_last_chars_command_undo_redo() {
        let mut d = doc();
        d.strict_first_and_last_chars = true;

        // Change strict to false (run tests execute, undo, and redo automatically)
        run(&mut d, FlowCommand::SetStrictFirstAndLastChars(false));
        assert!(!d.strict_first_and_last_chars);

        // Change strict back to true
        run(&mut d, FlowCommand::SetStrictFirstAndLastChars(true));
        assert!(d.strict_first_and_last_chars);
    }

    #[test]
    fn test_theme_font_lang_command_undo_redo() {
        let mut d = doc();
        assert!(d.theme_font_lang.is_none());

        let tfl = crate::flow::ThemeFontLang {
            val: Some("en-US".into()),
            east_asia: Some("ja-JP".into()),
            bidi: Some("ar-SA".into()),
        };

        // Set theme font lang
        run(&mut d, FlowCommand::SetThemeFontLang(Some(tfl.clone())));
        assert_eq!(d.theme_font_lang, Some(tfl));

        // Clear theme font lang
        run(&mut d, FlowCommand::SetThemeFontLang(None));
        assert!(d.theme_font_lang.is_none());
    }

    #[test]
    fn test_collapsed_heading_command_undo_redo() {
        let mut d = doc();
        let p_id = match &d.body[0] {
            Block::Paragraph(p) => p.id,
            _ => panic!("expected paragraph"),
        };
        assert!(!paragraph(&d, p_id).unwrap().collapsed);

        // Collapse paragraph
        run(&mut d, FlowCommand::SetParagraphFormat { first: p_id, last: p_id, change: ParaChange::Collapsed(true) });
        assert!(paragraph(&d, p_id).unwrap().collapsed);

        // Expand paragraph
        run(&mut d, FlowCommand::SetParagraphFormat { first: p_id, last: p_id, change: ParaChange::Collapsed(false) });
        assert!(!paragraph(&d, p_id).unwrap().collapsed);
    }
}

/// The heading level (1 to 9) of a paragraph: its style's (a style named
/// `heading N`, or based on one).
pub fn heading_level(styles: &FlowStyles, p: &FlowParagraph) -> Option<u32> {
    let id = p.style.as_deref()?;
    styles.chain(id).iter().rev().find_map(|s| {
        let name = s.name.to_ascii_lowercase();
        name.strip_prefix("heading ").and_then(|n| n.trim().parse::<u32>().ok()).filter(|n| (1..=9).contains(n))
    })
}

/// 見出しに番号を付ける (Word's アウトライン番号 1 / 1.1 / 1.1.1): a list of 9
/// levels (`%1`, `%1.%2`, …, a space after the number) and the heading
/// styles (`heading 1` …) numbered with it, level by level. `None` when the
/// document has no heading styles.
pub fn heading_numbering(doc: &FlowDocument) -> Option<Vec<FlowCommand>> {
    use crate::flow::{AbstractList, ListInstance, ListLevel, ListSuffix};
    let styles = &doc.styles;
    let headings: Vec<(u32, String)> = (1..=9u32).filter_map(|n| styles.paragraph_style_named(&format!("heading {n}")).map(|id| (n, id.to_string()))).collect();
    if headings.is_empty() {
        return None;
    }
    let mut numbering = styles.numbering.clone();
    let abstract_id = numbering.abstracts.keys().max().map_or(0, |m| m + 1);
    let num_id = numbering.nums.keys().max().map_or(1, |m| m + 1);
    let levels = (1..=9u32)
        .map(|n| ListLevel {
            start: 1,
            format: "decimal".into(),
            text: (1..=n).map(|k| format!("%{k}")).collect::<Vec<_>>().join("."),
            suffix: ListSuffix::Space,
            align: FlowAlign::Left,
            para: crate::flow::ParaFormat::default(),
            run: RunFormat::default(),
            pic_bullet: None,
        })
        .collect();
    numbering.abstracts.insert(abstract_id, AbstractList { levels, num_style_link: None });
    numbering.nums.insert(num_id, ListInstance { abstract_id, start_overrides: Default::default() });
    let mut commands = vec![FlowCommand::SetNumbering { numbering }];
    for (n, id) in headings {
        let mut style = styles.styles.get(&id)?.clone();
        style.para.num_id = Some(num_id);
        style.para.level = Some(n - 1);
        commands.push(FlowCommand::SetStyle { id, style: Some(style) });
    }
    Some(commands)
}

/// A caption's number in its format (`\* ARABIC`, `\* roman`, …).
fn seq_number(n: u32, instruction: &str) -> String {
    let format = instruction.split("\\*").nth(1).and_then(|f| f.split_whitespace().next()).unwrap_or("ARABIC");
    let name = match format {
        "ROMAN" => "upperRoman",
        "roman" => "lowerRoman",
        "ALPHABETIC" => "upperLetter",
        "alphabetic" => "lowerLetter",
        _ => "decimal",
    };
    crate::flow::format_list_number(n, name)
}

/// The words of a field's code, the first being its kind (`SEQ`).
pub fn field_words(instruction: &str) -> Vec<&str> {
    instruction.split_whitespace().collect()
}

/// Parse a LISTNUM field instruction:
/// `LISTNUM [Listname] [\l Level] [\s StartAt] [\* ...]`
/// Returns `(list_name, level, start_at)`.
pub fn parse_listnum(instruction: &str) -> (String, u32, Option<u32>) {
    let words = field_words(instruction);
    let mut list_name = None;
    let mut level = 1u32;
    let mut start_at = None;
    let mut i = 1;
    while i < words.len() {
        match words[i] {
            "\\l" => {
                if let Some(lvl) = words.get(i + 1).and_then(|v| v.parse::<u32>().ok()) {
                    level = lvl.clamp(1, 9);
                    i += 1;
                }
            }
            "\\s" => {
                if let Some(s) = words.get(i + 1).and_then(|v| v.parse::<u32>().ok()) {
                    start_at = Some(s);
                    i += 1;
                }
            }
            "\\*" | "\\#" => {
                i += 1;
            }
            w if !w.starts_with('\\') && list_name.is_none() => {
                list_name = Some(w.to_string());
            }
            _ => {}
        }
        i += 1;
    }
    let name = list_name.unwrap_or_else(|| "Numbering_Default".to_string());
    (name, level, start_at)
}

/// What a `REF` to the bookmark of paragraph `p` shows: its text up to and
/// including its caption's number (Word's 番号とラベルのみ), or all of it.
fn reference_text(p: &FlowParagraph, numbers: &std::collections::HashMap<(u32, usize), String>) -> String {
    let mut out = String::new();
    for (i, run) in p.runs.iter().enumerate() {
        match &run.kind {
            FlowRunKind::Text(t) => out.push_str(t),
            FlowRunKind::Tab | FlowRunKind::PositionalTab { .. } => out.push('\t'),
            FlowRunKind::Field { instruction, shown } if field_words(instruction).first() == Some(&"SEQ") => {
                out.push_str(numbers.get(&(p.id, i)).unwrap_or(shown));
                return out;
            }
            FlowRunKind::Field { shown, .. } | FlowRunKind::PageRef { shown, .. } => out.push_str(shown),
            _ => {}
        }
    }
    out
}

/// Extract the caption description text after the SEQ field (Word の 説明文のみ).
pub fn caption_description_text(p: &FlowParagraph) -> String {
    let mut found_seq = false;
    let mut out = String::new();
    for run in &p.runs {
        if found_seq {
            match &run.kind {
                FlowRunKind::Text(t) => out.push_str(t),
                FlowRunKind::Tab | FlowRunKind::PositionalTab { .. } => out.push('\t'),
                FlowRunKind::Field { shown, .. } | FlowRunKind::PageRef { shown, .. } => out.push_str(shown),
                _ => {}
            }
        } else if matches!(&run.kind, FlowRunKind::Field { instruction, .. } if field_words(instruction).first() == Some(&"SEQ")) {
            found_seq = true;
        }
    }
    let trimmed = out.trim();
    trimmed.strip_prefix(':').or_else(|| trimmed.strip_prefix('：')).or_else(|| trimmed.strip_prefix('-')).unwrap_or(trimmed).trim().to_string()
}

/// The entire text of a caption paragraph including SEQ field and description text.
pub fn entire_caption_text(p: &FlowParagraph) -> String {
    let mut out = String::new();
    for run in &p.runs {
        match &run.kind {
            FlowRunKind::Text(t) => out.push_str(t),
            FlowRunKind::Tab | FlowRunKind::PositionalTab { .. } => out.push('\t'),
            FlowRunKind::Field { shown, .. } | FlowRunKind::PageRef { shown, .. } => out.push_str(shown),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Determine the relative position of `target_para` relative to `current_para` in `doc`.
/// Returns "上" if target is before current, "下" if target is after current.
pub fn relative_paragraph_position(doc: &FlowDocument, current_para: u32, target_para: u32) -> &'static str {
    let paras = all_paragraphs(doc);
    let cur_idx = paras.iter().position(|p| p.id == current_para);
    let target_idx = paras.iter().position(|p| p.id == target_para);
    match (cur_idx, target_idx) {
        (Some(c), Some(t)) if t > c => "下",
        _ => "上",
    }
}

/// フィールドの更新: the commands giving the captions' numbers (`SEQ`, in
/// the order of the document, by their name) and the cross-references
/// (`REF`, the text of their bookmark) their values now. Empty when they
/// have them already.
pub fn updated_fields(doc: &FlowDocument) -> Vec<FlowCommand> {
    updated_fields_at(doc, None)
}

/// [`updated_fields`], and the dates and times (`DATE`, `TIME`) as of `now`
/// when it is given.
pub fn updated_fields_at(doc: &FlowDocument, now: Option<&crate::date_field::LocalTime>) -> Vec<FlowCommand> {
    updated_fields_in_range(doc, None, now)
}

/// [`updated_fields_at`] restricted to paragraph runs within `range` when given.
pub fn updated_fields_in_range(doc: &FlowDocument, range: Option<(FlowPos, FlowPos)>, now: Option<&crate::date_field::LocalTime>) -> Vec<FlowCommand> {
    updated_fields_in_range_with(doc, range, now, None)
}

/// As [`updated_fields_in_range`], with `include` giving the text an `INCLUDETEXT` field's instruction names (the file read by whoever
/// has the file system; `None` to leave the result as it is).
pub fn updated_fields_in_range_with(doc: &FlowDocument, range: Option<(FlowPos, FlowPos)>, now: Option<&crate::date_field::LocalTime>, include: Option<&dyn Fn(&str) -> Option<String>>) -> Vec<FlowCommand> {
    use std::collections::HashMap;
    let ps = all_paragraphs(doc);
    let order: Vec<u32> = ps.iter().map(|p| p.id).collect();
    let index = |id: u32| order.iter().position(|o| *o == id).unwrap_or(0);
    // The captions' numbers, by paragraph and run (across the entire document).
    let mut counters: HashMap<String, u32> = HashMap::new();
    let mut numbers: HashMap<(u32, usize), String> = HashMap::new();
    // The LISTNUM numbers, by paragraph and run (across the entire document).
    let mut listnum_counters: HashMap<String, [u32; 10]> = HashMap::new();
    let mut listnum_numbers: HashMap<(u32, usize), String> = HashMap::new();
    // The AUTONUM, AUTONUMLGL, AUTONUMOUT numbers (across the entire document).
    let mut autonum_counter = 0u32;
    let mut autonumlgl_counter = 0u32;
    let mut autonumout_counter = 0u32;
    let mut autonum_numbers: HashMap<(u32, usize), String> = HashMap::new();
    let mut set_bookmarks: HashMap<String, String> = HashMap::new();
    // 章ごとに戻す (`SEQ name \s level`): the headings of each level met so far, and how many each
    // caption's counter was last reset at.
    let mut headings_met = [0u32; 10];
    let mut reset_at: HashMap<String, u32> = HashMap::new();
    // The headings are counted only for a caption numbered again from a heading (`SEQ … \s`).
    let counts_headings = ps.iter().any(|p| p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Field { instruction, .. } if instruction.contains("\\s") && instruction.contains("SEQ"))));
    for p in &ps {
        if counts_headings
            && let Some(level) = heading_level(&doc.styles, p).filter(|l| (1..=9).contains(l))
        {
            headings_met[level as usize] += 1;
        }
        for (i, run) in p.runs.iter().enumerate() {
            let FlowRunKind::Field { instruction, .. } = &run.kind else { continue };
            let expanded_gather;
            let eff_gather = if instruction.contains('{') && instruction.contains('}') {
                expanded_gather = crate::field_format::expand_nested_fields(instruction, |inner| {
                    Some(evaluate_field_instruction(inner, doc, FlowPos::new(p.id, 0)))
                });
                &expanded_gather
            } else {
                instruction.as_str()
            };
            let words = field_words(eff_gather);
            let first_word = words.first().copied().unwrap_or("");
            if first_word.eq_ignore_ascii_case("SEQ") {
                let Some(name) = words.get(1) else { continue };
                let c = counters.entry(name.to_string()).or_insert(0);
                if let Some(at) = words.iter().position(|w| *w == "\\s")
                    && let Some(level) = words.get(at + 1).and_then(|v| v.parse::<usize>().ok()).filter(|l| (1..=9).contains(l))
                {
                    let met = headings_met[level];
                    if reset_at.insert(name.to_string(), met) != Some(met) {
                        *c = 0;
                    }
                }
                if let Some(at) = words.iter().position(|w| *w == "\\r")
                    && let Some(n) = words.get(at + 1).and_then(|v| v.parse::<u32>().ok())
                {
                    *c = n;
                } else if !words.contains(&"\\c") {
                    *c += 1;
                }
                numbers.insert((p.id, i), seq_number(*c, eff_gather));
            } else if first_word.eq_ignore_ascii_case("LISTNUM") {
                let (name, level, start) = parse_listnum(eff_gather);
                let lvl = level.clamp(1, 9) as usize;
                let c = listnum_counters.entry(name).or_insert([0; 10]);
                if let Some(s) = start {
                    c[lvl] = s;
                } else {
                    c[lvl] += 1;
                }
                // 下位レベル（lvl + 1..=9）のカウンターをリセット
                for sub in (lvl + 1)..=9 {
                    c[sub] = 0;
                }
                let num = c[lvl];
                let formatted = crate::field_format::format_listnum(num, level, eff_gather);
                listnum_numbers.insert((p.id, i), formatted);
            } else if first_word.eq_ignore_ascii_case("AUTONUM") {
                autonum_counter += 1;
                autonum_numbers.insert((p.id, i), crate::field_format::format_autonum(autonum_counter, "AUTONUM", eff_gather));
            } else if first_word.eq_ignore_ascii_case("AUTONUMLGL") {
                autonumlgl_counter += 1;
                autonum_numbers.insert((p.id, i), crate::field_format::format_autonum(autonumlgl_counter, "AUTONUMLGL", eff_gather));
            } else if first_word.eq_ignore_ascii_case("AUTONUMOUT") {
                autonumout_counter += 1;
                autonum_numbers.insert((p.id, i), crate::field_format::format_autonum(autonumout_counter, "AUTONUMOUT", eff_gather));
            } else if first_word.eq_ignore_ascii_case("SET") {
                if let Some((name, val)) = crate::field_format::parse_set_field(eff_gather) {
                    set_bookmarks.insert(name, val);
                }
            } else if first_word.eq_ignore_ascii_case("ASK") {
                if let Some((name, val)) = crate::field_format::parse_ask_field(eff_gather) {
                    set_bookmarks.insert(name, val);
                }
            }
        }
    }
    let bookmarked: HashMap<&str, &FlowParagraph> = ps.iter().flat_map(|p| p.bookmarks.iter().map(move |b| (b.as_str(), *p))).collect();
    let mut commands = Vec::new();
    for p in &ps {
        // Only the fields change: a paragraph with none is left alone (and not copied).
        if !p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::Field { .. })) {
            continue;
        }
        let i = index(p.id);
        let (lo, hi) = match range {
            None => (0, usize::MAX),
            Some((a, b)) => {
                if i < index(a.para) || i > index(b.para) {
                    continue;
                }
                (if i == index(a.para) { a.offset } else { 0 }, if i == index(b.para) { b.offset } else { usize::MAX })
            }
        };

        let mut q = (*p).clone();
        let mut at = 0;
        let mut changed = false;
        for (idx, run) in q.runs.iter_mut().enumerate() {
            let len = run.len();
            let inside = if lo == hi {
                at <= lo && lo <= at + len
            } else {
                at < hi && at + len > lo
            };
            at += len;
            if !inside {
                continue;
            }

            if run.field_locked {
                continue;
            }
            let FlowRunKind::Field { instruction, shown } = &mut run.kind else { continue };
            let expanded_eval;
            let eff_instruction = if instruction.contains('{') && instruction.contains('}') {
                expanded_eval = crate::field_format::expand_nested_fields(instruction, |inner| {
                    Some(evaluate_field_instruction(inner, doc, FlowPos::new(p.id, 0)))
                });
                &expanded_eval
            } else {
                instruction.as_str()
            };
            let words = field_words(eff_instruction);
            let name = words.first().copied().unwrap_or("").to_ascii_uppercase();
            let value = match name.as_str() {
                "SEQ" => numbers.get(&(p.id, idx)).cloned(),
                "LISTNUM" => listnum_numbers.get(&(p.id, idx)).cloned(),
                "AUTONUM" | "AUTONUMLGL" | "AUTONUMOUT" => autonum_numbers.get(&(p.id, idx)).cloned(),
                "REF" => {
                    let target_name = words.get(1).copied().unwrap_or("");
                    if let Some(target) = bookmarked.get(target_name) {
                        let val = if words.contains(&"\\p") {
                            relative_paragraph_position(doc, p.id, target.id).to_string()
                        } else if words.contains(&"\\r") {
                            let labels = crate::flow::list_labels(doc);
                            labels.get(&target.id).cloned().unwrap_or_else(|| "1".to_string())
                        } else {
                            reference_text(target, &numbers)
                        };
                        Some(crate::field_format::apply_instruction_text_switches(val, eff_instruction))
                    } else if let Some((_, val)) = set_bookmarks.iter().find(|(k, _)| k.eq_ignore_ascii_case(target_name)) {
                        Some(crate::field_format::apply_instruction_text_switches(val.clone(), eff_instruction))
                    } else {
                        None
                    }
                },
                "NOTEREF" => {
                    if words.contains(&"\\p") {
                        if let Some(bm) = words.get(1) {
                            if let Some(target) = bookmarked.get(bm) {
                                Some(relative_paragraph_position(doc, p.id, target.id).to_string())
                            } else {
                                Some("上".to_string())
                            }
                        } else {
                            Some("上".to_string())
                        }
                    } else {
                        doc.noteref_text(eff_instruction).map(|v| crate::field_format::apply_instruction_text_switches(v, eff_instruction))
                    }
                },
                "STYLEREF" => doc.styleref_text_at(eff_instruction, Some(p.id)).map(|v| crate::field_format::apply_instruction_text_switches(v, eff_instruction)),
                "AUTHOR" => Some(crate::field_format::apply_instruction_text_switches(doc.author_text(eff_instruction), eff_instruction)),
                "TITLE" => Some(crate::field_format::apply_instruction_text_switches(doc.title_text(eff_instruction), eff_instruction)),
                "SUBJECT" => Some(crate::field_format::apply_instruction_text_switches(doc.subject_text(eff_instruction), eff_instruction)),
                "COMMENTS" => Some(crate::field_format::apply_instruction_text_switches(doc.comments_text(eff_instruction), eff_instruction)),
                "DOCPROPERTY" => doc.doc_property_text(eff_instruction).map(|v| crate::field_format::apply_instruction_text_switches(v, eff_instruction)),
                "CREATEDATE" => doc.created_date_text(eff_instruction),
                "SAVEDATE" => doc.saved_date_text(eff_instruction),
                "PRINTDATE" => doc.print_date_text(eff_instruction),
                "FILENAME" => Some(crate::field_format::apply_instruction_text_switches(doc.file_name.clone().unwrap_or_else(|| "Document.docx".to_string()), eff_instruction)),
                "FILESIZE" => Some(doc.file_size_text(eff_instruction)),
                "DOCVARIABLE" => Some(doc.doc_variable_text(eff_instruction)),
                "USERNAME" => Some(doc.user_name_text(eff_instruction)),
                "USERINITIALS" => Some(doc.user_initials_text(eff_instruction)),
                "USERADDRESS" => Some(doc.user_address_text(eff_instruction)),
                "QUOTE" => Some(doc.quote_text(eff_instruction)),
                "SYMBOL" => Some(doc.symbol_text(eff_instruction)),
                "SET" => Some(String::new()),
                "IF" => crate::field_format::if_field_text(eff_instruction),
                "COMPARE" => crate::field_format::compare_field_text(eff_instruction),
                "HYPERLINK" => Some(doc.hyperlink_text(eff_instruction, shown)),
                "TEMPLATE" => Some(doc.template_text(eff_instruction)),
                "ADVANCE" => Some(doc.advance_text(eff_instruction)),
                "INFO" => doc.info_text(eff_instruction),
                "KEYWORDS" | "LASTSAVEDBY" | "NUMCHARS" | "NUMWORDS" | "REVNUM" => doc.bare_info_text(eff_instruction),
                "ASK" => Some(String::new()),
                "FILLIN" | "FILL-IN" => Some(doc.fillin_text(eff_instruction)),
                "AUTOTEXT" => Some(crate::field_format::format_autotext(eff_instruction)),
                "AUTOTEXTLIST" => Some(crate::field_format::format_autotextlist(eff_instruction)),
                "CITATION" => Some(crate::field_format::format_citation(eff_instruction)),
                "BIBLIOGRAPHY" => Some(crate::field_format::format_bibliography(eff_instruction)),
                "INCLUDETEXT" => include.and_then(|f| f(eff_instruction)).map(|v| crate::field_format::apply_instruction_text_switches(v, eff_instruction)),
                _ => {
                    if eff_instruction.trim_start().starts_with('=') {
                        if let Some((table, row, col)) = find_table_cell_for_paragraph(doc, p.id) {
                            let resolver = FlowTableCellResolver { table, row, col };
                            crate::field_format::formula_text_with_resolver(eff_instruction, Some(&resolver))
                        } else {
                            crate::field_format::formula_text(eff_instruction)
                        }
                    } else {
                        now.and_then(|t| crate::date_field::date_field_text(eff_instruction, t))
                    }
                }
            };
            if let Some(v) = value && *shown != v {
                *shown = v;
                changed = true;
            }
        }
        if changed && q != **p
            && let Some((container, block_idx)) = locate(doc, p.id)
        {
            commands.push(FlowCommand::ReplaceBlocks { container, index: block_idx, remove: 1, blocks: vec![Block::Paragraph(q)] });
        }
    }
    commands
}

/// A caption paragraph after paragraph `after` (図表番号の挿入): `label`,
/// Word の図表番号の番号書式（ST_NumberFormat）
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CaptionNumberFormat {
    #[default]
    Arabic,
    UpperRoman,
    LowerRoman,
    UpperLetter,
    LowerLetter,
}

impl CaptionNumberFormat {
    pub fn switch(self) -> &'static str {
        match self {
            Self::Arabic => "\\* ARABIC",
            Self::UpperRoman => "\\* ROMAN",
            Self::LowerRoman => "\\* roman",
            Self::UpperLetter => "\\* ALPHABETIC",
            Self::LowerLetter => "\\* alphabetic",
        }
    }

    pub fn format(self, n: u32) -> String {
        match self {
            Self::Arabic => n.to_string(),
            Self::UpperRoman => crate::flow::format_list_number(n, "upperRoman"),
            Self::LowerRoman => crate::flow::format_list_number(n, "lowerRoman"),
            Self::UpperLetter => crate::flow::format_list_number(n, "upperLetter"),
            Self::LowerLetter => crate::flow::format_list_number(n, "lowerLetter"),
        }
    }
}

/// A caption paragraph after paragraph `after`: `label` (if not excluded),
/// a space, its number (`SEQ label <switch>`) and `text`, in the style
/// `caption` when the document has it, with a bookmark (`_Ref…`) for
/// cross-references to it. The commands and the bookmark's name.
pub fn insert_caption_detailed(
    doc: &FlowDocument,
    after: u32,
    label: &str,
    text: &str,
    format: CaptionNumberFormat,
    exclude_label: bool,
) -> Option<(Vec<FlowCommand>, String)> {
    insert_caption_in_chapter(doc, after, label, text, format, exclude_label, None)
}

/// [`insert_caption_detailed`] with the chapter number in the caption (章番号を含める): `chapter` is the heading level
/// whose number goes first and the separator after it (`図 1-2`: `STYLEREF 1 \s`, `-`, `SEQ 図 \s 1`, the numbers
/// starting again at each chapter).
pub fn insert_caption_in_chapter(
    doc: &FlowDocument,
    after: u32,
    label: &str,
    text: &str,
    format: CaptionNumberFormat,
    exclude_label: bool,
    chapter: Option<(u32, &str)>,
) -> Option<(Vec<FlowCommand>, String)> {
    let (container, index) = locate(doc, after)?;
    let mut commands = Vec::new();
    // The caption style (図表番号), as Word defines it, when the document
    // has none.
    let mut styles = (*doc.styles).clone();
    if styles.paragraph_style_named("caption").is_none() {
        let japanese = styles.default_paragraph.as_deref() == Some("a");
        let mut style_id = "Caption".to_string();
        while styles.styles.contains_key(&style_id) {
            style_id.push('_');
        }
        let run = if japanese {
            RunFormat { bold: Some(true), ..RunFormat::default() }
        } else {
            RunFormat { italic: Some(true), size: Some(900), color: Some(crate::drawing::Rgb(0x44546A)), ..RunFormat::default() }
        };
        let para = crate::flow::ParaFormat { after: (!japanese).then_some(200 * 635), ..Default::default() };
        let style = crate::flow::FlowStyle { kind: crate::flow::StyleKind::Paragraph, name: "caption".into(), based_on: styles.default_paragraph.clone(), para, run, next: styles.default_paragraph.clone(), auto_update: false, ui_priority: Some(35), quick: true, conditional: Vec::new(), num_id: None };
        styles.styles.insert(style_id.clone(), style.clone());
        commands.push(FlowCommand::SetStyle { id: style_id, style: Some(style) });
    }
    let styles = &styles;
    let id = doc.new_id();
    let bookmark = format!("_Ref{:09}", 100_000_000 + id);
    let style = styles.paragraph_style_named("caption").map(str::to_string);
    let mut p = FlowParagraph { id, style, bookmarks: vec![bookmark.clone()], ..FlowParagraph::default() };
    let base = FlowRunStyle::default();
    let clean_label = label.trim();
    if !exclude_label && !clean_label.is_empty() {
        p.runs.push(FlowRun::new(FlowRunKind::Text(format!("{clean_label} ")), base.clone()));
    }
    let seq_label = if clean_label.is_empty() { "Figure" } else { clean_label };
    let mut instruction = format!("SEQ {seq_label} {}", format.switch());
    if let Some((level, separator)) = chapter {
        let reference = format!("STYLEREF {level} \\s");
        let shown = doc.styleref_text_at(&reference, Some(after)).unwrap_or_else(|| "0".to_string());
        p.runs.push(FlowRun::new(FlowRunKind::Field { instruction: reference, shown }, base.clone()));
        p.runs.push(FlowRun::new(FlowRunKind::Text(separator.to_string()), base.clone()));
        instruction.push_str(&format!(" \\s {level}"));
    }
    let shown = format.format(1);
    p.runs.push(FlowRun::new(FlowRunKind::Field { instruction, shown }, base.clone()));
    if !text.trim().is_empty() {
        p.runs.push(FlowRun::new(FlowRunKind::Text(format!(" {}", text.trim())), base));
    }
    p.resolve(styles);
    commands.push(FlowCommand::ReplaceBlocks { container, index: index + 1, remove: 0, blocks: vec![Block::Paragraph(p)] });
    Some((commands, bookmark))
}

pub fn insert_caption(doc: &FlowDocument, after: u32, label: &str, text: &str) -> Option<(Vec<FlowCommand>, String)> {
    insert_caption_detailed(doc, after, label, text, CaptionNumberFormat::Arabic, false)
}

/// ドキュメント内に存在する図表番号のラベル一覧（重複なし）を取得する。
pub fn caption_labels(doc: &FlowDocument) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for p in all_paragraphs(doc) {
        for run in &p.runs {
            if let FlowRunKind::Field { instruction, .. } = &run.kind {
                let words = field_words(instruction);
                if words.first().copied() == Some("SEQ") && words.len() >= 2 {
                    let label = words[1].to_string();
                    if !labels.contains(&label) {
                        labels.push(label);
                    }
                }
            }
        }
    }
    labels
}

/// The commands putting a cross-reference to caption paragraph `caption`
/// at `at` (相互参照, 番号とラベルのみ): a `REF` field to its bookmark (one
/// made for it when it has none).
pub fn cross_reference(doc: &FlowDocument, at: FlowPos, caption: u32) -> Option<Vec<FlowCommand>> {
    let p = paragraph(doc, caption)?;
    let mut commands = Vec::new();
    let bookmark = match p.bookmarks.iter().find(|b| b.starts_with("_Ref")) {
        Some(b) => b.clone(),
        None => {
            let b = format!("_Ref{:09}", 100_000_000 + p.id);
            let (container, index) = locate(doc, caption)?;
            let mut q = p.clone();
            q.bookmarks.push(b.clone());
            commands.push(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(q)] });
            b
        }
    };
    let shown = reference_text(p, &std::collections::HashMap::new());
    commands.push(FlowCommand::InsertField { at, kind: FlowRunKind::Field { instruction: format!("REF {bookmark} \\h"), shown } });
    Some(commands)
}

/// The captions of the document: their paragraph, bookmark (if any) and
/// what a cross-reference to them shows.
pub fn captions(doc: &FlowDocument) -> Vec<(u32, Option<String>, String)> {
    let numbers = std::collections::HashMap::new();
    all_paragraphs(doc)
        .into_iter()
        .filter(|p| p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Field { instruction, .. } if field_words(instruction).first() == Some(&"SEQ"))))
        .map(|p| (p.id, p.bookmarks.iter().find(|b| b.starts_with("_Ref")).cloned(), reference_text(p, &numbers)))
        .collect()
}

/// 相互参照で参照する項目の種類（Word の「参照する項目」Reference type）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CrossReferenceType {
    #[default]
    Heading,
    Bookmark,
    Footnote,
    Endnote,
    Caption,
}

impl CrossReferenceType {
    pub const ALL: [CrossReferenceType; 5] = [
        Self::Heading,
        Self::Bookmark,
        Self::Footnote,
        Self::Endnote,
        Self::Caption,
    ];
}

/// 相互参照で挿入する参照内容（Word の「挿入する参照内容」Insert reference to）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CrossReferenceKind {
    #[default]
    /// 見出しやブックマークのテキスト、または図表番号全体（`REF`）
    Text,
    /// ページ番号（`PAGEREF`）
    PageNumber,
    /// 段落番号 / アウトライン番号（`REF \r`）
    ParagraphNumber,
    /// 脚注 / 文末脚注番号（`NOTEREF`）
    NoteNumber,
    /// 図表の番号とラベルのみ（例: "図 1"、`REF`）
    LabelAndNumber,
    /// 図表の説明文のみ（例: "システム構成図"、`REF`）
    CaptionText,
    /// 上の位置/下の位置（相対位置、`REF \p` / `PAGEREF \p` / `NOTEREF \p`）
    RelativePosition,
}

/// 相互参照の対象項目。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossReferenceTarget {
    /// 参照先識別子（段落ID または 注ID）
    pub id: u32,
    /// 参照用ブックマーク名（既存のブックマーク、または挿入時に付与される `_Ref...`）
    pub bookmark: String,
    /// 一覧に表示する表示ラベル（見出し名、ブックマーク名、注の抜粋、図表名など）
    pub label: String,
    /// 段落に既存のブックマークがあるか（false なら挿入時に段落へ付与が必要）
    pub has_bookmark: bool,
}

/// ドキュメント内の指定された種類の相互参照対象一覧を取得する。
pub fn cross_reference_targets(doc: &FlowDocument, ref_type: CrossReferenceType) -> Vec<CrossReferenceTarget> {
    match ref_type {
        CrossReferenceType::Heading => {
            let styles = &doc.styles;
            let labels = crate::flow::list_labels(doc);
            let mut targets = Vec::new();
            for p in all_paragraphs(doc) {
                if heading_level(styles, p).is_some() {
                    let text: String = p.text().chars().filter(|c| !matches!(c, '\t' | '\u{b}' | '\u{c}' | '\u{e}' | '\u{fffc}')).collect();
                    if text.trim().is_empty() {
                        continue;
                    }
                    let label = match labels.get(&p.id).filter(|l| !l.is_empty()) {
                        Some(l) => format!("{l} {text}"),
                        None => text,
                    };
                    let (bookmark, has_bookmark) = match p.bookmarks.iter().find(|b| b.starts_with("_Ref")) {
                        Some(b) => (b.clone(), true),
                        None => (format!("_Ref{:09}", 100_000_000 + p.id), false),
                    };
                    targets.push(CrossReferenceTarget { id: p.id, bookmark, label, has_bookmark });
                }
            }
            targets
        }
        CrossReferenceType::Bookmark => {
            let mut targets = Vec::new();
            for p in all_paragraphs(doc) {
                for b in &p.bookmarks {
                    if !b.starts_with('_') {
                        targets.push(CrossReferenceTarget {
                            id: p.id,
                            bookmark: b.clone(),
                            label: b.clone(),
                            has_bookmark: true,
                        });
                    }
                }
            }
            targets
        }
        CrossReferenceType::Footnote => {
            let mut targets = Vec::new();
            for (i, note) in doc.footnotes.iter().enumerate() {
                let note_num = doc.page.footnotes.start.saturating_add(i as u32);
                let num_str = doc.page.footnotes.format.number(note_num);
                let text = note.blocks.first().and_then(|b| match b {
                    Block::Paragraph(p) => Some(p.text()),
                    _ => None,
                }).unwrap_or_default();
                let snippet: String = text.chars().filter(|c| !matches!(c, '\t' | '\u{b}' | '\u{c}' | '\u{e}' | '\u{fffc}')).take(30).collect();
                let label = if snippet.trim().is_empty() {
                    format!("{num_str}: (脚注)")
                } else {
                    format!("{num_str}: {snippet}")
                };
                let bookmark = format!("_RefFootnote_{}", note.id);
                // Check if any paragraph already has this bookmark
                let has_bm = all_paragraphs(doc).iter().any(|p| p.bookmarks.contains(&bookmark));
                targets.push(CrossReferenceTarget {
                    id: note.id,
                    bookmark,
                    label,
                    has_bookmark: has_bm,
                });
            }
            targets
        }
        CrossReferenceType::Endnote => {
            let mut targets = Vec::new();
            for (i, note) in doc.endnotes.iter().enumerate() {
                let note_num = doc.page.endnotes.start.saturating_add(i as u32);
                let num_str = doc.page.endnotes.format.number(note_num);
                let text = note.blocks.first().and_then(|b| match b {
                    Block::Paragraph(p) => Some(p.text()),
                    _ => None,
                }).unwrap_or_default();
                let snippet: String = text.chars().filter(|c| !matches!(c, '\t' | '\u{b}' | '\u{c}' | '\u{e}' | '\u{fffc}')).take(30).collect();
                let label = if snippet.trim().is_empty() {
                    format!("{num_str}: (文末脚注)")
                } else {
                    format!("{num_str}: {snippet}")
                };
                let bookmark = format!("_RefEndnote_{}", note.id);
                let has_bm = all_paragraphs(doc).iter().any(|p| p.bookmarks.contains(&bookmark));
                targets.push(CrossReferenceTarget {
                    id: note.id,
                    bookmark,
                    label,
                    has_bookmark: has_bm,
                });
            }
            targets
        }
        CrossReferenceType::Caption => {
            captions(doc).into_iter().map(|(id, bm, text)| {
                let (bookmark, has_bookmark) = match bm {
                    Some(b) => (b, true),
                    None => (format!("_Ref{:09}", 100_000_000 + id), false),
                };
                CrossReferenceTarget { id, bookmark, label: text, has_bookmark }
            }).collect()
        }
    }
}

/// 相互参照をキャレット位置 `at` に挿入するコマンド群を生成する（1ステップ完全 Undo/Redo 対応）。
pub fn insert_cross_reference_target(
    doc: &FlowDocument,
    at: FlowPos,
    target: &CrossReferenceTarget,
    ref_type: CrossReferenceType,
    kind: CrossReferenceKind,
    hyperlink: bool,
) -> Option<Vec<FlowCommand>> {
    let mut commands = Vec::new();
    let bookmark = target.bookmark.clone();

    // 1. ブックマークが未登録の場合、対象の段落にブックマークを付与
    if !target.has_bookmark {
        match ref_type {
            CrossReferenceType::Heading | CrossReferenceType::Caption => {
                if let Some(p) = paragraph(doc, target.id) {
                    if !p.bookmarks.contains(&bookmark) {
                        if let Some((container, index)) = locate(doc, target.id) {
                            let mut q = p.clone();
                            q.bookmarks.push(bookmark.clone());
                            commands.push(FlowCommand::ReplaceBlocks {
                                container,
                                index,
                                remove: 1,
                                blocks: vec![Block::Paragraph(q)],
                            });
                        }
                    }
                }
            }
            CrossReferenceType::Footnote | CrossReferenceType::Endnote => {
                let is_end = ref_type == CrossReferenceType::Endnote;
                // 本文中で該当する NoteRef を持つ段落を検索
                let note_ref_para = all_paragraphs(doc).into_iter().find(|p| {
                    p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::NoteRef { endnote, id } if *endnote == is_end && *id == target.id))
                });
                if let Some(p) = note_ref_para {
                    if !p.bookmarks.contains(&bookmark) {
                        if let Some((container, index)) = locate(doc, p.id) {
                            let mut q = p.clone();
                            q.bookmarks.push(bookmark.clone());
                            commands.push(FlowCommand::ReplaceBlocks {
                                container,
                                index,
                                remove: 1,
                                blocks: vec![Block::Paragraph(q)],
                            });
                        }
                    }
                }
            }
            CrossReferenceType::Bookmark => {}
        }
    }

    // 2. フィールド命令（コード）と初期表示文字列（shown）を組み立て
    let h_switch = if hyperlink { " \\h" } else { "" };
    let (instruction, shown) = match kind {
        CrossReferenceKind::PageNumber => {
            (format!("PAGEREF {bookmark}{h_switch}"), "1".to_string())
        }
        CrossReferenceKind::NoteNumber => {
            let num = doc.noteref_text(&format!("NOTEREF {bookmark}")).unwrap_or_else(|| "1".to_string());
            (format!("NOTEREF {bookmark}{h_switch}"), num)
        }
        CrossReferenceKind::ParagraphNumber => {
            let labels = crate::flow::list_labels(doc);
            let num = labels.get(&target.id).cloned().unwrap_or_else(|| "1".to_string());
            (format!("REF {bookmark} \\r{h_switch}"), num)
        }
        CrossReferenceKind::LabelAndNumber => {
            let label = if ref_type == CrossReferenceType::Caption {
                target.label.split_whitespace().take(2).collect::<Vec<_>>().join(" ")
            } else {
                target.label.clone()
            };
            (format!("REF {bookmark}{h_switch}"), label)
        }
        CrossReferenceKind::CaptionText => {
            let desc = if let Some(p) = paragraph(doc, target.id) {
                caption_description_text(p)
            } else {
                target.label.clone()
            };
            (format!("REF {bookmark}{h_switch}"), desc)
        }
        CrossReferenceKind::RelativePosition => {
            let pos_str = relative_paragraph_position(doc, at.para, target.id);
            let field_name = match ref_type {
                CrossReferenceType::Footnote | CrossReferenceType::Endnote => "NOTEREF",
                _ => "REF",
            };
            (format!("{field_name} {bookmark} \\p{h_switch}"), pos_str.to_string())
        }
        CrossReferenceKind::Text => {
            let text = if ref_type == CrossReferenceType::Caption {
                if let Some(p) = paragraph(doc, target.id) {
                    entire_caption_text(p)
                } else {
                    target.label.clone()
                }
            } else {
                target.label.clone()
            };
            (format!("REF {bookmark}{h_switch}"), text)
        }
    };

    commands.push(FlowCommand::InsertField {
        at,
        kind: FlowRunKind::Field { instruction, shown },
    });

    Some(commands)
}

/// How a numbered list goes on at a paragraph (番号の振り直し).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Renumber {
    /// Start again from `n` (1 から振り直す; 番号の設定 with a start value).
    StartAt(u32),
    /// Go on from the list before (前のリストの続きに戻す / 続けて番号を振る).
    Continue,
}

/// The paragraphs of the body (and its tables) in order.
pub fn all_paragraphs(doc: &FlowDocument) -> Vec<&FlowParagraph> {
    fn walk<'a>(blocks: &'a [Block], out: &mut Vec<&'a FlowParagraph>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => out.push(p),
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, out);
                        }
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&doc.body, &mut out);
    out
}

/// All paragraphs anywhere in the document (body, headers, footers, notes, comments).
pub fn all_paragraphs_everywhere(doc: &FlowDocument) -> Vec<&FlowParagraph> {
    fn walk<'a>(blocks: &'a [Block], out: &mut Vec<&'a FlowParagraph>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => out.push(p),
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, out);
                        }
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&doc.body, &mut out);
    walk(&doc.header, &mut out);
    walk(&doc.footer, &mut out);
    walk(&doc.header_first, &mut out);
    walk(&doc.footer_first, &mut out);
    walk(&doc.header_even, &mut out);
    walk(&doc.footer_even, &mut out);
    for b in &doc.body {
        if let Block::Paragraph(p) = b && let Some(sec) = &p.section {
            for part in &sec.parts {
                if let Some(blocks) = part {
                    walk(blocks, &mut out);
                }
            }
        }
    }
    for note in &doc.footnotes {
        walk(&note.blocks, &mut out);
    }
    for note in &doc.endnotes {
        walk(&note.blocks, &mut out);
    }
    for c in &doc.comments {
        walk(&c.blocks, &mut out);
    }
    out
}

/// The commands removing all hidden text runs across the document.
pub fn remove_all_hidden_text(doc: &FlowDocument) -> Option<FlowCommand> {
    let is_hidden = |r: &FlowRun| r.style.hidden || r.format.hidden == Some(true);
    let mut commands = Vec::new();
    for p in all_paragraphs_everywhere(doc) {
        if !p.runs.iter().any(is_hidden) {
            continue;
        }
        let mut q = p.clone();
        q.runs.retain(|r| !is_hidden(r));
        q.normalize();
        if let Some((container, index)) = locate(doc, p.id) {
            commands.push(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(q)] });
        }
    }
    if commands.is_empty() {
        None
    } else {
        Some(FlowCommand::Many(commands))
    }
}

/// The commands numbering the list of paragraph `id` from there on as
/// `how`: the paragraph and the later ones of its list go to a new list
/// (`w:num` with `w:startOverride`) or to the list before. `None` when the
/// paragraph is not in a list, or there is no list before to go on from.
pub fn renumber(doc: &FlowDocument, id: u32, how: Renumber) -> Option<Vec<FlowCommand>> {
    let ps = all_paragraphs(doc);
    let at = ps.iter().position(|p| p.id == id)?;
    let list = ps[at].list?;
    let numbering = &doc.styles.numbering;
    let abstract_id = numbering.nums.get(&list.num_id)?.abstract_id;
    let mut commands = Vec::new();
    let target = match how {
        Renumber::StartAt(n) => {
            let mut new = numbering.clone();
            let num_id = new.nums.keys().max().map_or(1, |m| m + 1);
            let mut start_overrides = std::collections::BTreeMap::new();
            start_overrides.insert(list.level, n);
            new.nums.insert(num_id, crate::flow::ListInstance { abstract_id, start_overrides });
            commands.push(FlowCommand::SetNumbering { numbering: new });
            num_id
        }
        Renumber::Continue => {
            let numbered = |num: u32| numbering.level(num, 0).is_some_and(|l| l.format != "bullet");
            ps[..at].iter().rev().filter_map(|p| p.list).map(|l| l.num_id).find(|&n| n != list.num_id && numbered(n))?
        }
    };
    for p in &ps[at..] {
        if p.list.is_some_and(|l| l.num_id == list.num_id) {
            commands.push(FlowCommand::SetParagraphFormat { first: p.id, last: p.id, change: ParaChange::List(Some(target)) });
        }
    }
    Some(commands)
}

/// 選択箇所と一致するように更新 (Word's Update Style to Match Selection):
/// the paragraph style of paragraph `id` with the paragraph's own
/// formatting and the formatting of the text at `offset` folded in, and the
/// paragraph without that formatting of its own (it has it from the style
/// now). `None` when the paragraph has no style to change.
pub fn style_matching(doc: &FlowDocument, id: u32, offset: usize) -> Option<(String, crate::flow::FlowStyle, FlowParagraph)> {
    let p = paragraph(doc, id)?;
    let style_id = p.style.clone().or_else(|| doc.styles.default_paragraph.clone())?;
    let mut style = doc.styles.styles.get(&style_id)?.clone();
    let moved_para = crate::flow::ParaFormat { num_id: None, level: None, ..p.direct.clone() };
    let (moved_run, _, _) = p.format_at(offset);
    style.para.overlay(&moved_para);
    style.run.overlay(&moved_run);
    let mut styles = (*doc.styles).clone();
    styles.styles.insert(style_id.clone(), style.clone());
    let mut q = p.clone();
    q.direct = crate::flow::ParaFormat { num_id: p.direct.num_id, level: p.direct.level, ..Default::default() };
    for run in &mut q.runs {
        run.format = run.format.without(&moved_run);
    }
    q.mark_format = q.mark_format.without(&moved_run);
    q.normalize();
    q.resolve(&styles);
    Some((style_id, style, q))
}

/// TOC (Table of Contents) フィールド命令の構成情報。
/// Word / ECMA-376 Part 1, §17.16.5.58 TOC に準拠。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocInstruction {
    /// `\o "from-to"`: 組み込み見出しレベルの範囲 (例: Some((1, 3)))
    pub heading_levels: Option<(u32, u32)>,
    /// `\t "StyleName,Level,..."`: 任意スタイル名とTOCレベル (1..=9) のマッピング
    pub custom_styles: Vec<(String, u32)>,
    /// `\c "label"`: 図表目次のラベル (例: "図", "表")
    pub caption_label: Option<String>,
    /// `\h`: 各項目をハイパーリンク（アンカー）にする
    pub hyperlinks: bool,
    /// `\z`: Webレイアウトでタブリーダーとページ番号を非表示
    pub hide_in_web: bool,
    /// `\u`: アウトラインレベルを使用
    pub use_outline_levels: bool,
    /// `\f ["id"]`: TC フィールドの項目から作る（`Some(None)` は識別子なし、`Some(Some(id))` はその識別子の TC だけ）
    pub tc_entries: Option<Option<String>>,
    /// `\l "from-to"`: TC 項目のレベルの範囲（既定 1〜9）
    pub tc_levels: (u32, u32),
    /// `\n`: ページ番号を付けない
    pub no_page_numbers: bool,
}

impl Default for TocInstruction {
    fn default() -> Self {
        Self {
            heading_levels: Some((1, 3)),
            custom_styles: Vec::new(),
            caption_label: None,
            hyperlinks: true,
            hide_in_web: true,
            use_outline_levels: true,
            tc_entries: None,
            tc_levels: (1, 9),
            no_page_numbers: false,
        }
    }
}

impl TocInstruction {
    /// 目次が必要とする全レベル一覧 (1..=9) を昇順で取得する。
    pub fn all_levels(&self) -> Vec<u32> {
        let mut lvls = std::collections::BTreeSet::new();
        if let Some((from, to)) = self.heading_levels {
            for l in from..=to {
                if (1..=9).contains(&l) {
                    lvls.insert(l);
                }
            }
        }
        for (_, l) in &self.custom_styles {
            if (1..=9).contains(l) {
                lvls.insert(*l);
            }
        }
        if lvls.is_empty() {
            lvls.insert(1);
        }
        lvls.into_iter().collect()
    }

    /// フィールド命令文字列（例: `TOC \o "1-3" \t "CustomStyle,1" \h \z \u`）を生成する。
    pub fn to_field_instruction(&self) -> String {
        let mut parts = vec!["TOC".to_string()];
        if let Some((from, to)) = self.heading_levels {
            parts.push(format!("\\o \"{from}-{to}\""));
        }
        if !self.custom_styles.is_empty() {
            let pairs: Vec<String> = self.custom_styles.iter().map(|(s, l)| format!("{s},{l}")).collect();
            parts.push(format!("\\t \"{}\"", pairs.join(",")));
        }
        if let Some(lbl) = &self.caption_label {
            parts.push(format!("\\c \"{lbl}\""));
        }
        if self.hyperlinks {
            parts.push("\\h".to_string());
        }
        if self.hide_in_web {
            parts.push("\\z".to_string());
        }
        if self.use_outline_levels {
            parts.push("\\u".to_string());
        }
        if let Some(id) = &self.tc_entries {
            match id {
                Some(id) => parts.push(format!("\\f \"{id}\"")),
                None => parts.push("\\f".to_string()),
            }
            parts.push(format!("\\l \"{}-{}\"", self.tc_levels.0, self.tc_levels.1));
        }
        if self.no_page_numbers {
            parts.push("\\n".to_string());
        }
        parts.join(" ")
    }
}

/// フィールド命令文字列からスイッチをトークン化する内部ヘルパー。
fn tokenize_field_switches(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
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
            tokens.push(s);
        } else {
            let mut s = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() {
                    break;
                }
                chars.next();
                s.push(ch);
            }
            tokens.push(s);
        }
    }
    tokens
}

/// TOC フィールド命令文字列をパースして `TocInstruction` を取得する。
pub fn parse_toc_instruction(instruction: &str) -> TocInstruction {
    let tokens = tokenize_field_switches(instruction);
    let mut heading_levels = None;
    let mut custom_styles = Vec::new();
    let mut caption_label = None;
    let mut hyperlinks = false;
    let mut hide_in_web = false;
    let mut use_outline_levels = false;
    let mut has_explicit_levels = false;
    let mut tc_entries: Option<Option<String>> = None;
    let mut tc_levels = (1u32, 9u32);
    let mut no_page_numbers = false;

    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        if tok.eq_ignore_ascii_case("\\f") {
            // `\f` alone, or with the identifier of the TC fields.
            if i + 1 < tokens.len() && !tokens[i + 1].starts_with('\\') {
                i += 1;
                tc_entries = Some(Some(tokens[i].clone()));
            } else {
                tc_entries = Some(None);
            }
            i += 1;
            continue;
        }
        if tok.eq_ignore_ascii_case("\\l") && i + 1 < tokens.len() {
            i += 1;
            let mut parts = tokens[i].split('-').filter_map(|p| p.trim().parse::<u32>().ok());
            match (parts.next(), parts.next()) {
                (Some(a), Some(b)) => tc_levels = (a, b),
                (Some(a), None) => tc_levels = (a, a),
                _ => {}
            }
            i += 1;
            continue;
        }
        if tok.eq_ignore_ascii_case("\\n") {
            no_page_numbers = true;
            i += 1;
            continue;
        }
        if tok.eq_ignore_ascii_case("TOC") {
            i += 1;
            continue;
        }
        if tok.eq_ignore_ascii_case("\\o") {
            has_explicit_levels = true;
            if i + 1 < tokens.len() {
                i += 1;
                let val = &tokens[i];
                let mut parts = val.split('-').filter_map(|p| p.trim().parse::<u32>().ok());
                match (parts.next(), parts.next()) {
                    (Some(a), Some(b)) => heading_levels = Some((a, b)),
                    (Some(a), None) => heading_levels = Some((a, a)),
                    _ => heading_levels = Some((1, 3)),
                }
            }
        } else if tok.eq_ignore_ascii_case("\\t") {
            has_explicit_levels = true;
            if i + 1 < tokens.len() {
                i += 1;
                let val = &tokens[i];
                let pairs: Vec<&str> = val.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()).collect();
                let mut p_idx = 0;
                while p_idx + 1 < pairs.len() {
                    let style_name = pairs[p_idx];
                    if let Ok(lvl) = pairs[p_idx + 1].parse::<u32>() {
                        if (1..=9).contains(&lvl) {
                            custom_styles.push((style_name.to_string(), lvl));
                        }
                    }
                    p_idx += 2;
                }
            }
        } else if tok.eq_ignore_ascii_case("\\c") {
            if i + 1 < tokens.len() {
                i += 1;
                let val = tokens[i].trim().trim_matches('"').trim_matches('\'');
                if !val.is_empty() {
                    caption_label = Some(val.to_string());
                }
            }
        } else if tok.eq_ignore_ascii_case("\\h") {
            hyperlinks = true;
        } else if tok.eq_ignore_ascii_case("\\z") {
            hide_in_web = true;
        } else if tok.eq_ignore_ascii_case("\\u") {
            use_outline_levels = true;
        }
        i += 1;
    }

    if !has_explicit_levels && caption_label.is_none() && tc_entries.is_none() {
        heading_levels = Some((1, 3));
        hyperlinks = true;
        hide_in_web = true;
        use_outline_levels = true;
    }

    TocInstruction { heading_levels, custom_styles, caption_label, hyperlinks, hide_in_web, use_outline_levels, tc_entries, tc_levels, no_page_numbers }
}

/// What a `TC "text" \f "id" \l 2 \n` field marks: an entry of a table of contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TcSpec {
    pub text: String,
    /// `\l`: the entry's level (1 when not given).
    pub level: u32,
    /// `\f`: the identifier of the table it belongs to.
    pub id: Option<String>,
    /// `\n`: no page number.
    pub no_page: bool,
}

/// The entry a TC field marks; `None` for another field.
pub fn tc_spec(instruction: &str) -> Option<TcSpec> {
    let tokens = tokenize_field_switches(instruction);
    if !tokens.first()?.eq_ignore_ascii_case("TC") {
        return None;
    }
    let text = tokens.get(1).filter(|t| !t.starts_with('\\'))?.clone();
    let mut level = 1;
    let mut id = None;
    let mut no_page = false;
    let mut i = 2;
    while i < tokens.len() {
        let t = &tokens[i];
        if t.eq_ignore_ascii_case("\\l") && i + 1 < tokens.len() {
            i += 1;
            level = tokens[i].parse::<u32>().ok().filter(|l| (1..=9).contains(l)).unwrap_or(1);
        } else if t.eq_ignore_ascii_case("\\f") && i + 1 < tokens.len() {
            i += 1;
            id = Some(tokens[i].clone());
        } else if t.eq_ignore_ascii_case("\\n") {
            no_page = true;
        }
        i += 1;
    }
    Some(TcSpec { text, level, id, no_page })
}

/// スタイルIDまたは名前が対象スタイル名と一致するか照合する。
pub fn matches_style_name(styles: &FlowStyles, style_id: &str, target_name: &str) -> bool {
    let target = target_name.trim();
    if target.is_empty() {
        return false;
    }
    if style_id.eq_ignore_ascii_case(target) {
        return true;
    }
    if let Some(s) = styles.styles.get(style_id) {
        if s.name.eq_ignore_ascii_case(target) {
            return true;
        }
    }
    for s in styles.chain(style_id) {
        if s.name.eq_ignore_ascii_case(target) {
            return true;
        }
    }
    false
}

/// 段落のスタイルと目次命令に基づいて、目次レベル (1..=9) を決定する。
pub fn resolve_paragraph_toc_level(
    styles: &FlowStyles,
    p: &FlowParagraph,
    instruction: &TocInstruction,
) -> Option<u32> {
    if let Some(style_id) = p.style.as_deref() {
        for (target_name, level) in &instruction.custom_styles {
            if matches_style_name(styles, style_id, target_name) {
                return Some(*level);
            }
        }
    }
    if let Some((from, to)) = instruction.heading_levels {
        if let Some(lvl) = heading_level(styles, p) {
            if (from..=to).contains(&lvl) {
                return Some(lvl);
            }
        }
    }
    None
}

/// The style a new header (or footer) takes (`header`, `footer`) when the
/// document lacks it, as Word defines it: based on the normal style, with a
/// centre tab and a right tab at the text area's middle and end (A4 with 1
/// inch margins: 4513 and 9026 twips; in the Japanese template 4252 and 8504).
pub fn missing_header_footer_style(styles: &crate::flow::FlowStyles, footer: bool) -> Option<(String, crate::flow::FlowStyle)> {
    use crate::flow::{FlowStyle, ParaFormat, StyleKind, TabAlign, TabChange, TabStop};
    let name = if footer { "footer" } else { "header" };
    if styles.paragraph_style_named(name).is_some() {
        return None;
    }
    let normal = styles.default_paragraph.clone();
    let (center, right) = if normal.as_deref() == Some("a") { (4252, 8504) } else { (4513, 9026) };
    let stop = |twips: i64, align| TabChange { position: twips * 635, stop: Some(TabStop { position: twips * 635, align, leader: false }) };
    let mut id = if footer { "Footer".to_string() } else { "Header".to_string() };
    while styles.styles.contains_key(&id) {
        id.push('_');
    }
    let para = ParaFormat { tabs: vec![stop(center, TabAlign::Center), stop(right, TabAlign::Right)], ..ParaFormat::default() };
    Some((id, FlowStyle { kind: StyleKind::Paragraph, name: name.to_string(), based_on: normal.clone(), next: normal, para, ..FlowStyle::default() }))
}

/// The styles a table of contents of given levels uses (`toc 1`
/// …) that the document lacks.
pub fn missing_toc_styles_for_levels(
    styles: &crate::flow::FlowStyles,
    levels: impl IntoIterator<Item = u32>,
) -> Vec<(String, crate::flow::FlowStyle)> {
    use crate::flow::{FlowStyle, ParaFormat, StyleKind};
    let normal = styles.default_paragraph.clone();
    let japanese = normal.as_deref() == Some("a");
    let mut out = Vec::new();
    let mut done = std::collections::HashSet::new();
    for level in levels {
        if !(1..=9).contains(&level) || !done.insert(level) {
            continue;
        }
        let name = format!("toc {level}");
        if styles.paragraph_style_named(&name).is_some() {
            continue;
        }
        let mut id = format!("TOC{level}");
        while styles.styles.contains_key(&id) {
            id.push('_');
        }
        let step: i64 = if japanese { 210 } else { 220 };
        let para = ParaFormat {
            left: (level > 1).then(|| (level as i64 - 1) * step * 635),
            after: (!japanese).then_some(100 * 635),
            ..ParaFormat::default()
        };
        out.push((id, FlowStyle { kind: StyleKind::Paragraph, name, based_on: normal.clone(), next: normal.clone(), para, ui_priority: Some(39), ..FlowStyle::default() }));
    }
    out
}

/// The styles a table of contents of levels `from` to `to` uses (`toc 1`
/// …) that the document lacks, as Word defines them: in a document from
/// Word's Japanese template (its normal style `a`), each level indented one
/// more character (10.5 pt); else 11 pt more, with 5 pt after.
pub fn missing_toc_styles(styles: &crate::flow::FlowStyles, from: u32, to: u32) -> Vec<(String, crate::flow::FlowStyle)> {
    missing_toc_styles_for_levels(styles, from.max(1)..=to.min(9))
}

/// A table of contents' entries based on a [`TocInstruction`],
/// with the pages `page_of` a paragraph is on; and the bookmarks
/// the headings/custom paragraphs need (`_Toc…`). The entries' ids
/// start at `first_id`; `width` is the text's width (where the page numbers end).
pub fn toc_entries_with_instruction(
    doc: &FlowDocument,
    instruction: &TocInstruction,
    page_of: &dyn Fn(u32) -> Option<u32>,
    first_id: u32,
    width: crate::geometry::Emu,
) -> (Vec<FlowParagraph>, Vec<(u32, String)>) {
    use crate::flow::{Link, TabAlign, TabChange, TabStop};
    let styles = &doc.styles;
    let labels = crate::flow::list_labels(doc);
    let mut entries = Vec::new();
    let mut bookmarks = Vec::new();
    let mut id = first_id;
    // The entries in the document's order: a heading (or a paragraph of a style given), then the TC fields in it.
    struct Item {
        level: u32,
        text: String,
        para: u32,
        no_page: bool,
        heading: bool,
    }
    let mut items: Vec<Item> = Vec::new();
    for b in &doc.body {
        let Block::Paragraph(h) = b else { continue };
        if let Some(level) = resolve_paragraph_toc_level(styles, h, instruction) {
            let text: String = h.text().chars().filter(|c| !matches!(c, '\t' | '\u{b}' | '\u{c}' | '\u{e}' | '\u{fffc}')).collect();
            if !text.trim().is_empty() {
                // A numbered heading's number, as Word puts it in the entry.
                let text = match labels.get(&h.id).filter(|l| !l.is_empty()) {
                    Some(label) => format!("{label} {text}"),
                    None => text,
                };
                items.push(Item { level, text, para: h.id, no_page: false, heading: true });
            }
        }
        if let Some(wanted) = &instruction.tc_entries {
            for run in &h.runs {
                let FlowRunKind::Field { instruction: field, .. } = &run.kind else { continue };
                let Some(tc) = tc_spec(field) else { continue };
                if tc.id != *wanted || !(instruction.tc_levels.0..=instruction.tc_levels.1).contains(&tc.level) || tc.text.trim().is_empty() {
                    continue;
                }
                items.push(Item { level: tc.level, text: tc.text, para: h.id, no_page: tc.no_page || instruction.no_page_numbers, heading: false });
            }
        }
    }
    for item in items {
        let (level, text, h_id) = (item.level, item.text, item.para);
        let h = doc.body.iter().find_map(|b| match b { Block::Paragraph(p) if p.id == h_id => Some(p), _ => None }).expect("the paragraph");
        let _ = item.heading;
        let bookmark = match h.bookmarks.iter().find(|b| b.starts_with("_Toc")) {
            Some(b) => b.clone(),
            None => {
                let b = format!("_Toc{:09}", 100_000_000 + h.id);
                if !bookmarks.iter().any(|(p, _)| *p == h.id) {
                    bookmarks.push((h.id, b.clone()));
                }
                b
            }
        };
        let style = styles.paragraph_style_named(&format!("toc {level}")).map(str::to_string);
        let link = if instruction.hyperlinks {
            Some(Link::Anchor(bookmark.clone()))
        } else {
            None
        };
        let mut direct = crate::flow::ParaFormat { tabs: vec![TabChange { position: width, stop: Some(TabStop { position: width, align: TabAlign::Right, leader: true }) }], ..Default::default() };
        if style.is_none() {
            direct.left = Some((level as i64 - 1) * 210 * 635);
        }
        let mut entry = FlowParagraph { id, style, direct, ..FlowParagraph::default() };
        id += 1;
        let base = FlowRunStyle::default();
        entry.runs.push(FlowRun { kind: FlowRunKind::Text(text), style: base.clone(), format: RunFormat::default(), char_style: None, link: link.clone(), revision: None, show_field_code: false, field_locked: false, control: None });
        if !item.no_page {
            entry.runs.push(FlowRun { kind: FlowRunKind::Tab, style: base.clone(), format: RunFormat::default(), char_style: None, link: link.clone(), revision: None, show_field_code: false, field_locked: false, control: None });
            let shown = page_of(h.id).map_or(String::new(), |n| n.to_string());
            entry.runs.push(FlowRun { kind: FlowRunKind::PageRef { bookmark, shown }, style: base, format: RunFormat::default(), char_style: None, link, revision: None, show_field_code: false, field_locked: false, control: None });
        }
        entry.resolve(styles);
        entries.push(entry);
    }
    if entries.is_empty() {
        let mut p = FlowParagraph { id, ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(FlowRunKind::Text("目次の項目が見つかりません。".into()), FlowRunStyle::default()));
        p.resolve(styles);
        entries.push(p);
    }
    let n = entries.len();
    entries[0].field_start = Some(instruction.to_field_instruction());
    entries[n - 1].field_end = true;
    (entries, bookmarks)
}

/// 手動作成の目次 (Word's 手動作成の目次): placeholder entries of levels 1 to 3
/// in the `toc N` styles, to type over (no field, no links). The ids start at `first_id`;
/// `width` is the text's width (where the page numbers end).
pub fn manual_toc_entries(doc: &FlowDocument, first_id: u32, width: crate::geometry::Emu) -> Vec<FlowParagraph> {
    use crate::flow::{TabAlign, TabChange, TabStop};
    let styles = &doc.styles;
    let mut entries = Vec::new();
    for (i, (level, text, page)) in [(1u32, "章のタイトル（レベル 1）", "1"), (2, "章のタイトル（レベル 2）", "2"), (3, "章のタイトル（レベル 3）", "3")].into_iter().enumerate() {
        let style = styles.paragraph_style_named(&format!("toc {level}")).map(str::to_string);
        let mut direct = crate::flow::ParaFormat { tabs: vec![TabChange { position: width, stop: Some(TabStop { position: width, align: TabAlign::Right, leader: true }) }], ..Default::default() };
        if style.is_none() {
            direct.left = Some((level as i64 - 1) * 210 * 635);
        }
        let mut entry = FlowParagraph { id: first_id + i as u32, style, direct, ..FlowParagraph::default() };
        let base = FlowRunStyle::default();
        for kind in [FlowRunKind::Text(text.to_string()), FlowRunKind::Tab, FlowRunKind::Text(page.to_string())] {
            entry.runs.push(FlowRun { kind, style: base.clone(), format: RunFormat::default(), char_style: None, link: None, revision: None, show_field_code: false, field_locked: false, control: None });
        }
        entry.resolve(styles);
        entries.push(entry);
    }
    entries
}

/// A table of contents' entries for headings of levels `from` to `to` (the
/// instruction `TOC \o "from-to" \h \z \u`), with the pages `page_of` a
/// paragraph is on; and the bookmarks the headings need (`_Toc…`). The
/// entries' ids start at `first_id`; `width` is the text's width (where the
/// page numbers end).
pub fn toc_entries(doc: &FlowDocument, from: u32, to: u32, page_of: &dyn Fn(u32) -> Option<u32>, first_id: u32, width: crate::geometry::Emu) -> (Vec<FlowParagraph>, Vec<(u32, String)>) {
    let instruction = TocInstruction {
        heading_levels: Some((from, to)),
        custom_styles: Vec::new(),
        caption_label: None,
        ..Default::default()
    };
    toc_entries_with_instruction(doc, &instruction, page_of, first_id, width)
}

/// TOC フィールド命令から \c スイッチで指定されたラベル名を取得する（例: `TOC \h \z \c "図"` -> `Some("図")`）。
pub fn toc_caption_label(instruction: &str) -> Option<String> {
    let mut parts = instruction.split_whitespace();
    while let Some(part) = parts.next() {
        if part.eq_ignore_ascii_case("\\c") {
            if let Some(val) = parts.next() {
                let trimmed = val.trim_matches('"').trim_matches('\'');
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

/// A table of figures' missing style `table of figures`, when the document has none.
pub fn missing_table_of_figures_style(styles: &crate::flow::FlowStyles) -> Option<(String, crate::flow::FlowStyle)> {
    if styles.paragraph_style_named("table of figures").is_some() {
        return None;
    }
    let normal = styles.default_paragraph.clone();
    let mut id = "TableofFigures".to_string();
    while styles.styles.contains_key(&id) {
        id.push('_');
    }
    Some((id, crate::flow::FlowStyle {
        kind: crate::flow::StyleKind::Paragraph,
        name: "table of figures".into(),
        based_on: normal.clone(),
        next: normal,
        para: crate::flow::ParaFormat::default(),
        ..crate::flow::FlowStyle::default()
    }))
}

/// A table of figures' entries for captions with `label` (the
/// instruction `TOC \h \z \c "{label}"`), with the pages `page_of` a
/// paragraph is on; and the bookmarks the captions need (`_Ref…`). The
/// entries' ids start at `first_id`; `width` is the text's width (where the
/// page numbers end).
pub fn table_of_figures_entries(
    doc: &FlowDocument,
    label: &str,
    page_of: &dyn Fn(u32) -> Option<u32>,
    first_id: u32,
    width: crate::geometry::Emu,
) -> (Vec<FlowParagraph>, Vec<(u32, String)>) {
    use crate::flow::{Link, TabAlign, TabChange, TabStop};
    let styles = &doc.styles;
    let mut entries = Vec::new();
    let mut bookmarks = Vec::new();
    let mut id = first_id;
    let clean_label = label.trim();

    for b in &doc.body {
        let Block::Paragraph(p) = b else { continue };
        let has_seq = p.runs.iter().any(|r| {
            if let FlowRunKind::Field { instruction, .. } = &r.kind {
                let words = field_words(instruction);
                words.first().copied() == Some("SEQ") && words.get(1).map_or(false, |w| w.trim() == clean_label)
            } else {
                false
            }
        });
        if !has_seq {
            continue;
        }

        let text: String = p.text().chars().filter(|c| !matches!(c, '\t' | '\u{b}' | '\u{c}' | '\u{e}' | '\u{fffc}')).collect();
        if text.trim().is_empty() {
            continue;
        }

        let bookmark = match p.bookmarks.iter().find(|b| b.starts_with("_Ref")) {
            Some(b) => b.clone(),
            None => {
                let b = format!("_Ref{:09}", 100_000_000 + p.id);
                bookmarks.push((p.id, b.clone()));
                b
            }
        };

        let style = styles.paragraph_style_named("table of figures").map(str::to_string);
        let link = Some(Link::Anchor(bookmark.clone()));
        let direct = crate::flow::ParaFormat {
            tabs: vec![TabChange {
                position: width,
                stop: Some(TabStop { position: width, align: TabAlign::Right, leader: true }),
            }],
            ..Default::default()
        };
        let mut entry = FlowParagraph { id, style, direct, ..FlowParagraph::default() };
        id += 1;
        let base = FlowRunStyle::default();
        entry.runs.push(FlowRun { kind: FlowRunKind::Text(text), style: base.clone(), format: RunFormat::default(), char_style: None, link: link.clone(), revision: None, show_field_code: false, field_locked: false, control: None });
        entry.runs.push(FlowRun { kind: FlowRunKind::Tab, style: base.clone(), format: RunFormat::default(), char_style: None, link: link.clone(), revision: None, show_field_code: false, field_locked: false, control: None });
        let shown = page_of(p.id).map_or(String::new(), |n| n.to_string());
        entry.runs.push(FlowRun { kind: FlowRunKind::PageRef { bookmark, shown }, style: base, format: RunFormat::default(), char_style: None, link, revision: None, show_field_code: false, field_locked: false, control: None });
        entry.resolve(styles);
        entries.push(entry);
    }

    if entries.is_empty() {
        let mut p = FlowParagraph { id, ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(FlowRunKind::Text("図表目次の項目が見つかりません。".into()), FlowRunStyle::default()));
        p.resolve(styles);
        entries.push(p);
    }

    let n = entries.len();
    entries[0].field_start = Some(format!("TOC \\h \\z \\c \"{clean_label}\""));
    entries[n - 1].field_end = true;
    (entries, bookmarks)
}

/// 文書内（または指定された範囲 `range` 内）のフィールド（`FlowRunKind::Field`, `PageRef`, `PageNumber`, `FileName`）を
/// 静的なプレーンテキスト（`FlowRunKind::Text`）に変換する（Word の「フィールドのリンク解除」/ Unlink Fields、Ctrl+Shift+F9 / ⇧⌘F9）。
pub fn unlink_fields(doc: &FlowDocument, range: Option<(FlowPos, FlowPos)>) -> Vec<FlowCommand> {
    unlink_fields_at(doc, range, None)
}

/// [`unlink_fields`] と同じだが、`DATE` や `TIME` 等の日時フィールドについて最新の日時 `now` を反映可能。
pub fn unlink_fields_at(doc: &FlowDocument, range: Option<(FlowPos, FlowPos)>, now: Option<&crate::date_field::LocalTime>) -> Vec<FlowCommand> {
    let ps = all_paragraphs(doc);
    let order: Vec<u32> = ps.iter().map(|p| p.id).collect();
    let index = |id: u32| order.iter().position(|o| *o == id).unwrap_or(0);
    let mut commands = Vec::new();

    for p in ps {
        if !p.runs.iter().any(|r| is_field_kind(&r.kind)) {
            continue;
        }

        let i = index(p.id);
        let (lo, hi) = match range {
            None => (0, usize::MAX),
            Some((a, b)) => {
                if i < index(a.para) || i > index(b.para) {
                    continue;
                }
                (if i == index(a.para) { a.offset } else { 0 }, if i == index(b.para) { b.offset } else { usize::MAX })
            }
        };

        let mut q = p.clone();
        let mut runs = Vec::new();
        let mut at = 0;
        let mut changed = false;

        for mut run in std::mem::take(&mut q.runs) {
            let len = run.len();
            let inside = if lo == hi {
                at <= lo && lo <= at + len
            } else {
                at < hi && at + len > lo
            };
            at += len;

            if inside && is_field_kind(&run.kind) {
                if let Some(text) = field_static_text(&run.kind, doc.file_name.as_deref(), now) {
                    run.kind = FlowRunKind::Text(text);
                    changed = true;
                }
            }
            runs.push(run);
        }

        if changed {
            q.runs = runs;
            q.resolve(&doc.styles);
            q.normalize();
            if q != *p && let Some((container, idx)) = locate(doc, p.id) {
                commands.push(FlowCommand::ReplaceBlocks { container, index: idx, remove: 1, blocks: vec![Block::Paragraph(q)] });
            }
        }
    }

    commands
}

fn is_field_kind(kind: &FlowRunKind) -> bool {
    matches!(kind, FlowRunKind::Field { .. } | FlowRunKind::PageRef { .. } | FlowRunKind::FileName(_) | FlowRunKind::PageNumber)
}

fn field_static_text(kind: &FlowRunKind, file_name: Option<&str>, now: Option<&crate::date_field::LocalTime>) -> Option<String> {
    match kind {
        FlowRunKind::Field { instruction, shown } => {
            let t = instruction.trim();
            if (t.starts_with("DATE") || t.starts_with("TIME")) && let Some(now) = now {
                if let Some(formatted) = crate::date_field::date_field_text(t, now) {
                    return Some(formatted);
                }
            }
            if !shown.is_empty() {
                Some(shown.clone())
            } else if let Some((enclosure, text)) = crate::field_format::enclosed(instruction) {
                Some(format!("{enclosure}{text}"))
            } else if let Some((_, args)) = crate::field_format::overstrike(instruction) {
                Some(args.iter().map(|(t, _)| t.as_str()).collect())
            } else {
                Some(instruction.clone())
            }
        }
        FlowRunKind::PageRef { shown, .. } => Some(shown.clone()),
        FlowRunKind::FileName(s) => Some(file_name.map(str::to_string).unwrap_or_else(|| s.clone())),
        FlowRunKind::PageNumber => Some("1".to_string()),
        _ => None,
    }
}

/// 文書内（または指定された範囲 `range` 内）のフィールド（`FlowRunKind::Field`, `PageRef`, `PageNumber`, `FileName`）の
/// フィールドコードの表示/非表示（`show_field_code`）をトグルする（Word の「フィールドコードの表示/非表示」Shift+F9 / ⇧F9）。
pub fn toggle_field_codes_at(doc: &FlowDocument, range: Option<(FlowPos, FlowPos)>) -> Vec<FlowCommand> {
    let ps = all_paragraphs(doc);
    let order: Vec<u32> = ps.iter().map(|p| p.id).collect();
    let index = |id: u32| order.iter().position(|o| *o == id).unwrap_or(0);
    let mut commands = Vec::new();

    for p in ps {
        if !p.runs.iter().any(|r| is_field_kind(&r.kind)) {
            continue;
        }

        let i = index(p.id);
        let (lo, hi) = match range {
            None => (0, usize::MAX),
            Some((a, b)) => {
                if i < index(a.para) || i > index(b.para) {
                    continue;
                }
                (if i == index(a.para) { a.offset } else { 0 }, if i == index(b.para) { b.offset } else { usize::MAX })
            }
        };

        let mut q = p.clone();
        let mut runs = Vec::new();
        let mut at = 0;
        let mut changed = false;

        for mut run in std::mem::take(&mut q.runs) {
            let len = run.len();
            let inside = if lo == hi {
                at <= lo && lo <= at + len
            } else {
                at < hi && at + len > lo
            };
            at += len;

            if inside && is_field_kind(&run.kind) {
                run.show_field_code = !run.show_field_code;
                changed = true;
            }
            runs.push(run);
        }

        if changed {
            q.runs = runs;
            q.resolve(&doc.styles);
            q.normalize();
            if q != *p && let Some((container, idx)) = locate(doc, p.id) {
                commands.push(FlowCommand::ReplaceBlocks { container, index: idx, remove: 1, blocks: vec![Block::Paragraph(q)] });
            }
        }
    }

    commands
}

/// 文書全体のすべてのフィールドコードの表示/非表示（`show_field_codes`）をトグルする（Word の「すべてのフィールドコードの表示/非表示」Alt+F9 / ⌥F9）。
pub fn toggle_all_field_codes(doc: &FlowDocument) -> FlowCommand {
    FlowCommand::SetShowFieldCodes(!doc.show_field_codes)
}

/// コンテンツ コントロールのデザイン モードの有効/無効を切り替える（Word の「デザイン モード」開発 > デザイン モード）。
pub fn toggle_form_design_mode(doc: &FlowDocument) -> FlowCommand {
    FlowCommand::SetFormDesignMode(!doc.form_design_mode)
}

/// 文書内（または指定された範囲 `range` 内）のフィールド（`FlowRunKind::Field`, `PageRef`, `PageNumber`, `FileName`）の
/// ロック状態（`field_locked`）を設定・解除する（Word の「フィールドのロック」Ctrl+F11 / ⌘F11、「ロック解除」Ctrl+Shift+F11 / ⇧⌘F11）。
pub fn lock_fields_at(doc: &FlowDocument, range: Option<(FlowPos, FlowPos)>, locked: bool) -> Vec<FlowCommand> {
    let ps = all_paragraphs(doc);
    let order: Vec<u32> = ps.iter().map(|p| p.id).collect();
    let index = |id: u32| order.iter().position(|o| *o == id).unwrap_or(0);
    let mut commands = Vec::new();

    for p in ps {
        if !p.runs.iter().any(|r| is_field_kind(&r.kind)) {
            continue;
        }

        let i = index(p.id);
        let (lo, hi) = match range {
            None => (0, usize::MAX),
            Some((a, b)) => {
                if i < index(a.para) || i > index(b.para) {
                    continue;
                }
                (if i == index(a.para) { a.offset } else { 0 }, if i == index(b.para) { b.offset } else { usize::MAX })
            }
        };

        let mut q = p.clone();
        let mut runs = Vec::new();
        let mut at = 0;
        let mut changed = false;

        for mut run in std::mem::take(&mut q.runs) {
            let len = run.len();
            let inside = if lo == hi {
                at <= lo && lo <= at + len
            } else {
                at < hi && at + len > lo
            };
            at += len;

            if inside && is_field_kind(&run.kind) && run.field_locked != locked {
                run.field_locked = locked;
                changed = true;
            }
            runs.push(run);
        }

        if changed {
            q.runs = runs;
            q.resolve(&doc.styles);
            q.normalize();
            if q != *p && let Some((container, idx)) = locate(doc, p.id) {
                commands.push(FlowCommand::ReplaceBlocks { container, index: idx, remove: 1, blocks: vec![Block::Paragraph(q)] });
            }
        }
    }

    commands
}

fn paragraph_slice_text(p: &FlowParagraph, from: usize, to: usize) -> String {
    let mut out = String::new();
    let mut at = 0;
    for run in &p.runs {
        let len = run.len();
        let r_from = at;
        let r_to = at + len;
        if r_from < to && r_to > from {
            if let FlowRunKind::Text(t) = &run.kind {
                let c_start = from.saturating_sub(r_from);
                let c_end = (to - r_from).min(len);
                if c_start < c_end {
                    out.extend(t.chars().skip(c_start).take(c_end - c_start));
                }
            }
        }
        at += len;
    }
    out
}

/// フィールド命令（コード）から初期評価値を計算する。
pub fn evaluate_field_instruction(instruction: &str, doc: &FlowDocument, _pos: FlowPos) -> String {
    let expanded;
    let instruction = if instruction.contains('{') && instruction.contains('}') {
        expanded = crate::field_format::expand_nested_fields(instruction, |inner| {
            Some(evaluate_field_instruction(inner, doc, _pos))
        });
        &expanded
    } else {
        instruction
    };
    let words = field_words(instruction);
    let name = words.first().copied().unwrap_or("").to_ascii_uppercase();
    match name.as_str() {
        "PAGE" => "1".to_string(),
        "NUMPAGES" => "1".to_string(),
        "SECTION" => "1".to_string(),
        "SECTIONPAGES" => "1".to_string(),
        "FILENAME" => crate::field_format::apply_instruction_text_switches(doc.file_name.clone().unwrap_or_else(|| "Document.docx".to_string()), instruction),
        "DATE" | "TIME" => {
            let picture = if let Some(idx) = instruction.find("\\@") {
                instruction[idx + 2..].trim().trim_matches('"').trim()
            } else if name == "TIME" {
                "H:mm:ss"
            } else {
                "yyyy/M/d"
            };
            let now = crate::date_field::LocalTime { year: 2026, month: 10, day: 8, hour: 12, minute: 0, second: 0, weekday: 4 };
            crate::date_field::format_picture(&now, picture)
        }
        "CREATEDATE" => doc.created_date_text(instruction).unwrap_or_else(|| "2026/10/8".to_string()),
        "SAVEDATE" => doc.saved_date_text(instruction).unwrap_or_else(|| "2026/10/8".to_string()),
        "PRINTDATE" => doc.print_date_text(instruction).unwrap_or_else(|| "2026/10/8".to_string()),
        "SEQ" => {
            if words.len() >= 2 {
                "1".to_string()
            } else {
                "1".to_string()
            }
        }
        "LISTNUM" => {
            let (_, level, start) = parse_listnum(instruction);
            let n = start.unwrap_or(1);
            crate::field_format::format_listnum(n, level, instruction)
        }
        "AUTONUM" | "AUTONUMLGL" | "AUTONUMOUT" => {
            crate::field_format::format_autonum(1, name.as_str(), instruction)
        }
        "FILESIZE" => doc.file_size_text(instruction),
        "NOTEREF" => {
            if words.contains(&"\\p") {
                if let Some(bm) = words.get(1) {
                    for p in all_paragraphs(doc) {
                        if p.bookmarks.iter().any(|b| b.eq_ignore_ascii_case(bm)) {
                            return relative_paragraph_position(doc, _pos.para, p.id).to_string();
                        }
                    }
                }
                "上".to_string()
            } else {
                doc.noteref_text(instruction).map(|v| crate::field_format::apply_instruction_text_switches(v, instruction)).unwrap_or_else(|| "1".to_string())
            }
        }
        "STYLEREF" => crate::field_format::apply_instruction_text_switches(doc.styleref_text(instruction).unwrap_or_default(), instruction),
        "REF" => {
            if let Some(bm) = words.get(1) {
                for p in all_paragraphs(doc) {
                    if p.bookmarks.iter().any(|b| b.eq_ignore_ascii_case(bm)) {
                        if words.contains(&"\\p") {
                            return relative_paragraph_position(doc, _pos.para, p.id).to_string();
                        } else if words.contains(&"\\r") {
                            let labels = crate::flow::list_labels(doc);
                            let val = labels.get(&p.id).cloned().unwrap_or_else(|| "1".to_string());
                            return crate::field_format::apply_instruction_text_switches(val, instruction);
                        } else {
                            let val = reference_text(p, &std::collections::HashMap::new());
                            return crate::field_format::apply_instruction_text_switches(val, instruction);
                        }
                    }
                }
                // もし段落ブックマークにない場合、SET / ASK フィールドで設定された変数も探す
                for p in all_paragraphs(doc) {
                    for r in &p.runs {
                        if let FlowRunKind::Field { instruction: set_instr, .. } = &r.kind {
                            if let Some((name, val)) = crate::field_format::parse_set_field(set_instr)
                                .or_else(|| crate::field_format::parse_ask_field(set_instr))
                            {
                                if name.eq_ignore_ascii_case(bm) {
                                    return crate::field_format::apply_instruction_text_switches(val, instruction);
                                }
                            }
                        }
                    }
                }
                crate::field_format::apply_instruction_text_switches(bm.to_string(), instruction)
            } else {
                String::new()
            }
        }
        "PAGEREF" => {
            if words.contains(&"\\p") {
                if let Some(bm) = words.get(1) {
                    for p in all_paragraphs(doc) {
                        if p.bookmarks.iter().any(|b| b.eq_ignore_ascii_case(bm)) {
                            return relative_paragraph_position(doc, _pos.para, p.id).to_string();
                        }
                    }
                }
                "上".to_string()
            } else {
                "1".to_string()
            }
        }
        "AUTHOR" => crate::field_format::apply_instruction_text_switches(doc.author_text(instruction), instruction),
        "TITLE" => crate::field_format::apply_instruction_text_switches(doc.title_text(instruction), instruction),
        "SUBJECT" => crate::field_format::apply_instruction_text_switches(doc.subject_text(instruction), instruction),
        "COMMENTS" => crate::field_format::apply_instruction_text_switches(doc.comments_text(instruction), instruction),
        "DOCPROPERTY" => crate::field_format::apply_instruction_text_switches(doc.doc_property_text(instruction).unwrap_or_default(), instruction),
        "DOCVARIABLE" => doc.doc_variable_text(instruction),
        "USERNAME" => doc.user_name_text(instruction),
        "USERINITIALS" => doc.user_initials_text(instruction),
        "USERADDRESS" => doc.user_address_text(instruction),
        "QUOTE" => doc.quote_text(instruction),
        "SYMBOL" => doc.symbol_text(instruction),
        "SET" => String::new(),
        "IF" => crate::field_format::if_field_text(instruction).unwrap_or_default(),
        "COMPARE" => crate::field_format::compare_field_text(instruction).unwrap_or_default(),
        "HYPERLINK" => doc.hyperlink_text(instruction, ""),
        "TEMPLATE" => doc.template_text(instruction),
        "ADVANCE" => doc.advance_text(instruction),
        "INFO" => doc.info_text(instruction).unwrap_or_default(),
        "KEYWORDS" | "LASTSAVEDBY" | "NUMCHARS" | "NUMWORDS" | "REVNUM" => doc.bare_info_text(instruction).unwrap_or_default(),
        "ASK" => String::new(),
        "FILLIN" | "FILL-IN" => doc.fillin_text(instruction),
        "AUTOTEXT" => crate::field_format::format_autotext(instruction),
        "AUTOTEXTLIST" => crate::field_format::format_autotextlist(instruction),
        "CITATION" => crate::field_format::format_citation(instruction),
        "BIBLIOGRAPHY" => crate::field_format::format_bibliography(instruction),
        _ => {
            if instruction.trim_start().starts_with('=') {
                if let Some((table, row, col)) = find_table_cell_for_paragraph(doc, _pos.para) {
                    let resolver = FlowTableCellResolver { table, row, col };
                    crate::field_format::formula_text_with_resolver(instruction, Some(&resolver)).unwrap_or_default()
                } else {
                    crate::field_format::formula_text(instruction).unwrap_or_default()
                }
            } else {
                String::new()
            }
        }
    }
}

/// 選択範囲をフィールドに変換、またはキャレット位置にフィールドを挿入する（Word の「フィールドの挿入」Ctrl+F9 / ⌘F9）。
/// - 選択範囲がある場合: その範囲を置き換えてフィールドを挿入。`instruction` が空なら選択テキストをコードとする。
/// - 選択範囲がない場合: キャレット位置にフィールドを挿入。
/// `show_code` が true の場合、個別の `show_field_code` を true に設定（Ctrl+F9 後のコード編集状態）。
pub fn insert_field_at_range(
    doc: &FlowDocument,
    range: Option<(FlowPos, FlowPos)>,
    instruction: &str,
    show_code: bool,
) -> Option<(Vec<FlowCommand>, FlowPos)> {
    let (start, end) = range.unwrap_or_else(|| {
        let p_id = doc.body.first().and_then(|b| match b {
            Block::Paragraph(p) => Some(p.id),
            _ => None,
        }).unwrap_or(0);
        (FlowPos::new(p_id, 0), FlowPos::new(p_id, 0))
    });

    let (container, index) = locate(doc, start.para)?;
    let list = blocks(doc, container)?;
    let Block::Paragraph(p) = &list[index] else { return None };

    let lo = start.offset.min(p.len());
    let hi = if start.para == end.para { end.offset.min(p.len()) } else { lo };

    let mut instr = instruction.trim().to_string();
    if instr.is_empty() && lo < hi {
        instr = paragraph_slice_text(p, lo, hi).trim().to_string();
    }

    let (format, char_style, style) = p.format_at(lo);
    let shown = evaluate_field_instruction(&instr, doc, FlowPos::new(p.id, lo));
    let kind = FlowRunKind::Field { instruction: instr, shown };
    let field_run = FlowRun {
        kind,
        style,
        format,
        char_style,
        link: None,
        revision: None,
        show_field_code: show_code,
        field_locked: false, control: None,
    };

    let mut q = FlowParagraph { runs: p.runs_before(lo), ..p.clone() };
    q.runs.push(field_run);
    q.runs.extend(p.runs_after(hi));
    q.resolve(&doc.styles);
    q.normalize();

    let new_pos = FlowPos::new(p.id, lo + 1);
    Some((
        vec![FlowCommand::ReplaceBlocks {
            container,
            index,
            remove: 1,
            blocks: vec![Block::Paragraph(q)],
        }],
        new_pos,
    ))
}

/// 指定位置 `pos` にあるフィールドの命令（コード）を更新し、結果を再計算する（Word の「フィールドの編集…」）。
pub fn edit_field_at(doc: &FlowDocument, pos: FlowPos, new_instruction: &str) -> Option<Vec<FlowCommand>> {
    let (container, index) = locate(doc, pos.para)?;
    let list = blocks(doc, container)?;
    let Block::Paragraph(p) = &list[index] else { return None };

    let mut q = p.clone();
    let mut runs = Vec::new();
    let mut at = 0;
    let mut changed = false;

    for mut run in std::mem::take(&mut q.runs) {
        let len = run.len();
        let inside = at <= pos.offset && pos.offset <= at + len;
        at += len;

        if inside && !changed {
            match &mut run.kind {
                FlowRunKind::Field { instruction, shown } => {
                    *instruction = new_instruction.trim().to_string();
                    *shown = evaluate_field_instruction(new_instruction, doc, pos);
                    changed = true;
                }
                FlowRunKind::PageNumber | FlowRunKind::FileName(_) | FlowRunKind::PageRef { .. } => {
                    let shown = evaluate_field_instruction(new_instruction, doc, pos);
                    run.kind = FlowRunKind::Field {
                        instruction: new_instruction.trim().to_string(),
                        shown,
                    };
                    changed = true;
                }
                _ => {}
            }
        }
        runs.push(run);
    }

    if changed {
        q.runs = runs;
        q.resolve(&doc.styles);
        q.normalize();
        Some(vec![FlowCommand::ReplaceBlocks {
            container,
            index,
            remove: 1,
            blocks: vec![Block::Paragraph(q)],
        }])
    } else {
        None
    }
}

/// 指定位置 `pos` にあるフィールドの現在のコード（`instruction`）を取得する。
pub fn get_field_at(doc: &FlowDocument, pos: FlowPos) -> Option<String> {
    let (container, index) = locate(doc, pos.para)?;
    let list = blocks(doc, container)?;
    let Block::Paragraph(p) = &list[index] else { return None };

    let mut at = 0;
    for run in &p.runs {
        let len = run.len();
        if at <= pos.offset && pos.offset <= at + len {
            if let Some(code) = run.field_code() {
                return Some(code);
            }
        }
        at += len;
    }
    None
}

/// セル内のテキストから数値をパースする（Word の表計算仕様）。
/// 括弧負数（`(100)` -> -100）、通貨記号（`¥`, `$`, `€`, `£`）、カンマ、パーセント（`%`）を処理する。
pub fn parse_cell_number(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let (is_paren_neg, t) = if t.starts_with('(') && t.ends_with(')') {
        (true, &t[1..t.len() - 1])
    } else {
        (false, t)
    };
    let is_percent = t.ends_with('%');
    let t = if is_percent { &t[..t.len() - 1] } else { t };
    let cleaned: String = t
        .chars()
        .filter(|&c| c != ',' && c != ' ' && c != '\u{a0}' && c != '¥' && c != '$' && c != '€' && c != '£')
        .collect();
    let mut val: f64 = cleaned.parse().ok()?;
    if is_percent {
        val /= 100.0;
    }
    if is_paren_neg {
        val = -val;
    }
    Some(val)
}

/// セル内の全ブロックからプレーンテキストを抽出する。
pub fn flow_cell_text(cell: &FlowCell) -> String {
    let mut out = String::new();
    for block in &cell.blocks {
        match block {
            Block::Paragraph(p) => {
                for run in &p.runs {
                    match &run.kind {
                        FlowRunKind::Text(t) => out.push_str(t),
                        FlowRunKind::Field { shown, .. } => out.push_str(shown),
                        FlowRunKind::Tab | FlowRunKind::PositionalTab { .. } => out.push('\t'),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// グリッド座標 (row_idx, col_idx) からセルを取得する。
pub fn get_table_cell_at(table: &FlowTable, row_idx: usize, col_idx: usize) -> Option<&FlowCell> {
    let row = table.rows.get(row_idx)?;
    let mut current_col = 0;
    for cell in &row.cells {
        let span = cell.span.max(1) as usize;
        if col_idx >= current_col && col_idx < current_col + span {
            return Some(cell);
        }
        current_col += span;
    }
    None
}

/// 指定した段落 `para_id` が含まれるテーブル、行インデックス、列インデックス（グリッド列）を探索する。
pub fn find_table_cell_for_paragraph(doc: &FlowDocument, para_id: u32) -> Option<(&FlowTable, usize, usize)> {
    fn search_blocks<'a>(blocks: &'a [Block], para_id: u32) -> Option<(&'a FlowTable, usize, usize)> {
        for b in blocks {
            match b {
                Block::Table(t) => {
                    for (r_idx, row) in t.rows.iter().enumerate() {
                        let mut col_start = 0;
                        for cell in &row.cells {
                            let span = cell.span.max(1) as usize;
                            for cell_block in &cell.blocks {
                                match cell_block {
                                    Block::Paragraph(p) if p.id == para_id => {
                                        return Some((t, r_idx, col_start));
                                    }
                                    Block::Table(_) => {
                                        if let Some(res) = search_blocks(&cell.blocks, para_id) {
                                            return Some(res);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            col_start += span;
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    search_blocks(&doc.body, para_id)
        .or_else(|| search_blocks(&doc.header, para_id))
        .or_else(|| search_blocks(&doc.footer, para_id))
        .or_else(|| search_blocks(&doc.header_first, para_id))
        .or_else(|| search_blocks(&doc.footer_first, para_id))
        .or_else(|| search_blocks(&doc.header_even, para_id))
        .or_else(|| search_blocks(&doc.footer_even, para_id))
}

/// `FlowTable` 上の計算式セル値解決器。
pub struct FlowTableCellResolver<'a> {
    pub table: &'a FlowTable,
    pub row: usize,
    pub col: usize,
}

impl<'a> crate::field_format::TableCellResolver for FlowTableCellResolver<'a> {
    fn resolve_direction(&self, dir: &str) -> Vec<f64> {
        let mut out = Vec::new();
        match dir {
            "ABOVE" => {
                let mut r = self.row;
                while r > 0 {
                    r -= 1;
                    if let Some(cell) = get_table_cell_at(self.table, r, self.col) {
                        let text = flow_cell_text(cell);
                        if let Some(val) = parse_cell_number(&text) {
                            out.push(val);
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                out.reverse();
            }
            "BELOW" => {
                let mut r = self.row + 1;
                while r < self.table.rows.len() {
                    if let Some(cell) = get_table_cell_at(self.table, r, self.col) {
                        let text = flow_cell_text(cell);
                        if let Some(val) = parse_cell_number(&text) {
                            out.push(val);
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                    r += 1;
                }
            }
            "LEFT" => {
                let mut c = self.col;
                while c > 0 {
                    c -= 1;
                    if let Some(cell) = get_table_cell_at(self.table, self.row, c) {
                        let text = flow_cell_text(cell);
                        if let Some(val) = parse_cell_number(&text) {
                            out.push(val);
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                out.reverse();
            }
            "RIGHT" => {
                let num_cols = self.table.columns.len().max(
                    self.table.rows.iter().map(|r| r.cells.iter().map(|c| c.span.max(1) as usize).sum()).max().unwrap_or(0)
                );
                let mut c = self.col + 1;
                while c < num_cols {
                    if let Some(cell) = get_table_cell_at(self.table, self.row, c) {
                        let text = flow_cell_text(cell);
                        if let Some(val) = parse_cell_number(&text) {
                            out.push(val);
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                    c += 1;
                }
            }
            _ => {}
        }
        out
    }

    fn resolve_reference(&self, reference: &str) -> Vec<f64> {
        let parse_cell_coord = |s: &str| -> Option<(usize, usize)> {
            let s = s.trim();
            let col_end = s.find(|c: char| c.is_ascii_digit())?;
            let col_str = &s[..col_end];
            let row_str = &s[col_end..];
            let row: usize = row_str.parse::<usize>().ok()?.checked_sub(1)?;
            let mut col = 0usize;
            for ch in col_str.chars() {
                if !ch.is_ascii_alphabetic() { return None; }
                col = col * 26 + (ch.to_ascii_uppercase() as usize - 'A' as usize + 1);
            }
            let col = col.checked_sub(1)?;
            Some((row, col))
        };

        if let Some((start_str, end_str)) = reference.split_once(':') {
            let Some((r1, c1)) = parse_cell_coord(start_str) else { return Vec::new() };
            let Some((r2, c2)) = parse_cell_coord(end_str) else { return Vec::new() };
            let (min_r, max_r) = (r1.min(r2), r1.max(r2));
            let (min_c, max_c) = (c1.min(c2), c1.max(c2));
            let mut out = Vec::new();
            for r in min_r..=max_r {
                for c in min_c..=max_c {
                    if let Some(cell) = get_table_cell_at(self.table, r, c) {
                        let text = flow_cell_text(cell);
                        if let Some(val) = parse_cell_number(&text) {
                            out.push(val);
                        }
                    }
                }
            }
            out
        } else if let Some((r, c)) = parse_cell_coord(reference) {
            if let Some(cell) = get_table_cell_at(self.table, r, c) {
                let text = flow_cell_text(cell);
                if let Some(val) = parse_cell_number(&text) {
                    return vec![val];
                }
            }
            Vec::new()
        } else {
            Vec::new()
        }
    }
}

/// The next `relative_height` (`wp:anchor/@relativeHeight`) for a newly inserted floating shape/text box,
/// strictly higher than all existing floating shapes across all stories of `doc` (at least `1`).
fn next_floating_relative_height(doc: &FlowDocument) -> u32 {
    let mut max_rh = 0u32;
    for p in all_paragraphs_everywhere(doc) {
        for r in &p.runs {
            if let FlowRunKind::Shape(shape) = &r.kind
                && let Some(floating) = &shape.floating
            {
                max_rh = max_rh.max(floating.relative_height);
            }
        }
    }
    max_rh.saturating_add(1).max(1)
}

/// An empty centred text body for a newly inserted preset shape (`InsertShape`), ready for in-place text editing.
fn centred_shape_text_body(font_size: u32) -> crate::text::TextBody {
    let style = crate::text::TextStyle {
        font_size: font_size.max(100),
        color: Rgb::WHITE,
        align: crate::text::TextAlign::Center,
        ..Default::default()
    };
    let mut body = crate::text::TextBody {
        anchor: crate::text::VerticalAnchor::Middle,
        insets: crate::text::Insets::default(),
        wrap: true,
        clip: true,
        ..crate::text::TextBody::plain("", style)
    };
    if let Some(p) = body.paragraphs.first_mut() {
        p.align = Some(crate::text::TextAlign::Center);
    }
    body
}

#[cfg(test)]
mod floating_zorder_and_shape_text_tests {
    use super::*;
    use crate::geometry::EmuSize;

    #[test]
    fn inserted_floating_shapes_have_strictly_increasing_relative_height() {
        let mut doc = FlowDocument::default();
        if doc.body.is_empty() {
            doc.body.push(Block::Paragraph(FlowParagraph::default()));
        }
        let para_id = match doc.body.first() {
            Some(Block::Paragraph(p)) => p.id,
            _ => panic!("expected default paragraph"),
        };
        apply_to(
            &mut doc,
            FlowCommand::InsertTextBox {
                at: FlowPos::new(para_id, 0),
                size: EmuSize::new(1_000_000, 500_000),
                text: "First".to_string(),
            },
        )
        .unwrap();
        apply_to(
            &mut doc,
            FlowCommand::InsertShape {
                at: FlowPos::new(para_id, 1),
                size: EmuSize::new(1_000_000, 500_000),
                geometry: crate::drawing::Geometry::Rect,
                fill: Some(Rgb(0x4472C4)),
                line: Some((Rgb(0x2F528F), 12_700)),
            },
        )
        .unwrap();
        apply_to(
            &mut doc,
            FlowCommand::InsertTextBox {
                at: FlowPos::new(para_id, 2),
                size: EmuSize::new(1_000_000, 500_000),
                text: "Third".to_string(),
            },
        )
        .unwrap();

        let Block::Paragraph(p) = &doc.body[0] else { panic!("expected paragraph") };
        let heights: Vec<u32> = p
            .runs
            .iter()
            .filter_map(|r| match &r.kind {
                FlowRunKind::Shape(s) => s.floating.as_ref().map(|f| f.relative_height),
                _ => None,
            })
            .collect();
        assert_eq!(heights.len(), 3);
        assert!(heights[0] < heights[1], "second shape must have higher relative_height than first: {:?}", heights);
        assert!(heights[1] < heights[2], "third shape must have higher relative_height than second: {:?}", heights);

        // Verify that InsertShape created a centred TextBody ready for in-place editing.
        let FlowRunKind::Shape(second_shape) = &p.runs[1].kind else { panic!("expected shape") };
        let body = second_shape.text_body().expect("InsertShape should attach a TextBody");
        assert_eq!(body.anchor, crate::text::VerticalAnchor::Middle);
        assert_eq!(body.style.align, crate::text::TextAlign::Center);
    }
}





