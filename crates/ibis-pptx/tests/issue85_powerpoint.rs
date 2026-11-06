// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue 85, against PowerPoint: a layout applied to a slide that "blank" emptied gives it the shapes PowerPoint gives it.
//!
//! `test/github-issue/85/layouts-4x3.pptx` is a deck of PowerPoint's own 11 layouts. `layouts-4x3-powerpoint.tsv` is what
//! PowerPoint (Windows, Microsoft 365 16.0.20527, COM) leaves on a slide after `CustomLayout = Blank` and then
//! `CustomLayout = <layout>`: per layout, the name, the shapes after Blank, the shapes after the layout, and each shape as
//! `type,left,top,width,height` (EMU). The files are not in the repository (`test/github-issue/` is not committed); the test
//! is skipped without them.
use ibis_doc_model::{Layout, LayoutId};

fn kind_number(kind: Option<&str>) -> i32 {
    // PowerPoint's PpPlaceholderType.
    match kind {
        Some("title") => 1,
        Some("ctrTitle") => 3,
        None | Some("body") => 2,
        Some("obj") => 7,
        Some("subTitle") => 4,
        Some("pic") => 18,
        Some("chart") => 8,
        Some("tbl") => 12,
        Some("media") => 16,
        Some("dgm") => 13,
        Some("clipArt") => 10,
        Some("vertTitle") => 5,
        _ => -1,
    }
}

fn defined(layout: &Layout) -> usize {
    layout.placeholders.iter().filter(|p| !matches!(p.placeholder.kind.as_deref(), Some("dt" | "ftr" | "sldNum" | "hdr"))).count()
}

#[test]
fn layouts_applied_after_blank_give_the_shapes_powerpoint_gives() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/github-issue/85");
    let (Ok(bytes), Ok(truth)) = (std::fs::read(dir.join("layouts-4x3.pptx")), std::fs::read_to_string(dir.join("layouts-4x3-powerpoint.tsv"))) else {
        eprintln!("skipped: test/github-issue/85 has no deck");
        return;
    };
    let opened = ibis_pptx::open(bytes).unwrap();
    let model = opened.presentation.clone();
    let slide = model.slides[0].id;
    let blank = model.layouts.iter().find(|l| defined(l) == 0).expect("blank").id;
    let mut next = 900_000u64;
    let mut checked = 0;
    for line in truth.lines().filter(|l| !l.trim().is_empty()) {
        let cols: Vec<&str> = line.split('\t').collect();
        let (name, after_blank, after, shapes) = (cols[0], cols[1].parse::<usize>().unwrap(), cols[2].parse::<usize>().unwrap(), cols.get(3).copied().unwrap_or(""));
        let layout: &Layout = model.layouts.iter().find(|l| l.name == name).unwrap_or_else(|| panic!("layout {name}"));
        let mut m = model.clone();
        let emptied = opened.source.with_layout(&m, slide, blank).unwrap();
        assert_eq!(emptied.shapes.len(), after_blank, "{name}: after Blank");
        m.slides[0] = emptied;
        let mut new_id = || {
            next += 1;
            ibis_doc_model::ShapeId(next)
        };
        let applied = opened.source.with_layout_ids(&m, slide, LayoutId(layout.id.0), &mut new_id).unwrap();
        assert_eq!(applied.shapes.len(), after, "{name}: shapes after the layout");
        let want: Vec<(i32, [i64; 4])> = shapes
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let v: Vec<i64> = s.split(',').map(|x| x.parse().unwrap()).collect();
                (v[0] as i32, [v[1], v[2], v[3], v[4]])
            })
            .collect();
        let got: Vec<(i32, [i64; 4])> = applied
            .shapes
            .iter()
            .map(|s| {
                let ph = applied.placeholders.get(&s.id).expect("a placeholder");
                (kind_number(ph.placeholder.kind.as_deref()), [s.frame.x, s.frame.y, s.frame.width, s.frame.height])
            })
            .collect();
        eprintln!("{name}: want {want:?}\n  got {got:?}");
        assert_eq!(got.len(), want.len(), "{name}");
        for (g, w) in got.iter().zip(&want) {
            // PowerPoint numbers the body, object and vertical-text placeholders by what they hold; the frames are what matter.
            assert_eq!(g.1, w.1, "{name}: frame of the {}th shape", got.iter().position(|x| x == g).unwrap());
        }
        checked += 1;
    }
    assert!(checked >= 10);
}
