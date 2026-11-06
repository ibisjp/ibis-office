// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Form controls of the Excel 2007 form: a worksheet with no `controls` element, the controls only in its VML
//! drawing (`x:ClientData ObjectType=...`) and `ctrlProp` parts (GitHub issue #95).

use std::sync::Arc;

use ibis_doc_model::{ControlKind, FormControl, Shape, ShapeKind};
use ibis_pptx::SaveOptions;

fn fixture() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-form-controls-2007.xlsx")).expect("read xlsx-form-controls-2007.xlsx")
}

fn controls(p: &ibis_doc_model::Presentation) -> Vec<(&Shape, &FormControl, bool)> {
    let slide = &p.slides[0];
    slide
        .shapes
        .iter()
        .filter_map(|s| match &s.kind {
            ShapeKind::Control(c) => Some((s, c.as_ref(), slide.hidden_shapes.contains(&s.id))),
            _ => None,
        })
        .collect()
}

fn zip_text(bytes: &[u8], name: &str) -> String {
    String::from_utf8(ibis_ooxml::Package::open(bytes.to_vec()).unwrap().read(name).expect("part")).unwrap()
}

#[test]
fn reads_the_controls_that_only_the_vml_describes() {
    let loaded = ibis_pptx::open(fixture()).expect("open");
    let found = controls(&loaded.presentation);
    let kinds: Vec<ControlKind> = found.iter().map(|(_, c, _)| c.kind).collect();
    assert_eq!(kinds, [ControlKind::Radio, ControlKind::Radio, ControlKind::GroupBox, ControlKind::CheckBox, ControlKind::Drop]);
    // The text, the linked cells and the state come from the VML and the ctrlProp parts.
    let (_, radio, _) = found[0];
    assert_eq!(radio.text, "Large");
    assert_eq!(radio.link.as_deref(), Some("$B$2"));
    assert!(radio.first_button);
    assert!(!radio.part.is_empty(), "the ctrlProp lined up with the shape");
    let (_, drop, _) = found[4];
    assert_eq!(drop.range.as_deref(), Some("$D$1:$D$3"));
    // The group box is hidden (`visibility:hidden`); the others are shown.
    assert_eq!(found.iter().map(|(_, _, h)| *h).collect::<Vec<_>>(), [false, false, true, false, false]);
    // The place: the style's `margin-left`, `margin-top`, `width` and `height` in points.
    let frame = found[0].0.frame;
    assert_eq!((frame.x, frame.y, frame.width, frame.height), (96 * 12700, 60 * 12700, 48 * 12700, 15 * 12700));
    // The linked cells decide the state: B2 = 1 chooses the first option button; B5 = 2 the second item.
    assert_eq!(found[0].1.checked, 1);
    assert_eq!(found[4].1.value, 2);
}

#[test]
fn a_changed_state_is_written_to_the_vml_and_the_ctrl_prop_and_the_rest_is_kept() {
    let mut loaded = ibis_pptx::open(fixture()).expect("open");
    let id = controls(&loaded.presentation)[1].0.id;
    {
        let slide = loaded.presentation.slides[0].shapes.iter_mut().find(|s| s.id == id).unwrap();
        let ShapeKind::Control(c) = &mut slide.kind else { unreachable!() };
        let mut next = (**c).clone();
        next.checked = 1;
        *c = Arc::new(next);
    }
    let saved = loaded.source.save(&loaded.presentation, SaveOptions::default()).expect("save");
    let vml = zip_text(&saved, "xl/drawings/vmlDrawing1.vml");
    assert!(vml.contains("<x:Checked>1</x:Checked>"), "the second option button's VML says it is chosen:\n{vml}");
    assert!(zip_text(&saved, "xl/ctrlProps/ctrlProp2.xml").contains("checked=\"Checked\""));
    // Reading the saved file gives the same controls; the group box is still hidden.
    let again = ibis_pptx::open(saved).expect("reopen");
    let found = controls(&again.presentation);
    assert_eq!(found.len(), 5);
    assert_eq!(found[1].1.checked, 1);
    assert!(found[2].2);
}

#[test]
fn deleting_a_control_takes_its_shape_and_ctrl_prop_with_it() {
    let mut loaded = ibis_pptx::open(fixture()).expect("open");
    let slide = &mut loaded.presentation.slides[0];
    let at = slide.shapes.iter().position(|s| matches!(&s.kind, ShapeKind::Control(c) if c.kind == ControlKind::CheckBox)).unwrap();
    slide.shapes.remove(at);
    let saved = loaded.source.save(&loaded.presentation, SaveOptions::default()).expect("save");
    assert!(!zip_text(&saved, "xl/drawings/vmlDrawing1.vml").contains("_x0000_s1028"));
    let again = ibis_pptx::open(saved).expect("reopen");
    assert_eq!(controls(&again.presentation).len(), 4);
}
