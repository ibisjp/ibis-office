// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-12: the format pane's effects (shadow, glow, soft edge, reflection) and the text's options (fill, outline, shadow, glow)
//! reach the shape and its text, and stay after saving and opening again.
use crate::shell::scenario::Scenario;
use gpui_kit::component::input::InputEvent;
use crate::ext::*;
use ibis_doc_model::{AutoShape, Color, EmuRect, Fill, Geometry, Rgb, Shape, ShapeId, ShapeKind, TextBody};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("pptx::format_pane_effects_20261112_shape_and_text_effects_persist", effects_persist)]
}

fn fixture() -> (Scenario, ShapeId) {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1400., 900.);
    let id = s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let mut auto = AutoShape::new(Geometry::Rect, Fill::Solid(Color::opaque(Rgb(0xCCCCCC))), None);
        auto.text = Some(TextBody::plain("Sample", Default::default()));
        e.insert_shape(Shape::new(id, EmuRect::new(1_500_000, 1_500_000, 4_000_000, 2_000_000), ShapeKind::Auto(auto)), cx);
        e.sidebar_open = true;
        cx.notify();
        id
    });
    (s, id)
}

/// Turn a section on and put `value` in its field, as typing does.
fn type_in(s: &mut Scenario, section: usize, key: &'static str, value: &str) {
    s.update(|e, _, _| {
        e.pptx_mut().unwrap().sidebar_settings.as_mut().unwrap().effects.as_mut().unwrap().enabled[section] = true;
    });
    let field = s.with(|e, _| e.pptx().unwrap().sidebar_settings.as_ref().unwrap().effects.as_ref().unwrap().editor.fields[key].clone());
    let value = value.to_string();
    s.update(|_, w, cx| field.update(cx, |f, cx| {
        f.set_value(value, w, cx);
        cx.emit(InputEvent::Change);
    }));
}

fn effects_persist() {
    let (mut s, id) = fixture();
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run(vec![ibis_doc_model::Command::SetShapeName { slide, shape: id, name: "Format pane sample".into() }], cx);
    });
    type_in(&mut s, 1, "shadow-blur", "6");
    type_in(&mut s, 2, "glow-size", "9");
    type_in(&mut s, 3, "soft-size", "4");
    type_in(&mut s, 5, "text-outline-color", "FF0000");
    type_in(&mut s, 7, "text-glow-size", "7");
    type_in(&mut s, 6, "text-shadow-distance", "5");
    // the text fill
    let fill = s.with(|e, _| e.pptx().unwrap().sidebar_settings.as_ref().unwrap().fields[7].clone());
    s.update(|_, w, cx| fill.update(cx, |f, cx| {
        f.set_value("336699", w, cx);
        cx.emit(InputEvent::Change);
    }));
    let check = |p: &ibis_doc_model::Presentation, what: &str| {
        let shape = p.slides.iter().find_map(|sl| sl.shape_names.iter().find(|(_, n)| n.as_str() == "Format pane sample").and_then(|(i, _)| sl.shape(*i))).expect("the shape");
        let ShapeKind::Auto(a) = &shape.kind else { panic!("auto") };
        assert_eq!(a.shadow.expect("shadow").blur, 6 * 12_700, "{what}");
        assert_eq!(a.glow.expect("glow").1, 9 * 12_700, "{what}");
        assert_eq!(a.soft_edge, Some(4 * 12_700), "{what}");
        let run = &a.text.as_ref().unwrap().paragraphs[0].runs[0].style;
        assert_eq!(run.outline.expect("outline").0.rgb, Rgb(0xFF0000), "{what}");
        assert_eq!(run.glow.unwrap_or_else(|| panic!("text glow {what}")).1, 7 * 12_700, "{what}");
        assert_eq!(run.shadow.expect("text shadow").distance, 5 * 12_700, "{what}");
        assert_eq!(run.color, Some(Rgb(0x336699)), "{what}");
    };
    let now = s.with(|e, _| e.presentation().clone());
    check(&now, "in the editor");
    s.save_as("format-pane-effects.pptx");
    let loaded = s.reopen();
    check(&loaded, "after saving and opening again");
    for tab in 0..3 {
        s.update(|e, _, cx| {
            e.pptx_mut().unwrap().sidebar_settings.as_mut().unwrap().effects.as_mut().unwrap().editor.tab = tab;
            for open in e.pptx_mut().unwrap().sidebar_settings.as_mut().unwrap().effects.as_mut().unwrap().editor.open.iter_mut() {
                *open = true;
            }
            cx.notify();
        });
        let _ = s.screenshot(&format!("format-pane-tab-{tab}"));
    }
}
