// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Explicit recipient scope, preflight, editable import and accessible lists.
use super::*;
use gpui_kit::component::input::InputEvent;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MergeIssue {
    pub(crate) record: Option<usize>,
    pub(crate) field: String,
}
impl MergeState {
    pub(crate) fn recipient_name(&self, index: usize) -> String {
        self.records
            .get(index)
            .map(|r| {
                r.iter()
                    .take(2)
                    .filter(|v| !v.trim().is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                tf(
                    "merge.review.row-number",
                    &[("n", &(index + 1).to_string())],
                )
            })
    }
    fn matches_filter(&self, i: usize) -> bool {
        self.filter
            .as_ref()
            .is_none_or(|f| f.matches(&self.records[i]))
    }
    pub(crate) fn visible_recipient_indices(&self) -> Vec<usize> {
        (0..self.records.len())
            .filter(|&i| {
                (!self.targets_only
                    || (self.included.get(i).copied().unwrap_or(true) && self.matches_filter(i)))
                    && (self.list_search.is_empty()
                        || self.records[i]
                            .iter()
                            .any(|v| v.contains(&self.list_search)))
            })
            .collect()
    }
}
impl SlideEditor {
    pub(crate) fn watch_merge_inputs(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.merge_state() else { return };
        if !m.subscriptions.is_empty() {
            return;
        }
        let find = m.find_field.clone();
        let numeric = [m.print_from_field.clone(), m.print_to_field.clone()];
        let mut subs = vec![
            cx.subscribe_in(&find, w, |e, input, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value().to_string();
                    if let Some(m) = e.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                        m.list_search = value;
                        m.list_page = 0;
                    }
                    cx.notify();
                }
            }),
        ];
        for input in numeric {
            subs.push(
                cx.subscribe_in(&input, w, |_, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                }),
            );
        }
        self.doc_state_mut()
            .unwrap()
            .merge
            .as_mut()
            .unwrap()
            .subscriptions = subs;
    }
    pub(crate) fn select_merge_field(
        &mut self,
        index: usize,
        insert: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .merge_state()
            .and_then(|m| m.fields.get(index))
            .cloned()
        else {
            return;
        };
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.selected_field = Some(index);
            m.review_ack = false;
        }
        if insert {
            self.insert_merge_field(&name, cx);
        }
        cx.notify();
    }
    pub(crate) fn review_field_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let m = self.merge_state().unwrap();
        let used = self
            .flow()
            .map(|d| merge_fields_used(d))
            .unwrap_or_default();
        v_flex()
            .id("merge-fields-list")
            .role(Role::ListBox)
            .aria_label(tr("merge.review.fields-list"))
            .track_focus(&m.field_focus)
            .tab_index(0)
            .w_full()
            .border_1()
            .border_color(rgb(chrome::BORDER))
            .rounded_md()
            .overflow_hidden()
            .on_key_down(cx.listener(|e, ev: &KeyDownEvent, _, cx| {
                let Some(m) = e.merge_state() else { return };
                if m.fields.is_empty() {
                    return;
                };
                let current = m.selected_field.unwrap_or(0);
                let last = m.fields.len() - 1;
                match ev.keystroke.key.as_str() {
                    "up" => e.select_merge_field(current.saturating_sub(1), false, cx),
                    "down" => e.select_merge_field((current + 1).min(last), false, cx),
                    "enter" | "space" => e.select_merge_field(current, true, cx),
                    _ => return,
                };
                cx.stop_propagation();
            }))
            .children(m.fields.iter().enumerate().map(|(i, name)| {
                let focus = m.field_focus.clone();
                let selected = m.selected_field == Some(i);
                let inserted = used.iter().any(|f| f.eq_ignore_ascii_case(name));
                h_flex()
                    .id(("merge-field-row", i))
                    .role(Role::ListBoxOption)
                    .aria_label(name.clone())
                    .aria_selected(selected)
                    .w_full()
                    .min_h(px(32.))
                    .px_3()
                    .py_2()
                    .gap_2()
                    .items_center()
                    .justify_start()
                    .cursor_pointer()
                    .when(i > 0, |d| d.border_t_1().border_color(rgb(chrome::BORDER)))
                    .hover(|s| s.bg(rgb(0xf5f5f5)))
                    .child(
                        div()
                            .w(px(18.))
                            .flex_none()
                            .text_color(rgb(chrome::PRIMARY_BUTTON_BG))
                            .child(if inserted { "✓" } else { "" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_left()
                            .child(name.clone()),
                    )
                    .child(
                        action_button(("merge-field-insert", i))
                            .label(tr("_shell.insert"))
                            .accessibility_label(format!("{} {}", name, tr("_shell.insert")))
                            .on_click(cx.listener(move |e, _, _, cx| {
                                cx.stop_propagation();
                                e.select_merge_field(i, true, cx)
                            })),
                    )
                    .on_click(cx.listener(move |e, _, w, cx| {
                        focus.focus(w, cx);
                        e.select_merge_field(i, false, cx)
                    }))
            }))
            .into_any_element()
    }
    pub(crate) fn merge_review_issues(&self) -> Vec<MergeIssue> {
        let (Some(m), Some(doc)) = (self.merge_state(), self.flow()) else {
            return vec![];
        };
        let mut issues = Vec::new();
        for field in merge_fields_used(doc) {
            let column = m
                .fields
                .iter()
                .position(|f| f.eq_ignore_ascii_case(&field))
                .or_else(|| {
                    doc.mail_merge_field_map
                        .iter()
                        .find(|f| f.mapped_name.eq_ignore_ascii_case(&field))
                        .and_then(|map| {
                            m.fields
                                .iter()
                                .position(|f| f.eq_ignore_ascii_case(&map.name))
                        })
                });
            if let Some(column) = column {
                for i in m.selected() {
                    if m.records[i].get(column).is_none_or(|v| v.trim().is_empty()) {
                        issues.push(MergeIssue {
                            record: Some(i),
                            field: field.clone(),
                        });
                    }
                }
            } else {
                issues.push(MergeIssue {
                    record: None,
                    field,
                });
            }
        }
        issues
    }
    fn review_signature(&self) -> String {
        let m = self.merge_state().unwrap();
        self.merge_review_issues()
            .iter()
            .map(|i| {
                format!(
                    "{:?}:{}:{}",
                    i.record,
                    i.field,
                    i.record.map(|r| m.recipient_name(r)).unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub(crate) fn review_can_output(&self) -> bool {
        let issues = self.merge_review_issues();
        issues.is_empty()
            || (issues.iter().all(|i| i.record.is_some())
                && self.merge_state().is_some_and(|m| {
                    m.review_ack && m.review_ack_signature == self.review_signature()
                }))
    }
    pub(crate) fn acknowledge_merge_warnings(&mut self, cx: &mut Context<Self>) {
        let signature = self.review_signature();
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.review_ack = !m.review_ack;
            m.review_ack_signature = signature;
        }
        cx.notify();
    }
    pub(crate) fn review_results(&self, cx: &mut Context<Self>) -> AnyElement {
        let issues = self.merge_review_issues();
        let missing = issues.iter().any(|i| i.record.is_none());
        v_flex()
            .gap_2()
            .p_2()
            .rounded_md()
            .bg(rgb(if issues.is_empty() {
                0xeef7f1
            } else {
                0xfff4e5
            }))
            .child(div().text_sm().child(if issues.is_empty() {
                tr("merge.review.no-issues").to_string()
            } else {
                tf(
                    "merge.review.issue-count",
                    &[("n", &issues.len().to_string())],
                )
            }))
            .children(issues.iter().enumerate().take(20).map(|(n, issue)| {
                let field = issue.field.clone();
                let record = issue.record;
                let label = if let Some(i) = record {
                    tf(
                        "merge.review.empty-field",
                        &[
                            ("name", &self.merge_state().unwrap().recipient_name(i)),
                            ("field", &field),
                        ],
                    )
                } else {
                    tf("merge.review.missing-field", &[("field", &field)])
                };
                div()
                    .id(("merge-issue", n))
                    .role(Role::Button)
                    .aria_label(label.clone())
                    .w_full()
                    .min_w_0()
                    .p_2()
                    .text_sm()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(chrome::SELECTED_ROW_BG)))
                    .child(label)
                    .on_click(cx.listener(move |e, _, w, cx| {
                        if let Some(i) = record {
                            e.open_merge_task(MergeDialog::Recipients, cx);
                            e.open_merge_row(Some(i), w, cx);
                        } else {
                            e.jump_merge_missing_field(&field, cx);
                        }
                    }))
            }))
            .when(issues.len() > 20, |d| {
                d.child(note("merge.review.more-issues"))
            })
            .when(!issues.is_empty() && !missing, |d| {
                d.child(chrome::checkbox_button(
                    "merge-warnings-ack",
                    tr("merge.review.continue-empty"),
                    self.review_can_output(),
                    false,
                    cx.listener(|e, _, _, cx| e.acknowledge_merge_warnings(cx)),
                ))
            })
            .when(missing, |d| d.child(note("merge.review.fix-missing")))
            .into_any_element()
    }
    fn jump_merge_missing_field(&mut self, field: &str, cx: &mut Context<Self>) {
        fn find(blocks: &[ibis_doc_model::Block], name: &str) -> Option<FlowPos> {
            for b in blocks {
                match b {
                    ibis_doc_model::Block::Paragraph(p) => {
                        let mut offset = 0;
                        for r in &p.runs {
                            if let FlowRunKind::Field { instruction, .. } = &r.kind {
                                if instruction.contains(name) {
                                    return Some(FlowPos::new(p.id, offset));
                                }
                            }
                            offset += r.len();
                        }
                    }
                    ibis_doc_model::Block::Table(t) => {
                        for row in &t.rows {
                            for cell in &row.cells {
                                if let Some(pos) = find(&cell.blocks, name) {
                                    return Some(pos);
                                }
                            }
                        }
                    }
                }
            }
            None
        }
        let pos = self.flow().and_then(|d| find(&d.body, field));
        self.set_merge_step(MergeStep::Compose, cx);
        if let Some(pos) = pos {
            self.set_caret(pos, false, cx);
        }
    }
    pub(crate) fn merge_output_indices(&self, cx: &App) -> Result<Vec<usize>, String> {
        let Some(m) = self.merge_state() else {
            return Ok(vec![]);
        };
        let selected = m.selected();
        let subset = match self.current_merge_print_range(cx) {
            MergePrintRange::All => selected,
            MergePrintRange::Current => selected.get(m.current).copied().into_iter().collect(),
            MergePrintRange::FromTo(a, b) => {
                if a == 0 || b < a || b > selected.len() {
                    return Err(tr("merge.review.invalid-range").to_string());
                }
                selected[a - 1..b].to_vec()
            }
        };
        Ok(subset
            .into_iter()
            .filter(|&i| {
                self.flow().is_none_or(|d| {
                    !ibis_doc_model::merge::record_skipped(d, &m.fields, &m.records[i])
                })
            })
            .collect())
    }
    pub(crate) fn review_output_summary(&self, cx: &App) -> AnyElement {
        match self.merge_output_indices(cx) {
            Ok(indices) => {
                let m = self.merge_state().unwrap();
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded_md()
                    .bg(rgb(0xedf5fc))
                    .child(tf(
                        "merge.review.actual-output",
                        &[("n", &indices.len().to_string())],
                    ))
                    .children(
                        indices
                            .iter()
                            .take(3)
                            .map(|&i| div().text_sm().child(m.recipient_name(i))),
                    )
                    .when(indices.len() > 3, |d| {
                        d.child(tf(
                            "merge.review.more-recipients",
                            &[("n", &(indices.len() - 3).to_string())],
                        ))
                    })
                    .into_any_element()
            }
            Err(error) => div()
                .text_sm()
                .text_color(rgb(0xc5312b))
                .child(error)
                .into_any_element(),
        }
    }
    pub(crate) fn set_visible_merge_recipients(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            let indices = m.visible_recipient_indices();
            m.included.resize(m.records.len(), true);
            for i in indices {
                if m.matches_filter(i) {
                    m.included[i] = on;
                }
            }
            m.current = 0;
            m.review_ack = false;
        }
        self.sync_merge_preview(cx);
        cx.notify();
    }
    pub(crate) fn clear_merge_filter(&mut self, cx: &mut Context<Self>) {
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            m.filter = None;
            m.review_ack = false;
            m.list_page = 0;
        }
        self.sync_merge_preview(cx);
        cx.notify();
    }
    pub(crate) fn review_recipient_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<SidebarPanel> {
        let m = self.merge_state()?;
        let indices = m.visible_recipient_indices();
        let selected = m.selected();
        let compact = self
            .panel_dialog
            .bounds
            .is_some_and(|b| f32::from(b.size.width) < 600.);
        let reorder=m.row_reorder.clone();
        let rows: Vec<(usize,AnyElement,bool)> = indices
            .iter()
            .copied()
            .skip(m.list_page * RECORDS_PER_PAGE)
            .take(RECORDS_PER_PAGE)
            .map(|i| {
                let r = &m.records[i];
                let eligible = m.matches_filter(i);
                let targeted = eligible && m.included.get(i).copied().unwrap_or(true);
                let status = tr(if !eligible {
                    "merge.review.filtered-out"
                } else if targeted {
                    "merge.review.included"
                } else {
                    "merge.review.excluded"
                });
                let edit=action_button(("merge-record-edit",i)).icon(crate::shell::icons::Material("edit")).tooltip(tr("merge.review.edit-row")).accessibility_label(format!("{} {}",m.recipient_name(i),tr("merge.review.edit-row"))).on_click(cx.listener(move|e,_,w,cx|{cx.stop_propagation();e.open_merge_row(Some(i),w,cx);}));
                let delete=action_button(("merge-record-delete",i)).icon(crate::shell::icons::Material("delete")).tooltip(tr("merge.review.delete-row")).accessibility_label(format!("{} {}",m.recipient_name(i),tr("merge.review.delete-row"))).on_click(cx.listener(move|e,_,w,cx|{cx.stop_propagation();e.delete_merge_record(i,w,cx);}));
                let controls=h_flex().gap_1().child(chrome::data_table::row_controls(reorder.clone(),i,1,edit)).child(chrome::data_table::row_controls(reorder.clone(),i,2,delete));
                let checkbox=||chrome::data_table::row_controls(reorder.clone(),i,0,div().w(px(24.)).flex_none().child(chrome::checkbox_button(("merge-record",i),"",targeted,!eligible,cx.listener(move|e,_,_,cx|e.toggle_merge_record(i,cx))).aria_label(m.recipient_name(i))));
                let row = div().id(("merge-recipient-row", i)).w_full().h_full().px_2()
                    .child(if compact {
                        v_flex()
                            .gap_2()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_start()
                                    .child(checkbox())
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_sm()
                                            .child(m.recipient_name(i)),
                                    ),
                            )
                            .child(div().text_sm().child(status.clone()))
                            .child(controls)
                            .into_any_element()
                    } else {
                        h_flex().w_full().h_full()
                            .gap_2()
                            .items_center()
                            .child(checkbox())
                            .children(m.fields.iter().enumerate().map(|(j, _)| {
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .child(r.get(j).cloned().unwrap_or_default())
                            }))
                            .child(div().w(px(60.)).text_sm().flex_none().child(status))
                            .child(h_flex().w(px(108.)).flex_none().items_center().child(div().w(px(80.)).flex_none().child(controls)).child(div().id(("merge-record-drag",i)).w(px(28.)).flex_none().cursor_pointer().child(Icon::new(crate::shell::icons::Material("menu")).size(px(18.)))))
                            .into_any_element()
                    })
                    .on_click(cx.listener(move |e, ev: &ClickEvent, w, cx| {
                        if let Some(m) = e.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                            m.selected_row = Some(i);
                        }
                        if ev.click_count() == 2 {
                            e.open_merge_row(Some(i), w, cx);
                        }
                        cx.notify();
                    }))
                    ;
                (i,row.into_any_element(),m.selected_row==Some(i))
            })
            .collect();
        let eligible: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&i| m.matches_filter(i))
            .collect();
        let checked = eligible
            .iter()
            .filter(|&&i| m.included.get(i).copied().unwrap_or(true))
            .count();
        let all = !eligible.is_empty() && checked == eligible.len();
        let mixed = checked > 0 && !all;
        let header_checkbox = || {
            chrome::Checkbox::new("merge-header-all")
                .checked(all)
                .indeterminate(mixed)
                .disabled(eligible.is_empty())
                .accessibility_label(tr("merge.review.select-all-header"))
                .on_click(
                    cx.listener(|e, on: &bool, _, cx| e.set_visible_merge_recipients(*on, cx)),
                )
        };
        let header=(!compact).then(||h_flex()
                        .w_full()
                        .px_2().h(px(chrome::data_table::TABLE_ROW_HEIGHT)).items_center()
                        .gap_2()
                        .bg(rgb(0xf3f4f6))
                        .child(div().w(px(24.)).flex_none().child(header_checkbox()))
                        .children(m.fields.iter().enumerate().map(|(i, f)| {
                            action_button(("merge-sort-column", i))
                                .flex_1()
                                .min_w_0()
                                .ghost()
                                .label(format!("{f} ↕"))
                                .on_click(cx.listener(move |e, _, _, cx| e.sort_merge_table(i, cx)))
                        }))
                        .child(
                            div()
                                .w(px(60.))
                                .flex_none()
                                .text_sm()
                                .child(tr("merge.review.target")),
                        )
                        .child(div().w(px(108.)).flex_none()).into_any_element());
        let table=chrome::data_table::DataTable::new("merge-recipient-table",header,rows,cx)
            .row_height(if compact{112.}else{chrome::data_table::TABLE_ROW_HEIGHT})
            .reorderable(reorder,|e,from,to,w,cx|e.move_merge_recipient(from,to,w,cx));
        let condition = m.filter.as_ref().map(|filter| {
            filter
                .any_of
                .iter()
                .map(|group| {
                    group
                        .iter()
                        .map(|c| {
                            format!(
                                "{} {} {}",
                                m.fields.get(c.field).cloned().unwrap_or_default(),
                                match c.op {
                                    ibis_doc_model::merge::FilterOp::Contains =>
                                        tr("merge.flow.contains").to_string(),
                                    ibis_doc_model::merge::FilterOp::NotContains =>
                                        format!("¬{}", tr("merge.flow.contains")),
                                    ibis_doc_model::merge::FilterOp::Equal => "=".into(),
                                    ibis_doc_model::merge::FilterOp::NotEqual => "≠".into(),
                                    ibis_doc_model::merge::FilterOp::Less => "<".into(),
                                    ibis_doc_model::merge::FilterOp::LessOrEqual => "≤".into(),
                                    ibis_doc_model::merge::FilterOp::Greater => ">".into(),
                                    ibis_doc_model::merge::FilterOp::GreaterOrEqual => "≥".into(),
                                },
                                c.value
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" ∧ ")
                })
                .collect::<Vec<_>>()
                .join(" ∨ ")
        });
        let body =
            v_flex()
                .p_3()
                .gap_3()
                .p_0().child(note("merge.review.recipient-help"))
                .child(text_field(&m.find_field).full_width())
                .child(
                    chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                        .child(
                            chrome::segment_control::SegmentItem::label(
                                "merge-all-view",
                                tr("merge.review.all-view"),
                                !m.targets_only,
                            )
                            .on_click(cx.listener(|e, _, _, cx| {
                                let m = e.doc_state_mut().unwrap().merge.as_mut().unwrap();
                                m.targets_only = false;
                                m.list_page = 0;
                                cx.notify();
                            })),
                        )
                        .child(
                            chrome::segment_control::SegmentItem::label(
                                "merge-targets-view",
                                tr("merge.review.targets-view"),
                                m.targets_only,
                            )
                            .on_click(cx.listener(|e, _, _, cx| {
                                let m = e.doc_state_mut().unwrap().merge.as_mut().unwrap();
                                m.targets_only = true;
                                m.list_page = 0;
                                cx.notify();
                            })),
                        ),
                )
                .when_some(condition, |d, text| {
                    d.child(
                        v_flex().gap_1().p_2().bg(rgb(0xfff4e5)).child(text).child(
                            action_button("merge-clear-filter")
                                .label(tr("merge.review.clear-filter"))
                                .on_click(cx.listener(|e, _, _, cx| e.clear_merge_filter(cx))),
                        ),
                    )
                })
                .child(
                    h_flex()
                        .gap_2()
                        .flex_wrap()
                        .child(
                            action_button("merge-filter")
                                .label(tr("merge.flow.filter"))
                                .on_click(cx.listener(|e, _, _, cx| {
                                    e.open_merge_task(MergeDialog::Filter, cx)
                                })),
                        )
                        .child(
                            action_button("merge-columns")
                                .label(tr("merge.flow.columns"))
                                .on_click(cx.listener(|e, _, _, cx| {
                                    e.open_merge_task(MergeDialog::Columns, cx)
                                })),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().flex_1().min_w_0().text_sm().child(tf(
                            "merge.review.list-counts",
                            &[
                                ("shown", &indices.len().to_string()),
                                ("target", &selected.len().to_string()),
                                ("total", &m.records.len().to_string()),
                            ],
                        )))
                        .child(
                            primary_button("merge-add-record")
                                .label(tr("docx.merge-panel.add-recipient"))
                                .disabled(m.fields.is_empty())
                                .on_click(cx.listener(|e, _, w, cx| e.open_merge_row(None, w, cx))),
                        ),
                )
                .when(compact, |d| {
                    d.child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(header_checkbox())
                            .child(tr("merge.review.select-all-header")),
                    )
                })
                .child(table)
                .when(indices.len() > RECORDS_PER_PAGE, |d| {
                    d.child(
                        h_flex()
                            .gap_2()
                            .child(action_button("merge-page-prev").label("‹").on_click(
                                cx.listener(|e, _, _, cx| {
                                    let m = e.doc_state_mut().unwrap().merge.as_mut().unwrap();
                                    m.list_page = m.list_page.saturating_sub(1);
                                    cx.notify();
                                }),
                            ))
                            .child(format!(
                                "{} / {}",
                                m.list_page + 1,
                                indices.len().div_ceil(RECORDS_PER_PAGE)
                            ))
                            .child(action_button("merge-page-next").label("›").on_click(
                                cx.listener(|e, _, _, cx| {
                                    let m = e.doc_state_mut().unwrap().merge.as_mut().unwrap();
                                    m.list_page = (m.list_page + 1).min(
                                        m.visible_recipient_indices()
                                            .len()
                                            .div_ceil(RECORDS_PER_PAGE)
                                            .saturating_sub(1),
                                    );
                                    cx.notify();
                                }),
                            )),
                    )
                });
        let footer = action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("merge-recipients-ok", tr("merge.flow.confirm-recipients"), crate::shell::chrome::ActionButtonKind::Primary)
                    .on_click(cx.listener(|e, _, _, cx| e.finish_merge_task(cx))),
            )
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(
            tr("merge.review.recipients-title"),
            body.into_any_element(),
            footer,
        ))
    }
    pub(crate) fn apply_merge_kind_review(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self.merge_state() else { return };
        let Some(kind) = m.kind_draft else { return };
        if matches!(kind, "envelopes" | "mailingLabels") && !m.replace_body_ack {
            return;
        }
        let env = m.envelope_preset;
        let label = m.label_preset;
        let sender = m.sender_field.read(cx).value().to_string();
        match kind {
            "envelopes" => {
                self.setup_envelope(env, (!sender.is_empty()).then_some(sender.as_str()), cx)
            }
            "mailingLabels" => self.setup_labels(label, cx),
            _ => self.set_mail_merge_type(Some(kind), cx),
        };
        self.finish_merge_task(cx);
    }
    pub(crate) fn reset_merge_import_headers(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.merge_state() else { return };
        let Some(import) = m.import.as_ref() else {
            return;
        };
        let (fields, _) = import.table();
        let range = [
            import.start_row,
            import.end_row,
            import.start_column,
            import.end_column,
        ];
        let range_inputs = m.import_range.clone();
        for (input, value) in range_inputs.iter().zip(range) {
            input.update(cx, |i, cx| i.set_value(value.to_string(), w, cx));
        }
        let mut inputs = Vec::new();
        let mut subs = Vec::new();
        for (index, name) in fields.iter().enumerate() {
            let input = cx.new(|cx| {
                let mut i = InputState::new(w, cx);
                i.set_value(name.clone(), w, cx);
                i
            });
            subs.push(
                cx.subscribe_in(&input, w, move |e, input, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = input.read(cx).value().trim().to_string();
                        if let Some(m) = e.doc_state_mut().and_then(|s| s.merge.as_mut()) {
                            if let Some(names) = m.import.as_mut().and_then(|i| i.names.as_mut()) {
                                if let Some(name) = names.get_mut(index) {
                                    *name = value;
                                }
                            }
                            m.import_error = None;
                        }
                        cx.notify();
                    }
                }),
            );
            inputs.push(input);
        }
        if let Some(m) = self.doc_state_mut().and_then(|s| s.merge.as_mut()) {
            if let Some(import) = m.import.as_mut() {
                import.names = Some(fields);
            }
            m.import_names = inputs;
            m.import_subscriptions = subs;
        }
        cx.notify();
    }
    pub(crate) fn set_merge_import_sheet(
        &mut self,
        index: usize,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(import) = self
            .doc_state_mut()
            .and_then(|s| s.merge.as_mut())
            .and_then(|m| m.import.as_mut())
        {
            if index >= import.sheets.len() {
                return;
            };
            import.sheet = index;
            let rows = import.raw_rows();
            import.start_row = 1;
            import.end_row = rows.len().max(1);
            import.start_column = 1;
            import.end_column = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
            import.names = None;
        }
        self.reset_merge_import_headers(w, cx);
    }
    pub(crate) fn apply_merge_import_range(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.merge_state() else { return };
        let values = m
            .import_range
            .iter()
            .map(|i| i.read(cx).value().trim().parse::<usize>())
            .collect::<Result<Vec<_>, _>>();
        let valid = values
            .ok()
            .filter(|v| v.len() == 4 && v.iter().all(|&n| n > 0) && v[0] <= v[1] && v[2] <= v[3]);
        let Some(v) = valid else {
            self.doc_state_mut()
                .unwrap()
                .merge
                .as_mut()
                .unwrap()
                .import_error = Some(tr("merge.review.invalid-import-range").to_string());
            cx.notify();
            return;
        };
        let m = self.doc_state_mut().unwrap().merge.as_mut().unwrap();
        let Some(import) = m.import.as_mut() else {
            return;
        };
        let rows = import.raw_rows();
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        if v[1] > rows.len() || v[3] > width {
            m.import_error = Some(tr("merge.review.invalid-import-range").to_string());
            cx.notify();
            return;
        }
        import.start_row = v[0];
        import.end_row = v[1];
        import.start_column = v[2];
        import.end_column = v[3];
        import.names = None;
        m.import_error = None;
        self.reset_merge_import_headers(w, cx);
    }
    pub(crate) fn review_import_dialog(&self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let m = self.merge_state()?;
        let import = m.import.as_ref()?;
        let (fields, records) = import.table();
        let import_expanded=import.advanced;
        let invalid = fields.iter().any(|f| f.is_empty())
            || fields
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != fields.len();
        let body = v_flex()
            .p_3()
            .gap_3()
            .p_0().child(import.name.clone())
            .child(note("merge.flow.import-help"))
            .child(note("merge.flow.sheet"))
            .child({
                let entries=import.sheets.iter().enumerate().map(|(i,(name,_))|chrome::MenuItem::new("merge.import.sheet",name.clone()).on(move|e,w,cx|e.set_merge_import_sheet(i,w,cx)).into()).collect();
                chrome::Dropdown::from_entries("merge-import-sheet",import.sheets.get(import.sheet).map(|s|s.0.clone()).unwrap_or_default(),entries,true,0,&cx.weak_entity()).into_any_element()
            })
            .child(chrome::checkbox_button(
                "merge-header",
                tr("merge.review.header-row"),
                import.header,
                false,
                cx.listener(|e, _, w, cx| {
                    if let Some(import) = e
                        .doc_state_mut()
                        .and_then(|s| s.merge.as_mut())
                        .and_then(|m| m.import.as_mut())
                    {
                        import.header = !import.header;
                        import.names = None;
                    }
                    e.reset_merge_import_headers(w, cx);
                }),
            ))
            .child(
                action_button("merge-import-advanced").w_full()
                    .child(h_flex().w_full().items_center().gap_1()
                        .child(Icon::new(crate::shell::icons::Material("arrow_right")).size(px(18.)).with_animation(("merge-disclosure",import.advanced as usize),Animation::new(std::time::Duration::from_millis(150)),move|icon,t|icon.transform(gpui::Transformation::rotate(gpui::radians(std::f32::consts::FRAC_PI_2*if import_expanded{t}else{1.-t})))))
                        .child(tr("merge.review.import-advanced")))
                    .on_click(cx.listener(|e, _, _, cx| {
                        let i = e
                            .doc_state_mut()
                            .unwrap()
                            .merge
                            .as_mut()
                            .unwrap()
                            .import
                            .as_mut()
                            .unwrap();
                        i.advanced = !i.advanced;
                        cx.notify();
                    })),
            )
            .when(import.advanced, |d| {
                d.child(note("merge.review.import-range-help"))
                    .child(
                        h_flex().gap_2().flex_wrap().children(
                            [
                                "merge.review.start-row",
                                "merge.review.end-row",
                                "merge.review.start-column",
                                "merge.review.end-column",
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(j, key)| {
                                v_flex().gap_1().child(note(key)).child(
                                    text_field(&m.import_range[j]).spinner(
                                        SharedString::from(format!("merge-import-range-{j}")),
                                        chrome::SpinnerSpec::new(1.)
                                            .range(1., 999999.)
                                            .integer_only(),
                                        cx,
                                        |_, _, _, _| {},
                                    ),
                                )
                            }),
                        ),
                    )
                    .child(
                        action_button("merge-import-range-apply").w(px(98.)).self_start()
                            .label(tr("merge.review.apply-range"))
                            .on_click(cx.listener(|e, _, w, cx| e.apply_merge_import_range(w, cx))),
                    )
            })
            .when(import.advanced || invalid, |d| {
                d.child(note("merge.review.rename-headers")).children(
                    m.import_names.iter().enumerate().map(|(i, input)| {
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().w(px(32.)).flex_none().child((i + 1).to_string()))
                            .child(text_field(input).full_width())
                    }),
                )
            })
            .when(invalid, |d| {
                d.child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xc5312b))
                        .child(tr("merge.review.invalid-headers")),
                )
            })
            .when_some(m.import_error.clone(), |d, error| {
                d.child(div().text_sm().text_color(rgb(0xc5312b)).child(error))
            })
            .child(chrome::sheet_preview("merge-import-preview",&fields,&records,&self.resources))
            .child(tf(
                "merge.flow.target-count",
                &[("n", &records.len().to_string())],
            ));
        let footer = action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("merge-import-ok", tr("merge.flow.import-confirm"), crate::shell::chrome::ActionButtonKind::Primary)
                    .disabled(invalid || records.is_empty() || m.import_error.is_some())
                    .on_click(cx.listener(|e, _, w, cx| e.confirm_merge_import(w, cx))),
            )
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(
            tr("merge.flow.import-title"),
            body.into_any_element(),
            footer,
        ))
    }
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_merge_recipient_list(&self, path: &std::path::Path) -> Result<(), String> {
        let m = self
            .merge_state()
            .ok_or_else(|| tr("docx.merge-panel.there-no-recipient-list").to_string())?;
        let text_cell = |v: &String| format!("'{}", v);
        // Reuse the workbook writer's literal-text input path rather than inferring numbers/formulas.
        let fields = m.fields.iter().map(text_cell).collect::<Vec<_>>();
        let records = m
            .records
            .iter()
            .map(|r| r.iter().map(text_cell).collect())
            .collect::<Vec<Vec<String>>>();
        let text = ibis_doc_model::merge::serialize_table(&fields, &records);
        let bytes =
            ibis_pptx::csv::workbook_from_with_delimiter(&text, &tr("merge.flow.recipients"), '\t')
                .map_err(|e| e.to_string())?;
        crate::shell::save::write_replacing(path, &bytes).map_err(|e| e.to_string())
    }

    pub(crate) fn save_merge_recipient_list(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self
                .path
                .as_deref()
                .and_then(|p| p.parent())
                .unwrap_or_else(|| std::path::Path::new("."));
            let picked = cx.prompt_for_new_path(directory, Some("recipients.xlsx"));
            cx.spawn_in(w, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else {
                    return;
                };
                this.update(cx, |e, cx| {
                    e.status = Some(match e.write_merge_recipient_list(&path) {
                        Ok(()) => tr("merge.review.list-saved").into(),
                        Err(err) => err.into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (w, cx);
    }
}
