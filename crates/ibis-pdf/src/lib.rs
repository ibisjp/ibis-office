// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Lossless PDF container, revision history, incremental update,
//! signature record inspection, and viewer scene model for ibisOffice.
//!
//! No UI or platform dependencies: shared by desktop and browser (wasm) builds.

pub mod container;
pub mod cos;
pub mod crypt;
#[cfg(feature = "interpret")]
pub mod interpret;
#[cfg(feature = "interpret")]
pub mod gradient;
#[cfg(feature = "interpret")]
pub mod softmask;
pub mod scene;
pub mod text_layout;
pub mod signature;
pub mod xref;

pub use container::{PdfContainer, SaveMode};
pub use cos::{CosDict, CosObj, CosString, ObjectId, StreamData};
pub use scene::{CharBox, DisplayCmd, PageInfo, PageScene, PageTextIndex, PdfRect};
pub use signature::{ByteRangeStatus, SignatureRecord, SubFilter};
pub use xref::{Revision, XrefEntry};

/// Errors returned when parsing or writing a PDF container.
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

pub type Result<T> = std::result::Result<T, Error>;
