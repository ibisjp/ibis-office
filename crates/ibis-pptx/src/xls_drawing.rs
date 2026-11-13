// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pictures of an old workbook (`.xls`), from the public specification (MS-XLS, MS-ODRAW): the
//! images of the workbook's drawing group (`MSODRAWINGGROUP`: the BLIP store) and, on each sheet, the
//! shapes of its drawing (`MSODRAWING`) that show one, with their anchors.

use crate::xls::u16_at;

/// An image of the BLIP store: its file extension and bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Image {
    pub ext: &'static str,
    pub data: Vec<u8>,
}

/// A shape of a sheet's drawing that shows a picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Picture {
    /// The image, 1 for the first of the store.
    pub blip: usize,
    /// The anchor: first column, offset in 1/1024 of its width, first row, offset in 1/256 of its height,
    /// then the same for the last.
    pub anchor: [u16; 8],
    /// The picture cropped: the fractions left out at the top, bottom, left and right (16.16 fixed point; MS-ODRAW
    /// cropFromTop … cropFromRight, 0x0100 to 0x0103).
    pub crop: [i32; 4],
}

struct Record<'a> {
    version: u16,
    instance: u16,
    kind: u16,
    data: &'a [u8],
}

/// The records of an OfficeArt stream, one level.
fn records(mut d: &[u8]) -> Vec<Record<'_>> {
    let mut out = Vec::new();
    while d.len() >= 8 {
        let head = u16_at(d, 0).unwrap_or(0);
        let kind = u16_at(d, 2).unwrap_or(0);
        let len = u32::from_le_bytes([d[4], d[5], d[6], d[7]]) as usize;
        let Some(data) = d.get(8..8 + len) else { break };
        out.push(Record { version: head & 0xF, instance: head >> 4, kind, data });
        d = &d[8 + len..];
    }
    out
}

/// A DIB (the bitmap without a file header) as a `.bmp` file.
fn dib_to_bmp(dib: &[u8]) -> Option<Vec<u8>> {
    let size = u32::from_le_bytes(dib.get(0..4)?.try_into().ok()?) as usize;
    let bits = u16_at(dib, 14)? as u32;
    let colors = u32::from_le_bytes(dib.get(32..36)?.try_into().ok()?);
    let palette = if bits <= 8 { (if colors == 0 { 1u32 << bits } else { colors }) * 4 } else { 0 };
    let offset = 14 + size as u32 + palette;
    let mut out = Vec::with_capacity(14 + dib.len());
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(14 + dib.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(dib);
    Some(out)
}

/// The images of the drawing group, in the BLIP store's order (`None` for one this does not read: a metafile,
/// or one stored outside).
pub(crate) fn read_blips(group: &[u8]) -> Vec<Option<Image>> {
    let mut out = Vec::new();
    for dgg in records(group).into_iter().filter(|r| r.kind == 0xF000) {
        for store in records(dgg.data).into_iter().filter(|r| r.kind == 0xF001) {
            for entry in records(store.data) {
                out.push(blip_of(&entry));
            }
        }
    }
    out
}

fn blip_of(entry: &Record<'_>) -> Option<Image> {
    // A file block: a BSE (header of 36 bytes, a name, the BLIP) or a BLIP itself.
    let blip = if entry.kind == 0xF007 {
        let name = *entry.data.get(33)? as usize;
        records(entry.data.get(36 + name..)?).into_iter().next()?
    } else {
        Record { version: entry.version, instance: entry.instance, kind: entry.kind, data: entry.data }
    };
    // Two identifiers when the instance's low bit is set.
    let uids = if blip.instance & 1 == 1 { 32 } else { 16 };
    let body = blip.data.get(uids + 1..)?;
    match blip.kind {
        0xF01D | 0xF02A => Some(Image { ext: "jpeg", data: body.to_vec() }),
        0xF01E => Some(Image { ext: "png", data: body.to_vec() }),
        0xF01F => dib_to_bmp(body).map(|data| Image { ext: "bmp", data }),
        0xF029 => Some(Image { ext: "tiff", data: body.to_vec() }),
        _ => None,
    }
}

/// The pictures of a sheet's drawing, in order.
pub(crate) fn read_pictures(drawing: &[u8]) -> Vec<Picture> {
    let mut out = Vec::new();
    fn walk(d: &[u8], out: &mut Vec<Picture>) {
        for r in records(d) {
            match r.kind {
                0xF004 => {
                    let mut blip = None;
                    let mut anchor = None;
                    let mut crop = [0i32; 4];
                    for part in records(r.data) {
                        match part.kind {
                            // The shape's properties: a 6-byte entry each (id, value); id 0x0104 is the picture.
                            0xF00B => {
                                for k in 0..part.instance as usize {
                                    let (Some(id), Some(v)) = (u16_at(part.data, k * 6), part.data.get(k * 6 + 2..k * 6 + 6)) else { break };
                                    let value = u32::from_le_bytes([v[0], v[1], v[2], v[3]]);
                                    match id & 0x3FFF {
                                        0x0104 => blip = Some(value as usize),
                                        k @ 0x0100..=0x0103 => crop[(k - 0x0100) as usize] = value as i32,
                                        _ => {}
                                    }
                                }
                            }
                            0xF010 => {
                                if part.data.len() >= 18 {
                                    let at = |k: usize| u16_at(part.data, 2 + k * 2).unwrap_or(0);
                                    anchor = Some([at(0), at(1), at(2), at(3), at(4), at(5), at(6), at(7)]);
                                }
                            }
                            _ => {}
                        }
                    }
                    if let (Some(blip), Some(anchor)) = (blip, anchor)
                        && blip > 0
                    {
                        out.push(Picture { blip, anchor, crop });
                    }
                }
                _ if r.version == 0xF => walk(r.data, out),
                _ => {}
            }
        }
    }
    walk(drawing, &mut out);
    out
}

/// The anchor of the last shape of a drawing piece: first column, offset, first row, offset, then the last.
/// A piece may stop inside a container (the first one holds the start of the group, whose length covers
/// every shape of the sheet): what is there is read.
pub(crate) fn last_anchor(drawing: &[u8]) -> Option<[u16; 8]> {
    fn walk(mut d: &[u8], out: &mut Option<[u16; 8]>) {
        while d.len() >= 8 {
            let head = u16_at(d, 0).unwrap_or(0);
            let kind = u16_at(d, 2).unwrap_or(0);
            let len = u32::from_le_bytes([d[4], d[5], d[6], d[7]]) as usize;
            let container = head & 0xF == 0xF;
            let end = (8 + len).min(d.len());
            let body = &d[8..end];
            if kind == 0xF010 && body.len() >= 18 {
                let at = |k: usize| u16_at(body, 2 + k * 2).unwrap_or(0);
                *out = Some([at(0), at(1), at(2), at(3), at(4), at(5), at(6), at(7)]);
            } else if container {
                walk(body, out);
            }
            d = &d[end..];
        }
    }
    let mut out = None;
    walk(drawing, &mut out);
    out
}

/// The body of the last record of a kind (`0xF009` FSPGR, `0xF00F` ChildAnchor) in a piece of a drawing: the innermost
/// shape's (the piece of a group's first shape holds the drawing's own top group before it).
pub(crate) fn last_body(drawing: &[u8], kind: u16) -> Option<&[u8]> {
    let mut d = drawing;
    let mut found = None;
    while d.len() >= 8 {
        let head = u16_at(d, 0).unwrap_or(0);
        let k = u16_at(d, 2).unwrap_or(0);
        let len = u32::from_le_bytes([d[4], d[5], d[6], d[7]]) as usize;
        let end = (8 + len).min(d.len());
        let body = &d[8..end];
        if k == kind {
            found = Some(body);
        } else if head & 0xF == 0xF
            && let Some(inner) = last_body(body, kind)
        {
            found = Some(inner);
        }
        d = &d[end..];
    }
    found
}

/// A rectangle of four 32-bit numbers (left, top, right, bottom: FSPGR, ChildAnchor).
pub(crate) fn rect_of(body: &[u8]) -> Option<[i32; 4]> {
    let at = |k: usize| body.get(k * 4..k * 4 + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    Some([at(0)?, at(1)?, at(2)?, at(3)?])
}

// ---------------------------------------------------------------------------------------------------------------
// The other shapes of a sheet's drawing: autoshapes, text boxes, lines, freeforms and groups (MS-XLS 2.4.181 Obj,
// 2.4.329 TxO; MS-ODRAW). Their look is read as a presentation's (`ppt::read_shape`) and written as DrawingML by
// `officeart_xml`; the text of a shape comes in the `TXO` and `CONTINUE` records after its `OBJ`.
// ---------------------------------------------------------------------------------------------------------------

use crate::ppt::{Props, Rec, ShapeIr};

/// The text of a drawing object (`TXO`): its characters, the runs (the first character of each and its font index,
/// `FontIndex`), the alignment across (`hAlignment`) and down (`vAlignment`) and the orientation (`rot`).
#[derive(Clone, Debug, Default)]
pub(crate) struct ObjText {
    pub text: String,
    pub runs: Vec<(usize, u16)>,
    pub h_align: u8,
    pub v_align: u8,
    pub rot: u16,
}

/// The text being read: the shape's id, the text so far, the characters and run bytes to come, the run bytes so
/// far, and whether the characters are all in.
type Reading = (u32, ObjText, usize, usize, Vec<u8>, bool);

/// The objects of a sheet (`OBJ`) by their shape's id: what each is (`ftCmo` ot) and its text.
#[derive(Default)]
pub(crate) struct ObjReader {
    current: Option<u32>,
    reading: Option<Reading>,
    pub kinds: std::collections::HashMap<u32, u16>,
    pub texts: std::collections::HashMap<u32, ObjText>,
    /// The objects that are not printed (`ftCmo`'s fPrint off: 「オブジェクトを印刷する」 cleared).
    pub no_print: std::collections::HashSet<u32>,
}

impl ObjReader {
    /// An `OBJ`, after the drawing piece `shape` of its shape.
    pub(crate) fn object(&mut self, d: &[u8], shape: &[u8]) {
        self.current = last_spid(shape);
        if let (Some(spid), Some(ot)) = (self.current, u16_at(d, 4)) {
            self.kinds.insert(spid, ot);
            // ftCmo (MS-XLS 2.5.143): ft, cb, ot, id, then the flags; fPrint is bit 4.
            if u16_at(d, 8).is_some_and(|f| f & 0x10 == 0) {
                self.no_print.insert(spid);
            }
        }
    }

    /// Whether a text box's text still has characters or runs to come (in `CONTINUE`s).
    pub(crate) fn wants_more(&self) -> bool {
        self.reading.is_some()
    }

    /// A `TXO`: the alignment, and how many characters and bytes of runs come in the `CONTINUE`s.
    pub(crate) fn text_box(&mut self, d: &[u8]) {
        // (A text whose shape was not found is still read, so that its CONTINUEs are not taken for the drawing's.)
        let spid = self.current.take().unwrap_or(0);
        let (Some(grbit), Some(rot), Some(cch), Some(cb_runs)) = (u16_at(d, 0), u16_at(d, 2), u16_at(d, 10), u16_at(d, 12)) else {
            self.reading = None;
            return;
        };
        let text = ObjText { h_align: ((grbit >> 1) & 7) as u8, v_align: ((grbit >> 4) & 7) as u8, rot, ..Default::default() };
        self.reading = Some((spid, text, cch as usize, cb_runs as usize, Vec::new(), cch == 0));
        self.finish_if_done();
    }

    /// A `CONTINUE` (not of the drawing): the characters (after a flag byte), then the runs.
    pub(crate) fn continued(&mut self, d: &[u8]) {
        let Some((_, text, cch, _, runs, text_done)) = self.reading.as_mut() else { return };
        if !*text_done {
            let Some(&flag) = d.first() else { return };
            let body = &d[1..];
            let chunk: String = if flag & 1 != 0 { String::from_utf16_lossy(&(0..body.len() / 2).filter_map(|k| u16_at(body, k * 2)).collect::<Vec<_>>()) } else { body.iter().map(|&b| b as char).collect() };
            text.text.push_str(&chunk);
            if text.text.chars().count() >= *cch {
                text.text = text.text.chars().take(*cch).collect();
                *text_done = true;
            }
        } else {
            runs.extend_from_slice(d);
        }
        self.finish_if_done();
    }

    fn finish_if_done(&mut self) {
        let done = matches!(&self.reading, Some((_, _, _, cb, runs, true)) if runs.len() >= *cb);
        if !done {
            return;
        }
        let Some((spid, mut text, cch, _, runs, _)) = self.reading.take() else { return };
        // TxORuns (MS-XLS 2.5.272): 8 bytes a run, a FormatRun (ich, ifnt) and 4 unused; the last one marks the end
        // of the text.
        for k in 0..runs.len() / 8 {
            let (Some(ich), Some(ifnt)) = (u16_at(&runs, k * 8), u16_at(&runs, k * 8 + 2)) else { break };
            if (ich as usize) < cch {
                text.runs.push((ich as usize, ifnt));
            }
        }
        if spid != 0 {
            self.texts.insert(spid, text);
        }
    }
}

/// The id of the last shape (`OfficeArtFSP`) of a drawing piece.
pub(crate) fn last_spid(drawing: &[u8]) -> Option<u32> {
    fn walk(mut d: &[u8], out: &mut Option<u32>) {
        while d.len() >= 8 {
            let head = u16_at(d, 0).unwrap_or(0);
            let kind = u16_at(d, 2).unwrap_or(0);
            let len = u32::from_le_bytes([d[4], d[5], d[6], d[7]]) as usize;
            let end = (8 + len).min(d.len());
            let body = &d[8..end];
            if kind == 0xF00A && body.len() >= 4 {
                *out = Some(u32::from_le_bytes([body[0], body[1], body[2], body[3]]));
            } else if head & 0xF == 0xF {
                walk(body, out);
            }
            d = &d[end..];
        }
    }
    let mut out = None;
    walk(drawing, &mut out);
    out
}

/// The default properties of the workbook's shapes: the drawing group's `OfficeArtFOPT`.
pub(crate) fn read_default_props(group: &[u8]) -> Props {
    let mut props = Props::default();
    for dgg in crate::ppt::records(group).into_iter().filter(|r| r.ty == 0xF000) {
        for r in dgg.children().into_iter().filter(|r| matches!(r.ty, 0xF00B | 0xF122)) {
            Props::parse(&r, &mut props);
        }
    }
    props
}

/// The anchor of a shape container (`OfficeArtClientAnchorSheet`): first column, offset in 1/1024 of its width,
/// first row, offset in 1/256 of its height, then the same for the last.
fn anchor_of(sp: &Rec<'_>) -> Option<[u16; 8]> {
    let a = sp.child(0xF010)?;
    (a.data.len() >= 18).then(|| std::array::from_fn(|k| u16_at(a.data, 2 + k * 2).unwrap_or(0)))
}

/// The anchor of a shape in a chart's own drawing (`OfficeArtClientAnchorChart`): two unused bytes, then
/// the left, top, right and bottom edges as 4-byte integers, in 1/4000 of the chart's width and height.
fn chart_anchor_of(sp: &Rec<'_>) -> Option<[i32; 4]> {
    let a = sp.child(0xF010)?;
    (a.data.len() >= 18).then(|| std::array::from_fn(|k| crate::xls::u32_at(a.data, 2 + k * 4).unwrap_or(0) as i32))
}

/// The shapes at the top of a sheet's drawing, each with its anchor on the sheet (a group with the shapes in it).
pub(crate) fn read_shapes(drawing: &[u8]) -> Vec<(ShapeIr, [u16; 8])> {
    read_shapes_with(drawing, anchor_of)
}

/// The shapes drawn on a chart (its own `MSODRAWING`: text boxes, arrows and the like over the plot), each with its place
/// in 1/4000 of the chart.
pub(crate) fn read_chart_shapes(drawing: &[u8]) -> Vec<(ShapeIr, [i32; 4])> {
    read_shapes_with(drawing, chart_anchor_of)
}

fn read_shapes_with<A>(drawing: &[u8], anchor_of: impl Fn(&Rec<'_>) -> Option<A>) -> Vec<(ShapeIr, A)> {
    let mut out = Vec::new();
    for dg in crate::ppt::records(drawing).into_iter().filter(|r| r.ty == 0xF002) {
        for patriarch in dg.children().into_iter().filter(|r| r.ty == 0xF003) {
            // The patriarch's own shape comes first.
            for r in patriarch.children().into_iter().skip(1) {
                match r.ty {
                    0xF004 => {
                        if let Some(anchor) = anchor_of(&r) {
                            out.push((crate::ppt::read_shape(&r, false, None), anchor));
                        }
                    }
                    0xF003 => {
                        let anchor = r.children().into_iter().find(|c| c.ty == 0xF004).and_then(|c| anchor_of(&c));
                        if let (Some(anchor), Some(mut group)) = (anchor, crate::ppt::read_group(&r, false).into_iter().next()) {
                            unscale(&mut group);
                            out.push((group, anchor));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

/// A group's space and its shapes' places as the file gives them: `ppt::read_shape` takes them for a presentation's
/// master units and makes them EMU (× 1587.5), but a workbook's are in a space of their own (Excel writes EMU).
fn unscale(group: &mut ShapeIr) {
    let back = |v: i64| (v as f64 / 1587.5).round() as i64;
    let four = |(x, y, w, h): (i64, i64, i64, i64)| (back(x), back(y), back(w), back(h));
    group.child_space = group.child_space.map(four);
    for c in &mut group.children {
        c.frame = c.frame.map(four);
        if c.is_group {
            unscale(c);
        }
    }
}

/// What writing a sheet's shapes needs from the workbook.
pub(crate) struct ShapeWriter<'a> {
    pub defaults: &'a Props,
    /// The sRGB of a palette index (`Icv`); `None` for the automatic ones.
    pub icv: &'a dyn Fn(u16) -> Option<u32>,
    /// A run's `a:rPr` for a font index.
    pub run_props: &'a dyn Fn(u16) -> String,
    pub kinds: &'a std::collections::HashMap<u32, u16>,
    pub texts: &'a std::collections::HashMap<u32, ObjText>,
    pub blips: &'a [Option<Image>],
    /// The drawing's pictures as they are written: (relationship id, file name in `xl/media/`, bytes).
    pub media: Vec<(String, String, Vec<u8>)>,
    /// How many images the package has before this drawing's.
    pub media_base: usize,
    pub pictures: std::collections::HashMap<usize, String>,
}

impl crate::officeart_xml::ArtContext for ShapeWriter<'_> {
    fn defaults(&self) -> &Props {
        self.defaults
    }

    fn color(&self, v: u32) -> u32 {
        let [r, g, b, flags] = v.to_le_bytes();
        // An index (fSchemeIndex): in a workbook, a palette index (`Icv`); 0x40 and 0x41 are the system's window
        // text (black) and window (white) colours.
        if flags & 0x08 != 0 {
            return match r {
                0x41 => 0xFFFFFF,
                i => (self.icv)(u16::from(i)).unwrap_or(0),
            };
        }
        if flags & 0x10 != 0 {
            return 0;
        }
        u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
    }

    fn picture(&mut self, index: usize) -> Option<String> {
        if let Some(r) = self.pictures.get(&index) {
            return Some(r.clone());
        }
        let image = self.blips.get(index.checked_sub(1)?)?.as_ref()?;
        let rid = format!("rIdS{}", self.pictures.len() + 1);
        let name = format!("image{}.{}", self.media_base + self.media.len() + 1, image.ext);
        self.media.push((rid.clone(), name, image.data.clone()));
        self.pictures.insert(index, rid.clone());
        Some(rid)
    }
}

/// Whether an object of type `ot` (`ftCmo`) is drawn as a shape here: groups, lines, rectangles, ovals, arcs, text
/// boxes, polygons and the other autoshapes, and pictures in a group (a picture at the top is written apart).
pub(crate) fn drawn_kind(ot: u16, in_group: bool) -> bool {
    matches!(ot, 0x00 | 0x01 | 0x02 | 0x03 | 0x04 | 0x06 | 0x09 | 0x1E) || (in_group && ot == 0x08)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

impl ShapeWriter<'_> {
    /// A run's properties as the end of a paragraph's.
    fn end_props(&self, font: Option<u16>) -> String {
        font.map(|f| (self.run_props)(f).replacen("<a:rPr", "<a:endParaRPr", 1).replace("</a:rPr>", "</a:endParaRPr>")).unwrap_or_default()
    }

    /// The text body of a shape with its object's text.
    fn text_body(&self, shape: &ShapeIr, t: &ObjText) -> String {
        let p = &shape.props;
        let get = |id: u16| p.get(id).or_else(|| self.defaults.get(id));
        // The margins (EMU; MS-ODRAW's defaults 0.1 inch across, 0.05 inch down), the wrapping and the anchor.
        let ins = |id: u16, d: u32| get(id).unwrap_or(d);
        let wrap = if get(0x85) == Some(2) { "none" } else { "square" };
        let anchor = match t.v_align {
            2 => "ctr",
            3 => "b",
            4 => "just",
            7 => "dist",
            1 => "t",
            _ => match get(0x87).unwrap_or(0) {
                1 | 4 => "ctr",
                2 | 5 => "b",
                _ => "t",
            },
        };
        let vert = match t.rot {
            1 => " vert=\"wordArtVert\"",
            2 => " vert=\"vert270\"",
            3 => " vert=\"vert\"",
            _ => "",
        };
        let algn = match t.h_align {
            2 => "ctr",
            3 => "r",
            4 => "just",
            7 => "dist",
            _ => "l",
        };
        let chars: Vec<char> = t.text.chars().collect();
        let font_at = |i: usize| t.runs.iter().rev().find(|(at, _)| *at <= i).map(|r| r.1);
        let mut paras = String::new();
        let mut para = String::new();
        let mut run = String::new();
        let mut run_font: Option<Option<u16>> = None;
        let flush_run = |para: &mut String, run: &mut String, font: Option<Option<u16>>| {
            if !run.is_empty() {
                let rpr = font.flatten().map(|f| (self.run_props)(f)).unwrap_or_default();
                para.push_str(&format!("<a:r>{rpr}<a:t xml:space=\"preserve\">{}</a:t></a:r>", esc(run)));
                run.clear();
            }
        };
        for (i, &c) in chars.iter().enumerate() {
            if c == '\n' || c == '\r' {
                if c == '\r' && chars.get(i + 1) == Some(&'\n') {
                    continue;
                }
                flush_run(&mut para, &mut run, run_font);
                paras.push_str(&format!("<a:p><a:pPr algn=\"{algn}\"/>{para}{}</a:p>", self.end_props(font_at(i))));
                para.clear();
                run_font = None;
                continue;
            }
            let f = font_at(i);
            if run_font != Some(f) {
                flush_run(&mut para, &mut run, run_font);
                run_font = Some(f);
            }
            run.push(c);
        }
        flush_run(&mut para, &mut run, run_font);
        paras.push_str(&format!("<a:p><a:pPr algn=\"{algn}\"/>{para}{}</a:p>", self.end_props(font_at(chars.len().saturating_sub(1)))));
        format!(
            "<xdr:txBody><a:bodyPr vertOverflow=\"clip\" wrap=\"{wrap}\" lIns=\"{}\" tIns=\"{}\" rIns=\"{}\" bIns=\"{}\" anchor=\"{anchor}\"{vert}/><a:lstStyle/>{paras}</xdr:txBody>",
            ins(0x81, 91440),
            ins(0x82, 45720),
            ins(0x83, 91440),
            ins(0x84, 45720)
        )
    }

    /// A shape (`frame`: x, y, width, height in EMU in the space it is in) as `xdr` content; empty when it is not drawn.
    pub(crate) fn shape_xml(&mut self, shape: &ShapeIr, frame: (i64, i64, i64, i64), in_group: bool) -> String {
        use crate::officeart_xml::{fill_xml, geometry_xml, line_xml, shadow_xml};
        match self.kinds.get(&shape.id) {
            Some(&ot) if !drawn_kind(ot, in_group) => return String::new(),
            None if !shape.is_group => return String::new(),
            _ => {}
        }
        let id = shape.id;
        let name = if shape.name.is_empty() { format!("Shape {}", id & 0x3FF) } else { esc(&shape.name) };
        let (x, y, w, h) = frame;
        let rotation = shape.props.get(0x04).map_or(0, |v| ((f64::from(v as i32) / 65536.0) * 60000.0).round() as i64);
        let mut attrs = String::new();
        if rotation != 0 {
            attrs += &format!(" rot=\"{}\"", rotation.rem_euclid(21_600_000));
        }
        if shape.flip_h {
            attrs += " flipH=\"1\"";
        }
        if shape.flip_v {
            attrs += " flipV=\"1\"";
        }
        if shape.is_group {
            let (cx, cy, cw, ch) = shape.child_space.unwrap_or((x, y, w, h));
            let kids: String = shape.children.iter().map(|c| self.shape_xml(c, c.frame.unwrap_or((cx, cy, cw, ch)), true)).collect();
            if kids.is_empty() {
                return String::new();
            }
            return format!(
                "<xdr:grpSp><xdr:nvGrpSpPr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvGrpSpPr/></xdr:nvGrpSpPr><xdr:grpSpPr><a:xfrm{attrs}><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/><a:chOff x=\"{cx}\" y=\"{cy}\"/><a:chExt cx=\"{cw}\" cy=\"{ch}\"/></a:xfrm></xdr:grpSpPr>{kids}</xdr:grpSp>"
            );
        }
        let xfrm = format!("<a:xfrm{attrs}><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm>");
        if let Some(index) = shape.props.get(0x104).map(|v| v as usize).filter(|i| *i > 0)
            && let Some(rid) = crate::officeart_xml::ArtContext::picture(self, index)
        {
            let line = line_xml(self, &shape.props, false);
            return format!(
                "<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr>{xfrm}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{line}</xdr:spPr></xdr:pic>"
            );
        }
        let is_line = shape.spt == 20 || shape.is_connector || (32..=40).contains(&shape.spt);
        let geometry = geometry_xml(shape.spt, &shape.props);
        // A workbook's shapes are filled and outlined unless they say not (MS-ODRAW's defaults).
        let fill = if is_line { String::new() } else { fill_xml(self, &shape.props, false) };
        let line = line_xml(self, &shape.props, true);
        let shadow = shadow_xml(self, &shape.props);
        let sp_pr = format!("<xdr:spPr>{xfrm}{geometry}{fill}{line}{shadow}</xdr:spPr>");
        if is_line {
            return format!("<xdr:cxnSp macro=\"\"><xdr:nvCxnSpPr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvCxnSpPr/></xdr:nvCxnSpPr>{sp_pr}</xdr:cxnSp>");
        }
        let body = self.texts.get(&shape.id).filter(|t| !t.text.is_empty()).map(|t| self.text_body(shape, t)).unwrap_or_default();
        let tx_box = if shape.spt == 202 { " txBox=\"1\"" } else { "" };
        format!("<xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvSpPr{tx_box}/></xdr:nvSpPr>{sp_pr}{body}</xdr:sp>")
    }
}

#[cfg(test)]
mod tests {
    use super::{last_body, read_chart_shapes, rect_of};

    /// The piece of a drawing before a group's OBJ holds the drawing's own top group (FSPGR 0 0 0 0) and then the group's:
    /// the group's frame is the last (corpus 145's charts in a group).
    #[test]
    fn a_groups_frame_is_the_last_fspgr_of_its_piece() {
        let fspgr = |r: [i32; 4]| {
            let mut d = vec![0x01, 0x00, 0x09, 0xF0, 16, 0, 0, 0];
            for v in r {
                d.extend_from_slice(&v.to_le_bytes());
            }
            d
        };
        let container = |body: Vec<u8>| {
            let mut d = vec![0x0F, 0x00, 0x04, 0xF0];
            d.extend_from_slice(&(body.len() as u32).to_le_bytes());
            d.extend(body);
            d
        };
        let mut piece = container(fspgr([0, 0, 0, 0]));
        piece.extend(container(fspgr([10, 20, 1010, 520])));
        assert_eq!(last_body(&piece, 0xF009).and_then(rect_of), Some([10, 20, 1010, 520]));
        assert_eq!(last_body(&piece, 0xF00F), None);
    }

    /// A shape drawn on a chart is placed by four 4-byte edges in 1/4000 of the chart (corpus 145's text boxes), not by cells.
    #[test]
    fn a_chart_shape_is_placed_in_four_thousandths() {
        let record = |ver_inst: u16, ty: u16, body: Vec<u8>| {
            let mut d = ver_inst.to_le_bytes().to_vec();
            d.extend_from_slice(&ty.to_le_bytes());
            d.extend_from_slice(&(body.len() as u32).to_le_bytes());
            d.extend(body);
            d
        };
        let shape_atom = |id: u32| {
            let mut b = id.to_le_bytes().to_vec();
            b.extend_from_slice(&0u32.to_le_bytes());
            record(0x0002 | 202 << 4, 0xF00A, b)
        };
        let mut anchor = vec![0, 0];
        for v in [279i32, 558, 644, 1025] {
            anchor.extend_from_slice(&v.to_le_bytes());
        }
        // The patriarch's own shape, then a text box with its anchor.
        let patriarch = record(0x000F, 0xF004, shape_atom(0x2000));
        let text_box = record(0x000F, 0xF004, [shape_atom(0x2001), record(0x0000, 0xF010, anchor)].concat());
        let drawing = record(0x000F, 0xF002, record(0x000F, 0xF003, [patriarch, text_box].concat()));
        let shapes = read_chart_shapes(&drawing);
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].1, [279, 558, 644, 1025]);
    }
}
