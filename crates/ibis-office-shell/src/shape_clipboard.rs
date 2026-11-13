// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Shapes copied and pasted in Office's own clipboard format (DrawingML; the OS side is `native_shape_clipboard`),
//! the same on every screen. Where the pasted shapes go is the screen's to say: into the text of a flowing document
//! (`FormatEditor::paste_shapes_in_text`), or on the page at the place `FormatEditor::place_pasted_shapes` gives.
#[cfg(headless_export)]
use crate::native_shape_clipboard::image_fallback;
use crate::native_shape_clipboard::{read, write};
use crate::editor::SlideEditor;
use gpui_kit::*;
use ibis_doc_model::{Command, Fill, MediaId, ShapeKind, TextBody};

thread_local! {
    /// The shapes of the last copy in a document, with the native bytes that went to the clipboard: 縦中横 (Word's, `w:eastAsianLayout`)
    /// is not in those bytes, so a paste into a document that finds the same bytes takes it back from here. Pasted anywhere else
    /// (a presentation, a workbook, another program) the text has none.
    static TATE_SIDE: std::cell::RefCell<Option<(Vec<u8>, Vec<ibis_doc_model::Shape>)>> = const { std::cell::RefCell::new(None) };
}

/// Gives the runs of `body` the 縦中横 the runs of `from` have (same paragraphs, same texts), or takes it away (`None`).
fn carry_tate_chu_yoko(body: &mut TextBody, from: Option<&TextBody>) {
    for (i, p) in body.paragraphs.iter_mut().enumerate() {
        let source = from.and_then(|b| b.paragraphs.get(i));
        for (k, r) in p.runs.iter_mut().enumerate() {
            r.style.tate_chu_yoko = source.and_then(|s| s.runs.get(k)).filter(|s| s.text == r.text).and_then(|s| s.style.tate_chu_yoko);
        }
    }
}

impl SlideEditor {
    pub fn paste_office_shapes(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(bytes) = read() else {
            return false;
        };
        let paste_stamp=crate::paste_placement::stamp(&bytes);
        let Ok(opened) = ibis_pptx::shape_clipboard::read_gvml(bytes.clone()) else {
            return false;
        };
        let foreign = opened.presentation;
        let Some(slide) = foreign.slides.first() else {
            return false;
        };
        if slide.shapes.is_empty() {
            return false;
        }
        let mut shapes = slide.shapes.clone();
        let mut map = std::collections::HashMap::new();
        for media in &foreign.media {
            let id = self
                .document
                .ids()
                .add_media(media.format, media.data.clone());
            self.resources.add_picture(id, media.format, &media.data);
            map.insert(media.id, id);
        }
        let remap = |id: &mut MediaId| {
            if let Some(new) = map.get(id) {
                *id = *new;
            }
        };
        let fill = |fill: &mut Fill| match fill {
            Fill::Image(id) | Fill::TileImage { media: id, .. } => remap(id),
            Fill::Blip(b) => remap(&mut b.media),
            _ => {}
        };
        let text = |body: &mut TextBody| {
            for p in &mut body.paragraphs {
                if let Some(id) = p.bullet.as_mut().and_then(|b| b.picture.as_mut()) {
                    remap(id);
                }
            }
        };
        // 縦中横 stays in the document it was copied in; anywhere else the text has none.
        let side = TATE_SIDE.with(|t| t.borrow().clone()).filter(|(b, _)| self.presentation().flow.is_some() && *b == bytes);
        for (i, shape) in shapes.iter_mut().enumerate() {
            let from = side.as_ref().and_then(|(_, s)| s.get(i));
            match &mut shape.kind {
                ShapeKind::Auto(a) => {
                    if let Some(body) = &mut a.text {
                        carry_tate_chu_yoko(body, from.and_then(|f| f.text_body()));
                    }
                }
                ShapeKind::TextBox(body) => carry_tate_chu_yoko(body, from.and_then(|f| f.text_body())),
                _ => {}
            }
        }
        let mut flow_shape_id = self.presentation().flow.as_ref().map(|doc| doc.new_shape_id());
        for shape in &mut shapes {
            shape.id = if let Some(id) = flow_shape_id.as_mut() {
                let value = *id;
                *id += 1;
                ibis_doc_model::ShapeId(value)
            } else {
                self.document.ids().new_shape_id()
            };
            match &mut shape.kind {
                ShapeKind::Picture(p) => remap(&mut p.media),
                ShapeKind::Auto(a) => {
                    fill(&mut a.fill);
                    if let Some(body) = &mut a.text {
                        text(body);
                    }
                }
                ShapeKind::TextBox(body) => text(body),
                ShapeKind::Table(t) => {
                    for cell in t.rows.iter_mut().flat_map(|r| &mut r.cells) {
                        fill(&mut cell.fill);
                        text(&mut cell.text);
                    }
                }
                _ => {}
            }
        }
        if self.presentation().flow.is_some() {
            return crate::format::with_format!(self, paste_shapes_in_text(self, shapes, cx));
        }
        crate::format::with_format!(self, place_pasted_shapes(self, &mut shapes, paste_stamp));
        let start = self.slide().shapes.len();
        let owner = self.current_slide;
        let ids = shapes.iter().map(|s| s.id).collect();
        let commands = shapes
            .into_iter()
            .enumerate()
            .map(|(i, shape)| Command::InsertShape {
                slide: owner,
                index: start + i,
                shape,
            })
            .collect();
        self.run(commands, cx);
        self.set_selection(ids);
        cx.notify();
        true
    }
}

impl SlideEditor {
    pub fn copy_office_shapes(
        &mut self,
        shapes: &[ibis_doc_model::Shape],
        cx: &mut Context<Self>,
    ) -> bool {
        if shapes.is_empty() {
            return false;
        }
        let Ok(native) = ibis_pptx::shape_clipboard::write_gvml(self.presentation(), shapes) else {
            return false;
        };
        if self.presentation().flow.is_some() {
            TATE_SIDE.with(|t| *t.borrow_mut() = Some((native.clone(), shapes.to_vec())));
        }
        let text = shapes
            .iter()
            .filter_map(|s| s.text_body())
            .map(|b| b.text())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut entries = Vec::new();
        #[cfg(headless_export)]
        if let Some(image) = image_fallback(self.presentation(), shapes) {
            entries.push(ClipboardEntry::Image(image));
        }
        if !text.is_empty() {
            entries.extend(
                ClipboardItem::new_string(text.clone())
                    .entries()
                    .iter()
                    .cloned(),
            );
        }
        // GPUI's native macOS multi-entry writer keeps only strings. Write
        // the image first (DIB as well on Windows), then append text/native
        // formats without clearing that image representation.
        let simulated = crate::native_shape_clipboard::is_simulated();
        if !simulated
            && cfg!(any(target_os = "macos", windows))
            && let Some(ClipboardEntry::Image(image)) = entries
                .iter()
                .find(|e| matches!(e, ClipboardEntry::Image(_)))
        {
            cx.write_to_clipboard(ClipboardItem::new_image(image));
        } else {
            cx.write_to_clipboard(ClipboardItem { entries });
        }
        let _ = write(&native, (!text.is_empty()).then_some(text.as_str()));
        true
    }
}
