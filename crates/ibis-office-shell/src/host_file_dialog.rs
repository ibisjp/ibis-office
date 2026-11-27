// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The file window of a host: 開く, 名前を付けて保存, 書き出し (and コピーを作成) in one window of the editor's own, over the host's
//! `listFolder` (design 2.6 and 5.3). The folder tree is on the left, the files on the right.
//!
//! A host keeps hundreds of thousands of folders, so nothing is fetched up front: the window starts with the folder to show (its path
//! comes back with the listing), and then only the folders from the top down to it; the children of a node are fetched when it is opened
//! (1000 at a time, with `さらに表示…` for the rest) and kept until the window closes; the right list is one page of the folder chosen,
//! the next page coming when the list is scrolled to its end. At most 4 requests go at once. A result of a folder that is not the one
//! shown any more is dropped, and so is one that comes after the window was closed. The window is built of the shared parts (the dialog
//! panel, `DataTable::virtualized`, the text fields, the action bar); the message boxes are the shared ones.

use std::collections::{HashMap, HashSet, VecDeque};

use gpui_kit::component::input::InputState;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::chrome::data_table::DataTable;
use crate::chrome::virtual_table::{TableColumn, TableRow, TableState};
use crate::chrome::{ActionButtonKind, ActionDefinition, SidebarPanel};
use crate::editor::SlideEditor;
use crate::host::{self, HostFailure, HostFileEntry, HostFolder, HostFolderRef, HostListing, ListRequest, Purpose, SaveKind};
use crate::i18n::{format, text};
use crate::icons::Material;
use crate::palette::rgb;

/// The width of the window.
pub(crate) const DIALOG_WIDTH: f32 = 820.;
/// The dialog is this high (or as high as the window allows): the folder tree and the file list take what the fixed rows leave, whatever they hold.
pub(crate) const DIALOG_HEIGHT: f32 = 560.;
/// The width of the folder tree (the splitter's left side) is kept in the app's settings (K0045).
pub const SPLIT_KEY: &str = "dialog.host-file.split";
pub(crate) const SPLIT_DEFAULT: f32 = 240.;
/// The narrowest the tree is dragged, and the narrowest the file list keeps.
pub const SPLIT_MIN: f32 = 120.;
const LIST_MIN: f32 = 240.;
const SPLITTER_WIDTH: f32 = 9.;

/// The tree's width the user last chose (an absent or unreadable value is the default; not yet fitted to the dialog).
fn remembered_split() -> f32 {
    crate::settings::get(SPLIT_KEY).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(SPLIT_DEFAULT)
}

/// The tree's width `split` kept inside a dialog `dialog_width` wide: the tree is at least [`SPLIT_MIN`] and the list keeps at least
/// [`LIST_MIN`] (the dialog's padding, border and the splitter are taken off the width first).
pub fn fit_split(split: f32, dialog_width: f32) -> f32 {
    let room = dialog_width - 2. * crate::chrome::FORM_PADDING - 2. - SPLITTER_WIDTH;
    split.clamp(SPLIT_MIN, (room - LIST_MIN).max(SPLIT_MIN))
}

/// The most requests to the host at once.
const MAX_IN_FLIGHT: u32 = 4;
/// One page of a listing.
const PAGE: u32 = 1000; // a page of listFolder: 1000 entries (the user's choice, 2026-11-20)
/// The most pages read for the same name when the host cannot check the target itself.
const MAX_CHECK_PAGES: u32 = 100;
const ROW_HEIGHT: f32 = 26.;
/// The file name's longest length (what the file systems allow).
const NAME_MAX: usize = 255;
/// The extensions the editor opens.
const OPEN_EXTENSIONS: [&str; 8] = ["docx", "xlsx", "pptx", "doc", "xls", "ppt", "xdw", "xbd"];

/// What the window is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Open,
    SaveAs,
    Export,
}

impl Mode {
    fn purpose(self) -> Purpose {
        match self {
            Mode::Open => Purpose::Open,
            Mode::SaveAs => Purpose::SaveAs,
            Mode::Export => Purpose::Export,
        }
    }
}

/// What 書き出し writes: the name, the format's extension and the bytes the caller made.
#[derive(Clone, Debug)]
pub struct ExportJob {
    pub name: String,
    pub extension: String,
    pub bytes: Vec<u8>,
    /// What the status bar says once it is written, when something is lacking (characters left out of a PDF); else 書き出しました.
    pub note: Option<String>,
}

/// Where the keyboard is in the window: Tab goes tree, list, name (the field and the buttons after it), as the design says (2.6).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pane {
    Tree,
    List,
    Name,
}

enum Job {
    /// The sub folders of a folder (a tree node), a page (`deferred`: asked with the host's `deferredCursor`, by a press of 続きの表示).
    Children { folder: Option<String>, cursor: Option<String>, deferred: bool },
    /// The right list: a page of the folder shown (the token tells a stale result).
    List { folder: Option<String>, cursor: Option<String>, token: u64, deferred: bool },
}

#[derive(Default)]
struct Node {
    children: Vec<String>,
    next_cursor: Option<String>,
    /// The continuation the host is slow to make: 続きの表示 asks with it, once the user presses it (K0041).
    deferred_cursor: Option<String>,
    deferred_loading: bool,
    loaded: bool,
    loading: bool,
    error: Option<String>,
}

#[derive(Clone)]
enum ListRowKind {
    Folder(HostFolder),
    File(HostFileEntry),
}

#[derive(Default)]
struct ListPane {
    rows: Vec<ListRowKind>,
    next_cursor: Option<String>,
    deferred_cursor: Option<String>,
    deferred_loading: bool,
    loading: bool,
    error: Option<String>,
    token: u64,
}

type Then = Box<dyn FnOnce(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>)>;

/// The window's state (one field of the editor's `EditorHost`).
pub struct HostFileDialogState {
    id: u64,
    mode: Mode,
    kind: SaveKind,
    export: Option<ExportJob>,
    /// The save's extension (without the dot): the open document's own, or the export's format.
    extension: String,
    nodes: HashMap<Option<String>, Node>,
    folders: HashMap<String, HostFolder>,
    parents: HashMap<String, Option<String>>,
    expanded: HashSet<Option<String>>,
    current: Option<String>,
    path: Vec<HostFolderRef>,
    list: ListPane,
    selected: Option<String>,
    name_input: Entity<InputState>,
    search_input: Entity<InputState>,
    tree_table: TableState,
    list_table: TableState,
    /// The pane the arrow keys act on (Tab moves it: the tree, the list, the name field).
    pane: Pane,
    queue: VecDeque<Job>,
    in_flight: u32,
    /// The folders from the top down to the first one shown are fetched once its path is known.
    path_pending: bool,
    busy: bool,
    then: Option<Then>,
    /// The tree's width and, while the splitter is held, the pointer's x and the width it was grabbed at.
    split: f32,
    split_drag: Option<(f32, f32)>,
    _subscriptions: Vec<Subscription>,
}

impl HostFileDialogState {
    /// The tree's width (the scenarios use it).
    pub fn split(&self) -> f32 {
        self.split
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The rows of the tree as written: a folder by its name, and `[more]` (さらに表示…), `[deferred]` (続きの表示), `[deferred-loading]` (取得中...),
    /// `[loading]` for the rows that are not folders; with their depth (the scenarios use it).
    pub fn tree_labels(&self) -> Vec<(usize, String)> {
        self.tree_rows()
            .into_iter()
            .map(|r| {
                let label = match r.kind {
                    TreeKind::Folder(id) => self.folders.get(&id).map(|f| f.name.clone()).unwrap_or_default(),
                    TreeKind::More(_) => "[more]".to_string(),
                    TreeKind::Deferred(_) => "[deferred]".to_string(),
                    TreeKind::DeferredLoading => "[deferred-loading]".to_string(),
                    TreeKind::Loading => "[loading]".to_string(),
                    TreeKind::Error(message) => format!("[error {message}]"),
                };
                (r.depth, label)
            })
            .collect()
    }

    /// The row after the right list's last: `Some(true)` 続きの表示 (to press), `Some(false)` 取得中..., `None` none.
    pub fn list_tail(&self) -> Option<bool> {
        if self.list.next_cursor.is_some() {
            None
        } else if self.list.deferred_loading {
            Some(false)
        } else if self.list.deferred_cursor.is_some() {
            Some(true)
        } else {
            None
        }
    }

    /// The folder shown (`None`: the top).
    pub fn current_folder(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// The names in the right list (folders first).
    pub fn list_names(&self) -> Vec<String> {
        self.list
            .rows
            .iter()
            .map(|r| match r {
                ListRowKind::Folder(f) => f.name.clone(),
                ListRowKind::File(f) => f.name.clone(),
            })
            .collect()
    }

    /// The tree's folders by depth (the top is depth 0).
    pub fn tree_names(&self) -> Vec<(usize, String)> {
        self.tree_rows()
            .into_iter()
            .filter_map(|r| match r.kind {
                TreeKind::Folder(id) => self.folders.get(&id).map(|f| (r.depth, f.name.clone())),
                _ => None,
            })
            .collect()
    }

    /// The name of the file chosen in the list.
    pub fn selected_name(&self) -> Option<String> {
        let id = self.selected.as_ref()?;
        self.list.rows.iter().find_map(|r| match r {
            ListRowKind::File(f) if &f.id == id => Some(f.name.clone()),
            _ => None,
        })
    }

    /// Whether a request is going or waiting.
    pub fn is_loading(&self) -> bool {
        self.in_flight > 0 || !self.queue.is_empty()
    }

    fn tree_rows(&self) -> Vec<TreeRow> {
        // The top of the tree is the host's roots, side by side (K0041): there is no folder above them.
        let mut rows = Vec::new();
        self.push_children(&None, 0, &mut rows);
        rows
    }

    fn push_children(&self, parent: &Option<String>, depth: usize, rows: &mut Vec<TreeRow>) {
        let Some(node) = self.nodes.get(parent) else { return };
        for id in &node.children {
            rows.push(TreeRow { depth, kind: TreeKind::Folder(id.clone()) });
            if self.expanded.contains(&Some(id.clone())) {
                self.push_children(&Some(id.clone()), depth + 1, rows);
            }
        }
        if node.loading {
            rows.push(TreeRow { depth, kind: TreeKind::Loading });
        } else if let Some(error) = &node.error {
            rows.push(TreeRow { depth, kind: TreeKind::Error(error.clone()) });
        } else if node.next_cursor.is_some() {
            rows.push(TreeRow { depth, kind: TreeKind::More(parent.clone()) });
        } else if node.deferred_loading {
            rows.push(TreeRow { depth, kind: TreeKind::DeferredLoading });
        } else if node.deferred_cursor.is_some() {
            // The quick pages are all in: the slow continuation waits for the user's press.
            rows.push(TreeRow { depth, kind: TreeKind::Deferred(parent.clone()) });
        }
    }

    /// The name as it is saved: the typed name with the save's extension (an extension typed otherwise is replaced), or the error of it.
    fn checked_name(&self, typed: &str) -> Result<String, NameError> {
        let typed = typed.trim();
        let ext = &self.extension;
        let text_family = |e: &str| ["csv", "tsv", "tab"].iter().any(|t| e.eq_ignore_ascii_case(t));
        let mut ext = ext.as_str();
        let stem = match typed.rsplit_once('.') {
            Some((stem, e)) if !stem.is_empty() && e.eq_ignore_ascii_case(ext) => stem,
            // A text file is saved as the other text format when the name says so (.csv, .tsv, .tab).
            // A workbook read from a text file can be saved as a workbook (.xlsx): Excel's 名前を付けて保存 offers it for a csv it opened.
            Some((stem, e)) if !stem.is_empty() && text_family(ext) && (text_family(e) || e.eq_ignore_ascii_case("xlsx")) => {
                ext = e;
                stem
            }
            _ => typed,
        };
        let stem = stem.trim_end();
        if stem.is_empty() {
            return Err(NameError::Empty);
        }
        if let Some(c) = stem.chars().find(|c| c.is_control() || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')) {
            return Err(NameError::BadChar(c));
        }
        if stem.ends_with('.') || stem.starts_with(' ') {
            return Err(NameError::BadEnd);
        }
        let name = if ext.is_empty() { stem.to_string() } else { format!("{stem}.{ext}") };
        if name.len() > NAME_MAX {
            return Err(NameError::TooLong);
        }
        Ok(name)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum NameError {
    Empty,
    BadChar(char),
    BadEnd,
    TooLong,
}

impl NameError {
    fn message(&self) -> String {
        match self {
            NameError::Empty => text("shell.host-file.error-empty").to_string(),
            NameError::BadChar(c) => format("shell.host-file.error-char", &[("char", c)]),
            NameError::BadEnd => text("shell.host-file.error-end").to_string(),
            NameError::TooLong => text("shell.host-file.error-long").to_string(),
        }
    }
}

struct TreeRow {
    depth: usize,
    kind: TreeKind,
}

#[derive(Clone)]
enum TreeKind {
    Folder(String),
    More(Option<String>),
    /// 続きの表示 of a node (K0041).
    Deferred(Option<String>),
    DeferredLoading,
    Loading,
    Error(String),
}

fn own_extension(kind: ibis_formats::Kind) -> &'static str {
    match kind {
        ibis_formats::Kind::Document => "docx",
        ibis_formats::Kind::Workbook => "xlsx",
        ibis_formats::Kind::Presentation => "pptx",
        ibis_formats::Kind::Pdf => "pdf",
    }
}

fn failure_message(failure: &HostFailure) -> String {
    failure.message.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| text("shell.host-file.error-list").to_string())
}

/// `updatedAt` (ISO 8601) as `yyyy/MM/dd HH:mm` in the time zone of the browser (the editor on a desktop has none of its own: there the
/// time is shown as the host wrote it). A text that is not a date, or has no offset (`2026-11-15T09:00:00`: a local time already), is
/// shown as written.
pub(crate) fn short_date(iso: &str) -> String {
    format_date(iso, local_offset_minutes)
}

/// The time zone of the browser at the instant `utc_millis`: minutes east of UTC (Japan: 540). `None` where there is no such thing.
#[cfg(target_family = "wasm")]
fn local_offset_minutes(utc_millis: i64) -> Option<i32> {
    // `Date.getTimezoneOffset` is minutes west of UTC, at that instant (summer time counts).
    Some(-(js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(utc_millis as f64)).get_timezone_offset() as i32))
}

#[cfg(not(target_family = "wasm"))]
fn local_offset_minutes(_utc_millis: i64) -> Option<i32> {
    None
}

/// `short_date` with the zone given by `offset_at` (the instant in UTC milliseconds to minutes east of UTC).
fn format_date(iso: &str, offset_at: impl Fn(i64) -> Option<i32>) -> String {
    let b = iso.as_bytes();
    let digits = |from: usize, to: usize| b.get(from..to).is_some_and(|s| s.iter().all(u8::is_ascii_digit));
    if !(b.len() >= 16 && digits(0, 4) && b[4] == b'-' && digits(5, 7) && b[7] == b'-' && digits(8, 10) && (b[10] == b'T' || b[10] == b' ') && digits(11, 13) && b[13] == b':' && digits(14, 16)) {
        return iso.to_string();
    }
    let written = || format!("{}/{}/{} {}:{}", &iso[0..4], &iso[5..7], &iso[8..10], &iso[11..13], &iso[14..16]);
    // After the minutes: optional seconds and fraction, then `Z` or `+hh:mm` / `+hhmm` / `+hh`.
    let mut at = 16;
    if b.get(at) == Some(&b':') && digits(at + 1, at + 3) {
        at += 3;
        if b.get(at) == Some(&b'.') {
            at += 1;
            while b.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
            }
        }
    }
    let host_offset: i32 = match b.get(at) {
        Some(b'Z') | Some(b'z') if at + 1 == b.len() => 0,
        Some(sign @ (b'+' | b'-')) => {
            let rest = &iso[at + 1..];
            let compact: String = rest.chars().filter(|c| *c != ':').collect();
            if !(compact.len() == 2 || compact.len() == 4) || !compact.bytes().all(|c| c.is_ascii_digit()) {
                return written();
            }
            let hours: i32 = compact[0..2].parse().unwrap_or(0);
            let minutes: i32 = if compact.len() == 4 { compact[2..4].parse().unwrap_or(0) } else { 0 };
            if hours > 23 || minutes > 59 {
                return written();
            }
            let total = hours * 60 + minutes;
            if *sign == b'-' { -total } else { total }
        }
        _ => return written(),
    };
    let num = |from: usize, to: usize| iso[from..to].parse::<i64>().unwrap_or(0);
    let (year, month, day, hour, minute) = (num(0, 4), num(5, 7), num(8, 10), num(11, 13), num(14, 16));
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return written();
    }
    // Minutes since 1970 of the written time, then of the instant (UTC), then in the browser's zone at that instant.
    let utc_minutes = days_from_civil(year, month, day) * 1440 + hour * 60 + minute - i64::from(host_offset);
    let Some(offset) = offset_at(utc_minutes * 60_000) else { return written() };
    let local = utc_minutes + i64::from(offset);
    let (days, rest) = (local.div_euclid(1440), local.rem_euclid(1440));
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}/{m:02}/{d:02} {:02}:{:02}", rest / 60, rest % 60)
}

/// Days since 1970-01-01 of a date of the proleptic Gregorian calendar (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod date_tests {
    use super::format_date;

    #[test]
    fn the_time_is_shown_in_the_zone_of_the_browser() {
        let tokyo = |_| Some(540);
        let los_angeles = |_| Some(-480);
        // The same instant, written in three ways, is one time in Tokyo.
        assert_eq!(format_date("2026-11-15T00:30:00Z", tokyo), "2026/11/15 09:30");
        assert_eq!(format_date("2026-11-14T19:30:00-05:00", tokyo), "2026/11/15 09:30");
        assert_eq!(format_date("2026-11-15T09:30:00.123+09:00", tokyo), "2026/11/15 09:30");
        assert_eq!(format_date("2026-11-15T09:30+0900", tokyo), "2026/11/15 09:30");
        // Across a day, a month and a year (and a leap day).
        assert_eq!(format_date("2026-12-31T20:00:00Z", tokyo), "2027/01/01 05:00");
        assert_eq!(format_date("2028-02-29T23:59:59Z", tokyo), "2028/03/01 08:59");
        assert_eq!(format_date("2026-01-01T03:00:00Z", los_angeles), "2025/12/31 19:00");
        // A zone with minutes (India, +05:30; Nepal's +05:45 the same way).
        assert_eq!(format_date("2026-11-15T00:00:00Z", |_| Some(330)), "2026/11/15 05:30");
        // The zone is asked about the instant (summer time): the offset changes with it.
        let summer = |ms: i64| Some(if ms >= 1_770_000_000_000 { 60 } else { 0 });
        assert_eq!(format_date("2026-05-28T00:00:00Z", summer), "2026/05/28 01:00");
        assert_eq!(format_date("2026-01-01T00:00:00Z", summer), "2026/01/01 00:00");
        // No zone to ask (a desktop), no offset in the text, or not a date: as written.
        assert_eq!(format_date("2026-11-15T00:30:00Z", |_| None), "2026/11/15 00:30");
        assert_eq!(format_date("2026-11-15T09:30:00", tokyo), "2026/11/15 09:30");
        assert_eq!(format_date("2026-11-15 09:30", tokyo), "2026/11/15 09:30");
        assert_eq!(format_date("yesterday", tokyo), "yesterday");
        assert_eq!(format_date("2026-13-15T00:00:00Z", tokyo), "2026/13/15 00:00");
    }
}

/// A size in KB, rounded up (under 1 KB is 1 KB).
pub(crate) fn size_kb(bytes: u64) -> String {
    format!("{} KB", bytes.div_ceil(1024).max(1))
}

/// The next window starts in the first root, as in a run that has not used one yet (the scenarios use it).
pub fn forget_last_folder() {
    LAST_FOLDER.with(|f| *f.borrow_mut() = None);
}

thread_local! {
    static NEXT_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
    /// The folder last used in a window, for the next one of this run.
    static LAST_FOLDER: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

impl SlideEditor {
    /// Open the file window for 開く or 名前を付けて保存. `then` runs once a save in it succeeded.
    pub(crate) fn open_host_file_dialog(&mut self, mode: Mode, then: Option<Then>, window: &mut Window, cx: &mut Context<Self>) {
        self.open_host_dialog(mode, SaveKind::SaveAs, None, then, window, cx);
    }

    /// コピーを作成: the file window; the document is not tied to the copy.
    pub(crate) fn open_host_copy_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_host_dialog(Mode::SaveAs, SaveKind::Copy, None, None, window, cx);
    }

    /// 書き出し: the file window for the file `job` makes (the bytes were made by the caller); it is saved to the host's storage.
    pub(crate) fn open_host_export_dialog(&mut self, job: ExportJob, window: &mut Window, cx: &mut Context<Self>) {
        self.open_host_dialog(Mode::Export, SaveKind::Export, Some(job), None, window, cx);
    }

    fn open_host_dialog(&mut self, mode: Mode, kind: SaveKind, export: Option<ExportJob>, then: Option<Then>, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        if self.dialog.active {
            self.close_dialog(window, cx);
        }
        let extension = match (&export, mode) {
            (Some(job), _) => job.extension.clone(),
            (None, Mode::Open) => String::new(),
            // A workbook read from a text file is saved as text again (.csv, .tsv); the others in their own format.
            (None, _) => match self.host_text_format(self.file_name.as_ref().map(|n| n.to_string()).as_deref()) {
                Some(format) => self.file_name.as_ref().and_then(|n| n.rsplit_once('.')).map(|(_, e)| e.to_ascii_lowercase()).filter(|e| e == "tab").unwrap_or_else(|| format.to_string()),
                None => own_extension(ibis_formats::Kind::of(self.presentation())).to_string(),
            },
        };
        let default_name = match (&export, mode) {
            (Some(job), _) => job.name.clone(),
            (None, Mode::Open) => String::new(),
            (None, _) => {
                let name = self.file_name.as_ref().map(|n| n.to_string()).unwrap_or_default();
                // An old binary file (.xls, .doc, .ppt) is offered under the name of the current format.
                match name.rsplit_once('.') {
                    Some((stem, _)) if self.opened_from_legacy && !stem.is_empty() => format!("{stem}.{extension}"),
                    _ => name,
                }
            }
        };
        let name_input = cx.new(|cx| InputState::new(window, cx).default_value(default_name));
        let search_input = cx.new(|cx| InputState::new(window, cx));
        let subscriptions = vec![cx.observe(&name_input, |_, _, cx| cx.notify()), cx.observe(&search_input, |_, _, cx| cx.notify()), Self::host_fd_key_interceptor(window, cx)];
        // The folder of the open file, else the one last used in this run, else the top.
        let start = self.host.binding.as_ref().and_then(|b| b.folder_id.clone()).or_else(|| LAST_FOLDER.with(|f| f.borrow().clone()));
        let id = NEXT_ID.with(|n| n.replace(n.get() + 1));
        let mut state = HostFileDialogState {
            id,
            mode,
            kind,
            export,
            extension,
            nodes: HashMap::new(),
            folders: HashMap::new(),
            parents: HashMap::new(),
            expanded: HashSet::new(),
            current: start.clone(),
            path: Vec::new(),
            list: ListPane { loading: true, ..ListPane::default() },
            selected: None,
            name_input: name_input.clone(),
            search_input,
            tree_table: TableState::default(),
            list_table: TableState::default(),
            pane: Pane::Name,
            queue: VecDeque::new(),
            in_flight: 0,
            path_pending: start.is_some(),
            busy: false,
            then,
            split: remembered_split(),
            split_drag: None,
            _subscriptions: subscriptions,
        };
        state.nodes.entry(None).or_default().loading = true;
        // With no folder to start in, the first root is shown once the roots are in.
        if start.is_some() {
            state.queue.push_back(Job::List { folder: start, cursor: None, token: 0, deferred: false });
        }
        state.queue.push_back(Job::Children { folder: None, cursor: None, deferred: false });
        self.host.dialog = Some(state);
        self.sidebar_open = true;
        self.dialog = Default::default();
        self.host_fd_pump(window, cx);
        if mode != Mode::Open {
            name_input.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    /// The window is closed (its × or キャンセル, or after the save): what is still coming for it is dropped.
    pub(crate) fn host_fd_discard(&mut self) {
        self.host.dialog = None;
    }

    fn host_fd_pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        loop {
            let Some(bridge) = host::bridge_with(|c| c.list_folder) else { return };
            let Some(state) = self.host.dialog.as_mut() else { return };
            if state.in_flight >= MAX_IN_FLIGHT {
                return;
            }
            let Some(job) = state.queue.pop_front() else { return };
            state.in_flight += 1;
            let id = state.id;
            let extensions: Vec<String> = match state.mode {
                Mode::Open => OPEN_EXTENSIONS.iter().map(|e| e.to_string()).collect(),
                _ => vec![state.extension.clone()],
            };
            let request = match &job {
                Job::Children { folder, cursor, .. } => ListRequest { folder_id: folder.clone(), purpose: state.mode.purpose(), include_folders: true, include_files: false, extensions: Vec::new(), cursor: cursor.clone(), limit: PAGE },
                Job::List { folder, cursor, .. } => ListRequest { folder_id: folder.clone(), purpose: state.mode.purpose(), include_folders: true, include_files: true, extensions, cursor: cursor.clone(), limit: PAGE },
            };
            cx.spawn_in(window, async move |this, cx| {
                let result = bridge.list_folder(request).await;
                this.update_in(cx, |this, window, cx| this.host_fd_result(id, job, result, window, cx)).ok();
            })
            .detach();
        }
    }

    fn host_fd_result(&mut self, id: u64, job: Job, result: Result<HostListing, HostFailure>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_mut().filter(|s| s.id == id) else { return };
        state.in_flight = state.in_flight.saturating_sub(1);
        // What is said under the window when 続きの表示 brought nothing new; the root shown first when none was chosen yet.
        let mut nothing_more = false;
        let mut first_root: Option<String> = None;
        match job {
            Job::Children { folder, cursor, deferred } => {
                let node = state.nodes.entry(folder.clone()).or_default();
                node.loading = false;
                node.deferred_loading = false;
                match result {
                    Ok(listing) => {
                        let before = node.children.len();
                        node.loaded = true;
                        node.error = None;
                        node.next_cursor = listing.next_cursor.clone();
                        // The slow continuation, once it was asked for, is not offered again; what the host says before it, is remembered.
                        node.deferred_cursor = if deferred { None } else { listing.deferred_cursor.clone().or(node.deferred_cursor.take()) };
                        for child in listing.folders {
                            // A folder that came first and again with the continuation is one (by its id).
                            if !node.children.contains(&child.id) {
                                node.children.push(child.id.clone());
                            }
                            state.parents.insert(child.id.clone(), folder.clone());
                            state.folders.insert(child.id.clone(), child);
                        }
                        let node = state.nodes.entry(folder.clone()).or_default();
                        nothing_more = deferred && listing.next_cursor.is_none() && node.children.len() == before;
                        if folder.is_none() && state.current.is_none() {
                            first_root = node.children.first().cloned();
                            if first_root.is_none() {
                                state.list.loading = false;
                            }
                        }
                    }
                    Err(failure) => {
                        node.error = Some(failure_message(&failure));
                        if deferred {
                            // The press can be made again.
                            node.deferred_cursor = cursor;
                        }
                        if folder.is_none() && state.current.is_none() {
                            state.list.loading = false;
                            state.list.error = node.error.clone();
                        }
                    }
                }
            }
            Job::List { folder, cursor, token, deferred } => {
                // A page of another folder than the one shown is not for it.
                if token == state.list.token && folder == state.current {
                    state.list.loading = false;
                    state.list.deferred_loading = false;
                    match result {
                        Ok(listing) => {
                            state.list.error = None;
                            if cursor.is_none() {
                                state.list.rows.clear();
                            }
                            // A folder that was listed with no sub folder at all has none, whatever `hasSubFolders` said: its arrow in the tree goes (K0046).
                            let childless = cursor.is_none() && listing.folders.is_empty() && listing.next_cursor.is_none() && listing.deferred_cursor.is_none();
                            if let Some((info, path)) = &listing.folder {
                                state.folders.insert(info.id.clone(), info.clone());
                                if childless && let Some(known) = state.folders.get_mut(&info.id) {
                                    known.has_sub_folders = false;
                                }
                                // The way the user came (the tree's, through a container such as the joined folders) stays; the host's own path
                                // is for the first folder, and for a folder not reached by the tree.
                                if state.path_pending || state.path.last().map(|p| &p.id) != Some(&info.id) {
                                    state.path = path.clone();
                                }
                                if state.path_pending {
                                    state.path_pending = false;
                                    // The top down to this folder: their children are fetched (the tree opens to it).
                                    let mut chain: Vec<Option<String>> = vec![None];
                                    let mut parent: Option<String> = None;
                                    for step in path {
                                        state.parents.insert(step.id.clone(), parent.clone());
                                        parent = Some(step.id.clone());
                                        chain.push(Some(step.id.clone()));
                                    }
                                    for node_id in chain {
                                        state.expanded.insert(node_id.clone());
                                        let node = state.nodes.entry(node_id.clone()).or_default();
                                        if !node.loaded && !node.loading {
                                            node.loading = true;
                                            state.queue.push_back(Job::Children { folder: node_id, cursor: None, deferred: false });
                                        }
                                    }
                                }
                            } else if folder.is_none() {
                                state.path.clear();
                            }
                            state.list.next_cursor = listing.next_cursor.clone();
                            state.list.deferred_cursor = if deferred { None } else { listing.deferred_cursor.clone().or(state.list.deferred_cursor.take()) };
                            let before = state.list.rows.len();
                            let known: HashSet<String> = state
                                .list
                                .rows
                                .iter()
                                .map(|r| match r {
                                    ListRowKind::Folder(f) => f.id.clone(),
                                    ListRowKind::File(f) => f.id.clone(),
                                })
                                .collect();
                            state.list.rows.extend(listing.folders.into_iter().filter(|f| !known.contains(&f.id)).map(ListRowKind::Folder));
                            state.list.rows.extend(listing.files.into_iter().filter(|f| !known.contains(&f.id)).map(ListRowKind::File));
                            nothing_more = deferred && listing.next_cursor.is_none() && state.list.rows.len() == before;
                        }
                        Err(failure) => {
                            state.list.error = Some(failure_message(&failure));
                            state.path_pending = false;
                            if deferred {
                                state.list.deferred_cursor = cursor;
                            }
                        }
                    }
                }
            }
        }
        if nothing_more {
            self.set_status(text("shell.host-file.deferred-empty").to_string(), cx);
        }
        if let Some(root) = first_root {
            self.host_fd_show_folder(Some(root), true, window, cx);
            return;
        }
        self.host_fd_pump(window, cx);
        cx.notify();
    }

    /// The tree's row (not its arrow) is clicked: the folder becomes the current one and its contents show in the list; the tree is not
    /// opened or closed by it (K0046).
    pub fn host_fd_select_current(&mut self, folder: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.host_fd_show_folder(folder, false, window, cx);
    }

    /// The tree's arrow is clicked (the scenarios use it): the node is opened or closed, the current folder stays.
    pub fn host_fd_toggle_node(&mut self, folder: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.host_fd_toggle(folder, window, cx);
    }

    /// A folder is chosen (the tree's row, a folder of the list, a part of the address): it is shown, and opened in the tree.
    pub fn host_fd_select_folder(&mut self, folder: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.host_fd_show_folder(folder, true, window, cx);
    }

    /// `open_in_tree`: the folder is also opened in the tree (not when the tree's own row was clicked).
    fn host_fd_show_folder(&mut self, folder: Option<String>, open_in_tree: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_mut() else { return };
        if state.busy {
            return;
        }
        state.current = folder.clone();
        state.selected = None;
        state.list = ListPane { loading: true, token: state.list.token + 1, ..ListPane::default() };
        state.list_table = TableState::default();
        // The tree's pages stay in the queue; the page of the folder before is not wanted any more.
        state.queue.retain(|job| matches!(job, Job::Children { .. }));
        let token = state.list.token;
        state.queue.push_back(Job::List { folder: folder.clone(), cursor: None, token, deferred: false });
        // The address: from the folders known by now (the listing brings the host's own).
        let mut path = Vec::new();
        let mut at = folder.clone();
        while let Some(id) = at {
            if let Some(info) = state.folders.get(&id) {
                path.push(HostFolderRef { id: id.clone(), name: info.name.clone() });
            }
            at = state.parents.get(&id).cloned().flatten();
        }
        path.reverse();
        state.path = path;
        if folder.is_some() && open_in_tree {
            state.expanded.insert(folder.clone());
            let has_sub = folder.as_ref().and_then(|id| state.folders.get(id)).is_none_or(|f| f.has_sub_folders);
            let node = state.nodes.entry(folder.clone()).or_default();
            if has_sub && !node.loaded && !node.loading {
                node.loading = true;
                state.queue.push_back(Job::Children { folder: folder.clone(), cursor: None, deferred: false });
            }
        }
        LAST_FOLDER.with(|f| *f.borrow_mut() = folder);
        self.host_fd_pump(window, cx);
        cx.notify();
    }

    /// The tree's node is opened or closed by its arrow.
    fn host_fd_toggle(&mut self, folder: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_mut() else { return };
        if !state.expanded.remove(&folder) {
            state.expanded.insert(folder.clone());
            let node = state.nodes.entry(folder.clone()).or_default();
            if !node.loaded && !node.loading {
                node.loading = true;
                state.queue.push_back(Job::Children { folder, cursor: None, deferred: false });
            }
        }
        self.host_fd_pump(window, cx);
        cx.notify();
    }

    /// さらに表示… of a tree node: the next page of its children.
    fn host_fd_more_children(&mut self, folder: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_mut() else { return };
        let node = state.nodes.entry(folder.clone()).or_default();
        let Some(cursor) = node.next_cursor.take() else { return };
        node.loading = true;
        state.queue.push_back(Job::Children { folder, cursor: Some(cursor), deferred: false });
        self.host_fd_pump(window, cx);
        cx.notify();
    }

    /// 続きの表示 of a tree node: the slow continuation the host kept back, asked for now (only the user's press asks for it).
    pub fn host_fd_deferred_children(&mut self, folder: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_mut() else { return };
        let node = state.nodes.entry(folder.clone()).or_default();
        let Some(cursor) = node.deferred_cursor.take() else { return };
        node.deferred_loading = true;
        state.queue.push_back(Job::Children { folder, cursor: Some(cursor), deferred: true });
        self.host_fd_pump(window, cx);
        cx.notify();
    }

    /// 続きの表示 at the end of the right list.
    pub fn host_fd_deferred_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_mut() else { return };
        let Some(cursor) = state.list.deferred_cursor.take() else { return };
        state.list.deferred_loading = true;
        let (folder, token) = (state.current.clone(), state.list.token);
        state.queue.push_back(Job::List { folder, cursor: Some(cursor), token, deferred: true });
        self.host_fd_pump(window, cx);
        cx.notify();
    }

    /// The list was scrolled near its end: the next page.
    pub fn host_fd_more_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_mut() else { return };
        if state.list.loading || state.list.error.is_some() {
            return;
        }
        let Some(cursor) = state.list.next_cursor.take() else { return };
        state.list.loading = true;
        let (folder, token) = (state.current.clone(), state.list.token);
        state.queue.push_back(Job::List { folder, cursor: Some(cursor), token, deferred: false });
        self.host_fd_pump(window, cx);
    }

    /// A key in the file window (2.6 of the design): ↑↓ move the choice in the list (in the tree while Tab put the keyboard there),
    /// ←→ close and open a folder of the tree, Backspace or Alt+↑ go to the parent folder, Tab goes tree, list, name field (then the
    /// field's own Tab to the buttons). Not while a field is being typed in, but for ↑↓ and Tab out of the list and tree. True when the
    /// key was used.
    pub(crate) fn host_fd_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(state) = self.host.dialog.as_ref() else { return false };
        if state.busy || key.modifiers.platform || key.modifiers.control {
            return false;
        }
        // A field being typed in keeps Backspace and the arrows (the search field is focused when the window opens; with nothing typed
        // in it the keys are the list's). Tab leaves every field but the name field (its Tab goes to the buttons).
        let name_focused = state.name_input.read(cx).focus_handle(cx).is_focused(window);
        let search_focused = state.search_input.read(cx).focus_handle(cx).is_focused(window);
        let typing = name_focused || (search_focused && !state.search_input.read(cx).value().is_empty());
        let pane = state.pane;
        let (alt, shift) = (key.modifiers.alt, key.modifiers.shift);
        match key.key.as_str() {
            "up" | "down" if !alt && !shift => {
                let step: isize = if key.key == "down" { 1 } else { -1 };
                if pane == Pane::Tree && !typing {
                    self.host_fd_step_tree(step, window, cx);
                } else {
                    self.host_fd_step_list(step, window, cx);
                }
                true
            }
            "up" if alt && !shift => self.host_fd_parent(window, cx),
            "backspace" if !alt && !shift && !typing => self.host_fd_parent(window, cx),
            "left" | "right" if !alt && !shift && !typing && pane == Pane::Tree => {
                let Some(state) = self.host.dialog.as_ref() else { return false };
                let folder = state.current.clone();
                let open = state.expanded.contains(&folder);
                if key.key == "right" {
                    if !open {
                        self.host_fd_toggle(folder, window, cx);
                    }
                } else if open && folder.is_some() {
                    self.host_fd_toggle(folder, window, cx);
                } else {
                    self.host_fd_parent(window, cx);
                }
                true
            }
            _ => false,
        }
    }

    /// Tab (or Shift+Tab) in the file window: the keyboard goes tree, list, name field (Open: the search field). Not from the name field:
    /// its Tab goes on to the buttons as everywhere. True when the key was used.
    fn host_fd_tab(&mut self, shift: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(state) = self.host.dialog.as_ref() else { return false };
        if state.busy || state.name_input.read(cx).focus_handle(cx).is_focused(window) {
            return false;
        }
        let next = match (state.pane, shift) {
            (Pane::Tree, false) => Pane::List,
            (Pane::List, false) => Pane::Name,
            (Pane::Name, false) => Pane::Tree,
            (Pane::Tree, true) => Pane::Name,
            (Pane::List, true) => Pane::Tree,
            (Pane::Name, true) => Pane::List,
        };
        let input = if state.mode == Mode::Open { state.search_input.clone() } else { state.name_input.clone() };
        if let Some(state) = self.host.dialog.as_mut() {
            state.pane = next;
        }
        if next == Pane::Name {
            // The field takes the keyboard.
            input.update(cx, |input, cx| input.focus(window, cx));
        } else {
            // The tree and the list take it from the fields (Backspace and the arrows are theirs then).
            window.focus(&self.focus_handle, cx);
        }
        cx.notify();
        true
    }

    /// The window's keys are taken before the key bindings of the editor and of the fields (`intercept_keystrokes`: Tab and Backspace are
    /// bound to actions, which would run first), as long as the window is open (the subscription is the window's).
    fn host_fd_key_interceptor(window: &Window, cx: &mut Context<Self>) -> Subscription {
        let editor = cx.weak_entity();
        let handle = window.window_handle();
        cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != handle {
                return;
            }
            let key = event.keystroke.clone();
            let used = editor
                .update(cx, |e, cx| if key.key == "tab" && !key.modifiers.alt && !key.modifiers.control && !key.modifiers.platform { e.host_fd_tab(key.modifiers.shift, window, cx) } else { e.host_fd_key(&key, window, cx) })
                .unwrap_or(false);
            if used {
                window.prevent_default();
                cx.stop_propagation();
            }
        })
    }

    /// The parent of the folder shown (the top has none). True when there was one.
    fn host_fd_parent(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(state) = self.host.dialog.as_ref() else { return false };
        let Some(current) = state.current.clone() else { return false };
        // A root has no parent (the roots are the top).
        let Some(parent) = state.parents.get(&current).cloned().flatten() else { return true };
        self.host_fd_select_folder(Some(parent), window, cx);
        true
    }

    /// The choice of the list moves by `step` files (the list is as the search filter leaves it); the first or last when none is chosen.
    fn host_fd_step_list(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_ref() else { return };
        let search = state.search_input.read(cx).value().to_string().to_lowercase();
        let files: Vec<(usize, &HostFileEntry)> = state
            .list
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| match r {
                ListRowKind::File(f) if search.is_empty() || f.name.to_lowercase().contains(&search) => Some((i, f)),
                _ => None,
            })
            .collect();
        if files.is_empty() {
            return;
        }
        let at = state.selected.as_ref().and_then(|id| files.iter().position(|(_, f)| &f.id == id));
        let to = match at {
            Some(at) => (at as isize + step).clamp(0, files.len() as isize - 1) as usize,
            None if step > 0 => 0,
            None => files.len() - 1,
        };
        let row = files[to].0;
        // The row's place among the rows shown (folders count: they are rows too) is where the list scrolls to.
        let shown_index = state
            .list
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                search.is_empty()
                    || match r {
                        ListRowKind::Folder(f) => f.name.to_lowercase().contains(&search),
                        ListRowKind::File(f) => f.name.to_lowercase().contains(&search),
                    }
            })
            .position(|(i, _)| i == row);
        if let Some(shown_index) = shown_index {
            state.list_table.scroll_to(shown_index);
        }
        self.host_fd_click_row(row, false, window, cx);
    }

    /// The folder shown moves by `step` rows of the tree.
    fn host_fd_step_tree(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_ref() else { return };
        let folders: Vec<Option<String>> = state
            .tree_rows()
            .into_iter()
            .filter_map(|r| match r.kind {
                TreeKind::Folder(id) => Some(Some(id)),
                _ => None,
            })
            .collect();
        if folders.is_empty() {
            return;
        }
        let at = folders.iter().position(|f| *f == state.current).unwrap_or(0);
        let to = (at as isize + step).clamp(0, folders.len() as isize - 1) as usize;
        if to != at {
            state.tree_table.scroll_to(to);
            self.host_fd_select_folder(folders[to].clone(), window, cx);
        }
    }

    /// The file named `name` of the right list is chosen, as by a click (the scenarios use it).
    pub fn host_fd_pick_file(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.host.dialog.as_ref().and_then(|s| s.list.rows.iter().position(|r| matches!(r, ListRowKind::File(f) if f.name == name))) else { return };
        self.host_fd_click_row(index, false, window, cx);
    }

    /// The name typed in the window's file name field (saving; the scenarios use it).
    pub fn host_fd_set_name(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.host.dialog.as_ref().map(|s| s.name_input.clone()) else { return };
        let name = name.to_string();
        input.update(cx, |input, cx| input.set_value(name, window, cx));
    }

    /// A row of the right list was clicked: a file is chosen (a double click does what the window's button does); a folder is entered by
    /// a double click.
    fn host_fd_click_row(&mut self, index: usize, double: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_mut() else { return };
        if state.busy {
            return;
        }
        match state.list.rows.get(index).cloned() {
            Some(ListRowKind::Folder(folder)) => {
                if double {
                    self.host_fd_select_folder(Some(folder.id), window, cx);
                }
            }
            Some(ListRowKind::File(file)) => {
                state.selected = Some(file.id.clone());
                if state.mode != Mode::Open {
                    // The name follows the file chosen (it is the file to write over).
                    let name = file.name.clone();
                    let input = state.name_input.clone();
                    input.update(cx, |input, cx| input.set_value(name, window, cx));
                }
                if double {
                    self.host_fd_confirm(window, cx);
                } else {
                    cx.notify();
                }
            }
            None => {}
        }
    }

    /// 開く / 保存 / 書き出し.
    pub fn host_fd_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_ref() else { return };
        if state.busy {
            return;
        }
        if state.mode == Mode::Open {
            let Some(file_id) = state.selected.clone() else { return };
            let Some(bridge) = host::bridge_with(|c| c.open_file) else { return };
            let id = state.id;
            if let Some(s) = self.host.dialog.as_mut() {
                s.busy = true;
            }
            cx.notify();
            cx.spawn_in(window, async move |this, cx| {
                let result = bridge.open_file(file_id).await;
                this.update_in(cx, |this, window, cx| {
                    if this.host.dialog.as_ref().is_none_or(|s| s.id != id) {
                        return;
                    }
                    match result {
                        Ok(()) => this.close_dialog(window, cx),
                        Err(failure) => {
                            if let Some(s) = this.host.dialog.as_mut() {
                                s.busy = false;
                            }
                            let message = failure.message.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| text("shell.host-file.error-open").to_string());
                            this.show_alert(text("shell.host-file.error-open-title"), message.into(), cx);
                        }
                    }
                })
                .ok();
            })
            .detach();
            return;
        }
        // Saving: the name is checked, the folder must be one that can be written, and a file of the same name is replaced only when agreed.
        let typed = state.name_input.read(cx).value().to_string();
        let name = match state.checked_name(&typed) {
            Ok(name) => name,
            Err(error) => return self.set_status(error.message(), cx),
        };
        let Some(folder_id) = state.current.clone() else { return };
        if !state.folders.get(&folder_id).is_some_and(|f| f.can_write) {
            return self.set_status(text("shell.host-file.error-no-permission").to_string(), cx);
        }
        let (id, purpose) = (state.id, state.mode.purpose());
        // A host that checks the target answers whether the folder can be written and whether the name is taken, paging or not (K0041).
        if let Some(bridge) = host::bridge_with(|c| c.check_save_target) {
            if let Some(s) = self.host.dialog.as_mut() {
                s.busy = true;
            }
            cx.notify();
            cx.spawn_in(window, async move |this, cx| {
                let result = bridge.check_save_target(folder_id.clone(), name.clone(), purpose).await;
                this.update_in(cx, |this, window, cx| {
                    if this.host.dialog.as_ref().is_none_or(|s| s.id != id) {
                        return;
                    }
                    if let Some(s) = this.host.dialog.as_mut() {
                        s.busy = false;
                    }
                    match result {
                        Ok(target) if !target.can_write => this.set_status(text("shell.host-file.error-no-permission").to_string(), cx),
                        Ok(target) => this.host_fd_save_or_ask(folder_id, name, target.existing, window, cx),
                        Err(failure) => {
                            let message = failure.message.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| text("shell.host-file.error-list").to_string());
                            this.show_alert(text("shell.host.error-title"), message.into(), cx);
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return;
        }
        // Without it, the list held decides; with pages still to come, the rest of them are read first (the same name can be in one of them).
        let same = state.list.rows.iter().find_map(|r| match r {
            ListRowKind::File(f) if f.name == name => Some(f.id.clone()),
            _ => None,
        });
        let more = state.list.next_cursor.clone();
        let extension = state.extension.clone();
        if same.is_none()
            && let Some(cursor) = more
            && let Some(bridge) = host::bridge_with(|c| c.list_folder)
        {
            if let Some(s) = self.host.dialog.as_mut() {
                s.busy = true;
            }
            cx.notify();
            cx.spawn_in(window, async move |this, cx| {
                let mut found: Result<Option<String>, HostFailure> = Ok(None);
                let mut next = Some(cursor);
                let mut pages = 0;
                while let Some(cursor) = next.take() {
                    pages += 1;
                    let request = ListRequest { folder_id: Some(folder_id.clone()), purpose, include_folders: false, include_files: true, extensions: vec![extension.clone()], cursor: Some(cursor), limit: PAGE };
                    match bridge.list_folder(request).await {
                        Ok(listing) => {
                            if let Some(file) = listing.files.iter().find(|f| f.name == name) {
                                found = Ok(Some(file.id.clone()));
                                break;
                            }
                            // A host that never ends its pages does not hold the window for ever.
                            next = listing.next_cursor.filter(|_| pages < MAX_CHECK_PAGES);
                        }
                        Err(failure) => {
                            found = Err(failure);
                            break;
                        }
                    }
                }
                this.update_in(cx, |this, window, cx| {
                    if this.host.dialog.as_ref().is_none_or(|s| s.id != id) {
                        return;
                    }
                    if let Some(s) = this.host.dialog.as_mut() {
                        s.busy = false;
                    }
                    match found {
                        Ok(existing) => this.host_fd_save_or_ask(folder_id, name, existing, window, cx),
                        Err(failure) => {
                            let message = failure.message.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| text("shell.host-file.error-list").to_string());
                            this.show_alert(text("shell.host.error-title"), message.into(), cx);
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return;
        }
        self.host_fd_save_or_ask(folder_id, name, same, window, cx);
    }

    /// The file of the same name (`existing`) is written over only when the user agrees; with none, it is saved.
    fn host_fd_save_or_ask(&mut self, folder_id: String, name: String, existing: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        match existing {
            Some(file_id) => {
                let message = format("shell.host-file.overwrite", &[("name", &name)]);
                self.ask(
                    text("shell.host-file.overwrite-title"),
                    message.into(),
                    std::rc::Rc::new(move |_, cx| {
                        let (name, folder_id, file_id) = (name.clone(), folder_id.clone(), file_id.clone());
                        // Run after this update is over (the message box's OK has no window of its own).
                        cx.defer(move |app| {
                            crate::host_editor::with_editor(app, move |editor, window, cx| editor.host_fd_save(folder_id, name, Some(file_id), window, cx));
                        });
                    }),
                    cx,
                );
            }
            None => self.host_fd_save(folder_id, name, None, window, cx),
        }
    }

    fn host_fd_save(&mut self, folder_id: String, name: String, overwrite: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_ref() else { return };
        let (kind, id) = (state.kind, state.id);
        let note = state.export.as_ref().and_then(|job| job.note.clone());
        let bytes = match state.export.as_ref() {
            Some(job) => Ok(job.bytes.clone()),
            None => self.host_bytes_named(Some(&name), window, cx),
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(error) => return self.set_status(format("file.save-failed", &[("error", &error)]), cx),
        };
        if let Some(s) = self.host.dialog.as_mut() {
            s.busy = true;
        }
        cx.notify();
        let folder = folder_id.clone();
        self.host_save_as(
            kind,
            folder_id,
            name,
            bytes,
            overwrite,
            window,
            cx,
            Box::new(move |this, result, window, cx| {
                if this.host.dialog.as_ref().is_none_or(|s| s.id != id) {
                    return;
                }
                match result {
                    Ok(_) => {
                        if kind == SaveKind::SaveAs
                            && let Some(binding) = &mut this.host.binding
                        {
                            binding.folder_id = Some(folder);
                        }
                        let then = this.host.dialog.as_mut().and_then(|s| s.then.take());
                        this.close_dialog(window, cx);
                        if kind == SaveKind::Export
                            && let Some(note) = &note
                        {
                            this.set_status(note.clone(), cx);
                        }
                        if let Some(then) = then {
                            then(this, window, cx);
                        }
                    }
                    Err(failure) => {
                        // The window stays: the name or the folder can be changed and tried again.
                        if let Some(s) = this.host.dialog.as_mut() {
                            s.busy = false;
                        }
                        let message = failure.message.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| text("shell.host.error-save-as").to_string());
                        this.show_alert(text("shell.host.error-title"), message.into(), cx);
                    }
                }
            }),
        );
    }

    /// ローカルストレージから開く: the OS's picker (the browser's `<input type=file>`), started by this click; the file is not the host's.
    fn host_fd_open_local(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_dialog(window, cx);
        self.open_file_local(window, cx);
    }

    /// ローカルに保存（ダウンロード）: as the browser saves without a host.
    fn host_fd_download(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.host.dialog.as_ref() else { return };
        let typed = state.name_input.read(cx).value().to_string();
        let name = match state.checked_name(&typed) {
            Ok(name) => name,
            Err(error) => return self.set_status(error.message(), cx),
        };
        let bytes = match state.export.as_ref() {
            Some(job) => Ok(job.bytes.clone()),
            None => self.host_bytes_named(Some(&name), window, cx),
        };
        match bytes {
            Ok(bytes) => {
                #[cfg(target_family = "wasm")]
                crate::web::download_bytes(&name, "application/octet-stream", &bytes);
                #[cfg(not(target_family = "wasm"))]
                let _ = (&name, &bytes);
                self.close_dialog(window, cx);
            }
            Err(error) => self.set_status(format("file.save-failed", &[("error", &error)]), cx),
        }
    }

    /// The window's panel (a modal dialog), or `None` when it is not open.
    pub(crate) fn host_file_dialog_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        // The list was scrolled to near its end: the next page is asked for.
        let more = {
            let state = self.host.dialog.as_ref()?;
            let visible = state.list_table.window(ROW_HEIGHT, state.list.rows.len());
            state.list.next_cursor.is_some() && !state.list.loading && !state.list.rows.is_empty() && visible.end + 3 >= state.list.rows.len()
        };
        if more {
            self.host_fd_more_list(window, cx);
        }
        let state = self.host.dialog.as_ref()?;
        let editor = cx.weak_entity();
        // The tree's width follows the dialog's width (the list keeps its room when the window gets narrower).
        let split = fit_split(state.split, self.dialog.bounds.map_or(super::host_file_dialog::DIALOG_WIDTH, |b| f32::from(b.size.width)));
        let mode = state.mode;
        let busy = state.busy;
        let heading = |key: &'static str| div().h_full().flex().items_center().text_xs().font_weight(FontWeight::MEDIUM).text_color(crate::chrome::text_secondary()).child(text(key)).into_any_element();

        // ---- the tree ----
        let tree_rows = state.tree_rows();
        let tree_count = tree_rows.len();
        let tree_state = state.tree_table.clone();
        let current = state.current.clone();
        let tree_build = {
            let editor = editor.clone();
            let folders = state.folders.clone();
            let expanded = state.expanded.clone();
            // The folders whose sub folders were read and there are none: no arrow, whatever the host said (`hasSubFolders`; K0046).
            let childless: HashSet<String> = state
                .nodes
                .iter()
                .filter_map(|(id, n)| id.as_ref().filter(|_| n.loaded && n.children.is_empty() && n.error.is_none() && n.next_cursor.is_none() && n.deferred_cursor.is_none()).cloned())
                .collect();
            let rows: Vec<(usize, TreeKind)> = tree_rows.into_iter().map(|r| (r.depth, r.kind)).collect();
            move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                range
                    .filter_map(|i| rows.get(i).map(|r| (i, r)))
                    .map(|(i, (depth, kind))| {
                        let indent = px(*depth as f32 * 14.);
                        // (label, the folder it stands for, chosen, arrow state, a note rather than a folder)
                        let (label, folder, selected, arrow, note): (SharedString, Option<Option<String>>, bool, Option<bool>, bool) = match kind {
                            TreeKind::Folder(id) => {
                                let info = folders.get(id);
                                let has_sub = info.is_none_or(|f| f.has_sub_folders) && !childless.contains(id);
                                (info.map(|f| f.name.clone()).unwrap_or_default().into(), Some(Some(id.clone())), current.as_deref() == Some(id.as_str()), has_sub.then(|| expanded.contains(&Some(id.clone()))), false)
                            }
                            TreeKind::More(_) => (text("shell.host-file.more"), None, false, None, true),
                            TreeKind::Deferred(_) => (text("shell.host-file.deferred"), None, false, None, true),
                            TreeKind::DeferredLoading => (text("shell.host-file.deferred-loading"), None, false, None, true),
                            TreeKind::Loading => (text("shell.host-file.loading"), None, false, None, true),
                            TreeKind::Error(message) => (message.clone().into(), None, false, None, true),
                        };
                        let chevron = match arrow {
                            Some(open) => Icon::new(Material(if open { "expand_more" } else { "chevron_right" })).size(px(16.)).into_any_element(),
                            None => div().w(px(16.)).into_any_element(),
                        };
                        let toggle = {
                            let (editor, folder) = (editor.clone(), folder.clone());
                            div().id(SharedString::from(format!("host-fd-arrow-{i}"))).cursor_pointer().rounded(px(3.)).hover(|d| d.bg(rgb(0xe1e4e8))).child(chevron).on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                if let Some(folder) = folder.clone() {
                                    editor.update(cx, |e, cx| e.host_fd_toggle(folder, window, cx)).ok();
                                }
                            })
                        };
                        let click_editor = editor.clone();
                        let more = match kind {
                            TreeKind::More(parent) => Some((parent.clone(), false)),
                            TreeKind::Deferred(parent) => Some((parent.clone(), true)),
                            _ => None,
                        };
                        // The icon of a root is by its kind; the other folders are folders.
                        let icon = match kind {
                            TreeKind::Folder(id) => folders.get(id).and_then(|f| f.root_kind).map_or("folder_open", |k| k.icon()),
                            _ => "folder_open",
                        };
                        // The arrow (open or close only) and the rest of the row (this folder becomes the current one, and nothing is
                        // opened) are two targets, each highlighted under the pointer on its own (K0046).
                        let rest = div()
                            .id(SharedString::from(format!("host-fd-row-{i}")))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .items_center()
                            .gap_1()
                            .rounded(px(3.))
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(0xf1f3f4)))
                            .when(folder.is_some(), |d| d.child(Icon::new(Material(icon)).size(px(16.))))
                            .child(div().truncate().text_sm().when(note, |d| d.italic().text_color(crate::chrome::text_secondary())).child(label))
                            .on_click(move |_, window, cx| {
                                if let Some((parent, deferred)) = more.clone() {
                                    click_editor.update(cx, |e, cx| if deferred { e.host_fd_deferred_children(parent, window, cx) } else { e.host_fd_more_children(parent, window, cx) }).ok();
                                } else if let Some(folder) = folder.clone() {
                                    click_editor.update(cx, |e, cx| e.host_fd_select_current(folder, window, cx)).ok();
                                }
                            });
                        let cell = h_flex().w_full().h_full().items_center().gap_1().pl(indent).child(toggle).child(rest);
                        TableRow::new(vec![cell.into_any_element()]).selected(selected).parts_hit()
                    })
                    .collect()
            }
        };
        let tree = DataTable::virtualized("host-fd-tree", vec![TableColumn::flex("name")], vec![heading("shell.host-file.col.folders")], tree_state, tree_count, tree_build, cx)
            .row_height(ROW_HEIGHT);

        // ---- the list ----
        let search = state.search_input.read(cx).value().to_string().to_lowercase();
        let shown: Vec<(usize, ListRowKind)> = state
            .list
            .rows
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_, r)| {
                search.is_empty()
                    || match r {
                        ListRowKind::Folder(f) => f.name.to_lowercase().contains(&search),
                        ListRowKind::File(f) => f.name.to_lowercase().contains(&search),
                    }
            })
            .collect();
        let selected_file = state.selected.clone();
        let list_state = state.list_table.clone();
        let list_columns = vec![TableColumn::flex("name"), TableColumn::fixed("updated", 130.), TableColumn::fixed("size", 70.)];
        let list_header = vec![heading("shell.host-file.col.name"), heading("shell.host-file.col.updated"), heading("shell.host-file.col.size")];
        // 続きの表示 (or 取得中...) at the end of the list, once the quick pages are all in (K0041).
        let tail = state.list_tail();
        let count = shown.len() + usize::from(tail.is_some());
        let list_build = {
            let editor = editor.clone();
            move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                range
                    .filter_map(|i| {
                        let Some((index, row)) = shown.get(i).cloned() else {
                            // The row after the last: 続きの表示 (pressable) or 取得中....
                            let pressable = tail?;
                            let label = if pressable { text("shell.host-file.deferred") } else { text("shell.host-file.deferred-loading") };
                            let cell = div().w_full().truncate().text_sm().italic().text_color(crate::chrome::text_secondary()).child(label);
                            let editor = editor.clone();
                            return Some(TableRow::new(vec![cell.into_any_element(), div().into_any_element(), div().into_any_element()]).on_click(move |_, window, cx| {
                                if pressable {
                                    editor.update(cx, |e, cx| e.host_fd_deferred_list(window, cx)).ok();
                                }
                            }));
                        };
                        let (icon, name, updated, size, selected) = match &row {
                            ListRowKind::Folder(f) => (f.root_kind.map_or("folder_open", |k| k.icon()), f.name.clone(), String::new(), String::new(), false),
                            ListRowKind::File(f) => ("description", f.name.clone(), short_date(&f.updated_at), size_kb(f.size), selected_file.as_deref() == Some(f.id.as_str())),
                        };
                        let name_cell = h_flex().w_full().items_center().gap_1().child(Icon::new(Material(icon)).size(px(16.))).child(div().truncate().text_sm().child(SharedString::from(name)));
                        let small = |s: String| div().w_full().pr_1().truncate().text_xs().text_color(crate::chrome::text_secondary()).child(SharedString::from(s)).into_any_element();
                        let editor = editor.clone();
                        Some(TableRow::new(vec![name_cell.into_any_element(), small(updated), small(size)]).selected(selected).on_click(move |event, window, cx| {
                            let double = event.click_count() >= 2;
                            editor.update(cx, |e, cx| e.host_fd_click_row(index, double, window, cx)).ok();
                        }))
                    })
                    .collect()
            }
        };
        let empty = div().p_3().text_sm().text_color(crate::chrome::text_secondary()).child(match (&state.list.error, state.list.loading) {
            (Some(error), _) => SharedString::from(error.clone()),
            (None, true) => text("shell.host-file.loading"),
            (None, false) => text("shell.host-file.empty"),
        });
        let list = DataTable::virtualized("host-fd-list", list_columns, list_header, list_state, count, list_build, cx).row_height(ROW_HEIGHT).empty(empty.into_any_element());

        // ---- the address and the search ----
        let mut address = h_flex().flex_1().min_w_0().items_center().gap_1().flex_wrap().text_sm();
        let parts: Vec<(Option<String>, SharedString)> = state.path.iter().map(|p| (Some(p.id.clone()), SharedString::from(p.name.clone()))).collect();
        let last = parts.len().saturating_sub(1);
        for (i, (folder, name)) in parts.into_iter().enumerate() {
            let editor = editor.clone();
            if i > 0 {
                address = address.child(div().text_color(crate::chrome::text_secondary()).child("›"));
            }
            address = address.child(
                div()
                    .id(SharedString::from(format!("host-fd-address-{i}")))
                    .when(i < last, |d| d.cursor_pointer().text_color(rgb(crate::chrome::PRIMARY_BUTTON_BG)).hover(|d| d.underline()))
                    .child(name)
                    .on_click(move |_, window, cx| {
                        editor.update(cx, |e, cx| e.host_fd_select_folder(folder.clone(), window, cx)).ok();
                    }),
            );
        }
        let search_field = div().w(px(200.)).child(crate::chrome::text_field(&state.search_input).full_width());

        // ---- the name ----
        let typed = state.name_input.read(cx).value().to_string();
        let name_check = if mode == Mode::Open { Ok(String::new()) } else { state.checked_name(&typed) };
        let name_error = match (&name_check, typed.trim().is_empty()) {
            (Err(NameError::Empty), true) => None,
            (Err(error), _) => Some(error.message()),
            _ => None,
        };
        let can_write_here = state.current.as_ref().and_then(|id| state.folders.get(id)).is_some_and(|f| f.can_write);
        let type_label = match state.extension.as_str() {
            "docx" => text("shell.host-file.type-docx"),
            "xlsx" => text("shell.host-file.type-xlsx"),
            "pptx" => text("shell.host-file.type-pptx"),
            other => SharedString::from(format!("*.{other}")),
        };
        let body = v_flex()
            .id("host-file-dialog")
            .size_full()
            .gap_2()
            .child(h_flex().w_full().items_center().gap_2().child(address).child(search_field))
            .child(h_flex().w_full().flex_1().min_h_0().items_stretch().child(div().w(px(split)).flex_none().h_full().child(tree)).child(splitter(&editor, cx)).child(div().flex_1().min_w_0().h_full().child(list)))
            .when(mode != Mode::Open, |d| {
                d.child(crate::chrome::text_field_row(text("shell.host-file.file-name"), crate::chrome::transient_text_field(&state.name_input).full_width()))
                    .children(name_error.map(|e| div().text_sm().text_color(rgb(0xc42b1c)).child(SharedString::from(e))))
                    .child(div().flex().items_center().gap_2().child(div().w(crate::chrome::FIELD_WIDTH).text_sm().child(text("shell.host-file.file-type"))).child(div().text_sm().child(type_label)))
            })
            .when(mode == Mode::Open, |d| {
                let name = state
                    .selected
                    .as_ref()
                    .and_then(|id| {
                        state.list.rows.iter().find_map(|r| match r {
                            ListRowKind::File(f) if &f.id == id => Some(f.name.clone()),
                            _ => None,
                        })
                    })
                    .unwrap_or_default();
                d.child(div().flex().items_center().gap_2().child(div().w(crate::chrome::FIELD_WIDTH).text_sm().child(text("shell.host-file.file-name"))).child(div().text_sm().child(SharedString::from(name))))
            });

        // ---- the buttons ----
        let can_confirm = !busy
            && match mode {
                Mode::Open => state.selected.is_some(),
                _ => name_check.is_ok() && can_write_here,
            };
        let local_files = host::options().local_files;
        let primary_label = match (mode, busy) {
            (Mode::Open, false) => text("shell.host-file.open"),
            (Mode::Open, true) => text("shell.host-file.opening"),
            (Mode::Export, false) => text("shell.host-file.export"),
            (_, false) => text("shell.host-file.save"),
            (_, true) => text("shell.host.saving"),
        };
        let mut actions = crate::chrome::action_bar().action(ActionDefinition::new("host-fd-confirm", primary_label, ActionButtonKind::Primary).disabled(!can_confirm).on_click(cx.listener(|e, _, w, cx| e.host_fd_confirm(w, cx))));
        if local_files {
            actions = if mode == Mode::Open {
                actions.action(ActionDefinition::new("host-fd-local", text("shell.host-file.open-local"), ActionButtonKind::Normal).disabled(busy).on_click(cx.listener(|e, _, w, cx| e.host_fd_open_local(w, cx))))
            } else {
                actions.action(ActionDefinition::new("host-fd-download", text("shell.host-file.save-local"), ActionButtonKind::Normal).disabled(busy).on_click(cx.listener(|e, _, w, cx| e.host_fd_download(w, cx))))
            };
        }
        let actions = actions.action(crate::chrome::dialog_cancel_definition(cx).disabled(busy));
        let title = match mode {
            Mode::Open => text("shell.host-file.title-open"),
            Mode::SaveAs if state.kind == SaveKind::Copy => text("shell.host-file.title-copy"),
            Mode::SaveAs => text("shell.host-file.title-save-as"),
            Mode::Export => text("shell.host-file.title-export"),
        };
        Some(SidebarPanel::with_footer(title, body.into_any_element(), actions))
    }
}

/// The splitter between the folder tree and the file list: the pointer drags it to change the tree's width (kept inside the dialog by
/// [`fit_split`]); letting go keeps the width in the app's settings (the localStorage in the browser).
fn splitter(editor: &WeakEntity<SlideEditor>, cx: &mut Context<SlideEditor>) -> impl IntoElement {
    let tracker = editor.clone();
    div()
        .id("host-fd-splitter")
        .relative()
        .w(px(SPLITTER_WIDTH))
        .h_full()
        .flex_none()
        .cursor_col_resize()
        .flex()
        .justify_center()
        .child(div().w(px(1.)).h_full().bg(rgb(crate::chrome::BORDER)))
        .on_mouse_down(MouseButton::Left, cx.listener(|e, event: &MouseDownEvent, _, cx| {
            let width = e.dialog.bounds.map_or(DIALOG_WIDTH, |b| f32::from(b.size.width));
            if let Some(d) = e.host.dialog.as_mut() {
                d.split = fit_split(d.split, width);
                d.split_drag = Some((f32::from(event.position.x), d.split));
            }
            cx.stop_propagation();
        }))
        .child(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    let moving = tracker.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = moving.update(cx, |e, cx| {
                                let width = e.dialog.bounds.map_or(DIALOG_WIDTH, |b| f32::from(b.size.width));
                                if let Some(d) = e.host.dialog.as_mut()
                                    && let Some((x0, w0)) = d.split_drag
                                {
                                    d.split = fit_split(w0 + f32::from(event.position.x) - x0, width);
                                    cx.notify();
                                }
                            });
                        }
                    });
                    let released = tracker.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = released.update(cx, |e, cx| {
                                if let Some(d) = e.host.dialog.as_mut()
                                    && d.split_drag.take().is_some()
                                {
                                    crate::settings::set(SPLIT_KEY, &format!("{}", d.split.round() as i32));
                                    cx.notify();
                                }
                            });
                        }
                    });
                },
            )
            .absolute()
            .size_0(),
        )
}
