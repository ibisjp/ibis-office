// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 関数の挿入 and 関数の引数 (fx, ⇧F3, 数式 > 関数の挿入…): a panel in which a function is looked up by its name
//! (or by a word of what it does, for the ones described), and its arguments are typed into boxes with the result
//! worked out as they are typed; OK puts the formula in the selected cell (one step to undo, or in the cell being
//! typed in). Which arguments a function has is its number (`FUNCTIONS`: the least and the most); what the
//! commonly used ones do is in the tables (`xlsx.fndesc.<name>`), in our own words.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_calc::functions::table::{FUNCTIONS, FunctionInfo};
use ibis_doc_model::{CellEdit, CellInput, Command, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

/// The functions offered before anything is typed in the search box.
const COMMON: &[&str] = &["SUM", "AVERAGE", "IF", "COUNT", "MAX", "MIN", "VLOOKUP", "XLOOKUP", "COUNTIF", "SUMIF", "CONCAT", "TEXT", "ROUND", "TODAY", "IFERROR", "INDEX", "MATCH"];

/// The functions with a line of what they do (`xlsx.fndesc.<name in lower case>`).
pub(crate) const DESCRIBED: &[&str] = &[
    "SUM", "AVERAGE", "COUNT", "COUNTA", "MAX", "MIN", "IF", "IFERROR", "IFS", "AND", "OR", "NOT", "VLOOKUP", "XLOOKUP", "HLOOKUP", "INDEX", "MATCH", "COUNTIF", "SUMIF", "AVERAGEIF", "COUNTIFS", "SUMIFS", "ROUND", "ROUNDUP", "ROUNDDOWN", "INT", "ABS", "MOD", "POWER", "SQRT", "CONCAT", "TEXTJOIN", "LEFT", "RIGHT", "MID", "LEN", "TRIM", "UPPER", "LOWER", "TEXT", "VALUE", "TODAY", "NOW", "DATE", "YEAR", "MONTH", "DAY", "FILTER", "SORT", "UNIQUE",
];

pub(crate) struct FunctionPanel {
    pub(crate) search: Entity<InputState>,
    /// The function chosen, and the boxes of its arguments.
    pub(crate) chosen: Option<&'static FunctionInfo>,
    pub(crate) args: Vec<Entity<InputState>>,
    scroll:ScrollHandle,
    subscriptions: Vec<Subscription>,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

fn described(info: &FunctionInfo) -> bool {
    DESCRIBED.contains(&info.name)
}

fn description_id(info: &FunctionInfo) -> String {
    format!("xlsx.fndesc.{}", info.name.to_lowercase())
}

/// What a function does, if the tables say.
fn description(info: &FunctionInfo) -> Option<String> {
    described(info).then(|| text(&description_id(info)).to_string())
}

/// The functions to list for `query`: the common ones for nothing, else those whose name has the words, or whose
/// description has them.
pub(crate) fn matching(query: &str) -> Vec<&'static FunctionInfo> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return COMMON.iter().filter_map(|n| FUNCTIONS.iter().find(|f| f.name == *n)).collect();
    }
    let mut found: Vec<&'static FunctionInfo> = FUNCTIONS
        .iter()
        .filter(|f| {
            let name = f.name.to_lowercase();
            let about = description(f).map(|d| d.to_lowercase()).unwrap_or_default();
            words.iter().all(|w| name.contains(w.as_str()) || about.contains(w.as_str()))
        })
        .collect();
    // Names that start with the first word come first.
    found.sort_by_key(|f| (!f.name.to_lowercase().starts_with(&words[0]), f.name));
    found.truncate(30);
    found
}

/// The formula a function and its argument texts make: no argument after the last one filled in.
pub(crate) fn formula_of(name: &str, args: &[String]) -> String {
    let used = args.iter().rposition(|a| !a.trim().is_empty()).map_or(0, |i| i + 1);
    format!("={name}({})", args[..used].iter().map(|a| a.trim()).collect::<Vec<_>>().join(","))
}

impl SlideEditor {
    /// 関数の挿入…: the panel, with the search box to type in.
    pub(crate) fn open_function_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(text("xlsx.function.search")));
        search.update(cx, |i, cx| i.focus(window, cx));
        let sub = cx.subscribe_in(&search, window, |e, _, event: &InputEvent, w, cx| {
            match event {
                InputEvent::Change=>{if let Some(p)=e.wb_mut().panels.function.as_mut(){p.scroll.set_offset(Point::default());}cx.notify();}
                InputEvent::PressEnter{..}=>{
                    let first=e.wb().panels.function.as_ref().and_then(|p|matching(&p.search.read(cx).value()).first().map(|f|f.name));
                    if let Some(name)=first{e.function_choose(name,w,cx);e.function_ok(w,cx);}
                },_=>{}
            }
        });
        let panel = FunctionPanel { scroll:ScrollHandle::new(),search, chosen: None, args: Vec::new(), subscriptions: vec![sub], message: String::new(), ui: Default::default() };
        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.close_settings();
        s.panels.function = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// A function chosen in the list: its arguments' boxes (the least the function takes, at least one).
    pub(crate) fn function_choose(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(info) = FUNCTIONS.iter().find(|f| f.name == name) else { return };
        let count = (info.min_args.max(1)) as usize;
        let mut args = Vec::new();
        let mut subscriptions = Vec::new();
        for _ in 0..count {
            let (input, sub) = self.function_argument_box(window, cx);
            args.push(input);
            subscriptions.push(sub);
        }
        if let Some(first) = args.first() {
            first.update(cx, |i, cx| i.focus(window, cx));
        }
        if let Some(p) = self.wb_mut().panels.function.as_mut() {
            p.chosen = Some(info);
            p.args = args;
            p.subscriptions.truncate(1);
            p.subscriptions.extend(subscriptions);
            p.message.clear();
        }
        cx.notify();
    }

    fn function_argument_box(&mut self, window: &mut Window, cx: &mut Context<Self>) -> (Entity<InputState>, Subscription) {
        let input = cx.new(|cx| InputState::new(window, cx));
        let sub = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| match event {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { .. } => this.function_ok(window, cx),
            _ => {}
        });
        (input, sub)
    }

    /// 引数を追加: one more box, while the function takes more.
    pub(crate) fn function_add_argument(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.function.as_ref()) else { return };
        let Some(info) = p.chosen else { return };
        if p.args.len() >= info.max_args as usize {
            return;
        }
        let (input, sub) = self.function_argument_box(window, cx);
        input.update(cx, |i, cx| i.focus(window, cx));
        if let Some(p) = self.wb_mut().panels.function.as_mut() {
            p.args.push(input);
            p.subscriptions.push(sub);
        }
        cx.notify();
    }

    /// The formula typed so far, with the arguments' texts.
    fn function_formula(&self, cx: &App) -> Option<String> {
        let p = self.sheet_state()?.panels.function.as_ref()?;
        let info = p.chosen?;
        let args: Vec<String> = p.args.iter().map(|a| a.read(cx).value().to_string()).collect();
        Some(formula_of(info.name, &args))
    }

    /// The result of the formula as the arguments stand, worked out in the selected cell's place.
    pub(crate) fn function_preview(&self, cx: &App) -> Option<String> {
        let formula = self.function_formula(cx)?;
        let (column, row) = self.current_cell()?;
        let workbook = self.presentation().workbook.as_ref()?;
        let sheet = workbook.sheets.iter().position(|s| s.slide == Some(self.current_slide))?;
        let view = ibis_doc_model::sheet_calc::BookView::new(self.presentation());
        let (value, unsupported) = ibis_calc::recalc::evaluate(&view, (sheet, row, column), formula.trim_start_matches('='));
        if unsupported {
            return None;
        }
        Some(ibis_doc_model::query::value_text(&value))
    }

    /// OK: the formula goes into the selected cell, or into the cell being typed in.
    pub(crate) fn function_ok(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(formula) = self.function_formula(cx) else { return };
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        if let Some(edit) = self.cell_edit_mut() {
            edit.text = formula;
            edit.caret = edit.text.encode_utf16().count();
            edit.anchor = None;
            self.wb_mut().panels.function = None;
            self.focus_handle.focus(window, cx);
            return cx.notify();
        }
        let Some((column, row)) = self.current_cell() else { return };
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let input = match ibis_calc::input::parse_input(&formula, date1904) {
            ibis_calc::input::Typed::Formula(f) => CellInput::Formula(f),
            ibis_calc::input::Typed::Value(v, _) => CellInput::Value(v),
        };
        let slide = self.current_slide;
        let edit = CellEdit { row, column, input };
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![edit] })]) {
            if let Some(p) = self.wb_mut().panels.function.as_mut() {
                p.message = format("edit.refused", &[("error", &err)]);
            }
            return cx.notify();
        }
        self.wb_mut().panels.function = None;
        // The keyboard back to the sheet (⌘Z undoes the cell, not a box of the panel).
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn function_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let preview = self.function_preview(cx);
        let p = self.sheet_state()?.panels.function.as_ref()?;
        let theme = cx.theme().clone();
        let query = p.search.read(cx).value().to_string();
        let chosen = p.chosen;
        let ui = p.ui.clone();
        let mark = move |id: &str, child: AnyElement| {
            let ui = ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, _, _| {
                            ui.borrow_mut().insert(id.clone(), bounds);
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                            ui.borrow_mut().insert(id.clone(), bounds);
                        })
        };
        let mut list=v_flex().w_full();
        let matches=matching(&query);
        if matches.is_empty(){list=list.child(div().p_3().text_color(theme.muted_foreground).child(text("xlsx.function.not-found")));}
        for (index,info) in matches.into_iter().enumerate(){
            let name=info.name;
            let highlighted=chosen.is_some_and(|c|c.name==name)||(index==0&&chosen.is_none());
            list=list.child(h_flex().id(SharedString::from(format!("function-{name}"))).h(px(28.)).px_2().gap_2().items_center().cursor_pointer().text_sm()
                .when(highlighted,|d|d.bg(theme.accent)).hover(|d|d.bg(theme.accent))
                .on_mouse_down(MouseButton::Left,cx.listener(move |e,_,w,cx|e.function_choose(name,w,cx)))
                .child(div().w(px(132.)).flex_none().font_weight(FontWeight::MEDIUM).child(name))
                .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_color(theme.muted_foreground).child(description(info).unwrap_or_default())));
        }
        let list=crate::shell::chrome::table_list("function-list",list.into_any_element(),p.scroll.clone());
        let mut body = v_flex().h_full().min_h_0()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(div().text_sm().child(text("xlsx.function.pick")))
            .child(mark("xlsx.function.search", crate::shell::chrome::text_field(&p.search).full_width().into_any_element()))
            .child(mark("xlsx.function.list", list.into_any_element()));
        if let Some(info) = chosen {
            let count = match (info.min_args, info.max_args) {
                (a, b) if a == b => format("_xlsx.n-arguments", &[("n", &a)]),
                (a, 255) => format("_xlsx.min-or-more-arguments", &[("min", &a)]),
                (a, b) => format("_xlsx.min-to-max-arguments", &[("min", &a), ("max", &b)]),
            };
            body = body
                .child(div().border_t_1().border_color(theme.border).pt_2().text_sm().font_weight(FontWeight::SEMIBOLD).child(format!("{}  ({count})", info.name)))
                .children(description(info).map(|d| div().text_sm().text_color(theme.muted_foreground).child(SharedString::from(d))));
            for (i, arg) in p.args.iter().enumerate() {
                let optional = i >= info.min_args as usize;
                let mut label = format("xlsx.function.argument", &[("n", &(i + 1))]);
                if optional {
                    label.push_str(&text("xlsx.function.optional"));
                }
                body = body
                    .child(div().text_sm().child(SharedString::from(label)))
                    .child(mark(&format!("xlsx.function.arg-{}", i + 1), crate::shell::chrome::text_field(arg).full_width().into_any_element()));
            }
            if p.args.len() < info.max_args as usize {
                body = body.child(mark(
                    "xlsx.function.add",
                    crate::shell::chrome::action_button("function-add").label(text("xlsx.function.add-argument")).on_click(cx.listener(|this, _, window, cx| this.function_add_argument(window, cx))).into_any_element(),
                ));
            }
            body = body.child(div().text_sm().child(format("xlsx.function.result", &[("value", &preview.clone().unwrap_or_default())])));
            body = body.child(div().text_sm().text_color(gpui::red()).child(SharedString::from(p.message.clone())));
        }
        let panel_actions = crate::shell::chrome::action_bar()
                .when(chosen.is_some(), |r| {
                    r.action(mark_action("xlsx.function.ok", crate::shell::chrome::ActionDefinition::new("function-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label(text("xlsx.function.ok")).on_click(cx.listener(|this, _, window, cx| this.function_ok(window, cx)))))
                })
                .action(mark_action("xlsx.function.close", crate::shell::chrome::ActionDefinition::new("function-close", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .label(text("_shell.close"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.wb_mut().panels.function = None;
                            cx.notify();
                        }))));
        Some(SidebarPanel::with_footer(text("xlsx.function.title").to_string(), body.into_any_element(), panel_actions))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn the_formula_leaves_out_the_arguments_after_the_last_one_filled_in() {
        assert_eq!(formula_of("SUM", &["A1:A3".into(), "".into()]), "=SUM(A1:A3)");
        assert_eq!(formula_of("NOW", &["".into()]), "=NOW()");
    }
}
