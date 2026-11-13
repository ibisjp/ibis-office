// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-09 (#133): the CMYK mode of the presentation screen. The first use in a file asks (PowerPoint has no CMYK); OK turns
//! the mode on for the file, Cancel leaves it. In a file with the mode on the colour picker shows and saves the colours as the
//! press prints them (through CMYK and back; the user sets no ink); the file saves the flag and the profile's name only; the
//! PDF writes the colours as DeviceCMYK.
use crate::shell::color_editor::ColorAction;
use crate::shell::scenario::{button_probe, Scenario};
use ibis_doc_model::Rgb;
use gpui_kit::px;
use std::{cell::Cell, rc::Rc};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::cmyk_20261109_consent_cancel_leaves_the_file_as_it_was", consent_cancel),
        ("pptx::cmyk_20261109_consent_ok_turns_the_mode_on_and_the_picker_shows_the_printed_colour", consent_ok_and_pick),
        ("pptx::cmyk_20261109_vivid_colours_come_back_inside_the_gamut_and_the_mode_round_trips", gamut_and_round_trip),
        ("pptx::cmyk_20261109_pdf_has_the_colours_as_devicecmyk", pdf_device_cmyk),
        ("pptx::cmyk_20261109_profile_bar_on_the_three_screens", profile_bar_on_three_screens),
        ("pptx::cmyk_20261109_profiles_found_in_the_os_places_and_chosen_by_name_only", profiles_found_and_saved_by_name),
        ("pptx::cmyk_20261109_picker_rows_stay_inside_the_dialog_in_both_languages", picker_rows_fit),
        ("pptx::cmyk_20261109_gradient_bar_shows_the_printed_colours", gradient_bar_printed),
        ("pptx::cmyk_20261109_settings_has_the_cmyk_profile_category", settings_category),
    ]
}

fn press(s: &mut Scenario, id: &str) {
    let _probe = button_probe::Probe::new();
    s.update(|_, _, cx| cx.notify());
    button_probe::click(s, id);
    s.settle();
}

/// The picker opened on `color`, and a cell that holds what picking gives.
fn open_picker(s: &mut Scenario, color: u32) -> Rc<Cell<Option<Rgb>>> {
    let got = Rc::new(Cell::new(None));
    let sink = got.clone();
    let pick: crate::shell::color_palette::PickColor = Rc::new(move |_, c, _, _| sink.set(c));
    s.update(|e, w, cx| e.open_color_picker_dialog(Rgb(color), pick, w, cx));
    s.settle();
    got
}

fn open_plan() -> Scenario {
    Scenario::open("test/editing/pptx-plan.pptx")
}

fn turn_mode_on(s: &mut Scenario) {
    s.update(|e, _, cx| {
        e.require_cmyk(Rc::new(|_, _| {}), cx);
        e.answer_ask(true, cx);
    });
}

fn consent_cancel() {
    let mut s = open_plan();
    open_picker(&mut s, 0x00ff00);
    press(&mut s, "color-cmyk-mode");
    assert!(s.with(|e, _| e.ask.is_some()), "the first use of CMYK asks");
    s.screenshot("cmyk-consent");
    press(&mut s, "ask-cancel");
    assert!(s.with(|e, _| e.ask.is_none() && !e.presentation().cmyk.enabled), "Cancel: nothing is turned on");
    assert_eq!(s.with(|e, _| e.picker_state_summary()), Some((Rgb(0x00ff00), false)), "the picker shows the colour as it is");
}

fn consent_ok_and_pick() {
    let mut s = open_plan();
    let got = open_picker(&mut s, 0x00ff00);
    s.screenshot("cmyk-picker-off");
    press(&mut s, "color-cmyk-mode");
    press(&mut s, "ask-ok");
    s.update(|_, _, cx| cx.notify());
    s.settle();
    assert!(s.with(|e, _| e.presentation().cmyk.enabled), "OK turns the mode on for the file");
    let (shown, gamut) = s.with(|e, _| e.picker_state_summary()).expect("the picker is open");
    assert!(gamut, "the picker shows the colours as printed");
    let printed = crate::shell::cmyk::gamut(Rgb(0x00ff00));
    assert_eq!(shown, printed, "the vivid green is the colour the press prints");
    assert_ne!(shown, Rgb(0x00ff00));
    s.screenshot("cmyk-picker");
    // A vivid blue picked on the wheel comes back inside the gamut too.
    s.update(|e, w, cx| {
        e.change_color_picker(ColorAction::Hue(240.), w, cx);
        e.change_color_picker(ColorAction::Sv(1., 1.), w, cx);
    });
    let blue = s.with(|e, _| e.picker_state_summary()).unwrap().0;
    assert_eq!(blue, crate::shell::cmyk::gamut(Rgb(0x0000ff)));
    assert_ne!(blue, Rgb(0x0000ff));
    s.update(|e, w, cx| e.commit_color_picker(w, cx));
    assert_eq!(got.get(), Some(blue), "the colour saved is the one the press prints (what is seen)");
}

fn gamut_and_round_trip() {
    let mut s = open_plan();
    turn_mode_on(&mut s);
    // The palette's vivid swatches: what a toolbar palette saves is the printed colour.
    let saved = Rc::new(Cell::new(None));
    let sink = saved.clone();
    let pick: crate::shell::color_palette::PickColor = Rc::new(move |_, c, _, _| sink.set(c));
    let swatch = crate::shell::color_palette::Swatch::new(Rgb(0x00ff00), "x", false, &pick);
    s.update(|e, w, cx| (swatch.item.handler.clone().unwrap())(e, w, cx));
    let chosen = saved.get().expect("picked");
    assert_eq!(chosen, crate::shell::cmyk::gamut(Rgb(0x00ff00)));
    assert_ne!(chosen, Rgb(0x00ff00));
    // The file: the flag and the profile's name come back, the body has the printed colour as an ordinary colour.
    let (slide, shape) = s.with(|e, _| (e.slide().id, e.slide().shapes[0].id));
    s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetFill { slide, shape, fill: ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(chosen)) }], cx));
    let bytes = s.with(|e, _| e.current_bytes()).expect("the file as saved");
    let opened = ibis_pptx::open(bytes).expect("reopen");
    assert!(opened.presentation.cmyk.enabled);
    let kept = opened.presentation.slide(slide).and_then(|sl| sl.shape(shape)).map(|sh| matches!(&sh.kind, ibis_doc_model::ShapeKind::Auto(a) if matches!(&a.fill, ibis_doc_model::Fill::Solid(c) if c.rgb == chosen)));
    assert_eq!(kept, Some(true), "the body has the printed colour as an ordinary colour");
}

fn pdf_device_cmyk() {
    let mut s = open_plan();
    turn_mode_on(&mut s);
    let printed = crate::shell::cmyk::gamut(Rgb(0x00ff00));
    let (slide, shape) = s.with(|e, _| (e.slide().id, e.slide().shapes[0].id));
    s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetFill { slide, shape, fill: ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(printed)) }], cx));
    let presentation = s.with(|e, _| e.presentation().clone());
    let dir = std::path::Path::new("target/cmyk-scenario");
    std::fs::create_dir_all(dir).unwrap();
    let options = crate::export::Options {
        format: crate::export::Format::Pdf,
        out_dir: dir.into(),
        name: "cmyk".into(),
        width: 960,
        slides: vec![1],
        layout: false,
        file_name: None,
        print_backgrounds: true,
        print_hidden_text: false,
        print_markup: false,
        color: Default::default(),
        handout: None,
        frame: false,
        handout_landscape: false,
        notes_pages: false,
        outline_pages: false,
        notes_source: None,
        handout_marks: Default::default(),
    };
    crate::export::export(presentation.clone(), &options).expect("the PDF");
    let with_cmyk = device_cmyk_fills(&std::fs::read(dir.join("cmyk.pdf")).expect("read the PDF"));
    assert!(with_cmyk > 0, "fills were written as DeviceCMYK (`c m y k k`)");
    // The same file with the mode off has none.
    let mut off = presentation;
    off.cmyk.enabled = false;
    crate::export::export(off, &options).expect("the PDF");
    assert_eq!(device_cmyk_fills(&std::fs::read(dir.join("cmyk.pdf")).unwrap()), 0, "a file without the mode stays RGB");
}

/// How many `c m y k k` fills the PDF's content streams have.
fn device_cmyk_fills(pdf: &[u8]) -> usize {
    let mut found = 0;
    let mut at = 0;
    while let Some(i) = pdf[at..].windows(6).position(|w| w == b"stream") {
        let start = at + i + 6;
        let start = start + pdf[start..].iter().take_while(|b| **b == b'\r' || **b == b'\n').count();
        let Some(len) = pdf[start..].windows(9).position(|w| w == b"endstream") else { break };
        if let Ok(data) = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..start + len]) {
            let text = String::from_utf8_lossy(&data);
            let t: Vec<&str> = text.split_whitespace().collect();
            found += t.windows(5).filter(|w| w[4] == "k" && w[..4].iter().all(|n| n.parse::<f32>().is_ok())).count();
        }
        at = start + len + 9;
    }
    found
}

fn profile_bar_on_three_screens() {
    for (kind, name) in [(ibis_pptx::Kind::Presentation, "pptx"), (ibis_pptx::Kind::Document, "docx"), (ibis_pptx::Kind::Workbook, "xlsx")] {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| {
            e.new_file(kind, w, cx);
            e.cmyk_notice = Some(crate::shell::cmyk_profile::CmykNotice::Hint);
            cx.notify();
        });
        s.settle();
        let _probe = button_probe::Probe::new();
        s.update(|_, _, cx| cx.notify());
        for id in ["infobar-btn-cmyk-choose", "infobar-btn-cmyk-source", "infobar-btn-cmyk-later"] {
            assert!(button_probe::bounds(id).is_some(), "{name}: the bar has {id}");
        }
        s.screenshot(&format!("cmyk-bar-{name}"));
        drop(_probe);
        press(&mut s, "infobar-btn-cmyk-later");
        assert!(s.with(|e, _| e.cmyk_notice.is_none()), "{name}: Later closes the bar");
    }
}

fn profiles_found_and_saved_by_name() {
    use crate::shell::cmyk_profile::{inspect, scan};
    let dir = std::path::Path::new("target/cmyk-scenario/icc");
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    // A made-up CMYK profile header and tag table (no data tables): found by its header, named by its description.
    let mut b = vec![0u8; 128];
    b[16..20].copy_from_slice(b"CMYK");
    b[36..40].copy_from_slice(b"acsp");
    b[84..100].copy_from_slice(&[9; 16]);
    b.extend_from_slice(&1u32.to_be_bytes());
    b.extend_from_slice(b"desc");
    b.extend_from_slice(&144u32.to_be_bytes());
    let name = "Scenario Press";
    let mut tag = b"desc\0\0\0\0".to_vec();
    tag.extend_from_slice(&(name.len() as u32 + 1).to_be_bytes());
    tag.extend_from_slice(name.as_bytes());
    tag.push(0);
    b.extend_from_slice(&(tag.len() as u32).to_be_bytes());
    b.extend_from_slice(&tag);
    std::fs::write(dir.join("press.icc"), &b).unwrap();
    std::fs::write(dir.join("notes.txt"), b"x").unwrap();
    let found = scan(&[dir.to_path_buf()]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, name);
    assert_eq!(inspect(&b, "press.icc").unwrap().1, found[0].id);
    // The file keeps the name and the identifier only (the profile is not in it).
    let label = crate::shell::cmyk_profile::profile_label(&found[0].name, &found[0].id);
    assert!(label.starts_with(name) && !label.contains('\0') && label.len() < 100);
    let _ = std::fs::remove_dir_all(dir);
}

/// (a) The colour picker's CMYK controls and its last row (HEX) are inside the dialog's body, not scrolled out of it, in the
/// Japanese and the English UI, with the mode off and on.
fn picker_rows_fit() {
    use crate::shell::i18n::{show, Language};
    for lang in [Language::Japanese, Language::English] {
        for on in [false, true] {
            let mut s = open_plan();
            show(lang);
            if on {
                turn_mode_on(&mut s);
            }
            open_picker(&mut s, 0x336699);
            let _probe = button_probe::Probe::new();
            s.update(|_, _, cx| cx.notify());
            s.settle();
            let dialog = s.with(|e, _| e.dialog.bounds).expect("the dialog's frame");
            let footer = s.with(|e, _| e.dialog.footer_height);
            let limit = dialog.origin.y + dialog.size.height - footer;
            let mut ids = vec!["color-cmyk-mode"];
            if on {
                ids.push("color-cmyk-profile");
            }
            for id in ids {
                let b = button_probe::bounds(id).unwrap_or_else(|| panic!("{id} not rendered: {:?}", button_probe::all()));
                assert!(b.origin.y >= dialog.origin.y && b.origin.y + b.size.height <= limit, "{lang:?} on={on}: {id} {b:?} is out of the dialog {dialog:?} (body ends at {limit:?})");
                assert!(b.origin.x >= dialog.origin.x && b.origin.x + b.size.width <= dialog.origin.x + dialog.size.width, "{lang:?} on={on}: {id} is out sideways");
            }
            // Nothing of the form (the HEX row at its end too) is scrolled out of the body.
            let overflow = s.with(|e, _| e.dialog.scroll.max_offset().y);
            assert!(overflow <= px(0.5), "{lang:?} on={on}: the body scrolls by {overflow:?}");
            s.screenshot(&format!("cmyk-picker-fit-{lang:?}-{on}"));
        }
    }
    crate::shell::i18n::show(crate::shell::i18n::Language::Japanese);
}

/// (b) The gradient panel's bar and knobs show the stops as the press prints them in a file with the CMYK mode on.
fn gradient_bar_printed() {
    let count = |s: &mut Scenario, c: Rgb| {
        // Inside the dialog's frame only (the slide behind it has the shape's own green); the picture is in device pixels.
        let frame = s.with(|e, _| e.dialog.bounds).expect("the dialog");
        let (w, pixels) = s.picture();
        let w = w as usize;
        let scale = w as f32 / f32::from(s.update(|_, w, _| w.viewport_size().width));
        let (x0, x1) = ((f32::from(frame.origin.x) * scale) as usize, (f32::from(frame.origin.x + frame.size.width) * scale) as usize);
        let (y0, y1) = ((f32::from(frame.origin.y) * scale) as usize, (f32::from(frame.origin.y + frame.size.height) * scale) as usize);
        let mut n = 0;
        for (i, p) in pixels.chunks_exact(4).enumerate() {
            let (x, y) = (i % w, i / w);
            if x >= x0 && x < x1 && y >= y0 && y < y1 && p[..3] == [c.r(), c.g(), c.b()] {
                n += 1;
            }
        }
        n
    };
    let green = Rgb(0x00ff00);
    let printed = crate::shell::cmyk::gamut(green);
    assert_ne!(printed, green);
    let mut results = Vec::new();
    for on in [false, true] {
        let mut s = open_plan();
        if on {
            turn_mode_on(&mut s);
        }
        let (slide, shape) = s.with(|e, _| (e.slide().id, e.slide().shapes[0].id));
        s.update(|e, _, cx| {
            e.run(vec![ibis_doc_model::Command::SetFill { slide, shape, fill: ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(green)) }], cx);
            e.set_selection(vec![shape]);
            cx.notify();
        });
        s.update(|e, w, cx| e.open_gradient_panel(false, w, cx));
        s.settle();
        s.screenshot(&format!("cmyk-gradient-{on}"));
        results.push((count(&mut s, green), count(&mut s, printed)));
    }
    assert!(results[0].0 > 20 && results[0].1 == 0, "mode off: the bar is the stop's own colour {results:?}");
    assert!(results[1].1 > 20 && results[1].0 == 0, "mode on: the bar is the printed colour {results:?}");
}

/// (c) ファイル > 設定 has the CMYK profile category: where the OS was searched and what was found, the list with the simple
/// conversion (the default) chosen, the file button, the download page and 既定に戻す (disabled while the default is in use).
fn settings_category() {
    let dir = std::path::Path::new("target/cmyk-scenario/icc-settings");
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    let mut b = vec![0u8; 128];
    b[16..20].copy_from_slice(b"CMYK");
    b[36..40].copy_from_slice(b"acsp");
    b[84..100].copy_from_slice(&[5; 16]);
    b.extend_from_slice(&1u32.to_be_bytes());
    b.extend_from_slice(b"desc");
    b.extend_from_slice(&144u32.to_be_bytes());
    let mut tag = b"desc\0\0\0\0".to_vec();
    tag.extend_from_slice(&(b"Settings Press".len() as u32 + 1).to_be_bytes());
    tag.extend_from_slice(b"Settings Press\0");
    b.extend_from_slice(&(tag.len() as u32).to_be_bytes());
    b.extend_from_slice(&tag);
    std::fs::write(dir.join("p.icc"), &b).unwrap();
    // Scenarios run one at a time, on the main thread: nothing else reads the variable meanwhile.
    unsafe { std::env::set_var("IBIS_ICC_DIR", dir) };
    let mut s = open_plan();
    let category = crate::shell::settings_dialog::CATEGORY_CMYK;
    s.update(|e, w, cx| e.open_settings(category, w, cx));
    s.settle();
    let _probe = button_probe::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    for id in ["cmyk-profile-simple", "cmyk-profile-0", "cmyk-profile-file", "cmyk-profile-source", "cmyk-profile-reset"] {
        assert!(button_probe::bounds(id).is_some(), "the category has {id}: {:?}", button_probe::all());
    }
    let titles = s.with(|e, _| e.settings.as_ref().map(|c| c.cmyk_found.as_ref().map(|f| f.len())));
    assert_eq!(titles, Some(Some(1)), "the profile in the OS's place is found and listed");
    s.screenshot("cmyk-settings");
    drop(_probe);
    // The default is in use: nothing to reset; the simple conversion is the one in use.
    assert_eq!(s.with(|e, _| e.cmyk_profile_name()), ibis_doc_model::CmykConverter::name(&ibis_doc_model::OpenCmykConverter));
    // The English UI has the category's words too.
    crate::shell::i18n::show(crate::shell::i18n::Language::English);
    s.update(|_, _, cx| cx.notify());
    s.settle();
    s.screenshot("cmyk-settings-en");
    crate::shell::i18n::show(crate::shell::i18n::Language::Japanese);
    unsafe { std::env::remove_var("IBIS_ICC_DIR") };
    let _ = std::fs::remove_dir_all(dir);
}
