// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 書式 > スライサーの設定…: the selected slicer's heading, whether the heading
//! shows, the columns of buttons and the buttons' height, and (for a pivot
//! slicer) the pivot tables it is connected to (レポートの接続: those that
//! share its data cache).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::*;
use ibis_doc_model::slicer::{Slicer, SlicerSource};
use ibis_doc_model::{Command, ShapeId, ShapeKind, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

pub(crate) struct SlicerSettingsPanel {
    pub(crate) shape: ShapeId,
    pub(crate) name: Entity<InputState>,
    pub(crate) caption: Entity<InputState>,
    pub(crate) show_caption: bool,
    pub(crate) descending: bool,
    pub(crate) style: String,
    pub(crate) columns: Entity<InputState>,
    /// The buttons' height in points.
    pub(crate) height: Entity<InputState>,
    /// The pivot tables that can be connected, and whether each is.
    pub(crate) pivots: Vec<(String, bool)>,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

const SLICER_STYLES: &[(&str, &str)] = &[
    ("SlicerStyleLight1", "Light 1"),
    ("SlicerStyleLight2", "Light 2"),
    ("SlicerStyleLight3", "Light 3"),
    ("SlicerStyleLight4", "Light 4"),
    ("SlicerStyleLight5", "Light 5"),
    ("SlicerStyleLight6", "Light 6"),
    ("SlicerStyleDark1", "Dark 1"),
    ("SlicerStyleDark2", "Dark 2"),
    ("SlicerStyleDark3", "Dark 3"),
    ("SlicerStyleDark4", "Dark 4"),
    ("SlicerStyleDark5", "Dark 5"),
    ("SlicerStyleDark6", "Dark 6"),
    ("SlicerStyleOther1", "Other 1"),
    ("SlicerStyleOther2", "Other 2"),
];

fn points(emu: i64) -> String {
    ibis_calc::number_to_xml((emu as f64 / ibis_doc_model::EMU_PER_POINT as f64 * 100.0).round() / 100.0)
}

impl SlideEditor {
    fn selected_slicer(&self) -> Option<(ShapeId, Slicer)> {
        let id = self.selection?;
        match &self.slide().shape(id)?.kind {
            ShapeKind::Slicer(s) => Some((id, (**s).clone())),
            _ => None,
        }
    }

    pub(crate) fn candidate_slicer_pivots(&self, slicer: &Slicer) -> Vec<(String, bool)> {
        let mut pivots = Vec::new();
        if let SlicerSource::Pivot { field, pivots: connected } = &slicer.source {
            let first = connected
                .first()
                .and_then(|name| ibis_doc_model::slicer::find_pivot(self.presentation(), name).map(|(sheet, i)| &sheet.pivots[i]));
            let cache = first.and_then(|p| p.cache_part.clone());
            let source = first.map(|p| p.source.clone());
            for slide in &self.presentation().slides {
                if let Some(sheet) = ibis_doc_model::sheet_calc::sheet_of_slide(slide) {
                    for p in &sheet.pivots {
                        let same_cache = cache.is_some() && p.cache_part == cache;
                        let same_source = source.as_ref().is_some_and(|src| &p.source == src) && p.fields.iter().any(|f| f.name == *field);
                        if same_cache || same_source || connected.contains(&p.name) {
                            pivots.push((p.name.clone(), connected.contains(&p.name)));
                        }
                    }
                }
            }
        }
        pivots
    }

    pub(crate) fn update_slicer(&mut self, shape: ShapeId, change: impl FnOnce(&mut Slicer), cx: &mut Context<Self>) {
        let Some((_, mut slicer)) = self.selected_slicer_of(shape) else { return };
        let before = slicer.clone();
        change(&mut slicer);
        if slicer == before {
            return;
        }
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::EditSlicer { slide, shape, slicer })]) {
            self.status = Some(format("edit.refused", &[("error", &err)]).into());
            return cx.notify();
        }
        self.sync_shape_formulas(cx);
        cx.notify();
    }

    pub(crate) fn open_slicer_settings_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((shape, slicer)) = self.selected_slicer() else {
            self.status = Some(text("xlsx.slicer-settings.select"));
            return cx.notify();
        };
        let make = |value: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(value, window, cx));
            state
        };
        let pivots = self.candidate_slicer_pivots(&slicer);
        let panel = SlicerSettingsPanel {
            shape,
            name: make(slicer.name.clone(), window, cx),
            caption: make(slicer.caption.clone(), window, cx),
            show_caption: slicer.show_caption,
            descending: slicer.descending,
            style: if slicer.style.is_empty() { "SlicerStyleLight1".to_string() } else { slicer.style.clone() },
            columns: make(slicer.columns.max(1).to_string(), window, cx),
            height: make(points(slicer.button_height()), window, cx),
            pivots,
            message: String::new(),
            ui: Default::default(),
        };
        let state = self.wb_mut();
        state.close_settings();
        state.panels.slicer_settings = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn apply_slicer_settings_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.slicer_settings.as_ref()) else { return };
        let shape = p.shape;
        let Some((_, mut slicer)) = self.selected_slicer_of(shape) else { return };
        let name = p.name.read(cx).value().trim().to_string();
        let caption = p.caption.read(cx).value().to_string();
        let style = p.style.clone();
        let columns = p.columns.read(cx).value().trim().parse::<u32>().ok().filter(|c| (1..=20).contains(c));
        let height = p.height.read(cx).value().trim().parse::<f64>().ok().filter(|h| (5.0..=200.0).contains(h));
        let (Some(columns), Some(height)) = (columns, height) else {
            if let Some(p) = self.wb_mut().panels.slicer_settings.as_mut() {
                p.message = text("xlsx.slicer-settings.bad-number").to_string();
            }
            return cx.notify();
        };
        if !name.is_empty() && name != slicer.name {
            let taken = self.presentation().slides.iter().flat_map(|sl| &sl.shapes).any(|sh| {
                sh.id != shape && matches!(&sh.kind, ShapeKind::Slicer(other) if other.name == name)
            });
            if taken {
                if let Some(p) = self.wb_mut().panels.slicer_settings.as_mut() {
                    p.message = text("xlsx.slicer-settings.name-in-use").to_string();
                }
                return cx.notify();
            }
        }
        let connected: Vec<String> = p.pivots.iter().filter(|(_, on)| *on).map(|(n, _)| n.clone()).collect();
        if matches!(slicer.source, SlicerSource::Pivot { .. }) && connected.is_empty() {
            if let Some(p) = self.wb_mut().panels.slicer_settings.as_mut() {
                p.message = text("xlsx.slicer-settings.connect-one").to_string();
            }
            return cx.notify();
        }
        if !name.is_empty() {
            slicer.name = name;
        }
        slicer.caption = if caption.trim().is_empty() { slicer.caption.clone() } else { caption };
        slicer.show_caption = p.show_caption;
        slicer.descending = p.descending;
        slicer.style = style;
        slicer.columns = columns;
        slicer.row_height = (height * ibis_doc_model::EMU_PER_POINT as f64).round() as i64;
        if let SlicerSource::Pivot { pivots, .. } = &mut slicer.source {
            *pivots = connected;
        }
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::EditSlicer { slide, shape, slicer: slicer.clone() })]) {
            if let Some(p) = self.wb_mut().panels.slicer_settings.as_mut() {
                p.message = format("edit.refused", &[("error", &err)]);
            }
            return cx.notify();
        }
        self.wb_mut().panels.slicer_settings = None;
        if matches!(slicer.source, SlicerSource::Pivot { .. }) && slicer.items.iter().any(|i| !i.selected) {
            let selected: Vec<bool> = slicer.items.iter().map(|i| i.selected).collect();
            self.select_slicer_items(&slicer, &selected, cx);
        }
        self.sync_shape_formulas(cx);
        cx.notify();
    }

    fn selected_slicer_of(&self, shape: ShapeId) -> Option<(ShapeId, Slicer)> {
        match &self.slide().shape(shape)?.kind {
            ShapeKind::Slicer(s) => Some((shape, (**s).clone())),
            _ => None,
        }
    }

    pub(crate) fn slicer_settings_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.slicer_settings.as_ref()?;
        let current_style = p.style.clone();
        let descending = p.descending;
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
        let caption = |id: &str| div().text_sm().child(text(id));
        let mut body = v_flex().gap_2().px(px(16.)).py(px(8.)).p_0();
        body = body.child(caption("xlsx.slicer-settings.name")).child(mark("name", crate::shell::chrome::text_field(&p.name).full_width().into_any_element()));
        body = body.child(caption("xlsx.slicer-settings.caption")).child(mark("caption", crate::shell::chrome::text_field(&p.caption).full_width().into_any_element()));
        body = body.child(mark(
            "show-caption",
            super::toggle::toggle_row(
                "slicer-show-caption",
                p.show_caption,
                text("xlsx.slicer-settings.show-caption"),
                cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.slicer_settings.as_mut() {
                        p.show_caption = !p.show_caption;
                    }
                    cx.notify();
                }),
            )
            .into_any_element(),
        ));
        let sort_radio = |id: &'static str, on: bool, label: &'static str, desc: bool| {
            mark(
                id,
                crate::shell::chrome::radio_button(id, on, false, text(label), cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.slicer_settings.as_mut() {
                            p.descending = desc;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        body = body.child(caption("_xlsx.sort-2")).child(
            crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                .child(sort_radio("sort-asc", !descending, "_xlsx.ascending", false))
                .child(sort_radio("sort-desc", descending, "_xlsx.descending", true)),
        );
        body = body.child(caption("xlsx.slicer-settings.style"));
        let choices=SLICER_STYLES.iter().map(|&(style,label)|{let owned=style.to_string();crate::shell::chrome::dropdown::ColorPaletteItem::single(label.into(),super::slicer_view::accent(style),Rc::new(move|e,_,cx|{if let Some(p)=e.wb_mut().panels.slicer_settings.as_mut(){p.style=owned.clone();}cx.notify();}))}).collect();
        let selected=SLICER_STYLES.iter().position(|(style,_)|*style==current_style).unwrap_or(0);
        body=body.child(crate::shell::chrome::ColorPaletteDropdown::new("slicer-style-dropdown",choices,selected,&cx.weak_entity()));
        body = body.child(caption("xlsx.slicer-settings.columns")).child(mark("columns", crate::shell::chrome::text_field(&p.columns).full_width().spinner("slicer-columns", crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, 20.0).integer_only(), cx, |_, _, _, _| {}).into_any_element()));
        body = body.child(caption("xlsx.slicer-settings.height")).child(mark("height", crate::shell::chrome::text_field(&p.height).unit("pt",px(20.)).spinner("slicer-height", crate::shell::chrome::SpinnerSpec::new(1.0).range(5.0, 200.0), cx, |_, _, _, _| {}).into_any_element()));
        if !p.pivots.is_empty() {
            body = body.child(caption("xlsx.slicer-settings.connections"));
            for (i, (name, on)) in p.pivots.iter().enumerate() {
                body = body.child(mark(
                    &format!("pivot-{i}"),
                    super::toggle::toggle_row(
                        SharedString::from(format!("slicer-pivot-{i}")),
                        *on,
                        SharedString::from(name.clone()),
                        cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.slicer_settings.as_mut()
                                && let Some(entry) = p.pivots.get_mut(i)
                            {
                                entry.1 = !entry.1;
                            }
                            cx.notify();
                        }),
                    )
                    .into_any_element(),
                ));
            }
        }
        if !p.message.is_empty() {
            body = body.children(crate::shell::chrome::panel_message(&p.message));
        }
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("slicer-settings-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label(text("OK")).on_click(cx.listener(|this, _, _, cx| this.apply_slicer_settings_panel(cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("xlsx.slicer-settings.title"), body.into_any_element(), panel_terminal_actions))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_caption_is_in_both_languages() {
        for table in [include_str!("../../i18n/ja/xlsx.tsv"), include_str!("../../i18n/en/xlsx.tsv")] {
            for id in ["xlsx.slicer-settings", "xlsx.slicer-settings.title", "xlsx.slicer-settings.select", "xlsx.slicer-settings.name", "xlsx.slicer-settings.name-in-use", "xlsx.slicer-settings.caption", "xlsx.slicer-settings.show-caption", "xlsx.slicer-settings.style", "xlsx.slicer-settings.columns", "xlsx.slicer-settings.height", "xlsx.slicer-settings.connections", "xlsx.slicer-settings.bad-number", "xlsx.slicer-settings.connect-one"] {
                assert!(table.lines().any(|l| l.split('\t').next() == Some(id)), "{id} is missing");
            }
        }
    }
}
