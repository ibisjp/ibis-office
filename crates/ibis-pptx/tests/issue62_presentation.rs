// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use ibis_doc_model::{AnimationLook, Color, EmuRect, Fill, Geometry, LayoutId, PlaceholderRef, Shape, ShapeId, ShapeKind, SlidePlaceholder};
use ibis_ooxml::{Package, SaveOptions};
fn package()->Package{Package::open(ibis_pptx::blank_file(ibis_pptx::Kind::Presentation).unwrap()).unwrap()}
fn slide(tree:&str,timing:&str)->String{format!(r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{tree}</p:spTree></p:cSld>{timing}</p:sld>"#)}
const TEXT:&str=r#"<p:sp><p:nvSpPr><p:cNvPr id="7" name="Zoom target"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="100000" y="100000"/><a:ext cx="2000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Zoom</a:t></a:r></a:p></p:txBody></p:sp>"#;
#[test]
fn absent_background_does_not_use_an_unreferenced_theme_fill(){
    let mut p=package();let parts:Vec<_>=p.names().filter(|n|(n.starts_with("ppt/slides/")||n.starts_with("ppt/slideLayouts/")||n.starts_with("ppt/slideMasters/"))&&n.ends_with(".xml")).map(str::to_string).collect();
    for part in parts{let d=p.xml_mut(&part).unwrap();let root=d.root_element().unwrap();if let Some(cs)=d.child(root,"cSld"){if let Some(bg)=d.child(cs,"bg"){d.detach(bg);}}}
    let theme=p.names().find(|n|n.starts_with("ppt/theme/theme")&&n.ends_with(".xml")).unwrap().to_owned();
    let mut xml=String::from_utf8(p.read(&theme).unwrap()).unwrap();let a=xml.find("<a:bgFillStyleLst>").unwrap()+"<a:bgFillStyleLst>".len();let b=xml.find("</a:bgFillStyleLst>").unwrap();xml.replace_range(a..b,r#"<a:solidFill><a:srgbClr val="0000FF"/></a:solidFill>"#);p.set_bytes(&theme,xml.into_bytes());
    let read=ibis_pptx::read(&p.save(SaveOptions::default()).unwrap()).unwrap();assert_eq!(read.slides[0].background,Fill::Solid(Color::WHITE));
}
#[test]
fn powerpoint_zoom_23_reads_and_writes_as_zoom(){
    let mut p=package();let timing=r#"<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" nodeType="tmRoot"><p:childTnLst><p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst><p:par><p:cTn id="3" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="4" presetID="23" presetClass="entr" presetSubtype="0" nodeType="clickEffect"><p:childTnLst><p:animScale><p:cBhvr><p:cTn id="5" dur="400"/><p:tgtEl><p:spTgt spid="7"/></p:tgtEl></p:cBhvr><p:from x="0" y="0"/><p:to x="100000" y="100000"/></p:animScale></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"#;
    p.set_bytes("ppt/slides/slide1.xml",slide(TEXT,timing).into_bytes());let opened=ibis_pptx::open(p.save(SaveOptions::default()).unwrap()).unwrap();let mut model=opened.presentation.clone();assert_eq!(model.slides[0].animations[0].look,AnimationLook::Zoom);model.slides[0].animations[0].duration=700;
    let bytes=opened.source.save(&model,ibis_pptx::SaveOptions::default()).unwrap();let pkg=Package::open(bytes.clone()).unwrap();let xml=String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();assert!(xml.contains("presetID=\"23\""));assert_eq!(ibis_pptx::read(&bytes).unwrap().slides[0].animations[0].look,AnimationLook::Zoom);
}
#[test]
fn powerpoint_zoom_53_from_the_issue_file_reads_as_zoom_not_fade(){
    // Timing written by PowerPoint for ズーム (ppt_w/ppt_h grow together with a fade).
    let mut p=package();let timing=r##"<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" nodeType="tmRoot"><p:childTnLst><p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst><p:par><p:cTn id="3" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="4" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="5" presetID="53" presetClass="entr" presetSubtype="528" fill="hold" nodeType="clickEffect"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:cTn id="6" dur="1" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid="7"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val="visible"/></p:to></p:set><p:anim calcmode="lin" valueType="num"><p:cBhvr><p:cTn id="7" dur="500" fill="hold"/><p:tgtEl><p:spTgt spid="7"/></p:tgtEl><p:attrNameLst><p:attrName>ppt_w</p:attrName></p:attrNameLst></p:cBhvr><p:tavLst><p:tav tm="0"><p:val><p:fltVal val="0"/></p:val></p:tav><p:tav tm="100000"><p:val><p:strVal val="#ppt_w"/></p:val></p:tav></p:tavLst></p:anim><p:animEffect transition="in" filter="fade"><p:cBhvr><p:cTn id="8" dur="500"/><p:tgtEl><p:spTgt spid="7"/></p:tgtEl></p:cBhvr></p:animEffect></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"##;
    p.set_bytes("ppt/slides/slide1.xml",slide(TEXT,timing).into_bytes());
    let model=ibis_pptx::read(&p.save(SaveOptions::default()).unwrap()).unwrap();
    assert_eq!(model.slides[0].animations[0].look,AnimationLook::Zoom);assert_eq!(model.slides[0].animations[0].duration,500);
}
#[test]
fn blank_layout_removes_empty_placeholders_but_keeps_entered_content(){
    let mut p=ibis_pptx::read(&ibis_pptx::blank_file(ibis_pptx::Kind::Presentation).unwrap()).unwrap();let mut blank=p.layouts[0].clone();blank.id=LayoutId(9999);blank.placeholders.clear();let id=blank.id;p.layouts.push(blank);
    let slide=&mut p.slides[0];slide.shapes.clear();slide.placeholders.clear();for (i,text)in [(10,""),(11,"Keep my title")]{let id=ShapeId(i);slide.shapes.push(Shape::text_box(id,EmuRect::new(0,0,2000000,1000000),text));slide.placeholders.insert(id,SlidePlaceholder{placeholder:PlaceholderRef{kind:Some("title".into()),index:None},own_frame:false});}
    let changed=p.with_layout(p.slides[0].id,id).unwrap();assert_eq!(changed.shapes.len(),1);assert_eq!(changed.shapes[0].text_body().unwrap().text(),"Keep my title");assert!(changed.placeholders.is_empty());assert_eq!(p.slides[0].shapes.len(),2);
}
#[test]
fn custom_picture_crop_and_legacy_internal_video_are_read(){
    let mut p=package();
    let picture=r#"<p:pic><p:nvPicPr><p:cNvPr id="8" name="Clipped video"/><p:cNvPicPr/><p:nvPr><a:videoFile r:link="rVideo"/></p:nvPr></p:nvPicPr><p:blipFill><a:blip r:embed="rPoster"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2000000" cy="1000000"/></a:xfrm><a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l="0" t="0" r="r" b="b"/><a:pathLst><a:path w="100" h="100"><a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:lnTo><a:pt x="100" y="0"/></a:lnTo><a:lnTo><a:pt x="50" y="100"/></a:lnTo><a:close/></a:path></a:pathLst></a:custGeom></p:spPr></p:pic>"#;
    let poster=p.add_relationship("ppt/slides/slide1.xml","http://schemas.openxmlformats.org/officeDocument/2006/relationships/image","../media/poster.png",false).unwrap();
    let video=p.add_relationship("ppt/slides/slide1.xml","http://schemas.openxmlformats.org/officeDocument/2006/relationships/video","../media/clip.mp4",false).unwrap();
    p.set_bytes("ppt/slides/slide1.xml",slide(&picture.replace("rPoster",&poster).replace("rVideo",&video),"").into_bytes());
    // The model reader retains media bytes; decoding is covered by native playback tests.
    p.set_bytes("ppt/media/poster.png",b"fixture poster".to_vec());p.set_override("ppt/media/poster.png","image/png").unwrap();p.set_bytes("ppt/media/clip.mp4",b"fixture video".to_vec());p.set_override("ppt/media/clip.mp4","video/mp4").unwrap();
    let model=ibis_pptx::read(&p.save(SaveOptions::default()).unwrap()).unwrap();let ShapeKind::Picture(pic)=&model.slides[0].shapes[0].kind else{panic!("a picture")};assert!(matches!(pic.geometry,Some(Geometry::Formula(_))));let clip=pic.clip.as_ref().expect("legacy embedded video");assert!(clip.link.is_none());assert_eq!(&*model.media(clip.media).unwrap().data,b"fixture video");
}
#[test]
fn blank_layout_conversion_survives_save_and_reopen(){
    let opened=ibis_pptx::open(ibis_pptx::blank_file(ibis_pptx::Kind::Presentation).unwrap()).unwrap();let mut model=opened.presentation.clone();
    let blank=model.layouts.iter().find(|l|l.placeholders.iter().all(|p|!matches!(p.placeholder.normalized(),"title"|"body"))).expect("blank layout").id;
    let original=&mut model.slides[0].shapes[0];let text=Shape::text_box(original.id,original.frame,"Keep entered text");*original.text_body_mut().unwrap()=text.text_body().unwrap().clone();
    let changed=opened.source.with_layout(&model,model.slides[0].id,blank).unwrap();assert!(changed.placeholders.is_empty());assert_eq!(changed.shapes.len(),1);let frame=changed.shapes[0].frame;model.slides[0]=changed;
    let bytes=opened.source.save(&model,ibis_pptx::SaveOptions::default()).unwrap();let reread=ibis_pptx::read(&bytes).unwrap();assert!(reread.slides[0].placeholders.is_empty());assert_eq!(reread.slides[0].shapes.len(),1);assert_eq!(reread.slides[0].shapes[0].frame,frame);assert_eq!(reread.slides[0].shapes[0].text_body().unwrap().text(),"Keep entered text");
}
#[test]
fn use_bg_fill_shape_takes_the_slide_background_not_its_style_fill(){
    // Slide 1 of the issue file: a full-slide rectangle with useBgFill="1" and an
    // accent1 fillRef in its style. PowerPoint shows the background (white here).
    let rect=r#"<p:sp useBgFill="1"><p:nvSpPr><p:cNvPr id="28" name="Rectangle 27"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:ln><a:noFill/></a:ln></p:spPr><p:style><a:lnRef idx="2"><a:schemeClr val="accent1"/></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></p:style></p:sp>"#;
    let xml=slide(rect,"").replace("<p:cSld>",r#"<p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="336699"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>"#);
    let mut p=package();p.set_bytes("ppt/slides/slide1.xml",xml.into_bytes());
    let model=ibis_pptx::read(&p.save(SaveOptions::default()).unwrap()).unwrap();
    let background=model.slides[0].background.clone();
    assert!(matches!(background,Fill::Solid(_)));
    let ShapeKind::Auto(auto)=&model.slides[0].shapes[0].kind else{panic!("an auto shape")};
    assert_eq!(auto.fill,background);
}
