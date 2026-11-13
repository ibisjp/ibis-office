// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A file that is open is edited in memory; if another app changes it on disk meanwhile, the
//! window asks whether to read it again (a message box with the actions of the common action bar).
//!
//! The operating system's file watcher (the `notify` crate: FSEvents, inotify,
//! ReadDirectoryChangesW) watches the *folder* of each open file, so a writer that saves by
//! writing a temporary file and renaming it over the file is seen too. A burst of events is
//! taken as one (`DEBOUNCE`), and then every window's file is compared with what this window last
//! read or wrote (`DiskStamp`): modification time and size, and for a file this window wrote itself,
//! the hash of its content, so the app's own save (which also renames a temporary file over the
//! file) never asks. A file that was deleted or moved is not handled (nothing is asked).
//!
//! The web version has no files on disk: only the plain data below exists there.

use std::path::PathBuf;
use std::time::SystemTime;

/// What a file looked like when this window read or wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskStamp {
    modified: Option<SystemTime>,
    len: u64,
    /// The hash of the content, known for a file this window wrote.
    hash: Option<u64>,
}

impl DiskStamp {
    /// The file at `path` as the file system says (modification time and size), if it is a file.
    #[cfg(not(target_family = "wasm"))]
    pub fn stat(path: &std::path::Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
        Some(Self { modified: meta.modified().ok(), len: meta.len(), hash: None })
    }

    /// The file at `path` just after `bytes` were written to it.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn after_write(path: &std::path::Path, bytes: &[u8]) -> Option<Self> {
        Some(Self { hash: Some(content_hash(bytes)), ..Self::stat(path)? })
    }

    fn same_stat(&self, other: &Self) -> bool {
        self.modified == other.modified && self.len == other.len
    }
}

/// FNV-1a, 64 bits: to tell whether a file's content is what this window wrote (not a safeguard against anyone).
pub(crate) fn content_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3))
}

/// A window's side of the watching.
#[derive(Default)]
pub struct ExternalWatch {
    /// The file as this window last read or wrote it.
    pub known: Option<DiskStamp>,
    /// The change the message box (or an answer of いいえ) is about: it is not asked again.
    pub asked: Option<DiskStamp>,
    /// Whether the message box is showing.
    pub dialog: bool,
    /// The file this window is loading (it has no `path` until it is loaded): another open of it is not made.
    pub(crate) opening_path: Option<PathBuf>,
    /// The view the window had before a file was read again (読み込み直す), put back when that file is loaded.
    pub(crate) restore_view: Option<(PathBuf, crate::view_state::ViewState)>,
}

#[cfg(target_family = "wasm")]
impl crate::editor::SlideEditor {
    /// There is no watching in the browser.
    pub(crate) fn dismiss_external_update_if_open(&mut self, _: &mut gpui_kit::Context<Self>) -> bool {
        false
    }
}

#[cfg(not(target_family = "wasm"))]
pub use desktop::{start, watch};

#[cfg(not(target_family = "wasm"))]
mod desktop {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    use futures::StreamExt as _;
    use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
    use gpui_kit::component::Root;
    use gpui_kit::*;

    use super::DiskStamp;
    use crate::chrome::{ActionButtonKind, ActionDefinition, action_bar};
    use crate::editor::SlideEditor;
    use crate::i18n::text;

    /// A burst of events (a save is a few writes and a rename) is taken as one, this long after the first.
    const DEBOUNCE: Duration = Duration::from_millis(400);

    struct Shared {
        watcher: Option<notify::RecommendedWatcher>,
        folders: HashSet<PathBuf>,
        sender: UnboundedSender<()>,
        receiver: Option<UnboundedReceiver<()>>,
    }

    fn shared() -> &'static Mutex<Shared> {
        static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();
        SHARED.get_or_init(|| {
            let (sender, receiver) = unbounded();
            Mutex::new(Shared { watcher: None, folders: HashSet::new(), sender, receiver: Some(receiver) })
        })
    }

    /// Watch the folder of the open file `path` (once per folder; kept until the app ends: an event in a folder with no
    /// open file finds nothing to ask about).
    pub fn watch(path: &Path) {
        use notify::Watcher as _;
        let folder = match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let Ok(mut shared) = shared().lock() else { return };
        if shared.folders.contains(&folder) {
            return;
        }
        if shared.watcher.is_none() {
            let sender = shared.sender.clone();
            let handler = move |event: notify::Result<notify::Event>| {
                // Reading a file is an event on some systems: only a change counts.
                if event.is_ok_and(|event| !event.kind.is_access()) {
                    sender.unbounded_send(()).ok();
                }
            };
            match notify::recommended_watcher(handler) {
                Ok(watcher) => shared.watcher = Some(watcher),
                Err(err) => {
                    log::warn!("file watching is not available: {err}");
                    return;
                }
            }
        }
        let Some(watcher) = shared.watcher.as_mut() else { return };
        match watcher.watch(&folder, notify::RecursiveMode::NonRecursive) {
            Ok(()) => {
                shared.folders.insert(folder);
            }
            Err(err) => log::warn!("cannot watch {}: {err}", folder.display()),
        }
    }

    /// Start the task that turns the watcher's events into a check of every window (once, with the app).
    pub fn start(cx: &mut App) {
        let Some(mut receiver) = shared().lock().ok().and_then(|mut shared| shared.receiver.take()) else { return };
        cx.spawn(async move |cx| {
            while receiver.next().await.is_some() {
                cx.background_executor().timer(DEBOUNCE).await;
                while receiver.try_recv().is_ok() {}
                cx.update(check_all_windows);
            }
        })
        .detach();
    }

    /// Ask every window whose file was changed from outside.
    pub(crate) fn check_all_windows(cx: &mut App) {
        for window in cx.windows() {
            let Some(root) = window.downcast::<Root>() else { continue };
            root.update(cx, |root, _, cx| {
                if let Ok(editor) = root.view().clone().downcast::<SlideEditor>() {
                    editor.update(cx, |editor, cx| editor.check_external_update(cx));
                }
            })
            .ok();
        }
    }

    impl SlideEditor {
        /// Whether the file on disk was changed by another app since this window read or wrote it; if so,
        /// ask (unless that very change was already asked about).
        pub fn check_external_update(&mut self, cx: &mut Context<Self>) {
            let Some(path) = self.path.clone() else { return };
            if !self.has_document || self.loading {
                return;
            }
            // Deleted or moved: not handled.
            let Some(now) = DiskStamp::stat(&path) else { return };
            let Some(known) = self.watch.known.clone() else {
                self.watch.known = Some(now);
                return;
            };
            if known.same_stat(&now) || self.watch.asked.as_ref().is_some_and(|asked| asked.same_stat(&now)) {
                return;
            }
            // Written by this window (it renames a file over it): the content is the same, whatever the time says.
            if let Some(hash) = known.hash
                && std::fs::read(&path).is_ok_and(|bytes| super::content_hash(&bytes) == hash)
            {
                self.watch.known = Some(DiskStamp { hash: Some(hash), ..now });
                return;
            }
            self.watch.asked = Some(now);
            self.watch.dialog = true;
            cx.notify();
        }

        /// いいえ: nothing is done, and this change is not asked about again.
        pub fn dismiss_external_update(&mut self, cx: &mut Context<Self>) {
            self.watch.dialog = false;
            self.watch.known = self.watch.asked.take().or_else(|| self.path.as_deref().and_then(DiskStamp::stat));
            cx.notify();
        }

        /// Escape / Cancel while the message box is showing: the same as いいえ.
        pub(crate) fn dismiss_external_update_if_open(&mut self, cx: &mut Context<Self>) -> bool {
            if !self.watch.dialog {
                return false;
            }
            self.dismiss_external_update(cx);
            true
        }

        /// はい / 今の変更を破棄して読み込み直す: read the file again in this window, as opening it does.
        pub fn reload_external_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            self.watch.dialog = false;
            self.watch.asked = None;
            let Some(path) = self.path.clone() else { return cx.notify() };
            // The window shows the same place again (scroll, zoom, sheet or slide, selection: `view_state.rs`).
            self.watch.restore_view = Some((path.clone(), self.save_view_state()));
            self.open_path(path, None, window, cx);
        }

        /// 名前をつけて保存して開く: save what is in this window under another name, then read the updated file.
        pub(crate) fn save_as_then_reload_external_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            let Some(path) = self.path.clone() else { return };
            let view = self.save_view_state();
            // The message box stays until the file is saved: a cancelled dialog leaves the question as it was.
            self.save_as_then(window, cx, move |this, window, cx| {
                this.watch.dialog = false;
                this.watch.asked = None;
                this.watch.restore_view = Some((path.clone(), view));
                this.open_path(path, None, window, cx);
            });
        }

        pub(crate) fn render_external_update_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
            let title: SharedString = self.file_name.clone().unwrap_or_default();
            let no = ActionDefinition::new("file-changed-no", text("shell.file-changed.no"), ActionButtonKind::Normal)
                .on_click(cx.listener(|this, _, _, cx| this.dismiss_external_update(cx)));
            let actions = if self.is_dirty() {
                action_bar()
                    .action(
                        ActionDefinition::new("file-changed-discard", text("shell.file-changed.discard"), ActionButtonKind::Destructive)
                            .on_click(cx.listener(|this, _, window, cx| this.reload_external_update(window, cx))),
                    )
                    .action(
                        ActionDefinition::new("file-changed-save-as", text("shell.file-changed.save-as"), ActionButtonKind::Normal)
                            .on_click(cx.listener(|this, _, window, cx| this.save_as_then_reload_external_update(window, cx))),
                    )
                    .action(no)
            } else {
                action_bar()
                    .action(
                        ActionDefinition::new("file-changed-yes", text("shell.file-changed.yes"), ActionButtonKind::Primary)
                            .on_click(cx.listener(|this, _, window, cx| this.reload_external_update(window, cx))),
                    )
                    .action(no)
            };
            crate::message_box::message_box("file-changed-dialog", title, text("shell.file-changed.message"), actions, &self.msgbox, true, cx)
        }
    }
}
