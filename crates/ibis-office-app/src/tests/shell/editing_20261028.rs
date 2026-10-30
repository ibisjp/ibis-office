// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pointer and keyboard regressions for the second 2026-10-28 UI request.
use super::{Scenario, button_probe as buttons};
use crate::{docx::scenarios::DocSteps, xlsx::scenarios::SheetSteps};
use gpui_kit::*;
use ibis_doc_model::{CellValue, Command, EmuRect, FlowPos, FlowRunKind, Shape};
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::edit20261028_empty_bar_signed", empty_bar),
        ("shell::edit20261028_ruby_apply", ruby),
        ("shell::edit20261028_shape_click_group_drag", shape_click),
        ("shell::edit20261028_docx_click_group_drag", docx_click),
        ("shell::edit20261028_custom_colors", custom_colors),
        ("shell::edit20261028_recent_menu", recent),
        ("shell::edit20261028_rotated_cursors", cursors),
        ("shell::edit20261028_headers_auto", headers),
        ("shell::edit20261028_cut_outline", cut_outline),
        ("shell::edit20261028_live_handles", live_handles),
        ("shell::edit20261028_caret_blink", blink),
        ("shell::edit20261028_segment_motion", segment_motion),
        ("shell::edit20261028_insert_text_edit", insert_text),
        ("shell::edit20261028_font_size_no_search", font_popup),
    ]
}
fn workbook() -> Scenario {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    s
}
fn empty_bar() {
    let mut s = workbook();
    s.click_cell("A1");
    s.enter("8", "enter");
    s.click_cell("B1");
    // Typed in the cell (Enter mode) the arrow points at A1. In the bar (Edit mode, measured in Excel 2026-10-28)
    // the arrow moves the caret instead.
    s.type_text("+");
    s.key("left");
    assert_eq!(s.with(|e, _| e.cell_edit().unwrap().text.clone()), "+A1");
    s.key("enter");
    assert_eq!(
        s.with(|e, _| e
            .current_sheet()
            .unwrap()
            .1
            .cell(0, 1)
            .unwrap()
            .value
            .clone()),
        CellValue::Number(8.)
    );
    s.click_cell("B2");
    s.enter("-123", "enter");
    assert_eq!(
        s.with(|e, _| e
            .current_sheet()
            .unwrap()
            .1
            .cell(1, 1)
            .unwrap()
            .value
            .clone()),
        CellValue::Number(-123.)
    );
    s.screenshot("signed-formula");
}
fn ruby() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Document, w, cx));
    s.type_text("Ruby");
    s.key("cmd-a");
    s.menu("docx.text.ruby");
    s.type_text("Reading");
    buttons::click(&mut s, "ruby-apply");
    assert!(s.document().body.iter().filter_map(|b|if let ibis_doc_model::Block::Paragraph(p)=b{Some(p)}else{None}).flat_map(|p|&p.runs).any(|r|matches!(&r.kind,FlowRunKind::Ruby{base,text,..}if base=="Ruby"&&text=="Reading")));
    s.screenshot("ruby-applied");
}
fn shape_click() {
    for kind in [ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Presentation] {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| {
            e.new_file(kind, w, cx);
            e.zoom = Some(1.);
            let first = e.slide().shapes.len();
            let commands = [1000000, 3500000]
                .into_iter()
                .enumerate()
                .map(|(i, x)| Command::InsertShape {
                    slide: e.current_slide,
                    index: first + i,
                    shape: Shape::text_box(
                        e.document.ids().new_shape_id(),
                        EmuRect::new(x, 1000000, 1500000, 800000),
                        "Text",
                    ),
                })
                .collect();
            e.run(commands, cx);
            e.set_selection(Vec::new());
        });
        let indices = s.with(|e, _| (e.slide().shapes.len() - 2, e.slide().shapes.len() - 1));
        s.click_shape(indices.0);
        assert!(
            s.with(|e, _| if kind == ibis_pptx::Kind::Workbook {
                e.editing.is_some()
            } else {
                e.text_editing().is_some()
            }),
            "single click enters text {kind:?}"
        );
        let at = s.shape_position(indices.1);
        s.dispatch_click_with(
            at,
            Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(
            s.with(|e, _| e.selected_ids().len()),
            2,
            "group selection {kind:?}"
        );
        let before = s.with(|e, _| {
            [
                e.slide().shapes[indices.0].frame,
                e.slide().shapes[indices.1].frame,
            ]
        });
        let from = s.shape_position(indices.0);
        s.drag(from, (from.0 + 30., from.1 + 20.));
        assert_eq!(
            s.with(|e, _| e.selected_ids().len()),
            2,
            "group selection {kind:?}"
        );
        let after = s.with(|e, _| {
            [
                e.slide().shapes[indices.0].frame,
                e.slide().shapes[indices.1].frame,
            ]
        });
        assert_ne!(before, after);
        assert_eq!(
            (after[0].x - before[0].x, after[0].y - before[0].y),
            (after[1].x - before[1].x, after[1].y - before[1].y)
        );
        s.screenshot(&format!("group-{kind:?}"));
    }
}
fn docx_click() {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Document, w, cx));
    let (p, _) = s.paragraph(0);
    s.update(|e, _, cx| {
        let commands = [1000000, 4000000]
            .into_iter()
            .enumerate()
            .map(|(i, x)| {
                let mut shape = Shape::text_box(
                    e.document.ids().new_shape_id(),
                    EmuRect::new(0, 0, 2000000, 1000000),
                    "Text",
                );
                shape.floating = Some(ibis_doc_model::FloatingAnchor {
                    offset_x: x,
                    offset_y: 2000000,
                    relative_h: "page".into(),
                    relative_v: "page".into(),
                    wrap: "none".into(),
                    allow_overlap: true,
                    ..Default::default()
                });
                ibis_doc_model::FlowCommand::InsertField {
                    at: FlowPos::new(p, i),
                    kind: FlowRunKind::Shape(Box::new(shape)),
                }
            })
            .collect();
        e.run_flow(commands, cx);
    });
    let points = |s: &mut Scenario| {
        s.with(|e, _| {
            (0..2)
                .map(|i| {
                    let (page, (x, y, w, h), _) =
                        e.floating_text_box_frame_of(FlowPos::new(p, i)).unwrap();
                    let (o, k) = e.page_origin(page).unwrap();
                    (
                        f32::from(o.x) + (x + w / 2.) * k,
                        f32::from(o.y) + (y + h / 2.) * k,
                    )
                })
                .collect::<Vec<_>>()
        })
    };
    let at = points(&mut s);
    s.click(at[0]);
    assert!(s.with(|e, _| e.doc_state().unwrap().box_edit.is_some()));
    s.dispatch_click_with(
        at[1],
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(s.with(|e, _| e.all_selected_shapes().len()), 2);
    let before = points(&mut s);
    s.drag(before[0], (before[0].0 + 30., before[0].1 + 20.));
    assert_eq!(s.with(|e, _| e.all_selected_shapes().len()), 2);
    let after = points(&mut s);
    assert!((after[0].0 - before[0].0 - 30.).abs() < 1.);
    assert!((after[1].0 - before[1].0 - 30.).abs() < 1.);
    s.screenshot("docx-group");
}
fn custom_colors() {
    let _probe = buttons::Probe::new();
    let mut s = workbook();
    s.click_cell("A1");
    s.enter("Color", "enter");
    s.click_cell("A1");
    let id = buttons::all()
        .into_iter()
        .find(|id| id.starts_with("text-color-") && id.ends_with("-menu"))
        .unwrap();
    buttons::click(&mut s, &id);
    buttons::click(&mut s, "menu-item-color.palette.more");
    assert_eq!(
        s.with(|e, _| e.panel_dialog.key),
        Some("color_picker_dialog")
    );
    let hex = s.with(|_, cx| buttons::inputs(cx).last().unwrap().clone());
    s.update(|_, w, cx| {
        hex.update(cx, |i, cx| {
            i.focus(w, cx);
            i.select_all(w, cx);
        })
    });
    s.type_text("123ABC");
    buttons::click(&mut s, "color-picker-ok");
    assert_eq!(
        s.with(|e, _| e.current_style().color),
        ibis_doc_model::Rgb(0x123abc)
    );
    s.screenshot("arbitrary-color");
}
fn recent() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::start_view();
    s.update(|e, _, cx| {
        e.recent_files = crate::shell::recent_files::add(
            &super::repo_root().join("test/editing/docx-report.docx"),
        );
        cx.notify();
    });
    let b = buttons::bounds("row-actions-0").unwrap();
    s.hover((f32::from(b.center().x), f32::from(b.center().y)));
    buttons::click(&mut s, "row-actions-0");
    let id = buttons::all()
        .into_iter()
        .find(|id| id.contains("searchable-item-row-actions") && id.ends_with(", 0)"))
        .unwrap();
    let b = buttons::bounds(&id).unwrap();
    s.hover((f32::from(b.center().x), f32::from(b.center().y)));
    assert_eq!(s.with(|e, _| e.toolbar_search_open), Some("row-actions"));
    s.click((f32::from(b.center().x), f32::from(b.center().y)));
    assert!(s.with(|e, _| {
        !e.recent_files
            .contains(&super::repo_root().join("test/editing/docx-report.docx"))
    }));
}
fn cursors() {
    use crate::shell::selection_handles::rotated_cursor;
    use ibis_doc_model::ResizeHandle;
    assert_eq!(
        rotated_cursor(ResizeHandle::Top, 0),
        CursorStyle::ResizeUpDown
    );
    assert_eq!(
        rotated_cursor(ResizeHandle::Top, -45 * 60000),
        CursorStyle::ResizeUpLeftDownRight
    );
    assert_eq!(
        rotated_cursor(ResizeHandle::Top, 90 * 60000),
        CursorStyle::ResizeLeftRight
    );
    assert_eq!(
        rotated_cursor(ResizeHandle::Right, 23 * 60000),
        CursorStyle::ResizeUpLeftDownRight
    );
}
fn headers() {
    let mut s = workbook();
    for (cell, value) in [
        ("B1", "Sales"),
        ("C1", "Profit"),
        ("A2", "Tokyo"),
        ("B2", "1"),
        ("C2", "2"),
        ("A3", "Osaka"),
        ("B3", "3"),
        ("C3", "4"),
    ] {
        s.click_cell(cell);
        s.enter(value, "enter");
    }
    s.update(|e, w, cx| e.go_to_reference("A1:C3", w, cx));
    let (by, refs) = s.with(|e, _| {
        let sheet = e.slide().name.as_ref().unwrap();
        let r = ibis_doc_model::CellRange {
            first_row: 0,
            first_column: 0,
            last_row: 2,
            last_column: 2,
        };
        (
            ibis_pptx::preferred_series_columns(e.presentation(), sheet, r),
            ibis_pptx::refs_for_range(e.presentation(), sheet, r, true).unwrap(),
        )
    });
    assert!(by);
    assert_eq!(refs.len(), 2);
    assert!(refs[0].name.as_ref().is_some_and(|n| n.contains("$B$1")));
    assert!(
        refs[0]
            .values
            .as_ref()
            .is_some_and(|n| n.contains("$B$2:$B$3"))
    );
}

fn workspace_crop(s: &mut Scenario, path: std::path::PathBuf) -> image::RgbImage {
    let (b, width) = s.update(|e, w, _| {
        (
            e.workspace_bounds.unwrap(),
            f32::from(w.viewport_size().width),
        )
    });
    let image = image::open(path).unwrap().to_rgb8();
    let scale = image.width() as f32 / width;
    image::imageops::crop_imm(
        &image,
        (f32::from(b.left()) * scale).ceil() as u32,
        (f32::from(b.top()) * scale).ceil() as u32,
        (f32::from(b.size.width) * scale).floor() as u32,
        (f32::from(b.size.height) * scale).floor() as u32,
    )
    .to_image()
}
fn blink() {
    for kind in [ibis_pptx::Kind::Document, ibis_pptx::Kind::Workbook] {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| e.new_file(kind, w, cx));
        if kind == ibis_pptx::Kind::Workbook {
            s.click_cell("A1");
        }
        s.type_text("Blink");
        let path = s.screenshot(&format!("caret-on-{kind:?}"));
        let a = workspace_crop(&mut s, path);
        std::thread::sleep(std::time::Duration::from_millis(550));
        s.update(|_, _, cx| cx.notify());
        let path = s.screenshot(&format!("caret-off-{kind:?}"));
        let b = workspace_crop(&mut s, path);
        assert!(a != b, "caret changes phase {kind:?}");
        std::thread::sleep(std::time::Duration::from_millis(550));
        s.update(|_, _, cx| cx.notify());
        let path = s.screenshot(&format!("caret-on-again-{kind:?}"));
        let c = workspace_crop(&mut s, path);
        assert!(a == c, "caret returns to its first phase {kind:?}");
    }
}
fn segment_fixture(
    e: &mut crate::shell::editor::SlideEditor,
    _: &mut Window,
    cx: &mut Context<crate::shell::editor::SlideEditor>,
) -> Option<crate::shell::chrome::SidebarPanel> {
    use crate::shell::chrome::segment_control::{Mode as SegmentMode, SegmentControl, SegmentItem};
    let selected = e.document_memory.get::<usize>("test.segment").unwrap_or(0);
    let control = SegmentControl::new(SegmentMode::Intrinsic)
        .available_width(px(130.))
        .children(
            ["First", "Long second"]
                .into_iter()
                .enumerate()
                .map(|(i, label)| {
                    SegmentItem::label(
                        if i == 0 {
                            "test-segment-first"
                        } else {
                            "test-segment-second"
                        },
                        label,
                        selected == i,
                    )
                    .on_click(cx.listener(move |e, _, _, cx| {
                        e.document_memory.set("test.segment", i);
                        cx.notify();
                    }))
                }),
        );
    Some(crate::shell::chrome::SidebarPanel {
        title: "Motion".into(),
        body: control.into_any_element(),
    })
}
fn segment_motion() {
    let _probe = buttons::Probe::new();
    let mut s = workbook();
    s.update(|e, _, _| e.sidebar_open = true);
    crate::shell::chrome::FORCED_SIDEBAR.with(|x| {
        x.set(Some((
            segment_fixture,
            "test-segment",
            crate::shell::panel_dialog::PanelMode::Modal,
        )))
    });
    s.update(|_, _, cx| cx.notify());
    let key = buttons::all()
        .into_iter()
        .find(|k| k.starts_with("segment-highlight-") && k.contains("test-segment-first"))
        .unwrap();
    let before = buttons::bounds(&key).unwrap();
    buttons::click(&mut s, "test-segment-second");
    std::thread::sleep(std::time::Duration::from_millis(50));
    s.update(|_, _, cx| cx.notify());
    let middle = buttons::bounds(&key).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(180));
    s.update(|_, _, cx| cx.notify());
    let end = buttons::bounds(&key).unwrap();
    let target = buttons::bounds("test-segment-second").unwrap();
    assert!(
        middle.origin.y > before.origin.y && middle.origin.y < end.origin.y,
        "wrap transition interpolates y"
    );
    assert!((target.origin.x - end.origin.x).abs() < px(1.));
    assert!((target.origin.y - end.origin.y).abs() < px(1.));
    assert!((target.size.width - end.size.width).abs() < px(1.));
    s.screenshot("wrapped-segment-motion");
    crate::shell::chrome::FORCED_SIDEBAR.with(|x| x.set(None));
}
fn insert_text() {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| {
        e.new_file(ibis_pptx::Kind::Document, w, cx);
        e.doc_insert_text_box(w, cx);
    });
    assert!(s.with(|e, _| e.doc_state().unwrap().box_edit.is_some()));
    s.type_text("Inserted");
    assert!(s.with(|e, _| {
        let at = e.doc_state().unwrap().box_edit.as_ref().unwrap().at;
        e.shape_text_body_at(at)
            .is_some_and(|b| b.text().contains("Inserted"))
    }));
    drop(s);
    for kind in [ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Presentation] {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| {
            e.new_file(kind, w, cx);
        });
        s.menu("insert.text-box");
        if kind == ibis_pptx::Kind::Presentation {
            let at=s.with(|e,_|{let layout=e.slide_layout().unwrap();(f32::from(layout.origin.x)+150.,f32::from(layout.origin.y)+150.)});
            s.drag(at,(at.0+140.,at.1+70.));
        }
        assert!(
            s.with(|e, _| if kind == ibis_pptx::Kind::Workbook {
                e.editing.is_some()
            } else {
                e.text_editing().is_some()
            }),
            "insert editing {kind:?}"
        );
        s.type_text("Inserted");
        assert!(s.with(|e, _| {
            e.slide()
                .shapes
                .iter()
                .filter_map(Shape::text_body)
                .any(|b| b.text().contains("Inserted"))
        }));
        s.screenshot(&format!("insert-edit-{kind:?}"));
    }
}
fn font_popup() {
    let _probe = buttons::Probe::new();
    let mut s = workbook();
    let key = buttons::all()
        .into_iter()
        .find(|k| k.starts_with("font-size-trigger-"))
        .unwrap();
    buttons::click(&mut s, &key);
    assert!(s.with(|e, _| e.font_size_menu_open));
    s.screenshot("font-size-candidates-no-search");
    s.key("down");
    s.key("enter");
    assert!(s.with(|e, _| !e.font_size_menu_open));
}

fn cut_outline() {
    let mut s = workbook();
    s.click_cell("A1");
    s.enter("Cut value", "enter");
    s.click_cell("A1");
    s.key("cmd-x");
    assert!(s.with(|e, _| e.wb().clip.as_ref().is_some_and(|c| c.cut)));
    s.screenshot("cut-source");
    s.click_cell("C3");
    s.screenshot("cut-source-after-selection-moves");
    s.key("cmd-v");
    assert_eq!(s.value("C3"), CellValue::Text("Cut value".into()));
}
fn live_handles() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| {
        e.new_file(ibis_pptx::Kind::Presentation, w, cx);
        e.zoom=Some(1.);
        let first = e.slide().shapes.len();
        let commands = [1000000, 3500000]
            .into_iter()
            .enumerate()
            .map(|(i, x)| Command::InsertShape {
                slide: e.current_slide,
                index: first + i,
                shape: Shape::text_box(
                    e.document.ids().new_shape_id(),
                    EmuRect::new(x, 1000000, 1500000, 800000),
                    "Text",
                ),
            })
            .collect();
        e.run(commands, cx);
        e.set_selection(Vec::new());
    });
    let first = s.with(|e, _| e.slide().shapes.len() - 2);
    let a = s.shape_position(first);
    let b = s.shape_position(first + 1);
    let from = point(px(a.0 - 100.), px(a.1 - 70.));
    let to = point(px(b.0 + 100.), px(b.1 + 70.));
    buttons::clear();
    s.dispatch(vec![
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: from,
            click_count: 1,
            ..Default::default()
        }),
        PlatformInput::MouseMove(MouseMoveEvent {
            position: to,
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        }),
    ]);
    assert!(s.with(|e, _| e.selected_ids().len()) >= 2);
    let handles = buttons::all()
        .into_iter()
        .filter(|id| id.contains("pptx-selection-handles-"))
        .count();
    assert!(handles >= 16, "live handles {handles}");
    s.screenshot("live-rubberband-handles");
    s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent {
        button: MouseButton::Left,
        position: to,
        click_count: 1,
        ..Default::default()
    })]);
}
