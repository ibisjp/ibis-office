// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The HTTP transport (GitHub issue #231, `tasks/doc/design/231-openai-mcp.md` 3.1, 3.4 and 10): MCP's Streamable HTTP in its smallest form, for ChatGPT's
//! Secure MCP Tunnel (OpenAI's `tunnel-client` runs on this PC and calls `http://127.0.0.1:<port>/mcp`).
//!
//! * `POST /mcp` takes one JSON-RPC message and answers with one JSON (`200`, `application/json`); a notification is `202` with no body. No SSE stream, no
//!   session, no old HTTP+SSE: nothing is sent to the client but answers. Every other path is `404`, every other method `405`.
//! * It listens on `127.0.0.1` only, and no TLS (the tunnel carries the HTTPS). There is no authentication: what reaches it is the tunnel client and whatever else
//!   runs on this PC as this user, so the `Host` is checked (`127.0.0.1` or `localhost` only: a page of another name that was pointed at this address by DNS,
//!   or a general tunnel, is refused) and so is the `Origin` (absent, or a loopback page's, or one the user allowed with `--allow-origin`).
//! * Limits: the body (10 MiB, as the tunnel's), the requests a minute (600, as the tunnel's), the requests at once, and the time one waits for an answer (120 s).
//! * The tools are those of the standard input and output (reads and writes, the same folders and rules). An `initialize` of a version the server does not
//!   speak is an error.

use std::collections::VecDeque;
use std::io::Read;
use std::net::SocketAddr;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ibis_doc_service::Service;
use serde_json::Value;
use tiny_http::{Header, Method, Request, Response, Server as Listener, StatusCode};

use crate::protocol::Server;

/// The path the server answers on.
const PATH: &str = "/mcp";

/// What the HTTP server is told.
#[derive(Clone, Debug)]
pub struct HttpConfig {
    /// The port on `127.0.0.1` (0: any free one; the command line does not allow it).
    pub port: u16,
    /// Origins besides the loopback pages' that may call (`https://example.org`, no path).
    pub allowed_origins: Vec<String>,
    /// The biggest body taken, in bytes.
    pub max_body: usize,
    /// The requests taken in a minute.
    pub max_per_minute: usize,
    /// The requests that are being served at once.
    pub max_in_flight: usize,
    /// How long a request waits for its answer.
    pub timeout: Duration,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self { port: 0, allowed_origins: Vec::new(), max_body: 10 * 1024 * 1024, max_per_minute: 600, max_in_flight: 8, timeout: Duration::from_secs(120) }
    }
}

/// Why a request is refused: the status and a short reason.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub status: u16,
    pub reason: &'static str,
}

fn refuse(status: u16, reason: &'static str) -> Result<(), Refusal> {
    Err(Refusal { status, reason })
}

/// The host part of a `Host` header or an origin's authority (`localhost:8080` is `localhost`); IPv6 forms are not loopback names here.
fn host_of(authority: &str) -> &str {
    authority.rsplit_once(':').filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit())).map_or(authority, |(host, _)| host)
}

fn is_loopback_name(host: &str) -> bool {
    host.eq_ignore_ascii_case("127.0.0.1") || host.eq_ignore_ascii_case("localhost")
}

/// The checks that are made of a request before its body is read (a pure function: the tests call it).
pub(crate) fn check(config: &HttpConfig, method: &str, path: &str, host: Option<&str>, origin: Option<&str>, content_type: Option<&str>, content_length: Option<usize>) -> Result<(), Refusal> {
    // The Host first: whatever the request is, one that was not made to this address is not answered.
    match host {
        Some(host) if is_loopback_name(host_of(host)) => {}
        _ => return refuse(403, "the Host is not 127.0.0.1 or localhost"),
    }
    if let Some(origin) = origin {
        let loopback = origin.split_once("://").is_some_and(|(_, authority)| is_loopback_name(host_of(authority.trim_end_matches('/'))));
        if !loopback && !config.allowed_origins.iter().any(|allowed| allowed.trim_end_matches('/') == origin.trim_end_matches('/')) {
            return refuse(403, "the Origin is not allowed");
        }
    }
    if path.split('?').next() != Some(PATH) {
        return refuse(404, "not found");
    }
    if !method.eq_ignore_ascii_case("POST") {
        return refuse(405, "only POST");
    }
    if !content_type.is_some_and(|t| t.trim().to_ascii_lowercase().starts_with("application/json")) {
        return refuse(415, "the body is application/json");
    }
    if content_length.is_some_and(|n| n > config.max_body) {
        return refuse(413, "the body is too large");
    }
    Ok(())
}

/// The requests of the last minute (a sliding window) and those being served.
struct Load {
    taken: Mutex<VecDeque<Instant>>,
    in_flight: Mutex<usize>,
}

impl Load {
    /// Whether one more request may be taken now (it is counted when it may).
    fn admit(&self, config: &HttpConfig, now: Instant) -> bool {
        let mut taken = self.taken.lock().unwrap_or_else(|e| e.into_inner());
        while taken.front().is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(60)) {
            taken.pop_front();
        }
        let mut in_flight = self.in_flight.lock().unwrap_or_else(|e| e.into_inner());
        if taken.len() >= config.max_per_minute || *in_flight >= config.max_in_flight {
            return false;
        }
        taken.push_back(now);
        *in_flight += 1;
        true
    }

    fn done(&self) {
        let mut in_flight = self.in_flight.lock().unwrap_or_else(|e| e.into_inner());
        *in_flight = in_flight.saturating_sub(1);
    }
}

/// Ends the server's loop (see [`HttpServer::stopper`]).
pub struct Stopper(Arc<Listener>);

impl Stopper {
    pub fn stop(&self) {
        self.0.unblock();
    }
}

/// A listening HTTP server of the MCP; [`HttpServer::run`] serves until [`HttpServer::stopper`].
pub struct HttpServer {
    listener: Arc<Listener>,
    config: HttpConfig,
    port: u16,
    work: Sender<(String, Sender<Option<String>>)>,
}

impl HttpServer {
    /// Listen on `127.0.0.1:<port>`; the service is moved into a thread of its own that answers the messages one at a time.
    pub fn bind(service: Service, config: HttpConfig) -> std::io::Result<Self> {
        let listener = Listener::http(SocketAddr::from(([127, 0, 0, 1], config.port))).map_err(|e| std::io::Error::other(e.to_string()))?;
        let port = listener.server_addr().to_ip().map_or(config.port, |a| a.port());
        let (work, jobs) = mpsc::channel::<(String, Sender<Option<String>>)>();
        std::thread::Builder::new().name("mcp-http-service".into()).spawn(move || {
            let mut server = Server::new(service);
            server.set_strict_versions(true);
            while let Ok((body, answer)) = jobs.recv() {
                let _ = answer.send(server.handle(&body));
            }
        })?;
        Ok(Self { listener: Arc::new(listener), config, port, work })
    }

    /// The port that is listened on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// A handle whose `stop()` makes [`HttpServer::run`] return.
    pub fn stopper(&self) -> Stopper {
        Stopper(self.listener.clone())
    }

    /// Serve requests until stopped.
    pub fn run(&self) {
        let load = Arc::new(Load { taken: Mutex::new(VecDeque::new()), in_flight: Mutex::new(0) });
        for request in self.listener.incoming_requests() {
            let (config, work, load) = (self.config.clone(), self.work.clone(), load.clone());
            if !load.admit(&self.config, Instant::now()) {
                respond(request, 429, "too many requests", None);
                continue;
            }
            // A thread for each request, so that one slow body does not hold the others (at most `max_in_flight` of them).
            let spawned = std::thread::Builder::new().name("mcp-http-request".into()).spawn({
                let load = load.clone();
                move || {
                    serve_request(&config, &work, request);
                    load.done();
                }
            });
            if spawned.is_err() {
                load.done();
            }
        }
    }
}

fn header_value(request: &Request, name: &str) -> Option<String> {
    request.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str().to_string())
}

/// Answer with a short plain reason (or the JSON `body`).
fn respond(request: Request, status: u16, reason: &str, body: Option<String>) {
    let json = body.is_some();
    let mut response = Response::from_string(body.unwrap_or_else(|| reason.to_string())).with_status_code(StatusCode(status));
    let content_type = if json { "application/json" } else { "text/plain; charset=utf-8" };
    for (name, value) in [("Content-Type", content_type), ("Cache-Control", "no-store"), ("X-Content-Type-Options", "nosniff")] {
        if let Ok(h) = Header::from_bytes(name.as_bytes(), value.as_bytes()) {
            response = response.with_header(h);
        }
    }
    if status == 405 && let Ok(h) = Header::from_bytes(&b"Allow"[..], &b"POST"[..]) {
        response = response.with_header(h);
    }
    let _ = request.respond(response);
}

fn serve_request(config: &HttpConfig, work: &Sender<(String, Sender<Option<String>>)>, mut request: Request) {
    let method = if *request.method() == Method::Post { "POST" } else { "OTHER" };
    let length = request.body_length();
    if let Err(refusal) = check(config, method, request.url(), header_value(&request, "Host").as_deref(), header_value(&request, "Origin").as_deref(), header_value(&request, "Content-Type").as_deref(), length) {
        return respond(request, refusal.status, refusal.reason, None);
    }
    // The length may be unknown (chunked) or a lie: never read more than the limit allows.
    let mut body = Vec::new();
    if request.as_reader().take(config.max_body as u64 + 1).read_to_end(&mut body).is_err() {
        return respond(request, 400, "the body could not be read", None);
    }
    if body.len() > config.max_body {
        return respond(request, 413, "the body is too large", None);
    }
    let Ok(text) = String::from_utf8(body) else {
        return respond(request, 400, "the body is not UTF-8", None);
    };
    let (answer, wait) = mpsc::channel();
    if work.send((text, answer)).is_err() {
        return respond(request, 500, "the server is stopping", None);
    }
    match wait.recv_timeout(config.timeout) {
        Ok(Some(body)) => {
            debug_assert!(serde_json::from_str::<Value>(&body).is_ok());
            respond(request, 200, "", Some(body))
        }
        Ok(None) => {
            let _ = request.respond(Response::empty(StatusCode(202)));
        }
        Err(_) => respond(request, 504, "no answer in time", None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(host: &str, origin: Option<&str>) -> Result<(), Refusal> {
        check(&HttpConfig::default(), "POST", "/mcp", Some(host), origin, Some("application/json"), Some(10))
    }

    #[test]
    fn only_the_loopback_names_are_taken_as_the_host() {
        for host in ["127.0.0.1", "127.0.0.1:8080", "localhost", "LocalHost:3000"] {
            assert_eq!(ok(host, None), Ok(()), "{host}");
        }
        for host in ["evil.example", "evil.example:80", "127.0.0.1.evil.example", "localhost.evil.example:1", "0.0.0.0:80", "[::1]:80", "192.168.1.5", "abc.ngrok-free.app"] {
            assert_eq!(ok(host, None).unwrap_err().status, 403, "{host}");
        }
        assert_eq!(check(&HttpConfig::default(), "POST", "/mcp", None, None, Some("application/json"), None).unwrap_err().status, 403, "no Host");
    }

    #[test]
    fn the_origin_is_absent_a_loopback_page_or_allowed_by_the_user() {
        assert_eq!(ok("127.0.0.1", None), Ok(()));
        assert_eq!(ok("127.0.0.1", Some("http://localhost:6274")), Ok(()));
        assert_eq!(ok("127.0.0.1", Some("http://127.0.0.1:9")), Ok(()));
        assert_eq!(ok("127.0.0.1", Some("https://chatgpt.com")).unwrap_err().status, 403);
        assert_eq!(ok("127.0.0.1", Some("null")).unwrap_err().status, 403);
        assert_eq!(ok("127.0.0.1", Some("http://localhost.evil.example")).unwrap_err().status, 403);
        let config = HttpConfig { allowed_origins: vec!["https://chatgpt.com".into()], ..HttpConfig::default() };
        assert_eq!(check(&config, "POST", "/mcp", Some("localhost"), Some("https://chatgpt.com"), Some("application/json"), None), Ok(()));
        assert_eq!(check(&config, "POST", "/mcp", Some("localhost"), Some("https://evil.example"), Some("application/json"), None).unwrap_err().status, 403);
    }

    #[test]
    fn the_path_the_method_the_type_and_the_size_are_checked() {
        let config = HttpConfig::default();
        let go = |method, path, ctype, len| check(&config, method, path, Some("127.0.0.1"), None, ctype, len).map_err(|r| r.status);
        assert_eq!(go("POST", "/mcp", Some("application/json; charset=utf-8"), Some(5)), Ok(()));
        assert_eq!(go("POST", "/mcp?x=1", Some("application/json"), None), Ok(()));
        assert_eq!(go("POST", "/other", Some("application/json"), None), Err(404));
        assert_eq!(go("GET", "/mcp", None, None), Err(405));
        assert_eq!(go("POST", "/mcp", Some("text/plain"), None), Err(415));
        assert_eq!(go("POST", "/mcp", None, None), Err(415));
        assert_eq!(go("POST", "/mcp", Some("application/json"), Some(config.max_body + 1)), Err(413));
    }

    #[test]
    fn requests_are_counted_by_the_minute_and_by_those_at_once() {
        let config = HttpConfig { max_per_minute: 3, max_in_flight: 2, ..HttpConfig::default() };
        let load = Load { taken: Mutex::new(VecDeque::new()), in_flight: Mutex::new(0) };
        let t0 = Instant::now();
        assert!(load.admit(&config, t0) && load.admit(&config, t0));
        assert!(!load.admit(&config, t0), "two at once");
        load.done();
        assert!(load.admit(&config, t0));
        load.done();
        load.done();
        assert!(!load.admit(&config, t0 + Duration::from_secs(30)), "three in the minute");
        assert!(load.admit(&config, t0 + Duration::from_secs(61)), "the minute has passed");
    }
}
