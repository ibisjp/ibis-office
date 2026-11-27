// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! One ibisOffice process to a user (GitHub issue #239). Windows and Linux start a new process for each file opened from the app's icon, the
//! file association or the command line, and a sheet cannot be copied or moved to a workbook open in another process. macOS hands a second start to
//! the running process by itself (Launch Services, `on_open_urls`); here the second start of the other systems does the same: it asks the running
//! process to open the files (or a new window) and ends.
//!
//! The way is one small line protocol over a per-user local endpoint, made with the standard library only:
//!
//! * Linux (X11 and Wayland alike): a Unix domain socket `instance.sock` in `$XDG_RUNTIME_DIR` (the user's own directory, mode 0700), else in
//!   `~/.cache/ibisOffice`.
//! * Windows: the way of Windows programs (#239): a named mutex tells the first process from a second, the first has a hidden message-only window, and the second finds it
//!   with `FindWindowEx` and sends the same lines in a `WM_COPYDATA` message (no port, no secret).
//!
//! A request is lines, ended by `END`: `OPEN <absolute path>` or `NEW` (a new window). The running process answers `OK`. A second start that finds no
//! answer (no endpoint, a socket nobody listens on after a crash, a process that hangs) starts as the one process and makes the endpoint again.

use std::io::BufRead;
#[cfg(not(windows))]
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What a second start asks the running process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Request {
    /// Open the file (in a window, or its window comes to the front if it is open).
    Open(PathBuf),
    /// A new, empty window.
    NewWindow,
}

/// How long a second start waits for the running process to answer before it takes it for dead.
const ANSWER_WAIT: Duration = Duration::from_millis(1500);

/// The directory that holds the endpoint of this user.
pub(crate) fn endpoint_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("ibisOffice"))
    }
    #[cfg(not(windows))]
    {
        match std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
            Some(dir) => Some(PathBuf::from(dir)),
            None => std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache").join("ibisOffice")),
        }
    }
}

fn encode(requests: &[Request]) -> String {
    let mut text = String::new();
    for request in requests {
        match request {
            Request::Open(path) => {
                // (A line break in a file name would end the line; the path is sent with the break escaped.)
                text.push_str("OPEN ");
                text.push_str(&path.to_string_lossy().replace('\\', "\\\\").replace('\n', "\\n"));
            }
            Request::NewWindow => text.push_str("NEW"),
        }
        text.push('\n');
    }
    text.push_str("END\n");
    text
}

fn decode_path(text: &str) -> PathBuf {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    PathBuf::from(out)
}

/// Read one request from `reader` (lines up to `END`).
fn read_request(reader: impl BufRead, token: Option<&str>) -> Option<Vec<Request>> {
    let mut requests = Vec::new();
    let mut lines = reader.lines();
    if let Some(token) = token {
        let first = lines.next()?.ok()?;
        if first.strip_prefix("TOKEN ")? != token {
            return None;
        }
    }
    for line in lines {
        let line = line.ok()?;
        if line == "END" {
            return Some(requests);
        } else if line == "NEW" {
            requests.push(Request::NewWindow);
        } else if let Some(path) = line.strip_prefix("OPEN ") {
            requests.push(Request::Open(decode_path(path)));
        } else {
            return None;
        }
    }
    None
}

#[cfg(not(windows))]
mod endpoint {
    use super::*;
    use std::os::unix::net::{UnixListener, UnixStream};

    fn socket_path(dir: &Path) -> PathBuf {
        dir.join("instance.sock")
    }

    /// Send the requests to the running process; `true` when it answered.
    pub(super) fn send(dir: &Path, requests: &[Request]) -> bool {
        let Ok(mut stream) = UnixStream::connect(socket_path(dir)) else { return false };
        stream.set_read_timeout(Some(ANSWER_WAIT)).ok();
        stream.set_write_timeout(Some(ANSWER_WAIT)).ok();
        if stream.write_all(encode(requests).as_bytes()).is_err() {
            return false;
        }
        let mut answer = String::new();
        BufReader::new(stream).read_line(&mut answer).is_ok() && answer.trim() == "OK"
    }

    /// Make the endpoint, and give each request that comes to `deliver` (on a thread of its own).
    pub(super) fn serve(dir: &Path, deliver: impl Fn(Vec<Request>) + Send + 'static) -> std::io::Result<()> {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        let path = socket_path(dir);
        // A socket left by a process that died: nobody answers on it; take it away and make it again.
        if path.exists() {
            if UnixStream::connect(&path).is_ok() {
                return Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "another ibisOffice is running"));
            }
            std::fs::remove_file(&path).ok();
        }
        let listener = UnixListener::bind(&path)?;
        std::thread::Builder::new().name("ibisOffice-instance".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                stream.set_read_timeout(Some(ANSWER_WAIT)).ok();
                let mut reply = match stream.try_clone() {
                    Ok(reply) => reply,
                    Err(_) => continue,
                };
                if let Some(requests) = read_request(BufReader::new(stream), None) {
                    deliver(requests);
                    reply.write_all(b"OK\n").ok();
                }
            }
        })?;
        Ok(())
    }

    /// Take the endpoint away (the process ends).
    pub(super) fn remove(dir: &Path) {
        std::fs::remove_file(socket_path(dir)).ok();
    }
}

#[cfg(windows)]
mod endpoint {
    //! The way of Windows programs (#239, the user: "FindWindow"): a named mutex tells the first process from a second; the first makes a hidden window that only receives messages
    //! (`HWND_MESSAGE`, of a window class of its own); the second finds it with `FindWindowEx` and hands over what it was asked with `WM_COPYDATA`, the same lines as the other
    //! systems' sockets carry (`encode`, `read_request`). No port, no file with a secret: a message window is reached only by the processes of the same session and the same
    //! integrity level.
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::DataExchange::COPYDATASTRUCT;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::{
        AllowSetForegroundWindow, CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowExW, GetMessageW, GetWindowThreadProcessId, HWND_MESSAGE, MSG, RegisterClassW, SMTO_ABORTIFHUNG,
        SendMessageTimeoutW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COPYDATA, WNDCLASSW,
    };
    use windows::core::PCWSTR;

    /// What `COPYDATASTRUCT::dwData` says of a message of ours (another program's `WM_COPYDATA` is not read).
    const MAGIC: usize = 0x4942_4953; // "IBIS"

    type Deliver = Box<dyn Fn(Vec<Request>) + Send>;

    /// The handlers of the message windows of this process, by window (a test makes several).
    fn handlers() -> &'static Mutex<HashMap<isize, Deliver>> {
        static HANDLERS: OnceLock<Mutex<HashMap<isize, Deliver>>> = OnceLock::new();
        HANDLERS.get_or_init(Default::default)
    }

    /// The names of this user's endpoint, from the directory it is for (the same for the processes of one user; another directory, as a test's, has its own).
    fn key(dir: &Path) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        dir.to_string_lossy().to_lowercase().hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn class_name(dir: &Path) -> Vec<u16> {
        wide(&format!("ibisOffice.SingleInstance.{}", key(dir)))
    }

    extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if message == WM_COPYDATA {
            // SAFETY: for `WM_COPYDATA` the system gives in `lparam` a `COPYDATASTRUCT` that lives while the message is handled, whose `lpData` holds `cbData` bytes.
            let requests = unsafe {
                let data = &*(lparam.0 as *const COPYDATASTRUCT);
                (data.dwData == MAGIC && !data.lpData.is_null()).then(|| std::slice::from_raw_parts(data.lpData as *const u8, data.cbData as usize))
            }
            .and_then(|bytes| read_request(std::io::Cursor::new(bytes), None));
            return match requests {
                Some(requests) => {
                    if let Some(deliver) = handlers().lock().unwrap_or_else(|e| e.into_inner()).get(&(window.0 as isize)) {
                        deliver(requests);
                    }
                    LRESULT(1)
                }
                None => LRESULT(0),
            };
        }
        // SAFETY: the default handling of a message of a window of this process.
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }

    fn find(dir: &Path) -> Option<HWND> {
        let class = class_name(dir);
        // SAFETY: `class` is a NUL-terminated wide string that outlives the call.
        unsafe { FindWindowExW(Some(HWND_MESSAGE), None, PCWSTR(class.as_ptr()), PCWSTR::null()).ok() }
    }

    /// Hand the requests to the running process's message window; `true` when it answered.
    pub(super) fn send(dir: &Path, requests: &[Request]) -> bool {
        let Some(window) = find(dir) else { return false };
        let text = encode(requests);
        let data = COPYDATASTRUCT { dwData: MAGIC, cbData: text.len() as u32, lpData: text.as_ptr() as *mut _ };
        let mut answer = 0usize;
        // SAFETY: `data` and `text` live through the call, which waits for the answer (or `ANSWER_WAIT`); a hung process is not waited for.
        unsafe {
            // The process that opens the files may come to the front (the second start has the right to give: it was just started by the user).
            let mut process = 0u32;
            GetWindowThreadProcessId(window, Some(&mut process));
            let _ = AllowSetForegroundWindow(process);
            SendMessageTimeoutW(window, WM_COPYDATA, WPARAM(0), LPARAM(&data as *const _ as isize), SMTO_ABORTIFHUNG, ANSWER_WAIT.as_millis() as u32, Some(&mut answer));
        }
        answer == 1
    }

    pub(super) fn serve(dir: &Path, deliver: impl Fn(Vec<Request>) + Send + 'static) -> std::io::Result<()> {
        let other = |what: &str| std::io::Error::other(what.to_string());
        let name = wide(&format!("Local\\ibisOffice.SingleInstance.{}", key(dir)));
        // SAFETY: `name` is a NUL-terminated wide string; the handle is kept to the end of the process (the mutex is what says "I am the first").
        let first = unsafe {
            let mutex = CreateMutexW(None, true, PCWSTR(name.as_ptr())).map_err(|e| other(&e.to_string()))?;
            let exists = GetLastError() == ERROR_ALREADY_EXISTS;
            if exists {
                let _ = windows::Win32::Foundation::CloseHandle(mutex);
            }
            !exists
        };
        if !first {
            return Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "another ibisOffice is running"));
        }
        let class = class_name(dir);
        let (ready, started) = std::sync::mpsc::channel::<Result<isize, String>>();
        std::thread::Builder::new().name("ibisOffice-instance".into()).spawn(move || {
            // SAFETY: a window class and a message-only window of this thread, which then runs the message loop for the life of the process.
            unsafe {
                let instance = match GetModuleHandleW(None) {
                    Ok(instance) => instance,
                    Err(e) => return drop(ready.send(Err(e.to_string()))),
                };
                let class_info = WNDCLASSW { lpfnWndProc: Some(window_proc), hInstance: instance.into(), lpszClassName: PCWSTR(class.as_ptr()), ..Default::default() };
                RegisterClassW(&class_info);
                let window = match CreateWindowExW(WINDOW_EX_STYLE(0), PCWSTR(class.as_ptr()), PCWSTR::null(), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(instance.into()), None) {
                    Ok(window) => window,
                    Err(e) => return drop(ready.send(Err(e.to_string()))),
                };
                handlers().lock().unwrap_or_else(|e| e.into_inner()).insert(window.0 as isize, Box::new(deliver));
                ready.send(Ok(window.0 as isize)).ok();
                let mut message = MSG::default();
                while GetMessageW(&mut message, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        })?;
        match started.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(why)) => Err(other(&why)),
            Err(_) => Err(other("the window of the instance did not start")),
        }
    }

    /// The process ends with its window and its mutex: nothing is left to take away.
    pub(super) fn remove(_dir: &Path) {}
}

/// Ask the running ibisOffice of this user (if there is one) to do `requests`; `true` when it did and this process can end.
pub(crate) fn hand_over(requests: &[Request]) -> bool {
    endpoint_dir().is_some_and(|dir| hand_over_at(&dir, requests))
}

pub(crate) fn hand_over_at(dir: &Path, requests: &[Request]) -> bool {
    endpoint::send(dir, requests)
}

/// Become the process that second starts are handed to: `deliver` gets each request (on another thread). Fails when another process is running.
pub(crate) fn serve(deliver: impl Fn(Vec<Request>) + Send + 'static) -> std::io::Result<()> {
    let dir = endpoint_dir().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no directory for the endpoint"))?;
    serve_at(&dir, deliver)
}

pub(crate) fn serve_at(dir: &Path, deliver: impl Fn(Vec<Request>) + Send + 'static) -> std::io::Result<()> {
    endpoint::serve(dir, deliver)
}

/// Take the endpoint away at the end of the process (a crash leaves it; the next start cleans it).
pub(crate) fn stop() {
    if let Some(dir) = endpoint_dir() {
        endpoint::remove(&dir);
    }
}

/// The requests a start with these command-line arguments makes: the file named (made absolute here, the running process has another current directory),
/// or a new window.
pub(crate) fn requests_for(path: Option<&Path>) -> Vec<Request> {
    match path {
        Some(path) => vec![Request::Open(std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()))],
        None => vec![Request::NewWindow],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("si{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    #[test]
    fn nobody_listens_so_the_start_is_alone() {
        let d = dir("alone");
        assert!(!hand_over_at(&d, &[Request::NewWindow]));
    }

    #[test]
    fn a_second_start_hands_its_file_and_a_new_window_to_the_first() {
        let d = dir("hand");
        let (sender, receiver) = mpsc::channel();
        let sender = std::sync::Mutex::new(sender);
        serve_at(&d, move |requests| {
            sender.lock().unwrap().send(requests).ok();
        })
        .unwrap();
        let sent = vec![Request::Open(PathBuf::from("/tmp/a b\\c\nd.xlsx")), Request::NewWindow];
        assert!(hand_over_at(&d, &sent));
        assert_eq!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), sent);
        // A second one asks again, and a third process cannot make the endpoint while the first one answers.
        assert!(hand_over_at(&d, &[Request::NewWindow]));
        assert_eq!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), vec![Request::NewWindow]);
        assert!(serve_at(&d, |_| {}).is_err());
        std::fs::remove_dir_all(&d).ok();
    }

    #[cfg(unix)]
    #[test]
    fn a_broken_socket_is_cleaned_and_made_again() {
        let d = dir("stale");
        // A socket a dead process left: the file is there, nobody listens.
        {
            std::fs::create_dir_all(&d).unwrap();
            let listener = std::os::unix::net::UnixListener::bind(d.join("instance.sock")).unwrap();
            drop(listener);
        }
        assert!(d.join("instance.sock").exists());
        assert!(!hand_over_at(&d, &[Request::NewWindow]));
        let (sender, receiver) = mpsc::channel();
        let sender = std::sync::Mutex::new(sender);
        serve_at(&d, move |requests| {
            sender.lock().unwrap().send(requests).ok();
        })
        .unwrap();
        assert!(hand_over_at(&d, &[Request::NewWindow]));
        assert_eq!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), vec![Request::NewWindow]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_process_that_does_not_answer_is_taken_for_dead() {
        // Something listens but never answers: the second start waits `ANSWER_WAIT`, then starts alone.
        let d = dir("silent");
        #[cfg(unix)]
        {
            std::fs::create_dir_all(&d).unwrap();
            let listener = std::os::unix::net::UnixListener::bind(d.join("instance.sock")).unwrap();
            let handle = std::thread::spawn(move || {
                let kept: Vec<_> = listener.incoming().take(1).flatten().collect();
                std::thread::sleep(Duration::from_secs(3));
                drop(kept);
            });
            assert!(!hand_over_at(&d, &[Request::NewWindow]));
            handle.join().ok();
        }
        std::fs::remove_dir_all(&d).ok();
    }
}
