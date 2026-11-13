// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibisOffice on the Linux desktop.
//!
//! - Its windows carry the app's id (X11's `WM_CLASS`, Wayland's app id
//!   `ibisoffice`), which ties them to the app's `.desktop` entry
//!   (`StartupWMClass`), and the app's icon.
//! - Run from its AppImage (`APPIMAGE` and `APPDIR` set by its runtime,
//!   `crates/ibis-appimage`), the first start asks 「アプリの一覧に登録しますか？」;
//!   「はい」 writes the AppImage's `.desktop` entry (`linux/ibisoffice.desktop`,
//!   with the AppImage as the command) to `~/.local/share/applications/`
//!   and its icons to `~/.local/share/icons/hicolor/`, which puts ibisOffice
//!   in the desktop's list of apps and in the 「別のアプリで開く」 list of the
//!   documents it opens (it does not become their default app); without the
//!   entry the dock shows no icon, so Help's "Add to List of Apps" asks again
//!   for who said no. The answer
//!   is kept in the settings (`linux-desktop`: `no`, or the AppImage
//!   registered), and Help's 「アプリの一覧に登録」 asks again; an AppImage moved elsewhere is registered again at its
//!   next start, without asking.

use gpui_kit::{App, WindowOptions};

/// Draw the questions apps ask (`Window::prompt`) as ibisOffice's own
/// dialogs on Linux, which has no system dialogs for them: GPUI's stand-in
/// is narrow and does not wrap the explanation.
#[cfg(target_os = "linux")]
pub fn install_prompts(cx: &mut App) {
    cx.set_prompt_builder(prompt::build);
}

#[cfg(not(target_os = "linux"))]
pub fn install_prompts(_: &mut App) {}

#[cfg(target_os = "linux")]
mod prompt {
    use gpui_kit::component::button::{Button, ButtonVariants as _};
    use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
    use gpui_kit::*;

    pub(super) fn build(
        _level: PromptLevel,
        message: &str,
        detail: Option<&str>,
        actions: &[PromptButton],
        handle: PromptHandle,
        window: &mut Window,
        cx: &mut App,
    ) -> RenderablePromptHandle {
        let view = cx.new(|cx| PromptDialog {
            message: message.to_string().into(),
            detail: detail.map(|d| d.to_string().into()),
            actions: actions.to_vec(),
            focus: cx.focus_handle(),
        });
        handle.with_view(view, window, cx)
    }

    struct PromptDialog {
        message: SharedString,
        detail: Option<SharedString>,
        actions: Vec<PromptButton>,
        focus: FocusHandle,
    }

    impl PromptDialog {
        /// The answer Enter gives (the first button that is not a cancel) and Escape gives (the cancel).
        fn default_answer(&self) -> Option<usize> {
            self.actions.iter().position(|a| !a.is_cancel())
        }

        fn cancel_answer(&self) -> Option<usize> {
            self.actions.iter().position(PromptButton::is_cancel)
        }
    }

    impl EventEmitter<PromptResponse> for PromptDialog {}

    impl Focusable for PromptDialog {
        fn focus_handle(&self, _: &App) -> FocusHandle {
            self.focus.clone()
        }
    }

    impl Render for PromptDialog {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = cx.theme().clone();
            let default = self.default_answer();
            // Buttons right-aligned, the default one last and marked (as GNOME puts them).
            let mut order: Vec<usize> = (0..self.actions.len()).filter(|&i| Some(i) != default).collect();
            order.extend(default);
            let buttons = order.into_iter().map(|ix| {
                let button = Button::new(("prompt-button", ix)).label(self.actions[ix].label().clone()).on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(PromptResponse(ix));
                    cx.stop_propagation();
                }));
                if Some(ix) == default { button.primary() } else { button }
            });
            let dialog = v_flex()
                .id("prompt")
                .track_focus(&self.focus)
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    let answer = match event.keystroke.key.as_str() {
                        "enter" => this.default_answer(),
                        "escape" => this.cancel_answer(),
                        _ => None,
                    };
                    if let Some(ix) = answer {
                        cx.emit(PromptResponse(ix));
                        cx.stop_propagation();
                    }
                }))
                .w(px(440.))
                .max_w(relative(0.9))
                .p_5()
                .gap_3()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.background)
                .text_color(theme.foreground)
                .shadow_lg()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(self.message.clone()))
                .children(self.detail.clone().map(|detail| div().text_sm().text_color(theme.muted_foreground).child(detail)))
                .child(h_flex().mt_2().gap_2().justify_end().children(buttons));
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui_kit::black().opacity(0.3))
                .child(dialog)
        }
    }
}

/// The app's id on the Linux desktop (`StartupWMClass` in its `.desktop` entry).
const APP_ID: &str = "ibisoffice";

/// Give a window's options the app's id and icon (Linux).
pub(crate) fn identify(options: &mut WindowOptions) {
    if cfg!(target_os = "linux") {
        options.app_id = Some(APP_ID.to_string());
        options.window_decorations = Some(gpui_kit::WindowDecorations::Client);
        #[cfg(all(target_os = "linux", feature = "export"))]
        {
            options.icon = window_icon();
        }
    }
}

/// The icon X11 shows for the windows (`_NET_WM_ICON`) when no `.desktop`
/// entry gives one: Windows' icon without its plate, at 48 pixels (at 256,
/// 262 KB, the property is more than one X request takes, and GPUI sets it
/// empty).
#[cfg(all(target_os = "linux", feature = "export"))]
fn window_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
    static ICON: std::sync::OnceLock<Option<std::sync::Arc<image::RgbaImage>>> = std::sync::OnceLock::new();
    ICON.get_or_init(|| {
        let png = include_bytes!("../logo/png/ico/ibisOffice_icon_win_48px_transparent.png");
        image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .map_err(|e| log::warn!("the window icon: {e}"))
            .ok()
            .map(|icon| std::sync::Arc::new(icon.to_rgba8()))
    })
    .clone()
}

/// Called once the first window is open: offer to put the AppImage in the
/// list of apps, or register it again where it has moved (see the module's docs).
#[cfg(target_os = "linux")]
pub fn start(cx: &mut App) {
    use crate::settings;

    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(std::path::PathBuf::from);
    let (Some(appimage), Some(appdir)) = (env("APPIMAGE"), env("APPDIR")) else { return };
    {
        let appdir = appdir.clone();
        std::thread::spawn(move || remove_old_unpacked(&appdir));
    }
    let registered = settings::get(KEY).unwrap_or_default();
    if registered == "no" || std::path::Path::new(&registered) == appimage {
        return;
    }
    if !registered.is_empty() {
        // Registered before, from another place: follow it.
        register_logged(&appimage, &appdir);
        return;
    }
    ask(cx, appimage, appdir);
}

/// Ask again whether to put the AppImage in the list of apps (Help menu), for
/// who said no at the first start: without the entry the dock (GNOME's, on
/// Wayland and X11) cannot find the app's icon. Does nothing outside an AppImage.
#[cfg(target_os = "linux")]
pub(crate) fn register_again(cx: &mut App) {
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(std::path::PathBuf::from);
    if let (Some(appimage), Some(appdir)) = (env("APPIMAGE"), env("APPDIR")) {
        ask(cx, appimage, appdir);
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn register_again(_: &mut App) {}

/// Whether this copy runs from an AppImage (the Help menu's item is shown only then).
pub(crate) fn is_appimage() -> bool {
    cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_some_and(|v| !v.is_empty()) && std::env::var_os("APPDIR").is_some_and(|v| !v.is_empty())
}

#[cfg(target_os = "linux")]
fn ask(cx: &mut App, appimage: std::path::PathBuf, appdir: std::path::PathBuf) {
    use crate::{i18n, settings};
    use gpui_kit::{AsyncApp, PromptButton, PromptLevel};
    cx.spawn(async move |cx: &mut AsyncApp| {
        let receiver = cx.update(|cx| {
            let window = cx.active_window().or_else(|| cx.windows().first().copied())?;
            let buttons = [PromptButton::ok(i18n::text("shell.linux-desktop.yes")), PromptButton::cancel(i18n::text("shell.linux-desktop.no"))];
            let detail = i18n::text("shell.linux-desktop.ask.detail");
            window
                .update(cx, |_, window, cx| window.prompt(PromptLevel::Info, &i18n::text("shell.linux-desktop.ask"), Some(&detail), &buttons, cx))
                .ok()
        });
        let Some(receiver) = receiver else { return };
        match receiver.await {
            Ok(0) => {
                if register_logged(&appimage, &appdir) {
                    settings::set(KEY, &appimage.to_string_lossy());
                }
            }
            Ok(_) => settings::set(KEY, "no"),
            Err(_) => {}
        }
    })
    .detach();
}

#[cfg(not(target_os = "linux"))]
pub fn start(_: &mut App) {}

/// Remove the other versions the AppImage's runtime unpacked beside
/// `appdir` (`~/.cache/ibisOffice/app/<id>/`, left by updates), and the
/// leftovers of unpacking stopped half way, except those a running program
/// is from (an older copy still open).
#[cfg(target_os = "linux")]
fn remove_old_unpacked(appdir: &std::path::Path) {
    let Some(root) = appdir.parent() else { return };
    // Only in the runtime's own folder.
    if root.file_name().is_none_or(|n| n != "app") || root.parent().and_then(|p| p.file_name()).is_none_or(|n| n != "ibisOffice") {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else { return };
    let in_use: Vec<std::path::PathBuf> = std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|p| std::fs::read_link(p.path().join("exe")).ok())
        .collect();
    for entry in entries.flatten() {
        let path = entry.path();
        if path == appdir || in_use.iter().any(|exe| exe.starts_with(&path)) {
            continue;
        }
        match std::fs::remove_dir_all(&path) {
            Ok(()) => log::info!("removed the old unpacked version {}", path.display()),
            Err(err) => log::warn!("{}: {err}", path.display()),
        }
    }
}

/// The settings' key of the answer (`no`, or the AppImage registered).
#[cfg(target_os = "linux")]
const KEY: &str = "linux-desktop";

#[cfg(target_os = "linux")]
fn register_logged(appimage: &std::path::Path, appdir: &std::path::Path) -> bool {
    match register(appimage, appdir, &data_home()) {
        Ok(()) => true,
        Err(err) => {
            log::warn!("registering ibisOffice on the desktop: {err}");
            false
        }
    }
}

/// `$XDG_DATA_HOME` (`~/.local/share`).
#[cfg(target_os = "linux")]
fn data_home() -> std::path::PathBuf {
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(std::path::PathBuf::from);
    env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share"))).unwrap_or_default()
}

/// The sizes of the icons in the AppImage (`usr/share/icons/hicolor/<n>x<n>/apps/ibisoffice.png`).
#[cfg(target_os = "linux")]
const ICON_SIZES: &[u32] = &[16, 24, 32, 48, 256];

/// Write the AppImage's `.desktop` entry, starting `appimage`, and its
/// icons from `appdir` under `data` (`~/.local/share`).
#[cfg(target_os = "linux")]
fn register(appimage: &std::path::Path, appdir: &std::path::Path, data: &std::path::Path) -> Result<(), String> {
    let err = |path: &std::path::Path, e: std::io::Error| format!("{}: {e}", path.display());
    let template_path = appdir.join(format!("{APP_ID}.desktop"));
    let template = std::fs::read_to_string(&template_path).map_err(|e| err(&template_path, e))?;
    let entry = desktop_entry(&template, appimage);
    for size in ICON_SIZES {
        let rel = format!("icons/hicolor/{size}x{size}/apps/{APP_ID}.png");
        let to = data.join(&rel);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|e| err(dir, e))?;
        }
        std::fs::copy(appdir.join("usr/share").join(&rel), &to).map_err(|e| err(&to, e))?;
    }
    let apps = data.join("applications");
    std::fs::create_dir_all(&apps).map_err(|e| err(&apps, e))?;
    let path = apps.join(format!("{APP_ID}.desktop"));
    let partial = apps.join(format!(".{APP_ID}.desktop.partial"));
    std::fs::write(&partial, entry).map_err(|e| err(&partial, e))?;
    std::fs::rename(&partial, &path).map_err(|e| err(&path, e))?;
    // The cache of which apps open which types, where the desktop keeps one
    // (not all have the tool; the entry works without it).
    if crate::external::may_launch(|| format!("update-desktop-database {}", apps.display())) {
        let _ = std::process::Command::new("update-desktop-database").arg(&apps).status();
    }
    Ok(())
}

/// The `.desktop` entry `template` with its command (`Exec`) starting `appimage`.
#[cfg(target_os = "linux")]
fn desktop_entry(template: &str, appimage: &std::path::Path) -> String {
    let mut out = String::new();
    for line in template.lines() {
        if line.starts_with("Exec=") {
            out.push_str(&format!("Exec={} %F", exec_argument(&appimage.to_string_lossy())));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// A path as an argument of a `.desktop` entry's `Exec`: quoted, with `"`
/// `` ` `` `$` `\` escaped by a backslash, then the value's own escapes
/// (`\` doubled) and `%` doubled (the Desktop Entry Specification).
#[cfg(target_os = "linux")]
fn exec_argument(path: &str) -> String {
    let mut quoted = String::from("\"");
    for c in path.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted.replace('\\', "\\\\").replace('%', "%%")
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn the_command_is_the_appimage_quoted() {
        assert_eq!(exec_argument("/home/a/ibisOffice.AppImage"), "\"/home/a/ibisOffice.AppImage\"");
        assert_eq!(exec_argument("/home/a b/x$y%z\"q"), "\"/home/a b/x\\\\$y%%z\\\\\"q\"");
    }

    #[test]
    fn registering_writes_the_entry_and_the_icons() {
        let root = std::env::temp_dir().join(format!("ibis-linux-desktop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let appdir = root.join("AppDir");
        std::fs::create_dir_all(&appdir).unwrap();
        std::fs::write(appdir.join("ibisoffice.desktop"), include_str!("../../ibis-office-app/linux/ibisoffice.desktop")).unwrap();
        for size in ICON_SIZES {
            let dir = appdir.join(format!("usr/share/icons/hicolor/{size}x{size}/apps"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("ibisoffice.png"), b"png").unwrap();
        }
        let data = root.join("share");
        register(std::path::Path::new("/opt/apps/ibisOffice.AppImage"), &appdir, &data).unwrap();
        let entry = std::fs::read_to_string(data.join("applications/ibisoffice.desktop")).unwrap();
        assert!(entry.contains("\nExec=\"/opt/apps/ibisOffice.AppImage\" %F\n"));
        assert!(entry.contains("\nStartupWMClass=ibisoffice\n"));
        assert!(entry.contains("application/vnd.openxmlformats-officedocument.wordprocessingml.document;"));
        assert!(data.join("icons/hicolor/256x256/apps/ibisoffice.png").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
