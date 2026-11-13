// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The colours of the app's chrome (title row, menus, toolbars, panels, dialogs) in the light and the dark appearance
//! (todo-fix #78). One place decides them.
//!
//! The chrome's code names its colours as light-mode values (`rgb(0xf8fafd)`); in the files of the chrome the
//! `rgb` of this module stands in for gpui's, and gives the same colour in the light appearance and its dark
//! counterpart in the dark one: the lightness turned over about the middle (white paper becomes a dark grey, black
//! text a light grey), the hue kept, so every colour of the chrome has its dark one without a second table.
//! What is in the document (the pages, the cells, the slides) is drawn with gpui's own `rgb` and stays as it is,
//! as Microsoft Office's does.
//!
//! The appearance follows the system's (`follow`, from the window's appearance, in the browser the
//! `prefers-color-scheme`), at once when it changes.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::*;

static DARK: AtomicBool = AtomicBool::new(false);
/// The appearance set by hand over the system's (the scenarios): `0` none, `1` light, `2` dark.
static FORCED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Whether the chrome is drawn in the dark appearance now.
pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// `hex` (`0xRRGGBB`, the colour in the light appearance) as the chrome shows it now.
pub fn rgb(hex: u32) -> Rgba {
    gpui::rgb(if is_dark() { darker(hex) } else { hex })
}

/// A colour of the document (the page, a caret on it, ...): gpui's own `rgb`, the same in both appearances.
pub fn doc(hex: u32) -> Rgba {
    gpui::rgb(hex)
}

/// `rgb` with an alpha (`0..=1`).
#[allow(dead_code)]
pub(crate) fn rgba_of(hex: u32, alpha: f32) -> Hsla {
    let mut c: Hsla = rgb(hex).into();
    c.a = alpha;
    c
}

/// A translucent overlay (a hover or a pressed shade): grey of lightness `light` over light, white over dark, `alpha` thick.
pub fn shade(light: f32, alpha: f32) -> Hsla {
    hsla(0.0, 0.0, if is_dark() { 1.0 } else { light }, alpha)
}

/// `top` (with its alpha) laid over the opaque `base`: the colour that shows, as the alpha compositing of a translucent cover.
pub(crate) fn over(base: Hsla, top: Hsla) -> Hsla {
    let (b, t) = (Rgba::from(base), Rgba::from(top));
    let mix = |below: f32, above: f32| above * t.a + below * (1. - t.a);
    Rgba { r: mix(b.r, t.r), g: mix(b.g, t.g), b: mix(b.b, t.b), a: b.a }.into()
}

/// `white()` of the chrome: the colour of its surfaces (the title row, the panels, the menus).
pub fn white() -> Hsla {
    rgb(0xffffff).into()
}

/// The blue of a selection (a selected dropdown or list row, the selected text of a field) in the dark appearance.
/// The lightness-turned counterpart of the light pale blue (`SELECTED_ROW_BG`, 0xe8f0fe) is 0x142137, 1.06:1 against the
/// dark surface 0x1c1c1c, so a selected row could hardly be told from the others. This is the same hue (217 degrees) at
/// HSL lightness 38% and saturation 53%: 2.3:1 against 0x1c1c1c, and the chrome's text (0xe3e3e3) reads on it at 5.7:1.
pub(crate) const DARK_SELECTION_BG: u32 = 0x2f5597;

/// Light colours whose dark counterpart is fixed by hand instead of by turning the lightness over.
const DARK_FIXED: &[(u32, u32)] = &[(0xe8f0fe, DARK_SELECTION_BG)];

/// The dark counterpart of the light colour `hex`: lightness turned over, kept off pure black and white, hue and saturation kept.
pub(crate) fn darker(hex: u32) -> u32 {
    if let Some((_, dark)) = DARK_FIXED.iter().find(|(light, _)| *light == hex) { return *dark; }
    let (r, g, b) = (((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        (((g - b) / d) % 6.0 + 6.0) % 6.0
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    // White to a dark grey (0.11), black to a light one (0.89); the middle stays.
    let l2 = 0.11 + (1.0 - l) * 0.78;
    // A tinted light surface (a pale blue) keeps its hue but not its strength.
    let s2 = if l > 0.8 { s * 0.5 } else { s };
    let c = (1.0 - (2.0 * l2 - 1.0).abs()) * s2;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let m = l2 - c / 2.0;
    let (r2, g2, b2) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |v: f32| (((v + m) * 255.0).round().clamp(0.0, 255.0)) as u32;
    byte(r2) << 16 | byte(g2) << 8 | byte(b2)
}

/// The theme of the components (buttons, inputs, menus) and the colours the chrome sets over it, for the appearance now.
pub fn apply_theme(window: Option<&mut Window>, cx: &mut App) {
    Theme::change(if is_dark() { ThemeMode::Dark } else { ThemeMode::Light }, window, cx);
    crate::settings_dialog::apply_scrollbar_mode_only(cx);
    let theme = Theme::global_mut(cx);
    theme.muted = rgb(crate::chrome::APP_BG).into();
    theme.sidebar = rgb(crate::chrome::APP_BG).into();
    // The selected text of a TextField: the theme's own dark blue (0x1d4ed8 at 30%) is nearly the surface; the light one is kept.
    if is_dark() { theme.selection = gpui::rgb(DARK_SELECTION_BG).into(); }
    crate::chrome::apply_primary_button_colors(theme);
}

/// The system's appearance is what the chrome shows now (the window's, or the application's with no window).
pub fn follow(appearance: WindowAppearance, window: Option<&mut Window>, cx: &mut App) {
    let dark = match FORCED.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark),
    };
    if DARK.swap(dark, Ordering::Relaxed) == dark && cx.has_global::<Theme>() && Theme::global(cx).mode.is_dark() == dark {
        return;
    }
    apply_theme(window, cx);
    cx.refresh_windows();
}

/// A window's changes of appearance are followed from now on (the application's was set at start).
pub(crate) fn watch(window: &mut Window, _cx: &mut App) {
    window.observe_window_appearance(|window, cx| follow(window.appearance(), None, cx)).detach();
}

/// The appearance set by hand (the scenarios).
#[cfg(feature = "scenarios")]
pub fn force(dark: bool, cx: &mut App) {
    FORCED.store(if dark { 2 } else { 1 }, Ordering::Relaxed);
    follow(if dark { WindowAppearance::Dark } else { WindowAppearance::Light }, None, cx);
}

/// The system's appearance is followed again (after `force`).
#[cfg(feature = "scenarios")]
pub fn release(cx: &mut App) {
    FORCED.store(0, Ordering::Relaxed);
    follow(cx.window_appearance(), None, cx);
}

#[cfg(test)]
mod tests {
    use super::{darker, DARK_SELECTION_BG};

    #[test]
    fn dark_counterparts() {
        assert_eq!(darker(0xffffff), 0x1c1c1c);
        assert_eq!(darker(0x000000), 0xe3e3e3);
        // Mid grey stays about where it is; a hue is kept.
        let mid = darker(0x808080);
        assert!((mid >> 16 & 255) as i32 - 0x80 < 8);
        let blue = darker(0x1a73e8);
        assert!((blue & 255) > (blue >> 16 & 255));
    }

    fn luminance(hex: u32) -> f32 {
        let f = |v: u32| {
            let c = (v & 255) as f32 / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * f(hex >> 16) + 0.7152 * f(hex >> 8) + 0.0722 * f(hex)
    }
    fn contrast(a: u32, b: u32) -> f32 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// The chrome's text (the font names of the font list are drawn in it) reads on the chrome's surfaces in the dark appearance (#114:
    /// the font previews were baked in #1f1f1f, 1.2:1 on the dark list).
    #[core::prelude::v1::test]
    fn dark_text_reads_on_dark_surfaces() {
        let text = crate::chrome::TEXT;
        for surface in [0xffffff, crate::chrome::APP_BG] {
            assert!(contrast(darker(text), darker(surface)) >= 7.0, "{surface:06x}");
            assert!(contrast(text, surface) >= 7.0, "light {surface:06x}");
        }
        assert!(contrast(0x1f1f1f, darker(0xffffff)) < 3.0, "the old baked colour is what was unreadable");
    }

    /// The selection blue of the dark appearance reads: the text on it, and the blue against the dark surface (the pale blue
    /// of the light appearance is untouched).
    #[core::prelude::v1::test]
    fn dark_selection_blue_reads() {
        let surface = darker(0xffffff);
        assert_eq!(darker(crate::chrome::SELECTED_ROW_BG), DARK_SELECTION_BG);
        assert!(contrast(darker(crate::chrome::TEXT), DARK_SELECTION_BG) >= 4.5);
        assert!(contrast(DARK_SELECTION_BG, surface) >= 2.0);
    }

    /// A half-transparent cover over a colour is the average of the two; an opaque cover hides it and a clear one leaves it.
    #[core::prelude::v1::test]
    fn over_is_the_alpha_compositing() {
        use super::{over, Rgba};
        let (base, grey) = (Rgba { r: 1., g: 0.5, b: 0., a: 1. }, Rgba { r: 0.5, g: 0.5, b: 0.5, a: 0.5 });
        let half = Rgba::from(over(base.into(), grey.into()));
        assert!((half.r - 0.75).abs() < 1e-4 && (half.g - 0.5).abs() < 1e-4 && (half.b - 0.25).abs() < 1e-4 && half.a == 1.);
        let hidden = Rgba::from(over(base.into(), Rgba { a: 1., ..grey }.into()));
        assert!((hidden.r - 0.5).abs() < 1e-4);
        let same = Rgba::from(over(base.into(), Rgba { a: 0., ..grey }.into()));
        assert!((same.r - 1.).abs() < 1e-4);
    }
}
