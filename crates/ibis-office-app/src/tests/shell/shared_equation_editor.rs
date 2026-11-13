// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The equation editor both formats share, driven for a document and for a presentation (it names both formats, so it is
//! the app's, not the document crate's: tasks/doc/crate-split-plan.md, 15.2).

use super::Scenario;
use crate::docx::ext::*;
use crate::docx::scenarios::DocSteps;
use crate::pptx::ext::*;

const REPORT: &str = "test/editing/docx-report.docx";

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("docx::ui_review_shared_equation_editor", ui_review_shared_equation_editor)]
}

/// Both format adapters use the same rendered equation controls and Enter lifecycle.
fn ui_review_shared_equation_editor() {
    use crate::shell::scenario::button_probe as buttons;
    use crate::shell::math_editor::MathFormat;
    use gpui_kit::component::input::InputEvent;
    let _probe = buttons::Probe::new();
    for (path, document) in [(REPORT, true), ("test/editing/pptx-math.pptx", false)] {
        let mut s = Scenario::open(path);
        s.size(1200., 900.);
        if document {
            let doc = s.document();
            let id = doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && !p.text().contains('\u{fffc}') && p.list.is_none() { Some(p.id) } else { None }).expect("a paragraph of text");
            s.click_text(ibis_doc_model::FlowPos::new(id, 0));
            s.update(|e, w, cx| e.open_docx_math_panel(w, cx));
        } else {
            s.update(|e, w, cx| e.open_pptx_math_panel(w, cx));
        }
        s.settle();
        buttons::click(&mut s, "math-editor-latex");
        assert_eq!(s.with(|e, _| if document { e.doc_state().unwrap().docx_math.as_ref().unwrap().format } else { e.pptx().unwrap().math_panel.as_ref().unwrap().format }), MathFormat::Latex);
        let input = s.with(|e, _| if document { e.doc_state().unwrap().docx_math.as_ref().unwrap().input.clone() } else { e.pptx().unwrap().math_panel.as_ref().unwrap().input.clone() });
        s.update(|_, w, cx| input.update(cx, |i, cx| { i.set_value(r"\frac{a}{b}", w, cx); cx.emit(InputEvent::Change); i.focus(w, cx); }));
        s.settle();
        let preview = buttons::bounds("math-editor-preview").expect("shared typeset preview is rendered");
        // The measured canvas excludes the shared preview frame's 1px top/bottom borders.
        assert!(preview.size.height + gpui_kit::px(2.) >= gpui_kit::px(80.));
        s.key("enter");
        s.settle();
        assert!(s.with(|e, _| if document { e.doc_state().unwrap().docx_math.is_none() } else { e.pptx().unwrap().math_panel.is_none() }), "Enter inserts and closes the equation dialog");
        if document {
            assert!(s.with(|e, _| e.docx_math_at_caret().is_some()));
        } else {
            assert!(s.with(|e, _| e.selected_math().is_some()));
        }
    }
}
