// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 図の圧縮・画像のサイズと品質設定 (Word の「図の圧縮…」「画像のサイズと品質」、DOCX-OPTIONS-011).
use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::{h_flex, v_flex, Disableable as _, Sizable as _};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Block, Crop, FlowCommand, FlowPos, FlowRunKind, MediaFormat, MediaId, ShapeKind};
#[cfg(all(target_os = "macos", feature = "export"))]
use crate::pptx::compress::{compress_picture, Resolution};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

#[cfg(not(all(target_os = "macos", feature = "export")))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Resolution {
    Hd,
    #[default]
    Print,
    Web,
    Email,
}

#[cfg(not(all(target_os = "macos", feature = "export")))]
impl Resolution {
    pub(crate) fn from_dpi(dpi: u32) -> Self {
        match dpi {
            330.. => Resolution::Hd,
            220.. => Resolution::Print,
            150.. => Resolution::Web,
            _ => Resolution::Email,
        }
    }
}

/// Collect all picture usages across document blocks.
/// Returns `(media_id, pos, width_emu, height_emu, crop, is_shape)`.
#[cfg_attr(not(all(target_os = "macos", feature = "export")), allow(dead_code))]
fn collect_pictures_in_blocks(blocks: &[Block], out: &mut Vec<(MediaId, FlowPos, i64, i64, Crop, bool)>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                let mut offset = 0;
                for r in &p.runs {
                    match &r.kind {
                        FlowRunKind::Picture { media, size, look } => {
                            out.push((*media, FlowPos::new(p.id, offset), size.width, size.height, look.crop, false));
                        }
                        FlowRunKind::Shape(s) => {
                            if let ShapeKind::Picture(pic) = &s.kind {
                                out.push((pic.media, FlowPos::new(p.id, offset), s.frame.width, s.frame.height, pic.crop, true));
                            }
                        }
                        _ => {}
                    }
                    offset += r.len();
                }
            }
            Block::Table(t) => {
                for r in &t.rows {
                    for c in &r.cells {
                        collect_pictures_in_blocks(&c.blocks, out);
                    }
                }
            }
        }
    }
}

/// The panel's choices for picture compression.
#[derive(Debug, Clone)]
pub(crate) struct DocxCompressState {
    pub(crate) resolution: Resolution,
    pub(crate) delete_crop: bool,
    pub(crate) selected_only: bool,
}

impl SlideEditor {
    pub(crate) fn open_docx_compress_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let has_pic = self.picture_at_caret().is_some();
        let default_res = self.default_image_dpi().map(Resolution::from_dpi).unwrap_or(Resolution::Print);
        let discard_crop = self.is_discard_image_editing_data() || true;
        if let Some(s) = self.doc_state_mut() {
            s.picture_edit = None;
            s.table_size = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.style_edit = None;
            s.link = None;
            s.find = None;
            s.comments = None;
            s.compress_panel = Some(DocxCompressState {
                resolution: default_res,
                delete_crop: discard_crop,
                selected_only: has_pic,
            });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_docx_compress_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.compress_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn edit_docx_compress_panel(&mut self, change: impl FnOnce(&mut DocxCompressState), cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.compress_panel.as_mut()) {
            change(p);
        }
        cx.notify();
    }

    pub(crate) fn docx_compress_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.compress_panel.as_ref()?;
        let resolution = p.resolution;
        let delete_crop = p.delete_crop;
        let selected_only = p.selected_only;
        let has_pic = self.picture_at_caret().is_some();

        let t = |k: &str| crate::shell::i18n::text(k);
        let heading = |k: &str| div().text_sm().text_color(rgb(0x5f6368)).font_weight(FontWeight::SEMIBOLD).mt_2().child(t(k));

        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(heading("docx.compress.target"))
            .child(crate::shell::chrome::radio_group()
                .child(crate::shell::chrome::radio_button("docx-compress-all", !selected_only, false, t("docx.compress.entire-file"),
                    cx.listener(|e, _, _, cx| e.edit_docx_compress_panel(|p| p.selected_only = false, cx))))
                .child(crate::shell::chrome::radio_button("docx-compress-selected", selected_only && has_pic, !has_pic, t("docx.compress.selected-picture"),
                    cx.listener(|e, _, _, cx| e.edit_docx_compress_panel(|p| p.selected_only = true, cx)))))
            .child(heading("docx.compress.options"))
            .child(
                Checkbox::new("docx-compress-delete")
                    .label(t("docx.compress.delete-cropped"))
                    .checked(delete_crop)
                    .on_click(cx.listener(|e, on: &bool, _, cx| {
                        e.edit_docx_compress_panel(|p| p.delete_crop = *on, cx);
                    })),
            )
            .child(heading("docx.compress.resolution"))
            .child(crate::shell::chrome::radio_group()
                .children([
                    ("docx-compress-hd", Resolution::Hd, "docx.compress.hd-330-ppi"),
                    ("docx-compress-print", Resolution::Print, "docx.compress.print-220-ppi"),
                    ("docx-compress-web", Resolution::Web, "docx.compress.web-150-ppi"),
                    ("docx-compress-email", Resolution::Email, "docx.compress.email-96-ppi"),
                ].into_iter().map(|(id, value, label)| crate::shell::chrome::radio_button(
                    id, resolution == value, false, t(label),
                    cx.listener(move |e, _, _, cx| e.edit_docx_compress_panel(|p| p.resolution = value, cx)),
                ))));
        let footer = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("docx-compress-apply", t("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary)
                .on_click(cx.listener(|e, _, w, cx| e.apply_docx_compress_panel(w, cx))))
            .action(crate::shell::chrome::ActionDefinition::new("docx-compress-cancel", t("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                .on_click(cx.listener(|e, _, w, cx| e.close_docx_compress_panel(w, cx))));
        Some(SidebarPanel::with_footer(t("docx.compress.title"), body.into_any_element(), footer))
    }

    pub(crate) fn apply_docx_compress_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((resolution, delete_crop, selected_only)) = self
            .doc_state()
            .and_then(|s| s.compress_panel.as_ref())
            .map(|p| (p.resolution, p.delete_crop, p.selected_only))
        else {
            return;
        };
        let (count, before, after) = self.compress_docx_pictures(resolution, delete_crop, selected_only, cx);
        self.close_docx_compress_panel(window, cx);
        let message = if count == 0 {
            crate::shell::i18n::text("docx.compress.no-smaller").to_string()
        } else {
            crate::shell::i18n::format(
                "docx.compress.compressed-n",
                &[
                    ("n", &count.to_string()),
                    ("before", &(before / 1024).to_string()),
                    ("after", &(after / 1024).to_string()),
                ],
            )
        };
        self.set_status(message, cx);
    }

    /// Compress pictures across the document according to `resolution` and `delete_crop`.
    /// Returns `(compressed_count, before_bytes, after_bytes)`.
    #[cfg(not(all(target_os = "macos", feature = "export")))]
    pub(crate) fn compress_docx_pictures(
        &mut self,
        _resolution: Resolution,
        _delete_crop: bool,
        _selected_only: bool,
        _cx: &mut Context<Self>,
    ) -> (usize, usize, usize) {
        (0, 0, 0)
    }

    /// Compress pictures across the document according to `resolution` and `delete_crop`.
    /// Returns `(compressed_count, before_bytes, after_bytes)`.
    #[cfg(all(target_os = "macos", feature = "export"))]
    pub(crate) fn compress_docx_pictures(
        &mut self,
        resolution: Resolution,
        delete_crop: bool,
        selected_only: bool,
        cx: &mut Context<Self>,
    ) -> (usize, usize, usize) {
        let mut uses = Vec::new();
        if let Some(flow) = self.flow() {
            collect_pictures_in_blocks(&flow.body, &mut uses);
            collect_pictures_in_blocks(&flow.header, &mut uses);
            collect_pictures_in_blocks(&flow.footer, &mut uses);
            collect_pictures_in_blocks(&flow.header_first, &mut uses);
            collect_pictures_in_blocks(&flow.footer_first, &mut uses);
            collect_pictures_in_blocks(&flow.header_even, &mut uses);
            collect_pictures_in_blocks(&flow.footer_even, &mut uses);
        }
        let caret_pic = self.picture_at_caret().map(|(at, ..)| at);
        if selected_only {
            if let Some(at) = caret_pic {
                uses.retain(|u| u.1 == at);
            } else {
                return (0, 0, 0);
            }
        }

        let mut medium_ids: Vec<MediaId> = uses.iter().map(|u| u.0).collect();
        medium_ids.sort();
        medium_ids.dedup();

        let (mut count, mut before, mut after) = (0, 0, 0);
        let mut replacement: std::collections::HashMap<MediaId, (MediaId, bool)> = std::collections::HashMap::new();

        for media in medium_ids {
            let Some(m) = self.presentation().media(media).cloned() else { continue };
            let mine: Vec<_> = uses.iter().filter(|u| u.0 == media).collect();
            let take_crop = delete_crop && mine.len() == 1;
            let (w, h) = (mine.iter().map(|u| u.2).max().unwrap_or(1), mine.iter().map(|u| u.3).max().unwrap_or(1));
            let crop = if mine.len() == 1 { mine[0].4 } else { Crop::default() };
            let Some(c) = compress_picture(&m.data, m.format, w, h, crop, resolution.ppi(), take_crop) else { continue };
            before += m.data.len();
            after += c.data.len();
            count += 1;
            let data: std::sync::Arc<[u8]> = c.data.into();
            let new = self.document.ids().add_media(c.format, data.clone());
            if let Some(f) = match c.format {
                MediaFormat::Png => Some(ImageFormat::Png),
                MediaFormat::Jpeg => Some(ImageFormat::Jpeg),
                _ => None,
            } {
                self.resources.images.insert(new, std::sync::Arc::new(gpui_kit::Image::from_bytes(f, data.to_vec())));
            }
            replacement.insert(media, (new, take_crop));
        }

        let mut commands = Vec::new();
        for (media, pos, ..) in uses {
            let Some((new_media, took)) = replacement.get(&media).copied() else { continue };
            commands.push(FlowCommand::CompressPicture { at: pos, media: new_media, reset_crop: took });
        }

        if !commands.is_empty() {
            self.run_flow(commands, cx);
        }

        (count, before, after)
    }

    // ----- Word Options: 画像のサイズと品質 (Image Size and Quality) -----

    pub(crate) fn is_do_not_compress_images(&self) -> bool {
        self.flow().is_some_and(|f| f.do_not_compress_images)
    }

    pub(crate) fn set_do_not_compress_images(&mut self, val: bool, cx: &mut Context<Self>) {
        let (dpi, crop) = self.flow().map(|f| (f.default_image_dpi, f.discard_image_editing_data)).unwrap_or_default();
        self.run_flow(vec![FlowCommand::SetImageQualitySettings { default_dpi: dpi, discard_crop: crop, do_not_compress: val }], cx);
    }

    pub(crate) fn toggle_do_not_compress_images(&mut self, cx: &mut Context<Self>) {
        let cur = self.is_do_not_compress_images();
        self.set_do_not_compress_images(!cur, cx);
    }

    pub(crate) fn is_discard_image_editing_data(&self) -> bool {
        self.flow().is_some_and(|f| f.discard_image_editing_data)
    }

    pub(crate) fn set_discard_image_editing_data(&mut self, val: bool, cx: &mut Context<Self>) {
        let (dpi, comp) = self.flow().map(|f| (f.default_image_dpi, f.do_not_compress_images)).unwrap_or_default();
        self.run_flow(vec![FlowCommand::SetImageQualitySettings { default_dpi: dpi, discard_crop: val, do_not_compress: comp }], cx);
    }

    pub(crate) fn toggle_discard_image_editing_data(&mut self, cx: &mut Context<Self>) {
        let cur = self.is_discard_image_editing_data();
        self.set_discard_image_editing_data(!cur, cx);
    }

    pub(crate) fn default_image_dpi(&self) -> Option<u32> {
        self.flow().and_then(|f| f.default_image_dpi)
    }

    pub(crate) fn set_default_image_dpi(&mut self, dpi: Option<u32>, cx: &mut Context<Self>) {
        let (crop, comp) = self.flow().map(|f| (f.discard_image_editing_data, f.do_not_compress_images)).unwrap_or_default();
        self.run_flow(vec![FlowCommand::SetImageQualitySettings { default_dpi: dpi, discard_crop: crop, do_not_compress: comp }], cx);
    }
}
