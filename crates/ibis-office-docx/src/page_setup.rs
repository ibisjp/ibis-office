// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The ページ設定 dialog (Word's, one dialog with four tabs; K0063): 文字数と行数 (the direction, the columns, the grid of
//! characters and lines), 余白 (margins, とじしろ, the orientation, multiple pages), 用紙 (the paper) and その他 (the header and
//! footer's distances, the vertical alignment, the first page number), applied to the whole document or to the caret's section
//! as one step of undo.
//!
//! The grid's numbers are linked as in Word: the number of characters (lines) along the text area and the pitch of one character
//! (line) give each other (`pitch = area / count`, `count = floor(area / pitch)`), so changing one sets the other.
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellDirection, DocGridType, GenkoPaper, GenkoStyle, MultiplePages, PageVAlign, Rgb, EMU_PER_POINT};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::ext::*;
use crate::shell::chrome::{radio_button, radio_group, RadioGroupLayout, SidebarPanel};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

const EMU_PER_MM: f64 = 36_000.0;
const EMU_PER_TWIP: i64 = 635;

/// The tabs, in the order of the dialog.
pub const TAB_GRID: usize = 0;
pub const TAB_MARGINS: usize = 1;
pub const TAB_PAPER: usize = 2;
pub const TAB_OTHER: usize = 3;

/// The fields, in millimetres: top, bottom, left, right, gutter, header, footer, width, height.
pub(crate) const FIELDS: [&str; 9] = ["_shell.top", "_shell.bottom", "fix.help-panels.left-inside", "fix.help-panels.right-outside", "fix.help-panels.gutter", "_shell.header", "_shell.footer", "_shell.width", "_shell.height"];

/// The paper sizes of the list (the name, and the width and height in twips, portrait).
pub(crate) const PAPERS: [(&str, i64, i64); 7] = [
    ("page.paper-a3", 16_838, 23_811),
    ("page.paper-a4", 11_906, 16_838),
    ("docx.page-setup.paper-a5", 8_392, 11_906),
    ("docx.page-setup.paper-b4", 14_570, 20_639),
    ("page.paper-b5", 10_318, 14_570),
    ("docx.page.letter", 12_240, 15_840),
    ("docx.page-setup.paper-legal", 12_240, 20_160),
];

/// The four numbers of the grid that follow each other.
const LINK_CHARS: usize = 0;
const LINK_CHAR_PITCH: usize = 1;
const LINK_LINES: usize = 2;
const LINK_LINE_PITCH: usize = 3;

pub struct PageSetupPanel {
    pub fields: [Entity<InputState>; 9],
    pub tab: usize,
    pub(crate) gutter_at_top: bool,
    pub(crate) mirror: bool,
    pub(crate) multiple_pages: MultiplePages,
    pub(crate) book_fold_sheets: Entity<InputState>,
    pub(crate) v_align: PageVAlign,
    /// 文字列の方向: across, or 縦書き (`TbRl`).
    pub vertical: bool,
    /// 印刷の向き: the paper's long side across.
    pub landscape: bool,
    /// 適用先: every section of the document (else the caret's section).
    pub apply_all: bool,
    /// 段数.
    pub columns: Entity<InputState>,
    /// The page number the section starts at (blank: it goes on from the section before).
    pub(crate) start: Entity<InputState>,
    /// 文字数と行数の指定.
    pub(crate) grid_type: DocGridType,
    pub(crate) lines: Entity<InputState>,
    pub(crate) line_pitch: Entity<InputState>,
    pub(crate) chars: Entity<InputState>,
    /// 字送り: the pitch of one character in pt (the font's size and `w:charSpace`).
    pub(crate) char_space: Entity<InputState>,
    /// The size of the characters the grid adds its pitch to (the document's default), EMU.
    pub(crate) base_pitch: i64,
    /// 原稿用紙のスタイル (`None`: 原稿用紙の設定にしない), 罫線の色 and the paper's name Word writes in the drawing.
    pub(crate) genko: Option<GenkoStyle>,
    pub(crate) genko_color: Rgb,
    pub(crate) genko_paper: String,
    /// The margins and counts of 原稿用紙 are set (once, when a style is first chosen).
    pub(crate) genko_seen: bool,
    /// The text a link set in each of the four grid fields, so the change event of that very text is not taken as the user's.
    /// The lengths the dialog was opened on (EMU) and their text: a field left as it was keeps the exact length (the text shows a tenth of a millimetre).
    opened: [(i64, String); 9],
    self_set: Rc<RefCell<[Option<String>; 4]>>,
    #[allow(dead_code)] // Kept alive: the subscriptions end with the panel.
    subs: Vec<Subscription>,
    pub(crate) message: String,
    pub ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

fn mm(emu: i64) -> String {
    let v = (emu as f64 / EMU_PER_MM * 10.0).round() / 10.0;
    if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

/// A length in pt, two decimals, without the trailing zeros.
pub(crate) fn pt_str(emu: i64) -> String {
    let v = (emu as f64 / EMU_PER_POINT as f64 * 100.0).round() / 100.0;
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else if (v * 10.0).fract() == 0.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

fn mm_to_emu(mm: f64) -> i64 {
    (mm * EMU_PER_MM).round() as i64
}

fn twip_round(emu: i64) -> i64 {
    (emu as f64 / EMU_PER_TWIP as f64).round() as i64 * EMU_PER_TWIP
}

/// The text area of a page along its lines (where the characters are) and across them (where the lines are), EMU: for across
/// writing the width and the height of the text area, for 縦書き the height and the width.
pub(crate) fn grid_area(width: i64, height: i64, top: i64, bottom: i64, left: i64, right: i64, gutter: i64, gutter_at_top: bool, vertical: bool) -> (i64, i64) {
    let w = width - left - right - if !gutter_at_top { gutter } else { 0 };
    let h = height - top - bottom - if gutter_at_top { gutter } else { 0 };
    let (along, across) = if vertical { (h, w) } else { (w, h) };
    (along.max(1), across.max(1))
}

/// The number of characters (or lines) of `pitch` in `area`: Word's whole number; the small addition keeps a pitch shown with two
/// decimals (`area / count` rounded) from counting one too few.
pub(crate) fn count_in(area: i64, pitch: i64) -> i64 {
    ((area as f64 / pitch.max(1) as f64) + 0.05).floor().max(1.0) as i64
}

/// `w:charSpace` for a character pitch of `pitch` EMU: the pitch less the characters' size (`base_pitch`), in whole 1/4096 pt (as the
/// reader converts it), returned in EMU.
fn char_space_of(pitch: f64, base_pitch: i64) -> i64 {
    (((pitch - base_pitch as f64) * 4096.0 / EMU_PER_POINT as f64).round() as i64) * EMU_PER_POINT / 4096
}

fn parse_number(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

#[ibis_office_macros::editor_ext(PageSetupExt)]
impl SlideEditor {
    /// Open the dialog on 余白 (Word's ページ設定…).
    pub fn open_page_setup_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_page_setup_tab(TAB_MARGINS, window, cx);
    }

    /// Open the dialog on a tab (`TAB_GRID` for 文字数と行数…).
    pub fn open_page_setup_tab(&mut self, tab: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.tab_edit = None;
        }
        let Some(page) = self.caret_page() else { return };
        let base_pitch = self
            .flow()
            .map(|d| d.styles.paragraph_base(None, None).1.size.unwrap_or(1000) as i64 * (EMU_PER_POINT / 100))
            .unwrap_or(10 * EMU_PER_POINT);
        let single_section = self.flow().map_or(true, |d| ibis_doc_model::flow_command::section_state(d, &ibis_doc_model::flow_command::section_paragraphs(d)).breaks.is_empty());
        let lengths = [page.top, page.bottom, page.left, page.right, page.gutter, page.header, page.footer, page.size.width, page.size.height];
        let values = lengths.map(mm);
        let opened: [(i64, String); 9] = std::array::from_fn(|i| (lengths[i], values[i].clone()));
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

        let vertical = page.direction == CellDirection::TbRl;
        let (along, across) = grid_area(page.size.width, page.size.height, page.top, page.bottom, page.left, page.right, page.gutter, page.gutter_at_top, vertical);
        let line_pitch_emu = page.line_pitch.unwrap_or(EMU_PER_POINT * 18);
        let char_pitch_emu = base_pitch + page.char_space.unwrap_or(0);
        let initial = [
            count_in(along, char_pitch_emu).to_string(),
            pt_str(char_pitch_emu),
            count_in(across, line_pitch_emu).to_string(),
            pt_str(line_pitch_emu),
        ];
        let [chars, char_space, lines, line_pitch] = initial.clone().map(|v| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(v, window, cx));
            state
        });
        let columns = cx.new(|cx| InputState::new(window, cx));
        columns.update(cx, |i, cx| i.set_value(page.columns.count.to_string(), window, cx));

        let grid_type = if page.doc_grid_type == DocGridType::Default && page.line_pitch.is_some() { DocGridType::Lines } else { page.doc_grid_type };
        // The values just put in the four fields are not the user's edits (an event of one may still come).
        let self_set = Rc::new(RefCell::new(initial.map(Some)));
        let mut subs = Vec::new();
        for (kind, field) in [(LINK_CHARS, &chars), (LINK_CHAR_PITCH, &char_space), (LINK_LINES, &lines), (LINK_LINE_PITCH, &line_pitch)] {
            subs.push(cx.subscribe_in(field, window, move |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.page_setup_link(Some(kind), window, cx);
                }
            }));
        }
        // The margins and the paper decide how many characters and lines the pitches make.
        for field in [&fields[0], &fields[1], &fields[2], &fields[3], &fields[4], &fields[7], &fields[8]] {
            subs.push(cx.subscribe_in(field, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.page_setup_link(None, window, cx);
                }
            }));
        }

        let panel = PageSetupPanel {
            fields,
            tab: tab.min(TAB_OTHER),
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
            vertical,
            landscape: page.size.width > page.size.height,
            apply_all: single_section,
            columns,
            start,
            grid_type,
            genko: self.flow().and_then(|d| d.genko.as_ref()).map(|g| g.style),
            genko_color: self.flow().and_then(|d| d.genko.as_ref()).map_or(GenkoPaper::DEFAULT_COLOR, |g| g.color),
            genko_seen: self.flow().is_some_and(|d| d.genko.is_some()),
            genko_paper: self.flow().and_then(|d| d.genko.as_ref()).map_or_else(|| "A4".to_string(), |g| g.paper.clone()),
            lines,
            line_pitch,
            chars,
            char_space,
            base_pitch,
            opened,
            self_set,
            subs,
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
            s.helper = crate::help_panels::HelpPanels::default();
            s.helper.page_setup = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// A number of the grid changed (`Some(kind)`), or the text area did (`None`): set the number that follows.
    fn page_setup_link(&mut self, kind: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.page_setup.as_ref()) else { return };
        let fields = [p.chars.clone(), p.char_space.clone(), p.lines.clone(), p.line_pitch.clone()];
        let self_set = p.self_set.clone();
        if let Some(kind) = kind {
            let now = fields[kind].read(cx).value().to_string();
            let mut set = self_set.borrow_mut();
            if set[kind].as_deref() == Some(now.as_str()) {
                set[kind] = None;
                return;
            }
        }
        let Some((along, across)) = p.grid_area_now(cx) else { return };
        let read = |i: usize| parse_number(fields[i].read(cx).value().as_ref());
        // (the field to set, its text)
        let mut sets: Vec<(usize, String)> = Vec::new();
        let pitch_emu = |pt: f64| (pt * EMU_PER_POINT as f64).round() as i64;
        match kind {
            Some(LINK_CHARS) => {
                if let Some(n) = read(LINK_CHARS).filter(|n| *n >= 1.0) {
                    sets.push((LINK_CHAR_PITCH, pt_str((along as f64 / n.floor()).round() as i64)));
                }
            }
            Some(LINK_LINES) => {
                if let Some(n) = read(LINK_LINES).filter(|n| *n >= 1.0) {
                    sets.push((LINK_LINE_PITCH, pt_str((across as f64 / n.floor()).round() as i64)));
                }
            }
            Some(LINK_CHAR_PITCH) => {
                if let Some(pt) = read(LINK_CHAR_PITCH).filter(|v| *v > 0.0) {
                    sets.push((LINK_CHARS, count_in(along, pitch_emu(pt)).to_string()));
                }
            }
            Some(LINK_LINE_PITCH) => {
                if let Some(pt) = read(LINK_LINE_PITCH).filter(|v| *v > 0.0) {
                    sets.push((LINK_LINES, count_in(across, pitch_emu(pt)).to_string()));
                }
            }
            // The text area changed: the pitches stay, the counts follow.
            _ => {
                if let Some(pt) = read(LINK_CHAR_PITCH).filter(|v| *v > 0.0) {
                    sets.push((LINK_CHARS, count_in(along, pitch_emu(pt)).to_string()));
                }
                if let Some(pt) = read(LINK_LINE_PITCH).filter(|v| *v > 0.0) {
                    sets.push((LINK_LINES, count_in(across, pitch_emu(pt)).to_string()));
                }
            }
        }
        for (target, value) in sets {
            if fields[target].read(cx).value().as_ref() == value.as_str() {
                continue;
            }
            self_set.borrow_mut()[target] = Some(value.clone());
            fields[target].update(cx, |i, cx| i.set_value(value, window, cx));
        }
    }

    /// 印刷の向き: the paper's width and height of the panel follow.
    fn page_setup_orientation(&mut self, landscape: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state_mut().and_then(|s| s.helper.page_setup.as_mut()) else { return };
        p.landscape = landscape;
        let (w, h) = (p.fields[7].clone(), p.fields[8].clone());
        let (wv, hv) = (w.read(cx).value().to_string(), h.read(cx).value().to_string());
        if let (Some(a), Some(b)) = (parse_number(&wv), parse_number(&hv)) {
            if (landscape && a < b) || (!landscape && a > b) {
                w.update(cx, |i, cx| i.set_value(hv, window, cx));
                h.update(cx, |i, cx| i.set_value(wv, window, cx));
            }
        }
        cx.notify();
    }

    /// 用紙サイズ: a paper of the list (its width and height, across or down as the orientation).
    fn page_setup_paper(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state_mut().and_then(|s| s.helper.page_setup.as_mut()) else { return };
        let Some(&(_, w, h)) = PAPERS.get(index) else { return };
        let (w, h) = (w * EMU_PER_TWIP, h * EMU_PER_TWIP);
        let (w, h) = if p.landscape { (h, w) } else { (w, h) };
        p.fields[7].update(cx, |i, cx| i.set_value(mm(w), window, cx));
        p.fields[8].update(cx, |i, cx| i.set_value(mm(h), window, cx));
        cx.notify();
    }

    /// 原稿用紙のスタイル: Word's 原稿用紙設定 takes the margins it draws on (25.4 mm above and below, 30 mm at the sides), the
    /// grid of characters and lines (20 × 20 at first), and a 縦書き page across the paper.
    fn page_setup_genko(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state_mut().and_then(|s| s.helper.page_setup.as_mut()) else { return };
        if p.genko.is_none() {
            p.genko_seen = false;
            return cx.notify();
        }
        p.grid_type = DocGridType::SnapToChars;
        let seeded = !p.genko_seen;
        if !p.genko_seen {
            p.genko_seen = true;
            // Word's margins (twips), the header and footer at the paper's edge, and no gutter.
            let twips = [1440, 1440, 1700, 1700, 0, 0, 0];
            for (i, tw) in twips.into_iter().enumerate() {
                let emu = tw * EMU_PER_TWIP;
                let text = mm(emu);
                p.opened[i] = (emu, text.clone());
                p.fields[i].update(cx, |f, cx| f.set_value(text, window, cx));
            }
            p.gutter_at_top = false;
            for field in [p.chars.clone(), p.lines.clone()] {
                field.update(cx, |f, cx| f.set_value("20", window, cx));
            }
        }
        // A 縦書き page lies across the paper.
        let landscape = p.vertical || p.landscape;
        self.page_setup_orientation(landscape, window, cx);
        if seeded {
            // The counts (20 × 20) were just set: the pitches follow them (not the other way round, as for a changed text area).
            self.page_setup_link(Some(LINK_CHARS), window, cx);
            self.page_setup_link(Some(LINK_LINES), window, cx);
        } else {
            self.page_setup_link(None, window, cx);
        }
        cx.notify();
    }

    fn page_setup_message(&mut self, message: gpui_kit::SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.page_setup.as_mut()) {
            p.message = message.to_string();
        }
        cx.notify();
    }

    /// Change the page setup of the caret's section, or of every section of the document (one step of undo).
    pub(crate) fn set_page_scoped(&mut self, all: bool, mut extra: Vec<ibis_doc_model::FlowCommand>, change: impl Fn(&mut ibis_doc_model::PageSetup), cx: &mut Context<Self>) {
        if !all {
            // The caret's section only: the commands that go with it follow as their own step.
            self.set_page(|p| change(p), cx);
            if !extra.is_empty() {
                self.run_flow(std::mem::take(&mut extra), cx);
            }
            return;
        }
        let Some(doc) = self.flow() else { return };
        let mut state = ibis_doc_model::flow_command::section_state(doc, &ibis_doc_model::flow_command::section_paragraphs(doc));
        let before = state.clone();
        change(&mut state.page);
        for (_, _, setup) in state.breaks.iter_mut() {
            if let Some(setup) = setup {
                change(&mut std::sync::Arc::make_mut(setup).page);
            }
        }
        if state != before {
            extra.insert(0, ibis_doc_model::FlowCommand::SetSections { state: Box::new(state) });
            self.run_flow(extra, cx);
        } else if !extra.is_empty() {
            self.run_flow(extra, cx);
        }
    }

    pub(crate) fn apply_page_setup_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.page_setup.as_ref()) else { return };
        let mut v = [0i64; 9];
        for (i, f) in p.fields.iter().enumerate() {
            let shown = f.read(cx).value().trim().to_string();
            // A length left as the dialog showed it stays exact (the paper's two sides may have changed places).
            let same = |j: usize| p.opened[j].1 == shown;
            if same(i) {
                v[i] = p.opened[i].0;
                continue;
            }
            if i >= 7 && same(15 - i) {
                v[i] = p.opened[15 - i].0;
                continue;
            }
            match shown.parse::<f64>() {
                Ok(x) if x >= 0.0 && x.is_finite() => v[i] = mm_to_emu(x),
                _ => return self.page_setup_message(crate::shell::i18n::format("page-setup.bad", &[("field", &text(FIELDS[i]))]).into(), cx),
            }
        }
        let [top, bottom, left, right, gutter, header, footer, mut width, mut height] = v;
        // 印刷の向き decides which side is across.
        if (p.landscape && width < height) || (!p.landscape && width > height) {
            std::mem::swap(&mut width, &mut height);
        }
        // The text area must keep some room (Word refuses margins that meet).
        let across = if p.gutter_at_top { left + right } else { left + right + gutter };
        let down = if p.gutter_at_top { top + bottom + gutter } else { top + bottom };
        if width <= 0 || height <= 0 || across >= width - 36_000 * 10 || down >= height - 36_000 * 10 {
            return self.page_setup_message(text("fix.help-panels.margins-too-large-paper"), cx);
        }
        let (gutter_at_top, mirror, multiple_pages, v_align, vertical, apply_all) = (p.gutter_at_top, p.mirror, p.multiple_pages, p.v_align, p.vertical, p.apply_all);
        let base_pitch = p.base_pitch;
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
        let columns = match p.columns.read(cx).value().trim().parse::<u8>() {
            Ok(n) if n >= 1 && n as usize <= ibis_doc_model::PageColumns::MAX => n,
            _ => return self.page_setup_message(text("docx.page-setup.bad-columns"), cx),
        };

        let grid_type = p.grid_type;
        let (_, across_area) = grid_area(width, height, top, bottom, left, right, gutter, gutter_at_top, vertical);
        let line_pitch_of = |this: &Self, cx: &Context<Self>| -> Option<i64> {
            let p = this.doc_state().and_then(|s| s.helper.page_setup.as_ref())?;
            match parse_number(p.line_pitch.read(cx).value().as_ref()) {
                Some(pt) if pt > 0.0 => Some(twip_round((pt * EMU_PER_POINT as f64).round() as i64).max(EMU_PER_TWIP)),
                _ => {
                    let n = p.lines.read(cx).value().trim().parse::<u32>().ok().filter(|n| *n >= 1)?;
                    Some(twip_round(across_area / n as i64).max(EMU_PER_TWIP))
                }
            }
        };
        let genko = p.genko;
        let (genko_color, genko_paper) = (p.genko_color, p.genko_paper.clone());
        let count = |this: &Self, cx: &Context<Self>, chars: bool| -> Option<i64> {
            let p = this.doc_state().and_then(|s| s.helper.page_setup.as_ref())?;
            let field = if chars { &p.chars } else { &p.lines };
            parse_number(field.read(cx).value().as_ref()).filter(|n| *n >= 1.0 && n.fract() == 0.0).map(|n| n as i64)
        };
        let (genko_chars, genko_lines) = (count(self, cx, true), count(self, cx, false));
        if genko.is_some() && (genko_chars.is_none() || genko_lines.is_none()) {
            return self.page_setup_message(text("docx.help-panels.enter-whole-number-1-more"), cx);
        }
        let (along_area, _) = grid_area(width, height, top, bottom, left, right, gutter, gutter_at_top, vertical);
        let (line_pitch, char_space) = if let (Some(_), Some(n_chars), Some(n_lines)) = (genko, genko_chars, genko_lines) {
            // 原稿用紙: the pitch of a line is Word's whole twips of the area's share, a character's pitch the area's share less the
            // characters' size, in 1/4096 pt.
            let pitch = (across_area / EMU_PER_TWIP / n_lines).max(1) * EMU_PER_TWIP;
            (Some(pitch), Some(char_space_of(along_area as f64 / n_chars as f64, base_pitch)))
        } else {
            match grid_type {
            DocGridType::Default => (None, None),
            DocGridType::Lines => match line_pitch_of(self, cx) {
                Some(pitch) => (Some(pitch), None),
                None => return self.page_setup_message(text("docx.help-panels.please-enter-positive-number-line"), cx),
            },
            DocGridType::LinesAndChars | DocGridType::SnapToChars => {
                let Some(pitch) = line_pitch_of(self, cx) else {
                    return self.page_setup_message(text("docx.help-panels.please-enter-positive-number-line"), cx);
                };
                let Some(char_pt) = self.doc_state().and_then(|s| s.helper.page_setup.as_ref()).and_then(|p| parse_number(p.char_space.read(cx).value().as_ref())).filter(|v| *v > 0.0) else {
                    return self.page_setup_message(text("docx.page-setup.bad-char-pitch"), cx);
                };
                // `w:charSpace` is the pitch less the characters' size, in 1/4096 pt (as the reader converts it).
                (Some(pitch), Some(char_space_of(char_pt * EMU_PER_POINT as f64, base_pitch)))
            }
            }
        };
        let grid_type = if genko.is_some() { DocGridType::SnapToChars } else { grid_type };

        let had_genko = self.flow().is_some_and(|d| d.genko.is_some());
        let new_genko = genko.zip(genko_chars).zip(genko_lines).map(|((style, chars), lines)| GenkoPaper { style, chars: chars as u32, lines: lines as u32, color: genko_color, paper: genko_paper });
        // The drawing is in the header: a document without one gets it.
        let mut extra = Vec::new();
        if new_genko.is_some() && self.flow().is_some_and(|d| d.header.is_empty()) {
            extra.extend(self.add_header_commands(false));
        }
        if new_genko.is_some() || had_genko {
            extra.push(ibis_doc_model::FlowCommand::SetGenko { genko: new_genko });
        }
        self.set_page_scoped(
            apply_all,
            extra,
            move |page| {
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
                if vertical != (page.direction == CellDirection::TbRl) {
                    page.direction = if vertical { CellDirection::TbRl } else { CellDirection::LrTb };
                }
                if page.columns.count != columns {
                    page.columns.count = columns;
                    page.columns.widths = None;
                    if columns == 1 {
                        page.columns.separator = false;
                    }
                    if page.columns.space == 0 {
                        page.columns.space = 720 * EMU_PER_TWIP;
                    }
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

    pub fn doc_page_setup_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
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
                radio_button(id, on, false, text(label), cx.listener(move |this, _, window, cx| {
                    if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.page_setup.as_mut()) {
                        set(p);
                    }
                    // The direction decides which side the characters run along.
                    this.page_setup_genko(window, cx);
                    this.page_setup_link(None, window, cx);
                    cx.notify();
                }))
                .into_any_element(),
            )
        };
        let orientation = |id: &'static str, landscape: bool, label: &str| {
            mark(
                id,
                radio_button(id, p.landscape == landscape, false, text(label), cx.listener(move |this, _, window, cx| this.page_setup_orientation(landscape, window, cx))).into_any_element(),
            )
        };
        // Every field here is a number; the dialog applies them with its button, so ▲▼ only change the number.
        use crate::shell::chrome::SpinnerSpec;
        let field_cx = &*cx;
        let aligned_row = |mark_id: &str, label_text: gpui_kit::SharedString, input: &Entity<InputState>, unit_text: gpui_kit::SharedString, spec: SpinnerSpec, disabled: bool| {
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().text_sm().child(label_text))
                .child(mark(mark_id, crate::shell::chrome::text_field(input).disabled(disabled).spinner(format!("page-setup-{mark_id}"), spec, field_cx, |_, _, _, _| {}).unit(unit_text, px(72.)).into_any_element()))
        };
        let field = |i: usize| aligned_row(&format!("field-{i}"), text(FIELDS[i]), &p.fields[i], "mm".into(), SpinnerSpec::new(1.0).range(if i >= 7 { 1.0 } else { 0.0 }, 558.0), false);
        let heading = |key: &str| div().text_sm().child(text(key));
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0().children((!p.message.is_empty()).then(|| div().text_sm().child(gpui_kit::SharedString::from(p.message.clone()))));
        match p.tab {
            TAB_GRID => {
                let chars_on = matches!(p.grid_type, DocGridType::LinesAndChars | DocGridType::SnapToChars);
                let lines_on = p.grid_type != DocGridType::Default;
                body = body
                    .child(heading("docx.chrome.text-direction"))
                    .child(
                        radio_group().layout(RadioGroupLayout::Flow)
                            .child(choice("direction-across", !p.vertical, "_shell.horizontal", |p| p.vertical = false))
                            .child(choice("direction-vertical", p.vertical, "_shell.vertical", |p| p.vertical = true)),
                    )
                    .child(aligned_row("columns", text("docx.page-setup.columns"), &p.columns, text("docx.page-setup.columns-unit"), SpinnerSpec::new(1.0).range(1.0, ibis_doc_model::PageColumns::MAX as f32).integer_only(), false))
                    .child(heading("docx.page-setup.grid-spec"))
                    .child(
                        radio_group().layout(RadioGroupLayout::Column)
                            .child(choice("grid-default", p.grid_type == DocGridType::Default, "docx.page-setup.grid-default", |p| p.grid_type = DocGridType::Default))
                            .child(choice("grid-lines", p.grid_type == DocGridType::Lines, "docx.page-setup.grid-lines", |p| p.grid_type = DocGridType::Lines))
                            .child(choice("grid-lines-chars", p.grid_type == DocGridType::LinesAndChars, "docx.page-setup.grid-lines-chars", |p| p.grid_type = DocGridType::LinesAndChars))
                            .child(choice("grid-snap", p.grid_type == DocGridType::SnapToChars, "docx.page-setup.grid-snap", |p| p.grid_type = DocGridType::SnapToChars)),
                    )
                    .child(heading("docx.page-setup.genko-style"))
                    .child(
                        radio_group().layout(RadioGroupLayout::Column)
                            .child(choice("genko-none", p.genko.is_none(), "docx.page-setup.genko-none", |p| p.genko = None))
                            .child(choice("genko-grid", p.genko == Some(GenkoStyle::Grid), "docx.page-setup.genko-grid", |p| p.genko = Some(GenkoStyle::Grid)))
                            .child(choice("genko-underline", p.genko == Some(GenkoStyle::Underline), "docx.page-setup.genko-underline", |p| p.genko = Some(GenkoStyle::Underline)))
                            .child(choice("genko-frame", p.genko == Some(GenkoStyle::Frame), "docx.page-setup.genko-frame", |p| p.genko = Some(GenkoStyle::Frame))),
                    )
                    .children(p.genko.map(|_| {
                        v_flex().gap_1().child(heading("docx.page-setup.genko-color")).child(
                            radio_group().layout(RadioGroupLayout::Flow)
                                .child(choice("genko-color-green", p.genko_color == GenkoPaper::DEFAULT_COLOR, "docx.page-setup.genko-color-green", |p| p.genko_color = GenkoPaper::DEFAULT_COLOR))
                                .child(choice("genko-color-black", p.genko_color == Rgb(0x000000), "docx.page-setup.genko-color-black", |p| p.genko_color = Rgb(0x000000)))
                                .child(choice("genko-color-red", p.genko_color == Rgb(0xFF0000), "docx.page-setup.genko-color-red", |p| p.genko_color = Rgb(0xFF0000)))
                                .child(choice("genko-color-blue", p.genko_color == Rgb(0x0000FF), "docx.page-setup.genko-color-blue", |p| p.genko_color = Rgb(0x0000FF)))
                                .child(choice("genko-color-gray", p.genko_color == Rgb(0x808080), "docx.page-setup.genko-color-gray", |p| p.genko_color = Rgb(0x808080))),
                        )
                    }))
                    .child(aligned_row("grid-chars", text("docx.help-panels.characters"), &p.chars, text("docx.paragraph-panel.characters"), SpinnerSpec::new(1.0).range(1.0, 999.0).integer_only(), !chars_on))
                    .child(aligned_row("grid-char-space", text("docx.help-panels.character-pitch"), &p.char_space, "pt".into(), SpinnerSpec::new(0.5).range(0.0, 1584.0), !chars_on))
                    .child(aligned_row("grid-lines-input", text("docx.help-panels.lines"), &p.lines, text("_xlsx.row"), SpinnerSpec::new(1.0).range(1.0, 999.0).integer_only(), !lines_on))
                    .child(aligned_row("grid-line-pitch", text("docx.help-panels.line-pitch"), &p.line_pitch, "pt".into(), SpinnerSpec::new(0.5).range(0.0, 1584.0), !lines_on));
            }
            TAB_MARGINS => {
                for i in 0..5 {
                    body = body.child(field(i));
                }
                body = body
                    .child(v_flex().gap_1().child(heading("fix.help-panels.gutter-position")).child(
                        radio_group().layout(RadioGroupLayout::Flow)
                            .child(choice("gutter-left", !p.gutter_at_top, "_shell.left", |p| p.gutter_at_top = false))
                            .child(choice("gutter-top", p.gutter_at_top, "_shell.top", |p| p.gutter_at_top = true)),
                    ))
                    .child(heading("docx.page-setup.orientation"))
                    .child(
                        radio_group().layout(RadioGroupLayout::Flow)
                            .child(orientation("orientation-portrait", false, "docx.page.portrait"))
                            .child(orientation("orientation-landscape", true, "docx.page.landscape")),
                    )
                    .child(heading("docx.page.multiple-pages"))
                    .child(
                        radio_group().layout(RadioGroupLayout::Flow)
                            .child(choice("page-normal", p.multiple_pages == MultiplePages::Normal, "docx.page.multiple-pages.normal", |p| {
                                p.multiple_pages = MultiplePages::Normal;
                                p.mirror = false;
                            }))
                            .child(choice("page-mirror", p.multiple_pages == MultiplePages::MirrorMargins, "docx.page.multiple-pages.mirror-margins", |p| {
                                p.multiple_pages = MultiplePages::MirrorMargins;
                                p.mirror = true;
                            }))
                            .child(choice("page-book-fold", p.multiple_pages == MultiplePages::BookFold, "docx.page.multiple-pages.book-fold", |p| {
                                p.multiple_pages = MultiplePages::BookFold;
                                p.mirror = true;
                            }))
                            .child(choice("page-reverse-book-fold", p.multiple_pages == MultiplePages::ReverseBookFold, "docx.page.multiple-pages.reverse-book-fold", |p| {
                                p.multiple_pages = MultiplePages::ReverseBookFold;
                                p.mirror = true;
                            }))
                            .child(choice("page-two-on-one", p.multiple_pages == MultiplePages::TwoOnOne, "docx.page.multiple-pages.two-on-one", |p| {
                                p.multiple_pages = MultiplePages::TwoOnOne;
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
                        false,
                    ));
                }
            }
            TAB_PAPER => {
                // The paper of the list the width and height make (either way round), else 「ユーザー設定」.
                let size_now = (parse_number(p.fields[7].read(cx).value().as_ref()), parse_number(p.fields[8].read(cx).value().as_ref()));
                let current = size_now.0.zip(size_now.1).and_then(|(a, b)| {
                    let (short, long) = (a.min(b), a.max(b));
                    PAPERS.iter().position(|&(_, w, h)| ((w * EMU_PER_TWIP) as f64 / EMU_PER_MM - short).abs() < 0.6 && ((h * EMU_PER_TWIP) as f64 / EMU_PER_MM - long).abs() < 0.6)
                });
                let items = PAPERS
                    .iter()
                    .enumerate()
                    .map(|(i, &(key, _, _))| {
                        crate::shell::chrome::DropdownItem::new(text(key)).checked(current == Some(i)).on_select(Rc::new(move |e: &mut SlideEditor, w, cx| e.page_setup_paper(i, w, cx)))
                    })
                    .collect();
                let label = text(current.map_or("docx.page-setup.paper-custom", |i| PAPERS[i].0));
                body = body
                    .child(crate::shell::chrome::dropdown_row(text("docx.page-setup.paper-kind"), crate::shell::chrome::Dropdown::new("page-setup-paper", label, items, &cx.weak_entity()).fill_width()))
                    .child(field(7))
                    .child(field(8));
            }
            _ => {
                body = body.child(heading("fix.help-panels.header-footer-edge")).child(field(5)).child(field(6));
                body = body.child(heading("_shell.vertical-alignment")).child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(choice("valign-top", p.v_align == PageVAlign::Top, "fix.help-panels.top", |p| p.v_align = PageVAlign::Top))
                        .child(choice("valign-center", p.v_align == PageVAlign::Center, "fix.help-panels.center", |p| p.v_align = PageVAlign::Center))
                        .child(choice("valign-bottom", p.v_align == PageVAlign::Bottom, "fix.help-panels.bottom", |p| p.v_align = PageVAlign::Bottom))
                        .child(choice("valign-both", p.v_align == PageVAlign::Both, "_shell.justify", |p| p.v_align = PageVAlign::Both)),
                );
                body = body
                    .child(heading("_xlsx.page-number"))
                    .child(aligned_row("start", text("fix.help-panels.start"), &p.start, text("docx.help-panels.blank-continue"), SpinnerSpec::new(1.0).range(1.0, 32767.0).integer_only(), false));
            }
        }
        // 適用先 (every tab).
        body = body.child(heading("docx.page-setup.apply-to")).child(
            radio_group().layout(RadioGroupLayout::Flow)
                .child(choice("scope-all", p.apply_all, "docx.page-setup.apply-all", |p| p.apply_all = true))
                .child(choice("scope-section", !p.apply_all, "docx.page-setup.apply-section", |p| p.apply_all = false)),
        );
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
        let tab = p.tab;
        Some(
            SidebarPanel::with_footer(text("_shell.page-setup"), body.into_any_element(), footer).header_tabs(
                "page-setup-tabs",
                [text("docx.help-panels.document-grid"), text("_shell.margins"), text("_shell.paper-size"), text("docx.page-setup.tab-other")],
                tab,
                |e, i, cx| {
                    if let Some(p) = e.doc_state_mut().and_then(|s| s.helper.page_setup.as_mut()) {
                        p.tab = i;
                    }
                    cx.notify();
                },
                cx,
            ),
        )
    }
}

impl PageSetupPanel {
    /// The text area along the lines and across them now (EMU), from the fields as they stand (`None` while one is not a number).
    fn grid_area_now(&self, cx: &App) -> Option<(i64, i64)> {
        let mut v = [0i64; 9];
        for (i, f) in self.fields.iter().enumerate() {
            v[i] = mm_to_emu(parse_number(f.read(cx).value().as_ref()).filter(|x| *x >= 0.0)?);
        }
        let [top, bottom, left, right, gutter, _, _, mut width, mut height] = v;
        if (self.landscape && width < height) || (!self.landscape && width > height) {
            std::mem::swap(&mut width, &mut height);
        }
        Some(grid_area(width, height, top, bottom, left, right, gutter, self.gutter_at_top, self.vertical))
    }
}
