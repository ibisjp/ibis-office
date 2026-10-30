// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Presentation show properties panel (スライド ショーの設定).
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, ShowProperties, ShowSlideRange, ShowType};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShowRangeKind {
    All,
    Range,
    Custom,
}

pub(crate) struct ShowPanel {
    pub(crate) show_type: ShowType,
    pub(crate) loop_continuously: bool,
    pub(crate) show_narration: bool,
    pub(crate) show_animation: bool,
    pub(crate) use_timings: bool,
    pub(crate) all_slides: bool,
    pub(crate) range_kind: ShowRangeKind,
    pub(crate) selected_custom_show: Option<u32>,
    pub(crate) from: Entity<InputState>,
    pub(crate) to: Entity<InputState>,
    pub(crate) error: Option<SharedString>,
}

impl SlideEditor {
    pub(crate) fn open_show_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let p = &self.presentation().show_properties;
        let show_type = p.show_type;
        let loop_continuously = p.loop_continuously;
        let show_narration = p.show_narration;
        let show_animation = p.show_animation;
        let use_timings = p.use_timings;
        let (range_kind, selected_custom_show, start_val, end_val) = match p.slide_range {
            ShowSlideRange::All => (ShowRangeKind::All, self.presentation().custom_shows.first().map(|s| s.id), 1, self.presentation().slides.len().max(1)),
            ShowSlideRange::Range { start, end } => (ShowRangeKind::Range, self.presentation().custom_shows.first().map(|s| s.id), start, end),
            ShowSlideRange::Custom(id) => (ShowRangeKind::Custom, Some(id), 1, self.presentation().slides.len().max(1)),
        };

        let from = cx.new(|cx| InputState::new(window, cx).default_value(start_val.to_string()));
        let to = cx.new(|cx| InputState::new(window, cx).default_value(end_val.to_string()));

        if let Some(s) = self.pptx_mut() {
            s.section_panel = None;
            s.gradient_panel = None;
            s.shadow_panel = None;
            s.glow_panel = None;
            s.paragraph_panel = None;
            s.timing_panel = None;
            s.find = None;
            s.size_panel = None;
            s.frame_panel = None;
            s.comments = None;
            s.chart_panel = None;
            s.header_footer = None;
            s.insets_panel = None;
            s.crop_panel = None;
            s.show_panel = Some(ShowPanel {
                show_type,
                loop_continuously,
                show_narration,
                show_animation,
                use_timings,
                all_slides: range_kind == ShowRangeKind::All,
                range_kind,
                selected_custom_show,
                from: from.clone(),
                to,
                error: None,
            });
        }
        self.sidebar_open = true;
        from.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn set_show_panel_type(&mut self, kind: ShowType, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
            p.show_type = kind;
        }
        cx.notify();
    }

    pub(crate) fn toggle_show_panel_option(&mut self, option: &str, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
            match option {
                "loop" => p.loop_continuously = !p.loop_continuously,
                "narration" => p.show_narration = !p.show_narration,
                "animation" => p.show_animation = !p.show_animation,
                "timings" => p.use_timings = !p.use_timings,
                _ => {}
            }
        }
        cx.notify();
    }

    pub(crate) fn apply_show_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.show_panel.as_ref()) else { return };
        let show_type = p.show_type;
        let loop_continuously = p.loop_continuously;
        let show_narration = p.show_narration;
        let show_animation = p.show_animation;
        let use_timings = p.use_timings;
        let pen_color = self.presentation().show_properties.pen_color;

        let range_kind = if !p.all_slides && p.range_kind == ShowRangeKind::All {
            ShowRangeKind::Range
        } else {
            p.range_kind
        };
        let slide_range = match range_kind {
            ShowRangeKind::All => ShowSlideRange::All,
            ShowRangeKind::Custom => {
                if let Some(id) = p.selected_custom_show {
                    ShowSlideRange::Custom(id)
                } else if let Some(first) = self.presentation().custom_shows.first() {
                    ShowSlideRange::Custom(first.id)
                } else {
                    ShowSlideRange::All
                }
            }
            ShowRangeKind::Range => {
                let from_str = p.from.read(cx).value();
                let to_str = p.to.read(cx).value();
                let start = from_str.trim().parse::<usize>().ok();
                let end = to_str.trim().parse::<usize>().ok();
                match (start, end) {
                    (Some(s), Some(e)) if s >= 1 && e >= 1 => {
                        let total = self.presentation().slides.len().max(1);
                        ShowSlideRange::Range {
                            start: s.min(total),
                            end: e.min(total),
                        }
                    }
                    _ => {
                        if let Some(p) = self.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
                            p.error = Some(crate::shell::i18n::text("pptx.show-panel.please-enter-valid-slide-number").into());
                        }
                        return cx.notify();
                    }
                }
            }
        };

        let new_props = ShowProperties {
            show_type,
            loop_continuously,
            show_narration,
            show_animation,
            use_timings,
            slide_range,
            pen_color,
        };

        if self.presentation().show_properties != new_props {
            self.run(vec![Command::SetShowProperties { properties: new_props }], cx);
        }
        self.close_show_panel(window, cx);
    }

    pub(crate) fn close_show_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.show_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn show_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.show_panel.as_ref()?;
        let title = crate::shell::i18n::text("pptx.show-panel.set-up-slide-show");
        let (show_type, loop_cont, narration, animation, timings, range_kind, selected_custom_show, error) = (
            p.show_type,
            p.loop_continuously,
            p.show_narration,
            p.show_animation,
            p.use_timings,
            p.range_kind,
            p.selected_custom_show,
            p.error.clone(),
        );
        let from = p.from.clone();
        let to = p.to.clone();

        let is_presenter = matches!(show_type, ShowType::Presenter);
        let is_browse = matches!(show_type, ShowType::Browse);
        let is_kiosk = matches!(show_type, ShowType::Kiosk { .. });

        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.))
            // 種類
            .p_0().child(div().text_sm().font_weight(FontWeight::BOLD).child(crate::shell::i18n::text("pptx.show.type")))
            .child(crate::shell::chrome::radio_button("pptx-show-type-presenter", is_presenter, false, crate::shell::i18n::text("pptx.show-panel.presented-speaker-full-screen-2"), cx.listener(|e, _, _, cx| e.set_show_panel_type(ShowType::Presenter, cx))))
            .child(crate::shell::chrome::radio_button("pptx-show-type-browse", is_browse, false, crate::shell::i18n::text("pptx.show-panel.browsed-individual-window-2"), cx.listener(|e, _, _, cx| e.set_show_panel_type(ShowType::Browse, cx))))
            .child(crate::shell::chrome::radio_button("pptx-show-type-kiosk", is_kiosk, false, crate::shell::i18n::text("pptx.show-panel.browsed-kiosk-full-screen-2"), cx.listener(|e, _, _, cx| e.set_show_panel_type(ShowType::Kiosk { restart: None }, cx))))
            // スライドの表示
            .child(div().text_sm().font_weight(FontWeight::BOLD).child(crate::shell::i18n::text("pptx.show-panel.show-slides")))
            .child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
            .child(crate::shell::chrome::radio_button("pptx-show-range-all", range_kind == ShowRangeKind::All, false, crate::shell::i18n::text("pptx.show-panel.all-2"), cx.listener(|e, _, _, cx| {
                if let Some(p) = e.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
                    p.all_slides = true;
                    p.range_kind = ShowRangeKind::All;
                }
                cx.notify();
            })))
            .child(crate::shell::chrome::radio_button("pptx-show-range-custom", range_kind == ShowRangeKind::Range, false, crate::shell::i18n::text("pptx.show-panel.from-2"), cx.listener(|e, _, _, cx| {
                if let Some(p) = e.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
                    p.all_slides = false;
                    p.range_kind = ShowRangeKind::Range;
                }
                cx.notify();
            })))
            )
            .child(
                h_flex().gap_2().items_center().flex_wrap().pl(px(28.))
                    .child(div().text_sm().child(crate::shell::i18n::text("pptx.animation-ui.from")))
                    .child(crate::shell::chrome::text_field(&from).disabled(range_kind != ShowRangeKind::Range).spinner("pptx-show-range-from", crate::shell::chrome::SpinnerSpec::new(1.0).min(1.0).integer_only(), cx, |_, _, _, _| {}))
                    .child(div().text_sm().child(crate::shell::i18n::text("pptx.show-panel.to")))
                    .child(crate::shell::chrome::text_field(&to).disabled(range_kind != ShowRangeKind::Range).spinner("pptx-show-range-to", crate::shell::chrome::SpinnerSpec::new(1.0).min(1.0).integer_only(), cx, |_, _, _, _| {}))
            )
            .child(crate::shell::chrome::radio_button("pptx-show-range-custom-show", range_kind == ShowRangeKind::Custom, false, crate::shell::i18n::text("_pptx.custom-show"), cx.listener(|e, _, _, cx| {
                let first_id = e.presentation().custom_shows.first().map(|s| s.id);
                if let Some(p) = e.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
                    p.all_slides = false;
                    p.range_kind = ShowRangeKind::Custom;
                    if p.selected_custom_show.is_none() {
                        p.selected_custom_show = first_id;
                    }
                }
                cx.notify();
            })));

        if range_kind == ShowRangeKind::Custom {
            let custom_shows = self.presentation().custom_shows.clone();
            if custom_shows.is_empty() {
                body = body.child(div().px(px(8.)).text_sm().text_color(rgb(0x888888)).child(crate::shell::i18n::text("_pptx.no-custom-shows-have-been-defined")));
            } else {
                for cs in custom_shows {
                    let cs_id = cs.id;
                    let selected = selected_custom_show == Some(cs_id);
                    let label = format!("{} {}", if selected { "✓" } else { " " }, cs.name);
                    body = body.child(
                        crate::shell::chrome::action_button(ElementId::Name(format!("pptx-custom-show-select-{}", cs_id).into()))
                            .small().label(label)
                            .on_click(cx.listener(move |e, _, _, cx| {
                                if let Some(p) = e.pptx_mut().and_then(|s| s.show_panel.as_mut()) {
                                    p.range_kind = ShowRangeKind::Custom;
                                    p.selected_custom_show = Some(cs_id);
                                }
                                cx.notify();
                            }))
                    );
                }
            }
        }

        body = body
            // オプション
            .child(div().text_sm().font_weight(FontWeight::BOLD).child(crate::shell::i18n::text("pptx.show-panel.show-options")))
            .child(crate::shell::chrome::checkbox_button("pptx-show-opt-loop", crate::shell::i18n::text("pptx.show-panel.loop-continuously-until-esc-2"), loop_cont, false, cx.listener(|e, _, _, cx| e.toggle_show_panel_option("loop", cx))))
            .child(crate::shell::chrome::checkbox_button("pptx-show-opt-anim", crate::shell::i18n::text("pptx.show-panel.show-without-animation-2"), !animation, false, cx.listener(|e, _, _, cx| e.toggle_show_panel_option("animation", cx))))
            .child(crate::shell::chrome::checkbox_button("pptx-show-opt-narration", crate::shell::i18n::text("pptx.show-panel.show-without-narration-2"), !narration, false, cx.listener(|e, _, _, cx| e.toggle_show_panel_option("narration", cx))))
            .child(crate::shell::chrome::checkbox_button("pptx-show-opt-timings", crate::shell::i18n::text("pptx.show-panel.use-timings-2"), timings, false, cx.listener(|e, _, _, cx| e.toggle_show_panel_option("timings", cx))));

        if let Some(err) = error {
            body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(err));
        }
        let panel_actions = crate::shell::chrome::action_bar()
                .action(crate::shell::chrome::ActionDefinition::new("pptx-show-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_show_panel(w, cx))))
                .action(crate::shell::chrome::ActionDefinition::new("pptx-show-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_show_panel(w, cx))));
        Some(SidebarPanel::with_footer(title, body.into_any_element(), panel_actions))
    }
}
