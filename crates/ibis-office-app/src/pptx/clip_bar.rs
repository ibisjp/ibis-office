// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 動画・音声のコントロールバー: the bar PowerPoint shows under a video or an
//! audio clip when the pointer is over it, in the editor too. Two rows (the
//! user's layout): the seek bar over the whole width; under it, from the
//! left, back to the start, one play/pause toggle, the volume icon (a click
//! mutes and unmutes), the volume slider (about 80px) and the time
//! `elapsed / length`. It is drawn in the editor's slide (so the zoom, the
//! page and the scroll carry it), in screen pixels (its size does not follow
//! the zoom, as PowerPoint's).
//!
//! What is decided here, and why (PowerPoint was not looked at for these;
//! to be checked on the real thing, see the report of the change):
//! - Under the clip's box, just outside it (rotation included), as wide as the
//!   box but never narrower than [`MIN_W`] so the controls fit: a speaker icon
//!   is small. Over the slide's bottom edge it goes on into the grey around the
//!   slide.
//! - Shown while the pointer is over the clip or over its bar, and while the
//!   clip plays, whatever the pointer does. A clip paused or stopped hides its
//!   bar once the pointer is gone.
//! - The seek bar covers the clip's trim (開始・終了のトリミング): its left end
//!   is the trim's start, its right end the trim's end (or the clip's end), and
//!   the time shown is counted from the trim's start, as the clip plays in the
//!   show. Back to the start goes to the trim's start.
//! - The volume of the bar is a playback aid of the editor, not the document:
//!   it multiplies the volume the clip is saved with (the ribbon's 音量, fades
//!   included), is kept per clip while the editor is open, and changes nothing
//!   that is saved or undone. The slider at 0 and the muted state look the same
//!   (the muted icon); the icon's click then unmutes to the level before the mute
//!   (or, if the slider was dragged to 0, the last level above 0, 100% at first).
//! - The press on the bar is the bar's: it does not select, move or resize
//!   shapes. A press on the seek bar or the slider goes on as a drag until the
//!   release.

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use ibis_doc_model::{EmuRect, ShapeId};

use super::{clips, interaction};
use crate::shell::editor::{SlideEditor, SlideLayout};

/// The bar's height, in screen pixels: the seek bar's row and the controls' row.
pub(crate) const BAR_H: f32 = 54.0;
/// The bar's least width.
pub(crate) const MIN_W: f32 = 250.0;
/// The space between the clip's box and the bar.
const GAP: f32 = 2.0;
const PAD: f32 = 8.0;
const BUTTON: f32 = 24.0;
const SLIDER_W: f32 = 80.0;
const THUMB: f32 = 10.0;
const TRACK_H: f32 = 4.0;
/// The seek bar's row: its middle, from the bar's top.
const SEEK_Y: f32 = 13.0;
/// The controls' row: its middle.
const ROW_Y: f32 = 36.0;

/// What a press on the bar is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum BarPart {
    Back,
    Play,
    Mute,
    /// The volume slider, at this fraction (0 to 1) of its length.
    Volume(f32),
    /// The seek bar, at this fraction (0 to 1) of its length.
    Seek(f32),
    /// The rest of the bar (the time, the margins): the bar's, and nothing happens.
    Inert,
}

/// Where the parts lie in a bar of width `w` (x from the bar's left; the rows' middles from its top).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geometry {
    pub(crate) seek: (f32, f32),
    pub(crate) seek_y: f32,
    pub(crate) row_y: f32,
    /// The left ends of the buttons (each `BUTTON` wide).
    pub(crate) back_x: f32,
    pub(crate) play_x: f32,
    pub(crate) mute_x: f32,
    pub(crate) volume: (f32, f32),
    pub(crate) time_x: f32,
}

pub(crate) fn geometry(w: f32) -> Geometry {
    let (back_x, play_x, mute_x) = (PAD, PAD + BUTTON + 2.0, PAD + 2.0 * (BUTTON + 2.0));
    let v0 = mute_x + BUTTON + 4.0 + THUMB / 2.0;
    Geometry {
        seek: (PAD + THUMB / 2.0, (w - PAD - THUMB / 2.0).max(PAD + THUMB)),
        seek_y: SEEK_Y,
        row_y: ROW_Y,
        back_x,
        play_x,
        mute_x,
        volume: (v0, v0 + SLIDER_W),
        time_x: v0 + SLIDER_W + THUMB / 2.0 + 12.0,
    }
}

/// The volume of a clip's bar (see the module's notes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BarVolume {
    pub(crate) level: f32,
    pub(crate) muted: bool,
    /// The last level above 0: where an unmute from 0 goes.
    last: f32,
}

impl Default for BarVolume {
    fn default() -> Self {
        Self { level: 1.0, muted: false, last: 1.0 }
    }
}

impl BarVolume {
    /// What multiplies the volume of the clip.
    pub(crate) fn factor(&self) -> f32 {
        if self.muted { 0.0 } else { self.level }
    }

    /// Shown as muted (the icon's picture).
    pub(crate) fn is_silent(&self) -> bool {
        self.factor() <= 0.0
    }

    pub(crate) fn set_level(&mut self, level: f32) {
        self.level = level.clamp(0.0, 1.0);
        self.muted = false;
        if self.level > 0.0 {
            self.last = self.level;
        }
    }

    /// The icon's click: mute, or back to the level before.
    pub(crate) fn toggle_mute(&mut self) {
        if self.is_silent() {
            self.muted = false;
            if self.level <= 0.0 {
                self.level = self.last;
            }
        } else {
            self.muted = true;
        }
    }
}

/// `m:ss` (`h:mm:ss` from an hour).
fn clock_text(ms: u32) -> String {
    let secs = ms / 1000;
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// What the bar shows of a clip: the time elapsed from the trim's start, the
/// length of the trim, and whether it plays.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct BarState {
    pub(crate) elapsed_ms: u32,
    pub(crate) length_ms: u32,
    pub(crate) playing: bool,
}

impl SlideEditor {
    /// The clips whose bars are shown now: the one the pointer is over and
    /// those that play, on the slide shown.
    pub(crate) fn clip_bars_shown(&self) -> Vec<ShapeId> {
        let Some(state) = self.pptx() else { return Vec::new() };
        if state.drag.is_some() || self.text_editing().is_some() {
            return Vec::new();
        }
        let slide = self.current_slide;
        let mut shown: Vec<ShapeId> = state.clips.iter().filter(|c| c.slide == slide && c.player.is_playing()).map(|c| c.shape).collect();
        if let Some(hover) = state.clip_hover.or(state.clip_seek.map(|(shape, _)| shape))
            && !shown.contains(&hover)
        {
            shown.push(hover);
        }
        shown.retain(|id| self.slide().shape(*id).is_some_and(|s| Self::clip_of(s).is_some()));
        shown
    }

    /// The time the bar of `shape`'s clip shows.
    pub(crate) fn clip_bar_state(&self, shape: ShapeId) -> BarState {
        let slide = self.current_slide;
        let Some(playing) = self.pptx().and_then(|s| s.clips.iter().find(|c| c.slide == slide && c.shape == shape)) else {
            return BarState::default();
        };
        let (start, end) = (playing.clip.options.trim_start_ms, playing.clip.options.trim_end_ms);
        let whole = playing.player.duration_ms();
        let end = if end > 0 && (whole == 0 || end < whole) { end } else { whole };
        let length_ms = end.saturating_sub(start);
        let elapsed_ms = playing.player.current_time_ms().saturating_sub(start).min(length_ms);
        BarState { elapsed_ms, length_ms, playing: playing.player.is_playing() }
    }

    /// The bar of `shape` in the window: under the box the shape takes.
    pub(crate) fn clip_bar_bounds(&self, shape: ShapeId, layout: SlideLayout) -> Option<Bounds<Pixels>> {
        let shape = self.slide().shape(shape)?;
        let EmuRect { x, y, width, height } = interaction::bounds(shape);
        let (left, bottom) = (f32::from(layout.to_px(x)), f32::from(layout.to_px(y + height)));
        let w = f32::from(layout.to_px(width)).max(MIN_W);
        Some(Bounds::new(point(layout.origin.x + px(left), layout.origin.y + px(bottom + GAP)), size(px(w), px(BAR_H))))
    }

    /// What a press at `position` is on a bar shown, if it is on one.
    pub(crate) fn clip_bar_part_at(&self, position: Point<Pixels>) -> Option<(ShapeId, BarPart)> {
        let layout = self.slide_layout()?;
        self.clip_bars_shown().into_iter().find_map(|shape| {
            let bounds = self.clip_bar_bounds(shape, layout)?;
            if !bounds.contains(&position) {
                return None;
            }
            let (x, y) = (f32::from(position.x - bounds.origin.x), f32::from(position.y - bounds.origin.y));
            let g = geometry(f32::from(bounds.size.width));
            let fraction = |(x0, x1): (f32, f32)| ((x - x0) / (x1 - x0)).clamp(0.0, 1.0);
            let in_button = |left: f32| x >= left && x <= left + BUTTON;
            let part = if y < (SEEK_Y + ROW_Y) / 2.0 - 2.0 {
                BarPart::Seek(fraction(g.seek))
            } else if in_button(g.back_x) {
                BarPart::Back
            } else if in_button(g.play_x) {
                BarPart::Play
            } else if in_button(g.mute_x) {
                BarPart::Mute
            } else if x >= g.volume.0 - THUMB && x <= g.volume.1 + THUMB {
                BarPart::Volume(fraction(g.volume))
            } else {
                BarPart::Inert
            };
            Some((shape, part))
        })
    }

    /// The pointer moved: the clip under it gets its bar (its player opened
    /// to know the length), and the bar goes when the pointer leaves both.
    pub(crate) fn update_clip_hover(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button.is_some() || self.panel_dialog_is_modal() {
            return;
        }
        let Some(layout) = self.slide_layout() else { return };
        let over_bar = self.clip_bar_part_at(event.position).map(|(shape, _)| shape);
        let next = over_bar.or_else(|| self.clip_at(layout.to_emu(event.position)));
        let Some(state) = self.pptx_mut() else { return };
        if state.clip_hover == next {
            return;
        }
        state.clip_hover = next;
        if let Some(shape) = next {
            self.prepare_clip(shape, false, cx);
        }
        cx.notify();
    }

    /// The pointer is not over any clip. Whether a bar was shown for it.
    pub(crate) fn clear_clip_hover(&mut self) -> bool {
        self.pptx_mut().is_some_and(|s| s.clip_seek.is_none() && s.clip_hover.take().is_some())
    }

    /// A press on a bar. Whether it was on a bar.
    pub(crate) fn clip_bar_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if event.button != MouseButton::Left {
            return false;
        }
        let Some((shape, part)) = self.clip_bar_part_at(event.position) else { return false };
        self.focus_handle.focus(window, cx);
        match part {
            BarPart::Play => self.toggle_clip(shape, cx),
            BarPart::Back => self.seek_clip_to(shape, 0.0, cx),
            BarPart::Mute => {
                if let Some(state) = self.pptx_mut() {
                    state.clip_volume.entry(shape).or_default().toggle_mute();
                }
                self.apply_clip_bar_volume(shape);
                cx.notify();
            }
            BarPart::Volume(fraction) => {
                if let Some(state) = self.pptx_mut() {
                    state.clip_seek = Some((shape, true));
                }
                self.set_clip_bar_level(shape, fraction, cx);
            }
            BarPart::Seek(fraction) => {
                if self.prepare_clip(shape, false, cx) {
                    if let Some(state) = self.pptx_mut() {
                        state.clip_seek = Some((shape, false));
                    }
                    self.seek_clip_to(shape, fraction, cx);
                }
            }
            BarPart::Inert => {}
        }
        true
    }

    /// The seek bar or the slider dragged: it goes where the pointer is. Whether a drag of the bar goes on.
    pub(crate) fn clip_seek_move(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some((shape, volume)) = self.pptx().and_then(|s| s.clip_seek) else { return false };
        let Some(bounds) = self.slide_layout().and_then(|l| self.clip_bar_bounds(shape, l)) else { return true };
        let g = geometry(f32::from(bounds.size.width));
        let (x0, x1) = if volume { g.volume } else { g.seek };
        let fraction = ((f32::from(position.x - bounds.origin.x) - x0) / (x1 - x0)).clamp(0.0, 1.0);
        if volume {
            self.set_clip_bar_level(shape, fraction, cx);
        } else {
            self.seek_clip_to(shape, fraction, cx);
        }
        true
    }

    /// The drag of the bar released.
    pub(crate) fn clip_seek_end(&mut self) -> bool {
        self.pptx_mut().and_then(|s| s.clip_seek.take()).is_some()
    }

    /// The clip of `shape` goes to `fraction` of its trim.
    pub(crate) fn seek_clip_to(&mut self, shape: ShapeId, fraction: f32, cx: &mut Context<Self>) {
        if !self.prepare_clip(shape, false, cx) {
            return;
        }
        let slide = self.current_slide;
        let state = self.clip_bar_state(shape);
        if let Some(playing) = self.pptx().and_then(|s| s.clips.iter().find(|c| c.slide == slide && c.shape == shape)) {
            let at = playing.clip.options.trim_start_ms + (state.length_ms as f32 * fraction.clamp(0.0, 1.0)) as u32;
            playing.player.seek_to_ms(at);
        }
        cx.notify();
    }

    /// The slider's level for the clip of `shape`.
    fn set_clip_bar_level(&mut self, shape: ShapeId, level: f32, cx: &mut Context<Self>) {
        if let Some(state) = self.pptx_mut() {
            state.clip_volume.entry(shape).or_default().set_level(level);
        }
        self.apply_clip_bar_volume(shape);
        cx.notify();
    }

    /// The volume of the bar of `shape` given to its player now (the ticks give it on: fades).
    fn apply_clip_bar_volume(&self, shape: ShapeId) {
        let slide = self.current_slide;
        let Some(state) = self.pptx() else { return };
        let factor = state.clip_volume.get(&shape).map_or(1.0, BarVolume::factor);
        if let Some(playing) = state.clips.iter().find(|c| c.slide == slide && c.shape == shape) {
            playing.player.set_volume(clips::volume_of(&playing.clip) * factor);
        }
    }

    /// The bar's volume of `shape`.
    pub(crate) fn clip_bar_volume(&self, shape: ShapeId) -> BarVolume {
        self.pptx().and_then(|s| s.clip_volume.get(&shape).copied()).unwrap_or_default()
    }

    /// The time label of the bar of `shape`: `elapsed / length`.
    pub(crate) fn clip_bar_time_text(&self, shape: ShapeId) -> String {
        let state = self.clip_bar_state(shape);
        crate::shell::i18n::format("pptx.clip.bar.time", &[("current", &clock_text(state.elapsed_ms)), ("total", &clock_text(state.length_ms))])
    }

    /// The bars shown, drawn in the slide.
    pub(crate) fn render_clip_bars(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(layout) = self.slide_layout() else { return Vec::new() };
        let theme = cx.theme();
        let (background, border, foreground, primary, muted) = (theme.background, theme.border, theme.foreground, theme.primary, theme.muted);
        self.clip_bars_shown()
            .into_iter()
            .filter_map(|shape| {
                let bounds = self.clip_bar_bounds(shape, layout)?;
                let state = self.clip_bar_state(shape);
                let volume = self.clip_bar_volume(shape);
                let g = geometry(f32::from(bounds.size.width));
                let fraction = if state.length_ms == 0 { 0.0 } else { state.elapsed_ms as f32 / state.length_ms as f32 };
                let (play_icon, play_tip) = if state.playing { ("pause_fill", "pptx.clip.bar.pause") } else { ("play_arrow_fill", "pptx.clip.bar.play") };
                let (mute_icon, mute_tip) = if volume.is_silent() { ("volume_off", "pptx.clip.bar.unmute") } else { ("volume_up", "pptx.clip.bar.mute") };
                let n = shape.0 as usize;
                let button = move |id: &'static str, left: f32, icon: &'static str, tip: &'static str| {
                    div()
                        .id((id, n))
                        .absolute()
                        .left(px(left - 1.0))
                        .top(px(ROW_Y - BUTTON / 2.0 - 1.0))
                        .size(px(BUTTON))
                        .rounded(px(3.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(move |s| s.bg(muted))
                        .tooltip(move |window, cx| Tooltip::new(crate::shell::i18n::text(tip)).build(window, cx))
                        .child(crate::shell::chrome::icon(icon, 18.0, foreground))
                };
                // A slider: a track, its filled part and a thumb, from `x0` to `x1`, its middle at `y`.
                let slider = move |id: &'static str, (x0, x1): (f32, f32), y: f32, fraction: f32, tip: &'static str| {
                    let fraction = fraction.clamp(0.0, 1.0);
                    div()
                        .id((id, n))
                        .absolute()
                        .left(px(x0 - 1.0))
                        .top(px(y - TRACK_H / 2.0 - 1.0))
                        .w(px(x1 - x0))
                        .h(px(TRACK_H))
                        .rounded(px(TRACK_H / 2.0))
                        .bg(border)
                        .tooltip(move |window, cx| Tooltip::new(crate::shell::i18n::text(tip)).build(window, cx))
                        .child(div().h_full().w(px((x1 - x0) * fraction)).rounded(px(TRACK_H / 2.0)).bg(primary))
                        .child(div().absolute().left(px((x1 - x0) * fraction - THUMB / 2.0)).top(px(-(THUMB - TRACK_H) / 2.0)).size(px(THUMB)).rounded_full().bg(primary))
                };
                // In the slide's pixels: the bar is a child of the slide.
                let (left, top) = (bounds.origin.x - layout.origin.x, bounds.origin.y - layout.origin.y);
                Some(
                    div()
                        .id(("clip-bar", n))
                        .absolute()
                        .left(left)
                        .top(top)
                        .w(bounds.size.width)
                        .h(bounds.size.height)
                        .rounded(px(4.))
                        .border_1()
                        .border_color(border)
                        .bg(background)
                        .text_color(foreground)
                        .shadow_sm()
                        .cursor_pointer()
                        .child(slider("clip-bar-seek", g.seek, g.seek_y, fraction, "pptx.clip.bar.seek"))
                        .child(button("clip-bar-back", g.back_x, "skip_previous_fill", "pptx.clip.bar.back"))
                        .child(button("clip-bar-play", g.play_x, play_icon, play_tip))
                        .child(button("clip-bar-mute", g.mute_x, mute_icon, mute_tip))
                        .child(slider("clip-bar-volume", g.volume, g.row_y, if volume.muted { 0.0 } else { volume.level }, "pptx.clip.bar.volume"))
                        .child(div().absolute().left(px(g.time_x - 1.0)).top(px(ROW_Y - 9.0 - 1.0)).h(px(18.0)).flex().items_center().text_xs().whitespace_nowrap().child(self.clip_bar_time_text(shape)))
                        .into_any_element(),
                )
            })
            .collect()
    }
}
