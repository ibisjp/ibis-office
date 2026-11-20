// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The catalog: every tool once, with its name (commandNamingRule A), what it takes (a JSON Schema), whether it only reads, and the function that runs it.
//! The MCP server's `tools/list` and the assistant's tool definitions are made from this table; nobody writes a tool twice.

use serde_json::{Value, json};

use crate::error::ToolError;
use crate::service::Service;
use crate::{tools_app, tools_docx, tools_pptx, tools_xlsx};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Reads (opening and closing a document are not changes of a file).
    Read,
    /// Changes an open document in memory.
    Edit,
    /// Writes a file.
    File,
}

pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub access: Access,
    /// The result holds the document's own text: a client must treat it as data, not as instructions.
    pub returns_content: bool,
    pub schema: fn() -> Value,
    pub(crate) run: fn(&mut Service, &Value) -> Result<Value, ToolError>,
}

/// An object schema: `props` as `(name, schema)`, `required` names.
pub(crate) fn object(props: &[(&str, Value)], required: &[&str]) -> Value {
    let mut map = serde_json::Map::new();
    for (name, schema) in props {
        map.insert((*name).to_string(), schema.clone());
    }
    json!({ "type": "object", "properties": map, "required": required, "additionalProperties": false })
}

pub(crate) fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

pub(crate) fn integer(description: &str) -> Value {
    json!({ "type": "integer", "minimum": 0, "description": description })
}

pub(crate) fn boolean(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

pub(crate) fn document_id() -> (&'static str, Value) {
    ("document_id", string("The handle app_open_document returned."))
}

/// Every tool.
pub fn catalog() -> &'static [Tool] {
    static ALL: std::sync::OnceLock<Vec<Tool>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut all = Vec::new();
        all.extend(tools_app::tools());
        all.extend(tools_docx::tools());
        all.extend(tools_docx::edit_tools());
        all.extend(tools_xlsx::tools());
        all.extend(tools_xlsx::edit_tools());
        all.extend(tools_pptx::tools());
        all.extend(tools_pptx::edit_tools());
        all
    })
}
