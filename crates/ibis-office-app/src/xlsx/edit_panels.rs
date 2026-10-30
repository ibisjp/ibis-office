// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The panels of the editing basics (重複の削除, 区切り位置, …), kept
//! together in the workbook's state so that adding one does not change
//! the state's other fields.

#[derive(Default)]
pub(crate) struct EditPanels {
    /// データ > 重複の削除.
    pub(crate) dedupe: Option<super::dedupe_edit::DedupePanel>,
    /// データ > 区切り位置.
    pub(crate) text_columns: Option<super::text_columns::TextColumnsPanel>,
    /// データ > ブックのリンク…
    pub(crate) external_links: Option<super::external_links::ExternalLinksPanel>,
    /// データ > テキスト/CSV から…
    pub(crate) import_text: Option<super::import_text::ImportTextPanel>,
    /// データ > テーブルまたは範囲から: the クエリ エディター, which takes the place of the sheet.
    pub(crate) query_editor: Option<super::query_editor::QueryEditor>,
    /// データ > クエリと接続.
    pub(crate) queries: Option<super::query_editor::QueriesPanel>,
    /// 挿入… / 削除… of the cell menu.
    pub(crate) cells_dialog: Option<super::cells_dialog::CellsDialog>,
    /// 形式を選択して貼り付け… dialog panel.
    pub(crate) paste_special_dialog: Option<super::paste_special_dialog::PasteSpecialDialog>,
    /// 形式を選択して貼り付け > 空白セルを無視する: copied cells with
    /// nothing in them leave the cells pasted on as they are.
    pub(crate) skip_blanks: bool,
    /// メモ (a cell's note).
    pub(crate) note: Option<super::notes::NotePanel>,
    /// シートの保護 / シート保護の解除.
    pub(crate) protect: Option<super::protection::ProtectPanel>,
    /// ブックの保護 / ブック保護の解除.
    pub(crate) protect_workbook: Option<super::protection::ProtectWorkbookPanel>,
    /// 範囲の編集の許可.
    pub(crate) allow_edit_ranges: Option<super::protection::AllowEditRangesPanel>,
    /// 連続データの作成.
    pub(crate) series: Option<super::series::SeriesPanel>,
    /// データ > What-If 分析 > データテーブル.
    pub(crate) data_table: Option<super::data_table::DataTablePanel>,
    /// データ > What-If 分析 > ゴールシーク.
    pub(crate) goal_seek: Option<super::goal_seek::GoalSeekPanel>,
    /// 関数の挿入・関数の引数 (fx).
    pub(crate) function: Option<super::function_dialog::FunctionPanel>,
    /// ツール > 文字カウント.
    pub(crate) word_count: Option<super::word_count::WordCountPanel>,
    /// ツール > 辞書 / 校閲 > 類義語辞典.
    pub(crate) thesaurus: Option<super::thesaurus_panel::SheetThesaurusPanel>,
    /// 編集 > 条件を選択してジャンプ.
    pub(crate) goto_special: Option<super::goto_special::GotoSpecialPanel>,
    /// 挿入 > ピボットテーブル, and a pivot table's fields.
    pub(crate) pivot: Option<super::pivot::PivotPanel>,
    /// 表示形式 > テーブルのデザイン.
    pub(crate) table_design: Option<super::table_design::TableDesignPanel>,
    /// データ > 詳細設定…
    pub(crate) advanced_filter: Option<super::advanced_filter::AdvancedFilterPanel>,
    /// ツール > オートコレクトのオプション…
    pub(crate) autocorrect: Option<super::autocorrect::AutoCorrectPanel>,
    /// ツール > エラー チェックのオプション…
    pub(crate) error_options: Option<super::error_check::ErrorOptionsPanel>,
    /// 書式 > スライサーの設定…
    pub(crate) slicer_settings: Option<super::slicer_settings::SlicerSettingsPanel>,
    /// 挿入 > スライサー…
    pub(crate) slicer_insert: Option<super::slicer_insert::SlicerInsertPanel>,
    /// 挿入 > タイムライン…
    pub(crate) timeline_insert: Option<super::timeline_insert::TimelineInsertPanel>,
    /// 開発 > コントロールの書式設定/プロパティ…
    pub(crate) control_properties: Option<super::control_insert::ControlPropertiesPanel>,
    /// 挿入 > 描画（インク）
    pub(crate) ink: Option<super::ink::InkPanel>,
    /// データ > ソルバー…
    pub(crate) solver: Option<super::solver_panel::SolverPanel>,
    /// データ > データ分析…
    pub(crate) analysis: Option<super::analysis_panel::AnalysisPanel>,
    /// データ > 予測シート… (XLSX-ANALYSIS-020).
    pub(crate) forecast: Option<super::forecast_sheet::ForecastPanel>,
    /// データ > 統合…
    pub(crate) consolidate: Option<super::consolidate::ConsolidatePanel>,
    /// 表示形式 > セルのスタイル > 新しいセル スタイル…
    pub(crate) new_cell_style: Option<super::cell_styles::NewCellStylePanel>,
    /// テーブル デザイン > テーブル スタイル > 新しいテーブル スタイル…
    pub(crate) new_table_style: Option<super::table_design::NewTableStylePanel>,
    /// データ > What-If 分析 > シナリオの登録と管理…
    pub(crate) scenario_manager: Option<super::scenario_manager::ScenarioManagerPanel>,
    /// 挿入 > スパークライン…
    pub(crate) sparkline: Option<super::sparkline_dialog::SparklinePanel>,
    /// データ > 小計…
    pub(crate) subtotal: Option<super::subtotal_dialog::SubtotalPanel>,
    /// データ > グループ化…
    pub(crate) pivot_group: Option<super::pivot_group::PivotGroupPanel>,
    /// データ > ピボットテーブル オプション…
    pub(crate) pivot_options: Option<super::pivot_options::PivotOptionsPanel>,
    /// データ > 計算フィールド…
    pub(crate) pivot_calc_field: Option<super::pivot_calc_field::PivotCalcFieldPanel>,
    /// データ > 集計アイテム…
    pub(crate) pivot_calc_item: Option<super::pivot_calc_item::PivotCalcItemPanel>,
    /// 数式オートコンプリート's list.
    pub(crate) completion: super::autocomplete::Completion,
    /// A pivot button's (▼) panel, while open.
    pub(crate) pivot_popup: Option<super::pivot::PivotPopup>,
    /// The ranges it selected with the selection.
    pub(crate) extra_ranges: Option<super::goto_special::ExtraRanges>,
    /// 表示 > 分割: the split panes of the sheets split.
    pub(crate) splits: std::collections::HashMap<ibis_doc_model::SlideId, super::split_view::SplitPanes>,
    /// The built-in cell styles with the workbook's theme (made when the
    /// menu is first shown).
    pub(crate) cell_styles: std::cell::RefCell<Option<std::rc::Rc<Vec<ibis_pptx::PresetCellStyle>>>>,
    /// データ > 数式の検証…
    pub(crate) evaluate_formula: Option<super::formula_audit::EvaluateFormulaPanel>,
    /// データ > ウォッチ ウィンドウ…
    pub(crate) watch_window: Option<super::formula_audit::WatchWindowPanel>,
    /// 登録されたウォッチ式一覧（パネルを閉じても保持）
    pub(crate) watches: Vec<super::formula_audit::WatchEntry>,
    /// ファイル > 情報（ブックのプロパティ）.
    pub(crate) properties: Option<super::properties_panel::DocPropertiesPanel>,
    /// 書式 > 代替テキスト… (図形・グラフの代替テキスト).
    pub(crate) alt_text: Option<super::alt_text_panel::AltTextPanel>,
    /// 挿入 > コメント (Threaded Comments).
    pub(crate) threaded_comment: Option<super::threaded_comments::ThreadedCommentPanel>,
    /// ツール > アクセシビリティ チェック…
    pub(crate) accessibility: Option<super::accessibility_panel::AccessibilityPanel>,
    /// ツール > 互換性チェック…
    pub(crate) compatibility: Option<super::compatibility_panel::CompatibilityPanel>,
    /// 表示 > 選択ウィンドウ… (ワークシート上の図形・画像・グラフ一覧).
    pub(crate) selection_pane: Option<super::selection_pane::SelectionPanePanel>,
    /// 書式 > サイズと位置… (図形のサイズ・位置・回転・縦横比固定).
    pub(crate) frame: Option<super::frame_panel::SheetFramePanel>,
    /// 開発 > XML > ソース…
    pub(crate) xml_source: Option<super::xml_maps::XmlSourcePanel>,
    /// 開発 > XML > XML マップのプロパティ…
    pub(crate) xml_map_properties: Option<super::xml_maps::XmlMapPropertiesPanel>,
    /// 表示形式 > テーマの色とフォント…
    pub(crate) theme: Option<super::theme_panel::XlsxThemePanel>,
    /// 表示 > ユーザー設定のビュー…
    pub(crate) custom_views: Option<super::custom_views::CustomViewsPanel>,
    /// 表示 > ウィンドウの再表示…
    pub(crate) unhide_window: Option<super::window_hide::UnhideWindowPanel>,
    /// グラフの移動… (XLSX-CHEDIT-020).
    pub(crate) move_chart: Option<super::move_chart::MoveChartPanel>,
    /// グラフ テンプレート… (XLSX-CHEDIT-021).
    pub(crate) chart_template: Option<super::chart_template::ChartTemplatePanel>,
    /// 表示 > ウィンドウの整列… (XLSX-SHEET-017).
    pub(crate) arrange_windows: Option<super::arrange_windows::ArrangeWindowsPanel>,
    /// ツール > バックアップと保存先の設定… (XLSX-FILE-019).
    pub(crate) backup_save: Option<super::backup_save::BackupSaveSettingsPanel>,
    /// ツール/ファイル > 破損ブックの開いて修復… (XLSX-FILE-020).
    pub(crate) repair: Option<super::repair::RepairWorkbookPanel>,
    /// ふりがなの編集 (XLSX-CELL-006).
    pub(crate) phonetic: Option<super::phonetic::PhoneticPanel>,
    /// 行の高さ・列の幅 (XLSX-FORMAT-001/002).
    pub(crate) row_col_size: Option<super::row_col_size::RowColSizePanel>,
    /// 挿入 > グラフ > おすすめグラフ (#128).
    pub(crate) recommended_charts: Option<super::chart_insert::RecommendedChartsPanel>,
}

impl EditPanels {
    /// Close them all (another panel opening).
    pub(crate) fn close(&mut self) {
        self.dedupe = None;
        self.text_columns = None;
        self.import_text = None;
        self.query_editor = None;
        self.queries = None;
        self.cells_dialog = None;
        self.paste_special_dialog = None;
        self.external_links = None;
        self.goto_special = None;
        self.note = None;
        self.protect = None;
        self.protect_workbook = None;
        self.allow_edit_ranges = None;
        self.series = None;
        self.goal_seek = None;
        self.function = None;
        self.word_count = None;
        self.thesaurus = None;
        self.data_table = None;
        self.pivot = None;
        self.table_design = None;
        self.advanced_filter = None;
        self.consolidate = None;
        self.analysis = None;
        self.solver = None;
        self.ink = None;
        self.slicer_insert = None;
        self.timeline_insert = None;
        self.control_properties = None;
        self.slicer_settings = None;
        self.error_options = None;
        self.autocorrect = None;
        self.forecast = None;
        self.new_cell_style = None;
        self.new_table_style = None;
        self.scenario_manager = None;
        self.sparkline = None;
        self.subtotal = None;
        self.pivot_group = None;
        self.pivot_options = None;
        self.pivot_calc_field = None;
        self.pivot_calc_item = None;
        self.evaluate_formula = None;
        self.watch_window = None;
        self.properties = None;
        self.alt_text = None;
        self.threaded_comment = None;
        self.accessibility = None;
        self.compatibility = None;
        self.selection_pane = None;
        self.frame = None;
        self.theme = None;
        self.custom_views = None;
        self.unhide_window = None;
        self.move_chart = None;
        self.chart_template = None;
        self.arrange_windows = None;
        self.backup_save = None;
        self.repair = None;
        self.phonetic = None;
        self.row_col_size = None;
        self.recommended_charts = None;
        self.xml_source = None;
        self.xml_map_properties = None;
    }
}

impl crate::shell::editor::SlideEditor {
    /// The editing basics' panel open, for the sidebar.
    pub(crate) fn edit_panel(&mut self, cx: &mut gpui_kit::Context<Self>) -> Option<crate::shell::chrome::SidebarPanel> {
        self.dedupe_panel(cx).map(|panel| panel.modal_named("dedupe_panel"))
            .or_else(|| self.recommended_charts_panel(cx).map(|panel| panel.modal_named("recommended_charts_panel")))
            .or_else(|| self.text_columns_panel(cx).map(|panel| panel.modal_named("text_columns_panel")))
            .or_else(|| self.import_text_panel(cx).map(|panel| panel.modal_named("import_text_panel")))
            .or_else(|| self.queries_panel(cx).map(|panel| panel.modal_named("queries_panel")))
            .or_else(|| self.cells_dialog_panel(cx).map(|panel| panel.modal_named("cells_dialog_panel")))
            .or_else(|| self.external_links_panel(cx).map(|panel| panel.modal_named("external_links_panel")))
            .or_else(|| self.goto_special_panel(cx).map(|panel| panel.modal_named("goto_special_panel")))
            .or_else(|| self.note_panel(cx))
            .or_else(|| self.threaded_comment_panel(cx))
            .or_else(|| self.protect_panel(cx).map(|panel| panel.modal_named("protect_panel")))
            .or_else(|| self.protect_workbook_panel(cx).map(|panel| panel.modal_named("protect_workbook_panel")))
            .or_else(|| self.allow_edit_ranges_panel(cx).map(|panel| panel.modal_named("allow_edit_ranges_panel")))
            .or_else(|| self.series_panel(cx).map(|panel| panel.modal_named("series_panel")))
            .or_else(|| self.goal_seek_panel(cx).map(|panel| panel.modal_named("goal_seek_panel")))
            .or_else(|| self.function_panel(cx).map(|panel| panel.modeless_named("function_panel")))
            .or_else(|| self.sheet_word_count_panel(cx).map(|panel| panel.modal_named("sheet_word_count_panel")))
            .or_else(|| self.sheet_thesaurus_panel(cx))
            .or_else(|| self.data_table_panel(cx).map(|panel| panel.modal_named("data_table_panel")))
            .or_else(|| self.scenario_manager_panel(cx).map(|panel| panel.modal_named("scenario_manager_panel")))
            .or_else(|| self.sparkline_panel(cx).map(|panel| panel.modal_named("sparkline_panel")))
            .or_else(|| self.subtotal_panel(cx).map(|panel| panel.modal_named("subtotal_panel")))
            .or_else(|| self.pivot_group_panel(cx).map(|panel| panel.modal_named("pivot_group_panel")))
            .or_else(|| self.pivot_options_panel(cx).map(|panel| panel.modal_named("pivot_options_panel")))
            .or_else(|| self.pivot_calc_field_panel(cx).map(|panel| panel.modal_named("pivot_calc_field_panel")))
            .or_else(|| self.pivot_calc_item_panel(cx).map(|panel| panel.modal_named("pivot_calc_item_panel")))
            .or_else(|| self.pivot_panel(cx).map(|panel| panel.modal_named("pivot_panel")))
            .or_else(|| self.table_design_panel(cx))
            .or_else(|| self.advanced_filter_panel(cx).map(|panel| panel.modal_named("advanced_filter_panel")))
            .or_else(|| self.consolidate_panel(cx).map(|panel| panel.modal_named("consolidate_panel")))
            .or_else(|| self.analysis_panel(cx).map(|panel| panel.modal_named("analysis_panel")))
            .or_else(|| self.solver_panel(cx).map(|panel| panel.modal_named(if self.wb().panels.solver.as_ref().is_some_and(|p|p.editor.is_some()){"solver_constraint_editor"}else{"solver_panel"})))
            .or_else(|| self.ink_panel(cx))
            .or_else(|| self.slicer_insert_panel(cx).map(|panel| panel.modal_named("slicer_insert_panel")))
            .or_else(|| self.timeline_insert_panel(cx).map(|panel| panel.modal_named("timeline_insert_panel")))
            .or_else(|| self.control_properties_panel(cx).map(|panel| panel.modal_named("control_properties_panel")))
            .or_else(|| self.slicer_settings_panel(cx).map(|panel| panel.modal_named("slicer_settings_panel")))
            .or_else(|| self.error_options_panel(cx).map(|panel| panel.modal_named("error_options_panel")))
            .or_else(|| self.autocorrect_panel(cx).map(|panel| panel.modal_named("autocorrect_panel")))
            .or_else(|| self.forecast_panel(cx).map(|panel| panel.modal_named("forecast_panel")))
            .or_else(|| self.new_cell_style_panel(cx).map(|panel| panel.modal_named("new_cell_style_panel")))
            .or_else(|| self.new_table_style_panel(cx).map(|panel| panel.modal_named("new_table_style_panel")))
            .or_else(|| self.xml_source_panel(cx))
            .or_else(|| self.xml_map_properties_panel(cx).map(|panel| panel.modal_named("xml_map_properties_panel")))
            .or_else(|| self.evaluate_formula_panel(cx).map(|panel| panel.modeless_named("evaluate_formula_panel")))
            .or_else(|| self.watch_window_panel(cx))
            .or_else(|| self.sheet_doc_properties_panel(cx).map(|panel| panel.modal_named("sheet_doc_properties_panel")))
            .or_else(|| self.sheet_alt_text_panel(cx).map(|panel|panel.modal_named("sheet_alt_text_panel")))
            .or_else(|| self.xlsx_accessibility_panel(cx))
            .or_else(|| self.xlsx_compatibility_panel(cx).map(|panel| panel.modal_named("xlsx_compatibility_panel")))
            .or_else(|| self.xlsx_selection_pane_panel(cx))
            .or_else(|| self.sheet_frame_panel(cx))
            .or_else(|| self.xlsx_theme_panel(cx).map(|panel| panel.modal_named("xlsx_theme_panel")))
            .or_else(|| self.custom_views_panel(cx).map(|panel| panel.modal_named("custom_views_panel")))
            .or_else(|| self.unhide_window_panel(cx).map(|panel| panel.modal_named("unhide_window_panel")))
            .or_else(|| self.move_chart_panel(cx).map(|panel| panel.modal_named("move_chart_panel")))
            .or_else(|| self.chart_template_panel(cx).map(|panel| panel.modal_named("chart_template_panel")))
            .or_else(|| self.arrange_windows_panel(cx).map(|panel| panel.modal_named("arrange_windows_panel")))
            .or_else(|| self.xlsx_backup_save_settings_panel(cx).map(|panel| panel.modal_named("xlsx_backup_save_settings_panel")))
            .or_else(|| self.autorecover_settings_panel(cx).map(|panel| panel.modal_named("autorecover_settings_panel")))
            .or_else(|| self.recover_unsaved_panel(cx).map(|panel| panel.modal_named("recover_unsaved_panel")))
            .or_else(|| self.xlsx_repair_panel(cx).map(|panel| panel.modal_named("xlsx_repair_panel")))
            .or_else(|| self.phonetic_panel(cx).map(|panel| panel.modal_named("phonetic_panel")))
            .or_else(|| self.row_col_size_panel(cx).map(|panel| panel.modal_named("row_col_size_panel")))
            .or_else(|| self.paste_special_panel(cx).map(|panel| panel.modal_named("paste_special_panel")))
    }
}
