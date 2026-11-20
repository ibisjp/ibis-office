// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/246)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pure-Rust pre-export PII preflight and sanitization for Office documents
//! (`DOCX`, `XLSX`, `PPTX`) before external / academic / inter-hospital sharing.

use std::sync::Arc;

use ibis_doc_model::{Presentation, ShapeKind, SheetState};

/// Category of pre-export PII / hidden-data finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PiiFindingKind {
    HiddenSlide,
    SpeakerNotes,
    SlideComment,
    HiddenWorksheet,
    SensitiveNumericPattern,
}

/// One preflight finding detected in a [`Presentation`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PiiFinding {
    pub kind: PiiFindingKind,
    pub location: String,
    pub detail: String,
}

/// Result of [`inspect_presentation_pii`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PiiPreflightReport {
    pub findings: Vec<PiiFinding>,
    pub hidden_slides: usize,
    pub notes_count: usize,
    pub comments_count: usize,
    pub hidden_sheets: usize,
    pub sensitive_pattern_hits: usize,
}

impl PiiPreflightReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Summary of items removed by [`sanitize_presentation_pii`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PiiSanitizeSummary {
    pub removed_hidden_slides: usize,
    pub cleared_notes: usize,
    pub removed_comments: usize,
    pub removed_hidden_sheets: usize,
}

/// Inspect a [`Presentation`] locally for hidden slides, speaker notes,
/// comments, hidden worksheets, and 12-digit MyNumber / 8-digit insurer-like
/// numeric patterns in spreadsheet cells or slide text.
pub fn inspect_presentation_pii(presentation: &Presentation) -> PiiPreflightReport {
    let mut report = PiiPreflightReport::default();

    for (idx, slide) in presentation.slides.iter().enumerate() {
        let slide_no = idx + 1;
        if slide.hidden {
            report.hidden_slides += 1;
            report.findings.push(PiiFinding {
                kind: PiiFindingKind::HiddenSlide,
                location: crate::words::format("medical.pii.location.slide", &[("no", &slide_no)]),
                detail: crate::words::text("medical.pii.detail.hidden-slide"),
            });
        }
        if !slide.notes.trim().is_empty() {
            report.notes_count += 1;
            report.findings.push(PiiFinding {
                kind: PiiFindingKind::SpeakerNotes,
                location: crate::words::format("medical.pii.location.notes", &[("no", &slide_no)]),
                detail: crate::words::text("medical.pii.detail.notes"),
            });
        }
        if !slide.comments.is_empty() {
            report.comments_count += slide.comments.len();
            report.findings.push(PiiFinding {
                kind: PiiFindingKind::SlideComment,
                location: crate::words::format("medical.pii.location.comments", &[("no", &slide_no)]),
                detail: crate::words::format("medical.pii.detail.comments", &[("count", &slide.comments.len())]),
            });
        }

        for shape in &slide.shapes {
            match &shape.kind {
                ShapeKind::Sheet(sheet) => {
                    for cell in &sheet.cells {
                        if let Some(reason) = detect_sensitive_text(&cell.text) {
                            report.sensitive_pattern_hits += 1;
                            report.findings.push(PiiFinding {
                                kind: PiiFindingKind::SensitiveNumericPattern,
                                location: crate::words::format("medical.pii.location.sheet-cell", &[("no", &slide_no), ("row", &(cell.row + 1)), ("column", &(cell.column + 1))]),
                                detail: reason,
                            });
                        }
                    }
                }
                ShapeKind::TextBox(tb) => {
                    for p in &tb.paragraphs {
                        let txt: String = p.runs.iter().map(|r| r.text.as_str()).collect();
                        if let Some(reason) = detect_sensitive_text(&txt) {
                            report.sensitive_pattern_hits += 1;
                            report.findings.push(PiiFinding {
                                kind: PiiFindingKind::SensitiveNumericPattern,
                                location: crate::words::format("medical.pii.location.text", &[("no", &slide_no)]),
                                detail: reason,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(wb) = &presentation.workbook {
        for sheet_ref in &wb.sheets {
            if sheet_ref.state != SheetState::Visible {
                report.hidden_sheets += 1;
                report.findings.push(PiiFinding {
                    kind: PiiFindingKind::HiddenWorksheet,
                    location: crate::words::format("medical.pii.location.hidden-sheet", &[("name", &sheet_ref.name)]),
                    detail: crate::words::text("medical.pii.detail.hidden-sheet"),
                });
            }
        }
    }

    report
}

/// Remove hidden slides, speaker notes, slide comments, and hidden worksheets
/// in memory before saving/sharing externally.
pub fn sanitize_presentation_pii(presentation: &mut Presentation) -> PiiSanitizeSummary {
    let mut summary = PiiSanitizeSummary::default();

    for slide in &mut presentation.slides {
        if !slide.notes.is_empty() {
            slide.notes.clear();
            summary.cleared_notes += 1;
        }
        if !slide.comments.is_empty() {
            summary.removed_comments += slide.comments.len();
            slide.comments.clear();
        }
    }

    let before_slides = presentation.slides.len();
    presentation.slides.retain(|s| !s.hidden);
    summary.removed_hidden_slides = before_slides - presentation.slides.len();

    if let Some(wb_arc) = presentation.workbook.as_mut() {
        let wb = Arc::make_mut(wb_arc);
        let before_sheets = wb.sheets.len();
        let removed_slide_ids: Vec<_> = wb
            .sheets
            .iter()
            .filter(|s| s.state != SheetState::Visible)
            .filter_map(|s| s.slide)
            .collect();
        wb.sheets.retain(|s| s.state == SheetState::Visible);
        summary.removed_hidden_sheets = before_sheets - wb.sheets.len();
        if !removed_slide_ids.is_empty() {
            presentation
                .slides
                .retain(|s| !removed_slide_ids.contains(&s.id));
        }
    }

    summary
}

fn detect_sensitive_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let digits_only: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits_only.len() == 12 && trimmed.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return Some(crate::words::text("medical.pii.detail.personal-number"));
    }
    if (trimmed.contains("患者ID") || trimmed.contains("被保険者番号"))
        && digits_only.len() >= 6
    {
        return Some(crate::words::text("medical.pii.detail.patient-id"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspects_and_sanitizes_hidden_slides_notes_and_pii_patterns() {
        let blank = crate::blank::blank_file(crate::Kind::Presentation).unwrap();
        let mut opened = crate::save::open(blank).unwrap();

        // Add notes and a hidden slide
        opened.presentation.slides[0].notes = "患者ID: 12345678".to_string();
        let mut hidden_slide = ibis_doc_model::Slide::new(opened.presentation.new_slide_id());
        hidden_slide.hidden = true;
        opened.presentation.slides.push(hidden_slide);

        let report = inspect_presentation_pii(&opened.presentation);
        assert!(!report.is_clean());
        assert_eq!(report.hidden_slides, 1);
        assert_eq!(report.notes_count, 1);

        let summary = sanitize_presentation_pii(&mut opened.presentation);
        assert_eq!(summary.removed_hidden_slides, 1);
        assert_eq!(summary.cleared_notes, 1);

        let after = inspect_presentation_pii(&opened.presentation);
        assert!(after.is_clean());
    }
}
