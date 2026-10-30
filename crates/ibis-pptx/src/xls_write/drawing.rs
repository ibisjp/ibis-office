// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The drawing layer of a sheet of an old workbook (OfficeArt records in `MSODRAWING`, MS-ODRAWING and MS-XLS): the
//! notes of cells: each a text box shape (`OBJ`, `TXO` and the text) tied to its cell by a `NOTE` record. The
//! shapes of a sheet are one drawing: a group, and a shape for each note, numbered from 1024 times the drawing's number.

use ibis_doc_model::SheetComment;

use super::{Out, unicode_string};

/// A shape of a sheet's drawing.
pub(super) enum Shape<'a> {
    Note(&'a SheetComment),
    /// A picture: its number in the workbook's pictures (from 1), and the anchor words
    /// (flag, first column, its offset, first row, its offset, last column, offset, last row, offset).
    Picture { bid: u32, anchor: [u16; 9] },
}

/// A picture of the workbook: its kind (`6` PNG, `5` JPEG) and bytes.
pub(super) struct Blip {
    pub kind: u8,
    pub data: std::sync::Arc<[u8]>,
}

fn header(ver: u16, instance: u16, kind: u16, len: usize) -> [u8; 8] {
    let mut h = [0u8; 8];
    h[0..2].copy_from_slice(&(instance << 4 | ver).to_le_bytes());
    h[2..4].copy_from_slice(&kind.to_le_bytes());
    h[4..8].copy_from_slice(&(len as u32).to_le_bytes());
    h
}

/// The properties of a note's shape (21 of them: text wrapping and anchoring, no fill pattern, the shape hidden).
const NOTE_PROPERTIES: [u8; 126] = [
    0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x85, 0x00, 0x02, 0x00, 0x00, 0x00, 0x87, 0x00, 0x01, 0x00, 0x00, 0x00, 0xbf, 0x00, 0x08, 0x00, 0x08, 0x00, 0x58, 0x01, 0x00, 0x00, 0x00, 0x00, 0x80, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x81, 0x01, 0xff, 0xff, 0xff, 0x00, 0x83, 0x01, 0x00, 0x00, 0x00, 0x00, 0xbf, 0x01, 0x10, 0x00, 0x10, 0x00, 0xc0, 0x01, 0x00, 0x00, 0x00, 0x00, 0xc1, 0x01, 0x00, 0x00, 0x01, 0x00, 0xc2, 0x01, 0xff,
    0xff, 0xff, 0x00, 0xd0, 0x01, 0x01, 0x00, 0x00, 0x00, 0xd2, 0x01, 0x01, 0x00, 0x00, 0x00, 0xd3, 0x01, 0x01, 0x00, 0x00, 0x00, 0xd6, 0x01, 0x01, 0x00, 0x00, 0x00, 0xd7, 0x01, 0x02, 0x00, 0x00, 0x00, 0xff,
    0x01, 0x18, 0x00, 0x18, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x3f, 0x02, 0x03, 0x00, 0x03, 0x00, 0xbf, 0x03, 0x02, 0x00, 0x0a, 0x00,
];

/// The shape of a note up to its client data (a text box follows it): its container's header, the shape, its
/// properties, where it is drawn, the client data.
fn note_shape(spid: u32, note: &SheetComment) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(header(2, 0xCA, 0xF00A, 8));
    body.extend(spid.to_le_bytes());
    body.extend(0xA00u32.to_le_bytes());
    body.extend(header(3, 21, 0xF00B, NOTE_PROPERTIES.len()));
    body.extend(NOTE_PROPERTIES);
    // Where the box is when the note is shown: right of the cell, three columns wide and three rows high.
    body.extend(header(0, 0, 0xF010, 18));
    let (row, col) = (note.row.min(65535) as u16, note.column.min(255) as u16);
    for v in [2u16, col + 1, 15, row.saturating_sub(1), 2, col + 3, 15, row.saturating_add(3), 14] {
        body.extend(v.to_le_bytes());
    }
    body.extend(header(0, 0, 0xF011, 0));
    // The container holds the text box record that follows in the next record: counted in its length.
    let mut out = header(0xF, 0, 0xF004, body.len() + 8).to_vec();
    out.extend(body);
    out
}

/// The properties of a picture's shape: the picture's number, no outline.
fn picture_properties(bid: u32) -> Vec<u8> {
    let mut p = Vec::new();
    for (id, value) in [(0x007Fu16, 0x0080_0080u32), (0x0085, 2), (0x0087, 1), (0x4104, bid), (0x0180, 3), (0x01BF, 0x0010_0000), (0x01FF, 0x0009_0000)] {
        p.extend(id.to_le_bytes());
        p.extend(value.to_le_bytes());
    }
    p
}

fn picture_shape(spid: u32, bid: u32, anchor: &[u16; 9]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(header(2, 0x4B, 0xF00A, 8));
    body.extend(spid.to_le_bytes());
    body.extend(0xA00u32.to_le_bytes());
    let props = picture_properties(bid);
    body.extend(header(3, (props.len() / 6) as u16, 0xF00B, props.len()));
    body.extend(props);
    body.extend(header(0, 0, 0xF010, 18));
    for v in anchor {
        body.extend(v.to_le_bytes());
    }
    body.extend(header(0, 0, 0xF011, 0));
    let mut out = header(0xF, 0, 0xF004, body.len()).to_vec();
    out.extend(body);
    out
}

/// The whole drawing of a sheet with these shapes (written as records into `o`), numbered `drawing` from 1.
pub(super) fn write_shapes(o: &mut Out, shapes: &[Shape], drawing: u32) {
    let base = drawing * 1024;
    let n = shapes.len();
    let group = {
        let mut sp = header(1, 0, 0xF009, 16).to_vec();
        sp.extend([0u8; 16]);
        sp.extend(header(2, 0, 0xF00A, 8));
        sp.extend(base.to_le_bytes());
        sp.extend(5u32.to_le_bytes());
        let mut out = header(0xF, 0, 0xF004, sp.len()).to_vec();
        out.extend(sp);
        out
    };
    let shape_bytes: Vec<Vec<u8>> = shapes
        .iter()
        .enumerate()
        .map(|(k, s)| match s {
            Shape::Note(note) => note_shape(base + k as u32 + 1, note),
            Shape::Picture { bid, anchor } => picture_shape(base + k as u32 + 1, *bid, anchor),
        })
        .collect();
    // A note's container also holds a text box record, which comes in the record after the object's: counted.
    let container_len = |k: usize| shape_bytes[k].len() + if matches!(shapes[k], Shape::Note(_)) { 8 } else { 0 };
    let shapes_len = group.len() + (0..n).map(container_len).sum::<usize>();
    for (k, shape) in shapes.iter().enumerate() {
        let id = k as u16 + 1;
        let mut data = Vec::new();
        if k == 0 {
            data.extend(header(0xF, 0, 0xF002, 16 + 8 + shapes_len));
            data.extend(header(0, drawing as u16, 0xF008, 8));
            data.extend((n as u32 + 1).to_le_bytes());
            data.extend((base + n as u32).to_le_bytes());
            data.extend(header(0xF, 0, 0xF003, shapes_len));
            data.extend(&group);
        }
        let mut bytes = shape_bytes[k].clone();
        // A note's container says it includes the text box (8 bytes more than written here).
        if matches!(shape, Shape::Note(_)) {
            let len = (bytes.len() - 8 + 8) as u32;
            bytes[4..8].copy_from_slice(&len.to_le_bytes());
        }
        data.extend(bytes);
        o.rec(0xEC, &data);
        match shape {
            Shape::Picture { .. } => {
                // OBJ: a picture.
                let mut obj = vec![0x15, 0x00, 0x12, 0x00, 0x08, 0x00];
                obj.extend(id.to_le_bytes());
                obj.extend([0x11, 0x60]);
                obj.extend([0u8; 12]);
                obj.extend([0, 0, 0, 0]);
                o.rec(0x5D, &obj);
            }
            Shape::Note(note) => write_note_text(o, note, drawing, id),
        }
    }
}

fn write_note_text(o: &mut Out, note: &SheetComment, drawing: u32, id: u16) {
    // OBJ: a note (the common data, then the note's own with an identity).
    let mut obj = Vec::new();
    obj.extend([0x15, 0x00, 0x12, 0x00, 0x19, 0x00]);
    obj.extend(id.to_le_bytes());
    obj.extend([0u8; 14]);
    obj.extend([0x0D, 0x00, 0x16, 0x00]);
    let mut guid = [0u8; 16];
    let seed = (drawing as u64) << 32 | u64::from(id) << 16 | u64::from(note.row ^ note.column.rotate_left(8));
    for (i, b) in guid.iter_mut().enumerate() {
        *b = (seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(i as u32 * 7) >> 24) as u8 ^ (i as u8).wrapping_mul(31);
    }
    obj.extend(guid);
    obj.extend([0u8; 6]);
    obj.extend([0u8; 4]);
    o.rec(0x5D, &obj);
    o.rec(0xEC, &header(0, 0, 0xF00D, 0));
    // The text: TXO, then the characters and the runs (the first font).
    let chars: Vec<u16> = note.text.encode_utf16().take(4000).collect();
    let wide = chars.iter().any(|&c| c > 0xFF);
    let mut txo = vec![0x12, 0x00, 0, 0, 0, 0, 0, 0, 0, 0];
    txo.extend((chars.len() as u16).to_le_bytes());
    txo.extend(16u16.to_le_bytes());
    txo.extend([0, 0, 0, 0]);
    o.rec(0x1B6, &txo);
    let mut text = vec![u8::from(wide)];
    if wide {
        text.extend(chars.iter().flat_map(|c| c.to_le_bytes()));
    } else {
        text.extend(chars.iter().map(|&c| c as u8));
    }
    o.rec(0x3C, &text);
    let mut runs = Vec::new();
    for (ich, ifnt) in [(0u16, 5u16), (chars.len() as u16, 0)] {
        runs.extend(ich.to_le_bytes());
        runs.extend(ifnt.to_le_bytes());
        runs.extend([0, 0, 0, 0]);
    }
    o.rec(0x3C, &runs);
}

/// The notes' `NOTE` records (after the drawing).
pub(super) fn write_note_records(o: &mut Out, notes: &[&SheetComment]) {
    for (k, note) in notes.iter().enumerate() {
        let mut d = Vec::new();
        d.extend((note.row.min(65535) as u16).to_le_bytes());
        d.extend((note.column.min(255) as u16).to_le_bytes());
        d.extend(0u16.to_le_bytes());
        d.extend((k as u16 + 1).to_le_bytes());
        d.extend(unicode_string(&note.author));
        d.push(0);
        o.rec(0x1C, &d);
    }
}

/// The group of drawings in the workbook's globals (`MSODRAWINGGROUP`): `shapes_per_drawing` are the shapes' counts of
/// each drawing, in order; the pictures go in the store. It may be longer than a record: the bytes of the whole.
pub(super) fn group_record(shapes_per_drawing: &[usize], blips: &[Blip]) -> Vec<u8> {
    let shapes: usize = shapes_per_drawing.iter().map(|n| n + 1).sum();
    let clusters = shapes_per_drawing.len();
    let last = shapes_per_drawing.iter().enumerate().map(|(i, n)| (i as u32 + 1) * 1024 + *n as u32).max().unwrap_or(1024);
    let mut dgg = Vec::new();
    dgg.extend((last).to_le_bytes());
    dgg.extend((clusters as u32 + 1).to_le_bytes());
    dgg.extend((shapes as u32).to_le_bytes());
    dgg.extend((clusters as u32).to_le_bytes());
    for (i, n) in shapes_per_drawing.iter().enumerate() {
        dgg.extend((i as u32 + 1).to_le_bytes());
        dgg.extend((*n as u32 + 1).to_le_bytes());
    }
    let mut body = header(0, 0, 0xF006, dgg.len()).to_vec();
    body.extend(dgg);
    if !blips.is_empty() {
        let mut store = Vec::new();
        for (i, b) in blips.iter().enumerate() {
            // The picture's record: a unique identity, a marker byte, the file.
            let mut uid = [0u8; 16];
            let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ i as u64;
            for (k, byte) in b.data.iter().enumerate() {
                h = (h ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3);
                uid[k % 16] ^= (h >> 24) as u8;
            }
            let (kind, blip_type, blip_instance) = if b.kind == 6 { (6u8, 0xF01Eu16, 0x6E0u16) } else { (5u8, 0xF01D, 0x46A) };
            let mut blip = header(0, blip_instance, blip_type, 16 + 1 + b.data.len()).to_vec();
            blip.extend(uid);
            blip.push(0xFF);
            blip.extend(b.data.iter());
            let mut bse = vec![kind, kind];
            bse.extend(uid);
            bse.extend(0xFFu16.to_le_bytes());
            bse.extend((blip.len() as u32).to_le_bytes());
            bse.extend(1u32.to_le_bytes());
            bse.extend(0u32.to_le_bytes());
            bse.extend([0, 0, 0, 0]);
            bse.extend(&blip);
            store.extend(header(2, kind as u16, 0xF007, bse.len()));
            store.extend(bse);
        }
        body.extend(header(0xF, blips.len() as u16, 0xF001, store.len()));
        body.extend(store);
    }
    // Properties of the drawings (a few defaults) and the colours the menus split off.
    let opt: [u8; 18] = [0xbf, 0x00, 0x08, 0x00, 0x08, 0x00, 0x81, 0x01, 0x09, 0x00, 0x00, 0x08, 0xc0, 0x01, 0x40, 0x00, 0x00, 0x08];
    body.extend(header(3, 3, 0xF00B, opt.len()));
    body.extend(opt);
    let menu: [u8; 16] = [0x0d, 0x00, 0x00, 0x08, 0x0c, 0x00, 0x00, 0x08, 0x17, 0x00, 0x00, 0x08, 0xf7, 0x00, 0x00, 0x10];
    body.extend(header(0, 4, 0xF11E, menu.len()));
    body.extend(menu);
    let mut out = header(0xF, 0, 0xF000, body.len()).to_vec();
    out.extend(body);
    out
}

/// The position of an anchor of a shape on the sheet, in the words of the file: the cell and the offset in it
/// (in 1024ths of the column's width and 256ths of the row's height) for each of the two corners.
pub(super) fn anchor_words(frame: ibis_doc_model::EmuRect, sheet: &ibis_doc_model::Sheet) -> [u16; 9] {
    fn locate(sizes: &[i64], default: i64, at: i64, limit: u32) -> (u32, i64, i64) {
        let (mut index, mut start) = (0u32, 0i64);
        loop {
            let size = sizes.get(index as usize).copied().unwrap_or(default);
            if at < start + size {
                return (index, at - start, size);
            }
            start += size;
            index += 1;
            if index >= limit {
                return (limit - 1, 0, size.max(1));
            }
        }
    }
    let (c0, x0, w0) = locate(&sheet.columns, sheet.default_column, frame.x, 256);
    let (r0, y0, h0) = locate(&sheet.rows, sheet.default_row, frame.y, 65536);
    let (c1, x1, w1) = locate(&sheet.columns, sheet.default_column, frame.x + frame.width, 256);
    let (r1, y1, h1) = locate(&sheet.rows, sheet.default_row, frame.y + frame.height, 65536);
    let dx = |off: i64, w: i64| (off * 1024 / w.max(1)).clamp(0, 1023) as u16;
    let dy = |off: i64, h: i64| (off * 256 / h.max(1)).clamp(0, 255) as u16;
    [0, c0 as u16, dx(x0, w0), r0 as u16, dy(y0, h0), c1 as u16, dx(x1, w1), r1 as u16, dy(y1, h1)]
}
