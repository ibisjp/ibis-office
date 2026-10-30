// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スペル チェック (ツール > スペル > スペルチェック…): the words the system's
//! spell checker does not know in the text of the document (the body with its
//! tables, headers and footers, notes and comments: what 検索と置換 searches),
//! one at a time in the shared side panel (`shell/spell_panel.rs`). The word is
//! selected in the text; 変更 replaces it as typing does (one step of undo,
//! tracked when 変更履歴 is on).
use std::collections::HashSet;

use gpui_kit::*;
use ibis_doc_model::flow_command;
use ibis_doc_model::{FlowDocument, FlowPos};

use super::find::{SearchScope, paragraphs};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct SpellPanel {
    pub(crate) issues: Vec<Issue>,
    pub(crate) current: usize,
    /// Words skipped for this check (すべて無視).
    pub(crate) ignored: HashSet<String>,
    pub(crate) suggestions: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IssueKind {
    Spelling,
    Grammar,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Issue {
    pub(crate) para: u32,
    /// Offsets in the paragraph (as `FlowPos`).
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) word: String,
    pub(crate) kind: IssueKind,
    pub(crate) rule_suggestions: Vec<String>,
}

/// Every word not known or grammar issue in the document, in reading order.
pub(crate) fn scan(doc: &FlowDocument, ignored: &HashSet<String>) -> Vec<Issue> {
    let mut found = Vec::new();
    for (para, chars) in paragraphs(doc, SearchScope::All) {
        if chars.is_empty() {
            continue;
        }
        let text: String = chars.iter().map(|(_, c)| *c).collect();
        // The text marked 校正しない (`w:noProof`) is not checked.
        let no_proof: Vec<(usize, usize)> = flow_command::paragraph(doc, para)
            .map(|p| {
                let mut at = 0;
                let mut out = Vec::new();
                for r in &p.runs {
                    if r.format.no_proof == Some(true) {
                        out.push((at, at + r.len()));
                    }
                    at += r.len();
                }
                out
            })
            .unwrap_or_default();

        let is_no_proof = |char_idx: usize| -> bool {
            if char_idx >= chars.len() {
                return false;
            }
            let offset = chars[char_idx].0;
            no_proof.iter().any(|&(a, b)| (a..b).contains(&offset))
        };

        // 1. スペルチェック (Spelling)
        for m in crate::spelling::check_by_settings(&text, None) {
            if m.end == 0 || m.end > chars.len() || ignored.contains(&m.word) || is_no_proof(m.start) {
                continue;
            }
            found.push(Issue {
                para,
                start: chars[m.start].0,
                end: chars[m.end - 1].0 + 1,
                word: m.word,
                kind: IssueKind::Spelling,
                rule_suggestions: Vec::new(),
            });
        }

        // 2. 文法チェック (Grammar)
        for g in crate::grammar::check_grammar(&text) {
            if g.end == 0 || g.end > chars.len() || ignored.contains(&g.text) || is_no_proof(g.start) {
                continue;
            }
            let para_start = chars[g.start].0;
            let para_end = chars[g.end - 1].0 + 1;
            // スペルミスと重複していればスペル側を優先
            if found.iter().any(|i| i.para == para && i.start == para_start && i.end == para_end) {
                continue;
            }
            found.push(Issue {
                para,
                start: para_start,
                end: para_end,
                word: g.text,
                kind: IssueKind::Grammar,
                rule_suggestions: g.suggestions,
            });
        }
    }
    found.sort_by_key(|i| (i.para, i.start));
    found
}

/// What the checker suggests for `issue`.
fn suggestions_for(doc: &FlowDocument, issue: &Issue) -> Vec<String> {
    if issue.kind == IssueKind::Grammar && !issue.rule_suggestions.is_empty() {
        return issue.rule_suggestions.clone();
    }
    let Some(p) = flow_command::paragraph(doc, issue.para) else { return Vec::new() };
    let chars = super::editing::visible_chars(p);
    let text: String = chars.iter().map(|(_, c)| *c).collect();
    let (Some(start), Some(end)) = (chars.iter().position(|(o, _)| *o == issue.start), chars.iter().position(|(o, _)| *o == issue.end - 1)) else { return Vec::new() };
    crate::spelling::suggestions(&text, start, end + 1, None)
}

impl SlideEditor {
    /// ツール > スペル > スペルチェック…: the panel, at the first word.
    pub(crate) fn open_doc_spelling(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(doc) = self.flow().cloned() else { return };
        let ignored = HashSet::new();
        let issues = scan(&doc, &ignored);
        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = super::help_panels::HelpPanels::default();
            s.helper.spelling = Some(SpellPanel { issues, current: 0, ignored, suggestions: Vec::new() });
        }
        self.sidebar_open = true;
        self.doc_spelling_goto(0, cx);
    }

    pub(crate) fn close_doc_spelling(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.helper.spelling = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn doc_spelling_issue(&self) -> Option<Issue> {
        let p = self.doc_state()?.helper.spelling.as_ref()?;
        p.issues.get(p.current).cloned()
    }

    /// The word number `index` selected in the text, with its suggestions.
    pub(crate) fn doc_spelling_goto(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(issue) = self.doc_state().and_then(|s| s.helper.spelling.as_ref()).and_then(|p| p.issues.get(index).cloned()) else {
            if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.spelling.as_mut()) {
                p.current = 0;
                p.suggestions.clear();
            }
            return cx.notify();
        };
        let suggestions = self.flow().map(|d| suggestions_for(d, &issue)).unwrap_or_default();
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.spelling.as_mut()) {
            p.current = index;
            p.suggestions = suggestions;
        }
        self.set_caret(FlowPos::new(issue.para, issue.start), false, cx);
        self.set_caret(FlowPos::new(issue.para, issue.end), true, cx);
        self.scroll_to_caret(cx);
        cx.notify();
    }

    pub(crate) fn doc_spelling_step(&mut self, forward: bool, _: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.spelling.as_ref()) else { return };
        let n = p.issues.len();
        if n == 0 {
            return;
        }
        let index = if forward { (p.current + 1) % n } else { (p.current + n - 1) % n };
        self.doc_spelling_goto(index, cx);
    }

    /// The check made again (after a change), at the word number `near`.
    fn doc_spelling_rescan(&mut self, near: usize, cx: &mut Context<Self>) {
        let Some(ignored) = self.doc_state().and_then(|s| s.helper.spelling.as_ref()).map(|p| p.ignored.clone()) else { return };
        let issues = self.flow().map(|d| scan(d, &ignored)).unwrap_or_default();
        let index = near.min(issues.len().saturating_sub(1));
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.spelling.as_mut()) {
            p.issues = issues;
        }
        self.doc_spelling_goto(index, cx);
    }

    /// 変更 (or すべて変更: the same word everywhere): the word replaced by `text`, in one step.
    pub(crate) fn doc_spelling_change(&mut self, text: &str, all: bool, _: &mut Window, cx: &mut Context<Self>) {
        let Some(issue) = self.doc_spelling_issue() else { return };
        let Some(doc) = self.flow().cloned() else { return };
        let Some(p) = self.doc_state().and_then(|s| s.helper.spelling.as_ref()) else { return };
        let at = p.current;
        let hits: Vec<Issue> = if all { p.issues.iter().filter(|i| i.word == issue.word).cloned().collect() } else { vec![issue] };
        // From the last, so the earlier positions stay where they are.
        let mut commands = Vec::new();
        for hit in hits.iter().rev() {
            let (a, b) = (FlowPos::new(hit.para, hit.start), FlowPos::new(hit.para, hit.end));
            commands.extend(Self::replacement_commands(&doc, a, b, text, None).0);
        }
        self.run_flow(commands, cx);
        self.doc_spelling_rescan(at, cx);
    }

    /// 無視 (or すべて無視): this word (or this spelling everywhere) is not told again.
    pub(crate) fn doc_spelling_ignore(&mut self, all: bool, _: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state_mut().and_then(|s| s.helper.spelling.as_mut()) else { return };
        let Some(issue) = p.issues.get(p.current).cloned() else { return };
        let at = p.current;
        if all {
            p.ignored.insert(issue.word);
            self.doc_spelling_rescan(at, cx);
        } else {
            p.issues.remove(at);
            let left = p.issues.len();
            self.doc_spelling_goto(at.min(left.saturating_sub(1)), cx);
        }
    }

    /// 辞書に追加: the word learned by the system's dictionary.
    pub(crate) fn doc_spelling_learn(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(issue) = self.doc_spelling_issue() else { return };
        crate::spelling::learn(&issue.word);
        let at = self.doc_state().and_then(|s| s.helper.spelling.as_ref()).map_or(0, |p| p.current);
        self.doc_spelling_rescan(at, cx);
    }

    pub(crate) fn doc_spelling_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.spelling.as_ref()?;
        let current = p.current.min(p.issues.len().saturating_sub(1));
        let view = crate::shell::spell_panel::SpellView { prefix: "docx-spell", current, total: p.issues.len(), word: p.issues.get(current).map(|i| i.word.as_str()), suggestions: &p.suggestions };
        let actions = crate::shell::spell_panel::SpellActions {
            change: Self::doc_spelling_change,
            ignore: Self::doc_spelling_ignore,
            learn: Self::doc_spelling_learn,
            step: Self::doc_spelling_step,
            close: Self::close_doc_spelling,
        };
        Some(crate::shell::spell_panel::sidebar(view, actions, cx))
    }
}
