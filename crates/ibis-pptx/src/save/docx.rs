// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Writing changes to a document (`.docx`) — the document担当's file.
//!
//! The reader (`crate::docx`) gives every paragraph and table an id and
//! records, in a [`SourceMap`], the part and element path it came from, and
//! for each position of a paragraph's runs (a character, a tab, a break, a
//! picture, a field) the `w:r` and the child element that made it.
//!
//! A save compares the paragraphs of the baseline and the current model by
//! id (see `write_changes`). The viewer adds page slides to the model as it
//! lays the document out, so only `flow` is compared, not the slides.

use std::collections::{HashMap, HashSet};

use ibis_doc_model::{
    Block, DropCapLocation, FlowAlign, FlowCell, FlowDocument, FlowParagraph, FlowRow, FlowRunKind, FlowTable, LineHeight, MediaId, ParaFormat,
    PartKind, Presentation, Revision, RevisionKind, Rgb, RunFormat, Shape, ShapeKind, VerticalMerge, VerticalPosition, Link,
};
use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::unsupported;
use crate::Error;

/// Where one position of a paragraph came from. Paths are below the
/// paragraph's `w:p`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AtomSource {
    /// A character, tab, break or picture: made by the child `node` of the
    /// run `run`.
    InRun { run: Vec<u32>, node: Vec<u32> },
    /// A field shown as one position (a page number, a file name): the
    /// elements it takes (its runs, or its `w:fldSimple`).
    Field { nodes: Vec<Vec<u32>> },
}

/// Where a paragraph came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParaSource {
    /// `word/document.xml`, or a header's or footer's part.
    pub part: String,
    /// Element path of the `w:p` in the part.
    pub path: Vec<u32>,
    /// For each position of the paragraph's runs.
    pub atoms: Vec<AtomSource>,
}

/// Where a table came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSource {
    pub part: String,
    pub path: Vec<u32>,
}

/// Where the things of a document's model came from.
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    pub paragraphs: HashMap<u32, ParaSource>,
    pub tables: HashMap<u32, TableSource>,
    /// The main part and the path of its `w:body`.
    pub body_part: String,
    pub body_path: Vec<u32>,
    /// The parts of the default header and footer read.
    pub header_part: Option<String>,
    pub footer_part: Option<String>,
    /// The parts of the sections' own headers and footers: by the paragraph
    /// ending the section (0 for the last section) and [`PartKind::index`].
    pub section_parts: HashMap<(u32, usize), String>,
    /// The footnotes' and the endnotes' parts.
    pub note_parts: [Option<String>; 2],
    /// The comments' parts.
    pub comments_part: Option<String>,
    pub comments_extended_part: Option<String>,
    /// Word 2013 and later (`compatibilityMode` 15): `w:tblInd` is to a
    /// table's left edge (the model's [`FlowTable::indent`]); before, to the
    /// first cell's text.
    pub table_indent_to_edge: bool,
}

// ----- Comparing the models ---------------------------------------------

/// Whether two documents have the same content (the file name, which the
/// viewer sets, aside).
fn same_document(a: &FlowDocument, b: &FlowDocument) -> bool {
    a.page == b.page && a.header == b.header && a.footer == b.footer && a.body == b.body && a.styles == b.styles && last_section(a) == last_section(b)
        && a.footnotes == b.footnotes && a.endnotes == b.endnotes && a.comments == b.comments && a.track_changes == b.track_changes && a.watermark == b.watermark && a.section_watermarks == b.section_watermarks
        && a.page_color == b.page_color && a.properties == b.properties
        && a.mark_as_final == b.mark_as_final && a.read_only_recommended == b.read_only_recommended
        && a.update_fields == b.update_fields && a.note_separators == b.note_separators && a.mail_merge_type == b.mail_merge_type && a.mail_merge_field_map == b.mail_merge_field_map && a.view_mode == b.view_mode
        && a.no_line_start == b.no_line_start && a.no_line_end == b.no_line_end
        && a.protection == b.protection && a.lock_tracking == b.lock_tracking
        && a.formatting_protection == b.formatting_protection
        && a.interaction_mode == b.interaction_mode
        && a.do_not_compress_images == b.do_not_compress_images
        && a.default_image_dpi == b.default_image_dpi
        && a.discard_image_editing_data == b.discard_image_editing_data
        && a.save_frequency == b.save_frequency
        && a.embed_true_type_fonts == b.embed_true_type_fonts
        && a.embed_system_fonts == b.embed_system_fonts
        && a.save_subset_fonts == b.save_subset_fonts
        && a.compatibility_mode == b.compatibility_mode
        && a.use_fe_layout == b.use_fe_layout
        && a.do_not_expand_shift_return == b.do_not_expand_shift_return
        && a.attached_template == b.attached_template
        && a.link_styles == b.link_styles
        && a.strict_first_and_last_chars == b.strict_first_and_last_chars
        && a.theme_font_lang == b.theme_font_lang
        && a.shape_groups == b.shape_groups
}

/// The rest of the last section: its other headers and footers and settings.
#[allow(clippy::type_complexity)]
fn last_section(d: &FlowDocument) -> (&[Block], &[Block], &[Block], &[Block], bool, bool, &Option<String>, &Option<(u32, String)>, bool, [bool; 6]) {
    (&d.header_first, &d.footer_first, &d.header_even, &d.footer_even, d.title_page, d.even_and_odd, &d.page_number_format, &d.page_chapter, d.restart_numbering, d.linked)
}

/// A section's settings its `w:sectPr` holds (besides its headers').
#[derive(Clone, Debug, PartialEq)]
struct SectView {
    page: ibis_doc_model::PageSetup,
    restart: bool,
    format: Option<String>,
    chapter: Option<(u32, String)>,
    title_page: bool,
}

fn last_view(d: &FlowDocument) -> SectView {
    SectView { page: d.page, restart: d.restart_numbering, format: d.page_number_format.clone(), chapter: d.page_chapter.clone(), title_page: d.title_page }
}

fn section_view(s: &ibis_doc_model::SectionSetup) -> SectView {
    SectView { page: s.page, restart: s.restart_numbering, format: s.page_number_format.clone(), chapter: s.page_chapter.clone(), title_page: s.title_page }
}

/// The headers and footers sections have of their own: by the paragraph
/// ending the section (0 for the last) and the kind, in order.
fn own_parts(doc: &FlowDocument) -> Vec<((u32, PartKind), &[Block])> {
    let mut out = Vec::new();
    for b in &doc.body {
        if let Block::Paragraph(p) = b
            && let Some(section) = &p.section
        {
            for kind in PartKind::ALL {
                if let Some(content) = &section.parts[kind.index()] {
                    out.push(((p.id, kind), content.as_slice()));
                }
            }
        }
    }
    let last = [&doc.header, &doc.footer, &doc.header_first, &doc.footer_first, &doc.header_even, &doc.footer_even];
    for kind in PartKind::ALL {
        if !doc.linked[kind.index()] {
            out.push(((0, kind), last[kind.index()].as_slice()));
        }
    }
    out
}

/// The prefixes of DrawingML as a new shape's XML writes them (`a:`).
fn drawing_prefixes() -> crate::save::pptx_new::Prefixes {
    let dummy = XmlDocument::parse(br#"<a:x xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"/>"#).expect("a dummy element");
    let root = dummy.root_element().expect("the root");
    crate::save::pptx_new::Prefixes::at(&dummy, root)
}

/// The text of a shape that is a text box (or any shape with text).
fn shape_body(shape: &Shape) -> Option<&ibis_doc_model::TextBody> {
    match &shape.kind {
        ShapeKind::Auto(a) => a.text.as_ref(),
        ShapeKind::TextBox(body) => Some(body),
        _ => None,
    }
}

/// The text of a `w:txbxContent`, paragraphs joined by `\n`.
/// What a text body's runs look like, paragraph by paragraph: the text of each run of the same look, with its look (bold, italic,
/// underline, underline kind, size in half points, colour).
fn run_looks(body: &ibis_doc_model::TextBody) -> Vec<Vec<(String, bool, bool, bool, ibis_doc_model::Underline, u32, u32)>> {
    body.paragraphs
        .iter()
        .map(|p| {
            let mut out: Vec<(String, bool, bool, bool, ibis_doc_model::Underline, u32, u32)> = Vec::new();
            for r in p.runs.iter().filter(|r| !r.text.is_empty()) {
                let look = (
                    r.style.bold.unwrap_or(body.style.bold),
                    r.style.italic.unwrap_or(body.style.italic),
                    r.style.underline,
                    r.style.underline_kind,
                    (r.style.font_size.unwrap_or(body.style.font_size) + 25) / 50,
                    r.style.color.unwrap_or(body.style.color).0 & 0xffffff,
                );
                match out.last_mut() {
                    Some(last) if (last.1, last.2, last.3, last.4, last.5, last.6) == look => last.0.push_str(&r.text),
                    _ => out.push((r.text.clone(), look.0, look.1, look.2, look.3, look.4, look.5)),
                }
            }
            out
        })
        .collect()
}

fn txbx_text(doc: &XmlDocument, content: NodeId) -> String {
    doc.children_named(content, "p")
        .map(|p| doc.descendants(p).into_iter().filter(|&n| doc.is_element(n) && doc.local_name(n) == "t").map(|n| doc.text(n)).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// One position of a paragraph, compared by what it is.
#[derive(Clone, Debug, PartialEq, Eq)]
enum AtomKey {
    Char(char),
    Tab,
    PositionalTab(ibis_doc_model::TabAlign, bool, bool),
    LineBreak,
    PageBreak,
    ColumnBreak,
    Picture(MediaId, i64, i64, ibis_doc_model::PictureLook),
    Shape(Box<Shape>),
    PageNumber(bool),
    FileName(String, bool),
    PageRef(String, String, bool),
    Field(String, String, bool),
    Ruby(String, String, u32, u32, ibis_doc_model::RubyAlign),
    NoteRef(bool, u32),
    NoteMark,
    CommentStart(u32),
    CommentEnd(u32),
    CommentRef(u32),
    CheckBox(bool, char, char, Option<Box<ibis_doc_model::ControlInfo>>),
    DropDownList(Vec<(String, String)>, usize, bool, Option<Box<ibis_doc_model::ControlInfo>>),
    DatePicker(String, String, String, Option<Box<ibis_doc_model::ControlInfo>>),
    RichText(String, bool, Option<Box<ibis_doc_model::ControlInfo>>),
    Math(ibis_doc_model::math::Math, bool),
    PlainText(String, bool, Option<Box<ibis_doc_model::ControlInfo>>),
    PictureControl(MediaId, i64, i64, ibis_doc_model::PictureLook, bool, Option<Box<ibis_doc_model::ControlInfo>>),
    BuildingBlockGallery(String, String, String, bool, Option<Box<ibis_doc_model::ControlInfo>>),
    RepeatingSection(String, Vec<String>, usize, bool, Option<Box<ibis_doc_model::ControlInfo>>),
    Group(String, bool, Option<Box<ibis_doc_model::ControlInfo>>),
    FormField(ibis_doc_model::FormFieldData),
}

/// The positions of a paragraph: what each is, its run's formatting and
/// the link it is part of.
type Atom<'a> = (AtomKey, &'a RunFormat, &'a Option<String>, &'a Option<Link>, &'a Option<Revision>);

/// How many positions the save tells apart in `p` (what the reader keeps a source node for each of).
pub(crate) fn atom_count(p: &FlowParagraph) -> usize {
    atoms(p).len()
}

fn atoms(p: &FlowParagraph) -> Vec<Atom<'_>> {
    let mut out = Vec::new();
    for run in &p.runs {
        let f = (&run.format, &run.char_style, &run.link, &run.revision);
        match &run.kind {
            FlowRunKind::Text(t) => out.extend(t.chars().map(|c| (AtomKey::Char(c), f.0, f.1, f.2, f.3))),
            FlowRunKind::Tab => out.push((AtomKey::Tab, f.0, f.1, f.2, f.3)),
            FlowRunKind::PositionalTab { align, relative_to_margin, leader } => out.push((AtomKey::PositionalTab(*align, *relative_to_margin, *leader), f.0, f.1, f.2, f.3)),
            FlowRunKind::LineBreak => out.push((AtomKey::LineBreak, f.0, f.1, f.2, f.3)),
            FlowRunKind::PageBreak => out.push((AtomKey::PageBreak, f.0, f.1, f.2, f.3)),
            FlowRunKind::ColumnBreak => out.push((AtomKey::ColumnBreak, f.0, f.1, f.2, f.3)),
            FlowRunKind::Picture { media, size, look } => out.push((AtomKey::Picture(*media, size.width, size.height, look.clone()), f.0, f.1, f.2, f.3)),
            FlowRunKind::PictureControl { media, size, look, placeholder } => out.push((AtomKey::PictureControl(*media, size.width, size.height, look.clone(), *placeholder, run.control.clone()), f.0, f.1, f.2, f.3)),
            FlowRunKind::BuildingBlockGallery { gallery, category, text, placeholder } => out.push((AtomKey::BuildingBlockGallery(gallery.clone(), category.clone(), text.clone(), *placeholder, run.control.clone()), f.0, f.1, f.2, f.3)),
            FlowRunKind::RepeatingSection { section_title, items, selected, do_not_allow_insert_delete } => out.push((AtomKey::RepeatingSection(section_title.clone(), items.clone(), *selected, *do_not_allow_insert_delete, run.control.clone()), f.0, f.1, f.2, f.3)),
            FlowRunKind::Group { text, placeholder } => out.push((AtomKey::Group(text.clone(), *placeholder, run.control.clone()), f.0, f.1, f.2, f.3)),
            FlowRunKind::FormField(ff) => out.push((AtomKey::FormField(ff.clone()), f.0, f.1, f.2, f.3)),
            FlowRunKind::Shape(s) => out.push((AtomKey::Shape(s.clone()), f.0, f.1, f.2, f.3)),
            FlowRunKind::PageNumber => out.push((AtomKey::PageNumber(run.field_locked), f.0, f.1, f.2, f.3)),
            FlowRunKind::FileName(t) => out.push((AtomKey::FileName(t.clone(), run.field_locked), f.0, f.1, f.2, f.3)),
            FlowRunKind::Field { instruction, shown } => out.push((AtomKey::Field(instruction.clone(), shown.clone(), run.field_locked), f.0, f.1, f.2, f.3)),
            FlowRunKind::PageRef { bookmark, shown } => out.push((AtomKey::PageRef(bookmark.clone(), shown.clone(), run.field_locked), f.0, f.1, f.2, f.3)),
            FlowRunKind::NoteRef { endnote, id } => out.push((AtomKey::NoteRef(*endnote, *id), f.0, f.1, f.2, f.3)),
            FlowRunKind::NoteMark => out.push((AtomKey::NoteMark, f.0, f.1, f.2, f.3)),
            FlowRunKind::Ruby { base, text, size, raise, align } => out.push((AtomKey::Ruby(base.clone(), text.clone(), *size, *raise, *align), f.0, f.1, f.2, f.3)),
            FlowRunKind::CommentStart(id) => out.push((AtomKey::CommentStart(*id), f.0, f.1, f.2, f.3)),
            FlowRunKind::CommentEnd(id) => out.push((AtomKey::CommentEnd(*id), f.0, f.1, f.2, f.3)),
            FlowRunKind::CommentRef(id) => out.push((AtomKey::CommentRef(*id), f.0, f.1, f.2, f.3)),
            FlowRunKind::CheckBox { checked, checked_char, unchecked_char } => out.push((AtomKey::CheckBox(*checked, *checked_char, *unchecked_char, run.control.clone()), f.0, f.1, f.2, f.3)),
            FlowRunKind::DropDownList { items, selected, combo } => out.push((AtomKey::DropDownList(items.clone(), *selected, *combo, run.control.clone()), f.0, f.1, f.2, f.3)),
            FlowRunKind::DatePicker { full_date, date_format, text } => out.push((AtomKey::DatePicker(full_date.clone(), date_format.clone(), text.clone(), run.control.clone()), f.0, f.1, f.2, f.3)),
            FlowRunKind::RichText { text, placeholder } => out.push((AtomKey::RichText(text.clone(), *placeholder, run.control.clone()), f.0, f.1, f.2, f.3)),
            FlowRunKind::Math { math, display } => out.push((AtomKey::Math(math.clone(), *display), f.0, f.1, f.2, f.3)),
            FlowRunKind::PlainText { text, placeholder } => out.push((AtomKey::PlainText(text.clone(), *placeholder, run.control.clone()), f.0, f.1, f.2, f.3)),
        }
    }
    out
}

/// For each item of `new`, the item of `old` it is kept from (a longest
/// common subsequence; beyond a size, only the common start and end).
fn matching<T: PartialEq>(old: &[T], new: &[T]) -> Vec<Option<usize>> {
    let mut out = vec![None; new.len()];
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    for (i, slot) in out.iter_mut().enumerate().take(prefix) {
        *slot = Some(i);
    }
    let max_suffix = old.len().min(new.len()) - prefix;
    let suffix = old.iter().rev().zip(new.iter().rev()).take(max_suffix).take_while(|(a, b)| a == b).count();
    for k in 0..suffix {
        out[new.len() - 1 - k] = Some(old.len() - 1 - k);
    }
    let a = &old[prefix..old.len() - suffix];
    let b = &new[prefix..new.len() - suffix];
    if a.is_empty() || b.is_empty() || a.len() * b.len() > 4_000_000 {
        return out;
    }
    // lengths[i][j]: the longest common subsequence of a[i..] and b[j..].
    let w = b.len() + 1;
    let mut lengths = vec![0u32; (a.len() + 1) * w];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lengths[i * w + j] = if a[i] == b[j] { lengths[(i + 1) * w + j + 1] + 1 } else { lengths[(i + 1) * w + j].max(lengths[i * w + j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            out[prefix + j] = Some(prefix + i);
            i += 1;
            j += 1;
        } else if lengths[(i + 1) * w + j] >= lengths[i * w + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

// ----- Writing -----------------------------------------------------------


/// Every paragraph and table of a list of blocks, cells' too, by id.
fn index_blocks<'a>(blocks: &'a [Block], out: &mut HashMap<u32, &'a Block>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                out.insert(p.id, b);
            }
            Block::Table(t) => {
                out.insert(t.id, b);
                for row in &t.rows {
                    for cell in &row.cells {
                        index_blocks(&cell.blocks, out);
                    }
                }
            }
        }
    }
}

fn block_id(b: &Block) -> u32 {
    match b {
        Block::Paragraph(p) => p.id,
        Block::Table(t) => t.id,
    }
}

/// Write theme colors, fonts, and effects to the document's theme part (`word/theme/theme1.xml`).
fn write_theme(package: &mut Package, main: &str, colors: &[(String, ibis_doc_model::Rgb)], fonts: Option<&ibis_doc_model::ThemeFonts>, effects: Option<&str>) -> Result<(), Error> {
    let mut parts = Vec::new();
    if let Ok(rels) = package.relationships(main) {
        for rel in rels.into_iter().filter(|r| !r.external && (r.kind == crate::package::REL_THEME || r.kind.ends_with("/theme"))) {
            if let Some(target) = rel.target_part(main) {
                if package.contains(&target) && !parts.contains(&target) {
                    parts.push(target);
                }
            }
        }
    }
    if parts.is_empty() && package.contains("word/theme/theme1.xml") {
        parts.push("word/theme/theme1.xml".to_string());
    }
    for part in parts {
        let doc = package.xml_mut(&part)?;
        super::pptx_theme::write_theme_elements(doc, colors, fonts);
        if let Some(effects) = effects {
            let _ = super::pptx_theme::write_theme_effects(doc, effects);
        }
    }
    Ok(())
}

pub(crate) fn write_changes(package: &mut Package, map: &SourceMap, baseline: &Presentation, current: &Presentation, new_charts: &HashMap<ibis_doc_model::ShapeId, super::NewChartEntry>, chart_data: &super::pptx_chart_data::ChartDataEdits) -> Result<(), Error> {
    let (base, cur) = match (&baseline.flow, &current.flow) {
        (Some(a), Some(b)) => (a, b),
        (None, None) => return Ok(()),
        _ => return Err(unsupported("文書の変更")),
    };
    let theme_changed = baseline.theme_colors != current.theme_colors
        || baseline.theme_fonts != current.theme_fonts
        || baseline.theme_effects != current.theme_effects;
    if theme_changed {
        write_theme(package, &map.body_part, &current.theme_colors, current.theme_fonts.as_ref(), current.theme_effects.as_deref())?;
    }
    let rethemed_base;
    let base = if baseline.theme_colors != current.theme_colors {
        let mut cloned = (**base).clone();
        cloned.retheme(&current.theme_colors);
        rethemed_base = std::sync::Arc::new(cloned);
        &rethemed_base
    } else {
        base
    };
    if std::sync::Arc::ptr_eq(base, cur) || same_document(base, cur) {
        return Ok(());
    }
    if base.styles != cur.styles {
        let without = |s: &ibis_doc_model::FlowStyles| ibis_doc_model::FlowStyles { numbering: Default::default(), styles: Default::default(), ..s.clone() };
        if without(&base.styles) != without(&cur.styles) {
            return Err(unsupported("文書の既定の書式の変更"));
        }
        if base.styles.styles != cur.styles.styles {
            write_styles(package, map, &base.styles, &cur.styles)?;
        }
        if base.styles.numbering != cur.styles.numbering {
            write_numbering(package, map, &base.styles.numbering, &cur.styles.numbering, current)?;
        }
    }
    if base.properties != cur.properties || base.mark_as_final != cur.mark_as_final {
        write_core_properties(package, &cur.properties)?;
        write_app_properties(package, &cur.properties)?;
        write_custom_properties(package, &cur.properties.custom, cur.mark_as_final)?;
    }
    let old_owns = own_parts(base);
    let new_owns = own_parts(cur);
    let mut old_blocks = HashMap::new();
    index_blocks(&base.body, &mut old_blocks);
    for (_, blocks) in &old_owns {
        index_blocks(blocks, &mut old_blocks);
    }
    let mut new_blocks = HashMap::new();
    index_blocks(&cur.body, &mut new_blocks);
    for (_, blocks) in &new_owns {
        index_blocks(blocks, &mut new_blocks);
    }
    // Pictures of the file, to copy one pasted elsewhere.
    let mut pictures: HashMap<MediaId, (u32, usize)> = HashMap::new();
    for (&id, block) in &old_blocks {
        if let Block::Paragraph(p) = block {
            for (i, (key, _, _, _, _)) in atoms(p).into_iter().enumerate() {
                match key {
                    AtomKey::Picture(media, ..) | AtomKey::PictureControl(media, ..) => {
                        pictures.entry(media).or_insert((id, i));
                    }
                    AtomKey::Shape(shape) => {
                        if let ShapeKind::Picture(pic) = &shape.kind {
                            pictures.entry(pic.media).or_insert((id, i));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // The part each header and footer is in: the one its paragraphs were
    // read from, else a new one.
    let part_of_blocks = |blocks: &[Block]| -> Option<String> {
        blocks.iter().find_map(|b| match b {
            Block::Paragraph(p) => map.paragraphs.get(&p.id).map(|s| s.part.clone()),
            Block::Table(t) => map.tables.get(&t.id).map(|s| s.part.clone()),
        })
        .filter(|p| *p != map.body_part)
    };
    let mut old_by_part: HashMap<String, &[Block]> = HashMap::new();
    for ((section, kind), blocks) in &old_owns {
        if let Some(part) = map.section_parts.get(&(*section, kind.index())).cloned().or_else(|| part_of_blocks(blocks)) {
            old_by_part.insert(part, blocks);
        }
    }
    let mut part_of: HashMap<(u32, PartKind), String> = HashMap::new();
    let mut created: HashSet<String> = HashSet::new();
    let mut used: HashSet<String> = HashSet::new();
    for ((section, kind), blocks) in &new_owns {
        let part = match part_of_blocks(blocks).filter(|p| old_by_part.contains_key(p) && !used.contains(p)) {
            Some(part) => part,
            None => {
                let part = new_part(package, map, kind.is_footer())?;
                created.insert(part.clone());
                part
            }
        };
        used.insert(part.clone());
        part_of.insert((*section, *kind), part);
    }
    let body_rels = package.relationships(&map.body_part)?;
    let rid_of = |part: &str| -> Option<String> { body_rels.iter().find(|r| !r.external && r.target_part(&map.body_part).as_deref() == Some(part)).map(|r| r.id.clone()) };
    // Each section's references, in order: its own parts' relationships,
    // or none (the previous section's).
    let mut wanted: Vec<(u32, [Option<String>; 6])> = Vec::new();
    for id in ibis_doc_model::flow_command::section_paragraphs(cur).into_iter().chain([0]) {
        let refs: [Option<String>; 6] = std::array::from_fn(|k| part_of.get(&(id, PartKind::ALL[k])).and_then(|p| rid_of(p)));
        wanted.push((id, refs));
    }

    let mut lists: Vec<(String, &[Block], &[Block], bool)> = vec![(map.body_part.clone(), base.body.as_slice(), cur.body.as_slice(), false)];
    for (key, blocks) in &new_owns {
        let part = &part_of[key];
        let old: &[Block] = old_by_part.get(part).copied().unwrap_or(&[]);
        lists.push((part.clone(), old, blocks, created.contains(part)));
    }
    let sections_changed = last_view(base) != last_view(cur) || base.linked != cur.linked || base.even_and_odd != cur.even_and_odd || !created.is_empty();
    let shape_groups_changed = base.shape_groups != cur.shape_groups;
    for (part, old, new, fresh) in lists {
        let body = part == map.body_part;
        if old == new && !(body && (sections_changed || shape_groups_changed)) {
            continue;
        }
        // Pictures new to this part: their media parts and relationships.
        let mut in_old = HashSet::new();
        pictures_in(old, &mut in_old);
        let mut in_new = HashSet::new();
        pictures_in(new, &mut in_new);
        let mut added: HashMap<MediaId, String> = HashMap::new();
        let mut svg_media: HashSet<MediaId> = HashSet::new();
        for media in in_new.difference(&in_old) {
            if pictures.get(media).is_some_and(|(para, _)| map.paragraphs.get(para).is_some_and(|s| s.part == part)) {
                continue; // a copy of a picture of this part: its element is copied
            }
            let m = current.media.iter().chain(&baseline.media).find(|m| m.id == *media).ok_or_else(|| unsupported("元の無い画像"))?;
            let rid = add_media(package, &part, m)?;
            if m.format == ibis_doc_model::MediaFormat::Svg {
                svg_media.insert(*media);
            }
            added.insert(*media, rid);
        }
        // The links' addresses: the part's relationship to each (made for
        // an address it has none to).
        let mut urls = HashSet::new();
        links_in(new, &mut urls);
        let mut link_rels: HashMap<String, String> = HashMap::new();
        if !urls.is_empty() {
            let rels = package.relationships(&part)?;
            for url in urls {
                let rid = match rels.iter().find(|r| r.external && r.kind == HYPERLINK_REL && r.target == url) {
                    Some(r) => r.id.clone(),
                    None => package.add_relationship(&part, HYPERLINK_REL, &url, true)?,
                };
                link_rels.insert(url, rid);
            }
        }
        let mut subdoc_targets = HashSet::new();
        subdocs_in(new, &mut subdoc_targets);
        if !subdoc_targets.is_empty() {
            let rels = package.relationships(&part)?;
            for target in subdoc_targets {
                let rid = match rels.iter().find(|r| r.external && r.kind == SUBDOC_REL && r.target == target) {
                    Some(r) => r.id.clone(),
                    None => package.add_relationship(&part, SUBDOC_REL, &target, true)?,
                };
                link_rels.insert(format!("subdoc:{target}"), rid);
            }
        }
        // New charts (挿入 > グラフ): their parts from the template, then the changes made to them since.
        let mut old_charts = Vec::new();
        charts_in(old, &mut old_charts);
        let mut new_chart_shapes = Vec::new();
        charts_in(new, &mut new_chart_shapes);
        let mut chart_rels: HashMap<u64, String> = HashMap::new();
        for (id, after) in new_chart_shapes.iter().filter(|(id, _)| !old_charts.iter().any(|(o, _)| o == id)) {
            let entry = new_charts.get(id).ok_or_else(|| unsupported("グラフの複製・貼り付け"))?;
            let (rel, chart_part) = super::pptx_new_chart::add_chart_part_at(package, &part, entry.kind, "word/charts/chart1.xml", "word/embeddings/Microsoft_Excel_Worksheet1.xlsx")?;
            if let Some(template) = &entry.template {
                super::pptx_shapes::write_chart_changes(package, &chart_part, template, after, super::pptx_chart_data::matching(chart_data, *id, after))?;
            }
            chart_rels.insert(id.0, rel);
        }
        // The file's own charts, changed (データの編集, a look or a kind changed): their chart part and workbook.
        for (id, after) in &new_chart_shapes {
            if let Some((_, before)) = old_charts.iter().find(|(o, _)| o == id) && before != after {
                let chart_part = existing_chart_part(package, map, old, *id)?;
                super::pptx_shapes::write_chart_changes(package, &chart_part, before, after, super::pptx_chart_data::matching(chart_data, *id, after))?;
            }
        }
        NEW_CHART_RELS.with(|r| *r.borrow_mut() = chart_rels);
        let pristine = package.xml(&part)?;
        let doc = package.xml_mut(&part)?;
        let root = doc.root_element();
        let mut writer = Writer { doc, pristine: &pristine, map, part: &part, old_blocks: &old_blocks, new_blocks: &new_blocks, pictures: &pictures, nodes: HashMap::new(), added: &added, svg_media: &svg_media, empty_parent: if fresh { root } else { None }, link_rels: &link_rels, written: HashMap::new(), shape_nodes: HashMap::new(), shape_olds: HashMap::new(), old_shape_groups: &base.shape_groups, new_shape_groups: &cur.shape_groups };
        writer.resolve_nodes();
        writer.list(old, new)?;
        if body {
            writer.sections(base, cur, &wanted)?;
        }
    }
    if base.watermark != cur.watermark || base.section_watermarks != cur.section_watermarks {
        let mut parts: Vec<&String> = part_of.iter().filter(|((_, k), _)| !k.is_footer()).map(|(_, p)| p).collect();
        parts.sort();
        parts.dedup();
        if cur.watermark.is_some() && parts.is_empty() {
            return Err(unsupported("ヘッダーの無い文書の透かし"));
        }
        let page = &cur.page;
        let pt = |e: i64| e as f32 / 12_700.0;
        let area = (pt(page.size.width - page.left - page.right - page.gutter), pt(page.size.height - page.top - page.bottom));
        for (i, part) in parts.into_iter().enumerate() {
            write_watermark(package, part, cur.watermark.as_ref(), area, i)?;
        }
    }
    if base.even_and_odd != cur.even_and_odd {
        write_setting(package, map, "evenAndOddHeaders", cur.even_and_odd)?;
    }
    if base.track_changes != cur.track_changes {
        write_setting(package, map, "trackRevisions", cur.track_changes)?;
    }
    if base.protection != cur.protection || base.lock_tracking != cur.lock_tracking || base.formatting_protection != cur.formatting_protection {
        write_document_protection(package, map, cur.protection, cur.formatting_protection)?;
        if cur.protection == ibis_doc_model::DocumentProtection::TrackedChanges && !cur.track_changes {
            write_setting(package, map, "trackRevisions", true)?;
        }
    }
    if base.page.multiple_pages != cur.page.multiple_pages
        || base.page.mirror_margins != cur.page.mirror_margins
        || base.page.book_fold_sheets != cur.page.book_fold_sheets
    {
        use ibis_doc_model::MultiplePages;
        let mp = if cur.page.multiple_pages != base.page.multiple_pages {
            cur.page.multiple_pages
        } else if cur.page.mirror_margins {
            MultiplePages::MirrorMargins
        } else {
            MultiplePages::Normal
        };
        write_setting(package, map, "mirrorMargins", mp == MultiplePages::MirrorMargins)?;
        write_setting(package, map, "bookFoldPrinting", mp == MultiplePages::BookFold)?;
        write_setting(package, map, "bookFoldRevPrinting", mp == MultiplePages::ReverseBookFold)?;
        write_setting(package, map, "printTwoOnOne", mp == MultiplePages::TwoOnOne)?;
        let sheets = cur.page.book_fold_sheets.filter(|_| mp.is_book_fold());
        write_setting_decimal(package, map, "bookFoldPrintingSheets", sheets)?;
    }
    if base.page.gutter_at_top != cur.page.gutter_at_top {
        write_setting(package, map, "gutterAtTop", cur.page.gutter_at_top)?;
    }
    if base.read_only_recommended != cur.read_only_recommended {
        write_setting(package, map, "readOnlyRecommended", cur.read_only_recommended)?;
    }
    if base.update_fields != cur.update_fields {
        write_setting(package, map, "updateFields", cur.update_fields)?;
    }
    for (i, endnote) in [false, true].into_iter().enumerate() {
        if base.note_separators[i] != cur.note_separators[i] {
            write_note_separator(package, map, endnote, cur.note_separators[i])?;
        }
    }
    if base.mail_merge_type != cur.mail_merge_type {
        write_mail_merge_type(package, map, cur.mail_merge_type.as_deref())?;
    }
    if base.mail_merge_field_map != cur.mail_merge_field_map {
        write_mail_merge_field_map(package, map, &cur.mail_merge_field_map)?;
    }
    if base.strict_first_and_last_chars != cur.strict_first_and_last_chars {
        write_setting(package, map, "strictFirstAndLastChars", cur.strict_first_and_last_chars)?;
    }
    if base.theme_font_lang != cur.theme_font_lang {
        write_theme_font_lang(package, map, &cur.theme_font_lang)?;
    }
    if base.no_line_start != cur.no_line_start {
        write_kinsoku_setting(package, map, "noLineBreaksBefore", cur.no_line_start.as_deref())?;
    }
    if base.no_line_end != cur.no_line_end {
        write_kinsoku_setting(package, map, "noLineBreaksAfter", cur.no_line_end.as_deref())?;
    }
    if base.view_mode != cur.view_mode {
        write_view_setting(package, map, cur.view_mode)?;
    }
    if base.doc_variables != cur.doc_variables {
        write_doc_variables(package, map, &cur.doc_variables)?;
    }
    if base.page_color != cur.page_color {
        write_page_color(package, map, cur.page_color)?;
    }
    if base.do_not_compress_images != cur.do_not_compress_images {
        write_setting(package, map, "doNotAutoCompressPictures", cur.do_not_compress_images)?;
    }
    if base.default_image_dpi != cur.default_image_dpi || base.discard_image_editing_data != cur.discard_image_editing_data {
        write_image_quality_settings(package, map, cur.default_image_dpi, cur.discard_image_editing_data)?;
    }
    if base.save_frequency != cur.save_frequency {
        write_save_frequency(package, map, cur.save_frequency)?;
    }
    if base.embed_true_type_fonts != cur.embed_true_type_fonts {
        write_setting(package, map, "embedTrueTypeFonts", cur.embed_true_type_fonts)?;
    }
    if base.embed_system_fonts != cur.embed_system_fonts {
        write_setting(package, map, "embedSystemFonts", cur.embed_system_fonts)?;
    }
    if base.save_subset_fonts != cur.save_subset_fonts {
        write_setting(package, map, "saveSubsetFonts", cur.save_subset_fonts)?;
    }
    if base.compatibility_mode != cur.compatibility_mode || base.use_fe_layout != cur.use_fe_layout || base.do_not_expand_shift_return != cur.do_not_expand_shift_return {
        write_compatibility(package, map, cur.compatibility_mode, cur.use_fe_layout, cur.do_not_expand_shift_return)?;
    }
    if base.attached_template != cur.attached_template {
        write_attached_template(package, map, cur.attached_template.as_deref())?;
    }
    if base.link_styles != cur.link_styles {
        write_setting(package, map, "linkStyles", cur.link_styles)?;
    }
    // The footnotes and endnotes.
    for (i, (old, new)) in [(&base.footnotes, &cur.footnotes), (&base.endnotes, &cur.endnotes)].into_iter().enumerate() {
        if old == new {
            continue;
        }
        let endnote = i == 1;
        let part = match &map.note_parts[i] {
            Some(p) => p.clone(),
            None => new_notes_part(package, map, endnote)?,
        };
        let mut old_notes = HashMap::new();
        for n in old {
            index_blocks(&n.blocks, &mut old_notes);
        }
        let mut new_notes = HashMap::new();
        for n in new {
            index_blocks(&n.blocks, &mut new_notes);
        }
        let added = HashMap::new();
        let svg_media = HashSet::new();
        let mut urls = HashSet::new();
        for note in new { links_in(&note.blocks, &mut urls); }
        let mut link_rels = HashMap::new();
        if !urls.is_empty() {
            let rels = package.relationships(&part)?;
            for url in urls {
                let rid = match rels.iter().find(|r| r.external && r.kind == HYPERLINK_REL && r.target == url) {
                    Some(r) => r.id.clone(),
                    None => package.add_relationship(&part, HYPERLINK_REL, &url, true)?,
                };
                link_rels.insert(url, rid);
            }
        }
        let pristine = package.xml(&part)?;
        let doc = package.xml_mut(&part)?;
        let root = doc.root_element().ok_or_else(|| unsupported("空の脚注のパーツ"))?;
        let element = if endnote { "endnote" } else { "footnote" };
        let mut writer = Writer { doc, pristine: &pristine, map, part: &part, old_blocks: &old_notes, new_blocks: &new_notes, pictures: &pictures, nodes: HashMap::new(), added: &added, svg_media: &svg_media, empty_parent: None, link_rels: &link_rels, written: HashMap::new(), shape_nodes: HashMap::new(), shape_olds: HashMap::new(), old_shape_groups: &[], new_shape_groups: &[] };
        writer.resolve_nodes();
        let find = |w: &Writer, id: u32| {
            w.doc.elements(root).find(|&e| {
                w.doc.namespace(e) == Some(W_NS)
                    && w.doc.local_name(e) == element
                    && word_attr_name(w.doc, e, "id").and_then(|a| w.doc.attr(e, a)) == Some(id.to_string().as_str())
            })
        };
        for note in old {
            if !new.iter().any(|n| n.id == note.id)
                && let Some(e) = find(&writer, note.id)
            {
                writer.doc.detach(e);
            }
        }
        for note in new {
            let was = old.iter().find(|n| n.id == note.id);
            if was == Some(note) {
                continue;
            }
            let e = match find(&writer, note.id) {
                Some(e) if was.is_some() => e,
                _ => {
                    let e = writer.doc.append_element(root, &format!("w:{element}"));
                    writer.doc.set_attr(e, "w:id", &note.id.to_string());
                    e
                }
            };
            writer.empty_parent = Some(e);
            writer.list(was.map_or(&[][..], |w| w.blocks.as_slice()), &note.blocks)?;
        }
    }
    if base.comments != cur.comments {
        write_comments(package, map, &base.comments, &cur.comments, &pictures)?;
    }
    Ok(())
}

const W14_NS: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const W15_NS: &str = "http://schemas.microsoft.com/office/word/2012/wordml";

/// The comments (`comments.xml`, made if missing) and which answers which
/// and which are resolved (`commentsExtended.xml`, written again).
fn write_comments(package: &mut Package, map: &SourceMap, old: &[ibis_doc_model::Comment], new: &[ibis_doc_model::Comment], pictures: &HashMap<MediaId, (u32, usize)>) -> Result<(), Error> {
    let dir = map.body_part.rsplit_once('/').map_or("", |(d, _)| d);
    let in_dir = |name: &str| if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") };
    let part = match &map.comments_part {
        Some(p) => p.clone(),
        None => {
            let part = in_dir("comments.xml");
            let mut doc = XmlDocument::new_with_declaration();
            let root = doc.append_element(XmlDocument::ROOT, "w:comments");
            doc.set_attr(root, "xmlns:w", W_NS);
            doc.set_attr(root, "xmlns:r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships");
            doc.set_attr(root, "xmlns:w14", W14_NS);
            package.set_xml(&part, doc);
            package.set_override(&part, "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml")?;
            package.add_relationship(&map.body_part, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments", "comments.xml", false)?;
            part
        }
    };
    let mut old_blocks = HashMap::new();
    for c in old {
        index_blocks(&c.blocks, &mut old_blocks);
    }
    let mut new_blocks = HashMap::new();
    for c in new {
        index_blocks(&c.blocks, &mut new_blocks);
    }
    {
        let added = HashMap::new();
        let svg_media = HashSet::new();
        let link_rels = HashMap::new();
        let pristine = package.xml(&part)?;
        let doc = package.xml_mut(&part)?;
        let root = doc.root_element().ok_or_else(|| unsupported("空の comments.xml"))?;
        if doc.lookup_prefix(root, W14_NS).is_none() {
            doc.set_attr(root, "xmlns:w14", W14_NS);
        }
        let w14 = doc.lookup_prefix(root, W14_NS).unwrap_or_else(|| "w14".into());
        let mut writer = Writer { doc, pristine: &pristine, map, part: &part, old_blocks: &old_blocks, new_blocks: &new_blocks, pictures, nodes: HashMap::new(), added: &added, svg_media: &svg_media, empty_parent: None, link_rels: &link_rels, written: HashMap::new(), shape_nodes: HashMap::new(), shape_olds: HashMap::new(), old_shape_groups: &[], new_shape_groups: &[] };
        writer.resolve_nodes();
        let find = |w: &Writer, id: u32| w.doc.elements(root).find(|&e| w.doc.local_name(e) == "comment" && w.doc.attr_local(e, "id") == Some(id.to_string().as_str()));
        for c in old {
            if !new.iter().any(|n| n.id == c.id)
                && let Some(e) = find(&writer, c.id)
            {
                writer.doc.detach(e);
            }
        }
        for c in new {
            let was = old.iter().find(|o| o.id == c.id);
            if was == Some(c) {
                continue;
            }
            let e = match find(&writer, c.id) {
                Some(e) if was.is_some() => e,
                _ => {
                    let e = writer.doc.append_element(root, "w:comment");
                    writer.doc.set_attr(e, "w:id", &c.id.to_string());
                    writer.doc.set_attr(e, "w:author", &c.author);
                    writer.doc.set_attr(e, "w:date", &c.date);
                    writer.doc.set_attr(e, "w:initials", &c.initials);
                    e
                }
            };
            writer.empty_parent = Some(e);
            writer.list(was.map_or(&[][..], |w| w.blocks.as_slice()), &c.blocks)?;
            // Its last paragraph's id, which commentsExtended.xml names.
            if let Some(id) = &c.para_id
                && let Some(last) = writer.doc.elements(e).filter(|&n| writer.doc.local_name(n) == "p").last()
                && writer.doc.attr_local(last, "paraId") != Some(id.as_str())
            {
                writer.doc.set_attr(last, &format!("{w14}:paraId"), id);
            }
        }
    }
    // commentsExtended.xml, from the comments now.
    let ext = |cs: &[ibis_doc_model::Comment]| -> Vec<(Option<String>, Option<String>, bool)> {
        cs.iter().map(|c| (c.para_id.clone(), c.parent.and_then(|p| cs.iter().find(|x| x.id == p)).and_then(|p| p.para_id.clone()), c.done)).collect()
    };
    if ext(old) != ext(new) {
        let part = match &map.comments_extended_part {
            Some(p) => p.clone(),
            None => {
                let part = in_dir("commentsExtended.xml");
                package.set_override(&part, "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml")?;
                package.add_relationship(&map.body_part, "http://schemas.microsoft.com/office/2011/relationships/commentsExtended", "commentsExtended.xml", false)?;
                part
            }
        };
        let mut doc = XmlDocument::new_with_declaration();
        let root = doc.append_element(XmlDocument::ROOT, "w15:commentsEx");
        doc.set_attr(root, "xmlns:w15", W15_NS);
        for (para, parent, done) in ext(new) {
            let Some(para) = para else { continue };
            let e = doc.append_element(root, "w15:commentEx");
            doc.set_attr(e, "w15:paraId", &para);
            if let Some(p) = parent {
                doc.set_attr(e, "w15:paraIdParent", &p);
            }
            doc.set_attr(e, "w15:done", if done { "1" } else { "0" });
        }
        package.set_xml(&part, doc);
    }
    Ok(())
}

/// A new footnotes (endnotes) part with Word's separators, its content
/// type, the body's relationship to it, and the settings' reference to the
/// separators.
fn new_notes_part(package: &mut Package, map: &SourceMap, endnote: bool) -> Result<String, Error> {
    let (kind, element) = if endnote { ("endnotes", "endnote") } else { ("footnotes", "footnote") };
    let dir = map.body_part.rsplit_once('/').map_or("", |(d, _)| d);
    let part = if dir.is_empty() { format!("{kind}.xml") } else { format!("{dir}/{kind}.xml") };
    let mut doc = XmlDocument::new_with_declaration();
    let root = doc.append_element(XmlDocument::ROOT, &format!("w:{kind}"));
    doc.set_attr(root, "xmlns:w", W_NS);
    doc.set_attr(root, "xmlns:r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships");
    for (id, kind_attr, mark) in [("-1", "separator", "separator"), ("0", "continuationSeparator", "continuationSeparator")] {
        let e = doc.append_element(root, &format!("w:{element}"));
        doc.set_attr(e, "w:type", kind_attr);
        doc.set_attr(e, "w:id", id);
        let p = doc.append_element(e, "w:p");
        let ppr = doc.append_element(p, "w:pPr");
        let sp = doc.append_element(ppr, "w:spacing");
        doc.set_attr(sp, "w:after", "0");
        doc.set_attr(sp, "w:line", "240");
        doc.set_attr(sp, "w:lineRule", "auto");
        let r = doc.append_element(p, "w:r");
        doc.append_element(r, &format!("w:{mark}"));
    }
    package.set_xml(&part, doc);
    package.set_override(&part, &format!("application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml"))?;
    package.add_relationship(&map.body_part, &format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}"), &format!("{kind}.xml"), false)?;
    // settings.xml: w:footnotePr (w:endnotePr) naming the separators.
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    if let Some(settings) = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)) {
        let doc = package.xml_mut(&settings)?;
        if let Some(root) = doc.root_element() {
            let pr_name = if endnote { "endnotePr" } else { "footnotePr" };
            if doc.elements(root).all(|c| doc.local_name(c) != pr_name) {
                const AFTER: &[&str] = &["endnotePr", "compat", "docVars", "rsids", "mathPr", "attachedSchema", "themeFontLang", "clrSchemeMapping", "doNotIncludeSubdocsInStats", "doNotAutoCompressPictures", "forceUpgrade", "captions", "readModeInkLockDown", "smartTagType", "schemaLibrary", "shapeDefaults", "doNotEmbedSmartTags", "decimalSymbol", "listSeparator"];
                let after: &[&str] = if endnote { &AFTER[1..] } else { AFTER };
                let before = doc.elements(root).find(|&c| after.contains(&doc.local_name(c)));
                let pr = doc.append_element(root, &format!("w:{pr_name}"));
                doc.detach(pr);
                for id in ["-1", "0"] {
                    let e = doc.append_element(pr, &format!("w:{element}"));
                    doc.set_attr(e, "w:id", id);
                }
                match before {
                    Some(b) => doc.insert_before(b, pr),
                    None => {
                        let end = doc.children(root).len();
                        doc.insert(root, end, pr);
                    }
                }
            }
        }
    }
    Ok(part)
}

/// A new header or footer part (empty, its paragraphs written later), with
/// its content type and the body's relationship to it.
fn new_part(package: &mut Package, map: &SourceMap, footer: bool) -> Result<String, Error> {
    let (kind, root_name) = if footer { ("footer", "w:ftr") } else { ("header", "w:hdr") };
    let dir = map.body_part.rsplit_once('/').map_or("", |(d, _)| d);
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    let n = (1..).find(|n| !package.contains(&format!("{prefix}{kind}{n}.xml"))).expect("a free name");
    let file = format!("{kind}{n}.xml");
    let part = format!("{prefix}{file}");
    let mut doc = XmlDocument::new_with_declaration();
    let root = doc.append_element(XmlDocument::ROOT, root_name);
    doc.set_attr(root, "xmlns:w", W_NS);
    doc.set_attr(root, "xmlns:r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships");
    package.set_xml(&part, doc);
    package.set_override(&part, &format!("application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml"))?;
    package.add_relationship(&map.body_part, &format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}"), &file, false)?;
    Ok(part)
}

/// Set an on/off setting of `settings.xml` (`w:evenAndOddHeaders`).
fn write_setting(package: &mut Package, map: &SourceMap, name: &str, on: bool) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("settings.xml の無い文書の設定の変更"))?;
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;
    // In CT_Settings, evenAndOddHeaders comes after these.
    const BEFORE: &[&str] = &[
        "writeProtection", "readOnlyRecommended", "view", "zoom", "removePersonalInformation", "removeDateAndTime", "doNotDisplayPageBoundaries", "displayBackgroundShape", "printPostScriptOverText", "printFractionalCharacterWidth", "printFormsData", "embedTrueTypeFonts", "embedSystemFonts", "saveSubsetFonts", "saveFormsData", "saveFrequency", "mirrorMargins", "alignBordersAndEdges", "bordersDoNotSurroundHeader", "bordersDoNotSurroundFooter", "gutterAtTop", "hideSpellingErrors", "hideGrammaticalErrors", "activeWritingStyle", "proofState", "formsDesign", "attachedTemplate", "linkStyles", "stylePaneFormatFilter", "stylePaneSortMethod", "documentType", "mailMerge", "revisionView", "trackRevisions", "doNotTrackMoves", "doNotTrackFormatting", "documentProtection", "autoFormatOverride", "styleLockTheme", "styleLockQFSet", "defaultTabStop", "autoHyphenation", "consecutiveHyphenLimit", "hyphenationZone", "doNotHyphenateCaps", "showEnvelope", "summaryLength", "clickAndTypeStyle", "defaultTableStyle", "evenAndOddHeaders", "bookFoldRevPrinting", "bookFoldPrinting", "bookFoldPrintingSheets", "drawingGridHorizontalSpacing", "drawingGridVerticalSpacing", "displayHorizontalDrawingGridEvery", "displayVerticalDrawingGridEvery", "doNotUseMarginsForDrawingGridOrigin", "drawingGridHorizontalOrigin", "drawingGridVerticalOrigin", "doNotShadeFormData", "noPunctuationKerning", "characterSpacingControl", "printTwoOnOne", "strictFirstAndLastChars", "noLineBreaksAfter", "noLineBreaksBefore", "saveThroughXslt", "showXMLTags", "alwaysShowPlaceholderText", "doNotAutoCompressPictures", "updateFields",
        "footnotePr", "endnotePr", "compat", "docVars", "rsids", "mathPr", "attachedSchema", "themeFontLang", "clrSchemeMapping", "doNotIncludeSubdocsInStats",
    ];
    // What comes before the setting in CT_Settings.
    let before: &[&str] = match BEFORE.iter().position(|n| *n == name) {
        Some(i) => &BEFORE[..i],
        None => BEFORE,
    };
    let existing = doc.elements(root).find(|&c| doc.local_name(c) == name);
    match (existing, on) {
        (Some(e), false) => doc.detach(e),
        (Some(e), true) => doc.remove_attr(e, "w:val"),
        (None, true) => {
            let e = doc.append_element(root, &format!("w:{name}"));
            doc.detach(e);
            let after = doc.elements(root).filter(|&c| before.contains(&doc.local_name(c))).last();
            match after {
                Some(a) => doc.insert_after(a, e),
                None => doc.insert(root, 0, e),
            }
        }
        (None, false) => {}
    }
    Ok(())
}

/// Set a decimal number setting of `settings.xml` (`w:bookFoldPrintingSheets`).
fn write_setting_decimal(package: &mut Package, map: &SourceMap, name: &str, val: Option<u32>) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("settings.xml の無い文書の設定の変更"))?;
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;
    const BEFORE: &[&str] = &[
        "writeProtection", "readOnlyRecommended", "view", "zoom", "removePersonalInformation", "removeDateAndTime", "doNotDisplayPageBoundaries", "displayBackgroundShape", "printPostScriptOverText", "printFractionalCharacterWidth", "printFormsData", "embedTrueTypeFonts", "embedSystemFonts", "saveSubsetFonts", "saveFormsData", "saveFrequency", "mirrorMargins", "alignBordersAndEdges", "bordersDoNotSurroundHeader", "bordersDoNotSurroundFooter", "gutterAtTop", "hideSpellingErrors", "hideGrammaticalErrors", "activeWritingStyle", "proofState", "formsDesign", "attachedTemplate", "linkStyles", "stylePaneFormatFilter", "stylePaneSortMethod", "documentType", "mailMerge", "revisionView", "trackRevisions", "doNotTrackMoves", "doNotTrackFormatting", "documentProtection", "autoFormatOverride", "styleLockTheme", "styleLockQFSet", "defaultTabStop", "autoHyphenation", "consecutiveHyphenLimit", "hyphenationZone", "doNotHyphenateCaps", "showEnvelope", "summaryLength", "clickAndTypeStyle", "defaultTableStyle", "evenAndOddHeaders", "bookFoldRevPrinting", "bookFoldPrinting", "bookFoldPrintingSheets", "drawingGridHorizontalSpacing", "drawingGridVerticalSpacing", "displayHorizontalDrawingGridEvery", "displayVerticalDrawingGridEvery", "doNotUseMarginsForDrawingGridOrigin", "drawingGridHorizontalOrigin", "drawingGridVerticalOrigin", "doNotShadeFormData", "noPunctuationKerning", "characterSpacingControl", "printTwoOnOne", "strictFirstAndLastChars", "noLineBreaksAfter", "noLineBreaksBefore", "saveThroughXslt", "showXMLTags", "alwaysShowPlaceholderText", "doNotAutoCompressPictures", "updateFields",
        "footnotePr", "endnotePr", "compat", "docVars", "rsids", "mathPr", "attachedSchema", "themeFontLang", "clrSchemeMapping", "doNotIncludeSubdocsInStats",
    ];
    let before: &[&str] = match BEFORE.iter().position(|n| *n == name) {
        Some(i) => &BEFORE[..i],
        None => BEFORE,
    };
    let existing = doc.elements(root).find(|&c| doc.local_name(c) == name);
    match (existing, val) {
        (Some(e), None) => doc.detach(e),
        (Some(e), Some(v)) => doc.set_attr(e, "w:val", &v.to_string()),
        (None, Some(v)) => {
            let e = doc.append_element(root, &format!("w:{name}"));
            doc.set_attr(e, "w:val", &v.to_string());
            doc.detach(e);
            let after = doc.elements(root).filter(|&c| before.contains(&doc.local_name(c))).last();
            match after {
                Some(a) => doc.insert_after(a, e),
                None => doc.insert(root, 0, e),
            }
        }
        (None, None) => {}
    }
    Ok(())
}

/// The line above the footnotes (or the endnotes) as `kind` says: the content of the note of the type `separator` (a mark of the short line,
/// one of the line across the text, or none).
fn write_note_separator(package: &mut Package, map: &SourceMap, endnote: bool, kind: ibis_doc_model::NoteSeparator) -> Result<(), Error> {
    let part = match &map.note_parts[usize::from(endnote)] {
        Some(p) => p.clone(),
        None => new_notes_part(package, map, endnote)?,
    };
    let element = if endnote { "endnote" } else { "footnote" };
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の脚注のパート"))?;
    let note = doc.elements(root).find(|&n| doc.local_name(n) == element && doc.attr(n, "w:type") == Some("separator")).ok_or_else(|| unsupported("注の区切り線の無い脚注のパート"))?;
    let found = doc.elements(note).find(|&c| doc.local_name(c) == "p");
    let paragraph = match found {
        Some(p) => p,
        None => doc.append_element(note, "w:p"),
    };
    // Everything in the paragraph but its properties goes; the mark of the line is put in a run.
    for child in doc.children(paragraph).to_vec() {
        if !(doc.is_element(child) && doc.local_name(child) == "pPr") {
            doc.detach(child);
        }
    }
    let mark = match kind {
        ibis_doc_model::NoteSeparator::Standard => Some("w:separator"),
        ibis_doc_model::NoteSeparator::Full => Some("w:continuationSeparator"),
        ibis_doc_model::NoteSeparator::Hidden => None,
    };
    if let Some(mark) = mark {
        let r = doc.append_element(paragraph, "w:r");
        doc.append_element(r, mark);
    }
    Ok(())
}

/// Set (`Some`) or take away (`None`) the kind of the mail merge's main document in `settings.xml`
/// (`w:mailMerge/w:mainDocumentType`, ECMA-376 Part 1 §17.15.1.51, §17.15.1.52). The merge's other settings stay as they are.
fn write_mail_merge_type(package: &mut Package, map: &SourceMap, kind: Option<&str>) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("settings.xml の無い文書の設定の変更"))?;
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;
    // What comes before `mailMerge` in CT_Settings.
    const BEFORE: &[&str] = &["writeProtection", "readOnlyRecommended", "view", "zoom", "removePersonalInformation", "removeDateAndTime", "doNotDisplayPageBoundaries", "displayBackgroundShape", "printPostScriptOverText", "printFractionalCharacterWidth", "printFormsData", "embedTrueTypeFonts", "embedSystemFonts", "saveSubsetFonts", "saveFormsData", "mirrorMargins", "alignBordersAndEdges", "bordersDoNotSurroundHeader", "bordersDoNotSurroundFooter", "gutterAtTop", "hideSpellingErrors", "hideGrammaticalErrors", "activeWritingStyle", "proofState", "formsDesign", "attachedTemplate", "linkStyles", "stylePaneFormatFilter", "stylePaneSortMethod", "documentType"];
    let existing = doc.elements(root).find(|&c| doc.local_name(c) == "mailMerge");
    match (existing, kind) {
        (Some(e), None) => {
            let found = doc.elements(e).find(|&c| doc.local_name(c) == "mainDocumentType");
            if let Some(t) = found {
                doc.detach(t);
            }
            if doc.elements(e).next().is_none() {
                doc.detach(e);
            }
        }
        (Some(e), Some(k)) => {
            let found = doc.elements(e).find(|&c| doc.local_name(c) == "mainDocumentType");
            match found {
            Some(t) => doc.set_attr(t, "w:val", k),
            None => {
                let t = doc.append_element(e, "w:mainDocumentType");
                doc.set_attr(t, "w:val", k);
                doc.detach(t);
                doc.insert(e, 0, t);
            }
            }
        }
        (None, Some(k)) => {
            let after = doc.elements(root).filter(|&c| BEFORE.contains(&doc.local_name(c))).last();
            let e = doc.append_element(root, "w:mailMerge");
            let t = doc.append_element(e, "w:mainDocumentType");
            doc.set_attr(t, "w:val", k);
            doc.detach(e);
            match after {
                Some(a) => doc.insert_after(a, e),
                None => doc.insert(root, 0, e),
            }
        }
        (None, None) => {}
    }
    Ok(())
}

/// Write or remove external data source field mappings in `settings.xml`
/// (`w:mailMerge/w:odso/w:fieldMapData`, ECMA-376 Part 1 §17.14.15).
fn write_mail_merge_field_map(package: &mut Package, map: &SourceMap, field_map: &[ibis_doc_model::merge::FieldMapData]) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("settings.xml の無い文書の設定の変更"))?;
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;
    const BEFORE: &[&str] = &["writeProtection", "readOnlyRecommended", "view", "zoom", "removePersonalInformation", "removeDateAndTime", "doNotDisplayPageBoundaries", "displayBackgroundShape", "printPostScriptOverText", "printFractionalCharacterWidth", "printFormsData", "embedTrueTypeFonts", "embedSystemFonts", "saveSubsetFonts", "saveFormsData", "mirrorMargins", "alignBordersAndEdges", "bordersDoNotSurroundHeader", "bordersDoNotSurroundFooter", "gutterAtTop", "hideSpellingErrors", "hideGrammaticalErrors", "activeWritingStyle", "proofState", "formsDesign", "attachedTemplate", "linkStyles", "stylePaneFormatFilter", "stylePaneSortMethod", "documentType"];
    let existing_mm = doc.elements(root).find(|&c| doc.local_name(c) == "mailMerge");
    let mail_merge = match existing_mm {
        Some(e) => e,
        None if field_map.is_empty() => return Ok(()),
        None => {
            let after = doc.elements(root).filter(|&c| BEFORE.contains(&doc.local_name(c))).last();
            let e = doc.append_element(root, "w:mailMerge");
            doc.detach(e);
            match after {
                Some(a) => doc.insert_after(a, e),
                None => doc.insert(root, 0, e),
            }
            e
        }
    };
    let existing_odso = doc.elements(mail_merge).find(|&c| doc.local_name(c) == "odso");
    let odso = match existing_odso {
        Some(o) => o,
        None if field_map.is_empty() => return Ok(()),
        None => doc.append_element(mail_merge, "w:odso"),
    };
    // Detach existing fieldMapData children
    for child in doc.elements(odso).collect::<Vec<_>>() {
        if doc.local_name(child) == "fieldMapData" {
            doc.detach(child);
        }
    }
    if field_map.is_empty() {
        if doc.elements(odso).next().is_none() {
            doc.detach(odso);
        }
        if doc.elements(mail_merge).next().is_none() {
            doc.detach(mail_merge);
        }
        return Ok(());
    }
    for item in field_map {
        let fmd = doc.append_element(odso, "w:fieldMapData");
        let t = doc.append_element(fmd, "w:type");
        doc.set_attr(t, "w:val", "dbColumn");
        let name = doc.append_element(fmd, "w:name");
        doc.set_attr(name, "w:val", &item.name);
        let mapped = doc.append_element(fmd, "w:mappedName");
        doc.set_attr(mapped, "w:val", &item.mapped_name);
        let col = doc.append_element(fmd, "w:column");
        doc.set_attr(col, "w:val", &item.column.to_string());
    }
    Ok(())
}

/// Set (`Some`) or take away (`None`) the Japanese list of characters a line may not start (`noLineBreaksBefore`) or end
/// (`noLineBreaksAfter`) with in `settings.xml` (ECMA-376 Part 1 §17.15.1.58, §17.15.1.57).
fn write_kinsoku_setting(package: &mut Package, map: &SourceMap, name: &str, list: Option<&str>) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("settings.xml の無い文書の設定の変更"))?;
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;
    // What comes before them in CT_Settings.
    const BEFORE: &[&str] = &["writeProtection", "readOnlyRecommended", "view", "zoom", "removePersonalInformation", "removeDateAndTime", "doNotDisplayPageBoundaries", "displayBackgroundShape", "printPostScriptOverText", "printFractionalCharacterWidth", "printFormsData", "embedTrueTypeFonts", "embedSystemFonts", "saveSubsetFonts", "saveFormsData", "mirrorMargins", "alignBordersAndEdges", "bordersDoNotSurroundHeader", "bordersDoNotSurroundFooter", "gutterAtTop", "hideSpellingErrors", "hideGrammaticalErrors", "activeWritingStyle", "proofState", "formsDesign", "attachedTemplate", "linkStyles", "stylePaneFormatFilter", "stylePaneSortMethod", "documentType", "mailMerge", "revisionView", "trackRevisions", "doNotTrackMoves", "doNotTrackFormatting", "documentProtection", "autoFormatOverride", "styleLockTheme", "styleLockQFSet", "defaultTabStop", "autoHyphenation", "consecutiveHyphenLimit", "hyphenationZone", "doNotHyphenateCaps", "showEnvelope", "summaryLength", "clickAndTypeStyle", "defaultTableStyle", "evenAndOddHeaders", "bookFoldRevPrinting", "bookFoldPrinting", "bookFoldPrintingSheets", "drawingGridHorizontalSpacing", "drawingGridVerticalSpacing", "displayHorizontalDrawingGridEvery", "displayVerticalDrawingGridEvery", "doNotUseMarginsForDrawingGridOrigin", "drawingGridHorizontalOrigin", "drawingGridVerticalOrigin", "doNotShadeFormData", "noPunctuationKerning", "characterSpacingControl", "printTwoOnOne", "strictFirstAndLastChars", "noLineBreaksAfter", "noLineBreaksBefore"];
    let before: &[&str] = &BEFORE[..BEFORE.iter().position(|n| *n == name).unwrap_or(BEFORE.len())];
    let ja = |d: &XmlDocument, e: NodeId| d.local_name(e) == name && d.attr(e, "w:lang").is_some_and(|l| l.starts_with("ja"));
    let existing = doc.elements(root).find(|&c| ja(doc, c));
    match (existing, list) {
        (Some(e), None) => doc.detach(e),
        (Some(e), Some(l)) => doc.set_attr(e, "w:val", l),
        (None, Some(l)) => {
            let e = doc.append_element(root, &format!("w:{name}"));
            doc.set_attr(e, "w:lang", "ja-JP");
            doc.set_attr(e, "w:val", l);
            doc.detach(e);
            let after = doc.elements(root).filter(|&c| before.contains(&doc.local_name(c))).last();
            match after {
                Some(a) => doc.insert_after(a, e),
                None => doc.insert(root, 0, e),
            }
        }
        (None, None) => {}
    }
    Ok(())
}

/// Write or remove `w:themeFontLang` in `settings.xml` (ECMA-376 Part 1 §17.15.1.88, DOCX-JA-019).
fn write_theme_font_lang(package: &mut Package, map: &SourceMap, theme_lang: &Option<ibis_doc_model::ThemeFontLang>) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("settings.xml の無い文書の設定の変更"))?;
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;
    const BEFORE: &[&str] = &[
        "writeProtection", "readOnlyRecommended", "view", "zoom", "removePersonalInformation", "removeDateAndTime", "doNotDisplayPageBoundaries", "displayBackgroundShape", "printPostScriptOverText", "printFractionalCharacterWidth", "printFormsData", "embedTrueTypeFonts", "embedSystemFonts", "saveSubsetFonts", "saveFormsData", "saveFrequency", "mirrorMargins", "alignBordersAndEdges", "bordersDoNotSurroundHeader", "bordersDoNotSurroundFooter", "gutterAtTop", "hideSpellingErrors", "hideGrammaticalErrors", "activeWritingStyle", "proofState", "formsDesign", "attachedTemplate", "linkStyles", "stylePaneFormatFilter", "stylePaneSortMethod", "documentType", "mailMerge", "revisionView", "trackRevisions", "doNotTrackMoves", "doNotTrackFormatting", "documentProtection", "autoFormatOverride", "styleLockTheme", "styleLockQFSet", "defaultTabStop", "autoHyphenation", "consecutiveHyphenLimit", "hyphenationZone", "doNotHyphenateCaps", "showEnvelope", "summaryLength", "clickAndTypeStyle", "defaultTableStyle", "evenAndOddHeaders", "bookFoldRevPrinting", "bookFoldPrinting", "bookFoldPrintingSheets", "drawingGridHorizontalSpacing", "drawingGridVerticalSpacing", "displayHorizontalDrawingGridEvery", "displayVerticalDrawingGridEvery", "doNotUseMarginsForDrawingGridOrigin", "drawingGridHorizontalOrigin", "drawingGridVerticalOrigin", "doNotShadeFormData", "noPunctuationKerning", "characterSpacingControl", "printTwoOnOne", "strictFirstAndLastChars", "noLineBreaksAfter", "noLineBreaksBefore", "saveThroughXslt", "showXMLTags", "alwaysShowPlaceholderText", "doNotAutoCompressPictures", "updateFields",
        "footnotePr", "endnotePr", "compat", "docVars", "rsids", "mathPr", "attachedSchema",
    ];
    let existing = doc.elements(root).find(|&c| doc.local_name(c) == "themeFontLang");
    match (existing, theme_lang) {
        (Some(e), None) => doc.detach(e),
        (Some(e), Some(tfl)) => {
            if let Some(val) = &tfl.val {
                doc.set_attr(e, "w:val", val);
            } else {
                doc.remove_attr(e, "w:val");
            }
            if let Some(ea) = &tfl.east_asia {
                doc.set_attr(e, "w:eastAsia", ea);
            } else {
                doc.remove_attr(e, "w:eastAsia");
            }
            if let Some(bidi) = &tfl.bidi {
                doc.set_attr(e, "w:bidi", bidi);
            } else {
                doc.remove_attr(e, "w:bidi");
            }
        }
        (None, Some(tfl)) => {
            let e = doc.append_element(root, "w:themeFontLang");
            if let Some(val) = &tfl.val {
                doc.set_attr(e, "w:val", val);
            }
            if let Some(ea) = &tfl.east_asia {
                doc.set_attr(e, "w:eastAsia", ea);
            }
            if let Some(bidi) = &tfl.bidi {
                doc.set_attr(e, "w:bidi", bidi);
            }
            doc.detach(e);
            let after = doc.elements(root).filter(|&c| BEFORE.contains(&doc.local_name(c))).last();
            match after {
                Some(a) => doc.insert_after(a, e),
                None => doc.insert(root, 0, e),
            }
        }
        (None, None) => {}
    }
    Ok(())
}

/// Set the document view mode (`w:view`) in `settings.xml` (ECMA-376 Part 1 §17.15.1.92).
fn write_view_setting(package: &mut Package, map: &SourceMap, mode: ibis_doc_model::DocumentViewMode) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("settings.xml の無い文書の設定の変更"))?;
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;
    const BEFORE: &[&str] = &["writeProtection", "readOnlyRecommended"];
    let existing = doc.elements(root).find(|&c| doc.local_name(c) == "view");
    match existing {
        Some(e) => {
            doc.set_attr(e, "w:val", mode.as_wval());
        }
        None => {
            let e = doc.append_element(root, "w:view");
            doc.set_attr(e, "w:val", mode.as_wval());
            doc.detach(e);
            let after = doc.elements(root).filter(|&c| BEFORE.contains(&doc.local_name(c))).last();
            match after {
                Some(a) => doc.insert_after(a, e),
                None => doc.insert(root, 0, e),
            }
        }
    }
    Ok(())
}

/// Set or clear documentProtection in `settings.xml` (ECMA-376 Part 1 §17.15.1.29).
fn write_document_protection(
    package: &mut Package,
    map: &SourceMap,
    protection: ibis_doc_model::DocumentProtection,
    formatting_protection: bool,
) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("settings.xml の無い文書の設定の変更"))?;
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;
    const BEFORE: &[&str] = &["writeProtection", "readOnlyRecommended", "view", "zoom", "removePersonalInformation", "removeDateAndTime", "doNotDisplayPageBoundaries", "displayBackgroundShape", "printPostScriptOverText", "printFractionalCharacterWidth", "printFormsData", "embedTrueTypeFonts", "embedSystemFonts", "saveSubsetFonts", "saveFormsData", "mirrorMargins", "alignBordersAndEdges", "bordersDoNotSurroundHeader", "bordersDoNotSurroundFooter", "gutterAtTop", "hideSpellingErrors", "hideGrammaticalErrors", "activeWritingStyle", "proofState", "formsDesign", "attachedTemplate", "linkStyles", "stylePaneFormatFilter", "stylePaneSortMethod", "documentType", "mailMerge", "revisionView", "trackRevisions", "doNotTrackMoves", "doNotTrackFormatting"];
    let existing = doc.elements(root).find(|&c| doc.local_name(c) == "documentProtection");
    let has_protection = protection != ibis_doc_model::DocumentProtection::None;
    if !has_protection && !formatting_protection {
        if let Some(e) = existing {
            doc.detach(e);
        }
        return Ok(());
    }

    let node = match existing {
        Some(e) => e,
        None => {
            let e = doc.append_element(root, "w:documentProtection");
            doc.detach(e);
            let after = doc.elements(root).filter(|&c| BEFORE.contains(&doc.local_name(c))).last();
            match after {
                Some(a) => doc.insert_after(a, e),
                None => doc.insert(root, 0, e),
            }
            e
        }
    };

    doc.set_attr(node, "w:enforcement", "1");
    if let Some(edit_val) = protection.as_wedit() {
        doc.set_attr(node, "w:edit", edit_val);
    } else {
        doc.remove_attr(node, "w:edit");
    }
    if formatting_protection {
        doc.set_attr(node, "w:formatting", "1");
    } else {
        doc.remove_attr(node, "w:formatting");
    }
    Ok(())
}

/// Write document variables (`w:docVars` / `w:docVar`) to `settings.xml` (ECMA-376 Part 1 §17.15.1.30-31).
fn write_doc_variables(package: &mut Package, map: &SourceMap, vars: &std::collections::BTreeMap<String, String>) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    let Some(part) = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)) else {
        return Ok(());
    };
    let doc = package.xml_mut(&part)?;
    let Some(root) = doc.root_element() else {
        return Ok(());
    };
    let existing = doc.elements(root).find(|&c| doc.local_name(c) == "docVars");
    if vars.is_empty() {
        if let Some(e) = existing {
            doc.detach(e);
        }
        return Ok(());
    }
    let target = match existing {
        Some(e) => {
            let ch: Vec<_> = doc.elements(e).collect();
            for c in ch {
                doc.detach(c);
            }
            e
        }
        None => {
            const AFTER: &[&str] = &["rsids", "mathPr", "attachedSchema", "themeFontLang", "clrSchemeMapping", "doNotIncludeSubdocsInStats", "doNotAutoCompressPictures", "forceUpgrade", "captions", "readModeInkLockDown", "smartTagType", "schemaLibrary", "shapeDefaults", "doNotEmbedSmartTags", "decimalSymbol", "listSeparator"];
            let before = doc.elements(root).find(|&c| AFTER.contains(&doc.local_name(c)));
            let e = doc.append_element(root, "w:docVars");
            doc.detach(e);
            match before {
                Some(b) => doc.insert_before(b, e),
                None => {
                    let end = doc.children(root).len();
                    doc.insert(root, end, e);
                }
            }
            e
        }
    };
    for (name, val) in vars {
        let var = doc.append_element(target, "w:docVar");
        doc.set_attr(var, "w:name", name);
        doc.set_attr(var, "w:val", val);
    }
    Ok(())
}

/// Write a document's page colour (`w:background` in `document.xml` and `displayBackgroundShape` in `settings.xml`).
fn write_page_color(package: &mut Package, map: &SourceMap, color: Option<Rgb>) -> Result<(), Error> {
    let main = map.body_part.clone();
    let doc = package.xml_mut(&main)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の document.xml"))?;
    let existing = doc.elements(root).find(|&c| doc.namespace(c) == Some(W_NS) && doc.local_name(c) == "background");
    match (existing, color) {
        (Some(e), Some(rgb)) => {
            doc.set_attr(e, "w:color", &format!("{:06X}", rgb.0));
            doc.remove_attr(e, "w:themeColor");
            doc.remove_attr(e, "w:themeTint");
            doc.remove_attr(e, "w:themeShade");
        }
        (None, Some(rgb)) => {
            let e = doc.append_element(root, "w:background");
            doc.detach(e);
            doc.set_attr(e, "w:color", &format!("{:06X}", rgb.0));
            let body = doc.elements(root).find(|&c| doc.namespace(c) == Some(W_NS) && doc.local_name(c) == "body");
            match body {
                Some(b) => doc.insert_before(b, e),
                None => doc.insert(root, 0, e),
            }
        }
        (Some(e), None) => {
            doc.detach(e);
        }
        (None, None) => {}
    }
    write_setting(package, map, "displayBackgroundShape", color.is_some())?;
    Ok(())
}

/// Write image quality extension settings (`w14:defaultImageDpi`, `w14:discardImageEditingData`) to `settings.xml` (DOCX-OPTIONS-011).
fn write_image_quality_settings(
    package: &mut Package,
    map: &SourceMap,
    default_dpi: Option<u32>,
    discard_crop: bool,
) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("settings.xml の無い文書の設定の変更"))?;
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;

    if doc.lookup_prefix(root, W14_NS).is_none() {
        doc.set_attr(root, "xmlns:w14", W14_NS);
    }

    // w14:defaultImageDpi
    let existing_dpi = doc.elements(root).find(|&c| doc.local_name(c) == "defaultImageDpi");
    match (existing_dpi, default_dpi) {
        (Some(e), None) => doc.detach(e),
        (Some(e), Some(dpi)) => {
            doc.set_attr(e, "w14:val", &dpi.to_string());
        }
        (None, Some(dpi)) => {
            let e = doc.append_element(root, "w14:defaultImageDpi");
            doc.set_attr(e, "w14:val", &dpi.to_string());
        }
        (None, None) => {}
    }

    // w14:discardImageEditingData
    let existing_discard = doc.elements(root).find(|&c| doc.local_name(c) == "discardImageEditingData");
    match (existing_discard, discard_crop) {
        (Some(e), false) => doc.detach(e),
        (Some(_e), true) => {}
        (None, true) => {
            doc.append_element(root, "w14:discardImageEditingData");
        }
        (None, false) => {}
    }

    Ok(())
}

/// Write auto-recover save frequency (`w:saveFrequency`) to `settings.xml` (DOCX-OPTIONS-012, ECMA-376 Part 1 §17.15.1.72).
fn write_save_frequency(package: &mut Package, map: &SourceMap, frequency: Option<u32>) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("settings.xml の無い文書の設定の変更"))?;
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;

    const BEFORE: &[&str] = &[
        "writeProtection", "readOnlyRecommended", "view", "zoom", "removePersonalInformation", "removeDateAndTime", "doNotDisplayPageBoundaries", "displayBackgroundShape", "printPostScriptOverText", "printFractionalCharacterWidth", "printFormsData", "embedTrueTypeFonts", "embedSystemFonts", "saveSubsetFonts", "saveFormsData",
    ];

    let existing = doc.elements(root).find(|&c| doc.local_name(c) == "saveFrequency");
    match (existing, frequency) {
        (Some(e), None) => doc.detach(e),
        (Some(e), Some(val)) => {
            doc.set_attr(e, "w:val", &val.to_string());
        }
        (None, Some(val)) => {
            let e = doc.append_element(root, "w:saveFrequency");
            doc.detach(e);
            doc.set_attr(e, "w:val", &val.to_string());
            let after = doc.elements(root).filter(|&c| BEFORE.contains(&doc.local_name(c))).last();
            match after {
                Some(a) => doc.insert_after(a, e),
                None => doc.insert(root, 0, e),
            }
        }
        (None, None) => {}
    }
    Ok(())
}

/// Write attached template (`w:attachedTemplate`) to `settings.xml` and its relationship in `settings.xml.rels` (ECMA-376 Part 1 §17.15.1.13, DOCX-STYLE-017, DOCX-FILE-009).
fn write_attached_template(package: &mut Package, map: &SourceMap, attached_template: Option<&str>) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    const ATTACHED_TEMPLATE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/attachedTemplate";
    const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("settings.xml の無い文書の設定の変更"))?;

    // 1. settings.xml から既存の attachedTemplate の有無と r:id を調べる
    let existing_rid = {
        let doc = package.xml_mut(&part)?;
        let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;
        let existing = doc.elements(root).find(|&c| doc.local_name(c) == "attachedTemplate");
        existing.map(|e| (true, doc.attr(e, "r:id").map(str::to_string)))
    };

    // 2. リレーションシップの更新または作成 (attachedTemplate は外部テンプレートのため external: true)
    let target_rid = match (existing_rid, attached_template) {
        (Some((_, Some(rid))), None) => {
            let _ = package.remove_relationship(&part, &rid);
            None
        }
        (Some((_, None)), None) => None,
        (Some((_, Some(rid))), Some(target)) => {
            let rels_name = ibis_ooxml::package::relationships_part(&part);
            if let Ok(rels_doc) = package.xml_mut(&rels_name) {
                if let Some(r_root) = rels_doc.root_element() {
                    let r_node = rels_doc.children_named(r_root, "Relationship").find(|&r| rels_doc.attr(r, "Id") == Some(&rid));
                    if let Some(r) = r_node {
                        rels_doc.set_attr(r, "Target", target);
                    }
                }
            }
            Some(rid)
        }
        (Some((_, None)), Some(target)) => {
            let rid = package.add_relationship(&part, ATTACHED_TEMPLATE_REL, target, true)?;
            Some(rid)
        }
        (None, Some(target)) => {
            let rid = package.add_relationship(&part, ATTACHED_TEMPLATE_REL, target, true)?;
            Some(rid)
        }
        (None, None) => None,
    };

    // 3. settings.xml のノードを更新
    const BEFORE: &[&str] = &[
        "writeProtection", "readOnlyRecommended", "trackRevisions", "doNotTrackMoves", "doNotTrackFormatting", "documentProtection", "autoFormatOverride", "styleLockTheme", "styleLockQFSet", "defaultTabStop", "autoHyphenation", "consecutiveHyphenLimit", "hyphenationZone", "doNotHyphenateCaps", "showEnvelope", "summaryLength", "clickAndTypeStyle", "defaultTableStyle", "evenAndOddHeaders", "bookFoldRevPrinting", "bookFoldPrinting", "bookFoldPrintingSheets", "drawingGridHorizontalSpacing", "drawingGridVerticalSpacing", "displayHorizontalDrawingGridEvery", "displayVerticalDrawingGridEvery", "doNotUseMarginsForDrawingGridOrigin", "drawingGridHorizontalOrigin", "drawingGridVerticalOrigin", "doNotShadeFormData", "noPunctuationKerning", "characterSpacingControl", "printTwoOnOne", "strictFirstAndLastChars", "noLineBreaksAfter", "noLineBreaksBefore", "saveFormsData", "mirrorMargins", "alignBordersAndEdges", "bordersDoNotSurroundHeader", "bordersDoNotSurroundFooter", "gutterAtTop", "hideSpellingErrors", "hideGrammaticalErrors", "activeWritingStyle", "proofState", "formsDesign",
    ];

    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;
    if doc.lookup_prefix(root, R_NS).is_none() {
        doc.set_attr(root, "xmlns:r", R_NS);
    }
    let existing_node = doc.elements(root).find(|&c| doc.local_name(c) == "attachedTemplate");

    match (existing_node, target_rid) {
        (Some(e), None) => {
            doc.detach(e);
        }
        (Some(e), Some(rid)) => {
            doc.set_attr(e, "r:id", &rid);
        }
        (None, Some(rid)) => {
            let e = doc.append_element(root, "w:attachedTemplate");
            doc.detach(e);
            doc.set_attr(e, "r:id", &rid);
            let after = doc.elements(root).filter(|&c| BEFORE.contains(&doc.local_name(c))).last();
            match after {
                Some(a) => doc.insert_after(a, e),
                None => doc.insert(root, 0, e),
            }
        }
        (None, None) => {}
    }

    Ok(())
}

/// Set or update `<w:compat>` in `settings.xml` (ECMA-376 Part 1 §17.15.1.21, DOCX-OPTIONS-013).
fn write_compatibility(
    package: &mut Package,
    map: &SourceMap,
    compatibility_mode: Option<u32>,
    use_fe_layout: bool,
    do_not_expand_shift_return: bool,
) -> Result<(), Error> {
    const SETTINGS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == SETTINGS_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("settings.xml の無い文書の設定の変更"))?;
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の settings.xml"))?;
    const BEFORE: &[&str] = &[
        "writeProtection", "readOnlyRecommended", "view", "zoom", "removePersonalInformation", "removeDateAndTime", "doNotDisplayPageBoundaries", "displayBackgroundShape", "printPostScriptOverText", "printFractionalCharacterWidth", "printFormsData", "embedTrueTypeFonts", "embedSystemFonts", "saveSubsetFonts", "saveFormsData", "saveFrequency", "mirrorMargins", "alignBordersAndEdges", "bordersDoNotSurroundHeader", "bordersDoNotSurroundFooter", "gutterAtTop", "hideSpellingErrors", "hideGrammaticalErrors", "activeWritingStyle", "proofState", "formsDesign", "attachedTemplate", "linkStyles", "stylePaneFormatFilter", "stylePaneSortMethod", "documentType", "mailMerge", "revisionView", "trackRevisions", "doNotTrackMoves", "doNotTrackFormatting", "documentProtection", "autoFormatOverride", "styleLockTheme", "styleLockQFSet", "defaultTabStop", "autoHyphenation", "consecutiveHyphenLimit", "hyphenationZone", "doNotHyphenateCaps", "showEnvelope", "summaryLength", "clickAndTypeStyle", "defaultTableStyle", "evenAndOddHeaders", "updateFields", "footnotePr", "endnotePr",
    ];

    let existing = doc.elements(root).find(|&c| doc.local_name(c) == "compat");
    let has_any = compatibility_mode.is_some() || use_fe_layout || do_not_expand_shift_return;
    let compat_node = match (existing, has_any) {
        (Some(e), _) => e,
        (None, false) => return Ok(()),
        (None, true) => {
            let e = doc.append_element(root, "w:compat");
            doc.detach(e);
            let after = doc.elements(root).filter(|&c| BEFORE.contains(&doc.local_name(c))).last();
            match after {
                Some(a) => doc.insert_after(a, e),
                None => doc.insert(root, 0, e),
            }
            e
        }
    };

    // useFELayout
    let fe_node = doc.elements(compat_node).find(|&c| doc.local_name(c) == "useFELayout");
    match (fe_node, use_fe_layout) {
        (Some(n), false) => doc.detach(n),
        (Some(_), true) => {}
        (None, true) => {
            let setting = doc.elements(compat_node).find(|&c| doc.local_name(c) == "compatSetting");
            let n = doc.append_element(compat_node, "w:useFELayout");
            if let Some(s) = setting {
                doc.detach(n);
                doc.insert_before(s, n);
            }
        }
        (None, false) => {}
    }

    // doNotExpandShiftReturn
    let shift_node = doc.elements(compat_node).find(|&c| doc.local_name(c) == "doNotExpandShiftReturn");
    match (shift_node, do_not_expand_shift_return) {
        (Some(n), false) => doc.detach(n),
        (Some(_), true) => {}
        (None, true) => {
            let setting = doc.elements(compat_node).find(|&c| doc.local_name(c) == "compatSetting");
            let n = doc.append_element(compat_node, "w:doNotExpandShiftReturn");
            if let Some(s) = setting {
                doc.detach(n);
                doc.insert_before(s, n);
            }
        }
        (None, false) => {}
    }

    // compatSetting for compatibilityMode
    let mode_node = doc.elements(compat_node).find(|&c| doc.local_name(c) == "compatSetting" && doc.attr(c, "w:name") == Some("compatibilityMode"));
    match (mode_node, compatibility_mode) {
        (Some(n), None) => doc.detach(n),
        (Some(n), Some(val)) => {
            doc.set_attr(n, "w:val", &val.to_string());
        }
        (None, Some(val)) => {
            let n = doc.append_element(compat_node, "w:compatSetting");
            doc.set_attr(n, "w:name", "compatibilityMode");
            doc.set_attr(n, "w:uri", "http://schemas.microsoft.com/office/word");
            doc.set_attr(n, "w:val", &val.to_string());
        }
        (None, None) => {}
    }

    if doc.elements(compat_node).next().is_none() {
        doc.detach(compat_node);
    }

    Ok(())
}

struct Writer<'a> {
    doc: &'a mut XmlDocument,
    pristine: &'a XmlDocument,
    map: &'a SourceMap,
    part: &'a str,
    old_blocks: &'a HashMap<u32, &'a Block>,
    new_blocks: &'a HashMap<u32, &'a Block>,
    pictures: &'a HashMap<MediaId, (u32, usize)>,
    /// The elements of the part's paragraphs and tables, by id, found
    /// before anything changed.
    nodes: HashMap<u32, NodeId>,
    /// Pictures new to the part: the relationship to their media part.
    added: &'a HashMap<MediaId, String>,
    /// Pictures new to the part that are SVGs.
    svg_media: &'a HashSet<MediaId>,
    /// The element new paragraphs go into when there are none to copy (a
    /// new header's root).
    empty_parent: Option<NodeId>,
    /// The part's relationships to the links' addresses.
    link_rels: &'a HashMap<String, String>,
    /// The elements of the blocks written, by id (new ones too).
    written: HashMap<u32, NodeId>,
    /// The elements of the shapes of the paragraph being written, by shape
    /// id (the old ones, to write an edited text box from).
    shape_nodes: HashMap<u64, NodeId>,
    /// The old shapes of those, by shape id (to see what was changed).
    shape_olds: HashMap<u64, Box<Shape>>,
    /// The group hierarchies (`wpg:wgp`) in the baseline and current documents.
    old_shape_groups: &'a [ibis_doc_model::ShapeGroup],
    new_shape_groups: &'a [ibis_doc_model::ShapeGroup],
}

/// CT_RPr's children in order.
const RPR_ORDER: &[&str] = &[
    "ins", "del", "moveFrom", "moveTo", "rStyle", "rFonts", "b", "bCs", "i", "iCs", "caps", "smallCaps", "strike", "dstrike", "outline",
    "shadow", "emboss", "imprint", "noProof", "snapToGrid", "vanish", "webHidden", "color", "spacing", "w", "kern", "position", "sz", "szCs",
    "highlight", "u", "effect", "bdr", "shd", "fitText", "vertAlign", "rtl", "cs", "em", "lang", "eastAsianLayout", "specVanish", "oMath",
    "rPrChange",
];

/// CT_PPr's children in order.
const PPR_ORDER: &[&str] = &[
    "pStyle", "keepNext", "keepLines", "pageBreakBefore", "framePr", "widowControl", "numPr", "suppressLineNumbers", "pBdr", "shd", "tabs",
    "suppressAutoHyphens", "kinsoku", "wordWrap", "overflowPunct", "topLinePunct", "autoSpaceDE", "autoSpaceDN", "bidi", "adjustRightInd",
    "snapToGrid", "spacing", "ind", "contextualSpacing", "mirrorIndents", "suppressOverlap", "jc", "textDirection", "textAlignment",
    "textboxTightWrap", "outlineLvl", "divId", "cnfStyle", "rPr", "sectPr", "pPrChange",
];

/// Where a position of the new paragraph goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Host {
    /// Into (a copy of) this run of the paragraph.
    Run(NodeId),
    /// A field kept as it is (the index of its position in the old paragraph).
    Field(usize),
    /// A new run formatted from the paragraph mark.
    Mark,
}

impl Writer<'_> {
    /// The title, tag and lock of a content control (`sdt`, a copy of the
    /// original or new), in `w:sdtPr`'s order: `w:alias`, `w:tag`, `w:id`,
    /// `w:lock`.
    fn apply_control(&mut self, sdt: NodeId, control: &Option<Box<ibis_doc_model::ControlInfo>>) {
        let Some(pr) = self.doc.child(sdt, "sdtPr") else { return };
        for name in ["alias", "tag", "lock"] {
            if let Some(old) = self.doc.child(pr, name) {
                self.doc.detach(old);
            }
        }
        let Some(info) = control else { return };
        let put = |this: &mut Self, name: &str, value: &str, after: &[&str]| {
            let at = this.doc.children(pr).iter().rposition(|&c| this.doc.is_element(c) && after.contains(&this.doc.local_name(c))).map_or(0, |i| i + 1);
            let element = this.doc.append_element(pr, &this.w(name));
            this.doc.detach(element);
            this.doc.set_attr(element, "w:val", value);
            this.doc.insert(pr, at, element);
        };
        if !info.title.is_empty() {
            put(self, "alias", &info.title, &["rPr"]);
        }
        if !info.tag.is_empty() {
            put(self, "tag", &info.tag, &["rPr", "alias"]);
        }
        if let Some(lock) = info.lock.name() {
            put(self, "lock", lock, &["rPr", "alias", "tag", "id"]);
        }
    }

    /// A text content control's text (`sdt`, a copy of the original): the
    /// first run of its content has it, the others go; `w:showingPlcHdr`
    /// says whether it is the prompt.
    fn set_control_text(&mut self, sdt: NodeId, text: &str, showing: bool) {
        if let Some(content) = self.doc.child(sdt, "sdtContent") {
            let runs: Vec<_> = self.doc.children_named(content, "r").collect();
            for (i, r) in runs.into_iter().enumerate() {
                if i > 0 {
                    self.doc.detach(r);
                } else if let Some(t) = self.doc.child(r, "t") {
                    self.doc.set_text(t, text);
                }
            }
        }
        let Some(pr) = self.doc.child(sdt, "sdtPr") else { return };
        if let Some(old) = self.doc.child(pr, "showingPlcHdr") {
            self.doc.detach(old);
        }
        if showing {
            let at = self.doc.children(pr).iter().rposition(|&c| self.doc.is_element(c) && matches!(self.doc.local_name(c), "rPr" | "alias" | "tag" | "id" | "lock" | "placeholder" | "temporary")).map_or(0, |i| i + 1);
            let element = self.doc.append_element(pr, &self.w("showingPlcHdr"));
            self.doc.detach(element);
            self.doc.insert(pr, at, element);
        }
    }

    fn w(&self, name: &str) -> String {
        format!("w:{name}")
    }

    fn wp(&self, name: &str) -> String {
        format!("wp:{name}")
    }

    /// Configure a `<w:drawing>`'s inner container as inline (`<wp:inline>`)
    /// or anchored (`<wp:anchor>`) with the specified floating properties.
    fn configure_drawing_anchor(&mut self, drawing: NodeId, anchor: Option<&ibis_doc_model::FloatingAnchor>) {
        let holder = match self.doc.children(drawing).iter().copied().find(|&n| self.doc.is_element(n) && matches!(self.doc.local_name(n), "inline" | "anchor")) {
            Some(h) => h,
            None => return,
        };

        if let Some(f) = anchor {
            // Convert or update to wp:anchor
            self.doc.rename(holder, &self.wp("anchor"));
            self.doc.set_attr(holder, "distT", &f.dist_t.to_string());
            self.doc.set_attr(holder, "distB", &f.dist_b.to_string());
            self.doc.set_attr(holder, "distL", &f.dist_l.to_string());
            self.doc.set_attr(holder, "distR", &f.dist_r.to_string());
            self.doc.set_attr(holder, "simplePos", "0");
            self.doc.set_attr(holder, "relativeHeight", &f.relative_height.to_string());
            self.doc.set_attr(holder, "behindDoc", if f.behind_doc { "1" } else { "0" });
            self.doc.set_attr(holder, "locked", if f.locked { "1" } else { "0" });
            self.doc.set_attr(holder, "layoutInCell", if f.layout_in_cell { "1" } else { "0" });
            self.doc.set_attr(holder, "allowOverlap", if f.allow_overlap { "1" } else { "0" });

            // Look for existing elements: simplePos, positionH, positionV, extent, effectExtent, wrap...
            let extent = self.doc.children(holder).iter().copied().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "extent");

            // 1. simplePos
            let simple_pos = self.doc.children(holder).iter().copied().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "simplePos");
            let sp_node = match simple_pos {
                Some(n) => n,
                None => {
                    let name = self.wp("simplePos");
                    let n = self.doc.append_element(holder, &name);
                    self.doc.detach(n);
                    if let Some(ext) = extent {
                        self.doc.insert_before(ext, n);
                    } else {
                        self.doc.insert(holder, 0, n);
                    }
                    n
                }
            };
            self.doc.set_attr(sp_node, "x", "0");
            self.doc.set_attr(sp_node, "y", "0");

            // 2. positionH
            let pos_h = self.doc.children(holder).iter().copied().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "positionH");
            let ph_node = match pos_h {
                Some(n) => n,
                None => {
                    let name = self.wp("positionH");
                    let n = self.doc.append_element(holder, &name);
                    self.doc.detach(n);
                    if let Some(ext) = extent {
                        self.doc.insert_before(ext, n);
                    } else {
                        let end = self.doc.children(holder).len();
                        self.doc.insert(holder, end, n);
                    }
                    n
                }
            };
            let rel_h = if f.relative_h.is_empty() { "column" } else { f.relative_h.as_str() };
            self.doc.set_attr(ph_node, "relativeFrom", rel_h);
            for child in self.doc.children(ph_node).to_vec() {
                self.doc.detach(child);
            }
            if let Some(align) = &f.align_h {
                let name = self.wp("align");
                let el = self.doc.append_element(ph_node, &name);
                self.doc.set_text(el, align);
            } else {
                let name = self.wp("posOffset");
                let el = self.doc.append_element(ph_node, &name);
                self.doc.set_text(el, &f.offset_x.to_string());
            }

            // 3. positionV
            let pos_v = self.doc.children(holder).iter().copied().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "positionV");
            let pv_node = match pos_v {
                Some(n) => n,
                None => {
                    let name = self.wp("positionV");
                    let n = self.doc.append_element(holder, &name);
                    self.doc.detach(n);
                    if let Some(ext) = extent {
                        self.doc.insert_before(ext, n);
                    } else {
                        let end = self.doc.children(holder).len();
                        self.doc.insert(holder, end, n);
                    }
                    n
                }
            };
            let rel_v = if f.relative_v.is_empty() { "paragraph" } else { f.relative_v.as_str() };
            self.doc.set_attr(pv_node, "relativeFrom", rel_v);
            for child in self.doc.children(pv_node).to_vec() {
                self.doc.detach(child);
            }
            if let Some(align) = &f.align_v {
                let name = self.wp("align");
                let el = self.doc.append_element(pv_node, &name);
                self.doc.set_text(el, align);
            } else {
                let name = self.wp("posOffset");
                let el = self.doc.append_element(pv_node, &name);
                self.doc.set_text(el, &f.offset_y.to_string());
            }

            // Ensure order: simplePos, positionH, positionV appear before extent
            if let Some(ext) = extent {
                self.doc.detach(sp_node);
                self.doc.detach(ph_node);
                self.doc.detach(pv_node);
                self.doc.insert_before(ext, sp_node);
                self.doc.insert_before(ext, ph_node);
                self.doc.insert_before(ext, pv_node);
            }

            // 4. wrap element
            // Detach any existing wrap elements
            for child in self.doc.children(holder).to_vec() {
                if self.doc.is_element(child) && matches!(self.doc.local_name(child), "wrapSquare" | "wrapTight" | "wrapThrough" | "wrapTopAndBottom" | "wrapNone") {
                    self.doc.detach(child);
                }
            }
            // Construct new wrap element
            let wrap_text = f.wrap_text.as_deref().unwrap_or("bothSides");
            let wrap_elem = match f.wrap.as_str() {
                "tight" => {
                    let name = self.wp("wrapTight");
                    let w = self.doc.append_element(holder, &name);
                    self.doc.detach(w);
                    self.doc.set_attr(w, "wrapText", wrap_text);
                    self.append_wrap_polygon(w, f.wrap_polygon.as_ref());
                    w
                }
                "through" => {
                    let name = self.wp("wrapThrough");
                    let w = self.doc.append_element(holder, &name);
                    self.doc.detach(w);
                    self.doc.set_attr(w, "wrapText", wrap_text);
                    self.append_wrap_polygon(w, f.wrap_polygon.as_ref());
                    w
                }
                "topAndBottom" => {
                    let name = self.wp("wrapTopAndBottom");
                    let w = self.doc.append_element(holder, &name);
                    self.doc.detach(w);
                    w
                }
                "none" => {
                    let name = self.wp("wrapNone");
                    let w = self.doc.append_element(holder, &name);
                    self.doc.detach(w);
                    w
                }
                _ => {
                    // "square" or default
                    let name = self.wp("wrapSquare");
                    let w = self.doc.append_element(holder, &name);
                    self.doc.detach(w);
                    self.doc.set_attr(w, "wrapText", wrap_text);
                    w
                }
            };
            // Place wrap element before docPr (or after effectExtent/extent)
            let doc_pr = self.doc.children(holder).iter().copied().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "docPr");
            if let Some(dp) = doc_pr {
                self.doc.insert_before(dp, wrap_elem);
            } else {
                let end = self.doc.children(holder).len();
                self.doc.insert(holder, end, wrap_elem);
            }
        } else {
            // Convert to wp:inline
            self.doc.rename(holder, &self.wp("inline"));
            self.doc.set_attr(holder, "distT", "0");
            self.doc.set_attr(holder, "distB", "0");
            self.doc.set_attr(holder, "distL", "0");
            self.doc.set_attr(holder, "distR", "0");
            for attr in ["simplePos", "relativeHeight", "behindDoc", "locked", "layoutInCell", "allowOverlap"] {
                self.doc.remove_attr(holder, attr);
            }
            // Detach anchor-specific children
            for child in self.doc.children(holder).to_vec() {
                if self.doc.is_element(child) && matches!(self.doc.local_name(child), "simplePos" | "positionH" | "positionV" | "wrapSquare" | "wrapTight" | "wrapThrough" | "wrapTopAndBottom" | "wrapNone") {
                    self.doc.detach(child);
                }
            }
        }
    }

    fn append_wrap_polygon(&mut self, wrap_elem: NodeId, polygon: Option<&ibis_doc_model::WrapPolygon>) {
        let default_poly = ibis_doc_model::WrapPolygon::default_rect_21600();
        let poly = polygon.unwrap_or(&default_poly);
        let poly_name = self.wp("wrapPolygon");
        let poly_node = self.doc.append_element(wrap_elem, &poly_name);
        self.doc.set_attr(poly_node, "edited", if poly.edited { "1" } else { "0" });
        let start_name = self.wp("start");
        let start_node = self.doc.append_element(poly_node, &start_name);
        self.doc.set_attr(start_node, "x", &poly.start.x.to_string());
        self.doc.set_attr(start_node, "y", &poly.start.y.to_string());
        let line_name = self.wp("lineTo");
        for pt in &poly.line_to {
            let line_node = self.doc.append_element(poly_node, &line_name);
            self.doc.set_attr(line_node, "x", &pt.x.to_string());
            self.doc.set_attr(line_node, "y", &pt.y.to_string());
        }
    }

    fn resolve_nodes(&mut self) {
        for (id, source) in &self.map.paragraphs {
            if source.part == self.part
                && let Some(node) = self.doc.by_path(&source.path)
            {
                self.nodes.insert(*id, node);
            }
        }
        for (id, source) in &self.map.tables {
            if source.part == self.part
                && let Some(node) = self.doc.by_path(&source.path)
            {
                self.nodes.insert(*id, node);
            }
        }
    }

    fn shape_group_of(
        groups: &[ibis_doc_model::ShapeGroup],
        id: ibis_doc_model::ShapeId,
    ) -> Option<&ibis_doc_model::ShapeGroup> {
        groups.iter().find(|g| g.contains_shape(id))
    }

    fn paragraph_group_changed(&self, old: &FlowParagraph, new: &FlowParagraph) -> bool {
        if self.old_shape_groups == self.new_shape_groups {
            return false;
        }
        let check = |p: &FlowParagraph| {
            p.runs.iter().any(|r| {
                if let FlowRunKind::Shape(s) = &r.kind {
                    Self::shape_group_of(self.old_shape_groups, s.id)
                        != Self::shape_group_of(self.new_shape_groups, s.id)
                } else {
                    false
                }
            })
        };
        check(old) || check(new)
    }

    /// Write the changes of a list of blocks.
    fn list(&mut self, old: &[Block], new: &[Block]) -> Result<(), Error> {
        // Blocks gone.
        let new_ids: HashSet<u32> = new.iter().map(block_id).collect();
        for b in old {
            let id = block_id(b);
            if !new_ids.contains(&id) {
                if self.new_blocks.contains_key(&id) {
                    return Err(unsupported("段落の移動"));
                }
                let node = *self.nodes.get(&id).ok_or_else(|| unsupported("元の場所が分からない段落の削除"))?;
                self.doc.detach(node);
            }
        }
        // The blocks kept are in their order.
        let old_order: HashMap<u32, usize> = old.iter().enumerate().map(|(i, b)| (block_id(b), i)).collect();
        let kept: Vec<usize> = new.iter().filter_map(|b| old_order.get(&block_id(b)).copied()).collect();
        // Moved blocks (a heading and what is under it moved): their elements are put in
        // the new order once all are written.
        let reordered = kept.windows(2).any(|w| w[0] >= w[1]);
        if kept.is_empty() && !new.is_empty() && self.empty_parent.is_none() {
            return Err(unsupported("段落をすべて置き換える変更"));
        }
        let mut previous: Option<NodeId> = None;
        for (i, block) in new.iter().enumerate() {
            let id = block_id(block);
            let node = match (self.old_blocks.get(&id).copied().filter(|_| old_order.contains_key(&id)), block) {
                (Some(Block::Paragraph(was)), Block::Paragraph(p)) => {
                    let node = *self.nodes.get(&id).ok_or_else(|| unsupported("元の場所が分からない段落の変更"))?;
                    if was != p || self.paragraph_group_changed(was, p) {
                        let source = &self.map.paragraphs[&id];
                        self.paragraph(node, was, p, &source.atoms)?;
                    }
                    node
                }
                (Some(Block::Table(was)), Block::Table(t)) => {
                    let node = *self.nodes.get(&id).ok_or_else(|| unsupported("元の場所が分からない表の変更"))?;
                    if was != t {
                        self.table(node, was, t)?;
                    }
                    node
                }
                (None, Block::Table(t)) => {
                    // A new table: its cells' paragraphs are written from the
                    // nearest paragraph of the list (their formatting then
                    // set to the new paragraphs').
                    let template = new[..i].iter().rev().chain(new[i + 1..].iter()).find_map(|b| match self.old_blocks.get(&block_id(b)) {
                        Some(Block::Paragraph(t)) if old_order.contains_key(&t.id) => Some(t),
                        _ => None,
                    });
                    // No paragraph to copy (a new header): the cells' paragraphs are
                    // written from nothing.
                    if template.is_none() && self.empty_parent.is_none() {
                        return Err(unsupported("段落の無い場所への表の追加"));
                    }
                    let node = self.new_table(t, template)?;
                    match previous {
                        Some(prev) => self.doc.insert_after(prev, node),
                        None => match new[i + 1..].iter().find_map(|b| self.nodes.get(&block_id(b)).copied().filter(|_| old_order.contains_key(&block_id(b)))) {
                            Some(next) => self.doc.insert_before(next, node),
                            None => {
                                let parent = self.empty_parent.ok_or_else(|| unsupported("表の追加"))?;
                                let end = self.doc.children(parent).len();
                                self.doc.insert(parent, end, node);
                            }
                        },
                    }
                    node
                }
                (None, Block::Paragraph(p)) => {
                    // A new paragraph: a copy of its neighbour's element
                    // (the paragraph it was split from), written over.
                    let template = new[..i].iter().rev().chain(new[i + 1..].iter()).find_map(|b| match self.old_blocks.get(&block_id(b)) {
                        Some(Block::Paragraph(t)) if old_order.contains_key(&t.id) => Some(t),
                        _ => None,
                    });
                    let Some(template) = template else {
                        // No paragraph to copy (a new header): a new element,
                        // written from nothing.
                        let Some(parent) = self.empty_parent else { return Err(unsupported("表の隣への段落の追加")) };
                        let name = self.w("p");
                        let node = self.doc.append_element(parent, &name);
                        self.doc.detach(node);
                        match previous {
                            Some(prev) => self.doc.insert_after(prev, node),
                            None => {
                                let end = self.doc.children(parent).len();
                                self.doc.insert(parent, end, node);
                            }
                        }
                        let blank = FlowParagraph { id: p.id, ..FlowParagraph::default() };
                        self.paragraph(node, &blank, p, &[])?;
                        self.written.insert(id, node);
                        previous = Some(node);
                        continue;
                    };
                    let source = &self.map.paragraphs[&template.id];
                    let original = self.pristine.by_path(&source.path).ok_or_else(|| unsupported("段落の追加"))?;
                    if let Some(Block::Paragraph(was))=self.old_blocks.get(&p.id).copied() {
                        // Horizontal cell merge: the source cell was removed,
                        // but its original paragraph XML must move intact.
                        let node=*self.nodes.get(&p.id).ok_or_else(||unsupported("セル内段落の移動"))?;
                        let source_map=&self.map.paragraphs[&p.id];
                        let source_original=self.pristine.by_path(&source_map.path).ok_or_else(||unsupported("セル内段落の移動"))?;
                        let source_cell=self.doc.parent(node).filter(|&n|self.doc.local_name(n)=="tc");
                        let original_cell=self.pristine.parent(source_original).filter(|&n|self.pristine.local_name(n)=="tc");
                        let target_cell=self.pristine.parent(original).filter(|&n|self.pristine.local_name(n)=="tc");
                        let same_row=original_cell.zip(target_cell).is_some_and(|(a,b)|self.pristine.parent(a)==self.pristine.parent(b));
                        if same_row && source_cell.is_some_and(|c|self.doc.parent(c).is_none()) {
                            let prev=previous.ok_or_else(||unsupported("先頭へのセル内段落の移動"))?;
                            self.doc.detach(node);
                            if was!=p {self.paragraph(node,was,p,&source_map.atoms)?;}
                            self.doc.insert_after(prev,node);
                            previous=Some(node);
                            continue;
                        }
                        return Err(unsupported("複雑なセル内段落の移動"));
                    }
                    let node = self.doc.import(self.pristine, original);
                    match previous {
                        Some(prev) => self.doc.insert_after(prev, node),
                        None => {
                            let next = new[i + 1..].iter().find_map(|b| self.nodes.get(&block_id(b)).copied().filter(|_| old_order.contains_key(&block_id(b))));
                            let next = next.ok_or_else(|| unsupported("段落の追加"))?;
                            self.doc.insert_before(next, node);
                        }
                    }
                    self.write_copy(node, template, p, &source.atoms)?;
                    node
                }
                _ => return Err(unsupported("段落と表の入れ替え")),
            };
            self.written.insert(id, node);
            previous = Some(node);
        }
        if reordered {
            // The elements of the blocks, in the new order, one after the other where the first
            // of them was; refused when something else sits between them.
            let nodes: Vec<NodeId> = new.iter().filter_map(|b| self.written.get(&block_id(b)).copied()).collect();
            let parent = nodes.first().and_then(|&n| self.doc.parent(n)).ok_or_else(|| unsupported("段落の並べ替え"))?;
            let children = self.doc.children(parent).to_vec();
            let positions: Vec<usize> = nodes.iter().filter_map(|n| children.iter().position(|c| c == n)).collect();
            if positions.len() != nodes.len() || nodes.iter().any(|&n| self.doc.parent(n) != Some(parent)) {
                return Err(unsupported("段落の並べ替え"));
            }
            let (low, high) = (*positions.iter().min().unwrap(), *positions.iter().max().unwrap());
            if high - low + 1 != nodes.len() {
                return Err(unsupported("段落の並べ替え"));
            }
            for &n in &nodes {
                self.doc.detach(n);
            }
            for (k, &n) in nodes.iter().enumerate() {
                self.doc.insert(parent, low + k, n);
            }
        }
        Ok(())
    }

    /// The sections' `w:sectPr` after the body was written: what changed in
    /// each (its page, page numbers, first page) and the references to its
    /// headers and footers (`wanted`, in order: a relationship, or none).
    fn sections(&mut self, base: &FlowDocument, cur: &FlowDocument, wanted: &[(u32, [Option<String>; 6])]) -> Result<(), Error> {
        let old_sections: HashMap<u32, (ibis_doc_model::SectionBreak, Option<std::sync::Arc<ibis_doc_model::SectionSetup>>)> =
            base.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some((p.id, (p.section_break, p.section.clone()))) } else { None }).collect();
        // The relationship each kind resolves to before each section.
        let mut resolved: [Option<String>; 6] = Default::default();
        for (id, refs) in wanted {
            let (sect, old, new, changed) = if *id == 0 {
                let body = self.doc.by_path(&self.map.body_path).ok_or_else(|| unsupported("セクションの変更"))?;
                let sect = match self.doc.child(body, "sectPr") {
                    Some(s) => s,
                    None => self.doc.append_element(body, "w:sectPr"),
                };
                let changed = last_view(base) != last_view(cur) || base.linked != cur.linked || PartKind::ALL.iter().any(|k| !cur.linked[k.index()] && self.map.section_parts.get(&(0, k.index())).is_none());
                (sect, last_view(base), last_view(cur), changed)
            } else {
                let Some(Block::Paragraph(p)) = cur.body.iter().find(|b| matches!(b, Block::Paragraph(p) if p.id == *id)) else { continue };
                let Some(setup) = &p.section else { continue };
                let node = self.written.get(id).or_else(|| self.nodes.get(id)).copied().ok_or_else(|| unsupported("セクションの変更"))?;
                let Some(sect) = self.doc.child(node, "pPr").and_then(|ppr| self.doc.child(ppr, "sectPr")) else { return Err(unsupported("セクションの変更")) };
                // A new break's sectPr is a copy of the last section's.
                let (old, changed) = match old_sections.get(id) {
                    Some((_, Some(was))) => (section_view(was), **was != **setup),
                    _ => (last_view(base), true),
                };
                (sect, old, section_view(setup), changed)
            };
            if changed {
                self.write_sect(sect, &old, &new)?;
                for kind in PartKind::ALL {
                    let k = kind.index();
                    let existing = self.reference(sect, kind);
                    match &refs[k] {
                        Some(rid) if existing.as_ref() != Some(rid) => self.set_reference(sect, kind, Some(rid)),
                        None if existing.is_some() && existing != resolved[k] => self.set_reference(sect, kind, None),
                        _ => {}
                    }
                }
            }
            for k in 0..6 {
                if refs[k].is_some() {
                    resolved[k].clone_from(&refs[k]);
                }
            }
        }
        // The breaks: how each section starts (its w:type) is the break
        // after the section before (the model's, on that one's last
        // paragraph), written where that changed or either is new.
        let ids = ibis_doc_model::flow_command::sections(cur);
        let break_of = |doc: &FlowDocument, id: u32| doc.body.iter().find_map(|b| if let Block::Paragraph(p) = b && p.id == id && p.section.is_some() { Some(p.section_break) } else { None });
        for k in 1..ids.len() {
            let (prev, id) = (ids[k - 1], ids[k]);
            let want = break_of(cur, prev).unwrap_or_default();
            let was = break_of(base, prev);
            let existed = id == 0 || break_of(base, id).is_some();
            if was == Some(want) && existed {
                continue;
            }
            let sect = if id == 0 {
                self.doc.by_path(&self.map.body_path).and_then(|body| self.doc.child(body, "sectPr"))
            } else {
                self.written.get(&id).or_else(|| self.nodes.get(&id)).copied().and_then(|node| self.doc.child(node, "pPr")).and_then(|ppr| self.doc.child(ppr, "sectPr"))
            };
            let Some(sect) = sect else { continue };
            use ibis_doc_model::SectionBreak;
            let value = match want {
                SectionBreak::Continuous => Some("continuous"),
                SectionBreak::OddPage => Some("oddPage"),
                SectionBreak::EvenPage => Some("evenPage"),
                SectionBreak::NextColumn => Some("nextColumn"),
                _ => None,
            };
            const SECT_ORDER: &[&str] = &["headerReference", "footerReference", "footnotePr", "endnotePr", "type", "pgSz", "pgMar", "paperSrc", "pgBorders", "lnNumType", "pgNumType", "cols", "formProt", "vAlign", "noEndnote", "titlePg", "textDirection", "bidi", "rtlGutter", "docGrid", "printerSettings", "sectPrChange"];
            self.val(sect, "type", value, SECT_ORDER);
        }
        Ok(())
    }

    /// The relationship a section's reference of `kind` names.
    fn reference(&self, sect: NodeId, kind: PartKind) -> Option<String> {
        let name = if kind.is_footer() { "footerReference" } else { "headerReference" };
        let wanted = kind.reference_type();
        self.doc
            .elements(sect)
            .find(|&e| self.doc.namespace(e) == Some(W_NS) && self.doc.local_name(e) == name && word_attr_name(self.doc, e, "type").and_then(|a| self.doc.attr(e, a)).unwrap_or("default") == wanted)
            .and_then(|e| namespace_attr_name(self.doc, e, "http://schemas.openxmlformats.org/officeDocument/2006/relationships", "id").and_then(|a| self.doc.attr(e, a)).map(str::to_string))
    }

    /// Point a section's reference of `kind` at relationship `rid`, or
    /// remove it.
    fn set_reference(&mut self, sect: NodeId, kind: PartKind, rid: Option<&str>) {
        let name = if kind.is_footer() { "footerReference" } else { "headerReference" };
        let wanted = kind.reference_type();
        let existing: Vec<NodeId> = self.doc.elements(sect).filter(|&e| self.doc.local_name(e) == name && self.doc.attr_local(e, "type").unwrap_or("default") == wanted).collect();
        for e in existing {
            self.doc.detach(e);
        }
        if let Some(rid) = rid {
            // References come first.
            let at = self.doc.children(sect).iter().position(|&c| self.doc.is_element(c) && !matches!(self.doc.local_name(c), "headerReference" | "footerReference")).unwrap_or(self.doc.children(sect).len());
            let e = self.doc.insert_element(sect, at, &format!("w:{name}"));
            self.doc.set_attr(e, "w:type", wanted);
            let r = self.doc.lookup_prefix(sect, "http://schemas.openxmlformats.org/officeDocument/2006/relationships").unwrap_or_else(|| "r".into());
            self.doc.set_attr(e, &format!("{r}:id"), rid);
        }
    }

    /// Write what differs between two settings of a section into its
    /// `w:sectPr`: the paper, the margins, the page numbers and the first
    /// page's own header.
    fn write_sect(&mut self, sect: NodeId, old: &SectView, new: &SectView) -> Result<(), Error> {
        const SECT_ORDER: &[&str] = &["headerReference", "footerReference", "footnotePr", "endnotePr", "type", "pgSz", "pgMar", "paperSrc", "pgBorders", "lnNumType", "pgNumType", "cols", "formProt", "vAlign", "noEndnote", "titlePg", "textDirection", "bidi", "rtlGutter", "docGrid", "printerSettings", "sectPrChange"];
        let (a, b) = (&old.page, &new.page);
        // (見開きページ and とじしろの位置 are the settings', written apart.)
        let same_rest = ibis_doc_model::PageSetup { size: a.size, top: a.top, right: a.right, bottom: a.bottom, left: a.left, header: a.header, footer: a.footer, first_page: a.first_page, gutter: a.gutter, v_align: a.v_align, mirror_margins: a.mirror_margins, gutter_at_top: a.gutter_at_top, multiple_pages: a.multiple_pages, book_fold_sheets: a.book_fold_sheets, footnotes: a.footnotes, endnotes: a.endnotes, direction: a.direction, line_numbers: a.line_numbers, columns: a.columns, borders: a.borders, borders_from_text: a.borders_from_text, line_pitch: a.line_pitch, char_space: a.char_space, doc_grid_type: a.doc_grid_type, ..*b } == *a;
        if !same_rest {
            return Err(unsupported("ページ設定の変更"));
        }
        let tw = |v: i64| (v / 635).to_string();
        if a.size != b.size {
            let e = self.ensure(sect, "pgSz", SECT_ORDER);
            self.doc.set_attr(e, "w:w", &tw(b.size.width));
            self.doc.set_attr(e, "w:h", &tw(b.size.height));
            if b.size.width > b.size.height {
                self.doc.set_attr(e, "w:orient", "landscape");
            } else {
                self.doc.remove_attr(e, "w:orient");
            }
        }
        let margins = [(a.top, b.top, "w:top"), (a.right, b.right, "w:right"), (a.bottom, b.bottom, "w:bottom"), (a.left, b.left, "w:left"), (a.header, b.header, "w:header"), (a.footer, b.footer, "w:footer"), (a.gutter, b.gutter, "w:gutter")];
        if margins.iter().any(|(x, y, _)| x != y) {
            let e = self.ensure(sect, "pgMar", SECT_ORDER);
            for (x, y, name) in margins {
                if x != y || self.doc.attr(e, name).is_none() {
                    self.doc.set_attr(e, name, &tw(y));
                }
            }
            if self.doc.attr(e, "w:gutter").is_none() {
                self.doc.set_attr(e, "w:gutter", "0");
            }
        }
        if old.restart != new.restart || old.format != new.format || old.chapter != new.chapter || (new.restart && a.first_page != b.first_page) {
            let e = self.ensure(sect, "pgNumType", SECT_ORDER);
            if new.restart {
                self.doc.set_attr(e, "w:start", &b.first_page.to_string());
            } else {
                self.doc.remove_attr(e, "w:start");
            }
            match &new.format {
                Some(f) => self.doc.set_attr(e, "w:fmt", f),
                None => self.doc.remove_attr(e, "w:fmt"),
            }
            match &new.chapter {
                Some((level, sep)) => {
                    self.doc.set_attr(e, "w:chapStyle", &level.to_string());
                    self.doc.set_attr(e, "w:chapSep", sep);
                }
                None => {
                    self.doc.remove_attr(e, "w:chapStyle");
                    self.doc.remove_attr(e, "w:chapSep");
                }
            }
            if self.doc.attributes(e).is_empty() {
                self.doc.detach(e);
            }
        }
        if old.title_page != new.title_page {
            self.toggle(sect, &["titlePg"], new.title_page.then_some(true), SECT_ORDER);
        }
        for (name, x, y) in [("footnotePr", a.footnotes, b.footnotes), ("endnotePr", a.endnotes, b.endnotes)] {
            if x == y {
                continue;
            }
            // pos, numFmt, numStart, numRestart (CT_FtnProps / CT_EdnProps).
            let e = self.ensure(sect, name, SECT_ORDER);
            for child in ["pos", "numFmt", "numStart", "numRestart"] {
                let found: Vec<_> = self.doc.children_named(e, child).collect();
                for c in found {
                    self.doc.detach(c);
                }
            }
            let pos = match y.position {
                ibis_doc_model::NotePosition::PageBottom => "pageBottom",
                ibis_doc_model::NotePosition::BeneathText => "beneathText",
                ibis_doc_model::NotePosition::SectionEnd => "sectEnd",
                ibis_doc_model::NotePosition::DocumentEnd => "docEnd",
            };
            let restart = match y.restart {
                ibis_doc_model::NoteRestart::Continuous => "continuous",
                ibis_doc_model::NoteRestart::EachSection => "eachSect",
                ibis_doc_model::NoteRestart::EachPage => "eachPage",
            };
            let values = [("pos", pos.to_string()), ("numFmt", y.format.ooxml().to_string()), ("numStart", y.start.to_string()), ("numRestart", restart.to_string())];
            for (i, (child, value)) in values.iter().enumerate() {
                let c = self.doc.insert_element(e, i, &format!("w:{child}"));
                self.doc.set_attr(c, "w:val", value);
            }
        }
        if a.direction != b.direction {
            let value = match b.direction {
                ibis_doc_model::CellDirection::TbRl => Some("tbRl"),
                ibis_doc_model::CellDirection::BtLr => Some("btLr"),
                ibis_doc_model::CellDirection::TbLrV => Some("tbLrV"),
                ibis_doc_model::CellDirection::LrTbV => Some("lrTbV"),
                ibis_doc_model::CellDirection::LrTb => None,
            };
            match value {
                Some(v) => {
                    let e = self.ensure(sect, "textDirection", SECT_ORDER);
                    self.doc.set_attr(e, "w:val", v);
                }
                None => {
                    let found = self.doc.children_named(sect, "textDirection").next();
                    if let Some(e) = found {
                        self.doc.detach(e);
                    }
                }
            }
        }
        if a.v_align != b.v_align {
            let value = match b.v_align {
                ibis_doc_model::PageVAlign::Top => None,
                ibis_doc_model::PageVAlign::Center => Some("center"),
                ibis_doc_model::PageVAlign::Bottom => Some("bottom"),
                ibis_doc_model::PageVAlign::Both => Some("both"),
            };
            match value {
                Some(v) => {
                    let e = self.ensure(sect, "vAlign", SECT_ORDER);
                    self.doc.set_attr(e, "w:val", v);
                }
                None => {
                    let found = self.doc.children_named(sect, "vAlign").next();
                    if let Some(e) = found {
                        self.doc.detach(e);
                    }
                }
            }
        }
        if a.line_numbers != b.line_numbers {
            match b.line_numbers {
                Some(ln) => {
                    let e = self.ensure(sect, "lnNumType", SECT_ORDER);
                    if ln.count_by != 1 {
                        self.doc.set_attr(e, "w:countBy", &ln.count_by.to_string());
                    } else {
                        self.doc.remove_attr(e, "w:countBy");
                    }
                    if ln.start != 1 {
                        self.doc.set_attr(e, "w:start", &(ln.start - 1).to_string());
                    } else {
                        self.doc.remove_attr(e, "w:start");
                    }
                    if ln.distance > 0 {
                        self.doc.set_attr(e, "w:distance", &(ln.distance / 635).to_string());
                    } else {
                        self.doc.remove_attr(e, "w:distance");
                    }
                    match ln.restart {
                        ibis_doc_model::LineNumberRestart::NewSection => {
                            self.doc.set_attr(e, "w:restart", "newSection");
                        }
                        ibis_doc_model::LineNumberRestart::Continuous => {
                            self.doc.set_attr(e, "w:restart", "continuous");
                        }
                        ibis_doc_model::LineNumberRestart::NewPage => {
                            self.doc.remove_attr(e, "w:restart");
                        }
                    }
                }
                None => {
                    let found = self.doc.children_named(sect, "lnNumType").next();
                    if let Some(e) = found {
                        self.doc.detach(e);
                    }
                }
            }
        }
        if a.columns != b.columns {
            let e = self.ensure(sect, "cols", SECT_ORDER);
            if b.columns.count > 1 {
                self.doc.set_attr(e, "w:num", &b.columns.count.to_string());
            } else {
                self.doc.remove_attr(e, "w:num");
            }
            if b.columns.space > 0 {
                self.doc.set_attr(e, "w:space", &tw(b.columns.space));
            } else {
                self.doc.remove_attr(e, "w:space");
            }
            if b.columns.separator && b.columns.count > 1 {
                self.doc.set_attr(e, "w:sep", "1");
            } else {
                self.doc.remove_attr(e, "w:sep");
            }
            let col_children: Vec<NodeId> = self.doc.children_named(e, "col").collect();
            for c in col_children {
                self.doc.detach(c);
            }
            if let Some(widths) = &b.columns.widths {
                if b.columns.count > 1 {
                    self.doc.set_attr(e, "w:equalWidth", "0");
                    let n = (b.columns.count as usize).clamp(1, ibis_doc_model::PageColumns::MAX);
                    for (i, &(w, sp)) in widths[..n].iter().enumerate() {
                        let col = self.doc.insert_element(e, i, "w:col");
                        self.doc.set_attr(col, "w:w", &tw(w));
                        if sp > 0 {
                            self.doc.set_attr(col, "w:space", &tw(sp));
                        }
                    }
                } else {
                    self.doc.remove_attr(e, "w:equalWidth");
                }
            } else {
                self.doc.remove_attr(e, "w:equalWidth");
            }
            if self.doc.attributes(e).is_empty() && self.doc.children(e).is_empty() {
                self.doc.detach(e);
            }
        }
        if a.borders != b.borders || a.borders_from_text != b.borders_from_text {
            const EDGES: &[&str] = &["top", "left", "bottom", "right"];
            if b.borders.iter().any(Option::is_some) {
                let e = self.ensure(sect, "pgBorders", SECT_ORDER);
                if b.borders_from_text {
                    self.doc.set_attr(e, "w:offsetFrom", "text");
                } else {
                    self.doc.remove_attr(e, "w:offsetFrom");
                }
                for (idx, edge_name) in [(0, "top"), (3, "left"), (2, "bottom"), (1, "right")] {
                    match &b.borders[idx] {
                        Some(border) => {
                            let edge = self.ensure(e, edge_name, EDGES);
                            if let Some(art) = border.art {
                                self.doc.set_attr(edge, "w:val", art.as_str());
                                self.doc.set_attr(edge, "w:sz", &(border.width / 12_700).to_string());
                                self.doc.set_attr(edge, "w:space", &(border.space / 12_700).to_string());
                                self.doc.set_attr(edge, "w:color", &format!("{:06X}", border.color.0));
                            } else {
                                self.doc.set_attr(edge, "w:val", paragraph_border_name(border.style));
                                self.doc.set_attr(edge, "w:sz", &((border.width * 8 + 6350) / 12_700).to_string());
                                self.doc.set_attr(edge, "w:space", &((border.space + 6350) / 12_700).to_string());
                                self.doc.set_attr(edge, "w:color", &format!("{:06X}", border.color.0));
                            }
                        }
                        None => {
                            self.remove_child(e, edge_name);
                        }
                    }
                }
            } else {
                let found = self.doc.children_named(sect, "pgBorders").next();
                if let Some(e) = found {
                    self.doc.detach(e);
                }
            }
        }
        if a.line_pitch != b.line_pitch || a.char_space != b.char_space || a.doc_grid_type != b.doc_grid_type {
            let is_default = b.doc_grid_type == ibis_doc_model::DocGridType::Default || (b.line_pitch.is_none() && b.char_space.is_none());
            if is_default {
                let found = self.doc.children_named(sect, "docGrid").next();
                if let Some(e) = found {
                    self.doc.set_attr(e, "w:type", "default");
                    self.doc.remove_attr(e, "w:linePitch");
                    self.doc.remove_attr(e, "w:charSpace");
                }
            } else {
                let e = self.ensure(sect, "docGrid", SECT_ORDER);
                let type_str = match b.doc_grid_type {
                    ibis_doc_model::DocGridType::Lines => "lines",
                    ibis_doc_model::DocGridType::LinesAndChars => "linesAndChars",
                    ibis_doc_model::DocGridType::SnapToChars => "snapToChars",
                    ibis_doc_model::DocGridType::Default => {
                        if b.char_space.is_some() {
                            "linesAndChars"
                        } else {
                            "lines"
                        }
                    }
                };
                self.doc.set_attr(e, "w:type", type_str);
                if let Some(pitch) = b.line_pitch {
                    self.doc.set_attr(e, "w:linePitch", &tw(pitch));
                } else {
                    self.doc.remove_attr(e, "w:linePitch");
                }
                if let Some(space) = b.char_space {
                    // ECMA-376 §17.6.5: charSpace is in 4096ths of a point
                    // (the reader in docx.rs converts it the same way).
                    let v = (space as f64 * 4096.0 / 12_700.0).round() as i64;
                    self.doc.set_attr(e, "w:charSpace", &v.to_string());
                } else {
                    self.doc.remove_attr(e, "w:charSpace");
                }
            }
        }
        Ok(())
    }

    /// The `w:tr` of a table element, and each row's cells (`w:tc`), as the
    /// reader listed them.
    fn rows_of(doc: &XmlDocument, tbl: NodeId) -> Vec<(NodeId, Vec<NodeId>)> {
        doc.children_named(tbl, "tr")
            .map(|tr| {
                let mut cells = Vec::new();
                for n in doc.elements(tr) {
                    match doc.local_name(n) {
                        "tc" => cells.push(n),
                        "sdt" => {
                            if let Some(c) = doc.child(n, "sdtContent") {
                                cells.extend(doc.children_named(c, "tc"));
                            }
                        }
                        _ => {}
                    }
                }
                (tr, cells)
            })
            .collect()
    }

    /// Write a changed table into its element: rows and cells added (copies
    /// of their neighbours') and removed, the grid, the cells' properties,
    /// and their contents.
    fn table(&mut self, node: NodeId, was: &FlowTable, t: &FlowTable) -> Result<(), Error> {
        if was.borders != t.borders {
            return Err(unsupported("表全体の書式の変更"));
        }
        const TBL_PR_ORDER: &[&str] = &["tblStyle", "tblpPr", "tblOverlap", "bidiVisual", "tblStyleRowBandSize", "tblStyleColBandSize", "tblW", "jc", "tblCellSpacing", "tblInd", "tblBorders", "shd", "tblLayout", "tblCellMar", "tblLook", "tblCaption", "tblDescription", "tblPrChange"];
        if was.align != t.align {
            let pr = match self.doc.child(node, "tblPr") {
                Some(p) => p,
                None => self.doc.insert_element(node, 0, &self.w("tblPr")),
            };
            let jc = self.ensure(pr, "jc", TBL_PR_ORDER);
            let val = match t.align {
                ibis_doc_model::FlowAlign::Center => "center",
                ibis_doc_model::FlowAlign::Right => "right",
                _ => "left",
            };
            self.doc.set_attr(jc, "w:val", val);
        }
        if was.indent != t.indent {
            let pr = match self.doc.child(node, "tblPr") {
                Some(p) => p,
                None => self.doc.insert_element(node, 0, &self.w("tblPr")),
            };
            let first_margin = t.rows.first().and_then(|r| r.cells.first()).map_or(0, |c| c.margins[3]);
            let ind = if self.map.table_indent_to_edge { t.indent } else { t.indent + first_margin };
            if ind != 0 {
                let e = self.ensure(pr, "tblInd", TBL_PR_ORDER);
                self.doc.set_attr(e, "w:w", &(ind / 635).to_string());
                self.doc.set_attr(e, "w:type", "dxa");
            } else if let Some(e) = self.doc.child(pr, "tblInd") {
                self.doc.detach(e);
            }
        }
        let source = self.map.tables.get(&t.id).ok_or_else(|| unsupported("元の場所が分からない表の変更"))?;
        let pristine_tbl = self.pristine.by_path(&source.path).ok_or_else(|| unsupported("表の変更"))?;
        let pristine_rows = Self::rows_of(self.pristine, pristine_tbl);
        let rows = Self::rows_of(self.doc, node);
        if rows.len() != was.rows.len() || rows.iter().zip(&was.rows).any(|((_, c), r)| c.len() != r.cells.len()) {
            return Err(unsupported("読み込みと形の合わない表の変更"));
        }
        if was.columns != t.columns {
            self.grid(node, &t.columns);
        }
        // 表スタイル (w:tblStyle)
        let was_style_id = was.style.as_ref().map(|s| s.id.as_str());
        let new_style_id = t.style.as_ref().map(|s| s.id.as_str());
        if was_style_id != new_style_id {
            let pr = match self.doc.child(node, "tblPr") {
                Some(p) => p,
                None => self.doc.insert_element(node, 0, &self.w("tblPr")),
            };
            if let Some(id) = new_style_id {
                let s = self.ensure(pr, "tblStyle", TBL_PR_ORDER);
                self.doc.set_attr(s, "w:val", id);
            } else if let Some(s) = self.doc.child(pr, "tblStyle") {
                self.doc.detach(s);
            }
        }
        // 表スタイルのオプション (w:tblLook): its attributes and the bits
        // of w:val (Word 2007's).
        let was_look = was.style.as_ref().map(|s| s.look);
        let new_look = t.style.as_ref().map(|s| s.look);
        if was_look != new_look && let Some(l) = new_look {
            let pr = match self.doc.child(node, "tblPr") {
                Some(p) => p,
                None => {
                    let p = self.doc.insert_element(node, 0, &self.w("tblPr"));
                    p
                }
            };
            let look = self.ensure(pr, "tblLook", TBL_PR_ORDER);
            let bits = [(l.first_row, 0x20, "w:firstRow"), (l.last_row, 0x40, "w:lastRow"), (l.first_column, 0x80, "w:firstColumn"), (l.last_column, 0x100, "w:lastColumn"), (l.no_hband, 0x200, "w:noHBand"), (l.no_vband, 0x400, "w:noVBand")];
            let val: u32 = bits.iter().filter(|b| b.0).map(|b| b.1).sum();
            self.doc.set_attr(look, "w:val", &format!("{val:04X}"));
            for (on, _, name) in bits {
                self.doc.set_attr(look, name, if on { "1" } else { "0" });
            }
        }
        // 表の配置 (w:jc)
        if was.align != t.align {
            const TBL_PR_ORDER: &[&str] = &["tblStyle", "tblpPr", "tblOverlap", "bidiVisual", "tblStyleRowBandSize", "tblStyleColBandSize", "tblW", "jc", "tblCellSpacing", "tblInd", "tblBorders", "shd", "tblLayout", "tblCellMar", "tblLook", "tblCaption", "tblDescription", "tblPrChange"];
            let pr = match self.doc.child(node, "tblPr") {
                Some(p) => p,
                None => self.doc.insert_element(node, 0, &self.w("tblPr")),
            };
            match t.align {
                ibis_doc_model::FlowAlign::Center => {
                    let jc = self.ensure(pr, "jc", TBL_PR_ORDER);
                    self.doc.set_attr(jc, "w:val", "center");
                }
                ibis_doc_model::FlowAlign::Right => {
                    let jc = self.ensure(pr, "jc", TBL_PR_ORDER);
                    self.doc.set_attr(jc, "w:val", "right");
                }
                _ => {
                    self.remove_child(pr, "jc");
                }
            }
        }
        // セル間隔 (w:tblCellSpacing)
        if was.cell_spacing != t.cell_spacing {
            const TBL_PR_ORDER: &[&str] = &["tblStyle", "tblpPr", "tblOverlap", "bidiVisual", "tblStyleRowBandSize", "tblStyleColBandSize", "tblW", "jc", "tblCellSpacing", "tblInd", "tblBorders", "shd", "tblLayout", "tblCellMar", "tblLook", "tblCaption", "tblDescription", "tblPrChange"];
            let pr = match self.doc.child(node, "tblPr") {
                Some(p) => p,
                None => self.doc.insert_element(node, 0, &self.w("tblPr")),
            };
            match t.cell_spacing {
                Some(sp) if sp > 0 => {
                    let e = self.ensure(pr, "tblCellSpacing", TBL_PR_ORDER);
                    self.doc.set_attr(e, "w:w", &(sp / 635).to_string());
                    self.doc.set_attr(e, "w:type", "dxa");
                }
                _ => {
                    self.remove_child(pr, "tblCellSpacing");
                }
            }
        }
        // 表の字下げ (w:tblInd)
        if was.indent != t.indent {
            const TBL_PR_ORDER: &[&str] = &["tblStyle", "tblpPr", "tblOverlap", "bidiVisual", "tblStyleRowBandSize", "tblStyleColBandSize", "tblW", "jc", "tblCellSpacing", "tblInd", "tblBorders", "shd", "tblLayout", "tblCellMar", "tblLook", "tblCaption", "tblDescription", "tblPrChange"];
            let pr = match self.doc.child(node, "tblPr") {
                Some(p) => p,
                None => self.doc.insert_element(node, 0, &self.w("tblPr")),
            };
            let first_margin = t.rows.first().and_then(|r| r.cells.first()).map_or(0, |c| c.margins[3]);
            let ind = if self.map.table_indent_to_edge { t.indent } else { t.indent + first_margin };
            if ind == 0 {
                self.remove_child(pr, "tblInd");
            } else {
                let e = self.ensure(pr, "tblInd", TBL_PR_ORDER);
                self.doc.set_attr(e, "w:w", &(ind / 635).to_string());
                self.doc.set_attr(e, "w:type", "dxa");
            }
        }
        // 浮動の表 (w:tblpPr)
        if was.floating != t.floating {
            const TBL_PR_ORDER: &[&str] = &["tblStyle", "tblpPr", "tblOverlap", "bidiVisual", "tblStyleRowBandSize", "tblStyleColBandSize", "tblW", "jc", "tblCellSpacing", "tblInd", "tblBorders", "shd", "tblLayout", "tblCellMar", "tblLook", "tblCaption", "tblDescription", "tblPrChange"];
            let pr = match self.doc.child(node, "tblPr") {
                Some(p) => p,
                None => self.doc.insert_element(node, 0, &self.w("tblPr")),
            };
            match &t.floating {
                Some(f) => {
                    let tblp = self.ensure(pr, "tblpPr", TBL_PR_ORDER);
                    Self::apply_table_float(&mut self.doc, tblp, f);
                }
                None => {
                    self.remove_child(pr, "tblpPr");
                }
            }
        }
        let old_rows: HashMap<u32, usize> = was.rows.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
        let new_rows: HashSet<u32> = t.rows.iter().map(|r| r.id).collect();
        for (i, r) in was.rows.iter().enumerate() {
            if !new_rows.contains(&r.id) {
                self.doc.detach(rows[i].0);
            }
        }
        let kept: Vec<usize> = t.rows.iter().filter_map(|r| old_rows.get(&r.id).copied()).collect();
        if kept.windows(2).any(|w| w[0] >= w[1]) {
            return Err(unsupported("表の行の並べ替え"));
        }
        let mut previous: Option<NodeId> = None;
        for (i, row) in t.rows.iter().enumerate() {
            let tr = match old_rows.get(&row.id) {
                Some(&r) => {
                    self.row_cells(rows[r].0, &was.rows[r], row, was, t, &rows[r].1, &pristine_rows[r].1)?;
                    self.row_props(rows[r].0, &was.rows[r], row);
                    rows[r].0
                }
                None => {
                    // A copy of the nearest old row, its cells written over.
                    let template = t.rows[..i].iter().rev().chain(t.rows[i + 1..].iter()).find_map(|r| old_rows.get(&r.id).copied());
                    let Some(template) = template else { return Err(unsupported("行をすべて置き換える表の変更")) };
                    let tr = self.doc.import(self.pristine, pristine_rows[template].0);
                    for attr in ["w14:paraId", "w14:textId"] {
                        self.doc.remove_attr(tr, attr);
                    }
                    match previous {
                        Some(prev) => self.doc.insert_after(prev, tr),
                        None => {
                            let first = rows.iter().find(|(n, _)| self.doc.parent(*n).is_some()).map(|(n, _)| *n).ok_or_else(|| unsupported("表の行の追加"))?;
                            self.doc.insert_before(first, tr);
                        }
                    }
                    let clone_cells = Self::rows_of(self.doc, node).into_iter().find(|(n, _)| *n == tr).map(|(_, c)| c).unwrap_or_default();
                    let template_row = &was.rows[template];
                    for (j, cell) in row.cells.iter().enumerate() {
                        let k = j.min(template_row.cells.len() - 1);
                        let tc = match clone_cells.get(j) {
                            Some(&tc) => tc,
                            None => {
                                let last = *clone_cells.last().ok_or_else(|| unsupported("表の行の追加"))?;
                                let tc = self.doc.deep_clone(last);
                                self.doc.insert_after(last, tc);
                                tc
                            }
                        };
                        self.new_cell(tc, &template_row.cells[k], was, cell, t, &pristine_rows[template].1[k])?;
                    }
                    for &extra in clone_cells.iter().skip(row.cells.len()) {
                        self.doc.detach(extra);
                    }
                    self.row_props(tr, template_row, row);
                    tr
                }
            };
            previous = Some(tr);
        }
        Ok(())
    }

    /// The table's grid (`w:tblGrid`) set to `columns`.
    fn grid(&mut self, tbl: NodeId, columns: &[i64]) {
        let grid = match self.doc.child(tbl, "tblGrid") {
            Some(g) => g,
            None => {
                let after = self.doc.child(tbl, "tblPr");
                let name = self.w("tblGrid");
                let g = self.doc.append_element(tbl, &name);
                self.doc.detach(g);
                match after {
                    Some(a) => self.doc.insert_after(a, g),
                    None => self.doc.insert(tbl, 0, g),
                }
                g
            }
        };
        for c in self.doc.children_named(grid, "gridCol").collect::<Vec<_>>() {
            self.doc.detach(c);
        }
        let name = self.w("gridCol");
        for &w in columns {
            let c = self.doc.append_element(grid, &name);
            self.doc.set_attr(c, "w:w", &(w / 635).to_string());
        }
    }

    /// The cells of a row kept: cells removed, added (copies of a
    /// neighbour), their properties and contents.
    #[allow(clippy::too_many_arguments)]
    fn row_cells(&mut self, tr: NodeId, was_row: &FlowRow, row: &FlowRow, was: &FlowTable, t: &FlowTable, cells: &[NodeId], pristine_cells: &[NodeId]) -> Result<(), Error> {
        let old_cells: HashMap<u32, usize> = was_row.cells.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
        let new_cells: HashSet<u32> = row.cells.iter().map(|c| c.id).collect();
        for (i, c) in was_row.cells.iter().enumerate() {
            if !new_cells.contains(&c.id) {
                self.doc.detach(cells[i]);
            }
        }
        let mut previous: Option<NodeId> = None;
        for (j, cell) in row.cells.iter().enumerate() {
            let tc = match old_cells.get(&cell.id) {
                Some(&c) => {
                    let old = &was_row.cells[c];
                    self.cell_props(cells[c], old, was, cell, t);
                    if old.blocks != cell.blocks {
                        self.list(&old.blocks, &cell.blocks)?;
                    }
                    cells[c]
                }
                None => {
                    let template = row.cells[..j].iter().rev().chain(row.cells[j + 1..].iter()).find_map(|c| old_cells.get(&c.id).copied());
                    let Some(template) = template else { return Err(unsupported("セルをすべて置き換える行の変更")) };
                    let tc = self.doc.import(self.pristine, pristine_cells[template]);
                    match previous {
                        Some(prev) => self.doc.insert_after(prev, tc),
                        None => {
                            let first = cells.iter().copied().find(|n| self.doc.parent(*n).is_some()).ok_or_else(|| unsupported("セルの追加"))?;
                            self.doc.insert_before(first, tc);
                        }
                    }
                    self.new_cell(tc, &was_row.cells[template], was, cell, t, &pristine_cells[template])?;
                    tc
                }
            };
            previous = Some(tc);
        }
        let _ = tr;
        Ok(())
    }

    /// The row's repeating as a header and its splitting (`w:trPr`), from
    /// `was`'s (the row's element is `was`'s or a copy of it).
    fn row_props(&mut self, tr: NodeId, was: &FlowRow, row: &FlowRow) {
        if was.header == row.header && was.cant_split == row.cant_split && was.height == row.height {
            return;
        }
        const TR_ORDER: &[&str] = &["tblPrEx", "trPr", "tc", "customXml", "sdt", "bookmarkStart"];
        const TR_PR_ORDER: &[&str] = &["cnfStyle", "divId", "gridBefore", "gridAfter", "wBefore", "wAfter", "cantSplit", "trHeight", "tblHeader", "tblCellSpacing", "jc", "hidden", "ins", "del", "trPrChange"];
        let pr = self.ensure(tr, "trPr", TR_ORDER);
        if was.height != row.height {
            match row.height {
                ibis_doc_model::RowHeight::Auto => self.remove_child(pr,"trHeight"),
                ibis_doc_model::RowHeight::AtLeast(height) | ibis_doc_model::RowHeight::Exact(height) => {
                    let e=self.ensure(pr,"trHeight",TR_PR_ORDER);
                    self.doc.set_attr(e,"w:val",&(height/635).to_string());
                    self.doc.set_attr(e,"w:hRule",if matches!(row.height,ibis_doc_model::RowHeight::Exact(_)) {"exact"} else {"atLeast"});
                }
            }
        }
        if was.cant_split != row.cant_split {
            self.toggle(pr, &["cantSplit"], row.cant_split.then_some(true), TR_PR_ORDER);
        }
        if was.header != row.header {
            self.toggle(pr, &["tblHeader"], row.header.then_some(true), TR_PR_ORDER);
        }
        if !self.doc.elements(pr).any(|_| true) && self.doc.attributes(pr).is_empty() {
            self.doc.detach(pr);
        }
    }

    /// A copied cell (`tc`, a copy of `template`'s element) written over
    /// with `cell`: its properties and new paragraphs.
    fn new_cell(&mut self, tc: NodeId, template: &FlowCell, was: &FlowTable, cell: &FlowCell, t: &FlowTable, _pristine: &NodeId) -> Result<(), Error> {
        self.cell_props(tc, template, was, cell, t);
        let template_para = template.blocks.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).ok_or_else(|| unsupported("段落の無いセルの写し"))?;
        let source = self.map.paragraphs.get(&template_para.id).ok_or_else(|| unsupported("セルの写し"))?;
        let original = self.pristine.by_path(&source.path).ok_or_else(|| unsupported("セルの写し"))?;
        for c in self.doc.children(tc).to_vec() {
            if self.doc.is_element(c) && self.doc.local_name(c) != "tcPr" {
                self.doc.detach(c);
            }
        }
        for block in &cell.blocks {
            match block {
                Block::Paragraph(p) => {
                    let node = self.doc.import(self.pristine, original);
                    let end = self.doc.children(tc).len();
                    self.doc.insert(tc, end, node);
                    self.write_copy(node, template_para, p, &source.atoms)?;
                }
                Block::Table(inner) => {
                    let node = self.new_table(inner, Some(template_para))?;
                    let end = self.doc.children(tc).len();
                    self.doc.insert(tc, end, node);
                }
            }
        }
        Ok(())
    }

    /// Write paragraph `new` into `node`, a copy of `template`'s element:
    /// its runs that hold none of the new text removed, and what must be
    /// unique taken away.
    fn write_copy(&mut self, node: NodeId, template: &FlowParagraph, new: &FlowParagraph, atoms: &[AtomSource]) -> Result<(), Error> {
        // The runs of the copy that hold none of the new paragraph's text (a
        // floating shape, a field's result…) stay with the original.
        let runs_before: Vec<NodeId> = self.doc.descendants(node).into_iter().filter(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "r").collect();
        let used = self.paragraph_runs(node, template, new, atoms)?;
        for r in runs_before {
            if !used.contains(&r) && self.doc.parent(r).is_some() {
                self.doc.detach(r);
            }
        }
        // What the copy must not keep goes before the new paragraph's own
        // marks (bookmarks, a field's start) are written.
        self.clean_copy(node);
        let template = FlowParagraph { bookmarks: Vec::new(), field_start: None, field_end: false, section_break: ibis_doc_model::SectionBreak::None, section: None, subdoc: None, ..template.clone() };
        self.paragraph_props(node, &template, new)?;
        Ok(())
    }

    /// Update one direct border, keeping unmodeled attributes on that edge.
    fn border_property(&mut self,parent:NodeId,name:&str,border:Option<Option<ibis_doc_model::FlowBorder>>,order:&[&str]) {
        let Some(border)=border else {self.remove_child(parent,name);return};
        let e=self.ensure(parent,name,order);
        for attr in ["w:themeColor","w:themeTint","w:themeShade"] {self.doc.remove_attr(e,attr);}
        if let Some(b)=border {
            self.doc.set_attr(e,"w:val",paragraph_border_name(b.style));
            self.doc.set_attr(e,"w:sz",&((b.width*8+6350)/12700).to_string());
            self.doc.set_attr(e,"w:color",&format!("{:06X}",b.color.0));
            self.doc.set_attr(e,"w:space",&((b.space+6350)/12700).to_string());
        } else {
            self.doc.set_attr(e,"w:val","nil");
            for attr in ["w:sz","w:color","w:space"] {self.doc.remove_attr(e,attr);}
        }
    }

    /// Write a cell's properties (`w:tcPr`) that differ from `old`'s.
    fn cell_props(&mut self, tc: NodeId, old: &FlowCell, was: &FlowTable, new: &FlowCell, t: &FlowTable) {
        const ORDER: &[&str] = &["cnfStyle", "tcW", "gridSpan", "hMerge", "vMerge", "tcBorders", "shd", "noWrap", "tcMar", "textDirection", "tcFitText", "vAlign", "hideMark", "headers", "cellIns", "cellDel", "cellMerge", "tcPrChange"];
        let width = |table: &FlowTable, row_cell: &FlowCell| -> i64 {
            // The cell's grid columns: found by its span only (the start
            // column is not kept), so the width is written when it changed.
            let _ = table;
            row_cell.span as i64
        };
        let _ = width;
        let pr = match self.doc.child(tc, "tcPr") {
            Some(p) => p,
            None => {
                let name = self.w("tcPr");
                self.doc.insert_element(tc, 0, &name)
            }
        };
        if old.span != new.span {
            let v = (new.span > 1).then(|| new.span.to_string());
            self.val(pr, "gridSpan", v.as_deref(), ORDER);
        }
        if old.merge != new.merge {
            match new.merge {
                VerticalMerge::None => self.remove_child(pr, "vMerge"),
                VerticalMerge::Start => self.val(pr, "vMerge", Some("restart"), ORDER),
                VerticalMerge::Continue => {
                    let e = self.ensure(pr, "vMerge", ORDER);
                    self.doc.remove_attr(e, "w:val");
                }
            }
        }
        if old.shading != new.shading {
            match new.shading {
                Some(c) => {
                    let e = self.ensure(pr, "shd", ORDER);
                    for a in ["w:themeFill", "w:themeFillTint", "w:themeFillShade"] {
                        self.doc.remove_attr(e, a);
                    }
                    self.doc.set_attr(e, "w:val", "clear");
                    self.doc.set_attr(e, "w:color", "auto");
                    self.doc.set_attr(e, "w:fill", &format!("{:06X}", c.0));
                }
                None => self.remove_child(pr, "shd"),
            }
        }
        if old.own_borders!=new.own_borders || old.diagonals!=new.diagonals {
            let borders=self.ensure(pr,"tcBorders",ORDER);
            const EDGES:&[&str]=&["top","start","left","bottom","end","right","insideH","insideV","tl2br","tr2bl"];
            for (i,name) in ["top","right","bottom","left"].into_iter().enumerate() {
                if old.own_borders[i]!=new.own_borders[i] {self.border_property(borders,name,new.own_borders[i],EDGES);}
            }
            for (i,name) in ["tl2br","tr2bl"].into_iter().enumerate() {
                if old.diagonals[i]!=new.diagonals[i] {self.border_property(borders,name,Some(new.diagonals[i]),EDGES);}
            }
            if !self.doc.elements(borders).any(|_|true) && self.doc.attributes(borders).is_empty() {self.doc.detach(borders);}
        }
        if old.anchor != new.anchor {
            let v = match new.anchor {
                ibis_doc_model::CellAnchor::Top => None,
                ibis_doc_model::CellAnchor::Center => Some("center"),
                ibis_doc_model::CellAnchor::Bottom => Some("bottom"),
            };
            self.val(pr, "vAlign", v, ORDER);
        }
        if old.direction != new.direction {
            let v = match new.direction {
                ibis_doc_model::CellDirection::LrTb => None,
                ibis_doc_model::CellDirection::TbRl => Some("tbRl"),
                ibis_doc_model::CellDirection::BtLr => Some("btLr"),
                ibis_doc_model::CellDirection::LrTbV => Some("lrTbV"),
                ibis_doc_model::CellDirection::TbLrV => Some("tbLrV"),
            };
            self.val(pr, "textDirection", v, ORDER);
        }
        if old.margins != new.margins {
            if new.margins == [0, 108 * 635, 0, 108 * 635] {
                self.remove_child(pr, "tcMar");
            } else {
                let mar = self.ensure(pr, "tcMar", ORDER);
                for c in self.doc.children(mar).to_vec() {
                    self.doc.detach(c);
                }
                for (name, val) in [("top", new.margins[0]), ("left", new.margins[3]), ("bottom", new.margins[2]), ("right", new.margins[1])] {
                    let e = self.doc.append_element(mar, &self.w(name));
                    self.doc.set_attr(e, "w:w", &(val / 635).to_string());
                    self.doc.set_attr(e, "w:type", "dxa");
                }
            }
        }
        if old.no_wrap != new.no_wrap {
            if new.no_wrap {
                self.ensure(pr, "noWrap", ORDER);
            } else {
                self.remove_child(pr, "noWrap");
            }
        }
        // The cell's width follows the grid.
        if was.columns != t.columns || old.span != new.span || old.id != new.id {
            let row_width: i64 = t.columns.iter().sum();
            let _ = row_width;
            if let Some(w) = self.cell_width(t, new) {
                let e = self.ensure(pr, "tcW", ORDER);
                self.doc.set_attr(e, "w:w", &(w / 635).to_string());
                self.doc.set_attr(e, "w:type", "dxa");
            }
        }
    }

    /// A cell's width: the grid columns it covers in its row.
    fn cell_width(&self, t: &FlowTable, cell: &FlowCell) -> Option<i64> {
        let row = t.rows.iter().find(|r| r.cells.iter().any(|c| c.id == cell.id))?;
        let mut at = 0usize;
        for c in &row.cells {
            let span = c.span.max(1) as usize;
            if c.id == cell.id {
                return Some(t.columns.iter().skip(at).take(span).sum());
            }
            at += span;
        }
        None
    }

    /// The element of a new table, its cells' paragraphs written from
    /// `template`'s element.
    fn new_table(&mut self, t: &FlowTable, template: Option<&FlowParagraph>) -> Result<NodeId, Error> {
        // The paragraph the cells' are copied from (none in a new header).
        let copied = match template {
            Some(template) => {
                let source = self.map.paragraphs.get(&template.id).ok_or_else(|| unsupported("表の追加"))?;
                Some((template, source.atoms.clone(), self.pristine.by_path(&source.path).ok_or_else(|| unsupported("表の追加"))?))
            }
            None => None,
        };
        let w = |l: &str| format!("w:{l}");
        let tbl = self.doc.append_element(XmlDocument::ROOT, &w("tbl"));
        self.doc.detach(tbl);
        let pr = self.doc.append_element(tbl, &w("tblPr"));
        if let Some(ref st) = t.style {
            let s = self.doc.append_element(pr, &w("tblStyle"));
            self.doc.set_attr(s, "w:val", &st.id);
        }
        if let Some(f) = &t.floating {
            let tblp = self.doc.append_element(pr, &w("tblpPr"));
            Self::apply_table_float(&mut self.doc, tblp, f);
        }
        let width = self.doc.append_element(pr, &w("tblW"));
        self.doc.set_attr(width, "w:w", "0");
        self.doc.set_attr(width, "w:type", "auto");
        if t.align != ibis_doc_model::FlowAlign::Left {
            let val = match t.align {
                ibis_doc_model::FlowAlign::Center => "center",
                ibis_doc_model::FlowAlign::Right => "right",
                _ => "left",
            };
            let jc = self.doc.append_element(pr, &w("jc"));
            self.doc.set_attr(jc, "w:val", val);
        }
        if let Some(sp) = t.cell_spacing.filter(|&s| s > 0) {
            let e = self.doc.append_element(pr, &w("tblCellSpacing"));
            self.doc.set_attr(e, "w:w", &(sp / 635).to_string());
            self.doc.set_attr(e, "w:type", "dxa");
        }
        // Before Word 2013, the indent is to the first cell's text.
        let first_margin = t.rows.first().and_then(|r| r.cells.first()).map_or(0, |c| c.margins[3]);
        let ind = if self.map.table_indent_to_edge { t.indent } else { t.indent + first_margin };
        if ind != 0 {
            let e = self.doc.append_element(pr, &w("tblInd"));
            self.doc.set_attr(e, "w:w", &(ind / 635).to_string());
            self.doc.set_attr(e, "w:type", "dxa");
        }
        let borders = self.doc.append_element(pr, &w("tblBorders"));
        for (i, name) in ["top", "left", "bottom", "right", "insideH", "insideV"].into_iter().enumerate() {
            // Model order: top, right, bottom, left, insideH, insideV.
            let b = t.borders[[0, 3, 2, 1, 4, 5][i]].flatten();
            let e = self.doc.append_element(borders, &w(name));
            match b {
                Some(b) => {
                    self.doc.set_attr(e, "w:val", "single");
                    self.doc.set_attr(e, "w:sz", &(b.width * 8 / 12_700).to_string());
                    self.doc.set_attr(e, "w:space", "0");
                    self.doc.set_attr(e, "w:color", &format!("{:06X}", b.color.0));
                }
                None => self.doc.set_attr(e, "w:val", "nil"),
            }
        }
        // The last child of tblPr (the schema's order): the look of the table style.
        let look = self.doc.append_element(pr, &w("tblLook"));
        if let Some(ref st) = t.style {
            let l = st.look;
            let bits = [(l.first_row, 0x20, "w:firstRow"), (l.last_row, 0x40, "w:lastRow"), (l.first_column, 0x80, "w:firstColumn"), (l.last_column, 0x100, "w:lastColumn"), (l.no_hband, 0x200, "w:noHBand"), (l.no_vband, 0x400, "w:noVBand")];
            let val: u32 = bits.iter().filter(|b| b.0).map(|b| b.1).sum();
            self.doc.set_attr(look, "w:val", &format!("{val:04X}"));
            for (on, _, name) in bits {
                self.doc.set_attr(look, name, if on { "1" } else { "0" });
            }
        } else {
            self.doc.set_attr(look, "w:val", "04A0");
        }
        self.grid(tbl, &t.columns);
        for row in &t.rows {
            let tr = self.doc.append_element(tbl, &w("tr"));
            self.row_props(tr, &FlowRow::default(), row);
            for cell in &row.cells {
                let tc = self.doc.append_element(tr, &w("tc"));
                let tc_pr = self.doc.append_element(tc, &w("tcPr"));
                let tw = self.doc.append_element(tc_pr, &w("tcW"));
                self.doc.set_attr(tw, "w:w", &(self.cell_width(t, cell).unwrap_or(0) / 635).to_string());
                self.doc.set_attr(tw, "w:type", "dxa");
                if cell.span > 1 {
                    let e = self.doc.append_element(tc_pr, &w("gridSpan"));
                    self.doc.set_attr(e, "w:val", &cell.span.to_string());
                }
                if cell.direction != ibis_doc_model::CellDirection::LrTb {
                    let d = match cell.direction {
                        ibis_doc_model::CellDirection::TbRl => "tbRl",
                        ibis_doc_model::CellDirection::BtLr => "btLr",
                        ibis_doc_model::CellDirection::LrTbV => "lrTbV",
                        ibis_doc_model::CellDirection::TbLrV => "tbLrV",
                        ibis_doc_model::CellDirection::LrTb => "",
                    };
                    let e = self.doc.append_element(tc_pr, &w("textDirection"));
                    self.doc.set_attr(e, "w:val", d);
                }
                if cell.no_wrap {
                    self.doc.append_element(tc_pr, &w("noWrap"));
                }
                if cell.margins != [0, 108 * 635, 0, 108 * 635] {
                    let tc_mar = self.doc.append_element(tc_pr, &w("tcMar"));
                    for (name, val) in [("top", cell.margins[0]), ("left", cell.margins[3]), ("bottom", cell.margins[2]), ("right", cell.margins[1])] {
                        let e = self.doc.append_element(tc_mar, &w(name));
                        self.doc.set_attr(e, "w:w", &(val / 635).to_string());
                        self.doc.set_attr(e, "w:type", "dxa");
                    }
                }
                for block in &cell.blocks {
                    match block {
                        Block::Paragraph(p) => match &copied {
                            Some((template, atoms, original)) => {
                                let node = self.doc.import(self.pristine, *original);
                                let end = self.doc.children(tc).len();
                                self.doc.insert(tc, end, node);
                                self.write_copy(node, template, p, atoms)?;
                            }
                            None => {
                                let node = self.doc.append_element(tc, &w("p"));
                                let blank = FlowParagraph { id: p.id, ..FlowParagraph::default() };
                                self.paragraph(node, &blank, p, &[])?;
                            }
                        },
                        Block::Table(inner) => {
                            let node = self.new_table(inner, template)?;
                            let end = self.doc.children(tc).len();
                            self.doc.insert(tc, end, node);
                        }
                    }
                }
            }
        }
        Ok(tbl)
    }

    fn apply_table_float(doc: &mut XmlDocument, tblp: NodeId, f: &ibis_doc_model::TableFloat) {
        let horz = match f.horz_anchor {
            ibis_doc_model::TableAnchor::Margin => "margin",
            ibis_doc_model::TableAnchor::Page => "page",
            ibis_doc_model::TableAnchor::Text => "text",
        };
        let vert = match f.vert_anchor {
            ibis_doc_model::TableAnchor::Margin => "margin",
            ibis_doc_model::TableAnchor::Page => "page",
            ibis_doc_model::TableAnchor::Text => "text",
        };
        doc.set_attr(tblp, "w:horzAnchor", horz);
        doc.set_attr(tblp, "w:vertAnchor", vert);

        if let Some(spec) = f.x_align {
            let val = match spec {
                ibis_doc_model::TableAlign::Start => "left",
                ibis_doc_model::TableAlign::Center => "center",
                ibis_doc_model::TableAlign::End => "right",
                ibis_doc_model::TableAlign::Inside => "inside",
                ibis_doc_model::TableAlign::Outside => "outside",
            };
            doc.set_attr(tblp, "w:tblpXSpec", val);
        } else {
            doc.set_attr(tblp, "w:tblpX", &(f.x / 635).to_string());
        }

        if let Some(spec) = f.y_align {
            let val = match spec {
                ibis_doc_model::TableAlign::Start => "top",
                ibis_doc_model::TableAlign::Center => "center",
                ibis_doc_model::TableAlign::End => "bottom",
                ibis_doc_model::TableAlign::Inside => "inside",
                ibis_doc_model::TableAlign::Outside => "outside",
            };
            doc.set_attr(tblp, "w:tblpYSpec", val);
        } else {
            doc.set_attr(tblp, "w:tblpY", &(f.y / 635).to_string());
        }

        doc.set_attr(tblp, "w:leftFromText", &(f.distance[0] / 635).to_string());
        doc.set_attr(tblp, "w:rightFromText", &(f.distance[1] / 635).to_string());
        doc.set_attr(tblp, "w:topFromText", &(f.distance[2] / 635).to_string());
        doc.set_attr(tblp, "w:bottomFromText", &(f.distance[3] / 635).to_string());
    }

    /// A picture's size set in its drawing (`wp:extent`, and the picture's
    /// `a:ext`), when it is not that already.
    fn set_picture_size(&mut self, drawing: NodeId, cx: i64, cy: i64) {
        let (cx, cy) = (cx.to_string(), cy.to_string());
        for n in self.doc.descendants(drawing) {
            if !self.doc.is_element(n) {
                continue;
            }
            let local = self.doc.local_name(n).to_string();
            let is_ext = local == "extent" || (local == "ext" && self.doc.parent(n).is_some_and(|p| self.doc.local_name(p) == "xfrm"));
            if is_ext && (self.doc.attr(n, "cx") != Some(cx.as_str()) || self.doc.attr(n, "cy") != Some(cy.as_str())) {
                self.doc.set_attr(n, "cx", &cx);
                self.doc.set_attr(n, "cy", &cy);
            }
        }
    }

    /// The picture's turn, flips, cut and outline: `a:xfrm`'s rot, flipH and flipV,
    /// `pic:blipFill/a:srcRect` and `pic:spPr/a:ln`, as `look` says (left out when nothing is
    /// changed, as Word writes them). What `look` already says is not touched.
    fn set_picture_look(&mut self, drawing: NodeId, look: &ibis_doc_model::PictureLook) {
        if let Some(doc_pr) = self.doc.descendants(drawing).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "docPr") {
            const DEC_URI: &str = "{C183D7F6-B498-43B3-948B-1728B52AA6E4}";
            if look.decorative {
                self.doc.remove_attr(doc_pr, "descr");
                self.doc.remove_attr(doc_pr, "title");
                let ext_lst = self.doc.child(doc_pr, "extLst").unwrap_or_else(|| {
                    self.doc.append_element(doc_pr, "a:extLst")
                });
                if self.doc.resolve_prefix(ext_lst, Some("a")).is_none() {
                    self.doc.set_attr(ext_lst, "xmlns:a", "http://schemas.openxmlformats.org/drawingml/2006/main");
                }
                let ext = self.doc.children(ext_lst).to_vec().into_iter().find(|&c| {
                    self.doc.is_element(c) && self.doc.local_name(c) == "ext" && self.doc.attr(c, "uri") == Some(DEC_URI)
                }).unwrap_or_else(|| {
                    let e = self.doc.append_element(ext_lst, "a:ext");
                    self.doc.set_attr(e, "uri", DEC_URI);
                    e
                });
                let dec = self.doc.children(ext).to_vec().into_iter().find(|&c| {
                    self.doc.is_element(c) && self.doc.local_name(c) == "decorative"
                }).unwrap_or_else(|| {
                    let d = self.doc.append_element(ext, "a1611:decorative");
                    self.doc.set_attr(d, "xmlns:a1611", "http://schemas.microsoft.com/office/drawing/2016/11/main");
                    d
                });
                self.doc.set_attr(dec, "val", "1");
            } else {
                if look.alt_text.description.is_empty() {
                    self.doc.remove_attr(doc_pr, "descr");
                } else {
                    self.doc.set_attr(doc_pr, "descr", &look.alt_text.description);
                }
                if look.alt_text.title.is_empty() {
                    self.doc.remove_attr(doc_pr, "title");
                } else {
                    self.doc.set_attr(doc_pr, "title", &look.alt_text.title);
                }
                if let Some(ext_lst) = self.doc.child(doc_pr, "extLst") {
                    let to_remove: Vec<_> = self.doc.children(ext_lst).to_vec().into_iter().filter(|&c| {
                        self.doc.is_element(c) && self.doc.local_name(c) == "ext" && (
                            self.doc.attr(c, "uri") == Some(DEC_URI) ||
                            self.doc.descendants(c).into_iter().any(|d| self.doc.is_element(d) && self.doc.local_name(d) == "decorative")
                        )
                    }).collect();
                    for ext in to_remove {
                        self.doc.detach(ext);
                    }
                    if self.doc.elements(ext_lst).next().is_none() {
                        self.doc.detach(ext_lst);
                    }
                }
            }
            if !look.name.is_empty() {
                self.doc.set_attr(doc_pr, "name", &look.name);
            }
        }
        let Some(pic) = self.doc.descendants(drawing).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "pic") else { return };
        if let Some(xfrm) = self.doc.child(pic, "spPr").and_then(|p| self.doc.child(p, "xfrm")) {
            let set = |doc: &mut XmlDocument, name: &str, value: Option<&str>| match value {
                Some(v) => {
                    if doc.attr(xfrm, name) != Some(v) {
                        doc.set_attr(xfrm, name, v)
                    }
                }
                None => {
                    if doc.attr(xfrm, name).is_some() {
                        doc.remove_attr(xfrm, name)
                    }
                }
            };
            // The old value counts as no change when it says the same.
            let old_rot = self.doc.attr(xfrm, "rot").and_then(|v| v.parse::<i64>().ok()).map_or(0, |v| v.rem_euclid(21_600_000) as i32);
            let rot = look.rotation.to_string();
            if old_rot != look.rotation {
                set(&mut self.doc, "rot", (look.rotation != 0).then_some(rot.as_str()));
            }
            for (name, on) in [("flipH", look.flip_h), ("flipV", look.flip_v)] {
                let was = self.doc.attr(xfrm, name).is_some_and(|v| v == "1" || v == "true");
                if was != on {
                    set(&mut self.doc, name, on.then_some("1"));
                }
            }
        }
        if let Some(fill) = self.doc.child(pic, "blipFill") {
            let c = look.crop;
            let existing = self.doc.child(fill, "srcRect");
            let old = existing.map(|n| {
                let side = |doc: &XmlDocument, name: &str| doc.attr(n, name).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
                ibis_doc_model::Crop { left: side(&self.doc, "l"), top: side(&self.doc, "t"), right: side(&self.doc, "r"), bottom: side(&self.doc, "b") }
            });
            if old.unwrap_or_default() != c {
                let rect = match existing {
                    Some(n) => n,
                    None => {
                        let at = self.doc.children(fill).iter().position(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "blip").map_or(0, |i| i + 1);
                        self.doc.insert_element(fill, at, "a:srcRect")
                    }
                };
                if c == ibis_doc_model::Crop::default() {
                    self.doc.detach(rect);
                } else {
                    for (name, v) in [("l", c.left), ("t", c.top), ("r", c.right), ("b", c.bottom)] {
                        if v != 0 {
                            self.doc.set_attr(rect, name, &v.to_string());
                        } else if self.doc.attr(rect, name).is_some() {
                            self.doc.remove_attr(rect, name);
                        }
                    }
                }
            }
        }
        // 明るさ・コントラスト (a:lum) and 色 (a:duotone) in the blip, before its extension list.
        if let Some(blip) = self.doc.child(pic, "blipFill").and_then(|f| self.doc.child(f, "blip")) {
            let read_lum = |doc: &XmlDocument, e: NodeId| {
                let v = |n: &str| doc.attr(e, n).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
                (v("bright"), v("contrast"))
            };
            let old_lum = self.doc.child(blip, "lum").map(|e| read_lum(self.doc, e)).filter(|l| *l != (0, 0));
            if old_lum != look.luminance {
                if let Some(e) = self.doc.child(blip, "lum") {
                    self.doc.detach(e);
                }
                if let Some((bright, contrast)) = look.luminance {
                    let at = self.doc.children(blip).iter().position(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "extLst").unwrap_or_else(|| self.doc.children(blip).len());
                    let e = self.doc.insert_element(blip, at, "a:lum");
                    if bright != 0 {
                        self.doc.set_attr(e, "bright", &bright.to_string());
                    }
                    if contrast != 0 {
                        self.doc.set_attr(e, "contrast", &contrast.to_string());
                    }
                }
            }
            let srgb_pair = |doc: &XmlDocument, e: NodeId| -> Option<(ibis_doc_model::Rgb, ibis_doc_model::Rgb)> {
                let colors: Vec<ibis_doc_model::Rgb> = doc.children(e).iter().filter(|&&c| doc.is_element(c) && doc.local_name(c) == "srgbClr").filter_map(|&c| doc.attr(c, "val").and_then(|v| u32::from_str_radix(v, 16).ok()).map(ibis_doc_model::Rgb)).collect();
                Some((*colors.first()?, *colors.get(1)?))
            };
            let existing = self.doc.child(blip, "duotone");
            // A duotone in other colours than sRGB ones (theme colours) is left as it is unless the
            // picture's changed to another.
            let old_duotone = existing.and_then(|e| srgb_pair(self.doc, e)).or(existing.map(|_| look.duotone).flatten());
            if existing.is_some() != look.duotone.is_some() || old_duotone != look.duotone {
                if let Some(e) = existing {
                    self.doc.detach(e);
                }
                if let Some((dark, light)) = look.duotone {
                    let at = self.doc.children(blip).iter().position(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "extLst").unwrap_or_else(|| self.doc.children(blip).len());
                    let e = self.doc.insert_element(blip, at, "a:duotone");
                    for c in [dark, light] {
                        let child = self.doc.append_element(e, "a:srgbClr");
                        self.doc.set_attr(child, "val", &format!("{:06X}", c.0 & 0xffffff));
                    }
                }
            }
            // 透明度 (a:alphaModFix)
            let existing_alpha = self.doc.child(blip, "alphaModFix");
            let old_alpha = existing_alpha.and_then(|e| self.doc.attr(e, "amt")).and_then(|v| v.parse::<i32>().ok());
            if old_alpha != look.effects.opacity {
                if let Some(e) = existing_alpha {
                    self.doc.detach(e);
                }
                if let Some(amt) = look.effects.opacity {
                    let at = self.doc.children(blip).iter().position(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "extLst").unwrap_or_else(|| self.doc.children(blip).len());
                    let e = self.doc.insert_element(blip, at, "a:alphaModFix");
                    self.doc.set_attr(e, "amt", &amt.to_string());
                }
            }
            // 彩度 (a:satMod)
            let existing_sat = self.doc.child(blip, "satMod");
            let old_sat = existing_sat.and_then(|e| self.doc.attr(e, "amt")).and_then(|v| v.parse::<i32>().ok());
            if old_sat != look.effects.saturation {
                if let Some(e) = existing_sat {
                    self.doc.detach(e);
                }
                if let Some(amt) = look.effects.saturation {
                    let at = self.doc.children(blip).iter().position(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "extLst").unwrap_or_else(|| self.doc.children(blip).len());
                    let e = self.doc.insert_element(blip, at, "a:satMod");
                    self.doc.set_attr(e, "amt", &amt.to_string());
                }
            }
            // 白黒 (a:biLevel)
            let existing_bilevel = self.doc.child(blip, "biLevel");
            let old_bilevel = existing_bilevel.and_then(|e| self.doc.attr(e, "thresh")).and_then(|v| v.parse::<i32>().ok());
            if old_bilevel != look.effects.bilevel {
                if let Some(e) = existing_bilevel {
                    self.doc.detach(e);
                }
                if let Some(thresh) = look.effects.bilevel {
                    let at = self.doc.children(blip).iter().position(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "extLst").unwrap_or_else(|| self.doc.children(blip).len());
                    let e = self.doc.insert_element(blip, at, "a:biLevel");
                    self.doc.set_attr(e, "thresh", &thresh.to_string());
                }
            }
            // グレースケール (a:grayscl)
            let existing_grayscl = self.doc.child(blip, "grayscl");
            let old_grayscl = existing_grayscl.is_some();
            if old_grayscl != look.effects.grayscale {
                if let Some(e) = existing_grayscl {
                    self.doc.detach(e);
                }
                if look.effects.grayscale {
                    let at = self.doc.children(blip).iter().position(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "extLst").unwrap_or_else(|| self.doc.children(blip).len());
                    self.doc.insert_element(blip, at, "a:grayscl");
                }
            }
        }
        if let Some(sp_pr) = self.doc.child(pic, "spPr") {
            let existing_ln = self.doc.child(sp_pr, "ln");
            match &look.stroke {
                None => {
                    if let Some(ln) = existing_ln {
                        self.doc.detach(ln);
                    }
                }
                Some(stroke) => {
                    let ln = match existing_ln {
                        Some(ln) => ln,
                        None => {
                            let at = self.doc.children(sp_pr).iter().position(|&n| {
                                self.doc.is_element(n) && matches!(self.doc.local_name(n), "effectLst" | "effectDag" | "scene3d" | "sp3d" | "extLst")
                            }).unwrap_or_else(|| self.doc.children(sp_pr).len());
                            self.doc.insert_element(sp_pr, at, "a:ln")
                        }
                    };
                    self.doc.set_attr(ln, "w", &stroke.width.to_string());
                    let rgb_hex = format!("{:06X}", stroke.color.rgb.0);
                    let fill = self.doc.children(ln).iter().copied().find(|&n| self.doc.is_element(n) && matches!(self.doc.local_name(n), "solidFill" | "noFill" | "gradFill"));
                    let solid_fill = match fill {
                        Some(f) if self.doc.local_name(f) == "solidFill" => f,
                        Some(f) => {
                            let at = self.doc.children(ln).iter().position(|&c| c == f).unwrap_or(0);
                            self.doc.detach(f);
                            self.doc.insert_element(ln, at, "a:solidFill")
                        }
                        None => self.doc.insert_element(ln, 0, "a:solidFill"),
                    };
                    let srgb = self.doc.child(solid_fill, "srgbClr");
                    match srgb {
                        Some(s) => self.doc.set_attr(s, "val", &rgb_hex),
                        None => {
                            for c in self.doc.children(solid_fill).to_vec() {
                                self.doc.detach(c);
                            }
                            let s = self.doc.append_element(solid_fill, "a:srgbClr");
                            self.doc.set_attr(s, "val", &rgb_hex);
                        }
                    }
                    let existing_dash = self.doc.child(ln, "prstDash");
                    if stroke.dash == ibis_doc_model::LineDash::Solid {
                        if let Some(d) = existing_dash {
                            self.doc.detach(d);
                        }
                    } else {
                        let dash_val = stroke.dash.name();
                        match existing_dash {
                            Some(d) => self.doc.set_attr(d, "val", dash_val),
                            None => {
                                let d = self.doc.append_element(ln, "a:prstDash");
                                self.doc.set_attr(d, "val", dash_val);
                            }
                        }
                    }
                }
            }
            // ぼかし・ソフトエッジ (a:softEdge in a:effectLst)
            let existing_effect_lst = self.doc.child(sp_pr, "effectLst");
            match look.soft_edge {
                None => {
                    if let Some(effect_lst) = existing_effect_lst {
                        if let Some(se) = self.doc.child(effect_lst, "softEdge") {
                            self.doc.detach(se);
                        }
                        if self.doc.elements(effect_lst).next().is_none() {
                            self.doc.detach(effect_lst);
                        }
                    }
                }
                Some(rad) => {
                    let effect_lst = match existing_effect_lst {
                        Some(e) => e,
                        None => {
                            let at = self.doc.children(sp_pr).iter().position(|&n| {
                                self.doc.is_element(n) && matches!(self.doc.local_name(n), "effectDag" | "scene3d" | "sp3d" | "extLst")
                            }).unwrap_or_else(|| self.doc.children(sp_pr).len());
                            self.doc.insert_element(sp_pr, at, "a:effectLst")
                        }
                    };
                    let se = match self.doc.child(effect_lst, "softEdge") {
                        Some(se) => se,
                        None => self.doc.append_element(effect_lst, "a:softEdge"),
                    };
                    self.doc.set_attr(se, "rad", &rad.to_string());
                }
            }
            // 図形に合わせてトリミング (a:prstGeom in spPr)
            let target_prst = look.geometry.as_ref().and_then(|g| g.preset_name()).unwrap_or("rect");
            let existing_prst = self.doc.child(sp_pr, "prstGeom");
            let prst_node = match existing_prst {
                Some(g) => {
                    self.doc.set_attr(g, "prst", target_prst);
                    g
                }
                None => {
                    let at = self.doc.children(sp_pr).iter().position(|&n| {
                        self.doc.is_element(n) && matches!(self.doc.local_name(n), "noFill" | "solidFill" | "gradFill" | "blipFill" | "pattFill" | "grpFill" | "ln" | "effectLst" | "effectDag" | "scene3d" | "sp3d" | "extLst")
                    }).unwrap_or_else(|| self.doc.children(sp_pr).len());
                    let g = self.doc.insert_element(sp_pr, at, "a:prstGeom");
                    self.doc.set_attr(g, "prst", target_prst);
                    g
                }
            };
            let av_lst = match self.doc.child(prst_node, "avLst") {
                Some(av) => av,
                None => self.doc.append_element(prst_node, "a:avLst"),
            };
            if let Some(ibis_doc_model::Geometry::RoundRect { radius }) = &look.geometry {
                let gd = match self.doc.child(av_lst, "gd") {
                    Some(gd) => gd,
                    None => self.doc.append_element(av_lst, "a:gd"),
                };
                self.doc.set_attr(gd, "name", "adj");
                self.doc.set_attr(gd, "fmla", &format!("val {radius}"));
            } else {
                for c in self.doc.children(av_lst).to_vec() {
                    self.doc.detach(c);
                }
            }
        }
    }

    /// A new inline picture (`w:drawing`) of the media related as `rid`.
    /// A new text box: an anchored `wps:wsp` in a `w:drawing` (inside
    /// `mc:AlternateContent`, as Word writes it), the frame, fill and line of
    /// `shape` and `body`'s text.
    fn new_shape(&mut self, shape: &Shape, text: Option<&ibis_doc_model::TextBody>) -> NodeId {
        let default_body = ibis_doc_model::TextBody::default();
        let body = text.unwrap_or(&default_body);
        let px = drawing_prefixes();
        let geometry = match &shape.kind {
            ShapeKind::Auto(a) => crate::save::pptx_new::geometry_xml(&px, &a.geometry)
                .unwrap_or_else(|_| r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#.to_string()),
            ShapeKind::Line(_) => r#"<a:prstGeom prst="line"><a:avLst/></a:prstGeom>"#.to_string(),
            _ => r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#.to_string(),
        };
        let is_ink = text.is_none()
            && matches!(&shape.kind, ShapeKind::Auto(a) if matches!(&a.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none()));
        // A line, or a shape of an elbow or curved connector's geometry, is a connector (`wps:cNvCnPr`, with what it is glued to).
        let is_line = matches!(shape.kind, ShapeKind::Line(_)) || matches!(&shape.kind, ShapeKind::Auto(a) if a.geometry.preset_name().is_some_and(|n| n.contains("Connector")));
        let connector_pr = {
            let glue = shape.floating.as_ref().map(|f| f.glue).unwrap_or_default();
            let end = |tag: &str, e: Option<(u64, u32)>| e.map(|(id, idx)| format!(r#"<a:{tag} id="{id}" idx="{idx}"/>"#)).unwrap_or_default();
            format!("<wps:cNvCnPr>{}{}</wps:cNvCnPr>", end("stCxn", glue.start), end("endCxn", glue.end))
        };
        let flips = format!("{}{}", if shape.flip_h { r#" flipH="1""# } else { "" }, if shape.flip_v { r#" flipV="1""# } else { "" });
        let root = self.doc.root_element().unwrap_or(XmlDocument::ROOT);
        let used_ids: std::collections::HashSet<u64> = self.doc.descendants(root).into_iter().filter(|&n| self.doc.is_element(n) && matches!(self.doc.local_name(n), "docPr" | "cNvPr")).filter_map(|n| self.doc.attr(n, "id").and_then(|v| v.parse::<u64>().ok())).collect();
        // The shape's own number when it is free (connectors name the shapes they are glued to by it), else the next one.
        let id = if shape.id.0 > 0 && !used_ids.contains(&shape.id.0) { shape.id.0 } else { used_ids.iter().max().copied().unwrap_or(0) + 1 };
        let (cx, cy) = (shape.frame.width, shape.frame.height);
        let (fill, line) = match &shape.kind {
            ShapeKind::Auto(a) => (
                crate::save::pptx_new::fill_xml(&px, &a.fill).unwrap_or_else(|_| "<a:noFill/>".to_string()),
                crate::save::pptx_new::line_xml(&px, a.stroke.as_ref()) + &crate::save::pptx_new::auto_effects_xml(&px, a).unwrap_or_default(),
            ),
            ShapeKind::Line(stroke) => (String::new(), crate::save::pptx_new::line_xml(&px, Some(stroke))),
            _ => ("<a:noFill/>".to_string(), "<a:ln><a:noFill/></a:ln>".to_string()),
        };
        let half = (body.style.font_size / 50).max(2);
        let esc = |t: &str| t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let first = body.paragraphs.iter().flat_map(|p| &p.runs).find(|r| !r.text.is_empty());
        let bold = first.and_then(|r| r.style.bold).unwrap_or(body.style.bold);
        let italic = first.and_then(|r| r.style.italic).unwrap_or(body.style.italic);
        let underline = first.is_some_and(|r| r.style.underline);
        let underline_kind = first.map_or(ibis_doc_model::Underline::Single, |r| r.style.underline_kind);
        let color = first.and_then(|r| r.style.color).unwrap_or(body.style.color);
        let half = first.and_then(|r| r.style.font_size).map_or(half, |v| (v / 50).max(2));
        let u_xml = |on: bool, kind: ibis_doc_model::Underline| {
            if on {
                format!(r#"<w:u w:val="{}"/>"#, ibis_doc_model::UnderlineStyle::from_drawing_underline(kind).name())
            } else {
                String::new()
            }
        };
        // CT_RPr order: b, i, color, sz, szCs, u.
        let rpr = format!(
            "{}{}{}{}<w:sz w:val=\"{half}\"/><w:szCs w:val=\"{half}\"/>",
            if bold { "<w:b/><w:bCs/>" } else { "" },
            if italic { "<w:i/><w:iCs/>" } else { "" },
            u_xml(underline, underline_kind),
            if color != ibis_doc_model::TextStyle::default().color { format!("<w:color w:val=\"{:06X}\"/>", color.0 & 0xffffff) } else { String::new() },
        );
        // Each run with its own look when the paragraphs have runs that differ (a part of the text bold…); else a run per line.
        let looks = run_looks(body);
        let paragraphs: String = if looks.iter().any(|p| p.len() > 1) && looks.len() == body.paragraphs.len() {
            looks
                .iter()
                .map(|p| {
                    let runs: String = p
                        .iter()
                        .map(|(t, b, i, u, uk, half, c)| {
                            let rpr = format!(
                                "{}{}{}{}<w:sz w:val=\"{half}\"/><w:szCs w:val=\"{half}\"/>",
                                if *b { "<w:b/><w:bCs/>" } else { "" },
                                if *i { "<w:i/><w:iCs/>" } else { "" },
                                u_xml(*u, *uk),
                                if *c != ibis_doc_model::TextStyle::default().color.0 & 0xffffff { format!("<w:color w:val=\"{c:06X}\"/>") } else { String::new() },
                            );
                            format!(r#"<w:r><w:rPr>{rpr}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#, esc(t))
                        })
                        .collect();
                    format!("<w:p>{runs}</w:p>")
                })
                .collect()
        } else {
            body.text().split('\n').map(|l| format!(r#"<w:p><w:r><w:rPr>{rpr}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#, esc(l))).collect()
        };
        let i = body.insets;
        let anchor_val = match text.map(|b| b.anchor).unwrap_or(ibis_doc_model::VerticalAnchor::Top) {
            ibis_doc_model::VerticalAnchor::Top => "t",
            ibis_doc_model::VerticalAnchor::Middle => "ctr",
            ibis_doc_model::VerticalAnchor::Bottom => "b",
        };
        let xml = format!(
            concat!(
                r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><mc:Choice Requires="wps">"#,
                r#"<w:drawing xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#,
                r#"<wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="{id}" name="{name} {id}"/><wp:cNvGraphicFramePr/>"#,
                r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp>{csp}"#,
                r#"<wps:spPr><a:xfrm{flips}><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>{geometry}{fill}{line}</wps:spPr>"#,
                "{txbx}",
                r#"<wps:bodyPr rot="0" vert="horz" wrap="square" lIns="{l}" tIns="{t}" rIns="{r}" bIns="{b}" anchor="{anchor}" anchorCtr="0"><a:noAutofit/></wps:bodyPr>"#,
                r#"</wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice></mc:AlternateContent>"#
            ),
            cx = cx, cy = cy, id = id, fill = fill, line = line, geometry = geometry, flips = flips, l = i.left, t = i.top, r = i.right, b = i.bottom, anchor = anchor_val,
            // As Word names them, in the UI's language.
            name = match (crate::english_names(), text.is_some(), is_line, is_ink) {
                (false, true, _, _) => "テキスト ボックス", // i18n-ok（文書に保存する名前。ibis-pptx は表を持たない）
                (false, false, true, _) => "直線", // i18n-ok（同上）
                (false, false, false, true) => "インク", // i18n-ok（同上）
                (false, false, false, false) => "図形", // i18n-ok（同上）
                (true, true, _, _) => "Text Box",
                (true, false, true, _) => "Straight Connector",
                (true, false, false, true) => "Ink",
                (true, false, false, false) => "Shape",
            },
            csp = if text.is_some() { r#"<wps:cNvSpPr txBox="1"/>"# } else if is_line { connector_pr.as_str() } else { r#"<wps:cNvSpPr/>"# },
            txbx = if text.is_some() { format!("<wps:txbx><w:txbxContent>{paragraphs}</w:txbxContent></wps:txbx>") } else { String::new() },
        );
        let parsed = XmlDocument::parse(xml.as_bytes()).expect("the shape's XML");
        let alt = parsed.root_element().expect("the text box");
        let node = self.doc.import(&parsed, alt);
        // The prefixes the part declares already need no declaration here.
        for (holder, prefix, ns) in [(node, "mc", "http://schemas.openxmlformats.org/markup-compatibility/2006")] {
            if self.doc.lookup_prefix(root, ns).as_deref() == Some(prefix) {
                self.doc.remove_attr(holder, &format!("xmlns:{prefix}"));
            }
        }
        if let Some(drawing) = self.doc.descendants(node).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "drawing") {
            for (prefix, ns) in [("w", "http://schemas.openxmlformats.org/wordprocessingml/2006/main"), ("wp", "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing")] {
                if self.doc.lookup_prefix(root, ns).as_deref() == Some(prefix) {
                    self.doc.remove_attr(drawing, &format!("xmlns:{prefix}"));
                }
            }
            self.configure_drawing_anchor(drawing, shape.floating.as_ref());
        }
        node
    }

    fn wsp_xml(&self, shape: &Shape, text: Option<&ibis_doc_model::TextBody>, id: u64, off_x: i64, off_y: i64, include_cnvpr: bool) -> String {
        let default_body = ibis_doc_model::TextBody::default();
        let body = text.unwrap_or(&default_body);
        let px = drawing_prefixes();
        let geometry = match &shape.kind {
            ShapeKind::Auto(a) => crate::save::pptx_new::geometry_xml(&px, &a.geometry)
                .unwrap_or_else(|_| r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#.to_string()),
            ShapeKind::Line(_) => r#"<a:prstGeom prst="line"><a:avLst/></a:prstGeom>"#.to_string(),
            _ => r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#.to_string(),
        };
        let is_ink = text.is_none()
            && matches!(&shape.kind, ShapeKind::Auto(a) if matches!(&a.geometry, ibis_doc_model::Geometry::Formula(f) if f.preset.is_none()));
        let is_line = matches!(shape.kind, ShapeKind::Line(_)) || matches!(&shape.kind, ShapeKind::Auto(a) if a.geometry.preset_name().is_some_and(|n| n.contains("Connector")));
        let connector_pr = {
            let glue = shape.floating.as_ref().map(|f| f.glue).unwrap_or_default();
            let end = |tag: &str, e: Option<(u64, u32)>| e.map(|(id, idx)| format!(r#"<a:{tag} id="{id}" idx="{idx}"/>"#)).unwrap_or_default();
            format!("<wps:cNvCnPr>{}{}</wps:cNvCnPr>", end("stCxn", glue.start), end("endCxn", glue.end))
        };
        let flips = format!(
            "{}{}{}",
            if shape.rotation != 0 { format!(r#" rot="{}""#, shape.rotation) } else { String::new() },
            if shape.flip_h { r#" flipH="1""# } else { "" },
            if shape.flip_v { r#" flipV="1""# } else { "" },
        );
        let (cx, cy) = (shape.frame.width, shape.frame.height);
        let (fill, line) = match &shape.kind {
            ShapeKind::Auto(a) => (
                crate::save::pptx_new::fill_xml(&px, &a.fill).unwrap_or_else(|_| "<a:noFill/>".to_string()),
                crate::save::pptx_new::line_xml(&px, a.stroke.as_ref()) + &crate::save::pptx_new::auto_effects_xml(&px, a).unwrap_or_default(),
            ),
            ShapeKind::Line(stroke) => (String::new(), crate::save::pptx_new::line_xml(&px, Some(stroke))),
            _ => ("<a:noFill/>".to_string(), "<a:ln><a:noFill/></a:ln>".to_string()),
        };
        let half = (body.style.font_size / 50).max(2);
        let esc = |t: &str| t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let first = body.paragraphs.iter().flat_map(|p| &p.runs).find(|r| !r.text.is_empty());
        let bold = first.and_then(|r| r.style.bold).unwrap_or(body.style.bold);
        let italic = first.and_then(|r| r.style.italic).unwrap_or(body.style.italic);
        let underline = first.is_some_and(|r| r.style.underline);
        let underline_kind = first.map_or(ibis_doc_model::Underline::Single, |r| r.style.underline_kind);
        let color = first.and_then(|r| r.style.color).unwrap_or(body.style.color);
        let half = first.and_then(|r| r.style.font_size).map_or(half, |v| (v / 50).max(2));
        let u_xml = |on: bool, kind: ibis_doc_model::Underline| {
            if on {
                format!(r#"<w:u w:val="{}"/>"#, ibis_doc_model::UnderlineStyle::from_drawing_underline(kind).name())
            } else {
                String::new()
            }
        };
        let rpr = format!(
            "{}{}{}{}<w:sz w:val=\"{half}\"/><w:szCs w:val=\"{half}\"/>",
            if bold { "<w:b/><w:bCs/>" } else { "" },
            if italic { "<w:i/><w:iCs/>" } else { "" },
            u_xml(underline, underline_kind),
            if color != ibis_doc_model::TextStyle::default().color { format!("<w:color w:val=\"{:06X}\"/>", color.0 & 0xffffff) } else { String::new() },
        );
        let looks = run_looks(body);
        let paragraphs: String = if looks.iter().any(|p| p.len() > 1) && looks.len() == body.paragraphs.len() {
            looks
                .iter()
                .map(|p| {
                    let runs: String = p
                        .iter()
                        .map(|(t, b, i, u, uk, half, c)| {
                            let rpr = format!(
                                "{}{}{}{}<w:sz w:val=\"{half}\"/><w:szCs w:val=\"{half}\"/>",
                                if *b { "<w:b/><w:bCs/>" } else { "" },
                                if *i { "<w:i/><w:iCs/>" } else { "" },
                                u_xml(*u, *uk),
                                if *c != ibis_doc_model::TextStyle::default().color.0 & 0xffffff { format!("<w:color w:val=\"{c:06X}\"/>") } else { String::new() },
                            );
                            format!(r#"<w:r><w:rPr>{rpr}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#, esc(t))
                        })
                        .collect();
                    format!("<w:p>{runs}</w:p>")
                })
                .collect()
        } else {
            body.text().split('\n').map(|l| format!(r#"<w:p><w:r><w:rPr>{rpr}</w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#, esc(l))).collect()
        };
        let i = body.insets;
        let anchor_val = match text.map(|b| b.anchor).unwrap_or(ibis_doc_model::VerticalAnchor::Top) {
            ibis_doc_model::VerticalAnchor::Top => "t",
            ibis_doc_model::VerticalAnchor::Middle => "ctr",
            ibis_doc_model::VerticalAnchor::Bottom => "b",
        };
        let name = match (crate::english_names(), text.is_some(), is_line, is_ink) {
            (false, true, _, _) => "テキスト ボックス", // i18n-ok（文書に保存する名前。ibis-pptx は表を持たない）
            (false, false, true, _) => "直線", // i18n-ok（同上）
            (false, false, false, true) => "インク", // i18n-ok（同上）
            (false, false, false, false) => "図形", // i18n-ok（同上）
            (true, true, _, _) => "Text Box",
            (true, false, true, _) => "Straight Connector",
            (true, false, false, true) => "Ink",
            (true, false, false, false) => "Shape",
        };
        let cnvpr = if include_cnvpr {
            format!(r#"<wps:cNvPr id="{id}" name="{name} {id}"/>"#)
        } else {
            String::new()
        };
        let csp = if text.is_some() { r#"<wps:cNvSpPr txBox="1"/>"# } else if is_line { connector_pr.as_str() } else { r#"<wps:cNvSpPr/>"# };
        let txbx = if text.is_some() { format!("<wps:txbx><w:txbxContent>{paragraphs}</w:txbxContent></wps:txbx>") } else { String::new() };
        format!(
            concat!(
                "<wps:wsp>{cnvpr}{csp}",
                r#"<wps:spPr><a:xfrm{flips}><a:off x="{off_x}" y="{off_y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>{geometry}{fill}{line}</wps:spPr>"#,
                "{txbx}",
                r#"<wps:bodyPr rot="0" vert="horz" wrap="square" lIns="{l}" tIns="{t}" rIns="{r}" bIns="{b}" anchor="{anchor}" anchorCtr="0"><a:noAutofit/></wps:bodyPr>"#,
                "</wps:wsp>"
            ),
            cnvpr = cnvpr, csp = csp, flips = flips, off_x = off_x, off_y = off_y, cx = cx, cy = cy,
            geometry = geometry, fill = fill, line = line, txbx = txbx,
            l = i.left, t = i.top, r = i.right, b = i.bottom, anchor = anchor_val,
        )
    }

    /// A new group shape (`<wpg:wgp>` inside `<mc:AlternateContent>`), containing `shapes`.
    fn new_group_shape(&mut self, group: &ibis_doc_model::ShapeGroup, shapes: &[&Shape]) -> NodeId {
        let root = self.doc.root_element().unwrap_or(XmlDocument::ROOT);
        let mut used_ids: std::collections::HashSet<u64> = self
            .doc
            .descendants(root)
            .into_iter()
            .filter(|&n| self.doc.is_element(n) && matches!(self.doc.local_name(n), "docPr" | "cNvPr"))
            .filter_map(|n| self.doc.attr(n, "id").and_then(|v| v.parse::<u64>().ok()))
            .collect();

        let pos_of = |s: &Shape| -> (i64, i64) {
            s.floating.as_ref().map_or((s.frame.x, s.frame.y), |f| (f.offset_x, f.offset_y))
        };
        let min_x = shapes.iter().map(|s| pos_of(s).0).min().unwrap_or(0);
        let min_y = shapes.iter().map(|s| pos_of(s).1).min().unwrap_or(0);
        let max_x = shapes.iter().map(|s| pos_of(s).0 + s.frame.width).max().unwrap_or(min_x + 1);
        let max_y = shapes.iter().map(|s| pos_of(s).1 + s.frame.height).max().unwrap_or(min_y + 1);
        let group_cx = (max_x - min_x).max(1);
        let group_cy = (max_y - min_y).max(1);

        let group_id = group
            .native_id
            .parse::<u64>()
            .ok()
            .filter(|&id| id > 0 && !used_ids.contains(&id))
            .unwrap_or_else(|| used_ids.iter().max().copied().unwrap_or(0) + 1);
        used_ids.insert(group_id);
        let group_name = if !group.name.is_empty() {
            group.name.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
        } else if crate::english_names() {
            format!("Group {group_id}")
        } else {
            format!("グループ {group_id}") // i18n-ok（文書に保存する名前。ibis-pptx は表を持たない）
        };

        let mut children_xml = String::new();
        for s in shapes {
            let child_id = if s.id.0 > 0 && !used_ids.contains(&s.id.0) {
                s.id.0
            } else {
                used_ids.iter().max().copied().unwrap_or(0) + 1
            };
            used_ids.insert(child_id);
            let (sx, sy) = pos_of(s);
            let off_x = sx - min_x;
            let off_y = sy - min_y;
            children_xml.push_str(&self.wsp_xml(s, shape_body(s), child_id, off_x, off_y, true));
        }

        let xml = format!(
            concat!(
                r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><mc:Choice Requires="wpg">"#,
                r#"<w:drawing xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#,
                r#"<wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="{id}" name="{name}"/><wp:cNvGraphicFramePr/>"#,
                r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:wgp><wpg:cNvGrpSpPr/>"#,
                r#"<wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/><a:chOff x="0" y="0"/><a:chExt cx="{cx}" cy="{cy}"/></a:xfrm></wpg:grpSpPr>"#,
                "{children}",
                r#"</wpg:wgp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice></mc:AlternateContent>"#
            ),
            cx = group_cx,
            cy = group_cy,
            id = group_id,
            name = group_name,
            children = children_xml,
        );
        let parsed = XmlDocument::parse(xml.as_bytes()).expect("the group shape's XML");
        let alt = parsed.root_element().expect("the group shape");
        let node = self.doc.import(&parsed, alt);
        for (holder, prefix, ns) in [(node, "mc", "http://schemas.openxmlformats.org/markup-compatibility/2006")] {
            if self.doc.lookup_prefix(root, ns).as_deref() == Some(prefix) {
                self.doc.remove_attr(holder, &format!("xmlns:{prefix}"));
            }
        }
        let mut group_anchor = shapes
            .first()
            .and_then(|s| s.floating.clone())
            .unwrap_or_else(|| ibis_doc_model::FloatingAnchor {
                relative_h: "column".to_string(),
                relative_v: "paragraph".to_string(),
                wrap: "none".to_string(),
                allow_overlap: true,
                layout_in_cell: true,
                relative_height: ibis_doc_model::FloatingAnchor::DEFAULT_RELATIVE_HEIGHT,
                ..Default::default()
            });
        group_anchor.offset_x = min_x;
        group_anchor.offset_y = min_y;
        group_anchor.align_h = None;
        group_anchor.align_v = None;
        if let Some(drawing) = self.doc.descendants(node).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "drawing") {
            for (prefix, ns) in [("w", "http://schemas.openxmlformats.org/wordprocessingml/2006/main"), ("wp", "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing")] {
                if self.doc.lookup_prefix(root, ns).as_deref() == Some(prefix) {
                    self.doc.remove_attr(drawing, &format!("xmlns:{prefix}"));
                }
            }
            self.configure_drawing_anchor(drawing, Some(&group_anchor));
        }
        node
    }

    /// An edited text box: `node` (a copy of its old element) gets the size
    /// and anchor of `shape` and the text of `body`, the formatting of its
    /// paragraphs and runs kept as far as they go.
    fn update_shape(&mut self, node: NodeId, shape: &Shape, body: Option<&ibis_doc_model::TextBody>, old: Option<&Shape>) {
        let old_body = old.and_then(shape_body);
        self.update_shape_look(node, shape, old);
        if let Some(body) = body {
            self.update_text_box(node, shape, body, old_body);
        } else {
            self.update_shape_frame(node, shape);
        }
    }

    /// The size and anchor of a shape's old element.
    fn update_shape_frame(&mut self, node: NodeId, shape: &Shape) {
        let drawing = if self.doc.local_name(node) == "drawing" { Some(node) } else { self.doc.descendants(node).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "drawing") };
        if let Some(drawing) = drawing {
            self.set_picture_size(drawing, shape.frame.width, shape.frame.height);
            if shape.floating.is_some() {
                self.configure_drawing_anchor(drawing, shape.floating.as_ref());
            }
        }
        self.sync_connector(node, shape);
    }

    /// A connector's flips and what its ends are glued to (`a:stCxn`, `a:endCxn`), as the shape says.
    fn sync_connector(&mut self, node: NodeId, shape: &Shape) {
        let Some(cnv) = self.doc.descendants(node).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "cNvCnPr") else { return };
        if let Some(xfrm) = self.doc.descendants(node).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "xfrm") {
            for (name, on) in [("flipH", shape.flip_h), ("flipV", shape.flip_v)] {
                if on {
                    self.doc.set_attr(xfrm, name, "1");
                } else {
                    self.doc.remove_attr(xfrm, name);
                }
            }
        }
        let glue = shape.floating.as_ref().map(|f| f.glue).unwrap_or_default();
        for (tag, end) in [("stCxn", glue.start), ("endCxn", glue.end)] {
            if let Some(old) = self.doc.child(cnv, tag) {
                self.doc.detach(old);
            }
            if let Some((id, idx)) = end {
                let e = self.doc.append_element(cnv, &format!("a:{tag}"));
                self.doc.set_attr(e, "id", &id.to_string());
                self.doc.set_attr(e, "idx", &idx.to_string());
            }
        }
    }

    /// The paragraphs of the text box in `node` written run by run as `body` has its runs (the paragraph properties and the first
    /// run's properties of each paragraph are kept). Nothing happens when the paragraphs are not the same in number.
    fn write_runs_by_look(&mut self, node: NodeId, body: &ibis_doc_model::TextBody) {
        let Some(content) = self.doc.descendants(node).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "txbxContent") else { return };
        let paragraphs: Vec<NodeId> = self.doc.children_named(content, "p").collect();
        if paragraphs.len() != body.paragraphs.len() {
            return;
        }
        for (p, model) in paragraphs.into_iter().zip(&body.paragraphs) {
            let first_run = self.doc.children_named(p, "r").next();
            let old_pr = first_run.and_then(|r| self.doc.child(r, "rPr"));
            let template = old_pr.map(|n| self.doc.deep_clone(n));
            for c in self.doc.children(p).to_vec() {
                if !(self.doc.is_element(c) && self.doc.local_name(c) == "pPr") {
                    self.doc.detach(c);
                }
            }
            for run in model.runs.iter().filter(|r| !r.text.is_empty()) {
                let r = self.doc.append_element(p, &self.w("r"));
                let rpr = match &template {
                    Some(t) => {
                        let copy = self.doc.deep_clone(*t);
                        self.doc.insert(r, 0, copy);
                        copy
                    }
                    None => self.doc.append_element(r, &self.w("rPr")),
                };
                let o = RPR_ORDER;
                self.toggle(rpr, &["b", "bCs"], Some(run.style.bold.unwrap_or(body.style.bold)), o);
                self.toggle(rpr, &["i", "iCs"], Some(run.style.italic.unwrap_or(body.style.italic)), o);
                if run.style.underline {
                    self.val(rpr, "u", Some(ibis_doc_model::UnderlineStyle::from_drawing_underline(run.style.underline_kind).name()), o);
                } else if self.doc.child(rpr, "u").is_some() {
                    self.val(rpr, "u", Some("none"), o);
                }
                let half = ((run.style.font_size.unwrap_or(body.style.font_size) + 25) / 50).to_string();
                self.val(rpr, "sz", Some(&half), o);
                self.val(rpr, "szCs", Some(&half), o);
                let color = run.style.color.unwrap_or(body.style.color);
                if color != ibis_doc_model::TextStyle::default().color || self.doc.child(rpr, "color").is_some() {
                    self.val(rpr, "color", Some(&format!("{:06X}", color.0 & 0xffffff)), o);
                }
                let t = self.doc.append_element(r, &self.w("t"));
                if needs_space_preserve(&run.text) {
                    self.doc.set_attr(t, "xml:space", "preserve");
                }
                self.doc.set_text(t, &run.text);
            }
        }
    }

    /// The fill, the line and the effects (a shadow, a glow…) of a shape's old element, when the shape's differ from the old one's.
    fn update_shape_look(&mut self, node: NodeId, shape: &Shape, old: Option<&Shape>) {
        let (new_fill, new_stroke, old_fill, old_stroke, new_auto, old_auto) = match (&shape.kind, old.map(|o| &o.kind)) {
            (ShapeKind::Auto(n), Some(ShapeKind::Auto(o))) => (Some(&n.fill), n.stroke.as_ref(), Some(&o.fill), o.stroke.as_ref(), Some(n), Some(o)),
            (ShapeKind::Line(n), Some(ShapeKind::Line(o))) => (None, Some(n), None, Some(o), None, None),
            _ => return,
        };
        let geometry_changed = match (new_auto, old_auto) {
            (Some(n), Some(o)) => n.geometry != o.geometry,
            _ => false,
        };
        let effects_changed = match (new_auto, old_auto) {
            (Some(n), Some(o)) => (&n.shadow, &n.glow, &n.inner_shadow, &n.soft_edge, &n.reflection) != (&o.shadow, &o.glow, &o.inner_shadow, &o.soft_edge, &o.reflection),
            _ => false,
        };
        if new_fill == old_fill && new_stroke == old_stroke && !effects_changed && !geometry_changed {
            return;
        }
        let Some(sp_pr) = self.doc.descendants(node).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "spPr" && self.doc.parent(n).is_some_and(|p| matches!(self.doc.local_name(p), "wsp" | "sp"))) else { return };
        let fragment = |w: &mut Self, xml: String| -> NodeId {
            let xml = format!(r#"<w:holder xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{xml}</w:holder>"#);
            let parsed = XmlDocument::parse(xml.as_bytes()).expect("the fill's XML");
            let holder = parsed.root_element().expect("the holder");
            let imported = w.doc.import(&parsed, holder);
            let inner = w.doc.children(imported).iter().copied().find(|&c| w.doc.is_element(c)).expect("the element");
            w.doc.detach(inner);
            inner
        };
        // After the geometry: the fill, then the line, then the effects (CT_ShapeProperties).
        let position = |w: &Self, anchors: &[&str]| w.doc.children(sp_pr).iter().rposition(|&c| w.doc.is_element(c) && anchors.contains(&w.doc.local_name(c))).map_or(0, |i| i + 1);
        if geometry_changed && let Some(auto) = new_auto {
            for c in self.doc.children(sp_pr).to_vec() {
                if self.doc.is_element(c) && matches!(self.doc.local_name(c), "custGeom" | "prstGeom") {
                    self.doc.detach(c);
                }
            }
            if let Ok(xml) = crate::save::pptx_new::geometry_xml(&drawing_prefixes(), &auto.geometry) {
                let element = fragment(self, xml);
                let at = position(self, &["xfrm"]);
                self.doc.insert(sp_pr, at, element);
            }
        }
        if new_fill != old_fill {
            for c in self.doc.children(sp_pr).to_vec() {
                if self.doc.is_element(c) && matches!(self.doc.local_name(c), "noFill" | "solidFill" | "gradFill" | "blipFill" | "pattFill" | "grpFill") {
                    self.doc.detach(c);
                }
            }
            let xml = match new_fill {
                Some(fill) => crate::save::pptx_new::fill_xml(&drawing_prefixes(), fill).unwrap_or_else(|_| "<a:noFill/>".to_string()),
                None => "<a:noFill/>".to_string(),
            };
            let element = fragment(self, xml);
            let at = position(self, &["xfrm", "custGeom", "prstGeom"]);
            self.doc.insert(sp_pr, at, element);
        }
        if new_stroke != old_stroke {
            for c in self.doc.children(sp_pr).to_vec() {
                if self.doc.is_element(c) && self.doc.local_name(c) == "ln" {
                    self.doc.detach(c);
                }
            }
            let xml = crate::save::pptx_new::line_xml(&drawing_prefixes(), new_stroke);
            let element = fragment(self, xml);
            let at = position(self, &["xfrm", "custGeom", "prstGeom", "noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill"]);
            self.doc.insert(sp_pr, at, element);
        }
        if effects_changed && let Some(auto) = new_auto {
            for c in self.doc.children(sp_pr).to_vec() {
                if self.doc.is_element(c) && matches!(self.doc.local_name(c), "effectLst" | "effectDag") {
                    self.doc.detach(c);
                }
            }
            if let Ok(xml) = crate::save::pptx_new::auto_effects_xml(&drawing_prefixes(), auto)
                && !xml.is_empty()
            {
                let element = fragment(self, xml);
                let at = position(self, &["xfrm", "custGeom", "prstGeom", "noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill", "ln"]);
                self.doc.insert(sp_pr, at, element);
            }
        }
    }

    fn update_text_box(&mut self, node: NodeId, shape: &Shape, body: &ibis_doc_model::TextBody, old_body: Option<&ibis_doc_model::TextBody>) {
        let drawing = if self.doc.local_name(node) == "drawing" { Some(node) } else { self.doc.descendants(node).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "drawing") };
        if let Some(drawing) = drawing {
            self.set_picture_size(drawing, shape.frame.width, shape.frame.height);
            if shape.floating.is_some() {
                self.configure_drawing_anchor(drawing, shape.floating.as_ref());
            }
        }
        if let Some(body_pr) = self.doc.descendants(node).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "bodyPr") {
            let anchor_val = match body.anchor {
                ibis_doc_model::VerticalAnchor::Top => "t",
                ibis_doc_model::VerticalAnchor::Middle => "ctr",
                ibis_doc_model::VerticalAnchor::Bottom => "b",
            };
            self.doc.set_attr(body_pr, "anchor", anchor_val);
        }
        let text = body.text();
        // The text's formatting (bold, italic, size, colour) of the first run, from the
        // model and from the old element: a change goes to every run of the box.
        // The values a body's first run is drawn with.
        let looks = |b: &ibis_doc_model::TextBody| {
            let first = b.paragraphs.iter().flat_map(|p| &p.runs).find(|r| !r.text.is_empty());
            (
                first.and_then(|r| r.style.bold).unwrap_or(b.style.bold),
                first.and_then(|r| r.style.italic).unwrap_or(b.style.italic),
                first.is_some_and(|r| r.style.underline),
                first.map_or(ibis_doc_model::Underline::Single, |r| r.style.underline_kind),
                (first.and_then(|r| r.style.font_size).unwrap_or(b.style.font_size) + 25) / 50 * 50,
                first.and_then(|r| r.style.color).unwrap_or(b.style.color),
            )
        };
        let (model_bold, model_italic, model_u, model_uk, model_size, model_color) = looks(body);
        let (old_bold, old_italic, old_u, old_uk, old_size, old_color) =
            old_body.map_or((model_bold, model_italic, model_u, model_uk, model_size, model_color), looks);
        let format_changed = (old_bold, old_italic, old_u, old_uk, old_size, old_color)
            != (model_bold, model_italic, model_u, model_uk, model_size, model_color);
        if format_changed {
            for run in self.doc.descendants(node).into_iter().filter(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "r" && self.doc.parent(n).is_some_and(|p| self.doc.local_name(p) == "p")).collect::<Vec<_>>() {
                let in_content = std::iter::successors(self.doc.parent(run), |&n| self.doc.parent(n)).any(|n| self.doc.is_element(n) && self.doc.local_name(n) == "txbxContent");
                if !in_content {
                    continue;
                }
                let rpr = match self.doc.child(run, "rPr") {
                    Some(r) => r,
                    None => {
                        let r = self.doc.append_element(run, &self.w("rPr"));
                        self.doc.detach(r);
                        self.doc.insert(run, 0, r);
                        r
                    }
                };
                let o = RPR_ORDER;
                if old_bold != model_bold {
                    self.toggle(rpr, &["b", "bCs"], Some(model_bold), o);
                }
                if old_italic != model_italic {
                    self.toggle(rpr, &["i", "iCs"], Some(model_italic), o);
                }
                if (old_u, old_uk) != (model_u, model_uk) {
                    let u_val = if model_u {
                        ibis_doc_model::UnderlineStyle::from_drawing_underline(model_uk).name()
                    } else {
                        "none"
                    };
                    self.val(rpr, "u", Some(u_val), o);
                }
                if old_size != model_size {
                    let half = (model_size / 50).to_string();
                    self.val(rpr, "sz", Some(&half), o);
                    self.val(rpr, "szCs", Some(&half), o);
                }
                if old_color != model_color {
                    self.val(rpr, "color", Some(&format!("{:06X}", model_color.0 & 0xffffff)), o);
                }
            }
        }
        // A shape drawn without text that got some (DOCX-IMAGE-011): its `wps:txbx` is made before `wps:bodyPr`.
        if !text.is_empty() && !self.doc.descendants(node).into_iter().any(|n| self.doc.is_element(n) && self.doc.local_name(n) == "txbxContent") {
            let wsp = self.doc.descendants(node).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "wsp");
            if let Some(wsp) = wsp {
                let at = self.doc.children(wsp).iter().position(|&c| self.doc.is_element(c) && self.doc.local_name(c) == "bodyPr").unwrap_or(self.doc.children(wsp).len());
                let txbx = self.doc.append_element(wsp, "wps:txbx");
                self.doc.detach(txbx);
                self.doc.insert(wsp, at, txbx);
                let content = self.doc.append_element(txbx, &self.w("txbxContent"));
                let p = self.doc.append_element(content, &self.w("p"));
                let r = self.doc.append_element(p, &self.w("r"));
                self.doc.append_element(r, &self.w("t"));
            }
        }
        // The text is the same but a part of it is formatted differently (bold words, another size…): the paragraphs are written
        // run by run, each with the formatting the model gives it.
        if let Some(old_body) = old_body
            && txbx_text(self.doc, self.doc.descendants(node).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "txbxContent").unwrap_or(node)) == text
            && run_looks(body) != run_looks(old_body)
        {
            self.write_runs_by_look(node, body);
            return;
        }
        for content in self.doc.descendants(node).into_iter().filter(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "txbxContent").collect::<Vec<_>>() {
            if txbx_text(self.doc, content) == text {
                continue;
            }
            let old: Vec<NodeId> = self.doc.children_named(content, "p").collect();
            let lines: Vec<&str> = text.split('\n').collect();
            let mut made = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                let template = old.get(i).or_else(|| old.last()).copied();
                let p = match template {
                    Some(t) => self.doc.deep_clone(t),
                    None => {
                        let p = self.doc.append_element(content, &self.w("p"));
                        self.doc.detach(p);
                        p
                    }
                };
                // Keep the paragraph's properties and the first run's formatting.
                let first_run = self.doc.children_named(p, "r").next();
                let rpr = first_run.and_then(|r| self.doc.child(r, "rPr")).map(|n| self.doc.deep_clone(n));
                for c in self.doc.children(p).to_vec() {
                    if !(self.doc.is_element(c) && self.doc.local_name(c) == "pPr") {
                        self.doc.detach(c);
                    }
                }
                let r = self.doc.append_element(p, &self.w("r"));
                if let Some(rpr) = rpr {
                    self.doc.insert(r, 0, rpr);
                }
                let t = self.doc.append_element(r, &self.w("t"));
                if needs_space_preserve(line) {
                    self.doc.set_attr(t, "xml:space", "preserve");
                }
                self.doc.set_text(t, line);
                made.push(p);
            }
            for p in old {
                self.doc.detach(p);
            }
            for p in made {
                let end = self.doc.children(content).len();
                self.doc.insert(content, end, p);
            }
        }
    }

    /// A new inline chart (`w:drawing`) showing the chart part related as `rid`.
    fn new_chart_drawing(&mut self, rid: &str, cx: i64, cy: i64) -> NodeId {
        let root = self.doc.root_element().unwrap_or(XmlDocument::ROOT);
        let used = self.doc.descendants(root).into_iter().filter(|&n| self.doc.is_element(n) && matches!(self.doc.local_name(n), "docPr" | "cNvPr")).filter_map(|n| self.doc.attr(n, "id").and_then(|v| v.parse::<u64>().ok())).max().unwrap_or(0);
        let id = used + 1;
        let xml = format!(
            concat!(
                r#"<w:drawing xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
                r#"<wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="{id}" name="{chart_name}"/><wp:cNvGraphicFramePr/>"#,
                r#"<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">"#,
                r#"<c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{rid}"/>"#,
                r#"</a:graphicData></a:graphic></wp:inline></w:drawing>"#
            ),
            cx = cx,
            cy = cy,
            id = id,
            rid = rid,
            chart_name = crate::chart_name(id)
        );
        let parsed = XmlDocument::parse(xml.as_bytes()).expect("the chart's XML");
        let drawing = parsed.root_element().expect("the drawing");
        let node = self.doc.import(&parsed, drawing);
        for (prefix, ns) in [("w", "http://schemas.openxmlformats.org/wordprocessingml/2006/main"), ("wp", "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"), ("r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships")] {
            if self.doc.lookup_prefix(root, ns).as_deref() == Some(prefix) {
                self.doc.remove_attr(node, &format!("xmlns:{prefix}"));
            }
        }
        node
    }

    fn new_picture(&mut self, rid: &str, cx: i64, cy: i64, is_svg: bool) -> NodeId {
        // An id for its drawing objects, above those of the part.
        let root = self.doc.root_element().unwrap_or(XmlDocument::ROOT);
        let used = self.doc.descendants(root).into_iter().filter(|&n| self.doc.is_element(n) && matches!(self.doc.local_name(n), "docPr" | "cNvPr")).filter_map(|n| self.doc.attr(n, "id").and_then(|v| v.parse::<u64>().ok())).max().unwrap_or(0);
        let id = (used + 1).to_string();
        let blip_fill = if is_svg {
            format!(
                r#"<pic:blipFill><a:blip r:embed="{rid}"><a:extLst><a:ext uri="{{96DAC542-7B81-4868-9F9E-1EA1F31DC1F3}}"><asvg:svgBlip xmlns:asvg="http://schemas.microsoft.com/office/drawing/2016/SVG/main" r:embed="{rid}"/></a:ext></a:extLst></a:blip><a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
                rid = rid
            )
        } else {
            format!(
                r#"<pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>"#,
                rid = rid
            )
        };
        let picture_name = crate::picture_name(&id);
        let xml = format!(
            concat!(
                r#"<w:drawing xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
                r#"<wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="{id}" name="{picture_name}"/>"#,
                r#"<wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" noChangeAspect="1"/></wp:cNvGraphicFramePr>"#,
                r#"<a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">"#,
                r#"<pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="{id}" name="{picture_name}"/><pic:cNvPicPr/></pic:nvPicPr>"#,
                "{blip_fill}",
                r#"<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>"#,
                r#"</a:graphicData></a:graphic></wp:inline></w:drawing>"#
            ),
            cx = cx,
            cy = cy,
            id = id,
            blip_fill = blip_fill,
            picture_name = picture_name
        );
        let parsed = XmlDocument::parse(xml.as_bytes()).expect("the picture's XML");
        let drawing = parsed.root_element().expect("the drawing");
        let node = self.doc.import(&parsed, drawing);
        // The prefixes the part declares already need no declaration here.
        for (prefix, ns) in [("w", "http://schemas.openxmlformats.org/wordprocessingml/2006/main"), ("wp", "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"), ("r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships")] {
            if self.doc.lookup_prefix(root, ns).as_deref() == Some(prefix) {
                self.doc.remove_attr(node, &format!("xmlns:{prefix}"));
            }
        }
        node
    }

    /// A paragraph's bookmarks (added at its start and end) and the start
    /// or end of a field spanning paragraphs.
    fn paragraph_marks(&mut self, p: NodeId, old: &FlowParagraph, new: &FlowParagraph) -> Result<(), Error> {
        let w = |l: &str| format!("w:{l}");
        // After the paragraph's properties: where new first children go.
        let first_index = |doc: &XmlDocument| doc.children(p).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "pPr").map_or(0, |i| i + 1);
        for name in old.bookmarks.iter().filter(|b| !new.bookmarks.contains(b)) {
            let starts: Vec<NodeId> = self.doc.descendants(p).into_iter().filter(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "bookmarkStart" && self.doc.attr_local(n, "name") == Some(name.as_str())).collect();
            for start in starts {
                let id = self.doc.attr_local(start, "id").map(str::to_string);
                self.doc.detach(start);
                let root = self.doc.root_element().unwrap_or(XmlDocument::ROOT);
                if let Some(id) = id
                    && let Some(end) = self.doc.descendants(root).into_iter().find(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "bookmarkEnd" && self.doc.attr_local(n, "id") == Some(id.as_str()))
                {
                    self.doc.detach(end);
                }
            }
        }
        let added: Vec<&String> = new.bookmarks.iter().filter(|b| !old.bookmarks.contains(b)).collect();
        if !added.is_empty() {
            let root = self.doc.root_element().unwrap_or(XmlDocument::ROOT);
            let mut next = self.doc.descendants(root).into_iter().filter(|&n| self.doc.is_element(n) && self.doc.local_name(n) == "bookmarkStart").filter_map(|n| self.doc.attr_local(n, "id").and_then(|v| v.parse::<u64>().ok())).max().map_or(0, |m| m + 1);
            for name in added {
                let at = first_index(self.doc);
                let start = self.doc.insert_element(p, at, &w("bookmarkStart"));
                self.doc.set_attr(start, "w:id", &next.to_string());
                self.doc.set_attr(start, "w:name", name);
                let end = self.doc.append_element(p, &w("bookmarkEnd"));
                self.doc.set_attr(end, "w:id", &next.to_string());
                // A caption's bookmark for cross-references ends after its
                // number (Word's 番号とラベルのみ).
                if name.starts_with("_Ref")
                    && let Some(after) = self.seq_end(p)
                {
                    self.doc.detach(end);
                    self.doc.insert_after(after, end);
                }
                next += 1;
            }
        }
        if old.field_start != new.field_start {
            let Some(instr) = &new.field_start else { return Err(unsupported("目次のフィールドの始まりの削除")) };
            if old.field_start.is_some() {
                return Err(unsupported("目次のフィールドの書き換え"));
            }
            // begin, instruction, separate: at the start (after the
            // bookmarks just added, before the text).
            let mut at = first_index(self.doc);
            while self.doc.children(p).get(at).is_some_and(|&c| !self.doc.is_element(c) || self.doc.local_name(c) == "bookmarkStart") {
                at += 1;
            }
            for (kind, value) in [("begin", None), ("instr", Some(instr.as_str())), ("separate", None)] {
                let r = self.doc.insert_element(p, at, &w("r"));
                at += 1;
                match value {
                    Some(v) => {
                        let e = self.doc.append_element(r, &w("instrText"));
                        self.doc.set_attr(e, "xml:space", "preserve");
                        self.doc.set_text(e, &format!(" {v} "));
                    }
                    None => {
                        let e = self.doc.append_element(r, &w("fldChar"));
                        self.doc.set_attr(e, "w:fldCharType", kind);
                    }
                }
            }
        }
        if old.field_end != new.field_end {
            if !new.field_end {
                return Err(unsupported("目次のフィールドの終わりの削除"));
            }
            let r = self.doc.append_element(p, &w("r"));
            let e = self.doc.append_element(r, &w("fldChar"));
            self.doc.set_attr(e, "w:fldCharType", "end");
        }
        Ok(())
    }

    /// The run ending the paragraph's first `SEQ` field.
    fn seq_end(&self, p: NodeId) -> Option<NodeId> {
        let runs: Vec<NodeId> = self.doc.elements(p).filter(|&r| self.doc.local_name(r) == "r").collect();
        let at = runs.iter().position(|&r| self.doc.elements(r).any(|c| self.doc.local_name(c) == "instrText" && self.doc.text(c).trim_start().starts_with("SEQ")))?;
        runs[at..].iter().copied().find(|&r| self.doc.elements(r).any(|c| self.doc.local_name(c) == "fldChar" && self.doc.attr_local(c, "fldCharType") == Some("end")))
    }

    /// Remove from a copied paragraph what must be unique or belongs to the
    /// original (bookmarks, comment ranges, ids, a section break).
    fn clean_copy(&mut self, p: NodeId) {
        for attr in ["w14:paraId", "w14:textId", "w:rsidR", "w:rsidRPr", "w:rsidRDefault", "w:rsidP"] {
            self.doc.remove_attr(p, attr);
        }
        for node in self.doc.descendants(p) {
            if matches!(self.doc.local_name(node), "bookmarkStart" | "bookmarkEnd" | "commentRangeStart" | "commentRangeEnd" | "commentReference" | "permStart" | "permEnd" | "subDoc") {
                self.doc.detach(node);
            }
        }
        if let Some(ppr) = self.doc.child(p, "pPr")
            && let Some(sect) = self.doc.child(ppr, "sectPr")
        {
            self.doc.detach(sect);
        }
        self.prune_wrappers(p);
    }

    /// Remove what wraps runs (a tracked change, a simple field, a link…)
    /// and is left with no run after the runs were written.
    fn prune_wrappers(&mut self, p: NodeId) {
        let mut nodes = self.doc.descendants(p);
        nodes.reverse();
        for node in nodes {
            if self.doc.is_element(node)
                && matches!(self.doc.local_name(node), "ins" | "del" | "moveFrom" | "moveTo" | "fldSimple" | "hyperlink" | "smartTag" | "customXml")
                && self.doc.parent(node).is_some_and(|parent| self.doc.local_name(parent) != "rPr" && self.doc.local_name(parent) != "pPr" && self.doc.local_name(parent) != "trPr")
                && !self.doc.elements(node).any(|c| !matches!(self.doc.local_name(c), "rPr" | "customXmlPr" | "smartTagPr"))
            {
                // An emptied link can still own unknown source comments
                // moved out of a removed revision. Retain them at paragraph
                // level rather than dropping them with the empty wrapper.
                if self.doc.local_name(node) == "hyperlink" {
                    for child in self.doc.children(node).to_vec() {
                        if !self.doc.is_element(child) { self.doc.insert_before(node, child); }
                    }
                }
                self.doc.detach(node);
            }
        }
    }

    /// The element at `path` below `from`.
    fn below(&self, from: NodeId, path: &[u32]) -> Option<NodeId> {
        let mut at = from;
        for &i in path {
            at = self.doc.elements(at).nth(i as usize)?;
        }
        Some(at)
    }

    /// Write paragraph `new` into the element `p`, which holds `old` (whose
    /// positions came from `sources`).
    /// Returns the runs of the old paragraph that still hold its positions
    /// (and a kept field's runs).
    fn paragraph(&mut self, p: NodeId, old: &FlowParagraph, new: &FlowParagraph, sources: &[AtomSource]) -> Result<HashSet<NodeId>, Error> {
        // The paragraph's properties last: a new `w:pPr` would move the
        // runs' paths.
        let used = self.paragraph_runs(p, old, new, sources)?;
        if old.runs != new.runs || self.paragraph_group_changed(old, new) {
            self.prune_wrappers(p);
        }
        self.paragraph_props(p, old, new)?;
        Ok(used)
    }

    fn paragraph_runs(&mut self, p: NodeId, old: &FlowParagraph, new: &FlowParagraph, sources: &[AtomSource]) -> Result<HashSet<NodeId>, Error> {
        let old_atoms = atoms(old);
        let new_atoms = atoms(new);
        let mut used: HashSet<NodeId> = HashSet::new();
        if old_atoms == new_atoms && !self.paragraph_group_changed(old, new) {
            for s in sources {
                match s {
                    AtomSource::InRun { run, .. } => used.extend(self.below(p, run)),
                    AtomSource::Field { nodes } => {
                        for path in nodes {
                            if let Some(n) = self.below(p, path) {
                                used.insert(n);
                                used.extend(self.doc.descendants(n));
                            }
                        }
                    }
                }
            }
            return Ok(used);
        }
        if sources.len() != old_atoms.len() {
            return Err(unsupported("読み込みで位置が合わない段落の変更"));
        }
        let keys_old: Vec<&AtomKey> = old_atoms.iter().map(|a| &a.0).collect();
        let keys_new: Vec<&AtomKey> = new_atoms.iter().map(|a| &a.0).collect();
        let mut kept = matching(&keys_old, &keys_new);
        if !self.old_shape_groups.is_empty() || !self.new_shape_groups.is_empty() {
            let mut unkept_group_shapes = Vec::new();
            for j in 0..new_atoms.len() {
                if let AtomKey::Shape(shape) = &new_atoms[j].0 {
                    let old_grp = Self::shape_group_of(self.old_shape_groups, shape.id);
                    let new_grp = Self::shape_group_of(self.new_shape_groups, shape.id);
                    let group_intact = old_grp == new_grp
                        && new_grp.is_none_or(|g| {
                            g.leaf_shape_ids().into_iter().all(|member_id| {
                                new_atoms
                                    .iter()
                                    .enumerate()
                                    .find(|(_, a)| matches!(&a.0, AtomKey::Shape(s) if s.id == member_id))
                                    .is_some_and(|(mj, _)| kept[mj].is_some())
                            })
                        });
                    if !group_intact {
                        unkept_group_shapes.push(j);
                    }
                }
            }
            for j in unkept_group_shapes {
                kept[j] = None;
            }
        }

        // The runs and nodes of the old positions (found before changing).
        let mut run_of: Vec<Option<NodeId>> = Vec::with_capacity(sources.len());
        let mut node_of: Vec<Option<NodeId>> = Vec::with_capacity(sources.len());
        for s in sources {
            match s {
                AtomSource::InRun { run, node } => {
                    run_of.push(self.below(p, run));
                    node_of.push(self.below(p, node));
                }
                AtomSource::Field { nodes } => {
                    run_of.push(None);
                    node_of.push(nodes.first().and_then(|path| self.below(p, path)));
                }
            }
        }
        if sources.iter().zip(&run_of).any(|(s, r)| matches!(s, AtomSource::InRun { .. }) && r.is_none()) {
            return Err(unsupported("読み込みで位置が合わない段落の変更"));
        }
        self.shape_nodes.clear();
        self.shape_olds.clear();
        for (i, a) in old_atoms.iter().enumerate() {
            if let (AtomKey::Shape(shape), Some(n)) = (&a.0, node_of[i]) {
                self.shape_nodes.insert(shape.id.0, n);
                self.shape_olds.insert(shape.id.0, shape.clone());
            }
        }
        let old_kept: HashSet<usize> = kept.iter().flatten().copied().collect();
        // Fields (their elements found before any is taken out, as paths
        // move then).
        let field_elements: Vec<Vec<NodeId>> = sources.iter().map(|s| match s {
            AtomSource::Field { nodes } => nodes.iter().filter_map(|path| self.below(p, path)).collect(),
            _ => Vec::new(),
        }).collect();
        // Links changed: the paragraph's links are taken apart here and made
        // again from the new paragraph's at the end.
        // A new character borrows a run host, which can already be inside an
        // owned hyperlink. Compare with that actual host's link rather than
        // assuming every new character starts outside a link. This retains
        // source parent attributes for typing within the same URL, and still
        // rebuilds when explicit formatting clears or changes that URL.
        let mut link_owners = HashMap::new();
        for run in run_of.iter().flatten().copied().collect::<HashSet<_>>() {
            let mut node = run;
            while let Some(parent) = self.doc.parent(node) {
                if parent == p { break; }
                if self.doc.local_name(parent) == "hyperlink" { link_owners.insert(run,parent); break; }
                node = parent;
            }
        }
        let mut following_runs = vec![None; kept.len()];
        let mut following = None;
        for j in (0..kept.len()).rev() {
            following_runs[j] = following;
            if let Some(i) = kept[j] && run_of[i].is_some() { following = Some(i); }
        }
        let mut host_source = None;
        let source_hosts: Vec<_> = kept.iter().enumerate().map(|(j,k)| {
            host_source = match *k {
                Some(i) => run_of[i].map(|_| i),
                None => host_source.or(following_runs[j]),
            };
            host_source
        }).collect();
        let links_changed = kept.iter().enumerate().any(|(j, k)| {
            let actual_link = match *k {
                Some(i) => old_atoms[i].3,
                None => source_hosts[j].filter(|&i| run_of[i].is_some_and(|r| link_owners.contains_key(&r))).map_or(&None, |i| old_atoms[i].3),
            };
            new_atoms[j].3 != actual_link
        });
        // If a link must be rebuilt, retain attributes only for supported
        // direct source links whose surviving atoms still have that same link.
        // Separate ownership prevents merging equal URLs with different frames.
        let simple_run = |r| self.doc.children(r).iter().all(|&c| self.doc.is_element(c) && matches!(self.doc.local_name(c),"rPr"|"t"|"delText"));
        let supported_links: HashSet<_> = link_owners.values().copied().collect::<HashSet<_>>().into_iter().filter(|&h| {
            self.doc.parent(h)==Some(p) && self.doc.elements(h).all(|n| match self.doc.local_name(n) {
                "r" => simple_run(n),
                "ins" | "del" | "moveFrom" | "moveTo" => self.doc.elements(n).all(|r| self.doc.local_name(r)=="r" && simple_run(r)),
                _ => false,
            })
        }).collect();
        let original_link_at: Vec<_> = source_hosts.iter().enumerate().map(|(j,i)| {
            let i = (*i)?;
            if new_atoms[j].3.is_none() || new_atoms[j].3 != old_atoms[i].3 { return None; }
            let h = *link_owners.get(&run_of[i]?)?;
            supported_links.contains(&h).then_some(h)
        }).collect();
        if links_changed {
            for h in self.doc.descendants(p) {
                if self.doc.is_element(h) && self.doc.local_name(h) == "hyperlink" && self.doc.parent(h).is_some() {
                    for c in self.doc.children(h).to_vec() {
                        self.doc.insert_before(h, c);
                    }
                    self.doc.detach(h);
                }
            }
        }
        // Tracked changes changed: the paragraph's `w:ins` and `w:del` are
        // taken apart here and made again from the new paragraph's at the end.
        // New ordinary text can borrow a deleted run as its host even when
        // its revision is None. Rebuild container placement whenever editing
        // a paragraph with old insertion/deletion scopes, so that borrowed
        // runs cannot leave ordinary text inside an old w:del/w:ins.
        let revisions_changed = old_atoms.iter().any(|a| a.4.as_ref().is_some_and(|r| matches!(r.kind, RevisionKind::Insert | RevisionKind::Delete | RevisionKind::MoveFrom | RevisionKind::MoveTo)))
            || kept.iter().enumerate().any(|(j, k)| new_atoms[j].4 != k.map_or(&None, |i| old_atoms[i].4));
        // Keep an unchanged original revision as an XML unit. Model revisions
        // carry no container ID or unknown attributes; rebuilding this unit
        // would silently merge it with a new change by the same author/date.
        // Complete, contiguous paragraph containers, including those under a
        // single direct hyperlink, qualify when content,
        // links and revisions are unchanged. Format-only edits can retain
        // wrapper metadata while using newly formatted simple runs; partial
        // removal of a simple insertion can retain its surviving identity.
        let mut original_revision_at = vec![None; new_atoms.len()];
        let mut original_revisions: HashMap<NodeId, (NodeId, Vec<NodeId>, bool)> = HashMap::new();
        // A link edit elsewhere in this paragraph does not invalidate a
        // direct run-only scope: complete atoms (including links) must still
        // match below before its original XML can be restored.
        if revisions_changed {
            for h in self.doc.descendants(p) {
                if !self.doc.is_element(h) || !matches!(self.doc.local_name(h), "ins" | "del" | "moveFrom" | "moveTo") { continue; }
                // Keep only a direct paragraph scope or a scope owned by one
                // direct hyperlink. Other nested wrappers remain unsupported.
                let Some(parent) = self.doc.parent(h) else { continue };
                if parent != p && !(self.doc.local_name(parent) == "hyperlink" && self.doc.parent(parent) == Some(p)) { continue; }
                let descendants = self.doc.descendants(h);
                // Nested wrappers can place segment nodes under different
                // parents. Direct complex fields qualify only as an opaque,
                // unchanged XML unit, with every field run owned by this scope.
                if self.doc.elements(h).any(|n| self.doc.local_name(n) != "r") { continue; }
                let field_scope = descendants.iter().any(|&n| matches!(self.doc.local_name(n), "fldChar" | "instrText"));
                let indices: Vec<_> = run_of.iter().enumerate().filter_map(|(i,r)| {
                    (r.is_some_and(|r| descendants.contains(&r)) || field_elements[i].iter().any(|n| descendants.contains(n))).then_some(i)
                }).collect();
                if indices.is_empty() { continue; }
                let positions: Vec<_> = kept.iter().enumerate()
                    .filter_map(|(j, i)| i.filter(|i| indices.contains(i)).map(|i| (j, i))).collect();
                let complete = positions.len() == indices.len();
                if positions.is_empty()
                    || positions.iter().any(|&(j, i)| new_atoms[j].0 != old_atoms[i].0 || new_atoms[j].3 != old_atoms[i].3 || new_atoms[j].4 != old_atoms[i].4)
                { continue; }
                let contiguous = positions.windows(2).all(|pair| pair[1].0 == pair[0].0 + 1);
                // A simple source insertion can survive in several pieces
                // around replacement text. Each piece keeps source attributes;
                // only the first uses its original ID. New text never inherits
                // that source identity. Other revision kinds remain excluded.
                if (!complete || !contiguous) && !matches!(self.doc.local_name(h), "ins" | "moveTo") { continue; }
                let exact = complete && contiguous && positions.iter().all(|&(j,i)| new_atoms[j] == old_atoms[i]);
                if field_scope && (!exact || indices.iter().any(|&i| field_elements[i].iter().any(|&n| self.doc.parent(n) != Some(h)))) {
                    return Err(unsupported("元の変更履歴内の複雑なフィールドを含む編集"));
                }
                // For a format-only edit, retain the wrapper metadata while
                // using newly formatted runs. Restrict this to simple run
                // children; unmodeled content cannot be safely rebuilt here.
                if !exact && self.doc.elements(h).any(|r| self.doc.children(r).iter().any(|&c| !self.doc.is_element(c) || !matches!(self.doc.local_name(c), "rPr" | "t" | "delText"))) { return Err(unsupported("元の変更履歴内の未対応内容を含む書式変更")); }
                let copy = self.doc.deep_clone(h);
                original_revisions.insert(h, (copy, self.doc.children(h).to_vec(), exact));
                for (j, _) in positions { original_revision_at[j] = Some(h); }
            }
        }
        if revisions_changed {
            for h in self.doc.descendants(p) {
                if self.doc.is_element(h)
                    && matches!(self.doc.local_name(h), "ins" | "del" | "moveFrom" | "moveTo")
                    && self.doc.parent(h).is_some_and(|q| !matches!(self.doc.local_name(q), "rPr" | "pPr" | "trPr" | "tcPr"))
                {
                    for c in self.doc.children(h).to_vec() {
                        self.doc.insert_before(h, c);
                    }
                    self.doc.detach(h);
                }
            }
        }
        let mut segment_nodes: HashMap<usize, Vec<NodeId>> = HashMap::new();

        // Hosts.
        let mut hosts: Vec<Host> = Vec::with_capacity(new_atoms.len());
        for (j, k) in kept.iter().enumerate() {
            let host = match k {
                Some(i) => match run_of[*i] {
                    Some(r) => Host::Run(r),
                    None => Host::Field(*i),
                },
                None => {
                    let before = hosts.last().copied().filter(|h| matches!(h, Host::Run(_)));
                    let after = || kept[j + 1..].iter().flatten().find_map(|&i| run_of[i]).map(Host::Run);
                    before.or_else(after).unwrap_or(Host::Mark)
                }
            };
            hosts.push(host);
        }
        for (i, s) in sources.iter().enumerate() {
            let AtomSource::Field { .. } = s else { continue };
            let elements = field_elements[i].clone();
            if !old_kept.contains(&i) {
                for e in elements {
                    self.doc.detach(e);
                }
            } else {
                for &e in &elements {
                    used.insert(e);
                    used.extend(self.doc.descendants(e));
                }
                let j = kept.iter().position(|k| *k == Some(i)).unwrap();
                let (_, f0, c0, _, _) = old_atoms[i];
                let (_, f1, c1, _, _) = new_atoms[j];
                let is_locked = match &new_atoms[j].0 {
                    AtomKey::PageNumber(l) | AtomKey::FileName(_, l) | AtomKey::PageRef(_, _, l) | AtomKey::Field(_, _, l) => *l,
                    _ => false,
                };
                for &e in &elements {
                    if self.doc.local_name(e) == "fldSimple" {
                        if is_locked {
                            self.doc.set_attr(e, "w:fldLock", "1");
                        } else {
                            self.doc.remove_attr(e, "w:fldLock");
                        }
                    } else {
                        for c in self.doc.descendants(e) {
                            if self.doc.local_name(c) == "fldChar" && self.doc.attr_local(c, "fldCharType") == Some("begin") {
                                if is_locked {
                                    self.doc.set_attr(c, "w:fldLock", "1");
                                } else {
                                    self.doc.remove_attr(c, "w:fldLock");
                                }
                            }
                        }
                    }
                }
                if (f0, c0) != (f1, c1) {
                    let mut runs = Vec::new();
                    for e in elements {
                        // A simple field's result keeps its formatting only
                        // with \* MERGEFORMAT (Word writes it again in the
                        // paragraph's otherwise).
                        if self.doc.local_name(e) == "fldSimple"
                            && let Some(name) = word_attr_name(self.doc, e, "instr").map(str::to_string)
                            && let Some(instr) = self.doc.attr(e, &name).map(str::to_string)
                            && !instr.to_ascii_uppercase().contains("MERGEFORMAT")
                        {
                            self.doc.set_attr(e, &name, &format!(" {} \\* MERGEFORMAT ", instr.trim()));
                        }
                        if self.doc.local_name(e) == "r" {
                            runs.push(e);
                        } else {
                            runs.extend(self.doc.descendants(e).into_iter().filter(|&n| self.doc.local_name(n) == "r"));
                        }
                    }
                    for r in runs {
                        self.run_props(r, f0, c0, f1, c1, false);
                    }
                }
            }
        }

        // Runs to write again: a position of theirs deleted, or one
        // inserted into them or formatted differently.
        let mut dirty: HashSet<NodeId> = HashSet::new();
        for (i, r) in run_of.iter().enumerate() {
            if let Some(r) = r
                && !old_kept.contains(&i)
            {
                dirty.insert(*r);
            }
        }
        for (j, host) in hosts.iter().enumerate() {
            if let Host::Run(r) = host {
                match kept[j] {
                    Some(i) if run_of[i] == Some(*r) && (old_atoms[i].1, old_atoms[i].2, old_atoms[i].3, old_atoms[i].4) == (new_atoms[j].1, new_atoms[j].2, new_atoms[j].3, new_atoms[j].4) => {}
                    _ => {
                        dirty.insert(*r);
                    }
                }
            }
        }
        // The old formatting of each run.
        let mut run_format: HashMap<NodeId, (&RunFormat, &Option<String>)> = HashMap::new();
        for (i, r) in run_of.iter().enumerate() {
            if let Some(r) = r {
                run_format.entry(*r).or_insert((old_atoms[i].1, old_atoms[i].2));
            }
        }

        // Segments: consecutive positions of one host and one formatting.
        let mut segments: Vec<(Host, Vec<usize>)> = Vec::new();
        for (j, host) in hosts.iter().enumerate() {
            match segments.last_mut() {
                Some((h, list)) if h == host && !matches!(host, Host::Field(_)) && original_revision_at[list[0]] == original_revision_at[j] && original_link_at[list[0]] == original_link_at[j] && (new_atoms[list[0]].1, new_atoms[list[0]].2, new_atoms[list[0]].3, new_atoms[list[0]].4) == (new_atoms[j].1, new_atoms[j].2, new_atoms[j].3, new_atoms[j].4) => list.push(j),
                _ => segments.push((*host, vec![j])),
            }
        }

        // Each dirty run: its copies with the new content go before it, and
        // the run keeps only what is not a position (field characters…).
        let templates: HashMap<NodeId, NodeId> = dirty.iter().map(|&r| (r, self.doc.deep_clone(r))).collect();
        let mut first_of: HashMap<NodeId, NodeId> = HashMap::new();
        let mut last_of: HashMap<NodeId, NodeId> = HashMap::new();
        for (s_index, (host, list)) in segments.iter().enumerate() {
            if let Host::Field(i) = host {
                // Include preserved field runs so the whole original scope
                // forms one group and its XML copy is restored exactly once.
                if original_revision_at[list[0]].is_some() {
                    segment_nodes.insert(s_index, field_elements[*i].clone());
                }
                continue;
            }
            let Host::Run(r) = host else { continue };
            if !dirty.contains(r) {
                segment_nodes.insert(s_index, vec![*r]);
                continue;
            }
            let (f0, c0) = run_format[r];
            let (_, f1, c1, _, _) = new_atoms[list[0]];
            let copy = self.doc.deep_clone(templates[r]);
            for child in self.doc.children(copy).to_vec() {
                if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                    self.doc.detach(child);
                }
            }
            self.run_props(copy, f0, c0, f1, c1, false);
            self.revision_format(copy, new_atoms[list[0]].4);
            let made = self.content(copy, list, &kept, &node_of, &new_atoms, new)?;
            if !self.doc.children(copy).iter().any(|&c| self.doc.is_element(c) && self.doc.local_name(c) != "rPr") && made.is_empty() {
                continue;
            }
            self.doc.insert_before(*r, copy);
            let mut last = copy;
            let mut nodes = vec![copy];
            for extra in made {
                self.doc.insert_after(last, extra);
                last = extra;
                nodes.push(extra);
            }
            segment_nodes.insert(s_index, nodes);
            first_of.entry(*r).or_insert(copy);
            last_of.insert(*r, last);
        }
        for &r in &dirty {
            for (i, rr) in run_of.iter().enumerate() {
                if *rr == Some(r)
                    && let Some(n) = node_of[i]
                    && self.doc.parent(n) == Some(r)
                {
                    self.doc.detach(n);
                }
            }
            for child in self.doc.children(r).to_vec() {
                if self.doc.is_element(child) && self.doc.local_name(child) == "lastRenderedPageBreak" {
                    self.doc.detach(child);
                }
            }
            let rest = self.doc.children(r).iter().any(|&c| self.doc.is_element(c) && self.doc.local_name(c) != "rPr");
            if !rest {
                self.doc.detach(r);
            }
        }
        for (i, r) in run_of.iter().enumerate() {
            if let Some(r) = r
                && old_kept.contains(&i)
                && self.doc.parent(*r).is_some()
            {
                used.insert(*r);
            }
        }
        // New runs formatted from the paragraph mark: before the next run
        // written, else after the last one, else at the end.
        for (s, (host, list)) in segments.iter().enumerate() {
            if *host != Host::Mark {
                continue;
            }
            let run = self.doc.append_element(p, &self.w("r"));
            self.doc.detach(run);
            if let Some(ppr) = self.doc.child(p, "pPr")
                && let Some(mark) = self.doc.child(ppr, "rPr")
            {
                let rpr = self.doc.deep_clone(mark);
                for child in self.doc.children(rpr).to_vec() {
                    if self.doc.is_element(child) && matches!(self.doc.local_name(child), "ins" | "del" | "moveFrom" | "moveTo" | "rPrChange") {
                        self.doc.detach(child);
                    }
                }
                self.doc.insert(run, 0, rpr);
            }
            let (_, f1, c1, _, _) = new_atoms[list[0]];
            self.run_props(run, &old.mark_format, &None, f1, c1, false);
            self.revision_format(run, new_atoms[list[0]].4);
            let made = self.content(run, list, &kept, &node_of, &new_atoms, new)?;
            if !self.doc.children(run).iter().any(|&c| self.doc.is_element(c) && self.doc.local_name(c) != "rPr") && made.is_empty() {
                continue;
            }
            let next = segments[s + 1..].iter().find_map(|(h, _)| match h {
                Host::Run(r) => first_of.get(r).copied().or(Some(*r)).filter(|n| self.doc.parent(*n).is_some()),
                _ => None,
            });
            let prev = segments[..s].iter().rev().find_map(|(h, _)| match h {
                Host::Run(r) => last_of.get(r).copied().or(Some(*r)).filter(|n| self.doc.parent(*n).is_some()),
                _ => None,
            });
            let mut all = vec![run];
            all.extend(made);
            segment_nodes.insert(s, all.clone());
            match (next, prev) {
                (Some(next), _) => {
                    for n in all {
                        self.doc.insert_before(next, n);
                    }
                }
                (None, Some(prev)) => {
                    let mut last = prev;
                    for n in all {
                        self.doc.insert_after(last, n);
                        last = n;
                    }
                }
                (None, None) => {
                    for n in all {
                        let end = self.doc.children(p).len();
                        self.doc.insert(p, end, n);
                    }
                }
            }
        }
        if links_changed {
            // Consecutive runs of one link go into one `w:hyperlink`.
            let mut groups: Vec<(Link, Option<NodeId>, Vec<NodeId>)> = Vec::new();
            for (s_index, (_, list)) in segments.iter().enumerate() {
                let link = new_atoms[list[0]].3.clone();
                let nodes = segment_nodes.get(&s_index).cloned().unwrap_or_default();
                let original = original_link_at[list[0]];
                match (link, groups.last_mut()) {
                    (Some(l), Some((g, owner, ns))) if *g == l && *owner == original => ns.extend(nodes),
                    (Some(l), _) => groups.push((l, original, nodes)),
                    (None, _) => groups.push((Link::Anchor(String::new()), None, Vec::new())),
                }
            }
            for (link, original, nodes) in groups {
                let Some(&first) = nodes.first() else { continue };
                let Some(parent) = self.doc.parent(first) else { continue };
                let h = if let Some(original) = original {
                    // The source was unwrapped, so this copies only its attrs.
                    self.doc.deep_clone(original)
                } else {
                    let name = self.w("hyperlink");
                    let h = self.doc.append_element(parent,&name);
                    self.doc.detach(h);
                    h
                };
                self.doc.insert_before(first, h);
                if original.is_none() { match &link {
                    Link::Url(url) => {
                        let Some(rid) = self.link_rels.get(url) else { return Err(unsupported("リンク")) };
                        let r = self.doc.lookup_prefix(parent, "http://schemas.openxmlformats.org/officeDocument/2006/relationships").unwrap_or_else(|| "r".into());
                        self.doc.set_attr(h, &format!("{r}:id"), rid);
                    }
                    Link::Anchor(a) => self.doc.set_attr(h, "w:anchor", a),
                }
                self.doc.set_attr(h, "w:history", "1"); }
                for n in nodes {
                    if self.doc.parent(n) == Some(parent) {
                        let end = self.doc.children(h).len();
                        self.doc.insert(h, end, n);
                    }
                }
            }
        }
        if revisions_changed {
            // Consecutive runs of one insertion or deletion (by one author,
            // at one time) go into one `w:ins` or `w:del`.
            let mut groups: Vec<(Option<Revision>, Option<NodeId>, Vec<NodeId>)> = Vec::new();
            for (s_index, (_, list)) in segments.iter().enumerate() {
                let revision = new_atoms[list[0]].4.clone().filter(|r| matches!(r.kind, RevisionKind::Insert | RevisionKind::Delete | RevisionKind::MoveFrom | RevisionKind::MoveTo));
                let nodes = segment_nodes.get(&s_index).cloned().unwrap_or_default();
                match groups.last_mut() {
                    Some((g, original, ns)) if revision.is_some() && *g == revision && *original == original_revision_at[list[0]] && ns.last().and_then(|&n| self.doc.parent(n)) == nodes.first().and_then(|&n| self.doc.parent(n)) => ns.extend(nodes),
                    _ => groups.push((revision, original_revision_at[list[0]], nodes)),
                }
            }
            let mut retained_original_nodes: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();
            for (_, original, nodes) in &groups {
                if let Some(original) = original { retained_original_nodes.entry(*original).or_default().extend(nodes.iter().copied()); }
            }
            let mut restored_originals = HashSet::new();
            for (revision, original, nodes) in groups {
                let (Some(revision), Some(&first)) = (revision, nodes.first()) else { continue };
                if let Some(original) = original {
                    let (template, children, exact) = &original_revisions[&original];
                    let mut split_id = None;
                    let copy = if restored_originals.insert(original) { *template } else {
                        // The unwrapped source retains its attributes but no
                        // children. Clone those for later pieces, with a fresh
                        // ID and no duplicate source comment/content.
                        split_id = Some(self.next_revision_id());
                        self.doc.deep_clone(original)
                    };
                    self.doc.insert_before(first, copy);
                    if let Some(id) = split_id {
                        // Resolve after attachment, including ancestor bindings;
                        // update the existing qualified name without duplicating it.
                        let name = word_attr_name(self.doc, copy, "id").unwrap_or("w:id").to_string();
                        self.doc.set_attr(copy, &name, &id.to_string());
                    }
                    if *exact {
                        for &node in nodes.iter().chain(children) { self.doc.detach(node); }
                    } else {
                        for r in self.doc.elements(copy).collect::<Vec<_>>() { self.doc.detach(r); }
                        for &node in &nodes {
                            let end = self.doc.children(copy).len();
                            self.doc.insert(copy,end,node);
                        }
                        for &node in children { if !retained_original_nodes[&original].contains(&node) { self.doc.detach(node); } }
                    }
                    used.extend(self.doc.descendants(copy));
                    continue;
                }
                let Some(parent) = self.doc.parent(first) else { continue };
                let tag = match revision.kind {
                    RevisionKind::Insert => "ins",
                    RevisionKind::Delete => "del",
                    RevisionKind::MoveTo => "moveTo",
                    RevisionKind::MoveFrom => "moveFrom",
                    _ => "ins",
                };
                let name = self.w(tag);
                let w = self.doc.append_element(parent, &name);
                self.doc.detach(w);
                self.doc.insert_before(first, w);
                let id = self.next_revision_id();
                self.doc.set_attr(w, "w:id", &id.to_string());
                self.doc.set_attr(w, "w:author", &revision.author);
                self.doc.set_attr(w, "w:date", &revision.date);
                for n in nodes {
                    if self.doc.parent(n) == Some(parent) {
                        let end = self.doc.children(w).len();
                        self.doc.insert(w, end, n);
                    }
                }
            }
        }
        Ok(used)
    }

    /// An id for a new tracked change: after the largest in the part.
    fn next_revision_id(&self) -> u64 {
        [ &*self.doc, self.pristine ].into_iter()
            .flat_map(|doc| {
                doc.descendants(doc.root_element().unwrap_or(XmlDocument::ROOT)).into_iter()
                    .filter(|&n| doc.is_element(n) && doc.namespace(n) == Some(W_NS) && matches!(doc.local_name(n), "ins" | "del" | "rPrChange" | "pPrChange" | "moveFrom" | "moveTo" | "sectPrChange" | "tblPrChange" | "trPrChange" | "tcPrChange" | "numberingChange"))
                    .filter_map(|n| word_attr_name(doc, n, "id").and_then(|name| doc.attr(n, name)).and_then(|v| v.parse::<u64>().ok()))
                    .collect::<Vec<_>>()
            })
            .max()
            .map_or(1, |m| m + 1)
    }

    /// A run's formatting change (`w:rPrChange`, with the formatting before)
    /// as its revision says, or none.
    fn revision_format(&mut self, run: NodeId, revision: &Option<Revision>) {
        let existing = self.doc.child(run, "rPr").and_then(|rpr| self.doc.child(rpr, "rPrChange"));
        match revision {
            Some(Revision { kind: RevisionKind::Format(old), author, date }) => {
                if let Some(e) = existing {
                    self.doc.detach(e);
                }
                let rpr = match self.doc.child(run, "rPr") {
                    Some(r) => r,
                    None => {
                        let name = self.w("rPr");
                        self.doc.insert_element(run, 0, &name)
                    }
                };
                // The formatting before, written as a run's would be.
                let temp_name = self.w("r");
                let temp = self.doc.append_element(rpr, &temp_name);
                self.run_props(temp, &RunFormat::default(), &None, old, &None, false);
                let change = self.doc.append_element(rpr, &self.w("rPrChange"));
                let id = self.next_revision_id();
                self.doc.set_attr(change, "w:id", &id.to_string());
                self.doc.set_attr(change, "w:author", author);
                self.doc.set_attr(change, "w:date", date);
                let inner = match self.doc.child(temp, "rPr") {
                    Some(r) => r,
                    None => {
                        let name = self.w("rPr");
                        self.doc.append_element(temp, &name)
                    }
                };
                self.doc.detach(inner);
                let end = self.doc.children(change).len();
                self.doc.insert(change, end, inner);
                self.doc.detach(temp);
            }
            _ => {
                if let Some(e) = existing {
                    self.doc.detach(e);
                }
            }
        }
    }

    /// Fill `run` with the positions `list` of the new paragraph. Returns
    /// further runs made after it (a new field's).
    fn content(&mut self, first_run: NodeId, list: &[usize], kept: &[Option<usize>], node_of: &[Option<NodeId>], new_atoms: &[Atom<'_>], para: &FlowParagraph) -> Result<Vec<NodeId>, Error> {
        let mut text = String::new();
        let mut extra = Vec::new();
        // The run content goes into: the host, then after a new field a
        // copy of it (so that what follows the field comes after it).
        let mut run = first_run;
        // Deleted text (a tracked deletion) is `w:delText`.
        let deleted = list.first().is_some_and(|&j| matches!(new_atoms[j].4, Some(Revision { kind: RevisionKind::Delete, .. })));
        let flush = |w: &mut Self, run: NodeId, text: &mut String| {
            if !text.is_empty() {
                let t = w.doc.append_element(run, &w.w(if deleted { "delText" } else { "t" }));
                if needs_space_preserve(text) {
                    w.doc.set_attr(t, "xml:space", "preserve");
                }
                w.doc.set_text(t, text);
                text.clear();
            }
        };
        for &j in list {
            let original = kept[j].and_then(|i| node_of[i]);
            match &new_atoms[j].0 {
                AtomKey::Char(c) => text.push(*c),
                AtomKey::Tab => {
                    flush(self, run, &mut text);
                    self.doc.append_element(run, &self.w("tab"));
                }
                AtomKey::PositionalTab(align, relative_to_margin, leader) => {
                    flush(self, run, &mut text);
                    match original.filter(|&n| self.doc.local_name(n) == "ptab") {
                        Some(n) => {
                            let c = self.doc.deep_clone(n);
                            let end = self.doc.children(run).len();
                            self.doc.insert(run, end, c);
                        }
                        None => {
                            let ptab = self.doc.append_element(run, &self.w("ptab"));
                            self.doc.set_attr(ptab, "w:alignment", match align {
                                ibis_doc_model::TabAlign::Right => "right",
                                ibis_doc_model::TabAlign::Center => "center",
                                _ => "left",
                            });
                            self.doc.set_attr(ptab, "w:relativeTo", if *relative_to_margin { "margin" } else { "indent" });
                            self.doc.set_attr(ptab, "w:leader", if *leader { "dot" } else { "none" });
                        }
                    }
                }
                AtomKey::LineBreak | AtomKey::PageBreak | AtomKey::ColumnBreak => {
                    flush(self, run, &mut text);
                    match original.filter(|&n| matches!(self.doc.local_name(n), "br" | "cr")) {
                        Some(n) => {
                            let c = self.doc.deep_clone(n);
                            let end = self.doc.children(run).len();
                            self.doc.insert(run, end, c);
                        }
                        None => {
                            let br = self.doc.append_element(run, &self.w("br"));
                            if new_atoms[j].0 == AtomKey::PageBreak {
                                self.doc.set_attr(br, "w:type", "page");
                            } else if new_atoms[j].0 == AtomKey::ColumnBreak {
                                self.doc.set_attr(br, "w:type", "column");
                            }
                        }
                    }
                }
                AtomKey::Ruby(base, ruby, size, raise, align) => {
                    flush(self, run, &mut text);
                    let node = match original.filter(|&n| self.doc.local_name(n) == "ruby") {
                        Some(n) => self.doc.deep_clone(n),
                        None => {
                            // A new ルビ: its base in the run's formatting.
                            let e = self.doc.append_element(run, &self.w("ruby"));
                            self.doc.detach(e);
                            let pr = self.doc.append_element(e, &self.w("rubyPr"));
                            let half = |v: u32| ((v + 25) / 50).to_string();
                            // The base's size (w:hpsBaseText, which Word needs).
                            let base_size = new_atoms[j].1.size.unwrap_or(raise + 100);
                            for (name, value) in [("rubyAlign", align.name().to_string()), ("hps", half(*size)), ("hpsRaise", half(*raise)), ("hpsBaseText", half(base_size)), ("lid", "ja-JP".to_string())] {
                                let v = self.doc.append_element(pr, &self.w(name));
                                self.doc.set_attr(v, "w:val", &value);
                            }
                            let rpr = self.doc.child(run, "rPr");
                            for (part, content, small) in [("rt", ruby, true), ("rubyBase", base, false)] {
                                let holder = self.doc.append_element(e, &self.w(part));
                                let r = self.doc.append_element(holder, &self.w("r"));
                                if let Some(rpr) = rpr {
                                    let copy = self.doc.deep_clone(rpr);
                                    self.doc.insert(r, 0, copy);
                                }
                                if small {
                                    let props = match self.doc.child(r, "rPr") {
                                        Some(p) => p,
                                        None => {
                                            let p = self.doc.append_element(r, &self.w("rPr"));
                                            p
                                        }
                                    };
                                    let o = RPR_ORDER;
                                    self.val(props, "sz", Some(&half(*size)), o);
                                }
                                let t = self.doc.append_element(r, &self.w("t"));
                                self.doc.set_text(t, content);
                            }
                            e
                        }
                    };
                    extra.push(node);
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::CheckBox(checked, checked_char, unchecked_char, control) => {
                    flush(self, run, &mut text);
                    let char_to_show = if *checked { *checked_char } else { *unchecked_char };
                    let root = self.doc.root_element().unwrap_or(XmlDocument::ROOT);
                    if self.doc.lookup_prefix(root, W14_NS).is_none() {
                        self.doc.set_attr(root, "xmlns:w14", W14_NS);
                    }
                    let w14 = self.doc.lookup_prefix(root, W14_NS).unwrap_or_else(|| "w14".into());

                    let candidate = original.filter(|&n| {
                        self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").and_then(|pr| self.doc.child(pr, "checkbox")).is_some()
                    }).or_else(|| {
                        node_of.iter().flatten().find(|&&n| {
                            self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").and_then(|pr| self.doc.child(pr, "checkbox")).is_some()
                        }).copied()
                    });

                    let node = match candidate {
                        Some(n) => {
                            let sdt = self.doc.deep_clone(n);
                            if let Some(pr) = self.doc.child(sdt, "sdtPr") {
                                if let Some(cb) = self.doc.child(pr, "checkbox") {
                                    let checked_el = match self.doc.child(cb, "checked") {
                                        Some(c) => c,
                                        None => self.doc.append_element(cb, &format!("{w14}:checked")),
                                    };
                                    self.doc.set_attr(checked_el, &format!("{w14}:val"), if *checked { "1" } else { "0" });
                                }
                            }
                            if let Some(content) = self.doc.child(sdt, "sdtContent") {
                                for r in self.doc.children_named(content, "r").collect::<Vec<_>>() {
                                    if let Some(t) = self.doc.child(r, "t") {
                                        self.doc.set_text(t, &char_to_show.to_string());
                                    }
                                }
                            }
                            sdt
                        }
                        None => {
                            let sdt = self.doc.append_element(first_run, &self.w("sdt"));
                            self.doc.detach(sdt);
                            let pr = self.doc.append_element(sdt, &self.w("sdtPr"));
                            let id = self.doc.append_element(pr, &self.w("id"));
                            let next_id = (new_atoms.len() as u64) * 1000 + (j as u64) + 100000;
                            self.doc.set_attr(id, "w:val", &next_id.to_string());
                            let cb = self.doc.append_element(pr, &format!("{w14}:checkbox"));
                            let chk = self.doc.append_element(cb, &format!("{w14}:checked"));
                            self.doc.set_attr(chk, &format!("{w14}:val"), if *checked { "1" } else { "0" });
                            let cs = self.doc.append_element(cb, &format!("{w14}:checkedState"));
                            self.doc.set_attr(cs, &format!("{w14}:val"), &format!("{:04X}", *checked_char as u32));
                            self.doc.set_attr(cs, &format!("{w14}:font"), "MS Gothic");
                            let ucs = self.doc.append_element(cb, &format!("{w14}:uncheckedState"));
                            self.doc.set_attr(ucs, &format!("{w14}:val"), &format!("{:04X}", *unchecked_char as u32));
                            self.doc.set_attr(ucs, &format!("{w14}:font"), "MS Gothic");

                            let content = self.doc.append_element(sdt, &self.w("sdtContent"));
                            let r = self.doc.append_element(content, &self.w("r"));
                            if let Some(rpr) = self.doc.child(run, "rPr") {
                                let copy = self.doc.deep_clone(rpr);
                                self.doc.insert(r, 0, copy);
                            }
                            let t = self.doc.append_element(r, &self.w("t"));
                            self.doc.set_text(t, &char_to_show.to_string());
                            sdt
                        }
                    };
                    self.apply_control(node, control);
                    extra.push(node);
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::DropDownList(items, selected, combo, control) => {
                    flush(self, run, &mut text);
                    let text_to_show = items.get(*selected).map(|(d, _)| d.as_str()).unwrap_or("");
                    let tag_name = if *combo { "comboBox" } else { "dropDownList" };

                    let candidate = original.filter(|&n| {
                        self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                            self.doc.child(pr, "dropDownList").is_some() || self.doc.child(pr, "comboBox").is_some()
                        }).unwrap_or(false)
                    }).or_else(|| {
                        node_of.iter().flatten().find(|&&n| {
                            self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                                self.doc.child(pr, "dropDownList").is_some() || self.doc.child(pr, "comboBox").is_some()
                            }).unwrap_or(false)
                        }).copied()
                    });

                    let node = match candidate {
                        Some(n) => {
                            let sdt = self.doc.deep_clone(n);
                            if let Some(pr) = self.doc.child(sdt, "sdtPr") {
                                let cur_dd = self.doc.child(pr, "dropDownList");
                                let cur_cb = self.doc.child(pr, "comboBox");
                                if *combo && cur_dd.is_some() {
                                    if let Some(d) = cur_dd {
                                        let items_children = self.doc.children(d).to_vec();
                                        self.doc.detach(d);
                                        let cb = self.doc.append_element(pr, &self.w("comboBox"));
                                        for item in items_children {
                                            let end = self.doc.children(cb).len();
                                            self.doc.insert(cb, end, item);
                                        }
                                    }
                                } else if !*combo && cur_cb.is_some() {
                                    if let Some(c) = cur_cb {
                                        let items_children = self.doc.children(c).to_vec();
                                        self.doc.detach(c);
                                        let dd = self.doc.append_element(pr, &self.w("dropDownList"));
                                        for item in items_children {
                                            let end = self.doc.children(dd).len();
                                            self.doc.insert(dd, end, item);
                                        }
                                    }
                                }
                            }
                            if let Some(content) = self.doc.child(sdt, "sdtContent") {
                                for r in self.doc.children_named(content, "r").collect::<Vec<_>>() {
                                    if let Some(t) = self.doc.child(r, "t") {
                                        self.doc.set_text(t, text_to_show);
                                    }
                                }
                            }
                            sdt
                        }
                        None => {
                            let sdt = self.doc.append_element(first_run, &self.w("sdt"));
                            self.doc.detach(sdt);
                            let pr = self.doc.append_element(sdt, &self.w("sdtPr"));
                            let id = self.doc.append_element(pr, &self.w("id"));
                            let next_id = (new_atoms.len() as u64) * 1000 + (j as u64) + 100000;
                            self.doc.set_attr(id, "w:val", &next_id.to_string());
                            let dd = self.doc.append_element(pr, &self.w(tag_name));
                            for (display, value) in items {
                                let item = self.doc.append_element(dd, &self.w("listItem"));
                                self.doc.set_attr(item, "w:displayText", display);
                                self.doc.set_attr(item, "w:value", value);
                            }

                            let content = self.doc.append_element(sdt, &self.w("sdtContent"));
                            let r = self.doc.append_element(content, &self.w("r"));
                            if let Some(rpr) = self.doc.child(run, "rPr") {
                                let copy = self.doc.deep_clone(rpr);
                                self.doc.insert(r, 0, copy);
                            }
                            let t = self.doc.append_element(r, &self.w("t"));
                            self.doc.set_text(t, text_to_show);
                            sdt
                        }
                    };
                    self.apply_control(node, control);
                    extra.push(node);
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::DatePicker(full_date, date_format, text_to_show, control) => {
                    flush(self, run, &mut text);
                    let candidate = original.filter(|&n| {
                        self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                            self.doc.child(pr, "date").is_some()
                        }).unwrap_or(false)
                    }).or_else(|| {
                        node_of.iter().flatten().find(|&&n| {
                            self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                                self.doc.child(pr, "date").is_some()
                            }).unwrap_or(false)
                        }).copied()
                    });

                    let node = match candidate {
                        Some(n) => {
                            let sdt = self.doc.deep_clone(n);
                            if let Some(pr) = self.doc.child(sdt, "sdtPr") {
                                if let Some(d) = self.doc.child(pr, "date") {
                                    self.doc.set_attr(d, "w:fullDate", full_date);
                                    if let Some(df) = self.doc.child(d, "dateFormat") {
                                        self.doc.set_attr(df, "w:val", date_format);
                                    }
                                }
                            }
                            if let Some(content) = self.doc.child(sdt, "sdtContent") {
                                for r in self.doc.children_named(content, "r").collect::<Vec<_>>() {
                                    if let Some(t) = self.doc.child(r, "t") {
                                        self.doc.set_text(t, text_to_show);
                                    }
                                }
                            }
                            sdt
                        }
                        None => {
                            let sdt = self.doc.append_element(first_run, &self.w("sdt"));
                            self.doc.detach(sdt);
                            let pr = self.doc.append_element(sdt, &self.w("sdtPr"));
                            let id = self.doc.append_element(pr, &self.w("id"));
                            let next_id = (new_atoms.len() as u64) * 1000 + (j as u64) + 100000;
                            self.doc.set_attr(id, "w:val", &next_id.to_string());
                            let date_el = self.doc.append_element(pr, &self.w("date"));
                            self.doc.set_attr(date_el, "w:fullDate", full_date);
                            let df = self.doc.append_element(date_el, &self.w("dateFormat"));
                            self.doc.set_attr(df, "w:val", date_format);
                            let lid = self.doc.append_element(date_el, &self.w("lid"));
                            self.doc.set_attr(lid, "w:val", "ja-JP");
                            let store = self.doc.append_element(date_el, &self.w("storeMappedDataAs"));
                            self.doc.set_attr(store, "w:val", "dateTime");
                            let cal = self.doc.append_element(date_el, &self.w("calendar"));
                            self.doc.set_attr(cal, "w:val", "gregorian");

                            let content = self.doc.append_element(sdt, &self.w("sdtContent"));
                            let r = self.doc.append_element(content, &self.w("r"));
                            if let Some(rpr) = self.doc.child(run, "rPr") {
                                let copy = self.doc.deep_clone(rpr);
                                self.doc.insert(r, 0, copy);
                            }
                            let t = self.doc.append_element(r, &self.w("t"));
                            self.doc.set_text(t, text_to_show);
                            sdt
                        }
                    };
                    self.apply_control(node, control);
                    extra.push(node);
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::RichText(text_to_show, showing, control) => {
                    flush(self, run, &mut text);
                    let candidate = original.filter(|&n| {
                        self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                            self.doc.child(pr, "checkbox").is_none()
                                && self.doc.child(pr, "dropDownList").is_none()
                                && self.doc.child(pr, "comboBox").is_none()
                                && self.doc.child(pr, "date").is_none()
                                && self.doc.child(pr, "text").is_none()
                                && self.doc.child(pr, "group").is_none()
                        }).unwrap_or(false)
                    }).or_else(|| {
                        node_of.iter().flatten().find(|&&n| {
                            self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                                self.doc.child(pr, "checkbox").is_none()
                                    && self.doc.child(pr, "dropDownList").is_none()
                                    && self.doc.child(pr, "comboBox").is_none()
                                    && self.doc.child(pr, "date").is_none()
                                    && self.doc.child(pr, "text").is_none()
                                    && self.doc.child(pr, "group").is_none()
                            }).unwrap_or(false)
                        }).copied()
                    });

                    let node = match candidate {
                        Some(n) => {
                            let sdt = self.doc.deep_clone(n);
                            self.set_control_text(sdt, text_to_show, *showing);
                            sdt
                        }
                        None => {
                            let sdt = self.doc.append_element(first_run, &self.w("sdt"));
                            self.doc.detach(sdt);
                            let pr = self.doc.append_element(sdt, &self.w("sdtPr"));
                            let id = self.doc.append_element(pr, &self.w("id"));
                            let next_id = (new_atoms.len() as u64) * 1000 + (j as u64) + 100000;
                            self.doc.set_attr(id, "w:val", &next_id.to_string());
                            let placeholder = self.doc.append_element(pr, &self.w("placeholder"));
                            let doc_part = self.doc.append_element(placeholder, &self.w("docPart"));
                            self.doc.set_attr(doc_part, "w:val", "DefaultPlaceholder");
                            if *showing {
                                self.doc.append_element(pr, &self.w("showingPlcHdr"));
                            }

                            let content = self.doc.append_element(sdt, &self.w("sdtContent"));
                            let r = self.doc.append_element(content, &self.w("r"));
                            if let Some(rpr) = self.doc.child(run, "rPr") {
                                let copy = self.doc.deep_clone(rpr);
                                self.doc.insert(r, 0, copy);
                            }
                            let t = self.doc.append_element(r, &self.w("t"));
                            self.doc.set_text(t, text_to_show);
                            sdt
                        }
                    };
                    self.apply_control(node, control);
                    extra.push(node);
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::PlainText(text_to_show, showing, control) => {
                    flush(self, run, &mut text);
                    let candidate = original.filter(|&n| {
                        self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                            self.doc.child(pr, "text").is_some()
                        }).unwrap_or(false)
                    }).or_else(|| {
                        node_of.iter().flatten().find(|&&n| {
                            self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                                self.doc.child(pr, "text").is_some()
                            }).unwrap_or(false)
                        }).copied()
                    });

                    let node = match candidate {
                        Some(n) => {
                            let sdt = self.doc.deep_clone(n);
                            self.set_control_text(sdt, text_to_show, *showing);
                            sdt
                        }
                        None => {
                            let sdt = self.doc.append_element(first_run, &self.w("sdt"));
                            self.doc.detach(sdt);
                            let pr = self.doc.append_element(sdt, &self.w("sdtPr"));
                            let id = self.doc.append_element(pr, &self.w("id"));
                            let next_id = (new_atoms.len() as u64) * 1000 + (j as u64) + 100000;
                            self.doc.set_attr(id, "w:val", &next_id.to_string());
                            let placeholder = self.doc.append_element(pr, &self.w("placeholder"));
                            let doc_part = self.doc.append_element(placeholder, &self.w("docPart"));
                            self.doc.set_attr(doc_part, "w:val", "DefaultPlaceholder");
                            if *showing {
                                self.doc.append_element(pr, &self.w("showingPlcHdr"));
                            }
                            self.doc.append_element(pr, &self.w("text"));

                            let content = self.doc.append_element(sdt, &self.w("sdtContent"));
                            let r = self.doc.append_element(content, &self.w("r"));
                            if let Some(rpr) = self.doc.child(run, "rPr") {
                                let copy = self.doc.deep_clone(rpr);
                                self.doc.insert(r, 0, copy);
                            }
                            let t = self.doc.append_element(r, &self.w("t"));
                            self.doc.set_text(t, text_to_show);
                            sdt
                        }
                    };
                    self.apply_control(node, control);
                    extra.push(node);
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::PictureControl(media, cx, cy, look, showing, control) => {
                    flush(self, run, &mut text);
                    let w14 = self.w14_prefix();
                    let candidate = original.filter(|&n| {
                        self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                            self.doc.child(pr, "picture").is_some()
                        }).unwrap_or(false)
                    }).or_else(|| {
                        node_of.iter().flatten().find(|&&n| {
                            self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                                self.doc.child(pr, "picture").is_some()
                            }).unwrap_or(false)
                        }).copied()
                    });

                    let node = match candidate {
                        Some(n) => {
                            let sdt = self.doc.deep_clone(n);
                            if let Some(pr) = self.doc.child(sdt, "sdtPr") {
                                if self.doc.child(pr, "picture").is_none() {
                                    self.doc.append_element(pr, &format!("{w14}:picture"));
                                }
                                let existing_plc = self.doc.child(pr, "showingPlcHdr");
                                if *showing {
                                    if existing_plc.is_none() {
                                        self.doc.append_element(pr, &self.w("showingPlcHdr"));
                                    }
                                } else if let Some(plc) = existing_plc {
                                    self.doc.detach(plc);
                                }
                            }
                            let content = self.doc.child(sdt, "sdtContent").unwrap_or_else(|| {
                                self.doc.append_element(sdt, &self.w("sdtContent"))
                            });
                            let existing_drawing = self.doc.descendants(content).into_iter().find(|&d| self.doc.is_element(d) && self.doc.local_name(d) == "drawing");
                            let drawing = if let Some(drawing) = existing_drawing && !self.added.contains_key(media) {
                                drawing
                            } else {
                                if let Some(old_d) = existing_drawing {
                                    self.doc.detach(old_d);
                                }
                                let d = if let Some(rid) = self.added.get(media) {
                                    self.new_picture(rid, *cx, *cy, self.svg_media.contains(media))
                                } else if let Some(&(para, index)) = self.pictures.get(media) {
                                    let source = &self.map.paragraphs[&para];
                                    if source.part != self.part {
                                        return Err(unsupported("ほかのパーツからの画像の貼り付け"));
                                    }
                                    let AtomSource::InRun { node, .. } = &source.atoms[index] else {
                                        return Err(unsupported("画像の挿入"));
                                    };
                                    let mut at = self.pristine.by_path(&source.path).ok_or_else(|| unsupported("画像の挿入"))?;
                                    for &i in node {
                                        at = self.pristine.elements(at).nth(i as usize).ok_or_else(|| unsupported("画像の挿入"))?;
                                    }
                                    self.doc.import(self.pristine, at)
                                } else {
                                    return Err(unsupported("画像コントロールの描画"));
                                };
                                let r = match self.doc.child(content, "r") {
                                    Some(r) => r,
                                    None => {
                                        let name = self.w("r");
                                        self.doc.append_element(content, &name)
                                    }
                                };
                                let end = self.doc.children(r).len();
                                self.doc.insert(r, end, d);
                                d
                            };
                            self.configure_drawing_anchor(drawing, None);
                            self.set_picture_size(drawing, *cx, *cy);
                            self.set_picture_look(drawing, look);
                            sdt
                        }
                        None => {
                            let sdt = self.doc.append_element(first_run, &self.w("sdt"));
                            self.doc.detach(sdt);
                            let pr = self.doc.append_element(sdt, &self.w("sdtPr"));
                            let id = self.doc.append_element(pr, &self.w("id"));
                            let next_id = (new_atoms.len() as u64) * 1000 + (j as u64) + 100000;
                            self.doc.set_attr(id, "w:val", &next_id.to_string());
                            let placeholder = self.doc.append_element(pr, &self.w("placeholder"));
                            let doc_part = self.doc.append_element(placeholder, &self.w("docPart"));
                            self.doc.set_attr(doc_part, "w:val", "DefaultPlaceholder");
                            if *showing {
                                self.doc.append_element(pr, &self.w("showingPlcHdr"));
                            }
                            self.doc.append_element(pr, &format!("{w14}:picture"));

                            let content = self.doc.append_element(sdt, &self.w("sdtContent"));
                            let r = self.doc.append_element(content, &self.w("r"));
                            if let Some(rpr) = self.doc.child(run, "rPr") {
                                let copy = self.doc.deep_clone(rpr);
                                self.doc.insert(r, 0, copy);
                            }
                            let drawing = if let Some(rid) = self.added.get(media) {
                                self.new_picture(rid, *cx, *cy, self.svg_media.contains(media))
                            } else if let Some(&(para, index)) = self.pictures.get(media) {
                                let source = &self.map.paragraphs[&para];
                                if source.part != self.part {
                                    return Err(unsupported("ほかのパーツからの画像の貼り付け"));
                                }
                                let AtomSource::InRun { node, .. } = &source.atoms[index] else {
                                    return Err(unsupported("画像の挿入"));
                                };
                                let mut at = self.pristine.by_path(&source.path).ok_or_else(|| unsupported("画像の挿入"))?;
                                for &i in node {
                                    at = self.pristine.elements(at).nth(i as usize).ok_or_else(|| unsupported("画像の挿入"))?;
                                }
                                self.doc.import(self.pristine, at)
                            } else {
                                return Err(unsupported("画像コントロールの描画"));
                            };
                            self.configure_drawing_anchor(drawing, None);
                            self.set_picture_size(drawing, *cx, *cy);
                            self.set_picture_look(drawing, look);
                            let end = self.doc.children(r).len();
                            self.doc.insert(r, end, drawing);
                            sdt
                        }
                    };
                    self.apply_control(node, control);
                    extra.push(node);
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::BuildingBlockGallery(gallery, category, text_to_show, showing, control) => {
                    flush(self, run, &mut text);
                    let candidate = original.filter(|&n| {
                        self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                            self.doc.child(pr, "docPartList").is_some() || self.doc.child(pr, "docPartObj").is_some()
                        }).unwrap_or(false)
                    }).or_else(|| {
                        node_of.iter().flatten().find(|&&n| {
                            self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                                self.doc.child(pr, "docPartList").is_some() || self.doc.child(pr, "docPartObj").is_some()
                            }).unwrap_or(false)
                        }).copied()
                    });

                    let node = match candidate {
                        Some(n) => {
                            let sdt = self.doc.deep_clone(n);
                            self.set_control_text(sdt, text_to_show, *showing);
                            if let Some(pr) = self.doc.child(sdt, "sdtPr") {
                                let dp = self.doc.child(pr, "docPartList").or_else(|| self.doc.child(pr, "docPartObj")).unwrap_or_else(|| {
                                    self.doc.append_element(pr, &self.w("docPartList"))
                                });
                                let dpg = self.doc.child(dp, "docPartGallery").unwrap_or_else(|| {
                                    self.doc.append_element(dp, &self.w("docPartGallery"))
                                });
                                self.doc.set_attr(dpg, "w:val", gallery);
                                let dpc = self.doc.child(dp, "docPartCategory").unwrap_or_else(|| {
                                    self.doc.append_element(dp, &self.w("docPartCategory"))
                                });
                                self.doc.set_attr(dpc, "w:val", category);
                            }
                            sdt
                        }
                        None => {
                            let sdt = self.doc.append_element(first_run, &self.w("sdt"));
                            self.doc.detach(sdt);
                            let pr = self.doc.append_element(sdt, &self.w("sdtPr"));
                            let id = self.doc.append_element(pr, &self.w("id"));
                            let next_id = (new_atoms.len() as u64) * 1000 + (j as u64) + 100000;
                            self.doc.set_attr(id, "w:val", &next_id.to_string());
                            let placeholder = self.doc.append_element(pr, &self.w("placeholder"));
                            let doc_part = self.doc.append_element(placeholder, &self.w("docPart"));
                            self.doc.set_attr(doc_part, "w:val", "DefaultPlaceholder");
                            if *showing {
                                self.doc.append_element(pr, &self.w("showingPlcHdr"));
                            }
                            let dp = self.doc.append_element(pr, &self.w("docPartList"));
                            let dpg = self.doc.append_element(dp, &self.w("docPartGallery"));
                            self.doc.set_attr(dpg, "w:val", gallery);
                            let dpc = self.doc.append_element(dp, &self.w("docPartCategory"));
                            self.doc.set_attr(dpc, "w:val", category);

                            let content = self.doc.append_element(sdt, &self.w("sdtContent"));
                            let r = self.doc.append_element(content, &self.w("r"));
                            if let Some(rpr) = self.doc.child(run, "rPr") {
                                let copy = self.doc.deep_clone(rpr);
                                self.doc.insert(r, 0, copy);
                            }
                            let t = self.doc.append_element(r, &self.w("t"));
                            self.doc.set_text(t, text_to_show);
                            sdt
                        }
                    };
                    self.apply_control(node, control);
                    extra.push(node);
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::RepeatingSection(section_title, items, _selected, no_edit, control) => {
                    flush(self, run, &mut text);
                    let w15 = self.w15_prefix();
                    let rep_elem = format!("{w15}:repeatingSection");
                    let title_elem = format!("{w15}:sectionTitle");
                    let dna_elem = format!("{w15}:doNotAllowInsertDelete");
                    let item_elem = format!("{w15}:repeatingSectionItem");

                    let candidate = original.filter(|&n| {
                        self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                            self.doc.child(pr, "repeatingSection").is_some()
                        }).unwrap_or(false)
                    }).or_else(|| {
                        node_of.iter().flatten().find(|&&n| {
                            self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                                self.doc.child(pr, "repeatingSection").is_some()
                            }).unwrap_or(false)
                        }).copied()
                    });

                    let node = match candidate {
                        Some(n) => {
                            let sdt = self.doc.deep_clone(n);
                            if let Some(pr) = self.doc.child(sdt, "sdtPr") {
                                let rep = self.doc.child(pr, "repeatingSection").unwrap_or_else(|| {
                                    self.doc.append_element(pr, &rep_elem)
                                });
                                let st = self.doc.child(rep, "sectionTitle").unwrap_or_else(|| {
                                    self.doc.append_element(rep, &title_elem)
                                });
                                self.doc.set_attr(st, "w:val", section_title);
                                if *no_edit {
                                    if self.doc.child(rep, "doNotAllowInsertDelete").is_none() {
                                        self.doc.append_element(rep, &dna_elem);
                                    }
                                } else if let Some(dna) = self.doc.child(rep, "doNotAllowInsertDelete") {
                                    self.doc.detach(dna);
                                }
                            }
                            if let Some(content) = self.doc.child(sdt, "sdtContent") {
                                for ch in self.doc.children(content).to_vec() {
                                    self.doc.detach(ch);
                                }
                                for (idx, item_text) in items.iter().enumerate() {
                                    let item_sdt = self.doc.append_element(content, &self.w("sdt"));
                                    let item_pr = self.doc.append_element(item_sdt, &self.w("sdtPr"));
                                    let item_id = self.doc.append_element(item_pr, &self.w("id"));
                                    let next_id = (new_atoms.len() as u64) * 1000 + (j as u64) + (idx as u64) + 200000;
                                    self.doc.set_attr(item_id, "w:val", &next_id.to_string());
                                    self.doc.append_element(item_pr, &item_elem);
                                    let item_content = self.doc.append_element(item_sdt, &self.w("sdtContent"));
                                    let r = self.doc.append_element(item_content, &self.w("r"));
                                    if let Some(rpr) = self.doc.child(run, "rPr") {
                                        let copy = self.doc.deep_clone(rpr);
                                        self.doc.insert(r, 0, copy);
                                    }
                                    let t = self.doc.append_element(r, &self.w("t"));
                                    self.doc.set_text(t, item_text);
                                }
                            }
                            sdt
                        }
                        None => {
                            let sdt = self.doc.append_element(first_run, &self.w("sdt"));
                            self.doc.detach(sdt);
                            let pr = self.doc.append_element(sdt, &self.w("sdtPr"));
                            let id = self.doc.append_element(pr, &self.w("id"));
                            let next_id = (new_atoms.len() as u64) * 1000 + (j as u64) + 100000;
                            self.doc.set_attr(id, "w:val", &next_id.to_string());
                            let rep = self.doc.append_element(pr, &rep_elem);
                            let st = self.doc.append_element(rep, &title_elem);
                            self.doc.set_attr(st, "w:val", section_title);
                            if *no_edit {
                                self.doc.append_element(rep, &dna_elem);
                            }
                            let content = self.doc.append_element(sdt, &self.w("sdtContent"));
                            for (idx, item_text) in items.iter().enumerate() {
                                let item_sdt = self.doc.append_element(content, &self.w("sdt"));
                                let item_pr = self.doc.append_element(item_sdt, &self.w("sdtPr"));
                                let item_id = self.doc.append_element(item_pr, &self.w("id"));
                                let next_id = (new_atoms.len() as u64) * 1000 + (j as u64) + (idx as u64) + 200000;
                                self.doc.set_attr(item_id, "w:val", &next_id.to_string());
                                self.doc.append_element(item_pr, &item_elem);
                                let item_content = self.doc.append_element(item_sdt, &self.w("sdtContent"));
                                let r = self.doc.append_element(item_content, &self.w("r"));
                                if let Some(rpr) = self.doc.child(run, "rPr") {
                                    let copy = self.doc.deep_clone(rpr);
                                    self.doc.insert(r, 0, copy);
                                }
                                let t = self.doc.append_element(r, &self.w("t"));
                                self.doc.set_text(t, item_text);
                            }
                            sdt
                        }
                    };
                    self.apply_control(node, control);
                    extra.push(node);
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::Group(text_to_show, showing, control) => {
                    flush(self, run, &mut text);
                    let candidate = original.filter(|&n| {
                        self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                            self.doc.child(pr, "group").is_some()
                        }).unwrap_or(false)
                    }).or_else(|| {
                        node_of.iter().flatten().find(|&&n| {
                            self.doc.local_name(n) == "sdt" && self.doc.child(n, "sdtPr").map(|pr| {
                                self.doc.child(pr, "group").is_some()
                            }).unwrap_or(false)
                        }).copied()
                    });

                    let node = match candidate {
                        Some(n) => {
                            let sdt = self.doc.deep_clone(n);
                            self.set_control_text(sdt, text_to_show, *showing);
                            sdt
                        }
                        None => {
                            let sdt = self.doc.append_element(first_run, &self.w("sdt"));
                            self.doc.detach(sdt);
                            let pr = self.doc.append_element(sdt, &self.w("sdtPr"));
                            let id = self.doc.append_element(pr, &self.w("id"));
                            let next_id = (new_atoms.len() as u64) * 1000 + (j as u64) + 100000;
                            self.doc.set_attr(id, "w:val", &next_id.to_string());
                            let placeholder = self.doc.append_element(pr, &self.w("placeholder"));
                            let doc_part = self.doc.append_element(placeholder, &self.w("docPart"));
                            self.doc.set_attr(doc_part, "w:val", "DefaultPlaceholder");
                            if *showing {
                                self.doc.append_element(pr, &self.w("showingPlcHdr"));
                            }
                            self.doc.append_element(pr, &self.w("group"));

                            let content = self.doc.append_element(sdt, &self.w("sdtContent"));
                            let r = self.doc.append_element(content, &self.w("r"));
                            if let Some(rpr) = self.doc.child(run, "rPr") {
                                let copy = self.doc.deep_clone(rpr);
                                self.doc.insert(r, 0, copy);
                            }
                            let t = self.doc.append_element(r, &self.w("t"));
                            self.doc.set_text(t, text_to_show);
                            sdt
                        }
                    };
                    self.apply_control(node, control);
                    extra.push(node);
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::Picture(media, cx, cy, look) => {
                    flush(self, run, &mut text);
                    let node = match original {
                        Some(n) => self.doc.deep_clone(n),
                        None if self.added.contains_key(media) => self.new_picture(&self.added[media].clone(), *cx, *cy, self.svg_media.contains(media)),
                        None => {
                            let &(para, index) = self.pictures.get(media).ok_or_else(|| unsupported("画像の挿入"))?;
                            let source = &self.map.paragraphs[&para];
                            if source.part != self.part {
                                return Err(unsupported("ほかのパーツからの画像の貼り付け"));
                            }
                            let AtomSource::InRun { node, .. } = &source.atoms[index] else { return Err(unsupported("画像の挿入")) };
                            let mut at = self.pristine.by_path(&source.path).ok_or_else(|| unsupported("画像の挿入"))?;
                            for &i in node {
                                at = self.pristine.elements(at).nth(i as usize).ok_or_else(|| unsupported("画像の挿入"))?;
                            }
                            self.doc.import(self.pristine, at)
                        }
                    };
                    self.configure_drawing_anchor(node, None);
                    self.set_picture_size(node, *cx, *cy);
                    self.set_picture_look(node, look);
                    let end = self.doc.children(run).len();
                    self.doc.insert(run, end, node);
                }
                AtomKey::Shape(shape) => {
                    flush(self, run, &mut text);
                    if let Some(group) = Self::shape_group_of(self.new_shape_groups, shape.id) {
                        let leaves = group.leaf_shape_ids();
                        if leaves.first().copied() == Some(shape.id) {
                            let node = if let Some(n) = original.filter(|&n| {
                                self.doc.descendants(n).into_iter().any(|d| self.doc.is_element(d) && self.doc.local_name(d) == "wgp")
                            }) {
                                self.doc.deep_clone(n)
                            } else {
                                let group_shapes: Vec<&Shape> = leaves
                                    .iter()
                                    .filter_map(|&id| {
                                        para.runs.iter().find_map(|r| match &r.kind {
                                            FlowRunKind::Shape(s) if s.id == id => Some(s.as_ref()),
                                            _ => None,
                                        })
                                    })
                                    .collect();
                                self.new_group_shape(group, &group_shapes)
                            };
                            let end = self.doc.children(run).len();
                            self.doc.insert(run, end, node);
                        }
                    } else if let ShapeKind::Picture(pic) = &shape.kind {
                        let media = &pic.media;
                        let cx = shape.frame.width;
                        let cy = shape.frame.height;
                        let look = ibis_doc_model::PictureLook {
                            rotation: shape.rotation,
                            flip_h: shape.flip_h,
                            flip_v: shape.flip_v,
                            crop: pic.crop,
                            stroke: pic.stroke.clone(),
                            luminance: pic.luminance,
                            duotone: pic.duotone,
                            alt_text: pic.alt_text.clone(),
                            decorative: pic.decorative,
                            name: pic.name.clone(),
                            linked_file: None,
                            soft_edge: pic.soft_edge,
                            effects: pic.effects,
                            geometry: pic.geometry.clone(),
                        };
                        let node = match original {
                            Some(n) => self.doc.deep_clone(n),
                            None if self.added.contains_key(media) => self.new_picture(&self.added[media].clone(), cx, cy, self.svg_media.contains(media)),
                            None => {
                                let &(para, index) = self.pictures.get(media).ok_or_else(|| unsupported("画像の挿入"))?;
                                let source = &self.map.paragraphs[&para];
                                if source.part != self.part {
                                    return Err(unsupported("ほかのパーツからの画像の貼り付け"));
                                }
                                let AtomSource::InRun { node, .. } = &source.atoms[index] else { return Err(unsupported("画像の挿入")) };
                                let mut at = self.pristine.by_path(&source.path).ok_or_else(|| unsupported("画像の挿入"))?;
                                for &i in node {
                                    at = self.pristine.elements(at).nth(i as usize).ok_or_else(|| unsupported("画像の挿入"))?;
                                }
                                self.doc.import(self.pristine, at)
                            }
                        };
                        self.configure_drawing_anchor(node, shape.floating.as_ref());
                        self.set_picture_size(node, cx, cy);
                        self.set_picture_look(node, &look);
                        let end = self.doc.children(run).len();
                        self.doc.insert(run, end, node);
                    } else if let Some(n) = original {
                        let node = self.doc.deep_clone(n);
                        let end = self.doc.children(run).len();
                        self.doc.insert(run, end, node);
                    } else if let (ShapeKind::Chart(_), Some(n)) = (&shape.kind, self.shape_nodes.get(&shape.id.0).copied()) {
                        // A chart of the file, changed: its drawing as it was (the chart part is written apart).
                        let node = self.doc.deep_clone(n);
                        let end = self.doc.children(run).len();
                        self.doc.insert(run, end, node);
                    } else if let ShapeKind::Chart(_) = &shape.kind {
                        // A new chart (an old one was copied above): an inline drawing of its part.
                        let rel = NEW_CHART_RELS.with(|r| r.borrow().get(&shape.id.0).cloned()).ok_or_else(|| unsupported("グラフの挿入"))?;
                        let node = self.new_chart_drawing(&rel, shape.frame.width, shape.frame.height);
                        let end = self.doc.children(run).len();
                        self.doc.insert(run, end, node);
                    } else if matches!(shape.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_) | ShapeKind::Line(_)) {
                        // A shape (a text box, a drawn shape): an edited one (found by its id) is its
                        // old element with the new text, size, place, fill and line; a new one is
                        // written whole.
                        let body = shape_body(shape);
                        let existing_wsp = self.shape_nodes.get(&shape.id.0).copied().filter(|&n| {
                            !self.doc.descendants(n).into_iter().any(|d| self.doc.is_element(d) && self.doc.local_name(d) == "wgp")
                        });
                        let node = match existing_wsp {
                            Some(n) => {
                                let node = self.doc.deep_clone(n);
                                let old = self.shape_olds.get(&shape.id.0).cloned();
                                self.update_shape(node, shape, body, old.as_deref());
                                node
                            }
                            None => self.new_shape(shape, body),
                        };
                        let end = self.doc.children(run).len();
                        self.doc.insert(run, end, node);
                    }
                }
                AtomKey::Math(math, display) => {
                    flush(self, run, &mut text);
                    let node = if let Some(n) = original.filter(|&n| matches!(self.doc.local_name(n), "oMath" | "oMathPara")) {
                        self.doc.deep_clone(n)
                    } else {
                        let xml = if *display {
                            crate::math_xml::write_math(math)
                        } else {
                            crate::math_xml::write_math_inline(math)
                        };
                        let temp_doc = ibis_ooxml::XmlDocument::parse(xml.as_bytes()).map_err(|_| unsupported("数式の挿入"))?;
                        let root = temp_doc.root_element().ok_or_else(|| unsupported("数式の挿入"))?;
                        self.doc.import(&temp_doc, root)
                    };
                    extra.push(node);
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::CommentStart(id) | AtomKey::CommentEnd(id) => {
                    // Between runs: what follows goes into a new run after it.
                    flush(self, run, &mut text);
                    let name = if matches!(new_atoms[j].0, AtomKey::CommentStart(_)) { "commentRangeStart" } else { "commentRangeEnd" };
                    let e = self.doc.append_element(run, &self.w(name));
                    self.doc.detach(e);
                    self.doc.set_attr(e, "w:id", &id.to_string());
                    extra.push(e);
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::CommentRef(id) => {
                    flush(self, run, &mut text);
                    let node = match original.filter(|&n| self.doc.local_name(n) == "commentReference") {
                        Some(n) => self.doc.deep_clone(n),
                        None => {
                            let e = self.doc.append_element(run, &self.w("commentReference"));
                            self.doc.detach(e);
                            self.doc.set_attr(e, "w:id", &id.to_string());
                            e
                        }
                    };
                    let end = self.doc.children(run).len();
                    self.doc.insert(run, end, node);
                }
                AtomKey::NoteRef(..) | AtomKey::NoteMark => {
                    flush(self, run, &mut text);
                    let node = match original.filter(|&n| matches!(self.doc.local_name(n), "footnoteReference" | "endnoteReference" | "footnoteRef" | "endnoteRef")) {
                        Some(n) if matches!((&new_atoms[j].0, self.doc.local_name(n)), (AtomKey::NoteRef(false, _), "footnoteReference") | (AtomKey::NoteRef(true, _), "endnoteReference") | (AtomKey::NoteMark, _)) => self.doc.deep_clone(n),
                        _ => {
                            let (name, id) = match &new_atoms[j].0 {
                                AtomKey::NoteRef(endnote, id) => (if *endnote { "endnoteReference" } else { "footnoteReference" }, Some(*id)),
                                _ => (if self.part.contains("endnote") { "endnoteRef" } else { "footnoteRef" }, None),
                            };
                            let e = self.doc.append_element(run, &self.w(name));
                            self.doc.detach(e);
                            if let Some(id) = id {
                                self.doc.set_attr(e, "w:id", &id.to_string());
                            }
                            e
                        }
                    };
                    let end = self.doc.children(run).len();
                    self.doc.insert(run, end, node);
                }
                AtomKey::FormField(ff) => {
                    flush(self, run, &mut text);
                    let instr = match &ff.kind {
                        ibis_doc_model::FormFieldKind::TextInput { .. } => " FORMTEXT ".to_string(),
                        ibis_doc_model::FormFieldKind::CheckBox { .. } => " FORMCHECKBOX ".to_string(),
                        ibis_doc_model::FormFieldKind::DropDown { .. } => " FORMDROPDOWN ".to_string(),
                    };
                    let shown = match &ff.kind {
                        ibis_doc_model::FormFieldKind::TextInput { value, default, .. } => {
                            if value.is_empty() {
                                if !default.is_empty() {
                                    default.clone()
                                } else {
                                    "\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}".to_string()
                                }
                            } else {
                                value.clone()
                            }
                        }
                        ibis_doc_model::FormFieldKind::CheckBox { .. } => String::new(),
                        ibis_doc_model::FormFieldKind::DropDown { items, selected, .. } => {
                            items.get(*selected).cloned().unwrap_or_default()
                        }
                    };

                    // 1. begin run with ffData
                    let r_begin = self.doc.deep_clone(run);
                    for child in self.doc.children(r_begin).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    let fld_begin = self.doc.append_element(r_begin, &self.w("fldChar"));
                    self.doc.set_attr(fld_begin, "w:fldCharType", "begin");
                    let ff_data = self.doc.append_element(fld_begin, &self.w("ffData"));
                    if !ff.name.is_empty() {
                        let n = self.doc.append_element(ff_data, &self.w("name"));
                        self.doc.set_attr(n, "w:val", &ff.name);
                    }
                    let en = self.doc.append_element(ff_data, &self.w("enabled"));
                    if !ff.enabled {
                        self.doc.set_attr(en, "w:val", "0");
                    }
                    let calc = self.doc.append_element(ff_data, &self.w("calcOnExit"));
                    self.doc.set_attr(calc, "w:val", if ff.calc_on_exit { "1" } else { "0" });
                    match &ff.kind {
                        ibis_doc_model::FormFieldKind::TextInput { default, max_length, .. } => {
                            let ti = self.doc.append_element(ff_data, &self.w("textInput"));
                            if !default.is_empty() {
                                let d = self.doc.append_element(ti, &self.w("default"));
                                self.doc.set_attr(d, "w:val", default);
                            }
                            if *max_length > 0 {
                                let m = self.doc.append_element(ti, &self.w("maxLength"));
                                self.doc.set_attr(m, "w:val", &max_length.to_string());
                            }
                        }
                        ibis_doc_model::FormFieldKind::CheckBox { checked, default, size_auto } => {
                            let cb = self.doc.append_element(ff_data, &self.w("checkBox"));
                            if *size_auto {
                                self.doc.append_element(cb, &self.w("sizeAuto"));
                            }
                            let def = self.doc.append_element(cb, &self.w("default"));
                            self.doc.set_attr(def, "w:val", if *default { "1" } else { "0" });
                            if *checked {
                                let chk = self.doc.append_element(cb, &self.w("checked"));
                                self.doc.set_attr(chk, "w:val", "1");
                            }
                        }
                        ibis_doc_model::FormFieldKind::DropDown { items, selected, default } => {
                            let dd = self.doc.append_element(ff_data, &self.w("ddList"));
                            let res = self.doc.append_element(dd, &self.w("result"));
                            self.doc.set_attr(res, "w:val", &selected.to_string());
                            let def = self.doc.append_element(dd, &self.w("default"));
                            self.doc.set_attr(def, "w:val", &default.to_string());
                            for item in items {
                                let le = self.doc.append_element(dd, &self.w("listEntry"));
                                self.doc.set_attr(le, "w:val", item);
                            }
                        }
                    }
                    extra.push(r_begin);

                    // 2. instrText run
                    let r_instr = self.doc.deep_clone(run);
                    for child in self.doc.children(r_instr).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    let it = self.doc.append_element(r_instr, &self.w("instrText"));
                    self.doc.set_attr(it, "xml:space", "preserve");
                    self.doc.set_text(it, &instr);
                    extra.push(r_instr);

                    // 3. separate run
                    let r_sep = self.doc.deep_clone(run);
                    for child in self.doc.children(r_sep).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    let fld_sep = self.doc.append_element(r_sep, &self.w("fldChar"));
                    self.doc.set_attr(fld_sep, "w:fldCharType", "separate");
                    extra.push(r_sep);

                    // 4. result run (if non-empty shown)
                    if !shown.is_empty() {
                        let r_res = self.doc.deep_clone(run);
                        for child in self.doc.children(r_res).to_vec() {
                            if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                                self.doc.detach(child);
                            }
                        }
                        let t = self.doc.append_element(r_res, &self.w("t"));
                        self.doc.set_attr(t, "xml:space", "preserve");
                        self.doc.set_text(t, &shown);
                        extra.push(r_res);
                    }

                    // 5. end run
                    let r_end = self.doc.deep_clone(run);
                    for child in self.doc.children(r_end).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    let fld_end = self.doc.append_element(r_end, &self.w("fldChar"));
                    self.doc.set_attr(fld_end, "w:fldCharType", "end");
                    extra.push(r_end);

                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
                AtomKey::PageNumber(_) | AtomKey::FileName(..) | AtomKey::PageRef(..) | AtomKey::Field(..) => {
                    // A new field: begin, instruction, separate, result, end.
                    flush(self, run, &mut text);
                    let (instr, shown, locked) = match &new_atoms[j].0 {
                        AtomKey::FileName(name, l) => (" FILENAME ".to_string(), name.clone(), *l),
                        AtomKey::PageRef(bookmark, shown, l) => (format!(" PAGEREF {bookmark} \\h "), shown.clone(), *l),
                        AtomKey::Field(instruction, shown, l) => (format!(" {instruction} "), shown.clone(), *l),
                        AtomKey::PageNumber(l) => (" PAGE ".to_string(), "1".to_string(), *l),
                        _ => unreachable!(),
                    };
                    let parts: [(&str, Option<&str>); 5] = [("begin", None), ("instr", Some(&instr)), ("separate", None), ("result", Some(&shown)), ("end", None)];
                    for (kind, value) in parts {
                        let r = self.doc.deep_clone(run);
                        for child in self.doc.children(r).to_vec() {
                            if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                                self.doc.detach(child);
                            }
                        }
                        match kind {
                            "instr" => {
                                let e = self.doc.append_element(r, &self.w("instrText"));
                                self.doc.set_attr(e, "xml:space", "preserve");
                                self.doc.set_text(e, value.unwrap_or(""));
                            }
                            "result" => {
                                let e = self.doc.append_element(r, &self.w("t"));
                                self.doc.set_text(e, value.unwrap_or(""));
                            }
                            _ => {
                                let e = self.doc.append_element(r, &self.w("fldChar"));
                                self.doc.set_attr(e, "w:fldCharType", kind);
                                if kind == "begin" && locked {
                                    self.doc.set_attr(e, "w:fldLock", "1");
                                }
                            }
                        }
                        extra.push(r);
                    }
                    // What follows the field: a new run like the host.
                    let next = self.doc.deep_clone(first_run);
                    for child in self.doc.children(next).to_vec() {
                        if !(self.doc.is_element(child) && self.doc.local_name(child) == "rPr") {
                            self.doc.detach(child);
                        }
                    }
                    extra.push(next);
                    run = next;
                }
            }
        }
        flush(self, run, &mut text);
        // A trailing copy left empty is not kept.
        while let Some(&last) = extra.last() {
            if self.doc.local_name(last) == "r" && self.doc.elements(last).all(|c| self.doc.local_name(c) == "rPr") {
                extra.pop();
            } else {
                break;
            }
        }
        Ok(extra)
    }

    /// The child `local` of `parent`, made in its place in `order` if missing.
    fn ensure(&mut self, parent: NodeId, local: &str, order: &[&str]) -> NodeId {
        if let Some(c) = self.doc.child(parent, local) {
            return c;
        }
        let rank = order.iter().position(|n| *n == local).unwrap_or(order.len());
        let children = self.doc.children(parent).to_vec();
        let mut index = children.len();
        for (i, &c) in children.iter().enumerate() {
            if self.doc.is_element(c) {
                let r = order.iter().position(|n| *n == self.doc.local_name(c)).unwrap_or(usize::MAX);
                if r != usize::MAX && r > rank {
                    index = i;
                    break;
                }
            }
        }
        let name = self.w(local);
        self.doc.insert_element(parent, index, &name)
    }

    fn remove_child(&mut self, parent: NodeId, local: &str) {
        while let Some(c) = self.doc.child(parent, local) {
            self.doc.detach(c);
        }
    }

    /// Set an on/off property: `<w:b/>`, `<w:b w:val="0"/>`, or none.
    fn toggle(&mut self, rpr: NodeId, names: &[&str], value: Option<bool>, order: &[&str]) {
        for name in names {
            match value {
                None => self.remove_child(rpr, name),
                Some(v) => {
                    let e = self.ensure(rpr, name, order);
                    if v {
                        self.doc.remove_attr(e, "w:val");
                    } else {
                        self.doc.set_attr(e, "w:val", "0");
                    }
                }
            }
        }
    }

    fn val(&mut self, parent: NodeId, local: &str, value: Option<&str>, order: &[&str]) {
        match value {
            None => self.remove_child(parent, local),
            Some(v) => {
                let e = self.ensure(parent, local, order);
                self.doc.set_attr(e, "w:val", v);
            }
        }
    }

    fn w14_prefix(&mut self) -> String {
        let root = self.doc.root_element().unwrap_or(XmlDocument::ROOT);
        if self.doc.lookup_prefix(root, W14_NS).is_none() {
            self.doc.set_attr(root, "xmlns:w14", W14_NS);
        }
        self.doc.lookup_prefix(root, W14_NS).unwrap_or_else(|| "w14".into())
    }

    fn w15_prefix(&mut self) -> String {
        let root = self.doc.root_element().unwrap_or(XmlDocument::ROOT);
        if self.doc.lookup_prefix(root, W15_NS).is_none() {
            self.doc.set_attr(root, "xmlns:w15", W15_NS);
        }
        self.doc.lookup_prefix(root, W15_NS).unwrap_or_else(|| "w15".into())
    }

    /// Change the run properties of `run` (a `w:r`, or a `w:pPr` when
    /// `mark`) from `(f0, c0)` to `(f1, c1)`: only what differs is written.
    fn run_props(&mut self, run: NodeId, f0: &RunFormat, c0: &Option<String>, f1: &RunFormat, c1: &Option<String>, mark: bool) {
        if f0 == f1 && c0 == c1 {
            return;
        }
        let rpr = match (mark, self.doc.child(run, "rPr")) {
            (true, _) => self.ensure(run, "rPr", PPR_ORDER),
            (false, Some(rpr)) => rpr,
            (false, None) => {
                let name = self.w("rPr");
                self.doc.insert_element(run, 0, &name)
            }
        };
        let o = RPR_ORDER;
        if c0 != c1 {
            self.val(rpr, "rStyle", c1.as_deref(), o);
        }
        if f0.font != f1.font || f0.east_asian_font != f1.east_asian_font || f0.cs_font != f1.cs_font || f0.east_asia_hint != f1.east_asia_hint {
            let fonts = self.ensure(rpr, "rFonts", o);
            if f0.east_asia_hint != f1.east_asia_hint {
                // `w:hint` is the run's own: a copy of another run's takes the model's (the reader reads `eastAsia` as true, else false).
                match f1.east_asia_hint {
                    Some(true) => self.doc.set_attr(fonts, "w:hint", "eastAsia"),
                    Some(false) => self.doc.set_attr(fonts, "w:hint", "default"),
                    None => self.doc.remove_attr(fonts, "w:hint"),
                }
            }
            if f0.font != f1.font {
                for a in ["w:asciiTheme", "w:hAnsiTheme"] {
                    self.doc.remove_attr(fonts, a);
                }
                match &f1.font {
                    Some(f) => {
                        self.doc.set_attr(fonts, "w:ascii", f);
                        self.doc.set_attr(fonts, "w:hAnsi", f);
                    }
                    None => {
                        self.doc.remove_attr(fonts, "w:ascii");
                        self.doc.remove_attr(fonts, "w:hAnsi");
                    }
                }
            }
            if f0.east_asian_font != f1.east_asian_font {
                self.doc.remove_attr(fonts, "w:eastAsiaTheme");
                match &f1.east_asian_font {
                    Some(f) => self.doc.set_attr(fonts, "w:eastAsia", f),
                    None => self.doc.remove_attr(fonts, "w:eastAsia"),
                }
            }
            if f0.cs_font != f1.cs_font {
                self.doc.remove_attr(fonts, "w:cstheme");
                match &f1.cs_font {
                    Some(f) => self.doc.set_attr(fonts, "w:cs", f),
                    None => self.doc.remove_attr(fonts, "w:cs"),
                }
            }
            if f0.east_asia_hint != f1.east_asia_hint {
                match f1.east_asia_hint {
                    Some(true)=>self.doc.set_attr(fonts,"w:hint","eastAsia"),
                    Some(false)=>self.doc.set_attr(fonts,"w:hint","default"),
                    None=>self.doc.remove_attr(fonts,"w:hint"),
                }
            }
            if self.doc.attributes(fonts).is_empty() {
                self.doc.detach(fonts);
            }
        }
        if f0.lang != f1.lang || f0.lang_east_asia != f1.lang_east_asia || f0.lang_bidi != f1.lang_bidi {
            let e = self.ensure(rpr, "lang", o);
            for (attr, now, was) in [("w:val", &f1.lang, &f0.lang), ("w:eastAsia", &f1.lang_east_asia, &f0.lang_east_asia), ("w:bidi", &f1.lang_bidi, &f0.lang_bidi)] {
                if now != was {
                    match now {
                        Some(v) => self.doc.set_attr(e, attr, v),
                        None => self.doc.remove_attr(e, attr),
                    }
                }
            }
            if self.doc.attributes(e).is_empty() {
                self.doc.detach(e);
            }
        }
        if f0.bold != f1.bold {
            self.toggle(rpr, &["b", "bCs"], f1.bold, o);
        }
        if f0.italic != f1.italic {
            self.toggle(rpr, &["i", "iCs"], f1.italic, o);
        }
        if f0.strike != f1.strike {
            self.toggle(rpr, &["strike"], f1.strike, o);
        }
        if f0.double_strike != f1.double_strike {
            self.toggle(rpr, &["dstrike"], f1.double_strike, o);
        }
        if f0.caps != f1.caps {
            self.toggle(rpr, &["caps"], f1.caps, o);
        }
        if f0.small_caps != f1.small_caps {
            self.toggle(rpr, &["smallCaps"], f1.small_caps, o);
        }
        if f0.hidden != f1.hidden {
            self.toggle(rpr, &["vanish"], f1.hidden, o);
        }
        if f0.outline != f1.outline {
            self.toggle(rpr, &["outline"], f1.outline, o);
        }
        if f0.shadow != f1.shadow {
            self.toggle(rpr, &["shadow"], f1.shadow, o);
        }
        if f0.emboss != f1.emboss {
            self.toggle(rpr, &["emboss"], f1.emboss, o);
        }
        if f0.no_proof != f1.no_proof {
            self.toggle(rpr, &["noProof"], f1.no_proof, o);
        }
        if f0.imprint != f1.imprint {
            self.toggle(rpr, &["imprint"], f1.imprint, o);
        }
        if f0.color != f1.color || f0.theme_color != f1.theme_color {
            match f1.color {
                Some(c) => {
                    let e = self.ensure(rpr, "color", o);
                    self.doc.set_attr(e, "w:val", &format!("{:06X}", c.0));
                    if let Some(tc) = &f1.theme_color {
                        self.doc.set_attr(e, "w:themeColor", &tc.name);
                        if let Some(tint) = &tc.tint {
                            self.doc.set_attr(e, "w:themeTint", tint);
                        } else {
                            self.doc.remove_attr(e, "w:themeTint");
                        }
                        if let Some(shade) = &tc.shade {
                            self.doc.set_attr(e, "w:themeShade", shade);
                        } else {
                            self.doc.remove_attr(e, "w:themeShade");
                        }
                    } else {
                        for a in ["w:themeColor", "w:themeTint", "w:themeShade"] {
                            self.doc.remove_attr(e, a);
                        }
                    }
                }
                None => self.remove_child(rpr, "color"),
            }
        }
        if f0.spacing != f1.spacing {
            let v = f1.spacing.map(|s| (s / 635).to_string());
            self.val(rpr, "spacing", v.as_deref(), o);
        }
        if f0.scale != f1.scale {
            let v = f1.scale.map(|s| s.to_string());
            self.val(rpr, "w", v.as_deref(), o);
        }
        if f0.fit_text != f1.fit_text {
            match f1.fit_text.flatten() {
                Some((width, id)) => {
                    let e = self.ensure(rpr, "fitText", o);
                    self.doc.set_attr(e, "w:val", &width.to_string());
                    self.doc.set_attr(e, "w:id", &id.to_string());
                }
                None => self.remove_child(rpr, "fitText"),
            }
        }
        if f0.kern != f1.kern {
            let v = f1.kern.map(|k| k.to_string());
            self.val(rpr, "kern", v.as_deref(), o);
        }
        if f0.emphasis != f1.emphasis {
            self.val(rpr, "em", f1.emphasis.map(|e| e.name()), o);
        }
        if f0.combine != f1.combine {
            use ibis_doc_model::Combine;
            match f1.combine.flatten() {
                Some(c) => {
                    let e = self.ensure(rpr, "eastAsianLayout", o);
                    self.doc.set_attr(e, "w:combine", "1");
                    match c {
                        Combine::Plain => self.doc.remove_attr(e, "w:combineBrackets"),
                        Combine::Round => self.doc.set_attr(e, "w:combineBrackets", "round"),
                        Combine::Square => self.doc.set_attr(e, "w:combineBrackets", "square"),
                        Combine::Angle => self.doc.set_attr(e, "w:combineBrackets", "angle"),
                        Combine::Curly => self.doc.set_attr(e, "w:combineBrackets", "curly"),
                    }
                }
                None => {
                    if let Some(e) = self.doc.child(rpr, "eastAsianLayout") {
                        self.doc.remove_attr(e, "w:combine");
                        self.doc.remove_attr(e, "w:combineBrackets");
                    }
                }
            }
        }
        if f0.tate_chu_yoko != f1.tate_chu_yoko {
            match f1.tate_chu_yoko {
                Some(true) => {
                    let e = self.ensure(rpr, "eastAsianLayout", o);
                    self.doc.set_attr(e, "w:vert", "1");
                }
                Some(false) => {
                    if let Some(e) = self.doc.child(rpr, "eastAsianLayout") {
                        self.doc.remove_attr(e, "w:vert");
                    }
                }
                None => {}
            }
        }
        if f0.size != f1.size || f0.cs_size != f1.cs_size {
            if f0.size != f1.size {
                let v = f1.size.map(|s| ((s + 25) / 50).to_string());
                self.val(rpr, "sz", v.as_deref(), o);
            }
            // w:szCs only when the complex scripts' size itself changed (a
            // size change sets both in the model, as Word does).
            if f0.cs_size != f1.cs_size {
                let cs_v = f1.cs_size.map(|s| ((s + 25) / 50).to_string());
                self.val(rpr, "szCs", cs_v.as_deref(), o);
            }
        }
        if f0.highlight != f1.highlight {
            match f1.highlight {
                None => {
                    self.remove_child(rpr, "highlight");
                    self.remove_child(rpr, "shd");
                }
                Some(None) => {
                    self.remove_child(rpr, "shd");
                    self.val(rpr, "highlight", Some("none"), o);
                }
                Some(Some(rgb)) => match highlight_name(rgb) {
                    Some(name) => {
                        self.remove_child(rpr, "shd");
                        self.val(rpr, "highlight", Some(name), o);
                    }
                    None => {
                        self.remove_child(rpr, "highlight");
                        let e = self.ensure(rpr, "shd", o);
                        self.doc.set_attr(e, "w:val", "clear");
                        self.doc.set_attr(e, "w:color", "auto");
                        self.doc.set_attr(e, "w:fill", &format!("{:06X}", rgb.0));
                    }
                },
            }
        }
        if f0.underline != f1.underline || f0.underline_style != f1.underline_style || f0.underline_color != f1.underline_color {
            match f1.underline {
                None => self.remove_child(rpr, "u"),
                Some(false) => self.val(rpr, "u", Some("none"), o),
                Some(true) => {
                    let e = self.ensure(rpr, "u", o);
                    let name = f1.underline_style.map_or("single", |s| s.name());
                    if self.doc.attr(e, "w:val") != Some(name) {
                        self.doc.set_attr(e, "w:val", name);
                    }
                    if f0.underline_color != f1.underline_color {
                        match f1.underline_color.flatten() {
                            Some(c) => self.doc.set_attr(e, "w:color", &format!("{:06X}", c.0)),
                            None => self.doc.remove_attr(e, "w:color"),
                        }
                    }
                }
            }
        }
        if f0.position != f1.position {
            let v = f1.position.map(|p| match p {
                VerticalPosition::Superscript => "superscript",
                VerticalPosition::Subscript => "subscript",
                VerticalPosition::Baseline => "baseline",
            });
            self.val(rpr, "vertAlign", v, o);
        }
        if f0.border != f1.border {
            self.border_property(rpr, "bdr", f1.border, o);
        }
        if f0.ligatures != f1.ligatures {
            match f1.ligatures {
                Some(lig) => {
                    let w14 = self.w14_prefix();
                    let e = match self.doc.child(rpr, "ligatures") {
                        Some(e) => e,
                        None => self.doc.append_element(rpr, &format!("{w14}:ligatures")),
                    };
                    self.doc.remove_attr(e, "val");
                    self.doc.set_attr(e, &format!("{w14}:val"), lig.to_ooxml());
                }
                None => self.remove_child(rpr, "ligatures"),
            }
        }
        if f0.num_form != f1.num_form {
            match f1.num_form {
                Some(nf) => {
                    let w14 = self.w14_prefix();
                    let e = match self.doc.child(rpr, "numForm") {
                        Some(e) => e,
                        None => self.doc.append_element(rpr, &format!("{w14}:numForm")),
                    };
                    self.doc.remove_attr(e, "val");
                    self.doc.set_attr(e, &format!("{w14}:val"), nf.to_ooxml());
                }
                None => self.remove_child(rpr, "numForm"),
            }
        }
        if f0.num_spacing != f1.num_spacing {
            match f1.num_spacing {
                Some(ns) => {
                    let w14 = self.w14_prefix();
                    let e = match self.doc.child(rpr, "numSpacing") {
                        Some(e) => e,
                        None => self.doc.append_element(rpr, &format!("{w14}:numSpacing")),
                    };
                    self.doc.remove_attr(e, "val");
                    self.doc.set_attr(e, &format!("{w14}:val"), ns.to_ooxml());
                }
                None => self.remove_child(rpr, "numSpacing"),
            }
        }
        if f0.contextual_alternates != f1.contextual_alternates {
            match f1.contextual_alternates {
                Some(true) => {
                    let w14 = self.w14_prefix();
                    let e = match self.doc.child(rpr, "cntxtAlts") {
                        Some(e) => e,
                        None => self.doc.append_element(rpr, &format!("{w14}:cntxtAlts")),
                    };
                    self.doc.remove_attr(e, "val");
                    self.doc.remove_attr(e, "w:val");
                    self.doc.remove_attr(e, &format!("{w14}:val"));
                }
                Some(false) => {
                    let w14 = self.w14_prefix();
                    let e = match self.doc.child(rpr, "cntxtAlts") {
                        Some(e) => e,
                        None => self.doc.append_element(rpr, &format!("{w14}:cntxtAlts")),
                    };
                    self.doc.remove_attr(e, "val");
                    self.doc.remove_attr(e, "w:val");
                    self.doc.set_attr(e, &format!("{w14}:val"), "0");
                }
                None => self.remove_child(rpr, "cntxtAlts"),
            }
        }
        if !self.doc.elements(rpr).any(|_| true) && self.doc.attributes(rpr).is_empty() {
            self.doc.detach(rpr);
        }
    }

    /// Write the paragraph's own formatting (`w:pPr`).
    fn paragraph_props(&mut self, p: NodeId, old: &FlowParagraph, new: &FlowParagraph) -> Result<(), Error> {
        if old.bookmarks != new.bookmarks || old.field_start != new.field_start || old.field_end != new.field_end {
            self.paragraph_marks(p, old, new)?;
        }
        if old.subdoc != new.subdoc {
            self.remove_child(p, "subDoc");
            if let Some(sub) = &new.subdoc && !sub.target.is_empty() {
                if let Some(rid) = self.link_rels.get(&format!("subdoc:{}", sub.target)) {
                    let name = self.w("subDoc");
                    let sd = self.doc.append_element(p, &name);
                    let r = match self.doc.lookup_prefix(p, "http://schemas.openxmlformats.org/officeDocument/2006/relationships") {
                        Some(prefix) => prefix,
                        None => {
                            if let Some(root) = self.doc.root_element() {
                                self.doc.set_attr(root, "xmlns:r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships");
                            }
                            "r".into()
                        }
                    };
                    self.doc.set_attr(sd, &format!("{r}:id"), rid);
                    if !sub.title.is_empty() && sub.title != sub.target {
                        self.doc.set_attr(sd, "w:title", &sub.title);
                    }
                    if sub.locked {
                        self.doc.set_attr(sd, "w:locked", "1");
                    }
                    if sub.expanded {
                        self.doc.set_attr(sd, "w:expanded", "1");
                    }
                    for (idx, line) in sub.content_paragraphs.iter().take(64).enumerate() {
                        self.doc.set_attr(sd, &format!("w:subPara{idx}"), line);
                    }
                }
            }
        }
        if old.style == new.style && old.direct == new.direct && old.mark_format == new.mark_format && old.section_break == new.section_break {
            return Ok(());
        }
        let o = PPR_ORDER;
        let ppr = match self.doc.child(p, "pPr") {
            Some(ppr) => ppr,
            None => {
                let name = self.w("pPr");
                self.doc.insert_element(p, 0, &name)
            }
        };
        if old.style != new.style {
            self.val(ppr, "pStyle", new.style.as_deref(), o);
        }
        let (a, b): (&ParaFormat, &ParaFormat) = (&old.direct, &new.direct);
        if a.shading != b.shading {
            match b.shading {
                None => self.remove_child(ppr,"shd"),
                Some(color) => {
                    let e=self.ensure(ppr,"shd",o);
                    for attr in ["w:themeFill","w:themeFillTint","w:themeFillShade","w:themeColor","w:themeTint","w:themeShade"] { self.doc.remove_attr(e,attr); }
                    self.doc.set_attr(e,"w:val",if color.is_some() {"clear"} else {"nil"});
                    self.doc.set_attr(e,"w:color","auto");
                    self.doc.set_attr(e,"w:fill",&color.map_or_else(||"auto".to_string(),|c|format!("{:06X}",c.0)));
                }
            }
        }
        if a.borders != b.borders {
            let group=self.ensure(ppr,"pBdr",o);
            const EDGES:&[&str]=&["top","left","bottom","right","between","bar"];
            for (i,name) in ["top","right","bottom","left"].into_iter().enumerate() {
                if a.borders[i]==b.borders[i] {continue;}
                match b.borders[i] {
                    None=>self.remove_child(group,name),
                    Some(border)=>{
                        let e=self.ensure(group,name,EDGES);
                        for attr in ["w:themeColor","w:themeTint","w:themeShade"] {self.doc.remove_attr(e,attr);}
                        if let Some(border)=border {
                            self.doc.set_attr(e,"w:val",paragraph_border_name(border.style));
                            self.doc.set_attr(e,"w:sz",&((border.width*8+6350)/12700).to_string());
                            self.doc.set_attr(e,"w:color",&format!("{:06X}",border.color.0));
                            self.doc.set_attr(e,"w:space",&((border.space+6350)/12700).to_string());
                        } else {
                            self.doc.set_attr(e,"w:val","nil");
                            for attr in ["w:sz","w:color","w:space"] {self.doc.remove_attr(e,attr);}
                        }
                    }
                }
            }
            if !self.doc.elements(group).any(|_|true) && self.doc.attributes(group).is_empty() {self.doc.detach(group);}
        }
        if a.tabs != b.tabs {
            // The tab stops written again from the new list.
            self.remove_child(ppr, "tabs");
            if !b.tabs.is_empty() {
                let tabs = self.ensure(ppr, "tabs", o);
                let name = self.w("tab");
                for change in &b.tabs {
                    let e = self.doc.append_element(tabs, &name);
                    let (val, leader) = match change.stop {
                        None => ("clear", false),
                        Some(t) => (
                            match t.align {
                                ibis_doc_model::TabAlign::Left => "left",
                                ibis_doc_model::TabAlign::Center => "center",
                                ibis_doc_model::TabAlign::Right => "right",
                                ibis_doc_model::TabAlign::Decimal => "decimal",
                            },
                            t.leader,
                        ),
                    };
                    self.doc.set_attr(e, "w:val", val);
                    if leader {
                        self.doc.set_attr(e, "w:leader", "dot");
                    }
                    self.doc.set_attr(e, "w:pos", &(change.position / 635).to_string());
                }
            }
        }
        if a.num_id != b.num_id || a.level != b.level {
            if b.num_id.is_none() && b.level.is_none() {
                self.remove_child(ppr, "numPr");
            } else {
                let num = self.ensure(ppr, "numPr", o);
                let order = &["ilvl", "numId", "numberingChange", "ins"];
                let level = b.level.map(|l| l.to_string());
                self.val(num, "ilvl", level.as_deref(), order);
                let id = b.num_id.map(|n| n.to_string());
                self.val(num, "numId", id.as_deref(), order);
            }
        }
        if a.align != b.align {
            let v = b.align.map(|al| match al {
                FlowAlign::Left => "left",
                FlowAlign::Center => "center",
                FlowAlign::Right => "right",
                FlowAlign::Justify => "both",
                FlowAlign::Distribute => "distribute",
            });
            self.val(ppr, "jc", v, o);
        }
        if a.text_alignment != b.text_alignment {
            let v = b.text_alignment.map(|ta| match ta {
                ibis_doc_model::FontAlign::Top => "top",
                ibis_doc_model::FontAlign::Center => "center",
                ibis_doc_model::FontAlign::Bottom => "bottom",
                ibis_doc_model::FontAlign::Baseline => "baseline",
                ibis_doc_model::FontAlign::Auto => "auto",
            });
            self.val(ppr, "textAlignment", v, o);
        }
        for (name, x, y) in [
            ("keepNext", a.keep_next, b.keep_next),
            ("keepLines", a.keep_lines, b.keep_lines),
            ("pageBreakBefore", a.page_break_before, b.page_break_before),
            ("snapToGrid", a.snap, b.snap),
            ("kinsoku", a.kinsoku, b.kinsoku),
            ("wordWrap", a.word_wrap, b.word_wrap),
            ("overflowPunct", a.overflow_punct, b.overflow_punct),
            ("autoSpaceDE", a.auto_space_letters, b.auto_space_letters),
            ("autoSpaceDN", a.auto_space_digits, b.auto_space_digits),
            ("bidi", a.bidi, b.bidi),
            ("contextualSpacing", a.contextual_spacing, b.contextual_spacing),
            ("suppressLineNumbers", a.suppress_line_numbers, b.suppress_line_numbers),
            ("suppressAutoHyphens", a.suppress_auto_hyphens, b.suppress_auto_hyphens),
            ("widowControl", a.widow_control, b.widow_control),
        ] {
            if x != y {
                self.toggle(ppr, &[name], y, o);
            }
        }
        if a.collapsed != b.collapsed {
            let w15 = self.w15_prefix();
            match b.collapsed {
                Some(true) => {
                    let e = match self.doc.child(ppr, "collapsed") {
                        Some(e) => e,
                        None => self.doc.append_element(ppr, &format!("{w15}:collapsed")),
                    };
                    self.doc.remove_attr(e, "w:val");
                    self.doc.remove_attr(e, &format!("{w15}:val"));
                }
                _ => {
                    while let Some(e) = self.doc.child(ppr, "collapsed") {
                        self.doc.detach(e);
                    }
                }
            }
        }
        if a.drop_cap != b.drop_cap {
            match b.drop_cap {
                Some(Some(dc)) if dc.location != DropCapLocation::None => {
                    let frame = self.ensure(ppr, "framePr", o);
                    let val = match dc.location {
                        DropCapLocation::Drop => "drop",
                        DropCapLocation::Margin => "margin",
                        DropCapLocation::None => "none",
                    };
                    self.doc.set_attr(frame, "w:dropCap", val);
                    self.doc.set_attr(frame, "w:lines", &dc.lines.to_string());
                    self.doc.set_attr(frame, "w:wrap", "around");
                    self.doc.set_attr(frame, "w:vAnchor", "text");
                    self.doc.set_attr(frame, "w:hAnchor", if dc.location == DropCapLocation::Margin { "margin" } else { "text" });
                    if dc.space > 0 {
                        self.doc.set_attr(frame, "w:hSpace", &(dc.space / 635).to_string());
                    } else {
                        self.doc.remove_attr(frame, "w:hSpace");
                    }
                }
                _ => {
                    self.remove_child(ppr, "framePr");
                }
            }
        }
        if a.before != b.before || a.after != b.after || a.line != b.line {
            let sp = self.ensure(ppr, "spacing", o);
            if a.before != b.before {
                self.doc.remove_attr(sp, "w:beforeLines");
                self.doc.remove_attr(sp, "w:beforeAutospacing");
                match b.before {
                    Some(v) => self.doc.set_attr(sp, "w:before", &(v / 635).to_string()),
                    None => self.doc.remove_attr(sp, "w:before"),
                }
            }
            if a.after != b.after {
                self.doc.remove_attr(sp, "w:afterLines");
                self.doc.remove_attr(sp, "w:afterAutospacing");
                match b.after {
                    Some(v) => self.doc.set_attr(sp, "w:after", &(v / 635).to_string()),
                    None => self.doc.remove_attr(sp, "w:after"),
                }
            }
            if a.line != b.line {
                match b.line {
                    Some(LineHeight::Auto(m)) => {
                        self.doc.set_attr(sp, "w:line", &m.to_string());
                        self.doc.set_attr(sp, "w:lineRule", "auto");
                    }
                    Some(LineHeight::Exact(v)) => {
                        self.doc.set_attr(sp, "w:line", &(v / 635).to_string());
                        self.doc.set_attr(sp, "w:lineRule", "exact");
                    }
                    Some(LineHeight::AtLeast(v)) => {
                        self.doc.set_attr(sp, "w:line", &(v / 635).to_string());
                        self.doc.set_attr(sp, "w:lineRule", "atLeast");
                    }
                    None => {
                        self.doc.remove_attr(sp, "w:line");
                        self.doc.remove_attr(sp, "w:lineRule");
                    }
                }
            }
            if self.doc.attributes(sp).is_empty() {
                self.doc.detach(sp);
            }
        }
        if a.left != b.left || a.right != b.right || a.first_line != b.first_line
            || a.left_chars != b.left_chars || a.right_chars != b.right_chars || a.first_line_chars != b.first_line_chars {
            let ind = self.ensure(ppr, "ind", o);
            if a.left != b.left || a.left_chars != b.left_chars {
                for x in ["w:start", "w:leftChars", "w:startChars"] {
                    self.doc.remove_attr(ind, x);
                }
                match b.left {
                    Some(v) => self.doc.set_attr(ind, "w:left", &(v / 635).to_string()),
                    None => self.doc.remove_attr(ind, "w:left"),
                }
                if let Some(chars) = b.left_chars {
                    self.doc.set_attr(ind, "w:leftChars", &chars.to_string());
                }
            }
            if a.right != b.right || a.right_chars != b.right_chars {
                for x in ["w:end", "w:rightChars", "w:endChars"] {
                    self.doc.remove_attr(ind, x);
                }
                match b.right {
                    Some(v) => self.doc.set_attr(ind, "w:right", &(v / 635).to_string()),
                    None => self.doc.remove_attr(ind, "w:right"),
                }
                if let Some(chars) = b.right_chars {
                    self.doc.set_attr(ind, "w:rightChars", &chars.to_string());
                }
            }
            if a.first_line != b.first_line || a.first_line_chars != b.first_line_chars {
                for x in ["w:firstLine", "w:hanging", "w:firstLineChars", "w:hangingChars"] {
                    self.doc.remove_attr(ind, x);
                }
                match b.first_line {
                    Some(v) if v < 0 => self.doc.set_attr(ind, "w:hanging", &(-v / 635).to_string()),
                    Some(v) if v > 0 => self.doc.set_attr(ind, "w:firstLine", &(v / 635).to_string()),
                    _ => {}
                }
                match b.first_line_chars {
                    Some(v) if v < 0 => self.doc.set_attr(ind, "w:hangingChars", &(-v).to_string()),
                    Some(v) if v > 0 => self.doc.set_attr(ind, "w:firstLineChars", &v.to_string()),
                    _ => {}
                }
            }
            if self.doc.attributes(ind).is_empty() {
                self.doc.detach(ind);
            }
        }
        if old.mark_format != new.mark_format {
            self.run_props(ppr, &old.mark_format, &None, &new.mark_format, &None, true);
        }
        if old.section_break != new.section_break {
            use ibis_doc_model::SectionBreak;
            match new.section_break {
                SectionBreak::None => self.remove_child(ppr, "sectPr"),
                kind => {
                    // A copy of the document's last section (its page, its
                    // headers and footers), with the break's kind.
                    let sect = match self.doc.child(ppr, "sectPr") {
                        Some(s) => s,
                        None => {
                            let body = self.pristine.by_path(&self.map.body_path).and_then(|b| self.pristine.child(b, "sectPr"));
                            let s = match body {
                                Some(b) if self.part == self.map.body_part => self.doc.import(self.pristine, b),
                                _ => {
                                    let name = self.w("sectPr");
                                    let s = self.doc.append_element(ppr, &name);
                                    self.doc.detach(s);
                                    s
                                }
                            };
                            for attr in ["w:rsidR", "w:rsidSect", "w:rsidRPr", "w:rsidDel"] {
                                self.doc.remove_attr(s, attr);
                            }
                            let rank = PPR_ORDER.iter().position(|n| *n == "sectPr").unwrap();
                            let before = self.doc.elements(ppr).find(|&c| PPR_ORDER.iter().position(|n| *n == self.doc.local_name(c)).is_some_and(|r| r > rank));
                            match before {
                                Some(b) => self.doc.insert_before(b, s),
                                None => {
                                    let end = self.doc.children(ppr).len();
                                    self.doc.insert(ppr, end, s);
                                }
                            }
                            s
                        }
                    };
                    // Its kind is written into the next section's w:type
                    // (how that one starts), with the sections.
                    let _ = (kind, sect);
                }
            }
        }
        if !self.doc.elements(ppr).any(|_| true) && self.doc.attributes(ppr).is_empty() {
            self.doc.detach(ppr);
        }
        Ok(())
    }
}

/// The name of one of the 16 highlight colours (`ST_HighlightColor`).
fn highlight_name(rgb: Rgb) -> Option<&'static str> {
    Some(match rgb.0 {
        0xffff00 => "yellow",
        0x00ff00 => "green",
        0x00ffff => "cyan",
        0xff00ff => "magenta",
        0x0000ff => "blue",
        0xff0000 => "red",
        0x00008b | 0x000080 => "darkBlue",
        0x008b8b | 0x008080 => "darkCyan",
        0x006400 | 0x008000 => "darkGreen",
        0x8b008b | 0x800080 => "darkMagenta",
        0x8b0000 | 0x800000 => "darkRed",
        0x808000 => "darkYellow",
        0xa9a9a9 | 0x808080 => "darkGray",
        0xd3d3d3 | 0xc0c0c0 => "lightGray",
        0x000000 => "black",
        0xffffff => "white",
        _ => return None,
    })
}

const NUMBERING_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
const NUMBERING_TYPE: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";
/// Attribute prefixes are aliases; unqualified attributes have no Word namespace.
fn word_attr_name<'a>(doc: &'a XmlDocument, node: NodeId, local: &str) -> Option<&'a str> {
    namespace_attr_name(doc, node, W_NS, local)
}

fn namespace_attr_name<'a>(doc: &'a XmlDocument, node: NodeId, namespace: &str, local: &str) -> Option<&'a str> {
    doc.attributes(node).iter().find_map(|a| {
        let (prefix, name) = a.name.split_once(':')?;
        (name == local && doc.resolve_prefix(node, Some(prefix)) == Some(namespace)).then_some(a.name.as_str())
    })
}

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// Lists added to the document, written into `numbering.xml` (made if the
/// document has none). Lists changed or removed are not saved yet.
const STYLES_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";

/// The order of a `w:style`'s children (`CT_Style`).
const STYLE_ORDER: &[&str] = &["name", "aliases", "basedOn", "next", "link", "autoRedefine", "hidden", "uiPriority", "semiHidden", "unhideWhenUsed", "qFormat", "locked", "personal", "personalCompose", "personalReply", "rsid", "pPr", "rPr", "tblPr", "trPr", "tcPr", "tblStylePr"];

/// Write the styles that changed (their paragraph and run properties) and
/// the new ones (after the last style) to `styles.xml`.
fn write_styles(package: &mut Package, map: &SourceMap, old: &ibis_doc_model::FlowStyles, new: &ibis_doc_model::FlowStyles) -> Result<(), Error> {
    use ibis_doc_model::StyleKind;
    if old.styles.keys().any(|id| !new.styles.contains_key(id)) {
        return Err(unsupported("スタイルの削除"));
    }
    let main = map.body_part.clone();
    let part = package.relationships(&main)?.into_iter().find(|r| r.kind == STYLES_REL && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| unsupported("styles.xml の無い文書のスタイルの変更"))?;
    let pristine = package.xml(&part)?;
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の styles.xml"))?;
    let none_blocks = HashMap::new();
    let none_pictures = HashMap::new();
    let none_added = HashMap::new();
    let none_svg = HashSet::new();
    let none_links = HashMap::new();
    let mut writer = Writer { doc, pristine: &pristine, map, part: &part, old_blocks: &none_blocks, new_blocks: &none_blocks, pictures: &none_pictures, nodes: HashMap::new(), added: &none_added, svg_media: &none_svg, empty_parent: None, link_rels: &none_links, written: HashMap::new(), shape_nodes: HashMap::new(), shape_olds: HashMap::new(), old_shape_groups: &[], new_shape_groups: &[] };
    let element_of = |doc: &XmlDocument, id: &str| doc.children_named(root, "style").find(|&e| doc.attr(e, "w:styleId") == Some(id));
    for (id, style) in &new.styles {
        let was = old.styles.get(id);
        if was == Some(style) {
            continue;
        }
        let node = match was {
            Some(was) => {
                if was.kind != style.kind || was.name != style.name {
                    return Err(unsupported("スタイルの種類・名前の変更"));
                }
                let e = element_of(writer.doc, id).ok_or_else(|| unsupported("元の場所が分からないスタイルの変更"))?;
                // 基準のスタイル・次の段落のスタイル (fix).
                if was.based_on != style.based_on {
                    writer.val(e, "basedOn", style.based_on.as_deref(), STYLE_ORDER);
                }
                if was.next != style.next {
                    writer.val(e, "next", style.next.as_deref(), STYLE_ORDER);
                }
                if was.auto_update != style.auto_update {
                    writer.toggle(e, &["autoRedefine"], style.auto_update.then_some(true), STYLE_ORDER);
                }
                e
            }
            None => {
                let e = writer.doc.append_element(root, "w:style");
                writer.doc.detach(e);
                match writer.doc.children_named(root, "style").last() {
                    Some(last) => writer.doc.insert_after(last, e),
                    None => {
                        let end = writer.doc.children(root).len();
                        writer.doc.insert(root, end, e);
                    }
                }
                let kind = match style.kind {
                    StyleKind::Paragraph => "paragraph",
                    StyleKind::Character => "character",
                    StyleKind::Table => "table",
                    StyleKind::Numbering => "numbering",
                };
                writer.doc.set_attr(e, "w:type", kind);
                writer.doc.set_attr(e, "w:styleId", id);
                writer.val(e, "name", Some(&style.name), STYLE_ORDER);
                writer.val(e, "basedOn", style.based_on.as_deref(), STYLE_ORDER);
                writer.val(e, "next", style.next.as_deref(), STYLE_ORDER);
                if style.auto_update {
                    writer.toggle(e, &["autoRedefine"], Some(true), STYLE_ORDER);
                }
                if let Some(priority) = style.ui_priority {
                    writer.val(e, "uiPriority", Some(&priority.to_string()), STYLE_ORDER);
                }
                if style.name.starts_with("toc ") || style.name == "caption" {
                    writer.toggle(e, &["unhideWhenUsed"], Some(true), STYLE_ORDER);
                }
                if style.quick {
                    writer.toggle(e, &["qFormat"], Some(true), STYLE_ORDER);
                }
                e
            }
        };
        let empty = ibis_doc_model::FlowStyle::default();
        let was = was.unwrap_or(&empty);
        if let Some(num_id) = style.num_id {
            let ppr = writer.ensure(node, "pPr", STYLE_ORDER);
            let numpr = writer.ensure(ppr, "numPr", &["pStyle", "keepNext", "keepLines", "pageBreakBefore", "framePr", "widowControl", "numPr"]);
            let numid_el = writer.ensure(numpr, "numId", &["ilvl", "numId"]);
            writer.doc.set_attr(numid_el, "w:val", &num_id.to_string());
        }
        if was.para != style.para {
            writer.ensure(node, "pPr", STYLE_ORDER);
            let a = FlowParagraph { direct: was.para.clone(), ..FlowParagraph::default() };
            let b = FlowParagraph { direct: style.para.clone(), ..FlowParagraph::default() };
            writer.paragraph_props(node, &a, &b)?;
        }
        if was.run != style.run {
            let rpr = writer.ensure(node, "rPr", STYLE_ORDER);
            writer.run_props(node, &was.run, &None, &style.run, &None, false);
            if !writer.doc.elements(rpr).any(|_| true) {
                writer.doc.detach(rpr);
            }
        }
    }
    Ok(())
}

fn write_numbering(package: &mut Package, map: &SourceMap, old: &ibis_doc_model::Numbering, new: &ibis_doc_model::Numbering, current: &Presentation) -> Result<(), Error> {
    // A list definition changed: only its levels' number format, number
    // text and what follows the number (番号の書式・番号の後) are written
    // into the existing definition; anything else is not saved yet.
    let only_formats = |a: &ibis_doc_model::AbstractList, b: &ibis_doc_model::AbstractList| {
        a.levels.len() == b.levels.len()
            && a.levels.iter().zip(&b.levels).all(|(x, y)| ibis_doc_model::ListLevel { format: y.format.clone(), text: y.text.clone(), suffix: y.suffix, run: y.run.clone(), ..x.clone() } == *y)
            && (a.num_style_link == b.num_style_link || a.num_style_link.is_none() || b.num_style_link.is_none())
    };
    let mut patched: Vec<(u32, &ibis_doc_model::AbstractList, &ibis_doc_model::AbstractList)> = Vec::new();
    for (id, a) in &old.abstracts {
        match new.abstracts.get(id) {
            Some(b) if b == a => {}
            Some(b) if only_formats(a, b) => patched.push((*id, a, b)),
            _ => return Err(unsupported("リストの定義の変更")),
        }
    }
    if old.nums.iter().any(|(id, n)| new.nums.get(id) != Some(n)) {
        return Err(unsupported("リストの定義の変更"));
    }
    let main = map.body_part.clone();
    let existing = package.relationships(&main)?.into_iter().find(|r| r.kind == NUMBERING_REL && !r.external).and_then(|r| r.target_part(&main));
    let part = match existing {
        Some(part) => part,
        None => {
            let dir = main.rsplit_once('/').map_or("", |(d, _)| d);
            let part = if dir.is_empty() { "numbering.xml".to_string() } else { format!("{dir}/numbering.xml") };
            let mut doc = XmlDocument::new_with_declaration();
            let root = doc.append_element(XmlDocument::ROOT, "w:numbering");
            doc.set_attr(root, "xmlns:w", W_NS);
            package.set_xml(&part, doc);
            package.set_override(&part, NUMBERING_TYPE)?;
            package.add_relationship(&main, NUMBERING_REL, "numbering.xml", false)?;
            part
        }
    };
    // The pictures of new picture bullets: their media parts, related from the numbering part.
    let mut new_pictures: Vec<(u32, String, Option<ibis_doc_model::EmuSize>)> = Vec::new();
    for (id, (media_id, size)) in new.pic_bullets.iter().filter(|(id, _)| !old.pic_bullets.contains_key(id)) {
        let media = current.media(*media_id).ok_or_else(|| unsupported("図の箇条書きの画像"))?;
        let rid = add_media(package, &part, media)?;
        new_pictures.push((*id, rid, *size));
    }
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空の numbering.xml への追加"))?;
    let w = |local: &str| format!("w:{local}");
    // The picture bullets come before the list definitions.
    for (id, rid, size) in new_pictures {
        let bullet = doc.append_element(root, &w("numPicBullet"));
        doc.detach(bullet);
        let first = doc.children_named(root, "abstractNum").next();
        let anchor = doc.children_named(root, "numPicBullet").last();
        match (anchor, first) {
            (Some(l), _) => doc.insert_after(l, bullet),
            (None, Some(f)) => doc.insert_before(f, bullet),
            (None, None) => {
                let end = doc.children(root).len();
                doc.insert(root, end, bullet);
            }
        }
        doc.set_attr(bullet, "w:numPicBulletId", &id.to_string());
        let pict = doc.append_element(bullet, &w("pict"));
        let shape = doc.append_element(pict, "v:shape");
        doc.set_attr(shape, "xmlns:v", "urn:schemas-microsoft-com:vml");
        doc.set_attr(shape, "xmlns:o", "urn:schemas-microsoft-com:office:office");
        doc.set_attr(shape, "xmlns:r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships");
        doc.set_attr(shape, "id", &format!("_x0000_i{}", 1025 + id));
        doc.set_attr(shape, "type", "#_x0000_t75");
        doc.set_attr(shape, "o:bullet", "t");
        let (cx, cy) = size.map_or((9.0, 9.0), |s| (s.width as f64 / 12_700.0, s.height as f64 / 12_700.0));
        doc.set_attr(shape, "style", &format!("width:{cx}pt;height:{cy}pt"));
        let image = doc.append_element(shape, "v:imagedata");
        doc.set_attr(image, "r:id", &rid);
        doc.set_attr(image, "o:title", "");
    }
    for (id, a, b) in patched {
        let id_text = id.to_string();
        let Some(node) = doc.children_named(root, "abstractNum").find(|&n| doc.attr(n, "w:abstractNumId") == Some(id_text.as_str())) else {
            return Err(unsupported("リストの定義の変更"));
        };
        if a.num_style_link != b.num_style_link {
            let found = doc.children_named(node, "numStyleLink").next();
            match (&b.num_style_link, found) {
                (Some(link), Some(e)) => doc.set_attr(e, "w:val", link),
                (Some(link), None) => {
                    let first_lvl = doc.children_named(node, "lvl").next();
                    let e = doc.append_element(node, &w("numStyleLink"));
                    doc.detach(e);
                    doc.set_attr(e, "w:val", link);
                    match first_lvl {
                        Some(first) => doc.insert_before(first, e),
                        None => {
                            let end = doc.children(node).len();
                            doc.insert(node, end, e);
                        }
                    }
                }
                (None, Some(e)) => doc.detach(e),
                (None, None) => {}
            }
        }
        for (l, (x, y)) in a.levels.iter().zip(&b.levels).enumerate() {
            if x == y {
                continue;
            }
            let l_text = l.to_string();
            let Some(lvl) = doc.children_named(node, "lvl").find(|&n| doc.attr(n, "w:ilvl") == Some(l_text.as_str())) else {
                return Err(unsupported("リストの定義の変更"));
            };
            // CT_Lvl: start, numFmt, lvlRestart, pStyle, isLgl, suff, lvlText, …
            let set = |doc: &mut XmlDocument, name: &str, value: Option<&str>, after: &[&str]| {
                let found = doc.children_named(lvl, name).next();
                match (found, value) {
                    (Some(e), Some(v)) => doc.set_attr(e, "w:val", v),
                    (Some(e), None) => doc.detach(e),
                    (None, Some(v)) => {
                        let e = doc.append_element(lvl, &format!("w:{name}"));
                        doc.detach(e);
                        doc.set_attr(e, "w:val", v);
                        match doc.elements(lvl).filter(|&c| after.contains(&doc.local_name(c))).last() {
                            Some(prev) => doc.insert_after(prev, e),
                            None => doc.insert(lvl, 0, e),
                        }
                    }
                    (None, None) => {}
                }
            };
            if x.format != y.format {
                set(doc, "numFmt", Some(&y.format), &["start"]);
            }
            if x.suffix != y.suffix {
                let v = match y.suffix {
                    ibis_doc_model::ListSuffix::Tab => None,
                    ibis_doc_model::ListSuffix::Space => Some("space"),
                    ibis_doc_model::ListSuffix::Nothing => Some("nothing"),
                };
                set(doc, "suff", v, &["start", "numFmt", "lvlRestart", "pStyle", "isLgl"]);
            }
            if x.text != y.text {
                set(doc, "lvlText", Some(&y.text), &["start", "numFmt", "lvlRestart", "pStyle", "isLgl", "suff"]);
            }
            if x.run.font != y.run.font {
                let existing_rpr = doc.children_named(lvl, "rPr").next();
                match (&y.run.font, existing_rpr) {
                    (Some(f), Some(rpr)) => {
                        let existing_rf = doc.children_named(rpr, "rFonts").next();
                        let rfonts = match existing_rf {
                            Some(rf) => rf,
                            None => doc.append_element(rpr, &w("rFonts")),
                        };
                        doc.set_attr(rfonts, "w:ascii", f);
                        doc.set_attr(rfonts, "w:hAnsi", f);
                        doc.set_attr(rfonts, "w:hint", "default");
                    }
                    (Some(f), None) => {
                        let rpr = doc.append_element(lvl, &w("rPr"));
                        let rfonts = doc.append_element(rpr, &w("rFonts"));
                        doc.set_attr(rfonts, "w:ascii", f);
                        doc.set_attr(rfonts, "w:hAnsi", f);
                        doc.set_attr(rfonts, "w:hint", "default");
                    }
                    (None, Some(rpr)) => {
                        let existing_rf = doc.children_named(rpr, "rFonts").next();
                        if let Some(rf) = existing_rf {
                            doc.detach(rf);
                        }
                    }
                    (None, None) => {}
                }
            }
        }
    }
    for (id, list) in new.abstracts.iter().filter(|(id, _)| !old.abstracts.contains_key(id)) {
        // After the last list definition (they come before the lists).
        let last = doc.children_named(root, "abstractNum").last();
        let first_num = doc.children_named(root, "num").next();
        let a = doc.append_element(root, &w("abstractNum"));
        doc.detach(a);
        match (last, first_num) {
            (Some(l), _) => doc.insert_after(l, a),
            (None, Some(n)) => doc.insert_before(n, a),
            (None, None) => {
                let end = doc.children(root).len();
                doc.insert(root, end, a);
            }
        }
        doc.set_attr(a, "w:abstractNumId", &id.to_string());
        let t = doc.append_element(a, &w("multiLevelType"));
        doc.set_attr(t, "w:val", "hybridMultilevel");
        if let Some(link) = &list.num_style_link {
            let nsl = doc.append_element(a, &w("numStyleLink"));
            doc.set_attr(nsl, "w:val", link);
        }
        for (l, level) in list.levels.iter().enumerate() {
            let lvl = doc.append_element(a, &w("lvl"));
            doc.set_attr(lvl, "w:ilvl", &l.to_string());
            for (name, value) in [("start", level.start.to_string()), ("numFmt", level.format.clone())] {
                let e = doc.append_element(lvl, &w(name));
                doc.set_attr(e, "w:val", &value);
            }
            if level.suffix != ibis_doc_model::ListSuffix::Tab {
                let e = doc.append_element(lvl, &w("suff"));
                doc.set_attr(e, "w:val", if level.suffix == ibis_doc_model::ListSuffix::Space { "space" } else { "nothing" });
            }
            let text = doc.append_element(lvl, &w("lvlText"));
            doc.set_attr(text, "w:val", &level.text);
            if let Some(picture) = level.pic_bullet {
                let e = doc.append_element(lvl, &w("lvlPicBulletId"));
                doc.set_attr(e, "w:val", &picture.to_string());
            }
            let jc = doc.append_element(lvl, &w("lvlJc"));
            doc.set_attr(jc, "w:val", "left");
            let ppr = doc.append_element(lvl, &w("pPr"));
            let ind = doc.append_element(ppr, &w("ind"));
            if let Some(left) = level.para.left {
                doc.set_attr(ind, "w:left", &(left / 635).to_string());
            }
            match level.para.first_line {
                Some(v) if v < 0 => doc.set_attr(ind, "w:hanging", &(-v / 635).to_string()),
                Some(v) => doc.set_attr(ind, "w:firstLine", &(v / 635).to_string()),
                None => {}
            }
            if let Some(font) = &level.run.font {
                let rpr = doc.append_element(lvl, &w("rPr"));
                let rfonts = doc.append_element(rpr, &w("rFonts"));
                doc.set_attr(rfonts, "w:ascii", font);
                doc.set_attr(rfonts, "w:hAnsi", font);
                doc.set_attr(rfonts, "w:hint", "default");
            }
        }
    }
    for (id, num) in new.nums.iter().filter(|(id, _)| !old.nums.contains_key(id)) {
        let last = doc.children_named(root, "num").last();
        let n = doc.append_element(root, &w("num"));
        doc.detach(n);
        match last.or_else(|| doc.children_named(root, "abstractNum").last()) {
            Some(l) => doc.insert_after(l, n),
            None => {
                let end = doc.children(root).len();
                doc.insert(root, end, n);
            }
        }
        doc.set_attr(n, "w:numId", &id.to_string());
        let a = doc.append_element(n, &w("abstractNumId"));
        doc.set_attr(a, "w:val", &num.abstract_id.to_string());
        // 番号の振り直し・開始番号: where the list starts again.
        for (level, start) in &num.start_overrides {
            let o = doc.append_element(n, &w("lvlOverride"));
            doc.set_attr(o, "w:ilvl", &level.to_string());
            let v = doc.append_element(o, &w("startOverride"));
            doc.set_attr(v, "w:val", &start.to_string());
        }
    }
    Ok(())
}

thread_local! {
    /// The relationships of the new charts' parts (by shape id), for the part being written.
    static NEW_CHART_RELS: std::cell::RefCell<HashMap<u64, String>> = std::cell::RefCell::new(HashMap::new());
}

/// The charts (shape ids and charts) in `blocks` (tables' too).
fn charts_in<'a>(blocks: &'a [Block], out: &mut Vec<(ibis_doc_model::ShapeId, &'a ibis_doc_model::Chart)>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                for r in &p.runs {
                    if let FlowRunKind::Shape(shape) = &r.kind && let ShapeKind::Chart(chart) = &shape.kind {
                        out.push((shape.id, chart));
                    }
                }
            }
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| charts_in(&c.blocks, out)),
        }
    }
}

/// The chart part the file's chart `id` (in `old`, the blocks as read) shows: found from the `c:chart` of its drawing.
fn existing_chart_part(package: &Package, map: &SourceMap, old: &[Block], id: ibis_doc_model::ShapeId) -> Result<String, Error> {
    fn find(blocks: &[Block], id: ibis_doc_model::ShapeId) -> Option<(u32, usize)> {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    if let Some(i) = atoms(p).iter().position(|a| matches!(&a.0, AtomKey::Shape(s) if s.id == id && matches!(s.kind, ShapeKind::Chart(_)))) {
                        return Some((p.id, i));
                    }
                }
                Block::Table(t) => {
                    if let Some(found) = t.rows.iter().flat_map(|r| &r.cells).find_map(|c| find(&c.blocks, id)) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
    let fail = || unsupported("このグラフの変更");
    let (para, index) = find(old, id).ok_or_else(fail)?;
    let source = map.paragraphs.get(&para).ok_or_else(fail)?;
    let AtomSource::InRun { node, .. } = source.atoms.get(index).ok_or_else(fail)? else { return Err(fail()) };
    let doc = package.xml(&source.part)?;
    let mut at = doc.by_path(&source.path).ok_or_else(fail)?;
    for &i in node {
        at = doc.elements(at).nth(i as usize).ok_or_else(fail)?;
    }
    let reference = doc.descendants(at).into_iter().find(|&n| doc.is_element(n) && doc.local_name(n) == "chart").ok_or_else(fail)?;
    let rid = doc.attributes(reference).iter().find(|a| a.name.ends_with(":id") && !a.name.starts_with("xmlns")).map(|a| a.value.clone()).ok_or_else(fail)?;
    package.relationships(&source.part)?.into_iter().find(|r| r.id == rid).and_then(|r| r.target_part(&source.part)).ok_or_else(fail)
}

/// The media of the pictures in `blocks` (tables' too).
fn pictures_in(blocks: &[Block], out: &mut HashSet<MediaId>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                for r in &p.runs {
                    match &r.kind {
                        FlowRunKind::Picture { media, .. } | FlowRunKind::PictureControl { media, .. } => {
                            out.insert(*media);
                        }
                        FlowRunKind::Shape(shape) => {
                            if let ShapeKind::Picture(pic) = &shape.kind {
                                out.insert(pic.media);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        pictures_in(&cell.blocks, out);
                    }
                }
            }
        }
    }
}

const IMAGE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

/// A new media part (`word/media/imageN.ext`) with `media`'s bytes, related
/// from `part`; its relationship's id.
fn add_media(package: &mut Package, part: &str, media: &ibis_doc_model::Media) -> Result<String, Error> {
    use ibis_doc_model::MediaFormat::*;
    let (ext, content_type) = match media.format {
        Png => ("png", "image/png"),
        Jpeg => ("jpeg", "image/jpeg"),
        Gif => ("gif", "image/gif"),
        Bmp => ("bmp", "image/bmp"),
        Tiff => ("tiff", "image/tiff"),
        Emf => ("emf", "image/x-emf"),
        Wmf => ("wmf", "image/x-wmf"),
        Svg => ("svg", "image/svg+xml"),
        _ => return Err(unsupported("この形式の画像の挿入")),
    };
    let dir = part.rsplit_once('/').map_or("", |(d, _)| d);
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    let n = (1..).find(|n| !package.contains(&format!("{prefix}media/image{n}.{ext}"))).expect("a free name");
    let name = format!("{prefix}media/image{n}.{ext}");
    package.set_bytes(&name, media.data.clone());
    package.set_override(&name, content_type)?;
    Ok(package.add_relationship(part, IMAGE_REL, &format!("media/image{n}.{ext}"), false)?)
}

const HYPERLINK_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
const SUBDOC_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/subDocument";

/// The targets of the subdocuments (`w:subDoc`) in `blocks`.
fn subdocs_in(blocks: &[Block], out: &mut HashSet<String>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                if let Some(sub) = &p.subdoc && !sub.target.is_empty() {
                    out.insert(sub.target.clone());
                }
            }
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        subdocs_in(&cell.blocks, out);
                    }
                }
            }
        }
    }
}

/// The addresses of the links in `blocks` (tables' too).
fn links_in(blocks: &[Block], out: &mut HashSet<String>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                for r in &p.runs {
                    if let Some(Link::Url(u)) = &r.link {
                        out.insert(u.clone());
                    }
                }
            }
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        links_in(&cell.blocks, out);
                    }
                }
            }
        }
    }
}

fn paragraph_border_name(style: ibis_doc_model::BorderStyle) -> &'static str {
    use ibis_doc_model::BorderStyle::*;
    match style {
        Single=>"single",Thick=>"thick",Double=>"double",Triple=>"triple",Dotted=>"dotted",Dashed=>"dashed",
        DotDash=>"dotDash",DotDotDash=>"dotDotDash",DashSmallGap=>"dashSmallGap",DashDotStroked=>"dashDotStroked",
        ThinThickSmallGap=>"thinThickSmallGap",ThickThinSmallGap=>"thickThinSmallGap",ThinThickThinSmallGap=>"thinThickThinSmallGap",
        ThinThickMediumGap=>"thinThickMediumGap",ThickThinMediumGap=>"thickThinMediumGap",ThinThickThinMediumGap=>"thinThickThinMediumGap",
        ThinThickLargeGap=>"thinThickLargeGap",ThickThinLargeGap=>"thickThinLargeGap",ThinThickThinLargeGap=>"thinThickThinLargeGap",
        Wave=>"wave",DoubleWave=>"doubleWave",ThreeDEmboss=>"threeDEmboss",ThreeDEngrave=>"threeDEngrave",Outset=>"outset",Inset=>"inset",
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The watermark in header `part`: Word's own (a run of a `v:shape`
/// `PowerPlusWaterMarkObject…`) taken out, `watermark` (if any) put at the
/// end of the header's first paragraph, as Word writes one (a WordArt
/// text path, centred on the margins, behind the text). Its type is Word's
/// own `#_x0000_t136`, which Word draws by its built-in definition (no
/// `v:shapetype` written; the test owner checked in Word).
fn write_watermark(package: &mut Package, part: &str, watermark: Option<&ibis_doc_model::Watermark>, area: (f32, f32), index: usize) -> Result<(), Error> {
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("空のヘッダーのパーツ"))?;
    let old: Vec<NodeId> = doc
        .descendants(root)
        .into_iter()
        .filter(|&n| doc.is_element(n) && doc.local_name(n) == "shape" && doc.attr(n, "id").is_some_and(|id| id.starts_with("PowerPlusWaterMarkObject")))
        .collect();
    for shape in old {
        let mut run = doc.parent(shape);
        while let Some(r) = run {
            if doc.is_element(r) && doc.local_name(r) == "r" && doc.namespace(r) == Some(W_NS) {
                break;
            }
            run = doc.parent(r);
        }
        if let Some(r) = run {
            doc.detach(r);
        }
    }
    let Some(w) = watermark else { return Ok(()) };
    let para = doc.descendants(root).into_iter().find(|&n| doc.is_element(n) && doc.local_name(n) == "p" && doc.namespace(n) == Some(W_NS));
    let para = match para {
        Some(p) => p,
        None => {
            let prefix = doc.lookup_prefix(root, W_NS).unwrap_or_else(|| "w".into());
            doc.append_element(root, &format!("{prefix}:p"))
        }
    };
    let (width, height) = w.size_in(area.0, area.1);
    let color = format!("#{:06x}", w.color.0 & 0xFF_FF_FF);
    let rotation = if w.rotation.rem_euclid(360) != 0 { format!("rotation:{};", w.rotation.rem_euclid(360)) } else { String::new() };
    let fill = if w.semitransparent { r#"<v:fill opacity=".5"/>"# } else { "" };
    let xml = format!(
        r##"<w:r xmlns:w="{W_NS}" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:w10="urn:schemas-microsoft-com:office:word"><w:rPr><w:noProof/></w:rPr><w:pict><v:shape id="PowerPlusWaterMarkObject{id}" o:spid="_x0000_s{spid}" type="#_x0000_t136" style="position:absolute;margin-left:0;margin-top:0;width:{width:.2}pt;height:{height:.2}pt;{rotation}z-index:-251657216;mso-position-horizontal:center;mso-position-horizontal-relative:margin;mso-position-vertical:center;mso-position-vertical-relative:margin" o:allowincell="f" fillcolor="{color}" stroked="f">{fill}<v:path textpathok="t"/><v:textpath on="t" fitshape="t" style="font-family:&quot;{font}&quot;;font-size:1pt" string="{text}"/><w10:wrap anchorx="margin" anchory="margin"/></v:shape></w:pict></w:r>"##,
        id = 357_831 + index,
        spid = 2_049 + index,
        font = xml_escape(&w.font),
        text = xml_escape(&w.text),
    );
    let fragment = XmlDocument::parse(xml.as_bytes()).map_err(|_| unsupported("透かしの XML"))?;
    let run = fragment.root_element().ok_or_else(|| unsupported("透かしの XML"))?;
    let node = doc.import(&fragment, run);
    let at = doc.children(para).len();
    doc.insert(para, at, node);
    Ok(())
}

pub(crate) fn write_core_properties(package: &mut Package, props: &ibis_doc_model::DocProperties) -> Result<(), Error> {
    const CORE_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
    const CORE_TYPE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
    let rels = package.relationships("")?;
    let core_part = rels
        .iter()
        .find(|r| !r.external && r.kind.ends_with("/metadata/core-properties"))
        .map(|r| r.target.clone())
        .unwrap_or_else(|| "docProps/core.xml".to_string());

    if !package.contains(&core_part) {
        package.set_override(&core_part, CORE_TYPE)?;
        let _ = package.add_relationship("", CORE_REL, &core_part, false);
    }

    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    xml.push('\n');
    xml.push_str(r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#);
    if let Some(t) = &props.title {
        xml.push_str(&format!("<dc:title>{}</dc:title>", xml_escape(t)));
    }
    if let Some(s) = &props.subject {
        xml.push_str(&format!("<dc:subject>{}</dc:subject>", xml_escape(s)));
    }
    if let Some(c) = &props.creator {
        xml.push_str(&format!("<dc:creator>{}</dc:creator>", xml_escape(c)));
    }
    if let Some(k) = &props.keywords {
        xml.push_str(&format!("<cp:keywords>{}</cp:keywords>", xml_escape(k)));
    }
    if let Some(d) = &props.description {
        xml.push_str(&format!("<dc:description>{}</dc:description>", xml_escape(d)));
    }
    if let Some(m) = &props.last_modified_by {
        xml.push_str(&format!("<cp:lastModifiedBy>{}</cp:lastModifiedBy>", xml_escape(m)));
    }
    if let Some(r) = &props.revision {
        xml.push_str(&format!("<cp:revision>{}</cp:revision>", xml_escape(r)));
    }
    if let Some(c) = &props.category {
        xml.push_str(&format!("<cp:category>{}</cp:category>", xml_escape(c)));
    }
    if let Some(created) = &props.created {
        xml.push_str(&format!(r#"<dcterms:created xsi:type="dcterms:W3CDTF">{}</dcterms:created>"#, xml_escape(created)));
    }
    if let Some(modified) = &props.modified {
        xml.push_str(&format!(r#"<dcterms:modified xsi:type="dcterms:W3CDTF">{}</dcterms:modified>"#, xml_escape(modified)));
    }
    if let Some(lp) = &props.last_printed {
        xml.push_str(&format!(r#"<cp:lastPrinted>{}</cp:lastPrinted>"#, xml_escape(lp)));
    }
    xml.push_str("</cp:coreProperties>");

    package.set_bytes(&core_part, xml.into_bytes());
    Ok(())
}

pub(crate) fn write_app_properties(package: &mut Package, props: &ibis_doc_model::DocProperties) -> Result<(), Error> {
    const APP_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties";
    const APP_TYPE: &str = "application/vnd.openxmlformats-officedocument.extended-properties+xml";
    let rels = package.relationships("")?;
    let app_part = rels
        .iter()
        .find(|r| !r.external && r.kind.ends_with("/extended-properties"))
        .map(|r| r.target.clone())
        .unwrap_or_else(|| "docProps/app.xml".to_string());

    if !package.contains(&app_part) {
        package.set_override(&app_part, APP_TYPE)?;
        let _ = package.add_relationship("", APP_REL, &app_part, false);
    }

    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    xml.push('\n');
    xml.push_str(r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">"#);
    if let Some(c) = &props.company {
        xml.push_str(&format!("<Company>{}</Company>", xml_escape(c)));
    }
    if let Some(m) = &props.manager {
        xml.push_str(&format!("<Manager>{}</Manager>", xml_escape(m)));
    }
    xml.push_str("</Properties>");

    package.set_bytes(&app_part, xml.into_bytes());
    Ok(())
}

pub(crate) fn write_custom_properties(
    package: &mut Package,
    custom_props: &[ibis_doc_model::CustomProperty],
    mark_as_final: bool,
) -> Result<(), Error> {
    const CUSTOM_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties";
    const CUSTOM_TYPE: &str = "application/vnd.openxmlformats-officedocument.custom-properties+xml";
    let rels = package.relationships("")?;
    let custom_part = rels
        .iter()
        .find(|r| !r.external && r.kind.ends_with("/custom-properties"))
        .map(|r| r.target.clone())
        .unwrap_or_else(|| "docProps/custom.xml".to_string());

    if custom_props.is_empty() && !mark_as_final {
        if package.contains(&custom_part) {
            package.remove(&custom_part);
            let _ = package.remove_override(&custom_part);
            let rel_id = package.relationships("")?.into_iter().find(|r| !r.external && r.kind.ends_with("/custom-properties")).map(|r| r.id);
            if let Some(id) = rel_id {
                let _ = package.remove_relationship("", &id);
            }
        }
        return Ok(());
    }

    if !package.contains(&custom_part) {
        package.set_override(&custom_part, CUSTOM_TYPE)?;
        let _ = package.add_relationship("", CUSTOM_REL, &custom_part, false);
    }

    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    xml.push('\n');
    xml.push_str(r#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">"#);

    let mut pid = 2u32;
    for prop in custom_props {
        xml.push_str(&format!(r#"<property fmtid="{{D5CDD505-2E9C-101B-9397-08002B2CF9AE}}" pid="{}" name="{}">"#, pid, xml_escape(&prop.name)));
        match &prop.value {
            ibis_doc_model::CustomPropertyValue::Text(s) => {
                xml.push_str("<vt:lpwstr>");
                xml.push_str(&xml_escape(s));
                xml.push_str("</vt:lpwstr>");
            }
            ibis_doc_model::CustomPropertyValue::Number(s) => {
                xml.push_str("<vt:r8>");
                xml.push_str(&xml_escape(s));
                xml.push_str("</vt:r8>");
            }
            ibis_doc_model::CustomPropertyValue::Bool(b) => {
                xml.push_str("<vt:bool>");
                xml.push_str(if *b { "true" } else { "false" });
                xml.push_str("</vt:bool>");
            }
            ibis_doc_model::CustomPropertyValue::DateTime(s) => {
                xml.push_str("<vt:filetime>");
                xml.push_str(&xml_escape(s));
                xml.push_str("</vt:filetime>");
            }
        }
        xml.push_str("</property>");
        pid += 1;
    }

    if mark_as_final && !custom_props.iter().any(|p| p.name == "_MarkAsFinal") {
        xml.push_str(&format!(r#"<property fmtid="{{D5CDD505-2E9C-101B-9397-08002B2CF9AE}}" pid="{}" name="_MarkAsFinal"><vt:bool>true</vt:bool></property>"#, pid));
    }

    xml.push_str("</Properties>");
    package.set_bytes(&custom_part, xml.into_bytes());
    Ok(())
}




/// Whether a `w:t` needs `xml:space="preserve"`: Word drops a space at the start or end of the text, and runs of spaces.
fn needs_space_preserve(text: &str) -> bool {
    text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace) || text.contains("  ")
}

#[cfg(test)]
mod space_tests {
    use super::needs_space_preserve;

    #[test]
    fn spaces_that_word_would_drop_are_preserved() {
        assert!(needs_space_preserve(" a"));
        assert!(needs_space_preserve("a "));
        assert!(needs_space_preserve("a  b"));
        assert!(!needs_space_preserve("a b"));
        assert!(!needs_space_preserve("abc"));
    }
}
