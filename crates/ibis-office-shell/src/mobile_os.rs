// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Whether this app runs on a phone or tablet OS (iOS, Android): a build decision (`cfg`), which a scenario on a Mac or a PC cannot take.
//! The scenarios of K0076 (`tasks/doc/test-design/K0076.md`) ask for the mobile menus with `force`, as `start_menu::force_bar` does for
//! the start window's bar. Nothing reads `is_mobile_os` yet: the fix of K0076 does (the menu's lines and the shortcuts shown).
//! Not the same as `SlideEditor::is_touch_device` (a finger on a computer's window also counts there): a computer with a touch screen
//! keeps its menus.

thread_local! {
    static FORCED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the app runs on iOS or Android (or a scenario made it so with `force`).
pub fn is_mobile_os() -> bool {
    FORCED.with(std::cell::Cell::get) || cfg!(any(target_os = "ios", target_os = "android"))
}

/// For the scenarios: behave as on iOS and Android until the guard is dropped.
pub struct ForcedMobileOs(());

impl ForcedMobileOs {
    pub fn new() -> Self {
        FORCED.with(|f| f.set(true));
        Self(())
    }
}

impl Default for ForcedMobileOs {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ForcedMobileOs {
    fn drop(&mut self) {
        FORCED.with(|f| f.set(false));
    }
}
