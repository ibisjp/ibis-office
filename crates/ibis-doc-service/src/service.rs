// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The service: the open documents, the folders and limits it was given, and `call`.

use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hasher};
use std::path::PathBuf;

use ibis_doc_model::Document;
use ibis_formats::{Kind, Source};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::catalog::{self, Access};
use crate::error::ToolError;
use crate::paths;

/// What the server was started with (design 4.2, 4.3, 5).
#[derive(Clone, Debug)]
pub struct Limits {
    /// Documents open at once.
    pub max_documents: usize,
    /// The size of a file that is opened, in bytes (200 MB).
    pub max_file_size: u64,
    /// The size a zip may expand to, as a multiple of the file's size (a zip bomb stops here).
    pub max_expansion: u64,
    /// The bytes of a tool's result before it is paged.
    pub page_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_documents: 8, max_file_size: 200 * 1024 * 1024, max_expansion: 20, page_bytes: 256 * 1024 }
    }
}

/// What the file was when it was opened: a saving over it checks that it is still that.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fingerprint {
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
    pub sha256: [u8; 32],
}

impl Fingerprint {
    pub fn of_file(path: &std::path::Path, data: &[u8]) -> Self {
        let metadata = std::fs::metadata(path).ok();
        Self { size: data.len() as u64, modified: metadata.and_then(|m| m.modified().ok()), sha256: Sha256::digest(data).into() }
    }

    /// Whether the file at `path` is what this was.
    pub fn still(&self, path: &std::path::Path) -> bool {
        match std::fs::read(path) {
            Ok(data) => data.len() as u64 == self.size && <[u8; 32]>::from(Sha256::digest(&data)) == self.sha256,
            Err(_) => false,
        }
    }
}

/// A file that is open.
pub(crate) struct Open {
    pub kind: Kind,
    pub document: Document,
    pub source: Source,
    pub path: PathBuf,
    pub fingerprint: Fingerprint,
    pub signed: bool,
    pub has_macros: bool,
}

pub struct Service {
    pub(crate) allow_dirs: Vec<PathBuf>,
    pub(crate) read_only: bool,
    pub(crate) limits: Limits,
    pub(crate) documents: BTreeMap<String, Open>,
    /// The ibisOffice program whose `--export` writes PDF and PNG (it needs the window-less drawing, which the service does not have); none: no export.
    pub(crate) exporter: Option<PathBuf>,
    /// The version of the app, for `app_get_version`.
    pub(crate) version: String,
    counter: u64,
}

impl Service {
    /// A service that may touch the files under `allow_dirs` (none: no file at all).
    pub fn new(allow_dirs: &[PathBuf], read_only: bool, limits: Limits) -> Self {
        Self { allow_dirs: paths::canonical_dirs(allow_dirs), read_only, limits, documents: BTreeMap::new(), exporter: None, version: env!("CARGO_PKG_VERSION").to_string(), counter: 0 }
    }

    /// The version `app_get_version` tells (the app's; the crate's own version when not set).
    pub fn set_version(&mut self, version: &str) {
        self.version = version.to_string();
    }

    /// The `ibisOffice` program that `app_export_document` runs (`--export`).
    pub fn set_exporter(&mut self, program: Option<PathBuf>) {
        self.exporter = program;
    }

    /// The service's limits.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// A handle nobody can guess (a random number from the process's hasher, and a counter that keeps two handles apart).
    pub(crate) fn new_document_id(&mut self) -> String {
        self.counter += 1;
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(self.counter);
        let random = hasher.finish();
        let more = std::collections::hash_map::RandomState::new().build_hasher().finish();
        format!("d{random:016x}{more:016x}")
    }

    pub(crate) fn open_document(&mut self, id: &str) -> Result<&mut Open, ToolError> {
        self.documents.get_mut(id).ok_or_else(|| ToolError::new("no_such_document", format!("no open document {id}")))
    }

    /// The tools this service offers: the catalog without the ones that change anything when it is read-only.
    pub fn tools(&self) -> Vec<&'static catalog::Tool> {
        catalog::catalog().iter().filter(|t| !self.read_only || t.access == Access::Read).collect()
    }

    /// Run the tool `name` with `args` (a JSON object).
    pub fn call(&mut self, name: &str, args: &Value) -> Result<Value, ToolError> {
        let tool = catalog::catalog().iter().find(|t| t.name == name).ok_or_else(|| ToolError::new("unsupported_operation", format!("no tool {name}")))?;
        if self.read_only && tool.access != Access::Read {
            return Err(ToolError::new("read_only", format!("{name} changes something and the server is read-only")));
        }
        if !args.is_object() {
            return Err(ToolError::invalid("the arguments are not an object"));
        }
        (tool.run)(self, args)
    }
}

/// Read an argument.
pub(crate) trait Args {
    fn str_arg(&self, name: &str) -> Result<&str, ToolError>;
    fn opt_str(&self, name: &str) -> Result<Option<&str>, ToolError>;
    fn usize_arg(&self, name: &str) -> Result<usize, ToolError>;
    fn opt_usize(&self, name: &str) -> Result<Option<usize>, ToolError>;
    fn opt_bool(&self, name: &str) -> Result<Option<bool>, ToolError>;
}

impl Args for Value {
    fn str_arg(&self, name: &str) -> Result<&str, ToolError> {
        self.opt_str(name)?.ok_or_else(|| ToolError::invalid(format!("{name} is required")))
    }

    fn opt_str(&self, name: &str) -> Result<Option<&str>, ToolError> {
        match self.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(_) => Err(ToolError::invalid(format!("{name} is not a string"))),
        }
    }

    fn usize_arg(&self, name: &str) -> Result<usize, ToolError> {
        self.opt_usize(name)?.ok_or_else(|| ToolError::invalid(format!("{name} is required")))
    }

    fn opt_usize(&self, name: &str) -> Result<Option<usize>, ToolError> {
        match self.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_u64().map(|n| Some(n as usize)).ok_or_else(|| ToolError::invalid(format!("{name} is not a non-negative integer"))),
        }
    }

    fn opt_bool(&self, name: &str) -> Result<Option<bool>, ToolError> {
        match self.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(ToolError::invalid(format!("{name} is not true or false"))),
        }
    }
}

/// The page of `items` a call asks for (`offset`, `limit`): the items and the `next_offset` (absent at the end). Never cut silently (design 5).
pub(crate) fn page<T: Clone>(args: &Value, items: &[T], default_limit: usize) -> Result<(Vec<T>, Option<usize>), ToolError> {
    let offset = args.opt_usize("offset")?.unwrap_or(0);
    let limit = args.opt_usize("limit")?.unwrap_or(default_limit).max(1);
    if offset > items.len() {
        return Err(ToolError::new("out_of_range", format!("offset {offset} is past the {} items", items.len())));
    }
    let end = (offset + limit).min(items.len());
    Ok((items[offset..end].to_vec(), (end < items.len()).then_some(end)))
}
