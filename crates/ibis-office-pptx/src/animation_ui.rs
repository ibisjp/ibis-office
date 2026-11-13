// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! アニメーション: effects put on the selected shapes from the toolbar's
//! アニメーション menu (each choice takes the place of their effects, as
//! PowerPoint's gallery does), when they start and how long they take, and
//! taking them away. They play in the slide show and are saved as the
//! slide's timing (`save/pptx_timing.rs`).

use ibis_doc_model::{Animation, AnimationAfterEffect, AnimationClass as C, AnimationLook as L, AnimationRepeat, AnimationSide, AnimationStart, AnimationTrigger, Command, ShapeId};

use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::shell::editor::SlideEditor;
use gpui_kit::Context;
use crate::ext::*;

const ANIMATION_TABLE: &str = "anim-table";
const ROW_HEIGHT: f32 = 42.;
const ORDER_WIDTH: f32 = 40.;
const FX_WIDTH: f32 = 30.;
const TIMELINE_WIDTH: f32 = 84.;
const CHILD_INDENT: f32 = 14.;

/// An effect of PowerPoint's catalogue (`presetID` `id`, option `presetSubtype`) with its usual time.
pub(crate) fn preset_effect(shape: ShapeId, class: C, id: u32, subtype: u32) -> Animation {
    if !is_filter_preset(class, id) {
        // The rest of PowerPoint's gallery: the effect as PowerPoint wrote it.
        if let Some(made) = ibis_pptx::animation_catalog::make(shape, class, id, subtype) {
            return made;
        }
    }
    let mut a = effect(shape, class, L::Preset);
    a.preset = (id, subtype);
    a.behaviors = ibis_doc_model::animation_preset::build_filter_preset(class, id, subtype, a.duration).unwrap_or_default();
    a
}

/// The 軌跡 effects (`presetID`) that are the straight lines the menu has items of its own for (左へ, 下へ, 右へ, 上へ).
const STRAIGHT_PATHS: [u32; 4] = [35, 42, 63, 64];

/// The submenu その他の効果 with the gallery's effects of `class` that the menu has no item for; none when there are none.
fn more_effects(class: C, mine: &[&Animation], selected: usize) -> Option<MenuEntry> {
    let items = catalog_items(class, mine, selected);
    (!items.is_empty()).then(|| Submenu::tr("pptx.anim.more-effects", "more_horiz", items).into())
}

/// Whether effect `id` of `class` is one of the basic filter effects (ブラインド, ボックス…, 開始 and 終了 only: an 強調 effect's
/// number means another effect).
fn is_filter_preset(class: C, id: u32) -> bool {
    matches!(class, C::Entrance | C::Exit) && ibis_doc_model::animation_preset::preset_def(id).is_some()
}

/// The first option (`presetSubtype`) of effect `id` of `class`.
fn first_option(class: C, id: u32) -> u32 {
    if is_filter_preset(class, id) {
        return ibis_doc_model::animation_preset::preset_def(id).map_or(0, |d| d.options[0].0);
    }
    ibis_pptx::animation_catalog::effect(class, id).and_then(|e| e.options.first()).map_or(0, |o| o.0)
}

/// The effects of PowerPoint's gallery that the menu has no item of its own for (the effects drawn by a look of their own, and the
/// basic filter effects, are the menu's), as a menu: 開始・終了・強調・軌跡 → その他の効果.
fn catalog_items(class: C, mine: &[&Animation], selected: usize) -> Vec<MenuEntry> {
    let items: Vec<MenuEntry> = ibis_pptx::animation_catalog::effects_of(class)
        .filter(|e| !is_filter_preset(class, e.id) && !(class == C::Path && STRAIGHT_PATHS.contains(&e.id)))
        .filter(|e| ibis_pptx::animation_catalog::make(ShapeId(0), class, e.id, e.options[0].0).is_some_and(|a| matches!(a.look, L::Preset | L::Motion(..) | L::Other)))
        .map(|e| {
            let id = e.id;
            let on = mine.len() == selected && mine.iter().all(|a| a.class == class && a.preset.0 == id && matches!(a.look, L::Preset | L::Motion(..) | L::Other));
            let first = e.options[0].0;
            MenuItem::tr(e.key).checked(on).on(move |ed, _, cx| ed.set_shape_preset(class, id, first, cx)).into()
        })
        .collect();
    items
}

/// The menu items of the catalogue effects for `class`: each effect's first option; its others are the 効果のオプション.
fn preset_items(class: C, mine: &[&Animation], selected: usize) -> Vec<MenuEntry> {
    ibis_doc_model::animation_preset::FILTER_PRESETS
        .iter()
        .map(|def| {
            let on = mine.len() == selected && mine.iter().all(|a| a.class == class && a.look == L::Preset && a.preset.0 == def.id);
            let first = def.options[0].0;
            MenuItem::tr(def.key).checked(on).on(move |e, _, cx| e.set_shape_preset(class, def.id, first, cx)).into()
        })
        .collect()
}

/// An effect PowerPoint names by its preset (`presetID`, `presetSubtype`),
/// with its usual time (milliseconds): as the file is read back.
pub fn effect(shape: ShapeId, class: C, look: L) -> Animation {
    let side = |s: AnimationSide| match s {
        AnimationSide::Top => 1,
        AnimationSide::Right => 2,
        AnimationSide::Bottom => 4,
        AnimationSide::Left => 8,
    };
    let (preset, duration) = match look {
        L::Appear => ((1, 0), 1),
        // 再生・一時停止・停止 are PowerPoint's presets 1, 2 and 3 of the class メディア.
        L::Media(c) => ((match c { ibis_doc_model::MediaCommand::Play => 1, ibis_doc_model::MediaCommand::Pause => 2, ibis_doc_model::MediaCommand::Stop => 3 }, 0), 1),
        L::Fade => ((10, 0), 500),
        L::Fly(s) => ((2, side(s)), 500),
        // ズーム as PowerPoint numbers it: 16 coming in, 32 going out.
        L::Zoom => ((23, if class == C::Exit { 32 } else { 16 }), 500),
        L::Float(s) => ((30, if s == AnimationSide::Bottom { 2 } else { 1 }), 1000),
        L::Wipe(s) => ((22, side(s)), 500),
        L::Replay => ((28, 0), 2000),
        L::Spin(_) => ((8, 0), 2000),
        L::Scale(..) => ((6, 0), 2000),
        L::Motion(x, y) => {
            let subtype = if x == 0 && y > 0 {
                4
            } else if x < 0 && y == 0 {
                8
            } else if x > 0 && y == 0 {
                2
            } else if x == 0 && y < 0 {
                1
            } else {
                0
            };
            ((61, subtype), 2000)
        }
        L::Other | L::Preset => ((0, 0), 500),
    };
    Animation {
        shape,
        class,
        start: AnimationStart::OnClick,
        look,
        delay: 0,
        duration,
        preset,
        repeat_count: AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: AnimationAfterEffect::Hold,
        trigger: AnimationTrigger::None,
        paragraph: None, chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None,
        dgm_element: None, dgm_build: None, behaviors: Vec::new(),
    }
}

/// 継続時間の候補（秒）。PowerPoint の「継続時間」の候補と同じ区切り。
const DURATION_CANDIDATES: &[f32] = &[0.25, 0.5, 1.0, 2.0, 3.0, 5.0, 10.0, 20.0];
/// 遅延の候補（秒）。
const DELAY_CANDIDATES: &[f32] = &[0.0, 0.25, 0.5, 1.0, 2.0, 3.0, 5.0, 10.0];

/// アニメーション効果のオプション詳細設定パネル (Effect Options Panel / Dialog)
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationOptionsPanel {
    pub(crate) index: usize,
    pub(crate) shape: ShapeId,
    pub(crate) class: C,
    pub(crate) look: L,
    pub(crate) start: AnimationStart,
    pub delay_ms: u32,
    pub(crate) duration_ms: u32,
    pub(crate) repeat_count: AnimationRepeat,
    pub(crate) auto_reverse: bool,
    pub(crate) rewind: bool,
    pub(crate) accel: u32,
    pub(crate) decel: u32,
    pub(crate) after_effect: AnimationAfterEffect,
    pub(crate) trigger: AnimationTrigger,
    /// The effect's preset and behaviours (changed with the effect by the dialog's effect dropdown).
    pub(crate) preset: (u32, u32),
    pub(crate) behaviors: Vec<ibis_doc_model::AnimBehavior>,
}

/// Detailed timeline information for an animation on the slide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TimelineItem {
    pub(crate) click_group: usize,
    pub(crate) start_time_ms: u32,
    pub(crate) end_time_ms: u32,
    pub delay_ms: u32,
    pub duration_ms: u32,
}

/// Compute the visual start and end times (in milliseconds) and click step groups
/// for all animations on the slide, in accordance with PowerPoint's timeline sequence.
pub(crate) fn compute_timeline(animations: &[Animation]) -> (Vec<TimelineItem>, u32) {
    let mut items = Vec::with_capacity(animations.len());
    let mut current_click_start = 0u32;
    let mut current_group = 1usize;
    let mut prev_start = 0u32;
    let mut prev_end = 0u32;
    let mut max_end_overall = 0u32;

    for (i, a) in animations.iter().enumerate() {
        let repeat_factor = match a.repeat_count {
            AnimationRepeat::None => 1,
            AnimationRepeat::Count(n) => n.max(1),
            AnimationRepeat::UntilNextClick | AnimationRepeat::UntilEndOfSlide => 1,
        };
        let anim_dur = a.duration * repeat_factor;
        let (start_time, end_time) = match a.start {
            AnimationStart::OnClick => {
                if i > 0 {
                    current_group += 1;
                    current_click_start = max_end_overall;
                }
                let s = current_click_start + a.delay;
                let e = s + anim_dur;
                (s, e)
            }
            AnimationStart::WithPrevious => {
                let s = if i == 0 { a.delay } else { prev_start + a.delay };
                let e = s + anim_dur;
                (s, e)
            }
            AnimationStart::AfterPrevious => {
                let s = if i == 0 { a.delay } else { prev_end + a.delay };
                let e = s + anim_dur;
                (s, e)
            }
        };
        prev_start = start_time;
        prev_end = end_time;
        max_end_overall = max_end_overall.max(end_time);

        items.push(TimelineItem {
            click_group: current_group,
            start_time_ms: start_time,
            end_time_ms: end_time,
            delay_ms: a.delay,
            duration_ms: anim_dur,
        });
    }

    let total_ms = max_end_overall.max(1000);
    (items, total_ms)
}

/// What an effect on text takes at a time (テキストの動き).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextBuild {
    Whole,
    Paragraph,
    Word,
    Letter,
}

#[ibis_office_macros::editor_ext(AnimationUi)]
impl SlideEditor {
    /// The selected shapes' effects changed by `change` (all of the
    /// slide's effects, and the selected shapes), as one step to undo.
    fn change_animations(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Vec<Animation>, &[ShapeId])) {
        let slide = self.current_slide;
        let selected = self.selected_ids();
        if selected.is_empty() {
            return;
        }
        let mut animations = self.slide().animations.clone();
        change(&mut animations, &selected);
        if animations != self.slide().animations {
            self.run(vec![Command::SetAnimations { slide, animations }], cx);
        }
    }

    /// One effect for each selected shape, in the place of theirs (where
    /// the first of them was, else at the end).
    pub(crate) fn set_shape_effect(&mut self, class: C, look: L, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            let at = all.iter().position(|a| !a.is_media() && selected.contains(&a.shape)).unwrap_or(all.len());
            let kept_before = all[..at].len();
            all.retain(|a| !(!a.is_media() && selected.contains(&a.shape)));
            let at = kept_before.min(all.len());
            for (i, shape) in selected.iter().enumerate() {
                // Several shapes at once (a group's, too) come together, with the first one's click.
                let mut each = effect(*shape, class, look);
                if i > 0 {
                    each.start = AnimationStart::WithPrevious;
                }
                all.insert(at + i, each);
            }
        });
    }

    /// An effect of PowerPoint's catalogue (`ibis_doc_model::animation_preset`), with its option, for each selected shape, in the
    /// place of theirs.
    pub(crate) fn set_shape_preset(&mut self, class: C, id: u32, subtype: u32, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            let at = all.iter().position(|a| !a.is_media() && selected.contains(&a.shape)).unwrap_or(all.len());
            let kept_before = all[..at].len();
            all.retain(|a| !(!a.is_media() && selected.contains(&a.shape)));
            let at = kept_before.min(all.len());
            for (i, shape) in selected.iter().enumerate() {
                let mut each = preset_effect(*shape, class, id, subtype);
                if i > 0 {
                    each.start = AnimationStart::WithPrevious;
                }
                all.insert(at + i, each);
            }
        });
    }

    /// 効果のオプション of a catalogue effect: the option (`presetSubtype`) of the selected shapes' effects.
    pub(crate) fn set_shape_preset_option(&mut self, subtype: u32, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape) && a.look == L::Preset) {
                if !is_filter_preset(a.class, a.preset.0) {
                    // PowerPoint's own effect with this option: its behaviours and time, the effect's start and delay kept.
                    if let Some(made) = ibis_pptx::animation_catalog::make(a.shape, a.class, a.preset.0, subtype) {
                        a.preset = made.preset;
                        a.behaviors = made.behaviors;
                        a.duration = made.duration;
                        a.look = made.look;
                    }
                } else if let Some(behaviors) = ibis_doc_model::animation_preset::build_filter_preset(a.class, a.preset.0, subtype, a.duration) {
                    a.preset.1 = subtype;
                    a.behaviors = behaviors;
                }
            }
        });
    }

    /// 効果の追加: an effect more for each selected shape, after the slide's
    /// others (theirs kept).
    pub(crate) fn add_shape_effect(&mut self, class: C, look: L, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for (i, shape) in selected.iter().enumerate() {
                let mut each = effect(*shape, class, look);
                if i > 0 {
                    each.start = AnimationStart::WithPrevious;
                }
                all.push(each);
            }
        });
    }

    /// メディア > 再生・一時停止・停止 (PowerPoint's アニメーションの追加 for a video or an audio): the command added to the main sequence, on a
    /// click, for each selected clip. The save writes the call (`pptx_media_timing::call_piece`).
    pub(crate) fn add_media_effect(&mut self, command: ibis_doc_model::MediaCommand, cx: &mut Context<Self>) {
        let clips: Vec<ShapeId> = self.selected_ids().into_iter().filter(|id| self.slide().shape(*id).is_some_and(|s| Self::clip_of(s).is_some())).collect();
        if clips.is_empty() {
            return;
        }
        self.change_animations(cx, move |all, _| {
            for (i, shape) in clips.iter().enumerate() {
                let mut each = effect(*shape, C::Media, L::Media(command));
                if i > 0 {
                    each.start = AnimationStart::WithPrevious;
                }
                all.push(each);
            }
        });
    }

    /// The effect at `index` of the slide's list one place up (or down).
    pub(crate) fn move_effect(&mut self, index: usize, up: bool, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        let other = if up { index.checked_sub(1) } else { Some(index + 1).filter(|i| *i < all.len()) };
        let Some(other) = other else { return };
        if index >= all.len() {
            return;
        }
        all.swap(index, other);
        self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
    }

    /// The first effect of the selected shapes in the slide's list (the one the menu's 順番を前に/後ろにする moves).
    fn selected_effect_index(&self) -> Option<usize> {
        let selected = self.selected_ids();
        self.slide().animations.iter().position(|a| !a.is_media() && selected.contains(&a.shape))
    }

    /// Whether the selected shape's first effect has a place to move to (one earlier or later in the list).
    pub(crate) fn can_move_selected_effect(&self, up: bool) -> bool {
        self.selected_effect_index().is_some_and(|i| if up { i > 0 } else { i + 1 < self.slide().animations.len() })
    }

    /// The selected shape's first effect one place earlier (or later) in the slide's list.
    pub(crate) fn move_selected_effect(&mut self, up: bool, cx: &mut Context<Self>) {
        if let Some(index) = self.selected_effect_index() {
            self.move_effect(index, up, cx);
        }
    }

    /// The effect at `index` of the slide's list taken away.
    pub fn remove_effect(&mut self, index: usize, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if index < all.len() {
            all.remove(index);
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 開始タイミングを設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_start_at(&mut self, index: usize, start: AnimationStart, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.start = start;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 開始タイミングを次のモードへサイクル切り替え (OnClick -> WithPrevious -> AfterPrevious -> OnClick).
    pub(crate) fn cycle_effect_start_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| a.start);
        if let Some(start) = current {
            let next = match start {
                AnimationStart::OnClick => AnimationStart::WithPrevious,
                AnimationStart::WithPrevious => AnimationStart::AfterPrevious,
                AnimationStart::AfterPrevious => AnimationStart::OnClick,
            };
            self.set_effect_start_at(index, next, cx);
        }
    }

    /// 継続時間を設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_duration_at(&mut self, index: usize, ms: u32, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            if a.look != L::Appear {
                a.duration = ms;
                self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
            }
        }
    }

    /// 遅延時間を設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_delay_at(&mut self, index: usize, ms: u32, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.delay = ms;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 継続時間を増減 (delta_ms: 例 500ms).
    pub(crate) fn adjust_effect_duration_at(&mut self, index: usize, delta_ms: i32, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| a.duration);
        if let Some(dur) = current {
            let new_dur = (dur as i32 + delta_ms).clamp(100, 60_000) as u32;
            self.set_effect_duration_at(index, new_dur, cx);
        }
    }

    /// 遅延時間を増減 (delta_ms: 例 500ms).
    pub(crate) fn adjust_effect_delay_at(&mut self, index: usize, delta_ms: i32, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| a.delay);
        if let Some(delay) = current {
            let new_delay = (delay as i32 + delta_ms).clamp(0, 60_000) as u32;
            self.set_effect_delay_at(index, new_delay, cx);
        }
    }

    /// 繰り返しを設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_repeat_at(&mut self, index: usize, repeat: AnimationRepeat, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.repeat_count = repeat;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 繰り返しを次のモードへサイクル切り替え (None -> Count(2) -> Count(3) -> Count(5) -> Count(10) -> UntilNextClick -> UntilEndOfSlide -> None).
    pub(crate) fn cycle_effect_repeat_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| a.repeat_count);
        if let Some(r) = current {
            let next = match r {
                AnimationRepeat::None => AnimationRepeat::Count(2),
                AnimationRepeat::Count(2) => AnimationRepeat::Count(3),
                AnimationRepeat::Count(3) => AnimationRepeat::Count(5),
                AnimationRepeat::Count(5) => AnimationRepeat::Count(10),
                AnimationRepeat::Count(_) => AnimationRepeat::UntilNextClick,
                AnimationRepeat::UntilNextClick => AnimationRepeat::UntilEndOfSlide,
                AnimationRepeat::UntilEndOfSlide => AnimationRepeat::None,
            };
            self.set_effect_repeat_at(index, next, cx);
        }
    }

    /// 自動反転をトグル切り替え (特定のインデックスの効果).
    pub(crate) fn toggle_effect_autoreverse_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.auto_reverse = !a.auto_reverse;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 滑らかさ (accel, decel) を設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_smooth_at(&mut self, index: usize, accel: u32, decel: u32, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.accel = accel;
            a.decel = decel;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 滑らかさを次のモードへサイクル切り替え (なし -> 開始 -> 終了 -> 両方 -> なし).
    pub(crate) fn cycle_effect_smooth_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| (a.accel > 0, a.decel > 0));
        if let Some(smooth) = current {
            let (next_accel, next_decel) = match smooth {
                (false, false) => (50_000, 0),        // 開始
                (true, false) => (0, 50_000),         // 終了
                (false, true) => (50_000, 50_000),    // 両方
                (true, true) => (0, 0),               // なし
            };
            self.set_effect_smooth_at(index, next_accel, next_decel, cx);
        }
    }

    /// 滑らかに開始をトグル切り替え (特定のインデックスの効果).
    pub(crate) fn toggle_effect_smooth_start_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| (a.accel, a.decel));
        if let Some((accel, decel)) = current {
            let next_accel = if accel > 0 { 0 } else { 50_000 };
            self.set_effect_smooth_at(index, next_accel, decel, cx);
        }
    }

    /// 滑らかに終了をトグル切り替え (特定のインデックスの効果).
    pub(crate) fn toggle_effect_smooth_end_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| (a.accel, a.decel));
        if let Some((accel, decel)) = current {
            let next_decel = if decel > 0 { 0 } else { 50_000 };
            self.set_effect_smooth_at(index, accel, next_decel, cx);
        }
    }

    /// 効果終了時の動作を設定 (特定のインデックスの効果).
    pub(crate) fn set_effect_after_at(&mut self, index: usize, after: AnimationAfterEffect, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.after_effect = after;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 効果終了時の動作を次のモードへサイクル切り替え (Hold -> Hide -> Dim -> HideOnNextClick -> Hold).
    pub(crate) fn cycle_effect_after_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let current = self.slide().animations.get(index).map(|a| a.after_effect);
        if let Some(after) = current {
            let next = match after {
                AnimationAfterEffect::Hold => AnimationAfterEffect::Hide,
                AnimationAfterEffect::Hide => AnimationAfterEffect::Dim,
                AnimationAfterEffect::Dim => AnimationAfterEffect::HideOnNextClick,
                AnimationAfterEffect::HideOnNextClick => AnimationAfterEffect::Hold,
            };
            self.set_effect_after_at(index, next, cx);
        }
    }

    /// 再生終了後に巻き戻すをトグル切り替え (特定のインデックスの効果).
    pub(crate) fn toggle_effect_rewind_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.rewind = !a.rewind;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// トリガー条件を設定 (特定のインデックスの効果).
    #[allow(dead_code)]
    pub(crate) fn set_effect_trigger_at(&mut self, index: usize, trigger: AnimationTrigger, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(index) {
            a.trigger = trigger;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 効果のオプション詳細設定パネルを開く.
    pub fn open_animation_options_panel(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(a) = self.slide().animations.get(index).filter(|a| !a.is_media()).cloned() else { return };
        self.shape_edit.gradient_panel = None; self.shape_edit.shadow_panel = None; self.shape_edit.glow_panel = None; self.shape_edit.paragraph_panel = None; self.shape_edit.size_panel = None; self.shape_edit.insets_panel = None; self.shape_edit.alt_text_panel = None;
        self.shape_edit.find = None; self.shape_edit.frame_panel = None; self.shape_edit.header_footer = None; self.shape_edit.crop_panel = None;
        self.shape_edit.chart_panel = None;
        if let Some(s) = self.pptx_mut() {
            s.section_panel = None;
            
            
            
            
            
            
            
            s.comments = None;
            
            
            
            
            s.timing_panel = None;
            
            s.properties_panel = None;
            s.inspect_panel = None;
            s.accessibility_panel = None;
            s.animation_options_panel = Some(AnimationOptionsPanel {
                index,
                shape: a.shape,
                class: a.class,
                look: a.look,
                start: a.start,
                delay_ms: a.delay,
                duration_ms: a.duration,
                repeat_count: a.repeat_count,
                auto_reverse: a.auto_reverse,
                rewind: a.rewind,
                accel: a.accel,
                decel: a.decel,
                after_effect: a.after_effect,
                trigger: a.trigger.clone(),
                preset: a.preset,
                behaviors: a.behaviors.clone(),
            });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 選択中の図形のアニメーションで効果のオプション詳細設定パネルを開く.
    pub(crate) fn open_selected_animation_options_panel(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        let index = self.slide().animations.iter().position(|a| !a.is_media() && selected.contains(&a.shape));
        if let Some(i) = index {
            self.open_animation_options_panel(i, cx);
        }
    }

    /// 効果のオプション詳細設定パネルを閉じる.
    pub fn close_animation_options_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.animation_options_panel = None;
        }
        cx.notify();
    }

    /// 効果のオプション詳細設定パネルの設定を適用して保存.
    pub(crate) fn apply_animation_options_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.animation_options_panel.as_ref()).cloned() else { return };
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if let Some(a) = all.get_mut(p.index) {
            a.look = p.look;
            a.start = p.start;
            a.delay = p.delay_ms;
            a.duration = p.duration_ms;
            a.repeat_count = p.repeat_count;
            a.auto_reverse = p.auto_reverse;
            a.rewind = p.rewind;
            a.accel = p.accel;
            a.decel = p.decel;
            a.after_effect = p.after_effect;
            a.trigger = p.trigger;
            a.preset = p.preset;
            a.behaviors = p.behaviors;
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
        self.close_animation_options_panel(cx);
    }

    /// The effects the dialog's effect dropdown offers for `class`: the menu's key, the look and (for the catalogue's) the preset.
    fn panel_effect_choices(class: C, current: L) -> Vec<(&'static str, L, Option<u32>)> {
        let mut v: Vec<(&'static str, L, Option<u32>)> = match class {
            C::Entrance => vec![
                ("pptx.anim.in.appear", L::Appear, None),
                ("pptx.anim.in.fade", L::Fade, None),
                ("pptx.anim.in.fly-bottom", L::Fly(AnimationSide::Bottom), None),
                ("pptx.anim.in.float", L::Float(AnimationSide::Top), None),
                ("pptx.anim.in.wipe", L::Wipe(AnimationSide::Bottom), None),
                ("pptx.anim.in.zoom", L::Zoom, None),
            ],
            C::Exit => vec![
                ("pptx.anim.out.disappear", L::Appear, None),
                ("pptx.anim.out.fade", L::Fade, None),
                ("pptx.anim.out.fly-bottom", L::Fly(AnimationSide::Bottom), None),
                ("pptx.anim.out.float", L::Float(AnimationSide::Bottom), None),
                ("pptx.anim.out.wipe", L::Wipe(AnimationSide::Bottom), None),
                ("pptx.anim.out.zoom", L::Zoom, None),
            ],
            C::Emphasis => vec![("pptx.anim.em.spin", L::Spin(21_600_000), None), ("pptx.anim.em.grow", L::Scale(150_000, 150_000), None)],
            C::Path => vec![
                ("pptx.anim.path.down", L::Motion(0, 25_000), None),
                ("pptx.anim.path.left", L::Motion(-25_000, 0), None),
                ("pptx.anim.path.right", L::Motion(25_000, 0), None),
                ("pptx.anim.path.up", L::Motion(0, -25_000), None),
            ],
            C::Media => Vec::new(),
        };
        v.extend(
            ibis_pptx::animation_catalog::effects_of(class)
                .filter(|e| !is_filter_preset(class, e.id) && !(class == C::Path && STRAIGHT_PATHS.contains(&e.id)))
                .filter(|e| ibis_pptx::animation_catalog::make(ShapeId(0), class, e.id, e.options[0].0).is_some_and(|a| matches!(a.look, L::Preset | L::Motion(..) | L::Other)))
                .map(|e| (e.key, L::Preset, Some(e.id))),
        );
        if matches!(class, C::Entrance | C::Exit) {
            v.extend(ibis_doc_model::animation_preset::FILTER_PRESETS.iter().map(|def| (def.key, L::Preset, Some(def.id))));
            if current == L::Replay {
                v.push((if class == C::Entrance { "pptx.anim.in.replay" } else { "pptx.anim.out.rewind" }, L::Replay, None));
            }
        }
        v
    }

    /// The dialog's effect dropdown: the panel takes the effect chosen (its look, preset and usual time), the timing kept.
    pub(crate) fn set_panel_effect(&mut self, look: L, preset_id: Option<u32>, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            let template = match preset_id {
                Some(id) => {
                    preset_effect(p.shape, p.class, id, first_option(p.class, id))
                }
                None => effect(p.shape, p.class, look),
            };
            p.look = template.look;
            p.preset = template.preset;
            p.behaviors = template.behaviors;
            p.duration_ms = template.duration;
        }
        cx.notify();
    }

    pub(crate) fn set_panel_trigger(&mut self, trigger: AnimationTrigger, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.trigger = trigger;
        }
        cx.notify();
    }

    pub(crate) fn set_panel_start(&mut self, start: AnimationStart, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.start = start;
        }
        cx.notify();
    }

    pub(crate) fn adjust_panel_duration(&mut self, delta_ms: i32, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.duration_ms = (p.duration_ms as i32 + delta_ms).clamp(100, 60_000) as u32;
        }
        cx.notify();
    }

    pub(crate) fn adjust_panel_delay(&mut self, delta_ms: i32, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.delay_ms = (p.delay_ms as i32 + delta_ms).clamp(0, 60_000) as u32;
        }
        cx.notify();
    }

    pub(crate) fn set_panel_repeat(&mut self, repeat: AnimationRepeat, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.repeat_count = repeat;
        }
        cx.notify();
    }

    pub(crate) fn toggle_panel_auto_reverse(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.auto_reverse = !p.auto_reverse;
        }
        cx.notify();
    }

    pub(crate) fn toggle_panel_rewind(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.rewind = !p.rewind;
        }
        cx.notify();
    }

    pub(crate) fn toggle_panel_smooth_start(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.accel = if p.accel > 0 { 0 } else { 50_000 };
        }
        cx.notify();
    }

    pub(crate) fn toggle_panel_smooth_end(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.decel = if p.decel > 0 { 0 } else { 50_000 };
        }
        cx.notify();
    }

    pub(crate) fn set_panel_after_effect(&mut self, after: AnimationAfterEffect, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.after_effect = after;
        }
        cx.notify();
    }

    /// The dialog's 継続時間 (ms), typed, stepped or picked from the candidates.
    pub(crate) fn set_panel_duration_ms(&mut self, ms: u32, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.duration_ms = ms.clamp(100, 60_000);
        }
        cx.notify();
    }

    /// The dialog's 遅延 (ms).
    pub(crate) fn set_panel_delay_ms(&mut self, ms: u32, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.delay_ms = ms.min(60_000);
        }
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn cycle_panel_after_effect(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            p.after_effect = match p.after_effect {
                AnimationAfterEffect::Hold => AnimationAfterEffect::Hide,
                AnimationAfterEffect::Hide => AnimationAfterEffect::Dim,
                AnimationAfterEffect::Dim => AnimationAfterEffect::HideOnNextClick,
                AnimationAfterEffect::HideOnNextClick => AnimationAfterEffect::Hold,
            };
        }
        cx.notify();
    }

    pub(crate) fn set_panel_fly_side(&mut self, side: AnimationSide, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            if matches!(p.look, L::Fly(_)) {
                p.look = L::Fly(side);
            }
        }
        cx.notify();
    }

    pub(crate) fn set_panel_float_side(&mut self, side: AnimationSide, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            if matches!(p.look, L::Float(_)) {
                p.look = L::Float(side);
            }
        }
        cx.notify();
    }

    pub(crate) fn set_panel_wipe_side(&mut self, side: AnimationSide, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            if matches!(p.look, L::Wipe(_)) {
                p.look = L::Wipe(side);
            }
        }
        cx.notify();
    }

    pub(crate) fn set_panel_motion_dir(&mut self, x: i32, y: i32, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.animation_options_panel.as_mut()) {
            if matches!(p.look, L::Motion(..)) {
                p.look = L::Motion(x, y);
            }
        }
        cx.notify();
    }

    /// Context menu entries for an effect card at `index` in the Animation Pane.
    pub fn animation_pane_context_menu_entries(&self, index: usize) -> Vec<MenuEntry> {
        let Some(a) = self.slide().animations.get(index) else { return Vec::new() };
        let count = self.slide().animations.len();
        let start = a.start;
        let auto_rev = a.auto_reverse;
        let rewind = a.rewind;
        vec![
            MenuItem::tr("pptx.anim.start.click")
                .icon("mouse")
                .checked(start == AnimationStart::OnClick)
                .on(move |e, _, cx| e.set_effect_start_at(index, AnimationStart::OnClick, cx))
                .into(),
            MenuItem::tr("pptx.anim.start.with")
                .icon("play_arrow")
                .checked(start == AnimationStart::WithPrevious)
                .on(move |e, _, cx| e.set_effect_start_at(index, AnimationStart::WithPrevious, cx))
                .into(),
            MenuItem::tr("pptx.anim.start.after")
                .icon("schedule")
                .checked(start == AnimationStart::AfterPrevious)
                .on(move |e, _, cx| e.set_effect_start_at(index, AnimationStart::AfterPrevious, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.anim.options-dialog")
                .icon("tune")
                .on(move |e, _, cx| e.open_animation_options_panel(index, cx))
                .into(),
            MenuItem::tr("pptx.anim.autoreverse")
                .checked(auto_rev)
                .on(move |e, _, cx| e.toggle_effect_autoreverse_at(index, cx))
                .into(),
            MenuItem::tr("pptx.anim.rewind")
                .checked(rewind)
                .on(move |e, _, cx| e.toggle_effect_rewind_at(index, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.smartart.move-up")
                .icon("arrow_upward")
                .disabled(index == 0)
                .on(move |e, _, cx| e.move_effect(index, true, cx))
                .into(),
            MenuItem::tr("pptx.smartart.move-down")
                .icon("arrow_downward")
                .disabled(index + 1 >= count)
                .on(move |e, _, cx| e.move_effect(index, false, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.anim.copy")
                .icon("content_copy")
                .shortcut("⌥⇧⌘C")
                .disabled(!self.can_copy_animations())
                .on(|e, _, cx| e.copy_shape_animations(cx))
                .into(),
            MenuItem::tr("pptx.anim.paste")
                .icon("content_paste")
                .shortcut("⌥⇧⌘V")
                .disabled(!self.can_paste_animations())
                .on(|e, _, cx| e.paste_shape_animations(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.anim.remove")
                .icon("delete")
                .on(move |e, _, cx| e.remove_effect(index, cx))
                .into(),
        ]
    }

    /// Open context menu for animation effect `index` in the Animation Pane at pointer `position`.
    pub(crate) fn open_animation_pane_context_menu(&mut self, index: usize, position: gpui_kit::Point<gpui_kit::Pixels>, window: &mut gpui_kit::Window, cx: &mut Context<Self>) {
        if let Some(a) = self.slide().animations.get(index) {
            let shape_id = a.shape;
            if !self.selected_ids().contains(&shape_id) {
                self.set_selection(vec![shape_id]);
                cx.notify();
            }
        }
        let entries = self.animation_pane_context_menu_entries(index);
        self.open_popup_entries(entries, position, window, cx);
    }

    /// The text box's row of the animation pane: its paragraphs' effects shown under it or folded.
    /// A row's handle is pressed to be dragged: the row's siblings and their descendants fold (a text box's or a plain row is dragged among
    /// the rows of the top level, so every open text box folds; a paragraph's row is dragged among the paragraphs of its own text box, which
    /// have no descendants, so nothing folds: its text box and the other text boxes stay as they are), and `animation_pane` opens them again
    /// when the drag is over. Only the ones that were open are opened again.
    pub(crate) fn fold_animation_boxes_for_drag(&mut self, paragraph_row: bool, cx: &mut Context<Self>) {
        if paragraph_row {
            return;
        }
        if let Some(s) = self.pptx_mut() {
            if s.animation_drag_folded.is_empty() {
                s.animation_drag_folded = s.animation_expanded.drain().collect();
            }
        }
        cx.notify();
    }

    /// Opens or folds one text box's row (`box_key`): each row of a text box is open or folded on its own, also when one shape has
    /// two of them on the slide.
    pub(crate) fn toggle_animation_expanded(&mut self, key: BoxKey, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            if !s.animation_expanded.remove(&key) {
                s.animation_expanded.insert(key);
            }
        }
        cx.notify();
    }

    /// The effects `first` to `last` of the slide's list taken away (one undo step).
    pub(crate) fn remove_effects(&mut self, first: usize, last: usize, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let mut all = self.slide().animations.clone();
        if first > last || last >= all.len() {
            return;
        }
        all.drain(first..=last);
        self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
    }

    /// アニメーション ウィンドウ: shown or hidden.
    pub fn toggle_animation_pane(&mut self, cx: &mut Context<Self>) {
        let shown = self.pptx().is_some_and(|s| s.animation_pane);
        if let Some(s) = self.pptx_mut() {
            s.animation_pane = !shown;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The pane: the slide's effects in order, one row each (start, class icon, shape and look, timeline bar), in the
    /// shared table. A row is dragged to reorder (`animation_row_dropped`), double-clicked or its 効果のオプション
    /// button pressed to edit the effect in the small dialog (`animation_options_panel`), right-clicked for the menu.
    pub fn animation_pane(&mut self, cx: &mut Context<Self>) -> Option<crate::shell::chrome::SidebarPanel> {
        use crate::shell::chrome::data_table::DataTable;
        use crate::shell::chrome::virtual_table::{TableColumn, TableRow};
        use crate::shell::icons::Material;
        use gpui_kit::component::{Sizable as _, h_flex, v_flex};
        use gpui_kit::*;
        if !self.pptx()?.animation_pane {
            return None;
        }
        let t = crate::shell::i18n::text;
        let slide = self.slide().clone();
        let selected_ids = self.selected_ids();
        if slide.animations.is_empty() {
            let body = v_flex().gap_2().p_0().child(div().text_sm().text_color(rgb(0x5f6368)).child(t("pptx.animation-ui.there-no-animations-slide")));
            return Some(crate::shell::chrome::SidebarPanel { title: t("pptx.animation-ui.animation-pane"), body: body.into_any_element() });
        }
        let (table, dnd) = self.pptx().map(|s| (s.animation_table.clone(), s.animation_dnd.clone()))?;
        // The text boxes that were folded for a text box's row to be dragged open again once the row is let go (or the drag is
        // given up).
        if dnd.carried().is_none() {
            if let Some(s) = self.pptx_mut() {
                let folded = std::mem::take(&mut s.animation_drag_folded);
                s.animation_expanded.extend(folded);
            }
        }

        let (timeline_items, max_time_ms) = compute_timeline(&slide.animations);
        let scale_ms = (((max_time_ms + 499) / 500) * 500).max(1000);
        let count = slide.animations.len();

        // What each row shows, worked out once (the table makes the rows of the visible range from it).
        struct PaneRow {
            name: String,
            look: String,
            class: C,
            media: Option<ibis_doc_model::MediaCommand>,
            start: AnimationStart,
            triggered: bool,
            click_no: usize,
            left: f32,
            width: f32,
            delay_width: f32,
            selected: bool,
            /// A paragraph's effect: its text (the row's name when it is listed under its text box's row).
            paragraph_name: Option<String>,
        }
        let mut click_no = 0usize;
        let rows: Vec<PaneRow> = slide.animations.iter().enumerate().map(|(i, a)| {
            let item = timeline_items.get(i).copied().unwrap_or(TimelineItem { click_group: 1, start_time_ms: 0, end_time_ms: a.duration, delay_ms: a.delay, duration_ms: a.duration });
            // A clip's rows carry the clip's name (PowerPoint lists 再生 / 一時停止 under the video's or audio's name).
            let clip_name = slide.shape(a.shape).filter(|_| a.is_media()).and_then(|s| match &s.kind {
                ibis_doc_model::ShapeKind::Picture(p) if !p.name.trim().is_empty() => Some(p.name.clone()),
                _ => None,
            });
            let name = clip_name.or_else(|| slide.shape(a.shape)
                .and_then(|s| s.text_body())
                .map(|b| b.text().chars().take(12).collect::<String>())
                .filter(|t| !t.trim().is_empty()))
                .unwrap_or_else(|| crate::shell::i18n::format("pptx.selection-pane.shape-n", &[("n", &(slide.shape_index(a.shape).map_or(0, |x| x + 1)))]));
            let triggered = a.is_triggered();
            if a.start == AnimationStart::OnClick && !triggered {
                click_no += 1;
            }
            let frac = |ms: u32| ms as f32 / scale_ms as f32;
            PaneRow {
                name,
                look: look_label(a.class, a.look, a.preset),
                class: a.class,
                media: a.media_command(),
                start: a.start,
                triggered,
                click_no: if a.start == AnimationStart::OnClick && !triggered { click_no } else { 0 },
                left: frac(item.start_time_ms),
                width: frac(item.end_time_ms - item.start_time_ms),
                delay_width: frac(item.delay_ms.min(item.start_time_ms)),
                selected: selected_ids.contains(&a.shape),
                paragraph_name: a.paragraph.and_then(|(first, _)| slide.shape(a.shape).and_then(|s| s.text_body()).and_then(|b| b.paragraphs.get(first as usize).map(|p| p.text().chars().take(24).collect::<String>())))
                    .filter(|t| !t.trim().is_empty()),
            }
        }).collect();
        // The rows shown: an effect for each paragraph of a text box (two or more in a row) is listed under one row of the
        // text box's (the parent: expanded or folded), its own rows (the children) a little indented.
        #[derive(Clone, Copy, PartialEq)]
        enum Kind {
            Plain,
            Parent { last: usize, expanded: bool },
            // `first`: the text box row's first effect (the paragraphs of one text box share the scope of their drag).
            Child { first: usize },
        }
        let expanded_boxes = self.pptx().map(|s| s.animation_expanded.clone()).unwrap_or_default();
        let mut display: Vec<(usize, Kind)> = Vec::new();
        let mut at = 0usize;
        while at < count {
            let (_, end) = paragraph_run(&slide.animations, at);
            if end > at {
                let open = expanded_boxes.contains(&box_keys(&slide.animations)[at]);
                display.push((at, Kind::Parent { last: end, expanded: open }));
                if open {
                    display.extend((at..=end).map(|i| (i, Kind::Child { first: at })));
                }
            } else {
                display.push((at, Kind::Plain));
            }
            at = end + 1;
        }
        let display = std::rc::Rc::new(display);
        let shown = display.len();
        let rows = std::rc::Rc::new(rows);
        let shapes: std::rc::Rc<Vec<ShapeId>> = std::rc::Rc::new(slide.animations.iter().map(|a| a.shape).collect());
        let keys: std::rc::Rc<Vec<BoxKey>> = std::rc::Rc::new(box_keys(&slide.animations));
        let editor = cx.entity().downgrade();

        let heading = |id: &'static str| div().text_xs().font_weight(FontWeight::MEDIUM).text_color(rgb(crate::shell::chrome::TEXT_SECONDARY)).child(t(id)).into_any_element();
        let columns = vec![TableColumn::fixed("order", ORDER_WIDTH), TableColumn::fixed("fx", FX_WIDTH), TableColumn::flex("name"), TableColumn::fixed("timeline", TIMELINE_WIDTH)];
        let header = vec![heading("pptx.anim.pane.col.order"), div().into_any_element(), heading("pptx.anim.pane.col.effect"), heading("_pptx.timeline")];

        let build = {
            let (rows, shapes, keys, editor, display) = (rows.clone(), shapes.clone(), keys.clone(), editor.clone(), display.clone());
            move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                range.filter_map(|d| display.get(d).and_then(|&(i, kind)| rows.get(i).map(|r| (i, kind, r)))).map(|(i, kind, r)| {
                    let (class_color, class_bg, class_icon) = match r.class {
                        C::Entrance => (0x2e7d32, 0xe8f5e9, "login"),
                        C::Emphasis => (0xed6c02, 0xfff3e0, "auto_awesome"),
                        C::Exit => (0xd32f2f, 0xffebee, "logout"),
                        C::Path => (0x1976d2, 0xe3f2fd, "conversion_path"),
                        // PowerPoint's 再生 / 一時停止 / 停止 rows: the command's own glyph in blue.
                        C::Media => (0x1976d2, 0xe3f2fd, match r.media {
                            Some(ibis_doc_model::MediaCommand::Pause) => "pause",
                            Some(ibis_doc_model::MediaCommand::Stop) => "stop",
                            _ => "play_arrow",
                        }),
                    };
                    let (start_icon, start_tip) = if r.triggered {
                        ("link", t("pptx.anim.trigger"))
                    } else {
                        match r.start {
                            AnimationStart::OnClick => ("mouse", t("pptx.anim.start.click")),
                            AnimationStart::WithPrevious => ("play_circle", t("_pptx.with-previous")),
                            AnimationStart::AfterPrevious => ("skip_next", t("_pptx.after-previous")),
                        }
                    };
                    let order = h_flex().id(("anim-row-order", i)).w_full().gap_0p5().items_center().text_xs().text_color(rgb(0x3c4043))
                        .child(crate::shell::chrome::icon(start_icon, 16., rgb(0x80868b).into()))
                        .children((r.click_no > 0).then(|| div().child(format!("{}", r.click_no))))
                        .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(start_tip.clone()).build(window, cx))
                        .into_any_element();
                    let fx = div().size(px(22.)).rounded(px(5.)).bg(rgb(class_bg)).flex().items_center().justify_center()
                        .child(crate::shell::chrome::icon(class_icon, 16., rgb(class_color).into())).into_any_element();
                    let title = match kind {
                        Kind::Child { .. } => r.paragraph_name.clone().unwrap_or_else(|| r.name.clone()),
                        _ => r.name.clone(),
                    };
                    let mut name = h_flex().w_full().min_w_0().pr_1().items_center().gap_0p5();
                    match kind {
                        Kind::Parent { expanded, .. } => {
                            // The fold arrow of the text box's row: it shows or hides the effects of its paragraphs.
                            let (fold, key) = (editor.clone(), keys[i]);
                            name = name.child(
                                div().id(("anim-row-fold", i)).size(px(18.)).flex_none().flex().items_center().justify_center().rounded_full().cursor_pointer()
                                    .hover(|s| s.bg(crate::shell::palette::shade(0.27, 0.08)))
                                    .child(crate::shell::chrome::icon(if expanded { "expand_more" } else { "chevron_right" }, 18., rgb(0x5f6368).into()))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        let _ = fold.update(cx, |e, cx| e.toggle_animation_expanded(key, cx));
                                    }),
                            );
                        }
                        // A paragraph's row stands in a little (the fold arrow's place and a bit more).
                        Kind::Child { .. } => name = name.pl(px(CHILD_INDENT)),
                        Kind::Plain => {}
                    }
                    let name = name.child(
                        v_flex().flex_1().min_w_0()
                            .child(div().w_full().truncate().text_sm().child(SharedString::from(title)))
                            .child(div().w_full().truncate().text_xs().text_color(rgb(crate::shell::chrome::TEXT_SECONDARY)).child(SharedString::from(r.look.clone()))),
                    ).into_any_element();
                    let mut track = div().relative().w(px(TIMELINE_WIDTH - 8.)).h(px(14.)).rounded_sm().bg(rgb(0xf1f3f4)).overflow_hidden();
                    if r.delay_width > 0. {
                        track = track.child(div().absolute().top_0().h_full().left(relative((r.left - r.delay_width).max(0.))).w(relative(r.delay_width)).bg(rgb(0xe0e3e7)));
                    }
                    // The text box's row spans from its first paragraph's effect to its last.
                    let width = match kind {
                        Kind::Parent { last, .. } => rows.get(last).map_or(r.width, |l| (l.left + l.width - r.left).max(r.width)),
                        _ => r.width,
                    };
                    track = track.child(div().absolute().top_0().h_full().rounded_sm().bg(rgb(class_color)).left(relative(r.left)).w(relative(width.max(0.03))));
                    let (select, edit, delete) = (editor.clone(), editor.clone(), editor.clone());
                    let shape = shapes[i];
                    let delete_last = if let Kind::Parent { last, .. } = kind { last } else { i };
                    let mut table_row = TableRow::new(vec![order, fx, name, track.into_any_element()])
                        .selected(r.selected)
                        .on_click(move |ev, _, cx| {
                            let clicks = ev.click_count();
                            let _ = select.update(cx, |e, cx| {
                                e.set_selection(vec![shape]);
                                if clicks >= 2 {
                                    e.open_animation_options_panel(i, cx);
                                }
                                cx.notify();
                            });
                        });
                    // The text box's row is dragged with all its paragraphs' effects (`GROUP_DRAG`), and the rows next to it take drops
                    // past it (a drop on it goes before or after all its paragraphs: `animation_row_dropped`).
                    // A paragraph's row moves among the rows of its own text box; the other rows go past the paragraphs.
                    // Pressing any row's handle folds the open text boxes (the rows to move among are then the text boxes' and the plain
                    // ones); `animation_pane` opens them again when the drag is over.
                    let fold = editor.clone();
                    let paragraph_row = matches!(kind, Kind::Child { .. });
                    table_row = match kind {
                        Kind::Parent { .. } => table_row.draggable(i as u64 | GROUP_DRAG, SharedString::from(r.name.clone())).drop_between(i as u64),
                        Kind::Child { first } => table_row.draggable(i as u64, SharedString::from(r.name.clone())).drop_between(i as u64).drag_scope(first as u64 + 1),
                        Kind::Plain => table_row.draggable(i as u64, SharedString::from(r.name.clone())).drop_between(i as u64),
                    }.on_press(move |_, cx| {
                        let _ = fold.update(cx, |e, cx| e.fold_animation_boxes_for_drag(paragraph_row, cx));
                    });
                    table_row
                        .action("anim-row-edit", "tune", t("pptx.anim.options-dialog"), move |_, _, cx| {
                            let _ = edit.update(cx, |e, cx| e.open_animation_options_panel(i, cx));
                        })
                        .action_when_wide("anim-row-delete", "delete", t("_shell.delete"), move |_, _, cx| {
                            let _ = delete.update(cx, |e, cx| e.remove_effects(i, delete_last, cx));
                        })
                })
                .collect()
            }
        };
        let table_element = DataTable::virtualized(ANIMATION_TABLE, columns, header, table, shown, build, cx)
            .row_height(ROW_HEIGHT)
            .row_actions(2)
            .wide_actions(1)
            .drag_handles()
            .live_reorder()
            .row_dnd(dnd.clone(), |e, ev, _, cx| e.animation_row_dropped(ev, cx));
        // The table fills the pane's height: with many effects the scrollbar is the data rows', not the pane's.
        let table_host = div().id("anim-table-host").w_full().flex_1().min_h_0()
            .child(table_element)
            .on_mouse_down(MouseButton::Right, cx.listener(move |e, ev: &MouseDownEvent, window, cx| {
                if let Some(i) = (0..shown).find(|i| dnd.row_bounds(ANIMATION_TABLE, *i).is_some_and(|b| b.contains(&ev.position))).and_then(|d| display.get(d).map(|&(i, _)| i)) {
                    cx.stop_propagation();
                    e.open_animation_pane_context_menu(i, ev.position, window, cx);
                }
            }));

        let play = crate::shell::chrome::primary_button("anim-play-all")
            .small()
            .icon(Material("play_arrow"))
            .label(t("pptx.anim.play-all"))
            .on_click(cx.listener(|e, _, window, cx| e.start_slideshow(window, cx)));
        let ruler = div().text_xs().text_color(rgb(crate::shell::chrome::TEXT_SECONDARY))
            .child(crate::shell::i18n::format("pptx.anim.pane.total", &[("s", &format!("{:.1}", scale_ms as f32 / 1000.0))]));

        let body = crate::shell::chrome::panel_form().p_0().size_full()
            .child(h_flex().w_full().items_center().justify_between().gap_2().child(play).child(ruler))
            .child(table_host);
        Some(crate::shell::chrome::SidebarPanel { title: t("pptx.animation-ui.animation-pane"), body: body.into_any_element() }.fill())
    }

    /// A row of the animation pane dropped on another: the effect takes that place in the playing order (one undo step).
    pub(crate) fn animation_row_dropped(&mut self, ev: crate::shell::chrome::row_dnd::RowDropEvent, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        if let Some(all) = reordered_effects(&self.slide().animations, ev.from, ev.target as usize, ev.place) {
            self.run(vec![Command::SetAnimations { slide, animations: all }], cx);
        }
    }

    /// 効果のオプション詳細設定パネル (Effect Options Panel / Dialog) の描画.
    pub fn animation_options_panel(&mut self, cx: &mut Context<Self>) -> Option<crate::shell::chrome::SidebarPanel> {
        self.pptx()?.animation_options_panel.as_ref()?;
        self.render_animation_options_panel(cx)
    }

    #[inline(never)]
    fn render_animation_options_panel(&mut self, cx: &mut Context<Self>) -> Option<crate::shell::chrome::SidebarPanel> {
        use gpui_kit::component::{h_flex, v_flex};
        use gpui_kit::*;
        use crate::shell::chrome::{MenuEntry, MenuItem};

        let p = self.pptx()?.animation_options_panel.as_ref()?.clone();
        let preset_of_panel = self.slide().animations.get(p.index).map_or((0, 0), |a| a.preset);
        let t = crate::shell::i18n::text;

        let title = t("pptx.anim.options-dialog");

        let (class_name, class_color, class_bg) = match p.class {
            C::Entrance => (crate::shell::i18n::text("_pptx.entrance"), rgb(0x2e7d32), rgb(0xe8f5e9)),
            C::Emphasis => (crate::shell::i18n::text("_pptx.emphasis"), rgb(0xed6c02), rgb(0xfff3e0)),
            C::Exit => (crate::shell::i18n::text("_pptx.exit"), rgb(0xd32f2f), rgb(0xffebee)),
            C::Path => (crate::shell::i18n::text("_pptx.motion-path"), rgb(0x1976d2), rgb(0xe3f2fd)),
            C::Media => (crate::shell::i18n::text("pptx.anim.media.class"), rgb(0x1976d2), rgb(0xe3f2fd)),
        };

        let mut body = v_flex().gap_1().px(px(16.)).py(px(12.)).p_0();

        // 1. 対象アニメーション情報バナー
        let banner = h_flex()
            .items_center()
            .justify_between()
            .p_2()
            .rounded_md()
            .bg(rgb(0xf5f5f5))
            .child(
                h_flex().items_center().gap_1p5().child(
                    div()
                        .w(px(22.))
                        .h(px(22.))
                        .rounded_full()
                        .bg(rgb(0xe0e0e0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .child(format!("{}", p.index + 1)),
                )
                .child(
                    div()
                        .px_2()
                        .py_0p5()
                        .rounded_sm()
                        .bg(class_bg)
                        .text_sm()
                        .text_color(class_color)
                        .child(format!("{}・{}", class_name, look_label(p.class, p.look, preset_of_panel))), // i18n-ok（区切りの「・」だけで、訳す語がない）
                ),
            );
        body = body.child(banner);

        // 効果の変更: the effect of the same class, from a dropdown at the top (the others' options follow it).
        {
            let choices = Self::panel_effect_choices(p.class, p.look);
            let on = |look: L, id: Option<u32>| match id {
                Some(id) => p.look == L::Preset && p.preset.0 == id,
                None => p.look != L::Preset && std::mem::discriminant(&p.look) == std::mem::discriminant(&look),
            };
            let entries: Vec<MenuEntry> = choices.iter().map(|&(key, look, id)| MenuItem::tr(key).checked(on(look, id)).on(move |e, _, cx| e.set_panel_effect(look, id, cx)).into()).collect();
            body = body.child(crate::shell::chrome::dropdown_row(t("pptx.anim.panel.effect-change"), crate::shell::chrome::Dropdown::from_entries("anim-opt-effect", look_label(p.class, p.look, p.preset), entries, true, 0, &cx.weak_entity()).fill_width()));
        }

        // 2. 「効果」セクション: 方向＝セグメント、滑らかさ・自動反転＝チェックボックス、再生後の動作＝ドロップダウン
        use crate::shell::chrome::segment_control::SegmentItem;
        let section_title = |key: &str| div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(rgb(0x212121)).pt_1().child(t(key));
        body = body.child(section_title("pptx.anim.panel.effect"));

        let direction_items: Vec<SegmentItem> = match p.look {
            L::Fly(cur) => [
                (AnimationSide::Bottom, "transition.dir.down"),
                (AnimationSide::Left, "transition.dir.left"),
                (AnimationSide::Right, "transition.dir.right"),
                (AnimationSide::Top, "transition.dir.up"),
            ].into_iter().map(|(s, key)| SegmentItem::label(("anim-fly-side", s as usize), t(key), cur == s).on_click(cx.listener(move |e, _, _, cx| e.set_panel_fly_side(s, cx)))).collect(),
            L::Float(cur) => [
                (AnimationSide::Top, "pptx.anim.opt.float-up"),
                (AnimationSide::Bottom, "pptx.anim.opt.float-down"),
            ].into_iter().map(|(s, key)| SegmentItem::label(("anim-float-side", s as usize), t(key), cur == s).on_click(cx.listener(move |e, _, _, cx| e.set_panel_float_side(s, cx)))).collect(),
            L::Wipe(cur) => [
                (AnimationSide::Bottom, "pptx.anim.opt.wipe-bottom"),
                (AnimationSide::Left, "pptx.anim.opt.wipe-left"),
                (AnimationSide::Right, "pptx.anim.opt.wipe-right"),
                (AnimationSide::Top, "pptx.anim.opt.wipe-top"),
            ].into_iter().map(|(s, key)| SegmentItem::label(("anim-wipe-side", s as usize), t(key), cur == s).on_click(cx.listener(move |e, _, _, cx| e.set_panel_wipe_side(s, cx)))).collect(),
            L::Motion(cur_x, cur_y) => [
                (0, 25_000, "docx.find.dir.down"),
                (-25_000, 0, "pptx.anim.opt.path-left"),
                (25_000, 0, "pptx.anim.opt.path-right"),
                (0, -25_000, "docx.find.dir.up"),
            ].into_iter().enumerate().map(|(idx, (x, y, key))| SegmentItem::label(("anim-motion-dir", idx), t(key), cur_x == x && cur_y == y).on_click(cx.listener(move |e, _, _, cx| e.set_panel_motion_dir(x, y, cx)))).collect(),
            _ => Vec::new(),
        };
        if !direction_items.is_empty() {
            body = body.child(crate::shell::chrome::segment_control_row(t("_pptx.direction"), direction_items));
        }
        body = body
            .child(crate::shell::chrome::checkbox_button("anim-opt-smooth-start", t("pptx.anim.smooth-start"), p.accel > 0, false, cx.listener(|e, _, _, cx| e.toggle_panel_smooth_start(cx))))
            .child(crate::shell::chrome::checkbox_button("anim-opt-smooth-end", t("pptx.anim.smooth-end"), p.decel > 0, false, cx.listener(|e, _, _, cx| e.toggle_panel_smooth_end(cx))))
            .child(crate::shell::chrome::checkbox_button("anim-opt-autorev", t("pptx.anim.autoreverse"), p.auto_reverse, false, cx.listener(|e, _, _, cx| e.toggle_panel_auto_reverse(cx))));

        // 再生後の動作（排他の 4 択）
        let after_key = |a: AnimationAfterEffect| match a {
            AnimationAfterEffect::Hold => "pptx.anim.after.hold",
            AnimationAfterEffect::Hide => "pptx.anim.after.hide",
            AnimationAfterEffect::Dim => "pptx.anim.after.dim",
            AnimationAfterEffect::HideOnNextClick => "pptx.anim.after.hide-next",
        };
        let after_entries: Vec<MenuEntry> = [AnimationAfterEffect::Hold, AnimationAfterEffect::Dim, AnimationAfterEffect::Hide, AnimationAfterEffect::HideOnNextClick]
            .into_iter()
            .map(|a| MenuItem::tr(after_key(a)).checked(p.after_effect == a).on(move |e, _, cx| e.set_panel_after_effect(a, cx)).into())
            .collect();
        body = body.child(crate::shell::chrome::dropdown_row(t("_pptx.after-animation"), crate::shell::chrome::Dropdown::from_entries("anim-opt-after", t(after_key(p.after_effect)), after_entries, true, 0, &cx.weak_entity()).fill_width()));

        // 3. 「タイミング」セクション
        body = body.child(section_title("pptx.anim.panel.timing"));
        body = body.child(crate::shell::chrome::segment_control_row(
            t("pptx.animation-ui.from"),
            [
                (AnimationStart::OnClick, "pptx.anim.start.click", "anim-opt-start-click"),
                (AnimationStart::WithPrevious, "_pptx.with-previous-2", "anim-opt-start-with"),
                (AnimationStart::AfterPrevious, "_pptx.after-previous-2", "anim-opt-start-after"),
            ].into_iter().map(|(st, key, id)| SegmentItem::label(id, t(key), p.start == st).on_click(cx.listener(move |e, _, _, cx| e.set_panel_start(st, cx)))),
        ));

        // 継続時間・遅延: ツールバーのフォント サイズの部品（背景なし）
        let unit = t("pptx.anim.unit.seconds");
        body = body.child(crate::shell::chrome::control_row(
            t("_pptx.duration"),
            crate::shell::chrome::number_stepper("anim-opt-duration", p.duration_ms as f32 / 1000.0, cx, |e, v, _, cx| e.set_panel_duration_ms((v * 1000.0).round() as u32, cx))
                .range(0.1, 60.0).step(0.5).unit(unit.clone()).candidates(DURATION_CANDIDATES).plain(),
        ));
        body = body.child(crate::shell::chrome::control_row(
            t("_pptx.delay"),
            crate::shell::chrome::number_stepper("anim-opt-delay", p.delay_ms as f32 / 1000.0, cx, |e, v, _, cx| e.set_panel_delay_ms((v * 1000.0).round() as u32, cx))
                .range(0.0, 60.0).step(0.5).unit(unit).candidates(DELAY_CANDIDATES).plain(),
        ));

        // 繰り返し（排他の選択肢）
        let repeat_key = |r: AnimationRepeat| match r {
            AnimationRepeat::None => "pptx.anim.repeat.none",
            AnimationRepeat::Count(2) => "pptx.anim.repeat.2",
            AnimationRepeat::Count(3) => "pptx.anim.repeat.3",
            AnimationRepeat::Count(4) => "pptx.anim.repeat.4",
            AnimationRepeat::Count(5) => "pptx.anim.repeat.5",
            AnimationRepeat::Count(10) => "pptx.anim.repeat.10",
            AnimationRepeat::Count(_) => "_pptx.multiple-times",
            AnimationRepeat::UntilNextClick => "pptx.anim.repeat.until-click",
            AnimationRepeat::UntilEndOfSlide => "pptx.anim.repeat.until-end",
        };
        let repeat_entries: Vec<MenuEntry> = [
            AnimationRepeat::None, AnimationRepeat::Count(2), AnimationRepeat::Count(3), AnimationRepeat::Count(4), AnimationRepeat::Count(5),
            AnimationRepeat::Count(10), AnimationRepeat::UntilNextClick, AnimationRepeat::UntilEndOfSlide,
        ].into_iter().map(|r| MenuItem::tr(repeat_key(r)).checked(p.repeat_count == r).on(move |e, _, cx| e.set_panel_repeat(r, cx)).into()).collect();
        body = body.child(crate::shell::chrome::dropdown_row(t("_pptx.repeat"), crate::shell::chrome::Dropdown::from_entries("anim-opt-repeat", t(repeat_key(p.repeat_count)), repeat_entries, true, 0, &cx.weak_entity()).fill_width()));
        body = body.child(crate::shell::chrome::checkbox_button("anim-opt-rewind", t("pptx.anim.rewind"), p.rewind, false, cx.listener(|e, _, _, cx| e.toggle_panel_rewind(cx))));

        // トリガー: 通常進行（クリック・自動）か図形のクリック時。同じ画面の中で選ぶ（図形のクリック時のときだけ対象の図形が出る）。
        let slide_shapes: Vec<(ShapeId, String)> = {
            let slide = self.slide();
            slide.shapes.iter().map(|sh| {
                let label = sh.text_body()
                    .map(|b| b.text().chars().take(12).collect::<String>())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| crate::shell::i18n::format("_pptx.shape-n", &[("n", &(slide.shape_index(sh.id).map_or(0, |x| x + 1)))]).to_string());
                (sh.id, label)
            }).collect()
        };
        let trig_shape = match &p.trigger { AnimationTrigger::OnShapeClick(s) => Some(*s), _ => None };
        let first_other = slide_shapes.iter().map(|(id, _)| *id).find(|id| *id != p.shape).or(Some(p.shape));
        // 通常進行（クリック・自動）か図形のクリック時の 2 択はラジオボタン。ブックマークは設定済みのときだけ 3 つ目に出る。
        let mut trigger_radios = crate::shell::chrome::radio_group()
            .child(crate::shell::chrome::radio_button("anim-opt-trigger-normal", p.trigger.is_none(), false, t("_pptx.normal-click-or-auto"),
                cx.listener(|e, _, _, cx| e.set_panel_trigger(AnimationTrigger::None, cx))))
            .child(crate::shell::chrome::radio_button("anim-opt-trigger-shape-click", trig_shape.is_some(), first_other.is_none(), t("pptx.anim.trigger.shape-click"),
                cx.listener(move |e, _, _, cx| { if let Some(s) = first_other { e.set_panel_trigger(AnimationTrigger::OnShapeClick(trig_shape.unwrap_or(s)), cx) } })));
        if let AnimationTrigger::OnMediaBookmark { bookmark, .. } = &p.trigger {
            trigger_radios = trigger_radios.child(crate::shell::chrome::radio_button("anim-opt-trigger-bookmark", true, false,
                crate::shell::i18n::format("_pptx.on-bookmark-bookmark", &[("bookmark", bookmark)]), |_, _, _| {}));
        }
        body = body.child(crate::shell::chrome::control_row(t("_pptx.trigger-2"), trigger_radios).stacked());
        if let Some(current) = trig_shape {
            let current_label = slide_shapes.iter().find(|(id, _)| *id == current).map(|(_, l)| l.clone()).unwrap_or_default();
            let shape_entries: Vec<MenuEntry> = slide_shapes.iter().map(|(id, label)| {
                let id = *id;
                MenuItem::new("pptx.anim.trigger.shape-item", label.clone()).checked(id == current).on(move |e, _, cx| e.set_panel_trigger(AnimationTrigger::OnShapeClick(id), cx)).into()
            }).collect();
            body = body.child(crate::shell::chrome::dropdown_row(t("pptx.anim.trigger.target"), crate::shell::chrome::Dropdown::from_entries("anim-opt-trigger-shape", current_label, shape_entries, true, 0, &cx.weak_entity()).fill_width()));
        }

        // 4. フッターボタン
        let footer = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("pptx-anim-opt-apply", t("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, _, cx| e.apply_animation_options_panel(cx))))
            .action(crate::shell::chrome::ActionDefinition::new("pptx-anim-opt-cancel", t("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, _, cx| e.close_animation_options_panel(cx))))
            .action(crate::shell::chrome::ActionDefinition::new("pptx-anim-opt-delete", t("_shell.delete"), crate::shell::chrome::ActionButtonKind::Destructive).on_click({
                let index = p.index;
                cx.listener(move |e, _, _, cx| { e.close_animation_options_panel(cx); e.remove_effect(index, cx) })
            }));

        let panel_terminal_actions = footer;
        Some(crate::shell::chrome::SidebarPanel::with_footer(title, body.into_any_element(), panel_terminal_actions))
    }

    /// 開始のタイミング of the selected shapes' effects.
    pub(crate) fn set_effect_start(&mut self, start: AnimationStart, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape)) {
                a.start = start;
            }
        });
    }

    /// 継続時間 of the selected shapes' effects (not an appear, which is at once).
    pub(crate) fn set_effect_duration(&mut self, ms: u32, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape) && a.look != L::Appear) {
                a.duration = ms;
            }
        });
    }

    /// 開始遅延 of the selected shapes' effects.
    pub(crate) fn set_effect_delay(&mut self, ms: u32, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape)) {
                a.delay = ms;
            }
        });
    }

    /// 繰り返し of the selected shapes' effects.
    pub(crate) fn set_effect_repeat(&mut self, repeat: AnimationRepeat, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape)) {
                a.repeat_count = repeat;
            }
        });
    }

    /// 自動反転 of the selected shapes' effects.
    pub(crate) fn toggle_effect_autoreverse(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        let all_rev = self.slide().animations.iter().filter(|a| !a.is_media() && selected.contains(&a.shape)).all(|a| a.auto_reverse);
        let new_rev = !all_rev;
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape)) {
                a.auto_reverse = new_rev;
            }
        });
    }

    /// 再生終了後に巻き戻す (rewind) of the selected shapes' effects.
    pub(crate) fn toggle_effect_rewind(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        let all_rew = self.slide().animations.iter().filter(|a| !a.is_media() && selected.contains(&a.shape)).all(|a| a.rewind);
        let new_rew = !all_rew;
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape)) {
                a.rewind = new_rew;
            }
        });
    }

    /// 再生終了後に巻き戻す (rewind) of the selected shapes' effects.
    #[allow(dead_code)]
    pub(crate) fn set_effect_rewind(&mut self, rewind: bool, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape)) {
                a.rewind = rewind;
            }
        });
    }

    /// 滑らかさ of the selected shapes' effects.
    pub(crate) fn set_effect_smooth(&mut self, accel: u32, decel: u32, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape)) {
                a.accel = accel;
                a.decel = decel;
            }
        });
    }

    /// 滑らかに開始 of the selected shapes' effects.
    pub(crate) fn toggle_effect_smooth_start(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        let all_smooth = self.slide().animations.iter().filter(|a| !a.is_media() && selected.contains(&a.shape)).all(|a| a.accel > 0);
        let new_accel = if all_smooth { 0 } else { 50_000 };
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape)) {
                a.accel = new_accel;
            }
        });
    }

    /// 滑らかに終了 of the selected shapes' effects.
    pub(crate) fn toggle_effect_smooth_end(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        let all_smooth = self.slide().animations.iter().filter(|a| !a.is_media() && selected.contains(&a.shape)).all(|a| a.decel > 0);
        let new_decel = if all_smooth { 0 } else { 50_000 };
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape)) {
                a.decel = new_decel;
            }
        });
    }

    /// 効果終了時の動作 of the selected shapes' effects.
    pub(crate) fn set_effect_after(&mut self, after: AnimationAfterEffect, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape)) {
                a.after_effect = after;
            }
        });
    }

    /// トリガー (OnShapeClick, OnMediaBookmark, None) of the selected shapes' effects.
    pub(crate) fn set_selected_effects_trigger(&mut self, trigger: AnimationTrigger, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape)) {
                a.trigger = trigger.clone();
            }
        });
    }

    /// トリガーの解除 (通常進行に戻す) of the selected shapes' effects.
    pub(crate) fn clear_selected_effects_trigger(&mut self, cx: &mut Context<Self>) {
        self.set_selected_effects_trigger(AnimationTrigger::None, cx);
    }

    /// アニメーションの削除: the selected shapes' effects.
    pub(crate) fn remove_shape_effects(&mut self, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| all.retain(|a| !(!a.is_media() && selected.contains(&a.shape))));
    }

    /// 効果のオプション of the selected shapes' effects.
    pub(crate) fn set_shape_effect_option(&mut self, new_look: L, cx: &mut Context<Self>) {
        self.change_animations(cx, |all, selected| {
            let side_subtype = |side: AnimationSide| match side {
                AnimationSide::Top => 1,
                AnimationSide::Right => 2,
                AnimationSide::Bottom => 4,
                AnimationSide::Left => 8,
            };
            for a in all.iter_mut().filter(|a| !a.is_media() && selected.contains(&a.shape)) {
                match (a.look, new_look) {
                    (L::Fly(_), L::Fly(side)) => {
                        a.look = L::Fly(side);
                        a.preset.1 = side_subtype(side);
                    }
                    (L::Float(_), L::Float(side)) => {
                        a.look = L::Float(side);
                        a.preset = (30, if side == AnimationSide::Bottom { 2 } else { 1 });
                    }
                    (L::Wipe(_), L::Wipe(side)) => {
                        a.look = L::Wipe(side);
                        a.preset = (22, side_subtype(side));
                    }
                    (L::Zoom, L::Zoom) => {
                        a.look = L::Zoom;
                        a.preset = (23, if a.class == C::Exit { 32 } else { 16 });
                    }
                    (L::Spin(_), L::Spin(by)) => {
                        a.look = L::Spin(by);
                        a.preset = (8, 0);
                    }
                    (L::Scale(..), L::Scale(x, y)) => {
                        a.look = L::Scale(x, y);
                        a.preset = (6, 0);
                    }
                    (L::Motion(..), L::Motion(x, y)) => {
                        let subtype = if x == 0 && y > 0 {
                            4
                        } else if x < 0 && y == 0 {
                            8
                        } else if x > 0 && y == 0 {
                            2
                        } else if x == 0 && y < 0 {
                            1
                        } else {
                            0
                        };
                        a.look = L::Motion(x, y);
                        a.preset = (61, subtype);
                    }
                    _ => {}
                }
            }
        });
    }

    /// テキストの動き: the selected shapes' effects on the shape as a whole (`Whole`), one for each paragraph of its text (each
    /// started by a click, the first as the effect was), or on the text a word or a letter at a time (`Word`, `Letter`: one effect,
    /// each next word or letter begins a tenth of the effect's time after the one before).
    pub(crate) fn set_effect_text_build(&mut self, build: TextBuild, cx: &mut Context<Self>) {
        let counts: std::collections::HashMap<ShapeId, usize> = self.selected_shapes().into_iter().filter_map(|s| Some((s.id, s.text_body()?.paragraphs.len()))).collect();
        self.change_animations(cx, |all, selected| {
            for &shape in selected {
                if build == TextBuild::Paragraph {
                    let n = counts.get(&shape).copied().unwrap_or(0);
                    if n < 2 {
                        continue;
                    }
                    let mut out = Vec::with_capacity(all.len() + n);
                    for a in all.drain(..) {
                        if a.shape == shape && a.paragraph.is_none() && a.trigger.is_none() {
                            for k in 0..n {
                                let mut each = a.clone();
                                each.paragraph = Some((k as u32, k as u32));
                                each.text_unit = None;
                                if k > 0 {
                                    each.start = AnimationStart::OnClick;
                                    each.delay = 0;
                                }
                                out.push(each);
                            }
                        } else {
                            out.push(a);
                        }
                    }
                    *all = out;
                } else {
                    // The shape as a whole again: the paragraphs' effects are one.
                    let mut seen = false;
                    all.retain_mut(|a| {
                        if a.shape != shape || a.paragraph.is_none() {
                            return true;
                        }
                        let keep = !seen;
                        seen = true;
                        a.paragraph = None;
                        keep
                    });
                    let unit = match build {
                        TextBuild::Word => Some((ibis_doc_model::TextUnit::Word, ibis_doc_model::UnitGap::Percent(10_000))),
                        TextBuild::Letter => Some((ibis_doc_model::TextUnit::Letter, ibis_doc_model::UnitGap::Percent(10_000))),
                        _ => None,
                    };
                    for a in all.iter_mut().filter(|a| a.shape == shape && a.paragraph.is_none() && a.chars.is_none()) {
                        a.text_unit = unit;
                    }
                }
            }
        });
    }

    /// グラフの系列・カテゴリ別アニメーション: build the selected chart shapes' effects by series, category, or element.
    pub(crate) fn set_effect_chart_build(&mut self, build: ibis_doc_model::ChartBuildStep, anim_bg: bool, cx: &mut Context<Self>) {
        let chart_info: std::collections::HashMap<ShapeId, (usize, usize)> = self.selected_shapes().into_iter().filter_map(|s| {
            if let ibis_doc_model::ShapeKind::Chart(c) = &s.kind {
                let series_count = c.groups.iter().map(|g| g.series.len()).sum::<usize>().max(1);
                let category_count = c.groups.iter().flat_map(|g| g.series.iter().map(|s| s.values.len())).max().unwrap_or(1);
                Some((s.id, (series_count, category_count)))
            } else {
                None
            }
        }).collect();

        self.change_animations(cx, |all, selected| {
            for &shape in selected {
                let Some(&(series_count, category_count)) = chart_info.get(&shape) else { continue };

                // Find the first effect on this shape to use as base, or make a default appear entrance effect
                let base = all.iter().find(|a| a.shape == shape).cloned().unwrap_or_else(|| effect(shape, C::Entrance, L::Appear));

                let mut out = Vec::with_capacity(all.len() + series_count * category_count + 1);
                let mut replaced = false;

                let build_step_anims = {
                    let mut anims = Vec::new();
                    match build {
                        ibis_doc_model::ChartBuildStep::AllAtOnce => {
                            let mut a = base.clone();
                            a.chart_element = None;
                            a.chart_build = Some((ibis_doc_model::ChartBuildStep::AllAtOnce, anim_bg));
                            anims.push(a);
                        }
                        ibis_doc_model::ChartBuildStep::Series => {
                            if anim_bg {
                                let mut bg = base.clone();
                                bg.chart_element = Some(ibis_doc_model::ChartAnimationElement { series_idx: -1, category_idx: -1 });
                                bg.chart_build = Some((ibis_doc_model::ChartBuildStep::Series, anim_bg));
                                anims.push(bg);
                            }
                            for s in 0..series_count {
                                let mut a = base.clone();
                                if !anim_bg && s == 0 {
                                    // keep base start
                                } else {
                                    a.start = AnimationStart::OnClick;
                                    a.delay = 0;
                                }
                                a.chart_element = Some(ibis_doc_model::ChartAnimationElement { series_idx: s as i32, category_idx: -1 });
                                a.chart_build = Some((ibis_doc_model::ChartBuildStep::Series, anim_bg));
                                anims.push(a);
                            }
                        }
                        ibis_doc_model::ChartBuildStep::Category => {
                            if anim_bg {
                                let mut bg = base.clone();
                                bg.chart_element = Some(ibis_doc_model::ChartAnimationElement { series_idx: -1, category_idx: -1 });
                                bg.chart_build = Some((ibis_doc_model::ChartBuildStep::Category, anim_bg));
                                anims.push(bg);
                            }
                            for c in 0..category_count {
                                let mut a = base.clone();
                                if !anim_bg && c == 0 {
                                    // keep base start
                                } else {
                                    a.start = AnimationStart::OnClick;
                                    a.delay = 0;
                                }
                                a.chart_element = Some(ibis_doc_model::ChartAnimationElement { series_idx: -1, category_idx: c as i32 });
                                a.chart_build = Some((ibis_doc_model::ChartBuildStep::Category, anim_bg));
                                anims.push(a);
                            }
                        }
                        ibis_doc_model::ChartBuildStep::SeriesEl => {
                            if anim_bg {
                                let mut bg = base.clone();
                                bg.chart_element = Some(ibis_doc_model::ChartAnimationElement { series_idx: -1, category_idx: -1 });
                                bg.chart_build = Some((ibis_doc_model::ChartBuildStep::SeriesEl, anim_bg));
                                anims.push(bg);
                            }
                            let mut first = true;
                            for s in 0..series_count {
                                for c in 0..category_count {
                                    let mut a = base.clone();
                                    if !anim_bg && first {
                                        // keep base start
                                    } else {
                                        a.start = AnimationStart::OnClick;
                                        a.delay = 0;
                                    }
                                    first = false;
                                    a.chart_element = Some(ibis_doc_model::ChartAnimationElement { series_idx: s as i32, category_idx: c as i32 });
                                    a.chart_build = Some((ibis_doc_model::ChartBuildStep::SeriesEl, anim_bg));
                                    anims.push(a);
                                }
                            }
                        }
                        ibis_doc_model::ChartBuildStep::CategoryEl => {
                            if anim_bg {
                                let mut bg = base.clone();
                                bg.chart_element = Some(ibis_doc_model::ChartAnimationElement { series_idx: -1, category_idx: -1 });
                                bg.chart_build = Some((ibis_doc_model::ChartBuildStep::CategoryEl, anim_bg));
                                anims.push(bg);
                            }
                            let mut first = true;
                            for c in 0..category_count {
                                for s in 0..series_count {
                                    let mut a = base.clone();
                                    if !anim_bg && first {
                                        // keep base start
                                    } else {
                                        a.start = AnimationStart::OnClick;
                                        a.delay = 0;
                                    }
                                    first = false;
                                    a.chart_element = Some(ibis_doc_model::ChartAnimationElement { series_idx: s as i32, category_idx: c as i32 });
                                    a.chart_build = Some((ibis_doc_model::ChartBuildStep::CategoryEl, anim_bg));
                                    anims.push(a);
                                }
                            }
                        }
                    }
                    anims
                };

                for a in all.drain(..) {
                    if a.shape == shape {
                        if !replaced {
                            out.extend(build_step_anims.clone());
                            replaced = true;
                        }
                    } else {
                        out.push(a);
                    }
                }
                if !replaced {
                    out.extend(build_step_anims);
                }
                *all = out;
            }
        });
    }

    /// スマートアートの要素別アニメーション: build the selected SmartArt diagram shapes' effects by element.
    pub(crate) fn set_effect_diagram_build(&mut self, build: ibis_doc_model::DiagramBuildStep, cx: &mut Context<Self>) {
        let slide = self.slide();
        let dgm_info: std::collections::HashMap<ShapeId, Vec<ShapeId>> = self.selected_shapes().into_iter().filter_map(|s| {
            if matches!(s.kind, ibis_doc_model::ShapeKind::Placeholder(ibis_doc_model::Placeholder::Diagram)) {
                let attached = slide.attached_to(s.id);
                Some((s.id, attached))
            } else {
                None
            }
        }).collect();

        self.change_animations(cx, |all, selected| {
            for &shape in selected {
                let Some(attached) = dgm_info.get(&shape) else { continue };

                // Find the first effect on this shape to use as base, or make a default appear entrance effect
                let base = all.iter().find(|a| a.shape == shape).cloned().unwrap_or_else(|| effect(shape, C::Entrance, L::Appear));

                let mut out = Vec::with_capacity(all.len() + attached.len() + 1);
                let mut replaced = false;

                let build_step_anims = {
                    let mut anims = Vec::new();
                    match build {
                        ibis_doc_model::DiagramBuildStep::Whole => {
                            let mut a = base.clone();
                            a.dgm_element = None;
                            a.dgm_build = Some(ibis_doc_model::DiagramBuildStep::Whole);
                            anims.push(a);
                        }
                        ibis_doc_model::DiagramBuildStep::AllAtOnce => {
                            if attached.is_empty() {
                                let mut a = base.clone();
                                a.dgm_element = None;
                                a.dgm_build = Some(ibis_doc_model::DiagramBuildStep::AllAtOnce);
                                anims.push(a);
                            } else {
                                for (i, &child_id) in attached.iter().enumerate() {
                                    let mut a = base.clone();
                                    if i > 0 {
                                        a.start = AnimationStart::WithPrevious;
                                        a.delay = 0;
                                    }
                                    a.dgm_element = Some(ibis_doc_model::DiagramAnimationElement {
                                        id: child_id.0.to_string(),
                                        bld_step: "sp".to_string(),
                                    });
                                    a.dgm_build = Some(ibis_doc_model::DiagramBuildStep::AllAtOnce);
                                    anims.push(a);
                                }
                            }
                        }
                        ibis_doc_model::DiagramBuildStep::DepthByNode | ibis_doc_model::DiagramBuildStep::BreadthByNode => {
                            // 1つずつ (One by One)
                            if attached.is_empty() {
                                let mut a = base.clone();
                                a.dgm_element = None;
                                a.dgm_build = Some(build);
                                anims.push(a);
                            } else {
                                for (i, &child_id) in attached.iter().enumerate() {
                                    let mut a = base.clone();
                                    if i > 0 {
                                        a.start = AnimationStart::OnClick;
                                        a.delay = 0;
                                    }
                                    a.dgm_element = Some(ibis_doc_model::DiagramAnimationElement {
                                        id: child_id.0.to_string(),
                                        bld_step: "sp".to_string(),
                                    });
                                    a.dgm_build = Some(build);
                                    anims.push(a);
                                }
                            }
                        }
                        ibis_doc_model::DiagramBuildStep::BreadthByLvl => {
                            // レベルごと (Level at once)
                            if attached.is_empty() {
                                let mut a = base.clone();
                                a.dgm_element = None;
                                a.dgm_build = Some(build);
                                anims.push(a);
                            } else {
                                for (i, &child_id) in attached.iter().enumerate() {
                                    let mut a = base.clone();
                                    if i > 0 {
                                        a.start = AnimationStart::OnClick;
                                        a.delay = 0;
                                    }
                                    a.dgm_element = Some(ibis_doc_model::DiagramAnimationElement {
                                        id: child_id.0.to_string(),
                                        bld_step: "sp".to_string(),
                                    });
                                    a.dgm_build = Some(build);
                                    anims.push(a);
                                }
                            }
                        }
                        ibis_doc_model::DiagramBuildStep::DepthByBranch | ibis_doc_model::DiagramBuildStep::Cust => {
                            // レベルで1つずつ (Level one by one / By branch)
                            if attached.is_empty() {
                                let mut a = base.clone();
                                a.dgm_element = None;
                                a.dgm_build = Some(build);
                                anims.push(a);
                            } else {
                                for (i, &child_id) in attached.iter().enumerate() {
                                    let mut a = base.clone();
                                    if i > 0 {
                                        a.start = AnimationStart::OnClick;
                                        a.delay = 0;
                                    }
                                    a.dgm_element = Some(ibis_doc_model::DiagramAnimationElement {
                                        id: child_id.0.to_string(),
                                        bld_step: "sp".to_string(),
                                    });
                                    a.dgm_build = Some(build);
                                    anims.push(a);
                                }
                            }
                        }
                    }
                    anims
                };

                for a in all.drain(..) {
                    if a.shape == shape {
                        if !replaced {
                            out.extend(build_step_anims.clone());
                            replaced = true;
                        }
                    } else {
                        out.push(a);
                    }
                }
                if !replaced {
                    out.extend(build_step_anims);
                }
                *all = out;
            }
        });
    }

    /// アニメーションのコピー: copy the animation effects of the selected shape.
    pub(crate) fn copy_shape_animations(&mut self, cx: &mut Context<Self>) {
        let selected = self.selected_ids();
        let Some(&shape_id) = selected.first() else { return };
        let animations: Vec<ibis_doc_model::Animation> = self.slide().animations.iter().filter(|a| a.shape == shape_id && !a.is_media()).cloned().collect();
        if animations.is_empty() {
            return;
        }
        if let Some(s) = self.pptx_mut() {
            s.animation_copy = Some(animations);
        }
        self.set_status(crate::shell::i18n::text("pptx.anim.copied").to_string(), cx);
        cx.notify();
    }

    /// アニメーションの貼り付け: paste the copied animation effects onto the selected shapes.
    pub(crate) fn paste_shape_animations(&mut self, cx: &mut Context<Self>) {
        let copied = self.pptx().and_then(|s| s.animation_copy.clone());
        let Some(copied) = copied else { return };
        if copied.is_empty() {
            return;
        }
        self.change_animations(cx, |all, selected| {
            for &shape in selected {
                all.retain(|a| a.shape != shape);
                for template in &copied {
                    let mut anim = template.clone();
                    anim.shape = shape;
                    all.push(anim);
                }
            }
        });
        self.set_status(crate::shell::i18n::text("pptx.anim.pasted").to_string(), cx);
    }

    /// Whether animation effects can be copied from the current selection.
    pub(crate) fn can_copy_animations(&self) -> bool {
        let selected = self.selected_ids();
        let Some(&shape_id) = selected.first() else { return false };
        self.slide().animations.iter().any(|a| a.shape == shape_id && !a.is_media())
    }

    /// Whether copied animation effects can be pasted onto the current selection.
    pub(crate) fn can_paste_animations(&self) -> bool {
        let has_copied = self.pptx().is_some_and(|s| s.animation_copy.as_ref().is_some_and(|c| !c.is_empty()));
        has_copied && !self.selected_ids().is_empty()
    }

    /// アニメーションの書式設定（Animation Painter）ボタンのトグル動作.
    pub(crate) fn toggle_animation_painter(&mut self, cx: &mut Context<Self>) {
        if self.can_copy_animations() {
            self.copy_shape_animations(cx);
        } else if self.can_paste_animations() {
            self.paste_shape_animations(cx);
        }
    }
}

/// The name shown for an effect: its look's, or, for the effects of PowerPoint's presets, the preset's (with its option).
fn look_label(class: C, look: L, preset: (u32, u32)) -> String {
    if look == L::Preset || (class == C::Path && !STRAIGHT_PATHS.contains(&preset.0) && ibis_pptx::animation_catalog::effect(class, preset.0).is_some()) {
        if is_filter_preset(class, preset.0) {
            let def = ibis_doc_model::animation_preset::preset_def(preset.0).expect("a filter preset");
            let name = crate::shell::i18n::text(def.key);
            return match def.options.iter().find(|(s, _, _)| *s == preset.1) {
                Some((_, option, _)) => format!("{name}（{}）", crate::shell::i18n::text(option)), // i18n-ok（括弧だけ）
                None => name.to_string(),
            };
        }
        if let Some(def) = ibis_pptx::animation_catalog::effect(class, preset.0) {
            let name = crate::shell::i18n::text(def.key);
            return match def.options.iter().find(|(s, _)| *s == preset.1).filter(|_| def.options.len() > 1) {
                Some((_, option)) => format!("{name}（{}）", crate::shell::i18n::text(option)), // i18n-ok（括弧だけ）
                None => name.to_string(),
            };
        }
        return crate::shell::i18n::text("pptx.anim.preset.other").to_string();
    }
    crate::shell::i18n::text(look_id(class, look)).to_string()
}

/// The menu id (and label) of an effect's look.
fn look_id(class: C, look: L) -> &'static str {
    match (class, look) {
        (C::Entrance, L::Appear) => "pptx.anim.in.appear",
        (C::Entrance, L::Fade) => "pptx.anim.in.fade",
        (C::Entrance, L::Fly(AnimationSide::Bottom)) => "pptx.anim.in.fly-bottom",
        (C::Entrance, L::Fly(AnimationSide::Left)) => "pptx.anim.in.fly-left",
        (C::Entrance, L::Fly(AnimationSide::Right)) => "pptx.anim.in.fly-right",
        (C::Entrance, L::Fly(AnimationSide::Top)) => "pptx.anim.in.fly-top",
        (C::Entrance, L::Float(AnimationSide::Top)) => "pptx.anim.in.float",
        (C::Entrance, L::Float(AnimationSide::Bottom)) => "pptx.anim.in.float-down",
        (C::Entrance, L::Float(_)) => "pptx.anim.in.float",
        (C::Entrance, L::Wipe(AnimationSide::Bottom)) => "pptx.anim.in.wipe-bottom",
        (C::Entrance, L::Wipe(AnimationSide::Left)) => "pptx.anim.in.wipe-left",
        (C::Entrance, L::Wipe(AnimationSide::Right)) => "pptx.anim.in.wipe-right",
        (C::Entrance, L::Wipe(AnimationSide::Top)) => "pptx.anim.in.wipe-top",
        (C::Entrance, L::Zoom) => "pptx.anim.in.zoom",
        (C::Entrance, L::Replay) => "pptx.anim.in.replay",
        (C::Exit, L::Appear) => "pptx.anim.out.disappear",
        (C::Exit, L::Fade) => "pptx.anim.out.fade",
        (C::Exit, L::Fly(AnimationSide::Bottom)) => "pptx.anim.out.fly-bottom",
        (C::Exit, L::Fly(AnimationSide::Left)) => "pptx.anim.out.fly-left",
        (C::Exit, L::Fly(AnimationSide::Right)) => "pptx.anim.out.fly-right",
        (C::Exit, L::Fly(AnimationSide::Top)) => "pptx.anim.out.fly-top",
        (C::Exit, L::Float(AnimationSide::Bottom)) => "pptx.anim.out.float",
        (C::Exit, L::Float(AnimationSide::Top)) => "pptx.anim.out.float-up",
        (C::Exit, L::Float(_)) => "pptx.anim.out.float",
        (C::Exit, L::Wipe(AnimationSide::Bottom)) => "pptx.anim.out.wipe-bottom",
        (C::Exit, L::Wipe(AnimationSide::Left)) => "pptx.anim.out.wipe-left",
        (C::Exit, L::Wipe(AnimationSide::Right)) => "pptx.anim.out.wipe-right",
        (C::Exit, L::Wipe(AnimationSide::Top)) => "pptx.anim.out.wipe-top",
        (C::Exit, L::Zoom) => "pptx.anim.out.zoom",
        (C::Exit, L::Replay) => "pptx.anim.out.rewind",
        (C::Emphasis, L::Spin(_)) => "pptx.anim.em.spin",
        (C::Emphasis, L::Scale(..)) => "pptx.anim.em.grow",
        (C::Path, L::Motion(x, y)) if x == 0 && y > 0 => "pptx.anim.path.down",
        (C::Path, L::Motion(x, y)) if x < 0 && y == 0 => "pptx.anim.path.left",
        (C::Path, L::Motion(x, y)) if x > 0 && y == 0 => "pptx.anim.path.right",
        (C::Path, L::Motion(x, y)) if x == 0 && y < 0 => "pptx.anim.path.up",
        (C::Path, _) => "pptx.anim.path",
        (C::Media, L::Media(ibis_doc_model::MediaCommand::Play)) => "pptx.anim.media.play",
        (C::Media, L::Media(ibis_doc_model::MediaCommand::Pause)) => "pptx.anim.media.pause",
        (C::Media, L::Media(ibis_doc_model::MediaCommand::Stop)) => "pptx.anim.media.stop",
        _ => "pptx.anim.other",
    }
}

/// Whether the selected shapes have an animation supporting effect options (direction, rotation, scale, or details panel).
pub(crate) fn has_animation_options(editor: &SlideEditor) -> bool {
    let selected = editor.selected_ids();
    editor.slide().animations.iter().any(|a| !a.is_media() && selected.contains(&a.shape))
}

/// The options menu for the currently selected shape's animation (direction, rotation, scale, or details panel).
pub(crate) fn animation_option_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let selected = editor.selected_ids();
    let mine: Vec<&Animation> = editor.slide().animations.iter().filter(|a| !a.is_media() && selected.contains(&a.shape)).collect();
    let first = mine.first();
    let mut items: Vec<MenuEntry> = match first.map(|a| a.look) {
        Some(L::Fly(current_side)) => {
            vec![
                MenuItem::tr("pptx.anim.opt.fly-bottom").checked(current_side == AnimationSide::Bottom).on(|e, _, cx| e.set_shape_effect_option(L::Fly(AnimationSide::Bottom), cx)).into(),
                MenuItem::tr("pptx.anim.opt.fly-left").checked(current_side == AnimationSide::Left).on(|e, _, cx| e.set_shape_effect_option(L::Fly(AnimationSide::Left), cx)).into(),
                MenuItem::tr("pptx.anim.opt.fly-right").checked(current_side == AnimationSide::Right).on(|e, _, cx| e.set_shape_effect_option(L::Fly(AnimationSide::Right), cx)).into(),
                MenuItem::tr("pptx.anim.opt.fly-top").checked(current_side == AnimationSide::Top).on(|e, _, cx| e.set_shape_effect_option(L::Fly(AnimationSide::Top), cx)).into(),
            ]
        }
        Some(L::Float(current_side)) => {
            vec![
                MenuItem::tr("pptx.anim.opt.float-up").checked(current_side == AnimationSide::Top).on(|e, _, cx| e.set_shape_effect_option(L::Float(AnimationSide::Top), cx)).into(),
                MenuItem::tr("pptx.anim.opt.float-down").checked(current_side == AnimationSide::Bottom).on(|e, _, cx| e.set_shape_effect_option(L::Float(AnimationSide::Bottom), cx)).into(),
            ]
        }
        Some(L::Wipe(current_side)) => {
            vec![
                MenuItem::tr("pptx.anim.opt.wipe-bottom").checked(current_side == AnimationSide::Bottom).on(|e, _, cx| e.set_shape_effect_option(L::Wipe(AnimationSide::Bottom), cx)).into(),
                MenuItem::tr("pptx.anim.opt.wipe-left").checked(current_side == AnimationSide::Left).on(|e, _, cx| e.set_shape_effect_option(L::Wipe(AnimationSide::Left), cx)).into(),
                MenuItem::tr("pptx.anim.opt.wipe-right").checked(current_side == AnimationSide::Right).on(|e, _, cx| e.set_shape_effect_option(L::Wipe(AnimationSide::Right), cx)).into(),
                MenuItem::tr("pptx.anim.opt.wipe-top").checked(current_side == AnimationSide::Top).on(|e, _, cx| e.set_shape_effect_option(L::Wipe(AnimationSide::Top), cx)).into(),
            ]
        }
        Some(L::Spin(current_by)) => {
            vec![
                MenuItem::tr("pptx.anim.opt.spin-360-cw").checked(current_by == 21_600_000).on(|e, _, cx| e.set_shape_effect_option(L::Spin(21_600_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.spin-360-ccw").checked(current_by == -21_600_000).on(|e, _, cx| e.set_shape_effect_option(L::Spin(-21_600_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.spin-180").checked(current_by == 10_800_000).on(|e, _, cx| e.set_shape_effect_option(L::Spin(10_800_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.spin-720").checked(current_by == 43_200_000).on(|e, _, cx| e.set_shape_effect_option(L::Spin(43_200_000), cx)).into(),
            ]
        }
        Some(L::Scale(current_x, _)) => {
            vec![
                MenuItem::tr("pptx.anim.opt.scale-150").checked(current_x == 150_000).on(|e, _, cx| e.set_shape_effect_option(L::Scale(150_000, 150_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.scale-200").checked(current_x == 200_000).on(|e, _, cx| e.set_shape_effect_option(L::Scale(200_000, 200_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.scale-50").checked(current_x == 50_000).on(|e, _, cx| e.set_shape_effect_option(L::Scale(50_000, 50_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.scale-25").checked(current_x == 25_000).on(|e, _, cx| e.set_shape_effect_option(L::Scale(25_000, 25_000), cx)).into(),
            ]
        }
        Some(L::Motion(current_x, current_y)) => {
            vec![
                MenuItem::tr("pptx.anim.opt.path-down").checked(current_x == 0 && current_y > 0).on(|e, _, cx| e.set_shape_effect_option(L::Motion(0, 25_000), cx)).into(),
                MenuItem::tr("pptx.anim.opt.path-left").checked(current_x < 0 && current_y == 0).on(|e, _, cx| e.set_shape_effect_option(L::Motion(-25_000, 0), cx)).into(),
                MenuItem::tr("pptx.anim.opt.path-right").checked(current_x > 0 && current_y == 0).on(|e, _, cx| e.set_shape_effect_option(L::Motion(25_000, 0), cx)).into(),
                MenuItem::tr("pptx.anim.opt.path-up").checked(current_x == 0 && current_y < 0).on(|e, _, cx| e.set_shape_effect_option(L::Motion(0, -25_000), cx)).into(),
            ]
        }
        Some(L::Preset) => {
            let a = first.expect("an effect");
            if is_filter_preset(a.class, a.preset.0) {
                ibis_doc_model::animation_preset::preset_def(a.preset.0)
                    .map(|def| {
                        def.options
                            .iter()
                            .map(|&(subtype, key, _)| {
                                MenuItem::tr(key).checked(a.preset.1 == subtype).on(move |e, _, cx| e.set_shape_preset_option(subtype, cx)).into()
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                // The options PowerPoint's own effect has (directions, colours, amounts), when it has more than one.
                ibis_pptx::animation_catalog::effect(a.class, a.preset.0)
                    .filter(|def| def.options.len() > 1)
                    .map(|def| {
                        def.options
                            .iter()
                            .map(|&(subtype, key)| MenuItem::tr(key).checked(a.preset.1 == subtype).on(move |e, _, cx| e.set_shape_preset_option(subtype, cx)).into())
                            .collect()
                    })
                    .unwrap_or_default()
            }
        }
        _ => Vec::new(),
    };
    let is_chart = editor.selected_shapes().iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Chart(_)));
    if is_chart && !mine.is_empty() {
        if !items.is_empty() {
            items.push(MenuEntry::Separator);
        }
        let current_chart_build = mine.iter().find_map(|a| a.chart_build).unwrap_or((ibis_doc_model::ChartBuildStep::AllAtOnce, true));
        let (chart_step, chart_anim_bg) = current_chart_build;
        items.push(MenuItem::tr("pptx.anim.chart.whole").checked(chart_step == ibis_doc_model::ChartBuildStep::AllAtOnce).on(move |e, _, cx| e.set_effect_chart_build(ibis_doc_model::ChartBuildStep::AllAtOnce, chart_anim_bg, cx)).into());
        items.push(MenuItem::tr("pptx.anim.chart.series").checked(chart_step == ibis_doc_model::ChartBuildStep::Series).on(move |e, _, cx| e.set_effect_chart_build(ibis_doc_model::ChartBuildStep::Series, chart_anim_bg, cx)).into());
        items.push(MenuItem::tr("pptx.anim.chart.category").checked(chart_step == ibis_doc_model::ChartBuildStep::Category).on(move |e, _, cx| e.set_effect_chart_build(ibis_doc_model::ChartBuildStep::Category, chart_anim_bg, cx)).into());
        items.push(MenuItem::tr("pptx.anim.chart.series-el").checked(chart_step == ibis_doc_model::ChartBuildStep::SeriesEl).on(move |e, _, cx| e.set_effect_chart_build(ibis_doc_model::ChartBuildStep::SeriesEl, chart_anim_bg, cx)).into());
        items.push(MenuItem::tr("pptx.anim.chart.category-el").checked(chart_step == ibis_doc_model::ChartBuildStep::CategoryEl).on(move |e, _, cx| e.set_effect_chart_build(ibis_doc_model::ChartBuildStep::CategoryEl, chart_anim_bg, cx)).into());
        if chart_step != ibis_doc_model::ChartBuildStep::AllAtOnce {
            items.push(MenuEntry::Separator);
            items.push(MenuItem::tr("pptx.anim.chart.anim-bg").checked(chart_anim_bg).on(move |e, _, cx| e.set_effect_chart_build(chart_step, !chart_anim_bg, cx)).into());
        }
    }
    let is_diagram = editor.selected_shapes().iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Placeholder(ibis_doc_model::Placeholder::Diagram)));
    if is_diagram && !mine.is_empty() {
        if !items.is_empty() {
            items.push(MenuEntry::Separator);
        }
        let current_dgm_build = mine.iter().find_map(|a| a.dgm_build).unwrap_or(ibis_doc_model::DiagramBuildStep::Whole);
        items.push(MenuItem::tr("pptx.anim.dgm.whole").checked(current_dgm_build == ibis_doc_model::DiagramBuildStep::Whole).on(move |e, _, cx| e.set_effect_diagram_build(ibis_doc_model::DiagramBuildStep::Whole, cx)).into());
        items.push(MenuItem::tr("pptx.anim.dgm.all").checked(current_dgm_build == ibis_doc_model::DiagramBuildStep::AllAtOnce).on(move |e, _, cx| e.set_effect_diagram_build(ibis_doc_model::DiagramBuildStep::AllAtOnce, cx)).into());
        items.push(MenuItem::tr("pptx.anim.dgm.one-by-one").checked(current_dgm_build == ibis_doc_model::DiagramBuildStep::DepthByNode).on(move |e, _, cx| e.set_effect_diagram_build(ibis_doc_model::DiagramBuildStep::DepthByNode, cx)).into());
        items.push(MenuItem::tr("pptx.anim.dgm.level-at-once").checked(current_dgm_build == ibis_doc_model::DiagramBuildStep::BreadthByLvl).on(move |e, _, cx| e.set_effect_diagram_build(ibis_doc_model::DiagramBuildStep::BreadthByLvl, cx)).into());
        items.push(MenuItem::tr("pptx.anim.dgm.level-one-by-one").checked(current_dgm_build == ibis_doc_model::DiagramBuildStep::DepthByBranch).on(move |e, _, cx| e.set_effect_diagram_build(ibis_doc_model::DiagramBuildStep::DepthByBranch, cx)).into());
    }
    if !mine.is_empty() {
        if !items.is_empty() {
            items.push(MenuEntry::Separator);
        }
        items.push(MenuItem::tr("pptx.anim.options-dialog").on(|e, _, cx| e.open_selected_animation_options_panel(cx)).into());
    }
    if items.is_empty() {
        // Nothing to set (no effect selected): the item is shown grey, as Office does.
        items.push(MenuItem::tr("pptx.anim.options-dialog").into());
    }
    items
}

/// The toolbar's アニメーション menu, for the selection.
pub(crate) fn animation_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let selected = editor.selected_ids();
    let mine: Vec<&Animation> = editor.slide().animations.iter().filter(|a| !a.is_media() && selected.contains(&a.shape)).collect();
    let has = !mine.is_empty();
    let options = animation_option_items(editor);
    let start = mine.first().map(|a| a.start);
    let duration = mine.iter().find(|a| a.look != L::Appear).map(|a| a.duration);
    let delay = mine.first().map(|a| a.delay);
    let effect_item = |id: &'static str, class: C, look: L| -> MenuEntry {
        let on = mine.len() == selected.len() && mine.iter().all(|a| a.class == class && a.look == look);
        MenuItem::tr(id).checked(on).on(move |e, _, cx| e.set_shape_effect(class, look, cx)).into()
    };
    let mut entrance = vec![
        effect_item("pptx.anim.in.appear", C::Entrance, L::Appear),
        effect_item("pptx.anim.in.fade", C::Entrance, L::Fade),
        effect_item("pptx.anim.in.fly-bottom", C::Entrance, L::Fly(AnimationSide::Bottom)),
        effect_item("pptx.anim.in.fly-left", C::Entrance, L::Fly(AnimationSide::Left)),
        effect_item("pptx.anim.in.fly-right", C::Entrance, L::Fly(AnimationSide::Right)),
        effect_item("pptx.anim.in.fly-top", C::Entrance, L::Fly(AnimationSide::Top)),
        effect_item("pptx.anim.in.float", C::Entrance, L::Float(AnimationSide::Top)),
        effect_item("pptx.anim.in.wipe", C::Entrance, L::Wipe(AnimationSide::Bottom)),
        effect_item("pptx.anim.in.zoom", C::Entrance, L::Zoom),
    ];
    let mut emphasis: Vec<MenuEntry> = vec![effect_item("pptx.anim.em.spin", C::Emphasis, L::Spin(21_600_000)), effect_item("pptx.anim.em.grow", C::Emphasis, L::Scale(150_000, 150_000))];
    emphasis.extend(more_effects(C::Emphasis, &mine, selected.len()));
    let mut exit = vec![
        effect_item("pptx.anim.out.disappear", C::Exit, L::Appear),
        effect_item("pptx.anim.out.fade", C::Exit, L::Fade),
        effect_item("pptx.anim.out.fly-bottom", C::Exit, L::Fly(AnimationSide::Bottom)),
        effect_item("pptx.anim.out.fly-left", C::Exit, L::Fly(AnimationSide::Left)),
        effect_item("pptx.anim.out.fly-right", C::Exit, L::Fly(AnimationSide::Right)),
        effect_item("pptx.anim.out.fly-top", C::Exit, L::Fly(AnimationSide::Top)),
        effect_item("pptx.anim.out.float", C::Exit, L::Float(AnimationSide::Bottom)),
        effect_item("pptx.anim.out.wipe", C::Exit, L::Wipe(AnimationSide::Bottom)),
        effect_item("pptx.anim.out.zoom", C::Exit, L::Zoom),
    ];
    entrance.extend(preset_items(C::Entrance, &mine, selected.len()));
    exit.extend(preset_items(C::Exit, &mine, selected.len()));
    entrance.extend(more_effects(C::Entrance, &mine, selected.len()));
    exit.extend(more_effects(C::Exit, &mine, selected.len()));
    let is_ink = editor.selected_shapes().iter().any(|s| crate::shell::ink::stroke_points(s).is_some());
    if is_ink {
        entrance.push(effect_item("pptx.anim.in.replay", C::Entrance, L::Replay));
        exit.push(effect_item("pptx.anim.out.rewind", C::Exit, L::Replay));
    }
    let path = vec![
        effect_item("pptx.anim.path.down", C::Path, L::Motion(0, 25_000)),
        effect_item("pptx.anim.path.left", C::Path, L::Motion(-25_000, 0)),
        effect_item("pptx.anim.path.right", C::Path, L::Motion(25_000, 0)),
        effect_item("pptx.anim.path.up", C::Path, L::Motion(0, -25_000)),
    ];
    let path = path.into_iter().chain(more_effects(C::Path, &mine, selected.len())).chain([MenuEntry::Separator, MenuItem::tr("pptx.anim.path.custom").disabled(!editor.can_draw_path()).on(|e, _, cx| e.begin_path_draw(cx)).into()]).collect::<Vec<_>>();
    let starts = [("pptx.anim.start.click", AnimationStart::OnClick), ("pptx.anim.start.with", AnimationStart::WithPrevious), ("pptx.anim.start.after", AnimationStart::AfterPrevious)]
        .into_iter()
        .map(|(id, s)| MenuItem::tr(id).disabled(!has).checked(start == Some(s)).on(move |e, _, cx| e.set_effect_start(s, cx)).into())
        .collect();
    let durations = [("pptx.anim.duration.500", 500), ("pptx.anim.duration.1000", 1000), ("pptx.anim.duration.2000", 2000)]
        .into_iter()
        .map(|(id, ms)| MenuItem::tr(id).disabled(!has).checked(duration == Some(ms)).on(move |e, _, cx| e.set_effect_duration(ms, cx)).into())
        .collect();
    let delays = [("pptx.anim.delay.0", 0), ("pptx.anim.delay.500", 500), ("pptx.anim.delay.1000", 1000), ("pptx.anim.delay.2000", 2000)]
        .into_iter()
        .map(|(id, ms)| MenuItem::tr(id).disabled(!has).checked(delay == Some(ms)).on(move |e, _, cx| e.set_effect_delay(ms, cx)).into())
        .collect();
    let repeat = mine.first().map(|a| a.repeat_count);
    let auto_rev = mine.first().map(|a| a.auto_reverse).unwrap_or(false);
    let rewind = mine.first().map(|a| a.rewind).unwrap_or(false);
    let repeats = [
        ("pptx.anim.repeat.none", AnimationRepeat::None),
        ("pptx.anim.repeat.2", AnimationRepeat::Count(2)),
        ("pptx.anim.repeat.3", AnimationRepeat::Count(3)),
        ("pptx.anim.repeat.4", AnimationRepeat::Count(4)),
        ("pptx.anim.repeat.5", AnimationRepeat::Count(5)),
        ("pptx.anim.repeat.10", AnimationRepeat::Count(10)),
        ("pptx.anim.repeat.until-click", AnimationRepeat::UntilNextClick),
        ("pptx.anim.repeat.until-end", AnimationRepeat::UntilEndOfSlide),
    ]
    .into_iter()
    .map(|(id, r)| MenuItem::tr(id).disabled(!has).checked(repeat == Some(r)).on(move |e, _, cx| e.set_effect_repeat(r, cx)).into())
    .collect();
    let accel = mine.first().map(|a| a.accel).unwrap_or(0);
    let decel = mine.first().map(|a| a.decel).unwrap_or(0);
    let smooths = [
        ("pptx.anim.smooth.none", 0, 0),
        ("pptx.anim.smooth.start", 50_000, 0),
        ("pptx.anim.smooth.end", 0, 50_000),
        ("pptx.anim.smooth.both", 50_000, 50_000),
    ]
    .into_iter()
    .map(|(id, ac, dc)| {
        MenuItem::tr(id)
            .disabled(!has)
            .checked(accel == ac && decel == dc)
            .on(move |e, _, cx| e.set_effect_smooth(ac, dc, cx))
            .into()
    })
    .collect();
    let by_paragraph = has && mine.iter().all(|a| a.paragraph.is_some());
    let builds = vec![
        MenuItem::tr("pptx.anim.build.whole").disabled(!has).checked(has && mine.iter().all(|a| a.paragraph.is_none() && a.text_unit.is_none())).on(|e, _, cx| e.set_effect_text_build(TextBuild::Whole, cx)).into(),
        MenuItem::tr("pptx.anim.build.paragraph")
            .disabled(!has || !editor.selected_shapes().iter().any(|s| s.text_body().is_some_and(|b| b.paragraphs.len() > 1)))
            .checked(by_paragraph)
            .on(|e, _, cx| e.set_effect_text_build(TextBuild::Paragraph, cx))
            .into(),
        MenuItem::tr("pptx.anim.build.word")
            .disabled(!has || !editor.selected_shapes().iter().any(|s| s.text_body().is_some()))
            .checked(has && mine.iter().all(|a| a.text_unit.is_some_and(|(u, _)| u == ibis_doc_model::TextUnit::Word)))
            .on(|e, _, cx| e.set_effect_text_build(TextBuild::Word, cx))
            .into(),
        MenuItem::tr("pptx.anim.build.letter")
            .disabled(!has || !editor.selected_shapes().iter().any(|s| s.text_body().is_some()))
            .checked(has && mine.iter().all(|a| a.text_unit.is_some_and(|(u, _)| u == ibis_doc_model::TextUnit::Letter)))
            .on(|e, _, cx| e.set_effect_text_build(TextBuild::Letter, cx))
            .into(),
    ];
    let is_chart = editor.selected_shapes().iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Chart(_)));
    let current_chart_build = mine.iter().find_map(|a| a.chart_build).unwrap_or((ibis_doc_model::ChartBuildStep::AllAtOnce, true));
    let (chart_step, chart_anim_bg) = current_chart_build;
    let chart_builds = vec![
        MenuItem::tr("pptx.anim.chart.whole")
            .disabled(!has || !is_chart)
            .checked(is_chart && chart_step == ibis_doc_model::ChartBuildStep::AllAtOnce)
            .on(move |e, _, cx| e.set_effect_chart_build(ibis_doc_model::ChartBuildStep::AllAtOnce, chart_anim_bg, cx))
            .into(),
        MenuItem::tr("pptx.anim.chart.series")
            .disabled(!has || !is_chart)
            .checked(is_chart && chart_step == ibis_doc_model::ChartBuildStep::Series)
            .on(move |e, _, cx| e.set_effect_chart_build(ibis_doc_model::ChartBuildStep::Series, chart_anim_bg, cx))
            .into(),
        MenuItem::tr("pptx.anim.chart.category")
            .disabled(!has || !is_chart)
            .checked(is_chart && chart_step == ibis_doc_model::ChartBuildStep::Category)
            .on(move |e, _, cx| e.set_effect_chart_build(ibis_doc_model::ChartBuildStep::Category, chart_anim_bg, cx))
            .into(),
        MenuItem::tr("pptx.anim.chart.series-el")
            .disabled(!has || !is_chart)
            .checked(is_chart && chart_step == ibis_doc_model::ChartBuildStep::SeriesEl)
            .on(move |e, _, cx| e.set_effect_chart_build(ibis_doc_model::ChartBuildStep::SeriesEl, chart_anim_bg, cx))
            .into(),
        MenuItem::tr("pptx.anim.chart.category-el")
            .disabled(!has || !is_chart)
            .checked(is_chart && chart_step == ibis_doc_model::ChartBuildStep::CategoryEl)
            .on(move |e, _, cx| e.set_effect_chart_build(ibis_doc_model::ChartBuildStep::CategoryEl, chart_anim_bg, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.anim.chart.anim-bg")
            .disabled(!has || !is_chart || chart_step == ibis_doc_model::ChartBuildStep::AllAtOnce)
            .checked(is_chart && chart_anim_bg)
            .on(move |e, _, cx| e.set_effect_chart_build(chart_step, !chart_anim_bg, cx))
            .into(),
    ];
    let is_diagram = editor.selected_shapes().iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Placeholder(ibis_doc_model::Placeholder::Diagram)));
    let current_dgm_build = mine.iter().find_map(|a| a.dgm_build).unwrap_or(ibis_doc_model::DiagramBuildStep::Whole);
    let dgm_builds = vec![
        MenuItem::tr("pptx.anim.dgm.whole")
            .disabled(!has || !is_diagram)
            .checked(is_diagram && current_dgm_build == ibis_doc_model::DiagramBuildStep::Whole)
            .on(move |e, _, cx| e.set_effect_diagram_build(ibis_doc_model::DiagramBuildStep::Whole, cx))
            .into(),
        MenuItem::tr("pptx.anim.dgm.all")
            .disabled(!has || !is_diagram)
            .checked(is_diagram && current_dgm_build == ibis_doc_model::DiagramBuildStep::AllAtOnce)
            .on(move |e, _, cx| e.set_effect_diagram_build(ibis_doc_model::DiagramBuildStep::AllAtOnce, cx))
            .into(),
        MenuItem::tr("pptx.anim.dgm.one-by-one")
            .disabled(!has || !is_diagram)
            .checked(is_diagram && current_dgm_build == ibis_doc_model::DiagramBuildStep::DepthByNode)
            .on(move |e, _, cx| e.set_effect_diagram_build(ibis_doc_model::DiagramBuildStep::DepthByNode, cx))
            .into(),
        MenuItem::tr("pptx.anim.dgm.level-at-once")
            .disabled(!has || !is_diagram)
            .checked(is_diagram && current_dgm_build == ibis_doc_model::DiagramBuildStep::BreadthByLvl)
            .on(move |e, _, cx| e.set_effect_diagram_build(ibis_doc_model::DiagramBuildStep::BreadthByLvl, cx))
            .into(),
        MenuItem::tr("pptx.anim.dgm.level-one-by-one")
            .disabled(!has || !is_diagram)
            .checked(is_diagram && current_dgm_build == ibis_doc_model::DiagramBuildStep::DepthByBranch)
            .on(move |e, _, cx| e.set_effect_diagram_build(ibis_doc_model::DiagramBuildStep::DepthByBranch, cx))
            .into(),
    ];
    let after_effect = mine.first().map(|a| a.after_effect);
    let after_effects = [
        ("pptx.anim.after.hold", AnimationAfterEffect::Hold),
        ("pptx.anim.after.hide", AnimationAfterEffect::Hide),
        ("pptx.anim.after.dim", AnimationAfterEffect::Dim),
        ("pptx.anim.after.hide-next", AnimationAfterEffect::HideOnNextClick),
    ]
    .into_iter()
    .map(|(id, af)| {
        MenuItem::tr(id)
            .disabled(!has)
            .checked(after_effect == Some(af))
            .on(move |e, _, cx| e.set_effect_after(af, cx))
            .into()
    })
    .collect();
    let add_item = |id: &'static str, class: C, look: L| -> MenuEntry { MenuItem::new(id, crate::shell::i18n::text(&id["add.".len()..])).on(move |e, _, cx| e.add_shape_effect(class, look, cx)).into() };
    // The four kinds in groups (the entrance and the exit effects have the same words: フェード, ワイプ, ズーム).
    let mut add_entrance = vec![
        add_item("add.pptx.anim.in.appear", C::Entrance, L::Appear),
        add_item("add.pptx.anim.in.fade", C::Entrance, L::Fade),
        add_item("add.pptx.anim.in.fly-bottom", C::Entrance, L::Fly(AnimationSide::Bottom)),
        add_item("add.pptx.anim.in.float", C::Entrance, L::Float(AnimationSide::Top)),
        add_item("add.pptx.anim.in.wipe", C::Entrance, L::Wipe(AnimationSide::Bottom)),
        add_item("add.pptx.anim.in.zoom", C::Entrance, L::Zoom),
    ];
    let add_emphasis = vec![
        add_item("add.pptx.anim.em.spin", C::Emphasis, L::Spin(21_600_000)),
        add_item("add.pptx.anim.em.grow", C::Emphasis, L::Scale(150_000, 150_000)),
    ];
    let mut add_exit = vec![
        add_item("add.pptx.anim.out.disappear", C::Exit, L::Appear),
        add_item("add.pptx.anim.out.fade", C::Exit, L::Fade),
        add_item("add.pptx.anim.out.fly-bottom", C::Exit, L::Fly(AnimationSide::Bottom)),
        add_item("add.pptx.anim.out.float", C::Exit, L::Float(AnimationSide::Bottom)),
        add_item("add.pptx.anim.out.wipe", C::Exit, L::Wipe(AnimationSide::Bottom)),
        add_item("add.pptx.anim.out.zoom", C::Exit, L::Zoom),
    ];
    let add_path = vec![
        add_item("add.pptx.anim.path.down", C::Path, L::Motion(0, 25_000)),
        add_item("add.pptx.anim.path.left", C::Path, L::Motion(-25_000, 0)),
        add_item("add.pptx.anim.path.right", C::Path, L::Motion(25_000, 0)),
        add_item("add.pptx.anim.path.up", C::Path, L::Motion(0, -25_000)),
    ];
    if is_ink {
        add_entrance.push(add_item("add.pptx.anim.in.replay", C::Entrance, L::Replay));
        add_exit.push(add_item("add.pptx.anim.out.rewind", C::Exit, L::Replay));
    }
    let mut add: Vec<MenuEntry> = Vec::new();
    for group in [add_entrance, add_emphasis, add_exit, add_path] {
        if !add.is_empty() {
            add.push(MenuEntry::Separator);
        }
        add.extend(group);
    }
    let media_menu: Option<MenuEntry> = (!selected.is_empty() && editor.selected_shapes().iter().all(|s| SlideEditor::clip_of(s).is_some())).then(|| {
        let items = [("pptx.anim.media.play", ibis_doc_model::MediaCommand::Play), ("pptx.anim.media.pause", ibis_doc_model::MediaCommand::Pause), ("pptx.anim.media.stop", ibis_doc_model::MediaCommand::Stop)]
            .into_iter()
            .map(|(id, c)| MenuItem::tr(id).on(move |e, _, cx| e.add_media_effect(c, cx)).into())
            .collect();
        Submenu::tr("pptx.anim.media.class", "play_circle", items).into()
    });
    let mut menu = vec![
        MenuItem::tr("pptx.anim.preview").disabled(editor.slide().animations.is_empty()).on(|e, w, cx| e.start_slideshow(w, cx)).into(),
        MenuEntry::Separator,
        Submenu::tr("pptx.anim.entrance", "animation", entrance).into(),
        Submenu::tr("pptx.anim.emphasis", "animation", emphasis).into(),
        Submenu::tr("pptx.anim.exit", "animation", exit).into(),
        Submenu::tr("pptx.anim.path", "conversion_path", path).into(),
        MenuEntry::Separator,
        Submenu::tr("pptx.anim.options", "tune", options).into(),
        Submenu::tr("pptx.anim.start", "schedule", starts).into(),
        Submenu::tr("pptx.anim.duration", "schedule", durations).into(),
        Submenu::tr("pptx.anim.delay", "schedule", delays).into(),
        Submenu::tr("pptx.anim.repeat", "repeat", repeats).into(),
        MenuItem::tr("pptx.anim.autoreverse").disabled(!has).checked(auto_rev).on(|e, _, cx| e.toggle_effect_autoreverse(cx)).into(),
        MenuItem::tr("pptx.anim.rewind").disabled(!has).checked(rewind).on(|e, _, cx| e.toggle_effect_rewind(cx)).into(),
        Submenu::tr("pptx.anim.smooth", "water", smooths).into(),
        Submenu::tr("pptx.anim.after", "visibility_off", after_effects).into(),
        Submenu::tr("pptx.anim.build", "format_list_bulleted", builds).into(),
        Submenu::tr("pptx.anim.chart-build", "bar_chart", chart_builds).into(),
        Submenu::tr("pptx.anim.dgm-build", "account_tree", dgm_builds).into(),
        Submenu::tr("pptx.anim.trigger", "touch_app", {
            let slide = editor.slide();
            let mut shape_click_items = Vec::new();
            for sh in &slide.shapes {
                let sh_id = sh.id;
                let label = sh.text_body()
                    .map(|b| b.text().chars().take(12).collect::<String>())
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| crate::shell::i18n::format("pptx.selection-pane.shape-n", &[("n", &slide.shape_index(sh_id).map_or(0, |x| x + 1))]).to_string());
                let is_checked = has && mine.iter().all(|a| matches!(a.trigger, AnimationTrigger::OnShapeClick(s) if s == sh_id));
                shape_click_items.push(
                    MenuItem::new("pptx.anim.trigger.shape-item", label)
                        .disabled(!has)
                        .checked(is_checked)
                        .on(move |e, _, cx| e.set_selected_effects_trigger(AnimationTrigger::OnShapeClick(sh_id), cx))
                        .into(),
                );
            }

            let mut bookmark_items = Vec::new();
            for sh in &slide.shapes {
                let sh_id = sh.id;
                if let Some(clip) = SlideEditor::clip_of(sh) {
                    for bmk in &clip.options.bookmarks {
                        let bmk_name = bmk.name.clone();
                        let label = format!("{}: {:.1}s", bmk_name, bmk.time_ms as f32 / 1000.0);
                        let bmk_check = bmk_name.clone();
                        let is_checked = has && mine.iter().all(|a| matches!(&a.trigger, AnimationTrigger::OnMediaBookmark { shape, bookmark } if *shape == sh_id && bookmark == &bmk_check));
                        let bmk_set = bmk_name.clone();
                        bookmark_items.push(
                            MenuItem::new("pptx.anim.trigger.bookmark-item", label)
                                .disabled(!has)
                                .checked(is_checked)
                                .on(move |e, _, cx| e.set_selected_effects_trigger(AnimationTrigger::OnMediaBookmark { shape: sh_id, bookmark: bmk_set.clone() }, cx))
                                .into(),
                        );
                    }
                }
            }

            let mut trigger_sub: Vec<MenuEntry> = Vec::new();
            if shape_click_items.is_empty() {
                trigger_sub.push(MenuItem::tr("pptx.anim.trigger.click").disabled(true).into());
            } else {
                trigger_sub.push(Submenu::tr("pptx.anim.trigger.click", "touch_app", shape_click_items).into());
            }
            if bookmark_items.is_empty() {
                trigger_sub.push(MenuItem::tr("pptx.anim.trigger.bookmark").disabled(true).into());
            } else {
                trigger_sub.push(Submenu::tr("pptx.anim.trigger.bookmark", "bookmark", bookmark_items).into());
            }
            trigger_sub.push(MenuEntry::Separator);
            let has_any_trigger = mine.iter().any(|a| !a.trigger.is_none());
            trigger_sub.push(
                MenuItem::tr("pptx.anim.trigger.none")
                    .disabled(!has || !has_any_trigger)
                    .on(|e, _, cx| e.clear_selected_effects_trigger(cx))
                    .into(),
            );
            trigger_sub
        }).into(),
        MenuItem::tr("pptx.anim.options-dialog").disabled(!has).on(|e, _, cx| e.open_selected_animation_options_panel(cx)).into(),
        MenuEntry::Separator,
        Submenu::tr("pptx.anim.add", "add", add).into(),
        MenuItem::tr("pptx.anim.remove").disabled(!has).on(|e, _, cx| e.remove_shape_effects(cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.anim.copy").shortcut("⌥⇧⌘C").disabled(!editor.can_copy_animations()).on(|e, _, cx| e.copy_shape_animations(cx)).into(),
        MenuItem::tr("pptx.anim.paste").shortcut("⌥⇧⌘V").disabled(!editor.can_paste_animations()).on(|e, _, cx| e.paste_shape_animations(cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.anim.order.earlier").disabled(!editor.can_move_selected_effect(true)).on(|e, _, cx| e.move_selected_effect(true, cx)).into(),
        MenuItem::tr("pptx.anim.order.later").disabled(!editor.can_move_selected_effect(false)).on(|e, _, cx| e.move_selected_effect(false, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.anim.pane").checked(editor.pptx().is_some_and(|s| s.animation_pane)).on(|e, _, cx| e.toggle_animation_pane(cx)).into(),
    ];
    // メディア (再生・一時停止・停止) after the paths: only for a video or an audio.
    if let Some(media) = media_menu
        && let Some(at) = menu.iter().position(|m| matches!(m, MenuEntry::Submenu(sub) if sub.id == "pptx.anim.path"))
    {
        menu.insert(at + 1, media);
    }
    menu
}

/// What names a text box's row in the pane: its shape, the first paragraph its effects take, and which of the rows of that shape
/// that begin there it is (`box_keys`). Another row of the same shape (its
/// other paragraphs, past an effect of another kind) is another key, and a key stays when the rows move.
pub(crate) type BoxKey = (ShapeId, u32, u32);

/// The flag of a text box's row in the id its drag carries (its first effect's number with this bit set): the row stands for all
/// its paragraphs' effects, and they move with it.
pub(crate) const GROUP_DRAG: u64 = 1 << 32;

/// The text boxes' rows of the list, from the front: the runs of effects (first, last) of one shape's paragraphs, none with a
/// trigger. An effect goes on the row before it when it is the same shape's paragraph that none of the row's effects covers: the
/// paragraphs may stand in any order (two children swapped stay under their text box), but a paragraph that the row has already
/// starts another row (the effects of paragraphs 0 and 1 twice are two rows, not one of four). An effect that is not one of them is
/// a row of its own.
fn text_box_runs(animations: &[Animation]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut at = 0;
    while at < animations.len() {
        let a = &animations[at];
        let mut last = at;
        if let (Some(first), true) = (a.paragraph, a.trigger.is_none()) {
            let mut used = vec![first];
            while let Some(n) = animations.get(last + 1) {
                let Some(q) = n.paragraph else { break };
                if n.shape != a.shape || !n.trigger.is_none() || used.iter().any(|u| q.0 <= u.1 && u.0 <= q.1) {
                    break;
                }
                used.push(q);
                last += 1;
            }
        }
        runs.push((at, last));
        at = last + 1;
    }
    runs
}

/// The effects of one text box's paragraphs that stand together in the list (two or more in a row, none with a trigger) around
/// effect `at`: the first and last of them, or `(at, at)` for an effect that is not one of them.
pub(crate) fn paragraph_run(animations: &[ibis_doc_model::Animation], at: usize) -> (usize, usize) {
    if animations.get(at).is_none_or(|a| a.paragraph.is_none() || !a.trigger.is_none()) {
        return (at, at);
    }
    text_box_runs(animations).into_iter().find(|&(f, l)| f <= at && at <= l).unwrap_or((at, at))
}

/// The key of the text box's row each effect is in (the same for the effects of one row; for the effects of no row, a key nothing
/// opens).
pub(crate) fn box_keys(animations: &[Animation]) -> Vec<BoxKey> {
    let mut keys = Vec::with_capacity(animations.len());
    let mut seen: Vec<(ShapeId, u32)> = Vec::new();
    let mut at = 0;
    while at < animations.len() {
        let (first, last) = paragraph_run(animations, at);
        // The key is the row's smallest paragraph, so that the paragraphs swapped keep the row's key (and its open or folded state).
        let start = (animations[first].shape, animations[first..=last].iter().filter_map(|a| a.paragraph.map(|p| p.0)).min().unwrap_or(0));
        // Two rows of one shape that begin at the same paragraph are told apart by their order in the list.
        let nth = if last > first { let n = seen.iter().filter(|k| **k == start).count() as u32; seen.push(start); n } else { 0 };
        keys.extend((first..=last).map(|_| (start.0, start.1, nth)));
        at = last + 1;
    }
    keys
}

/// The effects after a row of the pane (`from`: the id its drag carries) is dropped on the row of effect `target`, `None` when
/// the drop changes nothing. The same effects in another order: none is lost or made.
///
/// The effects of one text box's paragraphs stay together: another row dropped on one of them goes past all of them, a text box's
/// row takes all of them along, and a paragraph's effect stays among its own.
pub(crate) fn reordered_effects(animations: &[Animation], from: u64, target: usize, place: crate::shell::chrome::row_dnd::DropPlace) -> Option<Vec<Animation>> {
    use crate::shell::chrome::row_dnd::DropPlace;
    let group = from & GROUP_DRAG != 0;
    let from = (from & !GROUP_DRAG) as usize;
    if from >= animations.len() || target >= animations.len() || place == DropPlace::Into {
        return None;
    }
    let (from_first, from_last) = paragraph_run(animations, from);
    let (first, last) = paragraph_run(animations, target);
    // What moves (the effect, or all a text box's) and the place it goes to (before the effect `at` of the list as it is).
    let (moved, at) = if group {
        if (first, last) == (from_first, from_last) {
            return None;
        }
        ((from_first, from_last), if place == DropPlace::After { last + 1 } else { first })
    } else {
        if from_last > from_first && (first, last) != (from_first, from_last) {
            return None;
        }
        let target = if from_last == from_first && last > first { if place == DropPlace::After { last } else { first } } else { target };
        ((from, from), if place == DropPlace::After { target + 1 } else { target })
    };
    let mut all = animations.to_vec();
    let block: Vec<Animation> = all.drain(moved.0..=moved.1).collect();
    let at = if at > moved.1 { at - block.len() } else { at.min(moved.0) };
    all.splice(at..at, block);
    (all != animations).then_some(all)
}
