// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Where an Android window's content goes inside the system's bars, and how tall the soft keyboard is, from ONE set of insets
//! (`tasks/doc/test-design/K-android-safe-area-kbd-enter.md`). The activity (`IbisActivity.java`) hands over the raw insets of the window and puts the margins this
//! returns on its native view; the keyboard's height is what covers the view **below those margins** (the keyboard's top edge less the margin the view was given),
//! so the margin the content keeps off the bottom bar and the part of the keyboard the toolbar must clear are never worked out from two different formulas (the
//! toolbar sat half under the keyboard when they were: the margin came from the stable inset, which is 0 on Android 16, and the keyboard's height had the
//! navigation bar taken off it anyway).
//!
//! The safe area of a side is the largest of the system bars (status and navigation bars), the display cutout and the mandatory system gestures (the grip at the
//! bottom of the gesture navigation), as iOS's safe area is: all four sides.

/// How many integers the activity passes: the system bars, the display cutout and the mandatory system gestures (each left, top, right, bottom, in pixels), then the
/// keyboard's top edge's inset (the IME's bottom inset, in pixels).
pub const RAW_LEN: usize = 13;

/// The margins to put on the view (left, top, right, bottom, pixels) and the soft keyboard's height (dp) from the raw insets (`RAW_LEN` pixels, see above) and the
/// screen's density (pixels per dp).
pub fn compute(raw: &[i32; RAW_LEN], density: f32) -> [f32; 5] {
    let side = |i: usize| raw[i].max(raw[4 + i]).max(raw[8 + i]).max(0);
    let margins = [side(0), side(1), side(2), side(3)];
    // The keyboard covers the view from its top edge down; the view stops `margins[3]` above the window's bottom, so that much of the keyboard's height is already
    // below the view.
    let covered = (raw[12] - margins[3]).max(0);
    let density = if density > 0.0 { density } else { 1.0 };
    [margins[0] as f32, margins[1] as f32, margins[2] as f32, margins[3] as f32, covered as f32 / density]
}

thread_local! {
    static FORCED: std::cell::Cell<Option<[f32; 4]>> = const { std::cell::Cell::new(None) };
}

/// The safe area of the window (left, top, right, bottom, in the app's pixels): what the root of the window keeps its content in from the edges. Android's window is the whole
/// screen (the activity cannot move the surface it draws to), so the app does it (`gpui_android::safe_area`); on every other system it is nothing (the OS places the window).
pub fn current() -> [f32; 4] {
    if let Some(forced) = FORCED.with(std::cell::Cell::get) {
        return forced;
    }
    #[cfg(target_os = "android")]
    {
        gpui_android::safe_area()
    }
    #[cfg(not(target_os = "android"))]
    {
        [0.0; 4]
    }
}

/// For the scenarios: the window has this safe area until the guard is dropped.
pub struct ForcedSafeArea(());

impl ForcedSafeArea {
    pub fn new(area: [f32; 4]) -> Self {
        FORCED.with(|f| f.set(Some(area)));
        Self(())
    }
}

impl Drop for ForcedSafeArea {
    fn drop(&mut self) {
        FORCED.with(|f| f.set(None));
    }
}
