// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The table of the commands every screen shares (see `chrome.rs`): the menus with the items every format shares and
//! placeholders for what a format connects, and the toolbar's left part.
//!
//! Placeholder ids a format connects with `Menus::connect` (all start
//! disabled). Shared: `file.new.*`, `file.copy`, `file.export.*`,
//! `file.print`, `file.info`, `file.language`, `file.page-setup`,
//! `edit.cut`, `edit.copy`, `edit.paste`, `edit.paste-plain`,
//! `edit.select-all`, `edit.duplicate`, `edit.find-replace`,
//! `view.fullscreen`, `insert.image`, `insert.shape`, `insert.table`,
//! `insert.chart`, `insert.video`, `insert.audio`, `insert.equation`,
//! `insert.symbol`, `insert.link`, `insert.comment`, `format.text`,
//! `format.align`, `format.spacing`, `format.lists`, `format.borders`,
//! `format.options`, `format.clear`, `tools.spelling`, `tools.dictionary`,
//! `tools.accessibility`, `tools.settings`, `help.search`, `help.help`,
//! `help.shortcuts`, `help.about`.
//!
//! The lines only one screen has, and the whole of the menus only one screen has (データ, スライド, 配置), are the screen's:
//! the table asks for them at the places of `MenuPart` (`FormatEditor::menu_items`; `docx/menu_items.rs`,
//! `xlsx/menu_items.rs`, `pptx/menu_items.rs`). This file names no format.

use crate::chrome::{MenuEntry, MenuId, MenuItem, MenuPart, Menus, Submenu, ToolButton, ToolItem};
use crate::editor::{SlideEditor, Tool};
use crate::format::with_format;

/// A menu item with its icon, labelled in the UI's language by its id
/// (`i18n/*.tsv`).
pub fn item(id: &'static str, icon: &'static str) -> MenuItem {
    MenuItem::tr(id).icon(icon)
}

/// The same, labelled by another key of the tables (an id with two labels).
#[allow(dead_code)]
fn item_key(id: &'static str, icon: &'static str, key: &str) -> MenuItem {
    MenuItem::new(id, crate::i18n::text(key)).icon(icon)
}

pub fn sub(id: &'static str, icon: &'static str, items: Vec<MenuEntry>) -> MenuEntry {
    Submenu::tr(id, icon, items).into()
}

pub const SEP: MenuEntry = MenuEntry::Separator;

/// The icon of a recent file whose extension is no format's own.
const OTHER_FILE_ICON: &str = "description";

/// The menus for the open document.
pub(crate) fn menus(editor: &SlideEditor) -> Menus {
    let order: &[MenuId] = editor.format_info().command_menus;
    let mut menus = Menus::default();
    for &id in order {
        let entries = match id {
            MenuId::File => file_menu(editor),
            MenuId::Edit => edit_menu(editor),
            MenuId::View => view_menu(editor),
            MenuId::Insert => insert_menu(editor),
            MenuId::Format => format_menu(editor),
            // The menus only one screen has (データ, スライド, 配置) are the screen's.
            MenuId::Data | MenuId::Slide | MenuId::Arrange => with_format!(editor, menu_items(editor, MenuPart::Own(id))),
            MenuId::Tools => vec![
                sub("tools.spelling", "spellcheck", vec![]),
                item("tools.dictionary", "menu_book").shortcut("⇧⌘Y").into(),
                item("tools.word-count", "numbers").shortcut("⇧⌘C").into(),
                SEP,
                item("tools.accessibility", "accessibility_new").into(),
                item("tools.settings", "settings").shortcut("⌘,").on(|this, window, cx| this.open_settings(0, window, cx)).into(),
            ],
            MenuId::Window => window_menu(editor),
            // The tabs of the redesigned bar (`menu_layout.rs`) have no section of their own in the commands' source.
            MenuId::Design | MenuId::Layout | MenuId::References | MenuId::Mailings | MenuId::Review | MenuId::PageLayout | MenuId::Formulas | MenuId::Transitions | MenuId::Animations | MenuId::SlideShow => Vec::new(),
            MenuId::Help => {
                let mut entries: Vec<MenuEntry> = vec![
                    item("help.search", "search").shortcut("⌥/").on(|this, window, cx| this.open_toolbar_menu_search(window, cx)).into(),
                    item("help.help", "help").into(),
                    item("help.shortcuts", "keyboard").shortcut("⌘/").into(),
                    SEP,
                ];
                // The update's check and log (no update in the browser).
                // No check on iOS and Android (K0087: the stores update the app); the log stays.
                #[cfg(not(target_family = "wasm"))]
                if !crate::mobile_os::is_mobile_os() {
                    entries.push(item("help.check-update", "download").on(|_, _, cx| crate::update::check_now(cx)).into());
                }
                #[cfg(not(target_family = "wasm"))]
                entries.push(item("help.update-log", "description").on(|_, _, cx| crate::update::reveal_log(cx)).into());
                // Linux's AppImage: ask again about the list of apps (without its entry the dock shows no icon).
                if crate::linux_desktop::is_appimage() {
                    entries.push(item("help.register-desktop", "download").on(|_, _, cx| crate::linux_desktop::register_again(cx)).into());
                }
                // セットアップ (K0001): the fonts that can be downloaded (every platform; the browser keeps them in IndexedDB).
                // Not with `?fd=0` (the playground: no guidance to the downloads).
                if crate::embed::font_prompt() {
                    entries.push(item("help.setup", "download").on(|this, window, cx| this.open_font_setup(window, cx)).into());
                }
                // ソースコード (AGPL)… (K0036): the project's page on GitHub, so that the users of a fork's build also learn of ibis-office.
                entries.push(item("help.source-code", "link").on(|_, _, cx| crate::external::open_url(&crate::host::source_url(), cx)).into());
                // The About screen is a dialog inside the window on every platform (shell/about.rs).
                entries.push(item("help.about", "info").on(|this, _, cx| this.show_about(cx)).into());
                entries
            }
        };
        menus.menus.push((id, entries));
    }
    menus
}

/// The lines of ファイル > 最近使ったファイル: the recent files (the same as the start view's list), or the line that says there are none.
pub(crate) fn recent_entries(editor: &SlideEditor) -> Vec<MenuEntry> {
    #[cfg(not(target_family = "wasm"))]
    let recent: Vec<MenuEntry> = if editor.recent_files.is_empty() {
        vec![MenuItem::tr("file.recent.none").into()]
    } else {
        editor
            .recent_files
            .iter()
            .take(10)
            .map(|path| {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                // The icon of the format that opens the file (by its extension, as opening does); a file of no format's (text, a
                // web page, a PDF: read as a new document) has the plain page.
                let icon = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .and_then(ibis_formats::Kind::from_extension)
                    .map_or(OTHER_FILE_ICON, |kind| crate::format::info(kind).icon);
                let path = path.clone();
                let exists = path.is_file();
                MenuItem::new("file.recent.item", name).icon(icon).disabled(!exists).on(move |this, window, cx| this.open_path_or_raise(path.clone(), window, cx)).into()
            })
            .collect()
    };
    #[cfg(target_family = "wasm")]
    let recent: Vec<MenuEntry> = {
        let _ = editor;
        host_recent_entries()
    };
    #[cfg(not(target_family = "wasm"))]
    let recent = if crate::host::is_connected() && !crate::host::recent_files().is_empty() { host_recent_entries() } else { recent };
    recent
}

/// The recent files a host keeps (`listRecentFiles`): up to 20, each with the icon of its format and its name; one opens through the host's `openUrl` (a new tab).
/// The browser has no recent files of its own, so without a host that gave the list, or with it empty, the line says there are none.
fn host_recent_entries() -> Vec<MenuEntry> {
    let files = crate::host::recent_files();
    if !crate::host::is_connected() || files.is_empty() {
        return vec![MenuItem::tr("file.recent.none").into()];
    }
    let can_open = crate::host::capabilities().open_url;
    files
        .into_iter()
        .take(crate::host::RECENT_FILES_MAX)
        .map(|file| {
            let icon = file
                .name
                .rsplit_once('.')
                .and_then(|(_, e)| ibis_formats::Kind::from_extension(&e.to_ascii_lowercase()))
                .map_or(OTHER_FILE_ICON, |kind| crate::format::info(kind).icon);
            let url = file.url.clone();
            MenuItem::new("file.recent.item", file.name).icon(icon).disabled(!can_open).on(move |_, _, cx| crate::external::open_url(&url, cx)).into()
        })
        .collect()
}

fn file_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let recent = recent_entries(editor);
    let kind = editor.kind();
    // 新規作成: a line for each format, in the formats' order; ⌘N is the open screen's.
    let new_items: Vec<MenuEntry> = crate::format::creatable_infos()
        .into_iter()
        .map(|(format, info)| -> MenuEntry {
            let new = item(info.new_command, info.icon).on(move |this, window, cx| this.new_file(format, window, cx));
            if format == kind { new.shortcut("⌘N").into() } else { new.into() }
        })
        .collect();
    let mut items: Vec<crate::chrome::MenuEntry> = Vec::new();
    // A host with no start window (`ui.startWindow` off: ibisWorks) has neither this line nor the separator after it (K0037).
    if cfg!(not(target_arch = "wasm32")) && !(crate::host::is_connected() && !crate::host::options().start_window) {
        items.push(item("file.start-window", "grid_view").on(|_, _, cx| crate::app::open_editor(None, None, cx)).into());
        items.push(SEP);
    }
    items.extend(vec![
        sub("file.new", "note_add", new_items),
        item("file.open", "folder_open").shortcut("⌘O").on(|this, window, cx| this.open_file(&crate::actions::Open, window, cx)).into(),
        sub("file.recent", "schedule", recent),
        SEP,
        item("file.save", "save").shortcut("⌘S").on(|this, window, cx| this.save(&crate::actions::Save, window, cx)).into(),
        item("file.save-as", "save_as").shortcut("⇧⌘S").on(|this, window, cx| this.save_as(&crate::actions::SaveAs, window, cx)).into(),
        {
            // The browser version: the copy is downloaded at once, with the changes so far (the document is not saved by it; K0037).
            #[cfg(target_family = "wasm")]
            let copy = item_key("file.copy", "content_copy", "file.copy-download");
            #[cfg(not(target_family = "wasm"))]
            let copy = item("file.copy", "content_copy");
            copy.on(|this, window, cx| this.save_copy(window, cx)).into()
        },
        SEP,
        sub("file.export", "ios_share", {
            let mut entries = vec![
                item("file.export.pdf", "picture_as_pdf").disabled(!crate::host::export_allowed("pdf")).on(|this, window, cx| this.export_pdf(window, cx)).into(),
                item("file.export.png", "image").disabled(!crate::host::export_allowed("png")).on(|this, window, cx| this.export_png(window, cx)).into(),
                item("file.export.gif", "animation").disabled(!crate::host::export_allowed("gif")).on(|this, window, cx| this.export_gif(window, cx)).into(),
                item("file.export.svg", "draw").disabled(!crate::host::export_allowed("svg")).into(),
            ];
            if editor.is_jtd_read_only() && !editor.jtd_gallery {
                entries.push(item("file.export.docx", "description").on(|this, window, cx| this.export_jtd_docx(window, cx)).into());
                entries.push(item("file.export.html", "code").on(|this, window, cx| this.export_jtd_html(window, cx)).into());
                if !editor.read_only {
                    entries.push(item("file.edit-as-docx", "edit").on(|this, window, cx| this.edit_jtd_as_docx(window, cx)).into());
                }
            }
            // The screen's own kinds of file (a presentation: OpenDocument).
            entries.extend(with_format!(editor, menu_items(editor, MenuPart::FileExport)));
            entries
        }),
        item("file.print", "print").disabled(!crate::host::print_allowed()).shortcut("⌘P").on(|this, window, cx| this.print(window, cx)).into(),
        SEP,
        item("file.info", "info").into(),
        item("file.protect", "lock").on(|this, window, cx| this.protect_with_password(window, cx)).into(),
        item("file.language", "language").into(),
        item("file.page-setup", "crop_landscape").into(),
        SEP,
        file_close_item(),
    ]);
    // ファイル ▸ 終了 (2026-11-11 user decision: the last line of the File menu on every OS; a Mac keeps ibisOffice > 終了 too). The same
    // way as the app menu's Quit and ⌘Q / Ctrl+Q: windows with edits not saved ask first (`quit_asking_to_save`). The browser has no app
    // to quit (closing the tab is the browser's).
    #[cfg(not(target_family = "wasm"))]
    if !crate::mobile_os::is_mobile_os() {
        items.extend([SEP, item("file.quit", "logout").shortcut("⌘Q").on(|_, _, cx| crate::app::quit_asking_to_save(cx)).into()]);
    }
    items
}

/// ファイル > 閉じる. On iOS and Android (K0076) the one window shows the start view and the file in turn, so the line reads 「戻る」 and goes
/// back to the start view as the title row's ＜ does (K0067: asking to save first); closing the window would end the app. The id and the key stay.
fn file_close_item() -> MenuEntry {
    if crate::mobile_os::is_mobile_os() {
        MenuItem::new("file.close", crate::i18n::text("file.back")).icon("chevron_left").shortcut("⌘W").on(|this, window, cx| this.back_to_start_on(true, window, cx)).into()
    } else {
        item("file.close", "close").shortcut("⌘W").on(|this, window, cx| this.close_file(&crate::actions::CloseFile, window, cx)).into()
    }
}

fn edit_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let can_undo = editor.document.can_undo();
    let can_redo = editor.document.can_redo();
    let mut entries = vec![
        item("edit.undo", "undo").shortcut("⌘Z").disabled(!can_undo).on(|this, window, cx| this.undo(&crate::actions::Undo, window, cx)).into(),
        item("edit.redo", "redo").shortcut("⇧⌘Z").disabled(!can_redo).on(|this, window, cx| this.redo(&crate::actions::Redo, window, cx)).into(),
        SEP,
        item("edit.cut", "content_cut").shortcut("⌘X").into(),
        item("edit.copy", "content_copy").shortcut("⌘C").into(),
        item("edit.paste", "content_paste").shortcut("⌘V").into(),
        item("edit.paste-plain", "content_paste_go").shortcut("⇧⌘V").into(),
        item("edit.paste-match", "content_paste_search").shortcut("⌥⇧⌘V").into(),
        item("edit.copy-format", "format_paint").shortcut("⌥⌘C").into(),
        item("edit.paste-format", "format_paint").shortcut("⌥⌘V").into(),
        SEP,
        item("edit.select-all", "select_all").shortcut("⌘A").into(),
        item("edit.delete", "delete").shortcut("⌫").disabled(editor.selection.is_none()).on(|this, window, cx| this.delete_selection(&crate::actions::DeleteSelection, window, cx)).into(),
        item("edit.duplicate", "control_point_duplicate").shortcut("⌘D").into(),
        SEP,
        item("edit.find-replace", "find_replace").shortcut("⌘F").into(),
    ];
    entries.extend(with_format!(editor, menu_items(editor, MenuPart::EditEnd)));
    entries
}

pub fn view_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    // The screen's own lines come first: its views and what it shows.
    let mut entries = with_format!(editor, menu_items(editor, MenuPart::ViewStart));
    if editor.is_jtd_read_only() {
        if editor.jtd_thumbnail {
            entries.push(item("view.hide-jtd-thumbnail", "arrow_back").on(|this, window, cx| this.return_from_jtd_thumbnail(window, cx)).into());
        } else {
            entries.push(item("view.show-jtd-thumbnail", "image").on(|this, window, cx| this.show_jtd_thumbnail(window, cx)).into());
            if !editor.jtd_gallery {
                entries.push(item("view.show-jtd-images", "image").on(|this, window, cx| this.show_jtd_images(window, cx)).into());
            } else if editor.jtd_gallery_body.is_some() {
                entries.push(item("view.show-jtd-body", "description").on(|this, window, cx| this.show_jtd_body(window, cx)).into());
            }
        }
        entries.push(SEP);
    }
    entries.extend([
        MenuItem::tr("view.sidebar")
            .checked(editor.right_pane.shown)
            .disabled(!editor.right_pane.available)
            .on(|this, window, cx| {
                this.toggle_sidebar(window, cx);
            })
            .into(),
        // The soft proof (Photoshop's 色の校正): only a file with the CMYK mode on has it to switch.
        MenuItem::tr("view.print-colors")
            .checked(editor.cmyk_on() && editor.soft_proof)
            .disabled(!editor.cmyk_on())
            .on(|this, _, cx| {
                this.soft_proof = !this.soft_proof;
                cx.notify();
            })
            .into(),
        MenuItem::tr("view.menus")
            .shortcut("⌃⇧F")
            .on(|this, _, cx| {
                this.chrome_compact = !this.chrome_compact;
                if !this.chrome_compact {
                    this.show_menu_bar = true;
                }
                cx.notify();
            })
            .into(),
        SEP,
        sub("view.zoom", "zoom_in", vec![
            item("view.zoom-in", "zoom_in").shortcut("⌘+").on(|this, window, cx| this.step_zoom(true, window, cx)).into(),
            item("view.zoom-out", "zoom_out").shortcut("⌘-").on(|this, window, cx| this.step_zoom(false, window, cx)).into(),
            SEP,
            item("view.zoom-400", "").checked(zoom_approx(editor.current_zoom(), 4.0)).on(|this, window, cx| this.set_zoom(Some(4.0), window, cx)).into(),
            item("view.zoom-200", "").checked(zoom_approx(editor.current_zoom(), 2.0)).on(|this, window, cx| this.set_zoom(Some(2.0), window, cx)).into(),
            MenuItem::tr("view.zoom-100").shortcut("⌘0").checked(editor.zoom.is_some() && zoom_approx(editor.current_zoom(), 1.0)).on(|this, window, cx| this.set_zoom(Some(1.0), window, cx)).into(),
            item("view.zoom-66", "").checked(zoom_approx(editor.current_zoom(), 0.66)).on(|this, window, cx| this.set_zoom(Some(0.66), window, cx)).into(),
            item("view.zoom-50", "").checked(zoom_approx(editor.current_zoom(), 0.50)).on(|this, window, cx| this.set_zoom(Some(0.50), window, cx)).into(),
            item("view.zoom-33", "").checked(zoom_approx(editor.current_zoom(), 0.33)).on(|this, window, cx| this.set_zoom(Some(0.33), window, cx)).into(),
            SEP,
            item("view.zoom-fit", "fit_screen").shortcut("⌥⌘0").disabled(!with_format!(editor, can_fit())).checked(editor.zoom.is_none()).on(|this, window, cx| this.set_zoom(None, window, cx)).into(),
            SEP,
            item("view.zoom-dialog", "").on(|this, window, cx| this.open_zoom_dialog(window, cx)).into(),
        ]),
        item("view.fullscreen", "fullscreen").shortcut("⌃⌘F").on(|_, window, _| window.toggle_fullscreen()).into(),
    ]);
    entries
}

fn window_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let mut entries: Vec<MenuEntry> = Vec::new();
    // The windows' list and the next and previous window (menu bar redesign, stage 4). The keys are Ctrl+Tab, and Ctrl+F6 and
    // Ctrl+Shift+F6 (on a Mac, ⌃Tab and the system's ⌘` and ⌘⇧`). Not in the browser: one document per tab.
    #[cfg(not(target_family = "wasm"))]
    {
        let (next_key, prev_key) = if cfg!(target_os = "macos") { ("⌘`", "⌘⇧`") } else { ("⌃F6", "⌃⇧F6") };
        entries.push(item("window.list", "list").shortcut("⌃Tab").on(|this, window, cx| this.open_window_switcher(false, window, cx)).into());
        entries.push(item("window.next", "arrow_forward").disabled(editor.open_documents.len() < 2).shortcut(next_key).on(|this, window, cx| this.step_window(true, window, cx)).into());
        entries.push(item("window.previous", "arrow_back").disabled(editor.open_documents.len() < 2).shortcut(prev_key).on(|this, window, cx| this.step_window(false, window, cx)).into());
        entries.push(SEP);
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let label_key = editor.format_info().office.open_key;
        let can_open_in_office = editor.has_file_on_disk();
        entries.push(
            MenuItem::new("window.open-in-office", label_key)
                .disabled(!can_open_in_office)
                .on(|this, window, cx| this.open_in_office_action(window, cx))
                .into(),
        );
        entries.push(SEP);
    }
    entries.extend([
        item("window.new-window", "open_in_new").shortcut("⌘⇧N").on(|this, _, cx| this.new_window(cx)).into(),
        // A workbook's `menus()` (`xlsx/toolbar.rs`) connects these two lines to its own: the panel of arranging the windows, and
        // the split of the sheet (with its check mark). A document's `menus()` (`docx/toolbar.rs`) takes them out for its own
        // lines below. In a presentation the split line does nothing (as it always did there).
        item("window.arrange-all", "grid_view").on(|_, window, cx| SlideEditor::arrange_all_windows(Some(window), cx)).into(),
        item("window.split", "splitscreen").shortcut("⌥⌘S").on(|_, _, _| {}).into(),
        item("window.cascade", "filter_none").on(|_, window, cx| SlideEditor::cascade_windows(Some(window), cx)).into(),
    ]);
    // Arranging the windows side by side or one over the other, bringing them all forward and closing them all (not in the browser: one
    // document per tab).
    #[cfg(not(target_family = "wasm"))]
    entries.extend([
        item("window.tile-side-by-side", "vertical_split").on(|_, window, cx| SlideEditor::arrange_windows_of(crate::arrange_kind::ArrangeKind::Vertical, window, cx)).into(),
        item("window.tile-stacked", "horizontal_split").on(|_, window, cx| SlideEditor::arrange_windows_of(crate::arrange_kind::ArrangeKind::Horizontal, window, cx)).into(),
        item("window.bring-all-front", "flip_to_front").disabled(editor.open_documents.len() < 2).on(|this, window, cx| this.bring_all_windows_front(window, cx)).into(),
        item("window.close-all", "close").shortcut("⌥⌘W").on(|this, window, cx| this.close_all_documents(window, cx)).into(),
    ]);
    // The screen's own lines (a workbook's and a document's windows side by side, hiding a window, the split).
    entries.extend(with_format!(editor, menu_items(editor, MenuPart::WindowEnd)));
    let open_windows = editor.open_window_menu_items();
    if !open_windows.is_empty() {
        entries.push(SEP);
        entries.extend(open_windows);
    }
    entries
}

fn insert_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let text_box = item("insert.text-box", "text_fields");
    let text_box = if editor.allows_shapes() { text_box.on(|this, window, cx| this.set_tool(Tool::TextBox, window, cx)) } else { text_box };
    let mut entries = with_format!(editor, menu_items(editor, MenuPart::InsertStart));
    entries.extend([
        sub("insert.image", "image", vec![]),
        text_box.into(),
        sub("insert.shape", "shapes", vec![]),
        sub("insert.table", "table", vec![]),
        sub("insert.chart", "bar_chart", vec![]),
        sub("insert.smartart", "account_tree", vec![]),
        item("insert.video", "movie").into(),
        item("insert.audio", "music_note").into(),
        SEP,
        item("insert.equation", "functions").into(),
        item("insert.symbol", "emoji_symbols").into(),
        item("insert.link", "link").shortcut("⌘K").into(),
        item("insert.comment", "add_comment").shortcut("⌥⌘M").into(),
    ]);
    entries.extend(with_format!(editor, menu_items(editor, MenuPart::InsertEnd)));
    entries
}

fn format_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let mut entries = vec![
        sub("format.text", "format_bold", vec![]),
        sub("format.align", "format_align_left", vec![]),
        sub("format.spacing", "format_line_spacing", vec![]),
        sub("format.lists", "format_list_bulleted", vec![]),
        sub("format.borders", "border_style", vec![]),
        SEP,
        item("format.options", "tune").into(),
        item("format.clear", "format_clear").shortcut("⌘\\").into(),
    ];
    entries.extend(with_format!(editor, menu_items(editor, MenuPart::FormatEnd)));
    entries
}

// ----- Toolbar -------------------------------------------------------------

/// The toolbar's left part: メニューを検索, the screen's own buttons (`FormatEditor::toolbar_left`: 新しいスライド, スペル チェック)
/// and the format painter.
pub(crate) fn toolbar_left(editor: &SlideEditor) -> Vec<ToolItem> {
    let mut items: Vec<ToolItem> = vec![ToolButton::new("menu-search", "search", crate::i18n::text("_shell.search-menus")).searchable().into()];
    items.extend(crate::format::with_format!(editor, toolbar_left(editor)));
    // The format connects it (`FormatEditor::format_paint`).
    let mut paint = ToolButton::new("format-paint", "format_paint", crate::i18n::text("_shell.copy-paste-formatting"));
    if let Some((on, handler)) = crate::format::with_format!(editor, format_paint(editor)) {
        paint.on = on;
        paint.handler = Some(handler);
    }
    items.push(paint.into());
    items
}

/// Whether two zoom factors are approximately equal.
fn zoom_approx(current: f32, target: f32) -> bool {
    (current - target).abs() < 0.02
}


