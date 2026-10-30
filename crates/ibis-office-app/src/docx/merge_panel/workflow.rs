// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Mail merge keeps the document editable; each modal has one task.
use super::*;
mod review;
use crate::shell::chrome::{
    self, action_bar, action_button, primary_button, radio_button, text_field,
};
use crate::shell::i18n::{format as tf, text as tr};
use gpui_kit::component::Disableable as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MergeStep {
    Recipients,
    Compose,
    Preview,
    Output,
}
impl MergeStep {
    const ALL: [Self; 4] = [Self::Recipients, Self::Compose, Self::Preview, Self::Output];
    fn key(self) -> &'static str {
        match self {
            Self::Recipients => "merge.flow.recipients",
            Self::Compose => "merge.flow.compose",
            Self::Preview => "merge.flow.preview",
            Self::Output => "merge.flow.output",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MergeDialog {
    Recipients,
    Import,
    Columns,
    Filter,
    Condition,
    Mapping,
    Kind,
    Output,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MergeOutput {
    Document,
    Pdf,
    Print,
}
#[derive(Clone)]
pub(crate) struct RecipientSnapshot {
    fields: Vec<String>,
    records: Vec<Vec<String>>,
    included: Vec<bool>,
    current: usize,
    sort: Option<(usize, bool)>,
    filter: Option<ibis_doc_model::merge::RecordFilter>,
    source: String,
    table_sort: Option<(usize, bool)>,
}
impl RecipientSnapshot {
    fn of(m: &MergeState) -> Self {
        Self {
            fields: m.fields.clone(),
            records: m.records.clone(),
            included: m.included.clone(),
            current: m.current,
            sort: m.sort,
            filter: m.filter.clone(),
            source: m.source_name.clone(),
            table_sort: m.table_sort,
        }
    }
    fn restore(self, m: &mut MergeState) {
        m.fields = self.fields;
        m.records = self.records;
        m.included = self.included;
        m.current = self.current;
        m.sort = self.sort;
        m.filter = self.filter;
        m.source_name = self.source;
        m.table_sort = self.table_sort;
    }
}
pub(crate) struct MergeImport {
    pub(crate) name: String,
    pub(crate) sheets: Vec<(String, String)>,
    pub(crate) sheet: usize,
    pub(crate) header: bool,
    pub(crate) start_row: usize,
    pub(crate) end_row: usize,
    pub(crate) start_column: usize,
    pub(crate) end_column: usize,
    pub(crate) names: Option<Vec<String>>,
    pub(crate) advanced: bool,
}
impl MergeImport {
    pub(crate) fn raw_rows(&self) -> Vec<Vec<String>> {
        self.sheets
            .get(self.sheet)
            .map(|(_, text)| ibis_pptx::csv::parse(text.trim_start_matches('\u{feff}')))
            .unwrap_or_default()
    }
    pub(crate) fn table(&self) -> (Vec<String>, Vec<Vec<String>>) {
        let raw = self.raw_rows();
        let mut rows: Vec<Vec<String>> = raw
            .into_iter()
            .skip(self.start_row.saturating_sub(1))
            .take(self.end_row.saturating_sub(self.start_row) + 1)
            .map(|row| {
                (self.start_column.saturating_sub(1)..self.end_column)
                    .map(|j| row.get(j).cloned().unwrap_or_default())
                    .collect()
            })
            .collect();
        let fields = if self.header && !rows.is_empty() {
            rows.remove(0)
                .into_iter()
                .map(|s| s.trim().to_string())
                .collect()
        } else {
            (1..=self.end_column.saturating_sub(self.start_column) + 1)
                .map(|i| tf("merge.flow.column-number", &[("n", &i.to_string())]))
                .collect()
        };
        let fields = self.names.clone().unwrap_or(fields);
        rows.retain(|r| r.iter().any(|v| !v.trim().is_empty()));
        (fields, rows)
    }
}
/// Use ibisOffice's existing workbook reader and CSV decoder, without executing workbook macros.
pub(crate) fn read_recipient_file(path: &std::path::Path) -> Result<MergeImport, String> {
    let error = || tr("docx.merge-panel.cannot-read-file").to_string();
    let bytes = std::fs::read(path).map_err(|_| error())?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let sheets: Vec<(String, String)> = match ext.as_str() {
        "xlsx" | "xlsm" | "xls" => {
            let book = ibis_pptx::read(&bytes).map_err(|_| error())?;
            if book.workbook.is_none() {
                return Err(error());
            }
            book.slides
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    (
                        s.name.clone().unwrap_or_else(|| (i + 1).to_string()),
                        String::from_utf8_lossy(&ibis_pptx::csv::sheet_to_tsv(&book, i))
                            .into_owned(),
                    )
                })
                .collect()
        }
        "csv" | "tsv" => vec![(
            tr("merge.flow.data").to_string(),
            ibis_pptx::csv::decode(&bytes).map_err(|_| error())?,
        )],
        _ => return Err(tr("merge.flow.unsupported").to_string()),
    };
    let rows: Vec<Vec<String>> = sheets
        .first()
        .map(|(_, text)| ibis_pptx::csv::parse(text.trim_start_matches('\u{feff}')))
        .unwrap_or_default();
    let end_row = rows.len().max(1);
    let end_column = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    Ok(MergeImport {
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        sheets,
        sheet: 0,
        header: true,
        start_row: 1,
        end_row,
        start_column: 1,
        end_column,
        names: None,
        advanced: false,
    })
}
fn note(key: &str) -> Div {
    div()
        .text_sm()
        .text_color(rgb(chrome::TEXT_SECONDARY))
        .child(tr(key))
}
fn heading(key: &str) -> Div {
    div()
        .text_size(px(17.))
        .font_weight(FontWeight::MEDIUM)
        .child(tr(key))
}
fn record_table(fields: &[String], rows: &[Vec<String>]) -> AnyElement {
    v_flex()
        .min_w_0()
        .w_full()
        .border_1()
        .border_color(rgb(chrome::BORDER))
        .rounded_md()
        .child(
            h_flex().p_2().gap_2().bg(rgb(0xf3f4f6)).children(
                fields
                    .iter()
                    .map(|f| div().flex_1().min_w_0().text_sm().child(f.clone())),
            ),
        )
        .children(rows.iter().take(5).map(|r| {
            h_flex()
                .p_2()
                .gap_2()
                .border_t_1()
                .border_color(rgb(chrome::BORDER))
                .children(fields.iter().enumerate().map(|(j, _)| {
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .child(r.get(j).cloned().unwrap_or_default())
                }))
        }))
        .into_any_element()
}
impl SlideEditor {
    pub(crate) fn sort_merge_table(&mut self, field: usize, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            let ascending = !m.table_sort.is_some_and(|(j, a)| j == field && a);
            let selected = m.selected();
            let current = selected.get(m.current).copied();
            let mut indices: Vec<usize> = (0..m.records.len()).collect();
            indices.sort_by(|&a, &b| {
                let a = m.records[a].get(field).map(String::as_str).unwrap_or("");
                let b = m.records[b].get(field).map(String::as_str).unwrap_or("");
                let o = match (a.parse::<f64>(), b.parse::<f64>()) {
                    (Ok(a), Ok(b)) => a.total_cmp(&b),
                    _ => a.cmp(b),
                };
                if ascending { o } else { o.reverse() }
            });
            m.included = indices
                .iter()
                .map(|&i| m.included.get(i).copied().unwrap_or(true))
                .collect();
            m.records = indices.iter().map(|&i| m.records[i].clone()).collect();
            m.sort = None;
            m.table_sort = Some((field, ascending));
            m.current = current
                .and_then(|i| indices.iter().position(|&j| j == i))
                .and_then(|i| m.selected().iter().position(|&j| j == i))
                .unwrap_or(0);
        }
        self.sync_merge_preview(cx);
        cx.notify();
    }
    fn cycle_merge_mapping_draft(&mut self, field: &'static str, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            let current = m
                .mapping_draft
                .iter()
                .find(|f| f.mapped_name == field)
                .and_then(|f| m.fields.iter().position(|s| s == &f.name));
            let next = current.map_or(Some(0), |i| (i + 1 < m.fields.len()).then_some(i + 1));
            m.mapping_draft.retain(|f| f.mapped_name != field);
            if let Some(i) = next.filter(|&i| i < m.fields.len()) {
                m.mapping_draft.push(ibis_doc_model::merge::FieldMapData {
                    name: m.fields[i].clone(),
                    mapped_name: field.into(),
                    column: i,
                });
            }
        }
        cx.notify();
    }
    pub(crate) fn set_merge_step(&mut self, step: MergeStep, cx: &mut Context<Self>) {
        let Some(m) = self.merge_state() else { return };
        if step != MergeStep::Recipients && m.selected().is_empty() {
            return;
        }
        if matches!(step, MergeStep::Preview | MergeStep::Output)
            && self.flow().is_none_or(|d| merge_fields_used(d).is_empty())
        {
            return;
        }
        if step == MergeStep::Output && !self.review_can_output() {
            if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                m.step = MergeStep::Preview;
            }
            self.set_merge_preview(true, cx);
            return;
        }
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.step = step;
        }
        self.set_merge_preview(matches!(step, MergeStep::Preview | MergeStep::Output), cx);
    }
    pub(crate) fn open_merge_task(&mut self, task: MergeDialog, cx: &mut Context<Self>) {
        let mapping = self
            .flow()
            .map(|d| d.mail_merge_field_map.clone())
            .unwrap_or_default();
        let kind = match self.flow().and_then(|d| d.mail_merge_type.as_deref()) {
            Some("envelopes") => "envelopes",
            Some("mailingLabels") => "mailingLabels",
            Some("catalog") => "catalog",
            Some("email") => "email",
            _ => "formLetters",
        };
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            if task == MergeDialog::Recipients && m.recipient_backup.is_none() {
                m.recipient_backup = Some(RecipientSnapshot::of(m));
                m.list_search.clear();
                m.list_page = 0;
            }
            if task == MergeDialog::Kind {
                m.replace_body_ack = false;
                m.kind_draft = Some(kind);
            }
            if task == MergeDialog::Columns {
                m.column_backup = Some(RecipientSnapshot::of(m));
            }
            if task == MergeDialog::Output {
                m.output_backup = Some((
                    m.print_range,
                    m.print_from_field.read(cx).value().to_string(),
                    m.print_to_field.read(cx).value().to_string(),
                    m.separate,
                ));
            }
            if task == MergeDialog::Mapping {
                m.mapping_draft = mapping;
            }
            m.dialog_return = m.dialog;
            m.dialog = Some(task);
        }
        cx.notify();
    }
    pub(crate) fn cancel_merge_workflow_dialog(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            if m.row_editor.take().is_some() {
                m.row_inputs.clear();
            } else if m.dialog == Some(MergeDialog::Recipients) {
                if let Some(b) = m.recipient_backup.take() {
                    m.fields = b.fields;
                    m.records = b.records;
                    m.included = b.included;
                    m.current = b.current;
                    m.sort = b.sort;
                    m.filter = b.filter;
                    m.source_name = b.source;
                }
                m.dialog = None;
                m.dialog_return = None;
            } else {
                if m.dialog == Some(MergeDialog::Columns) {
                    if let Some(b) = m.column_backup.take() {
                        b.restore(m);
                    }
                }
                if m.dialog == Some(MergeDialog::Output) {
                    if let Some((r, from, to, separate)) = m.output_backup.take() {
                        m.print_range = r;
                        m.separate = separate;
                        let a = m.print_from_field.clone();
                        let b = m.print_to_field.clone();
                        a.update(cx, |i, cx| i.set_value(from, w, cx));
                        b.update(cx, |i, cx| i.set_value(to, w, cx));
                    }
                }
                m.dialog = m.dialog_return.take();
                m.import = None;
            }
        }
        self.write_merge_text(w, cx);
        self.sync_merge_preview(cx);
        cx.notify();
    }
    pub(crate) fn finish_merge_task(&mut self, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            if m.dialog == Some(MergeDialog::Recipients) {
                m.recipient_backup = None;
                m.dialog = None;
            } else {
                m.dialog = m.dialog_return.take();
                m.column_backup = None;
                m.output_backup = None;
            }
        }
        cx.notify();
    }
    pub(crate) fn confirm_merge_import(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let names = self.merge_state().map(|m| {
            m.import_names
                .iter()
                .map(|i| i.read(cx).value().trim().to_string())
                .collect::<Vec<_>>()
        });
        let Some(mut import) = self
            .doc_state_mut()
            .and_then(|s| s.merge.as_mut())
            .and_then(|m| m.import.take())
        else {
            return;
        };
        if let Some(names) = names.filter(|v| !v.is_empty()) {
            import.names = Some(names);
        }
        let (fields, records) = import.table();
        if fields.is_empty()
            || records.is_empty()
            || fields.iter().any(|f| f.is_empty())
            || fields
                .iter()
                .map(|f| f.to_lowercase())
                .collect::<std::collections::HashSet<_>>()
                .len()
                != fields.len()
        {
            self.status = Some(tr("merge.flow.invalid-data").into());
            if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                m.import_error = Some(tr("merge.flow.invalid-data").to_string());
            }
            if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                m.import = Some(import);
            }
            cx.notify();
            return;
        }
        self.set_merge_data(fields, records, cx);
        self.write_merge_text(w, cx);
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.source_name = import.name;
            m.dialog = m.dialog_return.take();
        }
        // A header editor is removed on confirmation; do not leave focus on its hidden input.
        self.focus_handle.focus(w, cx);
        self.sidebar_scroll.set_offset(Point::default());
        cx.notify();
    }
    pub(crate) fn render_merge_workflow(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let m = self.merge_state()?;
        let step = m.step;
        let selected = m.selected();
        let used = self
            .flow()
            .map(|d| merge_fields_used(d))
            .unwrap_or_default();
        let count = selected.len();
        let nav = h_flex()
            .w_full()
            .gap_1()
            .children(MergeStep::ALL.into_iter().enumerate().map(|(i, target)| {
                let disabled = target != MergeStep::Recipients
                    && (count == 0
                        || (matches!(target, MergeStep::Preview | MergeStep::Output)
                            && used.is_empty()));
                action_button(("merge-step", i))
                    .flex_1()
                    .min_w_0()
                    .px(px(3.))
                    .label(format!("{} {}", i + 1, tr(target.key())))
                    .selected(target == step)
                    .disabled(disabled)
                    .on_click(cx.listener(move |e, _, _, cx| e.set_merge_step(target, cx)))
            }));
        let body = v_flex().p_3().gap_3().p_0().child(nav);
        let content: AnyElement = match step {
            MergeStep::Recipients => {
                let mut body = v_flex()
                    .gap_3()
                    .p_0().child(heading("merge.flow.start-title"))
                    .when(m.records.is_empty(), |d| {
                        d.child(note("merge.flow.start-help"))
                    })
                    .child(
                        {
                            let current=m.kind_draft.or_else(||self.flow().and_then(|d|d.mail_merge_type.as_deref())).unwrap_or("formLetters");
                            let kinds=[("docx.merge-panel.type-letters","formLetters"),("docx.merge-panel.type-envelopes","envelopes"),("docx.merge-panel.type-labels","mailingLabels"),("docx.merge-panel.type-directory","catalog"),("docx.merge-panel.type-email","email")];
                            let label=kinds.iter().find(|(_,value)|*value==current).map(|(key,_)|tr(key)).unwrap_or_else(||tr("docx.merge-panel.type-letters"));
                            let entries=kinds.into_iter().map(|(key,value)|chrome::MenuItem::new("merge.kind.option",key).on(move|e,_,cx|{
                                let m=e.doc_state_mut().unwrap().merge.as_mut().unwrap();m.kind_draft=Some(value);m.replace_body_ack=false;
                                if !matches!(value,"envelopes"|"mailingLabels"){e.set_mail_merge_type(Some(value),cx);}cx.notify();
                            }).into()).collect();
                            chrome::Dropdown::from_entries("merge-kind",label,entries,true,0,&cx.weak_entity()).into_any_element()
                        },
                    );
                if m.kind_draft.is_some_and(|k|matches!(k,"envelopes"|"mailingLabels")){
                    if m.kind_draft==Some("envelopes"){
                        let entries=EnvelopePreset::ALL.into_iter().map(|p|chrome::MenuItem::new("merge.envelope.preset",p.label()).on(move|e,_,cx|{e.doc_state_mut().unwrap().merge.as_mut().unwrap().envelope_preset=p;cx.notify();}).into()).collect();
                        body=body.child(chrome::Dropdown::from_entries("merge-envelope-preset",m.envelope_preset.label(),entries,true,0,&cx.weak_entity()).into_any_element()).child(text_field(&m.sender_field).full_width());
                    }else{
                        let entries=LabelPreset::ALL.into_iter().map(|p|chrome::MenuItem::new("merge.label.preset",p.label()).on(move|e,_,cx|{e.doc_state_mut().unwrap().merge.as_mut().unwrap().label_preset=p;cx.notify();}).into()).collect();
                        body=body.child(chrome::Dropdown::from_entries("merge-label-preset",m.label_preset.label(),entries,true,0,&cx.weak_entity()).into_any_element());
                    }
                    body=body.child(note("merge.review.replace-warning"))
                        .child(chrome::checkbox_button("merge-inline-replace-ack",tr("merge.review.replace-ack"),m.replace_body_ack,false,cx.listener(|e,_,_,cx|{let m=e.doc_state_mut().unwrap().merge.as_mut().unwrap();m.replace_body_ack=!m.replace_body_ack;cx.notify();})))
                        .child(action_button("merge-inline-kind-apply").disabled(!m.replace_body_ack).label(tr("merge.review.replace-action")).on_click(cx.listener(|e,_,_,cx|{e.apply_merge_kind_review(cx);if let Some(m)=e.doc_state_mut().and_then(|s|s.merge.as_mut()){m.kind_draft=None;}})));
                }
                if m.records.is_empty() {
                    body = body.child(
                        div()
                            .p_3()
                            .rounded_md()
                            .bg(rgb(0xf3f6fa))
                            .text_sm()
                            .child(tr("merge.flow.equation")),
                    );
                } else {
                    body = body
                        .child(
                            div()
                                .p_3()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(chrome::BORDER))
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .child(m.source_name.clone())
                                        .child(tf(
                                            "merge.flow.count",
                                            &[
                                                ("n", &count.to_string()),
                                                ("total", &m.records.len().to_string()),
                                            ],
                                        ))
                                        .child(m.fields.join(" · ")),
                                ),
                        )
                        .child(
                            action_button("merge-recipients")
                                .label(tr("merge.flow.edit-recipients"))
                                .on_click(cx.listener(|e, _, _, cx| {
                                    e.open_merge_task(MergeDialog::Recipients, cx)
                                })),
                        );
                }
                body = body.when(!m.records.is_empty(), |d| {
                    d.child(note("merge.review.copy-help"))
                        .child(
                            action_button("merge-save-list")
                                .label(tr("merge.review.save-list"))
                                .tooltip(tr("merge.review.resume-help"))
                                .on_click(
                                    cx.listener(|e, _, w, cx| e.save_merge_recipient_list(w, cx)),
                                ),
                        )
                        .child(note("merge.review.resume-short"))
                });
                body.child(
                    (if m.records.is_empty() {
                        primary_button("merge-file")
                    } else {
                        action_button("merge-file")
                    })
                    .label(tr("merge.flow.load"))
                    .on_click(cx.listener(|e, _, w, cx| e.pick_merge_file(w, cx))),
                )
                .child(note("merge.flow.formats"))
                .child(
                    action_button("merge-new-list")
                        .label(tr("merge.flow.new-list"))
                        .on_click(cx.listener(|e, _, _, cx| {
                            e.open_merge_task(MergeDialog::Recipients, cx);
                            e.set_merge_data(
                                vec![
                                    tr("merge.flow.name").to_string(),
                                    tr("merge.flow.company").to_string(),
                                    tr("merge.flow.address").to_string(),
                                ],
                                vec![],
                                cx,
                            );
                            if let Some(m) = e.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                                m.source_name.clear();
                            }
                        })),
                )
                .into_any_element()
            }
            MergeStep::Compose => v_flex()
                .gap_3()
                .child(heading("merge.flow.compose-title"))
                .child(note("merge.flow.compose-help"))
                .child(self.review_field_list(cx))
                .child(
                    action_button("merge-advanced")
                        .label(tr("merge.flow.advanced"))
                        .selected(m.advanced)
                        .on_click(cx.listener(|e, _, _, cx| {
                            if let Some(m) = e.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                                m.advanced = !m.advanced;
                            }
                            cx.notify();
                        })),
                )
                .when(m.advanced, |v| {
                    v.child(
                        v_flex()
                            .gap_2()
                            .child(
                                action_button("merge-address")
                                    .label(tr("docx.merge-panel.address-block-shown"))
                                    .on_click(
                                        cx.listener(|e, _, _, cx| e.insert_merge_block(true, cx)),
                                    ),
                            )
                            .child(
                                action_button("merge-greeting")
                                    .label(tr("docx.merge-panel.greeting-line-shown"))
                                    .on_click(
                                        cx.listener(|e, _, _, cx| e.insert_merge_block(false, cx)),
                                    ),
                            )
                            .child(
                                action_button("merge-condition")
                                    .label(tr("merge.flow.condition"))
                                    .on_click(cx.listener(|e, _, _, cx| {
                                        e.open_merge_task(MergeDialog::Condition, cx)
                                    })),
                            )
                            .child(
                                action_button("merge-match-fields")
                                    .label(tr("docx.merge-panel.match-fields"))
                                    .on_click(cx.listener(|e, _, _, cx| {
                                        e.open_merge_task(MergeDialog::Mapping, cx)
                                    })),
                            ),
                    )
                })
                .into_any_element(),
            MergeStep::Preview => {
                let name = selected
                    .get(m.current.min(count.saturating_sub(1)))
                    .map(|&i| m.recipient_name(i))
                    .unwrap_or_default();
                v_flex()
                    .gap_3()
                    .child(heading("merge.flow.preview-title"))
                    .child(note("merge.flow.preview-help"))
                    .child(
                        chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                            .child(
                                chrome::segment_control::SegmentItem::label(
                                    "merge-show-fields",
                                    tr("merge.flow.fields"),
                                    !m.preview,
                                )
                                .on_click(
                                    cx.listener(|e, _, _, cx| e.set_merge_preview(false, cx)),
                                ),
                            )
                            .child(
                                chrome::segment_control::SegmentItem::label(
                                    "merge-show-result",
                                    tr("merge.flow.result"),
                                    m.preview,
                                )
                                .on_click(cx.listener(|e, _, _, cx| e.set_merge_preview(true, cx))),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .justify_center()
                            .items_center()
                            .child(
                                action_button("merge-prev").label("‹").on_click(
                                    cx.listener(|e, _, _, cx| e.step_merge_record(-1, cx)),
                                ),
                            )
                            .child(div().child(format!("{} / {}", m.current + 1, count)))
                            .child(
                                action_button("merge-next").label("›").on_click(
                                    cx.listener(|e, _, _, cx| e.step_merge_record(1, cx)),
                                ),
                            ),
                    )
                    .child(div().text_center().child(name))
                    .child(self.review_results(cx))
                    .child(
                        action_button("merge-check-errors")
                            .label(tr("docx.merge-panel.check-errors"))
                            .on_click(cx.listener(|e, _, _, cx| {
                                e.check_merge_errors(cx);
                            })),
                    )
                    .child(
                        action_button("merge-recipients")
                            .label(tr("merge.flow.edit-recipients"))
                            .on_click(cx.listener(|e, _, _, cx| {
                                e.open_merge_task(MergeDialog::Recipients, cx)
                            })),
                    )
                    .into_any_element()
            }
            MergeStep::Output => v_flex()
                .gap_3()
                .child(heading("merge.flow.output-title"))
                .child(note("merge.flow.output-help"))
                .child(tf("merge.flow.target-count", &[("n", &count.to_string())]))
                .children(
                    [
                        (MergeOutput::Document, "merge.flow.document"),
                        (MergeOutput::Pdf, "merge.flow.pdf"),
                        (MergeOutput::Print, "merge.flow.print"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(i, (kind, key))| {
                        radio_button(
                            ("merge-output", i),
                            m.output == kind,
                            false,
                            tr(key),
                            cx.listener(move |e, _, _, cx| {
                                if let Some(m) = e.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                                    m.output = kind;
                                }
                                cx.notify();
                            }),
                        )
                    }),
                )
                .into_any_element(),
        };
        let index = MergeStep::ALL.iter().position(|&s| s == step).unwrap_or(0);
        let next = MergeStep::ALL.get(index + 1).copied();
        let disabled = count == 0
            || (step == MergeStep::Compose && used.is_empty())
            || (matches!(step, MergeStep::Preview | MergeStep::Output)
                && !self.review_can_output());
        // Wizard exception: navigation and primary action are right-aligned, primary last.

        let footer = chrome::action_bar()
            .when(index > 0, |v| {
                v.action(
                    crate::shell::chrome::ActionDefinition::new("merge-back", tr("merge.flow.back"), crate::shell::chrome::ActionButtonKind::Normal)
                        .on_click(cx.listener(move |e, _, _, cx| {
                            e.set_merge_step(MergeStep::ALL[index - 1], cx)
                        })),
                )
            })
            .action(
                crate::shell::chrome::ActionDefinition::new("merge-continue", tr(match step {
                        MergeStep::Recipients => "merge.flow.next-compose",
                        MergeStep::Compose => "merge.flow.next-preview",
                        MergeStep::Preview => "merge.flow.next-output",
                        MergeStep::Output => "merge.flow.settings",
                    }), crate::shell::chrome::ActionButtonKind::Primary)
                    .disabled(disabled)
                    .on_click(cx.listener(move |e, _, _, cx| {
                        if let Some(next) = next {
                            e.set_merge_step(next, cx);
                        } else {
                            e.open_merge_task(MergeDialog::Output, cx);
                        }
                    })),
            );
        let body = body.child(content);
        Some(SidebarPanel::with_footer(
            tr("merge.flow.title"),
            body.into_any_element(),
            footer,
        ))
    }
    pub(crate) fn merge_workflow_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(SidebarPanel, &'static str)> {
        if !self.merge_state()?.visible {
            return None;
        }
        if self.merge_state()?.row_editor.is_some() {
            return self.merge_row_dialog(cx).map(|p| (p, "merge_row_editor"));
        }
        let m = self.merge_state()?;
        let task = m.dialog?;
        let cancel = crate::shell::chrome::dialog_cancel_definition(cx);
        let ok = crate::shell::chrome::ActionDefinition::new("merge-task-ok", tr("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary)
            .on_click(cx.listener(|e, _, _, cx| e.finish_merge_task(cx)));
        let (key, title, body, footer) = match task {
            MergeDialog::Recipients => {
                return self
                    .review_recipient_dialog(cx)
                    .map(|p| (p, "merge_recipients"));
            }
            MergeDialog::Import => {
                return self.review_import_dialog(cx).map(|p| (p, "merge_import"));
            }
            MergeDialog::Columns => {
                let body = v_flex()
                    .p_3()
                    .gap_3()
                    .child(note("merge.flow.columns-help"))
                    .children(m.fields.iter().enumerate().map(|(i, f)| {
                        h_flex().gap_2().justify_between().child(f.clone()).child(
                            action_button(("merge-field-delete", i))
                                .icon(crate::shell::icons::Material("delete"))
                                .on_click(
                                    cx.listener(move |e, _, w, cx| e.delete_merge_field(i, w, cx)),
                                ),
                        )
                    }))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(text_field(&m.new_field).full_width())
                            .child(
                                action_button("merge-add-field")
                                    .label(tr("docx.merge-panel.add-field"))
                                    .on_click(cx.listener(|e, _, w, cx| {
                                        let name = e
                                            .merge_state()
                                            .unwrap()
                                            .new_field
                                            .read(cx)
                                            .value()
                                            .to_string();
                                        e.add_merge_field(&name, w, cx);
                                    })),
                            ),
                    )
                    .child(note("merge.flow.recipient-draft"));
                (
                    "merge_columns",
                    "merge.flow.columns",
                    body.into_any_element(),
                    action_bar().action(ok).action(cancel),
                )
            }
            MergeDialog::Filter => {
                let body = v_flex()
                    .p_3()
                    .gap_3()
                    .child(note("merge.flow.filter-simple-help"))
                    .children(m.fields.iter().enumerate().map(|(i, f)| {
                        radio_button(
                            ("merge-filter-field", i),
                            m.filter_column == i,
                            false,
                            f.clone(),
                            cx.listener(move |e, _, _, cx| {
                                e.doc_state_mut()
                                    .unwrap()
                                    .merge
                                    .as_mut()
                                    .unwrap()
                                    .filter_column = i;
                                cx.notify();
                            }),
                        )
                    }))
                    .child(
                        chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                            .child(
                                chrome::segment_control::SegmentItem::label(
                                    "merge-filter-contains",
                                    tr("merge.flow.contains"),
                                    m.filter_contains,
                                )
                                .on_click(cx.listener(
                                    |e, _, _, cx| {
                                        e.doc_state_mut()
                                            .unwrap()
                                            .merge
                                            .as_mut()
                                            .unwrap()
                                            .filter_contains = true;
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                chrome::segment_control::SegmentItem::label(
                                    "merge-filter-exact",
                                    tr("merge.flow.exact"),
                                    !m.filter_contains,
                                )
                                .on_click(cx.listener(
                                    |e, _, _, cx| {
                                        e.doc_state_mut()
                                            .unwrap()
                                            .merge
                                            .as_mut()
                                            .unwrap()
                                            .filter_contains = false;
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                    .child(text_field(&m.filter_field).full_width());
                (
                    "merge_filter",
                    "merge.flow.filter",
                    body.into_any_element(),
                    action_bar()
                        .action(
                            crate::shell::chrome::ActionDefinition::new("merge-filter-ok", tr("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary)
                                .on_click(cx.listener(|e, _, _, cx| {
                                    if let Some(m) =
                                        e.doc_state_mut().and_then(|s| s.merge.as_mut())
                                    {
                                        let value = m.filter_field.read(cx).value().to_string();
                                        m.filter = (!value.trim().is_empty()).then(|| {
                                        ibis_doc_model::merge::RecordFilter {
                                            any_of: vec![vec![
                                                ibis_doc_model::merge::FilterCondition {
                                                    field: m.filter_column,
                                                    op: if m.filter_contains {
                                                        ibis_doc_model::merge::FilterOp::Contains
                                                    } else {
                                                        ibis_doc_model::merge::FilterOp::Equal
                                                    },
                                                    value,
                                                },
                                            ]],
                                        }
                                    });
                                        m.current = 0;
                                    }
                                    e.sync_merge_preview(cx);
                                    e.finish_merge_task(cx);
                                })),
                        )
                        .action(cancel),
                )
            }

            MergeDialog::Condition => {
                let body = v_flex()
                    .p_3()
                    .gap_3()
                    .child(note("merge.flow.condition-help"))
                    .child(note("merge.flow.condition-field"))
                    .children(m.fields.iter().enumerate().map(|(i, f)| {
                        radio_button(
                            ("merge-condition-field", i),
                            m.condition_column == i,
                            false,
                            f.clone(),
                            cx.listener(move |e, _, _, cx| {
                                if let Some(m) = e.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                                    m.condition_column = i;
                                }
                                cx.notify();
                            }),
                        )
                    }))
                    .child(note("merge.flow.equals"))
                    .child(text_field(&m.condition_value).full_width())
                    .child(note("merge.flow.yes"))
                    .child(text_field(&m.condition_yes).full_width())
                    .child(note("merge.flow.no"))
                    .child(text_field(&m.condition_no).full_width());
                (
                    "merge_condition",
                    "merge.flow.condition",
                    body.into_any_element(),
                    action_bar()
                        .action(
                            crate::shell::chrome::ActionDefinition::new("merge-condition-ok", tr("_shell.insert"), crate::shell::chrome::ActionButtonKind::Primary)
                                .on_click(cx.listener(|e, _, w, cx| {
                                    let Some(m) = e.merge_state() else { return };
                                    let Some(field) = m.fields.get(m.condition_column) else {
                                        return;
                                    };
                                    let quote =
                                        |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
                                    let rule = format!(
                                        "IF {{ {} }} = \"{}\" \"{}\" \"{}\"",
                                        merge_field_instruction(field),
                                        quote(&m.condition_value.read(cx).value()),
                                        quote(&m.condition_yes.read(cx).value()),
                                        quote(&m.condition_no.read(cx).value())
                                    );
                                    let input = m.rule_field.clone();
                                    input.update(cx, |i, cx| i.set_value(rule, w, cx));
                                    e.insert_merge_rule(cx);
                                    e.finish_merge_task(cx);
                                })),
                        )
                        .action(cancel),
                )
            }
            MergeDialog::Mapping => {
                let map = m.mapping_draft.clone();
                let body = v_flex()
                    .p_3()
                    .gap_2()
                    .child(note("docx.merge-panel.match-fields-guide"))
                    .child(
                        action_button("merge-auto-match")
                            .label(tr("docx.merge.auto-match-fields"))
                            .on_click(cx.listener(|e, _, _, cx| {
                                if let Some(m) = e.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                                    m.mapping_draft =
                                        ibis_doc_model::merge::auto_match_fields(&m.fields);
                                }
                                cx.notify();
                            })),
                    )
                    .children(
                        ibis_doc_model::merge::STANDARD_MERGE_FIELDS
                            .iter()
                            .enumerate()
                            .map(|(i, f)| {
                                let field = f.mapped_name;
                                let name = map
                                    .iter()
                                    .find(|m| m.mapped_name == field)
                                    .map(|m| m.name.clone())
                                    .unwrap_or_else(|| {
                                        tr("docx.merge-panel.unmatched").to_string()
                                    });
                                h_flex().gap_2().justify_between().child(field).child(
                                    action_button(("merge-map-col", i)).label(name).on_click(
                                        cx.listener(move |e, _, _, cx| {
                                            e.cycle_merge_mapping_draft(field, cx)
                                        }),
                                    ),
                                )
                            }),
                    );
                (
                    "merge_mapping",
                    "docx.merge-panel.match-fields",
                    body.into_any_element(),
                    action_bar()
                        .action(
                            crate::shell::chrome::ActionDefinition::new("merge-mapping-ok", tr("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary)
                                .on_click(cx.listener(|e, _, _, cx| {
                                    let map = e.merge_state().unwrap().mapping_draft.clone();
                                    e.run_flow(vec![FlowCommand::SetMailMergeFieldMap(map)], cx);
                                    e.sync_merge_preview(cx);
                                    e.finish_merge_task(cx);
                                })),
                        )
                        .action(cancel),
                )
            }
            MergeDialog::Kind => {
                let kind = m
                    .kind_draft
                    .or_else(|| self.flow().and_then(|d| d.mail_merge_type.as_deref()));
                let replaces = matches!(kind, Some("envelopes" | "mailingLabels"));
                let body = v_flex()
                    .p_3()
                    .gap_3()
                    .child(note("merge.review.kind-help"))
                    .when(replaces, |d| {
                        d.child(
                            div()
                                .p_3()
                                .rounded_md()
                                .bg(rgb(0xfff4e5))
                                .text_sm()
                                .child(tr("merge.review.replace-warning")),
                        )
                        .child(chrome::checkbox_button(
                            "merge-replace-ack",
                            tr("merge.review.replace-ack"),
                            m.replace_body_ack,
                            false,
                            cx.listener(|e, _, _, cx| {
                                let m = e.doc_state_mut().unwrap().merge.as_mut().unwrap();
                                m.replace_body_ack = !m.replace_body_ack;
                                cx.notify();
                            }),
                        ))
                    })
                    .children(
                        [
                            ("docx.merge-panel.type-letters", "formLetters"),
                            ("docx.merge-panel.type-envelopes", "envelopes"),
                            ("docx.merge-panel.type-labels", "mailingLabels"),
                            ("docx.merge-panel.type-directory", "catalog"),
                            ("docx.merge-panel.type-email", "email"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (label, value))| {
                            radio_button(
                                ("merge-kind", i),
                                kind == Some(value) || kind.is_none() && i == 0,
                                false,
                                tr(label),
                                cx.listener(move |e, _, _, cx| {
                                    if let Some(m) =
                                        e.doc_state_mut().and_then(|s| s.merge.as_mut())
                                    {
                                        m.kind_draft = Some(value);
                                        m.replace_body_ack = false;
                                    }
                                    cx.notify();
                                }),
                            )
                        }),
                    )
                    .when(kind == Some("envelopes"), |d| {
                        d.child(note("docx.merge-panel.envelope-setup"))
                            .child(h_flex().gap_1().flex_wrap().children(
                                EnvelopePreset::ALL.into_iter().enumerate().map(|(i, p)| {
                                    radio_button(("merge-env-preset", i),m.envelope_preset == p,false,p.label(),cx.listener(move |e, _, _, cx| {
                                            e.doc_state_mut()
                                                .unwrap()
                                                .merge
                                                .as_mut()
                                                .unwrap()
                                                .envelope_preset = p;
                                            cx.notify();
                                        }))
                                }),
                            ))
                            .child(text_field(&m.sender_field).full_width())
                    })
                    .when(kind == Some("mailingLabels"), |d| {
                        d.child(note("docx.merge-panel.label-setup")).child(
                            h_flex().gap_1().flex_wrap().children(
                                LabelPreset::ALL.into_iter().enumerate().map(|(i, p)| {
                                    radio_button(("merge-lbl-preset", i),m.label_preset == p,false,p.label(),cx.listener(move |e, _, _, cx| {
                                            e.doc_state_mut()
                                                .unwrap()
                                                .merge
                                                .as_mut()
                                                .unwrap()
                                                .label_preset = p;
                                            cx.notify();
                                        }))
                                }),
                            ),
                        )
                    });
                (
                    "merge_kind",
                    "docx.merge-panel.document-type",
                    body.into_any_element(),
                    action_bar()
                        .action(
                            crate::shell::chrome::ActionDefinition::new("merge-kind-ok", tr(if replaces {
                                    "merge.review.replace-action"
                                } else {
                                    "_shell.apply"
                                }), crate::shell::chrome::ActionButtonKind::Primary)
                                .disabled(replaces && !m.replace_body_ack)
                                .on_click(cx.listener(|e, _, _, cx| e.apply_merge_kind_review(cx))),
                        )
                        .action(cancel),
                )
            }
            MergeDialog::Output => {
                let body = v_flex()
                    .p_3()
                    .gap_3()
                    .child(self.review_output_summary(cx))
                    .when(m.output == MergeOutput::Document, |d| {
                        d.child(note("merge.flow.combine"))
                            .child(radio_button(
                                "merge-one-file",
                                !m.separate,
                                false,
                                tr("merge.flow.one-file"),
                                cx.listener(|e, _, _, cx| {
                                    e.doc_state_mut().unwrap().merge.as_mut().unwrap().separate =
                                        false;
                                    cx.notify();
                                }),
                            ))
                            .child(radio_button(
                                "merge-separate-file",
                                m.separate,
                                false,
                                tr("merge.flow.separate-file"),
                                cx.listener(|e, _, _, cx| {
                                    e.doc_state_mut().unwrap().merge.as_mut().unwrap().separate =
                                        true;
                                    cx.notify();
                                }),
                            ))
                    })
                    .child(note("merge.review.output-range"))
                    .children(
                        [
                            (MergePrintRange::All, "docx.merge-panel.range-all"),
                            (MergePrintRange::Current, "docx.merge-panel.range-current"),
                            (
                                MergePrintRange::FromTo(1, 1),
                                "docx.merge-panel.range-from-to",
                            ),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (r, key))| {
                            radio_button(
                                ("merge-range", i),
                                std::mem::discriminant(&m.print_range)
                                    == std::mem::discriminant(&r),
                                false,
                                tr(key),
                                cx.listener(move |e, _, _, cx| e.set_merge_print_range(r, cx)),
                            )
                        }),
                    )
                    .when(matches!(m.print_range, MergePrintRange::FromTo(..)), |d| {
                        d.child(
                            h_flex()
                                .gap_2()
                                .flex_wrap()
                                .child(
                                    text_field(&m.print_from_field).spinner(
                                        "merge-print-from",
                                        chrome::SpinnerSpec::new(1.)
                                            .range(1., m.selected().len().max(1) as f32)
                                            .integer_only(),
                                        cx,
                                        |_, _, _, _| {},
                                    ),
                                )
                                .child(div().child("–"))
                                .child(
                                    text_field(&m.print_to_field).spinner(
                                        "merge-print-to",
                                        chrome::SpinnerSpec::new(1.)
                                            .range(1., m.selected().len().max(1) as f32)
                                            .integer_only(),
                                        cx,
                                        |_, _, _, _| {},
                                    ),
                                ),
                        )
                    })
                    .child(note("merge.flow.output-location"));
                let label = match m.output {
                    MergeOutput::Document => "merge.flow.document",
                    MergeOutput::Pdf => "merge.flow.pdf",
                    MergeOutput::Print => "merge.flow.print",
                };
                (
                    "merge_output",
                    "merge.flow.output-title",
                    body.into_any_element(),
                    action_bar()
                        .action(
                            crate::shell::chrome::ActionDefinition::new("merge-output-confirm", tr(label), crate::shell::chrome::ActionButtonKind::Primary)
                                .disabled(
                                    !self.review_can_output()
                                        || self.merge_output_indices(cx).is_err()
                                        || self
                                            .merge_output_indices(cx)
                                            .is_ok_and(|r| r.is_empty()),
                                )
                                .on_click(cx.listener(|e, _, w, cx| {
                                    if !e.review_can_output() {
                                        if let Some(m) =
                                            e.doc_state_mut().and_then(|s| s.merge.as_mut())
                                        {
                                            m.step = MergeStep::Preview;
                                            m.dialog = None;
                                        }
                                        cx.notify();
                                        return;
                                    }
                                    let Some(m) = e.merge_state() else { return };
                                    let output = m.output;
                                    let separate = m.separate;
                                    if let Err(err) = e.merge_output_indices(cx) {
                                        e.status = Some(err.into());
                                        cx.notify();
                                        return;
                                    }
                                    // Validate before opening a save/print prompt; invalid ranges retain this dialog.
                                    if let Err(err) =
                                        e.merged_document_for_range(e.current_merge_print_range(cx))
                                    {
                                        e.status = Some(err.into());
                                        cx.notify();
                                        return;
                                    }
                                    e.finish_merge_task(cx);
                                    match output {
                                        MergeOutput::Document if separate => {
                                            e.save_merged_separately(w, cx)
                                        }
                                        MergeOutput::Document => e.save_merged_documents(w, cx),
                                        MergeOutput::Pdf => e.export_merged_pdf(w, cx),
                                        MergeOutput::Print => e.print_merged_documents(w, cx),
                                    }
                                })),
                        )
                        .action(cancel),
                )
            }
        };
        Some((
            SidebarPanel::with_footer(tr(title).trim_end_matches('…').to_string(), body, footer),
            key,
        ))
    }
}
