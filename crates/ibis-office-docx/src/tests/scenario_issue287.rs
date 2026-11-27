// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #287 and #289: the grid lines follow the text direction (across writing has lines across from the top, vertical writing lines down from the right
//! edge) and the marks at the corners of the text area stay at the corners of the body when the text direction is vertical, whichever way the paper
//! is turned.

use crate::ext::*;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{CellDirection, EMU_PER_POINT};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue287_gridlines_follow_the_text_direction", gridlines_follow_direction),
        ("docx::issue289_text_boundary_corners_are_the_bodys_corners_when_vertical", boundary_corners),
    ]
}

const REPORT: &str = "test/editing/docx-report.docx";
const MARGINS_PT: [i64; 4] = [60, 40, 50, 70]; // left, right, top, bottom

/// The paper `portrait` or landscape, the text `vertical` or across, margins as above and a gutter of `gutter_pt` on the left (`at_top`: on the top).
fn set(s: &mut Scenario, vertical: bool, portrait: bool, gutter_pt: i64, at_top: bool) -> ibis_doc_model::PageSetup {
    s.update(|e, _, cx| {
        e.set_page(
            |p| {
                let (a, b) = (p.size.width.min(p.size.height), p.size.width.max(p.size.height));
                p.size = if portrait { ibis_doc_model::EmuSize { width: a, height: b } } else { ibis_doc_model::EmuSize { width: b, height: a } };
                p.direction = if vertical { CellDirection::TbRl } else { CellDirection::LrTb };
                [p.left, p.right, p.top, p.bottom] = MARGINS_PT.map(|m| m * EMU_PER_POINT);
                p.gutter = gutter_pt * EMU_PER_POINT;
                p.gutter_at_top = at_top;
                p.line_pitch = Some(18 * EMU_PER_POINT);
            },
            cx,
        )
    });
    s.settle();
    s.with(|e, _| e.caret_page().expect("a page"))
}

fn gridlines_follow_direction() {
    let mut s = Scenario::open(REPORT);
    s.size(1200.0, 900.0);
    for (vertical, portrait) in [(false, true), (false, false), (true, true), (true, false)] {
        let page = set(&mut s, vertical, portrait, 0, false);
        let (w, h) = (page.size.width as f32 / EMU_PER_POINT as f32, page.size.height as f32 / EMU_PER_POINT as f32);
        // One pixel a point.
        let g = crate::doc_ui::gridline_stops(Some(page), w, h, 1.0 / EMU_PER_POINT as f32).expect("lines");
        let label = format!("vertical {vertical}, portrait {portrait}");
        assert_eq!(g.vertical, vertical, "{label}: only the lines of the text direction");
        let [l, t, r, b] = g.area;
        let m = MARGINS_PT.map(|m| m as f32);
        if vertical {
            // From the right edge of the text area (the right margin) to the left, a line pitch apart.
            assert!((r - (w - m[1])).abs() < 0.01 && (l - m[0]).abs() < 0.01, "{label}: {:?}", g.area);
            assert!((g.stops[0] - r).abs() < 0.01 && (g.stops[0] - g.stops[1] - 18.0).abs() < 0.01, "{label}: {:?}", &g.stops[..2]);
            assert!(g.stops.iter().all(|x| *x >= l - 0.5 && *x <= r + 0.01), "{label}");
        } else {
            assert!((t - m[2]).abs() < 0.01 && (b - (h - m[3])).abs() < 0.01 && (l - m[0]).abs() < 0.01, "{label}: {:?}", g.area);
            assert!((g.stops[0] - t).abs() < 0.01 && (g.stops[1] - g.stops[0] - 18.0).abs() < 0.01, "{label}");
            assert!(g.stops.iter().all(|y| *y >= t - 0.01 && *y <= b + 0.5), "{label}");
        }
    }
}

fn boundary_corners() {
    // A blank document: no header or footer pushes the body in from the margins.
    let mut s = Scenario::new_document(ibis_formats::Kind::Document);
    s.size(1200.0, 900.0);
    // The body's corners are the margins' corners, whichever way the text and the paper go (#289: they were turned wrongly, outside the paper), and the gutter is
    // where Word puts it (Windows, #287: on the left of across writing, on the right of vertical writing, on the top with 上とじ), and the area of the grid lines (#287)
    // is the body's.
    for (gutter, at_top) in [(0, false), (20, false), (20, true)] {
        for (vertical, portrait) in [(false, true), (false, false), (true, true), (true, false)] {
            let page = set(&mut s, vertical, portrait, gutter, at_top);
            let area = s.with(|e, _| {
                let doc = e.flow().expect("a document").clone();
                crate::flow_layout::pages(&doc, &e.resources).pages[0].text_area
            });
            let (w, h) = (page.size.width as f32 / EMU_PER_POINT as f32, page.size.height as f32 / EMU_PER_POINT as f32);
            let m = MARGINS_PT.map(|m| m as f32);
            let g = gutter as f32;
            let want = [m[0] + if !vertical && !at_top { g } else { 0.0 }, m[2] + if at_top { g } else { 0.0 }, w - m[1] - if vertical && !at_top { g } else { 0.0 }, h - m[3]];
            let label = format!("gutter {gutter} at top {at_top}, vertical {vertical}, portrait {portrait}: {area:?} want {want:?}");
            assert!(area.iter().zip(want).all(|(a, b)| (a - b).abs() <= 0.5), "{label}");
            let grid = crate::doc_ui::gridline_stops(Some(page), w, h, 1.0 / EMU_PER_POINT as f32).expect("lines");
            assert!(area.iter().zip(grid.area).all(|(a, b)| (a - b).abs() <= 0.5), "{label}: the grid's area {:?} is the body's", grid.area);
        }
    }
}
