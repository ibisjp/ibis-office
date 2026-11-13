// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The app's windows: opening an editor window, the start windows, quitting, and the app's menu bar.

use gpui_kit::component::Root;
use gpui_kit::*;

use crate::actions::*;
use crate::editor::SlideEditor;
use crate::linux_desktop;

/// The last window has closed (⌘W / ファイル > 閉じる on the start window, or the red button): the app ends with it,
/// on a Mac too. The scenarios record the request in `QUIT_REQUESTED` instead of ending.
pub fn quit_if_last_window_closed(cx: &mut App) {
    if !cx.windows().is_empty() {
        return;
    }
    #[cfg(feature = "scenarios")]
    QUIT_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(feature = "scenarios"))]
    cx.quit();
}

#[cfg(feature = "scenarios")]
pub(crate) static QUIT_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A native quit request must not destroy windows before their save questions are answered.
/// Once all documents have been saved/discarded, the next request is allowed synchronously.
pub fn native_quit_allowed(cx: &mut App) -> bool {
    let mut unsaved = false;
    for handle in cx.windows() {
        let Some(root) = handle.downcast::<Root>() else { continue };
        let editor = root.update(cx, |root, _, _| root.view().clone().downcast::<SlideEditor>().ok());
        match editor {
            Ok(Some(editor)) => unsaved |= editor.read(cx).is_dirty(),
            Ok(None) => {},
            // A borrowed window must never be mistaken for a clean window.
            Err(_) => unsaved = true,
        }
    }
    if unsaved {
        // The Dock can ask while the app is inactive; bring its save question into view.
        cx.activate(true);
        quit_asking_to_save(cx);
        false
    } else {
        true
    }
}

/// ibisOffice > 終了 (⌘Q): the windows with edits not saved stay and ask whether to save them (each goes once
/// answered); the others close. The app ends with its last window (`quit_if_last_window_closed`), or at once when
/// nothing is left to ask (GitHub issue #27).
pub fn quit_asking_to_save(cx: &mut App) {
    // ⌘Q comes while the window with the keys is being updated, when its editor cannot be read (the window would be
    // passed over and the app end without asking): ask once the event is done.
    cx.defer(quit_asking_to_save_now);
}

fn quit_asking_to_save_now(cx: &mut App) {
    let mut asking = false;
    for handle in cx.windows() {
        let Some(root) = handle.downcast::<Root>() else { continue };
        let editor = root.update(cx, |root, _, _| root.view().clone().downcast::<SlideEditor>().ok()).ok().flatten();
        let Some(editor) = editor else { continue };
        // A window that may close goes now; one that may not has opened the question.
        let close = editor.update(cx, |editor, cx| editor.window_should_close(cx));
        if close {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        } else {
            asking = true;
        }
    }
    if !asking {
        #[cfg(feature = "scenarios")]
        QUIT_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
        #[cfg(not(feature = "scenarios"))]
        cx.quit();
    }
}

/// The app's menu bar (macOS's), in the UI's language: set again when the
/// language changes.
pub fn set_app_menus(cx: &mut App) {
    use crate::i18n::text;
    cx.set_menus([
        Menu::new("ibisOffice").items([MenuItem::action(text("help.about"), OpenAbout), MenuItem::separator(), MenuItem::action(text("app.settings"), OpenSettings), MenuItem::separator(), MenuItem::action(text("app.quit"), Quit)]),
        Menu::new(text("menu.file")).items([
            MenuItem::action(text("file.start-window"), OpenStartWindow),
            MenuItem::separator(),
            MenuItem::action(text("app.file.new"), NewDocument),
            MenuItem::action(text("app.file.open"), OpenFile),
            MenuItem::action(text("app.file.save"), Save),
            MenuItem::action(text("app.file.save-as"), SaveAs),
            MenuItem::action(text("app.file.print"), PrintFile),
            MenuItem::action(text("app.file.close"), CloseFile),
        ]),
    ]);
}

/// The modifier of shortcuts such as undo: Command on a Mac, Control elsewhere.
/// GPUI's `secondary` picks it by the build target, so in the browser (not
/// macOS) it is always Control; ask the browser for its platform instead.
pub fn secondary_modifier() -> &'static str {
    #[cfg(target_family = "wasm")]
    return if crate::web::is_mac() { "cmd" } else { "ctrl" };
    #[cfg(not(target_family = "wasm"))]
    "secondary"
}

/// Whether the keys are a Mac's (the build target, or the browser's platform).
pub fn is_mac() -> bool {
    #[cfg(target_family = "wasm")]
    return crate::web::is_mac();
    #[cfg(not(target_family = "wasm"))]
    cfg!(target_os = "macos")
}

/// Open the editor window on `document` (a presentation and its file name),
/// showing slide number `slide` (1-based), or on the start view without one.
#[derive(Default)]
struct StartWindows(std::collections::HashMap<gpui_kit::EntityId,(gpui_kit::AnyWindowHandle,bool)>);
impl gpui_kit::Global for StartWindows {}
/// Keep start-view identity outside an editor borrow, so its own command can reuse it.
pub(crate) fn track_start_window(id:gpui_kit::EntityId,handle:Option<gpui_kit::AnyWindowHandle>,start:bool,cx:&mut App){
    if cx.try_global::<StartWindows>().is_none(){cx.set_global(StartWindows::default());}
    cx.update_global::<StartWindows,_>(|windows,_|{if let Some(handle)=handle{windows.0.insert(id,(handle,start));}else if let Some((_,state))=windows.0.get_mut(&id){*state=start;}});
}
pub fn open_editor(document: Option<(ibis_doc_model::Presentation, SharedString)>, slide: Option<usize>, cx: &mut App) {
    if document.is_none(){
        let alive=cx.windows();
        if cx.try_global::<StartWindows>().is_some(){cx.update_global::<StartWindows,_>(|s,_|s.0.retain(|_,(w,_)|alive.contains(w)));}
        let existing=cx.try_global::<StartWindows>().and_then(|s|s.0.values().find(|(w,start)|*start&&alive.contains(w)).map(|(w,_)|*w));
        if let Some(handle)=existing{let _=handle.update(cx,|_,w,_|w.activate_window());cx.activate(true);return;}
    }
    // A start window is as wide as its view (shell/start_view.rs).
    let width = if document.is_none() { crate::start_view::START_WINDOW_WIDTH } else { 1280. };
    // A start window looks for a newer version (its 「ibisOffice を更新する」 button), whatever the setting says.
    #[cfg(not(target_family = "wasm"))]
    if document.is_none() {
        crate::update::check_for_start_window(cx);
    }
    open_window_sized(width, cx, move |editor, window, cx| {
        if let Some((presentation, name)) = document {
            editor.load(presentation, name, window, cx);
        }
        if let Some(number) = slide {
            editor.show_slide_number(number, window, cx);
        }
    });
}

/// A new editor window, its editor set up by `setup`.
pub fn open_window_with(cx: &mut App, setup: impl FnOnce(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) {
    open_window_sized(1280., cx, setup);
}

/// A new editor window `width` pixels wide, its editor set up by `setup`.
fn open_window_sized(width: f32, cx: &mut App, setup: impl FnOnce(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) {
    let mut options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(width), px(800.)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            appears_transparent: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    linux_desktop::identify(&mut options);
    cx.open_window(options, |window, cx| {
        crate::palette::watch(window, cx);
        let view = cx.new(|cx| {
            let mut editor = SlideEditor::new(ibis_doc_model::Presentation::new(), window, cx);
            setup(&mut editor, window, cx);
            editor
        });
        let focus = view.read(cx).focus_handle.clone();
        focus.focus(window, cx);
        // The window's close button asks whether to save edits not saved (GitHub issue #27).
        let editor = view.downgrade();
        window.on_window_should_close(cx, move |_, cx| editor.update(cx, |editor, cx| editor.window_should_close(cx)).unwrap_or(true));
        // Root hosts gpui-component's overlays (tooltips, popovers, dialogs).
        cx.new(|cx| Root::new(view, window, cx))
    })
    .expect("failed to open the editor window");
    cx.activate(true);
}

/// Open `path` in the app: in the editor window if it shows no document
/// (the start view), else in a new window. A file a window already has open
/// is not opened again: that window is raised.
#[cfg(not(target_family = "wasm"))]
pub fn open_path_in_app(path: std::path::PathBuf, cx: &mut App) {
    // A file already open is not opened again (each window edits its own copy in memory, and saving one would
    // overwrite the other): its window comes to the front.
    if crate::open_guard::raise_if_open(&path, None, cx) {
        return;
    }
    for window in cx.windows() {
        let Some(root) = window.downcast::<Root>() else { continue };
        let editor = root.update(cx, |root, _, _| root.view().clone().downcast::<SlideEditor>().ok()).ok().flatten();
        if let Some(editor) = editor
            && !editor.read(cx).has_document
        {
            window
                .update(cx, |_, window, cx| editor.update(cx, |e, cx| e.open_path(path.clone(), None, window, cx)))
                .ok();
            return;
        }
    }
    open_editor_for(path, None, cx);
}

/// The Web version, opened as `?new=<ext>` (a new tab of ファイル > 新規作成): the page's editor shows a new file of `kind` (K0016).
#[cfg(target_family = "wasm")]
pub fn new_file_in_page(kind: ibis_pptx::Kind, cx: &mut App) {
    for window in cx.windows() {
        let Some(root) = window.downcast::<Root>() else { continue };
        let editor = root.update(cx, |root, _, _| root.view().clone().downcast::<SlideEditor>().ok()).ok().flatten();
        if let Some(editor) = editor {
            window.update(cx, |_, window, cx| editor.update(cx, |e, cx| e.new_file(kind, window, cx))).ok();
            return;
        }
    }
}

/// A new editor window that opens `path`, showing an empty document of its
/// kind while it loads, then slide `slide` (1-based) if given.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn open_editor_for(path: std::path::PathBuf, slide: Option<usize>, cx: &mut App) {
    open_window_with(cx, move |editor, window, cx| editor.open_path(path, slide, window, cx));
}
