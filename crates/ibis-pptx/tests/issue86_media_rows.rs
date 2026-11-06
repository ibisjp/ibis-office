// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue 86: the calls that play, pause or stop a video or an audio (`presetClass="mediacall"`) are rows of the
//! slide's animations (the animation pane lists them, the slide show plays them on its steps), and they are saved
//! as the file had them.
use ibis_doc_model::{AnimationClass, AnimationLook, AnimationStart, AnimationTrigger, MediaCommand};
use ibis_ooxml::{Package, SaveOptions};

fn layout_test() -> Option<Vec<u8>> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/github-issue/62/layout_test.pptx")).ok()
}

fn slide_xml(bytes: &[u8], n: usize) -> String {
    String::from_utf8(Package::open(bytes.to_vec()).unwrap().read(&format!("ppt/slides/slide{n}.xml")).unwrap()).unwrap()
}

#[test]
fn powerpoints_video_has_a_play_row_and_a_pause_row_by_click_on_it() {
    let Some(bytes) = layout_test() else { return };
    let opened = ibis_pptx::open(bytes).unwrap();
    let slide = &opened.presentation.slides[1];
    let rows: Vec<_> = slide.animations.iter().filter(|a| a.is_media()).collect();
    assert_eq!(rows.len(), 2, "{:?}", slide.animations);
    assert_eq!(rows[0].class, AnimationClass::Media);
    assert_eq!(rows[0].look, AnimationLook::Media(MediaCommand::Play));
    assert_eq!(rows[0].start, AnimationStart::OnClick);
    assert!(rows[0].trigger.is_none());
    assert_eq!(rows[0].duration, 36106);
    assert_eq!(rows[1].look, AnimationLook::Media(MediaCommand::Pause));
    assert!(matches!(rows[1].trigger, AnimationTrigger::OnShapeClick(s) if s == rows[1].shape));
}

#[test]
fn powerpoints_audio_row_comes_after_the_text_effects() {
    let Some(bytes) = layout_test() else { return };
    let opened = ibis_pptx::open(bytes).unwrap();
    let slide = &opened.presentation.slides[2];
    let looks: Vec<_> = slide.animations.iter().map(|a| a.class).collect();
    assert_eq!(looks.last(), Some(&AnimationClass::Media), "{looks:?}");
    assert_eq!(slide.animations.iter().filter(|a| a.is_media()).count(), 1);
    assert!(slide.animations.len() >= 2);
}

#[test]
fn the_media_rows_are_saved_as_the_file_had_them_even_when_the_order_changes() {
    let Some(bytes) = layout_test() else { return };
    let opened = ibis_pptx::open(bytes).unwrap();
    // Unchanged: the timing is the file's.
    let same = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
    assert_eq!(slide_xml(&same, 2).matches("playFrom(0.0)").count(), 1);
    assert_eq!(slide_xml(&same, 2).matches("togglePause").count(), 1);
    // The audio row moved to the front: the timing is rebuilt, the call stays and comes first.
    let mut model = opened.presentation.clone();
    let slide = &mut model.slides[2];
    let media = slide.animations.pop().unwrap();
    assert!(media.is_media());
    slide.animations.insert(0, media);
    let moved = opened.source.save(&model, SaveOptions::default()).unwrap();
    let xml = slide_xml(&moved, 3);
    let call = xml.find("playFrom(0.0)").expect("the call is kept");
    let text = xml.find("<p:pRg st=\"0\"").expect("the text effect is kept");
    assert!(call < text, "the audio's group first: {xml}");
    let again = ibis_pptx::open(moved).unwrap();
    assert_eq!(again.presentation.slides[2].animations[0].look, AnimationLook::Media(MediaCommand::Play));
    // Its row deleted: the call goes with it.
    let mut model = opened.presentation.clone();
    model.slides[2].animations.retain(|a| !a.is_media());
    let gone = opened.source.save(&model, SaveOptions::default()).unwrap();
    assert!(!slide_xml(&gone, 3).contains("playFrom"), "{}", slide_xml(&gone, 3));
}
