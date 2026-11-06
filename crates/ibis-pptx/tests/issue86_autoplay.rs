// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue 86: a video or an audio PowerPoint writes with the start "automatically" is read as
//! started with the slide (an `afterEffect` / `withEffect` `mediacall` with `playFrom(0.0)`).
use ibis_doc_model::{MediaStart, ShapeKind};
use ibis_ooxml::{Package, SaveOptions};

const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const REL_VIDEO: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/video";
const REL_AUDIO: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/audio";

/// The `p:timing` PowerPoint writes for a clip with the given start: the play call in the main sequence, the media node.
fn timing(audio: bool, node_type: &str, outer_cond: &str) -> String {
    let tag = if audio { "audio" } else { "video" };
    format!(
        r#"<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst><p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst><p:par><p:cTn id="3" fill="hold"><p:stCondLst>{outer_cond}</p:stCondLst><p:childTnLst><p:par><p:cTn id="4" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="5" presetID="1" presetClass="mediacall" presetSubtype="0" fill="hold" nodeType="{node_type}"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:cmd type="call" cmd="playFrom(0.0)"><p:cBhvr><p:cTn id="6" dur="36106" fill="hold"/><p:tgtEl><p:spTgt spid="5"/></p:tgtEl></p:cBhvr></p:cmd></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn><p:prevCondLst><p:cond evt="onPrev" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst><p:nextCondLst><p:cond evt="onNext" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq><p:{tag}><p:cMediaNode vol="80000"><p:cTn id="7" fill="hold" display="0"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid="5"/></p:tgtEl></p:cMediaNode></p:{tag}></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"#
    )
}

fn start_of(audio: bool, node_type: &str, outer_cond: &str) -> MediaStart {
    let mut p = Package::open(ibis_pptx::blank_file(ibis_pptx::Kind::Presentation).unwrap()).unwrap();
    let part = "ppt/slides/slide1.xml";
    let poster = p.add_relationship(part, REL_IMAGE, "../media/poster.png", false).unwrap();
    let (kind, file) = if audio { (REL_AUDIO, "../media/clip.mp3") } else { (REL_VIDEO, "../media/clip.mp4") };
    let link = p.add_relationship(part, kind, file, false).unwrap();
    let element = if audio { "audioFile" } else { "videoFile" };
    let pic = format!(
        r#"<p:pic><p:nvPicPr><p:cNvPr id="5" name="Clip"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr><a:{element} r:link="{link}"/></p:nvPr></p:nvPicPr><p:blipFill><a:blip r:embed="{poster}"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2000000" cy="1000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#
    );
    let xml = format!(
        r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{pic}</p:spTree></p:cSld>{}</p:sld>"#,
        timing(audio, node_type, outer_cond)
    );
    p.set_bytes(part, xml.into_bytes());
    p.set_bytes("ppt/media/poster.png", b"fixture poster".to_vec());
    p.set_override("ppt/media/poster.png", "image/png").unwrap();
    let (name, ct) = if audio { ("ppt/media/clip.mp3", "audio/mpeg") } else { ("ppt/media/clip.mp4", "video/mp4") };
    p.set_bytes(name, b"fixture clip".to_vec());
    p.set_override(name, ct).unwrap();
    let model = ibis_pptx::read(&p.save(SaveOptions::default()).unwrap()).unwrap();
    let pic = model.slides[0].shapes.iter().find_map(|s| match &s.kind {
        ShapeKind::Picture(pic) if pic.clip.is_some() => Some(pic),
        _ => None,
    });
    pic.expect("a clip").clip.as_ref().unwrap().options.start
}

/// What PowerPoint writes for 開始: 自動 (the call runs after the previous effect, the slide's first group starts itself).
const AUTO_COND: &str = r#"<p:cond delay="indefinite"/><p:cond evt="onBegin" delay="0"><p:tn val="2"/></p:cond>"#;
const CLICK_COND: &str = r#"<p:cond delay="indefinite"/>"#;

#[test]
fn powerpoint_automatic_video_and_audio_start_with_the_slide() {
    for audio in [false, true] {
        assert_eq!(start_of(audio, "afterEffect", AUTO_COND), MediaStart::Automatically, "audio={audio} afterEffect");
        assert_eq!(start_of(audio, "withEffect", AUTO_COND), MediaStart::Automatically, "audio={audio} withEffect");
        assert_eq!(start_of(audio, "clickEffect", CLICK_COND), MediaStart::OnClick, "audio={audio} clickEffect");
    }
}
