// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-14 (cmyk-redesign): the CMYK mode of a file. The colour window is RGB only; the mode is set in the print settings;
//! the screen shows the page as the press would print it (a soft proof); the PDF stays sRGB (the printer driver converts).
use crate::shell::scenario::{button_probe, Scenario};
use ibis_doc_model::Rgb;
use std::rc::Rc;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::cmyk_20261114_white_page_stays_white_on_screen_and_in_the_pdf", white_page),
        ("pptx::cmyk_20261114_colour_window_is_rgb_only_and_the_ring_has_no_dark_rim", picker_is_rgb_only),
        ("pptx::cmyk_20261114_print_settings_set_the_cmyk_mode_on_the_three_screens", print_settings_set_cmyk),
        ("pptx::cmyk_20261114_screen_is_a_soft_proof_of_the_print_colours", soft_proof),
        ("pptx::cmyk_20261114_colour_window_of_a_cmyk_file_is_a_soft_proof", picker_soft_proof),
        ("pptx::cmyk_20261114_real_profiles_keep_white_white", real_profiles),
        ("pptx::cmyk_20261114_view_menu_switches_the_soft_proof", view_menu_soft_proof),
        ("pptx::cmyk_20261114_print_panel_cancel_puts_the_cmyk_setting_back", cancel_puts_it_back),
        ("pptx::cmyk_20261114_mode_survives_saving_and_opening_on_the_three_screens", saved_and_reopened),
        ("pptx::cmyk_20261114_new_pages_stay_white_on_the_three_screens_with_the_mode_on", pages_white_on_three_screens),
    ]
}

fn turn_mode_on(s: &mut Scenario) {
    s.update(|e, _, cx| {
        e.require_cmyk(Rc::new(|_, _| {}), cx);
        e.answer_ask(true, cx);
    });
    s.settle();
}

/// The picture's pixel at fractions (x, y) of the window.
fn pixel(s: &mut Scenario, fx: f32, fy: f32) -> [u8; 3] {
    let (w, pixels) = s.picture();
    let w = w as usize;
    let h = pixels.len() / 4 / w;
    let (x, y) = ((fx * w as f32) as usize, (fy * h as f32) as usize);
    let p = &pixels[(y * w + x) * 4..];
    [p[0], p[1], p[2]]
}

/// A white page stays white with the mode on (with the open model and with an ICC profile), on screen and in the PDF.
fn white_page() {
    let profile = std::fs::read("/System/Library/ColorSync/Profiles/Generic CMYK Profile.icc").ok();
    for with_icc in [false, true] {
        if with_icc {
            let Some(bytes) = &profile else { continue };
            crate::shell::cmyk::set_converter(crate::shell::cmyk_profile::icc_converter(bytes, "generic").unwrap());
        }
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        turn_mode_on(&mut s);
        s.screenshot(&format!("white-page-icc-{with_icc}"));
        // Inside the slide, away from its text (the window's lower middle).
        let page = pixel(&mut s, 0.6, 0.7);
        assert!(page.iter().all(|v| *v >= 250), "the page is white on screen (icc {with_icc}): {page:?}");
        let presentation = s.with(|e, _| e.presentation().clone());
        assert!(presentation.cmyk.enabled);
        let dir = &crate::shell::scenario::repo_root().join("target/cmyk-scenario");
        std::fs::create_dir_all(dir).unwrap();
        let options = crate::export::Options {
            format: crate::export::Format::Pdf, out_dir: dir.into(), name: "white".into(), width: 960, slides: vec![1], layout: false,
            file_name: None, print_backgrounds: true, print_hidden_text: false, print_markup: false, color: Default::default(), handout: None,
            frame: false, handout_landscape: false, paper: None, fit_paper: true, notes_pages: false, outline_pages: false, notes_source: None, handout_marks: Default::default(),
        };
        crate::export::export(presentation, &options).expect("the PDF");
        let pdf = std::fs::read(dir.join("white.pdf")).unwrap();
        assert_eq!(device_cmyk_ops(&pdf), 0, "the PDF is sRGB: no DeviceCMYK colours");
    }
    crate::shell::cmyk::set_converter(std::sync::Arc::new(ibis_doc_model::OpenCmykConverter));
}

/// How many `c m y k k` colour operators the PDF's content streams have.
fn device_cmyk_ops(pdf: &[u8]) -> usize {
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

fn press(s: &mut Scenario, id: &str) {
    let _probe = button_probe::Probe::new();
    s.update(|_, _, cx| cx.notify());
    button_probe::click(s, id);
    s.settle();
}

fn has(s: &mut Scenario, id: &str) -> bool {
    let _probe = button_probe::Probe::new();
    s.update(|_, _, cx| cx.notify());
    button_probe::bounds(id).is_some()
}

/// The colour window has no CMYK segment, no ink fields, no mode check box, no profile button and no K row; the hue ring has no
/// dark rim (the grey tick marks of the old gamut warning made a dark circle round it).
fn picker_is_rgb_only() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, w, cx| e.open_color_picker_dialog(Rgb(0x2e3794), std::rc::Rc::new(|_, _, _, _| {}), w, cx));
    s.settle();
    s.screenshot("picker");
    for id in ["color-cmyk-mode", "color-cmyk-profile", "color-mode-cmyk", "color-slider-6", "color-channel-6"] {
        assert!(!has(&mut s, id), "no {id} in the colour window");
    }
    for id in ["color-mode-hsb", "color-mode-rgb", "color-picker-ok", "color-picker-cancel"] {
        assert!(has(&mut s, id), "the colour window has {id}");
    }
    assert_eq!(s.with(|e, _| e.picker_color()), Some(Rgb(0x2e3794)), "the colour is kept as it is");
    // The ring: the pixels just outside its outer edge are the dialog's own (light), not a dark rim.
    let wheel = s.with(|e, _| e.color_picker_wheel_bounds()).expect("the wheel");
    let (w, pixels) = s.picture();
    let scale = w as f32 / f32::from(s.update(|_, w, _| w.viewport_size().width));
    let (cx, cy) = (f32::from(wheel.center().x), f32::from(wheel.center().y));
    let radius = f32::from(wheel.size.width.min(wheel.size.height)) / 2.0 - 3.0;
    let at = |angle: f32, r: f32| {
        let (x, y) = ((cx + angle.cos() * r) * scale, (cy - angle.sin() * r) * scale);
        let i = (y as usize * w as usize + x as usize) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    for degree in (0..360).step_by(15) {
        let a = (degree as f32).to_radians();
        let rim = at(a, radius + 1.5);
        let ring = at(a, radius - 3.0);
        let dark = |p: [u8; 3]| p.iter().map(|v| *v as u32).sum::<u32>() < 3 * 110;
        assert!(!dark(rim) || dark(ring), "no dark rim outside the ring at {degree} degrees: {rim:?} vs ring {ring:?}");
    }
}

/// The 印刷 panel of every screen has the file's CMYK setting and 印刷 / 設定を保存 / キャンセル in this order; the mode is set
/// there (asking first), kept by 設定を保存 and shown again when the panel is reopened.
fn print_settings_set_cmyk() {
    for (kind, prefix, name) in [(ibis_formats::Kind::Presentation, "pptx-print", "pptx"), (ibis_formats::Kind::Document, "docx-print", "docx"), (ibis_formats::Kind::Workbook, "xlsx-print", "xlsx")] {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| e.new_file(kind, w, cx));
        s.settle();
        s.update(|e, w, cx| e.print(w, cx));
        s.settle();
        let ids = [format!("{prefix}-apply"), format!("{prefix}-save"), format!("{prefix}-cancel")];
        let xs: Vec<f32> = {
            let _probe = button_probe::Probe::new();
            s.update(|_, _, cx| cx.notify());
            ids.iter().map(|id| button_probe::bounds(id).unwrap_or_else(|| panic!("{name}: {id}")).origin.x.into()).collect()
        };
        assert!(xs[0] != xs[1] && xs[1] != xs[2], "{name}: three buttons {xs:?}");
        let mut order = xs.clone();
        order.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let save_between = (xs[0] < xs[1] && xs[1] < xs[2]) || (xs[2] < xs[1] && xs[1] < xs[0]);
        assert!(save_between, "{name}: the save button lies between print and cancel {xs:?}");
        assert!(has(&mut s, &format!("{prefix}-cmyk-mode")) && has(&mut s, &format!("{prefix}-cmyk-profile")), "{name}: the CMYK setting is in the panel");
        s.screenshot(&format!("print-{name}-off"));
        assert!(!s.with(|e, _| e.presentation().cmyk.enabled));
        press(&mut s, &format!("{prefix}-cmyk-mode"));
        // The first use of an ibisOffice feature in a file asks (a file that has them on does not).
        if s.with(|e, _| e.ask.is_some()) {
            press(&mut s, "ask-ok");
        }
        assert!(s.with(|e, _| e.presentation().cmyk.enabled), "{name}: OK turns the mode on");
        s.screenshot(&format!("print-{name}-on"));
        press(&mut s, &format!("{prefix}-save"));
        assert!(!has(&mut s, &format!("{prefix}-apply")), "{name}: the save button closes the panel");
        assert!(s.with(|e, _| e.presentation().cmyk.enabled), "{name}: and keeps the mode");
        s.update(|e, w, cx| e.print(w, cx));
        s.settle();
        assert!(has(&mut s, &format!("{prefix}-cmyk-mode")), "{name}: reopened");
        // Turning it off again.
        press(&mut s, &format!("{prefix}-cmyk-mode"));
        assert!(!s.with(|e, _| e.presentation().cmyk.enabled), "{name}: the check box turns it off");
    }
}

/// The count of pixels of the picture that are exactly `c`.
fn count(s: &mut Scenario, c: Rgb) -> usize {
    let (_, pixels) = s.picture();
    pixels.chunks_exact(4).filter(|p| p[..3] == [c.r(), c.g(), c.b()]).count()
}

/// A vivid green shape is drawn as the colour the press prints with the mode on, as it is with the mode off; the file's value
/// does not change.
fn soft_proof() {
    let green = Rgb(0x00ff00);
    let printed = crate::shell::cmyk::gamut(green);
    assert_ne!(printed, green);
    let mut results = Vec::new();
    for on in [false, true] {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        if on {
            turn_mode_on(&mut s);
        }
        let (slide, shape) = s.with(|e, _| (e.slide().id, e.slide().shapes[0].id));
        s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetFill { slide, shape, fill: ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(green)) }], cx));
        s.settle();
        s.screenshot(&format!("soft-proof-{on}"));
        results.push((count(&mut s, green), count(&mut s, printed)));
        let kept = s.with(|e, _| matches!(&e.slide().shapes[0].kind, ibis_doc_model::ShapeKind::Auto(a) if matches!(&a.fill, ibis_doc_model::Fill::Solid(c) if c.rgb == green)));
        assert!(kept, "the file keeps the sRGB value");
    }
    assert!(results[0].0 > 500 && results[0].1 == 0, "mode off: the shape is its own green {results:?}");
    assert!(results[1].1 > 500 && results[1].0 == 0, "mode on: the shape is the printed green {results:?}");
}

/// With the mode on, the page of a new file of each screen is white (the screenshot has a large white area, no black one).
fn pages_white_on_three_screens() {
    for (kind, name) in [(ibis_formats::Kind::Presentation, "pptx"), (ibis_formats::Kind::Document, "docx"), (ibis_formats::Kind::Workbook, "xlsx")] {
        let mut counts = Vec::new();
        for on in [false, true] {
            let mut s = Scenario::start_view();
            s.update(|e, w, cx| e.new_file(kind, w, cx));
            s.settle();
            if on {
                turn_mode_on(&mut s);
            }
            s.screenshot(&format!("page-{name}-{on}"));
            let (_, pixels) = s.picture();
            let white = pixels.chunks_exact(4).filter(|p| p[..3].iter().all(|v| *v >= 250)).count();
            let black = pixels.chunks_exact(4).filter(|p| p[..3].iter().all(|v| *v <= 20)).count();
            counts.push((white, black, pixels.len() / 4));
        }
        let (off, on) = (counts[0], counts[1]);
        assert!(on.0 * 100 >= off.0 * 95, "{name}: the white area is kept with the mode on {counts:?}");
        assert!(on.1 <= off.1 + on.2 / 100, "{name}: no black page {counts:?}");
    }
}

fn view_item(s: &mut Scenario) -> crate::shell::chrome::MenuItem {
    let menus = s.with(|e, _| e.menu_bar_menus());
    menus.menus.iter().find_map(|(_, entries)| crate::shell::scenario::find_item(entries, "view.print-colors").cloned()).expect("the View menu has the print colours item")
}

/// 表示 > 印刷の色で表示: enabled and on by default in a file with the CMYK mode on (the file's colours are drawn as printed);
/// switched off, the screen draws the colours as they are; disabled in a file without the mode (all three screens have it).
fn view_menu_soft_proof() {
    let green = Rgb(0x00ff00);
    let printed = crate::shell::cmyk::gamut(green);
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let off = view_item(&mut s);
    assert!(!off.enabled(), "no mode, nothing to switch");
    turn_mode_on(&mut s);
    let (slide, shape) = s.with(|e, _| (e.slide().id, e.slide().shapes[0].id));
    s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetFill { slide, shape, fill: ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(green)) }], cx));
    s.settle();
    let item = view_item(&mut s);
    assert!(item.enabled() && item.checked, "on by default with the mode on");
    assert!(count(&mut s, printed) > 500 && count(&mut s, green) == 0, "drawn as printed");
    let handler = item.handler.clone().expect("a handler");
    s.update(|e, w, cx| handler(e, w, cx));
    s.settle();
    s.screenshot("proof-off");
    assert!(!view_item(&mut s).checked, "switched off");
    assert!(count(&mut s, green) > 500 && count(&mut s, printed) == 0, "drawn as it is");
    s.update(|e, w, cx| handler(e, w, cx));
    s.settle();
    assert!(count(&mut s, printed) > 500, "and on again");
    drop(s);
    for kind in [ibis_formats::Kind::Document, ibis_formats::Kind::Workbook] {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| e.new_file(kind, w, cx));
        s.settle();
        assert!(!view_item(&mut s).enabled());
        turn_mode_on(&mut s);
        assert!(view_item(&mut s).enabled());
    }
}

/// キャンセル of the print panel puts the CMYK setting back (also the × of the panel); 設定を保存 keeps it.
fn cancel_puts_it_back() {
    for (kind, prefix, name) in [(ibis_formats::Kind::Presentation, "pptx-print", "pptx"), (ibis_formats::Kind::Document, "docx-print", "docx"), (ibis_formats::Kind::Workbook, "xlsx-print", "xlsx")] {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| e.new_file(kind, w, cx));
        s.settle();
        let on = |s: &mut Scenario| s.with(|e, _| e.presentation().cmyk.enabled);
        // Off before: turned on in the panel, Cancel: off again.
        s.update(|e, w, cx| e.print(w, cx));
        s.settle();
        press(&mut s, &format!("{prefix}-cmyk-mode"));
        if s.with(|e, _| e.ask.is_some()) {
            press(&mut s, "ask-ok");
        }
        assert!(on(&mut s), "{name}: on in the panel");
        press(&mut s, &format!("{prefix}-cancel"));
        assert!(!on(&mut s), "{name}: Cancel puts it back to off");
        // The same with the panel's × (the dialog's close).
        s.update(|e, w, cx| e.print(w, cx));
        s.settle();
        press(&mut s, &format!("{prefix}-cmyk-mode"));
        if s.with(|e, _| e.ask.is_some()) {
            press(&mut s, "ask-ok");
        }
        assert!(on(&mut s));
        s.update(|e, w, cx| e.close_dialog(w, cx));
        s.settle();
        assert!(!on(&mut s), "{name}: closing the panel is Cancel");
        // On before: turned off in the panel, Cancel: on again; turned off and 設定を保存: off.
        s.update(|e, w, cx| e.print(w, cx));
        s.settle();
        press(&mut s, &format!("{prefix}-cmyk-mode"));
        if s.with(|e, _| e.ask.is_some()) {
            press(&mut s, "ask-ok");
        }
        press(&mut s, &format!("{prefix}-save"));
        assert!(on(&mut s), "{name}: saved on");
        s.update(|e, w, cx| e.print(w, cx));
        s.settle();
        press(&mut s, &format!("{prefix}-cmyk-mode"));
        assert!(!on(&mut s));
        press(&mut s, &format!("{prefix}-cancel"));
        assert!(on(&mut s), "{name}: Cancel puts it back to on");
        s.update(|e, w, cx| e.print(w, cx));
        s.settle();
        press(&mut s, &format!("{prefix}-cmyk-mode"));
        press(&mut s, &format!("{prefix}-save"));
        assert!(!on(&mut s), "{name}: Save settings keeps the change");
    }
}

/// The file as the window would save it, opened again: the mode and the profile name are still there (all three formats).
fn saved_and_reopened() {
    for (kind, name) in [(ibis_formats::Kind::Presentation, "pptx"), (ibis_formats::Kind::Document, "docx"), (ibis_formats::Kind::Workbook, "xlsx")] {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| e.new_file(kind, w, cx));
        s.settle();
        turn_mode_on(&mut s);
        s.update(|e, _, cx| {
            let mut mode = e.presentation().cmyk.clone();
            mode.profile = Some("Test Press|0123".into());
            e.run(vec![ibis_doc_model::Command::SetCmykMode { cmyk: mode }], cx);
        });
        let bytes = s.with(|e, _| e.current_bytes()).unwrap_or_else(|_| panic!("{name}: the file as saved"));
        let opened = ibis_formats::open(bytes).unwrap_or_else(|e| panic!("{name}: reopen {e:?}"));
        assert!(opened.presentation.cmyk.enabled, "{name}: the mode is on after saving and opening");
        assert_eq!(opened.presentation.cmyk.profile.as_deref(), Some("Test Press|0123"), "{name}");
    }
}

/// Every CMYK ICC profile found in the usual places of this Mac (the system's, and the ones a layout app brings): paper white
/// comes back white and black dark (a profile read in the wrong direction gave a black page).
fn real_profiles() {
    let mut dirs: Vec<std::path::PathBuf> = vec!["/Library/ColorSync/Profiles".into(), "/System/Library/ColorSync/Profiles".into(), "/Applications/Affinity.app/Contents/Resources".into()];
    if let Ok(env) = std::env::var("IBIS_ICC_DIR") {
        dirs.push(env.into());
    }
    let mut checked = 0;
    for dir in dirs {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            let path = entry.path();
            if !matches!(path.extension().and_then(|e| e.to_str()), Some("icc" | "icm")) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            if bytes.len() < 20 || &bytes[16..20] != b"CMYK" {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let Ok(c) = crate::shell::cmyk_profile::icc_converter(&bytes, &name) else { eprintln!("PROFILE {name}: not usable"); continue };
            let white = c.to_cmyk(Rgb(0xffffff));
            let (w, k) = (c.to_gamut(Rgb(0xffffff)), c.to_gamut(Rgb(0x000000)));
            eprintln!("PROFILE {name}: white -> {white:?} -> {:06x}; black -> {:06x}", w.0, k.0);
            assert!(w.r() >= 240 && w.g() >= 240 && w.b() >= 240, "{name}: white stays white, got {:06x}", w.0);
            assert!(k.r() <= 120 && k.g() <= 120 && k.b() <= 120, "{name}: black stays dark (newsprint ink is grey), got {:06x}", k.0);
            // Chosen while the file is open with the mode on (the way a user gets there): the page stays white on screen.
            let mut s = Scenario::open("test/editing/pptx-plan.pptx");
            turn_mode_on(&mut s);
            crate::shell::cmyk::set_converter(c);
            s.update(|_, _, cx| cx.notify());
            s.settle();
            let page = pixel(&mut s, 0.6, 0.7);
            assert!(page.iter().all(|v| *v >= 250), "{name}: the page is white on screen after choosing the profile: {page:?}");
            checked += 1;
        }
    }
    crate::shell::cmyk::set_converter(std::sync::Arc::new(ibis_doc_model::OpenCmykConverter));
    eprintln!("PROFILE checked {checked}");
}

/// The colour window of a file with the CMYK mode on draws its palette, swatches, hue ring and square as the press prints them
/// (the same conversion as the document); what it saves is still the RGB colour (the window has no CMYK UI).
fn picker_soft_proof() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    turn_mode_on(&mut s);
    let got = std::rc::Rc::new(std::cell::Cell::new(None));
    let sink = got.clone();
    s.update(|e, w, cx| e.open_color_picker_dialog(Rgb(0x2e3794), std::rc::Rc::new(move |_, c, _, _| sink.set(c)), w, cx));
    s.settle();
    s.screenshot("picker-cmyk-mode");
    for id in ["color-cmyk-mode", "color-cmyk-profile", "color-mode-cmyk", "color-slider-6"] {
        assert!(!has(&mut s, id), "no {id}");
    }
    // The palette's vivid green swatch is the printed green.
    let green = Rgb(0x00ff00);
    let printed = crate::shell::cmyk::gamut(green);
    assert_ne!(printed, green);
    assert!(count(&mut s, printed) > 300, "the palette shows the printed green");
    assert_eq!(count(&mut s, green), 0, "no unconverted vivid green in the window");
    // The ring at 120 degrees (green) is drawn in the printed colour.
    let wheel = s.with(|e, _| e.color_picker_wheel_bounds()).expect("the wheel");
    let (w, pixels) = s.picture();
    let scale = w as f32 / f32::from(s.update(|_, w, _| w.viewport_size().width));
    let radius = f32::from(wheel.size.width.min(wheel.size.height)) / 2.0 - 3.0 - 11.0;
    let a = 120f32.to_radians();
    let (x, y) = ((f32::from(wheel.center().x) + a.cos() * radius) * scale, (f32::from(wheel.center().y) - a.sin() * radius) * scale);
    let i = (y as usize * w as usize + x as usize) * 4;
    let ring = [pixels[i], pixels[i + 1], pixels[i + 2]];
    let want = [printed.r(), printed.g(), printed.b()];
    assert!(ring.iter().zip(want).all(|(a, b)| (*a as i32 - b as i32).abs() <= 6), "the ring at 120 degrees is {ring:?}, wanted about {want:?}");
    // What is saved is the colour as chosen.
    s.update(|e, w, cx| e.commit_color_picker(w, cx));
    assert_eq!(got.get(), Some(Rgb(0x2e3794)));
}
