// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Direct borders of the caret's table cell, in one undoable table replacement.
use gpui_kit::*;
use ibis_doc_model::{Block,FlowCell,FlowBorder,BorderStyle,FlowCommand,Rgb};
use ibis_doc_model::flow_command;
use crate::shell::{chrome::{MenuEntry,MenuItem,Submenu},editor::SlideEditor};
const STYLE_IDS:&[&str]=&[
    "docx.cell.border.single",
    "docx.cell.border.thick",
    "docx.cell.border.double",
    "docx.cell.border.triple",
    "docx.cell.border.dotted",
    "docx.cell.border.dashed",
    "docx.cell.border.dotDash",
    "docx.cell.border.dotDotDash",
    "docx.cell.border.dashSmallGap",
    "docx.cell.border.dashDotStroked",
    "docx.cell.border.thinThickSmallGap",
    "docx.cell.border.thickThinSmallGap",
    "docx.cell.border.thinThickThinSmallGap",
    "docx.cell.border.thinThickMediumGap",
    "docx.cell.border.thickThinMediumGap",
    "docx.cell.border.thinThickThinMediumGap",
    "docx.cell.border.thinThickLargeGap",
    "docx.cell.border.thickThinLargeGap",
    "docx.cell.border.thinThickThinLargeGap",
    "docx.cell.border.wave",
    "docx.cell.border.doubleWave",
    "docx.cell.border.threeDEmboss",
    "docx.cell.border.threeDEngrave",
    "docx.cell.border.outset",
    "docx.cell.border.inset",
];
impl SlideEditor {
    fn border_cell(&self)->Option<&FlowCell> {
        let (table,row,cell)=self.table_at_caret()?;
        let d=self.flow()?;let (container,index)=flow_command::locate(d,table)?;
        let Block::Table(t)=flow_command::blocks(d,container)?.get(index)? else{return None};
        t.rows.get(row)?.cells.get(cell)
    }
    fn change_cell_border(&mut self,change:impl FnOnce(&mut FlowCell),cx:&mut Context<Self>) {
        let Some((id,row,cell))=self.table_at_caret() else{return};
        let Some(d)=self.flow() else{return};let Some((container,index))=flow_command::locate(d,id) else{return};
        let Some(Block::Table(t))=flow_command::blocks(d,container).and_then(|b|b.get(index)) else{return};
        let mut table=t.clone();let Some(c)=table.rows.get_mut(row).and_then(|r|r.cells.get_mut(cell)) else{return};
        change(c);table.resolve_borders();
        if &table==t {return;}
        self.run_flow(vec![FlowCommand::ReplaceBlocks {container,index,remove:1,blocks:vec![Block::Table(table)]}],cx);
    }
    pub(crate) fn choose_cell_border_edge(&mut self,edge:Option<usize>,cx:&mut Context<Self>) {
        if edge.is_some_and(|i|i>=4) {return;}
        if let Some(state)=self.doc_state_mut() {state.cell_border_edge=edge;}cx.notify();
    }
    pub(crate) fn set_cell_border_style(&mut self,style:Option<BorderStyle>,cx:&mut Context<Self>) {
        let edge=self.doc_state().and_then(|s|s.cell_border_edge);
        self.change_cell_border(|cell|{
            for i in 0..4 {
                if edge.is_some_and(|edge|edge!=i) {continue;}
                cell.own_borders[i]=Some(style.map(|style|FlowBorder {style,..cell.borders[i].unwrap_or(default_border())}));
            }
        },cx);
    }
    pub(crate) fn set_cell_border_width(&mut self,width:i64,cx:&mut Context<Self>) {
        let edge=self.doc_state().and_then(|s|s.cell_border_edge);
        self.change_cell_border(|cell|{
            for i in 0..4 {
                if edge.is_some_and(|edge|edge!=i) {continue;}cell.own_borders[i]=Some(Some(FlowBorder {width,..cell.borders[i].unwrap_or(default_border())}));}
        },cx);
    }
    pub(crate) fn set_cell_border_color(&mut self,color:Rgb,cx:&mut Context<Self>) {
        let edge=self.doc_state().and_then(|s|s.cell_border_edge);
        self.change_cell_border(|cell|{
            for i in 0..4 {
                if edge.is_some_and(|edge|edge!=i) {continue;}cell.own_borders[i]=Some(Some(FlowBorder {color,..cell.borders[i].unwrap_or(default_border())}));}
        },cx);
    }
    pub(crate) fn toggle_cell_diagonal(&mut self,index:usize,cx:&mut Context<Self>) {
        if index>=2 {return;}
        self.change_cell_border(|cell|{
            cell.diagonals[index]=if cell.diagonals[index].is_some() {None}else{Some(cell.borders.iter().flatten().copied().next().unwrap_or(default_border()))};
        },cx);
    }
}
fn default_border()->FlowBorder {FlowBorder {width:9525,color:Rgb(0),style:BorderStyle::Single,space:0,art:None}}
pub(super) fn menu(editor:&SlideEditor)->Vec<MenuEntry> {
    let cell=editor.border_cell();let enabled=cell.is_some();let scope=editor.doc_state().and_then(|s|s.cell_border_edge);let edges:Vec<_>=cell.map(|c|match scope {Some(i)=>vec![c.borders[i]],None=>c.borders.to_vec()}).unwrap_or_default();
    let scopes:Vec<MenuEntry>=[("docx.cell.border-edge.all","docx.cell-borders.all-borders",None),("docx.cell.border-edge.top","_shell.top",Some(0)),("docx.cell.border-edge.right","_shell.right",Some(1)),("docx.cell.border-edge.bottom","_shell.bottom",Some(2)),("docx.cell.border-edge.left","_shell.left",Some(3))].into_iter()
        .map(|(id,label,value)|MenuItem::new(id,crate::shell::i18n::text(label)).disabled(!enabled).checked(scope==value).on(move|this,_,cx|this.choose_cell_border_edge(value,cx)).into()).collect();
    let mut styles:Vec<MenuEntry>=super::paragraph_decorations::BORDERS.iter().zip(STYLE_IDS)
        .map(|(&(_,style),&id)|MenuItem::new(id,crate::shell::i18n::text(super::paragraph_decorations::border_label(style))).disabled(!enabled).checked(!edges.is_empty() && edges.iter().all(|b|b.is_some_and(|b|b.style==style)))
        .on(move|this,_,cx|this.set_cell_border_style(Some(style),cx)).into()).collect();
    styles.push(MenuItem::tr("docx.cell.border.none").disabled(!enabled).checked(!edges.is_empty() && edges.iter().all(Option::is_none)).on(|this,_,cx|this.set_cell_border_style(None,cx)).into());
    let widths:Vec<MenuEntry>=[("docx.cell.border-width.half","0.5 pt",6350),("docx.cell.border-width.one","1 pt",12700),("docx.cell.border-width.one-half","1.5 pt",19050),("docx.cell.border-width.three","3 pt",38100)].into_iter()
        .map(|(id,label,w)|MenuItem::new(id,crate::shell::i18n::text(label)).disabled(!enabled).checked(!edges.is_empty() && edges.iter().all(|b|b.is_some_and(|b|b.width==w))).on(move|this,_,cx|this.set_cell_border_width(w,cx)).into()).collect();
    let colors: Vec<MenuEntry> = {
        use crate::shell::color_palette::{Palette, PickColor, Swatch, theme_rows};
        let pick: PickColor = std::rc::Rc::new(|this, c, _, cx| {
            if let Some(rgb) = c {
                this.set_cell_border_color(rgb, cx);
            }
        });
        let cur = (!edges.is_empty()).then(|| {
            let first = edges[0]?.color;
            edges.iter().all(|b| b.is_some_and(|b| b.color == first)).then_some(first)
        }).flatten();
        let mut theme = theme_rows(&super::toolbar::THEME_10, &pick, cur);
        if !enabled {
            for s in theme.iter_mut().flatten() {
                s.item = s.item.clone().disabled(true);
            }
        }
        let standard: Vec<Swatch> = [
            ("docx.cell.border-color.black", "_shell.black", 0),
            ("docx.cell.border-color.red", "_shell.red", 0xff0000),
            ("docx.cell.border-color.blue", "_shell.blue", 0x0000ff),
            ("docx.cell.border-color.green", "_shell.green", 0x008000),
        ]
        .into_iter()
        .map(|(id, label, c)| Swatch {
            rgb: Rgb(c),
            item: MenuItem::new(id, crate::shell::i18n::text(label))
                .disabled(!enabled)
                .checked(!edges.is_empty() && edges.iter().all(|b| b.is_some_and(|b| b.color == Rgb(c))))
                .on(move |this, _, cx| this.set_cell_border_color(Rgb(c), cx)),
        })
        .collect();
        vec![MenuEntry::Palette(Palette {
            none: None,
            theme,
            standard,
            pick: Some(pick),
            more: None,
            button: None, current:None,
        })]
    };
    vec![Submenu::tr("docx.cell.border-edge", "border_style",scopes).into(),
        Submenu::tr("docx.cell.border.types", "border_style",styles).into(),
        Submenu::tr("docx.cell.border.width", "line_weight",widths).into(),
        Submenu::tr("docx.cell.border.color", "format_color_text",colors).into(),
        MenuItem::tr("docx.cell.diagonal.down").disabled(!enabled).checked(cell.is_some_and(|c|c.diagonals[0].is_some())).on(|this,_,cx|this.toggle_cell_diagonal(0,cx)).into(),
        MenuItem::tr("docx.cell.diagonal.up").disabled(!enabled).checked(cell.is_some_and(|c|c.diagonals[1].is_some())).on(|this,_,cx|this.toggle_cell_diagonal(1,cx)).into()]
}
