// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0029: the 図形の書式 pane is one shared component (`shell::shape_format_pane`) on the document, sheet and slide screens.
//! K0027: the context menu of a shape, a picture or a text box has 図形の書式設定… first, and it opens that pane.
use super::Scenario;
use crate::docx::scenarios::DocSteps;
use crate::docx::ext::*;
use crate::shell::chrome::MenuEntry;
use crate::shell::shape_format_pane::{LAST_BUILT, SHAPE_FORMAT_ITEM};
use crate::xlsx::ext::*;
use crate::pptx::ext::*;
use ibis_doc_model::{Command, EmuRect, Fill, FlowPos, FlowRunKind, Shape, ShapeKind};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0029_shape_pane_is_shared_xlsx_pptx", sheet_and_slide_use_the_shared_pane),
        ("shell::k0029_shape_pane_is_shared_docx", docx_uses_the_shared_pane),
        ("shell::k0027_shape_menu_first_item_xlsx_pptx", sheet_and_slide_menu),
        ("shell::k0027_shape_menu_first_item_docx", docx_menu),
    ]
}

/// The first item of `entries`, which must be 図形の書式設定…; returns its handler.
fn first_is_shape_format(entries: &[MenuEntry], what: &str) -> crate::shell::chrome::Handler {
    let Some(MenuEntry::Item(item)) = entries.first() else { panic!("{what}: the first entry is an item") };
    assert_eq!(item.id, SHAPE_FORMAT_ITEM, "{what}: the first item is the shape format item");
    item.handler.clone().expect("a handler")
}

fn built() -> Option<(&'static str, usize, bool)> {
    LAST_BUILT.with(|l| l.borrow_mut().take())
}

fn new_file(kind: ibis_pptx::Kind) -> (Scenario, usize) {
    let mut s = Scenario::start_view();
    let index = s.update(|e, w, cx| {
        e.new_file(kind, w, cx);
        e.zoom = Some(1.);
        let index = e.slide().shapes.len();
        let mut shape = Shape::text_box(e.document.ids().new_shape_id(), EmuRect::new(2000000, 3000000, 2400000, 1200000), "Text");
        // A drawn shape (fill, outline and the decoration tabs), not a bare text box.
        if let ShapeKind::TextBox(body) = shape.kind.clone() {
            let mut auto = ibis_doc_model::AutoShape::text_box(Fill::None, body);
            auto.stroke = Some(ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0)), 12_700));
            shape.kind = ShapeKind::Auto(auto);
        }
        e.run(vec![Command::InsertShape { slide: e.current_slide, index, shape }], cx);
        e.set_selection(Vec::new());
        index
    });
    (s, index)
}

fn fill_of(s: &mut Scenario, index: usize) -> Fill {
    s.with(|e, _| match &e.slide().shapes[index].kind {
        ShapeKind::Auto(a) => a.fill.clone(),
        _ => Fill::None,
    })
}
fn stroke_of(s: &mut Scenario, index: usize) -> Option<ibis_doc_model::Stroke> {
    s.with(|e, _| match &e.slide().shapes[index].kind {
        ShapeKind::Auto(a) => a.stroke.clone(),
        _ => None,
    })
}
fn solid(hex: u32) -> Fill {
    Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(hex)))
}

/// The presentation's sidebar and the sheet's pane are the shared pane (built by `shape_format_panel`, with the three tabs), and a
/// fill and an outline typed in it are written to the shape.
fn sheet_and_slide_use_the_shared_pane() {
    for kind in [ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Presentation] {
        let (mut s, index) = new_file(kind);
        let id = s.with(|e, _| e.slide().shapes[index].id);
        s.update(|e, _, cx| {
            e.set_selection(vec![id]);
            e.sidebar_open = true;
            cx.notify();
        });
        if kind == ibis_pptx::Kind::Workbook {
            s.update(|e, w, cx| e.open_sheet_frame_panel(w, cx));
        }
        built();
        s.settle();
        let (prefix, _, tabs) = built().unwrap_or_else(|| panic!("{kind:?}: the shared pane is built"));
        assert_eq!(prefix, if kind == ibis_pptx::Kind::Workbook { "xlsx" } else { "pptx" });
        assert!(tabs, "{kind:?}: with the three tabs");
        assert_eq!(
            crate::shell::chrome::LAST_SIDEBAR_TITLE.with(|t| t.borrow().clone()),
            crate::shell::i18n::text("shape.effects.panel-title").to_string(),
            "{kind:?}: titled as the shape format pane"
        );

        // A fill and an outline colour typed in the pane's fields.
        if kind == ibis_pptx::Kind::Workbook {
            s.update(|e, w, cx| {
                let (fill, line) = (e.frame_panel_attr(0).unwrap(), e.frame_panel_attr(1).unwrap());
                fill.update(cx, |i, cx| i.set_value("FF0000", w, cx));
                line.update(cx, |i, cx| i.set_value("00B050", w, cx));
                e.apply_sheet_frame_panel(w, cx);
            });
        } else {
            s.update(|e, w, cx| {
                let (fill, line) = (e.sidebar_field(5).unwrap(), e.sidebar_field(6).unwrap());
                for (field, value) in [(fill, "FF0000"), (line, "00B050")] {
                    field.update(cx, |i, cx| {
                        i.set_value(value, w, cx);
                        cx.emit(gpui_kit::component::input::InputEvent::Change);
                    });
                }
            });
        }
        s.settle();
        assert_eq!(fill_of(&mut s, index), solid(0xFF0000), "{kind:?}: the fill typed in the pane is the shape's");
        assert_eq!(stroke_of(&mut s, index).map(|st| st.color.rgb.0), Some(0x00B050), "{kind:?}: and so is the outline's colour");

        // The line style chosen in the shared dropdown.
        if kind == ibis_pptx::Kind::Workbook {
            s.update(|e, w, cx| e.choose_frame_panel_dash(ibis_doc_model::LineDash::Dash, w, cx));
        } else {
            s.update(|e, w, cx| e.choose_sidebar_dash(ibis_doc_model::LineDash::Dash, w, cx));
        }
        s.settle();
        assert_eq!(stroke_of(&mut s, index).map(|st| st.dash), Some(ibis_doc_model::LineDash::Dash), "{kind:?}: the line style is written");
    }
}

fn docx_with_text_box() -> (Scenario, FlowPos, (f32, f32, f32, f32)) {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Document, w, cx));
    let (p, _) = s.paragraph(0);
    s.update(|e, _, cx| {
        let mut shape = Shape::text_box(e.document.ids().new_shape_id(), EmuRect::new(0, 0, 2000000, 1000000), "Text");
        if let ShapeKind::TextBox(body) = shape.kind.clone() {
            shape.kind = ShapeKind::Auto(ibis_doc_model::AutoShape::text_box(Fill::None, body));
        }
        shape.floating = Some(ibis_doc_model::FloatingAnchor {
            offset_x: 1000000,
            offset_y: 2000000,
            relative_h: "page".into(),
            relative_v: "page".into(),
            wrap: "none".into(),
            allow_overlap: true,
            ..Default::default()
        });
        e.run_flow(vec![ibis_doc_model::FlowCommand::InsertField { at: FlowPos::new(p, 0), kind: FlowRunKind::Shape(Box::new(shape)) }], cx);
    });
    let frame = s.with(|e, _| {
        let (page, (x, y, w, h), _) = e.floating_text_box_frame_of(FlowPos::new(p, 0)).unwrap();
        let (o, k) = e.page_origin(page).unwrap();
        (f32::from(o.x) + x * k, f32::from(o.y) + y * k, w * k, h * k)
    });
    (s, FlowPos::new(p, 0), frame)
}

/// The document screen's pane is the shared pane, and a fill typed in it is written to the text box.
fn docx_uses_the_shared_pane() {
    let (mut s, at, (x, y, w, h)) = docx_with_text_box();
    s.click((x + w / 2., y + h / 2.));
    s.key("escape");
    s.update(|e, w, cx| e.open_doc_shape_format(w, cx));
    built();
    s.settle();
    let (prefix, _, tabs) = built().expect("the shared pane is built");
    assert_eq!(prefix, "docx");
    assert!(tabs, "with the three tabs");
    assert_eq!(
        crate::shell::chrome::LAST_SIDEBAR_TITLE.with(|t| t.borrow().clone()),
        crate::shell::i18n::text("shape.effects.panel-title").to_string(),
        "titled as the shape format pane"
    );
    s.update(|e, w, cx| {
        let st = e.doc_state().unwrap().text_box.as_ref().unwrap();
        let (fill, line) = (st.fill.clone(), st.line.clone());
        fill.update(cx, |i, cx| i.set_value("FF0000", w, cx));
        line.update(cx, |i, cx| i.set_value("00B050", w, cx));
        e.apply_text_box(cx);
    });
    s.settle();
    let info = s.with(|e, _| e.text_box_near(at)).expect("the text box");
    assert_eq!((info.fill.as_str(), info.line.as_str()), ("FF0000", "00B050"), "the pane's fill and outline are the text box's");
}

/// A right click on the shape (inside it, and on the hand band of the selected one) on the sheet and the slide: the shape's menu has
/// 図形の書式設定… first, and choosing it opens the shared pane.
fn sheet_and_slide_menu() {
    for kind in [ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Presentation] {
        let (mut s, index) = new_file(kind);
        let id = s.with(|e, _| e.slide().shapes[index].id);
        for (label, offset) in [("inside", None), ("on the band", Some(-2.0_f32))] {
            s.click_shape(index);
            s.key("escape");
            s.update(|e, _, cx| {
                e.sidebar_open = false;
                cx.notify();
            });
            let (x, y) = s.with(|e, _| {
                let layout = e.slide_layout().expect("laid out");
                let f = e.slide().shapes[index].frame;
                match offset {
                    Some(o) => (f32::from(layout.origin.x + layout.to_px(f.x)) + o, f32::from(layout.origin.y + layout.to_px(f.y + f.height / 4))),
                    None => (f32::from(layout.origin.x + layout.to_px(f.x + f.width / 2)), f32::from(layout.origin.y + layout.to_px(f.y + f.height / 2))),
                }
            });
            s.right_click((x, y));
            assert!(s.with(|e, _| e.selected_ids().contains(&id)), "{kind:?} {label}: the shape stays selected");
            let entries = s.with(|e, _| if kind == ibis_pptx::Kind::Workbook { e.shape_menu() } else { e.shape_context_menu_entries() });
            let handler = first_is_shape_format(&entries, &format!("{kind:?} {label}"));
            s.update(|e, w, cx| handler(e, w, cx));
            s.settle();
            if kind == ibis_pptx::Kind::Workbook {
                assert!(s.with(|e, _| e.frame_panel_open() && e.sidebar_open), "{kind:?} {label}: the pane opens");
            } else {
                assert!(s.with(|e, _| e.sidebar_open), "{kind:?} {label}: the pane opens");
            }
            assert!(built().is_some_and(|(_, _, tabs)| tabs), "{kind:?} {label}: it is the shared pane");
            s.key("escape");
            s.update(|e, _, cx| {
                e.set_selection(Vec::new());
                cx.notify();
            });
        }
    }
}

/// The document screen: a right click on the text box (inside it, and on the hand band of the selected one) keeps it selected, the
/// menu has 図形の書式設定… first, and it opens the shared pane; a right click on plain text has no such item.
fn docx_menu() {
    let (mut s, at, (x, y, w, h)) = docx_with_text_box();
    // Plain text: no 図形の書式設定….
    let plain = s.with(|e, _| e.doc_popup_entries());
    assert!(!matches!(plain.first(), Some(MenuEntry::Item(i)) if i.id == SHAPE_FORMAT_ITEM), "no shape format item without a shape");
    for (label, point) in [("inside", (x + w / 2., y + h / 2.)), ("on the band", (x - 2.0, y + h / 4.))] {
        if label == "on the band" {
            s.click((x + w / 2., y + h / 2.));
            s.key("escape");
        } else {
            s.update(|e, _, cx| {
                e.set_caret(FlowPos::new(at.para, 0), false, cx);
                cx.notify();
            });
        }
        s.right_click(point);
        assert!(s.with(|e, _| e.doc_state().unwrap().shape_selected), "{label}: the text box is selected");
        let entries = s.with(|e, _| e.doc_popup_entries());
        let handler = first_is_shape_format(&entries, label);
        s.update(|e, w, cx| {
            e.sidebar_open = false;
            handler(e, w, cx);
        });
        s.settle();
        assert!(s.with(|e, _| e.sidebar_open && e.doc_state().unwrap().text_box.is_some()), "{label}: the pane opens");
        s.key("escape");
        s.update(|e, _, cx| {
            if let Some(st) = e.doc_state_mut() {
                st.text_box = None;
            }
            cx.notify();
        });
    }
}
