// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Word's Math / Equation panel, structure templates, and conversions (DOCX-FONT-006, DOCX-IMAGE-017, DOCX-FIELD-012, DOCX-EDIT-002).

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::flow::{Block, FlowRun};
use ibis_doc_model::flow_command::{self, FlowCommand};
use ibis_doc_model::math::{Math, parse_latex, parse_linear};
use ibis_doc_model::{Container, FlowPos, FlowRunKind};

use crate::shell::chrome::{SidebarPanel, text_secondary};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) use crate::shell::math_editor::MathFormat;
use crate::shell::math_editor::{MathEditorState, MathDialogSpec, math_dialog};

pub(crate) struct DocxMathState {
    pub(crate) editor: MathEditorState,
    pub(crate) display: bool,
    pub(crate) error: Option<String>,
}
impl std::ops::Deref for DocxMathState {
    type Target = MathEditorState;
    fn deref(&self) -> &Self::Target { &self.editor }
}
impl std::ops::DerefMut for DocxMathState {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.editor }
}

/// Word 標準の組込数式（DOCX-MATH-003）。
pub(crate) struct BuiltinEquation {
    pub(crate) id: &'static str,
    pub(crate) action_id: &'static str,
    pub(crate) label_key: &'static str,
    pub(crate) latex: &'static str,
}

pub(crate) const BUILTIN_EQUATIONS: &[BuiltinEquation] = &[
    BuiltinEquation {
        id: "quadratic",
        action_id: "docx.math.builtin.quadratic",
        label_key: "docx.math.builtin.quadratic",
        latex: r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
    },
    BuiltinEquation {
        id: "binomial",
        action_id: "docx.math.builtin.binomial",
        label_key: "docx.math.builtin.binomial",
        latex: r"(x + a)^n = \sum_{k=0}^{n} \binom{n}{k} x^k a^{n-k}",
    },
    BuiltinEquation {
        id: "circle-area",
        action_id: "docx.math.builtin.circle-area",
        label_key: "docx.math.builtin.circle-area",
        latex: r"A = \pi r^2",
    },
    BuiltinEquation {
        id: "pythagorean",
        action_id: "docx.math.builtin.pythagorean",
        label_key: "docx.math.builtin.pythagorean",
        latex: r"a^2 + b^2 = c^2",
    },
    BuiltinEquation {
        id: "fourier",
        action_id: "docx.math.builtin.fourier",
        label_key: "docx.math.builtin.fourier",
        latex: r"f(x) = a_0 + \sum_{n=1}^{\infty} \left( a_n \cos \frac{n\pi x}{L} + b_n \sin \frac{n\pi x}{L} \right)",
    },
    BuiltinEquation {
        id: "taylor",
        action_id: "docx.math.builtin.taylor",
        label_key: "docx.math.builtin.taylor",
        latex: r"e^x = 1 + \frac{x}{1!} + \frac{x^2}{2!} + \dots",
    },
];

impl SlideEditor {
    /// 組込数式（二次方程式、二項定理、円の面積、ピタゴラスの定理、フーリエ級数、テイラー展開）を挿入する。
    pub(crate) fn insert_docx_math_builtin(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let Some(eq) = BUILTIN_EQUATIONS.iter().find(|e| e.id == id) else { return false };
        let math = parse_latex(eq.latex);
        let display = self.doc_state().and_then(|s| s.docx_math.as_ref()).map_or(false, |s| s.display);
        self.insert_docx_math_at_selection(math, display, cx)
    }
    /// キャレット位置にある数式ラン（`FlowRunKind::Math`）を取得する。
    pub(crate) fn docx_math_at_caret(&self) -> Option<(Container, usize, usize, Math, bool)> {
        let (start, _end) = self.ordered_selection()?;
        let doc = self.flow()?;
        let p = flow_command::paragraph(doc, start.para)?;
        let (container, index) = flow_command::locate(doc, start.para)?;
        let mut offset = 0;
        for (run_idx, r) in p.runs.iter().enumerate() {
            let len = r.len();
            if let FlowRunKind::Math { math, display } = &r.kind {
                if (start.offset >= offset && start.offset <= offset + len) || (offset == 0 && start.offset == 0) {
                    return Some((container, index, run_idx, math.clone(), *display));
                }
            }
            offset += len;
        }
        None
    }

    /// キャレット位置の既存の数式（`FlowRunKind::Math`）をその場で更新する（1ステップ Undo/Redo）。
    pub(crate) fn edit_docx_math_at_caret(&mut self, math: Math, display: bool, cx: &mut Context<Self>) -> bool {
        let Some((container, index, run_idx, _old_math, _old_display)) = self.docx_math_at_caret() else {
            return false;
        };
        let Some((start, _)) = self.ordered_selection() else { return false };
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, start.para) else { return false };
        let mut new_p = p.clone();
        let r = &new_p.runs[run_idx];
        new_p.runs[run_idx] = FlowRun {
            kind: FlowRunKind::Math { math, display },
            style: r.style.clone(),
            format: r.format.clone(),
            char_style: r.char_style.clone(),
            link: r.link.clone(),
            revision: r.revision.clone(),
            show_field_code: false,
            field_locked: false,
            control: None,
        };
        new_p.resolve(&doc.styles);
        let cmd = FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(new_p)] };
        self.run_flow(vec![cmd], cx)
    }

    /// 数式編集・挿入サイドバーパネルを開く。キャレット位置に数式がある場合はその内容を読み込む。
    pub(crate) fn open_docx_math_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        self.end_text_edit(window, cx);
        let existing = self.docx_math_at_caret();
        let initial_text = existing.as_ref().map(|(_, _, _, m, _)| m.linear()).unwrap_or_default();
        let initial_display = existing.as_ref().map_or(false, |(_, _, _, _, d)| *d);
        let editor = MathEditorState::new(&initial_text, window, cx, |e, _, cx| e.insert_docx_math_from_panel(cx));
        if let Some(state) = self.doc_state_mut() {
            state.docx_math = Some(DocxMathState {
                editor,
                display: initial_display,
                error: None,
            });
            state.quick_parts = None;
            state.table_formula = None;
            state.table_size = None;
            state.picture_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.style_edit = None;
            state.link = None;
            state.find = None;
            state.comments = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 選択範囲またはキャレット位置に数式（`FlowRunKind::Math`）を挿入する。
    pub(crate) fn insert_docx_math_at_selection(&mut self, math: Math, display: bool, cx: &mut Context<Self>) -> bool {
        let Some((start, end)) = self.ordered_selection() else { return false };
        let mut cmds = Vec::new();
        if start != end {
            cmds.push(FlowCommand::Delete { from: start, to: end });
        }
        cmds.push(FlowCommand::InsertField { at: start, kind: FlowRunKind::Math { math, display } });
        if self.run_flow(cmds, cx) {
            self.set_caret(FlowPos::new(start.para, start.offset + 1), false, cx);
            self.scroll_to_caret(cx);
            return true;
        }
        false
    }

    /// サイドバー入力欄の数式文字列をパースして挿入またはその場更新する。
    pub(crate) fn insert_docx_math_from_panel(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state().and_then(|s| s.docx_math.as_ref()) else { return };
        let text_val = state.input.read(cx).value().trim().to_string();
        let format = state.format;
        let display = state.display;

        if text_val.is_empty() {
            if let Some(state) = self.doc_state_mut().and_then(|s| s.docx_math.as_mut()) {
                state.error = Some(text("docx.math.error.empty").into());
            }
            cx.notify();
            return;
        }

        let math = if format == MathFormat::Latex || text_val.starts_with('\\') {
            parse_latex(&text_val)
        } else {
            parse_linear(&text_val)
        };

        let is_collapsed = self.ordered_selection().is_some_and(|(s, e)| s == e);
        let applied = if is_collapsed && self.docx_math_at_caret().is_some() {
            self.edit_docx_math_at_caret(math, display, cx)
        } else {
            self.insert_docx_math_at_selection(math, display, cx)
        };
        if applied {
            if let Some(state) = self.doc_state_mut() {
                state.docx_math = None;
            }
            cx.notify();
        }
    }

    /// 数式の構造テンプレート（分数・斜め分数・リニア分数・上付き・根号・積分・大型演算子・かっこ・関数・アクセント・極限・行列）を挿入する。
    pub(crate) fn insert_docx_math_structure(&mut self, kind: &str, cx: &mut Context<Self>) -> bool {
        let math = match kind {
            "fraction" => parse_latex(r"\frac{a}{b}"),
            "fraction-skewed" => parse_latex(r"\sfrac{a}{b}"),
            "fraction-linear" => parse_latex(r"\lfrac{a}{b}"),
            "fraction-nobar" => parse_latex(r"\nobarfrac{a}{b}"),
            "script" => parse_latex(r"x^{2}"),
            "subscript" => parse_latex(r"x_{1}"),
            "radical" => parse_latex(r"\sqrt{x}"),
            "integral" => parse_latex(r"\int_{a}^{b} x dx"),
            "large-op" => parse_latex(r"\sum_{i=1}^{n} x_{i}"),
            "brackets" => parse_latex(r"\left( x \right)"),
            "function" => parse_latex(r"\sin(x)"),
            "accent" => parse_latex(r"\vec{v}"),
            "limit" => parse_latex(r"\lim_{x \to \infty} f(x)"),
            "matrix" => parse_latex(r"\begin{matrix} a & b \\ c & d \end{matrix}"),
            _ => parse_linear(kind),
        };
        let display = self.doc_state().and_then(|s| s.docx_math.as_ref()).map_or(false, |s| s.display);
        self.insert_docx_math_at_selection(math, display, cx)
    }

    /// 数式の表示形式をプロフェッショナル（2D）とリニア（1Dテキスト）の間で変換・トグルする。
    pub(crate) fn convert_docx_math_mode(&mut self, to_linear: bool, cx: &mut Context<Self>) -> bool {
        let Some((start, end)) = self.ordered_selection() else { return false };
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, start.para) else { return false };
        let Some((container, index)) = flow_command::locate(doc, start.para) else { return false };

        // 1. キャレット位置が数式ラン（FlowRunKind::Math）上にある場合
        let mut offset = 0;
        for (run_idx, r) in p.runs.iter().enumerate() {
            let len = r.len();
            if let FlowRunKind::Math { math, display } = &r.kind {
                if (start.offset >= offset && start.offset <= offset + len) || (offset == 0 && start.offset == 0) {
                    let mut new_p = p.clone();
                    if to_linear {
                        // プロフェッショナル 2D -> リニア 1D UnicodeMath テキストに変換
                        let linear_text = math.linear();
                        new_p.runs[run_idx] = FlowRun {
                            kind: FlowRunKind::Text(linear_text),
                            style: r.style.clone(),
                            format: r.format.clone(),
                            char_style: r.char_style.clone(),
                            link: r.link.clone(),
                            revision: r.revision.clone(),
                            show_field_code: false,
                            field_locked: false,
                            control: None,
                        };
                    } else {
                        // プロフェッショナル変換: インライン/独立行のトグル、または 2D 再構築
                        new_p.runs[run_idx] = FlowRun {
                            kind: FlowRunKind::Math { math: math.clone(), display: !display },
                            style: r.style.clone(),
                            format: r.format.clone(),
                            char_style: r.char_style.clone(),
                            link: r.link.clone(),
                            revision: r.revision.clone(),
                            show_field_code: false,
                            field_locked: false,
                            control: None,
                        };
                    }
                    new_p.resolve(&doc.styles);
                    let cmd = FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(new_p)] };
                    return self.run_flow(vec![cmd], cx);
                }
            }
            offset += len;
        }

        // 2. 選択テキストがある場合、テキストを数式に変換（プロフェッショナル化）
        if start != end {
            let selected = self.selected_text();
            let trimmed = selected.trim();
            if !trimmed.is_empty() {
                let math = if trimmed.starts_with('\\') { parse_latex(trimmed) } else { parse_linear(trimmed) };
                return self.insert_docx_math_at_selection(math, false, cx);
            }
        }

        false
    }

    /// 数式サイドバーパネルの描画要素。
    pub(crate) fn docx_math_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?.docx_math.as_ref()?;
        let input_field = state.input.clone();
        let format = state.format;
        let display = state.display;
        let error = state.error.clone();

        let display_bar = h_flex()
            .gap_2()
            .child(
                crate::shell::chrome::radio_button("math-disp-inline", !display, false, text("docx.math.inline"), cx.listener(|this, _, _, cx| {
                        if let Some(s) = this.doc_state_mut().and_then(|s| s.docx_math.as_mut()) {
                            s.display = false;
                        }
                        cx.notify();
                    })),
            )
            .child(
                crate::shell::chrome::radio_button("math-disp-para", display, false, text("docx.math.display"), cx.listener(|this, _, _, cx| {
                        if let Some(s) = this.doc_state_mut().and_then(|s| s.docx_math.as_mut()) {
                            s.display = true;
                        }
                        cx.notify();
                    })),
            );

        // 構造（分数・上付き・根号・積分・アクセント・極限ほか）テンプレートボタン一覧
        let structures = [
            ("fraction", "a/b", "docx.math.struct.fraction"),
            ("script", "x²", "docx.math.struct.script"),
            ("radical", "√x", "docx.math.struct.radical"),
            ("integral", "∫x", "docx.math.struct.integral"),
            ("large-op", "∑x", "docx.math.struct.large-op"),
            ("brackets", "(x)", "docx.math.struct.brackets"),
            ("function", "sin", "docx.math.struct.function"),
            ("accent", "v⃗", "docx.math.struct.accent"),
            ("limit", "lim", "docx.math.struct.limit"),
            ("matrix", "[::]", "docx.math.struct.matrix"),
        ];

        let mut struct_grid = h_flex().gap_1().flex_wrap();
        for (kind, label_str, tr_key) in structures {
            struct_grid = struct_grid.child(
                crate::shell::chrome::action_button(format!("struct-{kind}"))
                    .small()
                    .label(format!("{label_str} {}", text(tr_key)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.insert_docx_math_structure(kind, cx);
                    })),
            );
        }

        let label = |tr_key: &'static str| {
            div().text_sm().text_color(text_secondary()).child(text(tr_key))
        };

        let options = vec![
            label("docx.math.layout-label").into_any_element(),
            display_bar.into_any_element(),
            label("docx.math.structures").into_any_element(),
            struct_grid.into_any_element(),
        ];
        let insert = crate::shell::chrome::ActionDefinition::new("btn-insert-math", text("docx.math.insert"), crate::shell::chrome::ActionButtonKind::Primary)
            .disabled(input_field.read(cx).value().trim().is_empty())
            .on_click(cx.listener(|this, _, _, cx| this.insert_docx_math_from_panel(cx)));
        let commands = vec![
            crate::shell::chrome::ActionDefinition::new("btn-convert-linear", text("docx.math.convert.linear"), crate::shell::chrome::ActionButtonKind::Normal)
                .on_click(cx.listener(|this, _, _, cx| { this.convert_docx_math_mode(true, cx); })),
            crate::shell::chrome::ActionDefinition::new("btn-convert-pro", text("docx.math.convert.professional"), crate::shell::chrome::ActionButtonKind::Normal)
                .on_click(cx.listener(|this, _, _, cx| { this.convert_docx_math_mode(false, cx); })),
        ];
        Some(math_dialog(MathDialogSpec {
            input: input_field, format,
            set_format: |e, format, cx| {
                if let Some(p) = e.doc_state_mut().and_then(|s| s.docx_math.as_mut()) { p.format = format; }
                cx.notify();
            },
            options, error, context_menu: None, insert, commands,
        }, cx))
    }
}
