// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing a page of a flowing document, as `flow_layout` laid it out.

use std::sync::Arc;

use gpui_kit::*;
use ibis_doc_model::{EMU_PER_POINT, EmuRect, FlowDocument};

use crate::docx::flow_layout::{Item, pages};
use crate::render::shape_view::Resources;

/// Page `index` of `document`, filling `frame` (frame-relative).
pub(crate) fn page_element(document: &Arc<FlowDocument>, index: usize, frame: EmuRect, scale: f32, res: &Resources) -> Div {
    let (w, h) = (frame.width as f32 * scale, frame.height as f32 * scale);
    let pagination = pages(document, res);
    let k = scale * EMU_PER_POINT as f32;
    let mut behind_pictures = Vec::new();
    let mut front_pictures = Vec::new();
    let hide_body = res.hide_document_text;
    let page_ref = pagination.pages.get(index);
    let body_items_count = page_ref.map_or(0, |p| p.body_items_count);
    for (i, item) in page_ref.into_iter().flat_map(|p| p.items.iter().enumerate()) {
        if hide_body && i < body_items_count {
            continue;
        }
        match item {
            Item::Picture { media, x, y, w, h } => {
                if let Some(image) = res.picture_at_size(*media, w * k, h * k) {
                    front_pictures.push(
                        img(image).absolute().left(px(x * k)).top(px(y * k)).w(px(w * k)).h(px(h * k)).object_fit(ObjectFit::Fill).into_any_element(),
                    );
                }
            }
            Item::Art { art, part, x, y, w, h, image_w, image_h, tint } => {
                let device = crate::docx::art_border::device_px(image_w.min(*image_h), k * crate::docx::art_border::device_scale());
                if let Some(image) = crate::docx::art_border::image(*art, *part, *tint, device) {
                    behind_pictures.push(
                        div()
                            .absolute()
                            .left(px(x * k))
                            .top(px(y * k))
                            .w(px(w * k))
                            .h(px(h * k))
                            .overflow_hidden()
                            .child(img(res.svg_picture(image)).absolute().left_0().top_0().w(px(image_w * k)).h(px(image_h * k)).object_fit(ObjectFit::Fill))
                            .into_any_element(),
                    );
                }
            }
            Item::Shape { shape, x, y, w, h } => {
                let s_frame = EmuRect::new(
                    (x * EMU_PER_POINT as f32).round() as i64,
                    (y * EMU_PER_POINT as f32).round() as i64,
                    (w * EMU_PER_POINT as f32).round() as i64,
                    (h * EMU_PER_POINT as f32).round() as i64,
                );
                let positions = res
                    .edited_box
                    .as_ref()
                    .and_then(|(id, pos, _)| (*id == shape.id).then(|| pos.clone()));
                let elem = crate::render::shape_view::shape_element_with_positions(shape, s_frame, scale, res, None, positions.as_ref());
                let is_behind = shape.floating.as_ref().map_or(false, |f| f.behind_doc);
                if is_behind {
                    behind_pictures.push(elem);
                } else {
                    front_pictures.push(elem);
                }
            }
            Item::CommentBubble { x, y, size } if !res.print => {
                front_pictures.push(
                    div()
                        .absolute()
                        .left(px(x * k))
                        .top(px(y * k))
                        .w(px(size * k))
                        .h(px(size * k))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            gpui_kit::component::Icon::new(crate::shell::icons::Material("comment"))
                                .size(px(size * k))
                                .text_color(rgb(crate::docx::flow_layout::COMMENT_BUBBLE_ICON.0)),
                        )
                        .into_any_element(),
                );
            }
            _ => {}
        }
    }
    let print = res.print;
    // ページの色: on the screen only (Word for Mac prints none) unless print_backgrounds is enabled.
    let dark = res.dark_page && !print;
    let paper = if dark { DARK_PAPER } else { document.page_color.filter(|_| !print || res.print_backgrounds).map_or(0xffffff, |c| c.0) };
    div().absolute().left_0().top_0().w(px(w)).h(px(h)).bg(rgb(paper)).children(behind_pictures).child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let o = bounds.origin;
                // The art borders' pictures are made for the window's scale
                // (an export sets its own, its pictures made ahead for it).
                if !print && crate::docx::art_border::set_device_scale(window.scale_factor()) {
                    window.refresh();
                }
                // Art borders' pictures on their way (the web version): draw
                // again when they may have come.
                if crate::docx::art_border::pending() {
                    window.request_animation_frame();
                }
                let Some(page) = pagination.pages.get(index) else { return };
                let visible = window.content_mask().bounds;
                for (i, item) in page.items.iter().enumerate() {
                    if hide_body && i < page.body_items_count {
                        continue;
                    }
                    match item {
                        Item::Rect { x, y, w, h, color } => {
                            let color = &dark_color(*color, dark);
                            // At least a device pixel, so thin rules show.
                            let min = 1.0 / window.scale_factor();
                            let r = Bounds::new(point(o.x + px(x * k), o.y + px(y * k)), size(px((w * k).max(min)), px((h * k).max(min))));
                            if r.intersects(&visible) {
                                window.paint_quad(fill(r, *color));
                            }
                        }
                        Item::Text { x, y, text, offsets, font, size: font_size, color } => {
                            let color = &dark_color(*color, dark);
                            let size_px = font_size * k;
                            let top = o.y + px(y * k - size_px);
                            if top > visible.origin.y + visible.size.height || top + px(size_px * 2.0) < visible.origin.y {
                                continue;
                            }
                            for ((i, c), dx) in text.char_indices().zip(offsets.iter()) {
                                let s = &text[i..i + c.len_utf8()];
                                if s.trim().is_empty() {
                                    continue;
                                }
                                let run = TextRun { len: s.len(), font: font.clone(), color: *color, background_color: None, underline: None, strikethrough: None };
                                let shaped = window.text_system().shape_line(SharedString::from(s.to_string()), px(size_px), &[run], None);
                                if font.features.tag_value_list().iter().any(|(tag,value)|tag=="vert" && *value==1)
                                    && let Some(run)=shaped.runs.first()
                                    && crate::render::vertical_text::paint_glyph(window,point(o.x+px((x+dx)*k),o.y+px(y*k)),run.font_id,c,size_px,*color)
                                {
                                    continue;
                                }
                                if matches!(c, '☐' | '☑' | '☒') && f32::from(shaped.width) < size_px * 0.85 {
                                    let box_run = TextRun { len: "□".len(), font: font.clone(), color: *color, background_color: None, underline: None, strikethrough: None };
                                    let box_shaped = window.text_system().shape_line(SharedString::from("□"), px(size_px), &[box_run], None);
                                    if f32::from(box_shaped.width) >= size_px * 0.9 {
                                        let ascent = f32::from(box_shaped.ascent);
                                        let origin = point(o.x + px((x + dx) * k), o.y + px(y * k - ascent));
                                        box_shaped.paint(origin, box_shaped.ascent + box_shaped.descent, TextAlign::Left, None, window, cx).ok();
                                        if c != '☐' {
                                            let mark_str = if c == '☑' { "✓" } else { "×" };
                                            let mark_run = TextRun { len: mark_str.len(), font: font.clone(), color: *color, background_color: None, underline: None, strikethrough: None };
                                            let mark_size = size_px * 0.75;
                                            let mark_shaped = window.text_system().shape_line(SharedString::from(mark_str), px(mark_size), &[mark_run], None);
                                            let mx = o.x + px((x + dx) * k + (size_px - f32::from(mark_shaped.width)) * 0.5);
                                            let my = o.y + px(y * k - f32::from(mark_shaped.ascent) - size_px * 0.08);
                                            mark_shaped.paint(point(mx, my), mark_shaped.ascent + mark_shaped.descent, TextAlign::Left, None, window, cx).ok();
                                        }
                                        continue;
                                    }
                                }
                                // The line box's top: the baseline less the ascent.
                                let ascent = f32::from(shaped.ascent);
                                let origin = point(o.x + px((x + dx) * k), o.y + px(y * k - ascent));
                                shaped.paint(origin, shaped.ascent + shaped.descent, TextAlign::Left, None, window, cx).ok();
                            }
                        }
                        Item::Turned { x, y, text, font, size: font_size, color, turns, xscale } => {
                            // Each glyph moved where turning the line about its
                            // start takes it, and turned about itself there.
                            let run = TextRun { len: text.len(), font: font.clone(), color: *color, background_color: None, underline: None, strikethrough: None };
                            let shaped = window.text_system().shape_line(text.clone(), px(font_size * k), &[run], None);
                            let angle = f32::from(*turns) * std::f32::consts::FRAC_PI_2;
                            let (sin, cos) = angle.sin_cos();
                            let start = point(o.x + px(x * k), o.y + px(y * k));
                            let scale = window.scale_factor();
                            for r in &shaped.runs {
                                for glyph in &r.glyphs {
                                    let (dx, dy) = (f32::from(glyph.position.x), f32::from(glyph.position.y));
                                    let turned = point(start.x + px(dx * cos - dy * sin), start.y + px(dx * sin + dy * cos));
                                    let pivot = point(ScaledPixels(f32::from(turned.x) * scale), ScaledPixels(f32::from(turned.y) * scale));
                                    let transformation = TransformationMatrix::unit()
                                        .translate(pivot)
                                        .rotate(Radians(angle))
                                        .scale(size(*xscale, 1.0))
                                        .translate(point(ScaledPixels(-pivot.x.0), ScaledPixels(-pivot.y.0)));
                                    let _ = window.paint_glyph_transformed(turned, r.font_id, glyph.id, px(font_size * k), *color, transformation);
                                }
                            }
                        }
                        Item::Scaled { x, y, text, font, size: font_size, color, xscale } => {
                            // The glyph squeezed about where it starts.
                            let color = &dark_color(*color, dark);
                            let run = TextRun { len: text.len(), font: font.clone(), color: *color, background_color: None, underline: None, strikethrough: None };
                            let shaped = window.text_system().shape_line(text.clone(), px(font_size * k), &[run], None);
                            let start = point(o.x + px(x * k), o.y + px(y * k));
                            let scale = window.scale_factor();
                            let pivot = point(ScaledPixels(f32::from(start.x) * scale), ScaledPixels(f32::from(start.y) * scale));
                            for r in &shaped.runs {
                                for glyph in &r.glyphs {
                                    let at = point(start.x + glyph.position.x, start.y + glyph.position.y);
                                    let transformation = TransformationMatrix::unit()
                                        .translate(pivot)
                                        .scale(size(*xscale, 1.0))
                                        .translate(point(ScaledPixels(-pivot.x.0), ScaledPixels(-pivot.y.0)));
                                    let _ = window.paint_glyph_transformed(at, r.font_id, glyph.id, px(font_size * k), *color, transformation);
                                }
                            }
                        }
                        Item::Stroke { x, y, x2, y2, w, color } => {
                            let color = &dark_color(*color, dark);
                            let mut path = PathBuilder::stroke(px((w * k).max(1.0 / window.scale_factor())));
                            path.move_to(point(o.x + px(x * k), o.y + px(y * k)));
                            path.line_to(point(o.x + px(x2 * k), o.y + px(y2 * k)));
                            if let Ok(path) = path.build() {
                                window.paint_path(path, *color);
                            }
                        }
                        Item::Picture { .. } | Item::Art { .. } | Item::Shape { .. } | Item::Line { .. } | Item::Shading { .. } | Item::CommentBubble { .. } => {}
                    }
                }
            },
        )
        .absolute()
        .size_full(),
    )
    .children(front_pictures)
}

/// The paper of a page shown dark.
const DARK_PAPER: u32 = 0x262626;

/// A colour as it is on a dark page: black (what the text and the lines are by default) is a light grey, a lighter colour stays.
fn dark_color(color: Hsla, dark: bool) -> Hsla {
    if dark && color.l < 0.15 && color.a > 0.0 {
        Hsla { h: color.h, s: 0.0, l: 0.95, a: color.a }
    } else {
        color
    }
}

#[cfg(test)]
mod dark_tests {
    use super::dark_color;
    use gpui_kit::hsla;

    #[test]
    fn a_dark_page_lightens_black_and_leaves_the_rest() {
        let black = hsla(0.0, 0.0, 0.0, 1.0);
        assert!(dark_color(black, true).l > 0.9, "black text is light on a dark page");
        assert_eq!(dark_color(black, false), black, "the light page is as it was");
        let red = hsla(0.0, 1.0, 0.4, 1.0);
        assert_eq!(dark_color(red, true), red, "a colour stays");
        let white = hsla(0.0, 0.0, 1.0, 1.0);
        assert_eq!(dark_color(white, true), white);
    }
}
