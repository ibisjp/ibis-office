// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 図の圧縮 (図の形式 > 図の圧縮…): the pictures made smaller by taking away
//! the pixels the slide cannot show at the resolution chosen (印刷用 220 ppi,
//! Web 150 ppi, 電子メール 96 ppi), and, if asked, the parts cut off by
//! the crop. A JPEG stays a JPEG (quality 85), a PNG a PNG; a picture that
//! does not get smaller is left alone.
use std::io::Cursor;

use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
#[cfg(all(any(target_os = "macos", windows, target_os = "linux"), feature = "export"))]
use image::{ImageEncoder as _, ImageFormat};
use ibis_doc_model::{Command, Crop, MediaFormat, MediaId, ShapeId, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// EMU in an inch.
const EMU_PER_INCH: f64 = 914_400.0;

/// The resolution the pictures are compressed to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Resolution {
    /// 高精細 (330 ppi).
    Hd,
    /// 印刷用 (220 ppi).
    #[default]
    Print,
    /// Web (150 ppi).
    Web,
    /// 電子メール (96 ppi).
    Email,
}

impl Resolution {
    pub(crate) fn ppi(self) -> f64 {
        match self {
            Resolution::Hd => 330.0,
            Resolution::Print => 220.0,
            Resolution::Web => 150.0,
            Resolution::Email => 96.0,
        }
    }

    pub(crate) fn from_dpi(dpi: u32) -> Self {
        match dpi {
            330.. => Resolution::Hd,
            220.. => Resolution::Print,
            150.. => Resolution::Web,
            _ => Resolution::Email,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn dpi(self) -> u32 {
        match self {
            Resolution::Hd => 330,
            Resolution::Print => 220,
            Resolution::Web => 150,
            Resolution::Email => 96,
        }
    }
}

/// What a compress made of one picture.
#[allow(dead_code)]
pub(crate) struct Compressed {
    pub(crate) data: Vec<u8>,
    pub(crate) format: MediaFormat,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// Picture `data` (`format`) shown `width_emu` × `height_emu` with `crop`,
/// compressed to `ppi`, with the cropped parts taken off if `delete_crop`;
/// `None` when it cannot be read or does not get smaller.
#[cfg(all(any(target_os = "macos", windows, target_os = "linux"), feature = "export"))]
pub(crate) fn compress_picture(data: &[u8], format: MediaFormat, width_emu: i64, height_emu: i64, crop: Crop, ppi: f64, delete_crop: bool) -> Option<Compressed> {
    let (kind, is_jpeg) = match format {
        MediaFormat::Jpeg => (ImageFormat::Jpeg, true),
        MediaFormat::Png => (ImageFormat::Png, false),
        _ => return None,
    };
    let mut image = image::load_from_memory_with_format(data, kind).ok()?;
    let (w0, h0) = (image.width(), image.height());
    let frac = |a: i32, b: i32| (1.0 - (a + b) as f64 / 100_000.0).max(0.01);
    let (fw, fh) = (frac(crop.left, crop.right), frac(crop.top, crop.bottom));
    let cropped = delete_crop && (crop.left != 0 || crop.top != 0 || crop.right != 0 || crop.bottom != 0) && crop.left >= 0 && crop.top >= 0 && crop.right >= 0 && crop.bottom >= 0;
    if cropped {
        let x = (w0 as f64 * crop.left as f64 / 100_000.0).round() as u32;
        let y = (h0 as f64 * crop.top as f64 / 100_000.0).round() as u32;
        let w = ((w0 as f64 * fw).round() as u32).clamp(1, w0 - x.min(w0 - 1));
        let h = ((h0 as f64 * fh).round() as u32).clamp(1, h0 - y.min(h0 - 1));
        image = image.crop_imm(x.min(w0 - 1), y.min(h0 - 1), w, h);
    }
    // The size the whole picture is shown at, inches.
    let (shown_w, shown_h) = if cropped { (width_emu as f64 / EMU_PER_INCH, height_emu as f64 / EMU_PER_INCH) } else { (width_emu as f64 / EMU_PER_INCH / fw, height_emu as f64 / EMU_PER_INCH / fh) };
    let (want_w, want_h) = ((shown_w * ppi).ceil().max(1.0), (shown_h * ppi).ceil().max(1.0));
    let scale = (want_w / image.width() as f64).min(want_h / image.height() as f64).min(1.0);
    if scale < 1.0 {
        let (w, h) = (((image.width() as f64 * scale).round() as u32).max(1), ((image.height() as f64 * scale).round() as u32).max(1));
        image = image.resize_exact(w, h, image::imageops::FilterType::Lanczos3);
    } else if !cropped {
        return None;
    }
    let (width, height) = (image.width(), image.height());
    let mut out = Vec::new();
    if is_jpeg {
        let rgb = image.to_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut Cursor::new(&mut out), 85).write_image(rgb.as_raw(), width, height, image::ExtendedColorType::Rgb8).ok()?;
    } else {
        let rgba = image.to_rgba8();
        image::codecs::png::PngEncoder::new_with_quality(&mut Cursor::new(&mut out), image::codecs::png::CompressionType::Best, image::codecs::png::FilterType::Adaptive)
            .write_image(rgba.as_raw(), width, height, image::ExtendedColorType::Rgba8)
            .ok()?;
    }
    (out.len() < data.len()).then_some(Compressed { data: out, format, width, height })
}

#[cfg(not(all(any(target_os = "macos", windows, target_os = "linux"), feature = "export")))]
pub(crate) fn compress_picture(_data: &[u8], _format: MediaFormat, _width_emu: i64, _height_emu: i64, _crop: Crop, _ppi: f64, _delete_crop: bool) -> Option<Compressed> {
    None
}

/// The panel's choices.
pub(crate) struct CompressPanel {
    pub(crate) resolution: Resolution,
    pub(crate) delete_crop: bool,
    /// Only the selected pictures (else every picture of the presentation).
    pub(crate) selected_only: bool,
}

impl SlideEditor {
    /// 図の圧縮…: the panel, the selected pictures its scope when there are some.
    pub(crate) fn open_compress_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let any_selected = self.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Picture(_)));
        if let Some(s) = self.pptx_mut() {
            s.section_panel = None;
            s.layout_panel = None;
            s.print_panel = None;
            s.compress_panel = Some(CompressPanel { resolution: Resolution::Print, delete_crop: true, selected_only: any_selected });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_compress_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.compress_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn edit_compress_panel(&mut self, change: impl FnOnce(&mut CompressPanel), cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.compress_panel.as_mut()) {
            change(p);
        }
        cx.notify();
    }

    /// The pictures compressed as asked: each medium made once (for the
    /// largest the slides show it at), its pictures pointed at it. Returns how
    /// many media got smaller and the bytes before and after.
    pub(crate) fn compress_pictures(&mut self, resolution: Resolution, delete_crop: bool, selected_only: bool, cx: &mut Context<Self>) -> (usize, usize, usize) {
        let selected: Vec<ShapeId> = self.selected_ids();
        // Every use of each medium: slide, shape, frame size, crop.
        let mut uses: Vec<(MediaId, ibis_doc_model::SlideId, ShapeId, i64, i64, Crop)> = Vec::new();
        for slide in &self.presentation().slides {
            for shape in &slide.shapes {
                if let ShapeKind::Picture(p) = &shape.kind {
                    if p.clip.is_some() || (selected_only && !(slide.id == self.current_slide && selected.contains(&shape.id))) {
                        continue;
                    }
                    uses.push((p.media, slide.id, shape.id, shape.frame.width, shape.frame.height, p.crop));
                }
            }
        }
        let mut done: Vec<(MediaId, MediaFormat, std::sync::Arc<[u8]>)> = Vec::new();
        let mut medium_ids: Vec<MediaId> = uses.iter().map(|u| u.0).collect();
        medium_ids.sort();
        medium_ids.dedup();
        let (mut count, mut before, mut after) = (0, 0, 0);
        let mut replacement: std::collections::HashMap<MediaId, (MediaId, bool)> = std::collections::HashMap::new();
        for media in medium_ids {
            let Some(m) = self.presentation().media(media).cloned() else { continue };
            let mine: Vec<_> = uses.iter().filter(|u| u.0 == media).collect();
            // The crop is taken off only from a medium one picture shows.
            let take_crop = delete_crop && mine.len() == 1;
            // The largest it is shown at decides the resolution.
            let (w, h) = (mine.iter().map(|u| u.3).max().unwrap_or(1), mine.iter().map(|u| u.4).max().unwrap_or(1));
            let crop = if mine.len() == 1 { mine[0].5 } else { Crop::default() };
            let Some(c) = compress_picture(&m.data, m.format, w, h, crop, resolution.ppi(), take_crop) else { continue };
            before += m.data.len();
            after += c.data.len();
            count += 1;
            let data: std::sync::Arc<[u8]> = c.data.into();
            let new = self.document.ids().add_media(c.format, data.clone());
            self.resources.add_picture(new, c.format, &data);
            done.push((new, c.format, data));
            replacement.insert(media, (new, take_crop));
        }
        let mut commands = Vec::new();
        for (media, slide, shape, ..) in uses {
            let Some((new, took)) = replacement.get(&media).copied() else { continue };
            let Some(sh) = self.presentation().slide(slide).and_then(|s| s.shape(shape)).cloned() else { continue };
            if let ShapeKind::Picture(mut p) = sh.kind {
                p.media = new;
                if took {
                    p.crop = Crop::default();
                }
                commands.push(Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Picture(p)) });
            }
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
        (count, before, after)
    }

    pub(crate) fn apply_compress_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((resolution, delete_crop, selected_only)) = self.pptx().and_then(|s| s.compress_panel.as_ref()).map(|p| (p.resolution, p.delete_crop, p.selected_only)) else { return };
        let (count, before, after) = self.compress_pictures(resolution, delete_crop, selected_only, cx);
        self.close_compress_panel(window, cx);
        let message = if count == 0 {
            crate::shell::i18n::text("pptx.compress.no-pictures-can-made-smaller").to_string()
        } else {
            crate::shell::i18n::format("pptx.compress.compressed-n-picture-s-before", &[("n", &count), ("before", &(before / 1024)), ("after", &(after / 1024))])
        };
        self.report(Ok(message), cx);
    }

    pub(crate) fn compress_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.compress_panel.as_ref()?;
        let (resolution, delete_crop, selected_only) = (p.resolution, p.delete_crop, p.selected_only);
        let any_selected = self.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Picture(_)));
        let t = crate::shell::i18n::text;
        let heading = |label: &str| div().text_sm().font_weight(FontWeight::BOLD).child(t(label));
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(heading("pptx.compress.compression-options"))
            .child(Checkbox::new("pptx-compress-delete").label(t("pptx.compress.delete-cropped-areas-pictures")).checked(delete_crop).on_click(cx.listener(|e, on: &bool, _, cx| {
                let on = *on;
                e.edit_compress_panel(|p| p.delete_crop = on, cx)
            })))
            .child(Checkbox::new("pptx-compress-selected").label(t("pptx.compress.apply-only-selected-pictures")).checked(selected_only && any_selected).on_click(cx.listener(|e, on: &bool, _, cx| {
                let on = *on;
                e.edit_compress_panel(|p| p.selected_only = on, cx)
            })))
            .child(heading("pptx.compress.resolution"))
            .child(crate::shell::chrome::radio_button("pptx-compress-print", resolution == Resolution::Print, false, crate::shell::i18n::text("pptx.compress.print-220-ppi"), cx.listener(|e, _, _, cx| e.edit_compress_panel(|p| p.resolution = Resolution::Print, cx))))
            .child(crate::shell::chrome::radio_button("pptx-compress-web", resolution == Resolution::Web, false, crate::shell::i18n::text("pptx.compress.web-150-ppi"), cx.listener(|e, _, _, cx| e.edit_compress_panel(|p| p.resolution = Resolution::Web, cx))))
            .child(crate::shell::chrome::radio_button("pptx-compress-email", resolution == Resolution::Email, false, crate::shell::i18n::text("pptx.compress.e-mail-96-ppi"), cx.listener(|e, _, _, cx| e.edit_compress_panel(|p| p.resolution = Resolution::Email, cx))))
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("pptx-compress-apply", t("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_compress_panel(w, cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("pptx-compress-cancel", t("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_compress_panel(w, cx))));
        Some(SidebarPanel::with_footer(t("pptx.compress.compress-pictures"), body.into_any_element(), panel_actions))
    }
}

#[cfg(test)]
mod tests {
    use super::{compress_picture, Crop, MediaFormat};
    use image::{ImageEncoder as _, Rgb, RgbImage};

    /// A noisy picture, so that it does not compress by itself.
    fn noisy(w: u32, h: u32) -> RgbImage {
        let mut state = 12345u32;
        RgbImage::from_fn(w, h, |x, y| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            Rgb([(x * 255 / w) as u8 ^ (state >> 24) as u8 / 8, (y * 255 / h) as u8, (state >> 16) as u8 / 2])
        })
    }

    fn jpeg(image: &RgbImage) -> Vec<u8> {
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut std::io::Cursor::new(&mut out), 100).write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgb8).unwrap();
        out
    }

    #[test]
    fn pictures_get_smaller() {
        let source = noisy(1600, 800);
        let data = jpeg(&source);
        // Shown 4 × 2 inches: 150 ppi is 600 × 300.
        let made = compress_picture(&data, MediaFormat::Jpeg, 4 * 914_400, 2 * 914_400, Crop::default(), 150.0, false).expect("smaller");
        assert_eq!((made.width, made.height), (600, 300));
        assert!(made.data.len() < data.len() / 4, "{} of {}", made.data.len(), data.len());
        assert!(made.data.starts_with(&[0xff, 0xd8, 0xff]));
        // Already small for its size: left alone.
        assert!(compress_picture(&data, MediaFormat::Jpeg, 20 * 914_400, 10 * 914_400, Crop::default(), 150.0, false).is_none());
        // A PNG stays a PNG, with its alpha.
        let mut png = Vec::new();
        let rgba = image::DynamicImage::ImageRgb8(source).to_rgba8();
        image::codecs::png::PngEncoder::new(&mut std::io::Cursor::new(&mut png)).write_image(rgba.as_raw(), 1600, 800, image::ExtendedColorType::Rgba8).unwrap();
        let made = compress_picture(&png, MediaFormat::Png, 4 * 914_400, 2 * 914_400, Crop::default(), 96.0, false).expect("smaller");
        assert_eq!((made.width, made.height), (384, 192));
        assert!(made.data.starts_with(b"\x89PNG"));
        // Other formats are not touched.
        assert!(compress_picture(&data, MediaFormat::Gif, 914_400, 914_400, Crop::default(), 96.0, false).is_none());
    }

    #[test]
    fn the_cropped_part_goes() {
        let data = jpeg(&noisy(1600, 800));
        // Half cut from the right: the left 800 × 800 stays, shown 2 × 2 inches at 150 ppi: 300 × 300.
        let crop = Crop { left: 0, top: 0, right: 50_000, bottom: 0 };
        let made = compress_picture(&data, MediaFormat::Jpeg, 2 * 914_400, 2 * 914_400, crop, 150.0, true).expect("smaller");
        assert_eq!((made.width, made.height), (300, 300));
        // Kept (not deleted): the whole picture is shown 4 inches wide for a 2-inch window.
        let kept = compress_picture(&data, MediaFormat::Jpeg, 2 * 914_400, 2 * 914_400, crop, 150.0, false).expect("smaller");
        assert_eq!((kept.width, kept.height), (600, 300));
    }
}
