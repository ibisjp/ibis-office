// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Word's Quick Parts & Building Blocks (クイック パーツ・定型句・文書パーツの登録・挿入・管理).
//! Implementation of DOCX-STYLE-018, DOCX-FIELD-012, DOCX-FORM-008.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use ibis_doc_model::{FlowCommand, FlowDocument, flow_command};

use crate::shell::chrome::{SidebarPanel, text_secondary};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// ギャラリー種別 (w:gallery)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuildingBlockGallery {
    QuickParts,
    AutoText,
    Custom,
}

impl BuildingBlockGallery {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::QuickParts => "QuickParts",
            Self::AutoText => "AutoText",
            Self::Custom => "Custom",
        }
    }

    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "AutoText" => Self::AutoText,
            "Custom" => Self::Custom,
            _ => Self::QuickParts,
        }
    }

    pub(crate) fn label(&self) -> gpui_kit::SharedString {
        crate::shell::i18n::text(match self {
            Self::QuickParts => "docx.quick-parts.gallery.quick-parts",
            Self::AutoText => "docx.quick-parts.gallery.auto-text",
            Self::Custom => "docx.quick-parts.gallery.custom",
        })
    }
}

/// 挿入オプション (w:behaviors)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuildingBlockOption {
    /// コンテンツのみ挿入（インライン）
    ContentOnly,
    /// 独自の段落に挿入
    Paragraph,
    /// 独自のページに挿入
    Page,
}

impl BuildingBlockOption {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::ContentOnly => "ContentOnly",
            Self::Paragraph => "Paragraph",
            Self::Page => "Page",
        }
    }

    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "Paragraph" => Self::Paragraph,
            "Page" => Self::Page,
            _ => Self::ContentOnly,
        }
    }

    pub(crate) fn label(&self) -> gpui_kit::SharedString {
        crate::shell::i18n::text(match self {
            Self::ContentOnly => "docx.quick-parts.option.content-only",
            Self::Paragraph => "docx.quick-parts.option.paragraph",
            Self::Page => "docx.quick-parts.option.page",
        })
    }
}

/// 保存スコープ（保存先）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuildingBlockScope {
    /// 全文書（共通テンプレート/ライブラリ）
    Global,
    /// 現在の文書
    Document,
}

impl BuildingBlockScope {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Global => "Global",
            Self::Document => "Document",
        }
    }

    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "Document" => Self::Document,
            _ => Self::Global,
        }
    }

    pub(crate) fn label(&self) -> gpui_kit::SharedString {
        crate::shell::i18n::text(match self {
            Self::Global => "docx.quick-parts.scope.global",
            Self::Document => "docx.quick-parts.scope.document",
        })
    }
}

/// 文書パーツ / 定型句エントリ (Word Building Block)
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildingBlock {
    pub(crate) name: String,
    pub(crate) gallery: BuildingBlockGallery,
    pub(crate) category: String,
    pub(crate) description: String,
    pub(crate) content: String,
    pub(crate) insert_option: BuildingBlockOption,
    pub(crate) scope: BuildingBlockScope,
}

impl BuildingBlock {
    pub(crate) fn preview_snippet(&self) -> String {
        let first_line = self.content.lines().next().unwrap_or("").trim();
        if first_line.chars().count() > 40 {
            let truncated: String = first_line.chars().take(37).collect();
            format!("{truncated}...")
        } else {
            first_line.to_string()
        }
    }

    pub(crate) fn encode(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            escape_field(&self.name),
            self.gallery.as_str(),
            escape_field(&self.category),
            escape_field(&self.description),
            escape_field(&self.content),
            self.insert_option.as_str(),
            self.scope.as_str(),
        )
    }

    pub(crate) fn decode(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 5 {
            return None;
        }
        let name = unescape_field(parts.first()?);
        if name.is_empty() {
            return None;
        }
        let gallery = BuildingBlockGallery::from_str(parts.get(1).unwrap_or(&""));
        let category = unescape_field(parts.get(2).unwrap_or(&""));
        let description = unescape_field(parts.get(3).unwrap_or(&""));
        let content = unescape_field(parts.get(4).unwrap_or(&""));
        let insert_option = BuildingBlockOption::from_str(parts.get(5).unwrap_or(&""));
        let scope = BuildingBlockScope::from_str(parts.get(6).unwrap_or(&""));

        Some(Self {
            name,
            gallery,
            category,
            description,
            content,
            insert_option,
            scope,
        })
    }
}

fn escape_field(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}

fn unescape_field(s: &str) -> String {
    s.replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\")
}

pub(crate) fn encode_building_blocks(blocks: &[BuildingBlock]) -> String {
    blocks.iter().map(|b| b.encode()).collect::<Vec<_>>().join("\r")
}

pub(crate) fn decode_building_blocks(s: &str) -> Vec<BuildingBlock> {
    s.split(|c| c == '\n' || c == '\r')
        .filter(|line| !line.trim().is_empty())
        .filter_map(BuildingBlock::decode)
        .collect()
}

/// グローバル保存キー (settings)
const GLOBAL_BUILDING_BLOCKS_KEY: &str = "docx.building_blocks.library";
/// 文書内保存キー (doc_variables)
pub(crate) const DOC_BUILDING_BLOCKS_VAR: &str = "_building_blocks";

/// 組み込み・既定のサンプル文書パーツ
pub(crate) fn default_building_blocks() -> Vec<BuildingBlock> {
    vec![
        BuildingBlock {
            name: "社外秘ヘッダー".to_string(), // i18n-ok（サンプルの文書パーツ（文書に入る既定の内容））
            gallery: BuildingBlockGallery::QuickParts,
            category: "全般".to_string(), // i18n-ok（サンプルの文書パーツ（文書に入る既定の内容））
            description: "機密保持文書の先頭に付与するヘッダー".to_string(), // i18n-ok（サンプルの文書パーツ（文書に入る既定の内容））
            content: "【社外秘 / CONFIDENTIAL】".to_string(), // i18n-ok（サンプルの文書パーツ（文書に入る既定の内容））
            insert_option: BuildingBlockOption::Paragraph,
            scope: BuildingBlockScope::Global,
        },
        BuildingBlock {
            name: "会社署名".to_string(), // i18n-ok（サンプルの文書パーツ（文書に入る既定の内容））
            gallery: BuildingBlockGallery::AutoText,
            category: "署名".to_string(), // i18n-ok（サンプルの文書パーツ（文書に入る既定の内容））
            description: "アイビス公式署名".to_string(), // i18n-ok（サンプルの文書パーツ（文書に入る既定の内容））
            content: "株式会社アイビス\n〒104-0032 東京都中央区八丁堀2-16-3\nTEL: 03-5250-7380".to_string(), // i18n-ok（サンプルの文書パーツ（文書に入る既定の内容））
            insert_option: BuildingBlockOption::Paragraph,
            scope: BuildingBlockScope::Global,
        },
        BuildingBlock {
            name: "時候の挨拶".to_string(), // i18n-ok（サンプルの文書パーツ（文書に入る既定の内容））
            gallery: BuildingBlockGallery::AutoText,
            category: "挨拶".to_string(), // i18n-ok（サンプルの文書パーツ（文書に入る既定の内容））
            description: "一般的なビジネスレターの時候の挨拶".to_string(), // i18n-ok（サンプルの文書パーツ（文書に入る既定の内容））
            content: "拝啓\n貴社におかれましては、ますますご清栄のこととお慶び申し上げます。\n平素は格別のご高配を賜り、厚く御礼申し上げます。".to_string(), // i18n-ok（サンプルの文書パーツ（文書に入る既定の内容））
            insert_option: BuildingBlockOption::Paragraph,
            scope: BuildingBlockScope::Global,
        },
    ]
}

/// グローバル（全文書共通）の文書パーツ読込
pub(crate) fn load_global_building_blocks() -> Vec<BuildingBlock> {
    if let Some(data) = crate::shell::settings::get(GLOBAL_BUILDING_BLOCKS_KEY) {
        let entries = decode_building_blocks(&data);
        if !entries.is_empty() {
            return entries;
        }
    }
    default_building_blocks()
}

/// グローバル（全文書共通）の文書パーツ保存
pub(crate) fn save_global_building_blocks(blocks: &[BuildingBlock]) {
    let data = encode_building_blocks(blocks);
    crate::shell::settings::set(GLOBAL_BUILDING_BLOCKS_KEY, &data);
}

/// 文書限定の文書パーツ読込 (doc_variables)
pub(crate) fn load_document_building_blocks(doc: &FlowDocument) -> Vec<BuildingBlock> {
    doc.doc_variables
        .get(DOC_BUILDING_BLOCKS_VAR)
        .map(|s| decode_building_blocks(s))
        .unwrap_or_default()
}

/// 文書限定の文書パーツ保存コマンド
pub(crate) fn save_document_building_blocks_command(
    doc: &FlowDocument,
    blocks: &[BuildingBlock],
) -> Option<FlowCommand> {
    let data = encode_building_blocks(blocks);
    let mut vars = doc.doc_variables.clone();
    vars.insert(DOC_BUILDING_BLOCKS_VAR.to_string(), data);
    Some(FlowCommand::SetDocVariables(vars))
}

/// 全文書パーツ（グローバル＋文書限定）の統合読込
pub(crate) fn load_all_building_blocks(doc: Option<&FlowDocument>) -> Vec<BuildingBlock> {
    let mut list = load_global_building_blocks();
    if let Some(d) = doc {
        let doc_blocks = load_document_building_blocks(d);
        for b in doc_blocks {
            // 文書限定のものが同名であれば優先
            if let Some(pos) = list.iter().position(|x| x.name.eq_ignore_ascii_case(&b.name)) {
                list[pos] = b;
            } else {
                list.push(b);
            }
        }
    }
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

/// 文書パーツ登録・編集フォーム状態
pub(crate) struct BuildingBlockFormState {
    pub(crate) is_new: bool,
    pub(crate) original_name: String,
    pub(crate) original_scope: BuildingBlockScope,
    pub(crate) name_input: Entity<InputState>,
    pub(crate) category_input: Entity<InputState>,
    pub(crate) description_input: Entity<InputState>,
    pub(crate) content_input: Entity<InputState>,
    pub(crate) gallery: BuildingBlockGallery,
    pub(crate) scope: BuildingBlockScope,
    pub(crate) insert_option: BuildingBlockOption,
    pub(crate) error: Option<String>,
}

/// クイックパーツ・文書パーツパネル状態
pub(crate) struct QuickPartsState {
    pub(crate) tab: usize,
    pub(crate) building_blocks: Vec<BuildingBlock>,
    pub(crate) filter_input: Entity<InputState>,
    #[allow(dead_code)]
    pub(crate) selected_block: Option<usize>,
    pub(crate) save_form: Option<BuildingBlockFormState>,
    pub(crate) custom_field: Entity<InputState>,
    pub(crate) message: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl SlideEditor {
    /// クイックパーツ（フィールド・文書プロパティ・文書パーツ）パネルを開く。
    pub(crate) fn open_quick_parts_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        self.end_text_edit(window, cx);
        let blocks = load_all_building_blocks(self.flow().map(|v| &**v));
        let filter_input = cx.new(|cx| InputState::new(window, cx).placeholder(text("docx.quick-parts.filter-placeholder")));
        let custom_field = cx.new(|cx| {
            InputState::new(window, cx).placeholder("TITLE, AUTHOR, PAGE, NUMPAGES...")
        });
        let subscriptions = vec![
            cx.subscribe_in(&custom_field, window, |this, _, event: &InputEvent, _window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let code = this.doc_state()
                        .and_then(|s| s.quick_parts.as_ref())
                        .map(|qp| qp.custom_field.read(cx).value().trim().to_string())
                        .unwrap_or_default();
                    if !code.is_empty() {
                        this.insert_quick_part_field(&code, cx);
                    }
                }
            }),
        ];
        if let Some(state) = self.doc_state_mut() {
            state.quick_parts = Some(QuickPartsState {
                tab: 0,
                building_blocks: blocks,
                filter_input,
                selected_block: None,
                save_form: None,
                custom_field,
                message: None,
                _subscriptions: subscriptions,
            });
            state.docx_math = None;
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
            state.source_manager = None;
            state.print_panel = None;
            state.inspect_panel = None;
            state.compatibility_panel = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 選択範囲を文書パーツとして登録するダイアログ（パネル内の登録フォーム）を開く。
    pub(crate) fn open_save_building_block_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        self.open_quick_parts_panel(window, cx);
        self.quick_parts_start_new(window, cx);
    }

    /// 新規文書パーツ登録フォームを開始する。
    pub(crate) fn quick_parts_start_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sel_text = self.selected_text();
        let default_name = if !sel_text.is_empty() {
            let first_line = sel_text.lines().next().unwrap_or("").trim();
            if first_line.chars().count() > 25 {
                first_line.chars().take(25).collect::<String>()
            } else {
                first_line.to_string()
            }
        } else {
            crate::shell::i18n::text("docx.quick-parts.default-name").to_string()
        };

        let name_input = cx.new(|cx| {
            let mut inp = InputState::new(window, cx).placeholder(text("docx.quick-parts.name"));
            inp.set_value(&default_name, window, cx);
            inp.focus(window, cx);
            inp
        });
        let category_input = cx.new(|cx| {
            let mut inp = InputState::new(window, cx).placeholder(text("docx.quick-parts.category"));
            inp.set_value(&crate::shell::i18n::text("docx.quick-parts.default-category"), window, cx);
            inp
        });
        let description_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(text("docx.quick-parts.description"))
        });
        let content_input = cx.new(|cx| {
            let mut inp = InputState::new(window, cx).placeholder(text("docx.quick-parts.content"));
            inp.set_value(&sel_text, window, cx);
            inp
        });

        if let Some(s) = self.doc_state_mut().and_then(|s| s.quick_parts.as_mut()) {
            s.save_form = Some(BuildingBlockFormState {
                is_new: true,
                original_name: String::new(),
                original_scope: BuildingBlockScope::Global,
                name_input,
                category_input,
                description_input,
                content_input,
                gallery: BuildingBlockGallery::AutoText,
                scope: BuildingBlockScope::Global,
                insert_option: BuildingBlockOption::ContentOnly,
                error: None,
            });
            s.message = None;
        }
        cx.notify();
    }

    /// 登録フォームをキャンセル
    pub(crate) fn quick_parts_cancel_form(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut().and_then(|s| s.quick_parts.as_mut()) {
            s.save_form = None;
        }
        cx.notify();
    }

    /// 登録フォームの内容を保存
    pub(crate) fn quick_parts_commit_form(&mut self, cx: &mut Context<Self>) {
        let (is_new, original_name, original_scope, block) = {
            let Some(s) = self.doc_state() else { return };
            let Some(qp) = s.quick_parts.as_ref() else { return };
            let Some(form) = qp.save_form.as_ref() else { return };

            let name = form.name_input.read(cx).value().trim().to_string();
            if name.is_empty() {
                if let Some(s) = self.doc_state_mut().and_then(|s| s.quick_parts.as_mut()).and_then(|q| q.save_form.as_mut()) {
                    s.error = Some(crate::shell::i18n::text("docx.quick-parts.name-required").to_string());
                }
                cx.notify();
                return;
            }

            let category = {
                let cat = form.category_input.read(cx).value().trim().to_string();
                if cat.is_empty() { crate::shell::i18n::text("docx.quick-parts.default-category").to_string() } else { cat }
            };
            let description = form.description_input.read(cx).value().trim().to_string();
            let content = form.content_input.read(cx).value().to_string();

            let block = BuildingBlock {
                name,
                gallery: form.gallery,
                category,
                description,
                content,
                insert_option: form.insert_option,
                scope: form.scope,
            };

            (form.is_new, form.original_name.clone(), form.original_scope, block)
        };

        // スコープに応じた永続化
        match block.scope {
            BuildingBlockScope::Global => {
                let mut globals = load_global_building_blocks();
                if is_new {
                    if let Some(pos) = globals.iter().position(|x| x.name.eq_ignore_ascii_case(&block.name)) {
                        globals[pos] = block.clone();
                    } else {
                        globals.push(block.clone());
                    }
                } else {
                    if original_scope == BuildingBlockScope::Global {
                        if let Some(pos) = globals.iter().position(|x| x.name.eq_ignore_ascii_case(&original_name)) {
                            globals[pos] = block.clone();
                        } else {
                            globals.push(block.clone());
                        }
                    } else {
                        globals.push(block.clone());
                    }
                }
                save_global_building_blocks(&globals);
            }
            BuildingBlockScope::Document => {
                if let Some(doc) = self.flow().cloned() {
                    let mut docs = load_document_building_blocks(&doc);
                    if is_new {
                        if let Some(pos) = docs.iter().position(|x| x.name.eq_ignore_ascii_case(&block.name)) {
                            docs[pos] = block.clone();
                        } else {
                            docs.push(block.clone());
                        }
                    } else {
                        if original_scope == BuildingBlockScope::Document {
                            if let Some(pos) = docs.iter().position(|x| x.name.eq_ignore_ascii_case(&original_name)) {
                                docs[pos] = block.clone();
                            } else {
                                docs.push(block.clone());
                            }
                        } else {
                            docs.push(block.clone());
                        }
                    }
                    if let Some(cmd) = save_document_building_blocks_command(&doc, &docs) {
                        self.run_flow(vec![cmd], cx);
                    }
                }
            }
        }

        // リスト再読込・完了メッセージ
        let reloaded = load_all_building_blocks(self.flow().map(|v| &**v));
        if let Some(s) = self.doc_state_mut().and_then(|s| s.quick_parts.as_mut()) {
            s.building_blocks = reloaded;
            s.save_form = None;
            s.message = Some(crate::shell::i18n::format("docx.quick-parts.registered", &[("name", &block.name)]));
        }
        self.status = Some(crate::shell::i18n::format("docx.quick-parts.registered", &[("name", &block.name)]).into());
        cx.notify();
    }

    /// 文書パーツをキャレット位置に挿入する。
    pub(crate) fn quick_parts_insert_block(&mut self, name: &str, cx: &mut Context<Self>) {
        let block = {
            let all = load_all_building_blocks(self.flow().map(|v| &**v));
            all.into_iter().find(|b| b.name.eq_ignore_ascii_case(name))
        };
        let Some(b) = block else { return };

        match b.insert_option {
            BuildingBlockOption::ContentOnly => {
                self.type_text(&b.content, cx);
            }
            BuildingBlockOption::Paragraph => {
                let txt = format!("\n{}\n", b.content.trim());
                self.type_text(&txt, cx);
            }
            BuildingBlockOption::Page => {
                let txt = format!("\u{000c}{}\u{000c}", b.content.trim());
                self.type_text(&txt, cx);
            }
        }

        self.status = Some(crate::shell::i18n::format("docx.quick-parts.inserted", &[("name", &b.name)]).into());
        cx.notify();
    }

    /// 文書パーツを削除する。
    pub(crate) fn quick_parts_delete_block(&mut self, name: &str, scope: BuildingBlockScope, cx: &mut Context<Self>) {
        match scope {
            BuildingBlockScope::Global => {
                let mut globals = load_global_building_blocks();
                globals.retain(|b| !b.name.eq_ignore_ascii_case(name));
                save_global_building_blocks(&globals);
            }
            BuildingBlockScope::Document => {
                if let Some(doc) = self.flow().cloned() {
                    let mut docs = load_document_building_blocks(&doc);
                    docs.retain(|b| !b.name.eq_ignore_ascii_case(name));
                    if let Some(cmd) = save_document_building_blocks_command(&doc, &docs) {
                        self.run_flow(vec![cmd], cx);
                    }
                }
            }
        }

        let reloaded = load_all_building_blocks(self.flow().map(|v| &**v));
        if let Some(s) = self.doc_state_mut().and_then(|s| s.quick_parts.as_mut()) {
            s.building_blocks = reloaded;
            s.message = Some(crate::shell::i18n::format("docx.quick-parts.deleted", &[("name", &name)]));
        }
        self.status = Some(crate::shell::i18n::format("docx.quick-parts.deleted", &[("name", &name)]).into());
        cx.notify();
    }

    /// クイックパーツ・フィールドコード（例: `TITLE`, `AUTHOR`, `PAGE` 等）を現在の選択・キャレット位置に挿入する。
    pub(crate) fn insert_quick_part_field(&mut self, field_code: &str, cx: &mut Context<Self>) -> bool {
        let trimmed = field_code.trim();
        if trimmed.is_empty() {
            return false;
        }
        let range = self.ordered_selection();
        let Some(doc) = self.flow() else { return false };
        if let Some((cmds, new_pos)) = flow_command::insert_field_at_range(doc, range, trimmed, false) {
            if self.run_flow(cmds, cx) {
                self.set_caret(new_pos, false, cx);
                self.scroll_to_caret(cx);
                return true;
            }
        }
        false
    }

    /// クイックパーツパネルの描画要素。
    pub(crate) fn quick_parts_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.doc_state()?.quick_parts.as_ref()?;
        self.render_quick_parts_panel(cx)
    }

    #[inline(never)]
    fn render_quick_parts_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?.quick_parts.as_ref()?;
        let active_tab = state.tab;
        let custom_input = state.custom_field.clone();
        let filter_input = state.filter_input.clone();
        let filter_text = state.filter_input.read(cx).value().trim().to_lowercase();
        let message = state.message.clone();

        let label = |tr_key: &'static str| {
            div().text_sm().font_weight(FontWeight::BOLD).text_color(text_secondary()).child(text(tr_key))
        };

        let mut body = v_flex().gap_3().p_0().child(
            crate::shell::chrome::tab_control_with_mode(
                [text("docx.quick-parts.tab.parts"), text("docx.quick-parts.tab.properties"), text("docx.quick-parts.tab.fields")],
                active_tab, "quick-parts-tabs", crate::shell::chrome::TabMode::Intrinsic,
                |e, tab, cx| {
                    if let Some(p) = e.doc_state_mut().and_then(|s| s.quick_parts.as_mut()) { p.tab = tab; }
                    cx.notify();
                }, cx,
            ),
        );

        // 状態メッセージ表示
        if let Some(msg) = message {
            body = body.child(
                div()
                    .p_2()
                    .rounded_md()
                    .bg(cx.theme().primary.opacity(0.1))
                    .text_sm()
                    .text_color(cx.theme().primary)
                    .child(msg),
            );
        }

        if active_tab == 0 {
        // 新規登録・編集フォーム表示
        if let Some(form) = &state.save_form {
            let name_in = form.name_input.clone();
            let cat_in = form.category_input.clone();
            let desc_in = form.description_input.clone();
            let cont_in = form.content_input.clone();
            let cur_gallery = form.gallery;
            let cur_scope = form.scope;
            let cur_opt = form.insert_option;
            let err = form.error.clone();

            let mut form_el = v_flex()
                .gap_2()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().muted.opacity(0.3))
                .child(div().font_weight(FontWeight::BOLD).text_sm().child(crate::shell::i18n::text("docx.quick-parts.save-title")));

            if let Some(e) = err {
                form_el = form_el.child(div().text_sm().text_color(cx.theme().danger).child(e));
            }

            // 名前
            form_el = form_el.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.quick-parts.name").to_string()))
                    .child(crate::shell::chrome::text_field(&name_in).full_width()),
            );

            // ギャラリー選択
            let mut gall_row = h_flex().gap_1();
            for g in [BuildingBlockGallery::AutoText, BuildingBlockGallery::QuickParts, BuildingBlockGallery::Custom] {
                let is_sel = g == cur_gallery;
                gall_row = gall_row.child(
                    crate::shell::chrome::radio_button(format!("gall-{:?}", g), is_sel, false, g.label(), cx.listener(move |this, _, _, cx| {
                            if let Some(form) = this.doc_state_mut().and_then(|s| s.quick_parts.as_mut()).and_then(|q| q.save_form.as_mut()) {
                                form.gallery = g;
                            }
                            cx.notify();
                        })),
                );
            }
            form_el = form_el.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.quick-parts.gallery").to_string()))
                    .child(gall_row),
            );

            // 分類 (Category)
            form_el = form_el.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.quick-parts.category").to_string()))
                    .child(crate::shell::chrome::text_field(&cat_in).full_width()),
            );

            // 保存先 (Scope)
            let mut scope_row = h_flex().gap_1();
            for sc in [BuildingBlockScope::Global, BuildingBlockScope::Document] {
                let is_sel = sc == cur_scope;
                scope_row = scope_row.child(
                    crate::shell::chrome::radio_button(format!("scope-{:?}", sc), is_sel, false, sc.label(), cx.listener(move |this, _, _, cx| {
                            if let Some(form) = this.doc_state_mut().and_then(|s| s.quick_parts.as_mut()).and_then(|q| q.save_form.as_mut()) {
                                form.scope = sc;
                            }
                            cx.notify();
                        })),
                );
            }
            form_el = form_el.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.quick-parts.scope").to_string()))
                    .child(scope_row),
            );

            // 挿入オプション (Insert Options)
            let mut opt_row = h_flex().gap_1().flex_wrap();
            for op in [BuildingBlockOption::ContentOnly, BuildingBlockOption::Paragraph, BuildingBlockOption::Page] {
                let is_sel = op == cur_opt;
                opt_row = opt_row.child(
                    crate::shell::chrome::radio_button(format!("opt-{:?}", op), is_sel, false, op.label(), cx.listener(move |this, _, _, cx| {
                            if let Some(form) = this.doc_state_mut().and_then(|s| s.quick_parts.as_mut()).and_then(|q| q.save_form.as_mut()) {
                                form.insert_option = op;
                            }
                            cx.notify();
                        })),
                );
            }
            form_el = form_el.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.quick-parts.options").to_string()))
                    .child(opt_row),
            );

            // 説明
            form_el = form_el.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.quick-parts.description").to_string()))
                    .child(crate::shell::chrome::text_field(&desc_in).full_width()),
            );

            // コンテンツ
            form_el = form_el.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.quick-parts.content").to_string()))
                    .child(crate::shell::chrome::text_field(&cont_in).full_width()),
            );

            // ボタン (保存 / キャンセル)
            form_el = form_el.child(
                crate::shell::chrome::action_bar()
                    .action(
                        crate::shell::chrome::ActionDefinition::new("btn-commit-bb", text("docx.quick-parts.save"), crate::shell::chrome::ActionButtonKind::Primary)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.quick_parts_commit_form(cx);
                            })),
                    )
                    .action(
                        crate::shell::chrome::ActionDefinition::new("btn-cancel-bb", text("docx.quick-parts.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.quick_parts_cancel_form(cx);
                            })),
                    ),
            );

            body = body.child(form_el);
        }

        // --- 登録済み文書パーツ一覧 ---
        body = body.child(label("docx.quick-parts.building-blocks"));
        body = body.child(crate::shell::chrome::text_field(&filter_input).full_width());

        let mut block_list = v_flex().gap_2();
        let filtered: Vec<BuildingBlock> = state.building_blocks.iter().filter(|b| {
            if filter_text.is_empty() {
                true
            } else {
                b.name.to_lowercase().contains(&filter_text)
                    || b.category.to_lowercase().contains(&filter_text)
                    || b.content.to_lowercase().contains(&filter_text)
            }
        }).cloned().collect();

        if filtered.is_empty() {
            block_list = block_list.child(
                div().text_sm().text_color(text_secondary()).p_2().child(text("docx.quick-parts.no-blocks")),
            );
        } else {
            for b in filtered {
                let _name = b.name.clone();
                let scope = b.scope;
                let preview = b.preview_snippet();
                let card = v_flex()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().background)
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .child(div().font_weight(FontWeight::BOLD).text_sm().child(b.name.clone()))
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .p_1()
                                            .rounded_sm()
                                            .bg(cx.theme().muted)
                                            .child(format!("{} | {}", b.category, b.scope.label())),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(
                                        crate::shell::chrome::action_button(format!("btn-insert-bb-{}", b.name))
                                            .small()
                                            .label(text("docx.quick-parts.insert"))
                                            .on_click(cx.listener({
                                                let name = b.name.clone();
                                                move |this, _, _, cx| {
                                                    this.quick_parts_insert_block(&name, cx);
                                                }
                                            })),
                                    )
                                    .child(
                                        crate::shell::chrome::action_button(format!("btn-del-bb-{}", b.name))
                                            .small().w(px(crate::shell::chrome::BUTTON_HEIGHT)).px_0()
                                            .icon(crate::shell::icons::Material("delete"))
                                            .on_click(cx.listener({
                                                let name = b.name.clone();
                                                move |this, _, _, cx| {
                                                    this.quick_parts_delete_block(&name, scope, cx);
                                                }
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        div().text_sm().text_color(text_secondary()).child(preview),
                    );
                block_list = block_list.child(card);
            }
        }
        body = body.child(block_list);
        }

        // --- ドキュメント プロパティ ---
        let doc_props = [
            ("TITLE", "docx.quick-parts.prop.title"),
            ("AUTHOR", "docx.quick-parts.prop.author"),
            ("SUBJECT", "docx.quick-parts.prop.subject"),
            (r#"DOCPROPERTY "Company""#, "docx.quick-parts.prop.company"),
            ("COMMENTS", "docx.quick-parts.prop.comments"),
            ("CREATEDATE", "docx.quick-parts.prop.createdate"),
            ("SAVEDATE", "docx.quick-parts.prop.savedate"),
        ];

        let mut prop_grid = h_flex().gap_1().flex_wrap();
        for (code, tr_key) in doc_props {
            prop_grid = prop_grid.child(
                crate::shell::chrome::action_button(format!("qp-prop-{code}"))
                    .small()
                    .label(text(tr_key))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.insert_quick_part_field(code, cx);
                    })),
            );
        }

        let standard_fields = [
            ("PAGE", "docx.quick-parts.field.page"),
            ("NUMPAGES", "docx.quick-parts.field.numpages"),
            ("FILENAME", "docx.quick-parts.field.filename"),
            ("AUTOTEXT", "docx.quick-parts.field.autotext"),
            ("DATE", "docx.quick-parts.field.date"),
            ("TIME", "docx.quick-parts.field.time"),
        ];

        let mut field_grid = h_flex().gap_1().flex_wrap();
        for (code, label_str) in standard_fields {
            field_grid = field_grid.child(
                crate::shell::chrome::action_button(format!("qp-field-{code}"))
                    .small()
                    .label(text(label_str))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.insert_quick_part_field(code, cx);
                    })),
            );
        }

        if active_tab == 1 {
            body = body.child(prop_grid);
        } else if active_tab == 2 {
            body = body.child(field_grid)
                .child(label("docx.quick-parts.custom-field"))
                .child(crate::shell::chrome::text_field(&custom_input).full_width());
        }
        if active_tab != 2 {
            return Some(SidebarPanel { title: text("docx.quick-parts"), body: body.into_any_element() });
        }
        let panel_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("btn-insert-custom-field", text("docx.quick-parts.insert"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| {
                        let code = this.doc_state()
                            .and_then(|s| s.quick_parts.as_ref())
                            .map(|qp| qp.custom_field.read(cx).value().trim().to_string())
                            .unwrap_or_default();
                        if !code.is_empty() {
                            this.insert_quick_part_field(&code, cx);
                        }
                    })),);
        Some(SidebarPanel::with_footer(text("docx.quick-parts"), body.into_any_element(), panel_actions))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BuildingBlock, BuildingBlockGallery, BuildingBlockOption, BuildingBlockScope,
        decode_building_blocks, default_building_blocks, encode_building_blocks,
        load_document_building_blocks, save_document_building_blocks_command,
    };
    use ibis_doc_model::{FlowCommand, FlowDocument};

    #[test]
    fn test_building_block_encode_decode() {
        let block = BuildingBlock {
            name: "TestBlock".to_string(),
            gallery: BuildingBlockGallery::AutoText,
            category: "General".to_string(),
            description: "Sample Description\nWith multiple lines".to_string(),
            content: "First line\twith tab\nSecond line".to_string(),
            insert_option: BuildingBlockOption::Paragraph,
            scope: BuildingBlockScope::Document,
        };

        let encoded = block.encode();
        assert!(!encoded.contains('\n'));
        let decoded = BuildingBlock::decode(&encoded).expect("should decode cleanly");

        assert_eq!(decoded.name, "TestBlock");
        assert_eq!(decoded.gallery, BuildingBlockGallery::AutoText);
        assert_eq!(decoded.category, "General");
        assert_eq!(decoded.description, "Sample Description\nWith multiple lines");
        assert_eq!(decoded.content, "First line\twith tab\nSecond line");
        assert_eq!(decoded.insert_option, BuildingBlockOption::Paragraph);
        assert_eq!(decoded.scope, BuildingBlockScope::Document);
    }

    #[test]
    fn test_encode_and_decode_building_blocks_list() {
        let blocks = vec![
            BuildingBlock {
                name: "Block1".to_string(),
                gallery: BuildingBlockGallery::QuickParts,
                category: "A".to_string(),
                description: "D1".to_string(),
                content: "Content 1".to_string(),
                insert_option: BuildingBlockOption::ContentOnly,
                scope: BuildingBlockScope::Global,
            },
            BuildingBlock {
                name: "Block2".to_string(),
                gallery: BuildingBlockGallery::Custom,
                category: "B".to_string(),
                description: "D2".to_string(),
                content: "Content 2".to_string(),
                insert_option: BuildingBlockOption::Page,
                scope: BuildingBlockScope::Document,
            },
        ];

        let encoded = encode_building_blocks(&blocks);
        let decoded = decode_building_blocks(&encoded);
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].name, "Block1");
        assert_eq!(decoded[1].name, "Block2");
        assert_eq!(decoded[1].insert_option, BuildingBlockOption::Page);
    }

    #[test]
    fn test_load_and_save_document_building_blocks() {
        let mut doc = FlowDocument::default();
        let initial = load_document_building_blocks(&doc);
        assert!(initial.is_empty());

        let blocks = vec![
            BuildingBlock {
                name: "DocSpecific".to_string(),
                gallery: BuildingBlockGallery::AutoText,
                category: "Doc".to_string(),
                description: "Doc only".to_string(),
                content: "Important doc footer".to_string(),
                insert_option: BuildingBlockOption::Paragraph,
                scope: BuildingBlockScope::Document,
            },
        ];

        let cmd = save_document_building_blocks_command(&doc, &blocks).expect("command generated");
        if let FlowCommand::SetDocVariables(vars) = cmd {
            doc.doc_variables = vars;
        }

        let loaded = load_document_building_blocks(&doc);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "DocSpecific");
        assert_eq!(loaded[0].content, "Important doc footer");
    }

    #[test]
    fn test_default_building_blocks() {
        let defaults = default_building_blocks();
        assert!(!defaults.is_empty());
        assert!(defaults.iter().any(|b| b.name == "会社署名"));
        assert!(defaults.iter().any(|b| b.name == "社外秘ヘッダー"));
    }
}
