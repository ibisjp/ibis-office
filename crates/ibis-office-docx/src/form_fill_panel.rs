// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #270: the input pane of a form document. The plain-text content controls tagged `ibis.form.<ID>` are the form's fields
//! (`ibis_formats::form_fill`); the pane lists them, one box for each ID, and what is typed goes into every place of the ID. It also tags the
//! control at the caret as a field, and writes the input out as a record (CSV or JSON, the shape `docx_form fill` reads).

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_formats::form_fill;
use ibis_formats::form_template::{FieldSpec, Violation};
use std::collections::{BTreeMap, BTreeSet};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// What the pane keeps: the boxes (one for each field ID, in the order of the document) and the two names it takes.
pub(crate) struct FormFillState {
    /// The field IDs the boxes are for; when the document's list differs, the boxes are made again.
    ids: Vec<String>,
    boxes: Vec<FieldBox>,
    /// The text each field had in the document at the last sync (a box is set from the document only when it changed).
    synced: Vec<String>,
    /// The ID that 項目にする gives the control at the caret.
    pub(crate) tag_id: Entity<InputState>,
    /// The form ID of the record written out.
    pub(crate) form_id: Entity<InputState>,
    box_subscriptions: Vec<Subscription>,
    /// The `type`, `maxChars` and `format` of the fields, from the form definition read in (#270); empty until one is read.
    specs: BTreeMap<String, FieldSpec>,
    /// What was typed in a box of a `number` field (the document shows it with the format; the record is written with this).
    raw: BTreeMap<String, String>,
    /// The `number` fields whose box holds something that is not a number.
    not_number: BTreeSet<String>,
    /// The rows of a CSV read in (each one a record) and the one shown in the boxes.
    records: Vec<Vec<(String, String)>>,
    row: usize,
    /// The text of the CSV the rows came from (the documents are made from it).
    csv: String,
}

/// The box of a field: a text field is a box of several lines, a `number` field a line (what is typed is a number; the document gets
/// the text the format makes of it).
#[derive(Clone)]
pub(crate) enum FieldBox {
    Text(Entity<TextareaState>),
    Number(Entity<InputState>),
}

impl FieldBox {
    fn value(&self, cx: &App) -> String {
        match self {
            Self::Text(b) => b.read(cx).value().to_string(),
            Self::Number(b) => b.read(cx).value().to_string(),
        }
    }

    fn set(&self, text: String, window: &mut Window, cx: &mut App) {
        match self {
            Self::Text(b) => b.update(cx, |s, cx| s.set_value(text, window, cx)),
            Self::Number(b) => b.update(cx, |s, cx| s.set_value(text, window, cx)),
        }
    }
}

/// What a person is told about a field: its text is longer than `maxChars`, or a `number` box does not hold a number.
fn problem_text(violation: &Violation) -> String {
    match violation {
        Violation::TooLong { chars, max } => crate::shell::i18n::format("docx.form-fill.too-long", &[("chars", &chars.to_string()), ("max", &max.to_string())]),
        Violation::Invalid => crate::shell::i18n::text("docx.form-fill.not-number").to_string(),
    }
}

/// A form ID from a file name: ASCII letters, digits, `.`, `_` and `-` only.
fn form_id_from_name(stem: &str) -> String {
    let id: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' }).collect();
    let id = id.trim_matches('-');
    if id.is_empty() { "form".to_owned() } else { id.to_owned() }
}

/// The documents of the rows of `csv`, from the file `template` and the definition file, as `record-N.docx` in `folder` (a file that is
/// there is not overwritten). Returns how many were written.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn write_documents(template: &std::path::Path, definition: &std::path::Path, csv: &str, folder: &std::path::Path) -> Result<usize, String> {
    use std::io::Write as _;
    let read = |path: &std::path::Path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
    let definition = String::from_utf8_lossy(&read(definition)?).into_owned();
    let documents = ibis_formats::form_template::fill_csv(&read(template)?, &definition, csv).map_err(|e| e.to_string())?;
    for (n, bytes) in documents.iter().enumerate() {
        let path = folder.join(format!("record-{}.docx", n + 1));
        std::fs::OpenOptions::new().write(true).create_new(true).open(&path).and_then(|mut f| f.write_all(bytes)).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(documents.len())
}

#[ibis_office_macros::editor_ext(FormFillPanel)]
impl SlideEditor {
    /// 表示 > 入力フォーム: open the pane.
    pub fn open_form_fill(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.doc_state().is_none_or(|s| s.form_fill.is_some()) {
            self.sidebar_open = true;
            cx.notify();
            return;
        }
        self.end_text_edit(window, cx);
        let (stem, _) = self.name_parts();
        let tag_id = crate::shell::chrome::ShownText::name_input("", window, cx);
        let form_id = crate::shell::chrome::ShownText::name_input(form_id_from_name(&stem), window, cx);
        if let Some(state) = self.doc_state_mut() {
            state.tab_edit = None;
            state.find = None;
            state.link = None;
            state.ruby = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.text_box = None;
            state.merge = None;
            state.table_size = None;
            state.comments = None;
            state.control = None;
            state.quick_parts = None;
            state.selection_pane = false;
            state.accessibility_panel = false;
            state.form_fill = Some(FormFillState { ids: Vec::new(), boxes: Vec::new(), synced: Vec::new(), tag_id, form_id, box_subscriptions: Vec::new(), specs: BTreeMap::new(), raw: BTreeMap::new(), not_number: BTreeSet::new(), records: Vec::new(), row: 0, csv: String::new() });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Before the pane is drawn: the boxes show what the document has (Undo and redo change it under them), and are made again when the
    /// fields of the document differ from the boxes.
    pub(crate) fn form_fill_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(items) = self.form_items() else { return };
        let Some(state) = self.doc_state().and_then(|s| s.form_fill.as_ref()) else { return };
        let ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
        if state.ids == ids {
            let changes: Vec<(FieldBox, String)> = state
                .boxes
                .iter()
                .zip(&state.synced)
                .zip(&items)
                .filter(|((_, last), item)| **last != item.text)
                .filter_map(|((b, _), item)| match b {
                    FieldBox::Text(_) => Some((b.clone(), item.text.clone())),
                    // A number box that already makes the document's text (the person typed it) is left as typed; after an Undo it shows
                    // the number the typed text had, or else the document's text.
                    FieldBox::Number(_) => {
                        let spec = state.specs.get(&item.id)?;
                        if spec.shown(&b.value(cx)).is_ok_and(|t| t == item.text) {
                            return None;
                        }
                        let raw = state.raw.get(&item.id).filter(|r| spec.shown(r).is_ok_and(|t| t == item.text));
                        Some((b.clone(), raw.cloned().unwrap_or_else(|| item.text.clone())))
                    }
                })
                .collect();
            for (b, text) in &changes {
                if b.value(cx) != *text {
                    b.set(text.clone(), window, cx);
                }
            }
            if let Some(state) = self.doc_state_mut().and_then(|s| s.form_fill.as_mut()) {
                state.synced = items.into_iter().map(|i| i.text).collect();
            }
            return;
        }
        let mut boxes = Vec::new();
        let mut subscriptions = Vec::new();
        for item in &items {
            let text = item.text.clone();
            let id = item.id.clone();
            if state.specs.get(&item.id).is_some_and(|s| s.kind == "number") {
                let field = crate::shell::chrome::ShownText::name_input(state.raw.get(&item.id).cloned().unwrap_or(text), window, cx);
                subscriptions.push(cx.subscribe_in(&field, window, move |this, field, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = field.read(cx).value().to_string();
                        this.form_fill_write_number(&id, &value, cx);
                    }
                }));
                boxes.push(FieldBox::Number(field));
                continue;
            }
            let field = cx.new(|cx| TextareaState::new(window, cx).default_value(text));
            subscriptions.push(cx.subscribe_in(&field, window, move |this, field, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = field.read(cx).value().to_string();
                    this.form_fill_write(&id, &value, cx);
                }
            }));
            boxes.push(FieldBox::Text(field));
        }
        if let Some(state) = self.doc_state_mut().and_then(|s| s.form_fill.as_mut()) {
            state.ids = ids;
            state.boxes = boxes;
            state.synced = items.into_iter().map(|i| i.text).collect();
            state.box_subscriptions = subscriptions;
        }
    }

    /// The fields of the document (None without a document).
    pub(crate) fn form_items(&self) -> Option<Vec<form_fill::FormItem>> {
        self.flow().map(|doc| form_fill::items(doc))
    }

    /// What is typed in a box goes into every place of its ID (one Undo step); a box that shows the document's text writes nothing.
    pub(crate) fn form_fill_write(&mut self, id: &str, value: &str, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let value = value.replace("\r\n", "\n").replace('\r', "\n");
        if form_fill::items(doc).iter().find(|i| i.id == id).is_none_or(|i| i.text == value) {
            return;
        }
        let commands = form_fill::set_commands(doc, id, &value);
        self.run_flow(commands, cx);
    }

    /// What is typed in the box of a `number` field: the document gets the text the format makes of the number (every place of the ID, one
    /// Undo step). Something that is not a number leaves the document as it is and is told in the pane; an empty box empties the field.
    pub(crate) fn form_fill_write_number(&mut self, id: &str, typed: &str, cx: &mut Context<Self>) {
        let Some(spec) = self.doc_state().and_then(|s| s.form_fill.as_ref()).and_then(|f| f.specs.get(id)).cloned() else { return };
        let shown = if typed.trim().is_empty() { Ok(String::new()) } else { spec.shown(typed) };
        if let Some(state) = self.doc_state_mut().and_then(|s| s.form_fill.as_mut()) {
            if shown.is_ok() {
                state.not_number.remove(id);
                state.raw.insert(id.to_owned(), typed.trim().to_owned());
            } else {
                state.not_number.insert(id.to_owned());
            }
        }
        if let Ok(text) = shown {
            self.form_fill_write(id, &text, cx);
        }
        cx.notify();
    }

    /// 定義を読み込み: the form definition (JSON or YAML) gives each field its `type`, `maxChars` and `format`. Returns the status text.
    pub(crate) fn form_fill_load_definition_text(&mut self, text: &str, cx: &mut Context<Self>) -> Result<String, String> {
        let definition = ibis_formats::form_data::parse_structured(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        let specs = ibis_formats::form_template::field_specs(&definition).map_err(|e| e.to_string())?;
        let count = specs.len();
        if let Some(state) = self.doc_state_mut().and_then(|s| s.form_fill.as_mut()) {
            state.specs = specs;
            state.raw.clear();
            state.not_number.clear();
            // The boxes are made again (a number field gets a line).
            state.ids.clear();
        }
        cx.notify();
        Ok(crate::shell::i18n::format("docx.form-fill.definition-loaded", &[("count", &count.to_string())]))
    }

    /// 定義を読み込み: pick the definition file.
    pub(crate) fn form_fill_load_definition(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("docx.form-fill.write-definition")) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let result = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()));
                this.update(cx, |this, cx| {
                    let status = result.and_then(|bytes| this.form_fill_load_definition_text(&String::from_utf8_lossy(&bytes), cx));
                    this.set_status(status.unwrap_or_else(|error| error), cx);
                }).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.set_status(crate::shell::i18n::text("docx.form-fill.csv-native-only").to_string(), cx);
        }
    }

    /// The fields that cannot be written out: text longer than `maxChars`, a number box without a number. Empty when the record can be written.
    pub(crate) fn form_fill_problems(&self) -> Vec<(String, Violation)> {
        let (Some(state), Some(items)) = (self.doc_state().and_then(|s| s.form_fill.as_ref()), self.form_items()) else { return Vec::new() };
        let mut problems = form_fill::violations(&items, &state.specs);
        problems.extend(state.not_number.iter().map(|id| (id.clone(), Violation::Invalid)));
        problems
    }

    /// 項目にする: the plain-text control at the caret becomes the field with the ID of the box.
    pub(crate) fn form_fill_tag(&mut self, cx: &mut Context<Self>) {
        let text = crate::shell::i18n::text;
        let Some(id) = self.doc_state().and_then(|s| s.form_fill.as_ref()).map(|f| f.tag_id.read(cx).value().trim().to_string()) else { return };
        let Some((start, _)) = self.ordered_selection() else { return };
        let command = self.flow().map(|doc| form_fill::tag_command(doc, start, &id));
        match command {
            Some(Ok(command)) => {
                self.run_flow(vec![command], cx);
                self.set_status(text("docx.form-fill.tagged").to_string(), cx);
            }
            Some(Err(_)) if !form_fill::valid_id(&id) => self.set_status(text("docx.form-fill.bad-id").to_string(), cx),
            _ => self.set_status(text("docx.form-fill.not-in-control").to_string(), cx),
        }
    }

    /// CSV を読み込み: the rows of a CSV file (the columns `form.id`, `form.version` and one for each field ID, as the record is written)
    /// become records; the first one fills the boxes, and the buttons step through the rest.
    pub(crate) fn form_fill_load_csv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("_shell.open")) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let result = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()));
                this.update(cx, |this, cx| {
                    let status = result.and_then(|bytes| this.form_fill_load_text(&String::from_utf8_lossy(&bytes), cx));
                    this.set_status(status.unwrap_or_else(|error| error), cx);
                }).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.set_status(crate::shell::i18n::text("docx.form-fill.csv-native-only").to_string(), cx);
        }
    }

    /// 入力済みの文書を書き出し: one DOCX for each row of the CSV read in (`record-1.docx` ...), made from the saved form file with the form
    /// definition the person picks and written to a folder the person picks; the same bytes as `docx_form fill-csv`.
    pub(crate) fn form_fill_write_documents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = crate::shell::i18n::text;
        let Some(template) = self.file_path() else {
            self.set_status(text("docx.form-fill.write-needs-file").to_string(), cx);
            return;
        };
        let csv = self.doc_state().and_then(|s| s.form_fill.as_ref()).map(|f| f.csv.clone()).unwrap_or_default();
        if csv.is_empty() {
            self.set_status(text("docx.form-fill.write-needs-csv").to_string(), cx);
            return;
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let definition = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(text("docx.form-fill.write-definition")) });
            let folder = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, extensions: Vec::new(), multiple: false, prompt: Some(text("docx.form-fill.write-documents")) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(definition))) = definition.await else { return };
                let Ok(Ok(Some(folder))) = folder.await else { return };
                let (Some(definition), Some(folder)) = (definition.into_iter().next(), folder.into_iter().next()) else { return };
                let result = write_documents(&template, &definition, &csv, &folder);
                let _ = this.update(cx, |this, cx| {
                    let status = match result {
                        Ok(count) => crate::shell::i18n::format("docx.form-fill.write-done", &[("count", &count.to_string())]),
                        Err(error) => error,
                    };
                    this.set_status(status, cx);
                });
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (window, template);
            self.set_status(text("docx.form-fill.csv-native-only").to_string(), cx);
        }
    }

    /// Step to another row of the CSV read in (`delta` is 1 or -1; the ends stay) and put its values into the document.
    pub(crate) fn form_fill_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut().and_then(|s| s.form_fill.as_mut()) {
            state.row = state.row.saturating_add_signed(delta).min(state.records.len().saturating_sub(1));
        }
        self.form_fill_show_row(cx);
    }

    /// The CSV text (the columns `form.id`, `form.version` and one for each field ID) becomes the records; the first row fills the fields.
    /// Returns the status text.
    pub(crate) fn form_fill_load_text(&mut self, text: &str, cx: &mut Context<Self>) -> Result<String, String> {
        let records = ibis_formats::form_data::records_from_csv(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        if records.is_empty() {
            return Ok(crate::shell::i18n::text("docx.form-fill.csv-empty").to_string());
        }
        let count = records.len();
        if let Some(state) = self.doc_state_mut().and_then(|s| s.form_fill.as_mut()) {
            state.records = records.iter().map(form_fill::record_values).collect();
            state.row = 0;
            state.csv = text.to_owned();
        }
        self.form_fill_show_row(cx);
        Ok(crate::shell::i18n::format("docx.form-fill.csv-loaded", &[("count", &count.to_string())]))
    }

    /// The values of the current row go into the fields the form has, all in one step of undo; a field the row lacks is kept.
    fn form_fill_show_row(&mut self, cx: &mut Context<Self>) {
        let values = self.doc_state().and_then(|s| s.form_fill.as_ref()).and_then(|f| f.records.get(f.row)).cloned().unwrap_or_default();
        let Some(doc) = self.flow() else { return };
        let current = form_fill::items(doc);
        let changed: Vec<(String, String)> = values
            .into_iter()
            .map(|(id, value)| (id, value.replace("\r\n", "\n").replace('\r', "\n")))
            .filter(|(id, value)| current.iter().any(|i| i.id == *id && i.text != *value))
            .collect();
        let commands = form_fill::set_many_commands(doc, &changed);
        if !commands.is_empty() {
            self.run_flow(commands, cx);
        }
        cx.notify();
    }

    /// 書き出し: the record of the input as CSV (`csv`) or JSON, to a file the person names.
    pub(crate) fn form_fill_export(&mut self, csv: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut items) = self.form_items() else { return };
        let problems = self.form_fill_problems();
        if !problems.is_empty() {
            let ids: Vec<String> = problems.iter().map(|(id, _)| id.clone()).collect();
            self.set_status(crate::shell::i18n::format("docx.form-fill.export-refused", &[("fields", &ids.join(", "))]), cx);
            return;
        }
        // A number is written as it was typed (the document shows it with the format; the record carries the number).
        if let Some(state) = self.doc_state().and_then(|s| s.form_fill.as_ref()) {
            for item in &mut items {
                if let (Some(spec), Some(raw)) = (state.specs.get(&item.id), state.raw.get(&item.id))
                    && spec.shown(raw).is_ok_and(|t| t == item.text)
                {
                    item.text = raw.clone();
                }
            }
        }
        let form_id = self.doc_state().and_then(|s| s.form_fill.as_ref()).map(|f| f.form_id.read(cx).value().trim().to_string()).unwrap_or_default();
        if !form_fill::valid_id(&form_id) {
            self.set_status(crate::shell::i18n::text("docx.form-fill.bad-form-id").to_string(), cx);
            return;
        }
        let bytes = if csv {
            // A byte order mark lets a spreadsheet read the Japanese text.
            form_fill::record_csv(&items, &form_id, 1).map(|t| [b"\xEF\xBB\xBF".as_slice(), t.as_bytes()].concat())
        } else {
            form_fill::record_json(&items, &form_id, 1)
        };
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(error) => {
                self.set_status(error.to_string(), cx);
                return;
            }
        };
        let (stem, _) = self.name_parts();
        let name = format!("{stem}.{}", if csv { "csv" } else { "json" });
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.file_path().and_then(|p| p.parent().map(std::path::Path::to_path_buf)).unwrap_or_else(|| ".".into());
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |editor, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                let result = std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()));
                let _ = editor.update(cx, |editor, cx| {
                    let status = match result {
                        Ok(()) => format!("{}: {}", crate::shell::i18n::text("docx.form-fill.exported"), path.display()),
                        Err(error) => error,
                    };
                    editor.set_status(status, cx);
                });
            }).detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let mime = if csv { "text/csv;charset=utf-8" } else { "application/json" };
            if !self.browser_export(name.clone(), mime, bytes, window, cx) {
                self.set_status(format!("{}: {name}", crate::shell::i18n::text("docx.form-fill.exported")), cx);
            }
        }
    }

    pub fn form_fill_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?.form_fill.as_ref()?;
        let (boxes, tag_id, form_id) = (state.boxes.clone(), state.tag_id.clone(), state.form_id.clone());
        let not_number = state.not_number.clone();
        let violations: BTreeMap<String, Violation> = form_fill::violations(&self.form_items()?, &state.specs).into_iter().collect();
        let (rows, row) = (state.records.len(), state.row);
        let items = self.form_items()?;
        let text = crate::shell::i18n::text;
        let secondary = crate::shell::chrome::text_secondary();
        let mut body = v_flex().gap_2().w_full().min_w_0().px(px(18.)).py(px(8.));
        if items.is_empty() {
            body = body.child(div().text_sm().text_color(secondary).child(text("docx.form-fill.empty")));
        } else {
            body = body.child(div().text_sm().text_color(secondary).child(crate::shell::i18n::format("docx.form-fill.count", &[("count", &items.len().to_string())])));
            for (item, field) in items.iter().zip(&boxes) {
                let label = if item.title.is_empty() { item.id.clone() } else { format!("{} ({})", item.title, item.id) };
                let label = if item.places > 1 { format!("{label} ×{}", item.places) } else { label };
                body = body.child(div().text_sm().child(SharedString::from(label)));
                body = match field {
                    FieldBox::Text(b) => body.child(crate::shell::chrome::text_area_field(b)),
                    FieldBox::Number(b) => body.child(crate::shell::chrome::text_field(b).full_width()),
                };
                let problem = if not_number.contains(&item.id) { Some(Violation::Invalid) } else { violations.get(&item.id).cloned() };
                if let Some(problem) = problem {
                    body = body.child(div().text_sm().text_color(rgb(0xd93025)).child(SharedString::from(problem_text(&problem))));
                }
            }
        }
        body = body
            .child(div().pt_2().text_sm().font_weight(FontWeight::BOLD).child(text("docx.form-fill.define")))
            .child(div().text_sm().text_color(secondary).child(text("docx.form-fill.define-hint")))
            .child(crate::shell::chrome::text_field(&tag_id).full_width())
            .child(
                crate::shell::chrome::action_button("form-fill-tag")
                    .label(text("docx.form-fill.tag"))
                    .small()
                    .on_click(cx.listener(|this, _, _, cx| this.form_fill_tag(cx))),
            )
            .child(
                crate::shell::chrome::action_button("form-fill-load-definition")
                    .label(text("docx.form-fill.load-definition"))
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| this.form_fill_load_definition(window, cx))),
            )
            .child(
                crate::shell::chrome::action_button("form-fill-load-csv")
                    .label(text("docx.form-fill.load-csv"))
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| this.form_fill_load_csv(window, cx))),
            )
            .child(
                crate::shell::chrome::action_button("form-fill-write-documents")
                    .label(text("docx.form-fill.write-documents"))
                    .small()
                    .on_click(cx.listener(|this, _, window, cx| this.form_fill_write_documents(window, cx))),
            )
            .when(rows > 1, |body| {
                body.child(
                    h_flex().gap_2().items_center()
                        .child(crate::shell::chrome::action_button("form-fill-prev").label(text("docx.form-fill.prev")).small().on_click(cx.listener(|this, _, _, cx| this.form_fill_step(-1, cx))))
                        .child(div().text_sm().child(SharedString::from(format!("{} / {rows}", row + 1))))
                        .child(crate::shell::chrome::action_button("form-fill-next").label(text("docx.form-fill.next")).small().on_click(cx.listener(|this, _, _, cx| this.form_fill_step(1, cx)))),
                )
            })
            .child(div().pt_2().text_sm().child(text("docx.form-fill.form-id")))
            .child(crate::shell::chrome::text_field(&form_id).full_width());
        let actions = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("form-fill-csv", text("docx.form-fill.export-csv"), crate::shell::chrome::ActionButtonKind::Normal)
                .on_click(cx.listener(|this, _, window, cx| this.form_fill_export(true, window, cx))))
            .action(crate::shell::chrome::ActionDefinition::new("form-fill-json", text("docx.form-fill.export-json"), crate::shell::chrome::ActionButtonKind::Normal)
                .on_click(cx.listener(|this, _, window, cx| this.form_fill_export(false, window, cx))));
        Some(SidebarPanel::with_footer(text("docx.form-fill.title"), body.into_any_element(), actions))
    }
}
