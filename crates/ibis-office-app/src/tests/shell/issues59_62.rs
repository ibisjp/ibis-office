// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::{Scenario,button_probe as buttons};
use gpui_kit::*;
use crate::docx::scenarios::DocSteps;
use ibis_doc_model::FlowPos;
use crate::docx::ext::*;
pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
    ("shell::issue61_recent_remove",recent_remove),
    ("shell::recent_missing_file_alert",recent_missing_alert),
    ("shell::brand_logo_responsive",logos),
    ("docx::issue59_control_click_link",control_click),
    ("docx::issue60_vertical_symbols_and_ime",vertical),
    ("pptx::issue62_slide_shortcuts",shortcuts),
    ("pptx::issue62_text_glow_outline",glow_outline),
    ("pptx::issue91_glow_formula_and_outline_anchor",glow_91),
    ("pptx::issue91_text_glow_is_one_smooth_blur",glow_smooth),
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
/// A recent file that is gone is greyed (checked once off the UI thread, not at each paint) and its click, like the click on a
/// file that cannot be read, tells it in a message box instead of doing nothing.
fn recent_missing_alert(){
    let _probe=buttons::Probe::new();let mut s=Scenario::start_view();
    let good=s.screenshot_path("recent-good").with_extension("docx");let gone=s.screenshot_path("recent-gone").with_extension("docx");let broken=s.screenshot_path("recent-broken").with_extension("docx");
    std::fs::write(&good,ibis_pptx::blank_file(ibis_pptx::Kind::Document).unwrap()).unwrap();std::fs::write(&broken,b"locked").unwrap();{use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&broken,std::fs::Permissions::from_mode(0)).unwrap();}let _=std::fs::remove_file(&gone);
    s.update(|e,_,cx|{e.recent_files=vec![good.clone(),gone.clone(),broken.clone()];e.recent_exists.clear();cx.notify();});
    s.wait(300);s.settle();
    assert_eq!(s.with(|e,_|e.recent_exists.get(&gone).copied()),Some(false),"the missing file is found once");
    assert_eq!(s.with(|e,_|e.recent_exists.get(&good).copied()),Some(true));
    s.screenshot("recent-missing-greyed");
    let row_click=|s:&mut Scenario,index:usize|{let b=buttons::bounds(&format!("row-actions-{index}")).unwrap();s.click((f32::from(b.origin.x)-80.,f32::from(b.center().y)));};
    // A greyed row is still clickable: the alert (title, name, reason) opens and the start view stays.
    row_click(&mut s,1);s.wait(300);s.settle();
    let alert=s.with(|e,_|e.alert.clone()).expect("the click on a missing file shows the message box");
    assert!(alert.1.contains("recent-gone.docx"),"the message names the file: {}",alert.1);assert!(!s.with(|e,_|e.has_document));
    s.screenshot("recent-missing-alert");
    buttons::click(&mut s,"alert-ok");assert!(s.with(|e,_|e.alert.is_none()));
    // A file that exists but cannot be read (no permission) gives the message box as well.
    row_click(&mut s,2);s.wait(500);s.settle();
    let (a,d,l)=s.with(|e,_|(e.alert.clone(),e.has_document,e.loading));assert!(a.is_some(),"an unreadable file shows the message box (has_document {d}, loading {l})");assert!(!s.with(|e,_|e.has_document));
    s.screenshot("recent-unreadable-alert");
    buttons::click(&mut s,"alert-ok");{use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&broken,std::fs::Permissions::from_mode(0o644)).unwrap();}
    // A good one still opens.
    row_click(&mut s,0);s.wait(800);s.settle();
    assert!(s.with(|e,_|e.has_document)&&s.with(|e,_|e.alert.is_none()),"a readable file opens");
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
        let mut shape=Shape::text_box(ibis_doc_model::ShapeId(9999),EmuRect::new(0,0,4000000,2000000),"O");let body=shape.text_body_mut().unwrap();body.style.font_size=7200;body.style.font=Some("Arial".into());for run in body.paragraphs.iter_mut().flat_map(|p|&mut p.runs){run.style.font_size=Some(7200);run.style.outline=(width>0).then_some((Color::opaque(Rgb(0)),width));run.style.glow=Some((Color::opaque(Rgb(0xff0000)),4*12700));}
        let rendered=crate::native_shape_clipboard::image_fallback(&p,&[shape]).unwrap();let pixels=image::load_from_memory(&rendered.bytes).unwrap().to_rgba8();pixels_per_point=pixels.width() as f32/(4000000. /12700.);let right=pixels.enumerate_pixels().filter(|(_,_,p)|p[0]>p[1].saturating_add(5)&&p[0]>p[2].saturating_add(5)).map(|(x,_,_)|x).max().expect("a red glow");right_edges.push(right);
    }
    let expansion=right_edges[1]as f32-right_edges[0]as f32;
    // An 8pt centred outline adds 4pt; account for the actual output pixel density.
    assert!((expansion-4.*pixels_per_point).abs()<=2.,"glow follows the outside of the outline: {right_edges:?}");
}

/// issue #91: PowerPoint's glow of a shape is the shape grown by half the radius and blurred with a Gaussian
/// of a sixth of it (fitted to its PDFs, tasks/doc/glow-measurements-20261030.md); the glow of outlined text sits
/// the same distance outside the outline's outer edge whatever the outline's width (measured: 3.37pt for a
/// 44pt, 10pt glow with outlines of 0, 2 and 6pt), so the outline's width is not part of the radius.
fn glow_91(){
    use ibis_doc_model::{Color,EmuRect,Rgb,Shape};
    for r in [10.0_f32,20.0,40.0,80.0]{let(spread,blur)=crate::render::shape_view::glow_spread_blur(r);assert!((spread-r/2.).abs()<1e-4&&(blur-r/6.).abs()<1e-4,"glow of {r}: {spread} {blur}");}
    let mut s=Scenario::start_view();s.update(|e,w,cx|e.new_file(ibis_pptx::Kind::Presentation,w,cx));
    let p=s.with(|e,_|e.presentation().clone());let mut gaps=Vec::new();
    for width in [0,8*12700]{
        let mut shape=Shape::text_box(ibis_doc_model::ShapeId(9999),EmuRect::new(0,0,4000000,2000000),"O");let body=shape.text_body_mut().unwrap();body.style.font_size=7200;body.style.font=Some("Arial".into());for run in body.paragraphs.iter_mut().flat_map(|p|&mut p.runs){run.style.font_size=Some(7200);run.style.outline=Some((Color::opaque(Rgb(0)),width.max(1)));run.style.glow=Some((Color::opaque(Rgb(0xff0000)),4*12700));if width==0{run.style.outline=None;}}
        let rendered=crate::native_shape_clipboard::image_fallback(&p,&[shape]).unwrap();let pixels=image::load_from_memory(&rendered.bytes).unwrap().to_rgba8();
        let ink=pixels.enumerate_pixels().filter(|(_,_,p)|p[3]>200&&p[0]<60&&p[1]<60&&p[2]<60).map(|(x,_,_)|x).max().expect("the text");
        let glow=pixels.enumerate_pixels().filter(|(_,_,p)|p[0]>p[1].saturating_add(5)&&p[0]>p[2].saturating_add(5)).map(|(x,_,_)|x).max().expect("a red glow");
        gaps.push(glow as f32-ink as f32);
    }
    assert!((gaps[0]-gaps[1]).abs()<=2.,"the glow is as far outside the text's outer edge with an outline as without: {gaps:?}");
}

/// issue #91: the glow of text is one Gaussian blur of the grown glyphs (not letters stacked in rings, which stepped): across the edge of a
/// vertical stem the glow's alpha falls without a step, and its size is Windows PowerPoint's: the distances from the edge at which the
/// alpha is 50 %, 10 % and 2 % are those measured there (`target/glow91/measure-table.tsv`, Arial H, 176 slides of
/// `tasks/doc/glow-measurements-20261106.md`) within 5 % (and a pixel). The PNGs are written to `target/glow91/` to look at.
fn glow_smooth(){
    use ibis_doc_model::{Color,EmuRect,Rgb,Shape};
    let mut s=Scenario::start_view();s.update(|e,w,cx|e.new_file(ibis_pptx::Kind::Presentation,w,cx));
    let p=s.with(|e,_|e.presentation().clone());
    // (size pt, rad pt, d50, d10, d02 in pt): rows of the measurement table.
    let measured:[(u32,i64,f32,f32,f32);9]=[(12,10,1.250,1.864,2.225),(24,5,1.125,1.629,1.910),(24,30,6.250,9.156,10.925),(44,10,3.000,4.500,5.425),(72,5,2.250,3.289,3.931),(72,30,12.958,19.250,22.800),(130,5,3.169,4.725,5.675),(130,10,6.875,10.031,11.925),(130,30,20.094,29.500,34.925)];
    for (size,rad,d50,d10,d02) in measured{
        let mut shape=Shape::text_box(ibis_doc_model::ShapeId(9999),EmuRect::new(0,0,6000000,3000000),"H");let body=shape.text_body_mut().unwrap();body.style.font_size=size*100;body.style.font=Some("Arial".into());body.style.align=ibis_doc_model::TextAlign::Center;for para in &mut body.paragraphs{para.align=Some(ibis_doc_model::TextAlign::Center);}
        for run in body.paragraphs.iter_mut().flat_map(|p|&mut p.runs){run.style.font_size=Some(size*100);run.style.glow=Some((Color::opaque(Rgb(0x00a000)),rad*12700));}
        let rendered=crate::native_shape_clipboard::image_fallback(&p,&[shape]).unwrap();
        std::fs::create_dir_all("target/glow91").unwrap();std::fs::write(format!("target/glow91/glow-{size}-{rad}.png"),&rendered.bytes).unwrap();
        let pixels=image::load_from_memory(&rendered.bytes).unwrap().to_rgba8();let per_point=pixels.width() as f32/(6000000./12700.);
        // The stem's middle row: alpha of the glow from the stem's left edge outwards.
        let rows:Vec<u32>=pixels.enumerate_pixels().filter(|(_,_,q)|q[0]<60&&q[1]<60).map(|(_,y,_)|y).collect();let y=(rows.iter().min().unwrap()+rows.iter().max().unwrap())/2;let row:Vec<f32>=(0..pixels.width()).map(|x|1.-pixels.get_pixel(x,y)[0] as f32/255.).collect();
        let ink=(0..pixels.width()).find(|&x|{let q=pixels.get_pixel(x,y);q[0]<60&&q[1]<60}).expect("the stem");
        let profile:Vec<f32>=(0..ink).rev().map(|x|row[x as usize]).collect();
        let at=|t:f32|profile.iter().position(|a|*a<t).unwrap_or(profile.len()) as f32/per_point;
        eprintln!("glow91 {size}pt rad {rad}pt: 50% {:.2}pt (PowerPoint {d50}), 10% {:.2}pt ({d10}), 2% {:.2}pt ({d02}), {per_point:.2} px/pt",at(0.5),at(0.1),at(0.02));
        let steepest=profile.windows(2).map(|w|(w[0]-w[1]).abs()).fold(0.,f32::max);
        assert!(profile.windows(2).all(|w|w[1]<=w[0]+0.02),"the glow only falls away from the stem: {profile:?}");
        // A Gaussian edge of deviation sigma falls at most 1/(sigma*sqrt(2*pi)) per point: no step beyond that (and a rounding).
        let sigma=(d10-d50)/1.28;let smooth=1./(sigma*2.5066*per_point);
        assert!(steepest<=smooth*1.5+0.03,"no step across the edge: {steepest} per pixel, a smooth edge {smooth}");
        for (got,want,name) in [(at(0.5),d50,"50%"),(at(0.1),d10,"10%"),(at(0.02),d02,"2%")]{
            assert!((got-want).abs()<=want*0.05+1.0/per_point,"{size}pt rad {rad}pt: the {name} distance is {got:.2}pt, PowerPoint's {want}pt");
        }
    }
}
