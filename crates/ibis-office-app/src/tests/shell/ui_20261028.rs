// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Common UI behavior requested on 2026-10-28; exercise real input routing.
use super::{Scenario, button_probe as buttons};
use crate::xlsx::scenarios::SheetSteps;
use crate::{
    docx::scenarios::DocSteps,
    shell::{
        chrome,
        editor::SlideEditor,
        panel_dialog::{PanelMode, SubmitType},
    },
};
use gpui_kit::{
    component::{
        h_flex,
        input::{InputState, Textarea, TextareaState},
        v_flex,
    },
    *,
};
use ibis_doc_model::{Command, EmuRect, FlowPos, Run, RunStyle, Shape};
#[derive(Clone, Copy)]
struct Form {
    fields: usize,
    area: bool,
    radio: bool,
    disabled_first: bool,
    auto_focus: bool,
    submit: SubmitType,
    normal: bool,
}
impl Default for Form {
    fn default() -> Self {
        Self {
            fields: 1,
            area: false,
            radio: false,
            disabled_first: false,
            auto_focus: true,
            submit: SubmitType::Auto,
            normal: false,
        }
    }
}
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        chrome::FORCED_SIDEBAR.with(|s| s.set(None));
        crate::native_shape_clipboard::simulate(None);
    }
}
fn fixture(
    e: &mut SlideEditor,
    w: &mut Window,
    cx: &mut Context<SlideEditor>,
) -> Option<chrome::SidebarPanel> {
    let config = e.document_memory.get::<Form>("ui.form").unwrap_or_default();
    let mut body = v_flex().gap_2();
    if config.radio {
        body = body.child(chrome::radio_button(
            "ui-radio",
            true,
            false,
            "Radio",
            |_, _, _| {},
        ));
    }
    for i in 0..config.fields {
        let input = e.ensure_toolbar_search_input(["ui-first", "ui-second", "ui-third"][i], w, cx);
        body = body.child(chrome::text_field(&input).disabled(i == 0 && config.disabled_first));
    }
    if config.area {
        let area = e
            .document_memory
            .get::<Entity<TextareaState>>("ui.area")
            .unwrap();
        body = body.child(Textarea::new(&area).h(px(70.)));
    }
    let kind = if config.normal {
        chrome::ActionButtonKind::Normal
    } else {
        chrome::ActionButtonKind::Primary
    };
    let footer = chrome::action_bar().action(
        chrome::ActionDefinition::new("ui-submit", "Submit", kind).on_click(cx.listener(
            |e, _, _, cx| {
                let n = e.document_memory.get::<u32>("ui.submits").unwrap_or(0);
                e.document_memory.set("ui.submits", n + 1);
                cx.notify();
            },
        )),
    );
    Some(
        chrome::SidebarPanel::with_footer("Form", body.into_any_element(), footer)
            .submit_type(config.submit)
            .auto_focus(config.auto_focus),
    )
}
fn form(config: Form) -> Scenario {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| {
        e.new_file(ibis_pptx::Kind::Document, w, cx);
        e.document_memory.set("ui.form", config);
        let area = cx.new(|cx| TextareaState::new(w, cx));
        e.document_memory.set("ui.area", area);
        e.sidebar_open = true;
    });
    chrome::FORCED_SIDEBAR.with(|s| s.set(Some((fixture, "ui-form", PanelMode::Modal))));
    s.update(|_, _, cx| cx.notify());
    s
}
fn submitted(s: &mut Scenario) -> u32 {
    s.with(|e, _| e.document_memory.get("ui.submits").unwrap_or(0))
}
fn forms() {
    let _reset = Reset;
    for config in [
        Form::default(),
        Form {
            fields: 2,
            ..Default::default()
        },
        Form {
            fields: 0,
            area: true,
            ..Default::default()
        },
        Form {
            fields: 2,
            submit: SubmitType::Enter,
            ..Default::default()
        },
        Form {
            submit: SubmitType::CtrlEnter,
            ..Default::default()
        },
        Form {
            submit: SubmitType::None,
            ..Default::default()
        },
        Form {
            fields: 2,
            area: true,
            normal: true,
            ..Default::default()
        },
    ] {
        let mut s = form(config);
        assert!(!s.with(|e, _| e.panel_dialog.form_controls.is_empty()));
        s.key("enter");
        let enter = config.normal
            || config.submit == SubmitType::Enter
            || config.submit == SubmitType::Auto && config.fields <= 1 && !config.area;
        assert_eq!(
            submitted(&mut s),
            u32::from(enter),
            "Enter policy {:?}",
            config.submit
        );
        s.key("cmd-enter");
        let modified =
            config.submit == SubmitType::CtrlEnter || config.submit == SubmitType::Auto && !enter;
        assert_eq!(
            submitted(&mut s),
            u32::from(enter) + u32::from(modified),
            "modified policy {:?}",
            config.submit
        );
        s.screenshot(&format!(
            "form-{}-{}-{:?}",
            config.fields, config.area, config.submit
        ));
        chrome::FORCED_SIDEBAR.with(|s| s.set(None));
    }
}
fn focus() {
    let _reset = Reset;
    for config in [
        Form::default(),
        Form {
            fields: 2,
            disabled_first: true,
            ..Default::default()
        },
        Form {
            radio: true,
            ..Default::default()
        },
        Form {
            auto_focus: false,
            ..Default::default()
        },
    ] {
        let mut s = form(config);
        let expected = if config.disabled_first {
            "ui-second"
        } else {
            "ui-first"
        };
        let focused = s.update(|e, w, cx| {
            e.toolbar_search_inputs[expected]
                .read(cx)
                .focus_handle(cx)
                .is_focused(w)
        });
        assert_eq!(focused, config.auto_focus && !config.radio);
        if focused {
            s.type_text("Focused");
            assert!(s.with(|e, cx| {
                e.toolbar_search_inputs[expected]
                    .read(cx)
                    .value()
                    .contains("Focused")
            }));
        }
        chrome::FORCED_SIDEBAR.with(|s| s.set(None));
    }
    let mut s = form(Form::default());
    s.update(|e, w, cx| {
        e.toolbar_search_inputs["ui-first"].update(cx, |i, cx| {
            i.replace_and_mark_text_in_range(None, "へ", Some(1..1), w, cx) // i18n-ok: Japanese IME composition fixture.
        })
    });
    s.key("enter");
    assert_eq!(submitted(&mut s), 0, "IME Enter must not submit the form");
}
fn font_sizes() {
    let _probe = buttons::Probe::new();
    for kind in [
        ibis_pptx::Kind::Document,
        ibis_pptx::Kind::Workbook,
        ibis_pptx::Kind::Presentation,
    ] {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| e.new_file(kind, w, cx));
        s.size(1400., 850.);
        if kind == ibis_pptx::Kind::Document {
            s.type_text("AB");
            let (p, _) = s.paragraph(0);
            s.update(|e, _, cx| {
                e.set_caret(FlowPos::new(p, 0), false, cx);
                e.set_caret(FlowPos::new(p, 1), true, cx);
                e.apply_text_properties(
                    ibis_doc_model::RunFormat {
                        size: Some(1000),
                        ..Default::default()
                    },
                    cx,
                );
                e.set_caret(FlowPos::new(p, 1), false, cx);
                e.set_caret(FlowPos::new(p, 2), true, cx);
                e.apply_text_properties(
                    ibis_doc_model::RunFormat {
                        size: Some(2000),
                        ..Default::default()
                    },
                    cx,
                );
                e.set_caret(FlowPos::new(p, 0), false, cx);
                e.set_caret(FlowPos::new(p, 2), true, cx);
            });
        } else if kind == ibis_pptx::Kind::Workbook {
            s.update(|e, w, cx| {
                for (cell, size) in [("A1", 1000), ("A2", 2000)] {
                    e.go_to_reference(cell, w, cx);
                    e.format_cells(ibis_doc_model::StyleChange::FontSize(size), cx);
                }
                e.go_to_reference("A1:A2", w, cx);
            });
        } else {
            s.update(|e, _, cx| {
                let id = e.document.ids().new_shape_id();
                let mut shape =
                    Shape::text_box(id, EmuRect::new(300000, 300000, 3000000, 1000000), "AB");
                shape.text_body_mut().unwrap().paragraphs[0].runs = vec![
                    Run {
                        text: "A".into(),
                        style: RunStyle {
                            font_size: Some(1000),
                            ..Default::default()
                        },
                    },
                    Run {
                        text: "B".into(),
                        style: RunStyle {
                            font_size: Some(2000),
                            ..Default::default()
                        },
                    },
                ];
                let index = e.slide().shapes.len();
                e.run(
                    vec![Command::InsertShape {
                        slide: e.current_slide,
                        index,
                        shape,
                    }],
                    cx,
                );
                e.set_selection(vec![id]);
            });
        }
        let size_label = |s: &mut Scenario| {
            s.with(|e, _| {
                e.toolbar_items()
                    .into_iter()
                    .find_map(|t| {
                        if let chrome::ToolItem::FontSize { value, .. } = t {
                            value.map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap()
            })
        };
        assert!(
            size_label(&mut s).contains('+'),
            "mixed {:?}: {}",
            kind,
            size_label(&mut s)
        );
        for (value, key) in [("24", "enter"), ("30", "tab")] {
            let b = buttons::bounds("font-size-edit").unwrap();
            s.click((f32::from(b.center().x), f32::from(b.center().y)));
            s.type_text(value);
            assert_eq!(
                s.with(|e, cx| e.font_size_edit_input.read(cx).value().to_string()),
                value,
                "click selects all"
            );
            s.key(key);
            assert_eq!(size_label(&mut s), value);
        }
        s.screenshot(&format!("font-size-{kind:?}"));
    }
}
fn chart() -> Scenario {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    for (cell, value) in [
        ("A1", "x"),
        ("B1", "y"),
        ("A2", "1"),
        ("B2", "2"),
        ("A3", "2"),
        ("B3", "4"),
    ] {
        s.click_cell(cell);
        s.enter(value, "enter");
    }
    s.update(|e, w, cx| {
        e.go_to_reference("A1:B3", w, cx);
    });
    s.menu("pptx.insert-chart.line");
    s.update(|e, w, cx| e.open_sheet_chart_panel(w, cx));
    s
}
fn charts() {
    let _probe = buttons::Probe::new();
    let mut s = chart();
    let tab = format!("{:?}", ElementId::from(("chart-format-tab", 2usize)));
    buttons::click(&mut s, &tab);
    assert_eq!(s.with(|e, _| e.wb().chart_edit.as_ref().unwrap().tab), 2);
    s.update(|e, w, cx| {
        e.wb_mut().chart_edit = None;
        e.open_sheet_chart_panel(w, cx);
    });
    assert_eq!(s.with(|e, _| e.wb().chart_edit.as_ref().unwrap().tab), 2);
    buttons::click(&mut s, "chart-palette-dropdown");
    s.screenshot("chart-palette");
    for row in 0..10 {
        let id = format!(
            "{:?}",
            ElementId::from((
                SharedString::from("searchable-item-chart-palette-dropdown"),
                row
            ))
        );
        assert!(
            buttons::bounds(&id).is_some(),
            "all palette rows have full-row hit targets: {id}"
        );
        if row == 2 || row == 9 {
            let b = buttons::bounds(&id).unwrap();
            s.hover((f32::from(b.center().x), f32::from(b.center().y)));
            s.wait(1000);
            s.screenshot(&format!("palette-tooltip-{row}"));
        }
    }
    s.key("escape");
    s.update(|e, w, cx| e.open_move_chart_panel(w, cx));
    s.screenshot("move-chart");
    s.key("escape");
    let items = s.with(|e, _| e.menus().find_entry("xlsx.chart-palettes"));
    let chrome::MenuEntry::Submenu(menu) = items.as_ref().unwrap() else {
        panic!()
    };
    assert!(menu.items.iter().all(|i|matches!(i,chrome::MenuEntry::Item(m)if matches!(m.dropdown_content,Some(chrome::dropdown::ItemContent::Palette(_))))));
    s.update(|e, w, cx| e.force_close_file(w, cx));
    assert!(s.with(|e, _| {
        e.document_memory
            .get::<usize>("xlsx.chart-format.tab")
            .is_none()
    }));
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    assert!(s.with(|e, _| {
        e.document_memory
            .get::<usize>("xlsx.chart-format.tab")
            .is_none()
    }));
}
fn paste() {
    let _reset = Reset;
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    s.click_cell("D5");
    let bytes = s.with(|e, _| {
        let mut p = e.presentation().clone();
        let a = Shape::text_box(
            p.new_shape_id(),
            EmuRect::new(100000, 200000, 200000, 300000),
            "A",
        );
        let b = Shape::text_box(
            p.new_shape_id(),
            EmuRect::new(500000, 600000, 200000, 300000),
            "B",
        );
        ibis_pptx::shape_clipboard::write_gvml(&p, &[a, b]).unwrap()
    });
    crate::native_shape_clipboard::simulate(Some(bytes));
    let origin = s.with(|e, _| e.sheet_insertion_origin());
    for n in 0..3 {
        assert!(s.update(|e, _, cx| e.paste_office_shapes(cx)));
        let bounds = s.with(|e, _| {
            ibis_pptx::shape_clipboard::drawing_bounds(
                &e.selected_shapes().into_iter().cloned().collect::<Vec<_>>(),
            )
            .unwrap()
        });
        let offset = n * crate::pptx::editing::PASTE_OFFSET;
        assert_eq!((bounds.x, bounds.y), (origin.0 + offset, origin.1 + offset));
    }
    s.click_cell("G8");
    let origin = s.with(|e, _| e.sheet_insertion_origin());
    s.update(|e, _, cx| {
        e.paste_office_shapes(cx);
    });
    assert_eq!(
        s.with(|e, _| e.selected_shapes().iter().map(|s| s.frame.x).min().unwrap()),
        origin.0
    );
    s.screenshot("paste-placement");
}
fn memory_settings() {
    let a = crate::shell::document_memory::DocumentMemoryStore::default();
    let b = crate::shell::document_memory::DocumentMemoryStore::default();
    a.set("tab", 2usize);
    assert_eq!(a.get::<usize>("tab"), Some(2));
    assert_eq!(b.get::<usize>("tab"), None);
    let clone = a.clone();
    clone.set("tab", 1usize);
    assert_eq!(a.get::<usize>("tab"), Some(1));
}
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::ui20261028_submit", forms),
        ("shell::ui20261028_focus_ime", focus),
        ("shell::ui20261028_font_sizes", font_sizes),
        ("shell::ui20261028_chart_memory_palette", charts),
        ("shell::ui20261028_paste_placement", paste),
        ("shell::ui20261028_memory", memory_settings),
        ("shell::ui20261028_tabs", tabs),
        ("shell::ui20261028_colors", colors),
        ("shell::ui20261028_additive_selection", additive_selection),
        ("shell::ui20261028_docx_selection", docx_selection),
        ("shell::ui20261028_raster_paste", raster_paste),
        ("shell::ui20261028_field_colors", field_colors),
    ]
}
fn tabs_fixture(
    e: &mut SlideEditor,
    _: &mut Window,
    cx: &mut Context<SlideEditor>,
) -> Option<chrome::SidebarPanel> {
    let tab = e.document_memory.get::<usize>("ui.tab").unwrap_or(0);
    Some(chrome::SidebarPanel {
        title: "Tabs".into(),
        body: chrome::tab_control_with_mode(
            ["First".into(), "Longer second tab".into(), "Third".into()],
            tab,
            "ui-tabs",
            chrome::TabMode::Intrinsic,
            |e, i, cx| {
                e.document_memory.set("ui.tab", i);
                cx.notify();
            },
            cx,
        )
        .into_any_element(),
    })
}
fn tabs() {
    let _reset = Reset;
    let _probe = buttons::Probe::new();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| {
        e.new_file(ibis_pptx::Kind::Document, w, cx);
        e.sidebar_open = true;
    });
    chrome::FORCED_SIDEBAR
        .with(|x| x.set(Some((tabs_fixture, "ui-tabs-dialog", PanelMode::Modal))));
    s.update(|_, _, cx| cx.notify());
    let before = buttons::bounds("tab-indicator-ui-tabs").unwrap();
    let id = format!("{:?}", ElementId::from(("ui-tabs", 1usize)));
    buttons::click(&mut s, &id);
    s.wait(250);
    let after = buttons::bounds("tab-indicator-ui-tabs").unwrap();
    assert!(after.origin.x > before.origin.x);
    assert!(after.size.width > before.size.width);
    let tab = buttons::bounds(&id).unwrap();
    s.hover((f32::from(tab.center().x), f32::from(tab.center().y)));
    s.screenshot("tabs-hover-line");
}
fn colors() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Document, w, cx));
    s.type_text("Colors");
    s.size(1300., 850.);
    let id = buttons::all()
        .into_iter()
        .find(|id| id.starts_with("text-color-") && id.ends_with("-menu"))
        .expect("font color dropdown");
    buttons::click(&mut s, &id);
    s.screenshot("toolbar-color-palette");
    let first = buttons::bounds("palette-standard-0").unwrap();
    let tenth = buttons::bounds("palette-standard-9").unwrap();
    assert!(
        (first.origin.y - tenth.origin.y).abs() < px(0.5),
        "ten standard colors per row"
    );
    let eleventh = buttons::bounds("palette-standard-10");
    if let Some(next) = eleventh {
        assert!(next.origin.y > first.origin.y, "the eleventh wraps");
    }
    s.hover((f32::from(first.center().x), f32::from(first.center().y)));
    s.screenshot("color-palette-hover");
}
fn additive_selection() {
    let _probe = buttons::Probe::new();
    for kind in [ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Presentation] {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| {
            e.new_file(kind, w, cx);
            e.zoom = Some(1.);
            let first = e.slide().shapes.len();
            let mut commands = Vec::new();
            for (i, x) in [1000000, 3500000].into_iter().enumerate() {
                let id = e.document.ids().new_shape_id();
                commands.push(Command::InsertShape {
                    slide: e.current_slide,
                    index: first + i,
                    shape: Shape::text_box(
                        id,
                        EmuRect::new(x, 1000000, 1500000, 800000),
                        "Selection",
                    ),
                });
            }
            e.run(commands, cx);
            e.set_selection(Vec::new());
        });
        let indices = s.with(|e, _| (e.slide().shapes.len() - 2, e.slide().shapes.len() - 1));
        s.click_shape(indices.0);
        assert_eq!(s.with(|e, _| e.selected_ids().len()), 1);
        let at = s.shape_position(indices.1);
        s.dispatch_click_with(
            at,
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        assert_eq!(
            s.with(|e, _| e.selected_ids().len()),
            2,
            "Ctrl selection {kind:?}"
        );
        s.click_shape(indices.0);
        assert_eq!(s.with(|e, _| e.selected_ids().len()), 1);
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
            "Shift selection {kind:?}"
        );
        s.screenshot(&format!("selection-{kind:?}"));
    }
}
fn docx_selection() {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Document, w, cx));
    let (p, _) = s.paragraph(0);
    s.update(|e, _, cx| {
        let mut commands = Vec::new();
        for (i, x) in [1000000, 4000000].into_iter().enumerate() {
            let mut shape = Shape::text_box(
                e.document.ids().new_shape_id(),
                EmuRect::new(0, 0, 2000000, 1000000),
                "Selection",
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
            commands.push(ibis_doc_model::FlowCommand::InsertField {
                at: FlowPos::new(p, i),
                kind: ibis_doc_model::FlowRunKind::Shape(Box::new(shape)),
            });
        }
        e.run_flow(commands, cx);
    });
    let at = s.with(|e, _| {
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
    });
    s.click(at[0]);
    assert_eq!(s.with(|e, _| e.all_selected_shapes().len()), 1);
    s.dispatch_click_with(
        at[1],
        Modifiers {
            control: true,
            ..Default::default()
        },
    );
    assert_eq!(s.with(|e, _| e.all_selected_shapes().len()), 2);
    s.click(at[0]);
    assert_eq!(s.with(|e, _| e.all_selected_shapes().len()), 1);
    s.dispatch_click_with(
        at[1],
        Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(s.with(|e, _| e.all_selected_shapes().len()), 2);
    s.screenshot("docx-selection");
}
fn raster_paste() {
    let _reset = Reset;
    crate::native_shape_clipboard::simulate(None);
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    s.click_cell("C4");
    let origin = s.with(|e, _| e.sheet_insertion_origin());
    let (data, _, _) = ibis_pptx::screenshot_snapshot(120, 80);
    s.update(|_, _, cx| {
        cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
            ImageFormat::Png,
            data,
        )))
    });
    for i in 0..3 {
        s.key("cmd-v");
        let frame = s.with(|e, _| e.selected_shape().unwrap().frame);
        let offset = crate::pptx::editing::PASTE_OFFSET * i;
        assert_eq!((frame.x, frame.y), (origin.0 + offset, origin.1 + offset));
    }
    s.screenshot("raster-paste");
}
fn field_colors_fixture(
    e: &mut SlideEditor,
    w: &mut Window,
    cx: &mut Context<SlideEditor>,
) -> Option<chrome::SidebarPanel> {
    let input = e.ensure_toolbar_search_input("ui-color-input", w, cx);
    if e.document_memory
        .get::<bool>("ui.color.initialized")
        .is_none()
    {
        input.update(cx, |i, cx| i.set_value("336699", w, cx));
        e.document_memory.set("ui.color.initialized", true);
    }
    let current = u32::from_str_radix(input.read(cx).value().as_ref(), 16)
        .ok()
        .map(ibis_doc_model::Rgb);
    let pick_input = input.clone();
    let pick: crate::shell::color_palette::PickColor = std::rc::Rc::new(move |_, color, w, cx| {
        pick_input.update(cx, |i, cx| {
            i.set_value(
                color.map_or(String::new(), |c| format!("{:06X}", c.0)),
                w,
                cx,
            )
        })
    });
    let colors = crate::shell::color_editor::palette_colors()
        .into_iter()
        .map(|c| (format!("#{:06X}", c.0).into(), c, "color.swatch"))
        .collect::<Vec<_>>();
    let entries = crate::shell::color_palette::simple_palette(None, None, &colors, current, pick);
    Some(chrome::SidebarPanel {
        title: "Color field".into(),
        body: chrome::color_text_field(
            "ui-color-field",
            &input,
            current,
            entries,
            &cx.weak_entity(),
        )
        .allow_none(true)
        .into_any_element(),
    })
}
fn field_colors() {
    let _probe = buttons::Probe::new();
    let _reset = Reset;
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| {
        e.new_file(ibis_pptx::Kind::Document, w, cx);
        e.sidebar_open = true;
    });
    chrome::FORCED_SIDEBAR.with(|x| {
        x.set(Some((
            field_colors_fixture,
            "ui-color-dialog",
            PanelMode::Modal,
        )))
    });
    s.update(|_, _, cx| cx.notify());
    buttons::click(&mut s, "ui-color-field-swatch");
    let first = buttons::bounds("palette-standard-0").unwrap();
    let tenth = buttons::bounds("palette-standard-9").unwrap();
    let next = buttons::bounds("palette-standard-10").unwrap();
    assert_eq!(first.origin.y, tenth.origin.y);
    assert!(next.origin.y > first.origin.y);
    assert!(buttons::bounds("palette-theme-0-9").is_some());
    s.screenshot("color-field-palette");
    buttons::click(&mut s, "menu-item-_shell.no-fill");
    s.screenshot("after-no-fill");
    assert_eq!(
        s.with(|e, cx| e.toolbar_search_inputs["ui-color-input"]
            .read(cx)
            .value()
            .to_string()),
        ""
    );
}
