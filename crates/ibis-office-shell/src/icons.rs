// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Icons: Google's Material Symbols (Apache-2.0), bundled as SVG files in
//! `icons/material/` (see its README) and served from memory.
//!
//! Use [`Material`] wherever an icon is taken (`Button::icon`, `Icon::new`):
//! `Material("format_bold")`, or `Material::fill("dock_to_left")` for the
//! filled variant (the files `<name>_fill.svg`). A name must have its file in
//! `icons/material/`; to add icons, see the README there.

use std::borrow::Cow;

use gpui_kit::assets::IconNamed;
use gpui_kit::*;

include!(concat!(env!("OUT_DIR"), "/material_icons.rs"));

const PREFIX: &str = "icons/material/";

/// The icon files the scenarios asked for that are not bundled (a path under `icons/material/` or `icons/ibis/` with no file here). In the Web version such a request goes to the
/// server and is a 404 (the Review menu's three icons, K0037); on the desktop it is a blank. The scenario run lists them at its end and fails (`scenario.rs`).
#[cfg(feature = "scenarios")]
static MISSING: std::sync::Mutex<std::collections::BTreeSet<String>> = std::sync::Mutex::new(std::collections::BTreeSet::new());

/// The paths asked for that no bundled file answers (sorted).
#[cfg(feature = "scenarios")]
pub fn missing_icon_requests() -> Vec<String> {
    MISSING.lock().map(|m| m.iter().cloned().collect()).unwrap_or_default()
}

/// A bundled Material Symbols icon, by name (such as `format_bold`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Material(pub &'static str);

impl Material {
    /// The filled variant (`<name>_fill.svg`).
    pub(crate) fn fill(name: &'static str) -> MaterialFill {
        MaterialFill(name)
    }
}

impl IconNamed for Material {
    fn path(self) -> SharedString {
        // `ibis/<name>`: an icon drawn for ibisOffice (icons/ibis/).
        match self.0.strip_prefix("ibis/") {
            Some(own) => format!("icons/ibis/{own}.svg").into(),
            None => format!("{PREFIX}{}.svg", self.0).into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MaterialFill(pub &'static str);

impl IconNamed for MaterialFill {
    fn path(self) -> SharedString {
        format!("{PREFIX}{}_fill.svg", self.0).into()
    }
}

/// The app's asset source: the bundled icons, then GPUI Kit's assets.
pub struct Assets {
    kit: gpui_kit::assets::AllAssets,
}

impl Assets {
    /// `endpoint` is where GPUI Kit's own assets are fetched from in the
    /// browser (see `gpui_kit::assets::AllAssets::new`).
    pub fn new(endpoint: impl Into<SharedString>) -> Self {
        Self { kit: gpui_kit::assets::AllAssets::new(endpoint) }
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let own = path.strip_prefix("icons/ibis/").map(|n| format!("ibis/{n}"));
        if let Some(name) = path.strip_prefix(PREFIX).or(own.as_deref()).and_then(|p| p.strip_suffix(".svg")) {
            if let Some((_, data)) = MATERIAL_ICONS.iter().find(|(n, _)| *n == name) {
                return Ok(Some(Cow::Borrowed(data)));
            }
            #[cfg(feature = "scenarios")]
            if let Ok(mut missing) = MISSING.lock() {
                missing.insert(path.to_string());
            }
        }
        // The browser fetches GPUI Kit's icons on first use and says "loading" as
        // an error until they are there; GPUI logs that as an ERROR in the
        // console. "Nothing yet" (`None`) is drawn nothing and asked again on the
        // next frame, without the log.
        match self.kit.load(path) {
            Err(err) if err.to_string().starts_with("Wasm assets loading") => Ok(None),
            other => other,
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut out = self.kit.list(path)?;
        out.extend(MATERIAL_ICONS.iter().filter(|(n, _)| !n.starts_with("ibis/")).map(|(n, _)| SharedString::from(format!("{PREFIX}{n}.svg"))).filter(|p| p.starts_with(path)));
        out.extend(MATERIAL_ICONS.iter().filter_map(|(n, _)| n.strip_prefix("ibis/")).map(|n| SharedString::from(format!("icons/ibis/{n}.svg"))).filter(|p| p.starts_with(path)));
        Ok(out)
    }
}

/// Returns the raw SVG bytes for the bundled icon `name` (without `.svg`), if found.
pub fn icon_svg_bytes(name: &str) -> Option<&'static [u8]> {
    MATERIAL_ICONS.iter().find(|(n, _)| *n == name).map(|(_, data)| *data)
}

/// Whether icon `name` is bundled.
#[cfg(test)]
pub(crate) fn has(name: &str) -> bool {
    MATERIAL_ICONS.iter().any(|(n, _)| *n == name)
}


#[cfg(test)]
mod bundled_icons_tests {
    /// Every icon name the code gives a menu, a toolbar, a panel or a button (`Material("x")`, `item("id", "x")`, `Submenu::tr("id", "x", ..)`, `ToolButton::new("id", "x", ..)`, `.icon("x")`,
    /// `icon: "x"`) has its file in `icons/material/`. The Web version asks the server for a name that is not bundled, and that is a 404 (the Review menu's accessibility, record_voice_over and find_in_page, K0037);
    /// on the desktop it is a blank. The scenarios list what they drew that is missing as well (`scenario.rs`); this one reads the sources, so a menu no scenario opens is checked too.
    #[test]
    fn every_icon_name_in_the_sources_is_bundled() {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        // (what comes before, whether a first string (the id) comes before the name)
        let patterns: [(&str, bool); 8] = [
            ("Material(\"", false),
            (".icon(\"", false),
            ("icon: \"", false),
            ("item(\"", true),
            ("item_key(\"", true),
            ("sub(\"", true),
            ("Submenu::tr(\"", true),
            ("ToolButton::new(\"", true),
        ];
        let names_in = |text: &str| -> Vec<String> {
            let mut out = Vec::new();
            for (prefix, id_first) in patterns {
                let mut from = 0;
                while let Some(at) = text[from..].find(prefix) {
                    let mut rest = &text[from + at + prefix.len()..];
                    from += at + prefix.len();
                    if id_first {
                        // The id, then `, "name"`.
                        let Some(end) = rest.find('"') else { break };
                        rest = rest[end + 1..].trim_start_matches(|c: char| c == ',' || c.is_whitespace());
                        let Some(after) = rest.strip_prefix('"') else { continue };
                        rest = after;
                    }
                    let Some(end) = rest.find('"') else { break };
                    let name = &rest[..end];
                    if !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                        out.push(name.to_string());
                    }
                }
            }
            // The filled variant is the file `<name>_fill.svg`.
            let mut from = 0;
            while let Some(at) = text[from..].find("Material::fill(\"") {
                let rest = &text[from + at + "Material::fill(\"".len()..];
                from += at + "Material::fill(\"".len();
                if let Some(end) = rest.find('"') {
                    out.push(format!("{}_fill", &rest[..end]));
                }
            }
            out
        };
        let mut missing = std::collections::BTreeMap::<String, String>::new();
        let mut files = Vec::new();
        let mut stack: Vec<std::path::PathBuf> = std::fs::read_dir(&crates).unwrap().flatten().map(|e| e.path().join("src")).filter(|p| p.is_dir()).collect();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|n| n != "tests") {
                        stack.push(path);
                    }
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        assert!(files.len() > 100, "the sources of the crates were read: {}", files.len());
        for file in files {
            // (This file's own comments give examples.)
            if file.file_name().is_some_and(|n| n == "icons.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            for name in names_in(&text) {
                if !super::has(&name) {
                    missing.entry(name).or_insert_with(|| file.file_name().unwrap().to_string_lossy().into_owned());
                }
            }
        }
        assert!(missing.is_empty(), "icon names with no file in icons/material/ (name: first file): {missing:?}");
    }
}
