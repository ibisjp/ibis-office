// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Form definitions and records as YAML, JSON or CSV (#270).
//!
//! YAML and JSON describe one record with its nesting; CSV carries flat fields in bulk: the
//! first row names the columns (`form.id`, `form.version`, then field IDs) and every
//! following row is one record.

use serde_json::{Map, Value, json};

use crate::Error;

/// Parse a form definition or a record from JSON or YAML (JSON is valid YAML, so one parser
/// covers both; a value that is not a mapping is refused).
pub fn parse_structured(text: &str) -> Result<Value, Error> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let value: Value = serde_saphyr::from_str(text).map_err(|e| Error::new(e.to_string()))?;
    if !value.is_object() {
        return Err(Error::new("Form data must be a mapping"));
    }
    Ok(value)
}

/// Split RFC 4180 CSV text into rows. A row of a single empty field (a blank line) is dropped.
fn csv_rows(text: &str) -> Result<Vec<Vec<String>>, Error> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut was_quoted = false;
    let mut chars = text.chars().peekable();
    let end_row = |row: &mut Vec<String>, field: &mut String, rows: &mut Vec<Vec<String>>| {
        row.push(std::mem::take(field));
        let blank = row.len() == 1 && row[0].is_empty();
        let done = std::mem::take(row);
        if !blank {
            rows.push(done);
        }
    };
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() && !was_quoted => {
                quoted = true;
                was_quoted = true;
            }
            '"' => return Err(Error::new("CSV has a quote inside an unquoted field")),
            ',' => {
                row.push(std::mem::take(&mut field));
                was_quoted = false;
            }
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                end_row(&mut row, &mut field, &mut rows);
                was_quoted = false;
            }
            _ if was_quoted => return Err(Error::new("CSV has text after a closing quote")),
            _ => field.push(c),
        }
    }
    if quoted {
        return Err(Error::new("CSV has an unterminated quote"));
    }
    if !field.is_empty() || !row.is_empty() {
        end_row(&mut row, &mut field, &mut rows);
    }
    Ok(rows)
}

/// Read CSV records: columns `form.id` and `form.version`, then one column per field ID. Every
/// row becomes `{"form": {"id", "version"}, "data": {field: text}}`. Values stay text, as the
/// fields of the form definition are text or dates.
pub fn records_from_csv(text: &str) -> Result<Vec<Value>, Error> {
    let rows = csv_rows(text)?;
    let (header, body) = rows
        .split_first()
        .ok_or_else(|| Error::new("CSV is empty"))?;
    let column = |name: &str| {
        let mut found = header.iter().enumerate().filter(|(_, h)| *h == name);
        let first = found.next().map(|(i, _)| i);
        if found.next().is_some() {
            Err(Error::new(format!("CSV has the column {name} twice")))
        } else {
            first.ok_or_else(|| Error::new(format!("CSV has no {name} column")))
        }
    };
    let (id_col, version_col) = (column("form.id")?, column("form.version")?);
    let mut seen = std::collections::BTreeSet::new();
    for (i, name) in header.iter().enumerate() {
        if i != id_col && i != version_col && (name.is_empty() || !seen.insert(name.as_str())) {
            return Err(Error::new(format!("CSV has an empty or repeated column name: {name:?}")));
        }
    }
    let mut records = Vec::with_capacity(body.len());
    for (n, row) in body.iter().enumerate() {
        if row.len() != header.len() {
            return Err(Error::new(format!(
                "CSV row {} has {} fields, the header has {}",
                n + 2,
                row.len(),
                header.len()
            )));
        }
        let version: u64 = row[version_col]
            .parse()
            .map_err(|_| Error::new(format!("CSV row {}: form.version is not a number", n + 2)))?;
        let mut data = Map::new();
        for (i, name) in header.iter().enumerate() {
            if i != id_col && i != version_col {
                data.insert(name.clone(), Value::String(row[i].clone()));
            }
        }
        records.push(json!({
            "form": {"id": row[id_col], "version": version},
            "data": data,
        }));
    }
    Ok(records)
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// Write records as CSV (the inverse of [`records_from_csv`]). The columns are the form's ID and
/// version, then the field IDs of `definition` in order; `records` that do not name this form or
/// miss a field are refused. With no records, the output is the header row (a blank sheet to fill).
pub fn records_to_csv(definition: &Value, records: &[Value]) -> Result<String, Error> {
    let form = &definition["form"];
    let id = form["id"]
        .as_str()
        .ok_or_else(|| Error::new("Form definition has no form ID"))?;
    let version = form["version"]
        .as_u64()
        .ok_or_else(|| Error::new("Form definition has no version"))?;
    let fields: Vec<&String> = definition["fields"]
        .as_object()
        .ok_or_else(|| Error::new("Missing field definitions"))?
        .keys()
        .collect();
    let mut out = String::from("form.id,form.version");
    for f in &fields {
        out.push(',');
        out.push_str(&csv_field(f));
    }
    out.push('\n');
    for record in records {
        if record["form"] != *form {
            return Err(Error::new("Record references a different form ID or version"));
        }
        let data = record["data"]
            .as_object()
            .ok_or_else(|| Error::new("Missing record data"))?;
        out.push_str(&format!("{},{version}", csv_field(id)));
        for f in &fields {
            let v = data
                .get(f.as_str())
                .and_then(Value::as_str)
                .ok_or_else(|| Error::new(format!("Record has no text for {f}")))?;
            out.push(',');
            out.push_str(&csv_field(v));
        }
        out.push('\n');
    }
    Ok(out)
}
