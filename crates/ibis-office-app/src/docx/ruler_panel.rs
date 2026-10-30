// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Live ruler and grid display settings, independent of document formatting.
use gpui_kit::*;
use gpui_kit::component::v_flex;
use crate::shell::{chrome::{Checkbox, SidebarPanel, radio_button, radio_group}, editor::{RulerUnit, SlideEditor}, i18n::text};

impl SlideEditor {
    pub(crate) fn open_ruler_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        self.ruler_settings_open = true;
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn ruler_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        if !self.ruler_settings_open { return None; }
        self.ruler_settings_ui.borrow_mut().clear();
        let mark = |id: &'static str, child: AnyElement| {
            let ui = self.ruler_settings_ui.clone();
            div().relative().child(child).child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                ui.borrow_mut().insert(id.to_string(), bounds);
            }).absolute().top_0().left_0().size_full()).into_any_element()
        };
        let mut units = radio_group();
        for (id, unit) in [
            ("docx.ruler-unit.cm", RulerUnit::Centimeters),
            ("docx.ruler-unit.mm", RulerUnit::Millimeters),
            ("docx.ruler-unit.inch", RulerUnit::Inches),
            ("docx.ruler-unit.point", RulerUnit::Points),
            ("docx.ruler-unit.pica", RulerUnit::Picas),
        ] {
            units = units.child(mark(id, radio_button(id, self.ruler_unit == unit, false, text(id),
                cx.listener(move |this, _, _, cx| this.set_ruler_unit(unit, cx))).into_any_element()));
        }
        let body = v_flex().gap_2().px(px(18.)).py(px(8.))
            .p_0().child(mark("ruler-horizontal", Checkbox::new("ruler-horizontal").label(text("docx.ruler.horizontal")).checked(self.ruler_open)
                .on_click(cx.listener(|this, on: &bool, _, cx| { this.ruler_open = *on; cx.notify(); })).into_any_element()))
            .child(mark("ruler-vertical", Checkbox::new("ruler-vertical").label(text("docx.vertical-ruler")).checked(self.vertical_ruler_open)
                .on_click(cx.listener(|this, on: &bool, _, cx| { this.vertical_ruler_open = *on; cx.notify(); })).into_any_element()))
            .child(div().text_sm().child(text("docx.ruler-unit")))
            .child(units)
            .child(mark("ruler-gridlines", Checkbox::new("ruler-gridlines").label(text("docx.gridlines")).checked(self.gridlines_open)
                .on_click(cx.listener(|this, on: &bool, _, cx| { this.gridlines_open = *on; cx.notify(); })).into_any_element()));
        Some(SidebarPanel { title: text("docx.ruler"), body: body.into_any_element() })
    }
}
