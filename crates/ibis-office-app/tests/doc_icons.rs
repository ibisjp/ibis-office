// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0048: the icons of the files ibisOffice opens (`icons/doc/`, made by `scripts/gen-doc-icons.py`) are in the colour of the format's new-file
//! badge on the start screen (`new_badge_color`), and the three bundles name them.
use std::path::Path;

fn read(relative: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(relative)).unwrap_or_else(|e| panic!("{relative}: {e}"))
}

/// The number after `new_badge_color: 0x` in a format crate.
fn badge(crate_source: &str) -> String {
    let text = read(crate_source);
    let at = text.find("new_badge_color: 0x").expect("a badge colour") + "new_badge_color: 0x".len();
    text[at..].chars().take_while(|c| c.is_ascii_hexdigit()).collect::<String>().to_uppercase()
}

#[test]
fn the_icons_are_in_the_colours_of_the_start_screens_badges() {
    let generator = read("scripts/gen-doc-icons.py").to_uppercase();
    for (kind, source) in [("document", "crates/ibis-office-docx/src/lib.rs"), ("spreadsheet", "crates/ibis-office-xlsx/src/lib.rs"), ("presentation", "crates/ibis-office-pptx/src/lib.rs")] {
        let color = badge(source);
        assert!(generator.contains(&format!("\"{}\": (0X{color},", kind.to_uppercase())), "{kind}: the generator's colour is the badge's 0x{color}");
        for file in [format!("icons/doc/{kind}.ico"), format!("icons/doc/{kind}.icns"), format!("icons/doc/{kind}/256.png")] {
            assert!(Path::new(env!("CARGO_MANIFEST_DIR")).join(&file).is_file(), "{file} is there");
        }
    }
}

#[test]
fn the_bundles_use_the_icons() {
    let mac = read("scripts/bundle-macos.sh");
    for icon in ["DocIcon", "SheetIcon", "SlideIcon"] {
        assert!(mac.contains(&format!("<string>{icon}</string>")), "the Info.plist names {icon}");
        assert!(mac.contains(&format!("Resources/{icon}.icns")), "{icon}.icns is copied into the app");
    }
    let linux = read("scripts/bundle-linux.sh");
    assert!(linux.contains("hicolor/${size}x${size}/mimetypes"), "the AppImage has the icons of the MIME types");
    let windows = read("crates/ibis-office-app/src/open_with.rs");
    assert!(windows.contains("DefaultIcon") && windows.contains("ibisOffice.Spreadsheet"), "Windows registers an icon for each kind");
}
