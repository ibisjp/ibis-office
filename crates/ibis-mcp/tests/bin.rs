// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #159: the program itself, as a client starts it: standard input and output only, the log on standard error, and it ends when its input is closed.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

#[test]
fn mcp_the_program_talks_over_standard_input_and_output() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/mcp-tests/bin");
    std::fs::create_dir_all(&dir).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ibisOffice-mcp")).arg("--allow-dir").arg(&dir).arg("--read-only").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for (id, method) in [(1, "server/discover"), (2, "tools/list")] {
        writeln!(stdin, "{}", json!({ "jsonrpc": "2.0", "id": id, "method": method })).unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let lines: Vec<Value> = String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str(l).expect("stdout holds the messages only")).collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["result"]["serverInfo"]["name"], "ibisOffice");
    assert!(lines[1]["result"]["tools"].as_array().unwrap().iter().all(|t| t["annotations"]["readOnlyHint"] == true), "read-only was asked");
    // A wrong argument stops the program with a message on standard error.
    let bad = Command::new(env!("CARGO_BIN_EXE_ibisOffice-mcp")).arg("--allow-dir").arg("/no/such/folder").output().unwrap();
    assert_eq!(bad.status.code(), Some(2));
    assert!(bad.stdout.is_empty() && !bad.stderr.is_empty());
}
