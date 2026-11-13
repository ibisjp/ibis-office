// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pictures IMAGE(address) shows in cells: fetched from their address
//! off the drawing thread, kept by address (a few hundred, within a limit of
//! bytes), and decoded when first drawn. The calculation does not reach the
//! network; the sheet's drawing asks here and draws again while a picture is
//! still on its way.
//!
//! Like Excel, only `https` addresses are fetched, and a picture is BMP,
//! JPEG, GIF, TIFF, PNG, ICO or WEBP (found by its first bytes, not by the
//! address). The desktop version fetches with the system's `curl` (as the
//! updater does: no network library is linked); the web version with the
//! browser's fetch (a server that does not allow it by CORS gives nothing).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use gpui_kit::{Image, ImageFormat, RenderImage};

/// The biggest picture taken (Excel's own limit is not published).
const MAX_BYTES: usize = 20 * 1024 * 1024;
/// How many pictures are kept, and how many bytes they take together.
const MAX_ENTRIES: usize = 200;
const MAX_TOTAL: usize = 256 * 1024 * 1024;
/// How many addresses are fetched at once.
#[cfg(not(target_family = "wasm"))]
const MAX_ACTIVE: usize = 4;
/// A failed address is tried again after this long (milliseconds).
const RETRY_MS: u64 = 30_000;

/// Why a picture is not there (the cell shows the error Excel shows for it: see [`Failure::error`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Not fetched: no connection, an address that is not found, an error from the server, curl missing.
    Connect,
    /// The host's name is not found (Excel: #VALUE!, not #CONNECT!).
    Unresolved,
    /// Fetched but not a picture Excel takes, or too big.
    NotImage,
    /// Not an https address (Excel: #BLOCKED!).
    NotHttps,
    /// Not an address at all (decided before any fetch).
    NotUrl,
}

impl Failure {
    /// The error the cell shows.
    pub fn error(self) -> ibis_calc::CellError {
        match self {
            Failure::Connect => ibis_calc::CellError::Connect,
            Failure::NotHttps => ibis_calc::CellError::Blocked,
            Failure::Unresolved | Failure::NotImage | Failure::NotUrl => ibis_calc::CellError::Value,
        }
    }
}

/// How an address stands, without decoding it (for laying out a cell).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Ready,
    Failed(Failure),
}

/// Where an address stands.
#[derive(Clone)]
pub enum Poll {
    /// On its way: draw again soon.
    Pending,
    /// Fetched and decoded.
    Ready(Arc<RenderImage>),
    /// Not fetched, or not a picture Excel takes (the cell is #CONNECT! or #VALUE!).
    Failed,
}

enum Entry {
    Pending,
    Bytes(Arc<Vec<u8>>),
    Failed(u64, Failure),
}

#[derive(Default)]
struct State {
    entries: HashMap<String, Entry>,
    /// The order the addresses were first asked for (the oldest are dropped first).
    order: VecDeque<String>,
    total: usize,
    #[cfg(not(target_family = "wasm"))]
    queue: VecDeque<String>,
    #[cfg(not(target_family = "wasm"))]
    active: usize,
}

fn state() -> MutexGuard<'static, State> {
    table().lock().unwrap_or_else(|e| e.into_inner())
}

fn table() -> &'static Mutex<State> {
    static TABLE: std::sync::OnceLock<Mutex<State>> = std::sync::OnceLock::new();
    TABLE.get_or_init(Default::default)
}

/// Changes each time an address is fetched (or fails): sheets with pictures are laid out again.
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

/// When curl was last found missing (milliseconds; 0: never): the status bar says so for a few seconds.
static NO_CURL_AT: AtomicU64 = AtomicU64::new(0);
const NOTICE_MS: u64 = 6_000;

/// Curl could not be started: the status bar says so for a few seconds.
pub fn note_curl_missing() {
    NO_CURL_AT.store(ibis_office_base::clock::unix_millis().max(1), Ordering::Relaxed);
}

/// Whether the status bar tells that curl is not found.
pub fn curl_missing_notice() -> bool {
    let at = NO_CURL_AT.load(Ordering::Relaxed);
    at != 0 && ibis_office_base::clock::unix_millis().saturating_sub(at) < NOTICE_MS
}

/// Where `url` stands, without starting a fetch (an address not asked for yet is pending).
pub fn status(url: &str) -> Status {
    match state().entries.get(url) {
        Some(Entry::Bytes(_)) => Status::Ready,
        Some(Entry::Failed(at, f)) if ibis_office_base::clock::unix_millis().saturating_sub(*at) < RETRY_MS => Status::Failed(*f),
        _ => Status::Pending,
    }
}

/// Used by the scenarios in place of the network: the bytes of an address, or why the fetch fails.
type Fetcher = Arc<dyn Fn(&str) -> Result<Vec<u8>, Failure> + Send + Sync>;

fn fetcher() -> &'static Mutex<Option<Fetcher>> {
    static FETCHER: Mutex<Option<Fetcher>> = Mutex::new(None);
    &FETCHER
}

/// Fetch through `f` instead of the network (the scenarios' local pictures); `None` puts the network back. Forgets
/// what was fetched.
#[allow(dead_code)]
pub fn set_fetcher(f: Option<Fetcher>) {
    *fetcher().lock().unwrap_or_else(|e| e.into_inner()) = f;
    forget_all();
}

/// Forget every picture (fetched again when drawn).
pub(crate) fn forget_all() {
    let mut s = state();
    let s = &mut *s;
    s.entries.retain(|_, e| matches!(e, Entry::Pending));
    let entries = &s.entries;
    s.order.retain(|k| entries.contains_key(k));
    s.total = 0;
    DECODED.with(|d| d.borrow_mut().clear());
}

thread_local! {
    /// Pictures decoded on the drawing thread, by address (`None`: not a picture).
    static DECODED: std::cell::RefCell<HashMap<String, Option<Arc<RenderImage>>>> = std::cell::RefCell::new(HashMap::new());
}

/// How many addresses are being fetched or waiting to be.
#[allow(dead_code)]
pub fn pending() -> usize {
    state().entries.values().filter(|e| matches!(e, Entry::Pending)).count()
}

/// The picture at `url`: starts fetching it the first time it is asked for.
pub fn poll(url: &str, cx: &gpui_kit::App) -> Poll {
    let now = ibis_office_base::clock::unix_millis();
    let mut s = state();
    match s.entries.get(url) {
        Some(Entry::Pending) => return Poll::Pending,
        Some(Entry::Failed(at, _)) if now.saturating_sub(*at) < RETRY_MS => return Poll::Failed,
        Some(Entry::Bytes(bytes)) => {
            let bytes = bytes.clone();
            drop(s);
            return DECODED.with(|d| {
                let mut d = d.borrow_mut();
                if let Some(done) = d.get(url) {
                    return done.clone().map_or(Poll::Failed, Poll::Ready);
                }
                if d.len() >= 64 {
                    d.clear();
                }
                let image = decode(&bytes, cx);
                d.insert(url.to_string(), image.clone());
                if image.is_none() {
                    // A broken picture: the cell shows the error (laid out again).
                    state().entries.insert(url.to_string(), Entry::Failed(ibis_office_base::clock::unix_millis(), Failure::NotImage));
                    GENERATION.fetch_add(1, Ordering::AcqRel);
                }
                image.map_or(Poll::Failed, Poll::Ready)
            });
        }
        _ => {}
    }
    // New (or a failure to try again).
    if s.entries.insert(url.to_string(), Entry::Pending).is_none() {
        s.order.push_back(url.to_string());
    }
    start(&mut s, url);
    Poll::Pending
}

fn decode(bytes: &[u8], cx: &gpui_kit::App) -> Option<Arc<RenderImage>> {
    let format = sniff(bytes)?;
    Image::from_bytes(format, bytes.to_vec()).to_image_data(cx.svg_renderer()).ok()
}

/// The format of a picture by its first bytes (Excel takes BMP, JPEG, GIF, TIFF, PNG, ICO and WEBP).
pub fn sniff(bytes: &[u8]) -> Option<ImageFormat> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some(ImageFormat::Png),
        [0xFF, 0xD8, 0xFF, ..] => Some(ImageFormat::Jpeg),
        [b'G', b'I', b'F', b'8', ..] => Some(ImageFormat::Gif),
        [b'B', b'M', ..] => Some(ImageFormat::Bmp),
        [b'I', b'I', 42, 0, ..] | [b'M', b'M', 0, 42, ..] => Some(ImageFormat::Tiff),
        [0, 0, 1, 0, ..] => Some(ImageFormat::Ico),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some(ImageFormat::Webp),
        _ => None,
    }
}

/// Keep what was fetched (or that it failed), within the limits.
fn finish(url: &str, result: Result<Vec<u8>, Failure>) {
    let mut s = state();
    let result = result.and_then(|b| if !b.is_empty() && b.len() <= MAX_BYTES && sniff(&b).is_some() { Ok(b) } else { Err(Failure::NotImage) });
    let entry = match result {
        Ok(b) => {
            s.total += b.len();
            Entry::Bytes(Arc::new(b))
        }
        Err(f) => Entry::Failed(ibis_office_base::clock::unix_millis(), f),
    };
    s.entries.insert(url.to_string(), entry);
    // The oldest go when there are too many, or too many bytes (never the ones on their way).
    let mut i = 0;
    while (s.entries.len() > MAX_ENTRIES || s.total > MAX_TOTAL) && i < s.order.len() {
        let key = s.order[i].clone();
        if key != url && matches!(s.entries.get(&key), Some(Entry::Bytes(_) | Entry::Failed(..))) {
            if let Some(Entry::Bytes(b)) = s.entries.remove(&key) {
                s.total = s.total.saturating_sub(b.len());
            } else {
                s.entries.remove(&key);
            }
            s.order.remove(i);
        } else {
            i += 1;
        }
    }
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

#[cfg(not(target_family = "wasm"))]
fn start(s: &mut State, url: &str) {
    if s.active >= MAX_ACTIVE {
        s.queue.push_back(url.to_string());
        return;
    }
    s.active += 1;
    let url = url.to_string();
    std::thread::spawn(move || {
        let mut next = Some(url);
        while let Some(url) = next {
            let result = fetch(&url);
            finish(&url, result);
            let mut s = state();
            next = s.queue.pop_front();
            if next.is_none() {
                s.active -= 1;
            }
        }
    });
}

#[cfg(not(target_family = "wasm"))]
fn fetch(url: &str) -> Result<Vec<u8>, Failure> {
    // Not an address (no scheme): decided before anything is fetched, as Excel does.
    let scheme = url.find("://").map(|i| &url[..i]).filter(|s| !s.is_empty() && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && s.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)));
    if scheme.is_none() {
        return Err(Failure::NotUrl);
    }
    // Excel takes https only.
    if !url.get(..8).is_some_and(|p| p.eq_ignore_ascii_case("https://")) {
        return Err(Failure::NotHttps);
    }
    let custom = fetcher().lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(f) = custom {
        return f(url);
    }
    let mut curl = std::process::Command::new("curl");
    curl.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        curl.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    curl.args(["--fail", "--silent", "--location", "--connect-timeout", "10", "--max-time", "30", "--max-filesize", &MAX_BYTES.to_string()]);
    // Only https, also after a redirect.
    curl.args(["--proto", "=https", "--proto-redir", "=https", "--output", "-", "--"]);
    curl.arg(url);
    let out = match curl.output() {
        Ok(out) => out,
        Err(err) => {
            if err.kind() == std::io::ErrorKind::NotFound {
                note_curl_missing();
            }
            return Err(Failure::Connect);
        }
    };
    if out.status.success() {
        Ok(out.stdout)
    } else if out.status.code() == Some(6) {
        // curl's "could not resolve host".
        Err(Failure::Unresolved)
    } else {
        // 404 (22), cannot connect (7), timeout (28) and the rest.
        Err(Failure::Connect)
    }
}

#[cfg(target_family = "wasm")]
static CLIENT: std::sync::OnceLock<Arc<dyn gpui_kit::http_client::HttpClient>> = std::sync::OnceLock::new();

/// The web version fetches through the browser's client.
#[cfg(target_family = "wasm")]
pub fn set_http_client(client: Arc<dyn gpui_kit::http_client::HttpClient>) {
    let _ = CLIENT.set(client);
}

#[cfg(target_family = "wasm")]
fn start(_: &mut State, url: &str) {
    use gpui_kit::http_client::AsyncBody;
    let Some(client) = CLIENT.get() else {
        // Not given one: nothing can be fetched (finished after this lock is let go).
        let url = url.to_string();
        wasm_bindgen_futures::spawn_local(async move { finish(&url, Err(Failure::Connect)) });
        return;
    };
    let request = client.get(url, AsyncBody::empty(), true);
    let url = url.to_string();
    wasm_bindgen_futures::spawn_local(async move {
        let bytes = async {
            let response = request.await.map_err(|_| Failure::Connect)?;
            if !response.status().is_success() {
                return Err(Failure::Connect);
            }
            let mut bytes = Vec::new();
            futures::AsyncReadExt::read_to_end(&mut response.into_body(), &mut bytes).await.map_err(|_| Failure::Connect)?;
            Ok(bytes)
        }
        .await;
        finish(&url, bytes);
    });
}
