// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Value sorting with ordered column keys on a captured worksheet range.
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, SheetCommand, SlideId};

/// What a level sorts by (Excel's 並べ替えのキー): the cells' values, or
/// a fill, text colour, or conditional formatting icon put on top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LevelKey {
    Value,
    CellColor(Option<ibis_doc_model::Rgb>),
    FontColor(Option<ibis_doc_model::Rgb>),
    Icon(String, u32),
}

/// Key of the 「リアルタイム更新」 setting in the document's `document_memory` (per open document, not saved in the file).
pub(crate) const SORT_REALTIME_KEY: &str = "xlsx.sort.realtime";

/// Re-sorts when a text box of the panel is edited (typing, or a range picked on the sheet).
fn watch_input(input: &Entity<InputState>, window: &mut Window, cx: &mut Context<SlideEditor>) {
    cx.subscribe_in(input, window, |editor, _, event: &InputEvent, _, cx| {
        if matches!(event, InputEvent::Change) {
            editor.sort_changed(cx);
        }
    })
    .detach();
}

pub(crate) struct SortLevelEdit{pub(crate) index:usize,new:bool,original:(String,bool,LevelKey,String)}
pub(crate) struct SortPanel {
    pub(crate) editor:Option<SortLevelEdit>,
    pub(crate) reorder:std::rc::Rc<std::cell::RefCell<crate::shell::chrome::data_table::RowReorder>>,
    pub(crate) range: Entity<InputState>,
    pub(crate) levels: Vec<(Entity<InputState>, bool)>,
    /// Each level's key, beside `levels`.
    pub(crate) keys: Vec<LevelKey>,
    /// Each level's custom list input, beside `levels`.
    pub(crate) custom_lists: Vec<Entity<InputState>>,
    /// Which level has the custom list settings open.
    pub(crate) custom_open: Option<usize>,
    /// The level whose column list (by the headings) is open.
    pub(crate) picking: Option<usize>,
    /// The level whose key list is open.
    pub(crate) keying: Option<usize>,
    pub(crate) ui:
        std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
    slide: SlideId,
    pub(crate) header: bool,
    pub(crate) match_case: bool,
    pub(crate) error: String,
    /// The undo length right after the last real-time sort. While the undo stack is still that long, nothing else
    /// was done since, so the next real-time sort replaces that step instead of stacking another.
    live: Option<usize>,
}
fn range(text: &str) -> Option<CellRange> {
    let mut ends = text.trim().split(':');
    let a = ibis_calc::formula::parser::parse_cell(ends.next()?)?;
    let b = match ends.next() {
        Some(s) => ibis_calc::formula::parser::parse_cell(s)?,
        None => a.clone(),
    };
    if ends.next().is_some() || a.row > b.row || a.column > b.column {
        return None;
    }
    Some(CellRange {
        first_row: a.row,
        last_row: b.row,
        first_column: a.column,
        last_column: b.column,
    })
}
fn column_input(
    column: u32,
    window: &mut Window,
    cx: &mut Context<SlideEditor>,
) -> Entity<InputState> {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.column-a-b")));
    let name = ibis_doc_model::cell_name(column, 0);
    input.update(cx, |i, cx| {
        i.set_value(name.strip_suffix('1').unwrap(), window, cx)
    });
    watch_input(&input, window, cx);
    input
}
fn custom_list_input(
    window: &mut Window,
    cx: &mut Context<SlideEditor>,
) -> Entity<InputState> {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.e-g-sun-mon-tue-wed-thu-fri-sat")));
    watch_input(&input, window, cx);
    input
}
impl SlideEditor {
 /// Excel's "Copy Level": the level is duplicated right below itself.
 pub(crate) fn copy_sort_level(&mut self,i:usize,w:&mut Window,cx:&mut Context<Self>){
  let Some(p)=self.wb().sort_edit.as_ref()else{return;};
  let Some((column,ascending))=p.levels.get(i)else{return;};
  let (column,ascending,key)=(column.read(cx).value().to_string(),*ascending,p.keys.get(i).cloned().unwrap_or(LevelKey::Value));
  let list=p.custom_lists.get(i).map(|c|c.read(cx).value().to_string()).unwrap_or_default();
  // The copy sorts by the next column to the right (the same column twice is refused as a duplicate); the last column keeps its own.
  let next=self.next_sort_column_text(&column,&p.range.read(cx).value(),p.header);
  let column=next.unwrap_or(column);
  let new_column=cx.new(|cx|InputState::new(w,cx).placeholder(crate::shell::i18n::text("_xlsx.column-a-b")));
  new_column.update(cx,|input,cx|input.set_value(column,w,cx));
  watch_input(&new_column,w,cx);
  let new_list=custom_list_input(w,cx);
  if !list.is_empty(){new_list.update(cx,|input,cx|input.set_value(list,w,cx));}
  if let Some(p)=self.wb_mut().sort_edit.as_mut(){
   let at=(i+1).min(p.levels.len());
   p.levels.insert(at,(new_column,ascending));p.keys.insert(at,key);p.custom_lists.insert(at,new_list);
   if let Some(open)=p.custom_open{if open>i{p.custom_open=Some(open+1);}}
  }
  self.sort_changed(cx);
 }
 /// The column right of the one a level names (a letter or a heading), in the same form; None at the range's last column.
 fn next_sort_column_text(&self,level:&str,range:&str,header:bool)->Option<String>{
  let r=self.resolve_sort_range(range)?;
  let raw=level.trim();let up=raw.to_uppercase();
  let by_letter=if !up.is_empty()&&up.bytes().all(|b|b.is_ascii_alphabetic()){
   ibis_calc::formula::parser::parse_cell(&format!("{up}1")).map(|a|a.column).filter(|c|(r.first_column..=r.last_column).contains(c))
  }else{None};
  let (_,sheet)=self.current_sheet()?;
  let (current,named)=match by_letter{
   Some(c)=>(c,false),
   None=>{
    if !header||raw.is_empty(){return None;}
    let c=(r.first_column..=r.last_column).find(|&c|sheet.cell(r.first_row,c).is_some_and(|cell|cell.text.trim().eq_ignore_ascii_case(raw)))?;
    (c,true)
   }
  };
  let next=current+1;
  if next>r.last_column{return None;}
  if named{
   if let Some(text)=sheet.cell(r.first_row,next).map(|cell|cell.text.trim().to_string()).filter(|t|!t.is_empty()){return Some(text);}
  }
  Some(super::dedupe_edit::column_name(next))
 }
 pub(crate) fn open_sort_level(&mut self,index:Option<usize>,w:&mut Window,cx:&mut Context<Self>){
  self.cancel_table_reorder(w,cx);
  let new=index.is_none();
  if new {let column=self.wb().sort_edit.as_ref().and_then(|p|self.resolve_sort_range(&p.range.read(cx).value()).map(|r|(r.first_column+p.levels.len()as u32).min(r.last_column))).unwrap_or(0);let Some(p)=self.wb_mut().sort_edit.as_mut()else{return;};let input=column_input(column,w,cx);let list=custom_list_input(w,cx);p.levels.push((input,true));p.keys.push(LevelKey::Value);p.custom_lists.push(list);}
  if new{self.sort_changed(cx);}
  let Some(p)=self.wb_mut().sort_edit.as_mut()else{return;};let index=index.unwrap_or(p.levels.len()-1);let Some((field,asc))=p.levels.get(index)else{return;};
  p.editor=Some(SortLevelEdit{index,new,original:(field.read(cx).value().to_string(),*asc,p.keys[index].clone(),p.custom_lists[index].read(cx).value().to_string())});p.picking=None;p.keying=None;p.custom_open=None;cx.notify();
 }
 pub(crate) fn finish_sort_level(&mut self,cancel:bool,w:&mut Window,cx:&mut Context<Self>){
  let Some(p)=self.wb_mut().sort_edit.as_mut()else{return;};let Some(edit)=p.editor.take()else{return;};
  if cancel {if edit.new {p.levels.remove(edit.index);p.keys.remove(edit.index);p.custom_lists.remove(edit.index);}else{p.levels[edit.index].0.update(cx,|i,cx|i.set_value(edit.original.0,w,cx));p.levels[edit.index].1=edit.original.1;p.keys[edit.index]=edit.original.2;p.custom_lists[edit.index].update(cx,|i,cx|i.set_value(edit.original.3,w,cx));}}
  p.picking=None;p.keying=None;p.custom_open=None;
  if cancel{self.sort_changed(cx);}else{cx.notify();}
 }
}
impl SlideEditor {
    pub(crate) fn sort_by_selected_cell_color(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Sort, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((range, column, header)) = self.sort_selection() else { return };
        let color = self.current_style().fill;
        self.run_sheet(
            vec![Command::Sheet(SheetCommand::SortBy {
                slide: self.current_slide,
                range,
                keys: vec![(
                    column,
                    ibis_doc_model::sheet_command::SortBy::CellColor { color, top: true },
                )],
                header,
                match_case: false,
            })],
            cx,
        );
    }

    pub(crate) fn sort_by_selected_font_color(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Sort, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((range, column, header)) = self.sort_selection() else { return };
        let style = self.current_style();
        let color = (style.color.0 != 0).then_some(style.color);
        self.run_sheet(
            vec![Command::Sheet(SheetCommand::SortBy {
                slide: self.current_slide,
                range,
                keys: vec![(
                    column,
                    ibis_doc_model::sheet_command::SortBy::FontColor { color, top: true },
                )],
                header,
                match_case: false,
            })],
            cx,
        );
    }

    pub(crate) fn sort_by_selected_icon(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Sort, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((column, row)) = self.current_cell() else { return };
        let Some((range, _, header)) = self.sort_selection() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some((icon_set, icon_id)) = ibis_doc_model::sheet_command::cell_icon(sheet, row, column) else {
            return;
        };
        self.run_sheet(
            vec![Command::Sheet(SheetCommand::SortBy {
                slide: self.current_slide,
                range,
                keys: vec![(
                    column,
                    ibis_doc_model::sheet_command::SortBy::Icon {
                        icon_set,
                        icon_id,
                        top: true,
                    },
                )],
                header,
                match_case: false,
            })],
            cx,
        );
    }

    pub(crate) fn open_sort_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((r, column, header)) = self.sort_selection() else {
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("A1:C10"));
        input.update(cx, |i, cx| {
            i.set_value(
                format!(
                    "{}:{}",
                    ibis_doc_model::cell_name(r.first_column, r.first_row),
                    ibis_doc_model::cell_name(r.last_column, r.last_row)
                ),
                window,
                cx,
            );
            i.focus(window, cx);
        });
        watch_input(&input, window, cx);
        let custom_inp = custom_list_input(window, cx);
        let panel = SortPanel {editor:None,reorder:Default::default(),
            range: input,
            levels: vec![(column_input(column, window, cx), true)],
            keys: vec![LevelKey::Value],
            custom_lists: vec![custom_inp],
            custom_open: None,
            picking: None,
            keying: None,
            ui: Default::default(),
            slide: self.current_slide,
            header,
            match_case: false,
            error: String::new(),
            live: None,
        };
        let state = self.wb_mut();
        state.custom_format = None;
        state.page_setup = false;
        state.names = None;
        state.validation = None;
        state.find = None;
        state.link = None;
        state.conditional_edit = None;
        state.close_settings();
        state.sort_edit = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }
    pub(crate) fn resolve_sort_range(&self, raw: &str) -> Option<CellRange> {
        self.resolve_advanced_filter_range(raw).or_else(|| range(raw))
    }

    pub(crate) fn set_sort_range_from_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((r, col, _)) = self.sort_selection().or_else(|| {
            self.selected_range().map(|r| (r, r.first_column, true))
        }) else {
            return;
        };
        let range_str = format!(
            "{}:{}",
            ibis_doc_model::cell_name(r.first_column, r.first_row),
            ibis_doc_model::cell_name(r.last_column, r.last_row)
        );
        let Some(p) = self.wb().sort_edit.as_ref() else { return };
        let range_inp = p.range.clone();
        let first_col_inp = p.levels.first().map(|(inp, _)| inp.clone());
        range_inp.update(cx, |i, cx| i.set_value(range_str, window, cx));
        if let Some(col_inp) = first_col_inp {
            let cur_text = col_inp.read(cx).value().trim().to_uppercase();
            let cur_col = ibis_calc::formula::parser::parse_cell(&format!("{cur_text}1")).map(|a| a.column);
            if !cur_col.is_some_and(|c| (r.first_column..=r.last_column).contains(&c)) {
                let target_col = col.clamp(r.first_column, r.last_column);
                let col_letter = ibis_doc_model::cell_name(target_col, 0).trim_end_matches('1').to_string();
                col_inp.update(cx, |i, cx| i.set_value(col_letter, window, cx));
            }
        }
        if let Some(p) = self.wb_mut().sort_edit.as_mut() {
            p.error.clear();
        }
        cx.notify();
    }

    fn sort_error(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().sort_edit.as_mut() {
            p.error = text.into();
        }
        cx.notify();
    }
    /// The panel's 並べ替え button: sorts, then gives the keyboard back to the sheet. The panel stays open, and its text
    /// boxes take ⌘Z for their own undo; left focused, they would swallow the ⌘Z meant to undo the sort.
    pub(crate) fn apply_sort_button(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_sort_panel(cx);
        self.focus_handle.focus(window, cx);
    }

    /// Whether 「リアルタイム更新」 is on (the default). Kept per open document in `document_memory`.
    pub(crate) fn sort_realtime(&self) -> bool {
        self.document_memory.get::<bool>(SORT_REALTIME_KEY).unwrap_or(true)
    }

    /// Turns 「リアルタイム更新」 on or off. Turning it on sorts at once, since the 並べ替える button goes away with it.
    pub(crate) fn set_sort_realtime(&mut self, on: bool, cx: &mut Context<Self>) {
        self.document_memory.set(SORT_REALTIME_KEY, on);
        if on {
            self.apply_sort_panel_as(true, cx);
        } else {
            cx.notify();
        }
    }

    /// A setting of the panel changed: with 「リアルタイム更新」 on, the sort follows it.
    pub(crate) fn sort_changed(&mut self, cx: &mut Context<Self>) {
        if self.sort_realtime() {
            self.apply_sort_panel_as(true, cx);
        } else {
            cx.notify();
        }
    }

    pub(crate) fn apply_sort_panel(&mut self, cx: &mut Context<Self>) {
        self.apply_sort_panel_as(false, cx);
    }

    /// Sorts by the panel's settings. `live` (real-time update) replaces the previous real-time sort in the Undo
    /// stack, as long as nothing else was done since, so a run of changes is one Undo step.
    fn apply_sort_panel_as(&mut self, live: bool, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.sort_edit.as_ref()) else {
            return;
        };
        if p.slide != self.current_slide {
            self.sort_error(&crate::shell::i18n::text("_xlsx.the-sheet-has-changed-reopen-it"), cx);
            return;
        }
        let Some(r) = self.resolve_sort_range(&p.range.read(cx).value()) else {
            self.sort_error(&crate::shell::i18n::text("_xlsx.enter-a-valid-cell-range"), cx);
            return;
        };
        let Some((_, sheet)) = self.current_sheet() else {
            return;
        };
        if r.last_row as usize >= sheet.rows.len()
            || r.last_column as usize >= sheet.columns.len()
            || (p.header && r.first_row == r.last_row)
        {
            self.sort_error(&crate::shell::i18n::text("_xlsx.check-the-range-and-headers"), cx);
            return;
        }
        let mut keys = Vec::new();
        for (level, (input, ascending)) in p.levels.iter().enumerate() {
            let key = p.keys.get(level).cloned().unwrap_or(LevelKey::Value);
            let raw_text = input.read(cx).value().trim().to_string();
            let text = raw_text.to_uppercase();
            let by_letter = if !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphabetic()) {
                ibis_calc::formula::parser::parse_cell(&format!("{text}1"))
                    .map(|a| a.column)
                    .filter(|c| (r.first_column..=r.last_column).contains(c))
            } else {
                None
            };
            let column = by_letter.or_else(|| {
                if p.header && !raw_text.is_empty() {
                    (r.first_column..=r.last_column).find(|&c| {
                        sheet
                            .cell(r.first_row, c)
                            .is_some_and(|cell| cell.text.trim().eq_ignore_ascii_case(&raw_text) || cell.text.trim() == raw_text)
                    })
                } else {
                    None
                }
            });
            let Some(column) = column.filter(|c| {
                (r.first_column..=r.last_column).contains(c)
                    && !keys.iter().enumerate().any(|(j, (old, _))| old == c && p.keys.get(j).cloned().unwrap_or(LevelKey::Value) == key)
            }) else {
                self.sort_error(
                    &crate::shell::i18n::text("_xlsx.specify-column-names-within-the-range-each-colum"),
                    cx,
                );
                return;
            };
            keys.push((column, *ascending));
        }
        let by: Vec<(u32, ibis_doc_model::sheet_command::SortBy)> = keys
            .iter()
            .enumerate()
            .map(|(level, &(column, ascending))| {
                use ibis_doc_model::sheet_command::SortBy;
                let key = p.keys.get(level).cloned().unwrap_or(LevelKey::Value);
                let custom_text = p.custom_lists.get(level).map(|c| c.read(cx).value().trim().to_string()).unwrap_or_default();
                (column, match key {
                    LevelKey::Value => {
                        let items: Vec<String> = custom_text
                            .split(|c| c == ',' || c == '、' || c == '\n')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        if !items.is_empty() {
                            SortBy::CustomList { list: items, ascending }
                        } else {
                            SortBy::Value(ascending)
                        }
                    }
                    LevelKey::CellColor(color) => SortBy::CellColor { color, top: ascending },
                    LevelKey::FontColor(color) => SortBy::FontColor { color, top: ascending },
                    LevelKey::Icon(icon_set, icon_id) => SortBy::Icon { icon_set, icon_id, top: ascending },
                })
            })
            .collect();
        let (slide, header, match_case, last_live) = (p.slide, p.header, p.match_case, p.live);
        if live && last_live == Some(self.document.undo_len()) {
            self.document.retract_last();
        }
        if let Err(error) = self
            .document
            .execute(vec![Command::Sheet(SheetCommand::SortBy {
                slide,
                range: r,
                keys: by,
                header,
                match_case,
            })])
        {
            self.sort_error(&error.to_string(), cx);
            return;
        }
        let undo_len = self.document.undo_len();
        self.wb_mut().last_fill = None;
        if let Some(p) = self.wb_mut().sort_edit.as_mut() {
            p.error.clear();
            p.live = live.then_some(undo_len);
        }
        cx.notify();
    }
    /// The range's columns (letter, heading or 列 X), the fills and text
    /// colours in its rows (when more than one), and the conditional
    /// formatting icons, for the level lists.
    fn sort_choices(&self, p: &SortPanel, cx: &App) -> (Vec<(String, String)>, Vec<Option<ibis_doc_model::Rgb>>, Vec<Option<ibis_doc_model::Rgb>>, Vec<(String, u32)>) {
        let (Some(r), Some((_, sheet))) = (self.resolve_sort_range(&p.range.read(cx).value()), self.current_sheet()) else { return Default::default() };
        let letter = |c: u32| ibis_doc_model::cell_name(c, 0).trim_end_matches('1').to_string();
        let columns = (r.first_column..=r.last_column.min(r.first_column + 200))
            .map(|c| {
                let heading = sheet.cell(r.first_row, c).map(|x| x.text.clone()).filter(|t| p.header && !t.is_empty());
                (letter(c), heading.unwrap_or_else(|| crate::shell::i18n::format("_xlsx.column-name", &[("name", &letter(c))])))
            })
            .collect();
        let (mut fills, mut fonts, mut icons) = (Vec::new(), Vec::new(), Vec::new());
        let first = r.first_row + u32::from(p.header);
        for c in sheet.cells.iter().filter(|c| (first..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column)) {
            let style = sheet.styles.get(c.style as usize);
            let fill = style.and_then(|s| s.fill);
            if !fills.contains(&fill) {
                fills.push(fill);
            }
            let font = style.map(|s| s.color).filter(|c| c.0 != 0);
            if !fonts.contains(&font) {
                fonts.push(font);
            }
            if let Some(ic) = ibis_doc_model::sheet_command::cell_icon(sheet, c.row, c.column)
                && !icons.contains(&ic)
            {
                icons.push(ic);
            }
        }
        if fills.len() < 2 {
            fills.clear();
        }
        if fonts.len() < 2 {
            fonts.clear();
        }
        (columns, fills, fonts, icons)
    }

    /// The right pane's sort panel; while a level is being edited, that level's form (shown as a dialog over the pane).
    pub(crate) fn sort_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.sort_panel_view(true, cx)
    }

    /// The pane's main sort panel, kept in view behind the level editor.
    pub(crate) fn sort_background_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.sort_panel_view(false, cx)
    }

    fn sort_panel_view(&mut self, level_form: bool, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let realtime = self.sort_realtime();
        let p = self.sheet_state()?.sort_edit.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
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
        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(crate::shell::i18n::text("_xlsx.range-to-sort")),
            )
            .child(mark(
                "range",
                super::range_field::local_range_field(&p.range,"sort_edit-p-range",false,cx).into_any_element(),
            ))
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(mark(
                        "header",
                        crate::shell::chrome::checkbox_button("sort-header",crate::shell::i18n::text("xlsx.sort.has-header"),p.header,false,cx.listener(|this,_,_,cx|{
                                if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                    p.header = !p.header;
                                }
                                this.sort_changed(cx);
                            }))
                            .into_any_element(),
                    ))
                    .child(mark(
                        "match-case",
                        crate::shell::chrome::checkbox_button("sort-match-case",crate::shell::i18n::text("_shell.match-case"),p.match_case,false,cx.listener(|this,_,_,cx|{
                                if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                    p.match_case = !p.match_case;
                                }
                                this.sort_changed(cx);
                            }))
                            .into_any_element(),
                    ))
                    .child(mark(
                        "realtime",
                        crate::shell::chrome::checkbox_button("sort-realtime",crate::shell::i18n::text("xlsx.sort-realtime"),realtime,false,cx.listener(|this,_,_,cx|{
                                let on = !this.sort_realtime();
                                this.set_sort_realtime(on, cx);
                            }))
                            .into_any_element(),
                    )),
            );
        // The headings (or column letters) of the range, and the colours and icons in it.
        let choices = self.sort_choices(p, cx);
        let editing=p.editor.as_ref().map(|e|e.index).filter(|_|level_form);
        if editing.is_some(){body=crate::shell::chrome::panel_form().p_0();}
        for (i, (column, ascending)) in p.levels.iter().enumerate() {
            if editing!=Some(i){continue;}
            let key = p.keys.get(i).cloned().unwrap_or(LevelKey::Value);
            let typed = column.read(cx).value().trim().to_uppercase();
            let heading = choices.0.iter().find(|(letter, _)| *letter == typed).map_or_else(|| crate::shell::i18n::text("_xlsx.select-column").to_string(), |(_, h)| h.clone());
            let key_label = match &key {
                LevelKey::Value => crate::shell::i18n::text("_xlsx.cell-values").to_string(),
                LevelKey::CellColor(c) => crate::shell::i18n::format("_xlsx.cell-color-color", &[("color", &colour_name(*c, true))]),
                LevelKey::FontColor(c) => crate::shell::i18n::format("_xlsx.font-color-color", &[("color", &colour_name(*c, false))]),
                LevelKey::Icon(set, id) => crate::shell::i18n::format("xlsx.sort-icon-key", &[("set", set), ("id", &id.to_string())]),
            };
            let custom_text = p.custom_lists.get(i).map(|c| c.read(cx).value().trim().to_string()).unwrap_or_default();
            let has_custom = key == LevelKey::Value && !custom_text.is_empty();
            let order_label = match (&key, *ascending, has_custom) {
                (LevelKey::Value, true, false) => crate::shell::i18n::text("_xlsx.ascending"),
                (LevelKey::Value, false, false) => crate::shell::i18n::text("_xlsx.descending"),
                (LevelKey::Value, true, true) => crate::shell::i18n::text("_xlsx.list-order"),
                (LevelKey::Value, false, true) => crate::shell::i18n::text("_xlsx.reverse-list-order"),
                (_, true, _) => crate::shell::i18n::text("_shell.top"),
                (_, false, _) => crate::shell::i18n::text("_shell.bottom"),
            };
            let row = h_flex()
                .gap_1()
                .child(mark(
                    &format!("column-{i}"),
                    crate::shell::chrome::text_field(column).width(px(56.)).into_any_element(),
                ))
                .child(mark(
                    &format!("pick-{i}"),
                    crate::shell::chrome::action_button(("sort-pick", i))
                        .small()
                        .label(format!("{heading} ▾"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                p.picking = if p.picking == Some(i) { None } else { Some(i) };
                                p.keying = None;
                                p.custom_open = None;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    &format!("key-{i}"),
                    crate::shell::chrome::action_button(("sort-key", i))
                        .small()
                        .label(format!("{key_label} ▾"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                p.keying = if p.keying == Some(i) { None } else { Some(i) };
                                p.picking = None;
                                p.custom_open = None;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    &format!("order-{i}"),
                    crate::shell::chrome::action_button(("sort-order", i))
                        .small()
                        .label(order_label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                if let Some((_, ascending)) = p.levels.get_mut(i) {
                                    *ascending = !*ascending;
                                }
                            }
                            this.sort_changed(cx);
                        }))
                        .into_any_element(),
                ));
            let custom_button=mark(&format!("custom-btn-{i}"),crate::shell::chrome::action_button(("sort-custom",i)).small().label(crate::shell::i18n::text("_xlsx.custom-list-comma-separated")).on_click(cx.listener(move|e,_,_,cx|{if let Some(p)=e.wb_mut().sort_edit.as_mut(){p.custom_open=if p.custom_open==Some(i){None}else{Some(i)};p.picking=None;p.keying=None;}cx.notify();})).into_any_element());
            body = body.child(crate::shell::i18n::format("_xlsx.level-n", &[("n", &(i + 1))])).child(row).child(custom_button);
            if p.custom_open == Some(i) {
                if let Some(custom_input) = p.custom_lists.get(i) {
                    let mut custom_box = v_flex()
                        .gap_1()
                        .pl_2()
                        .pr_2()
                        .py_1()
                        .child(
                            div()
                                .text_sm()
                                .child(crate::shell::i18n::text("_xlsx.custom-list-comma-separated")),
                        )
                        .child(mark(
                            &format!("custom-input-{i}"),
                            crate::shell::chrome::text_field(custom_input).full_width().into_any_element(),
                        ));

                    let presets: [(&str, &str); 7] = [
                        ("_xlsx.sun-sat", "日, 月, 火, 水, 木, 金, 土"), // i18n-ok（行 411 のプリセットの値（カスタムリストの中身。照合に使う））
                        ("_xlsx.sunday-saturday", "日曜日, 月曜日, 火曜日, 水曜日, 木曜日, 金曜日, 土曜日"), // i18n-ok（行 412 のプリセットの値（カスタムリストの中身））
                        ("_xlsx.jan-dec", "1月, 2月, 3月, 4月, 5月, 6月, 7月, 8月, 9月, 10月, 11月, 12月"), // i18n-ok（行 413 のプリセットの値（カスタムリストの中身））
                        ("_xlsx.q1-q4", "第1四半期, 第2四半期, 第3四半期, 第4四半期"), // i18n-ok（行 414 のプリセットの値（カスタムリストの中身））
                        ("_xlsx.sun-sat-2", "Sun, Mon, Tue, Wed, Thu, Fri, Sat"),
                        ("_xlsx.sunday-saturday-2", "Sunday, Monday, Tuesday, Wednesday, Thursday, Friday, Saturday"),
                        ("_xlsx.jan-dec-2", "Jan, Feb, Mar, Apr, May, Jun, Jul, Aug, Sep, Oct, Nov, Dec"),
                    ];
                    let mut preset_row1 = h_flex().gap_1();
                    for (p_idx, (label, val)) in presets[..4].iter().enumerate() {
                        let val = (*val).to_string();
                        preset_row1 = preset_row1.child(mark(
                            &format!("preset-{i}-{p_idx}"),
                            crate::shell::chrome::action_button(SharedString::from(format!("sort-preset-{i}-{p_idx}")))
                                .small()
                                .label(crate::shell::i18n::text(label))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                        if let Some(inp) = p.custom_lists.get(i) {
                                            let val = val.clone();
                                            inp.update(cx, |s, cx| s.set_value(val, window, cx));
                                        }
                                    }
                                    this.sort_changed(cx);
                                }))
                                .into_any_element(),
                        ));
                    }
                    let mut preset_row2 = h_flex().gap_1();
                    for (p_idx, (label, val)) in presets[4..].iter().enumerate() {
                        let p_idx = p_idx + 4;
                        let val = (*val).to_string();
                        preset_row2 = preset_row2.child(mark(
                            &format!("preset-{i}-{p_idx}"),
                            crate::shell::chrome::action_button(SharedString::from(format!("sort-preset-{i}-{p_idx}")))
                                .small()
                                .label(crate::shell::i18n::text(label))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                        if let Some(inp) = p.custom_lists.get(i) {
                                            let val = val.clone();
                                            inp.update(cx, |s, cx| s.set_value(val, window, cx));
                                        }
                                    }
                                    this.sort_changed(cx);
                                }))
                                .into_any_element(),
                        ));
                    }
                    let clear_btn = h_flex().gap_1().child(mark(
                        &format!("custom-clear-{i}"),
                        crate::shell::chrome::destructive_button(SharedString::from(format!("sort-custom-clear-{i}")),cx)
                            .small()
                            .label(crate::shell::i18n::text("_xlsx.clear-list-normal-order"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                    if let Some(inp) = p.custom_lists.get(i) {
                                        inp.update(cx, |s, cx| s.set_value(String::new(), window, cx));
                                    }
                                    p.custom_open = None;
                                }
                                this.sort_changed(cx);
                            }))
                            .into_any_element(),
                    ));
                    custom_box = custom_box.child(preset_row1).child(preset_row2).child(clear_btn);
                    body = body.child(custom_box);
                }
            }
            if p.picking == Some(i) {
                let mut list = v_flex().pl_2();
                for (letter, heading) in &choices.0 {
                    let letter = letter.clone();
                    list = list.child(mark(
                        &format!("pick-{i}-{letter}"),
                        crate::shell::chrome::action_button(SharedString::from(format!("sort-pick-{i}-{letter}")))
                            .small()
                            .label(heading.clone())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let Some(p) = this.wb_mut().sort_edit.as_mut() else { return };
                                p.picking = None;
                                if let Some((input, _)) = p.levels.get(i) {
                                    let input = input.clone();
                                    let letter = letter.clone();
                                    input.update(cx, |s, cx| s.set_value(letter, window, cx));
                                }
                                this.sort_changed(cx);
                            }))
                            .into_any_element(),
                    ));
                }
                body = body.child(list);
            }
            if p.keying == Some(i) {
                let mut list = v_flex().pl_2();
                let mut options = vec![LevelKey::Value];
                options.extend(choices.1.iter().map(|&c| LevelKey::CellColor(c)));
                options.extend(choices.2.iter().map(|&c| LevelKey::FontColor(c)));
                options.extend(choices.3.iter().map(|(set, id)| LevelKey::Icon(set.clone(), *id)));
                for (k, option) in options.into_iter().enumerate() {
                    let text = match &option {
                        LevelKey::Value => crate::shell::i18n::text("_xlsx.cell-values").to_string(),
                        LevelKey::CellColor(c) => crate::shell::i18n::format("_xlsx.cell-color-color", &[("color", &colour_name(*c, true))]),
                        LevelKey::FontColor(c) => crate::shell::i18n::format("_xlsx.font-color-color", &[("color", &colour_name(*c, false))]),
                        LevelKey::Icon(set, id) => crate::shell::i18n::format("xlsx.sort-icon-key", &[("set", set), ("id", &id.to_string())]),
                    };
                    let option_clone = option.clone();
                    list = list.child(mark(
                        &format!("key-{i}-{k}"),
                        crate::shell::chrome::action_button(SharedString::from(format!("sort-key-{i}-{k}")))
                            .small()
                            .label(text)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(p) = this.wb_mut().sort_edit.as_mut() {
                                    if let Some(key) = p.keys.get_mut(i) {
                                        *key = option_clone.clone();
                                    }
                                    p.keying = None;
                                    if option_clone != LevelKey::Value {
                                        p.custom_open = None;
                                    }
                                }
                                this.sort_changed(cx);
                            }))
                            .into_any_element(),
                    ));
                }
                body = body.child(list);
            }

        }
        if editing.is_none(){
            use crate::shell::chrome::{self,data_table::{DataTable,row_controls}};
            let reorder=p.reorder.clone();
            let header=h_flex().px_2().h_full().items_center().gap_2().children(["_xlsx.column","_xlsx.cell-values","xlsx.order"].into_iter().map(|key|div().flex_1().min_w_0().child(crate::shell::i18n::text(key))));
            let mut rows=Vec::new();for(i,(column,ascending))in p.levels.iter().enumerate(){
                let name=column.read(cx).value().to_string();let kind=match p.keys.get(i){Some(LevelKey::CellColor(_))=>"xlsx.sort.key.cell-color",Some(LevelKey::FontColor(_))=>"xlsx.sort.key.font-color",Some(LevelKey::Icon(..))=>"xlsx.sort.key.icon",_=>"_xlsx.cell-values"};
                let actions=h_flex().gap_1().child(row_controls(reorder.clone(),i,0,chrome::action_button(("sort-edit",i)).w(px(28.)).h(px(28.)).px_0().icon(crate::shell::icons::Material("edit")).on_click(cx.listener(move|e,_,w,cx|e.open_sort_level(Some(i),w,cx)))))
                    .child(row_controls(reorder.clone(),i,2,chrome::action_button(("sort-copy",i)).w(px(28.)).h(px(28.)).px_0().icon(crate::shell::icons::Material("content_copy")).tooltip(crate::shell::i18n::text("xlsx.sort-copy-level")).on_click(cx.listener(move|e,_,w,cx|e.copy_sort_level(i,w,cx)))))
                    .child(row_controls(reorder.clone(),i,1,chrome::action_button(("sort-delete",i)).w(px(28.)).h(px(28.)).px_0().icon(crate::shell::icons::Material("delete")).disabled(p.levels.len()<=1).on_click(cx.listener(move|e,_,_,cx|{if let Some(p)=e.wb_mut().sort_edit.as_mut(){if p.levels.len()>1{p.levels.remove(i);p.keys.remove(i);p.custom_lists.remove(i);}}e.sort_changed(cx);}))));
                let row=h_flex().px_2().h_full().items_center().gap_2().child(div().flex_1().min_w_0().child(name)).child(div().flex_1().min_w_0().truncate().child(crate::shell::i18n::text(kind)))
                    .child(div().flex_1().min_w_0().child(crate::shell::i18n::text(if *ascending{"_xlsx.ascending"}else{"_xlsx.descending"}))).child(actions).child(crate::shell::chrome::icon("drag_handle",18.,crate::shell::palette::rgb(0x444746).into()));rows.push((i,row.into_any_element(),false));
            }
            let table=DataTable::new("sort-levels-table",Some(header.into_any_element()),rows,cx).reorderable(reorder,|e,from,to,_,cx|{if let Some(p)=e.wb_mut().sort_edit.as_mut(){let level=p.levels.remove(from);p.levels.insert(to,level);let key=p.keys.remove(from);p.keys.insert(to,key);let custom=p.custom_lists.remove(from);p.custom_lists.insert(to,custom);}e.sort_changed(cx);});
            body=body.child(h_flex().w_full().justify_end().child(mark("add",chrome::action_button("sort-add").label(crate::shell::i18n::text("_xlsx.add-column")).on_click(cx.listener(|e,_,w,cx|e.open_sort_level(None,w,cx))).into_any_element()))).child(table);
        }
        body=body.children(crate::shell::chrome::panel_message(&p.error));
        if editing.is_some(){let footer=crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("sort-level-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label("OK").on_click(cx.listener(|e,_,w,cx|e.finish_sort_level(false,w,cx))))
            .action(crate::shell::chrome::ActionDefinition::new("sort-level-cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(crate::shell::i18n::text("_shell.cancel")).on_click(cx.listener(|e,_,w,cx|e.finish_sort_level(true,w,cx))));
            return Some(SidebarPanel::with_footer(crate::shell::i18n::text("xlsx.sort-level.edit"),body.into_any_element(),footer));}
        // Real-time update on: the sort follows every change, so the pane has no end-area button. Off: only the
        // 並べ替える button, which applies the settings (the pane stays open).
        if realtime {
            return Some(SidebarPanel { title: crate::shell::i18n::text("_xlsx.sort-2").into(), body: body.into_any_element() });
        }
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("sort-apply", "", crate::shell::chrome::ActionButtonKind::Primary)
                    .label(crate::shell::i18n::text("_xlsx.sort"))
                    .on_click(cx.listener(|this, _, window, cx| this.apply_sort_button(window, cx)))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("_xlsx.sort-2"), body.into_any_element(), panel_terminal_actions))
    }
}

/// A colour in the lists: its code, or none / automatic.
fn colour_name(c: Option<ibis_doc_model::Rgb>, cell: bool) -> String {
    match c {
        Some(c) => format!("#{:06X}", c.0 & 0xFF_FFFF),
        None if cell => crate::shell::i18n::text("_xlsx.no-fill").into(),
        None => crate::shell::i18n::text("_xlsx.automatic").into(),
    }
}
