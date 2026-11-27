// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use ibis_xdw_read::{Document, Error};

fn tlv(tag: u8, value: &[u8]) -> Vec<u8> {
    let mut bytes = vec![tag, u8::try_from(value.len()).unwrap()];
    bytes.extend(value);
    bytes
}

fn fixture(properties_tag: u8) -> Vec<u8> {
    // Raw LH5 of a synthetic property tree containing one A4 page and a
    // placement at (-2, 3) with a 90-degree, entry-backed scan.
    let properties = [
        0x00, 0x40, 0x4c, 0x6e, 0x92, 0x18, 0x0f, 0x6e, 0xf7, 0xe1, 0xf0, 0x86, 0x3e, 0x83, 0x11,
        0x54, 0x00, 0x88, 0x89, 0xb3, 0x92, 0x39, 0x01, 0x40, 0xbf, 0x41, 0x88, 0x6a, 0x22, 0x67,
        0x38, 0x00, 0x14, 0x95, 0x60, 0x32, 0x76, 0xf6, 0xcb, 0x18, 0x01, 0x1b, 0x3a, 0xce, 0x01,
        0x90, 0x1e, 0x60, 0x78, 0xa9, 0xea, 0x8b, 0x28, 0x0f, 0x30, 0x1b, 0xed, 0x41, 0xfc, 0x96,
        0xb0, 0x29, 0x3c, 0xe3, 0x4f, 0x9f, 0xb0, 0xd7, 0xdc, 0x8e, 0x40,
    ];
    let checksum = properties.chunks_exact(4).fold(0, |sum, word| {
        sum ^ u32::from_le_bytes(word.try_into().unwrap())
    });
    let header = tlv(0x60, &tlv(0x82, &[10]));
    let image_body = [tlv(0x80, &[5]), tlv(0x86, &[0xff, 0xd8, 0xff, 0xd9])].concat();
    let entry = tlv(0x64, &tlv(0x82, &image_body));
    let mut fields = [
        tlv(0x80, &[1]),
        tlv(0x81, &((header.len() + 2) as u32).to_le_bytes()),
        tlv(0x83, &[72]),
        tlv(0x84, &[properties.len() as u8]),
        tlv(0x85, &checksum.to_be_bytes()),
    ]
    .concat();
    fields.extend(tlv(0x86, &((fields.len() + 6) as u32).to_le_bytes()));
    let segment = [entry, tlv(properties_tag, &properties), tlv(0x68, &fields)].concat();
    [header, tlv(0x61, &segment)].concat()
}

#[test]
fn reads_signed_offsets_rotation_and_entry_zero() {
    let bytes = fixture(0x63);
    let document = Document::open(&bytes).unwrap();
    let pages = document.pages().unwrap();
    let [page] = pages.as_slice() else {
        panic!("expected one page")
    };
    assert_eq!((page.width, page.height), (21000, 29700));
    let [object] = page.objects.as_slice() else {
        panic!("expected one object")
    };
    assert_eq!(
        (object.kind, object.x, object.y, object.rotation),
        (0x8010, -2, 3, 90)
    );
    let drawing = document.object_drawing(object).unwrap().unwrap();
    assert_eq!(drawing.kind, 5);
    assert_eq!(drawing.bytes.as_ref(), [0xff, 0xd8, 0xff, 0xd9]);
}

#[test]
fn distinguishes_protection_and_rejects_bad_envelopes() {
    assert_eq!(Document::open(&fixture(0x66)).err(), Some(Error::Protected));
    let mut bytes = fixture(0x63);
    for length in 0..bytes.len() {
        assert!(Document::open(&bytes[..length]).is_err());
    }
    let properties_start = bytes.windows(2).position(|b| b == [0x63, 71]).unwrap() + 2;
    bytes[properties_start] ^= 1;
    assert_eq!(Document::open(&bytes).err(), Some(Error::Invalid));
}

#[test]
fn produces_scene_from_rotated_jpeg_document() {
    for (bytes, index) in [
        (include_bytes!("fixtures/rotated-jpeg.xdw").as_slice(), 0),
        (
            include_bytes!("fixtures/rotated-emf-scan.xdw").as_slice(),
            1,
        ),
    ] {
        // Synthetic landscape page containing a red/blue 32×48 JPEG rotated 90°.
        let document = Document::open(bytes).unwrap();
        let pages = document.pages().unwrap();
        let scene = document.page_scene(index, &pages[index]).unwrap();
        assert!(scene.info.crop_box.width() > scene.info.crop_box.height());
        let [
            ibis_pdf::DisplayCmd::DrawImage {
                width,
                height,
                rgba,
                ..
            },
        ] = scene.display_list.as_slice()
        else {
            panic!("expected one image");
        };
        assert_eq!((*width, *height), (48, 32));
        assert!(rgba[2] > 200 && rgba[47 * 4] > 200);
    }
}

#[test]
fn renders_synthetic_stamp_and_marker() {
    let document = Document::open(include_bytes!("fixtures/stamp-marker.xdw")).unwrap();
    let pages = document.pages().unwrap();
    let scene = document.page_scene(0, &pages[0]).unwrap();
    assert!(scene.display_list.iter().any(|command| matches!(
        command,
        ibis_pdf::DisplayCmd::DrawPath {
            fill_rgba: Some([255, 255, 0, 96]),
            ..
        }
    )));
    assert!(scene.display_list.iter().any(|command| matches!(
        command,
        ibis_pdf::DisplayCmd::DrawPath {
            stroke_rgba: Some([230, 0, 18, 255]),
            ..
        }
    )));
}
