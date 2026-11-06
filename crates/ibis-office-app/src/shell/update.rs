// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Automatic update (todo-fix #35; macOS, Windows and Linux).
//!
//! At startup (ツール > 設定… > 起動時に更新を確かめる, on unless turned off) a
//! background task reads version.txt on GitHub. If this platform's version
//! there is newer than the one built in (`IBIS_VERSION`), it downloads that
//! version's zip and asks 「ibisOffice の新しいバージョンが見つかりました。更新しますか？」.
//! On 「はい」 the zip is unpacked, the new app is checked, and
//! `ibisOfficeUpdater` from the new app is started; ibisOffice quits, the
//! updater swaps the installed copy for the new one and starts it (see
//! `crates/ibis-office-updater`).
//!
//! - macOS: the whole `ibisOffice.app` is replaced (replacing the executable
//!   alone would break the signature). The unpacked app must be signed by
//!   ibis inc.'s team and accepted by Gatekeeper (`codesign --verify --deep
//!   --strict`, `spctl -a -t exec`) before anything in it is run.
//! - Windows (the code signature is not checked yet): the zip's SHA-256 must
//!   equal the one in the `.sha256` file beside it on the Releases page.
//! - Linux: the AppImage (`ibisOffice-<version>-Linux.AppImage`, no zip) must
//!   match its `.sha256` the same way and be an AppImage; it takes the place
//!   of the one running (`APPIMAGE`, renamed over it in its folder, which a
//!   running program allows) and is started; no updater is needed. Its
//!   runtime unpacks the new version at that start (`crates/ibis-appimage`).
//! - Not done: from a build run out of `target/` (or on Linux not from its
//!   AppImage), from a folder the user cannot write to, or from macOS's App
//!   Translocation (the log says which, and a window asks to install it again
//!   by hand).
//! - Two attempts at one version that did not end in that version running
//!   stop the offers (a window says so once).
//!
//! Every step goes to `updater.log` (`ibis_office_updater::log`). Downloading
//! and unpacking use the system's `curl` and `ditto` (`tar` on Windows; the
//! hashes `certutil` on Windows, `sha256sum` on Linux).
//! For the tests: `IBIS_UPDATE_URL_BASE` replaces the GitHub addresses,
//! `IBIS_UPDATE_DIR` the folders of the files, `IBIS_UPDATE_AUTO_YES=1`
//! answers 「はい」, and `IBIS_UPDATE_SKIP_SIGNATURE=1` skips the signature
//! check, only in an app that is not signed by ibis inc. itself.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use gpui_kit::{App, AsyncApp, PromptButton, PromptLevel};
use ibis_office_updater::log::Log;
use ibis_office_updater::{paths, version};

use crate::shell::{i18n, settings};

/// This build's version (version.txt's line for this platform at build time).
const CURRENT: &str = env!("IBIS_VERSION");
const VERSION_URL: &str = "https://raw.githubusercontent.com/ibisjp/ibis-office/main/version.txt";
const RELEASES_URL: &str = "https://github.com/ibisjp/ibis-office/releases";
/// Attempts at one version after which no more offers are made.
const MAX_ATTEMPTS: u32 = 2;
/// ibis inc.'s Apple developer team.
#[cfg(target_os = "macos")]
const TEAM_ID: &str = "SQ8CQKBW45";

const CHECK_KEY: &str = "check-updates";
const ATTEMPT_KEY: &str = "update-attempt";
const NOTIFIED_KEY: &str = "update-notified";

/// Whether to look for updates at startup (settings; on unless turned off).
pub(crate) fn check_enabled() -> bool {
    settings::get(CHECK_KEY).as_deref() != Some("0")
}

pub(crate) fn set_check_enabled(on: bool) {
    settings::set(CHECK_KEY, if on { "1" } else { "0" });
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn log_for(version: &str) -> Option<Log> {
    Some(Log::new(paths::log_file()?, version))
}

/// ヘルプ > 更新のログを表示: show `updater.log` in Finder (Explorer).
pub(crate) fn reveal_log(cx: &mut App) {
    let Some(path) = paths::log_file() else { return };
    if !path.exists() {
        // Nothing logged yet: show the folder it will be in.
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
            cx.open_with_system(dir);
        }
        return;
    }
    cx.reveal_path(&path);
}

// ---- Start-up ----------------------------------------------------------

/// What the background check found.
#[derive(Clone)]
enum Found {
    /// This copy is the latest.
    Nothing,
    /// The check could not be made (no network, no version.txt line, the download failed) or this copy is a build that does
    /// not update itself (run out of `target/`). Silent at start-up; the user's own check says so.
    Failed,
    /// The zip of `version` is downloaded.
    Ready { version: String, zip: PathBuf },
    /// There is a newer version this copy cannot update to by itself.
    Manual { version: String },
}

thread_local! {
    /// The newer version the start-up check found (kept for the About window's 「新しいバージョンがあります。」 button, which asks
    /// the question again whatever was answered, or the setting 起動時に更新を確かめる says, at start-up).
    static AVAILABLE: std::cell::RefCell<Option<Found>> = const { std::cell::RefCell::new(None) };
    /// For the tests: the versions the question was asked about again.
    /// A check of the user's own is running (a second click while it runs does nothing).
    static CHECKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// For the tests: how many times the user's own check was asked for.
    #[cfg(any(test, feature = "scenarios"))]
    static CHECKED_NOW: std::cell::RefCell<u32> = const { std::cell::RefCell::new(0) };
    #[cfg(any(test, feature = "scenarios"))]
    static ASKED_AGAIN: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The newer version the start-up check found, if it did.
pub(crate) fn available_version() -> Option<String> {
    AVAILABLE.with(|a| match a.borrow().as_ref() {
        Some(Found::Ready { version, .. } | Found::Manual { version }) => Some(version.clone()),
        _ => None,
    })
}

/// For the tests (no network): what the start-up check would have found.
#[cfg(any(test, feature = "scenarios"))]
pub(crate) fn set_available_for_test(version: Option<&str>) {
    AVAILABLE.with(|a| *a.borrow_mut() = version.map(|v| Found::Ready { version: v.to_string(), zip: PathBuf::new() }));
}

/// For the tests: how many times 更新を確認 was used.
#[cfg(any(test, feature = "scenarios"))]
pub(crate) fn checked_now_for_test() -> u32 {
    CHECKED_NOW.with(|c| *c.borrow())
}

/// For the tests: the versions the question was asked about again, oldest first.
#[cfg(any(test, feature = "scenarios"))]
pub(crate) fn asked_again_for_test() -> Vec<String> {
    ASKED_AGAIN.with(|a| a.borrow().clone())
}

/// Ask the question the start-up check asked (「新しいバージョンが見つかりました。更新しますか？」) again. A click asks
/// whatever was answered before and whatever the setting says: an update that was refused is still there.
pub(crate) fn offer_again(cx: &mut App) {
    let _ = &cx;
    let Some(found) = AVAILABLE.with(|a| a.borrow().clone()) else { return };
    #[cfg(any(test, feature = "scenarios"))]
    {
        if let Found::Ready { version, .. } | Found::Manual { version } = &found {
            ASKED_AGAIN.with(|a| a.borrow_mut().push(version.clone()));
        }
        return;
    }
    #[allow(unreachable_code)]
    cx.spawn(async move |cx: &mut AsyncApp| match found {
        Found::Ready { version, zip } => offer(cx, version, zip, true).await,
        Found::Manual { .. } => show_failed(cx).await,
        Found::Nothing | Found::Failed => {}
    })
    .detach();
}

/// ヘルプ > 更新を確認 / the start page's 更新を確認: the user's own check, whatever the setting 起動時に更新を確かめる says
/// and whether the start-up question was refused. A newer version asks the update question; none says so (「最新です」);
/// a check that could not be made says so too. Not in the browser (no updater there; the caller does not show it).
pub(crate) fn check_now(cx: &mut App) {
    #[cfg(any(test, feature = "scenarios"))]
    {
        let _ = &cx;
        CHECKED_NOW.with(|c| *c.borrow_mut() += 1);
        return;
    }
    #[allow(unreachable_code)]
    {
        if CHECKING.with(|c| c.replace(true)) {
            return;
        }
        cx.spawn(async move |cx: &mut AsyncApp| {
            // A version the start-up check found and downloaded is asked about at once.
            let known = AVAILABLE.with(|a| a.borrow().clone());
            let found = match known {
                Some(found @ (Found::Ready { .. } | Found::Manual { .. })) => found,
                _ => cx.background_executor().spawn(async { check() }).await,
            };
            if matches!(found, Found::Ready { .. } | Found::Manual { .. }) {
                AVAILABLE.with(|a| *a.borrow_mut() = Some(found.clone()));
            }
            match found {
                Found::Ready { version, zip } => offer(cx, version, zip, true).await,
                Found::Manual { .. } => show_failed(cx).await,
                Found::Nothing => {
                    let detail = i18n::format("fix.update.latest.detail", &[("current", &CURRENT)]);
                    prompt(cx, PromptLevel::Info, i18n::text("fix.update.latest").to_string(), detail, vec![PromptButton::ok("OK")]).await;
                }
                Found::Failed => {
                    let log = paths::log_file().map(|p| p.display().to_string()).unwrap_or_default();
                    let detail = format!("{}\n\n{}", i18n::text("fix.update.check-failed.detail"), i18n::format("fix.update.log-path", &[("log", &log)]));
                    prompt(cx, PromptLevel::Warning, i18n::text("fix.update.check-failed").to_string(), detail, vec![PromptButton::ok("OK")]).await;
                }
            }
            CHECKING.with(|c| c.set(false));
        })
        .detach();
    }
}

/// Called once the first window is open: look for an update without delaying anything. The check is made whatever the setting
/// 起動時に更新を確かめる says (the start window's 「ibisOffice を更新する」 button needs it); the setting only decides whether
/// the question 「更新しますか？」 is asked by itself.
pub(crate) fn start(cx: &mut App) {
    background_check(cx, check_enabled(), Duration::from_secs(3));
}

/// A start window was opened (at launch or by the logo): look for an update in the background, unless that was done lately.
/// Nothing is asked; the window shows 「ibisOffice を更新する」 once a newer version is known. Not in the browser.
pub(crate) fn check_for_start_window(cx: &mut App) {
    background_check(cx, false, Duration::ZERO);
}

/// Minutes during which a check that was made is not made again.
const RECHECK_MILLIS: u64 = 30 * 60 * 1000;

thread_local! {
    /// A background check is running.
    static BACKGROUND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// When the last background check was started (`clock::unix_millis`).
    static LAST_BACKGROUND: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

fn background_check(cx: &mut App, ask: bool, delay: Duration) {
    // No network in the tests.
    #[cfg(any(test, feature = "scenarios"))]
    {
        let _ = (&cx, ask, delay);
        return;
    }
    #[allow(unreachable_code)]
    {
        let now = crate::clock::unix_millis();
        // A newer version is already known; or the check was made lately, or is running (a start window opened at launch).
        if available_version().is_some() || BACKGROUND.with(|b| b.get()) {
            return;
        }
        if LAST_BACKGROUND.with(|l| l.get()).is_some_and(|last| now.saturating_sub(last) < RECHECK_MILLIS) {
            return;
        }
        BACKGROUND.with(|b| b.set(true));
        LAST_BACKGROUND.with(|l| l.set(Some(now)));
        cx.spawn(async move |cx: &mut AsyncApp| {
            if !delay.is_zero() {
                // Not while the app is starting.
                cx.background_executor().timer(delay).await;
            }
            let found = cx.background_executor().spawn(async { check() }).await;
            BACKGROUND.with(|b| b.set(false));
            if matches!(found, Found::Ready { .. } | Found::Manual { .. }) {
                AVAILABLE.with(|a| *a.borrow_mut() = Some(found.clone()));
                // The start window shows its button.
                cx.update(|cx| cx.refresh_windows());
            }
            if !ask {
                return;
            }
            match found {
                Found::Nothing | Found::Failed => {}
                Found::Manual { version } => {
                    if settings::get(NOTIFIED_KEY).as_deref() != Some(&version) {
                        settings::set(NOTIFIED_KEY, &version);
                        show_failed(cx).await;
                    }
                }
                Found::Ready { version, zip } => offer(cx, version, zip, false).await,
            }
        })
        .detach();
    }
}

/// The steps up to the downloaded zip (run off the main thread).
fn check() -> Found {
    cleanup_old_files();
    clear_attempts_if_updated();

    let url = env("IBIS_UPDATE_URL_BASE").map(|base| format!("{}/version.txt", base.trim_end_matches('/'))).unwrap_or_else(|| VERSION_URL.to_string());
    let text = match fetch_text(&url) {
        Ok(text) => text,
        Err(_) => return Found::Failed,
    };
    let Some(latest) = version::find(&text, version::PLATFORM) else {
        return Found::Failed;
    };
    if !version::is_newer(&latest, CURRENT) {
        return Found::Nothing;
    }
    let Some(log) = log_for(&latest) else { return Found::Failed };
    log.ok("check", &format!("a newer version: {latest} (current {CURRENT})"));

    if attempts(&latest) >= MAX_ATTEMPTS {
        log.info("check", &format!("{MAX_ATTEMPTS} attempts at {latest} did not end in it running: no more offers"));
        return Found::Manual { version: latest };
    }
    if let Err(reason) = installed_copy().map(|_| ()) {
        log.fail("check", &format!("this copy cannot update itself: {reason}"));
        return if reason.starts_with("a build run") { Found::Failed } else { Found::Manual { version: latest } };
    }

    match download(&log, &latest) {
        Ok(zip) => Found::Ready { version: latest, zip },
        Err(err) => {
            log.fail("download", &err);
            Found::Failed
        }
    }
}

/// Folders of finished or older updates left in the update folder (an updater on Windows cannot remove the folder it runs from).
fn cleanup_old_files() {
    let Some(root) = paths::update_root() else { return };
    let Ok(entries) = std::fs::read_dir(&root) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if version::valid(&name) && !version::is_newer(&name, CURRENT) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

// ---- Attempts ----------------------------------------------------------

/// How many times an update to `version` was started.
fn attempts(version: &str) -> u32 {
    let stored = settings::get(ATTEMPT_KEY).unwrap_or_default();
    match stored.split_once(':') {
        Some((v, n)) if v == version => n.parse().unwrap_or(0),
        _ => 0,
    }
}

fn count_attempt(version: &str) {
    settings::set(ATTEMPT_KEY, &format!("{version}:{}", attempts(version) + 1));
}

/// Running the version an attempt was for (or a newer one): that attempt worked.
fn clear_attempts_if_updated() {
    let stored = settings::get(ATTEMPT_KEY).unwrap_or_default();
    if let Some((v, _)) = stored.split_once(':')
        && !version::is_newer(v, CURRENT)
    {
        settings::set(ATTEMPT_KEY, "");
    }
}

// ---- Where this copy is installed --------------------------------------

/// The installed copy to replace: the `.app` (macOS), the folder of `ibisOffice.exe` (Windows) or the AppImage (Linux), or why it cannot be.
fn installed_copy() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let exe = crate::shell::plain_path::canonical(&exe).unwrap_or(exe);
    let text = exe.to_string_lossy().replace('\\', "/");
    if text.contains("/target/release/") || text.contains("/target/debug/") || text.contains("/target/dist/") {
        return Err("a build run from the source tree".into());
    }
    #[cfg(target_os = "macos")]
    let app = {
        if text.contains("/AppTranslocation/") {
            return Err("running from macOS's App Translocation (move ibisOffice.app to Applications and open it from there)".into());
        }
        // Contents/MacOS/ibisOffice -> the .app.
        let app = exe.parent().and_then(Path::parent).and_then(Path::parent).map(Path::to_path_buf);
        match app {
            Some(app) if app.extension().is_some_and(|e| e == "app") => app,
            _ => return Err(format!("{} is not inside an .app", exe.display())),
        }
    };
    // The AppImage its runtime started this copy from.
    #[cfg(target_os = "linux")]
    let app = match env("APPIMAGE") {
        Some(appimage) => PathBuf::from(appimage),
        None => return Err("a build run without its AppImage (APPIMAGE is not set)".into()),
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let app = exe.parent().map(Path::to_path_buf).ok_or("no folder")?;
    let parent = app.parent().ok_or("no parent folder")?;
    let probe = parent.join(format!(".ibis-update-probe-{}", std::process::id()));
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            Ok(app)
        }
        Err(err) => Err(format!("{} is not writable: {err}", parent.display())),
    }
}

// ---- Downloading -------------------------------------------------------

fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

/// Run a command, with its output as the error when it fails.
fn run(command: &mut Command) -> Result<String, String> {
    let out = command.output().map_err(|e| format!("{:?}: {e}", command.get_program()))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        return Ok(stdout);
    }
    Err(format!("{:?} {}: {} {}", command.get_program(), out.status, stdout.trim(), String::from_utf8_lossy(&out.stderr).trim()))
}

fn curl(url: &str, max_seconds: u32) -> Command {
    let mut curl = command("curl");
    curl.args(["--fail", "--silent", "--show-error", "--location", "--connect-timeout", "10", "--max-time", &max_seconds.to_string()]);
    if env("IBIS_UPDATE_URL_BASE").is_none() {
        // Only https, also after a redirect.
        curl.args(["--proto", "=https", "--proto-redir", "=https"]);
    }
    curl.arg(url);
    curl
}

fn fetch_text(url: &str) -> Result<String, String> {
    run(&mut curl(url, 20))
}

/// The file of `version` on the Releases page: a zip, or on Linux the AppImage itself.
fn zip_name(version: &str) -> String {
    let extension = if cfg!(target_os = "linux") { "AppImage" } else { "zip" };
    format!("ibisOffice-{version}-{}.{extension}", version::PLATFORM)
}

fn zip_url(version: &str) -> String {
    match env("IBIS_UPDATE_URL_BASE") {
        Some(base) => format!("{}/{}", base.trim_end_matches('/'), zip_name(version)),
        None => format!("https://github.com/ibisjp/ibis-office/releases/download/v{version}/{}", zip_name(version)),
    }
}

/// The first 64 hex digits in a `.sha256` file (`<hex>  <file name>` or just `<hex>`), lower case.
fn parse_sha256(text: &str) -> Option<String> {
    let word = text.split_whitespace().next()?.to_ascii_lowercase();
    (word.len() == 64 && word.bytes().all(|b| b.is_ascii_hexdigit())).then_some(word)
}

fn sha256_of(file: &Path) -> Result<String, String> {
    #[cfg(windows)]
    let out = run(command("certutil").args(["-hashfile"]).arg(file).arg("SHA256"))?;
    // coreutils, on every Linux (shasum is Perl's).
    #[cfg(target_os = "linux")]
    let out = run(command("sha256sum").arg(file))?;
    #[cfg(not(any(windows, target_os = "linux")))]
    let out = run(command("shasum").args(["-a", "256"]).arg(file))?;
    hash_in_output(&out).ok_or_else(|| format!("no hash in the output: {out}"))
}

/// The hash in the output of `certutil` (a line of the hash, maybe with
/// spaces between its bytes, after a heading), `sha256sum` and `shasum`
/// (the hash, then the file's name).
fn hash_in_output(out: &str) -> Option<String> {
    let is_hash = |word: &String| word.len() == 64 && word.bytes().all(|b| b.is_ascii_hexdigit());
    out.lines().find_map(|line| {
        let first = line.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();
        let joined = line.split_whitespace().collect::<String>().to_ascii_lowercase();
        [first, joined].into_iter().find(is_hash)
    })
}

/// Download `version`'s zip (Linux: AppImage) into its update folder; on Windows and Linux it must match its `.sha256`.
fn download(log: &Log, version: &str) -> Result<PathBuf, String> {
    let dir = paths::update_dir(version).ok_or("no folder for the update")?;
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let zip = dir.join(zip_name(version));
    let part = dir.join(format!("{}.part", zip_name(version)));
    let url = zip_url(version);
    log.start("download", &url);
    if let Err(err) = run(curl(&url, 1800).arg("--output").arg(&part)) {
        let _ = std::fs::remove_dir_all(&dir);
        // curl exit 22 with a 404: the version.txt was pushed before the release was made.
        let hint = if err.contains("404") { " (not published yet?)" } else { "" };
        return Err(format!("{err}{hint}"));
    }
    let size = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    log.ok("download", &format!("{size} bytes"));

    if cfg!(any(windows, target_os = "linux")) {
        log.start("verify-hash", "SHA-256 against the .sha256 file");
        let verified = (|| {
            let expected = parse_sha256(&fetch_text(&format!("{url}.sha256"))?).ok_or("the .sha256 file has no hash")?;
            let actual = sha256_of(&part)?;
            if expected == actual { Ok(()) } else { Err(format!("the download's SHA-256 is {actual}, the .sha256 file says {expected}")) }
        })();
        if let Err(err) = verified {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!("not installing: {err}"));
        }
        log.ok("verify-hash", "the hash matches");
    }
    std::fs::rename(&part, &zip).map_err(|e| format!("{}: {e}", zip.display()))?;
    Ok(zip)
}

// ---- Unpacking and checking --------------------------------------------

/// Whether the signature check may be skipped: asked for with `IBIS_UPDATE_SKIP_SIGNATURE=1` and this app is not itself ibis inc.'s signed one (so a release cannot be told to skip it).
#[cfg(target_os = "macos")]
fn signature_check_may_be_skipped() -> bool {
    if env("IBIS_UPDATE_SKIP_SIGNATURE").as_deref() != Some("1") {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    let info = run_stderr(command("codesign").args(["-dv", "--verbose=2"]).arg(exe));
    !info.contains(&format!("TeamIdentifier={TEAM_ID}"))
}

/// The error stream of a command (codesign writes its report there), whether or not it succeeds.
#[cfg(target_os = "macos")]
fn run_stderr(command: &mut Command) -> String {
    command.output().map(|o| String::from_utf8_lossy(&o.stderr).into_owned()).unwrap_or_default()
}

/// Unpacked and checked: the new copy and the updater inside it.
struct Unpacked {
    /// The new `.app` (macOS), folder (Windows) or AppImage (Linux).
    new: PathBuf,
    /// None on Linux, where ibisOffice puts the new AppImage in place itself.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    updater: PathBuf,
}

/// Check the AppImage downloaded (run off the main thread): an ELF program
/// with AppImage's mark, nothing to unpack.
#[cfg(target_os = "linux")]
fn unpack(log: &Log, _version: &str, appimage: &Path) -> Result<Unpacked, String> {
    use std::io::Read as _;
    log.start("verify-app", &appimage.display().to_string());
    let mut head = [0u8; 11];
    std::fs::File::open(appimage).and_then(|mut f| f.read_exact(&mut head)).map_err(|e| format!("{}: {e}", appimage.display()))?;
    if head[..4] != *b"\x7fELF" || head[8..11] != *b"AI\x02" {
        return Err(format!("{} is not an AppImage", appimage.display()));
    }
    log.ok("verify-app", "an AppImage");
    Ok(Unpacked { new: appimage.to_path_buf(), updater: PathBuf::new() })
}

/// Unpack `zip` and check what is in it (run off the main thread).
#[cfg(not(target_os = "linux"))]
fn unpack(log: &Log, version: &str, zip: &Path) -> Result<Unpacked, String> {
    let dir = zip.parent().ok_or("no folder")?.join("unpacked");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    log.start("unpack", &zip.display().to_string());
    #[cfg(target_os = "macos")]
    run(command("ditto").args(["-x", "-k"]).arg(zip).arg(&dir))?;
    #[cfg(not(target_os = "macos"))]
    run(command("tar").arg("-xf").arg(zip).arg("-C").arg(&dir))?;
    log.ok("unpack", &dir.display().to_string());

    #[cfg(target_os = "macos")]
    {
        let app = std::fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .ok_or("the zip has no .app")?;
        log.start("verify-app", &app.display().to_string());
        let plist = app.join("Contents/Info.plist");
        let found = run(command("/usr/libexec/PlistBuddy").args(["-c", "Print :CFBundleShortVersionString"]).arg(&plist))?;
        if found.trim() != version {
            return Err(format!("the new app is version {}, not {version}", found.trim()));
        }
        if signature_check_may_be_skipped() {
            log.info("verify-app", "the signature check is skipped (IBIS_UPDATE_SKIP_SIGNATURE, an app not signed by ibis inc.)");
        } else {
            run(command("codesign").args(["--verify", "--deep", "--strict"]).arg(&app)).map_err(|e| format!("codesign --verify: {e}"))?;
            let info = run_stderr(command("codesign").args(["-dv", "--verbose=2"]).arg(&app));
            if !info.contains(&format!("TeamIdentifier={TEAM_ID}")) {
                return Err(format!("the new app is not signed by team {TEAM_ID}"));
            }
            run(command("spctl").args(["-a", "-t", "exec"]).arg(&app)).map_err(|e| format!("spctl: {e}"))?;
        }
        let updater = app.join("Contents/MacOS/ibisOfficeUpdater");
        if !updater.is_file() {
            return Err("the new app has no ibisOfficeUpdater".into());
        }
        log.ok("verify-app", &format!("version {version}"));
        Ok(Unpacked { new: app, updater })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = version;
        // The zip holds one folder, ibisOffice\ (ibisOffice.exe, ibisOfficeUpdater.exe, Resources\).
        let folder = std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).find(|p| p.join("ibisOffice.exe").is_file()).ok_or("the zip has no folder with ibisOffice.exe")?;
        let updater = folder.join("ibisOfficeUpdater.exe");
        if !updater.is_file() {
            return Err("the new folder has no ibisOfficeUpdater.exe".into());
        }
        log.ok("verify-app", &folder.display().to_string());
        Ok(Unpacked { new: folder, updater })
    }
}

// ---- Asking, then handing over -----------------------------------------

/// Whether any window has edits not yet saved (the same sign the close confirmation uses).
fn any_unsaved(cx: &mut App) -> bool {
    use crate::shell::editor::SlideEditor;
    let mut unsaved = false;
    for window in cx.windows() {
        if let Some(root) = window.downcast::<gpui_kit::component::Root>() {
            let _ = root.update(cx, |root, _, app| {
                if let Ok(editor) = root.view().clone().downcast::<SlideEditor>() {
                    let editor = editor.read(app);
                    unsaved |= editor.has_document && editor.document.can_undo();
                }
            });
        }
    }
    unsaved
}

/// Show a prompt over the active window; the index of the button chosen.
async fn prompt(cx: &mut AsyncApp, level: PromptLevel, message: String, detail: String, buttons: Vec<PromptButton>) -> Option<usize> {
    let receiver = cx.update(|cx| {
        let window = cx.active_window().or_else(|| cx.windows().first().copied())?;
        window.update(cx, |_, window, cx| window.prompt(level, &message, Some(&detail), &buttons, cx)).ok()
    })?;
    receiver.await.ok()
}

async fn show_failed(cx: &mut AsyncApp) {
    let log = paths::log_file().map(|p| p.display().to_string()).unwrap_or_default();
    let detail = format!("{}\n\n{}", i18n::format("fix.update.failed.detail", &[("url", &RELEASES_URL)]), i18n::format("fix.update.log-path", &[("log", &log)]));
    let buttons = vec![PromptButton::ok(i18n::text("fix.update.open-site")), PromptButton::cancel("OK")];
    if prompt(cx, PromptLevel::Warning, i18n::text("fix.update.failed").to_string(), detail, buttons).await == Some(0) {
        cx.update(|cx| cx.open_url(RELEASES_URL));
    }
}

async fn offer(cx: &mut AsyncApp, version: String, zip: PathBuf, asked_by_user: bool) {
    // Not while there is work not saved: the app quits when the update starts. (Not at the user's own click: the question is
    // shown at once, and saying はい with work unsaved is turned away below.)
    while !asked_by_user && cx.update(|cx| any_unsaved(cx)) {
        cx.background_executor().timer(Duration::from_secs(30)).await;
    }
    let answer = if env("IBIS_UPDATE_AUTO_YES").as_deref() == Some("1") {
        Some(0)
    } else {
        let buttons = vec![PromptButton::ok(i18n::text("fix.update.yes")), PromptButton::cancel(i18n::text("fix.update.no"))];
        let detail = i18n::format("fix.update.found.detail", &[("version", &version), ("current", &CURRENT)]);
        prompt(cx, PromptLevel::Info, i18n::text("fix.update.found").to_string(), detail, buttons).await
    };
    if answer != Some(0) {
        if let Some(log) = log_for(&version) {
            log.info("offer", "the user chose not to update now");
        }
        return;
    }
    if cx.update(|cx| any_unsaved(cx)) {
        // Edited while the question was open: ask again next time.
        if let Some(log) = log_for(&version) {
            log.info("offer", "there is unsaved work: not updating now");
        }
        return;
    }
    let zip_for_task = zip.clone();
    let version_for_task = version.clone();
    let unpacked = cx
        .background_executor()
        .spawn(async move {
            let log = log_for(&version_for_task).ok_or("no log")?;
            unpack(&log, &version_for_task, &zip_for_task).inspect_err(|err| log.fail("unpack", err))
        })
        .await;
    let Ok(unpacked) = unpacked else {
        // Counts as an attempt: a release that cannot be unpacked or checked must not be offered at every start.
        count_attempt(&version);
        show_failed(cx).await;
        return;
    };
    let version_for_start = version.clone();
    let started = cx.update(|cx| {
        let log = log_for(&version_for_start)?;
        match start_updater(&log, &version_for_start, &unpacked) {
            Ok(()) => {
                // The updater waits for this process to end.
                cx.quit();
                Some(())
            }
            Err(err) => {
                log.fail("start-updater", &err);
                None
            }
        }
    });
    if started.is_none() {
        show_failed(cx).await;
    }
}

/// Put the new AppImage in the place of the one running and start it (Linux):
/// copied beside it, made executable, and renamed over it, so the AppImage is
/// at every moment either the old one or the new one whole.
#[cfg(target_os = "linux")]
fn start_updater(log: &Log, version: &str, unpacked: &Unpacked) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    let old = installed_copy()?;
    count_attempt(version);
    let folder = old.parent().ok_or("no folder")?;
    let partial = folder.join(format!(".ibisOffice-{version}.partial-{}", std::process::id()));
    log.start("replace", &format!("{} -> {}", unpacked.new.display(), old.display()));
    let replaced = (|| {
        std::fs::copy(&unpacked.new, &partial).map_err(|e| format!("{}: {e}", partial.display()))?;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("{}: {e}", partial.display()))?;
        std::fs::rename(&partial, &old).map_err(|e| format!("{}: {e}", old.display()))
    })();
    if let Err(err) = replaced {
        let _ = std::fs::remove_file(&partial);
        return Err(err);
    }
    log.ok("replace", "the new AppImage is in place");
    let mut app = command(&old.to_string_lossy());
    app.current_dir(std::env::temp_dir()).stdout(Stdio::null()).stderr(Stdio::null());
    // Not the variables this copy's runtime set: the new runtime sets its own.
    for name in ["APPIMAGE", "APPDIR", "ARGV0", "OWD"] {
        app.env_remove(name);
    }
    app.spawn().map_err(|e| format!("{}: {e}", old.display()))?;
    if let Some(dir) = paths::update_dir(version) {
        let _ = std::fs::remove_dir_all(dir);
    }
    log.ok("start", "the new version started; ibisOffice quits now");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn start_updater(log: &Log, version: &str, unpacked: &Unpacked) -> Result<(), String> {
    let old = installed_copy()?;
    let workdir = paths::update_dir(version).ok_or("no update folder")?;
    count_attempt(version);
    log.start("start-updater", &format!("{} for {} -> {}", unpacked.updater.display(), unpacked.new.display(), old.display()));
    let mut updater = command(&unpacked.updater.to_string_lossy());
    updater
        .args(["--pid", &std::process::id().to_string(), "--version", version, "--old"])
        .arg(&old)
        .arg("--new")
        .arg(&unpacked.new)
        .arg("--log")
        .arg(log.path())
        .arg("--workdir")
        .arg(&workdir)
        // Not inside the folder being replaced (Windows cannot rename a folder a process is in) or the one being removed.
        .current_dir(std::env::temp_dir())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    updater.spawn().map_err(|e| format!("{}: {e}", unpacked.updater.display()))?;
    log.ok("start-updater", "started; ibisOffice quits now");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sha256_file_names_the_hash_first() {
        let hash = "A".repeat(64);
        assert_eq!(parse_sha256(&format!("{hash}  ibisOffice-0.9.0-Windows.zip\n")), Some("a".repeat(64)));
        assert_eq!(parse_sha256(&hash), Some("a".repeat(64)));
        assert_eq!(parse_sha256("not a hash"), None);
        assert_eq!(parse_sha256(""), None);
    }

    #[test]
    fn the_hash_is_found_in_each_tool_s_output() {
        let hash = "ab".repeat(32);
        let spaced = hash.as_bytes().chunks(2).map(|b| std::str::from_utf8(b).unwrap()).collect::<Vec<_>>().join(" ");
        let certutil = format!("SHA256 hash of file x.zip:\r\n{spaced}\r\nCertUtil: -hashfile command completed successfully.\r\n");
        assert_eq!(hash_in_output(&certutil), Some(hash.clone()));
        assert_eq!(hash_in_output(&format!("{hash}  /tmp/a b/ibisOffice-0.9.0-Linux.AppImage.part\n")), Some(hash.clone()));
        assert_eq!(hash_in_output("nothing here\n"), None);
    }

    #[test]
    fn the_zip_is_named_for_the_version_and_platform() {
        let name = zip_name("0.9.0");
        assert!(name == "ibisOffice-0.9.0-macOS.zip" || name == "ibisOffice-0.9.0-Windows.zip" || name == "ibisOffice-0.9.0-Linux.AppImage");
    }
}
