// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The production segmented control, measured and rasterized by the real GPUI scenario renderer.
use super::{Scenario, button_probe as buttons};
use crate::shell::chrome::segment_control::{Mode, SegmentControl, SegmentItem, tests};
use crate::shell::chrome::{FORCED_SIDEBAR, SidebarPanel};
use crate::shell::editor::SlideEditor;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

type Records = Rc<RefCell<BTreeMap<&'static str, Bounds<Pixels>>>>;
thread_local! { static RECORDS: RefCell<Option<Records>> = const { RefCell::new(None) }; }
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        FORCED_SIDEBAR.with(|slot| slot.set(None));
        RECORDS.with(|slot| slot.borrow_mut().take());
    }
}
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::segment_control_ui", run),
        (
            "shell::segment_intrinsic_layout",
            tests::intrinsic_hugs_content_with_standard_insets,
        ),
        (
            "shell::segment_proportional_layout",
            tests::fixed_track_uses_natural_content_ratio_and_leaves_no_remainder,
        ),
        (
            "shell::segment_clip_alignment",
            tests::clipping_alignment_depends_on_real_content_width,
        ),
        (
            "shell::segment_empty_layout",
            tests::tiny_or_empty_tracks_are_finite,
        ),
        (
            "shell::segment_narrow_icon",
            tests::icon_uses_the_entire_slot_for_centering_and_clipping,
        ),
    ]
}
fn recorded(key: &'static str, child: impl IntoElement, records: &Records) -> AnyElement {
    let records = records.clone();
    div()
        .relative()
        .self_start()
        .flex_none()
        .child(child)
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, _, _| {
                    records.borrow_mut().insert(key, bounds);
                },
            )
            .absolute()
            .inset_0(),
        )
        .into_any_element()
}
fn box_content(width: f32, color: u32) -> AnyElement {
    div()
        .w(px(width))
        .min_w(px(width))
        .h(px(14.))
        .bg(crate::shell::palette::rgb(color))
        .flex_none()
        .into_any_element()
}
fn panel(
    _: &mut SlideEditor,
    _: &mut Window,
    _: &mut Context<SlideEditor>,
) -> Option<SidebarPanel> {
    let records = RECORDS.with(|slot| slot.borrow().as_ref().unwrap().clone());
    let intrinsic = SegmentControl::new(Mode::Intrinsic)
        .child(SegmentItem::custom(
            "segment-i-a",
            || box_content(20., 0x008080),
            false,
        ))
        .child(SegmentItem::custom(
            "segment-i-b",
            || box_content(80., 0x008080),
            true,
        ));
    let proportional = SegmentControl::new(Mode::Proportional { width: px(200.) })
        .child(SegmentItem::custom(
            "segment-p-a",
            || box_content(20., 0x008080),
            false,
        ))
        .child(SegmentItem::custom(
            "segment-p-b",
            || box_content(80., 0x008080),
            true,
        ));
    let icon = SegmentControl::new(Mode::Proportional { width: px(30.) }).child(SegmentItem::icon(
        "segment-icon",
        || box_content(18., 0xff0000),
        true,
    ));
    let clip_records = records.clone();
    let custom = SegmentControl::new(Mode::Proportional { width: px(54.) }).child(
        SegmentItem::custom(
            "segment-custom",
            || {
                h_flex()
                    .w(px(120.))
                    .min_w(px(120.))
                    .h(px(14.))
                    .flex_none()
                    .child(box_content(12., 0xff0000))
                    .child(box_content(108., 0x0000ff))
                    .into_any_element()
            },
            true,
        )
        .on_bounds(move |bounds| {
            clip_records.borrow_mut().insert("custom-content", bounds);
        }),
    );
    let labels_fit = SegmentControl::new(Mode::Intrinsic)
        .child(SegmentItem::label(
            "segment-label-a",
            crate::shell::i18n::text("chart.ui.left-value-axis"),
            true,
        ))
        .child(SegmentItem::label("segment-label-b", "Right axis", false));
    let labels_clip =
        SegmentControl::new(Mode::Proportional { width: px(120.) }).child(SegmentItem::label(
            "segment-long-label",
            format!(
                "{} / A deliberately long label",
                crate::shell::i18n::text("chart.ui.left-value-axis")
            ),
            true,
        ));
    Some(SidebarPanel {
        title: "Segment layout verification".into(),
        body: v_flex()
            .gap_2()
            .p(px(12.))
            .child(recorded("row-inline",crate::shell::chrome::SegmentControlRow::new(Mode::Intrinsic)
                .label("Axis").child(SegmentItem::label("row-inline-left","L",true))
                .child(SegmentItem::label("row-inline-right","R",false)),&records))
            .child(recorded("row-wrapped",crate::shell::chrome::SegmentControlRow::new(Mode::Intrinsic)
                .label("Display units").children(["Hundreds","Thousands","Millions"].into_iter().enumerate().map(|(i,label)|SegmentItem::label(("row-wrap",i),label,false))),&records))
            .child(SegmentControl::new(Mode::Intrinsic).available_width(px(160.)).children(["wrap-i-a","wrap-i-b"].into_iter().map(|id|SegmentItem::custom(id,||box_content(80.,0x008080),false))))
            .child(SegmentControl::new(Mode::Proportional{width:px(160.)}).children(["wrap-p-a","wrap-p-b"].into_iter().map(|id|SegmentItem::custom(id,||box_content(80.,0x008080),false))))
            .child("Intrinsic: natural content")
            .child(recorded("intrinsic-track", intrinsic, &records))
            .child("Proportional: 20 : 80, track 200")
            .child(recorded("proportional-track", proportional, &records))
            .child("Icon 18 in a 24-wide button")
            .child(recorded("icon-track", icon, &records))
            .child("Custom 120: red leading 12 remains visible")
            .child(recorded("custom-track", custom, &records))
            .child("Labels fit: centered")
            .child(labels_fit)
            .child("Long label: left aligned, clipped")
            .child(labels_clip)
            .into_any_element(),
    })
}
fn run() {
    let _reset = Reset;
    let _probe = buttons::Probe::new();
    let records: Records = Rc::new(RefCell::new(BTreeMap::new()));
    RECORDS.with(|slot| *slot.borrow_mut() = Some(records.clone()));
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1000., 900.);
    FORCED_SIDEBAR.with(|slot| {
        slot.set(Some((
            panel,
            "segment-control-verification",
            crate::shell::panel_dialog::PanelMode::Sidebar,
        )))
    });
    s.update(|e, _, cx| {
        e.sidebar_open = true;
        cx.notify();
    });
    let logical = s.update(|_, window, _| window.bounds().size);
    let image = image::open(s.screenshot("segment-control-production-layout"))
        .unwrap()
        .to_rgba8();
    let sx = image.width() as f32 / f32::from(logical.width);
    let sy = image.height() as f32 / f32::from(logical.height);
    // A pixel of rounding tolerance is derived from the actual screenshot scale.
    let epsilon = 1. / sx;
    let get = |id: &str| buttons::bounds(id).unwrap_or_else(|| panic!("missing button {id}"));
    let rec = records.borrow();
    let inline=rec["row-inline"];
    let wrapped=rec["row-wrapped"];
    assert!(get("row-inline-left").origin.y < inline.origin.y+px(10.),"short controls and label share a line");
    let wrap_key=buttons::all().into_iter().find(|k|k.contains("row-wrap")&&k.ends_with(", 0)")).unwrap();
    assert!(get(&wrap_key).origin.y > wrapped.origin.y+px(20.),"long control wraps as one intact row");
    for (first,second)in[("wrap-i-a","wrap-i-b"),("wrap-p-a","wrap-p-b")]{let a=get(first);let b=get(second);assert_eq!(a.size.width,px(96.));assert_eq!(b.size.width,px(96.));assert!(b.origin.y>=a.bottom(),"whole buttons wrap while keeping natural widths");}
    let intrinsic = rec["intrinsic-track"];
    let ia = get("segment-i-a");
    let ib = get("segment-i-b");
    assert!((f32::from(intrinsic.size.width) - 140.).abs() <= epsilon);
    assert!((f32::from(ia.origin.x - intrinsic.origin.x) - 3.).abs() <= epsilon);
    assert!(
        (f32::from(intrinsic.origin.x + intrinsic.size.width - ib.origin.x - ib.size.width) - 3.)
            .abs()
            <= epsilon,
        "no unused trailing track"
    );
    let prop = rec["proportional-track"];
    let pa = get("segment-p-a");
    let pb = get("segment-p-b");
    assert!((f32::from(prop.size.width) - 200.).abs() <= epsilon);
    assert!((f32::from(pa.size.width) - 38.4).abs() <= epsilon);
    assert!((f32::from(pb.size.width) - 153.6).abs() <= epsilon);
    let icon = get("segment-icon");
    assert!((f32::from(icon.size.width) - 24.).abs() <= epsilon);
    let span = |bounds: Bounds<Pixels>, red: bool| -> (u32, u32) {
        let left = (f32::from(bounds.origin.x) * sx).ceil() as u32;
        let right = (f32::from(bounds.origin.x + bounds.size.width) * sx).floor() as u32;
        let y = (f32::from(bounds.origin.y + bounds.size.height / 2.) * sy).round() as u32;
        let pixels: Vec<_> = (left..right)
            .filter(|&x| {
                let p = image.get_pixel(x, y).0;
                if red {
                    p[0] == 255 && p[1] == 0 && p[2] == 0
                } else {
                    p[0] == 0 && p[1] == 0 && p[2] == 255
                }
            })
            .collect();
        (
            *pixels.first().expect("colored content visible"),
            *pixels.last().unwrap() + 1,
        )
    };
    let (red_start, red_end) = span(icon, true);
    assert!(
        ((red_end - red_start) as f32 / sx - 18.).abs() <= epsilon,
        "the full 18px icon is visible"
    );
    let center = (red_start + red_end) as f32 / (2. * sx) - f32::from(icon.origin.x);
    assert!(
        (center - 12.).abs() <= epsilon,
        "icon is centered in the full 24px slot"
    );
    let custom = get("segment-custom");
    let content = rec["custom-content"];
    let (r0, r1) = span(custom, true);
    let (b0, b1) = span(custom, false);
    assert!(
        ((r1 - r0) as f32 / sx - 12.).abs() <= epsilon,
        "custom content keeps its leading red 12px"
    );
    assert!(
        (r0 as f32 / sx - f32::from(content.origin.x)).abs() <= epsilon,
        "overflow starts at the left, not the center"
    );
    assert!((b0 as f32 / sx - f32::from(content.origin.x) - 12.).abs() <= epsilon);
    assert!(
        (b1 as f32 / sx - f32::from(content.origin.x + content.size.width)).abs() <= epsilon,
        "the blue remainder is clipped at the slot edge"
    );
}
