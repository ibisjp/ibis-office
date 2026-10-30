// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Settings dialogs: edit isolation, viewport access, movement and size limits.
use gpui_kit::{Bounds, Pixels};
use ibis_doc_model::{Block, FlowPos};
use crate::docx::scenarios::DocSteps;
use crate::shell::{panel_dialog::PanelMode, scenario::Scenario};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::panel_dialog_autocorrect_entry", autocorrect_entry),
        ("shell::panel_dialog_field_list_scroll", field_list_scroll),
        ("shell::panel_dialog_font_search_keyboard", font_search_keyboard),
        ("shell::panel_dialog_content_sizing", content_sizing),
        ("shell::panel_dialog_stable_top", stable_top),
        ("shell::panel_dialog_resize_scrollbar", resize_scrollbar),
        ("shell::panel_dialog_horizontal_overflow", horizontal_overflow),
        ("shell::panel_dialog_live_tab_rows",live_tab_rows),
        ("shell::panel_dialog_merge_row_draft",merge_row_draft),
        ("shell::panel_dialog_merge_reorder",merge_reorder),
        ("shell::panel_dialog_kinsoku_draft_field_height",kinsoku_draft_field_height),
        ("shell::panel_dialog_destructive_hover",destructive_hover),
        ("shell::panel_dialog_spinner_standard_width", spinner_standard_width),
        ("shell::panel_dialog_color_editor", color_editor_interaction),
        ("shell::panel_dialog_english_widgets", english_widgets),
        ("shell::panel_dialog_font_modal", font_modal),
        ("shell::panel_dialog_color_modal", color_modal),
        ("shell::panel_dialog_paragraph_modal", paragraph_modal),
        ("shell::panel_dialog_page_setup_modal", page_setup_modal),
        ("shell::panel_dialog_move_resize_and_close", move_resize_and_close),
        ("shell::panel_dialog_find_pane", find_pane),
        ("shell::panel_dialog_font_and_color_staging", font_and_color_staging),
        ("shell::panel_dialog_live_ruler_menu", live_ruler_menu),
        ("shell::panel_dialog_xlsx_command_and_cleanup", xlsx_command_and_cleanup),
        ("shell::panel_dialog_pptx_command_and_cleanup", pptx_command_and_cleanup),
        ("shell::panel_dialog_docx_compress_cleanup", docx_compress_cleanup),
    ]
}

fn text_position(s: &mut Scenario, pos: FlowPos) -> (f32, f32) {
    let (page, x, y, h) = s.update(|e, _, _| {
        let lines = e.flow_lines();
        e.caret_box(&lines, pos).expect("the test paragraph is laid out")
    });
    let (origin, scale) = s.with(|e, _| e.page_origin(page)).expect("the page is placed");
    (f32::from(origin.x) + x * scale + 0.5, f32::from(origin.y) + (y + h / 2.) * scale)
}

fn first_paragraph(s: &mut Scenario) -> u32 {
    s.document().body.iter().find_map(|block| match block {
        Block::Paragraph(p) if p.len() >= 6 => Some(p.id),
        _ => None,
    }).expect("a body paragraph with several characters")
}

#[derive(Clone, Copy, Debug)]
enum Settings { Font, Color, Paragraph, PageSetup, Print }

fn open_settings(s: &mut Scenario, kind: Settings) {
    s.update(|e, window, cx| match kind {
        Settings::Font => e.open_font_edit(window, cx),
        Settings::Color => e.open_color_edit(false, window, cx),
        Settings::Paragraph => e.open_paragraph_edit(None, window, cx),
        Settings::PageSetup => e.open_page_setup_panel(window, cx),
        Settings::Print => e.open_doc_print_panel(window, cx),
    });
    s.settle();
    assert!(s.with(|e, _| e.panel_dialog_is_modal()), "{kind:?} is modal");
    s.screenshot("settings-dialog");
}

fn modal_isolates_edits(kind: Settings) {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let para = first_paragraph(&mut s);
    s.click_text(FlowPos::new(para, 1));
    s.key("shift-right");
    s.key("shift-right");
    let selection = s.selection();
    let doc = s.document();
    let alternate = text_position(&mut s, FlowPos::new(para, 5));
    open_settings(&mut s, kind);
    if matches!(kind, Settings::PageSetup) {
        let (apply, cancel) = s.with(|e, _| {
            let ui = e.doc_state().unwrap().helper.page_setup.as_ref().unwrap().ui.borrow();
            (ui["apply"], ui["close"])
        });
        let height = gpui_kit::px(crate::shell::chrome::BUTTON_HEIGHT);
        assert_eq!(apply.size.height, height, "primary button uses the global height");
        assert_eq!(cancel.size.height, height, "normal button uses the same global height");
        assert_eq!(apply.origin.y, cancel.origin.y, "action buttons align vertically");
    }
    // Keep the document test point visible beside the floating dialog even when the text is centered.
    s.update(|e, _, cx| {
        if let Some(b) = e.panel_dialog.bounds.as_mut() { b.origin.x = gpui_kit::px(650.); }
        cx.notify();
    });
    s.settle();
    assert!(s.with(|e, _| !e.panel_dialog.bounds.unwrap().contains(&gpui_kit::point(gpui_kit::px(alternate.0), gpui_kit::px(alternate.1)))));
    s.click(alternate);
    s.double_click(alternate);
    s.shift_click(alternate);
    assert_eq!(s.selection(), selection, "{kind:?}: background pointer gestures cannot change selection");
    // Explicitly start from document focus: a dialog must also block typing if no field took focus.
    s.update(|e, window, cx| e.focus_handle.focus(window, cx));
    s.type_text("x");
    s.key("left");
    s.key("shift-right");
    s.key("backspace");
    s.key("cmd-b");
    assert!(s.document() == doc, "{kind:?}: background keyboard input cannot edit the document");
    assert_eq!(s.selection(), selection, "{kind:?}: background keyboard input cannot move selection");
    // File menu and toolbar are below the native title row and cannot accept clicks.
    s.click((76., 50.));
    let toolbar = s.with(|e, _| e.toolbar_bounds.get());
    s.click((f32::from(toolbar.origin.x) + 24., f32::from(toolbar.origin.y) + f32::from(toolbar.size.height) / 2.));
    assert!(s.with(|e, _| e.open_menu.is_none() && e.toolbar_search_open.is_none()));
    assert!(s.document() == doc, "the document remains unchanged");

    let workspace = s.with(|e, _| e.workspace_bounds.unwrap());
    let wheel_at = (f32::from(workspace.origin.x) + 32., f32::from(workspace.origin.y) + 180.);
    let before_scroll = s.with(|e, _| e.workspace_scroll.offset());
    s.wheel(wheel_at, -140.);
    s.settle();
    let after_scroll = s.with(|e, _| e.workspace_scroll.offset());
    assert!(after_scroll.y < before_scroll.y, "{kind:?}: the document still scrolls with the wheel");
    // Clicking the exposed vertical scrollbar track must also reach the viewport.
    s.click((f32::from(workspace.origin.x + workspace.size.width) - 5.,
        f32::from(workspace.origin.y + workspace.size.height) - 32.));
    s.settle();
    assert!(s.with(|e, _| e.workspace_scroll.offset().y) < after_scroll.y,
        "{kind:?}: the document scrollbar remains usable");
    let before_zoom = s.with(|e, _| e.current_zoom());
    s.key("cmd-+");
    assert!(s.with(|e, _| e.current_zoom()) > before_zoom, "{kind:?}: zoom shortcut remains available");
    s.key("cmd-0");
    assert!((s.with(|e, _| e.current_zoom()) - 1.).abs() < 0.001);
    let before_wheel_zoom = s.with(|e, _| e.current_zoom());
    s.wheel_zoom_lines(wheel_at, 1.);
    s.settle();
    assert_ne!(s.with(|e, _| e.current_zoom()), before_wheel_zoom, "{kind:?}: modified wheel still zooms");
    assert_eq!(s.selection(), selection);
    assert!(s.document() == doc, "the document remains unchanged");
    assert!(s.with(|e, _| e.panel_dialog_is_modal()));
}

fn font_modal() { modal_isolates_edits(Settings::Font); }
fn color_modal() { modal_isolates_edits(Settings::Color); }
fn paragraph_modal() { modal_isolates_edits(Settings::Paragraph); }
fn page_setup_modal() { modal_isolates_edits(Settings::PageSetup); }

fn dialog_bounds(s: &mut Scenario) -> Bounds<Pixels> {
    s.with(|e, _| e.panel_dialog.bounds.expect("dialog geometry"))
}

fn close_via_cross(s: &mut Scenario) {
    let b = dialog_bounds(s);
    s.click((f32::from(b.origin.x + b.size.width) - 26., f32::from(b.origin.y) + 22.));
    assert!(!s.with(|e, _| e.panel_dialog.active), "the title-bar cross closes the dialog");
}

fn move_resize_and_close() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    open_settings(&mut s, Settings::Print);
    let original_doc = s.document();
    let original_selection = s.selection();
    let before = dialog_bounds(&mut s);
    let viewport = s.update(|_, window, _| window.viewport_size());
    assert_eq!(before.origin.x, (viewport.width - before.size.width) / 2.);
    assert_eq!(before.origin.y, (viewport.height - before.size.height) / 2.);
    let scroll = s.with(|e, _| e.workspace_scroll.offset());
    let zoom = s.with(|e, _| e.current_zoom());
    let inside = (f32::from(before.origin.x) + 20., f32::from(before.origin.y) + 70.);
    s.wheel(inside, -140.);
    s.wheel_zoom_lines(inside, 1.);
    s.settle();
    assert_eq!(s.with(|e, _| e.workspace_scroll.offset()), scroll, "wheel inside dialog cannot scroll the document");
    assert_eq!(s.with(|e, _| e.current_zoom()), zoom, "wheel inside dialog cannot zoom the document");
    let title = (f32::from(before.origin.x) + 60., f32::from(before.origin.y) + 22.);
    use gpui_kit::{point, px, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};
    let at = |p: (f32, f32)| point(px(p.0), px(p.1));
    s.dispatch(vec![
        PlatformInput::MouseMove(MouseMoveEvent { position: at(title), ..Default::default() }),
        PlatformInput::MouseDown(MouseDownEvent { position: at(title), button: MouseButton::Left, click_count: 1, ..Default::default() }),
    ]);
    assert!(s.update(|_, window, _| window.captured_hitbox().is_some()), "title grab captures the pointer");
    // Cross the menu, toolbar and document, rendering each event as a real drag does.
    for target in [(title.0 + 20., 45.), (title.0 + 40., title.1 + 80.), (title.0 + 75., title.1 + 145.)] {
        s.dispatch(vec![PlatformInput::MouseMove(MouseMoveEvent { position: at(target), pressed_button: Some(MouseButton::Left), ..Default::default() })]);
        let current = dialog_bounds(&mut s);
        assert!((f32::from(current.origin.x - before.origin.x) - (target.0 - title.0)).abs() < 1.);
        assert!((f32::from(current.origin.y - before.origin.y) - (target.1 - title.1)).abs() < 1.);
    }
    s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent { position: at((title.0 + 75., title.1 + 145.)), button: MouseButton::Left, click_count: 1, ..Default::default() })]);
    let moved = dialog_bounds(&mut s);
    assert!((f32::from(moved.origin.x - before.origin.x) - 75.).abs() < 1.);
    assert!((f32::from(moved.origin.y - before.origin.y) - 145.).abs() < 1.);
    // Bring the bottom grip back into view after testing an unconstrained downward drag.
    let moved_title = (f32::from(moved.origin.x) + 60., f32::from(moved.origin.y) + 22.);
    s.drag(moved_title, (moved_title.0, moved_title.1 - 145.));
    let moved = dialog_bounds(&mut s);
    let grip = (f32::from(moved.origin.x + moved.size.width) - 3., f32::from(moved.origin.y + moved.size.height) - 3.);
    s.drag(grip, (f32::from(moved.origin.x) + 20., f32::from(moved.origin.y) + 20.));
    let small = dialog_bounds(&mut s);
    assert_eq!(f32::from(small.size.width), 320., "the minimum dialog width is 320px");
    assert!(f32::from(small.size.height) >= 180.);
    assert!(s.with(|e, _| e.panel_dialog.drag.is_none()), "release ends a move or resize");
    assert!(s.document() == original_doc, "the document remains unchanged");
    assert_eq!(s.selection(), original_selection, "moving and resizing the dialog does not select document text");
    s.screenshot("dialog-minimum-width");
    // The menu bar logo (Start) is an explicit exception to modal blocking.
    let original_windows = s.with_app(|app| app.windows().into_iter().map(|w| w.window_id()).collect::<Vec<_>>());
    let windows = original_windows.len();
    // The logo of the menu bar opens the start window (the title bar's icon is only a mark).
    let logo = s.with(|e, _| e.logo_bounds.get().expect("menu bar logo"));
    s.click((f32::from(logo.center().x), f32::from(logo.center().y)));
    assert_eq!(s.window_count(), windows + 1, "Start (the logo) remains available while a dialog is open");
    let new_window = s.with_app(|app| app.windows().into_iter().find(|w| !original_windows.contains(&w.window_id())).expect("new Start window"));
    s.close_window(new_window);
    assert_eq!(s.window_count(), windows);
    close_via_cross(&mut s);
    let para = first_paragraph(&mut s);
    s.click_text(FlowPos::new(para, 1));
    let before = s.document();
    s.type_text("X");
    assert!(s.document() != before, "closing restores document input");
}

fn find_pane() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let para = first_paragraph(&mut s);
    s.update(|e, window, cx| e.open_find(window, cx));
    assert!(s.with(|e, _| e.sidebar_open && !e.panel_dialog.active), "Find is a right pane rather than a dialog");
    let alternate = text_position(&mut s, FlowPos::new(para, 4));
    s.click(alternate);
    assert_eq!(s.selection().unwrap().1, FlowPos::new(para, 4), "Find pane permits document selection");
    let doc = s.document(); s.type_text("X");
    assert!(s.document() != doc, "Find pane permits editing");
    assert!(s.with(|e, _| e.sidebar_open && !e.panel_dialog.active));
}

fn font_and_color_staging() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let para = first_paragraph(&mut s);
    s.click_text(FlowPos::new(para, 1));
    s.key("shift-right");
    let original = s.document();
    open_settings(&mut s, Settings::Font);
    let font_name = s.with(|e, cx| {
        let field = &e.doc_state().unwrap().font_edit.as_ref().unwrap().fields[0];
        if field.read(cx).value().as_ref() == "Arial" { "Courier New" } else { "Arial" }
    });
    let trigger = s.with(|e, _| e.doc_state().unwrap().font_edit.as_ref().unwrap().ui.borrow()["docx-font-latin-list"]);
    s.click((f32::from(trigger.center().x), f32::from(trigger.center().y)));
    s.type_text(font_name);
    s.key("enter");
    assert!(s.document() == original, "choosing a font stages it until Apply");
    let bounds = dialog_bounds(&mut s);
    s.click((f32::from(bounds.center().x)-48., f32::from(bounds.bottom())-24.));
    assert!(s.document() != original, "Apply applies the selected font to the frozen selection");
    assert!(s.with(|e, _| e.panel_dialog_is_modal()));
    close_via_cross(&mut s);
    s.key("cmd-z");
    assert!(s.document() == original, "font application is one undo operation");

    open_settings(&mut s, Settings::Color);
    s.update(|e, window, cx| {
        let input = e.doc_state().unwrap().color_edit.as_ref().unwrap().field.clone();
        input.update(cx, |state, cx| state.focus(window, cx));
    });
    s.key("cmd-a");
    s.type_text("bad-input");
    s.key("cmd-enter");
    assert!(s.document() == original, "invalid RGB does not modify the document");
    assert!(s.with(|e, _| e.doc_state().unwrap().color_edit.as_ref().unwrap().error.is_some()));
    s.key("cmd-a");
    s.type_text("123456");
    assert!(s.document() == original, "RGB input is staged until Apply or Cmd+Enter");
    s.key("cmd-enter");
    assert!(s.document() != original, "the expected document edit is applied");
    close_via_cross(&mut s);
    s.key("cmd-z");
    assert!(s.document() == original, "the document remains unchanged");
}

fn live_ruler_menu() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let doc = s.document();
    assert!(s.menu_state("docx.ruler").is_some());
    for old in ["docx.vertical-ruler", "docx.ruler-unit.cm", "docx.gridlines"] {
        assert!(s.menu_state(old).is_none(), "{old} is no longer a separate View menu item");
    }
    s.menu("docx.ruler");
    assert!(s.with(|e, _| e.ruler_settings_open && e.sidebar_open && !e.panel_dialog.active));
    assert!(s.document() == doc, "opening ruler settings does not change document formatting");
    fn click_ruler(s: &mut Scenario, id: &str) {
        let bounds = s.with(|e, _| *e.ruler_settings_ui.borrow().get(id).unwrap_or_else(|| panic!("missing ruler UI {id}")));
        s.click((f32::from(bounds.origin.x + bounds.size.width / 2.), f32::from(bounds.origin.y + bounds.size.height / 2.)));
        s.settle();
    }
    for (id, read) in [
        ("ruler-horizontal", (|e: &crate::shell::editor::SlideEditor| e.ruler_open) as fn(&crate::shell::editor::SlideEditor) -> bool),
        ("ruler-vertical", (|e: &crate::shell::editor::SlideEditor| e.vertical_ruler_open) as fn(&crate::shell::editor::SlideEditor) -> bool),
        ("ruler-gridlines", (|e: &crate::shell::editor::SlideEditor| e.gridlines_open) as fn(&crate::shell::editor::SlideEditor) -> bool),
    ] {
        let before = s.with(|e, _| read(e));
        click_ruler(&mut s, id);
        assert_eq!(s.with(|e, _| read(e)), !before, "{id} updates immediately");
        assert!(s.document() == doc, "{id} changes the viewport without editing the document");
        click_ruler(&mut s, id);
        assert_eq!(s.with(|e, _| read(e)), before, "{id} can be restored by clicking again");
    }
    use crate::shell::editor::RulerUnit;
    for (id, unit) in [
        ("docx.ruler-unit.mm", RulerUnit::Millimeters),
        ("docx.ruler-unit.inch", RulerUnit::Inches),
        ("docx.ruler-unit.point", RulerUnit::Points),
        ("docx.ruler-unit.pica", RulerUnit::Picas),
        ("docx.ruler-unit.cm", RulerUnit::Centimeters),
    ] {
        click_ruler(&mut s, id);
        assert_eq!(s.with(|e, _| e.ruler_unit), unit, "{id} updates immediately");
        assert!(s.document() == doc, "ruler units do not change document dimensions");
    }
    assert!(s.with(|e, _| e.sidebar_open && !e.panel_dialog.active));
    s.screenshot("live-ruler-pane");
}


fn xlsx_command_and_cleanup() {
    use crate::xlsx::scenarios::SheetSteps;
    use ibis_doc_model::sheet_command::Axis;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B3");
    assert!(!s.with(|e, _| e.panel_dialog.active), "normal cell selection keeps its live formatting pane");
    let selected = s.selected();
    let presentation = s.with(|e, _| e.presentation().clone());
    let other_cell = s.with(|e, _| e.cell_center(2, 4).expect("C5 is on screen"));
    s.menu("xlsx.row-height");
    assert!(s.with(|e, _| e.panel_dialog_is_modal()));
    assert!(s.with(|e, _| e.wb().panels.row_col_size.as_ref().unwrap().axis == Axis::Rows));
    s.update(|e, window, cx| {
        if let Some(b) = e.panel_dialog.bounds.as_mut() {
            b.origin.x = gpui_kit::px(if f32::from(other_cell.x) < 600. { 650. } else { 0. });
        }
        e.focus_handle.focus(window, cx);
        cx.notify();
    });
    s.click((f32::from(other_cell.x), f32::from(other_cell.y)));
    s.type_text("blocked");
    s.key("right");
    s.key("backspace");
    assert_eq!(s.selected(), selected, "modal row height preserves the selected cell");
    assert!(s.with(|e, _| e.wb().cell_edit.is_none()), "typing cannot start a cell edit under a modal dialog");
    assert!(s.with(|e, _| e.presentation() == &presentation), "modal input leaves the workbook unchanged");
    close_via_cross(&mut s);
    assert!(s.with(|e, _| e.wb().panels.row_col_size.is_none()), "cross discards the row-height draft");
    s.menu("xlsx.column-width");
    assert!(s.with(|e, _| e.panel_dialog_is_modal()));
    assert!(s.with(|e, _| e.wb().panels.row_col_size.as_ref().unwrap().axis == Axis::Columns),
        "opening Column Width does not resurrect Row Height");
    close_via_cross(&mut s);
    s.click_cell("C5");
    assert_eq!(s.selected(), "C5");
    assert!(!s.with(|e, _| e.panel_dialog.active), "normal cell selection does not resurrect either dialog");
    assert!(s.with(|e, _| e.presentation() == &presentation));
}

fn pptx_command_and_cleanup() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.click_shape(0);
    let selected = s.with(|e, _| e.selected_ids());
    assert!(!selected.is_empty(), "fixture has a selectable shape");
    let presentation = s.with(|e, _| e.presentation().clone());
    let workspace = s.with(|e, _| e.workspace_bounds.unwrap());
    let background = (f32::from(workspace.origin.x) + 24., f32::from(workspace.origin.y) + 72.);
    s.menu("file.page-setup");
    assert!(s.with(|e, _| e.panel_dialog_is_modal() && e.pptx().unwrap().size_panel.is_some()));
    s.update(|e, window, cx| {
        if let Some(b) = e.panel_dialog.bounds.as_mut() { b.origin.x = gpui_kit::px(650.); }
        e.focus_handle.focus(window, cx);
        cx.notify();
    });
    s.click(background);
    assert_eq!(s.with(|e, _| e.selected_ids()), selected, "background click keeps selection");
    s.double_click(background);
    assert_eq!(s.with(|e, _| e.selected_ids()), selected, "background double click keeps selection");
    s.key("right");
    s.key("backspace");
    s.type_text("blocked");
    assert_eq!(s.with(|e, _| e.selected_ids()), selected, "modal slide size preserves shape selection");
    assert!(s.with(|e, _| e.presentation() == &presentation), "modal input cannot delete or alter a slide shape");
    // Enlarge via the permitted shortcut to create an overflowing slide viewport.
    let zoom = s.with(|e, _| e.current_zoom());
    for _ in 0..6 { s.key("cmd-+"); }
    assert!(s.with(|e, _| e.current_zoom()) > zoom, "PPTX zoom shortcut is allowed");
    s.update(|e, _, cx| {
        e.workspace_scroll.set_offset(gpui_kit::Point::default());
        cx.notify();
    });
    let before_scroll = s.with(|e, _| e.workspace_scroll.offset());
    s.wheel(background, -160.);
    s.settle();
    assert!(s.with(|e, _| e.workspace_scroll.offset().y) < before_scroll.y, "PPTX viewport still scrolls");
    assert!(s.with(|e, _| e.presentation() == &presentation));
    close_via_cross(&mut s);
    assert!(s.with(|e, _| e.pptx().unwrap().size_panel.is_none()), "cross discards the slide-size draft");
    s.key("cmd-0");
    s.click_shape(0);
    // A click inside a text shape puts the caret in its text (as PowerPoint does); Esc returns to the
    // selected shape, whose arrow keys then nudge it.
    s.key("escape");
    s.key("right");
    assert!(s.with(|e, _| e.presentation() != &presentation), "shape editing resumes after closing the dialog");
}

fn docx_compress_cleanup() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let original = s.document();
    s.update(|e, window, cx| e.open_docx_compress_panel(window, cx));
    assert!(s.with(|e, _| e.panel_dialog_is_modal() && e.doc_state().unwrap().compress_panel.is_some()));
    assert_eq!(s.with(|e, _| e.panel_dialog.key), Some("docx_compress_panel"));
    assert!(!s.with(|e,_|e.doc_state().unwrap().compress_panel.as_ref().unwrap().selected_only),"without a selected picture the target defaults to the entire file");
    close_via_cross(&mut s);
    let picture=s.with(|e,_|e.flow().unwrap().body.iter().find_map(|block| {
        let Block::Paragraph(p)=block else {return None;};
        let mut offset=0;
        for run in &p.runs {if matches!(run.kind,ibis_doc_model::FlowRunKind::Picture{..}) {return Some(FlowPos::new(p.id,offset));} offset+=run.len();}None
    }).unwrap());
    s.update(|e,_,cx|{e.set_caret(picture,false,cx);e.set_caret(FlowPos::new(picture.para,picture.offset+1),true,cx);});
    s.update(|e,w,cx|e.open_docx_compress_panel(w,cx));
    assert!(s.with(|e,_|e.doc_state().unwrap().compress_panel.as_ref().unwrap().selected_only),"selected picture is the default compression target");
    close_via_cross(&mut s);
    assert!(s.with(|e, _| e.doc_state().unwrap().compress_panel.is_none()), "cross discards compression settings");
    s.menu("file.page-setup");
    assert!(s.with(|e, _| e.panel_dialog_is_modal() && e.doc_state().unwrap().helper.page_setup.is_some()));
    assert_eq!(s.with(|e, _| e.panel_dialog.key), Some("doc_page_setup_panel"), "Page Setup replaces the closed compression dialog");
    assert!(s.with(|e, _| e.doc_state().unwrap().compress_panel.is_none()));
    close_via_cross(&mut s);
    assert!(s.document() == original, "opening and dismissing either command leaves the document unchanged");
}


fn english_widgets() {
    use crate::shell::i18n::{self, Language};
    struct RestoreLanguage;
    impl Drop for RestoreLanguage { fn drop(&mut self) { i18n::show(Language::Japanese); } }
    let _restore = RestoreLanguage;
    let mut s = Scenario::open("test/editing/docx-report.docx");
    i18n::show(Language::English);
    open_settings(&mut s, Settings::Font);
    assert_eq!(s.with(|e, _| e.panel_dialog.title.to_string()), "Font");
    s.screenshot("english-font-dialog");
    close_via_cross(&mut s);
    open_settings(&mut s, Settings::Color);
    assert_eq!(s.with(|e, _| e.panel_dialog.title.to_string()), "Font Color");
    s.screenshot("english-color-dialog");
    close_via_cross(&mut s);
    s.menu("docx.ruler");
    s.screenshot("english-ruler-pane");
}


fn font_search_keyboard() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let original = s.document();
    open_settings(&mut s, Settings::Font);
    for (index, id) in ["docx-font-latin-list", "docx-font-east-asian-list", "docx-font-complex-script-list"].into_iter().enumerate() {
        let trigger = s.with(|e, _| e.doc_state().unwrap().font_edit.as_ref().unwrap().ui.borrow()[id]);
        assert_eq!(f32::from(trigger.size.width),186., "font selectors use the shared standard width");
        s.click((f32::from(trigger.origin.x + trigger.size.width / 2.), f32::from(trigger.origin.y + trigger.size.height / 2.)));
        assert!(s.with(|e, _| e.toolbar_search_open == Some(id)), "the font arrow opens its searchable popup");
        s.type_text("Arial");
        assert_eq!(s.with(|e, cx| e.toolbar_search_inputs[id].read(cx).value().to_string()), "Arial", "the popup search accepts typing");
        s.screenshot("font-search-popup");
        s.key("enter");
        assert_eq!(s.with(|e, cx| e.doc_state().unwrap().font_edit.as_ref().unwrap().fields[index].read(cx).value().to_string()), "Arial",
            "search + Enter updates the staged font for {id}");
        assert!(s.document() == original, "selecting a candidate does not apply document formatting");
    }
    s.screenshot("font-search-selection");
}


fn horizontal_overflow() {
    use gpui_kit::*;
    use crate::shell::chrome::{SidebarPanel,FORCED_SIDEBAR};
    use crate::shell::editor::SlideEditor;
    fn panel(_: &mut SlideEditor,_:&mut Window,_:&mut Context<SlideEditor>)->Option<SidebarPanel> {
        Some(SidebarPanel {title:"Minimum width".into(),body:div().min_w(px(600.)).h(px(80.)).child("Wide content").into_any_element()})
    }
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e,w,cx|e.open_font_edit(w,cx));
    FORCED_SIDEBAR.with(|slot|slot.set(Some((panel,"minimum-width-test",PanelMode::Modal))));
    s.update(|_,_,cx|cx.notify());
    s.settle();
    let b = dialog_bounds(&mut s);
    let scroll = s.with(|e,_|e.panel_dialog.scroll.clone());
    assert!(scroll.max_offset().x > px(50.), "minimum width overflows the client area");
    assert!(scroll.max_offset().y < px(1.), "horizontal overflow does not force vertical overflow");
    let y = f32::from(b.bottom())-f32::from(s.with(|e,_|e.panel_dialog.footer_height))-6.;
    s.drag((f32::from(b.left())+4.,y),(f32::from(b.right())-20.,y));
    s.settle();
    assert!(scroll.offset().x < -px(30.), "persistent horizontal thumb scrolls minimum-width content");
    FORCED_SIDEBAR.with(|slot|slot.set(None));
}

fn resize_scrollbar() {
    use gpui_kit::px;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.update(|e,w,cx|e.open_validation_panel(w,cx));
    s.settle();
    let before = dialog_bounds(&mut s);
    let grip = (f32::from(before.right())-3., f32::from(before.bottom())-3.);
    s.drag(grip, (grip.0, f32::from(before.top())+180.-3.));
    s.settle();
    let small = dialog_bounds(&mut s);
    let scroll = s.with(|e,_|e.panel_dialog.scroll.clone());
    assert!(s.with(|e,_|e.panel_dialog.user_resized), "mouse drag resizes the dialog");
    assert!(scroll.max_offset().y > px(30.), "natural content overflows the resized client area");
    assert!(scroll.max_offset().x < px(1.), "responsive contents still fit the client width");
    // Drag the persistent thumb without a preceding wheel or hover activation.
    let x = f32::from(small.right())-6.;
    let top = f32::from(small.top())+46.;
    let bottom = f32::from(small.bottom())-f32::from(s.with(|e,_|e.panel_dialog.footer_height))-4.;
    s.drag((x,top+3.), (x,bottom));
    s.settle();
    assert!(scroll.offset().y < -px(20.), "the always-visible scrollbar scrolls the clipped content");
    s.screenshot("resized-dialog-scrollbar");
    let grip = (f32::from(small.right())-3.,f32::from(small.bottom())-3.);
    s.drag(grip,(grip.0,f32::from(before.bottom())+100.));
    s.settle();
    assert!(scroll.max_offset().y < px(1.), "enlarging the dialog removes overflow");
    assert!(scroll.offset().y.abs() < px(1.), "scroll position is clamped after enlargement");
}

fn stable_top() {
    use gpui_kit::px;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.update(|e,w,cx|e.open_validation_panel(w,cx));
    s.settle();
    let initial = dialog_bounds(&mut s);
    let viewport = s.update(|_,w,_|w.viewport_size());
    assert!((initial.origin.y - (viewport.height-initial.size.height)/2.).abs() < px(1.), "initial measured content is centered");
    for tab in [1,2,0,2,1,0] {
        s.update(|e,_,cx|{e.sheet_state_mut().unwrap().validation.as_mut().unwrap().tab=tab;cx.notify();});
        s.settle();
        assert_eq!(dialog_bounds(&mut s).origin.y, initial.origin.y, "tab {tab} keeps the top");
    }
    // A user-moved dialog also recenters if its newly grown content would leave the viewport.
    s.update(|e,_,cx|{e.panel_dialog.bounds.as_mut().unwrap().origin.y=viewport.height-px(10.);e.panel_dialog.user_moved=true;e.sheet_state_mut().unwrap().validation.as_mut().unwrap().tab=1;cx.notify();});
    s.settle();
    let fitted = dialog_bounds(&mut s);
    assert!(fitted.bottom() <= viewport.height);
    assert!((fitted.origin.y-(viewport.height-fitted.size.height)/2.).abs() < px(1.));
    s.update(|e,_,cx|{e.sheet_state_mut().unwrap().validation.as_mut().unwrap().tab=0;cx.notify();});
    s.settle();
    assert_eq!(dialog_bounds(&mut s).origin.y, fitted.origin.y, "shrinking retains the new top");
}

fn content_sizing() {
    let mut s=Scenario::open("test/editing/docx-report.docx");
    open_settings(&mut s,Settings::Font);
    let b=dialog_bounds(&mut s);
    assert_eq!(b.size.width, gpui_kit::px(320.), "40% reduction respects the 320px minimum");
    assert!(f32::from(b.size.height)<450., "font dialog fits its content instead of using 560px");
    close_via_cross(&mut s);
    open_settings(&mut s,Settings::Paragraph);
    assert_eq!(f32::from(dialog_bounds(&mut s).size.width),416.);
    close_via_cross(&mut s);
    s.update(|e,window,cx|e.open_tab_edit(window,cx));
    assert!(s.with(|e,_|e.sidebar_open && !e.panel_dialog.active));
    s.screenshot("tab-live-pane");
    s.update(|e,_,cx|{e.sidebar_open=false;if let Some(d)=e.doc_state_mut(){d.tab_edit=None;}cx.notify();});
    s.update(|e,w,cx|e.open_docx_compress_panel(w,cx));
    assert_eq!(dialog_bounds(&mut s).size.width,gpui_kit::px(320.),"compression reduction retains minimum width");
    s.screenshot("compact-compression-dialog");
    close_via_cross(&mut s);
    let para=first_paragraph(&mut s);s.click_text(FlowPos::new(para,1));s.key("shift-right");
    s.update(|e,w,cx|e.open_ruby(w,cx));
    assert_eq!(dialog_bounds(&mut s).size.width,gpui_kit::px(320.),"ruby reduction retains minimum width");
    s.type_text("ruby");
    s.screenshot("compact-ruby-dialog");
    close_via_cross(&mut s);
}

fn color_editor_interaction() {
    use crate::shell::color_editor::ColorAction;
    let mut s=Scenario::open("test/editing/docx-report.docx");
    let para=first_paragraph(&mut s);s.click_text(FlowPos::new(para,1));s.key("shift-right");
    let original=s.document();
    open_settings(&mut s,Settings::Color);
    // Hue must remain selectable even while value is black.
    s.update(|e,w,cx|e.edit_color_picker(ColorAction::Pick(ibis_doc_model::Rgb(0)),w,cx));
    s.update(|e,w,cx|e.edit_color_picker(ColorAction::Hue(240.),w,cx));
    s.update(|e,w,cx|e.edit_color_picker(ColorAction::Sv(1.,1.),w,cx));
    assert_eq!(s.with(|e,cx|e.doc_state().unwrap().color_edit.as_ref().unwrap().field.read(cx).value().to_string()),"0000FF");
    let wheel=s.with(|e,_|e.doc_state().unwrap().color_edit.as_ref().unwrap().picker.wheel_bounds.get().unwrap());
    // Click the top of the actual hue ring, keeping saturation and value at 100%.
    s.click((f32::from(wheel.center().x),f32::from(wheel.center().y)-106.));
    assert!((s.with(|e,_|e.doc_state().unwrap().color_edit.as_ref().unwrap().picker.hsv[0])-90.).abs()<1.);
    assert!(s.document()==original,"wheel only changes the staged preview");
    s.update(|e,w,cx|e.edit_color_picker(ColorAction::Mode(true),w,cx));
    s.update(|e,w,cx|e.edit_color_picker(ColorAction::Channel(3,18.),w,cx));
    s.update(|e,w,cx|e.edit_color_picker(ColorAction::Channel(4,52.),w,cx));
    s.update(|e,w,cx|e.edit_color_picker(ColorAction::Channel(5,86.),w,cx));
    assert_eq!(s.with(|e,cx|e.doc_state().unwrap().color_edit.as_ref().unwrap().field.read(cx).value().to_string()),"123456");
    s.screenshot("color-editor-rgb");
    s.update(|e,w,cx|e.apply_color_edit(w,cx));
    assert!(s.document()!=original);
    close_via_cross(&mut s);s.key("cmd-z");assert!(s.document()==original);
}

fn spinner_standard_width() {
    use gpui_kit::*;
    use gpui_kit::component::{h_flex,v_flex};
    use crate::shell::chrome::{SidebarPanel,SpinnerSpec,text_field,FORCED_SIDEBAR};
    use crate::shell::editor::SlideEditor;
    thread_local! {static WIDTHS:std::cell::RefCell<std::collections::BTreeMap<usize,Bounds<Pixels>>> = const {std::cell::RefCell::new(std::collections::BTreeMap::new())};}
    fn panel(e:&mut SlideEditor,_:&mut Window,cx:&mut Context<SlideEditor>)->Option<SidebarPanel> {
        let fields=e.doc_state()?.font_edit.as_ref()?.fields.clone();
        let rows=fields.iter().cloned().chain(std::iter::once(fields[0].clone())).enumerate().map(|(i,input)| {
            let field=match i {
                0=>text_field(&input).full_width().spinner("width-a",SpinnerSpec::new(1.),cx,|_,_,_,_|{}),
                1=>text_field(&input).width(px(300.)).spinner("width-b",SpinnerSpec::new(1.),cx,|_,_,_,_|{}),
                2=>text_field(&input).spinner("width-c",SpinnerSpec::new(1.),cx,|_,_,_,_|{}).full_width(),
                _=>text_field(&input).compact_spinner().spinner("width-color",SpinnerSpec::new(1.).range(0.,255.),cx,|_,_,_,_|{}),
            };
            h_flex().child(div().id(("field-wrapper",i)).relative().flex_none().child(field)
                .child(canvas(|_,_,_|(),move |bounds,_,_,_|WIDTHS.with(|map|{map.borrow_mut().insert(i,bounds);})).absolute().inset_0()))
        });
        Some(SidebarPanel{title:"Spinner widths".into(),body:v_flex().gap_2().children(rows).into_any_element()})
    }
    WIDTHS.with(|m|m.borrow_mut().clear());
    let mut s=Scenario::open("test/editing/docx-report.docx");
    s.update(|e,w,cx|e.open_font_edit(w,cx));
    FORCED_SIDEBAR.with(|slot|slot.set(Some((panel,"spinner-width-test",PanelMode::Sidebar))));
    s.update(|e,_,cx|{e.sidebar_open=true;cx.notify();});
    WIDTHS.with(|m|{let m=m.borrow();assert_eq!(m.len(),4);for (i,bounds) in m.iter(){assert_eq!(bounds.size.width,px(if *i==3 {80.} else {124.}),"only the compact color channel overrides the standard spinner width");}});
    FORCED_SIDEBAR.with(|slot|slot.set(None));
}

fn live_tab_rows() {
    let mut s=Scenario::open("test/editing/docx-report.docx");let p=first_paragraph(&mut s);
    s.click_text(FlowPos::new(p,1));s.update(|e,w,cx|e.open_tab_edit(w,cx));
    for value in ["72","144","216"] {
        s.update(|e,w,cx|{e.open_tab_row_editor(None,w,cx);let i=e.doc_state().unwrap().tab_edit.as_ref().unwrap().pos_field.clone();i.update(cx,|i,cx|i.set_value(value,w,cx));e.finish_tab_row_editor(w,cx);});
    }
    assert_eq!(s.with(|e,_|ibis_doc_model::flow_command::paragraph(e.flow().unwrap(),p).unwrap().tabs.len()),3);
    let duplicate_before=s.document();
    s.update(|e,w,cx|{e.open_tab_row_editor(None,w,cx);let i=e.doc_state().unwrap().tab_edit.as_ref().unwrap().pos_field.clone();i.update(cx,|i,cx|i.set_value("72",w,cx));e.finish_tab_row_editor(w,cx);});
    assert_eq!(s.document(),duplicate_before,"adding a duplicate must not overwrite an existing row");
    assert!(s.with(|e,_|e.doc_state().unwrap().tab_edit.as_ref().unwrap().error.is_some()));s.update(|e,w,cx|e.close_panel_dialog(w,cx));
    let before=s.document();s.update(|e,w,cx|e.open_tab_row_editor(Some(1),w,cx));
    let b=dialog_bounds(&mut s);assert_eq!(b.right(),s.update(|_,w,_|w.viewport_size().width));
    s.update(|e,w,cx|e.close_panel_dialog(w,cx));assert_eq!(s.document(),before);
    assert!(s.with(|e,_|e.sidebar_open && e.doc_state().unwrap().tab_edit.is_some()));
    s.update(|e,w,cx|{e.open_tab_row_editor(Some(1),w,cx);let i=e.doc_state().unwrap().tab_edit.as_ref().unwrap().pos_field.clone();i.update(cx,|i,cx|i.set_value("180",w,cx));e.finish_tab_row_editor(w,cx);});
    assert_eq!(s.with(|e,_|ibis_doc_model::flow_command::paragraph(e.flow().unwrap(),p).unwrap().tabs.len()),3,"moving a tab replaces its old position");
    s.update(|e,w,cx|e.delete_live_tab(1,w,cx));assert_eq!(s.with(|e,_|ibis_doc_model::flow_command::paragraph(e.flow().unwrap(),p).unwrap().tabs.len()),2);
    s.key("cmd-z");assert_eq!(s.with(|e,_|e.doc_state().unwrap().tab_edit.as_ref().unwrap().stops.len()),3,"undo refreshes live rows");
}

fn merge_row_draft() {
    let mut s=Scenario::open("test/editing/docx-report.docx");
    s.update(|e,w,cx|{e.open_merge(w,cx);e.set_merge_data(vec!["Name".into(),"City".into()],vec![vec!["Alice".into(),"Tokyo".into()],vec!["Bob".into(),"Osaka".into()]],cx);e.open_merge_row(Some(0),w,cx);});
    s.update(|e,w,cx|{let i=e.doc_state().unwrap().merge.as_ref().unwrap().row_inputs[0].clone();i.update(cx,|i,cx|i.set_value("Changed",w,cx));e.close_panel_dialog(w,cx);});
    assert_eq!(s.with(|e,_|e.doc_state().unwrap().merge.as_ref().unwrap().records[0][0].clone()),"Alice");
    s.update(|e,w,cx|{e.doc_state_mut().unwrap().merge.as_mut().unwrap().included[1]=false;e.open_merge_row(Some(0),w,cx);let i=e.doc_state().unwrap().merge.as_ref().unwrap().row_inputs[0].clone();i.update(cx,|i,cx|i.set_value("Changed",w,cx));e.save_merge_row(w,cx);});
    assert_eq!(s.with(|e,_|e.doc_state().unwrap().merge.as_ref().unwrap().records[0][0].clone()),"Changed");
    assert!(!s.with(|e,_|e.doc_state().unwrap().merge.as_ref().unwrap().included[1]),"editing a row preserves recipient inclusion");
}

fn destructive_hover() {
    use gpui_kit::*;use gpui_kit::component::{h_flex,v_flex};
    use crate::shell::chrome::{SidebarPanel,FORCED_SIDEBAR};
    thread_local!{static BUTTON:std::cell::Cell<Option<Bounds<Pixels>>>=const{std::cell::Cell::new(None)};}
    fn panel(_: &mut crate::shell::editor::SlideEditor,_:&mut Window,cx:&mut Context<crate::shell::editor::SlideEditor>)->Option<SidebarPanel>{
        let footer=crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("hover-delete","Delete",crate::shell::chrome::ActionButtonKind::Destructive).on_click(|_,_,_|{}).on_bounds(|b,_,_|BUTTON.with(|v|v.set(Some(b)))));
        Some(SidebarPanel::with_footer("Button colors",v_flex().p_3().child("Hover sample").into_any_element(),footer))
    }
    let mut s=Scenario::open("test/editing/docx-report.docx");s.update(|e,w,cx|e.open_font_edit(w,cx));
    FORCED_SIDEBAR.with(|slot|slot.set(Some((panel,"hover-test",PanelMode::Modal))));s.update(|e,_,cx|{e.sidebar_open=true;cx.notify();});
    let b=BUTTON.with(|v|v.get().unwrap());s.hover((f32::from(b.center().x),f32::from(b.center().y)));
    let viewport=s.update(|_,w,_|f32::from(w.viewport_size().width));let (width,rgba)=s.picture();let scale=width as f32/viewport;
    let x=((f32::from(b.origin.x)+6.)*scale) as usize;let y=((f32::from(b.origin.y)+8.)*scale) as usize;let at=(y*width as usize+x)*4;
    assert!(rgba[at]<255&&rgba[at]==rgba[at+1]&&rgba[at+1]==rgba[at+2],"destructive hover uses a neutral light-gray background");
    let red=(f32::from(b.origin.x)*scale) as usize;let mid=(f32::from(b.center().y)*scale) as usize;let colors=&rgba[(mid*width as usize+red)*4..];assert!(colors[0]>colors[1]+30,"destructive border stays red on hover");
    FORCED_SIDEBAR.with(|slot|slot.set(None));
}

fn merge_reorder() {
    let mut s=Scenario::open("test/editing/docx-report.docx");
    s.update(|e,w,cx|{e.open_merge(w,cx);e.set_merge_data(vec!["Name".into()],vec![vec!["Alice".into()],vec!["Bob".into()],vec!["Carol".into()]],cx);e.toggle_merge_record(2,cx);});
    s.update(|e,w,cx|{e.doc_state_mut().unwrap().merge.as_mut().unwrap().current=0;e.move_merge_recipient(2,0,w,cx);});
    assert_eq!(s.with(|e,_|{let m=e.doc_state().unwrap().merge.as_ref().unwrap();(m.records[0][0].clone(),m.included.clone(),m.current)}),("Carol".into(),vec![false,true,true],0),"drag keeps inclusion and current recipient with the moved record");
    s.update(|e,w,cx|e.move_merge_recipient(1,2,w,cx));
    assert_eq!(s.with(|e,_|{let m=e.doc_state().unwrap().merge.as_ref().unwrap();m.records[m.selected()[m.current]][0].clone()}),"Alice","preview follows the same included recipient when it moves");

}
fn kinsoku_draft_field_height() {
    let mut s=Scenario::open("test/editing/docx-report.docx");let before=s.document();
    s.update(|e,w,cx|e.open_kinsoku_panel(w,cx));s.update(|e,w,cx|{if e.strict_kinsoku(){e.select_normal_kinsoku_panel(w,cx);}else{e.select_strict_kinsoku_panel(w,cx);}});
    assert!(s.document()==before,"preset only stages the lists");
    s.update(|e,w,cx|e.close_panel_dialog(w,cx));assert!(s.document()==before,"cancel discards the preset");
    s.update(|e,w,cx|{e.open_kinsoku_panel(w,cx);e.select_normal_kinsoku_panel(w,cx);e.apply_kinsoku_panel(cx);});
    assert!(!s.with(|e,_|e.strict_kinsoku()));
    s.update(|e,w,cx|e.open_field_panel(w,cx));let bounds=dialog_bounds(&mut s);let height=s.update(|_,w,_|w.viewport_size().height);
    assert!((f32::from(bounds.size.height)-f32::from(height)+80.).abs()<2.,"field dialog uses the available height");
}

fn autocorrect_entry() {
    let mut s=Scenario::open("test/editing/docx-report.docx");
    s.size(1200.,1400.);
    s.update(|e,w,cx|e.open_doc_autocorrect_panel(w,cx));
    let before=s.with(|e,_|e.doc_state().unwrap().autocorrect.as_ref().unwrap().working.table.clone());
    let click=|s:&mut Scenario,id:&str|{let b=s.with(|e,_|e.doc_state().unwrap().autocorrect.as_ref().unwrap().ui.borrow()[id]);s.click((f32::from(b.origin.x+b.size.width/2.),f32::from(b.origin.y+b.size.height/2.)));};
    s.update(|e,_,cx|{e.doc_state_mut().unwrap().autocorrect.as_mut().unwrap().dictionary_tab=true;cx.notify();});
    click(&mut s,"add");
    assert!(s.with(|e,_|e.doc_state().unwrap().autocorrect.as_ref().unwrap().entry_open));
    s.update(|e,w,cx|{let p=e.doc_state().unwrap().autocorrect.as_ref().unwrap();p.replace.update(cx,|i,cx|i.set_value("brb",w,cx));p.with.update(cx,|i,cx|i.set_value("be right back",w,cx));});
    s.update(|e,w,cx|e.close_panel_dialog(w,cx));
    assert!(s.with(|e,_|e.doc_state().unwrap().autocorrect.as_ref().unwrap().working.table==before));
    assert!(s.with(|e,_|e.panel_dialog_is_modal()),"closing the child keeps the settings dialog open");
    click(&mut s,"add");
    s.update(|e,w,cx|{let p=e.doc_state().unwrap().autocorrect.as_ref().unwrap();p.replace.update(cx,|i,cx|i.set_value("brb",w,cx));p.with.update(cx,|i,cx|i.set_value("be right back",w,cx));});
    click(&mut s,"entry-ok");
    assert!(s.with(|e,_|e.doc_state().unwrap().autocorrect.as_ref().unwrap().working.table.iter().any(|(a,b)|a=="brb"&&b=="be right back")));
    s.update(|e,w,cx|e.close_panel_dialog(w,cx));
    assert!(!crate::shell::autocorrect::settings().table.iter().any(|(a,_)|a=="brb"),"parent cancellation discards the child changes");
}

fn field_list_scroll() {
    let mut s=Scenario::open("test/editing/docx-report.docx");
    s.update(|e,w,cx|e.open_field_panel(w,cx));
    let b=dialog_bounds(&mut s);let at=(f32::from(b.origin.x+gpui_kit::px(80.)),f32::from(b.origin.y+gpui_kit::px(180.)));
    s.wheel(at,-600.);
    assert!(s.with(|e,_|e.doc_state().unwrap().helper.field.as_ref().unwrap().list_scroll.offset().y)<gpui_kit::px(0.),"the list itself scrolls");
    assert_eq!(s.with(|e,_|e.panel_dialog.scroll.offset().y),gpui_kit::px(0.),"the dialog body has no scroll offset");
    s.screenshot("field-list-only-scroll");
}
