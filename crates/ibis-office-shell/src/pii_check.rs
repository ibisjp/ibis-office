// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 校閲 > 個人情報の消し忘れの検査… (GitHub issue #246): the door of `ibis_formats::pii_preflight` for the three screens. The document is looked at on this machine (nothing is sent):
//! hidden slides and sheets, notes, comments, and numbers that look like an Individual Number (マイナンバー) or a patient ID. What is found is listed in a message box; the
//! document is not changed (taking the things out is done with the screen's own tools, which can be undone).

use crate::chrome::MenuItem;
use crate::editor::SlideEditor;
use crate::i18n::{format, text};
use gpui_kit::*;

/// How many findings the message lists before 「ほか N 件」.
const LISTED: usize = 12;

/// The menu line 校閲 > 個人情報の消し忘れの検査… (`review.pii-check`), the same on every screen.
pub fn pii_check_item() -> MenuItem {
    MenuItem::tr("review.pii-check").icon("security").on(|this, _, cx| this.pii_check(cx))
}

impl SlideEditor {
    /// The words of the message box for what the check found (`None` findings: the document is clean).
    pub fn pii_check_message(&self) -> (String, usize) {
        let report = ibis_formats::pii_preflight::inspect_presentation_pii(self.presentation());
        if report.is_clean() {
            return (text("pii.clean").to_string(), 0);
        }
        let mut lines = vec![format("pii.found", &[("count", &report.findings.len())]).to_string(), String::new()];
        lines.extend(report.findings.iter().take(LISTED).map(|f| format!("• {}: {}", f.location, f.detail)));
        if report.findings.len() > LISTED {
            lines.push(format("pii.more", &[("count", &(report.findings.len() - LISTED))]).to_string());
        }
        (lines.join("\n"), report.findings.len())
    }

    /// 個人情報の消し忘れの検査: the document looked at, the result in a message box. When hidden slides or sheets, notes or comments were found, the box has
    /// 取り除く… (asks once more, then takes them out as one step that can be undone) beside 閉じる.
    pub fn pii_check(&mut self, cx: &mut Context<Self>) {
        let (message, _) = self.pii_check_message();
        let title: SharedString = text("review.pii-check").trim_end_matches('…').to_string().into();
        let (commands, summary) = ibis_formats::pii_preflight::plan_pii_sanitize(self.presentation());
        if commands.is_empty() {
            self.show_alert(title, message.into(), cx);
            return;
        }
        let what = format(
            "pii.remove.what",
            &[("slides", &summary.removed_hidden_slides), ("sheets", &summary.removed_hidden_sheets), ("notes", &summary.cleared_notes), ("comments", &summary.removed_comments)],
        )
        .to_string();
        let confirm_title = title.clone();
        let on_ok: std::rc::Rc<dyn Fn(&mut SlideEditor, &mut Context<SlideEditor>)> = std::rc::Rc::new(move |this, cx| {
            let confirm = format!("{what}\n\n{}", text("pii.remove.confirm"));
            let title = confirm_title.clone();
            this.ask(title, confirm.into(), std::rc::Rc::new(|this, cx| this.pii_remove(cx)), cx);
        });
        self.ask_with_buttons(title, message.into(), text("pii.remove"), text("shell.action.close"), on_ok, cx);
    }

    /// Take out what the check found (hidden slides and sheets, notes, comments) as one undoable step.
    pub fn pii_remove(&mut self, cx: &mut Context<Self>) {
        let (commands, _) = ibis_formats::pii_preflight::plan_pii_sanitize(self.presentation());
        if !commands.is_empty() {
            self.execute(commands, cx);
        }
    }
}
