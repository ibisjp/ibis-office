// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Paragraph shading and scoped borders, using existing paragraph replacement undo.
use gpui_kit::*;
use ibis_doc_model::{FlowCommand, ParaFormat, FlowBorder, BorderStyle, Rgb};
use ibis_doc_model::flow_command;
use crate::shell::editor::SlideEditor;
use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};

pub(super) const BORDERS: &[(&str, BorderStyle)] = &[
    ("single", BorderStyle::Single), ("thick", BorderStyle::Thick), ("double", BorderStyle::Double),
    ("triple", BorderStyle::Triple), ("dotted", BorderStyle::Dotted), ("dashed", BorderStyle::Dashed),
    ("dotDash", BorderStyle::DotDash), ("dotDotDash", BorderStyle::DotDotDash),
    ("dashSmallGap", BorderStyle::DashSmallGap), ("dashDotStroked", BorderStyle::DashDotStroked),
    ("thinThickSmallGap", BorderStyle::ThinThickSmallGap), ("thickThinSmallGap", BorderStyle::ThickThinSmallGap),
    ("thinThickThinSmallGap", BorderStyle::ThinThickThinSmallGap), ("thinThickMediumGap", BorderStyle::ThinThickMediumGap),
    ("thickThinMediumGap", BorderStyle::ThickThinMediumGap), ("thinThickThinMediumGap", BorderStyle::ThinThickThinMediumGap),
    ("thinThickLargeGap", BorderStyle::ThinThickLargeGap), ("thickThinLargeGap", BorderStyle::ThickThinLargeGap),
    ("thinThickThinLargeGap", BorderStyle::ThinThickThinLargeGap), ("wave", BorderStyle::Wave),
    ("doubleWave", BorderStyle::DoubleWave), ("threeDEmboss", BorderStyle::ThreeDEmboss),
    ("threeDEngrave", BorderStyle::ThreeDEngrave), ("outset", BorderStyle::Outset), ("inset", BorderStyle::Inset),
];

const BORDER_IDS: &[&str] = &[
    "docx.paragraph.border.single",
    "docx.paragraph.border.thick",
    "docx.paragraph.border.double",
    "docx.paragraph.border.triple",
    "docx.paragraph.border.dotted",
    "docx.paragraph.border.dashed",
    "docx.paragraph.border.dotDash",
    "docx.paragraph.border.dotDotDash",
    "docx.paragraph.border.dashSmallGap",
    "docx.paragraph.border.dashDotStroked",
    "docx.paragraph.border.thinThickSmallGap",
    "docx.paragraph.border.thickThinSmallGap",
    "docx.paragraph.border.thinThickThinSmallGap",
    "docx.paragraph.border.thinThickMediumGap",
    "docx.paragraph.border.thickThinMediumGap",
    "docx.paragraph.border.thinThickThinMediumGap",
    "docx.paragraph.border.thinThickLargeGap",
    "docx.paragraph.border.thickThinLargeGap",
    "docx.paragraph.border.thinThickThinLargeGap",
    "docx.paragraph.border.wave",
    "docx.paragraph.border.doubleWave",
    "docx.paragraph.border.threeDEmboss",
    "docx.paragraph.border.threeDEngrave",
    "docx.paragraph.border.outset",
    "docx.paragraph.border.inset",
];

use super::block_tree::{each_paragraph,paragraphs};

impl SlideEditor {
    fn apply_paragraph_decoration(&mut self, update: impl Fn(&mut ParaFormat, &ibis_doc_model::FlowParagraph), cx: &mut Context<Self>) {
        let Some((a,b)) = self.selected_paragraphs() else { return };
        let Some(doc) = self.flow() else { return };
        let (Some((container,first)),Some((other,last))) = (flow_command::locate(doc,a),flow_command::locate(doc,b)) else { return };
        if container != other || first > last { return; }
        let Some(list) = flow_command::blocks(doc,container) else { return };
        let mut blocks = list[first..=last].to_vec();
        each_paragraph(&mut blocks,&mut |p| {
            let mut direct=p.direct.clone();update(&mut direct,p);p.direct=direct;p.resolve(&doc.styles);
        });
        if blocks == list[first..=last] {return;}
        self.run_flow(vec![FlowCommand::ReplaceBlocks {container,index:first,remove:last-first+1,blocks}],cx);
    }
    pub(crate) fn set_paragraph_shading(&mut self, color: Option<Rgb>, cx: &mut Context<Self>) {
        self.apply_paragraph_decoration(|d,_| d.shading=Some(color),cx);
    }
    pub(crate) fn choose_paragraph_border_edge(&mut self,edge:Option<usize>,cx:&mut Context<Self>) {
        if edge.is_some_and(|i|i>=4) {return;}
        if let Some(state)=self.doc_state_mut() {state.paragraph_border_edge=edge;}cx.notify();
    }
    pub(crate) fn set_paragraph_border_style(&mut self, style: Option<BorderStyle>, cx: &mut Context<Self>) {
        let edge=self.doc_state().and_then(|s|s.paragraph_border_edge);
        self.apply_paragraph_decoration(|d,p| {
            for i in 0..4 {
                if edge.is_some_and(|edge|edge!=i) {continue;}
                let border=style.map(|style| FlowBorder {style,..p.borders[i].unwrap_or(FlowBorder {width:9525,color:Rgb(0),style:BorderStyle::Single,space:0,art:None})});
                d.borders[i]=Some(border);
            }
        },cx);
    }
    pub(crate) fn set_paragraph_border_width(&mut self, width:i64, cx:&mut Context<Self>) {
        let edge=self.doc_state().and_then(|s|s.paragraph_border_edge);
        self.apply_paragraph_decoration(|d,p| {
            for i in 0..4 {
                if edge.is_some_and(|edge|edge!=i) {continue;}
                let b=p.borders[i].unwrap_or(FlowBorder {width,color:Rgb(0),style:BorderStyle::Single,space:0,art:None});
                d.borders[i]=Some(Some(FlowBorder {width,..b}));
            }
        },cx);
    }
    pub(crate) fn set_paragraph_border_color(&mut self,color:Rgb,cx:&mut Context<Self>) {
        let edge=self.doc_state().and_then(|s|s.paragraph_border_edge);
        self.apply_paragraph_decoration(|d,p| {
            for i in 0..4 {
                if edge.is_some_and(|edge|edge!=i) {continue;}
                let b=p.borders[i].unwrap_or(FlowBorder {width:9525,color,style:BorderStyle::Single,space:0,art:None});
                d.borders[i]=Some(Some(FlowBorder {color,..b}));
            }
        },cx);
    }
}

pub(super) fn border_label(style:BorderStyle)->&'static str {
    use BorderStyle::*;
    match style {
        Single=>"docx.paragraph-decorations.single",Thick=>"_docx.thick-line",Double=>"_docx.double-line",Triple=>"docx.chrome.triple",Dotted=>"_docx.dotted-line",Dashed=>"_docx.dashed-line",
        DotDash=>"_docx.dash-dot-line",DotDotDash=>"_docx.dash-dot-dot-line",DashSmallGap=>"docx.paragraph-decorations.fine-dashed",DashDotStroked=>"docx.paragraph-decorations.dash-dot-stroked",
        ThinThickSmallGap=>"docx.paragraph-decorations.thin-thick-narrow-gap",ThickThinSmallGap=>"docx.paragraph-decorations.thick-thin-narrow-gap",ThinThickThinSmallGap=>"docx.paragraph-decorations.thin-thick-thin-narrow-gap",
        ThinThickMediumGap=>"docx.paragraph-decorations.thin-thick-medium-gap",ThickThinMediumGap=>"docx.paragraph-decorations.thick-thin-medium-gap",ThinThickThinMediumGap=>"docx.paragraph-decorations.thin-thick-thin-medium-gap",
        ThinThickLargeGap=>"docx.paragraph-decorations.thin-thick-wide-gap",ThickThinLargeGap=>"docx.paragraph-decorations.thick-thin-wide-gap",ThinThickThinLargeGap=>"docx.paragraph-decorations.thin-thick-thin-wide-gap",
        Wave=>"_docx.wavy-line",DoubleWave=>"docx.paragraph-decorations.double-wave",ThreeDEmboss=>"docx.paragraph-decorations.emboss",ThreeDEngrave=>"docx.paragraph-decorations.engrave",Outset=>"docx.paragraph-decorations.outset",Inset=>"docx.paragraph-decorations.inset",
    }
}

pub(super) fn menu(editor:&SlideEditor)->Vec<MenuEntry> {
    let paragraphs=editor.selected_paragraphs().and_then(|(a,b)| {
        let doc=editor.flow()?;let (container,first)=flow_command::locate(doc,a)?;let (other,last)=flow_command::locate(doc,b)?;
        if container!=other || first>last {return None;}
        let list=flow_command::blocks(doc,container)?;
        Some(paragraphs(&list[first..=last]))
    }).unwrap_or_default();
    let enabled=!paragraphs.is_empty();
    let edge=editor.doc_state().and_then(|s|s.paragraph_border_edge);
    let borders:Vec<_>=paragraphs.iter().flat_map(|p| match edge {Some(i)=>vec![p.borders[i]],None=>p.borders.to_vec()}).collect();
    let scopes:Vec<MenuEntry>=[("docx.paragraph.border-edge.all","docx.cell-borders.all-borders",None),("docx.paragraph.border-edge.top","_shell.top",Some(0)),("docx.paragraph.border-edge.right","_shell.right",Some(1)),("docx.paragraph.border-edge.bottom","_shell.bottom",Some(2)),("docx.paragraph.border-edge.left","_shell.left",Some(3))].into_iter()
        .map(|(id,label,value)|MenuItem::new(id,crate::shell::i18n::text(label)).disabled(!enabled).checked(edge==value).on(move|this,_,cx|this.choose_paragraph_border_edge(value,cx)).into()).collect();
    let fills: Vec<MenuEntry> = {
        use crate::shell::color_palette::{Palette, PickColor, Swatch, theme_rows};
        let pick: PickColor = std::rc::Rc::new(|this, c, _, cx| this.set_paragraph_shading(c, cx));
        let cur = (enabled && !paragraphs.is_empty()).then(|| {
            let first = paragraphs[0].shading?;
            paragraphs.iter().all(|p| p.shading == Some(first)).then_some(first)
        }).flatten();
        let mut theme = theme_rows(&super::toolbar::THEME_10, &pick, cur);
        if !enabled {
            for s in theme.iter_mut().flatten() {
                s.item = s.item.clone().disabled(true);
            }
        }
        let standard: Vec<Swatch> = [
            ("docx.paragraph.fill.blue", "_shell.light-blue", 0xdeeaf6),
            ("docx.paragraph.fill.green", "_shell.light-green", 0xe2efd9),
            ("docx.paragraph.fill.yellow", "_shell.light-yellow", 0xfff2cc),
            ("docx.paragraph.fill.red", "docx.paragraph-decorations.light-red", 0xfbe4d5),
            ("docx.paragraph.fill.gray", "_shell.gray", 0xd9d9d9),
        ]
        .into_iter()
        .map(|(id, label, c)| Swatch {
            rgb: Rgb(c),
            item: MenuItem::new(id, crate::shell::i18n::text(label))
                .checked(enabled && paragraphs.iter().all(|p| p.shading == Some(Rgb(c))))
                .disabled(!enabled)
                .on(move |this, _, cx| this.set_paragraph_shading(Some(Rgb(c)), cx)),
        })
        .collect();
        let none = MenuItem::tr("docx.paragraph.fill.none")
            .disabled(!enabled)
            .checked(enabled && paragraphs.iter().all(|p| p.shading.is_none()))
            .on(|this, _, cx| this.set_paragraph_shading(None, cx));
        vec![MenuEntry::Palette(Palette {
            none: Some(none),
            theme,
            standard,
            pick: Some(pick),
            more: None,
            button: None, current:None,
        })]
    };
    let mut types:Vec<MenuEntry>=BORDERS.iter().zip(BORDER_IDS).map(|(&(_,style),&id)|MenuItem::new(id,crate::shell::i18n::text(border_label(style))).disabled(!enabled)
        .checked(!borders.is_empty() && borders.iter().all(|b|b.is_some_and(|b|b.style==style))).on(move|this,_,cx|this.set_paragraph_border_style(Some(style),cx)).into()).collect();
    types.push(MenuItem::tr("docx.paragraph.border.none").disabled(!enabled).checked(!borders.is_empty() && borders.iter().all(Option::is_none)).on(|this,_,cx|this.set_paragraph_border_style(None,cx)).into());
    let widths:Vec<MenuEntry>=[("docx.paragraph.border-width.half","0.5 pt",6350),("docx.paragraph.border-width.one","1 pt",12700),("docx.paragraph.border-width.one-half","1.5 pt",19050),("docx.paragraph.border-width.three","3 pt",38100)].into_iter()
        .map(|(id,label,w)|MenuItem::new(id,crate::shell::i18n::text(label)).disabled(!enabled).checked(!borders.is_empty() && borders.iter().all(|b|b.is_some_and(|b|b.width==w)))
        .on(move|this,_,cx|this.set_paragraph_border_width(w,cx)).into()).collect();
    let colors: Vec<MenuEntry> = {
        use crate::shell::color_palette::{Palette, PickColor, Swatch, theme_rows};
        let pick: PickColor = std::rc::Rc::new(|this, c, _, cx| {
            if let Some(rgb) = c {
                this.set_paragraph_border_color(rgb, cx);
            }
        });
        let cur = (!borders.is_empty()).then(|| {
            let first = borders[0]?.color;
            borders.iter().all(|b| b.is_some_and(|b| b.color == first)).then_some(first)
        }).flatten();
        let mut theme = theme_rows(&super::toolbar::THEME_10, &pick, cur);
        if !enabled {
            for s in theme.iter_mut().flatten() {
                s.item = s.item.clone().disabled(true);
            }
        }
        let standard: Vec<Swatch> = [
            ("docx.paragraph.border-color.black", "_shell.black", 0),
            ("docx.paragraph.border-color.red", "_shell.red", 0xff0000),
            ("docx.paragraph.border-color.blue", "_shell.blue", 0x0000ff),
            ("docx.paragraph.border-color.green", "_shell.green", 0x008000),
        ]
        .into_iter()
        .map(|(id, label, c)| Swatch {
            rgb: Rgb(c),
            item: MenuItem::new(id, crate::shell::i18n::text(label))
                .disabled(!enabled)
                .checked(!borders.is_empty() && borders.iter().all(|b| b.is_some_and(|b| b.color == Rgb(c))))
                .on(move |this, _, cx| this.set_paragraph_border_color(Rgb(c), cx)),
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
    vec![Submenu::tr("docx.paragraph.border-edge", "border_style",scopes).into(),
        Submenu::tr("docx.paragraph.fill", "format_color_fill",fills).into(),
        Submenu::tr("docx.paragraph.border", "border_style",types).into(),
        Submenu::tr("docx.paragraph.border-width", "line_weight",widths).into(),
        Submenu::tr("docx.paragraph.border-color", "format_color_text",colors).into()]
}
