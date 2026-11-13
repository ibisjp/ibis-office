// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The keys of the menu bar (menu bar redesign, stage 8; `tasks/doc/menu-redesign/implementation-spec.md` section 5): on Windows and
//! Linux only (`access_key::supported()` is the one place that says so; on macOS and in the browser nothing here runs).
//!
//! * Alt+letter opens the menu that has the letter (`MenuId::access_letter`), also one folded into the ▾ (the ▾ opens, with the
//!   folded menus in it); Alt+D opens the Draw button's menu; Alt+J goes round the coloured menus, one more at each press.
//! * F10 puts the focus on the bar: ← and → move along the bar in the order it is drawn (the static menus, the coloured menus, the ▾,
//!   then Help at the right end), ↓ or Enter opens the one the focus is on, Esc gives the focus back. The access letters are underlined
//!   while Alt is held and while the bar has the focus.
use gpui_kit::*;

use crate::access_key;
use crate::editor::SlideEditor;
use crate::menu_bar::{BarMenu, DRAW_IX, OVERFLOW_IX};
use crate::chrome::MenuId;

/// What the keys know about the bar.
#[derive(Default)]
pub struct BarKeys {
    /// Alt is held (the letters are underlined).
    pub(crate) alt_held: bool,
    /// The bar has the focus (F10): the index (into the bar's menus; `OVERFLOW_IX` for the ▾) the focus is on.
    pub focus: Option<usize>,
    /// The coloured menu Alt+J opened last (an index into the bar's menus), so that the next press goes to the next one.
    pub(crate) last_context: Option<usize>,
}

impl SlideEditor {
    /// The parts of the bar in the order they are drawn, as indices into the bar's menus (`OVERFLOW_IX` for the ▾): the visible
    /// static menus except Help, the visible coloured menus, the ▾ (when something is folded), then Help (when it shows).
    pub(crate) fn bar_navigation(&self, window: &Window) -> Vec<usize> {
        let items = self.bar_items();
        let layout = self.bar_layout(&items, window);
        let is_help = |ix: usize| matches!(items[ix].0, BarMenu::Static(MenuId::Help));
        let mut order: Vec<usize> = layout.visible.iter().copied().filter(|ix| !is_help(*ix)).collect();
        if !layout.hidden.is_empty() {
            order.push(OVERFLOW_IX);
        }
        order.extend(layout.visible.iter().copied().filter(|ix| is_help(*ix)));
        order
    }

    /// The keys of the bar are free to act: this OS has them, no dialog or message has the keys.
    fn bar_keys_active(&self) -> bool {
        access_key::supported() && self.has_document && !self.interaction_layer().is_confirmation() && !self.dialog_is_modal()
    }

    /// Open the menu at index `ix` of the bar's menus (a folded one through the ▾).
    fn open_bar_menu(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.bar_items();
        let layout = self.bar_layout(&items, window);
        self.bar_keys.focus = None;
        let target = if layout.hidden.contains(&ix) { OVERFLOW_IX } else { ix };
        self.open_menu_at(target, window, cx);
    }

    /// A key press, for the bar. True when the bar took it.
    pub(crate) fn bar_key_down(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.bar_keys_active() {
            return false;
        }
        let m = &key.modifiers;
        let plain = !m.alt && !m.control && !m.platform && !m.shift;
        // The bar has the focus: the arrows, Enter and Esc are its; any other key gives the focus back and goes on.
        if let Some(at) = self.bar_keys.focus {
            let order = self.bar_navigation(window);
            let position = order.iter().position(|ix| *ix == at).unwrap_or(0);
            if plain {
                match key.key.as_str() {
                    "right" | "left" if !order.is_empty() => {
                        let next = if key.key == "right" { (position + 1) % order.len() } else { (position + order.len() - 1) % order.len() };
                        self.bar_keys.focus = Some(order[next]);
                        cx.notify();
                        return true;
                    }
                    "down" | "enter" | "space" => {
                        self.open_bar_menu(at, window, cx);
                        return true;
                    }
                    "escape" | "f10" => {
                        self.bar_keys.focus = None;
                        cx.notify();
                        return true;
                    }
                    _ => {}
                }
            }
            self.bar_keys.focus = None;
            cx.notify();
        }
        if key.key == "f10" && !m.alt && !m.control && !m.platform {
            let order = self.bar_navigation(window);
            if let Some(first) = order.first().copied() {
                self.close_menu(cx);
                self.bar_keys.focus = Some(first);
                cx.notify();
                return true;
            }
            return false;
        }
        // Alt+letter.
        if m.alt && !m.control && !m.platform && !m.shift {
            let mut chars = key.key.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else { return false };
            let letter = c.to_ascii_lowercase();
            if letter == 'd' {
                self.close_menu(cx);
                self.open_menu_at(DRAW_IX, window, cx);
                return true;
            }
            let items = self.bar_items();
            if letter == 'j' {
                // The coloured menus, one after another; with none, nothing happens.
                let contexts: Vec<usize> = items.iter().enumerate().filter(|(_, (m, _))| matches!(m, BarMenu::Context(_))).map(|(ix, _)| ix).collect();
                if contexts.is_empty() {
                    return false;
                }
                let next = match self.bar_keys.last_context.and_then(|last| contexts.iter().position(|ix| *ix == last)) {
                    Some(at) => contexts[(at + 1) % contexts.len()],
                    None => contexts[0],
                };
                self.bar_keys.last_context = Some(next);
                self.close_menu(cx);
                self.open_bar_menu(next, window, cx);
                return true;
            }
            if let Some(ix) = items.iter().position(|(m, _)| matches!(m, BarMenu::Static(id) if id.access_letter() == Some(letter))) {
                self.close_menu(cx);
                self.open_bar_menu(ix, window, cx);
                return true;
            }
        }
        false
    }
}
