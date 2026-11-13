// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #138: the clauses of the composing text are underlined as the input method says: the clause being converted thick, the
//! converted ones thin, the part still being typed dotted (what Word shows). Every surface with text input draws it through
//! `render::ime_text`; each test gives a surface composing text with clauses and reads back what would be drawn, as numbers.
use super::Scenario;
use crate::render::ime_text::{runs, spans, underline_shapes, ImeSpan};
use crate::shell::format::with_format;
use gpui_kit::{ImeClause, ImeLine};
use ibis_doc_model::{Underline, UnderlineStyle};
use crate::pptx::ext::*;
use crate::docx::ext::*;
use crate::xlsx::ext::*;

pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
    ("docx::issue138_ime_spans_and_lines",spans_and_lines),
    ("docx::issue127_ime_clauses_from_the_browser_selection",from_browser_selection),
    ("docx::issue138_ime_clauses_in_the_document_body",in_document),
    ("docx::issue138_ime_clauses_in_a_docx_text_box",in_docx_box),
    ("pptx::issue138_ime_clauses_in_a_text_box",in_pptx_box),
    ("pptx::issue138_ime_clauses_in_the_outline",in_outline),
    ("xlsx::issue138_ime_clauses_in_the_cell_and_the_bar",in_xlsx_cell),
    ("xlsx::issue138_ime_clauses_in_a_text_box",in_xlsx_box),
]}

/// "ここでは" being converted as three clauses: "ここで" the converting one (thick), "は" still typed (dotted), the rest converted (thin).
fn word_like()->Vec<ImeClause>{vec![
    ImeClause{range:0..3,thick:true,line:ImeLine::Solid},
    ImeClause{range:3..4,thick:false,line:ImeLine::Dotted},
    ImeClause{range:4..11,thick:false,line:ImeLine::Solid},
]}
const TEXT:&str="ここでは着物を脱いで"; // i18n-ok: Japanese IME composition fixture from issue #138.

/// GitHub issue #127 (the web): a browser says only which part of the composing text is selected. Nothing selected: still being typed, all
/// dotted. A clause selected: it thick, the rest thin and solid. The ranges are UTF-16 units, cut to the text.
fn from_browser_selection(){
    let dotted=|r:std::ops::Range<usize>|ImeClause{range:r,thick:false,line:ImeLine::Dotted};
    let solid=|r:std::ops::Range<usize>,thick|ImeClause{range:r,thick,line:ImeLine::Solid};
    assert_eq!(ImeClause::from_selection(5,5..5),vec![dotted(0..5)],"a caret: not converted");
    assert_eq!(ImeClause::from_selection(5,0..0),vec![dotted(0..5)]);
    assert_eq!(ImeClause::from_selection(0,0..0),vec![],"no text, no clause");
    assert_eq!(ImeClause::from_selection(5,0..5),vec![solid(0..5,true)],"all selected: one converting clause");
    assert_eq!(ImeClause::from_selection(5,2..4),vec![solid(0..2,false),solid(2..4,true),solid(4..5,false)]);
    assert_eq!(ImeClause::from_selection(5,3..99),vec![solid(0..3,false),solid(3..5,true)],"cut to the text");
    assert_eq!(ImeClause::from_selection(5,9..12),vec![dotted(0..5)],"a selection outside the composition: not a clause");
    // Through the line every surface draws: the converting clause thick, the others thin; not converted: dotted.
    let drawn=|c:&[ImeClause]|spans(TEXT,c).iter().map(|s|s.style()).collect::<Vec<_>>();
    assert_eq!(drawn(&ImeClause::from_selection(10,3..6)),vec![UnderlineStyle::Single,UnderlineStyle::Thick,UnderlineStyle::Single]);
    assert_eq!(drawn(&ImeClause::from_selection(10,10..10)),vec![UnderlineStyle::Dotted]);
}

fn spans_and_lines(){
    let s=spans(TEXT,&word_like());
    assert_eq!(s,vec![
        ImeSpan{range:0..3,thick:true,line:ImeLine::Solid},
        ImeSpan{range:3..4,thick:false,line:ImeLine::Dotted},
        ImeSpan{range:4..10,thick:false,line:ImeLine::Solid},
    ],"the clauses as characters, the last cut to the text");
    assert_eq!(s.iter().map(|s|s.style()).collect::<Vec<_>>(),vec![UnderlineStyle::Thick,UnderlineStyle::Dotted,UnderlineStyle::Single]);
    // Nothing said by the platform (or no clause for some characters): a thin solid line.
    assert_eq!(spans("\u{3042}\u{3044}",&[]),vec![ImeSpan{range:0..2,thick:false,line:ImeLine::Solid}]);
    // A character outside the BMP is two UTF-16 units but one character: the clause 2..3 is the second character.
    assert_eq!(spans("𠮷ab",&[ImeClause{range:2..3,thick:true,line:ImeLine::Solid}]),vec![
        ImeSpan{range:0..1,thick:false,line:ImeLine::Solid},ImeSpan{range:1..2,thick:true,line:ImeLine::Solid},ImeSpan{range:2..3,thick:false,line:ImeLine::Solid}]);
    // Every kind: the style of the page's underline, hence its thickness and dashes (`underline_items`).
    let all=[(false,ImeLine::Solid,UnderlineStyle::Single),(true,ImeLine::Solid,UnderlineStyle::Thick),(false,ImeLine::Dotted,UnderlineStyle::Dotted),(true,ImeLine::Dotted,UnderlineStyle::DottedHeavy),(false,ImeLine::Wavy,UnderlineStyle::Wave),(true,ImeLine::Wavy,UnderlineStyle::WavyHeavy)];
    for (thick,line,style) in all{assert_eq!(ImeSpan{range:0..1,thick,line}.style(),style);}
    // The numbers drawn, 12 pt text, a run 30 pt long: a box (x, y, w, h) a thin solid line, a thicker one for the converting clause.
    let (thin,_)=underline_shapes(UnderlineStyle::Single,12.,0.,30.,10.);let (thick,_)=underline_shapes(UnderlineStyle::Thick,12.,0.,30.,10.);
    assert_eq!((thin.len(),thick.len()),(1,1));assert_eq!((thin[0][0],thin[0][2]),(0.,30.));
    assert!(thick[0][3]>thin[0][3],"thick {:?} over thin {:?}",thick[0],thin[0]);
    // Dotted: many short boxes with gaps, together shorter than the run; wavy: strokes.
    let (dots,_)=underline_shapes(UnderlineStyle::Dotted,12.,0.,30.,10.);assert!(dots.len()>5&&dots.iter().map(|b|b[2]).sum::<f32>()<30.,"{dots:?}");
    let (_,waves)=underline_shapes(UnderlineStyle::Wave,12.,0.,30.,10.);assert!(!waves.is_empty());
    // Characters 10 px wide: the three spans join into three runs of the right extent.
    let cells:Vec<(f32,f32,usize)>=(0..10).map(|i|(i as f32*10.,10.,i)).collect();
    assert_eq!(runs(&cells,&s),vec![(UnderlineStyle::Thick,0.,30.),(UnderlineStyle::Dotted,30.,40.),(UnderlineStyle::Single,40.,100.)]);
}

fn in_document(){
    let mut s=Scenario::start_view();s.update(|e,w,cx|e.new_file(ibis_pptx::Kind::Document,w,cx));
    s.update(|e,w,cx|e.doc_replace_and_mark_text_in_range(None,TEXT,Some(0..0),w,cx));
    assert!(s.with(|e,_|e.doc_state().unwrap().marked_clauses.is_empty()),"not said yet: thin everywhere");
    s.update(|e,w,cx|with_format!(e,set_ime_clauses(e,word_like(),w,cx)));
    assert_eq!(s.with(|e,_|e.doc_state().unwrap().marked_clauses.clone()),word_like());
    let styles=s.with(|e,_|{let d=e.doc_state().unwrap();spans(d.marked.as_deref().unwrap(),&d.marked_clauses).iter().map(|s|s.style()).collect::<Vec<_>>()});
    assert_eq!(styles,vec![UnderlineStyle::Thick,UnderlineStyle::Dotted,UnderlineStyle::Single]);
    s.screenshot("issue138-ime-clauses-body");
    // A new composition string forgets them until the platform says again; the commit clears them.
    s.update(|e,w,cx|e.doc_replace_and_mark_text_in_range(None,TEXT,Some(0..0),w,cx));
    assert!(s.with(|e,_|e.doc_state().unwrap().marked_clauses.is_empty()));
    s.update(|e,w,cx|with_format!(e,set_ime_clauses(e,word_like(),w,cx)));
    s.update(|e,w,cx|e.doc_replace_text_in_range(None,"確定",w,cx)); // i18n-ok: IME commit fixture.
    assert!(s.with(|e,_|e.doc_state().unwrap().marked_clauses.is_empty()));
    // Vertical writing: the same spans, drawn on the column's left (`paint_column`).
    s.menu("docx.page.direction.vertical");
    s.update(|e,w,cx|e.doc_replace_and_mark_text_in_range(None,TEXT,Some(0..0),w,cx));
    s.update(|e,w,cx|with_format!(e,set_ime_clauses(e,word_like(),w,cx)));
    assert_eq!(s.with(|e,_|e.doc_state().unwrap().marked_clauses.len()),3);
    s.screenshot("issue138-ime-clauses-vertical");
}

/// The kinds of underline of the composing run's characters of a text body, by character.
fn body_kinds(body:&ibis_doc_model::TextBody,from:usize,count:usize)->Vec<Underline>{
    let mut out=Vec::new();for p in &body.paragraphs{for r in &p.runs{for _ in r.text.chars(){out.push(if r.style.underline{r.style.underline_kind}else{Underline::Words});}}}
    out[from..from+count].to_vec()
}
fn expected_kinds()->Vec<Underline>{let mut v=vec![Underline::Heavy;3];v.push(Underline::Dotted);v.extend(vec![Underline::Single;6]);v}

fn in_docx_box(){
    let mut s=Scenario::open("test/editing/docx-report.docx");
    s.menu("insert.text-box");s.settle();
    let rect=crate::scenario_list::scenario_fix::docx_box_win_rect(&mut s);
    s.double_click((rect.0+rect.2/2.,rect.1+rect.3/2.));s.settle();
    assert!(s.with(|e,_|e.doc_state().is_some_and(|d|d.box_edit.is_some())),"entered the text box");
    s.update(|e,_,cx|e.input_mark(TEXT,cx));
    s.update(|e,w,cx|with_format!(e,set_ime_clauses(e,word_like(),w,cx)));
    s.settle();
    let kinds=s.with(|e,_|{let (_,_,comp)=e.resources.edited_box.as_ref().expect("the edited box");body_kinds(comp.as_ref().expect("composing body"),0,10)});
    // As Word draws a text box: the converting clause reversed (no line), every other clause thin dotted.
    let mut word_box=vec![Underline::Words;3];word_box.extend(vec![Underline::Dotted;7]);
    assert_eq!(kinds,word_box);
    s.screenshot("issue138-ime-clauses-docx-box");
}

fn pptx_text_box(s:&mut Scenario){
    s.menu("pptx.new-slide");s.settle();s.menu("insert.text-box");s.settle();
    let at=s.with(|e,_|{let l=e.slide_layout().unwrap();(f32::from(l.origin.x)+150.,f32::from(l.origin.y)+150.)});
    s.drag(at,(at.0+220.,at.1+90.));s.settle();
    assert!(s.with(|e,_|e.text_editing().is_some()),"the new text box is edited");
}

fn in_pptx_box(){
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");pptx_text_box(&mut s);
    s.update(|e,_,cx|e.input_mark(TEXT,cx));
    s.update(|e,w,cx|with_format!(e,set_ime_clauses(e,word_like(),w,cx)));
    s.settle();
    let kinds=s.with(|e,_|{let id=e.text_editing().unwrap().shape;body_kinds(&e.composing_body(id).unwrap(),0,10)});
    assert_eq!(kinds,expected_kinds());
    s.screenshot("issue138-ime-clauses-pptx-box");
    // A new mark forgets the clauses: thin everywhere again.
    s.update(|e,_,cx|e.input_mark(TEXT,cx));
    let kinds=s.with(|e,_|{let id=e.text_editing().unwrap().shape;body_kinds(&e.composing_body(id).unwrap(),0,10)});
    assert_eq!(kinds,vec![Underline::Single;10]);
}

fn in_outline(){
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e,w,cx|{e.toggle_outline_view(w,cx);e.begin_outline_edit(0,0,w,cx);});
    assert!(s.with(|e,_|e.is_outline_editing()));
    s.update(|e,w,cx|with_format!(e,replace_and_mark_text_in_range(e,None,TEXT,Some(0..0),w,cx)));
    s.update(|e,w,cx|with_format!(e,set_ime_clauses(e,word_like(),w,cx)));
    assert_eq!(s.with(|e,_|e.outline_edit().unwrap().marked_clauses.clone()),word_like());
    s.screenshot("issue138-ime-clauses-outline");
}

fn in_xlsx_cell(){
    let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");
    s.update(|e,_,cx|e.cell_replace(None,TEXT,Some(Some(0..10)),cx));
    s.update(|e,w,cx|with_format!(e,set_ime_clauses(e,word_like(),w,cx)));
    // The cell's text: the underline of each clause as bytes of the text (3 bytes a character here), as the editor and the bar draw them.
    let (u,bar)=s.with(|e,_|{let c=e.cell_edit().unwrap();(c.marked_underlines(),c.bar_line(0,&c.text).marked_underlines)});
    let start=s.with(|e,_|e.cell_edit().unwrap().text.find('こ').unwrap());
    assert_eq!(u,vec![(UnderlineStyle::Thick,start..start+9),(UnderlineStyle::Dotted,start+9..start+12),(UnderlineStyle::Single,start+12..start+30)]);
    assert_eq!(bar,u,"the formula bar draws the same clauses");
    s.screenshot("issue138-ime-clauses-xlsx-cell");
    s.update(|e,_,cx|e.cell_replace(None,TEXT,Some(Some(0..10)),cx));
    assert_eq!(s.with(|e,_|e.cell_edit().unwrap().marked_underlines().len()),1,"forgotten with a new composition string: thin everywhere");
}

fn in_xlsx_box(){
    let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.text-box");s.settle();
    s.update(|e,_,cx|e.input_mark(TEXT,cx));
    s.update(|e,w,cx|with_format!(e,set_ime_clauses(e,word_like(),w,cx)));
    s.settle();
    let kinds=s.with(|e,_|{let id=e.text_editing().unwrap().shape;body_kinds(&e.composing_body(id).unwrap(),0,10)});
    assert_eq!(kinds,expected_kinds());
}
