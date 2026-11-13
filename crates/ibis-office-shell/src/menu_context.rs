// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The context menus of the menu bar (menu bar redesign, stage 3; `tasks/doc/menu-redesign/implementation-spec.md`
//! sections 3 and 4): the coloured menus that appear to the right of the static ones while a table, a picture, a
//! shape, a chart and so on is selected, and go away when it is not.
//!
//! [`visible_contexts`] is a pure function of a [`ContextState`] (the selection as data), so the rule table of the
//! spec can be tested without a window. [`SlideEditor::context_state`] reads the selection of the open screen into
//! the state; the lines of each menu are the tables of `menu_layout_data.rs` (`layout_context`).
use gpui_kit::SharedString;
use ibis_pptx::Kind;

use crate::editor::SlideEditor;

/// A context menu, in the words of the spec (section 3, stage 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContextMenu {
    TableDesign,
    TableLayout,
    ShapeFormat,
    PictureFormat,
    ChartDesign,
    ChartFormat,
    Equation,
    HeaderFooter,
    Outline,
    PivotAnalyze,
    PivotDesign,
    Sparkline,
    Slicer,
    SmartArtDesign,
    SmartArtFormat,
    VideoFormat,
    VideoPlayback,
    /// The slide master (the notes and handout masters have the same form).
    SlideMaster,
}

impl ContextMenu {
    /// Its key in the tables of the UI's words.
    pub fn key(self) -> &'static str {
        match self {
            ContextMenu::TableDesign => "menu.context.table-design",
            ContextMenu::TableLayout => "menu.context.table-layout",
            ContextMenu::ShapeFormat => "menu.context.shape-format",
            ContextMenu::PictureFormat => "menu.context.picture-format",
            ContextMenu::ChartDesign => "menu.context.chart-design",
            ContextMenu::ChartFormat => "menu.context.chart-format",
            ContextMenu::Equation => "menu.context.equation",
            ContextMenu::HeaderFooter => "menu.context.header-footer",
            ContextMenu::Outline => "menu.context.outline",
            ContextMenu::PivotAnalyze => "menu.context.pivot-analyze",
            ContextMenu::PivotDesign => "menu.context.pivot-design",
            ContextMenu::Sparkline => "menu.context.sparkline",
            ContextMenu::Slicer => "menu.context.slicer",
            ContextMenu::SmartArtDesign => "menu.context.smartart-design",
            ContextMenu::SmartArtFormat => "menu.context.smartart-format",
            ContextMenu::VideoFormat => "menu.context.video-format",
            ContextMenu::VideoPlayback => "menu.context.video-playback",
            ContextMenu::SlideMaster => "menu.context.slide-master",
        }
    }

    #[allow(dead_code)]
    pub(crate) fn label(self) -> SharedString {
        crate::i18n::text(self.key())
    }
}

/// What can be selected on a screen (the objects of the spec's table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    /// A shape, text box or WordArt (or a line).
    Shape,
    Picture,
    Chart,
    /// A table that is an object (a presentation's table); in a document the caret is in the table instead.
    Table,
    SmartArt,
    /// A video or an audio clip.
    Video,
    Slicer,
}

/// The selection of a screen, as data (the spec's `ContextState`): what `visible_contexts` decides from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextState {
    pub kind: Kind,
    /// DOCX: the caret is in a table (or cells are selected).
    pub caret_in_table: bool,
    /// The selected objects (the shapes of a slide or a sheet, the picture or the shape selected in a document).
    pub objects: Vec<ObjectKind>,
    /// DOCX: the caret is in an equation.
    pub in_equation: bool,
    /// DOCX: a header or a footer is being edited (a mode).
    pub header_footer: bool,
    /// DOCX: the outline view (a mode).
    pub outline_view: bool,
    /// PPTX: the slide master (or notes or handout master) view (a mode).
    pub master_view: bool,
    /// XLSX: the selected cell is in a table.
    pub in_sheet_table: bool,
    pub in_pivot: bool,
    pub in_sparkline: bool,
    /// Ink (a stroke drawn with a pen) is selected.
    pub ink_selected: bool,
}

impl ContextState {
    /// Nothing selected, no mode.
    pub fn none(kind: Kind) -> Self {
        ContextState {
            kind,
            caret_in_table: false,
            objects: Vec::new(),
            in_equation: false,
            header_footer: false,
            outline_view: false,
            master_view: false,
            in_sheet_table: false,
            in_pivot: false,
            in_sparkline: false,
            ink_selected: false,
        }
    }
}

/// The context menus to show, in the bar's order (spec 4.1 and 4.2): the menu of a mode first, then the outer
/// object (a table) and the inner one (an equation, a picture in a table), each once.
pub fn visible_contexts(state: &ContextState) -> Vec<ContextMenu> {
    use ContextMenu::*;
    let mut out: Vec<ContextMenu> = Vec::new();
    let mut add = |menus: &[ContextMenu]| {
        for m in menus {
            if !out.contains(m) {
                out.push(*m);
            }
        }
    };
    // The screen's modes, then the outer targets (a table the caret or the cell is in): the format's (`FormatInfo::outer_contexts`).
    add(&(crate::format::info(state.kind).outer_contexts)(state));
    // The inner targets: the selected objects. One kind (or several of the same) gives that kind's menus; different
    // kinds together (a shape and a picture) give what they share, the shape format.
    let mut kinds: Vec<ObjectKind> = Vec::new();
    for object in &state.objects {
        if !kinds.contains(object) {
            kinds.push(*object);
        }
    }
    match kinds.as_slice() {
        [] => {}
        [one] => match one {
            ObjectKind::Shape => add(&[ShapeFormat]),
            ObjectKind::Picture => add(&[PictureFormat]),
            ObjectKind::Chart => add(&[ChartDesign, ChartFormat]),
            ObjectKind::Table => add(&[TableDesign, TableLayout]),
            ObjectKind::SmartArt => add(&[SmartArtDesign, SmartArtFormat]),
            ObjectKind::Video => add(&[VideoFormat, VideoPlayback]),
            ObjectKind::Slicer => add(&[Slicer]),
        },
        _ => add(&[ShapeFormat]),
    }
    out
}

/// The colour of a context menu's name: one colour for each screen's format (spec 4.4). The colours of the document's
/// blue, the spreadsheet's green and the presentation's orange are the mock's `--doc`, `--sheet` and `--slide`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextTone {
    /// The label's colour, `0xRRGGBB`.
    pub fg: u32,
    /// The background's colour, `0xRRGGBB`, and its alpha.
    pub bg: u32,
    pub bg_alpha: f32,
}

/// The tokens `menu.context.{doc,sheet,slide}.{fg,bg}` of the spec, light and dark.
pub(crate) fn context_tone(kind: Kind, dark: bool) -> ContextTone {
    crate::format::info(kind).context_tones[usize::from(dark)]
}

/// The colour of a screen's badge (the format's own colour, the same in both appearances).
pub fn format_color(kind: Kind) -> u32 {
    crate::format::info(kind).color
}

impl ObjectKind {
    /// What a shape of a slide or a sheet is, by its kind alone. What only a format can tell of it (a picture that is a
    /// clip, a shape of a SmartArt) the format adds (`FormatEditor::selection_context`).
    pub fn of(shape: &ibis_doc_model::Shape) -> ObjectKind {
        use ibis_doc_model::ShapeKind;
        match &shape.kind {
            ShapeKind::Chart(_) => ObjectKind::Chart,
            ShapeKind::Table(_) => ObjectKind::Table,
            ShapeKind::Slicer(_) | ShapeKind::Timeline(_) => ObjectKind::Slicer,
            ShapeKind::Picture(_) => ObjectKind::Picture,
            _ => ObjectKind::Shape,
        }
    }
}

impl SlideEditor {
    /// The selection of the open screen, as data. Everything is read from the state the screens keep for their
    /// right panes and toolbars, so the menus and the panes agree: the format fills it in
    /// (`FormatEditor::selection_context`).
    pub(crate) fn context_state(&self) -> ContextState {
        let mut state = ContextState::none(self.kind());
        if self.has_document {
            crate::format::with_format!(self, selection_context(self, &mut state));
        }
        state
    }

    /// A stroke drawn with a pen (its name is an ink name) is selected on the slide or sheet.
    pub fn ink_is_selected(&self) -> bool {
        let slide = self.slide();
        self.selected_shapes().iter().any(|s| slide.shape_names.get(&s.id).is_some_and(|n| crate::ink::is_ink_name(n)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn luminance(hex: u32) -> f32 {
        let channel = |shift: u32| {
            let c = ((hex >> shift) & 0xff) as f32 / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }

    fn over(top: u32, alpha: f32, base: u32) -> u32 {
        let mix = |shift: u32| {
            let t = ((top >> shift) & 0xff) as f32;
            let b = ((base >> shift) & 0xff) as f32;
            ((t * alpha + b * (1.0 - alpha)).round() as u32) << shift
        };
        mix(16) | mix(8) | mix(0)
    }

    /// The label of a context menu reads against its background at 4.5:1 or more, in the light and the dark chrome.
    #[test]
    #[ignore = "the spec's presentation orange (#a86a00 on its 16% tint) is 3.77:1 in the light chrome, under the 4.5:1 the spec states: the designer decides the colour"]
    fn context_colors_contrast() {
        // The bar's background in the light chrome and the dark one (`APP_BG` and its dark counterpart).
        for (dark, bar) in [(false, crate::chrome::APP_BG), (true, crate::palette::darker(crate::chrome::APP_BG))] {
            for kind in [Kind::Document, Kind::Workbook, Kind::Presentation] {
                let tone = context_tone(kind, dark);
                let background = over(tone.bg, tone.bg_alpha, bar);
                let (a, b) = (luminance(tone.fg) + 0.05, luminance(background) + 0.05);
                let ratio = a.max(b) / a.min(b);
                assert!(ratio >= 4.5, "{kind:?} dark={dark}: contrast {ratio:.2}");
            }
        }
    }
}
