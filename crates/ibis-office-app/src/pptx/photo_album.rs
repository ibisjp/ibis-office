// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use std::sync::Arc;
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{
    AutoShape, Color, Command, EmuRect, Fill, Geometry, LineCompound,
    MediaFormat, PhotoAlbum, PhotoAlbumFrame, PhotoAlbumLayout, Picture, Rgb, Shadow,
    Shape, ShapeKind, Slide, Stroke, TextAlign, TextBody, TextStyle,
};
use crate::shell::editor::SlideEditor;
use crate::shell::chrome::SidebarPanel;

/// 写真アルバムに追加する写真データ。
#[derive(Clone, Debug, PartialEq)]
pub struct PhotoAlbumPicture {
    pub caption: String,
    pub data: Arc<[u8]>,
    pub format: MediaFormat,
    pub width: u32,
    pub height: u32,
}

pub(crate) struct PhotoAlbumState {
    pub(crate) pictures: Vec<PhotoAlbumPicture>,
    pub(crate) selected_picture: Option<usize>,
    pub(crate) layout: PhotoAlbumLayout,
    pub(crate) frame: PhotoAlbumFrame,
    pub(crate) bw: bool,
    pub(crate) show_captions: bool,
    pub(crate) message: Option<SharedString>,
}

impl SlideEditor {
    #[allow(dead_code)]
    pub(crate) fn photo_album(&self) -> Option<&PhotoAlbum> {
        self.presentation().photo_album.as_ref()
    }

    pub(crate) fn set_photo_album(&mut self, album: Option<PhotoAlbum>, cx: &mut Context<Self>) {
        self.run(vec![Command::SetPhotoAlbum { album }], cx);
    }

    pub(crate) fn open_photo_album_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let existing = self.presentation().photo_album.clone().unwrap_or_default();
        if let Some(s) = self.pptx_mut() {
            s.photo_album = Some(PhotoAlbumState {
                pictures: Vec::new(),
                selected_picture: None,
                layout: existing.layout,
                frame: existing.frame,
                bw: existing.bw,
                show_captions: existing.show_captions,
                message: None,
            });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_photo_album_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.photo_album = None;
        }
        cx.notify();
    }

    /// 写真アルバムを作成してプレゼンテーションにスライド群を追加する。
    pub(crate) fn create_photo_album(
        &mut self,
        pictures: &[PhotoAlbumPicture],
        options: PhotoAlbum,
        cx: &mut Context<Self>,
    ) -> bool {
        if pictures.is_empty() {
            return false;
        }

        let slide_size = self.presentation().slide_size;
        let sw = slide_size.width;
        let sh = slide_size.height;
        let start_index = self.presentation().slides.len();
        let mut commands = Vec::new();

        // 1. アルバム表紙（タイトルスライド）の生成
        let title_slide_id = self.document.ids().new_slide_id();
        let mut title_slide = Slide::new(title_slide_id);

        let title_style = TextStyle {
            color: Rgb(0x1f1f1f),
            font_size: 4000,
            bold: true,
            align: TextAlign::Center,
            ..Default::default()
        };
        let title_shape_id = self.document.ids().new_shape_id();
        let title_frame = EmuRect::new(sw / 10, sh / 4, sw * 8 / 10, sh / 5);
        let mut title_auto = AutoShape::new(Geometry::Rect, Fill::None, None);
        title_auto.text = Some(TextBody::plain(&crate::shell::i18n::text("pptx.photo-album.title"), title_style));
        title_slide.shapes.push(Shape::new(title_shape_id, title_frame, ShapeKind::Auto(title_auto)));

        let subtitle_text = self.presentation().properties.creator.clone().unwrap_or_else(|| "ibisOffice".to_string());
        let subtitle_style = TextStyle {
            color: Rgb(0x5f6368),
            font_size: 2000,
            align: TextAlign::Center,
            ..Default::default()
        };
        let subtitle_shape_id = self.document.ids().new_shape_id();
        let subtitle_frame = EmuRect::new(sw / 10, sh / 2, sw * 8 / 10, sh / 8);
        let mut subtitle_auto = AutoShape::new(Geometry::Rect, Fill::None, None);
        subtitle_auto.text = Some(TextBody::plain(&subtitle_text, subtitle_style));
        title_slide.shapes.push(Shape::new(subtitle_shape_id, subtitle_frame, ShapeKind::Auto(subtitle_auto)));

        commands.push(Command::InsertSlide { index: start_index, slide: title_slide });

        // 2. 写真スライドの生成
        let per_slide = options.layout.pictures_per_slide();
        for (chunk_idx, chunk) in pictures.chunks(per_slide).enumerate() {
            let slide_id = self.document.ids().new_slide_id();
            let mut slide = Slide::new(slide_id);

            // タイトル付きレイアウトの場合、スライド見出しを追加
            if options.layout.has_title() {
                let heading_id = self.document.ids().new_shape_id();
                let heading_frame = EmuRect::new(sw / 10, sh / 18, sw * 8 / 10, 600_000);
                let heading_style = TextStyle {
                    color: Rgb(0x202124),
                    font_size: 2400,
                    bold: true,
                    align: TextAlign::Left,
                    ..Default::default()
                };
                let mut heading_auto = AutoShape::new(Geometry::Rect, Fill::None, None);
                heading_auto.text = Some(TextBody::plain(&crate::shell::i18n::text("pptx.photo-album.title"), heading_style));
                slide.shapes.push(Shape::new(heading_id, heading_frame, ShapeKind::Auto(heading_auto)));
            }

            // スロットの矩形領域を計算
            let slots = calculate_photo_slots(sw, sh, options.layout, options.show_captions);

            for (photo_idx, photo) in chunk.iter().enumerate() {
                let slot = slots.get(photo_idx).copied().unwrap_or(EmuRect::new(0, 0, sw, sh));

                // メディア登録
                let media = self.document.ids().add_media(photo.format, photo.data.clone());
                self.resources.add_picture(media, photo.format, &photo.data);

                // 写真アスペクト比フィッティング
                let (pic_frame, caption_frame) = if options.layout == PhotoAlbumLayout::FitToSlide {
                    (slot, None)
                } else {
                    let caption_h = if options.show_captions { 320_000 } else { 0 };
                    let max_pic_h = slot.height.saturating_sub(caption_h);
                    let aspect = photo.width as f64 / (photo.height as f64).max(1.0);
                    let (target_w, target_h) = if (slot.width as f64 / max_pic_h as f64) > aspect {
                        ((max_pic_h as f64 * aspect) as i64, max_pic_h)
                    } else {
                        (slot.width, (slot.width as f64 / aspect) as i64)
                    };
                    let px = slot.x + (slot.width - target_w) / 2;
                    let py = slot.y + (max_pic_h - target_h) / 2;
                    let pic_rect = EmuRect::new(px, py, target_w, target_h);

                    let cap_rect = if options.show_captions {
                        Some(EmuRect::new(px, pic_rect.bottom() + 40_000, target_w, caption_h.saturating_sub(40_000)))
                    } else {
                        None
                    };
                    (pic_rect, cap_rect)
                };

                // Picture 構造体生成
                let mut picture = Picture::new(media);
                if options.bw {
                    picture.effects.grayscale = true;
                }
                match options.frame {
                    PhotoAlbumFrame::FrameStyle1 => {
                        // 長方形（枠なし）
                    }
                    PhotoAlbumFrame::FrameStyle2 => {
                        // 角丸長方形
                        picture.corner = 16_667;
                        picture.geometry = ibis_pptx::preset_geometry("roundRect");
                    }
                    PhotoAlbumFrame::FrameStyle3 => {
                        // 単純な枠、白
                        picture.stroke = Some(Stroke::solid(Color::opaque(Rgb::WHITE), 25_400));
                    }
                    PhotoAlbumFrame::FrameStyle4 => {
                        // 単純な枠、黒
                        picture.stroke = Some(Stroke::solid(Color::opaque(Rgb::BLACK), 25_400));
                    }
                    PhotoAlbumFrame::FrameStyle5 => {
                        // 二重枠、黒
                        let mut s = Stroke::solid(Color::opaque(Rgb::BLACK), 38_100);
                        s.compound = LineCompound::Double;
                        picture.stroke = Some(s);
                    }
                    PhotoAlbumFrame::FrameStyle6 => {
                        // 中心の影
                        picture.shadow = Some(Shadow::simple(Color::opaque(Rgb(0x404040)), 50_800, 38_100, 5_400_000));
                    }
                    PhotoAlbumFrame::FrameStyle7 => {
                        // ソフト エッジ
                        picture.soft_edge = Some(63_500);
                    }
                }

                let pic_shape_id = self.document.ids().new_shape_id();
                slide.shapes.push(Shape::new(pic_shape_id, pic_frame, ShapeKind::Picture(picture)));

                // キャプションの追加
                if let Some(c_rect) = caption_frame {
                    let cap_shape_id = self.document.ids().new_shape_id();
                    let cap_style = TextStyle {
                        color: Rgb(0x3c4043),
                        font_size: 1200,
                        align: TextAlign::Center,
                        ..Default::default()
                    };
                    let mut cap_auto = AutoShape::new(Geometry::Rect, Fill::None, None);
                    cap_auto.text = Some(TextBody::plain(&photo.caption, cap_style));
                    slide.shapes.push(Shape::new(cap_shape_id, c_rect, ShapeKind::Auto(cap_auto)));
                }
            }

            commands.push(Command::InsertSlide { index: start_index + 1 + chunk_idx, slide });
        }

        // 3. フォトアルバムメタデータをプレゼンテーションに設定
        commands.push(Command::SetPhotoAlbum { album: Some(options) });

        // 一括実行（1ステップで Undo/Redo 可能）
        self.run(commands, cx);
        true
    }

    pub(crate) fn photo_album_move_picture(&mut self, index: usize, delta: isize, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut().and_then(|p| p.photo_album.as_mut()) {
            let len = s.pictures.len();
            if index < len {
                let target = index as isize + delta;
                if target >= 0 && (target as usize) < len {
                    let target_idx = target as usize;
                    s.pictures.swap(index, target_idx);
                    s.selected_picture = Some(target_idx);
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn photo_album_duplicate_picture(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut().and_then(|p| p.photo_album.as_mut()) {
            if let Some(pic) = s.pictures.get(index).cloned() {
                s.pictures.insert(index + 1, pic);
                s.selected_picture = Some(index + 1);
            }
        }
        cx.notify();
    }

    pub(crate) fn photo_album_remove_picture(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut().and_then(|p| p.photo_album.as_mut()) {
            if index < s.pictures.len() {
                s.pictures.remove(index);
                s.selected_picture = if s.pictures.is_empty() {
                    None
                } else {
                    Some(index.min(s.pictures.len() - 1))
                };
            }
        }
        cx.notify();
    }

    pub(crate) fn photo_album_apply_create(&mut self, cx: &mut Context<Self>) {
        let (pics, options) = if let Some(s) = self.pptx().and_then(|p| p.photo_album.as_ref()) {
            (
                s.pictures.clone(),
                PhotoAlbum {
                    bw: s.bw,
                    show_captions: s.show_captions,
                    layout: s.layout,
                    frame: s.frame,
                },
            )
        } else {
            return;
        };
        if self.create_photo_album(&pics, options, cx) {
            if let Some(s) = self.pptx_mut().and_then(|p| p.photo_album.as_mut()) {
                s.message = Some(crate::shell::i18n::text("pptx.photo-album.create-success"));
            }
            cx.notify();
        }
    }

    /// Context menu entries for photo album item `index` (PPTX-IMAGE-017).
    pub(crate) fn photo_album_item_context_menu_entries(&self, index: usize) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let Some(state) = self.pptx().and_then(|p| p.photo_album.as_ref()) else {
            return Vec::new();
        };
        if index >= state.pictures.len() {
            return Vec::new();
        }
        let len = state.pictures.len();
        let bw = state.bw;
        let show_captions = state.show_captions;

        vec![
            MenuItem::tr("pptx.photo-album.move-up")
                .icon("arrow_upward")
                .disabled(index == 0)
                .on(move |e, _, cx| e.photo_album_move_picture(index, -1, cx))
                .into(),
            MenuItem::tr("pptx.photo-album.move-down")
                .icon("arrow_downward")
                .disabled(index + 1 >= len)
                .on(move |e, _, cx| e.photo_album_move_picture(index, 1, cx))
                .into(),
            MenuItem::tr("edit.duplicate")
                .icon("control_point_duplicate")
                .on(move |e, _, cx| e.photo_album_duplicate_picture(index, cx))
                .into(),
            MenuItem::tr("pptx.photo-album.remove")
                .icon("delete")
                .on(move |e, _, cx| e.photo_album_remove_picture(index, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.photo-album.bw-on")
                .icon("contrast")
                .checked(bw)
                .on(|e, _, cx| {
                    if let Some(s) = e.pptx_mut().and_then(|p| p.photo_album.as_mut()) {
                        s.bw = !s.bw;
                    }
                    cx.notify();
                })
                .into(),
            MenuItem::tr("pptx.photo-album.captions-on")
                .icon("subtitles")
                .checked(show_captions)
                .on(|e, _, cx| {
                    if let Some(s) = e.pptx_mut().and_then(|p| p.photo_album.as_mut()) {
                        s.show_captions = !s.show_captions;
                    }
                    cx.notify();
                })
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.photo-album.create")
                .icon("photo_library")
                .on(|e, _, cx| e.photo_album_apply_create(cx))
                .into(),
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|e, _, cx| e.close_photo_album_panel(cx))
                .into(),
        ]
    }

    /// Open context menu for photo album item `index` at pointer `position`.
    pub(crate) fn open_photo_album_item_context_menu(
        &mut self,
        index: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(s) = self.pptx_mut().and_then(|p| p.photo_album.as_mut()) {
            if index < s.pictures.len() {
                s.selected_picture = Some(index);
            }
        }
        let entries = self.photo_album_item_context_menu_entries(index);
        if !entries.is_empty() {
            self.open_popup_entries(entries, position, window, cx);
        }
    }

    pub(crate) fn photo_album_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.pptx()?.photo_album.as_ref()?;
        let pic_count = state.pictures.len();
        let cur_layout = state.layout;
        let cur_frame = state.frame;
        let cur_bw = state.bw;
        let cur_captions = state.show_captions;
        let message = state.message.clone();

        let mut body = v_flex().gap_2().p_3().p_0();

        if let Some(msg) = message {
            body = body.child(
                div()
                    .p_2()
                    .rounded_md()
                    .bg(rgb(0xe6f4ea))
                    .text_sm()
                    .text_color(rgb(0x137333))
                    .child(msg),
            );
        }

        // 写真枚数
        body = body.child(
            h_flex().justify_between().child(
                div().text_sm().font_weight(FontWeight::BOLD).text_color(rgb(0x3c4043)).child(crate::shell::i18n::text("pptx.photo-album.pictures-in-album")),
            ).child(
                div().text_sm().text_color(rgb(0x70757a)).child(format!("{pic_count}")),
            ),
        );

        // 写真リスト
        if !state.pictures.is_empty() {
            let mut list = v_flex().id("pptx-photo-album-list").gap_1().max_h(px(120.)).overflow_y_scroll().border_1().border_color(rgb(0xdadce0)).rounded_md().p_1();
            for (idx, pic) in state.pictures.iter().enumerate() {
                let is_sel = state.selected_picture == Some(idx);
                let mut item_btn = crate::shell::chrome::action_button(ElementId::Name(format!("pptx-photo-item-{idx}").into()))
                    .small()
                    .label(format!("{}. {}", idx + 1, pic.caption))
                    .on_click(cx.listener(move |e, _, _, cx| {
                        if let Some(s) = e.pptx_mut().and_then(|p| p.photo_album.as_mut()) {
                            s.selected_picture = Some(idx);
                        }
                        cx.notify();
                    }));
                if !is_sel {
                    item_btn = item_btn.ghost();
                }
                let row = div()
                    .id(("pptx-photo-row", idx))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |e, event: &MouseDownEvent, w, cx| {
                            cx.stop_propagation();
                            e.open_photo_album_item_context_menu(idx, event.position, w, cx);
                        }),
                    )
                    .child(item_btn);
                list = list.child(row);
            }
            body = body.child(list);

            // 写真順序移動・削除ボタン
            body = body.child(
                h_flex()
                    .gap_1()
                    .child(
                        crate::shell::chrome::action_button("pptx-photo-move-up")
                            .small()

                            .label(crate::shell::i18n::text("pptx.photo-album.move-up"))
                            .on_click(cx.listener(|e, _, _, cx| {
                                if let Some(sel) = e.pptx().and_then(|p| p.photo_album.as_ref()).and_then(|s| s.selected_picture) {
                                    e.photo_album_move_picture(sel, -1, cx);
                                }
                            })),
                    )
                    .child(
                        crate::shell::chrome::action_button("pptx-photo-move-down")
                            .small()

                            .label(crate::shell::i18n::text("pptx.photo-album.move-down"))
                            .on_click(cx.listener(|e, _, _, cx| {
                                if let Some(sel) = e.pptx().and_then(|p| p.photo_album.as_ref()).and_then(|s| s.selected_picture) {
                                    e.photo_album_move_picture(sel, 1, cx);
                                }
                            })),
                    )
                    .child(
                        crate::shell::chrome::destructive_button("pptx-photo-remove",cx)
                            .small()
                            .label(crate::shell::i18n::text("pptx.photo-album.remove"))
                            .on_click(cx.listener(|e, _, _, cx| {
                                if let Some(sel) = e.pptx().and_then(|p| p.photo_album.as_ref()).and_then(|s| s.selected_picture) {
                                    e.photo_album_remove_picture(sel, cx);
                                }
                            })),
                    ),
            );
        }

        // オプション（白黒、キャプション）
        body = body.child(div().text_sm().font_weight(FontWeight::BOLD).text_color(rgb(0x3c4043)).child(crate::shell::i18n::text("pptx.photo-album.picture-options")));
        body = body
            .child(crate::shell::chrome::checkbox_button("pptx-photo-bw", crate::shell::i18n::text("pptx.photo-album.bw-off"), cur_bw, false, cx.listener(|e, _, _, cx| {
                if let Some(s) = e.pptx_mut().and_then(|p| p.photo_album.as_mut()) { s.bw = !s.bw; }
                cx.notify();
            })))
            .child(crate::shell::chrome::checkbox_button("pptx-photo-captions", crate::shell::i18n::text("pptx.photo-album.captions-off"), cur_captions, false, cx.listener(|e, _, _, cx| {
                if let Some(s) = e.pptx_mut().and_then(|p| p.photo_album.as_mut()) { s.show_captions = !s.show_captions; }
                cx.notify();
            })));

        // レイアウト選択
        body = body.child(div().text_sm().font_weight(FontWeight::BOLD).text_color(rgb(0x3c4043)).child(crate::shell::i18n::text("pptx.photo-album.album-layout")));
        let layouts = [
            (PhotoAlbumLayout::FitToSlide, "pptx.photo-album.layout.fit-to-slide"),
            (PhotoAlbumLayout::OnePic, "pptx.photo-album.layout.1pic"),
            (PhotoAlbumLayout::TwoPic, "pptx.photo-album.layout.2pic"),
            (PhotoAlbumLayout::FourPic, "pptx.photo-album.layout.4pic"),
            (PhotoAlbumLayout::OnePicTitle, "pptx.photo-album.layout.1pic-title"),
            (PhotoAlbumLayout::TwoPicTitle, "pptx.photo-album.layout.2pic-title"),
            (PhotoAlbumLayout::FourPicTitle, "pptx.photo-album.layout.4pic-title"),
        ];
        let label = layouts.iter().find(|(value, _)| *value == cur_layout).map(|(_, key)| crate::shell::i18n::text(key)).unwrap_or_default();
        let entries = layouts.into_iter().map(|(value, key)| crate::shell::chrome::MenuItem::new("pptx.photo-album.layout-choice", crate::shell::i18n::text(key)).checked(cur_layout == value).on(move |e, _, cx| {
            if let Some(s) = e.pptx_mut().and_then(|p| p.photo_album.as_mut()) { s.layout = value; }
            cx.notify();
        }).into()).collect();
        body = body.child(crate::shell::chrome::Dropdown::from_entries("pptx-album-layout",label,entries,true,0,&cx.weak_entity()).into_any_element());

        // 枠の形状選択
        body = body.child(div().text_sm().font_weight(FontWeight::BOLD).text_color(rgb(0x3c4043)).child(crate::shell::i18n::text("pptx.photo-album.frame-shape")));
        let frames = [
            (PhotoAlbumFrame::FrameStyle1, "pptx.photo-album.frame.rect"),
            (PhotoAlbumFrame::FrameStyle2, "pptx.photo-album.frame.round-rect"),
            (PhotoAlbumFrame::FrameStyle3, "pptx.photo-album.frame.simple-white"),
            (PhotoAlbumFrame::FrameStyle4, "pptx.photo-album.frame.simple-black"),
            (PhotoAlbumFrame::FrameStyle5, "pptx.photo-album.frame.compound-black"),
            (PhotoAlbumFrame::FrameStyle6, "pptx.photo-album.frame.center-shadow"),
            (PhotoAlbumFrame::FrameStyle7, "pptx.photo-album.frame.soft-edge"),
        ];
        let selected = frames.iter().position(|(value, _)| *value == cur_frame).unwrap_or(0);
        let choices = frames.into_iter().map(|(value, key)| crate::shell::chrome::preview_dropdown::PreviewChoice {
            label: crate::shell::i18n::text(key),
            content: std::rc::Rc::new(move || {
                let mut frame = div().w(px(68.)).h(px(28.)).flex().items_center().justify_center().bg(rgb(0x8ab4d8));
                frame = match value {
                    PhotoAlbumFrame::FrameStyle1 => frame.border_1().border_color(rgb(0x5f6368)),
                    PhotoAlbumFrame::FrameStyle2 => frame.rounded(px(8.)).border_1().border_color(rgb(0x5f6368)),
                    PhotoAlbumFrame::FrameStyle3 => frame.border_4().border_color(rgb(0xffffff)),
                    PhotoAlbumFrame::FrameStyle4 => frame.border_4().border_color(rgb(0x202124)),
                    PhotoAlbumFrame::FrameStyle5 => frame.border_4().border_color(rgb(0x202124)).child(div().absolute().inset(px(5.)).border_1().border_color(rgb(0xffffff))),
                    PhotoAlbumFrame::FrameStyle6 => frame.shadow_md(),
                    PhotoAlbumFrame::FrameStyle7 => frame.rounded(px(6.)).border_4().border_color(rgb(0xc9dce9)),
                };
                div().relative().h(px(36.)).w_full().flex().items_center().justify_center().child(frame.child(crate::shell::chrome::icon("landscape", 20., rgb(0xffffff).into()))).into_any_element()
            }),
            choose: std::rc::Rc::new(move |e, _, cx| {
                if let Some(s) = e.pptx_mut().and_then(|p| p.photo_album.as_mut()) { s.frame = value; }
                cx.notify();
            }),
        }).collect();
        body = body.child(crate::shell::chrome::preview_dropdown::preview_dropdown("pptx-album-frame", choices, selected, cx));

        // アクションボタン（作成・閉じる）
        let panel_actions = crate::shell::chrome::action_bar()
                .action(
                    crate::shell::chrome::ActionDefinition::new("pptx-photo-album-create", crate::shell::i18n::text("pptx.photo-album.create"), crate::shell::chrome::ActionButtonKind::Primary)
                        .on_click(cx.listener(|e, _, _, cx| {
                            let (pics, options) = if let Some(s) = e.pptx().and_then(|p| p.photo_album.as_ref()) {
                                (
                                    s.pictures.clone(),
                                    PhotoAlbum {
                                        bw: s.bw,
                                        show_captions: s.show_captions,
                                        layout: s.layout,
                                        frame: s.frame,
                                    },
                                )
                            } else {
                                return;
                            };
                            if e.create_photo_album(&pics, options, cx) {
                                if let Some(s) = e.pptx_mut().and_then(|p| p.photo_album.as_mut()) {
                                    s.message = Some(crate::shell::i18n::text("pptx.photo-album.create-success"));
                                }
                                cx.notify();
                            }
                        })),
                )

            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("pptx.photo-album.title"), body.into_any_element(), panel_actions))
    }
}

/// スライド寸法とレイアウトから各写真スロットの矩形領域を計算する。
fn calculate_photo_slots(
    sw: i64,
    sh: i64,
    layout: PhotoAlbumLayout,
    show_captions: bool,
) -> Vec<EmuRect> {
    match layout {
        PhotoAlbumLayout::FitToSlide => {
            vec![EmuRect::new(0, 0, sw, sh)]
        }
        PhotoAlbumLayout::OnePic => {
            let cap_margin = if show_captions { sh * 8 / 100 } else { 0 };
            vec![EmuRect::new(
                sw / 10,
                sh / 12,
                sw * 8 / 10,
                sh * 80 / 100 - cap_margin,
            )]
        }
        PhotoAlbumLayout::OnePicTitle => {
            let top = sh / 18 + 700_000;
            let cap_margin = if show_captions { 360_000 } else { 0 };
            let h = (sh - top - 600_000 - cap_margin).max(1_000_000);
            vec![EmuRect::new(sw / 10, top, sw * 8 / 10, h)]
        }
        PhotoAlbumLayout::TwoPic => {
            let top = sh / 12;
            let cap_margin = if show_captions { sh * 8 / 100 } else { 0 };
            let h = sh * 80 / 100 - cap_margin;
            let gap = sw * 4 / 100;
            let w = (sw * 88 / 100 - gap) / 2;
            vec![
                EmuRect::new(sw * 6 / 100, top, w, h),
                EmuRect::new(sw * 6 / 100 + w + gap, top, w, h),
            ]
        }
        PhotoAlbumLayout::TwoPicTitle => {
            let top = sh / 18 + 700_000;
            let cap_margin = if show_captions { 360_000 } else { 0 };
            let h = (sh - top - 600_000 - cap_margin).max(1_000_000);
            let gap = sw * 4 / 100;
            let w = (sw * 88 / 100 - gap) / 2;
            vec![
                EmuRect::new(sw * 6 / 100, top, w, h),
                EmuRect::new(sw * 6 / 100 + w + gap, top, w, h),
            ]
        }
        PhotoAlbumLayout::FourPic => {
            let top = sh / 15;
            let cap_margin = if show_captions { sh * 5 / 100 } else { 0 };
            let total_h = sh * 82 / 100 - cap_margin;
            let hgap = sw * 3 / 100;
            let vgap = sh * 3 / 100;
            let w = (sw * 88 / 100 - hgap) / 2;
            let h = (total_h - vgap) / 2;
            let left1 = sw * 6 / 100;
            let left2 = left1 + w + hgap;
            let top1 = top;
            let top2 = top1 + h + vgap;
            vec![
                EmuRect::new(left1, top1, w, h),
                EmuRect::new(left2, top1, w, h),
                EmuRect::new(left1, top2, w, h),
                EmuRect::new(left2, top2, w, h),
            ]
        }
        PhotoAlbumLayout::FourPicTitle => {
            let top = sh / 18 + 700_000;
            let cap_margin = if show_captions { 300_000 } else { 0 };
            let total_h = (sh - top - 500_000 - cap_margin).max(1_500_000);
            let hgap = sw * 3 / 100;
            let vgap = sh * 3 / 100;
            let w = (sw * 88 / 100 - hgap) / 2;
            let h = (total_h - vgap) / 2;
            let left1 = sw * 6 / 100;
            let left2 = left1 + w + hgap;
            let top1 = top;
            let top2 = top1 + h + vgap;
            vec![
                EmuRect::new(left1, top1, w, h),
                EmuRect::new(left2, top1, w, h),
                EmuRect::new(left1, top2, w, h),
                EmuRect::new(left2, top2, w, h),
            ]
        }
    }
}
