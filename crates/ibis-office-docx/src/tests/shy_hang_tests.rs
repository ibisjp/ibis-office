// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A justified paragraph with a soft hyphen (U+00AD) where the line has 1 pt left was never laid out (GitHub issue #161): the
//! punctuation compression found the line over by the soft hyphen's held-back width, squeezed nothing and went round again.

use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("docx::soft_hyphen_at_the_end_of_a_justified_line_is_laid_out", a_soft_hyphen_at_the_end_of_a_justified_line_is_laid_out)]
}

fn a_soft_hyphen_at_the_end_of_a_justified_line_is_laid_out() {
    // The scenario runs on the main thread, so a watchdog ends the run where the loop of the issue would hang it.
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = done.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(20));
        if !flag.load(std::sync::atomic::Ordering::SeqCst) {
            eprintln!("docx::soft_hyphen_at_the_end_of_a_justified_line_is_laid_out: not laid out within 20 s (GitHub issue #161)");
            std::process::exit(1);
        }
    });
    let mut s = Scenario::open("test/editing/shy-justify-hang.docx");
    let pages = s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
    done.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(pages.pages.len(), 1);
    // The soft hyphen ends the first line as a hyphen: the 46 kana and the hyphen.
    let first: String = pages.pages[0].items.iter().filter_map(|it| if let super::flow_layout::Item::Text { text, .. } = it { Some(text.to_string()) } else { None }).next().unwrap_or_default();
    assert!(first.starts_with("\u{3042}\u{3044}\u{3046}\u{3048}\u{304A}") && first.chars().filter(|c| *c != '-').count() == 46, "{first}");
}
