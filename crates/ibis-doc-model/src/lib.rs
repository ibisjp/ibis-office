// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibisOffice presentation engine: the document model and its editing commands.
//!
//! This crate has no UI or platform dependencies so it can be shared by the
//! browser client (wasm), the server and native builds, and published as OSS.

pub mod analysis;
pub mod animation;
pub mod slide_comment;
pub mod chart;
pub mod chart_refresh;
pub mod command;
pub mod compare;
pub mod connector;
pub mod cover;
pub mod date_field;
pub mod eq_field;
pub mod field_format;
pub mod drawing;
pub mod error_check;
pub mod flow;
pub mod flow_command;
pub mod data_table;
pub mod geometry;
pub mod math;
pub mod latex;
pub mod forecast_sheet;
pub mod goal_seek;
pub mod merge;
pub mod model;
pub mod pivot;
pub mod query;
pub mod shape_geometry;
pub mod shape_merge;
pub mod sheet;
pub mod sheet_calc;
pub mod sheet_command;
pub mod sheet_ops;
pub mod form_control;
pub mod timeline;
pub mod slicer;
pub mod solver;
pub mod table;
pub mod text;

pub use chart::{ExFormat, ExLabels, View3D, 
    Axis, AxisCrosses, AxisKind, AxisSourceKind, BoxStats, Chart, ChartDataTable, ChartEx, TreeNode, ChartGroup, ChartText, ChartTitle, ChartType, DataLabels, DispBlanksAs, FractionRect, Grouping,
    LabelPiece, LabelPosition, Legend, LegendPosition, Marker, MarkerSource, MarkerSourceSymbol, MarkerSymbol, Number, OfPieKind, OfPieSettings, OfPieSplit, PointLabel, RadarStyle, Series, SeriesCells, ChartExOptions, ChartExSource, ExCells, ExSourceRef, BinBy, Side, TimeScale, TimeUnit,
    Trendline, TrendlineType,
    ErrorBarDirection, ErrorBarType, ErrorBarValueType, ErrorBars,
    format_number,
};
pub use field_format::{AdvanceFieldInfo, HyperlinkFieldInfo, apply_instruction_text_switches, apply_text_switches, compare_field_text, default_listnum_format, evaluate_with_resolver, expand_nested_fields, format_ask, format_autonum, format_autotext, format_autotextlist, format_bibliography, format_citation, format_doc_variable, format_filesize, format_fillin, format_hyperlink_field, format_listnum, format_quote, format_symbol, format_template_field, format_user_address, format_user_initials, format_user_name, formula_text_with_resolver, if_field_text, initials_from_name, parse_advance_field, parse_ask_field, parse_hyperlink_field, parse_info_field, parse_set_field, TableCellResolver};
pub use command::{Command, Document, EditError};
pub use flow_command::{CaptionNumberFormat, CaseConversion, Container, CrossReferenceKind, CrossReferenceTarget, CrossReferenceType, FlowCommand, FlowPos, FlowTableCellResolver, missing_header_footer_style, InsertFormat, ParaChange, RunChange, TableEdit, TocInstruction, all_paragraphs, caption_description_text, caption_labels, comment_at_pos, comment_marks, comment_range, cross_reference_targets, edit_field_at, entire_caption_text, find_table_cell_for_paragraph, flow_cell_text, get_field_at, get_table_cell_at, insert_caption_detailed, insert_cross_reference_target, insert_field_at_range, missing_table_of_figures_style, missing_toc_styles_for_levels, parse_cell_number, parse_listnum, parse_toc_instruction, relative_paragraph_position, table_of_figures_entries, toc_caption_label, toc_entries_with_instruction, updated_fields_in_range};
pub use sheet_command::{calculate_missing_values, ReportCell, CellEdit, CellInput, CellSlot, ChartMoveTarget, Edge, PutCell, SheetCommand, SheetPatch, SortBy, StyleChange};
pub use drawing::{
    Bevel, BevelPreset, BlipFill, Color, EndSize, Fill, Geometry, GradientPath, GradientStop, LineAlignment, LineCap, LineCompound, LineDash, LineEnd, LineJoin, Reflection, Rgb, Shadow, Stroke, TileAlign, TileFlip, pattern_tile, spreadsheet_pattern_of_preset, spreadsheet_pattern_tile,
};
pub use flow::{
    SubdocumentInfo,
    ControlInfo, ControlLock, TextEffects, TextShadow, ArtBorder, Block, BorderStyle, CellAnchor, CellDirection, Combine, CustomProperty, CustomPropertyValue, DocProperties, DocumentInteractionMode, DocumentProtection, DocumentViewMode, DropCap, DropCapLocation, EmphasisMark, RubyAlign, TableAlign, TableAnchor, TableCondition, TableFloat, TableLook, TableRegion, TableStyleRef, FlowAlign, FlowBorder, FlowCell, FlowDocument, FlowParagraph, FlowRow, FlowRun, FlowRunKind, FlowRunStyle, PictureLook,
    FormFieldData, FormFieldKind,
    AbstractList, AutoFit, DocGridType, FlowStyle, FlowStyles, FlowTable, Link, PreferredWidth, LineHeight, ListInstance, ListLevel, ListRef, ListSuffix, LineNumbers, LineNumberRestart, MultiplePages, Numbering, PageColumns, PageSetup, PageVAlign, NoteSettings, NoteFormat, NoteRestart, NotePosition, ParaFormat, ParagraphSpacingPreset, StyleSetPreset, Watermark,
    Comment, Note, PartKind, Revision, RevisionKind, RowHeight, RunFormat, SectionBreak, SectionSetup, ShadingPattern, StyleKind, TabAlign, TabChange, TabStop, UnderlineStyle, VerticalMerge, VerticalPosition, FlowThemeColor, NoteSeparator, format_list_number, format_page_number, list_labels,
    Ligatures, NumberForm, NumberSpacing,
    ThemeFontLang,
    DEFAULT_PICTURE_CONTROL_PLACEHOLDER_PNG, DEFAULT_PICTURE_CONTROL_WIDTH_EMU, DEFAULT_PICTURE_CONTROL_HEIGHT_EMU,
    DEFAULT_BUILDING_BLOCK_GALLERY_PROMPT, DEFAULT_BUILDING_BLOCK_GALLERY_NAME, DEFAULT_BUILDING_BLOCK_GALLERY_CATEGORY,
    DEFAULT_REPEATING_SECTION_TITLE, DEFAULT_REPEATING_SECTION_ITEM_PROMPT,
    DEFAULT_GROUP_PROMPT,
};
pub use merge::{
    FieldMapData, StandardMergeField, STANDARD_MERGE_FIELDS, auto_match_fields, default_address_format_with_mapping,
    default_greeting_format_with_mapping, resolve_merge_value,
};
pub use geometry::{EMU_PER_INCH, EMU_PER_POINT, Emu, EmuPoint, EmuRect, EmuSize, ResizeHandle};
pub use slide_comment::{Author, CommentReply, SlideComment};
pub use animation::{Animation, AnimationAfterEffect, AnimationClass, AnimationLook, AnimationRepeat, AnimationSide, AnimationStart, AnimationTrigger, ChartAnimationElement, ChartBuildStep, DiagramAnimationElement, DiagramBuildStep, TextUnit, UnitGap};
pub use model::{
    Glue,
    AltText, ArtisticEffect, AutoShape, BlipEffects, Clip, PictureTile, ClipOptions, Crop, CustomShow, EmbeddedFont, FloatingAnchor, Layout, LayoutId, LayoutPlaceholder, LayoutScale, Master, MasterId, NotesDate, NotesMarks, MediaBookmark, MediaCaptionTrack, MediaStart, MediaVolume, PlaceholderRef, SlidePlaceholder, Media, MediaFormat, MediaId, Picture, PictureWrapType, Placeholder, Presentation, Shape,
    WrapPoint, WrapPolygon,
    Section, ShapeGroup, GroupMember, ShapeId, ShapeKind, ShowProperties, ShowSlideRange, ShowType, Slide, SlideId, Tilt, CameraPreset, Transition, TransitionSound,
    ModifyVerifier, ObjectDefaults, ObjectPlacement, ShapeDefault,
    PhotoAlbum, PhotoAlbumFrame, PhotoAlbumLayout,
};
pub use form_control::{ControlHit, ControlKind, FormControl};
pub use shape_geometry::{FormulaGeometry, GeometryDef, GeometryPath, Guide, Outline, PathFill, PathOp, Subpath};
pub use sheet::{
    cell_name, CellImage, ExternalBook, ExternalSheet, CalculationCompatibilityVersion, CalcMode, Border, BorderLine, CellUnderline, CellAlign, CellError, CellGradient, CellKind, CellRange, CellStyle, CellValue, CellVerticalAlign, CustomCellStyle, DefinedName, Sheet,
    SheetCell, CellRun, DataTable, ConsolidateFunction, DataConsolidate, SubtotalParams, ScenarioInputCell, SheetScenario, SheetScenarios, SheetComment, ThemeColor, Unsaved, SheetHeaderFooter, SheetPrint, SheetProtection, ProtectedRange, WorkbookProtection, PasswordHash, ProtectionAllowed, SheetState, Workbook, WorkbookSheet, SheetFloat, ConditionalFormat, CfRule, CfKind, CfValue, CfStyle, CfIcon, DataBarAxisPosition, DataBarDirection, Validation, Hyperlink, AutoFilter, ColumnFilter, DateGroup, SheetTable, TableStyleInfo, TableTotal, ThreadPost, ThreadedComment, Sparkline, SparklineAxis, SparklineEmpty, SparklineGroup, SparklineKind,
    CustomWorkbookView, CustomSheetView,
    PhoneticType, PhoneticAlignment, PhoneticProperties,
};
pub use pivot::{DateGroupBy, PivotAxis, PivotDataField, PivotDetails, PivotField, PivotFieldGrouping, PivotFilter, PivotItem, PivotLine, PivotPageField, PivotSort, PivotSource, PivotStyleInfo, PivotTable, chart_from_pivot, update_chart_from_pivot};
pub use sheet_calc::{BookView, ensure_calc_index, warmup_calc_index_ahead};
pub use table::{CellBorders, Table, TableCell, TableRow, TableStyleOptions};
pub use text::{KernMin, NumberScheme, AutoNumber, 
    Bullet, SymbolBullet, FONT_SIZE_STEPS, FontAlign, FontSet, CharacterFormat, ListKind, ParagraphFormat, ThemeFonts, is_east_asian, Insets, LineSpacing, Paragraph, Run, RunStyle, TextAlign, TextAutofit, TextBody,
    SlideLink, TextField, TextStyle, VerticalAnchor, step_font_size, autofit_font_size_pt, Strike, Caps, Underline, VerticalKind,
    TextTabAlign, TextTabStop,
};
