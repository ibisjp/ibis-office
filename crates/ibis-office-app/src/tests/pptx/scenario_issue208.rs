// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #208: the web page of a presentation is a slide show. The page carries its script (the keys, the swipe, the number),
//! and, where Chrome is installed, opening it in headless Chrome shows the first slide filling the window with black where the
//! slide's proportions leave room.
use std::process::Command;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue208_the_page_has_the_show_script_and_no_outside_reads", script),
        ("pptx::issue208_chrome_shows_the_first_slide_full_window_letterboxed", chrome),
    ]
}

fn page() -> String {
    let root = crate::shell::scenario::repo_root();
    let data = std::fs::read(root.join("test/editing/pptx-plan.pptx")).unwrap();
    let presentation = ibis_formats::read(&data).unwrap();
    ibis_formats::slides_html::slides_to_html(&presentation, "plan", "ja", &ibis_formats::slides_html::Labels::default()).unwrap()
}

fn script() {
    let html = page();
    assert!(html.contains("<script>") && html.contains("function onKey(e)") && html.contains("show-number"));
    assert!(!html.contains("fetch(") && !html.contains("XMLHttpRequest") && !html.contains("<script src"));
}

fn chrome() {
    let chrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
    if !std::path::Path::new(chrome).exists() {
        return;
    }
    let dir = crate::shell::scenario::repo_root().join("target/scenario/issue208");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("slideshow.html");
    std::fs::write(&file, page()).unwrap();
    let png = dir.join("first-slide.png");
    let _ = std::fs::remove_file(&png);
    // A square window: the slide (wider than tall) leaves black above and below.
    let status = Command::new(chrome)
        .args(["--headless=new", "--disable-gpu", "--hide-scrollbars", "--window-size=1000,1000", "--virtual-time-budget=2000"])
        .arg(format!("--user-data-dir={}", dir.join("chrome-profile").display()))
        .arg(format!("--screenshot={}", png.display()))
        .arg(format!("file://{}", file.display()))
        .output()
        .unwrap();
    assert!(png.exists(), "no screenshot: {}", String::from_utf8_lossy(&status.stderr));
    let img = image::open(&png).unwrap().to_rgba8();
    let (w, h) = img.dimensions();
    let black = |x: u32, y: u32| {
        let p = img.get_pixel(x, y).0;
        p[0] < 8 && p[1] < 8 && p[2] < 8
    };
    assert!(black(w / 2, 2) && black(w / 2, h - 3), "black above and below the slide ({w}x{h})");
    // The slide touches both sides of the window: its width is the window's.
    let row = h / 2;
    assert!(!black(2, row) || !black(w / 2, row), "the slide is drawn in the middle");
    let lit = (0..h).filter(|y| !black(w / 2, *y)).count() as f64 / h as f64;
    assert!(lit > 0.3 && lit < 0.8, "the slide takes the window's width (lit share {lit})");
}
