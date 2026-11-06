// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ヘルプ > ibisOffice について (the app menu's ibisOffice について on macOS):
//! the version, "Powered by ibisOffice" with the link to the project, the
//! copyright and the license. The "Powered by ibisOffice" line and its link
//! are the additional term under AGPL-3.0 section 7(b) (NOTICE,
//! docs/TRADEMARK.md): a derivative keeps them on its own About screen.
use gpui_kit::component::{ActiveTheme as _, Root, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::shell::i18n;

/// What a derivative must show (NOTICE): the line and where the link goes.
pub(crate) const POWERED_BY: &str = "Powered by ibisOffice";
pub(crate) const PROJECT_URL: &str = "https://github.com/ibisjp/ibis-office";

/// The blue of the start view's links (`start_view.rs`): the 「新しいバージョンがあります。」 button.
const UPDATE_LINK_COLOR: u32 = 0x0f6cbd;

pub(crate) struct AboutWindow {
    focus: FocusHandle,
}

thread_local! {
    /// The About window, while it is open (one at a time).
    static OPEN: std::cell::RefCell<Option<WindowHandle<Root>>> = const { std::cell::RefCell::new(None) };
}

/// Open the About window in the middle of `parent` (the window the user is
/// in), or bring it to the front.
pub(crate) fn open_over(parent: Option<Bounds<Pixels>>, display: Option<DisplayId>, cx: &mut App) {
    if let Some(handle) = OPEN.with(|o| *o.borrow())
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    let (width, height) = (px(420.), px(if update_available() { 380. } else { 350. }));
    let bounds = match parent {
        Some(p) => Bounds::new(point(p.origin.x + (p.size.width - width) / 2.0, p.origin.y + (p.size.height - height) / 2.0), size(width, height)),
        None => Bounds::centered(None, size(width, height), cx),
    };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        display_id: display,
        titlebar: Some(TitlebarOptions { title: Some(i18n::text("help.about")), ..Default::default() }),
        ..Default::default()
    };
    let opened = cx.open_window(options, |window, cx| {
        crate::shell::palette::watch(window, cx);
        let view = cx.new(|cx| AboutWindow { focus: cx.focus_handle() });
        let focus = view.read(cx).focus.clone();
        focus.focus(window, cx);
        cx.new(|cx| Root::new(view, window, cx))
    });
    if let Ok(handle) = opened {
        OPEN.with(|o| *o.borrow_mut() = Some(handle));
    }
}

/// Open it over the window the user is in (the app menu's item).
pub(crate) fn open(cx: &mut App) {
    let parent = cx.active_window().and_then(|w| w.update(cx, |_, window, cx| (window.bounds(), window.display(cx).map(|d| d.id()))).ok());
    open_over(parent.map(|p| p.0), parent.and_then(|p| p.1), cx);
}

/// The About window, while it is open.
#[allow(dead_code)]
pub(crate) fn handle() -> Option<WindowHandle<Root>> {
    OPEN.with(|o| *o.borrow())
}

/// Whether the start-up check found a newer version (never in the browser).
pub(crate) fn update_available() -> bool {
    #[cfg(not(target_family = "wasm"))]
    {
        crate::shell::update::available_version().is_some()
    }
    #[cfg(target_family = "wasm")]
    {
        false
    }
}

/// The button's click: the question the start-up check asked, once more.
fn offer_update_again(_cx: &mut App) {
    #[cfg(not(target_family = "wasm"))]
    crate::shell::update::offer_again(_cx);
}

/// The version line, as shown.
pub(crate) fn version_line() -> String {
    i18n::format("about.version", &[("version", &env!("IBIS_VERSION"))])
}

/// The About content (logo, version, the update button, "Powered by ibisOffice" with its link, copyright, license),
/// the same in the window and in the in-window dialog of the browser.
fn about_body(cx: &App) -> Div {
    let theme = cx.theme().clone();
    v_flex()
        .items_center()
        .justify_center()
        .gap_2()
        .child(img(crate::shell::chrome::app_logo()).h(px(34.)).w(px(34.0 * crate::shell::chrome::LOGO_ASPECT)))
        .child(div().text_color(theme.muted_foreground).child(version_line()))
        // A newer version was found at start-up: a link-like button (no frame, no underline) asks the question again.
        .when(update_available(), |d| {
            d.child(div().id("about-update").text_color(gpui_kit::rgb(UPDATE_LINK_COLOR)).cursor_pointer().child(i18n::text("about.update-available")).on_click(|_, _, cx| offer_update_again(cx)))
        })
        .child(div().id("about-powered").mt_3().text_color(theme.link).underline().cursor_pointer().child(POWERED_BY).on_click(|_, _, cx| cx.open_url(PROJECT_URL)))
        .child(div().id("about-url").text_size(px(12.)).text_color(theme.link).cursor_pointer().child(PROJECT_URL).on_click(|_, _, cx| cx.open_url(PROJECT_URL)))
        .child(div().mt_3().text_size(px(12.)).text_color(theme.muted_foreground).child("Copyright (c) ibis inc."))
        .child(div().text_size(px(12.)).text_color(theme.muted_foreground).child(i18n::text("about.license")))
}

/// The browser has one window only (a second `open_window` fails), so there the About screen is a modal dialog inside the
/// window (shell/dialog.rs `InteractionLayer::About`), over the window's confirmation surface; 閉じる, Esc and Enter close it.
pub(crate) fn render_modal(on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let footer = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("about-close", i18n::text("_shell.close"), crate::shell::chrome::ActionButtonKind::Normal).on_click(on_close));
    crate::shell::dialog::confirmation_surface("about-dialog", false).child(
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
            .child(crate::shell::chrome::action_bar_host(footer)),
    )
}

impl Render for AboutWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(&i18n::text("help.about"));
        let theme = cx.theme().clone();
        // The close button's click and Esc / Enter (on the focused side) close this window.
        let close = |_: &ClickEvent, window: &mut Window, _: &mut App| window.remove_window();
        let footer = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("about-close", i18n::text("_shell.close"), crate::shell::chrome::ActionButtonKind::Normal).on_click(close));
        v_flex()
            .id("about")
            .track_focus(&self.focus)
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .on_key_down(|event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke;
                if (key.key == "escape" || key.key == "enter") && !key.modifiers.platform && !key.modifiers.control && !key.modifiers.alt && !key.modifiers.shift {
                    window.remove_window();
                    cx.stop_propagation();
                }
            })
            .child(about_body(cx).flex_1().min_h_0())
            .child(crate::shell::chrome::action_bar_host(footer))
    }
}
