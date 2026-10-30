// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
use super::SheetSteps;
use ibis_doc_model::{CellValue,CellRange,Validation,Command,SheetCommand};

pub(super) fn run() {
    let mut s=super::new_sheet();
    s.size(1200.,800.);
    s.update(|e,_,cx| {
        let rule=Validation {ranges:vec![CellRange{first_row:0,last_row:19,first_column:0,last_column:0}],kind:"list".into(),operator:"between".into(),formula1:Some("\"Apple,Meta,Alphabet,Microsoft,Amazon\"".into()),formula2:None,allow_blank:true,dropdown:true,prompt:None,error:None};
        e.run_sheet(vec![Command::Sheet(SheetCommand::SetValidation{slide:e.current_slide,ranges:rule.ranges.clone(),rule:Some(rule)})],cx);
    });
    s.click_cell("A1");
    s.key("alt-down");
    assert!(s.with(|e,_| e.wb().validation_choices.is_some()));
    assert_eq!(s.with(|e,_|e.filtered_validation_choices()).len(),5);
    s.settle();
    s.with(|e,_| {
        let inner=e.wb().validation_choices.as_ref().unwrap().bounds.get().unwrap();
        // The absolute canvas measures the padding box, excluding the 1px borders.
        let b=gpui_kit::Bounds::new(inner.origin-gpui_kit::point(gpui_kit::px(1.),gpui_kit::px(1.)),inner.size+gpui_kit::size(gpui_kit::px(2.),gpui_kit::px(2.)));
        let cell=e.validation_cell_bounds(0,0).unwrap();
        assert!((b.left()-cell.left()).abs()<=gpui_kit::px(1.1),"popup={b:?}, cell={cell:?}");
        assert!((b.top()-cell.bottom()).abs()<=gpui_kit::px(1.1),"popup={b:?}, cell={cell:?}");
        assert!(b.size.width+gpui_kit::px(1.1)>=cell.size.width+gpui_kit::px(f32::from(cell.size.height).clamp(12.,20.)+1.));
        assert!(b.size.width<=gpui_kit::px(601.));
    });
    s.key("tab");
    assert_eq!(s.value("A1"),CellValue::Text("Apple".into()));
    assert!(s.with(|e,_|e.wb().validation_choices.is_none()));
    s.click_cell("A2");
    s.type_text("A");
    assert!(s.with(|e,_|e.wb().validation_choices.is_some()),"typing A opens the list without Option+Down");
    assert_eq!(s.with(|e,_|e.filtered_validation_choices()),vec!["Apple","Alphabet","Amazon"]);
    s.key("down");
    s.key("tab");
    assert_eq!(s.value("A2"),CellValue::Text("Alphabet".into()));
    for (cell,key) in [("A3","enter"),("A4","tab"),("A5","click")] {
        s.click_cell(cell);
        let before=s.with(|e,_|e.current_cell());
        s.type_text("H");
        if key=="click" {s.click_cell("B7");} else {s.key(key);}
        assert!(s.with(|e,_|e.panel_message.as_ref().is_some_and(|t|t.contains("リストから選択してください"))),"{key}"); // i18n-ok: Assert the Japanese locale translation of the validation message.
        assert_eq!(s.with(|e,_|e.current_cell()),before,"selection stays at invalid cell: {key}");
        assert!(s.value(cell).is_empty());
        assert!(s.with(|e,_|e.wb().validation_rejected.is_some()));
        s.settle();
        s.with(|e,_| {
            let (c,r)=e.wb().validation_rejected.unwrap();
            let cell=e.validation_cell_bounds(c,r).unwrap();
            let b=e.wb().validation_message_bounds.unwrap();
            assert!((b.center().y-cell.center().y).abs()<=gpui_kit::px(1.1),"message centers against cell: {b:?} / {cell:?}");
            assert!((b.left()-cell.right()-gpui_kit::px(88.)).abs()<=gpui_kit::px(1.1),"popup={b:?}, cell={cell:?}");
        });
        s.click_cell("B8");
        assert_eq!(s.with(|e,_|e.current_cell()),before,"modal blocks click-through");
        s.key("enter");
        assert!(s.with(|e,_|e.panel_message.is_none()));
        assert!(s.with(|e,_|e.cell_edit().is_none()),"dismissal ends invalid editing");
        assert!(s.value(cell).is_empty(),"dismissal clears the cell");
        s.key("escape");
    }
    // Confirm the filtered selection by clicking a different cell.
    s.click_cell("A10");
    s.key("alt-down");
    s.type_text("M");
    s.key("up");
    s.click_cell("B10");
    assert_eq!(s.value("A10"),CellValue::Text("Microsoft".into()));
    assert!(s.with(|e,_|e.wb().validation_choices.is_none()));
    s.click_cell("A10");
    s.type_text("H");
    s.key("enter");
    assert_eq!(s.value("A10"),CellValue::Text("Microsoft".into()));
    s.key("enter");
    assert!(s.value("A10").is_empty(),"dismissal clears an old value as well as the invalid draft");
    // A cell near the right edge cannot accommodate the message on its right.
    let target=s.with(|e,_|(0..30).filter_map(|c|e.cell_center(c,9)).find(|p|f32::from(p.x)>900. && f32::from(p.x)<1100.).unwrap());
    s.click((f32::from(target.x),f32::from(target.y)));
    s.update(|e,_,cx|e.set_list_validation("Apple,Meta", "",cx));
    s.type_text("H");
    s.key("enter");
    s.settle();
    s.with(|e,_| {
        let b=e.wb().validation_message_bounds.unwrap();
        assert!((f32::from(b.center().x)-600.).abs()<1.1,"fallback centers in window: {b:?}");
        assert!((f32::from(b.center().y)-400.).abs()<1.1,"fallback centers vertically: {b:?}");
    });
    s.key("enter");
    s.key("escape");

    // Upward placement uses the input row's top, rather than flipping around its bottom.
    s.size(1200.,420.);
    s.click_cell("A8");
    s.key("alt-down");
    s.settle();
    s.with(|e,_| {
        let inner=e.wb().validation_choices.as_ref().unwrap().bounds.get().unwrap();
        let cell=e.validation_cell_bounds(0,7).unwrap();
        assert!((inner.bottom()+gpui_kit::px(1.)-cell.top()).abs()<=gpui_kit::px(1.1),"upward list ends at input top: {inner:?} / {cell:?}");
        assert!(inner.top()>=gpui_kit::px(0.));
    });
    s.screenshot("upward-common-dropdown");
    s.key("escape");

}
