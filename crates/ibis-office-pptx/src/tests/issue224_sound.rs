// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #224: the sound of a slide's transition (`p:sndAc`, the wav the file embeds) is played in the slide show when the slide comes in, stopped by the next sound or `p:endSnd`,
//! and when the show ends.
use crate::ext::*;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{Command, MediaFormat, Transition, TransitionSound};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("pptx::issue224_the_transition_sound_plays_in_the_show_and_stops", plays_and_stops as fn())]
}

fn playing(s: &mut Scenario) -> bool {
    s.with(|e, _| e.pptx().is_some_and(|p| p.transition_sound.is_some()))
}

fn plays_and_stops() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let wav = std::fs::read(crate::shell::scenario::repo_root().join("test/editing/clip-tone.wav")).expect("a wav file");
    let ids: Vec<_> = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect());
    assert!(ids.len() >= 4);
    // Slide 2 comes in with a sound (the wav embedded), slide 3 with none, slide 4 stops it.
    s.update(|e, _, cx| {
        let media = e.document.presentation_mut().add_media(MediaFormat::Wav, wav.clone());
        let transition = |sound: Option<TransitionSound>| Some(Transition { effect: "fade".into(), direction: None, duration: 500, on_click: true, after: None, sound });
        let mut two = e.presentation().slide(ids[1]).unwrap().clone();
        two.transition = transition(Some(TransitionSound::Play { name: "Tone".into(), loop_sound: false }));
        two.transition_sound = Some(media);
        let mut three = e.presentation().slide(ids[2]).unwrap().clone();
        three.transition = transition(None);
        let mut four = e.presentation().slide(ids[3]).unwrap().clone();
        four.transition = transition(Some(TransitionSound::Stop));
        e.run(vec![Command::SetSlide { slide: two }, Command::SetSlide { slide: three }, Command::SetSlide { slide: four }], cx)
    });
    s.page(1);
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
    assert!(!playing(&mut s), "the first slide of the show comes in with no transition");
    s.key("space");
    s.settle();
    assert!(playing(&mut s), "slide 2's sound plays");
    s.key("space");
    s.settle();
    assert!(playing(&mut s), "slide 3 has no sound action: the sound goes on");
    s.key("space");
    s.settle();
    assert!(!playing(&mut s), "slide 4 stops it");
    for _ in 0..3 {
        s.key("left");
    }
    s.settle();
    assert!(!playing(&mut s), "going back plays no transition sound (and the one before is stopped by slide 4 already)");
    s.key("space");
    s.settle();
    assert!(playing(&mut s), "slide 2 again: it plays again");
    s.update(|e, w, cx| e.end_slideshow(w, cx));
    assert!(!playing(&mut s), "the end of the show stops it");
}
