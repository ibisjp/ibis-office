// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Errors of the evaluator. An M error is a value too (Reason, Message, Detail); the kinds that the application words in its own
//! language (a syntax error with its line, a function that is not supported yet with its line) are kept apart from the rest.
use crate::value::Value;

#[derive(Debug, Clone)]
pub enum ErrKind {
    /// The text is not M. `line` is 1-based.
    Syntax,
    /// A library function that this interpreter does not have yet (design 9.4): the name and the line of the call.
    Unsupported { name: String, line: u32 },
    /// An M error: `Expression.Error`, `DataFormat.Error`, or what `error [Reason=...]` raised.
    Raised,
    /// The evaluation went too deep or too long (stops runaway recursion).
    Limit,
}

#[derive(Debug, Clone)]
pub struct MError {
    pub kind: ErrKind,
    pub line: u32,
    pub reason: String,
    pub message: String,
    pub detail: Value,
}

impl MError {
    pub fn syntax(line: u32, message: &str) -> Self {
        MError { kind: ErrKind::Syntax, line, reason: "Expression.SyntaxError".into(), message: message.into(), detail: Value::Null }
    }
    pub fn expr(message: impl Into<String>) -> Self {
        MError { kind: ErrKind::Raised, line: 0, reason: "Expression.Error".into(), message: message.into(), detail: Value::Null }
    }
    pub fn format(message: impl Into<String>) -> Self {
        MError { kind: ErrKind::Raised, line: 0, reason: "DataFormat.Error".into(), message: message.into(), detail: Value::Null }
    }
    pub fn unsupported(name: &str, line: u32) -> Self {
        MError {
            kind: ErrKind::Unsupported { name: name.into(), line },
            line,
            reason: "Expression.Error".into(),
            message: format!("{name} is not supported yet."),
            detail: Value::Null,
        }
    }
    pub fn limit(message: &str) -> Self {
        MError { kind: ErrKind::Limit, line: 0, reason: "Expression.Error".into(), message: message.into(), detail: Value::Null }
    }
}

impl std::fmt::Display for MError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(f, "line {}: {}", self.line, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}
