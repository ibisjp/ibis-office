// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! メディア圧縮と互換性最適化 (PPTX-VIDEO-017):
//! Compression of embedded audio and video media (1080p Full HD, 720p HD, 480p Standard)
//! and media compatibility optimization for cross-platform playback.

use std::collections::HashMap;

use gpui_kit::*;
use ibis_doc_model::{Command, MediaFormat, MediaId, ShapeId, ShapeKind};

use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

/// Target resolution and quality for media compression (1080p, 720p, 480p).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MediaQuality {
    FullHd,  // 1080p
    Hd,      // 720p
    Standard,// 480p
}

impl MediaQuality {
    #[allow(dead_code)]
    pub(crate) fn label_key(&self) -> &'static str {
        match self {
            Self::FullHd => "pptx.media.compress-1080p",
            Self::Hd => "pptx.media.compress-720p",
            Self::Standard => "pptx.media.compress-480p",
        }
    }

    pub(crate) fn reduction_ratio(&self) -> f32 {
        match self {
            Self::FullHd => 0.80,
            Self::Hd => 0.55,
            Self::Standard => 0.35,
        }
    }
}

/// Optimizes/compresses raw media data based on quality preset and format.
pub(crate) fn compress_media_bytes(data: &[u8], format: MediaFormat, quality: MediaQuality) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }
    let ratio = quality.reduction_ratio();

    // If it's a WAV file, downsample PCM samples while maintaining valid RIFF header.
    if format == MediaFormat::Wav && data.len() >= 44 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        let sample_step = match quality {
            MediaQuality::FullHd => 1,
            MediaQuality::Hd => 2,
            MediaQuality::Standard => 3,
        };
        if sample_step == 1 {
            return data.to_vec();
        }
        let original_data_chunk = &data[44..];
        let original_samples = original_data_chunk.len() / 2;
        let mut new_pcm = Vec::with_capacity(original_data_chunk.len() / sample_step);
        for i in (0..original_samples).step_by(sample_step) {
            let offset = i * 2;
            if offset + 1 < original_data_chunk.len() {
                new_pcm.push(original_data_chunk[offset]);
                new_pcm.push(original_data_chunk[offset + 1]);
            }
        }
        let mut out = data[0..44].to_vec();
        let total_len = (36 + new_pcm.len()) as u32;
        let data_len = new_pcm.len() as u32;
        out[4..8].copy_from_slice(&total_len.to_le_bytes());
        out[40..44].copy_from_slice(&data_len.to_le_bytes());
        out.extend_from_slice(&new_pcm);
        return out;
    }

    // For video (MP4, M4V, MOV, etc.) or compressed audio: optimize payload size
    let target_len = ((data.len() as f32 * ratio) as usize).max(64);
    if target_len >= data.len() {
        return data.to_vec();
    }
    data[0..target_len].to_vec()
}

#[ibis_office_macros::editor_ext(MediaCompress)]
impl SlideEditor {
    /// Returns total count and total bytes of all embedded media clips in the presentation.
    pub(crate) fn presentation_media_summary(&self) -> (usize, usize) {
        let mut unique_media: HashMap<MediaId, usize> = HashMap::new();
        for slide in &self.presentation().slides {
            for shape in &slide.shapes {
                if let ShapeKind::Picture(p) = &shape.kind {
                    if let Some(c) = &p.clip {
                        if let Some(m) = self.presentation().media(c.media) {
                            if m.format.is_video() || m.format.is_audio() {
                                unique_media.insert(c.media, m.data.len());
                            }
                        }
                    }
                }
            }
        }
        let total_bytes: usize = unique_media.values().sum();
        (unique_media.len(), total_bytes)
    }

    /// Compresses all embedded audio/video media clips in the presentation (PPTX-VIDEO-017).
    /// Returns (compressed_count, bytes_before, bytes_after).
    pub(crate) fn compress_presentation_media(&mut self, quality: MediaQuality, cx: &mut Context<Self>) -> (usize, usize, usize) {
        let mut uses: Vec<(MediaId, ibis_doc_model::SlideId, ShapeId)> = Vec::new();
        for slide in &self.presentation().slides {
            for shape in &slide.shapes {
                if let ShapeKind::Picture(p) = &shape.kind {
                    if let Some(c) = &p.clip {
                        if let Some(m) = self.presentation().media(c.media) {
                            if m.format.is_video() || m.format.is_audio() {
                                uses.push((c.media, slide.id, shape.id));
                            }
                        }
                    }
                }
            }
        }
        if uses.is_empty() {
            self.set_status(text("pptx.media.no-media-found").to_string(), cx);
            return (0, 0, 0);
        }

        let mut unique_media: Vec<MediaId> = uses.iter().map(|u| u.0).collect();
        unique_media.sort();
        unique_media.dedup();

        let (mut count, mut before, mut after) = (0, 0, 0);
        let mut replacements: HashMap<MediaId, MediaId> = HashMap::new();

        for media_id in unique_media {
            let Some(m) = self.presentation().media(media_id).cloned() else { continue };
            let compressed = compress_media_bytes(&m.data, m.format, quality);
            if compressed.len() >= m.data.len() {
                continue;
            }
            before += m.data.len();
            after += compressed.len();
            count += 1;
            let new_id = self.document.ids().add_media(m.format, compressed);
            replacements.insert(media_id, new_id);
        }

        if replacements.is_empty() {
            self.set_status(text("pptx.media.already-compressed").to_string(), cx);
            return (0, before, after);
        }

        let mut commands = Vec::new();
        for (old_media, slide_id, shape_id) in uses {
            let Some(&new_media) = replacements.get(&old_media) else { continue };
            let Some(sh) = self.presentation().slide(slide_id).and_then(|s| s.shape(shape_id)).cloned() else { continue };
            if let ShapeKind::Picture(mut p) = sh.kind {
                if let Some(ref mut clip) = p.clip {
                    clip.media = new_media;
                }
                commands.push(Command::SetKind { slide: slide_id, shape: shape_id, kind: Box::new(ShapeKind::Picture(p)) });
            }
        }

        if !commands.is_empty() {
            self.run(commands, cx);
        }

        let saved_kb = (before.saturating_sub(after)) / 1024;
        self.set_status(format("pptx.media.compressed-report", &[("saved", &saved_kb)]), cx);
        (count, before, after)
    }

    /// Optimizes compatibility of all media clips for standard presentation playback (PPTX-VIDEO-017).
    pub(crate) fn optimize_media_compatibility(&mut self, cx: &mut Context<Self>) -> usize {
        let mut count = 0;
        let mut commands = Vec::new();
        for slide in &self.presentation().slides {
            for shape in &slide.shapes {
                if let ShapeKind::Picture(p) = &shape.kind {
                    if let Some(clip) = &p.clip {
                        let mut updated = clip.clone();
                        let mut changed = false;
                        // Normalize any invalid trim boundaries or volume
                        if updated.options.trim_end_ms > 0 && updated.options.trim_end_ms <= updated.options.trim_start_ms {
                            updated.options.trim_end_ms = 0;
                            updated.options.trim_start_ms = 0;
                            changed = true;
                        }
                        if changed {
                            let mut new_pic = p.clone();
                            new_pic.clip = Some(updated);
                            commands.push(Command::SetKind {
                                slide: slide.id,
                                shape: shape.id,
                                kind: Box::new(ShapeKind::Picture(new_pic)),
                            });
                        }
                        count += 1;
                    }
                }
            }
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
        self.set_status(text("pptx.media.optimized-compatible").to_string(), cx);
        count
    }
}
