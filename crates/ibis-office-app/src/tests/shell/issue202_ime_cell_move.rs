// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #202: choosing another cell (a click, an arrow key) while the input method is still composing puts the composing text into
//! the cell being left (as Excel does) and ends the composition; nothing of it is carried on into the next cell.
use super::Scenario;
use crate::xlsx::scenarios::SheetSteps;
use crate::xlsx::ext::*;

pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
    ("xlsx::issue202_ime_composing_text_is_committed_to_the_cell_left_by_a_click",left_by_a_click),
    ("xlsx::issue202_ime_composing_text_is_committed_to_the_cell_left_by_an_arrow_key",left_by_an_arrow),
]}

const COMPOSING:&str="あいう"; // i18n-ok: Japanese IME composition fixture (issue #202).
const NEXT:&str="え"; // i18n-ok: Japanese IME composition fixture (issue #202).

fn text_at(s:&mut Scenario,cell:(u32,u32))->String{
    s.with(|e,_|e.current_sheet().unwrap().1.cell(cell.0,cell.1).map(|c|c.text.clone()).unwrap_or_default())
}

fn compose_then(leave:&dyn Fn(&mut Scenario)){
    let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("H20");
    s.update(|e,_,cx|e.cell_replace(None,COMPOSING,Some(Some(0..3)),cx));
    assert!(s.with(|e,_|!e.cell_edit().unwrap().marked_underlines().is_empty()),"the text is being composed");
    leave(&mut s);
    assert!(s.with(|e,_|e.cell_edit().is_none()),"typing ended, nothing is composing");
    assert_eq!(text_at(&mut s,(19,7)),COMPOSING,"the cell left holds the composing text");
    assert_eq!(text_at(&mut s,(20,7)),"","the next cell holds nothing");
    // What the input method sends next belongs to the new cell alone.
    s.update(|e,_,cx|e.cell_replace(None,NEXT,Some(Some(0..1)),cx));
    assert_eq!(s.with(|e,_|e.cell_edit().map(|c|c.text.clone())),Some(NEXT.to_string()),"the next cell starts from its own text");
}

fn left_by_a_click(){compose_then(&|s|s.click_cell("H21"));}
fn left_by_an_arrow(){compose_then(&|s|s.key("down"));}
