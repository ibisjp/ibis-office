// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A grid of pictures in a menu (2026-11-03): the items that have a picture ([`MenuItem::thumb`]) are drawn as the
//! picture over the name, [`COLUMNS`] to a row, instead of one line each (the 挿入 > グラフ submenu of a
//! spreadsheet). The items stay plain `MenuItem`s for everything that looks them up (ids, the menu search, the
//! scenarios); `group_tiles` turns them into rows only when a menu is filled.
//!
//! A row is one item of the popup, so the arrow keys up and down move between rows as between lines, Enter
//! chooses the tile the row is on, and the left and right arrows move between the tiles of the selected row
//! (the left arrow in the first column still closes the submenu). The pointer chooses the tile it is on.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::chrome::{MenuEntry, MenuItem, TEXT};
use crate::editor::SlideEditor;
use crate::palette::rgb;

/// Tiles in a row.
pub(crate) const COLUMNS: usize = 2;
/// A tile's width and its padding; the picture is the width inside the padding, 4:3.
const TILE_WIDTH: f32 = 124.0;
const TILE_PADDING: f32 = 4.0;
const THUMB_WIDTH: f32 = TILE_WIDTH - 2.0 * TILE_PADDING;
const THUMB_HEIGHT: f32 = THUMB_WIDTH * 3.0 / 4.0;
/// The tile width of a grid of small pictures (the shapes, K0006 F): the picture is half the chart's picture ([`THUMB_WIDTH`] / 2), with the same padding.
pub const SMALL_TILE_WIDTH: f32 = THUMB_WIDTH / 2.0 + 2.0 * TILE_PADDING;
/// The name under the picture: up to two lines of 12 px text.
const LABEL_GAP: f32 = 2.0;
const LABEL_HEIGHT: f32 = 30.0;
const TILE_HEIGHT: f32 = TILE_PADDING + THUMB_HEIGHT + LABEL_GAP + LABEL_HEIGHT + TILE_PADDING;
/// What `PopupMenu` adds around an item's content: 2 px between items (the rows of a menu are 2 px apart: measured with the scenario
/// `menus_20261109_chart_submenu`, the tiles' rows are `TILE_HEIGHT + 2` apart; an earlier 8 px for padding made the scroll bar
/// reach out of the popup); and 4 px (`p_1`) plus 8 px (`px_2`) at each side.
const ROW_EXTRA_HEIGHT: f32 = 2.0;
const POPUP_EXTRA_WIDTH: f32 = 24.0;

/// How tall a row of tiles is in a menu, the 2 px gap to the next line included (the scrolling of a long menu
/// counts it as the height of a line).
pub const ROW_HEIGHT: f32 = TILE_HEIGHT + ROW_EXTRA_HEIGHT;

/// The width of the tile of `item`: its own `tile_width`, or [`TILE_WIDTH`].
fn tile_width_of(item: &MenuItem) -> f32 {
    if item.tile_width > 0.0 { item.tile_width } else { TILE_WIDTH }
}

/// The height of a tile that is `width` wide: the 4:3 picture, the name and the padding.
fn tile_height_for(width: f32) -> f32 {
    TILE_PADDING + (width - 2.0 * TILE_PADDING) * 3.0 / 4.0 + LABEL_GAP + LABEL_HEIGHT + TILE_PADDING
}

/// How tall a row of `items` is (the 2 px gap to the next line included).
pub(crate) fn row_height_of(items: &[MenuItem]) -> f32 {
    tile_height_for(items.first().map_or(TILE_WIDTH, tile_width_of)) + ROW_EXTRA_HEIGHT
}

/// How many tiles the row of `item` has.
fn columns_of(item: &MenuItem) -> usize {
    if item.tile_columns == 0 { COLUMNS } else { item.tile_columns }
}

/// How wide a menu with a row of `n` tiles of the default width is at least.
pub(crate) fn row_width(n: usize) -> f32 {
    n.max(1) as f32 * TILE_WIDTH + POPUP_EXTRA_WIDTH
}

/// How wide a menu with the row `items` is at least (the row's own `tile_columns` of tiles, a blank one included).
pub(crate) fn row_width_of(items: &[MenuItem]) -> f32 {
    let columns = items.first().map_or(COLUMNS, columns_of).max(items.len());
    columns as f32 * items.first().map_or(TILE_WIDTH, tile_width_of) + POPUP_EXTRA_WIDTH
}

/// `entries` with every run of items that have a picture made into rows of [`COLUMNS`] tiles (or the items' own `tile_columns`) (separators inside
/// such a run are dropped: the grid is one block).
pub(crate) fn group_tiles(entries: Vec<MenuEntry>) -> Vec<MenuEntry> {
    let is_tile = |entry: &MenuEntry| matches!(entry, MenuEntry::Item(item) if item.thumb.is_some());
    let mut out: Vec<MenuEntry> = Vec::with_capacity(entries.len());
    let mut run: Vec<MenuItem> = Vec::new();
    let flush = |run: &mut Vec<MenuItem>, out: &mut Vec<MenuEntry>| {
        let columns = run.first().map_or(COLUMNS, columns_of);
        for row in run.chunks(columns) {
            out.push(MenuEntry::Tiles(row.to_vec()));
        }
        run.clear();
    };
    let mut entries = entries.into_iter().peekable();
    while let Some(entry) = entries.next() {
        match entry {
            MenuEntry::Item(item) if item.thumb.is_some() => {
                // A run of tiles is one grid: a change of the number of columns starts another.
                if run.first().is_some_and(|first| columns_of(first) != columns_of(&item)) {
                    flush(&mut run, &mut out);
                }
                run.push(item)
            }
            MenuEntry::Separator if !run.is_empty() && entries.peek().is_some_and(is_tile) => {}
            other => {
                flush(&mut run, &mut out);
                out.push(other);
            }
        }
    }
    flush(&mut run, &mut out);
    out
}

/// The row of tiles `items` (at most [`COLUMNS`]) as an item of a popup menu.
pub(crate) fn tile_row(items: &[MenuItem], editor: &WeakEntity<SlideEditor>) -> PopupMenuItem {
    let items: Rc<Vec<MenuItem>> = Rc::new(items.to_vec());
    // The tile the row is on: the one under the pointer, or the one the arrow keys moved to.
    let active: Rc<Cell<usize>> = Rc::new(Cell::new(0));
    let by_pointer: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    let was_selected: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    let row = {
        let (key_active, key_count) = (active.clone(), items.len());
        let (items, active, by_pointer, was_selected, editor) = (items.clone(), active.clone(), by_pointer.clone(), was_selected.clone(), editor.clone());
        PopupMenuItem::element_with_selection(move |selected, window, cx| {
            // A row the keys come to starts at its first tile; one the pointer comes to, at the tile under it.
            if !selected {
                was_selected.set(false);
                by_pointer.set(false);
            } else if !was_selected.replace(true) && !by_pointer.get() {
                active.set(0);
            }
            let count = items.len();
            let blank_width = items.first().map_or(TILE_WIDTH, tile_width_of);
            let blank_height = tile_height_for(blank_width);
            let current = active.get().min(count.saturating_sub(1));
            let mut tiles = h_flex().w_full().items_start();
            for (index, item) in items.iter().enumerate() {
                let is_current = selected && current == index && item.enabled();
                tiles = tiles.child(tile(item, index, is_current, &active, &by_pointer, &editor, cx));
            }
            for index in count..items.first().map_or(COLUMNS, columns_of) {
                // A blank in the last row: the pointer there chooses nothing.
                let (active, by_pointer) = (active.clone(), by_pointer.clone());
                tiles = tiles.child(div().id(SharedString::from(format!("menu-tile-blank-{index}"))).w(px(blank_width)).h(px(blank_height)).on_hover(move |hovered, _, _| {
                    if *hovered {
                        active.set(index);
                        by_pointer.set(true);
                    }
                }));
            }
            let _ = window;
            div().relative().w_full().child(tiles)
        })
        .hoverable(false)
        // The left and right arrows move between the tiles of the selected row; the left arrow in the first column
        // is left to the menu (it closes the submenu), and the right arrow in the last column does nothing.
        .on_horizontal_key({
            let (active, count) = (key_active, key_count);
            move |right, window, _| {
                let now = active.get().min(count.saturating_sub(1));
                let next = match (right, now) {
                    (true, n) if n + 1 < count => n + 1,
                    (false, n) if n > 0 => n - 1,
                    _ => return right,
                };
                active.set(next);
                window.refresh();
                true
            }
        })
    };
    // Enter, and a click on a tile: the tile the row is on (the popup runs this, then closes the menu).
    let (items, active, editor) = (items, active, editor.clone());
    row.on_click(move |_, window, cx| {
        let Some(handler) = items.get(active.get()).filter(|item| item.enabled()).and_then(|item| item.handler.clone()) else { return };
        editor.update(cx, |this, cx| {
            this.ruler_settings_open = false;
            handler(this, window, cx)
        }).ok();
    })
}

/// One tile: the picture over the name.
fn tile(item: &MenuItem, index: usize, current: bool, active: &Rc<Cell<usize>>, by_pointer: &Rc<Cell<bool>>, editor: &WeakEntity<SlideEditor>, cx: &mut App) -> impl IntoElement {
    let enabled = item.enabled();
    let (tile_width, tile_height) = (tile_width_of(item), tile_height_for(tile_width_of(item)));
    let (thumb_width, thumb_height) = (tile_width - 2.0 * TILE_PADDING, (tile_width - 2.0 * TILE_PADDING) * 3.0 / 4.0);
    let label = crate::chrome::menu_item_display_label(item);
    let picture = match (&item.thumb, editor.upgrade()) {
        (Some(thumb), Some(editor)) => thumb(editor.read(cx), thumb_width),
        _ => div().into_any_element(),
    };
    let (active, by_pointer) = (active.clone(), by_pointer.clone());
    let tooltip = label.clone();
    let accent = cx.theme().tokens.accent;
    let tile = v_flex()
        .id(SharedString::from(format!("menu-item-{}", item.id)))
        .w(px(tile_width))
        .h(px(tile_height))
        .p(px(TILE_PADDING))
        .gap(px(LABEL_GAP))
        .rounded(px(4.))
        .when(current, |d| d.bg(accent))
        .when(!enabled, |d| d.opacity(0.45))
        .on_hover(move |hovered, _, _| {
            if *hovered {
                active.set(index);
                by_pointer.set(true);
            }
        })
        .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx))
        .child(
            div()
                .relative()
                .flex_none()
                .w(px(thumb_width))
                .h(px(thumb_height))
                .overflow_hidden()
                .rounded(px(2.))
                .border_1()
                .border_color(crate::palette::shade(0.0, 0.2))
                .bg(rgb(0xffffff))
                .child(picture),
        )
        .child(div().h(px(LABEL_HEIGHT)).overflow_hidden().text_size(px(12.)).line_height(px(15.)).text_color(rgb(TEXT)).child(label));
    // The scenarios find a tile by its item's id, as they find a line.
    #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
    let tile = {
        let id = SharedString::from(format!("menu-item-{}", item.id));
        crate::scenario::button_probe::observe_control(tile, id.into())
    };
    tile
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile_item(id: &'static str) -> MenuEntry {
        MenuItem::new(id, id).on(|_, _, _| {}).thumb(|_, _| div().into_any_element()).into()
    }

    // `use gpui_kit::*` in the parent brings gpui's own `test` attribute into scope through `use super::*`; it expands without end on a plain test. The std one is named in full.
    #[core::prelude::v1::test]
    fn items_with_a_picture_become_rows_of_two_and_separators_between_them_go() {
        let entries = vec![
            MenuItem::new("first", "first").on(|_, _, _| {}).into(),
            MenuEntry::Separator,
            tile_item("a"),
            tile_item("b"),
            MenuEntry::Separator,
            tile_item("c"),
        ];
        let grouped = group_tiles(entries);
        let shape: Vec<String> = grouped
            .iter()
            .map(|e| match e {
                MenuEntry::Item(i) => format!("item {}", i.id),
                MenuEntry::Separator => "separator".into(),
                MenuEntry::Tiles(t) => format!("tiles {}", t.iter().map(|i| i.id).collect::<Vec<_>>().join("+")),
                _ => "other".into(),
            })
            .collect();
        assert_eq!(shape, ["item first", "separator", "tiles a+b", "tiles c"]);
    }

    // `use gpui_kit::*` in the parent brings gpui's own `test` attribute into scope through `use super::*`; it expands without end on a plain test. The std one is named in full.
    #[core::prelude::v1::test]
    fn a_row_is_as_tall_as_its_tiles_and_the_menu_as_wide_as_its_row() {
        assert_eq!(ROW_HEIGHT, 4.0 + 87.0 + 2.0 + 30.0 + 4.0 + 2.0);
        assert_eq!(row_width(COLUMNS), 2.0 * TILE_WIDTH + 24.0);
        // The small tiles' picture is half the chart's.
        assert_eq!(SMALL_TILE_WIDTH - 2.0 * TILE_PADDING, THUMB_WIDTH / 2.0);
    }
}
