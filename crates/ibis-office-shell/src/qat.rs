// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The quick access toolbar (クイック アクセス ツール バー): commands the user puts
//! in the title row beside undo and redo (ツール > 設定… > クイック アクセス ツール
//! バー). A command is a menu item by its id; the row shows the ones the open
//! document's menus have.
use std::cell::RefCell;

use crate::settings;

/// The commands to choose from (menu item ids, each with a label of its own id in the language files).
pub(crate) const CANDIDATES: &[&str] = &[
    "file.open",
    "file.save",
    "file.save-as",
    "file.print",
    "file.export.pdf",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "edit.paste-plain",
    "edit.select-all",
    "edit.find-replace",
    "format.clear",
    "view.zoom-in",
    "view.zoom-out",
    "insert.link",
    "insert.comment",
    "xlsx.insert-row-above",
    "xlsx.delete-row",
    "xlsx.insert-table",
    "xlsx.insert-picture",
    "xlsx.pivot-insert",
    "xlsx.slicer-insert",
    "xlsx.fill-down",
    "xlsx.freeze",
    "xlsx.gridlines",
    "xlsx.zoom-selection",
    "xlsx.copy-as-picture",
    "xlsx.ink",
    "xlsx.sort",
    "xlsx.sort-descending",
    "xlsx.filter",
    "xlsx.calculate-now",
    "xlsx.custom-views",
    "xlsx.solver",
    "xlsx.forecast-sheet",
    "xlsx.validation",
    "docx.track.record",
    "docx.track.accept",
    "docx.track.reject",
    "docx.accessibility-check",
    "docx.read-aloud",
    "docx.inspect-document",
    "docx.compatibility-check",
    "docx.print-settings",
    "docx.file.convert-format",
    "pptx.new-slide",
    "pptx.duplicate-slide",
    "pptx.delete-slide",
    "pptx.selection-pane",
    "pptx.reading-order",
    "pptx.check-accessibility",
    "pptx.inspect-document",
    "pptx.3d-model",
    "pptx.record-preview",
    "pptx.screen-reader",
];

const KEY: &str = "qat";

thread_local! {
    static CACHE: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// The commands chosen, in order.
pub fn commands() -> Vec<String> {
    CACHE.with(|c| {
        c.borrow_mut()
            .get_or_insert_with(|| settings::get(KEY).unwrap_or_default().split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect())
            .clone()
    })
}

/// The commands chosen replaced (kept).
pub(crate) fn set(ids: &[String]) {
    settings::set(KEY, &ids.join(","));
    CACHE.with(|c| *c.borrow_mut() = Some(ids.to_vec()));
}

/// A command put in the row, or taken out.
pub fn toggle(id: &str) {
    let mut ids = commands();
    match ids.iter().position(|i| i == id) {
        Some(at) => {
            ids.remove(at);
        }
        None => ids.push(id.to_string()),
    }
    set(&ids);
}

/// The command at place `from` moved to place `to` (the others keep their order). Whether anything moved.
pub fn move_to(from: usize, to: usize) -> bool {
    let mut ids = commands();
    if from == to || from >= ids.len() || to >= ids.len() {
        return false;
    }
    let id = ids.remove(from);
    ids.insert(to, id);
    set(&ids);
    true
}

/// A command added at the end of the row (nothing when it is in it already). Whether it was added.
pub(crate) fn add(id: &str) -> bool {
    let mut ids = commands();
    if ids.iter().any(|i| i == id) {
        return false;
    }
    ids.push(id.to_string());
    set(&ids);
    true
}

/// A command taken out of the row. Whether it was in it.
pub(crate) fn remove(id: &str) -> bool {
    let mut ids = commands();
    let Some(at) = ids.iter().position(|i| i == id) else { return false };
    ids.remove(at);
    set(&ids);
    true
}

/// Reset QAT commands to empty.
pub fn reset() {
    set(&[]);
}

/// Forget what is cached (the settings changed under it).
#[allow(dead_code)]
pub fn reload() {
    CACHE.with(|c| *c.borrow_mut() = None);
}
