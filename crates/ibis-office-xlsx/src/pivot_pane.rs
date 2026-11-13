// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The right pane follows the current cell into a pivot table (ピボットテーブルのフィールド) and back out
//! (doc/rule/implimentationRule.md, 「右ペインの自動表示の積み」). The pane that was on show is kept in memory
//! (pushed) when the current cell enters a table and is put back (popped) when it leaves.

use gpui_kit::*;
use ibis_doc_model::SlideId;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The right pane as it was before the table's fields came over it.
pub(crate) struct Saved {
    sidebar_open: bool,
    right_key: Option<super::right_pane::RightKey>,
    custom_format: Option<super::custom_format::FormatPanel>,
    chart_edit: Option<super::chart_edit::ChartPanel>,
    validation: Option<super::validation::ValidationPanel>,
    names: Option<super::names_panel::NamesPanel>,
    find: Option<super::find::FindPanel>,
    link: Option<super::links::LinkPanel>,
    page_setup: bool,
    print_panel: bool,
    header_footer_edit: Option<super::header_footer_edit::HeaderFooterEditPanel>,
    conditional_edit: Option<super::conditional_edit::ConditionalPanel>,
    sort_edit: Option<super::sort_edit::SortPanel>,
    panels: super::edit_panels::EditPanels,
}

/// The fields pane the current cell opened by itself.
pub(crate) struct Auto {
    pub(crate) slide: SlideId,
    pub(crate) saved: Option<Saved>,
}

#[derive(Default)]
pub(crate) struct PivotPane {
    /// Set while the pane is the cell's own (not opened by the user).
    pub(crate) auto: Option<Auto>,
    /// The table whose pane the user put away (or replaced) while the cell stays in it: not opened again until the
    /// cell has been out of it.
    pub(crate) dismissed: Option<(SlideId, usize)>,
    /// The table the current cell was in when the pane was last synced (a pane is pushed on entering only).
    pub(crate) inside: Option<(SlideId, usize)>,
}

#[ibis_office_macros::editor_ext(PivotPaneExt)]
impl SlideEditor {
    fn take_saved(&mut self) -> Saved {
        let open = self.sidebar_open;
        let s = self.wb_mut();
        Saved {
            sidebar_open: open,
            right_key: s.right_key.take(),
            custom_format: s.custom_format.take(),
            chart_edit: s.chart_edit.take(),
            validation: s.validation.take(),
            names: s.names.take(),
            find: s.find.take(),
            link: s.link.take(),
            page_setup: std::mem::take(&mut s.page_setup),
            print_panel: std::mem::take(&mut s.print_panel),
            header_footer_edit: s.header_footer_edit.take(),
            conditional_edit: s.conditional_edit.take(),
            sort_edit: s.sort_edit.take(),
            panels: std::mem::take(&mut s.panels),
        }
    }

    fn put_back(&mut self, saved: Saved) {
        self.sidebar_open = saved.sidebar_open;
        let s = self.wb_mut();
        s.right_key = saved.right_key;
        s.custom_format = saved.custom_format;
        s.chart_edit = saved.chart_edit;
        s.validation = saved.validation;
        s.names = saved.names;
        s.find = saved.find;
        s.link = saved.link;
        s.page_setup = saved.page_setup;
        s.print_panel = saved.print_panel;
        s.header_footer_edit = saved.header_footer_edit;
        s.conditional_edit = saved.conditional_edit;
        s.sort_edit = saved.sort_edit;
        s.panels = saved.panels;
    }

    /// The fields pane showing a table (not a dialog of the pivot panel) now, and whose.
    fn fields_pane_target(&self) -> Option<(SlideId, usize)> {
        let p = self.sheet_state()?.panels.pivot.as_ref()?;
        if p.create.is_some() || p.change_source.is_some() || p.move_pivot.is_some() {
            return None;
        }
        p.target
    }

    /// The pane the table gave up: made when the user closed it or opened another pane over it.
    /// The kept pane is let go (the user's choice stands); the table is not opened again until the cell has left it.
    fn release_pivot_auto(&mut self, at: Option<(SlideId, usize)>) {
        let pane = &mut self.wb_mut().pivot_pane;
        pane.auto = None;
        pane.dismissed = at;
    }

    /// Called as the right pane is drawn: push on entering a table, pop on leaving it.
    pub(crate) fn sync_pivot_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_workbook() || self.show_running() || self.sheet_range_pick_active() {
            return;
        }
        let slide = self.current_slide;
        let here = self.pivot_at_selection().map(|i| (slide, i));
        let showing = self.fields_pane_target();
        let was_inside = std::mem::replace(&mut self.wb_mut().pivot_pane.inside, here);
        let auto_slide = self.wb().pivot_pane.auto.as_ref().map(|a| a.slide);
        // The cell left the table it dismissed (or is in another one): the choice is over.
        if let Some(d) = self.wb().pivot_pane.dismissed
            && here != Some(d)
        {
            self.wb_mut().pivot_pane.dismissed = None;
        }
        if auto_slide.is_some() {
            // The user closed the pane, or opened another over it: the choice stands.
            if showing.is_none() {
                self.release_pivot_auto(here);
                return;
            }
            match here {
                // Still in a table: a different table takes the pane over, nothing is pushed again.
                Some(h) => {
                    if showing != Some(h) {
                        if let Some(p) = self.wb_mut().panels.pivot.as_mut() {
                            p.target = Some(h);
                        }
                        cx.notify();
                    }
                    if let Some(a) = self.wb_mut().pivot_pane.auto.as_mut() {
                        a.slide = h.0;
                    }
                }
                // Out of every table: the pane is put back as it was.
                None => {
                    let auto = self.wb_mut().pivot_pane.auto.take();
                    self.wb_mut().panels.pivot = None;
                    self.wb_mut().panels.pivot_popup = None;
                    match auto.and_then(|a| a.saved) {
                        Some(saved) => self.put_back(saved),
                        None => self.sidebar_open = false,
                    }
                    cx.notify();
                }
            }
            return;
        }
        // Not ours: the user's own pane stays; a table's cell opens the fields when nothing of the user's is in the way.
        let Some(h) = here else { return };
        // Only entering the table pushes (a pane opened while the cell is in it is the user's).
        if was_inside == Some(h) || self.wb().pivot_pane.dismissed == Some(h) || showing.is_some() {
            return;
        }
        let saved = self.take_saved();
        let ui = super::pivot_fields::FieldsUi::new(window, cx);
        self.show_pivot_panel(super::pivot::PivotPanel { create: None, change_source: None, move_pivot: None, target: Some(h), value: None, value_name: None, field_selected: None, message: String::new(), ui: Some(ui) }, cx);
        self.wb_mut().pivot_pane.auto = Some(Auto { slide: h.0, saved: Some(saved) });
    }
}
