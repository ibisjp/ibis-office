// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Editing commands.
//!
//! Every change to a [`Presentation`] goes through a [`Command`]. Applying a
//! command returns its inverse, which is what undo applies.
//!
//! Slides and shapes (presentations, and the shapes on worksheets) are
//! edited by the commands here; worksheets' cells by
//! [`crate::SheetCommand`] (`sheet_command.rs`) and flowing documents by
//! [`crate::FlowCommand`] (`flow_command.rs`), so that each format's
//! commands are in a file of its own. The same values are
//! what the client will send to the server over the binary protocol, so a
//! command carries ids and final values, never UI state.

use crate::drawing::{Fill, Stroke};
use crate::geometry::{EmuRect, EmuSize};
use crate::model::{AutoShape, Presentation, Shape, ShapeId, ShapeKind, Slide, SlideId};
use crate::text::{TextBody, TextStyle};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    InsertSlide { index: usize, slide: Slide },
    DeleteSlide { slide: SlideId },
    /// Insert shapes at indices, in ascending order (each index is where the shape is once the ones before it are in). Exactly: nothing else
    /// changes. The inverse of [`Command::DeleteShape`].
    InsertShape { slide: SlideId, shapes: Vec<(usize, Shape)> },
    /// Delete shapes. `with_attached`: the shapes attached to each (see [`Slide::attached`]) go with it, and a shape that is attached cannot be named
    /// without its owner (the delete of the user); not: exactly the shapes named, as when a chart is replaced or removed with the shapes drawn over it. The
    /// inverse is [`Command::InsertShape`] of all that were removed.
    DeleteShape { slide: SlideId, shapes: Vec<ShapeId>, with_attached: bool },
    /// Set the frames of shapes. `with_attached`: the shapes attached to each follow it (moved and scaled with it, unless they are named too: then
    /// their own frame is used), and a placeholder that takes its place from the layout has its own place from then on (the move of the user); not:
    /// exactly the frames named (the inverse of the first, so it needs neither). The inverse is [`Command::SetFrame`] of the old frames of all shapes
    /// that changed, not with attached.
    SetFrame { slide: SlideId, frames: Vec<(ShapeId, EmuRect)>, with_attached: bool },
    /// Several commands as one (the inverse of an edit that changes several
    /// shapes, such as removing a chart with the shapes drawn over it).
    Batch(Vec<Command>),
    /// Move a slide to `index` in the slide order (the index it has after
    /// the move).
    MoveSlide { slide: SlideId, index: usize },
    /// Move a shape to `index` in its slide's z-order (0 is the back).
    MoveShape { slide: SlideId, shape: ShapeId, index: usize },
    /// Set a shape's rotation (60 000ths of a degree, clockwise, kept in
    /// 0..21 600 000) and flips.
    SetTransform { slide: SlideId, shape: ShapeId, rotation: i32, flip_h: bool, flip_v: bool },
    /// Set the fill of a shape (an auto shape or a text box, which becomes
    /// an auto shape of the same frame when filled).
    SetFill { slide: SlideId, shape: ShapeId, fill: Fill },
    /// Set the outline of a shape, a picture or a line (`None` removes an
    /// outline; a line keeps one).
    SetStroke { slide: SlideId, shape: ShapeId, stroke: Option<Stroke> },
    SetText { slide: SlideId, shape: ShapeId, text: String },
    SetTextStyle { slide: SlideId, shape: ShapeId, style: TextStyle },
    /// Replace a shape's whole text body. The inverse of the text edits, which
    /// would otherwise lose run and paragraph formatting on undo.
    SetTextBody { slide: SlideId, shape: ShapeId, body: TextBody },
    /// Replace `remove` characters at character `at` of a shape's text with
    /// `insert`, keeping runs and their formatting (see [`TextBody::splice`]).
    ReplaceTextRange { slide: SlideId, shape: ShapeId, at: usize, remove: usize, insert: String },
    /// Search replacement: inserted text takes the first matched run format.
    ReplaceText { slide: SlideId, shape: ShapeId, at: usize, remove: usize, insert: String },
    /// Replace what a shape is (the inverse of changes that turn a text box
    /// into an auto shape).
    SetShapeKind { slide: SlideId, shape: ShapeId, kind: Box<ShapeKind> },
    /// Give characters `start..end` of a shape's text a character format
    /// (範囲の文字の書式).
    SetCharacterFormat { slide: SlideId, shape: ShapeId, start: usize, end: usize, format: crate::text::CharacterFormat },
    /// Give the paragraphs characters `start..end` of a shape's text touch a
    /// paragraph format (配置・行間・箇条書き・インデント).
    SetParagraphFormat { slide: SlideId, shape: ShapeId, start: usize, end: usize, format: crate::text::ParagraphFormat },
    /// Whether placeholder `shape` has its own place (see
    /// [`crate::model::SlidePlaceholder`]); moving it gives it one.
    SetPlaceholderOwnFrame { slide: SlideId, shape: ShapeId, own: bool },
    /// Replace a slide (the same id) with `slide`: レイアウトの適用
    /// (see [`Presentation::with_layout`]), and its own inverse.
    SetSlide { slide: Slide },
    /// Replace the guides: those of `slide`, or of the normal view (`None`); its own inverse.
    SetGuides { slide: Option<SlideId>, guides: Vec<crate::model::SlideGuide> },
    /// Set a slide's background (背景の書式設定).
    SetBackground { slide: SlideId, fill: Fill },
    /// Change the presentation canvas, preserving every shape and the notes page size.
    SetSlideSize { size: EmuSize },
    /// The layouts as scaled with the slide canvas's contents, and how
    /// (`Presentation::layout_scale`); its own inverse.
    SetLayouts { layouts: Vec<crate::model::Layout>, scale: crate::model::LayoutScale },
    /// Rename a master (マスター名の変更); its own inverse.
    RenameMaster { master: crate::model::MasterId, name: String },
    /// Add a master at `index` (a copy of another: its `copy_of` names it);
    /// the inverse of `DeleteMaster`. Its layouts are added with
    /// `InsertLayout`.
    InsertMaster { index: usize, master: crate::model::Master },
    /// Remove a master no layout follows; the inverse of `InsertMaster`.
    DeleteMaster { master: crate::model::MasterId },
    /// Rename a layout (レイアウト名の変更); its own inverse.
    RenameLayout { layout: crate::model::LayoutId, name: String },
    /// Add a layout at `index` (a copy of another: its `copy_of` names it);
    /// the inverse of `DeleteLayout`.
    InsertLayout { index: usize, layout: crate::model::Layout },
    /// Remove a layout no slide uses; the inverse of `InsertLayout`.
    DeleteLayout { layout: crate::model::LayoutId },
    /// The theme's colours and fonts changed (テーマの色・フォント). Only
    /// the theme itself: the shapes drawn in the new colours come with it
    /// as [`Command::SetSlide`]s; its own inverse.
    SetTheme { colors: Vec<(String, crate::drawing::Rgb)>, fonts: Option<crate::text::ThemeFonts> },
    /// The theme's format / effect scheme changed (テーマ効果: `a:fmtScheme/@name`).
    SetThemeEffects { name: Option<String> },
    /// The number the first slide has (スライド番号の開始値); the slides'
    /// number fields follow. Its own inverse.
    SetFirstSlideNumber { number: usize },
    /// Set ヘッダーとフッター's marks for the whole presentation.
    SetHeaderFooterMarks { marks: crate::model::HeaderFooterMarks },
    /// The header, date, footer and page number of the notes pages; its own inverse.
    SetNotesMarks { marks: crate::model::NotesMarks },
    /// スライド ショーの設定 (`p:showPr`). Its own inverse.
    SetShowProperties { properties: crate::model::ShowProperties },
    /// 目的別スライド ショーのリスト (`p:custShowLst`). Its own inverse.
    SetCustomShows { custom_shows: Vec<crate::model::CustomShow> },
    /// 最終版としてマーク（PowerPoint の「最終版にする」: `docProps/custom.xml` の `_MarkAsFinal`）。
    SetMarkAsFinal { mark_as_final: bool },
    /// 読み取り専用を推奨（PowerPoint の「読み取り専用を推奨」: `p1710:readonlyRecommended`）。
    SetReadOnlyRecommended { read_only_recommended: bool },
    /// The file's CMYK mode (on or off, the profile's name, the colours given as CMYK), replaced as a whole.
    SetCmykMode { cmyk: crate::cmyk::CmykMode },
    /// Turn on (or off) ibisOffice's own features PowerPoint does not have, for the whole file.
    SetExtendedFeatures { enabled: bool },
    /// 書き込みパスワード（`p:modifyVerifier`）を設定・解除する。
    SetModifyVerifier { verifier: Option<crate::model::ModifyVerifier> },
    /// ドキュメント プロパティ（タイトル・作成者・カスタムプロパティ等: `docProps/core.xml`, `docProps/app.xml`, `docProps/custom.xml`）.
    SetDocumentProperties { properties: crate::flow::DocProperties },
    /// 既定の図形・線の書式設定（`<a:objectDefaults>`）。
    SetObjectDefaults { defaults: crate::model::ObjectDefaults },
    /// フォントの代替（フォント代替マップの更新）。
    SetFontSubstitutions { substitutions: std::collections::BTreeMap<String, String> },
    /// 埋め込みフォント一覧の更新。
    SetEmbeddedFonts { fonts: Vec<crate::model::EmbeddedFont> },
    /// フォトアルバムの設定 (`p:photoAlbum`)。
    SetPhotoAlbum { album: Option<crate::model::PhotoAlbum> },
    /// Atomic inverse of leaf insertion/deletion and its group membership.
    ShapeStructure {slide:SlideId,command:Box<Command>,groups:Vec<crate::model::ShapeGroup>},
    /// Replace the slide group hierarchy; the flattened shapes retain their frames.
    SetShapeGroups {slide:SlideId,groups:Vec<crate::model::ShapeGroup>},
    /// Replace the section names, order and contiguous slide membership.
    SetSections { sections: Vec<crate::model::Section> },
    /// Atomic inverse of a slide-list edit and its section membership.
    SlideStructure { command: Box<Command>, sections: Vec<crate::model::Section> },
    /// A shape made a hyperlink (`Some`) or not.
    SetShapeLink { slide: SlideId, shape: ShapeId, link: Option<crate::text::SlideLink> },
    /// A shape hyperlink's ScreenTip / tooltip (`a:hlinkClick/@tooltip`).
    SetShapeLinkTooltip { slide: SlideId, shape: ShapeId, tooltip: Option<String> },
    /// 縦横比を固定: a shape's proportions kept (or not) when resized.
    SetAspectLocked { slide: SlideId, shape: ShapeId, locked: bool },
    /// Name a shape (選択ウィンドウ); an empty name takes it away.
    SetShapeName { slide: SlideId, shape: ShapeId, name: String },
    /// Alternative text of a shape (代替テキスト: `cNvPr/@descr`, `@title`).
    SetShapeAltText { slide: SlideId, shape: ShapeId, alt_text: crate::model::AltText },
    /// Mark a shape as decorative (装飾用オブジェクト: `a1611:decorative`).
    SetShapeDecorative { slide: SlideId, shape: ShapeId, decorative: bool },
    /// How text wraps round a picture or video (ibisOffice's own; `ObjectWrapKind::None` takes it away).
    SetObjectWrap { slide: SlideId, shape: ShapeId, wrap: crate::ObjectWrap },
    /// Whether a text box's text ignores the wrap round pictures and videos.
    SetWrapIgnore { slide: SlideId, shape: ShapeId, ignore: bool },
    /// The slide's chains of text boxes made `chains`, and the paragraphs of the boxes in `bodies` made what it says (a link made
    /// or broken, a text flowed again): one step, so that undo brings both back.
    SetTextChains { slide: SlideId, chains: Vec<crate::TextChain>, bodies: Vec<(ShapeId, Vec<crate::text::Paragraph>)> },
    /// セルに対する配置方式 (editAs: twoCell, oneCell, absolute).
    SetShapePlacement { slide: SlideId, shape: ShapeId, placement: crate::model::ObjectPlacement },
    /// オブジェクトを印刷する (fPrintsWithSheet).
    SetShapePrint { slide: SlideId, shape: ShapeId, print: bool },
    /// セルの値を表示する数式連動 (`xdr:f`).
    SetShapeFormula { slide: SlideId, shape: ShapeId, formula: Option<String> },
    /// 背景グラフィックを表示しない: a slide's backdrop hidden or shown.
    SetBackgroundGraphicsHidden { slide: SlideId, hidden: bool },
    /// Show or hide a shape (選択ウィンドウ's eye).
    SetShapeHidden { slide: SlideId, shape: ShapeId, hidden: bool },
    /// ズーム後に元へ戻る: a zoom shape returns to its origin slide after viewing (or not).
    SetShapeReturnToZoom { slide: SlideId, shape: ShapeId, return_to_zoom: bool },
    /// The picture `shape` is a ズーム object (PowerPoint's スライド/セクション/サマリー ズーム) or is not one (`None`).
    SetZoomObject { slide: SlideId, shape: ShapeId, zoom: Option<crate::model::ZoomObject> },
    /// ズームの背景: a zoom shape shows the slide background or has a transparent background (`showBg`).
    SetShapeZoomShowBackground { slide: SlideId, shape: ShapeId, show_bg: Option<bool> },
    /// ズームの継続時間: zoom transition duration in milliseconds (`dur`).
    SetShapeZoomDuration { slide: SlideId, shape: ShapeId, duration_ms: Option<u32> },
    /// コネクタの接続（結合点）: what its ends are attached to (`a:stCxn`, `a:endCxn`).
    SetShapeGlue { slide: SlideId, shape: ShapeId, glue: Option<crate::model::Glue> },
    /// コネクタの配置と反転（追従時更新用）.
    SetConnectorFrame { slide: SlideId, shape: ShapeId, frame: EmuRect, flip_h: bool, flip_v: bool },
    /// Set a slide's speaker notes (スピーカー ノート), paragraphs joined by `\n`.
    SetNotes { slide: SlideId, notes: String },
    /// Set a slide's name (スライド名・非表示タイトル: p:cSld/@name).
    SetSlideName { slide: SlideId, name: Option<String> },
    /// Set or remove a placeholder definition for a shape.
    SetPlaceholder { slide: SlideId, shape: ShapeId, placeholder: Option<crate::model::SlidePlaceholder> },
    /// A text command (`SetText`, `SetTextStyle`, `EditText`,
    /// `SetCharacterFormat`, `SetParagraphFormat`, `SetTextBody`) on the text
    /// of the cell at `row`, `column` of the table it names, instead of the
    /// shape's text (セルの文字の編集).
    InCell { row: usize, column: usize, command: Box<Command> },
    /// Set a slide's transition (画面切り替え), or none.
    SetTransition { slide: SlideId, transition: Option<crate::model::Transition> },
    /// Set a slide's animations (アニメーション: effects added, changed,
    /// reordered, removed).
    SetAnimations { slide: SlideId, animations: Vec<crate::Animation> },
    /// Set a slide's comments (コメント: added, answered, resolved, removed).
    SetComments { slide: SlideId, comments: Vec<crate::SlideComment> },
    /// A worksheet edit (see `sheet_command.rs`).
    Sheet(crate::sheet_command::SheetCommand),
    /// A flowing document's edit (see `flow_command.rs`).
    Flow(crate::flow_command::FlowCommand),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    NoSuchSlide(SlideId),
    NoSuchShape(ShapeId),
    DuplicateId,
    IndexOutOfRange,
    InvalidSlideSize,
    InvalidSections,
    InvalidShapeGroups,
    LastSlide,
    NotText(ShapeId),
    /// The shape has no such property (a fill on a picture, no outline on a line).
    NotApplicable(ShapeId),
    /// The shape is part of another one (drawn over a chart) and is edited with it.
    Attached(ShapeId),
    /// A cell of a data table, which is changed as a whole.
    DataTablePart,
    /// The workbook structure is protected.
    WorkbookProtected,
    /// テーブルを含むブックではユーザー設定のビューを使用できない.
    CustomViewsDisabledByTable,
    /// 指定された項目が見つからない.
    NotFound,
    /// A locked content control (`w:lock`): it cannot be deleted or its content edited.
    ControlLocked,
    /// 変更履歴の記録がロックされている（Word の変更履歴の記録のロック / w:documentProtection w:edit="trackedChanges"）.
    TrackingLocked,
    /// This document is a read-only preview.
    ReadOnly,
    /// A slide uses the layout.
    LayoutInUse,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSuchSlide(id) => write!(f, "slide {} does not exist", id.0),
            Self::NoSuchShape(id) => write!(f, "shape {} does not exist", id.0),
            Self::DuplicateId => write!(f, "id is already in use"),
            Self::IndexOutOfRange => write!(f, "index is out of range"),
            Self::LayoutInUse => write!(f, "a slide uses the layout"),
            Self::InvalidShapeGroups => write!(f,"shape group IDs or membership are invalid"),
            Self::InvalidSections => write!(f, "section IDs and ordered slide membership are invalid"),
            Self::InvalidSlideSize => write!(f, "slide dimensions must be between 1 and 56 inches"),
            Self::LastSlide => write!(f, "a presentation keeps at least one slide"),
            Self::NotText(id) => write!(f, "shape {} has no text", id.0),
            Self::NotApplicable(id) => write!(f, "shape {} cannot take this change", id.0),
            Self::ControlLocked => write!(f, "the content control is locked"),
            Self::Attached(id) => write!(f, "shape {} is part of another shape", id.0),
            Self::DataTablePart => write!(f, "a cell of a data table cannot be changed on its own"),
            Self::WorkbookProtected => write!(f, "workbook structure is protected"),
            Self::CustomViewsDisabledByTable => write!(f, "custom views cannot be used in workbooks that contain tables"),
            Self::NotFound => write!(f, "item not found"),
            Self::TrackingLocked => write!(f, "tracking is locked"),
            Self::ReadOnly => write!(f, "document is read-only"),
        }
    }
}

impl std::error::Error for EditError {}

impl Presentation {
    /// Apply `command` and return the command that undoes it.
    ///
    /// On error the presentation is unchanged.
    pub fn apply(&mut self, command: Command) -> Result<Command, EditError> {
        // A placeholder moved, resized or turned has its own place from then on.
        let moved: Vec<(SlideId, ShapeId)> = match &command {
            Command::SetFrame { slide, frames, with_attached: true } => frames.iter().map(|(shape, _)| (*slide, *shape)).collect(),
            Command::SetTransform { slide, shape, .. } => vec![(*slide, *shape)],
            _ => Vec::new(),
        };
        let own: Vec<Command> = moved
            .into_iter()
            .filter(|(slide, shape)| self.slide(*slide).and_then(|s| s.placeholders.get(shape)).is_some_and(|p| !p.own_frame))
            .map(|(slide, shape)| Command::SetPlaceholderOwnFrame { slide, shape, own: true })
            .collect();
        if !own.is_empty() {
            let mut all = vec![command];
            all.extend(own);
            return self.apply(Command::Batch(all));
        }
        // A worksheet's page range reaches the shapes on it (sheet_command.rs `used_extents`).
        let on_sheet = |p: &Self, slide: SlideId| p.workbook.is_some() && p.slide(slide).is_some_and(|s| s.shapes.iter().any(|s| matches!(s.kind, ShapeKind::Sheet(_))));
        if let Command::InsertShape { slide, .. }
        | Command::DeleteShape { slide, .. }
        | Command::SetFrame { slide, .. }
        | Command::SetTransform { slide, .. } = &command
            && on_sheet(self, *slide)
        {
            return crate::sheet_command::with_printed_extent(self, |p| p.apply_one(command));
        }
        self.apply_one(command)
    }

    fn apply_one(&mut self, command: Command) -> Result<Command, EditError> {
        if !self.sections.is_empty() && matches!(&command,Command::InsertSlide {..}|Command::DeleteSlide {..}|Command::MoveSlide {..}) {
            let sections=self.sections.clone();let inverse=self.apply_primitive(command)?;self.reconcile_sections();
            return Ok(Command::SlideStructure {command:Box::new(inverse),sections});
        }
        let structural=match &command {Command::InsertShape {slide,..}|Command::DeleteShape {slide,..}=>Some(*slide),_=>None};
        if let Some(slide)=structural {if let Some(groups)=self.slide(slide).filter(|s|!s.groups.is_empty()).map(|s|s.groups.clone()) {
            let inverse=self.apply_primitive(command)?;let target=self.slide_mut(slide).expect("structural edit retained slide");target.groups=target.live_groups(&groups);
            return Ok(Command::ShapeStructure {slide,command:Box::new(inverse),groups});
        }}
        self.apply_primitive(command)
    }

    fn apply_primitive(&mut self, command: Command) -> Result<Command, EditError> {
        match command {
            Command::ShapeStructure {slide,command,groups} => {
                let target=match &*command {Command::InsertShape {slide,..}|Command::DeleteShape {slide,..}=>Some(*slide),_=>None};
                if target!=Some(slide) {return Err(EditError::InvalidShapeGroups);}
                let inverse=self.apply_primitive(*command)?;
                if !self.slide(slide).is_some_and(|s|s.valid_groups(&groups)) {let _=self.apply_primitive(inverse);return Err(EditError::InvalidShapeGroups);}
                let previous=std::mem::replace(&mut self.slide_mut(slide).expect("checked").groups,groups);Ok(Command::ShapeStructure {slide,command:Box::new(inverse),groups:previous})
            }
            Command::SlideStructure {command,sections} => {
                if !matches!(&*command,Command::InsertSlide {..}|Command::DeleteSlide {..}|Command::MoveSlide {..}) {return Err(EditError::InvalidSections);}
                let inverse=self.apply_primitive(*command)?;
                if !self.valid_sections(&sections) {let _=self.apply_primitive(inverse);return Err(EditError::InvalidSections);}
                let previous=std::mem::replace(&mut self.sections,sections);
                Ok(Command::SlideStructure {command:Box::new(inverse),sections:previous})
            }
            Command::InsertSlide { index, slide } => {
                if index > self.slides.len() {
                    return Err(EditError::IndexOutOfRange);
                }
                if self.slide(slide.id).is_some() {
                    return Err(EditError::DuplicateId);
                }
                let id = slide.id;
                self.slides.insert(index, slide);
                self.renumber_slides();
                Ok(Command::DeleteSlide { slide: id })
            }
            Command::DeleteSlide { slide } => {
                let index = self.slide_index(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if self.slides.len() == 1 {
                    return Err(EditError::LastSlide);
                }
                let slide = self.slides.remove(index);
                self.renumber_slides();
                Ok(Command::InsertSlide { index, slide })
            }
            Command::InsertShape { slide, shapes } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                // Ascending indices, each where it is after the ones before are in.
                let mut expected = s.shapes.len();
                for (i, (index, shape)) in shapes.iter().enumerate() {
                    expected += 1;
                    if *index >= expected || s.shape(shape.id).is_some() || shapes[..i].iter().any(|(_, x)| x.id == shape.id) {
                        return Err(if s.shape(shape.id).is_some() { EditError::DuplicateId } else { EditError::IndexOutOfRange });
                    }
                }
                let ids = shapes.iter().map(|(_, x)| x.id).collect();
                for (index, shape) in shapes {
                    s.shapes.insert(index, shape);
                }
                Ok(Command::DeleteShape { slide, shapes: ids, with_attached: false })
            }
            Command::DeleteShape { slide, shapes, with_attached } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let mut ids = shapes;
                if with_attached {
                    // A shape that is attached goes with its owner, not alone; the ones attached to a shape named go with it.
                    for id in &ids {
                        if s.is_attached(*id) && !ids.iter().any(|o| s.attached_to(*o).contains(id)) {
                            return Err(EditError::Attached(*id));
                        }
                    }
                    let mut more = Vec::new();
                    for id in &ids {
                        for a in s.attached_to(*id) {
                            if !ids.contains(&a) && !more.contains(&a) {
                                more.push(a);
                            }
                        }
                    }
                    ids.extend(more);
                }
                let mut indices: Vec<usize> = ids.iter().map(|id| s.shape_index(*id).ok_or(EditError::NoSuchShape(*id))).collect::<Result<_, _>>()?;
                indices.sort_unstable();
                indices.dedup();
                let mut removed: Vec<(usize, Shape)> = Vec::with_capacity(indices.len());
                for &i in indices.iter().rev() {
                    removed.push((i, s.shapes.remove(i)));
                }
                removed.reverse();
                Ok(Command::InsertShape { slide, shapes: removed })
            }
            Command::SetFrame { slide, frames, with_attached } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if let Some((missing, _)) = frames.iter().find(|(id, _)| s.shape(*id).is_none()) {
                    return Err(EditError::NoSuchShape(*missing));
                }
                if with_attached {
                    // A shape that is attached follows its owner, and is not moved alone.
                    for (id, _) in &frames {
                        if s.is_attached(*id) && !frames.iter().any(|(o, _)| s.attached_to(*o).contains(id)) {
                            return Err(EditError::Attached(*id));
                        }
                    }
                }
                let mut old: Vec<(ShapeId, EmuRect)> = Vec::with_capacity(frames.len());
                let mut follow_ups: Vec<(ShapeId, EmuRect, EmuRect, EmuRect)> = Vec::new();
                for (id, frame) in &frames {
                    let target = s.shapes.iter_mut().find(|x| x.id == *id).expect("checked");
                    let was = std::mem::replace(&mut target.frame, *frame);
                    old.push((*id, was));
                    if with_attached {
                        for a in s.attached_to(*id) {
                            if !frames.iter().any(|(named, _)| *named == a) {
                                follow_ups.push((a, was, *frame, EmuRect::default()));
                            }
                        }
                    }
                }
                // Attached shapes keep their place in the frame.
                for (id, from, to, _) in follow_ups {
                    let Some(a) = s.shapes.iter_mut().find(|x| x.id == id) else { continue };
                    old.push((id, a.frame));
                    a.frame = follow(a.frame, from, to);
                }
                Ok(Command::SetFrame { slide, frames: old, with_attached: false })
            }
            Command::Batch(commands) => {
                let mut inverses = Vec::with_capacity(commands.len());
                for command in commands {
                    match self.apply_one(command) {
                        Ok(inverse) => inverses.push(inverse),
                        Err(err) => {
                            for inverse in inverses.into_iter().rev() {
                                let _ = self.apply_one(inverse);
                            }
                            return Err(err);
                        }
                    }
                }
                inverses.reverse();
                Ok(Command::Batch(inverses))
            }
            Command::MoveSlide { slide, index } => {
                let from = self.slide_index(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if index >= self.slides.len() {
                    return Err(EditError::IndexOutOfRange);
                }
                let s = self.slides.remove(from);
                self.slides.insert(index, s);
                self.renumber_slides();
                Ok(Command::MoveSlide { slide, index: from })
            }
            Command::MoveShape { slide, shape, index } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let from = s.shape_index(shape).ok_or(EditError::NoSuchShape(shape))?;
                if s.is_attached(shape) {
                    return Err(EditError::Attached(shape));
                }
                // The shape and those attached to it right after it move as one.
                let attached = s.attached_to(shape);
                let count = 1 + s.shapes[from + 1..].iter().take_while(|x| attached.contains(&x.id)).count();
                if index + count > s.shapes.len() {
                    return Err(EditError::IndexOutOfRange);
                }
                let block: Vec<Shape> = s.shapes.drain(from..from + count).collect();
                for (i, x) in block.into_iter().enumerate() {
                    s.shapes.insert(index + i, x);
                }
                Ok(Command::MoveShape { slide, shape, index: from })
            }
            Command::SetTransform { slide, shape, rotation, flip_h, flip_v } => {
                let s = self.slide(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.is_attached(shape) || !s.attached_to(shape).is_empty() {
                    return Err(EditError::Attached(shape));
                }
                let target = self.shape_mut(slide, shape)?;
                let old = Command::SetTransform { slide, shape: target.id, rotation: target.rotation, flip_h: target.flip_h, flip_v: target.flip_v };
                target.rotation = rotation.rem_euclid(21_600_000);
                target.flip_h = flip_h;
                target.flip_v = flip_v;
                Ok(old)
            }
            Command::SetFill { slide, shape, fill } => {
                let target = self.shape_mut(slide, shape)?;
                match &mut target.kind {
                    ShapeKind::Auto(auto) => {
                        let old = std::mem::replace(&mut auto.fill, fill);
                        Ok(Command::SetFill { slide, shape, fill: old })
                    }
                    ShapeKind::TextBox(body) => {
                        // A filled text box is a rectangle with text, as in PowerPoint.
                        let body = body.clone();
                        let old = target.kind.clone();
                        target.kind = ShapeKind::Auto(AutoShape::text_box(fill, body));
                        Ok(Command::SetShapeKind { slide, shape, kind: Box::new(old) })
                    }
                    _ => Err(EditError::NotApplicable(shape)),
                }
            }
            Command::SetStroke { slide, shape, stroke } => {
                let target = self.shape_mut(slide, shape)?;
                match &mut target.kind {
                    ShapeKind::Auto(auto) => {
                        let old = std::mem::replace(&mut auto.stroke, stroke);
                        Ok(Command::SetStroke { slide, shape, stroke: old })
                    }
                    ShapeKind::Picture(picture) => {
                        let old = std::mem::replace(&mut picture.stroke, stroke);
                        Ok(Command::SetStroke { slide, shape, stroke: old })
                    }
                    ShapeKind::Line(line) => {
                        let new = stroke.ok_or(EditError::NotApplicable(shape))?;
                        let old = std::mem::replace(line, new);
                        Ok(Command::SetStroke { slide, shape, stroke: Some(old) })
                    }
                    ShapeKind::TextBox(body) => {
                        let Some(stroke) = stroke else { return Ok(Command::SetStroke { slide, shape, stroke: None }) };
                        let body = body.clone();
                        let old = target.kind.clone();
                        let mut auto = AutoShape::text_box(Fill::None, body);
                        auto.stroke = Some(stroke);
                        target.kind = ShapeKind::Auto(auto);
                        Ok(Command::SetShapeKind { slide, shape, kind: Box::new(old) })
                    }
                    _ => Err(EditError::NotApplicable(shape)),
                }
            }
            Command::SetShapeKind { slide, shape, kind } => {
                let target = self.shape_mut(slide, shape)?;
                let old = std::mem::replace(&mut target.kind, *kind);
                Ok(Command::SetShapeKind { slide, shape, kind: Box::new(old) })
            }
            Command::SetText { slide, shape, text } => {
                let body = self.shape_mut(slide, shape)?.text_body_mut().ok_or(EditError::NotText(shape))?;
                let old = body.clone();
                body.set_text(&text);
                Ok(Command::SetTextBody { slide, shape, body: old })
            }
            Command::SetTextStyle { slide, shape, style } => {
                let body = self.shape_mut(slide, shape)?.text_body_mut().ok_or(EditError::NotText(shape))?;
                let old = body.clone();
                body.apply_style(style);
                Ok(Command::SetTextBody { slide, shape, body: old })
            }
            Command::ReplaceText { slide, shape, at, remove, insert } => {
                let body = self.shape_mut(slide, shape)?.text_body_mut().ok_or(EditError::NotText(shape))?;
                let old = body.clone();
                body.replace_text(at, remove, &insert);
                Ok(Command::SetTextBody { slide, shape, body: old })
            }
            Command::ReplaceTextRange { slide, shape, at, remove, insert } => {
                let body = self.shape_mut(slide, shape)?.text_body_mut().ok_or(EditError::NotText(shape))?;
                let old = body.clone();
                body.splice(at, remove, &insert);
                Ok(Command::SetTextBody { slide, shape, body: old })
            }
            Command::SetCharacterFormat { slide, shape, start, end, format } => {
                let body = self.shape_mut(slide, shape)?.text_body_mut().ok_or(EditError::NotText(shape))?;
                let old = body.clone();
                body.format_runs(start, end, &format);
                Ok(Command::SetTextBody { slide, shape, body: old })
            }
            Command::SetParagraphFormat { slide, shape, start, end, format } => {
                let body = self.shape_mut(slide, shape)?.text_body_mut().ok_or(EditError::NotText(shape))?;
                let old = body.clone();
                body.format_paragraphs(start, end, &format);
                Ok(Command::SetTextBody { slide, shape, body: old })
            }
            Command::SetPlaceholderOwnFrame { slide, shape, own } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let ph = s.placeholders.get_mut(&shape).ok_or(EditError::NoSuchShape(shape))?;
                let old = std::mem::replace(&mut ph.own_frame, own);
                Ok(Command::SetPlaceholderOwnFrame { slide, shape, own: old })
            }
            Command::SetSlide { slide } => {
                let index = self.slide_index(slide.id).ok_or(EditError::NoSuchSlide(slide.id))?;
                let old = std::mem::replace(&mut self.slides[index], slide);
                self.renumber_slides();
                Ok(Command::SetSlide { slide: old })
            }
            Command::SetShapeGroups {slide,groups} => {
                if self.flow.is_some() {return Err(EditError::InvalidShapeGroups);}
                let target=self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if !target.valid_groups(&groups) {return Err(EditError::InvalidShapeGroups);}
                let old=std::mem::replace(&mut target.groups,groups);Ok(Command::SetShapeGroups {slide,groups:old})
            }
            Command::SetSections {sections} => {
                if self.flow.is_some() || self.workbook.is_some() || !self.valid_sections(&sections) {return Err(EditError::InvalidSections);}
                let previous=std::mem::replace(&mut self.sections,sections);Ok(Command::SetSections {sections:previous})
            }
            Command::SetSlideSize { size } => {
                // PresentationML ST_SlideSizeCoordinate: 1–56 inches in EMU.
                if !(914_400..=51_206_400).contains(&size.width) || !(914_400..=51_206_400).contains(&size.height) {
                    return Err(EditError::InvalidSlideSize);
                }
                let old = std::mem::replace(&mut self.slide_size, size);
                Ok(Command::SetSlideSize { size: old })
            }
            Command::RenameMaster { master, name } => {
                let m = self.masters.iter_mut().find(|m| m.id == master).ok_or(EditError::NotFound)?;
                let old = std::mem::replace(&mut m.name, name);
                Ok(Command::RenameMaster { master, name: old })
            }
            Command::InsertMaster { index, master } => {
                if self.masters.iter().any(|m| m.id == master.id) {
                    return Err(EditError::DuplicateId);
                }
                let id = master.id;
                let index = index.min(self.masters.len());
                self.masters.insert(index, master);
                Ok(Command::DeleteMaster { master: id })
            }
            Command::DeleteMaster { master } => {
                if self.layouts.iter().any(|l| l.master == Some(master)) {
                    return Err(EditError::LayoutInUse);
                }
                let index = self.masters.iter().position(|m| m.id == master).ok_or(EditError::NotFound)?;
                let removed = self.masters.remove(index);
                Ok(Command::InsertMaster { index, master: removed })
            }
            Command::RenameLayout { layout, name } => {
                let l = self.layouts.iter_mut().find(|l| l.id == layout).ok_or(EditError::NotFound)?;
                let old = std::mem::replace(&mut l.name, name);
                Ok(Command::RenameLayout { layout, name: old })
            }
            Command::InsertLayout { index, layout } => {
                if self.layouts.iter().any(|l| l.id == layout.id) {
                    return Err(EditError::DuplicateId);
                }
                let id = layout.id;
                let index = index.min(self.layouts.len());
                self.layouts.insert(index, layout);
                Ok(Command::DeleteLayout { layout: id })
            }
            Command::DeleteLayout { layout } => {
                if self.slides.iter().any(|s| s.layout == Some(layout)) {
                    return Err(EditError::LayoutInUse);
                }
                let index = self.layouts.iter().position(|l| l.id == layout).ok_or(EditError::NotFound)?;
                let removed = self.layouts.remove(index);
                Ok(Command::InsertLayout { index, layout: removed })
            }
            Command::SetLayouts { layouts, scale } => {
                let layouts = std::mem::replace(&mut self.layouts, layouts);
                let scale = std::mem::replace(&mut self.layout_scale, scale);
                Ok(Command::SetLayouts { layouts, scale })
            }
            Command::SetTheme { colors, fonts } => {
                let old_colors = std::mem::replace(&mut self.theme_colors, colors.clone());
                let old_fonts = std::mem::replace(&mut self.theme_fonts, fonts.clone());
                if colors != old_colors {
                    self.resolve_theme_colors();
                }
                if let Some(flow_arc) = &mut self.flow
                    && colors != old_colors
                {
                    let flow = std::sync::Arc::make_mut(flow_arc);
                    flow.retheme(&colors);
                }
                if let Some(workbook) = &mut self.workbook {
                    let wb = std::sync::Arc::make_mut(workbook);
                    wb.theme_colors = colors.clone();
                    wb.theme_fonts = fonts.clone();
                    if let Some(style) = &mut wb.default_style {
                        style.retheme(&colors);
                    }
                    for custom in &mut wb.custom_cell_styles {
                        custom.format.retheme(&colors);
                    }
                    for ws in &mut wb.sheets {
                        if let Some(sheet_arc) = &mut ws.hidden {
                            let sheet = std::sync::Arc::make_mut(sheet_arc);
                            for style in &mut sheet.styles {
                                style.retheme(&colors);
                            }
                        }
                    }
                }
                for slide in &mut self.slides {
                    for shape in &mut slide.shapes {
                        if let crate::model::ShapeKind::Sheet(sheet_arc) = &mut shape.kind {
                            let sheet = std::sync::Arc::make_mut(sheet_arc);
                            for style in &mut sheet.styles {
                                style.retheme(&colors);
                            }
                        }
                    }
                }
                Ok(Command::SetTheme { colors: old_colors, fonts: old_fonts })
            }
            Command::SetThemeEffects { name } => {
                let old = std::mem::replace(&mut self.theme_effects, name);
                Ok(Command::SetThemeEffects { name: old })
            }
            Command::SetFontSubstitutions { substitutions } => {
                let old = std::mem::replace(&mut self.font_substitutions, substitutions);
                Ok(Command::SetFontSubstitutions { substitutions: old })
            }
            Command::SetEmbeddedFonts { fonts } => {
                let old = std::mem::replace(&mut self.fonts, fonts);
                Ok(Command::SetEmbeddedFonts { fonts: old })
            }
            Command::SetPhotoAlbum { album } => {
                let old = std::mem::replace(&mut self.photo_album, album);
                Ok(Command::SetPhotoAlbum { album: old })
            }
            Command::SetFirstSlideNumber { number } => {
                let old = std::mem::replace(&mut self.first_slide_number, number.min(9999));
                self.renumber_slides();
                Ok(Command::SetFirstSlideNumber { number: old })
            }
            Command::SetNotesMarks { marks } => {
                let old = std::mem::replace(&mut self.notes_marks, marks);
                Ok(Command::SetNotesMarks { marks: old })
            }
            Command::SetHeaderFooterMarks { marks } => {
                let old = std::mem::replace(&mut self.header_footer, marks);
                Ok(Command::SetHeaderFooterMarks { marks: old })
            }
            Command::SetShowProperties { properties } => {
                let old = std::mem::replace(&mut self.show_properties, properties);
                Ok(Command::SetShowProperties { properties: old })
            }
            Command::SetCustomShows { custom_shows } => {
                let old = std::mem::replace(&mut self.custom_shows, custom_shows);
                Ok(Command::SetCustomShows { custom_shows: old })
            }
            Command::SetMarkAsFinal { mark_as_final } => {
                let old = std::mem::replace(&mut self.mark_as_final, mark_as_final);
                Ok(Command::SetMarkAsFinal { mark_as_final: old })
            }
            Command::SetCmykMode { cmyk } => {
                let old = std::mem::replace(&mut self.cmyk, cmyk);
                Ok(Command::SetCmykMode { cmyk: old })
            }
            Command::SetExtendedFeatures { enabled } => {
                let old = std::mem::replace(&mut self.ibis_extensions, enabled);
                Ok(Command::SetExtendedFeatures { enabled: old })
            }
            Command::SetReadOnlyRecommended { read_only_recommended } => {
                let old = std::mem::replace(&mut self.read_only_recommended, read_only_recommended);
                Ok(Command::SetReadOnlyRecommended { read_only_recommended: old })
            }
            Command::SetModifyVerifier { verifier } => {
                let old = std::mem::replace(&mut self.modify_verifier, verifier);
                Ok(Command::SetModifyVerifier { verifier: old })
            }
            Command::SetDocumentProperties { properties } => {
                let old = std::mem::replace(&mut self.properties, properties);
                Ok(Command::SetDocumentProperties { properties: old })
            }
            Command::SetObjectDefaults { defaults } => {
                let old = std::mem::replace(&mut self.object_defaults, defaults);
                Ok(Command::SetObjectDefaults { defaults: old })
            }
            Command::SetBackground { slide, fill } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let old = std::mem::replace(&mut s.background, fill);
                Ok(Command::SetBackground { slide, fill: old })
            }
            Command::SetAspectLocked { slide, shape, locked } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let was = if locked { !s.aspect_locked.insert(shape) } else { s.aspect_locked.remove(&shape) };
                Ok(Command::SetAspectLocked { slide, shape, locked: was })
            }
            Command::SetShapeLink { slide, shape, link } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = match link {
                    Some(l) => s.shape_links.insert(shape, l),
                    None => s.shape_links.remove(&shape),
                };
                Ok(Command::SetShapeLink { slide, shape, link: old })
            }
            Command::SetShapeLinkTooltip { slide, shape, tooltip } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = match tooltip {
                    Some(t) if !t.trim().is_empty() => s.shape_link_tooltips.insert(shape, t.trim().to_string()),
                    _ => s.shape_link_tooltips.remove(&shape),
                };
                Ok(Command::SetShapeLinkTooltip { slide, shape, tooltip: old })
            }
            Command::SetShapeName { slide, shape, name } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = if name.is_empty() { s.shape_names.remove(&shape) } else { s.shape_names.insert(shape, name) };
                Ok(Command::SetShapeName { slide, shape, name: old.unwrap_or_default() })
            }
            Command::SetShapeAltText { slide, shape, alt_text } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = if alt_text.title.is_empty() && alt_text.description.is_empty() {
                    s.shape_alt_texts.remove(&shape)
                } else {
                    s.shape_alt_texts.insert(shape, alt_text)
                };
                Ok(Command::SetShapeAltText { slide, shape, alt_text: old.unwrap_or_default() })
            }
            Command::SetObjectWrap { slide, shape, wrap } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = if wrap.is_none() { s.ibis_wrap.remove(&shape) } else { s.ibis_wrap.insert(shape, wrap) };
                Ok(Command::SetObjectWrap { slide, shape, wrap: old.unwrap_or_default() })
            }
            Command::SetTextChains { slide, chains, bodies } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                for (shape, _) in &bodies {
                    if s.shape(*shape).and_then(|sh| sh.text_body()).is_none() {
                        return Err(EditError::NoSuchShape(*shape));
                    }
                }
                let old_chains = std::mem::replace(&mut s.text_chains, chains);
                let mut old_bodies = Vec::new();
                for (shape, paragraphs) in bodies {
                    if let Some(body) = s.shapes.iter_mut().find(|sh| sh.id == shape).and_then(|sh| sh.text_body_mut()) {
                        old_bodies.push((shape, std::mem::replace(&mut body.paragraphs, paragraphs)));
                    }
                }
                Ok(Command::SetTextChains { slide, chains: old_chains, bodies: old_bodies })
            }
            Command::SetWrapIgnore { slide, shape, ignore } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let was = if ignore { !s.ibis_wrap_ignore.insert(shape) } else { s.ibis_wrap_ignore.remove(&shape) };
                Ok(Command::SetWrapIgnore { slide, shape, ignore: was })
            }
            Command::SetShapeDecorative { slide, shape, decorative } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let was = if decorative {
                    !s.decorative_shapes.insert(shape)
                } else {
                    s.decorative_shapes.remove(&shape)
                };
                if let Some(sh) = s.shapes.iter_mut().find(|sh| sh.id == shape) {
                    if let crate::model::ShapeKind::Picture(pic) = &mut sh.kind {
                        pic.decorative = decorative;
                    }
                }
                Ok(Command::SetShapeDecorative { slide, shape, decorative: was })
            }
            Command::SetShapePlacement { slide, shape, placement } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = s.shape_placements.insert(shape, placement).unwrap_or_default();
                Ok(Command::SetShapePlacement { slide, shape, placement: old })
            }
            Command::SetShapePrint { slide, shape, print } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = s.shape_prints.insert(shape, print).unwrap_or(true);
                Ok(Command::SetShapePrint { slide, shape, print: old })
            }
            Command::SetShapeFormula { slide, shape, formula } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = match formula {
                    Some(f) => s.shape_formulas.insert(shape, f),
                    None => s.shape_formulas.remove(&shape),
                };
                Ok(Command::SetShapeFormula { slide, shape, formula: old })
            }
            Command::SetZoomObject { slide, shape, zoom } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = match zoom {
                    Some(z) => s.zoom_objects.insert(shape, z),
                    None => s.zoom_objects.remove(&shape),
                };
                Ok(Command::SetZoomObject { slide, shape, zoom: old })
            }
            Command::SetShapeReturnToZoom { slide, shape, return_to_zoom } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let was = if return_to_zoom {
                    !s.shape_return_to_zoom.insert(shape)
                } else {
                    s.shape_return_to_zoom.remove(&shape)
                };
                Ok(Command::SetShapeReturnToZoom { slide, shape, return_to_zoom: was })
            }
            Command::SetShapeZoomShowBackground { slide, shape, show_bg } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = match show_bg {
                    Some(bg) => s.shape_zoom_show_bg.insert(shape, bg),
                    None => s.shape_zoom_show_bg.remove(&shape),
                };
                Ok(Command::SetShapeZoomShowBackground { slide, shape, show_bg: old })
            }
            Command::SetShapeZoomDuration { slide, shape, duration_ms } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = match duration_ms {
                    Some(d) => s.shape_zoom_durations.insert(shape, d),
                    None => s.shape_zoom_durations.remove(&shape),
                };
                Ok(Command::SetShapeZoomDuration { slide, shape, duration_ms: old })
            }
            Command::SetShapeGlue { slide, shape, glue } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = match glue {
                    Some(g) => s.shape_glues.insert(shape, g),
                    None => s.shape_glues.remove(&shape),
                };
                Ok(Command::SetShapeGlue { slide, shape, glue: old })
            }
            Command::SetConnectorFrame { slide, shape, frame, flip_h, flip_v } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let target = s.shapes.iter_mut().find(|x| x.id == shape).ok_or(EditError::NoSuchShape(shape))?;
                let old_frame = target.frame;
                let old_flip_h = target.flip_h;
                let old_flip_v = target.flip_v;
                target.frame = frame;
                target.flip_h = flip_h;
                target.flip_v = flip_v;
                Ok(Command::SetConnectorFrame { slide, shape, frame: old_frame, flip_h: old_flip_h, flip_v: old_flip_v })
            }
            Command::SetGuides { slide, guides } => match slide {
                None => Ok(Command::SetGuides { slide: None, guides: std::mem::replace(&mut self.guides, guides) }),
                Some(id) => {
                    let s = self.slide_mut(id).ok_or(EditError::NoSuchSlide(id))?;
                    Ok(Command::SetGuides { slide, guides: std::mem::replace(&mut s.guides, guides) })
                }
            },
            Command::SetBackgroundGraphicsHidden { slide, hidden } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let was = std::mem::replace(&mut s.hide_backdrop, hidden);
                Ok(Command::SetBackgroundGraphicsHidden { slide, hidden: was })
            }
            Command::SetShapeHidden { slide, shape, hidden } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let was = s.hidden_shapes.contains(&shape);
                if hidden {
                    s.hidden_shapes.insert(shape);
                } else {
                    s.hidden_shapes.remove(&shape);
                }
                Ok(Command::SetShapeHidden { slide, shape, hidden: was })
            }
            Command::SetNotes { slide, notes } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let old = std::mem::replace(&mut s.notes, notes);
                Ok(Command::SetNotes { slide, notes: old })
            }
            Command::SetSlideName { slide, name } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let old = std::mem::replace(&mut s.name, name);
                Ok(Command::SetSlideName { slide, name: old })
            }
            Command::SetPlaceholder { slide, shape, placeholder } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let old = match placeholder {
                    Some(ph) => s.placeholders.insert(shape, ph),
                    None => s.placeholders.remove(&shape),
                };
                Ok(Command::SetPlaceholder { slide, shape, placeholder: old })
            }
            Command::SetTransition { slide, transition } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let old = std::mem::replace(&mut s.transition, transition);
                Ok(Command::SetTransition { slide, transition: old })
            }
            Command::SetAnimations { slide, animations } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let old = std::mem::replace(&mut s.animations, animations);
                Ok(Command::SetAnimations { slide, animations: old })
            }
            Command::SetComments { slide, comments } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let old = std::mem::replace(&mut s.comments, comments);
                Ok(Command::SetComments { slide, comments: old })
            }
            Command::Sheet(command) => crate::sheet_command::apply(self, command),
            Command::Flow(command) => crate::flow_command::apply(self, command),
            Command::SetTextBody { slide, shape, body } => {
                let target = self.shape_mut(slide, shape)?.text_body_mut().ok_or(EditError::NotText(shape))?;
                let old = std::mem::replace(target, body);
                Ok(Command::SetTextBody { slide, shape, body: old })
            }
            Command::InCell { row, column, command } => {
                let (slide, shape) = match &*command {
                    Command::SetText { slide, shape, .. }
                    | Command::SetTextStyle { slide, shape, .. }
                    | Command::ReplaceTextRange { slide, shape, .. }
                    | Command::ReplaceText { slide, shape, .. }
                    | Command::SetCharacterFormat { slide, shape, .. }
                    | Command::SetParagraphFormat { slide, shape, .. }
                    | Command::SetTextBody { slide, shape, .. } => (*slide, *shape),
                    _ => return Err(EditError::LayoutInUse),
                };
                let target = self.shape_mut(slide, shape)?;
                let ShapeKind::Table(table) = &mut target.kind else { return Err(EditError::NotApplicable(shape)) };
                let body = &mut table.rows.get_mut(row).and_then(|r| r.cells.get_mut(column)).ok_or(EditError::IndexOutOfRange)?.text;
                let old = body.clone();
                match *command {
                    Command::SetText { text, .. } => body.set_text(&text),
                    Command::SetTextStyle { style, .. } => body.apply_style(style),
                    Command::ReplaceTextRange { at, remove, insert, .. } => body.splice(at, remove, &insert),
                    Command::ReplaceText { at, remove, insert, .. } => body.replace_text(at, remove, &insert),
                    Command::SetCharacterFormat { start, end, format, .. } => body.format_runs(start, end, &format),
                    Command::SetParagraphFormat { start, end, format, .. } => body.format_paragraphs(start, end, &format),
                    Command::SetTextBody { body: new, .. } => *body = new,
                    _ => unreachable!("a text command"),
                }
                Ok(Command::InCell { row, column, command: Box::new(Command::SetTextBody { slide, shape, body: old }) })
            }
        }
    }

    /// A shape to edit; one attached to another is edited with it only.
    fn shape_mut(&mut self, slide: SlideId, shape: ShapeId) -> Result<&mut Shape, EditError> {
        let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
        if s.is_attached(shape) {
            return Err(EditError::Attached(shape));
        }
        s.shapes
            .iter_mut()
            .find(|s| s.id == shape)
            .ok_or(EditError::NoSuchShape(shape))
    }
}

/// `frame` of a shape attached to one whose frame goes from `from` to `to`:
/// at the same place relative to it.
fn follow(frame: EmuRect, from: EmuRect, to: EmuRect) -> EmuRect {
    let sx = if from.width == 0 { 1.0 } else { to.width as f64 / from.width as f64 };
    let sy = if from.height == 0 { 1.0 } else { to.height as f64 / from.height as f64 };
    let x = |v: i64| (to.x as f64 + (v - from.x) as f64 * sx).round() as i64;
    let y = |v: i64| (to.y as f64 + (v - from.y) as f64 * sy).round() as i64;
    let (left, top) = (x(frame.x), y(frame.y));
    EmuRect::new(left, top, (x(frame.right()) - left).max(0), (y(frame.bottom()) - top).max(0))
}

/// A presentation with undo and redo.
///
/// Each [`Document::execute`] call is one undo step, however many commands it
/// holds, so "insert a text box and type into it" can be undone at once.
#[derive(Clone, Debug)]
pub struct Document {
    presentation: Presentation,
    read_only: bool,
    undo: Vec<Vec<Command>>,
    redo: Vec<Vec<Command>>,
    /// Last successful worksheet edit, before undo inversion, for Repeat (F4).
    repeat_sheet: Vec<crate::sheet_command::SheetCommand>,
    repeat_sheet_group: bool,
    /// The `undo.len()` at which the document was opened or last saved (`None` if that state is no
    /// longer reachable on the undo/redo stack, or if mutated outside undo history).
    clean_undo_len: Option<usize>,
    /// The file this document is saved as (its whole path), for what the calculation says of it
    /// (`CELL("filename")`).
    file: Option<String>,
    /// A touch gesture that can still be taken back (`begin_gesture`).
    gesture: Option<GestureState>,
    gesture_serial: u64,
}

/// What `Document` keeps while a gesture is open, to put the history back at `rollback_gesture`.
#[derive(Clone, Debug)]
struct GestureState {
    serial: u64,
    /// The undo steps below this many existed before the gesture: they are not the gesture's.
    floor: usize,
    /// The undo steps the gesture took away from below `floor` (`retract_last`): each as the commands that put it
    /// back, the last retracted last.
    retracted: Vec<Vec<Command>>,
    /// The redo steps an edit of the gesture dropped (an edit clears the redo list), to put back.
    saved_redo: Option<Vec<Vec<Command>>>,
    clean_undo_len: Option<usize>,
    repeat_sheet: Vec<crate::sheet_command::SheetCommand>,
    repeat_sheet_group: bool,
}

/// A gesture opened by [`Document::begin_gesture`]; give it back to `commit_gesture` or `rollback_gesture`.
#[derive(Debug)]
#[must_use = "a gesture is ended with commit_gesture or rollback_gesture"]
pub struct GestureMark {
    serial: u64,
}

impl Default for Document {
    fn default() -> Self {
        Self::new(Presentation::default())
    }
}

impl Document {
    pub fn new(presentation: Presentation) -> Self {
        Self { presentation, read_only: false, undo: Vec::new(), redo: Vec::new(), repeat_sheet:Vec::new(), repeat_sheet_group:false, clean_undo_len: Some(0), file: None, gesture: None, gesture_serial: 0 }
    }

    /// Prevent document-changing commands for read-only import previews.
    pub fn set_read_only(&mut self, read_only: bool) { self.read_only = read_only; }

    /// Mark the current state of the document as saved (clean).
    pub fn mark_clean(&mut self) {
        self.clean_undo_len = Some(self.undo.len());
    }

    /// Clear all undo/redo history and mark the document as clean.
    /// Used when initializing a newly created document or template instance.
    pub fn reset_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.repeat_sheet.clear();
        self.repeat_sheet_group = false;
        self.clean_undo_len = Some(0);
        self.gesture = None;
    }

    /// Mark the document as having unsaved changes.
    pub fn mark_dirty(&mut self) {
        self.clean_undo_len = None;
    }

    /// Whether the document has unsaved changes since it was opened or last saved.
    pub fn is_dirty(&self) -> bool {
        self.clean_undo_len != Some(self.undo.len())
    }

    /// Tell the document which file it is (`None`: not saved yet).
    pub fn set_file(&mut self, file: Option<String>) {
        self.file = file;
    }

    pub fn presentation(&self) -> &Presentation {
        &self.presentation
    }

    pub fn presentation_mut(&mut self) -> &mut Presentation {
        &mut self.presentation
    }

    /// For id allocation only; edits must go through [`Document::execute`].
    pub fn ids(&mut self) -> &mut Presentation {
        &mut self.presentation
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// How many undo steps there are (a mark to tell whether the document was edited since).
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Apply `commands` as one undo step. If any command fails, the ones
    /// before it are rolled back and the document is unchanged.
    pub fn execute(&mut self, commands: Vec<Command>) -> Result<(), EditError> {
        if self.read_only && commands.iter().any(|command| !matches!(command, Command::Flow(crate::flow_command::FlowCommand::SetViewMode(_)))) {
            return Err(EditError::ReadOnly);
        }
        let repeat:Vec<_> = commands.iter().filter_map(|command|match command {
            Command::Sheet(edit @ (crate::SheetCommand::SetFormat{..}|crate::SheetCommand::Insert{..}|crate::SheetCommand::Delete{..}|crate::SheetCommand::ClearFormats{..}|crate::SheetCommand::InsertOrDeleteCells{..})) => Some(edit.clone()),
            _ => None,
        }).collect();
        let inverses = self.apply_all(commands)?;
        if !inverses.is_empty() {
            if self.clean_undo_len.is_some_and(|clean| clean > self.undo.len()) {
                self.clean_undo_len = None;
            }
            self.undo.push(inverses);
            self.drop_redo();
            let same_target = self.repeat_sheet_group && !repeat.is_empty() && !self.repeat_sheet.is_empty() && repeat.iter().chain(&self.repeat_sheet).all(|edit|match (repeat.first(),edit) {
                (Some(crate::SheetCommand::SetFormat{sheet:a,ranges:x,..}),crate::SheetCommand::SetFormat{sheet:b,ranges:y,..}) => a==b && x==y,
                _ => false,
            });
            if same_target {self.repeat_sheet.extend(repeat);} else {self.repeat_sheet = repeat;}
        }
        Ok(())
    }

    pub fn repeat_sheet_commands(&self) -> &[crate::SheetCommand] { &self.repeat_sheet }
    pub fn begin_repeat_sheet_group(&mut self) { self.repeat_sheet.clear(); self.repeat_sheet_group=true; }
    pub fn end_repeat_sheet_group(&mut self) { self.repeat_sheet_group=false; }

    /// Apply `commands` as part of the most recent undo step, so that for
    /// example inserting a text box and typing its first text undo together.
    /// Starts a new step when there is none.
    pub fn amend_last(&mut self, commands: Vec<Command>) -> Result<(), EditError> {
        let mut inverses = self.apply_all(commands)?;
        if inverses.is_empty() {
            return Ok(());
        }
        if self.clean_undo_len.is_some_and(|clean| clean >= self.undo.len()) {
            self.clean_undo_len = None;
        }
        // A step from before an open gesture is not amended (the gesture could not take it back): a new step.
        let amend = self.gesture.as_ref().is_none_or(|g| self.undo.len() > g.floor);
        match self.undo.last_mut() {
            Some(step) if amend => {
                inverses.append(step);
                *step = inverses;
            }
            _ => self.undo.push(inverses),
        }
        self.drop_redo();
        Ok(())
    }

    /// Undo the most recent step without making it redoable, for an edit the
    /// user abandoned (such as a new text box left empty).
    pub fn retract_last(&mut self) -> bool {
        let Some(step) = self.undo.pop() else { return false };
        if self.clean_undo_len.is_some_and(|clean| clean > self.undo.len()) {
            self.clean_undo_len = None;
        }
        match self.apply_all(step) {
            Ok(back) => {
                // A step of before an open gesture goes: the gesture keeps what puts it back.
                if let Some(g) = self.gesture.as_mut().filter(|g| self.undo.len() < g.floor) {
                    g.floor = self.undo.len();
                    g.retracted.push(back);
                }
                true
            }
            Err(_) => false,
        }
    }

    pub fn undo(&mut self) -> bool {
        // The user's own Undo while a gesture is open: the history is theirs now, the gesture can no longer be taken back.
        self.gesture = None;
        let Some(step) = self.undo.pop() else { return false };
        match self.apply_all(step) {
            Ok(inverses) => {
                self.redo.push(inverses);
                true
            }
            Err(_) => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        self.gesture = None;
        let Some(step) = self.redo.pop() else { return false };
        match self.apply_all(step) {
            Ok(inverses) => {
                self.undo.push(inverses);
                true
            }
            Err(_) => false,
        }
    }

    /// An edit clears what could be redone; while a gesture is open the redo list is kept for `rollback_gesture`.
    fn drop_redo(&mut self) {
        if self.redo.is_empty() {
            return;
        }
        match self.gesture.as_mut() {
            Some(g) if g.saved_redo.is_none() => g.saved_redo = Some(std::mem::take(&mut self.redo)),
            _ => self.redo.clear(),
        }
    }

    /// Open a gesture: a user action (a touch from the finger going down until it leaves) whose edits are taken
    /// back together by [`rollback_gesture`](Self::rollback_gesture) or made one undo step by
    /// [`commit_gesture`](Self::commit_gesture). Every change of the document goes through `execute`, so the
    /// history is all there is to put back. A gesture still open is committed first.
    pub fn begin_gesture(&mut self) -> GestureMark {
        if self.gesture.is_some() {
            self.commit_open_gesture();
        }
        self.gesture_serial += 1;
        let serial = self.gesture_serial;
        self.gesture = Some(GestureState {
            serial,
            floor: self.undo.len(),
            retracted: Vec::new(),
            saved_redo: None,
            clean_undo_len: self.clean_undo_len,
            repeat_sheet: self.repeat_sheet.clone(),
            repeat_sheet_group: self.repeat_sheet_group,
        });
        GestureMark { serial }
    }

    /// Whether `mark` is the gesture that is open now.
    pub fn gesture_is_open(&self, mark: &GestureMark) -> bool {
        self.gesture.as_ref().is_some_and(|g| g.serial == mark.serial)
    }

    /// Take back everything the gesture changed: the undo steps it made are undone (not redoable), the steps it
    /// retracted are back, and the redo list and the saved/clean state are as they were. `false` if the gesture is
    /// no longer open (the user's own Undo or Redo ended it, or a newer gesture began).
    pub fn rollback_gesture(&mut self, mark: GestureMark) -> bool {
        if !self.gesture_is_open(&mark) {
            return false;
        }
        let Some(g) = self.gesture.take() else { return false };
        let mut ok = true;
        while self.undo.len() > g.floor {
            let Some(step) = self.undo.pop() else { break };
            ok &= self.apply_all(step).is_ok();
        }
        for back in g.retracted.into_iter().rev() {
            match self.apply_all(back) {
                Ok(inverses) => self.undo.push(inverses),
                Err(_) => ok = false,
            }
        }
        if let Some(redo) = g.saved_redo {
            self.redo = redo;
        }
        self.clean_undo_len = g.clean_undo_len;
        self.repeat_sheet = g.repeat_sheet;
        self.repeat_sheet_group = g.repeat_sheet_group;
        ok
    }

    /// Make the gesture final: the undo steps it made become one step (Undo once takes all of it back).
    pub fn commit_gesture(&mut self, mark: GestureMark) {
        if self.gesture_is_open(&mark) {
            self.commit_open_gesture();
        }
    }

    fn commit_open_gesture(&mut self) {
        let Some(g) = self.gesture.take() else { return };
        let made = self.undo.len().saturating_sub(g.floor);
        if made < 2 {
            return;
        }
        let old_len = self.undo.len();
        // The inverses of the later steps run first.
        let merged: Vec<Command> = self.undo.drain(g.floor..).rev().flatten().collect();
        self.undo.push(merged);
        self.clean_undo_len = match self.clean_undo_len {
            Some(clean) if clean == old_len => Some(self.undo.len()),
            Some(clean) if clean > g.floor => None,
            other => other,
        };
    }

    /// Apply in order; return inverses in the order that undoes them.
    fn apply_all(&mut self, commands: Vec<Command>) -> Result<Vec<Command>, EditError> {
        // (Calculated for this document: its file's name, not another open one's.)
        ibis_calc::eval::convert::set_workbook_file(self.file.clone());
        let mut inverses = Vec::with_capacity(commands.len());
        for command in commands {
            match self.presentation.apply(command) {
                Ok(inverse) => inverses.push(inverse),
                Err(err) => {
                    for inverse in inverses.into_iter().rev() {
                        let _ = self.presentation.apply(inverse);
                    }
                    return Err(err);
                }
            }
        }
        inverses.reverse();
        Ok(inverses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::EmuRect;

    fn doc_with_box() -> (Document, SlideId, ShapeId) {
        let mut doc = Document::new(Presentation::new());
        let slide = doc.presentation().slides[0].id;
        let shape = doc.ids().new_shape_id();
        doc.execute(vec![Command::InsertShape { slide, shapes: vec![(0, Shape::text_box(shape, EmuRect::new(0, 0, 100, 50), "こんにちは"))] }])
        .unwrap();
        (doc, slide, shape)
    }

    #[test]
    fn shape_group_hierarchy_undo_and_invalid_membership() {
        use crate::{ShapeGroup,GroupMember};let (mut doc,slide,shape)=doc_with_box();let id=doc.ids().new_shape_id();let mut second=doc.presentation().slides[0].shapes[0].clone();second.id=id;doc.execute(vec![Command::InsertShape { slide, shapes: vec![(1, second)] }]).unwrap();
        let before=doc.presentation().clone();let groups=vec![ShapeGroup {has_unmodelled_children:false,soft_edge:None,native_id:"99".into(),name:"outer".into(),children:vec![GroupMember::Group(Box::new(ShapeGroup {has_unmodelled_children:false,soft_edge:None,native_id:"100".into(),name:"inner".into(),children:vec![GroupMember::Shape(shape)]})),GroupMember::Shape(id)]}];doc.execute(vec![Command::SetShapeGroups {slide,groups:groups.clone()}]).unwrap();let grouped=doc.presentation().clone();assert!(grouped.slides[0].valid_groups(&groups));assert!(doc.undo());assert_eq!(doc.presentation(),&before);assert!(doc.redo());assert_eq!(doc.presentation(),&grouped);
        for kind in 0..4 {let mut invalid=groups.clone();match kind {0=>invalid[0].native_id="invalid".into(),1=>invalid[0].children.push(GroupMember::Shape(shape)),2=>invalid[0].children.push(GroupMember::Shape(ShapeId(u64::MAX))),3=>invalid[0].name="bad\0name".into(),_=>unreachable!()}assert_eq!(doc.execute(vec![Command::SetShapeGroups {slide,groups:invalid}]),Err(EditError::InvalidShapeGroups));assert_eq!(doc.presentation(),&grouped);}
        for command in [Command::DeleteShape { slide, shapes: vec![shape], with_attached: true },Command::DeleteShape { slide, shapes: vec![shape,id], with_attached: false }] {let before=doc.presentation().clone();doc.execute(vec![command]).unwrap();let after=doc.presentation().clone();assert!(after.slides[0].valid_groups(&after.slides[0].groups));assert!(doc.undo());assert_eq!(doc.presentation(),&before);assert!(doc.redo());assert_eq!(doc.presentation(),&after);assert!(doc.undo());assert_eq!(doc.presentation(),&before);}
        let mut duplicate=groups.clone();let GroupMember::Group(inner)=&mut duplicate[0].children[0] else {panic!("nested")};inner.native_id="99".into();assert_eq!(doc.execute(vec![Command::SetShapeGroups {slide,groups:duplicate}]),Err(EditError::InvalidShapeGroups));assert_eq!(doc.presentation(),&grouped);
        assert!(doc.execute(vec![Command::SetShapeGroups {slide,groups:vec![]},Command::DeleteShape { slide, shapes: vec![ShapeId(u64::MAX)], with_attached: true }]).is_err());assert_eq!(doc.presentation(),&grouped,"partial command failure restores hierarchy");
        let mut attached=grouped.clone();attached.slides[0].attached.insert(shape,vec![id]);attached.slides[0].groups=vec![ShapeGroup {has_unmodelled_children:false,soft_edge:None,native_id:"200".into(),name:"attached owner".into(),children:vec![GroupMember::Shape(shape)]}];let mut doc=Document::new(attached.clone());doc.execute(vec![Command::DeleteShape { slide, shapes: vec![shape], with_attached: true }]).unwrap();let removed=doc.presentation().clone();assert!(removed.slides[0].groups.is_empty());assert!(removed.slides[0].shapes.is_empty());assert!(doc.undo());assert_eq!(doc.presentation(),&attached);assert!(doc.redo());assert_eq!(doc.presentation(),&removed);

    }

    #[test]
    fn section_structure_undo_redo_and_validation() {
        use crate::Section;
        let mut p=Presentation::new();for _ in 0..2 {let id=p.new_slide_id();p.slides.push(Slide::new(id));}
        let ids=p.slides.iter().map(|s|s.id).collect::<Vec<_>>();let mut doc=Document::new(p);
        let sections=vec![Section {id:"{11111111-1111-1111-1111-111111111111}".into(),name:"first".into(),slides:ids[..2].to_vec()},Section {id:"{22222222-2222-2222-2222-222222222222}".into(),name:"second".into(),slides:ids[2..].to_vec()}];
        doc.execute(vec![Command::SetSections {sections:sections.clone()}]).unwrap();
        let fresh=doc.ids().new_slide_id();
        for command in [Command::MoveSlide {slide:ids[0],index:2},Command::InsertSlide {index:1,slide:Slide::new(fresh)},Command::DeleteSlide {slide:ids[1]},Command::Batch(vec![Command::MoveSlide {slide:ids[2],index:0},Command::DeleteSlide {slide:fresh}])] {
            let before=doc.presentation().clone();doc.execute(vec![command]).unwrap();let after=doc.presentation().clone();assert!(after.valid_sections(&after.sections));assert!(doc.undo());assert_eq!(doc.presentation(),&before);assert!(doc.redo());assert_eq!(doc.presentation(),&after);
        }
        let before=doc.presentation().clone();let good=before.sections.clone();
        for kind in 0..4 {let mut bad=good.clone();match kind {0=>bad[0].id="invalid".into(),1=>bad[1].id=bad[0].id.clone(),2=>{bad.iter_mut().find(|section|!section.slides.is_empty()).unwrap().slides.pop();},3=>bad[0].name="bad\0name".into(),_=>unreachable!()}assert_eq!(doc.execute(vec![Command::SetSections {sections:bad}]),Err(EditError::InvalidSections));assert_eq!(doc.presentation(),&before);}
        assert!(doc.execute(vec![Command::MoveSlide {slide:ids[0],index:0},Command::DeleteSlide {slide:SlideId(u64::MAX)}]).is_err());assert_eq!(doc.presentation(),&before,"group rollback keeps section membership");
    }

    #[test]
    fn slide_size_rejects_invalid_dimensions_without_changes() {
        let (mut doc, _, _) = doc_with_box();
        let before = doc.presentation().clone();
        for size in [EmuSize::new(0, 6_858_000), EmuSize::new(914_399, 6_858_000), EmuSize::new(12_192_000, 51_206_401)] {
            assert_eq!(doc.execute(vec![Command::SetSlideSize { size }]), Err(EditError::InvalidSlideSize));
            assert_eq!(doc.presentation(), &before);
        }
    }

    #[test]
    fn undo_redo_round_trip() {
        let (mut doc, slide, shape) = doc_with_box();
        let before = doc.presentation().clone();
        doc.execute(vec![
            Command::SetFrame { slide, frames: vec![(shape, EmuRect::new(10, 10, 200, 80))], with_attached: true },
            Command::SetText { slide, shape, text: "変更".into() },
        ])
        .unwrap();
        let after = doc.presentation().clone();
        assert!(doc.undo());
        assert_eq!(doc.presentation(), &before);
        assert!(doc.redo());
        assert_eq!(doc.presentation(), &after);
    }

    #[test]
    fn failed_step_rolls_back() {
        let (mut doc, slide, shape) = doc_with_box();
        let before = doc.presentation().clone();
        let err = doc.execute(vec![
            Command::SetText { slide, shape, text: "x".into() },
            Command::DeleteShape { slide, shapes: vec![ShapeId(999)], with_attached: true },
        ]);
        assert_eq!(err, Err(EditError::NoSuchShape(ShapeId(999))));
        assert_eq!(doc.presentation(), &before);
        assert!(doc.undo()); // only the insert is on the stack
        assert!(!doc.can_undo());
    }

    #[test]
    fn remove_and_restore_keeps_z_order() {
        let (mut doc, slide, first) = doc_with_box();
        let second = doc.ids().new_shape_id();
        doc.execute(vec![Command::InsertShape { slide, shapes: vec![(1, Shape::text_box(second, EmuRect::new(0, 0, 1, 1), ""))] }])
        .unwrap();
        doc.execute(vec![Command::DeleteShape { slide, shapes: vec![first], with_attached: true }]).unwrap();
        doc.undo();
        let ids: Vec<_> = doc.presentation().slides[0].shapes.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![first, second]);
    }

    #[test]
    fn amend_merges_into_one_step() {
        let (mut doc, slide, shape) = doc_with_box();
        doc.amend_last(vec![Command::SetText { slide, shape, text: "追記".into() }]).unwrap();
        assert!(doc.undo());
        assert!(doc.presentation().slides[0].shapes.is_empty());
        assert!(doc.redo());
        assert_eq!(doc.presentation().slides[0].shapes[0].text_body().unwrap().text(), "追記");
    }

    #[test]
    fn retract_leaves_no_redo() {
        let (mut doc, _, _) = doc_with_box();
        assert!(doc.retract_last());
        assert!(doc.presentation().slides[0].shapes.is_empty());
        assert!(!doc.can_redo());
        assert!(!doc.can_undo());
    }

    #[test]
    fn last_slide_cannot_be_removed() {
        let (mut doc, slide, _) = doc_with_box();
        assert_eq!(doc.execute(vec![Command::DeleteSlide { slide }]), Err(EditError::LastSlide));
    }

    #[test]
    fn undoing_text_edit_restores_formatting() {
        let (mut doc, slide, shape) = doc_with_box();
        let style = TextStyle { bold: true, ..TextStyle::default() };
        doc.execute(vec![Command::SetTextStyle { slide, shape, style }]).unwrap();
        doc.execute(vec![Command::SetText { slide, shape, text: "a\nb".into() }]).unwrap();
        let formatted = doc.presentation().clone();
        doc.execute(vec![Command::SetTextStyle { slide, shape, style: TextStyle::default() }])
            .unwrap();
        doc.undo();
        assert_eq!(doc.presentation(), &formatted);
    }

    #[test]
    fn edit_text_undoes_exactly() {
        let (mut doc, slide, shape) = doc_with_box();
        let before = doc.presentation().clone();
        doc.execute(vec![Command::ReplaceTextRange { slide, shape, at: 2, remove: 1, insert: "ば".into() }]).unwrap();
        assert_eq!(doc.presentation().slides[0].shapes[0].text_body().unwrap().text(), "こんばちは");
        doc.undo();
        assert_eq!(doc.presentation(), &before);
    }

    #[test]
    fn new_edit_clears_redo() {
        let (mut doc, slide, shape) = doc_with_box();
        doc.execute(vec![Command::SetText { slide, shape, text: "a".into() }]).unwrap();
        doc.undo();
        assert!(doc.can_redo());
        doc.execute(vec![Command::SetText { slide, shape, text: "b".into() }]).unwrap();
        assert!(!doc.can_redo());
    }

    #[test]
    fn move_slide_and_shape_undo() {
        let (mut doc, slide, first) = doc_with_box();
        let second = doc.ids().new_shape_id();
        doc.execute(vec![Command::InsertShape { slide, shapes: vec![(1, Shape::text_box(second, EmuRect::new(0, 0, 1, 1), ""))] }]).unwrap();
        let s2 = doc.ids().new_slide_id();
        doc.execute(vec![Command::InsertSlide { index: 1, slide: Slide::new(s2) }]).unwrap();
        let before = doc.presentation().clone();
        doc.execute(vec![Command::MoveSlide { slide: s2, index: 0 }, Command::MoveShape { slide, shape: first, index: 1 }]).unwrap();
        assert_eq!(doc.presentation().slides[0].id, s2);
        assert_eq!(doc.presentation().slides[1].shapes[1].id, first);
        doc.undo();
        assert_eq!(doc.presentation(), &before);
        assert_eq!(doc.execute(vec![Command::MoveSlide { slide: s2, index: 2 }]), Err(EditError::IndexOutOfRange));
    }

    #[test]
    fn transform_fill_and_stroke_undo() {
        let (mut doc, slide, shape) = doc_with_box();
        let before = doc.presentation().clone();
        let stroke = Stroke::solid(crate::Color::BLACK, 12_700);
        doc.execute(vec![
            Command::SetTransform { slide, shape, rotation: -5_400_000, flip_h: true, flip_v: false },
            Command::SetFill { slide, shape, fill: Fill::Solid(crate::Color::WHITE) },
            Command::SetStroke { slide, shape, stroke: Some(stroke.clone()) },
        ])
        .unwrap();
        let s = &doc.presentation().slides[0].shapes[0];
        assert_eq!(s.rotation, 16_200_000);
        let ShapeKind::Auto(auto) = &s.kind else { panic!("a filled text box is an auto shape") };
        assert_eq!(auto.stroke, Some(stroke));
        assert_eq!(auto.text.as_ref().unwrap().text(), "こんにちは");
        doc.undo();
        assert_eq!(doc.presentation(), &before);
    }

    #[test]
    fn slide_numbers_follow_the_order() {
        use crate::text::{Run, RunStyle, TextField};
        let mut doc = Document::new(Presentation::new());
        let a = doc.presentation().slides[0].id;
        let mut body = crate::TextBody::default();
        body.paragraphs[0].runs = vec![Run { text: "1".into(), style: RunStyle { field: Some(TextField::SlideNumber), ..RunStyle::default() } }];
        let id = doc.ids().new_shape_id();
        doc.execute(vec![Command::InsertShape { slide: a, shapes: vec![(0, Shape::new(id, EmuRect::new(0, 0, 1, 1), ShapeKind::TextBox(body)))] }]).unwrap();
        let b = doc.ids().new_slide_id();
        doc.execute(vec![Command::InsertSlide { index: 0, slide: Slide::new(b) }]).unwrap();
        assert_eq!(doc.presentation().slides[1].shapes[0].text_body().unwrap().text(), "2");
        doc.undo();
        assert_eq!(doc.presentation().slides[0].shapes[0].text_body().unwrap().text(), "1");
    }

    #[test]
    fn slide_name_and_placeholder_undo_redo() {
        use crate::model::{PlaceholderRef, SlidePlaceholder};
        let (mut doc, slide, shape) = doc_with_box();
        let before = doc.presentation().clone();
        assert_eq!(before.slides[0].name, None);
        assert!(!before.slides[0].placeholders.contains_key(&shape));

        let ph = SlidePlaceholder {
            placeholder: PlaceholderRef { kind: Some("title".to_string()), index: None },
            own_frame: true,
        };
        doc.execute(vec![
            Command::SetSlideName { slide, name: Some("スライドタイトル".into()) },
            Command::SetPlaceholder { slide, shape, placeholder: Some(ph.clone()) },
        ]).unwrap();

        let modified = doc.presentation().clone();
        assert_eq!(modified.slides[0].name.as_deref(), Some("スライドタイトル"));
        assert_eq!(modified.slides[0].placeholders.get(&shape), Some(&ph));

        assert!(doc.undo());
        assert_eq!(doc.presentation(), &before);

        assert!(doc.redo());
        assert_eq!(doc.presentation(), &modified);
    }

    #[test]
    fn shape_link_tooltip_undo_redo() {
        let (mut doc, slide, shape) = doc_with_box();
        let before = doc.presentation().clone();
        assert!(!before.slides[0].shape_link_tooltips.contains_key(&shape));

        doc.execute(vec![
            Command::SetShapeLinkTooltip { slide, shape, tooltip: Some("クリックして詳細を表示".into()) },
        ]).unwrap();

        let modified = doc.presentation().clone();
        assert_eq!(modified.slides[0].shape_link_tooltips.get(&shape).map(|s| s.as_str()), Some("クリックして詳細を表示"));

        assert!(doc.undo());
        assert_eq!(doc.presentation(), &before);

        assert!(doc.redo());
        assert_eq!(doc.presentation(), &modified);
    }

    // ----- gestures (a touch that can be taken back) -----

    fn move_by(doc: &mut Document, slide: SlideId, shape: ShapeId, dx: i64) {
        let rect = doc.presentation().slide(slide).unwrap().shape(shape).unwrap().frame;
        doc.execute(vec![Command::SetFrame { slide, frames: vec![(shape, EmuRect::new(rect.x + dx, rect.y, rect.width, rect.height))], with_attached: true }]).unwrap();
    }

    fn frame_x(doc: &Document, slide: SlideId, shape: ShapeId) -> i64 {
        doc.presentation().slide(slide).unwrap().shape(shape).unwrap().frame.x
    }

    fn insert_box(doc: &mut Document, slide: SlideId) {
        let id = doc.ids().new_shape_id();
        doc.execute(vec![Command::InsertShape { slide, shapes: vec![(0, Shape::text_box(id, EmuRect::new(5, 5, 10, 10), "x"))] }]).unwrap();
    }

    #[test]
    fn rolling_a_gesture_back_restores_the_document_the_redo_list_and_the_clean_state() {
        let (mut doc, slide, shape) = doc_with_box();
        move_by(&mut doc, slide, shape, 10);
        doc.undo();
        doc.mark_clean();
        let (undo_len, clean_x) = (doc.undo_len(), frame_x(&doc, slide, shape));
        assert!(doc.can_redo() && !doc.is_dirty());

        let mark = doc.begin_gesture();
        move_by(&mut doc, slide, shape, 100);
        move_by(&mut doc, slide, shape, 100);
        assert_eq!(frame_x(&doc, slide, shape), clean_x + 200);
        assert!(doc.is_dirty() && !doc.can_redo());

        assert!(doc.rollback_gesture(mark));
        assert_eq!(frame_x(&doc, slide, shape), clean_x);
        assert_eq!((doc.undo_len(), doc.can_redo(), doc.is_dirty()), (undo_len, true, false));
        // The redo that was there still works.
        assert!(doc.redo());
        assert_eq!(frame_x(&doc, slide, shape), clean_x + 10);
    }

    #[test]
    fn a_committed_gesture_is_one_undo_step() {
        let (mut doc, slide, shape) = doc_with_box();
        let before = frame_x(&doc, slide, shape);
        let steps = doc.undo_len();
        let mark = doc.begin_gesture();
        move_by(&mut doc, slide, shape, 5);
        move_by(&mut doc, slide, shape, 7);
        move_by(&mut doc, slide, shape, 11);
        doc.commit_gesture(mark);
        assert_eq!(doc.undo_len(), steps + 1);
        assert!(doc.undo());
        assert_eq!(frame_x(&doc, slide, shape), before);
        assert!(doc.redo());
        assert_eq!(frame_x(&doc, slide, shape), before + 23);
    }

    #[test]
    fn a_gesture_that_changed_nothing_leaves_no_step() {
        let (mut doc, ..) = doc_with_box();
        let steps = doc.undo_len();
        let mark = doc.begin_gesture();
        doc.commit_gesture(mark);
        assert_eq!(doc.undo_len(), steps);
        let mark = doc.begin_gesture();
        assert!(doc.rollback_gesture(mark));
        assert_eq!(doc.undo_len(), steps);
    }

    #[test]
    fn a_step_retracted_during_a_gesture_comes_back_at_rollback() {
        let (mut doc, slide, shape) = doc_with_box();
        let steps = doc.undo_len();
        let mark = doc.begin_gesture();
        // (an empty new box left by a tap elsewhere: the app retracts the step that made it)
        assert!(doc.retract_last());
        assert!(doc.presentation().slide(slide).unwrap().shape(shape).is_none());
        insert_box(&mut doc, slide);
        assert!(doc.rollback_gesture(mark));
        assert_eq!(doc.undo_len(), steps);
        assert!(doc.presentation().slide(slide).unwrap().shape(shape).is_some());
        // and it is still one undo step that removes the box again
        assert!(doc.undo());
        assert!(doc.presentation().slide(slide).unwrap().shape(shape).is_none());
    }

    #[test]
    fn an_amend_during_a_gesture_does_not_change_a_step_from_before_it() {
        let (mut doc, slide, shape) = doc_with_box();
        let steps = doc.undo_len();
        let before = frame_x(&doc, slide, shape);
        let mark = doc.begin_gesture();
        let rect = doc.presentation().slide(slide).unwrap().shape(shape).unwrap().frame;
        doc.amend_last(vec![Command::SetFrame { slide, frames: vec![(shape, EmuRect::new(rect.x + 9, rect.y, rect.width, rect.height))], with_attached: true }]).unwrap();
        assert!(doc.rollback_gesture(mark));
        assert_eq!((doc.undo_len(), frame_x(&doc, slide, shape)), (steps, before));
    }

    #[test]
    fn the_users_own_undo_ends_the_gesture() {
        let (mut doc, slide, shape) = doc_with_box();
        let mark = doc.begin_gesture();
        move_by(&mut doc, slide, shape, 3);
        doc.undo();
        assert!(!doc.gesture_is_open(&mark));
        assert!(!doc.rollback_gesture(mark));
    }
}
