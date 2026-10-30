// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Mail merge: a four-step live pane and focused dialogs; recipients read from CSV
//! / TSV / Excel file), the 差し込みフィールド to put at the caret, the 結果のプレビュー of one record after
//! another, and 個々のドキュメントの編集 (a new file with one copy of the letter for each record).

use gpui_kit::prelude::FluentBuilder as _;
use std::sync::Arc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::component::{Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::merge::{
    EnvelopePreset, LabelPreset, MergePrintRange, envelope_blocks, label_sheet_table, merge_field_instruction,
    merge_field_placeholder, merge_fields_used, merged_letters, parse_table,
};
use ibis_doc_model::{Container, FlowCommand, FlowPos, FlowRunKind};

use crate::shell::chrome::SidebarPanel;
mod workflow;
pub(crate) use workflow::{MergeDialog, MergeStep, MergeOutput};
use crate::shell::editor::SlideEditor;

/// 差し込み印刷のエラーチェック結果.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MergeCheckResult {
    pub(crate) recipient_count: usize,
    pub(crate) fields_used: Vec<String>,
    pub(crate) missing_fields: Vec<String>,
    pub(crate) empty_record_indices: Vec<usize>,
}

/// The merge's data and what it shows.
pub(crate) struct MergeState {
    /// Whether its dialog is visible; recipient data and preview remain document-scoped.
    pub(crate) visible: bool,
    pub(crate) step: MergeStep,
    pub(crate) dialog: Option<MergeDialog>,
    pub(crate) output: MergeOutput,
    pub(crate) separate: bool,
    pub(crate) source_name: String,
    pub(crate) recipient_backup: Option<workflow::RecipientSnapshot>,
    pub(crate) import: Option<workflow::MergeImport>,
    pub(crate) dialog_return: Option<MergeDialog>,
    pub(crate) selected_row: Option<usize>,
    pub(crate) row_reorder:std::rc::Rc<std::cell::RefCell<crate::shell::chrome::data_table::RowReorder>>,
    pub(crate) selected_field:Option<usize>,
    pub(crate) field_focus:FocusHandle,
    pub(crate) targets_only:bool,
    pub(crate) review_ack:bool,
    pub(crate) review_ack_signature:String,
    pub(crate) replace_body_ack:bool,
    pub(crate) import_names:Vec<Entity<InputState>>,
    pub(crate) import_range:Vec<Entity<InputState>>,
    pub(crate) import_error:Option<String>,
    pub(crate) subscriptions:Vec<Subscription>,
    pub(crate) import_subscriptions:Vec<Subscription>,
    pub(crate) advanced: bool,
    pub(crate) list_search:String,
    pub(crate) kind_draft:Option<&'static str>,
    pub(crate) column_backup: Option<workflow::RecipientSnapshot>,
    pub(crate) output_backup: Option<(MergePrintRange,String,String,bool)>,
    pub(crate) mapping_draft:Vec<ibis_doc_model::merge::FieldMapData>,
    pub(crate) table_sort:Option<(usize,bool)>,
    pub(crate) condition_value: Entity<InputState>,
    pub(crate) condition_yes: Entity<InputState>,
    pub(crate) condition_no: Entity<InputState>,
    pub(crate) condition_column: usize,
    pub(crate) filter_column:usize,
    pub(crate) filter_contains:bool,
    pub row_editor: Option<Option<usize>>,
    pub row_inputs: Vec<Entity<InputState>>,
    /// The 宛先一覧 as text: the first line the field names (comma, tab or semicolon separated).
    pub(crate) text: Entity<TextareaState>,
    pub(crate) fields: Vec<String>,
    pub(crate) records: Vec<Vec<String>>,
    /// The record the preview shows.
    pub(crate) current: usize,
    pub(crate) preview: bool,
    /// 宛先の選択: the records left out are `false` (by record).
    pub(crate) included: Vec<bool>,
    /// 並べ替え: the field and whether ascending.
    pub(crate) sort: Option<(usize, bool)>,
    /// 抽出: the conditions (`項目名=値 AND 項目名~値 OR …`).
    pub(crate) filter: Option<ibis_doc_model::merge::RecordFilter>,
    /// The page of the recipient list shown (`RECORDS_PER_PAGE` records on each).
    pub(crate) list_page: usize,
    pub(crate) sort_field: Entity<InputState>,
    pub(crate) filter_field: Entity<InputState>,
    /// 宛先の追加: the name of a field to add; the record number to go to; the text to find a recipient by.
    pub(crate) new_field: Entity<InputState>,
    pub(crate) goto_field: Entity<InputState>,
    pub(crate) find_field: Entity<InputState>,
    /// 規則（IF など）: the instruction of a field to insert (`IF { MERGEFIELD 年齢 } >= 20 "成人" "未成年"`).
    pub(crate) rule_field: Entity<InputState>,
    /// 封筒サイズと宛名配置 (DOCX-MERGE-016).
    pub(crate) envelope_preset: EnvelopePreset,
    pub(crate) sender_field: Entity<InputState>,
    /// ラベル規格と差込ラベル (DOCX-MERGE-017).
    pub(crate) label_preset: LabelPreset,
    /// フィールドと列の対応 (DOCX-MERGE-009).
    pub(crate) match_fields_open: bool,
    /// 差し込み印刷の印刷範囲 (DOCX-MERGE-014).
    pub(crate) print_range: MergePrintRange,
    pub(crate) print_from_field: Entity<InputState>,
    pub(crate) print_to_field: Entity<InputState>,
}

/// How many records of the list the panel shows at once.
pub(crate) const RECORDS_PER_PAGE: usize = 50;

impl MergeState {
    /// The records merged, as indices into `records`: those chosen (宛先の選択) that pass the filter (宛先の抽出),
    /// in the order of the sort (宛先の並べ替え).
    pub(crate) fn selected(&self) -> Vec<usize> {
        let mut out: Vec<usize> = (0..self.records.len())
            .filter(|&i| self.included.get(i).copied().unwrap_or(true))
            .filter(|&i| self.filter.as_ref().is_none_or(|f| f.matches(&self.records[i])))
            .collect();
        if let Some((field, ascending)) = self.sort {
            let key = |i: &usize| self.records[*i].get(field).cloned().unwrap_or_default();
            out.sort_by(|a, b| {
                // Numbers by value, text by characters.
                let (x, y) = (key(a), key(b));
                let order = match (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
                    (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                    _ => x.cmp(&y),
                };
                if ascending { order } else { order.reverse() }
            });
        }
        out
    }
}

impl SlideEditor {
    fn merge_state(&self) -> Option<&MergeState> {
        self.doc_state()?.merge.as_ref()
    }

    /// ツール > 差し込み印刷…: the panel (its data kept while the document is open).
    pub(crate) fn open_merge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        if self.doc_state().is_some_and(|s| s.merge.is_none()) {
            let text = cx.new(|cx| TextareaState::new(window, cx));
            if let Some(state) = self.doc_state_mut() {
                let sort_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.field-sort")));
                let filter_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("merge.flow.filter-value")));
                let rule_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.rule-placeholder")));
                let new_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.new-field-name")));
                let goto_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.go-to-number")));
                let find_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.find-recipient-placeholder")));
                let sender_field = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.merge-panel.sender-address")));
                let print_from_field = cx.new(|cx| {let mut i=InputState::new(window,cx);i.set_value("1",window,cx);i});
                let print_to_field = cx.new(|cx| {let mut i=InputState::new(window,cx);i.set_value("1",window,cx);i});
                state.merge = Some(MergeState {
                    visible: true, step:MergeStep::Recipients, dialog:None, output:MergeOutput::Document, separate:false,
                    source_name:String::new(),recipient_backup:None,import:None,dialog_return:None,selected_row:None,row_reorder:Default::default(),selected_field:None,field_focus:cx.focus_handle(),targets_only:false,review_ack:false,review_ack_signature:String::new(),replace_body_ack:false,import_names:Vec::new(),import_range:(0..4).map(|_|cx.new(|cx|InputState::new(window,cx))).collect(),import_error:None,subscriptions:Vec::new(),import_subscriptions:Vec::new(),advanced:false,list_search:String::new(),kind_draft:None,
                    column_backup:None,output_backup:None,mapping_draft:Vec::new(),table_sort:None,condition_column:0,filter_column:0,filter_contains:true,
                    condition_value:cx.new(|cx|InputState::new(window,cx)),
                    condition_yes:cx.new(|cx|InputState::new(window,cx)),
                    condition_no:cx.new(|cx|InputState::new(window,cx)),
                    row_editor:None, row_inputs:Vec::new(),
                    text,
                    fields: Vec::new(),
                    records: Vec::new(),
                    current: 0,
                    preview: false,
                    included: Vec::new(),
                    sort: None,
                    filter: None,
                    list_page: 0,
                    sort_field,
                    filter_field,
                    new_field,
                    goto_field,
                    find_field,
                    rule_field,
                    envelope_preset: EnvelopePreset::Chou3,
                    sender_field,
                    label_preset: LabelPreset::A4_24,
                    match_fields_open: false,
                    print_range: MergePrintRange::All,
                    print_from_field,
                    print_to_field,
                });
            }
        }
        if let Some(state) = self.doc_state_mut() {
            if let Some(merge) = state.merge.as_mut() { merge.visible = true; }
            state.find = None;
            state.link = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            state.comments = None;
            state.ruby = None;
            state.text_box = None;
            state.helper = Default::default();
        }
        self.watch_merge_inputs(window,cx);
        self.sidebar_open = true;
        cx.notify();
    }

    /// The panel's text read as the 宛先一覧: the first line its field names, the others its records.
    pub(crate) fn apply_merge_data(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.merge_state().map(|m| m.text.read(cx).value().to_string()) else { return };
        let (fields, records) = parse_table(&text);
        self.set_merge_data(fields, records, cx);
    }

    /// The 宛先一覧: `fields` and the `records` (rows of values).
    pub(crate) fn set_merge_data(&mut self, fields: Vec<String>, records: Vec<Vec<String>>, cx: &mut Context<Self>) {
        let preview = self.merge_state().is_some_and(|m| m.preview);
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.review_ack=false;
            m.fields = fields;
            m.included = vec![true; records.len()];
            m.records = records;
            m.sort = None;
            m.filter = None;
            m.list_page = 0;
            m.list_search.clear();m.table_sort=None;
            m.current = 0;
        }
        if preview {
            self.sync_merge_preview(cx);
        }
        cx.notify();
    }

    /// 宛先の選択: the records of a CSV, TSV, text or Excel file (its first sheet, the first row the field names).
    pub(crate) fn pick_merge_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: vec!["xlsx".into(),"csv".into(),"tsv".into(),"xls".into(),"xlsm".into()], multiple: false, prompt: Some(crate::shell::i18n::text("merge.flow.load")) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update_in(cx, |this, window, cx| this.read_merge_file(&path, window, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    /// The 宛先一覧 from the file at `path`.
    pub(crate) fn read_merge_file(&mut self, path: &std::path::Path, window: &mut Window, cx: &mut Context<Self>) {
        match workflow::read_recipient_file(path) {
            Ok(import) => {
                if let Some(m)=self.doc_state_mut().and_then(|s|s.merge.as_mut()) {
                    m.dialog_return=m.dialog;
                    m.import=Some(import);m.import_error=None;m.dialog=Some(MergeDialog::Import);
                }
            }
            Err(error) => self.status=Some(error.into()),
        }
        self.reset_merge_import_headers(window,cx);
        cx.notify();
    }

    /// 差し込み印刷の開始: the kind of the main document (`formLetters`, `email`, `fax`, `envelopes`, `mailingLabels`, `catalog`; `None`:
    /// an ordinary document), written to the file's settings (`w:mailMerge/w:mainDocumentType`).
    pub(crate) fn set_mail_merge_type(&mut self, kind: Option<&str>, cx: &mut Context<Self>) {
        if self.flow().is_some_and(|d| d.mail_merge_type.as_deref() == kind) {
            return;
        }
        self.run_flow(vec![FlowCommand::SetMailMergeType(kind.map(str::to_string))], cx);
    }

    /// 封筒の設定 (DOCX-MERGE-016): 封筒サイズと宛名配置を設定する
    pub(crate) fn setup_envelope(&mut self, preset: EnvelopePreset, return_address: Option<&str>, cx: &mut Context<Self>) {
        let (format, body_len, first_id) = {
            let fields = self.merge_state().map(|m| m.fields.clone()).unwrap_or_default();
            let mapping = self.flow().map(|d| d.mail_merge_field_map.clone()).unwrap_or_default();
            let format = ibis_doc_model::merge::default_address_format_with_mapping(&fields, &mapping);
            let len = self.flow().map_or(0, |d| d.body.len());
            let first_id = self.flow().and_then(|d| d.body.first()).map(|b| match b {
                ibis_doc_model::flow::Block::Paragraph(p) => p.id,
                ibis_doc_model::flow::Block::Table(t) => t.id,
            });
            (format, len, first_id)
        };
        let mut next_id = self.flow().map_or(1, |d| d.new_id());
        let mut blocks = envelope_blocks(preset, return_address, &format, &mut next_id);
        if let Some(fid) = first_id && let Some(ibis_doc_model::flow::Block::Paragraph(p)) = blocks.first_mut() {
            p.id = fid;
        }
        let commands = vec![
            FlowCommand::SetMailMergeType(Some("envelopes".to_string())),
            FlowCommand::SetPage { page: preset.page_setup() },
            FlowCommand::ReplaceBlocks { container: Container::Body, index: 0, remove: body_len, blocks },
        ];
        self.run_flow(commands, cx);
    }

    /// ラベル規格と差込ラベル (DOCX-MERGE-017): ラベル規格に応じた表グリッドと NEXT/ADDRESSBLOCK フィールドを生成する
    pub(crate) fn setup_labels(&mut self, preset: LabelPreset, cx: &mut Context<Self>) {
        let (format, body_len, first_id) = {
            let fields = self.merge_state().map(|m| m.fields.clone()).unwrap_or_default();
            let mapping = self.flow().map(|d| d.mail_merge_field_map.clone()).unwrap_or_default();
            let format = ibis_doc_model::merge::default_address_format_with_mapping(&fields, &mapping);
            let len = self.flow().map_or(0, |d| d.body.len());
            let first_id = self.flow().and_then(|d| d.body.first()).map(|b| match b {
                ibis_doc_model::flow::Block::Paragraph(p) => p.id,
                ibis_doc_model::flow::Block::Table(t) => t.id,
            });
            (format, len, first_id)
        };
        let mut next_id = self.flow().map_or(1, |d| d.new_id());
        let table = label_sheet_table(preset, &format, &mut next_id);
        let trailing_p = ibis_doc_model::FlowParagraph {
            id: first_id.unwrap_or_else(|| { let id = next_id; next_id += 1; id }),
            mark: ibis_doc_model::flow::FlowRunStyle {
                size: 200,
                ..ibis_doc_model::flow::FlowRunStyle::default()
            },
            ..ibis_doc_model::FlowParagraph::default()
        };
        let blocks = vec![
            ibis_doc_model::flow::Block::Table(table),
            ibis_doc_model::flow::Block::Paragraph(trailing_p),
        ];
        let commands = vec![
            FlowCommand::SetMailMergeType(Some("mailingLabels".to_string())),
            FlowCommand::SetPage { page: preset.page_setup() },
            FlowCommand::ReplaceBlocks { container: Container::Body, index: 0, remove: body_len, blocks },
        ];
        self.run_flow(commands, cx);
    }

    /// 差し込みフィールドの挿入: the field `name` at the caret.
    pub(crate) fn insert_merge_field(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let kind = FlowRunKind::Field { instruction: merge_field_instruction(name), shown: merge_field_placeholder(name) };
        if self.run_flow(vec![FlowCommand::InsertField { at: caret, kind }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
        }
    }

    /// 住所ブロック / 挨拶文の挿入: a block of the fields the recipient list has, at the caret.
    pub(crate) fn insert_merge_block(&mut self, address: bool, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let Some(fields) = self.merge_state().map(|m| m.fields.clone()) else { return };
        let (instruction, shown) = if address {
            let mapping = self.flow().map(|d| d.mail_merge_field_map.as_slice()).unwrap_or(&[]);
            (ibis_doc_model::merge::address_block_instruction(&ibis_doc_model::merge::default_address_format_with_mapping(&fields, mapping)), crate::shell::i18n::text("docx.merge-panel.address-block-shown"))
        } else {
            let mapping = self.flow().map(|d| d.mail_merge_field_map.as_slice()).unwrap_or(&[]);
            (ibis_doc_model::merge::greeting_line_instruction(&ibis_doc_model::merge::default_greeting_format_with_mapping(&fields, mapping)), crate::shell::i18n::text("docx.merge-panel.greeting-line-shown"))
        };
        let kind = FlowRunKind::Field { instruction, shown: shown.to_string() };
        if self.run_flow(vec![FlowCommand::InsertField { at: caret, kind }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
        }
    }

    /// 結果のプレビュー on or off: the fields show the current record's values.
    pub(crate) fn set_merge_preview(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.preview = on;
        }
        self.sync_merge_preview(cx);
    }

    /// The previous or next record in the preview.
    pub(crate) fn step_merge_record(&mut self, delta: i64, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            let count = m.selected().len();
            if count > 0 {
                m.current = (m.current as i64 + delta).rem_euclid(count as i64) as usize;
            }
        }
        self.sync_merge_preview(cx);
    }

    /// The layout's values from the state: the current record when previewing.
    fn sync_merge_preview(&mut self, cx: &mut Context<Self>) {
        if let Some(m)=self.doc_state_mut().and_then(|s|s.merge.as_mut()){m.current=m.current.min(m.selected().len().saturating_sub(1));}
        let field_map = self.flow().map(|d| d.mail_merge_field_map.clone()).unwrap_or_default();
        let values = self.merge_state().filter(|m| m.preview && !m.selected().is_empty()).map(|m| {
            let selected = m.selected();
            let record = &m.records[selected[m.current.min(selected.len() - 1)]];
            let mut map: std::collections::HashMap<String, String> = m.fields.iter().cloned().zip(record.iter().cloned()).collect();
            for fmd in &field_map {
                if let Some(val) = map.get(&fmd.name).cloned() {
                    map.entry(fmd.mapped_name.clone()).or_insert(val);
                }
            }
            Arc::new(map)
        });
        self.resources.merge_values = values;
        self.relayout_now(cx);
        cx.notify();
    }

    /// 宛先の選択: record `index` in or out.
    pub(crate) fn toggle_merge_record(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut())
            && let Some(flag) = m.included.get_mut(index)
        {
            *flag = !*flag;
            m.current = 0;
        }
        self.sync_merge_preview(cx);
    }

    /// 宛先の並べ替え: by the field the panel names, ascending or descending (an empty or unknown name: as the list is).
    pub(crate) fn sort_merge_records(&mut self, ascending: bool, cx: &mut Context<Self>) {
        let name = self.merge_state().map(|m| m.sort_field.read(cx).value().trim().to_string()).unwrap_or_default();
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.sort = m.fields.iter().position(|f| *f == name).map(|i| (i, ascending));
            m.current = 0;
        }
        self.sync_merge_preview(cx);
    }

    /// 宛先の抽出: conditions `項目名=値`, `!=`, `~` (has), `!~`, `<`, `<=`, `>`, `>=`, joined with AND・OR (`merge::parse_filter`); empty: all.
    pub(crate) fn filter_merge_records(&mut self, cx: &mut Context<Self>) {
        let text = self.merge_state().map(|m| m.filter_field.read(cx).value().trim().to_string()).unwrap_or_default();
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.filter = ibis_doc_model::merge::parse_filter(&text, &m.fields);
            m.current = 0;
        }
        self.sync_merge_preview(cx);
    }

    /// The panel's text made again from the lists (after a record or a field is added or taken away).
    fn write_merge_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((input, text)) = self.merge_state().map(|m| (m.text.clone(), ibis_doc_model::merge::serialize_table(&m.fields, &m.records))) else { return };
        input.update(cx, |a, cx| a.set_value(text, window, cx));
    }

    /// 宛先の追加: an empty record at the end of the list.
    pub(crate) fn add_merge_record(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) else { return };
        if m.fields.is_empty() {
            return;
        }
        m.records.push(vec![String::new(); m.fields.len()]);
        m.included.push(true);
        m.list_page = (m.records.len() - 1) / RECORDS_PER_PAGE;
        self.write_merge_text(window, cx);
        self.sync_merge_preview(cx);
    }

    /// 宛先の削除: record `index` taken out of the list.
    pub(crate) fn delete_merge_record(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) else { return };
        if index >= m.records.len() {
            return;
        }
        m.records.remove(index);
        if index < m.included.len() {
            m.included.remove(index);
        }
        m.current = 0;
        m.list_page = m.list_page.min(m.records.len().saturating_sub(1) / RECORDS_PER_PAGE);
        self.write_merge_text(window, cx);
        self.sync_merge_preview(cx);
    }

    /// 項目の追加: a field `name` (empty in every record) at the end; the sort and the filter are cleared (they name fields by place).
    pub(crate) fn add_merge_field(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let name = name.trim();
        let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) else { return };
        if name.is_empty() || m.fields.iter().any(|f| f.eq_ignore_ascii_case(name)) {
            return;
        }
        m.fields.push(name.to_string());
        for r in &mut m.records {
            r.push(String::new());
        }
        m.sort = None;
        m.filter = None;
        let input = m.new_field.clone();
        input.update(cx, |a, cx| a.set_value("", window, cx));
        self.write_merge_text(window, cx);
        self.sync_merge_preview(cx);
    }

    /// 項目の削除: field `index` taken out of the list and of every record.
    pub(crate) fn delete_merge_field(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) else { return };
        if index >= m.fields.len() {
            return;
        }
        m.fields.remove(index);
        for r in &mut m.records {
            if index < r.len() {
                r.remove(index);
            }
        }
        m.sort = None;
        m.filter = None;
        self.write_merge_text(window, cx);
        self.sync_merge_preview(cx);
    }

    /// 宛先の選択: every record in (`on`) or out.
    pub(crate) fn set_all_merge_records(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.included = vec![on; m.records.len()];
            m.current = 0;
        }
        self.sync_merge_preview(cx);
    }

    /// The page of the list shown, `delta` pages on.
    pub(crate) fn step_merge_page(&mut self, delta: i64, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            let pages = m.records.len().div_ceil(RECORDS_PER_PAGE).max(1) as i64;
            m.list_page = (m.list_page as i64 + delta).clamp(0, pages - 1) as usize;
        }
        cx.notify();
    }

    /// The preview at the first (`0`) or last record, or at record number `n` (1 the first) of those merged.
    pub(crate) fn goto_merge_record(&mut self, n: usize, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            let count = m.selected().len();
            if count > 0 {
                m.current = n.saturating_sub(1).min(count - 1);
            }
        }
        self.sync_merge_preview(cx);
    }

    /// 宛先の検索: the preview goes to the next record (after the one shown, round again) one of whose values has the panel's text.
    pub(crate) fn find_merge_record(&mut self, cx: &mut Context<Self>) -> bool {
        let text = self.merge_state().map(|m| m.find_field.read(cx).value().trim().to_string()).unwrap_or_default();
        let found = self.merge_state().and_then(|m| {
            let selected = m.selected();
            if text.is_empty() || selected.is_empty() {
                return None;
            }
            (1..=selected.len()).map(|k| (m.current + k) % selected.len()).find(|&at| m.records[selected[at]].iter().any(|v| v.contains(text.as_str())))
        });
        let Some(at) = found else {
            self.status = Some(crate::shell::i18n::text("docx.merge-panel.recipient-not-found").into());
            cx.notify();
            return false;
        };
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.current = at;
        }
        self.sync_merge_preview(cx);
        true
    }

    /// フィールドと列の対応の表示切り替え (DOCX-MERGE-009).
    pub(crate) fn toggle_match_fields(&mut self, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.match_fields_open = !m.match_fields_open;
            cx.notify();
        }
    }

    /// フィールドと列の対応 (DOCX-MERGE-009): 宛先一覧の列名から標準差し込みフィールドを自動対応付けする
    pub(crate) fn auto_match_merge_fields(&mut self, cx: &mut Context<Self>) {
        let fields = self.merge_state().map(|m| m.fields.clone()).unwrap_or_default();
        let map = ibis_doc_model::merge::auto_match_fields(&fields);
        if self.run_flow(vec![FlowCommand::SetMailMergeFieldMap(map)], cx) {
            self.sync_merge_preview(cx);
        }
    }

    /// フィールドと列の対応 (DOCX-MERGE-009): フィールドと列の対応をクリアする
    pub(crate) fn clear_merge_field_map(&mut self, cx: &mut Context<Self>) {
        if self.run_flow(vec![FlowCommand::SetMailMergeFieldMap(Vec::new())], cx) {
            self.sync_merge_preview(cx);
        }
    }

    /// フィールドと列の対応 (DOCX-MERGE-009): 指定の標準フィールドに対応する列を次へ切り替える（未対応 -> 列0 -> 列1 -> ... -> 未対応）
    pub(crate) fn cycle_merge_field_mapping(&mut self, standard_name: &str, cx: &mut Context<Self>) {
        let fields = self.merge_state().map(|m| m.fields.clone()).unwrap_or_default();
        if fields.is_empty() {
            return;
        }
        let current_map = self.flow().map(|d| d.mail_merge_field_map.clone()).unwrap_or_default();
        let current_mapped = current_map.iter().find(|m| m.mapped_name == standard_name).map(|m| m.name.as_str());
        let next_mapped: Option<&str> = match current_mapped {
            None => fields.first().map(|s| s.as_str()),
            Some(curr) => {
                if let Some(pos) = fields.iter().position(|f| f == curr) {
                    if pos + 1 < fields.len() {
                        Some(fields[pos + 1].as_str())
                    } else {
                        None
                    }
                } else {
                    fields.first().map(|s| s.as_str())
                }
            }
        };
        let mut new_map: Vec<ibis_doc_model::merge::FieldMapData> = current_map.into_iter().filter(|m| m.mapped_name != standard_name).collect();
        if let Some(col_name) = next_mapped {
            let col_idx = fields.iter().position(|f| f == col_name).unwrap_or(0);
            new_map.push(ibis_doc_model::merge::FieldMapData {
                name: col_name.to_string(),
                mapped_name: standard_name.to_string(),
                column: col_idx,
            });
        }
        new_map.sort_by(|a, b| a.mapped_name.cmp(&b.mapped_name));
        if self.run_flow(vec![FlowCommand::SetMailMergeFieldMap(new_map)], cx) {
            self.sync_merge_preview(cx);
        }
    }

    /// フィールドと列の対応 (DOCX-MERGE-009): 指定の標準フィールドに対応する列を明示設定する
    #[allow(dead_code)]
    pub(crate) fn set_merge_field_mapping(&mut self, standard_name: &str, mapped_column: Option<&str>, cx: &mut Context<Self>) {
        let fields = self.merge_state().map(|m| m.fields.clone()).unwrap_or_default();
        let current_map = self.flow().map(|d| d.mail_merge_field_map.clone()).unwrap_or_default();
        let mut new_map: Vec<ibis_doc_model::merge::FieldMapData> = current_map.into_iter().filter(|m| m.mapped_name != standard_name).collect();
        if let Some(col_name) = mapped_column {
            let col_idx = fields.iter().position(|f| f == col_name).unwrap_or(0);
            new_map.push(ibis_doc_model::merge::FieldMapData {
                name: col_name.to_string(),
                mapped_name: standard_name.to_string(),
                column: col_idx,
            });
        }
        new_map.sort_by(|a, b| a.mapped_name.cmp(&b.mapped_name));
        if self.run_flow(vec![FlowCommand::SetMailMergeFieldMap(new_map)], cx) {
            self.sync_merge_preview(cx);
        }
    }

    /// 個々のドキュメントの編集: the file `path` with a copy of the main document for each record (what the fields
    /// say not used: their values), each on its own page.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_merged_documents(&mut self, path: &std::path::Path) -> Result<usize, String> {
        self.write_merged_documents_range(path,MergePrintRange::All)
    }
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_merged_documents_range(&self,path:&std::path::Path,range:MergePrintRange)->Result<usize,String> {
        let (merged,count)=self.merged_document_for_range(range)?;
        let source=self.source.clone().ok_or_else(||crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string())?;
        let mut presentation=self.presentation().clone();presentation.flow=Some(Arc::new(merged));
        let bytes=source.save(&presentation,ibis_pptx::SaveOptions::default()).map_err(|e|e.to_string())?;
        crate::shell::save::write_replacing(path,&bytes).map_err(|e|e.to_string())?;
        Ok(count)
    }

    /// 規則の挿入: the field of the instruction in the panel's rule field (`IF`, `COMPARE`, `SKIPIF`… with `{ MERGEFIELD 項目 }` in
    /// it) at the caret; it shows «規則» until previewed.
    pub(crate) fn insert_merge_rule(&mut self, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let instruction = self.merge_state().map(|m| m.rule_field.read(cx).value().trim().to_string()).unwrap_or_default();
        if instruction.is_empty() {
            return;
        }
        let word = instruction.split_whitespace().next().unwrap_or("").to_ascii_uppercase();
        let kind = FlowRunKind::Field { instruction: format!("{instruction} "), shown: format!("«{word}»") };
        if self.run_flow(vec![FlowCommand::InsertField { at: caret, kind }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
        }
    }

    /// 差し込み印刷のエラーチェック (Word: 差し込み文書 > エラーチェック / docx.merge-panel.check-errors).
    /// 文書内の MERGEFIELD・ADDRESSBLOCK・GREETINGLINE フィールドを宛先リストと照合し、
    /// 未定義フィールド、未読込宛先リスト、空白レコード等を検出して結果を通知する。
    pub(crate) fn check_merge_errors(&mut self, cx: &mut Context<Self>) -> MergeCheckResult {
        let Some(doc) = self.flow().cloned() else {
            let msg = crate::shell::i18n::text("docx.merge-panel.there-no-document");
            self.status = Some(msg.into());
            cx.notify();
            return MergeCheckResult {
                recipient_count: 0,
                fields_used: Vec::new(),
                missing_fields: Vec::new(),
                empty_record_indices: Vec::new(),
            };
        };

        let fields_used = ibis_doc_model::merge::merge_fields_used(&doc);
        let Some(m) = self.merge_state() else {
            let msg = crate::shell::i18n::text("docx.merge-panel.error-check.no-recipients");
            self.status = Some(msg.into());
            cx.notify();
            return MergeCheckResult {
                recipient_count: 0,
                fields_used,
                missing_fields: Vec::new(),
                empty_record_indices: Vec::new(),
            };
        };

        if m.records.is_empty() {
            let msg = crate::shell::i18n::text("docx.merge-panel.error-check.no-recipients");
            self.status = Some(msg.into());
            cx.notify();
            return MergeCheckResult {
                recipient_count: 0,
                fields_used,
                missing_fields: Vec::new(),
                empty_record_indices: Vec::new(),
            };
        }

        let recipient_count = m.selected().len();
        let field_map = &doc.mail_merge_field_map;

        // Check for missing fields
        let mut missing_fields = Vec::new();
        for f in &fields_used {
            let found_in_fields = m.fields.iter().any(|col| col.eq_ignore_ascii_case(f));
            let found_in_map = field_map.iter().any(|mapping| {
                mapping.mapped_name.eq_ignore_ascii_case(f) && m.fields.iter().any(|col| col.eq_ignore_ascii_case(&mapping.name))
            });
            if !found_in_fields && !found_in_map {
                missing_fields.push(f.clone());
            }
        }

        // Inspect only output targets and every field actually used by this document.
        let mut empty_record_indices:Vec<usize>=self.merge_review_issues().into_iter().filter_map(|i|i.record).collect();
        empty_record_indices.sort_unstable();empty_record_indices.dedup();

        // Status message
        if !missing_fields.is_empty() {
            let list = missing_fields.join(", ");
            let msg = crate::shell::i18n::format("docx.merge-panel.error-check.missing-fields", &[("fields", &list)]);
            self.status = Some(msg.into());
        } else if !empty_record_indices.is_empty() {
            let count_str = empty_record_indices.len().to_string();
            let msg = crate::shell::i18n::format("docx.merge-panel.error-check.empty-records", &[("count", &count_str)]);
            self.status = Some(msg.into());
        } else if fields_used.is_empty() {
            let msg = crate::shell::i18n::text("docx.merge-panel.error-check.no-fields");
            self.status = Some(msg.into());
        } else {
            let count_str = recipient_count.to_string();
            let fcount_str = fields_used.len().to_string();
            let msg = crate::shell::i18n::format(
                "docx.merge-panel.error-check.passed",
                &[("count", &count_str), ("field_count", &fcount_str)],
            );
            self.status = Some(msg.into());
        }

        cx.notify();
        MergeCheckResult {
            recipient_count,
            fields_used,
            missing_fields,
            empty_record_indices,
        }
    }

    /// 宛先ごとの別のファイル: one file in `directory` for each record chosen (`{stem} 1.docx`…), its own copy of the main
    /// document; a record a `SKIPIF` leaves out has none. How many files.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_merged_separately(&self, directory: &std::path::Path, stem: &str) -> Result<usize, String> {
        self.write_merged_separately_range(directory,stem,MergePrintRange::All)
    }
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_merged_separately_range(&self,directory:&std::path::Path,stem:&str,range:MergePrintRange)->Result<usize,String> {
        let Some(m) = self.merge_state() else { return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string()) };
        let selected = m.selected();
        if selected.is_empty() {
            return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string());
        }
        let (fields, records) = (m.fields.clone(), selected.iter().map(|&i| m.records[i].clone()).collect::<Vec<_>>());
        // Validate and resolve against the selected recipients, before creating any files.
        self.merged_document_for_range(range)?;
        let records=match range {MergePrintRange::All=>records,MergePrintRange::Current=>vec![records[m.current].clone()],MergePrintRange::FromTo(start,end)=>records[start-1..end.min(records.len())].to_vec()};
        let source = self.source.clone().ok_or_else(|| crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string())?;
        let doc = self.flow().ok_or_else(|| crate::shell::i18n::text("docx.merge-panel.there-no-document").to_string())?.clone();
        let mut count = 0;
        for record in &records {
            if ibis_doc_model::merge::record_skipped(&doc, &fields, record) {
                continue;
            }
            count += 1;
            let merged = merged_letters(&doc, &fields, std::slice::from_ref(record));
            let mut presentation = self.presentation().clone();
            presentation.flow = Some(Arc::new(merged));
            let bytes = source.save(&presentation, ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?;
            crate::shell::save::write_replacing(&directory.join(format!("{stem} {count}.docx")), &bytes).map_err(|e| e.to_string())?;
        }
        Ok(count)
    }

    /// 宛先ごとに別のファイルで作成…: choose the folder, and write the files.
    pub(crate) fn save_merged_separately(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, extensions: Vec::new(), multiple: false, prompt: None });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(directory) = paths.into_iter().next() else { return };
                this.update(cx, |this, cx| {
                    let stem = this.file_name.as_ref().map(|n| n.trim_end_matches(".docx").to_string()).unwrap_or_else(|| crate::shell::i18n::text("_docx.merged-result-stem").to_string());
                    this.status = Some(match this.write_merged_separately_range(&directory, &stem,this.current_merge_print_range(cx)) {
                        Ok(n) => crate::shell::i18n::format("merge.done", &[("n", &n.to_string())]).into(),
                        Err(e) => e.into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (window, cx);
        }
    }

    /// 個々のドキュメントの編集…: choose where the merged file goes, and write it.
    pub(crate) fn save_merged_documents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).unwrap_or_else(|| std::env::var_os("HOME").map(Into::into).unwrap_or_else(|| ".".into()));
            let name = self.file_name.as_ref().map(|n| crate::shell::i18n::format("_docx.merged-result-n", &[("n", &n)])).unwrap_or_else(|| crate::shell::i18n::text("_docx.merged-result-docx").to_string());
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update(cx, |this, cx| {
                    this.status = Some(match this.write_merged_documents_range(&path,this.current_merge_print_range(cx)) {
                        Ok(n) => crate::shell::i18n::format("merge.done", &[("n", &n)]).into(),
                        Err(e) => crate::shell::i18n::format("file.save-failed", &[("error", &e)]).into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    /// 差し込み印刷の印刷範囲を設定する (DOCX-MERGE-014).
    pub(crate) fn set_merge_print_range(&mut self, range: ibis_doc_model::merge::MergePrintRange, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.print_range = range;
            cx.notify();
        }
    }

    /// 現在選択されている印刷範囲を解決する (DOCX-MERGE-014).
    pub(crate) fn current_merge_print_range(&self, cx: &App) -> ibis_doc_model::merge::MergePrintRange {
        let Some(m) = self.merge_state() else { return ibis_doc_model::merge::MergePrintRange::All; };
        match m.print_range {
            ibis_doc_model::merge::MergePrintRange::All => ibis_doc_model::merge::MergePrintRange::All,
            ibis_doc_model::merge::MergePrintRange::Current => ibis_doc_model::merge::MergePrintRange::Current,
            ibis_doc_model::merge::MergePrintRange::FromTo(_, _) => {
                let from_str = m.print_from_field.read(cx).value().trim().to_string();
                let to_str = m.print_to_field.read(cx).value().trim().to_string();
                let start = from_str.parse::<usize>().unwrap_or(1);
                let end = to_str.parse::<usize>().unwrap_or(usize::MAX);
                ibis_doc_model::merge::MergePrintRange::FromTo(start, end)
            }
        }
    }

    /// 差し込み印刷用の FlowDocument を生成する (DOCX-MERGE-014).
    pub(crate) fn merged_document_for_range(&self, range: ibis_doc_model::merge::MergePrintRange) -> Result<(ibis_doc_model::FlowDocument, usize), String> {
        let Some(m) = self.merge_state() else {
            return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string());
        };
        let selected = m.selected();
        if selected.is_empty() {
            return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string());
        }
        let (fields, records) = (m.fields.clone(), selected.iter().map(|&i| m.records[i].clone()).collect::<Vec<_>>());
        let doc = self.flow().ok_or_else(|| crate::shell::i18n::text("docx.merge-panel.there-no-document").to_string())?;
        ibis_doc_model::merge::merged_letters_for_range(doc, &fields, &records, range, m.current)
    }

    /// 差し込み印刷の PDF を書き出す (DOCX-MERGE-014).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_merged_pdf(&self, pdf_path: &std::path::Path, range: ibis_doc_model::merge::MergePrintRange) -> Result<usize, String> {
        let Some(m) = self.merge_state() else {
            return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string());
        };
        let selected = m.selected();
        if selected.is_empty() {
            return Err(crate::shell::i18n::text("docx.merge-panel.there-no-recipient-list").to_string());
        }
        let (fields, records) = (m.fields.clone(), selected.iter().map(|&i| m.records[i].clone()).collect::<Vec<_>>());
        let source = self.source.clone().ok_or_else(|| crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string())?;
        let doc = self.flow().ok_or_else(|| crate::shell::i18n::text("docx.merge-panel.there-no-document").to_string())?;
        let (merged, count) = ibis_doc_model::merge::merged_letters_for_range(doc, &fields, &records, range, m.current)?;

        let mut presentation = self.presentation().clone();
        presentation.flow = Some(Arc::new(merged));
        let docx_bytes = source.save(&presentation, ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?;

        let dir = std::env::temp_dir().join(format!("ibisoffice-merge-pdf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let out = dir.join("out");
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let temp_docx = dir.join("merged.docx");
        std::fs::write(&temp_docx, &docx_bytes).map_err(|e| e.to_string())?;

        let program = crate::shell::file_actions::export_program()?;
        let mut cmd = std::process::Command::new(program);
        cmd.args(["--export", "pdf", "--out-dir"]).arg(&out).arg(&temp_docx);
        let status = cmd.status().map_err(|e| e.to_string())?;
        if !status.success() {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!("PDF export failed with status: {status}"));
        }
        let made = std::fs::read_dir(&out)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
            .ok_or_else(|| "Could not find exported PDF".to_string())?;
        let data = std::fs::read(&made).map_err(|e| e.to_string())?;
        crate::shell::save::write_replacing(pdf_path, &data).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&dir);
        Ok(count)
    }

    /// 差し込み印刷を実行する（macOS は Preview の印刷ダイアログ、DOCX-MERGE-014）.
    pub(crate) fn print_merged_documents(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let range = self.current_merge_print_range(cx);
            let temp_pdf = std::env::temp_dir().join(format!("ibisoffice-print-merge-{}.pdf", std::process::id()));
            match self.write_merged_pdf(&temp_pdf, range) {
                Ok(count) => {
                    #[cfg(target_os = "macos")]
                    if !crate::shell::file_actions::running_under_test() {
                        let script = format!(
                            "tell application \"Preview\"\nactivate\nopen POSIX file \"{}\"\nprint front document with print dialog\nend tell",
                            temp_pdf.display().to_string().replace('"', "\\\"")
                        );
                        let _ = std::process::Command::new("osascript").args(["-e", &script]).output();
                    }
                    #[cfg(not(target_os = "macos"))]
                    let _ = crate::shell::file_actions::open_pdf_in_default_app(&temp_pdf, cx);
                    self.status = Some(crate::shell::i18n::format("docx.merge.printed-count", &[("n", &count.to_string())]).into());
                    cx.notify();
                }
                Err(e) => {
                    self.status = Some(crate::shell::i18n::format("docx.merge.print-failed", &[("error", &e)]).into());
                    cx.notify();
                }
            }
        }
        #[cfg(target_family = "wasm")]
        let _ = cx;
    }

    /// 差し込み印刷を PDF に書き出す (DOCX-MERGE-014).
    pub(crate) fn export_merged_pdf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let range = self.current_merge_print_range(cx);
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).unwrap_or_else(|| std::env::var_os("HOME").map(Into::into).unwrap_or_else(|| ".".into()));
            let name = self.file_name.as_ref().map(|n| format!("{}-merged.pdf", n.trim_end_matches(".docx"))).unwrap_or_else(|| "merged.pdf".to_string());
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update(cx, |this, cx| {
                    this.status = Some(crate::shell::i18n::text("docx.merge.printing").into());
                    cx.notify();
                    let result = this.write_merged_pdf(&path, range);
                    this.status = Some(match result {
                        Ok(n) => crate::shell::i18n::format("docx.merge.exported-pdf-count", &[("n", &n.to_string())]).into(),
                        Err(e) => crate::shell::i18n::format("docx.merge.pdf-failed", &[("error", &e)]).into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    pub(crate) fn merge_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        if !self.merge_state()?.visible { return None; }
        self.render_merge_workflow(cx)
    }

    pub(crate) fn move_merge_recipient(&mut self,from:usize,to:usize,w:&mut Window,cx:&mut Context<Self>) {
        let Some(m)=self.doc_state_mut().and_then(|s|s.merge.as_mut()) else{return};
        if from>=m.records.len()||to>=m.records.len()||from==to{return;}
        // current indexes selected(), not the unfiltered records. Preserve the recipient's identity.
        let selected=m.selected();
        let current_record=selected.get(m.current.min(selected.len().saturating_sub(1))).copied();
        let record=m.records.remove(from);m.records.insert(to,record);
        m.included.resize(m.records.len(),true);let included=m.included.remove(from);m.included.insert(to,included);
        let moved_index=|i|if i==from{to}else if from<i&&i<=to{i-1}else if to<=i&&i<from{i+1}else{i};
        m.sort=None;m.table_sort=None;
        let selected=m.selected();
        m.current=current_record.and_then(|i|selected.iter().position(|&j|j==moved_index(i))).unwrap_or(0);
        self.write_merge_text(w,cx);self.sync_merge_preview(cx);cx.notify();
    }

    #[inline(never)]
    pub(crate) fn open_merge_row(&mut self,index:Option<usize>,w:&mut Window,cx:&mut Context<Self>) {
        let Some(m)=self.merge_state() else{return};let values=index.and_then(|i|m.records.get(i).cloned()).unwrap_or_else(||vec![String::new();m.fields.len()]);
        let inputs=values.into_iter().map(|v|cx.new(|cx|{let mut i=InputState::new(w,cx);i.set_value(v,w,cx);i})).collect();
        if let Some(m)=self.doc_state_mut().and_then(|s|s.merge.as_mut()){m.row_editor=Some(index);m.row_inputs=inputs;}cx.notify();
    }
    pub(crate) fn save_merge_row(&mut self,w:&mut Window,cx:&mut Context<Self>) {
        let Some(m)=self.merge_state() else{return};let Some(index)=m.row_editor else{return};let values=m.row_inputs.iter().map(|i|i.read(cx).value().to_string()).collect();
        if let Some(m)=self.doc_state_mut().and_then(|s|s.merge.as_mut()) {
            if let Some(i)=index {if i<m.records.len(){m.records[i]=values;}} else {m.records.push(values);m.included.push(true);}
            m.review_ack=false;
            m.row_editor=None;
        }
        self.write_merge_text(w,cx);self.sync_merge_preview(cx);cx.notify();
    }
    pub(crate) fn merge_row_dialog(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel> {
        let m=self.merge_state()?;let mode=m.row_editor?;
        let body=v_flex().p_3().gap_2().p_0().children(m.fields.iter().zip(m.row_inputs.iter()).map(|(label,input)|h_flex().gap_2().items_center().child(div().w(px(100.)).text_sm().child(label.clone())).child(crate::shell::chrome::text_field(input).full_width())));
        let footer=crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("merge-row-ok", "OK", crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e,_,w,cx|e.save_merge_row(w,cx))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx))
            .when_some(mode,|d,i|d.action(crate::shell::chrome::ActionDefinition::new("merge-row-delete", crate::shell::i18n::text("_shell.delete"), crate::shell::chrome::ActionButtonKind::Destructive).on_click(cx.listener(move |e,_,w,cx|{e.delete_merge_record(i,w,cx);if let Some(m)=e.doc_state_mut().and_then(|s|s.merge.as_mut()){m.row_editor=None;}cx.notify();}))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("ui.merge-row"),body.into_any_element(),footer))
    }

}
