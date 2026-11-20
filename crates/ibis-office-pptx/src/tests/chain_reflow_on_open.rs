// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A presentation with linked text boxes whose text is all in the first box (the press release's `text-chain-wrap-hv2`, its second box's text moved into the first) is flowed
//! through the chain when it is opened: the text that does not fit the first box goes on in the second, instead of running out of the first box's bottom.
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("pptx::chain_a_text_all_in_the_first_box_flows_on_into_the_next_when_the_file_is_opened", all_in_the_first_box as fn())]
}

fn all_in_the_first_box() {
    let mut s = Scenario::open("test/editing/text-chain-all-in-first.pptx");
    let (first, second, whole) = s.with(|e, _| {
        let chain = e.slide().text_chains.first().expect("a chain").clone();
        let text = |id| e.slide().shape(id).and_then(|sh| sh.text_body()).map(|b| b.text()).unwrap_or_default();
        (text(chain.boxes[0]), text(chain.boxes[1]), chain.boxes.len())
    });
    assert_eq!(whole, 2);
    assert!(!second.trim().is_empty(), "the text that does not fit goes on in the second box: first {first:?}");
    assert!(!first.trim().is_empty());
}

/// Moved and saved, the chain of a file with the same chain number in both boxes is the same chain when it is read again (the press release's hv2, saved by a person, had the
/// two boxes in two chains: `chain="1"` and `chain="0"`).
pub(crate) fn all_saved() -> Vec<(&'static str, fn())> {
    vec![("pptx::chain_a_moved_and_saved_chain_is_read_again_as_one", moved_and_saved as fn())]
}

fn moved_and_saved() {
    let mut s = Scenario::open("test/editing/text-chain-all-in-first.pptx");
    let before = s.with(|e, _| e.slide().text_chains.clone());
    assert_eq!(before.len(), 1, "one chain of two boxes: {before:?}");
    assert_eq!(before[0].boxes.len(), 2);
    s.click_shape(1);
    s.key("right");
    s.save_as("chain-moved.pptx");
    let again = s.reopen();
    let slide = &again.slides[0];
    assert_eq!(slide.text_chains.len(), 1, "the chain is kept as one: {:?}", slide.text_chains);
    assert_eq!(slide.text_chains[0].boxes.len(), 2);
}
