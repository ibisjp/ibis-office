// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! セットアップ (K0001): the fonts ibisOffice does not ship, which the user can download.
//!
//! * **The list** is `docs/font-list.txt` of the public repository (JSON Lines, one font a line; see [`FontEntry`]), read over
//!   the network. The font files themselves are on the repository's GitHub Pages (`fonts/<id>/`, each font in a folder of its
//!   own with its licence file), and the list gives their addresses.
//! * **Where they go**: the app's own data folder (`ibis_office_base::app_data::fonts_dir`, beside the settings), not the
//!   operating system's font folder: no administrator, nothing left behind when the app's data is removed, and the font is not
//!   offered to other apps (a licence may allow use with ibisOffice only). The render crate gives the files in that folder to
//!   the text system at start (`shape_view::downloaded_fonts`), so a document that names the font draws with it. In the
//!   browser they are kept in IndexedDB, in chunks (`www/main.ts`, `crate::web`), and given to the text system the same way.
//! * **Skipped fonts** are kept in the settings, by the font file's name: `font-setup.skip.<file>=1`.
//! * **When it is offered**: at the first start and at the first start of a new version (`settings::migrate_on_startup` forgets
//!   `font-setup.checked-version` then; it is set once the list was read), when the list has a font that is not installed and
//!   not skipped (`font_setup_dialog.rs`). Help > セットアップ opens it at any time.
//!
//! Nothing here stops the start: a list that cannot be read, a download that fails or runs out of disk only says so in the
//! dialog (or the log), and the font is asked about again.

/// The public repository's list. `IBIS_FONT_LIST_URL` replaces it (the tests; a local server, or a file's address).
pub const LIST_URL: &str = "https://raw.githubusercontent.com/ibisjp/ibis-office/main/docs/font-list.txt";

/// The setting that records the version whose start has asked about the fonts.
pub const CHECKED_KEY: &str = "font-setup.checked-version";
const SKIP_PREFIX: &str = "font-setup.skip.";

/// One font of the list: a line of `font-list.txt`, a JSON object with these keys.
///
/// | key | meaning |
/// |---|---|
/// | `id` | the font's folder name (on Pages and here); `[A-Za-z0-9._-]`; default: the file's name without its extension, lower case |
/// | `name_en` | the family name in English (required) |
/// | `name_ja` | the name in Japanese (default `name_en`) |
/// | `file` | the font file's name (`.ttf`, `.otf` or `.ttc`; required) |
/// | `size` | the file's size in bytes (the download is checked against it) |
/// | `url` | the font file's address (required; https) |
/// | `license` | the licence's name, as shown (`SIL OFL 1.1`) |
/// | `license_url` | the address of the licence's file (https) |
/// | `sha256` | the file's SHA-256, hex (optional; checked when given) |
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontEntry {
    pub id: String,
    pub name_en: String,
    pub name_ja: String,
    pub file: String,
    pub size: u64,
    pub url: String,
    pub license: String,
    pub license_url: String,
    pub sha256: Option<String>,
}

impl FontEntry {
    /// The name in the language of the UI.
    pub fn display_name(&self) -> &str {
        if crate::i18n::language() == ibis_office_base::i18n::Language::Japanese { &self.name_ja } else { &self.name_en }
    }

    /// The licence file's name beside the font (the last part of its address).
    pub fn license_file(&self) -> String {
        let last = self.license_url.split(['?', '#']).next().unwrap_or("").rsplit('/').next().unwrap_or("");
        if safe_name(last) { last.to_string() } else { "LICENSE.txt".to_string() }
    }
}

/// A name that is one plain path component: ASCII letters, digits, `.`, `_`, `-`, and not beginning with a dot.
fn safe_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// An address the app may fetch: https, or a loopback server (the tests).
fn safe_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost")
}

// ---- JSON Lines ----------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Str(String),
    Num(f64),
    Other,
}

/// A line of a flat JSON object (string, number, true/false/null values) as its pairs; `None` when it is not one.
fn parse_object(line: &str) -> Option<Vec<(String, Json)>> {
    let mut chars = line.trim().chars().peekable();
    let skip_ws = |chars: &mut std::iter::Peekable<std::str::Chars>| {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
    };
    fn string(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
        if chars.next()? != '"' {
            return None;
        }
        let mut out = String::new();
        let hex4 = |chars: &mut std::iter::Peekable<std::str::Chars>| -> Option<u32> {
            let mut v = 0;
            for _ in 0..4 {
                v = v * 16 + chars.next()?.to_digit(16)?;
            }
            Some(v)
        };
        loop {
            match chars.next()? {
                '"' => return Some(out),
                '\\' => match chars.next()? {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' | 'f' => {}
                    'u' => {
                        let first = hex4(chars)?;
                        let code = if (0xD800..0xDC00).contains(&first) {
                            // A surrogate pair: `😀`.
                            if chars.next()? != '\\' || chars.next()? != 'u' {
                                return None;
                            }
                            let second = hex4(chars)?;
                            if !(0xDC00..0xE000).contains(&second) {
                                return None;
                            }
                            0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
                        } else {
                            first
                        };
                        out.push(char::from_u32(code)?);
                    }
                    _ => return None,
                },
                c => out.push(c),
            }
        }
    }
    skip_ws(&mut chars);
    if chars.next()? != '{' {
        return None;
    }
    let mut pairs = Vec::new();
    skip_ws(&mut chars);
    if chars.peek() == Some(&'}') {
        return Some(pairs);
    }
    loop {
        skip_ws(&mut chars);
        let key = string(&mut chars)?;
        skip_ws(&mut chars);
        if chars.next()? != ':' {
            return None;
        }
        skip_ws(&mut chars);
        let value = match chars.peek()? {
            '"' => Json::Str(string(&mut chars)?),
            _ => {
                let mut word = String::new();
                while chars.peek().is_some_and(|c| !matches!(c, ',' | '}') && !c.is_whitespace()) {
                    word.push(chars.next()?);
                }
                match word.parse::<f64>() {
                    Ok(n) => Json::Num(n),
                    Err(_) if matches!(word.as_str(), "true" | "false" | "null") => Json::Other,
                    Err(_) => return None,
                }
            }
        };
        pairs.push((key, value));
        skip_ws(&mut chars);
        match chars.next()? {
            ',' => {}
            '}' => break,
            _ => return None,
        }
    }
    skip_ws(&mut chars);
    chars.next().is_none().then_some(pairs)
}

/// The fonts of `font-list.txt`: one JSON object a line; empty lines and lines beginning with `#` are skipped, and so is a
/// line that is not a font (not JSON, a required key missing, a name or an address that is not plain), which does not stop the
/// others. A font file name that is listed twice is listed once (the first).
pub fn parse_list(text: &str) -> Vec<FontEntry> {
    let mut out: Vec<FontEntry> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let Some(pairs) = parse_object(line) else {
            log::warn!("font-list.txt: a line is not a JSON object: {line}");
            continue;
        };
        let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        let string = |key: &str| match get(key) {
            Some(Json::Str(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
            _ => None,
        };
        let (Some(name_en), Some(file), Some(url)) = (string("name_en"), string("file"), string("url")) else {
            log::warn!("font-list.txt: a font without name_en, file or url: {line}");
            continue;
        };
        let extension_ok = file.rsplit_once('.').is_some_and(|(_, e)| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc"));
        let id = string("id").unwrap_or_else(|| file.rsplit_once('.').map_or(file.clone(), |(stem, _)| stem.to_ascii_lowercase()));
        let license_url = string("license_url").unwrap_or_default();
        if !safe_name(&file) || !extension_ok || !safe_name(&id) || !safe_url(&url) || (!license_url.is_empty() && !safe_url(&license_url)) {
            log::warn!("font-list.txt: a name or an address that is not allowed: {line}");
            continue;
        }
        let size = match get("size") {
            Some(Json::Num(n)) if *n >= 0. => *n as u64,
            _ => 0,
        };
        let sha256 = string("sha256").map(|s| s.to_ascii_lowercase()).filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()));
        if out.iter().any(|e| e.file == file) {
            continue;
        }
        out.push(FontEntry {
            id,
            name_ja: string("name_ja").unwrap_or_else(|| name_en.clone()),
            name_en,
            file,
            size,
            url,
            license: string("license").unwrap_or_default(),
            license_url,
            sha256,
        });
    }
    out
}

// ---- The skip flags (the settings file) -----------------------------------

/// Whether the user said 「インストールしない」 for the font file `file`.
pub fn is_skipped(file: &str) -> bool {
    crate::settings::get(&format!("{SKIP_PREFIX}{file}")).as_deref() == Some("1")
}

/// Record (or forget) that the font file `file` is skipped.
pub fn set_skipped(file: &str, skipped: bool) {
    let key = format!("{SKIP_PREFIX}{file}");
    if skipped {
        crate::settings::set(&key, "1");
    } else {
        crate::settings::remove(&key);
    }
}

// ---- What is installed ----------------------------------------------------

/// The folder of a font: `fonts/<id>` in the app's data.
#[cfg(not(target_family = "wasm"))]
pub fn font_folder(entry: &FontEntry) -> Option<std::path::PathBuf> {
    Some(ibis_office_base::app_data::fonts_dir()?.join(&entry.id))
}

/// Whether the font's file is installed (here, with the size the list gives).
#[cfg(not(target_family = "wasm"))]
pub fn is_installed(entry: &FontEntry) -> bool {
    font_folder(entry)
        .and_then(|dir| std::fs::metadata(dir.join(&entry.file)).ok())
        .is_some_and(|m| m.is_file() && (entry.size == 0 || m.len() == entry.size))
}

// ---- The rows of the dialog -----------------------------------------------

/// A font the dialog offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub entry: FontEntry,
    /// Whether it is to be downloaded.
    pub checked: bool,
    /// Whether the user skipped it before (it is off by default, and listed last).
    pub skipped: bool,
}

/// The rows for the fonts of the list that are not installed (`installed` says which are): the ones not skipped first, in the
/// list's order, then the skipped ones; the skipped ones are off, the others on.
pub fn build_rows(entries: &[FontEntry], installed: impl Fn(&FontEntry) -> bool, skipped: impl Fn(&FontEntry) -> bool) -> Vec<Row> {
    let mut rows: Vec<Row> = entries
        .iter()
        .filter(|e| !installed(e))
        .map(|e| {
            let skipped = skipped(e);
            Row { entry: e.clone(), checked: !skipped, skipped }
        })
        .collect();
    // A stable sort: the list's order is kept inside each part.
    rows.sort_by_key(|r| r.skipped);
    rows
}

/// Whether the startup check has something to ask: a font that is not installed and not skipped.
pub fn has_new(entries: &[FontEntry], installed: impl Fn(&FontEntry) -> bool, skipped: impl Fn(&FontEntry) -> bool) -> bool {
    entries.iter().any(|e| !installed(e) && !skipped(e))
}

/// The header checkbox: all on, all off, or some of both (`None`).
pub fn all_checked(rows: &[Row]) -> Option<bool> {
    let on = rows.iter().filter(|r| r.checked).count();
    match (on, rows.len()) {
        (_, 0) => Some(false),
        (n, total) if n == total => Some(true),
        (0, _) => Some(false),
        _ => None,
    }
}

/// A click on the header checkbox: every row on, unless every row is on already (then every row off).
pub fn toggle_all(rows: &mut [Row]) {
    let to = all_checked(rows) != Some(true);
    rows.iter_mut().for_each(|r| r.checked = to);
}

/// 「インストールしない」: every row of the dialog is recorded as skipped.
pub fn skip_all(rows: &[Row]) {
    for row in rows {
        set_skipped(&row.entry.file, true);
    }
}

/// The size as shown (`12.3 MB`).
pub fn size_text(bytes: u64) -> String {
    const MB: f64 = 1024. * 1024.;
    if bytes == 0 {
        String::new()
    } else if (bytes as f64) < MB {
        format!("{:.0} KB", (bytes as f64 / 1024.).max(1.))
    } else {
        format!("{:.1} MB", bytes as f64 / MB)
    }
}

// ---- The list and the files, over the network (not in the browser) ----------

/// The address of the list.
#[cfg(not(target_family = "wasm"))]
pub fn list_url() -> String {
    std::env::var("IBIS_FONT_LIST_URL").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| LIST_URL.to_string())
}

/// Read and parse the list (run off the main thread). The error is a line for the log and the dialog.
#[cfg(not(target_family = "wasm"))]
pub fn fetch_list() -> Result<Vec<FontEntry>, String> {
    let text = fetch_text_from(&list_url())?;
    Ok(parse_list(&text))
}

/// Why a transfer did not end in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    /// The user cancelled.
    Cancelled,
    /// Anything else: the words for the log and the dialog (curl's message, the disk's, the check's).
    Failed(String),
}

#[cfg(not(target_family = "wasm"))]
fn curl_command(url: &str, max_seconds: u32) -> std::process::Command {
    use std::process::{Command, Stdio};
    let mut curl = Command::new("curl");
    curl.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        curl.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    curl.args(["--fail", "--silent", "--show-error", "--location", "--connect-timeout", "10", "--max-time", &max_seconds.to_string()]);
    // Only https, also after a redirect; a loopback server (the tests) may be plain http.
    if url.starts_with("https://") {
        curl.args(["--proto", "=https", "--proto-redir", "=https"]);
    } else {
        curl.args(["--proto", "=http,https", "--proto-redir", "=https"]);
    }
    curl.arg(url);
    curl
}

/// The text at `url`.
#[cfg(not(target_family = "wasm"))]
pub fn fetch_text_from(url: &str) -> Result<String, String> {
    if !safe_url(url) {
        return Err(format!("{url}: only https addresses are read"));
    }
    let out = curl_command(url, 20).output().map_err(|e| format!("curl: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    Err(format!("curl {}: {}", out.status, String::from_utf8_lossy(&out.stderr).trim()))
}

/// Receive `url` into the file `part` with curl; `progress` is the bytes in the file so far (polled), and `cancel` set stops curl.
#[cfg(not(target_family = "wasm"))]
fn fetch_to_file(url: &str, part: &std::path::Path, max_seconds: u32, progress: Option<&std::sync::atomic::AtomicU64>, cancel: &std::sync::atomic::AtomicBool) -> Result<(), DownloadError> {
    use std::sync::atomic::Ordering;
    if !safe_url(url) {
        return Err(DownloadError::Failed(format!("{url}: only https addresses are read")));
    }
    let mut child = curl_command(url, max_seconds)
        .arg("--output")
        .arg(part)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| DownloadError::Failed(format!("curl: {e}")))?;
    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(DownloadError::Cancelled);
        }
        if let Some(progress) = progress {
            progress.store(std::fs::metadata(part).map(|m| m.len()).unwrap_or(0), Ordering::SeqCst);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut message = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    use std::io::Read as _;
                    let _ = stderr.read_to_string(&mut message);
                }
                if let Some(progress) = progress {
                    progress.store(std::fs::metadata(part).map(|m| m.len()).unwrap_or(0), Ordering::SeqCst);
                }
                return if status.success() { Ok(()) } else { Err(DownloadError::Failed(format!("curl {status}: {}", message.trim()))) };
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => return Err(DownloadError::Failed(format!("curl: {e}"))),
        }
    }
}

/// Download the font of `entry` into its folder (run off the main thread): the licence file first (a font is not installed
/// without its licence beside it), then the font into `<file>.part`, checked against `size` and `sha256` when the list has them,
/// then renamed to its name. `progress` is the bytes of the font file received so far; `cancel` set stops the transfer
/// (the unfinished file is removed). Any failure leaves no unfinished file.
#[cfg(not(target_family = "wasm"))]
pub fn download(entry: &FontEntry, progress: &std::sync::atomic::AtomicU64, cancel: &std::sync::atomic::AtomicBool) -> Result<(), DownloadError> {
    use std::sync::atomic::Ordering;
    let failed = |m: String| DownloadError::Failed(m);
    let dir = font_folder(entry).ok_or_else(|| failed("no folder for the fonts".into()))?;
    std::fs::create_dir_all(&dir).map_err(|e| failed(format!("{}: {e}", dir.display())))?;
    progress.store(0, Ordering::SeqCst);
    if !entry.license_url.is_empty() {
        let license = dir.join(entry.license_file());
        let part = dir.join(format!("{}.part", entry.license_file()));
        let result = fetch_to_file(&entry.license_url, &part, 120, None, cancel);
        finish(result, &part, &license, None, None)?;
    }
    let font = dir.join(&entry.file);
    let part = dir.join(format!("{}.part", entry.file));
    let result = fetch_to_file(&entry.url, &part, 1800, Some(progress), cancel);
    finish(result, &part, &font, (entry.size > 0).then_some(entry.size), entry.sha256.as_deref())
}

/// The end of one transfer: on failure the part is removed; else it is checked and put in its place.
#[cfg(not(target_family = "wasm"))]
fn finish(result: Result<(), DownloadError>, part: &std::path::Path, to: &std::path::Path, size: Option<u64>, sha256: Option<&str>) -> Result<(), DownloadError> {
    let fail = |e: DownloadError| {
        let _ = std::fs::remove_file(part);
        Err(e)
    };
    if let Err(e) = result {
        return fail(e);
    }
    if let Some(expected) = size {
        let got = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        if got != expected {
            return fail(DownloadError::Failed(format!("the size is {got} bytes, the list says {expected}")));
        }
    }
    if let Some(expected) = sha256 {
        match crate::update::sha256_of(part) {
            Ok(got) if got == expected => {}
            Ok(got) => return fail(DownloadError::Failed(format!("the SHA-256 is {got}, the list says {expected}"))),
            Err(e) => return fail(DownloadError::Failed(e)),
        }
    }
    std::fs::rename(part, to).map_err(|e| {
        let _ = std::fs::remove_file(part);
        DownloadError::Failed(format!("{}: {e}", to.display()))
    })
}

/// A small HTTP server on 127.0.0.1 for the tests (the list, font files, a 404, a transfer that stalls): the tests never use the
/// internet. It stops when it is dropped.
#[cfg(all(any(test, feature = "scenarios"), not(target_family = "wasm")))]
pub mod test_support {
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// What a path answers.
    #[derive(Clone)]
    pub enum Route {
        /// 200 with this body.
        Body(Vec<u8>),
        /// 404.
        NotFound,
        /// 200 that says it has `total` bytes, sends 256 KB and then waits (until the server stops or 10 s).
        Stalls { total: usize },
    }

    pub struct Server {
        pub port: u16,
        stop: Arc<AtomicBool>,
    }

    impl Server {
        pub fn start(routes: Vec<(&str, Route)>) -> Server {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
            listener.set_nonblocking(true).unwrap();
            let port = listener.local_addr().unwrap().port();
            let stop = Arc::new(AtomicBool::new(false));
            let routes: HashMap<String, Route> = routes.into_iter().map(|(p, r)| (p.to_string(), r)).collect();
            let flag = stop.clone();
            std::thread::spawn(move || {
                while !flag.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            stream.set_nonblocking(false).ok();
                            let (routes, flag) = (routes.clone(), flag.clone());
                            std::thread::spawn(move || {
                                let mut request = Vec::new();
                                let mut buf = [0u8; 1024];
                                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                                    match stream.read(&mut buf) {
                                        Ok(0) | Err(_) => return,
                                        Ok(n) => request.extend_from_slice(&buf[..n]),
                                    }
                                }
                                let line = String::from_utf8_lossy(&request).lines().next().unwrap_or("").to_string();
                                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                                match routes.get(&path) {
                                    Some(Route::Body(body)) => {
                                        let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                                        let _ = stream.write_all(body);
                                    }
                                    Some(Route::Stalls { total }) => {
                                        let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n");
                                        let _ = stream.write_all(&[0u8; 256 * 1024]);
                                        let _ = stream.flush();
                                        for _ in 0..100 {
                                            if flag.load(Ordering::SeqCst) {
                                                break;
                                            }
                                            std::thread::sleep(std::time::Duration::from_millis(100));
                                        }
                                    }
                                    _ => {
                                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                                    }
                                }
                            });
                        }
                        Err(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
                    }
                }
            });
            Server { port, stop }
        }

        /// The address of `path` on this server.
        pub fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{path}", self.port)
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{all_checked, build_rows, has_new, parse_list, size_text, toggle_all, FontEntry};

    fn entry(file: &str) -> FontEntry {
        parse_list(&format!("{{\"name_en\":\"N\",\"file\":\"{file}\",\"url\":\"https://example.com/{file}\"}}")).remove(0)
    }

    #[test]
    fn a_line_of_the_list_gives_a_font() {
        let list = parse_list(concat!(
            "# a comment\n",
            "\n",
            "{\"id\":\"carlito\",\"name_en\":\"Carlito\",\"name_ja\":\"\\u30ab\\u30fc\\u30ea\\u30c8\",\"file\":\"Carlito-Regular.ttf\",\"size\":2048,",
            "\"url\":\"https://x.example/fonts/carlito/Carlito-Regular.ttf\",\"license\":\"SIL OFL 1.1\",",
            "\"license_url\":\"https://x.example/fonts/carlito/OFL.txt\",\"sha256\":\"",
            "0000000000000000000000000000000000000000000000000000000000000AbC\"}\n",
        ));
        assert_eq!(list.len(), 1);
        let f = &list[0];
        assert_eq!((f.id.as_str(), f.name_en.as_str(), f.file.as_str(), f.size), ("carlito", "Carlito", "Carlito-Regular.ttf", 2048));
        assert_eq!(f.name_ja, "\u{30ab}\u{30fc}\u{30ea}\u{30c8}");
        assert_eq!(f.license_file(), "OFL.txt");
        assert!(f.sha256.is_some(), "the hash is read, in lower case");
        assert!(f.sha256.as_ref().unwrap().ends_with("abc"));
    }

    #[test]
    fn a_surrogate_pair_and_raw_utf8_are_read() {
        let list = parse_list("{\"name_en\":\"A\",\"name_ja\":\"\\ud83d\\ude00\u{3042}\",\"file\":\"a.otf\",\"url\":\"https://x.example/a.otf\"}");
        assert_eq!(list[0].name_ja, "\u{1F600}\u{3042}");
        assert_eq!(list[0].id, "a", "the folder defaults to the file's name");
    }

    #[test]
    fn a_line_that_is_not_a_font_is_skipped_and_the_others_stay() {
        let list = parse_list(concat!(
            "not json\n",
            "{\"name_en\":\"No file\",\"url\":\"https://x.example/a.ttf\"}\n",
            "{\"name_en\":\"Path\",\"file\":\"../evil.ttf\",\"url\":\"https://x.example/a.ttf\"}\n",
            "{\"name_en\":\"Folder\",\"id\":\"a/b\",\"file\":\"a.ttf\",\"url\":\"https://x.example/a.ttf\"}\n",
            "{\"name_en\":\"Exe\",\"file\":\"a.exe\",\"url\":\"https://x.example/a.exe\"}\n",
            "{\"name_en\":\"Plain http\",\"file\":\"p.ttf\",\"url\":\"http://x.example/p.ttf\"}\n",
            "{\"name_en\":\"Local\",\"file\":\"l.ttf\",\"url\":\"http://127.0.0.1:8000/l.ttf\"}\n",
            "{\"name_en\":\"Local again\",\"file\":\"l.ttf\",\"url\":\"http://127.0.0.1:8000/l2.ttf\"}\n",
            "{\"name_en\":\"Broken\",\"file\":\"b.ttf\",\"url\":\"https://x.example/b.ttf\"\n",
        ));
        assert_eq!(list.iter().map(|f| f.file.as_str()).collect::<Vec<_>>(), ["l.ttf"], "one font, the first of the same file name");
    }

    #[test]
    fn rows_leave_out_the_installed_and_put_the_skipped_last_and_off() {
        let all: Vec<FontEntry> = ["a.ttf", "b.ttf", "c.ttf", "d.ttf"].iter().map(|f| entry(f)).collect();
        let rows = build_rows(&all, |e| e.file == "a.ttf", |e| e.file == "b.ttf");
        assert_eq!(rows.iter().map(|r| (r.entry.file.as_str(), r.checked)).collect::<Vec<_>>(), [("c.ttf", true), ("d.ttf", true), ("b.ttf", false)]);
        assert!(rows[2].skipped && !rows[0].skipped);
    }

    #[test]
    fn the_startup_check_asks_only_for_a_font_that_is_neither_installed_nor_skipped() {
        let all: Vec<FontEntry> = ["a.ttf", "b.ttf"].iter().map(|f| entry(f)).collect();
        assert!(has_new(&all, |_| false, |_| false));
        assert!(has_new(&all, |e| e.file == "a.ttf", |_| false));
        assert!(!has_new(&all, |e| e.file == "a.ttf", |e| e.file == "b.ttf"));
        assert!(!has_new(&all, |_| true, |_| false));
        assert!(!has_new(&[], |_| false, |_| false));
    }

    #[test]
    fn the_header_checkbox_turns_everything_on_or_off() {
        let all: Vec<FontEntry> = ["a.ttf", "b.ttf", "c.ttf"].iter().map(|f| entry(f)).collect();
        let mut rows = build_rows(&all, |_| false, |e| e.file == "c.ttf");
        assert_eq!(all_checked(&rows), None, "some on, some off");
        toggle_all(&mut rows);
        assert_eq!(all_checked(&rows), Some(true), "mixed -> all on");
        toggle_all(&mut rows);
        assert_eq!(all_checked(&rows), Some(false), "all on -> all off");
        toggle_all(&mut rows);
        assert_eq!(all_checked(&rows), Some(true), "all off -> all on");
        assert_eq!(all_checked(&[]), Some(false));
    }

    #[test]
    fn sizes_are_shown_in_kb_and_mb() {
        assert_eq!(size_text(0), "");
        assert_eq!(size_text(500), "1 KB");
        assert_eq!(size_text(2048), "2 KB");
        assert_eq!(size_text(12 * 1024 * 1024 + 512 * 1024), "12.5 MB");
    }

    #[cfg(not(target_family = "wasm"))]
    mod download {
        use super::super::test_support::{Route, Server};
        use super::super::{download, font_folder, is_installed, is_skipped, parse_list, set_skipped, DownloadError, FontEntry};
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

        fn entry(server: &Server, id: &str, file: &str, size: usize, sha256: Option<&str>) -> FontEntry {
            let sha = sha256.map(|h| format!(",\"sha256\":\"{h}\"")).unwrap_or_default();
            parse_list(&format!(
                "{{\"id\":\"{id}\",\"name_en\":\"T\",\"file\":\"{file}\",\"size\":{size},\"url\":\"{}\",\"license\":\"SIL OFL 1.1\",\"license_url\":\"{}\"{sha}}}",
                server.url(&format!("/{id}/{file}")),
                server.url(&format!("/{id}/OFL.txt")),
            ))
            .remove(0)
        }

        fn clean(e: &FontEntry) {
            let _ = std::fs::remove_dir_all(font_folder(e).unwrap());
        }

        #[test]
        fn a_font_is_downloaded_with_its_licence_and_found_installed() {
            let body: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
            let server = Server::start(vec![("/t-ok/a.ttf", Route::Body(body.clone())), ("/t-ok/OFL.txt", Route::Body(b"licence".to_vec()))]);
            let e = entry(&server, "t-ok", "a.ttf", body.len(), None);
            clean(&e);
            assert!(!is_installed(&e));
            let (progress, cancel) = (AtomicU64::new(0), AtomicBool::new(false));
            download(&e, &progress, &cancel).expect("the download");
            assert!(is_installed(&e));
            assert_eq!(std::fs::read(font_folder(&e).unwrap().join("a.ttf")).unwrap(), body);
            assert_eq!(std::fs::read(font_folder(&e).unwrap().join("OFL.txt")).unwrap(), b"licence");
            assert_eq!(progress.load(Ordering::SeqCst), body.len() as u64);
            assert!(ibis_office_base::app_data::downloaded_font_files().iter().any(|p| p.ends_with("t-ok/a.ttf")), "the render crate finds the file");
            clean(&e);
        }

        #[test]
        fn a_wrong_size_a_wrong_hash_and_a_missing_file_fail_and_leave_nothing() {
            let server = Server::start(vec![("/t-bad/a.ttf", Route::Body(vec![1; 100])), ("/t-bad/OFL.txt", Route::Body(b"l".to_vec())), ("/t-bad/b.ttf", Route::Body(vec![2; 100]))]);
            let wrong_size = entry(&server, "t-bad", "a.ttf", 99, None);
            let wrong_hash = entry(&server, "t-bad", "b.ttf", 100, Some(&"0".repeat(64)));
            let missing = entry(&server, "t-bad", "c.ttf", 100, None);
            let (progress, cancel) = (AtomicU64::new(0), AtomicBool::new(false));
            for e in [&wrong_size, &wrong_hash, &missing] {
                let result = download(e, &progress, &cancel);
                assert!(matches!(result, Err(DownloadError::Failed(_))), "{}: {result:?}", e.file);
                assert!(!is_installed(e));
                let leftovers: Vec<_> = std::fs::read_dir(font_folder(e).unwrap()).unwrap().flatten().map(|f| f.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".part") || n.ends_with(".ttf")).collect();
                assert!(leftovers.is_empty(), "{}: nothing unfinished is left: {leftovers:?}", e.file);
            }
            clean(&wrong_size);
        }

        #[test]
        fn no_server_is_a_failure_not_a_panic() {
            let server = Server::start(vec![]);
            let e = entry(&server, "t-down", "a.ttf", 10, None);
            drop(server);
            std::thread::sleep(std::time::Duration::from_millis(200));
            let (progress, cancel) = (AtomicU64::new(0), AtomicBool::new(false));
            assert!(matches!(download(&e, &progress, &cancel), Err(DownloadError::Failed(_))));
            clean(&e);
        }

        #[test]
        fn cancelling_stops_the_transfer_and_removes_the_part() {
            let server = Server::start(vec![("/t-cancel/a.ttf", Route::Stalls { total: 1 << 20 }), ("/t-cancel/OFL.txt", Route::Body(b"l".to_vec()))]);
            let e = entry(&server, "t-cancel", "a.ttf", 1 << 20, None);
            let (progress, cancel) = (AtomicU64::new(0), AtomicBool::new(false));
            let result = std::thread::scope(|scope| {
                scope.spawn(|| {
                    // Once some of the file has arrived.
                    for _ in 0..100 {
                        if progress.load(Ordering::SeqCst) > 0 {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    cancel.store(true, Ordering::SeqCst);
                });
                download(&e, &progress, &cancel)
            });
            assert_eq!(result, Err(DownloadError::Cancelled));
            assert!(progress.load(Ordering::SeqCst) > 0, "it had begun");
            assert!(!is_installed(&e));
            assert!(!font_folder(&e).unwrap().join("a.ttf.part").exists());
            clean(&e);
        }

        #[test]
        fn the_list_in_the_repository_is_a_valid_list_and_starts_with_ipamj_mincho() {
            let list = parse_list(include_str!("../../../publish/doc/font-list.txt"));
            assert_eq!(list.len(), 1);
            let f = &list[0];
            assert_eq!((f.id.as_str(), f.file.as_str(), f.name_en.as_str()), ("IPAmjMincho", "ipamjm.ttf", "IPAmj Mincho"));
            assert_eq!(f.license_file(), "LICENSE");
            assert!(f.url.ends_with("/fonts/IPAmjMincho/ipamjm.ttf"));
        }

        #[test]
        fn the_list_is_read_over_http_and_skips_are_kept_in_the_settings() {
            let list = "{\"id\":\"t-list\",\"name_en\":\"L\",\"file\":\"t-list-skip.ttf\",\"url\":\"http://127.0.0.1:1/x.ttf\"}\n";
            let server = Server::start(vec![("/font-list.txt", Route::Body(list.as_bytes().to_vec()))]);
            let text = super::super::fetch_text_from(&server.url("/font-list.txt")).expect("the list");
            assert_eq!(parse_list(&text).len(), 1);
            assert!(super::super::fetch_text_from(&server.url("/nothing")).is_err(), "a 404 is an error");
            assert!(super::super::fetch_text_from("http://example.com/font-list.txt").is_err(), "plain http to another host is refused");
            assert!(!is_skipped("t-list-skip.ttf"));
            set_skipped("t-list-skip.ttf", true);
            assert!(is_skipped("t-list-skip.ttf"));
            set_skipped("t-list-skip.ttf", false);
            assert!(!is_skipped("t-list-skip.ttf"));
        }
    }
}
