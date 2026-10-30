// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! PresentationML to [`Presentation`].

use std::collections::HashMap;

use ibis_doc_model::{
    AutoShape, Bullet, Color, Crop, EmuRect, EmuSize, Fill, GradientPath, GradientStop,
    Insets, LineEnd, LineSpacing, MediaFormat, MediaId, Paragraph, Picture,
    Placeholder, Presentation, Run, RunStyle, Shape, ShapeKind, Slide, Stroke, TextBody,
    TextStyle, TileAlign, VerticalAnchor,
};
use roxmltree::{Document, Node};

use crate::Error;
use crate::package::{REL_CHART_USER_SHAPES, 
    Package, REL_OFFICE_DOCUMENT, REL_SLIDE_LAYOUT, REL_SLIDE_MASTER, REL_TABLE_STYLES, REL_THEME,
    Relationship,
};
use crate::style::{BulletProps, ListStyle, ParaProps, RunProps, Spacing, auto_number, bullet_glyph};
use crate::theme::{Colors, Theme, bg_fill_style, effect_style, fill_style, line_style};
use crate::xml::{attr, attr_bool, attr_i64, child, children, elements, path};

type Rels = HashMap<String, Relationship>;

// Foreign extension elements can share effect names; compare URI, not prefix.
fn namespace_child<'a, 'i>(node: Node<'a, 'i>, namespace:&str, name: &str) -> Option<Node<'a, 'i>> {
    elements(node).find(|n| n.has_tag_name((namespace, name)))
}
fn unqualified_attribute<'a>(node:Node<'a,'_>,name:&str)->Option<&'a str> {
    node.attributes().find(|a|a.namespace().is_none() && a.name()==name).map(|a|a.value())
}
fn drawing_child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    namespace_child(node,"http://schemas.openxmlformats.org/drawingml/2006/main",name)
}

/// A parsed part and its relationships.
struct Part<'d, 'i> {
    /// The part's name, to record where a shape came from.
    name: String,
    root: Node<'d, 'i>,
    rels: Rels,
}

pub(crate) fn read(data: &[u8]) -> Result<Presentation, Error> {
    read_with_sources(data).map(|(presentation, _)| presentation)
}

/// Read a presentation, with where each slide shape came from (for saving).
pub(crate) fn read_with_sources(data: &[u8]) -> Result<(Presentation, crate::save::pptx::SourceMap), Error> {
    let package = Package::new(data)?;
    let main = package
        .related("", REL_OFFICE_DOCUMENT)?
        .ok_or_else(|| Error::new("プレゼンテーション本体が見つかりません"))?;

    // Load every XML part the slides need up front, so the parsed documents
    // can borrow their text for the rest of the read.
    let main_rels = package.relationships(&main)?;
    let main_xml = package.read_xml(&main)?;
    let main_doc = parse(&main, &main_xml)?;
    let root = main_doc.root_element();

    let slide_refs: Vec<(String, String)> = child(root, "sldIdLst")
        .into_iter()
        .flat_map(|list| children(list, "sldId"))
        .filter_map(attr_rid)
        .filter_map(|rid| main_rels.get(rid).map(|r| (rid.to_string(), r.target.clone())))
        .collect();
    let slide_parts: Vec<String> = slide_refs.iter().map(|(_, part)| part.clone()).collect();
    if slide_parts.is_empty() {
        return Err(Error::new("スライドがありません"));
    }

    let mut xml: HashMap<String, String> = HashMap::new();
    let mut chain: Vec<(String, String, String, Option<String>)> = Vec::new();
    for slide in &slide_parts {
        let layout = package.related(slide, REL_SLIDE_LAYOUT)?.ok_or_else(|| Error::new(format!("{slide} のレイアウトがありません")))?;
        let master = package.related(&layout, REL_SLIDE_MASTER)?.ok_or_else(|| Error::new(format!("{layout} のマスターがありません")))?;
        let theme = package.related(&master, REL_THEME)?;
        for part in [Some(slide), Some(&layout), Some(&master), theme.as_ref()].into_iter().flatten() {
            if !xml.contains_key(part) {
                xml.insert(part.clone(), package.read_xml(part)?);
            }
        }
        chain.push((slide.clone(), layout, master, theme));
    }
    // Every layout of the masters the slides use, for new slides and
    // "apply layout", in each master's order.
    let mut layouts: Vec<(String, String, Option<String>)> = Vec::new();
    for (_, _, master, theme) in &chain {
        if layouts.iter().any(|(_, m, _)| m == master) {
            continue;
        }
        let master_rels = package.relationships(master)?;
        let master_doc_text = xml[master].clone();
        let master_doc = parse(master, &master_doc_text)?;
        let ordered: Vec<String> = child(master_doc.root_element(), "sldLayoutIdLst")
            .into_iter()
            .flat_map(|list| children(list, "sldLayoutId"))
            .filter_map(attr_rid)
            .filter_map(|rid| master_rels.get(rid).filter(|r| r.kind == REL_SLIDE_LAYOUT && !r.external).map(|r| r.target.clone()))
            .collect();
        for layout in ordered {
            if !xml.contains_key(&layout) {
                // A layout that cannot be read is left out of the list.
                let Ok(text) = package.read_xml(&layout) else { continue };
                xml.insert(layout.clone(), text);
            }
            layouts.push((layout, master.clone(), theme.clone()));
        }
    }
    // Layouts slides use but their master does not list come last.
    for (_, layout, master, theme) in &chain {
        if !layouts.iter().any(|(l, _, _)| l == layout) {
            layouts.push((layout.clone(), master.clone(), theme.clone()));
        }
    }
    // Shapes drawn over charts (`c:userShapes`), by the chart's part name.
    let mut user_shapes: HashMap<String, String> = HashMap::new();
    for slide in &slide_parts {
        for rel in package.relationships(slide)?.values().filter(|r| !r.external && r.kind.ends_with("/chart")) {
            if let Ok(Some(drawing)) = package.related(&rel.target, REL_CHART_USER_SHAPES)
                && let Ok(text) = package.read_xml(&drawing)
            {
                user_shapes.insert(rel.target.clone(), drawing.clone());
                xml.insert(drawing, text);
            }
        }
    }
    // The notes pages (their shapes other than placeholders are read).
    let mut notes_parts: HashMap<String, String> = HashMap::new();
    for slide in &slide_parts {
        if let Ok(Some(notes)) = package.related(slide, crate::package::REL_NOTES_SLIDE)
            && let Ok(text) = package.read_xml(&notes)
        {
            notes_parts.insert(slide.clone(), notes.clone());
            xml.insert(notes, text);
        }
    }
    // SmartArt graphics' drawings (`dsp:drawing`), by their data part: the
    // data part names the slide's relationship to it (`dsp:dataModelExt`).
    let mut diagram_drawings: HashMap<String, String> = HashMap::new();
    for slide in &slide_parts {
        let slide_rels = package.relationships(slide)?;
        for rel in slide_rels.values().filter(|r| !r.external && r.kind.ends_with("/diagramData")) {
            let Ok(data) = package.read_xml(&rel.target) else { continue };
            let Some(id) = data.split("dataModelExt").nth(1).and_then(|s| s.split("relId=\"").nth(1)).and_then(|s| s.split('"').next()) else { continue };
            let Some(drawing) = slide_rels.get(id).filter(|r| !r.external && r.kind.ends_with("/diagramDrawing")).map(|r| r.target.clone()) else { continue };
            if let Ok(text) = package.read_xml(&drawing) {
                diagram_drawings.insert(rel.target.clone(), drawing.clone());
                xml.insert(drawing, text);
            }
        }
    }
    let table_styles_part = main_rels.values().find(|r| r.kind == REL_TABLE_STYLES && !r.external).map(|r| r.target.clone());
    if let Some(part) = &table_styles_part
        && let Ok(text) = package.read_xml(part)
    {
        xml.insert(part.clone(), text);
    }
    let mut docs: HashMap<&str, Document> = HashMap::new();
    for (name, text) in &xml {
        docs.insert(name.as_str(), parse(name, text)?);
    }
    let mut rels: HashMap<&str, Rels> = HashMap::new();
    for name in xml.keys() {
        rels.insert(name.as_str(), package.relationships(name)?);
    }
    let mut themes: HashMap<&str, Theme> = HashMap::new();
    for (_, _, _, theme) in &chain {
        if let Some(t) = theme {
            themes.entry(t.as_str()).or_insert_with(|| Theme::read(docs[t.as_str()].root_element()));
        }
    }
    let empty_theme = Theme::default();

    let size = child(root, "sldSz")
        .map(|s| EmuSize::new(attr_i64(s, "cx").unwrap_or(0), attr_i64(s, "cy").unwrap_or(0)))
        .filter(|s| s.width > 0 && s.height > 0)
        .unwrap_or(EmuSize::WIDESCREEN);
    let notes_size = child(root, "notesSz")
        .map(|s| EmuSize::new(attr_i64(s, "cx").unwrap_or(0), attr_i64(s, "cy").unwrap_or(0)))
        .filter(|s| s.width > 0 && s.height > 0)
        .unwrap_or(EmuSize::new(6_858_000, 9_144_000));
    let mut reader = Reader {
        package: &package,
        presentation: Presentation::empty(size),
        media: HashMap::new(),
        default_text: child(root, "defaultTextStyle"),
        table_styles: table_styles_part.as_deref().and_then(|p| docs.get(p)).map(|d| d.root_element()),
        user_shapes: user_shapes.iter().map(|(chart, drawing)| (chart.clone(), (docs[drawing.as_str()].root_element(), rels[drawing.as_str()].clone()))).collect(),
        diagrams: diagram_drawings.iter().map(|(data, drawing)| (data.clone(), (docs[drawing.as_str()].root_element(), rels[drawing.as_str()].clone()))).collect(),
        native_ids: HashMap::new(),
        last_native_ids: HashMap::new(),
        pending: Vec::new(),
        default_font_size: 1800,
        anchor: None,
        charts: Default::default(),
        sources: crate::save::pptx::SourceMap { presentation: main.clone(), ..Default::default() },
        text_sources: None,
        para_sources: Vec::new(),
        slide_placeholders: HashMap::new(),
        slide_names: HashMap::new(),
        slide_alt_texts: HashMap::new(),
        slide_decorative: Default::default(),
        slide_return_to_zoom: Default::default(),
        slide_zoom_show_bg: HashMap::new(),
        slide_zoom_durations: HashMap::new(),
        slide_hidden: Default::default(),
        slide_links: HashMap::new(),
        slide_link_tooltips: HashMap::new(),
        slide_aspect: Default::default(),
        slide_formulas: HashMap::new(),
        slide_glues: Vec::new(),
        slide_parts: HashMap::new(),
        chart_sources: Vec::new(),
        xlsx_shape_sources: Vec::new(),
    };

    let comment_authors = crate::comments::authors(&package, &main_rels);
    // The slides' ids first, for hyperlinks to slides read later.
    let slide_ids: Vec<ibis_doc_model::SlideId> = chain.iter().map(|_| reader.presentation.new_slide_id()).collect();
    reader.slide_parts = chain.iter().zip(&slide_ids).map(|((part, _, _, _), id)| (part.clone(), *id)).collect();
    for (number, (slide, layout, master, theme)) in chain.iter().enumerate() {
        let part = |name: &str| Part { name: name.to_string(), root: docs[name].root_element(), rels: rels[name].clone() };
        let (slide, layout, master) = (part(slide), part(layout), part(master));
        let theme_root = theme.as_ref().map(|t| docs[t.as_str()].root_element());
        let theme = theme.as_ref().map(|t| &themes[t.as_str()]).unwrap_or(&empty_theme);
        let colors = Colors::new(
            theme,
            child(master.root, "clrMap"),
            &[child(layout.root, "clrMapOvr"), child(slide.root, "clrMapOvr")],
        );
        // Slide numbers count from `firstSlideNum` (1 unless stated).
        let first = attr_i64(root, "firstSlideNum").unwrap_or(1).max(0) as usize;
        let ctx = SlideCtx { colors, theme: theme_root, slide, layout, master, number: number + first };
        let mut slide = reader.slide(&ctx, slide_ids[number]);
        slide.notes = read_notes(&package, &ctx.slide.name);
        if let Some(notes_part) = notes_parts.get(&ctx.slide.name) {
            let notes = part(notes_part);
            if reader.presentation.notes_marks.is_empty()
                && let Some(marks) = notes_marks_of(notes.root)
            {
                reader.presentation.notes_marks = marks;
            }
            let notes_colors = Colors::new(theme, child(ctx.master.root, "clrMap"), &[child(ctx.layout.root, "clrMapOvr")]);
            let notes_ctx = SlideCtx { colors: notes_colors, theme: ctx.theme, slide: Part { name: notes.name.clone(), root: notes.root, rels: notes.rels.clone() }, layout: notes, master: Part { name: ctx.master.name.clone(), root: ctx.master.root, rels: ctx.master.rels.clone() }, number: ctx.number };
            let mut shapes = Vec::new();
            // As a layout's: its placeholders (the slide image, the notes) are not shapes.
            reader.tree(&notes_ctx, notes_ctx.slide.root, &notes_ctx.slide.rels, Level::Layout, &mut shapes);
            slide.notes_shapes = shapes;
        }
        slide.comments = crate::comments::read(&package, &ctx.slide.name, &comment_authors, &reader.last_native_ids);
        reader.sources.slides.insert(slide.id, crate::save::pptx::SlideSource { part: ctx.slide.name.clone(), rel_id: slide_refs[number].0.clone() });
        reader.presentation.slides.push(slide);
    }
    // Where the notes master puts the header, date, footer and number.
    if let Ok(Some(master)) = package.related(&main, crate::package::REL_NOTES_MASTER)
        && let Ok(text) = package.read_xml(&master)
        && let Ok(doc) = Document::parse(&text)
    {
        reader.presentation.notes_frames = notes_master_frames(doc.root_element());
    }
    // Sections use the numeric IDs from presentation.xml, not model IDs.
    const P14:&str="http://schemas.microsoft.com/office/powerpoint/2010/main";
    const P:&str="http://schemas.openxmlformats.org/presentationml/2006/main";
    let numeric=namespace_child(root,P,"sldIdLst").into_iter().flat_map(elements).filter(|n|n.has_tag_name((P,"sldId"))).filter_map(|n|Some((attr_rid(n)?,unqualified_attribute(n,"id")?))).collect::<HashMap<_,_>>();
    let members=slide_refs.iter().zip(&reader.presentation.slides).filter_map(|((rid,_),slide)|numeric.get(rid.as_str()).map(|&number|(number,slide.id))).collect::<HashMap<_,_>>();
    if let Some(ext)=namespace_child(root,P,"extLst") {
        if let Some(list)=elements(ext).filter(|n|n.has_tag_name((P,"ext"))&&unqualified_attribute(*n,"uri")==Some("{521415D9-36F7-43E2-AB2F-B90AF26B5E84}")).find_map(|n|namespace_child(n,P14,"sectionLst")) {
            reader.presentation.sections=elements(list).filter(|n|n.has_tag_name((P14,"section"))).map(|n|ibis_doc_model::Section {
                id:unqualified_attribute(n,"id").unwrap_or("").to_owned(),name:unqualified_attribute(n,"name").unwrap_or("").to_owned(),
                slides:namespace_child(n,P14,"sldIdLst").into_iter().flat_map(elements).filter(|n|n.has_tag_name((P14,"sldId"))).filter_map(|n|unqualified_attribute(n,"id").and_then(|id|members.get(id)).copied()).collect(),
            }).collect();
        }
    }
    // Layouts are read after the slides, so the ids of what is on the
    // slides do not depend on them.
    let first = attr_i64(root, "firstSlideNum").unwrap_or(1).max(0) as usize;
    let mut layout_ids: HashMap<String, ibis_doc_model::LayoutId> = HashMap::new();
    let mut master_ids: HashMap<String, ibis_doc_model::MasterId> = HashMap::new();
    for (layout, master, theme) in &layouts {
        let master_id = match master_ids.get(master) {
            Some(id) => *id,
            None => {
                let id = reader.presentation.new_master_id();
                let name = theme.as_ref().and_then(|t| attr(docs[t.as_str()].root_element(), "name")).unwrap_or("").to_string();
                reader.presentation.masters.push(ibis_doc_model::Master { id, name, copy_of: None });
                reader.sources.masters.insert(id, master.clone());
                master_ids.insert(master.clone(), id);
                id
            }
        };
        let part = |name: &str| Part { name: name.to_string(), root: docs[name].root_element(), rels: rels[name].clone() };
        let (layout_part, same, master_part) = (part(layout), part(layout), part(master));
        let theme_root = theme.as_ref().map(|t| docs[t.as_str()].root_element());
        let theme = theme.as_ref().map(|t| &themes[t.as_str()]).unwrap_or(&empty_theme);
        let colors = Colors::new(theme, child(master_part.root, "clrMap"), &[child(layout_part.root, "clrMapOvr")]);
        let name = path(layout_part.root, &["cSld"]).and_then(|c| attr(c, "name")).unwrap_or("").to_string();
        let ctx = SlideCtx { colors, theme: theme_root, slide: layout_part, layout: same, master: master_part, number: first };
        let id = reader.presentation.new_layout_id();
        let background = reader.background(&ctx);
        let mut backdrop = Vec::new();
        if attr_bool(ctx.layout.root, "showMasterSp").unwrap_or(true) {
            reader.tree(&ctx, ctx.master.root, &ctx.master.rels, Level::Master, &mut backdrop);
        }
        reader.tree(&ctx, ctx.layout.root, &ctx.layout.rels, Level::Layout, &mut backdrop);
        let placeholders = layout_placeholders(ctx.layout.root);
        let master_placeholders = layout_placeholders(ctx.master.root);
        reader.presentation.layouts.push(ibis_doc_model::Layout { id, name, background, backdrop, placeholders, master_placeholders, copy_of: None, master: Some(master_id) });
        reader.sources.layouts.insert(id, layout.clone());
        layout_ids.insert(layout.clone(), id);
    }
    for (slide, (_, layout, _, _)) in reader.presentation.slides.iter_mut().zip(&chain) {
        slide.layout = layout_ids.get(layout).copied();
    }
    reader.presentation.first_slide_number = first;
    reader.presentation.notes_size = notes_size;
    // ヘッダーとフッター's marks: the presentation's, and the first slide's
    // master's `p:hf` (each kind shown unless stated).
    let hf = chain.first().and_then(|(_, _, master, _)| child(docs[master.as_str()].root_element(), "hf"));
    let shown = |name: &str| hf.and_then(|h| attr_bool(h, name)).unwrap_or(true);
    reader.presentation.header_footer = ibis_doc_model::model::HeaderFooterMarks {
        on_title_slides: attr_bool(root, "showSpecialPlsOnTitleSld").unwrap_or(true),
        header: shown("hdr"),
        date: shown("dt"),
        footer: shown("ftr"),
        number: shown("sldNum"),
    };
    // The theme of the first slide's master.
    reader.presentation.theme_fonts = chain.first().and_then(|(_, _, _, t)| t.as_ref()).map(|t| themes[t.as_str()].fonts());
    reader.presentation.theme_colors = chain.first().and_then(|(_, _, _, t)| t.as_ref()).map(|t| themes[t.as_str()].colors()).unwrap_or_default();
    reader.presentation.theme_effects = chain
        .first()
        .and_then(|(_, _, _, t)| t.as_ref())
        .and_then(|t| themes[t.as_str()].fmt_scheme_name.as_deref())
        .map(|name| {
            crate::theme_effects::find_effect_preset(name)
                .map(|p| p.id.to_string())
                .unwrap_or_else(|| name.to_string())
        });
    if let Some(t) = chain.first().and_then(|(_, _, _, t)| t.as_ref()) {
        if let Some(theme_doc) = docs.get(t.as_str()) {
            let theme_colors = Colors::new(&themes[t.as_str()], None, &[]);
            reader.presentation.object_defaults = read_object_defaults(theme_doc.root_element(), &theme_colors);
        }
    }
    reader.presentation.fonts = embedded_fonts(&package, root, &main_rels);
    reader.presentation.show_properties = read_show_properties(&package, &main, &main_rels);
    // Custom shows (目的別スライド ショー, p:custShowLst)
    let slide_by_rel_id: HashMap<&str, ibis_doc_model::SlideId> = slide_refs
        .iter()
        .zip(&reader.presentation.slides)
        .map(|((rid, _), slide)| (rid.as_str(), slide.id))
        .collect();
    if let Some(list) = namespace_child(root, P, "custShowLst") {
        reader.presentation.custom_shows = elements(list)
            .filter(|n| n.has_tag_name((P, "custShow")))
            .map(|n| {
                let id = unqualified_attribute(n, "id").and_then(|s| s.parse().ok()).unwrap_or(0);
                let name = unqualified_attribute(n, "name").unwrap_or("").to_string();
                let slides = namespace_child(n, P, "sldLst")
                    .into_iter()
                    .flat_map(elements)
                    .filter(|s| s.has_tag_name((P, "sld")))
                    .filter_map(|s| attr_rid(s).and_then(|rid| slide_by_rel_id.get(rid)).copied())
                    .collect();
                ibis_doc_model::CustomShow { id, name, slides }
            })
            .collect();
    }
    // フォトアルバムの設定 (p:photoAlbum)
    if let Some(album_node) = namespace_child(root, P, "photoAlbum").or_else(|| child(root, "photoAlbum")) {
        let bw = unqualified_attribute(album_node, "bw").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        let show_captions = unqualified_attribute(album_node, "showCaptions").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        let layout = unqualified_attribute(album_node, "layout").and_then(ibis_doc_model::PhotoAlbumLayout::parse).unwrap_or_default();
        let frame = unqualified_attribute(album_node, "frame").and_then(ibis_doc_model::PhotoAlbumFrame::parse).unwrap_or_default();
        reader.presentation.photo_album = Some(ibis_doc_model::PhotoAlbum { bw, show_captions, layout, frame });
    }
    // ドキュメント プロパティ（docProps/core.xml, docProps/app.xml, docProps/custom.xml）および最終版フラグ.
    let mut props = crate::package::read_doc_properties(&package);
    if let Some(pos) = props.custom.iter().position(|p| p.name == "_MarkAsFinal") {
        let final_prop = props.custom.remove(pos);
        if let ibis_doc_model::CustomPropertyValue::Bool(b) = final_prop.value {
            if b {
                reader.presentation.mark_as_final = true;
            }
        }
    }
    for prop in &props.custom {
        if let Some(target) = prop.name.strip_prefix("_FontSub_") {
            if let ibis_doc_model::CustomPropertyValue::Text(ref sub) = prop.value {
                reader.presentation.font_substitutions.insert(target.to_string(), sub.clone());
            }
        }
    }
    reader.presentation.properties = props;
    // 書き込みパスワード（p:modifyVerifier）: the algorithm by its number or its name, the salt and the hash.
    if let Some(v) = child(root, "modifyVerifier") {
        let algorithm = attr(v, "cryptAlgorithmSid").and_then(|s| s.parse::<u32>().ok()).or_else(|| attr(v, "algorithmName").and_then(ibis_ooxml::modify_password::Algorithm::from_name).map(|a| a.sid()));
        let salt = attr(v, "saltData").or_else(|| attr(v, "saltValue"));
        let hash = attr(v, "hashData").or_else(|| attr(v, "hashValue"));
        if let (Some(algorithm_sid), Some(salt), Some(hash)) = (algorithm, salt, hash) {
            reader.presentation.modify_verifier = Some(ibis_doc_model::ModifyVerifier { algorithm_sid, spin_count: attr(v, "spinCount").and_then(|s| s.parse().ok()).unwrap_or(0), salt: salt.to_string(), hash: hash.to_string() });
        }
    }
    // 読み取り専用を推奨（presentation.xml の p:extLst 内 readonlyRecommended）.
    if let Some(ext_lst) = namespace_child(root, P, "extLst") {
        for ext in elements(ext_lst).filter(|n| n.has_tag_name((P, "ext"))) {
            for child_node in elements(ext) {
                if child_node.tag_name().name() == "readonlyRecommended" {
                    let val = unqualified_attribute(child_node, "val").unwrap_or("1");
                    if val == "1" || val.eq_ignore_ascii_case("true") {
                        reader.presentation.read_only_recommended = true;
                    }
                }
            }
        }
    }
    Ok((reader.presentation, reader.sources))
}

/// The fonts of `p:embeddedFontLst`. A font part that cannot be read is left out.
fn embedded_fonts(package: &Package, root: Node, rels: &Rels) -> Vec<ibis_doc_model::EmbeddedFont> {
    let mut out = Vec::new();
    let Some(list) = child(root, "embeddedFontLst") else { return out };
    for entry in children(list, "embeddedFont") {
        let Some(typeface) = child(entry, "font").and_then(|f| attr(f, "typeface")) else { continue };
        for (name, bold, italic) in [("regular", false, false), ("bold", true, false), ("italic", false, true), ("boldItalic", true, true)] {
            let Some(rel) = child(entry, name).and_then(attr_rid).and_then(|id| rels.get(id)) else { continue };
            if let Ok(data) = package.read(&rel.target) {
                out.push(ibis_doc_model::EmbeddedFont { typeface: typeface.to_string(), bold, italic, data: data.into() });
            }
        }
    }
    out
}

fn read_object_defaults<'d, 'i>(theme_node: Node<'d, 'i>, colors: &Colors) -> ibis_doc_model::ObjectDefaults {
    let mut defaults = ibis_doc_model::ObjectDefaults::default();
    let Some(obj_def) = drawing_child(theme_node, "objectDefaults").or_else(|| child(theme_node, "objectDefaults")) else {
        return defaults;
    };

    // 1. spDef -> ShapeDefault
    if let Some(sp_def) = drawing_child(obj_def, "spDef").or_else(|| child(obj_def, "spDef")) {
        if let Some(sp_pr) = drawing_child(sp_def, "spPr").or_else(|| child(sp_def, "spPr")) {
            let fill = fill_element(sp_pr).and_then(|node| match node.tag_name().name() {
                "noFill" => Some(Fill::None),
                "solidFill" => colors.in_parent(node, None).map(Fill::Solid),
                "gradFill" => {
                    let mut stops: Vec<GradientStop> = drawing_child(node, "gsLst")
                        .into_iter()
                        .flat_map(|l| elements(l).filter(|n| n.has_tag_name(("http://schemas.openxmlformats.org/drawingml/2006/main", "gs"))))
                        .filter_map(|gs| {
                            let color = colors.in_parent(gs, None)?;
                            Some(GradientStop { position: unqualified_attribute(gs, "pos").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0).clamp(0, 100_000) as u32, color })
                        })
                        .collect();
                    stops.sort_by_key(|s| s.position);
                    if stops.is_empty() {
                        None
                    } else {
                        let lin = drawing_child(node, "lin");
                        let angle = lin.and_then(|l| unqualified_attribute(l, "ang")).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(5_400_000) as i32;
                        let scaled = lin.and_then(|l| unqualified_attribute(l, "scaled")).map_or(false, |v| v == "1" || v == "true");
                        Some(Fill::Linear { angle, scaled, stops })
                    }
                }
                _ => None,
            }).unwrap_or(Fill::Solid(Color::opaque(ibis_doc_model::Rgb(0x4472c4))));

            let stroke = drawing_child(sp_pr, "ln").and_then(|ln| {
                let color_node = fill_element(ln)?;
                let color = match color_node.tag_name().name() {
                    "solidFill" => colors.in_parent(color_node, None)?,
                    _ => return None,
                };
                let width = unqualified_attribute(ln, "w").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(Stroke::DEFAULT_WIDTH);
                let dash_kind = drawing_child(ln, "prstDash")
                    .and_then(|d| unqualified_attribute(d, "val"))
                    .and_then(ibis_doc_model::LineDash::parse)
                    .unwrap_or_default();
                let end = |name| {
                    let e = drawing_child(ln, name);
                    ibis_doc_model::LineEnd::parse(e.and_then(|e| unqualified_attribute(e, "type")), e.and_then(|e| unqualified_attribute(e, "w")), e.and_then(|e| unqualified_attribute(e, "len")))
                };
                Some(Stroke {
                    color,
                    width,
                    dashed: dash_kind != ibis_doc_model::LineDash::Solid,
                    head: end("headEnd"),
                    tail: end("tailEnd"),
                    dash: dash_kind,
                    cap: match unqualified_attribute(ln, "cap") {
                        Some("rnd") => ibis_doc_model::LineCap::Round,
                        Some("sq") => ibis_doc_model::LineCap::Square,
                        _ => ibis_doc_model::LineCap::Flat,
                    },
                    join: ibis_doc_model::LineJoin::Miter,
                    compound: unqualified_attribute(ln, "cmpd").and_then(ibis_doc_model::LineCompound::parse).unwrap_or_default(),
                    alignment: unqualified_attribute(ln, "algn").and_then(ibis_doc_model::LineAlignment::parse).unwrap_or_default(),
                    fill: None,
                })
            });

            let effect_lst = drawing_child(sp_pr, "effectLst");
            let shadow = effect_lst.and_then(|e| drawing_child(e, "outerShdw")).and_then(|e| {
                let color = colors.in_parent(e, None)?;
                Some(ibis_doc_model::Shadow {
                    color,
                    blur: unqualified_attribute(e, "blurRad").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0),
                    distance: unqualified_attribute(e, "dist").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0),
                    direction: unqualified_attribute(e, "dir").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0) as i32,
                    sx: unqualified_attribute(e, "sx").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(100_000) as i32,
                    sy: unqualified_attribute(e, "sy").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(100_000) as i32,
                    kx: unqualified_attribute(e, "kx").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0) as i32,
                    ky: unqualified_attribute(e, "ky").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0) as i32,
                    align: unqualified_attribute(e, "algn").map(TileAlign::parse).unwrap_or(TileAlign::Bottom),
                    rot_with_shape: unqualified_attribute(e, "rotWithShape").is_none_or(|v| v != "0" && v != "false"),
                })
            });
            let glow = effect_lst.and_then(|e| drawing_child(e, "glow")).and_then(|e| {
                let color = colors.in_parent(e, None)?;
                let radius = unqualified_attribute(e, "rad").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
                (radius > 0).then_some((color, radius))
            });
            let soft_edge = effect_lst.and_then(|e| drawing_child(e, "softEdge")).and_then(|e| {
                unqualified_attribute(e, "rad").and_then(|v| v.trim().parse::<i64>().ok()).filter(|&r| r > 0)
            });
            let reflection = effect_lst.and_then(|e| drawing_child(e, "reflection")).map(|e| {
                ibis_doc_model::Reflection {
                    blur: unqualified_attribute(e, "blurRad").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0),
                    start_alpha: unqualified_attribute(e, "stA").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(100_000) as i32,
                    end_alpha: unqualified_attribute(e, "endA").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0) as i32,
                    start_pos: unqualified_attribute(e, "stPos").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0) as i32,
                    end_pos: unqualified_attribute(e, "endPos").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(100_000) as i32,
                    distance: unqualified_attribute(e, "dist").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0),
                    direction: unqualified_attribute(e, "dir").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0) as i32,
                    fade_direction: unqualified_attribute(e, "fadeDir").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(5_400_000) as i32,
                    sx: unqualified_attribute(e, "sx").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(100_000) as i32,
                    sy: unqualified_attribute(e, "sy").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(100_000) as i32,
                    kx: unqualified_attribute(e, "kx").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0) as i32,
                    ky: unqualified_attribute(e, "ky").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0) as i32,
                    align: unqualified_attribute(e, "algn").map(TileAlign::parse).unwrap_or(TileAlign::Bottom),
                    rot_with_shape: unqualified_attribute(e, "rotWithShape").is_none_or(|v| v != "0" && v != "false"),
                }
            });

            let text = if let Some(lst_style) = drawing_child(sp_def, "lstStyle").or_else(|| child(sp_def, "lstStyle")) {
                let def_p_pr = drawing_child(lst_style, "defPPr").or_else(|| child(lst_style, "defPPr"));
                let align = def_p_pr
                    .and_then(|p| unqualified_attribute(p, "algn"))
                    .map(|a| match a {
                        "l" => ibis_doc_model::TextAlign::Left,
                        "r" => ibis_doc_model::TextAlign::Right,
                        "just" => ibis_doc_model::TextAlign::Justify,
                        _ => ibis_doc_model::TextAlign::Center,
                    })
                    .unwrap_or(ibis_doc_model::TextAlign::Center);
                let def_r_pr = def_p_pr.and_then(|p| drawing_child(p, "defRPr").or_else(|| child(p, "defRPr")));
                let bold = def_r_pr.and_then(|r| unqualified_attribute(r, "b")).map(|v| v == "1" || v == "true").unwrap_or(false);
                let italic = def_r_pr.and_then(|r| unqualified_attribute(r, "i")).map(|v| v == "1" || v == "true").unwrap_or(false);
                let font_size = def_r_pr.and_then(|r| unqualified_attribute(r, "sz")).and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(1800);
                let color = def_r_pr.and_then(fill_element).and_then(|node| match node.tag_name().name() {
                    "solidFill" => colors.in_parent(node, None),
                    _ => None,
                }).map(|c| c.rgb).unwrap_or(ibis_doc_model::Rgb::WHITE);
                ibis_doc_model::TextStyle {
                    color,
                    font_size,
                    bold,
                    italic,
                    align,
                    ..Default::default()
                }
            } else {
                ibis_doc_model::TextStyle { color: ibis_doc_model::Rgb::WHITE, align: ibis_doc_model::TextAlign::Center, ..Default::default() }
            };

            defaults.shape = Some(ibis_doc_model::ShapeDefault {
                fill,
                stroke,
                text,
                shadow,
                glow,
                reflection,
                soft_edge,
            });
        }
    }

    // 2. lnDef -> Stroke
    if let Some(ln_def) = drawing_child(obj_def, "lnDef").or_else(|| child(obj_def, "lnDef")) {
        if let Some(sp_pr) = drawing_child(ln_def, "spPr").or_else(|| child(ln_def, "spPr")) {
            if let Some(ln) = drawing_child(sp_pr, "ln") {
                if let Some(color_node) = fill_element(ln) {
                    if let Some(color) = colors.in_parent(color_node, None) {
                        let width = unqualified_attribute(ln, "w").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(Stroke::DEFAULT_WIDTH);
                        let dash_kind = drawing_child(ln, "prstDash")
                            .and_then(|d| unqualified_attribute(d, "val"))
                            .and_then(ibis_doc_model::LineDash::parse)
                            .unwrap_or_default();
                        let end = |name| {
                            let e = drawing_child(ln, name);
                            ibis_doc_model::LineEnd::parse(e.and_then(|e| unqualified_attribute(e, "type")), e.and_then(|e| unqualified_attribute(e, "w")), e.and_then(|e| unqualified_attribute(e, "len")))
                        };
                        defaults.line = Some(Stroke {
                            color,
                            width,
                            dashed: dash_kind != ibis_doc_model::LineDash::Solid,
                            head: end("headEnd"),
                            tail: end("tailEnd"),
                            dash: dash_kind,
                            cap: match unqualified_attribute(ln, "cap") {
                                Some("rnd") => ibis_doc_model::LineCap::Round,
                                Some("sq") => ibis_doc_model::LineCap::Square,
                                _ => ibis_doc_model::LineCap::Flat,
                            },
                            join: ibis_doc_model::LineJoin::Miter,
                            compound: unqualified_attribute(ln, "cmpd").and_then(ibis_doc_model::LineCompound::parse).unwrap_or_default(),
                            alignment: unqualified_attribute(ln, "algn").and_then(ibis_doc_model::LineAlignment::parse).unwrap_or_default(),
                            fill: None,
                        });
                    }
                }
            }
        }
    }

    defaults
}

const REL_PRES_PROPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps";

fn read_show_properties(package: &Package, _main: &str, rels: &HashMap<String, crate::package::Relationship>) -> ibis_doc_model::ShowProperties {
    use ibis_doc_model::{ShowProperties, ShowSlideRange, ShowType};
    let target = rels
        .values()
        .find(|r| !r.external && r.kind == REL_PRES_PROPS)
        .map(|r| r.target.clone())
        .unwrap_or_else(|| "ppt/presProps.xml".to_string());
    let Ok(data) = package.read(&target) else {
        return ShowProperties::default();
    };
    let Ok(text) = std::str::from_utf8(&data) else {
        return ShowProperties::default();
    };
    let Ok(doc) = roxmltree::Document::parse(text) else {
        return ShowProperties::default();
    };
    let Some(show_pr) = doc.descendants().find(|n| n.is_element() && n.tag_name().name() == "showPr") else {
        return ShowProperties::default();
    };

    let loop_continuously = attr_bool(show_pr, "loop").unwrap_or(false);
    let show_narration = attr_bool(show_pr, "showNarration").unwrap_or(true);
    let show_animation = attr_bool(show_pr, "showAnimation").unwrap_or(true);
    let use_timings = attr_bool(show_pr, "useTimings").unwrap_or(true);

    let mut show_type = ShowType::Presenter;
    let mut slide_range = ShowSlideRange::All;
    let mut pen_color = None;

    for child in show_pr.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "present" => show_type = ShowType::Presenter,
            "browse" => show_type = ShowType::Browse,
            "kiosk" => {
                let restart = child.attribute("restart").and_then(|s| s.parse().ok());
                show_type = ShowType::Kiosk { restart };
            }
            "sldAll" => slide_range = ShowSlideRange::All,
            "sldRg" => {
                let start = child.attribute("st").and_then(|s| s.parse().ok()).unwrap_or(1);
                let end = child.attribute("end").and_then(|s| s.parse().ok()).unwrap_or(1);
                slide_range = ShowSlideRange::Range { start, end };
            }
            "custShow" => {
                if let Some(id) = child.attribute("id").and_then(|s| s.parse().ok()) {
                    slide_range = ShowSlideRange::Custom(id);
                }
            }
            "penClr" => {
                if let Some(srgb) = child.descendants().find(|n| n.is_element() && n.tag_name().name() == "srgbClr") {
                    if let Some(val) = srgb.attribute("val") {
                        if let Ok(rgb) = u32::from_str_radix(val, 16) {
                            pen_color = Some(ibis_doc_model::drawing::Rgb(rgb));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    ShowProperties {
        show_type,
        loop_continuously,
        show_narration,
        show_animation,
        use_timings,
        slide_range,
        pen_color,
    }
}

/// `r:id` of an `p:sldId`; its namespace prefix varies between producers.
fn descendant_named<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.descendants().find(|n| n.is_element() && n.tag_name().name() == name)
}

fn attr_rid<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    node.attributes().find(|a| a.name() == "id" && a.namespace().is_some()).map(|a| a.value())
}

/// The formulas of `sheet` with the external workbooks they refer to by
/// name (`[1]Data!A1` as `[売上.xlsx]Data!A1`), as Excel shows them.
fn name_external_books(sheet: &mut ibis_doc_model::Sheet, books: &[String]) {
    if books.iter().all(String::is_empty) {
        return;
    }
    for cell in sheet.cells.iter_mut() {
        if let Some(f) = cell.formula.as_deref().filter(|f| ibis_calc::formula::has_external_book(f)) {
            cell.formula = Some(ibis_calc::formula::books_by_name(f, books));
        }
    }
}

/// The size in pixels of a PNG, JPEG or GIF picture, from its header.
fn image_pixels(data: &[u8]) -> Option<(u32, u32)> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") && data.len() >= 24 {
        return Some((u32::from_be_bytes(data[16..20].try_into().ok()?), u32::from_be_bytes(data[20..24].try_into().ok()?)));
    }
    if data.starts_with(b"GIF8") && data.len() >= 10 {
        return Some((u16::from_le_bytes([data[6], data[7]]) as u32, u16::from_le_bytes([data[8], data[9]]) as u32));
    }
    if data.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = data[i + 1];
            if marker == 0xFF || marker == 0x00 || marker == 0x01 || (0xD0..=0xD9).contains(&marker) {
                i += if marker == 0xFF { 1 } else { 2 };
                continue;
            }
            let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                return Some((u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32, u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32));
            }
            i += 2 + len;
        }
    }
    None
}

fn parse<'i>(name: &str, text: &'i str) -> Result<Document<'i>, Error> {
    let options = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    Document::parse_with_options(text, options).map_err(|e| Error::xml(name, e))
}

struct Reader<'p, 'd, 'i> {
    package: &'p Package<'p>,
    presentation: Presentation,
    /// Media already stored, by part name.
    media: HashMap<String, (MediaId, MediaFormat)>,
    default_text: Option<Node<'d, 'i>>,
    /// `a:tblStyleLst` of the presentation's table styles part.
    table_styles: Option<Node<'d, 'i>>,
    /// The `c:userShapes` of charts, with their relationships, by chart part.
    user_shapes: HashMap<String, (Node<'d, 'i>, Rels)>,
    /// SmartArt drawings (`dsp:drawing`) and their relationships, by the
    /// graphic's data part.
    diagrams: HashMap<String, (Node<'d, 'i>, Rels)>,
    /// The slide's shapes by their drawing id (`cNvPr/@id`), which its
    /// animations name (`p:spTgt/@spid`).
    native_ids: HashMap<String, ibis_doc_model::ShapeId>,
    /// The last slide's drawing ids (for its comments' shapes).
    last_native_ids: HashMap<String, ibis_doc_model::ShapeId>,
    /// Shapes to add after the one just read (a chart's user shapes).
    pending: Vec<Shape>,
    /// Text size where nothing sets one: 18 pt on slides, 11 pt on sheets.
    default_font_size: u32,
    /// The frame of the next graphic frame, from a sheet's cell anchor
    /// (whose own `xfrm` is empty).
    anchor: Option<EmuRect>,
    /// Charts read ahead, by part, taken as the drawing reaches them.
    charts: std::cell::RefCell<HashMap<String, Option<ibis_doc_model::Chart>>>,
    /// Where each slide shape came from, for saving.
    sources: crate::save::pptx::SourceMap,
    /// While a slide shape's text is read: where each run of each
    /// paragraph came from (see `paragraph`).
    text_sources: Option<Vec<Vec<Option<crate::save::pptx::RunSource>>>>,
    /// Alongside `text_sources`: the `a:p` of each paragraph and the
    /// element of each of its runs.
    para_sources: Vec<crate::save::pptx::ParagraphSource>,
    /// The placeholders of the slide being read.
    slide_placeholders: HashMap<ibis_doc_model::ShapeId, ibis_doc_model::SlidePlaceholder>,
    /// The slide's shapes' names and those hidden (`cNvPr/@name`, `@hidden`).
    slide_names: HashMap<ibis_doc_model::ShapeId, String>,
    slide_alt_texts: HashMap<ibis_doc_model::ShapeId, ibis_doc_model::AltText>,
    slide_decorative: std::collections::HashSet<ibis_doc_model::ShapeId>,
    slide_return_to_zoom: std::collections::HashSet<ibis_doc_model::ShapeId>,
    slide_zoom_show_bg: HashMap<ibis_doc_model::ShapeId, bool>,
    slide_zoom_durations: HashMap<ibis_doc_model::ShapeId, u32>,
    slide_hidden: std::collections::HashSet<ibis_doc_model::ShapeId>,
    slide_links: HashMap<ibis_doc_model::ShapeId, ibis_doc_model::SlideLink>,
    slide_link_tooltips: HashMap<ibis_doc_model::ShapeId, String>,
    slide_aspect: std::collections::HashSet<ibis_doc_model::ShapeId>,
    slide_formulas: HashMap<ibis_doc_model::ShapeId, String>,
    slide_glues: Vec<(ibis_doc_model::ShapeId, Option<(String, u32)>, Option<(String, u32)>)>,
    /// The slides by their parts (for hyperlinks to them).
    slide_parts: HashMap<String, ibis_doc_model::SlideId>,
    /// A sheet's charts as read: the shape, its chart part and where the
    /// drawing has it (for saving changes to them).
    chart_sources: Vec<crate::save::xlsx::ChartSource>,
    /// A sheet's shapes in drawings as read: the shape, drawing part, and cNvPr id.
    xlsx_shape_sources: Vec<crate::save::xlsx::XlsxShapeSource>,
}

/// Text format a container gives before list styles (a shape style's
/// `fontRef`, a table style's `tcTxStyle`).
#[derive(Clone, Copy, Debug, Default)]
struct TextDefaults {
    color: Option<Color>,
    bold: Option<bool>,
    italic: Option<bool>,
}

struct SlideCtx<'d, 'i, 't> {
    colors: Colors<'t>,
    theme: Option<Node<'d, 'i>>,
    slide: Part<'d, 'i>,
    layout: Part<'d, 'i>,
    master: Part<'d, 'i>,
    /// 1-based, for slide number fields.
    number: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Slide,
    Layout,
    Master,
}

/// Maps a group's child coordinates to slide coordinates: scaled and moved
/// (`sx`…`ty`), then turned and flipped by each group that is (`turns`,
/// outermost first; each about its group's centre on the slide).
#[derive(Clone, Copy, Debug)]
struct Transform {
    sx: f64,
    sy: f64,
    tx: f64,
    ty: f64,
    turns: [Option<Turn>; MAX_TURNS],
    groups: [Option<GroupFrame>; MAX_TURNS],
}

/// A group's rotation and flips, about its centre (`cx`, `cy`, EMU).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Turn {
    cx: f64,
    cy: f64,
    rotation: i32,
    flip_h: bool,
    flip_v: bool,
}

/// A group's bounding extents and turn in its parent's coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
struct GroupFrame {
    off: (i64, i64),
    ext: (i64, i64),
    ch_off: (i64, i64),
    ch_ext: (i64, i64),
    rotation: i32,
    flip_h: bool,
    flip_v: bool,
}

/// Whether a shape rotated by `rotation` (60 000ths of a degree) lies on its side: turned 45 to 135
/// degrees, or 225 to 315.
pub(crate) fn turned_sideways(rotation: i32) -> bool {
    (2_700_000..=8_100_000).contains(&rotation.rem_euclid(10_800_000))
}

/// How many turned groups deep a shape's turns are followed.
const MAX_TURNS: usize = 8;

impl Transform {
    const IDENTITY: Self = Self {
        sx: 1.0,
        sy: 1.0,
        tx: 0.0,
        ty: 0.0,
        turns: [None; MAX_TURNS],
        groups: [None; MAX_TURNS],
    };

    /// The turns, outermost first, as the saving needs them.
    fn group_turns(&self) -> Vec<crate::save::pptx::GroupTurn> {
        self.turns
            .iter()
            .flatten()
            .map(|t| crate::save::pptx::GroupTurn { cx: t.cx, cy: t.cy, rotation: t.rotation, flip_h: t.flip_h, flip_v: t.flip_v })
            .collect()
    }

    /// Transform a shape through its hierarchical groups, from innermost to outermost.
    fn apply_groups(
        &self,
        off: (i64, i64),
        ext: (i64, i64),
        mut rotation: i32,
        mut flip_h: bool,
        mut flip_v: bool,
    ) -> (EmuRect, i32, bool, bool) {
        let sideways = turned_sideways(rotation);
        let mut cx = off.0 as f64 + ext.0 as f64 / 2.0;
        let mut cy = off.1 as f64 + ext.1 as f64 / 2.0;
        let mut ux = [1.0f64, 0.0f64];
        let mut uy = [0.0f64, 1.0f64];

        for g in self.groups.iter().flatten().rev() {
            let kx = if g.ch_ext.0 == 0 { 1.0 } else { g.ext.0 as f64 / g.ch_ext.0 as f64 };
            let ky = if g.ch_ext.1 == 0 { 1.0 } else { g.ext.1 as f64 / g.ch_ext.1 as f64 };

            let x1 = (cx - g.ch_off.0 as f64) * kx;
            let y1 = (cy - g.ch_off.1 as f64) * ky;

            let gcx = g.ext.0 as f64 / 2.0;
            let gcy = g.ext.1 as f64 / 2.0;
            let mut dx = x1 - gcx;
            let mut dy = y1 - gcy;
            if g.flip_h {
                dx = -dx;
            }
            if g.flip_v {
                dy = -dy;
            }

            let rad = (g.rotation as f64 / 60_000.0).to_radians();
            let cos_a = rad.cos();
            let sin_a = rad.sin();
            let dx_rot = dx * cos_a - dy * sin_a;
            let dy_rot = dx * sin_a + dy * cos_a;

            cx = dx_rot + gcx + g.off.0 as f64;
            cy = dy_rot + gcy + g.off.1 as f64;

            ux[0] *= kx;
            ux[1] *= ky;
            uy[0] *= kx;
            uy[1] *= ky;

            if g.flip_h {
                ux[0] = -ux[0];
                uy[0] = -uy[0];
            }
            if g.flip_v {
                ux[1] = -ux[1];
                uy[1] = -uy[1];
            }

            let ux_rot = [ux[0] * cos_a - ux[1] * sin_a, ux[0] * sin_a + ux[1] * cos_a];
            let uy_rot = [uy[0] * cos_a - uy[1] * sin_a, uy[0] * sin_a + uy[1] * cos_a];
            ux = ux_rot;
            uy = uy_rot;

            if g.flip_h != g.flip_v {
                rotation = -rotation;
            }
            rotation = (rotation + g.rotation).rem_euclid(21_600_000);
            flip_h ^= g.flip_h;
            flip_v ^= g.flip_v;
        }

        let len_ux = ux[0].hypot(ux[1]);
        let len_uy = uy[0].hypot(uy[1]);
        // A shape turned about a quarter has its turned look stretched: the unturned width takes the
        // vertical scale and the height the horizontal one.
        let (len_w, len_h) = if sideways { (len_uy, len_ux) } else { (len_ux, len_uy) };
        let final_w = (ext.0 as f64 * len_w).round().max(0.0) as i64;
        let final_h = (ext.1 as f64 * len_h).round().max(0.0) as i64;
        let frame = EmuRect::new(
            (cx - final_w as f64 / 2.0).round() as i64,
            (cy - final_h as f64 / 2.0).round() as i64,
            final_w,
            final_h,
        );
        (frame, rotation, flip_h, flip_v)
    }

    /// A shape's frame (already through `rect`), rotation and flips on the
    /// slide, through the turns of its groups (the innermost first): its
    /// centre turns about each group's, a flip mirrors its rotation, and
    /// the group's rotation adds to it.
    fn turn(&self, (mut frame, mut rotation, mut flip_h, mut flip_v): (EmuRect, i32, bool, bool)) -> (EmuRect, i32, bool, bool) {
        for t in self.turns.iter().rev().flatten() {
            let (mut x, mut y) = (frame.x as f64 + frame.width as f64 / 2.0, frame.y as f64 + frame.height as f64 / 2.0);
            if t.flip_h {
                x = 2.0 * t.cx - x;
            }
            if t.flip_v {
                y = 2.0 * t.cy - y;
            }
            let a = (t.rotation as f64 / 60_000.0).to_radians();
            let (dx, dy) = (x - t.cx, y - t.cy);
            let (x, y) = (t.cx + dx * a.cos() - dy * a.sin(), t.cy + dx * a.sin() + dy * a.cos());
            frame.x = (x - frame.width as f64 / 2.0).round() as i64;
            frame.y = (y - frame.height as f64 / 2.0).round() as i64;
            if t.flip_h != t.flip_v {
                rotation = -rotation;
            }
            rotation = (rotation + t.rotation).rem_euclid(21_600_000);
            flip_h ^= t.flip_h;
            flip_v ^= t.flip_v;
        }
        (frame, rotation, flip_h, flip_v)
    }

    fn rect(&self, x: i64, y: i64, w: i64, h: i64) -> EmuRect {
        EmuRect::new(
            (x as f64 * self.sx + self.tx).round() as i64,
            (y as f64 * self.sy + self.ty).round() as i64,
            (w as f64 * self.sx).round().max(0.0) as i64,
            (h as f64 * self.sy).round().max(0.0) as i64,
        )
    }

    /// The transform for the children of a group with this `xfrm`.
    fn group(&self, xfrm: Node) -> Self {
        let (off, ext) = offset_extent(xfrm);
        let ch_off = child(xfrm, "chOff").map(|n| point(n, "x", "y")).unwrap_or(off);
        let ch_ext = child(xfrm, "chExt").map(|n| point(n, "cx", "cy")).unwrap_or(ext);
        let scale = |e: i64, c: i64| if c == 0 { 1.0 } else { e as f64 / c as f64 };
        let (kx, ky) = (scale(ext.0, ch_ext.0), scale(ext.1, ch_ext.1));
        let mut turns = self.turns;
        let rotation = attr_i64(xfrm, "rot").unwrap_or(0) as i32;
        let (flip_h, flip_v) = (attr_bool(xfrm, "flipH").unwrap_or(false), attr_bool(xfrm, "flipV").unwrap_or(false));
        if rotation.rem_euclid(21_600_000) != 0 || flip_h || flip_v {
            // About the group's centre, before the outer groups' turns.
            let frame = self.rect(off.0, off.1, ext.0, ext.1);
            let turn = Turn { cx: frame.x as f64 + frame.width as f64 / 2.0, cy: frame.y as f64 + frame.height as f64 / 2.0, rotation, flip_h, flip_v };
            if let Some(slot) = turns.iter_mut().find(|t| t.is_none()) {
                *slot = Some(turn);
            }
        }
        let mut groups = self.groups;
        if let Some(slot) = groups.iter_mut().find(|g| g.is_none()) {
            *slot = Some(GroupFrame { off, ext, ch_off, ch_ext, rotation, flip_h, flip_v });
        }
        Self {
            sx: self.sx * kx,
            sy: self.sy * ky,
            tx: (off.0 as f64 - ch_off.0 as f64 * kx) * self.sx + self.tx,
            ty: (off.1 as f64 - ch_off.1 as f64 * ky) * self.sy + self.ty,
            turns,
            groups,
        }
    }
}

/// A slide's transition (`p:transition`, or the first choice of the
/// `mc:AlternateContent` PowerPoint 2010 wraps it in).
fn read_transition(root: Node) -> Option<ibis_doc_model::Transition> {
    const P:&str="http://schemas.openxmlformats.org/presentationml/2006/main";
    const MC:&str="http://schemas.openxmlformats.org/markup-compatibility/2006";
    const P14:&str="http://schemas.microsoft.com/office/powerpoint/2010/main";
    let element=namespace_child(root,P,"transition").or_else(|| {
        elements(root).filter(|n|n.has_tag_name((MC,"AlternateContent"))).find_map(|ac| {
            let choice=namespace_child(ac,MC,"Choice").and_then(|c|namespace_child(c,P,"transition"));
            choice.or_else(||namespace_child(ac,MC,"Fallback").and_then(|f|namespace_child(f,P,"transition")))
        })
    })?;
    // 変形 (`p159:morph`, PowerPoint 2016's) is one of its effects.
    const P159:&str="http://schemas.microsoft.com/office/powerpoint/2015/09/main";
    let effect=elements(element).find(|e|matches!(e.tag_name().namespace(),Some(P)|Some(P14)|Some(P159)) && !matches!(e.tag_name().name(),"sndAc"|"extLst"));
    // p14 duration is namespaced; the legacy timing attributes are not.
    let duration=element.attribute((P14,"dur")).and_then(|v|v.parse().ok()).unwrap_or(match unqualified_attribute(element,"spd") {Some("slow")=>1000,Some("med")=>750,_=>500});
    let on_click=match unqualified_attribute(element,"advClick") {Some("0"|"false"|"off")=>false,_=>true};
    let direction = effect.and_then(|e| {
        if e.tag_name().name() == "morph" {
            // How the objects are matched (`option`): by object (the default), word or character.
            unqualified_attribute(e, "option").filter(|o| *o != "byObject").map(str::to_string)
        } else if e.tag_name().name() == "split" {
            let orient = unqualified_attribute(e, "orient").unwrap_or("vert");
            let dir = unqualified_attribute(e, "dir").unwrap_or("out");
            match (orient, dir) {
                ("horz", "in") => Some("horzIn".to_string()),
                ("horz", "out") => Some("horzOut".to_string()),
                ("vert", "in") => Some("in".to_string()),
                ("vert", "out") => Some("out".to_string()),
                (_, "horzIn") => Some("horzIn".to_string()),
                (_, "horzOut") => Some("horzOut".to_string()),
                _ => unqualified_attribute(e, "dir").map(str::to_string),
            }
        } else {
            unqualified_attribute(e, "dir").map(str::to_string)
        }
    });
    let sound = elements(element).find(|e| matches!(e.tag_name().name(), "sndAc")).and_then(|snd_ac| {
        if elements(snd_ac).any(|e| matches!(e.tag_name().name(), "endSnd")) {
            return Some(ibis_doc_model::TransitionSound::Stop);
        }
        let st_snd = elements(snd_ac).find(|e| matches!(e.tag_name().name(), "stSnd"))?;
        let loop_sound = matches!(unqualified_attribute(st_snd, "loop"), Some("1" | "true"));
        let snd = elements(st_snd).find(|e| matches!(e.tag_name().name(), "snd"))?;
        let name = unqualified_attribute(snd, "name").unwrap_or("").to_string();
        if name.is_empty() {
            None
        } else {
            Some(ibis_doc_model::TransitionSound::Play { name, loop_sound })
        }
    });
    Some(ibis_doc_model::Transition {
        effect:effect.map(|e|e.tag_name().name().to_string()).unwrap_or_default(),
        direction,
        duration,on_click,
        after:unqualified_attribute(element,"advTm").and_then(|v|v.trim().parse::<i64>().ok()).map(|v|v.max(0) as u32),
        sound,
    })
}

/// The text of slide `part`'s speaker notes (the body placeholder of its
/// notes page), paragraphs joined by `\n`; empty without notes.
fn read_notes(package: &Package, part: &str) -> String {
    let Ok(Some(notes)) = package.related(part, crate::package::REL_NOTES_SLIDE) else { return String::new() };
    let Ok(text) = package.read_xml(&notes) else { return String::new() };
    let Ok(doc) = Document::parse(&text) else { return String::new() };
    notes_body(doc.root_element()).map(|body| notes_text(body)).unwrap_or_default()
}

/// The `p:txBody` of a notes page's body placeholder.
pub(crate) fn notes_body<'a, 'i>(root: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    let tree = path(root, &["cSld", "spTree"])?;
    elements(tree).filter(|n| n.has_tag_name("sp")).find_map(|sp| {
        let ph = path(sp, &["nvSpPr", "nvPr", "ph"])?;
        (attr(ph, "type") == Some("body")).then(|| child(sp, "txBody")).flatten()
    })
}

/// A text body's paragraphs, joined by `\n` (a line break counts as one too).
pub(crate) fn notes_text(body: Node) -> String {
    children(body, "p")
        .map(|p| {
            elements(p)
                .map(|n| match n.tag_name().name() {
                    "r" | "fld" => child(n, "t").and_then(|t| t.text()).unwrap_or("").to_string(),
                    "br" => "\n".to_string(),
                    _ => String::new(),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn point(node: Node, x: &str, y: &str) -> (i64, i64) {
    (attr_i64(node, x).unwrap_or(0), attr_i64(node, y).unwrap_or(0))
}

/// What a notes page carries in its header, date, footer and number
/// placeholders (`None` when it has none of them).
fn notes_marks_of(root: Node) -> Option<ibis_doc_model::NotesMarks> {
    let tree = path(root, &["cSld", "spTree"])?;
    let mut marks = ibis_doc_model::NotesMarks::default();
    let mut found = false;
    for sp in elements(tree).filter(|n| n.has_tag_name("sp")) {
        let Some(ph) = path(sp, &["nvSpPr", "nvPr", "ph"]) else { continue };
        let kind = attr(ph, "type").unwrap_or("");
        if !matches!(kind, "hdr" | "dt" | "ftr" | "sldNum") {
            continue;
        }
        found = true;
        let body = child(sp, "txBody");
        let text = body.map(notes_text).unwrap_or_default();
        match kind {
            "hdr" => marks.header = Some(text),
            "ftr" => marks.footer = Some(text),
            "sldNum" => marks.number = true,
            _ => {
                let auto = body.is_some_and(|b| b.descendants().any(|n| n.has_tag_name("fld") && attr(n, "type").is_some_and(|t| t.starts_with("datetime"))));
                marks.date = Some(if auto { ibis_doc_model::NotesDate::Auto(text) } else { ibis_doc_model::NotesDate::Fixed(text) });
            }
        }
    }
    found.then_some(marks)
}

/// The frames of the header, date, footer and number placeholders of a notes master.
fn notes_master_frames(root: Node) -> Vec<(String, EmuRect)> {
    let Some(tree) = path(root, &["cSld", "spTree"]) else { return Vec::new() };
    elements(tree)
        .filter(|n| n.has_tag_name("sp"))
        .filter_map(|sp| {
            let kind = attr(path(sp, &["nvSpPr", "nvPr", "ph"])?, "type")?;
            matches!(kind, "hdr" | "dt" | "ftr" | "sldNum").then_some(())?;
            let ((x, y), (w, h)) = offset_extent(path(sp, &["spPr", "xfrm"])?);
            Some((kind.to_string(), EmuRect::new(x, y, w, h)))
        })
        .collect()
}

fn offset_extent(xfrm: Node) -> ((i64, i64), (i64, i64)) {
    let off = child(xfrm, "off").map(|n| point(n, "x", "y")).unwrap_or((0, 0));
    let ext = child(xfrm, "ext").map(|n| point(n, "cx", "cy")).unwrap_or((0, 0));
    (off, ext)
}

/// A placeholder shape of a slide with its counterparts on the layout and master.
struct Inherited<'d, 'i> {
    /// Placeholder type, normalised (`ctrTitle` is `title`, no type is `body`).
    kind: String,
    layout: Option<Node<'d, 'i>>,
    master: Option<Node<'d, 'i>>,
}

impl<'d, 'i> Inherited<'d, 'i> {
    /// Ancestors, nearest first.
    fn ancestors(&self) -> impl Iterator<Item = Node<'d, 'i>> {
        self.layout.into_iter().chain(self.master)
    }
}

fn placeholder_of<'d, 'i>(shape: Node<'d, 'i>) -> Option<Node<'d, 'i>> {
    let nv = elements(shape).find(|n| n.tag_name().name().starts_with("nv"))?;
    path(nv, &["nvPr", "ph"])
}

/// A placeholder as the model names it.
fn placeholder_ref(ph: Node) -> ibis_doc_model::PlaceholderRef {
    ibis_doc_model::PlaceholderRef { kind: attr(ph, "type").map(str::to_string), index: attr(ph, "idx").map(str::to_string) }
}

/// The placeholders of a layout or master, with their own places.
fn layout_placeholders(root: Node) -> Vec<ibis_doc_model::LayoutPlaceholder> {
    placeholders(root)
        .into_iter()
        .filter_map(|node| {
            let ph = placeholder_of(node)?;
            let xfrm = elements(node).find(|n| n.tag_name().name() == "spPr").and_then(|p| child(p, "xfrm")).filter(|x| child(*x, "off").is_some());
            let placement = xfrm.map(|x| {
                let (off, ext) = offset_extent(x);
                let frame = Transform::IDENTITY.rect(off.0, off.1, ext.0, ext.1);
                (frame, attr_i64(x, "rot").unwrap_or(0) as i32, attr_bool(x, "flipH").unwrap_or(false), attr_bool(x, "flipV").unwrap_or(false))
            });
            // Its own prompt (hasCustomPrompt): its text.
            let prompt = attr_bool(ph, "hasCustomPrompt")
                .unwrap_or(false)
                .then(|| {
                    let body = child(node, "txBody")?;
                    let lines: Vec<String> = elements(body).filter(|p| p.tag_name().name() == "p").map(|p| p.descendants().filter(|t| t.is_element() && t.tag_name().name() == "t").filter_map(|t| t.text()).collect()).collect();
                    Some(lines.join("\n"))
                })
                .flatten()
                .filter(|t| !t.trim().is_empty());
            Some(ibis_doc_model::LayoutPlaceholder { placeholder: placeholder_ref(ph), placement, prompt })
        })
        .collect()
}

fn normalize_type(kind: Option<&str>) -> &str {
    match kind {
        Some("ctrTitle") | Some("title") => "title",
        None | Some("obj") | Some("body") => "body",
        Some(other) => other,
    }
}

/// Placeholders in a shape tree, including those inside groups.
fn placeholders<'d, 'i>(root: Node<'d, 'i>) -> Vec<Node<'d, 'i>> {
    let Some(tree) = path(root, &["cSld", "spTree"]) else { return Vec::new() };
    tree.descendants().filter(|n| n.is_element() && matches!(n.tag_name().name(), "sp" | "pic") && placeholder_of(*n).is_some()).collect()
}

/// Find the placeholder matching `ph` in `root`: by index first, then by type.
fn find_placeholder<'d, 'i>(root: Node<'d, 'i>, ph: Node, by_index: bool) -> Option<Node<'d, 'i>> {
    let candidates = placeholders(root);
    let kind = normalize_type(attr(ph, "type"));
    if by_index
        && let Some(idx) = attr(ph, "idx")
        && let Some(found) = candidates.iter().find(|c| placeholder_of(**c).and_then(|p| attr(p, "idx")) == Some(idx))
    {
        return Some(*found);
    }
    let same_type = |c: &&Node| placeholder_of(**c).map(|p| normalize_type(attr(p, "type"))) == Some(kind);
    if let Some(found) = candidates.iter().find(same_type) {
        return Some(*found);
    }
    // Subtitles and content placeholders take the master's body.
    let fallback = match kind {
        "subTitle" | "pic" | "chart" | "tbl" | "dgm" | "media" | "clipArt" => "body",
        _ => return None,
    };
    candidates.into_iter().find(|c| placeholder_of(*c).map(|p| normalize_type(attr(p, "type"))) == Some(fallback))
}

const FILL_ELEMENTS: [&str; 6] = ["noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill"];

fn fill_element<'d, 'i>(parent: Node<'d, 'i>) -> Option<Node<'d, 'i>> {
    elements(parent).find(|n| n.tag_name().namespace()==Some("http://schemas.openxmlformats.org/drawingml/2006/main") && FILL_ELEMENTS.contains(&n.tag_name().name()))
}

struct PresetShadowDefaults {
    blur: i64,
    dist: i64,
    dir: i32,
    sx: i32,
    sy: i32,
    kx: i32,
    ky: i32,
    align: TileAlign,
}

impl<'p, 'd, 'i> Reader<'p, 'd, 'i> {
    fn slide(&mut self, ctx: &SlideCtx<'d, 'i, '_>, id: ibis_doc_model::SlideId) -> Slide {
        let mut slide = Slide::new(id);
        slide.background = self.background(ctx);

        // Layout and master shapes show unless a slide or layout hides them
        // (the slide's are read, not drawn: 背景グラフィックを表示しない).
        let show = |root: Node| attr_bool(root, "showMasterSp").unwrap_or(true);
        slide.hide_backdrop = !show(ctx.slide.root);
        if show(ctx.layout.root) {
            self.tree(ctx, ctx.master.root, &ctx.master.rels, Level::Master, &mut slide.backdrop);
        }
        self.tree(ctx, ctx.layout.root, &ctx.layout.rels, Level::Layout, &mut slide.backdrop);
        self.tree(ctx, ctx.slide.root, &ctx.slide.rels, Level::Slide, &mut slide.shapes);
        for shape in &slide.shapes {
            if let Some(attached) = self.sources.attached.get(&shape.id) {
                slide.attached.insert(shape.id, attached.clone());
            }
        }
        // Group identities are native cNvPr IDs, independent of allocated shape IDs.
        // Keep the hierarchy alongside the already flattened rendering model.
        let paths=slide.shapes.iter().filter_map(|shape|self.sources.shapes.get(&shape.id).filter(|source|source.part==ctx.slide.name).map(|source|(source.path.clone(),shape.id))).collect::<HashMap<_,_>>();
        fn groups(parent:Node,paths:&HashMap<Vec<u32>,ibis_doc_model::ShapeId>)->Vec<ibis_doc_model::GroupMember> {
            const P:&str="http://schemas.openxmlformats.org/presentationml/2006/main";
            const MC:&str="http://schemas.openxmlformats.org/markup-compatibility/2006";
            let mut out=Vec::new();
            for node in elements(parent) {
                if node.has_tag_name((P,"grpSp")) {
                    let properties=namespace_child(node,P,"nvGrpSpPr").and_then(|nv|namespace_child(nv,P,"cNvPr"));
                    let native_id=properties.and_then(|n|unqualified_attribute(n,"id")).unwrap_or("").to_owned();let name=properties.and_then(|n|unqualified_attribute(n,"name")).unwrap_or("").to_owned();
                    let has_unmodelled_children=elements(node).any(|n|n.has_tag_name((MC,"AlternateContent")) || (n.tag_name().namespace()==Some(P)&&matches!(n.tag_name().name(),"sp"|"cxnSp"|"pic"|"graphicFrame"|"contentPart")&&!paths.contains_key(&crate::xml::element_path(n))));
                    out.push(ibis_doc_model::GroupMember::Group(Box::new(ibis_doc_model::ShapeGroup {native_id,name,has_unmodelled_children,children:groups(node,paths)})));
                } else if let Some(&id)=paths.get(&crate::xml::element_path(node)) {out.push(ibis_doc_model::GroupMember::Shape(id));}
                else if node.has_tag_name((MC,"AlternateContent")) {if let Some(branch)=elements(node).find(|n|n.has_tag_name((MC,"Choice"))).or_else(||elements(node).find(|n|n.has_tag_name((MC,"Fallback")))) {out.extend(groups(branch,paths));}}
            }
            out
        }
        if let Some(tree)=namespace_child(ctx.slide.root,"http://schemas.openxmlformats.org/presentationml/2006/main","cSld").and_then(|n|namespace_child(n,"http://schemas.openxmlformats.org/presentationml/2006/main","spTree")) {
            slide.groups=groups(tree,&paths).into_iter().filter_map(|member|match member {ibis_doc_model::GroupMember::Group(group)=>Some(*group),_=>None}).collect();
        }
        slide.placeholders = std::mem::take(&mut self.slide_placeholders);
        slide.shape_names = std::mem::take(&mut self.slide_names);
        slide.shape_alt_texts = std::mem::take(&mut self.slide_alt_texts);
        slide.decorative_shapes = std::mem::take(&mut self.slide_decorative);
        slide.shape_return_to_zoom = std::mem::take(&mut self.slide_return_to_zoom);
        slide.shape_zoom_show_bg = std::mem::take(&mut self.slide_zoom_show_bg);
        slide.shape_zoom_durations = std::mem::take(&mut self.slide_zoom_durations);
        slide.hidden_shapes = std::mem::take(&mut self.slide_hidden);
        slide.shape_links = std::mem::take(&mut self.slide_links);
        slide.shape_link_tooltips = std::mem::take(&mut self.slide_link_tooltips);
        slide.aspect_locked = std::mem::take(&mut self.slide_aspect);
        slide.shape_formulas = std::mem::take(&mut self.slide_formulas);
        slide.hidden = attr_bool(ctx.slide.root, "show") == Some(false);
        slide.name = path(ctx.slide.root, &["cSld"]).and_then(|c| attr(c, "name")).filter(|s| !s.is_empty()).map(str::to_string);
        slide.transition = read_transition(ctx.slide.root);
        for (connector_id, start, end) in std::mem::take(&mut self.slide_glues) {
            let resolve = |cxn: Option<(String, u32)>| -> Option<(u64, u32)> {
                let (native_id, idx) = cxn?;
                let target_id = self.native_ids.get(&native_id)?;
                Some((target_id.0, idx))
            };
            let glue = ibis_doc_model::Glue {
                start: resolve(start),
                end: resolve(end),
            };
            if glue.start.is_some() || glue.end.is_some() {
                slide.shape_glues.insert(connector_id, glue);
            }
        }
        self.last_native_ids = std::mem::take(&mut self.native_ids);
        // An effect on a group is one on each of its shapes.
        fn leaves(group: &ibis_doc_model::ShapeGroup, out: &mut Vec<ibis_doc_model::ShapeId>) {
            for member in &group.children {
                match member {
                    ibis_doc_model::GroupMember::Shape(id) => out.push(*id),
                    ibis_doc_model::GroupMember::Group(inner) => leaves(inner, out),
                }
            }
        }
        fn collect(groups: &[ibis_doc_model::ShapeGroup], out: &mut HashMap<String, Vec<ibis_doc_model::ShapeId>>) {
            for group in groups {
                let mut members = Vec::new();
                leaves(group, &mut members);
                out.insert(group.native_id.clone(), members);
                for member in &group.children {
                    if let ibis_doc_model::GroupMember::Group(inner) = member {
                        collect(std::slice::from_ref(inner), out);
                    }
                }
            }
        }
        let mut group_targets = HashMap::new();
        collect(&slide.groups, &mut group_targets);
        slide.animations = crate::animation::read(ctx.slide.root, &self.last_native_ids, &group_targets);
        read_media_nodes(ctx.slide.root, &self.last_native_ids, &mut slide.shapes);
        slide
    }

    fn background(&mut self, ctx: &SlideCtx<'d, 'i, '_>) -> Fill {
        for part in [&ctx.slide, &ctx.layout, &ctx.master] {
            let Some(bg) = path(part.root, &["cSld", "bg"]) else { continue };
            if let Some(pr) = child(bg, "bgPr")
                && let Some(fill) = fill_element(pr).and_then(|f| self.fill(ctx, f, &part.rels, None, None))
            {
                return fill;
            }
            if let Some(reference) = child(bg, "bgRef") {
                let idx = attr_i64(reference, "idx").unwrap_or(0) as usize;
                if idx > 0 {
                    let color = ctx.colors.in_parent(reference, None);
                    let style = ctx.theme.and_then(|t| bg_fill_style(t, idx));
                    let fill = style.and_then(|s| self.fill(ctx, s, &part.rels, color, None));
                    if let Some(fill) = fill.or(color.map(Fill::Solid)) {
                        return fill;
                    }
                }
            }
        }
        // An absent background inherits the layout/master above, then the
        // default white. A theme fill list is a palette, not an implicit bgRef.
        Fill::Solid(Color::WHITE)
    }

    fn tree(&mut self, ctx: &SlideCtx<'d, 'i, '_>, root: Node<'d, 'i>, rels: &Rels, level: Level, out: &mut Vec<Shape>) {
        let Some(tree) = path(root, &["cSld", "spTree"]) else { return };
        self.shapes(ctx, tree, rels, level, Transform::IDENTITY, None, out);
    }

    #[allow(clippy::too_many_arguments)]
    fn shapes(
        &mut self,
        ctx: &SlideCtx<'d, 'i, '_>,
        parent: Node<'d, 'i>,
        rels: &Rels,
        level: Level,
        transform: Transform,
        group_fill: Option<&Fill>,
        out: &mut Vec<Shape>,
    ) {
        for node in elements(parent) {
            let hidden = elements(node)
                .find(|n| n.tag_name().name().starts_with("nv"))
                .and_then(|nv| child(nv, "cNvPr"))
                .and_then(|c| attr_bool(c, "hidden"))
                .unwrap_or(false);
            // A hidden shape of the slide is read (選択ウィンドウ shows it), not drawn;
            // hidden groups and frames, shapes in groups, and those of layouts
            // and masters, are left out.
            if hidden && !(level == Level::Slide && parent.tag_name().name() == "spTree" && matches!(node.tag_name().name(), "sp" | "cxnSp" | "pic")) {
                continue;
            }
            // Layout and master placeholders are templates, not content.
            if level != Level::Slide && placeholder_of(node).is_some() {
                continue;
            }
            match node.tag_name().name() {
                "sp" | "cxnSp" => {
                    self.text_sources = (level == Level::Slide).then(Vec::new);
                    self.para_sources.clear();
                    let shape = self.shape(ctx, node, rels, level, transform, group_fill);
                    let runs = self.text_sources.take();
                    let paragraphs = std::mem::take(&mut self.para_sources);
                    if let Some(shape) = shape {
                        self.record_source(ctx, level, &shape, node, runs, transform);
                        if let Some(source) = self.sources.shapes.get_mut(&shape.id)
                            && source.runs.is_some()
                        {
                            source.paragraphs = paragraphs;
                        }
                        out.push(shape);
                    }
                }
                "pic" => {
                    if let Some(shape) = self.picture(ctx, node, rels, level, transform) {
                        self.record_source(ctx, level, &shape, node, None, transform);
                        out.push(shape);
                    }
                }
                "graphicFrame" => {
                    if let Some(shape) = self.graphic_frame(ctx, node, rels, transform) {
                        self.record_source(ctx, level, &shape, node, None, transform);
                        if level == Level::Slide && !self.pending.is_empty() {
                            self.sources.attached.insert(shape.id, self.pending.iter().map(|s| s.id).collect());
                        }
                        out.push(shape);
                    }
                    out.append(&mut self.pending);
                }
                "grpSp" => {
                    let pr = child(node, "grpSpPr");
                    let inner = pr.and_then(|p| child(p, "xfrm")).map(|x| transform.group(x)).unwrap_or(transform);
                    let fill = pr.and_then(fill_element).and_then(|f| self.fill(ctx, f, rels, None, group_fill));
                    let fill = fill.as_ref().or(group_fill);
                    self.shapes(ctx, node, rels, level, inner, fill, out);
                }
                "AlternateContent" => {
                    // Ink (`p:contentPart`) is drawn from its strokes; its
                    // fallback picture only if they cannot be read.
                    let ink = child(node, "Choice").and_then(|c| elements(c).find(|n| n.has_tag_name("contentPart")));
                    if let Some(shape) = ink.and_then(|part| self.ink(part, rels, transform)) {
                        out.push(shape);
                        continue;
                    }
                    // Take the first choice: its content is the richer one,
                    // and unknown elements in it are skipped anyway.
                    let choice = child(node, "Choice").filter(|c| !elements(*c).any(|n| n.has_tag_name("contentPart")));
                    let branch = choice.or_else(|| child(node, "Fallback")).or_else(|| child(node, "Choice"));
                    if let Some(branch) = branch {
                        self.shapes(ctx, branch, rels, level, transform, group_fill, out);
                    }
                }
                _ => {}
            }
        }
    }

    /// Remember that slide shape `shape` was read from `node` of the slide
    /// part, with its runs' sources `runs` (see `paragraph`).
    fn record_source(&mut self, ctx: &SlideCtx<'d, 'i, '_>, level: Level, shape: &Shape, node: Node, runs: Option<Vec<Vec<Option<crate::save::pptx::RunSource>>>>, transform: Transform) {
        // Shapes over a chart are read from the chart's drawing part.
        if level != Level::Slide || !std::ptr::eq(node.document(), ctx.slide.root.document()) {
            return;
        }
        let c_nv_pr = node.descendants().find(|n| n.is_element() && n.tag_name().name() == "cNvPr");
        if let Some(native) = c_nv_pr.and_then(|n| n.attribute("id")) {
            self.native_ids.insert(native.to_string(), shape.id);
            self.xlsx_shape_sources.push(crate::save::xlsx::XlsxShapeSource {
                shape: shape.id,
                drawing: ctx.slide.name.clone(),
                c_nv_pr_id: native.to_string(),
            });
        }
        if let Some(name) = c_nv_pr.and_then(|n| n.attribute("name")).filter(|n| !n.is_empty()) {
            self.slide_names.insert(shape.id, name.to_string());
        }
        let descr = c_nv_pr.and_then(|n| n.attribute("descr")).unwrap_or("");
        let title = c_nv_pr.and_then(|n| n.attribute("title")).unwrap_or("");
        if !descr.is_empty() || !title.is_empty() {
            self.slide_alt_texts.insert(shape.id, ibis_doc_model::AltText {
                title: title.to_string(),
                description: descr.to_string(),
            });
        }
        let is_decorative = c_nv_pr.and_then(|n| child(n, "extLst")).is_some_and(|ext_lst| {
            elements(ext_lst).any(|ext| {
                ext.tag_name().name() == "ext"
                    && ext.attribute("uri").is_some_and(|u| u.eq_ignore_ascii_case("{C183D7F6-B498-43B3-948B-1728B52AA6E4}"))
                    && elements(ext).any(|d| d.tag_name().name() == "decorative" && attr_bool(d, "val") != Some(false))
            })
        });
        if is_decorative {
            self.slide_decorative.insert(shape.id);
        }
        let ext_zoom = c_nv_pr.and_then(|n| child(n, "extLst")).and_then(|ext_lst| {
            elements(ext_lst).find(|ext| {
                ext.tag_name().name() == "ext"
                    && ext.attribute("uri").is_some_and(|u| u.eq_ignore_ascii_case("{D3508630-5900-4276-8680-4D969FE36771}"))
            })
        }).and_then(|ext| elements(ext).find(|z| z.tag_name().name() == "zoom"));
        if let Some(z) = ext_zoom {
            if attr_bool(z, "returnToZoom") == Some(true) || attr(z, "returnToZoom") == Some("1") {
                self.slide_return_to_zoom.insert(shape.id);
            }
            if let Some(show_bg) = attr_bool(z, "showBg").or_else(|| attr(z, "showBg").map(|v| v == "1" || v == "true")) {
                self.slide_zoom_show_bg.insert(shape.id, show_bg);
            }
            if let Some(dur) = attr_i64(z, "dur").filter(|&d| d > 0) {
                self.slide_zoom_durations.insert(shape.id, dur as u32);
            }
        }
        if c_nv_pr.and_then(|n| attr_bool(n, "hidden")) == Some(true) {
            self.slide_hidden.insert(shape.id);
        }
        if let Some(h) = c_nv_pr.and_then(|n| child(n, "hlinkClick")) {
            if let Some(link) = link_of(h, &ctx.slide.rels, &self.slide_parts) {
                self.slide_links.insert(shape.id, link);
            }
            if let Some(tooltip) = attr(h, "tooltip").filter(|s| !s.is_empty()) {
                self.slide_link_tooltips.insert(shape.id, tooltip.to_string());
            }
        }
        // 縦横比を固定: `noChangeAspect` of the shape's own locks.
        if let Some(parent) = c_nv_pr.and_then(|n| n.parent())
            && elements(parent).any(|n| n.tag_name().name().starts_with("cNv") && n.tag_name().name() != "cNvPr" && elements(n).any(|l| l.tag_name().name().ends_with("Locks") && attr_bool(l, "noChangeAspect") == Some(true)))
        {
            self.slide_aspect.insert(shape.id);
        }
        // コネクタの接続 (Glue): `cNvCxnSpPr` の `stCxn` / `endCxn`.
        if let Some(parent) = c_nv_pr.and_then(|n| n.parent()) {
            if let Some(cxn_pr) = elements(parent).find(|n| n.tag_name().name() == "cNvCxnSpPr") {
                let parse_cxn = |tag: &str| -> Option<(String, u32)> {
                    let el = elements(cxn_pr).find(|n| n.tag_name().name() == tag)?;
                    let id = attr(el, "id")?.to_string();
                    let idx = attr(el, "idx").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                    Some((id, idx))
                };
                let st = parse_cxn("stCxn");
                let end = parse_cxn("endCxn");
                if st.is_some() || end.is_some() {
                    self.slide_glues.push((shape.id, st, end));
                }
            }
        }
        if let Some(ph) = placeholder_of(node) {
            let own_frame = elements(node).find(|n| n.tag_name().name() == "spPr").and_then(|p| child(p, "xfrm")).is_some_and(|x| child(x, "off").is_some());
            self.slide_placeholders.insert(shape.id, ibis_doc_model::SlidePlaceholder { placeholder: placeholder_ref(ph), own_frame });
        }
        let text = runs.filter(|_| shape.text_body().is_some());
        let group = (transform.sx != 1.0 || transform.sy != 1.0 || transform.tx != 0.0 || transform.ty != 0.0).then_some([transform.sx, transform.sy, transform.tx, transform.ty]);
        let source = crate::save::pptx::ShapeSource { part: ctx.slide.name.clone(), path: crate::xml::element_path(node), runs: text, group, paragraphs: Vec::new(), turns: transform.group_turns() };
        self.sources.shapes.insert(shape.id, source);
    }

    fn inherited(&self, ctx: &SlideCtx<'d, 'i, '_>, node: Node<'d, 'i>, level: Level) -> Option<Inherited<'d, 'i>> {
        if level != Level::Slide {
            return None;
        }
        let ph = placeholder_of(node)?;
        let layout = find_placeholder(ctx.layout.root, ph, true);
        // The master is matched by the layout placeholder's type when known.
        let master_key = layout.and_then(placeholder_of).unwrap_or(ph);
        let master = find_placeholder(ctx.master.root, master_key, false);
        Some(Inherited { kind: normalize_type(attr(ph, "type")).to_string(), layout, master })
    }

    /// Position, rotation and flips from `xfrm`, falling back to ancestors.
    fn placement(&self, xfrm: Option<Node>, inherited: Option<&Inherited<'d, 'i>>, transform: Transform) -> Option<(EmuRect, i32, bool, bool)> {
        let xfrm = xfrm
            .filter(|x| child(*x, "off").is_some())
            .or_else(|| inherited.and_then(|i| i.ancestors().find_map(|a| elements(a).find_map(|pr| child(pr, "xfrm")))))?;
        let (off, ext) = offset_extent(xfrm);
        let rotation = attr_i64(xfrm, "rot").unwrap_or(0) as i32;
        let flip_h = attr_bool(xfrm, "flipH").unwrap_or(false);
        let flip_v = attr_bool(xfrm, "flipV").unwrap_or(false);
        if transform.groups.iter().flatten().count() > 1 {
            Some(transform.apply_groups(off, ext, rotation, flip_h, flip_v))
        } else {
            let mut frame = transform.rect(off.0, off.1, ext.0, ext.1);
            if turned_sideways(rotation) {
                // See `apply_groups`: the turned look is what the group stretches.
                let (cx, cy) = (frame.x as f64 + frame.width as f64 / 2.0, frame.y as f64 + frame.height as f64 / 2.0);
                let (w, h) = ((ext.0 as f64 * transform.sy).round().max(0.0) as i64, (ext.1 as f64 * transform.sx).round().max(0.0) as i64);
                frame = EmuRect::new((cx - w as f64 / 2.0).round() as i64, (cy - h as f64 / 2.0).round() as i64, w, h);
            }
            Some(transform.turn((frame, rotation, flip_h, flip_v)))
        }
    }

    fn preset_shadow_defaults(prst: &str) -> PresetShadowDefaults {
        let (blur, dist, dir) = match prst {
            "shdw1" => (0, 76200, 18_900_000),
            "shdw2" => (0, 76200, 2_700_000),
            "shdw3" => (0, 76200, 13_500_000),
            "shdw4" => (0, 76200, 8_100_000),
            "shdw5" => (0, 76200, 5_400_000),
            "shdw6" => (0, 76200, 16_200_000),
            "shdw7" => (0, 76200, 0),
            "shdw8" => (0, 76200, 10_800_000),
            "shdw9" => (0, 76200, 18_900_000),
            "shdw10" => (0, 76200, 2_700_000),
            "shdw11" => (0, 76200, 13_500_000),
            "shdw12" => (0, 76200, 8_100_000),
            "shdw13" => (0, 76200, 18_900_000),
            "shdw14" => (0, 76200, 2_700_000),
            "shdw15" => (0, 76200, 13_500_000),
            "shdw16" => (0, 76200, 8_100_000),
            "shdw17" => (0, 76200, 5_400_000),
            "shdw18" => (0, 76200, 16_200_000),
            "shdw19" => (0, 76200, 2_700_000),
            "shdw20" => (0, 76200, 18_900_000),
            _ => (0, 76200, 2_700_000),
        };
        let (sx, sy, kx, ky, align) = match prst {
            "shdw3" => (100_000, 50_000, 2_453_400, 0, TileAlign::Bottom),
            "shdw4" => (100_000, 50_000, -2_453_400, 0, TileAlign::Bottom),
            "shdw7" => (100_000, -50_000, 2_453_400, 0, TileAlign::Bottom),
            "shdw8" => (100_000, -50_000, -2_453_400, 0, TileAlign::Bottom),
            "shdw9" => (75_000, 75_000, 0, 0, TileAlign::TopLeft),
            "shdw10" => (125_000, 125_000, 0, 0, TileAlign::BottomRight),
            "shdw11" => (100_000, 50_000, 2_453_400, 0, TileAlign::Bottom),
            "shdw12" => (100_000, 50_000, -2_453_400, 0, TileAlign::Bottom),
            "shdw15" => (100_000, -50_000, 2_453_400, 0, TileAlign::Bottom),
            "shdw16" => (100_000, -50_000, -2_453_400, 0, TileAlign::Bottom),
            "shdw17" | "shdw18" => (100_000, 100_000, 0, 0, TileAlign::Center),
            "shdw19" => (100_000, 50_000, 0, 0, TileAlign::Bottom),
            "shdw20" => (100_000, -100_000, 0, 0, TileAlign::Bottom),
            _ => (100_000, 100_000, 0, 0, TileAlign::Bottom),
        };
        PresetShadowDefaults { blur, dist, dir, sx, sy, kx, ky, align }
    }

    fn shape(
        &mut self,
        ctx: &SlideCtx<'d, 'i, '_>,
        node: Node<'d, 'i>,
        rels: &Rels,
        level: Level,
        transform: Transform,
        group_fill: Option<&Fill>,
    ) -> Option<Shape> {
        let inherited = self.inherited(ctx, node, level);
        let sp_pr = child(node, "spPr");
        let (frame, rotation, flip_h, flip_v) =
            self.placement(sp_pr.and_then(|p| child(p, "xfrm")), inherited.as_ref(), transform)?;
        let style = child(node, "style");
        let chain = || sp_pr.into_iter().chain(inherited.iter().flat_map(|i| i.ancestors().filter_map(|a| child(a, "spPr"))));

        let geometry_node = chain().find_map(|p| child(p, "prstGeom").or_else(|| child(p, "custGeom")));
        let preset = geometry_node.filter(|g| g.has_tag_name("prstGeom")).and_then(|g| attr(g, "prst"));
        // Straight lines are drawn as lines (with arrowheads); bent and
        // curved connectors by their geometry.
        let is_line = matches!(preset, Some("line" | "straightConnector1"))
            || (node.has_tag_name("cxnSp") && geometry_node.is_none());

        let ln = chain().find_map(|p| child(p, "ln"));
        let stroke = self.stroke(ctx, ln, style);
        let id = self.presentation.new_shape_id();
        if let Some(f_node) = child(node, "f") {
            if let Some(f_text) = f_node.text().map(str::trim).filter(|s| !s.is_empty()) {
                self.slide_formulas.insert(id, f_text.to_string());
            }
        }
        let shape = |kind| Shape { id, frame, rotation, flip_h, flip_v, kind, floating: None };

        let tx_body = child(node, "txBody").or_else(||path(node,&["txSp","txBody"]));
        let has_text = tx_body.is_some_and(|b| {
            b.descendants().any(|n| n.has_tag_name("t") && !n.text().unwrap_or("").is_empty())
        });

        if is_line && !has_text {
            return stroke.map(|s| shape(ShapeKind::Line(s)));
        }

        // `useBgFill`: the shape takes the fill of the slide's background
        // (ECMA-376 19.3.1.43), so it shows the background through itself.
        let use_bg_fill = unqualified_attribute(node, "useBgFill").is_some_and(|v| v == "1" || v == "true");
        let mut fill = match chain().find_map(fill_element).filter(|_| !use_bg_fill) {
            Some(f) => {
                let mut parsed = self.fill(ctx, f, rels, None, group_fill).unwrap_or_default();
                if f.tag_name().name() == "gradFill" && unqualified_attribute(f, "rotWithShape").map_or(false, |v| v == "0" || v == "false") {
                    match parsed {
                        Fill::Linear { angle, scaled, stops } => {
                            let mut a = angle - rotation;
                            if flip_v {
                                a = -a;
                            }
                            if flip_h {
                                a = 10_800_000 - a;
                            }
                            parsed = Fill::Linear { angle: a.rem_euclid(21_600_000), scaled, stops };
                        }
                        Fill::TileLinear { angle, scaled, stops, tile, flip } => {
                            let mut a = angle - rotation;
                            if flip_v {
                                a = -a;
                            }
                            if flip_h {
                                a = 10_800_000 - a;
                            }
                            parsed = Fill::TileLinear { angle: a.rem_euclid(21_600_000), scaled, stops, tile, flip };
                        }
                        _ => {}
                    }
                }
                parsed
            }
            None if use_bg_fill => self.background(ctx),
            None => self.style_fill(ctx, style, rels),
        };
        if is_line {
            fill = Fill::None;
        }
        let defaults = TextDefaults {
            color: style.and_then(|s| child(s, "fontRef")).and_then(|r| ctx.colors.in_parent(r, None)),
            ..TextDefaults::default()
        };
        let text = child(node, "txBody").or_else(||path(node,&["txSp","txBody"])).map(|body| self.text_body(ctx, body, defaults, inherited.as_ref(), false));
        // An empty placeholder keeps its empty text (and the style it takes
        // from its layout) for typing into; it draws nothing, and the editor
        // shows its prompt (SlidePlaceholder::prompt_text).
        let text = text.filter(|t| inherited.is_some() || !t.is_empty());
        let geometry = if is_line && geometry_node.is_none() {
            crate::geometry::preset("straightConnector1").unwrap_or(ibis_doc_model::Geometry::Rect)
        } else {
            crate::geometry::geometry(geometry_node)
        };
        let tilt = sp_pr.and_then(|p| tilt(p, &ctx.colors));
        let effect_ref = style.and_then(|s| child(s, "effectRef"));
        let effect_idx = effect_ref.and_then(|r| attr_i64(r, "idx")).unwrap_or(0).max(0) as usize;
        let effect_ref_color = effect_ref.and_then(|r| ctx.colors.in_parent(r, None));
        let themed_effect_style = if effect_idx > 0 { ctx.theme.and_then(|t| effect_style(t, effect_idx)) } else { None };
        let has_effect_dag = chain().any(|p| drawing_child(p, "effectDag").is_some());
        let effect_lst = chain()
            .find_map(|p| drawing_child(p, "effectLst"))
            .or_else(|| {
                (!has_effect_dag)
                    .then(|| themed_effect_style.and_then(|s| drawing_child(s, "effectLst")))
                    .flatten()
            });
        let sp3d_node = chain()
            .find_map(|p| drawing_child(p, "sp3d"))
            .or_else(|| themed_effect_style.and_then(|s| drawing_child(s, "sp3d")));
        let shadow = effect_lst
            .and_then(|e| drawing_child(e, "outerShdw").or_else(|| drawing_child(e, "prstShdw")))
            .and_then(|s| {
                let is_prst = s.tag_name().name() == "prstShdw";
                let prst_defaults = if is_prst {
                    Self::preset_shadow_defaults(attr(s, "prst").unwrap_or(""))
                } else {
                    PresetShadowDefaults {
                        blur: 0,
                        dist: 0,
                        dir: 0,
                        sx: 100_000,
                        sy: 100_000,
                        kx: 0,
                        ky: 0,
                        align: TileAlign::Bottom,
                    }
                };
                let align = attr(s, "algn").map(TileAlign::parse).unwrap_or(prst_defaults.align);
                let sx = attr_i64(s, "sx").map(|v| v as i32).unwrap_or(prst_defaults.sx);
                let sy = attr_i64(s, "sy").map(|v| v as i32).unwrap_or(prst_defaults.sy);
                let kx = attr_i64(s, "kx").map(|v| v as i32).unwrap_or(prst_defaults.kx);
                let ky = attr_i64(s, "ky").map(|v| v as i32).unwrap_or(prst_defaults.ky);
                let rot_with_shape = attr(s, "rotWithShape").map(|v| v != "0").unwrap_or(true);
                Some(ibis_doc_model::Shadow {
                    color: ctx.colors.in_parent(s, effect_ref_color)?,
                    blur: attr_i64(s, "blurRad").unwrap_or(prst_defaults.blur),
                    distance: attr_i64(s, "dist").unwrap_or(prst_defaults.dist),
                    direction: attr_i64(s, "dir").unwrap_or(prst_defaults.dir as i64) as i32,
                    sx,
                    sy,
                    kx,
                    ky,
                    align,
                    rot_with_shape,
                })
            });
        let glow = effect_lst
            .and_then(|e| drawing_child(e, "glow"))
            .and_then(|g| Some((ctx.colors.in_parent(g, effect_ref_color)?, attr_i64(g, "rad").unwrap_or(0))));
        let inner_shadow = effect_lst
            .and_then(|e| drawing_child(e, "innerShdw"))
            .and_then(|s| {
                Some(ibis_doc_model::Shadow {
                    color: ctx.colors.in_parent(s, effect_ref_color)?,
                    blur: attr_i64(s, "blurRad").unwrap_or(0),
                    distance: attr_i64(s, "dist").unwrap_or(0),
                    direction: attr_i64(s, "dir").unwrap_or(0) as i32,
                    sx: 100_000,
                    sy: 100_000,
                    kx: 0,
                    ky: 0,
                    align: TileAlign::Bottom,
                    rot_with_shape: true,
                })
            });
        let soft_edge = effect_lst
            .and_then(|e| drawing_child(e, "softEdge"))
            .and_then(|e| attr_i64(e, "rad"))
            .filter(|&r| r > 0);
        // Defaults: ECMA-376 Part 4 第 5 版 dml-main.xsd の CT_ReflectionEffect.
        let reflection = effect_lst
            .and_then(|e| drawing_child(e, "reflection"))
            .map(|e| ibis_doc_model::Reflection {
                blur: attr_i64(e, "blurRad").unwrap_or(0),
                start_alpha: attr_i64(e, "stA").unwrap_or(100_000) as i32,
                end_alpha: attr_i64(e, "endA").unwrap_or(0) as i32,
                start_pos: attr_i64(e, "stPos").unwrap_or(0) as i32,
                end_pos: attr_i64(e, "endPos").unwrap_or(100_000) as i32,
                distance: attr_i64(e, "dist").unwrap_or(0),
                direction: attr_i64(e, "dir").unwrap_or(0) as i32,
                fade_direction: attr_i64(e, "fadeDir").unwrap_or(5_400_000) as i32,
                sx: attr_i64(e, "sx").unwrap_or(100_000) as i32,
                sy: attr_i64(e, "sy").unwrap_or(100_000) as i32,
                kx: attr_i64(e, "kx").unwrap_or(0) as i32,
                ky: attr_i64(e, "ky").unwrap_or(0) as i32,
                align: attr(e, "algn").map(TileAlign::parse).unwrap_or(TileAlign::Bottom),
                rot_with_shape: attr(e, "rotWithShape").is_none_or(|v| v != "0" && v != "false"),
            });
        let bevel = sp3d_node
            .and_then(|s| drawing_child(s, "bevelT"))
            .map(|b| {
                let preset = attr(b, "prst").and_then(ibis_doc_model::BevelPreset::parse).unwrap_or_default();
                let width = attr_i64(b, "w").unwrap_or(ibis_doc_model::Bevel::DEFAULT_SIZE);
                let height = attr_i64(b, "h").unwrap_or(ibis_doc_model::Bevel::DEFAULT_SIZE);
                ibis_doc_model::Bevel { preset, width, height }
            });
        Some(shape(ShapeKind::Auto(AutoShape { geometry, fill, stroke, text, tilt, shadow, glow, inner_shadow, soft_edge, reflection, bevel })))
    }

fn parse_artistic_effect(blip: Node) -> Option<ibis_doc_model::ArtisticEffect> {
    use ibis_doc_model::ArtisticEffect;
    let img_effect = blip.descendants().find(|n| n.is_element() && n.tag_name().name() == "imgEffect")?;
    for child in img_effect.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name();
        let attr_i32 = |name: &str| child.attribute(name).and_then(|v| v.parse::<i32>().ok());
        match tag {
            "artisticBlur" => {
                let radius = attr_i32("radius").unwrap_or(10);
                return Some(ArtisticEffect::Blur { radius });
            }
            "artisticCement" => {
                let crack_spacing = attr_i32("crackSpacing").unwrap_or(24);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::Cement { crack_spacing, trans });
            }
            "artisticChalkSketch" => {
                let pressure = attr_i32("pressure").unwrap_or(24);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::ChalkSketch { pressure, trans });
            }
            "artisticCrisscrossEtching" => {
                let pressure = attr_i32("pressure").unwrap_or(24);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::CrisscrossEtching { pressure, trans });
            }
            "artisticCutout" => {
                let number_of_shades = attr_i32("numberOfShades").unwrap_or(6);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::Cutout { number_of_shades, trans });
            }
            "artisticFilmGrain" => {
                let grain_size = attr_i32("grainSize").unwrap_or(40);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::FilmGrain { grain_size, trans });
            }
            "artisticGlass" => {
                let scaling = attr_i32("scaling").unwrap_or(25);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::Glass { scaling, trans });
            }
            "artisticGlowDiffused" => {
                let intensity = attr_i32("intensity").unwrap_or(5);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::GlowDiffused { intensity, trans });
            }
            "artisticGlowEdges" => {
                let smoothness = attr_i32("smoothness").unwrap_or(10);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::GlowEdges { smoothness, trans });
            }
            "artisticLightScreen" => {
                let grid_size = attr_i32("gridSize").unwrap_or(4);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::LightScreen { grid_size, trans });
            }
            "artisticLineDrawing" => {
                let pencil_size = attr_i32("pencilSize").unwrap_or(2);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::LineDrawing { pencil_size, trans });
            }
            "artisticMarker" => {
                let size = attr_i32("size").unwrap_or(15);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::Marker { size, trans });
            }
            "artisticMosiaicBubbles" | "artisticMosaicBubbles" => {
                let pressure = attr_i32("pressure").unwrap_or(15);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::MosaicBubbles { pressure, trans });
            }
            "artisticPaintBrush" => {
                let brush_size = attr_i32("brushSize").unwrap_or(7);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::PaintBrush { brush_size, trans });
            }
            "artisticPaintStrokes" => {
                let intensity = attr_i32("intensity").unwrap_or(5);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::PaintStrokes { intensity, trans });
            }
            "artisticPastelsSmooth" => {
                let scaling = attr_i32("scaling").unwrap_or(24);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::PastelsSmooth { scaling, trans });
            }
            "artisticPencilGrayscale" => {
                let pencil_size = attr_i32("pencilSize").unwrap_or(20);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::PencilGrayscale { pencil_size, trans });
            }
            "artisticPencilSketch" => {
                let pressure = attr_i32("pressure").unwrap_or(24);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::PencilSketch { pressure, trans });
            }
            "artisticPhotocopy" => {
                let detail = attr_i32("detail").unwrap_or(3);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::Photocopy { detail, trans });
            }
            "artisticPlasticWrap" => {
                let smoothness = attr_i32("smoothness").unwrap_or(24);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::PlasticWrap { smoothness, trans });
            }
            "artisticTexturizer" => {
                let scaling = attr_i32("scaling").unwrap_or(50);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::Texturizer { scaling, trans });
            }
            "artisticWatercolorSponge" => {
                let brush_size = attr_i32("brushSize").unwrap_or(9);
                let trans = attr_i32("trans").unwrap_or(0);
                return Some(ArtisticEffect::WatercolorSponge { brush_size, trans });
            }
            _ => {}
        }
    }
    None
}

    fn picture(&mut self, ctx: &SlideCtx<'d, 'i, '_>, node: Node<'d, 'i>, rels: &Rels, level: Level, transform: Transform) -> Option<Shape> {
        let inherited = self.inherited(ctx, node, level);
        let xfrm = path(node, &["spPr", "xfrm"]);
        // A picture in a sheet's drawing may have no `a:xfrm` (other makers leave it out): the anchor's box is its frame.
        let anchored = if xfrm.is_none_or(|x| child(x, "off").is_none()) { self.anchor.take() } else { None };
        let (frame, rotation, flip_h, flip_v) = match anchored {
            Some(rect) => (rect, 0, false, false),
            None => self.placement(xfrm, inherited.as_ref(), transform)?,
        };
        let blip_fill = child(node, "blipFill")?;
        let embed = child(blip_fill, "blip").and_then(blip_target)?;
        let (media, format) = self.media(rels, embed)?;
        let kind = if drawable(format) {
            let crop = child(blip_fill, "srcRect")
                .map(|r| {
                    let v = |name| attr_i64(r, name).unwrap_or(0) as i32;
                    Crop { left: v("l"), top: v("t"), right: v("r"), bottom: v("b") }
                })
                .unwrap_or_default();
            let sp_pr = child(node, "spPr");
            let stroke = sp_pr.and_then(|p| child(p, "ln")).and_then(|ln| self.stroke(ctx, Some(ln), None));
            let picture_geometry = crate::geometry::geometry(sp_pr.and_then(|p| child(p, "custGeom").or_else(||child(p, "prstGeom"))));
            let corner = match picture_geometry {
                ibis_doc_model::Geometry::RoundRect { radius } => radius,
                _ => 0,
            };
            // Cut to a shape other than a rectangle (Office's "Crop to Shape").
            let geometry = (!matches!(picture_geometry, ibis_doc_model::Geometry::Rect | ibis_doc_model::Geometry::RoundRect { .. })).then_some(picture_geometry);
            let effect_lst = sp_pr.and_then(|p| drawing_child(p, "effectLst"));
            let shadow = effect_lst.and_then(|e| drawing_child(e, "outerShdw")).and_then(|s| {
                Some(ibis_doc_model::Shadow {
                    color: ctx.colors.in_parent(s, None)?,
                    blur: attr_i64(s, "blurRad").unwrap_or(0),
                    distance: attr_i64(s, "dist").unwrap_or(0),
                    direction: attr_i64(s, "dir").unwrap_or(0) as i32,
                    sx: attr_i64(s, "sx").unwrap_or(100_000) as i32,
                    sy: attr_i64(s, "sy").unwrap_or(100_000) as i32,
                    kx: attr_i64(s, "kx").unwrap_or(0) as i32,
                    ky: attr_i64(s, "ky").unwrap_or(0) as i32,
                    align: attr(s, "algn").map(TileAlign::parse).unwrap_or(TileAlign::Bottom),
                    rot_with_shape: attr(s, "rotWithShape").is_none_or(|v| v != "0" && v != "false"),
                })
            });
            let reflection = effect_lst.and_then(|e| drawing_child(e, "reflection")).map(|e| ibis_doc_model::Reflection {
                blur: attr_i64(e, "blurRad").unwrap_or(0),
                start_alpha: attr_i64(e, "stA").unwrap_or(100_000) as i32,
                end_alpha: attr_i64(e, "endA").unwrap_or(0) as i32,
                start_pos: attr_i64(e, "stPos").unwrap_or(0) as i32,
                end_pos: attr_i64(e, "endPos").unwrap_or(100_000) as i32,
                distance: attr_i64(e, "dist").unwrap_or(0),
                direction: attr_i64(e, "dir").unwrap_or(0) as i32,
                fade_direction: attr_i64(e, "fadeDir").unwrap_or(5_400_000) as i32,
                sx: attr_i64(e, "sx").unwrap_or(100_000) as i32,
                sy: attr_i64(e, "sy").unwrap_or(100_000) as i32,
                kx: attr_i64(e, "kx").unwrap_or(0) as i32,
                ky: attr_i64(e, "ky").unwrap_or(0) as i32,
                align: attr(e, "algn").map(TileAlign::parse).unwrap_or(TileAlign::Bottom),
                rot_with_shape: attr(e, "rotWithShape").is_none_or(|v| v != "0" && v != "false"),
            });
            let soft_edge = effect_lst.and_then(|e| drawing_child(e, "softEdge")).and_then(|e| attr_i64(e, "rad")).filter(|&r| r > 0);
            let duotone = child(blip_fill, "blip").and_then(|b| drawing_child(b, "duotone")).and_then(|d| {
                let colors: Vec<_> = elements(d).filter_map(|c| ctx.colors.resolve(c, None)).collect();
                Some((colors.first()?.rgb, colors.get(1)?.rgb))
            });
            // A colour changed to one with no opacity is made clear.
            let clear = child(blip_fill, "blip").and_then(|b| drawing_child(b, "clrChange")).and_then(|c| {
                let to = child(c, "clrTo").and_then(|t| elements(t).next())?;
                let clear = child(to, "alpha").and_then(|a| attr_i64(a, "val")).is_some_and(|v| v == 0);
                let from = child(c, "clrFrom").and_then(|f| ctx.colors.in_parent(f, None))?;
                clear.then_some(from.rgb)
            });
            let luminance = child(blip_fill, "blip").and_then(|b| drawing_child(b, "lum")).and_then(|l| {
                let bright = attr_i64(l, "bright").unwrap_or(0).clamp(-100_000, 100_000) as i32;
                let contrast = attr_i64(l, "contrast").unwrap_or(0).clamp(-100_000, 100_000) as i32;
                (bright != 0 || contrast != 0).then_some((bright, contrast))
            });
            let tile = child(blip_fill, "tile").map(|t| ibis_doc_model::PictureTile {
                tx: attr_i64(t, "tx").unwrap_or(0),
                ty: attr_i64(t, "ty").unwrap_or(0),
                sx: attr_i64(t, "sx").unwrap_or(100_000) as i32,
                sy: attr_i64(t, "sy").unwrap_or(100_000) as i32,
                flip: ibis_doc_model::TileFlip::parse(attr(t, "flip").unwrap_or("none")),
                align: ibis_doc_model::TileAlign::parse(attr(t, "algn").unwrap_or("tl")),
            });
            let fill_rect = child(blip_fill, "stretch").and_then(|s| child(s, "fillRect")).map(|r| ["l", "t", "r", "b"].map(|k| attr_i64(r, k).unwrap_or(0) as i32)).filter(|r| *r != [0, 0, 0, 0]);
            // The other colour effects: grey, black and white, one colour for all, a colour changed, the opacity.
            let mut effects = child(blip_fill, "blip").map_or_else(ibis_doc_model::BlipEffects::default, |b| {
                let mut e = ibis_doc_model::BlipEffects::default();
                e.grayscale = drawing_child(b, "grayscl").is_some();
                e.bilevel = drawing_child(b, "biLevel").map(|l| attr_i64(l, "thresh").unwrap_or(50_000).clamp(0, 100_000) as i32);
                e.replace = drawing_child(b, "clrRepl").and_then(|r| elements(r).next()).and_then(|c| ctx.colors.resolve(c, None)).map(|c| c.rgb);
                e.change = drawing_child(b, "clrChange").and_then(|c| {
                    let to = child(c, "clrTo").and_then(|t| elements(t).next())?;
                    let transparent = child(to, "alpha").and_then(|a| attr_i64(a, "val")).is_some_and(|v| v == 0);
                    if transparent {
                        return None;
                    }
                    let from = child(c, "clrFrom").and_then(|f| ctx.colors.in_parent(f, None))?;
                    Some((from.rgb, ctx.colors.resolve(to, None)?.rgb))
                });
                e.opacity = drawing_child(b, "alphaModFix")
                    .map(|a| attr_i64(a, "amt").unwrap_or(100_000))
                    .or_else(|| drawing_child(b, "alphaRepl").and_then(|a| attr_i64(a, "a")))
                    .map(|v| v.clamp(0, 100_000) as i32)
                    .filter(|v| *v != 100_000);
                e
            });
            effects.background = sp_pr
                .and_then(fill_element)
                .and_then(|f| self.fill(ctx, f, rels, None, None))
                .or_else(|| Some(self.style_fill(ctx, child(node, "style"), rels)))
                .and_then(|f| match f {
                    Fill::Solid(c) => Some(c),
                    _ => None,
                });
            // A video or audio clip: the file a `p14:media` embeds.
            let clip = child(node, "nvPicPr").and_then(|n| child(n, "nvPr")).and_then(|nv| {
                let media_elem = nv.descendants().find(|n| n.is_element() && n.tag_name().name() == "media");
                // The file embedded, or (ファイルへのリンク) one linked: `a:videoFile`
                // or `a:audioFile` related to a file outside the presentation.
                let embedded = media_elem.and_then(blip_target).and_then(|embed| self.media(rels, embed));
                let (clip, format, link) = match embedded {
                    Some((clip, format)) => (clip, format, None),
                    None => {
                        let file = nv.descendants().find(|n| n.is_element() && matches!(n.tag_name().name(), "videoFile" | "audioFile" | "wavAudioFile"))?;
                        let key=attr(file,"link").or_else(||attr(file,"embed"))?;
                        let rel=rels.get(key)?;
                        if rel.external {
                            let format=MediaFormat::from_path(&rel.target);
                            (self.presentation.add_media(format,Vec::new()),format,Some(rel.target.clone()))
                        } else {
                            let (media,format)=self.media(rels,key)?;
                            (media,format,None)
                        }
                    }
                };
                if !format.is_video() && !format.is_audio() {
                    return None;
                }
                let mut options = ibis_doc_model::ClipOptions::default();
                if let Some(m) = media_elem {
                    if let Some(vol_str) = m.attribute("vol") {
                        if let Ok(vol) = vol_str.parse::<u32>() {
                            options.volume = if vol == 0 {
                                ibis_doc_model::MediaVolume::Mute
                            } else if vol <= 45000 {
                                ibis_doc_model::MediaVolume::Low
                            } else if vol <= 80000 {
                                ibis_doc_model::MediaVolume::Medium
                            } else {
                                ibis_doc_model::MediaVolume::High
                            };
                        }
                    }
                    if m.attribute("loop") == Some("1") || m.attribute("loop") == Some("true") {
                        options.loop_until_stopped = true;
                    }
                    if m.attribute("rewind") == Some("1") || m.attribute("rewind") == Some("true") {
                        options.rewind = true;
                    }
                    if m.attribute("autoPlay") == Some("1") || m.attribute("autoPlay") == Some("true") {
                        options.start = ibis_doc_model::MediaStart::Automatically;
                    }
                    if m.attribute("fullScreen") == Some("1") || m.attribute("fullScreen") == Some("true") {
                        options.full_screen = true;
                    }
                    if m.attribute("showWhenStopped") == Some("0") || m.attribute("showWhenStopped") == Some("false") {
                        options.hide_while_not_playing = true;
                    }
                    if m.attribute("isNarration") == Some("1") || m.attribute("isNarration") == Some("true") {
                        options.hide_during_show = true;
                    }
                    if m.attribute("playAcrossSlides") == Some("1") || m.attribute("playAcrossSlides") == Some("true") {
                        options.play_across_slides = true;
                    }
                    if let Some(trim) = m.descendants().find(|n| n.is_element() && n.tag_name().name() == "trim") {
                        if let Some(st) = trim.attribute("st").and_then(|v| v.parse().ok()) {
                            options.trim_start_ms = st;
                        }
                        if let Some(end) = trim.attribute("end").and_then(|v| v.parse().ok()) {
                            options.trim_end_ms = end;
                        }
                    }
                    if let Some(fade) = m.descendants().find(|n| n.is_element() && n.tag_name().name() == "fade") {
                        if let Some(in_dur) = fade.attribute("in").and_then(|v| v.parse().ok()) {
                            options.fade_in_ms = in_dur;
                        }
                        if let Some(out_dur) = fade.attribute("out").and_then(|v| v.parse().ok()) {
                            options.fade_out_ms = out_dur;
                        }
                    }
                    if let Some(bmk_lst) = m.descendants().find(|n| n.is_element() && n.tag_name().name() == "bmkLst") {
                        for bmk in bmk_lst.children().filter(|n| n.is_element() && n.tag_name().name() == "bmk") {
                            let name = bmk.attribute("name").unwrap_or("").to_string();
                            let time_ms = bmk.attribute("time").and_then(|v| v.parse().ok()).unwrap_or(0);
                            if !name.is_empty() || time_ms > 0 {
                                options.bookmarks.push(ibis_doc_model::MediaBookmark { name, time_ms });
                            }
                        }
                        options.bookmarks.sort_by_key(|b| b.time_ms);
                    }
                    if let Some(captions_node) = m.descendants().find(|n| n.is_element() && (n.tag_name().name() == "captionTracks" || n.tag_name().name() == "captions")) {
                        for track in captions_node.children().filter(|n| n.is_element() && (n.tag_name().name() == "captionTrack" || n.tag_name().name() == "caption")) {
                            let label = track.attribute("label").unwrap_or("").to_string();
                            let lang = track.attribute("lang").unwrap_or("").to_string();
                            let vtt = track.attribute("vtt").or_else(|| track.text()).unwrap_or("").to_string();
                            if !label.is_empty() || !lang.is_empty() || !vtt.is_empty() {
                                options.captions.push(ibis_doc_model::MediaCaptionTrack { label, lang, vtt });
                            }
                        }
                    }
                }
                Some(ibis_doc_model::Clip { media: clip, audio: format.is_audio(), options, link })
            });
            let artistic_effect = child(blip_fill, "blip").and_then(Self::parse_artistic_effect);
            let name = child(node, "nvPicPr").and_then(|n| child(n, "cNvPr")).and_then(|c| attr(c, "name")).unwrap_or("").to_string();
            ShapeKind::Picture(Picture { media, crop, stroke, corner, duotone, clear, luminance, clip, shadow, reflection, soft_edge, alt_text: Default::default(), decorative: false, name, geometry, effects, tile, fill_rect, artistic_effect })
        } else {
            ShapeKind::Placeholder(Placeholder::Picture(format))
        };
        let id = self.presentation.new_shape_id();
        if let Some(f_node) = child(node, "f") {
            if let Some(f_text) = f_node.text().map(str::trim).filter(|s| !s.is_empty()) {
                self.slide_formulas.insert(id, f_text.to_string());
            }
        }
        Some(Shape { id, frame, rotation, flip_h, flip_v, kind, floating: None })
    }

    fn graphic_frame(&mut self, ctx: &SlideCtx<'d, 'i, '_>, node: Node<'d, 'i>, rels: &Rels, transform: Transform) -> Option<Shape> {
        let (frame, rotation, flip_h, flip_v) = match self.anchor.take() {
            Some(frame) => (frame, 0, false, false),
            None => self.placement(child(node, "xfrm"), None, transform)?,
        };
        let data = path(node, &["graphic", "graphicData"])?;
        let uri = attr(data, "uri").unwrap_or("");
        let kind = if uri.ends_with("/table") {
            match child(data, "tbl").and_then(|t| self.table(ctx, t, rels)) {
                Some(table) => ShapeKind::Table(Box::new(table)),
                None => ShapeKind::Placeholder(Placeholder::Table),
            }
        } else if uri.contains("chart") {
            self.chart_user_shapes(ctx, data, rels, frame);
            let kind = match self.chart(ctx, data, rels) {
                Some(chart) => ShapeKind::Chart(Box::new(chart)),
                None => ShapeKind::Placeholder(Placeholder::Chart),
            };
            // Where it came from, for saving a chart edited on a sheet.
            let part = data.descendants().find(|n| n.is_element() && n.tag_name().name() == "chart").and_then(attr_rid).and_then(|rid| rels.get(rid)).filter(|r| !r.external).map(|r| r.target.clone());
            let frame_id = path(node, &["nvGraphicFramePr", "cNvPr"]).and_then(|n| attr(n, "id")).map(str::to_string);
            if let (Some(chart), ShapeKind::Chart(_)) = (part, &kind) {
                let id = self.presentation.new_shape_id();
                self.chart_sources.push(crate::save::xlsx::ChartSource { shape: id, chart, drawing: ctx.slide.name.clone(), frame_id });
                return Some(Shape { id, frame, rotation, flip_h, flip_v, kind, floating: None });
            }
            kind
        } else if uri.ends_with("/diagram") {
            self.diagram_shapes(ctx, data, rels, frame);
            ShapeKind::Placeholder(Placeholder::Diagram)
        } else {
            // OLE objects and the like carry a picture of their content.
            let pic = data.descendants().find(|n| n.has_tag_name("pic"))?;
            let mut shape = self.picture(ctx, pic, rels, Level::Layout, transform)?;
            shape.frame = frame;
            return Some(shape);
        };
        let id = self.presentation.new_shape_id();
        Some(Shape { id, frame, rotation, flip_h, flip_v, kind, floating: None })
    }

    /// Read the shapes drawn over the chart in `data` into `self.pending`:
    /// each anchor places its shape by fractions of the chart's `frame`
    /// (`cdr:relSizeAnchor`), or by a corner and a size (`cdr:absSizeAnchor`).
    fn chart_user_shapes(&mut self, ctx: &SlideCtx<'d, 'i, '_>, data: Node, rels: &Rels, frame: EmuRect) {
        let Some(reference) = data.descendants().find(|n| n.is_element() && n.tag_name().name() == "chart") else { return };
        let Some(part) = attr_rid(reference).and_then(|rid| rels.get(rid)).map(|r| r.target.clone()) else { return };
        let Some((root, drawing_rels)) = self.user_shapes.get(&part).cloned() else { return };
        let corner = |n: Option<Node>| -> Option<(f64, f64)> {
            let n = n?;
            let num = |name| child(n, name).and_then(|c| c.text()).and_then(|t| t.trim().parse::<f64>().ok());
            Some((num("x")?, num("y")?))
        };
        let (fx, fy, fw, fh) = (frame.x as f64, frame.y as f64, frame.width as f64, frame.height as f64);
        let mut shapes = Vec::new();
        for anchor in elements(root) {
            let from = corner(child(anchor, "from"));
            let rect = match anchor.tag_name().name() {
                "relSizeAnchor" => from.zip(corner(child(anchor, "to"))).map(|((x0, y0), (x1, y1))| {
                    (fx + x0 * fw, fy + y0 * fh, (x1 - x0) * fw, (y1 - y0) * fh)
                }),
                "absSizeAnchor" => from.zip(child(anchor, "ext")).map(|((x0, y0), ext)| {
                    (fx + x0 * fw, fy + y0 * fh, attr_i64(ext, "cx").unwrap_or(0) as f64, attr_i64(ext, "cy").unwrap_or(0) as f64)
                }),
                _ => None,
            };
            let Some((x, y, w, h)) = rect else { continue };
            // Map the shape's own box onto the anchor's.
            let Some(shape) = elements(anchor).find(|n| matches!(n.tag_name().name(), "sp" | "grpSp" | "cxnSp" | "pic")) else { continue };
            // A text box with no width or height shows nothing (Excel cuts its text to it: `vertOverflow="clip"`).
            if shape.has_tag_name("sp") && (w <= 0.0 || h <= 0.0) && child(shape, "txBody").is_some() {
                continue;
            }
            let xfrm = elements(shape).find(|n| matches!(n.tag_name().name(), "spPr" | "grpSpPr")).and_then(|p| child(p, "xfrm"));
            let (off, ext) = xfrm.map(offset_extent).unwrap_or(((0, 0), (w as i64, h as i64)));
            let scale = |e: f64, c: i64| if c == 0 { 1.0 } else { e / c as f64 };
            let (sx, sy) = (scale(w, ext.0), scale(h, ext.1));
            let transform = Transform { sx, sy, tx: x - off.0 as f64 * sx, ty: y - off.1 as f64 * sy, turns: [None; MAX_TURNS], groups: [None; MAX_TURNS] };
            self.shapes(ctx, anchor, &drawing_rels, Level::Slide, transform, None, &mut shapes);
        }
        self.pending.extend(shapes);
    }

    /// Read the shapes of the SmartArt graphic in `data` into `self.pending`,
    /// from the drawing PowerPoint keeps of it (`dsp:drawing`), whose shapes
    /// are placed from the graphic's top-left corner. A graphic without one
    /// (written by another program) shows nothing.
    fn diagram_shapes(&mut self, ctx: &SlideCtx<'d, 'i, '_>, data: Node, rels: &Rels, frame: EmuRect) {
        let Some(ids) = data.descendants().find(|n| n.is_element() && n.tag_name().name() == "relIds") else { return };
        let Some(rid) = ids.attributes().find(|a| a.name() == "dm" && a.namespace().is_some()).map(|a| a.value()) else { return };
        let Some(part) = rels.get(rid).filter(|r| !r.external).map(|r| r.target.clone()) else { return };
        let Some((root, drawing_rels)) = self.diagrams.get(&part).cloned() else { return };
        let Some(tree) = child(root, "spTree") else { return };
        let transform = Transform { sx: 1.0, sy: 1.0, tx: frame.x as f64, ty: frame.y as f64, turns: [None; MAX_TURNS], groups: [None; MAX_TURNS] };
        let mut shapes = Vec::new();
        self.shapes(ctx, tree, &drawing_rels, Level::Slide, transform, None, &mut shapes);
        self.pending.extend(shapes);
    }

    /// The chart a `c:chart` element in `data` refers to.
    fn chart(&self, ctx: &SlideCtx<'d, 'i, '_>, data: Node, rels: &Rels) -> Option<ibis_doc_model::Chart> {
        let reference = data.descendants().find(|n| n.is_element() && n.tag_name().name() == "chart")?;
        let rid = attr_rid(reference)?;
        let part = rels.get(rid).filter(|r| !r.external)?.target.clone();
        if let Some(chart) = self.charts.borrow_mut().remove(&part) {
            return chart;
        }
        let xml = self.package.read_xml(&part).ok()?;
        let doc = parse(&part, &xml).ok()?;
        let override_theme = self
            .package
            .related(&part, crate::package::REL_THEME_OVERRIDE)
            .ok()
            .flatten()
            .and_then(|t| {
                let s = self.package.read_xml(&t).ok()?;
                let d = parse(&t, &s).ok()?;
                Some(ctx.colors.theme().with_override(d.root_element()))
            });
        let colors = match &override_theme {
            Some(t) => ctx.colors.with_theme(t),
            None => ctx.colors.clone(),
        };
        crate::chart::read_in(doc.root_element(), &colors, false)
    }

    /// Ink (`p:contentPart`): its InkML strokes drawn as an SVG picture.
    /// The part's `xfrm` is the box of the strokes' points; the picture
    /// reaches past it by half the brush.
    fn ink(&mut self, part: Node, rels: &Rels, transform: Transform) -> Option<Shape> {
        let target = rels.get(attr_rid(part)?).filter(|r| !r.external)?.target.clone();
        let xml = self.package.read_xml(&target).ok()?;
        let doc = parse(&target, &xml).ok()?;
        let xfrm = child(part, "xfrm")?;
        let (off, ext) = offset_extent(xfrm);
        let drawing = ink::read(doc.root_element(), (off.0 as f64, off.1 as f64, ext.0 as f64, ext.1 as f64))?;
        let (x, y, w, h) = drawing.bounds;
        let frame = transform.rect(x.round() as i64, y.round() as i64, w.round() as i64, h.round() as i64);
        let media = self.presentation.add_media(MediaFormat::Svg, drawing.svg.into_bytes());
        let kind = ShapeKind::Picture(Picture { media, crop: Default::default(), stroke: None, corner: 0, duotone: None, clear: None, luminance: None, clip: None, shadow: None, reflection: None, soft_edge: None, alt_text: Default::default(), decorative: false, name: String::new(), geometry: None, effects: Default::default(), tile: None, fill_rect: None, artistic_effect: None });
        let id = self.presentation.new_shape_id();
        Some(Shape { id, frame, rotation: 0, flip_h: false, flip_v: false, kind, floating: None })
    }

    /// Store (once) the media part `rid` points to.
    fn media(&mut self, rels: &Rels, rid: &str) -> Option<(MediaId, MediaFormat)> {
        let rel = rels.get(rid).filter(|r| !r.external)?;
        if let Some(found) = self.media.get(&rel.target) {
            return Some(*found);
        }
        let data = self.package.read(&rel.target).ok()?;
        // A picture whose name does not say what it is (a `.tmp` file) is told by its first bytes.
        let format = match MediaFormat::from_path(&rel.target) {
            MediaFormat::Other => MediaFormat::sniff(&data).unwrap_or(MediaFormat::Other),
            named => named,
        };
        let id = self.presentation.add_media(format, data);
        self.media.insert(rel.target.clone(), (id, format));
        Some((id, format))
    }

fn parse_grad_fill(colors: &crate::theme::Colors, node: Node, placeholder: Option<Color>) -> Option<Fill> {
    let mut stops: Vec<GradientStop> = drawing_child(node, "gsLst")
        .into_iter()
        .flat_map(|l| elements(l).filter(|n| n.has_tag_name(("http://schemas.openxmlformats.org/drawingml/2006/main", "gs"))))
        .filter_map(|gs| {
            let color = colors.in_parent(gs, placeholder)?;
            Some(GradientStop { position: unqualified_attribute(gs, "pos").and_then(|v|v.trim().parse::<i64>().ok()).unwrap_or(0).clamp(0, 100_000) as u32, color })
        })
        .collect();
    stops.sort_by_key(|s| s.position);
    if stops.is_empty() {
        return None;
    }
    if let Some(path) = drawing_child(node, "path") {
        let kind = unqualified_attribute(path, "path").and_then(GradientPath::parse).unwrap_or(GradientPath::Circle);
        let rect = drawing_child(path, "fillToRect");
        if let Some(r) = rect {
            let has_any_attr = ["l", "t", "r", "b"].iter().any(|&k| unqualified_attribute(r, k).is_some());
            if !has_any_attr {
                return Some(match kind {
                    GradientPath::Circle => Fill::Path { path: kind, focus: [0, 0, 100_000, 100_000], stops },
                    _ => Fill::Solid(stops[0].color),
                });
            }
        }
        let focus = ["l", "t", "r", "b"].map(|k| rect.and_then(|r| unqualified_attribute(r, k)).and_then(|v|v.trim().parse::<i64>().ok()).unwrap_or(0) as i32);
        return Some(Fill::Path { path: kind, focus, stops });
    }
    let lin = drawing_child(node, "lin");
    let angle = lin.and_then(|l| unqualified_attribute(l, "ang")).and_then(|v|v.trim().parse::<i64>().ok()).unwrap_or(5_400_000) as i32;
    let scaled = lin.and_then(|l| unqualified_attribute(l, "scaled")).map_or(false, |v| v == "1" || v == "true");
    let flip = ibis_doc_model::TileFlip::parse(unqualified_attribute(node, "flip").unwrap_or("none"));
    if let Some(tr) = drawing_child(node, "tileRect") {
        let tile = ["l", "t", "r", "b"].map(|k| {
            unqualified_attribute(tr, k)
                .and_then(|v| v.trim().parse::<i64>().ok())
                .unwrap_or(0) as i32
        });
        return Some(Fill::TileLinear { angle, scaled, stops, tile, flip });
    }
    let mut angle = angle;
    match flip {
        ibis_doc_model::TileFlip::X => angle = 10_800_000 - angle,
        ibis_doc_model::TileFlip::Y => angle = -angle,
        ibis_doc_model::TileFlip::Xy => angle = angle + 10_800_000,
        _ => {}
    }
    angle = angle.rem_euclid(21_600_000);
    Some(Fill::Linear { angle, scaled, stops })
}

    /// A fill element (`a:solidFill`, `a:gradFill`, …). `placeholder` is the
    /// colour `phClr` stands for; `group` is the fill `a:grpFill` refers to.
    fn fill(&mut self, ctx: &SlideCtx<'d, 'i, '_>, node: Node, rels: &Rels, placeholder: Option<Color>, group: Option<&Fill>) -> Option<Fill> {
        match node.tag_name().name() {
            "noFill" => Some(Fill::None),
            "solidFill" => Some(Fill::Solid(ctx.colors.in_parent(node, placeholder)?)),
            "gradFill" => Self::parse_grad_fill(&ctx.colors, node, placeholder),
            "blipFill" => {
                let embed = child(node, "blip").and_then(blip_target)?;
                let (media, format) = self.media(rels, embed)?;
                if !drawable(format) {
                    return Some(Fill::None);
                }
                let mut blip = ibis_doc_model::BlipFill::new(media);
                blip.rot_with_shape = unqualified_attribute(node, "rotWithShape").is_none_or(|v| v != "0" && v != "false");
                if let Some(r) = child(node, "srcRect") {
                    let v = |name| attr_i64(r, name).unwrap_or(0) as i32;
                    blip.crop = ibis_doc_model::Crop { left: v("l"), top: v("t"), right: v("r"), bottom: v("b") };
                }
                blip.fill_rect = child(node, "stretch").and_then(|s| child(s, "fillRect")).map(|r| ["l", "t", "r", "b"].map(|k| attr_i64(r, k).unwrap_or(0) as i32)).filter(|r| *r != [0, 0, 0, 0]);
                if let Some(b) = child(node, "blip") {
                    blip.duotone = drawing_child(b, "duotone").and_then(|d| {
                        let colors: Vec<_> = elements(d).filter_map(|c| ctx.colors.resolve(c, placeholder)).collect();
                        Some((colors.first()?.rgb, colors.get(1)?.rgb))
                    });
                    blip.luminance = drawing_child(b, "lum").and_then(|l| {
                        let bright = attr_i64(l, "bright").unwrap_or(0).clamp(-100_000, 100_000) as i32;
                        let contrast = attr_i64(l, "contrast").unwrap_or(0).clamp(-100_000, 100_000) as i32;
                        (bright != 0 || contrast != 0).then_some((bright, contrast))
                    });
                    blip.effects.grayscale = drawing_child(b, "grayscl").is_some();
                    blip.effects.bilevel = drawing_child(b, "biLevel").map(|l| attr_i64(l, "thresh").unwrap_or(50_000).clamp(0, 100_000) as i32);
                    blip.effects.opacity = drawing_child(b, "alphaModFix")
                        .map(|a| attr_i64(a, "amt").unwrap_or(100_000))
                        .or_else(|| drawing_child(b, "alphaRepl").and_then(|a| attr_i64(a, "a")))
                        .map(|v| v.clamp(0, 100_000) as i32)
                        .filter(|v| *v != 100_000);
                    blip.effects.replace = drawing_child(b, "clrRepl").and_then(|r| elements(r).next()).and_then(|c| ctx.colors.resolve(c, placeholder)).map(|c| c.rgb);
                }
                let tile = child(node, "tile").map(|tile| ibis_doc_model::PictureTile {
                    tx: attr_i64(tile, "tx").unwrap_or(0),
                    ty: attr_i64(tile, "ty").unwrap_or(0),
                    sx: attr_i64(tile, "sx").unwrap_or(100_000) as i32,
                    sy: attr_i64(tile, "sy").unwrap_or(100_000) as i32,
                    flip: ibis_doc_model::TileFlip::parse(unqualified_attribute(tile, "flip").unwrap_or("none")),
                    align: ibis_doc_model::TileAlign::parse(unqualified_attribute(tile, "algn").unwrap_or("tl")),
                });
                blip.tile = tile;
                if blip.is_plain() {
                    Some(match tile {
                        Some(t) => Fill::TileImage { media, tx: t.tx, ty: t.ty, sx: t.sx, sy: t.sy, flip: t.flip, align: t.align },
                        None => Fill::Image(media),
                    })
                } else {
                    Some(Fill::Blip(Box::new(blip)))
                }
            }
            "pattFill" => {
                let fg = drawing_child(node, "fgClr").and_then(|c| ctx.colors.in_parent(c, placeholder)).unwrap_or(Color::BLACK);
                let bg = drawing_child(node, "bgClr").and_then(|c| ctx.colors.in_parent(c, placeholder)).unwrap_or(Color::WHITE);
                let preset = unqualified_attribute(node, "prst").unwrap_or("pct5");
                Some(match ibis_doc_model::pattern_tile(preset) {
                    Some(_) => Fill::Pattern { preset: preset.to_string(), foreground: fg, background: bg },
                    None => Fill::Solid(fg),
                })
            }
            "grpFill" => Some(group.cloned().unwrap_or_default()),
            _ => None,
        }
    }

    /// The fill a shape style (`p:style/a:fillRef`) gives.
    fn style_fill(&mut self, ctx: &SlideCtx<'d, 'i, '_>, style: Option<Node>, rels: &Rels) -> Fill {
        let Some(reference) = style.and_then(|s| child(s, "fillRef")) else { return Fill::None };
        let idx = attr_i64(reference, "idx").unwrap_or(0).max(0) as usize;
        if idx == 0 {
            return Fill::None;
        }
        let color = ctx.colors.in_parent(reference, None);
        let themed = ctx.theme.and_then(|t| fill_style(t, idx)).and_then(|f| self.fill(ctx, f, rels, color, None));
        themed.or(color.map(Fill::Solid)).unwrap_or_default()
    }

    fn stroke(&self, ctx: &SlideCtx<'d, 'i, '_>, ln: Option<Node>, style: Option<Node>) -> Option<Stroke> {
        let reference = style.and_then(|s| child(s, "lnRef"));
        let ref_color = reference.and_then(|r| ctx.colors.in_parent(r, None));
        let ref_idx = reference.and_then(|r| attr_i64(r, "idx")).unwrap_or(0).max(0) as usize;
        let themed = if ref_idx > 0 { ctx.theme.and_then(|t| line_style(t, ref_idx)) } else { None };

        // `Some(None)` is an explicit "no line".
        let color_of = |ln: Node| -> Option<Option<Color>> {
            let fill = fill_element(ln)?;
            Some(match fill.tag_name().name() {
                "noFill" => None,
                "solidFill" => ctx.colors.in_parent(fill, ref_color),
                "gradFill" => child(fill, "gsLst").and_then(|l| elements(l).next()).and_then(|gs| ctx.colors.in_parent(gs, ref_color)),
                _ => ref_color,
            })
        };
        // A style's line reference with index 0 is "no line", whatever
        // colour it names.
        let referenced = if ref_idx > 0 { ref_color.map(Some) } else { None };
        let color = ln.and_then(color_of).or_else(|| themed.and_then(color_of)).or(referenced)??;
        let width = ln.and_then(|l| attr_i64(l, "w")).or_else(|| themed.and_then(|l| attr_i64(l, "w"))).unwrap_or(Stroke::DEFAULT_WIDTH);
        let dash = ln.and_then(|l| drawing_child(l, "prstDash")).or_else(|| themed.and_then(|l| drawing_child(l, "prstDash")));
        let end = |name| {
            let e = ln.and_then(|l| drawing_child(l, name));
            LineEnd::parse(e.and_then(|e| attr(e, "type")), e.and_then(|e| attr(e, "w")), e.and_then(|e| attr(e, "len")))
        };
        let dash_kind = dash.and_then(|d| attr(d, "val")).and_then(ibis_doc_model::LineDash::parse).unwrap_or_default();
        // A `cap` of its own, else the theme line's; with none, PowerPoint
        // draws flat ends (the coverage suite PSHP; the standard says `sq`).
        let cap = ln.and_then(|l| attr(l, "cap")).or_else(|| themed.and_then(|l| attr(l, "cap")));
        let cap = match cap {
            Some("rnd") => ibis_doc_model::LineCap::Round,
            Some("sq") => ibis_doc_model::LineCap::Square,
            _ => ibis_doc_model::LineCap::Flat,
        };
        let join_of = |l: Node| {
            if drawing_child(l, "round").is_some() {
                Some(ibis_doc_model::LineJoin::Round)
            } else if drawing_child(l, "bevel").is_some() {
                Some(ibis_doc_model::LineJoin::Bevel)
            } else if drawing_child(l, "miter").is_some() {
                Some(ibis_doc_model::LineJoin::Miter)
            } else {
                None
            }
        };
        let compound = ln.and_then(|l| attr(l, "cmpd")).or_else(|| themed.and_then(|l| attr(l, "cmpd"))).and_then(ibis_doc_model::LineCompound::parse).unwrap_or_default();
        let default_join = ibis_doc_model::LineJoin::Miter;
        let join = ln.and_then(join_of).or_else(|| themed.and_then(join_of)).unwrap_or(default_join);
        let line_fill = ln.and_then(fill_element).and_then(|f| {
            if f.tag_name().name() == "gradFill" {
                Self::parse_grad_fill(&ctx.colors, f, ref_color)
            } else {
                None
            }
        });
        Some(Stroke {
            color,
            width,
            dashed: dash_kind != ibis_doc_model::LineDash::Solid,
            head: end("headEnd"),
            tail: end("tailEnd"),
            dash: dash_kind,
            cap,
            join,
            compound,
            alignment: ln.and_then(|l| attr(l, "algn")).or_else(|| themed.and_then(|l| attr(l, "algn"))).and_then(ibis_doc_model::LineAlignment::parse).unwrap_or_default(),
            fill: line_fill,
        })
    }

    fn text_body(&mut self, ctx: &SlideCtx<'d, 'i, '_>, body: Node<'d, 'i>, defaults: TextDefaults, inherited: Option<&Inherited<'d, 'i>>, table_cell: bool) -> TextBody {
        let colors = &ctx.colors;

        // List styles, from the most general to the shape's own.
        let mut levels = ListStyle::default();
        match inherited {
            Some(inh) => {
                let styles = path(ctx.master.root, &["txStyles"]);
                let name = match inh.kind.as_str() {
                    "title" => "titleStyle",
                    "dt" | "ftr" | "sldNum" | "hdr" => "otherStyle",
                    _ => "bodyStyle",
                };
                if let Some(base) = styles.and_then(|s| child(s, name)) {
                    levels.merge(base, colors);
                }
                for ancestor in [inh.master, inh.layout].into_iter().flatten() {
                    if let Some(list) = path(ancestor, &["txBody", "lstStyle"]) {
                        levels.merge_shape(list, colors);
                    }
                }
            }
            None => {
                if let Some(default) = self.default_text {
                    levels.merge(default, colors);
                }
                // A table cell's text follows the master's otherStyle after the presentation's defaultTextStyle; the text of
                // a shape that is not a placeholder follows the defaultTextStyle only (PowerPoint's PDFs: a run with no `sz`
                // is 18 pt where the master's otherStyle says 12 pt).
                if table_cell && let Some(other) = path(ctx.master.root, &["txStyles", "otherStyle"]) {
                    levels.merge(other, colors);
                }
            }
        }
        for level in &mut levels.levels {
            if let Some(color) = defaults.color {
                level.run.color = Some(color);
            }
            if let Some(bold) = defaults.bold {
                level.run.bold = Some(bold);
            }
            if let Some(italic) = defaults.italic {
                level.run.italic = Some(italic);
            }
        }
        if let Some(list) = child(body, "lstStyle") {
            levels.merge_shape(list, colors);
        }

        // Body properties: ancestors first, the shape's own last.
        let mut out = TextBody::default();
        let body_props: Vec<Node> = inherited
            .into_iter()
            .flat_map(|i| [i.master, i.layout])
            .flatten()
            .filter_map(|a| path(a, &["txBody", "bodyPr"]))
            .chain(child(body, "bodyPr"))
            .collect();
        let mut line_reduction = 0u32;
        for pr in &body_props {
            let inset = |name, current| attr_i64(*pr, name).unwrap_or(current);
            out.insets = Insets {
                left: inset("lIns", out.insets.left),
                top: inset("tIns", out.insets.top),
                right: inset("rIns", out.insets.right),
                bottom: inset("bIns", out.insets.bottom),
            };
            match attr(*pr, "anchor") {
                Some("ctr") => out.anchor = VerticalAnchor::Middle,
                Some("b") => out.anchor = VerticalAnchor::Bottom,
                Some(_) => out.anchor = VerticalAnchor::Top,
                None => {}
            }
            if let Some(wrap) = attr(*pr, "wrap") {
                out.wrap = wrap != "none";
            }
            if let Some(overflow) = attr(*pr, "vertOverflow") {
                out.clip = matches!(overflow, "clip" | "ellipsis");
            }
            if let Some(vert) = attr(*pr, "vert") {
                let kind = ibis_doc_model::VerticalKind::parse(vert);
                out.vertical = kind.is_some();
                out.vertical_kind = kind.unwrap_or_default();
                out.sideways = match vert {
                    "vert" => 5_400_000,
                    "vert270" => 16_200_000,
                    _ => 0,
                };
            }
            if let Some(rot) = attr_i64(*pr, "rot") {
                out.rotation = (rot as i32).rem_euclid(21_600_000);
            }
            if let Some(upright) = attr_bool(*pr, "upright") {
                out.upright = upright;
            }
            if let Some(cols) = attr_i64(*pr, "numCol") {
                out.columns = (cols as u32).clamp(1, 16);
            }
            if let Some(spc) = attr_i64(*pr, "spcCol") {
                out.column_spacing = spc.max(0);
            }
            if let Some(warp) = drawing_child(*pr, "prstTxWarp") {
                if let Some(prst) = attr(warp, "prst") {
                    if prst != "textNoShape" {
                        out.warp = Some(prst.to_string());
                        out.warp_adjust = drawing_child(warp, "avLst")
                            .into_iter()
                            .flat_map(|l| elements(l).filter(|g| g.tag_name().name() == "gd"))
                            .filter_map(|g| Some((attr(g, "name")?.to_string(), attr(g, "fmla")?.strip_prefix("val ")?.trim().parse::<i64>().ok()?)))
                            .collect();
                    }
                }
            }
            if let Some(fit) = drawing_child(*pr, "normAutofit") {
                out.autofit = ibis_doc_model::TextAutofit::Shrink;
                out.font_scale = attr_i64(fit, "fontScale").unwrap_or(100_000).clamp(1, 100_000) as u32;
                line_reduction = attr_i64(fit, "lnSpcReduction").unwrap_or(0).clamp(0, 100_000) as u32;
            } else if drawing_child(*pr, "spAutoFit").is_some() {
                out.autofit = ibis_doc_model::TextAutofit::ResizeShape;
                out.font_scale = 100_000;
                line_reduction = 0;
            } else if drawing_child(*pr, "noAutofit").is_some() {
                out.autofit = ibis_doc_model::TextAutofit::None;
                out.font_scale = 100_000;
                line_reduction = 0;
            }
        }

        let mut paragraphs = Vec::new();
        let mut numbering: Option<(u8, String, u32)> = None;
        for p in children(body, "p") {
            let ppr = child(p, "pPr");
            let level = ppr.and_then(|n| attr_i64(n, "lvl")).unwrap_or(0).clamp(0, 8) as usize;
            let mut props = levels.levels[level].clone();
            if let Some(ppr) = ppr {
                props.merge(ppr, colors);
            }
            paragraphs.extend(self.paragraph(ctx, p, &props, level as u8, line_reduction, &mut numbering));
        }
        if !paragraphs.is_empty() {
            out.paragraphs = paragraphs;
        }

        // The ribbon shows the format of the first run.
        let first = out.paragraphs.iter().flat_map(|p| p.runs.first().map(|r| (p, &r.style))).next();
        let (align, run) = match first {
            Some((p, run)) => (p.align, run.clone()),
            None => (out.paragraphs[0].align, out.paragraphs[0].end_style.clone()),
        };
        out.style = TextStyle { align: align.unwrap_or_default(), ..TextStyle::default() }.with_run(&run);
        out
    }

    /// One `a:p`; a line break (`a:br`) starts a continuation paragraph
    /// without bullet or spacing above.
    fn paragraph(
        &mut self,
        ctx: &SlideCtx<'d, 'i, '_>,
        p: Node,
        props: &ParaProps,
        level: u8,
        line_reduction: u32,
        numbering: &mut Option<(u8, String, u32)>,
    ) -> Vec<Paragraph> {
        let colors = &ctx.colors;
        let default_font_size = self.default_font_size;
        let slide_parts = &self.slide_parts;
        let run_style = |rpr: Option<Node>, text: &str| -> RunStyle {
            let mut run: RunProps = props.run.clone();
            if let Some(rpr) = rpr {
                run.merge(rpr, colors);
            }
            RunStyle {
                font_size: Some(run.size.unwrap_or(default_font_size)),
                bold: Some(run.bold.unwrap_or(false)),
                italic: Some(run.italic.unwrap_or(false)),
                underline: run.underline.unwrap_or(false),
                highlight: run.highlight.map(|c| c.rgb),
                outline: run.outline.flatten().map(|(c, w)| (c.rgb, w)),
                fill: run.fill.clone().flatten(),
                invisible: false,
                baseline: run.baseline.unwrap_or(0),
                spacing: run.spacing.unwrap_or(0),
                color: Some(run.color.map(|c| c.rgb).unwrap_or(ibis_doc_model::Rgb::BLACK)),
                font: run.font_for(text),
                fallback_font: run.fallback_for(text),
                kern: ibis_doc_model::KernMin(run.kern),
                field: None,
                strike: run.strike.unwrap_or_default(),
                caps: run.caps.unwrap_or_default(),
                underline_kind: run.underline_kind.unwrap_or_default(),
                link: rpr.and_then(|r| child(r, "hlinkClick")).and_then(|h| link_of(h, &ctx.slide.rels, slide_parts)),
                link_tooltip: rpr.and_then(|r| child(r, "hlinkClick")).and_then(|h| attr(h, "tooltip")).filter(|s| !s.is_empty()).map(str::to_string),
                shadow: run.shadow.flatten(),
                glow: run.glow.flatten(),
                lang: run.lang,
                math: None,
                math_inline: false,
                border: None,
            }
        };

        let mut lines: Vec<Vec<Run>> = vec![Vec::new()];
        // The `a:rPr` of the `a:br` (or run) that ended each line, for an empty line's height.
        let mut line_ends: Vec<Option<RunStyle>> = Vec::new();
        // Where each run's text is in the part, when recording (see `text_sources`).
        let recording = self.text_sources.is_some();
        let mut sources: Vec<Vec<Option<crate::save::pptx::RunSource>>> = vec![Vec::new()];
        let mut elements_of_runs: Vec<Vec<Vec<u32>>> = vec![Vec::new()];
        for n in elements(p) {
            match n.tag_name().name() {
                "r" | "fld" => {
                    let t = child(n, "t");
                    let mut text = t.and_then(|t| t.text()).unwrap_or("").to_string();
                    // A field's text is the file's; a slide number is computed.
                    let mut from_file = t.is_some();
                    let mut field = None;
                    if n.has_tag_name("fld") && attr(n, "type") == Some("slidenum") {
                        text = ctx.number.to_string();
                        from_file = false;
                        field = Some(ibis_doc_model::TextField::SlideNumber);
                    }
                    // A date: the file's text (the app brings it up to date).
                    if n.has_tag_name("fld")
                        && let Some(format) = attr(n, "type").and_then(|t| t.strip_prefix("datetime")).and_then(|d| match d {
                            "" => Some(0),
                            "yyyy" => Some(14),
                            _ => d.parse::<u8>().ok().filter(|f| (1..=13).contains(f)),
                        })
                    {
                        let lang = child(n, "rPr").and_then(|r| attr(r, "lang")).unwrap_or("");
                        let east_asian = ["ja", "zh", "ko"].iter().any(|l| lang.starts_with(l));
                        from_file = false;
                        field = Some(ibis_doc_model::TextField::DateTime { format, east_asian });
                    }
                    let t_path = if recording && from_file { t.map(crate::xml::element_path) } else { None };
                    let r_path = if recording { crate::xml::element_path(n) } else { Vec::new() };
                    let mut offset = 0usize;
                    // A vertical tab is a line break inside a run.
                    // In a worksheet's drawing a line feed inside a run is a line break too (Excel for Windows' PDF of a
                    // text box holding one `a:t` of several lines shows each on its own line).
                    let in_sheet = p.ancestors().any(|a| a.has_tag_name(("http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing", "wsDr")));
                    for (i, piece) in text.split(|c| c == '\u{b}' || (in_sheet && c == '\n')).enumerate() {
                        if i > 0 {
                            line_ends.push(Some(run_style(child(n, "rPr"), "")));
                            lines.push(Vec::new());
                            sources.push(Vec::new());
                            elements_of_runs.push(Vec::new());
                            offset += 1;
                        }
                        // PowerPoint draws East Asian characters with the
                        // East Asian font and the others with the Latin one,
                        // so a run is split where the script changes.
                        for segment in script_segments(piece) {
                            let style = RunStyle { field, ..run_style(child(n, "rPr"), segment) };
                            lines.last_mut().expect("a line").push(Run { text: segment.to_string(), style });
                            let len = segment.chars().count();
                            let source = t_path.clone().map(|t| crate::save::pptx::RunSource { t, start: offset, len });
                            sources.last_mut().expect("a line").push(source);
                            elements_of_runs.last_mut().expect("a line").push(r_path.clone());
                            offset += len;
                        }
                    }
                }
                "br" => {
                    line_ends.push(child(n, "rPr").map(|r| run_style(Some(r), "")));
                    lines.push(Vec::new());
                    sources.push(Vec::new());
                    elements_of_runs.push(Vec::new());
                }
                // A formula (`a14:m`, alone or in the `Choice` of a markup-compatibility wrapper).
                "AlternateContent" | "m" => {
                    let formula = if n.has_tag_name("m") { Some(n) } else { child(n, "Choice").and_then(|c| elements(c).find(|e| e.has_tag_name("m"))) };
                    if let Some(math) = formula.and_then(|f| elements(f).find(|e| e.tag_name().namespace() == Some(crate::math_xml::M))) {
                        let text = crate::math_xml::read_math(math).linear();
                        if !text.is_empty() {
                            let style = RunStyle { font: Some("Cambria Math".to_string()), math: Some(Box::new(crate::math_xml::read_math(math))), math_inline: math.tag_name().name() == "oMath", ..run_style(None, &text) };
                            lines.last_mut().expect("a line").push(Run { text, style });
                            sources.last_mut().expect("a line").push(None);
                            elements_of_runs.last_mut().expect("a line").push(crate::xml::element_path(n));
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(all) = self.text_sources.as_mut() {
            all.extend(sources);
            let p_path = crate::xml::element_path(p);
            for (line, runs) in elements_of_runs.into_iter().enumerate() {
                self.para_sources.push(crate::save::pptx::ParagraphSource { p: p_path.clone(), line, runs });
            }
        }
        let end_style = run_style(child(p, "endParaRPr"), "");
        let size = lines.iter().flatten().next().and_then(|r| r.style.font_size).or(end_style.font_size).unwrap_or(self.default_font_size);
        let spacing = |s: &Option<Spacing>| match s {
            Some(Spacing::Points(v)) => *v,
            // Spacing percentage (`a:spcPct`) is relative to the single-spaced line height (`1.2 × size`; 100_000 = 100%).
            Some(Spacing::Percent(v)) => (*v as u64 * size as u64 * 6 / 500_000) as u32,
            None => 0,
        };
        let line_spacing = match props.line_spacing {
            Some(LineSpacing::Percent(v)) => Some(LineSpacing::Percent(v.saturating_sub(line_reduction).max(10_000))),
            None if line_reduction > 0 => Some(LineSpacing::Percent(100_000 - line_reduction)),
            other => other,
        };

        let has_text = lines.iter().any(|l| !l.is_empty());
        // A picture bullet's relationship is the slide's, its layout's or its master's.
        let picture = match &props.bullet {
            Some(BulletProps::Picture(rid)) => {
                let parts = [ctx.slide.rels.clone(), ctx.layout.rels.clone(), ctx.master.rels.clone()];
                parts.iter().find_map(|rels| self.media(rels, rid)).map(|(id, _)| id)
            }
            _ => None,
        };
        // An empty paragraph keeps its bullet (it shows once text is typed;
        // it is not drawn while empty) and does not count in a numbering.
        let bullet = match &props.bullet {
            Some(BulletProps::Picture(_)) if picture.is_some() => {
                if has_text {
                    *numbering = None;
                }
                Some(String::new())
            }
            Some(BulletProps::Char(text)) => {
                if has_text {
                    *numbering = None;
                }
                Some(bullet_glyph(text, props.bullet_font.as_deref()))
            }
            Some(BulletProps::AutoNumber { scheme, start_at }) => {
                let n = match numbering {
                    Some((l, s, n)) if *l == level && s == scheme => *n + 1,
                    _ => *start_at,
                };
                if has_text {
                    *numbering = Some((level, scheme.clone(), n));
                }
                Some(auto_number(scheme, n))
            }
            // A paragraph without a bullet ends a numbering, empty or not (PowerPoint numbers again from
            // its start after one).
            _ => {
                *numbering = None;
                None
            }
        };
        let bullet_pct = match props.bullet_size.flatten() {
            Some(v) if v >= crate::style::BULLET_POINTS => ((v - crate::style::BULLET_POINTS) as u64 * 100_000 / size.max(1) as u64).clamp(25_000, 400_000) as u32,
            Some(v) => v,
            None => 100_000,
        };
        let bullet_size = bullet_pct as f32
            * if matches!(props.bullet, Some(BulletProps::Char(_))) { crate::style::bullet_scale(props.bullet_font.as_deref()) } else { 1.0 };
        let auto = match &props.bullet {
            Some(BulletProps::AutoNumber { scheme, start_at }) => Some(ibis_doc_model::text::AutoNumber { scheme: scheme.clone(), start_at: *start_at }),
            _ => None,
        };
        let symbol = match (&props.bullet, props.bullet_font.as_deref()) {
            (Some(BulletProps::Char(code)), Some(font)) if crate::style::is_symbol_font(font) => Some(ibis_doc_model::SymbolBullet {
                font: font.to_string(),
                code: code.clone(),
                stand_in_percent: (crate::style::bullet_scale(Some(font)) * 100.0).round() as u32,
            }),
            _ => None,
        };
        let bullet = bullet.map(|text| Bullet { text, picture, size: bullet_size as u32, color: props.bullet_color.flatten().map(|c| c.rgb), auto, symbol });

        let count = lines.len();
        lines
            .into_iter()
            .enumerate()
            .map(|(i, runs)| Paragraph {
                end_style: if runs.is_empty() {
                    line_ends.get(i).and_then(|s| s.clone()).unwrap_or_else(|| end_style.clone())
                } else {
                    end_style.clone()
                },
                runs,
                align: Some(props.align.unwrap_or_default()),
                margin_left: props.margin_left.unwrap_or(0),
                indent: if i == 0 { props.indent.unwrap_or(0) } else { 0 },
                bullet: if i == 0 { bullet.clone() } else { None },
                line_spacing,
                font_align: props.font_align.unwrap_or_default(),
                space_before: if i == 0 { spacing(&props.space_before) } else { 0 },
                space_before_exact: matches!(props.space_before, Some(Spacing::Percent(_))),
                space_after: if i + 1 == count { spacing(&props.space_after) } else { 0 },
                line_break: i > 0,
                level,
                tabs: props.tabs.clone().unwrap_or_default(),
                default_tab: props.default_tab.filter(|&v| v != 914_400),
                no_snap_to_grid: false,
            })
            .collect()
    }
}

/// Formats kept as pictures. Metafiles are included: the app draws the
/// bitmap most of them carry, and shows a placeholder for the rest.
/// The picture an `a:blip` shows: its SVG version (`asvg:svgBlip`, which
/// stays sharp at any size) when it has one, else its own `r:embed`.
fn blip_target<'a>(blip: Node<'a, '_>) -> Option<&'a str> {
    let svg = blip.descendants().find(|n| n.is_element() && n.tag_name().name() == "svgBlip").and_then(|n| attr(n, "embed"));
    svg.or_else(|| attr(blip, "embed"))
}

fn drawable(format: MediaFormat) -> bool {
    !matches!(format, MediaFormat::Other)
}

/// One part of a table style (`a:wholeTbl`, `a:firstRow`, …) and the cells
/// it covers, as inclusive row and column ranges.
struct StylePart<'d, 'i> {
    node: Node<'d, 'i>,
    rows: (usize, usize),
    columns: (usize, usize),
}

impl<'p, 'd, 'i> Reader<'p, 'd, 'i> {
    /// An `a:tbl`: its grid, its table style and each cell's own format.
    fn table(&mut self, ctx: &SlideCtx<'d, 'i, '_>, tbl: Node<'d, 'i>, rels: &Rels) -> Option<ibis_doc_model::Table> {
        use ibis_doc_model::{CellBorders, Table, TableCell, TableRow};

        let columns: Vec<i64> = child(tbl, "tblGrid").into_iter().flat_map(|g| children(g, "gridCol")).map(|c| attr_i64(c, "w").unwrap_or(0)).collect();
        let rows: Vec<Node> = children(tbl, "tr").collect();
        if columns.is_empty() || rows.is_empty() {
            return None;
        }
        let (last_row, last_col) = (rows.len() - 1, columns.len() - 1);
        let pr = child(tbl, "tblPr");
        let flag = |name| pr.and_then(|p| attr_bool(p, name)).unwrap_or(false);
        let style_options = ibis_doc_model::TableStyleOptions {
            first_row: flag("firstRow"),
            last_row: flag("lastRow"),
            first_col: flag("firstCol"),
            last_col: flag("lastCol"),
            band_row: flag("bandRow"),
            band_col: flag("bandCol"),
        };
        let style_id = pr.and_then(|p| child(p, "tableStyleId")).and_then(|n| n.text()).map(str::trim);
        // A table without `a:tableStyleId` has no style: PowerPoint does not
        // fall back to the `def` of `tableStyles.xml` (that one only seeds new tables).
        let style = style_id.and_then(|target_id| {
            self.table_styles
                .and_then(|list| children(list, "tblStyle").find(|s| attr(*s, "styleId").is_some_and(|id| id.eq_ignore_ascii_case(target_id))))
                .or_else(|| crate::drawing_table_style::find_builtin_style(target_id))
        });

        // Style parts in the order they apply, each with the cells it covers.
        let mut parts: Vec<StylePart> = Vec::new();
        if let Some(style) = style {
            let mut part = |name: &str, rows: (usize, usize), columns: (usize, usize)| {
                if let Some(node) = child(style, name) {
                    parts.push(StylePart { node, rows, columns });
                }
            };
            part("wholeTbl", (0, last_row), (0, last_col));
            let (first_data_row, last_data_row) = (flag("firstRow") as usize, last_row - flag("lastRow") as usize);
            if flag("bandRow") {
                for r in first_data_row..=last_data_row.min(last_row) {
                    let name = if (r - first_data_row).is_multiple_of(2) { "band1H" } else { "band2H" };
                    part(name, (r, r), (0, last_col));
                }
            }
            if flag("bandCol") {
                let first_data_col = flag("firstCol") as usize;
                for c in first_data_col..=(last_col - flag("lastCol") as usize) {
                    let name = if (c - first_data_col).is_multiple_of(2) { "band1V" } else { "band2V" };
                    part(name, (first_data_row, last_data_row.min(last_row)), (c, c));
                }
            }
            if flag("lastCol") {
                part("lastCol", (0, last_row), (last_col, last_col));
            }
            if flag("firstCol") {
                part("firstCol", (0, last_row), (0, 0));
            }
            if flag("lastRow") {
                part("lastRow", (last_row, last_row), (0, last_col));
            }
            if flag("firstRow") {
                part("firstRow", (0, 0), (0, last_col));
            }
        }

        let mut out = Vec::new();
        for (r, tr) in rows.iter().enumerate() {
            let mut cells = Vec::new();
            for (c, tc) in children(*tr, "tc").enumerate() {
                let mut defaults = TextDefaults::default();
                let mut fill = Fill::None;
                let mut borders = CellBorders::default();
                // The table's own fill (`a:wholeTbl`), which a part's translucent fill is laid over.
                let mut base_fill = Fill::None;
                for part in parts.iter().filter(|p| (p.rows.0..=p.rows.1).contains(&r) && (p.columns.0..=p.columns.1).contains(&c)) {
                    if let Some(text) = child(part.node, "tcTxStyle") {
                        if let Some(color) = ctx.colors.in_parent(text, None) {
                            defaults.color = Some(color);
                        }
                        if let Some(b) = attr(text, "b") {
                            defaults.bold = Some(b == "on");
                        }
                        if let Some(i) = attr(text, "i") {
                            defaults.italic = Some(i == "on");
                        }
                    }
                    let Some(cell_style) = child(part.node, "tcStyle") else { continue };
                    if let Some(f) = child(cell_style, "fill").and_then(fill_element).and_then(|f| self.fill(ctx, f, rels, None, None)) {
                        if part.node.tag_name().name() == "wholeTbl" {
                            base_fill = f.clone();
                        }
                        fill = match (&f, &base_fill) {
                            (Fill::Solid(top), Fill::Solid(under)) if top.alpha < 255 && under.alpha == 255 => {
                                let a = top.alpha as f32 / 255.0;
                                let mix = |t: u8, u: u8| (t as f32 * a + u as f32 * (1.0 - a)) as u8;
                                Fill::Solid(ibis_doc_model::drawing::Color::opaque(ibis_doc_model::Rgb(
                                    (mix((top.rgb.0 >> 16) as u8, (under.rgb.0 >> 16) as u8) as u32) << 16
                                        | (mix((top.rgb.0 >> 8) as u8, (under.rgb.0 >> 8) as u8) as u32) << 8
                                        | mix(top.rgb.0 as u8, under.rgb.0 as u8) as u32,
                                )))
                            }
                            _ => f,
                        };
                    } else if child(cell_style, "fillRef").is_some() {
                        fill = self.style_fill(ctx, Some(cell_style), rels);
                    }
                    if let Some(bdr) = child(cell_style, "tcBdr") {
                        // Outer edges of the part's area, or the lines inside it.
                        let edge = |outer: bool, outer_name: &str, inner_name: &str| {
                            child(bdr, if outer { outer_name } else { inner_name })
                        };
                        let pick = |node: Option<Node>| node.map(|n| self.stroke(ctx, child(n, "ln"), Some(n)));
                        if let Some(s) = pick(edge(c == part.columns.0, "left", "insideV")) {
                            borders.left = s;
                        }
                        if let Some(s) = pick(edge(c == part.columns.1, "right", "insideV")) {
                            borders.right = s;
                        }
                        if let Some(s) = pick(edge(r == part.rows.0, "top", "insideH")) {
                            borders.top = s;
                        }
                        if let Some(s) = pick(edge(r == part.rows.1, "bottom", "insideH")) {
                            borders.bottom = s;
                        }
                        if let Some(s) = pick(child(bdr, "tl2br")) {
                            borders.tl_to_br = s;
                        }
                        if let Some(s) = pick(child(bdr, "tr2bl")) {
                            borders.bl_to_tr = s;
                        }
                    }
                }

                // The cell's own format wins over the style.
                let tc_pr = child(tc, "tcPr");
                if let Some(tc_pr) = tc_pr {
                    if let Some(f) = fill_element(tc_pr).and_then(|f| self.fill(ctx, f, rels, None, None)) {
                        fill = f;
                    }
                    for (name, target) in [
                        ("lnL", &mut borders.left),
                        ("lnR", &mut borders.right),
                        ("lnT", &mut borders.top),
                        ("lnB", &mut borders.bottom),
                        ("lnTlToBr", &mut borders.tl_to_br),
                        ("lnBlToTr", &mut borders.bl_to_tr),
                    ] {
                        if let Some(ln) = child(tc_pr, name) {
                            *target = self.stroke(ctx, Some(ln), None);
                        }
                    }
                }

                let mut text = match child(tc, "txBody") {
                    Some(body) => self.text_body(ctx, body, defaults, None, true),
                    None => TextBody::default(),
                };
                let margin = |name, default| tc_pr.and_then(|p| attr_i64(p, name)).unwrap_or(default);
                text.insets = Insets { left: margin("marL", 91_440), right: margin("marR", 91_440), top: margin("marT", 45_720), bottom: margin("marB", 45_720) };
                text.anchor = match tc_pr.and_then(|p| attr(p, "anchor")) {
                    Some("ctr") => VerticalAnchor::Middle,
                    Some("b") => VerticalAnchor::Bottom,
                    _ => VerticalAnchor::Top,
                };
                if let Some(vert) = tc_pr.and_then(|p| attr(p, "vert")) {
                    let kind = ibis_doc_model::VerticalKind::parse(vert);
                    text.vertical = kind.is_some();
                    text.vertical_kind = kind.unwrap_or_default();
                    text.sideways = match vert {
                        "vert" => 5_400_000,
                        "vert270" => 16_200_000,
                        _ => 0,
                    };
                }
                let span = |name| attr_i64(tc, name).unwrap_or(1).max(1) as u32;
                cells.push(TableCell {
                    text,
                    fill,
                    borders,
                    column_span: span("gridSpan"),
                    row_span: span("rowSpan"),
                    merged: attr_bool(tc, "hMerge").unwrap_or(false) || attr_bool(tc, "vMerge").unwrap_or(false),
                });
            }
            let source = Some(out.len());
            out.push(TableRow { height: attr_i64(*tr, "h").unwrap_or(0), cells, source });
        }
        let column_sources = (0..columns.len()).map(Some).collect();
        Some(Table { columns, rows: out, column_sources, style_options, style_id: style_id.map(str::to_string) })
    }
}

/// `text` split into stretches of East Asian and other characters (see
/// `RunProps::font_for`).
/// A hyperlink's target (`a:hlinkClick`): a move in the show
/// (`ppaction://hlinkshowjump?jump=…`), else its relationship's (a web page
/// or file, external; a slide of `slides`).
fn link_of(h: Node, rels: &Rels, slides: &HashMap<String, ibis_doc_model::SlideId>) -> Option<ibis_doc_model::SlideLink> {
    use ibis_doc_model::SlideLink;
    let action = attr(h, "action").unwrap_or("");
    if let Some(jump) = action.strip_prefix("ppaction://hlinkshowjump?jump=") {
        return match jump {
            "nextslide" => Some(SlideLink::NextSlide),
            "previousslide" => Some(SlideLink::PreviousSlide),
            "firstslide" => Some(SlideLink::FirstSlide),
            "lastslide" => Some(SlideLink::LastSlide),
            "lastslideviewed" => Some(SlideLink::LastSlideViewed),
            "endshow" => Some(SlideLink::EndShow),
            _ => None,
        };
    }
    let rel = attr_rid(h).and_then(|id| rels.get(id))?;
    if rel.external {
        return (action.is_empty()).then(|| SlideLink::Url(rel.target.clone()));
    }
    if action.is_empty() || action == "ppaction://hlinksldjump" {
        return slides.get(&rel.target).copied().map(SlideLink::Slide);
    }
    None
}

/// `text` split where the script changes, each piece East Asian or not
/// (as runs are read), for a date field's new text.
pub fn script_pieces(text: &str) -> Vec<(String, bool)> {
    script_segments(text).into_iter().map(|s| (s.to_string(), s.chars().next().is_some_and(crate::style::is_east_asian))).collect()
}

fn script_segments(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut last = None;
    for (i, c) in text.char_indices() {
        let east_asian = crate::style::is_east_asian(c);
        if last.is_some_and(|l| l != east_asian) {
            out.push(&text[start..i]);
            start = i;
        }
        last = Some(east_asian);
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// The 3D rotation, depth and extrusion of a shape (`a:scene3d` and `a:sp3d`).
fn tilt(sp_pr: Node, colors: &Colors) -> Option<ibis_doc_model::Tilt> {
    use ibis_doc_model::CameraPreset;
    let camera = path(sp_pr, &["scene3d", "camera"]);
    let sp3d = child(sp_pr, "sp3d");

    if camera.is_none() && sp3d.is_none() {
        return None;
    }

    let camera_preset = camera.and_then(|c| attr(c, "prst")).and_then(CameraPreset::from_str);
    let (def_lat, def_lon, def_rev) = camera_preset.map(|p| p.default_angles()).unwrap_or((0, 0, 0));

    let parse_angle = |v: i64| -> i32 {
        let m = 21_600_000i64;
        let norm = (v % m + m) % m;
        if norm > 10_800_000 {
            (norm - m) as i32
        } else {
            norm as i32
        }
    };
    let rot = camera.and_then(|c| child(c, "rot"));
    let lat = rot.and_then(|r| attr_i64(r, "lat")).map(parse_angle).unwrap_or(def_lat);
    let lon = rot.and_then(|r| attr_i64(r, "lon")).map(parse_angle).unwrap_or(def_lon);
    let rev = rot.and_then(|r| attr_i64(r, "rev")).map(parse_angle).unwrap_or(def_rev);

    let depth = sp3d.and_then(|s| attr_i64(s, "z")).unwrap_or(0);
    let extrusion_h = sp3d.and_then(|s| attr_i64(s, "extrusionH")).unwrap_or(0);
    let extrusion_color = sp3d.and_then(|s| child(s, "extrusionClr")).and_then(|c| colors.in_parent(c, None));

    if lat != 0 || lon != 0 || rev != 0 || depth != 0 || extrusion_h != 0 || camera_preset.is_some() || extrusion_color.is_some() {
        Some(ibis_doc_model::Tilt {
            lat,
            lon,
            rev,
            depth,
            extrusion_h,
            extrusion_color,
            camera_preset,
        })
    } else {
        None
    }
}

/// Empty columns and rows shown past a worksheet's used cells, and the
/// least a sheet shows (an empty one shows a screenful).
const GRID_MARGIN_COLUMNS: u32 = 26;
const GRID_MARGIN_ROWS: u32 = 200;
pub(crate) const MIN_GRID_COLUMNS: u32 = 52;
pub(crate) const MIN_GRID_ROWS: u32 = 300;

const REL_WORKSHEET: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
const REL_DRAWING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";

/// Whether the package's main part is a workbook (`xl/workbook.xml`).
pub(crate) fn is_workbook(data: &[u8]) -> Result<bool, Error> {
    let package = Package::new(data)?;
    Ok(package.related("", REL_OFFICE_DOCUMENT)?.is_some_and(|main| main.ends_with("workbook.xml")))
}

/// Read a `.xlsx` workbook: each visible worksheet becomes a slide the size
/// of its used range (and drawings), holding the cells as one [`Sheet`]
/// shape with the sheet's charts, pictures and shapes in front.
///
/// [`Sheet`]: ibis_doc_model::Sheet
pub(crate) fn read_xlsx(data: &[u8]) -> Result<Presentation, Error> {
    Ok(read_xlsx_with_sources(data)?.0)
}

/// [`read_xlsx`], and where the workbook's parts are, for saving.
pub(crate) fn read_xlsx_with_sources(data: &[u8]) -> Result<(Presentation, crate::save::xlsx::SourceMap), Error> {
    read_xlsx_with_records(data, None)
}

/// As `read_xlsx_with_sources`; `records` (a text file's) fill the cells of the one empty sheet in `data` (`csv::workbook_from("", ..)`) without being written into it and read back.
pub(crate) fn read_xlsx_with_records(data: &[u8], records: Option<&[Vec<String>]>) -> Result<(Presentation, crate::save::xlsx::SourceMap), Error> {
    use crate::xlsx;

    let package = Package::new(data)?;
    let main = package.related("", REL_OFFICE_DOCUMENT)?.ok_or_else(|| Error::new("ブック本体が見つかりません"))?;
    let main_rels = package.relationships(&main)?;
    let main_xml = package.read_xml(&main)?;
    let main_doc = parse(&main, &main_xml)?;
    let root = main_doc.root_element();
    let part_of = |suffix: &str| main_rels.values().find(|r| !r.external && r.kind.ends_with(suffix)).map(|r| r.target.clone());
    let read_optional = |part: Option<String>| -> Option<(String, String)> {
        let part = part?;
        let text = package.read_xml(&part).ok()?;
        Some((part, text))
    };
    let theme_xml = read_optional(part_of("/theme"));
    let styles_xml = read_optional(part_of("/styles"));
    let strings_xml = read_optional(part_of("/sharedStrings"));
    let theme_doc = theme_xml.as_ref().map(|(p, t)| parse(p, t)).transpose()?;
    let styles_doc = styles_xml.as_ref().map(|(p, t)| parse(p, t)).transpose()?;
    let strings_doc = strings_xml.as_ref().map(|(p, t)| parse(p, t)).transpose()?;
    let theme = theme_doc.as_ref().map(|d| Theme::read(d.root_element())).unwrap_or_default();
    let sheet_colors = xlsx::SheetColors::new(&theme, styles_doc.as_ref().map(|d| d.root_element()));
    let mut styles = xlsx::read_styles(styles_doc.as_ref().map(|d| d.root_element()), &sheet_colors);
    styles.date1904 = child(root, "workbookPr").and_then(|p| attr_bool(p, "date1904")).unwrap_or(false);
    let strings = xlsx::read_shared_strings(strings_doc.as_ref().map(|d| d.root_element()));
    let phonetics = xlsx::read_shared_extras(strings_doc.as_ref().map(|d| d.root_element()), &sheet_colors);
    // The people of the threaded comments.
    let persons = main_rels
        .values()
        .find(|r| !r.external && r.kind == crate::package::REL_PERSONS)
        .and_then(|r| package.read_xml(&r.target).ok())
        .and_then(|xml| roxmltree::Document::parse(&xml).ok().map(|d| xlsx::read_persons(d.root_element())))
        .unwrap_or_default();
    const REL_METADATA: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata";
    let metadata_parts: Vec<_> = main_rels.values().filter(|r| !r.external && r.kind == REL_METADATA).collect();
    let dynamic_metadata = if let Some(metadata) = metadata_parts.first() {
        package.read_xml(&metadata.target).ok().and_then(|xml| parse(&metadata.target, &xml).ok()
            .map(|doc| xlsx::dynamic_metadata_indices(doc.root_element()))).unwrap_or_default()
    } else { Default::default() };
    // Errors kept in rich values (#CALC!, #SPILL!), stored as #VALUE!.
    if let Some(metadata) = metadata_parts.first() {
        const REL_RICH_VALUE: &str = "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValue";
        const REL_RICH_STRUCTURE: &str = "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueStructure";
        let meta_rels = package.relationships(&metadata.target).unwrap_or_default();
        let part = |kind: &str| main_rels.values().chain(meta_rels.values()).find(|r| !r.external && (r.kind == kind || r.kind.ends_with(kind.rsplit_once('/').map_or(kind, |(_, s)| s)))).and_then(|r| package.read_xml(&r.target).ok());
        if let (Ok(meta), Some(values), Some(structures)) = (package.read_xml(&metadata.target), part(REL_RICH_VALUE), part(REL_RICH_STRUCTURE))
            && let (Ok(meta), Ok(values), Ok(structures)) = (roxmltree::Document::parse(&meta), roxmltree::Document::parse(&values), roxmltree::Document::parse(&structures))
        {
            styles.rich_errors = xlsx::rich_value_errors(meta.root_element(), values.root_element(), structures.root_element());
            styles.rich_images = xlsx::rich_value_images(meta.root_element(), values.root_element(), structures.root_element());
        }
    }
    // The maximum digit width of the default font, in points (`xlsx::EXCEL_METRICS`).
    let digit = xlsx::EXCEL_METRICS.digit_pt(styles.default_font.0.as_deref(), styles.default_font.1 as f64 / 100.0);

    // Print areas by the sheet's position in the workbook.
    let print_areas: HashMap<usize, ibis_doc_model::CellRange> = child(root, "definedNames")
        .into_iter()
        .flat_map(|d| children(d, "definedName"))
        .filter(|d| attr(*d, "name") == Some("_xlnm.Print_Area"))
        .filter_map(|d| Some((attr_i64(d, "localSheetId")? as usize, xlsx::parse_range(d.text()?)?)))
        .collect();
    // Every sheet of the workbook, in order: its name, state and part (a
    // worksheet's), for formulas and saving.
    let sheet_ids: Vec<u32> = child(root, "sheets")
        .into_iter()
        .flat_map(|s| children(s, "sheet"))
        .enumerate()
        .map(|(i, s)| attr_i64(s, "sheetId").map_or(i as u32 + 1, |v| v.max(0) as u32))
        .collect();
    let all_sheets: Vec<(String, ibis_doc_model::SheetState, Option<String>)> = child(root, "sheets")
        .into_iter()
        .flat_map(|s| children(s, "sheet"))
        .map(|s| {
            let state = match attr(s, "state") {
                Some("hidden") => ibis_doc_model::SheetState::Hidden,
                Some("veryHidden") => ibis_doc_model::SheetState::VeryHidden,
                _ => ibis_doc_model::SheetState::Visible,
            };
            let part = attr_rid(s).and_then(|rid| main_rels.get(rid)).filter(|rel| rel.kind == REL_WORKSHEET && !rel.external).map(|rel| rel.target.clone());
            (attr(s, "name").unwrap_or("").to_string(), state, part)
        })
        .collect();
    // The external workbooks the formulas refer to (`[1]`…), with their cached cells.
    let external_books: Vec<ibis_doc_model::ExternalBook> = child(root, "externalReferences")
        .into_iter()
        .flat_map(|e| children(e, "externalReference"))
        .map(|r| {
            let Some(rel) = attr_rid(r).and_then(|rid| main_rels.get(rid)).filter(|rel| !rel.external) else { return Default::default() };
            let Ok(xml) = package.read_xml(&rel.target) else { return Default::default() };
            let Ok(doc) = parse(&rel.target, &xml) else { return Default::default() };
            let target = package.relationships(&rel.target).ok().and_then(|rels| rels.values().find(|r| r.kind.ends_with("/externalLinkPath")).map(|r| r.target.clone()));
            crate::xlsx_external::read_link(doc.root_element(), target.as_deref())
        })
        .collect();
    // The クエリ エディター's queries (a Custom XML Data part in our namespace).
    let (queries, query_parameters) = main_rels
        .values()
        .filter(|r| !r.external && r.kind.ends_with("/customXml"))
        .filter_map(|r| package.read(&r.target).ok())
        .filter_map(|bytes| String::from_utf8(bytes).ok())
        .find(|text| text.contains("https://ibis.ne.jp/ibisoffice/queries"))
        .and_then(|text| {
            let doc = roxmltree::Document::parse(&text).ok()?;
            Some(ibis_doc_model::query::decode_queries(doc.root_element().text().unwrap_or("")))
        })
        .unwrap_or_default();
    let book_names: Vec<String> = external_books.iter().map(|b| b.name.clone()).collect();
    // Each link's part (`xl/externalLinks/externalLink1.xml`), in the same order, for saving.
    let external_parts: Vec<Option<String>> = child(root, "externalReferences")
        .into_iter()
        .flat_map(|e| children(e, "externalReference"))
        .map(|r| attr_rid(r).and_then(|rid| main_rels.get(rid)).filter(|rel| !rel.external).map(|rel| rel.target.clone()))
        .collect();
    // The slicers' caches (what each filters), and the tables by their id.
    let slicer_caches: HashMap<String, crate::xlsx_slicer::SlicerCache> = main_rels
        .values()
        .filter(|r| !r.external && r.kind.ends_with("/slicerCache"))
        .filter_map(|r| package.read_xml(&r.target).ok())
        .filter_map(|xml| parse("slicerCache", &xml).ok().and_then(|d| crate::xlsx_slicer::read_cache(d.root_element())))
        .map(|c| (c.name.clone(), c))
        .collect();
    let timeline_caches: HashMap<String, crate::xlsx_timeline::TimelineCache> = main_rels
        .values()
        .filter(|r| !r.external && r.kind.ends_with("/timelineCache"))
        .filter_map(|r| package.read_xml(&r.target).ok().map(|x| (r.target.clone(), x)))
        .filter_map(|(part, xml)| parse("timelineCache", &xml).ok().and_then(|d| crate::xlsx_timeline::read_cache(d.root_element())).map(|mut c| {
            c.part = part;
            c
        }))
        .map(|c| (c.name.clone(), c))
        .collect();
    let mut slicer_tables: HashMap<u32, String> = HashMap::new();
    if !slicer_caches.is_empty() {
        for part in all_sheets.iter().filter_map(|(_, _, p)| p.as_ref()) {
            let Ok(rels) = package.relationships(part) else { continue };
            for rel in rels.values().filter(|r| !r.external && r.kind.ends_with("/table")) {
                let Ok(xml) = package.read_xml(&rel.target) else { continue };
                let Ok(doc) = parse(&rel.target, &xml) else { continue };
                let t = doc.root_element();
                if let (Some(id), Some(name)) = (attr_i64(t, "id"), attr(t, "displayName").or_else(|| attr(t, "name"))) {
                    slicer_tables.insert(id.max(0) as u32, name.to_string());
                }
            }
        }
    }
    // The files of the pictures in cells (セルに配置): `richValueRel.xml`'s relationships, in order.
    let rich_rel_part = main_rels.values().find(|r| !r.external && r.kind.ends_with("/richValueRel")).map(|r| r.target.clone());
    let rich_rel_rids: Vec<String> = rich_rel_part
        .as_ref()
        .and_then(|p| package.read_xml(p).ok())
        .and_then(|xml| parse("richValueRel", &xml).ok().map(|d| elements(d.root_element()).filter(|n| n.tag_name().name() == "rel").filter_map(|n| attr_rid(n).map(str::to_string)).collect()))
        .unwrap_or_default();
    let rich_rel_rels = rich_rel_part.as_ref().and_then(|p| package.relationships(p).ok()).unwrap_or_default();
    let sheets: Vec<(String, String, Option<ibis_doc_model::CellRange>)> = child(root, "sheets")
        .into_iter()
        .flat_map(|s| children(s, "sheet"))
        .enumerate()
        .filter(|(_, s)| !matches!(attr(*s, "state"), Some("hidden" | "veryHidden")))
        .filter_map(|(index, s)| {
            let rel = attr_rid(s).and_then(|rid| main_rels.get(rid))?;
            (rel.kind == REL_WORKSHEET && !rel.external)
                .then(|| (attr(s, "name").unwrap_or("").to_string(), rel.target.clone(), print_areas.get(&index).copied()))
        })
        .collect();
    if sheets.is_empty() {
        return Err(Error::new("シートがありません"));
    }

    // The sheets' XML unpacked and parsed, and their cells read, on several
    // threads (most of the time of opening a large workbook); the drawings
    // follow in order.
    let xmls = par_map(&sheets, |(_, part, _)| package.read_xml(part));
    let xmls = xmls.into_iter().collect::<Result<Vec<String>, Error>>()?;
    // For the progress bar (the shares of a large workbook's time, measured): unpacking, parsing, then the rows.
    crate::progress::fraction(0.05);
    crate::progress::rows_begin(xmls.iter().map(|x| x.len() as u64).sum(), 0.18, 0.96);
    let indexed: Vec<(usize, &String)> = xmls.iter().enumerate().collect();
    // The biggest sheets first: they are what the threads end on otherwise.
    let read = par_map_largest_first(&indexed, |(_, xml)| xml.len(), |(i, xml)| {
        let doc = parse(&sheets[*i].1, xml)?;
        crate::progress::fraction(0.18);
        let mut sheet = xlsx::read_sheet(doc.root_element(), &styles, &strings, &phonetics, digit, &sheet_colors, &dynamic_metadata);
        if let Some(records) = records {
            xlsx::fill_csv_sheet(&mut sheet, doc.root_element(), &styles, digit, records);
        }
        // Sparkline data on the sheet itself, named or not.
        for sp in sheet.sparklines.iter_mut().flat_map(|g| g.sparklines.iter_mut()) {
            if sp.sheet.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&sheets[*i].0)) {
                sp.sheet = None;
            }
        }
        Ok::<_, Error>((doc, sheet))
    });
    let read = read.into_iter().collect::<Result<Vec<_>, Error>>()?;
    // The new chart kinds (chartEx) name their data (`_xlchart.v1.0`): the cells of every sheet to read it from, and the
    // defined names the references are.
    let has_chart_ex = package.read_xml("[Content_Types].xml").map(|t| t.contains("chartex")).unwrap_or(false);
    let chart_ex_sheets: Vec<(String, ibis_doc_model::Sheet)> = if has_chart_ex { sheets.iter().zip(&read).map(|((name, ..), (_, sheet))| (name.to_lowercase(), sheet.clone())).collect() } else { Vec::new() };
    let chart_ex_names: HashMap<String, String> = if has_chart_ex {
        child(root, "definedNames").into_iter().flat_map(|d| children(d, "definedName")).filter_map(|d| Some((attr(d, "name")?.to_string(), d.text()?.to_string()))).collect()
    } else {
        HashMap::new()
    };
    let mut read = read.into_iter();

    let mut presentation = Presentation::empty(EmuSize::WIDESCREEN);
    let mut chart_sources: Vec<crate::save::xlsx::ChartSource> = Vec::new();
    let mut xlsx_shape_sources: Vec<crate::save::xlsx::XlsxShapeSource> = Vec::new();
    let mut media = HashMap::new();
    let mut slide_of_part: HashMap<String, ibis_doc_model::SlideId> = HashMap::new();
    for (number, (name, part, print_area)) in sheets.iter().enumerate() {
        let (doc, mut sheet) = read.next().expect("a sheet read for each");
        let sheet_root = doc.root_element();
        sheet.print_area = *print_area;
        name_external_books(&mut sheet, &book_names);
        sheet.tab_color = child(sheet_root, "sheetPr").and_then(|p| child(p, "tabColor")).and_then(|c| sheet_colors.color(c));

        // The sheet's drawing and the shapes over its charts.
        let sheet_rels = package.relationships(part)?;
        // The slicers on the sheet (their drawing places them below).
        let mut slicer_parts: Vec<&String> = sheet_rels.values().filter(|r| !r.external && r.kind.ends_with("/slicer")).map(|r| &r.target).collect();
        slicer_parts.sort();
        let sheet_slicers: Vec<ibis_doc_model::slicer::Slicer> = slicer_parts
            .into_iter()
            .filter_map(|p| package.read_xml(p).ok())
            .flat_map(|xml| parse("slicers", &xml).map(|d| crate::xlsx_slicer::read_slicers(d.root_element(), &slicer_caches, &slicer_tables)).unwrap_or_default())
            .collect();

        let sheet_timelines: Vec<ibis_doc_model::timeline::Timeline> = sheet_rels
            .values()
            .filter(|r| !r.external && r.kind.ends_with("/timeline"))
            .filter_map(|r| package.read_xml(&r.target).ok().map(|x| (r.target.clone(), x)))
            .flat_map(|(part, xml)| parse("timelines", &xml).map(|d| crate::xlsx_timeline::read_timelines(d.root_element(), &part, &timeline_caches)).unwrap_or_default())
            .collect();

        // Hyperlinks, their web addresses from the sheet's relationships.
        sheet.hyperlinks = child(sheet_root, "hyperlinks")
            .into_iter()
            .flat_map(|h| children(h, "hyperlink"))
            .filter_map(|h| {
                Some(ibis_doc_model::Hyperlink {
                    range: attr(h, "ref").and_then(xlsx::parse_range)?,
                    target: attr_rid(h).and_then(|rid| sheet_rels.get(rid)).map(|r| r.target.clone()),
                    location: attr(h, "location").map(str::to_string),
                    tooltip: attr(h, "tooltip").map(str::to_string),
                })
            })
            .collect();
        // Comments on the sheet's cells.
        for rel in sheet_rels.values() {
            if rel.kind == crate::package::REL_COMMENTS && !rel.external {
                if let Ok(xml) = package.read_xml(&rel.target) {
                    if let Ok(doc) = roxmltree::Document::parse(&xml) {
                        sheet.comments = xlsx::read_comments(doc.root_element());
                    }
                }
            }
            if rel.kind == crate::package::REL_THREADED_COMMENTS && !rel.external {
                if let Ok(xml) = package.read_xml(&rel.target) {
                    if let Ok(doc) = roxmltree::Document::parse(&xml) {
                        sheet.threads = xlsx::read_threads(doc.root_element(), &persons);
                    }
                }
            }
        }
        // Pivot tables, each with its cache's definition.
        let mut pivot_parts: Vec<&String> = sheet_rels.values().filter(|r| !r.external && r.kind.ends_with("/pivotTable")).map(|r| &r.target).collect();
        pivot_parts.sort();
        for part in pivot_parts {
            let Ok(xml) = package.read_xml(part) else { continue };
            let Ok(doc) = roxmltree::Document::parse(&xml) else { continue };
            let cache_part = package.relationships(part).ok().and_then(|rels| rels.values().find(|r| !r.external && r.kind.ends_with("/pivotCacheDefinition")).map(|r| r.target.clone()));
            let cache_xml = cache_part.as_ref().and_then(|p| package.read_xml(p).ok());
            let cache_doc = cache_xml.as_deref().and_then(|x| roxmltree::Document::parse(x).ok());
            if let Some(pivot) = crate::xlsx_pivot::read_pivot(doc.root_element(), cache_doc.as_ref().map(|d| d.root_element()), part, cache_part.as_deref()) {
                sheet.pivots.push(pivot);
            }
        }
        // Table styles and borders from table parts (xl/tables/table*.xml).
        let table_parts: Vec<String> = child(sheet_root, "tableParts")
            .into_iter()
            .flat_map(|tp| children(tp, "tablePart"))
            .filter_map(|p| attr_rid(p).and_then(|rid| sheet_rels.get(rid)))
            .filter(|r| !r.external)
            .map(|r| r.target.clone())
            .collect();
        let mut table_xmls: Vec<String> = Vec::new();
        for tp in &table_parts {
            if let Ok(xml) = package.read_xml(tp) {
                table_xmls.push(xml);
            }
        }
        if !table_xmls.is_empty() {
            let xml_refs: Vec<&str> = table_xmls.iter().map(String::as_str).collect();
            xlsx::apply_tables(&mut sheet, &xml_refs, &styles.dxfs);
            // The rows the tables' filters hide, as the file has them hidden.
            let mut hidden: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
            for t in &sheet.tables {
                if let Some(f) = &t.filter {
                    hidden.extend(ibis_doc_model::sheet_command::filtered_rows(&sheet, f, styles.date1904).into_iter().filter(|&row| sheet.rows.get(row as usize) == Some(&0)));
                }
            }
            sheet.filtered_rows.extend(hidden);
            crate::table_style::apply_table_styles(&mut sheet, &xml_refs, styles_doc.as_ref().map(|d| d.root_element()), &sheet_colors);
        }

        let drawing = child(sheet_root, "drawing")
            .and_then(attr_rid)
            .and_then(|rid| sheet_rels.get(rid))
            .filter(|r| r.kind == REL_DRAWING && !r.external)
            .map(|r| r.target.clone());
        let mut xml_parts: HashMap<String, String> = HashMap::new();
        let mut chart_drawings: HashMap<String, String> = HashMap::new();
        let mut chart_parts: Vec<String> = Vec::new();
        // SmartArt graphics' drawings (`dsp:drawing`), by their data part (as on a slide).
        let mut diagram_drawings: HashMap<String, String> = HashMap::new();
        if let Some(drawing) = &drawing {
            xml_parts.insert(drawing.clone(), package.read_xml(drawing)?);
            let drawing_rels = package.relationships(drawing)?;
            for rel in drawing_rels.values().filter(|r| !r.external && r.kind.ends_with("/diagramData")) {
                let Ok(data) = package.read_xml(&rel.target) else { continue };
                let Some(id) = data.split("dataModelExt").nth(1).and_then(|s| s.split("relId=\"").nth(1)).and_then(|s| s.split('"').next()) else { continue };
                let Some(shapes) = drawing_rels.get(id).filter(|r| !r.external && r.kind.ends_with("/diagramDrawing")).map(|r| r.target.clone()) else { continue };
                if let Ok(text) = package.read_xml(&shapes) {
                    diagram_drawings.insert(rel.target.clone(), shapes.clone());
                    xml_parts.insert(shapes, text);
                }
            }
            for rel in package.relationships(drawing)?.values().filter(|r| !r.external && (r.kind.ends_with("/chart") || r.kind.ends_with("/chartEx"))) {
                chart_parts.push(rel.target.clone());
                if let Ok(Some(shapes)) = package.related(&rel.target, REL_CHART_USER_SHAPES)
                    && let Ok(text) = package.read_xml(&shapes)
                {
                    chart_drawings.insert(rel.target.clone(), shapes.clone());
                    xml_parts.insert(shapes, text);
                }
            }
        }
        let mut docs: HashMap<&str, Document> = HashMap::new();
        for (name, text) in &xml_parts {
            docs.insert(name.as_str(), parse(name, text)?);
        }
        // The sheet's charts read on several threads (a sheet of charts
        // with thousands of points each), for the drawing to take in order.
        let chart_ex_names_shared = std::sync::Arc::new(chart_ex_names.clone());
        let theme_shared = std::sync::Arc::new(theme.clone());
        let charts: HashMap<String, Option<ibis_doc_model::Chart>> = par_map(&chart_parts, |part| {
            let colors = Colors::new(&theme, None, &[]);
            let xml = package.read_xml(part).ok();
            let raw_xml = xml.clone().filter(|x| has_chart_ex && x.contains("chartex"));
            // A chartEx's data named by references: read from the cells.
            let xml = xml.map(|x| {
                if !has_chart_ex || !x.contains("chartex") {
                    return x;
                }
                crate::chart_ex::fill_refs(&x, &chart_ex_names, &|sheet: &str, range: ibis_doc_model::CellRange| {
                    let (_, sh) = chart_ex_sheets.iter().find(|(n, _)| *n == sheet.to_lowercase())?;
                    let last_row = range.last_row.min(sh.cells.iter().map(|c| c.row).max().unwrap_or(0));
                    let last_column = range.last_column.min(sh.cells.iter().map(|c| c.column).max().unwrap_or(0));
                    let mut grid = Vec::new();
                    for row in range.first_row..=last_row {
                        let mut line = Vec::new();
                        for column in range.first_column..=last_column {
                            line.push(match sh.cell(row, column) {
                                Some(c) => (c.text.clone(), if let ibis_doc_model::CellValue::Number(n) = c.value { Some(n) } else { None }, sh.styles.get(c.style as usize).map(|s| s.number_format.clone()).unwrap_or_else(|| "General".into())),
                                None => (String::new(), None, "General".into()),
                            });
                        }
                        grid.push(line);
                    }
                    Some(grid)
                })
            });
            let chart = xml.as_deref().and_then(|xml| parse(part, xml).ok()).and_then(|doc| {
                let mut chart = crate::chart::read(doc.root_element(), &colors)?;
                // A worksheet's chart is laid out as Excel's engine does
                // (Word's and PowerPoint's differ: DCHT and PCHT).
                chart.excel_layout = true;
                // On a sheet, a chart area with no format of its own has
                // Excel's thin grey border (in a slide it has none).
                if child(doc.root_element(), "spPr").is_none() && chart.border.is_none() {
                    chart.border = Some(ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x868686)), 9_525));
                }
                // A chartEx chart goes on from its part's XML: made again as its cells change.
                if chart.ex.is_some() || raw_xml.is_some() {
                    if let Some(raw) = raw_xml {
                        chart.ex_source = Some(ibis_doc_model::ExSourceRef(std::sync::Arc::new(crate::chart_ex::ExSourceImpl::new(raw, chart_ex_names_shared.clone(), theme_shared.clone())), true));
                    }
                }
                Some(chart)
            });
            (part.clone(), chart)
        })
        .into_iter()
        .collect();
        let mut rels: HashMap<&str, Rels> = HashMap::new();
        for name in xml_parts.keys() {
            rels.insert(name.as_str(), package.relationships(name)?);
        }

        let anchors: Vec<Node> = drawing
            .as_deref()
            .map(|d| docs[d].root_element())
            .into_iter()
            .flat_map(drawing_anchors)
            .collect();
        // Cover the cells the drawings reach, then place them.
        let (mut columns, mut rows) = (0u32, 0u32);
        for anchor in &anchors {
            for corner in [child(*anchor, "from"), child(*anchor, "to")].into_iter().flatten() {
                let (c, r) = anchor_cell(corner);
                columns = columns.max(c.0 + 1);
                rows = rows.max(r.0 + 1);
            }
        }
        // The print area covers its cells (whole rows or columns only as far
        // as the sheet goes).
        if let Some(area) = print_area {
            if area.last_column < xlsx::OPEN_END {
                columns = columns.max(area.last_column + 1);
            }
            if area.last_row < xlsx::OPEN_END {
                rows = rows.max(area.last_row + 1);
            }
        }
        xlsx::extend_sheet(&mut sheet, sheet_root, columns, rows, digit, (&styles.default_font.0, styles.default_font.1));
        // What the sheet uses: the page printed and exported.
        // The rows holding only a height (no cell) stay in the sheet but are not used: Excel prints no page for them.
        let (xs_used, ys_used) = (sheet.column_offsets(), sheet.row_offsets());
        let used_rows = if *sheet.printed_rows > 0 { ((*sheet.printed_rows).max(rows) as usize).min(ys_used.len() - 1) } else { ys_used.len() - 1 };
        let used = (*xs_used.last().unwrap_or(&0), ys_used[used_rows]);
        // Beyond it the grid goes on, as in Excel, for text running on past
        // the last cell and for new values.
        xlsx::extend_sheet(&mut sheet, sheet_root, (columns + GRID_MARGIN_COLUMNS).max(MIN_GRID_COLUMNS), (rows + GRID_MARGIN_ROWS).max(MIN_GRID_ROWS), digit, (&styles.default_font.0, styles.default_font.1));
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let at = |(c, r): ((u32, i64), (u32, i64))| -> (i64, i64) {
            let x = xs[(c.0 as usize).min(xs.len() - 1)] + c.1;
            let y = ys[(r.0 as usize).min(ys.len() - 1)] + r.1;
            (x, y)
        };
        let grid = (*xs.last().unwrap_or(&0), *ys.last().unwrap_or(&0));
        let mut extent = used;

        let cell_images_of_sheet = sheet.cell_images.clone();
        let mut reader = Reader {
            package: &package,
            presentation: std::mem::replace(&mut presentation, Presentation::empty(EmuSize::WIDESCREEN)),
            media: std::mem::take(&mut media),
            default_text: None,
            table_styles: None,
            user_shapes: chart_drawings.iter().map(|(chart, d)| (chart.clone(), (docs[d.as_str()].root_element(), rels[d.as_str()].clone()))).collect(),
            diagrams: diagram_drawings.iter().map(|(data, d)| (data.clone(), (docs[d.as_str()].root_element(), rels[d.as_str()].clone()))).collect(),
            native_ids: HashMap::new(),
            last_native_ids: HashMap::new(),
            pending: Vec::new(),
            default_font_size: 1100,
            anchor: None,
            charts: std::cell::RefCell::new(charts),
            sources: Default::default(),
            text_sources: None,
            para_sources: Vec::new(),
            slide_placeholders: HashMap::new(),
        slide_names: HashMap::new(),
        slide_alt_texts: HashMap::new(),
        slide_decorative: Default::default(),
        slide_return_to_zoom: Default::default(),
        slide_zoom_show_bg: HashMap::new(),
        slide_zoom_durations: HashMap::new(),
        slide_hidden: Default::default(),
        slide_links: HashMap::new(),
        slide_link_tooltips: HashMap::new(),
        slide_aspect: Default::default(),
        slide_formulas: HashMap::new(),
        slide_glues: Vec::new(),
        slide_parts: HashMap::new(),
            chart_sources: Vec::new(),
            xlsx_shape_sources: Vec::new(),
        };
        let id = reader.presentation.new_slide_id();
        slide_of_part.insert(part.clone(), id);
        let mut slide = Slide::new(id);
        slide.name = Some(name.clone());
        let sheet_id = reader.presentation.new_shape_id();
        let cell_images = cell_images_of_sheet.clone();
        slide.shapes.push(Shape {
            id: sheet_id,
            frame: EmuRect::new(0, 0, grid.0, grid.1),
            rotation: 0,
            flip_h: false,
            flip_v: false,
            kind: ShapeKind::Sheet(std::sync::Arc::new(sheet)),
            floating: None,
        });
        if let Some(drawing) = &drawing {
            let drawing_part = || Part { name: drawing.clone(), root: docs[drawing.as_str()].root_element(), rels: rels[drawing.as_str()].clone() };
            let ctx = SlideCtx {
                colors: Colors::new(&theme, None, &[]),
                theme: theme_doc.as_ref().map(|d| d.root_element()),
                slide: drawing_part(),
                layout: drawing_part(),
                master: drawing_part(),
                number: number + 1,
            };
            for anchor in &anchors {
                let rect = match anchor.tag_name().name() {
                    "twoCellAnchor" => {
                        let (Some(from), Some(to)) = (child(*anchor, "from"), child(*anchor, "to")) else { continue };
                        let ((x0, y0), (x1, y1)) = (at(anchor_cell(from)), at(anchor_cell(to)));
                        EmuRect::new(x0, y0, (x1 - x0).max(0), (y1 - y0).max(0))
                    }
                    "oneCellAnchor" => {
                        let (Some(from), Some(ext)) = (child(*anchor, "from"), child(*anchor, "ext")) else { continue };
                        let (x, y) = at(anchor_cell(from));
                        let (w, h) = point(ext, "cx", "cy");
                        EmuRect::new(x, y, w, h)
                    }
                    "absoluteAnchor" => {
                        let (Some(pos), Some(ext)) = (child(*anchor, "pos"), child(*anchor, "ext")) else { continue };
                        let ((x, y), (w, h)) = (point(pos, "x", "y"), point(ext, "cx", "cy"));
                        EmuRect::new(x, y, w, h)
                    }
                    _ => continue,
                };
                extent = (extent.0.max(rect.right()), extent.1.max(rect.bottom()));
                // A timeline likewise (`tsle:timeslicer`).
                if let Some(timeline) = crate::xlsx_timeline::drawing_timeline_name(*anchor).and_then(|name| sheet_timelines.iter().find(|t| t.name == name)) {
                    let id = reader.presentation.new_shape_id();
                    slide.shapes.push(Shape { id, frame: rect, rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Timeline(std::sync::Arc::new(timeline.clone())), floating: None });
                    slide.shape_placements.insert(id, ibis_doc_model::ObjectPlacement::MoveAndSize);
                    continue;
                }
                // A slicer is drawn by the app from its definition (Excel puts the
                // `mc:AlternateContent` with its `sle:slicer` inside the anchor).
                if let Some(slicer) = crate::xlsx_slicer::drawing_slicer_name(*anchor).and_then(|name| sheet_slicers.iter().find(|s| s.name == name)) {
                    let id = reader.presentation.new_shape_id();
                    let placement = match anchor.tag_name().name() {
                        "twoCellAnchor" => match unqualified_attribute(*anchor, "editAs") {
                            Some("oneCell") => ibis_doc_model::ObjectPlacement::MoveOnly,
                            Some("absolute") => ibis_doc_model::ObjectPlacement::Free,
                            _ => ibis_doc_model::ObjectPlacement::MoveAndSize,
                        },
                        "oneCellAnchor" => ibis_doc_model::ObjectPlacement::MoveOnly,
                        "absoluteAnchor" => ibis_doc_model::ObjectPlacement::Free,
                        _ => ibis_doc_model::ObjectPlacement::MoveAndSize,
                    };
                    let print = child(*anchor, "clientData").and_then(|cd| unqualified_attribute(cd, "fPrintsWithSheet")).map(|v| v != "0" && v != "false").unwrap_or(true);
                    slide.shapes.push(Shape { id, frame: rect, rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Slicer(std::sync::Arc::new(slicer.clone())), floating: None });
                    slide.shape_placements.insert(id, placement);
                    slide.shape_prints.insert(id, print);
                    continue;
                }
                // The shape of the anchor: its own, or in the Choice of a
                // markup-compatibility wrapper (Excel wraps a chartEx so).
                let is_shape = |n: &Node| matches!(n.tag_name().name(), "sp" | "grpSp" | "cxnSp" | "pic" | "graphicFrame");
                let Some(shape) = elements(*anchor).find(|n| is_shape(n)).or_else(|| {
                    elements(*anchor)
                        .filter(|n| n.tag_name().name() == "AlternateContent")
                        .find_map(|ac| child(ac, "Choice").and_then(|c| elements(c).find(|n| is_shape(n))).or_else(|| child(ac, "Fallback").and_then(|c| elements(c).find(|n| is_shape(n)))))
                }) else {
                    continue;
                };
                // Map the shape's own box onto the anchor's.
                let xfrm = elements(shape).find(|n| matches!(n.tag_name().name(), "spPr" | "grpSpPr")).and_then(|p| child(p, "xfrm"));
                let (off, ext) = xfrm.map(offset_extent).unwrap_or(((0, 0), (rect.width, rect.height)));
                // A shape turned nearer a quarter turn than not (45° to 135°,
                // 225° to 315°) is anchored by its turned box: the anchor's
                // width is the shape's height and its height the width, about
                // the same centre (a text box at rot="16200000" with xfrm
                // 1187537 × 218016 sits in an anchor 218016 wide).
                let quarter = xfrm.and_then(|x| attr_i64(x, "rot")).is_some_and(|r| {
                    let d = r.rem_euclid(21_600_000);
                    (2_700_000..8_100_000).contains(&d) || (13_500_000..18_900_000).contains(&d)
                });
                let rect = if quarter {
                    let (cx, cy) = (rect.x + rect.width / 2, rect.y + rect.height / 2);
                    EmuRect::new(cx - rect.height / 2, cy - rect.width / 2, rect.height, rect.width)
                } else {
                    rect
                };
                let scale = |e: i64, c: i64| if c == 0 { 1.0 } else { e as f64 / c as f64 };
                let (sx, sy) = (scale(rect.width, ext.0), scale(rect.height, ext.1));
                let transform = Transform { sx, sy, tx: rect.x as f64 - off.0 as f64 * sx, ty: rect.y as f64 - off.1 as f64 * sy, turns: [None; MAX_TURNS], groups: [None; MAX_TURNS] };
                let placement = match anchor.tag_name().name() {
                    "twoCellAnchor" => match unqualified_attribute(*anchor, "editAs") {
                        Some("oneCell") => ibis_doc_model::ObjectPlacement::MoveOnly,
                        Some("absolute") => ibis_doc_model::ObjectPlacement::Free,
                        _ => ibis_doc_model::ObjectPlacement::MoveAndSize,
                    },
                    "oneCellAnchor" => ibis_doc_model::ObjectPlacement::MoveOnly,
                    "absoluteAnchor" => ibis_doc_model::ObjectPlacement::Free,
                    _ => ibis_doc_model::ObjectPlacement::MoveAndSize,
                };
                let print = child(*anchor, "clientData")
                    .and_then(|cd| unqualified_attribute(cd, "fPrintsWithSheet"))
                    .map(|v| v != "0" && v != "false")
                    .unwrap_or(true);
                if shape.has_tag_name("graphicFrame") || shape.has_tag_name("pic") {
                    reader.anchor = Some(rect);
                }
                let start_shapes = slide.shapes.len();
                reader.shapes(&ctx, *anchor, &ctx.slide.rels, Level::Slide, transform, None, &mut slide.shapes);
                for s in &slide.shapes[start_shapes..] {
                    slide.shape_placements.insert(s.id, placement);
                    slide.shape_prints.insert(s.id, print);
                }
                reader.anchor = None;
            }
        }
        // The shapes of SmartArt graphics (and of charts' own shapes) belong to their frame.
        for shape in &slide.shapes {
            if let Some(attached) = reader.sources.attached.get(&shape.id) {
                slide.attached.insert(shape.id, attached.clone());
            }
        }
        // Pictures in cells: each drawn in its cell (scaled to fit, centred).
        for image in &cell_images {
            let Some(rid) = rich_rel_rids.get(image.index as usize) else { continue };
            let Some((media, _)) = reader.media(&rich_rel_rels, rid) else { continue };
            let Some(rel) = rich_rel_rels.get(rid.as_str()) else { continue };
            let size = package.read(&rel.target).ok().and_then(|d| image_pixels(&d));
            let ((x0, y0), (x1, y1)) = (at(((image.column, 0), (image.row, 0))), at(((image.column + 1, 0), (image.row + 1, 0))));
            let (cw, ch) = ((x1 - x0).max(1), (y1 - y0).max(1));
            let frame = match size {
                Some((w, h)) if w > 0 && h > 0 => {
                    let scale = (cw as f64 / w as f64).min(ch as f64 / h as f64);
                    let (fw, fh) = ((w as f64 * scale) as i64, (h as f64 * scale) as i64);
                    EmuRect::new(x0 + (cw - fw) / 2, y0 + (ch - fh) / 2, fw.max(1), fh.max(1))
                }
                _ => EmuRect::new(x0, y0, cw, ch),
            };
            let id = reader.presentation.new_shape_id();
            slide.shapes.push(Shape { id, frame, rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Picture(ibis_doc_model::Picture::new(media)), floating: None });
            slide.shape_placements.insert(id, ibis_doc_model::ObjectPlacement::MoveAndSize);
        }
        // Embedded objects (`oleObject`): the picture that stands for each (`objectPr`'s image, a PNG
        // or an EMF), drawn where `objectPr/anchor` puts it.
        for object in sheet_root.descendants().filter(|n| n.is_element() && n.tag_name().name() == "objectPr") {
            let Some(rid) = attr_rid(object) else { continue };
            let Some(anchor) = child(object, "anchor") else { continue };
            let (Some(from), Some(to)) = (child(anchor, "from"), child(anchor, "to")) else { continue };
            let Some((media, _)) = reader.media(&sheet_rels, rid) else { continue };
            let ((x0, y0), (x1, y1)) = (at(anchor_cell(from)), at(anchor_cell(to)));
            extent = (extent.0.max(x1), extent.1.max(y1));
            let id = reader.presentation.new_shape_id();
            slide.shapes.push(Shape { id, frame: EmuRect::new(x0, y0, (x1 - x0).max(1), (y1 - y0).max(1)), rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Picture(ibis_doc_model::Picture::new(media)), floating: None });
            slide.shape_placements.insert(id, ibis_doc_model::ObjectPlacement::MoveAndSize);
        }
        // Form controls (フォーム コントロール): each `control` of the sheet with its `formControlPr`, placed by its anchor,
        // with the text its drawing shape has.
        let vml_xml = child(sheet_root, "legacyDrawing")
            .and_then(attr_rid)
            .and_then(|rid| sheet_rels.get(rid))
            .or_else(|| sheet_rels.values().find(|r| !r.external && r.kind.ends_with("/vmlDrawing")))
            .and_then(|r| package.read_xml(&r.target).ok());
        let vml_doc = vml_xml.as_deref().and_then(|x| roxmltree::Document::parse(x).ok());
        for control in sheet_root.descendants().filter(|n| n.is_element() && n.tag_name().name() == "control" && unqualified_attribute(*n, "shapeId").is_some()) {
            let Some(rel) = attr_rid(control).and_then(|rid| sheet_rels.get(rid)) else { continue };
            let Some(xml) = package.read_xml(&rel.target).ok() else { continue };
            let Some(props_doc) = parse(&rel.target, &xml).ok() else { continue };
            let props = props_doc.root_element();
            let get = |name: &str| unqualified_attribute(props, name);
            let number = |name: &str, default: i64| get(name).and_then(|v| v.parse::<i64>().ok()).unwrap_or(default);
            use ibis_doc_model::ControlKind as K;
            let kind = match get("objectType") {
                Some("Button") => K::Button,
                Some("CheckBox") => K::CheckBox,
                Some("Radio") => K::Radio,
                Some("Scroll") => K::Scroll,
                Some("Spin") => K::Spin,
                Some("Drop") => K::Drop,
                Some("List") => K::List,
                Some("GBox") => K::GroupBox,
                Some("Label") => K::Label,
                _ => continue,
            };
            let Some(anchor) = descendant_named(control, "anchor") else { continue };
            let (Some(from), Some(to)) = (child(anchor, "from"), child(anchor, "to")) else { continue };
            let ((x0, y0), (x1, y1)) = (at(anchor_cell(from)), at(anchor_cell(to)));
            extent = (extent.0.max(x1), extent.1.max(y1));
            let shape_id = unqualified_attribute(control, "shapeId").unwrap_or("");
            // The text and its character properties: the runs of the drawing shape of the same number.
            let sp_node = drawing
                .as_deref()
                .and_then(|d| docs.get(d))
                .and_then(|doc| doc.descendants().find(|n| n.is_element() && n.tag_name().name() == "cNvPr" && unqualified_attribute(*n, "id") == Some(shape_id)))
                .and_then(|c| c.ancestors().find(|n| n.is_element() && n.tag_name().name() == "sp"));
            let text = sp_node
                .map(|sp| sp.descendants().filter(|n| n.is_element() && n.tag_name().name() == "t").filter_map(|t| t.text()).collect::<Vec<_>>().join(""))
                .unwrap_or_default();
            let mut c = ibis_doc_model::FormControl::new(kind, unqualified_attribute(control, "name").unwrap_or(""));
            c.text = text;
            c.part = rel.target.clone();
            c.link = get("fmlaLink").map(str::to_string);
            c.range = get("fmlaRange").map(str::to_string);
            c.min = number("min", 0);
            c.max = number("max", if kind == K::Scroll { 100 } else { 30000 });
            c.step = number("inc", 1);
            c.page = number("page", 10);
            c.horizontal = get("horiz") == Some("1");
            c.checked = match get("checked") {
                Some("Checked") => 1,
                Some("Mixed") => 2,
                _ => 0,
            };
            c.first_button = get("firstButton") == Some("1");
            c.three_d = match kind {
                K::Button | K::Scroll | K::Spin => true,
                K::Label => false,
                _ => !matches!(get("noThreeD"), Some("1" | "true")),
            };
            if let Some(rpr) = sp_node.and_then(|sp| sp.descendants().find(|n| n.is_element() && n.tag_name().name() == "rPr")) {
                c.font_size = unqualified_attribute(rpr, "sz").and_then(|v| v.parse::<i32>().ok());
                c.bold = matches!(unqualified_attribute(rpr, "b"), Some("1" | "true"));
                c.italic = matches!(unqualified_attribute(rpr, "i"), Some("1" | "true"));
                c.font = child(rpr, "ea")
                    .or_else(|| child(rpr, "latin"))
                    .and_then(|n| unqualified_attribute(n, "typeface"))
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                c.text_color = child(rpr, "solidFill")
                    .and_then(|sf| child(sf, "srgbClr"))
                    .and_then(|clr| unqualified_attribute(clr, "val"))
                    .and_then(|hex| u32::from_str_radix(hex.trim(), 16).ok())
                    .map(ibis_doc_model::Rgb);
            }
            c.value = if matches!(kind, K::Drop | K::List) { number("sel", 0) } else { number("val", 0) };
            if kind == K::Radio {
                c.value = 0;
            }
            let id = reader.presentation.new_shape_id();
            let vml_hidden = !shape_id.is_empty()
                && vml_doc.as_ref().is_some_and(|doc| {
                    let suffix = format!("_s{shape_id}");
                    doc.descendants().any(|n| {
                        n.is_element()
                            && n.tag_name().name() == "shape"
                            && unqualified_attribute(n, "id").is_some_and(|v| v == shape_id || v.ends_with(&suffix))
                            && unqualified_attribute(n, "style").is_some_and(|st| {
                                st.split(';').any(|kv| {
                                    let Some((k, v)) = kv.split_once(':') else { return false };
                                    k.trim().eq_ignore_ascii_case("visibility") && v.trim().eq_ignore_ascii_case("hidden")
                                })
                            })
                    })
                });
            if vml_hidden {
                reader.slide_hidden.insert(id);
            }
            slide.shapes.push(Shape { id, frame: EmuRect::new(x0, y0, (x1 - x0).max(1), (y1 - y0).max(1)), rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Control(std::sync::Arc::new(c)), floating: None });
            slide.shape_placements.insert(id, ibis_doc_model::ObjectPlacement::MoveAndSize);
        }
        slide.size = Some(EmuSize::new(extent.0.max(1), extent.1.max(1)));
        slide.placeholders = std::mem::take(&mut reader.slide_placeholders);
        slide.shape_names = std::mem::take(&mut reader.slide_names);
        slide.shape_alt_texts = std::mem::take(&mut reader.slide_alt_texts);
        slide.decorative_shapes = std::mem::take(&mut reader.slide_decorative);
        slide.shape_return_to_zoom = std::mem::take(&mut reader.slide_return_to_zoom);
        slide.shape_zoom_show_bg = std::mem::take(&mut reader.slide_zoom_show_bg);
        slide.shape_zoom_durations = std::mem::take(&mut reader.slide_zoom_durations);
        slide.hidden_shapes = std::mem::take(&mut reader.slide_hidden);
        slide.shape_links = std::mem::take(&mut reader.slide_links);
        slide.shape_link_tooltips = std::mem::take(&mut reader.slide_link_tooltips);
        slide.aspect_locked = std::mem::take(&mut reader.slide_aspect);
        slide.shape_formulas = std::mem::take(&mut reader.slide_formulas);
        chart_sources.append(&mut reader.chart_sources);
        xlsx_shape_sources.append(&mut reader.xlsx_shape_sources);
        let new_cell_images: Vec<ibis_doc_model::CellImage> = if let Some(first) = slide.shapes.first() {
            if let ShapeKind::Sheet(sheet) = &first.kind {
                let xs = sheet.column_offsets();
                let ys = sheet.row_offsets();
                slide.shapes[1..]
                    .iter()
                    .filter(|s| matches!(s.kind, ShapeKind::Picture(_)) && slide.shape_placements.get(&s.id) == Some(&ibis_doc_model::ObjectPlacement::MoveAndSize))
                    .filter_map(|s| {
                        let x_pos = s.frame.x + (s.frame.width / 4).min(95_250).max(1);
                        let y_pos = s.frame.y + (s.frame.height / 4).min(95_250).max(1);
                        let col = xs.windows(2).position(|w| x_pos >= w[0] && x_pos < w[1]).map(|i| i as u32)?;
                        let row = ys.windows(2).position(|w| y_pos >= w[0] && y_pos < w[1]).map(|i| i as u32)?;
                        if !sheet.cell_images.iter().any(|i| i.column == col && i.row == row) {
                            Some(ibis_doc_model::CellImage { row, column: col, index: 0 })
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        if !new_cell_images.is_empty() {
            if let Some(first) = slide.shapes.first_mut() {
                if let ShapeKind::Sheet(sheet) = &mut first.kind {
                    std::sync::Arc::make_mut(sheet).cell_images.extend(new_cell_images);
                }
            }
        }
        reader.presentation.slides.push(slide);
        presentation = reader.presentation;
        media = reader.media;
    }
    // The first sheet's size stands for the workbook's.
    if let Some(size) = presentation.slides.first().and_then(|s| s.size) {
        presentation.slide_size = size;
    }

    // The workbook's own things: every sheet (the hidden worksheets' cells
    // read for formulas that refer to them), the defined names, the date
    // system.
    // Only the registered extension under SpreadsheetML ancestors changes
    // calculation. Foreign lookalikes and malformed versions stay opaque.
    const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    const VERSION_NS: &str = "http://schemas.microsoft.com/office/spreadsheetml/2024/workbookCompatibilityVersion";
    const VERSION_URI: &str = "{D14903EA-33C4-47F7-8F05-3474C54BE107}";
    let lists: Vec<_> = elements(root).filter(|n| n.has_tag_name((MAIN, "extLst"))).collect();
    let versions: Vec<_> = lists.iter().copied().flat_map(elements)
        .filter(|n| n.has_tag_name((MAIN, "ext")) && xlsx::unqualified_attr(*n, "uri") == Some(VERSION_URI))
        .flat_map(elements).filter(|n| n.has_tag_name((VERSION_NS, "version"))).collect();
    let compatibility_version = if root.has_tag_name((MAIN, "workbook")) && lists.len() == 1 && versions.len() == 1 {
        let node = versions[0];
        xlsx::unqualified_attr(node, "setVersion").and_then(xlsx::metadata_index).filter(|&v| v >= 1).filter(|&version| {
            xlsx::unqualified_attr(node, "warnBelowVersion").is_none_or(|w| xlsx::metadata_index(w).is_some_and(|minimum| minimum >= 1 && minimum <= version))
        })
    } else { None };
    let calc_pr = child(root, "calcPr");
    let calc_mode = calc_pr.and_then(|p| attr(p, "calcMode"))
        .and_then(ibis_doc_model::CalcMode::from_str)
        .unwrap_or_default();
    let calc_on_save = calc_pr.and_then(|p| attr_bool(p, "calcOnSave")).unwrap_or(true);
    let full_precision = calc_pr.and_then(|p| attr_bool(p, "fullPrecision")).unwrap_or(true);
    let iterate = calc_pr.and_then(|p| attr_bool(p, "iterate")).unwrap_or(false);
    let max_iterations = calc_pr.and_then(|p| attr(p, "maxIterations")).and_then(|s| s.parse::<u32>().ok()).unwrap_or(100);
    let max_change = calc_pr.and_then(|p| attr(p, "maxChange")).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.001);
    let mut workbook = ibis_doc_model::Workbook {
        compatibility_version,
        date1904: child(root, "workbookPr").and_then(|p| attr_bool(p, "date1904")).unwrap_or(false),
        default_style: styles.formats.first().map(|f| f.style.clone()),
        theme_colors: theme.colors(),
        theme_fonts: Some(theme.fonts()),
        custom_cell_styles: styles.custom_cell_styles.clone(),
        calc_mode,
        calc_on_save,
        full_precision,
        iterate,
        max_iterations,
        max_change: max_change.into(),
        protection: xlsx::read_workbook_protection(root),
        properties: crate::package::read_doc_properties(&package),
        custom_views: xlsx::read_custom_workbook_views(root),
        window_hidden: xlsx::read_workbook_window_hidden(root),
        external_books,
        queries,
        query_parameters,
        ..Default::default()
    };
    for (name, state, part) in &all_sheets {
        let slide = part.as_ref().and_then(|p| slide_of_part.get(p)).copied();
        let hidden = match (slide, part) {
            (None, Some(part)) => package
                .read_xml(part)
                .ok()
                .and_then(|xml| parse(part, &xml).ok().map(|doc| {
                    let mut s = xlsx::read_sheet(doc.root_element(), &styles, &strings, &phonetics, digit, &sheet_colors, &dynamic_metadata);
                    if let Ok(rels) = package.relationships(part) {
                        for rel in rels.values() {
                            if rel.kind == crate::package::REL_COMMENTS && !rel.external {
                                if let Ok(cxml) = package.read_xml(&rel.target) {
                                    if let Ok(cdoc) = roxmltree::Document::parse(&cxml) {
                                        s.comments = xlsx::read_comments(cdoc.root_element());
                                    }
                                }
                            }
                        }
                    }
                    name_external_books(&mut s, &book_names);
                    std::sync::Arc::new(s)
                })),
            _ => None,
        };
        workbook.sheets.push(ibis_doc_model::WorkbookSheet { name: name.clone(), state: *state, slide, hidden, other: part.is_none() });
    }
    workbook.names = child(root, "definedNames")
        .into_iter()
        .flat_map(|d| children(d, "definedName"))
        .filter_map(|d| {
            Some(ibis_doc_model::DefinedName {
                name: attr(d, "name")?.to_string(),
                sheet: attr_i64(d, "localSheetId").map(|i| i.max(0) as usize),
                formula: ibis_calc::formula::books_by_name(&ibis_calc::formula::from_file(d.text().unwrap_or("")), &book_names),
                hidden: attr_bool(d, "hidden").unwrap_or(false),
            })
        })
        .collect();
    let date1904 = workbook.date1904;
    presentation.theme_colors = workbook.theme_colors.clone();
    presentation.theme_fonts = workbook.theme_fonts.clone();
    for slide in &mut presentation.slides {
        if let Some(ShapeKind::Sheet(s)) = slide.shapes.iter_mut().map(|sh| &mut sh.kind).find(|k| matches!(k, ShapeKind::Sheet(_))) {
            let sheet = std::sync::Arc::make_mut(s);
            for sv in &mut sheet.custom_views {
                if sv.name.is_empty() {
                    if let Some(wb_cv) = workbook.custom_views.iter().find(|w| w.guid == sv.guid) {
                        sv.name = wb_cv.name.clone();
                    }
                }
            }
        }
    }
    presentation.workbook = Some(std::sync::Arc::new(workbook));
    ibis_doc_model::slicer::refresh_slicers(&mut presentation);
    ibis_doc_model::form_control::refresh_controls(&mut presentation);
    // Formulas the file has no value for (written by a program that does not calculate).
    let _ = ibis_doc_model::calculate_missing_values(&mut presentation);
    {
        let evaluated: Vec<(ibis_doc_model::SlideId, ibis_doc_model::CellRange)> = {
            let view = ibis_doc_model::BookView::new(&presentation);
            presentation
                .workbook
                .as_deref()
                .map(|wb| {
                    wb.names
                        .iter()
                        .filter(|dn| dn.name.eq_ignore_ascii_case("_xlnm.Print_Area"))
                        .filter_map(|dn| {
                            let idx = dn.sheet?;
                            let slide_id = wb.sheets.get(idx)?.slide?;
                            let s = presentation.slide(slide_id)?.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s) } else { None })?;
                            if s.print_area.is_some() {
                                return None;
                            }
                            let expr = ibis_calc::formula::parse(dn.formula.trim().trim_start_matches('=')).ok()?;
                            let mut ctx = ibis_calc::eval::Ctx::new(&view, &ibis_calc::eval::Stored, idx, 0, 0);
                            let ibis_calc::eval::Value::Ref(areas) = ctx.eval(&expr) else { return None };
                            let first = areas.first()?;
                            let mut r = ibis_doc_model::CellRange {
                                first_row: first.first_row,
                                first_column: first.first_column,
                                last_row: first.last_row,
                                last_column: first.last_column,
                            };
                            for a in &areas[1..] {
                                r.first_row = r.first_row.min(a.first_row);
                                r.first_column = r.first_column.min(a.first_column);
                                r.last_row = r.last_row.max(a.last_row);
                                r.last_column = r.last_column.max(a.last_column);
                            }
                            Some((slide_id, r))
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        for (slide_id, area) in evaluated {
            if let Some(slide) = presentation.slides.iter_mut().find(|s| s.id == slide_id)
                && let Some(ShapeKind::Sheet(s)) = slide.shapes.iter_mut().map(|sh| &mut sh.kind).find(|k| matches!(k, ShapeKind::Sheet(_)))
            {
                std::sync::Arc::make_mut(s).print_area = Some(area);
            }
        }
    }
    ibis_doc_model::chart_refresh::fill_charts_without_data(&mut presentation);
    build_pivots_on_load(&mut presentation, date1904, styles_doc.as_ref().map(|d| d.root_element()), &sheet_colors);

    let map = crate::save::xlsx::SourceMap {
        workbook: main.clone(),
        styles: styles_xml.as_ref().map(|(p, _)| p.clone()),
        shared_strings: strings_xml.as_ref().map(|(p, _)| p.clone()),
        calc_chain: part_of("/calcChain"),
        sheet_parts: all_sheets.iter().map(|(_, _, p)| p.clone()).collect(),
        sheet_ids,
        digit,
        charts: chart_sources,
        shapes: xlsx_shape_sources,
        external_parts,
    };
    Ok((presentation, map))
}

/// The pivot tables the file asks to be built on opening (`refreshOnLoad`,
/// as a file written by a program other than Excel has them, with no cells
/// yet): built from their source, as Excel builds them.
fn build_pivots_on_load(presentation: &mut Presentation, date1904: bool, styles: Option<Node>, colors: &crate::xlsx::SheetColors<'_>) {
    let base = presentation.workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
    for si in 0..presentation.slides.len() {
        let Some(sheet) = presentation.slides[si].shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None }) else { continue };
        for (pi, pivot) in sheet.pivots.iter().enumerate() {
            let styled = |presentation: &mut Presentation, rows: &[ibis_doc_model::pivot::PivotLineKind], columns: &[ibis_doc_model::pivot::PivotLineKind]| {
                if let Some(ShapeKind::Sheet(s)) = presentation.slides[si].shapes.iter_mut().map(|sh| &mut sh.kind).find(|k| matches!(k, ShapeKind::Sheet(_))) {
                    crate::table_style::apply_pivot_style(std::sync::Arc::make_mut(s), pi, rows, columns, styles, colors);
                }
            };
            if !pivot.refresh_on_load {
                let (rows, columns) = pivot.line_kinds();
                styled(presentation, &rows, &columns);
                continue;
            }
            let source = crate::xlsx_pivot::pivot_source(presentation, &sheet, pivot);
            let Some(data) = source else { continue };
            let mut pivot = pivot.clone();
            pivot.fill_shared(&data);
            let built = pivot.build(&data);
            let slide = &mut presentation.slides[si];
            if let Some(ShapeKind::Sheet(s)) = slide.shapes.iter_mut().map(|sh| &mut sh.kind).find(|k| matches!(k, ShapeKind::Sheet(_))) {
                let sheet = std::sync::Arc::make_mut(s);
                if let Some(p) = sheet.pivots.get_mut(pi) {
                    *p = pivot;
                }
                sheet.put_pivot(pi, &built, &base, date1904);
            }
            styled(presentation, &built.rows, &built.columns);
        }
    }
}

/// The anchors of a spreadsheet drawing, through markup-compatibility
/// wrappers.
fn drawing_anchors<'d, 'i>(root: Node<'d, 'i>) -> Vec<Node<'d, 'i>> {
    let mut out = Vec::new();
    for node in elements(root) {
        match node.tag_name().name() {
            "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor" => out.push(node),
            "AlternateContent" => {
                if let Some(branch) = child(node, "Choice").or_else(|| child(node, "Fallback")) {
                    out.extend(drawing_anchors(branch));
                }
            }
            _ => {}
        }
    }
    out
}

/// A cell anchor's column and row, each with its offset in EMU.
fn anchor_cell(corner: Node) -> ((u32, i64), (u32, i64)) {
    let number = |name: &str| child(corner, name).and_then(|n| n.text()).and_then(|t| t.trim().parse::<i64>().ok()).unwrap_or(0);
    ((number("col").max(0) as u32, number("colOff")), (number("row").max(0) as u32, number("rowOff")))
}

/// Ink: InkML (`inkml:ink`) strokes drawn as SVG.
mod ink {
    use std::collections::HashMap;
    use std::fmt::Write;

    use roxmltree::Node;

    use crate::xml::{attr, elements};

    /// EMU per inch; the SVG is drawn at 288 pixels per inch.
    const EMU_PER_PIXEL: f64 = 914_400.0 / 288.0;
    /// EMU per centimetre, the unit brush sizes are given in.
    const EMU_PER_CM: f64 = 360_000.0;

    /// Strokes ready to draw: an SVG document and the box it covers on the
    /// slide (x, y, width, height in EMU).
    pub(super) struct Drawing {
        pub(super) svg: String,
        pub(super) bounds: (f64, f64, f64, f64),
    }

    struct Brush {
        color: String,
        /// Across and along a stroke, in EMU.
        width: f64,
        height: f64,
        rectangle: bool,
        opacity: f64,
    }

    impl Default for Brush {
        fn default() -> Self {
            Self { color: "#000000".into(), width: 0.05 * EMU_PER_CM, height: 0.05 * EMU_PER_CM, rectangle: false, opacity: 1.0 }
        }
    }

    fn brush(node: Node) -> Brush {
        let mut brush = Brush::default();
        for p in elements(node).filter(|n| n.tag_name().name() == "brushProperty") {
            let value = attr(p, "value").unwrap_or("");
            let size = || {
                let v: f64 = value.parse().ok()?;
                Some(match attr(p, "units") {
                    Some("mm") => v * EMU_PER_CM / 10.0,
                    Some("in") => v * 914_400.0,
                    _ => v * EMU_PER_CM,
                })
            };
            match attr(p, "name") {
                Some("color") if value.starts_with('#') && value.len() == 7 => brush.color = value.to_string(),
                Some("width") => brush.width = size().unwrap_or(brush.width),
                Some("height") => brush.height = size().unwrap_or(brush.height),
                Some("tip") => brush.rectangle = value == "rectangle",
                // A highlighter: see-through, the text shows through it
                // (PowerPoint's PDF shows #FFFC00 as #FFFD7F on white).
                Some("rasterOp") if value == "maskPen" => brush.opacity *= 0.5,
                Some("transparency") => {
                    if let Ok(t) = value.parse::<f64>() {
                        brush.opacity *= (1.0 - t / 255.0).clamp(0.0, 1.0);
                    }
                }
                _ => {}
            }
        }
        brush
    }

    /// The points of a trace, each channel's values in order (InkML's
    /// packed notation: `!` an explicit value, `'` a difference from the
    /// last value, `"` a difference from the last difference; a value
    /// without a prefix is read as the channel's last one was).
    fn trace_points(text: &str, channels: usize) -> Vec<Vec<f64>> {
        #[derive(Clone, Copy)]
        enum Mode {
            Explicit,
            First,
            Second,
        }
        let mut modes = vec![Mode::Explicit; channels];
        let mut last = vec![0.0; channels];
        let mut velocity = vec![0.0; channels];
        let mut out = Vec::new();
        for (index, point) in text.split(',').enumerate() {
            let mut values = Vec::new();
            let chars: Vec<char> = point.chars().collect();
            let mut i = 0;
            let mut prefix = None;
            while i < chars.len() {
                let c = chars[i];
                match c {
                    '!' | '\'' | '"' => {
                        prefix = Some(c);
                        i += 1;
                    }
                    '-' | '.' | '0'..='9' => {
                        let start = i;
                        i += 1;
                        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == 'e' || chars[i] == 'E') {
                            i += 1;
                        }
                        let number: String = chars[start..i].iter().collect();
                        values.push((prefix.take(), number.parse::<f64>().unwrap_or(0.0)));
                    }
                    _ => i += 1,
                }
            }
            if values.is_empty() {
                continue;
            }
            let mut p = Vec::with_capacity(channels);
            for (c, (prefix, v)) in values.into_iter().take(channels).enumerate() {
                match prefix {
                    Some('!') => modes[c] = Mode::Explicit,
                    Some('\'') => modes[c] = Mode::First,
                    Some('"') => modes[c] = Mode::Second,
                    _ => {}
                }
                let value = match (index, modes[c]) {
                    (0, _) | (_, Mode::Explicit) => {
                        velocity[c] = if index == 0 { 0.0 } else { v - last[c] };
                        v
                    }
                    (_, Mode::First) => {
                        velocity[c] = v;
                        last[c] + v
                    }
                    (_, Mode::Second) => {
                        velocity[c] += v;
                        last[c] + velocity[c]
                    }
                };
                last[c] = value;
                p.push(value);
            }
            if p.len() >= 2 {
                out.push(p);
            }
        }
        out
    }

    /// The convex hull of `points`, counter-clockwise.
    fn hull(mut points: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
        points.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        points.dedup();
        if points.len() < 3 {
            return points;
        }
        let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
        let mut lower: Vec<(f64, f64)> = Vec::new();
        for &p in &points {
            while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
                lower.pop();
            }
            lower.push(p);
        }
        let mut upper: Vec<(f64, f64)> = Vec::new();
        for &p in points.iter().rev() {
            while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
                upper.pop();
            }
            upper.push(p);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    }

    /// The strokes of `root`, placed so that their points fill `frame`
    /// (x, y, width, height in EMU: the content part's `xfrm`).
    pub(super) fn read(root: Node, frame: (f64, f64, f64, f64)) -> Option<Drawing> {
        let mut brushes: HashMap<String, Brush> = HashMap::new();
        let mut channels: Option<(usize, usize, usize)> = None;
        for node in root.descendants().filter(|n| n.is_element()) {
            match node.tag_name().name() {
                "brush" => {
                    let id = node.attributes().find(|a| a.name() == "id").map(|a| a.value().to_string()).unwrap_or_default();
                    brushes.insert(id, brush(node));
                }
                "traceFormat" if channels.is_none() => {
                    let names: Vec<&str> = elements(node).filter(|n| n.tag_name().name() == "channel").filter_map(|n| attr(n, "name")).collect();
                    let x = names.iter().position(|n| *n == "X")?;
                    let y = names.iter().position(|n| *n == "Y")?;
                    channels = Some((names.len(), x, y));
                }
                _ => {}
            }
        }
        let (count, xi, yi) = channels.unwrap_or((2, 0, 1));
        let default_brush = Brush::default();
        let traces: Vec<(Vec<(f64, f64)>, &Brush)> = root
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "trace")
            .map(|t| {
                let points = trace_points(t.text().unwrap_or(""), count).into_iter().filter_map(|p| Some((*p.get(xi)?, *p.get(yi)?))).collect();
                let brush = attr(t, "brushRef").and_then(|r| brushes.get(r.trim_start_matches('#'))).unwrap_or(&default_brush);
                (points, brush)
            })
            .filter(|(p, _): &(Vec<(f64, f64)>, _)| !p.is_empty())
            .collect();
        if traces.is_empty() {
            return None;
        }
        // The points' box in ink units, mapped onto the frame; InkML's
        // usual unit is 1/1000 cm (360 EMU).
        let all = traces.iter().flat_map(|(p, _)| p.iter());
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in all {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        let scale = |ext: f64, span: f64| if span > 0.0 && ext > 0.0 { ext / span } else { 360.0 };
        let (sx, sy) = (scale(frame.2, x1 - x0), scale(frame.3, y1 - y0));
        let at = |(x, y): (f64, f64)| (frame.0 + (x - x0) * sx, frame.1 + (y - y0) * sy);
        let reach = traces.iter().map(|(_, b)| b.width.max(b.height) / 2.0).fold(0.0, f64::max) + 1.0;
        let bounds = (frame.0 - reach, frame.1 - reach, frame.2 + reach * 2.0, frame.3 + reach * 2.0);
        let (px_w, px_h) = ((bounds.2 / EMU_PER_PIXEL).ceil().clamp(1.0, 4096.0), (bounds.3 / EMU_PER_PIXEL).ceil().clamp(1.0, 4096.0));
        let mut svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{px_w}" height="{px_h}" viewBox="{} {} {} {}" preserveAspectRatio="none">"#,
            bounds.0, bounds.1, bounds.2, bounds.3
        );
        for (points, brush) in &traces {
            let points: Vec<(f64, f64)> = points.iter().map(|&p| at(p)).collect();
            // A group's opacity applies once, where strokes overlap too.
            let _ = write!(svg, r#"<g opacity="{}" fill="{}" stroke="none">"#, brush.opacity, brush.color);
            if brush.rectangle {
                // The rectangular tip swept along each segment.
                let (hw, hh) = (brush.width / 2.0, brush.height / 2.0);
                let corners = |(x, y): (f64, f64)| [(x - hw, y - hh), (x + hw, y - hh), (x + hw, y + hh), (x - hw, y + hh)];
                let segments: Vec<[(f64, f64); 2]> = match points.len() {
                    1 => vec![[points[0], points[0]]],
                    _ => points.windows(2).map(|w| [w[0], w[1]]).collect(),
                };
                svg.push_str("<path d=\"");
                for [a, b] in segments {
                    let outline = hull(corners(a).into_iter().chain(corners(b)).collect());
                    for (i, (x, y)) in outline.iter().enumerate() {
                        let _ = write!(svg, "{}{x:.0} {y:.0}", if i == 0 { "M" } else { "L" });
                    }
                    svg.push('Z');
                }
                svg.push_str("\" fill-rule=\"nonzero\"/>");
            } else {
                let size = (brush.width + brush.height) / 2.0;
                let _ = write!(svg, r#"<polyline fill="none" stroke="{}" stroke-width="{size:.0}" stroke-linecap="round" stroke-linejoin="round" points=""#, brush.color);
                let points = if points.len() == 1 { vec![points[0], points[0]] } else { points };
                for (x, y) in points {
                    let _ = write!(svg, "{x:.0},{y:.0} ");
                }
                svg.push_str("\"/>");
            }
            svg.push_str("</g>");
        }
        svg.push_str("</svg>");
        Some(Drawing { svg, bounds })
    }

    #[cfg(test)]
    mod tests {
        use super::trace_points;

        #[test]
        fn packed_trace() {
            // Explicit, then a difference, then differences of differences.
            let p = trace_points("1 35,'91'0,\"0\"0,-1 0", 2);
            assert_eq!(p, vec![vec![1.0, 35.0], vec![92.0, 35.0], vec![183.0, 35.0], vec![273.0, 35.0]]);
        }
    }
}

/// This thread yields to the app's own (the window being made and drawn while
/// a large file is read on all the cores slowed the start by some 100 ms).
#[cfg(target_os = "macos")]
fn lower_priority() {
    unsafe extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
    }
    // QOS_CLASS_UTILITY
    unsafe { pthread_set_qos_class_self_np(0x11, 0) };
}

#[cfg(not(target_os = "macos"))]
fn lower_priority() {}

/// `f` of each of `items`, in order, on up to 8 threads (one on the web,
/// which has none to spare).
pub(crate) fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    #[cfg(target_family = "wasm")]
    return items.iter().map(f).collect();
    #[cfg(not(target_family = "wasm"))]
    {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().saturating_sub(1).max(1)).min(16).min(items.len());
        if threads <= 1 {
            return items.iter().map(f).collect();
        }
        let next = AtomicUsize::new(0);
        let mut out: Vec<Option<R>> = (0..items.len()).map(|_| None).collect();
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        lower_priority();
                        let mut done = Vec::new();
                        loop {
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            let Some(item) = items.get(i) else { break };
                            done.push((i, f(item)));
                        }
                        done
                    })
                })
                .collect();
            for worker in workers {
                for (i, r) in worker.join().unwrap_or_else(|e| std::panic::resume_unwind(e)) {
                    out[i] = Some(r);
                }
            }
        });
        out.into_iter().map(|r| r.expect("each item mapped")).collect()
    }
}

/// [`par_map`] with the items that weigh most started first (the threads
/// then end together instead of on the last big item); the results are in
/// the order of `items`.
fn par_map_largest_first<T: Sync, R: Send>(items: &[T], weight: impl Fn(&T) -> usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(weight(&items[i])));
    let done = par_map(&order, |&i| f(&items[i]));
    let mut out: Vec<Option<R>> = (0..items.len()).map(|_| None).collect();
    for (i, r) in order.into_iter().zip(done) {
        out[i] = Some(r);
    }
    out.into_iter().map(|r| r.expect("each item mapped")).collect()
}

fn read_media_nodes(slide_root: Node, native_ids: &std::collections::HashMap<String, ibis_doc_model::ShapeId>, shapes: &mut [ibis_doc_model::Shape]) {
    let Some(timing) = child(slide_root, "timing") else { return };
    for node in timing.descendants().filter(|n| n.is_element() && n.tag_name().name() == "cMediaNode") {
        let Some(target) = node.descendants().find(|n| n.is_element() && n.tag_name().name() == "spTgt").and_then(|n| n.attribute("spid")) else { continue };
        let Some(&shape_id) = native_ids.get(target) else { continue };
        let Some(shape) = shapes.iter_mut().find(|s| s.id == shape_id) else { continue };
        let ibis_doc_model::ShapeKind::Picture(pic) = &mut shape.kind else { continue };
        let Some(clip) = pic.clip.as_mut() else { continue };

        if let Some(vol_str) = node.attribute("vol") {
            if let Ok(vol) = vol_str.parse::<u32>() {
                clip.options.volume = if vol == 0 {
                    ibis_doc_model::MediaVolume::Mute
                } else if vol <= 45000 {
                    ibis_doc_model::MediaVolume::Low
                } else if vol <= 80000 {
                    ibis_doc_model::MediaVolume::Medium
                } else {
                    ibis_doc_model::MediaVolume::High
                };
            }
        }
        if node.attribute("rewind") == Some("1") || node.attribute("rewind") == Some("true") {
            clip.options.rewind = true;
        }
        if node.attribute("fullScreen") == Some("1") || node.attribute("fullScreen") == Some("true") {
            clip.options.full_screen = true;
        }
        if node.attribute("showWhenStopped") == Some("0") || node.attribute("showWhenStopped") == Some("false") {
            if clip.audio {
                clip.options.hide_during_show = true;
            } else {
                clip.options.hide_while_not_playing = true;
            }
        }
        if node.parent().and_then(|h| h.attribute("fullScrn")).is_some_and(|v| v == "1" || v == "true") {
            clip.options.full_screen = true;
        }
        // Started with the slide: the play call is an effect after or with the previous.
        let started_by_slide = timing.descendants().filter(|n| n.is_element() && n.tag_name().name() == "cmd" && n.attribute("cmd").is_some_and(|c| c.starts_with("play"))).any(|cmd| {
            cmd.descendants().any(|t| t.is_element() && t.tag_name().name() == "spTgt" && t.attribute("spid") == Some(target))
                && cmd.ancestors().find(|a| a.is_element() && a.tag_name().name() == "cTn" && a.attribute("presetClass") == Some("mediacall")).is_some_and(|c| matches!(c.attribute("nodeType"), Some("afterEffect" | "withEffect")))
        });
        if started_by_slide {
            clip.options.start = ibis_doc_model::MediaStart::Automatically;
        }
        if let Some(num_slds) = node.attribute("numSld").or_else(|| node.attribute("numSlds")) {
            if let Ok(n) = num_slds.parse::<u32>() {
                if n > 1 {
                    clip.options.play_across_slides = true;
                }
            }
        }
        if let Some(ctn) = child(node, "cTn") {
            if ctn.attribute("repeatCount") == Some("indefinite") {
                clip.options.loop_until_stopped = true;
            }
            if ctn.attribute("fill") == Some("remove") {
                clip.options.rewind = true;
            }
            if let Some(st_cond) = child(ctn, "stCondLst") {
                for cond in st_cond.children().filter(|n| n.is_element() && n.tag_name().name() == "cond") {
                    if cond.attribute("delay") == Some("0") && cond.attribute("evt").is_none() {
                        clip.options.start = ibis_doc_model::MediaStart::Automatically;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod group_turn_tests {
    use super::{EmuRect, MAX_TURNS, Transform, Turn};

    fn turned(turn: Turn) -> Transform {
        let mut t = Transform::IDENTITY;
        t.turns = [None; MAX_TURNS];
        t.turns[0] = Some(turn);
        t
    }

    #[test]
    fn a_shape_in_a_group_turned_a_quarter() {
        // The group is 0..200 × 0..100; its left half turns to its top.
        let t = turned(Turn { cx: 100.0, cy: 50.0, rotation: 5_400_000, flip_h: false, flip_v: false });
        let (frame, rotation, h, v) = t.turn((EmuRect::new(0, 0, 100, 100), 0, false, false));
        assert_eq!((frame, rotation, h, v), (EmuRect::new(50, -50, 100, 100), 5_400_000, false, false));
    }

    #[test]
    fn saving_undoes_the_turns() {
        // Nested: a turned and flipped group in a turned one.
        let mut t = Transform::IDENTITY;
        t.turns[0] = Some(Turn { cx: 500.0, cy: 300.0, rotation: 2_700_000, flip_h: false, flip_v: false });
        t.turns[1] = Some(Turn { cx: 300.0, cy: 200.0, rotation: 5_400_000, flip_h: true, flip_v: false });
        let placed = (EmuRect::new(100, 50, 120, 80), 600_000, false, true);
        let (frame, rotation, flip_h, flip_v) = t.turn(placed);
        let id = ibis_doc_model::ShapeId(1);
        let shape = ibis_doc_model::Shape { id, frame, rotation, flip_h, flip_v, kind: ibis_doc_model::ShapeKind::Line(ibis_doc_model::Stroke::solid(ibis_doc_model::Color::BLACK, 9525)), floating: None };
        let back = crate::save::pptx_shapes::untwisted(&shape, &t.group_turns());
        assert!((back.frame.x - 100).abs() <= 1 && (back.frame.y - 50).abs() <= 1, "{:?}", back.frame);
        assert_eq!((back.frame.width, back.frame.height, back.rotation, back.flip_h, back.flip_v), (120, 80, 600_000, false, true));
    }

    #[test]
    fn a_shape_in_a_flipped_group() {
        // Flipped left to right: the left shape goes right, its turn mirrors.
        let t = turned(Turn { cx: 100.0, cy: 50.0, rotation: 0, flip_h: true, flip_v: false });
        let (frame, rotation, h, v) = t.turn((EmuRect::new(0, 0, 50, 100), 1_800_000, false, false));
        assert_eq!((frame, rotation, h, v), (EmuRect::new(150, 0, 50, 100), 21_600_000 - 1_800_000, true, false));
    }
}
