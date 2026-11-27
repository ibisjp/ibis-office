// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Exercise the actual native open path with a synthetic JTD, without writing its source.
use super::Scenario;
use crate::docx::scenarios::DocSteps;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::native_jtd_open_read_only", open_read_only), ("shell::native_jtd_html_export", html_export), ("shell::native_jtd_omissions", omissions), ("shell::native_jtd_template", template), ("shell::native_jtd_compressed_template", compressed_template), ("shell::native_jtd_unplaced_images", unplaced_images), ("shell::native_jtd_image_switch", image_switch)]
}

fn synthetic_thumbnail() -> Vec<u8> {
    let (width, height) = (32u32, 48u32);
    let mut raw = Vec::new();
    for y in 0..height {
        for x in 0..width {
            raw.extend_from_slice(if x < 3 || x >= width - 3 || y < 3 || y >= height - 3 { &[160, 70, 20] } else if y < 20 { &[80, 190, 245] } else { &[240, 240, 240] });
        }
    }
    let mut encoded = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let start = i; i += 1;
        while i < raw.len() && raw[i] == raw[start] && i - start < 255 { i += 1; }
        let n = i - start;
        if n == 1 { encoded.push(raw[start]); } else { encoded.extend_from_slice(&[raw[start], raw[start], n as u8]); }
    }
    let mut bytes = vec![0u8; 56];
    bytes[0..4].copy_from_slice(&(40 + raw.len() as u32).to_le_bytes());
    bytes[4..8].copy_from_slice(&(40 + encoded.len() as u32).to_le_bytes());
    bytes[16..20].copy_from_slice(&40u32.to_le_bytes());
    bytes[20..24].copy_from_slice(&width.to_le_bytes());
    bytes[24..28].copy_from_slice(&height.to_le_bytes());
    bytes[28..30].copy_from_slice(&1u16.to_le_bytes());
    bytes[30..32].copy_from_slice(&24u16.to_le_bytes());
    bytes.extend(encoded);
    bytes
}

fn image_switch() {
    use ibis_formats::ibis_ooxml::cfb::{self, Item};
    let original = include_bytes!("../../../../../test/fixtures/jtd/synthetic.jtd");
    let text = cfb::stream_path(original, "DocumentText").unwrap();
    let wmf = &include_bytes!("../../../../ibis-emf/tests/fixtures/shapes.wmf")[22..];
    let mut press = vec![0u8; 48];
    press[..8].copy_from_slice(b"METAFILE");
    press.extend_from_slice(wmf);
    let bytes = cfb::write(&[
        Item::stream("DocumentText", text),
        Item::storage("EmbedItems"), Item::storage("EmbedItems/Embedding 7"),
        Item::stream("EmbedItems/Embedding 7/\u{3}EmbeddedPress", press.clone()),
        Item::storage("EmbedItems/Embedding 2"),
        Item::stream("EmbedItems/Embedding 2/\u{3}EmbeddedPress", press),
        Item::storage("EmbedItems/Embedding 9"),
        Item::stream("EmbedItems/Embedding 9/\u{3}EmbeddedPress", b"unsupported".to_vec()),
        Item::stream("Thumbnail", synthetic_thumbnail()),
    ]);
    let mut s = Scenario::start_view();
    let path = s.screenshot_path("jtd-mixed-source").with_extension("jtd");
    std::fs::write(&path, &bytes).unwrap();
    s.update(|e, w, cx| e.open_path(path.clone(), None, w, cx));
    for _ in 0..100 {
        s.wait(50);
        if s.with(|e, _| e.is_jtd_read_only()) { break; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(s.with(|e, _| e.is_jtd_read_only() && !e.jtd_gallery));
    assert!(s.with(|e, _| e.write_pdf(path.clone())).is_err());
    assert!(s.with(|e, _| e.write_png(path.clone(), None)).is_err());
    assert!(s.with(|e, _| e.write_gif(path.clone(), None)).is_err());
    assert!(s.paragraph(0).1.contains("日本語 test"));
    let body_before = s.with(|e, _| e.presentation().flow.as_ref().unwrap().body.clone());
    s.with(|e, _| {
        let menu = crate::shell::chrome_defaults::view_menu(e);
        assert!(crate::shell::scenario::find_item(&menu, "view.show-jtd-images").is_some());
        assert!(crate::shell::scenario::find_item(&menu, "view.show-jtd-thumbnail").is_some());
        assert!(crate::shell::scenario::find_item(&menu, "view.show-jtd-body").is_none());
    });
    // A later open request invalidates the pending image-list result even if
    // that later file fails and the existing body remains visible.
    let invalid = path.with_file_name("invalid-switch.jtd");
    std::fs::write(&invalid, b"not a JTD").unwrap();
    s.update(|e, w, cx| {
        e.show_jtd_images(w, cx);
        e.open_path(invalid.clone(), None, w, cx);
    });
    s.wait(1000);
    assert!(s.with(|e, _| e.is_jtd_read_only() && !e.jtd_gallery));
    assert_eq!(s.with(|e, _| e.path.clone()), Some(path.clone()));
    s.update(|e, w, cx| {
        e.show_jtd_thumbnail(w, cx);
        e.open_path(invalid.clone(), None, w, cx);
    });
    s.wait(1000);
    assert!(s.with(|e, _| e.is_jtd_read_only() && !e.jtd_thumbnail && !e.jtd_gallery));
    assert_eq!(s.with(|e, _| e.path.clone()), Some(path.clone()));
    let old_stamp = s.with(|e, _| e.watch.known.clone());
    // Thumbnail from the body remains read-only, cannot be exported, and returns to the same body.
    s.update(|e, w, cx| e.show_jtd_thumbnail(w, cx));
    for _ in 0..200 { s.wait(50); if s.with(|e, _| e.jtd_thumbnail) { break; } std::thread::sleep(std::time::Duration::from_millis(10)); }
    s.with(|e, _| {
        assert!(e.jtd_thumbnail && e.jtd_gallery && e.is_read_only());
        assert_eq!(e.presentation().media.len(), 1);
        assert_eq!(e.path.as_deref(), Some(path.as_path()));
        assert_eq!(e.watch.known, old_stamp);
        assert!(e.current_bytes().is_err());
        assert!(e.export_jtd_docx_to(&path.with_extension("docx")).is_err());
        let menu = crate::shell::chrome_defaults::view_menu(e);
        assert!(crate::shell::scenario::find_item(&menu, "view.hide-jtd-thumbnail").is_some());
        assert!(crate::shell::scenario::find_item(&menu, "view.show-jtd-images").is_none());
    });
    s.update(|e, w, cx| e.edit_jtd_as_docx(w, cx));
    assert!(s.with(|e, _| e.jtd_thumbnail && e.is_jtd_read_only()));
    s.wait(1000);
    s.screenshot("jtd-thumbnail");
    s.update(|e, w, cx| e.return_from_jtd_thumbnail(w, cx));
    assert!(s.with(|e, _| !e.jtd_thumbnail && !e.jtd_gallery));
    assert_eq!(s.with(|e, _| e.presentation().flow.as_ref().unwrap().body.clone()), body_before);
    s.update(|e, w, cx| e.show_jtd_images(w, cx));
    for _ in 0..200 {
        s.wait(50);
        if s.with(|e, _| e.jtd_gallery) { break; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    s.with(|e, _| {
        assert!(e.jtd_gallery && e.jtd_gallery_body.is_some() && e.is_read_only());
        assert_eq!(e.presentation().media.len(), 2);
        let texts: Vec<String> = e.presentation().flow.as_ref().unwrap().body.iter().filter_map(|block| {
            if let ibis_doc_model::Block::Paragraph(paragraph) = block { Some(paragraph.text()) } else { None }
        }).collect();
        let first = crate::shell::i18n::format("view.jtd-gallery-item", &[("index", &2)]);
        let second = crate::shell::i18n::format("view.jtd-gallery-item", &[("index", &7)]);
        assert!(texts.iter().position(|text| text == &first).unwrap() < texts.iter().position(|text| text == &second).unwrap());
        assert!(texts.contains(&crate::shell::i18n::format("view.jtd-gallery-count", &[("displayed", &2), ("candidates", &3)])));
        assert_eq!(e.path.as_deref(), Some(path.as_path()));
        assert_eq!(e.watch.known, old_stamp);
        assert!(e.current_bytes().is_err());
        assert!(e.export_jtd_docx_to(&path.with_extension("docx")).is_err());
        let menu = crate::shell::chrome_defaults::view_menu(e);
        assert!(crate::shell::scenario::find_item(&menu, "view.show-jtd-body").is_some());
    });
    s.wait(1000);
    s.screenshot("jtd-mixed-unplaced");
    let gallery_before = s.with(|e, _| e.presentation().flow.as_ref().unwrap().body.clone());
    s.update(|e, w, cx| e.show_jtd_thumbnail(w, cx));
    for _ in 0..200 { s.wait(50); if s.with(|e, _| e.jtd_thumbnail) { break; } std::thread::sleep(std::time::Duration::from_millis(10)); }
    assert!(s.with(|e, _| e.jtd_thumbnail && e.jtd_gallery));
    s.update(|e, w, cx| e.return_from_jtd_thumbnail(w, cx));
    assert!(s.with(|e, _| !e.jtd_thumbnail && e.jtd_gallery && e.jtd_gallery_body.is_some()));
    assert_eq!(s.with(|e, _| e.presentation().flow.as_ref().unwrap().body.clone()), gallery_before);
    s.update(|e, w, cx| e.show_jtd_body(w, cx));
    assert!(s.with(|e, _| e.is_jtd_read_only() && !e.jtd_gallery && e.jtd_gallery_body.is_none()));
    assert!(s.paragraph(0).1.contains("日本語 test"));
    assert_eq!(s.with(|e, _| e.presentation().flow.as_ref().unwrap().body.clone()), body_before);
    assert_eq!(s.with(|e, _| e.path.clone()), Some(path.clone()));
    s.update(|e, w, cx| e.edit_jtd_as_docx(w, cx));
    assert!(!s.with(|e, _| e.is_jtd_read_only()));
    let output = path.with_extension("docx");
    s.update(|e, w, _| e.save_to(output.clone(), w)).unwrap();
    let saved = ibis_formats::read(&std::fs::read(output).unwrap()).unwrap();
    assert!(saved.flow.unwrap().body.iter().any(|b| matches!(b, ibis_doc_model::Block::Paragraph(p) if p.text().contains("日本語 test"))));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

fn html_export() {
    let mut s = Scenario::start_view();
    let bytes = include_bytes!("../../../../../test/fixtures/jtd/synthetic.jtd");
    let compressed = include_bytes!("../../../../../test/fixtures/jtd/synthetic.jttc");
    for (extension, source) in [("JTD", bytes.as_slice()), ("JTT", bytes.as_slice()), ("JTTC", compressed.as_slice()), ("html", bytes.as_slice())] {
        let path = s.screenshot_path("synthetic-html-export").with_extension(extension);
        let output = path.with_file_name(format!("export-{extension}.HTML"));
        assert!(s.with(|e, _| e.export_jtd_html_to(&output)).is_err());
        std::fs::write(&path, source).unwrap();
        s.update(|e, w, cx| e.open_path(path.clone(), None, w, cx));
        for _ in 0..100 {
            s.wait(50);
            if s.with(|e, _| e.is_jtd_read_only()) { break; }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(s.with(|e, _| e.is_jtd_read_only()));
        s.with(|e, _| {
            let mut menus = e.menus();
            let item = menus.find_mut("file.export.html").expect("HTML export menu");
            assert_eq!(item.label.as_str(), crate::shell::i18n::text("file.export.html").as_ref());
            assert!(item.handler.is_some());
            assert!(e.export_jtd_html_to(&path).is_err());
            for ext in ["jtd", "JTT", "jttc", "JTDC", "docx", "txt"] {
                let forbidden = path.with_file_name(format!("forbidden.{ext}"));
                std::fs::write(&forbidden, b"keep").unwrap();
                assert!(e.export_jtd_html_to(&forbidden).is_err());
                assert_eq!(std::fs::read(forbidden).unwrap(), b"keep");
            }
        });
        #[cfg(unix)]
        {
            let alias = path.with_file_name("source-alias.html");
            let _ = std::fs::remove_file(&alias);
            std::os::unix::fs::symlink(&path, &alias).unwrap();
            assert!(s.with(|e, _| e.export_jtd_html_to(&alias)).is_err());
        }
        s.with(|e, _| e.export_jtd_html_to(&output)).unwrap();
        let html = std::fs::read_to_string(&output).unwrap();
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("日本語 test"));
        assert_eq!(std::fs::read(&path).unwrap(), source);
        assert!(s.with(|e, _| e.is_jtd_read_only() && e.path.as_ref() == Some(&path)));
        s.update(|e, w, cx| e.edit_jtd_as_docx(w, cx));
        assert!(s.with(|e, _| e.export_jtd_html_to(&output)).is_err());
        assert_eq!(std::fs::read_to_string(output).unwrap(), html);
    }
}

fn unplaced_images() {
    use ibis_formats::ibis_ooxml::cfb::{self, Item};
    assert_eq!(crate::jtd_preview::inspection_size(u32::MAX, 1), ibis_doc_model::EmuSize::new(5 * 914_400, 1));
    // A one-unit, whitespace-only TextV.01 in an SsmgV.01 store.
    let mut text = b"TextV.01".to_vec();
    text.extend_from_slice(&1u32.to_be_bytes());
    text.extend_from_slice(&10u16.to_be_bytes());
    text.push(0);
    text.extend_from_slice(&1u32.to_be_bytes());
    let len = text.len() as u32;
    text.resize(256, 0);
    let mut body = b"SsmgV.01".to_vec();
    for value in [1u32, 256, 1] { body.extend_from_slice(&value.to_be_bytes()); }
    body.extend(text);
    for value in [0u32, 0, len, 1, 1, 0, 0] { body.extend_from_slice(&value.to_be_bytes()); }
    let wmf = &include_bytes!("../../../../ibis-emf/tests/fixtures/shapes.wmf")[22..];
    let mut press = vec![0u8; 48];
    press[..8].copy_from_slice(b"METAFILE");
    press.extend_from_slice(wmf);
    let bytes = cfb::write(&[
        Item::stream("DocumentText", body),
        Item::storage("EmbedItems"), Item::storage("EmbedItems/Embedding 7"),
        Item::stream("EmbedItems/Embedding 7/\u{3}EmbeddedPress", press),
    ]);
    let mut s = Scenario::start_view();
    let path = s.screenshot_path("jtd-unplaced-source").with_extension("jtd");
    std::fs::write(&path, &bytes).unwrap();
    let loaded = crate::read_file(&path).unwrap();
    assert!(loaded.jtd_gallery);
    assert_eq!(loaded.presentation.media.len(), 1);
    s.update(|e, w, cx| e.open_path(path.clone(), None, w, cx));
    for _ in 0..100 {
        s.wait(50);
        if s.with(|e, _| e.is_jtd_read_only()) { break; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    s.with(|e, _| {
        assert!(e.is_jtd_read_only() && e.jtd_gallery);
        assert!(e.status.as_deref().unwrap().contains(&*crate::shell::i18n::text("view.jtd-gallery-notice")));
        assert!(e.current_bytes().is_err());
        assert!(e.export_jtd_docx_to(&path.with_extension("docx")).is_err());
        let menu = crate::shell::chrome_defaults::view_menu(e);
        assert!(crate::shell::scenario::find_item(&menu, "view.show-jtd-body").is_none());
    });
    s.update(|e, w, cx| e.edit_jtd_as_docx(w, cx));
    assert!(s.with(|e, _| e.is_jtd_read_only() && e.jtd_gallery));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    s.screenshot("jtd-unplaced-images");
    // Optional, locally owned sample; never part of a distributable fixture.
    if let Some(sample) = std::env::var_os("IBIS_JTD_GALLERY_SAMPLE").map(std::path::PathBuf::from) {
        let sample_original = std::fs::read(&sample).unwrap();
        let loaded = crate::read_file(&sample).unwrap();
        assert!(loaded.jtd_gallery && !loaded.presentation.media.is_empty());
        if let Ok(expected) = std::env::var("IBIS_JTD_GALLERY_EXPECT") {
            assert_eq!(loaded.presentation.media.len(), expected.parse::<usize>().unwrap());
        }
        s.update(|e, w, cx| e.open_path(sample.clone(), None, w, cx));
        for _ in 0..100 {
            s.wait(50);
            if s.with(|e, _| e.path.as_deref() == Some(sample.as_path()) && e.jtd_gallery) { break; }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(s.with(|e, _| e.path.as_deref() == Some(sample.as_path()) && e.jtd_gallery));
        s.wait(1000);
        s.screenshot("jtd-local-unplaced-images");
        s.update(|e, w, cx| e.show_jtd_thumbnail(w, cx));
        for _ in 0..200 { s.wait(50); if s.with(|e, _| e.jtd_thumbnail) { break; } std::thread::sleep(std::time::Duration::from_millis(10)); }
        assert!(s.with(|e, _| e.jtd_thumbnail && e.is_read_only() && e.path.as_deref() == Some(sample.as_path())));
        s.wait(1000);
        s.screenshot("jtd-local-saved-thumbnail");
        s.update(|e, w, cx| e.return_from_jtd_thumbnail(w, cx));
        assert!(s.with(|e, _| e.jtd_gallery && !e.jtd_thumbnail && e.path.as_deref() == Some(sample.as_path())));
        assert_eq!(std::fs::read(&sample).unwrap(), sample_original);
    }
}

fn compressed_template() {
    let mut s = Scenario::start_view();
    let path = s.screenshot_path("synthetic-compressed-template").with_extension("JTTC");
    let bytes = include_bytes!("../../../../../test/fixtures/jtd/synthetic.jttc");
    std::fs::write(&path, bytes).unwrap();
    assert_eq!(ibis_formats::Kind::from_extension("JTTC"), Some(ibis_formats::Kind::Document));
    assert!(ibis_formats::Kind::OPENABLE_EXTENSIONS.contains(&"jttc"));
    s.update(|e, w, cx| e.open_path(path.clone(), None, w, cx));
    for _ in 0..100 {
        s.wait(50);
        if s.with(|e, _| e.is_jtd_read_only()) { break; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(s.with(|e, _| e.is_jtd_read_only()));
    assert!(s.paragraph(0).1.contains("日本語 test"));
    s.update(|e, w, cx| e.edit_jtd_as_docx(w, cx));
    assert!(!s.with(|e, _| e.is_jtd_read_only()));
    assert!(s.update(|e, w, _| e.save_to(path.clone(), w)).is_err());
    assert!(s.update(|e, w, _| e.save_to(path.with_file_name("other.jttc"), w)).is_err());
    assert!(s.update(|e, w, _| e.save_to(path.with_file_name("other.jtdc"), w)).is_err());
    let docx = path.with_extension("docx");
    s.update(|e, w, _| e.save_to(docx.clone(), w)).unwrap();
    assert!(ibis_formats::read(&std::fs::read(docx).unwrap()).is_ok());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let renamed = path.with_file_name("renamed-compressed.docx");
    std::fs::write(&renamed, bytes).unwrap();
    assert!(crate::read_file(&renamed).unwrap().jtd_original.is_some());
    let invalid = path.with_file_name("invalid.jttc");
    std::fs::write(&invalid, b"not a JTTC").unwrap();
    assert!(crate::read_file(&invalid).is_err());
}

fn template() {
    let mut s = Scenario::start_view();
    let path = s.screenshot_path("synthetic-template").with_extension("JTT");
    let bytes = include_bytes!("../../../../../test/fixtures/jtd/synthetic.jtd");
    std::fs::write(&path, bytes).unwrap();
    assert_eq!(ibis_formats::Kind::from_extension("JTT"), Some(ibis_formats::Kind::Document));
    assert!(ibis_formats::Kind::OPENABLE_EXTENSIONS.contains(&"jtt"));
    s.update(|e, w, cx| e.open_path(path.clone(), None, w, cx));
    for _ in 0..100 {
        s.wait(50);
        if s.with(|e, _| e.is_jtd_read_only()) { break; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(s.with(|e, _| e.is_jtd_read_only()));
    assert!(s.paragraph(0).1.contains("日本語 test"));
    s.update(|e, w, cx| e.edit_jtd_as_docx(w, cx));
    assert!(!s.with(|e, _| e.is_jtd_read_only()));
    assert!(s.update(|e, w, _| e.save_to(path.clone(), w)).is_err());
    assert!(s.update(|e, w, _| e.save_to(path.with_file_name("other.jtt"), w)).is_err());
    let docx = path.with_extension("docx");
    s.update(|e, w, _| e.save_to(docx.clone(), w)).unwrap();
    assert!(ibis_formats::read(&std::fs::read(docx).unwrap()).is_ok());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let invalid = path.with_file_name("invalid.jtt");
    std::fs::write(&invalid, b"not a JTT").unwrap();
    assert!(crate::read_file(&invalid).is_err());
}

fn omissions() {
    use ibis_formats::ibis_ooxml::cfb::{self, Item};
    let original = include_bytes!("../../../../../test/fixtures/jtd/synthetic.jtd");
    let text = cfb::stream_path(original, "DocumentText").unwrap();
    let bytes = cfb::write(&[
        Item::storage("ObjectSheets"),
        Item::storage("ObjectSheets/DocSheet"),
        Item::storage("ObjectSheets/DocSheet/DOCS_0001"),
        Item::stream("ObjectSheets/DocSheet/DOCS_0001/DocumentText", text.clone()),
        Item::storage("Embedding 1"),
        Item::stream("DocumentText", text),
    ]);
    let mut s = Scenario::start_view();
    let path = s.screenshot_path("jtd-omissions").with_extension("jtd");
    std::fs::write(&path, bytes).unwrap();
    s.update(|e, w, cx| e.open_path(path, None, w, cx));
    for _ in 0..100 {
        s.wait(50);
        if s.with(|e, _| e.is_jtd_read_only()) { break; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    s.with(|e, _| {
        let status = e.status.as_deref().unwrap();
        assert!(status.contains(&crate::shell::i18n::format("view.jtd-omitted-sheets", &[("count", &1)])));
        assert!(status.contains(&crate::shell::i18n::format("view.jtd-omitted-objects", &[("count", &1)])));
    });
    s.update(|e, w, cx| e.edit_jtd_as_docx(w, cx));
    s.with(|e, _| {
        assert!(!e.is_jtd_read_only());
        let status = e.status.as_deref().unwrap();
        assert!(status.contains(&crate::shell::i18n::format("view.jtd-omitted-sheets", &[("count", &1)])));
        assert!(status.contains(&crate::shell::i18n::format("view.jtd-omitted-objects", &[("count", &1)])));
    });
}

fn open_read_only() {
    let mut s = Scenario::start_view();
    let path = s.screenshot_path("synthetic").with_extension("JTD");
    let bytes = include_bytes!("../../../../../test/fixtures/jtd/synthetic.jtd");
    std::fs::write(&path, bytes).unwrap();
    assert_eq!(ibis_formats::Kind::from_extension("JTD"), Some(ibis_formats::Kind::Document));
    assert!(ibis_formats::Kind::OPENABLE_EXTENSIONS.contains(&"jtd"));
    s.update(|e, w, cx| e.open_path(path.clone(), None, w, cx));
    for _ in 0..100 {
        s.wait(50);
        if s.with(|e, _| e.is_jtd_read_only()) { break; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(s.with(|e, _| e.is_read_only() && e.is_jtd_read_only()));
    assert!(s.paragraph(0).1.contains("日本語 test"));
    s.update(|e, w, cx| e.show_jtd_thumbnail(w, cx));
    for _ in 0..100 {
        s.wait(50);
        if s.with(|e, _| e.status.as_deref() == Some(&*crate::shell::i18n::text("view.jtd-thumbnail-unavailable"))) { break; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(s.with(|e, _| e.is_jtd_read_only() && !e.jtd_gallery && !e.jtd_thumbnail && e.status.as_deref() == Some(&*crate::shell::i18n::text("view.jtd-thumbnail-unavailable"))));
    // A document without supported images remains on its body preview.
    s.update(|e, w, cx| e.show_jtd_images(w, cx));
    for _ in 0..100 {
        s.wait(50);
        if s.with(|e, _| e.status.as_deref() == Some(&*crate::shell::i18n::text("view.jtd-no-embedded-images"))) { break; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(s.with(|e, _| e.is_jtd_read_only() && !e.jtd_gallery && e.status.as_deref() == Some(&*crate::shell::i18n::text("view.jtd-no-embedded-images"))));
    let (paragraph, _) = s.paragraph(0);
    s.click_text(ibis_doc_model::FlowPos::new(paragraph, 0));
    s.type_text("forbidden");
    assert!(!s.paragraph(0).1.contains("forbidden"));
    s.update(|e, w, cx| e.save(&crate::Save, w, cx));
    s.update(|e, w, cx| e.save_as(&crate::SaveAs, w, cx));
    assert!(s.update(|e, w, _| e.save_to(path.clone(), w)).is_err());
    assert!(s.with(|e, _| e.copy_to(&path)).is_err());
    assert!(s.with(|e, _| e.write_pdf(path.clone())).is_err());
    assert!(s.with(|e, _| e.write_png(path.clone(), None)).is_err());
    let output = path.with_extension("docx");
    assert!(s.with(|e, _| e.export_jtd_docx_to(&path)).is_err());
    #[cfg(unix)]
    {
        let alias = path.with_file_name("alias.docx");
        let _ = std::fs::remove_file(&alias);
        std::os::unix::fs::symlink(&path, &alias).unwrap();
        assert!(s.with(|e, _| e.export_jtd_docx_to(&alias)).is_err());
        let hardlink = path.with_file_name("hardlink.docx");
        let _ = std::fs::remove_file(&hardlink);
        std::fs::hard_link(&path, &hardlink).unwrap();
        assert!(s.with(|e, _| e.export_jtd_docx_to(&hardlink)).is_err());
    }
    s.with(|e, _| e.export_jtd_docx_to(&output)).unwrap();
    let imported = ibis_formats::read(&std::fs::read(output).unwrap()).unwrap();
    assert!(imported.flow.unwrap().body.iter().any(|b| matches!(b, ibis_doc_model::Block::Paragraph(p) if p.text().contains("日本語 test"))));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);

    // Editing begins from a DOCX model, with no path to the original file.
    s.update(|e, w, cx| { e.read_only = true; e.edit_jtd_as_docx(w, cx); });
    assert!(s.with(|e, _| e.is_jtd_read_only()));
    s.update(|e, _, _| e.read_only = false);
    s.update(|e, w, cx| e.edit_jtd_as_docx(w, cx));
    assert!(s.with(|e, _| !e.is_read_only() && !e.is_jtd_read_only()));
    let (paragraph, _) = s.paragraph(0);
    s.click_text(ibis_doc_model::FlowPos::new(paragraph, 0));
    s.type_text("edited");
    assert!(s.paragraph(0).1.contains("edited"));
    assert!(s.update(|e, w, _| e.save_to(path.clone(), w)).is_err());
    assert!(s.with(|e, _| e.copy_to(&path)).is_err());
    assert!(s.with(|e, _| e.write_pdf(path.clone())).is_err());
    assert!(s.with(|e, _| e.write_png(path.clone(), None)).is_err());
    assert!(s.update(|e, w, _| e.save_to(path.with_file_name("other.jtd"), w)).is_err());
    #[cfg(unix)]
    {
        let alias = path.with_file_name("alias.docx");
        assert!(s.update(|e, w, _| e.save_to(alias, w)).is_err());
    }
    let edited_path = path.with_file_name("edited.docx");
    s.update(|e, w, _| e.save_to(edited_path.clone(), w)).unwrap();
    let edited = ibis_formats::read(&std::fs::read(edited_path).unwrap()).unwrap();
    assert!(edited.flow.unwrap().body.iter().any(|b| matches!(b, ibis_doc_model::Block::Paragraph(p) if p.text().contains("edited"))));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);

    // The signature, not just the extension, keeps a renamed JTD read-only.
    let renamed = path.with_extension("docx");
    std::fs::write(&renamed, bytes).unwrap();
    let loaded = crate::read_file(&renamed).unwrap();
    assert!(loaded.jtd_original.is_some());
    std::fs::write(&renamed, b"not a JTD").unwrap();
    let invalid = path.with_file_name("invalid.jtd");
    std::fs::write(&invalid, b"not a JTD").unwrap();
    assert!(crate::read_file(&invalid).is_err());
}
