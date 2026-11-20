// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Open and Repair subsystem for damaged or corrupted Excel (.xlsx) workbooks.
//!
//! Provides structural salvaging (local header scan, XML sanitization, unclosed
//! tag balancing, missing part synthesis) and data extraction modes (Recover
//! Formulas, Convert to Values) complying with Excel's "開いて修復" workflow.

use std::collections::HashMap;
use std::sync::Arc;

use ibis_doc_model::Presentation;
use ibis_ooxml::zip::{EntryMeta, Writer};

use crate::{Error, Source};

/// Repair execution mode (Office's 開いて修復 options).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RepairMode {
    /// できる限りのデータ・書式・数式・段落を修復して開く（推奨）
    #[default]
    Repair,
    /// データの抽出: 数式を回復（数式と値を抽出: Excel 標準）
    ExtractFormulas,
    /// データの抽出: 値に変換（数式を計算済みの値に変換・除去して抽出: Excel 標準）
    ExtractValues,
    /// テキストの回復（すべてのファイルからテキストを回復 / Recover Text from Any File: Word 標準）
    RecoverText,
}

impl RepairMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Repair => "repair",
            Self::ExtractFormulas => "extract_formulas",
            Self::ExtractValues => "extract_values",
            Self::RecoverText => "recover_text",
        }
    }
}

/// Severity / classification of a log entry during repair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairLogLevel {
    Info,
    Warning,
    Repaired,
    Removed,
}

impl RepairLogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARN",
            Self::Repaired => "REPAIRED",
            Self::Removed => "REMOVED",
        }
    }
}

/// A single logged event during workbook repair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepairLogEntry {
    pub level: RepairLogLevel,
    pub message: String,
}

/// Detailed report generated after repair.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RepairReport {
    pub mode: RepairMode,
    pub success: bool,
    pub logs: Vec<RepairLogEntry>,
    pub sheets_recovered: usize,
    pub cells_recovered: usize,
    pub formulas_recovered: usize,
    pub formulas_converted_to_values: usize,
    pub paragraphs_recovered: usize,
    pub runs_recovered: usize,
    pub tables_recovered: usize,
    pub parts_recovered: usize,
    pub parts_reconstructed: usize,
}

impl RepairReport {
    pub fn summary_text(&self) -> String {
        if self.paragraphs_recovered > 0 {
            format!(
                "修復完了: 段落 {} 個、表 {} 個復旧（再構築パーツ {} 件）",
                self.paragraphs_recovered, self.tables_recovered, self.parts_reconstructed
            )
        } else {
            format!(
                "修復完了: シート {} 枚、セル {} 個、数式 {} 個復旧（再構築パーツ {} 件）",
                self.sheets_recovered, self.cells_recovered, self.formulas_recovered, self.parts_reconstructed
            )
        }
    }
}

/// Result of a successful repair operation.
pub struct RepairedWorkbook {
    pub presentation: Presentation,
    pub source: Source,
    pub repaired_bytes: Vec<u8>,
    pub report: RepairReport,
}

/// Repair or extract data from corrupted Excel (.xlsx) file bytes.
pub fn repair_workbook(data: &[u8], mode: RepairMode) -> Result<RepairedWorkbook, Error> {
    let mut logs = Vec::new();
    let mut parts_reconstructed = 0;
    let mut formulas_converted = 0;

    logs.push(RepairLogEntry {
        level: RepairLogLevel::Info,
        message: format!("破損ブックの修復処理を開始しました（モード: {:?}）", mode),
    });

    // 1. Raw extraction: try standard archive, fallback to local headers scan.
    let mut parts = extract_parts(data, &mut logs);
    if parts.is_empty() {
        return Err(Error::new("修復可能なデータ（ワークシートまたはパッケージ）が見つかりませんでした"));
    }

    let parts_recovered = parts.len();

    // 2. XML sanitization & tag balancing on all XML parts.
    let part_names: Vec<String> = parts.keys().cloned().collect();
    for name in part_names {
        if name.ends_with(".xml") || name.ends_with(".rels") {
            if let Some(raw_bytes) = parts.remove(&name) {
                let sanitized = sanitize_xml_bytes(&raw_bytes, &name, &mut logs);
                let text = match String::from_utf8(sanitized) {
                    Ok(t) => t,
                    Err(e) => {
                        logs.push(RepairLogEntry {
                            level: RepairLogLevel::Warning,
                            message: format!("パーツ '{}' は非 UTF-8 バイトを含んでいたため損失許容デコードを行いました: {}", name, e),
                        });
                        String::from_utf8_lossy(&e.into_bytes()).into_owned()
                    }
                };
                let repaired_text = repair_xml_syntax(&text, &name, &mut logs);
                parts.insert(name, repaired_text.into_bytes());
            }
        }
    }

    // 3. Find and inventory worksheets.
    let mut sheet_parts: Vec<String> = parts
        .keys()
        .filter(|k| k.starts_with("xl/worksheets/sheet") && k.ends_with(".xml"))
        .cloned()
        .collect();
    sheet_parts.sort();

    // If no standard sheet parts found, check if any XML part looks like a sheet.
    if sheet_parts.is_empty() {
        for (name, content) in &parts {
            if let Ok(text) = std::str::from_utf8(content) {
                if text.contains("<sheetData") || text.contains("<worksheet") {
                    let fallback_name = "xl/worksheets/sheet1.xml".to_string();
                    logs.push(RepairLogEntry {
                        level: RepairLogLevel::Repaired,
                        message: format!("シート様パーツ '{}' を標準パス '{}' に再配置しました", name, fallback_name),
                    });
                    sheet_parts.push(fallback_name);
                    break;
                }
            }
        }
        if !sheet_parts.is_empty() {
            let src = parts.keys().find(|k| {
                if let Ok(text) = std::str::from_utf8(&parts[*k]) {
                    text.contains("<sheetData") || text.contains("<worksheet")
                } else {
                    false
                }
            }).cloned().unwrap();
            let bytes = parts.remove(&src).unwrap();
            parts.insert("xl/worksheets/sheet1.xml".to_string(), bytes);
        }
    }

    // If still no sheets, synthesize an empty sheet.
    if sheet_parts.is_empty() {
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Warning,
            message: "ワークシートが存在しないため、空のワークシートを作成しました".to_string(),
        });
        parts.insert(
            "xl/worksheets/sheet1.xml".to_string(),
            b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData/></worksheet>".to_vec(),
        );
        sheet_parts.push("xl/worksheets/sheet1.xml".to_string());
        parts_reconstructed += 1;
    }

    // 4. Mode-specific adjustments on sheets.
    if mode == RepairMode::ExtractValues {
        for sheet_name in &sheet_parts {
            if let Some(bytes) = parts.remove(sheet_name) {
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    let stripped = strip_formulas_from_sheet(text, &mut formulas_converted);
                    parts.insert(sheet_name.clone(), stripped.into_bytes());
                } else {
                    parts.insert(sheet_name.clone(), bytes);
                }
            }
        }
        if formulas_converted > 0 {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Repaired,
                message: format!("モード【値に変換】により {} 個の数式を静的な値に変換しました", formulas_converted),
            });
        }
    }

    // 5. Ensure xl/workbook.xml exists and references our sheets.
    let mut need_workbook = true;
    if let Some(wb_bytes) = parts.get("xl/workbook.xml") {
        if let Ok(text) = std::str::from_utf8(wb_bytes) {
            if roxmltree::Document::parse(text).is_ok() && text.contains("<sheet") {
                need_workbook = false;
            }
        }
    }
    if need_workbook {
        let mut wb_xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\n  <sheets>\n");
        for (idx, _) in sheet_parts.iter().enumerate() {
            let sheet_name = format!("Sheet{}", idx + 1);
            let r_id = format!("rId{}", idx + 1);
            wb_xml.push_str(&format!("    <sheet name=\"{}\" sheetId=\"{}\" r:id=\"{}\"/>\n", sheet_name, idx + 1, r_id));
        }
        wb_xml.push_str("  </sheets>\n</workbook>\n");
        parts.insert("xl/workbook.xml".to_string(), wb_xml.into_bytes());
        parts_reconstructed += 1;
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: format!("欠落または破損していた xl/workbook.xml を {} 枚のシートで再構築しました", sheet_parts.len()),
        });
    }

    // 6. Ensure xl/_rels/workbook.xml.rels exists.
    let mut need_wb_rels = true;
    if let Some(wb_rels_bytes) = parts.get("xl/_rels/workbook.xml.rels") {
        if let Ok(text) = std::str::from_utf8(wb_rels_bytes) {
            if roxmltree::Document::parse(text).is_ok() {
                need_wb_rels = false;
            }
        }
    }
    if need_wb_rels {
        let mut rels_xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\n");
        for (idx, sheet_name) in sheet_parts.iter().enumerate() {
            let r_id = format!("rId{}", idx + 1);
            let target = sheet_name.strip_prefix("xl/").unwrap_or(sheet_name);
            rels_xml.push_str(&format!("  <Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"{}\"/>\n", r_id, target));
        }
        let mut next_id = sheet_parts.len() + 1;
        if parts.contains_key("xl/styles.xml") {
            rels_xml.push_str(&format!("  <Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>\n", next_id));
            next_id += 1;
        }
        if parts.contains_key("xl/sharedStrings.xml") {
            rels_xml.push_str(&format!("  <Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/>\n", next_id));
        }
        rels_xml.push_str("</Relationships>\n");
        parts.insert("xl/_rels/workbook.xml.rels".to_string(), rels_xml.into_bytes());
        parts_reconstructed += 1;
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: "欠落していた xl/_rels/workbook.xml.rels のリレーションシップを再構築しました".to_string(),
        });
    }

    // 7. Ensure _rels/.rels exists.
    let mut need_root_rels = true;
    if let Some(rels_bytes) = parts.get("_rels/.rels") {
        if let Ok(text) = std::str::from_utf8(rels_bytes) {
            if roxmltree::Document::parse(text).is_ok() {
                need_root_rels = false;
            }
        }
    }
    if need_root_rels {
        let rels_xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\n  <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\n</Relationships>\n";
        parts.insert("_rels/.rels".to_string(), rels_xml.as_bytes().to_vec());
        parts_reconstructed += 1;
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: "欠落していた _rels/.rels を再構築しました".to_string(),
        });
    }

    // 8. Ensure [Content_Types].xml exists.
    let mut need_ct = true;
    if let Some(ct_bytes) = parts.get("[Content_Types].xml") {
        if let Ok(text) = std::str::from_utf8(ct_bytes) {
            if roxmltree::Document::parse(text).is_ok() && text.contains("spreadsheetml") {
                need_ct = false;
            }
        }
    }
    if need_ct {
        let mut ct_xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\n  <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\n  <Default Extension=\"xml\" ContentType=\"application/xml\"/>\n  <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>\n");
        for sheet_name in &sheet_parts {
            let part_name = if sheet_name.starts_with('/') { sheet_name.clone() } else { format!("/{}", sheet_name) };
            ct_xml.push_str(&format!("  <Override PartName=\"{}\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>\n", part_name));
        }
        if parts.contains_key("xl/styles.xml") {
            ct_xml.push_str("  <Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>\n");
        }
        if parts.contains_key("xl/sharedStrings.xml") {
            ct_xml.push_str("  <Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>\n");
        }
        ct_xml.push_str("</Types>\n");
        parts.insert("[Content_Types].xml".to_string(), ct_xml.into_bytes());
        parts_reconstructed += 1;
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: "欠落していた [Content_Types].xml を再構築しました".to_string(),
        });
    }

    // 9. Re-bundle into a clean OOXML ZIP package.
    let mut writer = Writer::new();
    for (name, content) in &parts {
        if let Err(e) = writer.add(name, EntryMeta::new(), content) {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Warning,
                message: format!("パーツ '{}' の書き込みをスキップしました: {}", name, e),
            });
        }
    }

    let clean_bytes = writer.finish().map_err(|e| Error::new(format!("パッケージの再構築に失敗しました: {}", e)))?;

    // 10. Open using standard ibis_formats::open.
    let opened = crate::open(clean_bytes.clone()).map_err(|e| {
        Error::new(format!("修復後データの解析に失敗しました: {}", e))
    })?;

    // 11. Count recovered statistics.
    let sheets_recovered = opened.presentation.slides.len();
    let mut cells_recovered = 0;
    let mut formulas_recovered = 0;
    for slide in &opened.presentation.slides {
        for shape in &slide.shapes {
            if let ibis_doc_model::ShapeKind::Sheet(sheet) = &shape.kind {
                for cell in &sheet.cells {
                    cells_recovered += 1;
                    if cell.formula.is_some() {
                        formulas_recovered += 1;
                    }
                }
            }
        }
    }
    if let Some(wb) = &opened.presentation.workbook {
        for s in &wb.sheets {
            if let Some(hidden) = &s.hidden {
                for cell in &hidden.cells {
                    cells_recovered += 1;
                    if cell.formula.is_some() {
                        formulas_recovered += 1;
                    }
                }
            }
        }
    }

    logs.push(RepairLogEntry {
        level: RepairLogLevel::Info,
        message: format!(
            "修復完了: {} 枚のシート、{} 個のセル、{} 個の数式を復旧しました",
            sheets_recovered, cells_recovered, formulas_recovered
        ),
    });

    let report = RepairReport {
        mode,
        success: true,
        logs,
        sheets_recovered,
        cells_recovered,
        formulas_recovered,
        formulas_converted_to_values: formulas_converted,
        paragraphs_recovered: 0,
        runs_recovered: 0,
        tables_recovered: 0,
        parts_recovered,
        parts_reconstructed,
    };

    Ok(RepairedWorkbook {
        presentation: opened.presentation,
        source: opened.source,
        repaired_bytes: clean_bytes,
        report,
    })
}

/// Extract all recoverable parts from data.
fn extract_parts(data: &[u8], logs: &mut Vec<RepairLogEntry>) -> HashMap<String, Vec<u8>> {
    let mut parts = HashMap::new();

    // First attempt: read via standard Archive.
    if let Ok(archive) = ibis_ooxml::zip::Archive::new(Arc::from(data)) {
        for entry in archive.entries() {
            if let Ok(bytes) = archive.read(entry) {
                parts.insert(entry.name.clone(), bytes);
            }
        }
        if !parts.is_empty() {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Info,
                message: format!("ZIP セントラルディレクトリから {} 個のパーツを回収しました", parts.len()),
            });
            return parts;
        }
    }

    // Fallback: scan for local file headers (0x0403_4b50 / PK\x03\x04).
    logs.push(RepairLogEntry {
        level: RepairLogLevel::Warning,
        message: "セントラルディレクトリからパーツを回収できなかったため、ローカルヘッダー走査を実行します".to_string(),
    });

    let mut i = 0;
    while i + 30 <= data.len() {
        if &data[i..i + 4] == b"PK\x03\x04" {
            let method = u16::from_le_bytes(data[i + 8..i + 10].try_into().unwrap_or_default());
            let comp_size = u32::from_le_bytes(data[i + 18..i + 22].try_into().unwrap_or_default()) as usize;
            let uncomp_size = u32::from_le_bytes(data[i + 22..i + 26].try_into().unwrap_or_default()) as usize;
            let name_len = u16::from_le_bytes(data[i + 26..i + 28].try_into().unwrap_or_default()) as usize;
            let extra_len = u16::from_le_bytes(data[i + 28..i + 30].try_into().unwrap_or_default()) as usize;
            let name_start = i + 30;

            if name_start + name_len <= data.len() {
                if let Ok(name_str) = std::str::from_utf8(&data[name_start..name_start + name_len]) {
                    let is_plausible_name = name_str == "[Content_Types].xml"
                        || name_str.starts_with("xl/")
                        || name_str.starts_with("word/")
                        || name_str.starts_with("_rels/")
                        || name_str.starts_with("docProps/");

                    if is_plausible_name && !parts.contains_key(name_str) {
                        let data_start = name_start + name_len + extra_len;
                        if comp_size > 0 && data_start + comp_size <= data.len() {
                            let raw = &data[data_start..data_start + comp_size];
                            let decompressed = match method {
                                0 => Some(raw.to_vec()),
                                8 => miniz_oxide::inflate::decompress_to_vec_with_limit(
                                    raw,
                                    uncomp_size.max(comp_size).max(1024 * 1024 * 16),
                                ).ok(),
                                _ => None,
                            };
                            if let Some(content) = decompressed {
                                logs.push(RepairLogEntry {
                                    level: RepairLogLevel::Repaired,
                                    message: format!("ローカルヘッダー走査によりパーツ '{}' を回収しました", name_str),
                                });
                                parts.insert(name_str.to_string(), content);
                            }
                        } else if method == 8 && data_start < data.len() {
                            let max_decomp = 1024 * 1024 * 16;
                            if let Ok(content) = miniz_oxide::inflate::decompress_to_vec_with_limit(&data[data_start..], max_decomp) {
                                logs.push(RepairLogEntry {
                                    level: RepairLogLevel::Repaired,
                                    message: format!("ストリーム展開によりパーツ '{}' を回収しました", name_str),
                                });
                                parts.insert(name_str.to_string(), content);
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }

    parts
}

/// Replace invalid XML 1.0 control characters (0x00..=0x1F except \t, \n, \r) with space.
fn sanitize_xml_bytes(bytes: &[u8], name: &str, logs: &mut Vec<RepairLogEntry>) -> Vec<u8> {
    let mut modified = false;
    let mut out = Vec::with_capacity(bytes.len());
    for &b in bytes {
        if b < 0x20 && b != b'\t' && b != b'\n' && b != b'\r' {
            out.push(b' ');
            modified = true;
        } else {
            out.push(b);
        }
    }
    if modified {
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: format!("パーツ '{}' 内の不正な XML 制御文字 (0x00-0x1F) をサニタイズしました", name),
        });
    }
    out
}

/// Ensure XML string is well-formed; if truncated or unclosed, balance/close tags.
fn repair_xml_syntax(xml_str: &str, name: &str, logs: &mut Vec<RepairLogEntry>) -> String {
    if roxmltree::Document::parse(xml_str).is_ok() {
        return xml_str.to_string();
    }

    logs.push(RepairLogEntry {
        level: RepairLogLevel::Warning,
        message: format!("パーツ '{}' の XML 構文破損を検知しました。構文修復を試みます", name),
    });

    let mut repaired = xml_str.to_string();

    // 1. If ends with incomplete open tag like "<c r=\"A1\"", strip it.
    if let Some(last_lt) = repaired.rfind('<') {
        let after_lt = &repaired[last_lt..];
        if !after_lt.contains('>') {
            repaired.truncate(last_lt);
        }
    }

    // 2. Try closing blocks commonly found in SpreadsheetML and WordprocessingML.
    for block in [
        "</w:t></w:r></w:p></w:body></w:document>",
        "</w:r></w:p></w:body></w:document>",
        "</w:p></w:body></w:document>",
        "</w:tc></w:tr></w:tbl></w:body></w:document>",
        "</w:tr></w:tbl></w:body></w:document>",
        "</w:tbl></w:body></w:document>",
        "</w:body></w:document>",
        "</w:document>",
        "</w:styles>",
        "</w:numbering>",
        "</v></c></row></sheetData></worksheet>",
        "</c></row></sheetData></worksheet>",
        "</row></sheetData></worksheet>",
        "</sheetData></worksheet>",
        "</worksheet>",
        "</workbook>",
        "</Relationships>",
        "</Types>",
    ] {
        let test = format!("{}{}", repaired, block);
        if roxmltree::Document::parse(&test).is_ok() {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Repaired,
                message: format!("パーツ '{}' の構造終了タグ群を補完修復しました", name),
            });
            return test;
        }
    }

    // 3. Fallback: Tag-stack based auto-closing of unclosed tags.
    let mut stack = Vec::new();
    let mut i = 0;
    let bytes = repaired.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if i + 1 < bytes.len() && (bytes[i + 1] == b'?' || bytes[i + 1] == b'!') {
                if let Some(end) = repaired[i..].find('>') {
                    i += end + 1;
                    continue;
                }
            } else if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                let start = i + 2;
                if let Some(end) = repaired[start..].find('>') {
                    let tag_name = repaired[start..start + end].trim().to_string();
                    if let Some(pos) = stack.iter().rposition(|t| t == &tag_name) {
                        stack.truncate(pos);
                    }
                    i = start + end + 1;
                    continue;
                }
            } else {
                let start = i + 1;
                if let Some(end) = repaired[start..].find('>') {
                    let tag_content = &repaired[start..start + end];
                    if !tag_content.ends_with('/') {
                        let tag_name = tag_content.split_whitespace().next().unwrap_or("").to_string();
                        if !tag_name.is_empty() {
                            stack.push(tag_name);
                        }
                    }
                    i = start + end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }

    if !stack.is_empty() {
        let mut test = repaired.clone();
        while let Some(tag) = stack.pop() {
            test.push_str(&format!("</{}>", tag));
        }
        if roxmltree::Document::parse(&test).is_ok() {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Repaired,
                message: format!("パーツ '{}' の階層タグを自動スタック補完修復しました", name),
            });
            return test;
        }
    }

    repaired
}

/// Strip <f>...</f> formula elements from sheet XML for ExtractValues mode.
fn strip_formulas_from_sheet(sheet_xml: &str, count: &mut usize) -> String {
    let mut result = String::with_capacity(sheet_xml.len());
    let mut pos = 0;
    while let Some(start) = sheet_xml[pos..].find("<f") {
        let abs_start = pos + start;
        let next_char = sheet_xml[abs_start..].chars().nth(2);
        if next_char == Some('>') || next_char == Some(' ') || next_char == Some('/') {
            if let Some(end_tag) = sheet_xml[abs_start..].find("</f>") {
                let abs_end = abs_start + end_tag + 4;
                result.push_str(&sheet_xml[pos..abs_start]);
                pos = abs_end;
                *count += 1;
                continue;
            } else if let Some(self_close) = sheet_xml[abs_start..].find("/>") {
                let abs_end = abs_start + self_close + 2;
                result.push_str(&sheet_xml[pos..abs_start]);
                pos = abs_end;
                *count += 1;
                continue;
            }
        }
        result.push_str(&sheet_xml[pos..abs_start + 2]);
        pos = abs_start + 2;
    }
    result.push_str(&sheet_xml[pos..]);
    result
}

/// Result of a successful Word document repair operation.
pub struct RepairedDocument {
    pub presentation: Presentation,
    pub source: Source,
    pub repaired_bytes: Vec<u8>,
    pub report: RepairReport,
}

/// Repair or extract text from corrupted Word (.docx) file bytes.
pub fn repair_document(data: &[u8], mode: RepairMode) -> Result<RepairedDocument, Error> {
    let mut logs = Vec::new();
    let mut parts_reconstructed = 0;

    logs.push(RepairLogEntry {
        level: RepairLogLevel::Info,
        message: format!("破損文書の修復処理を開始しました（モード: {:?}）", mode),
    });

    // 1. Raw extraction: try standard archive, fallback to local headers scan.
    let mut parts = extract_parts(data, &mut logs);

    // If parts is empty and mode is RecoverText, try fallback raw text recovery.
    if parts.is_empty() {
        if let Some(text) = extract_raw_text_from_data(data) {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Warning,
                message: "バイナリデータから直接プレーンテキストを抽出して文書を再構築します".to_string(),
            });
            let clean_bytes = synthesize_document_from_text(&text)?;
            let opened = crate::open(clean_bytes.clone())?;
            let report = RepairReport {
                mode,
                success: true,
                logs,
                paragraphs_recovered: text.lines().filter(|l| !l.trim().is_empty()).count().max(1),
                runs_recovered: text.lines().filter(|l| !l.trim().is_empty()).count().max(1),
                parts_recovered: 0,
                parts_reconstructed: 3,
                ..Default::default()
            };
            return Ok(RepairedDocument {
                presentation: opened.presentation,
                source: opened.source,
                repaired_bytes: clean_bytes,
                report,
            });
        }
        return Err(Error::new("修復可能なデータ（文書本体またはパッケージ）が見つかりませんでした"));
    }

    let parts_recovered = parts.len();

    // 2. XML sanitization & tag balancing on all XML and rels parts.
    let part_names: Vec<String> = parts.keys().cloned().collect();
    for name in part_names {
        if name.ends_with(".xml") || name.ends_with(".rels") {
            if let Some(raw_bytes) = parts.remove(&name) {
                let sanitized = sanitize_xml_bytes(&raw_bytes, &name, &mut logs);
                let text = match String::from_utf8(sanitized) {
                    Ok(t) => t,
                    Err(e) => {
                        logs.push(RepairLogEntry {
                            level: RepairLogLevel::Warning,
                            message: format!("パーツ '{}' は非 UTF-8 バイトを含んでいたため損失許容デコードを行いました: {}", name, e),
                        });
                        String::from_utf8_lossy(&e.into_bytes()).into_owned()
                    }
                };
                let repaired_text = repair_xml_syntax(&text, &name, &mut logs);
                parts.insert(name, repaired_text.into_bytes());
            }
        }
    }

    // 3. Locate word/document.xml or plausible main document part.
    if !parts.contains_key("word/document.xml") {
        let mut found_doc = None;
        for (name, content) in &parts {
            if let Ok(text) = std::str::from_utf8(content) {
                if text.contains("<w:body") || text.contains("<w:document") {
                    found_doc = Some(name.clone());
                    break;
                }
            }
        }
        if let Some(src) = found_doc {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Repaired,
                message: format!("文書様パーツ '{}' を標準パス 'word/document.xml' に再配置しました", src),
            });
            let bytes = parts.remove(&src).unwrap();
            parts.insert("word/document.xml".to_string(), bytes);
        } else {
            // Synthesize minimal document.xml
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Warning,
                message: "文書本体 (word/document.xml) が存在しないため、最小限の本文を作成しました".to_string(),
            });
            let minimal = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>（修復された空の文書）</w:t></w:r></w:p></w:body></w:document>"#;
            parts.insert("word/document.xml".to_string(), minimal.as_bytes().to_vec());
            parts_reconstructed += 1;
        }
    }

    // 4. RecoverText mode: extract text strings and rebuild clean plain document.xml
    if mode == RepairMode::RecoverText {
        if let Some(doc_bytes) = parts.get("word/document.xml") {
            if let Ok(doc_str) = std::str::from_utf8(doc_bytes) {
                let cleaned_xml = extract_text_only_document_xml(doc_str);
                parts.insert("word/document.xml".to_string(), cleaned_xml.into_bytes());
                logs.push(RepairLogEntry {
                    level: RepairLogLevel::Info,
                    message: "テキスト回復モード: 本文からテキスト情報のみを抽出して再構築しました".to_string(),
                });
            }
        }
    }

    // 5. Synthesize required package parts if missing.
    // [Content_Types].xml
    if !parts.contains_key("[Content_Types].xml") {
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: "必須パーツ '[Content_Types].xml' を再構築しました".to_string(),
        });
        let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Default Extension="png" ContentType="image/png"/>
<Default Extension="jpeg" ContentType="image/jpeg"/>
<Default Extension="jpg" ContentType="image/jpeg"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;
        parts.insert("[Content_Types].xml".to_string(), ct.as_bytes().to_vec());
        parts_reconstructed += 1;
    }

    // _rels/.rels
    if !parts.contains_key("_rels/.rels") {
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: "必須パーツ '_rels/.rels' を再構築しました".to_string(),
        });
        let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;
        parts.insert("_rels/.rels".to_string(), rels.as_bytes().to_vec());
        parts_reconstructed += 1;
    }

    // word/_rels/document.xml.rels
    if !parts.contains_key("word/_rels/document.xml.rels") {
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: "パーツ 'word/_rels/document.xml.rels' を再構築しました".to_string(),
        });
        let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#;
        parts.insert("word/_rels/document.xml.rels".to_string(), doc_rels.as_bytes().to_vec());
        parts_reconstructed += 1;
    }

    // word/styles.xml
    if !parts.contains_key("word/styles.xml") {
        logs.push(RepairLogEntry {
            level: RepairLogLevel::Repaired,
            message: "パーツ 'word/styles.xml' を標準定義で再構築しました".to_string(),
        });
        let styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults>
<w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:eastAsia="Yu Gothic" w:hAnsi="Calibri"/><w:sz w:val="21"/></w:rPr></w:rPrDefault>
</w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal">
<w:name w:val="Normal"/>
</w:style>
</w:styles>"#;
        parts.insert("word/styles.xml".to_string(), styles.as_bytes().to_vec());
        parts_reconstructed += 1;
    }

    // 6. Repack into valid ZIP archive using ibis_ooxml::zip::Writer.
    let mut writer = Writer::new();
    let mut sorted_parts: Vec<(String, Vec<u8>)> = parts.into_iter().collect();
    sorted_parts.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, content) in sorted_parts {
        if let Err(e) = writer.add(&name, EntryMeta::new(), &content) {
            logs.push(RepairLogEntry {
                level: RepairLogLevel::Warning,
                message: format!("パーツ '{}' の書き込みをスキップしました: {}", name, e),
            });
        }
    }

    let clean_bytes = writer.finish().map_err(|e| Error::new(format!("パッケージの再構築に失敗しました: {}", e)))?;

    // 7. Open using crate::open.
    let opened = crate::open(clean_bytes.clone()).map_err(|e| {
        Error::new(format!("修復後データの解析に失敗しました: {}", e))
    })?;

    // 8. Count recovered statistics.
    let mut paragraphs_recovered = 0;
    let mut runs_recovered = 0;
    let mut tables_recovered = 0;

    if let Some(doc) = &opened.presentation.flow {
        for block in &doc.body {
            match block {
                ibis_doc_model::Block::Paragraph(p) => {
                    paragraphs_recovered += 1;
                    runs_recovered += p.runs.len();
                }
                ibis_doc_model::Block::Table(t) => {
                    tables_recovered += 1;
                    for row in &t.rows {
                        for cell in &row.cells {
                            for b in &cell.blocks {
                                if let ibis_doc_model::Block::Paragraph(p) = b {
                                    paragraphs_recovered += 1;
                                    runs_recovered += p.runs.len();
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    logs.push(RepairLogEntry {
        level: RepairLogLevel::Info,
        message: format!(
            "修復完了: 段落 {} 個、表 {} 個、テキストラン {} 個を復旧しました",
            paragraphs_recovered, tables_recovered, runs_recovered
        ),
    });

    let report = RepairReport {
        mode,
        success: true,
        logs,
        paragraphs_recovered,
        runs_recovered,
        tables_recovered,
        parts_recovered,
        parts_reconstructed,
        ..Default::default()
    };

    Ok(RepairedDocument {
        presentation: opened.presentation,
        source: opened.source,
        repaired_bytes: clean_bytes,
        report,
    })
}

/// Extract clean text from word/document.xml into minimalist paragraphs.
fn extract_text_only_document_xml(xml: &str) -> String {
    let mut out = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#);

    let mut p_count = 0;
    let mut pos = 0;
    while let Some(p_start) = xml[pos..].find("<w:p") {
        let abs_start = pos + p_start;
        let p_end = xml[abs_start..].find("</w:p>").map(|idx| abs_start + idx + 6).unwrap_or(xml.len());
        let p_content = &xml[abs_start..p_end];

        let mut t_pos = 0;
        let mut p_text = String::new();
        while let Some(t_start) = p_content[t_pos..].find("<w:t") {
            let abs_t = t_pos + t_start;
            if let Some(close_tag) = p_content[abs_t..].find('>') {
                let text_start = abs_t + close_tag + 1;
                if let Some(t_end) = p_content[text_start..].find("</w:t>") {
                    let text = &p_content[text_start..text_start + t_end];
                    p_text.push_str(text);
                    t_pos = text_start + t_end + 6;
                    continue;
                }
            }
            t_pos = abs_t + 4;
        }

        if !p_text.is_empty() {
            out.push_str("<w:p><w:r><w:t xml:space=\"preserve\">");
            out.push_str(&html_escape(&p_text));
            out.push_str("</w:t></w:r></w:p>");
            p_count += 1;
        }
        pos = p_end;
    }

    if p_count == 0 {
        out.push_str("<w:p><w:r><w:t>（テキストが検出されませんでした）</w:t></w:r></w:p>");
    }
    out.push_str("</w:body></w:document>");
    out
}

/// Extract printable text strings or XML tags from raw non-ZIP binary data.
fn extract_raw_text_from_data(data: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(data);
    if s.contains("<w:t") {
        let mut out = String::new();
        let mut pos = 0;
        while let Some(t_start) = s[pos..].find("<w:t") {
            let abs_t = pos + t_start;
            if let Some(close_tag) = s[abs_t..].find('>') {
                let text_start = abs_t + close_tag + 1;
                if let Some(t_end) = s[text_start..].find("</w:t>") {
                    let text = &s[text_start..text_start + t_end];
                    out.push_str(text);
                    out.push('\n');
                    pos = text_start + t_end + 6;
                    continue;
                }
            }
            pos = abs_t + 4;
        }
        if !out.trim().is_empty() {
            return Some(out);
        }
    }

    let mut extracted_lines = Vec::new();
    for line in s.lines() {
        let clean: String = line.chars().filter(|c| !c.is_control() || *c == '\t').collect();
        let trimmed = clean.trim();
        if !trimmed.is_empty() {
            extracted_lines.push(trimmed.to_string());
        }
    }
    if !extracted_lines.is_empty() {
        return Some(extracted_lines.join("\n"));
    }
    None
}

/// Synthesize a clean DOCX package containing the given plain text lines.
fn synthesize_document_from_text(text: &str) -> Result<Vec<u8>, Error> {
    let mut writer = Writer::new();
    let mut doc_xml = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#);
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            doc_xml.push_str("<w:p><w:r><w:t xml:space=\"preserve\">");
            doc_xml.push_str(&html_escape(trimmed));
            doc_xml.push_str("</w:t></w:r></w:p>");
        }
    }
    doc_xml.push_str("</w:body></w:document>");

    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

    for (name, content) in [
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", rels.as_bytes()),
        ("word/document.xml", doc_xml.as_bytes()),
    ] {
        writer.add(name, EntryMeta::new(), content).map_err(|e| Error::new(e.to_string()))?;
    }
    writer.finish().map_err(|e| Error::new(e.to_string()))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
