// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! One editable size control for all three document formats.
use super::*;
#[derive(IntoElement)]
pub(crate) struct FontSizeControl {
    pub value: Option<SharedString>,
    pub search: bool,
    pub smaller: Option<Handler>,
    pub larger: Option<Handler>,
    pub index: usize,
    pub editor: WeakEntity<SlideEditor>,
}
impl RenderOnce for FontSizeControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let search=self.search;
        let value = &self.value;
        let smaller = &self.smaller;
        let larger = &self.larger;
        let index = self.index;
        let editor = &self.editor;
        let enabled = value.is_some();
        let current_text = value.clone().unwrap_or_default();
        let init_val = current_text.to_string();
        let owner = editor.upgrade().expect("font size owner");
        let input = owner.read(cx).font_size_edit_input.clone();
        if !input.read(cx).focus_handle(cx).is_focused(window)
            && input.read(cx).value().as_str() != current_text.as_ref()
        {
            input.update(cx, |i, cx| i.set_value(current_text.clone(), window, cx));
        }
        let editor_click = editor.clone();
        let init_val_btn = init_val.clone();
        let popup: Option<StepperPopup> = enabled.then(|| {
            let editor_open = editor.clone();
            let editor_content = editor.clone();
            let is_open = owner.read(cx).font_size_menu_open;
            let toggle: Rc<dyn Fn(&mut Window, &mut App)> = Rc::new(move |w, cx| {
                let _ = editor_click.update(cx, |e, cx| {
                    e.font_size_menu_open = !e.font_size_menu_open;
                    e.toolbar_search_open = None;
                    e.font_size_filtering = false;
                    e.font_size_selected = None;
                    e.font_size_scroll.set_offset(Point::default());
                    if e.font_size_menu_open {
                        e.font_size_input.update(cx, |i, cx| {
                            i.set_value(init_val_btn.trim_end_matches('+').to_string(), w, cx);
                            if search {i.focus(w, cx);}
                        });
                    }
                    cx.notify();
                });
            });
            let open_changed: Rc<dyn Fn(bool, &mut Window, &mut App)> = Rc::new(move |open: bool, window: &mut Window, cx: &mut App| {
                    let init_val = init_val.clone();
                    editor_open
                        .update(cx, |this, cx| {
                            this.font_size_menu_open = open;
                            this.font_size_filtering = false;
                            this.font_size_selected = None;
                            if open {
                                this.toolbar_search_open = None;
                                this.font_size_scroll.set_offset(Point::default());
                                let clean = init_val.trim_end_matches('+').to_string();
                                this.font_size_input.update(cx, |input, cx| {
                                    input.set_value(clean, window, cx);
                                    if search {input.focus(window, cx);}
                                });
                            }
                            cx.notify();
                        })
                        .ok();
            });
            let content: Rc<dyn Fn(&mut Window, &mut App) -> AnyElement> = Rc::new(move |window: &mut Window, cx: &mut App| {
                    let Some(ed) = editor_content.upgrade() else {
                        return div().into_any_element();
                    };
                    let (input, scroll_handle, selected_idx, filtering) = {
                        let r = ed.read(cx);
                        (
                            r.font_size_input.clone(),
                            r.font_size_scroll.clone(),
                            r.font_size_selected,
                            r.font_size_filtering,
                        )
                    };
                    let current_str = ed
                        .read(cx)
                        .toolbar_items()
                        .into_iter()
                        .find_map(|it| match it {
                            ToolItem::FontSize { value: Some(v), .. } => Some(v.to_string()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    let query = if search {input.read(cx).value().to_string()} else {String::new()};
                    let candidates = filtered_font_size_candidates(&query, filtering && search);
                    let total_candidates = candidates.len();
                    let row_h = 24.0;
                    let full_h = (FONT_SIZE_CANDIDATES.len() as f32) * row_h;
                    let avail_h = (f32::from(window.viewport_size().height) - 160.0).max(144.0);
                    let list_h = full_h.min((avail_h / row_h).floor().max(4.0) * row_h);
                    let visible_rows = (list_h / row_h).round().max(1.0) as usize;
                    let current_clean = current_str.trim_end_matches('+');
                    let current_match_idx = candidates
                        .iter()
                        .position(|&pt| font_size_candidate_label(pt) == current_clean)
                        .or_else(|| (filtering && !candidates.is_empty()).then_some(0));
                    let editor_for_list = editor_content.clone();
                    let editor_for_keys = editor_content.clone();
                    let scroll_for_keys = scroll_handle.clone();
                    v_flex()
                        .w(px(108.))
                        .p(px(4.))
                        .gap(px(4.))
                        .on_mouse_down_stop()
                        .child(
                            canvas(
                                |_, _, _| {},
                                move |_, _, window, _| {
                                    let ed_key = editor_for_keys.clone();
                                    let scroll_key = scroll_for_keys.clone();
                                    window.on_key_event(
                                        move |event: &KeyDownEvent, phase, window, cx| {
                                            if phase != DispatchPhase::Capture {
                                                return;
                                            }
                                            let key = event.keystroke.key.as_str();
                                            if key == "escape" {
                                                ed_key
                                                    .update(cx, |this, cx| {
                                                        this.font_size_menu_open = false;
                                                        this.font_size_filtering = false;
                                                        this.font_size_selected = None;
                                                        this.focus_handle.focus(window, cx);
                                                        cx.notify();
                                                    })
                                                    .ok();
                                                window.refresh();
                                                cx.stop_propagation();
                                                return;
                                            }
                                            if total_candidates == 0 {
                                                return;
                                            }
                                            let page_step = visible_rows.saturating_sub(1).max(1);
                                            let next_idx = match key {
                                                "down" => {
                                                    Some(match selected_idx.or(current_match_idx) {
                                                        Some(i) => {
                                                            (i + 1).min(total_candidates - 1)
                                                        }
                                                        None => 0,
                                                    })
                                                }
                                                "pagedown" => {
                                                    Some(match selected_idx.or(current_match_idx) {
                                                        Some(i) => (i + page_step)
                                                            .min(total_candidates - 1),
                                                        None => page_step.min(total_candidates - 1),
                                                    })
                                                }
                                                "up" => selected_idx
                                                    .or(current_match_idx)
                                                    .map(|i| i.saturating_sub(1)),
                                                "pageup" => selected_idx
                                                    .or(current_match_idx)
                                                    .map(|i| i.saturating_sub(page_step)),
                                                _ => None,
                                            };
                                            if let Some(idx) = next_idx {
                                                scroll_row_into_view(
                                                    &scroll_key,
                                                    idx,
                                                    row_h,
                                                    list_h,
                                                );
                                                ed_key
                                                    .update(cx, |this, cx| {
                                                        this.font_size_selected = Some(idx);
                                                        cx.notify();
                                                    })
                                                    .ok();
                                                window.refresh();
                                                cx.stop_propagation();
                                            }
                                        },
                                    );
                                },
                            )
                            .absolute()
                            .size_0(),
                        )
                        .when(search,|d|d.child(text_field(&input).full_width()))
                        .child(
                            div()
                                .relative()
                                .w_full()
                                .h(px(list_h))
                                .child(
                                    dropdown_list::list("font-size-candidates", &scroll_handle)
                                        .children(if candidates.is_empty() {
                                            vec![
                                                div()
                                                    .w_full()
                                                    .h(px(24.))
                                                    .px(px(6.))
                                                    .flex()
                                                    .items_center()
                                                    .text_size(px(12.))
                                                    .text_color(rgb(TEXT_SECONDARY))
                                                    .child(crate::i18n::text(
                                                        "fix.menu.not-found",
                                                    ))
                                                    .into_any_element(),
                                            ]
                                        } else {
                                            candidates
                                                .into_iter()
                                                .enumerate()
                                                .map(|(idx, pt)| {
                                                    let label = font_size_candidate_label(pt);
                                                    let is_highlighted = match selected_idx {
                                                        Some(sel) => sel == idx,
                                                        None => current_match_idx == Some(idx),
                                                    };
                                                    let ed_click = editor_for_list.clone();
                                                    dropdown_list::option(
                                                        ("font-size-candidate", idx),
                                                        false,
                                                        is_highlighted,
                                                        true,
                                                    )
                                                    .h(px(24.))
                                                    .px(px(8.))
                                                    .justify_between()
                                                    .text_size(px(13.))
                                                    .text_color(rgb(TEXT))
                                                    .when(is_highlighted, |d| {
                                                        d.bg(crate::palette::shade(
                                                            0.0, 0.08,
                                                        ))
                                                        .font_weight(FontWeight::MEDIUM)
                                                    })
                                                    .hover(|s| {
                                                        s.bg(crate::palette::shade(
                                                            0.0, 0.08,
                                                        ))
                                                    })
                                                    .child(label)
                                                    .on_mouse_down_stop()
                                                    .on_click(move |_, window, cx| {
                                                        ed_click
                                                            .update(cx, |this, cx| {
                                                                this.font_size_menu_open = false;
                                                                this.font_size_filtering = false;
                                                                this.font_size_selected = None;
                                                                this.set_toolbar_font_size(
                                                                    pt, window, cx,
                                                                );
                                                                cx.notify();
                                                            })
                                                            .ok();
                                                    })
                                                    .into_any_element()
                                                })
                                                .collect()
                                        }),
                                )
                                .child(always_visible_scrollbar_overlay(
                                    "font-size-scrollbar",
                                    scroll_handle,
                                    Some((list_h, (total_candidates as f32) * row_h)),
                                )),
                        )
                        .into_any_element()
            });
            StepperPopup { open: is_open, toggle, open_changed, content }
        });
        let (minus, plus) = (smaller.clone(), larger.clone());
        let mut stepper = bound_number_stepper("font-size", &input, editor).disabled(!enabled)
            .value_width(px(48.))
            .tooltips(crate::i18n::text("_shell.decrease-font-size"), crate::i18n::text("_shell.increase-font-size"))
            .on_step(minus, plus)
            .on_commit(Rc::new(move |e, w, cx| e.commit_toolbar_font_size(w, cx)));
        if let Some(popup) = popup { stepper = stepper.popup(popup); }
        let _ = index;
        stepper.into_any_element()
    }
}
