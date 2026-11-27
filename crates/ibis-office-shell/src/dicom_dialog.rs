// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 挿入 > 画像 > 医療画像（DICOM）… (`insert.medical-image`, moved from 校閲 on 2026-11-27 by the user, GitHub issue #246), on the document and presentation screens.
//!
//! The user picks a DICOM file; the dialog shows its picture on the left. Dragging over the picture adds a rectangle to black out, dragging
//! a rectangle moves it; the list on the right (with the number fields of the chosen one) adds, chooses, deletes. Under the list: the
//! dates to take out, and more tags to take out. When the file says patient information is burned into the pixels (`BurnedInAnnotation`),
//! a warning stands on top. 挿入 makes the picture again from the file with the rectangles painted into the pixels
//! (`ibis_formats::dicom::anonymize_and_extract`: the PNG holds only the blacked-out pixels, the original ones are in no file) and puts it
//! where the screen puts a picture (`FormatEditor::insert_made_picture`), as one step that one undo takes back.
//! Everything happens on this machine; nothing is sent.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use ibis_formats::dicom::{DicomAnonymizationProfile, RedactionRect};

use crate::chrome::{self, ActionButtonKind, ActionDefinition, ButtonGroupItem, Shown, ShownNumber, ShownText, SidebarPanel, SpinnerSpec, Typed};
use crate::editor::SlideEditor;
use crate::format::with_format;
use crate::i18n::{format, text};
use crate::palette::rgb;

/// The dialog's width (the picture and the list side by side).
pub const DIALOG_WIDTH: f32 = 880.;
/// The room the picture is fitted into.
const PREVIEW_W: f32 = 440.;
const PREVIEW_H: f32 = 380.;
/// The smallest rectangle a drag makes (smaller ones are a click, not a rectangle).
const MIN_DRAG: f32 = 3.;
const SELECTED_EDGE: u32 = 0xffc400;

/// What the pointer is doing over the picture.
#[derive(Clone, Copy)]
enum Drag {
    /// A new rectangle from `anchor` (image pixels).
    New { anchor: (f32, f32), index: usize },
    /// The rectangle `index` carried; `grab` is where inside it the pointer took hold.
    Move { grab: (f32, f32), index: usize },
}

/// The dialog's state.
pub struct DicomDialogState {
    /// The file's name, for the name of the picture made.
    pub name: String,
    data: Rc<Vec<u8>>,
    preview: Arc<Image>,
    /// The picture's size in pixels.
    pub size: (u32, u32),
    /// The file says patient information may be written in the pixels.
    pub burned_in: bool,
    /// The rectangles to black out, in image pixels.
    pub rects: Vec<RedactionRect>,
    pub selected: Option<usize>,
    pub strip_dates: bool,
    tags_input: Entity<InputState>,
    /// x, y, width, height of the chosen rectangle.
    fields: [Entity<InputState>; 4],
    drag: Option<Drag>,
    /// Where the picture is on the screen (written by the painting).
    bounds: Rc<Cell<Bounds<Pixels>>>,
    _subscriptions: Vec<Subscription>,
}

/// A rectangle's number in the four fields: whole pixels of the picture, shown as they are (nothing rounded for display).
/// The fields start empty (no rectangle chosen); what is typed is read back by `resolve`, not by a bare parse.
fn pixel_number() -> ShownNumber {
    ShownNumber::new(Shown::None, 1.0, SpinnerSpec::new(1.0).decimals(0))
}

fn fit(size: (u32, u32)) -> (f32, f32, f32) {
    let scale = (PREVIEW_W / size.0.max(1) as f32).min(PREVIEW_H / size.1.max(1) as f32);
    (size.0 as f32 * scale, size.1 as f32 * scale, scale)
}

impl SlideEditor {
    /// 校閲 > 医療画像（DICOM）の匿名化と挿入…: choose a DICOM file, then the dialog.
    pub fn insert_medical_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(feature = "scenarios")]
        if let Some((name, bytes)) = scenario_file::take() {
            self.open_dicom_dialog(&name, bytes, window, cx);
            return;
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(text("_shell.insert")) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let bytes = std::fs::read(&path);
                this.update_in(cx, |this, window, cx| match bytes {
                    Ok(bytes) => this.open_dicom_dialog(&name, bytes, window, cx),
                    Err(error) => this.show_alert(dialog_title(), format("dicom.insert.read-failed", &[("error", &error)]).into(), cx),
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        cx.spawn_in(window, async move |this, cx| {
            let picked = crate::web::pick_file().await;
            this.update_in(cx, |this, window, cx| match picked {
                Ok(Some((name, bytes))) => this.open_dicom_dialog(&name, bytes, window, cx),
                Ok(None) => {}
                Err(error) => this.show_alert(dialog_title(), format("dicom.insert.read-failed", &[("error", &error)]).into(), cx),
            })
            .ok();
        })
        .detach();
    }

    /// The dialog for the DICOM file `bytes` called `name`; a message box says so when it cannot give a picture.
    pub fn open_dicom_dialog(&mut self, name: &str, bytes: Vec<u8>, window: &mut Window, cx: &mut Context<Self>) {
        if !ibis_formats::dicom::is_dicom(&bytes) {
            self.show_alert(dialog_title(), text("dicom.insert.not-dicom").to_string().into(), cx);
            return;
        }
        // The picture as it is (no rectangles): what the left side shows. It is not kept as a file.
        let asset = match ibis_formats::dicom::anonymize_and_extract(&bytes, &DicomAnonymizationProfile::default()) {
            Ok(asset) => asset,
            Err(error) => {
                self.show_alert(dialog_title(), format("dicom.insert.read-failed", &[("error", &error.to_string())]).into(), cx);
                return;
            }
        };
        let Some(png) = asset.png_bytes.clone() else {
            let reason = asset.warnings.join("\n");
            self.show_alert(dialog_title(), format("dicom.insert.no-image", &[("reason", &reason)]).into(), cx);
            return;
        };
        let size = ibis_formats::dicom::png_size(&png).unwrap_or((1, 1));
        let preview = Arc::new(Image::from_bytes(ImageFormat::Png, png));
        self.end_text_edit(window, cx);
        if self.dialog.active {
            self.close_dialog(window, cx);
        }
        let tags_input = ShownText::new(Shown::None).input(window, cx);
        let fields: [Entity<InputState>; 4] = std::array::from_fn(|_| pixel_number().input(window, cx));
        let mut subscriptions = Vec::new();
        for (which, input) in fields.iter().enumerate() {
            subscriptions.push(cx.subscribe_in(input, window, move |this, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change | InputEvent::PressEnter { .. }) && input.read(cx).focus_handle(cx).is_focused(window) {
                    let typed = match pixel_number().resolve(input.read(cx).value().as_ref()) {
                        Typed::Value(value) => Some(value as i64),
                        _ => None,
                    };
                    this.dicom_set_field(which, typed, cx);
                }
            }));
        }
        self.dicom_dialog = Some(DicomDialogState {
            name: name.to_string(),
            data: Rc::new(bytes),
            preview,
            size,
            burned_in: asset.burned_in,
            rects: Vec::new(),
            selected: None,
            strip_dates: true,
            tags_input,
            fields,
            drag: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
            _subscriptions: subscriptions,
        });
        self.sidebar_open = true;
        cx.notify();
    }

    /// The chosen rectangle's numbers shown in the four fields.
    fn dicom_sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.dicom_dialog.as_ref() else { return };
        let rect = state.selected.and_then(|i| state.rects.get(i)).copied();
        let values = rect.map(|r| [r.x, r.y, r.width, r.height].map(|v| v.to_string())).unwrap_or_default();
        for (input, value) in state.fields.clone().iter().zip(values) {
            input.update(cx, |input, cx| {
                if input.value().as_ref() != value.as_str() {
                    input.set_value(value, window, cx);
                }
            });
        }
    }

    /// A number typed in one of the fields (0 x, 1 y, 2 width, 3 height) is put into the chosen rectangle, kept inside the picture.
    pub fn dicom_set_field(&mut self, which: usize, value: Option<i64>, cx: &mut Context<Self>) {
        let Some(state) = self.dicom_dialog.as_mut() else { return };
        let (w, h) = (state.size.0 as i64, state.size.1 as i64);
        let (Some(index), Some(value)) = (state.selected, value) else { return };
        let Some(rect) = state.rects.get_mut(index) else { return };
        match which {
            0 => rect.x = value.clamp(0, (w - 1).max(0)) as u32,
            1 => rect.y = value.clamp(0, (h - 1).max(0)) as u32,
            2 => rect.width = value.clamp(1, w.max(1)) as u32,
            _ => rect.height = value.clamp(1, h.max(1)) as u32,
        }
        rect.width = rect.width.min(state.size.0.saturating_sub(rect.x).max(1));
        rect.height = rect.height.min(state.size.1.saturating_sub(rect.y).max(1));
        cx.notify();
    }

    /// 範囲を足す: a rectangle across the picture's top (where an image usually carries its name line), chosen.
    pub fn dicom_add_rect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.dicom_dialog.as_mut() else { return };
        let (w, h) = state.size;
        state.rects.push(RedactionRect { x: 0, y: 0, width: w, height: (h / 10).max(1) });
        state.selected = Some(state.rects.len() - 1);
        self.dicom_sync_fields(window, cx);
        cx.notify();
    }

    /// 選んだ範囲を消す.
    pub fn dicom_delete_rect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.dicom_dialog.as_mut() else { return };
        if let Some(index) = state.selected.take().filter(|i| *i < state.rects.len()) {
            state.rects.remove(index);
        }
        self.dicom_sync_fields(window, cx);
        cx.notify();
    }

    pub fn dicom_select_rect(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.dicom_dialog.as_mut() {
            state.selected = (index < state.rects.len()).then_some(index);
        }
        self.dicom_sync_fields(window, cx);
        cx.notify();
    }

    /// The pointer position in the picture's pixels (kept inside it); `None` while the dialog's picture is not painted yet.
    fn dicom_image_point(&self, position: Point<Pixels>) -> Option<(f32, f32)> {
        let state = self.dicom_dialog.as_ref()?;
        let b = state.bounds.get();
        // The scale is read from where the picture really is (its painted size over its pixels), not from the size it was asked to be, so
        // that a window or UI scale that changes the laid-out size cannot shift the point.
        let (bw, bh) = (f32::from(b.size.width), f32::from(b.size.height));
        if bw < 1. || bh < 1. || state.size.0 == 0 || state.size.1 == 0 {
            return None;
        }
        let (sx, sy) = (bw / state.size.0 as f32, bh / state.size.1 as f32);
        let x = ((f32::from(position.x) - f32::from(b.origin.x)) / sx).clamp(0., state.size.0 as f32);
        let y = ((f32::from(position.y) - f32::from(b.origin.y)) / sy).clamp(0., state.size.1 as f32);
        Some((x, y))
    }

    /// Where the picture is on the screen, and the numbers in the four fields (x, y, width, height); for the scenarios.
    #[cfg(feature = "scenarios")]
    pub fn dicom_picture_bounds(&self) -> Option<Bounds<Pixels>> {
        self.dicom_dialog.as_ref().map(|s| s.bounds.get())
    }

    #[cfg(feature = "scenarios")]
    pub fn dicom_field_texts(&self, cx: &App) -> Vec<String> {
        self.dicom_dialog.as_ref().map(|s| s.fields.iter().map(|f| f.read(cx).value().to_string()).collect()).unwrap_or_default()
    }

    #[cfg(feature = "scenarios")]
    pub fn dicom_focus_field(&self, which: usize, window: &mut Window, cx: &mut App) {
        if let Some(state) = self.dicom_dialog.as_ref() {
            let handle = state.fields[which].read(cx).focus_handle(cx);
            handle.focus(window, cx);
        }
    }

    /// The pointer went down on the picture: on a rectangle it takes hold of it (and chooses it), elsewhere it starts a new one.
    pub fn dicom_press(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some((x, y)) = self.dicom_image_point(position) else { return };
        self.dicom_press_px(x, y, window, cx);
    }

    /// [`Self::dicom_press`] at a point given in the picture's pixels.
    pub fn dicom_press_px(&mut self, x: f32, y: f32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.dicom_dialog.as_mut() else { return };
        // The last one is on top.
        let hit = state.rects.iter().rposition(|r| x >= r.x as f32 && x < (r.x + r.width) as f32 && y >= r.y as f32 && y < (r.y + r.height) as f32);
        match hit {
            Some(index) => {
                let r = state.rects[index];
                state.selected = Some(index);
                state.drag = Some(Drag::Move { grab: (x - r.x as f32, y - r.y as f32), index });
            }
            None => {
                state.rects.push(RedactionRect { x: x as u32, y: y as u32, width: 0, height: 0 });
                let index = state.rects.len() - 1;
                state.selected = Some(index);
                state.drag = Some(Drag::New { anchor: (x, y), index });
            }
        }
        self.dicom_sync_fields(window, cx);
        cx.notify();
    }

    /// The pointer moved with the button down.
    pub fn dicom_drag(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some((x, y)) = self.dicom_image_point(position) else { return };
        self.dicom_drag_px(x, y, window, cx);
    }

    /// [`Self::dicom_drag`] at a point given in the picture's pixels.
    pub fn dicom_drag_px(&mut self, x: f32, y: f32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.dicom_dialog.as_mut() else { return };
        let (iw, ih) = (state.size.0 as f32, state.size.1 as f32);
        match state.drag {
            Some(Drag::New { anchor, index }) => {
                let (x0, x1) = (anchor.0.min(x), anchor.0.max(x));
                let (y0, y1) = (anchor.1.min(y), anchor.1.max(y));
                if let Some(r) = state.rects.get_mut(index) {
                    *r = RedactionRect { x: x0 as u32, y: y0 as u32, width: (x1 - x0).ceil() as u32, height: (y1 - y0).ceil() as u32 };
                }
            }
            Some(Drag::Move { grab, index }) => {
                if let Some(r) = state.rects.get_mut(index) {
                    let nx = (x - grab.0).clamp(0., (iw - r.width as f32).max(0.));
                    let ny = (y - grab.1).clamp(0., (ih - r.height as f32).max(0.));
                    r.x = nx.round() as u32;
                    r.y = ny.round() as u32;
                }
            }
            None => return,
        }
        self.dicom_sync_fields(window, cx);
        cx.notify();
    }

    /// The button went up: a rectangle that is only a click is dropped.
    pub fn dicom_release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.dicom_dialog.as_mut() else { return };
        let Some(drag) = state.drag.take() else { return };
        if let Drag::New { index, .. } = drag
            && state.rects.get(index).is_some_and(|r| (r.width as f32) < MIN_DRAG || (r.height as f32) < MIN_DRAG)
        {
            state.rects.remove(index);
            state.selected = None;
        }
        self.dicom_sync_fields(window, cx);
        cx.notify();
    }

    /// The tags typed in the field under the list, and the pieces that are not tags.
    fn dicom_tags(&self, cx: &App) -> (Vec<ibis_formats::dicom::Tag>, Vec<String>) {
        let Some(state) = self.dicom_dialog.as_ref() else { return Default::default() };
        ibis_formats::dicom::parse_tag_list(&state.tags_input.read(cx).value())
    }

    /// The picture the dialog would insert: the file anonymised with the dialog's choices, the rectangles painted into the pixels. The
    /// PNG is the only pixels there are (the original ones are in no file).
    pub fn dicom_made_picture(&self, cx: &App) -> Result<Vec<u8>, String> {
        let state = self.dicom_dialog.as_ref().ok_or_else(String::new)?;
        let (tags, _) = self.dicom_tags(cx);
        let profile = DicomAnonymizationProfile { extra_strip_tags: tags, strip_dates: state.strip_dates, roi_masks: state.rects.clone() };
        let asset = ibis_formats::dicom::anonymize_and_extract(&state.data, &profile).map_err(|e| e.to_string())?;
        asset.png_bytes.ok_or_else(|| asset.warnings.join("\n"))
    }

    /// 挿入: the picture with the rectangles blacked out goes into the document; one step to undo.
    pub fn apply_dicom_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let made = self.dicom_made_picture(cx);
        let Some(state) = self.dicom_dialog.as_ref() else { return };
        let (count, stem) = (state.rects.len(), state.name.rsplit_once('.').map_or(state.name.clone(), |(stem, _)| stem.to_string()));
        match made {
            Ok(png) => {
                self.close_dialog(window, cx);
                let name = format!("{}.png", if stem.is_empty() { "dicom" } else { &stem });
                with_format!(self, insert_made_picture(self, &name, png, cx));
                self.set_status(format("dicom.insert.inserted", &[("count", &count)]).to_string(), cx);
            }
            Err(error) => self.show_alert(dialog_title(), format("dicom.insert.no-image", &[("reason", &error)]).into(), cx),
        }
    }

    /// The dialog's panel.
    pub(crate) fn dicom_dialog_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let (_, bad_tags) = self.dicom_tags(cx);
        let state = self.dicom_dialog.as_ref()?;
        let (dw, dh, scale) = fit(state.size);
        let bounds = state.bounds.clone();
        let rects = state.rects.clone();
        let selected = state.selected;

        // The picture, with the rectangles over it.
        let mut picture = div()
            .id("dicom-picture")
            .relative()
            .flex_none()
            .w(px(dw))
            .h(px(dh))
            .bg(rgb(0x000000))
            .cursor_crosshair()
            .child(img(state.preview.clone()).absolute().inset_0().size_full().object_fit(ObjectFit::Fill))
            .child(canvas(|_, _, _| {}, move |b, _, _, _| bounds.set(b)).absolute().inset_0());
        for (i, r) in rects.iter().enumerate() {
            let chosen = selected == Some(i);
            picture = picture.child(
                div()
                    .absolute()
                    .left(px(r.x as f32 * scale))
                    .top(px(r.y as f32 * scale))
                    .w(px((r.width as f32 * scale).max(1.)))
                    .h(px((r.height as f32 * scale).max(1.)))
                    .bg(rgb(0x000000))
                    .border_1()
                    .border_color(rgb(if chosen { SELECTED_EDGE } else { 0xffffff })),
            );
        }
        let picture = picture
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, w, cx| this.dicom_press(e.position, w, cx)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, w, cx| {
                if e.dragging() {
                    this.dicom_drag(e.position, w, cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, w, cx| this.dicom_release(w, cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, w, cx| this.dicom_release(w, cx)));

        let left = v_flex()
            .gap_2()
            .flex_none()
            .w(px(PREVIEW_W))
            .child(div().w(px(PREVIEW_W)).h(px(PREVIEW_H)).flex().items_center().justify_center().child(picture))
            .child(chrome::panel_note(format("dicom.insert.size", &[("width", &state.size.0), ("height", &state.size.1)]).to_string()));

        // The list of the rectangles and the numbers of the chosen one.
        let mut list = v_flex().gap_1().w_full();
        if rects.is_empty() {
            list = list.child(chrome::panel_note(text("dicom.insert.none").to_string()));
        }
        for (i, r) in rects.iter().enumerate() {
            let label = format("dicom.insert.range-label", &[("no", &(i + 1)), ("x", &r.x), ("y", &r.y), ("width", &r.width), ("height", &r.height)]).to_string();
            list = list.child(
                div()
                    .id(SharedString::from(format!("dicom-range-{i}")))
                    .w_full()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .text_sm()
                    .bg(rgb(if selected == Some(i) { chrome::SELECTED_ROW_BG } else { 0xffffff }))
                    .child(label)
                    .on_click(cx.listener(move |this, _, w, cx| this.dicom_select_rect(i, w, cx))),
            );
        }
        let field = |which: usize, label: &'static str| {
            chrome::text_field_row(text(label), chrome::transient_text_field(&state.fields[which]).width(px(90.)).unit("px", px(20.)).disabled(selected.is_none()))
        };
        let buttons = chrome::button_group([
            ButtonGroupItem::new("dicom-add", text("dicom.insert.add")).on_click(cx.listener(|this, _, w, cx| this.dicom_add_rect(w, cx))),
            ButtonGroupItem::new("dicom-delete", text("dicom.insert.delete")).disabled(selected.is_none()).on_click(cx.listener(|this, _, w, cx| this.dicom_delete_rect(w, cx))),
        ])
        .compact();
        let strip_dates = state.strip_dates;
        let right = v_flex()
            .gap_2()
            .flex_1()
            .min_w_0()
            .child(chrome::section_heading(text("dicom.insert.ranges")))
            .child(chrome::panel_note(text("dicom.insert.hint").to_string()))
            .child(div().id("dicom-ranges").w_full().max_h(px(120.)).overflow_y_scroll().child(list))
            .child(buttons)
            .child(field(0, "dicom.insert.x"))
            .child(field(1, "dicom.insert.y"))
            .child(field(2, "dicom.insert.width"))
            .child(field(3, "dicom.insert.height"))
            .child(chrome::checkbox_button("dicom-strip-dates", text("dicom.insert.strip-dates"), strip_dates, false, cx.listener(|this, _, _, cx| {
                if let Some(state) = this.dicom_dialog.as_mut() {
                    state.strip_dates = !state.strip_dates;
                }
                cx.notify();
            })))
            .child(chrome::text_field_row(text("dicom.insert.extra-tags"), chrome::transient_text_field(&state.tags_input).full_width()))
            .child(chrome::panel_note(text("dicom.insert.extra-tags-hint").to_string()))
            .children((!bad_tags.is_empty()).then(|| chrome::panel_note(format("dicom.insert.bad-tags", &[("tags", &bad_tags.join(" "))]).to_string())))
            .child(chrome::panel_note(text("dicom.insert.note").to_string()));

        let mut body = chrome::panel_form();
        if state.burned_in {
            body = body.child(
                div()
                    .id("dicom-burned-in")
                    .w_full()
                    .p_2()
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(SELECTED_EDGE))
                    .bg(rgb(0xfff4d6))
                    .text_sm()
                    .text_color(rgb(0x5c4300))
                    .child(text("dicom.insert.burned-in").to_string()),
            );
        }
        let body = body.child(h_flex().gap_4().items_start().w_full().child(left).child(right));
        let actions = chrome::action_bar()
            .action(ActionDefinition::new("dicom-insert", text("dicom.insert.apply"), ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_dicom_dialog(w, cx))))
            .action(chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(dialog_title(), body.into_any_element(), actions))
    }
}

/// The menu line, the same on the document and presentation screens.
pub fn insert_medical_image_item() -> chrome::MenuItem {
    chrome::MenuItem::tr("insert.medical-image").icon("image").on(|this, window, cx| this.insert_medical_image(window, cx))
}

fn dialog_title() -> SharedString {
    text("dicom.insert.title").into()
}

/// For the scenarios: the file the picker would have given.
#[cfg(feature = "scenarios")]
pub mod scenario_file {
    use std::cell::RefCell;
    thread_local! { static FILE: RefCell<Option<(String, Vec<u8>)>> = const { RefCell::new(None) }; }
    /// The next 校閲 > 医療画像（DICOM）の匿名化と挿入… takes this file instead of asking for one.
    pub fn set(name: &str, bytes: Vec<u8>) {
        FILE.with(|f| *f.borrow_mut() = Some((name.to_string(), bytes)));
    }
    pub(super) fn take() -> Option<(String, Vec<u8>)> {
        FILE.with(|f| f.borrow_mut().take())
    }
}
