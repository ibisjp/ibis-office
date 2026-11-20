// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/247)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Android platform backend of GPUI (empty on any other target, so that the workspace checks everywhere).
#![cfg(target_os = "android")]
mod dispatcher;
mod display;
mod jni_bridge;
mod platform;
mod window;

pub use android_activity;
pub use jni_bridge::sync_saved_file;
pub use platform::AndroidPlatform;

pub fn init_logging() {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("ibisOffice"),
    );
    std::panic::set_hook(Box::new(|info| {
        log::error!("RUST PANIC: {info}");
    }));
}
