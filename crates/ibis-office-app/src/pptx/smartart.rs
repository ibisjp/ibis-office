// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! SmartArt 図表:
//! - SmartArt を図形に変換 (SmartArt のデザイン > 変換 > 図形に変換, PPTX-SMART-017):
//!   PowerPoint の `dsp:drawing` から読み込んだ描画図形をスライド自身のグループ図形にし、
//!   図表枠とファイル内の部品を外す。
//! - SmartArt プリセット挿入 (挿入 > SmartArt, PPTX-SMART-001〜008):
//!   リスト型・手順型・循環型・階層型・関係型・マトリックス型・ピラミッド型・画像型
//!   の各種ダイアグラムをスライド上に新規作成しグループ化。
//! - 箇条書きから SmartArt へ変換 (PPTX-SMART-016):
//!   選択したテキストボックスの箇条書き段落をダイアグラムの各要素としてレイアウト。
//! - SmartArt 図形の追加と削除 (PPTX-SMART-010):
//!   SmartArt グループ内の要素数を動的に増減し再配置。
//! - 階層の上げ下げ・並べ替え (PPTX-SMART-011):
//!   上へ移動・下へ移動・レベル上げ・レベル下げ。
//! - アシスタントの追加 (PPTX-SMART-012):
//!   組織図におけるアシスタント ノードの追加・レイアウト。
//! - レイアウト変更 (PPTX-SMART-013):
//!   既存テキスト・サイズを保持したままダイアグラム種類を変更。
//! - 色と SmartArt スタイル (PPTX-SMART-014):
//!   カラフル・アクセント・枠線等の配色スタイルを適用。
//! - 図形ごとの書式 (PPTX-SMART-015):
//!   ダイアグラム内の個別図形に対する塗り・線・書式設定。
//! - テキスト ウィンドウ (PPTX-SMART-009):
//!   SmartArt の全項目をリスト表示・並べ替え・編集可能なサイドバー パネル。

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{Command, EmuRect, Fill, Placeholder, Rgb, Shape, ShapeGroup, ShapeId, ShapeKind, Stroke};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// Kinds of SmartArt layouts available for insertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmartArtKind {
    /// リスト型: 基本ブロック リスト (PPTX-SMART-001)
    BasicBlockList,
    /// リスト型: 縦方向リスト (PPTX-SMART-001)
    VerticalList,
    /// 手順型: 基本手順 (PPTX-SMART-002)
    BasicProcess,
    /// 手順型: 連続山形矢印 (PPTX-SMART-002)
    ContinuousChevronProcess,
    /// 循環型: 基本循環 (PPTX-SMART-003)
    BasicCycle,
    /// 階層型・組織図: 組織図 (PPTX-SMART-004)
    OrganizationChart,
    /// 関係型: 基本集合関係 (PPTX-SMART-005)
    BasicRelationship,
    /// マトリックス型: 基本マトリックス (PPTX-SMART-006)
    BasicMatrix,
    /// ピラミッド型: 基本ピラミッド (PPTX-SMART-007)
    BasicPyramid,
    /// 画像型: 画像付きリスト (PPTX-SMART-008)
    PictureList,
}

impl SmartArtKind {
    /// The name stored in the file's group name (Japanese whatever the UI's
    /// language, so that the group is found again); `display_label` is for the screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::BasicBlockList => "基本ブロック リスト", // i18n-ok（ファイルの中のグループ名）
            Self::VerticalList => "縦方向リスト", // i18n-ok（ファイルの中のグループ名）
            Self::BasicProcess => "基本手順", // i18n-ok（ファイルの中のグループ名）
            Self::ContinuousChevronProcess => "連続山形矢印", // i18n-ok（ファイルの中のグループ名）
            Self::BasicCycle => "基本循環", // i18n-ok（ファイルの中のグループ名）
            Self::OrganizationChart => "組織図", // i18n-ok（ファイルの中のグループ名）
            Self::BasicRelationship => "基本集合関係", // i18n-ok（ファイルの中のグループ名）
            Self::BasicMatrix => "基本マトリックス", // i18n-ok（ファイルの中のグループ名）
            Self::BasicPyramid => "基本ピラミッド", // i18n-ok（ファイルの中のグループ名）
            Self::PictureList => "画像付きリスト", // i18n-ok（ファイルの中のグループ名）
        }
    }

    pub fn display_label(self) -> gpui_kit::SharedString {
        crate::shell::i18n::text(match self {
            Self::BasicBlockList => "pptx.smartart.layout.list",
            Self::VerticalList => "pptx.smartart.layout.vlist",
            Self::BasicProcess => "pptx.smartart.layout.process",
            Self::ContinuousChevronProcess => "pptx.smartart.layout.chevron",
            Self::BasicCycle => "pptx.smartart.layout.cycle",
            Self::OrganizationChart => "pptx.smartart.layout.hierarchy",
            Self::BasicRelationship => "pptx.smartart.layout.relationship",
            Self::BasicMatrix => "pptx.smartart.layout.matrix",
            Self::BasicPyramid => "pptx.smartart.layout.pyramid",
            Self::PictureList => "pptx.smartart.layout.picture",
        })
    }

    pub fn default_items(self) -> Vec<String> {
        let t = |k: &str| crate::shell::i18n::text(k).to_string();
        let n = |k: &str, n: usize| crate::shell::i18n::format(k, &[("n", &n)]);
        match self {
            Self::BasicBlockList | Self::VerticalList | Self::PictureList => (1..=3).map(|i| n("pptx.smartart.item.item-n", i)).collect(),
            Self::BasicProcess | Self::ContinuousChevronProcess => (1..=3).map(|i| n("pptx.smartart.item.step-n", i)).collect(),
            Self::BasicCycle => ["plan", "do", "evaluate", "improve"].iter().map(|k| t(&format!("pptx.smartart.item.{k}"))).collect(),
            Self::OrganizationChart => ["leader", "member-a", "member-b"].iter().map(|k| t(&format!("pptx.smartart.item.{k}"))).collect(),
            Self::BasicRelationship => ["element-a", "element-b"].iter().map(|k| t(&format!("pptx.smartart.item.{k}"))).collect(),
            Self::BasicMatrix => (1..=4).map(|i| n("pptx.smartart.item.quadrant-n", i)).collect(),
            Self::BasicPyramid => ["top", "middle", "base"].iter().map(|k| t(&format!("pptx.smartart.item.{k}"))).collect(),
        }
    }
}

/// Color styles for SmartArt diagrams (PPTX-SMART-014).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SmartArtColorStyle {
    /// カラフル (アクセントの組み合わせ)
    #[default]
    Colorful,
    /// アクセント 1 (青系濃淡)
    Accent1,
    /// アクセント 2 (オレンジ系濃淡)
    Accent2,
    /// アクセント 3 (緑系濃淡)
    Accent3,
    /// 枠線のみ (白背景＋枠線)
    Outline,
}

impl SmartArtColorStyle {
    pub fn label(self) -> gpui_kit::SharedString {
        crate::shell::i18n::text(match self {
            Self::Colorful => "pptx.smartart.color.colorful",
            Self::Accent1 => "pptx.smartart.color.accent1",
            Self::Accent2 => "pptx.smartart.color.accent2",
            Self::Accent3 => "pptx.smartart.color.accent3",
            Self::Outline => "pptx.smartart.color.outline",
        })
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Colorful => "Colorful",
            Self::Accent1 => "Accent1",
            Self::Accent2 => "Accent2",
            Self::Accent3 => "Accent3",
            Self::Outline => "Outline",
        }
    }

    pub fn from_code(s: &str) -> Self {
        match s {
            "Accent1" => Self::Accent1,
            "Accent2" => Self::Accent2,
            "Accent3" => Self::Accent3,
            "Outline" => Self::Outline,
            _ => Self::Colorful,
        }
    }

    pub fn palette(self, index: usize) -> (Rgb, Option<Rgb>, Rgb) {
        match self {
            Self::Colorful => {
                let fill = SMARTART_PALETTE[index % SMARTART_PALETTE.len()];
                let stroke = fill.tinted(-250_000);
                (fill, Some(stroke), Rgb::WHITE)
            }
            Self::Accent1 => {
                let base = Rgb(0x4472c4);
                let tints = [0, 250_000, 450_000, -200_000, 150_000, 350_000];
                let fill = base.tinted(tints[index % tints.len()]);
                let stroke = fill.tinted(-250_000);
                (fill, Some(stroke), Rgb::WHITE)
            }
            Self::Accent2 => {
                let base = Rgb(0xed7d31);
                let tints = [0, 250_000, 450_000, -200_000, 150_000, 350_000];
                let fill = base.tinted(tints[index % tints.len()]);
                let stroke = fill.tinted(-250_000);
                (fill, Some(stroke), Rgb::WHITE)
            }
            Self::Accent3 => {
                let base = Rgb(0x70ad47);
                let tints = [0, 250_000, 450_000, -200_000, 150_000, 350_000];
                let fill = base.tinted(tints[index % tints.len()]);
                let stroke = fill.tinted(-250_000);
                (fill, Some(stroke), Rgb::WHITE)
            }
            Self::Outline => {
                let fill = Rgb::WHITE;
                let stroke = Rgb(0x4472c4);
                (fill, Some(stroke), Rgb(0x262626))
            }
        }
    }
}

const SMARTART_PALETTE: [Rgb; 6] = [
    Rgb(0x4472c4), // Accent 1: Blue
    Rgb(0xed7d31), // Accent 2: Orange
    Rgb(0x70ad47), // Accent 6: Green
    Rgb(0xffc000), // Accent 4: Yellow/Gold
    Rgb(0x5b9bd5), // Accent 5: Light Blue
    Rgb(0x7030a0), // Accent 7: Purple
];

fn make_smartart_node(
    id: ShapeId,
    geom_preset: &str,
    frame: EmuRect,
    fill_rgb: Rgb,
    border_rgb: Option<Rgb>,
    text_color: Rgb,
    text: &str,
) -> Shape {
    let geometry = ibis_pptx::preset_geometry(geom_preset).unwrap_or(ibis_doc_model::Geometry::Rect);
    let border = border_rgb.map(|c| ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(c), 12_700));
    let mut auto = ibis_doc_model::AutoShape::new(
        geometry,
        ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(fill_rgb)),
        border,
    );
    let style = ibis_doc_model::TextStyle {
        color: text_color,
        bold: true,
        align: ibis_doc_model::TextAlign::Center,
        font_size: 1600,
        ..ibis_doc_model::TextStyle::default()
    };
    auto.text = Some(ibis_doc_model::TextBody {
        anchor: ibis_doc_model::VerticalAnchor::Middle,
        ..ibis_doc_model::TextBody::plain(text, style)
    });
    Shape::new(id, frame, ShapeKind::Auto(auto))
}

fn make_smartart_arrow(id: ShapeId, geom_preset: &str, frame: EmuRect, fill_rgb: Rgb) -> Shape {
    let geometry = ibis_pptx::preset_geometry(geom_preset).unwrap_or(ibis_doc_model::Geometry::Rect);
    let auto = ibis_doc_model::AutoShape::new(
        geometry,
        ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(fill_rgb)),
        None,
    );
    Shape::new(id, frame, ShapeKind::Auto(auto))
}

fn union_rect(a: EmuRect, b: EmuRect) -> EmuRect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = a.right().max(b.right());
    let bottom = a.bottom().max(b.bottom());
    EmuRect::new(x, y, right - x, bottom - y)
}

impl SlideEditor {
    /// The selected SmartArt graphic's frame and the shapes drawn for it.
    fn selected_diagram(&self) -> Option<(ShapeId, Vec<Shape>)> {
        let selected = self.selected_shapes();
        let [shape] = selected.as_slice() else { return None };
        if !matches!(shape.kind, ShapeKind::Placeholder(Placeholder::Diagram)) {
            return None;
        }
        let slide = self.slide();
        let ids = slide.attached.get(&shape.id)?;
        let parts: Vec<Shape> = slide.shapes.iter().filter(|s| ids.contains(&s.id)).cloned().collect();
        (!parts.is_empty()).then_some((shape.id, parts))
    }

    /// Whether the selection is a SmartArt graphic that has its drawing or is a SmartArt group.
    pub(crate) fn can_convert_diagram(&self) -> bool {
        self.selected_diagram().is_some() || self.selected_smartart_group().is_some()
    }

    /// 図形に変換: the graphic's shapes, as copies of their own (grouped), in the graphic's place.
    pub(crate) fn convert_diagram_to_shapes(&mut self, cx: &mut Context<Self>) {
        if let Some((owner, parts)) = self.selected_diagram() {
            if !parts.iter().all(|s| ibis_pptx::save::check_shape_insertion(s).is_ok()) {
                return self.set_status(crate::shell::i18n::text("pptx.smartart.smartart-has-parts-cannot").to_string(), cx);
            }
            let slide = self.current_slide;
            let Some(at) = self.slide().shape_index(owner) else { return };
            let before = self.slide().shapes[..at].iter().filter(|s| !parts.iter().any(|p| p.id == s.id)).count();
            let copies: Vec<Shape> = parts
                .into_iter()
                .map(|mut s| {
                    s.id = self.document.ids().new_shape_id();
                    s
                })
                .collect();
            let ids: Vec<ShapeId> = copies.iter().map(|s| s.id).collect();
            let mut commands = vec![Command::RemoveShape { slide, shape: owner }];
            commands.extend(copies.into_iter().enumerate().map(|(i, shape)| Command::InsertShape { slide, index: before + i, shape }));
            if self.apply_pptx_commands_checked(vec![Command::Group(commands)], cx) {
                self.set_selection(ids);
                self.group_selected(cx);
            }
        } else if self.selected_smartart_group().is_some() {
            self.ungroup_selected(cx);
        }
    }

    /// Whether the selection can be converted to SmartArt (a text box or shape with text).
    pub(crate) fn can_convert_text_to_smartart(&self) -> bool {
        let selected = self.selected_shapes();
        let [shape] = selected.as_slice() else { return false };
        shape.text_body().is_some_and(|b| !b.is_empty())
    }

    /// Find the selected SmartArt group if any.
    pub(crate) fn selected_smartart_group(&self) -> Option<(usize, ShapeGroup)> {
        let slide = self.slide();
        let selected = self.selected_ids();
        if selected.is_empty() {
            return None;
        }
        for (idx, g) in slide.groups.iter().enumerate() {
            if g.name.starts_with("SmartArt") {
                let mut leaves = Vec::new();
                Self::collect_group_leaves(g, &mut leaves);
                if selected.iter().any(|id| leaves.contains(id)) {
                    return Some((idx, g.clone()));
                }
            }
        }
        None
    }

    /// Extract SmartArt group metadata, kind, color style, leaves, items, and bounding box.
    pub(crate) fn selected_smartart_data(&self) -> Option<(ShapeGroup, SmartArtKind, SmartArtColorStyle, Vec<ShapeId>, Vec<String>, EmuRect)> {
        let (_, group) = self.selected_smartart_group()?;
        let (kind, style) = Self::smartart_info_from_group_name(&group.name)?;
        let mut leaves = Vec::new();
        Self::collect_group_leaves(&group, &mut leaves);
        if leaves.is_empty() {
            return None;
        }
        let slide = self.slide();
        let mut current_items = Vec::new();
        let mut bounds: Option<EmuRect> = None;
        for &id in &leaves {
            if let Some(sh) = slide.shape(id) {
                bounds = Some(bounds.map_or(sh.frame, |b| union_rect(b, sh.frame)));
                if let Some(body) = sh.text_body() {
                    let text = body.text().trim().to_string();
                    if !text.is_empty() && !text.starts_with('📷') {
                        current_items.push(text);
                    }
                }
            }
        }
        let frame = bounds?;
        Some((group, kind, style, leaves, current_items, frame))
    }

    /// Find the item index corresponding to the currently selected shape, if any.
    pub(crate) fn selected_smartart_item_index(&self) -> Option<usize> {
        let (_, _, _, leaves, _, _) = self.selected_smartart_data()?;
        let selected_ids = self.selected_ids();
        let [selected_id] = selected_ids.as_slice() else {
            return None;
        };
        let slide = self.slide();
        let mut item_idx = 0;
        for &id in &leaves {
            if let Some(sh) = slide.shape(id) {
                if let Some(body) = sh.text_body() {
                    let text = body.text().trim().to_string();
                    if !text.is_empty() && !text.starts_with('📷') {
                        if id == *selected_id {
                            return Some(item_idx);
                        }
                        item_idx += 1;
                    }
                }
            }
        }
        None
    }

    /// Whether the selected SmartArt group can add/remove items.
    pub(crate) fn can_modify_smartart_items(&self) -> bool {
        self.selected_smartart_group().is_some()
    }

    /// Insert a SmartArt diagram of the specified preset with default items.
    pub(crate) fn insert_smartart(&mut self, kind: SmartArtKind, cx: &mut Context<Self>) {
        let defaults = kind.default_items();
        let defaults: Vec<&str> = defaults.iter().map(String::as_str).collect();
        self.insert_smartart_with_items(kind, &defaults, None, cx);
    }

    /// Insert a SmartArt diagram with specified kind and color style.
    pub(crate) fn insert_smartart_with_style(
        &mut self,
        kind: SmartArtKind,
        style: SmartArtColorStyle,
        items: &[&str],
        replaced: Option<(ShapeId, EmuRect)>,
        cx: &mut Context<Self>,
    ) -> Vec<ShapeId> {
        let (replaced_id, frame) = match replaced {
            Some((id, f)) => (Some(id), f),
            None => match self.empty_content_placeholder_for("dgm") {
                Some((ph_id, ph_frame)) => (Some(ph_id), ph_frame),
                None => {
                    let size = self.presentation().size_of(self.slide());
                    let width = (size.width * 75 / 100).min(9_000_000);
                    let height = (size.height * 60 / 100).min(4_500_000);
                    let x = (size.width - width) / 2;
                    let y = (size.height - height) / 2;
                    (None, EmuRect::new(x, y, width, height))
                }
            },
        };

        let mut shapes = Vec::new();
        let mut ids = Vec::new();
        self.build_smartart_shapes(kind, style, items, frame, &mut shapes, &mut ids);

        let slide = self.current_slide;
        let mut commands = Vec::new();
        if let Some(old) = replaced_id {
            commands.push(Command::RemoveShape { slide, shape: old });
        }
        let start_index = self.slide().shapes.len() - usize::from(replaced_id.is_some());
        for (i, shape) in shapes.into_iter().enumerate() {
            commands.push(Command::InsertShape { slide, index: start_index + i, shape });
        }

        if self.apply_pptx_commands_checked(vec![Command::Group(commands)], cx) {
            self.set_selection(ids.clone());
            self.group_smartart(kind, style, cx);
        }
        ids
    }

    /// Convert the selected text box / shape's text paragraphs into a SmartArt diagram.
    pub(crate) fn convert_selected_text_to_smartart(&mut self, kind: SmartArtKind, cx: &mut Context<Self>) {
        let selected = self.selected_shapes();
        let [shape] = selected.as_slice() else { return };
        let Some(body) = shape.text_body() else { return };
        let items: Vec<String> = body
            .paragraphs
            .iter()
            .map(|p| p.text().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if items.is_empty() {
            return;
        }
        let items_ref: Vec<&str> = items.iter().map(String::as_str).collect();
        let frame = shape.frame;
        let old_id = shape.id;
        self.insert_smartart_with_items(kind, &items_ref, Some((old_id, frame)), cx);
    }

    /// Internal layout & insertion engine for SmartArt presets.
    pub(crate) fn insert_smartart_with_items(
        &mut self,
        kind: SmartArtKind,
        items: &[&str],
        replaced: Option<(ShapeId, EmuRect)>,
        cx: &mut Context<Self>,
    ) -> Vec<ShapeId> {
        self.insert_smartart_with_style(kind, SmartArtColorStyle::Colorful, items, replaced, cx)
    }

    fn build_smartart_shapes(
        &mut self,
        kind: SmartArtKind,
        style: SmartArtColorStyle,
        items: &[&str],
        frame: EmuRect,
        shapes: &mut Vec<Shape>,
        ids: &mut Vec<ShapeId>,
    ) {
        match kind {
            SmartArtKind::BasicBlockList => {
                let n = items.len().max(1);
                let gap = (frame.width / 40).max(100_000);
                let item_w = (frame.width - (n - 1) as i64 * gap) / n as i64;
                let item_h = frame.height * 70 / 100;
                let item_y = frame.y + (frame.height - item_h) / 2;
                for (i, text) in items.iter().enumerate() {
                    let id = self.document.ids().new_shape_id();
                    let x = frame.x + i as i64 * (item_w + gap);
                    let rect = EmuRect::new(x, item_y, item_w, item_h);
                    let (fill, stroke, text_color) = style.palette(i);
                    shapes.push(make_smartart_node(id, "rect", rect, fill, stroke, text_color, text));
                    ids.push(id);
                }
            }
            SmartArtKind::VerticalList => {
                let n = items.len().max(1);
                let gap = (frame.height / 30).max(80_000);
                let item_h = (frame.height - (n - 1) as i64 * gap) / n as i64;
                let item_w = frame.width * 85 / 100;
                let item_x = frame.x + (frame.width - item_w) / 2;
                for (i, text) in items.iter().enumerate() {
                    let id = self.document.ids().new_shape_id();
                    let y = frame.y + i as i64 * (item_h + gap);
                    let rect = EmuRect::new(item_x, y, item_w, item_h);
                    let (fill, stroke, text_color) = style.palette(i);
                    shapes.push(make_smartart_node(id, "roundRect", rect, fill, stroke, text_color, text));
                    ids.push(id);
                }
            }
            SmartArtKind::BasicProcess => {
                let n = items.len().max(1);
                let arrow_w = if n > 1 { (frame.width * 14 / 100) / (n - 1) as i64 } else { 0 };
                let gap = if n > 1 { (frame.width * 4 / 100) / (n - 1) as i64 } else { 0 };
                let total_arrow = (n - 1) as i64 * (arrow_w + gap);
                let block_w = (frame.width - total_arrow) / n as i64;
                let block_h = frame.height * 65 / 100;
                let block_y = frame.y + (frame.height - block_h) / 2;
                let arrow_h = frame.height * 24 / 100;
                let arrow_y = frame.y + (frame.height - arrow_h) / 2;
                let mut cur_x = frame.x;
                for (i, text) in items.iter().enumerate() {
                    let id = self.document.ids().new_shape_id();
                    let rect = EmuRect::new(cur_x, block_y, block_w, block_h);
                    let (fill, stroke, text_color) = style.palette(i);
                    shapes.push(make_smartart_node(id, "roundRect", rect, fill, stroke, text_color, text));
                    ids.push(id);
                    cur_x += block_w;
                    if i + 1 < n {
                        let arr_id = self.document.ids().new_shape_id();
                        let arr_rect = EmuRect::new(cur_x + gap / 2, arrow_y, arrow_w, arrow_h);
                        let arr_color = match style {
                            SmartArtColorStyle::Outline => Rgb(0x4472c4),
                            _ => Rgb(0xa5a5a5),
                        };
                        shapes.push(make_smartart_arrow(arr_id, "rightArrow", arr_rect, arr_color));
                        ids.push(arr_id);
                        cur_x += arrow_w + gap;
                    }
                }
            }
            SmartArtKind::ContinuousChevronProcess => {
                let n = items.len().max(1);
                let overlap = (frame.width * 3 / 100).max(50_000);
                let item_w = (frame.width + (n - 1) as i64 * overlap) / n as i64;
                let item_h = frame.height * 70 / 100;
                let item_y = frame.y + (frame.height - item_h) / 2;
                for (i, text) in items.iter().enumerate() {
                    let id = self.document.ids().new_shape_id();
                    let x = frame.x + i as i64 * (item_w - overlap);
                    let rect = EmuRect::new(x, item_y, item_w, item_h);
                    let (fill, stroke, text_color) = style.palette(i);
                    shapes.push(make_smartart_node(id, "chevron", rect, fill, stroke, text_color, text));
                    ids.push(id);
                }
            }
            SmartArtKind::BasicCycle => {
                let n = items.len().max(1);
                let cx = frame.x + frame.width / 2;
                let cy = frame.y + frame.height / 2;
                let node_w = (frame.width * 26 / 100).max(500_000);
                let node_h = (frame.height * 28 / 100).max(400_000);
                let rx = (frame.width - node_w) / 2;
                let ry = (frame.height - node_h) / 2;
                for (i, text) in items.iter().enumerate() {
                    let id = self.document.ids().new_shape_id();
                    let theta = -std::f64::consts::FRAC_PI_2 + (2.0 * std::f64::consts::PI * i as f64 / n as f64);
                    let nx = cx + (rx as f64 * theta.cos()) as i64 - node_w / 2;
                    let ny = cy + (ry as f64 * theta.sin()) as i64 - node_h / 2;
                    let rect = EmuRect::new(nx, ny, node_w, node_h);
                    let (fill, stroke, text_color) = style.palette(i);
                    shapes.push(make_smartart_node(id, "ellipse", rect, fill, stroke, text_color, text));
                    ids.push(id);
                }
            }
            SmartArtKind::OrganizationChart => {
                let top_w = frame.width * 36 / 100;
                let top_h = frame.height * 30 / 100;
                let top_x = frame.x + (frame.width - top_w) / 2;
                let top_y = frame.y;
                let id0 = self.document.ids().new_shape_id();
                let (fill0, stroke0, text0) = style.palette(0);
                shapes.push(make_smartart_node(id0, "rect", EmuRect::new(top_x, top_y, top_w, top_h), fill0, stroke0, text0, items.first().copied().unwrap_or(&*crate::shell::i18n::text("pptx.smartart.item.leader"))));
                ids.push(id0);

                let fallback_a = crate::shell::i18n::text("pptx.smartart.item.member-a");
                let fallback_b = crate::shell::i18n::text("pptx.smartart.item.member-b");
                let fallback_rest = [&*fallback_a, &*fallback_b];
                let rest = if items.len() > 1 { &items[1..] } else { &fallback_rest[..] };
                let (asst, children) = if !rest.is_empty() && (rest[0].starts_with("アシスタント") /* i18n-ok（入力の照合の語） */ || rest[0].starts_with("Assistant")) {
                    (Some(rest[0]), &rest[1..])
                } else {
                    (None, rest)
                };

                if let Some(asst_text) = asst {
                    let id_a = self.document.ids().new_shape_id();
                    let asst_w = top_w * 80 / 100;
                    let asst_h = top_h * 75 / 100;
                    let asst_x = frame.x + frame.width * 15 / 100;
                    let asst_y = frame.y + top_h + (frame.height - top_h - top_h) * 20 / 100;
                    let (fill_a, stroke_a, text_a) = style.palette(5);
                    shapes.push(make_smartart_node(id_a, "roundRect", EmuRect::new(asst_x, asst_y, asst_w, asst_h), fill_a, stroke_a, text_a, asst_text));
                    ids.push(id_a);
                }

                let m = children.len().max(1);
                let child_w = (frame.width * 80 / 100) / m as i64;
                let child_h = frame.height * 30 / 100;
                let child_y = frame.y + frame.height - child_h;
                let child_gap = (frame.width - m as i64 * child_w) / (m + 1) as i64;
                for (j, text) in children.iter().enumerate() {
                    let id_c = self.document.ids().new_shape_id();
                    let cx = frame.x + child_gap + j as i64 * (child_w + child_gap);
                    let (fill_c, stroke_c, text_c) = style.palette((j + 1) % 6);
                    shapes.push(make_smartart_node(id_c, "roundRect", EmuRect::new(cx, child_y, child_w, child_h), fill_c, stroke_c, text_c, text));
                    ids.push(id_c);
                }
            }
            SmartArtKind::BasicRelationship => {
                let node_w = frame.width * 38 / 100;
                let node_h = frame.height * 60 / 100;
                let node_y = frame.y + (frame.height - node_h) / 2;
                let id_l = self.document.ids().new_shape_id();
                let (fill0, stroke0, text0) = style.palette(0);
                shapes.push(make_smartart_node(id_l, "roundRect", EmuRect::new(frame.x, node_y, node_w, node_h), fill0, stroke0, text0, items.first().copied().unwrap_or(&*crate::shell::i18n::text("pptx.smartart.item.item-a"))));
                ids.push(id_l);

                let arr_w = frame.width * 16 / 100;
                let arr_h = frame.height * 24 / 100;
                let arr_x = frame.x + (frame.width - arr_w) / 2;
                let arr_y = frame.y + (frame.height - arr_h) / 2;
                let id_arr = self.document.ids().new_shape_id();
                let arr_color = match style {
                    SmartArtColorStyle::Outline => Rgb(0x4472c4),
                    _ => Rgb(0x5b9bd5),
                };
                shapes.push(make_smartart_arrow(id_arr, "leftRightArrow", EmuRect::new(arr_x, arr_y, arr_w, arr_h), arr_color));
                ids.push(id_arr);

                let id_r = self.document.ids().new_shape_id();
                let (fill1, stroke1, text1) = style.palette(1);
                shapes.push(make_smartart_node(id_r, "roundRect", EmuRect::new(frame.right() - node_w, node_y, node_w, node_h), fill1, stroke1, text1, items.get(1).copied().unwrap_or(&*crate::shell::i18n::text("pptx.smartart.item.item-b"))));
                ids.push(id_r);
            }
            SmartArtKind::BasicMatrix => {
                let gap_x = frame.width * 4 / 100;
                let gap_y = frame.height * 5 / 100;
                let cell_w = (frame.width - gap_x) / 2;
                let cell_h = (frame.height - gap_y) / 2;
                let coords = [(0, 0), (1, 0), (0, 1), (1, 1)];
                for (idx, &(c, r)) in coords.iter().enumerate() {
                    let id = self.document.ids().new_shape_id();
                    let x = frame.x + c * (cell_w + gap_x);
                    let y = frame.y + r * (cell_h + gap_y);
                    let quadrant = crate::shell::i18n::text("pptx.smartart.item.quadrant");
                    let text = items.get(idx).copied().unwrap_or(&*quadrant);
                    let (fill, stroke, text_color) = style.palette(idx);
                    shapes.push(make_smartart_node(id, "rect", EmuRect::new(x, y, cell_w, cell_h), fill, stroke, text_color, text));
                    ids.push(id);
                }
            }
            SmartArtKind::BasicPyramid => {
                let n = items.len().max(3);
                let tier_h = frame.height / n as i64;
                for (i, text) in items.iter().take(n).enumerate() {
                    let id = self.document.ids().new_shape_id();
                    let frac = (i + 1) as f64 / n as f64;
                    let w = (frame.width as f64 * (0.35 + 0.65 * frac)) as i64;
                    let x = frame.x + (frame.width - w) / 2;
                    let y = frame.y + i as i64 * tier_h;
                    let geom = if i == 0 { "triangle" } else { "trapezoid" };
                    let (fill, stroke, text_color) = style.palette(i);
                    shapes.push(make_smartart_node(id, geom, EmuRect::new(x, y, w, tier_h * 9 / 10), fill, stroke, text_color, text));
                    ids.push(id);
                }
            }
            SmartArtKind::PictureList => {
                let n = items.len().max(1);
                let gap = (frame.width * 4 / 100).max(60_000);
                let item_w = (frame.width - (n - 1) as i64 * gap) / n as i64;
                let circle_size = (item_w * 55 / 100).min(frame.height * 40 / 100);
                let card_h = frame.height * 48 / 100;
                let card_y = frame.y + circle_size + frame.height * 4 / 100;
                for (i, text) in items.iter().enumerate() {
                    let x = frame.x + i as i64 * (item_w + gap);
                    let circle_x = x + (item_w - circle_size) / 2;
                    let circle_id = self.document.ids().new_shape_id();
                    let (fill, stroke, text_color) = style.palette(i);
                    shapes.push(make_smartart_node(circle_id, "ellipse", EmuRect::new(circle_x, frame.y, circle_size, circle_size), fill, stroke, text_color, "📷"));
                    ids.push(circle_id);

                    let card_id = self.document.ids().new_shape_id();
                    let card_fill = match style {
                        SmartArtColorStyle::Outline => Rgb::WHITE,
                        _ => fill.tinted(650_000),
                    };
                    let card = make_smartart_node(card_id, "roundRect", EmuRect::new(x, card_y, item_w, card_h), card_fill, stroke, Rgb::BLACK, text);
                    shapes.push(card);
                    ids.push(card_id);
                }
            }
        }
    }

    fn group_smartart(&mut self, kind: SmartArtKind, style: SmartArtColorStyle, cx: &mut Context<Self>) {
        let selected_ids = self.selected_ids();
        if selected_ids.is_empty() {
            return;
        }
        let Some(slide) = self.presentation().slide(self.current_slide).cloned() else { return };
        let mut children = Vec::new();
        let mut remaining_groups = Vec::new();
        let mut consumed = std::collections::HashSet::new();

        for g in &slide.groups {
            let mut leaves = Vec::new();
            Self::collect_group_leaves(g, &mut leaves);
            if leaves.iter().any(|id| selected_ids.contains(id)) {
                children.push(ibis_doc_model::GroupMember::Group(Box::new(g.clone())));
                consumed.extend(leaves);
            } else {
                remaining_groups.push(g.clone());
            }
        }
        for shape in &slide.shapes {
            if selected_ids.contains(&shape.id) && !consumed.contains(&shape.id) {
                children.push(ibis_doc_model::GroupMember::Shape(shape.id));
                consumed.insert(shape.id);
            }
        }
        if children.is_empty() {
            return;
        }

        let mut used_ids = std::collections::HashSet::new();
        for g in &slide.groups {
            if let Ok(id) = g.native_id.parse::<u32>() {
                used_ids.insert(id);
            }
        }
        let mut next_native_id = 20000u32;
        while used_ids.contains(&next_native_id) {
            next_native_id += 1;
        }

        let group_name = if style == SmartArtColorStyle::Colorful {
            format!("SmartArt: {}", kind.label())
        } else {
            format!("SmartArt: {} [{}]", kind.label(), style.code())
        };

        let group = ibis_doc_model::ShapeGroup {
            has_unmodelled_children: false,
            native_id: next_native_id.to_string(),
            name: group_name,
            children,
        };
        remaining_groups.push(group);
        let commands = vec![Command::SetShapeGroups { slide: self.current_slide, groups: remaining_groups }];
        self.apply_pptx_commands_checked(vec![Command::Group(commands)], cx);
    }

    pub(crate) fn smartart_info_from_group_name(name: &str) -> Option<(SmartArtKind, SmartArtColorStyle)> {
        let label = name.strip_prefix("SmartArt: ")?;
        let (kind_part, style) = if let Some(open) = label.find('[') {
            let close = label.find(']').unwrap_or(label.len());
            let style_str = &label[open + 1..close];
            (label[..open].trim(), SmartArtColorStyle::from_code(style_str))
        } else {
            (label.trim(), SmartArtColorStyle::Colorful)
        };
        for &k in &[
            SmartArtKind::BasicBlockList,
            SmartArtKind::VerticalList,
            SmartArtKind::BasicProcess,
            SmartArtKind::ContinuousChevronProcess,
            SmartArtKind::BasicCycle,
            SmartArtKind::OrganizationChart,
            SmartArtKind::BasicRelationship,
            SmartArtKind::BasicMatrix,
            SmartArtKind::BasicPyramid,
            SmartArtKind::PictureList,
        ] {
            if kind_part == k.label() {
                return Some((k, style));
            }
        }
        None
    }

    #[allow(dead_code)]
    pub(crate) fn smartart_kind_from_group_name(name: &str) -> Option<SmartArtKind> {
        Self::smartart_info_from_group_name(name).map(|(k, _)| k)
    }
}

/// Helper function to parse SmartArt kind and color style from group name.
pub(crate) fn smartart_info_from_group_name(name: &str) -> Option<(SmartArtKind, SmartArtColorStyle)> {
    SlideEditor::smartart_info_from_group_name(name)
}

impl SlideEditor {

    /// レイアウト変更 (PPTX-SMART-013): 既存テキスト・枠サイズを維持して別の SmartArt プリセットへ変換。
    pub(crate) fn change_smartart_layout(&mut self, new_kind: SmartArtKind, cx: &mut Context<Self>) {
        let Some((_, _, style, leaves, items, frame)) = self.selected_smartart_data() else { return };
        let items_ref: Vec<&str> = items.iter().map(String::as_str).collect();
        self.replace_smartart_shapes(&leaves, new_kind, style, &items_ref, frame, cx);
    }

    /// 色とSmartArtスタイル (PPTX-SMART-014): カラフル・アクセント・枠線のみ等の配色スタイルを適用。
    pub(crate) fn set_smartart_color_style(&mut self, new_style: SmartArtColorStyle, cx: &mut Context<Self>) {
        let Some((_, kind, _, leaves, items, frame)) = self.selected_smartart_data() else { return };
        let items_ref: Vec<&str> = items.iter().map(String::as_str).collect();
        self.replace_smartart_shapes(&leaves, kind, new_style, &items_ref, frame, cx);
    }

    /// Add a new item into the selected SmartArt group and re-layout (PPTX-SMART-010).
    pub(crate) fn smartart_add_shape(&mut self, cx: &mut Context<Self>) {
        let Some((_, kind, style, leaves, mut items, frame)) = self.selected_smartart_data() else { return };
        let next_num = items.len() + 1;
        items.push(crate::shell::i18n::format("pptx.smartart.item.item-n", &[("n", &next_num)]));
        let items_ref: Vec<&str> = items.iter().map(String::as_str).collect();
        self.replace_smartart_shapes(&leaves, kind, style, &items_ref, frame, cx);
    }

    /// Remove the last item from the selected SmartArt group and re-layout (PPTX-SMART-010).
    pub(crate) fn smartart_remove_shape(&mut self, cx: &mut Context<Self>) {
        let Some((_, kind, style, leaves, mut items, frame)) = self.selected_smartart_data() else { return };
        if items.len() <= 1 {
            return;
        }
        items.pop();
        let items_ref: Vec<&str> = items.iter().map(String::as_str).collect();
        self.replace_smartart_shapes(&leaves, kind, style, &items_ref, frame, cx);
    }

    /// アシスタントの追加 (PPTX-SMART-012): 組織図にアシスタント ノードを追加。
    pub(crate) fn smartart_add_assistant(&mut self, cx: &mut Context<Self>) {
        let Some((_, kind, style, leaves, mut items, frame)) = self.selected_smartart_data() else { return };
        let insert_at = if items.is_empty() { 0 } else { 1 };
        items.insert(insert_at, crate::shell::i18n::text("pptx.smartart.item.assistant").to_string());
        let items_ref: Vec<&str> = items.iter().map(String::as_str).collect();
        self.replace_smartart_shapes(&leaves, kind, style, &items_ref, frame, cx);
    }

    /// 上へ移動 (PPTX-SMART-011): 選択された項目の順序を 1 つ前へ移動。
    pub(crate) fn smartart_move_item_up(&mut self, cx: &mut Context<Self>) {
        let Some((_, kind, style, leaves, mut items, frame)) = self.selected_smartart_data() else { return };
        if items.len() <= 1 {
            return;
        }
        let idx = self.selected_smartart_item_index().unwrap_or(items.len() - 1);
        if idx > 0 && idx < items.len() {
            items.swap(idx, idx - 1);
            let items_ref: Vec<&str> = items.iter().map(String::as_str).collect();
            self.replace_smartart_shapes_with_selection(&leaves, kind, style, &items_ref, frame, Some(idx - 1), cx);
        }
    }

    /// 下へ移動 (PPTX-SMART-011): 選択された項目の順序を 1 つ後ろへ移動。
    pub(crate) fn smartart_move_item_down(&mut self, cx: &mut Context<Self>) {
        let Some((_, kind, style, leaves, mut items, frame)) = self.selected_smartart_data() else { return };
        if items.len() <= 1 {
            return;
        }
        let idx = self.selected_smartart_item_index().unwrap_or(0);
        if idx + 1 < items.len() {
            items.swap(idx, idx + 1);
            let items_ref: Vec<&str> = items.iter().map(String::as_str).collect();
            self.replace_smartart_shapes_with_selection(&leaves, kind, style, &items_ref, frame, Some(idx + 1), cx);
        }
    }

    /// レベル上げ (PPTX-SMART-011): 選択された項目を上位レベル（先頭）へ移動。
    pub(crate) fn smartart_promote(&mut self, cx: &mut Context<Self>) {
        let Some((_, kind, style, leaves, mut items, frame)) = self.selected_smartart_data() else { return };
        if items.len() <= 1 {
            return;
        }
        let idx = self.selected_smartart_item_index().unwrap_or(items.len() - 1);
        if idx > 0 && idx < items.len() {
            let item = items.remove(idx);
            items.insert(0, item);
            let items_ref: Vec<&str> = items.iter().map(String::as_str).collect();
            self.replace_smartart_shapes_with_selection(&leaves, kind, style, &items_ref, frame, Some(0), cx);
        }
    }

    /// レベル下げ (PPTX-SMART-011): 選択された項目を下位レベル（末尾）へ移動。
    pub(crate) fn smartart_demote(&mut self, cx: &mut Context<Self>) {
        let Some((_, kind, style, leaves, mut items, frame)) = self.selected_smartart_data() else { return };
        if items.len() <= 1 {
            return;
        }
        let idx = self.selected_smartart_item_index().unwrap_or(0);
        if idx < items.len() - 1 {
            let item = items.remove(idx);
            let new_idx = items.len();
            items.push(item);
            let items_ref: Vec<&str> = items.iter().map(String::as_str).collect();
            self.replace_smartart_shapes_with_selection(&leaves, kind, style, &items_ref, frame, Some(new_idx), cx);
        }
    }

    /// 図形ごとの書式 (PPTX-SMART-015): SmartArt 内の個別図形に塗りや枠線を設定。
    pub(crate) fn format_smartart_shape(
        &mut self,
        shape_id: ShapeId,
        fill: Option<Fill>,
        stroke: Option<Option<Stroke>>,
        cx: &mut Context<Self>,
    ) {
        let slide = self.current_slide;
        let mut commands = Vec::new();
        if let Some(f) = fill {
            commands.push(Command::SetFill { slide, shape: shape_id, fill: f });
        }
        if let Some(st) = stroke {
            commands.push(Command::SetStroke { slide, shape: shape_id, stroke: st });
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
    }

    /// SmartArt 全項目のテキストを一括更新。
    pub(crate) fn smartart_set_items(&mut self, new_items: &[&str], cx: &mut Context<Self>) {
        let Some((_, kind, style, leaves, _, frame)) = self.selected_smartart_data() else { return };
        self.replace_smartart_shapes(&leaves, kind, style, new_items, frame, cx);
    }

    fn replace_smartart_shapes(
        &mut self,
        old_leaves: &[ShapeId],
        kind: SmartArtKind,
        style: SmartArtColorStyle,
        items: &[&str],
        frame: EmuRect,
        cx: &mut Context<Self>,
    ) {
        self.replace_smartart_shapes_with_selection(old_leaves, kind, style, items, frame, None, cx);
    }

    fn replace_smartart_shapes_with_selection(
        &mut self,
        old_leaves: &[ShapeId],
        kind: SmartArtKind,
        style: SmartArtColorStyle,
        items: &[&str],
        frame: EmuRect,
        select_item_index: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let slide = self.current_slide;
        let mut commands = Vec::new();
        commands.push(Command::RemoveShapes { slide, shapes: old_leaves.to_vec() });
        let mut new_shapes = Vec::new();
        let mut new_ids = Vec::new();
        self.build_smartart_shapes(kind, style, items, frame, &mut new_shapes, &mut new_ids);
        let removed_count = old_leaves.iter().filter(|id| self.slide().shape(**id).is_some()).count();
        let at = self.slide().shapes.len().saturating_sub(removed_count);
        for (i, shape) in new_shapes.into_iter().enumerate() {
            commands.push(Command::InsertShape { slide, index: at + i, shape });
        }
        // Rebuilding the nodes and their group is one user operation and one Undo step.
        // Preserve the old SmartArt group's native id; other groups remain untouched.
        let mut groups = Vec::new();
        let mut native_id = None;
        for group in &self.slide().groups {
            let mut leaves = Vec::new();
            Self::collect_group_leaves(group, &mut leaves);
            if leaves.iter().any(|id| old_leaves.contains(id)) {
                native_id = Some(group.native_id.clone());
            } else {groups.push(group.clone());}
        }
        let native_id = native_id.unwrap_or_else(|| {
            let used = groups.iter().map(|g| g.native_id.as_str()).collect::<std::collections::HashSet<_>>();
            let mut id = 20000u32;
            while used.contains(id.to_string().as_str()) {id += 1;}
            id.to_string()
        });
        let name = if style == SmartArtColorStyle::Colorful {format!("SmartArt: {}", kind.label())} else {format!("SmartArt: {} [{}]", kind.label(), style.code())};
        groups.push(ibis_doc_model::ShapeGroup {has_unmodelled_children:false,native_id,name,children:new_ids.iter().copied().map(ibis_doc_model::GroupMember::Shape).collect()});
        commands.push(Command::SetShapeGroups {slide,groups});
        if self.apply_pptx_commands_checked(vec![Command::Group(commands)], cx) {
            self.set_selection(new_ids.clone());
            if let Some(target_idx) = select_item_index {
                let item_shapes: Vec<ShapeId> = new_ids.into_iter().filter(|id| {
                    self.slide().shape(*id).and_then(|s| s.text_body()).is_some_and(|b| {
                        let text = b.text();
                        let t = text.trim();
                        !t.is_empty() && !t.starts_with('📷')
                    })
                }).collect();
                if let Some(&target_shape_id) = item_shapes.get(target_idx) {
                    self.set_selection(vec![target_shape_id]);
                }
            }
        }
    }

    /// テキスト ウィンドウ (PPTX-SMART-009): SmartArt テキスト ウィンドウの表示・非表示を切り替え。
    pub(crate) fn toggle_smartart_text_pane(&mut self, cx: &mut Context<Self>) {
        let shown = self.pptx().is_some_and(|s| s.smartart_text_pane);
        if let Some(s) = self.pptx_mut() {
            s.smartart_text_pane = !shown;
            s.smartart_reorder = Default::default();
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Select the SmartArt node shape corresponding to item `index` (PPTX-SMART-009).
    pub(crate) fn select_smartart_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((_, _, _, leaves, _, _)) = self.selected_smartart_data() else { return };
        let slide = self.slide();
        let item_shapes: Vec<ShapeId> = leaves
            .into_iter()
            .filter(|id| {
                slide.shape(*id).and_then(|s| s.text_body()).is_some_and(|b| {
                    let t = b.text().trim().to_string();
                    !t.is_empty() && !t.starts_with('📷')
                })
            })
            .collect();
        if let Some(&target_shape_id) = item_shapes.get(index) {
            self.set_selection(vec![target_shape_id]);
            cx.notify();
        }
    }

    /// Context menu entries for row `index` in the SmartArt Text Pane (PPTX-SMART-009..014).
    pub(crate) fn smartart_text_pane_context_menu_entries(&self, index: usize) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
        let Some((_, kind, style, _, items, _)) = self.selected_smartart_data() else { return Vec::new() };
        let n = items.len();
        let mut entries: Vec<MenuEntry> = vec![
            MenuItem::tr("pptx.smartart.add-shape").icon("add_box").on(|e, _, cx| e.smartart_add_shape(cx)).into(),
        ];
        if kind == SmartArtKind::OrganizationChart {
            entries.push(MenuItem::tr("pptx.smartart.add-assistant").icon("person_add").on(|e, _, cx| e.smartart_add_assistant(cx)).into());
        }
        entries.push(
            MenuItem::tr("pptx.smartart.remove-shape")
                .icon("indeterminate_check_box")
                .disabled(n <= 1 || index >= n)
                .on(move |e, _, cx| {
                    if let Some((_, kind, style, leaves, mut it, frame)) = e.selected_smartart_data() {
                        if it.len() > 1 && index < it.len() {
                            it.remove(index);
                            let r: Vec<&str> = it.iter().map(String::as_str).collect();
                            let next_sel = index.min(r.len().saturating_sub(1));
                            e.replace_smartart_shapes_with_selection(&leaves, kind, style, &r, frame, Some(next_sel), cx);
                        }
                    }
                })
                .into(),
        );
        entries.push(MenuEntry::Separator);
        entries.push(
            MenuItem::tr("pptx.smartart.promote")
                .icon("format_indent_decrease")
                .disabled(n <= 1 || index == 0 || index >= n)
                .on(move |e, _, cx| {
                    e.select_smartart_item(index, cx);
                    e.smartart_promote(cx);
                })
                .into(),
        );
        entries.push(
            MenuItem::tr("pptx.smartart.demote")
                .icon("format_indent_increase")
                .disabled(n <= 1 || index + 1 >= n)
                .on(move |e, _, cx| {
                    e.select_smartart_item(index, cx);
                    e.smartart_demote(cx);
                })
                .into(),
        );
        entries.push(
            MenuItem::tr("pptx.smartart.move-up")
                .icon("arrow_upward")
                .disabled(n <= 1 || index == 0 || index >= n)
                .on(move |e, _, cx| {
                    e.select_smartart_item(index, cx);
                    e.smartart_move_item_up(cx);
                })
                .into(),
        );
        entries.push(
            MenuItem::tr("pptx.smartart.move-down")
                .icon("arrow_downward")
                .disabled(n <= 1 || index + 1 >= n)
                .on(move |e, _, cx| {
                    e.select_smartart_item(index, cx);
                    e.smartart_move_item_down(cx);
                })
                .into(),
        );
        entries.push(MenuEntry::Separator);
        let layout_items: Vec<MenuEntry> = [
            (SmartArtKind::BasicBlockList, "pptx.smartart.layout.list", "view_Agenda"),
            (SmartArtKind::VerticalList, "pptx.smartart.layout.vlist", "format_list_bulleted"),
            (SmartArtKind::BasicProcess, "pptx.smartart.layout.process", "trending_flat"),
            (SmartArtKind::ContinuousChevronProcess, "pptx.smartart.layout.chevron", "double_arrow"),
            (SmartArtKind::BasicCycle, "pptx.smartart.layout.cycle", "autorenew"),
            (SmartArtKind::OrganizationChart, "pptx.smartart.layout.hierarchy", "account_tree"),
            (SmartArtKind::BasicRelationship, "pptx.smartart.layout.relationship", "compare_arrows"),
            (SmartArtKind::BasicMatrix, "pptx.smartart.layout.matrix", "grid_view"),
            (SmartArtKind::BasicPyramid, "pptx.smartart.layout.pyramid", "change_history"),
            (SmartArtKind::PictureList, "pptx.smartart.layout.picture", "perm_media"),
        ]
        .into_iter()
        .map(|(k, id, icon)| {
            MenuItem::tr(id)
                .icon(icon)
                .checked(kind == k)
                .on(move |e, _, cx| e.change_smartart_layout(k, cx))
                .into()
        })
        .collect();
        entries.push(Submenu::tr("pptx.smartart.change-layout", "account_tree", layout_items).into());

        let color_items: Vec<MenuEntry> = [
            (SmartArtColorStyle::Colorful, "pptx.smartart.color.colorful"),
            (SmartArtColorStyle::Accent1, "pptx.smartart.color.accent1"),
            (SmartArtColorStyle::Accent2, "pptx.smartart.color.accent2"),
            (SmartArtColorStyle::Accent3, "pptx.smartart.color.accent3"),
            (SmartArtColorStyle::Outline, "pptx.smartart.color.outline"),
        ]
        .into_iter()
        .map(|(st, id)| {
            MenuItem::tr(id)
                .icon("palette")
                .checked(style == st)
                .on(move |e, _, cx| e.set_smartart_color_style(st, cx))
                .into()
        })
        .collect();
        entries.push(Submenu::tr("pptx.smartart.color", "palette", color_items).into());
        entries.push(MenuEntry::Separator);
        entries.push(MenuItem::tr("pptx.smartart-to-shapes").icon("widgets").on(|e, _, cx| e.convert_diagram_to_shapes(cx)).into());
        entries
    }

    /// Open context menu for row `index` in the SmartArt Text Pane at pointer `position`.
    pub(crate) fn open_smartart_text_pane_context_menu(&mut self, index: usize, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.select_smartart_item(index, cx);
        let entries = self.smartart_text_pane_context_menu_entries(index);
        if !entries.is_empty() {
            self.open_popup_entries(entries, position, window, cx);
        }
    }

    /// テキスト ウィンドウ サイドバー パネル (PPTX-SMART-009).
    pub(crate) fn smartart_text_pane(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        if !self.pptx()?.smartart_text_pane {
            return None;
        }
        let t = |k| crate::shell::i18n::text(k);
        let title = t("pptx.smartart.text-pane");

        let data = self.selected_smartart_data();
        let selected_idx = self.selected_smartart_item_index();
        let mut body = v_flex().gap_2().px(px(14.)).py(px(10.)).p_0();

        if let Some((_, kind, style, _leaves, items, _frame)) = data {
            let header = v_flex().w_full().min_w_0().gap_1()
                .child(div().text_sm().font_weight(gpui_kit::FontWeight::BOLD).child(format!("{}: {}", t("pptx.smartart.change-layout"), kind.display_label())))
                .child(div().text_sm().text_color(rgb(0x5f6368)).child(format!("({})", style.label())));
            body = body.child(header);

            let reorder = self.pptx()?.smartart_reorder.clone();
            let table_top = h_flex().w_full().justify_end().child(crate::shell::chrome::action_button("smart-add").small().label(t("pptx.smartart.add-item-short")).on_click(cx.listener(|e, _, _, cx| e.smartart_add_shape(cx))));
            body = body.child(table_top);
            let mut rows = Vec::new();
            for (i, text) in items.iter().enumerate() {
                let row = h_flex().id(("smart-row", i)).w_full().h_full().min_w_0().gap_1().items_center().px_1()
                    .on_click(cx.listener(move |e, _, _, cx| e.select_smartart_item(i, cx)))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |e, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation(); e.open_smartart_text_pane_context_menu(i, event.position, window, cx);
                    }))
                    .child(div().w(px(20.)).text_sm().text_color(rgb(0x5f6368)).child(format!("{}.", i + 1)))
                    .child(div().flex_1().min_w_0().overflow_hidden().text_sm().child(text.clone()))
                    .child(crate::shell::chrome::data_table::row_controls(reorder.clone(), i, 0,
                        crate::shell::chrome::action_button(("smart-del", i)).small().w(px(28.)).icon(crate::shell::chrome::icon("delete", 18., rgb(0x3c4043).into())).disabled(items.len() <= 1).on_click(cx.listener(move |e, _, _, cx| {
                            cx.stop_propagation();
                            if let Some((_, kind, style, leaves, mut it, frame)) = e.selected_smartart_data() {
                                if it.len() > 1 && i < it.len() {it.remove(i); let r: Vec<&str> = it.iter().map(String::as_str).collect();e.replace_smartart_shapes(&leaves, kind, style, &r, frame, cx);}
                            }
                        }))))
                    .child(crate::shell::chrome::icon("menu", 18., rgb(0x5f6368).into()).cursor_grab());
                rows.push((i, row.into_any_element(), selected_idx == Some(i)));
            }
            let table = crate::shell::chrome::data_table::DataTable::new("pptx-smartart-text-table", None, rows, cx).reorderable(reorder, |e, from, to, _, cx| {
                if let Some((_, kind, style, leaves, mut it, frame)) = e.selected_smartart_data() {
                    if from < it.len() && to < it.len() {let item = it.remove(from); it.insert(to, item);let r: Vec<&str> = it.iter().map(String::as_str).collect();e.replace_smartart_shapes(&leaves, kind, style, &r, frame, cx);}
                }
            });
            body = body.child(table);
            if kind == SmartArtKind::OrganizationChart {
                body = body.child(crate::shell::chrome::action_button("smart-add-asst").small().label(t("pptx.smartart.add-assistant")).on_click(cx.listener(|e, _, _, cx| e.smartart_add_assistant(cx))));
            }
            let styles = [(SmartArtColorStyle::Colorful, "pptx.smartart.color.colorful"), (SmartArtColorStyle::Accent1, "pptx.smartart.color.accent1"), (SmartArtColorStyle::Accent2, "pptx.smartart.color.accent2"), (SmartArtColorStyle::Accent3, "pptx.smartart.color.accent3"), (SmartArtColorStyle::Outline, "pptx.smartart.color.outline")];
            let selected = styles.iter().position(|(value, _)| *value == style).unwrap_or(0);
            let choices = styles.into_iter().map(|(value, key)| crate::shell::chrome::preview_dropdown::PreviewChoice {
                label: t(key),
                content: std::rc::Rc::new(move || h_flex().w_full().gap_1().children((0..5).map(|i| {let (fill, stroke, _) = value.palette(i);div().flex_1().h(px(18.)).rounded(px(3.)).bg(rgb(fill.0)).border_1().border_color(rgb(stroke.unwrap_or(fill).0))})).into_any_element()),
                choose: std::rc::Rc::new(move |e, _, cx| e.set_smartart_color_style(value, cx)),
            }).collect();
            body = body.child(crate::shell::chrome::dropdown_row(t("pptx.smartart.color"), crate::shell::chrome::preview_dropdown::preview_dropdown("pptx-smartart-color", choices, selected, cx)));
        } else {
            body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(t("pptx.smartart.select-prompt")));
        }

        Some(SidebarPanel { title: title.into(), body: body.into_any_element() })
    }
}
