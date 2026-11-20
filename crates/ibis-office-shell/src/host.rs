// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The host: the page (or program) that embeds the editor and keeps the files for it (a cloud storage's viewer). The editor does
//! not know any one host. It tells the host what the user wants to do (open, save, rename, list a folder) and the host decides how
//! (its API, its locks, its permissions); the editor hands over bytes, names and versions, and receives bytes, names, versions and
//! the words to show. `www/ibisoffice-host.ts` is the interface as the host sees it (see `publish/docs/web-embedding.md`).
//!
//! `HostBridge` is the host's functions as Rust calls them. Two implementations: `JsHost` (the browser: `www/main.ts`'s
//! `ibisOfficeHostCall`) and `MemoryHost` (folders and files in memory, for the scenario tests of the desktop build: failures and
//! conflicts are put in by the test). Which functions the host gave is `HostCapabilities`; each feature is on when its function
//! is there, and a host that gave none of them (the playground) leaves the editor as it was.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

/// What a host function returns: the browser's are `Promise`s, so the editor's thread is not blocked (one thread; not `Send`).
pub type HostFuture<T> = Pin<Box<dyn Future<Output = T>>>;

/// Why the host refused (`HostErrorReason` of the interface).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HostReason {
    Locked,
    Conflict,
    ReadOnly,
    NoPermission,
    NotFound,
    Exists,
    TooLarge,
    Unauthorized,
    Network,
    Other,
}

impl HostReason {
    pub fn parse(text: &str) -> Self {
        match text {
            "locked" => Self::Locked,
            "conflict" => Self::Conflict,
            "readOnly" => Self::ReadOnly,
            "noPermission" => Self::NoPermission,
            "notFound" => Self::NotFound,
            "exists" => Self::Exists,
            "tooLarge" => Self::TooLarge,
            "unauthorized" => Self::Unauthorized,
            "network" => Self::Network,
            _ => Self::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Locked => "locked",
            Self::Conflict => "conflict",
            Self::ReadOnly => "readOnly",
            Self::NoPermission => "noPermission",
            Self::NotFound => "notFound",
            Self::Exists => "exists",
            Self::TooLarge => "tooLarge",
            Self::Unauthorized => "unauthorized",
            Self::Network => "network",
            Self::Other => "other",
        }
    }
}

/// A host function's refusal (`HostFailure`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HostFailure {
    pub reason: HostReason,
    /// The words for the user (the host chose the language).
    pub message: Option<String>,
    /// Lead the user to 名前を付けて保存 (absent: by the reason, `offers_save_as`).
    pub offer_save_as: Option<bool>,
}

impl HostFailure {
    pub fn new(reason: HostReason, message: impl Into<String>) -> Self {
        Self { reason, message: Some(message.into()), offer_save_as: None }
    }

    /// Whether 名前を付けて保存 is offered next: what the host said, else yes for a file that cannot be written where it is.
    pub fn offers_save_as(&self) -> bool {
        self.offer_save_as.unwrap_or(matches!(self.reason, HostReason::Locked | HostReason::Conflict | HostReason::ReadOnly | HostReason::NotFound | HostReason::NoPermission))
    }
}

/// What kind of a top place a root is (`HostRootKind`). The editor uses it for the icon only, and keeps the order the host gave; what a
/// kind means is the host's (K0041).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RootKind {
    Personal,
    Shared,
    Joined,
    Pinned,
    Other,
}

impl RootKind {
    /// A kind the host named; one this editor does not know is `Other` (a host may add its own kinds).
    pub fn parse(text: &str) -> Self {
        match text {
            "personal" => Self::Personal,
            "shared" => Self::Shared,
            "joined" => Self::Joined,
            "pinned" => Self::Pinned,
            _ => Self::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Shared => "shared",
            Self::Joined => "joined",
            Self::Pinned => "pinned",
            Self::Other => "other",
        }
    }

    /// The icon (a Material name) of the root in the tree.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Personal => "person",
            Self::Shared | Self::Other => "folder_open",
            Self::Joined => "group",
            Self::Pinned => "star",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HostFolder {
    pub id: String,
    pub name: String,
    /// Whether it has sub folders (the tree's expand mark).
    pub has_sub_folders: bool,
    /// Whether a file can be saved in it.
    pub can_write: bool,
    /// Set on the roots (what `listFolder(null)` gives), not on an ordinary folder.
    pub root_kind: Option<RootKind>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HostFolderRef {
    pub id: String,
    pub name: String,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HostFileEntry {
    pub id: String,
    pub name: String,
    pub size: u64,
    /// ISO 8601.
    pub updated_at: String,
    pub updated_by: Option<String>,
}

/// One page of a folder's children.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct HostListing {
    /// The folder seen and its path from the top; `None` for the top.
    pub folder: Option<(HostFolder, Vec<HostFolderRef>)>,
    pub folders: Vec<HostFolder>,
    pub files: Vec<HostFileEntry>,
    /// The value that asks for the next page (not looked into), when there is one.
    pub next_cursor: Option<String>,
    /// The value of a continuation that takes the host long to make: the editor does not use it by itself but shows 続きの表示, and
    /// asks with it (the same folder, this cursor) only when the user presses it (K0041).
    pub deferred_cursor: Option<String>,
}

/// What `checkSaveTarget` found: whether the folder can be written, and the file of the same name there, if any.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SaveTarget {
    pub can_write: bool,
    /// The id of the file of that name.
    pub existing: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    Open,
    SaveAs,
    Export,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::SaveAs => "saveAs",
            Self::Export => "export",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ListRequest {
    /// `None`: the top.
    pub folder_id: Option<String>,
    pub purpose: Purpose,
    pub include_folders: bool,
    pub include_files: bool,
    /// Lower case, no dot; empty: all files.
    pub extensions: Vec<String>,
    pub cursor: Option<String>,
    pub limit: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveTrigger {
    User,
    Auto,
    Close,
}

impl SaveTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Auto => "auto",
            Self::Close => "close",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveKind {
    SaveAs,
    Export,
    Copy,
}

impl SaveKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SaveAs => "saveAs",
            Self::Export => "export",
            Self::Copy => "copy",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SaveAsRequest {
    pub kind: SaveKind,
    pub folder_id: String,
    pub name: String,
    pub bytes: Vec<u8>,
    /// Save over this file.
    pub overwrite_file_id: Option<String>,
}

/// The file `saveFileAs` made (or wrote over) and its version.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SavedAs {
    pub file_id: String,
    pub file_name: String,
    pub version: String,
}

/// The new name and version after a rename.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Renamed {
    pub name: String,
    pub version: String,
}

/// What replaced the document (`documentChanged`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DocumentSource {
    Local,
    Host,
    New,
}

impl DocumentSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Host => "host",
            Self::New => "new",
        }
    }
}

/// A notice to the host (no answer).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum HostEvent {
    Loaded { name: String, kind: &'static str },
    Dirty(bool),
    DocumentChanged(DocumentSource),
    Saved(SaveTrigger),
    CloseRequested,
    Error(String),
}

/// One of the files opened lately, as the host keeps them (`listRecentFiles`, `addRecentFile`): the name shown and the address that opens it (the host opens it, in a new tab, through `openUrl`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RecentFile {
    pub name: String,
    pub url: String,
}

/// The most recent files kept and shown (ファイル > 最近使ったファイル).
pub const RECENT_FILES_MAX: usize = 20;

/// The host's functions. A function the host did not give returns `HostReason::Other` (the editor checks `HostCapabilities` first).
pub trait HostBridge {
    fn list_folder(&self, request: ListRequest) -> HostFuture<Result<HostListing, HostFailure>>;
    /// Before 名前を付けて保存 / 書き出し: whether `name` can be written in the folder and whether a file of that name is there (a host
    /// whose list is paged answers without the list; optional, see `HostCapabilities::check_save_target`).
    fn check_save_target(&self, folder_id: String, name: String, purpose: Purpose) -> HostFuture<Result<SaveTarget, HostFailure>>;
    fn open_file(&self, file_id: String) -> HostFuture<Result<(), HostFailure>>;
    /// Save the open file's contents; the new version.
    fn save_file(&self, bytes: Vec<u8>, base_version: String, trigger: SaveTrigger) -> HostFuture<Result<String, HostFailure>>;
    fn save_file_as(&self, request: SaveAsRequest) -> HostFuture<Result<SavedAs, HostFailure>>;
    fn rename_file(&self, name: String, base_version: String) -> HostFuture<Result<Renamed, HostFailure>>;
    fn get_file_location(&self) -> HostFuture<Result<String, HostFailure>>;
    fn open_url(&self, url: String) -> HostFuture<Result<(), HostFailure>>;
    /// The files opened lately, newest first (the host keeps them: ibisWorks in the browser's localStorage). At most `RECENT_FILES_MAX` are used.
    fn list_recent_files(&self) -> HostFuture<Result<Vec<RecentFile>, HostFailure>>;
    /// A file was opened (or saved under a new name): the host puts it first in the list (the same address is not listed twice).
    fn add_recent_file(&self, file: RecentFile) -> HostFuture<Result<(), HostFailure>>;
    /// Printing, step 1 (`printBegin`): the click that asked for it is still fresh, so the host opens an empty tab now, before the
    /// PDF is made (a tab opened later is stopped by the pop-up blocker). No answer is waited for beyond the call.
    fn print_begin(&self) -> HostFuture<()>;
    /// The PDF could not be made (or was cancelled): the host closes the empty tab (`printCancel`).
    fn print_cancel(&self) -> HostFuture<()>;
    /// Printing, step 2 (`printPdf`): the PDF; the host shows it in the tab it opened.
    fn print_pdf(&self, name: String, bytes: Vec<u8>) -> HostFuture<Result<(), HostFailure>>;
    fn emit(&self, event: HostEvent);
}

/// Which functions the host gave.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct HostCapabilities {
    pub list_folder: bool,
    /// Optional: without it the file window decides from the list it holds (the rest of the pages are read first).
    pub check_save_target: bool,
    pub open_file: bool,
    pub save_file: bool,
    pub save_file_as: bool,
    pub rename_file: bool,
    pub get_file_location: bool,
    pub open_url: bool,
    pub print_pdf: bool,
    /// `listRecentFiles` (the menu's list of recent files; with `openUrl` to open one).
    pub list_recent_files: bool,
    pub add_recent_file: bool,
}

impl HostCapabilities {
    /// All of them (a host that implements everything; the tests').
    pub fn all() -> Self {
        Self { list_folder: true, check_save_target: true, open_file: true, save_file: true, save_file_as: true, rename_file: true, get_file_location: true, open_url: true, print_pdf: true, list_recent_files: true, add_recent_file: true }
    }

    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut caps = Self::default();
        for name in names {
            match name {
                "listFolder" => caps.list_folder = true,
                "checkSaveTarget" => caps.check_save_target = true,
                "openFile" => caps.open_file = true,
                "saveFile" => caps.save_file = true,
                "saveFileAs" => caps.save_file_as = true,
                "renameFile" => caps.rename_file = true,
                "getFileLocation" => caps.get_file_location = true,
                "openUrl" => caps.open_url = true,
                "printPdf" => caps.print_pdf = true,
                "listRecentFiles" => caps.list_recent_files = true,
                "addRecentFile" => caps.add_recent_file = true,
                _ => {}
            }
        }
        caps
    }
}

/// The options of the host that the editor itself looks at (`IbisOfficeOptions`; the rest is used by the loader or by a later feature).
#[derive(Clone, PartialEq, Debug)]
pub struct HostOptions {
    /// Show 開く from local storage and 保存 as a download (`features.localFiles`; default on).
    pub local_files: bool,
    /// `features.export`: `None` all formats; `Some(vec![])` none; `Some(list)` those.
    pub export: Option<Vec<String>>,
    /// `features.openLinks` (default on).
    pub open_links: bool,
    /// Where 「ソースコード (AGPL)…」 goes (the host's modified source), if it gave one.
    pub source_url: Option<String>,
    /// `startUrl`: the host's start view (a page that runs ibisOffice with no document). The logo of the menu bar opens it in a new tab
    /// (through the host's `openUrl`); without it the logo does nothing (K0037).
    pub start_url: Option<String>,
    /// `ui.startWindow` (default on): the start view (スタート画面) a window shows while it has no document. Off (ibisWorks): the window is blank until the host's `load`, and the
    /// start window menu items are not shown (K0037).
    pub start_window: bool,
    /// `resources.listUrl`: the address of `resource-list.txt` (the sounds of the slide transitions, fetched on demand), absolute (the loader made it so); `None`: the public repository's.
    pub resource_list_url: Option<String>,
    /// `resources.fontListUrl`: the address of `font-list.txt` (the fonts セットアップ offers, and the PDF's fallback font), absolute (the loader made it so); `None`: the public repository's.
    pub font_list_url: Option<String>,
    /// The name of the user (the author of comments and tracked changes), if given.
    pub user_name: Option<String>,
    /// `theme`: `Some(true)` dark, `Some(false)` light, `None` the system's (`auto`, or not given).
    pub dark: Option<bool>,
    /// `features.fontDownload` (default on): the font setup (セットアップ) that downloads fonts the host did not give.
    pub font_download: bool,
    /// `watermark`: text over the document on the screen (not saved in the file).
    pub watermark: Option<Watermark>,
}

/// The host's watermark.
#[derive(Clone, PartialEq, Debug)]
pub struct Watermark {
    pub text: String,
    /// `0..=1`.
    pub opacity: f32,
    /// Whether the printed PDF has it too (the PDF the browser makes for printing; the default is yes).
    pub include_in_print: bool,
}

impl Watermark {
    /// The default opacity of the interface.
    pub const DEFAULT_OPACITY: f32 = 0.12;
    /// The longest text that is drawn (a watermark is a word or two).
    pub const MAX_CHARS: usize = 100;
}

impl Default for HostOptions {
    fn default() -> Self {
        Self { local_files: true, export: None, open_links: true, source_url: None, start_url: None, start_window: true, resource_list_url: None, font_list_url: None, user_name: None, dark: None, font_download: true, watermark: None }
    }
}

impl HostOptions {
    /// Whether exporting in `format` (`pdf`, `html`, ...) is allowed.
    pub fn export_allowed(&self, format: &str) -> bool {
        self.export.as_ref().is_none_or(|list| list.iter().any(|f| f.eq_ignore_ascii_case(format)))
    }
}

struct Connected {
    bridge: Rc<dyn HostBridge>,
    caps: HostCapabilities,
    options: HostOptions,
}

thread_local! {
    static HOST: RefCell<Option<Rc<Connected>>> = const { RefCell::new(None) };
}

/// A host connected (the browser: before the editor starts, `set_host_mode`; the tests: a `MemoryHost`).
pub fn install(bridge: Rc<dyn HostBridge>, caps: HostCapabilities, options: HostOptions) {
    if options.dark.is_some() {
        crate::palette::set_host_theme(options.dark);
    }
    HOST.with(|h| *h.borrow_mut() = Some(Rc::new(Connected { bridge, caps, options })));
}

/// The host is gone (the tests').
pub fn uninstall() {
    if connected().is_some_and(|c| c.options.dark.is_some()) {
        crate::palette::set_host_theme(None);
    }
    HOST.with(|h| *h.borrow_mut() = None);
    set_recent_files(Vec::new());
}

fn connected() -> Option<Rc<Connected>> {
    HOST.with(|h| h.borrow().clone())
}

/// Whether a host is connected (the page was started with `?host=1` and the host connected).
pub fn is_connected() -> bool {
    HOST.with(|h| h.borrow().is_some())
}

/// The functions the host gave (none when no host is connected).
pub fn capabilities() -> HostCapabilities {
    connected().map(|c| c.caps).unwrap_or_default()
}

thread_local! {
    static RECENT: RefCell<Vec<RecentFile>> = const { RefCell::new(Vec::new()) };
}

/// The recent files the host gave last (`refresh_recent_files`), newest first.
pub fn recent_files() -> Vec<RecentFile> {
    RECENT.with(|r| r.borrow().clone())
}

pub fn set_recent_files(files: Vec<RecentFile>) {
    RECENT.with(|r| *r.borrow_mut() = files.into_iter().take(RECENT_FILES_MAX).collect());
}

/// The host's options (the defaults when no host is connected).
pub fn options() -> HostOptions {
    connected().map(|c| c.options.clone()).unwrap_or_default()
}

/// The host, if there is one and it gave `has` (`|caps| caps.save_file`).
pub fn bridge_with(has: impl Fn(&HostCapabilities) -> bool) -> Option<Rc<dyn HostBridge>> {
    connected().filter(|c| has(&c.caps)).map(|c| c.bridge.clone())
}

/// The watermark to draw over the document now, if the host asked for one.
pub fn watermark() -> Option<Watermark> {
    connected().and_then(|c| c.options.watermark.clone())
}

/// Whether the host lets the file be exported as `format` (`features.export`; always with no host).
pub fn export_allowed(format: &str) -> bool {
    !is_connected() || options().export_allowed(format)
}

/// Whether printing is there: with no host, or with a host that gave `printPdf` (the bridge gives its own unless `features.print` is off).
pub fn print_allowed() -> bool {
    !is_connected() || capabilities().print_pdf
}

/// Where 「ソースコード (AGPL)…」 goes: the host's `sourceUrl` (the source of the modified build it serves; AGPL section 13), else the
/// project's page.
pub fn source_url() -> String {
    options().source_url.unwrap_or_else(|| crate::about::PROJECT_URL.to_string())
}

/// Tell the host (no answer). Nothing happens without a host.
pub fn emit(event: HostEvent) {
    if let Some(c) = connected() {
        c.bridge.emit(event);
    }
}

/// Whether `url` may be given to the host's `openUrl`: https, http or mailto (the host checks again, and its check is the one that
/// counts), nothing with a blank or control character.
pub fn is_allowed_url(url: &str) -> bool {
    if url.is_empty() || url.len() > 8192 || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    let Some((scheme, rest)) = url.split_once(':') else { return false };
    let scheme = scheme.to_ascii_lowercase();
    match scheme.as_str() {
        "https" | "http" => rest.starts_with("//") && rest.len() > 2,
        "mailto" => !rest.is_empty(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// MemoryHost
// ---------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct MemFolder {
    name: String,
    parent: Option<String>,
    can_write: bool,
    /// The kind of a root (a folder with no parent).
    root_kind: Option<RootKind>,
    /// A folder the host is slow to find: it is in the list of its parent only after the continuation behind `deferredCursor`.
    late: bool,
}

#[derive(Clone, Debug)]
struct MemFile {
    name: String,
    folder: Option<String>,
    bytes: Vec<u8>,
    version: u32,
}

#[derive(Default)]
struct MemoryState {
    folders: Vec<(String, MemFolder)>,
    files: Vec<(String, MemFile)>,
    next_id: u32,
    /// The file the editor has open (what `saveFile` and `renameFile` act on).
    current: Option<String>,
    /// A failure to give to the next call of a function, by its name (`"saveFile"`, ...).
    fail_next: HashMap<&'static str, HostFailure>,
    calls: Vec<String>,
    printed: Vec<(String, Vec<u8>)>,
    events: Vec<HostEvent>,
    opened_urls: Vec<String>,
    recent: Vec<RecentFile>,
    opened: Vec<String>,
    /// Folders whose list always has a `deferredCursor`, though nothing is behind it (a host that finds nothing more).
    deferred_empty: HashSet<String>,
}

/// A host with its folders and files in memory (the scenario tests'; also what a test of the file window lists). The test builds the
/// folders and files, names the open file (`set_current`), and puts failures in (`fail_next`); every call and event is recorded.
#[derive(Clone, Default)]
pub struct MemoryHost {
    state: Rc<RefCell<MemoryState>>,
}

impl MemoryHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// A folder in `parent` (`None`: the top); returns its id.
    pub fn add_folder(&self, parent: Option<&str>, name: &str, can_write: bool) -> String {
        let mut s = self.state.borrow_mut();
        s.next_id += 1;
        let id = format!("d{}", s.next_id);
        s.folders.push((id.clone(), MemFolder { name: name.to_string(), parent: parent.map(str::to_string), can_write, root_kind: None, late: false }));
        id
    }

    /// A root of a kind (a folder with no parent, in the list of the top); returns its id.
    pub fn add_root(&self, name: &str, kind: RootKind, can_write: bool) -> String {
        let id = self.add_folder(None, name, can_write);
        if let Some((_, f)) = self.state.borrow_mut().folders.iter_mut().find(|(fid, _)| *fid == id) {
            f.root_kind = Some(kind);
        }
        id
    }

    /// A folder that comes only after the continuation (`deferredCursor`) of `parent`'s list, as the joined folders of a host that is
    /// slow to find them; returns its id.
    pub fn add_late_folder(&self, parent: &str, name: &str, can_write: bool) -> String {
        let id = self.add_folder(Some(parent), name, can_write);
        if let Some((_, f)) = self.state.borrow_mut().folders.iter_mut().find(|(fid, _)| *fid == id) {
            f.late = true;
        }
        id
    }

    /// A file in `folder` (`None`: the top; a host's top holds folders only, but the test may put one); returns its id.
    pub fn add_file(&self, folder: Option<&str>, name: &str, bytes: &[u8]) -> String {
        let mut s = self.state.borrow_mut();
        s.next_id += 1;
        let id = format!("f{}", s.next_id);
        s.files.push((id.clone(), MemFile { name: name.to_string(), folder: folder.map(str::to_string), bytes: bytes.to_vec(), version: 1 }));
        id
    }

    /// The list of `folder` gives a `deferredCursor` whatever it holds, and what is behind it is nothing.
    pub fn defer_nothing(&self, folder: &str) {
        self.state.borrow_mut().deferred_empty.insert(folder.to_string());
    }

    pub fn set_current(&self, file_id: Option<&str>) {
        self.state.borrow_mut().current = file_id.map(str::to_string);
    }

    /// The next call of `function` fails with `failure`.
    pub fn fail_next(&self, function: &'static str, failure: HostFailure) {
        self.state.borrow_mut().fail_next.insert(function, failure);
    }

    /// The version a file has now.
    pub fn version_of(&self, file_id: &str) -> Option<String> {
        self.state.borrow().files.iter().find(|(id, _)| id == file_id).map(|(_, f)| format!("v{}", f.version))
    }

    /// Somebody else saves the file (its version moves on: the editor's next save is a conflict).
    pub fn touch(&self, file_id: &str) {
        if let Some((_, f)) = self.state.borrow_mut().files.iter_mut().find(|(id, _)| id == file_id) {
            f.version += 1;
        }
    }

    pub fn bytes_of(&self, file_id: &str) -> Option<Vec<u8>> {
        self.state.borrow().files.iter().find(|(id, _)| id == file_id).map(|(_, f)| f.bytes.clone())
    }

    pub fn name_of(&self, file_id: &str) -> Option<String> {
        self.state.borrow().files.iter().find(|(id, _)| id == file_id).map(|(_, f)| f.name.clone())
    }

    /// The id of the file named `name` in `folder`.
    pub fn find_file(&self, folder: Option<&str>, name: &str) -> Option<String> {
        self.state.borrow().files.iter().find(|(_, f)| f.folder.as_deref() == folder && f.name == name).map(|(id, _)| id.clone())
    }

    /// The id of the first folder named `name`.
    pub fn find_folder(&self, name: &str) -> String {
        self.state.borrow().folders.iter().find(|(_, f)| f.name == name).map(|(id, _)| id.clone()).unwrap_or_else(|| panic!("no folder named {name}"))
    }

    pub fn calls(&self) -> Vec<String> {
        self.state.borrow().calls.clone()
    }

    pub fn events(&self) -> Vec<HostEvent> {
        self.state.borrow().events.clone()
    }

    pub fn opened_urls(&self) -> Vec<String> {
        self.state.borrow().opened_urls.clone()
    }

    /// The PDFs `printPdf` got: name and bytes.
    pub fn printed(&self) -> Vec<(String, Vec<u8>)> {
        self.state.borrow().printed.clone()
    }

    /// The ids `openFile` was asked for.
    pub fn opened(&self) -> Vec<String> {
        self.state.borrow().opened.clone()
    }

    fn take_failure(&self, function: &'static str) -> Option<HostFailure> {
        let mut s = self.state.borrow_mut();
        s.calls.push(function.to_string());
        s.fail_next.remove(function)
    }

    fn path_of(&self, id: &str) -> Vec<HostFolderRef> {
        let s = self.state.borrow();
        let mut path = Vec::new();
        let mut at = Some(id.to_string());
        while let Some(current) = at {
            let Some((_, folder)) = s.folders.iter().find(|(fid, _)| *fid == current) else { break };
            path.push(HostFolderRef { id: current.clone(), name: folder.name.clone() });
            at = folder.parent.clone();
        }
        path.reverse();
        path
    }

    /// One page of `request`'s folder: the sub folders, then the files (each by name), cut into pages by the cursor (an offset, as text).
    fn list_now(&self, request: &ListRequest) -> Result<HostListing, HostFailure> {
        let parent = request.folder_id.clone();
        let folder = match &parent {
            Some(id) => Some((self.folder_info(id).ok_or_else(|| HostFailure::new(HostReason::NotFound, "folder not found"))?, self.path_of(id))),
            None => None,
        };
        let mut folders: Vec<(String, String)> = Vec::new();
        let mut files: Vec<(String, MemFile)> = Vec::new();
        {
            let s = self.state.borrow();
            if request.include_folders {
                folders = s.folders.iter().filter(|(_, f)| f.parent == parent).map(|(id, f)| (f.name.clone(), id.clone())).collect();
                // The roots stay in the order they were given (the host's order); the folders below are by name.
                if parent.is_some() {
                    folders.sort();
                }
            }
            if request.include_files {
                files = s.files.iter().filter(|(_, f)| f.folder == parent && (request.extensions.is_empty() || request.extensions.contains(&extension_of(&f.name)))).cloned().collect();
                files.sort_by(|a, b| a.1.name.cmp(&b.1.name));
            }
        }
        // A parent with late folders gives the quick ones first and a `deferredCursor` ("late:0"); with that cursor it gives them all.
        let (cursor_all, offset) = match request.cursor.as_deref() {
            Some(c) => match c.strip_prefix("late:") {
                Some(n) => (true, n.parse().unwrap_or(0)),
                None => (false, c.parse().unwrap_or(0)),
            },
            None => (false, 0),
        };
        let has_late = request.include_folders && {
            let s = self.state.borrow();
            s.folders.iter().any(|(_, f)| f.parent == parent && f.late) || parent.as_ref().is_some_and(|p| s.deferred_empty.contains(p))
        };
        if has_late && !cursor_all {
            let late: HashSet<String> = self.state.borrow().folders.iter().filter(|(_, f)| f.late).map(|(id, _)| id.clone()).collect();
            folders.retain(|(_, id)| !late.contains(id));
        }
        let total = folders.len() + files.len();
        let start: usize = offset.min(total);
        let end = (start + request.limit.max(1) as usize).min(total);
        let mut listing = HostListing { folder, ..HostListing::default() };
        for index in start..end {
            if index < folders.len() {
                listing.folders.push(self.folder_info(&folders[index].1).expect("a folder of the list"));
            } else {
                let (id, file) = &files[index - folders.len()];
                listing.files.push(HostFileEntry { id: id.clone(), name: file.name.clone(), size: file.bytes.len() as u64, updated_at: "2026-11-15T09:30:00+09:00".to_string(), updated_by: None });
            }
        }
        if end < total {
            listing.next_cursor = Some(if cursor_all { format!("late:{end}") } else { end.to_string() });
        }
        if has_late && !cursor_all {
            listing.deferred_cursor = Some("late:0".to_string());
        }
        Ok(listing)
    }

    fn folder_info(&self, id: &str) -> Option<HostFolder> {
        let s = self.state.borrow();
        let (_, f) = s.folders.iter().find(|(fid, _)| fid == id)?;
        Some(HostFolder { id: id.to_string(), name: f.name.clone(), has_sub_folders: s.folders.iter().any(|(_, c)| c.parent.as_deref() == Some(id)), can_write: f.can_write, root_kind: f.root_kind })
    }
}

fn extension_of(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

impl HostBridge for MemoryHost {
    fn list_folder(&self, request: ListRequest) -> HostFuture<Result<HostListing, HostFailure>> {
        let this = self.clone();
        Box::pin(async move {
            if let Some(failure) = this.take_failure("listFolder") {
                return Err(failure);
            }
            this.list_now(&request)
        })
    }

    fn check_save_target(&self, folder_id: String, name: String, _purpose: Purpose) -> HostFuture<Result<SaveTarget, HostFailure>> {
        let this = self.clone();
        Box::pin(async move {
            if let Some(failure) = this.take_failure("checkSaveTarget") {
                return Err(failure);
            }
            let Some(folder) = this.folder_info(&folder_id) else { return Err(HostFailure::new(HostReason::NotFound, "folder not found")) };
            Ok(SaveTarget { can_write: folder.can_write, existing: this.find_file(Some(&folder_id), &name) })
        })
    }

    fn open_file(&self, file_id: String) -> HostFuture<Result<(), HostFailure>> {
        let this = self.clone();
        Box::pin(async move {
            if let Some(failure) = this.take_failure("openFile") {
                return Err(failure);
            }
            this.state.borrow_mut().opened.push(file_id);
            Ok(())
        })
    }

    fn save_file(&self, bytes: Vec<u8>, base_version: String, _trigger: SaveTrigger) -> HostFuture<Result<String, HostFailure>> {
        let this = self.clone();
        Box::pin(async move {
            if let Some(failure) = this.take_failure("saveFile") {
                return Err(failure);
            }
            let mut s = this.state.borrow_mut();
            let Some(current) = s.current.clone() else { return Err(HostFailure::new(HostReason::NotFound, "no open file")) };
            let Some((_, file)) = s.files.iter_mut().find(|(id, _)| *id == current) else { return Err(HostFailure::new(HostReason::NotFound, "file not found")) };
            if format!("v{}", file.version) != base_version {
                return Err(HostFailure::new(HostReason::Conflict, "somebody saved it first"));
            }
            file.version += 1;
            file.bytes = bytes;
            Ok(format!("v{}", file.version))
        })
    }

    fn save_file_as(&self, request: SaveAsRequest) -> HostFuture<Result<SavedAs, HostFailure>> {
        let this = self.clone();
        Box::pin(async move {
            if let Some(failure) = this.take_failure("saveFileAs") {
                return Err(failure);
            }
            let Some(folder) = this.folder_info(&request.folder_id) else { return Err(HostFailure::new(HostReason::NotFound, "folder not found")) };
            if !folder.can_write {
                return Err(HostFailure::new(HostReason::NoPermission, "no permission to write here"));
            }
            let mut s = this.state.borrow_mut();
            let same = s.files.iter().position(|(_, f)| f.folder.as_deref() == Some(request.folder_id.as_str()) && f.name == request.name);
            let id = match (&request.overwrite_file_id, same) {
                (Some(over), _) => {
                    let Some(at) = s.files.iter().position(|(id, _)| id == over) else { return Err(HostFailure::new(HostReason::NotFound, "file not found")) };
                    let (id, file) = &mut s.files[at];
                    file.bytes = request.bytes;
                    file.version += 1;
                    file.name = request.name.clone();
                    id.clone()
                }
                (None, Some(_)) => return Err(HostFailure::new(HostReason::Exists, "a file of that name exists")),
                (None, None) => {
                    s.next_id += 1;
                    let id = format!("f{}", s.next_id);
                    s.files.push((id.clone(), MemFile { name: request.name.clone(), folder: Some(request.folder_id.clone()), bytes: request.bytes, version: 1 }));
                    id
                }
            };
            let version = format!("v{}", s.files.iter().find(|(fid, _)| *fid == id).map(|(_, f)| f.version).unwrap_or(1));
            if request.kind == SaveKind::SaveAs {
                s.current = Some(id.clone());
            }
            Ok(SavedAs { file_id: id, file_name: request.name, version })
        })
    }

    fn rename_file(&self, name: String, base_version: String) -> HostFuture<Result<Renamed, HostFailure>> {
        let this = self.clone();
        Box::pin(async move {
            if let Some(failure) = this.take_failure("renameFile") {
                return Err(failure);
            }
            let mut s = this.state.borrow_mut();
            let Some(current) = s.current.clone() else { return Err(HostFailure::new(HostReason::NotFound, "no open file")) };
            let Some(at) = s.files.iter().position(|(id, _)| *id == current) else { return Err(HostFailure::new(HostReason::NotFound, "file not found")) };
            if format!("v{}", s.files[at].1.version) != base_version {
                return Err(HostFailure::new(HostReason::Conflict, "somebody changed it"));
            }
            let folder = s.files[at].1.folder.clone();
            if s.files.iter().any(|(id, f)| *id != current && f.folder == folder && f.name == name) {
                return Err(HostFailure::new(HostReason::Exists, "a file of that name exists"));
            }
            let file = &mut s.files[at].1;
            file.name = name.clone();
            file.version += 1;
            Ok(Renamed { name, version: format!("v{}", file.version) })
        })
    }

    fn get_file_location(&self) -> HostFuture<Result<String, HostFailure>> {
        let this = self.clone();
        Box::pin(async move {
            if let Some(failure) = this.take_failure("getFileLocation") {
                return Err(failure);
            }
            match this.state.borrow().current.clone() {
                Some(id) => Ok(format!("memory://file/{id}")),
                None => Err(HostFailure::new(HostReason::NotFound, "no open file")),
            }
        })
    }

    fn list_recent_files(&self) -> HostFuture<Result<Vec<RecentFile>, HostFailure>> {
        let this = self.clone();
        Box::pin(async move {
            if let Some(failure) = this.take_failure("listRecentFiles") {
                return Err(failure);
            }
            Ok(this.state.borrow().recent.clone())
        })
    }

    fn add_recent_file(&self, file: RecentFile) -> HostFuture<Result<(), HostFailure>> {
        let this = self.clone();
        Box::pin(async move {
            if let Some(failure) = this.take_failure("addRecentFile") {
                return Err(failure);
            }
            let mut s = this.state.borrow_mut();
            s.recent.retain(|f| f.url != file.url);
            s.recent.insert(0, file);
            s.recent.truncate(RECENT_FILES_MAX);
            Ok(())
        })
    }

    fn open_url(&self, url: String) -> HostFuture<Result<(), HostFailure>> {
        let this = self.clone();
        Box::pin(async move {
            if let Some(failure) = this.take_failure("openUrl") {
                return Err(failure);
            }
            this.state.borrow_mut().opened_urls.push(url);
            Ok(())
        })
    }

    fn print_begin(&self) -> HostFuture<()> {
        let this = self.clone();
        Box::pin(async move {
            let _ = this.take_failure("printBegin");
        })
    }

    fn print_cancel(&self) -> HostFuture<()> {
        let this = self.clone();
        Box::pin(async move {
            let _ = this.take_failure("printCancel");
        })
    }

    fn print_pdf(&self, name: String, bytes: Vec<u8>) -> HostFuture<Result<(), HostFailure>> {
        let this = self.clone();
        Box::pin(async move {
            if let Some(failure) = this.take_failure("printPdf") {
                return Err(failure);
            }
            this.state.borrow_mut().printed.push((name, bytes));
            Ok(())
        })
    }

    fn emit(&self, event: HostEvent) {
        self.state.borrow_mut().events.push(event);
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// JsHost (the browser)
// ---------------------------------------------------------------------------------------------------------------------------

#[cfg(target_family = "wasm")]
mod js_host {
    use super::*;
    use js_sys::{Array, Object, Reflect};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        /// Defined by `www/main.ts`: a function of the host (`method`, its arguments as an object; `bytes`: the file's contents).
        /// Resolves to the host's answer, an object (`{ ok: true, ... }` or a `HostFailure`).
        #[wasm_bindgen(catch, js_name = ibisOfficeHostCall)]
        fn host_call_js(method: &str, params: &JsValue, bytes: Option<Vec<u8>>) -> Result<js_sys::Promise, JsValue>;
        /// Defined by `www/main.ts`: a notice to the host (an object; `bytes` for 'download').
        #[wasm_bindgen(js_name = ibisOfficeHostEvent)]
        fn host_event_js(event: &JsValue, bytes: Option<Vec<u8>>);
    }

    fn set(object: &Object, key: &str, value: impl Into<JsValue>) {
        let _ = Reflect::set(object, &JsValue::from_str(key), &value.into());
    }

    fn get(value: &JsValue, key: &str) -> JsValue {
        Reflect::get(value, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
    }

    fn get_string(value: &JsValue, key: &str) -> Option<String> {
        get(value, key).as_string()
    }

    fn get_bool(value: &JsValue, key: &str) -> Option<bool> {
        get(value, key).as_bool()
    }

    fn describe(error: JsValue) -> String {
        error.as_string().or_else(|| get_string(&error, "message")).unwrap_or_else(|| format!("{error:?}"))
    }

    fn failure_of(answer: &JsValue) -> HostFailure {
        HostFailure {
            reason: HostReason::parse(&get_string(answer, "reason").unwrap_or_default()),
            message: get_string(answer, "message"),
            offer_save_as: get_bool(answer, "offerSaveAs"),
        }
    }

    /// Calls the host's `method` and gives its answer when it is `ok` (or has no `ok`: a listing is its fields), its failure when it is not.
    async fn call(method: &str, params: Object, bytes: Option<Vec<u8>>) -> Result<JsValue, HostFailure> {
        let promise = host_call_js(method, &params, bytes).map_err(|e| HostFailure::new(HostReason::Other, describe(e)))?;
        let answer = wasm_bindgen_futures::JsFuture::from(promise).await.map_err(|e| HostFailure::new(HostReason::Other, describe(e)))?;
        if answer.is_object() && get_bool(&answer, "ok") == Some(false) {
            Err(failure_of(&answer))
        } else {
            Ok(answer)
        }
    }

    fn folder_of(value: &JsValue) -> HostFolder {
        HostFolder {
            id: get_string(value, "id").unwrap_or_default(),
            name: get_string(value, "name").unwrap_or_default(),
            has_sub_folders: get_bool(value, "hasSubFolders").unwrap_or(false),
            can_write: get_bool(value, "canWrite").unwrap_or(false),
            root_kind: get_string(value, "rootKind").map(|k| RootKind::parse(&k)),
        }
    }

    pub struct JsHost;

    impl HostBridge for JsHost {
        fn list_folder(&self, request: ListRequest) -> HostFuture<Result<HostListing, HostFailure>> {
            Box::pin(async move {
                let params = Object::new();
                set(&params, "folderId", request.folder_id.as_deref().map(JsValue::from_str).unwrap_or(JsValue::NULL));
                set(&params, "purpose", request.purpose.as_str());
                let include = Object::new();
                set(&include, "folders", request.include_folders);
                set(&include, "files", request.include_files);
                set(&params, "include", include);
                if !request.extensions.is_empty() {
                    set(&params, "extensions", request.extensions.iter().map(|e| JsValue::from_str(e)).collect::<Array>());
                }
                if let Some(cursor) = &request.cursor {
                    set(&params, "cursor", cursor.as_str());
                }
                set(&params, "limit", request.limit);
                let answer = call("listFolder", params, None).await?;
                let mut listing = HostListing::default();
                let folder = get(&answer, "folder");
                if folder.is_object() {
                    let path = Array::from(&get(&folder, "path")).iter().map(|r| HostFolderRef { id: get_string(&r, "id").unwrap_or_default(), name: get_string(&r, "name").unwrap_or_default() }).collect();
                    listing.folder = Some((folder_of(&folder), path));
                }
                listing.folders = Array::from(&get(&answer, "folders")).iter().map(|f| folder_of(&f)).collect();
                listing.files = Array::from(&get(&answer, "files"))
                    .iter()
                    .map(|f| HostFileEntry {
                        id: get_string(&f, "id").unwrap_or_default(),
                        name: get_string(&f, "name").unwrap_or_default(),
                        size: get(&f, "size").as_f64().unwrap_or(0.0).max(0.0) as u64,
                        updated_at: get_string(&f, "updatedAt").unwrap_or_default(),
                        updated_by: get_string(&f, "updatedBy"),
                    })
                    .collect();
                listing.next_cursor = get_string(&answer, "nextCursor");
                listing.deferred_cursor = get_string(&answer, "deferredCursor");
                Ok(listing)
            })
        }

        fn check_save_target(&self, folder_id: String, name: String, purpose: Purpose) -> HostFuture<Result<SaveTarget, HostFailure>> {
            Box::pin(async move {
                let params = Object::new();
                set(&params, "folderId", folder_id.as_str());
                set(&params, "name", name.as_str());
                set(&params, "purpose", purpose.as_str());
                let answer = call("checkSaveTarget", params, None).await?;
                let existing = get(&answer, "existing");
                Ok(SaveTarget { can_write: get_bool(&answer, "canWrite").unwrap_or(false), existing: if existing.is_object() { get_string(&existing, "id") } else { None } })
            })
        }

        fn open_file(&self, file_id: String) -> HostFuture<Result<(), HostFailure>> {
            Box::pin(async move {
                let params = Object::new();
                set(&params, "fileId", file_id.as_str());
                call("openFile", params, None).await.map(|_| ())
            })
        }

        fn save_file(&self, bytes: Vec<u8>, base_version: String, trigger: SaveTrigger) -> HostFuture<Result<String, HostFailure>> {
            Box::pin(async move {
                let params = Object::new();
                set(&params, "baseVersion", base_version.as_str());
                set(&params, "trigger", trigger.as_str());
                let answer = call("saveFile", params, Some(bytes)).await?;
                Ok(get_string(&answer, "version").unwrap_or_default())
            })
        }

        fn save_file_as(&self, request: SaveAsRequest) -> HostFuture<Result<SavedAs, HostFailure>> {
            Box::pin(async move {
                let params = Object::new();
                set(&params, "kind", request.kind.as_str());
                set(&params, "folderId", request.folder_id.as_str());
                set(&params, "name", request.name.as_str());
                if let Some(over) = &request.overwrite_file_id {
                    set(&params, "overwriteFileId", over.as_str());
                }
                let answer = call("saveFileAs", params, Some(request.bytes)).await?;
                let file = get(&answer, "file");
                Ok(SavedAs { file_id: get_string(&file, "id").unwrap_or_default(), file_name: get_string(&file, "name").unwrap_or(request.name), version: get_string(&answer, "version").unwrap_or_default() })
            })
        }

        fn rename_file(&self, name: String, base_version: String) -> HostFuture<Result<Renamed, HostFailure>> {
            Box::pin(async move {
                let params = Object::new();
                set(&params, "name", name.as_str());
                set(&params, "baseVersion", base_version.as_str());
                let answer = call("renameFile", params, None).await?;
                Ok(Renamed { name: get_string(&answer, "name").unwrap_or(name), version: get_string(&answer, "version").unwrap_or_default() })
            })
        }

        fn get_file_location(&self) -> HostFuture<Result<String, HostFailure>> {
            Box::pin(async move {
                let answer = call("getFileLocation", Object::new(), None).await?;
                get_string(&answer, "url").ok_or_else(|| HostFailure::new(HostReason::Other, "the host gave no location"))
            })
        }

        fn open_url(&self, url: String) -> HostFuture<Result<(), HostFailure>> {
            Box::pin(async move {
                let params = Object::new();
                set(&params, "url", url.as_str());
                call("openUrl", params, None).await.map(|_| ())
            })
        }

        fn list_recent_files(&self) -> HostFuture<Result<Vec<RecentFile>, HostFailure>> {
            Box::pin(async move {
                let answer = call("listRecentFiles", Object::new(), None).await?;
                let files = get(&answer, "files");
                let mut out = Vec::new();
                if Array::is_array(&files) {
                    for item in Array::from(&files).iter().take(RECENT_FILES_MAX) {
                        if let (Some(name), Some(url)) = (get_string(&item, "name"), get_string(&item, "url")) {
                            out.push(RecentFile { name, url });
                        }
                    }
                }
                Ok(out)
            })
        }

        fn add_recent_file(&self, file: RecentFile) -> HostFuture<Result<(), HostFailure>> {
            Box::pin(async move {
                let params = Object::new();
                set(&params, "name", file.name.as_str());
                set(&params, "url", file.url.as_str());
                call("addRecentFile", params, None).await.map(|_| ())
            })
        }

        fn print_begin(&self) -> HostFuture<()> {
            Box::pin(async move {
                let _ = call("printBegin", Object::new(), None).await;
            })
        }

        fn print_cancel(&self) -> HostFuture<()> {
            Box::pin(async move {
                let _ = call("printCancel", Object::new(), None).await;
            })
        }

        fn print_pdf(&self, name: String, bytes: Vec<u8>) -> HostFuture<Result<(), HostFailure>> {
            Box::pin(async move {
                let params = Object::new();
                set(&params, "name", name.as_str());
                call("printPdf", params, Some(bytes)).await.map(|_| ())
            })
        }

        fn emit(&self, event: HostEvent) {
            let object = Object::new();
            match event {
                HostEvent::Loaded { name, kind } => {
                    set(&object, "type", "loaded");
                    set(&object, "name", name.as_str());
                    set(&object, "kind", kind);
                }
                HostEvent::Dirty(dirty) => {
                    set(&object, "type", "dirty");
                    set(&object, "dirty", dirty);
                }
                HostEvent::DocumentChanged(source) => {
                    set(&object, "type", "documentChanged");
                    set(&object, "source", source.as_str());
                }
                HostEvent::Saved(trigger) => {
                    set(&object, "type", "saved");
                    set(&object, "trigger", trigger.as_str());
                }
                HostEvent::CloseRequested => set(&object, "type", "closeRequested"),
                HostEvent::Error(message) => {
                    set(&object, "type", "error");
                    set(&object, "message", message.as_str());
                }
            }
            host_event_js(&object, None);
        }
    }

    /// The options of the host that the editor looks at (a field that is not there is the default).
    fn options_of(options: &JsValue) -> HostOptions {
        let mut result = HostOptions::default();
        let features = get(options, "features");
        if features.is_object() {
            if let Some(local) = get_bool(&features, "localFiles") {
                result.local_files = local;
            }
            if let Some(download) = get_bool(&features, "fontDownload") {
                result.font_download = download;
            }
            if let Some(links) = get_bool(&features, "openLinks") {
                result.open_links = links;
            }
            let export = get(&features, "export");
            if let Some(all) = export.as_bool() {
                result.export = if all { None } else { Some(Vec::new()) };
            } else if Array::is_array(&export) {
                result.export = Some(Array::from(&export).iter().filter_map(|v| v.as_string()).collect());
            }
        }
        let watermark = get(options, "watermark");
        if watermark.is_object() {
            let text: String = get_string(&watermark, "text").unwrap_or_default().chars().take(Watermark::MAX_CHARS).collect();
            if !text.trim().is_empty() {
                let opacity = get(&watermark, "opacity").as_f64().map(|v| v as f32).filter(|v| v.is_finite()).unwrap_or(Watermark::DEFAULT_OPACITY).clamp(0.0, 1.0);
                result.watermark = Some(Watermark { text, opacity, include_in_print: get_bool(&watermark, "includeInPrint").unwrap_or(true) });
            }
        }
        // `autoSaveSeconds` is not read: the editor has no automatic save to the host (every save is the user's, or the close's).
        result.dark = match get_string(options, "theme").as_deref() {
            Some("dark") => Some(true),
            Some("light") => Some(false),
            _ => None,
        };
        let ui = get(options, "ui");
        if ui.is_object() {
            if let Some(start) = get_bool(&ui, "startWindow") {
                result.start_window = start;
            }
        }
        let resources = get(options, "resources");
        if resources.is_object() {
            result.resource_list_url = get_string(&resources, "listUrl").filter(|u| crate::font_setup::safe_url(u));
            result.font_list_url = get_string(&resources, "fontListUrl").filter(|u| crate::font_setup::safe_url(u));
        }
        result.source_url = get_string(options, "sourceUrl").filter(|u| is_allowed_url(u));
        result.start_url = get_string(options, "startUrl").filter(|u| is_allowed_url(u));
        let user = get(options, "user");
        if user.is_object() {
            result.user_name = get_string(&user, "name").map(|n| n.trim().chars().take(255).collect::<String>()).filter(|n| !n.is_empty());
        }
        result
    }

    /// Called by `www/main.ts` before `run`, when a host connected: its options (`IbisOfficeOptions`) and the names of the functions it gave.
    #[wasm_bindgen]
    pub fn set_host_mode(options: JsValue, capabilities: Vec<String>) {
        // The page's storage is the host's: the settings are kept in memory only, and the language is the host's (`locale`) when it gave one.
        ibis_office_base::settings::use_memory_only();
        match get_string(&options, "locale").as_deref() {
            Some("ja") => ibis_office_base::i18n::set_setting(ibis_office_base::i18n::LanguageSetting::Fixed(ibis_office_base::i18n::Language::Japanese)),
            Some("en") => ibis_office_base::i18n::set_setting(ibis_office_base::i18n::LanguageSetting::Fixed(ibis_office_base::i18n::Language::English)),
            _ => {}
        }
        install(Rc::new(JsHost), HostCapabilities::from_names(capabilities.iter().map(String::as_str)), options_of(&options));
    }
}

#[cfg(target_family = "wasm")]
pub use js_host::JsHost;
