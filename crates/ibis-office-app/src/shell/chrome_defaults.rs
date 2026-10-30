// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What the chrome holds until a format gives its own (see `chrome.rs`):
//! the menus with the items every format shares and placeholders for the
//! rest, and a toolbar and sidebar per format made of what the editor can
//! already do.
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
//! Workbooks (xlsx): `xlsx.insert-row-above`, `xlsx.insert-row-below`,
//! `xlsx.insert-column-left`, `xlsx.insert-column-right`,
//! `xlsx.insert-cells`, `xlsx.insert-sheet`, `xlsx.delete-row`,
//! `xlsx.delete-column`, `xlsx.delete-cells`, `xlsx.delete-sheet`,
//! `xlsx.rename-sheet`, `xlsx.duplicate-sheet`, `xlsx.hide-sheet`,
//! `xlsx.sort`, `xlsx.filter`, `xlsx.protect`, `xlsx.named-ranges`,
//! `xlsx.validation`, `xlsx.split-text`, `xlsx.freeze`,
//! `xlsx.gridlines`, `xlsx.headings`, `xlsx.formula-bar`, `xlsx.page-break-preview`, `xlsx.page-layout`.
//!
//! Presentations (pptx): `pptx.duplicate-slide`, `pptx.skip-slide`,
//! `pptx.transition`, `pptx.background`, `pptx.layout`, `pptx.theme`,
//! `pptx.edit-theme`, `pptx.order.*`, `pptx.align`, `pptx.distribute`,
//! `pptx.center`, `pptx.rotate.*`, `pptx.group`, `pptx.ungroup`,
//! `pptx.motion`, `pptx.grid-view`, `pptx.notes`.
//!
//! Documents (docx): `docx.page-break`, `docx.column-break`, `docx.header-footer`,
//! `docx.page-number`, `docx.toc`, `docx.footnote`, `docx.columns`,
//! `docx.word-count`, `docx.navigation-pane`, `docx.print-layout`.

use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::TextAlign;
use ibis_pptx::Kind;

use crate::shell::chrome::{MenuEntry, MenuId, MenuItem, Menus, SidebarPanel, Submenu, ToolButton, ToolItem, text_secondary};
use crate::shell::editor::{RulerUnit, SlideEditor, Tool};

/// A menu item with its icon, labelled in the UI's language by its id
/// (`i18n/*.tsv`).
fn item(id: &'static str, icon: &'static str) -> MenuItem {
    MenuItem::tr(id).icon(icon)
}

/// The same, labelled by another key of the tables (an id with two labels).
#[allow(dead_code)]
fn item_key(id: &'static str, icon: &'static str, key: &str) -> MenuItem {
    MenuItem::new(id, crate::shell::i18n::text(key)).icon(icon)
}

fn sub(id: &'static str, icon: &'static str, items: Vec<MenuEntry>) -> MenuEntry {
    Submenu::tr(id, icon, items).into()
}

const SEP: MenuEntry = MenuEntry::Separator;

/// The menus for the open document.
pub(crate) fn menus(editor: &SlideEditor) -> Menus {
    let kind = editor.kind();
    let order: &[MenuId] = match kind {
        Kind::Document => &[MenuId::File, MenuId::Edit, MenuId::View, MenuId::Insert, MenuId::Format, MenuId::Tools, MenuId::Window, MenuId::Help],
        Kind::Workbook => &[MenuId::File, MenuId::Edit, MenuId::View, MenuId::Insert, MenuId::Format, MenuId::Data, MenuId::Tools, MenuId::Window, MenuId::Help],
        Kind::Presentation => &[MenuId::File, MenuId::Edit, MenuId::View, MenuId::Insert, MenuId::Format, MenuId::Slide, MenuId::Arrange, MenuId::Tools, MenuId::Window, MenuId::Help],
    };
    let mut menus = Menus::default();
    for &id in order {
        let entries = match id {
            MenuId::File => file_menu(editor),
            MenuId::Edit => edit_menu(editor, kind),
            MenuId::View => view_menu(editor, kind),
            MenuId::Insert => insert_menu(editor, kind),
            MenuId::Format => format_menu(kind),
            MenuId::Data => data_menu(),
            MenuId::Slide => slide_menu(editor),
            MenuId::Arrange => arrange_menu(),
            MenuId::Tools => vec![
                sub("tools.spelling", "spellcheck", vec![]),
                item("tools.dictionary", "menu_book").shortcut("⇧⌘Y").into(),
                item("tools.word-count", "numbers").shortcut("⇧⌘C").into(),
                SEP,
                item("tools.accessibility", "accessibility_new").into(),
                item("tools.settings", "settings").shortcut("⌘,").on(|_, window, cx| crate::shell::settings_window::open_over(Some(window.bounds()), window.display(cx).map(|d| d.id()), cx)).into(),
            ],
            MenuId::Window => window_menu(editor, kind),
            MenuId::Help => {
                let mut entries: Vec<MenuEntry> = vec![
                    item("help.search", "search").shortcut("⌥/").on(|this, window, cx| this.open_toolbar_menu_search(window, cx)).into(),
                    item("help.help", "help").into(),
                    item("help.shortcuts", "keyboard").shortcut("⌘/").into(),
                    SEP,
                ];
                // The automatic update's log (no update in the browser).
                #[cfg(not(target_family = "wasm"))]
                entries.push(item("help.update-log", "description").on(|_, _, cx| crate::shell::update::reveal_log(cx)).into());
                entries.push(item("help.about", "info").on(|_, window, cx| crate::shell::about_window::open_over(Some(window.bounds()), window.display(cx).map(|d| d.id()), cx)).into());
                entries
            }
        };
        menus.menus.push((id, entries));
    }
    menus
}

fn file_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
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
                let icon = match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
                    Some("xlsx" | "xlsm" | "xls" | "ods" | "csv" | "tsv" | "tab" | "txt") => "table_chart",
                    Some("docx" | "docm") => "description",
                    _ => "slideshow",
                };
                let path = path.clone();
                let exists = path.is_file();
                MenuItem::new("file.recent.item", name).icon(icon).disabled(!exists).on(move |this, window, cx| this.open_path(path.clone(), None, window, cx)).into()
            })
            .collect()
    };
    #[cfg(target_family = "wasm")]
    let recent: Vec<MenuEntry> = {
        let _ = editor;
        vec![MenuItem::tr("file.recent.none").into()]
    };
    let kind = editor.kind();
    let mut doc_item = item("file.new.document", "description").on(|this, window, cx| this.new_file(Kind::Document, window, cx));
    let mut sheet_item = item("file.new.workbook", "table_chart").on(|this, window, cx| this.new_file(Kind::Workbook, window, cx));
    let mut pres_item = item("file.new.presentation", "slideshow").on(|this, window, cx| this.new_file(Kind::Presentation, window, cx));
    match kind {
        Kind::Document => doc_item = doc_item.shortcut("⌘N"),
        Kind::Workbook => sheet_item = sheet_item.shortcut("⌘N"),
        Kind::Presentation => pres_item = pres_item.shortcut("⌘N"),
    }
    let mut items: Vec<crate::shell::chrome::MenuEntry> = Vec::new();
    #[cfg(not(target_arch = "wasm32"))]
    {
        items.push(item("file.start-window", "grid_view").on(|_, _, cx| crate::open_editor(None, None, cx)).into());
        items.push(SEP);
    }
    items.extend(vec![
        sub("file.new", "note_add", vec![
            doc_item.into(),
            sheet_item.into(),
            pres_item.into(),
        ]),
        item("file.open", "folder_open").shortcut("⌘O").on(|this, window, cx| this.open_file(&crate::OpenFile, window, cx)).into(),
        sub("file.recent", "schedule", recent),
        SEP,
        item("file.save", "save").shortcut("⌘S").on(|this, window, cx| this.save(&crate::Save, window, cx)).into(),
        item("file.save-as", "save_as").shortcut("⇧⌘S").on(|this, window, cx| this.save_as(&crate::SaveAs, window, cx)).into(),
        item("file.copy", "content_copy").on(|this, window, cx| this.save_copy(window, cx)).into(),
        SEP,
        sub("file.export", "ios_share", {
            let mut entries = vec![
                item("file.export.pdf", "picture_as_pdf").on(|this, window, cx| this.export_pdf(window, cx)).into(),
                item("file.export.png", "image").on(|this, window, cx| this.export_png(window, cx)).into(),
                item("file.export.gif", "animation").on(|this, window, cx| this.export_gif(window, cx)).into(),
                item("file.export.svg", "draw").into(),
            ];
            // A presentation can also be written as an OpenDocument presentation.
            if editor.kind() == Kind::Presentation {
                entries.push(item("file.export.odp", "slideshow").on(|this, window, cx| this.export_odp(window, cx)).into());
            }
            entries
        }),
        item("file.print", "print").shortcut("⌘P").on(|this, window, cx| this.print(window, cx)).into(),
        SEP,
        item("file.info", "info").into(),
        item("file.protect", "lock").on(|this, window, cx| this.protect_with_password(window, cx)).into(),
        item("file.language", "language").into(),
        item("file.page-setup", "crop_landscape").into(),
        SEP,
        item("file.close", "close").shortcut("⌘W").on(|this, window, cx| this.close_file(&crate::CloseFile, window, cx)).into(),
    ]);
    items
}

fn edit_menu(editor: &SlideEditor, kind: Kind) -> Vec<MenuEntry> {
    let can_undo = editor.document.can_undo();
    let can_redo = editor.document.can_redo();
    let mut entries = vec![
        item("edit.undo", "undo").shortcut("⌘Z").disabled(!can_undo).on(|this, window, cx| this.undo(&crate::Undo, window, cx)).into(),
        item("edit.redo", "redo").shortcut("⇧⌘Z").disabled(!can_redo).on(|this, window, cx| this.redo(&crate::Redo, window, cx)).into(),
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
        item("edit.delete", "delete").shortcut("⌫").disabled(editor.selection.is_none()).on(|this, window, cx| this.delete_selection(&crate::DeleteSelection, window, cx)).into(),
        item("edit.duplicate", "control_point_duplicate").shortcut("⌘D").into(),
        SEP,
        item("edit.find-replace", "find_replace").shortcut("⇧⌘H").into(),
    ];
    if kind == Kind::Workbook {
        entries.extend([
            SEP,
            sub("xlsx.delete-menu", "delete", vec![
                item("xlsx.delete-row", "delete").into(),
                item("xlsx.delete-column", "delete").into(),
                item("xlsx.delete-cells", "delete").into(),
            ]),
            SEP,
            sub("xlsx.sheet-menu", "table_chart", vec![
                item("xlsx.rename-sheet", "edit").into(),
                item("xlsx.duplicate-sheet", "content_copy").into(),
                item("xlsx.hide-sheet", "visibility_off").into(),
                item("xlsx.delete-sheet", "delete").into(),
            ]),
        ]);
    }
    entries
}

pub(crate) fn view_menu(editor: &SlideEditor, kind: Kind) -> Vec<MenuEntry> {
    let mut entries = Vec::new();
    match kind {
        Kind::Presentation => entries.extend([
            item("pptx.normal-view", "view_compact").checked(editor.is_normal_view()).on(|this, window, cx| this.switch_to_normal_view(window, cx)).into(),
            item("pptx.outline-view", "format_list_bulleted").shortcut("⌃⌥2").checked(editor.is_outline_view()).on(|this, window, cx| {
                if this.is_outline_view() {
                    this.switch_to_normal_view(window, cx);
                } else {
                    this.switch_to_outline_view(window, cx);
                }
            }).into(),
            item("pptx.grid-view", "grid_view").shortcut("⌃⌥1").checked(editor.is_grid_view()).on(|this, window, cx| this.toggle_grid_view(window, cx)).into(),
            item("pptx.notes-page-view", "sticky_note_2").shortcut("⌃⌥3").checked(editor.is_notes_page_view()).on(|this, window, cx| {
                if this.is_notes_page_view() {
                    this.switch_to_normal_view(window, cx);
                } else {
                    this.switch_to_notes_page_view(window, cx);
                }
            }).into(),
            item("pptx.master-view", "dashboard_customize").checked(editor.is_master_view()).on(|this, window, cx| {
                if this.is_master_view() {
                    this.close_master_view(window, cx);
                } else {
                    this.open_master_view(window, cx);
                }
            }).into(),
            item("pptx.notes-format", "format_bold").checked(editor.is_notes_format()).on(|this, window, cx| {
                if this.is_notes_format() {
                    this.close_notes_format(window, cx);
                } else {
                    this.open_notes_format(window, cx);
                }
            }).into(),
            item("pptx.notes-master", "sticky_note_2").checked(editor.is_notes_master()).on(|this, window, cx| {
                if this.is_notes_master() {
                    this.close_notes_format(window, cx);
                } else {
                    this.open_notes_master(window, cx);
                }
            }).into(),
            item("pptx.handout-master", "view_agenda").checked(editor.is_handout_master()).on(|this, window, cx| {
                if this.is_handout_master() {
                    this.close_notes_format(window, cx);
                } else {
                    this.open_handout_master(window, cx);
                }
            }).into(),
            item("pptx.reading-view", "chrome_reader_mode").checked(editor.is_reading_view()).on(|this, window, cx| {
                if this.is_reading_view() {
                    this.switch_to_normal_view(window, cx);
                } else {
                    this.start_reading_view(window, cx);
                }
            }).into(),
            SEP,
            item("view.slideshow", "play_arrow").shortcut("⌘↩").on(|this, window, cx| this.start_slideshow(window, cx)).into(),
            item("pptx.motion", "animation").shortcut("⌥⌘⇧B").into(),
            SEP,
            item("pptx.gridlines", "grid_on").shortcut("⇧F9").checked(editor.is_pptx_gridlines_enabled()).on(|this, _, cx| this.toggle_pptx_gridlines(cx)).into(),
            item("pptx.guides", "straighten").shortcut("⌥F9").checked(editor.is_pptx_guides_enabled()).on(|this, _, cx| this.toggle_pptx_guides(cx)).into(),
            SEP,
            item("pptx.snap-to-grid", "").checked(editor.is_pptx_snap_to_grid_enabled()).on(|this, _, cx| this.toggle_pptx_snap_to_grid(cx)).into(),
            item("pptx.snap-to-shape", "").checked(editor.is_pptx_snap_to_shape_enabled()).on(|this, _, cx| this.toggle_pptx_snap_to_shape(cx)).into(),
            SEP,
            item("pptx.notes", "").into(),
            SEP,
            sub("pptx.color-mode", "contrast", vec![
                item("pptx.view.color", "palette")
                    .checked(editor.color_mode() == crate::render::ColorMode::Color)
                    .on(|this, _, cx| {
                        this.set_color_mode(crate::render::ColorMode::Color, cx);
                    })
                    .into(),
                item("pptx.view.grayscale", "filter_b_and_w")
                    .checked(editor.color_mode() == crate::render::ColorMode::Grayscale)
                    .on(|this, _, cx| {
                        this.set_color_mode(crate::render::ColorMode::Grayscale, cx);
                    })
                    .into(),
                item("pptx.view.high-contrast", "contrast")
                    .shortcut("⌥⌘H")
                    .checked(editor.color_mode() == crate::render::ColorMode::HighContrast)
                    .on(|this, _, cx| {
                        this.toggle_high_contrast(cx);
                    })
                    .into(),
            ]),
            SEP,
        ]),
        Kind::Workbook => entries.extend([
            sub("xlsx.freeze", "view_column", vec![
                item("xlsx.freeze", "")
                    .on(|this, _, cx| {
                        this.toggle_freeze(cx);
                    })
                    .into(),
                item("xlsx.freeze-panes", "")
                    .on(|this, _, cx| {
                        this.freeze_panes(cx);
                    })
                    .into(),
                item("xlsx.freeze-top-row", "")
                    .on(|this, _, cx| {
                        this.freeze_top_row(cx);
                    })
                    .into(),
                item("xlsx.freeze-first-column", "")
                    .on(|this, _, cx| {
                        this.freeze_first_column(cx);
                    })
                    .into(),
                item("xlsx.unfreeze-panes", "")
                    .on(|this, _, cx| {
                        this.unfreeze_panes(cx);
                    })
                    .into(),
            ]).into(),
            item("xlsx.ruler", "rule").into(),
            sub("xlsx.ruler-unit", "straighten", vec![
                item("xlsx.ruler-unit.cm", "")
                    .checked(editor.ruler_unit == RulerUnit::Centimeters)
                    .disabled(!editor.current_sheet().is_some_and(|(_, s)| s.page_layout && s.show_ruler))
                    .on(|this, _, cx| {
                        this.set_ruler_unit(RulerUnit::Centimeters, cx);
                    })
                    .into(),
                item("xlsx.ruler-unit.mm", "")
                    .checked(editor.ruler_unit == RulerUnit::Millimeters)
                    .disabled(!editor.current_sheet().is_some_and(|(_, s)| s.page_layout && s.show_ruler))
                    .on(|this, _, cx| {
                        this.set_ruler_unit(RulerUnit::Millimeters, cx);
                    })
                    .into(),
                item("xlsx.ruler-unit.inch", "")
                    .checked(editor.ruler_unit == RulerUnit::Inches)
                    .disabled(!editor.current_sheet().is_some_and(|(_, s)| s.page_layout && s.show_ruler))
                    .on(|this, _, cx| {
                        this.set_ruler_unit(RulerUnit::Inches, cx);
                    })
                    .into(),
                item("xlsx.ruler-unit.point", "")
                    .checked(editor.ruler_unit == RulerUnit::Points)
                    .disabled(!editor.current_sheet().is_some_and(|(_, s)| s.page_layout && s.show_ruler))
                    .on(|this, _, cx| {
                        this.set_ruler_unit(RulerUnit::Points, cx);
                    })
                    .into(),
                item("xlsx.ruler-unit.pica", "")
                    .checked(editor.ruler_unit == RulerUnit::Picas)
                    .disabled(!editor.current_sheet().is_some_and(|(_, s)| s.page_layout && s.show_ruler))
                    .on(|this, _, cx| {
                        this.set_ruler_unit(RulerUnit::Picas, cx);
                    })
                    .into(),
            ]),
            item("xlsx.gridlines", "grid_on").into(),
            item("xlsx.headings", "view_headline").into(),
            item("xlsx.formula-bar", "functions").into(),
            item("xlsx.show-formulas", "functions")
                .shortcut("⌘`")
                .into(),
            item("xlsx.show-zeros", "exposure_zero").into(),
            sub("xlsx.view-mode", "grid_view", vec![
                item("xlsx.normal-view", "grid_view").into(),
                item("xlsx.page-break-preview", "insert_page_break").into(),
                item("xlsx.page-layout", "description").into(),
            ]),
            item("xlsx.zoom-selection", "aspect_ratio")
                .disabled(editor.selection.is_some() || !editor.selected_ids().is_empty() || editor.selected_range().is_none())
                .on(|this, window, cx| {
                    this.zoom_to_selection(window, cx);
                })
                .into(),
            item("xlsx.custom-views", "view_quilt")
                .disabled(editor.presentation().has_tables())
                .on(|this, window, cx| {
                    this.toggle_custom_views_panel(window, cx);
                })
                .into(),
            SEP,
        ]),
        Kind::Document => entries.extend([
            sub("docx.interaction", "edit_document", vec![
                item("docx.interaction.editing", "edit")
                    .checked(editor.is_editing_mode())
                    .disabled(editor.is_track_changes_locked())
                    .on(|this, window, cx| {
                        this.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Editing, window, cx);
                    })
                    .into(),
                item("docx.interaction.reviewing", "rate_review")
                    .checked(editor.is_reviewing_mode())
                    .on(|this, window, cx| {
                        this.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Reviewing, window, cx);
                    })
                    .into(),
                item("docx.interaction.viewing", "visibility")
                    .checked(editor.is_viewing_mode())
                    .on(|this, window, cx| {
                        this.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Viewing, window, cx);
                    })
                    .into(),
            ]),
            SEP,
            item("docx.view.read-mode", "chrome_reader_mode")
                .checked(editor.is_read_mode())
                .on(|this, window, cx| {
                    this.set_document_view_mode(ibis_doc_model::DocumentViewMode::Reading, window, cx);
                })
                .into(),
            item("docx.print-layout", "description")
                .checked(editor.is_print_layout_mode())
                .on(|this, window, cx| {
                    this.set_document_view_mode(ibis_doc_model::DocumentViewMode::Print, window, cx);
                })
                .into(),
            item("docx.view.web-layout", "web")
                .checked(editor.is_web_layout_mode())
                .on(|this, window, cx| {
                    this.set_document_view_mode(ibis_doc_model::DocumentViewMode::Web, window, cx);
                })
                .into(),
            item("docx.view.outline", "view_list")
                .checked(editor.is_outline_mode())
                .on(|this, window, cx| {
                    this.set_document_view_mode(ibis_doc_model::DocumentViewMode::Outline, window, cx);
                })
                .into(),
            item("docx.view.draft", "article")
                .checked(editor.is_draft_mode())
                .on(|this, window, cx| {
                    this.set_document_view_mode(ibis_doc_model::DocumentViewMode::Draft, window, cx);
                })
                .into(),
            item("docx.view.focus", "center_focus_strong")
                .checked(editor.is_focus_mode())
                .on(|this, window, cx| {
                    this.toggle_focus_mode(window, cx);
                })
                .into(),
            SEP,
            item("docx.ruler", "rule").on(|this, _, cx| this.open_ruler_panel(cx)).into(),
            item("docx.formatting-marks", "notes").checked(editor.formatting_marks_open).on(|this, _, cx| {
                this.toggle_formatting_marks(cx);
            })
            .into(),
            item("docx.white-space", "vertical_distribute").checked(editor.show_white_space).on(|this, _, cx| {
                this.toggle_white_space(cx);
            })
            .into(),
            item("docx.navigation-pane", "toc").checked(editor.outline_open).on(|this, _, cx| {
                this.outline_open = !this.outline_open;
                cx.notify();
            })
            .into(),
            SEP,
            sub("docx.page-movement", "swap_horiz", vec![
                item("docx.page-movement.vertical", "swap_vert")
                    .checked(!editor.page_movement_side_to_side)
                    .on(|this, window, cx| {
                        this.set_page_movement_side_to_side(false, window, cx);
                    })
                    .into(),
                item("docx.page-movement.side-to-side", "swap_horiz")
                    .checked(editor.page_movement_side_to_side)
                    .on(|this, window, cx| {
                        this.set_page_movement_side_to_side(true, window, cx);
                    })
                    .into(),
            ]),
            SEP,
            item("docx.view.one-page", "description").checked(editor.is_zoom_one_page()).on(|this, window, cx| {
                this.zoom_one_page(window, cx);
            })
            .into(),
            item("docx.view.multiple-pages", "filter_none").checked(editor.is_zoom_multiple_pages()).on(|this, window, cx| {
                this.zoom_multiple_pages(window, cx);
            })
            .into(),
            item("docx.view.page-width", "fit_screen").checked(editor.is_zoom_page_width()).on(|this, window, cx| {
                this.zoom_page_width(window, cx);
            })
            .into(),
            item("docx.view.zoom-100", "zoom_in").shortcut("⌘0").checked(editor.is_zoom_100()).on(|this, window, cx| {
                this.zoom_100_percent(window, cx);
            })
            .into(),
            SEP,
        ]),
    }
    entries.extend([
        MenuItem::tr("view.sidebar")
            .checked(editor.right_pane.shown)
            .disabled(!editor.right_pane.available)
            .on(|this, window, cx| {
                this.toggle_sidebar(window, cx);
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
            item("view.zoom-fit", "fit_screen").shortcut("⌥⌘0").disabled(kind == Kind::Workbook).checked(editor.zoom.is_none()).on(|this, window, cx| this.set_zoom(None, window, cx)).into(),
            SEP,
            item("view.zoom-dialog", "").on(|this, window, cx| this.open_zoom_dialog(window, cx)).into(),
        ]),
        item("view.fullscreen", "fullscreen").shortcut("⌃⌘F").on(|_, window, _| window.toggle_fullscreen()).into(),
    ]);
    entries
}

fn window_menu(editor: &SlideEditor, kind: Kind) -> Vec<MenuEntry> {
    let mut entries: Vec<MenuEntry> = Vec::new();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let label_key = match kind {
            Kind::Document => "window.open-in-word",
            Kind::Workbook => "window.open-in-excel",
            Kind::Presentation => "window.open-in-powerpoint",
        };
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
        item("window.arrange-all", "grid_view").on(|this, window, cx| {
            if this.is_workbook() {
                this.toggle_arrange_windows_panel(window, cx);
            } else {
                SlideEditor::arrange_all_windows(Some(window), cx);
            }
        }).into(),
        item("window.split", "splitscreen")
            .shortcut("⌥⌘S")
            .checked(if editor.is_workbook() { editor.split_panes().is_some() } else { editor.is_document_split() })
            .on(|this, _, cx| {
                if this.is_workbook() {
                    this.toggle_split(cx);
                } else if this.is_document() {
                    this.toggle_document_split(cx);
                }
            })
            .into(),
        item("window.cascade", "filter_none").on(|_, window, cx| SlideEditor::cascade_windows(Some(window), cx)).into(),
    ]);
    match kind {
        Kind::Workbook => entries.extend([
            SEP,
            item("xlsx.new-window", "open_in_new")
                .shortcut("⌘⇧N")
                .on(|this, _, cx| {
                    this.new_window(cx);
                })
                .into(),
            item("xlsx.arrange-all", "dashboard")
                .on(|this, window, cx| {
                    this.toggle_arrange_windows_panel(window, cx);
                })
                .into(),
            item("xlsx.view-side-by-side", "splitscreen")
                .checked(editor.side_by_side)
                .on(|this, window, cx| {
                    this.toggle_view_side_by_side(window, cx);
                })
                .into(),
            item("xlsx.sync-scrolling", "sync")
                .checked(editor.sync_scrolling)
                .disabled(!editor.side_by_side)
                .on(|this, _window, cx| {
                    this.toggle_sync_scrolling(cx);
                })
                .into(),
            item("xlsx.reset-window-position", "restart_alt")
                .disabled(!editor.side_by_side)
                .on(|this, window, cx| {
                    this.reset_side_by_side_positions(window, cx);
                })
                .into(),
            item("xlsx.hide-window", "visibility_off")
                .disabled(editor.presentation().is_workbook_window_hidden() || editor.presentation().is_workbook_windows_locked())
                .on(|this, _window, cx| {
                    this.hide_workbook_window(cx);
                })
                .into(),
            item("xlsx.unhide-window", "visibility")
                .disabled(!editor.presentation().is_workbook_window_hidden() || editor.presentation().is_workbook_windows_locked())
                .on(|this, window, cx| {
                    this.toggle_unhide_window_panel(window, cx);
                })
                .into(),
        ]),
        Kind::Document => entries.extend([
            SEP,
            item("docx.new-window", "open_in_new")
                .shortcut("⌘⇧N")
                .on(|this, _, cx| {
                    this.new_window(cx);
                })
                .into(),
            item("docx.arrange-all", "grid_view")
                .on(|_, window, cx| {
                    SlideEditor::arrange_all_windows(Some(window), cx);
                })
                .into(),
            item(if editor.is_document_split() { "docx.remove-split" } else { "docx.split" }, "splitscreen")
                .shortcut("⌥⌘S")
                .checked(editor.is_document_split())
                .on(|this, _, cx| {
                    this.toggle_document_split(cx);
                })
                .into(),
            item("docx.view-side-by-side", "splitscreen")
                .checked(editor.side_by_side)
                .on(|this, window, cx| {
                    this.toggle_view_side_by_side(window, cx);
                })
                .into(),
            item("docx.sync-scrolling", "sync")
                .checked(editor.sync_scrolling)
                .disabled(!editor.side_by_side)
                .on(|this, _window, cx| {
                    this.toggle_sync_scrolling(cx);
                })
                .into(),
            item("docx.reset-window-position", "restart_alt")
                .disabled(!editor.side_by_side)
                .on(|this, window, cx| {
                    this.reset_side_by_side_positions(window, cx);
                })
                .into(),
        ]),
        Kind::Presentation => {}
    }
    let open_windows = editor.open_window_menu_items();
    if !open_windows.is_empty() {
        entries.push(SEP);
        entries.extend(open_windows);
    }
    entries
}

fn insert_menu(editor: &SlideEditor, kind: Kind) -> Vec<MenuEntry> {
    let text_box = item("insert.text-box", "text_fields");
    let text_box = if editor.allows_shapes() { text_box.on(|this, window, cx| this.set_tool(Tool::TextBox, window, cx)) } else { text_box };
    let mut entries = Vec::new();
    if kind == Kind::Workbook {
        entries.extend([
            sub("xlsx.insert-cells-menu", "add", vec![
                item("xlsx.insert-row-above", "add").into(),
                item("xlsx.insert-row-below", "add").into(),
                item("xlsx.insert-column-left", "add").into(),
                item("xlsx.insert-column-right", "add").into(),
                item("xlsx.insert-cells", "add").into(),
            ]),
            SEP,
            item("xlsx.insert-sheet", "note_add").shortcut("⇧F11").into(),
            SEP,
            item("xlsx.insert-table", "table").shortcut("⌘T").into(),
            SEP,
        ]);
    }
    if kind == Kind::Document {
        entries.extend([
            item("docx.page-break", "insert_page_break").shortcut("⌘↩").into(),
            item("docx.column-break", "vertical_split").shortcut("⌃⇧↩").into(),
            item("docx.header-footer", "view_quilt").into(),
            item("docx.page-number", "tag").into(),
            item("docx.toc", "toc").into(),
            SEP,
        ]);
    }
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
    if kind == Kind::Document {
        entries.push(item("insert.rich-text", "text_fields").into());
        entries.push(item("insert.plain-text", "short_text").into());
        entries.push(item("insert.picture-control", "image").into());
        entries.push(item("insert.building-block-gallery", "dashboard_customize").into());
        entries.push(item("insert.repeating-section", "control_point_duplicate").into());
        entries.push(item("insert.group", "group_work").into());
        entries.push(item("insert.checkbox", "checklist").into());
        entries.push(item("insert.dropdown-list", "arrow_drop_down_circle").into());
        entries.push(item("insert.combo-box", "arrow_drop_down").into());
        entries.push(item("insert.date-picker", "schedule").into());
        entries.push(item("insert.form-text", "text_snippet").into());
        entries.push(item("insert.form-checkbox", "check_box").into());
        entries.push(item("insert.form-dropdown", "list").into());
    }
    entries
}

fn format_menu(kind: Kind) -> Vec<MenuEntry> {
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
    if kind == Kind::Document {
        entries.extend([
            SEP,
            item("docx.form.checkbox.toggle", "check").into(),
            item("docx.form.dropdown.cycle", "autorenew").into(),
            item("docx.form.legacy-dropdown.cycle", "autorenew").into(),
            item("docx.form.date.step", "schedule").into(),
            item("docx.form.picture.replace", "image").into(),
            item("docx.form.picture.reset", "restart_alt").into(),
            item("docx.form.gallery.cycle", "autorenew").into(),
            item("docx.form.gallery.reset", "restart_alt").into(),
            item("docx.form.repeating.add", "add").into(),
            item("docx.form.repeating.delete", "delete").into(),
            item("docx.form.repeating.cycle", "autorenew").into(),
            item("docx.form.group", "group_work").into(),
            item("docx.form.ungroup", "workspaces").into(),
            item("docx.form.design-mode", "design_services").into(),
            item("docx.form.properties", "settings").into(),
        ]);
    }
    if kind == Kind::Workbook {
        entries.extend([
            SEP,
            item("xlsx.chart-format", "bar_chart").into(),
            item("xlsx.move-chart", "drive_file_move").into(),
            item("xlsx.chart.save-template", "save").into(),
            item("xlsx.chart.apply-template", "style").into(),
        ]);
    }
    entries
}

fn data_menu() -> Vec<MenuEntry> {
    vec![
        item("xlsx.sort", "sort").into(),
        item("xlsx.filter", "filter_alt").into(),
        item("xlsx.table-to-range", "table").into(),
        SEP,
        item("xlsx.protect", "lock").into(),
        item("xlsx.named-ranges", "label").into(),
        item("xlsx.validation", "rule").into(),
        item("xlsx.validation.circle-invalid", "radio_button_unchecked").into(),
        item("xlsx.validation.clear-circles", "clear").into(),
        item("xlsx.split-text", "view_column").into(),
    ]
}

fn slide_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let can_delete = editor.presentation().slides.len() > 1;
    vec![
        item("pptx.new-slide", "add").shortcut("⌘M").on(|this, window, cx| this.add_slide(window, cx)).into(),
        item("pptx.duplicate-slide", "content_copy").into(),
        item("pptx.delete-slide", "delete").disabled(!can_delete).on(|this, window, cx| this.delete_slide(window, cx)).into(),
        item("pptx.skip-slide", "visibility_off").into(),
        SEP,
        item("pptx.transition", "animation").into(),
        item("pptx.background", "wallpaper").into(),
        sub("pptx.layout", "view_quilt", vec![]),
        item("pptx.theme", "palette").into(),
        item("pptx.edit-theme", "dashboard_customize").into(),
        SEP,
        item("slideshow.settings", "settings").into(),
    ]
}

fn arrange_menu() -> Vec<MenuEntry> {
    vec![
        sub("pptx.order", "flip_to_front", vec![
            item("pptx.order.front", "flip_to_front").shortcut("⌥⇧⌘↑").into(),
            item("pptx.order.forward", "arrow_upward").shortcut("⌥⌘↑").into(),
            item("pptx.order.backward", "arrow_downward").shortcut("⌥⌘↓").into(),
            item("pptx.order.back", "flip_to_back").shortcut("⌥⇧⌘↓").into(),
        ]),
        sub("pptx.align", "align_horizontal_left", vec![]),
        sub("pptx.distribute", "horizontal_distribute", vec![]),
        sub("pptx.center", "center_focus_strong", vec![]),
        sub("pptx.merge", "cell_merge", vec![]),
        item("pptx.edit-points", "polyline").into(),
        item("pptx.edit-points.smooth", "line_curve").into(),
        item("pptx.edit-points.straight", "polyline").into(),
        item("pptx.edit-points.corner", "turn_sharp_right").into(),
        item("pptx.smartart-to-shapes", "shapes").into(),
        item("pptx.smartart.convert-text", "account_tree").into(),
        item("pptx.smartart.add-shape", "add_circle").into(),
        item("pptx.smartart.remove-shape", "remove_circle").into(),
        item("pptx.smartart.add-assistant", "person_add").into(),
        item("pptx.smartart.move-up", "arrow_upward").into(),
        item("pptx.smartart.move-down", "arrow_downward").into(),
        item("pptx.smartart.promote", "keyboard_double_arrow_up").into(),
        item("pptx.smartart.demote", "keyboard_double_arrow_down").into(),
        item("pptx.smartart.text-pane", "list_alt").into(),
        sub("pptx.smartart.change-layout", "dashboard_customize", vec![
            item("pptx.smartart.layout.list", "view_list").into(),
            item("pptx.smartart.layout.vlist", "format_list_bulleted").into(),
            item("pptx.smartart.layout.process", "arrow_forward").into(),
            item("pptx.smartart.layout.chevron", "double_arrow").into(),
            item("pptx.smartart.layout.cycle", "autorenew").into(),
            item("pptx.smartart.layout.hierarchy", "account_tree").into(),
            item("pptx.smartart.layout.relationship", "sync_alt").into(),
            item("pptx.smartart.layout.matrix", "grid_view").into(),
            item("pptx.smartart.layout.pyramid", "change_history").into(),
            item("pptx.smartart.layout.picture", "insert_photo").into(),
        ]),
        sub("pptx.smartart.color", "palette", vec![
            item("pptx.smartart.color.colorful", "palette").into(),
            item("pptx.smartart.color.accent1", "palette").into(),
            item("pptx.smartart.color.accent2", "palette").into(),
            item("pptx.smartart.color.accent3", "palette").into(),
            item("pptx.smartart.color.outline", "check_box_outline_blank").into(),
        ]),
        sub("pptx.rotate", "rotate_90_degrees_cw", vec![
            item("pptx.rotate.right", "rotate_90_degrees_cw").into(),
            item("pptx.rotate.left", "rotate_90_degrees_ccw").into(),
            SEP,
            item("pptx.rotate.flip-h", "flip").into(),
            item("pptx.rotate.flip-v", "flip").into(),
        ]),
        SEP,
        item("pptx.group", "group_work").shortcut("⌥⌘G").into(),
        item("pptx.ungroup", "workspaces").shortcut("⌥⇧⌘G").into(),
    ]
}

// ----- Toolbar -------------------------------------------------------------

/// The toolbar's left part every format has: メニューを検索 and the format painter.
pub(crate) fn toolbar_left(editor: &SlideEditor) -> Vec<ToolItem> {
    let mut items: Vec<ToolItem> = vec![ToolButton::new("menu-search", "search", crate::shell::i18n::text("_shell.search-menus")).searchable().into()];
    match editor.kind() {
        Kind::Presentation => items.push(
            // A split button as PowerPoint's: the left half adds a slide in the layout of the current one, the ▼ has the
            // layouts to add one in.
            ToolButton::new("new-slide", "add", crate::shell::i18n::text("_shell.new-slide-m"))
                .menu({
                    let mut items = crate::pptx::chrome::new_slide_menu(editor);
                    if items.is_empty() {
                        items.push(MenuItem::tr("pptx.new-slide.blank").icon("add").on(|this, window, cx| this.add_slide(window, cx)).into());
                    }
                    items
                })
                .quick(|this, window, cx| this.add_slide(window, cx))
                .into(),
        ),
        Kind::Document => items.push(ToolButton::new("spelling", "spellcheck", "_shell.spelling-and-grammar").into()),
        Kind::Workbook => {}
    }
    // The format connects it (`FormatEditor::format_paint`).
    let mut paint = ToolButton::new("format-paint", "format_paint", crate::shell::i18n::text("_shell.copy-paste-formatting"));
    if let Some((on, handler)) = crate::shell::format::with_format!(editor, format_paint(editor)) {
        paint.on = on;
        paint.handler = Some(handler);
    }
    items.push(paint.into());
    items
}

fn sep() -> ToolItem {
    ToolItem::Separator
}

/// Text format buttons for the selected shape's text (what the editor can
/// change now: size, bold, italic, alignment).
fn text_format(editor: &SlideEditor, font_default: &'static str) -> Vec<ToolItem> {
    let style = editor.selected_shape().and_then(|s| s.text_body()).map(|b| b.style.clone());
    let on = style.is_some();
    let font: SharedString = style.as_ref().and_then(|s| s.font.clone()).unwrap_or_else(|| font_default.to_string()).into();
    let align = style.as_ref().map(|s| s.align);
    let align_item = |id: &'static str, icon: &'static str, label: &'static str, a: TextAlign| -> MenuEntry {
        MenuItem::new(id, label).icon(icon).checked(align == Some(a)).on(move |this, window, cx| this.update_style(window, cx, |s| s.align = a)).into()
    };
    vec![
        sep(),
        ToolItem::Select { id: "font", value: font, width: 128.0, tooltip: "_shell.font".into(), menu: Vec::new(), disabled: true },
        sep(),
        ToolItem::FontSize {
            value: style.as_ref().map(|s| format!("{}", s.font_size_pt()).into()),
            smaller: on.then(|| -> crate::shell::chrome::Handler { std::rc::Rc::new(|this: &mut SlideEditor, window, cx| this.step_font_size(false, window, cx)) }),
            larger: on.then(|| -> crate::shell::chrome::Handler { std::rc::Rc::new(|this: &mut SlideEditor, window, cx| this.step_font_size(true, window, cx)) }),
        },
        sep(),
        ToolButton::new("bold", "format_bold", "_shell.bold-b").on(style.as_ref().is_some_and(|s| s.bold)).disabled(!on).click(|this, window, cx| this.update_style(window, cx, |s| s.bold = !s.bold)).into(),
        ToolButton::new("italic", "format_italic", "_shell.italic-i").on(style.as_ref().is_some_and(|s| s.italic)).disabled(!on).click(|this, window, cx| this.update_style(window, cx, |s| s.italic = !s.italic)).into(),
        ToolButton::new("underline", "format_underlined", "_shell.underline-u").into(),
        ToolButton::new("text-color", "format_color_text", "_shell.font-color").color(crate::shell::palette::doc(0x000000).into()).into(),
        sep(),
        ToolButton::new("align", "format_align_left", "_shell.arrange")
            .disabled(!on)
            .menu(vec![
                align_item("align.left", "format_align_left", "align.left", TextAlign::Left),
                align_item("align.center", "format_align_center", "align.center", TextAlign::Center),
                align_item("align.right", "format_align_right", "align.right", TextAlign::Right),
            ])
            .into(),
        ToolButton::new("line-spacing", "format_line_spacing", "_shell.line-paragraph-spacing").into(),
        ToolButton::new("bullets", "format_list_bulleted", "_shell.bullets").into(),
        ToolButton::new("numbered", "format_list_numbered", "_shell.numbering").into(),
        ToolButton::new("indent-less", "format_indent_decrease", "_shell.decrease-indent").into(),
        ToolButton::new("indent-more", "format_indent_increase", "_shell.increase-indent").into(),
        ToolButton::new("clear-format", "format_clear", "pptx.chrome.clear-all-formatting").into(),
    ]
}

/// The toolbar's right half until the format gives its own.
pub(crate) fn toolbar_right(editor: &SlideEditor) -> Vec<ToolItem> {
    match editor.kind() {
        Kind::Presentation => {
            let tool = editor.tool;
            let mut items: Vec<ToolItem> = vec![
                ToolButton::new("tool-select", "arrow_selector_tool", "_shell.select-esc").on(tool == Tool::Select).click(|this, window, cx| this.set_tool(Tool::Select, window, cx)).into(),
                ToolButton::new("tool-text", "text_fields", "_shell.text-box").on(tool == Tool::TextBox).click(|this, window, cx| this.set_tool(Tool::TextBox, window, cx)).into(),
                ToolButton::new("insert-image", "image", "_shell.insert-picture").menu(Vec::new()).into(),
                ToolButton::new("insert-shape", "shapes", "_shell.shape").menu(Vec::new()).into(),
                ToolButton::new("insert-line", "horizontal_rule", "_shell.line-2").menu(Vec::new()).into(),
            ];
            if editor.selected_shape().is_some_and(|s| s.text_body().is_some()) {
                items.extend([sep(), ToolButton::new("fill", "format_color_fill", "_shell.fill-color").color(rgb(0xcfe2f7).into()).into(), ToolButton::new("border-color", "border_color", "_shell.border-color").color(rgb(0x444746).into()).into()]);
                items.extend(text_format(editor, "IBM Plex Sans"));
            } else if editor.selection.is_none() {
                items.extend([
                    sep(),
                    ToolButton::text("background", "_shell.background", "_shell.background").into(),
                    ToolButton::text("layout", "_shell.layout", "_shell.layout").menu(Vec::new()).into(),
                    ToolButton::text("theme", "_shell.theme", "_shell.theme").into(),
                    ToolButton::text("transition", "_shell.transitions", "_shell.transitions").into(),
                ]);
            }
            items
        }
        Kind::Workbook => vec![
            ToolButton::text("currency", "¥", "_shell.accounting-number-format").into(),
            ToolButton::new("percent", "percent", "_shell.percent-style").into(),
            ToolButton::text("comma", ",", "_shell.comma-style").into(),
            ToolButton::new("decimal-less", "decimal_decrease", "_shell.decrease-decimal").into(),
            ToolButton::new("decimal-more", "decimal_increase", "_shell.increase-decimal").into(),
            ToolButton::text("number-format", "123", "_shell.more-number-formats").menu(Vec::new()).into(),
            sep(),
            ToolItem::Select { id: "font", value: "IBM Plex Sans".into(), width: 128.0, tooltip: "_shell.font".into(), menu: Vec::new(), disabled: true },
            sep(),
            ToolItem::FontSize { value: None, smaller: None, larger: None },
            sep(),
            ToolButton::new("bold", "format_bold", "_shell.bold-b").into(),
            ToolButton::new("italic", "format_italic", "_shell.italic-i").into(),
            ToolButton::new("underline", "format_underlined", "_shell.underline-u").into(),
            ToolButton::new("text-color", "format_color_text", "_shell.font-color").color(crate::shell::palette::doc(0x000000).into()).into(),
            sep(),
            ToolButton::new("fill", "format_color_fill", "_shell.fill-color").color(crate::shell::palette::doc(0xffffff).into()).into(),
            ToolButton::new("borders", "border_all", "_shell.line").menu(Vec::new()).into(),
            ToolButton::new("merge", "cell_merge", "_shell.merge-cells").menu(Vec::new()).into(),
            sep(),
            ToolButton::new("halign", "format_align_left", "_shell.horizontal-alignment").menu(Vec::new()).into(),
            ToolButton::new("valign", "vertical_align_bottom", "_shell.vertical-alignment").menu(Vec::new()).into(),
            ToolButton::new("wrap", "wrap_text", "_shell.wrap-text").menu(Vec::new()).into(),
            ToolButton::new("rotate", "text_rotate_up", "_shell.text-rotation").menu(Vec::new()).into(),
            sep(),
            ToolButton::new("link", "link", "_shell.insert-link").into(),
            ToolButton::new("insert-chart", "insert_chart", "_shell.insert-chart").into(),
            ToolButton::new("filter", "filter_alt", "_shell.create-filter").menu(Vec::new()).into(),
            ToolButton::new("functions", "functions", "_shell.functions").menu(Vec::new()).into(),
        ],
        Kind::Document => {
            let mut items: Vec<ToolItem> = vec![ToolItem::Select { id: "style", value: crate::shell::i18n::text("_shell.normal-text").into(), width: 112.0, tooltip: "_shell.style".into(), menu: Vec::new(), disabled: true }];
            items.extend(text_format(editor, "IBM Plex Sans"));
            items
        }
    }
}

// ----- Sidebar -------------------------------------------------------------

/// A labelled value of a sidebar panel.
fn field(label: &'static str, value: String) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .py(px(4.))
        .child(div().w(px(72.)).text_size(px(13.)).text_color(text_secondary()).child(label))
        .child(
            div()
                .flex_1()
                .h(px(32.))
                .px(px(8.))
                .flex()
                .items_center()
                .border_1()
                .border_color(rgb(0x747775))
                .rounded(px(4.))
                .text_size(px(14.))
                .child(value),
        )
}

/// The sidebar until the format gives its own: a presentation's selected
/// shape's size and rotation (read from the model), or its slide.
pub(crate) fn sidebar(editor: &mut SlideEditor, _window: &mut Window, _cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
    if editor.kind() != Kind::Presentation {
        return None;
    }
    let cm = |emu: i64| format!("{:.2} cm", emu as f64 / 360_000.0);
    let body = match editor.selected_shape() {
        Some(shape) => div()
            .px(px(18.))
            .py(px(8.))
            .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).py(px(6.)).child(crate::shell::i18n::text("_shell.size-rotation")))
            .child(field("_shell.width", cm(shape.frame.width)))
            .child(field("_shell.height", cm(shape.frame.height)))
            .child(field("_shell.rotate", format!("{}°", shape.rotation as f64 / 60_000.0)))
            .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).pt(px(14.)).pb(px(6.)).child(crate::shell::i18n::text("_shell.position")))
            .child(field("X", cm(shape.frame.x)))
            .child(field("Y", cm(shape.frame.y))),
        None => {
            let index = editor.presentation().slide_index(editor.current_slide).unwrap_or(0) + 1;
            let size = editor.presentation().slide_size;
            div()
                .px(px(18.))
                .py(px(8.))
                .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).py(px(6.)).child(crate::shell::i18n::format("pptx.chrome.slide-index", &[("index", &index)])))
                .child(field("_shell.width", cm(size.width)))
                .child(field("_shell.height", cm(size.height)))
        }
    };
    let title = if editor.selection.is_some() { "format.options" } else { "menu.slide" };
    Some(SidebarPanel { title: title.into(), body: body.into_any_element() })
}

/// Whether two zoom factors are approximately equal.
fn zoom_approx(current: f32, target: f32) -> bool {
    (current - target).abs() < 0.02
}


