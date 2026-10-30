// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::{Scenario,button_probe as buttons};
use gpui_kit::*;
use crate::docx::scenarios::DocSteps;
use ibis_doc_model::FlowPos;
pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
    ("shell::issue61_recent_remove",recent_remove),
    ("shell::brand_logo_responsive",logos),
    ("docx::issue59_control_click_link",control_click),
    ("docx::issue60_vertical_symbols_and_ime",vertical),
    ("pptx::issue62_slide_shortcuts",shortcuts),
    ("pptx::issue62_text_glow_outline",glow_outline),
]}
fn center(b:Bounds<Pixels>)->(f32,f32){(f32::from(b.center().x),f32::from(b.center().y))}
fn recent_remove(){
    let _probe=buttons::Probe::new();let mut s=Scenario::start_view();
    let keep=s.screenshot_path("history-keep").with_extension("docx");let remove=s.screenshot_path("history-remove").with_extension("docx");let missing=s.screenshot_path("history-missing").with_extension("docx");
    let bytes=ibis_pptx::blank_file(ibis_pptx::Kind::Document).unwrap();std::fs::write(&keep,&bytes).unwrap();std::fs::write(&remove,&bytes).unwrap();
    s.update(|e,_,cx|{e.recent_files=crate::shell::recent_files::add(&keep);e.recent_files=crate::shell::recent_files::add(&remove);cx.notify();});
    let b=buttons::bounds("row-actions-0").unwrap();s.hover(center(b));let shot=s.screenshot("recent-hover");let image=image::open(shot).unwrap().to_rgba8();let viewport=s.update(|_,w,_|f32::from(w.viewport_size().width));let scale=image.width()as f32/viewport;
    let mut ink=0;for y in (f32::from(b.origin.y)*scale)as u32..(f32::from(b.bottom())*scale)as u32{for x in (f32::from(b.origin.x)*scale)as u32..(f32::from(b.right())*scale)as u32{let p=image.get_pixel(x,y);if p[0]<100&&p[1]<100&&p[2]<100{ink+=1;}}}assert!(ink>5,"the overflow dots must be visible");buttons::click(&mut s,"row-actions-0");
    s.screenshot("recent-actions");
    let option=buttons::all().into_iter().find(|id|id.contains("searchable-item-row-actions")).expect("history menu option");buttons::click(&mut s,&option);
    assert!(!s.with(|e,_|e.has_document),"overflow click must not open the file");assert!(remove.is_file());assert!(!crate::shell::recent_files::load().contains(&remove));assert!(crate::shell::recent_files::load().contains(&keep));
    s.update(|e,_,cx|{e.recent_files=crate::shell::recent_files::add(&missing);cx.notify();});let b=buttons::bounds("row-actions-0").unwrap();s.right_click((f32::from(b.origin.x)-80.,f32::from(b.center().y)));assert!(s.with(|e,_|e.link_popup.is_some()));buttons::click(&mut s,"menu-item-start.recent.remove");assert!(!crate::shell::recent_files::load().contains(&missing));assert!(keep.is_file());
}
fn logos(){
    let _probe=buttons::Probe::new();let mut s=Scenario::start_view();s.size(1200.,800.);let b=buttons::bounds("start-app-logo").unwrap();assert!(f32::from(b.size.height)>80.);s.screenshot("start-logo");
    s.size(760.,800.);let b=buttons::bounds("start-app-logo").unwrap();assert!((f32::from(b.size.width)/f32::from(b.size.height)-crate::shell::chrome::LOGO_ASPECT).abs()<0.1);s.screenshot("start-logo-narrow");
    for (kind,name)in [(ibis_pptx::Kind::Document,"docx"),(ibis_pptx::Kind::Workbook,"xlsx"),(ibis_pptx::Kind::Presentation,"pptx")]{
        s.update(|e,w,cx|e.new_file(kind,w,cx));s.size(1200.,800.);assert!(buttons::bounds("editor-app-logo").is_some());s.screenshot(&format!("{name}-logo"));buttons::clear();s.size(400.,800.);assert!(buttons::bounds("editor-app-logo").is_none(),"narrow {name}");s.screenshot(&format!("{name}-logo-hidden"));buttons::clear();s.size(1200.,800.);assert!(buttons::bounds("editor-app-logo").is_some());
    }
}
struct Capture;
impl Drop for Capture{fn drop(&mut self){crate::shell::links::CAPTURE_URLS.with(|s|*s.borrow_mut()=None);}}
fn control_click(){
    crate::shell::links::CAPTURE_URLS.with(|s|*s.borrow_mut()=Some(Vec::new()));let _capture=Capture;
    let mut s=Scenario::start_view();s.update(|e,w,cx|e.new_file(ibis_pptx::Kind::Document,w,cx));s.type_text("Follow this link");let(p,_) = s.paragraph(0);
    s.update(|e,_,cx|{e.set_caret(FlowPos::new(p,0),false,cx);e.set_caret(FlowPos::new(p,16),true,cx);e.set_link("https://example.com/issue59",cx);e.set_caret(FlowPos::new(p,0),false,cx);});
    let at=s.update(|e,_,_|{let lines=e.flow_lines();let(page,x,y,h)=e.caret_box(&lines,FlowPos::new(p,2)).unwrap();let(o,k)=e.page_origin(page).unwrap();(f32::from(o.x)+x*k+1.,f32::from(o.y)+(y+h/2.)*k)});
    for modifiers in [Modifiers{control:true,..Default::default()},Modifiers{platform:true,..Default::default()}]{s.dispatch_click_with(at,modifiers);assert_eq!(crate::shell::links::CAPTURE_URLS.with(|s|s.borrow().as_ref().unwrap().last().cloned()),Some("https://example.com/issue59".into()));}
    s.screenshot("control-click-hyperlink");
}
fn vertical(){
    let mut s=Scenario::start_view();s.update(|e,w,cx|e.new_file(ibis_pptx::Kind::Document,w,cx));
    s.type_text("あ。あ、あ（）〔〕［］｛｝〈〉《》「」『』【】ー～“”‘’㌢㍍㌘㍑"); // i18n-ok: Japanese document fixture content, not UI labels.
    s.menu("docx.page.direction.vertical");let(p,_) = s.paragraph(0);
    let steps=s.with(|e,_|{let lines=e.flow_lines();(0..5).map(|i|e.caret_box(&lines,FlowPos::new(p,i)).unwrap().2).collect::<Vec<_>>()});let cell=steps[1]-steps[0];assert!(cell>0.);assert!(((steps[2]-steps[1])-cell).abs()<0.5);assert!(((steps[4]-steps[3])-cell).abs()<0.5);
    s.update(|e,w,cx|e.doc_replace_and_mark_text_in_range(None,"へんかんちゅう",Some(7..7),w,cx)); // i18n-ok: Japanese IME composition fixture.
    assert!(s.with(|e,_|e.doc_state().unwrap().marked.is_some()));s.screenshot("vertical-symbols-ime");
    s.update(|e,w,cx|e.doc_replace_text_in_range(None,"変換中",w,cx)); // i18n-ok: Japanese IME commit fixture.
    assert!(s.with(|e,_|e.doc_state().unwrap().marked.is_none()));
}
// ⌃D is the text editing key (delete forward) on a Mac, so duplicating is ⌘D there.
const DUPLICATE_KEY:&str=if cfg!(target_os="macos"){"cmd-d"}else{"ctrl-d"};
fn shortcuts(){
    let mut s=Scenario::start_view();s.update(|e,w,cx|e.new_file(ibis_pptx::Kind::Presentation,w,cx));let before=s.with(|e,_|e.presentation().slides.len());s.key("ctrl-m");assert_eq!(s.with(|e,_|e.presentation().slides.len()),before+1);s.key(DUPLICATE_KEY);assert_eq!(s.with(|e,_|e.presentation().slides.len()),before+2);
    s.update(|e,_,cx|{let id=e.document.ids().new_shape_id();let shape=ibis_doc_model::Shape::text_box(id,ibis_doc_model::EmuRect::new(0,0,2000000,1000000),"Duplicate shape");let index=e.slide().shapes.len();e.run(vec![ibis_doc_model::Command::InsertShape{slide:e.current_slide,index,shape}],cx);e.set_selection(vec![id]);});
    let count=s.with(|e,_|e.slide().shapes.len());s.key(DUPLICATE_KEY);assert_eq!(s.with(|e,_|e.slide().shapes.len()),count+1);s.key("cmd-z");assert_eq!(s.with(|e,_|e.slide().shapes.len()),count);s.screenshot("slide-shortcuts");
}

fn glow_outline(){
    use ibis_doc_model::{Color,EmuRect,Rgb,Shape};
    let mut s=Scenario::start_view();s.update(|e,w,cx|e.new_file(ibis_pptx::Kind::Presentation,w,cx));
    let p=s.with(|e,_|e.presentation().clone());let mut right_edges=Vec::new();let mut pixels_per_point=0.;
    for width in [0,8*12700] {
        let mut shape=Shape::text_box(ibis_doc_model::ShapeId(9999),EmuRect::new(0,0,4000000,2000000),"O");let body=shape.text_body_mut().unwrap();body.style.font_size=7200;body.style.font=Some("Arial".into());for run in body.paragraphs.iter_mut().flat_map(|p|&mut p.runs){run.style.font_size=Some(7200);run.style.outline=(width>0).then_some((Rgb(0),width));run.style.glow=Some((Color::opaque(Rgb(0xff0000)),4*12700));}
        let rendered=crate::native_shape_clipboard::image_fallback(&p,&[shape]).unwrap();let pixels=image::load_from_memory(&rendered.bytes).unwrap().to_rgba8();pixels_per_point=pixels.width() as f32/(4000000. /12700.);let right=pixels.enumerate_pixels().filter(|(_,_,p)|p[0]>p[1].saturating_add(5)&&p[0]>p[2].saturating_add(5)).map(|(x,_,_)|x).max().expect("a red glow");right_edges.push(right);
    }
    let expansion=right_edges[1]as f32-right_edges[0]as f32;
    // An 8pt centred outline adds 4pt; account for the actual output pixel density.
    assert!((expansion-4.*pixels_per_point).abs()<=2.,"glow follows the outside of the outline: {right_edges:?}");
}
