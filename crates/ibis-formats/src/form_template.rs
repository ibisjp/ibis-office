// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/270)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Populate tagged plain-text fields in a DOCX form template.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use ibis_ooxml::zip::{Archive, Writer};
use roxmltree::{Document, Node};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::Error;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const PREFIX: &str = "ibis.form.";

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children().find(|n| n.has_tag_name((W, name)))
}

struct Field<'a, 'input> {
    id: &'a str,
    control: Node<'a, 'input>,
    run: Node<'a, 'input>,
    text: Node<'a, 'input>,
}

fn fields<'a, 'input>(xml: &'a Document<'input>) -> Result<Vec<Field<'a, 'input>>, Error> {
    let mut fields = Vec::new();
    for node in xml.descendants().filter(|n| n.has_tag_name((W, "sdt"))) {
        let Some(id) = child(node, "sdtPr")
            .and_then(|props| child(props, "tag"))
            .and_then(|tag| tag.attribute((W, "val")))
            .and_then(|tag| tag.strip_prefix(PREFIX))
        else {
            continue;
        };
        let unsupported = || Error::new(format!("Unsupported form field: {id}"));
        let content = child(node, "sdtContent").ok_or_else(unsupported)?;
        let run = child(content, "r").ok_or_else(unsupported)?;
        let text = child(run, "t").ok_or_else(unsupported)?;
        if id.is_empty()
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            || !node
                .parent()
                .is_some_and(|parent| parent.has_tag_name((W, "p")))
            || content.children().filter(Node::is_element).count() != 1
            || run
                .children()
                .filter(Node::is_element)
                .any(|n| n != text && !n.has_tag_name((W, "rPr")))
        {
            return Err(unsupported());
        }
        fields.push(Field {
            id,
            control: node,
            run,
            text,
        });
    }
    if fields.is_empty() {
        return Err(Error::new("No tagged DOCX form fields"));
    }
    Ok(fields)
}

fn package(bytes: &[u8]) -> Result<(Archive, String), Error> {
    let archive = Archive::new(Arc::from(bytes)).map_err(|e| Error::new(e.to_string()))?;
    let mut parts = archive
        .entries()
        .iter()
        .filter(|entry| entry.name == "word/document.xml");
    let document = parts
        .next()
        .ok_or_else(|| Error::new("Missing word/document.xml"))?;
    if parts.next().is_some() || document.size > 16 * 1024 * 1024 {
        return Err(Error::new("Unsupported DOCX main part"));
    }
    let xml = archive
        .read(document)
        .map_err(|e| Error::new(e.to_string()))?;
    let xml = String::from_utf8(xml).map_err(|e| Error::new(e.to_string()))?;
    Ok((archive, xml))
}

/// List the field IDs in the main document, including occurrence counts.
pub fn inspect(bytes: &[u8]) -> Result<BTreeMap<String, usize>, Error> {
    let (_, xml) = package(bytes)?;
    let xml = Document::parse(&xml).map_err(|e| Error::new(e.to_string()))?;
    let mut counts = BTreeMap::new();
    for field in fields(&xml)? {
        *counts.entry(field.id.to_owned()).or_default() += 1;
    }
    Ok(counts)
}

/// What a field of the form definition says (#270): its `type` (`text`, `date-ja`, `number`), the most characters the document may
/// show for it (`maxChars`), and, for a number, the Excel number format code that makes the text of the document (`format`, as in a cell;
/// `General` when absent). In the definition a field is either the type alone as a string (`"text"`) or `{type, maxChars, format}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldSpec {
    pub kind: String,
    pub max_chars: Option<usize>,
    pub format: Option<String>,
}

/// Why a value cannot go in a field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Violation {
    /// The value does not fit the type (not a number, not a date).
    Invalid,
    /// The text of the document would have `chars` characters, more than `max`.
    TooLong { chars: usize, max: usize },
}

impl FieldSpec {
    /// The spec of one entry of `fields`: a type string, or an object with `type` and optionally `maxChars` and `format`.
    pub fn from_json(value: &Value) -> Result<Self, Error> {
        let bad = |what: &str| Error::new(format!("Invalid field definition: {what}"));
        let (kind, max_chars, format) = match value {
            Value::String(kind) => (kind.as_str(), None, None),
            Value::Object(map) => {
                if let Some(key) = map.keys().find(|k| !matches!(k.as_str(), "type" | "maxChars" | "format")) {
                    return Err(bad(key));
                }
                let kind = map.get("type").and_then(Value::as_str).ok_or_else(|| bad("type"))?;
                let max = match map.get("maxChars") {
                    None => None,
                    Some(v) => Some(v.as_u64().filter(|n| *n > 0).and_then(|n| usize::try_from(n).ok()).ok_or_else(|| bad("maxChars"))?),
                };
                let format = match map.get("format") {
                    None => None,
                    Some(v) => Some(v.as_str().ok_or_else(|| bad("format"))?),
                };
                (kind, max, format)
            }
            _ => return Err(bad("a field is a type or an object")),
        };
        if !matches!(kind, "text" | "date-ja" | "number") {
            return Err(bad(kind));
        }
        if format.is_some() && kind != "number" {
            return Err(bad("format is for the type number"));
        }
        Ok(Self { kind: kind.to_owned(), max_chars, format: format.map(str::to_owned) })
    }

    /// The text the document shows for `value` (a number is read as a plain decimal and shown with the format), or why the value is
    /// refused as the wrong type. The length is not looked at; see [`FieldSpec::show`].
    pub fn shown(&self, value: &str) -> Result<String, Violation> {
        Ok(match self.kind.as_str() {
            "text" => value.replace("\r\n", "\n").replace('\r', "\n"),
            "date-ja" => japanese_date(value).ok_or(Violation::Invalid)?,
            _ => {
                let n = parse_number(value).ok_or(Violation::Invalid)?;
                let code = self.format.as_deref().unwrap_or("General");
                ibis_calc::numfmt::format_text_only(&ibis_calc::value::CellValue::Number(n), code, false)
            }
        })
    }

    /// The text the document shows for `value`, or why the value is refused. `maxChars` counts the characters of the shown text, not the
    /// line breaks. A value is never cut short.
    pub fn show(&self, value: &str) -> Result<String, Violation> {
        let text = self.shown(value)?;
        self.check_length(&text)?;
        Ok(text)
    }

    /// Whether the text the document shows is within `maxChars` (line breaks are not counted).
    pub fn check_length(&self, shown: &str) -> Result<(), Violation> {
        match self.max_chars {
            Some(max) => {
                let chars = shown.chars().filter(|c| *c != '\n' && *c != '\r').count();
                if chars > max { Err(Violation::TooLong { chars, max }) } else { Ok(()) }
            }
            None => Ok(()),
        }
    }
}

/// A plain decimal number (`-1234.5`, `1e3`); surrounding blanks are ignored, anything else (a thousands comma, a unit) is not a number.
pub fn parse_number(value: &str) -> Option<f64> {
    let t = value.trim();
    (!t.is_empty() && t.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E')))
        .then(|| t.parse::<f64>().ok())
        .flatten()
        .filter(|n| n.is_finite())
}

/// The specs of the `fields` of a form definition, by ID.
pub fn field_specs(definition: &Value) -> Result<BTreeMap<String, FieldSpec>, Error> {
    definition["fields"]
        .as_object()
        .ok_or_else(|| Error::new("Missing field definitions"))?
        .iter()
        .map(|(id, v)| Ok((id.clone(), FieldSpec::from_json(v).map_err(|e| Error::new(format!("{id}: {e}")))?)))
        .collect()
}

/// Reiwa and Heisei dates (2019-05-01 onward, 1989-01-08 to 2019-04-30) from a
/// strict `YYYY-MM-DD` value; anything else (including dates that do not exist) is refused.
fn japanese_date(value: &str) -> Option<String> {
    let b = value.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let s = value.get(r)?;
        s.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| s.parse().ok())?
    };
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if day == 0 || day > days {
        return None;
    }
    let (era, base) = if (year, month, day) >= (2019, 5, 1) {
        ("令和", 2018)
    } else if (year, month, day) >= (1989, 1, 8) {
        ("平成", 1988)
    } else {
        return None;
    };
    let y = year - base;
    let y = if y == 1 {
        "元".to_owned()
    } else {
        y.to_string()
    };
    Some(format!("{era}{y}年{month}月{day}日"))
}

fn write_text(value: &str) -> String {
    value
        .split('\n')
        .map(|line| {
            let text = line
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            format!(r#"<w:t xmlns:w="{W}" xml:space="preserve">{text}</w:t>"#)
        })
        .collect::<Vec<_>>()
        .join(&format!(r#"<w:br xmlns:w="{W}"/>"#))
}

fn same_form(definition: &Value, data: &Value) -> bool {
    let form = &definition["form"];
    form["id"].as_str().is_some_and(|id| !id.is_empty())
        && form["version"].as_u64().is_some_and(|v| v > 0)
        && *form == data["form"]
}

/// Merge a text form definition and versioned record into DOCX.
pub fn fill(bytes: &[u8], definition: &Value, data: &Value) -> Result<Vec<u8>, Error> {
    if !same_form(definition, data) {
        return Err(Error::new(
            "Record references a different form ID or version",
        ));
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    if definition["templateSha256"].as_str() != Some(digest.as_str()) {
        return Err(Error::new(
            "DOCX template does not match the form definition",
        ));
    }
    let (archive, mut xml) = package(bytes)?;
    let document = Document::parse(&xml).map_err(|e| Error::new(e.to_string()))?;
    let fields = fields(&document)?;
    let specs = field_specs(definition)?;
    let values = data["data"]
        .as_object()
        .ok_or_else(|| Error::new("Missing record data"))?;
    let ids: BTreeSet<&str> = fields.iter().map(|field| field.id).collect();
    if ids != specs.keys().map(String::as_str).collect()
        || ids != values.keys().map(String::as_str).collect()
    {
        return Err(Error::new(
            "Template, definition and record must have the same field IDs",
        ));
    }
    let mut replacements = Vec::with_capacity(fields.len());
    for Field {
        id,
        control,
        run,
        text,
    } in fields
    {
        let replacement = specs[id]
            .show(values[id].as_str().ok_or_else(|| Error::new(format!("Invalid value or field type: {id}")))?)
            .map_err(|v| match v {
                Violation::TooLong { chars, max } => Error::new(format!("Field {id} has {chars} characters, more than maxChars {max}")),
                Violation::Invalid => Error::new(format!("Invalid value or field type: {id}")),
            })
            .and_then(|s| {
                s.chars()
                    .all(|c| matches!(c, '\t' | '\n') || c >= ' ' && !matches!(c, '\u{fffe}' | '\u{ffff}'))
                    .then_some(s)
                    .ok_or_else(|| Error::new(format!("Invalid value or field type: {id}")))
            })?;
        let run_range = run.range();
        let text_range = text.range();
        let mut result = xml[run_range.start..text_range.start].to_owned();
        result.push_str(&write_text(&replacement));
        result.push_str(&xml[text_range.end..run_range.end]);
        replacements.push((control.range(), result));
    }
    for (range, value) in replacements.into_iter().rev() {
        xml.replace_range(range, &value);
    }

    Document::parse(&xml).map_err(|e| Error::new(e.to_string()))?;
    let mut output = Writer::new();
    output.set_comment(archive.comment());
    for entry in archive.entries() {
        if entry.name.starts_with("_xmlsignatures/") {
            return Err(Error::new("Signed DOCX templates are unsupported"));
        }
        let result = if entry.name == "word/document.xml" {
            output.add(&entry.name, entry.meta, xml.as_bytes())
        } else {
            output.copy(&archive, entry)
        };
        result.map_err(|e| Error::new(e.to_string()))?;
    }
    output.finish().map_err(|e| Error::new(e.to_string()))
}

/// One DOCX for each row of the CSV (the columns `form.id`, `form.version` and one for each field ID), filled from the template by the
/// form definition (JSON or YAML text). `docx_form fill-csv` and the input pane write the same bytes.
pub fn fill_csv(bytes: &[u8], definition: &str, csv: &str) -> Result<Vec<Vec<u8>>, Error> {
    let definition = crate::form_data::parse_structured(definition)?;
    crate::form_data::records_from_csv(csv.trim_start_matches('\u{feff}'))?
        .iter()
        .map(|record| fill(bytes, &definition, record))
        .collect()
}
