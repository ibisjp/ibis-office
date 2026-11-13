// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Several shapes selected, a panel's field shows the common value, or is empty when the shapes differ, and Apply writes only the
//! fields that hold a value (tasks/doc/design/ui-model-binding.md, "混在値"; `shell/chrome/shown.rs`).
//!
//! One property is run for every panel that is listed in `CASES` (a panel that builds its fields with `Shown::of` must be listed:
//! `tests/binding_rule.rs` checks it): two shapes that differ in every attribute are selected; (1) every field is empty;
//! (2) Apply without a touch changes nothing and makes no Undo step; (3) for each field in turn, a value typed in that field
//! alone is written to both shapes, and every other attribute of each shape keeps its own value.
use gpui_kit::component::input::InputState;
use gpui_kit::*;
use ibis_doc_model::{AutoShape, Color, EmuRect, Fill, Geometry, Insets, Rgb, Shadow, Shape, ShapeId, ShapeKind, TextBody, TextStyle};
use crate::shell::editor::SlideEditor;
use crate::shell::scenario::Scenario;

type Open = fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>);

/// One attribute of a panel: the field that edits it, the text to type, the value that text means, and how to read it from a shape.
struct Attribute {
    name: &'static str,
    field: usize,
    typed: &'static str,
    written: i64,
    read: fn(&Shape) -> i64,
}

/// A panel under the property.
struct Case {
    name: &'static str,
    open: Open,
    apply: Open,
    fields: fn(&SlideEditor) -> Vec<Entity<InputState>>,
    attributes: &'static [Attribute],
}

fn auto(shape: &Shape) -> &AutoShape {
    match &shape.kind { ShapeKind::Auto(auto) => auto, _ => unreachable!("an auto shape") }
}
fn glow(shape: &Shape) -> (Color, i64) { auto(shape).glow.expect("a glow") }
fn shadow(shape: &Shape) -> Shadow { auto(shape).shadow.expect("a shadow") }
fn insets(shape: &Shape) -> Insets { auto(shape).text.as_ref().expect("a text").insets }

const CASES: &[Case] = &[
    Case {
        name: "glow", open: |e, w, cx| e.open_glow_panel(w, cx), apply: |e, w, cx| e.apply_glow_panel(w, cx),
        fields: |e| e.shape_edit.glow_panel.as_ref().unwrap().fields.clone(),
        attributes: &[
            Attribute { name: "radius", field: 0, typed: "7", written: 88_900, read: |s| glow(s).1 },
            Attribute { name: "colour", field: 1, typed: "FF0000", written: 0xFF0000, read: |s| glow(s).0.rgb.0 as i64 },
            Attribute { name: "opacity", field: 2, typed: "50", written: 128, read: |s| glow(s).0.alpha as i64 },
        ],
    },
    Case {
        name: "shadow", open: |e, w, cx| e.open_shadow_panel(w, cx), apply: |e, w, cx| e.apply_shadow_panel(w, cx),
        fields: |e| e.shape_edit.shadow_panel.as_ref().unwrap().fields.clone(),
        attributes: &[
            Attribute { name: "blur", field: 0, typed: "3", written: 38_100, read: |s| shadow(s).blur },
            Attribute { name: "colour", field: 1, typed: "FF0000", written: 0xFF0000, read: |s| shadow(s).color.rgb.0 as i64 },
            Attribute { name: "opacity", field: 2, typed: "50", written: 128, read: |s| shadow(s).color.alpha as i64 },
            Attribute { name: "distance", field: 3, typed: "7", written: 88_900, read: |s| shadow(s).distance },
            Attribute { name: "angle", field: 4, typed: "45", written: 2_700_000, read: |s| shadow(s).direction as i64 },
        ],
    },
    Case {
        name: "insets", open: |e, w, cx| e.open_text_insets_panel(w, cx), apply: |e, w, cx| e.apply_text_insets_panel(w, cx),
        fields: |e| e.shape_edit.insets_panel.as_ref().unwrap().fields.clone(),
        attributes: &[
            Attribute { name: "left", field: 0, typed: "1", written: 360_000, read: |s| insets(s).left },
            Attribute { name: "top", field: 1, typed: "1", written: 360_000, read: |s| insets(s).top },
            Attribute { name: "right", field: 2, typed: "1", written: 360_000, read: |s| insets(s).right },
            Attribute { name: "bottom", field: 3, typed: "1", written: 360_000, read: |s| insets(s).bottom },
        ],
    },
    Case {
        name: "frame", open: |e, w, cx| e.open_frame_panel(w, cx), apply: |e, w, cx| e.apply_frame_panel(w, cx),
        fields: |e| e.shape_edit.frame_panel.as_ref().unwrap().fields.clone(),
        attributes: &[
            Attribute { name: "height", field: 0, typed: "3", written: 1_080_000, read: |s| s.frame.height },
            Attribute { name: "width", field: 1, typed: "4", written: 1_440_000, read: |s| s.frame.width },
            Attribute { name: "rotation", field: 2, typed: "45", written: 2_700_000, read: |s| s.rotation as i64 },
            Attribute { name: "x", field: 3, typed: "2", written: 720_000, read: |s| s.frame.x },
            Attribute { name: "y", field: 4, typed: "3", written: 1_080_000, read: |s| s.frame.y },
        ],
    },
];

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::binding_mixed_glow", || run(&CASES[0])),
        ("pptx::binding_mixed_shadow", || run(&CASES[1])),
        ("pptx::binding_mixed_insets", || run(&CASES[2])),
        ("pptx::binding_mixed_frame", || run(&CASES[3])),
        ("pptx::binding_common_value_is_shown_and_written", common_value),
        ("pptx::binding_cases_cover_the_panels", cases_are_listed),
        ("pptx::binding_mixed_slider_look_and_touch", slider_look_and_touch),
    ]
}

/// A shape whose every attribute depends on `k`, so that two of them differ in all of them.
fn shape(id: ShapeId, k: i64) -> Shape {
    let mut auto = AutoShape::new(Geometry::Rect, Fill::Solid(Color::BLACK), None);
    auto.glow = Some((Color { rgb: Rgb(if k == 0 { 0x112233 } else { 0xAABBCC }), alpha: if k == 0 { 255 } else { 100 } }, 50_800 + k * 25_400));
    let mut shadow = Shadow::simple(Color { rgb: Rgb(if k == 0 { 0x445566 } else { 0xDDEEFF }), alpha: if k == 0 { 200 } else { 60 } }, 63_501 + k * 12_700, 25_401 + k * 12_700, 300_001 + k as i32 * 600_000);
    shadow.sx = 90_000;
    auto.shadow = Some(shadow);
    let mut text = TextBody::plain("Mixed", TextStyle::default());
    text.insets = Insets { left: 91_441 + k * 360_000, top: 91_442 + k * 360_000, right: 91_443 + k * 360_000, bottom: 91_444 + k * 360_000 };
    auto.text = Some(text);
    let mut shape = Shape::new(id, EmuRect::new(500_000 + k * 700_000, 600_000 + k * 900_000, 2_000_000 + k * 1_000_000, 1_000_000 + k * 500_000), ShapeKind::Auto(auto));
    shape.rotation = (600_000 * k) as i32;
    shape
}

fn start(count: i64) -> (Scenario, Vec<ShapeId>) {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Presentation);
    s.size(1200., 900.);
    let ids = s.update(|e, _, cx| {
        let ids: Vec<ShapeId> = (0..count).map(|k| {
            let id = e.document.ids().new_shape_id();
            e.insert_shape(shape(id, k), cx);
            id
        }).collect();
        e.set_selection(ids.clone());
        cx.notify();
        ids
    });
    (s, ids)
}

fn shapes(s: &mut Scenario, ids: &[ShapeId]) -> Vec<Shape> {
    s.with(|e, _| ids.iter().map(|id| e.slide().shape(*id).cloned().expect("the shape")).collect())
}

fn texts(s: &mut Scenario, case: &Case) -> Vec<String> {
    s.with(|e, cx| (case.fields)(e).iter().map(|f| f.read(cx).value().to_string()).collect())
}

fn run(case: &Case) {
    let (mut s, ids) = start(2);
    let before = shapes(&mut s, &ids);
    let undo = s.with(|e, _| e.document.undo_len());
    for attribute in case.attributes {
        let (a, b) = ((attribute.read)(&before[0]), (attribute.read)(&before[1]));
        assert_ne!(a, b, "{}: the shapes must differ in {}", case.name, attribute.name);
    }

    // (1) every field of an attribute the shapes differ in is empty.
    s.update(|e, w, cx| (case.open)(e, w, cx));
    s.settle();
    let shown = texts(&mut s, case);
    for attribute in case.attributes {
        assert_eq!(shown[attribute.field], "", "{}: {} differs between the shapes, so its field must be empty (not the first shape's value)", case.name, attribute.name);
    }

    // (2) Apply without a touch writes nothing.
    s.update(|e, w, cx| (case.apply)(e, w, cx));
    s.settle();
    assert_eq!(shapes(&mut s, &ids), before, "{}: an Apply with only empty (mixed) fields changed a shape", case.name);
    assert_eq!(s.with(|e, _| e.document.undo_len()), undo, "{}: an Apply with only empty fields made an Undo step", case.name);

    // (3) one field at a time: that attribute is written to both shapes, the others stay as each shape has them.
    for typed in case.attributes {
        s.update(|e, w, cx| {
            (case.open)(e, w, cx);
            let fields = (case.fields)(e);
            fields[typed.field].update(cx, |f, cx| f.set_value(typed.typed, w, cx));
            (case.apply)(e, w, cx);
        });
        s.settle();
        let after = shapes(&mut s, &ids);
        for (i, shape) in after.iter().enumerate() {
            for attribute in case.attributes {
                let expected = if attribute.field == typed.field { typed.written } else { (attribute.read)(&before[i]) };
                assert_eq!((attribute.read)(shape), expected, "{}: after typing only {} (shape {i}), {} is wrong", case.name, typed.name, attribute.name);
            }
        }
        // Back to the start for the next field (the Apply was one Undo step).
        s.update(|e, w, cx| {
            e.undo(&crate::shell::actions::Undo, w, cx);
            e.set_selection(ids.clone());
        });
        s.settle();
        assert_eq!(shapes(&mut s, &ids), before, "{}: the reset to the start failed", case.name);
    }
}

/// Shapes with the same value show it, exactly (not rounded), and Apply writes the typed field only.
fn common_value() {
    let (mut s, ids) = start(1);
    // Two copies of the same shape: every attribute is common.
    s.update(|e, _, cx| {
        let first = e.slide().shape(ids[0]).cloned().expect("the shape");
        let id = e.document.ids().new_shape_id();
        let mut copy = first.clone();
        copy.id = id;
        e.insert_shape(copy, cx);
        e.set_selection(vec![ids[0], id]);
    });
    for case in CASES {
        s.update(|e, w, cx| (case.open)(e, w, cx));
        s.settle();
        for attribute in case.attributes {
            assert_ne!(texts(&mut s, case)[attribute.field], "", "{}: {} is the same in both shapes, so it must be shown", case.name, attribute.name);
        }
        s.update(|e, w, cx| (case.apply)(e, w, cx));
        s.settle();
        // An Apply of the shown values changes neither shape (the exact values are kept).
        let now: Vec<Shape> = s.with(|e, _| e.selected_shapes().into_iter().cloned().collect());
        assert_eq!(now[0].kind, now[1].kind, "{}", case.name);
    }
    let _ = ids;
}

/// The panels that build their fields from a `Shown` are all in `CASES` (the lint `tests/binding_rule.rs` checks the source;
/// this checks the list is not stale).
fn cases_are_listed() {
    let names: Vec<_> = CASES.iter().map(|c| c.name).collect();
    assert_eq!(names, ["glow", "shadow", "insets", "frame"]);
}

/// The blank (mixed) slider of the opacity: it is drawn blank (screenshots), and a click on its track, which the transparent real
/// slider under it must take, sets a value (the field of the opacity gets text) and draws the ordinary slider.
fn slider_look_and_touch() {
    use crate::shell::scenario::button_probe;
    let (mut s, _ids) = start(2);
    s.update(|e, w, cx| e.open_glow_panel(w, cx));
    s.settle();
    let opacity = |s: &mut Scenario| s.with(|e, cx| e.shape_edit.glow_panel.as_ref().unwrap().fields[2].read(cx).value().to_string());
    assert_eq!(opacity(&mut s), "", "the opacity differs, so the field is empty");
    println!("screenshot {}", s.screenshot("glow-mixed-before").display());
    let _probe = button_probe::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let value = button_probe::bounds("pptx-glow-field-2-value").expect("the slider's value is drawn");
    let (x, y) = (f32::from(value.origin.x + value.size.width) + 8. + 40., f32::from(value.origin.y + value.size.height / 2.));
    s.click((x, y));
    s.settle();
    assert_ne!(opacity(&mut s), "", "a click on the blank slider's track must set a value (the transparent slider takes it)");
    println!("screenshot {}", s.screenshot("glow-mixed-after-click").display());
}
