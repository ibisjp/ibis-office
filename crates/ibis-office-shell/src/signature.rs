// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Digital signatures of the open file (GitHub issue #232): the information bar that says the file is signed, the question before a
//! save or an export that makes the signatures invalid, and the report of what the signatures are (`ibis_ooxml::signature`, for the
//! document, spreadsheet and presentation screens alike).
//!
//! What is looked at is only the file as it was opened: whether the signature value and the digests of the parts the signature
//! covers agree with the file. The certificate's trust and revocation, time stamps, and whether the signature covers the whole
//! document are not looked at, and the report says so. A file changed here is checked by saving it and opening the saved file again.

use std::rc::Rc;

use gpui_kit::*;
use ibis_formats::ibis_ooxml::signature::{Check, Inspection, Malformed, Reason, SignatureReport, Status};

use crate::editor::SlideEditor;
use crate::format::with_format;
use crate::i18n::{format, text};

/// What the screen knows of the open file's signatures.
#[derive(Clone, Debug, Default)]
pub struct SignatureState {
    /// The file as opened has digital signatures.
    pub present: bool,
    /// The user answered OK to "saving makes the signatures invalid": not asked again for this file.
    pub save_acknowledged: bool,
    /// The same for an export (a PDF, an image, a copy in another format), which never carries the signatures.
    pub export_acknowledged: bool,
}

/// What is about to be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SignatureWay {
    /// 保存, 名前を付けて保存, コピーを作成: the file with the edits made; invalid only if something was edited.
    Save,
    /// A format that does not carry signatures (PDF, images, OpenDocument, a slide show or template, without macros).
    Export,
}

/// How many lines of what does not agree one signature lists.
const DETAIL_LINES: usize = 5;

impl SlideEditor {
    /// Whether the file as opened has digital signatures.
    pub fn has_digital_signature(&self) -> bool {
        self.signature.present
    }

    /// The file just opened: remember whether it is signed (a cheap look at the two relationship parts; the check is made when asked).
    pub(crate) fn note_loaded_signatures(&mut self, source: &ibis_formats::Source) {
        self.signature = SignatureState { present: source.has_signature(), ..SignatureState::default() };
    }

    /// Before writing the file: if it is signed and what is written would not carry valid signatures, ask (OK or キャンセル) and
    /// return true; the caller stops, and `again` runs the same action once OK is answered (the question is not asked twice for the
    /// file). Returns false when there is nothing to ask. Every way of writing the document starts here.
    pub(crate) fn confirm_signature_loss(
        &mut self,
        way: SignatureWay,
        window: &mut Window,
        cx: &mut Context<Self>,
        again: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> bool {
        let needed = self.signature.present
            && match way {
                SignatureWay::Save => !self.signature.save_acknowledged && self.is_dirty(),
                SignatureWay::Export => !self.signature.export_acknowledged,
            };
        if !needed {
            return false;
        }
        let handle = window.window_handle();
        let again = Rc::new(again);
        let message = match way {
            SignatureWay::Save => text("shell.signature.confirm.save"),
            SignatureWay::Export => text("shell.signature.confirm.export"),
        };
        self.ask(
            text("shell.signature.confirm.title"),
            message,
            Rc::new(move |this: &mut Self, cx: &mut Context<Self>| {
                match way {
                    SignatureWay::Save => this.signature.save_acknowledged = true,
                    SignatureWay::Export => this.signature.export_acknowledged = true,
                }
                let again = again.clone();
                // The answer has no window; the action goes on in the window it was asked in.
                cx.spawn(async move |this, cx| {
                    cx.update_window(handle, |_, window, cx| {
                        this.update(cx, |this, cx| again(this, window, cx)).ok();
                    })
                    .ok();
                })
                .detach();
            }),
            cx,
        );
        true
    }

    /// What is about to be written, without the digital signatures when the file as written would not carry valid ones (Office
    /// takes them out too): the file was edited, or `always` (an export, or a save into another kind of file). Returns the bytes and
    /// whether the signatures were taken out. An unedited save keeps them (the file is the original, so they still agree).
    pub(crate) fn without_signatures(&self, bytes: Vec<u8>, always: bool) -> (Vec<u8>, bool) {
        if !self.signature.present || !(always || self.is_dirty()) {
            return (bytes, false);
        }
        match ibis_formats::ibis_ooxml::signature::strip_bytes(&bytes) {
            Ok(Some(stripped)) => (stripped, true),
            // Not a package, or none inside: as it is. A package that cannot be rewritten is written as it is, not lost.
            Ok(None) | Err(_) => (bytes, false),
        }
    }

    /// The signatures were written out of the file: the bar goes, and nothing more is asked.
    pub(crate) fn signatures_removed(&mut self) {
        self.signature = SignatureState::default();
    }

    /// The information bar's button: look at the signatures of the file as opened, and show the result.
    pub fn show_signature_report(&mut self, cx: &mut Context<Self>) {
        // A viewer of a PDF, XPS or OFD words its own report (its signatures are not an Open Packaging package).
        if let Some(report) = with_format!(self, signature_report(self)) {
            self.show_alert(text("shell.signature.report.title"), report.into(), cx);
            return;
        }
        let Some(source) = self.source.clone() else { return };
        let inspection = source.inspect_signatures();
        self.show_alert(text("shell.signature.report.title"), report_text(&inspection).into(), cx);
    }
}

/// The words of the report for what `ibis_ooxml::signature::inspect` found.
pub(crate) fn report_text(inspection: &Inspection) -> String {
    let mut lines: Vec<String> = Vec::new();
    match inspection {
        Inspection::Unsigned => lines.push(text("shell.signature.report.none").to_string()),
        Inspection::Malformed(why) => lines.push(format("shell.signature.report.broken", &[("reason", &malformed_word(why))])),
        Inspection::Signed(reports) => {
            for (i, report) in reports.iter().enumerate() {
                lines.extend(signature_lines(i + 1, report));
            }
        }
    }
    lines.push(String::new());
    lines.push(text("shell.signature.report.notice").to_string());
    lines.join("\n")
}

fn signature_lines(n: usize, report: &SignatureReport) -> Vec<String> {
    let mut lines = Vec::new();
    let result = match (report.status, &report.malformed) {
        (Status::Consistent, _) => text("shell.signature.result.consistent").to_string(),
        (Status::Mismatch, _) => text("shell.signature.result.mismatch").to_string(),
        (Status::Unsupported, _) => text("shell.signature.result.unsupported").to_string(),
        (Status::Uninspectable, _) => text("shell.signature.result.uninspectable").to_string(),
        (Status::Malformed, why) => format("shell.signature.result.malformed", &[("reason", &why.as_ref().map(malformed_word).unwrap_or_default())]),
    };
    lines.push(format("shell.signature.report.head", &[("n", &n), ("part", &report.part), ("result", &result)]));
    if let Some(signer) = &report.signer {
        lines.push(format("shell.signature.report.signer", &[("name", signer)]));
    }
    if let Some(value) = &report.signature_value {
        lines.push(format("shell.signature.report.value", &[("state", &check_word(value))]));
    }
    if !report.references.is_empty() {
        let matched = report.references.iter().filter(|r| r.check == Check::Match).count();
        lines.push(format("shell.signature.report.digests", &[("matched", &matched), ("total", &report.references.len())]));
        let others: Vec<_> = report.references.iter().filter(|r| r.check != Check::Match).collect();
        for r in others.iter().take(DETAIL_LINES) {
            lines.push(format("shell.signature.report.detail", &[("what", &reference_name(&r.uri)), ("state", &check_word(&r.check))]));
        }
        if others.len() > DETAIL_LINES {
            lines.push(format("shell.signature.report.more", &[("count", &(others.len() - DETAIL_LINES))]));
        }
    }
    lines
}

/// A reference's URI as the part it names (`/word/document.xml?ContentType=…` is `word/document.xml`).
fn reference_name(uri: &str) -> String {
    match uri.split('?').next() {
        Some(path) if !path.is_empty() => path.trim_start_matches('/').to_string(),
        _ => uri.to_string(),
    }
}

fn check_word(check: &Check) -> String {
    match check {
        Check::Match => text("shell.signature.check.match").to_string(),
        Check::Mismatch => text("shell.signature.check.mismatch").to_string(),
        Check::PartMissing => text("shell.signature.check.part-missing").to_string(),
        Check::Unsupported(what) => format("shell.signature.check.unsupported", &[("what", what)]),
        Check::Uninspectable(reason) => format(
            "shell.signature.check.uninspectable",
            &[(
                "reason",
                &match reason {
                    Reason::External => text("shell.signature.reason.external"),
                    Reason::NoKey => text("shell.signature.reason.no-key"),
                    Reason::BadKey => text("shell.signature.reason.bad-key"),
                    Reason::TargetMissing => text("shell.signature.reason.target-missing"),
                    Reason::NotXml => text("shell.signature.reason.not-xml"),
                },
            )],
        ),
    }
}

fn malformed_word(why: &Malformed) -> String {
    text(match why {
        Malformed::Relationships => "shell.signature.malformed.relationships",
        Malformed::OriginMissing => "shell.signature.malformed.origin-missing",
        Malformed::OriginPartMissing => "shell.signature.malformed.origin-part-missing",
        Malformed::SignaturePartMissing => "shell.signature.malformed.signature-part-missing",
        Malformed::Unreadable => "shell.signature.malformed.unreadable",
        Malformed::Xml => "shell.signature.malformed.xml",
        Malformed::NotSignature => "shell.signature.malformed.not-signature",
        Malformed::NoSignedInfo => "shell.signature.malformed.no-signed-info",
        Malformed::NoSignatureValue => "shell.signature.malformed.no-signature-value",
        Malformed::NoReference => "shell.signature.malformed.no-reference",
        Malformed::BadReference => "shell.signature.malformed.bad-reference",
        Malformed::BadBase64 => "shell.signature.malformed.bad-base64",
    })
    .to_string()
}
