// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Every scroll container stretches past its end and springs back for a finger's scroll, as `UIScrollView` does (K0065, test design Q15; `tasks/doc/test-design/K0065.md`).
//!
//! The stretch is in gpui's `Div` (`forks/gpui-pre/src/elements/div.rs`, `div_bounce.rs`): every element with `overflow_scroll()`, `overflow_x_scroll()` or `overflow_y_scroll()`
//! has it, so none is forgotten (the rule this test keeps is that the only way to be without it is the explicit `.no_bounce()`). `.no_bounce()` is for the scroll area inside an outer
//! element that stretches (gpui-base's `ScrollBounce`, which moves more than the area: the headers and frozen panes of a sheet, the bars of K0061): a file that uses it must also use
//! `ScrollBounce`, and a `ScrollBounce` must have the `.no_bounce()` of its area (else the two would stretch, twice).

use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests" || n == "target") {
                continue;
            }
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") && !path.file_name().is_some_and(|n| n.to_string_lossy().contains("scenarios") || n.to_string_lossy().ends_with("tests.rs")) {
            out.push(path);
        }
    }
}

fn sources() -> Vec<(String, String)> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(crates).unwrap().flatten() {
        let src = entry.path().join("src");
        if src.is_dir() {
            walk(&src, &mut files);
        }
    }
    files.into_iter().map(|f| (f.strip_prefix(crates).unwrap().to_string_lossy().replace('\\', "/"), std::fs::read_to_string(&f).unwrap())).collect()
}

/// A scroll container has the stretch unless it says `.no_bounce()`, and that only inside a `ScrollBounce`.
#[test]
fn every_scroll_container_stretches_at_its_end() {
    // The stretch is there by default: gpui's Div has it for a finger's scroll, and `no_bounce` is off unless said.
    let div = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../forks/gpui-pre/src/elements/div.rs")).unwrap();
    assert!(div.contains("event.touch_fingers != 0") && div.contains("pub(crate) no_bounce: bool"), "gpui's Div stretches a finger's scroll, unless no_bounce");
    assert!(div.contains("self.bounce = (!self.no_bounce)"), "by default, for the elements with a tracked scroll handle or an id");
    let mut wrong = Vec::new();
    for (name, text) in sources() {
        let (no_bounce, wrappers) = (text.matches(".no_bounce()").count(), text.matches("ScrollBounce::new").count());
        if no_bounce > 0 && wrappers == 0 {
            wrong.push(format!("{name}: .no_bounce() with no ScrollBounce around the area (it would not stretch at all)"));
        }
        if wrappers > 0 && no_bounce == 0 && text.contains("overflow") {
            wrong.push(format!("{name}: a ScrollBounce whose area has no .no_bounce() (it would stretch twice)"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
