// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The セットアップ dialog (K0001; the model and the files are `font_setup.rs`): a table of the fonts that can be downloaded,
//! with a check box for each (and one in the heading row for all), their sizes and licences, and the action bar's
//! 「インストール」 and 「インストールしない」. It is a modal dialog of the editor (the shared dialog host, the shared table), as the
//! CMYK profile's (`cmyk_profile.rs`); its state is kept in the editor's `document_memory`.
//!
//! It is opened by the check at startup (`start`: once for each version, when the list has a font that is neither installed nor
//! skipped) and by ヘルプ > セットアップ (`open_font_setup`), which reads the list again. A list that cannot be read, a failed
//! download and a full disk are told in the dialog; none of them stops anything.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::chrome::virtual_table::{TableColumn, TableRow, TableState};
use super::chrome::data_table::DataTable;
use super::chrome::{self, Shown};
use super::editor::SlideEditor;
use super::font_setup::{self, DownloadError, FontEntry, Row};
use super::i18n::text;

const MEMORY_KEY: &str = "font-setup.dialog";
const ROW_HEIGHT: f32 = 32.;
/// How many rows the table shows before it scrolls.
const VISIBLE_ROWS: usize = 8;

/// What the dialog is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Phase {
    /// The list is being read.
    Loading,
    /// The list is shown; nothing runs.
    Idle,
    /// Font `index` of `count` (from 1) is being downloaded.
    Downloading { index: usize, count: usize, name: String, total: u64 },
    /// Everything chosen is installed.
    Done,
}

struct SetupState {
    rows: Vec<Row>,
    table: TableState,
    phase: Phase,
    /// The line under the table: why nothing is listed, what failed, what was done.
    message: Option<SharedString>,
    /// The bytes of the font being downloaded that have arrived.
    received: u64,
    /// Whether the editor's right pane was open (it is put back when the dialog closes).
    sidebar: bool,
    cancel: Arc<AtomicBool>,
}

type Shared = Rc<RefCell<SetupState>>;

// ---- The list and what is installed ------------------------------------------

/// The fonts of the list and the file names of the ones installed (read off the main thread).
async fn load_entries(cx: &AsyncApp) -> Result<(Vec<FontEntry>, HashSet<String>), String> {
    #[cfg(not(target_family = "wasm"))]
    {
        cx.background_executor()
            .spawn(async {
                let list = font_setup::fetch_list()?;
                let installed = list.iter().filter(|e| font_setup::is_installed(e)).map(|e| e.file.clone()).collect();
                Ok((list, installed))
            })
            .await
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = cx;
        let text = crate::web::font_fetch_text(&font_setup::LIST_URL).await?;
        let installed = crate::web::fonts_installed().await.unwrap_or_default().into_iter().collect();
        Ok((font_setup::parse_list(&text), installed))
    }
}

/// The rows to offer for `entries` (the ones not installed; the skipped last and off).
fn rows_for(entries: &[FontEntry], installed: &HashSet<String>) -> Vec<Row> {
    font_setup::build_rows(entries, |e| installed.contains(&e.file), |e| font_setup::is_skipped(&e.file))
}

// ---- Startup -------------------------------------------------------------------

/// Called once the first window is open (and the migration at startup has run): when this version has not asked about the fonts
/// yet (`font-setup.checked-version`; the migration forgets it at the first start and at an update), the list is read in the
/// background, and the dialog is opened if it has a font that is neither installed nor skipped. A list that cannot be read is
/// only logged: the next start asks again. Not in the tests (no network) unless `IBIS_FONT_LIST_URL` names a local list.
pub fn start(cx: &mut App) {
    #[cfg(any(test, feature = "scenarios"))]
    if std::env::var_os("IBIS_FONT_LIST_URL").is_none_or(|v| v.is_empty()) {
        let _ = &cx;
        return;
    }
    if crate::settings::get(font_setup::CHECKED_KEY).as_deref() == Some(crate::VERSION) {
        return;
    }
    cx.spawn(async move |cx: &mut AsyncApp| {
        // Not while the app is starting.
        let delay = if cfg!(any(test, feature = "scenarios")) { Duration::from_millis(200) } else { Duration::from_secs(5) };
        cx.background_executor().timer(delay).await;
        let (entries, installed) = match load_entries(cx).await {
            Ok(loaded) => loaded,
            Err(err) => {
                log::warn!("font setup: the list could not be read: {err}");
                return;
            }
        };
        crate::settings::set(font_setup::CHECKED_KEY, crate::VERSION);
        if !font_setup::has_new(&entries, |e| installed.contains(&e.file), |e| font_setup::is_skipped(&e.file)) {
            return;
        }
        // Over the window the user is working in, when it has nothing else open.
        loop {
            let rows = rows_for(&entries, &installed);
            if cx.update(|cx| show_in_a_window(rows, cx)) {
                return;
            }
            cx.background_executor().timer(Duration::from_secs(30)).await;
        }
    })
    .detach();
}

/// Open the dialog with `rows` over the active window (or the first one) if it is an editor with no dialog open. Whether it was.
fn show_in_a_window(rows: Vec<Row>, cx: &mut App) -> bool {
    let Some(window) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return false };
    let Some(root) = window.downcast::<gpui_kit::component::Root>() else { return false };
    root.update(cx, |root, window, app| {
        let Ok(editor) = root.view().clone().downcast::<SlideEditor>() else { return false };
        editor.update(app, |editor, cx| {
            if editor.dialog.active || editor.font_setup_state().is_some() || editor.interaction_layer().is_confirmation() {
                return false;
            }
            editor.show_font_setup(rows, window, cx);
            true
        })
    })
    .unwrap_or(false)
}

// ---- The editor's side -----------------------------------------------------------

impl SlideEditor {
    pub(crate) fn font_setup_is_open(&self) -> bool {
        self.font_setup_state().is_some()
    }

    fn font_setup_state(&self) -> Option<Shared> {
        self.document_memory.get::<Option<Shared>>(MEMORY_KEY).flatten()
    }

    fn open_dialog_state(&mut self, rows: Vec<Row>, phase: Phase, cx: &mut Context<Self>) {
        let state = SetupState { rows, table: TableState::default(), phase, message: None, received: 0, sidebar: self.sidebar_open, cancel: Arc::new(AtomicBool::new(false)) };
        self.document_memory.set(MEMORY_KEY, Some(Rc::new(RefCell::new(state))));
        self.sidebar_open = true;
        self.dialog = Default::default();
        cx.notify();
    }

    /// The dialog with the rows the startup check found.
    pub(crate) fn show_font_setup(&mut self, rows: Vec<Row>, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_dialog_state(rows, Phase::Idle, cx);
    }

    /// ヘルプ > セットアップ: the dialog, with the list read again.
    pub(crate) fn open_font_setup(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.font_setup_state().is_some() {
            return;
        }
        self.open_dialog_state(Vec::new(), Phase::Loading, cx);
        cx.spawn(async move |this, cx| {
            let loaded = load_entries(cx).await;
            this.update(cx, |this, cx| this.font_setup_loaded(loaded, cx)).ok();
        })
        .detach();
    }

    fn font_setup_loaded(&mut self, loaded: Result<(Vec<FontEntry>, HashSet<String>), String>, cx: &mut Context<Self>) {
        let Some(state) = self.font_setup_state() else { return };
        {
            let mut s = state.borrow_mut();
            match loaded {
                Ok((entries, installed)) => {
                    s.rows = rows_for(&entries, &installed);
                    s.message = s.rows.is_empty().then(|| text("shell.font-setup.empty"));
                    // The list was read: this version has asked (the startup check does not ask again).
                    crate::settings::set(font_setup::CHECKED_KEY, crate::VERSION);
                }
                Err(err) => {
                    log::warn!("font setup: the list could not be read: {err}");
                    s.message = Some(crate::i18n::format("shell.font-setup.list-failed", &[("error", &err)]).into());
                }
            }
            s.phase = Phase::Idle;
        }
        cx.notify();
    }

    /// Close the dialog (a download that runs is cancelled).
    pub(crate) fn close_font_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.font_setup_state() {
            self.sidebar_open = state.borrow().sidebar;
        }
        self.font_setup_discard();
        self.dialog = Default::default();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// The dialog went away by its × or Escape: its state is dropped and a download that runs stops.
    pub(crate) fn font_setup_discard(&mut self) {
        if let Some(state) = self.font_setup_state() {
            state.borrow().cancel.store(true, Ordering::SeqCst);
        }
        self.document_memory.set(MEMORY_KEY, None::<Shared>);
    }

    fn font_setup_toggle(&mut self, file: &str, cx: &mut Context<Self>) {
        if let Some(state) = self.font_setup_state() {
            let mut s = state.borrow_mut();
            if s.phase == Phase::Idle
                && let Some(row) = s.rows.iter_mut().find(|r| r.entry.file == file)
            {
                row.checked = !row.checked;
            }
        }
        cx.notify();
    }

    fn font_setup_toggle_all(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.font_setup_state() {
            let mut s = state.borrow_mut();
            if s.phase == Phase::Idle {
                font_setup::toggle_all(&mut s.rows);
            }
        }
        cx.notify();
    }

    /// 「インストールしない」: the fonts the table shows now are recorded as skipped, and the dialog closes.
    pub(crate) fn font_setup_skip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.font_setup_state() {
            font_setup::skip_all(&state.borrow().rows);
        }
        self.close_font_setup(window, cx);
    }

    /// 「取り消し」: the transfer stops (the unfinished file is removed); the dialog stays.
    fn font_setup_cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.font_setup_state() {
            state.borrow().cancel.store(true, Ordering::SeqCst);
        }
        cx.notify();
    }

    /// 「インストール」: the fonts that are checked are downloaded one after another, with the progress in the dialog.
    pub(crate) fn font_setup_install(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.font_setup_state() else { return };
        let (selected, cancel) = {
            let mut s = state.borrow_mut();
            let selected: Vec<FontEntry> = s.rows.iter().filter(|r| r.checked).map(|r| r.entry.clone()).collect();
            if s.phase != Phase::Idle || selected.is_empty() {
                return;
            }
            s.cancel = Arc::new(AtomicBool::new(false));
            s.message = None;
            s.received = 0;
            s.phase = Phase::Downloading { index: 1, count: selected.len(), name: selected[0].display_name().to_string(), total: selected[0].size };
            (selected, s.cancel.clone())
        };
        cx.notify();
        cx.spawn(async move |this, cx| {
            let mut installed: Vec<String> = Vec::new();
            let mut failures: Vec<(String, String)> = Vec::new();
            let mut cancelled = false;
            for (i, entry) in selected.iter().enumerate() {
                let started = this.update(cx, |this, cx| {
                    if let Some(state) = this.font_setup_state() {
                        let mut s = state.borrow_mut();
                        s.received = 0;
                        s.phase = Phase::Downloading { index: i + 1, count: selected.len(), name: entry.display_name().to_string(), total: entry.size };
                    }
                    cx.notify();
                });
                if started.is_err() {
                    return;
                }
                match download_entry(&this, cx, entry, &cancel).await {
                    Ok(()) => installed.push(entry.file.clone()),
                    Err(DownloadError::Cancelled) => {
                        cancelled = true;
                        break;
                    }
                    Err(DownloadError::Failed(message)) => {
                        log::warn!("font setup: {}: {message}", entry.file);
                        failures.push((entry.display_name().to_string(), message));
                    }
                }
            }
            // The fonts just installed are given to the text system now: the font lists and the documents use them without a restart.
            if !installed.is_empty() {
                #[cfg(not(target_family = "wasm"))]
                let files: Vec<Vec<u8>> = selected
                    .iter()
                    .filter(|e| installed.contains(&e.file))
                    .filter_map(|e| font_setup::font_folder(e).and_then(|dir| std::fs::read(dir.join(&e.file)).ok()))
                    .collect();
                #[cfg(target_family = "wasm")]
                let files: Vec<Vec<u8>> = crate::web::load_downloaded_fonts().await;
                this.update(cx, |_, cx| {
                    if crate::render::shape_view::add_fonts_now(cx, files) {
                        cx.refresh_windows();
                    }
                })
                .ok();
            }
            this.update(cx, |this, cx| this.font_setup_finished(&installed, &failures, cancelled, cx)).ok();
        })
        .detach();
    }

    fn font_setup_finished(&mut self, installed: &[String], failures: &[(String, String)], cancelled: bool, cx: &mut Context<Self>) {
        let Some(state) = self.font_setup_state() else { return };
        {
            let mut s = state.borrow_mut();
            for file in installed {
                font_setup::set_skipped(file, false);
            }
            let unchecked: Vec<String> = s.rows.iter().filter(|r| !r.checked).map(|r| r.entry.file.clone()).collect();
            s.rows.retain(|r| !installed.contains(&r.entry.file));
            s.phase = Phase::Idle;
            s.received = 0;
            if cancelled {
                s.message = Some(text("shell.font-setup.cancelled"));
            } else if let Some((name, error)) = failures.first() {
                s.message = Some(crate::i18n::format("shell.font-setup.failed", &[("name", name), ("error", error), ("count", &failures.len())]).into());
            } else {
                // Everything chosen is in: the fonts that were left unchecked are not wanted, so they are not asked about again.
                for file in &unchecked {
                    font_setup::set_skipped(file, true);
                }
                s.phase = Phase::Done;
                s.message = Some(text("shell.font-setup.done"));
            }
        }
        cx.notify();
    }

    /// For the scenarios: whether the dialog is open.
    #[cfg(any(test, feature = "scenarios"))]
    pub fn font_setup_open(&self) -> bool {
        self.font_setup_is_open()
    }

    /// For the scenarios: the rows as the table shows them (font file, checked, skipped before).
    #[cfg(any(test, feature = "scenarios"))]
    pub fn font_setup_rows(&self) -> Vec<(String, bool, bool)> {
        self.font_setup_state().map(|s| s.borrow().rows.iter().map(|r| (r.entry.file.clone(), r.checked, r.skipped)).collect()).unwrap_or_default()
    }

    /// For the scenarios: the line under the table, and whether a download is running.
    #[cfg(any(test, feature = "scenarios"))]
    pub fn font_setup_status(&self) -> (Option<String>, bool, bool) {
        self.font_setup_state().map(|s| {
            let s = s.borrow();
            (s.message.as_ref().map(|m| m.to_string()), matches!(s.phase, Phase::Downloading { .. }), s.phase == Phase::Loading)
        }).unwrap_or_default()
    }

    /// For the scenarios: what the check box of font file `file` does when it is clicked (the header's with `None`).
    #[cfg(any(test, feature = "scenarios"))]
    pub fn font_setup_click_check(&mut self, file: Option<&str>, cx: &mut Context<Self>) {
        match file {
            Some(file) => self.font_setup_toggle(file, cx),
            None => self.font_setup_toggle_all(cx),
        }
    }

    /// The dialog (a panel of the shared dialog host), or `None` when it is not open.
    pub(crate) fn font_setup_dialog(&mut self, cx: &mut Context<Self>) -> Option<chrome::SidebarPanel> {
        let state = self.font_setup_state()?;
        let editor = cx.weak_entity();
        let (rows, phase, message, received, table) = {
            let s = state.borrow();
            (s.rows.clone(), s.phase.clone(), s.message.clone(), s.received, s.table.clone())
        };
        let idle = phase == Phase::Idle;

        let all = match font_setup::all_checked(&rows) {
            Some(on) => Shown::One(on),
            None => Shown::Mixed,
        };
        let header_check = {
            let editor = editor.clone();
            chrome::Checkbox::new("font-setup-all")
                .accessibility_label(text("shell.font-setup.select-all"))
                .shown(&all)
                .disabled(!idle || rows.is_empty())
                .on_click(move |_, _, cx| {
                    editor.update(cx, |e, cx| e.font_setup_toggle_all(cx)).ok();
                })
        };
        let heading = |label: &'static str| {
            div().h_full().flex().items_center().text_xs().font_weight(FontWeight::MEDIUM).text_color(chrome::text_secondary()).child(text(label)).into_any_element()
        };
        let columns = vec![TableColumn::fixed("check", 40.), TableColumn::flex("name"), TableColumn::fixed("size", 80.), TableColumn::fixed("license", 170.)];
        let header = vec![
            div().size_full().flex().items_center().justify_center().child(header_check).into_any_element(),
            heading("shell.font-setup.col.name"),
            heading("shell.font-setup.col.size"),
            heading("shell.font-setup.col.license"),
        ];
        let build = {
            let (rows, editor) = (rows.clone(), editor.clone());
            move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                range
                    .filter_map(|i| rows.get(i).map(|r| (i, r)))
                    .map(|(i, row)| {
                        let file = row.entry.file.clone();
                        let check = {
                            let (editor, file) = (editor.clone(), file.clone());
                            chrome::Checkbox::new(SharedString::from(format!("font-setup-check-{i}")))
                                .accessibility_label(SharedString::from(row.entry.display_name().to_string()))
                                .checked(row.checked)
                                .disabled(!idle)
                                .on_click(move |_, _, cx| {
                                    editor.update(cx, |e, cx| e.font_setup_toggle(&file, cx)).ok();
                                })
                        };
                        let name = div()
                            .w_full()
                            .pr_1()
                            .truncate()
                            .text_sm()
                            .when(row.skipped, |d| d.text_color(chrome::text_secondary()))
                            .child(SharedString::from(row.entry.display_name().to_string()));
                        let size = div().w_full().pr_1().truncate().text_xs().text_color(chrome::text_secondary()).child(SharedString::from(font_setup::size_text(row.entry.size)));
                        let license_label = if row.entry.license.is_empty() { text("shell.font-setup.license-file") } else { SharedString::from(row.entry.license.clone()) };
                        let license = if row.entry.license_url.is_empty() {
                            div().w_full().pr_1().truncate().text_xs().child(license_label).into_any_element()
                        } else {
                            let url = row.entry.license_url.clone();
                            div()
                                .id(SharedString::from(format!("font-setup-license-{i}")))
                                .w_full()
                                .pr_1()
                                .truncate()
                                .text_xs()
                                .text_color(rgb(chrome::PRIMARY_BUTTON_BG))
                                .underline()
                                .cursor_pointer()
                                .child(license_label)
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    crate::external::open_url(&url, cx);
                                })
                                .into_any_element()
                        };
                        TableRow {
                            cells: vec![
                                div().size_full().flex().items_center().justify_center().child(check).into_any_element(),
                                name.into_any_element(),
                                size.into_any_element(),
                                license,
                            ],
                            ..TableRow::default()
                        }
                    })
                    .collect()
            }
        };
        let empty = div().p_3().text_sm().text_color(chrome::text_secondary()).child(if phase == Phase::Loading { text("shell.font-setup.loading") } else { text("shell.font-setup.empty") });
        let table_element = DataTable::virtualized("font-setup-table", columns, header, table, rows.len(), build, cx)
            .row_height(ROW_HEIGHT)
            .fit_rows(VISIBLE_ROWS)
            .empty(empty.into_any_element());

        let status = match &phase {
            Phase::Downloading { index, count, name, total } => {
                let received = font_setup::size_text(received.max(1));
                let total = font_setup::size_text(*total);
                Some(SharedString::from(crate::i18n::format(
                    "shell.font-setup.progress",
                    &[("name", name), ("index", index), ("count", count), ("received", &received), ("total", &total)],
                )))
            }
            _ => message,
        };
        let body = v_flex()
            .gap_2()
            .w_full()
            .child(div().id("font-setup-heading").text_sm().font_weight(FontWeight::MEDIUM).child(text("shell.font-setup.heading")))
            .child(table_element)
            .children(status.map(|m| div().id("font-setup-status").text_sm().child(m)))
            ;

        let any_checked = rows.iter().any(|r| r.checked);
        let mut actions = chrome::action_bar();
        match phase {
            Phase::Downloading { .. } => {
                actions = actions.action(
                    chrome::ActionDefinition::new("font-setup-cancel", text("shell.font-setup.cancel"), chrome::ActionButtonKind::Normal)
                        .on_click(cx.listener(|e, _, _, cx| e.font_setup_cancel(cx))),
                );
            }
            Phase::Idle if !rows.is_empty() => {
                actions = actions
                    .action(
                        chrome::ActionDefinition::new("font-setup-install", text("shell.font-setup.install"), chrome::ActionButtonKind::Primary)
                            .disabled(!any_checked)
                            .on_click(cx.listener(|e, _, _, cx| e.font_setup_install(cx))),
                    )
                    .action(
                        chrome::ActionDefinition::new("font-setup-skip", text("shell.font-setup.skip"), chrome::ActionButtonKind::Normal)
                            .on_click(cx.listener(|e, _, w, cx| e.font_setup_skip(w, cx))),
                    );
            }
            _ => {
                actions = actions.action(
                    chrome::ActionDefinition::new("font-setup-close", text("_shell.close"), chrome::ActionButtonKind::Primary)
                        .on_click(cx.listener(|e, _, w, cx| e.close_font_setup(w, cx))),
                );
            }
        }
        Some(chrome::SidebarPanel::with_footer(text("shell.font-setup.title"), body.into_any_element(), actions))
    }
}

/// Download one font: off the main thread (the transfer is curl's), with the progress polled for the dialog. Stops when the
/// user cancels or the editor is gone.
#[cfg(not(target_family = "wasm"))]
async fn download_entry(this: &WeakEntity<SlideEditor>, cx: &mut AsyncApp, entry: &FontEntry, cancel: &Arc<AtomicBool>) -> Result<(), DownloadError> {
    let progress = Arc::new(AtomicU64::new(0));
    let (sender, receiver) = std::sync::mpsc::channel();
    {
        let (entry, progress, cancel) = (entry.clone(), progress.clone(), cancel.clone());
        std::thread::spawn(move || {
            sender.send(font_setup::download(&entry, &progress, &cancel)).ok();
        });
    }
    loop {
        cx.background_executor().timer(Duration::from_millis(120)).await;
        let received = progress.load(Ordering::SeqCst);
        let alive = this.update(cx, |this, cx| {
            if let Some(state) = this.font_setup_state() {
                state.borrow_mut().received = received;
            }
            cx.notify();
        });
        if alive.is_err() {
            cancel.store(true, Ordering::SeqCst);
        }
        match receiver.try_recv() {
            Ok(result) => return result,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => return Err(DownloadError::Failed("the download thread ended".into())),
        }
    }
}

/// The browser keeps the font in IndexedDB, in chunks (`crate::web`).
#[cfg(target_family = "wasm")]
async fn download_entry(_this: &WeakEntity<SlideEditor>, _cx: &mut AsyncApp, entry: &FontEntry, cancel: &Arc<AtomicBool>) -> Result<(), DownloadError> {
    let _ = AtomicU64::new(0);
    if cancel.load(Ordering::SeqCst) {
        return Err(DownloadError::Cancelled);
    }
    crate::web::font_download(&entry.file, &entry.url, entry.size).await.map_err(DownloadError::Failed)
}
