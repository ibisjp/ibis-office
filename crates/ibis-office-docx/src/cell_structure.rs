// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Bounded horizontal cell merge and split, preserving paragraph identities.
use gpui_kit::*;
use ibis_doc_model::{Block,FlowTable,FlowCommand,FlowParagraph,VerticalMerge};
use ibis_doc_model::flow_command;
use crate::shell::editor::SlideEditor;
use crate::ext::*;
#[ibis_office_macros::editor_ext(CellStructure)]
impl SlideEditor {
    fn structure_table(&self)->Option<(&FlowTable,usize,usize)> {
        let (id,row,cell)=self.table_at_caret()?;
        let d=self.flow()?;let (container,index)=flow_command::locate(d,id)?;
        let Block::Table(t)=flow_command::blocks(d,container)?.get(index)? else{return None};
        Some((t,row,cell))
    }
    pub(crate) fn can_merge_right_cell(&self)->bool {
        let Some((t,row,cell))=self.structure_table() else{return false};
        let Some(row)=t.rows.get(row) else{return false};
        let (Some(a),Some(b))=(row.cells.get(cell),row.cells.get(cell+1)) else{return false};
        a.merge==VerticalMerge::None && b.merge==VerticalMerge::None && !a.blocks.is_empty() && !b.blocks.is_empty()
            && a.blocks.iter().chain(&b.blocks).all(|b|matches!(b,Block::Paragraph(_)))
    }
    pub(crate) fn can_split_cell_grid(&self)->bool {
        let Some((t,row,cell))=self.structure_table() else{return false};
        t.rows.get(row).and_then(|r|r.cells.get(cell)).is_some_and(|c|c.merge==VerticalMerge::None && (2..=63).contains(&c.span)
            && !c.blocks.is_empty() && c.blocks.iter().all(|b|matches!(b,Block::Paragraph(_))))
    }
    /// Grid column where each cell of a row starts.
    fn grid_starts(row:&ibis_doc_model::FlowRow)->Vec<usize> {
        let mut at=0;row.cells.iter().map(|c|{let s=at;at+=c.span.max(1) as usize;s}).collect()
    }
    /// The cell below the caret's cell (its row, index) with the last row
    /// the caret's cell reaches by a vertical merge: the rows of it and the
    /// one under it.
    fn cell_below(&self)->Option<(usize,usize,usize,usize)> {
        let (t,row,cell)=self.structure_table()?;
        let starts=Self::grid_starts(t.rows.get(row)?);
        let col=*starts.get(cell)?;
        // The merge's first row, then its last.
        let mut top=row;let mut top_cell=cell;
        while t.rows[top].cells[top_cell].merge==VerticalMerge::Continue {
            if top==0 {return None;}
            top-=1;
            top_cell=Self::grid_starts(&t.rows[top]).iter().position(|s|*s==col)?;
        }
        let span=t.rows[top].cells[top_cell].span.max(1);
        let mut last=top;
        while let Some(r)=t.rows.get(last+1) {
            let i=Self::grid_starts(r).iter().position(|s|*s==col);
            match i {Some(i) if r.cells[i].merge==VerticalMerge::Continue&&r.cells[i].span.max(1)==span=>last+=1,_=>break}
        }
        let below=t.rows.get(last+1)?;
        let i=Self::grid_starts(below).iter().position(|s|*s==col)?;
        let b=&below.cells[i];
        if b.merge!=VerticalMerge::None||b.span.max(1)!=span {return None;}
        Some((top,top_cell,last+1,i))
    }
    /// A cell merges down into an empty cell below it, which keeps its
    /// paragraphs (the text of one that is not empty cannot move between
    /// rows when saved yet).
    pub(crate) fn can_merge_below_cell(&self)->bool {
        let Some((t,..))=self.structure_table() else{return false};
        let Some((top,tc,br,bc))=self.cell_below() else{return false};
        t.rows[top].cells[tc].blocks.iter().all(|b|matches!(b,Block::Paragraph(_)))
            && !t.rows[br].cells[bc].blocks.is_empty()
            && t.rows[br].cells[bc].blocks.iter().all(|b|matches!(b,Block::Paragraph(p)if p.len()==0))
    }
    pub(crate) fn merge_below_cell(&mut self,cx:&mut Context<Self>) {
        if !self.can_merge_below_cell() {return;}
        let Some((t,..))=self.structure_table() else{return};let mut table=t.clone();
        let Some((top,tc,br,bc))=self.cell_below() else{return};
        table.rows[br].cells[bc].merge=VerticalMerge::Continue;
        table.rows[top].cells[tc].merge=VerticalMerge::Start;
        table.resolve_borders();self.replace_structure_table(table,cx);
    }
    pub(crate) fn can_unmerge_below_cell(&self)->bool {
        let Some((t,row,cell))=self.structure_table() else{return false};
        t.rows.get(row).and_then(|r|r.cells.get(cell)).is_some_and(|c|c.merge!=VerticalMerge::None)
    }
    pub(crate) fn unmerge_below_cell(&mut self,cx:&mut Context<Self>) {
        if !self.can_unmerge_below_cell() {return;}
        let Some((t,row,cell))=self.structure_table() else{return};let mut table=t.clone();
        let col=Self::grid_starts(&table.rows[row])[cell];
        // The merge's rows: the first (its text stays there) to the last.
        let mut first=row;
        while table.rows[first].cells[Self::grid_starts(&table.rows[first]).iter().position(|s|*s==col).unwrap()].merge==VerticalMerge::Continue {
            if first==0 {return;}first-=1;
        }
        let mut r=first;
        loop {
            let i=Self::grid_starts(&table.rows[r]).iter().position(|s|*s==col).unwrap();
            table.rows[r].cells[i].merge=VerticalMerge::None;
            r+=1;
            let Some(next)=table.rows.get(r) else{break};
            match Self::grid_starts(next).iter().position(|s|*s==col) {Some(j) if next.cells[j].merge==VerticalMerge::Continue=>{},_=>break}
        }
        table.resolve_borders();self.replace_structure_table(table,cx);
    }
    pub(crate) fn merge_right_cell(&mut self,cx:&mut Context<Self>) {
        if !self.can_merge_right_cell() {return;}
        let Some((t,row,cell))=self.structure_table() else{return};let mut table=t.clone();
        let right=table.rows[row].cells.remove(cell+1);
        let left=&mut table.rows[row].cells[cell];
        left.span=left.span.max(1)+right.span.max(1);
        left.own_borders[1]=right.own_borders[1];left.blocks.extend(right.blocks);
        table.resolve_borders();self.replace_structure_table(table,cx);
    }
    pub(crate) fn split_cell_grid(&mut self,cx:&mut Context<Self>) {
        if !self.can_split_cell_grid() {return;}
        let Some((t,row,cell))=self.structure_table() else{return};let mut table=t.clone();
        let template=table.rows[row].cells[cell].clone();
        let Some(Block::Paragraph(p))=template.blocks.first() else{return};
        let mut next=self.flow().unwrap().new_id();
        let count=template.span.max(1) as usize;table.rows[row].cells[cell].span=1;
        let mut cells=Vec::new();
        for _ in 1..count {
            let mut c=template.clone();c.id=next;next+=1;c.span=1;
            let mut blank=FlowParagraph {id:next,style:p.style.clone(),table_style:p.table_style.clone(),direct:p.direct.clone(),mark_format:p.mark_format.clone(),..Default::default()};
            next+=1;blank.resolve(&self.flow().unwrap().styles);c.blocks=vec![Block::Paragraph(blank)];cells.push(c);
        }
        table.rows[row].cells.splice(cell+1..cell+1,cells);table.resolve_borders();self.replace_structure_table(table,cx);
    }
    fn replace_structure_table(&mut self,table:FlowTable,cx:&mut Context<Self>) {
        let Some((container,index))=self.flow().and_then(|d|flow_command::locate(d,table.id)) else{return};
        let command=FlowCommand::ReplaceBlocks {container,index,remove:1,blocks:vec![Block::Table(table)]};
        // Validate the bounded save path before changing the live document.
        // Wrapped/complex source cells can be rejected without losing work.
        if let Some(source)=self.source.as_ref() {
            let mut candidate=ibis_doc_model::Document::new(self.presentation().clone());
            if candidate.execute(vec![ibis_doc_model::Command::Flow(command.clone())]).is_err() {return;}
            if let Err(error)=source.save(candidate.presentation(),ibis_pptx::SaveOptions::default()) {
                self.status=Some(crate::shell::i18n::format("docx.cell-structure.merging-splitting-cells-can-t", &[("error", &error)]).into());cx.notify();return;
            }
        }
        self.run_flow(vec![command],cx);
    }
}
