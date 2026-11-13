// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-07: the animation pane's drag of `layout_test2.pptx` page 3 (both text boxes open): any row's handle folds the open text
//! boxes, and two paragraphs swapped keep their text box's row.
use super::fix_20261106_animation::{mouse_down, mouse_move, mouse_up, step};
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::fix_20261107_every_row_handle_folds_the_open_text_boxes", folds_on_any_row),
        ("pptx::fix_20261107_two_paragraphs_swapped_keep_their_text_box_row", swapped_children_keep_the_parent),
        ("pptx::fix_20261107_held_row_keeps_its_place_when_siblings_fold", held_row_stays),
        ("pptx::fix_20261107_text_box_runs_in_any_paragraph_order", runs_in_any_order),
        ("pptx::fix_20261107_dialog_panels_come_before_the_animation_pane", dialogs_before_the_pane),
    ]
}

const FILE: &str = "test/github-issue/62/layout_test2.pptx";

fn present() -> bool {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(FILE).exists()
}

/// Page 3 with every text box (two or more paragraphs in a row) open.
fn open_page() -> Scenario {
    let mut s = Scenario::open(FILE);
    s.size(1280., 720.);
    s.page(3);
    s.update(|e, _, cx| e.toggle_animation_pane(cx));
    let keys: Vec<crate::animation_ui::BoxKey> = s.with(|e, _| {
        let a = &e.slide().animations;
        let all = crate::animation_ui::box_keys(a);
        let mut out = vec![];
        let mut i = 0;
        while i < a.len() {
            let (f, l) = crate::animation_ui::paragraph_run(a, i);
            if l > f {
                out.push(all[f]);
            }
            i = l + 1;
        }
        out
    });
    assert!(keys.len() >= 2, "page 3 has {} text box rows, two are needed", keys.len());
    for k in keys {
        s.update(|e, _, cx| e.toggle_animation_expanded(k, cx));
    }
    s.settle();
    s
}

fn shown(s: &mut Scenario) -> usize {
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    (0..40).take_while(|i| dnd.row_bounds("anim-table", *i).is_some()).count()
}

fn open_count(s: &mut Scenario) -> usize {
    s.with(|e, _| e.pptx().unwrap().animation_expanded.len())
}

fn folds_on_any_row() {
    if !present() {
        return;
    }
    let mut s = open_page();
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    let all_open = open_count(&mut s);
    let rows_open = shown(&mut s);
    // The kinds of display rows: (effect index, parent/child/plain) from the effects.
    let kinds: Vec<(usize, bool, bool)> = s.with(|e, _| {
        let a = &e.slide().animations;
        let mut out = vec![];
        let mut i = 0;
        while i < a.len() {
            let (f, l) = crate::animation_ui::paragraph_run(a, i);
            if l > f {
                out.push((f, true, false));
                out.extend((f..=l).map(|c| (c, false, true)));
            } else {
                out.push((i, false, false));
            }
            i = l + 1;
        }
        out
    });
    assert_eq!(kinds.len(), rows_open);
    let b = |i: usize| dnd.row_bounds("anim-table", i).unwrap();
    s.screenshot("both-open");
    for (name, want) in [("plain", (false, false)), ("child", (false, true))] {
        let row = kinds.iter().position(|(_, p, c)| (*p, *c) == want).expect("a row of this kind");
        let (x, y) = (f32::from(b(row).origin.x) + 120., f32::from(b(row).origin.y) + 20.);
        step(&mut s, mouse_move(x, y, false));
        step(&mut s, mouse_down(x, y));
        let open = open_count(&mut s);
        let rows = shown(&mut s);
        s.screenshot(&format!("{name}-row-pressed"));
        if name == "plain" {
            assert_eq!(open, 0, "pressing a plain row folds every open text box");
        } else {
            assert_eq!(open, all_open, "pressing a paragraph's row folds nothing: its siblings have no descendants");
        }
        if name == "plain" {
            assert!(rows < rows_open, "{name}: {rows} rows after the press, {rows_open} before");
        } else {
            assert_eq!(rows, rows_open, "{name}: no row folds");
        }
        step(&mut s, mouse_move(x, y + 10., true));
        s.screenshot(&format!("{name}-row-dragged"));
        step(&mut s, mouse_up(x, y + 10.));
        assert_eq!(open_count(&mut s), all_open, "{name}: opened again after the button goes up");
        // Escape too.
        step(&mut s, mouse_move(x, y, false));
        step(&mut s, mouse_down(x, y));
        step(&mut s, mouse_move(x, y + 10., true));
        s.key("escape");
        s.settle();
        assert_eq!(open_count(&mut s), all_open, "{name}: opened again after Escape");
    }
}

fn text_box_runs_are_rows(a: &[ibis_doc_model::Animation]) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut i = 0;
    while i < a.len() {
        let r = crate::animation_ui::paragraph_run(a, i);
        out.push(r);
        i = r.1 + 1;
    }
    out
}

fn swapped_children_keep_the_parent() {
    if !present() {
        return;
    }
    let mut s = open_page();
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    let before: Vec<ibis_doc_model::Animation> = s.with(|e, _| e.slide().animations.clone());
    let runs_before = text_box_runs_are_rows(&before);
    // The first text box row with two or more children: display row of its first child.
    let mut row = 0;
    let (mut first, mut last) = (0, 0);
    for &(f, l) in &runs_before {
        if l > f {
            (first, last) = (f, l);
            break;
        }
        row += 1;
    }
    let child = row + 1; // the display row of the first paragraph's effect
    let b = |i: usize| dnd.row_bounds("anim-table", i).unwrap_or_else(|| panic!("no row {i}"));
    let rows_before = shown(&mut s);
    let height = f32::from(b(child + 1).origin.y - b(child).origin.y);
    let (x, y) = (f32::from(b(child).origin.x) + 120., f32::from(b(child).origin.y) + 20.);
    s.screenshot("swap-0-at-rest");
    step(&mut s, mouse_move(x, y, false));
    step(&mut s, mouse_down(x, y));
    let mut frame = 0;
    // Down by 1.2 rows in frames; at every frame the parent row (display row `row`) is there and no row is lost.
    let mut d = 0.;
    while d < height * 1.2 {
        d += 6.;
        step(&mut s, mouse_move(x, y + d, true));
        frame += 1;
        let n = shown(&mut s);
        // Folded for the drag, except the child's own text box: the other text box is a row, this one has its parent and children.
        assert!(n >= 1 + (last - first + 1), "frame {frame}: {n} rows");
        assert!(dnd.row_bounds("anim-table", row).is_some_and(|r| f32::from(r.size.height) > 10.), "frame {frame}: the parent row {row} is not there");
        if frame % 3 == 0 {
            s.screenshot(&format!("swap-{frame:02}-dragging"));
        }
    }
    step(&mut s, mouse_up(x, y + d));
    s.screenshot("swap-after-drop");
    let after: Vec<ibis_doc_model::Animation> = s.with(|e, _| e.slide().animations.clone());
    let order = |v: &[ibis_doc_model::Animation]| v.iter().map(|a| a.paragraph.map(|p| p.0)).collect::<Vec<_>>();
    assert_ne!(order(&before), order(&after), "the drop changed the order");
    assert_eq!(text_box_runs_are_rows(&after), runs_before, "the text box's row is still one row of the same effects: {:?} became {:?}", order(&before), order(&after));
    let key = |a: &ibis_doc_model::Animation| format!("{a:?}");
    let (mut kb, mut ka): (Vec<String>, Vec<String>) = (before.iter().map(key).collect(), after.iter().map(key).collect());
    kb.sort();
    ka.sort();
    assert_eq!(kb, ka, "no effect lost");
    assert_eq!(shown(&mut s), rows_before, "as many rows after the drop as before");
    assert!(dnd.row_bounds("anim-table", row).is_some());
    assert_eq!(open_count(&mut s), 2, "both text boxes are open again, the swapped one by the same key");
}

fn runs_in_any_order() {
    use ibis_doc_model::{Animation, ShapeId};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let _ = &mut s;
    let (shape, other) = (ShapeId(1), ShapeId(2));
    let e = |sh: ShapeId, p: Option<u32>| -> Animation {
        let mut a = super::fix_20261106_animation::effect_of(sh, p, 0);
        a.shape = sh;
        a
    };
    let v = vec![e(shape, Some(1)), e(shape, Some(0)), e(shape, Some(2)), e(other, None), e(shape, Some(0)), e(shape, Some(1))];
    assert_eq!(text_box_runs_are_rows(&v), vec![(0, 2), (3, 3), (4, 5)], "swapped paragraphs are one row; the same paragraphs again are another");
    let keys = crate::animation_ui::box_keys(&v);
    assert_eq!(keys[0], keys[2]);
    assert_eq!(keys[0].1, 0, "the key is the smallest paragraph");
    let sorted = vec![e(shape, Some(0)), e(shape, Some(1)), e(shape, Some(2))];
    assert_eq!(crate::animation_ui::box_keys(&sorted)[0], keys[0], "swapping does not change the key");
}

/// The panels opened as dialogs are shown while the animation pane (a pane of the sidebar) is open: the right pane gives the dialog's
/// panel, not the pane's.
fn dialogs_before_the_pane() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1280., 720.);
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    s.update(|e, _, cx| e.toggle_animation_pane(cx));
    s.settle();
    let modal = |s: &mut Scenario| s.with(|e, _| e.dialog_is_modal());
    assert!(!modal(&mut s), "the animation pane alone is not a dialog");
    s.update(|e, w, cx| e.open_theme_panel(w, cx));
    s.settle();
    assert!(modal(&mut s), "the theme panel is shown (as a dialog) over the open animation pane");
    s.update(|e, w, cx| e.close_theme_panel(w, cx));
    s.settle();
    assert!(!modal(&mut s));
    s.update(|e, w, cx| e.open_slide_size_panel(w, cx));
    s.settle();
    assert!(modal(&mut s), "the slide size panel is shown over the open animation pane");
    s.update(|e, w, cx| e.close_slide_size_panel(w, cx));
    s.settle();
    s.click_shape(0);
    s.update(|e, w, cx| e.slide_open_link_panel(w, cx));
    s.settle();
    assert!(modal(&mut s), "the link panel is shown over the open animation pane");
    s.update(|e, w, cx| e.slide_close_link_panel(w, cx));
}

/// Where the held row stands on the screen: its slot's top plus how far it is lifted.
fn held_top(s: &mut Scenario) -> Option<(usize, f32, usize)> {
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    let n = shown(s);
    (0..n).find_map(|i| dnd.lift_offset("anim-table", i).map(|off| (i, f32::from(dnd.row_bounds("anim-table", i).unwrap().origin.y) + off, n)))
}

/// Grabbing a row of page 3 (both text boxes open) folds only its siblings and their descendants, and the grabbed row does not move
/// on the screen: the rows around it come to it. It is kept between the top of the first row and the bottom of the last row of the folded list.
fn held_row_stays() {
    if !present() {
        return;
    }
    let mut s = open_page();
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    // display rows: (effect index, parent?, child?)
    let kinds: Vec<(bool, bool)> = s.with(|e, _| {
        let a = &e.slide().animations;
        let mut out = vec![];
        let mut i = 0;
        while i < a.len() {
            let (f, l) = crate::animation_ui::paragraph_run(a, i);
            if l > f {
                out.push((true, false));
                out.extend((f..=l).map(|_| (false, true)));
            } else {
                out.push((false, false));
            }
            i = l + 1;
        }
        out
    });
    let parents: Vec<usize> = kinds.iter().enumerate().filter(|(_, k)| k.0).map(|(i, _)| i).collect();
    assert!(parents.len() >= 2);
    let second_parent = parents[1];
    let rows_open = kinds.len();
    // (name, display row to grab)
    let cases = [("parent2", second_parent), ("parent2-child", second_parent + 1)];
    let all_open = open_count(&mut s);
    for (name, row) in cases {
        let b = dnd.row_bounds("anim-table", row).unwrap();
        let (x, y) = (f32::from(b.origin.x) + 120., f32::from(b.origin.y) + 20.);
        let top0 = f32::from(b.origin.y);
        step(&mut s, mouse_move(x, y, false));
        s.screenshot(&format!("{name}-0-before"));
        step(&mut s, mouse_down(x, y));
        s.screenshot(&format!("{name}-1-pressed"));
        let (i1, top1, n1) = held_top(&mut s).expect("the row is held");
        eprintln!("{name}: row {row} -> {i1}, rows {rows_open} -> {n1}, y {top0} -> {top1}");
        // The folded list's first row's top and last row's bottom limit it (a row that stood below the folded list goes to its last row).
        let bounds_of = |s: &mut Scenario| {
            let first = dnd.row_bounds("anim-table", 0).map(|r| f32::from(r.origin.y)).unwrap();
            let (last, height) = (0..shown(s)).filter_map(|i| dnd.row_bounds("anim-table", i)).map(|r| (f32::from(r.origin.y + r.size.height), f32::from(r.size.height))).fold((0., 0.), |a, b| if b.0 > a.0 { b } else { a });
            (first, last, height)
        };
        let (first, last, height) = bounds_of(&mut s);
        let want = top0.clamp(first, (last - height).max(first));
        assert!((top1 - want).abs() < 0.6, "{name}: the held row jumped from {top0} to {top1} (it may only go to {want})");
        if name == "parent2-child" {
            assert!((top1 - top0).abs() < 0.6, "{name}: nothing folds, the row stays");
        }
        if name == "parent2" {
            assert!(n1 < rows_open, "{name}: the siblings' descendants fold");
        } else {
            assert_eq!(n1, rows_open, "{name}: a paragraph's siblings have no descendants: nothing folds");
            assert_eq!(open_count(&mut s), all_open);
        }
        // Dragged up a little: still the same offset from the pointer.
        step(&mut s, mouse_move(x, y - 8., true));
        s.screenshot(&format!("{name}-2-dragged"));
        let (_, top2, _) = held_top(&mut s).expect("held");
        // A paragraph's row (the first of its text box here) stays within its text box's paragraphs.
        let up_limit = if name == "parent2-child" { top0 } else { first };
        assert!((top2 - (top0 - 8.).clamp(up_limit, (last - height).max(up_limit))).abs() < 0.6, "{name}: follows the pointer (kept in the list): {top2} vs {}", top0 - 8.);
        // Far up and far down: kept between the first row's top and the last row's bottom of the folded list.
        step(&mut s, mouse_move(x, y - 400., true));
        s.screenshot(&format!("{name}-3-far-up"));
        let (_, top, _) = held_top(&mut s).expect("held");
        assert!(top >= up_limit - 0.6, "{name}: above its range: {top} < {up_limit}");
        step(&mut s, mouse_move(x, y + 600., true));
        s.screenshot(&format!("{name}-4-far-down"));
        let (_, top, _) = held_top(&mut s).expect("held");
        assert!(top + height <= last + 0.6, "{name}: below the list: {} > {last}", top + height);
        step(&mut s, mouse_move(x, y, true));
        s.key("escape");
        s.settle();
        assert_eq!(open_count(&mut s), all_open, "{name}: opened again");
    }
}
