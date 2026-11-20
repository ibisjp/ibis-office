// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! OOXML packages (`.pptx`, `.xlsx`, `.docx`) for saving without breaking
//! what ibisOffice does not understand.
//!
//! - [`zip`]: reading and writing the ZIP container; parts that are not
//!   changed are copied as they were compressed.
//! - [`xml`]: a read-write XML tree that keeps unknown elements, attributes,
//!   namespace declarations, comments, whitespace and the source text of
//!   every node it does not change.
//! - [`package`]: the parts of a file, `[Content_Types].xml` and the
//!   relationships, changed in place and saved.
//! - [`signature`]: looking at the digital signatures of a package (not changing them).
//! - [`canon`]: comparing two files by meaning (normalized XML), for tests
//!   and the `ibis-ooxml-diff` tool.
//!
//! No UI or document model here: the formats' readers and writers build on it.

pub mod canon;
pub mod base64;
pub mod cfb;
pub mod encryption;
pub mod legacy_crypt;
pub mod modify_password;
pub mod package;
pub mod signature;
pub mod xml;
pub mod zip;

pub use package::{Package, Relationship, SaveOptions};
pub use xml::{Document as XmlDocument, NodeId};

/// Why a package could not be read or written. The message is for the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}
