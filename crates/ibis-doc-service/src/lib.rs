// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The headless document service (GitHub issue #159, `tasks/doc/design/2026-11-17-159-mcp.md`): files are opened into [`Service`], read and
//! edited through tools by name (`docx_get_text`, `xlsx_set_cells`, ...), and saved. The tools are listed once, in [`catalog`]; the MCP server and
//! the assistant (#231) are thin doors over it. No window, no GPUI: the model (`ibis-doc-model`), the readers and writers (`ibis-formats`) and the
//! calculation (`ibis-calc`) only.

mod catalog;
mod a1;
mod edit;
mod error;
mod paths;
mod service;
mod tools_app;
mod tools_docx;
mod tools_pptx;
mod tools_xlsx;

pub use catalog::{Access, Tool, catalog};
pub use error::ToolError;
pub use service::{Limits, Service};
