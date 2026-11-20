// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 差し込み印刷 (mail merge): the `MERGEFIELD` fields of a document filled with the values of a
//! record, and a document made of one copy of the main document for each record.

use std::collections::HashMap;

use crate::flow::{
    Block, CellAnchor, CellDirection, FlowAlign, FlowCell, FlowDocument, FlowParagraph, FlowRow, FlowRun, FlowRunKind, FlowRunStyle,
    FlowTable, LineHeight, PageSetup, RowHeight, RunFormat,
};
use crate::flow_command::renumbered;
use crate::geometry::{Emu, EmuSize};

/// The field a `MERGEFIELD` instruction names (`MERGEFIELD 氏名 \* MERGEFORMAT`, or
/// `MERGEFIELD "Last Name"`).
pub fn merge_field_name(instruction: &str) -> Option<String> {
    let rest = instruction.trim().strip_prefix("MERGEFIELD").or_else(|| instruction.trim().strip_prefix("mergefield"))?;
    let rest = rest.trim_start();
    let name = if let Some(quoted) = rest.strip_prefix('"') {
        quoted.split('"').next()?.to_string()
    } else {
        rest.split_whitespace().next()?.to_string()
    };
    (!name.is_empty() && !name.starts_with('\\')).then_some(name)
}

/// The instruction of the `MERGEFIELD` for field `name`.
/// A mapping between a column in the data source and a standard Word merge field (ECMA-376 Part 1 §17.14.15 `w:fieldMapData`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FieldMapData {
    /// The name of the column in the data source (e.g. "郵便番号", "Zip").
    pub name: String,
    /// The standard Word field name (e.g. "Postal Code", "Last Name").
    pub mapped_name: String,
    /// The 0-based column index in the data source.
    pub column: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct StandardMergeField {
    pub mapped_name: &'static str,
    pub label_ja: &'static str,
    pub aliases: &'static [&'static str],
}

pub const STANDARD_MERGE_FIELDS: &[StandardMergeField] = &[
    StandardMergeField {
        mapped_name: "Last Name",
        label_ja: "姓",
        aliases: &["姓", "氏名", "名前", "お名前", "氏", "lastname", "last_name", "last name", "familyname", "family_name", "family name", "name", "full_name", "fullname"],
    },
    StandardMergeField {
        mapped_name: "First Name",
        label_ja: "名",
        aliases: &["名", "firstname", "first_name", "first name", "givenname"],
    },
    StandardMergeField {
        mapped_name: "Courtesy Title",
        label_ja: "敬称",
        aliases: &["敬称", "title", "courtesy", "courtesytitle", "courtesy_title"],
    },
    StandardMergeField {
        mapped_name: "Job Title",
        label_ja: "役職",
        aliases: &["役職", "肩書", "職位", "jobtitle", "job_title", "position"],
    },
    StandardMergeField {
        mapped_name: "Company",
        label_ja: "会社名",
        aliases: &["会社名", "会社", "勤務先", "御社", "company", "organization", "org", "corp", "corporation"],
    },
    StandardMergeField {
        mapped_name: "Address 1",
        label_ja: "住所 1",
        aliases: &["住所1", "住所１", "住所", "町名・番地", "address1", "address_1", "address 1", "address", "addr1", "addr", "street"],
    },
    StandardMergeField {
        mapped_name: "Address 2",
        label_ja: "住所 2",
        aliases: &["住所2", "住所２", "建物名", "ビル名", "部屋番号", "address2", "address_2", "address 2", "addr2", "building", "bldg", "apt", "suite"],
    },
    StandardMergeField {
        mapped_name: "City",
        label_ja: "市区町村",
        aliases: &["市区町村", "市区", "市", "区", "町", "村", "city", "town"],
    },
    StandardMergeField {
        mapped_name: "State",
        label_ja: "都道府県",
        aliases: &["都道府県", "県", "都", "府", "state", "province", "prefecture", "pref"],
    },
    StandardMergeField {
        mapped_name: "Postal Code",
        label_ja: "郵便番号",
        aliases: &["郵便番号", "郵便", "〒", "postalcode", "postal_code", "postal code", "zip", "zipcode", "zip_code"],
    },
    StandardMergeField {
        mapped_name: "Country or Region",
        label_ja: "国または地域",
        aliases: &["国または地域", "国名", "国", "country", "region", "country_region"],
    },
    StandardMergeField {
        mapped_name: "Business Phone",
        label_ja: "電話番号",
        aliases: &["電話番号", "電話", "tel", "phone", "businessphone", "business_phone", "tel_no"],
    },
    StandardMergeField {
        mapped_name: "Email Address",
        label_ja: "電子メール アドレス",
        aliases: &["電子メール アドレス", "メールアドレス", "メール", "email", "e-mail", "mail", "email_address"],
    },
];

/// Automatically matches columns in `fields` to standard merge fields based on aliases.
pub fn auto_match_fields(fields: &[String]) -> Vec<FieldMapData> {
    let mut out = Vec::new();
    let mut used_columns = Vec::new();
    for std_field in STANDARD_MERGE_FIELDS {
        for alias in std_field.aliases {
            let norm_alias = alias.replace([' ', '_', '-'], "").to_lowercase();
            if let Some((idx, col)) = fields.iter().enumerate().find(|(i, f)| {
                !used_columns.contains(i) && f.replace([' ', '_', '-'], "").to_lowercase() == norm_alias
            }) {
                used_columns.push(idx);
                out.push(FieldMapData {
                    name: col.clone(),
                    mapped_name: std_field.mapped_name.to_string(),
                    column: idx,
                });
                break;
            }
        }
    }
    out
}

/// Checks whether `alias` matches standard field `mapped_name`.
pub fn is_standard_field_alias(mapped_name: &str, alias: &str) -> bool {
    let norm = alias.replace([' ', '_', '-'], "").to_lowercase();
    STANDARD_MERGE_FIELDS
        .iter()
        .find(|s| s.mapped_name.eq_ignore_ascii_case(mapped_name))
        .is_some_and(|s| {
            s.mapped_name.replace([' ', '_', '-'], "").to_lowercase() == norm
                || s.label_ja.replace([' ', '_', '-'], "").to_lowercase() == norm
                || s.aliases.iter().any(|a| a.replace([' ', '_', '-'], "").to_lowercase() == norm)
        })
}

/// Resolves a merge field name to its value in `values`, using `field_map` if direct lookup fails.
pub fn resolve_merge_value(name: &str, values: &HashMap<String, String>, field_map: &[FieldMapData]) -> Option<String> {
    if let Some(v) = values.get(name) {
        return Some(v.clone());
    }
    if let Some((_, v)) = values.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)) {
        return Some(v.clone());
    }
    // Try field mapping
    if let Some(fmd) = field_map.iter().find(|m| {
        m.mapped_name.eq_ignore_ascii_case(name)
            || m.name.eq_ignore_ascii_case(name)
            || is_standard_field_alias(&m.mapped_name, name)
    }) {
        if let Some(v) = values.get(&fmd.name) {
            return Some(v.clone());
        }
        if let Some((_, v)) = values.iter().find(|(k, _)| k.eq_ignore_ascii_case(&fmd.name)) {
            return Some(v.clone());
        }
    }
    None
}

/// The instruction of the `MERGEFIELD` for field `name`.
pub fn merge_field_instruction(name: &str) -> String {
    if name.contains(char::is_whitespace) { format!("MERGEFIELD \"{name}\" ") } else { format!("MERGEFIELD {name} ") }
}

/// The format of an `ADDRESSBLOCK` or `GREETINGLINE` (`\f "<郵便番号>\n<住所>"`): the text between the quotes after `\f`, where
/// `<列名>` is the record's value of that field and `\n` ends a line.
fn block_format(instruction: &str) -> Option<String> {
    let at = instruction.find("\\f")?;
    let rest = instruction[at + 2..].trim_start().strip_prefix('"')?;
    Some(rest.split('"').next()?.to_string())
}

/// The field names a block format uses (`<氏名>`).
fn block_names(format: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = format;
    while let Some(open) = rest.find('<') {
        let Some(close) = rest[open..].find('>').map(|c| open + c) else { break };
        let name = rest[open + 1..close].to_string();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
        rest = &rest[close + 1..];
    }
    out
}

/// `ADDRESSBLOCK`: the format's lines with the record's values; a line whose fields are all empty is left out. `GREETINGLINE`
/// (`one_line`): the format with the values, one line.
fn merged_block(format: &str, one_line: bool, value: &dyn Fn(&str) -> Option<String>) -> String {
    let mut lines = Vec::new();
    for line in format.split("\\n") {
        let mut out = String::new();
        let (mut names, mut filled) = (0, 0);
        let mut rest = line;
        while let Some(open) = rest.find('<') {
            let Some(close) = rest[open..].find('>').map(|c| open + c) else { break };
            out.push_str(&rest[..open]);
            names += 1;
            let text = value(&rest[open + 1..close]).unwrap_or_default();
            filled += usize::from(!text.trim().is_empty());
            out.push_str(text.trim());
            rest = &rest[close + 1..];
        }
        out.push_str(rest);
        if names == 0 || filled > 0 || one_line {
            lines.push(out.trim().to_string());
        }
    }
    lines.join("\n")
}

/// The instruction of an `ADDRESSBLOCK` (住所ブロック) of `format`.
pub fn address_block_instruction(format: &str) -> String {
    format!("ADDRESSBLOCK \\f \"{format}\" ")
}

/// The instruction of a `GREETINGLINE` (挨拶文) of `format`.
pub fn greeting_line_instruction(format: &str) -> String {
    format!("GREETINGLINE \\f \"{format}\" ")
}

/// The address block for a list of `fields`: postal code, address, company, department and name, those it has.
pub fn default_address_format(fields: &[String]) -> String {
    default_address_format_with_mapping(fields, &[])
}

/// The address block for a list of `fields` with optional field mapping.
pub fn default_address_format_with_mapping(fields: &[String], map: &[FieldMapData]) -> String {
    let mapped_col = |std_name: &str| map.iter().find(|m| m.mapped_name == std_name).map(|m| m.name.clone());
    let has = |names: &[&str]| names.iter().find(|n| fields.iter().any(|f| f.eq_ignore_ascii_case(n))).map(|n| n.to_string());
    let mut lines = Vec::new();
    if let Some(n) = mapped_col("Postal Code").or_else(|| has(&["郵便番号", "PostalCode", "Zip"])) {
        lines.push(format!("〒<{n}>"));
    }
    let mut addr_parts = Vec::new();
    if let Some(n) = mapped_col("State").or_else(|| has(&["都道府県", "State", "Prefecture"])) {
        addr_parts.push(format!("<{n}>"));
    }
    if let Some(n) = mapped_col("City").or_else(|| has(&["市区町村", "City"])) {
        addr_parts.push(format!("<{n}>"));
    }
    if let Some(n) = mapped_col("Address 1").or_else(|| has(&["住所", "Address", "住所1"])) {
        addr_parts.push(format!("<{n}>"));
    }
    if !addr_parts.is_empty() {
        lines.push(addr_parts.concat());
    }
    if let Some(n) = mapped_col("Address 2").or_else(|| has(&["住所2", "Address2", "Building", "建物名", "ビル名"])) {
        lines.push(format!("<{n}>"));
    }
    for names in [&["会社名", "会社", "Company"][..], &["部署", "Department"]] {
        let col = if names[0] == "会社名" { mapped_col("Company").or_else(|| has(names)) } else { has(names) };
        if let Some(n) = col {
            lines.push(format!("<{n}>"));
        }
    }
    let last = mapped_col("Last Name");
    let first = mapped_col("First Name");
    if let (Some(l), Some(f)) = (last.as_ref(), first.as_ref()) {
        lines.push(format!("<{l}> <{f}> 様"));
    } else if let Some(n) = last.or_else(|| has(&["氏名", "名前", "Name"])) {
        lines.push(format!("<{n}> 様"));
    }
    lines.join("\\n")
}

/// The greeting line for a list of `fields`: the name and 様 (or the first field).
pub fn default_greeting_format(fields: &[String]) -> String {
    default_greeting_format_with_mapping(fields, &[])
}

/// The greeting line for a list of `fields` with optional field mapping.
pub fn default_greeting_format_with_mapping(fields: &[String], map: &[FieldMapData]) -> String {
    let mapped_col = |std_name: &str| map.iter().find(|m| m.mapped_name == std_name).map(|m| m.name.clone());
    let last = mapped_col("Last Name");
    let first = mapped_col("First Name");
    if let (Some(l), Some(f)) = (last.as_ref(), first.as_ref()) {
        format!("<{l}> <{f}> 様")
    } else {
        let name = last.or_else(|| ["氏名", "名前", "Name"].iter().find(|n| fields.iter().any(|f| f.eq_ignore_ascii_case(n))).map(|n| n.to_string())).or_else(|| fields.first().cloned()).unwrap_or_default();
        format!("<{name}> 様")
    }
}

/// What a merge field shows before it is merged (Word's «氏名»).
pub fn merge_field_placeholder(name: &str) -> String {
    format!("«{name}»")
}

/// The fields of `doc`'s main text (their names, in the order they are first used).
pub fn merge_fields_used(doc: &FlowDocument) -> Vec<String> {
    fn walk(blocks: &[Block], out: &mut Vec<String>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    for r in &p.runs {
                        if let FlowRunKind::Field { instruction, .. } = &r.kind
                        {
                            let names = match merge_field_name(instruction) {
                                Some(name) => vec![name],
                                None if matches!(first_word(instruction).as_str(), "ADDRESSBLOCK" | "GREETINGLINE") => block_format(instruction).map(|f| block_names(&f)).unwrap_or_default(),
                                None => Vec::new(),
                            };
                            for name in names {
                                if !out.contains(&name) {
                                    out.push(name);
                                }
                            }
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut out = Vec::new();
    walk(&doc.body, &mut out);
    out
}

/// The first word of a field's instruction, upper-cased (`IF`, `MERGEFIELD`).
fn first_word(instruction: &str) -> String {
    instruction.split_whitespace().next().unwrap_or("").to_ascii_uppercase()
}

/// `instruction` with each `{ MERGEFIELD 名 }` in it replaced by the record's value (a value with a space and no quotes
/// around the field gets quotes, so the comparison reads it as one word).
fn expand_merge_fields(instruction: &str, value: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = instruction;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}').map(|c| open + c) else { break };
        let inner = rest[open + 1..close].trim();
        out.push_str(&rest[..open]);
        if let Some(name) = merge_field_name(inner) {
            let text = merged_text(inner, &name, value);
            if !out.ends_with('"') && text.contains(char::is_whitespace) {
                out.push_str(&format!("\"{text}\""));
            } else {
                out.push_str(&text);
            }
        } else {
            out.push_str(&rest[open..=close]);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// What a `MERGEFIELD` shows: its value, with the case switches (`\* Upper`…) and the text before (`\b`) and after (`\f`)
/// it when the value is not empty.
fn merged_text(instruction: &str, name: &str, value: &dyn Fn(&str) -> Option<String>) -> String {
    let text = value(name).unwrap_or_default();
    if text.is_empty() {
        return text;
    }
    let words = crate::field_format::field_words(instruction);
    let switch = |flag: &str| words.iter().position(|w| w == flag).and_then(|i| words.get(i + 1)).cloned().unwrap_or_default();
    let text = crate::field_format::apply_text_switches(text, &words);
    format!("{}{text}{}", switch("\\b"), switch("\\f"))
}

/// What `instruction` shows once merged, or `None` for a field that is not a merge's.
fn merged_field_text(instruction: &str, value: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    if let Some(name) = merge_field_name(instruction) {
        return Some(merged_text(instruction, &name, value));
    }
    match first_word(instruction).as_str() {
        "IF" if instruction.contains('{') => Some(crate::field_format::if_field_text(&expand_merge_fields(instruction, value)).unwrap_or_default()),
        "COMPARE" if instruction.contains('{') => Some(crate::field_format::compare_field_text(&expand_merge_fields(instruction, value)).unwrap_or_default()),
        "SKIPIF" | "NEXT" | "NEXTIF" => Some(String::new()),
        "ADDRESSBLOCK" => Some(merged_block(&block_format(instruction)?, false, value)),
        "GREETINGLINE" => Some(merged_block(&block_format(instruction)?, true, value)),
        _ => None,
    }
}

fn fill_paragraph(p: &mut FlowParagraph, value: &dyn Fn(&str) -> Option<String>) {
    for run in &mut p.runs {
        let FlowRunKind::Field { instruction, .. } = &run.kind else { continue };
        let Some(text) = merged_field_text(instruction, value) else { continue };
        run.kind = FlowRunKind::Text(text);
    }
    // A block of several lines: a line break between them.
    if p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Text(t) if t.contains('\n'))) {
        let mut runs = Vec::new();
        for run in std::mem::take(&mut p.runs) {
            match &run.kind {
                FlowRunKind::Text(t) if t.contains('\n') => {
                    for (i, line) in t.split('\n').enumerate() {
                        if i > 0 {
                            runs.push(FlowRun { kind: FlowRunKind::LineBreak, ..run.clone() });
                        }
                        runs.push(FlowRun { kind: FlowRunKind::Text(line.to_string()), ..run.clone() });
                    }
                }
                _ => runs.push(run),
            }
        }
        p.runs = runs;
    }
    p.normalize();
}

/// Whether a `SKIPIF` field in `blocks` holds for the record `value` gives (`SKIPIF { MERGEFIELD 氏名 } = ""`): that
/// record is left out of the letters.
fn skipped(blocks: &[Block], value: &dyn Fn(&str) -> Option<String>) -> bool {
    blocks.iter().any(|b| match b {
        Block::Paragraph(p) => p.runs.iter().any(|r| {
            let FlowRunKind::Field { instruction, .. } = &r.kind else { return false };
            let trimmed = instruction.trim_start();
            first_word(trimmed) == "SKIPIF"
                && crate::field_format::compare_field_text(&expand_merge_fields(&format!("COMPARE{}", &trimmed[6..]), value)).as_deref() == Some("1")
        }),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).any(|c| skipped(&c.blocks, value)),
    })
}

/// `blocks` with the fields of record `*record` of `records` (the values of `fields`), going on to the next record after
/// each `NEXT` (`NEXTIF`): several records on one page, as for labels.
fn fill_blocks_sequence(blocks: &mut [Block], fields: &[String], records: &[Vec<String>], map: &[FieldMapData], record: &mut usize) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                let mut i = 0;
                while i < p.runs.len() {
                    let values: HashMap<String, String> = fields.iter().cloned().zip(records[(*record).min(records.len() - 1)].iter().cloned()).collect();
                    let lookup = |name: &str| resolve_merge_value(name, &values, map);
                    let is_next = match &p.runs[i].kind {
                        FlowRunKind::Field { instruction, .. } => {
                            let trimmed = instruction.trim_start();
                            match first_word(trimmed).as_str() {
                                "NEXT" => true,
                                "NEXTIF" => crate::field_format::compare_field_text(&expand_merge_fields(&format!("COMPARE{}", &trimmed[6..]), &lookup)).as_deref() == Some("1"),
                                _ => false,
                            }
                        }
                        _ => false,
                    };
                    if let FlowRunKind::Field { instruction, .. } = &p.runs[i].kind {
                        if let Some(text) = merged_field_text(instruction, &lookup) {
                            p.runs[i].kind = FlowRunKind::Text(text);
                        }
                    }
                    if is_next {
                        *record = (*record + 1).min(records.len() - 1);
                    }
                    i += 1;
                }
                // A block of several lines: a line break between them.
                if p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Text(t) if t.contains('\n'))) {
                    let mut runs = Vec::new();
                    for run in std::mem::take(&mut p.runs) {
                        match &run.kind {
                            FlowRunKind::Text(t) if t.contains('\n') => {
                                for (i, line) in t.split('\n').enumerate() {
                                    if i > 0 {
                                        runs.push(FlowRun { kind: FlowRunKind::LineBreak, ..run.clone() });
                                    }
                                    runs.push(FlowRun { kind: FlowRunKind::Text(line.to_string()), ..run.clone() });
                                }
                            }
                            _ => runs.push(run),
                        }
                    }
                    p.runs = runs;
                }
                p.normalize();
            }
            Block::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        fill_blocks_sequence(&mut cell.blocks, fields, records, map, record);
                    }
                }
            }
        }
    }
}

/// Whether the block has a `NEXT` or `NEXTIF` field.
fn has_next(b: &Block) -> bool {
    match b {
        Block::Paragraph(p) => p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Field { instruction, .. } if matches!(first_word(instruction.trim_start()).as_str(), "NEXT" | "NEXTIF"))),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).any(|c| c.blocks.iter().any(has_next)),
    }
}

fn fill_blocks(blocks: &mut [Block], value: &dyn Fn(&str) -> Option<String>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => fill_paragraph(p, value),
            Block::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        fill_blocks(&mut cell.blocks, value);
                    }
                }
            }
        }
    }
}

/// `doc` with each merge field replaced by its value in `values` (a field it has no value for: empty), its headers and
/// footers too: what 結果のプレビュー shows.
pub fn merged_view(doc: &FlowDocument, values: &HashMap<String, String>) -> FlowDocument {
    let value = |name: &str| resolve_merge_value(name, values, &doc.mail_merge_field_map);
    let mut out = doc.clone();
    fill_blocks(&mut out.body, &value);
    for list in [&mut out.header, &mut out.footer, &mut out.header_first, &mut out.footer_first, &mut out.header_even, &mut out.footer_even] {
        fill_blocks(list, &value);
    }
    out
}

/// Whether the record (`record`, the values of `fields`) is left out of the letters by a `SKIPIF` in `doc`.
pub fn record_skipped(doc: &FlowDocument, fields: &[String], record: &[String]) -> bool {
    let values: HashMap<String, String> = fields.iter().cloned().zip(record.iter().cloned()).collect();
    let lookup = |name: &str| resolve_merge_value(name, &values, &doc.mail_merge_field_map);
    skipped(&doc.body, &lookup)
}

/// A document with one copy of `doc`'s main text for each record (`records` are the rows of `fields`' values), each
/// on a new page: 個々のドキュメントの編集.
pub fn merged_letters(doc: &FlowDocument, fields: &[String], records: &[Vec<String>]) -> FlowDocument {
    let mut out = doc.clone();
    let mut body: Vec<Block> = Vec::new();
    let mut next = doc.new_id();
    let mut n = 0;
    let mut i = 0;
    while i < records.len() {
        let record = &records[i];
        let values: HashMap<String, String> = fields.iter().cloned().zip(record.iter().cloned()).collect();
        let lookup = |name: &str| resolve_merge_value(name, &values, &doc.mail_merge_field_map);
        if skipped(&doc.body, &lookup) {
            i += 1;
            continue;
        }
        let mut copy = merged_view(doc, &values);
        // A NEXT in the text goes on to the following record within the same copy.
        let mut last = i;
        if doc.body.iter().any(|b| has_next(b)) {
            copy.body = doc.body.clone();
            fill_blocks_sequence(&mut copy.body, fields, records, &doc.mail_merge_field_map, &mut last);
        }
        i = last + 1;
        let mut blocks = if n == 0 { copy.body } else { renumbered(&copy.body, &mut next) };
        if n > 0 {
            match blocks.first_mut() {
                Some(Block::Paragraph(p)) => {
                    p.page_break_before = true;
                    // The direct format too: that is what is saved.
                    p.direct.page_break_before = Some(true);
                }
                _ => {
                    let mut p = FlowParagraph { id: next, ..FlowParagraph::default() };
                    next += 1;
                    p.page_break_before = true;
                    p.direct.page_break_before = Some(true);
                    p.runs.push(FlowRun::new(FlowRunKind::Text(String::new()), Default::default()));
                    blocks.insert(0, Block::Paragraph(p));
                }
            }
        }
        body.extend(blocks);
        n += 1;
    }
    if body.is_empty() {
        body = doc.body.clone();
    }
    out.body = body;
    out
}

/// 差し込み印刷の印刷範囲 (Word's Merge to Printer range: All, Current record, or From..To).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MergePrintRange {
    #[default]
    All,
    Current,
    FromTo(usize, usize),
}

/// Merges records for print based on `range` and `current` (0-based index of current record).
/// Returns the merged `FlowDocument` and the number of merged records.
pub fn merged_letters_for_range(
    doc: &FlowDocument,
    fields: &[String],
    records: &[Vec<String>],
    range: MergePrintRange,
    current: usize,
) -> Result<(FlowDocument, usize), String> {
    if records.is_empty() {
        return Err("No recipients".to_string());
    }
    let subset: Vec<Vec<String>> = match range {
        MergePrintRange::All => records.to_vec(),
        MergePrintRange::Current => {
            if current >= records.len() {
                return Err(format!("Current recipient index {current} out of range (max {})", records.len()));
            }
            vec![records[current].clone()]
        }
        MergePrintRange::FromTo(start, end) => {
            if start == 0 {
                return Err("Start record index must be >= 1".to_string());
            }
            if start > end {
                return Err(format!("Start record index {start} cannot be greater than end index {end}"));
            }
            if start > records.len() {
                return Err(format!("Start record index {start} out of range (max {})", records.len()));
            }
            let end_idx = end.min(records.len());
            records[start - 1..end_idx].to_vec()
        }
    };

    let active: Vec<Vec<String>> = subset
        .into_iter()
        .filter(|rec| !record_skipped(doc, fields, rec))
        .collect();

    if active.is_empty() {
        return Err("No active recipients to print after applying rules".to_string());
    }

    let count = active.len();
    let merged = merged_letters(doc, fields, &active);
    Ok((merged, count))
}

/// A table of text values read from `text`: comma, tab or semicolon separated (the first line's, whichever it has most), quotes as CSV
/// has them; the first line the field names. Empty lines are left out.
pub fn parse_table(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let text = text.trim_start_matches('\u{feff}');
    let first = text.lines().next().unwrap_or("");
    let separator = [',', '\t', ';'].into_iter().max_by_key(|c| first.matches(*c).count()).filter(|c| first.contains(*c)).unwrap_or(',');
    let mut rows: Vec<Vec<String>> = Vec::new();
    let (mut row, mut cell, mut quoted, mut chars) = (Vec::new(), String::new(), false, text.chars().peekable());
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == separator && !quoted => row.push(std::mem::take(&mut cell)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            c => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    let mut rows = rows.into_iter();
    let fields: Vec<String> = rows.next().unwrap_or_default().into_iter().map(|f| f.trim().to_string()).collect();
    let width = fields.len();
    let records = rows.map(|mut r| {
        r.resize(width, String::new());
        r
    }).collect();
    (fields, records)
}

/// The recipient list as text (the first line the field names, tab separated; a quoted cell when it has a tab, a quote or a line
/// break), what `parse_table` reads back.
pub fn serialize_table(fields: &[String], records: &[Vec<String>]) -> String {
    let cell = |c: &str| if c.contains(['\t', '"', '\n', '\r']) { format!("\"{}\"", c.replace('"', "\"\"")) } else { c.to_string() };
    let mut out = String::new();
    for row in std::iter::once(fields).chain(records.iter().map(Vec::as_slice)) {
        out.push_str(&row.iter().map(|c| cell(c)).collect::<Vec<_>>().join("\t"));
        out.push('\n');
    }
    out
}

/// How a condition of a recipient filter compares a field with its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterOp {
    Equal,
    NotEqual,
    Contains,
    NotContains,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

/// One condition of a filter: field `field` (an index into the fields) compared with `value`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterCondition {
    pub field: usize,
    pub op: FilterOp,
    pub value: String,
}

impl FilterCondition {
    fn matches(&self, record: &[String]) -> bool {
        let text = record.get(self.field).map(String::as_str).unwrap_or("").trim();
        let value = self.value.trim();
        // Numbers by value, text by characters.
        let order = || match (text.parse::<f64>(), value.parse::<f64>()) {
            (Ok(a), Ok(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
            _ => text.cmp(value),
        };
        match self.op {
            FilterOp::Equal => text == value,
            FilterOp::NotEqual => text != value,
            FilterOp::Contains => text.contains(value),
            FilterOp::NotContains => !text.contains(value),
            FilterOp::Less => order().is_lt(),
            FilterOp::LessOrEqual => order().is_le(),
            FilterOp::Greater => order().is_gt(),
            FilterOp::GreaterOrEqual => order().is_ge(),
        }
    }
}

/// A recipient filter (宛先の抽出): conditions joined by AND and OR (AND binds tighter, as the query of Word's 宛先の絞り込み reads),
/// as a list of groups any of which has to hold, each a list of conditions all of which have to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordFilter {
    pub any_of: Vec<Vec<FilterCondition>>,
}

impl RecordFilter {
    pub fn matches(&self, record: &[String]) -> bool {
        self.any_of.is_empty() || self.any_of.iter().any(|all| all.iter().all(|c| c.matches(record)))
    }
}

/// A filter read from text: conditions `項目名=値`, `項目名!=値`, `項目名~値` (has), `項目名!~値` (has not), `<`, `<=`, `>`, `>=`, joined with
/// `AND`・`OR` (or `かつ`・`または`; also `&&` and `||`). `None` when empty or when a condition names no field or has no operator.
pub fn parse_filter(text: &str, fields: &[String]) -> Option<RecordFilter> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    // Split at the joins, which stand between spaces (or are `&&`, `||`).
    let normalised = text.replace("&&", " AND ").replace("||", " OR ").replace(" かつ ", " AND ").replace(" または ", " OR ");
    let mut any_of: Vec<Vec<FilterCondition>> = vec![Vec::new()];
    let mut part = String::new();
    let flush = |part: &mut String, any_of: &mut Vec<Vec<FilterCondition>>| -> Option<()> {
        let condition = parse_condition(part.trim(), fields)?;
        any_of.last_mut()?.push(condition);
        part.clear();
        Some(())
    };
    for word in normalised.split(' ') {
        match word.to_ascii_uppercase().as_str() {
            "AND" => {
                flush(&mut part, &mut any_of)?;
            }
            "OR" => {
                flush(&mut part, &mut any_of)?;
                any_of.push(Vec::new());
            }
            _ => {
                if !part.is_empty() {
                    part.push(' ');
                }
                part.push_str(word);
            }
        }
    }
    flush(&mut part, &mut any_of)?;
    Some(RecordFilter { any_of })
}

fn parse_condition(text: &str, fields: &[String]) -> Option<FilterCondition> {
    // The operator: the earliest one in the text (the longer one where two start together).
    const OPS: [(&str, FilterOp); 10] = [("!=", FilterOp::NotEqual), ("!~", FilterOp::NotContains), ("<=", FilterOp::LessOrEqual), (">=", FilterOp::GreaterOrEqual), ("≠", FilterOp::NotEqual), ("=", FilterOp::Equal), ("~", FilterOp::Contains), ("<", FilterOp::Less), (">", FilterOp::Greater), ("≤", FilterOp::LessOrEqual)];
    let (at, symbol, op) = OPS.iter().filter_map(|(symbol, op)| text.find(symbol).map(|at| (at, *symbol, *op))).min_by_key(|(at, symbol, _)| (*at, std::cmp::Reverse(symbol.len())))?;
    let name = text[..at].trim();
    let value = text[at + symbol.len()..].trim().to_string();
    let field = fields.iter().position(|f| f.as_str() == name)?;
    Some(FilterCondition { field, op, value })
}

/// Preset envelope sizes (DOCX-MERGE-016: 封筒サイズと宛名配置).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopePreset {
    /// 長形3号: 120 × 235 mm (A4三つ折り用)
    Chou3,
    /// 長形4号: 90 × 205 mm (B5四つ折り用)
    Chou4,
    /// 洋形2号: 162 × 114 mm (はがき・招待状用)
    You2,
    /// 洋形長3号: 235 × 120 mm (横型A4三つ折り用)
    YouChou3,
    /// 角形2号: 240 × 332 mm (A4そのまま用)
    Kaku2,
    /// DL: 220 × 110 mm (国際標準封筒)
    Dl,
    /// Commercial #10: 241.3 × 104.8 mm (米国標準ビジネス封筒)
    Comm10,
}

impl EnvelopePreset {
    pub const ALL: [Self; 7] = [
        Self::Chou3,
        Self::Chou4,
        Self::You2,
        Self::YouChou3,
        Self::Kaku2,
        Self::Dl,
        Self::Comm10,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Chou3 => "chou3",
            Self::Chou4 => "chou4",
            Self::You2 => "you2",
            Self::YouChou3 => "youChou3",
            Self::Kaku2 => "kaku2",
            Self::Dl => "dl",
            Self::Comm10 => "comm10",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Chou3 => "長形3号 (120×235mm)",
            Self::Chou4 => "長形4号 (90×205mm)",
            Self::You2 => "洋形2号 (162×114mm)",
            Self::YouChou3 => "洋形長3号 (235×120mm)",
            Self::Kaku2 => "角形2号 (240×332mm)",
            Self::Dl => "DL (220×110mm)",
            Self::Comm10 => "#10 (241.3×104.8mm)",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id().eq_ignore_ascii_case(id))
    }

    pub fn size(self) -> EmuSize {
        const MM: Emu = 36_000;
        match self {
            Self::Chou3 => EmuSize::new(120 * MM, 235 * MM),
            Self::Chou4 => EmuSize::new(90 * MM, 205 * MM),
            Self::You2 => EmuSize::new(162 * MM, 114 * MM),
            Self::YouChou3 => EmuSize::new(235 * MM, 120 * MM),
            Self::Kaku2 => EmuSize::new(240 * MM, 332 * MM),
            Self::Dl => EmuSize::new(220 * MM, 110 * MM),
            Self::Comm10 => EmuSize::new(8_686_800, 3_772_800),
        }
    }

    pub fn is_landscape(self) -> bool {
        let s = self.size();
        s.width > s.height
    }

    pub fn margins(self) -> (Emu, Emu, Emu, Emu) {
        const MM: Emu = 36_000;
        if self.is_landscape() {
            (12 * MM, 15 * MM, 12 * MM, 15 * MM)
        } else if self == Self::Kaku2 {
            (20 * MM, 20 * MM, 20 * MM, 20 * MM)
        } else if self == Self::Chou4 {
            (15 * MM, 12 * MM, 15 * MM, 12 * MM)
        } else {
            (15 * MM, 15 * MM, 15 * MM, 15 * MM)
        }
    }

    pub fn page_setup(self) -> PageSetup {
        let (top, right, bottom, left) = self.margins();
        PageSetup {
            size: self.size(),
            top,
            right,
            bottom,
            left,
            header: top / 2,
            footer: bottom / 2,
            ..PageSetup::default()
        }
    }
}

/// Blocks of an envelope layout with recipient address block and optional return address (DOCX-MERGE-016).
pub fn envelope_blocks(
    preset: EnvelopePreset,
    return_address: Option<&str>,
    address_format: &str,
    next_id: &mut u32,
) -> Vec<Block> {
    let mut blocks = Vec::new();
    let size = preset.size();
    let landscape = preset.is_landscape();

    let make_return_para = |text: &str, id: u32, before: Emu, left: Emu, font_size_pt: u32| -> FlowParagraph {
        let mut p = FlowParagraph { id, ..FlowParagraph::default() };
        p.direct.line = Some(LineHeight::Auto(240));
        p.direct.before = Some(before);
        p.direct.left = Some(left);
        let mut runs = Vec::new();
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                runs.push(FlowRun::new(FlowRunKind::LineBreak, FlowRunStyle::default()));
            }
            let mut r = FlowRun::new(FlowRunKind::Text(line.to_string()), FlowRunStyle::default());
            r.format = RunFormat {
                size: Some(font_size_pt * 100),
                ..RunFormat::default()
            };
            runs.push(r);
        }
        p.runs = runs;
        p
    };

    let make_delivery_para = |id: u32, before: Emu, left: Emu, font_size_pt: u32| -> FlowParagraph {
        let mut p = FlowParagraph { id, ..FlowParagraph::default() };
        p.direct.line = Some(LineHeight::Auto(240));
        p.direct.before = Some(before);
        p.direct.left = Some(left);
        let mut run = FlowRun::new(
            FlowRunKind::Field {
                instruction: address_block_instruction(address_format),
                shown: "«住所ブロック»".to_string(),
            },
            FlowRunStyle::default(),
        );
        run.format = RunFormat {
            size: Some(font_size_pt * 100),
            ..RunFormat::default()
        };
        p.runs.push(run);
        p
    };

    if landscape {
        // Landscape (You2, YouChou3, Dl, Comm10):
        // Return address at top-left
        if let Some(sender) = return_address.filter(|s| !s.trim().is_empty()) {
            let p = make_return_para(sender, *next_id, 0, 0, 10);
            *next_id += 1;
            blocks.push(Block::Paragraph(p));
        }
        // Delivery address indented towards center-right
        let left = size.width * 2 / 5;
        let before = size.height / 4;
        let p = make_delivery_para(*next_id, before, left, 14);
        *next_id += 1;
        blocks.push(Block::Paragraph(p));
    } else {
        // Portrait (Chou3, Chou4, Kaku2):
        // Delivery address in central area
        let left = size.width / 4;
        let before = size.height / 5;
        let p = make_delivery_para(*next_id, before, left, 16);
        *next_id += 1;
        blocks.push(Block::Paragraph(p));
        // Return address in lower area
        if let Some(sender) = return_address.filter(|s| !s.trim().is_empty()) {
            let ret_before = size.height / 4;
            let ret_left = 10 * 36_000;
            let p = make_return_para(sender, *next_id, ret_before, ret_left, 9);
            *next_id += 1;
            blocks.push(Block::Paragraph(p));
        }
    }

    blocks
}

/// Standard label sheet presets (DOCX-MERGE-017: ラベル規格と差込ラベル).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelPreset {
    /// A4 24面: 3列 × 8行 (70.0 × 33.9 mm)
    A4_24,
    /// A4 12面: 2列 × 6行 (86.4 × 42.3 mm)
    A4_12,
    /// A4 10面: 2列 × 5行 (91.0 × 55.0 mm)
    A4_10,
    /// A4 21面: 3列 × 7行 (70.0 × 38.1 mm)
    A4_21,
    /// Avery 5160: Letter 30面 (3列 × 10行, 66.7 × 25.4 mm / 2.625 × 1.0 in)
    Avery5160,
    /// Avery 5163: Letter 10面 (2列 × 5行, 101.6 × 50.8 mm / 4.0 × 2.0 in)
    Avery5163,
}

impl LabelPreset {
    pub const ALL: [Self; 6] = [
        Self::A4_24,
        Self::A4_12,
        Self::A4_10,
        Self::A4_21,
        Self::Avery5160,
        Self::Avery5163,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::A4_24 => "a4_24",
            Self::A4_12 => "a4_12",
            Self::A4_10 => "a4_10",
            Self::A4_21 => "a4_21",
            Self::Avery5160 => "avery5160",
            Self::Avery5163 => "avery5163",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::A4_24 => "A4 24面 (70.0×33.9mm)",
            Self::A4_12 => "A4 12面 (86.4×42.3mm)",
            Self::A4_10 => "A4 10面 (91.0×55.0mm)",
            Self::A4_21 => "A4 21面 (70.0×38.1mm)",
            Self::Avery5160 => "Avery 5160 30面 (Letter)",
            Self::Avery5163 => "Avery 5163 10面 (Letter)",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id().eq_ignore_ascii_case(id))
    }

    pub fn rows(self) -> usize {
        match self {
            Self::A4_24 => 8,
            Self::A4_12 => 6,
            Self::A4_10 => 5,
            Self::A4_21 => 7,
            Self::Avery5160 => 10,
            Self::Avery5163 => 5,
        }
    }

    pub fn cols(self) -> usize {
        match self {
            Self::A4_24 => 3,
            Self::A4_12 => 2,
            Self::A4_10 => 2,
            Self::A4_21 => 3,
            Self::Avery5160 => 3,
            Self::Avery5163 => 2,
        }
    }

    pub fn count(self) -> usize {
        self.rows() * self.cols()
    }

    pub fn cell_width(self) -> Emu {
        const MM: Emu = 36_000;
        match self {
            Self::A4_24 => 70 * MM,
            Self::A4_12 => 86_400 * 36, // 86.4mm
            Self::A4_10 => 91 * MM,
            Self::A4_21 => 70 * MM,
            Self::Avery5160 => 2_400_300, // 2.625 in
            Self::Avery5163 => 3_657_600, // 4.0 in
        }
    }

    pub fn cell_height(self) -> Emu {
        const MM: Emu = 36_000;
        match self {
            Self::A4_24 => 33_900 * 36, // 33.9mm
            Self::A4_12 => 42_300 * 36, // 42.3mm
            Self::A4_10 => 55 * MM,     // 55mm
            Self::A4_21 => 38_100 * 36, // 38.1mm
            Self::Avery5160 => 914_400, // 1.0 in
            Self::Avery5163 => 1_828_800, // 2.0 in
        }
    }

    pub fn page_size(self) -> EmuSize {
        match self {
            Self::Avery5160 | Self::Avery5163 => EmuSize::new(7_772_400, 10_058_400), // Letter 8.5 x 11.0 in
            _ => EmuSize::new(7_560_000, 10_692_000), // A4 210 x 297 mm
        }
    }

    pub fn page_margins(self) -> (Emu, Emu, Emu, Emu) {
        let sheet = self.page_size();
        let total_w = self.cell_width() * self.cols() as Emu;
        let total_h = self.cell_height() * self.rows() as Emu;
        let margin_h = (sheet.width - total_w).max(0) / 2;
        let margin_v = (sheet.height - total_h).max(0) / 2;
        (margin_v, margin_h, margin_v, margin_h)
    }

    pub fn page_setup(self) -> PageSetup {
        let (top, right, bottom, left) = self.page_margins();
        PageSetup {
            size: self.page_size(),
            top,
            right,
            bottom,
            left,
            header: top / 2,
            footer: bottom / 2,
            ..PageSetup::default()
        }
    }
}

/// A table grid configured for a standard label sheet (DOCX-MERGE-017).
/// Cell (0, 0) has `{ ADDRESSBLOCK }`, while all subsequent cells have `{ NEXT }{ ADDRESSBLOCK }`.
pub fn label_sheet_table(preset: LabelPreset, address_format: &str, next_id: &mut u32) -> FlowTable {
    let cols = preset.cols();
    let rows = preset.rows();
    let cell_w = preset.cell_width();
    let cell_h = preset.cell_height();

    let table_id = *next_id;
    *next_id += 1;

    let mut table_rows = Vec::with_capacity(rows);
    for r in 0..rows {
        let row_id = *next_id;
        *next_id += 1;

        let mut cells = Vec::with_capacity(cols);
        for c in 0..cols {
            let cell_id = *next_id;
            *next_id += 1;

            let para_id = *next_id;
            *next_id += 1;

            let mut p = FlowParagraph { id: para_id, ..FlowParagraph::default() };
            p.direct.line = Some(LineHeight::Auto(240));

            let mut runs = Vec::new();
            if r > 0 || c > 0 {
                // Subsequent cells need NEXT field first
                runs.push(FlowRun::new(
                    FlowRunKind::Field {
                        instruction: "NEXT ".to_string(),
                        shown: String::new(),
                    },
                    FlowRunStyle::default(),
                ));
            }
            let mut addr_run = FlowRun::new(
                FlowRunKind::Field {
                    instruction: address_block_instruction(address_format),
                    shown: "«住所ブロック»".to_string(),
                },
                FlowRunStyle::default(),
            );
            addr_run.format = RunFormat {
                size: Some(1050), // 10.5pt standard label text
                ..RunFormat::default()
            };
            runs.push(addr_run);
            p.runs = runs;

            let cell = FlowCell {
                id: cell_id,
                own_borders: [None; 4],
                span: 1,
                merge: crate::flow::VerticalMerge::None,
                blocks: vec![Block::Paragraph(p)],
                borders: [None; 4],
                shading: None,
                shading_fill: None,
                shading_pattern: None,
                shading_color: None,
                anchor: CellAnchor::Top,
                margins: [144_000; 4], // 4mm padding
                diagonals: [None; 2],
                style_fill: None,
                direction: CellDirection::LrTb,
                no_wrap: false,
            };
            cells.push(cell);
        }

        table_rows.push(FlowRow {
            id: row_id,
            height: RowHeight::Exact(cell_h),
            header: false,
            cant_split: true,
            cells,
        });
    }

    FlowTable {
        id: table_id,
        borders: [None; 6],
        columns: vec![cell_w; cols],
        indent: 0,
        align: FlowAlign::Center,
        rows: table_rows,
        style: None,
        floating: None,
        autofit: None,
        fit_to_page: true,
        fixed_percent: None,
        cell_spacing: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_with_and_or_and_the_table_round_trips() {
        let fields: Vec<String> = ["氏名", "年齢", "市"].iter().map(|s| s.to_string()).collect();
        let records: Vec<Vec<String>> = vec![vec!["山田".into(), "30".into(), "東京".into()], vec!["佐藤".into(), "17".into(), "大阪".into()], vec!["鈴木".into(), "45".into(), "東京".into()]];
        let pick = |text: &str| -> Vec<usize> {
            let filter = parse_filter(text, &fields).expect("a filter");
            (0..records.len()).filter(|&i| filter.matches(&records[i])).collect()
        };
        assert_eq!(pick("市=東京"), [0, 2]);
        assert_eq!(pick("市=東京 AND 年齢>=40"), [2]);
        assert_eq!(pick("市=大阪 OR 年齢>=40"), [1, 2]);
        assert_eq!(pick("市=東京 AND 年齢<40 OR 氏名~佐"), [0, 1]);
        assert_eq!(pick("年齢>9"), [0, 1, 2], "numbers by value");
        assert_eq!(pick("氏名!~田"), [1, 2]);
        assert!(parse_filter("住所=東京", &fields).is_none() && parse_filter("市", &fields).is_none() && parse_filter("  ", &fields).is_none());
        let text = serialize_table(&fields, &records);
        assert_eq!(parse_table(&text), (fields.clone(), records.clone()));
        let tricky = vec![vec!["a\tb".to_string(), "c\"d".to_string(), "e".to_string()]];
        assert_eq!(parse_table(&serialize_table(&fields, &tricky)).1, tricky);
    }

    #[test]
    fn field_names() {
        assert_eq!(merge_field_name("MERGEFIELD 氏名 \\* MERGEFORMAT").as_deref(), Some("氏名"));
        assert_eq!(merge_field_name(" MERGEFIELD \"Last Name\" ").as_deref(), Some("Last Name"));
        assert_eq!(merge_field_name("PAGE"), None);
        assert_eq!(merge_field_name(&merge_field_instruction("会社 名")).as_deref(), Some("会社 名"));
    }

    fn field(instruction: &str) -> FlowRun {
        FlowRun::new(FlowRunKind::Field { instruction: instruction.into(), shown: String::new() }, Default::default())
    }

    fn text_of(doc: &FlowDocument) -> Vec<String> {
        doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.runs.iter().filter_map(|r| if let FlowRunKind::Text(t) = &r.kind { Some(t.as_str()) } else { None }).collect()) } else { None }).collect()
    }

    #[test]
    fn if_and_switches_and_skipif() {
        let mut doc = FlowDocument::default();
        let mut p = FlowParagraph { id: 1, ..FlowParagraph::default() };
        p.runs.push(field("IF { MERGEFIELD 年齢 } >= 20 \"成人\" \"未成年\" "));
        p.runs.push(field("MERGEFIELD 氏名 \\b \"様: \" \\* Upper"));
        p.runs.push(field("IF \"{ MERGEFIELD 会社 }\" = \"ibis\" \"自社\" \"他社\""));
        doc.body.push(Block::Paragraph(p));
        let mut skip = FlowParagraph { id: 2, ..FlowParagraph::default() };
        skip.runs.push(field("SKIPIF { MERGEFIELD 年齢 } < 10"));
        let mut with_skip = doc.clone();
        with_skip.body.push(Block::Paragraph(skip));
        let values: HashMap<String, String> = [("年齢", "30"), ("氏名", "yamada"), ("会社", "ibis")].map(|(k, v)| (k.to_string(), v.to_string())).into_iter().collect();
        assert_eq!(text_of(&merged_view(&doc, &values)), ["成人様: YAMADA自社"]);
        let young: HashMap<String, String> = [("年齢", "5"), ("氏名", ""), ("会社", "a b")].map(|(k, v)| (k.to_string(), v.to_string())).into_iter().collect();
        assert_eq!(text_of(&merged_view(&doc, &young)), ["未成年他社"]);
        let fields = ["年齢".to_string(), "氏名".to_string(), "会社".to_string()];
        let records = vec![vec!["30".into(), "a".into(), "x".into()], vec!["5".into(), "b".into(), "y".into()], vec!["12".into(), "c".into(), "z".into()]];
        let letters = merged_letters(&with_skip, &fields, &records);
        assert_eq!(letters.body.len(), 4, "the record under 10 is left out");
    }

    #[test]
    fn next_goes_on_to_the_following_record() {
        let mut doc = FlowDocument::default();
        for (id, text) in [(1u32, "MERGEFIELD 氏名"), (2, "NEXT"), (3, "MERGEFIELD 氏名")] {
            let mut p = FlowParagraph { id, ..FlowParagraph::default() };
            p.runs.push(field(text));
            doc.body.push(Block::Paragraph(p));
        }
        let fields = ["氏名".to_string()];
        let records: Vec<Vec<String>> = ["a", "b", "c", "d", "e"].iter().map(|n| vec![n.to_string()]).collect();
        let letters = merged_letters(&doc, &fields, &records);
        // Two records a page: (a, b) (c, d) (e, e): the last has no next record, so it shows its own again.
        assert_eq!(text_of(&letters), ["a", "", "b", "c", "", "d", "e", "", "e"]);
        let breaks = letters.body.iter().filter(|b| matches!(b, Block::Paragraph(p) if p.page_break_before)).count();
        assert_eq!(breaks, 2, "three copies, a page break before the second and the third");
        // NEXTIF holds only for some records.
        let mut doc = FlowDocument::default();
        for (id, text) in [(1u32, "MERGEFIELD 氏名"), (2, "NEXTIF { MERGEFIELD 氏名 } = \"a\""), (3, "MERGEFIELD 氏名")] {
            let mut p = FlowParagraph { id, ..FlowParagraph::default() };
            p.runs.push(field(text));
            doc.body.push(Block::Paragraph(p));
        }
        let letters = merged_letters(&doc, &fields, &records[..3]);
        assert_eq!(text_of(&letters), ["a", "", "b", "c", "", "c"]);
    }

    #[test]
    fn tables_from_text() {
        let (fields, records) = parse_table("氏名,住所\n山田,\"東京都, 港区\"\n鈴木,大阪\n\n");
        assert_eq!(fields, ["氏名", "住所"]);
        assert_eq!(records, [["山田", "東京都, 港区"], ["鈴木", "大阪"]]);
        let (fields, records) = parse_table("a\tb\tc\n1\t2\n");
        assert_eq!((fields.len(), records), (3, vec![vec!["1".to_string(), "2".to_string(), String::new()]]));
    }

    #[test]
    fn envelope_preset_and_blocks_generation() {
        for preset in EnvelopePreset::ALL {
            assert!(!preset.id().is_empty());
            assert!(!preset.label().is_empty());
            assert_eq!(EnvelopePreset::from_id(preset.id()), Some(preset));
            let s = preset.size();
            assert!(s.width > 0 && s.height > 0);
            let ps = preset.page_setup();
            assert_eq!(ps.size, s);
        }
        // Chou3: portrait
        let mut next_id = 100;
        let blocks = envelope_blocks(EnvelopePreset::Chou3, Some("差出人\n東京都渋谷区"), "<氏名> 様", &mut next_id);
        assert_eq!(blocks.len(), 2, "delivery address and return address");
        let Block::Paragraph(delivery) = &blocks[0] else { panic!() };
        assert!(delivery.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Field { instruction, .. } if instruction.contains("ADDRESSBLOCK"))));
        let Block::Paragraph(sender) = &blocks[1] else { panic!() };
        assert!(sender.text().contains("差出人") && sender.text().contains("東京都渋谷区"));
        // DL: landscape
        let mut next_id = 200;
        let blocks_dl = envelope_blocks(EnvelopePreset::Dl, Some("Sender Info"), "<氏名> 様", &mut next_id);
        assert_eq!(blocks_dl.len(), 2);
        let Block::Paragraph(first) = &blocks_dl[0] else { panic!() };
        assert!(first.text().contains("Sender Info"), "sender is at top-left on landscape");

        // Merge letters with envelope blocks
        let doc = FlowDocument { body: blocks, ..FlowDocument::default() };
        let fields = vec!["氏名".to_string()];
        let records = vec![vec!["山田太郎".to_string()], vec!["鈴木花子".to_string()]];
        let merged = merged_letters(&doc, &fields, &records);
        assert_eq!(merged.body.len(), 4, "2 envelopes of 2 blocks each");
    }

    #[test]
    fn label_preset_and_sheet_table_generation() {
        for preset in LabelPreset::ALL {
            assert!(!preset.id().is_empty());
            assert!(!preset.label().is_empty());
            assert_eq!(LabelPreset::from_id(preset.id()), Some(preset));
            assert_eq!(preset.count(), preset.rows() * preset.cols());
            assert!(preset.cell_width() > 0 && preset.cell_height() > 0);
            let ps = preset.page_setup();
            assert!(ps.size.width > 0 && ps.size.height > 0);
        }

        // A4 24面 table
        let mut next_id = 500;
        let table = label_sheet_table(LabelPreset::A4_24, "<氏名>", &mut next_id);
        assert_eq!(table.rows.len(), 8);
        assert_eq!(table.columns.len(), 3);
        assert_eq!(table.rows[0].cells.len(), 3);
        assert!(table.rows[0].cant_split);

        // Cell (0, 0) has 1 field: ADDRESSBLOCK
        let Block::Paragraph(p0) = &table.rows[0].cells[0].blocks[0] else { panic!() };
        assert_eq!(p0.runs.len(), 1);
        assert!(matches!(&p0.runs[0].kind, FlowRunKind::Field { instruction, .. } if instruction.contains("ADDRESSBLOCK")));

        // Cell (0, 1) has 2 fields: NEXT and ADDRESSBLOCK
        let Block::Paragraph(p1) = &table.rows[0].cells[1].blocks[0] else { panic!() };
        assert_eq!(p1.runs.len(), 2);
        assert!(matches!(&p1.runs[0].kind, FlowRunKind::Field { instruction, .. } if instruction.contains("NEXT")));
        assert!(matches!(&p1.runs[1].kind, FlowRunKind::Field { instruction, .. } if instruction.contains("ADDRESSBLOCK")));

        // Merge 25 records into A4 24-label sheet -> should generate 2 sheets (pages)
        let doc = FlowDocument { body: vec![Block::Table(table)], ..FlowDocument::default() };
        let fields = vec!["氏名".to_string()];
        let records: Vec<Vec<String>> = (0..25).map(|i| vec![format!("Person {i}")]).collect();
        let merged = merged_letters(&doc, &fields, &records);

        // Find all rendered texts in tables
        let mut label_texts = Vec::new();
        for b in &merged.body {
            if let Block::Table(t) = b {
                for r in &t.rows {
                    for c in &r.cells {
                        for cb in &c.blocks {
                            if let Block::Paragraph(p) = cb {
                                label_texts.push(p.text().trim().to_string());
                            }
                        }
                    }
                }
            }
        }
        // First 24 labels on sheet 1: Person 0 to Person 23
        for i in 0..24 {
            assert_eq!(label_texts[i], format!("Person {i}"), "label {i} matches record {i}");
        }
        // Label 24 (cell 0, 0 of sheet 2): Person 24
        assert_eq!(label_texts[24], "Person 24");
    }

    #[test]
    fn field_mappings_and_auto_match() {
        let fields: Vec<String> = vec![
            "ZipCode".into(),
            "Prefecture".into(),
            "City".into(),
            "Street".into(),
            "Bldg".into(),
            "Corp".into(),
            "FamilyName".into(),
            "GivenName".into(),
            "Phone".into(),
            "Email".into(),
        ];
        let map = auto_match_fields(&fields);
        assert_eq!(map.len(), 10);
        let find_map = |mapped: &str| map.iter().find(|m| m.mapped_name == mapped).map(|m| (m.name.as_str(), m.column));
        assert_eq!(find_map("Postal Code"), Some(("ZipCode", 0)));
        assert_eq!(find_map("State"), Some(("Prefecture", 1)));
        assert_eq!(find_map("City"), Some(("City", 2)));
        assert_eq!(find_map("Address 1"), Some(("Street", 3)));
        assert_eq!(find_map("Address 2"), Some(("Bldg", 4)));
        assert_eq!(find_map("Company"), Some(("Corp", 5)));
        assert_eq!(find_map("Last Name"), Some(("FamilyName", 6)));
        assert_eq!(find_map("First Name"), Some(("GivenName", 7)));
        assert_eq!(find_map("Business Phone"), Some(("Phone", 8)));
        assert_eq!(find_map("Email Address"), Some(("Email", 9)));

        // Test address & greeting formatting with mapping
        let addr_fmt = default_address_format_with_mapping(&fields, &map);
        assert_eq!(addr_fmt, "〒<ZipCode>\\n<Prefecture><City><Street>\\n<Bldg>\\n<Corp>\\n<FamilyName> <GivenName> 様");
        let greet_fmt = default_greeting_format_with_mapping(&fields, &map);
        assert_eq!(greet_fmt, "<FamilyName> <GivenName> 様");

        // Test value resolution
        let record = vec![
            "100-0001".into(),
            "東京都".into(),
            "千代田区".into(),
            "千代田1-1".into(),
            "101号室".into(),
            "アイビス".into(),
            "山田".into(),
            "太郎".into(),
            "03-1234-5678".into(),
            "taro@example.com".into(),
        ];
        let values: HashMap<String, String> = fields.iter().cloned().zip(record.iter().cloned()).collect();
        assert_eq!(resolve_merge_value("ZipCode", &values, &map).as_deref(), Some("100-0001"));
        assert_eq!(resolve_merge_value("Postal Code", &values, &map).as_deref(), Some("100-0001"));
        assert_eq!(resolve_merge_value("郵便番号", &values, &map).as_deref(), Some("100-0001"));
        assert_eq!(resolve_merge_value("Last Name", &values, &map).as_deref(), Some("山田"));
        assert_eq!(resolve_merge_value("姓", &values, &map).as_deref(), Some("山田"));
        assert_eq!(resolve_merge_value("氏名", &values, &map).as_deref(), Some("山田"));

        // Test merged_view with field mapping
        let mut doc = FlowDocument {
            mail_merge_field_map: map.clone(),
            ..FlowDocument::default()
        };
        let mut p = FlowParagraph { id: 1, ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(FlowRunKind::Field { instruction: "MERGEFIELD \"Postal Code\"".into(), shown: String::new() }, Default::default()));
        p.runs.push(FlowRun::new(FlowRunKind::Text(" ".into()), Default::default()));
        p.runs.push(FlowRun::new(FlowRunKind::Field { instruction: "MERGEFIELD 郵便番号".into(), shown: String::new() }, Default::default()));
        doc.body.push(Block::Paragraph(p));

        let view = merged_view(&doc, &values);
        let text: String = view.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect();
        assert_eq!(text, "100-0001 100-0001");
    }

    #[test]
    fn test_merge_print_range_selection_and_generation() {
        let fields = vec!["氏名".to_string(), "会社".to_string()];
        let records = vec![
            vec!["山田太郎".to_string(), "アイビス".to_string()],
            vec!["鈴木花子".to_string(), "オープンオフィス".to_string()],
            vec!["佐藤一郎".to_string(), "テストラボ".to_string()],
        ];

        let mut doc = FlowDocument::default();
        let mut p = FlowParagraph { id: 1, ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(FlowRunKind::Field { instruction: "MERGEFIELD 氏名".into(), shown: String::new() }, Default::default()));
        p.runs.push(FlowRun::new(FlowRunKind::Text(" 殿 (".into()), Default::default()));
        p.runs.push(FlowRun::new(FlowRunKind::Field { instruction: "MERGEFIELD 会社".into(), shown: String::new() }, Default::default()));
        p.runs.push(FlowRun::new(FlowRunKind::Text(")".into()), Default::default()));
        doc.body.push(Block::Paragraph(p));

        // 1. All records
        let (all_doc, count) = merged_letters_for_range(&doc, &fields, &records, MergePrintRange::All, 0).unwrap();
        assert_eq!(count, 3);
        let texts: Vec<String> = all_doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect();
        assert_eq!(texts, vec!["山田太郎 殿 (アイビス)", "鈴木花子 殿 (オープンオフィス)", "佐藤一郎 殿 (テストラボ)"]);

        // 2. Current record (index 1 = 鈴木花子)
        let (cur_doc, count) = merged_letters_for_range(&doc, &fields, &records, MergePrintRange::Current, 1).unwrap();
        assert_eq!(count, 1);
        let texts: Vec<String> = cur_doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect();
        assert_eq!(texts, vec!["鈴木花子 殿 (オープンオフィス)"]);

        // 3. FromTo range: 2..=3 (1-based: 鈴木花子, 佐藤一郎)
        let (range_doc, count) = merged_letters_for_range(&doc, &fields, &records, MergePrintRange::FromTo(2, 3), 0).unwrap();
        assert_eq!(count, 2);
        let texts: Vec<String> = range_doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect();
        assert_eq!(texts, vec!["鈴木花子 殿 (オープンオフィス)", "佐藤一郎 殿 (テストラボ)"]);

        // 4. Invalid ranges
        assert!(merged_letters_for_range(&doc, &fields, &records, MergePrintRange::FromTo(0, 2), 0).is_err());
        assert!(merged_letters_for_range(&doc, &fields, &records, MergePrintRange::FromTo(3, 1), 0).is_err());
        assert!(merged_letters_for_range(&doc, &fields, &records, MergePrintRange::FromTo(10, 12), 0).is_err());
        assert!(merged_letters_for_range(&doc, &fields, &records, MergePrintRange::Current, 5).is_err());
        assert!(merged_letters_for_range(&doc, &fields, &[], MergePrintRange::All, 0).is_err());
    }
}
