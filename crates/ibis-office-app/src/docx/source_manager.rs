// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 資料文献の管理 (Source Manager: DOCX-REF-016):
//! Word 標準の引用文献・資料文献マネージャー。
//! - マスター一覧 (Master List): ユーザー環境で共通利用できる文献一覧（永続設定 `docx.sources.master`）
//! - 現在の文書一覧 (Current Document List): 現在の文書内で使われる文献一覧（`doc_variables["_sources"]` および `CITATION` フィールドから収集）
//! - マスターと文書間での相互コピー (<- コピー / コピー ->)
//! - 新規作成 (New)、編集 (Edit)、削除 (Delete)
//! - 引用文献・文献目録プレビュー (Citation / Bibliography Preview)
//! - 引用文献のワンクリック挿入 (`CITATION <tag>`)

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{FlowCommand, FlowDocument};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 文献の種類 (Source Type)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum SourceType {
    #[default]
    Book,
    BookSection,
    JournalArticle,
    Report,
    Website,
    ConferenceProceedings,
    Miscellaneous,
}

impl SourceType {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Book => "Book",
            Self::BookSection => "BookSection",
            Self::JournalArticle => "JournalArticle",
            Self::Report => "Report",
            Self::Website => "Website",
            Self::ConferenceProceedings => "ConferenceProceedings",
            Self::Miscellaneous => "Miscellaneous",
        }
    }

    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "Book" => Self::Book,
            "BookSection" => Self::BookSection,
            "JournalArticle" => Self::JournalArticle,
            "Report" => Self::Report,
            "Website" => Self::Website,
            "ConferenceProceedings" => Self::ConferenceProceedings,
            _ => Self::Miscellaneous,
        }
    }

    pub(crate) fn label_key(&self) -> &'static str {
        match self {
            Self::Book => "docx.reference.source.type.book",
            Self::BookSection => "docx.reference.source.type.book-section",
            Self::JournalArticle => "docx.reference.source.type.journal-article",
            Self::Report => "docx.reference.source.type.report",
            Self::Website => "docx.reference.source.type.website",
            Self::ConferenceProceedings => "docx.reference.source.type.proceedings",
            Self::Miscellaneous => "docx.reference.source.type.misc",
        }
    }

    pub(crate) fn all() -> &'static [SourceType] {
        &[
            Self::Book,
            Self::BookSection,
            Self::JournalArticle,
            Self::Report,
            Self::Website,
            Self::ConferenceProceedings,
            Self::Miscellaneous,
        ]
    }
}

/// 1件の資料文献エントリ
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct SourceEntry {
    /// タグ名 (識別子: CITATION フィールド引数、例: "Smith2024", "Yamada2024")
    pub(crate) tag: String,
    /// 文献の種類
    pub(crate) source_type: SourceType,
    /// 著者名 (例: "Smith, John", "山田 太郎")
    pub(crate) author: String,
    /// タイトル (例: "Office Suite Design")
    pub(crate) title: String,
    /// 発行年 (例: "2024")
    pub(crate) year: String,
    /// 発行元・出版社 (例: "Ibis Press")
    pub(crate) publisher: String,
    /// 都市 (例: "Tokyo")
    pub(crate) city: String,
    /// URL (Webサイト用)
    pub(crate) url: String,
}

impl SourceEntry {
    /// 引用文献プレビュー（例: `(Yamada, 2024)`）
    pub(crate) fn citation_preview(&self) -> String {
        let auth = self.author.trim();
        let yr = self.year.trim();
        if !auth.is_empty() && !yr.is_empty() {
            format!("({auth}, {yr})")
        } else if !auth.is_empty() {
            format!("({auth})")
        } else if !yr.is_empty() {
            format!("({}, {yr})", self.tag)
        } else {
            format!("({})", self.tag)
        }
    }

    /// 文献目録プレビュー（例: `山田 太郎 (2024). Office Suite Design. Tokyo: Ibis Press.`）
    pub(crate) fn bibliography_preview(&self) -> String {
        let mut parts = Vec::new();
        let auth = self.author.trim();
        if !auth.is_empty() {
            parts.push(auth.to_string());
        }
        let yr = self.year.trim();
        if !yr.is_empty() {
            parts.push(format!("({yr})."));
        }
        let title = self.title.trim();
        if !title.is_empty() {
            parts.push(format!("{title}."));
        }
        match self.source_type {
            SourceType::Website => {
                let url = self.url.trim();
                if !url.is_empty() {
                    parts.push(url.to_string());
                }
            }
            _ => {
                let city = self.city.trim();
                let publ = self.publisher.trim();
                if !city.is_empty() && !publ.is_empty() {
                    parts.push(format!("{city}: {publ}."));
                } else if !publ.is_empty() {
                    parts.push(format!("{publ}."));
                } else if !city.is_empty() {
                    parts.push(format!("{city}."));
                }
            }
        }
        if parts.is_empty() {
            format!("[{}] {}", self.tag, self.tag)
        } else {
            parts.join(" ")
        }
    }

    /// TSV エンコード
    pub(crate) fn encode(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            escape_field(&self.tag),
            self.source_type.as_str(),
            escape_field(&self.author),
            escape_field(&self.title),
            escape_field(&self.year),
            escape_field(&self.publisher),
            escape_field(&self.city),
            escape_field(&self.url),
        )
    }

    /// TSV デコード
    pub(crate) fn decode(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 {
            return None;
        }
        let tag = unescape_field(parts.first()?);
        if tag.is_empty() {
            return None;
        }
        let source_type = SourceType::from_str(parts.get(1).unwrap_or(&""));
        let author = parts.get(2).map(|s| unescape_field(s)).unwrap_or_default();
        let title = parts.get(3).map(|s| unescape_field(s)).unwrap_or_default();
        let year = parts.get(4).map(|s| unescape_field(s)).unwrap_or_default();
        let publisher = parts.get(5).map(|s| unescape_field(s)).unwrap_or_default();
        let city = parts.get(6).map(|s| unescape_field(s)).unwrap_or_default();
        let url = parts.get(7).map(|s| unescape_field(s)).unwrap_or_default();

        Some(Self {
            tag,
            source_type,
            author,
            title,
            year,
            publisher,
            city,
            url,
        })
    }
}

fn escape_field(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}

fn unescape_field(s: &str) -> String {
    s.replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\")
}

pub(crate) fn encode_sources(sources: &[SourceEntry]) -> String {
    sources.iter().map(|s| s.encode()).collect::<Vec<_>>().join("\r")
}

pub(crate) fn decode_sources(s: &str) -> Vec<SourceEntry> {
    s.split(|c| c == '\n' || c == '\r')
        .filter(|line| !line.trim().is_empty())
        .filter_map(SourceEntry::decode)
        .collect()
}

/// マスター一覧設定キー
const MASTER_SOURCES_KEY: &str = "docx.sources.master";
/// 文書内 doc_variables 保存キー
pub(crate) const DOC_SOURCES_VAR: &str = "_sources";

/// マスター一覧の読込
pub(crate) fn load_master_sources() -> Vec<SourceEntry> {
    if let Some(data) = crate::shell::settings::get(MASTER_SOURCES_KEY) {
        let entries = decode_sources(&data);
        if !entries.is_empty() {
            return entries;
        }
    }
    // 既定のサンプルエントリ
    vec![
        SourceEntry {
            tag: "Ecma376".to_string(),
            source_type: SourceType::Report,
            author: "Ecma International".to_string(),
            title: "Office Open XML File Formats (ECMA-376)".to_string(),
            year: "2021".to_string(),
            publisher: "Ecma International".to_string(),
            city: "Geneva".to_string(),
            url: "https://www.ecma-international.org/publications-and-standards/standards/ecma-376/".to_string(),
        },
    ]
}

/// マスター一覧の保存
pub(crate) fn save_master_sources(sources: &[SourceEntry]) {
    let data = encode_sources(sources);
    crate::shell::settings::set(MASTER_SOURCES_KEY, &data);
}

/// 文書一覧の読込（doc_variables["_sources"] と CITATION フィールドの結合）
pub(crate) fn load_document_sources(doc: &FlowDocument) -> Vec<SourceEntry> {
    let mut sources: Vec<SourceEntry> = doc.doc_variables
        .get(DOC_SOURCES_VAR)
        .map(|s| decode_sources(s))
        .unwrap_or_default();

    // 文書内の CITATION フィールドからタグを収集し、未登録のものがあればスタブを追加
    let citations = super::bibliography::collect_citations(doc);
    for c in citations {
        if !sources.iter().any(|s| s.tag.eq_ignore_ascii_case(&c.tag)) {
            sources.push(SourceEntry {
                tag: c.tag,
                source_type: SourceType::Miscellaneous,
                author: String::new(),
                title: String::new(),
                year: String::new(),
                publisher: String::new(),
                city: String::new(),
                url: String::new(),
            });
        }
    }

    sources.sort_by(|a, b| a.tag.to_lowercase().cmp(&b.tag.to_lowercase()));
    sources
}

/// 文書から指定タグの資料文献を検索
pub(crate) fn find_source_in_doc(doc: &FlowDocument, tag: &str) -> Option<SourceEntry> {
    load_document_sources(doc).into_iter().find(|s| s.tag.eq_ignore_ascii_case(tag))
}

/// 文書一覧を doc_variables に保存するコマンド
pub(crate) fn save_document_sources_command(
    doc: &FlowDocument,
    sources: &[SourceEntry],
) -> Option<FlowCommand> {
    let data = encode_sources(sources);
    let mut vars = doc.doc_variables.clone();
    vars.insert(DOC_SOURCES_VAR.to_string(), data);
    Some(FlowCommand::SetDocVariables(vars))
}

/// 現在選択中のリスト種別
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveList {
    Master,
    Current,
}

/// 新規作成・編集フォーム状態
pub(crate) struct SourceEditFormState {
    pub(crate) is_new: bool,
    pub(crate) target_list: ActiveList,
    pub(crate) original_tag: String,
    pub(crate) tag_input: Entity<InputState>,
    pub(crate) source_type: SourceType,
    pub(crate) author_input: Entity<InputState>,
    pub(crate) title_input: Entity<InputState>,
    pub(crate) year_input: Entity<InputState>,
    pub(crate) publisher_input: Entity<InputState>,
    pub(crate) city_input: Entity<InputState>,
    pub(crate) url_input: Entity<InputState>,
    pub(crate) error: Option<String>,
}

/// サイドバーパネル状態
pub(crate) struct SourceManagerState {
    pub(crate) master_list: Vec<SourceEntry>,
    pub(crate) current_list: Vec<SourceEntry>,
    pub(crate) active_list: ActiveList,
    pub(crate) selected_master: Option<usize>,
    pub(crate) selected_current: Option<usize>,
    pub(crate) edit_form: Option<SourceEditFormState>,
    pub(crate) filter_input: Entity<InputState>,
    pub(crate) message: Option<String>,
}

impl SlideEditor {
    /// 資料文献の管理パネルを開く
    pub(crate) fn open_source_manager_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        self.end_text_edit(window, cx);
        let master = load_master_sources();
        let current = self.flow().map(|d| load_document_sources(d)).unwrap_or_default();
        let filter_input = cx.new(|cx| InputState::new(window, cx).placeholder(text("docx.source-manager.filter-placeholder")));

        if let Some(s) = self.doc_state_mut() {
            s.source_manager = Some(SourceManagerState {
                master_list: master,
                current_list: current,
                active_list: ActiveList::Current,
                selected_master: None,
                selected_current: None,
                edit_form: None,
                filter_input,
                message: None,
            });
            // 他の競合パネルを閉じる
            s.print_panel = None;
            s.inspect_panel = None;
            s.compatibility_panel = None;
            s.quick_parts = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 資料文献の管理パネルを閉じる
    pub(crate) fn close_source_manager_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.source_manager = None;
        }
        cx.notify();
    }

    /// マスターから現在の文書へコピー (<- コピー)
    pub(crate) fn source_manager_copy_master_to_current(&mut self, cx: &mut Context<Self>) {
        let Some(doc_clone) = self.flow().cloned() else { return };
        let entry = {
            let Some(s) = self.doc_state() else { return };
            let Some(mgr) = s.source_manager.as_ref() else { return };
            let Some(idx) = mgr.selected_master else { return };
            let Some(entry) = mgr.master_list.get(idx).cloned() else { return };
            entry
        };

        let mut current = {
            let Some(s) = self.doc_state() else { return };
            s.source_manager.as_ref().map(|m| m.current_list.clone()).unwrap_or_default()
        };

        if let Some(pos) = current.iter().position(|s| s.tag.eq_ignore_ascii_case(&entry.tag)) {
            current[pos] = entry.clone();
        } else {
            current.push(entry.clone());
            current.sort_by(|a, b| a.tag.to_lowercase().cmp(&b.tag.to_lowercase()));
        }

        if let Some(cmd) = save_document_sources_command(&doc_clone, &current) {
            self.run_flow(vec![cmd], cx);
        }

        if let Some(s) = self.doc_state_mut().and_then(|s| s.source_manager.as_mut()) {
            s.current_list = current;
            s.message = Some(crate::shell::i18n::format("docx.source-manager.added-current", &[("tag", &entry.tag)]));
        }
        cx.notify();
    }

    /// 現在の文書からマスターへコピー (コピー ->)
    pub(crate) fn source_manager_copy_current_to_master(&mut self, cx: &mut Context<Self>) {
        let entry = {
            let Some(s) = self.doc_state() else { return };
            let Some(mgr) = s.source_manager.as_ref() else { return };
            let Some(idx) = mgr.selected_current else { return };
            let Some(entry) = mgr.current_list.get(idx).cloned() else { return };
            entry
        };

        let mut master = {
            let Some(s) = self.doc_state() else { return };
            s.source_manager.as_ref().map(|m| m.master_list.clone()).unwrap_or_default()
        };

        if let Some(pos) = master.iter().position(|s| s.tag.eq_ignore_ascii_case(&entry.tag)) {
            master[pos] = entry.clone();
        } else {
            master.push(entry.clone());
            master.sort_by(|a, b| a.tag.to_lowercase().cmp(&b.tag.to_lowercase()));
        }

        save_master_sources(&master);

        if let Some(s) = self.doc_state_mut().and_then(|s| s.source_manager.as_mut()) {
            s.master_list = master;
            s.message = Some(crate::shell::i18n::format("docx.source-manager.saved-master", &[("tag", &entry.tag)]));
        }
        cx.notify();
    }

    /// 選択中の文献を削除
    pub(crate) fn source_manager_delete_selected(&mut self, cx: &mut Context<Self>) {
        let active = {
            let Some(s) = self.doc_state() else { return };
            let Some(mgr) = s.source_manager.as_ref() else { return };
            mgr.active_list
        };

        match active {
            ActiveList::Master => {
                let Some(s) = self.doc_state_mut().and_then(|s| s.source_manager.as_mut()) else { return };
                if let Some(idx) = s.selected_master {
                    if idx < s.master_list.len() {
                        let removed = s.master_list.remove(idx);
                        s.selected_master = None;
                        save_master_sources(&s.master_list);
                        s.message = Some(crate::shell::i18n::format("docx.source-manager.deleted-master", &[("tag", &removed.tag)]));
                    }
                }
            }
            ActiveList::Current => {
                let Some(doc_clone) = self.flow().cloned() else { return };
                let (removed, current) = {
                    let Some(s) = self.doc_state_mut().and_then(|s| s.source_manager.as_mut()) else { return };
                    let Some(idx) = s.selected_current else { return };
                    if idx >= s.current_list.len() { return };
                    let removed = s.current_list.remove(idx);
                    s.selected_current = None;
                    (removed, s.current_list.clone())
                };

                if let Some(cmd) = save_document_sources_command(&doc_clone, &current) {
                    self.run_flow(vec![cmd], cx);
                }
                if let Some(s) = self.doc_state_mut().and_then(|s| s.source_manager.as_mut()) {
                    s.message = Some(crate::shell::i18n::format("docx.source-manager.deleted-current", &[("tag", &removed.tag)]));
                }
            }
        }
        cx.notify();
    }

    /// 新規作成フォームを開く
    pub(crate) fn source_manager_start_new(
        &mut self,
        target: ActiveList,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tag_input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.source-manager.tag-placeholder")));
        let author_input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.source-manager.author-placeholder")));
        let title_input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.source-manager.title-placeholder")));
        let year_input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.source-manager.year-placeholder")));
        let publisher_input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.source-manager.publisher-placeholder")));
        let city_input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.source-manager.city-placeholder")));
        let url_input = cx.new(|cx| InputState::new(window, cx).placeholder("URL"));

        tag_input.update(cx, |i, cx| i.focus(window, cx));

        if let Some(s) = self.doc_state_mut().and_then(|s| s.source_manager.as_mut()) {
            s.edit_form = Some(SourceEditFormState {
                is_new: true,
                target_list: target,
                original_tag: String::new(),
                tag_input,
                source_type: SourceType::Book,
                author_input,
                title_input,
                year_input,
                publisher_input,
                city_input,
                url_input,
                error: None,
            });
            s.message = None;
        }
        cx.notify();
    }

    /// 選択中の文献の編集フォームを開く
    pub(crate) fn source_manager_start_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (target, entry) = {
            let Some(s) = self.doc_state() else { return };
            let Some(mgr) = s.source_manager.as_ref() else { return };
            match mgr.active_list {
                ActiveList::Master => {
                    let Some(idx) = mgr.selected_master else { return };
                    (ActiveList::Master, mgr.master_list.get(idx).cloned())
                }
                ActiveList::Current => {
                    let Some(idx) = mgr.selected_current else { return };
                    (ActiveList::Current, mgr.current_list.get(idx).cloned())
                }
            }
        };
        let Some(entry) = entry else { return };

        let tag_input = cx.new(|cx| {
            let mut inp = InputState::new(window, cx);
            inp.set_value(&entry.tag, window, cx);
            inp
        });
        let author_input = cx.new(|cx| {
            let mut inp = InputState::new(window, cx);
            inp.set_value(&entry.author, window, cx);
            inp
        });
        let title_input = cx.new(|cx| {
            let mut inp = InputState::new(window, cx);
            inp.set_value(&entry.title, window, cx);
            inp
        });
        let year_input = cx.new(|cx| {
            let mut inp = InputState::new(window, cx);
            inp.set_value(&entry.year, window, cx);
            inp
        });
        let publisher_input = cx.new(|cx| {
            let mut inp = InputState::new(window, cx);
            inp.set_value(&entry.publisher, window, cx);
            inp
        });
        let city_input = cx.new(|cx| {
            let mut inp = InputState::new(window, cx);
            inp.set_value(&entry.city, window, cx);
            inp
        });
        let url_input = cx.new(|cx| {
            let mut inp = InputState::new(window, cx);
            inp.set_value(&entry.url, window, cx);
            inp
        });

        if let Some(s) = self.doc_state_mut().and_then(|s| s.source_manager.as_mut()) {
            s.edit_form = Some(SourceEditFormState {
                is_new: false,
                target_list: target,
                original_tag: entry.tag.clone(),
                tag_input,
                source_type: entry.source_type,
                author_input,
                title_input,
                year_input,
                publisher_input,
                city_input,
                url_input,
                error: None,
            });
            s.message = None;
        }
        cx.notify();
    }

    /// 編集フォームをキャンセル
    pub(crate) fn source_manager_cancel_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut().and_then(|s| s.source_manager.as_mut()) {
            s.edit_form = None;
        }
        cx.notify();
    }

    /// 編集フォームの内容を保存
    pub(crate) fn source_manager_commit_edit(&mut self, cx: &mut Context<Self>) {
        let (is_new, target, original_tag, entry) = {
            let Some(s) = self.doc_state() else { return };
            let Some(mgr) = s.source_manager.as_ref() else { return };
            let Some(form) = mgr.edit_form.as_ref() else { return };

            let tag = form.tag_input.read(cx).value().trim().to_string();
            if tag.is_empty() {
                if let Some(s) = self.doc_state_mut().and_then(|s| s.source_manager.as_mut()).and_then(|m| m.edit_form.as_mut()) {
                    s.error = Some(crate::shell::i18n::text("docx.source-manager.tag-required").to_string());
                }
                cx.notify();
                return;
            }

            let entry = SourceEntry {
                tag,
                source_type: form.source_type,
                author: form.author_input.read(cx).value().trim().to_string(),
                title: form.title_input.read(cx).value().trim().to_string(),
                year: form.year_input.read(cx).value().trim().to_string(),
                publisher: form.publisher_input.read(cx).value().trim().to_string(),
                city: form.city_input.read(cx).value().trim().to_string(),
                url: form.url_input.read(cx).value().trim().to_string(),
            };

            (form.is_new, form.target_list, form.original_tag.clone(), entry)
        };

        match target {
            ActiveList::Master => {
                let Some(s) = self.doc_state_mut().and_then(|s| s.source_manager.as_mut()) else { return };
                if is_new {
                    if let Some(pos) = s.master_list.iter().position(|x| x.tag.eq_ignore_ascii_case(&entry.tag)) {
                        s.master_list[pos] = entry.clone();
                    } else {
                        s.master_list.push(entry.clone());
                    }
                } else if let Some(pos) = s.master_list.iter().position(|x| x.tag.eq_ignore_ascii_case(&original_tag)) {
                    s.master_list[pos] = entry.clone();
                }
                s.master_list.sort_by(|a, b| a.tag.to_lowercase().cmp(&b.tag.to_lowercase()));
                save_master_sources(&s.master_list);
                s.edit_form = None;
                s.message = Some(crate::shell::i18n::format("docx.source-manager.saved-master2", &[("tag", &entry.tag)]));
            }
            ActiveList::Current => {
                let Some(doc_clone) = self.flow().cloned() else { return };
                let current = {
                    let Some(s) = self.doc_state_mut().and_then(|s| s.source_manager.as_mut()) else { return };
                    if is_new {
                        if let Some(pos) = s.current_list.iter().position(|x| x.tag.eq_ignore_ascii_case(&entry.tag)) {
                            s.current_list[pos] = entry.clone();
                        } else {
                            s.current_list.push(entry.clone());
                        }
                    } else if let Some(pos) = s.current_list.iter().position(|x| x.tag.eq_ignore_ascii_case(&original_tag)) {
                        s.current_list[pos] = entry.clone();
                    }
                    s.current_list.sort_by(|a, b| a.tag.to_lowercase().cmp(&b.tag.to_lowercase()));
                    s.edit_form = None;
                    s.message = Some(crate::shell::i18n::format("docx.source-manager.saved-current", &[("tag", &entry.tag)]));
                    s.current_list.clone()
                };

                if let Some(cmd) = save_document_sources_command(&doc_clone, &current) {
                    self.run_flow(vec![cmd], cx);
                }
            }
        }
        cx.notify();
    }

    /// 選択中の文献の CITATION フィールドを挿入
    pub(crate) fn source_manager_insert_selected_citation(&mut self, cx: &mut Context<Self>) {
        let tag = {
            let Some(s) = self.doc_state() else { return };
            let Some(mgr) = s.source_manager.as_ref() else { return };
            match mgr.active_list {
                ActiveList::Master => mgr.selected_master.and_then(|i| mgr.master_list.get(i).map(|e| e.tag.clone())),
                ActiveList::Current => mgr.selected_current.and_then(|i| mgr.current_list.get(i).map(|e| e.tag.clone())),
            }
        };

        if let Some(tag) = tag {
            self.insert_citation(&tag, cx);
            if let Some(s) = self.doc_state_mut().and_then(|s| s.source_manager.as_mut()) {
                s.message = Some(crate::shell::i18n::format("docx.source-manager.citation-inserted", &[("tag", &tag)]));
            }
            cx.notify();
        }
    }

    /// 資料文献の管理サイドバーパネルの描画
    pub(crate) fn doc_source_manager_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.doc_state()?.source_manager.as_ref()?;
        self.render_doc_source_manager_panel(cx)
    }

    #[inline(never)]
    fn render_doc_source_manager_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let (master_list, current_list, active_list, selected_master, selected_current, edit_form, filter_input, message) = {
            let s = self.doc_state()?;
            let mgr = s.source_manager.as_ref()?;
            (
                mgr.master_list.clone(),
                mgr.current_list.clone(),
                mgr.active_list,
                mgr.selected_master,
                mgr.selected_current,
                mgr.edit_form.as_ref().map(|f| (
                    f.is_new,
                    f.target_list,
                    f.tag_input.clone(),
                    f.source_type,
                    f.author_input.clone(),
                    f.title_input.clone(),
                    f.year_input.clone(),
                    f.publisher_input.clone(),
                    f.city_input.clone(),
                    f.url_input.clone(),
                    f.error.clone(),
                )),
                mgr.filter_input.clone(),
                mgr.message.clone(),
            )
        };

        let mut body = v_flex().gap_2().p_3().p_0();

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

        // 新規作成／編集フォーム表示中の場合
        if let Some((is_new, _target, tag_in, st, auth_in, title_in, yr_in, pub_in, city_in, url_in, err)) = edit_form {
            let form_title = if is_new {
                text("docx.source-manager.form.new-title")
            } else {
                text("docx.source-manager.form.edit-title")
            };
            let mut form = v_flex()
                .gap_2()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().muted.opacity(0.3))
                .child(div().font_weight(FontWeight::BOLD).text_sm().child(form_title.to_string()));

            if let Some(err_msg) = err {
                form = form.child(div().text_sm().text_color(cx.theme().danger).child(err_msg));
            }

            // タグ
            form = form.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.source-manager.form.tag").to_string()))
                    .child(crate::shell::chrome::text_field(&tag_in).full_width()),
            );

            // 文献の種類切替ボタン
            let mut type_row = h_flex().gap_1().flex_wrap();
            for t in SourceType::all() {
                let is_sel = *t == st;
                let t_val = *t;
                type_row = type_row.child(
                    crate::shell::chrome::radio_button(format!("st-{:?}", t_val), is_sel, false, text(t_val.label_key()), cx.listener(move |this, _, _, cx| {
                            if let Some(form) = this.doc_state_mut().and_then(|s| s.source_manager.as_mut()).and_then(|m| m.edit_form.as_mut()) {
                                form.source_type = t_val;
                            }
                            cx.notify();
                        })),
                );
            }
            form = form.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.source-manager.form.type").to_string()))
                    .child(type_row),
            );

            // 著者
            form = form.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.source-manager.form.author").to_string()))
                    .child(crate::shell::chrome::text_field(&auth_in).full_width()),
            );

            // タイトル
            form = form.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.source-manager.form.title").to_string()))
                    .child(crate::shell::chrome::text_field(&title_in).full_width()),
            );

            // 年
            form = form.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.source-manager.form.year").to_string()))
                    .child(crate::shell::chrome::text_field(&yr_in).full_width()),
            );

            // 発行元
            form = form.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.source-manager.form.publisher").to_string()))
                    .child(crate::shell::chrome::text_field(&pub_in).full_width()),
            );

            // 都市
            form = form.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.source-manager.form.city").to_string()))
                    .child(crate::shell::chrome::text_field(&city_in).full_width()),
            );

            // URL
            form = form.child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.source-manager.form.url").to_string()))
                    .child(crate::shell::chrome::text_field(&url_in).full_width()),
            );

            // フォームアクション
            form = form.child(
                crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar()
                    .action(
                        crate::shell::chrome::ActionDefinition::new("src-form-save", text("docx.source-manager.form.save"), crate::shell::chrome::ActionButtonKind::Primary)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.source_manager_commit_edit(cx);
                            })),
                    )
                    .action(
                        crate::shell::chrome::ActionDefinition::new("src-form-cancel", text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.source_manager_cancel_edit(cx);
                            })),
                    ))),
            );

            body = body.child(form);
            return Some(SidebarPanel {
                title: text("docx.source-manager.title").into(),
                body: body.into_any_element(),
            });
        }

        // 検索フィルター
        body = body.child(
            v_flex()
                .gap_1()
                .child(crate::shell::chrome::text_field(&filter_input).full_width()),
        );

        let query = filter_input.read(cx).value().trim().to_lowercase();

        // 引用されているタグ一覧
        let cited_tags: Vec<String> = self.flow()
            .map(|d| super::bibliography::collect_citations(d).into_iter().map(|c| c.tag).collect())
            .unwrap_or_default();

        // --- マスター一覧と現在の文書一覧の 2 カラムレイアウト ---
        let mut master_col = v_flex().gap_1().flex_1();
        master_col = master_col.child(
            div()
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .text_color(if active_list == ActiveList::Master { cx.theme().primary } else { cx.theme().foreground })
                .child(format!("{} ({})", text("docx.source-manager.master-list"), master_list.len())),
        );

        let mut current_col = v_flex().gap_1().flex_1();
        current_col = current_col.child(
            div()
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .text_color(if active_list == ActiveList::Current { cx.theme().primary } else { cx.theme().foreground })
                .child(format!("{} ({})", text("docx.source-manager.current-list"), current_list.len())),
        );

        // マスターエントリ一覧
        for (idx, entry) in master_list.iter().enumerate() {
            if !query.is_empty() && !entry.tag.to_lowercase().contains(&query) && !entry.author.to_lowercase().contains(&query) && !entry.title.to_lowercase().contains(&query) {
                continue;
            }
            let is_sel = active_list == ActiveList::Master && selected_master == Some(idx);
            let tag_name = entry.tag.clone();
            let author_preview = if entry.author.is_empty() { String::new() } else { format!(" - {}", entry.author) };

            master_col = master_col.child(
                crate::shell::chrome::action_button(format!("master-src-{idx}"))
                    .small()

                    .bg(if is_sel { cx.theme().primary.opacity(0.18) } else { cx.theme().background })
                    .border_1()
                    .border_color(if is_sel { cx.theme().primary } else { cx.theme().border })
                    .label(format!("{}{}", tag_name, author_preview))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(s) = this.doc_state_mut().and_then(|s| s.source_manager.as_mut()) {
                            s.active_list = ActiveList::Master;
                            s.selected_master = Some(idx);
                        }
                        cx.notify();
                    })),
            );
        }

        // 文書エントリ一覧
        for (idx, entry) in current_list.iter().enumerate() {
            if !query.is_empty() && !entry.tag.to_lowercase().contains(&query) && !entry.author.to_lowercase().contains(&query) && !entry.title.to_lowercase().contains(&query) {
                continue;
            }
            let is_sel = active_list == ActiveList::Current && selected_current == Some(idx);
            let is_cited = cited_tags.iter().any(|t| t.eq_ignore_ascii_case(&entry.tag));
            let mark = if is_cited { "✓ " } else { "   " };
            let tag_name = entry.tag.clone();
            let author_preview = if entry.author.is_empty() { String::new() } else { format!(" - {}", entry.author) };

            current_col = current_col.child(
                crate::shell::chrome::action_button(format!("current-src-{idx}"))
                    .small()

                    .bg(if is_sel { cx.theme().primary.opacity(0.18) } else { cx.theme().background })
                    .border_1()
                    .border_color(if is_sel { cx.theme().primary } else { cx.theme().border })
                    .label(format!("{}{}{}", mark, tag_name, author_preview))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(s) = this.doc_state_mut().and_then(|s| s.source_manager.as_mut()) {
                            s.active_list = ActiveList::Current;
                            s.selected_current = Some(idx);
                        }
                        cx.notify();
                    })),
            );
        }

        body = body.child(
            h_flex()
                .gap_2()
                .child(master_col)
                .child(current_col),
        );

        // --- 相互コピーボタン ---
        let copy_row = h_flex()
            .gap_1()
            .child(
                crate::shell::chrome::action_button("src-copy-to-current")
                    .small()

                    .label(text("docx.source-manager.copy-to-current"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.source_manager_copy_master_to_current(cx);
                    })),
            )
            .child(
                crate::shell::chrome::action_button("src-copy-to-master")
                    .small()

                    .label(text("docx.source-manager.copy-to-master"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.source_manager_copy_current_to_master(cx);
                    })),
            );
        body = body.child(copy_row);

        // --- プレビュー ---
        let selected_entry = match active_list {
            ActiveList::Master => selected_master.and_then(|i| master_list.get(i)),
            ActiveList::Current => selected_current.and_then(|i| current_list.get(i)),
        };

        if let Some(entry) = selected_entry {
            let preview_box = v_flex()
                .gap_1()
                .p_2()
                .rounded_md()
                .bg(cx.theme().muted.opacity(0.2))
                .border_1()
                .border_color(cx.theme().border)
                .child(div().font_weight(FontWeight::BOLD).text_sm().child(text("docx.source-manager.preview-title").to_string()))
                .child(
                    h_flex()
                        .gap_1()
                        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.source-manager.citation-preview").to_string()))
                        .child(div().text_sm().child(entry.citation_preview())),
                )
                .child(
                    v_flex()
                        .gap_1()
                        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.source-manager.bib-preview").to_string()))
                        .child(div().text_sm().child(entry.bibliography_preview())),
                );
            body = body.child(preview_box);
        }

        // --- 操作ボタングループ ---
        let active_list_copy = active_list;
        let actions = crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("src-new", text("docx.source-manager.new"), crate::shell::chrome::ActionButtonKind::Primary)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.source_manager_start_new(active_list_copy, window, cx);
                    })),
            )
            .action(
                crate::shell::chrome::ActionDefinition::new("src-edit", text("docx.source-manager.edit"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.source_manager_start_edit(window, cx);
                    })),
            )
            .action(
                crate::shell::chrome::ActionDefinition::new("src-insert-citation", text("docx.source-manager.insert-citation"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.source_manager_insert_selected_citation(cx);
                    })),
            )
            .action(
                crate::shell::chrome::ActionDefinition::new("src-delete", text("docx.source-manager.delete"), crate::shell::chrome::ActionButtonKind::Destructive)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.source_manager_delete_selected(cx);
                    })),
            )));

        body = body.child(actions);

        // クローズボタン

        Some(SidebarPanel {title:text("docx.source-manager.title"),body:body.into_any_element()})
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_sources, encode_sources, find_source_in_doc, load_document_sources,
        save_document_sources_command, SourceEntry, SourceType,
    };
    use ibis_doc_model::{FlowDocument, FlowParagraph, FlowRun, FlowRunKind, FlowRunStyle};

    #[test]
    fn test_source_entry_previews() {
        let entry = SourceEntry {
            tag: "Smith2024".to_string(),
            source_type: SourceType::Book,
            author: "Smith, John".to_string(),
            title: "Rust Programming".to_string(),
            year: "2024".to_string(),
            publisher: "Tech Press".to_string(),
            city: "Tokyo".to_string(),
            url: String::new(),
        };

        assert_eq!(entry.citation_preview(), "(Smith, John, 2024)");
        assert_eq!(
            entry.bibliography_preview(),
            "Smith, John (2024). Rust Programming. Tokyo: Tech Press."
        );
    }

    #[test]
    fn test_source_entry_website_preview() {
        let entry = SourceEntry {
            tag: "RustDoc".to_string(),
            source_type: SourceType::Website,
            author: "Rust Team".to_string(),
            title: "The Rust Reference".to_string(),
            year: "2024".to_string(),
            publisher: String::new(),
            city: String::new(),
            url: "https://doc.rust-lang.org/reference/".to_string(),
        };

        assert_eq!(entry.citation_preview(), "(Rust Team, 2024)");
        assert_eq!(
            entry.bibliography_preview(),
            "Rust Team (2024). The Rust Reference. https://doc.rust-lang.org/reference/"
        );
    }

    #[test]
    fn test_encode_and_decode_sources() {
        let entries = vec![
            SourceEntry {
                tag: "Tag1".to_string(),
                source_type: SourceType::Book,
                author: "Author 1".to_string(),
                title: "Title 1".to_string(),
                year: "2024".to_string(),
                publisher: "Pub 1".to_string(),
                city: "City 1".to_string(),
                url: String::new(),
            },
            SourceEntry {
                tag: "Tag2".to_string(),
                source_type: SourceType::Website,
                author: "Author 2".to_string(),
                title: "Title 2".to_string(),
                year: "2023".to_string(),
                publisher: String::new(),
                city: String::new(),
                url: "https://example.com".to_string(),
            },
        ];

        let encoded = encode_sources(&entries);
        let decoded = decode_sources(&encoded);
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].tag, "Tag1");
        assert_eq!(decoded[0].author, "Author 1");
        assert_eq!(decoded[1].tag, "Tag2");
        assert_eq!(decoded[1].url, "https://example.com");
    }

    #[test]
    fn test_load_and_save_document_sources() {
        let mut doc = FlowDocument::default();

        // 1. Initially empty
        let sources = load_document_sources(&doc);
        assert!(sources.is_empty());

        // 2. Add a citation field in document
        let mut p = FlowParagraph { id: 1, ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(
            FlowRunKind::Field {
                instruction: "CITATION Yamada2024 \\l 1041".to_string(),
                shown: "(Yamada, 2024)".to_string(),
            },
            FlowRunStyle::default(),
        ));
        doc.body.push(ibis_doc_model::Block::Paragraph(p));

        // 3. Now load_document_sources discovers Yamada2024
        let sources2 = load_document_sources(&doc);
        assert_eq!(sources2.len(), 1);
        assert_eq!(sources2[0].tag, "Yamada2024");

        // 4. Save metadata via save_document_sources_command
        let detailed_entry = SourceEntry {
            tag: "Yamada2024".to_string(),
            source_type: SourceType::Book,
            author: "山田 太郎".to_string(),
            title: "オフィスソフトの設計".to_string(),
            year: "2024".to_string(),
            publisher: "アイビス出版".to_string(),
            city: "東京".to_string(),
            url: String::new(),
        };
        let cmd = save_document_sources_command(&doc, &[detailed_entry.clone()]).unwrap();
        if let ibis_doc_model::FlowCommand::SetDocVariables(vars) = cmd {
            doc.doc_variables = vars;
        }

        // 5. Verify find_source_in_doc finds full metadata
        let found = find_source_in_doc(&doc, "Yamada2024").expect("source found");
        assert_eq!(found.author, "山田 太郎");
        assert_eq!(found.title, "オフィスソフトの設計");
    }
}
