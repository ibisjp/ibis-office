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
    RemoveSlide { slide: SlideId },
    InsertShape { slide: SlideId, index: usize, shape: Shape },
    RemoveShape { slide: SlideId, shape: ShapeId },
    /// Set a shape's frame. The shapes attached to it (see
    /// [`Slide::attached`]) are moved and scaled with it.
    SetFrame { slide: SlideId, shape: ShapeId, frame: EmuRect },
    /// Set the frames of shapes exactly (attached shapes are not moved):
    /// the inverse of [`Command::SetFrame`].
    SetFrames { slide: SlideId, frames: Vec<(ShapeId, EmuRect)> },
    /// Remove shapes exactly (with no attached shapes along): the inverse
    /// of [`Command::InsertShapes`], and how a chart is removed with the
    /// shapes drawn over it.
    RemoveShapes { slide: SlideId, shapes: Vec<ShapeId> },
    /// Insert shapes at indices, in ascending order (each index is where the
    /// shape is once the ones before it are in).
    InsertShapes { slide: SlideId, shapes: Vec<(usize, Shape)> },
    /// Several commands as one (the inverse of an edit that changes several
    /// shapes, such as removing a chart with the shapes drawn over it).
    Group(Vec<Command>),
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
    EditText { slide: SlideId, shape: ShapeId, at: usize, remove: usize, insert: String },
    /// Search replacement: inserted text takes the first matched run format.
    ReplaceText { slide: SlideId, shape: ShapeId, at: usize, remove: usize, insert: String },
    /// Replace what a shape is (the inverse of changes that turn a text box
    /// into an auto shape).
    SetKind { slide: SlideId, shape: ShapeId, kind: Box<ShapeKind> },
    /// Give characters `start..end` of a shape's text a character format
    /// (範囲の文字の書式).
    SetCharacterFormat { slide: SlideId, shape: ShapeId, start: usize, end: usize, format: crate::text::CharacterFormat },
    /// Give the paragraphs characters `start..end` of a shape's text touch a
    /// paragraph format (配置・行間・箇条書き・インデント).
    SetParagraphFormat { slide: SlideId, shape: ShapeId, start: usize, end: usize, format: crate::text::ParagraphFormat },
    /// Whether placeholder `shape` has its own place (see
    /// [`crate::model::SlidePlaceholder`]); moving it gives it one.
    SetOwnFrame { slide: SlideId, shape: ShapeId, own: bool },
    /// Replace a slide (the same id) with `slide`: レイアウトの適用
    /// (see [`Presentation::with_layout`]), and its own inverse.
    SetSlide { slide: Slide },
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
    /// the inverse of `RemoveMaster`. Its layouts are added with
    /// `InsertLayout`.
    InsertMaster { index: usize, master: crate::model::Master },
    /// Remove a master no layout follows; the inverse of `InsertMaster`.
    RemoveMaster { master: crate::model::MasterId },
    /// Rename a layout (レイアウト名の変更); its own inverse.
    RenameLayout { layout: crate::model::LayoutId, name: String },
    /// Add a layout at `index` (a copy of another: its `copy_of` names it);
    /// the inverse of `RemoveLayout`.
    InsertLayout { index: usize, layout: crate::model::Layout },
    /// Remove a layout no slide uses; the inverse of `InsertLayout`.
    RemoveLayout { layout: crate::model::LayoutId },
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
    /// 書き込みパスワード（`p:modifyVerifier`）を設定・解除する。
    SetModifyVerifier { verifier: Option<crate::model::ModifyVerifier> },
    /// ドキュメント プロパティ（タイトル・作成者・カスタムプロパティ等: `docProps/core.xml`, `docProps/app.xml`, `docProps/custom.xml`）.
    SetDocProperties { properties: crate::flow::DocProperties },
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
    /// セルに対する配置方式 (editAs: twoCell, oneCell, absolute).
    SetShapePlacement { slide: SlideId, shape: ShapeId, placement: crate::model::ObjectPlacement },
    /// オブジェクトを印刷する (fPrintsWithSheet).
    SetShapePrint { slide: SlideId, shape: ShapeId, print: bool },
    /// セルの値を表示する数式連動 (`xdr:f`).
    SetShapeFormula { slide: SlideId, shape: ShapeId, formula: Option<String> },
    /// 背景グラフィックを表示しない: a slide's backdrop hidden or shown.
    SetBackdropHidden { slide: SlideId, hidden: bool },
    /// Show or hide a shape (選択ウィンドウ's eye).
    SetShapeHidden { slide: SlideId, shape: ShapeId, hidden: bool },
    /// ズーム後に元へ戻る: a zoom shape returns to its origin slide after viewing (or not).
    SetShapeReturnToZoom { slide: SlideId, shape: ShapeId, return_to_zoom: bool },
    /// ズームの背景: a zoom shape shows the slide background or has a transparent background (`showBg`).
    SetShapeZoomShowBg { slide: SlideId, shape: ShapeId, show_bg: Option<bool> },
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
        if let Command::SetFrame { slide, shape, .. } | Command::SetTransform { slide, shape, .. } = &command {
            let (slide, shape) = (*slide, *shape);
            let takes_layouts = self.slide(slide).and_then(|s| s.placeholders.get(&shape)).is_some_and(|p| !p.own_frame);
            if takes_layouts {
                return self.apply(Command::Group(vec![command, Command::SetOwnFrame { slide, shape, own: true }]));
            }
        }
        self.apply_one(command)
    }

    fn apply_one(&mut self, command: Command) -> Result<Command, EditError> {
        if !self.sections.is_empty() && matches!(&command,Command::InsertSlide {..}|Command::RemoveSlide {..}|Command::MoveSlide {..}) {
            let sections=self.sections.clone();let inverse=self.apply_primitive(command)?;self.reconcile_sections();
            return Ok(Command::SlideStructure {command:Box::new(inverse),sections});
        }
        let structural=match &command {Command::InsertShape {slide,..}|Command::RemoveShape {slide,..}|Command::InsertShapes {slide,..}|Command::RemoveShapes {slide,..}=>Some(*slide),_=>None};
        if let Some(slide)=structural {if let Some(groups)=self.slide(slide).filter(|s|!s.groups.is_empty()).map(|s|s.groups.clone()) {
            let inverse=self.apply_primitive(command)?;let target=self.slide_mut(slide).expect("structural edit retained slide");target.groups=target.live_groups(&groups);
            return Ok(Command::ShapeStructure {slide,command:Box::new(inverse),groups});
        }}
        self.apply_primitive(command)
    }

    fn apply_primitive(&mut self, command: Command) -> Result<Command, EditError> {
        match command {
            Command::ShapeStructure {slide,command,groups} => {
                let target=match &*command {Command::InsertShape {slide,..}|Command::RemoveShape {slide,..}|Command::InsertShapes {slide,..}|Command::RemoveShapes {slide,..}=>Some(*slide),_=>None};
                if target!=Some(slide) {return Err(EditError::InvalidShapeGroups);}
                let inverse=self.apply_primitive(*command)?;
                if !self.slide(slide).is_some_and(|s|s.valid_groups(&groups)) {let _=self.apply_primitive(inverse);return Err(EditError::InvalidShapeGroups);}
                let previous=std::mem::replace(&mut self.slide_mut(slide).expect("checked").groups,groups);Ok(Command::ShapeStructure {slide,command:Box::new(inverse),groups:previous})
            }
            Command::SlideStructure {command,sections} => {
                if !matches!(&*command,Command::InsertSlide {..}|Command::RemoveSlide {..}|Command::MoveSlide {..}) {return Err(EditError::InvalidSections);}
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
                Ok(Command::RemoveSlide { slide: id })
            }
            Command::RemoveSlide { slide } => {
                let index = self.slide_index(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if self.slides.len() == 1 {
                    return Err(EditError::LastSlide);
                }
                let slide = self.slides.remove(index);
                self.renumber_slides();
                Ok(Command::InsertSlide { index, slide })
            }
            Command::InsertShape { slide, index, shape } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if index > s.shapes.len() {
                    return Err(EditError::IndexOutOfRange);
                }
                if s.shape(shape.id).is_some() {
                    return Err(EditError::DuplicateId);
                }
                let id = shape.id;
                s.shapes.insert(index, shape);
                Ok(Command::RemoveShape { slide, shape: id })
            }
            Command::RemoveShape { slide, shape } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let index = s.shape_index(shape).ok_or(EditError::NoSuchShape(shape))?;
                if s.is_attached(shape) {
                    return Err(EditError::Attached(shape));
                }
                // The shapes attached to it go with it.
                let attached = s.attached_to(shape);
                if !attached.is_empty() {
                    let shapes = std::iter::once(shape).chain(attached).collect();
                    return self.apply_primitive(Command::RemoveShapes { slide, shapes });
                }
                let removed = s.shapes.remove(index);
                Ok(Command::InsertShape { slide, index, shape: removed })
            }
            Command::RemoveShapes { slide, shapes } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let mut indices: Vec<usize> = shapes.iter().map(|id| s.shape_index(*id).ok_or(EditError::NoSuchShape(*id))).collect::<Result<_, _>>()?;
                indices.sort_unstable();
                indices.dedup();
                let mut removed: Vec<(usize, Shape)> = Vec::with_capacity(indices.len());
                for &i in indices.iter().rev() {
                    removed.push((i, s.shapes.remove(i)));
                }
                removed.reverse();
                Ok(Command::InsertShapes { slide, shapes: removed })
            }
            Command::InsertShapes { slide, shapes } => {
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
                Ok(Command::RemoveShapes { slide, shapes: ids })
            }
            Command::SetFrame { slide, shape, frame } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.is_attached(shape) {
                    return Err(EditError::Attached(shape));
                }
                let attached = s.attached_to(shape);
                let target = s.shapes.iter_mut().find(|x| x.id == shape).ok_or(EditError::NoSuchShape(shape))?;
                let old = std::mem::replace(&mut target.frame, frame);
                if attached.is_empty() {
                    return Ok(Command::SetFrame { slide, shape, frame: old });
                }
                // Attached shapes keep their place in the frame.
                let mut frames = vec![(shape, old)];
                for id in attached {
                    let Some(a) = s.shapes.iter_mut().find(|x| x.id == id) else { continue };
                    frames.push((id, a.frame));
                    a.frame = follow(a.frame, old, frame);
                }
                Ok(Command::SetFrames { slide, frames })
            }
            Command::SetFrames { slide, frames } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if let Some((missing, _)) = frames.iter().find(|(id, _)| s.shape(*id).is_none()) {
                    return Err(EditError::NoSuchShape(*missing));
                }
                let mut old = Vec::with_capacity(frames.len());
                for (id, frame) in frames {
                    let target = s.shapes.iter_mut().find(|x| x.id == id).expect("checked");
                    old.push((id, std::mem::replace(&mut target.frame, frame)));
                }
                Ok(Command::SetFrames { slide, frames: old })
            }
            Command::Group(commands) => {
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
                Ok(Command::Group(inverses))
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
                        Ok(Command::SetKind { slide, shape, kind: Box::new(old) })
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
                        Ok(Command::SetKind { slide, shape, kind: Box::new(old) })
                    }
                    _ => Err(EditError::NotApplicable(shape)),
                }
            }
            Command::SetKind { slide, shape, kind } => {
                let target = self.shape_mut(slide, shape)?;
                let old = std::mem::replace(&mut target.kind, *kind);
                Ok(Command::SetKind { slide, shape, kind: Box::new(old) })
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
            Command::EditText { slide, shape, at, remove, insert } => {
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
            Command::SetOwnFrame { slide, shape, own } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let ph = s.placeholders.get_mut(&shape).ok_or(EditError::NoSuchShape(shape))?;
                let old = std::mem::replace(&mut ph.own_frame, own);
                Ok(Command::SetOwnFrame { slide, shape, own: old })
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
                Ok(Command::RemoveMaster { master: id })
            }
            Command::RemoveMaster { master } => {
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
                Ok(Command::RemoveLayout { layout: id })
            }
            Command::RemoveLayout { layout } => {
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
            Command::SetReadOnlyRecommended { read_only_recommended } => {
                let old = std::mem::replace(&mut self.read_only_recommended, read_only_recommended);
                Ok(Command::SetReadOnlyRecommended { read_only_recommended: old })
            }
            Command::SetModifyVerifier { verifier } => {
                let old = std::mem::replace(&mut self.modify_verifier, verifier);
                Ok(Command::SetModifyVerifier { verifier: old })
            }
            Command::SetDocProperties { properties } => {
                let old = std::mem::replace(&mut self.properties, properties);
                Ok(Command::SetDocProperties { properties: old })
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
            Command::SetShapeZoomShowBg { slide, shape, show_bg } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                if s.shape(shape).is_none() {
                    return Err(EditError::NoSuchShape(shape));
                }
                let old = match show_bg {
                    Some(bg) => s.shape_zoom_show_bg.insert(shape, bg),
                    None => s.shape_zoom_show_bg.remove(&shape),
                };
                Ok(Command::SetShapeZoomShowBg { slide, shape, show_bg: old })
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
            Command::SetBackdropHidden { slide, hidden } => {
                let s = self.slide_mut(slide).ok_or(EditError::NoSuchSlide(slide))?;
                let was = std::mem::replace(&mut s.hide_backdrop, hidden);
                Ok(Command::SetBackdropHidden { slide, hidden: was })
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
                    | Command::EditText { slide, shape, .. }
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
                    Command::EditText { at, remove, insert, .. } => body.splice(at, remove, &insert),
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
}

impl Default for Document {
    fn default() -> Self {
        Self::new(Presentation::default())
    }
}

impl Document {
    pub fn new(presentation: Presentation) -> Self {
        Self { presentation, undo: Vec::new(), redo: Vec::new(), repeat_sheet:Vec::new(), repeat_sheet_group:false, clean_undo_len: Some(0), file: None }
    }

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
        let repeat:Vec<_> = commands.iter().filter_map(|command|match command {
            Command::Sheet(edit @ (crate::SheetCommand::SetFormat{..}|crate::SheetCommand::Insert{..}|crate::SheetCommand::Delete{..}|crate::SheetCommand::ClearFormats{..}|crate::SheetCommand::ShiftCells{..})) => Some(edit.clone()),
            _ => None,
        }).collect();
        let inverses = self.apply_all(commands)?;
        if !inverses.is_empty() {
            if self.clean_undo_len.is_some_and(|clean| clean > self.undo.len()) {
                self.clean_undo_len = None;
            }
            self.undo.push(inverses);
            self.redo.clear();
            let same_target = self.repeat_sheet_group && !repeat.is_empty() && !self.repeat_sheet.is_empty() && repeat.iter().chain(&self.repeat_sheet).all(|edit|match (repeat.first(),edit) {
                (Some(crate::SheetCommand::SetFormat{slide:a,ranges:x,..}),crate::SheetCommand::SetFormat{slide:b,ranges:y,..}) => a==b && x==y,
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
        match self.undo.last_mut() {
            Some(step) => {
                inverses.append(step);
                *step = inverses;
            }
            None => self.undo.push(inverses),
        }
        self.redo.clear();
        Ok(())
    }

    /// Undo the most recent step without making it redoable, for an edit the
    /// user abandoned (such as a new text box left empty).
    pub fn retract_last(&mut self) -> bool {
        let Some(step) = self.undo.pop() else { return false };
        if self.clean_undo_len.is_some_and(|clean| clean > self.undo.len()) {
            self.clean_undo_len = None;
        }
        self.apply_all(step).is_ok()
    }

    pub fn undo(&mut self) -> bool {
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
        let Some(step) = self.redo.pop() else { return false };
        match self.apply_all(step) {
            Ok(inverses) => {
                self.undo.push(inverses);
                true
            }
            Err(_) => false,
        }
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
        doc.execute(vec![Command::InsertShape {
            slide,
            index: 0,
            shape: Shape::text_box(shape, EmuRect::new(0, 0, 100, 50), "こんにちは"),
        }])
        .unwrap();
        (doc, slide, shape)
    }

    #[test]
    fn shape_group_hierarchy_undo_and_invalid_membership() {
        use crate::{ShapeGroup,GroupMember};let (mut doc,slide,shape)=doc_with_box();let id=doc.ids().new_shape_id();let mut second=doc.presentation().slides[0].shapes[0].clone();second.id=id;doc.execute(vec![Command::InsertShape {slide,index:1,shape:second}]).unwrap();
        let before=doc.presentation().clone();let groups=vec![ShapeGroup {has_unmodelled_children:false,native_id:"99".into(),name:"outer".into(),children:vec![GroupMember::Group(Box::new(ShapeGroup {has_unmodelled_children:false,native_id:"100".into(),name:"inner".into(),children:vec![GroupMember::Shape(shape)]})),GroupMember::Shape(id)]}];doc.execute(vec![Command::SetShapeGroups {slide,groups:groups.clone()}]).unwrap();let grouped=doc.presentation().clone();assert!(grouped.slides[0].valid_groups(&groups));assert!(doc.undo());assert_eq!(doc.presentation(),&before);assert!(doc.redo());assert_eq!(doc.presentation(),&grouped);
        for kind in 0..4 {let mut invalid=groups.clone();match kind {0=>invalid[0].native_id="invalid".into(),1=>invalid[0].children.push(GroupMember::Shape(shape)),2=>invalid[0].children.push(GroupMember::Shape(ShapeId(u64::MAX))),3=>invalid[0].name="bad\0name".into(),_=>unreachable!()}assert_eq!(doc.execute(vec![Command::SetShapeGroups {slide,groups:invalid}]),Err(EditError::InvalidShapeGroups));assert_eq!(doc.presentation(),&grouped);}
        for command in [Command::RemoveShape {slide,shape},Command::RemoveShapes {slide,shapes:vec![shape,id]}] {let before=doc.presentation().clone();doc.execute(vec![command]).unwrap();let after=doc.presentation().clone();assert!(after.slides[0].valid_groups(&after.slides[0].groups));assert!(doc.undo());assert_eq!(doc.presentation(),&before);assert!(doc.redo());assert_eq!(doc.presentation(),&after);assert!(doc.undo());assert_eq!(doc.presentation(),&before);}
        let mut duplicate=groups.clone();let GroupMember::Group(inner)=&mut duplicate[0].children[0] else {panic!("nested")};inner.native_id="99".into();assert_eq!(doc.execute(vec![Command::SetShapeGroups {slide,groups:duplicate}]),Err(EditError::InvalidShapeGroups));assert_eq!(doc.presentation(),&grouped);
        assert!(doc.execute(vec![Command::SetShapeGroups {slide,groups:vec![]},Command::RemoveShape {slide,shape:ShapeId(u64::MAX)}]).is_err());assert_eq!(doc.presentation(),&grouped,"partial command failure restores hierarchy");
        let mut attached=grouped.clone();attached.slides[0].attached.insert(shape,vec![id]);attached.slides[0].groups=vec![ShapeGroup {has_unmodelled_children:false,native_id:"200".into(),name:"attached owner".into(),children:vec![GroupMember::Shape(shape)]}];let mut doc=Document::new(attached.clone());doc.execute(vec![Command::RemoveShape {slide,shape}]).unwrap();let removed=doc.presentation().clone();assert!(removed.slides[0].groups.is_empty());assert!(removed.slides[0].shapes.is_empty());assert!(doc.undo());assert_eq!(doc.presentation(),&attached);assert!(doc.redo());assert_eq!(doc.presentation(),&removed);

    }

    #[test]
    fn section_structure_undo_redo_and_validation() {
        use crate::Section;
        let mut p=Presentation::new();for _ in 0..2 {let id=p.new_slide_id();p.slides.push(Slide::new(id));}
        let ids=p.slides.iter().map(|s|s.id).collect::<Vec<_>>();let mut doc=Document::new(p);
        let sections=vec![Section {id:"{11111111-1111-1111-1111-111111111111}".into(),name:"first".into(),slides:ids[..2].to_vec()},Section {id:"{22222222-2222-2222-2222-222222222222}".into(),name:"second".into(),slides:ids[2..].to_vec()}];
        doc.execute(vec![Command::SetSections {sections:sections.clone()}]).unwrap();
        let fresh=doc.ids().new_slide_id();
        for command in [Command::MoveSlide {slide:ids[0],index:2},Command::InsertSlide {index:1,slide:Slide::new(fresh)},Command::RemoveSlide {slide:ids[1]},Command::Group(vec![Command::MoveSlide {slide:ids[2],index:0},Command::RemoveSlide {slide:fresh}])] {
            let before=doc.presentation().clone();doc.execute(vec![command]).unwrap();let after=doc.presentation().clone();assert!(after.valid_sections(&after.sections));assert!(doc.undo());assert_eq!(doc.presentation(),&before);assert!(doc.redo());assert_eq!(doc.presentation(),&after);
        }
        let before=doc.presentation().clone();let good=before.sections.clone();
        for kind in 0..4 {let mut bad=good.clone();match kind {0=>bad[0].id="invalid".into(),1=>bad[1].id=bad[0].id.clone(),2=>{bad.iter_mut().find(|section|!section.slides.is_empty()).unwrap().slides.pop();},3=>bad[0].name="bad\0name".into(),_=>unreachable!()}assert_eq!(doc.execute(vec![Command::SetSections {sections:bad}]),Err(EditError::InvalidSections));assert_eq!(doc.presentation(),&before);}
        assert!(doc.execute(vec![Command::MoveSlide {slide:ids[0],index:0},Command::RemoveSlide {slide:SlideId(u64::MAX)}]).is_err());assert_eq!(doc.presentation(),&before,"group rollback keeps section membership");
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
            Command::SetFrame { slide, shape, frame: EmuRect::new(10, 10, 200, 80) },
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
            Command::RemoveShape { slide, shape: ShapeId(999) },
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
        doc.execute(vec![Command::InsertShape {
            slide,
            index: 1,
            shape: Shape::text_box(second, EmuRect::new(0, 0, 1, 1), ""),
        }])
        .unwrap();
        doc.execute(vec![Command::RemoveShape { slide, shape: first }]).unwrap();
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
        assert_eq!(doc.execute(vec![Command::RemoveSlide { slide }]), Err(EditError::LastSlide));
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
        doc.execute(vec![Command::EditText { slide, shape, at: 2, remove: 1, insert: "ば".into() }]).unwrap();
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
        doc.execute(vec![Command::InsertShape { slide, index: 1, shape: Shape::text_box(second, EmuRect::new(0, 0, 1, 1), "") }]).unwrap();
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
        doc.execute(vec![Command::InsertShape { slide: a, index: 0, shape: Shape::new(id, EmuRect::new(0, 0, 1, 1), ShapeKind::TextBox(body)) }]).unwrap();
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
}
