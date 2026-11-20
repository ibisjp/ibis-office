// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The error of a tool: a code a machine reads and a message (design 7).

use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError {
    /// `not_allowed_path`, `invalid_argument`, `file_not_found`, `unsupported_format`, `needs_password`, `file_too_large`, `no_such_document`, `no_such_sheet`,
    /// `no_such_shape`, `out_of_range`, `read_only`, `discard_required`, `file_changed_on_disk`, `unsupported_operation`, `edit_rejected`, `internal`.
    pub code: &'static str,
    pub message: String,
}

impl ToolError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_argument", message)
    }

    pub fn to_json(&self) -> Value {
        json!({ "code": self.code, "message": self.message })
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ToolError {}
