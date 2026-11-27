// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The MCP server (GitHub issue #159, `tasks/doc/design/2026-11-17-159-mcp.md`): a thin door over `ibis-doc-service`. The protocol's versions are
//! in `protocol.rs` alone (2026-07-28 is the main one, 2025-11-25 is answered too); the tools, their arguments and their results are the catalog's.

mod cli;
mod http;
mod protocol;
mod stdio;

/// The version of the app this server is of (build-version.txt).
pub const VERSION: &str = env!("IBIS_VERSION");

pub use cli::run_from_args;
pub use http::{HttpConfig, HttpServer, Stopper};
pub use protocol::Server;
pub use stdio::serve;
