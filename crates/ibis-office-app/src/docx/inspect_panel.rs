// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ドキュメントの検査 (Inspect Document: DOCX-SEC-011):
//! 文書内の個人情報、プロパティ、コメント、変更履歴、ヘッダー・フッター、隠し文字を検査し、一括削除する。

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::flow::{DocProperties, FlowDocument};
use ibis_doc_model::flow_command;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// ドキュメントの検査結果
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DocumentInspection {
    /// ドキュメントのプロパティと個人情報の有無
    pub(crate) has_properties: bool,
    /// 検出されたプロパティの詳細説明リスト
    pub(crate) properties_details: Vec<String>,
    /// コメント総件数
    pub(crate) comment_count: usize,
    /// 変更履歴総件数
    pub(crate) revision_count: usize,
    /// ヘッダー・フッターの有無
    pub(crate) has_headers_footers: bool,
    /// ヘッダー・フッターの箇所数
    pub(crate) header_footer_count: usize,
    /// 隠し文字の箇所数
    pub(crate) hidden_text_count: usize,
}

/// ドキュメントをスキャンして個人情報・非表示要素を検査する
pub(crate) fn inspect_document(doc: &FlowDocument) -> DocumentInspection {
    let mut details = Vec::new();
    let props = &doc.properties;

    if let Some(t) = &props.title {
        if !t.trim().is_empty() { details.push(crate::shell::i18n::format("_docx.title-value", &[("value", &t.trim())])); }
    }
    if let Some(c) = &props.creator {
        if !c.trim().is_empty() { details.push(crate::shell::i18n::format("_docx.author-value", &[("value", &c.trim())])); }
    }
    if let Some(s) = &props.subject {
        if !s.trim().is_empty() { details.push(crate::shell::i18n::format("_docx.subject-value", &[("value", &s.trim())])); }
    }
    if let Some(k) = &props.keywords {
        if !k.trim().is_empty() { details.push(crate::shell::i18n::format("_docx.keywords-value", &[("value", &k.trim())])); }
    }
    if let Some(d) = &props.description {
        if !d.trim().is_empty() { details.push(crate::shell::i18n::format("_docx.comments-value", &[("value", &d.trim())])); }
    }
    if let Some(cat) = &props.category {
        if !cat.trim().is_empty() { details.push(crate::shell::i18n::format("_docx.category-value", &[("value", &cat.trim())])); }
    }
    if let Some(m) = &props.manager {
        if !m.trim().is_empty() { details.push(crate::shell::i18n::format("_docx.manager-value", &[("value", &m.trim())])); }
    }
    if let Some(co) = &props.company {
        if !co.trim().is_empty() { details.push(crate::shell::i18n::format("_docx.company-value", &[("value", &co.trim())])); }
    }
    if let Some(lm) = &props.last_modified_by {
        if !lm.trim().is_empty() { details.push(crate::shell::i18n::format("_docx.last-modified-by-value", &[("value", &lm.trim())])); }
    }
    if !props.custom.is_empty() {
        details.push(crate::shell::i18n::format("_docx.custom-properties-count", &[("count", &props.custom.len())]));
    }

    let has_properties = !details.is_empty();
    let comment_count = doc.comments.len();

    let all_paras = flow_command::all_paragraphs_everywhere(doc);

    let mut revision_count = 0;
    let mut hidden_text_count = 0;
    for p in &all_paras {
        for r in &p.runs {
            if r.revision.is_some() {
                revision_count += 1;
            }
            if r.style.hidden || r.format.hidden == Some(true) {
                hidden_text_count += 1;
            }
        }
    }

    let mut header_footer_count = 0;
    for list in [&doc.header, &doc.footer, &doc.header_first, &doc.footer_first, &doc.header_even, &doc.footer_even] {
        if !list.is_empty() {
            header_footer_count += list.len();
        }
    }
    for b in &doc.body {
        if let ibis_doc_model::Block::Paragraph(p) = b && let Some(sec) = &p.section {
            for part in &sec.parts {
                if let Some(blocks) = part && !blocks.is_empty() {
                    header_footer_count += blocks.len();
                }
            }
        }
    }
    let has_headers_footers = header_footer_count > 0;

    DocumentInspection {
        has_properties,
        properties_details: details,
        comment_count,
        revision_count,
        has_headers_footers,
        header_footer_count,
        hidden_text_count,
    }
}

/// サイドバーに表示するドキュメント検査パネル状態
pub(crate) struct DocInspectState {
    pub(crate) inspection: DocumentInspection,
    pub(crate) properties_removed: bool,
    pub(crate) comments_removed: bool,
    pub(crate) revisions_removed: bool,
    pub(crate) headers_footers_removed: bool,
    pub(crate) hidden_text_removed: bool,
    pub(crate) message: String,
}

impl SlideEditor {
    /// ドキュメント検査パネルを開く
    pub(crate) fn open_doc_inspect_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(doc) = self.flow() else { return };
        let inspection = inspect_document(doc);
        let panel = DocInspectState {
            inspection,
            properties_removed: false,
            comments_removed: false,
            revisions_removed: false,
            headers_footers_removed: false,
            hidden_text_removed: false,
            message: String::new(),
        };

        if let Some(s) = self.doc_state_mut() {
            s.inspect_panel = Some(panel);
            s.compatibility_panel = None;
            s.docx_math = None;
            s.quick_parts = None;
            s.table_formula = None;
            s.table_size = None;
            s.picture_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.style_edit = None;
            s.link = None;
            s.find = None;
            s.comments = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// ドキュメント検査パネルを閉じる
    pub(crate) fn close_doc_inspect_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.inspect_panel = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// 検査を再実行
    pub(crate) fn recheck_doc_inspection(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let inspection = inspect_document(doc);
        if let Some(p) = self.doc_state_mut().and_then(|s| s.inspect_panel.as_mut()) {
            p.inspection = inspection;
            p.message = text("docx.inspect-panel.reinspection-complete").to_string();
            cx.notify();
        }
    }

    /// ドキュメントのプロパティと個人情報をすべて削除
    pub(crate) fn doc_inspect_remove_all_properties(&mut self, cx: &mut Context<Self>) {
        let empty_props = DocProperties::default();
        self.run_flow(vec![ibis_doc_model::FlowCommand::SetDocProperties(empty_props)], cx);
        if let Some(doc) = self.flow() {
            let inspection = inspect_document(doc);
            if let Some(p) = self.doc_state_mut().and_then(|s| s.inspect_panel.as_mut()) {
                p.properties_removed = true;
                p.inspection = inspection;
                p.message = text("docx.inspect-panel.removed-properties").to_string();
                cx.notify();
            }
        }
    }

    /// コメントをすべて削除
    pub(crate) fn doc_inspect_remove_all_comments(&mut self, cx: &mut Context<Self>) {
        let cmd = self.flow().and_then(|d| flow_command::delete_all_comments(d));
        if let Some(cmd) = cmd {
            self.run_flow(vec![cmd], cx);
        }
        if let Some(doc) = self.flow() {
            let inspection = inspect_document(doc);
            if let Some(p) = self.doc_state_mut().and_then(|s| s.inspect_panel.as_mut()) {
                p.comments_removed = true;
                p.inspection = inspection;
                p.message = text("docx.inspect-panel.removed-comments").to_string();
                cx.notify();
            }
        }
    }

    /// 変更履歴をすべて承諾
    pub(crate) fn doc_inspect_remove_all_revisions(&mut self, cx: &mut Context<Self>) {
        self.resolve_revisions(true, true, cx);
        if let Some(doc) = self.flow() {
            let inspection = inspect_document(doc);
            if let Some(p) = self.doc_state_mut().and_then(|s| s.inspect_panel.as_mut()) {
                p.revisions_removed = true;
                p.inspection = inspection;
                p.message = text("docx.inspect-panel.removed-revisions").to_string();
                cx.notify();
            }
        }
    }

    /// ヘッダーとフッターをすべて削除
    pub(crate) fn doc_inspect_remove_all_headers_footers(&mut self, cx: &mut Context<Self>) {
        let cmd = self.flow().and_then(|d| flow_command::clear_all_headers_footers(d));
        if let Some(cmd) = cmd {
            self.run_flow(vec![cmd], cx);
        }
        if let Some(doc) = self.flow() {
            let inspection = inspect_document(doc);
            if let Some(p) = self.doc_state_mut().and_then(|s| s.inspect_panel.as_mut()) {
                p.headers_footers_removed = true;
                p.inspection = inspection;
                p.message = text("docx.inspect-panel.removed-headers-footers").to_string();
                cx.notify();
            }
        }
    }

    /// 隠し文字をすべて削除
    pub(crate) fn doc_inspect_remove_all_hidden_text(&mut self, cx: &mut Context<Self>) {
        let cmd = self.flow().and_then(|d| flow_command::remove_all_hidden_text(d));
        if let Some(cmd) = cmd {
            self.run_flow(vec![cmd], cx);
        }
        if let Some(doc) = self.flow() {
            let inspection = inspect_document(doc);
            if let Some(p) = self.doc_state_mut().and_then(|s| s.inspect_panel.as_mut()) {
                p.hidden_text_removed = true;
                p.inspection = inspection;
                p.message = text("docx.inspect-panel.removed-hidden-text").to_string();
                cx.notify();
            }
        }
    }

    /// サイドバーのドキュメント検査パネル UI 要素
    pub(crate) fn doc_inspect_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.doc_state()?.inspect_panel.as_ref()?;
        let insp = panel.inspection.clone();
        let props_removed = panel.properties_removed;
        let comments_removed = panel.comments_removed;
        let revisions_removed = panel.revisions_removed;
        let headers_removed = panel.headers_footers_removed;
        let hidden_removed = panel.hidden_text_removed;
        let msg = panel.message.clone();

        let header = v_flex()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(text("docx.inspect-panel.inspect-hint").to_string()),
            );

        // 1. プロパティと個人情報
        let prop_item = {
            let found = insp.has_properties;
            let status_text = if props_removed {
                text("docx.inspect-panel.removed-properties").to_string()
            } else if found {
                crate::shell::i18n::format("_docx.found-details", &[("details", &insp.properties_details.join(", "))])
            } else {
                text("docx.inspect-panel.no-properties").to_string()
            };

            let remove_btn = (found && !props_removed).then(|| {
                crate::shell::chrome::destructive_button("inspect-remove-props",cx)
                    .label(text("docx.inspect.remove-all"))
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.doc_inspect_remove_all_properties(cx)))
            });

            v_flex()
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if found && !props_removed { cx.theme().danger.opacity(0.3) } else { cx.theme().border })
                .bg(if found && !props_removed { cx.theme().danger.opacity(0.05) } else { cx.theme().background })
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_sm()
                                .child(text("docx.inspect-panel.document-properties").to_string()),
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
                text("docx.inspect-panel.removed-comments").to_string()
            } else if found {
                crate::shell::i18n::format("_docx.count-comments-found", &[("count", &insp.comment_count)])
            } else {
                text("docx.inspect-panel.no-comments").to_string()
            };

            let remove_btn = (found && !comments_removed).then(|| {
                crate::shell::chrome::destructive_button("inspect-remove-comments",cx)
                    .label(text("docx.inspect.remove-all"))
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.doc_inspect_remove_all_comments(cx)))
            });

            v_flex()
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if found && !comments_removed { cx.theme().danger.opacity(0.3) } else { cx.theme().border })
                .bg(if found && !comments_removed { cx.theme().danger.opacity(0.05) } else { cx.theme().background })
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_sm()
                                .child(text("docx.inspect-panel.comments").to_string()),
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

        // 3. 変更履歴
        let revision_item = {
            let found = insp.revision_count > 0;
            let status_text = if revisions_removed {
                text("docx.inspect-panel.removed-revisions").to_string()
            } else if found {
                crate::shell::i18n::format("_docx.count-revisions-found", &[("count", &insp.revision_count)])
            } else {
                text("docx.inspect-panel.no-revisions").to_string()
            };

            let remove_btn = (found && !revisions_removed).then(|| {
                crate::shell::chrome::destructive_button("inspect-remove-revisions",cx)
                    .label(text("docx.inspect.remove-all"))
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.doc_inspect_remove_all_revisions(cx)))
            });

            v_flex()
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if found && !revisions_removed { cx.theme().danger.opacity(0.3) } else { cx.theme().border })
                .bg(if found && !revisions_removed { cx.theme().danger.opacity(0.05) } else { cx.theme().background })
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_sm()
                                .child(text("docx.inspect-panel.revisions").to_string()),
                        )
                        .children(remove_btn),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(if found && !revisions_removed { cx.theme().danger } else { cx.theme().muted_foreground })
                        .child(status_text),
                )
        };

        // 4. ヘッダーとフッター
        let header_footer_item = {
            let found = insp.has_headers_footers;
            let status_text = if headers_removed {
                text("docx.inspect-panel.removed-headers-footers").to_string()
            } else if found {
                crate::shell::i18n::format("_docx.count-headers-footers-found", &[("count", &insp.header_footer_count)])
            } else {
                text("docx.inspect-panel.no-headers-footers").to_string()
            };

            let remove_btn = (found && !headers_removed).then(|| {
                crate::shell::chrome::destructive_button("inspect-remove-headers-footers",cx)
                    .label(text("docx.inspect.remove-all"))
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.doc_inspect_remove_all_headers_footers(cx)))
            });

            v_flex()
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if found && !headers_removed { cx.theme().danger.opacity(0.3) } else { cx.theme().border })
                .bg(if found && !headers_removed { cx.theme().danger.opacity(0.05) } else { cx.theme().background })
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_sm()
                                .child(text("docx.inspect-panel.headers-footers").to_string()),
                        )
                        .children(remove_btn),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(if found && !headers_removed { cx.theme().danger } else { cx.theme().muted_foreground })
                        .child(status_text),
                )
        };

        // 5. 隠し文字
        let hidden_text_item = {
            let found = insp.hidden_text_count > 0;
            let status_text = if hidden_removed {
                text("docx.inspect-panel.removed-hidden-text").to_string()
            } else if found {
                crate::shell::i18n::format("_docx.count-hidden-text-found", &[("count", &insp.hidden_text_count)])
            } else {
                text("docx.inspect-panel.no-hidden-text").to_string()
            };

            let remove_btn = (found && !hidden_removed).then(|| {
                crate::shell::chrome::destructive_button("inspect-remove-hidden-text",cx)
                    .label(text("docx.inspect.remove-all"))
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.doc_inspect_remove_all_hidden_text(cx)))
            });

            v_flex()
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if found && !hidden_removed { cx.theme().danger.opacity(0.3) } else { cx.theme().border })
                .bg(if found && !hidden_removed { cx.theme().danger.opacity(0.05) } else { cx.theme().background })
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_sm()
                                .child(text("docx.inspect-panel.hidden-text").to_string()),
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

        // メッセージ通知表示
        let message_box = (!msg.is_empty()).then(|| {
            div()
                .p_2()
                .text_sm()
                .rounded_md()
                .bg(cx.theme().primary.opacity(0.1))
                .text_color(cx.theme().primary)
                .child(msg)
        });

        let footer = crate::shell::chrome::action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("inspect-recheck", text("docx.inspect-panel.reinspect"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| this.recheck_doc_inspection(cx))),
            )
            .action(
                crate::shell::chrome::ActionDefinition::new("inspect-close", text("_shell.close"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| this.close_doc_inspect_panel(cx))),
            );

        let body = v_flex()
            .p_4()
            .gap_3()
            .p_0().child(header)
            .children(message_box)
            .child(prop_item)
            .child(comment_item)
            .child(revision_item)
            .child(header_footer_item)
            .child(hidden_text_item)
            ;

        let panel_terminal_actions = footer;
        Some(SidebarPanel::with_footer(text("docx.inspect-panel.title"), body.into_any_element(), panel_terminal_actions))
    }
}
