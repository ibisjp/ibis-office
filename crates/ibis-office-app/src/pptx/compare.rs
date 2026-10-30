// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! プレゼンテーションの比較と変更の統合 (PPTX-REVIEW-019 / Compare Presentations & Review Changes):
//! 別のプレゼンテーション ファイル（改訂版・比較対象）を読み込み、スライドの追加・削除、
//! 図形・テキスト・配置・スピーカーノートの変更差分を抽出してサイドバーのレビュー パネルに一覧表示。
//! 各変更の個別反映 (Accept) / 破棄 (Reject)、一括反映 (Accept All) / 一括破棄 (Reject All)、
//! レビュー終了 (End Review)、および Undo / Redo 連動を完全実装。

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, EmuRect, Presentation, Shape, ShapeId, ShapeKind, Slide, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ShapeDiff {
    Added { shape: Shape },
    Deleted { shape_id: ShapeId, name: String },
    ModifiedText { shape_id: ShapeId, name: String, old_text: String, new_text: String },
    ModifiedFrame { shape_id: ShapeId, name: String, old_frame: EmuRect, new_frame: EmuRect },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SlideDiff {
    AddedSlide { index: usize, slide: Slide },
    DeletedSlide { index: usize, slide_id: SlideId, title: String },
    ModifiedSlide {
        slide_id: SlideId,
        slide_index: usize,
        title: String,
        shape_diffs: Vec<ShapeDiff>,
        notes_diff: Option<(String, String)>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct ComparePanel {
    pub(crate) file_path: String,
    pub(crate) diffs: Vec<SlideDiff>,
    pub(crate) selected_diff: Option<usize>,
}

fn shape_kind_name(kind: &ShapeKind) -> SharedString {
    text(match kind {
        ShapeKind::TextBox(_) => "pptx.compare.kind.text-box",
        ShapeKind::Picture(_) => "pptx.compare.kind.picture",
        ShapeKind::Chart(_) => "pptx.compare.kind.chart",
        ShapeKind::Line(_) => "pptx.compare.kind.line",
        ShapeKind::Auto(_) => "pptx.compare.kind.shape",
        ShapeKind::Table(_) => "pptx.compare.kind.table",
        _ => "pptx.compare.kind.object",
    })
}

fn slide_default_title(n: usize) -> String {
    crate::shell::i18n::format("pptx.grid-view.slide-n", &[("n", &n)])
}

fn shape_label(shape: &Shape, slide: &Slide, index: usize) -> String {
    if let Some(name) = slide.shape_names.get(&shape.id) {
        return name.clone();
    }
    format!("{} {}", shape_kind_name(&shape.kind), index + 1)
}

/// Compare two presentations and produce a list of slide and shape differences.
pub(crate) fn compare_presentations(orig: &Presentation, rev: &Presentation) -> Vec<SlideDiff> {
    let mut diffs = Vec::new();
    let common_len = orig.slides.len().min(rev.slides.len());

    for i in 0..common_len {
        let o_slide = &orig.slides[i];
        let r_slide = &rev.slides[i];
        let o_title = o_slide.shapes.first().and_then(|s| s.text_body()).map(|b| b.text().trim().to_string()).unwrap_or_default();
        let title = if !o_title.is_empty() {
            o_title
        } else {
            slide_default_title(i + 1)
        };

        let mut shape_diffs = Vec::new();
        // Shapes are paired by what they are, not by their place in the stacking
        // order (PowerPoint's compare follows the object, so a shape inserted in
        // the middle does not make every shape after it look changed). Ids do not
        // help: a saved file renumbers them. In turn: the same text and frame,
        // the same text (moved), the same frame (retyped). Shapes left unpaired
        // are added or deleted.
        let text_of = |s: &Shape| s.text_body().map(|b| b.text().trim().to_string()).unwrap_or_default();
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut o_taken = vec![false; o_slide.shapes.len()];
        let mut r_taken = vec![false; r_slide.shapes.len()];
        type Same = fn(&Shape, &Shape, &dyn Fn(&Shape) -> String) -> bool;
        let passes: [Same; 3] = [
            |o, r, t| t(o) == t(r) && o.frame == r.frame,
            |o, r, t| t(o) == t(r),
            |o, r, _| o.frame == r.frame,
        ];
        for same in passes {
            for (j, o_shape) in o_slide.shapes.iter().enumerate() {
                if o_taken[j] {
                    continue;
                }
                if let Some(k) = (0..r_slide.shapes.len()).find(|&k| !r_taken[k] && same(o_shape, &r_slide.shapes[k], &text_of)) {
                    o_taken[j] = true;
                    r_taken[k] = true;
                    pairs.push((j, k));
                }
            }
        }
        pairs.sort_unstable();
        let o_left: Vec<usize> = (0..o_slide.shapes.len()).filter(|&j| !o_taken[j]).collect();
        let r_left: Vec<usize> = (0..r_slide.shapes.len()).filter(|&k| !r_taken[k]).collect();

        for &(j, k) in &pairs {
            let o_shape = &o_slide.shapes[j];
            let r_shape = &r_slide.shapes[k];
            let name = shape_label(o_shape, o_slide, j);

            let o_text = o_shape.text_body().map(|b| b.text().trim().to_string()).unwrap_or_default();
            let r_text = r_shape.text_body().map(|b| b.text().trim().to_string()).unwrap_or_default();
            if o_text != r_text {
                shape_diffs.push(ShapeDiff::ModifiedText {
                    shape_id: o_shape.id,
                    name: name.clone(),
                    old_text: o_text,
                    new_text: r_text,
                });
            }

            if o_shape.frame != r_shape.frame {
                shape_diffs.push(ShapeDiff::ModifiedFrame {
                    shape_id: o_shape.id,
                    name: name.clone(),
                    old_frame: o_shape.frame,
                    new_frame: r_shape.frame,
                });
            }
        }

        // Added shapes in revision
        for &k in &r_left {
            shape_diffs.push(ShapeDiff::Added {
                shape: r_slide.shapes[k].clone(),
            });
        }

        // Deleted shapes in revision (shapes existing in original but not revision)
        for &j in &o_left {
            let o_shape = &o_slide.shapes[j];
            let name = shape_label(o_shape, o_slide, j);
            shape_diffs.push(ShapeDiff::Deleted {
                shape_id: o_shape.id,
                name,
            });
        }

        // Notes differences
        let o_notes = o_slide.notes.trim();
        let r_notes = r_slide.notes.trim();
        let notes_diff = if o_notes != r_notes {
            Some((o_notes.to_string(), r_notes.to_string()))
        } else {
            None
        };

        if !shape_diffs.is_empty() || notes_diff.is_some() {
            diffs.push(SlideDiff::ModifiedSlide {
                slide_id: o_slide.id,
                slide_index: i,
                title,
                shape_diffs,
                notes_diff,
            });
        }
    }

    // Added slides in revision
    for i in common_len..rev.slides.len() {
        diffs.push(SlideDiff::AddedSlide {
            index: i,
            slide: rev.slides[i].clone(),
        });
    }

    // Deleted slides in revision (slides existing in original but not revision)
    for i in common_len..orig.slides.len() {
        let o_slide = &orig.slides[i];
        let title = o_slide.shapes.first().and_then(|s| s.text_body()).map(|b| b.text().trim().to_string()).unwrap_or_else(|| {
            slide_default_title(i + 1)
        });
        diffs.push(SlideDiff::DeletedSlide {
            index: i,
            slide_id: o_slide.id,
            title,
        });
    }

    diffs
}

impl SlideEditor {
    /// 校閲 > プレゼンテーションの比較: ファイル選択ダイアログを開いて比較を実行
    pub(crate) fn pick_compare_presentation_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                extensions: vec!["pptx".into()],
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_shell.open")),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update(cx, |this, cx| this.compare_presentation_with_file(&path, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    /// 比較対象のプレゼンテーション ファイルを読み込んで差分を解析
    pub(crate) fn compare_presentation_with_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let data = std::fs::read(path).ok();
        let rev = data.as_deref().and_then(|d| ibis_pptx::read(d).ok());
        let Some(rev_pres) = rev else {
            self.status = Some(text("pptx.compare.cannot-read").into());
            cx.notify();
            return;
        };
        let diffs = compare_presentations(self.presentation(), &rev_pres);
        let path_str = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string_lossy().to_string());
        if let Some(s) = self.pptx_mut() {
            s.compare_panel = Some(ComparePanel {
                file_path: path_str,
                diffs,
                selected_diff: None,
            });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 比較パネルが開いているか
    pub(crate) fn is_comparing_presentations(&self) -> bool {
        self.pptx().and_then(|s| s.compare_panel.as_ref()).is_some()
    }

    /// 変更の反映 (Accept Change)
    pub(crate) fn compare_accept_diff(&mut self, diff_idx: usize, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.compare_panel.as_ref()) else { return };
        let Some(diff) = panel.diffs.get(diff_idx).cloned() else { return };
        self.apply_slide_diff(&diff, cx);
        if let Some(panel) = self.pptx_mut().and_then(|s| s.compare_panel.as_mut()) {
            if diff_idx < panel.diffs.len() {
                panel.diffs.remove(diff_idx);
            }
        }
        cx.notify();
    }

    /// 変更の破棄 (Reject Change)
    pub(crate) fn compare_reject_diff(&mut self, diff_idx: usize, cx: &mut Context<Self>) {
        if let Some(panel) = self.pptx_mut().and_then(|s| s.compare_panel.as_mut()) {
            if diff_idx < panel.diffs.len() {
                panel.diffs.remove(diff_idx);
            }
        }
        cx.notify();
    }

    /// 現在選択中の変更を反映
    pub(crate) fn compare_accept_current(&mut self, cx: &mut Context<Self>) {
        let idx = self.pptx().and_then(|p| p.compare_panel.as_ref()).and_then(|p| p.selected_diff).unwrap_or(0);
        self.compare_accept_diff(idx, cx);
    }

    /// 現在選択中の変更を破棄
    pub(crate) fn compare_reject_current(&mut self, cx: &mut Context<Self>) {
        let idx = self.pptx().and_then(|p| p.compare_panel.as_ref()).and_then(|p| p.selected_diff).unwrap_or(0);
        self.compare_reject_diff(idx, cx);
    }

    /// すべての変更を反映 (Accept All Changes)
    pub(crate) fn compare_accept_all(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.compare_panel.as_ref()) else { return };
        let diffs = panel.diffs.clone();
        for diff in &diffs {
            self.apply_slide_diff(diff, cx);
        }
        if let Some(panel) = self.pptx_mut().and_then(|s| s.compare_panel.as_mut()) {
            panel.diffs.clear();
        }
        cx.notify();
    }

    /// すべての変更を破棄 (Reject All Changes)
    pub(crate) fn compare_reject_all(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.pptx_mut().and_then(|s| s.compare_panel.as_mut()) {
            panel.diffs.clear();
        }
        cx.notify();
    }

    /// レビューの終了 (End Review)
    pub(crate) fn compare_end_review(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.compare_panel = None;
        }
        cx.notify();
    }

    /// 差分項目を選択し、該当スライド・図形へフォーカスを移動
    pub(crate) fn compare_select_diff(&mut self, diff_idx: usize, cx: &mut Context<Self>) {
        let diff = {
            let Some(panel) = self.pptx_mut().and_then(|s| s.compare_panel.as_mut()) else { return };
            if diff_idx >= panel.diffs.len() {
                return;
            }
            panel.selected_diff = Some(diff_idx);
            panel.diffs[diff_idx].clone()
        };

        match diff {
            SlideDiff::AddedSlide { index, .. } => {
                if let Some(s) = self.presentation().slides.get(index.saturating_sub(1)).or_else(|| self.presentation().slides.last()) {
                    self.current_slide = s.id;
                }
            }
            SlideDiff::DeletedSlide { slide_id, .. } => {
                if self.presentation().slides.iter().any(|s| s.id == slide_id) {
                    self.current_slide = slide_id;
                }
            }
            SlideDiff::ModifiedSlide { slide_id, shape_diffs, .. } => {
                if self.presentation().slides.iter().any(|s| s.id == slide_id) {
                    self.current_slide = slide_id;
                }
                let focus_shape = shape_diffs.iter().find_map(|sd| match sd {
                    ShapeDiff::ModifiedText { shape_id, .. }
                    | ShapeDiff::ModifiedFrame { shape_id, .. }
                    | ShapeDiff::Deleted { shape_id, .. } => Some(*shape_id),
                    ShapeDiff::Added { .. } => None,
                });
                self.selection = focus_shape;
                if let Some(sid) = focus_shape {
                    if let Some(s) = self.pptx_mut() {
                        s.selection = vec![sid];
                    }
                } else if let Some(s) = self.pptx_mut() {
                    s.selection.clear();
                }
            }
        }
        cx.notify();
    }

    /// 比較差分カードの右クリック コンテキストメニュー項目 (PPTX-REVIEW-019, PPTX-AUDIT-003)
    pub(crate) fn compare_diff_context_menu_entries(&self, diff_idx: usize) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let Some(panel) = self.pptx().and_then(|s| s.compare_panel.as_ref()) else { return Vec::new() };
        let has_diffs = !panel.diffs.is_empty();
        let valid_idx = diff_idx < panel.diffs.len();

        vec![
            MenuItem::tr("_shell.view")
                .icon("visibility")
                .disabled(!valid_idx)
                .on(move |e, _, cx| e.compare_select_diff(diff_idx, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.compare.accept")
                .icon("check")
                .disabled(!valid_idx)
                .on(move |e, _, cx| e.compare_accept_diff(diff_idx, cx))
                .into(),
            MenuItem::tr("pptx.compare.reject")
                .icon("close")
                .disabled(!valid_idx)
                .on(move |e, _, cx| e.compare_reject_diff(diff_idx, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.review.accept-all")
                .icon("checklist")
                .disabled(!has_diffs)
                .on(|e, _, cx| e.compare_accept_all(cx))
                .into(),
            MenuItem::tr("pptx.review.reject-all")
                .icon("delete")
                .disabled(!has_diffs)
                .on(|e, _, cx| e.compare_reject_all(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.review.end-review")
                .icon("close")
                .on(|e, _, cx| e.compare_end_review(cx))
                .into(),
        ]
    }

    /// 比較差分カード上の右クリック メニューを開く
    pub(crate) fn open_compare_diff_context_menu(&mut self, diff_idx: usize, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.compare_select_diff(diff_idx, cx);
        let entries = self.compare_diff_context_menu_entries(diff_idx);
        self.open_popup_entries(entries, position, window, cx);
    }

    fn apply_slide_diff(&mut self, diff: &SlideDiff, cx: &mut Context<Self>) {
        match diff {
            SlideDiff::AddedSlide { index, slide } => {
                let at = (*index).min(self.presentation().slides.len());
                self.run(vec![Command::InsertSlide { index: at, slide: slide.clone() }], cx);
            }
            SlideDiff::DeletedSlide { slide_id, .. } => {
                if self.presentation().slides.iter().any(|s| s.id == *slide_id) {
                    self.run(vec![Command::RemoveSlide { slide: *slide_id }], cx);
                }
            }
            SlideDiff::ModifiedSlide { slide_id, shape_diffs, notes_diff, .. } => {
                let mut cmds = Vec::new();
                for sd in shape_diffs {
                    match sd {
                        ShapeDiff::Added { shape } => {
                            let at = self.presentation().slide(*slide_id).map(|s| s.shapes.len()).unwrap_or(0);
                            cmds.push(Command::InsertShape { slide: *slide_id, index: at, shape: shape.clone() });
                        }
                        ShapeDiff::Deleted { shape_id, .. } => {
                            cmds.push(Command::RemoveShape { slide: *slide_id, shape: *shape_id });
                        }
                        ShapeDiff::ModifiedText { shape_id, new_text, .. } => {
                            cmds.push(Command::SetText { slide: *slide_id, shape: *shape_id, text: new_text.clone() });
                        }
                        ShapeDiff::ModifiedFrame { shape_id, new_frame, .. } => {
                            cmds.push(Command::SetFrame { slide: *slide_id, shape: *shape_id, frame: *new_frame });
                        }
                    }
                }
                if let Some((_, new_notes)) = notes_diff {
                    cmds.push(Command::SetNotes { slide: *slide_id, notes: new_notes.clone() });
                }
                if !cmds.is_empty() {
                    self.run(cmds, cx);
                }
            }
        }
    }

    /// 比較サイドバー パネルの描画
    pub(crate) fn compare_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.pptx()?.compare_panel.as_ref()?;
        let file_path = panel.file_path.clone();
        let diffs = panel.diffs.clone();
        let selected_diff = panel.selected_diff;

        let title = text("pptx.compare.title").to_string();

        let mut body = v_flex().gap_2().px(px(14.)).py(px(10.)).p_0();

        // ヘッダー情報
        let header = v_flex().gap_1()
            .child(div().text_sm().font_weight(FontWeight::BOLD).child(format!("📄 {}", file_path)))
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child(
                if diffs.is_empty() {
                    text("pptx.compare.no-changes").to_string()
                } else {
                    crate::shell::i18n::format("pptx.compare.title-count", &[("title", &text("pptx.compare.title")), ("n", &diffs.len())])
                }
            ));
        body = body.child(header);

        if !diffs.is_empty() {
            // 一括操作ボタン
            let top_actions = h_flex().gap_1().flex_wrap()
                .child(crate::shell::chrome::primary_button("cmp-accept-all").small().label(text("pptx.review.accept-all")).on_click(cx.listener(|this, _, _, cx| this.compare_accept_all(cx))))
                .child(crate::shell::chrome::action_button("cmp-reject-all").small().label(text("pptx.review.reject-all")).on_click(cx.listener(|this, _, _, cx| this.compare_reject_all(cx))));
            body = body.child(top_actions);

            // 差分リスト
            let mut list_box = v_flex().gap_2().pt_1();
            for (idx, diff) in diffs.iter().enumerate() {
                let is_selected = selected_diff == Some(idx);
                let mut card = v_flex()
                    .id(("compare-diff-card", idx))
                    .p_2()
                    .gap_1()
                    .rounded_md()
                    .border_1()
                    .border_color(if is_selected { cx.theme().primary } else { rgb(0xdadce0).into() })
                    .bg(if is_selected { cx.theme().primary.opacity(0.05) } else { rgb(0xffffff).into() })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.compare_select_diff(idx, cx);
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_compare_diff_context_menu(idx, event.position, window, cx);
                        }),
                    );

                match diff {
                    SlideDiff::AddedSlide { index, slide } => {
                        let slide_title = slide.shapes.first().and_then(|s| s.text_body()).map(|b| b.text().trim().to_string()).unwrap_or_else(|| slide_default_title(index + 1));
                        card = card.child(div().font_weight(FontWeight::SEMIBOLD).text_sm().text_color(rgb(0x137333)).child(crate::shell::i18n::format("pptx.compare.slide-added", &[("n", &(index + 1)), ("title", &slide_title)])));
                    }
                    SlideDiff::DeletedSlide { index, title, .. } => {
                        card = card.child(div().font_weight(FontWeight::SEMIBOLD).text_sm().text_color(rgb(0xc5221f)).child(crate::shell::i18n::format("pptx.compare.slide-deleted", &[("n", &(index + 1)), ("title", title)])));
                    }
                    SlideDiff::ModifiedSlide { slide_index, title, shape_diffs, notes_diff, .. } => {
                        card = card.child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child(crate::shell::i18n::format("pptx.compare.slide-modified", &[("n", &(slide_index + 1)), ("title", title)])));
                        for sd in shape_diffs {
                            let detail = match sd {
                                ShapeDiff::Added { shape } => crate::shell::i18n::format("pptx.compare.shape-added", &[("kind", &shape_kind_name(&shape.kind))]),
                                ShapeDiff::Deleted { name, .. } => crate::shell::i18n::format("pptx.compare.shape-deleted", &[("name", name)]),
                                ShapeDiff::ModifiedText { name, old_text, new_text, .. } => {
                                    let old_short = if old_text.chars().count() > 15 { format!("{}…", old_text.chars().take(15).collect::<String>()) } else { old_text.clone() };
                                    let new_short = if new_text.chars().count() > 15 { format!("{}…", new_text.chars().take(15).collect::<String>()) } else { new_text.clone() };
                                    crate::shell::i18n::format("pptx.compare.text-changed", &[("name", name), ("old", &old_short), ("new", &new_short)])
                                }
                                ShapeDiff::ModifiedFrame { name, .. } => crate::shell::i18n::format("pptx.compare.frame-changed", &[("name", name)]),
                            };
                            card = card.child(div().text_sm().text_color(rgb(0x3c4043)).child(detail));
                        }
                        if let Some((old_n, new_n)) = notes_diff {
                            let old_short = if old_n.chars().count() > 15 { format!("{}…", old_n.chars().take(15).collect::<String>()) } else { old_n.clone() };
                            let new_short = if new_n.chars().count() > 15 { format!("{}…", new_n.chars().take(15).collect::<String>()) } else { new_n.clone() };
                            card = card.child(div().text_sm().text_color(rgb(0x188038)).child(crate::shell::i18n::format("pptx.compare.notes-changed", &[("old", &old_short), ("new", &new_short)])));
                        }
                    }
                }

                // 反映・破棄ボタン
                let actions = crate::shell::chrome::action_bar().justify_end()
                    .action(crate::shell::chrome::ActionDefinition::new(("cmp-acc", idx), text("pptx.compare.accept"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(move |this, _, _, cx| {
                        this.compare_accept_diff(idx, cx);
                    })))
                    .action(crate::shell::chrome::ActionDefinition::new(("cmp-rej", idx), text("pptx.compare.reject"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(move |this, _, _, cx| {
                        this.compare_reject_diff(idx, cx);
                    })));
                card = card.child(actions);
                list_box = list_box.child(card);
            }
            body = body.child(list_box);
        }

        // レビュー終了ボタン

        let panel_terminal_actions = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("cmp-close", text("pptx.review.end-review"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|this, _, _, cx| this.compare_end_review(cx))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(title, body.into_any_element(), panel_terminal_actions))
    }
}
