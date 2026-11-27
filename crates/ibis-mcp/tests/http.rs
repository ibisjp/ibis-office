// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #231: the HTTP transport (`POST /mcp`, JSON answers) does what the standard input and output do (reads, edits, saves, the folders and the read-only rule),
//! and refuses a wrong Host, Origin, method, path, type and size. The client is a plain socket: the bytes on the wire are what a tunnel client sends.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

use ibis_doc_service::{Limits, Service};
use ibis_mcp::{HttpConfig, HttpServer};
use serde_json::{Value, json};

fn sandbox(test: &str, files: &[&str]) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/mcp-tests").join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in files {
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(f), dir.join(f)).unwrap();
    }
    dir.canonicalize().unwrap()
}

struct Running {
    port: u16,
    stopper: ibis_mcp::Stopper,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stopper.stop();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn start(service: Service, config: HttpConfig) -> Running {
    let server = HttpServer::bind(service, config).unwrap();
    let (port, stopper) = (server.port(), server.stopper());
    let thread = std::thread::spawn(move || server.run());
    Running { port, stopper, thread: Some(thread) }
}

/// (status, body) of one request written as it is.
fn send(port: u16, head: &str, body: &[u8]) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).unwrap();
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).to_string();
    let status = text.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let (head, body) = text.split_once("\r\n\r\n").map(|(h, b)| (h.to_string(), b.to_string())).unwrap_or_default();
    // A long answer (the tool list) comes in chunks: size in hex, line break, the bytes, line break; 0 ends.
    let body = if head.to_ascii_lowercase().contains("transfer-encoding: chunked") { dechunk(&body) } else { body };
    (status, body)
}

fn dechunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some((size, after)) = rest.split_once("\r\n") {
        let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if n == 0 || after.len() < n {
            break;
        }
        out.push_str(&after[..n]);
        rest = after[n..].strip_prefix("\r\n").unwrap_or(&after[n..]);
    }
    out
}

fn post(port: u16, host: &str, extra: &str, body: &str) -> (u16, String) {
    let head = format!("POST /mcp HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n", body.len());
    send(port, &head, body.as_bytes())
}

fn rpc(port: u16, id: u64, method: &str, params: Value) -> Value {
    let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
    let (status, answer) = post(port, &format!("127.0.0.1:{port}"), "", &body);
    assert_eq!(status, 200, "{answer}");
    serde_json::from_str(&answer).unwrap()
}

fn call(port: u16, id: u64, name: &str, arguments: Value) -> Value {
    rpc(port, id, "tools/call", json!({ "name": name, "arguments": arguments }))
}

#[test]
fn http_the_handshake_the_tools_and_a_notification() {
    let server = start(Service::new(&[], false, Limits::default()), HttpConfig::default());
    let port = server.port;
    let init = rpc(port, 1, "initialize", json!({ "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } }));
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(init["result"]["serverInfo"]["name"], "ibisOffice");
    // A notification has no answer: 202 and no body.
    let (status, body) = post(port, &format!("localhost:{port}"), "", &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string());
    assert_eq!((status, body.as_str()), (202, ""));
    let tools = rpc(port, 2, "tools/list", json!({}));
    let list = tools["result"]["tools"].as_array().unwrap();
    assert!(list.iter().any(|t| t["name"] == "docx_replace_text" && t["inputSchema"]["type"] == "object"));
    assert!(list.iter().any(|t| t["name"] == "app_save_document"), "the writing tools are offered over HTTP too");
    let discover = rpc(port, 3, "server/discover", json!({ "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } }));
    assert_eq!(discover["result"]["supportedVersions"], json!(["2026-07-28", "2025-11-25"]));
    // An older version at the handshake is an error (not answered with the server's own).
    let old = rpc(port, 4, "initialize", json!({ "protocolVersion": "2024-11-05" }));
    assert_eq!(old["error"]["code"], -32602);
    // A message that is not JSON is answered, and the server goes on.
    let (status, body) = post(port, &format!("127.0.0.1:{port}"), "", "{not json");
    assert_eq!(status, 200);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["error"]["code"], -32700);
    assert_eq!(rpc(port, 5, "ping", json!({}))["result"], json!({}));
}

#[test]
fn http_a_document_is_read_edited_and_saved_as_over_standard_input() {
    let dir = sandbox("http-call", &["autospace-letters.docx"]);
    let server = start(Service::new(std::slice::from_ref(&dir), false, Limits::default()), HttpConfig::default());
    let port = server.port;
    let opened = call(port, 1, "app_open_document", json!({ "path": dir.join("autospace-letters.docx").to_string_lossy() }));
    assert_eq!(opened["result"]["isError"], false);
    let id = opened["result"]["structuredContent"]["document_id"].as_str().unwrap().to_string();
    let read = call(port, 2, "docx_list_paragraphs", json!({ "document_id": id, "limit": 1 }));
    assert!(read["result"]["_meta"]["io.ibisoffice/untrusted-content"].is_string());
    let edit = call(port, 3, "docx_insert_text", json!({ "document_id": id, "at": { "paragraph": 0, "offset": 0 }, "text": "HTTP " }));
    assert_eq!(edit["result"]["structuredContent"]["dirty"], true);
    let saved = call(port, 4, "app_save_document", json!({ "document_id": id, "path": dir.join("out.docx").to_string_lossy() }));
    assert_eq!(saved["result"]["isError"], false, "{saved}");
    assert!(dir.join("out.docx").is_file());
    let refused = call(port, 5, "app_open_document", json!({ "path": outside_path() }));
    assert_eq!(refused["result"]["structuredContent"]["error"]["code"], "not_allowed_path");
    let reopened = call(port, 6, "app_open_document", json!({ "path": dir.join("out.docx").to_string_lossy() }));
    let id2 = reopened["result"]["structuredContent"]["document_id"].as_str().unwrap().to_string();
    let text = call(port, 7, "docx_get_text", json!({ "document_id": id2, "format": "plain" }));
    assert!(text["result"]["structuredContent"]["text"].as_str().unwrap().starts_with("HTTP "));
}

#[test]
fn http_read_only_offers_no_writing_tool() {
    let server = start(Service::new(&[], true, Limits::default()), HttpConfig::default());
    let tools = rpc(server.port, 1, "tools/list", json!({}));
    let list = tools["result"]["tools"].as_array().unwrap();
    assert!(list.iter().all(|t| t["annotations"]["readOnlyHint"] == true));
    assert_eq!(call(server.port, 2, "docx_insert_text", json!({}))["error"]["code"], -32602);
}

#[test]
fn http_refuses_the_wrong_host_origin_method_path_type_and_size() {
    let config = HttpConfig { max_body: 1000, allowed_origins: vec!["https://ok.example".into()], ..HttpConfig::default() };
    let server = start(Service::new(&[], false, Limits::default()), config);
    let port = server.port;
    let ping = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }).to_string();
    for host in ["evil.example", "evil.example:80", "127.0.0.1.evil.example", "abc.ngrok-free.app", "192.168.0.2"] {
        assert_eq!(post(port, host, "", &ping).0, 403, "{host}");
    }
    assert_eq!(post(port, "127.0.0.1", "Origin: https://evil.example\r\n", &ping).0, 403);
    assert_eq!(post(port, "127.0.0.1", "Origin: null\r\n", &ping).0, 403);
    assert_eq!(post(port, "127.0.0.1", "Origin: https://ok.example\r\n", &ping).0, 200);
    assert_eq!(post(port, "127.0.0.1", "Origin: http://localhost:6274\r\n", &ping).0, 200);
    assert_eq!(send(port, &format!("GET /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"), b"").0, 405);
    assert_eq!(send(port, &format!("POST /elsewhere HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n"), b"{}").0, 404);
    assert_eq!(send(port, &format!("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: text/plain\r\nContent-Length: 2\r\nConnection: close\r\n\r\n"), b"{}").0, 415);
    // Too large by the length it says, and by what a chunked body turns out to hold.
    let big = format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{{\"x\":\"{}\"}}}}", "a".repeat(2000));
    assert_eq!(post(port, &format!("127.0.0.1:{port}"), "", &big).0, 413);
    let chunked = format!("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{big}\r\n0\r\n\r\n", big.len());
    assert_eq!(send(port, &chunked, b"").0, 413);
    // It still answers after all that.
    assert_eq!(post(port, &format!("127.0.0.1:{port}"), "", &ping).0, 200);
}

#[test]
fn http_the_rate_is_limited() {
    let config = HttpConfig { max_per_minute: 3, ..HttpConfig::default() };
    let server = start(Service::new(&[], false, Limits::default()), config);
    let ping = json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }).to_string();
    let statuses: Vec<u16> = (0..5).map(|_| post(server.port, &format!("127.0.0.1:{}", server.port), "", &ping).0).collect();
    assert_eq!(statuses, vec![200, 200, 200, 429, 429]);
}

#[test]
fn http_the_program_listens_on_the_loopback_only() {
    use std::process::{Command, Stdio};
    let dir = sandbox("http-bin", &[]);
    // A free port to ask for.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibisOffice-mcp")).arg("--allow-dir").arg(&dir).arg("--http").arg(port.to_string()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let mut up = false;
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            up = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let answered = up && rpc(port, 1, "ping", json!({})) == json!({ "jsonrpc": "2.0", "id": 1, "result": {} });
    // Not reachable by another address of this computer, when it has one (the listener is bound to 127.0.0.1 alone).
    let elsewhere = local_other_address().map(|ip| TcpStream::connect_timeout(&format!("{ip}:{port}").parse().unwrap(), std::time::Duration::from_millis(500)).is_ok());
    let _ = child.kill();
    let _ = child.wait();
    assert!(answered, "the program answers on the port it was told");
    assert_ne!(elsewhere, Some(true), "reachable from another address of this computer");
}

/// An address of this computer that is not 127.0.0.1, when there is one (a UDP socket's own address toward a public address; nothing is sent).
fn local_other_address() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback()).then(|| ip.to_string())
}

/// An existing file outside every allowed folder (`/etc/passwd` is not an absolute path on Windows).
fn outside_path() -> &'static str {
    if cfg!(windows) { r"C:\Windows\win.ini" } else { "/etc/passwd" }
}
