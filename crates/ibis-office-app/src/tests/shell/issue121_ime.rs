// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #121 (the user's F19): the text the input method is composing is part of the layout (the text after it is pushed on, in
//! horizontal and in vertical writing, as in Word), and the page scrolls so the end of it is in view.
use super::Scenario;
use ibis_doc_model::FlowPos;
use crate::shell::format::with_format;
use crate::docx::ext::*;
use crate::docx::scenarios::DocSteps;

pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
    ("docx::issue121_composing_text_pushes_the_text_after_it_horizontal",pushes_horizontal),
    ("docx::issue121_composing_text_pushes_the_text_after_it_vertical",pushes_vertical),
    ("docx::issue121_composing_text_is_scrolled_into_view",scrolled_into_view),
    ("docx::ime_candidate_rect_is_the_last_line_of_wrapped_composing_text",candidate_rect_last_line),
]}

const AFTER:&str="\u{5f8c}\u{308d}\u{306e}\u{6587}\u{5b57}"; // i18n-ok: Japanese document fixture, text after the caret.
const COMPOSING:&str="\u{3042}\u{3044}\u{3046}\u{3048}\u{304a}"; // i18n-ok: Japanese IME composition fixture ("aiueo").

/// How far the end of the paragraph is from its start along the line, before and while the five characters are composed at its start.
fn end_shift(vertical:bool)->(f32,f32,f32){
    let mut s=Scenario::start_view();s.update(|e,w,cx|e.new_file(ibis_pptx::Kind::Document,w,cx));
    s.type_text(AFTER);
    if vertical{s.menu("docx.page.direction.vertical");}
    let (p,_)=s.paragraph(0);let n=AFTER.chars().count();
    let size=s.with(|e,_|e.current_format().map_or(12.,|(_,st)|st.size as f32/100.));
    let along=|s:&mut Scenario,n:usize|s.with(|e,_|{let lines=e.flow_lines();let end=e.caret_box(&lines,FlowPos::new(p,n)).unwrap();let start=e.caret_box(&lines,FlowPos::new(p,0)).unwrap();if vertical{end.2-start.2}else{end.1-start.1}});
    s.update(|e,_,cx|e.set_caret(FlowPos::new(p,0),false,cx));
    let before=along(&mut s,n);
    s.update(|e,w,cx|e.doc_replace_and_mark_text_in_range(None,COMPOSING,Some(0..0),w,cx));
    s.screenshot(if vertical{"issue121-ime-pushes-vertical"}else{"issue121-ime-pushes-horizontal"});
    // The end of the paragraph is now after the composing text too (the layout has it typed in).
    let after=s.with(|e,_|{let lines=e.flow_lines();let end=e.caret_box(&lines,FlowPos::new(p,n+COMPOSING.chars().count())).unwrap();let start=e.caret_box(&lines,FlowPos::new(p,0)).unwrap();if vertical{end.2-start.2}else{end.1-start.1}});
    s.update(|e,w,cx|e.doc_replace_text_in_range(None,COMPOSING,w,cx));
    let committed=along(&mut s,n+COMPOSING.chars().count());
    (after-before,committed-before,size)
}

fn pushes_horizontal(){
    let (pushed,committed,size)=end_shift(false);
    assert!(pushed>4.*size&&pushed<6.*size,"the five composing characters push the text on by about five ems: {pushed} ({size} pt)");
    assert!((pushed-committed).abs()<1.,"as committed text does: {pushed} against {committed}");
}
fn pushes_vertical(){
    let (pushed,committed,size)=end_shift(true);
    assert!(pushed>4.*size&&pushed<6.*size,"the five composing characters push the text down by about five ems: {pushed} ({size} pt)");
    assert!((pushed-committed).abs()<1.,"as committed text does: {pushed} against {committed}");
}

fn scrolled_into_view(){
    let mut s=Scenario::start_view();s.size(1200.,800.);s.update(|e,w,cx|e.new_file(ibis_pptx::Kind::Document,w,cx));
    for _ in 0..40{s.type_text("\u{3042}\u{3044}\u{3046}\u{3048}\u{304a}");s.key("enter");} // i18n-ok: Japanese document fixture.
    // The caret is at the end of the last paragraph, in view; a long composing string runs on below it.
    let scrolled=|s:&mut Scenario|s.with(|e,_|f32::from(e.page_origin(0).unwrap().0.y));
    let at_start=scrolled(&mut s);
    let long:String="\u{3042}".repeat(600); // i18n-ok: Japanese IME composition fixture.
    s.update(|e,w,cx|e.doc_replace_and_mark_text_in_range(None,&long,Some(0..0),w,cx));
    assert!(scrolled(&mut s)<at_start-50.,"the page scrolled down ({at_start} to {})",scrolled(&mut s));
    let (bottom,view)=s.with(|e,_|{
        let (_,caret)=e.text_selection().unwrap();let lines=e.flow_lines();
        let n=long.chars().count();let (page,_,y,h)=e.caret_box(&lines,FlowPos::new(caret.para,caret.offset+n)).unwrap();
        let (origin,k)=e.page_origin(page).unwrap();
        (f32::from(origin.y)+(y+h)*k,e.workspace_bounds.map(|b|f32::from(b.origin.y)+f32::from(b.size.height)).unwrap())
    });
    assert!(bottom<=view+1.,"the end of the composing text ({bottom}) is in view (the page's bottom edge is {view})");
}

/// The rectangle the input method gets for the composing text (where it opens its candidate window) is on the LAST line of the text
/// when it wraps, not the first, so the candidates do not cover the text being typed.
fn candidate_rect_last_line(){
    let mut s=Scenario::start_view();s.update(|e,w,cx|e.new_file(ibis_pptx::Kind::Document,w,cx));
    let long:String="\u{3042}".repeat(120); // i18n-ok: Japanese IME composition fixture.
    s.update(|e,w,cx|e.doc_replace_and_mark_text_in_range(None,&long,Some(0..0),w,cx));
    let n=long.encode_utf16().count();
    let y=|s:&mut Scenario,r:std::ops::Range<usize>|s.update(|e,w,cx|with_format!(e,bounds_for_range(e,r,Default::default(),w,cx)).map(|b|f32::from(b.origin.y)).unwrap());
    let (first,last)=(y(&mut s,0..0),y(&mut s,n..n));
    assert!(last>first+10.,"the end of the composing text is on a later line: first {first}, last {last}");
    // The candidate rectangle, as the platform layer computes it from the marked range, is the last line.
    let got=s.update(|e,w,cx|{
        let sel=with_format!(e,selected_text_range(e,true,w,cx)).unwrap();
        gpui_kit::PlatformInputHandler::compute_ime_candidate_bounds(Some(0..n),&sel,|r|with_format!(e,bounds_for_range(e,r,Default::default(),w,cx))).map(|b|f32::from(b.origin.y))
    });
    assert!((got.unwrap()-last).abs()<1.,"candidate rect is on the last line: {got:?} against {last}");
}
