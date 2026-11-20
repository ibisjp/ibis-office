// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #182: reconversion. Selecting text and pressing the Convert key (Windows) or the re-convert gesture (macOS) makes the
//! input method ask the surface for the selected range (`selected_text_range`) and its text (`text_for_range`), then compose over it.
//! Each surface with text input must tell both: the document body, a cell being edited, a text box. The platform half (Windows
//! `WM_IME_REQUEST` in the `gpui-pre-windows` fork) is checked on a Windows PC.
use super::Scenario;
use crate::shell::format::with_format;
use crate::docx::ext::*;
use crate::xlsx::ext::*;

pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
    ("docx::issue182_reconvert_document_body",in_document),
    ("xlsx::issue182_reconvert_cell",in_xlsx_cell),
]}

const TEXT:&str="再変換の試験"; // i18n-ok: Japanese reconversion fixture from issue #182.

/// What the input method reads for reconversion: the selection's range and the text in it.
fn asked(s:&mut Scenario)->(std::ops::Range<usize>,String){
    s.update(|e,w,cx|{
        let sel=with_format!(e,selected_text_range(e,false,w,cx)).expect("a selection is told");
        let mut adjusted=None;
        let text=with_format!(e,text_for_range(e,sel.range.clone(),&mut adjusted,w,cx)).unwrap_or_default();
        (sel.range,text)
    })
}

fn in_document(){
    let mut s=Scenario::start_view();s.update(|e,w,cx|e.new_file(ibis_formats::Kind::Document,w,cx));
    s.update(|e,w,cx|e.doc_replace_text_in_range(None,TEXT,w,cx));
    assert_eq!(asked(&mut s),(6..6,String::new()),"a caret: nothing to reconvert");
    s.key("cmd-a");
    assert_eq!(asked(&mut s),(0..6,TEXT.to_string()),"all selected: the whole text is offered");
    // Composing over the selection replaces it (what the input method does after reading it).
    s.update(|e,w,cx|e.doc_replace_and_mark_text_in_range(None,"さいへんかん",Some(0..0),w,cx)); // i18n-ok: IME fixture.
    assert_eq!(s.with(|e,_|e.doc_state().unwrap().marked.clone()).as_deref(),Some("さいへんかん")); // i18n-ok: IME fixture.
}

fn in_xlsx_cell(){
    let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");
    s.update(|e,_,cx|e.cell_replace(None,TEXT,None,cx));
    assert_eq!(asked(&mut s),(6..6,String::new()),"a caret: nothing to reconvert");
    s.key("shift-home");
    assert_eq!(asked(&mut s),(0..6,TEXT.to_string()),"the selected text of the cell is offered");
    s.update(|e,_,cx|e.cell_replace(None,"さいへんかん",Some(None),cx)); // i18n-ok: IME fixture.
    assert_eq!(s.with(|e,_|e.cell_edit().unwrap().text.clone()),"さいへんかん","composing replaced the selection"); // i18n-ok: IME fixture.
}
