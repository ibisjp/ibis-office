// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! グリッドとガイド: the dialog of PowerPoint's 表示 > グリッドとガイド（オブジェクトをグリッドに合わせる、他のオブジェクトに合わせる、
//! グリッドの間隔、グリッドを表示、ガイドを表示、スマート ガイド）. The spacing, the snapping and the guides are the file's
//! (`viewProps.xml`: `p:gridSpacing`, `snapToGrid`, `snapToObjects`, `showGuides`); the shared dialog is the panel's.
use gpui_kit::component::input::InputState;
use gpui_kit::component::v_flex;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{EmuSize, ViewGrid};

use crate::shell::chrome::{ShownNumber, SidebarPanel, SpinnerSpec, Typed};
use crate::shell::editor::SlideEditor;
use crate::shell::shape_editing::GridAndGuides;
use crate::ext::*;

pub(crate) struct GridGuidesPanel {
    pub(crate) spacing: Entity<InputState>,
    /// The spacing as the file has it (EMU) and as the field shows it (cm): an Apply without an edit keeps it exactly.
    number: ShownNumber,
    pub(crate) snap_to_grid: bool,
    pub(crate) snap_to_objects: bool,
    pub(crate) show_grid: bool,
    pub(crate) show_guides: bool,
    pub(crate) smart_guides: bool,
    error: Option<SharedString>,
}

fn cm_spec() -> SpinnerSpec { SpinnerSpec::new(0.1).decimals(2) }

/// What the field means in EMU, between the least and the most (the spacing the file has is kept as it is, even outside them).
fn spacing_of(number: &ShownNumber, value: &str) -> Option<i64> {
    match number.resolve(value) {
        Typed::Value(emu) if emu.is_finite() && ((ViewGrid::MIN_SPACING as f64..=ViewGrid::MAX_SPACING as f64).contains(&emu) || value.trim() == number.text()) => Some(emu as i64),
        _ => None,
    }
}

#[ibis_office_macros::editor_ext(GridGuidesPanelExt)]
impl SlideEditor {
    /// 表示 > グリッドとガイド…
    pub fn open_grid_guides_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_presentation() { return; }
        self.end_text_edit(window, cx);
        let spacing = self.grid_spacing();
        let number = ShownNumber::one(spacing.width, 360_000., cm_spec());
        let field = number.input(window, cx);
        let panel = GridGuidesPanel {
            spacing: field.clone(),
            number,
            snap_to_grid: self.is_pptx_snap_to_grid_enabled(),
            snap_to_objects: self.is_pptx_snap_to_shape_enabled(),
            show_grid: self.is_pptx_gridlines_enabled(),
            show_guides: self.is_pptx_guides_enabled(),
            smart_guides: self.is_pptx_smart_guides_enabled(),
            error: None,
        };
        if let Some(p) = self.pptx_mut() {
            p.grid_guides_panel = Some(panel);
        }
        self.sidebar_open = true;
        field.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    /// One of the check boxes turned over (`snap-grid`, `snap-objects`, `show-grid`, `show-guides`, `smart`).
    pub(crate) fn toggle_grid_guides_option(&mut self, which: &str, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.grid_guides_panel.as_mut()) {
            let flag = match which {
                "snap-grid" => &mut p.snap_to_grid,
                "snap-objects" => &mut p.snap_to_objects,
                "show-grid" => &mut p.show_grid,
                "show-guides" => &mut p.show_guides,
                _ => &mut p.smart_guides,
            };
            *flag = !*flag;
        }
        cx.notify();
    }

    pub fn apply_grid_guides_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.grid_guides_panel.as_ref()) else { return };
        let value = panel.spacing.read(cx).value().to_string();
        let Some(spacing) = spacing_of(&panel.number, &value) else {
            if let Some(p) = self.pptx_mut().and_then(|s| s.grid_guides_panel.as_mut()) {
                p.error = Some(crate::shell::i18n::text("pptx.grid-guides-panel.spacing-error"));
            }
            return cx.notify();
        };
        let (snap_to_grid, snap_to_objects, show_grid, show_guides, smart_guides) = (panel.snap_to_grid, panel.snap_to_objects, panel.show_grid, panel.show_guides, panel.smart_guides);
        // Only the width of the spacing is asked (PowerPoint's dialog has one number); the height follows unless the file's two differ and the number is as shown.
        let old = self.grid_spacing();
        let new = if spacing == old.width { old } else { EmuSize::new(spacing, spacing) };
        self.set_grid_and_guides(GridAndGuides { snap_to_grid, snap_to_objects, spacing: new, show_grid, smart_guides }, cx);
        if self.is_pptx_guides_enabled() != show_guides {
            self.toggle_pptx_guides(cx);
        }
        self.close_grid_guides_panel(window, cx);
    }

    pub(crate) fn close_grid_guides_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.grid_guides_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub fn grid_guides_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.grid_guides_panel.as_ref()?;
        let (spacing, error) = (p.spacing.clone(), p.error.clone());
        let flags = [p.snap_to_grid, p.snap_to_objects, p.show_grid, p.show_guides, p.smart_guides];
        let t = crate::shell::i18n::text;
        let check = |id: &'static str, label: &str, on: bool, which: &'static str, cx: &mut Context<Self>| {
            crate::shell::chrome::checkbox_button(id, t(label), on, false, cx.listener(move |e, _, _, cx| e.toggle_grid_guides_option(which, cx)))
        };
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        body = body.child(check("pptx-grid-snap-grid", "pptx.grid-guides-panel.snap-to-grid", flags[0], "snap-grid", cx));
        body = body.child(check("pptx-grid-snap-objects", "pptx.grid-guides-panel.snap-to-objects", flags[1], "snap-objects", cx));
        body = body.child(crate::shell::chrome::text_field_row(t("pptx.grid-guides-panel.spacing-cm"), crate::shell::chrome::text_field(&spacing).unit("cm", px(24.)).spinner("pptx-grid-spacing", cm_spec(), cx, |_, _, _, _| {})));
        body = body.child(check("pptx-grid-show-grid", "pptx.grid-guides-panel.show-grid", flags[2], "show-grid", cx));
        body = body.child(check("pptx-grid-show-guides", "pptx.grid-guides-panel.show-guides", flags[3], "show-guides", cx));
        body = body.child(check("pptx-grid-smart", "pptx.grid-guides-panel.smart-guides", flags[4], "smart", cx));
        if let Some(error) = error {
            body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error));
        }
        let panel_actions = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("pptx-grid-apply", t("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_grid_guides_panel(w, cx))))
            .action(crate::shell::chrome::ActionDefinition::new("pptx-grid-cancel", t("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_grid_guides_panel(w, cx))));
        Some(SidebarPanel::with_footer(t("pptx.grid-and-guides"), body.into_any_element(), panel_actions))
    }
}
