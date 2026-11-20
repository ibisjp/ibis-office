// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! [`FormatEditor`] implementation for `Kind::Pdf`.

use std::sync::{Arc, Mutex};

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarAxis};
use gpui_kit::{
    AnyElement, AppContext as _, Bounds, ClipboardItem, Focusable as _, Context, Entity, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point,
    SharedString, Size, StatefulInteractiveElement as _, Styled, Subscription, Window, canvas,
    div, fill, hsla, point, px, rgb, size,
};
use ibis_doc_model::Presentation;
use ibis_office_shell::chrome::MenuId;
use ibis_office_shell::commands::KindMask;
use ibis_office_shell::editor::{SlideEditor, WorkspaceContent};
use ibis_office_shell::format::{
    FormatEditor, FormatInfo, OfficeApp, ShapeRules, impl_format_state,
};
use ibis_office_shell::menu_context::{ContextMenu, ContextState, ContextTone};

use crate::PdfViewerState;

/// PDF document format driver.
pub struct PdfDocument;

/// Per-window PDF viewer state held in `SlideEditor::format`.
pub struct State {
    pub viewer: Arc<Mutex<PdfViewerState>>,
    /// Where the pages were drawn last (for the pointer), and the drag in progress.
    pub ui: Arc<Mutex<Ui>>,
    /// The search bar's field and the bar of the page number's field, and whether they are shown.
    pub find_input: Entity<InputState>,
    pub page_input: Entity<InputState>,
    pub find_open: bool,
    pub page_open: bool,
    _subscriptions: Vec<Subscription>,
}

/// What the drawing of the pages tells the pointer: the canvas's place in the window (its left, top and width) and the drag.
#[derive(Default)]
pub struct Ui {
    pub canvas: Option<(f32, f32, f32)>,
    pub dragging: bool,
}

/// A PDF is only looked at: no touch gesture changes anything of the state the viewer keeps (the scroll is the workspace's).
impl ibis_office_shell::format::GestureView for State {
    type Snapshot = ();

    fn gesture_snapshot(&self) {
        let State { viewer: _, ui: _, find_input: _, page_input: _, find_open: _, page_open: _, _subscriptions: _ } = self;
    }

    fn gesture_restore(&mut self, _: ()) {}
}

impl_format_state!(State, ibis_formats::Kind::Pdf);

const PAGE_GAP_PX: f32 = 24.0;
const PAGE_PAD_PX: f32 = 32.0;

fn viewer_of(editor: &SlideEditor) -> Option<Arc<Mutex<PdfViewerState>>> {
    editor.format_state::<State>().map(|s| s.viewer.clone())
}

/// Pixels per point of the pages at the screen's zoom.
fn page_scale(editor: &SlideEditor) -> f32 {
    editor.zoom.unwrap_or(1.0).max(0.1) * (96.0 / 72.0)
}

/// The top of each page in the content of the workspace (from the top of the content) and its height, in pixels.
fn page_tops(viewer: &PdfViewerState, scale: f32) -> Vec<(f32, f32)> {
    let mut y = PAGE_PAD_PX;
    viewer
        .pages
        .iter()
        .map(|p| {
            let h = p.crop_box.height() * scale;
            let top = y;
            y += h + PAGE_GAP_PX;
            (top, h)
        })
        .collect()
}

/// The page a point of the window is over (the nearest, above the first or below the last), and the point in page points.
fn pdf_point(editor: &SlideEditor, at: Point<Pixels>) -> Option<(usize, f32, f32)> {
    let state = editor.format_state::<State>()?;
    let (left, top, width) = state.ui.lock().expect("pdf ui lock").canvas?;
    let scale = page_scale(editor);
    let guard = state.viewer.lock().expect("pdf viewer lock");
    let tops = page_tops(&guard, scale);
    let (y, x) = (f32::from(at.y), f32::from(at.x));
    let page = tops.iter().position(|(t, h)| y < top + t + h + PAGE_GAP_PX / 2.0).unwrap_or(tops.len().checked_sub(1)?);
    let pw = guard.pages[page].crop_box.width() * scale;
    let page_left = left + ((width - pw) * 0.5).max(PAGE_PAD_PX);
    Some((page, (x - page_left) / scale, (y - top - tops[page].0) / scale))
}

/// The page at the top of what is shown (the one the status bar says).
fn page_at_top(editor: &SlideEditor) -> Option<usize> {
    let viewer = viewer_of(editor)?;
    let guard = viewer.lock().expect("pdf viewer lock");
    let tops = page_tops(&guard, page_scale(editor));
    let seen = -f32::from(editor.workspace_scroll.offset().y) + PAGE_GAP_PX;
    tops.iter().position(|(t, h)| seen < t + h + PAGE_GAP_PX / 2.0).or(tops.len().checked_sub(1))
}

/// Scroll so that `page` (and the point `y_pt` down from its top) is at the top of the view.
fn scroll_to(editor: &mut SlideEditor, page: usize, y_pt: f32) {
    let Some(viewer) = viewer_of(editor) else { return };
    let scale = page_scale(editor);
    let top = {
        let guard = viewer.lock().expect("pdf viewer lock");
        let tops = page_tops(&guard, scale);
        let Some((t, _)) = tops.get(page) else { return };
        t + y_pt * scale
    };
    let offset = editor.workspace_scroll.offset();
    editor.workspace_scroll.set_offset(point(offset.x, px(-(top - PAGE_PAD_PX).max(0.0))));
    editor.clamp_workspace_scroll();
}

/// Show page `page` (0-based) at the top, and make it the page of the screen.
fn go_to_page(editor: &mut SlideEditor, page: usize, cx: &mut Context<SlideEditor>) {
    let last = editor.presentation().slides.len().saturating_sub(1);
    let page = page.min(last);
    scroll_to(editor, page, 0.0);
    editor.current_slide = editor.presentation().slides[page].id;
    cx.notify();
}

/// The next (or previous) place the search found, shown.
fn find_step(editor: &mut SlideEditor, forward: bool, cx: &mut Context<SlideEditor>) {
    let Some(viewer) = viewer_of(editor) else { return };
    let target = {
        let mut guard = viewer.lock().expect("pdf viewer lock");
        let Some(i) = guard.step_find(forward) else { return };
        let m = guard.find_matches[i];
        let y = guard.char_rect(m.page, m.start).map_or(0.0, |r| r.y0.min(r.y1));
        (m.page, y)
    };
    // A little above the place, so that the line before it shows too.
    scroll_to(editor, target.0, (target.1 - 40.0).max(0.0));
    cx.notify();
}

fn open_find(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
    // The text picked goes into the field, as in the other screens' ⌘F.
    let picked = viewer_of(editor).map(|v| v.lock().expect("pdf viewer lock").selected_text()).filter(|t| !t.is_empty() && !t.contains('\n'));
    let Some(state) = editor.format_state_mut::<State>() else { return };
    state.find_open = true;
    state.page_open = false;
    let input = state.find_input.clone();
    input.update(cx, |field, cx| {
        if let Some(text) = picked {
            field.set_value(text, window, cx);
        }
        field.focus(window, cx);
        field.select_all(window, cx);
    });
    cx.notify();
}

fn open_go_to(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
    let page = page_at_top(editor).map_or(1, |p| p + 1);
    let Some(state) = editor.format_state_mut::<State>() else { return };
    state.page_open = true;
    state.find_open = false;
    let input = state.page_input.clone();
    input.update(cx, |field, cx| {
        field.set_value(page.to_string(), window, cx);
        field.focus(window, cx);
        field.select_all(window, cx);
    });
    cx.notify();
}

fn copy_selection(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) {
    let Some(viewer) = viewer_of(editor) else { return };
    let text = viewer.lock().expect("pdf viewer lock").selected_text();
    if !text.is_empty() {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}

fn select_all(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) {
    if let Some(viewer) = viewer_of(editor) {
        viewer.lock().expect("pdf viewer lock").select_all();
    }
    cx.notify();
}

/// The place of the character `index` of `page`, in the window (the middle of its box), for the scenarios that press the pointer on text.
pub fn char_position(editor: &SlideEditor, page: usize, index: usize) -> Option<(f32, f32)> {
    let state = editor.format_state::<State>()?;
    let (left, top, width) = state.ui.lock().expect("pdf ui lock").canvas?;
    let scale = page_scale(editor);
    let mut guard = state.viewer.lock().expect("pdf viewer lock");
    let tops = page_tops(&guard, scale);
    let r = guard.char_rect(page, index)?;
    let pw = guard.pages[page].crop_box.width() * scale;
    let page_left = left + ((width - pw) * 0.5).max(PAGE_PAD_PX);
    Some((page_left + (r.x0 + r.x1) * 0.5 * scale, top + tops[page].0 + (r.y0 + r.y1) * 0.5 * scale))
}

/// The text picked (for the scenarios that press the pointer on text).
pub fn selected_text(editor: &SlideEditor) -> String {
    viewer_of(editor).map(|v| v.lock().expect("pdf viewer lock").selected_text()).unwrap_or_default()
}

/// What the search shows: the number of places found and the one shown (0-based), when the bar is open.
pub fn find_status(editor: &SlideEditor) -> Option<(usize, Option<usize>)> {
    let state = editor.format_state::<State>()?;
    if !state.find_open {
        return None;
    }
    let guard = state.viewer.lock().expect("pdf viewer lock");
    Some((guard.find_matches.len(), guard.find_current))
}

fn outer_contexts(_state: &ContextState) -> Vec<ContextMenu> {
    Vec::new()
}

const fn empty_menus() -> &'static [ibis_office_shell::sidebars::SidebarDef] {
    &[]
}

impl FormatEditor for PdfDocument {
    const INFO: &'static FormatInfo = &FormatInfo {
        order: 3,
        extension: "pdf",
        new_file_name: || "Document.pdf".into(),
        untitled_key: "_shell.document-1-docx",
        office: OfficeApp {
            name: "PDF",
            setting_key: "office_path.pdf",
            #[cfg(target_os = "macos")]
            bundle: "Preview.app",
            #[cfg(target_os = "windows")]
            exe: "msedge.exe",
            #[cfg(target_os = "macos")]
            open_key: "window.open-pdf-in-preview",
            #[cfg(target_os = "windows")]
            open_key: "window.open-pdf-in-edge",
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            logo_svg: b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'/>",
        },
        new_command: "file.new.document",
        // The screen of a document (Word's), whole: what a PDF cannot do is grey (`menu_items` and the buttons below), not left out.
        command_menus: &[MenuId::File, MenuId::Edit, MenuId::View, MenuId::Insert, MenuId::Format, MenuId::Tools, MenuId::Window, MenuId::Help],
        icon: "picture_as_pdf",
        color: 0xc93b32,
        context_tones: [
            ContextTone {
                fg: 0x9c2821,
                bg: 0xc93b32,
                bg_alpha: 0.12,
            },
            ContextTone {
                fg: 0xf48b84,
                bg: 0xf48b84,
                bg_alpha: 0.16,
            },
        ],
        menus: &ibis_office_shell::menu_layout::DOCUMENT_MENUS,
        fold_order: &[],
        sidebars: empty_menus(),
        mask: KindMask::PDF,
        several_windows: None,
        shapes: ShapeRules {
            in_shell: false,
            several: false,
            try_first: false,
            cmyk: false,
            break_is_typed: false,
        },
        outer_contexts,
        font_substitutes: false,
        drawing_canvas: false,
        new_badge_color: 0xc93b32,
        search_history_key: "menu-search.history.pdf",
        px_per_inch_at_100: 96.0,
        #[cfg(not(target_family = "wasm"))]
        // A file that is looked at is not saved as another kind (a copy of it is the file as it is: `bytes_for_path`).
        save_formats: &[],
    };

    const PAGE_NOUN: &'static str = "docx.mod.page";
    const FIT_TOOLTIP: &'static str = "docx.mod.zoom-page-width";
    const SHAPES: bool = false;

    fn new_state(
        presentation: &Presentation,
        window: &mut Window,
        cx: &mut Context<SlideEditor>,
    ) -> Box<dyn ibis_office_shell::format::FormatState> {
        let viewer = presentation
            .pdf_bytes
            .as_ref()
            .filter(|bytes| !bytes.is_empty())
            .and_then(|bytes| PdfViewerState::open(bytes.clone()).ok())
            .unwrap_or_else(PdfViewerState::empty);
        let find_input = cx.new(|cx| InputState::new(window, cx).placeholder(ibis_office_shell::i18n::text("_shell.find-what")));
        let page_input = cx.new(|cx| InputState::new(window, cx).placeholder(ibis_office_shell::i18n::text("shell.pdf.page-number")));
        let subscriptions = vec![
            cx.subscribe_in(&find_input, window, |editor: &mut SlideEditor, input, event: &InputEvent, window, cx| match event {
                // The text typed is looked for at once, on all the pages.
                InputEvent::Change => {
                    let query = input.read(cx).value().to_string();
                    if let Some(viewer) = viewer_of(editor) {
                        let mut guard = viewer.lock().expect("pdf viewer lock");
                        if guard.find_query != query {
                            guard.find_query = query.clone();
                            guard.run_find(&query);
                        }
                    }
                    cx.notify();
                }
                // Enter is the next place, Shift+Enter the previous one; the field keeps the keys.
                InputEvent::PressEnter { shift, .. } => {
                    find_step(editor, !*shift, cx);
                    input.update(cx, |field, cx| field.focus(window, cx));
                }
                _ => {}
            }),
            cx.subscribe_in(&page_input, window, |editor: &mut SlideEditor, input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let typed = input.read(cx).value().to_string();
                    if let Ok(number) = typed.trim().parse::<usize>() {
                        go_to_page(editor, number.saturating_sub(1), cx);
                        if let Some(state) = editor.format_state_mut::<State>() {
                            state.page_open = false;
                        }
                        window.focus(&editor.focus_handle, cx);
                    }
                    cx.notify();
                }
            }),
        ];
        Box::new(State {
            viewer: Arc::new(Mutex::new(viewer)),
            ui: Arc::new(Mutex::new(Ui::default())),
            find_input,
            page_input,
            find_open: false,
            page_open: false,
            _subscriptions: subscriptions,
        })
    }

    /// A copy of the file, or the file saved, is the file as it was opened (it is looked at, not changed): the bytes of the original, whatever the
    /// extension of the name.
    #[cfg(not(target_family = "wasm"))]
    fn bytes_for_path(
        editor: &SlideEditor,
        _path: &std::path::Path,
        _source: &ibis_formats::Source,
        _copy: bool,
    ) -> Option<Result<ibis_office_shell::format::FileBytes, String>> {
        let original = editor.presentation().pdf_bytes.as_ref()?;
        Some(Ok(ibis_office_shell::format::FileBytes { bytes: original.to_vec(), protect: false, note: None }))
    }

    /// What a PDF that is looked at cannot do is grey: the document's own commands (which have no handler here), and the shared ones that write the
    /// document or draw its pages again (save, protect, the pictures of the pages: a PDF has the pages of its file only).
    fn menus(_editor: &SlideEditor, menus: &mut ibis_office_shell::chrome::Menus) {
        fn grey(entries: &mut [ibis_office_shell::chrome::MenuEntry]) {
            use ibis_office_shell::chrome::MenuEntry;
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) if item.id.starts_with("docx.") || matches!(item.id, "file.save" | "file.save-as" | "file.protect" | "file.export.png" | "file.export.gif" | "file.export.svg") => item.disabled = true,
                    MenuEntry::Submenu(sub) => grey(&mut sub.items),
                    _ => {}
                }
            }
        }
        for (_, entries) in &mut menus.menus {
            grey(entries);
        }
        // What a PDF that is looked at can do with its text: pick it (all of it), copy it, search it, go to a page.
        let has_text = _editor.format_state::<State>().is_some_and(|s| s.viewer.lock().expect("pdf viewer lock").has_text());
        let selected = _editor.format_state::<State>().is_some_and(|s| s.viewer.lock().expect("pdf viewer lock").selection.is_some());
        menus.connect("edit.copy", |this, _, cx| copy_selection(this, cx));
        menus.connect("edit.select-all", |this, _, cx| select_all(this, cx));
        menus.connect("edit.find-replace", |this, window, cx| open_find(this, window, cx));
        // ジャンプ… (the number of a page) is not in the document's Edit menu: it is put after 検索.
        {
            use ibis_office_shell::chrome::{MenuEntry, MenuId, MenuItem};
            let entries = menus.items_mut(MenuId::Edit);
            if !entries.iter().any(|e| matches!(e, MenuEntry::Item(i) if i.id == "edit.go-to")) {
                let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "edit.find-replace")).map_or(entries.len(), |i| i + 1);
                entries.insert(at, MenuItem::tr("edit.go-to").icon("arrow_forward").on(|this, window, cx| open_go_to(this, window, cx)).into());
            }
        }
        if let Some(item) = menus.find_mut("edit.copy") {
            item.disabled = !selected;
        }
        // Select all and Find need text somewhere in the file (a scanned document of pictures has none); Go to a page needs only pages.
        let has_pages = _editor.format_state::<State>().is_some_and(|s| !s.viewer.lock().expect("pdf viewer lock").pages.is_empty());
        for (id, enabled) in [("edit.select-all", has_text), ("edit.find-replace", has_text), ("edit.go-to", has_pages)] {
            if let Some(item) = menus.find_mut(id) {
                item.disabled = !enabled;
            }
        }
    }

    fn menu_items(editor: &SlideEditor, part: ibis_office_shell::chrome::MenuPart) -> Vec<ibis_office_shell::chrome::MenuEntry> {
        ibis_office_docx::viewer_menu_items(editor, part)
    }

    fn toolbar_left(_: &SlideEditor) -> Vec<ibis_office_shell::chrome::ToolItem> {
        grey_buttons(ibis_office_docx::viewer_toolbar_left())
    }

    fn toolbar_right(editor: &SlideEditor) -> Vec<ibis_office_shell::chrome::ToolItem> {
        grey_buttons(ibis_office_docx::viewer_toolbar_right(editor))
    }

    /// The keys the menus name as their shortcuts do what the menu item does (⌘C, ⌘A, ⌘F...): the screen has no text of its own to type in
    /// but the two small fields; with the cursor in one of them the keys are its own.
    fn format_key(editor: &mut SlideEditor, keystroke: &gpui_kit::Keystroke, window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        let in_field = editor.format_state::<State>().is_some_and(|s| s.find_input.read(cx).focus_handle(cx).is_focused(window) || s.page_input.read(cx).focus_handle(cx).is_focused(window));
        if in_field && !(keystroke.modifiers.platform && keystroke.key == "f") {
            return false;
        }
        // PageDown and PageUp: the next and the previous page.
        let m = &keystroke.modifiers;
        if !in_field && !(m.platform || m.control || m.alt || m.shift) && matches!(keystroke.key.as_str(), "pagedown" | "pageup") {
            return Self::step_page(editor, keystroke.key == "pagedown", window, cx);
        }
        editor.run_menu_shortcut(keystroke, window, cx)
    }

    /// The page of the screen follows what is shown (the status bar says it).
    fn before_render(editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) {
        if let Some(page) = page_at_top(editor)
            && let Some(slide) = editor.presentation().slides.get(page)
            && slide.id != editor.current_slide
        {
            editor.current_slide = slide.id;
        }
    }

    /// A page chosen elsewhere (the page's number, the next page) is shown at the top.
    fn show_page(editor: &mut SlideEditor, slide: ibis_doc_model::SlideId, _window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        match editor.presentation().slide_index(slide) {
            Some(index) => {
                go_to_page(editor, index, cx);
                true
            }
            None => false,
        }
    }

    /// PageDown / PageUp: the next / the previous page.
    fn step_page(editor: &mut SlideEditor, forward: bool, _window: &mut Window, cx: &mut Context<SlideEditor>) -> bool {
        let Some(page) = page_at_top(editor) else { return false };
        go_to_page(editor, if forward { page + 1 } else { page.saturating_sub(1) }, cx);
        true
    }

    fn workspace_content(editor: &SlideEditor, workspace: Size<Pixels>) -> WorkspaceContent {
        let zoom = editor.zoom.unwrap_or(1.0).max(0.1);
        let scale = zoom * (96.0 / 72.0);
        let Some(state) = editor.format_state::<State>() else {
            return WorkspaceContent {
                size: workspace,
                slide_origin: point(px(0.0), px(0.0)),
                scale: 1.0,
            };
        };
        let guard = state.viewer.lock().expect("pdf viewer lock");
        let mut max_w = 0.0f32;
        let mut total_h = PAGE_PAD_PX;
        for page in &guard.pages {
            max_w = max_w.max(page.crop_box.width() * scale);
            total_h += page.crop_box.height() * scale + PAGE_GAP_PX;
        }
        total_h += PAGE_PAD_PX;
        let content_w = (max_w + PAGE_PAD_PX * 2.0).max(f32::from(workspace.width));
        let content_h = total_h.max(f32::from(workspace.height));
        let origin_x = ((content_w - max_w) * 0.5).max(PAGE_PAD_PX);
        WorkspaceContent {
            size: size(px(content_w), px(content_h)),
            slide_origin: point(px(origin_x), px(PAGE_PAD_PX)),
            scale,
        }
    }

    /// The bar that says the file is signed (a viewer changes nothing, so its words differ from the document's: no "editing removes them").
    fn infobar_kind(editor: &SlideEditor) -> Option<ibis_office_shell::infobar::InfoBarKind> {
        let state = editor.format_state::<State>()?;
        let guard = state.viewer.lock().expect("pdf viewer lock");
        guard.signature_status_text().map(|_| ibis_office_shell::infobar::InfoBarKind::DigitalSignature)
    }

    fn infobar_spec(kind: ibis_office_shell::infobar::InfoBarKind) -> Option<ibis_office_shell::infobar::InfoBarSpec> {
        match kind {
            ibis_office_shell::infobar::InfoBarKind::DigitalSignature => Some(("infobar-signature", "infobar-btn-signature-inspect", "shell.infobar.signature.title", "shell.infobar.signature.message-viewer", "shell.infobar.signature.inspect")),
            _ => None,
        }
    }

    fn signature_report(editor: &SlideEditor) -> Option<String> {
        let state = editor.format_state::<State>()?;
        let guard = state.viewer.lock().expect("pdf viewer lock");
        guard.signature_report_text()
    }

    fn status_items(editor: &SlideEditor) -> Vec<SharedString> {
        let Some(state) = editor.format_state::<State>() else {
            return Vec::new();
        };
        let guard = state.viewer.lock().expect("pdf viewer lock");
        guard
            .signature_status_text()
            .map(SharedString::from)
            .into_iter()
            .collect()
    }

    fn render_body(
        editor: &mut SlideEditor,
        _window: &mut Window,
        cx: &mut Context<SlideEditor>,
    ) -> AnyElement {
        let Some(state) = editor.format_state::<State>() else {
            return div().size_full().into_any_element();
        };
        let viewer = state.viewer.clone();
        let ui = state.ui.clone();
        let (find_open, page_open) = (state.find_open, state.page_open);
        let (find_input, page_input) = (state.find_input.clone(), state.page_input.clone());
        let scale = page_scale(editor);
        let workspace = editor
            .workspace_bounds
            .map(|b| b.size)
            .unwrap_or(size(px(1200.0), px(800.0)));
        let content = Self::workspace_content(editor, workspace);
        let find_bar = find_open.then(|| find_bar(editor, &find_input, cx));
        let page_bar = page_open.then(|| page_bar(editor, &page_input, cx));

        div()
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(rgb(0xf2f3f5))
            .child(
                div()
                    .id("pdf-workspace-scroll")
                    .size_full()
                    .overflow_scroll()
                    .track_scroll(&editor.workspace_scroll)
                    .on_scroll_wheel(cx.listener(SlideEditor::on_workspace_scroll))
                    .child(
                        div()
                            .relative()
                            .flex_none()
                            .w(content.size.width)
                            .h(content.size.height)
                            // The text is picked by pressing on it and dragging; a press without a drag picks nothing.
                            .on_mouse_down(MouseButton::Left, cx.listener(|editor, event: &MouseDownEvent, window, cx| press(editor, event, window, cx)))
                            .on_mouse_move(cx.listener(|editor, event: &MouseMoveEvent, _, cx| drag(editor, event, cx)))
                            .on_mouse_up(MouseButton::Left, cx.listener(|editor, _: &MouseUpEvent, _, cx| release(editor, cx)))
                            .child(
                                canvas(
                                    |_bounds, _window, _cx| {},
                                    move |bounds: Bounds<Pixels>, _, window: &mut Window, cx| {
                                        ui.lock().expect("pdf ui lock").canvas = Some((f32::from(bounds.origin.x), f32::from(bounds.origin.y), f32::from(bounds.size.width)));
                                        let mut guard = viewer.lock().expect("pdf viewer lock");
                                        let page_count = guard.pages.len();
                                        let mut y = f32::from(bounds.origin.y) + PAGE_PAD_PX;
                                        let view_h = f32::from(window.viewport_size().height);

                                        for idx in 0..page_count {
                                            let (pw, ph) = {
                                                let p = &guard.pages[idx];
                                                (
                                                    p.crop_box.width() * scale,
                                                    p.crop_box.height() * scale,
                                                )
                                            };
                                            let page_bottom = y + ph;
                                            if page_bottom >= -100.0 && y <= view_h + 100.0 {
                                                let x = f32::from(bounds.origin.x)
                                                    + ((f32::from(bounds.size.width) - pw) * 0.5)
                                                        .max(PAGE_PAD_PX);
                                                let page_bounds = Bounds {
                                                    origin: point(px(x), px(y)),
                                                    size: size(px(pw), px(ph)),
                                                };
                                                window.paint_quad(
                                                    fill(page_bounds, rgb(0xffffff))
                                                        .border_widths(px(1.0))
                                                        .border_color(hsla(0.0, 0.0, 0.0, 0.14)),
                                                );
                                                let _ = guard.paint_page(
                                                    idx,
                                                    point(px(x), px(y)),
                                                    scale,
                                                    window,
                                                    cx,
                                                );
                                                paint_marks(&mut guard, idx, x, y, scale, window);
                                            }
                                            y += ph + PAGE_GAP_PX;
                                        }
                                    },
                                )
                                .size_full(),
                            ),
                    ),
            )
            .scrollbar(&editor.workspace_scroll, ScrollbarAxis::Both)
            .children(find_bar)
            .children(page_bar)
            .into_any_element()
    }
}

/// The marks over a page: the text picked, the places the search found (the one shown stronger).
fn paint_marks(viewer: &mut PdfViewerState, page: usize, left: f32, top: f32, scale: f32, window: &mut Window) {
    let mut mark = |viewer: &mut PdfViewerState, from: usize, to: usize, color: gpui_kit::Hsla, window: &mut Window| {
        for i in from..=to {
            if let Some(r) = viewer.char_rect(page, i) {
                let bounds = Bounds { origin: point(px(left + r.x0.min(r.x1) * scale), px(top + r.y0.min(r.y1) * scale)), size: size(px(r.width() * scale), px(r.height() * scale)) };
                window.paint_quad(fill(bounds, color));
            }
        }
    };
    let found: Vec<(usize, crate::FindMatch)> = viewer.find_matches.iter().copied().enumerate().filter(|(_, m)| m.page == page).collect();
    for (k, m) in found {
        let color = if viewer.find_current == Some(k) { hsla(0.07, 1.0, 0.5, 0.55) } else { hsla(0.15, 1.0, 0.5, 0.4) };
        mark(viewer, m.start, m.end.saturating_sub(1), color, window);
    }
    if let Some((first, last)) = viewer.selected_range(page) {
        mark(viewer, first, last, hsla(0.6, 0.9, 0.5, 0.35), window);
    }
}

/// The press: the character under the pointer is where a selection starts (with Shift, where the one there is stretched to).
fn press(editor: &mut SlideEditor, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<SlideEditor>) {
    window.focus(&editor.focus_handle, cx);
    let Some((page, x, y)) = pdf_point(editor, event.position) else { return };
    let Some(state) = editor.format_state::<State>() else { return };
    {
        let mut guard = state.viewer.lock().expect("pdf viewer lock");
        let pos = guard.hit(page, x, y);
        guard.selection = match (pos, event.modifiers.shift, guard.selection) {
            (Some(p), true, Some(sel)) => Some(crate::Selection { anchor: sel.anchor, focus: p }),
            (Some(p), _, _) => Some(crate::Selection { anchor: p, focus: p }),
            (None, ..) => None,
        };
    }
    state.ui.lock().expect("pdf ui lock").dragging = true;
    cx.notify();
}

/// The drag: the selection reaches to the character under the pointer.
fn drag(editor: &mut SlideEditor, event: &MouseMoveEvent, cx: &mut Context<SlideEditor>) {
    let Some(state) = editor.format_state::<State>() else { return };
    if !state.ui.lock().expect("pdf ui lock").dragging || event.pressed_button != Some(MouseButton::Left) {
        return;
    }
    let Some((page, x, y)) = pdf_point(editor, event.position) else { return };
    let Some(state) = editor.format_state::<State>() else { return };
    let mut guard = state.viewer.lock().expect("pdf viewer lock");
    if let (Some(pos), Some(sel)) = (guard.hit(page, x, y), guard.selection)
        && sel.focus != pos
    {
        guard.selection = Some(crate::Selection { anchor: sel.anchor, focus: pos });
        drop(guard);
        cx.notify();
    }
}

/// The release ends the drag; a press that picked a single character and did not move picks nothing.
fn release(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) {
    let Some(state) = editor.format_state::<State>() else { return };
    state.ui.lock().expect("pdf ui lock").dragging = false;
    let mut guard = state.viewer.lock().expect("pdf viewer lock");
    if guard.selection.is_some_and(|s| s.anchor == s.focus) {
        guard.selection = None;
    }
    drop(guard);
    cx.notify();
}

fn bar_style() -> gpui_kit::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(6.0))
        .bg(rgb(0xffffff))
        .border_1()
        .border_color(hsla(0.0, 0.0, 0.0, 0.18))
        .shadow_md()
}

/// The search bar over the top right of the pages: the field, the previous and the next place, how many were found, close.
fn find_bar(editor: &SlideEditor, input: &Entity<InputState>, cx: &mut Context<SlideEditor>) -> AnyElement {
    use ibis_office_shell::chrome::{FindNav, action_button, find_nav_button, text_field};
    use ibis_office_shell::i18n::{format, text};
    let (n, current) = find_status(editor).unwrap_or((0, None));
    let status: SharedString = match current {
        Some(i) => format("_pptx.n-of-total", &[("n", &(i + 1)), ("total", &n)]).into(),
        None => format("docx.find.n-results", &[("n", &n)]).into(),
    };
    bar_style()
        .id("pdf-find-bar")
        .absolute()
        .top(px(12.0))
        .right(px(24.0))
        .child(text_field(input).width(px(220.0)).select_on_focus())
        .child(find_nav_button("pdf-find-previous", FindNav::Previous).on_click(cx.listener(|e, _, _, cx| find_step(e, false, cx))))
        .child(find_nav_button("pdf-find-next", FindNav::Next).on_click(cx.listener(|e, _, _, cx| find_step(e, true, cx))))
        .child(div().text_size(px(12.0)).child(status))
        .child(action_button("pdf-find-close").small().label(text("_shell.close")).on_click(cx.listener(|e, _, window, cx| {
            if let Some(state) = e.format_state_mut::<State>() {
                state.find_open = false;
                let mut guard = state.viewer.lock().expect("pdf viewer lock");
                guard.find_matches.clear();
                guard.find_current = None;
                guard.find_query.clear();
            }
            window.focus(&e.focus_handle, cx);
            cx.notify();
        })))
        .into_any_element()
}

/// The bar at the bottom of the pages where the number of a page is typed: the field, the number of pages, and the button.
fn page_bar(editor: &SlideEditor, input: &Entity<InputState>, cx: &mut Context<SlideEditor>) -> AnyElement {
    use ibis_office_shell::chrome::{action_button, text_field};
    use ibis_office_shell::i18n::{format, text};
    let total = editor.presentation().slides.len();
    bar_style()
        .id("pdf-page-bar")
        .absolute()
        .bottom(px(16.0))
        .left(px(24.0))
        .child(text_field(input).width(px(72.0)).select_on_focus())
        .child(div().text_size(px(12.0)).child(SharedString::from(format("shell.pdf.page-of", &[("total", &total)]))))
        .child(action_button("pdf-page-go").small().label(text("shell.pdf.go")).on_click(cx.listener(|e, _, window, cx| {
            let typed = e.format_state::<State>().map(|s| s.page_input.read(cx).value().to_string()).unwrap_or_default();
            if let Ok(number) = typed.trim().parse::<usize>() {
                go_to_page(e, number.saturating_sub(1), cx);
                if let Some(state) = e.format_state_mut::<State>() {
                    state.page_open = false;
                }
                window.focus(&e.focus_handle, cx);
            }
            cx.notify();
        })))
        .into_any_element()
}

/// The document's buttons of the toolbar, grey (and what is in their menus): none of them does anything to a PDF.
fn grey_buttons(items: Vec<ibis_office_shell::chrome::ToolItem>) -> Vec<ibis_office_shell::chrome::ToolItem> {
    use ibis_office_shell::chrome::{MenuEntry, ToolItem};
    fn grey(entries: &mut [MenuEntry]) {
        for entry in entries {
            match entry {
                MenuEntry::Item(item) => item.disabled = true,
                MenuEntry::Submenu(sub) => grey(&mut sub.items),
                _ => {}
            }
        }
    }
    items
        .into_iter()
        .map(|item| match item {
            ToolItem::Button(mut b) => {
                b.disabled = true;
                if let Some(menu) = b.menu.as_mut() {
                    grey(menu);
                }
                ToolItem::Button(b)
            }
            other => other,
        })
        .collect()
}
