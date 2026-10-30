// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The menu bar and the menu it opens (2026-10-07, #12 and #15).
//!
//! The bar keeps which menu is open itself (not a `DropdownMenu` button per
//! menu), so that moving the pointer onto another menu's name while one is
//! open closes it and opens the other, as the macOS menu bar does. A menu
//! that does not fit the window shows a window of its lines between two
//! fixed rows ▲ and ▼: hovering a row scrolls, the wheel scrolls, and a
//! bar at the right edge (shown while the pointer is there) scrolls by
//! pressing or dragging.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::{Icon, Selectable as _, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::base::ElementExt as _;
use gpui_kit::*;
use crate::shell::palette::rgb;

use crate::shell::chrome::{APP_BG, MenuEntry, fill_menu_entries, menu_width_for_entries};
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;

/// A line of a menu is this tall (26 px and the 2 px gap), a separator this.
const ITEM_HEIGHT: f32 = 28.0;
const SEPARATOR_HEIGHT: f32 = 8.0;
/// The menu's padding, and the room left below it and above the window's edge.
const MENU_PADDING: f32 = 8.0;
const WINDOW_MARGIN: f32 = 12.0;
/// Bottom of the menu bar, where a menu starts.
const BAR_BOTTOM: f32 = 36.0 + 28.0;
/// How often a hovered ▲ or ▼ scrolls by one line, in milliseconds.
const SCROLL_MS: u64 = 70;
const MENU_WIDTH: f32 = 280.0;
/// Apply the same viewport limit to roots and arbitrarily deep submenus.
pub(crate) fn fitted_menu_width(width:f32, viewport:f32)->f32 {
    width.min(crate::shell::chrome::MENU_ITEM_MAX_WIDTH).min((viewport-2.*WINDOW_MARGIN).max(1.))
}


fn height_of(entry: &MenuEntry) -> f32 {
    match entry {
        MenuEntry::Separator => SEPARATOR_HEIGHT,
        _ => ITEM_HEIGHT,
    }
}

/// How many lines, starting at `start`, fit in `budget` pixels.
fn lines_from(entries: &[MenuEntry], start: usize, budget: f32) -> usize {
    let mut used = 0.0;
    let mut n = 0;
    for entry in &entries[start.min(entries.len())..] {
        used += height_of(entry);
        if used > budget {
            break;
        }
        n += 1;
    }
    n
}

/// The window of a menu: the lines shown, `start..end`, and whether it is cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Visible {
    pub start: usize,
    pub end: usize,
    pub cut: bool,
}

/// Which lines of `entries` show, from `start` (moved into the range that is
/// possible), when the menu may be `height` pixels tall.
pub(crate) fn visible(entries: &[MenuEntry], start: usize, height: f32) -> Visible {
    let all: f32 = entries.iter().map(height_of).sum::<f32>() + MENU_PADDING;
    if all <= height {
        return Visible { start: 0, end: entries.len(), cut: false };
    }
    // Two fixed rows (▲ ▼) take the room of two lines.
    let budget = (height - MENU_PADDING - 2.0 * ITEM_HEIGHT).max(ITEM_HEIGHT);
    // The last start that still fills the window to the end.
    let mut max_start = entries.len();
    let mut used = 0.0;
    while max_start > 0 && used + height_of(&entries[max_start - 1]) <= budget {
        max_start -= 1;
        used += height_of(&entries[max_start]);
    }
    let start = start.min(max_start);
    let end = (start + lines_from(entries, start, budget)).max((start + 1).min(entries.len()));
    Visible { start, end, cut: true }
}

struct SubmenuScroll {
    entries: Rc<Vec<MenuEntry>>,
    start: usize,
    width: f32,
    editor: WeakEntity<SlideEditor>,
    popup: WeakEntity<PopupMenu>,
    bars: [Bounds<Pixels>; 2],
    dragging: bool,
    scroll_task: Option<Task<()>>,
}

impl SubmenuScroll {
    fn on_bar(&self, dir: i32, at: Point<Pixels>) -> bool {
        let mut bounds = self.bars[usize::from(dir > 0)];
        let slack = px(ITEM_HEIGHT);
        if dir > 0 {
            bounds.origin.y -= slack;
        }
        bounds.size.height += slack;
        bounds.contains(&at)
    }
}

fn scroll_submenu_to(state: &Rc<std::cell::RefCell<SubmenuScroll>>, start: usize, window: &mut Window, cx: &mut App) -> bool {
    let room = SlideEditor::submenu_room(window);
    let (popup, changed) = {
        let mut s = state.borrow_mut();
        let view = visible(&s.entries, start, room);
        if view.start == s.start {
            return false;
        }
        s.start = view.start;
        (s.popup.clone(), true)
    };
    if changed {
        let state_rebuild = state.clone();
        popup
            .update(cx, |popup, cx| {
                popup.rebuild(window, cx, move |menu, window, cx| fill_submenu_window(menu, &state_rebuild, window, cx));
            })
            .ok();
        window.refresh();
    }
    changed
}

fn scroll_submenu_by(state: &Rc<std::cell::RefCell<SubmenuScroll>>, lines: i32, window: &mut Window, cx: &mut App) -> bool {
    let current = state.borrow().start;
    let next = (current as i64 + lines as i64).max(0) as usize;
    scroll_submenu_to(state, next, window, cx)
}

fn submenu_hover_scroll(state: &Rc<std::cell::RefCell<SubmenuScroll>>, dir: i32, hovered: bool, window: &mut Window, cx: &mut App) {
    if !hovered {
        state.borrow_mut().scroll_task = None;
        return;
    }
    if state.borrow().scroll_task.is_some() {
        return;
    }
    scroll_submenu_by(state, dir, window, cx);
    let state_loop = state.clone();
    let task = window.spawn(cx, async move |cx| {
        loop {
            cx.background_executor().timer(std::time::Duration::from_millis(SCROLL_MS)).await;
            let go = cx
                .update(|window, cx| {
                    let on_row = state_loop.borrow().on_bar(dir, window.mouse_position());
                    if !on_row {
                        return false;
                    }
                    scroll_submenu_by(&state_loop, dir, window, cx)
                })
                .unwrap_or(false);
            if !go {
                state_loop.borrow_mut().scroll_task = None;
                break;
            }
        }
    });
    state.borrow_mut().scroll_task = Some(task);
}

fn submenu_scroll_row(
    dir: i32,
    with_overlay: bool,
    view: Visible,
    entries: Rc<Vec<MenuEntry>>,
    state: &Rc<std::cell::RefCell<SubmenuScroll>>,
) -> PopupMenuItem {
    let state = state.clone();
    PopupMenuItem::element(move |_, _| {
        let state_hover = state.clone();
        let state_click = state.clone();
        let state_prepaint = state.clone();
        let state_canvas = state.clone();
        let entries_canvas = entries.clone();
        div()
            .id(if dir < 0 { "submenu-scroll-up" } else { "submenu-scroll-down" })
            .occlude()
            .relative()
            .w_full()
            .h(px(ITEM_HEIGHT - 2.))
            .flex()
            .items_center()
            .justify_center()
            .child(
                Icon::new(Material(if dir < 0 { "keyboard_arrow_up" } else { "keyboard_arrow_down" }))
                    .size(px(18.))
                    .text_color(rgb(0x5f6368)),
            )
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                scroll_submenu_by(&state_click, dir, window, cx);
                cx.stop_propagation();
            })
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_hover(move |hovered, window, cx| {
                submenu_hover_scroll(&state_hover, dir, *hovered, window, cx);
            })
            .on_prepaint(move |bounds, _, _| {
                state_prepaint.borrow_mut().bars[usize::from(dir > 0)] = bounds;
            })
            .when(with_overlay, |d| {
                d.child(
                    canvas(
                        |bounds, _, _| bounds,
                        move |bounds, _, window, _| {
                            let total = entries_canvas.len().max(1);
                            let items_h: f32 = entries_canvas[view.start..view.end].iter().map(height_of).sum();
                            let up_h = if view.start > 0 { ITEM_HEIGHT } else { 0.0 };
                            let down_h = if view.end < total { ITEM_HEIGHT } else { 0.0 };
                            let total_h = up_h + items_h + down_h;
                            let menu_top = if dir < 0 {
                                bounds.origin.y
                            } else {
                                bounds.origin.y + bounds.size.height - px(total_h)
                            };
                            let menu_bounds = Bounds::new(
                                point(bounds.origin.x - px(4.), menu_top - px(4.)),
                                size(bounds.size.width + px(8.), px(total_h + 8.0)),
                            );
                            let track_top = menu_top + px(up_h);
                            let track_bounds = Bounds::new(
                                point(bounds.origin.x + bounds.size.width - px(10.), track_top),
                                size(px(12.), px(items_h.max(16.0))),
                            );
                            #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
                            crate::shell::scenario::button_probe::record_bounds("submenu-scroll-track", track_bounds);
                            let shown = ((view.end - view.start) as f32 / total as f32).clamp(0.05, 1.0);
                            let top_frac = (view.start as f32 / total as f32).clamp(0.0, 1.0 - shown);
                            let thumb_h = (items_h * shown).max(16.0);
                            let thumb_y = track_top + px((items_h - thumb_h).max(0.0) * (if shown < 1.0 { top_frac / (1.0 - shown) } else { 0.0 }));
                            let thumb_bounds = Bounds::new(
                                point(bounds.origin.x + bounds.size.width - px(6.), thumb_y),
                                size(px(7.), px(thumb_h)),
                            );
                            window.paint_quad(
                                gpui::fill(thumb_bounds, crate::shell::palette::shade(0.0, 0.28)).corner_radii(px(3.5)),
                            );

                            let state_wheel = state_canvas.clone();
                            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                                if phase == DispatchPhase::Capture && menu_bounds.contains(&event.position) {
                                    let dy = f32::from(event.delta.pixel_delta(px(ITEM_HEIGHT)).y);
                                    let lines = (-dy / ITEM_HEIGHT).round() as i32;
                                    let lines = if lines == 0 && dy != 0.0 {
                                        if dy < 0.0 { 1 } else { -1 }
                                    } else {
                                        lines
                                    };
                                    if lines != 0 {
                                        scroll_submenu_by(&state_wheel, lines, window, cx);
                                    }
                                    cx.stop_propagation();
                                }
                            });

                            let scroll_at = {
                                let state_at = state_canvas.clone();
                                move |y: Pixels, window: &mut Window, cx: &mut App| {
                                    let h = f32::from(track_bounds.size.height).max(1.0);
                                    let fraction = (f32::from(y - track_bounds.origin.y) / h).clamp(0.0, 1.0);
                                    let start = ((fraction - shown / 2.0) * total as f32).round().max(0.0) as usize;
                                    scroll_submenu_to(&state_at, start, window, cx);
                                }
                            };

                            // This canvas owns only the scrollbar track. A menu-wide Bubble
                            // stop would run before earlier row listeners and disable mouse clicks.
                            let state_down = state_canvas.clone();
                            let scroll_down = scroll_at.clone();
                            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                                if phase == DispatchPhase::Capture && event.button == MouseButton::Left && track_bounds.contains(&event.position) {
                                    state_down.borrow_mut().dragging = true;
                                    scroll_down(event.position.y, window, cx);
                                    cx.stop_propagation();
                                }
                            });

                            let state_move = state_canvas.clone();
                            let scroll_move = scroll_at;
                            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                                if phase == DispatchPhase::Capture && event.dragging() && state_move.borrow().dragging {
                                    scroll_move(event.position.y, window, cx);
                                    cx.stop_propagation();
                                }
                            });

                            let state_up = state_canvas;
                            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                                    state_up.borrow_mut().dragging = false;
                                }
                            });
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            })
    })
    .disabled(true)
}

fn fill_submenu_window(
    menu: PopupMenu,
    state: &Rc<std::cell::RefCell<SubmenuScroll>>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let room = SlideEditor::submenu_room(window);
    let (entries, view, width, editor) = {
        let mut s = state.borrow_mut();
        let view = visible(&s.entries, s.start, room);
        s.start = view.start;
        (s.entries.clone(), view, s.width, s.editor.clone())
    };
    let width=fitted_menu_width(width,f32::from(window.viewport_size().width));
    let mut menu = menu.min_w(px(width)).max_w(px(width)).scrollable(false);
    let has_up = view.cut && view.start > 0;
    let has_down = view.cut && view.end < entries.len();
    if has_up {
        menu = menu.item(submenu_scroll_row(-1, true, view, entries.clone(), state));
    }
    menu = fill_menu_entries(menu, &entries[view.start..view.end], width, &editor, window, cx);
    if has_down {
        menu = menu.item(submenu_scroll_row(1, !has_up, view, entries, state));
    }
    menu
}

/// Fill a submenu or dropdown `PopupMenu` with `entries`, using the same `▲`/`▼` hover rows,
/// mouse-wheel scrolling, and always-visible scrollbar as the main menu bar when the entries
/// exceed the available window height (#136).
pub(crate) fn fill_scrollable_popup_menu(
    menu: PopupMenu,
    entries: Vec<MenuEntry>,
    width: f32,
    editor: &WeakEntity<SlideEditor>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let room = SlideEditor::submenu_room(window);
    let width=fitted_menu_width(width,f32::from(window.viewport_size().width));
    let view = visible(&entries, 0, room);
    if !view.cut {
        return fill_menu_entries(menu.min_w(px(width)).max_w(px(width)).scrollable(false), &entries, width, editor, window, cx);
    }
    let state = Rc::new(std::cell::RefCell::new(SubmenuScroll {
        entries: Rc::new(entries),
        start: 0,
        width,
        editor: editor.clone(),
        popup: cx.entity().downgrade(),
        bars: Default::default(),
        dragging: false,
        scroll_task: None,
    }));
    fill_submenu_window(menu, &state, window, cx)
}

/// The menu that is open.
pub(crate) struct OpenMenu {
    ix: usize,
    entries: Rc<Vec<MenuEntry>>,
    start: usize,
    bars: Bars,
    popup: Entity<PopupMenu>,
    /// Scrolls while ▲ or ▼ is hovered.
    scroll_task: Option<Task<()>>,
    /// The scroll bar is being dragged.
    dragging: bool,
    /// Where the scroll bar was drawn.
    bar: Rc<std::cell::Cell<Bounds<Pixels>>>,
    _dismiss: Subscription,
}

impl OpenMenu {
    pub(crate) fn index(&self) -> usize {
        self.ix
    }
    /// The first line shown.
    pub(crate) fn start(&self) -> usize {
        self.start
    }

    /// The pointer is on the ▲ (`dir` < 0) or ▼ row. The row moves by up to a
    /// line as the lines shown change (their heights differ), so the room a
    /// line's height inward counts too.
    fn on_bar(&self, dir: i32, at: Point<Pixels>) -> bool {
        let mut bounds = self.bars.borrow()[usize::from(dir > 0)];
        let slack = px(ITEM_HEIGHT);
        if dir > 0 {
            bounds.origin.y -= slack;
        }
        bounds.size.height += slack;
        bounds.contains(&at)
    }
}

/// Where the ▲ and ▼ rows were drawn last.
type Bars = Rc<std::cell::RefCell<[Bounds<Pixels>; 2]>>;

/// The ▲ or ▼ row: fixed, and scrolling while the pointer is on it.
fn scroll_row(dir: i32, editor: &WeakEntity<SlideEditor>, bars: &Bars) -> PopupMenuItem {
    let (editor, bars) = (editor.clone(), bars.clone());
    PopupMenuItem::element(move |_, _| {
        let (editor, editor_click, bars) = (editor.clone(), editor.clone(), bars.clone());
        div()
            .id(if dir < 0 { "menu-scroll-up" } else { "menu-scroll-down" })
            .occlude()
            .w_full()
            .h(px(ITEM_HEIGHT - 2.))
            .flex()
            .items_center()
            .justify_center()
            .child(Icon::new(Material(if dir < 0 { "keyboard_arrow_up" } else { "keyboard_arrow_down" })).size(px(18.)).text_color(rgb(0x5f6368)))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                editor_click.update(cx, |this, cx| this.scroll_menu_by(dir, window, cx)).ok();
                cx.stop_propagation();
            })
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_hover(move |hovered, window, cx| {
                editor.update(cx, |this, cx| this.menu_hover_scroll(dir, *hovered, window, cx)).ok();
            })
            // Where the row is drawn, for the scrolling to know the pointer is still on it.
            .on_prepaint(move |bounds, _, _| bars.borrow_mut()[usize::from(dir > 0)] = bounds)
    })
    .disabled(true)
}

fn fill_window(menu: PopupMenu, open: &OpenMenuParts, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let OpenMenuParts { entries, view, editor, bars, width } = open;
    let mut menu = menu.min_w(px(*width)).max_w(px(*width)).scrollable(false);
    if view.cut && view.start > 0 {
        menu = menu.item(scroll_row(-1, editor, bars));
    }
    menu = fill_menu_entries(menu, &entries[view.start..view.end], *width, editor, window, cx);
    if view.cut && view.end < entries.len() {
        menu = menu.item(scroll_row(1, editor, bars));
    }
    menu
}

struct OpenMenuParts {
    entries: Rc<Vec<MenuEntry>>,
    view: Visible,
    editor: WeakEntity<SlideEditor>,
    bars: Bars,
    width: f32,
}

impl SlideEditor {
    /// How tall a menu may be in this window.
    fn menu_room(window: &Window) -> f32 {
        f32::from(window.viewport_size().height) - BAR_BOTTOM - WINDOW_MARGIN
    }

    /// How tall a submenu opened from a row inside a menu may be in this window (#136).
    /// Submenus open below their parent row (e.g., row 8 in `ツール > 変更履歴の記録`),
    /// so we reserve room for the parent row offset so the submenu never overflows the bottom of the window.
    pub(crate) fn submenu_room(window: &Window) -> f32 {
        Self::submenu_room_for_viewport(f32::from(window.viewport_size().height))
    }

    pub(crate) fn submenu_room_for_viewport(viewport_h: f32) -> f32 {
        (viewport_h - 2.0 * WINDOW_MARGIN).max(ITEM_HEIGHT + MENU_PADDING)
    }

    /// Open menu `ix` (closing the one that is open).
    pub(crate) fn open_menu_at(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let menus = self.menus();
        let Some((_, entries)) = menus.menus.into_iter().nth(ix) else {
            return;
        };
        let width = fitted_menu_width(menu_width_for_entries(&entries).max(MENU_WIDTH),f32::from(window.viewport_size().width));
        let entries = Rc::new(entries);
        let editor = cx.entity().downgrade();
        let view = visible(&entries, 0, Self::menu_room(window));
        let bars: Bars = Default::default();
        let parts = OpenMenuParts { entries: entries.clone(), view, editor, bars: bars.clone(), width };
        let popup = PopupMenu::build(window, cx, move |menu, window, cx| fill_window(menu, &parts, window, cx));
        popup.focus_handle(cx).focus(window, cx);
        let dismiss = cx.subscribe_in(&popup, window, |this, _, _: &DismissEvent, window, cx| {
            this.open_menu = None;
            this.menu_dismissed_at = Some(window.mouse_position());
            this.focus_handle.focus(window, cx);
            cx.notify();
        });
        self.open_menu = Some(OpenMenu { ix, entries, start: 0, bars, popup, scroll_task: None, dragging: false, bar: Default::default(), _dismiss: dismiss });
        cx.notify();
    }

    pub(crate) fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.open_menu = None;
        cx.notify();
    }

    /// Show the menu's lines from `start` (moved into the possible range).
    pub(crate) fn scroll_menu_to(&mut self, start: usize, window: &mut Window, cx: &mut Context<Self>) {
        let room = Self::menu_room(window);
        let editor = cx.entity().downgrade();
        let Some(open) = self.open_menu.as_mut() else {
            return;
        };
        let view = visible(&open.entries, start, room);
        if view.start == open.start {
            return;
        }
        open.start = view.start;
        let width = fitted_menu_width(menu_width_for_entries(&open.entries).max(MENU_WIDTH),f32::from(window.viewport_size().width));
        let parts = OpenMenuParts { entries: open.entries.clone(), view, editor, bars: open.bars.clone(), width };
        open.popup.update(cx, |popup, cx| popup.rebuild(window, cx, move |menu, window, cx| fill_window(menu, &parts, window, cx)));
        cx.notify();
    }

    fn scroll_menu_by(&mut self, lines: i32, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = &self.open_menu {
            let start = (open.start as i64 + lines as i64).max(0) as usize;
            self.scroll_menu_to(start, window, cx);
        }
    }

    /// The pointer entered (or left) the ▲ (`dir` -1) or ▼ (1) row.
    pub(crate) fn menu_hover_scroll(&mut self, dir: i32, hovered: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.open_menu.as_mut() else {
            return;
        };
        if !hovered {
            open.scroll_task = None;
            return;
        }
        if open.scroll_task.is_some() {
            return;
        }
        self.scroll_menu_by(dir, window, cx);
        let task = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(SCROLL_MS)).await;
                let go = this
                    .update_in(cx, |this, window, cx| {
                        // Still on the row?
                        let on_row = this.open_menu.as_ref().is_some_and(|o| o.on_bar(dir, window.mouse_position()));
                        if !on_row {
                            return false;
                        }
                        let before = this.open_menu.as_ref().map(|o| o.start);
                        this.scroll_menu_by(dir, window, cx);
                        before.is_some() && before != this.open_menu.as_ref().map(|o| o.start)
                    })
                    .unwrap_or(false);
                if !go {
                    // Finished: a hover on a row starts it again.
                    this.update(cx, |this, _| {
                        if let Some(open) = this.open_menu.as_mut() {
                            open.scroll_task = None;
                        }
                    })
                    .ok();
                    break;
                }
            }
        });
        if let Some(open) = self.open_menu.as_mut() {
            open.scroll_task = Some(task);
        }
    }

    /// The scroll bar at the menu's right edge: shows where the window is
    /// in the menu, pressing or dragging moves it.
    fn render_menu_scrollbar(&self, view: Visible, total: usize, cx: &mut Context<Self>) -> impl IntoElement {
        // The track is as tall as the menu's items (the menu's height is
        // that of its lines); the thumb the part of the lines shown.
        let shown = (view.end - view.start) as f32 / total as f32;
        let top = view.start as f32 / total as f32;
        let at = move |this: &mut Self, y: Pixels, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>| {
            let fraction = ((f32::from(y - bounds.origin.y)) / f32::from(bounds.size.height)).clamp(0.0, 1.0);
            // The middle of the thumb follows the pointer.
            let start = ((fraction - shown / 2.0) * total as f32).round().max(0.0) as usize;
            this.scroll_menu_to(start, window, cx);
        };
        let top_inset = MENU_PADDING / 2.0 + if view.start > 0 { ITEM_HEIGHT } else { 0.0 };
        let bottom_inset = MENU_PADDING / 2.0 + if view.end < total { ITEM_HEIGHT } else { 0.0 };
        let cell = self.open_menu.as_ref().map(|o| o.bar.clone()).unwrap_or_default();
        let (down, drag) = (cell.clone(), cell.clone());
        div()
            .id("menu-scrollbar")
            .group("menu-scrollbar")
            .absolute()
            .top(px(top_inset))
            .bottom(px(bottom_inset))
            .right_0()
            .w(px(12.))
            .on_prepaint(move |bounds, _, _| cell.set(bounds))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                if let Some(open) = this.open_menu.as_mut() {
                    open.dragging = true;
                }
                at(this, event.position.y, down.get(), window, cx);
                cx.stop_propagation();
            }))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                if event.dragging() && this.open_menu.as_ref().is_some_and(|o| o.dragging) {
                    at(this, event.position.y, drag.get(), window, cx);
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| {
                if let Some(open) = this.open_menu.as_mut() {
                    open.dragging = false;
                }
            }))
            .child(
                div()
                    .absolute()
                    .right(px(2.))
                    .w(px(7.))
                    .top(relative(top))
                    .h(relative(shown))
                    .rounded(px(4.))
                    .bg(crate::shell::palette::shade(0.0, 0.28))
                    .group_hover("menu-scrollbar", |s| s.bg(crate::shell::palette::shade(0.0, 0.45))),
            )
    }

    /// The menu bar.
    pub(crate) fn render_menu_bar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menus = self.menus();
        let open_ix = self.open_menu.as_ref().map(|o| o.ix);
        let room = Self::menu_room(window);
        let logo_height=28.*0.8*0.8;
        let menu_width=menus.menus.iter().map(|(id,_)|crate::shell::chrome::dropdown_list::label_width(window,&id.label(),14.)+16.).sum::<f32>();
        let show_logo=f32::from(window.viewport_size().width)>=8.+menu_width+16.+logo_height*crate::shell::chrome::LOGO_ASPECT+12.;
        // The logo is a button with the menu names' padding, so it needs no gap after it and starts a little further
        // left: the menu names stay where they were before the logo became a button (4 + 8 + logo + 8 = 8 + logo + 12).
        let leading=if show_logo{(8.+12.-2.*MENU_BAR_BUTTON_PADDING).max(0.)}else{8.};
        h_flex()
            .h(px(28.))
            .flex_shrink_0()
            .pl(px(leading))
            .items_center()
            .bg(rgb(APP_BG))
            .when(show_logo,|bar|bar.child(div().relative().child({
                    // The logo opens the start window: a modal dialog leaves it available (`panel_dialog_allows_pointer`).
                    let bounds=self.logo_bounds.clone();
                    crate::shell::editor::bounds_observer(move |b,_,_|bounds.set(Some(b)))
                }).child(
                {
                    let logo=menu_bar_button("editor-app-logo".into())
                        .child(crate::shell::chrome::brand_logo("editor-app-logo-image",logo_height,false));
                    // The Web version has one page and no start window: there the logo is only the mark.
                    #[cfg(not(target_arch="wasm32"))]
                    let logo=logo.cursor_pointer().tooltip(crate::shell::i18n::text("shell.start-window"))
                        .on_click(cx.listener(|_,_,_,cx|crate::open_editor(None,None,cx)));
                    logo
                })))
            .children(menus.menus.into_iter().enumerate().map(|(ix, (id, _))| {
                let selected = open_ix == Some(ix);
                div()
                    .id(SharedString::from(format!("menu-{}", id.key())))
                    .relative()
                    .child(
                        menu_bar_button(SharedString::from(format!("menu-button-{}", id.key())))
                            .label(id.label())
                            .text_size(px(14.))
                            .selected(selected)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    // Stop propagation to avoid dragging the window.
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    // The press that closed the menu (a press outside it
                                    // closes it before this runs) does not open it again.
                                    let pressed_to_close = this.menu_dismissed_at.take().is_some_and(|p| p == event.position);
                                    if selected || pressed_to_close {
                                        this.close_menu(cx);
                                    } else {
                                        this.open_menu_at(ix, window, cx);
                                    }
                                }),
                            ),
                    )
                    // While a menu is open, the pointer on another menu's name
                    // opens that one.
                    .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                        if *hovered && this.open_menu.as_ref().is_some_and(|o| o.ix != ix) {
                            this.open_menu_at(ix, window, cx);
                        }
                    }))
                    .when(selected, |d| d.child(self.render_open_menu(room, cx)))
            }))
            .child(div().flex_1())

    }

    fn render_open_menu(&self, room: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.open_menu.as_ref().expect("a menu is open");
        let total = open.entries.len();
        let view = visible(&open.entries, open.start, room);
        deferred(
            anchored().anchor(Anchor::TopLeft).snap_to_window_with_margin(px(8.)).child(
                div()
                    .occlude()
                    .top_1()
                    .relative()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .child(open.popup.clone())
                    // Over the menu (an element with a handler that does not occlude,
                    // as the menu itself occludes what is behind it): the wheel.
                    .child(div().absolute().top_0().left_0().size_full().on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                        let dy = f32::from(event.delta.pixel_delta(px(ITEM_HEIGHT)).y);
                        // The content moves with the fingers: scrolling down shows later lines.
                        let lines = (-dy / ITEM_HEIGHT).round() as i32;
                        let lines = if lines == 0 && dy != 0.0 { if dy < 0.0 { 1 } else { -1 } } else { lines };
                        if lines != 0 {
                            this.scroll_menu_by(lines, window, cx);
                        }
                        cx.stop_propagation();
                    })))
                    .when(view.cut, |d| d.child(self.render_menu_scrollbar(view, total, cx))),
            ),
        )
        .with_priority(gpui_kit::base::POPUP_PRIORITY)
    }
}

/// Height of a menu bar item's hover/selected rounded rectangle.
pub(crate) const MENU_BAR_BUTTON_HEIGHT: f32 = 24.;
/// Horizontal padding inside a menu bar item (the label or the logo).
pub(crate) const MENU_BAR_BUTTON_PADDING: f32 = 8.;
/// Corner radius of a menu bar item's hover/selected rectangle.
pub(crate) const MENU_BAR_BUTTON_RADIUS: f32 = 4.;
/// One menu bar item: the menus and the ibisOffice logo share its shape, hover and selected colours.
pub(crate) fn menu_bar_button(id: SharedString) -> Button {
    let button = Button::new(id.clone())
        .ghost()
        .small()
        .h(px(MENU_BAR_BUTTON_HEIGHT))
        .px(px(MENU_BAR_BUTTON_PADDING))
        .rounded(px(MENU_BAR_BUTTON_RADIUS));
    #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
    let button = crate::shell::scenario::button_probe::observe(button, id.into());
    button
}

#[cfg(test)]
mod tests {
    use super::{SlideEditor, visible};
    use crate::shell::chrome::{MenuEntry, MenuItem};

    #[test]
    fn submenu_room_cuts_long_track_changes_submenu_and_reaches_last_item() {
        let mut entries: Vec<MenuEntry> = Vec::new();
        for i in 0..23 {
            if i == 5 || i == 13 {
                entries.push(MenuEntry::Separator);
            }
            let id: &'static str = if i == 22 {
                "docx.track.change-user-name"
            } else {
                "docx.track.item"
            };
            entries.push(MenuItem::new(id, format!("Item {i}")).into());
        }
        assert_eq!(entries.len(), 25);

        assert!(!visible(&entries,0,SlideEditor::submenu_room_for_viewport(800.)).cut,"use the available height in a roomy window");
        let room = SlideEditor::submenu_room_for_viewport(560.0);
        let initial = visible(&entries, 0, room);
        assert!(initial.cut, "25-entry submenu in a 560px window must be scrollable (cut: true)");
        assert_eq!(initial.start, 0);
        assert!(initial.end < entries.len());

        let scrolled_to_end = visible(&entries, entries.len(), room);
        assert!(scrolled_to_end.cut);
        assert_eq!(scrolled_to_end.end, entries.len(), "Scrolling to the bottom must include the last item (docx.track.change-user-name)");
        assert!(scrolled_to_end.start > 0);
    }
}
