// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Slide Zoom (PPTX-TRANS-014): insert slide zoom cards linking to other slides,
//! navigate seamlessly during slide shows, and modal picker dialog UI.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{h_flex, v_flex, Disableable as _, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{
    AutoShape, Color, Command, EmuRect, Fill, Geometry, Rgb, Shape, ShapeId, SlideId, SlideLink,
    Stroke, TextBody, TextStyle,
};

use crate::shell::editor::SlideEditor;

/// The open state of the Insert Slide Zoom modal dialog.
pub(crate) struct SlideZoomDialog {
    /// The currently selected target slide to zoom to.
    pub(crate) selected_slide: Option<SlideId>,
}

impl SlideEditor {
    /// Open the Insert Slide Zoom dialog.
    pub(crate) fn open_slide_zoom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let first_other = self
            .presentation()
            .slides
            .iter()
            .find(|s| s.id != self.current_slide)
            .map(|s| s.id);
        if let Some(pptx) = self.pptx_mut() {
            pptx.slide_zoom_dialog = Some(SlideZoomDialog {
                selected_slide: first_other,
            });
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Close the Insert Slide Zoom dialog.
    pub(crate) fn close_slide_zoom_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(pptx) = self.pptx_mut() {
            pptx.slide_zoom_dialog = None;
        }
        cx.notify();
    }

    /// Insert a Slide Zoom shape targeting `target_slide_id` onto the current slide.
    pub(crate) fn insert_slide_zoom(
        &mut self,
        target_slide_id: SlideId,
        cx: &mut Context<Self>,
    ) -> Option<ShapeId> {
        let current_slide_id = self.current_slide;
        let slide_size = self.presentation().slide_size;

        // Retrieve information about the target slide
        let (target_index, target_title) = {
            let p = self.presentation();
            let idx = p.slide_index(target_slide_id)?;
            let title = p
                .slide(target_slide_id)
                .and_then(|s| {
                    s.shapes.iter().find_map(|sh| {
                        if let Some(tb) = sh.text_body() {
                            let t = tb.text().trim().to_string();
                            if !t.is_empty() {
                                return Some(t);
                            }
                        }
                        None
                    })
                })
                .unwrap_or_else(|| format!("Slide {}", idx + 1));
            (idx + 1, title)
        };

        // Determine zoom card dimensions (roughly 35% of slide dimensions, maintaining aspect ratio)
        let zoom_w = 2_800_000.min(slide_size.width / 2);
        let zoom_h = (zoom_w as f64 * (slide_size.height as f64 / slide_size.width as f64)) as i64;

        // Position on current slide (centered, with staggered offset if existing zoom shapes exist)
        let existing_zooms = self
            .slide()
            .shape_links
            .values()
            .filter(|l| matches!(l, SlideLink::Slide(_)))
            .count();
        let offset = (existing_zooms as i64 % 5) * 300_000;
        let x = ((slide_size.width - zoom_w) / 2 + offset).max(200_000);
        let y = ((slide_size.height - zoom_h) / 2 + offset).max(200_000);
        let frame = EmuRect::new(x, y, zoom_w, zoom_h);

        // Create the Slide Zoom shape: AutoShape card with border and slide title preview
        let shape_id = self.document.ids().new_shape_id();
        let stroke = Stroke::solid(Color::opaque(Rgb(0xd0d0d0)), 12_700);
        let fill = Fill::Solid(Color::opaque(Rgb(0xffffff)));

        let header_text = format!("🔍 Slide {} (Zoom)\n{}", target_index, target_title);
        let body = TextBody::plain(
            &header_text,
            TextStyle {
                font_size: 14_00,
                color: Rgb(0x333333),
                bold: true,
                ..Default::default()
            },
        );

        let mut auto = AutoShape::new(Geometry::Rect, fill, Some(stroke));
        auto.text = Some(body);
        let shape = Shape::new(shape_id, frame, ibis_doc_model::ShapeKind::Auto(auto));

        let insert_index = self.slide().shapes.len();
        let link_tooltip = format!("Slide {} Zoom: {}", target_index, target_title);

        let commands = vec![
            Command::InsertShape {
                slide: current_slide_id,
                index: insert_index,
                shape,
            },
            Command::SetShapeLink {
                slide: current_slide_id,
                shape: shape_id,
                link: Some(SlideLink::Slide(target_slide_id)),
            },
            Command::SetShapeLinkTooltip {
                slide: current_slide_id,
                shape: shape_id,
                tooltip: Some(link_tooltip),
            },
        ];

        self.run(commands, cx);
        self.set_selection(vec![shape_id]);
        Some(shape_id)
    }

    /// Render the Insert Slide Zoom modal dialog if open.
    pub(crate) fn render_slide_zoom_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pptx = self.pptx()?;
        let dialog = pptx.slide_zoom_dialog.as_ref()?;
        let selected = dialog.selected_slide;
        let current_id = self.current_slide;
        let t = crate::shell::i18n::text;

        let slides_list = self
            .presentation()
            .slides
            .iter()
            .enumerate()
            .filter(|(_, s)| s.id != current_id)
            .map(|(idx, s)| {
                let s_id = s.id;
                let is_sel = selected == Some(s_id);
                let title = s
                    .shapes
                    .iter()
                    .find_map(|sh| {
                        sh.text_body().and_then(|tb| {
                            let text = tb.text().trim().to_string();
                            (!text.is_empty()).then_some(text)
                        })
                    })
                    .unwrap_or_else(|| format!("{}: {}", t("_shell.slide"), idx + 1));

                div()
                    .id(("zoom-slide-row", idx as u64))
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_sel, |d| d.bg(rgb(0xd3e3fd)))
                    .when(!is_sel, |d| d.hover(|h| h.bg(rgb(0xf1f3f4))))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.pptx_mut().and_then(|p| p.slide_zoom_dialog.as_mut())
                        {
                            p.selected_slide = Some(s_id);
                        }
                        cx.notify();
                    }))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .w_6()
                                    .h_6()
                                    .rounded_sm()
                                    .border_1()
                                    .border_color(rgb(0xdadce0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .child(format!("{}", idx + 1)),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(if is_sel {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .child(title),
                            ),
                    )
            })
            .collect::<Vec<_>>();

        Some(
            div()
                .id("slide-zoom-modal-backdrop")
                .absolute()
                .inset_0()
                .bg(rgba(0x00000088))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    v_flex()
                        .w(px(460.))
                        .max_h(px(480.))
                        .bg(rgb(0xffffff))
                        .rounded_xl()
                        .shadow_lg()
                        .border_1()
                        .border_color(rgb(0xdadce0))
                        .p_5()
                        .gap_4()
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb(0x202124))
                                        .child(t("pptx.zoom.dialog.title")),
                                )
                                .child(
                                    crate::shell::chrome::action_button("zoom-close")
                                        .small()
                                        .label("✕")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.close_slide_zoom_dialog(cx);
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x5f6368))
                                .child(t("pptx.zoom.dialog.prompt")),
                        )
                        .child(
                            div()
                                .id("zoom-slide-list")
                                .flex()
                                .flex_col()
                                .max_h(px(260.))
                                .overflow_y_scroll()
                                .gap_1()
                                .children(slides_list),
                        )
                        .child(
                            crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar()
                                .action(
                                    crate::shell::chrome::ActionDefinition::new("zoom-insert", t("pptx.zoom.insert"), crate::shell::chrome::ActionButtonKind::Primary)
                                        .disabled(selected.is_none())
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(target) = selected {
                                                this.insert_slide_zoom(target, cx);
                                                this.close_slide_zoom_dialog(cx);
                                            }
                                        })),
                                )
                                .action(
                                    crate::shell::chrome::ActionDefinition::new("zoom-cancel", t("pptx.zoom.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.close_slide_zoom_dialog(cx);
                                        })),
                                ))),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Open the Insert Section Zoom dialog.
    pub(crate) fn open_section_zoom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let first_section = self
            .presentation()
            .sections
            .first()
            .map(|s| s.id.clone());
        if let Some(pptx) = self.pptx_mut() {
            pptx.section_zoom_dialog = Some(SectionZoomDialog {
                selected_section_id: first_section,
            });
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Close the Insert Section Zoom dialog.
    pub(crate) fn close_section_zoom_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(pptx) = self.pptx_mut() {
            pptx.section_zoom_dialog = None;
        }
        cx.notify();
    }

    /// Insert a Section Zoom shape targeting `section_id` onto the current slide.
    pub(crate) fn insert_section_zoom(
        &mut self,
        section_id: &str,
        cx: &mut Context<Self>,
    ) -> Option<ShapeId> {
        let current_slide_id = self.current_slide;
        let slide_size = self.presentation().slide_size;

        let p = self.presentation();
        let section = p.sections.iter().find(|s| s.id == section_id)?;
        let first_slide_id = section.slides.first().copied()?;
        let section_name = section.name.clone();

        // Retrieve information about the first slide of the section
        let (slide_index, slide_title) = {
            let idx = p.slide_index(first_slide_id)?;
            let title = p
                .slide(first_slide_id)
                .and_then(|s| {
                    s.shapes.iter().find_map(|sh| {
                        if let Some(tb) = sh.text_body() {
                            let t = tb.text().trim().to_string();
                            if !t.is_empty() {
                                return Some(t);
                            }
                        }
                        None
                    })
                })
                .unwrap_or_else(|| format!("Slide {}", idx + 1));
            (idx + 1, title)
        };

        // Determine zoom card dimensions
        let zoom_w = 2_800_000.min(slide_size.width / 2);
        let zoom_h = (zoom_w as f64 * (slide_size.height as f64 / slide_size.width as f64)) as i64;

        let existing_zooms = self
            .slide()
            .shape_links
            .values()
            .filter(|l| matches!(l, SlideLink::Slide(_)))
            .count();
        let offset = (existing_zooms as i64 % 5) * 300_000;
        let x = ((slide_size.width - zoom_w) / 2 + offset).max(200_000);
        let y = ((slide_size.height - zoom_h) / 2 + offset).max(200_000);
        let frame = EmuRect::new(x, y, zoom_w, zoom_h);

        let shape_id = self.document.ids().new_shape_id();
        let stroke = Stroke::solid(Color::opaque(Rgb(0xd0d0d0)), 12_700);
        let fill = Fill::Solid(Color::opaque(Rgb(0xffffff)));

        let header_text = format!("🔍 Section: {} (Zoom)\n{}", section_name, slide_title);
        let body = TextBody::plain(
            &header_text,
            TextStyle {
                font_size: 14_00,
                color: Rgb(0x333333),
                bold: true,
                ..Default::default()
            },
        );

        let mut auto = AutoShape::new(Geometry::Rect, fill, Some(stroke));
        auto.text = Some(body);
        let shape = Shape::new(shape_id, frame, ibis_doc_model::ShapeKind::Auto(auto));

        let insert_index = self.slide().shapes.len();
        let link_tooltip = format!("Section Zoom: {} (Slide {})", section_name, slide_index);

        let commands = vec![
            Command::InsertShape {
                slide: current_slide_id,
                index: insert_index,
                shape,
            },
            Command::SetShapeLink {
                slide: current_slide_id,
                shape: shape_id,
                link: Some(SlideLink::Slide(first_slide_id)),
            },
            Command::SetShapeLinkTooltip {
                slide: current_slide_id,
                shape: shape_id,
                tooltip: Some(link_tooltip),
            },
            Command::SetShapeReturnToZoom {
                slide: current_slide_id,
                shape: shape_id,
                return_to_zoom: true,
            },
        ];

        self.run(commands, cx);
        self.set_selection(vec![shape_id]);
        Some(shape_id)
    }

    /// Render the Insert Section Zoom modal dialog if open.
    pub(crate) fn render_section_zoom_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pptx = self.pptx()?;
        let dialog = pptx.section_zoom_dialog.as_ref()?;
        let selected = dialog.selected_section_id.clone();
        let t = crate::shell::i18n::text;

        let sections_list = self
            .presentation()
            .sections
            .iter()
            .enumerate()
            .map(|(idx, s)| {
                let s_id = s.id.clone();
                let is_sel = selected.as_deref() == Some(&s_id);
                let count = s.slides.len();
                let first_title = s.slides.first().and_then(|&id| {
                    self.presentation().slide(id).and_then(|sl| {
                        sl.shapes.iter().find_map(|sh| {
                            sh.text_body().and_then(|tb| {
                                let txt = tb.text().trim().to_string();
                                (!txt.is_empty()).then_some(txt)
                            })
                        })
                    })
                }).unwrap_or_default();

                let subtitle = if first_title.is_empty() {
                    format!("{} {}", count, t("_shell.slide"))
                } else {
                    format!("{} {} • {}", count, t("_shell.slide"), first_title)
                };

                div()
                    .id(("zoom-section-row", idx as u64))
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_sel, |d| d.bg(rgb(0xd3e3fd)))
                    .when(!is_sel, |d| d.hover(|h| h.bg(rgb(0xf1f3f4))))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.pptx_mut().and_then(|p| p.section_zoom_dialog.as_mut())
                        {
                            p.selected_section_id = Some(s_id.clone());
                        }
                        cx.notify();
                    }))
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(if is_sel {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .child(s.name.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x5f6368))
                                    .child(subtitle),
                            ),
                    )
            })
            .collect::<Vec<_>>();

        Some(
            div()
                .id("section-zoom-modal-backdrop")
                .absolute()
                .inset_0()
                .bg(rgba(0x00000088))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    v_flex()
                        .w(px(460.))
                        .max_h(px(480.))
                        .bg(rgb(0xffffff))
                        .rounded_xl()
                        .shadow_lg()
                        .border_1()
                        .border_color(rgb(0xdadce0))
                        .p_5()
                        .gap_4()
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(rgb(0x202124))
                                        .child(t("pptx.zoom.section.dialog.title")),
                                )
                                .child(
                                    crate::shell::chrome::action_button("section-zoom-close")
                                        .small()
                                        .label("✕")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.close_section_zoom_dialog(cx);
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x5f6368))
                                .child(t("pptx.zoom.section.dialog.prompt")),
                        )
                        .child(
                            div()
                                .id("zoom-section-list")
                                .flex()
                                .flex_col()
                                .max_h(px(260.))
                                .overflow_y_scroll()
                                .gap_1()
                                .children(sections_list),
                        )
                        .child(
                            crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar()
                                .action(
                                    crate::shell::chrome::ActionDefinition::new("section-zoom-insert", t("pptx.zoom.insert"), crate::shell::chrome::ActionButtonKind::Primary)
                                        .disabled(selected.is_none())
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(target) = selected.as_deref() {
                                                this.insert_section_zoom(target, cx);
                                                this.close_section_zoom_dialog(cx);
                                            }
                                        })),
                                )
                                .action(
                                    crate::shell::chrome::ActionDefinition::new("section-zoom-cancel", t("pptx.zoom.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.close_section_zoom_dialog(cx);
                                        })),
                                ))),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Whether the shape has "Return to Zoom" enabled.
    pub(crate) fn shape_return_to_zoom(&self, shape_id: ShapeId) -> bool {
        self.slide().shape_return_to_zoom.contains(&shape_id)
    }

    /// Toggle "Return to Zoom" on the specified shape.
    pub(crate) fn toggle_shape_return_to_zoom(&mut self, shape_id: ShapeId, cx: &mut Context<Self>) {
        let current_slide_id = self.current_slide;
        let is_currently = self.slide().shape_return_to_zoom.contains(&shape_id);
        let cmd = Command::SetShapeReturnToZoom {
            slide: current_slide_id,
            shape: shape_id,
            return_to_zoom: !is_currently,
        };
        self.run(vec![cmd], cx);
    }

    /// Whether the zoom shape shows the slide background (default true, false = transparent background).
    pub(crate) fn shape_zoom_show_bg(&self, shape_id: ShapeId) -> bool {
        self.slide().shape_zoom_show_bg.get(&shape_id).copied().unwrap_or(true)
    }

    /// Toggle "Zoom Background" (show slide background vs transparent background) on the specified shape.
    pub(crate) fn toggle_shape_zoom_show_bg(&mut self, shape_id: ShapeId, cx: &mut Context<Self>) {
        let current_slide_id = self.current_slide;
        let is_currently = self.shape_zoom_show_bg(shape_id);
        let cmd = Command::SetShapeZoomShowBg {
            slide: current_slide_id,
            shape: shape_id,
            show_bg: Some(!is_currently),
        };
        self.run(vec![cmd], cx);
    }

    /// The transition duration of the zoom shape in milliseconds (default 1000ms).
    pub(crate) fn shape_zoom_duration(&self, shape_id: ShapeId) -> u32 {
        self.slide().shape_zoom_durations.get(&shape_id).copied().unwrap_or(1000)
    }

    /// Set the transition duration of the zoom shape in milliseconds.
    pub(crate) fn set_shape_zoom_duration(&mut self, shape_id: ShapeId, duration_ms: u32, cx: &mut Context<Self>) {
        let current_slide_id = self.current_slide;
        let cmd = Command::SetShapeZoomDuration {
            slide: current_slide_id,
            shape: shape_id,
            duration_ms: Some(duration_ms),
        };
        self.run(vec![cmd], cx);
    }

    /// Whether the shape is a Zoom shape (Slide Zoom or Section Zoom).
    pub(crate) fn is_zoom_shape(&self, shape_id: ShapeId) -> bool {
        matches!(self.slide().shape_links.get(&shape_id), Some(ibis_doc_model::SlideLink::Slide(_)))
    }

    /// Whether the zoom shape has a custom image (Picture kind).
    pub(crate) fn has_custom_zoom_image(&self, shape_id: ShapeId) -> bool {
        self.is_zoom_shape(shape_id)
            && self.slide().shape(shape_id).is_some_and(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_)))
    }

    /// Change the image of the specified zoom shape.
    pub(crate) fn change_zoom_image(&mut self, shape_id: ShapeId, name: &str, data: Vec<u8>, cx: &mut Context<Self>) -> bool {
        if !self.is_zoom_shape(shape_id) {
            return false;
        }
        let format = ibis_doc_model::MediaFormat::from_path(name);
        if crate::pptx::editing::validated_picture_size(format, &data).is_none() {
            log::warn!("Invalid picture data for zoom image: {name}");
            return false;
        }
        let current_slide_id = self.current_slide;
        let Some(shape) = self.slide().shape(shape_id).cloned() else { return false };
        let media = self.document.ids().add_media(format, data.clone());
        self.resources.add_picture(media, format, &data);

        let stroke = match &shape.kind {
            ibis_doc_model::ShapeKind::Auto(a) => a.stroke.clone(),
            ibis_doc_model::ShapeKind::Picture(p) => p.stroke.clone(),
            _ => None,
        };
        let shadow = match &shape.kind {
            ibis_doc_model::ShapeKind::Auto(a) => a.shadow,
            ibis_doc_model::ShapeKind::Picture(p) => p.shadow,
            _ => None,
        };

        let mut picture = ibis_doc_model::Picture::new(media);
        picture.stroke = stroke;
        picture.shadow = shadow;

        let cmd = Command::SetKind {
            slide: current_slide_id,
            shape: shape_id,
            kind: Box::new(ibis_doc_model::ShapeKind::Picture(picture)),
        };
        self.run(vec![cmd], cx);
        true
    }

    /// Reset the zoom shape image back to its default preview card appearance.
    pub(crate) fn reset_zoom_image(&mut self, shape_id: ShapeId, cx: &mut Context<Self>) -> bool {
        if !self.has_custom_zoom_image(shape_id) {
            return false;
        }
        let current_slide_id = self.current_slide;
        let target_slide_id = match self.slide().shape_links.get(&shape_id) {
            Some(ibis_doc_model::SlideLink::Slide(id)) => *id,
            _ => return false,
        };

        let p = self.presentation();
        let target_idx = p.slide_index(target_slide_id).unwrap_or(0);
        let target_title = p
            .slide(target_slide_id)
            .and_then(|s| {
                s.shapes.iter().find_map(|sh| {
                    if let Some(tb) = sh.text_body() {
                        let t = tb.text().trim().to_string();
                        if !t.is_empty() {
                            return Some(t);
                        }
                    }
                    None
                })
            })
            .unwrap_or_else(|| format!("Slide {}", target_idx + 1));

        let tooltip = self.slide().shape_link_tooltips.get(&shape_id).cloned().unwrap_or_default();
        let is_section_zoom = tooltip.starts_with("Section Zoom");

        let header_text = if is_section_zoom {
            let sec_name = p.sections.iter().find(|s| s.slides.first() == Some(&target_slide_id)).map(|s| s.name.as_str()).unwrap_or("Section");
            format!("🔍 Section: {} (Zoom)\n{}", sec_name, target_title)
        } else {
            format!("🔍 Slide {} (Zoom)\n{}", target_idx + 1, target_title)
        };

        let stroke = ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xd0d0d0)), 12_700);
        let fill = ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffffff)));
        let body = ibis_doc_model::TextBody::plain(
            &header_text,
            ibis_doc_model::TextStyle {
                font_size: 14_00,
                color: ibis_doc_model::Rgb(0x333333),
                bold: true,
                ..Default::default()
            },
        );

        let mut auto = ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, fill, Some(stroke));
        auto.text = Some(body);

        let cmd = Command::SetKind {
            slide: current_slide_id,
            shape: shape_id,
            kind: Box::new(ibis_doc_model::ShapeKind::Auto(auto)),
        };
        self.run(vec![cmd], cx);
        true
    }

    /// Open file dialog to pick an image for the selected zoom shape.
    pub(crate) fn pick_zoom_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.selected_shapes().first().map(|s| s.id).filter(|&id| self.is_zoom_shape(id));
        let Some(shape_id) = target else { return };
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false, extensions: Vec::new(),
                multiple: false,
                prompt: Some(crate::shell::i18n::text("pptx.zoom.change-image").to_string().into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let name = path.to_string_lossy().into_owned();
                this.update(cx, |this, cx| {
                    this.change_zoom_image(shape_id, &name, data, cx);
                }).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Some((name, data))) = crate::web::pick_file().await else { return };
            this.update(cx, |this, cx| {
                this.change_zoom_image(shape_id, &name, data, cx);
            }).ok();
        })
        .detach();
    }

    /// Open the Insert Summary Zoom dialog.
    pub(crate) fn open_summary_zoom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current_id = self.current_slide;
        let mut initial = std::collections::HashSet::new();
        for s in &self.presentation().slides {
            if s.id != current_id {
                initial.insert(s.id);
            }
        }
        if let Some(pptx) = self.pptx_mut() {
            pptx.summary_zoom_dialog = Some(SummaryZoomDialog {
                selected_slides: initial,
            });
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Close the Insert Summary Zoom dialog.
    pub(crate) fn close_summary_zoom_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(pptx) = self.pptx_mut() {
            pptx.summary_zoom_dialog = None;
        }
        cx.notify();
    }

    /// Toggle selection of a slide in the Summary Zoom dialog.
    pub(crate) fn toggle_summary_zoom_slide(&mut self, slide_id: SlideId, cx: &mut Context<Self>) {
        if let Some(dlg) = self.pptx_mut().and_then(|p| p.summary_zoom_dialog.as_mut()) {
            if dlg.selected_slides.contains(&slide_id) {
                dlg.selected_slides.remove(&slide_id);
            } else {
                dlg.selected_slides.insert(slide_id);
            }
            cx.notify();
        }
    }

    /// Insert Summary Zoom cards targeting `target_slide_ids` onto the current slide.
    pub(crate) fn insert_summary_zoom(
        &mut self,
        target_slide_ids: &[SlideId],
        cx: &mut Context<Self>,
    ) -> Vec<ShapeId> {
        if target_slide_ids.is_empty() {
            return Vec::new();
        }

        let p = self.presentation();
        let mut sorted_targets: Vec<SlideId> = target_slide_ids.to_vec();
        sorted_targets.sort_by_key(|id| p.slide_index(*id).unwrap_or(usize::MAX));

        let current_slide_id = self.current_slide;
        let slide_size = p.slide_size;
        let count = sorted_targets.len();

        let cols = match count {
            1 => 1,
            2 => 2,
            3 => 3,
            4 => 2,
            5..=6 => 3,
            7..=8 => 4,
            _ => 4,
        };
        let rows = (count + cols - 1) / cols;

        let margin_x = (slide_size.width as f64 * 0.08) as i64;
        let margin_top = (slide_size.height as f64 * 0.18) as i64;
        let margin_bottom = (slide_size.height as f64 * 0.08) as i64;

        let avail_w = slide_size.width - margin_x * 2;
        let avail_h = slide_size.height - margin_top - margin_bottom;

        let gap_x = (avail_w as f64 * 0.04) as i64;
        let gap_y = (avail_h as f64 * 0.04) as i64;

        let total_gap_x = gap_x * (cols as i64 - 1).max(0);
        let total_gap_y = gap_y * (rows as i64 - 1).max(0);

        let cell_w = (avail_w - total_gap_x) / cols as i64;
        let cell_h = (avail_h - total_gap_y) / rows as i64;

        let slide_aspect = slide_size.width as f64 / slide_size.height as f64;
        let (card_w, card_h) = if cell_w as f64 / cell_h as f64 > slide_aspect {
            let h = cell_h;
            let w = (h as f64 * slide_aspect) as i64;
            (w, h)
        } else {
            let w = cell_w;
            let h = (w as f64 / slide_aspect) as i64;
            (w, h)
        };

        let grid_w = card_w * cols as i64 + total_gap_x;
        let grid_h = card_h * rows as i64 + total_gap_y;
        let offset_x = margin_x + (avail_w - grid_w) / 2;
        let offset_y = margin_top + (avail_h - grid_h) / 2;

        let mut commands = Vec::new();
        let mut created_ids = Vec::new();
        let mut insert_index = self.slide().shapes.len();

        for (i, &target_slide_id) in sorted_targets.iter().enumerate() {
            let row = i / cols;
            let col = i % cols;

            let x = offset_x + col as i64 * (card_w + gap_x);
            let y = offset_y + row as i64 * (card_h + gap_y);
            let frame = EmuRect::new(x, y, card_w, card_h);

            let (target_index, target_title) = {
                let p = self.presentation();
                let idx = p.slide_index(target_slide_id).unwrap_or(0);
                let title = p
                    .slide(target_slide_id)
                    .and_then(|s| {
                        s.shapes.iter().find_map(|sh| {
                            if let Some(tb) = sh.text_body() {
                                let t = tb.text().trim().to_string();
                                if !t.is_empty() {
                                    return Some(t);
                                }
                            }
                            None
                        })
                    })
                    .unwrap_or_else(|| format!("Slide {}", idx + 1));
                (idx + 1, title)
            };

            let shape_id = self.document.ids().new_shape_id();
            let stroke = Stroke::solid(Color::opaque(Rgb(0xd0d0d0)), 12_700);
            let fill = Fill::Solid(Color::opaque(Rgb(0xffffff)));

            let font_size = if cols >= 4 || rows >= 3 { 11_00 } else { 13_00 };
            let header_text = format!("🔍 Slide {} (Zoom)\n{}", target_index, target_title);
            let body = TextBody::plain(
                &header_text,
                TextStyle {
                    font_size,
                    color: Rgb(0x333333),
                    bold: true,
                    ..Default::default()
                },
            );

            let mut auto = AutoShape::new(Geometry::Rect, fill, Some(stroke));
            auto.text = Some(body);
            let shape = Shape::new(shape_id, frame, ibis_doc_model::ShapeKind::Auto(auto));

            let link_tooltip = format!("Slide {} Zoom: {}", target_index, target_title);

            commands.push(Command::InsertShape {
                slide: current_slide_id,
                index: insert_index,
                shape,
            });
            commands.push(Command::SetShapeLink {
                slide: current_slide_id,
                shape: shape_id,
                link: Some(SlideLink::Slide(target_slide_id)),
            });
            commands.push(Command::SetShapeLinkTooltip {
                slide: current_slide_id,
                shape: shape_id,
                tooltip: Some(link_tooltip),
            });
            commands.push(Command::SetShapeReturnToZoom {
                slide: current_slide_id,
                shape: shape_id,
                return_to_zoom: true,
            });

            created_ids.push(shape_id);
            insert_index += 1;
        }

        self.run(commands, cx);
        self.set_selection(created_ids.clone());
        created_ids
    }

    /// Render the Insert Summary Zoom modal dialog if open.
    pub(crate) fn render_summary_zoom_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pptx = self.pptx()?;
        let dialog = pptx.summary_zoom_dialog.as_ref()?;
        let selected = &dialog.selected_slides;
        let current_id = self.current_slide;
        let t = crate::shell::i18n::text;

        let slides_list = self
            .presentation()
            .slides
            .iter()
            .enumerate()
            .filter(|(_, s)| s.id != current_id)
            .map(|(idx, s)| {
                let s_id = s.id;
                let is_checked = selected.contains(&s_id);
                let title = s
                    .shapes
                    .iter()
                    .find_map(|sh| {
                        sh.text_body().and_then(|tb| {
                            let text = tb.text().trim().to_string();
                            (!text.is_empty()).then_some(text)
                        })
                    })
                    .unwrap_or_else(|| format!("{}: {}", t("_shell.slide"), idx + 1));

                div()
                    .id(("summary-zoom-slide-row", idx as u64))
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(is_checked, |d| d.bg(rgb(0xd3e3fd)))
                    .when(!is_checked, |d| d.hover(|h| h.bg(rgb(0xf1f3f4))))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_summary_zoom_slide(s_id, cx);
                    }))
                    .child(
                        div()
                            .w(px(18.))
                            .h(px(18.))
                            .rounded(px(3.))
                            .border_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .when(is_checked, |d| d.bg(rgb(0x1a73e8)).border_color(rgb(0x1a73e8)).text_color(rgb(0xffffff)).child("✓"))
                            .when(!is_checked, |d| d.bg(rgb(0xffffff)).border_color(rgb(0x757575))),
                    )
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(0x202124))
                                    .child(format!("{}. {}", idx + 1, title)),
                            ),
                    )
            })
            .collect::<Vec<_>>();

        let selected_vec: Vec<SlideId> = selected.iter().copied().collect();
        let count = selected.len();

        Some(
            div()
                .id("summary-zoom-modal-scrim")
                .absolute()
                .inset_0()
                .bg(rgba(0x00000066))
                .flex()
                .items_center()
                .justify_center()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.close_summary_zoom_dialog(cx);
                }))
                .child(
                    div()
                        .id("summary-zoom-modal-card")
                        .w(px(460.))
                        .max_w_full()
                        .bg(rgb(0xffffff))
                        .rounded_xl()
                        .shadow_lg()
                        .p_5()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .on_click(|_, _, _| {})
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_base()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(rgb(0x202124))
                                        .child(t("pptx.zoom.summary.dialog.title")),
                                )
                                .child(
                                    crate::shell::chrome::action_button("summary-zoom-close-btn")
                                        .small()
                                        .label("✕")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.close_summary_zoom_dialog(cx);
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x5f6368))
                                .child(t("pptx.zoom.summary.dialog.prompt")),
                        )
                        .child(
                            div()
                                .id("summary-zoom-slide-list")
                                .flex()
                                .flex_col()
                                .max_h(px(280.))
                                .overflow_y_scroll()
                                .gap_1()
                                .children(slides_list),
                        )
                        .child(
                            crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar()
                                .action(
                                    crate::shell::chrome::ActionDefinition::new("summary-zoom-insert", if count > 0 {
                                            format!("{} ({})", t("pptx.zoom.insert"), count)
                                        } else {
                                            t("pptx.zoom.insert").to_string()
                                        }, crate::shell::chrome::ActionButtonKind::Primary)
                                        .disabled(count == 0)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.insert_summary_zoom(&selected_vec, cx);
                                            this.close_summary_zoom_dialog(cx);
                                        })),
                                )
                                .action(
                                    crate::shell::chrome::ActionDefinition::new("summary-zoom-cancel", t("pptx.zoom.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.close_summary_zoom_dialog(cx);
                                        })),
                                ))),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// The open state of the Insert Section Zoom modal dialog.
pub(crate) struct SectionZoomDialog {
    /// The currently selected target section id to zoom to.
    pub(crate) selected_section_id: Option<String>,
}

/// The open state of the Insert Summary Zoom modal dialog.
pub(crate) struct SummaryZoomDialog {
    /// The currently selected target slides to include in the summary zoom.
    pub(crate) selected_slides: std::collections::HashSet<SlideId>,
}
