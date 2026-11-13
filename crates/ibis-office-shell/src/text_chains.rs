// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Linked text boxes (`ibis_doc_model::TextChain`, GitHub issue #131), the part every shape edit needs: the text of a chain
//! read from its boxes, flowed through them, and flowed again after an edit (`SlideEditor::run`, the end of a text edit, a box
//! deleted). Making and breaking the links, their marks and menus are the presentation's (`pptx/ibis_text.rs`).

use ibis_doc_model::{Command, ShapeId, ShapeKind};

use crate::editor::SlideEditor;

/// What a chain's box offers the text: the width its lines are broken to (EMU, along the lines) and the room for the lines
/// (points, across them: the height of the text area, or the width of a vertical box).
pub fn chain_space(shape: &ibis_doc_model::Shape) -> Option<(f64, f32)> {
    let body = shape.text_body()?;
    let (fw, fh) = (shape.frame.width as f64, shape.frame.height as f64);
    let (l, t, r, b) = match &shape.kind {
        ShapeKind::Auto(auto) => auto.geometry.text_rect(fw, fh),
        _ => (0.0, 0.0, fw, fh),
    };
    let (w, h) = (r - l, b - t);
    let i = body.insets;
    let (along, across, a_ins, c_ins) = if body.vertical { (h, w, i.top + i.bottom, i.left + i.right) } else { (w, h, i.left + i.right, i.top + i.bottom) };
    Some(((along - a_ins as f64).max(0.0), ((across - c_ins as f64).max(0.0) / ibis_doc_model::EMU_PER_POINT as f64) as f32))
}

impl SlideEditor {
    /// Whether `shape` can be in a chain: a text box or a shape with text.
    pub fn is_chainable(&self, shape: ShapeId) -> bool {
        self.slide().shape(shape).is_some_and(|s| matches!(s.kind, ShapeKind::TextBox(_) | ShapeKind::Auto(_)) && s.text_body().is_some())
    }

    /// The boxes of the chain of `shape` that are still on the slide, in order.
    pub fn chain_boxes_of(&self, shape: ShapeId) -> Option<Vec<ShapeId>> {
        let slide = self.slide();
        slide.chain_of(shape).map(|c| c.boxes.iter().copied().filter(|b| slide.shape(*b).is_some()).collect())
    }

    /// The chains of the slide with `boxes` as one (a chain with those boxes is replaced, else it is added; fewer than two
    /// boxes: no chain).
    fn chains_with(&self, boxes: &[ShapeId], flowed: Option<&ibis_doc_model::ibis_text::Flowed>) -> Vec<ibis_doc_model::TextChain> {
        let mut chains: Vec<_> = self.slide().text_chains.iter().filter(|c| !c.boxes.iter().any(|b| boxes.contains(b))).cloned().collect();
        if boxes.len() >= 2 {
            let (joined, padding) = flowed.map_or((vec![false; boxes.len()], vec![false; boxes.len()]), |f| (f.joined.clone(), f.padding.clone()));
            chains.push(ibis_doc_model::TextChain { boxes: boxes.to_vec(), joined, padding });
        }
        chains
    }

    /// `full` flowed through `boxes`: how the text divides among them.
    pub fn flow_through(&self, boxes: &[ShapeId], full: Vec<ibis_doc_model::text::Paragraph>) -> Option<ibis_doc_model::ibis_text::Flowed> {
        let slide = self.slide();
        let shapes: Vec<&ibis_doc_model::Shape> = boxes.iter().map(|id| slide.shape(*id)).collect::<Option<_>>()?;
        let spaces: Vec<(f64, f32)> = shapes.iter().map(|s| chain_space(s)).collect::<Option<_>>()?;
        let capacities: Vec<f32> = spaces.iter().map(|s| s.1).collect();
        let text = self.resources.window_text_system();
        let res = &self.resources;
        Some(ibis_doc_model::ibis_text::flow_text(full, &capacities, &mut |p, b| res.measure_paragraph(shapes[b].text_body().expect("a box has text"), p, spaces[b].0, false, false, 1.0, None, &text)))
    }

    /// `full` flowed through `boxes`: the command that makes the slide's chains and the boxes' texts so (`None` when a box
    /// cannot hold text).
    pub fn flow_command(&self, boxes: &[ShapeId], full: Vec<ibis_doc_model::text::Paragraph>) -> Option<Command> {
        let flowed = self.flow_through(boxes, full)?;
        let chains = self.chains_with(boxes, Some(&flowed));
        let bodies = boxes.iter().copied().zip(flowed.portions.iter().cloned()).collect();
        Some(Command::SetTextChains { slide: self.slide().id, chains, bodies })
    }

    /// The whole text of a chain, from what its boxes hold.
    pub fn chain_text(&self, boxes: &[ShapeId], chain: Option<&ibis_doc_model::TextChain>) -> Vec<ibis_doc_model::text::Paragraph> {
        let slide = self.slide();
        let bodies: Vec<Vec<ibis_doc_model::text::Paragraph>> = boxes.iter().map(|id| slide.shape(*id).and_then(|s| s.text_body()).map(|b| b.paragraphs.clone()).unwrap_or_default()).collect();
        // (A box that was only padding and has text typed in it is a box of the text now.)
        let unpadded = |padding: Vec<bool>| -> Vec<bool> { padding.into_iter().zip(&bodies).map(|(p, b)| p && b.iter().all(|q| q.text().is_empty()) && b.len() <= 1).collect() };
        match chain {
            // (A chain with a box gone: the parts that were flags of the others are read as they were.)
            Some(c) if c.boxes.len() == boxes.len() => ibis_doc_model::ibis_text::collect_flow(&ibis_doc_model::TextChain { boxes: c.boxes.clone(), joined: c.joined.clone(), padding: unpadded(c.padding.clone()) }, &bodies),
            Some(c) => {
                let kept: Vec<usize> = c.boxes.iter().enumerate().filter(|(_, b)| boxes.contains(b)).map(|(i, _)| i).collect();
                let pruned = ibis_doc_model::TextChain { boxes: boxes.to_vec(), joined: kept.iter().map(|&i| c.joined[i]).collect(), padding: unpadded(kept.iter().map(|&i| c.padding[i]).collect()) };
                ibis_doc_model::ibis_text::collect_flow(&pruned, &bodies)
            }
            None => bodies.into_iter().flatten().collect(),
        }
    }

    /// The command that flows the text of the chain `shape` is in again through its boxes (boxes that are gone are dropped from it);
    /// `None` when the boxes already hold what the flow gives.
    fn reflow_command(&self, shape: ShapeId) -> Option<Command> {
        let boxes = self.chain_boxes_of(shape)?;
        let chain = self.slide().chain_of(shape).cloned();
        let full = self.chain_text(&boxes, chain.as_ref());
        let command = self.flow_command(&boxes, full)?;
        let slide = self.slide();
        if let Command::SetTextChains { chains, bodies, .. } = &command {
            let same = *chains == slide.text_chains && bodies.iter().all(|(id, paragraphs)| slide.shape(*id).and_then(|s| s.text_body()).is_some_and(|b| b.paragraphs == *paragraphs));
            if same {
                return None;
            }
        }
        Some(command)
    }

    /// The text of the chain `shape` is in flowed again (after an edit), as part of the last undo step.
    pub(crate) fn reflow_chain_of(&mut self, shape: ShapeId, cx: &mut gpui_kit::Context<Self>) {
        if let Some(command) = self.reflow_command(shape) {
            let _ = self.document.amend_last(vec![command]);
            self.sync_selection();
            cx.notify();
        }
    }

    /// Every chain of the slide flowed again, as part of the last undo step: after any change (a box deleted, moved or resized, its
    /// text or its format changed, a paste).
    pub(crate) fn reflow_all_chains(&mut self) {
        for chain in self.slide().text_chains.clone() {
            let Some(&first) = chain.boxes.first() else { continue };
            if let Some(command) = self.reflow_command(first) {
                let _ = self.document.amend_last(vec![command]);
            }
        }
    }
}

// ----- Editing the text of a chain (#131) ---------------------------------------------------------------------------

/// Where the text of each box of a chain lies in the text of the whole chain: positions in characters, the paragraphs of the whole text joined
/// by a line feed (as `TextBody::text`). The end of a box and the start of the box that goes on from it are the same place when the text
/// was cut in the middle of a paragraph (`TextChain::joined`), and a line feed apart when it was cut between two paragraphs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainSpan {
    pub boxes: Vec<ShapeId>,
    /// No text of the chain reached the box (it holds only the empty paragraph a text box has).
    pub blank: Vec<bool>,
    pub starts: Vec<usize>,
    pub ends: Vec<usize>,
}

impl ChainSpan {
    /// The span of boxes holding `lengths` characters each, `joined` as `TextChain::joined` says.
    pub fn new(boxes: Vec<ShapeId>, lengths: &[usize], joined: &[bool], blank: Vec<bool>) -> Self {
        let n = boxes.len();
        let (mut starts, mut ends) = (vec![0; n], vec![0; n]);
        let mut at: Option<usize> = None;
        for i in 0..n {
            if blank[i] {
                (starts[i], ends[i]) = (at.unwrap_or(0), at.unwrap_or(0));
                continue;
            }
            let start = match at {
                None => 0,
                Some(end) if joined.get(i).copied().unwrap_or(false) => end,
                Some(end) => end + 1,
            };
            (starts[i], ends[i]) = (start, start + lengths[i]);
            at = Some(ends[i]);
        }
        Self { boxes, blank, starts, ends }
    }

    /// The box position `g` of the whole text is in, and the place in that box: the first box that reaches it (the end of a box is not
    /// the start of the next one), the last box for a place past all the text.
    pub fn locate(&self, g: usize) -> (usize, usize) {
        let mut last = 0;
        for i in 0..self.boxes.len() {
            if self.blank[i] {
                continue;
            }
            last = i;
            if g <= self.ends[i] {
                return (i, g.saturating_sub(self.starts[i]));
            }
        }
        (last, self.ends[last] - self.starts[last])
    }

    /// The length of the whole text.
    pub fn total(&self) -> usize {
        self.ends.last().copied().unwrap_or(0)
    }
}

/// What a chain with a text being composed looks like: each box with the part of the text it holds (the composing text in it, drawn as it is),
/// and where the caret is (the end of the composing text, which may be in a box after the one being edited).
#[derive(Clone, Debug)]
pub struct ChainDisplay {
    pub bodies: Vec<(ShapeId, ibis_doc_model::TextBody)>,
    pub caret_box: ShapeId,
    /// The caret in the text of `caret_box`.
    pub caret: usize,
}

impl SlideEditor {
    /// Where the text of the chain `shape` is in lies in each of its boxes now.
    pub fn chain_span(&self, shape: ShapeId) -> Option<ChainSpan> {
        let slide = self.slide();
        let chain = slide.chain_of(shape)?;
        let mut lengths = Vec::new();
        let mut blank = Vec::new();
        for (i, id) in chain.boxes.iter().enumerate() {
            let body = slide.shape(*id)?.text_body()?;
            let length = body.text().chars().count();
            blank.push(chain.padding.get(i).copied().unwrap_or(false) && length == 0 && body.paragraphs.len() <= 1);
            lengths.push(length);
        }
        Some(ChainSpan::new(chain.boxes.clone(), &lengths, &chain.joined, blank))
    }

    /// The text being composed in a box of a chain, with the text flowed through the boxes as if it were typed (so what does not fit the box
    /// goes on in the next, as typed text does). `None` when nothing is composed in a chain. `word_box` as `composing_body_as`.
    pub fn chain_display(&self, word_box: bool) -> Option<std::rc::Rc<ChainDisplay>> {
        if !self.uses_shape_edit() {
            return None;
        }
        let edit = self.shape_edit.edit.as_ref().filter(|e| e.cell.is_none() && e.marked.is_some())?;
        self.slide().chain_of(edit.shape)?;
        let key = (edit.clone(), self.document.undo_len(), self.current_slide, word_box);
        if let Some((cached, display)) = self.shape_edit.chain_display_cache.borrow().as_ref()
            && *cached == key
        {
            return display.clone();
        }
        let display = self.compute_chain_display(edit, word_box).map(std::rc::Rc::new);
        *self.shape_edit.chain_display_cache.borrow_mut() = Some((key, display.clone()));
        display
    }

    fn compute_chain_display(&self, edit: &crate::text_edit::TextEditing, word_box: bool) -> Option<ChainDisplay> {
        let marked = edit.marked.as_ref()?;
        let slide = self.slide();
        let boxes = self.chain_boxes_of(edit.shape)?;
        let chain = slide.chain_of(edit.shape)?.clone();
        if boxes.len() != chain.boxes.len() {
            return None;
        }
        let span = self.chain_span(edit.shape)?;
        let at = boxes.iter().position(|b| *b == edit.shape)?;
        let mut whole = slide.shape(edit.shape)?.text_body()?.clone();
        whole.paragraphs = self.chain_text(&boxes, Some(&chain));
        let range = edit.range();
        let (start, remove) = if self.chain_select_all_active() { (0, span.total()) } else { (span.starts[at] + range.start, range.len()) };
        whole.splice(start, remove, marked);
        crate::text_edit::ime_formats(&mut whole, start, marked, &edit.marked_clauses, word_box);
        let flowed = self.flow_through(&boxes, whole.paragraphs)?;
        let lengths: Vec<usize> = flowed.portions.iter().map(|portion| portion.iter().map(|p| p.text().chars().count()).sum::<usize>() + portion.len().saturating_sub(1)).collect();
        let shown = ChainSpan::new(boxes.clone(), &lengths, &flowed.joined, flowed.padding.clone());
        let (j, caret) = shown.locate(start + marked.chars().count());
        let bodies = boxes
            .iter()
            .zip(flowed.portions)
            .filter_map(|(id, portion)| {
                let mut body = slide.shape(*id)?.text_body()?.clone();
                body.paragraphs = portion;
                Some((*id, body))
            })
            .collect();
        Some(ChainDisplay { bodies, caret_box: boxes[j], caret })
    }

    /// Whether Select All took the text of the whole chain (the selection of the box being edited, and the text of the boxes round it).
    pub fn chain_select_all_active(&self) -> bool {
        let Some(edit) = self.shape_edit.edit.as_ref().filter(|e| e.cell.is_none() && !e.range().is_empty()) else { return false };
        self.shape_edit.chain_select_all.as_ref().is_some_and(|(shape, range)| *shape == edit.shape && *range == edit.range()) && self.slide().chain_of(edit.shape).is_some()
    }

    /// ⌘A in a box of a chain: all the text of the box, and with it the text of the boxes round it (copying, deleting, typing and composing take
    /// the text of the whole chain).
    pub(crate) fn select_all_in_chain(&mut self) {
        let in_chain = self.shape_edit.edit.as_ref().is_some_and(|e| e.cell.is_none() && !e.range().is_empty() && self.slide().chain_of(e.shape).is_some());
        self.shape_edit.chain_select_all = if in_chain { self.shape_edit.edit.as_ref().map(|e| (e.shape, e.range())) } else { None };
    }

    /// The whole text of the chain of the box being edited, as plain text (what copying the selection of Select All takes).
    pub fn chain_selected_text(&self) -> Option<String> {
        let shape = self.shape_edit.edit.as_ref()?.shape;
        let boxes = self.chain_boxes_of(shape)?;
        let chain = self.slide().chain_of(shape).cloned();
        Some(self.chain_text(&boxes, chain.as_ref()).iter().map(|p| p.text()).collect::<Vec<_>>().join("\n"))
    }

    /// The selection of the whole chain taken away (Select All, then a deletion or typing): the text of every box goes, the caret is at the
    /// start of the first. With `typing`, what is typed next joins this step of undo. `true` if there was such a selection.
    pub(crate) fn delete_chain_selection(&mut self, typing: bool, cx: &mut gpui_kit::Context<Self>) -> bool {
        if !self.chain_select_all_active() {
            return false;
        }
        let Some(shape) = self.shape_edit.edit.as_ref().map(|e| e.shape) else { return false };
        let Some(boxes) = self.chain_boxes_of(shape) else { return false };
        let chain = self.slide().chain_of(shape).cloned();
        let mut paragraph = self.chain_text(&boxes, chain.as_ref()).into_iter().next().unwrap_or_default();
        paragraph.runs.clear();
        let Some(command) = self.flow_command(&boxes, vec![paragraph]) else { return false };
        if self.document.execute(vec![command]).is_err() {
            return false;
        }
        self.shape_edit.chain_select_all = None;
        if let Some(e) = self.shape_edit.edit.as_mut() {
            e.shape = boxes[0];
            e.anchor = 0;
            e.caret = 0;
            e.typing = typing;
            e.is_new = false;
        }
        self.set_selection(vec![boxes[0]]);
        cx.notify();
        true
    }

    /// The box the caret is drawn in: the box being edited, or, while text is composed in a chain, the box the end of the composing text is in.
    pub fn editing_display_shape(&self) -> Option<ShapeId> {
        self.chain_display(false).map(|d| d.caret_box).or_else(|| self.text_editing().map(|e| e.shape))
    }

    /// The text of the chain being edited flowed through its boxes again, and the caret (and the selection) put where the text it was in
    /// now is: typing at the end of a box goes on in the next when the box is full, and a deletion pulls the text of the next box back.
    /// Called by the edits of the text, after they put the caret where they leave it.
    pub(crate) fn flow_edited_chain(&mut self, cx: &mut gpui_kit::Context<Self>) {
        if !self.uses_shape_edit() {
            return;
        }
        let Some(edit) = self.shape_edit.edit.clone().filter(|e| e.cell.is_none()) else { return };
        let Some(span) = self.chain_span(edit.shape) else { return };
        let Some(at) = span.boxes.iter().position(|b| *b == edit.shape) else { return };
        let (caret, anchor) = (span.starts[at] + edit.caret, span.starts[at] + edit.anchor);
        let Some(command) = self.reflow_command(edit.shape) else { return };
        let _ = self.document.amend_last(vec![command]);
        let Some(after) = self.chain_span(edit.shape) else { return };
        let (to, local_caret) = after.locate(caret);
        let (anchor_box, local_anchor) = after.locate(anchor);
        let shape = after.boxes[to];
        let length = self.slide().shape(shape).and_then(|s| s.text_body()).map_or(0, |b| b.text().chars().count());
        if let Some(e) = self.shape_edit.edit.as_mut() {
            e.shape = shape;
            e.caret = local_caret;
            // (A selection that now lies over two boxes goes to the edge of the box the caret is in.)
            e.anchor = if anchor_box == to { local_anchor } else if anchor < caret { 0 } else { length };
        }
        if shape != edit.shape {
            self.set_selection(vec![shape]);
        }
        self.sync_selection();
        cx.notify();
    }

    /// ← and → at the edge of a box of a chain: the caret goes on in the box after (or before) it, to the next (or the previous) character of
    /// the text: where the text was cut in the middle of a paragraph, that is the second character of the next box, as the end of one box and
    /// the start of the next are the same place. `true` if the caret moved.
    pub(crate) fn move_caret_across_boxes(&mut self, key: &str, cx: &mut gpui_kit::Context<Self>) -> bool {
        let Some((edit, span)) = self.edit_in_chain() else { return false };
        let at = span.boxes.iter().position(|b| *b == edit.shape).unwrap_or(0);
        let joined = |i: usize| self.slide().chain_of(edit.shape).and_then(|c| c.joined.get(i).copied()).unwrap_or(false);
        let length = |i: usize| span.ends[i] - span.starts[i];
        let (to, local) = match key {
            "right" if edit.caret >= length(at) && at + 1 < span.boxes.len() && !span.blank[at + 1] => (at + 1, if joined(at + 1) { 1.min(length(at + 1)) } else { 0 }),
            "left" if edit.caret == 0 && at > 0 && !span.blank[at - 1] => (at - 1, if joined(at) { length(at - 1).saturating_sub(1) } else { length(at - 1) }),
            _ => return false,
        };
        self.enter_chain_box(span.boxes[to], local, cx);
        true
    }

    /// ↑ and ↓ on the first and the last line of a box of a chain (`to` is where the key moves the caret in the box: its start or its end):
    /// the caret goes on at the end of the box before, or at the start of the box after. `true` if the caret moved.
    pub(crate) fn move_caret_across_boxes_vertically(&mut self, key: &str, to: usize, cx: &mut gpui_kit::Context<Self>) -> bool {
        let Some((edit, span)) = self.edit_in_chain() else { return false };
        let at = span.boxes.iter().position(|b| *b == edit.shape).unwrap_or(0);
        let length = |i: usize| span.ends[i] - span.starts[i];
        let (target, local) = match key {
            "down" if to >= length(at) && at + 1 < span.boxes.len() && !span.blank[at + 1] => (at + 1, 0),
            "up" if to == 0 && at > 0 && !span.blank[at - 1] => (at - 1, length(at - 1)),
            _ => return false,
        };
        self.enter_chain_box(span.boxes[target], local, cx);
        true
    }

    /// The text edit in progress, when it is of a box of a chain, with nothing selected or composed, and where the text of the chain lies.
    fn edit_in_chain(&self) -> Option<(crate::text_edit::TextEditing, ChainSpan)> {
        if !self.uses_shape_edit() {
            return None;
        }
        let edit = self.shape_edit.edit.clone().filter(|e| e.cell.is_none() && e.marked.is_none() && e.range().is_empty())?;
        let span = self.chain_span(edit.shape)?;
        Some((edit, span))
    }

    /// The caret goes to `local` in the text of `shape`, a box of the chain being edited.
    fn enter_chain_box(&mut self, shape: ShapeId, local: usize, cx: &mut gpui_kit::Context<Self>) {
        if let Some(e) = self.shape_edit.edit.as_mut() {
            e.shape = shape;
            e.anchor = local;
            e.caret = local;
            e.typing = false;
        }
        self.shape_edit.goal_x = None;
        self.shape_edit.chain_select_all = None;
        self.set_selection(vec![shape]);
        self.restart_caret_blink(cx);
        cx.notify();
    }
}
