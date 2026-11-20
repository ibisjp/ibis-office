// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The order of the toolbar's buttons the user chose (ツール > 設定… > ドキュメント／スプレッドシート／プレゼンテーション >
//! ツールバーのカスタマイズ, GitHub issue #241).
//!
//! The toolbar's right half differs with what is selected, so an order is kept for each *scene* of each format (the scenes of a
//! format: `scenes`; the scene the open document is in: `FormatEditor::toolbar_scene`). An order is a list of keys:
//! - a button of the toolbar: its id (`ToolButton::id`; a drop-down box's id; `font-size` for the size box);
//! - `|`: a separator (as many as the user put);
//! - `cmd:<id>`: a menu command (a menu item's id) as a button of its own;
//! - `menu:<id>`: a submenu as a button with a ▼ that opens it (a submenu is put there only so, never as a plain button).
//!
//! It is kept in the settings (the settings file; localStorage in the browser) as `toolbar.<extension>.<scene>`, the keys joined
//! by `,`. No such setting: the buttons stay as the format makes them. The two buttons in `LOCKED` are always first, whatever the
//! order says (so every command stays reachable through メニューを検索).
use crate::chrome::{MenuEntry, Menus, ToolButton, ToolItem};
use crate::i18n::text;
use crate::settings;

/// The separator's key in an order.
pub const SEPARATOR: &str = "|";
/// The prefix of a menu command's key, and of a submenu's (a button with a ▼).
pub const COMMAND: &str = "cmd:";
pub const SUBMENU: &str = "menu:";
/// A set of buttons that go together: the three buttons of an alignment and the ▼ that has the rest of it. It is one part of an order
/// (`key`), that the toolbar draws as its buttons side by side, and the dialog lists as one.
pub struct ButtonSet {
    pub key: &'static str,
    /// The ids of its buttons, in the order they are drawn (the ▼ is the last of those the format has).
    pub buttons: &'static [&'static str],
    pub icon: &'static str,
    pub name: &'static str,
}

pub const SETS: &[ButtonSet] = &[
    ButtonSet { key: "set:align-h", buttons: &["align-left", "align-center", "align-right", "align", "halign"], icon: "format_align_left", name: "toolbar.customize.set.align-h" },
    ButtonSet { key: "set:align-v", buttons: &["valign-top", "valign-center", "valign-bottom", "vertical-align", "valign"], icon: "vertical_align_top", name: "toolbar.customize.set.align-v" },
];

/// The set that has the button `id`.
fn set_of(id: &str) -> Option<&'static ButtonSet> {
    SETS.iter().find(|set| set.buttons.contains(&id))
}

/// The buttons that always come first and cannot be taken out.
pub const LOCKED: [&str; 2] = ["menu-search", "format-paint"];

/// A scene of a format: what the toolbar is while it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scene {
    /// The id the format's `toolbar_scene` answers and the settings key has.
    pub id: &'static str,
    /// The id of its name in the language files.
    pub name: &'static str,
}

const DOCX_SCENES: &[Scene] = &[
    Scene { id: "none", name: "toolbar.scene.doc.none" },
    Scene { id: "table", name: "toolbar.scene.doc.table" },
    Scene { id: "image", name: "toolbar.scene.doc.image" },
];
const XLSX_SCENES: &[Scene] = &[Scene { id: "cell", name: "toolbar.scene.sheet.cell" }];
const PPTX_SCENES: &[Scene] = &[
    Scene { id: "none", name: "toolbar.scene.slide.none" },
    Scene { id: "shape", name: "toolbar.scene.slide.shape" },
    Scene { id: "image", name: "toolbar.scene.slide.image" },
    Scene { id: "video", name: "toolbar.scene.slide.video" },
    Scene { id: "line", name: "toolbar.scene.slide.line" },
    Scene { id: "table", name: "toolbar.scene.slide.table" },
];

/// The クイック アクセス ツール バー is edited by the same dialog as a format's toolbar: it has one order, for every format (`qat.rs`).
const QAT_SCENES: &[Scene] = &[Scene { id: "all", name: "settings.qat" }];

/// The scenes of the format with the extension `extension` (`docx`, `xlsx`, `pptx`), in the order the dialog lists them.
pub fn scenes(extension: &str) -> &'static [Scene] {
    match extension {
        "docx" => DOCX_SCENES,
        "xlsx" => XLSX_SCENES,
        "pptx" => PPTX_SCENES,
        "qat" => QAT_SCENES,
        _ => &[],
    }
}

/// The settings key of a scene's order.
pub fn setting_key(extension: &str, scene: &str) -> String {
    format!("toolbar.{extension}.{scene}")
}

/// The order the user saved for a scene; `None` when none is (the buttons stay as they are made).
pub fn saved(extension: &str, scene: &str) -> Option<Vec<String>> {
    let value = settings::get(&setting_key(extension, scene))?;
    // A command or submenu kept under an id that was renamed (K0038) is read as the new id.
    let current = |key: &str| match key.split_once(':') {
        Some((kind, id)) => format!("{kind}:{}", crate::menu_id_migration::current(id)),
        None => key.to_string(),
    };
    Some(value.split(',').map(str::trim).filter(|k| !k.is_empty()).map(current).collect())
}

/// Keep an order for a scene (an empty order is kept as the empty line, which is not "none saved").
pub fn save(extension: &str, scene: &str, keys: &[String]) {
    settings::set(&setting_key(extension, scene), &keys.join(","));
}

/// Take a scene's saved order away: its buttons are the format's own again.
pub fn reset(extension: &str, scene: &str) {
    settings::remove(&setting_key(extension, scene));
}

/// The key of a part of the toolbar.
pub fn item_key(item: &ToolItem) -> &'static str {
    match item {
        ToolItem::Button(b) => b.id,
        ToolItem::Separator => SEPARATOR,
        ToolItem::Select { id, .. } => id,
        ToolItem::FontSize { .. } => "font-size",
    }
}

/// The keys of the toolbar as a format makes it, without the locked buttons: the order a scene has until the user changes it.
pub fn default_keys(items: &[ToolItem]) -> Vec<String> {
    let keys: Vec<&str> = items.iter().filter(|i| !matches!(i, ToolItem::Button(b) if b.hidden)).map(item_key).filter(|k| !LOCKED.contains(k)).collect();
    // The buttons of a set are one part of the order.
    let mut folded: Vec<&str> = Vec::with_capacity(keys.len());
    for key in keys {
        match set_of(key) {
            Some(set) if folded.contains(&set.key) => {}
            Some(set) => folded.push(set.key),
            None => folded.push(key),
        }
    }
    tidy(folded).into_iter().map(str::to_string).collect()
}

/// An order without a separator at its ends or two together.
fn tidy(keys: Vec<&str>) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::with_capacity(keys.len());
    for key in keys {
        if key == SEPARATOR && out.last().is_none_or(|last| *last == SEPARATOR) {
            continue;
        }
        out.push(key);
    }
    while out.last() == Some(&SEPARATOR) {
        out.pop();
    }
    out
}

/// The button of a menu command or of a submenu (`key` is `cmd:<id>` or `menu:<id>`), made from the menus; `None` when the menus have
/// no such command (the document is another format's or the command is not there now).
pub fn menu_button(menus: &Menus, key: &str) -> Option<ToolItem> {
    if let Some(id) = key.strip_prefix(COMMAND) {
        let item = crate::chrome::quick_access_item(menus, id)?;
        let mut button = match item.icon.filter(|n| !n.is_empty()) {
            Some(icon) => ToolButton::new(item.id, icon, item.label.clone()),
            None => ToolButton::text(item.id, item.label.clone(), item.label.clone()),
        };
        button.disabled = item.disabled;
        button.on = item.checked;
        button.handler = item.handler.clone();
        return Some(button.into());
    }
    let id = key.strip_prefix(SUBMENU)?;
    match menus.find_entry(id)? {
        MenuEntry::Submenu(sub) => Some(ToolButton::new(sub.id, sub.icon.filter(|n| !n.is_empty()).unwrap_or("more_horiz"), sub.label.clone()).menu(sub.items.clone()).into()),
        _ => None,
    }
}

/// `items` (the toolbar as the format makes it) in the order `layout` says. A key whose button is not in `items` (another scene's)
/// or whose command the menus do not have is left out. The locked buttons come first, and the buttons that are not drawn (hidden:
/// their menus are the Format menu's) stay.
pub fn apply(items: Vec<ToolItem>, layout: &[String], menus: Option<&Menus>) -> Vec<ToolItem> {
    let mut hidden: Vec<ToolItem> = Vec::new();
    let mut pool: Vec<(&'static str, ToolItem)> = Vec::new();
    for item in items {
        match &item {
            ToolItem::Separator => {}
            ToolItem::Button(b) if b.hidden => hidden.push(item),
            _ => {
                let key = item_key(&item);
                if !pool.iter().any(|(k, _)| *k == key) {
                    pool.push((key, item));
                }
            }
        }
    }
    let take = |pool: &mut Vec<(&'static str, ToolItem)>, key: &str| pool.iter().position(|(k, _)| *k == key).map(|at| pool.remove(at).1);
    let mut out: Vec<ToolItem> = LOCKED.iter().filter_map(|key| take(&mut pool, key)).collect();
    let mut placed: Vec<ToolItem> = Vec::new();
    for key in layout {
        if key == SEPARATOR {
            placed.push(ToolItem::Separator);
        } else if let Some(set) = SETS.iter().find(|set| set.key == key) {
            placed.extend(set.buttons.iter().filter_map(|id| take(&mut pool, id)));
        } else if key.contains(':') {
            placed.extend(menus.and_then(|m| menu_button(m, key)));
        } else {
            placed.extend(take(&mut pool, key));
        }
    }
    // Separators: none at the ends, never two together.
    let mut tidy_placed: Vec<ToolItem> = Vec::with_capacity(placed.len());
    for item in placed {
        if matches!(item, ToolItem::Separator) && tidy_placed.last().is_none_or(|last| matches!(last, ToolItem::Separator)) {
            continue;
        }
        tidy_placed.push(item);
    }
    while matches!(tidy_placed.last(), Some(ToolItem::Separator)) {
        tidy_placed.pop();
    }
    if !out.is_empty() && !tidy_placed.is_empty() {
        out.push(ToolItem::Separator);
    }
    out.extend(tidy_placed);
    out.extend(hidden);
    out
}

thread_local! {
    /// What the buttons seen so far look like (their icon and words), by key: the dialog draws the buttons of a scene the open document
    /// is not in (and the preview) from this.
    static KNOWN: std::cell::RefCell<std::collections::HashMap<&'static str, (Option<&'static str>, gpui_kit::SharedString)>> = Default::default();
    /// The keys of each scene as the format made them (the order a scene has until it is changed), as last seen: (format, scene).
    static SEEN: std::cell::RefCell<std::collections::HashMap<(String, String), Vec<String>>> = Default::default();
}

/// Note the toolbar `items` the format made for `scene` of the format `extension`: the buttons' looks, and the scene's own order.
pub fn record(extension: &str, scene: &str, items: &[ToolItem]) {
    KNOWN.with(|k| {
        let mut k = k.borrow_mut();
        for item in items {
            let (key, icon, label) = match item {
                ToolItem::Button(b) => (b.id, b.icon, b.text.clone().unwrap_or_else(|| b.tooltip.clone())),
                ToolItem::Select { id, tooltip, .. } => (*id, Some("arrow_drop_down"), tooltip.clone()),
                ToolItem::FontSize { .. } => ("font-size", Some("format_size"), text("toolbar.customize.font-size")),
                ToolItem::Separator => continue,
            };
            // A tooltip may be the id of its words (`_shell.bold-b`).
            let label = crate::i18n::lookup_words(&label).unwrap_or(label);
            k.entry(key).or_insert((icon, label));
        }
    });
    if !scene.is_empty() {
        let keys = default_keys(items);
        SEEN.with(|s| {
            let mut s = s.borrow_mut();
            let entry = s.entry((extension.to_string(), scene.to_string())).or_default();
            if *entry != keys {
                *entry = keys;
            }
        });
    }
}

/// The order a scene has until it is changed, if the scene has been seen.
pub fn seen_default(extension: &str, scene: &str) -> Option<Vec<String>> {
    SEEN.with(|s| s.borrow().get(&(extension.to_string(), scene.to_string())).cloned()).or_else(|| crate::toolbar_defaults::keys(extension, scene))
}

/// The icon and the words of a button seen before.
pub fn known(key: &str) -> Option<(Option<&'static str>, gpui_kit::SharedString)> {
    if let Some(set) = SETS.iter().find(|set| set.key == key) {
        return Some((Some(set.icon), text(set.name)));
    }
    KNOWN.with(|k| k.borrow().get(key).cloned()).or_else(|| {
        let (icon, tip) = crate::toolbar_defaults::button(key)?;
        let label = crate::i18n::lookup_words(tip).unwrap_or_else(|| tip.to_string().into());
        Some(((!icon.is_empty()).then_some(icon), label))
    })
}

/// The name of a scene in the UI's language.
pub fn scene_name(scene: &Scene) -> gpui_kit::SharedString {
    text(scene.name)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn button(id: &'static str) -> ToolItem {
        ToolButton::new(id, "add", id).into()
    }
    fn keys(items: &[ToolItem]) -> Vec<&'static str> {
        items.iter().map(item_key).collect()
    }

    /// An order puts the buttons in its order, keeps the locked ones first and the hidden ones, leaves out what it does not name
    /// and what the scene does not have, and never ends or doubles a separator.
    #[test]
    fn an_order_arranges_the_buttons() {
        let hidden: ToolItem = {
            let mut b = ToolButton::new("h", "add", "h");
            b.hidden = true;
            b.into()
        };
        let items = vec![button("menu-search"), button("a"), button("format-paint"), ToolItem::Separator, button("b"), button("c"), hidden];
        let layout: Vec<String> = ["|", "c", "|", "|", "nope", "a", "|"].iter().map(|s| s.to_string()).collect();
        let out = apply(items, &layout, None);
        assert_eq!(keys(&out), ["menu-search", "format-paint", "|", "c", "|", "a", "h"]);
    }

    /// The order a scene has until it is changed is the format's own, without the locked buttons and the hidden ones.
    #[test]
    fn the_default_order_is_the_formats() {
        let items = vec![button("menu-search"), button("a"), button("format-paint"), ToolItem::Separator, button("b"), ToolItem::Separator];
        assert_eq!(default_keys(&items), ["a", "|", "b"]);
        // Applying it changes nothing but the place of the locked buttons.
        let out = apply(items, &default_keys(&[button("a"), ToolItem::Separator, button("b")]), None);
        assert_eq!(keys(&out), ["menu-search", "format-paint", "|", "a", "|", "b"]);
    }

    #[test]
    fn the_scenes_of_the_formats() {
        assert_eq!(scenes("docx").len(), 3);
        assert_eq!(scenes("xlsx").len(), 1);
        assert_eq!(scenes("pptx").len(), 6);
        assert!(scenes("pdf").is_empty());
    }
}
