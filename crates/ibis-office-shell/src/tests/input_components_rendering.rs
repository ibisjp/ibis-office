// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The seven input components (text field, spinner, dropdown, radio group, checkbox, segment control, slider) in their states
//! (ordinary, mixed `Shown::Mixed`, disabled, focused, a dropdown open), in the light and the dark appearance, drawn through the
//! production parts in a forced sidebar and cropped to one picture per state. For the report
//! `tasks/doc/reports/2026-11-05-input-components-rendering.md`. The pictures go to `IBIS_INPUTS_DIR` when it is set (otherwise
//! nothing is written outside the scenario's own folder).
use super::{Scenario, button_probe as buttons};
use crate::chrome::segment_control::{Mode, SegmentControl, SegmentItem};
use crate::chrome::{
    self, Checkbox, Dropdown, DropdownItem, FORCED_SIDEBAR, Shown, ShownNumber, ShownText, SidebarPanel, SpinnerSpec,
    radio_button_shown, slider,
};
use crate::dialog::DialogMode;
use crate::editor::SlideEditor;
use gpui_kit::component::input::InputState;
use gpui_kit::component::slider::SliderState;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::input_components_rendering", run)]
}

type Records = Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>;
struct Inputs {
    records: Records,
    text: Entity<InputState>,
    text_mixed: Entity<InputState>,
    text_disabled: Entity<InputState>,
    spin: Entity<InputState>,
    spin_mixed: Entity<InputState>,
    spin_disabled: Entity<InputState>,
    slider: Entity<SliderState>,
    slider_mixed: Entity<SliderState>,
    slider_disabled: Entity<SliderState>,
}
thread_local! { static INPUTS: RefCell<Option<Rc<Inputs>>> = const { RefCell::new(None) }; }
struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        FORCED_SIDEBAR.with(|slot| slot.set(None));
        INPUTS.with(|slot| slot.borrow_mut().take());
    }
}

fn recorded(key: &str, child: impl IntoElement, records: &Records) -> AnyElement {
    let (records, key) = (records.clone(), key.to_string());
    div().relative().w_full().child(child)
        .child(canvas(|_, _, _| (), move |b, _, _, _| { records.borrow_mut().insert(key.clone(), b); }).absolute().inset_0())
        .into_any_element()
}
fn caption(text: &'static str) -> Div { div().mt_2().text_xs().child(text) }

fn items(current: usize) -> Vec<DropdownItem> {
    ["Left", "Center", "Right"].iter().enumerate().map(|(i, label)| DropdownItem::new(*label).checked(i == current).on_select(Rc::new(|_, _, _| {}))).collect()
}
fn segments(shown: &Shown<usize>, disabled: bool, id: &'static str) -> SegmentControl {
    SegmentControl::new(Mode::Intrinsic).shown(shown).children(
        ["Left", "Center", "Right"].iter().enumerate().map(|(i, label)| SegmentItem::label((id, i), *label, shown.selects(&i)).disabled(disabled)),
    )
}
fn radios(shown: &Shown<usize>, disabled: bool, id: &'static str) -> impl IntoElement {
    h_flex().gap_2().children(["Single", "Double", "Exact"].iter().enumerate().map(|(i, label)| {
        radio_button_shown((id, i), shown, &i, disabled, *label, |_, _, _| {})
    }))
}

fn panel(_: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
    let inputs = INPUTS.with(|slot| slot.borrow().clone().unwrap());
    let r = &inputs.records;
    let weak = cx.weak_entity();
    let (one, mixed) = (Shown::One(1usize), Shown::<usize>::Mixed);
    let spin = |input: &Entity<InputState>, id: &'static str, disabled: bool| {
        chrome::transient_text_field(input).no_clear().spinner(id, SpinnerSpec::new(1.0), cx, |_, _, _, _| {}).disabled(disabled)
    };
    let dd = |id: &'static str, shown: &Shown<usize>| {
        Dropdown::new(id, shown.label(|i| ["Left", "Center", "Right"][*i].into()), items(*shown.one().unwrap_or(&usize::MAX)), &weak).shown(shown)
    };
    let body = v_flex().gap_1().p(px(12.))
        .child(caption("1 text field"))
        .child(recorded("t-normal", chrome::transient_text_field(&inputs.text), r))
        .child(recorded("t-mixed", chrome::transient_text_field(&inputs.text_mixed), r))
        .child(recorded("t-disabled", chrome::transient_text_field(&inputs.text_disabled).disabled(true), r))
        .child(caption("2 spinner"))
        .child(recorded("s-normal", spin(&inputs.spin, "sp-n", false), r))
        .child(recorded("s-mixed", spin(&inputs.spin_mixed, "sp-m", false), r))
        .child(recorded("s-disabled", spin(&inputs.spin_disabled, "sp-d", true), r))
        .child(caption("3 dropdown"))
        .child(recorded("d-normal", h_flex().child(dd("dd-n", &one)), r))
        .child(recorded("d-mixed", h_flex().child(dd("dd-m", &mixed)), r))
        .child(recorded("d-disabled", h_flex().child(Dropdown::new("dd-d", "Center", items(1), &weak).disabled(true)), r))
        .child(caption("4 radio"))
        .child(recorded("r-normal", radios(&one, false, "rd-n"), r))
        .child(recorded("r-mixed", radios(&mixed, false, "rd-m"), r))
        .child(recorded("r-disabled", radios(&one, true, "rd-d"), r))
        .child(caption("5 checkbox"))
        .child(recorded("c-on", h_flex().child(Checkbox::new("cb-on").label("Shadow").shown(&Shown::One(true))), r))
        .child(recorded("c-off", h_flex().child(Checkbox::new("cb-off").label("Shadow").shown(&Shown::One(false))), r))
        .child(recorded("c-mixed", h_flex().child(Checkbox::new("cb-mixed").label("Shadow").shown(&Shown::Mixed)), r))
        .child(recorded("c-disabled-on", h_flex().child(Checkbox::new("cb-d-on").label("Shadow").shown(&Shown::One(true)).disabled(true)), r))
        .child(recorded("c-disabled-off", h_flex().child(Checkbox::new("cb-d-off").label("Shadow").shown(&Shown::One(false)).disabled(true)), r))
        .child(caption("6 segment control"))
        .child(recorded("g-normal", h_flex().child(segments(&one, false, "sg-n")), r))
        .child(recorded("g-mixed", h_flex().child(segments(&mixed, false, "sg-m")), r))
        .child(recorded("g-disabled", h_flex().child(segments(&one, true, "sg-d")), r))
        .child(caption("7 slider"))
        .child(recorded("l-normal", slider("sl-n", &inputs.slider).value(cx).unit("%").shown(&one), r))
        .child(recorded("l-mixed", slider("sl-m", &inputs.slider_mixed).value(cx).unit("%").shown(&mixed), r))
        .child(recorded("l-disabled", slider("sl-d", &inputs.slider_disabled).value(cx).unit("%").disabled(true).shown(&one), r));
    Some(SidebarPanel { title: "Inputs".into(), body: body.into_any_element() })
}

fn out_dir() -> std::path::PathBuf {
    let dir = std::env::var("IBIS_INPUTS_DIR").map(Into::into).unwrap_or_else(|_| std::env::temp_dir().join("ibis-inputs"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Crops `bounds` (logical pixels, grown by `pad`) out of the screenshot `shot` and saves it as `name`.
fn crop(shot: &std::path::Path, scale: f32, bounds: Bounds<Pixels>, pad: (f32, f32, f32, f32), name: &str) {
    let image = image::open(shot).unwrap().to_rgba8();
    let px = |v: f32| (v * scale).max(0.) as u32;
    let (x, y) = (px(f32::from(bounds.origin.x) - pad.0), px(f32::from(bounds.origin.y) - pad.1));
    let w = px(f32::from(bounds.size.width) + pad.0 + pad.2).min(image.width() - x);
    let h = px(f32::from(bounds.size.height) + pad.1 + pad.3).min(image.height() - y);
    image::imageops::crop_imm(&image, x, y, w, h).to_image().save(out_dir().join(format!("{name}.png"))).unwrap();
}

fn run() {
    let _probe = buttons::Probe::new();
    let records: Records = Rc::new(RefCell::new(BTreeMap::new()));
    let mut s = Scenario::open("test/editing/docx-report.docx");
    // Dropped before the scenario, so no entity is held when it is torn down.
    let _reset = Reset;
    s.size(1000., 1500.);
    s.update(|_, w, cx| {
        let sample = ShownText::new(Shown::One("Sample text".to_string()));
        let sp = |v: Shown<i64>| ShownNumber::new(v, 1., SpinnerSpec::new(1.0));
        let (text, text_mixed, text_disabled) = (sample.input(w, cx), ShownText::new(Shown::Mixed).input(w, cx), sample.input(w, cx));
        let (spin, spin_mixed, spin_disabled) = (sp(Shown::One(12)).input(w, cx), sp(Shown::Mixed).input(w, cx), sp(Shown::One(12)).input(w, cx));
        let make = |cx: &mut Context<SlideEditor>, v: f32| cx.new(|_| SliderState::new().min(0.).max(100.).step(1.).default_value(v));
        let inputs = Inputs {
            records: records.clone(), text, text_mixed, text_disabled, spin, spin_mixed, spin_disabled,
            slider: make(cx, 40.), slider_mixed: make(cx, 0.), slider_disabled: make(cx, 40.),
        };
        INPUTS.with(|slot| *slot.borrow_mut() = Some(Rc::new(inputs)));
        // Only the text field with the number is a "normal" one that takes the focus below.
    });
    FORCED_SIDEBAR.with(|slot| slot.set(Some((panel, "inputs", DialogMode::Sidebar))));
    s.update(|e, _, cx| { e.sidebar_open = true; cx.notify(); });
    let logical = s.update(|_, window, _| window.bounds().size);
    for dark in [false, true] {
        s.update(|_, _, cx| crate::palette::force(dark, cx));
        s.settle();
        let suffix = if dark { "-dark" } else { "" };
        let shot = s.screenshot(&format!("inputs{suffix}"));
        let scale = image::open(&shot).unwrap().width() as f32 / f32::from(logical.width);
        let rec = |key: &str| records.borrow().get(key).copied().unwrap_or_else(|| panic!("no bounds for {key}"));
        let pad = (6., 4., 6., 4.);
        let names: &[(&str, &str)] = &[
            ("t-normal", "01-textfield-normal"), ("t-mixed", "01-textfield-mixed"), ("t-disabled", "01-textfield-disabled"),
            ("s-normal", "02-spinner-normal"), ("s-mixed", "02-spinner-mixed"), ("s-disabled", "02-spinner-disabled"),
            ("d-normal", "03-dropdown-closed"), ("d-mixed", "03-dropdown-closed-mixed"), ("d-disabled", "03-dropdown-disabled"),
            ("r-normal", "04-radio-selected"), ("r-mixed", "04-radio-mixed"), ("r-disabled", "04-radio-disabled"),
            ("c-on", "05-checkbox-on"), ("c-off", "05-checkbox-off"), ("c-mixed", "05-checkbox-indeterminate"),
            ("c-disabled-on", "05-checkbox-disabled-on"), ("c-disabled-off", "05-checkbox-disabled-off"),
            ("g-normal", "06-segment-selected"), ("g-mixed", "06-segment-mixed"), ("g-disabled", "06-segment-disabled"),
            ("l-normal", "07-slider-normal"), ("l-mixed", "07-slider-mixed"), ("l-disabled", "07-slider-disabled"),
        ];
        for (key, name) in names { crop(&shot, scale, rec(key), pad, &format!("{name}{suffix}")); }
        // Focus: click the text field and the spinner field.
        for (key, name) in [("t-normal", "01-textfield-focused"), ("s-normal", "02-spinner-focused")] {
            let b = rec(key);
            s.click((f32::from(b.origin.x) + 40., f32::from(b.origin.y + b.size.height / 2.)));
            s.settle();
            let shot = s.screenshot(&format!("{name}{suffix}"));
            crop(&shot, scale, rec(key), pad, &format!("{name}{suffix}"));
        }
        // The slider's number: grey under the pointer, then a plain field (no ✕, no ▲▼) with the text selected; Esc gives it up.
        for (key, id) in [("l-normal", "sl-n"), ("l-mixed", "sl-m")] {
            let label = buttons::bounds(&format!("{id}-value")).unwrap_or_else(|| panic!("the slider number {id}-value is not drawn: {:?}", buttons::all()));
            let at = (f32::from(label.origin.x + label.size.width / 2.), f32::from(label.origin.y + label.size.height / 2.));
            s.hover(at);
            s.settle();
            let name = if key == "l-normal" { "07-slider-hover" } else { "07-slider-mixed-hover" };
            let shot = s.screenshot(&format!("{name}{suffix}"));
            crop(&shot, scale, rec(key), pad, &format!("{name}{suffix}"));
            s.click(at);
            s.settle();
            assert!(buttons::bounds(&format!("{id}-field")).is_some(), "a click on the number makes a field");
            let name = if key == "l-normal" { "07-slider-editing" } else { "07-slider-mixed-editing" };
            let shot = s.screenshot(&format!("{name}{suffix}"));
            crop(&shot, scale, rec(key), pad, &format!("{name}{suffix}"));
            s.type_text("77");
            s.key("escape");
            s.settle();
            // (The measured bounds of a field that is gone stay recorded: the picture shows that Esc closed it.)
            let name = if key == "l-normal" { "07-slider-after-esc" } else { "07-slider-mixed-after-esc" };
            let shot = s.screenshot(&format!("{name}{suffix}"));
            crop(&shot, scale, rec(key), pad, &format!("{name}{suffix}"));
        }
        let value_of = |s: &mut Scenario, state: fn(&Inputs) -> &Entity<SliderState>| s.with(|_, cx| INPUTS.with(|slot| state(slot.borrow().as_ref().unwrap()).read(cx).value().end()));
        assert_eq!(value_of(&mut s, |i| &i.slider), 40., "Esc leaves the value");
        assert_eq!(value_of(&mut s, |i| &i.slider_mixed), 0., "Esc leaves a mixed slider alone");
        // The disabled dropdown does not open on a click.
        let b = rec("d-disabled");
        s.click((f32::from(b.origin.x) + 40., f32::from(b.origin.y + b.size.height / 2.)));
        s.settle();
        assert!(s.with(|e, _| e.toolbar_search_open.is_none()), "a disabled dropdown does not open");
        // A dropdown open: ordinary and mixed.
        for (key, name) in [("d-normal", "03-dropdown-open"), ("d-mixed", "03-dropdown-open-mixed")] {
            let b = rec(key);
            s.click((f32::from(b.origin.x) + 40., f32::from(b.origin.y + b.size.height / 2.)));
            s.settle();
            // A mixed value selects nothing, so no row is highlighted when the list opens; an ordinary one highlights the checked row.
            let id = if key == "d-mixed" { "dd-m" } else { "dd-n" };
            let highlighted = s.with(|e, _| e.toolbar_search_selected.get(id).copied().flatten());
            if key == "d-mixed" { assert_eq!(highlighted, None, "a mixed dropdown opens with no row highlighted"); } else { assert_eq!(highlighted, Some(1)); }
            let shot = s.screenshot(&format!("{name}{suffix}"));
            let region = Bounds { origin: b.origin, size: size(b.size.width + px(20.), px(132.)) };
            crop(&shot, scale, region, (16., 4., 16., 4.), &format!("{name}{suffix}"));
            s.key("escape");
            s.settle();
        }
    }
    s.update(|_, _, cx| crate::palette::release(cx));
}
