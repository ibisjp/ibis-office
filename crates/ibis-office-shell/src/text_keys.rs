// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The keys while a shape's text is edited (the caret, the selection, deleting, Enter, the format shortcuts), on every
//! screen that has shapes, and the keys the menus name as their shortcuts (`run_menu_shortcut`: every shortcut the menus
//! show works, not only the ones bound one by one). Characters come through the text input (`text_edit.rs`), not here.

use gpui_kit::*;

use crate::editor::SlideEditor;
use crate::format::with_format;

/// ⌘B, ⌘I or ⌘U while a shape's text is edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharacterToggle {
    Bold,
    Italic,
    Underline,
}

/// A shortcut as the menus write it (`⌥⇧⌘↑`): its modifiers (bits: ⌘ 1, ⌃ 2, ⌥ 4, ⇧ 8) and key.
pub fn parse_shortcut(label: &str) -> Option<(u8, String)> {
    let mut bits = 0u8;
    let mut key = String::new();
    for c in label.chars() {
        match c {
            '⌘' => bits |= 1,
            '⌃' => bits |= 2,
            '⌥' => bits |= 4,
            '⇧' => bits |= 8,
            c => key.extend(c.to_uppercase()),
        }
    }
    (!key.is_empty()).then_some((bits, key))
}

/// The key pressed in the same form as [`parse_shortcut`].
pub fn shortcut_of(k: &Keystroke) -> Option<(u8, String)> {
    let m = &k.modifiers;
    let bits = u8::from(m.platform) | u8::from(m.control) << 1 | u8::from(m.alt) << 2 | u8::from(m.shift) << 3;
    let key = match k.key.as_str() {
        "up" => "↑".to_string(),
        "down" => "↓".to_string(),
        "left" => "←".to_string(),
        "right" => "→".to_string(),
        "enter" => "↩".to_string(),
        "backspace" => "⌫".to_string(),
        "delete" => "⌦".to_string(),
        other if !other.is_empty() => other.to_uppercase(),
        _ => return None,
    };
    Some((bits, key))
}

impl SlideEditor {
    /// Keys while a text is edited: the caret, selection, deleting,
    /// Enter, and the format shortcuts. Characters come through the text
    /// input (`text_edit.rs`), not here.
    pub fn on_text_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // The format's own keys come first (a presentation's path being drawn, its outline's line being edited).
        if with_format!(self, text_key_early(self, event, window, cx)) {
            return;
        }
        if self.text_editing().is_none() {
            return;
        }
        let k = &event.keystroke;
        let command = k.modifiers.platform || (k.modifiers.control && !cfg!(target_os = "macos"));
        let key = k.key.as_str();
        match key {
            "left" | "right" | "up" | "down" | "home" | "end" => {
                cx.stop_propagation();
                if matches!(key, "up" | "down") && !command {
                    self.move_caret_vertically(key, k.modifiers.shift, cx);
                } else if k.modifiers.alt && matches!(key, "left" | "right") {
                    // ⌥← ⌥→: by words.
                    if let Some(to) = self.word_target(key == "right") {
                        self.shape_move_caret(to, k.modifiers.shift, cx);
                    }
                } else {
                    // ⌘← ⌘→ go to the line's start and end (⌘↑ ⌘↓ to the text's), as in any Mac text.
                    let (key, whole) = match (key, command) {
                        ("left", true) => ("home", false),
                        ("right", true) => ("end", false),
                        (key, command) => (key, command),
                    };
                    // At the edge of a box of linked text boxes, the caret goes on in the next (or the previous) box.
                    if matches!(key, "left" | "right") && !whole && !k.modifiers.shift && self.move_caret_across_boxes(key, cx) {
                        return;
                    }
                    if let Some(to) = self.caret_target(key, whole, k.modifiers.shift) {
                        self.shape_move_caret(to, k.modifiers.shift, cx);
                    }
                }
            }
            // The Mac's own text keys: ⌃A ⌃E line start and end, ⌃B ⌃F left and right, ⌃P ⌃N up and down,
            // ⌃D forward delete, ⌃H backspace, ⌃K delete to the line's end.
            "a" | "e" | "b" | "f" | "p" | "n" | "d" | "h" | "k" if crate::app::is_mac() && k.modifiers.control && !k.modifiers.platform && !k.modifiers.alt => {
                cx.stop_propagation();
                match key {
                    "d" => self.delete_text(true, cx),
                    "h" => self.delete_text(false, cx),
                    "k" => self.delete_to(true, true, cx),
                    _ => {
                        let target = match key {
                            "a" => "home",
                            "e" => "end",
                            "b" => "left",
                            "f" => "right",
                            "p" => "up",
                            _ => "down",
                        };
                        if matches!(target, "up" | "down") {
                            self.move_caret_vertically(target, k.modifiers.shift, cx);
                        } else if let Some(to) = self.caret_target(target, false, k.modifiers.shift) {
                            self.shape_move_caret(to, k.modifiers.shift, cx);
                        }
                    }
                }
            }
            "backspace" | "delete" => {
                cx.stop_propagation();
                if k.modifiers.alt || command {
                    // ⌥⌫: a word; ⌘⌫: to the start of the line.
                    self.delete_to(key == "delete", command, cx);
                } else {
                    self.delete_text(key == "delete", cx);
                }
            }
            "enter" if k.modifiers.shift => {
                cx.stop_propagation();
                self.insert_line_break(cx);
            }
            "enter" => {
                cx.stop_propagation();
                self.shape_type_text("\n", cx);
            }
            "escape" => {
                cx.stop_propagation();
                self.end_text_edit(window, cx);
            }
            "c" | "x" if command && !k.modifiers.alt => {
                cx.stop_propagation();
                self.copy_edited_text(key == "x", cx);
            }
            "v" if command && !k.modifiers.alt => {
                cx.stop_propagation();
                self.paste_edited_text(cx);
            }
            "a" if command => {
                cx.stop_propagation();
                let len = self.edited_text().map_or(0, |t| t.chars().count());
                self.shape_move_caret(0, false, cx);
                self.shape_move_caret(len, true, cx);
                // In linked text boxes the text of all the boxes is selected.
                self.select_all_in_chain();
            }
            "b" | "i" | "u" if command => {
                cx.stop_propagation();
                let toggle = match key {
                    "b" => CharacterToggle::Bold,
                    "i" => CharacterToggle::Italic,
                    _ => CharacterToggle::Underline,
                };
                // A format whose text boxes hold its own runs (a document's) changes them by its own means.
                if !with_format!(self, toggle_text_characters(self, toggle, cx)) {
                    use ibis_doc_model::CharacterFormat;
                    match toggle {
                        CharacterToggle::Bold => self.toggle_characters(window, cx, |b, r| b.with_run(r).bold, |on| CharacterFormat { bold: Some(on), ..Default::default() }),
                        CharacterToggle::Italic => self.toggle_characters(window, cx, |b, r| b.with_run(r).italic, |on| CharacterFormat { italic: Some(on), ..Default::default() }),
                        CharacterToggle::Underline => self.toggle_characters(window, cx, |_, r| r.underline, |on| CharacterFormat { underline: Some(on), ..Default::default() }),
                    }
                }
            }
            _ => {
                // A key the menus name as their shortcut does what the menu item does.
                if self.run_menu_shortcut(&event.keystroke, window, cx) {
                    cx.stop_propagation();
                }
            }
        }
    }

    /// The menu item whose shortcut is the key pressed (one with ⌘, ⌃ or ⌥, or a function key), run if it is
    /// enabled: every shortcut the menus show works, not only the ones bound one by one. `true` if one ran.
    pub fn run_menu_shortcut(&mut self, k: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let m = &k.modifiers;
        let function = k.key.len() >= 2 && k.key.starts_with('f') && k.key[1..].chars().all(|c| c.is_ascii_digit());
        if !(m.platform || m.control || m.alt || function) {
            return false;
        }
        let Some(want) = shortcut_of(k) else { return false };
        let menus = self.menus();
        let mut found = None;
        fn find(entries: &[crate::chrome::MenuEntry], want: &(u8, String), found: &mut Option<crate::chrome::Handler>) {
            use crate::chrome::MenuEntry;
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) if found.is_none() && item.enabled() && item.shortcut.and_then(parse_shortcut).as_ref() == Some(want) => *found = item.handler.clone(),
                    MenuEntry::Submenu(sub) => find(&sub.items, want, found),
                    _ => {}
                }
            }
        }
        for (_, entries) in &menus.menus {
            find(entries, &want, &mut found);
        }
        match found {
            Some(handler) => {
                handler(self, window, cx);
                true
            }
            None => false,
        }
    }
}
