// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The 18 sounds a slide's transition can play (画面切り替えの音: アプライズ… of PowerPoint's list), shipped with the desktop apps as Ogg Vorbis (`sounds/<author>/<id>.ogg`: CC0
//! sounds of Kenney and OpenGameArt's authors and five of ibis inc.'s own; see each folder's README.md and `scripts/transition-sounds/build.py`) and given back as WAV
//! (what PowerPoint embeds in a `.pptx`). The browser version carries none of them (it fetches them when they are needed: `resource-list.txt`).

/// The id of a sound by its name in the menu ("Cash Register" is `cash-register`).
pub fn id_of(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace(' ', "-")
}

/// The Ogg Vorbis file of the sound called `name`, when this build carries it (or, in the browser, when the page has fetched it: `provide`).
pub fn ogg(name: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
    let id = id_of(name);
    if let Some(bytes) = ogg_of_id(&id) {
        return Some(std::borrow::Cow::Borrowed(bytes));
    }
    fetched().lock().ok()?.get(&id).map(|b| std::borrow::Cow::Owned(b.clone()))
}

fn fetched() -> &'static std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>> {
    static FETCHED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>> = std::sync::OnceLock::new();
    FETCHED.get_or_init(Default::default)
}

/// Keep the Ogg Vorbis file `bytes` of the sound `id` that was fetched (the browser version carries none; `ibis_office_shell::resource_list` says where they are).
pub fn provide(id: &str, bytes: Vec<u8>) {
    if let Ok(mut map) = fetched().lock() {
        map.insert(id.to_string(), bytes);
    }
}

macro_rules! sounds {
    ($($id:literal => $path:literal),* $(,)?) => {
        #[cfg(not(target_family = "wasm"))]
        fn ogg_of_id(id: &str) -> Option<&'static [u8]> {
            match id {
                $($id => Some(include_bytes!($path) as &'static [u8]),)*
                _ => None,
            }
        }
        #[cfg(target_family = "wasm")]
        fn ogg_of_id(_: &str) -> Option<&'static [u8]> {
            None
        }
        /// The ids of the sounds.
        pub const IDS: &[&str] = &[$($id),*];
    };
}

sounds! {
    "applause" => "../sounds/opengameart/applause.ogg",
    "arrow" => "../sounds/ibis/arrow.ogg",
    "bomb" => "../sounds/kenney/bomb.ogg",
    "breeze" => "../sounds/ibis/breeze.ogg",
    "camera" => "../sounds/kenney/camera.ogg",
    "cash-register" => "../sounds/ibis/cash-register.ogg",
    "chime" => "../sounds/kenney/chime.ogg",
    "click" => "../sounds/kenney/click.ogg",
    "coin" => "../sounds/kenney/coin.ogg",
    "drum-roll" => "../sounds/ibis/drum-roll.ogg",
    "explosion" => "../sounds/kenney/explosion.ogg",
    "hammer" => "../sounds/kenney/hammer.ogg",
    "laser" => "../sounds/kenney/laser.ogg",
    "push" => "../sounds/kenney/push.ogg",
    "suction" => "../sounds/kenney/suction.ogg",
    "typewriter" => "../sounds/opengameart/typewriter.ogg",
    "voltage" => "../sounds/kenney/voltage.ogg",
    "wind" => "../sounds/ibis/wind.ogg",
}

/// Ogg Vorbis bytes decoded to 16-bit mono PCM and its sample rate (the channels averaged).
pub fn decode_ogg(bytes: &[u8]) -> Option<(Vec<i16>, u32)> {
    use lewton::inside_ogg::OggStreamReader;
    let mut reader = OggStreamReader::new(std::io::Cursor::new(bytes)).ok()?;
    let channels = usize::from(reader.ident_hdr.audio_channels).max(1);
    let rate = reader.ident_hdr.audio_sample_rate;
    let mut out: Vec<i16> = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl().ok()? {
        for frame in packet.chunks_exact(channels) {
            out.push((frame.iter().map(|&s| i32::from(s)).sum::<i32>() / channels as i32) as i16);
        }
    }
    Some((out, rate))
}

/// A WAV file (RIFF PCM, 16-bit, mono) of `samples` at `rate`.
pub fn wav_bytes(samples: &[i16], rate: u32) -> Vec<u8> {
    let data = samples.len() * 2;
    let mut w = Vec::with_capacity(44 + data);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&((36 + data) as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(data as u32).to_le_bytes());
    for s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

/// The sound called `name` as a WAV file, for embedding in a `.pptx` and for the player.
pub fn wav(name: &str) -> Option<Vec<u8>> {
    let (samples, rate) = decode_ogg(&ogg(name)?)?;
    Some(wav_bytes(&samples, rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_of_the_menu_is_there_as_a_wav_of_a_second_or_so() {
        for name in ["Applause", "Arrow", "Bomb", "Breeze", "Camera", "Cash Register", "Chime", "Click", "Coin", "Drum Roll", "Explosion", "Hammer", "Laser", "Push", "Suction", "Typewriter", "Voltage", "Wind"] {
            #[cfg(not(target_family = "wasm"))]
            {
                let wav = wav(name).unwrap_or_else(|| panic!("{name}"));
                assert_eq!(&wav[..4], b"RIFF");
                let rate = u32::from_le_bytes(wav[24..28].try_into().unwrap());
                let seconds = (wav.len() - 44) as f64 / (f64::from(rate) * 2.0);
                assert!((0.03..=3.1).contains(&seconds), "{name}: {seconds} s");
                // Not silent.
                assert!(wav[44..].chunks_exact(2).any(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs() > 500), "{name} is silent");
            }
        }
        assert_eq!(IDS.len(), 18);
        assert_eq!(id_of("Cash Register"), "cash-register");
    }
}
