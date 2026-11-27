// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #159: the protocol over a standard input and output (design 8, 3): the 2026-07-28 and the 2025-11-25 ways in, errors, and a document edited through `tools/call`.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use ibis_doc_service::{Limits, Service};
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

/// The answers to the lines, one a line.
fn talk(service: Service, lines: &[String]) -> Vec<Value> {
    let input = lines.join("\n") + "\n";
    let mut output = Vec::new();
    ibis_mcp::serve(service, Cursor::new(input.into_bytes()), &mut output).unwrap();
    String::from_utf8(output).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn request(id: u64, method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

fn empty_service() -> Service {
    Service::new(&[], false, Limits::default())
}

#[test]
fn mcp_both_versions_are_answered() {
    let answers = talk(
        empty_service(),
        &[
            // 2025-11-25: the handshake, then a notification (no answer), then the tools.
            request(1, "initialize", json!({ "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } })),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string(),
            request(2, "tools/list", json!({})),
            // 2026-07-28: no handshake; the version is in the request.
            request(3, "server/discover", json!({ "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } })),
            request(4, "tools/list", json!({ "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } })),
            request(5, "tools/list", json!({ "_meta": { "io.modelcontextprotocol/protocolVersion": "2024-01-01" } })),
            request(6, "ping", json!({})),
            request(7, "nothing/here", json!({})),
        ],
    );
    assert_eq!(answers.len(), 7, "the notification has no answer");
    assert_eq!(answers[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(answers[0]["result"]["serverInfo"]["name"], "ibisOffice");
    assert_eq!(answers[0]["result"]["serverInfo"]["version"], ibis_mcp::VERSION);
    assert!(ibis_mcp::VERSION.starts_with("0.8.") && ibis_mcp::VERSION != "0.8.0", "the app's version (build-version.txt), not the crate's: {}", ibis_mcp::VERSION);
    let tools = answers[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), answers[3]["result"]["tools"].as_array().unwrap().len(), "the same tools in both versions");
    assert!(tools.iter().any(|t| t["name"] == "docx_replace_text" && t["inputSchema"]["type"] == "object"));
    assert_eq!(answers[2]["result"]["supportedVersions"], json!(["2026-07-28", "2025-11-25"]));
    assert_eq!(answers[4]["error"]["code"], -32602, "an unsupported version");
    assert_eq!(answers[5]["result"], json!({}));
    assert_eq!(answers[6]["error"]["code"], -32601);
    // An older version asked at the handshake is answered with the one the server has.
    let answers = talk(empty_service(), &[request(1, "initialize", json!({ "protocolVersion": "2024-11-05" }))]);
    assert_eq!(answers[0]["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn mcp_bad_messages_are_answered_not_fatal() {
    let big = format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{{\"x\":\"{}\"}}}}", "a".repeat(100_000));
    let answers = talk(empty_service(), &["{not json".into(), "[1,2]".into(), json!({ "jsonrpc": "2.0", "id": 9 }).to_string(), big, request(2, "tools/call", json!({ "name": "no_such_tool" })), request(3, "tools/call", json!({ "arguments": {} })), request(4, "ping", json!({}))]);
    assert_eq!(answers[0]["error"]["code"], -32700);
    assert_eq!(answers[1]["error"]["code"], -32600);
    assert_eq!(answers[2]["error"]["code"], -32600);
    assert_eq!(answers[3]["result"], json!({}));
    assert_eq!(answers[4]["error"]["code"], -32602);
    assert_eq!(answers[5]["error"]["code"], -32602);
    assert_eq!(answers[6]["result"], json!({}), "the server goes on after a bad message");
}

#[test]
fn mcp_a_document_is_edited_through_tools_call() {
    let dir = sandbox("call", &["autospace-letters.docx"]);
    let service = Service::new(&[dir.clone()], false, Limits::default());
    let call = |id: u64, name: &str, arguments: Value| request(id, "tools/call", json!({ "name": name, "arguments": arguments }));
    // The document_id is only known after the first answer, so the conversation is in two parts.
    let first = talk(service, &[call(1, "app_open_document", json!({ "path": dir.join("autospace-letters.docx").to_string_lossy() }))]);
    assert_eq!(first[0]["result"]["isError"], false);
    assert_eq!(first[0]["result"]["_meta"], Value::Null, "opening a document returns no document text");
    // A new service (the first one is gone with its input): the whole conversation goes through one input, ids are read from the previous answers by the test.
    let service = Service::new(&[dir.clone()], false, Limits::default());
    let mut server = ibis_mcp::Server::new(service);
    let answer = |server: &mut ibis_mcp::Server, line: String| -> Value { serde_json::from_str(&server.handle(&line).unwrap()).unwrap() };
    let opened = answer(&mut server, call(1, "app_open_document", json!({ "path": dir.join("autospace-letters.docx").to_string_lossy() })));
    let id = opened["result"]["structuredContent"]["document_id"].as_str().unwrap().to_string();
    let read = answer(&mut server, call(2, "docx_list_paragraphs", json!({ "document_id": id, "limit": 1 })));
    assert!(read["result"]["_meta"]["io.ibisoffice/untrusted-content"].is_string(), "document text is marked as data");
    assert_eq!(read["result"]["content"][0]["type"], "text");
    let edit = answer(&mut server, call(3, "docx_insert_text", json!({ "document_id": id, "at": { "paragraph": 0, "offset": 0 }, "text": "MCP " })));
    assert_eq!(edit["result"]["structuredContent"]["dirty"], true);
    let saved = answer(&mut server, call(4, "app_save_document", json!({ "document_id": id, "path": dir.join("out.docx").to_string_lossy() })));
    assert_eq!(saved["result"]["isError"], false, "{saved}");
    // A tool error is a result with isError, and has the code.
    let refused = answer(&mut server, call(5, "app_open_document", json!({ "path": outside_path() })));
    assert_eq!(refused["result"]["isError"], true);
    assert_eq!(refused["result"]["structuredContent"]["error"]["code"], "not_allowed_path");
    let reopened = answer(&mut server, call(6, "app_open_document", json!({ "path": dir.join("out.docx").to_string_lossy() })));
    let id2 = reopened["result"]["structuredContent"]["document_id"].as_str().unwrap().to_string();
    let text = answer(&mut server, call(7, "docx_get_text", json!({ "document_id": id2, "format": "plain" })));
    assert!(text["result"]["structuredContent"]["text"].as_str().unwrap().starts_with("MCP "));
}

#[test]
fn mcp_read_only_does_not_list_or_run_what_changes() {
    let service = Service::new(&[], true, Limits::default());
    let mut server = ibis_mcp::Server::new(service);
    let list: Value = serde_json::from_str(&server.handle(&request(1, "tools/list", json!({}))).unwrap()).unwrap();
    let tools = list["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().all(|t| t["annotations"]["readOnlyHint"] == true));
    assert!(!tools.iter().any(|t| t["name"] == "app_save_document" || t["name"] == "docx_insert_text"));
    let call: Value = serde_json::from_str(&server.handle(&request(2, "tools/call", json!({ "name": "docx_insert_text", "arguments": {} }))).unwrap()).unwrap();
    assert_eq!(call["error"]["code"], -32602);
}

/// An existing file outside every allowed folder (`/etc/passwd` is not an absolute path on Windows).
fn outside_path() -> &'static str {
    if cfg!(windows) { r"C:\Windows\win.ini" } else { "/etc/passwd" }
}
