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
use crate::palette::rgb;

use crate::chrome::{APP_BG, MenuEntry, MenuId, Submenu, fill_menu_entries, menu_width_for_entries};
use crate::editor::SlideEditor;
use crate::icons::Material;
use crate::menu_context::{context_tone, ContextMenu};
use crate::menu_layout::TitleMenu;

/// A line of a menu is this tall (26 px and the 2 px gap), a separator this.
const ITEM_HEIGHT: f32 = 28.0;
const SEPARATOR_HEIGHT: f32 = 8.0;
/// The menu's padding, and the room left below it and above the window's edge.
const MENU_PADDING: f32 = 8.0;
const WINDOW_MARGIN: f32 = 12.0;
/// Bottom of the menu bar, where a menu starts.
pub(crate) const BAR_BOTTOM: f32 = 36.0 + 28.0;
/// How often a hovered ▲ or ▼ scrolls by one line, in milliseconds.
const SCROLL_MS: u64 = 70;
const MENU_WIDTH: f32 = 280.0;
/// How far a row's content is from the popup's left and right edges: the popup's `p_1` (4) and a row's `px_2`
/// (8) in `PopupMenu`. A submenu draws its scroll bar from the ▲/▼ row, so it reaches out this far to the edge.
const POPUP_ROW_INSET_X: f32 = 12.0;
/// Apply the same viewport limit to roots and arbitrarily deep submenus.
pub(crate) fn fitted_menu_width(width:f32, viewport:f32)->f32 {
    width.min(crate::chrome::MENU_ITEM_MAX_WIDTH).min((viewport-2.*WINDOW_MARGIN).max(1.))
}


fn height_of(entry: &MenuEntry) -> f32 {
    match entry {
        MenuEntry::Separator => SEPARATOR_HEIGHT,
        MenuEntry::Tiles(items) => crate::menu_tiles::row_height_of(items),
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
pub struct Visible {
    pub start: usize,
    pub end: usize,
    pub cut: bool,
}

/// Which lines of `entries` show, from `start` (moved into the range that is
/// possible), when the menu may be `height` pixels tall.
pub fn visible(entries: &[MenuEntry], start: usize, height: f32) -> Visible {
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

struct MenuScroll {
    entries: Rc<Vec<MenuEntry>>,
    start: usize,
    width: f32,
    editor: WeakEntity<SlideEditor>,
    popup: WeakEntity<PopupMenu>,
    room: MenuRoom,
    bars: [Bounds<Pixels>; 2],
    scrollbar: Rc<crate::menu_scrollbar::ScrollbarState>,
    scroll_task: Option<Task<()>>,
}

/// Where a menu opens, which sets how tall it may be.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuRoom {
    /// Below its name in the menu bar.
    BelowBar,
    /// Beside a row of a menu (a submenu or a dropdown).
    Beside,
}

impl MenuScroll {
    /// How tall the menu may be in `window`.
    fn room(&self, window: &Window) -> f32 {
        match self.room {
            MenuRoom::BelowBar => SlideEditor::menu_room(window),
            MenuRoom::Beside => SlideEditor::submenu_room(window),
        }
    }

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

fn scroll_menu_to(state: &Rc<std::cell::RefCell<MenuScroll>>, start: usize, window: &mut Window, cx: &mut App) -> bool {
    let (popup, changed) = {
        let mut s = state.borrow_mut();
        let room = s.room(window);
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
                popup.rebuild(window, cx, move |menu, window, cx| fill_menu_window(menu, &state_rebuild, window, cx));
            })
            .ok();
        window.refresh();
    }
    changed
}

fn scroll_menu_by(state: &Rc<std::cell::RefCell<MenuScroll>>, lines: i32, window: &mut Window, cx: &mut App) -> bool {
    let current = state.borrow().start;
    let next = (current as i64 + lines as i64).max(0) as usize;
    scroll_menu_to(state, next, window, cx)
}

fn menu_hover_scroll(state: &Rc<std::cell::RefCell<MenuScroll>>, dir: i32, hovered: bool, window: &mut Window, cx: &mut App) {
    if !hovered {
        state.borrow_mut().scroll_task = None;
        return;
    }
    if state.borrow().scroll_task.is_some() {
        return;
    }
    scroll_menu_by(state, dir, window, cx);
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
                    scroll_menu_by(&state_loop, dir, window, cx)
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

fn menu_scroll_row(
    dir: i32,
    with_overlay: bool,
    view: Visible,
    entries: Rc<Vec<MenuEntry>>,
    state: &Rc<std::cell::RefCell<MenuScroll>>,
) -> PopupMenuItem {
    let state = state.clone();
    PopupMenuItem::element(move |_, _| {
        let state_hover = state.clone();
        let state_click = state.clone();
        let state_prepaint = state.clone();
        let state_canvas = state.clone();
        let entries_canvas = entries.clone();
        div()
            .id(if dir < 0 { "menu-scroll-up" } else { "menu-scroll-down" })
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
                scroll_menu_by(&state_click, dir, window, cx);
                cx.stop_propagation();
            })
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_hover(move |hovered, window, cx| {
                menu_hover_scroll(&state_hover, dir, *hovered, window, cx);
            })
            .on_prepaint(move |bounds, _, _| {
                state_prepaint.borrow_mut().bars[usize::from(dir > 0)] = bounds;
            })
            .when(with_overlay, |d| {
                // Deferred: drawn after all the rows, so the highlighted row's background does not cover the scroll bar
                // (the canvas sits in the first scroll row, which is drawn before the items when the menu is scrolled; #175).
                d.child(deferred(
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
                            // `bounds` is the popup's whole width (the canvas reaches out of the row to its edges).
                            let menu_bounds = Bounds::new(
                                point(bounds.origin.x, menu_top - px(MENU_PADDING / 2.0)),
                                size(bounds.size.width, px(total_h + MENU_PADDING)),
                            );
                            let track_top = menu_top + px(up_h);
                            let track_bounds = Bounds::new(
                                point(bounds.origin.x + bounds.size.width - px(crate::menu_scrollbar::TRACK_WIDTH), track_top),
                                size(px(crate::menu_scrollbar::TRACK_WIDTH), px(items_h.max(16.0))),
                            );
                            #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
                            crate::scenario::button_probe::record_bounds("submenu-popup", bounds);
                            let (scrollbar_state, scroll_state) = (state_canvas.borrow().scrollbar.clone(), state_canvas.clone());
                            crate::menu_scrollbar::paint(window, crate::menu_scrollbar::Scrollbar {
                                track: track_bounds,
                                start: view.start,
                                end: view.end,
                                total,
                                state: scrollbar_state,
                                scroll_to: Rc::new(move |start, window, cx| { scroll_menu_to(&scroll_state, start, window, cx); }),
                                // Only the names the scenarios find them by differ.
                                probes: match state_canvas.borrow().room {
                                    MenuRoom::BelowBar => ("menu-scrollbar-track", "menu-scrollbar-thumb"),
                                    MenuRoom::Beside => ("submenu-scroll-track", "submenu-scroll-thumb"),
                                },
                            });

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
                                        scroll_menu_by(&state_wheel, lines, window, cx);
                                    }
                                    cx.stop_propagation();
                                }
                            });

                        },
                    )
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(-POPUP_ROW_INSET_X))
                    .right(px(-POPUP_ROW_INSET_X)),
                )
                // Above the popup's own deferred layer (POPUP_PRIORITY), or the popup's background would cover it.
                .with_priority(gpui_kit::base::POPUP_PRIORITY + 1))
            })
    })
    .disabled(true)
}

fn fill_menu_window(
    menu: PopupMenu,
    state: &Rc<std::cell::RefCell<MenuScroll>>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let (entries, view, width, editor) = {
        let mut s = state.borrow_mut();
        let room = s.room(window);
        let view = visible(&s.entries, s.start, room);
        s.start = view.start;
        (s.entries.clone(), view, s.width, s.editor.clone())
    };
    let width=fitted_menu_width(width,f32::from(window.viewport_size().width));
    let mut menu = menu.min_w(px(width)).max_w(px(width)).scrollable(false);
    let has_up = view.cut && view.start > 0;
    let has_down = view.cut && view.end < entries.len();
    if has_up {
        menu = menu.item(menu_scroll_row(-1, true, view, entries.clone(), state));
    }
    menu = fill_menu_entries(menu, &entries[view.start..view.end], width, &editor, window, cx);
    if has_down {
        menu = menu.item(menu_scroll_row(1, !has_up, view, entries, state));
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
    let state = Rc::new(std::cell::RefCell::new(MenuScroll {
        entries: Rc::new(entries),
        start: 0,
        width,
        editor: editor.clone(),
        popup: cx.entity().downgrade(),
        room: MenuRoom::Beside,
        bars: Default::default(),
        scrollbar: Default::default(),
        scroll_task: None,
    }));
    fill_menu_window(menu, &state, window, cx)
}

/// The menu that is open: the popup, and the same scrolling state (`MenuScroll`) a submenu has.
pub struct OpenMenu {
    ix: usize,
    /// The corner of the popup that is put on its anchor point (`TopRight` for the title row's Draw button).
    anchor: Anchor,
    /// Which menu it is (`None` for the ▾ that holds the menus that do not fit).
    menu: Option<BarMenu>,
    scroll: Rc<std::cell::RefCell<MenuScroll>>,
    popup: Entity<PopupMenu>,
    _dismiss: Subscription,
}

impl OpenMenu {
    pub fn index(&self) -> usize {
        self.ix
    }
    /// The first line shown.
    pub fn start(&self) -> usize {
        self.scroll.borrow().start
    }
}

/// A menu of the bar: one of the screen's static menus, or a context menu of the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarMenu {
    Static(MenuId),
    Context(ContextMenu),
}

impl BarMenu {
    /// Its key in the tables of the UI's words (also the id of its parts in the bar).
    pub fn key(self) -> &'static str {
        match self {
            BarMenu::Static(id) => id.key(),
            BarMenu::Context(id) => id.key(),
        }
    }

    pub(crate) fn label(self) -> SharedString {
        crate::i18n::text(self.key())
    }

    /// The name as the bar draws it: with its access key (`ファイル(F)`, the letter to be underlined while Alt is held) on Windows and Linux for a
    /// static menu; plain for a context menu (Alt+J goes round them) and on the other systems.
    pub fn display(self) -> crate::access_key::Resolved {
        let label = self.label();
        match self {
            BarMenu::Static(id) => match id.access_letter() {
                Some(letter) => crate::access_key::menu_label(&label, letter),
                None => crate::access_key::Resolved { text: label.to_string(), underline: None, key: None },
            },
            BarMenu::Context(_) => crate::access_key::Resolved { text: label.to_string(), underline: None, key: None },
        }
    }
}

/// The index of the ▾ (the menus that do not fit) among the bar's menus, for `open_menu_at`.
pub const OVERFLOW_IX: usize = usize::MAX;
/// The index of the title row's Draw button's menu, for `open_menu_at` (it is not a menu of the bar).
pub(crate) const DRAW_IX: usize = usize::MAX - 1;
/// The ▾ button's width (the toolbar's small triangle).
const MORE_WIDTH: f32 = 24.0;
/// Room left at the right end of the bar.
const BAR_TRAILING: f32 = 8.0;
/// The logo's height in the bar (the old `28 * 0.8 * 0.8`: it stays as it was).
const LOGO_HEIGHT_IN_BAR: f32 = 28. * 0.8 * 0.8;
/// How long a note shows in the bar, in seconds.
const NOTICE_SECONDS: u64 = 5;
/// A context menu that appears fades in this long, in milliseconds (spec 4.1).
const FADE_MS: u64 = 280;
/// The width of the thin line between the static menus and the context menus (1 and a margin of 4 each side).
const GROUP_LINE_WIDTH: f32 = 9.0;

/// The room at the bar's left: the logo's padding when the logo shows, else 8.
fn bar_leading(show_logo: bool) -> f32 {
    if show_logo { (8. + 12. - 2. * MENU_BAR_BUTTON_PADDING).max(0.) } else { 8. }
}

/// Where the parts of the bar go (`SlideEditor::bar_layout`).
pub(crate) struct BarLayout {
    show_logo: bool,
    leading: f32,
    /// The indices (into the bar's menus) that show, and those folded into the ▾, each in the bar's order.
    pub(crate) visible: Vec<usize>,
    pub(crate) hidden: Vec<usize>,
}

/// In what order a menu is folded into the ▾ when the window is too narrow (spec 4.3): first the Help menu, then the
/// static menus that are used least, then the context menus from the right; the File, Edit, Insert, View and Window
/// menus never are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fold {
    Never,
    /// Folded at this rank (smaller first).
    Rank(u32),
}

/// A menu of the bar as the layout sees it: how wide its name is (with the button's padding) and when it folds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BarSlot {
    pub width: f32,
    pub fold: Fold,
}

/// Which of the static menus are used least, in the order they fold (spec 4.3).
fn static_fold_rank(kind: ibis_pptx::Kind, id: MenuId) -> Fold {
    let order: &[MenuId] = crate::format::info(kind).fold_order;
    match id {
        MenuId::Help => Fold::Rank(0),
        id => order.iter().position(|m| *m == id).map_or(Fold::Never, |i| Fold::Rank(1 + i as u32)),
    }
}

/// The slots of the bar's menus, in the bar's order (the static menus, then the context menus: these fold from the right).
fn bar_slots(items: &[(BarMenu, Vec<MenuEntry>)], kind: ibis_pptx::Kind, window: &Window) -> Vec<BarSlot> {
    let contexts = items.iter().filter(|(m, _)| matches!(m, BarMenu::Context(_))).count();
    let mut seen_contexts = 0;
    items
        .iter()
        .map(|(menu, _)| {
            let label = menu.label();
            match menu {
                BarMenu::Static(id) => BarSlot { width: crate::chrome::dropdown_list::label_width(window, &menu.display().text, 14.) + 2. * MENU_BAR_BUTTON_PADDING, fold: static_fold_rank(kind, *id) },
                BarMenu::Context(_) => {
                    let first = seen_contexts == 0;
                    let rank = 1000 + (contexts - 1 - seen_contexts) as u32;
                    seen_contexts += 1;
                    BarSlot { width: bold_label_width(window, &label, 14.) + 2. * MENU_BAR_BUTTON_PADDING + if first { GROUP_LINE_WIDTH } else { 0.0 }, fold: Fold::Rank(rank) }
                }
            }
        })
        .collect()
}

/// How wide `label` is in the bold type of a context menu's name.
fn bold_label_width(window: &Window, label: &str, size: f32) -> f32 {
    let mut font = window.text_style().font();
    font.weight = FontWeight::BOLD;
    let run = TextRun { len: label.len(), font, color: rgb(crate::chrome::TEXT).into(), background_color: None, underline: None, strikethrough: None };
    f32::from(window.text_system().shape_line(SharedString::from(label.to_owned()), px(size), &[run], None).width)
}

/// Which menus show and which fold into the ▾ (spec 4.3): `(visible, hidden)` as indices of `slots`, each in order.
/// The menus that can fold do so by their rank, smallest first, until what is left (and the ▾ itself, when something
/// is folded) fits in `available`. Nothing is kept: the same widths give the same answer, so widening the window
/// brings the menus back as they were.
pub(crate) fn overflow_split(slots: &[BarSlot], available: f32, more_width: f32) -> (Vec<usize>, Vec<usize>) {
    let mut hidden: Vec<usize> = Vec::new();
    loop {
        let used: f32 = slots.iter().enumerate().filter(|(i, _)| !hidden.contains(i)).map(|(_, s)| s.width).sum::<f32>() + if hidden.is_empty() { 0.0 } else { more_width };
        if used <= available {
            break;
        }
        let next = slots
            .iter()
            .enumerate()
            .filter(|(i, s)| !hidden.contains(i) && matches!(s.fold, Fold::Rank(_)))
            .min_by_key(|(_, s)| match s.fold {
                Fold::Rank(r) => r,
                Fold::Never => u32::MAX,
            })
            .map(|(i, _)| i);
        let Some(next) = next else { break };
        hidden.push(next);
    }
    hidden.sort_unstable();
    let visible = (0..slots.len()).filter(|i| !hidden.contains(i)).collect();
    (visible, hidden)
}

impl SlideEditor {
    /// How tall a menu may be in this window.
    pub(crate) fn menu_room(window: &Window) -> f32 {
        f32::from(window.viewport_size().height) - BAR_BOTTOM - WINDOW_MARGIN
    }

    /// How tall a submenu opened from a row inside a menu may be in this window (#136).
    /// Submenus open below their parent row (e.g., row 8 in `ツール > 変更履歴の記録`),
    /// so we reserve room for the parent row offset so the submenu never overflows the bottom of the window.
    pub fn submenu_room(window: &Window) -> f32 {
        Self::submenu_room_for_viewport(f32::from(window.viewport_size().height))
    }

    pub(crate) fn submenu_room_for_viewport(viewport_h: f32) -> f32 {
        (viewport_h - 2.0 * WINDOW_MARGIN).max(ITEM_HEIGHT + MENU_PADDING)
    }

    /// Open menu `ix` of the bar (the static menus, then the context menus), closing the one that is open. `OVERFLOW_IX` is
    /// the ▾, whose menu lists the menus that do not fit as submenus.
    pub fn open_menu_at(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.bar_items();
        let (menu, entries) = if ix == DRAW_IX {
            (None, self.title_menu_entries(TitleMenu::Draw))
        } else if ix == OVERFLOW_IX {
            let (_, hidden) = self.bar_split(&items, window);
            let entries: Vec<MenuEntry> = hidden
                .into_iter()
                .filter_map(|i| items.get(i))
                .map(|(menu, entries)| MenuEntry::Submenu(Submenu { id: menu.key(), icon: None, label: menu.key().into(), items: entries.clone(), access: Some(menu.display()).filter(|d| d.key.is_some()) }))
                .collect();
            if entries.is_empty() {
                return;
            }
            (None, entries)
        } else {
            let Some((menu, entries)) = items.into_iter().nth(ix) else {
                return;
            };
            (Some(menu), entries)
        };
        // The lines' access keys (Windows and Linux): the menu's own level, which `fill_menu` is not asked to make.
        let mut entries = entries;
        crate::chrome::assign_access_keys(&mut entries);
        let width = fitted_menu_width(menu_width_for_entries(&entries).max(MENU_WIDTH), f32::from(window.viewport_size().width));
        // A menu of the bar hangs from the left of its name; the Draw button's follows the one rule of the drop-down buttons.
        let anchor = if ix == DRAW_IX {
            self.draw_button_bounds.get().map_or(Anchor::TopRight, |b| crate::chrome::dropdown_list::below_button(b, px(width), window.viewport_size()).1)
        } else {
            Anchor::TopLeft
        };
        // The same scrolling state, ▲▼ rows, wheel and scroll bar as a submenu's; only the room differs.
        let scroll = Rc::new(std::cell::RefCell::new(MenuScroll {
            entries: Rc::new(entries),
            start: 0,
            width,
            editor: cx.entity().downgrade(),
            popup: WeakEntity::new_invalid(),
            room: MenuRoom::BelowBar,
            bars: Default::default(),
            scrollbar: Default::default(),
            scroll_task: None,
        }));
        let built = scroll.clone();
        let popup = PopupMenu::build(window, cx, move |menu, window, cx| {
            built.borrow_mut().popup = cx.entity().downgrade();
            fill_menu_window(menu, &built, window, cx)
        });
        popup.focus_handle(cx).focus(window, cx);
        let dismiss = cx.subscribe_in(&popup, window, |this, popup, _: &DismissEvent, window, cx| {
            this.open_menu = None;
            this.menu_dismissed_at = Some(window.mouse_position());
            this.focus_back_after_menu(popup.read(cx).focus_handle(cx), window, cx);
            cx.notify();
        });
        self.open_menu = Some(OpenMenu { ix, anchor, menu, scroll, popup, _dismiss: dismiss });
        cx.notify();
    }

    /// The menus of the bar in the order of `open_menu_at`'s index: the static menus, then the context menus of the
    /// selection.
    pub fn bar_items(&self) -> Vec<(BarMenu, Vec<MenuEntry>)> {
        let bar = self.menu_bar_all();
        let mut items: Vec<(BarMenu, Vec<MenuEntry>)> = bar.statics.menus.into_iter().map(|(id, entries)| (BarMenu::Static(id), entries)).collect();
        items.extend(bar.contexts.into_iter().map(|(id, entries)| (BarMenu::Context(id), entries)));
        items
    }

    /// Which menus of `items` show in the bar and which are folded into the ▾ at this window's width.
    fn bar_split(&self, items: &[(BarMenu, Vec<MenuEntry>)], window: &Window) -> (Vec<usize>, Vec<usize>) {
        let layout = self.bar_layout(items, window);
        (layout.visible, layout.hidden)
    }

    /// A menu closed: the editor takes the keyboard back, unless the item's own handler moved it to something else
    /// (the sheet tab's name field of 編集 > シート > シートの名前を変更... (#174), a dialog's field): taking it
    /// back then would leave that field open but out of reach of the keys.
    pub fn focus_back_after_menu(&self, popup: FocusHandle, window: &mut Window, cx: &mut Context<Self>) {
        let moved_away = window.focused(cx).is_some_and(|h| h != popup && h != self.focus_handle && !popup.contains_focused(window, cx));
        if !moved_away {
            self.focus_handle.focus(window, cx);
        }
    }

    /// Close a context menu that is open when its target is gone, and tell why (spec 4.3); another open menu stays open
    /// when the context menus come and go. Called once a frame, before the bar is drawn.
    pub(crate) fn sync_context_menus(&mut self, cx: &mut Context<Self>) {
        let now = crate::menu_context::visible_contexts(&self.context_state());
        let previous = std::mem::replace(&mut self.bar_contexts, now.clone());
        if let Some(BarMenu::Context(open)) = self.open_menu.as_ref().and_then(|o| o.menu)
            && !now.contains(&open)
        {
            self.open_menu = None;
            let _ = previous;
            self.say_in_bar(crate::i18n::text("menu.context.gone"), cx);
            cx.notify();
        }
    }

    /// Show `text` in the bar for a few seconds.
    pub(crate) fn say_in_bar(&mut self, text: SharedString, cx: &mut Context<Self>) {
        self.menu_notice = Some(text);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_secs(NOTICE_SECONDS)).await;
            this.update(cx, |this, cx| {
                this.menu_notice = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.open_menu = None;
        cx.notify();
    }

    /// Show the menu's lines from `start` (moved into the possible range).
    pub fn scroll_menu_to(&mut self, start: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(scroll) = self.open_menu.as_ref().map(|open| open.scroll.clone()) else {
            return;
        };
        if scroll_menu_to(&scroll, start, window, cx) {
            cx.notify();
        }
    }

    /// The menu bar: the static menus, a thin line and the context menus of the selection (coloured), then the ▾ that holds
    /// the menus that do not fit (right after the last one that does). The Help menu is in the row like the others
    /// (the first to be folded into the ▾).
    pub(crate) fn render_menu_bar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.bar_items();
        let layout = self.bar_layout(&items, window);
        let open_ix = self.open_menu.as_ref().map(|o| o.ix);
        let logo_height = LOGO_HEIGHT_IN_BAR;
        let dark = crate::palette::is_dark();
        // The Help menu stands with the others, at the left (the user's decision, 2026-11-08); it is still the first folded into the ▾.
        let main: Vec<usize> = layout.visible.clone();
        let help: Option<usize> = None;
        let first_context = main.iter().copied().find(|i| matches!(items[*i].0, BarMenu::Context(_)));
        let kind = self.kind();
        let one = |cx: &mut Context<Self>, ix: usize| -> AnyElement {
            let menu = items[ix].0;
            let selected = open_ix == Some(ix);
            let id_key = menu.key();
            let tone = matches!(menu, BarMenu::Context(_)).then(|| context_tone(kind, dark));
            let button = menu_bar_button(SharedString::from(format!("menu-button-{id_key}")));
            let display = menu.display();
            let focused = self.bar_keys.focus == Some(ix);
            let shown_keys = display.key.is_some() && (self.bar_keys.alt_held || self.bar_keys.focus.is_some());
            let button = match tone {
                None if display.key.is_some() => {
                    // The key's letter is underlined only while Alt is held or the bar has the focus (F10).
                    let resolved = if shown_keys { display.clone() } else { crate::access_key::Resolved { underline: None, ..display.clone() } };
                    button.child(div().text_size(px(14.)).child(crate::access_key::label_element(&resolved)))
                }
                None => button.label(menu.label()).text_size(px(14.)),
                Some(tone) => button.child(div().text_size(px(14.)).font_weight(FontWeight::BOLD).text_color(crate::palette::doc(tone.fg)).child(menu.label())),
            };
            let button = button
                .selected(selected || focused)
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
                );
            let name = div()
                .id(SharedString::from(format!("menu-{id_key}")))
                .relative()
                .child(match tone {
                    Some(tone) => {
                        let mut bg: Hsla = crate::palette::doc(tone.bg).into();
                        bg.a = tone.bg_alpha;
                        div().rounded(px(MENU_BAR_BUTTON_RADIUS)).bg(bg).child(button).into_any_element()
                    }
                    None => button.into_any_element(),
                })
                // While a menu is open, the pointer on another menu's name opens that one.
                .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                    if *hovered && this.open_menu.as_ref().is_some_and(|o| o.menu != Some(menu)) {
                        this.open_menu_at(ix, window, cx);
                    }
                }))
                .when(selected, |d| d.child(self.render_open_menu()));
            // A context menu that appears fades in (not with the system's reduced motion).
            match tone {
                Some(_) if !cx.reduce_motion() => name
                    .with_animation(SharedString::from(format!("menu-fade-{id_key}")), Animation::new(std::time::Duration::from_millis(FADE_MS)), |d, t| d.opacity(t))
                    .into_any_element(),
                _ => name.into_any_element(),
            }
        };
        let overflow_open = open_ix == Some(OVERFLOW_IX);
        let muted: Hsla = rgb(crate::chrome::TEXT_SECONDARY).into();
        h_flex()
            .h(px(28.))
            .flex_shrink_0()
            .pl(px(layout.leading))
            .items_center()
            .bg(rgb(APP_BG))
            .when(layout.show_logo, |bar| {
                bar.child(div().relative().child({
                    // The logo opens the start window: a modal dialog leaves it available (`dialog_allows_pointer`).
                    let bounds = self.logo_bounds.clone();
                    crate::editor::bounds_observer(move |b, _, _| bounds.set(Some(b)))
                }).child({
                    let logo = menu_bar_button("editor-app-logo".into()).child(crate::chrome::brand_logo("editor-app-logo-image", logo_height, false));
                    #[cfg(not(target_arch = "wasm32"))]
                    let logo = logo.cursor_pointer().tooltip(crate::i18n::text("shell.start-window")).on_click(cx.listener(|_, _, _, cx| crate::app::open_editor(None, None, cx)));
                    // The Web version has one editor in a page: the start view is this page in another tab (K0016).
                    #[cfg(target_arch = "wasm32")]
                    let logo = logo.cursor_pointer().tooltip(crate::i18n::text("shell.start-window")).on_click(cx.listener(|_, _, _, _| crate::web::open_in_new_tab("")));
                    logo
                }))
            })
            .children(main.iter().copied().flat_map(|ix| {
                let mut parts: Vec<AnyElement> = Vec::new();
                if Some(ix) == first_context {
                    parts.push(div().w(px(1.)).h(px(16.)).mx(px(4.)).bg(crate::palette::shade(0.0, 0.15)).into_any_element());
                }
                parts.push(one(cx, ix));
                parts
            }).collect::<Vec<_>>())
            .when(!layout.hidden.is_empty(), |bar| {
                bar.child(
                    div().id("menu-overflow").relative().child(
                        menu_bar_button("menu-button-overflow".into())
                            .w(px(MORE_WIDTH))
                            .px(px(0.))
                            .child(crate::chrome::icon("arrow_drop_down", 16.0, rgb(crate::chrome::TEXT).into()))
                            .tooltip(crate::i18n::text("menu.overflow"))
                            .selected(overflow_open)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    let pressed_to_close = this.menu_dismissed_at.take().is_some_and(|p| p == event.position);
                                    if overflow_open || pressed_to_close {
                                        this.close_menu(cx);
                                    } else {
                                        this.open_menu_at(OVERFLOW_IX, window, cx);
                                    }
                                }),
                            ),
                    )
                    .when(overflow_open, |d| d.child(self.render_open_menu())),
                )
            })
            .child(div().flex_1())
            .when_some(self.menu_notice.clone(), |bar, text| bar.child(div().px(px(8.)).text_size(px(12.)).text_color(muted).child(text)))
            .when_some(help, |bar, ix| bar.child(one(cx, ix)))
            .child(div().w(px(BAR_TRAILING)))
    }

    /// Where the bar's parts go at this window's width: whether the logo shows, the room at its left, and which menus of
    /// `items` show and which are folded into the ▾.
    pub(crate) fn bar_layout(&self, items: &[(BarMenu, Vec<MenuEntry>)], window: &Window) -> BarLayout {
        let slots = bar_slots(items, self.kind(), window);
        let viewport = f32::from(window.viewport_size().width);
        let total: f32 = slots.iter().map(|s| s.width).sum();
        let logo_width = LOGO_HEIGHT_IN_BAR * crate::chrome::LOGO_ASPECT + 2.0 * MENU_BAR_BUTTON_PADDING;
        let show_logo = viewport >= 8. + total + 16. + LOGO_HEIGHT_IN_BAR * crate::chrome::LOGO_ASPECT + 12.;
        let leading = bar_leading(show_logo);
        let available = viewport - leading - if show_logo { logo_width } else { 0.0 } - BAR_TRAILING;
        let (visible, hidden) = overflow_split(&slots, available, MORE_WIDTH);
        BarLayout { show_logo, leading, visible, hidden }
    }

    /// The Draw button of the title row: a round icon button that opens the Ink panel (the same on the three screens; K0004).
    pub(crate) fn render_draw_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let open = self.open_menu.as_ref().is_some_and(|o| o.ix == DRAW_IX);
        let ink_open = crate::format::with_format!(self, ink_panel_open(self));
        let bounds = self.draw_button_bounds.clone();
        let anchor = self.open_menu.as_ref().map_or(Anchor::TopRight, |o| o.anchor);
        let button = div()
            .id("title-draw")
            .relative()
            .size(px(32.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .hover(|s| s.bg(crate::palette::shade(0.27, 0.08)))
            .when(open, |d| d.bg(crate::palette::shade(0.27, 0.08)))
            // Filled while the Ink panel is open (as the comments button is while its panel is).
            .child(if ink_open { Icon::new(Material::fill("draw")).size(px(20.)).text_color(rgb(crate::chrome::ICON)).into_any_element() } else { crate::chrome::icon("draw", 20.0, rgb(crate::chrome::ICON).into()).into_any_element() })
            .child(crate::editor::bounds_observer(move |b, _, _| bounds.set(Some(b))))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    // The press that closed the menu (a press outside it closes it first) does not open it again.
                    let pressed_to_close = this.menu_dismissed_at.take().is_some_and(|p| p == event.position);
                    if this.dialog_is_modal() {
                        return;
                    }
                    if open || pressed_to_close {
                        this.close_menu(cx);
                    } else if ink_open {
                        // Pressed while the Ink panel is open: it closes (the user, K0004, 2026-11-13).
                        crate::format::with_format!(this, close_ink_panel(this, cx));
                    } else {
                        // The button opens the Ink panel (the user, K0004, 2026-11-12); the Draw menu is the menu bar's.
                        this.close_menu(cx);
                        this.open_ink_panel_from_title(window, cx);
                    }
                }),
            )
            .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::i18n::text(if ink_open { "_shell.hide-ink-panel" } else { "_shell.show-ink-panel" })).build(window, cx))
            .when(open, |d| {
                let corner = div().absolute().top(px(32.));
                d.child(if anchor == Anchor::TopLeft { corner.left_0() } else { corner.right_0() }.child(self.render_open_menu()))
            });
        #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
        let button = crate::scenario::button_probe::observe_control(button, "title-draw".into());
        button.into_any_element()
    }

    /// The popup of the open menu below its name. Everything inside it (▲▼ rows, wheel, scroll bar) is the
    /// submenus' too (`MenuScroll`).
    pub(crate) fn render_open_menu(&self) -> impl IntoElement {
        let open = self.open_menu.as_ref().expect("a menu is open");
        deferred(
            anchored().anchor(open.anchor).snap_to_window_with_margin(px(8.)).child(
                div()
                    .occlude()
                    .top_1()
                    .relative()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .child(open.popup.clone()),
            ),
        )
        .with_priority(gpui_kit::base::POPUP_PRIORITY)
    }
}

/// Height of a menu bar item's hover/selected rounded rectangle.
pub const MENU_BAR_BUTTON_HEIGHT: f32 = 24.;
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
    let button = crate::scenario::button_probe::observe(button, id.into());
    button
}

#[cfg(test)]
mod tests {
    use super::{overflow_split, BarSlot, Fold, SlideEditor, visible};
    use crate::chrome::{MenuEntry, MenuItem};

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

    /// The menu a menu bar opens and every submenu go through one scrolling part (`MenuScroll`): one definition of
    /// each piece, so a change to one cannot leave the other behind (a second copy fails here).
    #[test]
    fn menu_and_submenu_share_one_scrolling_implementation() {
        let source = include_str!("menu_bar.rs");
        let production = &source[..source.find("#[cfg(test)]").unwrap()];
        for piece in ["struct MenuScroll", "fn menu_scroll_row(", "fn menu_hover_scroll(", "fn scroll_menu_to(state:", "fn scroll_menu_by(", "fn fill_menu_window("] {
            assert_eq!(production.matches(piece).count(), 1, "{piece}");
        }
        for old in ["struct SubmenuScroll", "fn scroll_row(", "fn fill_window(", "struct OpenMenuParts", "fn render_menu_scrollbar"] {
            assert!(!production.contains(old), "{old} is a second copy");
        }
        assert!(production.contains("menu_scrollbar::paint"), "the scroll bar is drawn by the shared part");
        assert_eq!(production.matches("menu_scrollbar::paint(").count(), 1, "and from one place");
    }

    /// The slots of a document's bar: File, Edit, Insert, Design, Review, View, Window, Help and two context menus (each 60 wide).
    fn slots() -> Vec<BarSlot> {
        let never = |w| BarSlot { width: w, fold: Fold::Never };
        let rank = |w, r| BarSlot { width: w, fold: Fold::Rank(r) };
        vec![never(60.), never(60.), never(60.), rank(60., 3), rank(60., 5), never(60.), never(60.), rank(60., 0), rank(60., 1001), rank(60., 1000)]
    }

    /// Help folds first, then the least used static menus, then the context menus from the right; the ▾ takes room too.
    #[test]
    fn menu_overflow_order() {
        let s = slots();
        let (visible, hidden) = overflow_split(&s, 1000., 24.);
        assert_eq!((visible.len(), hidden.len()), (10, 0), "room for all: nothing folded");
        // 10 menus are 600 wide. At 590 something folds, and the ▾ (24) must fit with what is left.
        let (_, hidden) = overflow_split(&s, 590., 24.);
        assert_eq!(hidden, vec![7], "Help first");
        let (_, hidden) = overflow_split(&s, 540., 24.);
        assert_eq!(hidden, vec![3, 7], "then the least used static menu (Design, rank 3, before Review, rank 5)");
        let (_, hidden) = overflow_split(&s, 480., 24.);
        assert_eq!(hidden, vec![3, 4, 7]);
        let (_, hidden) = overflow_split(&s, 400., 24.);
        assert_eq!(hidden, vec![3, 4, 7, 9], "the context menus go after the static ones, from the right (index 9 is the rightmost)");
        let (visible, hidden) = overflow_split(&s, 100., 24.);
        assert_eq!(visible, vec![0, 1, 2, 5, 6], "File, Edit, Insert, View and Window are never folded");
        assert_eq!(hidden, vec![3, 4, 7, 8, 9]);
    }

    /// The same widths give the same answer: a window made narrower and wider again shows what it showed.
    #[test]
    fn menu_overflow_restores() {
        let s = slots();
        let before = overflow_split(&s, 700., 24.);
        let _narrow = overflow_split(&s, 300., 24.);
        assert_eq!(overflow_split(&s, 700., 24.), before);
        // With nothing folded the ▾ takes no room.
        assert_eq!(overflow_split(&s, 600., 24.).1, Vec::<usize>::new());
    }

    /// The ▾ is where the last menu that is shown ends: what folds is only the menus after the ones that stay, never a gap
    /// in the middle of the always-shown ones.
    #[test]
    fn menu_overflow_hides_by_rank_only() {
        let s = slots();
        let (visible, hidden) = overflow_split(&s, 250., 24.);
        for i in &hidden {
            assert!(matches!(s[*i].fold, Fold::Rank(_)), "{i} cannot fold");
        }
        assert_eq!(visible.len() + hidden.len(), s.len());
    }
}
