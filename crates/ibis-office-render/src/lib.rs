// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing the document model: shapes, text, charts, tables and pictures,
//! shared by the views of every format (and the export).
//!
//! Depends on: `ibis-office-base` (and the model crates). Nothing above it: what a format draws as a shape (a worksheet, a page
//! of a document) is asked of the format through `shape_view::format_hooks`, whose functions the app hands over when it starts (`register_hooks.rs`).
//! Known compromise of the split (stage 1): every module and most items are `pub` (they were `pub(crate)`); narrow them to what
//! the other crates use when the formats are cut out.

pub mod chart_view;
pub(crate) mod fallback_widths;
pub mod font_metrics;
pub(crate) mod glyph_outline;
#[cfg(target_family = "wasm")]
pub mod browser_raster;
pub mod bevel;
pub(crate) mod blur_shadow;
pub mod metafile_fonts;
pub mod picture_svg;
pub(crate) mod soft_edge;
#[cfg(all(feature = "export", not(target_family = "wasm")))]
pub(crate) mod cmyk_jpeg;
pub mod preload;
pub mod gradient;
pub mod line_break;
pub(crate) mod math_font;
pub mod math_view;
pub mod rich_text;
pub(crate) mod rotated_text;
pub mod shape_view;
pub mod table_view;
pub mod web_image;

pub use shape_view::ColorMode;

pub mod vertical_text;
pub mod ime_text;
pub mod markup_view;
pub mod media_player;
