// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! オブジェクト挿入 (Word's 挿入 > アイコン, SmartArt, ワードアート, 署名欄, クイック パーツ):
//! inserting icons, SmartArt diagrams, WordArt, signature lines, and Quick Parts fields
//! at the caret in the FlowDocument.

use std::sync::Arc;
use gpui_kit::*;
use ibis_doc_model::{
    Color, EmuSize, FlowCommand, FlowPos, FlowRunKind, Rgb, Shadow,
};

use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::shell::editor::SlideEditor;
use crate::ext::*;

pub(crate) const DOCX_WORDART_STYLES: &[(&'static str, &'static str, &'static str)] = &[
    ("docx.wordart.style1", "docx.insert.wordart.style1", "docx.text.wordart.style1"),
    ("docx.wordart.style2", "docx.insert.wordart.style2", "docx.text.wordart.style2"),
    ("docx.wordart.style3", "docx.insert.wordart.style3", "docx.text.wordart.style3"),
    ("docx.wordart.style4", "docx.insert.wordart.style4", "docx.text.wordart.style4"),
    ("docx.wordart.style5", "docx.insert.wordart.style5", "docx.text.wordart.style5"),
    ("docx.wordart.style6", "docx.insert.wordart.style6", "docx.text.wordart.style6"),
];

#[ibis_office_macros::editor_ext(InsertObjects)]
impl SlideEditor {
    /// 挿入 > アイコン…: insert a bundled Material icon as an SVG picture at the caret.
    pub(crate) fn doc_insert_icon(&mut self, icon_name: &str, cx: &mut Context<Self>) -> bool {
        let Some((_, caret)) = self.text_selection() else { return false };
        let bytes = crate::shell::icons::icon_svg_bytes(icon_name)
            .or_else(|| crate::shell::icons::icon_svg_bytes("auto_awesome"))
            .or_else(|| crate::shell::icons::icon_svg_bytes("description"));
        let Some(raw) = bytes else { return false };
        let data = raw.to_vec();
        let format = ibis_doc_model::MediaFormat::Svg;
        let media = self.document.ids().add_media(format, data.clone());
        self.resources.images.insert(media, Arc::new(Image::from_bytes(ImageFormat::Svg, data)));
        // Default size: 1 inch square (914_400 EMU = 72 pt)
        let size = EmuSize::new(914_400, 914_400);
        if self.run_flow(vec![FlowCommand::InsertPicture { at: caret, media, size }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            cx.notify();
            return true;
        }
        false
    }

    /// 挿入 > SmartArt: insert a structured SmartArt placeholder text box at the caret.
    pub(crate) fn doc_insert_smartart(&mut self, kind_name: &str, cx: &mut Context<Self>) -> bool {
        let Some((_, caret)) = self.text_selection() else { return false };
        let text = match kind_name {
            "list" | "basic-block-list" | "pptx.insert.smartart.list" => {
                format!(
                    "[SmartArt: {}]\n• {}\n• {}\n• {}",
                    crate::shell::i18n::text("pptx.insert.smartart.list"),
                    crate::shell::i18n::text("_docx.smartart-item-1"),
                    crate::shell::i18n::text("_docx.smartart-item-2"),
                    crate::shell::i18n::text("_docx.smartart-item-3")
                )
            }
            "vlist" | "vertical-list" | "pptx.insert.smartart.vlist" => {
                format!(
                    "[SmartArt: {}]\n1. {}\n2. {}\n3. {}",
                    crate::shell::i18n::text("pptx.insert.smartart.vlist"),
                    crate::shell::i18n::text("_docx.smartart-item-1"),
                    crate::shell::i18n::text("_docx.smartart-item-2"),
                    crate::shell::i18n::text("_docx.smartart-item-3")
                )
            }
            "cycle" | "basic-cycle" | "pptx.insert.smartart.cycle" => {
                format!(
                    "[SmartArt: {}]\n1. {}\n2. {}\n3. {}",
                    crate::shell::i18n::text("pptx.insert.smartart.cycle"),
                    crate::shell::i18n::text("_docx.smartart-phase-1"),
                    crate::shell::i18n::text("_docx.smartart-phase-2"),
                    crate::shell::i18n::text("_docx.smartart-phase-3")
                )
            }
            "hierarchy" | "organization-chart" | "pptx.insert.smartart.hierarchy" => {
                format!(
                    "[SmartArt: {}]\n• {}\n  - {}\n    * {}",
                    crate::shell::i18n::text("pptx.insert.smartart.hierarchy"),
                    crate::shell::i18n::text("_docx.smartart-leader"),
                    crate::shell::i18n::text("_docx.smartart-manager"),
                    crate::shell::i18n::text("_docx.smartart-member")
                )
            }
            "relationship" | "basic-relationship" | "pptx.insert.smartart.relationship" => {
                format!(
                    "[SmartArt: {}]\n• {}\n• {}",
                    crate::shell::i18n::text("pptx.insert.smartart.relationship"),
                    crate::shell::i18n::text("_docx.smartart-elem-a"),
                    crate::shell::i18n::text("_docx.smartart-elem-b")
                )
            }
            "matrix" | "basic-matrix" | "pptx.insert.smartart.matrix" => {
                format!(
                    "[SmartArt: {}]\n• {}\n• {}\n• {}\n• {}",
                    crate::shell::i18n::text("pptx.insert.smartart.matrix"),
                    crate::shell::i18n::text("_docx.smartart-quad-1"),
                    crate::shell::i18n::text("_docx.smartart-quad-2"),
                    crate::shell::i18n::text("_docx.smartart-quad-3"),
                    crate::shell::i18n::text("_docx.smartart-quad-4")
                )
            }
            "pyramid" | "basic-pyramid" | "pptx.insert.smartart.pyramid" => {
                format!(
                    "[SmartArt: {}]\n1. {}\n2. {}\n3. {}",
                    crate::shell::i18n::text("pptx.insert.smartart.pyramid"),
                    crate::shell::i18n::text("_docx.smartart-top"),
                    crate::shell::i18n::text("_docx.smartart-middle"),
                    crate::shell::i18n::text("_docx.smartart-base")
                )
            }
            "picture" | "picture-list" | "pptx.insert.smartart.picture" => {
                format!(
                    "[SmartArt: {}]\n1. {}\n2. {}\n3. {}",
                    crate::shell::i18n::text("pptx.insert.smartart.picture"),
                    crate::shell::i18n::text("_docx.smartart-item-1"),
                    crate::shell::i18n::text("_docx.smartart-item-2"),
                    crate::shell::i18n::text("_docx.smartart-item-3")
                )
            }
            _ => {
                // process / default
                format!(
                    "[SmartArt: {}]\n1. {}\n2. {}\n3. {}",
                    crate::shell::i18n::text("pptx.insert.smartart.process"),
                    crate::shell::i18n::text("_docx.smartart-step-1"),
                    crate::shell::i18n::text("_docx.smartart-step-2"),
                    crate::shell::i18n::text("_docx.smartart-step-3")
                )
            }
        };
        let size = EmuSize::new(4_000_000, 1_800_000);
        if self.run_flow(vec![FlowCommand::InsertTextBox { at: caret, size, text }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            cx.notify();
            return true;
        }
        false
    }

    /// 挿入 > ワードアート: insert a WordArt text box styled according to preset index.
    pub(crate) fn doc_insert_wordart(&mut self, style_idx: usize, text: Option<&str>, cx: &mut Context<Self>) -> bool {
        let Some((_, caret)) = self.text_selection() else { return false };
        let preset = crate::shell::shape_items::WORDART_PRESETS
            .get(style_idx)
            .copied()
            .unwrap_or(crate::shell::shape_items::WORDART_PRESETS[0]);
        let default_text = crate::shell::i18n::text("_pptx.wordart-default-text");
        let initial_text = text.unwrap_or(&default_text);
        let size = EmuSize::new(3_810_000, 952_500);

        let mut commands = vec![
            FlowCommand::InsertTextBox { at: caret, size, text: initial_text.to_string() },
            FlowCommand::SetShapeTextFormat {
                at: caret,
                bold: Some(preset.bold),
                italic: None,
                size: Some(3600),
                color: Some(preset.color),
            },
        ];
        if let Some((dir, dist, blur)) = preset.shadow {
            let shadow = Shadow {
                color: Color { rgb: Rgb(0), alpha: 102 },
                blur,
                distance: dist,
                direction: dir,
                sx: 100_000,
                sy: 100_000,
                kx: 0,
                ky: 0,
                align: ibis_doc_model::TileAlign::TopLeft,
                rot_with_shape: true,
            };
            commands.push(FlowCommand::SetShapeEffects {
                at: caret,
                fill: None,
                shadow: Some(Some(shadow)),
                glow: None,
                soft_edge: None,
                reflection: None,
            });
        }
        if self.run_flow(commands, cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            cx.notify();
            return true;
        }
        false
    }

    /// 挿入 > 署名欄…: place a signature line placeholder text box in the document.
    pub(crate) fn doc_insert_signature_line(&mut self, signer: &str, title: &str, cx: &mut Context<Self>) -> bool {
        let Some((_, caret)) = self.text_selection() else { return false };
        let size = EmuSize::new(3_000_000, 1_200_000);
        let label = crate::shell::i18n::text("xlsx.signature-line");
        let text = if signer.is_empty() {
            format!("✕ _______________________________\n{label}") // i18n-ok: signature placeholder
        } else if title.is_empty() {
            format!("✕ _______________________________\n{signer}") // i18n-ok: signature line
        } else {
            format!("✕ _______________________________\n{signer}\n{title}") // i18n-ok: signature line
        };
        if self.run_flow(vec![FlowCommand::InsertTextBox { at: caret, size, text }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            cx.notify();
            return true;
        }
        false
    }

    /// 挿入 > クイック パーツ: insert document property or dynamic field at caret.
    pub(crate) fn doc_insert_quick_part(&mut self, field_code: &str, display_text: &str, cx: &mut Context<Self>) -> bool {
        let Some((_, caret)) = self.text_selection() else { return false };
        let kind = match field_code {
            "FILENAME" => FlowRunKind::FileName(display_text.to_string()),
            "PAGE" => FlowRunKind::PageNumber,
            _ => FlowRunKind::Field {
                instruction: field_code.to_string(),
                shown: display_text.to_string(),
            },
        };
        if self.run_flow(vec![FlowCommand::InsertField { at: caret, kind }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            cx.notify();
            return true;
        }
        false
    }
}

pub(crate) fn docx_wordart_insert_items() -> Vec<MenuEntry> {
    DOCX_WORDART_STYLES
        .iter()
        .enumerate()
        .map(|(idx, &(name_id, insert_id, _))| {
            MenuItem::new(insert_id, crate::shell::i18n::text(name_id))
                .on(move |e, _, cx| {
                    e.doc_insert_wordart(idx, None, cx);
                })
                .into()
        })
        .collect()
}

pub(crate) fn docx_smartart_insert_items() -> Vec<MenuEntry> {
    vec![
        MenuItem::tr("pptx.insert.smartart.process")
            .icon("arrow_forward")
            .on(|e, _, cx| {
                e.doc_insert_smartart("process", cx);
            })
            .into(),
        MenuItem::tr("pptx.insert.smartart.list")
            .icon("view_list")
            .on(|e, _, cx| {
                e.doc_insert_smartart("list", cx);
            })
            .into(),
        MenuItem::tr("pptx.insert.smartart.vlist")
            .icon("format_list_bulleted")
            .on(|e, _, cx| {
                e.doc_insert_smartart("vlist", cx);
            })
            .into(),
        MenuItem::tr("pptx.insert.smartart.cycle")
            .icon("autorenew")
            .on(|e, _, cx| {
                e.doc_insert_smartart("cycle", cx);
            })
            .into(),
        MenuItem::tr("pptx.insert.smartart.hierarchy")
            .icon("account_tree")
            .on(|e, _, cx| {
                e.doc_insert_smartart("hierarchy", cx);
            })
            .into(),
        MenuItem::tr("pptx.insert.smartart.relationship")
            .icon("sync_alt")
            .on(|e, _, cx| {
                e.doc_insert_smartart("relationship", cx);
            })
            .into(),
        MenuItem::tr("pptx.insert.smartart.matrix")
            .icon("grid_view")
            .on(|e, _, cx| {
                e.doc_insert_smartart("matrix", cx);
            })
            .into(),
        MenuItem::tr("pptx.insert.smartart.pyramid")
            .icon("change_history")
            .on(|e, _, cx| {
                e.doc_insert_smartart("pyramid", cx);
            })
            .into(),
        MenuItem::tr("pptx.insert.smartart.picture")
            .icon("insert_photo")
            .on(|e, _, cx| {
                e.doc_insert_smartart("picture", cx);
            })
            .into(),
    ]
}

pub(crate) fn docx_icon_insert_items() -> Vec<MenuEntry> {
    use crate::shell::shape_items::ICON_CATEGORIES;
    ICON_CATEGORIES
        .iter()
        .map(|cat| {
            let sub_items = cat
                .items
                .iter()
                .map(|(icon_name, label_key)| {
                    let iname = *icon_name;
                    MenuItem::tr(label_key)
                        .icon(iname)
                        .on(move |e, _, cx| {
                            e.doc_insert_icon(iname, cx);
                        })
                        .into()
                })
                .collect::<Vec<MenuEntry>>();
            Submenu::tr(cat.label_key, cat.icon, sub_items).into()
        })
        .collect()
}

