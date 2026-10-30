// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The slide show: the presentation full screen, a slide at a time, fitted
//! on black, from the slide shown, as PowerPoint's 現在のスライドから.
//!
//! A click, →, ↓, Space, Enter, N, PageDown or the wheel down shows the next
//! slide; ←, ↑, Backspace, P, PageUp or the wheel up the previous one; Home and End the first and
//! last; B or . blacks the screen out and W or , whites it (the keys are
//! bound in [`crate::init`], and Esc to `CancelTool`, which ends the show).
//! After the last slide comes a black screen saying so, as in PowerPoint;
//! going on from there ends the show.
//!
//! A slide whose transition says to move on after a time (`advTm`, 自動的に
//! 切り替え) is followed by the next one after its transition and that
//! time; one that says not to move on with a click (`advClick="0"`) is not
//! moved on from by a click, only by the keys or its time.
//!
//! A slide's animations (アニメーション) play as PowerPoint plays them: the
//! effects before the first click-started one when the slide comes in, then
//! a group of effects at each click (or key) before the next slide; going
//! back takes back the last group. Entrance effects bring their shape in
//! (appearing, fading in, or flying in from a side), exit effects take it
//! out, motion paths move it; emphasis effects are played as nothing yet
//! (GPUI elements cannot be turned or scaled). See [`ShapeMotion`].
//!
//! The presenter's view (発表者ビュー, ⌥⌘↩) shows, instead of the slide
//! alone: the slide shown, the next one, the slide's speaker notes, the
//! time since the show started and the slide's number, in one window. With
//! two displays or more, the presenter's view is a full-screen window of the
//! display the editor is on and the audience's slide show a full-screen
//! window of another display (as PowerPoint), both following the one show
//! (slide, animations, clips); the button 画面の入れ替え exchanges them.
//! The editor's window stays as it was.
//!
//! A slide with a transition (画面切り替え) comes in over the one before it:
//! fading in, or sliding in from its side for push, cover and wipe (the
//! other effects fade); going back shows the slide at once.

use std::collections::HashMap;

use gpui_kit::component::Icon;
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;

use ibis_doc_model::{LineCap, LineJoin, ShapeKind};
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;
use crate::render::shape_view::{polygon, polygon_styled, shape_element, slide_backdrop};

/// Key context of the slide show; see the bindings in [`crate::init`].
pub(crate) const SLIDESHOW_CONTEXT: &str = "Slideshow";

/// The text on the black screen after the last slide, and its colour.
const END_TEXT: &str = "pptx.slideshow.end-slide-show-click-exit"; // the key of its translation
const END_TEXT_COLOR: u32 = 0xffffff;

/// The pointer mode during the slide show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum PointerMode {
    /// Normal arrow pointer: click advances slides or activates links.
    #[default]
    Arrow,
    /// Laser pointer: moves a bright red dot, clicks do not advance.
    Laser,
    /// Pen drawing mode: drag draws freehand ink strokes.
    Pen,
    /// Highlighter drawing mode: drag draws translucent wide highlight strokes.
    Highlighter,
    /// Eraser mode: click removes nearby ink strokes.
    Eraser,
}

/// A point normalized to slide dimensions (0.0 to 1.0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NormalizedPoint {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

/// A freehand ink stroke drawn during the slide show.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InkStroke {
    pub(crate) points: Vec<NormalizedPoint>,
    pub(crate) color: u32,
    pub(crate) width: f32,
    pub(crate) opacity: f32,
}

/// Active zoom return target during the slide show (PPTX-TRANS-016).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ZoomReturnState {
    /// The slide containing the zoom object, to which we return.
    pub(crate) return_slide_id: ibis_doc_model::SlideId,
    /// The slide whose completion triggers the return.
    pub(crate) trigger_slide_id: ibis_doc_model::SlideId,
}

/// A slide show in progress.
pub(crate) struct Slideshow {
    /// The black screen after the last slide is shown.
    at_end: bool,
    /// The window was made full screen for the show, and goes back after it.
    made_fullscreen: bool,
    /// The screen blacked or whited out, in that colour, instead of the slide.
    blank: Option<u32>,
    /// The slide the shown one is coming in over, and a number for each
    /// change (each runs its own animation).
    came_from: Option<ibis_doc_model::SlideId>,
    /// 変形: when the slide that came in began to (by `changes`), for its progress.
    morph_clock: std::cell::RefCell<Option<(usize, crate::clock::Instant)>>,
    changes: usize,
    /// Counts each slide shown, so that a timer set for one (`advTm`) does
    /// nothing once another is shown.
    shown: usize,
    /// The presenter's view, and when the show started (its clock).
    pub(crate) presenter: bool,
    /// The presenter's view's notes: the text size (pt), the teleprompter (自動スクロール: the speed in pt per second
    /// while it runs), the scroll the notes are at and the slide they were scrolled for.
    pub(crate) notes_size: f32,
    pub(crate) teleprompter: Option<f32>,
    notes_scroll: ScrollHandle,
    notes_slide: std::cell::Cell<Option<ibis_doc_model::SlideId>>,
    started: crate::clock::Instant,
    paused_elapsed: Option<std::time::Duration>,
    accumulated: std::time::Duration,
    /// How many of the slide's animation groups have played
    /// ([`animation_groups`]), and a number for each one played (each runs
    /// its own animation).
    pub(crate) step: usize,
    played: usize,
    /// With two displays or more: the windows of the presenter's view and the
    /// audience's slide show, and which display each is on.
    external: Option<ShowWindows>,
    /// Active custom show identifier, if running a specific custom show.
    pub(crate) custom_show: Option<u32>,
    /// Pointer mode for the presentation (Arrow, Laser, Pen, Highlighter, Eraser).
    pub(crate) pointer_mode: PointerMode,
    /// Current laser pointer position in slide normalized coordinates.
    pub(crate) laser_pos: Option<NormalizedPoint>,
    /// Ink strokes drawn on each slide (kept during the show).
    pub(crate) slide_inks: HashMap<ibis_doc_model::SlideId, Vec<InkStroke>>,
    /// Active stroke currently being drawn with pen or highlighter.
    pub(crate) active_stroke: Option<InkStroke>,
    /// Whether the "See All Slides" grid overview is open.
    pub(crate) grid_open: bool,
    /// Zoom scale factor during the show (1.0 = normal).
    pub(crate) zoom: f32,
    /// Pan offset in pixels when zoomed in (dx, dy).
    pub(crate) pan: (f32, f32),
    /// Start position of drag gesture for panning when zoomed in.
    pub(crate) drag_start: Option<Point<Pixels>>,
    /// Pan offset when the drag gesture started.
    pub(crate) pan_start: (f32, f32),
    /// Rehearsal state if the show was started in rehearsal mode.
    pub(crate) rehearsal: Option<Rehearsal>,
    /// Whether the show is running in Reading View (閲覧表示) mode.
    pub(crate) reading_view: bool,
    /// Whether the confirmation dialog to keep ink annotations is open.
    pub(crate) ink_dialog_open: bool,
    /// Active return-to-zoom target during the show (PPTX-TRANS-016).
    pub(crate) zoom_return: Option<ZoomReturnState>,
    /// Active / triggered animation indices played on the current slide.
    pub(crate) triggered_played: Vec<usize>,
}

impl Slideshow {
    /// Whether any ink stroke has been drawn on any slide during this show.
    pub(crate) fn has_inks(&self) -> bool {
        self.slide_inks.values().any(|v| !v.is_empty())
    }
}

/// State of a slide show rehearsal (timing measurement).
#[derive(Clone, Debug)]
pub(crate) struct Rehearsal {
    /// When the current slide started showing.
    pub(crate) slide_started: crate::clock::Instant,
    /// Accumulated duration for the current slide across pauses.
    pub(crate) slide_accumulated: std::time::Duration,
    /// If paused, the accumulated duration at pause.
    pub(crate) slide_paused: Option<std::time::Duration>,
    /// When the whole rehearsal started.
    pub(crate) total_started: crate::clock::Instant,
    /// Accumulated duration for the whole rehearsal across pauses.
    pub(crate) total_accumulated: std::time::Duration,
    /// If total is paused.
    pub(crate) total_paused: Option<std::time::Duration>,
    /// Recorded duration for each slide in milliseconds.
    pub(crate) recorded_timings: HashMap<ibis_doc_model::SlideId, u32>,
    /// Whether the completion confirmation dialog is open.
    pub(crate) dialog_open: bool,
    /// The final total rehearsal duration when completed.
    pub(crate) final_total: Option<std::time::Duration>,
}

impl Rehearsal {
    pub(crate) fn slide_elapsed(&self) -> std::time::Duration {
        if let Some(paused) = self.slide_paused {
            paused
        } else {
            self.slide_accumulated + self.slide_started.elapsed()
        }
    }

    pub(crate) fn total_elapsed(&self) -> std::time::Duration {
        if let Some(final_time) = self.final_total {
            final_time
        } else if let Some(paused) = self.total_paused {
            paused
        } else {
            self.total_accumulated + self.total_started.elapsed()
        }
    }
}

/// The windows of a slide show on two displays.
struct ShowWindows {
    /// The editor's display, and the presenter's view's and audience's
    /// windows (closed when the show ends).
    editor_display: DisplayId,
    swapped: bool,
    windows: Vec<AnyWindowHandle>,
}

/// The 0-based slide index range permitted by the presentation's show properties.
pub(crate) fn presentation_slide_bounds(pres: &ibis_doc_model::Presentation) -> (usize, usize) {
    let total = pres.slides.len();
    if total == 0 {
        return (0, 0);
    }
    match pres.show_properties.slide_range {
        ibis_doc_model::ShowSlideRange::All | ibis_doc_model::ShowSlideRange::Custom(_) => (0, total - 1),
        ibis_doc_model::ShowSlideRange::Range { start, end } => {
            let s_idx = start.saturating_sub(1).min(total - 1);
            let e_idx = end.saturating_sub(1).min(total - 1);
            (s_idx.min(e_idx), s_idx.max(e_idx))
        }
    }
}

/// The ordered list of slide IDs to be presented, based on show properties and active custom show.
pub(crate) fn presentation_show_slides(pres: &ibis_doc_model::Presentation, custom_show: Option<u32>) -> Vec<ibis_doc_model::SlideId> {
    let custom_id = custom_show.or_else(|| match pres.show_properties.slide_range {
        ibis_doc_model::ShowSlideRange::Custom(id) => Some(id),
        _ => None,
    });
    if let Some(id) = custom_id
        && let Some(show) = pres.custom_show(id)
    {
        return show.slides.iter().copied().filter(|&sid| pres.slide(sid).is_some_and(|s| !s.hidden)).collect();
    }
    let (min_idx, max_idx) = presentation_slide_bounds(pres);
    if pres.slides.is_empty() {
        return Vec::new();
    }
    pres.slides[min_idx..=max_idx].iter().filter(|s| !s.hidden).map(|s| s.id).collect()
}

/// Which window a slide show is drawn for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShowKind {
    /// The editor's window, taken over by the show.
    Main,
    /// The presenter's view in a window of its own.
    Presenter,
    /// The audience's slide show in a window of its own.
    Audience,
}

impl SlideEditor {
    /// Start the slide show from the slide shown (a presentation only).
    pub(crate) fn start_slideshow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_show(true, window, cx);
    }

    /// Start a custom slide show by its numeric identifier.
    pub(crate) fn start_custom_slideshow(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = self.presentation().custom_show(id).and_then(|cs| cs.slides.first()).copied() {
            self.current_slide = target;
        }
        self.start_show_with_custom(true, Some(id), window, cx);
    }

    /// Whether the editor's window shows the slide show (not windows of its own).
    pub(crate) fn show_in_main(&self) -> bool {
        self.slideshow.as_ref().is_some_and(|s| s.external.is_none())
    }

    /// Whether the presentation is in Reading View (閲覧表示).
    pub(crate) fn is_reading_view(&self) -> bool {
        self.slideshow.as_ref().is_some_and(|s| s.reading_view)
    }

    /// Whether the presentation is in Outline View (アウトライン表示).
    pub(crate) fn is_outline_view(&self) -> bool {
        self.pptx().is_some_and(|s| s.outline_view && !s.grid_view && !s.notes_page_view) && self.slideshow.is_none()
    }

    /// Whether the presentation is in Notes Page View (ノート表示).
    pub(crate) fn is_notes_page_view(&self) -> bool {
        self.pptx().is_some_and(|s| s.notes_page_view && !s.grid_view) && self.slideshow.is_none()
    }

    /// Whether the presentation is in Normal View (標準表示).
    pub(crate) fn is_normal_view(&self) -> bool {
        !self.is_grid_view() && !self.is_outline_view() && !self.is_notes_page_view() && self.slideshow.is_none()
    }

    /// Toggle Outline View (アウトライン表示).
    pub(crate) fn toggle_outline_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_outline_view() {
            self.switch_to_normal_view(window, cx);
        } else {
            self.switch_to_outline_view(window, cx);
        }
    }

    /// Switch to Outline View (アウトライン表示).
    pub(crate) fn switch_to_outline_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_grid_view() {
            self.toggle_grid_view(window, cx);
        }
        if self.slideshow.is_some() {
            self.end_slideshow(window, cx);
        }
        if let Some(s) = self.pptx_mut() {
            s.outline_view = true;
            s.notes_page_view = false;
        }
        cx.notify();
    }

    /// Toggle Notes Page View (ノート表示).
    pub(crate) fn toggle_notes_page_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_notes_page_view() {
            self.switch_to_normal_view(window, cx);
        } else {
            self.switch_to_notes_page_view(window, cx);
        }
    }

    /// Switch to Notes Page View (ノート表示).
    pub(crate) fn switch_to_notes_page_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_grid_view() {
            self.toggle_grid_view(window, cx);
        }
        if self.slideshow.is_some() {
            self.end_slideshow(window, cx);
        }
        if let Some(s) = self.pptx_mut() {
            s.outline_view = false;
            s.notes_page_view = true;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Start Reading View (閲覧表示) within the main window without fullscreen.
    pub(crate) fn start_reading_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_reading_view() {
            return;
        }
        if self.is_grid_view() {
            self.toggle_grid_view(window, cx);
        }
        if let Some(s) = self.pptx_mut() {
            s.outline_view = false;
            s.notes_page_view = false;
        }
        if self.slideshow.is_some() {
            self.end_slideshow(window, cx);
        }
        self.start_show_mode(false, None, true, window, cx);
    }

    /// Switch back to Normal View (標準表示).
    pub(crate) fn switch_to_normal_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_master_view() {
            self.close_master_view(window, cx);
        }
        if self.is_page_view() {
            self.close_notes_format(window, cx);
        }
        if self.is_grid_view() {
            self.toggle_grid_view(window, cx);
        }
        if self.slideshow.is_some() {
            self.end_slideshow(window, cx);
        }
        if let Some(s) = self.pptx_mut() {
            s.outline_view = false;
            s.notes_page_view = false;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn start_show(&mut self, fullscreen_main: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.start_show_mode(fullscreen_main, None, false, window, cx);
    }

    fn start_show_with_custom(&mut self, fullscreen_main: bool, custom_show: Option<u32>, window: &mut Window, cx: &mut Context<Self>) {
        self.start_show_mode(fullscreen_main, custom_show, false, window, cx);
    }

    fn start_show_mode(&mut self, fullscreen_main: bool, custom_show: Option<u32>, reading_view: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_some() || !self.has_document || self.loading || !self.pages_are_slides() {
            return;
        }
        self.commit_edit(window, cx);
        // The editor's clips (opened for their control bars) do not play in the show.
        self.stop_clips(cx);
        // Nothing is edited during the show (Delete would remove the selection).
        self.drag = None;
        self.selection = None;
        self.tool = crate::shell::editor::Tool::Select;
        let show_props = &self.presentation().show_properties;
        let fullscreen_main = !reading_view && fullscreen_main && !matches!(show_props.show_type, ibis_doc_model::ShowType::Browse);
        let made_fullscreen = fullscreen_main && !window.is_fullscreen();
        if made_fullscreen {
            window.toggle_fullscreen();
        }
        // If slide range or custom show is specified, jump into the sequence if current slide is outside.
        let show_slides = presentation_show_slides(self.presentation(), custom_show);
        if !show_slides.is_empty() && !show_slides.contains(&self.current_slide) {
            if let Some(&target) = show_slides.first() {
                self.current_slide = target;
            }
        }
        self.slideshow = Some(Slideshow {
            at_end: false,
            made_fullscreen,
            blank: None,
            came_from: None,
            morph_clock: std::cell::RefCell::new(None),
            changes: 0,
            shown: 0,
            presenter: false,
            notes_size: 20.0,
            teleprompter: None,
            notes_scroll: ScrollHandle::new(),
            notes_slide: std::cell::Cell::new(None),
            started: crate::clock::Instant::now(),
            paused_elapsed: None,
            accumulated: std::time::Duration::ZERO,
            step: 0,
            played: 0,
            external: None,
            custom_show,
            pointer_mode: PointerMode::Arrow,
            laser_pos: None,
            slide_inks: HashMap::new(),
            active_stroke: None,
            grid_open: false,
            zoom: 1.0,
            pan: (0.0, 0.0),
            drag_start: None,
            pan_start: (0.0, 0.0),
            rehearsal: None,
            reading_view,
            ink_dialog_open: false,
            zoom_return: None,
            triggered_played: Vec::new(),
        });
        self.enter_slide_animations(true, cx);
        self.focus_handle.focus(window, cx);
        self.time_slideshow(window, cx);
        cx.notify();
    }

    /// Start the slide show in rehearsal mode from the first slide.
    pub(crate) fn start_rehearsal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_some() || !self.has_document || self.loading || !self.pages_are_slides() {
            return;
        }
        self.show_slide_number(1, window, cx);
        self.start_show(false, window, cx);
        let Some(show) = self.slideshow.as_mut() else { return };
        let now = crate::clock::Instant::now();
        show.rehearsal = Some(Rehearsal {
            slide_started: now,
            slide_accumulated: std::time::Duration::ZERO,
            slide_paused: None,
            total_started: now,
            total_accumulated: std::time::Duration::ZERO,
            total_paused: None,
            recorded_timings: HashMap::new(),
            dialog_open: false,
            final_total: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_secs(1)).await;
                let going = this.update(cx, |this, cx| {
                    let going = this.slideshow.as_ref().is_some_and(|s| s.rehearsal.is_some());
                    if going {
                        cx.notify();
                    }
                    going
                });
                if !matches!(going, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// Toggle pause / resume during rehearsal timing measurement.
    pub(crate) fn toggle_rehearsal_pause(&mut self, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        let Some(rehearsal) = show.rehearsal.as_mut() else { return };
        if let Some(s) = rehearsal.slide_paused.take() {
            rehearsal.slide_accumulated = s;
            rehearsal.slide_started = crate::clock::Instant::now();
            let t = rehearsal.total_paused.take().unwrap_or(rehearsal.total_accumulated);
            rehearsal.total_accumulated = t;
            rehearsal.total_started = crate::clock::Instant::now();
        } else {
            let s = rehearsal.slide_accumulated + rehearsal.slide_started.elapsed();
            rehearsal.slide_paused = Some(s);
            let t = rehearsal.total_accumulated + rehearsal.total_started.elapsed();
            rehearsal.total_paused = Some(t);
        }
        cx.notify();
    }

    /// Repeat the current slide's rehearsal from 0 seconds.
    pub(crate) fn repeat_rehearsal_slide(&mut self, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        let Some(rehearsal) = show.rehearsal.as_mut() else { return };
        rehearsal.slide_accumulated = std::time::Duration::ZERO;
        rehearsal.slide_started = crate::clock::Instant::now();
        rehearsal.slide_paused = None;
        cx.notify();
    }

    /// Save recorded rehearsal slide timings to slide transitions and enable `use_timings`.
    pub(crate) fn save_rehearsal_timings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        let Some(rehearsal) = show.rehearsal.as_ref() else { return };
        let timings = rehearsal.recorded_timings.clone();
        let mut commands = Vec::new();
        for (slide_id, ms) in timings {
            let mut trans = self.document.presentation().slide(slide_id)
                .and_then(|s| s.transition.clone())
                .unwrap_or_else(|| ibis_doc_model::Transition::new(""));
            trans.after = Some(ms);
            commands.push(ibis_doc_model::Command::SetTransition { slide: slide_id, transition: Some(trans) });
        }
        let mut props = self.document.presentation().show_properties.clone();
        props.use_timings = true;
        commands.push(ibis_doc_model::Command::SetShowProperties { properties: props });
        self.execute(commands, cx);
        self.end_slideshow(window, cx);
    }

    /// Discard recorded rehearsal slide timings and end the slideshow.
    pub(crate) fn discard_rehearsal_timings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_slideshow(window, cx);
    }

    /// Start the slide show from the slide shown, with the presenter's view;
    /// its clock is redrawn every second.
    pub(crate) fn start_presenter_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start_presenter_view_on(true, window, cx);
    }

    /// The presenter's view; with `own_windows` and two displays or more, in
    /// windows of their own (the editor's stays).
    pub(crate) fn start_presenter_view_on(&mut self, own_windows: bool, window: &mut Window, cx: &mut Context<Self>) {
        let displays = cx.displays();
        let editor_display = window.display(cx).map(|d| d.id());
        let other = own_windows && editor_display.is_some() && displays.iter().any(|d| Some(d.id()) != editor_display);
        self.start_show(!other, window, cx);
        let Some(show) = self.slideshow.as_mut() else { return };
        show.presenter = true;
        if let (true, Some(editor_display)) = (other, editor_display) {
            show.external = Some(ShowWindows { editor_display, swapped: false, windows: Vec::new() });
            // Not here, inside the editor's own update: GPUI draws a window once before `open_window` returns, and the show's windows
            // update the editor to draw (`ShowView::render`), which panics while the editor is being updated (a panic abort on Windows,
            // exit code 0xc0000409: issue #51, the presenter view started with two displays).
            let editor = cx.entity();
            let handle = window.window_handle();
            cx.defer(move |cx| {
                Self::open_show_windows(&editor, cx);
                // No window could be opened: as with one display, in the editor's window.
                handle
                    .update(cx, |_, window, cx| {
                        editor.update(cx, |this, cx| {
                            if this.show_windows().is_empty()
                                && let Some(show) = this.slideshow.as_mut()
                            {
                                show.external = None;
                                show.made_fullscreen = !window.is_fullscreen();
                                if show.made_fullscreen {
                                    window.toggle_fullscreen();
                                }
                                cx.notify();
                            }
                        })
                    })
                    .ok();
            });
        }
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_secs(1)).await;
                let going = this.update(cx, |this, cx| {
                    let going = this.slideshow.as_ref().is_some_and(|s| s.presenter);
                    if going {
                        cx.notify();
                    }
                    going
                });
                if !matches!(going, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// The presenter's view's notes: the size, whether the teleprompter runs, and how far they are scrolled (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub(crate) fn notes_view_state(&self) -> Option<(f32, Option<f32>, f32)> {
        let show = self.slideshow.as_ref()?;
        Some((show.notes_size, show.teleprompter, -f32::from(show.notes_scroll.offset().y)))
    }

    /// 発表者ビューのノートの文字の大きさを変える（8〜72 pt）。
    pub(crate) fn change_notes_size(&mut self, delta: f32, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        show.notes_size = (show.notes_size + delta).clamp(8.0, 72.0);
        cx.notify();
    }

    /// 発表者ビューのテレプロンプター: the notes scroll by themselves (on, at 20 pt per second at first; off).
    pub(crate) fn toggle_teleprompter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        if show.teleprompter.take().is_some() {
            cx.notify();
            return;
        }
        show.teleprompter = Some(20.0);
        cx.spawn_in(window, async move |this, cx| {
            let step = std::time::Duration::from_millis(50);
            loop {
                cx.background_executor().timer(step).await;
                let going = this.update(cx, |this, cx| {
                    let Some(show) = this.slideshow.as_ref() else { return false };
                    let Some(speed) = show.teleprompter else { return false };
                    let at = show.notes_scroll.offset();
                    let max = show.notes_scroll.max_offset();
                    let next = (f32::from(at.y) - speed * step.as_secs_f32()).max(-f32::from(max.y));
                    show.notes_scroll.set_offset(point(at.x, px(next)));
                    cx.notify();
                    true
                });
                if !matches!(going, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// テレプロンプターの速さを変える（5〜120 pt 毎秒）。
    pub(crate) fn change_teleprompter_speed(&mut self, delta: f32, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        if let Some(speed) = show.teleprompter.as_mut() {
            *speed = (*speed + delta).clamp(5.0, 120.0);
        }
        cx.notify();
    }

    /// 発表者ビューの経過時間（秒）
    pub(crate) fn presenter_timer_elapsed_secs(&self) -> u64 {
        let Some(show) = self.slideshow.as_ref() else { return 0 };
        if let Some(paused) = show.paused_elapsed {
            paused.as_secs()
        } else {
            (show.accumulated + show.started.elapsed()).as_secs()
        }
    }

    /// 発表者ビューのタイマーが一時停止中かどうか
    pub(crate) fn presenter_timer_is_paused(&self) -> bool {
        self.slideshow.as_ref().is_some_and(|s| s.paused_elapsed.is_some())
    }

    /// 発表者ビューのタイマーの一時停止 / 再開をトグル
    pub(crate) fn toggle_presenter_timer_pause(&mut self, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        if let Some(paused) = show.paused_elapsed.take() {
            show.accumulated = paused;
            show.started = crate::clock::Instant::now();
        } else {
            show.paused_elapsed = Some(show.accumulated + show.started.elapsed());
        }
        cx.notify();
    }

    /// 発表者ビューのタイマーを 0 にリセット
    pub(crate) fn reset_presenter_timer(&mut self, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        if show.paused_elapsed.is_some() {
            show.paused_elapsed = Some(std::time::Duration::ZERO);
        } else {
            show.accumulated = std::time::Duration::ZERO;
            show.started = crate::clock::Instant::now();
        }
        cx.notify();
    }

    /// The displays of the presenter's view and the audience's slide show
    /// (the editor's display and another one, exchanged when swapped).
    fn show_displays(&self, cx: &App) -> Option<(std::rc::Rc<dyn PlatformDisplay>, std::rc::Rc<dyn PlatformDisplay>)> {
        let external = self.slideshow.as_ref()?.external.as_ref()?;
        let displays = cx.displays();
        let editor = displays.iter().find(|d| d.id() == external.editor_display).cloned().or_else(|| cx.primary_display())?;
        let other = displays.iter().find(|d| d.id() != editor.id()).cloned()?;
        Some(if external.swapped { (other, editor) } else { (editor, other) })
    }

    /// The full-screen windows of the presenter's view and the audience's
    /// slide show, each on its display.
    ///
    /// Not inside an update of the editor: GPUI draws a window once before `open_window` returns, and a show window updates the
    /// editor to draw (`ShowView::render`), which panics while the editor is being updated.
    fn open_show_windows(editor: &Entity<Self>, cx: &mut App) {
        let Some((presenter, audience)) = editor.read(cx).show_displays(cx) else { return };
        let mut handles = Vec::new();
        for (kind, display) in [(ShowKind::Audience, audience), (ShowKind::Presenter, presenter)] {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Fullscreen(display.bounds())),
                display_id: Some(display.id()),
                titlebar: None,
                focus: kind == ShowKind::Presenter,
                ..Default::default()
            };
            let editor = editor.clone();
            match cx.open_window(options, move |window, cx| cx.new(|cx| super::show_window::ShowView::new(editor, kind, window, cx))) {
                Ok(handle) => handles.push(handle.into()),
                Err(err) => log::warn!("Cannot open the slide show window: {err}"),
            }
        }
        editor.update(cx, |this, _| {
            if let Some(external) = this.slideshow.as_mut().and_then(|s| s.external.as_mut()) {
                external.windows = handles;
            }
        });
    }

    /// The windows of the show on their own displays (presenter's view,
    /// audience's slide show), if it has them.
    #[allow(dead_code)]
    pub(crate) fn show_windows(&self) -> Vec<AnyWindowHandle> {
        self.slideshow.as_ref().and_then(|s| s.external.as_ref()).map(|e| e.windows.clone()).unwrap_or_default()
    }

    /// Close the show's windows once the event being handled is done (it may
    /// be one of theirs).
    fn close_show_windows(&mut self, windows: Vec<AnyWindowHandle>, cx: &mut Context<Self>) {
        if windows.is_empty() {
            return;
        }
        cx.defer(move |cx| {
            for handle in windows {
                handle.update(cx, |_, window, _| window.remove_window()).ok();
            }
        });
    }

    /// 画面の入れ替え: the presenter's view and the audience's slide show
    /// change displays.
    pub(crate) fn swap_show_displays(&mut self, cx: &mut Context<Self>) {
        let Some(external) = self.slideshow.as_mut().and_then(|s| s.external.as_mut()) else { return };
        external.swapped = !external.swapped;
        let old = std::mem::take(&mut external.windows);
        self.close_show_windows(old, cx);
        // After this update (see `start_presenter_view_on`).
        let editor = cx.entity();
        cx.defer(move |cx| Self::open_show_windows(&editor, cx));
    }

    /// End the slide show, back to the editor on the slide it ended on.
    pub(crate) fn end_slideshow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.take() else { return };
        if let Some(state) = self.pptx_mut() {
            state.slide_popup = None;
        }
        self.stop_clips(cx);
        self.close_show_windows(show.external.map(|e| e.windows).unwrap_or_default(), cx);
        if show.made_fullscreen && window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        // The slide list follows the slide shown last.
        if let Some(index) = self.presentation().slide_index(self.current_slide) {
            let row = self.row_of_slide(index);
            self.list_scroll.scroll_to_item(row);
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Context menu entries for the slide show / presenter view / reading view (PPTX-SHOW-009..017, PPTX-RECORD-008, PPTX-NOTES-015).
    pub(crate) fn slideshow_context_menu_entries(&self) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let Some(show) = self.slideshow.as_ref() else { return Vec::new() };
        let mode = show.pointer_mode;
        let has_prev_viewed = show.came_from.is_some();
        let grid_open = show.grid_open;
        let zoomed = show.zoom > 1.001;
        let blank = show.blank;
        let has_inks = show.slide_inks.get(&self.current_slide).is_some_and(|v| !v.is_empty());

        let mut entries: Vec<MenuEntry> = vec![
            MenuItem::tr("pptx.links.next-slide").icon("arrow_forward").shortcut("→").on(|e, w, cx| e.step_slideshow(true, w, cx)).into(),
            MenuItem::tr("pptx.links.previous-slide").icon("arrow_back").shortcut("←").on(|e, w, cx| e.step_slideshow(false, w, cx)).into(),
            MenuItem::tr("pptx.links.last-slide-viewed").icon("history").disabled(!has_prev_viewed).on(|e, w, cx| e.follow_link_in_show(&ibis_doc_model::SlideLink::LastSlideViewed, w, cx)).into(),
            MenuItem::tr("pptx.misc.see-all-slides").icon("grid_view").checked(grid_open).on(|e, _, cx| e.toggle_slideshow_grid(cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("_shell.zoom-in").icon("zoom_in").on(|e, _, cx| e.zoom_slideshow(1.25, cx)).into(),
            MenuItem::tr("_shell.zoom-out").icon("zoom_out").disabled(!zoomed).on(|e, _, cx| e.zoom_slideshow(0.8, cx)).into(),
            MenuItem::tr("_pptx.100").icon("fit_screen").disabled(!zoomed).on(|e, _, cx| e.reset_slideshow_zoom(cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("_shell.black").icon("contrast").shortcut("B").checked(blank == Some(0x000000)).on(|e, _, cx| e.blank_slideshow(0x000000, cx)).into(),
            MenuItem::tr("_shell.white").icon("wb_sunny").shortcut("W").checked(blank == Some(0xffffff)).on(|e, _, cx| e.blank_slideshow(0xffffff, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("_shell.arrow").icon("arrow_selector_tool").checked(mode == PointerMode::Arrow).on(|e, _, cx| e.set_slideshow_pointer_mode(PointerMode::Arrow, cx)).into(),
            MenuItem::tr("pptx.misc.laser-pointer").icon("center_focus_strong").checked(mode == PointerMode::Laser).on(|e, _, cx| e.set_slideshow_pointer_mode(PointerMode::Laser, cx)).into(),
            MenuItem::tr("pptx.misc.pen").icon("edit").checked(mode == PointerMode::Pen).on(|e, _, cx| e.set_slideshow_pointer_mode(PointerMode::Pen, cx)).into(),
            MenuItem::tr("pptx.misc.highlighter").icon("ink_highlighter").checked(mode == PointerMode::Highlighter).on(|e, _, cx| e.set_slideshow_pointer_mode(PointerMode::Highlighter, cx)).into(),
            MenuItem::tr("pptx.misc.eraser").icon("cleaning_services").checked(mode == PointerMode::Eraser).on(|e, _, cx| e.set_slideshow_pointer_mode(PointerMode::Eraser, cx)).into(),
            MenuItem::tr("pptx.misc.clear-all-ink-slide").icon("delete_sweep").disabled(!has_inks).on(|e, _, cx| e.clear_current_slide_inks(cx)).into(),
        ];

        if show.presenter {
            let is_paused = show.paused_elapsed.is_some();
            let teleprompter_on = show.teleprompter.is_some();
            let has_external = show.external.is_some();
            entries.extend([
                MenuEntry::Separator,
                MenuItem::tr(if is_paused { "slideshow.rehearsal.resume" } else { "slideshow.rehearsal.pause" })
                    .icon(if is_paused { "play_arrow" } else { "pause" })
                    .on(|e, _, cx| e.toggle_presenter_timer_pause(cx))
                    .into(),
                MenuItem::tr("pptx.slideshow.reset")
                    .icon("restart_alt")
                    .on(|e, _, cx| e.reset_presenter_timer(cx))
                    .into(),
                MenuItem::tr("pptx.slideshow.teleprompter")
                    .icon("auto_stories")
                    .checked(teleprompter_on)
                    .on(|e, w, cx| e.toggle_teleprompter(w, cx))
                    .into(),
                MenuItem::tr("pptx.slideshow.notes-larger")
                    .icon("text_increase")
                    .on(|e, _, cx| e.change_notes_size(2.0, cx))
                    .into(),
                MenuItem::tr("pptx.slideshow.notes-smaller")
                    .icon("text_decrease")
                    .on(|e, _, cx| e.change_notes_size(-2.0, cx))
                    .into(),
                MenuItem::tr("pptx.slideshow.swap-displays")
                    .icon("swap_horiz")
                    .disabled(!has_external)
                    .on(|e, _, cx| e.swap_show_displays(cx))
                    .into(),
            ]);
        }

        if show.reading_view {
            entries.extend([
                MenuEntry::Separator,
                MenuItem::tr("view.slideshow")
                    .icon("play_arrow")
                    .shortcut("⌘↩")
                    .on(|e, w, cx| {
                        e.end_slideshow(w, cx);
                        e.start_slideshow(w, cx);
                    })
                    .into(),
                MenuItem::tr("_pptx.return-to-normal-view")
                    .icon("view_compact")
                    .on(|e, w, cx| e.switch_to_normal_view(w, cx))
                    .into(),
            ]);
        }

        entries.push(MenuEntry::Separator);
        entries.push(MenuItem::tr("pptx.links.end-show").icon("close").shortcut("Esc").on(|e, w, cx| e.request_end_slideshow(w, cx)).into());
        entries
    }

    /// Open context menu during the slide show at pointer `position`.
    pub(crate) fn open_slideshow_context_menu(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.slideshow.is_none() {
            return;
        }
        let entries = self.slideshow_context_menu_entries();
        self.open_popup_entries(entries, position, window, cx);
    }

    /// Request ending slideshow. If there are ink annotations and the confirmation dialog is not open,
    /// opens the dialog to ask whether to keep ink annotations.
    pub(crate) fn request_end_slideshow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(show) = self.slideshow.as_mut() {
            if show.has_inks() && !show.ink_dialog_open {
                show.ink_dialog_open = true;
                cx.notify();
                return;
            }
        }
        self.end_slideshow(window, cx);
    }

    /// End slideshow with explicit choice of whether to keep ink annotations as slide shapes.
    pub(crate) fn end_slideshow_with_ink_choice(&mut self, keep: bool, window: &mut Window, cx: &mut Context<Self>) {
        if keep {
            if let Some(show) = self.slideshow.as_ref() {
                let inks = show.slide_inks.clone();
                self.keep_slideshow_inks(inks, cx);
            }
        }
        self.end_slideshow(window, cx);
    }

    /// Convert slideshow ink strokes into slide shapes and insert them via undoable commands.
    pub(crate) fn keep_slideshow_inks(
        &mut self,
        inks: HashMap<ibis_doc_model::SlideId, Vec<InkStroke>>,
        cx: &mut Context<Self>,
    ) {
        use ibis_doc_model::Command;
        let slide_size = self.presentation().slide_size;
        let mut commands = Vec::new();
        for (slide_id, strokes) in inks {
            let mut index = self
                .presentation()
                .slide(slide_id)
                .map(|s| s.shapes.len())
                .unwrap_or(0);
            for stroke in strokes {
                let shape_id = self.document.ids().new_shape_id();
                if let Some(shape) = ink_stroke_to_shape(&stroke, slide_size, shape_id) {
                    commands.push(Command::InsertShape {
                        slide: slide_id,
                        index,
                        shape,
                    });
                    index += 1;
                }
            }
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
    }

    /// The next slide (`forward`) or the previous one of the slide show.
    pub(crate) fn step_slideshow(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let groups = animation_groups(&self.document.presentation().slide(self.current_slide).map(|s| s.animations.clone()).unwrap_or_default());
        let show_props = &self.document.presentation().show_properties;
        let is_loop = show_props.loop_continuously || matches!(show_props.show_type, ibis_doc_model::ShowType::Kiosk { .. });
        let show_anim = show_props.show_animation;
        let Some(show) = self.slideshow.as_mut() else { return };
        if show.rehearsal.as_ref().is_some_and(|r| r.dialog_open) {
            return;
        }
        // Out of a blank screen first, on the same slide.
        if show.blank.take().is_some() {
            return cx.notify();
        }

        let show_slides = presentation_show_slides(self.document.presentation(), show.custom_show);
        let first_in_range = show_slides.first().copied();
        let cur_pos = show_slides.iter().position(|&id| id == self.current_slide);
        let next = cur_pos.and_then(|pos| show_slides.get(pos + 1)).copied();
        let previous = cur_pos.and_then(|pos| if pos > 0 { show_slides.get(pos - 1).copied() } else { None });

        // The slide's animations first: the next group, or back one (unless animations disabled).
        if !show.at_end && show_anim {
            if forward && show.step < groups.len() {
                show.step += 1;
                show.played += 1;
                return cx.notify();
            }
            if !forward && show.step > groups.iter().take_while(|g| g.auto).count() {
                show.step -= 1;
                return cx.notify();
            }
        }
        if forward && show.rehearsal.is_some() {
            let rehearsal = show.rehearsal.as_mut().unwrap();
            let elapsed_ms = rehearsal.slide_elapsed().as_millis().max(1000) as u32;
            rehearsal.recorded_timings.insert(self.current_slide, elapsed_ms);
        }
        // If Return to Zoom is armed and the trigger slide just completed, return to the zoom source slide (PPTX-TRANS-016).
        if forward && !show.at_end {
            if let Some(ret) = show.zoom_return.as_ref() {
                if self.current_slide == ret.trigger_slide_id {
                    let return_to = ret.return_slide_id;
                    show.zoom_return = None;
                    if let Some(rehearsal) = show.rehearsal.as_mut() {
                        rehearsal.slide_started = crate::clock::Instant::now();
                        rehearsal.slide_accumulated = std::time::Duration::ZERO;
                        rehearsal.slide_paused = None;
                    }
                    show.came_from = Some(self.current_slide);
                    show.changes += 1;
                    self.current_slide = return_to;
                    self.enter_slide_animations(true, cx);
                    self.resources.preload.prioritize(self.slide());
                    self.time_slideshow(window, cx);
                    return cx.notify();
                }
            }
        }
        match (forward, show.at_end) {
            (true, true) => {
                if let Some(rehearsal) = show.rehearsal.as_mut() {
                    rehearsal.dialog_open = true;
                    if rehearsal.final_total.is_none() {
                        rehearsal.final_total = Some(rehearsal.total_elapsed());
                    }
                    return cx.notify();
                }
                if is_loop {
                    show.at_end = false;
                    self.current_slide = first_in_range.unwrap_or(self.current_slide);
                    self.enter_slide_animations(true, cx);
                } else {
                    return self.request_end_slideshow(window, cx);
                }
            }
            (false, true) => show.at_end = false,
            (true, false) => match next {
                Some(id) => {
                    if let Some(rehearsal) = show.rehearsal.as_mut() {
                        rehearsal.slide_started = crate::clock::Instant::now();
                        rehearsal.slide_accumulated = std::time::Duration::ZERO;
                        rehearsal.slide_paused = None;
                    }
                    show.came_from = Some(self.current_slide);
                    show.changes += 1;
                    self.current_slide = id;
                    self.enter_slide_animations(true, cx);
                }
                None => {
                    if let Some(rehearsal) = show.rehearsal.as_mut() {
                        rehearsal.dialog_open = true;
                        if rehearsal.final_total.is_none() {
                            rehearsal.final_total = Some(rehearsal.total_elapsed());
                        }
                        return cx.notify();
                    }
                    if is_loop {
                        show.came_from = Some(self.current_slide);
                        show.changes += 1;
                        self.current_slide = first_in_range.unwrap_or(self.current_slide);
                        self.enter_slide_animations(true, cx);
                    } else {
                        show.at_end = true;
                    }
                }
            },
            (false, false) => match previous {
                Some(id) => {
                    if let Some(rehearsal) = show.rehearsal.as_mut() {
                        rehearsal.slide_started = crate::clock::Instant::now();
                        rehearsal.slide_accumulated = std::time::Duration::ZERO;
                        rehearsal.slide_paused = None;
                    }
                    show.came_from = None;
                    self.current_slide = id;
                    self.enter_slide_animations(false, cx);
                }
                // Before the first slide nothing changes.
                None => return,
            },
        }
        // Its pictures before those still waiting.
        self.resources.preload.prioritize(self.slide());
        self.time_slideshow(window, cx);
        cx.notify();
    }

    /// スライド上の自動再生クリップを開始
    pub(crate) fn play_automatic_clips(&mut self, cx: &mut Context<Self>) {
        let auto_clips: Vec<ibis_doc_model::ShapeId> = self.slide().shapes.iter().filter_map(|sh| {
            if let ibis_doc_model::ShapeKind::Picture(p) = &sh.kind {
                if p.clip.as_ref().is_some_and(|c| c.options.start == ibis_doc_model::MediaStart::Automatically) {
                    let is_playing = self.pptx().is_some_and(|st| st.clips.iter().any(|c| c.shape == sh.id));
                    if !is_playing {
                        return Some(sh.id);
                    }
                }
            }
            None
        }).collect();
        for id in auto_clips {
            self.toggle_clip(id, cx);
        }
    }

    /// The slide shown comes in: its animations from the start (`fresh`:
    /// those before the first click play), or all played (coming back).
    fn enter_slide_animations(&mut self, fresh: bool, cx: &mut Context<Self>) {
        self.stop_clips_on_slide_change(cx);
        let groups = animation_groups(&self.document.presentation().slide(self.current_slide).map(|s| s.animations.clone()).unwrap_or_default());
        let show_anim = self.presentation().show_properties.show_animation;
        let Some(show) = self.slideshow.as_mut() else { return };
        show.zoom = 1.0;
        show.pan = (0.0, 0.0);
        show.drag_start = None;
        show.step = if !show_anim || !fresh { groups.len() } else { groups.iter().take_while(|g| g.auto).count() };
        show.played += 1;
        show.triggered_played.clear();
        if fresh {
            self.play_automatic_clips(cx);
        }
    }

    /// Trigger animations tied to clicking a specific shape (OnShapeClick, PPTX-ANIM-015).
    pub(crate) fn trigger_animations_for_shape_click(&mut self, clicked_shape: ibis_doc_model::ShapeId, cx: &mut Context<Self>) -> bool {
        let indices: Vec<usize> = self.slide().animations.iter().enumerate()
            .filter(|(_, a)| matches!(a.trigger, ibis_doc_model::AnimationTrigger::OnShapeClick(s) if s == clicked_shape))
            .map(|(i, _)| i)
            .collect();
        if indices.is_empty() {
            return false;
        }
        if let Some(show) = self.slideshow.as_mut() {
            for idx in indices {
                if !show.triggered_played.contains(&idx) {
                    show.triggered_played.push(idx);
                }
            }
            show.played += 1;
        }
        cx.notify();
        true
    }

    /// Trigger animations tied to reaching a media bookmark (OnMediaBookmark, PPTX-ANIM-016 / PPTX-VIDEO-015).
    pub(crate) fn trigger_animations_for_media_bookmark(&mut self, shape: ibis_doc_model::ShapeId, bookmark: &str, cx: &mut Context<Self>) -> bool {
        let indices: Vec<usize> = self.slide().animations.iter().enumerate()
            .filter(|(_, a)| matches!(&a.trigger, ibis_doc_model::AnimationTrigger::OnMediaBookmark { shape: s, bookmark: b } if *s == shape && b == bookmark))
            .map(|(i, _)| i)
            .collect();
        if indices.is_empty() {
            return false;
        }
        if let Some(show) = self.slideshow.as_mut() {
            for idx in indices {
                if !show.triggered_played.contains(&idx) {
                    show.triggered_played.push(idx);
                }
            }
            show.played += 1;
        }
        cx.notify();
        true
    }

    /// A click on the slide show: the next slide, unless the slide shown
    /// says not to move on with a click (`advClick="0"`).
    fn click_slideshow(&mut self, position: Point<Pixels>, audience: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((at_end, blank, presenter, zoom, pan)) = self.slideshow.as_ref().map(|s| (s.at_end, s.blank, s.presenter, s.zoom, s.pan)) else { return };
        // A click on a link (ハイパーリンク) follows it.
        if !at_end && blank.is_none() && (!presenter || audience) {
            let size = self.presentation().size_of(self.slide());
            let view = window.viewport_size();
            let scale = (f32::from(view.width) / size.width.max(1) as f32).min(f32::from(view.height) / size.height.max(1) as f32);
            let (w, h) = (size.width as f32 * scale, size.height as f32 * scale);
            let (x0, y0) = ((f32::from(view.width) - w) / 2.0, (f32::from(view.height) - h) / 2.0);
            let content_x0 = x0 + (w - w * zoom) / 2.0 + pan.0;
            let content_y0 = y0 + (h - h * zoom) / 2.0 + pan.1;
            let at = ibis_doc_model::EmuPoint::new(
                ((f32::from(position.x) - content_x0) / (scale * zoom)) as i64,
                ((f32::from(position.y) - content_y0) / (scale * zoom)) as i64,
            );
            // A click on a video or audio clip plays it, or pauses it.
            if let Some(shape) = self.clip_at(at) {
                return self.toggle_clip(shape, cx);
            }
            // A click on an animation trigger shape fires its triggered animations (PPTX-ANIM-015).
            if let Some(clicked_shape) = self.slide().hit_test(at) {
                if self.trigger_animations_for_shape_click(clicked_shape, cx) {
                    return;
                }
            }
            if let Some((shape, link)) = self.slide_shape_and_link_at(at) {
                return self.follow_link_from_shape_in_show(Some(shape), &link, window, cx);
            }
        }
        let by_click = at_end || blank.is_some() || self.slide().transition.as_ref().is_none_or(|t| t.on_click);
        if by_click {
            self.step_slideshow(true, window, cx);
        }
    }

    /// Mouse press during the slide show: handles pointer tools (pen, eraser, laser) or advances slides.
    pub(crate) fn mouse_down_slideshow(&mut self, pos: Point<Pixels>, audience: bool, window: &mut Window, cx: &mut Context<Self>) {
        let slide_size = self.presentation().size_of(self.slide());
        let current_slide_id = self.current_slide;
        let Some(show) = self.slideshow.as_mut() else { return };
        if show.rehearsal.as_ref().is_some_and(|r| r.dialog_open) {
            return;
        }
        if show.at_end || show.blank.is_some() || (show.presenter && !audience) {
            self.click_slideshow(pos, audience, window, cx);
            return;
        }
        let view = window.viewport_size();
        let norm = slide_normalized_point(pos, view, slide_size, show.zoom, show.pan);
        match show.pointer_mode {
            PointerMode::Arrow => {
                if show.zoom > 1.001 {
                    show.drag_start = Some(pos);
                    show.pan_start = show.pan;
                } else {
                    self.click_slideshow(pos, audience, window, cx);
                }
            }
            PointerMode::Laser => {
                show.laser_pos = norm;
                cx.notify();
            }
            PointerMode::Pen => {
                if let Some(p) = norm {
                    show.active_stroke = Some(InkStroke {
                        points: vec![p],
                        color: 0xdf2020,
                        width: 3.0,
                        opacity: 1.0,
                    });
                    cx.notify();
                }
            }
            PointerMode::Highlighter => {
                if let Some(p) = norm {
                    show.active_stroke = Some(InkStroke {
                        points: vec![p],
                        color: 0xffe600,
                        width: 16.0,
                        opacity: 0.5,
                    });
                    cx.notify();
                }
            }
            PointerMode::Eraser => {
                if let Some(p) = norm {
                    let scale = (f32::from(view.width) / slide_size.width.max(1) as f32).min(f32::from(view.height) / slide_size.height.max(1) as f32);
                    let (w, h) = (slide_size.width as f32 * scale * show.zoom, slide_size.height as f32 * scale * show.zoom);
                    let threshold_px = 20.0;
                    if let Some(inks) = show.slide_inks.get_mut(&current_slide_id) {
                        let before_len = inks.len();
                        inks.retain(|stroke| !stroke_hits_point(stroke, p, w, h, threshold_px));
                        if inks.len() != before_len {
                            cx.notify();
                        }
                    }
                }
            }
        }
    }

    /// Mouse move during the slide show: updates laser position or drawing stroke.
    pub(crate) fn mouse_move_slideshow(&mut self, pos: Point<Pixels>, audience: bool, window: &mut Window, cx: &mut Context<Self>) {
        let slide_size = self.presentation().size_of(self.slide());
        let Some(show) = self.slideshow.as_mut() else { return };
        if show.at_end || show.blank.is_some() || (show.presenter && !audience) {
            return;
        }
        let view = window.viewport_size();
        if let Some(start) = show.drag_start {
            let dx = f32::from(pos.x - start.x);
            let dy = f32::from(pos.y - start.y);
            let scale = (f32::from(view.width) / slide_size.width.max(1) as f32).min(f32::from(view.height) / slide_size.height.max(1) as f32);
            let (w, h) = (slide_size.width as f32 * scale, slide_size.height as f32 * scale);
            let max_pan_x = ((show.zoom - 1.0) * w) / 2.0;
            let max_pan_y = ((show.zoom - 1.0) * h) / 2.0;
            show.pan.0 = (show.pan_start.0 + dx).clamp(-max_pan_x, max_pan_x);
            show.pan.1 = (show.pan_start.1 + dy).clamp(-max_pan_y, max_pan_y);
            cx.notify();
            return;
        }
        let norm = slide_normalized_point(pos, view, slide_size, show.zoom, show.pan);
        match show.pointer_mode {
            PointerMode::Laser => {
                show.laser_pos = norm;
                cx.notify();
            }
            PointerMode::Pen | PointerMode::Highlighter => {
                if let Some(stroke) = show.active_stroke.as_mut()
                    && let Some(p) = norm
                {
                    let should_add = stroke.points.last().map_or(true, |last| {
                        (last.x - p.x).hypot(last.y - p.y) > 0.001
                    });
                    if should_add {
                        stroke.points.push(p);
                        cx.notify();
                    }
                }
            }
            _ => {}
        }
    }

    /// Mouse release during the slide show: finishes active drawing stroke.
    pub(crate) fn mouse_up_slideshow(&mut self, pos: Point<Pixels>, audience: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        if let Some(start) = show.drag_start.take() {
            let dist = f32::from(pos.x - start.x).hypot(f32::from(pos.y - start.y));
            if dist < 3.0 && show.pointer_mode == PointerMode::Arrow {
                self.click_slideshow(pos, audience, window, cx);
            }
            return;
        }
        if let Some(stroke) = show.active_stroke.take() {
            if !stroke.points.is_empty() {
                let slide_id = self.current_slide;
                show.slide_inks.entry(slide_id).or_default().push(stroke);
                cx.notify();
            }
        }
    }

    /// Change pointer mode (Arrow, Laser, Pen, Highlighter, Eraser).
    pub(crate) fn set_slideshow_pointer_mode(&mut self, mode: PointerMode, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        show.pointer_mode = mode;
        if mode != PointerMode::Laser {
            show.laser_pos = None;
        }
        show.active_stroke = None;
        cx.notify();
    }

    /// Clear all ink strokes drawn on the current slide.
    pub(crate) fn clear_current_slide_inks(&mut self, cx: &mut Context<Self>) {
        let slide_id = self.current_slide;
        let Some(show) = self.slideshow.as_mut() else { return };
        if let Some(inks) = show.slide_inks.get_mut(&slide_id) {
            inks.clear();
            cx.notify();
        }
    }

    /// Toggle the "See All Slides" grid overview overlay during the slide show.
    pub(crate) fn toggle_slideshow_grid(&mut self, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        show.grid_open = !show.grid_open;
        cx.notify();
    }

    /// Jump to a chosen slide from the grid overview and close the grid.
    pub(crate) fn select_slide_in_grid(&mut self, id: ibis_doc_model::SlideId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        show.grid_open = false;
        show.at_end = false;
        show.blank = None;
        show.came_from = Some(self.current_slide);
        show.changes += 1;
        show.zoom = 1.0;
        show.pan = (0.0, 0.0);
        show.drag_start = None;
        self.current_slide = id;
        self.enter_slide_animations(true, cx);
        self.resources.preload.prioritize(self.slide());
        self.time_slideshow(window, cx);
        cx.notify();
    }

    /// Zoom in or out during the slide show by a factor (e.g. 1.25 or 0.8).
    pub(crate) fn zoom_slideshow(&mut self, factor: f32, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        let new_zoom = (show.zoom * factor).clamp(1.0, 4.0);
        show.zoom = new_zoom;
        if (show.zoom - 1.0).abs() < 0.001 {
            show.zoom = 1.0;
            show.pan = (0.0, 0.0);
            show.drag_start = None;
        }
        cx.notify();
    }

    /// Reset zoom and pan during the slide show to 100% (fit).
    pub(crate) fn reset_slideshow_zoom(&mut self, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        show.zoom = 1.0;
        show.pan = (0.0, 0.0);
        show.drag_start = None;
        cx.notify();
    }

    /// Move on by itself from the slide shown after its time (`advTm`),
    /// counted from when its transition has played.
    fn time_slideshow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.presentation().show_properties.use_timings {
            return;
        }
        let Some(show) = self.slideshow.as_mut() else { return };
        show.shown += 1;
        if show.at_end {
            return;
        }
        let Some(after) = self.document.presentation().slide(self.current_slide).and_then(|s| s.transition.as_ref()).and_then(|t| t.after.map(|a| a + t.duration)) else { return };
        let shown = show.shown;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(after as u64)).await;
            this.update_in(cx, |this, window, cx| {
                if this.slideshow.as_ref().is_some_and(|s| s.shown == shown && s.blank.is_none()) {
                    this.step_slideshow(true, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// A link followed in the show: a page in the browser, or the slide
    /// gone to (its animations from the start).
    pub(crate) fn follow_link_in_show(&mut self, link: &ibis_doc_model::SlideLink, window: &mut Window, cx: &mut Context<Self>) {
        self.follow_link_from_shape_in_show(None, link, window, cx);
    }

    /// A link followed from a specific shape in the show (enables Return to Zoom if configured).
    pub(crate) fn follow_link_from_shape_in_show(&mut self, from_shape: Option<ibis_doc_model::ShapeId>, link: &ibis_doc_model::SlideLink, window: &mut Window, cx: &mut Context<Self>) {
        use ibis_doc_model::SlideLink as L;
        let slides = &self.document.presentation().slides;
        let index = self.document.presentation().slide_index(self.current_slide).unwrap_or(0);
        let to = match link {
            L::Url(url) => return cx.open_url(url),
            L::NextSlide => return self.step_slideshow(true, window, cx),
            L::PreviousSlide => slides[..index].iter().rev().find(|s| !s.hidden).map(|s| s.id),
            L::FirstSlide => return self.slideshow_to_end(false, cx),
            L::LastSlide => return self.slideshow_to_end(true, cx),
            L::LastSlideViewed => {
                let came_from = self.slideshow.as_ref().and_then(|s| s.came_from);
                came_from.and_then(|id| slides.iter().find(|s| s.id == id && !s.hidden).map(|s| s.id))
            }
            L::EndShow => return self.end_slideshow(window, cx),
            L::Slide(id) => slides.iter().any(|s| s.id == *id).then_some(*id),
        };
        let Some(to) = to else { return };

        // Handle Return to Zoom if the clicked shape has return_to_zoom enabled (PPTX-TRANS-016)
        let origin_slide = self.current_slide;
        let zoom_return_state = if let Some(shape_id) = from_shape {
            if self.slide().shape_return_to_zoom.contains(&shape_id) && matches!(link, L::Slide(_)) {
                let p = self.document.presentation();
                let section_last_slide = p.sections.iter().find_map(|sec| {
                    if sec.slides.first() == Some(&to) {
                        sec.slides.last().copied()
                    } else {
                        None
                    }
                });
                let trigger_slide_id = section_last_slide.unwrap_or(to);
                Some(ZoomReturnState {
                    return_slide_id: origin_slide,
                    trigger_slide_id,
                })
            } else {
                None
            }
        } else {
            None
        };

        let Some(show) = self.slideshow.as_mut() else { return };
        show.came_from = Some(origin_slide);
        show.changes += 1;
        if zoom_return_state.is_some() {
            show.zoom_return = zoom_return_state;
        }
        self.current_slide = to;
        self.enter_slide_animations(true, cx);
        self.resources.preload.prioritize(self.slide());
        self.time_slideshow(window, cx);
        cx.notify();
    }

    /// The first slide of the slide show, or the last (`last`).
    pub(crate) fn slideshow_to_end(&mut self, last: bool, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        show.at_end = false;
        show.blank = None;
        // (A timer set for the slide shown before does nothing now.)
        show.shown += 1;
        let slides = &self.document.presentation().slides;
        let shown = |s: &&ibis_doc_model::Slide| !s.hidden;
        let (min_idx, max_idx) = presentation_slide_bounds(self.presentation());
        let in_range = &slides[min_idx..=max_idx];
        let pick = if last { in_range.iter().rev().find(shown) } else { in_range.iter().find(shown) };
        self.current_slide = pick.map_or(if last { in_range[in_range.len() - 1].id } else { in_range[0].id }, |s| s.id);
        self.enter_slide_animations(true, cx);
        self.resources.preload.prioritize(self.slide());
        cx.notify();
    }

    /// Black or white out the slide show in `color`, or show the slide again.
    pub(crate) fn blank_slideshow(&mut self, color: u32, cx: &mut Context<Self>) {
        let Some(show) = self.slideshow.as_mut() else { return };
        show.blank = if show.blank == Some(color) { None } else { Some(color) };
        cx.notify();
    }

    /// Returns the colour the slide show is blanked in (`0x000000` or `0xffffff`), or `None`.
    pub(crate) fn slideshow_blank(&self) -> Option<u32> {
        self.slideshow.as_ref().and_then(|s| s.blank)
    }

    /// The slide show, filling the window, while it runs.
    pub(crate) fn render_slideshow(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.show_in_main() {
            return None;
        }
        self.render_slideshow_for(ShowKind::Main, window, cx)
    }

    /// The slide show as drawn for `kind` (a window of its own: its keys and
    /// focus are the window's).
    pub(crate) fn render_slideshow_for(&self, kind: ShowKind, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        // The slide show's pictures play (animated GIFs) while it is drawn.
        struct Playing<'a>(&'a std::cell::Cell<bool>);
        impl Drop for Playing<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        self.resources.animate_pictures.set(true);
        let _playing = Playing(&self.resources.animate_pictures);
        let show = self.slideshow.as_ref()?;
        let audience = kind == ShowKind::Audience;
        let popup = (!audience).then(|| {
            self.pptx().and_then(|s| s.slide_popup.as_ref()).map(|p| {
                deferred(anchored().position(p.position).snap_to_window_with_margin(px(8.)).child(p.menu.clone())).with_priority(2)
            })
        }).flatten();
        let root = div()
            .id("slideshow")
            .when(kind == ShowKind::Main, |d| d.key_context(SLIDESHOW_CONTEXT).track_focus(&self.focus_handle))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .bg(black())
            .when(show.pointer_mode == PointerMode::Arrow, |d| d.cursor_default())
            .when(show.pointer_mode != PointerMode::Arrow, |d| d.cursor_crosshair())
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| this.mouse_down_slideshow(event.position, audience, window, cx)))
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                if !audience {
                    cx.stop_propagation();
                    this.open_slideshow_context_menu(event.position, window, cx);
                }
            }))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, window, cx| this.mouse_move_slideshow(event.position, audience, window, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(move |this, event: &MouseUpEvent, window, cx| this.mouse_up_slideshow(event.position, audience, window, cx)))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                let steps = this.wheel_pages(event);
                for _ in 0..steps.unsigned_abs() {
                    this.step_slideshow(steps > 0, window, cx);
                }
            }))
            .children(popup);
        if show.grid_open {
            return Some(root.child(self.slideshow_grid_view(show, window, cx)).into_any_element());
        }
        if let Some(color) = show.blank {
            return Some(root.bg(rgb(color)).into_any_element());
        }
        if show.at_end {
            if show.ink_dialog_open {
                return Some(root.child(self.ink_dialog(cx)).into_any_element());
            }
            if let Some(rehearsal) = show.rehearsal.as_ref() {
                if rehearsal.dialog_open {
                    return Some(root.child(self.rehearsal_dialog(rehearsal, cx)).into_any_element());
                }
            }
            return Some(
                root.items_start()
                    .justify_start()
                    .p_4()
                    .text_color(rgb(END_TEXT_COLOR))
                    .child(crate::shell::i18n::text(END_TEXT))
                    .into_any_element(),
            );
        }
        if show.presenter && !audience {
            return Some(root.child(self.presenter_view(show, window, cx)).into_any_element());
        }
        let slide = self.slide();
        let size = self.presentation().size_of(slide);
        let view = window.viewport_size();
        let scale = (f32::from(view.width) / size.width.max(1) as f32).min(f32::from(view.height) / size.height.max(1) as f32);
        let (w, h) = (size.width as f32 * scale, size.height as f32 * scale);
        let current_id = slide.id;
        let zoom = show.zoom;
        let pan = show.pan;
        let (zw, zh) = (w * zoom, h * zoom);
        let ox = (w - zw) / 2.0 + pan.0;
        let oy = (h - zh) / 2.0 + pan.1;
        let zoomed_scale = scale * zoom;
        let page = |slide: &ibis_doc_model::Slide| {
            // The slide coming in plays its animations; the one before is
            // shown as it was left (all played).
            let step = if slide.id == current_id { show.step } else { usize::MAX };
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .overflow_hidden()
                .bg(white())
                .child(slide_backdrop(slide, zoomed_scale, &self.resources))
                .children(animated_shapes(slide, size, zoomed_scale, step, show.played, &show.triggered_played, |shape| {

                    if let ShapeKind::Picture(pic) = &shape.kind {
                        if let Some(ref clip) = pic.clip {
                            if clip.audio && clip.options.hide_during_show {
                                return div().into_any_element();
                            }
                            let playing = self.pptx().is_some_and(|st| st.clips.iter().any(|c| c.shape == shape.id));
                            if !clip.audio && clip.options.hide_while_not_playing && !playing {
                                return div().into_any_element();
                            }
                            if !clip.audio && clip.options.full_screen && playing {
                                let full_frame = ibis_doc_model::EmuRect::new(0, 0, size.width, size.height);
                                return shape_element(shape, full_frame, zoomed_scale, &self.resources, None);
                            }
                        }
                    }
                    shape_element(shape, shape.frame, zoomed_scale, &self.resources, None)
                }))
        };
        let before = show.came_from.and_then(|id| self.presentation().slide(id));
        let transition = slide.transition.as_ref().filter(|t| !t.effect.is_empty() && t.effect != "cut" && t.duration > 0);
        let shown: AnyElement = match (before, transition) {
            // 変形: the shapes both slides have move from the one to the other.
            (Some(old), Some(t)) if t.effect == "morph" => {
                let elapsed = {
                    let mut clock = show.morph_clock.borrow_mut();
                    if clock.as_ref().is_none_or(|(changes, _)| *changes != show.changes) {
                        *clock = Some((show.changes, crate::clock::Instant::now()));
                    }
                    clock.as_ref().map_or(0.0, |(_, at)| at.elapsed().as_secs_f32() * 1000.0)
                };
                let progress = (elapsed / t.duration.max(1) as f32).min(1.0);
                if progress < 1.0 {
                    window.request_animation_frame();
                    self.morph_page(old, slide, ease_in_out(progress), zoomed_scale)
                } else {
                    page(slide).into_any_element()
                }
            }
            (Some(_), Some(t)) => {
                let animation = Animation::new(std::time::Duration::from_millis(t.duration as u64)).with_easing(ease_in_out);
                // Push, cover, wipe and pull slide in from their side;
                // the others fade in.
                let from = match (t.effect.as_str(), t.direction.as_deref()) {
                    ("push" | "cover" | "wipe" | "pull", Some("l")) => Some((-zw, 0.0)),
                    ("push" | "cover" | "wipe" | "pull", Some("r")) => Some((zw, 0.0)),
                    ("push" | "cover" | "wipe" | "pull", Some("u")) => Some((0.0, -zh)),
                    ("push" | "cover" | "wipe" | "pull", Some("d")) => Some((0.0, zh)),
                    ("push", _) => Some((0.0, zh)),
                    ("cover" | "wipe" | "pull", _) => Some((zw, 0.0)),
                    _ => None,
                };
                page(slide)
                    .with_animation(("slide-in", show.changes), animation, move |el, t| match from {
                        Some((dx, dy)) => el.left(px(dx * (1.0 - t))).top(px(dy * (1.0 - t))),
                        None => el.opacity(t),
                    })
                    .into_any_element()
            }
            _ => page(slide).into_any_element(),
        };
        let inks = show.slide_inks.get(&current_id).cloned().unwrap_or_default();
        let active = show.active_stroke.clone();
        let laser = show.laser_pos;
        let ink_canvas = canvas(
            |_bounds, _window, _cx| {},
            move |bounds, _window, window, _cx| {
                let ox = bounds.origin.x;
                let oy = bounds.origin.y;
                for stroke in &inks {
                    paint_ink_stroke(window, stroke, ox, oy, zw, zh);
                }
                if let Some(ref stroke) = active {
                    paint_ink_stroke(window, stroke, ox, oy, zw, zh);
                }
                if let Some(laser_pt) = laser {
                    paint_laser_pointer(window, laser_pt, ox, oy, zw, zh);
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        Some(
            root.child(
                div()
                    .relative()
                    .flex_none()
                    .w(px(w))
                    .h(px(h))
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .left(px(ox))
                            .top(px(oy))
                            .w(px(zw))
                            .h(px(zh))
                            .children(before.filter(|_| transition.is_some()).map(page))
                            .child(shown)
                            .child(ink_canvas),
                    ),
            )
            .when(!audience, |d| d.child(self.pointer_toolbar(show, cx)))
            .when(show.reading_view, |d| d.child(self.reading_view_toolbar(show, cx)))
            .when_some(show.rehearsal.as_ref().filter(|r| !r.dialog_open), |d, r| d.child(self.rehearsal_toolbar(r, cx)))
            .when_some(show.rehearsal.as_ref().filter(|r| r.dialog_open), |d, r| d.child(self.rehearsal_dialog(r, cx)))
            .when(show.ink_dialog_open, |d| d.child(self.ink_dialog(cx)))
            .into_any_element(),
        )
    }

    /// The presenter's view: the slide shown (with its transition shown as
    /// a fade), the next slide, the notes, the clock and the number.
    fn presenter_view(&self, show: &Slideshow, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let view = window.viewport_size();
        let (vw, vh) = (f32::from(view.width), f32::from(view.height));
        let presentation = self.presentation();
        let slides = &presentation.slides;
        let index = presentation.slide_index(self.current_slide).unwrap_or(0);
        let next = slides.iter().skip(index + 1).find(|s| !s.hidden);
        let shown_count = slides.iter().filter(|s| !s.hidden).count();
        let number = slides[..=index.min(slides.len().saturating_sub(1))].iter().filter(|s| !s.hidden).count();
        // A slide's picture `width` pixels wide.
        let picture = |slide: &ibis_doc_model::Slide, width: f32| {
            let size = presentation.size_of(slide);
            let scale = width / size.width.max(1) as f32;
            div()
                .relative()
                .flex_none()
                .w(px(width))
                .h(px(size.height as f32 * scale))
                .overflow_hidden()
                .bg(white())
                .child(slide_backdrop(slide, scale, &self.resources))
                .children(slide.shown_shapes().map(|shape| shape_element(shape, shape.frame, scale, &self.resources, None)))
        };
        let elapsed = if let Some(paused) = show.paused_elapsed {
            paused.as_secs()
        } else {
            (show.accumulated + show.started.elapsed()).as_secs()
        };
        let is_paused = show.paused_elapsed.is_some();
        let clock = format!("{}:{:02}:{:02}", elapsed / 3600, elapsed / 60 % 60, elapsed % 60);
        let label = |text: String| div().text_size(px(14.)).text_color(rgb(0xc0c0c0)).child(text);
        let current: AnyElement = if show.at_end {
            div().w(px(vw * 0.6)).h(px(vw * 0.6 * 9.0 / 16.0)).bg(black()).text_color(rgb(END_TEXT_COLOR)).p_4().child(crate::shell::i18n::text(END_TEXT)).into_any_element()
        } else if let Some(color) = show.blank {
            div().w(px(vw * 0.6)).h(px(vw * 0.6 * 9.0 / 16.0)).bg(rgb(color)).into_any_element()
        } else {
            picture(self.slide(), vw * 0.6).into_any_element()
        };
        let notes = self.slide().notes.clone();
        // The notes start from the top when another slide comes.
        if show.notes_slide.get() != Some(self.current_slide) {
            show.notes_slide.set(Some(self.current_slide));
            show.notes_scroll.set_offset(point(px(0.), px(0.)));
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x202124))
            .p_4()
            .gap_3()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().id("presenter-clock").text_size(px(28.)).text_color(white()).child(clock))
                            .child(
                                div()
                                    .id("presenter-timer-pause")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .bg(rgb(0x3c4043))
                                    .cursor_pointer()
                                    .text_size(px(13.))
                                    .text_color(white())
                                    .child(if is_paused { crate::shell::i18n::text("slideshow.rehearsal.resume") } else { crate::shell::i18n::text("slideshow.rehearsal.pause") })
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .on_click(cx.listener(|this, _, _, cx| this.toggle_presenter_timer_pause(cx))),
                            )
                            .child(
                                div()
                                    .id("presenter-timer-reset")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .bg(rgb(0x3c4043))
                                    .cursor_pointer()
                                    .text_size(px(13.))
                                    .text_color(white())
                                    .child(crate::shell::i18n::text("pptx.slideshow.reset"))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .on_click(cx.listener(|this, _, _, cx| this.reset_presenter_timer(cx))),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .items_center()
                            .children(show.external.is_some().then(|| {
                                div()
                                    .id("presenter-swap")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .bg(rgb(0x3c4043))
                                    .cursor_pointer()
                                    .text_size(px(14.))
                                    .text_color(white())
                                    .child(crate::shell::i18n::text("pptx.slideshow.swap-displays"))
                                    // Not a click on the slide (which goes on).
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .on_click(cx.listener(|this, _, _, cx| this.swap_show_displays(cx)))
                            }))
                            .child(
                                div()
                                    .id("presenter-all-slides")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .bg(if show.grid_open { rgb(0x4a4d52) } else { rgb(0x3c4043) })
                                    .cursor_pointer()
                                    .text_size(px(14.))
                                    .text_color(white())
                                    .child(crate::shell::i18n::text("pptx.slideshow.see-all-slides"))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .on_click(cx.listener(|this, _, _, cx| this.toggle_slideshow_grid(cx))),
                            )
                            .child(
                                div()
                                    .id("presenter-zoom-in")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .bg(rgb(0x3c4043))
                                    .cursor_pointer()
                                    .text_size(px(14.))
                                    .text_color(white())
                                    .child(crate::shell::i18n::text("_shell.zoom-in"))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .on_click(cx.listener(|this, _, _, cx| this.zoom_slideshow(1.25, cx))),
                            )
                            .child(
                                div()
                                    .id("presenter-zoom-reset")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.))
                                    .bg(if show.zoom > 1.001 { rgb(0x4a4d52) } else { rgb(0x3c4043) })
                                    .cursor_pointer()
                                    .text_size(px(14.))
                                    .text_color(white())
                                    .child(crate::shell::i18n::text("_pptx.100"))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .on_click(cx.listener(|this, _, _, cx| this.reset_slideshow_zoom(cx))),
                            )
                            .child(label(format!("{} {number} / {shown_count}", crate::shell::i18n::text("_shell.slide")))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(current)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(label(crate::shell::i18n::text("pptx.links.next-slide").to_string()))
                            .children(next.map(|s| picture(s, vw * 0.3)))
                            .when(next.is_none(), |d| d.child(label(crate::shell::i18n::text("pptx.slideshow.last-slide").to_string()))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(notes_button("presenter-notes-smaller", crate::shell::i18n::text("pptx.slideshow.notes-smaller").to_string(), false, cx.listener(|this, _, _, cx| this.change_notes_size(-2.0, cx))))
                    .child(label(format!("{} pt", show.notes_size)))
                    .child(notes_button("presenter-notes-larger", crate::shell::i18n::text("pptx.slideshow.notes-larger").to_string(), false, cx.listener(|this, _, _, cx| this.change_notes_size(2.0, cx))))
                    .child(notes_button("presenter-teleprompter", crate::shell::i18n::text("pptx.slideshow.teleprompter").to_string(), show.teleprompter.is_some(), cx.listener(|this, _, w, cx| this.toggle_teleprompter(w, cx))))
                    .children(show.teleprompter.map(|speed| {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(notes_button("presenter-teleprompter-slower", "−".to_string(), false, cx.listener(|this, _, _, cx| this.change_teleprompter_speed(-5.0, cx))))
                            .child(label(crate::shell::i18n::format("pptx.slideshow.teleprompter-speed", &[("n", &speed)])))
                            .child(notes_button("presenter-teleprompter-faster", "+".to_string(), false, cx.listener(|this, _, _, cx| this.change_teleprompter_speed(5.0, cx))))
                    })),
            )
            .child(
                div()
                    .id("presenter-notes")
                    .flex_1()
                    .min_h(px(vh * 0.1))
                    .overflow_y_scroll()
                    .track_scroll(&show.notes_scroll)
                    .text_size(px(show.notes_size))
                    .text_color(white())
                    .child(if notes.is_empty() { crate::shell::i18n::text("pptx.slideshow.no-notes").to_string() } else { notes }),
            )
            .into_any_element()
    }

    /// Overview grid of all slides in the presentation.
    fn slideshow_grid_view(&self, _show: &Slideshow, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let pres = self.presentation();
        let slides = &pres.slides;
        let current_id = self.current_slide;
        let card_w = 240.0;

        div()
            .id("slideshow-grid-view")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x18191c))
            .text_color(white())
            .child(
                // Header bar
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_6()
                    .py_4()
                    .border_b_1()
                    .border_color(rgb(0x2d3035))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .id("slideshow-grid-close")
                                    .p_1()
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .bg(rgb(0x282a2d))
                                    .hover(|s| s.bg(rgb(0x3c4043)))
                                    .child(Icon::new(Material("arrow_left")).size(px(18.)).text_color(white()))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .on_click(cx.listener(|this, _, _, cx| this.toggle_slideshow_grid(cx))),
                            )
                            .child(
                                div()
                                    .text_size(px(18.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(crate::shell::i18n::text("pptx.slideshow.see-all-slides")),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(0x9aa0a6))
                            .child(format!("{} / {}", slides.len(), crate::shell::i18n::text("_shell.slide"))),
                    ),
            )
            .child(
                // Scrollable grid area
                div()
                    .id("slideshow-grid-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_6()
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_6()
                            .justify_center()
                            .children(slides.iter().enumerate().map(|(index, slide)| {
                                let slide_id = slide.id;
                                let is_current = slide_id == current_id;
                                let is_hidden = slide.hidden;
                                let size = pres.size_of(slide);
                                let scale = card_w / size.width.max(1) as f32;
                                let card_h = size.height as f32 * scale;

                                div()
                                    .id(SharedString::from(format!("slideshow-grid-card-{}", index + 1)))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .cursor_pointer()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .px_1()
                                            .child(
                                                div()
                                                    .text_size(px(13.))
                                                    .text_color(if is_current { rgb(0x8ab4f8) } else { rgb(0xbdc1c6) })
                                                    .child(format!("{}.", index + 1)),
                                            )
                                            .when(is_hidden, |d| {
                                                d.child(
                                                    Icon::new(Material("visibility_off"))
                                                        .size(px(14.))
                                                        .text_color(rgb(0x80868b)),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .relative()
                                            .flex_none()
                                            .w(px(card_w))
                                            .h(px(card_h))
                                            .rounded(px(4.))
                                            .overflow_hidden()
                                            .bg(white())
                                            .border_2()
                                            .border_color(if is_current {
                                                rgb(0x8ab4f8)
                                            } else {
                                                rgb(0x3c4043)
                                            })
                                            .hover(|s| s.border_color(rgb(0xffffff)))
                                            .when(is_hidden, |d| d.opacity(0.5))
                                            .child(slide_backdrop(slide, scale, &self.resources))
                                            .children(slide.shown_shapes().map(|shape| {
                                                shape_element(shape, shape.frame, scale, &self.resources, None)
                                            })),
                                    )
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select_slide_in_grid(slide_id, window, cx);
                                    }))
                            })),
                    ),
            )
            .into_any_element()
    }

    /// Floating toolbar for slideshow navigation and pointer tools (pen, laser, etc.).
    fn pointer_toolbar(&self, show: &Slideshow, cx: &Context<Self>) -> impl IntoElement {
        let mode = show.pointer_mode;
        let btn = |id: &'static str, icon_name: &'static str, active: bool| {
            div()
                .id(id)
                .p_1()
                .rounded(px(4.))
                .cursor_pointer()
                .bg(if active { rgb(0x4a4d52) } else { rgb(0x282a2d) })
                .hover(|s| s.bg(rgb(0x3c4043)))
                .child(
                    Icon::new(Material(icon_name))
                        .size(px(16.))
                        .text_color(if active { rgb(0x8ab4f8) } else { rgb(0xffffff) }),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };

        div()
            .id("slideshow-pointer-toolbar")
            .absolute()
            .bottom(px(16.))
            .left(px(16.))
            .flex()
            .items_center()
            .gap_1()
            .p_1()
            .rounded(px(6.))
            .bg(rgb(0x202124))
            .shadow_md()
            .child(
                btn("slideshow-prev", "arrow_left", false)
                    .on_click(cx.listener(|this, _, window, cx| this.step_slideshow(false, window, cx))),
            )
            .child(
                btn("slideshow-next", "arrow_right", false)
                    .on_click(cx.listener(|this, _, window, cx| this.step_slideshow(true, window, cx))),
            )
            .child(
                btn("slideshow-arrow", "arrow_selector_tool", mode == PointerMode::Arrow)
                    .on_click(cx.listener(|this, _, _, cx| this.set_slideshow_pointer_mode(PointerMode::Arrow, cx))),
            )
            .child(
                btn("slideshow-laser", "center_focus_strong", mode == PointerMode::Laser)
                    .on_click(cx.listener(|this, _, _, cx| this.set_slideshow_pointer_mode(PointerMode::Laser, cx))),
            )
            .child(
                btn("slideshow-pen", "edit", mode == PointerMode::Pen)
                    .on_click(cx.listener(|this, _, _, cx| this.set_slideshow_pointer_mode(PointerMode::Pen, cx))),
            )
            .child(
                btn("slideshow-highlighter", "ink_highlighter", mode == PointerMode::Highlighter)
                    .on_click(cx.listener(|this, _, _, cx| this.set_slideshow_pointer_mode(PointerMode::Highlighter, cx))),
            )
            .child(
                btn("slideshow-eraser", "cleaning_services", mode == PointerMode::Eraser)
                    .on_click(cx.listener(|this, _, _, cx| this.set_slideshow_pointer_mode(PointerMode::Eraser, cx))),
            )
            .child(
                btn("slideshow-clear-inks", "delete_sweep", false)
                    .on_click(cx.listener(|this, _, _, cx| this.clear_current_slide_inks(cx))),
            )
            .child(
                btn("slideshow-grid", "grid_view", show.grid_open)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_slideshow_grid(cx))),
            )
            .child(
                btn("slideshow-zoom-in", "zoom_in", false)
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_slideshow(1.25, cx))),
            )
            .child(
                btn("slideshow-zoom-out", "zoom_out", false)
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_slideshow(0.8, cx))),
            )
            .child(
                btn("slideshow-zoom-reset", "fit_screen", show.zoom > 1.001)
                    .on_click(cx.listener(|this, _, _, cx| this.reset_slideshow_zoom(cx))),
            )
            .child(
                btn("slideshow-close", "close", false)
                    .on_click(cx.listener(|this, _, window, cx| this.request_end_slideshow(window, cx))),
            )
    }

    /// Bottom right reading view toolbar: slide index and buttons to step or return to normal view.
    fn reading_view_toolbar(&self, _show: &Slideshow, cx: &Context<Self>) -> impl IntoElement {
        let total = self.presentation().slides.len();
        let current = self.presentation().slide_index(self.current_slide).map_or(1, |i| i + 1);
        let count_text = crate::shell::i18n::format("pptx.reading.slide-count", &[("current", &current), ("total", &total)]);

        let btn = |id: &'static str, icon_name: &'static str| {
            div()
                .id(id)
                .p_1()
                .rounded(px(4.))
                .cursor_pointer()
                .bg(rgb(0x282a2d))
                .hover(|s| s.bg(rgb(0x3c4043)))
                .child(
                    Icon::new(Material(icon_name))
                        .size(px(16.))
                        .text_color(rgb(0xffffff)),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };

        div()
            .id("slideshow-reading-toolbar")
            .absolute()
            .bottom(px(16.))
            .right(px(16.))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .rounded(px(6.))
            .bg(rgb(0x202124))
            .shadow_md()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(0xcccccc))
                    .child(count_text),
            )
            .child(
                btn("reading-prev", "arrow_left")
                    .on_click(cx.listener(|this, _, window, cx| this.step_slideshow(false, window, cx))),
            )
            .child(
                btn("reading-next", "arrow_right")
                    .on_click(cx.listener(|this, _, window, cx| this.step_slideshow(true, window, cx))),
            )
            .child(
                btn("reading-exit-normal", "view_sidebar")
                    .on_click(cx.listener(|this, _, window, cx| this.request_end_slideshow(window, cx))),
            )
    }

    fn rehearsal_toolbar(&self, rehearsal: &Rehearsal, cx: &mut Context<Self>) -> AnyElement {
        let is_paused = rehearsal.slide_paused.is_some();
        let slide_secs = rehearsal.slide_elapsed().as_secs();
        let slide_time = format!("{}:{:02}:{:02}", slide_secs / 3600, (slide_secs / 60) % 60, slide_secs % 60);
        let total_secs = rehearsal.total_elapsed().as_secs();
        let total_time = format!("{}:{:02}:{:02}", total_secs / 3600, (total_secs / 60) % 60, total_secs % 60);

        div()
            .id("rehearsal-toolbar")
            .absolute()
            .top_4()
            .left_4()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .rounded(px(6.))
            .bg(rgba(0x202124f0))
            .border_1()
            .border_color(rgb(0x3c4043))
            .shadow_md()
            .text_color(white())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0x9aa0a6))
                    .child(crate::shell::i18n::text("slideshow.rehearse-timings")),
            )
            .child(
                div()
                    .id("rehearsal-next")
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .bg(rgb(0x3c4043))
                    .hover(|s| s.bg(rgb(0x4a4d52)))
                    .child(Icon::new(Material("arrow_forward")).size(px(16.)).text_color(white()))
                    .on_click(cx.listener(|this, _, window, cx| this.step_slideshow(true, window, cx))),
            )
            .child(
                div()
                    .id("rehearsal-pause")
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .bg(if is_paused { rgb(0x8ab4f8) } else { rgb(0x3c4043) })
                    .hover(|s| s.bg(rgb(0x4a4d52)))
                    .text_size(px(12.))
                    .text_color(if is_paused { rgb(0x202124) } else { rgb(0xffffff) })
                    .child(if is_paused {
                        crate::shell::i18n::text("slideshow.rehearsal.resume")
                    } else {
                        crate::shell::i18n::text("slideshow.rehearsal.pause")
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_rehearsal_pause(cx))),
            )
            .child(
                div()
                    .id("rehearsal-slide-time")
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .bg(rgb(0x18191c))
                    .border_1()
                    .border_color(rgb(0x3c4043))
                    .text_size(px(13.))
                    .font_family(".AppleSystemUIFontMonospaced")
                    .text_color(white())
                    .child(slide_time),
            )
            .child(
                div()
                    .id("rehearsal-repeat")
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .bg(rgb(0x3c4043))
                    .hover(|s| s.bg(rgb(0x4a4d52)))
                    .text_size(px(12.))
                    .text_color(white())
                    .child(crate::shell::i18n::text("slideshow.rehearsal.repeat"))
                    .on_click(cx.listener(|this, _, _, cx| this.repeat_rehearsal_slide(cx))),
            )
            .child(
                div()
                    .id("rehearsal-total-time")
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .bg(rgb(0x18191c))
                    .border_1()
                    .border_color(rgb(0x3c4043))
                    .text_size(px(13.))
                    .font_family(".AppleSystemUIFontMonospaced")
                    .text_color(white())
                    .child(total_time),
            )
            .into_any_element()
    }

    fn rehearsal_dialog(&self, rehearsal: &Rehearsal, cx: &mut Context<Self>) -> AnyElement {
        let total_secs = rehearsal.total_elapsed().as_secs();
        let total_time = format!("{}:{:02}:{:02}", total_secs / 3600, (total_secs / 60) % 60, total_secs % 60);
        let prompt_text = crate::shell::i18n::format("slideshow.rehearsal.prompt", &[("time", &total_time)]);

        div()
            .id("rehearsal-dialog")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000088))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(480.))
                    .p_6()
                    .rounded(px(8.))
                    .bg(rgb(0x282a2d))
                    .border_1()
                    .border_color(rgb(0x3c4043))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .text_color(white())
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(crate::shell::i18n::text("slideshow.rehearse-timings")),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .line_height(relative(1.4))
                            .text_color(rgb(0xbdc1c6))
                            .child(prompt_text),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_3()
                            .child(
                                div()
                                    .id("rehearsal-dialog-save")
                                    .px_4()
                                    .py_2()
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .bg(rgb(0x1a73e8))
                                    .hover(|s| s.bg(rgb(0x1b66c9)))
                                    .text_size(px(13.))
                                    .text_color(white())
                                    .child(crate::shell::i18n::text("slideshow.rehearsal.yes"))
                                    .on_click(cx.listener(|this, _, window, cx| this.save_rehearsal_timings(window, cx))),
                            )
                            .child(
                                div()
                                    .id("rehearsal-dialog-discard")
                                    .px_4()
                                    .py_2()
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .bg(rgb(0x3c4043))
                                    .hover(|s| s.bg(rgb(0x4a4d52)))
                                    .text_size(px(13.))
                                    .text_color(white())
                                    .child(crate::shell::i18n::text("slideshow.rehearsal.no"))
                                    .on_click(cx.listener(|this, _, window, cx| this.discard_rehearsal_timings(window, cx))),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Dialog asking whether to keep ink annotations drawn during the slideshow.
    fn ink_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("ink-dialog")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000088))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(480.))
                    .p_6()
                    .rounded(px(8.))
                    .bg(rgb(0x282a2d))
                    .border_1()
                    .border_color(rgb(0x3c4043))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .text_color(white())
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(crate::shell::i18n::text("slideshow.ink.title")),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .line_height(relative(1.4))
                            .text_color(rgb(0xbdc1c6))
                            .child(crate::shell::i18n::text("slideshow.ink.prompt")),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_3()
                            .child(
                                div()
                                    .id("ink-dialog-keep")
                                    .px_4()
                                    .py_2()
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .bg(rgb(0x1a73e8))
                                    .hover(|s| s.bg(rgb(0x1b66c9)))
                                    .text_size(px(13.))
                                    .text_color(white())
                                    .child(crate::shell::i18n::text("slideshow.ink.keep"))
                                    .on_click(cx.listener(|this, _, window, cx| this.end_slideshow_with_ink_choice(true, window, cx))),
                            )
                            .child(
                                div()
                                    .id("ink-dialog-discard")
                                    .px_4()
                                    .py_2()
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .bg(rgb(0x3c4043))
                                    .hover(|s| s.bg(rgb(0x4a4d52)))
                                    .text_size(px(13.))
                                    .text_color(white())
                                    .child(crate::shell::i18n::text("slideshow.ink.discard"))
                                    .on_click(cx.listener(|this, _, window, cx| this.end_slideshow_with_ink_choice(false, window, cx))),
                            ),
                    ),
            )
            .into_any_element()
    }
}

/// A group of effects played together: those started by one click (or,
/// `auto`, by the slide coming in), each with when it begins and ends
/// after the group starts (milliseconds).
pub(crate) struct AnimationGroup {
    pub(crate) auto: bool,
    pub(crate) effects: Vec<(usize, u32, u32)>,
}

/// A slide's effects in groups, timed as PowerPoint times them: 直前の動作と
/// 同時 starts with the effect before, 直前の動作の後 after everything before
/// it in the group has ended.
pub(crate) fn animation_groups(animations: &[ibis_doc_model::Animation]) -> Vec<AnimationGroup> {
    use ibis_doc_model::AnimationStart;
    let mut groups: Vec<AnimationGroup> = Vec::new();
    let (mut previous_begin, mut ended) = (0u32, 0u32);
    for (index, a) in animations.iter().enumerate() {
        if a.is_triggered() {
            continue;
        }
        let start = match (a.start, groups.last_mut()) {
            (AnimationStart::OnClick, _) | (_, None) => {
                groups.push(AnimationGroup { auto: a.start != AnimationStart::OnClick && groups.is_empty(), effects: Vec::new() });
                ended = 0;
                0
            }
            (AnimationStart::WithPrevious, Some(_)) => previous_begin,
            (AnimationStart::AfterPrevious, Some(_)) => ended,
        };
        let repeat_factor = match a.repeat_count {
            ibis_doc_model::AnimationRepeat::None => 1,
            ibis_doc_model::AnimationRepeat::Count(n) => n.max(1),
            ibis_doc_model::AnimationRepeat::UntilNextClick | ibis_doc_model::AnimationRepeat::UntilEndOfSlide => 1,
        };
        let anim_dur = a.duration * repeat_factor;
        let begin = start + a.delay;
        let end = begin + anim_dur;
        previous_begin = start;
        ended = ended.max(end);
        groups.last_mut().expect("a group").effects.push((index, begin, end));
    }
    groups
}

/// How a shape looks at a moment of its animations: whether it shows, how
/// opaque, and how far it is moved (pixels).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShapeMotion {
    pub(crate) opacity: f32,
    pub(crate) dx: f32,
    pub(crate) dy: f32,
    /// How far it is turned (degrees, clockwise) by the emphasis effects スピン.
    pub(crate) turn: f32,
    /// How much larger it is across and down (1 is its size) by 拡大／収縮.
    pub(crate) grow: (f32, f32),
}

impl ShapeMotion {
    /// Shown, in its place, not turned, at its size.
    pub(crate) const fn at_rest(opacity: f32) -> Self {
        Self { opacity, dx: 0.0, dy: 0.0, turn: 0.0, grow: (1.0, 1.0) }
    }

    /// Whether the shape has to be drawn turned or resized.
    fn transforms(&self) -> bool {
        self.turn != 0.0 || self.grow != (1.0, 1.0)
    }
}

/// `shape` turned and resized (about its centre) as `motion` says.
pub(crate) fn transformed(shape: &ibis_doc_model::Shape, motion: ShapeMotion) -> ibis_doc_model::Shape {
    let mut s = shape.clone();
    if motion.turn != 0.0 {
        s.rotation = (f64::from(s.rotation) + f64::from(motion.turn) * 60_000.0).round().rem_euclid(21_600_000.0) as i32;
    }
    if motion.grow != (1.0, 1.0) {
        let f = shape.frame;
        let (w, h) = ((f.width as f64 * f64::from(motion.grow.0)).round() as i64, (f.height as f64 * f64::from(motion.grow.1)).round() as i64);
        s.frame = ibis_doc_model::EmuRect::new(f.x + (f.width - w) / 2, f.y + (f.height - h) / 2, w, h);
        // The text grows with the shape (by the mean of the two factors).
        let text = match &mut s.kind {
            ibis_doc_model::ShapeKind::Auto(auto) => auto.text.as_mut(),
            ibis_doc_model::ShapeKind::TextBox(body) => Some(body),
            _ => None,
        };
        if let Some(body) = text {
            body.font_scale = (f64::from(body.font_scale) * f64::from(motion.grow.0 + motion.grow.1) / 2.0).round() as u32;
        }
    }
    s
}

/// The slide's shapes, each as `element` draws it, moved and faded by its
/// animations: the groups before `step` played, the last of them playing
/// (`played` keys its animation).
pub(crate) fn animated_shapes<'a>(
    slide: &'a ibis_doc_model::Slide,
    size: ibis_doc_model::EmuSize,
    scale: f32,
    step: usize,
    played: usize,
    triggered_played: &'a [usize],
    element: impl Fn(&ibis_doc_model::Shape) -> AnyElement + 'a,
) -> impl Iterator<Item = AnyElement> + 'a {
    let groups = animation_groups(&slide.animations);
    let step = step.min(groups.len());
    slide.shown_shapes().flat_map(move |shape| {
        // SmartArt diagram frame itself draws nothing
        if matches!(shape.kind, ibis_doc_model::ShapeKind::Placeholder(ibis_doc_model::Placeholder::Diagram)) {
            return vec![];
        }
        // Attached shapes of a SmartArt diagram
        if let Some((&parent_id, children)) = slide.attached.iter().find(|(_, children)| children.contains(&shape.id)) {
            let dgm_anims: Vec<&ibis_doc_model::Animation> = slide.animations.iter().filter(|a| a.shape == parent_id).collect();
            if !dgm_anims.is_empty() {
                let id_str = shape.id.0.to_string();
                let idx_str = children.iter().position(|&sid| sid == shape.id).map(|idx| idx.to_string());
                let idx1_str = children.iter().position(|&sid| sid == shape.id).map(|idx| (idx + 1).to_string());

                let matched_elem_id = dgm_anims.iter().filter_map(|a| a.dgm_element.as_ref()).find_map(|d| {
                    if d.id == id_str
                        || idx_str.as_ref() == Some(&d.id)
                        || idx1_str.as_ref() == Some(&d.id)
                    {
                        Some(d.id.clone())
                    } else {
                        None
                    }
                });

                let target = if let Some(element_id) = matched_elem_id {
                    Target::DiagramElement { parent_id, element_id }
                } else if dgm_anims.iter().any(|a| a.dgm_element.is_none()) {
                    Target::DiagramWhole { parent_id }
                } else {
                    Target::Whole
                };
                return vec![animated_view(slide, shape, target, (size, scale, step, played, triggered_played), &groups, &element)];
            }
        }
        let paragraphs = animated_paragraphs(slide, shape.id);
        if paragraphs.is_empty() {
            let chart_parts = animated_chart_elements(slide, shape);
            if !chart_parts.is_empty() {
                let has_bg_anim = chart_parts.contains(&(-1, -1));
                let bg_shape = chart_element_view(shape, -1, -1);
                let mut views = Vec::new();
                if has_bg_anim {
                    views.push(animated_view(slide, &bg_shape, Target::ChartElement { series_idx: -1, category_idx: -1 }, (size, scale, step, played, triggered_played), &groups, &element));
                } else {
                    views.push(animated_view(slide, &bg_shape, Target::Whole, (size, scale, step, played, triggered_played), &groups, &element));
                }
                for &(s_idx, c_idx) in &chart_parts {
                    if s_idx == -1 && c_idx == -1 {
                        continue;
                    }
                    let part_shape = chart_element_view(shape, s_idx, c_idx);
                    views.push(animated_view(slide, &part_shape, Target::ChartElement { series_idx: s_idx, category_idx: c_idx }, (size, scale, step, played, triggered_played), &groups, &element));
                }
                return views;
            }
            let parts = animated_parts(slide, shape);
            if parts.is_empty() {
                return vec![animated_view(slide, shape, Target::Whole, (size, scale, step, played, triggered_played), &groups, &element)];
            }
            // The text taken in parts (characters, words, letters): the shape with those characters transparent, and each part over
            // it, alone, with its own effects.
            let base = chars_view(shape, &|c| parts.iter().any(|(_, (first, last))| (*first..=*last).contains(&c)), false);
            let mut views = vec![animated_view(slide, &base, Target::Whole, (size, scale, step, played, triggered_played), &groups, &element)];
            for (target, (first, last)) in parts {
                let only = chars_view(shape, &|c| !(first..=last).contains(&c), true);
                views.push(animated_view(slide, &only, target, (size, scale, step, played, triggered_played), &groups, &element));
            }
            return views;
        }
        // The text of a shape whose paragraphs come one by one: the shape
        // with those paragraphs transparent, and each of them over it, alone,
        // with its own effects.
        let base = paragraph_view(shape, &|i| paragraphs.contains(&i), false);
        let mut views = vec![animated_view(slide, &base, Target::Whole, (size, scale, step, played, triggered_played), &groups, &element)];
        for &i in &paragraphs {
            let only = paragraph_view(shape, &|j| j != i, true);
            views.push(animated_view(slide, &only, Target::Paragraph(i), (size, scale, step, played, triggered_played), &groups, &element));
        }
        views
    })
}

/// The chart element targets (`seriesIdx`, `categoryIdx`) of shape `shape` that effects move, in order.
fn animated_chart_elements(slide: &ibis_doc_model::Slide, shape: &ibis_doc_model::Shape) -> Vec<(i32, i32)> {
    if !matches!(shape.kind, ibis_doc_model::ShapeKind::Chart(_)) {
        return Vec::new();
    }
    let mut found = Vec::new();
    for a in &slide.animations {
        if a.shape == shape.id {
            if let Some(el) = a.chart_element {
                let pair = (el.series_idx, el.category_idx);
                if !found.contains(&pair) {
                    found.push(pair);
                }
            }
        }
    }
    found
}

/// A view of `shape` (a chart) showing either only the background (axes, title, legend, plot area without series)
/// or only the specified series/category elements.
pub(crate) fn chart_element_view(shape: &ibis_doc_model::Shape, series_idx: i32, category_idx: i32) -> ibis_doc_model::Shape {
    use ibis_doc_model::{Fill, ShapeKind};
    let mut shape = shape.clone();
    if let ShapeKind::Chart(chart) = &mut shape.kind {
        if series_idx == -1 && category_idx == -1 {
            // Background only: clear all series data points
            for g in &mut chart.groups {
                g.series.clear();
            }
        } else {
            // Element / series only: remove chart background, border, title, axes, legend
            chart.background = Fill::None;
            chart.plot_background = Fill::None;
            chart.border = None;
            chart.title = None;
            chart.axes.clear();
            chart.legend = None;
            if series_idx >= 0 && category_idx == -1 {
                // Keep only series `series_idx`
                let mut current_idx = 0;
                for g in &mut chart.groups {
                    g.series.retain(|_| {
                        let keep = current_idx == series_idx as usize;
                        current_idx += 1;
                        keep
                    });
                }
            } else if series_idx == -1 && category_idx >= 0 {
                // Keep only category `category_idx` in each series
                let cat = category_idx as usize;
                for g in &mut chart.groups {
                    for s in &mut g.series {
                        for (ci, val) in s.values.iter_mut().enumerate() {
                            if ci != cat {
                                *val = None;
                            }
                        }
                    }
                }
            } else if series_idx >= 0 && category_idx >= 0 {
                // Keep series `series_idx` and category `category_idx`
                let mut current_idx = 0;
                let cat = category_idx as usize;
                for g in &mut chart.groups {
                    g.series.retain_mut(|s| {
                        let keep = current_idx == series_idx as usize;
                        current_idx += 1;
                        if keep {
                            for (ci, val) in s.values.iter_mut().enumerate() {
                                if ci != cat {
                                    *val = None;
                                }
                            }
                        }
                        keep
                    });
                }
            }
        }
    }
    shape
}

/// Which effects of a shape move what is drawn: the shape as a whole, one of its paragraphs, a range of its characters, or one
/// word or letter of an effect that goes by words or letters (that effect's, `offset` ms later than the first), or a chart element.
#[derive(Clone, PartialEq, Eq)]
enum Target {
    Whole,
    Paragraph(usize),
    Chars(u32, u32),
    Unit { effect: usize, first: u32, offset: u32 },
    ChartElement { series_idx: i32, category_idx: i32 },
    DiagramElement { parent_id: ibis_doc_model::ShapeId, element_id: String },
    DiagramWhole { parent_id: ibis_doc_model::ShapeId },
}

impl Target {
    /// Whether animation `index` (`a`) moves this of shape `shape`.
    fn moves(&self, index: usize, a: &ibis_doc_model::Animation, shape: ibis_doc_model::ShapeId) -> bool {
        match self {
            Target::Whole => {
                a.shape == shape
                    && a.paragraph.is_none()
                    && a.chars.is_none()
                    && a.text_unit.is_none()
                    && a.chart_element.is_none()
                    && a.dgm_element.is_none()
            }
            Target::Paragraph(i) => a.shape == shape && a.paragraph.is_some_and(|(first, last)| (first as usize..=last as usize).contains(i)),
            Target::Chars(first, last) => a.shape == shape && a.chars == Some((*first, *last)),
            Target::Unit { effect, .. } => a.shape == shape && index == *effect,
            Target::ChartElement { series_idx, category_idx } => {
                a.shape == shape && a.chart_element == Some(ibis_doc_model::ChartAnimationElement { series_idx: *series_idx, category_idx: *category_idx })
            }
            Target::DiagramElement { parent_id, element_id } => {
                a.shape == *parent_id && a.dgm_element.as_ref().is_some_and(|d| &d.id == element_id)
            }
            Target::DiagramWhole { parent_id } => {
                a.shape == *parent_id && a.dgm_element.is_none()
            }
        }
    }

    /// How much later than the effect's own time this target's part of it plays (ms).
    fn offset(&self) -> u32 {
        match self {
            Target::Unit { offset, .. } => *offset,
            _ => 0,
        }
    }

    /// A number that tells this target from the others of its shape (for the key of its animation).
    fn key(&self) -> usize {
        match self {
            Target::Whole => 0,
            Target::Paragraph(i) => 7919 * (*i + 1),
            Target::Chars(first, last) => 104_729 * (*first as usize + 1) + *last as usize,
            Target::Unit { effect, first, .. } => 1_299_709 * (*effect + 1) + *first as usize,
            Target::ChartElement { series_idx, category_idx } => {
                10_000_019 + (*series_idx + 2) as usize * 1000 + (*category_idx + 2) as usize
            }
            Target::DiagramElement { parent_id, element_id } => {
                20_000_000 + (parent_id.0 as usize % 1000) * 10_000 + element_id.parse::<usize>().unwrap_or(0)
            }
            Target::DiagramWhole { parent_id } => {
                30_000_000 + parent_id.0 as usize
            }
        }
    }
}

/// The words or letters of `body`'s text, as (first, last) character indices (the text's paragraphs joined by one character each):
/// a word with the spaces after it, a letter alone; Japanese text goes a character at a time even by words.
pub(crate) fn text_units(body: &ibis_doc_model::TextBody, unit: ibis_doc_model::TextUnit) -> Vec<(u32, u32)> {
    let text: Vec<char> = body.paragraphs.iter().map(|p| p.text()).collect::<Vec<_>>().join("\n").chars().collect();
    let cjk = |c: char| c as u32 >= 0x2E80;
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if text[i].is_whitespace() {
            i += 1;
            continue;
        }
        let first = i;
        if unit == ibis_doc_model::TextUnit::Word && !cjk(text[i]) {
            while i < text.len() && !text[i].is_whitespace() && !cjk(text[i]) {
                i += 1;
            }
            while i < text.len() && text[i].is_whitespace() && text[i] != '\n' {
                i += 1;
            }
        } else {
            i += 1;
        }
        out.push((first as u32, i as u32 - 1));
    }
    out
}

/// The parts of shape `shape`'s text that effects play on their own: each character range, and each word or letter of the effects
/// that go by words or letters (with its delay from the first), as targets with the characters they hold.
fn animated_parts(slide: &ibis_doc_model::Slide, shape: &ibis_doc_model::Shape) -> Vec<(Target, (u32, u32))> {
    let mut parts: Vec<(Target, (u32, u32))> = Vec::new();
    for (index, a) in slide.animations.iter().enumerate().filter(|(_, a)| a.shape == shape.id && a.paragraph.is_none()) {
        if let Some((first, last)) = a.chars {
            if !parts.iter().any(|(t, _)| *t == Target::Chars(first, last)) {
                parts.push((Target::Chars(first, last), (first, last)));
            }
        } else if let (Some((unit, gap)), Some(body)) = (a.text_unit, shape.text_body()) {
            let gap_ms = match gap {
                ibis_doc_model::UnitGap::Percent(v) => (a.duration.max(1) as f32 * v as f32 / 100_000.0).round() as u32,
                ibis_doc_model::UnitGap::Millis(v) => v,
            };
            for (k, (first, last)) in text_units(body, unit).into_iter().enumerate() {
                parts.push((Target::Unit { effect: index, first, offset: gap_ms * k as u32 }, (first, last)));
            }
        }
    }
    parts
}

/// `shape` with the characters `hidden` says transparent; with `text_only`, nothing else of the shape drawn (the bullets only of the
/// paragraphs with characters shown).
/// The characters are counted over the text, its paragraphs one character apart.
pub(crate) fn chars_view(shape: &ibis_doc_model::Shape, hidden: &dyn Fn(u32) -> bool, text_only: bool) -> ibis_doc_model::Shape {
    use ibis_doc_model::{Fill, Run, ShapeKind};
    let mut shape = shape.clone();
    let hide = |text: &mut ibis_doc_model::TextBody| {
        let mut at = 0u32;
        for paragraph in &mut text.paragraphs {
            let mut runs: Vec<Run> = Vec::new();
            for run in &paragraph.runs {
                let mut current = String::new();
                let mut current_hidden = false;
                let flush = |text: &mut String, h: bool, runs: &mut Vec<Run>| {
                    if !text.is_empty() {
                        let mut style = run.style.clone();
                        style.invisible = style.invisible || h;
                        runs.push(Run { text: std::mem::take(text), style });
                    }
                };
                for c in run.text.chars() {
                    let h = hidden(at);
                    at += 1;
                    if h != current_hidden {
                        flush(&mut current, current_hidden, &mut runs);
                        current_hidden = h;
                    }
                    current.push(c);
                }
                flush(&mut current, current_hidden, &mut runs);
            }
            at += 1;
            paragraph.runs = runs;
            if text_only {
                // A paragraph with none of the characters shown has no bullet either.
                if paragraph.runs.iter().all(|r| r.style.invisible) {
                    paragraph.bullet = None;
                }
                paragraph.end_style.invisible = true;
            }
        }
    };
    match &mut shape.kind {
        ShapeKind::Auto(auto) => {
            if let Some(text) = &mut auto.text {
                hide(text);
            }
            if text_only {
                auto.fill = Fill::None;
                auto.stroke = None;
                (auto.shadow, auto.glow, auto.inner_shadow, auto.soft_edge, auto.reflection, auto.bevel) = (None, None, None, None, None, None);
            }
        }
        ShapeKind::TextBox(text) => hide(text),
        _ => {}
    }
    shape
}

/// The paragraphs of shape `shape` that effects move, in order.
fn animated_paragraphs(slide: &ibis_doc_model::Slide, shape: ibis_doc_model::ShapeId) -> Vec<usize> {
    let mut found: Vec<usize> = slide.animations.iter().filter(|a| a.shape == shape).filter_map(|a| a.paragraph).flat_map(|(first, last)| first as usize..=last as usize).collect();
    found.sort_unstable();
    found.dedup();
    found
}

/// `shape` with the paragraphs `hidden` says transparent (their text and
/// bullets); with `text_only`, nothing else of the shape drawn.
pub(crate) fn paragraph_view(shape: &ibis_doc_model::Shape, hidden: &dyn Fn(usize) -> bool, text_only: bool) -> ibis_doc_model::Shape {
    use ibis_doc_model::{Fill, ShapeKind};
    let mut shape = shape.clone();
    let hide = |text: &mut ibis_doc_model::TextBody| {
        for (i, paragraph) in text.paragraphs.iter_mut().enumerate() {
            if hidden(i) {
                for run in &mut paragraph.runs {
                    run.style.invisible = true;
                }
                paragraph.end_style.invisible = true;
                paragraph.bullet = None;
            }
        }
    };
    match &mut shape.kind {
        ShapeKind::Auto(auto) => {
            if let Some(text) = &mut auto.text {
                hide(text);
            }
            if text_only {
                auto.fill = Fill::None;
                auto.stroke = None;
                (auto.shadow, auto.glow, auto.inner_shadow, auto.soft_edge, auto.reflection, auto.bevel) = (None, None, None, None, None, None);
            }
        }
        ShapeKind::TextBox(text) => hide(text),
        _ => {}
    }
    shape
}

/// What the slide show draws of `shape` for `target`: as it is at `step`.
#[allow(clippy::too_many_arguments)]
fn animated_view(
    slide: &ibis_doc_model::Slide,
    shape: &ibis_doc_model::Shape,
    target: Target,
    (size, scale, step, played, triggered_played): (ibis_doc_model::EmuSize, f32, usize, usize, &[usize]),
    groups: &[AnimationGroup],
    element: &dyn Fn(&ibis_doc_model::Shape) -> AnyElement,
) -> AnyElement {
    {
        let mine: Vec<usize> = slide.animations.iter().enumerate().filter(|(i, a)| target.moves(*i, a, shape.id)).map(|(i, _)| i).collect();
        if mine.is_empty() {
            return element(shape);
        }
        let (w, h) = (size.width as f32 * scale, size.height as f32 * scale);
        let frame = (shape.frame.x as f32 * scale, shape.frame.y as f32 * scale, shape.frame.width as f32 * scale, shape.frame.height as f32 * scale);
        // Hidden before its first effect if that brings it in.
        let first = &slide.animations[mine[0]];
        let mut base = ShapeMotion {
            opacity: if first.class == ibis_doc_model::AnimationClass::Entrance {
                if first.is_triggered() && triggered_played.contains(&mine[0]) {
                    1.0
                } else {
                    0.0
                }
            } else {
                1.0
            },
            dx: 0.0,
            dy: 0.0,
            turn: 0.0,
            grow: (1.0, 1.0),
        };
        for group in groups.iter().take(step.saturating_sub(1)) {
            for &(i, _, _) in &group.effects {
                if target.moves(i, &slide.animations[i], shape.id) {
                    if !slide.animations[i].rewind {
                        base = motion_at(&slide.animations[i], base, 1.0, frame, (w, h));
                        if slide.animations[i].after_effect == ibis_doc_model::AnimationAfterEffect::HideOnNextClick {
                            base.opacity = 0.0;
                        }
                    }
                }
            }
        }
        // Previously triggered effects that have completed:
        let last_triggered = triggered_played.last().copied();
        for &i in triggered_played {
            if Some(i) == last_triggered {
                continue;
            }
            if let Some(a) = slide.animations.get(i).filter(|a| target.moves(i, a, shape.id)) {
                if !a.rewind {
                    base = motion_at(a, base, 1.0, frame, (w, h));
                    if a.after_effect == ibis_doc_model::AnimationAfterEffect::HideOnNextClick {
                        base.opacity = 0.0;
                    }
                }
            }
        }
        let mut playing: Vec<(ibis_doc_model::Animation, u32, u32)> = step
            .checked_sub(1)
            .and_then(|g| groups.get(g))
            .map(|g| g.effects.iter().filter(|(i, _, _)| target.moves(*i, &slide.animations[*i], shape.id)).map(|&(i, b, e)| (slide.animations[i].clone(), b + target.offset(), e + target.offset())).collect())
            .unwrap_or_default();
        // If the most recently triggered effect belongs to this shape, play it:
        if let Some(last_i) = last_triggered {
            if let Some(a) = slide.animations.get(last_i).filter(|a| target.moves(last_i, a, shape.id)) {
                playing.push((a.clone(), target.offset(), target.offset() + a.duration.max(1)));
            }
        }
        let place = move |el: Div, m: ShapeMotion| el.opacity(m.opacity).left(px(m.dx)).top(px(m.dy));
        // Turned or resized by an emphasis effect (スピン・拡大／収縮): the shape is drawn again, turned or resized, for each
        // moment instead of moved as it is drawn.
        let transforms = base.transforms() || mine.iter().any(|&i| matches!(slide.animations[i].look, ibis_doc_model::AnimationLook::Spin(_) | ibis_doc_model::AnimationLook::Scale(..) | ibis_doc_model::AnimationLook::Zoom));
        let holder_of = |m: ShapeMotion| div().absolute().top_0().left_0().size_full().child(if transforms { element(&transformed(shape, m)) } else { element(shape) });
        let holder = holder_of(base);
        if playing.is_empty() {
            return place(holder, base).into_any_element();
        }
        let total = playing.iter().map(|(_, _, e)| *e).max().unwrap_or(0).max(1);
        let state = move |ms: f32| {
            let mut m = base;
            for (a, begin, end) in &playing {
                let p = if ms <= *begin as f32 { 0.0 } else if *end <= *begin { 1.0 } else { ((ms - *begin as f32) / (*end - *begin) as f32).min(1.0) };
                let eased_p = apply_timing_easing(p, a.accel, a.decel);
                // An effect not begun yet leaves an entering shape hidden.
                m = if ms < *begin as f32 { m } else { motion_at(a, m, eased_p, frame, (w, h)) };
            }
            m
        };
        if transforms {
            const FRAMES: usize = 24;
            let key = played * 100_000 + (shape.id.0 as usize + target.key()) % 100_000;
            let mut stack = div().absolute().top_0().left_0().size_full();
            for k in 0..=FRAMES {
                let m = state(total as f32 * k as f32 / FRAMES as f32);
                let opacity = m.opacity;
                let frame = place(holder_of(m), ShapeMotion { opacity: 1.0, ..m });
                stack = stack.child(frame.with_animation(("shape-transform", key * 32 + k), Animation::new(std::time::Duration::from_millis(total as u64)), move |el, t| el.opacity(if ((t * FRAMES as f32).round() as usize).min(FRAMES) == k { opacity } else { 0.0 })));
            }
            return stack.into_any_element();
        }
        let start = state(0.0);
        place(holder, start)
            .with_animation(("shape-animation", played * 100_000 + (shape.id.0 as usize + target.key()) % 100_000), Animation::new(std::time::Duration::from_millis(total as u64)), move |el, t| place(el, state(t * total as f32)))
            .into_any_element()
    }
}

/// Where `p` (0 to 1) of the way along a path from (0, 0) through `points`
/// is, by the length it has on a slide of `slide` pixels (the same speed on
/// every line), in 1/100 000 of the slide.
pub(crate) fn point_on_path(points: &[(i32, i32)], p: f32, slide: (f32, f32)) -> (f32, f32) {
    let mut all = vec![(0.0f32, 0.0f32)];
    all.extend(points.iter().map(|&(x, y)| (x as f32, y as f32)));
    let length = |a: (f32, f32), b: (f32, f32)| ((b.0 - a.0) * slide.0 / 100_000.0).hypot((b.1 - a.1) * slide.1 / 100_000.0);
    let total: f32 = all.windows(2).map(|w| length(w[0], w[1])).sum();
    if total <= 0.0 || p >= 1.0 {
        return *all.last().unwrap();
    }
    let mut left = p.max(0.0) * total;
    for w in all.windows(2) {
        let l = length(w[0], w[1]);
        if left <= l && l > 0.0 {
            let t = left / l;
            return (w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t);
        }
        left -= l;
    }
    *all.last().unwrap()
}

/// Shape `frame` (x, y, width, height, pixels) on a slide `slide` (width,
/// height) after effect `a` has run `p` (0 to 1) of its time, from `from`.
pub(crate) fn motion_at(a: &ibis_doc_model::Animation, from: ShapeMotion, p: f32, frame: (f32, f32, f32, f32), slide: (f32, f32)) -> ShapeMotion {
    use ibis_doc_model::{AnimationAfterEffect, AnimationClass, AnimationLook, AnimationSide as Side};
    let off = |side: Side| match side {
        Side::Left => (-(frame.0 + frame.2), 0.0),
        Side::Right => (slide.0 - frame.0, 0.0),
        Side::Top => (0.0, -(frame.1 + frame.3)),
        Side::Bottom => (0.0, slide.1 - frame.1),
    };
    let mut res = match (a.class, a.look) {
        (AnimationClass::Entrance, AnimationLook::Fade) => ShapeMotion { opacity: p, ..from },
        (AnimationClass::Entrance, AnimationLook::Zoom) => ShapeMotion { opacity: p, grow: (from.grow.0 * p, from.grow.1 * p), ..from },
        (AnimationClass::Entrance, AnimationLook::Float(side)) => {
            let dy = match side {
                Side::Top => -30.0 * (1.0 - p),
                _ => 30.0 * (1.0 - p),
            };
            ShapeMotion { opacity: p, dy: from.dy + dy, ..from }
        }
        (AnimationClass::Entrance, AnimationLook::Wipe(_)) => ShapeMotion { opacity: p, ..from },
        (AnimationClass::Entrance, AnimationLook::Replay) => ShapeMotion { opacity: p, ..from },
        (AnimationClass::Entrance, AnimationLook::Fly(side)) => {
            let (x, y) = off(side);
            ShapeMotion { opacity: 1.0, dx: from.dx + x * (1.0 - p), dy: from.dy + y * (1.0 - p), ..from }
        }
        (AnimationClass::Entrance, _) => ShapeMotion { opacity: 1.0, ..from },
        (AnimationClass::Exit, AnimationLook::Fade) => ShapeMotion { opacity: 1.0 - p, ..from },
        (AnimationClass::Exit, AnimationLook::Zoom) => {
            let scale = (1.0 - p).max(0.0);
            ShapeMotion { opacity: if p >= 1.0 { 0.0 } else { scale }, grow: (from.grow.0 * scale, from.grow.1 * scale), ..from }
        }
        (AnimationClass::Exit, AnimationLook::Float(side)) => {
            let dy = match side {
                Side::Top => -30.0 * p,
                _ => 30.0 * p,
            };
            ShapeMotion { opacity: if p >= 1.0 { 0.0 } else { 1.0 - p }, dy: from.dy + dy, ..from }
        }
        (AnimationClass::Exit, AnimationLook::Wipe(_)) => ShapeMotion { opacity: if p >= 1.0 { 0.0 } else { 1.0 - p }, ..from },
        (AnimationClass::Exit, AnimationLook::Replay) => ShapeMotion { opacity: if p >= 1.0 { 0.0 } else { 1.0 - p }, ..from },
        (AnimationClass::Exit, AnimationLook::Fly(side)) => {
            let (x, y) = off(side);
            ShapeMotion { opacity: if p >= 1.0 { 0.0 } else { 1.0 }, dx: from.dx + x * p, dy: from.dy + y * p, ..from }
        }
        (AnimationClass::Exit, _) => ShapeMotion { opacity: if p >= 1.0 { 0.0 } else { from.opacity }, ..from },
        (AnimationClass::Path, AnimationLook::Motion(x, y)) => {
            // カスタム軌跡: along its lines, as far as the time says.
            let (x, y) = if a.path.is_empty() { (x as f32 * p, y as f32 * p) } else { point_on_path(&a.path, p, slide) };
            ShapeMotion { dx: from.dx + slide.0 * x / 100_000.0, dy: from.dy + slide.1 * y / 100_000.0, ..from }
        }
        (AnimationClass::Emphasis, AnimationLook::Spin(by)) => ShapeMotion { turn: from.turn + by as f32 / 60_000.0 * p, ..from },
        (AnimationClass::Emphasis, AnimationLook::Scale(x, y)) => ShapeMotion { grow: (from.grow.0 * (1.0 + (x as f32 / 100_000.0 - 1.0) * p), from.grow.1 * (1.0 + (y as f32 / 100_000.0 - 1.0) * p)), ..from },
        _ => from,
    };
    if p >= 1.0 {
        if a.rewind {
            return from;
        }
        match a.after_effect {
            AnimationAfterEffect::Hide => res.opacity = 0.0,
            AnimationAfterEffect::Dim => res.opacity = 0.3,
            _ => {}
        }
    }
    res
}

/// Apply SMIL/PowerPoint animation acceleration and deceleration easing to progress `p` (0.0 to 1.0).
pub(crate) fn apply_timing_easing(p: f32, accel: u32, decel: u32) -> f32 {
    if accel == 0 && decel == 0 {
        return p;
    }
    let p = p.clamp(0.0, 1.0);
    let mut a = accel as f32 / 100_000.0;
    let mut d = decel as f32 / 100_000.0;
    let sum = a + d;
    if sum > 1.0 {
        a /= sum;
        d /= sum;
    }
    let v_max = 2.0 / (2.0 - a - d);
    if a > 0.0 && p <= a {
        0.5 * (v_max / a) * p * p
    } else if d > 0.0 && p >= (1.0 - d) {
        let remain = 1.0 - p;
        1.0 - 0.5 * (v_max / d) * remain * remain
    } else {
        let x_a = 0.5 * a * v_max;
        x_a + v_max * (p - a)
    }
}

/// Normalized coordinates of `pos` on the slide, if inside the slide bounds.
fn slide_normalized_point(
    pos: Point<Pixels>,
    window_size: Size<Pixels>,
    slide_size: ibis_doc_model::EmuSize,
    zoom: f32,
    pan: (f32, f32),
) -> Option<NormalizedPoint> {
    let scale = (f32::from(window_size.width) / slide_size.width.max(1) as f32).min(f32::from(window_size.height) / slide_size.height.max(1) as f32);
    let (w, h) = (slide_size.width as f32 * scale, slide_size.height as f32 * scale);
    let (x0, y0) = ((f32::from(window_size.width) - w) / 2.0, (f32::from(window_size.height) - h) / 2.0);
    let zw = w * zoom;
    let zh = h * zoom;
    let content_x0 = x0 + (w - zw) / 2.0 + pan.0;
    let content_y0 = y0 + (h - zh) / 2.0 + pan.1;
    let nx = (f32::from(pos.x) - content_x0) / zw.max(1.0);
    let ny = (f32::from(pos.y) - content_y0) / zh.max(1.0);
    if (0.0..=1.0).contains(&nx) && (0.0..=1.0).contains(&ny) {
        Some(NormalizedPoint { x: nx, y: ny })
    } else {
        None
    }
}

/// Whether an ink stroke is within threshold pixels of a target point.
fn stroke_hits_point(stroke: &InkStroke, target: NormalizedPoint, w: f32, h: f32, threshold_px: f32) -> bool {
    let tx = target.x * w;
    let ty = target.y * h;
    let r = threshold_px + stroke.width / 2.0;
    if stroke.points.len() <= 1 {
        if let Some(p) = stroke.points.first() {
            return (p.x * w - tx).hypot(p.y * h - ty) <= r;
        }
        return false;
    }
    for seg in stroke.points.windows(2) {
        let (p1, p2) = (seg[0], seg[1]);
        if dist_to_segment(tx, ty, p1.x * w, p1.y * h, p2.x * w, p2.y * h) <= r {
            return true;
        }
    }
    false
}

/// Shortest distance from point (px, py) to line segment (x1, y1)-(x2, y2).
fn dist_to_segment(px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= 1e-4 {
        return (px - x1).hypot(py - y1);
    }
    let t = (((px - x1) * dx + (py - y1) * dy) / len_sq).clamp(0.0, 1.0);
    let proj_x = x1 + t * dx;
    let proj_y = y1 + t * dy;
    (px - proj_x).hypot(py - proj_y)
}

/// Paint an ink stroke onto the window.
fn paint_ink_stroke(window: &mut Window, stroke: &InkStroke, ox: Pixels, oy: Pixels, w: f32, h: f32) {
    if stroke.points.is_empty() {
        return;
    }
    let color = hsla_from_u32(stroke.color, stroke.opacity);
    if stroke.points.len() == 1 {
        let p = stroke.points[0];
        let cx = ox + px(p.x * w);
        let cy = oy + px(p.y * h);
        paint_disc(window, cx, cy, stroke.width / 2.0, color);
        return;
    }
    let pts: Vec<Point<Pixels>> = stroke
        .points
        .iter()
        .map(|p| point(ox + px(p.x * w), oy + px(p.y * h)))
        .collect();
    if let Some(path) = polygon_styled(&pts, Some(px(stroke.width)), false, LineCap::Round, LineJoin::Round) {
        window.paint_path(path, color);
    }
}

/// Paint a laser pointer spot with glowing halos onto the window.
fn paint_laser_pointer(window: &mut Window, laser: NormalizedPoint, ox: Pixels, oy: Pixels, w: f32, h: f32) {
    let cx = ox + px(laser.x * w);
    let cy = oy + px(laser.y * h);
    // Outer glow
    paint_disc(window, cx, cy, 14.0, Hsla { h: 0.0, s: 1.0, l: 0.5, a: 0.2 });
    // Mid glow
    paint_disc(window, cx, cy, 8.0, Hsla { h: 0.0, s: 1.0, l: 0.5, a: 0.5 });
    // Core dot
    paint_disc(window, cx, cy, 4.0, Hsla { h: 0.0, s: 1.0, l: 0.55, a: 1.0 });
    // Center bright highlight
    paint_disc(window, cx, cy, 1.5, Hsla { h: 0.0, s: 0.0, l: 1.0, a: 0.9 });
}

/// Paint a filled circle disc.
fn paint_disc(window: &mut Window, cx: Pixels, cy: Pixels, r: f32, color: Hsla) {
    if r <= 0.0 {
        return;
    }
    let disc: Vec<Point<Pixels>> = (0..16)
        .map(|i| {
            let t = std::f32::consts::TAU * i as f32 / 16.0;
            point(cx + px(r * t.cos()), cy + px(r * t.sin()))
        })
        .collect();
    if let Some(path) = polygon(&disc, None, true) {
        window.paint_path(path, color);
    }
}

/// Convert an RGB u32 and alpha into GPUI Hsla.
fn hsla_from_u32(rgb: u32, opacity: f32) -> Hsla {
    let r = ((rgb >> 16) & 0xff) as f32 / 255.0;
    let g = ((rgb >> 8) & 0xff) as f32 / 255.0;
    let b = (rgb & 0xff) as f32 / 255.0;
    Rgba { r, g, b, a: opacity }.into()
}

/// Convert an InkStroke drawn during a slideshow to a Shape (AutoShape with FormulaGeometry).
pub(crate) fn ink_stroke_to_shape(
    stroke: &InkStroke,
    slide_size: ibis_doc_model::EmuSize,
    id: ibis_doc_model::ShapeId,
) -> Option<ibis_doc_model::Shape> {
    if stroke.points.is_empty() {
        return None;
    }
    let emu_points: Vec<(i64, i64)> = stroke
        .points
        .iter()
        .map(|p| {
            (
                (p.x * slide_size.width as f32).round() as i64,
                (p.y * slide_size.height as f32).round() as i64,
            )
        })
        .collect();

    let min_x = emu_points.iter().map(|(x, _)| *x).min()?;
    let max_x = emu_points.iter().map(|(x, _)| *x).max()?;
    let min_y = emu_points.iter().map(|(_, y)| *y).min()?;
    let max_y = emu_points.iter().map(|(_, y)| *y).max()?;

    let width = (max_x - min_x).max(12_700);
    let height = (max_y - min_y).max(12_700);
    let frame = ibis_doc_model::EmuRect::new(min_x, min_y, width, height);

    let local_points: Vec<(i64, i64)> = emu_points
        .iter()
        .map(|(x, y)| (x - min_x, y - min_y))
        .collect();

    let color = ibis_doc_model::Color {
        rgb: ibis_doc_model::Rgb(stroke.color & 0x00ffffff),
        alpha: (stroke.opacity * 255.0).round().clamp(0.0, 255.0) as u8,
    };
    let emu_width = (stroke.width * 9_525.0).round() as i64;
    let mut stroke_style = ibis_doc_model::Stroke::solid(color, emu_width.max(12_700));
    stroke_style.cap = ibis_doc_model::LineCap::Round;
    stroke_style.join = ibis_doc_model::LineJoin::Round;

    Some(crate::pptx::editing::new_freehand_shape(
        id,
        frame,
        &local_points,
        Some(stroke_style),
    ))
}

/// A small button of the presenter's view (the notes' size and the teleprompter).
fn notes_button(id: &'static str, text: String, on: bool, listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded(px(4.))
        .bg(if on { rgb(0x4a4d52) } else { rgb(0x3c4043) })
        .cursor_pointer()
        .text_size(px(13.))
        .text_color(white())
        .child(text)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(listener)
}
