// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ドキュメントの検査 (Inspect Presentation: PPTX-REVIEW-018):
//! プレゼンテーション内の個人情報、ドキュメントのプロパティ、コメント、
//! 発表者ノート、非表示スライド、スライド外コンテンツを検査し、一括削除する。

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::Command;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// プレゼンテーションの検査結果
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DocumentInspection {
    /// ドキュメントのプロパティと個人情報の有無
    pub(crate) has_properties: bool,
    /// 検出されたプロパティの詳細説明リスト
    pub(crate) properties_details: Vec<String>,
    /// コメント総件数
    pub(crate) comment_count: usize,
    /// 発表者ノートのあるスライド数
    pub(crate) notes_count: usize,
    /// 非表示スライドの枚数
    pub(crate) hidden_slide_count: usize,
    /// スライド領域外の図形数
    pub(crate) off_slide_shape_count: usize,
}

/// プレゼンテーションをスキャンして個人情報・非表示要素を検査する
pub(crate) fn inspect_presentation(p: &ibis_doc_model::Presentation) -> DocumentInspection {
    let mut details = Vec::new();
    let props = &p.properties;

    if let Some(t) = &props.title {
        if !t.trim().is_empty() { details.push(crate::shell::i18n::format("_pptx.title-value", &[("value", &t.trim())])); }
    }
    if let Some(c) = &props.creator {
        if !c.trim().is_empty() { details.push(crate::shell::i18n::format("_pptx.author-value", &[("value", &c.trim())])); }
    }
    if let Some(s) = &props.subject {
        if !s.trim().is_empty() { details.push(crate::shell::i18n::format("_pptx.subject-value", &[("value", &s.trim())])); }
    }
    if let Some(k) = &props.keywords {
        if !k.trim().is_empty() { details.push(crate::shell::i18n::format("_pptx.keywords-value", &[("value", &k.trim())])); }
    }
    if let Some(d) = &props.description {
        if !d.trim().is_empty() { details.push(crate::shell::i18n::format("_pptx.comments-value", &[("value", &d.trim())])); }
    }
    if let Some(cat) = &props.category {
        if !cat.trim().is_empty() { details.push(crate::shell::i18n::format("_pptx.category-value", &[("value", &cat.trim())])); }
    }
    if let Some(m) = &props.manager {
        if !m.trim().is_empty() { details.push(crate::shell::i18n::format("_pptx.manager-value", &[("value", &m.trim())])); }
    }
    if let Some(co) = &props.company {
        if !co.trim().is_empty() { details.push(crate::shell::i18n::format("_pptx.company-value", &[("value", &co.trim())])); }
    }
    if let Some(lm) = &props.last_modified_by {
        if !lm.trim().is_empty() { details.push(crate::shell::i18n::format("_pptx.last-modified-by-value", &[("value", &lm.trim())])); }
    }
    if !props.custom.is_empty() {
        details.push(crate::shell::i18n::format("_pptx.custom-properties-count", &[("count", &props.custom.len())]));
    }

    let has_properties = !details.is_empty();
    let comment_count: usize = p.slides.iter().map(|s| s.comments.len()).sum();
    let notes_count: usize = p.slides.iter().filter(|s| !s.notes.trim().is_empty()).count();
    let hidden_slide_count: usize = p.slides.iter().filter(|s| s.hidden).count();

    let mut off_slide_shape_count = 0;
    for slide in &p.slides {
        let size = p.size_of(slide);
        for shape in &slide.shapes {
            let f = shape.frame;
            if f.x + f.width <= 0 || f.y + f.height <= 0 || f.x >= size.width || f.y >= size.height {
                off_slide_shape_count += 1;
            }
        }
    }

    DocumentInspection {
        has_properties,
        properties_details: details,
        comment_count,
        notes_count,
        hidden_slide_count,
        off_slide_shape_count,
    }
}

/// サイドバーに表示するドキュメント検査パネル
pub(crate) struct PresentationInspectPanel {
    pub(crate) inspection: DocumentInspection,
    pub(crate) properties_removed: bool,
    pub(crate) comments_removed: bool,
    pub(crate) notes_removed: bool,
    pub(crate) hidden_slides_removed: bool,
    pub(crate) off_slide_removed: bool,
    pub(crate) message: String,
}

impl SlideEditor {
    /// ドキュメント検査パネルを開く
    pub(crate) fn open_presentation_inspect_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let inspection = inspect_presentation(self.presentation());
        let panel = PresentationInspectPanel {
            inspection,
            properties_removed: false,
            comments_removed: false,
            notes_removed: false,
            hidden_slides_removed: false,
            off_slide_removed: false,
            message: String::new(),
        };

        if let Some(s) = self.pptx_mut() {
            s.inspect_panel = Some(panel);
            s.properties_panel = None;
            s.alt_text_panel = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// ドキュメント検査パネルを閉じる
    pub(crate) fn close_presentation_inspect_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.inspect_panel = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// 検査を再実行
    pub(crate) fn recheck_presentation_inspection(&mut self, cx: &mut Context<Self>) {
        let inspection = inspect_presentation(self.presentation());
        if let Some(p) = self.pptx_mut().and_then(|s| s.inspect_panel.as_mut()) {
            p.inspection = inspection;
            p.message = text("pptx.inspect-panel.reinspection-complete").to_string();
            cx.notify();
        }
    }

    /// ドキュメントのプロパティと個人情報をすべて削除
    pub(crate) fn inspect_remove_all_properties(&mut self, cx: &mut Context<Self>) {
        let empty_props = ibis_doc_model::flow::DocProperties::default();
        self.run(vec![Command::SetDocProperties { properties: empty_props }], cx);
        let inspection = inspect_presentation(self.presentation());
        if let Some(p) = self.pptx_mut().and_then(|s| s.inspect_panel.as_mut()) {
            p.properties_removed = true;
            p.inspection = inspection;
            p.message = text("pptx.inspect-panel.removed-document-properties").to_string();
            cx.notify();
        }
    }

    /// コメントをすべて削除
    pub(crate) fn inspect_remove_all_comments(&mut self, cx: &mut Context<Self>) {
        let mut commands = Vec::new();
        for slide in &self.presentation().slides {
            if !slide.comments.is_empty() {
                commands.push(Command::SetComments { slide: slide.id, comments: Vec::new() });
            }
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
        let inspection = inspect_presentation(self.presentation());
        if let Some(p) = self.pptx_mut().and_then(|s| s.inspect_panel.as_mut()) {
            p.comments_removed = true;
            p.inspection = inspection;
            p.message = text("pptx.inspect-panel.removed-all-comments").to_string();
            cx.notify();
        }
    }

    /// プレゼンテーション ノートをすべて削除
    pub(crate) fn inspect_remove_all_notes(&mut self, cx: &mut Context<Self>) {
        let mut commands = Vec::new();
        for slide in &self.presentation().slides {
            if !slide.notes.trim().is_empty() {
                commands.push(Command::SetNotes { slide: slide.id, notes: String::new() });
            }
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
        let inspection = inspect_presentation(self.presentation());
        if let Some(p) = self.pptx_mut().and_then(|s| s.inspect_panel.as_mut()) {
            p.notes_removed = true;
            p.inspection = inspection;
            p.message = text("pptx.inspect-panel.removed-all-presentation-notes").to_string();
            cx.notify();
        }
    }

    /// 非表示スライドをすべて削除
    pub(crate) fn inspect_remove_all_hidden_slides(&mut self, cx: &mut Context<Self>) {
        let hidden_ids: Vec<_> = self.presentation().slides.iter().filter(|s| s.hidden).map(|s| s.id).collect();
        let total_slides = self.presentation().slides.len();
        if hidden_ids.len() < total_slides {
            let commands: Vec<_> = hidden_ids.into_iter().map(|slide| Command::RemoveSlide { slide }).collect();
            self.run(commands, cx);
        } else {
            // 全スライドが非表示の場合は非表示フラグを解除
            let mut commands = Vec::new();
            for slide in &self.presentation().slides {
                let mut unhidden = slide.clone();
                unhidden.hidden = false;
                commands.push(Command::SetSlide { slide: unhidden });
            }
            self.run(commands, cx);
        }
        let inspection = inspect_presentation(self.presentation());
        if let Some(p) = self.pptx_mut().and_then(|s| s.inspect_panel.as_mut()) {
            p.hidden_slides_removed = true;
            p.inspection = inspection;
            p.message = text("pptx.inspect-panel.removed-hidden-slides").to_string();
            cx.notify();
        }
    }

    /// スライド領域外の図形をすべて削除
    pub(crate) fn inspect_remove_all_off_slide_shapes(&mut self, cx: &mut Context<Self>) {
        let p = self.presentation();
        let mut commands = Vec::new();
        for slide in &p.slides {
            let size = p.size_of(slide);
            for shape in &slide.shapes {
                let f = shape.frame;
                if f.x + f.width <= 0 || f.y + f.height <= 0 || f.x >= size.width || f.y >= size.height {
                    commands.push(Command::RemoveShape { slide: slide.id, shape: shape.id });
                }
            }
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
        let inspection = inspect_presentation(self.presentation());
        if let Some(panel) = self.pptx_mut().and_then(|s| s.inspect_panel.as_mut()) {
            panel.off_slide_removed = true;
            panel.inspection = inspection;
            panel.message = text("pptx.inspect-panel.removed-off-slide-content").to_string();
            cx.notify();
        }
    }

    /// Context menu entries for an inspection category card (`0..5`: Properties, Comments, Notes, HiddenSlides, OffSlideShapes).
    pub(crate) fn inspect_category_context_menu_entries(&self, category_idx: usize) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let Some(panel) = self.pptx().and_then(|s| s.inspect_panel.as_ref()) else {
            return Vec::new();
        };
        let insp = &panel.inspection;
        let mut entries: Vec<MenuEntry> = match category_idx {
            0 => {
                let can_remove = insp.has_properties && !panel.properties_removed;
                vec![
                    MenuItem::tr("pptx.inspect.remove-all")
                        .icon("delete")
                        .disabled(!can_remove)
                        .on(|e, _, cx| e.inspect_remove_all_properties(cx))
                        .into(),
                    MenuItem::tr("file.info")
                        .icon("info")
                        .on(|e, w, cx| e.open_presentation_properties_panel(w, cx))
                        .into(),
                ]
            }
            1 => {
                let can_remove = insp.comment_count > 0 && !panel.comments_removed;
                vec![
                    MenuItem::tr("pptx.inspect.remove-all")
                        .icon("delete")
                        .disabled(!can_remove)
                        .on(|e, _, cx| e.inspect_remove_all_comments(cx))
                        .into(),
                    MenuItem::tr("insert.comment")
                        .icon("comment")
                        .on(|e, w, cx| e.open_slide_comments(w, cx))
                        .into(),
                ]
            }
            2 => {
                let can_remove = insp.notes_count > 0 && !panel.notes_removed;
                vec![
                    MenuItem::tr("pptx.inspect.remove-all")
                        .icon("delete")
                        .disabled(!can_remove)
                        .on(|e, _, cx| e.inspect_remove_all_notes(cx))
                        .into(),
                    MenuItem::tr("pptx.notes-page-view")
                        .icon("description")
                        .checked(self.is_notes_page_view())
                        .on(|e, w, cx| e.toggle_notes_page_view(w, cx))
                        .into(),
                ]
            }
            3 => {
                let can_remove = insp.hidden_slide_count > 0 && !panel.hidden_slides_removed;
                vec![
                    MenuItem::tr("pptx.inspect.remove-all")
                        .icon("delete")
                        .disabled(!can_remove)
                        .on(|e, _, cx| e.inspect_remove_all_hidden_slides(cx))
                        .into(),
                    MenuItem::tr("pptx.grid-view")
                        .icon("grid_view")
                        .checked(self.is_grid_view())
                        .on(|e, w, cx| e.toggle_grid_view(w, cx))
                        .into(),
                ]
            }
            4 => {
                let can_remove = insp.off_slide_shape_count > 0 && !panel.off_slide_removed;
                vec![
                    MenuItem::tr("pptx.inspect.remove-all")
                        .icon("delete")
                        .disabled(!can_remove)
                        .on(|e, _, cx| e.inspect_remove_all_off_slide_shapes(cx))
                        .into(),
                    MenuItem::tr("pptx.selection-pane")
                        .icon("layers")
                        .on(|e, _, cx| e.toggle_selection_pane(cx))
                        .into(),
                ]
            }
            _ => return Vec::new(),
        };
        entries.extend([
            MenuEntry::Separator,
            MenuItem::tr("pptx.inspect-panel.reinspect")
                .icon("refresh")
                .on(|e, _, cx| e.recheck_presentation_inspection(cx))
                .into(),
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|e, _, cx| e.close_presentation_inspect_panel(cx))
                .into(),
        ]);
        entries
    }

    /// Open context menu for an inspection category card at pointer `position`.
    pub(crate) fn open_inspect_category_context_menu(
        &mut self,
        category_idx: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.inspect_category_context_menu_entries(category_idx);
        if !entries.is_empty() {
            self.open_popup_entries(entries, position, window, cx);
        }
    }

    /// サイドバーのドキュメント検査パネル UI 要素
    pub(crate) fn presentation_inspect_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.pptx()?.inspect_panel.as_ref()?;
        let insp = panel.inspection.clone();
        let props_removed = panel.properties_removed;
        let comments_removed = panel.comments_removed;
        let notes_removed = panel.notes_removed;
        let hidden_removed = panel.hidden_slides_removed;
        let off_slide_removed = panel.off_slide_removed;
        let msg = panel.message.clone();

        let header = v_flex()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(text("pptx.inspect-panel.inspect-presentation-personal").to_string()),
            );

        // 1. プロパティと個人情報
        let prop_item = {
            let found = insp.has_properties;
            let status_text = if props_removed {
                text("pptx.inspect-panel.removed-document-properties").to_string()
            } else if found {
                crate::shell::i18n::format("_pptx.found-details", &[("details", &insp.properties_details.join(", "))])
            } else {
                text("pptx.inspect-panel.no-document-properties-personal").to_string()
            };

            let remove_btn = (found && !props_removed).then(|| {
                crate::shell::chrome::destructive_button("inspect-remove-props",cx)
                    .label(text("pptx.inspect.remove-all"))
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.inspect_remove_all_properties(cx)))
            });

            v_flex()
                .id(("inspect-card", 0usize))
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if found && !props_removed { cx.theme().danger.opacity(0.3) } else { cx.theme().border })
                .bg(if found && !props_removed { cx.theme().danger.opacity(0.05) } else { cx.theme().background })
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, event: &MouseDownEvent, w, cx| {
                        cx.stop_propagation();
                        this.open_inspect_category_context_menu(0, event.position, w, cx);
                    }),
                )
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_sm()
                                .child(text("pptx.inspect-panel.document-properties-personal").to_string()),
                        )
                        .children(remove_btn),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(if found && !props_removed { cx.theme().danger } else { cx.theme().muted_foreground })
                        .child(status_text),
                )
        };

        // 2. コメント
        let comment_item = {
            let found = insp.comment_count > 0;
            let status_text = if comments_removed {
                text("pptx.inspect-panel.removed-all-comments").to_string()
            } else if found {
                crate::shell::i18n::format("_pptx.count-comments-found", &[("count", &insp.comment_count)])
            } else {
                text("pptx.inspect-panel.no-comments-were-found").to_string()
            };

            let remove_btn = (found && !comments_removed).then(|| {
                crate::shell::chrome::destructive_button("inspect-remove-comments",cx)
                    .label(text("pptx.inspect.remove-all"))
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.inspect_remove_all_comments(cx)))
            });

            v_flex()
                .id(("inspect-card", 1usize))
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if found && !comments_removed { cx.theme().danger.opacity(0.3) } else { cx.theme().border })
                .bg(if found && !comments_removed { cx.theme().danger.opacity(0.05) } else { cx.theme().background })
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, event: &MouseDownEvent, w, cx| {
                        cx.stop_propagation();
                        this.open_inspect_category_context_menu(1, event.position, w, cx);
                    }),
                )
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_sm()
                                .child(text("_shell.comment").to_string()),
                        )
                        .children(remove_btn),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(if found && !comments_removed { cx.theme().danger } else { cx.theme().muted_foreground })
                        .child(status_text),
                )
        };

        // 3. プレゼンテーション ノート
        let notes_item = {
            let found = insp.notes_count > 0;
            let status_text = if notes_removed {
                text("pptx.inspect-panel.removed-all-presentation-notes").to_string()
            } else if found {
                crate::shell::i18n::format("_pptx.speaker-notes-found-on-count-slides", &[("count", &insp.notes_count)])
            } else {
                text("pptx.inspect-panel.no-presentation-notes-were-found").to_string()
            };

            let remove_btn = (found && !notes_removed).then(|| {
                crate::shell::chrome::destructive_button("inspect-remove-notes",cx)
                    .label(text("pptx.inspect.remove-all"))
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.inspect_remove_all_notes(cx)))
            });

            v_flex()
                .id(("inspect-card", 2usize))
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if found && !notes_removed { cx.theme().danger.opacity(0.3) } else { cx.theme().border })
                .bg(if found && !notes_removed { cx.theme().danger.opacity(0.05) } else { cx.theme().background })
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, event: &MouseDownEvent, w, cx| {
                        cx.stop_propagation();
                        this.open_inspect_category_context_menu(2, event.position, w, cx);
                    }),
                )
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_sm()
                                .child(text("pptx.inspect-panel.presentation-notes").to_string()),
                        )
                        .children(remove_btn),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(if found && !notes_removed { cx.theme().danger } else { cx.theme().muted_foreground })
                        .child(status_text),
                )
        };

        // 4. 非表示スライド
        let hidden_item = {
            let found = insp.hidden_slide_count > 0;
            let status_text = if hidden_removed {
                text("pptx.inspect-panel.removed-hidden-slides").to_string()
            } else if found {
                crate::shell::i18n::format("_pptx.count-hidden-slides-found", &[("count", &insp.hidden_slide_count)])
            } else {
                text("pptx.inspect-panel.no-hidden-slides-were-found").to_string()
            };

            let remove_btn = (found && !hidden_removed).then(|| {
                crate::shell::chrome::destructive_button("inspect-remove-hidden",cx)
                    .label(text("pptx.inspect.remove-all"))
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.inspect_remove_all_hidden_slides(cx)))
            });

            v_flex()
                .id(("inspect-card", 3usize))
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if found && !hidden_removed { cx.theme().danger.opacity(0.3) } else { cx.theme().border })
                .bg(if found && !hidden_removed { cx.theme().danger.opacity(0.05) } else { cx.theme().background })
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, event: &MouseDownEvent, w, cx| {
                        cx.stop_propagation();
                        this.open_inspect_category_context_menu(3, event.position, w, cx);
                    }),
                )
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_sm()
                                .child(text("pptx.inspect-panel.hidden-slides").to_string()),
                        )
                        .children(remove_btn),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(if found && !hidden_removed { cx.theme().danger } else { cx.theme().muted_foreground })
                        .child(status_text),
                )
        };

        // 5. スライド外コンテンツ
        let off_slide_item = {
            let found = insp.off_slide_shape_count > 0;
            let status_text = if off_slide_removed {
                text("pptx.inspect-panel.removed-off-slide-content").to_string()
            } else if found {
                crate::shell::i18n::format("_pptx.count-off-slide-objects-found", &[("count", &insp.off_slide_shape_count)])
            } else {
                text("pptx.inspect-panel.no-off-slide-objects-were").to_string()
            };

            let remove_btn = (found && !off_slide_removed).then(|| {
                crate::shell::chrome::destructive_button("inspect-remove-off-slide",cx)
                    .label(text("pptx.inspect.remove-all"))
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.inspect_remove_all_off_slide_shapes(cx)))
            });

            v_flex()
                .id(("inspect-card", 4usize))
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if found && !off_slide_removed { cx.theme().danger.opacity(0.3) } else { cx.theme().border })
                .bg(if found && !off_slide_removed { cx.theme().danger.opacity(0.05) } else { cx.theme().background })
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, event: &MouseDownEvent, w, cx| {
                        cx.stop_propagation();
                        this.open_inspect_category_context_menu(4, event.position, w, cx);
                    }),
                )
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_sm()
                                .child(text("pptx.inspect-panel.off-slide-content").to_string()),
                        )
                        .children(remove_btn),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(if found && !off_slide_removed { cx.theme().danger } else { cx.theme().muted_foreground })
                        .child(status_text),
                )
        };

        let bottom_actions = crate::shell::chrome::action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("inspect-recheck", text("pptx.inspect-panel.reinspect"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| this.recheck_presentation_inspection(cx))),
            )

            .action(crate::shell::chrome::dialog_cancel_definition(cx));

        let content = v_flex()
            .p_4()
            .gap_3()
            .p_0().child(header)
            .when(!msg.is_empty(), |el| {
                el.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .bg(cx.theme().primary.opacity(0.1))
                        .text_sm()
                        .text_color(cx.theme().primary)
                        .child(msg),
                )
            })
            .child(prop_item)
            .child(comment_item)
            .child(notes_item)
            .child(hidden_item)
            .child(off_slide_item)
            .child(div().h_2())
            ;

        let panel_terminal_actions = bottom_actions;
        Some(SidebarPanel::with_footer(text("pptx.inspect-panel.inspect-presentation"), content.into_any_element(), panel_terminal_actions))
    }
}
