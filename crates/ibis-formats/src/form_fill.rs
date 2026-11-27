// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #270: the form fields of a document on the screen. A field is a plain-text content control tagged
//! `ibis.form.<ID>` (the same tag `form_template` fills); here they are listed, set from the input pane and written out as a
//! record. Nothing here draws; the document screen calls these.

use ibis_doc_model::{ControlInfo, FlowCommand, FlowDocument, FlowPos, FlowRunKind, flow_command};
use serde_json::{Map, Value, json};

use crate::{Error, form_data};

/// The tag of a form field starts with this, followed by the field ID.
pub const TAG_PREFIX: &str = "ibis.form.";

/// One field of the form: all the places of a field ID in the document are one item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormItem {
    pub id: String,
    /// The control's title (what a person calls the field), empty when it has none.
    pub title: String,
    /// What the first place shows; empty while it still shows the prompt.
    pub text: String,
    /// How many places of the document the field is in.
    pub places: usize,
}

/// Whether `id` is a field ID `form_template` accepts: ASCII letters, digits, `.`, `_` and `-`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The plain-text controls tagged `ibis.form.*` as (paragraph, position, field ID), in the order of the document (body and tables).
fn places(doc: &FlowDocument) -> Vec<(u32, usize, String)> {
    let mut out = Vec::new();
    for p in flow_command::all_paragraphs(doc) {
        let mut at = 0;
        for run in &p.runs {
            if matches!(run.kind, FlowRunKind::PlainText { .. })
                && let Some(id) = run.control.as_ref().and_then(|c| c.tag.strip_prefix(TAG_PREFIX))
                && valid_id(id)
            {
                out.push((p.id, at, id.to_owned()));
            }
            at += run.len();
        }
    }
    out
}

/// The fields of the document, once for each ID, in the order of the first place.
pub fn items(doc: &FlowDocument) -> Vec<FormItem> {
    let mut items: Vec<FormItem> = Vec::new();
    for (para, at, id) in places(doc) {
        if let Some(item) = items.iter_mut().find(|i| i.id == id) {
            item.places += 1;
            continue;
        }
        let run = flow_command::paragraph(doc, para).and_then(|p| {
            let mut offset = 0;
            p.runs.iter().find(|r| {
                let here = offset == at;
                offset += r.len();
                here && matches!(r.kind, FlowRunKind::PlainText { .. })
            })
        });
        let (title, text) = run.map_or_else(Default::default, |r| {
            let text = match &r.kind {
                FlowRunKind::PlainText { text, placeholder: false } => text.clone(),
                _ => String::new(),
            };
            (r.control.as_ref().map(|c| c.title.clone()).unwrap_or_default(), text)
        });
        items.push(FormItem { id, title, text, places: 1 });
    }
    items
}

/// The commands that put `value` in every place of field `id` (empty is empty text, not the prompt).
pub fn set_commands(doc: &FlowDocument, id: &str, value: &str) -> Vec<FlowCommand> {
    set_many_commands(doc, &[(id.to_owned(), value.to_owned())])
}

/// The commands that put each value in every place of its field (IDs the document lacks are skipped). The places are taken from the last to
/// the first, so that the text one place gets does not move the offset of a later place in the same paragraph. Run together, they are one step.
pub fn set_many_commands(doc: &FlowDocument, values: &[(String, String)]) -> Vec<FlowCommand> {
    places(doc)
        .into_iter()
        .rev()
        .filter_map(|(para, at, i)| {
            let (_, value) = values.iter().find(|(id, _)| *id == i)?;
            Some(FlowCommand::SetContentControlText {
                at: FlowPos::new(para, at),
                text: value.replace("\r\n", "\n").replace('\r', "\n"),
                placeholder: false,
            })
        })
        .collect()
}

/// The command that makes the plain-text control at `at` (its first position) the form field `id`; the title and the lock stay.
pub fn tag_command(doc: &FlowDocument, at: FlowPos, id: &str) -> Result<FlowCommand, Error> {
    if !valid_id(id) {
        return Err(Error::new("A field ID is made of ASCII letters, digits, '.', '_' and '-'"));
    }
    let p = flow_command::paragraph(doc, at.para).ok_or_else(|| Error::new("No such paragraph"))?;
    let mut offset = 0;
    for run in &p.runs {
        let len = run.len();
        if matches!(run.kind, FlowRunKind::PlainText { .. }) && at.offset >= offset && at.offset <= offset + len {
            let mut info = run.control.as_deref().cloned().unwrap_or_else(ControlInfo::default);
            info.tag = format!("{TAG_PREFIX}{id}");
            return Ok(FlowCommand::SetControlInfo { at: FlowPos::new(at.para, offset), info: Some(Box::new(info)) });
        }
        offset += len;
    }
    Err(Error::new("The caret is not in a plain-text content control"))
}

/// The record of the fields as `form_template::fill` reads it: `{"form": {id, version}, "data": {field: text}}`.
pub fn record(items: &[FormItem], form_id: &str, version: u64) -> Value {
    let data: Map<String, Value> = items.iter().map(|i| (i.id.clone(), Value::String(i.text.clone()))).collect();
    json!({"form": {"id": form_id, "version": version}, "data": data})
}

/// The (field ID, text) pairs of a record's `data` (what `records_from_csv` gives for a row); the fields the form has and the record
/// lacks are left to the caller.
pub fn record_values(record: &Value) -> Vec<(String, String)> {
    record["data"].as_object().map_or_else(Vec::new, |data| {
        data.iter().map(|(k, v)| (k.clone(), v.as_str().map_or_else(|| v.to_string(), str::to_owned))).collect()
    })
}

/// The record as indented JSON text (what `docx_form fill` reads).
pub fn record_json(items: &[FormItem], form_id: &str, version: u64) -> Result<Vec<u8>, Error> {
    serde_json::to_vec_pretty(&record(items, form_id, version)).map_err(|e| Error::new(e.to_string()))
}

/// The same record as one CSV row under a header (the columns `docx_form fill-csv` reads).
pub fn record_csv(items: &[FormItem], form_id: &str, version: u64) -> Result<String, Error> {
    let definition = json!({
        "form": {"id": form_id, "version": version},
        "fields": items.iter().map(|i| (i.id.clone(), json!("text"))).collect::<Map<_, _>>(),
    });
    form_data::records_to_csv(&definition, &[record(items, form_id, version)])
}

/// The fields whose text in the document is longer than the `maxChars` of their definition, with the violation; the writing of a record
/// (CSV, JSON) is refused while there is one (#270).
pub fn violations(items: &[FormItem], specs: &std::collections::BTreeMap<String, crate::form_template::FieldSpec>) -> Vec<(String, crate::form_template::Violation)> {
    items
        .iter()
        .filter_map(|i| specs.get(&i.id)?.check_length(&i.text).err().map(|v| (i.id.clone(), v)))
        .collect()
}
