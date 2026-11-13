// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The documents that are open in the app's windows (menu bar redesign, stage 4;
//! `tasks/doc/menu-redesign/implementation-spec.md` section 3): which window shows which document, in the order they
//! were opened and in the order they were last used (MRU). The Window menu's list at the bottom, the next and
//! previous window and the quick switcher (`window_switcher.rs`) read it.
//!
//! One app-wide [`Global`] keyed by the editor's entity id (the windows share one thread: nothing about a window is
//! kept in a `thread_local!`, see `tests/window_state_rule.rs`). A window registers itself when it draws a document
//! and a closed window (its editor dropped) is swept out when the table is read. The order is a counter, not a
//! time (`clock` is for time; only the order matters here).
use gpui_kit::{AnyWindowHandle, App, BorrowAppContext as _, EntityId, Global, SharedString, WeakEntity};
use ibis_pptx::Kind;

use crate::editor::SlideEditor;

/// A document window.
#[derive(Clone)]
pub struct OpenDocument {
    pub window: AnyWindowHandle,
    pub id: EntityId,
    pub editor: WeakEntity<SlideEditor>,
    /// The name the Window menu shows (the file's name, with `:2` for a second window on it).
    pub name: SharedString,
    pub kind: Kind,
    /// The document has changes that are not saved.
    pub dirty: bool,
    pub(crate) opened_seq: u64,
    pub(crate) activated_seq: u64,
}

/// The open documents (the app's `Global`).
#[derive(Default)]
pub struct OpenDocuments {
    next_seq: u64,
    docs: Vec<OpenDocument>,
}

impl Global for OpenDocuments {}

impl OpenDocuments {
    fn ensure(cx: &mut App) {
        if cx.try_global::<OpenDocuments>().is_none() {
            cx.set_global(OpenDocuments::default());
        }
    }

    /// The table, with the windows that are closed left out.
    pub(crate) fn read(cx: &App) -> Vec<OpenDocument> {
        cx.try_global::<OpenDocuments>().map(|d| d.docs.iter().filter(|doc| doc.editor.upgrade().is_some()).cloned().collect()).unwrap_or_default()
    }

    /// Record a document window (or update its name, kind and unsaved state). A window seen for the first time is the
    /// newest one in both orders. `activated` says the window has just been brought to the front (it was not the active
    /// window at the last frame and is now), which makes it the most recently used. Every window is redrawn when the
    /// table changed, so that the lists they show (the Window menu's) follow it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sync(cx: &mut App, window: AnyWindowHandle, id: EntityId, editor: WeakEntity<SlideEditor>, name: SharedString, kind: Kind, dirty: bool, activated: bool) {
        Self::ensure(cx);
        let changed = cx.update_global::<OpenDocuments, _>(|docs, _| {
            docs.docs.retain(|d| d.editor.upgrade().is_some() || d.id == id);
            match docs.docs.iter_mut().find(|d| d.id == id) {
                Some(doc) => {
                    let mut changed = doc.name != name || doc.kind != kind || doc.dirty != dirty;
                    doc.window = window;
                    doc.editor = editor;
                    doc.name = name;
                    doc.kind = kind;
                    doc.dirty = dirty;
                    if activated {
                        docs.next_seq += 1;
                        let seq = docs.next_seq;
                        if let Some(doc) = docs.docs.iter_mut().find(|d| d.id == id) {
                            doc.activated_seq = seq;
                        }
                        changed = true;
                    }
                    changed
                }
                None => {
                    docs.next_seq += 1;
                    let seq = docs.next_seq;
                    docs.docs.push(OpenDocument { window, id, editor, name, kind, dirty, opened_seq: seq, activated_seq: seq });
                    true
                }
            }
        });
        if changed {
            Self::redraw_all(cx);
        }
    }

    /// Redraw every document window (after the table changed).
    fn redraw_all(cx: &mut App) {
        let editors: Vec<WeakEntity<SlideEditor>> = Self::read(cx).into_iter().map(|d| d.editor).collect();
        cx.defer(move |cx| {
            for editor in editors {
                editor.update(cx, |_, cx| cx.notify()).ok();
            }
        });
    }

    /// The window is not a document window any more (it shows the start screen, or is closed).
    pub(crate) fn unregister(cx: &mut App, id: EntityId) {
        if cx.try_global::<OpenDocuments>().is_some() {
            let removed = cx.update_global::<OpenDocuments, _>(|docs, _| {
                let before = docs.docs.len();
                docs.docs.retain(|d| d.id != id);
                docs.docs.len() != before
            });
            if removed {
                Self::redraw_all(cx);
            }
        }
    }

    /// The window `id` is the most recently used now.
    pub fn activate(cx: &mut App, id: EntityId) {
        Self::ensure(cx);
        cx.update_global::<OpenDocuments, _>(|docs, _| {
            docs.next_seq += 1;
            let seq = docs.next_seq;
            if let Some(doc) = docs.docs.iter_mut().find(|d| d.id == id) {
                doc.activated_seq = seq;
            }
        });
        Self::redraw_all(cx);
    }

    /// Most recently used first (the first is the window in front).
    pub fn mru(cx: &App) -> Vec<OpenDocument> {
        let mut docs = Self::read(cx);
        docs.sort_by_key(|d| std::cmp::Reverse(d.activated_seq));
        docs
    }

    /// In the order the documents were opened.
    pub fn by_open_order(cx: &App) -> Vec<OpenDocument> {
        let mut docs = Self::read(cx);
        docs.sort_by_key(|d| d.opened_seq);
        docs
    }

    /// The window after (or before) `id` in the order of opening, wrapping round at the ends.
    pub fn neighbour(cx: &App, id: EntityId, forward: bool) -> Option<OpenDocument> {
        let docs = Self::by_open_order(cx);
        let at = docs.iter().position(|d| d.id == id)?;
        if docs.len() < 2 {
            return None;
        }
        let next = if forward { (at + 1) % docs.len() } else { (at + docs.len() - 1) % docs.len() };
        docs.into_iter().nth(next)
    }
}
