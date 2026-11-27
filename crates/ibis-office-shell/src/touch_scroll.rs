// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! One finger on the page selects and moves; two fingers scroll it (K0065; design `tasks/doc/design/2026-11-24-k0065-touch-scroll.md`).
//!
//! gpui's touch recognizer (`forks/gpui-pre/src/gestures.rs`) reads the raw touches of every platform alike: a tap, a pan (a scroll of one
//! finger with its coasting, `touch_fingers` 1), and two fingers moving together (a scroll with `touch_fingers` 2, with coasting too; the first
//! finger's gesture is taken back as #209 says). What this module decides is what **one finger on the page** (the document, the grid, the
//! slide) is: not a scroll but the mouse. The window offers each touch as a *touch drag* before it is anything else
//! (`TouchDragEvent`); the window's root claims it when it began on the page, and the page gets the press, the moves and the release of the mouse
//! at the finger's places, so a drag selects text or cells and moves shapes, and a tap places the caret (double and triple taps count as the
//! mouse's clicks do).
//!
//! A touch that began outside the page (a panel, a list, a bar) is not claimed: it scrolls, and coasts and stretches at the end as
//! `UIScrollView` does.

use gpui_kit::*;

use crate::editor::SlideEditor;

/// A tap of one place within this of the last, within [`MULTI_TAP_INTERVAL`], counts as the next click of a double or triple click
/// (gpui's `GestureTuning` values for taps).
const MULTI_TAP_SLOP: f32 = 16.;
const MULTI_TAP_INTERVAL: std::time::Duration = std::time::Duration::from_millis(400);
/// A touch that moves less than this is a tap (gpui's `touch_slop`).
const TAP_SLOP: f32 = 8.;
/// A finger held within [`TAP_SLOP`] this long is the right click (K0073; gpui's `GestureTuning::long_press_duration`: the same 500 ms on every platform).
const LONG_PRESS: std::time::Duration = std::time::Duration::from_millis(500);

/// How far from a splitter's line a finger takes hold of it (K0084): 22 points to each side, 44 across (iOS's advice for a touch target, Android's 48 dp), where the
/// mouse has the line's own few points.
const SPLITTER_GRAB: f32 = 22.;

/// The state of the touch drags of a window (`SlideEditor::touch_scroll`).
#[derive(Default)]
pub struct TouchScroll {
    /// A touch that began on the page is being given to it as the mouse.
    dragging: bool,
    /// The mouse's press has been sent for it (it moved out of the tap slop).
    pressed: bool,
    /// Where it began, how many clicks it is, and the last tap (when, where, how many clicks) for counting a double click.
    started_at: Point<Pixels>,
    clicks: usize,
    last_tap: Option<(crate::clock::Instant, Point<Pixels>, usize)>,
    /// The wait for the long press (dropped when the finger moves out of the slop, lifts, or is taken back) and whether it has come:
    /// the right click was sent, and the rest of this touch (its moves and its lift) sends nothing, so that the lift does not click the menu away.
    long_press: Option<Task<()>>,
    long_pressed: bool,
    /// The touch took hold of a splitter (K0084): how far the finger's places are from the line's (the press, the moves and the release are sent at the line's
    /// places: the finger's plus this), so a finger beside the line, within [`SPLITTER_GRAB`], moves it as one on it does.
    splitter: Option<Point<Pixels>>,
}

impl SlideEditor {
    /// Whether a finger that goes down at `position` is the mouse for the page.
    fn finger_drags_at(&self, position: Point<Pixels>, cx: &App) -> bool {
        self.is_touch_device() && !self.dialog_is_modal() && self.workspace_bounds.is_some_and(|bounds| bounds.contains(&position)) && !self.popup_at(position, cx)
    }

    /// Whether a menu or a dropdown list that is open lies over `position`: a finger on it scrolls it (K0078), it is not the page's mouse. (A menu, its submenus and the menu
    /// search's list are the ones that lie over the page and scroll; the other popups keep what they did.)
    fn popup_at(&self, position: Point<Pixels>, cx: &App) -> bool {
        if let Some(open) = self.open_menu.as_ref() {
            let mut menu = Some(open.popup_entity().clone());
            while let Some(m) = menu {
                if m.read(cx).painted_bounds().contains(&position) {
                    return true;
                }
                menu = m.read(cx).shown_submenu();
            }
        }
        self.toolbar_search_open.and_then(|id| self.toolbar_search_scrolls.get(id)).is_some_and(|handle| handle.bounds().contains(&position))
    }

    /// A touch drag offered by the window: claimed when it began on the page. Returns whether it is.
    ///
    /// Nothing is sent to the page when the finger goes down: a second finger that comes at once makes the two a scroll (and an IME's composition
    /// must not be broken by the first). The press is sent when the finger has moved out of the tap slop (a drag, at the place it began), or with the
    /// release (a tap).
    pub(crate) fn on_touch_drag(&mut self, event: &TouchDragEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match event.phase {
            TouchPhase::Started => {
                // A splitter's line within a finger's reach is taken first (also in a dialog, and outside the page).
                let splitter = self.splitter_shift_at(event.start_position);
                if splitter.is_none() && !self.finger_drags_at(event.start_position, cx) {
                    return false;
                }
                let position = event.start_position;
                let clicks = match &self.touch_scroll.last_tap {
                    Some((at, last, count)) if at.elapsed() <= MULTI_TAP_INTERVAL && distance(*last, position) <= MULTI_TAP_SLOP => count % 3 + 1,
                    _ => 1,
                };
                self.touch_scroll.dragging = true;
                self.touch_scroll.pressed = false;
                self.touch_scroll.started_at = position;
                self.touch_scroll.clicks = clicks;
                self.touch_scroll.long_pressed = false;
                self.touch_scroll.splitter = splitter;
                // (A finger on a splitter has no right click.)
                self.touch_scroll.long_press = splitter.is_none().then(|| cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(LONG_PRESS).await;
                    this.update_in(cx, |this, window, cx| this.long_press_on_page(window, cx)).ok();
                }));
                true
            }
            TouchPhase::Moved if self.touch_scroll.dragging && self.touch_scroll.long_pressed => true,
            TouchPhase::Moved if self.touch_scroll.dragging => {
                let start = self.touch_scroll.started_at;
                let clicks = self.touch_scroll.clicks;
                let mut inputs = Vec::new();
                if !self.touch_scroll.pressed && distance(start, event.position) >= TAP_SLOP {
                    self.touch_scroll.pressed = true;
                    self.touch_scroll.long_press = None;
                    let at = self.at_line(start);
                    inputs.push(PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: None, modifiers: Modifiers::default() }));
                    inputs.push(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at, modifiers: Modifiers::default(), click_count: clicks, first_mouse: false }));
                }
                if self.touch_scroll.pressed {
                    inputs.push(PlatformInput::MouseMove(MouseMoveEvent { position: self.at_line(event.position), pressed_button: Some(MouseButton::Left), modifiers: Modifiers::default() }));
                    self.send(inputs, window, cx);
                }
                true
            }
            TouchPhase::Ended if self.touch_scroll.dragging && self.touch_scroll.long_pressed => {
                self.touch_scroll.dragging = false;
                self.touch_scroll.long_pressed = false;
                self.touch_scroll.last_tap = None;
                true
            }
            TouchPhase::Ended if self.touch_scroll.dragging => {
                self.touch_scroll.dragging = false;
                self.touch_scroll.long_press = None;
                let (clicks, position) = (self.touch_scroll.clicks, self.at_line(event.position));
                let mut inputs = Vec::new();
                if self.touch_scroll.splitter.is_some() && !self.touch_scroll.pressed {
                    // A tap on a splitter changes nothing.
                    self.touch_scroll.splitter = None;
                    return true;
                }
                if self.touch_scroll.pressed {
                    self.touch_scroll.last_tap = None;
                } else {
                    // A tap: the press and the release together, as a click.
                    self.touch_scroll.last_tap = Some((crate::clock::Instant::now(), position, clicks));
                    inputs.push(PlatformInput::MouseMove(MouseMoveEvent { position, pressed_button: None, modifiers: Modifiers::default() }));
                    inputs.push(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position, modifiers: Modifiers::default(), click_count: clicks, first_mouse: false }));
                }
                self.touch_scroll.pressed = false;
                self.touch_scroll.splitter = None;
                inputs.push(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position, modifiers: Modifiers::default(), click_count: clicks }));
                self.send(inputs, window, cx);
                true
            }
            // Taken back (a second finger came down, #209): the window ends the touch session as cancelled, and the page's changes since are undone.
            TouchPhase::Cancelled if self.touch_scroll.dragging => {
                // A splitter that was being moved goes back to where it was taken (the mouse back to the press and released there).
                if self.touch_scroll.splitter.is_some() && self.touch_scroll.pressed {
                    let at = self.at_line(self.touch_scroll.started_at);
                    self.send(
                        vec![
                            PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: Some(MouseButton::Left), modifiers: Modifiers::default() }),
                            PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at, modifiers: Modifiers::default(), click_count: 1 }),
                        ],
                        window,
                        cx,
                    );
                }
                self.touch_scroll.splitter = None;
                self.touch_scroll.dragging = false;
                self.touch_scroll.long_press = None;
                self.touch_scroll.long_pressed = false;
                self.touch_scroll.pressed = false;
                self.touch_scroll.last_tap = None;
                true
            }
            _ => false,
        }
    }

    /// The finger has been held on the page for [`LONG_PRESS`] without leaving the slop: the right click at the place it went down (the menu opens as a
    /// right click's does, with the caret as #140 has it), while the finger is still down.
    fn long_press_on_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.touch_scroll.long_press = None;
        if !self.touch_scroll.dragging || self.touch_scroll.pressed || self.touch_scroll.long_pressed {
            return;
        }
        self.touch_scroll.long_pressed = true;
        let position = self.touch_scroll.started_at;
        self.send(
            vec![
                PlatformInput::MouseMove(MouseMoveEvent { position, pressed_button: None, modifiers: Modifiers::default() }),
                PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Right, position, modifiers: Modifiers::default(), click_count: 1, first_mouse: false }),
                PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Right, position, modifiers: Modifiers::default(), click_count: 1 }),
            ],
            window,
            cx,
        );
    }

    /// The page gets `inputs` after the touch event that asked for them is over (the page's own handlers are not run inside this one).
    fn send(&self, inputs: Vec<PlatformInput>, window: &mut Window, cx: &mut Context<Self>) {
        window.defer(cx, move |window, cx| {
            for input in inputs {
                window.dispatch_event(input, cx);
            }
        });
    }

    /// An element to put in a splitter's line (inside the positioned line itself): the line's place is kept (`splitter_lines`) so that a finger within
    /// [`SPLITTER_GRAB`] of it takes hold of it (`on_touch_drag`). The lines are forgotten with each render and kept again as they are drawn.
    pub fn splitter_mark(&self) -> impl IntoElement {
        let lines = self.splitter_lines.clone();
        crate::editor::bounds_observer(move |bounds, _, _| lines.borrow_mut().push(bounds))
    }

    /// How far a finger that goes down at `position` is from the line of the splitter it can take hold of: the nearest line within [`SPLITTER_GRAB`] across it and
    /// along it, a vertical one (taller than wide) is moved across, a horizontal one down.
    fn splitter_shift_at(&self, position: Point<Pixels>) -> Option<Point<Pixels>> {
        let (x, y) = (f32::from(position.x), f32::from(position.y));
        let mut best: Option<(f32, Point<Pixels>)> = None;
        for line in self.splitter_lines.borrow().iter() {
            let (left, top, right, bottom) = (f32::from(line.left()), f32::from(line.top()), f32::from(line.right()), f32::from(line.bottom()));
            let (across, shift, along_ok) = if (right - left) <= (bottom - top) {
                let middle = (left + right) / 2.0;
                ((x - middle).abs(), point(px(middle - x), px(0.)), y >= top && y <= bottom)
            } else {
                let middle = (top + bottom) / 2.0;
                ((y - middle).abs(), point(px(0.), px(middle - y)), x >= left && x <= right)
            };
            if along_ok && across <= SPLITTER_GRAB && best.as_ref().is_none_or(|(d, _)| across < *d) {
                best = Some((across, shift));
            }
        }
        best.map(|(_, shift)| shift)
    }

    /// `position` as the page gets it: at the line of the splitter the finger holds (K0084), else as it is.
    fn at_line(&self, position: Point<Pixels>) -> Point<Pixels> {
        match self.touch_scroll.splitter {
            Some(shift) => point(position.x + shift.x, position.y + shift.y),
            None => position,
        }
    }

    /// How far the body is stretched past an end of its scroll now (x, y; pixels).
    pub fn touch_overscroll(&self) -> (f32, f32) {
        (self.overscroll_x.get(), self.overscroll_y.get())
    }

    /// For the scenarios: a touch is being given to the page as the mouse.
    #[cfg(feature = "scenarios")]
    pub fn touch_dragging_for_scenario(&self) -> bool {
        self.touch_scroll.dragging
    }
}

fn distance(a: Point<Pixels>, b: Point<Pixels>) -> f32 {
    ((f32::from(a.x) - f32::from(b.x)).powi(2) + (f32::from(a.y) - f32::from(b.y)).powi(2)).sqrt()
}

/// The element (put once in the window's root) that offers the window's touch drags to the editor.
pub fn touch_scroll_listener(cx: &Context<SlideEditor>) -> impl IntoElement {
    let editor = cx.entity().downgrade();
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let editor = editor.clone();
            window.on_mouse_event(move |event: &TouchDragEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && editor.update(cx, |this, cx| this.on_touch_drag(event, window, cx)).unwrap_or(false) && event.phase == TouchPhase::Started {
                    window.prevent_default();
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
