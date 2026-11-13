// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What every ibisOffice crate shares and that needs nothing of the editor window: the clock (`clock`), where the user's data
//! and settings are kept (`app_data`, `settings`), the UI's words (`i18n`) and the CMYK converter in use (`cmyk`).
//!
//! The crate below the drawing (`ibis-office-render`) and the window (`ibis-office-app`): see tasks/doc/crate-split-plan.md.

pub mod app_data;
pub mod clock;
pub mod cmyk;
pub mod i18n;
pub mod settings;
