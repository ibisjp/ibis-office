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
        let color: Hsla = if enabled {
            rgb(TEXT).into()
        } else {
            disabled_color()
        };
        let step =
            |id: &'static str, name: &'static str, tip: &'static str, handler: &Option<Handler>| {
                tool_button(
                    &ToolButton {
                        handler: handler.clone(),
                        ..ToolButton::new(id, name, tip)
                    },
                    index,
                    editor,
                )
            };
        let current_text = value.clone().unwrap_or_default();
        let init_val = current_text.to_string();
        let owner = editor.upgrade().expect("font size owner");
        let input = owner.read(cx).font_size_edit_input.clone();
        if !input.read(cx).focus_handle(cx).is_focused(window)
            && input.read(cx).value().as_str() != current_text.as_ref()
        {
            input.update(cx, |i, cx| i.set_value(current_text.clone(), window, cx));
        }
        let focus_input = input.clone();
        let focus_owner=editor.clone();
        let tab_editor = editor.clone();
        let tab_action_editor = editor.clone();
        let back_tab_editor = editor.clone();
        let size_control = div()
            .relative()
            .id(SharedString::from(format!("font-size-box-{index}")))
            .bg(rgb(0xffffff))
            .w(px(48.))
            .h(px(26.))
            .border_1()
            .border_color(rgb(if input.read(cx).focus_handle(cx).is_focused(window){PRIMARY_BUTTON_BG}else{BORDER}))
            .rounded(px(4.))
            .capture_action(move |_: &gpui_kit::component::input::IndentInline, w, cx| {
                let _ = tab_action_editor.update(cx, |e, cx| e.commit_toolbar_font_size(w, cx));
                cx.stop_propagation();
            })
            .capture_action(
                move |_: &gpui_kit::component::input::OutdentInline, w, cx| {
                    let _ = back_tab_editor.update(cx, |e, cx| e.commit_toolbar_font_size(w, cx));
                    cx.stop_propagation();
                },
            )
            .capture_key_down(move |event: &KeyDownEvent, w, cx| {
                if event.keystroke.key == "tab" {
                    let _ = tab_editor.update(cx, |e, cx| e.commit_toolbar_font_size(w, cx));
                    w.prevent_default();
                    cx.stop_propagation();
                }
            })
            .on_mouse_down(MouseButton::Left,|_,_,cx|cx.stop_propagation())
            .on_click(|_,_,cx|cx.stop_propagation())
            .capture_any_mouse_down(move |_, w, cx| {
                let select_all = !focus_input.read(cx).focus_handle(cx).is_focused(w);
                let field = focus_input.clone();
                let owner=focus_owner.clone();
                w.defer(cx, move |w, cx| {
                    field.update(cx, |i, cx| {
                        i.focus(w, cx);
                        if select_all { i.select_all(w, cx); }
                    });
                    let _=owner.update(cx,|_,cx|cx.notify());
                });
            })
            .child(
                canvas(
                    |_, _, _| (),
                    |bounds, _, _, _| {
                        #[cfg(all(
                            feature = "scenarios",
                            any(target_os = "macos", target_os = "linux")
                        ))]
                        crate::shell::scenario::button_probe::record_bounds(
                            "font-size-edit",
                            bounds,
                        );
                    },
                )
                .absolute()
                .inset_0(),
            )
            .child(
                gpui_kit::component::input::Input::new(&input)
                    .small()
                    .appearance(false)
                    .px_1()
                    .cleanable(false)
                    .disabled(!enabled),
            )
            .into_any_element();
        let editor_click = editor.clone();
        let init_val_btn = init_val.clone();
        let caret = action_button(SharedString::from(format!("font-size-trigger-{index}")))
            .ghost()
            .xsmall()
            .w(px(TOOLBAR_CARET_WIDTH))
            .h(px(28.))
            .px_0()
            .disabled(!enabled)
            .child(icon("arrow_drop_down", 16., color))
            .on_click(move |_, w, cx| {
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
                    set_current_toolbar_open(e.font_size_menu_open, None);
                    cx.notify();
                });
            });
        let popover_anchor: Option<AnyElement> = enabled.then(|| {
            let editor_open = editor.clone();
            let editor_content = editor.clone();
            let is_open = CURRENT_TOOLBAR_OPEN.with(|c| c.get().0);
            Popover::new(SharedString::from(format!("font-size-popover-{index}")))
                .anchor(Anchor::TopLeft)
                .open(is_open)
                .trigger(
                    Button::new(SharedString::from(format!("font-size-anchor-{index}")))
                        .ghost()
                        .xsmall()
                        .p_0()
                        .w(px(0.))
                        .min_w(px(0.))
                        .h(px(28.)),
                )
                .on_open_change(move |open, window, cx| {
                    let init_val = init_val.clone();
                    editor_open
                        .update(cx, |this, cx| {
                            this.font_size_menu_open = *open;
                            this.font_size_filtering = false;
                            this.font_size_selected = None;
                            if *open {
                                this.toolbar_search_open = None;
                                this.font_size_scroll.set_offset(Point::default());
                                let clean = init_val.trim_end_matches('+').to_string();
                                this.font_size_input.update(cx, |input, cx| {
                                    input.set_value(clean, window, cx);
                                    if search {input.focus(window, cx);}
                                });
                            }
                            set_current_toolbar_open(
                                this.font_size_menu_open,
                                this.toolbar_search_open,
                            );
                            cx.notify();
                        })
                        .ok();
                })
                .content(move |_, window, cx| {
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
                                                        set_current_toolbar_open(
                                                            false,
                                                            this.toolbar_search_open,
                                                        );
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
                                                    .child(crate::shell::i18n::text(
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
                                                        d.bg(crate::shell::palette::shade(
                                                            0.0, 0.08,
                                                        ))
                                                        .font_weight(FontWeight::MEDIUM)
                                                    })
                                                    .hover(|s| {
                                                        s.bg(crate::shell::palette::shade(
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
                                                                set_current_toolbar_open(
                                                                    false,
                                                                    this.toolbar_search_open,
                                                                );
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
                })
                .into_any_element()
        });
        h_flex()
            .h(px(28.))
            .flex_none()
            .items_center()
            .children(popover_anchor)
            .child(step(
                "font-smaller",
                "remove",
                "_shell.decrease-font-size",
                smaller,
            ))
            .child(size_control)
            .child(caret)
            .child(step(
                "font-larger",
                "add",
                "_shell.increase-font-size",
                larger,
            ))
            .into_any_element()
    }
}
