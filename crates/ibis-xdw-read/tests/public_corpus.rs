// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use std::path::PathBuf;

use ibis_xdw_read::Document;

// XDW_CORPUS_DIR contains the public samples, with the filenames below: scripts/download-xdw-corpus.sh fetches them
// from their publishers (they may not be redistributed, so they are not in the repository; issue #254).
#[test]
#[ignore = "requires the public XDW samples in XDW_CORPUS_DIR"]
fn renders_public_corpus() {
    let root = PathBuf::from(std::env::var_os("XDW_CORPUS_DIR").expect("set XDW_CORPUS_DIR"));
    for (name, expected_pages) in [
        ("codelibs-test_test_xdw_ver8.xdw", 1),
        ("codelibs-test_test_xdw_ver9.xdw", 1),
        ("koto-ku_r604kinyumihon.xdw", 3),
        ("ohta-geo_rocfall.xdw", 2),
        ("okinawa-pref_63.xdw", 122),
        ("okinawa-pref_79.xdw", 2),
        ("saitama-city_taishouichiran_kyouryou.xdw", 10),
    ] {
        let bytes =
            std::fs::read(root.join(name)).unwrap_or_else(|error| panic!("{name}: {error}"));
        let doc = Document::open(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let pages = doc
            .pages()
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_eq!(pages.len(), expected_pages, "{name}");
        for (index, page) in pages.iter().enumerate() {
            doc.page_scene(index, page)
                .unwrap_or_else(|error| panic!("{name}, page {}: {error:?}", index + 1));
        }
    }
}
