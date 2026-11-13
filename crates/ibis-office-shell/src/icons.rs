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

