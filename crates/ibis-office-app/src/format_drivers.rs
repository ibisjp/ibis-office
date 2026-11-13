// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The formats the window can open: the one place that lists them. The shell asks for a format's driver by kind
//! (`shell::format::driver`) and never names a format; `register_hooks` (`register_hooks.rs`) hands `DRIVERS` over when the app starts. A new format adds its `FormatEditor` (a crate of its own, once the formats are cut out) and one
//! line here, and its kind in `ibis-pptx`.

use ibis_pptx::Kind;

use crate::shell::format::{Driver, FormatDriver};
use crate::{docx, pptx, xlsx};

static PRESENTATION: Driver<pptx::Presentation> = Driver::new();
static WORKBOOK: Driver<xlsx::Workbook> = Driver::new();
static DOCUMENT: Driver<docx::Document> = Driver::new();

pub(crate) static DRIVERS: &[(Kind, &dyn FormatDriver)] = &[(Kind::Presentation, &PRESENTATION), (Kind::Workbook, &WORKBOOK), (Kind::Document, &DOCUMENT)];
