// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The standard input and output transport: one JSON message a line, in both directions; the log is the standard error (the protocol says so).

use std::io::{BufRead, Write};

use ibis_doc_service::Service;

use crate::protocol::Server;

/// The biggest message that is taken (a line longer than this is answered with a parse error and skipped).
const MAX_MESSAGE: usize = 32 * 1024 * 1024;

/// Read the messages of `input` and write the answers to `output` until the input is closed.
pub fn serve(service: Service, mut input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    let mut server = Server::new(service);
    let mut line = Vec::new();
    loop {
        line.clear();
        let mut too_long = false;
        loop {
            let available = input.fill_buf()?;
            if available.is_empty() {
                break;
            }
            let (take, found) = match available.iter().position(|b| *b == b'\n') {
                Some(i) => (i + 1, true),
                None => (available.len(), false),
            };
            if line.len() + take > MAX_MESSAGE {
                too_long = true;
            } else {
                line.extend_from_slice(&available[..take]);
            }
            input.consume(take);
            if found {
                break;
            }
        }
        if line.is_empty() && !too_long {
            return Ok(());
        }
        let answer = if too_long {
            Some(Server::parse_error("the message is too large").to_string())
        } else {
            match std::str::from_utf8(&line) {
                Ok(text) if text.trim().is_empty() => None,
                Ok(text) => server.handle(text.trim_end_matches(['\r', '\n'])),
                Err(_) => Some(Server::parse_error("the message is not UTF-8").to_string()),
            }
        };
        if let Some(answer) = answer {
            output.write_all(answer.as_bytes())?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
}
