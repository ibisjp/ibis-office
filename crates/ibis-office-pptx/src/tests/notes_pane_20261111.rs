// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-11 (#M96 (4)): a presentation opens with its notes pane as the file's normal view left it (`viewProps.xml`'s
//! `normalViewPr`, as measured in PowerPoint): folded (no pane here) for PowerPoint's own 94606, open with the share
//! `100 - restoredTop` for a smaller slide pane, none for `maximized`. The button (and the menu) open the folded pane at the default height.
use crate::ext::*;
use crate::shell::scenario::{repo_root, Scenario};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::notes_pane_20261111_a_folded_file_opens_without_the_pane_and_the_button_opens_it", folded),
        ("pptx::notes_pane_20261111_a_small_slide_pane_opens_the_notes_at_its_share", open_share),
        ("pptx::notes_pane_20261111_maximized_has_no_notes_pane", maximized),
        ("pptx::notes_pane_20261111_a_slide_pane_below_its_least_height_opens_at_the_least", least_slide_pane),
        ("pptx::notes_pane_20261111_hiding_moving_and_showing_the_pane_are_saved", saved_after_the_user_touches_it),
        ("pptx::notes_pane_20261111_the_border_taken_to_the_bottom_hides_the_pane", to_the_bottom_hides),
        ("pptx::notes_pane_20261111_hiding_after_a_move_keeps_the_moved_border", hidden_after_a_move),
        ("pptx::notes_pane_20261111_taken_to_the_top_after_a_move_keeps_the_moved_border", top_after_a_move),
    ]
}

/// The base file with its `normalViewPr` replaced, written under `target/scenario-input/`, and opened as the application opens a file (`open_path`).
fn open_with(name: &str, normal: &str) -> Scenario {
    let source = repo_root().join("test/editing/pptx-comments-modern.pptx");
    let mut package = ibis_pptx::ibis_ooxml::Package::open(std::fs::read(&source).unwrap()).unwrap();
    let xml = String::from_utf8(package.read("ppt/viewProps.xml").unwrap()).unwrap();
    let (start, end) = (xml.find("<p:normalViewPr>").unwrap(), xml.find("</p:normalViewPr>").unwrap() + "</p:normalViewPr>".len());
    package.set_bytes("ppt/viewProps.xml", format!("{}{}{}", &xml[..start], normal, &xml[end..]).into_bytes());
    let path = repo_root().join("target/scenario-input").join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, package.save(Default::default()).unwrap()).unwrap();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.open_path(path.clone(), None, w, cx));
    for _ in 0..200 {
        s.settle();
        if s.with(|e, _| e.has_document) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(s.with(|e, _| e.has_document), "the file opens");
    s.settle();
    s
}

fn normal(attrs: &str, top: i64) -> String {
    format!("<p:normalViewPr{attrs}><p:restoredLeft sz=\"15621\"/><p:restoredTop sz=\"{top}\"/></p:normalViewPr>")
}

/// PowerPoint's own 94606: the pane is folded, which is no pane here; the title row's button (`toggle_notes`) opens it at the default height.
fn folded() {
    let mut s = open_with("notes-pane-folded.pptx", &normal("", 94_606));
    assert!(!s.with(|e, _| e.notes_shown()), "folded on opening");
    s.update(|e, w, cx| e.toggle_notes(w, cx));
    s.settle();
    assert!(s.with(|e, _| e.notes_shown()), "the button opens it");
    assert_eq!(s.with(|e, _| e.pptx().map(|p| p.notes_height)), Some(crate::notes::PANE_HEIGHT), "at the default height");
    s.update(|e, w, cx| e.toggle_notes(w, cx));
    assert!(!s.with(|e, _| e.notes_shown()), "and closes it again");
}

/// A slide pane of 70% leaves 30% to the notes: the pane is open, and its share of the two panes together is 30%.
fn open_share() {
    let mut s = open_with("notes-pane-open.pptx", &normal("", 70_000));
    assert!(s.with(|e, _| e.notes_shown()), "open on opening");
    s.settle();
    let (notes, slide) = s.with(|e, _| (e.pptx().map_or(0.0, |p| p.notes_height), e.workspace_bounds.map_or(0.0, |b| f32::from(b.size.height))));
    assert!(s.with(|e, _| e.pptx().is_some_and(|p| p.notes_open_percent.is_none())), "sized once");
    let share = notes / (notes + slide);
    assert!((share - 0.30).abs() < 0.03, "the notes pane is 30% of the two panes: {notes} of {}", notes + slide);
}

fn maximized() {
    let mut s = open_with("notes-pane-maximized.pptx", &normal(" horzBarState=\"maximized\"", 70_000));
    assert!(!s.with(|e, _| e.notes_shown()), "no notes pane");
}

/// `restoredTop` of 10000 (the slide pane would be 10%): the screen keeps the slide pane at its own least height (`SLIDE_PANE_MIN_HEIGHT`,
/// 120 pixels), whatever the share, and the notes pane gets the rest.
fn least_slide_pane() {
    let mut s = open_with("notes-pane-least.pptx", &normal("", 10_000));
    s.settle();
    let (notes, slide) = s.with(|e, _| (e.pptx().map_or(0.0, |p| p.notes_height), e.workspace_bounds.map_or(0.0, |b| f32::from(b.size.height))));
    assert!(slide >= crate::notes::SLIDE_PANE_MIN_HEIGHT - 1.0 && slide < crate::notes::SLIDE_PANE_MIN_HEIGHT + 2.0, "the slide pane at its least: {slide} (the notes pane {notes})");
}

/// The `normalViewPr` of the file `path`.
fn normal_view_of(path: &std::path::Path) -> String {
    let package = ibis_pptx::ibis_ooxml::Package::open(std::fs::read(path).unwrap()).unwrap();
    let xml = String::from_utf8(package.read("ppt/viewProps.xml").unwrap()).unwrap();
    let (start, end) = (xml.find("<p:normalViewPr").unwrap(), xml.find("</p:normalViewPr>").unwrap());
    xml[start..end].to_string()
}

/// Hiding the pane saves `maximized`, showing it again (the border not moved) saves no `horzBarState`, and moving the border saves the slide
/// pane's share in `restoredTop` with `autoAdjust="0"` (tasks/doc/m101-softedge-1b-notes-pane.md, (3)).
fn saved_after_the_user_touches_it() {
    let mut s = open_with("notes-pane-touch.pptx", &normal("", 70_000));
    s.update(|e, w, cx| e.toggle_notes(w, cx));
    let path = s.save_as("notes-pane-hidden.pptx");
    let view = normal_view_of(&path);
    assert!(view.starts_with("<p:normalViewPr horzBarState=\"maximized\">") && view.contains("<p:restoredTop sz=\"70000\"/>"), "{view}");
    s.update(|e, w, cx| e.toggle_notes(w, cx));
    let path = s.save_as("notes-pane-shown.pptx");
    let view = normal_view_of(&path);
    assert!(view.starts_with("<p:normalViewPr>") && view.contains("<p:restoredTop sz=\"70000\"/>"), "{view}");
    // The border moved (as the divider's drag sets the height): the slide pane's share, in 1/1000 % of the two panes together.
    s.update(|e, _, _| {
        if let Some(p) = e.pptx_mut() {
            p.notes_height = 200.0;
            p.notes_touched.moved = true;
        }
    });
    s.settle();
    let (notes, slide) = s.with(|e, _| (e.pptx().map_or(0.0, |p| p.notes_height), e.workspace_bounds.map_or(0.0, |b| f32::from(b.size.height))));
    let path = s.save_as("notes-pane-moved.pptx");
    let view = normal_view_of(&path);
    let sz: f32 = view.split("<p:restoredTop sz=\"").nth(1).and_then(|v| v.split('"').next()).and_then(|v| v.parse().ok()).expect(&view);
    assert!(view.starts_with("<p:normalViewPr>") && view.contains("autoAdjust=\"0\""), "{view}");
    assert!((sz / 100_000.0 - slide / (slide + notes)).abs() < 0.01, "{sz} for the slide pane {slide} of {}", slide + notes);
}

/// The divider between the slide and the notes pane, window coordinates: just under the workspace.
fn divider(s: &mut Scenario) -> (f32, f32) {
    s.with(|e, _| {
        let b = e.workspace_bounds.expect("laid out");
        (f32::from(b.origin.x) + f32::from(b.size.width) / 2.0, f32::from(b.origin.y) + f32::from(b.size.height) + 2.0)
    })
}

/// The slide pane's `restoredTop` and the `horzBarState` as written in the file `path`.
fn view_values(path: &std::path::Path) -> (Option<i64>, Option<i64>, bool, Option<String>) {
    let view = normal_view_of(path);
    let top = |v: &str| v.split("<p:restoredTop sz=\"").nth(1).and_then(|t| t.split('"').next()).and_then(|t| t.parse().ok());
    let state = view.split("horzBarState=\"").nth(1).and_then(|t| t.split('"').next()).map(str::to_string);
    (top(&view), None, view.contains("autoAdjust=\"0\""), state)
}

/// The border dragged down until the notes pane would be under half its least height hides the pane: `maximized`, and `restoredTop` the file's own (the border was
/// not moved before). Letting go higher up only moves the border (#M114 (a)).
fn to_the_bottom_hides() {
    let mut s = open_with("notes-pane-bottom.pptx", &normal("", 70_000));
    s.settle();
    let (x, y) = divider(&mut s);
    // Not far enough down: the notes pane at its least height. The pane stays.
    let h0 = s.with(|e, _| e.pptx().map_or(0.0, |p| p.notes_height));
    s.drag_through(&[(x, y), (x, y + h0 - 60.0)]);
    s.settle();
    assert!(s.with(|e, _| e.notes_shown()), "not at the bottom: the pane stays");
    // All the way down, from a new start.
    drop(s);
    let mut s = open_with("notes-pane-bottom-b.pptx", &normal("", 70_000));
    s.settle();
    let (x, y) = divider(&mut s);
    let h0 = s.with(|e, _| e.pptx().map_or(0.0, |p| p.notes_height));
    s.drag_through(&[(x, y), (x, y + h0)]);
    s.settle();
    assert!(!s.with(|e, _| e.notes_shown()), "the pane is hidden");
    let (top, _, adjusted, state) = view_values(&s.save_as("notes-pane-bottom-1.pptx"));
    assert_eq!(state.as_deref(), Some("maximized"));
    assert_eq!((top, adjusted), (Some(70_000), false), "the file's own restoredTop");
}

/// Moved, then hidden from the button: `restoredTop` is where the border stood (with `autoAdjust="0"`), not the file's value (#M114 (b)).
fn hidden_after_a_move() {
    let mut s = open_with("notes-pane-move-hide.pptx", &normal("", 70_000));
    s.settle();
    let (x, y) = divider(&mut s);
    s.drag_through(&[(x, y), (x, y - 150.0)]);
    s.settle();
    let (notes, slide) = s.with(|e, _| (e.pptx().map_or(0.0, |p| p.notes_height), e.workspace_bounds.map_or(0.0, |b| f32::from(b.size.height))));
    let moved = slide / (slide + notes);
    assert!((moved - 0.70).abs() > 0.1, "the border moved: {moved}");
    s.update(|e, w, cx| e.toggle_notes(w, cx));
    s.settle();
    let (top, _, adjusted, state) = view_values(&s.save_as("notes-pane-move-hide-out.pptx"));
    assert_eq!(state.as_deref(), Some("maximized"));
    assert!(adjusted, "autoAdjust 0");
    let top = top.expect("restoredTop") as f32 / 100_000.0;
    assert!((top - moved).abs() < 0.01, "restoredTop {top} is the border that was moved to {moved}");
}

/// Moved, then taken to the top: `minimized` with `restoredTop` the border before that last drag.
fn top_after_a_move() {
    let mut s = open_with("notes-pane-move-top.pptx", &normal("", 70_000));
    s.settle();
    let (x, y) = divider(&mut s);
    s.drag_through(&[(x, y), (x, y - 100.0)]);
    s.settle();
    let (notes, slide) = s.with(|e, _| (e.pptx().map_or(0.0, |p| p.notes_height), e.workspace_bounds.map_or(0.0, |b| f32::from(b.size.height))));
    let moved = slide / (slide + notes);
    let (x, y) = divider(&mut s);
    // Up to the slide pane's least height, and further.
    let (_notes, slide) = s.with(|e, _| (e.pptx().map_or(0.0, |p| p.notes_height), e.workspace_bounds.map_or(0.0, |b| f32::from(b.size.height))));
    s.drag_through(&[(x, y), (x, y - (slide - crate::notes::SLIDE_PANE_MIN_HEIGHT) - 3.0)]);
    s.settle();
    let (top, _, adjusted, state) = view_values(&s.save_as("notes-pane-move-top-out.pptx"));
    assert_eq!(state.as_deref(), Some("minimized"));
    assert!(adjusted, "autoAdjust 0");
    let top = top.expect("restoredTop") as f32 / 100_000.0;
    assert!((top - moved).abs() < 0.01, "restoredTop {top} is the border before the last drag: {moved}");
}
