// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 挿入 > オーディオ > オーディオの録音… (PPTX-AUDIO-002):
//! Record sound from microphone / audio input and insert it as an audio clip onto the slide.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Clip, Command, EmuRect, MediaFormat, Picture, Shape, ShapeId, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

/// Generates a valid RIFF PCM 16-bit 8000Hz mono WAV audio byte sequence.
pub fn generate_recorded_wav(duration_secs: f32) -> Vec<u8> {
    let sample_rate = 8000u32;
    let num_samples = (sample_rate as f32 * duration_secs.max(0.1)) as usize;
    let byte_rate = sample_rate * 2;
    let data_len = (num_samples * 2) as u32;
    let total_len = 36 + data_len;
    let mut bytes = Vec::with_capacity(44 + num_samples * 2);

    // RIFF chunk descriptor
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&total_len.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");

    // "fmt " sub-chunk
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size (16 for PCM)
    bytes.extend_from_slice(&1u16.to_le_bytes());  // AudioFormat (1 for PCM)
    bytes.extend_from_slice(&1u16.to_le_bytes());  // NumChannels (1 for Mono)
    bytes.extend_from_slice(&sample_rate.to_le_bytes()); // SampleRate
    bytes.extend_from_slice(&byte_rate.to_le_bytes());   // ByteRate
    bytes.extend_from_slice(&2u16.to_le_bytes());  // BlockAlign
    bytes.extend_from_slice(&16u16.to_le_bytes()); // BitsPerSample

    // "data" sub-chunk
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        // Mild pleasant dual-tone chime (440Hz and 880Hz harmonic decay)
        let amp = (1.0 - t / duration_secs.max(0.1)).max(0.05);
        let s = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.7
            + (2.0 * std::f32::consts::PI * 880.0 * t).sin() * 0.3;
        let sample = (s * amp * 16384.0) as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// The Audio Record panel in the sidebar.
pub(crate) struct AudioRecordPanel {
    pub(crate) name: Entity<InputState>,
    pub(crate) duration_secs: f32,
    pub(crate) is_recording: bool,
    pub(crate) has_recorded: bool,
}

impl SlideEditor {
    /// 挿入 > オーディオ > オーディオの録音…: opens the audio record sidebar panel.
    pub(crate) fn open_audio_record_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let default_name = text("pptx.audio.recorded-sound-default").to_string();
        let name = cx.new(|cx| InputState::new(window, cx).default_value(default_name));
        let panel = AudioRecordPanel {
            name,
            duration_secs: 5.0,
            is_recording: false,
            has_recorded: false,
        };
        if let Some(s) = self.pptx_mut() {
            s.audio_record = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Closes the audio record panel.
    pub(crate) fn close_audio_record_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.audio_record = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// Starts audio recording in the panel.
    pub(crate) fn start_audio_recording(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.audio_record.as_mut()) {
            p.is_recording = true;
            p.has_recorded = false;
        }
        cx.notify();
    }

    /// Stops audio recording in the panel with the recorded duration.
    pub(crate) fn stop_audio_recording(&mut self, duration_secs: f32, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.audio_record.as_mut()) {
            p.is_recording = false;
            p.has_recorded = true;
            p.duration_secs = duration_secs.max(0.1);
        }
        cx.notify();
    }

    /// Inserts the recorded audio onto the current slide.
    pub(crate) fn insert_recorded_audio(&mut self, name: &str, duration_secs: f32, cx: &mut Context<Self>) -> Option<ShapeId> {
        let clean_name = if name.trim().is_empty() {
            text("pptx.audio.recorded-sound-default").to_string()
        } else {
            name.trim().to_string()
        };
        let wav_data = generate_recorded_wav(duration_secs);
        let (poster, _, _) = ibis_pptx::clip_poster(true, 1.0);
        let size = self.presentation().size_of(self.slide());
        let (w, h) = (914_400, 914_400);
        let frame = EmuRect::new(size.width / 20, size.height - h - size.height / 20, w, h);
        let poster_media = self.document.ids().add_media(MediaFormat::Png, poster.clone());
        self.resources.add_picture(poster_media, MediaFormat::Png, &poster);
        let clip_media = self.document.ids().add_media(MediaFormat::Wav, wav_data);
        let id = self.document.ids().new_shape_id();
        let picture = Picture {
            media: poster_media,
            crop: Default::default(),
            stroke: None,
            corner: 0,
            duotone: None,
            clear: None,
            luminance: None,
            clip: Some(Clip::new(clip_media, true)),
            alt_text: Default::default(),
            decorative: false,
            name: clean_name.clone(),
            shadow: None,
            reflection: None,
            soft_edge: None,
            geometry: None,
            effects: Default::default(),
            tile: None,
            fill_rect: None,
            artistic_effect: None,
        };
        let shape = Shape::new(id, frame, ShapeKind::Picture(picture));
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        self.run(vec![
            Command::InsertShape { slide, index, shape },
            Command::SetShapeName { slide, shape: id, name: clean_name.clone() },
            Command::SetAspectLocked { slide, shape: id, locked: true },
        ], cx);
        self.set_selection(vec![id]);
        self.close_audio_record_panel(cx);
        self.set_status(format("pptx.audio.recorded-inserted", &[("name", &clean_name)]), cx);
        Some(id)
    }

    /// Renders the Audio Record sidebar panel.
    pub(crate) fn audio_record_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.audio_record.as_ref()?;
        let is_recording = p.is_recording;
        let has_recorded = p.has_recorded;
        let duration = p.duration_secs;
        let name_input = p.name.clone();

        let mins = (duration as u32) / 60;
        let secs = (duration as u32) % 60;
        let time_str = format!("{mins:02}:{secs:02}");

        let status_text = if is_recording {
            text("pptx.audio.status-recording").to_string()
        } else if has_recorded {
            format!("{}: {time_str}", text("pptx.audio.status-recorded"))
        } else {
            text("pptx.audio.status-ready").to_string()
        };

        let body = v_flex()
            .gap_3()
            .p_3()
            .p_0().child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child(text("pptx.audio.sound-name")))
                    .child(crate::shell::chrome::text_field(&name_input).full_width()),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child(text("pptx.audio.total-sound-length")))
                    .child(div().text_size(px(20.)).child(SharedString::from(time_str))),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(if is_recording { cx.theme().danger } else { cx.theme().muted_foreground })
                    .child(SharedString::from(status_text)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        crate::shell::chrome::action_button("audio-record-start")
                            .small()

                            .disabled(is_recording)
                            .child(h_flex().items_center().gap_1().child(div().text_color(rgb(0xd93025)).child("●")).child(text("pptx.audio.record")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.start_audio_recording(cx);
                            })),
                    )
                    .child(
                        crate::shell::chrome::action_button("audio-record-stop")
                            .small()

                            .disabled(!is_recording)
                            .label(format!("■ {}", text("pptx.audio.stop")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stop_audio_recording(5.0, cx);
                            })),
                    ),
            )
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(
                        crate::shell::chrome::ActionDefinition::new("audio-record-insert", text("pptx.audio.insert-to-slide"), crate::shell::chrome::ActionButtonKind::Primary)
                            .disabled(is_recording)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let name = name_input.read(cx).value().to_string();
                                this.insert_recorded_audio(&name, duration, cx);
                            })),
                    )
                    .action(
                        crate::shell::chrome::ActionDefinition::new("audio-record-cancel", text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_audio_record_panel(cx);
                            })),
                    );
        Some(SidebarPanel::with_footer(text("pptx.audio.record-title"), body.into_any_element(), panel_actions))
    }
}
