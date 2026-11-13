// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ヘルプ > ibisOffice について (the app menu's ibisOffice について on macOS), a dialog inside the editor window on every
//! platform (the shared dialog layer, shell/dialog.rs `InteractionLayer::About`):
//! the version, "Powered by ibisOffice" with the link to the project, the
//! copyright and the license. The "Powered by ibisOffice" line and its link
//! are the additional term under AGPL-3.0 section 7(b) (NOTICE,
//! docs/TRADEMARK.md): a derivative keeps them on its own About screen.
use gpui_kit::component::{ActiveTheme as _, Root, v_flex};
use crate::editor::SlideEditor;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::i18n;

/// What a derivative must show (NOTICE): the line and where the link goes.
pub const POWERED_BY: &str = "Powered by ibisOffice";
pub const PROJECT_URL: &str = "https://github.com/ibisjp/ibis-office";

/// The blue of the start view's links (`start_view.rs`): the 「新しいバージョンがあります。」 button.
const UPDATE_LINK_COLOR: u32 = 0x0f6cbd;

/// ヘルプ > ibisOffice について (and the app menu's item): open the About dialog in the editor window the user is in (the
/// active one, else any editor window; with none open, a start window is opened for it). Deferred: the action comes while
/// the window with the keys is being updated, when its editor cannot be updated.
pub fn open(cx: &mut App) {
    cx.defer(|cx| {
        let mut found = editor_window(cx);
        if found.is_none() {
            crate::app::open_editor(None, None, cx);
            found = editor_window(cx);
        }
        if let Some((handle, editor)) = found {
            editor.update(cx, |editor, cx| editor.show_about(cx));
            let _ = handle.update(cx, |_, window, _| window.activate_window());
        }
    });
}

/// The editor window to show the dialog in: the active window when it is one, else the first.
fn editor_window(cx: &mut App) -> Option<(AnyWindowHandle, Entity<SlideEditor>)> {
    let of = |handle: AnyWindowHandle, cx: &mut App| -> Option<(AnyWindowHandle, Entity<SlideEditor>)> {
        let root = handle.downcast::<Root>()?;
        let editor = root.update(cx, |root, _, _| root.view().clone().downcast::<SlideEditor>().ok()).ok().flatten()?;
        Some((handle, editor))
    };
    if let Some(found) = cx.active_window().and_then(|w| of(w, cx)) {
        return Some(found);
    }
    cx.windows().into_iter().find_map(|w| of(w, cx))
}

/// Whether the start-up check found a newer version (never in the browser).
pub fn update_available() -> bool {
    #[cfg(not(target_family = "wasm"))]
    {
        crate::update::available_version().is_some()
    }
    #[cfg(target_family = "wasm")]
    {
        false
    }
}

/// The button's click: the question the start-up check asked, once more.
fn offer_update_again(_cx: &mut App) {
    #[cfg(not(target_family = "wasm"))]
    crate::update::offer_again(_cx);
}

/// The version line, as shown.
pub fn version_line() -> String {
    i18n::format("about.version", &[("version", &env!("IBIS_VERSION"))])
}

/// The About content (logo, version, the update button, "Powered by ibisOffice" with its link, copyright, license),
/// the dialog's content.
fn about_body(cx: &App) -> Div {
    let theme = cx.theme().clone();
    v_flex()
        .items_center()
        .justify_center()
        .gap_2()
        .child(img(crate::chrome::app_logo()).h(px(34.)).w(px(34.0 * crate::chrome::LOGO_ASPECT)))
        .child(div().text_color(theme.muted_foreground).child(version_line()))
        // A newer version was found at start-up: a link-like button (no frame, no underline) asks the question again.
        .when(update_available(), |d| {
            d.child(div().id("about-update").text_color(gpui_kit::rgb(UPDATE_LINK_COLOR)).cursor_pointer().child(i18n::text("about.update-available")).on_click(|_, _, cx| offer_update_again(cx)))
        })
        .child(div().id("about-powered").mt_3().text_color(theme.link).underline().cursor_pointer().child(POWERED_BY).on_click(|_, _, cx| crate::external::open_url(PROJECT_URL, cx)))
        .child(div().id("about-url").text_size(px(12.)).text_color(theme.link).cursor_pointer().child(PROJECT_URL).on_click(|_, _, cx| crate::external::open_url(PROJECT_URL, cx)))
        .child(div().mt_3().text_size(px(12.)).text_color(theme.muted_foreground).child("Copyright (c) ibis inc."))
        .child(div().text_size(px(12.)).text_color(theme.muted_foreground).child(i18n::text("about.license")))
}

/// The About screen: a modal dialog inside the window (shell/dialog.rs `InteractionLayer::About`), over the window's
/// confirmation surface, the same on every platform; 閉じる, Esc and Enter close it.
pub(crate) fn render_modal(on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let footer = crate::chrome::action_bar().action(crate::chrome::ActionDefinition::new("about-close", i18n::text("_shell.close"), crate::chrome::ActionButtonKind::Normal).on_click(on_close));
    crate::dialog::confirmation_surface("about-dialog", false).child(
        v_flex()
            .id("about-dialog-box")
            .w(px(420.))
            .max_w_full()
            .rounded_lg()
            .overflow_hidden()
            .bg(theme.background)
            .text_color(theme.foreground)
            .border_1()
            .border_color(theme.border)
            .shadow_xl()
            .child(about_body(cx).py_6())
            .child(crate::chrome::action_bar_host(footer.access_scope(crate::access_key::Scope::MessageBox))),
    )
}
