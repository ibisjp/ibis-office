// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pictures of the whole editor window, drawn headless, for checking the
//! window's layout without a screen (no Screen Recording permission).
//!
//! ```text
//! ibisOffice --export ui [--out-dir <dir>] [--steps start,pagedown,end] [<file>]
//! ```
//!
//! Without a file the window shows the start view. Each step is done in
//! turn and the window (1280 × 800) captured after it as
//! `<out-dir>/<name>-ui-NN.png`: `start` (nothing), a key such as `pagedown`, `end`,
//! `down` or `cmd-up`, `wheel:<pixels>` (a trackpad scroll over the
//! workspace, down if positive) or `wheel:<pixels>:<x>:<y>` (at a window
//! position), `page:<n>` (show page n), `loading:pptx|xlsx|docx` (an empty
//! document of the kind, as shown while a file loads), `zoom:<percent>`
//! or `zoom:fit`, `click` (a press in the middle of the workspace) or
//! `click:<x>:<y>` (at a window position), `dblclick[:<x>:<y>]` (a double
//! click, which starts editing a shape's text), `save-as:<path>` (save the
//! file there, as 名前を付けて保存 does), `drag:<x0>:<y0>:<x1>:<y1>`
//! (press, move and release), and `move:<x>:<y>` (the pointer moved there,
//! no button pressed, for hover), `wait:<ms>` (let animations such as
//! a scrollbar's fading run that long), `size:<width>:<height>` (resize
//! the window; later pictures are that size), `zoom-menu` (open the status
//! bar's list of zooms), `zoom-wheel:<pixels>[:<x>:<y>]` (a trackpad scroll
//! with the wheel's zoom modifier held, zooming in if positive),
//! `slideshow` (start the slide show), `transition-at:<ms>` (draw the slide show's transition at that time since the slide came in, whatever
//! the clock says: the same moment of an effect on any machine; give it after the `right` that starts the transition), `goto:<reference>` (select a
//! worksheet's cells as the name box does, such as `goto:A201` or
//! `goto:'Sheet 2'!B3:C4`), `type:<text>` (type the text into what has
//! the keys, such as the name box after a `click` on it) and `open:<path>`
//! (open the file as 開く… does, adding it to the recent files; set `HOME`
//! to keep the real list untouched).

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::Root;
use gpui_kit::*;

use crate::SlideEditor;
use crate::pptx::ext::*;
use crate::xlsx::ext::*;

/// Open `document` (a presentation and the name of its file) in the editor,
/// or the start view without one, and
/// write a picture of the window after each of `steps`, named `<stem>-ui-NN.png`.
pub(crate) fn run(
    document: Option<(ibis_doc_model::Presentation, SharedString)>,
    out: &std::path::Path,
    stem: &str,
    steps: &str,
) -> Result<(), String> {

    let (_platform, mut cx) = crate::export::headless_app();
    cx.allow_parking();
    cx.update(crate::init);
    let editor: Rc<RefCell<Option<Entity<SlideEditor>>>> = Rc::default();
    let slot = editor.clone();
    let window = cx
        .open_window(size(px(1280.), px(800.)), move |window, app| {
            let view = app.new(|cx| {
                let mut editor = SlideEditor::new(ibis_doc_model::Presentation::new(), window, cx);
                if let Some((presentation, name)) = document {
                    editor.load(presentation, name, window, cx);
                }
                editor
            });
            let focus = view.read(app).focus_handle.clone();
            focus.focus(window, app);
            *slot.borrow_mut() = Some(view.clone());
            app.new(|cx| Root::new(view, window, cx))
        })
        .map_err(|e| e.to_string())?;
    let editor = editor.borrow().clone().unwrap();
    let draw = |cx: &mut HeadlessAppContext| {
        // Every picture decoded, as a person would see after a moment.
        cx.update(|app| editor.read(app).resources.preload.clone()).wait_all();
        for _ in 0..3 {
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, _| window.refresh()).ok();
        }
    };
    draw(&mut cx);
    for (n, step) in steps.split(',').map(str::trim).enumerate() {
        let middle = cx.update(|app| editor.read(app).workspace_bounds.map(|b| b.center()));
        let middle = middle.unwrap_or(point(px(640.), px(400.)));
        cx.update_window(window.into(), |_, window, app| {
            let zoom_wheel = step.strip_prefix("zoom-wheel:");
            if let Some(args) = step.strip_prefix("wheel:").or(zoom_wheel) {
                let numbers: Vec<f32> = args.split(':').filter_map(|v| v.parse().ok()).collect();
                let y = numbers.first().copied().unwrap_or(0.0);
                let middle = numbers.get(1..3).map_or(middle, |v| point(px(v[0]), px(v[1])));
                // The zoom modifier: Command and Control on a Mac.
                let modifiers = Modifiers { control: zoom_wheel.is_some(), platform: zoom_wheel.is_some(), ..Default::default() };
                // A zoom in turns the wheel up, the other way from scrolling down.
                let y = if zoom_wheel.is_some() { -y } else { y };
                // The pointer over the workspace first, so the wheel reaches it.
                let hover = MouseMoveEvent { position: middle, ..Default::default() };
                window.dispatch_event(PlatformInput::MouseMove(hover), app);
                window.dispatch_event(
                    PlatformInput::ScrollWheel(ScrollWheelEvent {
                        position: middle,
                        delta: ScrollDelta::Pixels(point(px(0.), px(-y))),
                        modifiers,
                        ..Default::default()
                    }),
                    app,
                );
            } else if let Some(args) = step.strip_prefix("size:") {
                // Resize the window's content, as when the user drags its edge.
                let numbers: Vec<f32> = args.split(':').filter_map(|v| v.parse().ok()).collect();
                if let [width, height] = numbers[..] {
                    window.resize(size(px(width), px(height)));
                    // The headless window has no resize callback: lay out at the new size.
                    window.bounds_changed(app);
                }
            } else if let Some(page) = step.strip_prefix("page:") {
                editor.update(app, |e, cx| e.show_slide_number(page.parse().unwrap_or(1), window, cx));
            } else if let Some(kind) = step.strip_prefix("loading:").and_then(ibis_pptx::Kind::from_extension) {
                // What a window shows while a file of the kind loads.
                editor.update(app, |e, cx| {
                    e.has_document = false;
                    e.begin_loading(kind, "loading".into(), window, cx)
                });
            } else if let Some(zoom) = step.strip_prefix("zoom:") {
                let zoom = zoom.parse::<f32>().ok().map(|z| z / 100.0);
                editor.update(app, |e, cx| e.set_zoom(zoom, window, cx));
            } else if step == "zoom-menu" {
                editor.update(app, |e, cx| e.set_zoom_menu_open(true, window, cx));
            } else if let Some(reference) = step.strip_prefix("goto:") {
                if !editor.update(app, |e, cx| e.go_to_reference(reference, window, cx)) {
                    eprintln!("{step}: invalid reference");
                }
            } else if let Some(text) = step.strip_prefix("type:") {
                for c in text.chars() {
                    let key = c.to_lowercase().to_string();
                    let modifiers = Modifiers { shift: c.is_uppercase(), ..Default::default() };
                    window.dispatch_keystroke(Keystroke { modifiers, key, key_char: Some(c.to_string()) }, app);
                }
            } else if let Some(path) = step.strip_prefix("open:") {
                editor.update(app, |e, cx| e.open_path(path.into(), None, window, cx));
            } else if step == "slideshow" {
                editor.update(app, |e, cx| e.start_slideshow(window, cx));
            } else if let Some(ms) = step.strip_prefix("transition-at:") {
                let at = ms.parse::<f32>().ok();
                editor.update(app, |e, cx| {
                    if let Some(show) = e.slideshow_mut() {
                        show.transition_at = at;
                    }
                    cx.notify();
                });
                window.refresh();
            } else if let Some(ms) = step.strip_prefix("wait:") {
                std::thread::sleep(std::time::Duration::from_millis(ms.parse().unwrap_or(0)));
                window.refresh();
            } else if let Some(args) = step.strip_prefix("move:") {
                let numbers: Vec<f32> = args.split(':').filter_map(|v| v.parse().ok()).collect();
                let position = numbers.get(0..2).map_or(middle, |v| point(px(v[0]), px(v[1])));
                let hover = MouseMoveEvent { position, ..Default::default() };
                window.dispatch_event(PlatformInput::MouseMove(hover), app);
            } else if let Some(path) = step.strip_prefix("save-as:") {
                // Save as 名前を付けて保存 does, without its dialog.
                if let Err(err) = editor.update(app, |e, _| e.save_to(path.into(), window)) {
                    eprintln!("{step}: {err}");
                }
            } else if step == "click" || step.starts_with("click:") || step.starts_with("drag:") || step == "dblclick" || step.starts_with("dblclick:") {
                let numbers: Vec<f32> = step.split(':').skip(1).filter_map(|v| v.parse().ok()).collect();
                let at = |i: usize| numbers.get(i..i + 2).map_or(middle, |v| point(px(v[0]), px(v[1])));
                let (from, to) = (at(0), if numbers.len() >= 4 { at(2) } else { at(0) });
                if step.starts_with("dblclick") {
                    // The first click of the two.
                    let down = MouseDownEvent { button: MouseButton::Left, position: from, click_count: 1, ..Default::default() };
                    window.dispatch_event(PlatformInput::MouseDown(down), app);
                    let up = MouseUpEvent { button: MouseButton::Left, position: from, click_count: 1, ..Default::default() };
                    window.dispatch_event(PlatformInput::MouseUp(up), app);
                }
                let clicks = if step.starts_with("dblclick") { 2 } else { 1 };
                let down = MouseDownEvent { button: MouseButton::Left, position: from, click_count: clicks, ..Default::default() };
                window.dispatch_event(PlatformInput::MouseDown(down), app);
                if to != from {
                    let mv = MouseMoveEvent { position: to, pressed_button: Some(MouseButton::Left), ..Default::default() };
                    window.dispatch_event(PlatformInput::MouseMove(mv), app);
                }
                let up = MouseUpEvent { button: MouseButton::Left, position: to, click_count: clicks, ..Default::default() };
                window.dispatch_event(PlatformInput::MouseUp(up), app);
            } else if step != "start"
                && let Ok(keystroke) = Keystroke::parse(step)
            {
                window.dispatch_keystroke(keystroke, app);
            }
        })
        .map_err(|e| e.to_string())?;
        draw(&mut cx);
        let image = cx.capture_screenshot(window.into()).map_err(|e| e.to_string())?;
        let target = out.join(format!("{stem}-ui-{:02}.png", n + 1));
        image.save(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        let state = cx.update(|app| {
            let e = editor.read(app);
            let index = e.presentation().slide_index(e.current_slide).unwrap_or(0) + 1;
            (index, e.presentation().slides.len(), e.workspace_scroll.offset(), e.selection, e.selected_range())
        });
        println!("{step}: {} {state:?}", target.display());
    }
    Ok(())
}
