// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #200: copying a selected shape in the dark appearance turned the chrome light (the picture put on the clipboard is drawn in a headless app of its own,
//! whose start followed that app's light window appearance and set the process-wide appearance).

use ibis_doc_model::FlowPos;

use crate::shell::scenario::Scenario;
use crate::ext::*;
use super::scenarios::DocSteps;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("docx::issue200_copying_a_shape_keeps_the_dark_appearance", copying_a_shape_keeps_the_dark_appearance)]
}

fn copying_a_shape_keeps_the_dark_appearance() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let (_, id, _) = super::scenarios::text_paragraph(&mut s, 10);
    s.click_text(FlowPos::new(id, 3));
    s.menu("shape.rightArrow");
    s.click_text(FlowPos::new(id, 3));
    assert!(s.with(|e, _| e.doc_state().is_some_and(|st| st.shape_selected)), "the shape is selected");
    s.update(|_, _, cx| crate::shell::palette::system_dark(cx));
    s.settle();
    assert!(crate::shell::palette::is_dark());
    s.key("cmd-c");
    s.settle();
    assert!(crate::shell::palette::is_dark(), "the appearance is still dark after the copy");
    assert!(s.with_app(|cx| gpui_kit::component::theme::Theme::global(cx).mode.is_dark()), "the components' theme is still dark");
    s.update(|_, _, cx| crate::shell::palette::release(cx));
}
