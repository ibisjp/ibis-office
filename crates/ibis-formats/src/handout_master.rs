// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 配布資料マスター: the handout master (`ppt/handoutMasters`: where the header,
//! date, footer and page number of a printed handout go, and how their text
//! looks) edited as a slide. `build` makes, from a presentation's file, another
//! one holding one slide of the notes page's size whose shapes are those
//! four places (made up as PowerPoint does if the file has no handout
//! master); `apply` puts what the editing did (a place's frame, the size,
//! alignment, colour and the like of its text) back into the handout master
//! of the file, made if there was none. [`layout`] reads the places for
//! printing.

use ibis_ooxml::{NodeId, Package, SaveOptions, XmlDocument};

use crate::Error;
use crate::notes_page::{main_part, xml_of};
use crate::save::pptx::{next_part_name, relative};
use crate::save::pptx_new::Prefixes;

const REL_HANDOUT_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/handoutMaster";
const REL_NOTES_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster";
const REL_SLIDE_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
const REL_THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
const REL_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const REL_SLIDE_LAYOUT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
const CT_SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
const CT_HANDOUT_MASTER: &str = "application/vnd.openxmlformats-officedocument.presentationml.handoutMaster+xml";
const CT_THEME: &str = "application/vnd.openxmlformats-officedocument.theme+xml";

/// Whether `node` or what it holds names a relationship (a picture).
fn has_relationship(doc: &XmlDocument, node: NodeId) -> bool {
    std::iter::once(node).chain(doc.descendants(node)).any(|n| doc.is_element(n) && doc.attributes(n).iter().any(|a| a.name.starts_with("r:") || a.name.contains(":embed") || a.name.contains(":link")))
}

fn err(what: impl Into<String>) -> Error {
    Error::new(what)
}

/// Which of the four places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Header,
    Date,
    Footer,
    Number,
}

impl Mark {
    const ALL: [Mark; 4] = [Mark::Header, Mark::Date, Mark::Footer, Mark::Number];

    fn ph(self) -> &'static str {
        match self {
            Mark::Header => "hdr",
            Mark::Date => "dt",
            Mark::Footer => "ftr",
            Mark::Number => "sldNum",
        }
    }

    fn from_ph(ph: &str) -> Option<Mark> {
        Mark::ALL.into_iter().find(|m| m.ph() == ph)
    }

    fn sample(self) -> &'static str {
        match self {
            Mark::Header => "ヘッダー",
            Mark::Date => "日付",
            Mark::Footer => "フッター",
            Mark::Number => "‹#›",
        }
    }
}

/// A place of the handout master, in EMU of the notes page (`page`), its text in `size_pt` points.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub mark: Mark,
    pub frame: (i64, i64, i64, i64),
    pub size_pt: f32,
    /// `l`, `ctr` or `r`.
    pub align: String,
    /// The text sits at the bottom of the place.
    pub bottom: bool,
}

/// The places of the handout master and the size of the page they are in.
#[derive(Clone, Debug, PartialEq)]
pub struct HandoutLayout {
    pub page: (i64, i64),
    pub places: Vec<Place>,
}

impl Default for HandoutLayout {
    fn default() -> Self {
        let (page, w, h) = ((6858000, 9144000), 2971800, 458788);
        let place = |mark, x, y, align: &str, bottom| Place { mark, frame: (x, y, w, h), size_pt: 12.0, align: align.to_string(), bottom };
        HandoutLayout {
            page,
            places: vec![place(Mark::Header, 0, 0, "l", false), place(Mark::Date, 3884613, 0, "r", false), place(Mark::Footer, 0, 8685213, "l", true), place(Mark::Number, 3884613, 8685213, "r", true)],
        }
    }
}

/// What PowerPoint writes for a handout master of its own.
fn default_master() -> String {
    let l = HandoutLayout::default();
    let mut shapes = String::new();
    for (i, p) in l.places.iter().enumerate() {
        let (name, idx) = match p.mark {
            Mark::Header => ("ヘッダー プレースホルダー 1", String::new()),
            Mark::Date => ("日付プレースホルダー 2", " idx=\"1\"".to_string()),
            Mark::Footer => ("フッター プレースホルダー 3", " idx=\"2\"".to_string()),
            Mark::Number => ("スライド番号プレースホルダー 4", " idx=\"3\"".to_string()),
        };
        let anchor = if p.bottom { " anchor=\"b\"" } else { "" };
        let text = if p.mark == Mark::Number { "<a:fld id=\"{D6B3A3F5-0A4B-4E58-9B87-6C1E4B5E7A10}\" type=\"slidenum\"><a:rPr lang=\"ja-JP\"/><a:t>‹#›</a:t></a:fld>".to_string() } else { "<a:endParaRPr lang=\"ja-JP\"/>".to_string() };
        shapes.push_str(&format!(
            "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"{ph}\" sz=\"quarter\"{idx}/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr vert=\"horz\" lIns=\"91440\" tIns=\"45720\" rIns=\"91440\" bIns=\"45720\" rtlCol=\"0\"{anchor}/><a:lstStyle><a:lvl1pPr algn=\"{align}\"><a:defRPr sz=\"1200\"/></a:lvl1pPr></a:lstStyle><a:p>{text}</a:p></p:txBody></p:sp>",
            id = i + 2,
            ph = p.mark.ph(),
            x = p.frame.0,
            y = p.frame.1,
            w = p.frame.2,
            h = p.frame.3,
            align = p.align,
        ));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:handoutMaster xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/></p:handoutMaster>"
    )
}

fn handout_part(package: &Package, main: &str) -> Result<Option<String>, Error> {
    Ok(package.relationships(main)?.into_iter().find(|r| r.kind == REL_HANDOUT_MASTER && !r.external).and_then(|r| r.target_part(main)).filter(|p| package.contains(p)))
}

fn notes_size(package: &Package, main: &str) -> Result<(i64, i64), Error> {
    let doc = package.xml(main)?;
    let root = doc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
    let get = |a: &str, d: i64| doc.child(root, "notesSz").and_then(|n| doc.attr(n, a)).and_then(|v| v.parse().ok()).unwrap_or(d);
    Ok((get("cx", 6858000), get("cy", 9144000)))
}

fn master_doc(package: &Package, main: &str) -> Result<XmlDocument, Error> {
    match handout_part(package, main)? {
        Some(part) => Ok(package.xml(&part)?),
        None => XmlDocument::parse(default_master().as_bytes()).map_err(|e| err(format!("配布資料マスターを作れません: {e}"))),
    }
}

/// The (id, name, mark, node) of each place of a handout master tree.
fn places_of(doc: &XmlDocument) -> Vec<(String, Mark, NodeId)> {
    let Some(root) = doc.root_element() else { return Vec::new() };
    let Some(tree) = doc.path(root, &["cSld", "spTree"]) else { return Vec::new() };
    doc.children_named(tree, "sp")
        .filter_map(|sp| {
            let ph = doc.path(sp, &["nvSpPr", "nvPr", "ph"])?;
            let mark = Mark::from_ph(doc.attr(ph, "type")?)?;
            let id = doc.path(sp, &["nvSpPr", "cNvPr"]).and_then(|c| doc.attr(c, "id")).unwrap_or("0").to_string();
            Some((id, mark, sp))
        })
        .collect()
}

fn place_of(doc: &XmlDocument, mark: Mark, sp: NodeId) -> Place {
    let default = HandoutLayout::default().places.into_iter().find(|p| p.mark == mark).expect("a place");
    let num = |n: Option<NodeId>, a: &str| n.and_then(|n| doc.attr(n, a)).and_then(|v| v.parse::<i64>().ok());
    let xfrm = doc.path(sp, &["spPr", "xfrm"]);
    let (off, ext) = (xfrm.and_then(|x| doc.child(x, "off")), xfrm.and_then(|x| doc.child(x, "ext")));
    let frame = match (num(off, "x"), num(off, "y"), num(ext, "cx"), num(ext, "cy")) {
        (Some(x), Some(y), Some(w), Some(h)) => (x, y, w, h),
        _ => default.frame,
    };
    let level = doc.path(sp, &["txBody", "lstStyle", "lvl1pPr"]);
    let size = level.and_then(|l| doc.child(l, "defRPr")).and_then(|d| doc.attr(d, "sz")).and_then(|v| v.parse::<f32>().ok()).map_or(default.size_pt, |v| v / 100.0);
    let align = level.and_then(|l| doc.attr(l, "algn")).map_or(default.align.clone(), str::to_string);
    let bottom = doc.path(sp, &["txBody", "bodyPr"]).and_then(|b| doc.attr(b, "anchor")).map_or(default.bottom, |a| a == "b");
    Place { mark, frame, size_pt: size, align, bottom }
}

/// The places of the handout master of `data` (the usual ones if it has none).
pub fn layout(data: &[u8]) -> Result<HandoutLayout, Error> {
    let package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    let page = notes_size(&package, &main)?;
    let doc = master_doc(&package, &main)?;
    let mut places = Vec::new();
    for mark in Mark::ALL {
        if let Some((_, _, sp)) = places_of(&doc).into_iter().find(|(_, m, _)| *m == mark) {
            places.push(place_of(&doc, mark, sp));
        }
    }
    Ok(HandoutLayout { page, places })
}


/// Which of the four places of the handout master are on (its `p:hf`: each of `hdr`, `dt`, `ftr` and `sldNum` is on unless it says
/// `0`; a file with no handout master, or no `p:hf`, has them all on, as PowerPoint makes one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderFooterFlags {
    pub header: bool,
    pub date: bool,
    pub footer: bool,
    pub number: bool,
}

pub fn header_footer_flags(data: &[u8]) -> Result<HeaderFooterFlags, Error> {
    let package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    let doc = master_doc(&package, &main)?;
    let hf = doc.root_element().and_then(|root| doc.child(root, "hf"));
    let on = |name: &str| !hf.and_then(|n| doc.attr(n, name)).is_some_and(|v| v == "0" || v == "false");
    Ok(HeaderFooterFlags { header: on("hdr"), date: on("dt"), footer: on("ftr"), number: on("sldNum") })
}

/// Where a notes page puts the slide's picture and the notes' text, in EMU of the notes page (`page`), as the notes master says.
#[derive(Clone, Debug, PartialEq)]
pub struct NotesFrames {
    pub page: (i64, i64),
    /// The slide image placeholder (`sldImg`).
    pub image: (i64, i64, i64, i64),
    /// The notes' body placeholder.
    pub body: (i64, i64, i64, i64),
}

/// The slide image and the body of the notes master of `data`. A file without a notes master gets what PowerPoint makes: the image
/// 5486400 EMU wide at (685800, 1143000) with the slide's proportions, the body under it.
pub fn notes_frames(data: &[u8]) -> Result<NotesFrames, Error> {
    let package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    let page = notes_size(&package, &main)?;
    let slide = {
        let doc = package.xml(&main)?;
        let root = doc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
        let get = |a: &str| doc.child(root, "sldSz").and_then(|n| doc.attr(n, a)).and_then(|v| v.parse::<i64>().ok());
        (get("cx").unwrap_or(9144000), get("cy").unwrap_or(6858000))
    };
    let image_default = (685800, 1143000, 5486400, (5486400.0 * slide.1 as f64 / slide.0.max(1) as f64).round() as i64);
    let body_default = (685800, 4400550, 5486400, 3600450);
    let mut frames = NotesFrames { page, image: image_default, body: body_default };
    let master = package.relationships(&main)?.into_iter().find(|r| r.kind == REL_NOTES_MASTER && !r.external).and_then(|r| r.target_part(&main)).filter(|p| package.contains(p));
    if let Some(part) = master {
        let doc = package.xml(&part)?;
        let tree = doc.root_element().and_then(|root| doc.path(root, &["cSld", "spTree"]));
        for sp in tree.into_iter().flat_map(|t| doc.children_named(t, "sp").collect::<Vec<_>>()) {
            let Some(ph) = doc.path(sp, &["nvSpPr", "nvPr", "ph"]) else { continue };
            let num = |n: Option<NodeId>, a: &str| n.and_then(|n| doc.attr(n, a)).and_then(|v| v.parse::<i64>().ok());
            let xfrm = doc.path(sp, &["spPr", "xfrm"]);
            let (off, ext) = (xfrm.and_then(|x| doc.child(x, "off")), xfrm.and_then(|x| doc.child(x, "ext")));
            let (Some(x), Some(y), Some(w), Some(h)) = (num(off, "x"), num(off, "y"), num(ext, "cx"), num(ext, "cy")) else { continue };
            match doc.attr(ph, "type") {
                Some("sldImg") => frames.image = (x, y, w, h),
                Some("body") => frames.body = (x, y, w, h),
                _ => {}
            }
        }
    }
    Ok(frames)
}

/// A file like `data` with one slide of the notes page's size, the places of its handout master as its text boxes.
pub fn build(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    let size = notes_size(&package, &main)?;
    let master = master_doc(&package, &main)?;
    let mut shapes = String::new();
    for (id, mark, sp) in places_of(&master) {
        let place = place_of(&master, mark, sp);
        let xfrm = format!("<a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>", place.frame.0, place.frame.1, place.frame.2, place.frame.3);
        let name = master.path(sp, &["nvSpPr", "cNvPr"]).and_then(|c| master.attr(c, "name")).unwrap_or("").replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;");
        // The master's text properties, which a text box of the slide takes as its own.
        let level = master.path(sp, &["txBody", "lstStyle", "lvl1pPr"]);
        let rpr = level.and_then(|l| master.child(l, "defRPr")).map(|d| {
            let attrs: String = master.attributes(d).iter().map(|a| format!(" {}=\"{}\"", a.name, a.value.replace('&', "&amp;").replace('"', "&quot;"))).collect();
            let inner: String = master.elements(d).map(|c| xml_of(c, &master)).collect();
            (attrs, inner)
        });
        let (attrs, inner) = rpr.unwrap_or_default();
        let attrs = if attrs.contains(" sz=") { attrs } else { format!("{attrs} sz=\"1200\"") };
        let anchor = if place.bottom { "b" } else { "t" };
        shapes.push_str(&format!(
            "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr>{xfrm}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:ln w=\"6350\"><a:solidFill><a:srgbClr val=\"BFBFBF\"/></a:solidFill><a:prstDash val=\"dash\"/></a:ln></p:spPr><p:txBody><a:bodyPr wrap=\"square\" lIns=\"91440\" tIns=\"45720\" rIns=\"91440\" bIns=\"45720\" rtlCol=\"0\" anchor=\"{anchor}\"><a:noAutofit/></a:bodyPr><a:lstStyle/><a:p><a:pPr algn=\"{align}\"/><a:r><a:rPr lang=\"ja-JP\"{attrs}>{inner}</a:rPr><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp>",
            align = place.align,
            text = mark.sample(),
        ));
    }
    let mut doc = XmlDocument::parse(
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" showMasterSp=\"0\"><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{shapes}</p:spTree></p:cSld></p:sld>").as_bytes(),
    )
    .map_err(|e| err(format!("配布資料マスターを開けません: {e}")))?;
    // The handout master's own background (背景), when it is one without a picture: the page's.
    if let Some(bg) = master.root_element().and_then(|r| master.child(r, "cSld")).and_then(|c| master.child(c, "bg")).filter(|&b| !has_relationship(&master, b)) {
        let csld = doc.root_element().and_then(|r| doc.child(r, "cSld"));
        if let Some(csld) = csld {
            if let Some(old) = doc.child(csld, "bg") {
                doc.detach(old);
            }
            let copy = doc.import(&master, bg);
            doc.insert(csld, 0, copy);
        }
    }
    let temp = next_part_name(&package, "ppt/slides/slide1.xml");
    package.set_xml(&temp, doc);
    package.set_override(&temp, CT_SLIDE)?;
    // The first slide's layout, emptied.
    let first = {
        let d = package.xml(&main)?;
        let root = d.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
        let list = d.child(root, "sldIdLst").ok_or_else(|| err("スライドの一覧がありません"))?;
        let entry = d.elements(list).next().ok_or_else(|| err("スライドがありません"))?;
        let rid = crate::notes_page::rel_id(&d, entry).ok_or_else(|| err("スライドの関係がありません"))?;
        package.relationships(&main)?.into_iter().find(|r| r.id == rid).and_then(|r| r.target_part(&main)).ok_or_else(|| err("スライドの部品がありません"))?
    };
    let layout = package.relationships(&first)?.into_iter().find(|r| r.kind == REL_SLIDE_LAYOUT && !r.external).and_then(|r| r.target_part(&first)).ok_or_else(|| err("スライドのレイアウトがありません"))?;
    {
        let ldoc = package.xml_mut(&layout)?;
        let lroot = ldoc.root_element().ok_or_else(|| err("レイアウトが空です"))?;
        let tree = ldoc.path(lroot, &["cSld", "spTree"]).ok_or_else(|| err("レイアウトの図形の木がありません"))?;
        for child in ldoc.children(tree).to_vec() {
            if ldoc.is_element(child) && !matches!(ldoc.local_name(child), "nvGrpSpPr" | "grpSpPr") {
                ldoc.detach(child);
            }
        }
    }
    package.add_relationship(&temp, REL_SLIDE_LAYOUT, &relative(&temp, &layout), false)?;
    let rid = package.add_relationship(&main, REL_SLIDE, &relative(&main, &temp), false)?;
    let pdoc = package.xml_mut(&main)?;
    let proot = pdoc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
    let list = pdoc.child(proot, "sldIdLst").ok_or_else(|| err("スライドの一覧がありません"))?;
    for child in pdoc.children(list).to_vec() {
        pdoc.detach(child);
    }
    let x = Prefixes::at(pdoc, list);
    let entry = x.import(pdoc, &format!("<{p}sldId id=\"256\" {r}id=\"{rid}\"/>", p = x.p, r = x.r))?;
    pdoc.insert(list, 0, entry);
    if let Some(sz) = pdoc.child(proot, "sldSz") {
        pdoc.set_attr(sz, "cx", &size.0.to_string());
        pdoc.set_attr(sz, "cy", &size.1.to_string());
        pdoc.remove_attr(sz, "type");
    }
    if let Some(shows) = pdoc.child(proot, "custShowLst") {
        pdoc.detach(shows);
    }
    if let Some(ext) = pdoc.child(proot, "extLst") {
        for e in pdoc.elements(ext).collect::<Vec<_>>() {
            if pdoc.descendants(e).into_iter().any(|d| pdoc.is_element(d) && pdoc.local_name(d) == "sectionLst") {
                pdoc.detach(e);
            }
        }
    }
    Ok(package.save(SaveOptions::default())?)
}

/// The handout master of the file made if it has none (with a theme of the file's own copied).
fn ensure_master(package: &mut Package, main: &str) -> Result<String, Error> {
    if let Some(part) = handout_part(package, main)? {
        return Ok(part);
    }
    let part = next_part_name(package, "ppt/handoutMasters/handoutMaster1.xml");
    package.set_xml(&part, XmlDocument::parse(default_master().as_bytes()).map_err(|e| err(format!("配布資料マスターを作れません: {e}")))?);
    package.set_override(&part, CT_HANDOUT_MASTER)?;
    // Its theme: a copy of the notes master's, else the slide master's.
    let source = [REL_NOTES_MASTER, REL_SLIDE_MASTER].iter().find_map(|kind| {
        let master = package.relationships(main).ok()?.into_iter().find(|r| r.kind == *kind && !r.external).and_then(|r| r.target_part(main))?;
        package.relationships(&master).ok()?.into_iter().find(|r| r.kind == REL_THEME && !r.external).and_then(|r| r.target_part(&master))
    });
    if let Some(theme) = source {
        let copy = next_part_name(package, "ppt/theme/theme1.xml");
        let doc = package.xml(&theme)?;
        package.set_xml(&copy, doc);
        package.set_override(&copy, CT_THEME)?;
        package.add_relationship(&part, REL_THEME, &relative(&part, &copy), false)?;
    }
    let rid = package.add_relationship(main, REL_HANDOUT_MASTER, &relative(main, &part), false)?;
    let pdoc = package.xml_mut(main)?;
    let root = pdoc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
    if pdoc.child(root, "handoutMasterIdLst").is_none() {
        // After the slide masters and the notes master, before the slides.
        let at = pdoc.children(root).iter().rposition(|&c| pdoc.is_element(c) && matches!(pdoc.local_name(c), "sldMasterIdLst" | "notesMasterIdLst")).map_or(0, |i| i + 1);
        let x = Prefixes::at(pdoc, root);
        let list = x.import(pdoc, &format!("<{p}handoutMasterIdLst><{p}handoutMasterId {r}id=\"{rid}\"/></{p}handoutMasterIdLst>", p = x.p, r = x.r))?;
        pdoc.insert(root, at, list);
    }
    Ok(part)
}

/// `data` with the places of its handout master as the text boxes of the slide of `edited` (a file `build` made from `data`, then edited) are.
pub fn apply(data: &[u8], edited: &[u8]) -> Result<Vec<u8>, Error> {
    let mut package = Package::open(data.to_vec())?;
    let edited = Package::open(edited.to_vec())?;
    let emain = main_part(&edited)?;
    let eslide = {
        let d = edited.xml(&emain)?;
        let root = d.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
        let list = d.child(root, "sldIdLst").ok_or_else(|| err("スライドの一覧がありません"))?;
        let entry = d.elements(list).next().ok_or_else(|| err("スライドがありません"))?;
        let rid = crate::notes_page::rel_id(&d, entry).ok_or_else(|| err("スライドの関係がありません"))?;
        edited.relationships(&emain)?.into_iter().find(|r| r.id == rid).and_then(|r| r.target_part(&emain)).ok_or_else(|| err("スライドの部品がありません"))?
    };
    let edoc = edited.xml(&eslide)?;
    let eroot = edoc.root_element().ok_or_else(|| err("スライドが空です"))?;
    let etree = edoc.path(eroot, &["cSld", "spTree"]).ok_or_else(|| err("スライドの図形の木がありません"))?;
    let main = main_part(&package)?;
    let part = ensure_master(&mut package, &main)?;
    let mut doc = package.xml(&part)?;
    // The background as edited on the page (one without a picture).
    if let (Some(ebg), Some(csld)) = (edoc.child(eroot, "cSld").and_then(|c| edoc.child(c, "bg")).filter(|&b| !has_relationship(&edoc, b)), doc.root_element().and_then(|r| doc.child(r, "cSld"))) {
        if let Some(old) = doc.child(csld, "bg") {
            doc.detach(old);
        }
        let copy = doc.import(&edoc, ebg);
        doc.insert(csld, 0, copy);
    }
    let places = places_of(&doc);
    for (id, _, sp) in places {
        let found = edoc.children_named(etree, "sp").find(|&e| edoc.path(e, &["nvSpPr", "cNvPr"]).and_then(|c| edoc.attr(c, "id")) == Some(id.as_str()));
        let Some(e) = found else {
            // Taken away on the page: no place of that kind.
            doc.detach(sp);
            continue;
        };
        // The frame.
        if let Some(exfrm) = edoc.path(e, &["spPr", "xfrm"]) {
            let copy = doc.import(&edoc, exfrm);
            let props = match doc.child(sp, "spPr") {
                Some(p) => p,
                None => {
                    let x = Prefixes::at(&doc, sp);
                    let p = x.import(&mut doc, &format!("<{p}spPr/>", p = x.p))?;
                    let at = doc.children(sp).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "txBody").unwrap_or(doc.children(sp).len());
                    doc.insert(sp, at, p);
                    p
                }
            };
            if let Some(old) = doc.child(props, "xfrm") {
                doc.detach(old);
            }
            doc.insert(props, 0, copy);
        }
        // The text: the first run's look, and the paragraph's alignment, as the place's own.
        let Some(tx) = doc.child(sp, "txBody") else { continue };
        let etx = edoc.child(e, "txBody");
        if let (Some(ebody), Some(body)) = (etx.and_then(|t| edoc.child(t, "bodyPr")), doc.child(tx, "bodyPr")) {
            match edoc.attr(ebody, "anchor") {
                Some(a) if a != "t" => doc.set_attr(body, "anchor", a),
                _ => doc.remove_attr(body, "anchor"),
            }
        }
        let paragraph = etx.and_then(|t| edoc.children_named(t, "p").next());
        let algn = paragraph.and_then(|p| edoc.child(p, "pPr")).and_then(|p| edoc.attr(p, "algn")).unwrap_or("l").to_string();
        let run = paragraph.and_then(|p| edoc.children_named(p, "r").next());
        let rpr = run.and_then(|r| edoc.child(r, "rPr"));
        let lst = match doc.child(tx, "lstStyle") {
            Some(l) => l,
            None => {
                let x = Prefixes::at(&doc, tx);
                let l = x.import(&mut doc, &format!("<{a}lstStyle/>", a = x.a))?;
                doc.insert(tx, 1, l);
                l
            }
        };
        if let Some(old) = doc.child(lst, "lvl1pPr") {
            doc.detach(old);
        }
        let x = Prefixes::at(&doc, lst);
        let level = x.import(&mut doc, &format!("<{a}lvl1pPr algn=\"{algn}\"><{a}defRPr/></{a}lvl1pPr>", a = x.a))?;
        doc.insert(lst, 0, level);
        if let (Some(rpr), Some(def)) = (rpr, doc.child(level, "defRPr")) {
            for a in edoc.attributes(rpr) {
                if matches!(a.name.as_str(), "sz" | "b" | "i" | "u" | "strike" | "baseline" | "spc") {
                    doc.set_attr(def, &a.name, &a.value);
                }
            }
            for c in edoc.elements(rpr).collect::<Vec<_>>() {
                if matches!(edoc.local_name(c), "solidFill" | "latin" | "ea" | "cs") {
                    let copy = doc.import(&edoc, c);
                    let at = doc.children(def).len();
                    doc.insert(def, at, copy);
                }
            }
        }
    }
    package.set_xml(&part, doc);
    Ok(package.save(SaveOptions::default())?)
}

/// The header, date, footer and page number the notes carry (挿入 > ヘッダーとフッター > ノートと配布資料) put as the text of
/// the places of the handout master (made if the file has none and `marks` has something): the header, date and footer
/// places hold what `marks` say (empty where it has none), the number place its field. A place taken away stays away.
pub(crate) fn write_marks(package: &mut Package, main: &str, marks: &ibis_doc_model::NotesMarks) -> Result<(), Error> {
    use ibis_doc_model::NotesDate;
    if marks.is_empty() && handout_part(package, main)?.is_none() {
        return Ok(());
    }
    let part = ensure_master(package, main)?;
    let doc = package.xml_mut(&part)?;
    for (_, mark, sp) in places_of(doc) {
        let (text, field): (String, Option<&str>) = match mark {
            Mark::Header => (marks.header.clone().unwrap_or_default(), None),
            Mark::Footer => (marks.footer.clone().unwrap_or_default(), None),
            Mark::Date => match &marks.date {
                Some(NotesDate::Auto(t)) => (t.clone(), Some("datetime1")),
                Some(NotesDate::Fixed(t)) => (t.clone(), None),
                None => (String::new(), None),
            },
            Mark::Number => ("‹#›".to_string(), Some("slidenum")),
        };
        let Some(body) = doc.child(sp, "txBody") else { continue };
        for p in doc.children_named(body, "p").collect::<Vec<_>>() {
            doc.detach(p);
        }
        let x = Prefixes::at(doc, body);
        let wrapper = format!("<{a}txBody>{}</{a}txBody>", crate::save::pptx_notes::mark_paragraph(&x.a, &text, field), a = x.a);
        let new_body = x.import(doc, &wrapper)?;
        for p in doc.children(new_body).to_vec() {
            if doc.is_element(p) {
                let end = doc.children(body).len();
                doc.insert(body, end, p);
            }
        }
    }
    Ok(())
}
