// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The panels the fix担当 adds to the document editor, kept in one field of
//! its state (`State::helper`): ページ設定 for now.
//!
//! 表示形式 > ページ設定 > ページ設定… (Word's ページ設定 dialog): the caret's
//! section's margins (top, bottom, left or inside, right or outside),
//! とじしろ and its place (left or top), 見開きページ, the header's and
//! footer's distance from the edges, the paper's width and height, and the
//! body's vertical alignment, in millimetres.
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::PageVAlign;

use crate::shell::chrome::{SidebarPanel, radio_button, radio_group, RadioGroupLayout};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

#[derive(Default)]
pub(crate) struct HelpPanels {
    pub(crate) page_setup: Option<PageSetupPanel>,
    pub(crate) notes: Option<NotesPanel>,
    pub(crate) bookmarks: Option<BookmarkPanel>,
    pub(crate) watermark: Option<WatermarkPanel>,
    pub(crate) new_style: Option<super::style_new::NewStylePanel>,
    pub(crate) word_count: Option<WordCountPanel>,
    pub(crate) field: Option<FieldPanel>,
    pub(crate) caption: Option<CaptionPanel>,
    pub(crate) properties: Option<DocPropertiesPanel>,
    pub(crate) cross_reference: Option<CrossReferencePanel>,
    pub(crate) update_toc: Option<UpdateTocPanel>,
    pub(crate) user_info: Option<UserInfoPanel>,
    pub(crate) kinsoku: Option<super::kinsoku_panel::KinsokuPanel>,
    pub(crate) go_to: Option<super::goto::GoToPanel>,
    pub(crate) spelling: Option<super::spelling_panel::SpellPanel>,
    /// データの編集 of a chart inserted in the document: the panel and where the chart is.
    pub(crate) chart: Option<(crate::pptx::chart_panel::ChartPanel, ibis_doc_model::FlowPos)>,
}

/// 挿入 > 目次 > 目次の更新… (Word's 目次の更新 Update Table of Contents dialog):
/// ページ番号だけを更新するか、目次全体を更新するかを選択。
pub(crate) struct UpdateTocPanel {
    pub(crate) kind: UpdateTocKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum UpdateTocKind {
    /// ページ番号だけを更新する (Update page numbers only)
    #[default]
    PageNumbersOnly,
    /// 目次をすべて更新する (Update entire table)
    EntireTable,
}

/// 挿入 > 図表番号… (Word's 図表番号 dialog):
/// ラベル選択・新規ラベル、番号書式、ラベル除外、キャプション文字列の入力。
pub(crate) struct CaptionPanel {
    pub custom_label: bool,
    pub(crate) label: Entity<InputState>,
    pub(crate) text: Entity<InputState>,
    pub(crate) format: ibis_doc_model::CaptionNumberFormat,
    pub(crate) exclude_label: bool,
    /// 章番号を含める: the heading level and the separator (an index of [`CAPTION_SEPARATORS`]).
    pub(crate) chapter: Option<(u32, usize)>,
    pub(crate) presets: Vec<String>,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

/// The separators after the chapter number: Word's ハイフン・ピリオド・コロン・長いダッシュ・短いダッシュ.
pub(crate) const CAPTION_SEPARATORS: [(&str, &str); 5] = [("_docx.hyphen", "-"), ("_docx.period", "."), ("_docx.colon", ":"), ("_docx.em-dash", "\u{2014}"), ("_docx.en-dash", "\u{2013}")];

pub(crate) const CAPTION_FORMATS: [(ibis_doc_model::CaptionNumberFormat, &str); 5] = [
    (ibis_doc_model::CaptionNumberFormat::Arabic, "1, 2, 3, …"),
    (ibis_doc_model::CaptionNumberFormat::UpperRoman, "I, II, III, …"),
    (ibis_doc_model::CaptionNumberFormat::LowerRoman, "i, ii, iii, …"),
    (ibis_doc_model::CaptionNumberFormat::UpperLetter, "A, B, C, …"),
    (ibis_doc_model::CaptionNumberFormat::LowerLetter, "a, b, c, …"),
];

/// 挿入 > フィールド… / フィールドの編集… (Word's フィールド dialog):
/// プリセットの選択、コード入力、挿入または更新。
pub(crate) struct FieldPanel {
    pub(crate) list_scroll: ScrollHandle,
    pub query: Entity<InputState>,
    _query_subscription: Subscription,
    pub(crate) code: Entity<InputState>,
    pub(crate) is_edit: bool,
    pub(crate) target_pos: ibis_doc_model::FlowPos,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

pub(crate) const FIELD_PRESETS: [(&str, &str); 37] = [
    ("PAGE", "docx.field.page"),
    ("NUMPAGES", "docx.field.numpages"),
    ("DATE", "docx.field.date"),
    ("TIME", "docx.field.time"),
    ("CREATEDATE", "docx.field.createdate"),
    ("SAVEDATE", "docx.field.savedate"),
    ("FILENAME", "docx.field.filename"),
    ("FILESIZE \\k", "docx.field.filesize"),
    ("DOCVARIABLE \"VariableName\"", "docx.field.docvariable"),
    ("USERNAME", "docx.field.username"),
    ("USERINITIALS", "docx.field.userinitials"),
    ("QUOTE \"Text\"", "docx.field.quote"),
    ("SYMBOL 65", "docx.field.symbol"),
    ("SET Bookmark \"Text\"", "docx.field.set"),
    ("ASK Bookmark \"Prompt\" \\d \"Default\"", "docx.field.ask"),
    ("FILLIN \"Prompt\" \\d \"Default\"", "docx.field.fillin"),
    ("HYPERLINK \"https://www.example.com\"", "docx.field.hyperlink"),
    ("TEMPLATE", "docx.field.template"),
    ("ADVANCE \\d 5", "docx.field.advance"),
    ("INFO Author", "docx.field.info"),
    ("= { PAGE } + 1", "docx.field.nested-page-calc"),
    ("AUTHOR", "docx.field.author"),
    ("TITLE", "docx.field.title"),
    ("SUBJECT", "docx.field.subject"),
    ("DOCPROPERTY \"Author\"", "docx.field.docprop-author"),
    ("DOCPROPERTY \"Title\"", "docx.field.docprop-title"),
    ("_docx.caption-seq-figure", "docx.field.seq-figure"),
    ("LISTNUM Numbering_Default \\l 1", "docx.field.listnum"),
    ("AUTONUM", "docx.field.autonum"),
    ("AUTONUMOUT", "docx.field.autonumout"),
    ("PAGEREF", "docx.field.pageref"),
    ("NOTEREF", "docx.field.noteref"),
    ("STYLEREF \"Heading 1\"", "_docx.heading-reference-heading-1"),
    ("IF 1 = 1 \"True\" \"False\"", "docx.field.if"),
    ("COMPARE 1 = 1", "docx.field.compare"),
    ("TOC \\o \"1-3\" \\h \\z \\u", "docx.field.toc"),
    ("TOC \\o \"1-3\" \\t \"Custom,1\" \\h \\z", "docx.field.toc-custom"),
];

/// デザイン > 透かし… (Word's ユーザー設定の透かし, its テキスト): the text
/// (or one of Word's), the font, the colour, 半透明 and the layout.
pub(crate) struct WatermarkPanel {
    pub custom_text: bool,
    pub(crate) text: Entity<InputState>,
    pub(crate) font: Entity<InputState>,
    pub(crate) color: u32,
    pub(crate) semitransparent: bool,
    pub(crate) diagonal: bool,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

/// Word's texts for a watermark.
const WATERMARK_TEXTS: [&str; 6] = ["fix.help-panels.confidential", "fix.help-panels.do-not-copy", "fix.help-panels.asap", "fix.help-panels.draft", "fix.help-panels.sample", "fix.help-panels.urgent"];
/// The colours offered: (name, RGB).
const WATERMARK_COLORS: [(&str, u32); 5] = [("fix.help-panels.silver", 0xC0C0C0), ("_shell.gray", 0x808080), ("_shell.black", 0x000000), ("_shell.red", 0xFF0000), ("_shell.blue", 0x0070C0)];

/// 挿入 > ブックマーク… (Word's dialog): a name to add at the caret's
/// paragraph, the document's bookmarks (the hidden ones, `_Toc…`, if
/// asked) to jump to or delete.
pub(crate) struct BookmarkPanel {
    pub(crate) name: Entity<InputState>,
    pub(crate) hidden: bool,
    /// The bookmark chosen in the list.
    pub(crate) chosen: Option<String>,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

/// Whether `name` is a bookmark's name as Word takes one: a letter first,
/// then letters, digits and `_`, at most 40 characters.
pub(crate) fn bookmark_name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(char::is_alphabetic) && name.chars().count() <= 40 && name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// The document's bookmarks: (name, paragraph), in the order of the body.
fn bookmarks(doc: &ibis_doc_model::FlowDocument) -> Vec<(String, u32)> {
    fn walk(blocks: &[ibis_doc_model::Block], out: &mut Vec<(String, u32)>) {
        for b in blocks {
            match b {
                ibis_doc_model::Block::Paragraph(p) => out.extend(p.bookmarks.iter().map(|n| (n.clone(), p.id))),
                ibis_doc_model::Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    walk(&doc.body, &mut out);
    out
}

/// 脚注と文末脚注 (Word's dialog): which kind, where, the numbers' format,
/// the first number and when they start again; for the whole document.
pub(crate) struct NotesPanel {
    pub(crate) endnotes: bool,
    pub(crate) footnotes: ibis_doc_model::NoteSettings,
    pub(crate) endnote_settings: ibis_doc_model::NoteSettings,
    pub(crate) start: Entity<InputState>,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

const FORMAT_NAMES: [&str; 8] = ["1, 2, 3, …", "I, II, III, …", "i, ii, iii, …", "A, B, C, …", "a, b, c, …", "*, †, ‡, §, …", "①, ②, ③, …", "１, ２, ３, …"]; // i18n-ok（番号書式の見本（全角数字の見本）。迷った：英語の画面でも同じ見本のまま。訳さない）

/// The fields, in millimetres: top, bottom, left, right, gutter, header,
/// footer, width, height.
pub(crate) const FIELDS: [&str; 9] = ["_shell.top", "_shell.bottom", "fix.help-panels.left-inside", "fix.help-panels.right-outside", "fix.help-panels.gutter", "_shell.header", "_shell.footer", "_shell.width", "_shell.height"];

pub(crate) struct PageSetupPanel {
    pub(crate) fields: [Entity<InputState>; 9],
    pub(crate) gutter_at_top: bool,
    pub(crate) mirror: bool,
    pub(crate) multiple_pages: ibis_doc_model::MultiplePages,
    pub(crate) book_fold_sheets: Entity<InputState>,
    pub(crate) v_align: PageVAlign,
    /// 文字列の方向: across, or 縦書き (`TbRl`).
    pub(crate) vertical: bool,
    /// The page number the section starts at (blank: it goes on from the
    /// section before).
    pub(crate) start: Entity<InputState>,
    /// 文字数と行数（原稿設定）
    pub(crate) grid_type: ibis_doc_model::DocGridType,
    pub(crate) lines: Entity<InputState>,
    pub(crate) line_pitch: Entity<InputState>,
    pub(crate) chars: Entity<InputState>,
    pub(crate) char_space: Entity<InputState>,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

const EMU_PER_MM: f64 = 36_000.0;

fn mm(emu: i64) -> String {
    let v = (emu as f64 / EMU_PER_MM * 10.0).round() / 10.0;
    if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

fn pt_str(emu: i64) -> String {
    let v = (emu as f64 / ibis_doc_model::EMU_PER_POINT as f64 * 10.0).round() / 10.0;
    if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

impl SlideEditor {
    pub(crate) fn open_page_setup_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(page) = self.caret_page() else { return };
        let values = [page.top, page.bottom, page.left, page.right, page.gutter, page.header, page.footer, page.size.width, page.size.height].map(mm);
        let fields = values.map(|v| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(v, window, cx));
            state
        });
        let start = cx.new(|cx| InputState::new(window, cx));
        let restart = self.flow().and_then(|d| ibis_doc_model::flow_command::section_setup(d, self.caret_section())).is_some_and(|s| s.restart_numbering);
        if restart || self.caret_section() == 0 && page.first_page != 1 {
            start.update(cx, |i, cx| i.set_value(page.first_page.to_string(), window, cx));
        }

        let is_vert = page.direction == ibis_doc_model::CellDirection::TbRl;
        let text_h = if is_vert {
            page.size.width - page.left - page.right - if !page.gutter_at_top { page.gutter } else { 0 }
        } else {
            page.size.height - page.top - page.bottom - if page.gutter_at_top { page.gutter } else { 0 }
        }.max(1);
        let text_w = if is_vert {
            page.size.height - page.top - page.bottom - if page.gutter_at_top { page.gutter } else { 0 }
        } else {
            page.size.width - page.left - page.right - if !page.gutter_at_top { page.gutter } else { 0 }
        }.max(1);

        let default_pitch = ibis_doc_model::EMU_PER_POINT * 18;
        let pitch = page.line_pitch.unwrap_or(default_pitch);
        let lines_val = (text_h / pitch.max(1)).max(1);

        let lines = cx.new(|cx| InputState::new(window, cx));
        lines.update(cx, |i, cx| i.set_value(lines_val.to_string(), window, cx));

        let line_pitch = cx.new(|cx| InputState::new(window, cx));
        line_pitch.update(cx, |i, cx| i.set_value(pt_str(pitch), window, cx));

        let default_char_pitch = (ibis_doc_model::EMU_PER_POINT as f64 * 10.5).round() as i64;
        let char_pitch = page.char_space.map(|s| default_char_pitch + s).unwrap_or(default_char_pitch);
        let chars_val = (text_w / char_pitch.max(1)).max(1);

        let chars = cx.new(|cx| InputState::new(window, cx));
        chars.update(cx, |i, cx| i.set_value(chars_val.to_string(), window, cx));

        let char_space = cx.new(|cx| InputState::new(window, cx));
        char_space.update(cx, |i, cx| i.set_value(pt_str(page.char_space.unwrap_or(0)), window, cx));

        let grid_type = if page.doc_grid_type == ibis_doc_model::DocGridType::Default && page.line_pitch.is_some() {
            ibis_doc_model::DocGridType::Lines
        } else {
            page.doc_grid_type
        };

        let panel = PageSetupPanel {
            fields,
            gutter_at_top: page.gutter_at_top,
            mirror: page.is_mirrored(),
            multiple_pages: page.multiple_pages,
            book_fold_sheets: {
                let state = cx.new(|cx| InputState::new(window, cx));
                if let Some(s) = page.book_fold_sheets {
                    state.update(cx, |i, cx| i.set_value(s.to_string(), window, cx));
                }
                state
            },
            v_align: page.v_align,
            vertical: page.direction == ibis_doc_model::CellDirection::TbRl,
            start,
            grid_type,
            lines,
            line_pitch,
            chars,
            char_space,
            message: String::new(),
            ui: Default::default(),
        };
        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = HelpPanels::default();
            s.helper.page_setup = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    fn page_setup_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.page_setup.as_mut()) {
            p.message = message.to_string();
        }
        cx.notify();
    }

    pub(crate) fn apply_page_setup_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.page_setup.as_ref()) else { return };
        let mut v = [0i64; 9];
        for (i, f) in p.fields.iter().enumerate() {
            match f.read(cx).value().trim().parse::<f64>() {
                Ok(x) if x >= 0.0 && x.is_finite() => v[i] = (x * EMU_PER_MM).round() as i64,
                _ => return self.page_setup_message(crate::shell::i18n::format("page-setup.bad", &[("field", &text(FIELDS[i]))]).into(), cx),
            }
        }
        let [top, bottom, left, right, gutter, header, footer, width, height] = v;
        // The text area must keep some room (Word refuses margins that meet).
        let across = if p.gutter_at_top { left + right } else { left + right + gutter };
        let down = if p.gutter_at_top { top + bottom + gutter } else { top + bottom };
        if width <= 0 || height <= 0 || across >= width - 36_000 * 10 || down >= height - 36_000 * 10 {
            return self.page_setup_message(text("fix.help-panels.margins-too-large-paper"), cx);
        }
        let (gutter_at_top, mirror, multiple_pages, v_align, vertical) = (p.gutter_at_top, p.mirror, p.multiple_pages, p.v_align, p.vertical);
        let book_fold_sheets_str = p.book_fold_sheets.read(cx).value().trim().to_string();
        let book_fold_sheets = if book_fold_sheets_str.is_empty() || book_fold_sheets_str == "0" {
            None
        } else {
            match book_fold_sheets_str.parse::<u32>() {
                Ok(n) if n >= 1 => Some(n),
                _ => return self.page_setup_message(text("docx.help-panels.enter-whole-number-1-more"), cx),
            }
        };
        // The first page number: a number, or blank for going on.
        let start = p.start.read(cx).value().trim().to_string();
        let start = if start.is_empty() {
            None
        } else {
            match start.parse::<u32>() {
                Ok(n) if n >= 1 => Some(n),
                _ => return self.page_setup_message(text("docx.help-panels.enter-whole-number-1-more"), cx),
            }
        };

        let grid_type = p.grid_type;
        let (line_pitch, char_space) = match grid_type {
            ibis_doc_model::DocGridType::Default => (None, None),
            ibis_doc_model::DocGridType::Lines => {
                let pitch_str = p.line_pitch.read(cx).value().trim().to_string();
                let pitch = match pitch_str.parse::<f64>() {
                    Ok(pt) if pt > 0.0 && pt.is_finite() => (pt * ibis_doc_model::EMU_PER_POINT as f64).round() as i64,
                    _ => {
                        let lines_str = p.lines.read(cx).value().trim().to_string();
                        match lines_str.parse::<u32>() {
                            Ok(n) if n >= 1 => {
                                let is_vert = vertical;
                                let text_h = if is_vert {
                                    width - left - right - if !gutter_at_top { gutter } else { 0 }
                                } else {
                                    height - top - bottom - if gutter_at_top { gutter } else { 0 }
                                }.max(1);
                                (text_h / n as i64).max(ibis_doc_model::EMU_PER_POINT)
                            }
                            _ => return self.page_setup_message(text("docx.help-panels.please-enter-positive-number-line"), cx),
                        }
                    }
                };
                (Some(pitch), None)
            }
            ibis_doc_model::DocGridType::LinesAndChars => {
                let pitch_str = p.line_pitch.read(cx).value().trim().to_string();
                let pitch = match pitch_str.parse::<f64>() {
                    Ok(pt) if pt > 0.0 && pt.is_finite() => (pt * ibis_doc_model::EMU_PER_POINT as f64).round() as i64,
                    _ => {
                        let lines_str = p.lines.read(cx).value().trim().to_string();
                        match lines_str.parse::<u32>() {
                            Ok(n) if n >= 1 => {
                                let is_vert = vertical;
                                let text_h = if is_vert {
                                    width - left - right - if !gutter_at_top { gutter } else { 0 }
                                } else {
                                    height - top - bottom - if gutter_at_top { gutter } else { 0 }
                                }.max(1);
                                (text_h / n as i64).max(ibis_doc_model::EMU_PER_POINT)
                            }
                            _ => return self.page_setup_message(text("docx.help-panels.please-enter-positive-number-line"), cx),
                        }
                    }
                };
                let space_str = p.char_space.read(cx).value().trim().to_string();
                let space = match space_str.parse::<f64>() {
                    Ok(pt) if pt >= 0.0 && pt.is_finite() => (pt * ibis_doc_model::EMU_PER_POINT as f64).round() as i64,
                    _ => 0,
                };
                (Some(pitch), Some(space))
            }
            ibis_doc_model::DocGridType::SnapToChars => (None, None),
        };

        self.set_page(
            |page| {
                page.top = top;
                page.bottom = bottom;
                page.left = left;
                page.right = right;
                page.gutter = gutter;
                page.header = header;
                page.footer = footer;
                page.size = ibis_doc_model::EmuSize::new(width, height);
                page.gutter_at_top = gutter_at_top;
                page.multiple_pages = multiple_pages;
                page.book_fold_sheets = book_fold_sheets;
                page.mirror_margins = mirror || multiple_pages.is_mirrored();
                page.v_align = v_align;
                page.doc_grid_type = grid_type;
                page.line_pitch = line_pitch;
                page.char_space = char_space;
                // A direction the panel does not offer is kept.
                if vertical != (page.direction == ibis_doc_model::CellDirection::TbRl) {
                    page.direction = if vertical { ibis_doc_model::CellDirection::TbRl } else { ibis_doc_model::CellDirection::LrTb };
                }
            },
            cx,
        );
        let current_restart = self.flow().and_then(|d| ibis_doc_model::flow_command::section_setup(d, self.caret_section())).is_some_and(|s| s.restart_numbering);
        let current_start = self.caret_page().map(|p| p.first_page).unwrap_or(1);
        let need_section_change = match start {
            Some(n) => !current_restart || current_start != n,
            None => current_restart,
        };
        if need_section_change {
            self.set_section(
                move |s| match start {
                    Some(n) => {
                        s.restart_numbering = true;
                        s.page.first_page = n;
                    }
                    None => {
                        if s.restart_numbering {
                            s.restart_numbering = false;
                            s.page.first_page = 1;
                        }
                    }
                },
                cx,
            );
        }
        self.page_setup_message(text("fix.help-panels.page-setup-changed"), cx);
    }

    pub(crate) fn doc_page_setup_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.page_setup.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        type Set = fn(&mut PageSetupPanel);
        let choice = |id: &'static str, on: bool, label: &str, set: Set| {
            mark(
                id,
                crate::shell::chrome::radio_button(id, on, false, text(label), cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.page_setup.as_mut()) {
                            set(p);
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        // Every field here is a number; the panel applies them with its button, so ▲▼ only change the number.
        use crate::shell::chrome::SpinnerSpec;
        let field_cx = &*cx;
        let aligned_row = |mark_id: &str, label_text: SharedString, input: &Entity<InputState>, unit_text: SharedString, spec: SpinnerSpec| {
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().text_sm().child(label_text))
                .child(mark(mark_id,crate::shell::chrome::text_field(input).spinner(format!("page-setup-{mark_id}"),spec,field_cx,|_,_,_,_|{}).unit(unit_text,px(72.)).into_any_element()))
        };
        let field = |i: usize| aligned_row(&format!("field-{i}"), text(FIELDS[i]), &p.fields[i], "mm".into(), SpinnerSpec::new(1.0).range(if i >= 7 { 1.0 } else { 0.0 }, 558.0));
        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().children((!p.message.is_empty()).then(||div().text_sm().child(SharedString::from(p.message.clone()))))
            .child(div().text_sm().child(text("_shell.margins")));
        for i in 0..5 {
            body = body.child(field(i));
        }
        body = body
            .child(v_flex().gap_1().child(div().text_sm().child(text("fix.help-panels.gutter-position"))).child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow).child(choice("gutter-left", !p.gutter_at_top, "_shell.left", |p| p.gutter_at_top = false)).child(choice("gutter-top", p.gutter_at_top, "_shell.top", |p| p.gutter_at_top = true))))
            .child(div().text_sm().child(text("docx.page.multiple-pages")))
            .child(
                crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                    .child(choice("page-normal", p.multiple_pages == ibis_doc_model::MultiplePages::Normal, "docx.page.multiple-pages.normal", |p| {
                        p.multiple_pages = ibis_doc_model::MultiplePages::Normal;
                        p.mirror = false;
                    }))
                    .child(choice("page-mirror", p.multiple_pages == ibis_doc_model::MultiplePages::MirrorMargins, "docx.page.multiple-pages.mirror-margins", |p| {
                        p.multiple_pages = ibis_doc_model::MultiplePages::MirrorMargins;
                        p.mirror = true;
                    }))
                    .child(choice("page-book-fold", p.multiple_pages == ibis_doc_model::MultiplePages::BookFold, "docx.page.multiple-pages.book-fold", |p| {
                        p.multiple_pages = ibis_doc_model::MultiplePages::BookFold;
                        p.mirror = true;
                    }))
                    .child(choice("page-reverse-book-fold", p.multiple_pages == ibis_doc_model::MultiplePages::ReverseBookFold, "docx.page.multiple-pages.reverse-book-fold", |p| {
                        p.multiple_pages = ibis_doc_model::MultiplePages::ReverseBookFold;
                        p.mirror = true;
                    }))
                    .child(choice("page-two-on-one", p.multiple_pages == ibis_doc_model::MultiplePages::TwoOnOne, "docx.page.multiple-pages.two-on-one", |p| {
                        p.multiple_pages = ibis_doc_model::MultiplePages::TwoOnOne;
                        p.mirror = false;
                    })),
            );
        if p.multiple_pages.is_book_fold() {
            body = body.child(aligned_row(
                "book-fold-sheets",
                text("docx.page.book-fold-sheets"),
                &p.book_fold_sheets,
                text("docx.help-panels.book-fold-sheets-hint"),
                SpinnerSpec::new(4.0).range(0.0, 1024.0).integer_only(),
            ));
        }
        body = body
            .child(div().text_sm().child(text("docx.help-panels.document-grid")))
            .child(
                crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                    .child(choice("grid-default", p.grid_type == ibis_doc_model::DocGridType::Default, "docx.help-panels.no-grid", |p| p.grid_type = ibis_doc_model::DocGridType::Default))
                    .child(choice("grid-lines", p.grid_type == ibis_doc_model::DocGridType::Lines, "docx.help-panels.lines-only", |p| p.grid_type = ibis_doc_model::DocGridType::Lines))
                    .child(choice("grid-lines-chars", p.grid_type == ibis_doc_model::DocGridType::LinesAndChars, "docx.help-panels.document-grid", |p| p.grid_type = ibis_doc_model::DocGridType::LinesAndChars)),
            )
            .child(aligned_row("grid-lines-input", text("docx.help-panels.lines"), &p.lines, text("_xlsx.row"), SpinnerSpec::new(1.0).range(1.0, 999.0).integer_only()))
            .child(aligned_row("grid-line-pitch", text("docx.help-panels.line-pitch"), &p.line_pitch, "pt".into(), SpinnerSpec::new(0.5).range(0.0, 1584.0)))
            .child(aligned_row("grid-chars", text("docx.help-panels.characters"), &p.chars, text("docx.paragraph-panel.characters"), SpinnerSpec::new(1.0).range(1.0, 999.0).integer_only()))
            .child(aligned_row("grid-char-space", text("docx.help-panels.character-pitch"), &p.char_space, "pt".into(), SpinnerSpec::new(0.5).range(0.0, 1584.0)));
        body = body.child(div().text_sm().child(text("_shell.vertical-alignment"))).child(
            h_flex()
                .gap_1()
                .flex_wrap()
                .child(choice("valign-top", p.v_align == PageVAlign::Top, "fix.help-panels.top", |p| p.v_align = PageVAlign::Top))
                .child(choice("valign-center", p.v_align == PageVAlign::Center, "fix.help-panels.center", |p| p.v_align = PageVAlign::Center))
                .child(choice("valign-bottom", p.v_align == PageVAlign::Bottom, "fix.help-panels.bottom", |p| p.v_align = PageVAlign::Bottom))
                .child(choice("valign-both", p.v_align == PageVAlign::Both, "_shell.justify", |p| p.v_align = PageVAlign::Both)),
        );
        body = body
            .child(div().text_sm().child(text("docx.chrome.text-direction")))
            .child(
                crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                    .child(choice("direction-across", !p.vertical, "_shell.horizontal", |p| p.vertical = false))
                    .child(choice("direction-vertical", p.vertical, "_shell.vertical", |p| p.vertical = true)),
            )
            .child(div().text_sm().child(text("_xlsx.page-number")))
            .child(aligned_row("start", text("fix.help-panels.start"), &p.start, text("docx.help-panels.blank-continue"), SpinnerSpec::new(1.0).range(1.0, 32767.0).integer_only()));
        body = body.child(div().text_sm().child(text("_shell.paper-size")));
        for i in 7..9 {
            body = body.child(field(i));
        }
        body = body.child(div().text_sm().child(text("fix.help-panels.header-footer-edge")));
        for i in 5..7 {
            body = body.child(field(i));
        }
        let footer = crate::shell::chrome::action_bar()
            .action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("page-setup-apply", text("OK"), crate::shell::chrome::ActionButtonKind::Primary)
                    .on_click(cx.listener(|this, _, _, cx| this.apply_page_setup_panel(cx)))))
            .action(mark_action("close", crate::shell::chrome::ActionDefinition::new("page-setup-close-target", text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(s) = this.doc_state_mut() {
                            s.helper.page_setup = None;
                        }
                        this.sidebar_open = false;
                        cx.notify();
                    }))));
        Some(SidebarPanel::with_footer(
            crate::shell::i18n::text("_shell.page-setup"),
            body.into_any_element(),
            footer,
        ))
    }
}

impl SlideEditor {
    pub(crate) fn open_notes_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(page) = self.caret_page() else { return };
        let start = cx.new(|cx| InputState::new(window, cx));
        let first = page.footnotes.start.to_string();
        start.update(cx, |i, cx| i.set_value(first, window, cx));
        let panel = NotesPanel { endnotes: false, footnotes: page.footnotes, endnote_settings: page.endnotes, start, message: String::new(), ui: Default::default() };
        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = HelpPanels::default();
            s.helper.notes = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The panel's settings for the kind shown, set by `edit`.
    pub(crate) fn edit_notes_panel(&mut self, edit: impl FnOnce(&mut ibis_doc_model::NoteSettings), window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.notes.as_mut()) {
            let set = if p.endnotes { &mut p.endnote_settings } else { &mut p.footnotes };
            edit(set);
        }
        let _ = window;
        cx.notify();
    }

    /// Switch the panel between footnotes and endnotes (its first number
    /// shown for the kind).
    fn switch_notes_panel(&mut self, endnotes: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state_mut().and_then(|s| s.helper.notes.as_mut()) else { return };
        p.endnotes = endnotes;
        let start = (if endnotes { p.endnote_settings.start } else { p.footnotes.start }).to_string();
        let field = p.start.clone();
        field.update(cx, |i, cx| i.set_value(start, window, cx));
        cx.notify();
    }

    /// 適用: the settings for every section (Word's 文書全体).
    pub(crate) fn apply_notes_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.notes.as_ref()) else { return };
        let Ok(start) = p.start.read(cx).value().trim().parse::<u32>() else {
            return self.notes_message(text("fix.help-panels.enter-whole-number-1-more"), cx);
        };
        if start == 0 {
            return self.notes_message(text("fix.help-panels.enter-whole-number-1-more"), cx);
        }
        let (mut foot, mut end) = (p.footnotes, p.endnote_settings);
        if p.endnotes {
            end.start = start;
        } else {
            foot.start = start;
        }
        let Some(doc) = self.flow() else { return };
        let mut state = ibis_doc_model::flow_command::section_state(doc, &ibis_doc_model::flow_command::section_paragraphs(doc));
        state.page.footnotes = foot;
        state.page.endnotes = end;
        for (_, _, setup) in state.breaks.iter_mut() {
            if let Some(setup) = setup {
                let setup = std::sync::Arc::make_mut(setup);
                setup.page.footnotes = foot;
                setup.page.endnotes = end;
            }
        }
        self.run_flow(vec![ibis_doc_model::FlowCommand::SetSections { state: Box::new(state) }], cx);
        self.notes_message(text("fix.help-panels.footnote-endnote-settings-changed"), cx);
    }

    fn notes_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.notes.as_mut()) {
            p.message = message.to_string();
        }
        cx.notify();
    }

    pub(crate) fn notes_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
        use ibis_doc_model::{NoteFormat, NotePosition, NoteRestart};
        let p = self.doc_state()?.helper.notes.as_ref()?;
        let set = if p.endnotes { p.endnote_settings } else { p.footnotes };
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let radio = |id: String, on: bool, label: &str, f: Box<dyn Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>)>| {
            let f = std::rc::Rc::new(f);
            mark(
                &id,
                crate::shell::chrome::radio_button(SharedString::from(id.clone()), on, false, text(label),
                    cx.listener(move |this, _, window, cx| f(this, window, cx)))
                    .into_any_element(),
            )
        };
        let entity = cx.entity().downgrade();
        let format_index = NoteFormat::ALL.iter().position(|f| *f == set.format).unwrap_or(0);
        let mut formats=crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow);
        for (i,&format) in NoteFormat::ALL.iter().enumerate() {
            formats=formats.child(crate::shell::chrome::radio_button(("notes-format",i),set.format==format,false,FORMAT_NAMES[i],cx.listener(move|e,_,w,cx|e.edit_notes_panel(|s|s.format=format,w,cx))));
        }
        let positions: Vec<(NotePosition, &str)> = if p.endnotes {
            vec![(NotePosition::SectionEnd, "fix.help-panels.end-section"), (NotePosition::DocumentEnd, "fix.help-panels.end-document")]
        } else {
            vec![(NotePosition::PageBottom, "fix.help-panels.bottom-page"), (NotePosition::BeneathText, "fix.help-panels.below-text")]
        };
        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().children((!p.message.is_empty()).then(||div().text_sm().child(SharedString::from(p.message.clone()))))

            .child(
                crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                    .child(radio("kind-foot".into(), !p.endnotes, "fix.goto.footnote", Box::new(|this, w, cx| this.switch_notes_panel(false, w, cx))))
                    .child(radio("kind-end".into(), p.endnotes, "fix.goto.endnote", Box::new(|this, w, cx| this.switch_notes_panel(true, w, cx)))),
            )
            .child(div().text_sm().child(text("fix.help-panels.location")));
        let mut places = crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow);
        for (i, (position, label)) in positions.into_iter().enumerate() {
            places = places.child(radio(format!("pos-{i}"), set.position == position, label, Box::new(move |this, w, cx| this.edit_notes_panel(|s| s.position = position, w, cx))));
        }
        body = body
            .child(places)
            .child(h_flex().items_center().gap_2().child(div().w(px(92.)).text_sm().child(text("fix.help-panels.number-format"))).child(div().flex_1().min_w_0().child(formats)))
            // The first note number: applied by the panel's button, so ▲▼ only change the number.
            .child(h_flex().items_center().gap_2().child(div().w(px(92.)).text_sm().child(text("fix.help-panels.start"))).child(mark("start", crate::shell::chrome::text_field(&p.start).spinner("notes-start", crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, 32767.0).integer_only(), cx, |_, _, _, _| {}).into_any_element())))
            .child(div().text_sm().child(text("fix.help-panels.numbering")));
        let mut restarts = crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow);
        for (i, (restart, label)) in [(NoteRestart::Continuous, "fix.help-panels.continuous"), (NoteRestart::EachSection, "fix.help-panels.restart-each-section"), (NoteRestart::EachPage, "fix.help-panels.restart-each-page")].into_iter().enumerate() {
            restarts = restarts.child(radio(format!("restart-{i}"), set.restart == restart, label, Box::new(move |this, w, cx| this.edit_notes_panel(|s| s.restart = restart, w, cx))));
        }
        body = body.child(restarts);
        let footer = crate::shell::chrome::action_bar()
                    .action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("notes-apply", text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|this, _, _, cx| this.apply_notes_panel(cx)))))

            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer("fix.help-panels.footnote-endnote", body.into_any_element(), footer))
    }
}

impl SlideEditor {
    pub(crate) fn open_bookmark_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(text("fix.help-panels.bookmark-name")));
        name.update(cx, |i, cx| i.focus(window, cx));
        let panel = BookmarkPanel { name, hidden: false, chosen: None, message: String::new(), ui: Default::default() };
        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = HelpPanels::default();
            s.helper.bookmarks = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    fn bookmark_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.bookmarks.as_mut()) {
            p.message = message.to_string();
        }
        cx.notify();
    }

    /// 追加: the name at the caret's paragraph (moved there if another has it).
    pub(crate) fn add_bookmark(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.bookmarks.as_ref()) else { return };
        let name = p.name.read(cx).value().trim().to_string();
        if !bookmark_name_ok(&name) {
            return self.bookmark_message(text("fix.help-panels.bookmark-name-must-begin-letter"), cx);
        }
        let (Some((start, _)), Some(doc)) = (self.ordered_selection(), self.flow()) else { return };
        let mut commands = Vec::new();
        for (n, para) in bookmarks(doc) {
            if n == name && para != start.para {
                commands.push(ibis_doc_model::FlowCommand::SetParagraphFormat { first: para, last: para, change: ibis_doc_model::ParaChange::RemoveBookmark(name.clone()) });
            }
        }
        commands.push(ibis_doc_model::FlowCommand::SetParagraphFormat { first: start.para, last: start.para, change: ibis_doc_model::ParaChange::AddBookmark(name.clone()) });
        self.run_flow(commands, cx);
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.bookmarks.as_mut()) {
            p.chosen = Some(name);
        }
        self.bookmark_message(text("fix.help-panels.bookmark-added"), cx);
    }

    /// 削除: the chosen bookmark taken off.
    pub(crate) fn delete_bookmark(&mut self, cx: &mut Context<Self>) {
        let Some(name) = self.doc_state().and_then(|s| s.helper.bookmarks.as_ref()).and_then(|p| p.chosen.clone()) else { return };
        let Some(doc) = self.flow() else { return };
        let commands: Vec<_> = bookmarks(doc).into_iter().filter(|(n, _)| *n == name).map(|(_, para)| ibis_doc_model::FlowCommand::SetParagraphFormat { first: para, last: para, change: ibis_doc_model::ParaChange::RemoveBookmark(name.clone()) }).collect();
        if commands.is_empty() {
            return;
        }
        self.run_flow(commands, cx);
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.bookmarks.as_mut()) {
            p.chosen = None;
        }
        self.bookmark_message(text("fix.help-panels.bookmark-deleted"), cx);
    }

    /// ジャンプ: the caret to the chosen bookmark's paragraph.
    pub(crate) fn jump_to_bookmark(&mut self, cx: &mut Context<Self>) {
        let Some(name) = self.doc_state().and_then(|s| s.helper.bookmarks.as_ref()).and_then(|p| p.chosen.clone()) else { return };
        let Some(para) = self.flow().and_then(|d| bookmarks(d).into_iter().find(|(n, _)| *n == name).map(|(_, p)| p)) else { return };
        self.set_caret(ibis_doc_model::FlowPos::new(para, 0), false, cx);
        self.scroll_to_caret(cx);
        cx.notify();
    }

    pub(crate) fn bookmark_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.bookmarks.as_ref()?;
        let all = self.flow().map(|d| bookmarks(d)).unwrap_or_default();
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(div().text_sm().child(text("fix.help-panels.bookmark-name")))
            .child(mark("name", crate::shell::chrome::text_field(&p.name).full_width().into_any_element()))
            .children((!p.message.is_empty()).then(||div().text_sm().child(SharedString::from(p.message.clone()))))

            .child(mark(
                "hidden",
                Checkbox::new("bookmark-hidden").checked(p.hidden).label(text("fix.help-panels.hidden-bookmarks"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.bookmarks.as_mut()) {
                            p.hidden = !p.hidden;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        for (name, _) in all.into_iter().filter(|(n, _)| p.hidden || !n.starts_with('_')) {
            let on = p.chosen.as_deref() == Some(name.as_str());
            let n2 = name.clone();
            body = body.child(mark(
                &format!("item-{name}"),
                crate::shell::chrome::action_button(SharedString::from(format!("bookmark-item-{name}")))
                    .small()

                    .label(format!("{} {name}", if on { "▸" } else { " " }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.bookmarks.as_mut()) {
                            p.chosen = Some(n2.clone());
                            let field = p.name.clone();
                            let n3 = n2.clone();
                            field.update(cx, |i, cx| i.set_value(n3, window, cx));
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        }
        let footer = crate::shell::chrome::action_bar()
                    .action(mark_action("add", crate::shell::chrome::ActionDefinition::new("bookmark-add", text("fix.common.add"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|this, _, _, cx| this.add_bookmark(cx)))))
                    .action(mark_action("jump", crate::shell::chrome::ActionDefinition::new("bookmark-jump", text("_xlsx.go-to"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|this, _, _, cx| this.jump_to_bookmark(cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx))
                    .action(mark_action("delete", crate::shell::chrome::ActionDefinition::new("bookmark-delete", text("_shell.delete"), crate::shell::chrome::ActionButtonKind::Destructive).on_click(cx.listener(|this, _, _, cx| this.delete_bookmark(cx)))));
        Some(SidebarPanel::with_footer("fix.help-panels.bookmark", body.into_any_element(), footer))
    }
}

impl SlideEditor {
    pub(crate) fn open_watermark_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let current = self.flow().and_then(|d| d.watermark.clone());
        let english = crate::shell::i18n::language() == crate::shell::i18n::Language::English;
        let first = current.as_ref().map_or_else(|| text(WATERMARK_TEXTS[0]).to_string(), |w| w.text.clone());
        let font_name = current.as_ref().map_or_else(|| if english { "Calibri".to_string() } else { "游ゴシック".to_string() }, |w| w.font.clone()); // i18n-ok（既定のフォント名（文書に保存される）。英語の画面でも Calibri の分岐のみ）
        let field = cx.new(|cx| InputState::new(window, cx));
        field.update(cx, |i, cx| i.set_value(first, window, cx));
        let font = cx.new(|cx| InputState::new(window, cx));
        font.update(cx, |i, cx| i.set_value(font_name, window, cx));
        let custom_text=!WATERMARK_TEXTS.iter().any(|k|text(k).as_ref()==field.read(cx).value().as_ref());
        let panel = WatermarkPanel {
            custom_text,
            text: field,
            font,
            color: current.as_ref().map_or(0xC0C0C0, |w| w.color.0),
            semitransparent: current.as_ref().is_none_or(|w| w.semitransparent),
            diagonal: current.as_ref().is_none_or(|w| w.diagonal()),
            ui: Default::default(),
        };
        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = HelpPanels::default();
            s.helper.watermark = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 適用: the panel's watermark on the document (a header made for it if
    /// the document has none).
    pub(crate) fn apply_watermark(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.watermark.as_ref()) else { return };
        let words = p.text.read(cx).value().trim().to_string();
        if words.is_empty() {
            return self.remove_watermark(cx);
        }
        let watermark = ibis_doc_model::Watermark { text: words, font: p.font.read(cx).value().trim().to_string(), color: ibis_doc_model::Rgb(p.color), semitransparent: p.semitransparent, rotation: if p.diagonal { 315 } else { 0 }, size: None };
        let Some(doc) = self.flow() else { return };
        if doc.watermark.as_ref() == Some(&watermark) && doc.section_watermarks.is_empty() {
            return;
        }
        let mut commands = Vec::new();
        if doc.header.is_empty() && ibis_doc_model::flow_command::resolved_part(doc, 0, ibis_doc_model::PartKind::Header).is_empty() {
            commands.extend(self.add_header_commands(false));
        }
        commands.push(ibis_doc_model::FlowCommand::SetWatermark { watermark: Some(watermark), sections: Vec::new() });
        self.run_flow(commands, cx);
    }

    /// 透かしの削除 (Word's 透かしなし).
    pub(crate) fn remove_watermark(&mut self, cx: &mut Context<Self>) {
        if self.flow().is_some_and(|d| d.watermark.is_some() || d.section_watermarks.iter().any(|(_, w)| w.is_some())) {
            self.run_flow(vec![ibis_doc_model::FlowCommand::SetWatermark { watermark: None, sections: Vec::new() }], cx);
        }
    }

    pub(crate) fn watermark_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.watermark.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let mut presets=radio_group().layout(RadioGroupLayout::Flow)
            .child(mark("custom",radio_button("watermark-custom",p.custom_text,false,text("ui.custom"),cx.listener(|e,_,_,cx|{if let Some(p)=e.doc_state_mut().and_then(|s|s.helper.watermark.as_mut()){p.custom_text=true;}cx.notify();})).into_any_element()));
        for (i,key) in WATERMARK_TEXTS.iter().copied().enumerate() {
            let on=!p.custom_text && p.text.read(cx).value().as_ref()==text(key).as_ref();
            presets=presets.child(mark(&format!("text-{i}"),radio_button(("watermark-preset",i),on,false,text(key),cx.listener(move |e,_,w,cx|{if let Some(p)=e.doc_state_mut().and_then(|s|s.helper.watermark.as_mut()){p.custom_text=false;p.text.update(cx,|i,cx|i.set_value(text(key),w,cx));}cx.notify();})).into_any_element()));
        }
        let mut colors = radio_group().layout(RadioGroupLayout::Flow);
        for (i, (name, value)) in WATERMARK_COLORS.iter().enumerate() {
            let value = *value;
            colors = colors.child(mark(
                &format!("color-{i}"),
                crate::shell::chrome::color_radio_button(SharedString::from(format!("watermark-color-{i}")), p.color == value, value, text(name), cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.watermark.as_mut()) {
                            p.color = value;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        }
        let toggle = |id: &'static str, on: bool, label: &'static str, set: fn(&mut WatermarkPanel)| {
            radio_button(SharedString::from(format!("watermark-{id}")), on, false, text(label),
                cx.listener(move |this, _, _, cx| {
                    if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.watermark.as_mut()) { set(p); }
                    cx.notify();
                })).into_any_element()
        };
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))

            .p_0().child(div().text_sm().child(text("_shell.text")))
            .child(presets)
            .child(mark("text", crate::shell::chrome::text_field(&p.text).full_width().disabled(!p.custom_text).into_any_element()))
            .child(h_flex().items_center().gap_2().child(div().w(px(64.)).text_sm().child(text("_shell.font"))).child(mark("font",crate::shell::chrome::panel_font_dropdown("panel-font-watermark",&p.font,cx))))
            .child(div().text_sm().child(text("docx.common.color")))
            .child(colors)
            .child(mark("semi", Checkbox::new("watermark-semi").checked(p.semitransparent).label(text("fix.help-panels.semitransparent"))
                .on_click(cx.listener(|this, on: &bool, _, cx| {
                    if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.watermark.as_mut()) { p.semitransparent = *on; }
                    cx.notify();
                })).into_any_element()))
            .child(div().text_sm().child(text("_shell.layout")))
            .child(
                radio_group().layout(RadioGroupLayout::Flow)
                    .child(mark("diagonal", toggle("diagonal", p.diagonal, "fix.help-panels.diagonal", |p| p.diagonal = true)))
                    .child(mark("horizontal", toggle("horizontal", !p.diagonal, "fix.print.horizontal", |p| p.diagonal = false))),
            );
        let footer = crate::shell::chrome::action_bar()
                    .action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("watermark-apply", text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|this, _, _, cx| this.apply_watermark(cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx))
                    .action(mark_action("remove", crate::shell::chrome::ActionDefinition::new("watermark-remove", text("fix.help-panels.remove-watermark"), crate::shell::chrome::ActionButtonKind::Destructive).on_click(cx.listener(|this, _, _, cx| this.remove_watermark(cx)))));
        Some(SidebarPanel::with_footer("fix.help-panels.watermark", body.into_any_element(), footer))
    }
}

/// Statistics of a document or selection (Word's 文字カウント dialog).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DocStatistics {
    pub(crate) is_selection: bool,
    pub(crate) pages: usize,
    pub(crate) words: usize,
    pub(crate) characters_no_spaces: usize,
    pub(crate) characters_with_spaces: usize,
    pub(crate) paragraphs: usize,
    pub(crate) lines: usize,
}

/// Word counts each CJK character as one word.
pub(crate) fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
        | '\u{3040}'..='\u{309F}' // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
        | '\u{3400}'..='\u{4DBF}' // CJK Unified Ideographs Extension A
        | '\u{4E00}'..='\u{9FFF}' // CJK Unified Ideographs
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
        | '\u{AC00}'..='\u{D7AF}' // Hangul Syllables
        | '\u{FF01}'..='\u{FF60}' // Fullwidth Forms
        | '\u{FF61}'..='\u{FF9F}' // Halfwidth Katakana
    )
}

/// Count (words, characters without whitespace, characters with spaces) in `text`.
pub(crate) fn count_words_and_chars(text: &str) -> (usize, usize, usize) {
    let mut words = 0;
    let mut no_spaces = 0;
    let mut with_spaces = 0;
    let mut in_word = false;

    for c in text.chars() {
        if c == '\u{fffc}' {
            if in_word {
                words += 1;
                in_word = false;
            }
            continue;
        }
        if matches!(c, '\r' | '\n' | '\u{b}' | '\u{c}' | '\u{e}') {
            if in_word {
                words += 1;
                in_word = false;
            }
            continue;
        }

        with_spaces += 1;

        if c.is_whitespace() {
            if in_word {
                words += 1;
                in_word = false;
            }
        } else {
            no_spaces += 1;
            if is_cjk_char(c) {
                if in_word {
                    words += 1;
                    in_word = false;
                }
                words += 1;
            } else if c.is_alphanumeric() || c == '_' {
                in_word = true;
            } else {
                if in_word {
                    words += 1;
                    in_word = false;
                }
            }
        }
    }

    if in_word {
        words += 1;
    }

    (words, no_spaces, with_spaces)
}

/// サイドバーの「文字カウント」パネル (Word's 文字カウント dialog).
pub(crate) struct WordCountPanel {
    pub(crate) stats: DocStatistics,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    /// Calculate document or selection statistics (pages, words, characters, paragraphs, lines).
    pub(crate) fn document_statistics(&self) -> DocStatistics {
        if self.has_selection() {
            let text = self.selected_text();
            let (words, characters_no_spaces, characters_with_spaces) = count_words_and_chars(&text);
            let lines_geom = self.flow_lines();
            let mut paras = 0;
            let mut lines_count = 0;
            let mut pages_set = std::collections::BTreeSet::new();

            if let Some((start, end)) = self.ordered_selection() {
                if let Some(doc) = self.flow() {
                    let order = super::editing::order(doc);
                    if let (Some(i), Some(j)) = (order.iter().position(|&p| p == start.para), order.iter().position(|&p| p == end.para)) {
                        paras = j.saturating_sub(i) + 1;
                    } else {
                        paras = 1;
                    }
                }
                for l in &lines_geom {
                    if l.para >= start.para.min(end.para) && l.para <= start.para.max(end.para) {
                        lines_count += 1;
                        pages_set.insert(l.page);
                    }
                }
            }
            if paras == 0 && !text.is_empty() {
                paras = 1;
            }
            if lines_count == 0 {
                lines_count = text.lines().count().max(1);
            }
            let pages = pages_set.len().max(1);

            DocStatistics {
                is_selection: true,
                pages,
                words,
                characters_no_spaces,
                characters_with_spaces,
                paragraphs: paras,
                lines: lines_count,
            }
        } else {
            let Some(doc) = self.flow() else { return DocStatistics::default() };
            let mut all_text = String::new();
            let mut paras = 0;
            for b in &doc.body {
                match b {
                    ibis_doc_model::Block::Paragraph(p) => {
                        paras += 1;
                        if !all_text.is_empty() {
                            all_text.push('\n');
                        }
                        all_text.push_str(&p.text());
                    }
                    ibis_doc_model::Block::Table(t) => {
                        for r in &t.rows {
                            for c in &r.cells {
                                for cb in &c.blocks {
                                    if let ibis_doc_model::Block::Paragraph(p) = cb {
                                        paras += 1;
                                        if !all_text.is_empty() {
                                            all_text.push('\n');
                                        }
                                        all_text.push_str(&p.text());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let (words, characters_no_spaces, characters_with_spaces) = count_words_and_chars(&all_text);
            let pages = self.presentation().slides.len().max(1);
            let lines = self.flow_lines().len().max(paras);

            DocStatistics {
                is_selection: false,
                pages,
                words,
                characters_no_spaces,
                characters_with_spaces,
                paragraphs: paras,
                lines,
            }
        }
    }

    /// Open the Word Count sidebar panel.
    pub(crate) fn open_word_count_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let stats = self.document_statistics();
        let panel = WordCountPanel {
            stats,
            ui: Default::default(),
        };
        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = HelpPanels::default();
            s.helper.word_count = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Close the Word Count sidebar panel.
    pub(crate) fn close_word_count_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.helper.word_count = None;
        }
        cx.notify();
    }

    /// Render the Word Count sidebar panel.
    pub(crate) fn word_count_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.doc_state()?.helper.word_count.as_ref()?;
        let live_stats=self.document_statistics();
        let p = self.doc_state()?.helper.word_count.as_ref()?;
        let stats = &live_stats;
        let theme = cx.theme();

        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };

        let stat_row = |id: &'static str, label: &'static str, value: usize| {
            mark(
                id,
                h_flex()
                    .justify_between()
                    .items_center()
                    .py(px(5.))
                    .border_b_1()
                    .border_color(theme.title_bar_border)
                    .child(div().text_sm().text_color(theme.foreground).child(text(label)))
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child(crate::shell::ribbon::thousands(value)))
                    .into_any_element(),
            )
        };

        let target_desc = if stats.is_selection {
            text("docx.help-panels.selected-text")
        } else {
            text("docx.help-panels.entire-document")
        };

        let body = v_flex()
            .gap_1()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .pb(px(4.))
                    .child(div().text_sm().text_color(theme.muted_foreground).child(target_desc))
,
            )
            .child(div().h(px(4.)))
            .child(stat_row("stat-pages", "docx.help-panels.pages", stats.pages))
            .child(stat_row("stat-words", "docx.help-panels.words", stats.words))
            .child(stat_row("stat-chars-no-spaces", "docx.help-panels.characters-no-spaces", stats.characters_no_spaces))
            .child(stat_row("stat-chars-with-spaces", "docx.help-panels.characters-spaces", stats.characters_with_spaces))
            .child(stat_row("stat-paragraphs", "docx.help-panels.paragraphs", stats.paragraphs))
            .child(stat_row("stat-lines", "docx.help-panels.lines", stats.lines));

        Some(SidebarPanel { title: text("docx.help-panels.word-count"), body: body.into_any_element() })
    }

    pub(crate) fn open_field_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let caret = self.doc_state().and_then(|s| s.caret).unwrap_or(ibis_doc_model::FlowPos::new(0, 0));
        let (existing_code, is_edit) = if let Some(doc) = self.flow() {
            if let Some(code) = ibis_doc_model::get_field_at(doc, caret) {
                (code, true)
            } else {
                let sel_text = self.selected_text();
                let init = if !sel_text.is_empty() { sel_text } else { "PAGE".to_string() };
                (init, false)
            }
        } else {
            ("PAGE".to_string(), false)
        };

        let code = cx.new(|cx| InputState::new(window, cx).default_value(&existing_code).placeholder(text("_docx.field-code")));
        code.update(cx, |i, cx| i.focus(window, cx));
        let query=cx.new(|cx|InputState::new(window,cx).placeholder(text("fix.menu.search-placeholder")));
        let query_subscription=cx.subscribe_in(&query,window,|_,_,_:&gpui_kit::component::input::InputEvent,_,cx|cx.notify());
        let panel = FieldPanel {
            list_scroll: ScrollHandle::new(),
            query, _query_subscription:query_subscription,
            code,
            is_edit,
            target_pos: caret,
            message: String::new(),
            ui: Default::default(),
        };
        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = HelpPanels::default();
            s.helper.field = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn apply_field_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.field.as_ref()) else { return };
        let code_str = p.code.read(cx).value().trim().to_string();
        if code_str.is_empty() {
            if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.field.as_mut()) {
                p.message = text("_docx.enter-a-field-code").to_string();
            }
            cx.notify();
            return;
        }

        let is_edit = p.is_edit;
        let pos = p.target_pos;

        if is_edit {
            if let Some(doc) = self.flow() {
                if let Some(cmds) = ibis_doc_model::edit_field_at(doc, pos, &code_str) {
                    self.run_flow(cmds, cx);
                }
            }
        } else {
            let range = self.ordered_selection();
            if let Some(doc) = self.flow() {
                if let Some((cmds, new_pos)) = ibis_doc_model::insert_field_at_range(doc, range, &code_str, false) {
                    if self.run_flow(cmds, cx) {
                        self.set_caret(new_pos, false, cx);
                    }
                }
            }
        }

        if let Some(s) = self.doc_state_mut() {
            s.helper.field = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    pub(crate) fn field_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.field.as_ref()?;
        let is_edit = p.is_edit;
        let theme = cx.theme();
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };

        let mut body = crate::shell::chrome::panel_form().size_full().min_h_0().p_0();
        body = body.child(crate::shell::chrome::text_field(&p.query).full_width());
        let query=p.query.read(cx).value().to_lowercase();
        // プリセット一覧
        let mut presets = v_flex().gap_1();
        for &(instr, label) in &FIELD_PRESETS {
            if !query.is_empty() && !format!("{} {}",instr,text(label)).to_lowercase().contains(&query) {continue;}
            let instr_str = instr.to_string();
            let code_entity = p.code.clone();
            presets = presets.child(
                crate::shell::chrome::action_button(format!("preset_{instr}"))
                    .small()
                    .ghost()
                    .child(h_flex().w_full().child(format!("{instr}  ({})", text(label))))
                    .on_click(cx.listener(move |_, _, window, cx| {
                        code_entity.update(cx, |i, cx| {
                            i.set_value(&instr_str, window, cx);
                        });
                        cx.notify();
                    }))
            );
        }
        body = body.child(div().relative().flex_1().min_h_0().w_full()
            .child(div().id("field-preset-list").size_full().overflow_y_scroll().track_scroll(&p.list_scroll).child(presets))
            .child(crate::shell::chrome::always_visible_scrollbar_overlay("field-list-scrollbar",p.list_scroll.clone(),None)));

        // コード入力欄
        body = body.child(
            v_flex()
                .flex_none()
                .gap_1()
                .child(div().text_sm().text_color(theme.muted_foreground).child(text("_docx.field-codes")))
                .child(mark("field-code",crate::shell::chrome::text_field(&p.code).full_width().into_any_element()))
        );

        if !p.message.is_empty() {
            body = body.children((!p.message.is_empty()).then(||div().text_sm().child(SharedString::from(p.message.clone()))));
        }

        // 実行ボタンと閉じるボタン
        let btn_label = if is_edit { "_xlsx.update" } else { "menu.insert" };
        let footer = crate::shell::chrome::action_bar()
                .action(
                    mark_action("apply_field", crate::shell::chrome::ActionDefinition::new("apply_field", text(btn_label), crate::shell::chrome::ActionButtonKind::Primary)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.apply_field_panel(cx);
                            })))
                )
                .action(
                    crate::shell::chrome::ActionDefinition::new("close_field", text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(s) = this.doc_state_mut() {
                                s.helper.field = None;
                            }
                            this.sidebar_open = false;
                            cx.notify();
                        }))
                );

        let title = if is_edit { "_docx.edit-field" } else { "_xlsx.insert-field" };
        Some(SidebarPanel::with_footer(text(title), body.into_any_element(), footer))
    }

    pub(crate) fn open_caption_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let is_en = crate::shell::i18n::language() == crate::shell::i18n::Language::English;
        let default_label: gpui::SharedString = if is_en { "Figure".into() } else { crate::shell::i18n::text("docx.common.figure") };
        let mut presets = if is_en {
            vec!["Figure".to_string(), "Table".to_string(), "Equation".to_string()]
        } else {
            vec![crate::shell::i18n::text("docx.common.figure").to_string(), crate::shell::i18n::text("insert.table").to_string(), crate::shell::i18n::text("insert.equation").to_string()]
        };
        if let Some(doc) = self.flow() {
            for l in ibis_doc_model::caption_labels(doc) {
                if !presets.contains(&l) {
                    presets.push(l);
                }
            }
        }

        let label = cx.new(|cx| InputState::new(window, cx).default_value(default_label).placeholder(text("_docx.label")));
        let text_in = cx.new(|cx| InputState::new(window, cx).placeholder(text("_docx.caption-text")));
        text_in.update(cx, |i, cx| i.focus(window, cx));

        let panel = CaptionPanel {
            custom_label:false,
            label,
            text: text_in,
            format: ibis_doc_model::CaptionNumberFormat::Arabic,
            exclude_label: false,
            chapter: None,
            presets,
            message: String::new(),
            ui: Default::default(),
        };

        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = HelpPanels::default();
            s.helper.caption = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn apply_caption_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.caption.as_ref()) else { return };
        let label_str = p.label.read(cx).value().trim().to_string();
        let text_str = p.text.read(cx).value().trim().to_string();
        let format = p.format;
        let exclude = p.exclude_label;
        let chapter = p.chapter.map(|(level, sep)| (level, CAPTION_SEPARATORS[sep.min(CAPTION_SEPARATORS.len() - 1)].1));

        if label_str.is_empty() && !exclude {
            if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.caption.as_mut()) {
                p.message = text("_docx.enter-a-label").to_string();
            }
            cx.notify();
            return;
        }

        self.insert_caption_detailed(&label_str, &text_str, format, exclude, chapter, cx);

        if let Some(s) = self.doc_state_mut() {
            s.helper.caption = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    pub(crate) fn caption_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.caption.as_ref()?;
        let theme = cx.theme();
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };

        let mut body = v_flex().gap_3().p_3().p_0();
        let mut presets=radio_group().layout(RadioGroupLayout::Flow).child(radio_button("caption-custom",p.custom_label,false,text("ui.custom"),cx.listener(|e,_,_,cx|{if let Some(p)=e.doc_state_mut().and_then(|s|s.helper.caption.as_mut()){p.custom_label=true;}cx.notify();})));
        for (i,preset) in p.presets.iter().enumerate() {
            let value=preset.clone();let input=p.label.clone();let on=!p.custom_label && p.label.read(cx).value().as_ref()==preset;
            presets=presets.child(radio_button(("caption-preset",i),on,false,preset.clone(),cx.listener(move |e,_,w,cx|{input.update(cx,|i,cx|i.set_value(value.clone(),w,cx));if let Some(p)=e.doc_state_mut().and_then(|s|s.helper.caption.as_mut()){p.custom_label=false;}cx.notify();})));
        }
        body=body.child(presets).child(h_flex().items_center().gap_2().child(div().text_sm().child(text("ui.caption-label"))).child(crate::shell::chrome::text_field(&p.label).width(px(124.)).disabled(!p.custom_label)));

        // 説明文入力
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().text_color(theme.muted_foreground).child(text("_docx.caption-text-2")))
                .child(crate::shell::chrome::text_field(&p.text).full_width())
        );

        // 番号書式選択
        let current_format = p.format;
        let mut formats = radio_group().layout(RadioGroupLayout::Flow);
        for &(fmt, lbl) in &CAPTION_FORMATS {
            let is_sel = fmt == current_format;
            formats = formats.child(
                mark(
                    &format!("fmt_{:?}", fmt),
                    radio_button(format!("btn_fmt_{:?}", fmt), is_sel, false, lbl, cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.caption.as_mut()) {
                                p.format = fmt;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                )
            );
        }
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().text_color(theme.muted_foreground).child(text("_docx.number-format")))
                .child(formats)
        );

        // ラベルを除外チェックボックス
        let exclude = p.exclude_label;
        body = body.child(
            mark(
                "exclude_label",
                Checkbox::new("btn_exclude_label").checked(exclude).label(text("_docx.exclude-label-from-caption"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.caption.as_mut()) {
                            p.exclude_label = !p.exclude_label;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        );

        // 章番号を含める: the heading level, and the separator.
        let chapter = p.chapter;
        body = body.child(
            mark(
                "chapter_toggle",
                Checkbox::new("btn_chapter").checked(chapter.is_some()).label(text("_docx.include-chapter-number"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.caption.as_mut()) {
                            p.chapter = if p.chapter.is_some() { None } else { Some((1, 0)) };
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        );
        if let Some((level, sep)) = chapter {
            let mut levels = radio_group().layout(RadioGroupLayout::Flow);
            for l in 1..=3u32 {
                levels = levels.child(radio_button(format!("btn_chapter_level_{l}"), l == level, false, crate::shell::i18n::format("docx.help-panels.heading-n", &[("n", &l)]), cx.listener(move |this, _, _, cx| {
                    if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.caption.as_mut()) {
                        p.chapter = p.chapter.map(|(_, s)| (l, s));
                    }
                    cx.notify();
                })));
            }
            let mut separators = radio_group().layout(RadioGroupLayout::Flow);
            for (i, (name, _)) in CAPTION_SEPARATORS.iter().enumerate() {
                separators = separators.child(radio_button(format!("btn_chapter_sep_{i}"), i == sep, false, text(name), cx.listener(move |this, _, _, cx| {
                    if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.caption.as_mut()) {
                        p.chapter = p.chapter.map(|(l, _)| (l, i));
                    }
                    cx.notify();
                })));
            }
            body = body.child(v_flex().gap_1().child(levels).child(separators));
        }

        if !p.message.is_empty() {
            body = body.children((!p.message.is_empty()).then(||div().text_sm().child(SharedString::from(p.message.clone()))));
        }

        // 実行ボタンとキャンセルボタン
        let footer = crate::shell::chrome::action_bar()
                .action(
                    mark_action("apply_caption", crate::shell::chrome::ActionDefinition::new("apply_caption", text("_shell.insert"), crate::shell::chrome::ActionButtonKind::Primary)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.apply_caption_panel(cx);
                            })))
                )
                .action(
                    crate::shell::chrome::ActionDefinition::new("close_caption", text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(s) = this.doc_state_mut() {
                                s.helper.caption = None;
                            }
                            this.sidebar_open = false;
                            cx.notify();
                        }))
                );

        Some(SidebarPanel::with_footer(text("docx.help-panels.caption"), body.into_any_element(), footer))
    }
}

/// ファイル > 情報 / プロパティ… (Word's プロパティ dialog):
/// タイトル・作成者・件名・キーワード・コメント・管理者・会社名・カテゴリの編集とメタデータ・統計情報の閲覧。
#[derive(Clone)]
pub(crate) struct DocPropertiesPanel {
    pub(crate) title: Entity<InputState>,
    pub(crate) creator: Entity<InputState>,
    pub(crate) subject: Entity<InputState>,
    pub(crate) keywords: Entity<InputState>,
    pub(crate) description: Entity<InputState>,
    pub(crate) category: Entity<InputState>,
    pub(crate) manager: Entity<InputState>,
    pub(crate) company: Entity<InputState>,
    pub(crate) custom_properties: Vec<ibis_doc_model::CustomProperty>,
    pub(crate) new_custom_name: Entity<InputState>,
    pub(crate) new_custom_val: Entity<InputState>,
    pub(crate) embed_true_type_fonts: bool,
    pub(crate) save_subset_fonts: bool,
    pub(crate) embed_system_fonts: bool,
    pub(crate) created: String,
    pub(crate) modified: String,
    pub(crate) last_printed: String,
    pub(crate) revision: String,
    pub(crate) stats: DocStatistics,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    /// Open the Document Properties sidebar panel.
    pub(crate) fn open_doc_properties_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(doc) = self.flow() else { return };
        let props = &doc.properties;
        let stats = self.document_statistics();
        let title = cx.new(|cx| InputState::new(window, cx).default_value(props.title.clone().unwrap_or_default()));
        let creator = cx.new(|cx| InputState::new(window, cx).default_value(props.creator.clone().unwrap_or_default()));
        let subject = cx.new(|cx| InputState::new(window, cx).default_value(props.subject.clone().unwrap_or_default()));
        let keywords = cx.new(|cx| InputState::new(window, cx).default_value(props.keywords.clone().unwrap_or_default()));
        let description = cx.new(|cx| InputState::new(window, cx).default_value(props.description.clone().unwrap_or_default()));
        let category = cx.new(|cx| InputState::new(window, cx).default_value(props.category.clone().unwrap_or_default()));
        let manager = cx.new(|cx| InputState::new(window, cx).default_value(props.manager.clone().unwrap_or_default()));
        let company = cx.new(|cx| InputState::new(window, cx).default_value(props.company.clone().unwrap_or_default()));
        let created = props.created.clone().unwrap_or_default();
        let modified = props.modified.clone().unwrap_or_default();
        let last_printed = props.last_printed.clone().unwrap_or_default();
        let revision = props.revision.clone().unwrap_or_default();

        let new_custom_name = cx.new(|cx| InputState::new(window, cx));
        let new_custom_val = cx.new(|cx| InputState::new(window, cx));
        let custom_properties = props.custom.clone();
        let embed_true_type_fonts = doc.embed_true_type_fonts;
        let save_subset_fonts = doc.save_subset_fonts;
        let embed_system_fonts = doc.embed_system_fonts;

        let panel = DocPropertiesPanel {
            title,
            creator,
            subject,
            keywords,
            description,
            category,
            manager,
            company,
            custom_properties,
            new_custom_name,
            new_custom_val,
            embed_true_type_fonts,
            save_subset_fonts,
            embed_system_fonts,
            created,
            modified,
            last_printed,
            revision,
            stats,
            message: String::new(),
            ui: Default::default(),
        };

        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = HelpPanels::default();
            s.helper.properties = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Close the Document Properties sidebar panel.
    pub(crate) fn close_doc_properties_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.helper.properties = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// Toggle font embedding in the Document Properties panel.
    pub(crate) fn toggle_doc_properties_embed_fonts(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.properties.as_mut()) {
            p.embed_true_type_fonts = enabled;
        }
        self.set_embed_true_type_fonts(enabled, cx);
        cx.notify();
    }

    /// Toggle save subset fonts in the Document Properties panel.
    pub(crate) fn toggle_doc_properties_save_subset_fonts(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.properties.as_mut()) {
            if !p.embed_true_type_fonts {
                return;
            }
            p.save_subset_fonts = enabled;
        }
        self.set_save_subset_fonts(enabled, cx);
        cx.notify();
    }

    /// Toggle embed system fonts in the Document Properties panel.
    pub(crate) fn toggle_doc_properties_embed_system_fonts(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.properties.as_mut()) {
            if !p.embed_true_type_fonts {
                return;
            }
            p.embed_system_fonts = enabled;
        }
        self.set_embed_system_fonts(enabled, cx);
        cx.notify();
    }

    /// Add or update a custom document property in the panel.
    pub(crate) fn add_doc_custom_property(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (name, val) = {
            let Some(p) = self.doc_state().and_then(|s| s.helper.properties.as_ref()) else { return };
            (
                p.new_custom_name.read(cx).text().to_string().trim().to_string(),
                p.new_custom_val.read(cx).text().to_string().trim().to_string(),
            )
        };
        if name.is_empty() {
            return;
        }
        let Some(p) = self.doc_state_mut().and_then(|s| s.helper.properties.as_mut()) else { return };
        if let Some(existing) = p.custom_properties.iter_mut().find(|cp| cp.name == name) {
            existing.value = ibis_doc_model::CustomPropertyValue::Text(val);
        } else {
            p.custom_properties.push(ibis_doc_model::CustomProperty {
                name,
                value: ibis_doc_model::CustomPropertyValue::Text(val),
            });
        }
        p.new_custom_name.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        p.new_custom_val.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        cx.notify();
    }

    /// Remove a custom property by index from the panel.
    pub(crate) fn remove_doc_custom_property(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state_mut().and_then(|s| s.helper.properties.as_mut()) else { return };
        if index < p.custom_properties.len() {
            p.custom_properties.remove(index);
            cx.notify();
        }
    }

    /// Apply the entered document properties via `FlowCommand::SetDocProperties` and font embedding settings.
    pub(crate) fn apply_doc_properties(&mut self, cx: &mut Context<Self>) {
        let (title, creator, subject, keywords, description, category, manager, company, custom, embed_tt, save_sub, embed_sys) = {
            let Some(p) = self.doc_state().and_then(|s| s.helper.properties.as_ref()) else { return };
            (
                p.title.read(cx).text().to_string().trim().to_string(),
                p.creator.read(cx).text().to_string().trim().to_string(),
                p.subject.read(cx).text().to_string().trim().to_string(),
                p.keywords.read(cx).text().to_string().trim().to_string(),
                p.description.read(cx).text().to_string().trim().to_string(),
                p.category.read(cx).text().to_string().trim().to_string(),
                p.manager.read(cx).text().to_string().trim().to_string(),
                p.company.read(cx).text().to_string().trim().to_string(),
                p.custom_properties.clone(),
                p.embed_true_type_fonts,
                p.save_subset_fonts,
                p.embed_system_fonts,
            )
        };

        let opt = |s: String| if s.is_empty() { None } else { Some(s) };

        let mut new_props = self.flow().map(|d| d.properties.clone()).unwrap_or_default();
        new_props.title = opt(title);
        new_props.creator = opt(creator);
        new_props.subject = opt(subject);
        new_props.keywords = opt(keywords);
        new_props.description = opt(description);
        new_props.category = opt(category);
        new_props.manager = opt(manager);
        new_props.company = opt(company);
        new_props.custom = custom;

        let mut commands = vec![ibis_doc_model::FlowCommand::SetDocProperties(new_props)];
        if let Some(doc) = self.flow() {
            if doc.embed_true_type_fonts != embed_tt || doc.embed_system_fonts != embed_sys || doc.save_subset_fonts != save_sub {
                commands.push(ibis_doc_model::FlowCommand::SetFontEmbedding {
                    embed_true_type_fonts: embed_tt,
                    embed_system_fonts: embed_sys,
                    save_subset_fonts: save_sub,
                });
            }
        }
        if self.run_flow(commands, cx) {
            // Also trigger fields update so document fields (AUTHOR, TITLE, etc.) reflect the change
            self.update_fields(true, cx);
            if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.properties.as_mut()) {
                p.message = text("docx.properties-panel.document-properties-updated").to_string();
            }
            cx.notify();
        }
    }

    /// Render the Document Properties sidebar panel.
    pub(crate) fn doc_properties_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.properties.as_ref()?;
        let theme = cx.theme();
        let ui = p.ui.clone();

        let mark = |id: &str, child: AnyElement| {
            let ui = ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };

        let stat_item = |label: &'static str, value: usize| {
            h_flex()
                .justify_between()
                .items_center()
                .py(px(2.))
                .child(div().text_sm().text_color(theme.muted_foreground).child(text(label)))
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child(crate::shell::ribbon::thousands(value)))
        };

        let meta_item = |label: &'static str, val: &str| {
            let val_str: SharedString = if val.is_empty() { "-".into() } else { val.to_string().into() };
            h_flex()
                .justify_between()
                .items_center()
                .py(px(2.))
                .child(div().text_sm().text_color(theme.muted_foreground).child(text(label)))
                .child(div().text_sm().text_color(theme.foreground).child(val_str))
        };

        let field_input = |id: &'static str, label: &'static str, entity: &Entity<InputState>| {
            v_flex()
                .gap_1()
                .py(px(3.))
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.muted_foreground).child(text(label)))
                .child(mark(id, crate::shell::chrome::text_field(entity).full_width().into_any_element()))
        };

        let mut body = v_flex()
            .gap_2()
            .px(px(16.))
            .py(px(8.)).p_0();

        // 統計・メタ情報セクション
        let stats_box = v_flex()
            .gap_1()
            .p(px(8.))
            .rounded(px(4.))
            .bg(theme.title_bar_border.opacity(0.15))
            .child(stat_item("docx.help-panels.pages", p.stats.pages))
            .child(stat_item("docx.help-panels.words", p.stats.words))
            .child(stat_item("docx.help-panels.characters", p.stats.characters_with_spaces))
            .child(meta_item("pptx.properties-panel.created", &crate::clock::format_local_datetime(&p.created)))
            .child(meta_item("pptx.properties-panel.modified", &crate::clock::format_local_datetime(&p.modified)))
            .child(meta_item("docx.properties-panel.last-printed", &crate::clock::format_local_datetime(&p.last_printed)))
            .child(meta_item("docx.properties-panel.revision", &p.revision));

        body = body.child(stats_box);

        // プロパティ入力フィールド群
        body = body
            .child(field_input("title", "_shell.title", &p.title))
            .child(field_input("creator", "pptx.properties-panel.author", &p.creator))
            .child(field_input("subject", "pptx.properties-panel.subject", &p.subject))
            .child(field_input("keywords", "docx.properties-panel.keywords", &p.keywords))
            .child(field_input("description", "_shell.comment", &p.description))
            .child(field_input("company", "docx.properties-panel.company", &p.company))
            .child(field_input("manager", "docx.properties-panel.manager", &p.manager))
            .child(field_input("category", "docx.properties-panel.category", &p.category));

        // カスタムプロパティセクション
        let mut custom_section = v_flex().gap_1().py(px(4.));
        custom_section = custom_section.child(
            div().text_sm().font_weight(FontWeight::BOLD).text_color(theme.foreground).child(text("pptx.properties-panel.custom-properties"))
        );
        for (ix, prop) in p.custom_properties.iter().enumerate() {
            let val_str = match &prop.value {
                ibis_doc_model::CustomPropertyValue::Text(s) => s.clone(),
                ibis_doc_model::CustomPropertyValue::Number(s) => s.clone(),
                ibis_doc_model::CustomPropertyValue::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
                ibis_doc_model::CustomPropertyValue::DateTime(s) => crate::clock::format_local_datetime(s),
            };
            let prop_name = prop.name.clone();
            let row = h_flex()
                .justify_between()
                .items_center()
                .py(px(2.))
                .px(px(4.))
                .rounded(px(3.))
                .bg(theme.title_bar_border.opacity(0.1))
                .child(
                    h_flex()
                        .gap_1()
                        .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child(SharedString::from(prop_name)))
                        .child(div().text_sm().text_color(theme.muted_foreground).child(": "))
                        .child(div().text_sm().text_color(theme.foreground).child(SharedString::from(val_str)))
                )
                .child(
                    mark(
                        &format!("remove_custom_{ix}"),
                        crate::shell::chrome::destructive_button(("remove_custom", ix),cx)
                            .small()
                            .label(text("×"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_doc_custom_property(ix, cx);
                            }))
                            .into_any_element(),
                    )
                );
            custom_section = custom_section.child(row);
        }

        // カスタムプロパティ追加フォーム
        let add_row = h_flex()
            .gap_1()
            .items_center()
            .pt(px(2.))
            .child(div().flex_1().child(mark("new_custom_name", crate::shell::chrome::text_field(&p.new_custom_name).full_width().into_any_element())))
            .child(div().flex_1().child(mark("new_custom_val", crate::shell::chrome::text_field(&p.new_custom_val).full_width().into_any_element())))
            .child(
                mark(
                    "add_custom",
                    crate::shell::chrome::action_button("add_custom")
                        .secondary()
                        .small()
                        .label(text("fix.common.add"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.add_doc_custom_property(window, cx);
                        }))
                        .into_any_element(),
                )
            );
        custom_section = custom_section.child(add_row);
        body = body.child(custom_section);

        // フォントの埋め込み (DOCX-OPTIONS-013 / DocProperties)
        let font_embed_section = v_flex()
            .gap_1()
            .py(px(4.))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.foreground)
                    .child(text("docx.properties-panel.fonts-section"))
            )
            .child(
                mark(
                    "embed_fonts",
                    Checkbox::new("docx-doc-props-embed-fonts")
                        .label(text("docx.options.font.embed"))
                        .checked(p.embed_true_type_fonts)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            this.toggle_doc_properties_embed_fonts(*on, cx);
                        }))
                        .into_any_element(),
                )
            )
            .child(
                mark(
                    "save_subset_fonts",
                    Checkbox::new("docx-doc-props-save-subset")
                        .label(text("docx.options.font.save-subset"))
                        .checked(p.save_subset_fonts)
                        .disabled(!p.embed_true_type_fonts)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            this.toggle_doc_properties_save_subset_fonts(*on, cx);
                        }))
                        .into_any_element(),
                )
            )
            .child(
                mark(
                    "embed_system_fonts",
                    Checkbox::new("docx-doc-props-embed-system")
                        .label(text("docx.options.font.embed-system"))
                        .checked(!p.embed_system_fonts)
                        .disabled(!p.embed_true_type_fonts)
                        .on_click(cx.listener(|this, do_not_embed: &bool, _, cx| {
                            this.toggle_doc_properties_embed_system_fonts(!*do_not_embed, cx);
                        }))
                        .into_any_element(),
                )
            );
        body = body.child(font_embed_section);

        if !p.message.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(SharedString::from(p.message.clone()))
            );
        }

        // アクションボタン
        let footer = crate::shell::chrome::action_bar()
                .action(
                    mark_action("apply_properties", crate::shell::chrome::ActionDefinition::new("apply_properties", text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.apply_doc_properties(cx);
                            })))
                )

            .action(crate::shell::chrome::dialog_cancel_definition(cx));

        Some(SidebarPanel::with_footer(text("docx.help-panels.document-properties"), body.into_any_element(), footer))
    }
}

/// 挿入 > 相互参照… (Word's 相互参照 Cross-reference dialog):
/// 参照する項目（見出し、ブックマーク、脚注、文末脚注、図表番号）、
/// 挿入する参照内容（テキスト、ページ番号、段落番号、注番号、番号とラベルのみ）、
/// 対象項目一覧、ハイパーリンクとして挿入のチェックボックス、挿入ボタン。
pub(crate) struct CrossReferencePanel {
    pub(crate) ref_type: ibis_doc_model::CrossReferenceType,
    pub(crate) ref_kind: ibis_doc_model::CrossReferenceKind,
    pub(crate) chosen_index: Option<usize>,
    pub(crate) hyperlink: bool,
    pub(crate) targets: Vec<ibis_doc_model::CrossReferenceTarget>,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    pub(crate) fn open_cross_reference_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let targets = self.flow().map(|d| ibis_doc_model::cross_reference_targets(d, ibis_doc_model::CrossReferenceType::Heading)).unwrap_or_default();
        let chosen_index = if targets.is_empty() { None } else { Some(0) };
        let panel = CrossReferencePanel {
            ref_type: ibis_doc_model::CrossReferenceType::Heading,
            ref_kind: ibis_doc_model::CrossReferenceKind::Text,
            chosen_index,
            hyperlink: true,
            targets,
            message: String::new(),
            ui: Default::default(),
        };
        if let Some(state) = self.doc_state_mut() {
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.tab_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            state.style_edit = None;
            state.link = None;
            state.find = None;
            state.comments = None;
            state.helper.page_setup = None;
            state.helper.notes = None;
            state.helper.bookmarks = None;
            state.helper.watermark = None;
            state.helper.new_style = None;
            state.helper.word_count = None;
            state.helper.field = None;
            state.helper.caption = None;
            state.helper.properties = None;
            state.helper.cross_reference = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_cross_reference_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.helper.cross_reference = None;
        }
        cx.notify();
    }

    pub(crate) fn choose_cross_reference_type(&mut self, ref_type: ibis_doc_model::CrossReferenceType, cx: &mut Context<Self>) {
        let targets = self.flow().map(|d| ibis_doc_model::cross_reference_targets(d, ref_type)).unwrap_or_default();
        let chosen_index = if targets.is_empty() { None } else { Some(0) };
        let default_kind = match ref_type {
            ibis_doc_model::CrossReferenceType::Heading | ibis_doc_model::CrossReferenceType::Bookmark => ibis_doc_model::CrossReferenceKind::Text,
            ibis_doc_model::CrossReferenceType::Footnote | ibis_doc_model::CrossReferenceType::Endnote => ibis_doc_model::CrossReferenceKind::NoteNumber,
            ibis_doc_model::CrossReferenceType::Caption => ibis_doc_model::CrossReferenceKind::LabelAndNumber,
        };
        if let Some(panel) = self.doc_state_mut().and_then(|s| s.helper.cross_reference.as_mut()) {
            panel.ref_type = ref_type;
            panel.ref_kind = default_kind;
            panel.chosen_index = chosen_index;
            panel.targets = targets;
            panel.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn choose_cross_reference_kind(&mut self, kind: ibis_doc_model::CrossReferenceKind, cx: &mut Context<Self>) {
        if let Some(panel) = self.doc_state_mut().and_then(|s| s.helper.cross_reference.as_mut()) {
            panel.ref_kind = kind;
        }
        cx.notify();
    }

    pub(crate) fn select_cross_reference_item(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(panel) = self.doc_state_mut().and_then(|s| s.helper.cross_reference.as_mut()) {
            panel.chosen_index = Some(idx);
        }
        cx.notify();
    }

    pub(crate) fn toggle_cross_reference_hyperlink(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.doc_state_mut().and_then(|s| s.helper.cross_reference.as_mut()) {
            panel.hyperlink = !panel.hyperlink;
        }
        cx.notify();
    }

    pub(crate) fn apply_cross_reference(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.doc_state().and_then(|s| s.helper.cross_reference.as_ref()) else { return };
        let Some(idx) = panel.chosen_index else { return };
        let Some(target) = panel.targets.get(idx).cloned() else { return };
        let ref_type = panel.ref_type;
        let ref_kind = panel.ref_kind;
        let hyperlink = panel.hyperlink;

        let Some((start, _)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow() else { return };

        let Some(commands) = ibis_doc_model::insert_cross_reference_target(doc, start, &target, ref_type, ref_kind, hyperlink) else { return };
        if self.run_flow(commands, cx) {
            self.set_caret(ibis_doc_model::FlowPos::new(start.para, start.offset + 1), false, cx);
            if let Some(panel) = self.doc_state_mut().and_then(|s| s.helper.cross_reference.as_mut()) {
                panel.message = text("docx.xref.inserted").to_string();
            }
        }
        cx.notify();
    }

    pub(crate) fn cross_reference_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.doc_state()?.helper.cross_reference.as_ref()?;
        let theme = cx.theme();
        let ref_type = panel.ref_type;
        let ref_kind = panel.ref_kind;
        let chosen_index = panel.chosen_index;
        let hyperlink = panel.hyperlink;
        let targets = panel.targets.clone();
        let message = panel.message.clone();

        let mark = |id: &str, child: AnyElement| {
            let ui = panel.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = panel.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };

        let mut body = v_flex().gap_3().px(px(14.)).py(px(10.)).p_0();

        // 1. 参照する項目（Reference Type）
        let types: [(ibis_doc_model::CrossReferenceType, &'static str); 5] = [
            (ibis_doc_model::CrossReferenceType::Heading, "docx.xref.type.heading"),
            (ibis_doc_model::CrossReferenceType::Bookmark, "docx.xref.type.bookmark"),
            (ibis_doc_model::CrossReferenceType::Footnote, "docx.xref.type.footnote"),
            (ibis_doc_model::CrossReferenceType::Endnote, "docx.xref.type.endnote"),
            (ibis_doc_model::CrossReferenceType::Caption, "docx.xref.type.caption"),
        ];
        let mut type_buttons = radio_group().layout(RadioGroupLayout::Flow);
        for (t, label_key) in types {
            let is_sel = ref_type == t;
            let btn_id = format!("xref-type-{}", label_key.strip_prefix("docx.xref.type.").unwrap_or(label_key));
            type_buttons = type_buttons.child(
                mark(&btn_id,
                    radio_button(SharedString::from(btn_id.clone()), is_sel, false, text(label_key), cx.listener(move |this, _, _, cx| {
                            this.choose_cross_reference_type(t, cx);
                        }))
                        .into_any_element()
                )
            );
        }
        body = body.child(
            v_flex().gap_1()
                .child(div().text_sm().text_color(theme.muted_foreground).child(text("docx.xref.type")))
                .child(type_buttons)
        );

        // 2. 挿入する参照内容（Reference Content Kind）
        let available_kinds: Vec<(ibis_doc_model::CrossReferenceKind, &'static str)> = match ref_type {
            ibis_doc_model::CrossReferenceType::Heading => vec![
                (ibis_doc_model::CrossReferenceKind::Text, "docx.xref.kind.text"),
                (ibis_doc_model::CrossReferenceKind::PageNumber, "docx.xref.kind.page"),
                (ibis_doc_model::CrossReferenceKind::ParagraphNumber, "docx.xref.kind.paragraph"),
                (ibis_doc_model::CrossReferenceKind::RelativePosition, "docx.xref.kind.relative"),
            ],
            ibis_doc_model::CrossReferenceType::Bookmark => vec![
                (ibis_doc_model::CrossReferenceKind::Text, "docx.xref.kind.text"),
                (ibis_doc_model::CrossReferenceKind::PageNumber, "docx.xref.kind.page"),
                (ibis_doc_model::CrossReferenceKind::RelativePosition, "docx.xref.kind.relative"),
            ],
            ibis_doc_model::CrossReferenceType::Footnote | ibis_doc_model::CrossReferenceType::Endnote => vec![
                (ibis_doc_model::CrossReferenceKind::NoteNumber, "docx.xref.kind.note"),
                (ibis_doc_model::CrossReferenceKind::PageNumber, "docx.xref.kind.page"),
                (ibis_doc_model::CrossReferenceKind::RelativePosition, "docx.xref.kind.relative"),
            ],
            ibis_doc_model::CrossReferenceType::Caption => vec![
                (ibis_doc_model::CrossReferenceKind::LabelAndNumber, "docx.xref.kind.label_number"),
                (ibis_doc_model::CrossReferenceKind::Text, "docx.xref.kind.entire_caption"),
                (ibis_doc_model::CrossReferenceKind::CaptionText, "docx.xref.kind.caption_text"),
                (ibis_doc_model::CrossReferenceKind::PageNumber, "docx.xref.kind.page"),
                (ibis_doc_model::CrossReferenceKind::RelativePosition, "docx.xref.kind.relative"),
            ],
        };
        let mut kind_buttons = radio_group().layout(RadioGroupLayout::Flow);
        for (k, label_key) in available_kinds {
            let is_sel = ref_kind == k;
            let btn_id = format!("xref-kind-{}", label_key.strip_prefix("docx.xref.kind.").unwrap_or(label_key));
            kind_buttons = kind_buttons.child(
                mark(&btn_id,
                    radio_button(SharedString::from(btn_id.clone()), is_sel, false, text(label_key), cx.listener(move |this, _, _, cx| {
                            this.choose_cross_reference_kind(k, cx);
                        }))
                        .into_any_element()
                )
            );
        }
        body = body.child(
            v_flex().gap_1()
                .child(div().text_sm().text_color(theme.muted_foreground).child(text("docx.xref.kind")))
                .child(kind_buttons)
        );

        // 3. ハイパーリンクチェックボックス
        body = body.child(mark(
            "xref-hyperlink",
            Checkbox::new("xref-hyperlink").checked(hyperlink).label(text("docx.xref.hyperlink"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_cross_reference_hyperlink(cx);
                }))
                .into_any_element(),
        ));

        // 4. 対象項目リスト
        let mut list_container = v_flex().gap_1();

        if targets.is_empty() {
            list_container = list_container.child(
                div()
                    .px(px(8.))
                    .py(px(4.))
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(text("docx.xref.empty"))
            );
        } else {
            for (idx, target) in targets.into_iter().enumerate() {
                let is_sel = chosen_index == Some(idx);
                let item_id = format!("xref-item-{idx}");
                body = body.child(mark(
                    &item_id,
                    h_flex().id(SharedString::from(item_id.clone())).w_full().p_2().text_sm().cursor_pointer()
                        .when(is_sel,|d|d.bg(rgb(crate::shell::chrome::SELECTED_ROW_BG)))
                        .child(target.label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_cross_reference_item(idx, cx);
                        }))
                        .into_any_element()
                ));
            }
        }
        body = body.child(list_container);

        // メッセージ
        if !message.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(SharedString::from(message))
            );
        }

        // 5. アクションボタン
        let mut actions = crate::shell::chrome::action_bar();
        if chosen_index.is_some() {
            actions = actions.action(mark_action("xref-insert", crate::shell::chrome::ActionDefinition::new("xref-insert", text("docx.xref.insert"), crate::shell::chrome::ActionButtonKind::Primary)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.apply_cross_reference(cx);
                    }))));
        }
        actions = actions.action(crate::shell::chrome::dialog_cancel_definition(cx));


        Some(SidebarPanel::with_footer(text("docx.xref.title"), body.into_any_element(), actions))
    }

    /// 挿入 > 目次 > 目次の更新…: Word の「目次の更新」ダイアログ（パネル）を開く。
    pub(crate) fn open_update_toc_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.accessibility_panel = false;
            s.selection_pane = false;
            s.helper = HelpPanels::default();
            s.helper.update_toc = Some(UpdateTocPanel { kind: UpdateTocKind::PageNumbersOnly });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 目次の更新パネルを閉じる。
    pub(crate) fn close_update_toc_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.helper.update_toc = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// 目次の更新パネルのオプション（ページ番号のみ / 目次全体）を設定。
    pub(crate) fn set_update_toc_kind(&mut self, kind: UpdateTocKind, cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.update_toc.as_mut()) {
            p.kind = kind;
        }
        cx.notify();
    }

    /// 目次の更新パネルの現在の選択（kind）を取得。
    pub(crate) fn update_toc_kind(&self) -> Option<UpdateTocKind> {
        self.doc_state().and_then(|s| s.helper.update_toc.as_ref()).map(|p| p.kind)
    }

    /// 目次の更新パネルの「OK」実行：選択されたオプションで更新を適用。
    pub(crate) fn apply_update_toc_panel(&mut self, cx: &mut Context<Self>) {
        let kind = self.update_toc_kind().unwrap_or_default();
        self.close_update_toc_panel(cx);
        match kind {
            UpdateTocKind::PageNumbersOnly => {
                self.update_toc_page_numbers_only(cx);
            }
            UpdateTocKind::EntireTable => {
                self.update_toc(None, cx);
            }
        }
    }

    /// Render the Update Table of Contents sidebar panel.
    pub(crate) fn update_toc_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.update_toc.as_ref()?;
        let current_kind = p.kind;
        let theme = cx.theme();

        let mut body = v_flex().gap_3().p_3().p_0();

        // 1. 説明プロンプト
        body = body.child(
            div()
                .text_sm()
                .text_color(theme.foreground)
                .child(text("docx.toc.update-dialog.prompt"))
        );

        // 2. ラジオ選択肢: ページ番号だけを更新する
        let is_pages = current_kind == UpdateTocKind::PageNumbersOnly;
        let pages_btn = radio_button("toc-update-radio-pages", is_pages, false, text("docx.toc.update-page-numbers-only"), cx.listener(|this, _, _, cx| {
                this.set_update_toc_kind(UpdateTocKind::PageNumbersOnly, cx);
            }));


        // 3. ラジオ選択肢: 目次をすべて更新する
        let is_entire = current_kind == UpdateTocKind::EntireTable;
        let entire_btn = radio_button("toc-update-radio-entire", is_entire, false, text("docx.toc.update-entire-table"), cx.listener(|this, _, _, cx| {
                this.set_update_toc_kind(UpdateTocKind::EntireTable, cx);
            }));
        body = body.child(radio_group().child(pages_btn).child(entire_btn));

        // 4. アクションボタン: OK / キャンセル
        let actions = crate::shell::chrome::action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("toc-update-ok", text("docx.toc.update-ok"), crate::shell::chrome::ActionButtonKind::Primary)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.apply_update_toc_panel(cx);
                    }))
            )
            .action(
                crate::shell::chrome::ActionDefinition::new("toc-update-cancel", text("docx.toc.update-cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_update_toc_panel(cx);
                    }))
            );


        Some(SidebarPanel::with_footer(text("docx.toc.update-dialog.title"), body.into_any_element(), actions))
    }
}

/// 校閲 > ユーザー情報… (Word Options > 全般: ユーザー設定 DOCX-OPTIONS-003):
/// ユーザー名と頭文字の編集・適用・既定値リセット。
pub(crate) struct UserInfoPanel {
    pub(crate) name_input: Entity<InputState>,
    pub(crate) initials_input: Entity<InputState>,
}

impl SlideEditor {
    /// Open the User Information sidebar panel.
    pub(crate) fn open_user_info_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let cur_name = self.user_name();
        let cur_inits = self.user_initials();

        let name_input = cx.new(|cx| InputState::new(window, cx).default_value(cur_name).placeholder(text("docx.user-info.name-placeholder")));
        let initials_input = cx.new(|cx| InputState::new(window, cx).default_value(cur_inits).placeholder(text("docx.user-info.initials-placeholder")));

        let panel = UserInfoPanel {
            name_input,
            initials_input,
        };

        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = HelpPanels::default();
            s.helper.user_info = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Close the User Information sidebar panel.
    pub(crate) fn close_user_info_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.helper.user_info = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// Apply custom user name and initials from the panel.
    pub(crate) fn apply_user_info_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.user_info.as_ref()) else { return };
        let name_val = p.name_input.read(cx).value().trim().to_string();
        let inits_val = p.initials_input.read(cx).value().trim().to_string();

        let name_opt = if name_val.is_empty() { None } else { Some(name_val) };
        let inits_opt = if inits_val.is_empty() { None } else { Some(inits_val) };

        self.set_user_info(name_opt, inits_opt, cx);

        if let Some(s) = self.doc_state_mut() {
            s.helper.user_info = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// Reset user name and initials to system default.
    pub(crate) fn reset_user_info_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_user_info(None, None, cx);
        let def_name = crate::shell::user_name::default_user_name();
        let def_inits = crate::shell::user_name::initials(def_name);

        if let Some(p) = self.doc_state().and_then(|s| s.helper.user_info.as_ref()) {
            p.name_input.update(cx, |i, cx| i.set_value(def_name, window, cx));
            p.initials_input.update(cx, |i, cx| i.set_value(&def_inits, window, cx));
        }
        cx.notify();
    }

    /// Render the User Information panel element.
    pub(crate) fn user_info_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.user_info.as_ref()?;
        let theme = cx.theme();

        let mut body = v_flex().gap_3().p_3().p_0();

        body = body.child(
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(text("docx.user-info.description"))
        );

        // ユーザー名
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().text_color(theme.muted_foreground).child(text("docx.user-info.name")))
                .child(crate::shell::chrome::text_field(&p.name_input).full_width())
        );

        // 頭文字
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().text_color(theme.muted_foreground).child(text("docx.user-info.initials")))
                .child(crate::shell::chrome::text_field(&p.initials_input).full_width())
        );

        // アクションボタン: 適用 / 既定に戻す / キャンセル
        let actions=crate::shell::chrome::action_bar().flex_nowrap()
            .action(crate::shell::chrome::ActionDefinition::new("user-info-apply", text("docx.user-info.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e,_,_,cx|e.apply_user_info_panel(cx))))
            .action(crate::shell::chrome::ActionDefinition::new("user-info-cancel", text("docx.user-info.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,_,cx|e.close_user_info_panel(cx))))
            .action(crate::shell::chrome::ActionDefinition::new("user-info-reset", text("docx.user-info.reset-default"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,w,cx|e.reset_user_info_panel(w,cx))));


        Some(SidebarPanel::with_footer(text("docx.user-info.title"), body.into_any_element(), actions))
    }
}



