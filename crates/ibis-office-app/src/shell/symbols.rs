// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The characters panel shared by the formats (挿入 > 特殊文字…, Word's 記号と
//! 特殊文字, PowerPoint's 記号と特殊文字): the characters of a few groups of
//! Unicode's blocks (punctuation, currency, arrows, mathematical operators,
//! Greek, box drawing and shapes, circled and squared signs), built from the
//! blocks' code point ranges. The format keeps which group is shown and
//! says what pressing a character does (`picker`'s `pick`).

use std::rc::Rc;

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The panel's state: the group shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SymbolState {
    pub(crate) group: usize,
}

impl SlideEditor {
    /// 挿入 > 記号と特殊文字… (Word / PowerPoint / Excel): open the shared symbols panel in the sidebar.
    pub(crate) fn open_symbols(&mut self, cx: &mut Context<Self>) {
        self.symbol.get_or_insert_with(SymbolState::default);
        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.link = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            state.comments = None;
            state.ruby = None;
            state.text_box = None;
            state.merge = None;
            state.symbol.get_or_insert_with(SymbolState::default);
        }
        if let Some(state) = self.pptx_mut() {
            state.find = None;
            state.size_panel = None;
            state.section_panel = None;
            state.paragraph_panel = None;
            state.timing_panel = None;
            state.glow_panel = None;
            state.shadow_panel = None;
            state.gradient_panel = None;
            state.crop_panel = None;
            state.frame_panel = None;
            state.comments = None;
            state.chart_panel = None;
            state.header_footer = None;
            state.link_panel = None;
            state.theme_panel = None;
            state.insets_panel = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Insert symbol `c` at the caret / selection according to the active format.
    #[allow(dead_code)]
    pub(crate) fn insert_symbol(&mut self, c: char, cx: &mut Context<Self>) {
        if self.doc_state().is_some() {
            self.type_text(&c.to_string(), cx);
        } else if self.pptx().is_some() {
            if self.text_editing().is_some() {
                self.shape_type_text(&c.to_string(), cx);
            }
        } else if self.sheet_state().is_some() {
            if self.cell_edit().is_some() {
                self.cell_replace(None, &c.to_string(), None, cx);
            } else {
                self.begin_cell_edit(crate::xlsx::cell_edit::EditMode::Edit, true, cx);
                self.cell_replace(None, &c.to_string(), None, cx);
            }
        }
        cx.notify();
    }

    /// Insert symbol with window available (for PowerPoint when text editing needs to start).
    pub(crate) fn insert_symbol_with_window(&mut self, c: char, window: &mut Window, cx: &mut Context<Self>) {
        if self.doc_state().is_some() {
            self.type_text(&c.to_string(), cx);
        } else if self.pptx().is_some() {
            if self.text_editing().is_some() {
                self.shape_type_text(&c.to_string(), cx);
            } else if let Some(&shape) = self.selected_ids().first() {
                self.begin_text_edit(shape, false, false, window, cx);
                self.shape_type_text(&c.to_string(), cx);
            } else {
                let slide_sz = self.slide().size.unwrap_or(ibis_doc_model::EmuSize::new(9_144_000, 6_858_000));
                let center = ibis_doc_model::EmuPoint::new(slide_sz.width / 2 - 914_400, slide_sz.height / 2 - 457_200);
                let frame = self.presentation().default_text_box_frame(center);
                let id = self.document.ids().new_shape_id();
                let shape = ibis_doc_model::Shape::text_box(id, frame, "");
                self.insert_shape(shape, cx);
                self.begin_text_edit(id, true, false, window, cx);
                self.shape_type_text(&c.to_string(), cx);
            }
        } else if self.sheet_state().is_some() {
            if self.cell_edit().is_some() {
                self.cell_replace(None, &c.to_string(), None, cx);
            } else {
                self.begin_cell_edit(crate::xlsx::cell_edit::EditMode::Edit, true, cx);
                self.cell_replace(None, &c.to_string(), None, cx);
            }
        }
        cx.notify();
    }

    /// The shared symbols panel for the sidebar.
    pub(crate) fn symbol_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let group = self.symbol.as_ref().or_else(|| self.doc_state().and_then(|d| d.symbol.as_ref()))?.group;
        let select = Rc::new(|this: &mut SlideEditor, i: usize, cx: &mut Context<SlideEditor>| {
            if let Some(s) = this.symbol.as_mut() {
                s.group = i;
            }
            if let Some(s) = this.doc_state_mut().and_then(|s| s.symbol.as_mut()) {
                s.group = i;
            }
            cx.notify();
        });
        let pick = Rc::new(|this: &mut SlideEditor, c: char, window: &mut Window, cx: &mut Context<SlideEditor>| {
            this.insert_symbol_with_window(c, window, cx);
            this.focus_handle.focus(window, cx);
        });
        Some(SidebarPanel { title: crate::shell::i18n::text("fix.symbols.symbols-special-characters"), body: picker(group, select, pick, self.panel_dialog.bounds.map_or(px(520.), |b| b.size.width), cx) })
    }
}

/// The groups: their name (a key of the UI's texts) and the code point ranges
/// of the Unicode blocks they take their characters from.
const PICKER_INSET: f32 = 18.;

const GROUPS: &[(&str, &[(u32, u32)])] = &[
    ("fix.symbols.punctuation-symbols", &[(0xa1, 0xbf), (0x2010, 0x2027), (0x2030, 0x203e), (0x3001, 0x3003), (0x3008, 0x3011), (0x3014, 0x301f)]),
    ("_shell.currency-symbols", &[(0xa2, 0xa5), (0x20a0, 0x20bf)]),
    ("_shell.arrows", &[(0x2190, 0x21ff)]),
    ("fix.symbols.mathematical-operators", &[(0xb1, 0xb1), (0xd7, 0xd7), (0xf7, 0xf7), (0x2200, 0x22ff)]),
    ("fix.symbols.greek", &[(0x391, 0x3a1), (0x3a3, 0x3a9), (0x3b1, 0x3c9)]),
    ("fix.symbols.box-drawing-shapes", &[(0x2500, 0x257f), (0x25a0, 0x25ff)]),
    ("fix.symbols.circled-numbers-units", &[(0x2460, 0x2473), (0x2103, 0x2103), (0x2109, 0x2109), (0x3371, 0x33df)]),
];

/// The characters of group `group`.
pub(crate) fn characters(group: usize) -> Vec<char> {
    GROUPS.get(group).map_or_else(Vec::new, |(_, ranges)| ranges.iter().flat_map(|&(a, b)| (a..=b).filter_map(char::from_u32)).filter(|c| !c.is_control() && !c.is_whitespace() && *c != '\u{ad}').collect())
}

/// The names of the groups.
#[allow(dead_code)]
pub(crate) fn group_names() -> Vec<&'static str> {
    GROUPS.iter().map(|g| g.0).collect()
}

/// What pressing a character does.
pub(crate) type Pick = Rc<dyn Fn(&mut SlideEditor, char, &mut Window, &mut Context<SlideEditor>)>;

/// The panel's body: the groups across the top (`select` chooses one) and
/// the characters of group `group` below (`pick` is given the one pressed).
pub(crate) fn picker(group: usize, select: Rc<dyn Fn(&mut SlideEditor, usize, &mut Context<SlideEditor>)>, pick: Pick, dialog_width: Pixels, cx: &mut Context<SlideEditor>) -> AnyElement {
    let theme = cx.theme().clone();
    let tabs=crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic).children(GROUPS.iter().enumerate().map(|(i,(name,_))| {
        let select=select.clone();crate::shell::chrome::segment_control::SegmentItem::label(("symbol-group",i),crate::shell::i18n::text(name),i==group)
            .on_click(cx.listener(move |e,_,_,cx|select(e,i,cx)))
    }));
    let grid = h_flex().flex_wrap().gap(px(2.)).children(characters(group).into_iter().enumerate().map(|(i, c)| {
        let pick = pick.clone();
        div()
            .id(("symbol", i))
            .w(px(30.))
            .h(px(30.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .cursor_pointer()
            .text_size(px(16.))
            .hover(|s| s.bg(theme.accent))
            .child(c.to_string())
            .on_click(cx.listener(move |this, _, window, cx| pick(this, c, window, cx)))
    }));
    v_flex().gap_2().px(px(PICKER_INSET)).py(px(8.)).child(tabs).child(grid).into_any_element()
}
