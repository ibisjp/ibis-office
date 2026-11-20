// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/247)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! iOS platform backend of GPUI (empty on any other target, so that the workspace checks everywhere).
#![cfg(target_os = "ios")]
mod bridge;
mod dispatcher;
mod display;
mod platform;
mod window;

pub use bridge::{make_non_null_view, sync_saved_file};
pub use platform::IosPlatform;
