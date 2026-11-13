// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What only the app (`ibis-office-app`) knows and the shell calls: the formats' drivers (by kind) and the app's setup. The app
//! hands them over once, when it starts (`register`, from `ibis_office_app::register_hooks`, which every entry point calls: the
//! window, the export, the browser, the scenarios). Asking before that panics and says so.

use std::sync::OnceLock;

use gpui_kit::App;
use ibis_pptx::Kind;

use crate::format::FormatDriver;

/// What the app hands over.
pub struct AppHooks {
    /// The app's setup (`ibis_office_app::init`: components, theme, every key binding, the formats' among them), for the apps the
    /// shell makes without a window (a scenario's, the picture of a copy).
    pub app_init: fn(&mut App),
    /// The drivers of every format, by kind (a new format adds one line, in the app's `format_drivers.rs`).
    pub drivers: &'static [(Kind, &'static dyn FormatDriver)],
}

static HOOKS: OnceLock<AppHooks> = OnceLock::new();

/// Hand over what the app knows. The first call counts; a later one is ignored.
pub fn register(hooks: AppHooks) {
    let _ = HOOKS.set(hooks);
}

/// This crate's own tests have stand-ins (no format registered); every other program has to register.
#[cfg(test)]
fn hooks() -> &'static AppHooks {
    HOOKS.get_or_init(|| AppHooks { app_init: |_| {}, drivers: &[] })
}
#[cfg(not(test))]
fn hooks() -> &'static AppHooks {
    HOOKS.get().expect("ibis-office-shell: the app's hooks are not registered (call ibis_office_app::register_hooks() first)")
}

pub(crate) fn app_init(cx: &mut App) {
    (hooks().app_init)(cx)
}

pub(crate) fn drivers() -> &'static [(Kind, &'static dyn FormatDriver)] {
    hooks().drivers
}
