// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue 62: a video or an audio put on a slide is played by the slide's timing
//! (a `mediacall` effect, a `p:video` / `p:audio` with a `p:cMediaNode`), as PowerPoint writes it.
use std::collections::HashSet;

use ibis_doc_model::{Animation, AnimationAfterEffect, AnimationClass, AnimationLook, AnimationRepeat, AnimationStart, AnimationTrigger, Clip, ClipOptions, EmuRect, MediaFormat, MediaStart, MediaVolume, Picture, Shape, ShapeId, ShapeKind};
use ibis_ooxml::Package;

/// An MP4 header with a time scale of 600 and 3600 units: 6 seconds.
fn mp4() -> Vec<u8> {
    let mut v = vec![0, 0, 0, 0x6c];
    v.extend_from_slice(b"mvhd");
    v.extend_from_slice(&[0; 12]);
    v.extend_from_slice(&600u32.to_be_bytes());
    v.extend_from_slice(&3600u32.to_be_bytes());
    v
}

fn picture(model: &mut ibis_doc_model::Presentation, name: &str, audio: bool, options: ClipOptions) -> ShapeId {
    let poster = model.add_media(MediaFormat::Png, vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    let file = if audio { model.add_media(MediaFormat::M4a, mp4()) } else { model.add_media(MediaFormat::Mp4, mp4()) };
    let id = ShapeId(model.slides[0].shapes.iter().map(|s| s.id.0).max().unwrap_or(0) + 1);
    let pic = Picture {
        media: poster, crop: Default::default(), stroke: None, corner: 0, duotone: None, clear: None, luminance: None,
        clip: Some(Clip { media: file, audio, options, link: None }), shadow: None, reflection: None, soft_edge: None,
        alt_text: Default::default(), decorative: false, name: name.to_string(), geometry: None, effects: Default::default(),
        tile: None, fill_rect: None, artistic_effect: None,
    };
    model.slides[0].shapes.push(Shape::new(id, EmuRect::new(100_000, 100_000, 3_000_000, 2_000_000), ShapeKind::Picture(pic)));
    id
}

fn slide_xml(bytes: &[u8]) -> String {
    String::from_utf8(Package::open(bytes.to_vec()).unwrap().read("ppt/slides/slide1.xml").unwrap()).unwrap()
}

fn assert_unique_ids(xml: &str) {
    let mut seen = HashSet::new();
    for part in xml.split("<p:cTn id=\"").skip(1) {
        let id = part.split('"').next().unwrap();
        assert!(seen.insert(id.to_string()), "cTn id {id} twice: {xml}");
    }
}

fn open_blank() -> ibis_pptx::Opened {
    ibis_pptx::open(ibis_pptx::blank_file(ibis_pptx::Kind::Presentation).unwrap()).unwrap()
}

#[test]
fn a_new_video_gets_the_play_call_the_media_node_and_the_pause_by_click() {
    let opened = open_blank();
    let mut model = opened.presentation.clone();
    let id = picture(&mut model, "Movie", false, ClipOptions::default());
    let bytes = opened.source.save(&model, ibis_pptx::SaveOptions::default()).unwrap();
    let xml = slide_xml(&bytes);
    let spid = xml.split("<p:cNvPr id=\"").skip(1).find(|p| p.contains("name=\"Movie\"")).unwrap().split('"').next().unwrap().to_string();
    // PowerPoint's own pieces.
    assert!(xml.contains("<a:videoFile r:link="), "{xml}");
    assert!(xml.contains("<p14:media "), "{xml}");
    assert!(xml.contains(&format!("presetClass=\"mediacall\" presetSubtype=\"0\" fill=\"hold\" nodeType=\"clickEffect\"")), "{xml}");
    assert!(xml.contains(&format!("<p:cmd type=\"call\" cmd=\"playFrom(0.0)\"><p:cBhvr><p:cTn id=\"")), "{xml}");
    assert!(xml.contains("dur=\"6000\" fill=\"hold\"/>"), "the length of the movie is the length of the call: {xml}");
    assert!(xml.contains(&format!("<p:video><p:cMediaNode vol=\"100000\"><p:cTn id=")), "{xml}");
    assert!(xml.contains(&format!("<p:cond evt=\"onClick\" delay=\"0\"><p:tgtEl><p:spTgt spid=\"{spid}\"/>")), "{xml}");
    assert!(xml.contains("cmd=\"togglePause\""), "{xml}");
    // The options are not attributes of `p14:media`.
    let media = xml.split("<p14:media ").nth(1).unwrap().split("/>").next().unwrap();
    assert!(!media.contains("vol="), "{media}");
    assert_unique_ids(&xml);
    // Read back.
    let reread = ibis_pptx::read(&bytes).unwrap();
    let ShapeKind::Picture(pic) = &reread.slides[0].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Picture(p) if p.clip.is_some())).unwrap().kind else { panic!() };
    assert_eq!(pic.clip.as_ref().unwrap().options.start, MediaStart::OnClick);
    let _ = id;
}

#[test]
fn an_audio_started_with_the_slide_looped_and_quiet_round_trips_through_the_timing() {
    let opened = open_blank();
    let mut model = opened.presentation.clone();
    let options = ClipOptions { start: MediaStart::Automatically, volume: MediaVolume::Low, loop_until_stopped: true, rewind: true, hide_during_show: true, ..Default::default() };
    picture(&mut model, "Sound", true, options.clone());
    let bytes = opened.source.save(&model, ibis_pptx::SaveOptions::default()).unwrap();
    let xml = slide_xml(&bytes);
    assert!(xml.contains("<p:cond delay=\"indefinite\"/><p:cond evt=\"onBegin\" delay=\"0\"><p:tn val=\"2\"/></p:cond>"), "{xml}");
    assert!(xml.contains("nodeType=\"afterEffect\""), "{xml}");
    assert!(xml.contains("<p:audio><p:cMediaNode vol=\"33000\" showWhenStopped=\"0\"><p:cTn id="), "{xml}");
    assert!(xml.contains("repeatCount=\"indefinite\""), "{xml}");
    assert!(xml.contains("evt=\"onStopAudio\""), "{xml}");
    assert!(!xml.contains("togglePause"), "an audio has no pause by click: {xml}");
    assert_unique_ids(&xml);
    let reread = ibis_pptx::read(&bytes).unwrap();
    let ShapeKind::Picture(pic) = &reread.slides[0].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Picture(p) if p.clip.is_some())).unwrap().kind else { panic!() };
    let clip = pic.clip.as_ref().unwrap();
    assert!(clip.audio);
    assert_eq!(clip.options.start, options.start);
    assert_eq!(clip.options.volume, options.volume);
    assert!(clip.options.loop_until_stopped && clip.options.rewind && clip.options.hide_during_show);
}

#[test]
fn the_play_survives_a_change_of_the_animations_of_the_slide() {
    let opened = open_blank();
    let mut model = opened.presentation.clone();
    picture(&mut model, "Movie", false, ClipOptions::default());
    let first = opened.source.save(&model, ibis_pptx::SaveOptions::default()).unwrap();
    // Open what was saved and give the slide an animation of its own.
    let again = ibis_pptx::open(first).unwrap();
    let mut model = again.presentation.clone();
    let target = model.slides[0].shapes.iter().find(|s| matches!(&s.kind, ShapeKind::Picture(p) if p.clip.is_none())).map(|s| s.id).or_else(|| model.slides[0].shapes.first().map(|s| s.id)).unwrap();
    model.slides[0].animations.push(Animation {
        shape: target, class: AnimationClass::Entrance, start: AnimationStart::OnClick, look: AnimationLook::Fade, delay: 0, duration: 500, preset: (10, 0),
        repeat_count: AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: AnimationAfterEffect::Hold,
        trigger: AnimationTrigger::None, paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None,
    });
    let bytes = again.source.save(&model, ibis_pptx::SaveOptions::default()).unwrap();
    let xml = slide_xml(&bytes);
    assert!(xml.contains("presetClass=\"entr\""), "{xml}");
    assert!(xml.contains("cmd=\"playFrom(0.0)\"") && xml.contains("<p:video>") && xml.contains("cmd=\"togglePause\""), "the play was lost: {xml}");
    assert_unique_ids(&xml);
    // And a second round does not add a second play.
    let third = ibis_pptx::open(bytes).unwrap();
    let saved = third.source.save(&third.presentation, ibis_pptx::SaveOptions::default()).unwrap();
    assert_eq!(slide_xml(&saved).matches("playFrom(0.0)").count(), 1);
}

/// The file of the issue (PowerPoint's video and audio): read as clips, and an edit of the animations keeps their play.
#[test]
fn the_issue_file_keeps_its_media_play_when_an_animation_is_edited() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/github-issue/62/layout/layout_test.pptx");
    let Ok(data) = std::fs::read(&path) else { return };
    let opened = ibis_pptx::open(data).unwrap();
    let mut model = opened.presentation.clone();
    for (index, audio) in [(1usize, false), (2usize, true)] {
        let clip = model.slides[index].shapes.iter().find_map(|s| if let ShapeKind::Picture(p) = &s.kind { p.clip.clone() } else { None }).expect("a clip");
        assert_eq!(clip.audio, audio);
        assert_eq!(clip.options.volume, MediaVolume::Medium, "vol=80000 is 80 %");
    }
    // Change the duration of an animation of the audio slide.
    model.slides[2].animations[0].duration = 700;
    let bytes = opened.source.save(&model, ibis_pptx::SaveOptions::default()).unwrap();
    let pkg = Package::open(bytes).unwrap();
    let xml = String::from_utf8(pkg.read("ppt/slides/slide3.xml").unwrap()).unwrap();
    assert!(xml.contains("cmd=\"playFrom(0.0)\"") && xml.contains("<p:audio>"), "the play was lost");
    assert_unique_ids(&xml);
}
