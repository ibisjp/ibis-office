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
//! * Windows: a TCP port on the loopback address, written with a token to `instance.port` in `%LOCALAPPDATA%\ibisOffice` (a named pipe needs a crate or
//!   unsafe code; the token keeps other users' programs, and web pages, from sending requests).
//!
//! A request is lines, ended by `END`: `OPEN <absolute path>` or `NEW` (a new window). The running process answers `OK`. A second start that finds no
//! answer (no endpoint, a socket nobody listens on after a crash, a process that hangs) starts as the one process and makes the endpoint again.

use std::io::{BufRead, BufReader, Write};
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
    use super::*;
    use std::net::{Ipv4Addr, TcpListener, TcpStream};

    fn port_file(dir: &Path) -> PathBuf {
        dir.join("instance.port")
    }

    fn read_port(dir: &Path) -> Option<(u16, String)> {
        let text = std::fs::read_to_string(port_file(dir)).ok()?;
        let mut parts = text.split_whitespace();
        Some((parts.next()?.parse().ok()?, parts.next()?.to_string()))
    }

    pub(super) fn send(dir: &Path, requests: &[Request]) -> bool {
        let Some((port, token)) = read_port(dir) else { return false };
        let Ok(mut stream) = TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), ANSWER_WAIT) else { return false };
        stream.set_read_timeout(Some(ANSWER_WAIT)).ok();
        stream.set_write_timeout(Some(ANSWER_WAIT)).ok();
        if stream.write_all(format!("TOKEN {token}\n{}", encode(requests)).as_bytes()).is_err() {
            return false;
        }
        let mut answer = String::new();
        BufReader::new(stream).read_line(&mut answer).is_ok() && answer.trim() == "OK"
    }

    pub(super) fn serve(dir: &Path, deliver: impl Fn(Vec<Request>) + Send + 'static) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        // A port file left by a process that died: nobody answers on it.
        if let Some((port, _)) = read_port(dir)
            && TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), ANSWER_WAIT).is_ok()
        {
            return Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "another ibisOffice is running"));
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        // A token nobody can guess: the hash of the process and two random seeds of the standard library.
        let token = {
            use std::hash::{BuildHasher, Hasher};
            let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
            hasher.write_u32(std::process::id());
            format!("{:016x}{:016x}", hasher.finish(), std::collections::hash_map::RandomState::new().build_hasher().finish())
        };
        std::fs::write(port_file(dir), format!("{port} {token}"))?;
        std::thread::Builder::new().name("ibisOffice-instance".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                stream.set_read_timeout(Some(ANSWER_WAIT)).ok();
                let mut reply = match stream.try_clone() {
                    Ok(reply) => reply,
                    Err(_) => continue,
                };
                if let Some(requests) = read_request(BufReader::new(stream), Some(&token)) {
                    deliver(requests);
                    reply.write_all(b"OK\n").ok();
                }
            }
        })?;
        Ok(())
    }

    pub(super) fn remove(dir: &Path) {
        std::fs::remove_file(port_file(dir)).ok();
    }
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
