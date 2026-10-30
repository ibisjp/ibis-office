// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing the document model: shapes, text, charts, tables and pictures,
//! shared by the views of every format (and the export).

pub(crate) mod chart_view;
pub(crate) mod font_metrics;
pub(crate) mod glyph_outline;
pub(crate) mod bevel;
pub(crate) mod metafile_fonts;
pub(crate) mod picture_svg;
pub(crate) mod preload;
pub(crate) mod gradient;
pub(crate) mod math_font;
pub(crate) mod math_view;
pub(crate) mod rich_text;
pub(crate) mod rotated_text;
pub(crate) mod shape_view;
pub(crate) mod table_view;
pub(crate) mod web_image;

pub(crate) use shape_view::ColorMode;

pub(crate) mod vertical_text;
