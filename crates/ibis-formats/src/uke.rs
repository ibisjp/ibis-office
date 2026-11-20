// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/246)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pure-Rust Japanese Electronic Receipt (`RECEIPTC.UKE` / `.uke`) parser and
//! leading-zero-safe workbook builder for ibisOffice.
//!
//! Variable-length medical receipt CSVs use Shift_JIS (CP932) or UTF-8 and
//! identify each row by a 2-letter record tag (`IR`, `RE`, `HO`, `KO`, `SY`,
//! `SI`, `IY`, `TO`, `CO`, `GO`, etc.). Opening them through standard CSV
//! type inference strips leading zeros from insurer numbers (`06130012`),
//! facility codes (`0123456`), and procedure/drug codes. This module keeps all
//! identifiers and dates as text, resolves optional locally-cached master
//! tables without bundling external master files, and builds a two-sheet
//! inspection workbook (`レセプト明細` and `レセプトサマリー`) with point
//! cross-checks.

use std::collections::HashMap;
use std::sync::Arc;

use ibis_doc_model::{
    CellKind, CellStyle, CellValue, EmuRect, Shape, ShapeKind, Sheet, SheetCell, SheetState, Slide,
    WorkbookSheet,
};

use crate::{Error, Opened};

/// Known 2-letter record tags in Japanese electronic receipt (`RECEIPTC.UKE`) files.
const KNOWN_TAGS: &[&str] = &[
    "IR", "RS", "RE", "HO", "KO", "KH", "SN", "JD", "MF", "SY", "SI", "IY", "TO", "CO", "SJ",
    "RI", "RO", "GO", "SK", "MK", "ZK", "SH", "SI", "CZ", "UK",
];

/// The words of a receipt record tag (what the screen's language says for it).
pub fn record_label(tag: &str) -> String {
    crate::words::text(record_id(tag))
}

fn record_id(tag: &str) -> &'static str {
    match tag {
        "IR" => "medical.uke.record.information",
        "RS" => "medical.uke.record.common-dental",
        "RE" => "medical.uke.record.common",
        "HO" => "medical.uke.record.insurer",
        "KO" => "medical.uke.record.public",
        "KH" => "medical.uke.record.public-extra",
        "SN" => "medical.uke.record.eligibility",
        "JD" => "medical.uke.record.visit-date",
        "MF" => "medical.uke.record.copay",
        "SY" => "medical.uke.record.diagnosis",
        "SI" => "medical.uke.record.procedure",
        "IY" => "medical.uke.record.medicine",
        "TO" => "medical.uke.record.material",
        "CO" => "medical.uke.record.comment",
        "SJ" => "medical.uke.record.symptoms",
        "RI" => "medical.uke.record.inpatient",
        "RO" => "medical.uke.record.elderly",
        "CZ" => "medical.uke.record.dispensing",
        "GO" => "medical.uke.record.claim-total",
        "UK" => "medical.uke.record.reception",
        _ => "medical.uke.record.other",
    }
}

/// Optional locally-cached medical master lookup table (never bundled in repo).
///
/// Users or hospital IT admins can populate this at runtime from official
/// ministry/payment-fund CSVs cached on their local machine.
#[derive(Debug, Clone, Default)]
pub struct MedicalMasterIndex {
    /// Procedure / drug / material / disease code -> display name.
    pub names: HashMap<String, String>,
    /// Optional standard points per code.
    pub points: HashMap<String, f64>,
}

impl MedicalMasterIndex {
    /// Parse a simple UTF-8/Shift_JIS 2-or-3-column master CSV (`code,name[,points]`).
    pub fn from_csv_bytes(data: &[u8]) -> Result<Self, Error> {
        let text = crate::csv::decode(data)?;
        let mut index = Self::default();
        for rec in crate::csv::parse_with_delimiter(&text, ',') {
            let Some(code) = rec.first().map(|s| s.trim()).filter(|s| !s.is_empty()) else {
                continue;
            };
            let Some(name) = rec.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()) else {
                continue;
            };
            index.names.insert(code.to_string(), name.to_string());
            if let Some(pts) = rec.get(2).and_then(|s| s.trim().parse::<f64>().ok()) {
                index.points.insert(code.to_string(), pts);
            }
        }
        Ok(index)
    }

    /// Resolve a code to its master display name if present.
    pub fn resolve_name(&self, code: &str) -> Option<&str> {
        self.names.get(code.trim()).map(String::as_str)
    }
}

/// Returns `true` if `data` looks like a Japanese `RECEIPTC.UKE` variable-length receipt CSV.
///
/// Requires:
/// - non-ZIP text decodable as UTF-8 or Shift_JIS,
/// - first non-empty record begins with `IR` (医療機関情報) or `UK` and has >= 6 fields,
/// - at least one subsequent receipt record (`RE`, `HO`, `SY`, `SI`, `IY`, `TO`, `GO`, etc.).
pub fn is_uke(data: &[u8]) -> bool {
    if !crate::csv::is_csv(data) || data.is_empty() {
        return false;
    }
    let Ok(text) = crate::csv::decode(data) else {
        return false;
    };
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let Some(first) = lines.next() else {
        return false;
    };
    let first_fields: Vec<&str> = first.split(',').collect();
    if first_fields.len() < 6 {
        return false;
    }
    let head = first_fields[0].trim().trim_matches('"');
    if head != "IR" && head != "UK" {
        return false;
    }
    let mut saw_receipt_tag = false;
    for line in lines.take(32) {
        let Some(tag) = line.split(',').next() else {
            continue;
        };
        let tag = tag.trim().trim_matches('"');
        if !KNOWN_TAGS.contains(&tag) {
            return false;
        }
        if matches!(tag, "RE" | "HO" | "KO" | "SY" | "SI" | "IY" | "TO" | "GO") {
            saw_receipt_tag = true;
        }
    }
    saw_receipt_tag
}

/// Summary of one parsed receipt (`RE` block) in a UKE file.
#[derive(Debug, Clone, PartialEq)]
pub struct ReceiptSummary {
    pub receipt_no: String,
    pub patient_name: String,
    pub sex_code: String,
    pub birth_date: String,
    pub billing_ym: String,
    pub insurer_no: String,
    pub reported_points: Option<f64>,
    pub computed_points: f64,
    pub disease_count: usize,
    pub item_count: usize,
}

/// Parsed UKE document containing raw rows and per-receipt summaries.
#[derive(Debug, Clone)]
pub struct UkeDocument {
    pub facility_code: String,
    pub facility_name: String,
    pub billing_ym: String,
    pub rows: Vec<Vec<String>>,
    pub summaries: Vec<ReceiptSummary>,
    pub go_total_count: Option<usize>,
    pub go_total_points: Option<f64>,
}

/// Parse `RECEIPTC.UKE` bytes into a structured [`UkeDocument`].
pub fn parse_uke(data: &[u8]) -> Result<UkeDocument, Error> {
    let text = crate::csv::decode(data)?;
    let rows = crate::csv::parse_with_delimiter(&text, ',');
    if rows.is_empty() {
        return Err(Error::new("UKE ファイルが空です"));
    }

    let mut facility_code = String::new();
    let mut facility_name = String::new();
    let mut billing_ym = String::new();
    let mut summaries: Vec<ReceiptSummary> = Vec::new();
    let mut current: Option<ReceiptSummary> = None;
    let mut go_total_count = None;
    let mut go_total_points = None;

    for row in &rows {
        let tag = row.first().map(|s| s.trim()).unwrap_or("");
        match tag {
            "IR" => {
                // IR,審査支払機関,都道府県,点数表,医療機関コード,予備,医療機関名称,請求年月,...
                facility_code = field(row, 4);
                facility_name = field(row, 6);
                if billing_ym.is_empty() {
                    billing_ym = field(row, 7);
                }
            }
            "RE" => {
                if let Some(prev) = current.take() {
                    summaries.push(prev);
                }
                // RE,レセプト番号,レセプト種別,診療年月,氏名,男女区分,生年月日,...
                let ym = field(row, 3);
                if billing_ym.is_empty() && !ym.is_empty() {
                    billing_ym = ym.clone();
                }
                current = Some(ReceiptSummary {
                    receipt_no: field(row, 1),
                    patient_name: field(row, 4),
                    sex_code: field(row, 5),
                    birth_date: field(row, 6),
                    billing_ym: ym,
                    insurer_no: String::new(),
                    reported_points: None,
                    computed_points: 0.0,
                    disease_count: 0,
                    item_count: 0,
                });
            }
            "HO" => {
                // HO,保険者番号,被保険者証記号,被保険者証番号,診療実日数,合計点数,...
                if let Some(cur) = current.as_mut() {
                    if cur.insurer_no.is_empty() {
                        cur.insurer_no = field(row, 1);
                    }
                    if let Some(pts) = parse_f64_opt(row.get(5)) {
                        cur.reported_points = Some(cur.reported_points.unwrap_or(0.0) + pts);
                    }
                }
            }
            "SY" => {
                if let Some(cur) = current.as_mut() {
                    cur.disease_count += 1;
                }
            }
            "SI" | "IY" | "TO" => {
                // SI/IY/TO: ...,コード(col 3),数量(col 4),点数(col 5),回数(col 6)
                if let Some(cur) = current.as_mut() {
                    cur.item_count += 1;
                    let pts = parse_f64_opt(row.get(5)).unwrap_or(0.0);
                    let times = parse_f64_opt(row.get(6)).unwrap_or(1.0);
                    cur.computed_points += pts * times;
                }
            }
            "GO" => {
                // GO,総件数,総合計点数,マルチボリューム識別情報
                go_total_count = field(row, 1).parse::<usize>().ok();
                go_total_points = parse_f64_opt(row.get(2));
            }
            _ => {}
        }
    }
    if let Some(last) = current.take() {
        summaries.push(last);
    }

    Ok(UkeDocument {
        facility_code,
        facility_name,
        billing_ym,
        rows,
        summaries,
        go_total_count,
        go_total_points,
    })
}

/// Open a `RECEIPTC.UKE` byte buffer as an ibisOffice two-sheet [`Opened`] workbook
/// without external master data.
pub fn open_uke(data: &[u8]) -> Result<Opened, Error> {
    open_uke_with_master(data, &MedicalMasterIndex::default())
}

/// Open a `RECEIPTC.UKE` byte buffer using an optional locally-cached [`MedicalMasterIndex`].
pub fn open_uke_with_master(
    data: &[u8],
    master: &MedicalMasterIndex,
) -> Result<Opened, Error> {
    let doc = parse_uke(data)?;
    let blank = crate::blank::blank_file(crate::Kind::Workbook)?;
    let mut opened = crate::save::open(blank)?;

    let detail_cells = build_detail_cells(&doc, master);
    let summary_cells = build_summary_cells(&doc);

    // Populate first sheet: レセプト明細
    if let Some(wb_arc) = opened.presentation.workbook.as_mut()
        && let Some(first_meta) = Arc::make_mut(wb_arc).sheets.first_mut()
    {
        first_meta.name = crate::words::text("medical.uke.sheet-detail");
    }
    if let Some(slide) = opened.presentation.slides.first_mut() {
        attach_cells_to_slide(slide, detail_cells);
    }

    // Add second sheet: レセプトサマリー
    let summary_slide_id = opened.presentation.new_slide_id();
    let template_sheet = opened
        .presentation
        .slides
        .first()
        .and_then(|s| {
            s.shapes.iter().find_map(|sh| match &sh.kind {
                ShapeKind::Sheet(sheet) => Some((**sheet).clone()),
                _ => None,
            })
        })
        .unwrap_or_default();

    let mut summary_sheet = Sheet {
        cells: summary_cells.into(),
        styles: template_sheet.styles.clone(),
        ..Default::default()
    };
    ensure_bold_style(&mut summary_sheet);

    let mut summary_slide = Slide::new(summary_slide_id);
    let template_frame = opened
        .presentation
        .slides
        .first()
        .and_then(|s| s.shapes.first())
        .map(|sh| sh.frame)
        .unwrap_or_else(|| EmuRect::new(0, 0, 0, 0));
    summary_slide.shapes.push(Shape::new(
        opened.presentation.new_shape_id(),
        template_frame,
        ShapeKind::Sheet(Arc::new(summary_sheet)),
    ));
    opened.presentation.slides.push(summary_slide);
    if let Some(wb_arc) = opened.presentation.workbook.as_mut() {
        Arc::make_mut(wb_arc).sheets.push(WorkbookSheet {
            name: crate::words::text("medical.uke.sheet-summary"),
            state: SheetState::Visible,
            slide: Some(summary_slide_id),
            hidden: None,
            other: false,
        });
    }

    Ok(opened)
}

fn attach_cells_to_slide(slide: &mut Slide, cells: Vec<SheetCell>) {
    for sh in &mut slide.shapes {
        if let ShapeKind::Sheet(sheet_arc) = &mut sh.kind {
            let mut sheet = (**sheet_arc).clone();
            ensure_bold_style(&mut sheet);
            sheet.cells = cells.into();
            *sheet_arc = Arc::new(sheet);
            break;
        }
    }
}

fn ensure_bold_style(sheet: &mut Sheet) {
    if sheet.styles.len() < 2 {
        let base = sheet.styles.first().cloned().unwrap_or_default();
        if sheet.styles.is_empty() {
            sheet.styles.push(base.clone());
        }
        sheet.styles.push(CellStyle { bold: true, ..base });
    }
}

fn build_detail_cells(doc: &UkeDocument, master: &MedicalMasterIndex) -> Vec<SheetCell> {
    let mut cells = Vec::new();
    let headers = ["line", "kind", "record", "receipt-no", "main-code", "master-name", "points", "count", "raw"];
    for (c, h) in headers.iter().enumerate() {
        cells.push(text_cell(0, c as u32, &crate::words::text(&format!("medical.uke.detail.{h}")), 1));
    }

    let mut cur_receipt = String::new();
    for (idx, row) in doc.rows.iter().enumerate() {
        let r = (idx + 1) as u32;
        let tag = row.first().map(|s| s.trim()).unwrap_or("");
        if tag == "RE" {
            cur_receipt = field(row, 1);
        }
        let code = match tag {
            "IR" => field(row, 4),
            "HO" => field(row, 1),
            "SY" => field(row, 1),
            "SI" | "IY" | "TO" | "CO" => field(row, 3),
            _ => String::new(),
        };
        let resolved = master.resolve_name(&code).unwrap_or("");
        let points_val = match tag {
            "HO" => parse_f64_opt(row.get(5)),
            "SI" | "IY" | "TO" => parse_f64_opt(row.get(5)),
            "GO" => parse_f64_opt(row.get(2)),
            _ => None,
        };
        let times_val = match tag {
            "SI" | "IY" | "TO" => parse_f64_opt(row.get(6)),
            _ => None,
        };

        cells.push(num_cell(r, 0, (idx + 1) as f64, 0));
        cells.push(text_cell(r, 1, tag, 0));
        cells.push(text_cell(r, 2, &record_label(tag), 0));
        if !cur_receipt.is_empty() {
            cells.push(text_cell(r, 3, &cur_receipt, 0));
        }
        if !code.is_empty() {
            cells.push(text_cell(r, 4, &code, 0));
        }
        if !resolved.is_empty() {
            cells.push(text_cell(r, 5, resolved, 0));
        }
        if let Some(p) = points_val {
            cells.push(num_cell(r, 6, p, 0));
        }
        if let Some(t) = times_val {
            cells.push(num_cell(r, 7, t, 0));
        }

        // Preserve all raw fields as text so leading zeros are never lost.
        for (fi, raw) in row.iter().enumerate() {
            if !raw.is_empty() {
                cells.push(text_cell(r, (8 + fi) as u32, raw, 0));
            }
        }
    }
    cells
}

fn build_summary_cells(doc: &UkeDocument) -> Vec<SheetCell> {
    let mut cells = Vec::new();
    // Top metadata block
    cells.push(text_cell(0, 0, &crate::words::text("medical.uke.summary.facility-code"), 1));
    cells.push(text_cell(0, 1, &doc.facility_code, 0));
    cells.push(text_cell(0, 2, &crate::words::text("medical.uke.summary.facility-name"), 1));
    cells.push(text_cell(0, 3, &doc.facility_name, 0));
    cells.push(text_cell(0, 4, &crate::words::text("medical.uke.summary.billing-month"), 1));
    cells.push(text_cell(0, 5, &doc.billing_ym, 0));

    let headers = ["receipt-no", "patient", "sex", "birth", "insurer-no", "claimed", "computed", "difference", "diseases", "items"];
    let header_row = 2u32;
    for (c, h) in headers.iter().enumerate() {
        cells.push(text_cell(header_row, c as u32, &crate::words::text(&format!("medical.uke.summary.{h}")), 1));
    }

    let mut total_reported = 0.0f64;
    let mut total_computed = 0.0f64;
    for (i, s) in doc.summaries.iter().enumerate() {
        let r = header_row + 1 + i as u32;
        cells.push(text_cell(r, 0, &s.receipt_no, 0));
        cells.push(text_cell(r, 1, &s.patient_name, 0));
        cells.push(text_cell(r, 2, &s.sex_code, 0));
        cells.push(text_cell(r, 3, &s.birth_date, 0));
        cells.push(text_cell(r, 4, &s.insurer_no, 0));
        let rep = s.reported_points.unwrap_or(0.0);
        total_reported += rep;
        total_computed += s.computed_points;
        cells.push(num_cell(r, 5, rep, 0));
        cells.push(num_cell(r, 6, s.computed_points, 0));
        cells.push(num_cell(r, 7, rep - s.computed_points, 0));
        cells.push(num_cell(r, 8, s.disease_count as f64, 0));
        cells.push(num_cell(r, 9, s.item_count as f64, 0));
    }

    let total_row = header_row + 1 + doc.summaries.len() as u32;
    cells.push(text_cell(total_row, 0, &crate::words::text("medical.uke.summary.total"), 1));
    cells.push(num_cell(total_row, 5, total_reported, 1));
    cells.push(num_cell(total_row, 6, total_computed, 1));
    cells.push(num_cell(total_row, 7, total_reported - total_computed, 1));
    if let Some(go_pts) = doc.go_total_points {
        cells.push(text_cell(total_row + 1, 0, &crate::words::text("medical.uke.summary.go-total"), 1));
        cells.push(num_cell(total_row + 1, 5, go_pts, 0));
        cells.push(num_cell(total_row + 1, 7, go_pts - total_computed, 0));
    }

    cells
}

fn field(row: &[String], idx: usize) -> String {
    row.get(idx).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn parse_f64_opt(s: Option<&String>) -> Option<f64> {
    s.map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .and_then(|v| v.parse::<f64>().ok())
}

fn text_cell(row: u32, column: u32, s: &str, style: u32) -> SheetCell {
    let val = CellValue::Text(s.to_string());
    SheetCell {
        row,
        column,
        kind: CellKind::of(&val),
        value: val,
        text: s.to_string(),
        style,
        formula: None,
        phonetic: None,
        runs: None,
    }
}

fn num_cell(row: u32, column: u32, n: f64, style: u32) -> SheetCell {
    let val = CellValue::Number(n);
    let text = if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    };
    SheetCell {
        row,
        column,
        kind: CellKind::of(&val),
        value: val,
        text,
        style,
        formula: None,
        phonetic: None,
        runs: None,
    }
}

    #[cfg(test)]
mod tests {
    use super::*;
    use ibis_doc_model::Presentation;

    const SYNTHETIC_UKE: &str = "\
IR,1,13,1,0123456,,テスト診療所,50810,03-0000-0000
RE,00001,1112,50810,山田　太郎,1,4550512,,100
HO,06130012,テスト,12345,1,392
SY,8830001,5080101,1,,01,急性上気道炎
SI,11,1,111000110,1,291,1
IY,21,1,620000001,3,33,3
CO,81,1,810000001,経過観察
GO,1,392,0
";

    fn sheet_by_index(pres: &Presentation, idx: usize) -> Sheet {
        pres.slides[idx]
            .shapes
            .iter()
            .find_map(|sh| match &sh.kind {
                ShapeKind::Sheet(s) => Some((**s).clone()),
                _ => None,
            })
            .expect("sheet")
    }

    fn cell_text(sheet: &Sheet, row: u32, col: u32) -> Option<String> {
        sheet
            .cells
            .iter()
            .find(|c| c.row == row && c.column == col)
            .map(|c| c.text.clone())
    }

    #[test]
    fn detects_and_preserves_leading_zeros_in_uke() {
        assert!(is_uke(SYNTHETIC_UKE.as_bytes()));
        assert!(!is_uke(b"a,b,c\n1,2,3\n"));

        let sjis = crate::csv::encode_sjis(SYNTHETIC_UKE);
        assert!(is_uke(&sjis));

        let opened = open_uke(&sjis).expect("open_uke");
        let wb = opened.presentation.workbook.as_ref().expect("workbook");
        assert_eq!(wb.sheets.len(), 2);
        assert_eq!(wb.sheets[0].name, "レセプト明細");
        assert_eq!(wb.sheets[1].name, "レセプトサマリー");

        let detail = sheet_by_index(&opened.presentation, 0);
        // Row 1 is IR: facility code 0123456 must keep its leading zero!
        assert_eq!(cell_text(&detail, 1, 4).as_deref(), Some("0123456"));
        // Row 3 is HO: insurer_no 06130012 must keep its leading zero!
        assert_eq!(cell_text(&detail, 3, 4).as_deref(), Some("06130012"));

        let summary = sheet_by_index(&opened.presentation, 1);
        assert_eq!(cell_text(&summary, 0, 1).as_deref(), Some("0123456"));
        assert_eq!(cell_text(&summary, 3, 0).as_deref(), Some("00001"));
        assert_eq!(cell_text(&summary, 3, 4).as_deref(), Some("06130012"));
        // 291 * 1 + 33 * 3 = 390; reported = 392 => diff = 2
        assert_eq!(cell_text(&summary, 3, 5).as_deref(), Some("392"));
        assert_eq!(cell_text(&summary, 3, 6).as_deref(), Some("390"));
        assert_eq!(cell_text(&summary, 3, 7).as_deref(), Some("2"));
    }

    #[test]
    fn resolves_optional_external_master_codes() {
        let master_csv = "111000110,初診料,291\n620000001,テスト錠１０ｍｇ,33\n";
        let master = MedicalMasterIndex::from_csv_bytes(master_csv.as_bytes()).unwrap();
        let opened = open_uke_with_master(SYNTHETIC_UKE.as_bytes(), &master).unwrap();
        let detail = sheet_by_index(&opened.presentation, 0);
        // Row 5 is SI, Row 6 is IY
        assert_eq!(cell_text(&detail, 5, 5).as_deref(), Some("初診料"));
        assert_eq!(cell_text(&detail, 6, 5).as_deref(), Some("テスト錠１０ｍｇ"));
    }
}
