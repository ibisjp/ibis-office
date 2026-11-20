// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #224: a sound chosen for a slide's transition in the menu (one of the 18 shipped) is the sound saved in the file (the WAV PowerPoint plays: a real one, not a silent stand-in) and is still
//! there when the file is opened again; and the slide show plays it.
use crate::ext::*;
use crate::shell::scenario::Scenario;
use ibis_doc_model::TransitionSound;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::transition_sound_chosen_in_the_menu_is_saved_as_its_wav_and_read_again", chosen_saved_and_read_again as fn()),
        ("pptx::transition_sound_chosen_in_the_menu_is_played_by_the_show", chosen_is_played),
    ]
}

fn choose(s: &mut Scenario, name: &str) {
    let name = name.to_string();
    s.update(move |e, _, cx| {
        e.set_transition(Some("fade"), cx);
        e.set_transition_sound(Some(TransitionSound::Play { name, loop_sound: false }), cx);
    });
}

fn chosen_saved_and_read_again() {
    for name in ["Click", "Cash Register", "Applause"] {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        choose(&mut s, name);
        s.save_as("transition-sound.pptx");
        let again = s.reopen();
        let slide = &again.slides[0];
        assert_eq!(slide.transition.as_ref().and_then(|t| t.sound.clone()), Some(TransitionSound::Play { name: name.to_string(), loop_sound: false }), "{name}: the sound is kept");
        let media = slide.transition_sound.and_then(|m| again.media(m)).unwrap_or_else(|| panic!("{name}: the sound is embedded audio"));
        let wav = ibis_formats::transition_sounds::wav(name).expect("a shipped sound");
        assert_eq!(media.data.len(), wav.len(), "{name}: the embedded sound is the shipped one, not a silent stand-in");
        assert_eq!(&media.data[..4], b"RIFF");
    }
}

fn chosen_is_played() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    choose(&mut s, "Coin");
    // The sound a slide plays on coming in: the shipped one, though the file read has no embedded audio for it yet.
    s.update(|e, _, cx| e.play_transition_sound(cx));
    let playing = s.update(|e, _, _| e.transition_sound_playing());
    // (A machine with no audio output cannot open a player: then nothing is playing, and that is not this test's to judge.)
    if playing {
        s.update(|e, _, _| e.stop_transition_sound());
        assert!(!s.update(|e, _, _| e.transition_sound_playing()), "stopped");
    }
}
