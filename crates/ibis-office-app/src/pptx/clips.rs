// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 動画・音声 (挿入 > ビデオ・オーディオ): a video or audio file put on the
//! slide as a picture (its poster frame or a speaker icon) that plays when
//! it is clicked in the slide show, or from the toolbar's 再生 in the
//! editor. Played by the system's player (`media_player`); a video's frames
//! replace its poster while it plays.

use std::path::{Path, PathBuf};

use gpui_kit::*;
use ibis_doc_model::{Clip, ClipOptions, Command, EmuPoint, EmuRect, MediaId, MediaStart, MediaVolume, Shape, ShapeId, ShapeKind, SlideId};

use crate::media_player::Player;
use crate::shell::editor::SlideEditor;

/// A clip being played.
pub(crate) struct Playing {
    pub(crate) slide: SlideId,
    pub(crate) shape: ShapeId,
    pub(crate) clip: Clip,
    pub(crate) player: Player,
    pub(crate) passed_bookmarks: std::collections::HashSet<String>,
}

/// The volume a clip is saved with (the ribbon's 音量), 0 to 1.
pub(crate) fn volume_of(clip: &Clip) -> f32 {
    match clip.options.volume {
        MediaVolume::High => 1.0,
        MediaVolume::Medium => 0.66,
        MediaVolume::Low => 0.33,
        MediaVolume::Mute => 0.0,
    }
}

/// How often the frames of the video playing are taken.
const FRAME_MS: u64 = 33;

/// The file of clip `media` for the player: written once to the temporary directory.
fn clip_file(presentation: &ibis_doc_model::Presentation, media: MediaId, link: Option<&str>, base: Option<&Path>) -> Option<PathBuf> {
    // A linked file: where the link says (a path, or a `file:` URL), beside the presentation if relative.
    if let Some(link) = link {
        let path = linked_path(link, base);
        return path.exists().then_some(path);
    }
    let m = presentation.media(media)?;
    let (ext, _) = m.format.av_type()?;
    let dir = std::env::temp_dir().join(format!("ibisOffice-clips-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("clip-{}.{ext}", media.0));
    if std::fs::metadata(&path).map(|f| f.len()).ok() != Some(m.data.len() as u64) {
        std::fs::write(&path, &m.data).ok()?;
    }
    Some(path)
}

/// The file a link names: its path (a `file:` URL decoded), made absolute with `base`
/// (the presentation's folder) when it is relative.
pub(crate) fn linked_path(link: &str, base: Option<&Path>) -> PathBuf {
    let path = match link.strip_prefix("file://") {
        Some(rest) => {
            // `file:///Users/me/a.mp4` (`/Users/me/a.mp4`), or `file:///C:/a.mp4` (`C:/a.mp4`).
            let rest = rest.strip_prefix("localhost").unwrap_or(rest);
            let rest = if rest.len() > 2 && rest.as_bytes()[0] == b'/' && rest.as_bytes()[2] == b':' { &rest[1..] } else { rest };
            percent_decode(rest)
        }
        None => link.to_string(),
    };
    let path = PathBuf::from(path);
    match base {
        Some(base) if path.is_relative() => base.join(path),
        _ => path,
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() + 0 && let Ok(v) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl SlideEditor {
    /// The clip of a shape.
    pub(crate) fn clip_of(shape: &Shape) -> Option<Clip> {
        match &shape.kind {
            ShapeKind::Picture(p) => p.clip.clone(),
            _ => None,
        }
    }

    /// The shape with a clip at `p` of the slide shown.
    pub(crate) fn clip_at(&self, p: EmuPoint) -> Option<ShapeId> {
        let slide = self.slide();
        let id = slide.hit_test(p)?;
        slide.shape(id).and_then(Self::clip_of).map(|_| id)
    }

    /// 再生・一時停止: the clip of `shape` of the slide shown played, or paused.
    pub(crate) fn toggle_clip(&mut self, shape: ShapeId, cx: &mut Context<Self>) {
        if !self.prepare_clip(shape, true, cx) {
            return;
        }
        let slide = self.current_slide;
        if let Some(playing) = self.pptx_mut().and_then(|s| s.clips.iter().find(|c| c.slide == slide && c.shape == shape)) {
            if playing.player.is_playing() { playing.player.pause() } else { playing.player.play() }
        }
        cx.notify();
    }

    /// The player of the clip of `shape` opened (paused at the start of its trim) if it is not yet, so its
    /// length and position can be shown and sought (`report`: say in the status bar when it cannot). Whether the clip has a player now.
    pub(crate) fn prepare_clip(&mut self, shape: ShapeId, report: bool, cx: &mut Context<Self>) -> bool {
        let slide = self.current_slide;
        if self.pptx().is_some_and(|s| s.clips.iter().any(|c| c.slide == slide && c.shape == shape)) {
            return true;
        }
        let Some(clip) = self.slide().shape(shape).and_then(Self::clip_of) else { return false };
        #[cfg(not(target_family = "wasm"))]
        let base = self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        #[cfg(target_family = "wasm")]
        let base: Option<PathBuf> = None;
        let Some(path) = clip_file(self.presentation(), clip.media, clip.link.as_deref(), base.as_deref()) else {
            if let Some(link) = clip.link.as_ref().filter(|_| report) {
                self.set_status(crate::shell::i18n::format("pptx.clips.linked-file-was-not-found", &[("path", &link)]), cx);
            }
            return false;
        };
        let Some(player) = Player::open(Path::new(&path), !clip.audio) else {
            if report {
                self.set_status(crate::shell::i18n::text("pptx.clips.cannot-played-here").to_string(), cx);
            }
            return false;
        };
        let volume_f32 = match clip.options.volume {
            MediaVolume::High => 1.0,
            MediaVolume::Medium => 0.66,
            MediaVolume::Low => 0.33,
            MediaVolume::Mute => 0.0,
        };
        if clip.options.trim_start_ms > 0 {
            player.seek_to_ms(clip.options.trim_start_ms);
        }
        let initial_volume = if clip.options.fade_in_ms > 0 { 0.0 } else { volume_f32 };
        let factor = self.pptx().and_then(|s| s.clip_volume.get(&shape)).map_or(1.0, |v| v.factor());
        player.set_volume(initial_volume * factor);
        let first = self.pptx().is_some_and(|s| s.clips.is_empty());
        if let Some(s) = self.pptx_mut() {
            s.clips.push(Playing { slide, shape, clip, player, passed_bookmarks: std::collections::HashSet::new() });
        }
        if first {
            self.run_clip_loop(cx);
        }
        true
    }

    /// Everything stops (the show ended, the slide changed).
    pub(crate) fn stop_clips(&mut self, cx: &mut Context<Self>) {
        let Some(playing) = self.pptx_mut().map(|s| std::mem::take(&mut s.clips)) else { return };
        for p in &playing {
            self.resources.set_live_frame(p.clip.media, None);
        }
        if !playing.is_empty() {
            cx.notify();
        }
    }

    /// スライド遷移時に呼び出し: スライドをまたいで再生（play_across_slides）するクリップ以外を停止
    pub(crate) fn stop_clips_on_slide_change(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.pptx_mut() else { return };
        let mut gone = Vec::new();
        state.clips.retain(|c| {
            if c.clip.options.play_across_slides {
                true
            } else {
                gone.push(c.clip.media);
                false
            }
        });
        for media in &gone {
            self.resources.set_live_frame(*media, None);
        }
        if !gone.is_empty() {
            cx.notify();
        }
    }

    /// The frames of the playing videos taken, while any plays.
    fn run_clip_loop(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(std::time::Duration::from_millis(FRAME_MS)).await;
            let alive = this.update(cx, |this, cx| this.poll_clips(cx)).unwrap_or(false);
            if !alive {
                break;
            }
        })
        .detach();
    }

    /// One tick: new frames shown, clips that ended or are on another slide
    /// stopped. Whether any clip is left.
    fn poll_clips(&mut self, cx: &mut Context<Self>) -> bool {
        let shown = self.current_slide;
        let mut frames = Vec::new();
        let mut gone = Vec::new();
        let mut triggered_bookmarks = Vec::new();
        let factors = self.pptx().map(|s| s.clip_volume.clone()).unwrap_or_default();
        let left = if let Some(state) = self.pptx_mut() {
            state.clips.retain_mut(|c| {
                if c.slide != shown && !c.clip.options.play_across_slides {
                    gone.push(c.clip.media);
                    return false;
                }
                let base_volume = match c.clip.options.volume {
                    MediaVolume::High => 1.0,
                    MediaVolume::Medium => 0.66,
                    MediaVolume::Low => 0.33,
                    MediaVolume::Mute => 0.0,
                };
                let current_ms = c.player.current_time_ms();
                let trim_start = c.clip.options.trim_start_ms;
                let trim_end = c.clip.options.trim_end_ms;
                if c.player.finished_with_trim(trim_end) {
                    c.passed_bookmarks.clear();
                    if c.clip.options.loop_until_stopped {
                        if trim_start > 0 {
                            c.player.seek_to_ms(trim_start);
                        } else {
                            c.player.rewind();
                        }
                        let initial_vol = if c.clip.options.fade_in_ms > 0 { 0.0 } else { base_volume };
                        c.player.set_volume(initial_vol);
                        c.player.play();
                        return true;
                    }
                    if c.clip.options.rewind {
                        if trim_start > 0 {
                            c.player.seek_to_ms(trim_start);
                        } else {
                            c.player.rewind();
                        }
                    }
                    gone.push(c.clip.media);
                    return false;
                }

                // Check bookmarks reached during playback (PPTX-ANIM-016 / PPTX-VIDEO-015)
                for bmk in &c.clip.options.bookmarks {
                    if current_ms >= bmk.time_ms && !c.passed_bookmarks.contains(&bmk.name) {
                        c.passed_bookmarks.insert(bmk.name.clone());
                        triggered_bookmarks.push((c.shape, bmk.name.clone()));
                    }
                }

                // The bar's volume (clip_bar.rs) on top of the clip's own, fades included.
                let bar_factor = factors.get(&c.shape).map_or(1.0, |v| v.factor());
                if base_volume > 0.0 && (c.clip.options.fade_in_ms > 0 || c.clip.options.fade_out_ms > 0) {
                    let mut vol_scale = 1.0f32;
                    let elapsed_from_start = current_ms.saturating_sub(trim_start);
                    if c.clip.options.fade_in_ms > 0 && elapsed_from_start < c.clip.options.fade_in_ms {
                        vol_scale = vol_scale.min(elapsed_from_start as f32 / c.clip.options.fade_in_ms as f32);
                    }
                    if c.clip.options.fade_out_ms > 0 && trim_end > 0 && current_ms < trim_end {
                        let remaining = trim_end.saturating_sub(current_ms);
                        if remaining < c.clip.options.fade_out_ms {
                            vol_scale = vol_scale.min(remaining as f32 / c.clip.options.fade_out_ms as f32);
                        }
                    }
                    c.player.set_volume(base_volume * vol_scale.clamp(0.0, 1.0) * bar_factor);
                } else {
                    c.player.set_volume(base_volume * bar_factor);
                }
                if let Some(frame) = c.player.frame() {
                    frames.push((c.clip.media, frame));
                }
                true
            });
            !state.clips.is_empty()
        } else {
            false
        };
        for (shape, bmk_name) in triggered_bookmarks {
            self.trigger_animations_for_media_bookmark(shape, &bmk_name, cx);
        }
        // The control bar's time moves while a clip plays.
        let changed = !frames.is_empty() || !gone.is_empty() || self.pptx().is_some_and(|s| s.clips.iter().any(|c| c.player.is_playing()));
        for (media, frame) in frames {
            self.resources.set_live_frame(media, Some(frame));
        }
        for media in gone {
            self.resources.set_live_frame(media, None);
        }
        if changed {
            cx.notify();
        }
        left
    }

    /// 挿入 > ビデオ…・オーディオ…: choose a file and put it on the slide.
    pub(crate) fn pick_clip(&mut self, audio: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("_shell.insert").to_string().into()) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                this.update(cx, |this, cx| {
                    this.insert_clip(&path, data, audio, cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Some((name, data))) = crate::web::pick_file().await else { return };
            this.update(cx, |this, cx| {
                this.insert_clip(Path::new(&name), data, audio, cx);
            })
            .ok();
        })
        .detach();
    }

    /// 挿入 > ビデオ・オーディオ > ファイルにリンク: the file is kept where it is and the slide points at it.
    pub(crate) fn pick_clip_link(&mut self, audio: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("_shell.insert").to_string().into()) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update(cx, |this, cx| {
                    this.insert_clip_with(&path, Vec::new(), audio, Some(path.to_string_lossy().into_owned()), cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.set_status(crate::shell::i18n::text("pptx.clips.linking-file-not-available-here").to_string(), cx);
        }
    }

    /// The file `path` (its `data`) put on the current slide as a clip: a
    /// video of the size of the slide's 40%, an audio clip as an icon.
    pub(crate) fn insert_clip(&mut self, path: &Path, data: Vec<u8>, audio: bool, cx: &mut Context<Self>) -> Option<ShapeId> {
        self.insert_clip_with(path, data, audio, None, cx)
    }

    /// As `insert_clip`; `link` the path the clip points at instead of keeping the file (`data` empty).
    pub(crate) fn insert_clip_with(&mut self, path: &Path, data: Vec<u8>, audio: bool, link: Option<String>, cx: &mut Context<Self>) -> Option<ShapeId> {
        let format = ibis_doc_model::MediaFormat::from_path(&path.to_string_lossy());
        if (audio && !format.is_audio()) || (!audio && !format.is_video()) {
            self.set_status(crate::shell::i18n::text("pptx.clips.file-cannot-inserted").to_string(), cx);
            return None;
        }
        // A video's shape is its picture's aspect, from the file when the system can read it.
        let aspect = if audio { 1.0 } else { crate::media_player::video_aspect(path).unwrap_or(16.0 / 9.0) };
        let (poster, _, _) = ibis_pptx::clip_poster(audio, aspect);
        let placeholder = if !audio { self.empty_content_placeholder_for("media") } else { None };
        let (replaced, frame) = match placeholder {
            Some((old_id, ph_frame)) => {
                let target_w = ph_frame.width;
                let target_h = (target_w as f64 / aspect) as i64;
                let (w, h) = if target_h <= ph_frame.height {
                    (target_w, target_h)
                } else {
                    let h = ph_frame.height;
                    ((h as f64 * aspect) as i64, h)
                };
                let x = ph_frame.x + (ph_frame.width - w) / 2;
                let y = ph_frame.y + (ph_frame.height - h) / 2;
                (Some(old_id), EmuRect::new(x, y, w, h))
            }
            None => {
                let size = self.presentation().size_of(self.slide());
                let (w, h) = if audio {
                    (914_400, 914_400)
                } else {
                    let w = (size.width as f64 * 0.4) as i64;
                    (w, (w as f64 / aspect) as i64)
                };
                let frame = if audio { EmuRect::new(size.width / 20, size.height - h - size.height / 20, w, h) } else { EmuRect::new((size.width - w) / 2, (size.height - h) / 2, w, h) };
                (None, frame)
            }
        };
        let poster_media = self.document.ids().add_media(ibis_doc_model::MediaFormat::Png, poster.clone());
        self.resources.add_picture(poster_media, ibis_doc_model::MediaFormat::Png, &poster);
        let clip_media = self.document.ids().add_media(format, data);
        let id = self.document.ids().new_shape_id();
        let picture = ibis_doc_model::Picture { media: poster_media, crop: Default::default(), stroke: None, corner: 0, duotone: None, clear: None, luminance: None, clip: Some(Clip { link, ..Clip::new(clip_media, audio) }), alt_text: Default::default(), decorative: false, name: String::new(), shadow: None, reflection: None, soft_edge: None, geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None };
        let shape = Shape::new(id, frame, ShapeKind::Picture(picture));
        let slide = self.current_slide;
        let mut commands = Vec::new();
        if let Some(old) = replaced {
            commands.push(Command::RemoveShapes { slide, shapes: vec![old] });
        }
        let index = self.slide().shapes.len() - usize::from(replaced.is_some());
        commands.push(Command::InsertShape { slide, index, shape });
        commands.push(Command::SetAspectLocked { slide, shape: id, locked: true });
        self.run(commands, cx);
        self.set_selection(vec![id]);
        Some(id)
    }

    /// 選択されているクリップの現在の再生オプション
    pub(crate) fn selected_clip_options(&self) -> Option<ClipOptions> {
        let ids = self.selected_ids();
        let id = *ids.first()?;
        let shape = self.slide().shape(id)?;
        Self::clip_of(shape).map(|c| c.options.clone())
    }

    /// 選択されているクリップの開始タイミングを設定（OnClick / Automatically）
    pub(crate) fn set_selected_clip_start(&mut self, start: MediaStart, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| opt.start = start);
    }

    /// 選択されているクリップの音量を設定（High / Medium / Low / Mute）
    pub(crate) fn set_selected_clip_volume(&mut self, volume: MediaVolume, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| opt.volume = volume);
    }

    /// 選択されているクリップの繰り返し再生（ループ）をトグル
    pub(crate) fn toggle_selected_clip_loop(&mut self, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| opt.loop_until_stopped = !opt.loop_until_stopped);
    }

    /// 選択されているクリップの再生後巻き戻しをトグル
    pub(crate) fn toggle_selected_clip_rewind(&mut self, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| opt.rewind = !opt.rewind);
    }

    /// 選択されている動画クリップの全画面再生をトグル
    pub(crate) fn toggle_selected_clip_fullscreen(&mut self, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| opt.full_screen = !opt.full_screen);
    }

    /// 選択されている動画クリップの停止時非表示をトグル
    pub(crate) fn toggle_selected_clip_hide_stopped(&mut self, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| opt.hide_while_not_playing = !opt.hide_while_not_playing);
    }

    /// 選択されている音声クリップのスライドショー中非表示をトグル
    pub(crate) fn toggle_selected_clip_hide_show(&mut self, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| opt.hide_during_show = !opt.hide_during_show);
    }

    /// 選択されている音声クリップのスライド切り替え後も再生をトグル
    pub(crate) fn toggle_selected_clip_play_across(&mut self, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| opt.play_across_slides = !opt.play_across_slides);
    }

    /// 選択されている音声クリップのバックグラウンド再生プリセットをトグル
    pub(crate) fn toggle_selected_clip_background(&mut self, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| {
            let next = !opt.is_background_play();
            opt.set_background_play(next);
        });
    }

    /// 選択されているクリップのポスター画像をファイルから設定
    pub(crate) fn set_selected_clip_poster_image(&mut self, path: &Path, data: Vec<u8>, cx: &mut Context<Self>) {
        let format = ibis_doc_model::MediaFormat::from_path(&path.to_string_lossy());
        if !format.is_image() {
            self.set_status(crate::shell::i18n::text("_pptx.this-file-is-not-a-picture").to_string(), cx);
            return;
        }
        let poster_media = self.document.ids().add_media(format, data.clone());
        self.resources.add_picture(poster_media, format, &data);

        let slide = self.current_slide;
        let selected = self.selected_ids();
        let targets: Vec<(ShapeId, ibis_doc_model::Picture)> = selected.iter().filter_map(|&id| {
            let sh = self.slide().shape(id)?;
            if let ShapeKind::Picture(pic) = &sh.kind {
                if pic.clip.is_some() {
                    return Some((id, pic.clone()));
                }
            }
            None
        }).collect();

        let mut commands = Vec::new();
        for (id, mut new_pic) in targets {
            new_pic.media = poster_media;
            commands.push(Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Picture(new_pic)) });
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
    }

    /// 選択されているクリップのポスター画像をリセット（既定のポスターに戻す）
    pub(crate) fn reset_selected_clip_poster_image(&mut self, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let selected = self.selected_ids();
        let targets: Vec<(ShapeId, ibis_doc_model::Picture, bool, f64)> = selected.iter().filter_map(|&id| {
            let sh = self.slide().shape(id)?;
            if let ShapeKind::Picture(pic) = &sh.kind {
                let clip = pic.clip.as_ref()?;
                let aspect = if clip.audio { 1.0 } else { sh.frame.width as f64 / sh.frame.height.max(1) as f64 };
                Some((id, pic.clone(), clip.audio, aspect))
            } else {
                None
            }
        }).collect();

        let mut commands = Vec::new();
        for (id, mut new_pic, audio, aspect) in targets {
            let (poster, _, _) = ibis_pptx::clip_poster(audio, aspect);
            let poster_media = self.document.ids().add_media(ibis_doc_model::MediaFormat::Png, poster.clone());
            self.resources.add_picture(poster_media, ibis_doc_model::MediaFormat::Png, &poster);
            new_pic.media = poster_media;
            commands.push(Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Picture(new_pic)) });
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
    }

    /// ポスター画像のファイル選択ダイアログを開く
    pub(crate) fn pick_clip_poster_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("_pptx.poster-image").to_string().into()) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                this.update(cx, |this, cx| {
                    this.set_selected_clip_poster_image(&path, data, cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Some((name, data))) = crate::web::pick_file().await else { return };
            this.update(cx, |this, cx| {
                this.set_selected_clip_poster_image(Path::new(&name), data, cx);
            })
            .ok();
        })
        .detach();
    }

    /// 選択されているクリップのトリミング時間（開始・終了ミリ秒）を設定
    pub(crate) fn set_selected_clip_trim(&mut self, start_ms: u32, end_ms: u32, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| {
            opt.trim_start_ms = start_ms;
            opt.trim_end_ms = end_ms;
        });
    }

    /// 選択されているクリップのフェード時間（イン・アウトミリ秒）を設定
    pub(crate) fn set_selected_clip_fade(&mut self, fade_in_ms: u32, fade_out_ms: u32, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| {
            opt.fade_in_ms = fade_in_ms;
            opt.fade_out_ms = fade_out_ms;
        });
    }

    /// 選択されているクリップのフェードイン時間を設定
    pub(crate) fn set_selected_clip_fade_in(&mut self, fade_in_ms: u32, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| opt.fade_in_ms = fade_in_ms);
    }

    /// 選択されているクリップのフェードアウト時間を設定
    pub(crate) fn set_selected_clip_fade_out(&mut self, fade_out_ms: u32, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| opt.fade_out_ms = fade_out_ms);
    }

    /// 選択されているクリップにブックマークを追加（未指定時は現在の再生時間、未再生時はトリム開始時間）
    pub(crate) fn add_selected_clip_bookmark(&mut self, time_ms: Option<u32>, name: Option<String>, cx: &mut Context<Self>) {
        let current_time = time_ms.or_else(|| {
            let selected = self.selected_shapes();
            let shape = selected.into_iter().find(|s| SlideEditor::clip_of(s).is_some())?;
            if let Some(state) = self.pptx() {
                if let Some(playing) = state.clips.iter().find(|c| c.shape == shape.id) {
                    return Some(playing.player.current_time_ms());
                }
            }
            let opt = self.selected_clip_options()?;
            Some(opt.trim_start_ms)
        }).unwrap_or(0);

        self.update_selected_clip_options(cx, |opt| {
            opt.add_bookmark(current_time, name.clone());
        });
    }

    /// 選択されているクリップの指定インデックスのブックマークを削除
    pub(crate) fn remove_selected_clip_bookmark(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| {
            opt.remove_bookmark(index);
        });
    }

    /// 選択されているクリップの全ブックマークを削除
    pub(crate) fn clear_selected_clip_bookmarks(&mut self, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| {
            opt.clear_bookmarks();
        });
    }

    /// 選択されているクリップに字幕トラックを追加
    pub(crate) fn add_selected_clip_caption(&mut self, label: &str, lang: &str, vtt: &str, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| {
            opt.add_caption(label, lang, vtt);
        });
    }

    /// 選択されているクリップの指定インデックスの字幕トラックを削除
    pub(crate) fn remove_selected_clip_caption(&mut self, index: usize, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| {
            opt.remove_caption(index);
        });
    }

    /// 選択されているクリップの全字幕トラックを削除
    pub(crate) fn clear_selected_clip_captions(&mut self, cx: &mut Context<Self>) {
        self.update_selected_clip_options(cx, |opt| {
            opt.clear_captions();
        });
    }

    /// 選択されているクリップの指定ブックマーク位置へシーク（必要に応じて再生開始）
    pub(crate) fn seek_selected_clip_to_bookmark(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(shape) = self.selected_shapes().into_iter().find(|s| SlideEditor::clip_of(s).is_some()) else { return };
        let shape_id = shape.id;
        let Some(opt) = self.selected_clip_options() else { return };
        let Some(bmk) = opt.bookmarks.get(index) else { return };
        let target_time = bmk.time_ms;

        let is_playing = self.pptx().is_some_and(|st| st.clips.iter().any(|c| c.shape == shape_id));
        if !is_playing {
            self.toggle_clip(shape_id, cx);
        }
        if let Some(state) = self.pptx_mut() {
            if let Some(playing) = state.clips.iter_mut().find(|c| c.shape == shape_id) {
                playing.player.seek_to_ms(target_time);
            }
        }
        cx.notify();
    }

    fn update_selected_clip_options(&mut self, cx: &mut Context<Self>, f: impl Fn(&mut ClipOptions)) {
        let slide = self.current_slide;
        let selected = self.selected_ids();
        let mut commands = Vec::new();
        for &id in &selected {
            if let Some(shape) = self.slide().shape(id) {
                if let ShapeKind::Picture(pic) = &shape.kind {
                    if let Some(mut clip) = pic.clip.clone() {
                        f(&mut clip.options);
                        let mut new_pic = pic.clone();
                        new_pic.clip = Some(clip);
                        commands.push(Command::SetKind { slide, shape: id, kind: Box::new(ShapeKind::Picture(new_pic)) });
                    }
                }
            }
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
    }

    /// 挿入 > 3D モデル…: choose a 3D model file (.glb/.gltf/.obj/.3mf) and place it on the slide.
    pub(crate) fn pick_3d_model(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                extensions: vec!["glb".into(), "gltf".into(), "obj".into(), "3mf".into()],
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_shell.insert").to_string().into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update(cx, |this, cx| {
                    this.insert_3d_model_object(&path, cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.set_status(crate::shell::i18n::text("pptx.3d-model.not-available-here").to_string(), cx);
        }
    }

    /// Place a 3D model placeholder shape on the slide.
    pub(crate) fn insert_3d_model_object(&mut self, path: &Path, cx: &mut Context<Self>) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("3D Model");
        let (w, h) = (3_000_000, 1_500_000);
        let (slide_w, slide_h) = (self.presentation().slide_size.width, self.presentation().slide_size.height);
        let x = (slide_w - w) / 2;
        let y = (slide_h - h) / 2;
        let shape_id = self.document.ids().new_shape_id();
        let shape = ibis_doc_model::Shape::text_box(shape_id, ibis_doc_model::EmuRect::new(x, y, w, h), format!("📦 {name}"));
        let slide = self.current_slide;
        let idx = self.slide().shapes.len();
        self.run(vec![ibis_doc_model::Command::InsertShape { slide, index: idx, shape }], cx);
    }

    /// 挿入 > オブジェクトの挿入…: choose an external file and place it on the slide as an embedded object icon.
    pub(crate) fn pick_insert_object(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                extensions: Vec::new(),
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_shell.insert").to_string().into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update(cx, |this, cx| {
                    this.insert_embedded_object(&path, cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.set_status(crate::shell::i18n::text("pptx.insert-object.not-available-here").to_string(), cx);
        }
    }

    /// Place an embedded object placeholder shape on the slide.
    pub(crate) fn insert_embedded_object(&mut self, path: &Path, cx: &mut Context<Self>) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("Object");
        let (w, h) = (2_500_000, 1_500_000);
        let (slide_w, slide_h) = (self.presentation().slide_size.width, self.presentation().slide_size.height);
        let x = (slide_w - w) / 2;
        let y = (slide_h - h) / 2;
        let shape_id = self.document.ids().new_shape_id();
        let shape = ibis_doc_model::Shape::text_box(shape_id, ibis_doc_model::EmuRect::new(x, y, w, h), format!("📎 {name}"));
        let slide = self.current_slide;
        let idx = self.slide().shapes.len();
        self.run(vec![ibis_doc_model::Command::InsertShape { slide, index: idx, shape }], cx);
    }
}

#[cfg(test)]
mod tests {
    use super::linked_path;
    use std::path::Path;

    #[test]
    fn links_name_files() {
        assert_eq!(linked_path("file:///Users/me/Movies/my%20movie.mp4", None), Path::new("/Users/me/Movies/my movie.mp4"));
        assert_eq!(linked_path("file:///C:/Videos/a.mp4", None), Path::new("C:/Videos/a.mp4"));
        assert_eq!(linked_path("/tmp/a.mp4", Some(Path::new("/x"))), Path::new("/tmp/a.mp4"));
        // Beside the presentation when relative.
        assert_eq!(linked_path("clips/a.mp4", Some(Path::new("/docs"))), Path::new("/docs/clips/a.mp4"));
        assert_eq!(linked_path("a.mp4", None), Path::new("a.mp4"));
    }
}
