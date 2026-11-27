// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! OpenDocument presentations (`.odp`, `.otp`; ISO/IEC 26300, ODF 1.3): read as the `.pptx` made from them, as an old `.ppt`
//! is. Written from the OpenDocument specification, not from any program's code.
//!
//! What is read: the slides (names, hidden, background, transition and automatic advance), their master pages (background and
//! the objects that show on every slide), the text with its formatting (fonts, size, colour, bold, italic, underline,
//! strike-out, super and subscript, case, spacing; paragraph alignment, indents, spacing, line height; bullets and numbering;
//! links; page number and date fields), shapes (rectangles, ellipses, arcs, lines, connectors, polygons, paths, preset custom
//! shapes, groups) with fill (solid, gradient, picture, hatch), line (width, colour, dashes, arrows) and shadow, pictures
//! (crop) and tables, and the notes.

use std::collections::HashMap;
use std::fmt::Write as _;

use roxmltree::{Document, Node};

use crate::odp_shape::{Geom, geom_of, objects};
use crate::odp_style::{DRAW, FO, OFFICE, PRESENTATION, SMIL, STYLE, Styles, XLINK, attr, children, emu, is};
use crate::odp_text::{esc, paragraphs, Body};
use ibis_formats::Error;

const MIMETYPE: &str = "application/vnd.oasis.opendocument.presentation";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;

fn bad(message: &str) -> Error {
    Error::new(format!("OpenDocument プレゼンテーション：{message}"))
}

/// Whether `data` is an OpenDocument presentation or template (a zip whose `mimetype` says so).
/// The default text style of the presentation: pair kerning on (`style:letter-kerning` has no default in ODF 1.3; PowerPoint's `-SaveAs pptx` of an
/// .odp writes `kern="1200"` on all nine levels of `p:defaultTextStyle` and on every run whose style does not say otherwise).
const DEFAULT_LEVELS: &str = r#"<a:lvl1pPr><a:defRPr kern="1200"/></a:lvl1pPr><a:lvl2pPr><a:defRPr kern="1200"/></a:lvl2pPr><a:lvl3pPr><a:defRPr kern="1200"/></a:lvl3pPr><a:lvl4pPr><a:defRPr kern="1200"/></a:lvl4pPr><a:lvl5pPr><a:defRPr kern="1200"/></a:lvl5pPr><a:lvl6pPr><a:defRPr kern="1200"/></a:lvl6pPr><a:lvl7pPr><a:defRPr kern="1200"/></a:lvl7pPr><a:lvl8pPr><a:defRPr kern="1200"/></a:lvl8pPr><a:lvl9pPr><a:defRPr kern="1200"/></a:lvl9pPr>"#;

pub fn is_odp(data: &[u8]) -> bool {
    if !data.starts_with(b"PK") {
        return false;
    }
    let Ok(archive) = ibis_formats::zip::Archive::new(data) else { return false };
    archive.read("mimetype").ok().flatten().is_some_and(|m| m.starts_with(MIMETYPE.as_bytes()))
}

/// The pictures of the package so far.
#[derive(Default)]
pub(crate) struct MediaStore {
    files: Vec<(String, Vec<u8>)>,
    by_href: HashMap<String, String>,
    extensions: Vec<&'static str>,
    sizes: HashMap<String, Option<(f64, f64)>>,
    /// The charts made so far (`ppt/charts/chartN.xml` are in `files`).
    charts: usize,
}

/// A relationship of a part: id, type, target, and whether the target is outside the package.
type Rel = (String, String, String, bool);

/// What building the shapes of one part (a slide, a master, a notes page) needs.
pub(crate) struct Cx<'s, 'a, 'i> {
    pub(crate) st: &'s Styles<'a, 'i>,
    archive: &'s ibis_formats::zip::Archive<'s>,
    media: &'s std::cell::RefCell<MediaStore>,
    rels: Vec<Rel>,
    next_rel: u32,
    next_id: u32,
    links: HashMap<String, String>,
    /// The slide (1-based) of each page name: `text:a xlink:href="#Slide4273"` is a jump to that page.
    pub(crate) page_numbers: HashMap<String, usize>,
    /// The relationships of `links` that are jumps to a slide (`ppaction://hlinksldjump`).
    slide_links: std::collections::HashSet<String>,
    /// The rotation of the shape being written (degrees, clockwise): a gradient on the page turns with it in DrawingML.
    pub(crate) shape_rotation: f64,
    /// The shapes of the page by their `draw:id` or `xml:id`, for what the animations name.
    pub(crate) shape_ids: HashMap<String, u32>,
}

impl<'s, 'a, 'i> Cx<'s, 'a, 'i> {
    fn new(st: &'s Styles<'a, 'i>, archive: &'s ibis_formats::zip::Archive<'s>, media: &'s std::cell::RefCell<MediaStore>, first_rel: u32) -> Self {
        Cx { st, archive, media, rels: Vec::new(), next_rel: first_rel, next_id: 2, links: HashMap::new(), page_numbers: HashMap::new(), slide_links: Default::default(), shape_rotation: 0.0, shape_ids: HashMap::new() }
    }

    pub(crate) fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub(crate) fn is_slide_link(&self, id: &str) -> bool {
        self.slide_links.contains(id)
    }

    fn add_rel(&mut self, kind: &str, target: String, external: bool) -> String {
        let id = format!("rId{}", self.next_rel);
        self.next_rel += 1;
        self.rels.push((id.clone(), kind.to_string(), target, external));
        id
    }

    /// The relationship to the chart part made from an embedded chart object (`Object 1/`, ODF 1.3 chapter 11): the same reader and the same
    /// `c:chartSpace` writer as the charts of an `.ods` (the series read the chart's own table). `None` for an object that is no chart.
    pub(crate) fn chart_rel(&mut self, href: &str) -> Option<String> {
        let dir = href.trim_start_matches('#').trim_start_matches("./").trim_end_matches('/');
        if dir.is_empty() || dir.contains("://") {
            return None;
        }
        let content = String::from_utf8(self.archive.read(&format!("{dir}/content.xml")).ok().flatten()?).ok()?;
        let mut chart = crate::ods::chart::read_chart(&content, &[])?;
        // PowerPoint ignores the `chart:gap-width` and `chart:overlap` of a chart that has a bar group and a line group (measured with
        // topptx on a bar + line chart whose y axis says gap 200 / overlap 0 and gap 80 / overlap -27: both give `c:gapWidth` 150 and no
        // `c:overlap`; the same bars alone keep their values). A stacked group's overlap is 100 either way.
        let has = |k: ibis_formats::xls_chart::Kind| chart.groups.iter().any(|g| g.kind == k);
        if has(ibis_formats::xls_chart::Kind::Bar) && has(ibis_formats::xls_chart::Kind::Line) {
            for g in chart.groups.iter_mut().filter(|g| g.kind == ibis_formats::xls_chart::Kind::Bar) {
                g.gap = 150;
                g.overlap = 0;
            }
        }
        // PowerPoint lists the series of an area chart that is not stacked (standard and percent stacked) in the opposite order to the
        // `.odp` (measured with topptx on 007: ODF series D, C, B -> Series 1, 2, 3 for a standard area; B, C, D -> Series 3, 2, 1 for
        // a percent stacked one; a stacked area keeps B, C, D).
        let reversed = chart.groups.len() == 1 && chart.groups[0].kind == ibis_formats::xls_chart::Kind::Area && !chart.groups[0].stacked;
        if reversed {
            chart.series.reverse();
        }
        let xml = ibis_formats::xls_chart::chart_xml(&chart, &crate::ods::chart::OdsCells { tables: Vec::new() }, &|n| format!("Series{n}"));
        let xml = crate::odp_chart_style::style_chart(xml, &content, reversed);
        let name = {
            let mut media = self.media.borrow_mut();
            media.charts += 1;
            let name = format!("chart{}.xml", media.charts);
            media.files.push((format!("ppt/charts/{name}"), xml.into_bytes()));
            name
        };
        Some(self.add_rel("chart", format!("../charts/{name}"), false))
    }

    /// The relationship to a picture of the package (in `Pictures/` or another folder), and its size in points when known.
    pub(crate) fn image_rel_sized(&mut self, href: &str, size_of: fn(&[u8]) -> Option<(f64, f64)>) -> Option<(String, Option<(f64, f64)>)> {
        let path = href.trim_start_matches('#').trim_start_matches("./");
        if path.contains("://") || path.starts_with("file:") || path.is_empty() {
            return None;
        }
        let known = self.media.borrow().by_href.contains_key(path);
        if !known {
            let data = self.archive.read(path).ok().flatten()?;
            let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            let ext: &'static str = match ext.as_str() {
                "png" => "png",
                "jpg" | "jpeg" => "jpeg",
                "gif" => "gif",
                "bmp" => "bmp",
                "tif" | "tiff" => "tiff",
                "emf" => "emf",
                "wmf" => "wmf",
                "svg" => "svg",
                // A picture Word or PowerPoint wrote with a name that tells nothing (`media/image5.tmp`, 015: PNG data): PowerPoint draws it
                // (topptx of 015 shows them), so the kind comes from the bytes.
                _ => sniff_image(&data)?,
            };
            let mut media = self.media.borrow_mut();
            let name = format!("image{}.{ext}", media.files.len() + 1);
            if !media.extensions.contains(&ext) {
                media.extensions.push(ext);
            }
            media.sizes.insert(path.to_string(), size_of(&data));
            media.files.push((format!("ppt/media/{name}"), data));
            media.by_href.insert(path.to_string(), name);
        }
        let (name, size) = {
            let media = self.media.borrow();
            (media.by_href.get(path)?.clone(), media.sizes.get(path).copied().flatten())
        };
        let id = self.add_rel("image", format!("../media/{name}"), false);
        Some((id, size))
    }

    pub(crate) fn image_rel(&mut self, href: &str) -> Option<String> {
        self.image_rel_sized(href, |_| None).map(|(id, _)| id)
    }

    /// The relationship of a hyperlink to an address outside the presentation.
    pub(crate) fn link_rel(&mut self, url: &str) -> Option<String> {
        if let Some(name) = url.strip_prefix('#') {
            // A link to a page by its name (PowerPoint's ODP: `#Slide4273`): a relationship to that slide, action `hlinksldjump`.
            let n = *self.page_numbers.get(name)?;
            let key = format!("#{n}");
            if let Some(id) = self.links.get(&key) {
                return Some(id.clone());
            }
            let id = self.add_rel("slide", format!("slide{n}.xml"), false);
            self.slide_links.insert(id.clone());
            self.links.insert(key, id.clone());
            return Some(id);
        }
        if url.is_empty() {
            return None;
        }
        if let Some(id) = self.links.get(url) {
            return Some(id.clone());
        }
        let id = self.add_rel("hyperlink", url.to_string(), true);
        self.links.insert(url.to_string(), id.clone());
        Some(id)
    }
}

fn rels_xml(items: &[Rel]) -> String {
    let mut s = format!(r#"{HEAD}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, kind, target, external) in items {
        let mode = if *external { r#" TargetMode="External""# } else { "" };
        let _ = write!(s, r#"<Relationship Id="{id}" Type="{REL}/{kind}" Target="{}"{mode}/>"#, esc(target));
    }
    s + "</Relationships>"
}

fn rel(id: &str, kind: &str, target: &str) -> Rel {
    (id.to_string(), kind.to_string(), target.to_string(), false)
}

fn tree(shapes: &str) -> String {
    format!(
        r#"<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree>"#
    )
}

/// The box of the first object of a class on a master page.
fn master_box(master: Option<Node>, class: &str) -> Option<Geom> {
    let master = master?;
    master.children().filter(|c| is(*c, DRAW, "frame") && attr(*c, PRESENTATION, "class") == Some(class)).find_map(geom_of)
}

/// A page's background (`p:bg`) from its drawing-page style, when the style gives a fill.
fn background(cx: &mut Cx, style: Option<&str>) -> String {
    let Some(style) = style else { return String::new() };
    let chain = vec![("drawing-page", style.to_string())];
    let fill = crate::odp_shape::fill_of(cx, &chain);
    if fill.is_empty() { String::new() } else { format!("<p:bg><p:bgPr>{fill}<a:effectLst/></p:bgPr></p:bg>") }
}

/// The transition of a page and the time before the next one, from its drawing-page style.
fn transition(cx: &Cx, page: Node) -> String {
    let Some(style) = attr(page, DRAW, "style-name") else { return String::new() };
    let get = |ns: &str, local: &str| cx.st.prop("drawing-page", style, ns, local);
    let mut adv = String::new();
    if get(PRESENTATION, "transition-type") == Some("automatic") {
        if let Some(ms) = get(PRESENTATION, "duration").and_then(duration_ms) {
            let _ = write!(adv, r#" advTm="{ms}""#);
        }
    }
    let speed = match get(PRESENTATION, "transition-speed") {
        Some("slow") => r#" spd="slow""#,
        Some("fast") => r#" spd="fast""#,
        _ => "",
    };
    let kind = get(SMIL, "type");
    let sub = get(SMIL, "subtype").unwrap_or("");
    let direction = get(SMIL, "direction").unwrap_or("forward");
    // (A transition set on the page as an animation rather than by its style, as LibreOffice writes some.)
    let from_anim = page.descendants().find(|d| d.is_element() && d.tag_name().name() == "transitionFilter").map(|d| (attr(d, SMIL, "type").unwrap_or("fade"), attr(d, SMIL, "subtype").unwrap_or("")));
    let (kind, sub) = match (kind, from_anim) {
        (Some(k), _) => (Some(k), sub),
        (None, Some((k, s))) => (Some(k), s),
        _ => (None, sub),
    };
    let reverse = direction == "reverse";
    let dir = |forward: &'static str, backward: &'static str| if reverse { backward } else { forward };
    let child = match kind {
        None => String::new(),
        Some("fade") => if sub == "fadeOverColor" { r#"<p:fade thruBlk="1"/>"#.to_string() } else { "<p:fade/>".to_string() },
        Some("barWipe") => format!(r#"<p:wipe dir="{}"/>"#, if sub == "topToBottom" { dir("d", "u") } else { dir("r", "l") }),
        Some("pushWipe") => format!(r#"<p:push dir="{}"/>"#, match sub { "fromTop" => "d", "fromBottom" => "u", "fromRight" => "l", _ => "r" }),
        Some("slideWipe") => format!(r#"<p:cover dir="{}"/>"#, match sub { "fromTop" => "d", "fromBottom" => "u", "fromRight" => "l", _ => "r" }),
        Some("splitWipe") => format!(r#"<p:split orient="{}" dir="{}"/>"#, if sub.starts_with("vertical") { "vert" } else { "horz" }, if reverse { "in" } else { "out" }),
        Some("randomBarWipe") => format!(r#"<p:randomBar dir="{}"/>"#, if sub == "vertical" { "vert" } else { "horz" }),
        Some("checkerBoardWipe") => format!(r#"<p:checker dir="{}"/>"#, if sub == "down" { "vert" } else { "horz" }),
        Some("blindsWipe") => format!(r#"<p:blinds dir="{}"/>"#, if sub == "vertical" { "vert" } else { "horz" }),
        Some("dissolve") => "<p:dissolve/>".to_string(),
        Some("irisWipe" | "ellipseWipe" | "fanWipe" | "clockWipe") => r#"<p:zoom dir="in"/>"#.to_string(),
        Some(_) => "<p:fade/>".to_string(),
    };
    if child.is_empty() && adv.is_empty() {
        return String::new();
    }
    format!(r#"<p:transition{speed} advClick="1"{adv}>{child}</p:transition>"#)
}

/// A duration "PT00H00M05S" or "PT5S" in milliseconds.
fn duration_ms(text: &str) -> Option<u64> {
    let t = text.strip_prefix("PT")?;
    let (mut total, mut num) = (0.0f64, String::new());
    for c in t.chars() {
        match c {
            'H' => total += num.parse::<f64>().ok()? * 3_600_000.0,
            'M' => total += num.parse::<f64>().ok()? * 60_000.0,
            'S' => total += num.parse::<f64>().ok()? * 1000.0,
            _ => {
                num.push(c);
                continue;
            }
        }
        num.clear();
    }
    Some(total.round() as u64)
}

/// The `.pptx` made from the presentation in `data`.
pub fn to_pptx(data: &[u8]) -> Result<Vec<u8>, Error> {
    let archive = ibis_formats::zip::Archive::new(data)?;
    let read_xml = |name: &str| -> Result<Option<String>, Error> {
        match archive.read(name)? {
            Some(b) => Ok(Some(String::from_utf8(b).map_err(|_| bad(&format!("{name} が UTF-8 ではありません")))?)),
            None => Ok(None),
        }
    };
    let content_text = read_xml("content.xml")?.ok_or_else(|| bad("content.xml がありません"))?;
    let options = || roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let content = Document::parse_with_options(&content_text, options()).map_err(|e| bad(&format!("content.xml を読めません：{e}")))?;
    let styles_text = read_xml("styles.xml")?;
    let styles_doc = match &styles_text {
        Some(t) => Some(Document::parse_with_options(t, options()).map_err(|e| bad(&format!("styles.xml を読めません：{e}")))?),
        None => None,
    };
    let mut roots: Vec<Node> = Vec::new();
    if let Some(d) = &styles_doc {
        roots.push(d.root_element());
    }
    roots.push(content.root_element());
    let st = Styles::new(&roots);
    let body = children(content.root_element(), OFFICE, "body").next().ok_or_else(|| bad("本文がありません"))?;
    let presentation = children(body, OFFICE, "presentation").next().ok_or_else(|| bad("プレゼンテーションではありません"))?;
    let pages: Vec<Node> = children(presentation, DRAW, "page").collect();
    if pages.is_empty() {
        return Err(bad("スライドがありません"));
    }

    // The master pages the slides use, in the order they are first used.
    let master_node = |name: &str| st.master_pages.iter().copied().find(|m| attr(*m, STYLE, "name") == Some(name));
    let mut masters: Vec<String> = Vec::new();
    for page in &pages {
        let name = attr(*page, DRAW, "master-page-name").unwrap_or("").to_string();
        if !masters.contains(&name) {
            masters.push(name);
        }
    }
    // The slide size: the first master's page layout.
    let (width, height) = masters
        .first()
        .and_then(|m| master_node(m))
        .and_then(|m| attr(m, STYLE, "page-layout-name"))
        .and_then(|l| st.page_layouts.get(l).copied())
        .and_then(|l| l.children().find(|c| is(*c, STYLE, "page-layout-properties")))
        .and_then(|p| Some((attr(p, FO, "page-width").and_then(emu)?, attr(p, FO, "page-height").and_then(emu)?)))
        .unwrap_or((9_144_000, 5_143_500));

    let media = std::cell::RefCell::new(MediaStore::default());
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut overrides = String::new();
    let mut pres_rels: Vec<Rel> = Vec::new();
    let mut master_ids = String::new();
    let mut slide_ids = String::new();
    let layout_names = [("Title Slide", "title"), ("Title and Content", "obj"), ("Blank", "blank")];

    for (i, name) in masters.iter().enumerate() {
        let n = i + 1;
        let node = master_node(name);
        let mut cx = Cx::new(&st, &archive, &media, 5);
        let shapes = match node {
            Some(m) => objects(&mut cx, m, true, &|_| None),
            None => String::new(),
        };
        let bg = background(&mut cx, node.and_then(|m| attr(m, DRAW, "style-name")));
        // The master's placeholders: where the master page puts the title and the outline.
        let default_title = Geom { x: width / 20, y: height / 20, w: width * 9 / 10, h: height / 6, ..Default::default() };
        let default_body = Geom { x: width / 20, y: height / 4, w: width * 9 / 10, h: height * 11 / 20, ..Default::default() };
        let title_box = master_box(node, "title").unwrap_or(default_title);
        let body_box = master_box(node, "outline").unwrap_or(default_body);
        let sub_box = master_box(node, "subtitle").unwrap_or(body_box);
        let ph = |id: u32, name: &str, ph: &str, g: &Geom, prompt: &str| {
            format!(
                r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr>{ph}</p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>{prompt}</a:t></a:r></a:p></p:txBody></p:sp>"#,
                g.x, g.y, g.w, g.h
            )
        };
        let id0 = cx.next_id;
        let master_shapes = format!("{shapes}{}{}", ph(id0, "Title", r#"<p:ph type="title"/>"#, &title_box, "Click to edit Master title style"), ph(id0 + 1, "Text", r#"<p:ph type="body" idx="1"/>"#, &body_box, "Click to edit Master text styles"));
        let master_rels_extra = std::mem::take(&mut cx.rels);
        let master_xml = format!(
            r#"{HEAD}<p:sldMaster xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld>{bg}{}</p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst>{}</p:sldLayoutIdLst></p:sldMaster>"#,
            tree(&master_shapes),
            (0..3).map(|k| format!(r#"<p:sldLayoutId id="{}" r:id="rId{}"/>"#, 2_147_483_649u64 + (i as u64) * 4 + k as u64, k + 1)).collect::<String>()
        );
        let mut rels_of_master: Vec<Rel> = (0..3).map(|k| rel(&format!("rId{}", k + 1), "slideLayout", &format!("../slideLayouts/slideLayout{}.xml", i * 3 + k as usize + 1))).collect();
        rels_of_master.push(rel("rId4", "theme", &format!("../theme/theme{n}.xml")));
        rels_of_master.extend(master_rels_extra);
        files.push((format!("ppt/slideMasters/slideMaster{n}.xml"), master_xml.into_bytes()));
        files.push((format!("ppt/slideMasters/_rels/slideMaster{n}.xml.rels"), rels_xml(&rels_of_master).into_bytes()));
        files.push((format!("ppt/theme/theme{n}.xml"), ibis_formats::new_file::theme().into_bytes()));
        let _ = write!(overrides, r#"<Override PartName="/ppt/slideMasters/slideMaster{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/theme/theme{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#);
        for (k, (layout_name, kind)) in layout_names.iter().enumerate() {
            let ln = i * 3 + k + 1;
            let layout_shapes = match *kind {
                "title" => format!("{}{}", ph(2, "Title", r#"<p:ph type="ctrTitle"/>"#, &title_box, "Click to edit Master title style"), ph(3, "Subtitle", r#"<p:ph type="subTitle" idx="1"/>"#, &sub_box, "Click to edit Master subtitle style")),
                "obj" => format!("{}{}", ph(2, "Title", r#"<p:ph type="title"/>"#, &title_box, "Click to edit Master title style"), ph(3, "Content", r#"<p:ph idx="1"/>"#, &body_box, "Click to edit Master text styles")),
                _ => String::new(),
            };
            let layout_xml = format!(
                r#"{HEAD}<p:sldLayout xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}" type="{kind}" preserve="1"><p:cSld name="{layout_name}">{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#,
                tree(&layout_shapes)
            );
            files.push((format!("ppt/slideLayouts/slideLayout{ln}.xml"), layout_xml.into_bytes()));
            files.push((format!("ppt/slideLayouts/_rels/slideLayout{ln}.xml.rels"), rels_xml(&[rel("rId1", "slideMaster", &format!("../slideMasters/slideMaster{n}.xml"))]).into_bytes()));
            let _ = write!(overrides, r#"<Override PartName="/ppt/slideLayouts/slideLayout{ln}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>"#);
        }
        pres_rels.push(rel(&format!("rId{}", pages.len() + n), "slideMaster", &format!("slideMasters/slideMaster{n}.xml")));
        let _ = write!(master_ids, r#"<p:sldMasterId id="{}" r:id="rId{}"/>"#, 2_147_483_648u64 + (i as u64) * 4, pages.len() + n);
    }

    let mut have_notes = false;
    let mut notes_box: Option<Geom> = None;
    let mut slide_rels_all: Vec<Rel> = Vec::new();
    let page_numbers: HashMap<String, usize> = pages.iter().enumerate().filter_map(|(i, p)| attr(*p, DRAW, "name").map(|n| (n.to_string(), i + 1))).collect();
    for (i, page) in pages.iter().enumerate() {
        let n = i + 1;
        let master_name = attr(*page, DRAW, "master-page-name").unwrap_or("");
        let master_i = masters.iter().position(|m| m == master_name).unwrap_or(0);
        let node = master_node(master_name);
        let mut cx = Cx::new(&st, &archive, &media, 2);
        cx.page_numbers = page_numbers.clone();
        let shapes = objects(&mut cx, *page, false, &|class| master_box(node, class));
        let has = |class: &str| page.children().any(|c| is(c, DRAW, "frame") && attr(c, PRESENTATION, "class") == Some(class));
        let layout = if has("subtitle") { 0 } else if has("title") || has("outline") { 1 } else { 2 };
        let slide_style = attr(*page, DRAW, "style-name");
        let bg = if slide_style.and_then(|s| st.prop("drawing-page", s, DRAW, "fill")).is_some() { background(&mut cx, slide_style) } else { String::new() };
        let mut show = String::new();
        // `presentation:visibility` is a property of the page's drawing-page style (ODF 1.3 20.xxx); PowerPoint writes it there.
        if attr(*page, PRESENTATION, "visibility") == Some("hidden") || slide_style.and_then(|s| st.prop("drawing-page", s, PRESENTATION, "visibility")) == Some("hidden") {
            show += r#" show="0""#;
        }
        if slide_style.and_then(|s| st.prop("drawing-page", s, PRESENTATION, "background-objects-visible")) == Some("false") {
            show += r#" showMasterSp="0""#;
        }
        let trans = transition(&cx, *page) + &crate::odp_anim::timing(&mut cx, *page);
        let page_name = attr(*page, DRAW, "name").map(esc).unwrap_or_default();
        let c_sld_name = if page_name.is_empty() { String::new() } else { format!(r#" name="{page_name}""#) };
        let slide_xml = format!(
            r#"{HEAD}<p:sld xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"{show}><p:cSld{c_sld_name}>{bg}{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{trans}</p:sld>"#,
            tree(&shapes)
        );
        let mut slide_rels = vec![rel("rId1", "slideLayout", &format!("../slideLayouts/slideLayout{}.xml", master_i * 3 + layout + 1))];
        // The notes page, when it has text.
        let notes = children(*page, PRESENTATION, "notes").next();
        let notes_frame = notes.and_then(|nt| nt.children().find(|c| is(*c, DRAW, "frame") && attr(*c, PRESENTATION, "class") == Some("notes")));
        let notes_text = notes_frame.and_then(|f| children(f, DRAW, "text-box").next()).filter(|t| crate::odp_text::has_text(*t));
        let mut notes_part: Option<(String, Vec<Rel>)> = None;
        if let (Some(frame), Some(text_box)) = (notes_frame, notes_text) {
            let mut ncx = Cx::new(&st, &archive, &media, 3);
            let chain: Vec<(&'static str, String)> = {
                let mut c = Vec::new();
                if let Some(s) = attr(frame, PRESENTATION, "style-name") {
                    c.push(("presentation", s.to_string()));
                }
                if let Some(s) = attr(frame, DRAW, "text-style-name") {
                    c.push(("paragraph", s.to_string()));
                }
                c
            };
            let paras = paragraphs(&mut ncx, text_box, &chain, Body::default());
            let shape = format!(
                r#"<p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>{paras}</p:txBody></p:sp>"#
            );
            if notes_box.is_none() {
                notes_box = geom_of(frame);
            }
            let mut notes_rels = vec![rel("rId1", "notesMaster", "../notesMasters/notesMaster1.xml"), rel("rId2", "slide", &format!("../slides/slide{n}.xml"))];
            notes_rels.extend(std::mem::take(&mut ncx.rels));
            notes_part = Some((shape, notes_rels));
        }
        if let Some((shape, notes_rels)) = notes_part {
            let notes_xml = format!(r#"{HEAD}<p:notes xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld>{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"#, tree(&shape));
            files.push((format!("ppt/notesSlides/notesSlide{n}.xml"), notes_xml.into_bytes()));
            files.push((format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"), rels_xml(&notes_rels).into_bytes()));
            let _ = write!(overrides, r#"<Override PartName="/ppt/notesSlides/notesSlide{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml"/>"#);
            have_notes = true;
            let id = format!("rId{}", cx.next_rel);
            cx.next_rel += 1;
            slide_rels.push(rel(&id, "notesSlide", &format!("../notesSlides/notesSlide{n}.xml")));
        }
        slide_rels.extend(std::mem::take(&mut cx.rels));
        files.push((format!("ppt/slides/slide{n}.xml"), slide_xml.into_bytes()));
        files.push((format!("ppt/slides/_rels/slide{n}.xml.rels"), rels_xml(&slide_rels).into_bytes()));
        let _ = write!(overrides, r#"<Override PartName="/ppt/slides/slide{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#);
        pres_rels.push(rel(&format!("rId{n}"), "slide", &format!("slides/slide{n}.xml")));
        let _ = write!(slide_ids, r#"<p:sldId id="{}" r:id="rId{n}"/>"#, 255 + n);
        slide_rels_all.clear();
    }
    let extra = pages.len() + masters.len();
    pres_rels.push(rel(&format!("rId{}", extra + 1), "presProps", "presProps.xml"));
    pres_rels.push(rel(&format!("rId{}", extra + 2), "viewProps", "viewProps.xml"));
    pres_rels.push(rel(&format!("rId{}", extra + 3), "theme", "theme/theme1.xml"));
    let mut notes_master_list = String::new();
    if have_notes {
        let g = notes_box.unwrap_or(Geom { x: 685_800, y: 4_400_550, w: 5_486_400, h: 3_600_450, ..Default::default() });
        let shapes = format!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg" idx="2"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="685800" y="1143000"/><a:ext cx="5486400" cy="3086100"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" sz="quarter" idx="1"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="en-US"/></a:p></p:txBody></p:sp>"#,
            g.x.min(1_000_000), g.y.max(4_000_000), 5_486_400, 3_600_450
        );
        let notes_master = format!(
            r#"{HEAD}<p:notesMaster xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg>{}</p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/></p:notesMaster>"#,
            tree(&shapes)
        );
        let theme_n = masters.len() + 1;
        files.push(("ppt/notesMasters/notesMaster1.xml".to_string(), notes_master.into_bytes()));
        files.push(("ppt/notesMasters/_rels/notesMaster1.xml.rels".to_string(), rels_xml(&[rel("rId1", "theme", &format!("../theme/theme{theme_n}.xml"))]).into_bytes()));
        files.push((format!("ppt/theme/theme{theme_n}.xml"), ibis_formats::new_file::theme().into_bytes()));
        let _ = write!(overrides, r#"<Override PartName="/ppt/notesMasters/notesMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml"/><Override PartName="/ppt/theme/theme{theme_n}.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#);
        pres_rels.push(rel(&format!("rId{}", extra + 4), "notesMaster", "notesMasters/notesMaster1.xml"));
        notes_master_list = format!(r#"<p:notesMasterIdLst><p:notesMasterId r:id="rId{}"/></p:notesMasterIdLst>"#, extra + 4);
    }
    let presentation_xml = format!(
        r#"{HEAD}<p:presentation xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}" saveSubsetFonts="1"><p:sldMasterIdLst>{master_ids}</p:sldMasterIdLst>{notes_master_list}<p:sldIdLst>{slide_ids}</p:sldIdLst><p:sldSz cx="{width}" cy="{height}"/><p:notesSz cx="6858000" cy="9144000"/><p:defaultTextStyle><a:defPPr><a:defRPr lang="en-US"/></a:defPPr>{DEFAULT_LEVELS}</p:defaultTextStyle></p:presentation>"#
    );
    let mut defaults = String::from(r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#);
    for ext in &media.borrow().extensions {
        let mime = match *ext {
            "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "tiff" => "image/tiff",
            "emf" => "image/x-emf",
            "wmf" => "image/x-wmf",
            "svg" => "image/svg+xml",
            _ => "application/octet-stream",
        };
        let _ = write!(defaults, r#"<Default Extension="{ext}" ContentType="{mime}"/>"#);
    }
    let content_types = format!(
        r#"{HEAD}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">{defaults}<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/presProps.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presProps+xml"/><Override PartName="/ppt/viewProps.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml"/>{overrides}</Types>"#
    );
    let mut content_types = content_types;
    {
        let media = media.borrow();
        let charts: String = media.files.iter().filter(|(n, _)| n.starts_with("ppt/charts/")).map(|(n, _)| format!(r#"<Override PartName="/{n}" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/>"#)).collect();
        content_types = content_types.replace("</Types>", &format!("{charts}</Types>"));
    }
    let mut writer = ibis_ooxml::zip::Writer::new();
    let mut add = |name: &str, data: &[u8]| writer.add(name, ibis_ooxml::zip::EntryMeta::default(), data).map_err(|e| Error::new(format!("{e}")));
    add("[Content_Types].xml", content_types.as_bytes())?;
    add("_rels/.rels", rels_xml(&[rel("rId1", "officeDocument", "ppt/presentation.xml")]).as_bytes())?;
    add("ppt/presentation.xml", presentation_xml.as_bytes())?;
    add("ppt/_rels/presentation.xml.rels", rels_xml(&pres_rels).as_bytes())?;
    add("ppt/presProps.xml", format!(r#"{HEAD}<p:presentationPr xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"/>"#).as_bytes())?;
    add("ppt/viewProps.xml", format!(r#"{HEAD}<p:viewPr xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:normalViewPr><p:restoredLeft sz="15620"/><p:restoredTop sz="94660"/></p:normalViewPr></p:viewPr>"#).as_bytes())?;
    for (name, data) in files.iter() {
        add(name, data)?;
    }
    for (name, data) in &media.borrow().files {
        add(name, data)?;
    }
    writer.finish().map_err(|e| Error::new(format!("{e}")))
}

#[allow(dead_code)]
fn unused() -> &'static str {
    XLINK
}

/// The kind of a picture from its first bytes (the extension of its file in the package when that says nothing).
fn sniff_image(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpeg")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("gif")
    } else if data.starts_with(b"BM") {
        Some("bmp")
    } else if data.starts_with(&[b'I', b'I', 42, 0]) || data.starts_with(&[b'M', b'M', 0, 42]) {
        Some("tiff")
    } else if data.len() > 44 && data.starts_with(&[1, 0, 0, 0]) && &data[40..44] == b" EMF" {
        Some("emf")
    } else if data.starts_with(&[0xD7, 0xCD, 0xC6, 0x9A]) {
        Some("wmf")
    } else {
        None
    }
}
