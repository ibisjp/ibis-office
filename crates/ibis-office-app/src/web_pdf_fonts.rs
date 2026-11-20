// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Fonts for the text the browser drew itself (K0037, the browser version's PDF).
//!
//! The text system draws a character no font it holds has with the browser's Canvas, in the families the document asked for (then the
//! platform's gothic or mincho list): the record of the page then has a glyph without a font file. Before the PDF is written, each such
//! text is given a font file: the viewer's own file for the first family of its request that has the character (`queryLocalFonts`:
//! Chrome asks the user once), then a font the text system holds that has it. A character no font has is left out, and counted.

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::{FontId, GlyphId};

use crate::pdf_vector::PdfWriter;

/// Fonts given to the writer have ids of their own, apart from the text system's (which has a bit set for the browser's drawn ones).
const GIVEN_FONT_BIT: usize = 1 << (usize::BITS - 2);

/// A font file whose characters are looked up.
struct Face {
    data: Arc<Vec<u8>>,
    index: u32,
    id: FontId,
}

impl Face {
    /// The glyph for `c`, if the font has it.
    fn glyph(&self, c: char) -> Option<u16> {
        use skrifa::MetadataProvider as _;
        let font = skrifa::FontRef::from_index(&self.data, self.index).ok()?;
        let glyph = font.charmap().map(c)?.to_u32();
        (glyph != 0 && glyph <= u16::MAX as u32).then_some(glyph as u16)
    }
}

/// What the browser was asked to draw: the families, first to last, the weight and style.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Request {
    families: Vec<String>,
    weight: u32,
    italic: bool,
}

/// Give `writer` a font for each glyph the browser drew (see the module's note), and put those glyphs in them.
pub(crate) async fn give_fonts(writer: &mut PdfWriter) {
    let drawn = writer.drawn_glyphs();
    if drawn.is_empty() {
        return;
    }
    // The characters each request needs.
    let mut needed: HashMap<Request, BTreeSet<char>> = HashMap::new();
    let mut of_glyph: HashMap<(FontId, GlyphId), (Request, char)> = HashMap::new();
    for (font_id, glyph_id, glyph) in drawn {
        if glyph.color {
            continue;
        }
        let Some(c) = glyph.text.chars().next() else { continue };
        let request = Request { families: glyph.families, weight: glyph.weight.round() as u32, italic: glyph.italic };
        needed.entry(request.clone()).or_default().insert(c);
        of_glyph.insert((font_id, glyph_id), (request, c));
    }
    // The faces of each request, in the order of its families, as many as its characters need. A face is loaded once.
    let mut loaded: HashMap<(String, u32, bool), Option<Rc<Face>>> = HashMap::new();
    let mut next_id = 0usize;
    let mut faces_of: HashMap<Request, Vec<Rc<Face>>> = HashMap::new();
    let mut fallback: HashMap<&'static str, Option<Rc<Face>>> = HashMap::new();
    for (request, characters) in &needed {
        let mut uncovered = characters.clone();
        let mut faces = Vec::new();
        for family in &request.families {
            if uncovered.is_empty() {
                break;
            }
            let key = (family.to_lowercase(), request.weight, request.italic);
            if !loaded.contains_key(&key) {
                let face = load_face(family, request.weight, request.italic).await.map(|(data, index)| {
                    let id = FontId(GIVEN_FONT_BIT | next_id);
                    next_id += 1;
                    writer.give_font(id, data.clone(), index);
                    Rc::new(Face { data, index, id })
                });
                loaded.insert(key.clone(), face);
            }
            if let Some(Some(face)) = loaded.get(&key) {
                let covered: Vec<char> = uncovered.iter().copied().filter(|c| face.glyph(*c).is_some()).collect();
                if !covered.is_empty() {
                    for c in covered {
                        uncovered.remove(&c);
                    }
                    faces.push(face.clone());
                }
            }
        }
        // What no font of the viewer's PC has: BIZ UDGothic / BIZ UDMincho (OFL; fetched once and kept), when a character needs one beyond ASCII.
        if uncovered.iter().any(|c| !c.is_ascii()) {
            if let Some(face) = fallback_face(&request.families, &mut fallback, &mut next_id, writer).await {
                if uncovered.iter().any(|c| face.glyph(*c).is_some()) {
                    faces.push(face);
                }
            }
        }
        faces_of.insert(request.clone(), faces);
    }
    writer.substitute_drawn(&mut |font_id, glyph_id| {
        let (request, c) = of_glyph.get(&(font_id, glyph_id))?;
        faces_of.get(request)?.iter().find_map(|face| face.glyph(*c).map(|glyph| (face.id, GlyphId(glyph as u32))))
    });
}

/// The fallback font for text asked in `families` (see `ibis_office_shell::web::pdf_fallback_font`), loaded once; `None` when it cannot be had.
async fn fallback_face(families: &[String], fallback: &mut HashMap<&'static str, Option<Rc<Face>>>, next_id: &mut usize, writer: &mut PdfWriter) -> Option<Rc<Face>> {
    let id = ibis_office_shell::font_setup::pdf_fallback_for(families).0;
    if let Some(known) = fallback.get(id) {
        return known.clone();
    }
    let face = match ibis_office_shell::web::pdf_fallback_font(families).await {
        Some((_, bytes)) => {
            let data = Arc::new(bytes);
            let font_id = FontId(GIVEN_FONT_BIT | *next_id);
            *next_id += 1;
            writer.give_font(font_id, data.clone(), 0);
            Some(Rc::new(Face { data, index: 0, id: font_id }))
        }
        None => None,
    };
    fallback.insert(id, face.clone());
    face
}

/// The viewer's font file for `family` at `weight` and `italic`, and the face's index in it (a collection holds several).
async fn load_face(family: &str, weight: u32, italic: bool) -> Option<(Arc<Vec<u8>>, u32)> {
    let (bytes, postscript_name) = ibis_office_shell::web::local_font_face(family, weight as f32, italic).await?;
    let index = collection_index(&bytes, &postscript_name);
    Some((Arc::new(bytes), index))
}

/// The index of the face called `postscript_name` in a font collection (0 for a file that is not one, or when no face has the name).
fn collection_index(data: &[u8], postscript_name: &str) -> u32 {
    let Ok(skrifa::raw::FileRef::Collection(collection)) = skrifa::raw::FileRef::new(data) else { return 0 };
    (0..collection.len()).find(|&i| crate::pdf_vector::font_name(data, i) == postscript_name).unwrap_or(0)
}
