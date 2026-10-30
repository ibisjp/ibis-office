// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ハイパーリンク: ⌘K (or 挿入 > リンク) opens the sidebar's panel to link
//! the selected cells to a web address or a place in the workbook
//! (`#Sheet1!A1`), shown as Excel's 「ハイパーリンク」 style; ⌘-click a
//! linked cell follows it; the panel takes a link off.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use crate::shell::palette::rgb;
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, Hyperlink, Rgb, SheetCommand, StyleChange};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// Excel's 「ハイパーリンク」 style colour (the Office theme's hyperlink).
const LINK_COLOR: u32 = 0x0563C1;

pub(crate) struct LinkPanel {
    pub(crate) address: Entity<InputState>,
    pub(crate) display_text: Entity<InputState>,
    pub(crate) tooltip: Entity<InputState>,
}

impl SlideEditor {
    /// The link of the selected cell.
    pub(crate) fn current_link(&self) -> Option<Hyperlink> {
        let (column, row) = self.current_cell()?;
        let (_, sheet) = self.current_sheet()?;
        sheet.hyperlinks.iter().find(|h| (h.range.first_row..=h.range.last_row).contains(&row) && (h.range.first_column..=h.range.last_column).contains(&column)).cloned()
    }

    pub(crate) fn open_link_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let address = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.https-or-sheet1-a1")));
        let display_text = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("xlsx.link-display-text")));
        let tooltip = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("xlsx.link-tooltip")));
        if let Some((col, row)) = self.current_cell()
            && let Some((_, sheet)) = self.current_sheet()
            && let Some(cell) = sheet.cell(row, col)
            && !cell.text.is_empty()
        {
            let cur_text = cell.text.clone();
            display_text.update(cx, |d, cx| d.set_value(cur_text, window, cx));
        }
        if let Some(link) = self.current_link() {
            let text = link.target.clone().unwrap_or_else(|| format!("#{}", link.location.clone().unwrap_or_default()));
            address.update(cx, |a, cx| a.set_value(text, window, cx));
            if let Some(tip) = link.tooltip.clone() {
                tooltip.update(cx, |t, cx| t.set_value(tip, window, cx));
            }
        }
        address.update(cx, |a, cx| a.focus(window, cx));
        if let Some(s) = self.sheet_state_mut() {
            s.names = None;
            s.validation = None;
            s.find = None;
            s.link = Some(LinkPanel { address, display_text, tooltip });
            s.page_setup = false;
            s.custom_format = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Link the selected cells to `address` (`#…` a place in the workbook),
    /// optionally setting display text and ScreenTip (`tooltip`), in one undo step.
    pub(crate) fn insert_cell_link_full(
        &mut self,
        address: &str,
        display_text: Option<&str>,
        tooltip: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.commit_cell_edit(cx);
        let address = address.trim();
        let Some(range) = self.selected_range() else { return };
        if address.is_empty() {
            return;
        }
        let tip = tooltip.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        let link = match address.strip_prefix('#') {
            Some(place) => Hyperlink { range, target: None, location: Some(place.to_string()), tooltip: tip },
            None => Hyperlink { range, target: Some(address.to_string()), location: None, tooltip: tip },
        };
        let slide = self.current_slide;
        let mut commands = vec![
            Command::Sheet(SheetCommand::SetHyperlink { slide, range, link: Some(link) }),
            Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: StyleChange::Color(Rgb(LINK_COLOR)) }),
            Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: StyleChange::Underline(true) }),
        ];
        if let Some(disp) = display_text.map(str::trim).filter(|s| !s.is_empty()) {
            if self.current_sheet().is_some_and(|(_, s)| {
                s.cell(range.first_row, range.first_column)
                    .is_none_or(|c| c.formula.is_none() && c.text != disp)
            }) {
                let edits = vec![CellEdit {
                    row: range.first_row,
                    column: range.first_column,
                    input: CellInput::Value(CellValue::Text(disp.to_string())),
                }];
                commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits }));
            }
        } else if self.current_sheet().is_some_and(|(_, s)| {
            s.cell(range.first_row, range.first_column)
                .is_none_or(|c| c.value.is_empty() && c.formula.is_none())
        }) {
            let edits = vec![CellEdit {
                row: range.first_row,
                column: range.first_column,
                input: CellInput::Value(CellValue::Text(address.trim_start_matches('#').to_string())),
            }];
            commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits }));
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("link rejected: {err}");
        }
        cx.notify();
    }

    /// Link the selected cells to `address` (`#…` a place in the workbook):
    /// one step with the link's style, and the address as the text of an
    /// empty cell.
    pub(crate) fn insert_cell_link(&mut self, address: &str, cx: &mut Context<Self>) {
        self.insert_cell_link_full(address, None, None, cx);
    }

    /// Take the links off the selected cells (Excel leaves the text and its format).
    pub(crate) fn remove_cell_link(&mut self, cx: &mut Context<Self>) {
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetHyperlink { slide, range, link: None })]) {
            log::warn!("removing a link rejected: {err}");
        }
        cx.notify();
    }

    /// ⌘-click on a linked cell: open its address, or go to its place.
    pub(crate) fn follow_cell_link(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(link) = self.current_link() else { return false };
        match (&link.target, &link.location) {
            (Some(url), _) => {
                crate::shell::links::open_external_url(url,cx);
                self.status = Some(crate::shell::i18n::format("_xlsx.opening-url", &[("url", url)]).into());
                cx.notify();
            }
            (None, Some(place)) => {
                if !self.go_to_reference(place, window, cx) {
                    self.status = Some(crate::shell::i18n::format("_xlsx.invalid-reference-place", &[("place", place)]).into());
                    cx.notify();
                }
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn cell_link_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.link.as_ref()?;
        let (address, display_text, tooltip) = (panel.address.clone(), panel.display_text.clone(), panel.tooltip.clone());
        let (a2, d2, t2) = (address.clone(), display_text.clone(), tooltip.clone());
        let current: SharedString = self.current_link().map_or(
            crate::shell::i18n::text("_xlsx.no-link").into(),
            |l| {
                let target = l.target.unwrap_or_else(|| format!("#{}", l.location.unwrap_or_default()));
                match l.tooltip {
                    Some(tip) if !tip.is_empty() => format!("{target} ({tip})").into(),
                    _ => target.into(),
                }
            },
        );

        let mut places: Vec<(String, String)> = Vec::new();
        if let Some(w) = self.presentation().workbook.as_ref() {
            for s in w.sheets.iter().filter(|s| s.slide.is_some()) {
                let ref_str = format!("#{}!A1", ibis_calc::formula::quote_sheet(&s.name));
                places.push((s.name.clone(), ref_str));
            }
            for n in w.names.iter().filter(|n| !n.hidden && !n.name.starts_with("_xlnm.")).take(6) {
                places.push((n.name.clone(), format!("#{}", n.name)));
            }
        }

        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(div().text_sm().child(current))
            .child(crate::shell::chrome::text_field(&address).full_width())
            .child(div().text_sm().text_color(rgb(0x666666)).child(crate::shell::i18n::text("xlsx.link-display-text")))
            .child(crate::shell::chrome::text_field(&display_text).full_width())
            .child(div().text_sm().text_color(rgb(0x666666)).child(crate::shell::i18n::text("xlsx.link-tooltip")))
            .child(crate::shell::chrome::text_field(&tooltip).full_width());

        if !places.is_empty() {
            let mut place_row = h_flex().gap_1().flex_wrap();
            for (idx, (label, target_ref)) in places.into_iter().enumerate() {
                let a_target = address.clone();
                place_row = place_row.child(
                    crate::shell::chrome::action_button(("link-place", idx))
                        .small()

                        .label(label)
                        .on_click(cx.listener(move |_, _, window, cx| {
                            let val = target_ref.clone();
                            a_target.update(cx, |a, cx| a.set_value(val, window, cx));
                            cx.notify();
                        })),
                );
            }
            body = body
                .child(div().text_sm().text_color(rgb(0x666666)).child(crate::shell::i18n::text("xlsx.link-places-in-workbook")))
                .child(place_row);
        }

        body = body
            ;
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("link-insert", "", crate::shell::chrome::ActionButtonKind::Primary)
                    .label(crate::shell::i18n::text("_shell.insert-link"))
                    .disabled(a2.read(cx).value().trim().is_empty())
                    .on_click(cx.listener(move |this, _, w, cx| {
                        let text = a2.read(cx).value().to_string();
                        let disp_str = d2.read(cx).value().to_string();
                        let tip_str = t2.read(cx).value().to_string();
                        let disp = (!disp_str.trim().is_empty()).then_some(disp_str.as_str());
                        let tip = (!tip_str.trim().is_empty()).then_some(tip_str.as_str());
                        this.insert_cell_link_full(&text, disp, tip, cx);
                        if this.current_link().is_some(){this.close_panel_dialog(w,cx);}
                    })))
            .action(crate::shell::chrome::dialog_cancel_definition(cx)).when(self.current_link().is_some(),|bar|bar.action(crate::shell::chrome::ActionDefinition::new("link-remove", "", crate::shell::chrome::ActionButtonKind::Destructive)
                    .label(crate::shell::i18n::text("_shell.remove-link"))
                    .on_click(cx.listener(|this, _, _, cx| this.remove_cell_link(cx)))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("insert.link"), body.into_any_element(), panel_terminal_actions))
    }
}
