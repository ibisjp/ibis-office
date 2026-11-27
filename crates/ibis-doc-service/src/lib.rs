// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The headless document service (GitHub issue #159, `tasks/doc/design/2026-11-17-159-mcp.md`): files are opened into [`Service`], read and
//! edited through tools by name (`docx_get_text`, `xlsx_set_cells`, ...), and saved. The tools are listed once, in [`catalog`]; the MCP server and
//! the assistant (#231) are thin doors over it. No window, no GPUI: the model (`ibis-doc-model`), the readers and writers (`ibis-formats`) and the
//! calculation (`ibis-calc`) only.

mod catalog;
mod clock;
mod a1;
mod edit;
mod error;
mod git_read;
mod paths;
mod picture_file;
mod service;
mod tools_app;
mod tools_docx;
mod tools_docx_layout;
mod tools_docx_more;
mod tools_docx_objects;
mod tools_docx_read;
mod tools_docx_set;
mod tools_docx_struct;
mod tools_pptx;
mod tools_pptx_b;
mod tools_xlsx;
mod tools_xlsx_read;
mod tools_xlsx_set;
mod tools_xlsx_struct;
mod tools_xlsx_chart;

pub use catalog::{Access, Tool, catalog};
pub use error::ToolError;
pub use service::{Limits, Service};
