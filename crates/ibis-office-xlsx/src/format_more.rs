// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示形式 menu's cell formats beside the toolbar's: 文字の方向 (Excel's
//! ホーム > 方向), 上付き・下付き, 塗りつぶしのパターン (the patterns of
//! セルの書式設定 > 塗りつぶし), パターンの色, and 塗りつぶしのグラデーション.

use ibis_doc_model::{CellGradient, GradientPath, Rgb, StyleChange, VerticalPosition};

use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::ext::*;

/// Excel's patterns, as its dialog names them, by the DrawingML names the
/// model keeps them by (saved as `patternType`'s names).
pub(crate) const PATTERNS: [(&str, &str); 18] = [
    ("_shell.none", ""),
    ("_xlsx.75-gray", "pct75"),
    ("_xlsx.50-gray", "pct50"),
    ("_xlsx.25-gray", "pct25"),
    ("_xlsx.12-5-gray", "pct10"),
    ("_xlsx.6-25-gray", "pct5"),
    ("_xlsx.horizontal-stripe", "dkHorz"),
    ("_xlsx.vertical-stripe", "dkVert"),
    ("_xlsx.downward-diagonal-stripe", "dkDnDiag"),
    ("_xlsx.upward-diagonal-stripe", "dkUpDiag"),
    ("_xlsx.diagonal-crosshatch", "cross"),
    ("_xlsx.thick-diagonal-crosshatch", "trellis"),
    ("_xlsx.light-horizontal-stripe", "ltHorz"),
    ("_xlsx.light-vertical-stripe", "ltVert"),
    ("_xlsx.light-downward-diagonal-stripe", "ltDnDiag"),
    ("_xlsx.light-upward-diagonal-stripe", "ltUpDiag"),
    ("_xlsx.small-grid", "smGrid"),
    ("_xlsx.narrow-diagonal-crosshatch", "diagCross"),
];

/// Standard pattern colors (Excel's pattern palette).
pub(crate) const PATTERN_COLORS: [(&str, Rgb); 12] = [
    ("_xlsx.automatic-black", Rgb(0x000000)),
    ("_shell.dark-gray", Rgb(0x595959)),
    ("_shell.gray", Rgb(0x808080)),
    ("_shell.white", Rgb(0xFFFFFF)),
    ("_shell.dark-red", Rgb(0xC00000)),
    ("_shell.red", Rgb(0xFF0000)),
    ("_shell.orange", Rgb(0xED7D31)),
    ("_shell.yellow", Rgb(0xFFC000)),
    ("_shell.green", Rgb(0x70AD47)),
    ("_shell.light-blue", Rgb(0x5B9BD5)),
    ("_shell.blue", Rgb(0x4472C4)),
    ("_shell.purple", Rgb(0x7030A0)),
];

pub(crate) fn menus() -> Vec<MenuEntry> {
    let t = |s: &str| crate::shell::i18n::text(s);
    let item = |id: &'static str, label: &str, change: StyleChange| -> MenuEntry { MenuItem::new(id, t(label)).on(move |this, _, cx| this.format_cells(change.clone(), cx)).into() };
    let direction = vec![
        item("xlsx.rotate-ccw", "_xlsx.angle-counterclockwise", StyleChange::Rotation(45)),
        item("xlsx.rotate-cw", "_xlsx.angle-clockwise", StyleChange::Rotation(135)),
        item("xlsx.rotate-vertical", "_shell.vertical", StyleChange::Rotation(255)),
        item("xlsx.rotate-up", "_xlsx.rotate-text-up", StyleChange::Rotation(90)),
        item("xlsx.rotate-down", "_xlsx.rotate-text-down", StyleChange::Rotation(180)),
        item("xlsx.rotate-none", "_xlsx.no-rotation", StyleChange::Rotation(0)),
    ];
    let effects = vec![
        item("xlsx.superscript", "docx.chrome.superscript", StyleChange::VertAlign(VerticalPosition::Superscript)),
        item("xlsx.subscript", "docx.chrome.subscript", StyleChange::VertAlign(VerticalPosition::Subscript)),
        item("xlsx.baseline", "_xlsx.normal-no-superscript-or-subscript", StyleChange::VertAlign(VerticalPosition::Baseline)),
    ];
    let patterns = PATTERNS
        .iter()
        .map(|&(label, preset)| -> MenuEntry {
            MenuItem::new("xlsx.pattern", t(label))
                .on(move |this, _, cx| {
                    let (fg, bg) = this.current_cell()
                        .and_then(|(c, r)| this.current_sheet().and_then(|(_, s)| s.cell(r, c).map(|x| {
                            let st = &s.styles[x.style as usize];
                            let fg = st.pattern.as_ref().map(|p| p.1).unwrap_or(Rgb(0));
                            let bg = st.pattern.as_ref().map(|p| p.2).or(st.fill).unwrap_or(Rgb(0xFF_FFFF));
                            (fg, bg)
                        })))
                        .unwrap_or((Rgb(0), Rgb(0xFF_FFFF)));
                    let change = if preset.is_empty() {
                        StyleChange::Pattern(None)
                    } else {
                        StyleChange::Pattern(Some((preset.to_string(), fg, bg)))
                    };
                    this.format_cells(change, cx)
                })
                .into()
        })
        .collect();

    let pattern_colors = PATTERN_COLORS
        .iter()
        .map(|&(label, col)| -> MenuEntry {
            MenuItem::new("xlsx.pattern-color", t(label))
                .icon("circle")
                .on(move |this, _, cx| {
                    this.format_cells(StyleChange::PatternColor(col), cx)
                })
                .into()
        })
        .collect();

    let mut gradients: Vec<MenuEntry> = vec![
        MenuItem::new("xlsx.gradient-none", t("_xlsx.no-gradient"))
            .on(|this, _, cx| this.format_cells(StyleChange::Gradient(None), cx))
            .into(),
    ];
    for (id, label, deg) in [
        ("xlsx.gradient-horz", "_xlsx.horizontal-left-to-right", 0),
        ("xlsx.gradient-vert", "_xlsx.vertical-top-to-bottom", 90),
        ("xlsx.gradient-diag-down", "_xlsx.diagonal-top-left-to-bottom-right", 45),
        ("xlsx.gradient-diag-up", "_xlsx.diagonal-top-right-to-bottom-left", 135),
    ] {
        gradients.push(
            MenuItem::new(id, t(label))
                .on(move |this, _, cx| {
                    let col = this.current_cell()
                        .and_then(|(c, r)| this.current_sheet().and_then(|(_, s)| s.cell(r, c).and_then(|x| s.styles[x.style as usize].fill)))
                        .unwrap_or(Rgb(0x4472C4));
                    let grad = CellGradient::Linear {
                        degree: deg,
                        stops: vec![(0, col), (1000, Rgb(0xFF_FFFF))],
                    };
                    this.format_cells(StyleChange::Gradient(Some(grad)), cx)
                })
                .into()
        );
    }
    gradients.push(
        MenuItem::new("xlsx.gradient-center", t("_xlsx.from-center"))
            .on(|this, _, cx| {
                let col = this.current_cell()
                    .and_then(|(c, r)| this.current_sheet().and_then(|(_, s)| s.cell(r, c).and_then(|x| s.styles[x.style as usize].fill)))
                    .unwrap_or(Rgb(0x4472C4));
                let grad = CellGradient::Path {
                    path: GradientPath::Rect,
                    focus: [50_000, 50_000, 50_000, 50_000],
                    stops: vec![(0, col), (1000, Rgb(0xFF_FFFF))],
                };
                this.format_cells(StyleChange::Gradient(Some(grad)), cx)
            })
            .into()
    );

    vec![
        Submenu::tr("xlsx.text-direction", "text_rotation_angleup", direction).into(),
        Submenu::tr("xlsx.font-effects", "text_fields", effects).into(),
        Submenu::tr("xlsx.fill-pattern", "format_color_fill", patterns).into(),
        Submenu::tr("xlsx.pattern-color", "palette", pattern_colors).into(),
        Submenu::tr("xlsx.fill-gradient", "gradient", gradients).into(),
    ]
}
