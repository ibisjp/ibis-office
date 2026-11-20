// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `ibisOffice-mcp`: the MCP server for Windows, where `ibisOffice.exe` is a GUI program with no standard input and output. The same as `ibisOffice --mcp`.

fn main() {
    std::process::exit(ibis_mcp::run_from_args(std::env::args().skip(1)));
}
