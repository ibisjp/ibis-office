// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! XLSX-FILE-019: バックアップと保存先設定 (Backup & Save Location Settings).
//!
//! - 保存時に常にバックアップ ファイルを作成する (Always create backup copy):
//!   Excelの「全般オプション > バックアップ ファイルを作成する」に準拠。
//!   上書き保存時に、直前の状態を `.xlk` または `.xlsx` バックアップとして保存。
//!   元のファイルと同じフォルダーまたは専用フォルダーに保存。
//! - 既定の保存場所 (Default local save location):
//!   新規ブックの保存ダイアログの初期フォルダーをカスタマイズ。
//! - バックアップと保存先設定パネル (BackupSaveSettingsPanel)。

use std::path::{Path, PathBuf};

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// バックアップ ファイルの形式.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BackupFormat {
    /// Excel 互換形式 (.xlk)
    #[default]
    Xlk,
    /// 通常の Excel ワークブック形式 (.xlsx)
    Xlsx,
}

impl BackupFormat {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            BackupFormat::Xlk => "xlk",
            BackupFormat::Xlsx => "xlsx",
        }
    }

    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "xlsx" => BackupFormat::Xlsx,
            _ => BackupFormat::Xlk,
        }
    }
}

/// バックアップ ファイルの保存場所.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum BackupLocation {
    /// 保存対象ファイルと同一フォルダー
    #[default]
    SameFolder,
    /// 指定されたカスタム フォルダー
    Custom(PathBuf),
}

/// バックアップおよび保存先の設定.
#[derive(Debug, Clone)]
pub(crate) struct BackupSaveConfig {
    /// 上書き保存時に常にバックアップを作成するか
    pub(crate) always_create_backup: bool,
    /// 新規ブックの既定の保存場所（設定されている場合は優先使用）
    pub(crate) default_save_location: Option<PathBuf>,
    /// バックアップの保存形式 (.xlk / .xlsx)
    pub(crate) backup_format: BackupFormat,
    /// バックアップの保存場所（同一フォルダー / カスタム）
    pub(crate) backup_location: BackupLocation,
}

impl Default for BackupSaveConfig {
    fn default() -> Self {
        Self {
            always_create_backup: false,
            default_save_location: None,
            backup_format: BackupFormat::Xlk,
            backup_location: BackupLocation::SameFolder,
        }
    }
}

// ----------------------------------------------------------------------------
// 設定ファイルの永続化 (Persistence via TSV)
// ----------------------------------------------------------------------------

fn config_store_path() -> Option<PathBuf> {
    let base = crate::shell::app_data::base();
    Some(base?.join("ibisOffice").join("backup-save-config.tsv"))
}

pub(crate) fn load_backup_save_config() -> BackupSaveConfig {
    let Some(path) = config_store_path() else { return BackupSaveConfig::default() };
    let Ok(content) = std::fs::read_to_string(path) else { return BackupSaveConfig::default() };
    let mut config = BackupSaveConfig::default();
    for line in content.lines() {
        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() == 2 {
            match parts[0] {
                "always_create_backup" => {
                    config.always_create_backup = parts[1] == "1" || parts[1].eq_ignore_ascii_case("true");
                }
                "default_save_location" => {
                    config.default_save_location = if parts[1].trim().is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(parts[1].trim()))
                    };
                }
                "backup_format" => {
                    config.backup_format = BackupFormat::from_str(parts[1].trim());
                }
                "backup_location" => {
                    let loc_str = parts[1].trim();
                    config.backup_location = if loc_str.is_empty() || loc_str == "same" {
                        BackupLocation::SameFolder
                    } else {
                        BackupLocation::Custom(PathBuf::from(loc_str))
                    };
                }
                _ => {}
            }
        }
    }
    config
}

pub(crate) fn save_backup_save_config(config: &BackupSaveConfig) -> std::io::Result<()> {
    let Some(path) = config_store_path() else { return Ok(()) };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let def_loc_str = config.default_save_location.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let loc_str = match &config.backup_location {
        BackupLocation::SameFolder => "same".to_string(),
        BackupLocation::Custom(p) => p.to_string_lossy().into_owned(),
    };
    let text = format!(
        "always_create_backup\t{}\ndefault_save_location\t{}\nbackup_format\t{}\nbackup_location\t{}\n",
        if config.always_create_backup { "1" } else { "0" },
        def_loc_str,
        config.backup_format.as_str(),
        loc_str
    );
    std::fs::write(path, text)
}

/// OS の既定の保存場所（Documents または HOME）.
pub(crate) fn system_default_save_location() -> PathBuf {
    #[cfg(not(target_family = "wasm"))]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            let docs = home.join("Documents");
            if docs.is_dir() {
                return docs;
            }
            return home;
        }
    }
    PathBuf::from(".")
}

/// 有効な既定の保存場所（カスタム設定があればそれ、なければ OS 既定）.
pub(crate) fn effective_default_save_location() -> PathBuf {
    let config = load_backup_save_config();
    if let Some(custom) = config.default_save_location {
        if custom.exists() || std::fs::create_dir_all(&custom).is_ok() {
            return custom;
        }
    }
    system_default_save_location()
}

// ----------------------------------------------------------------------------
// バックアップ ファイル生成ロジック
// ----------------------------------------------------------------------------

/// バックアップ ファイル名を生成.
/// Excel 互換形式: `<ファイル名> のバックアップ.xlk` または `<ファイル名>.xlk`
pub(crate) fn generate_backup_file_name(target_path: &Path, format: BackupFormat) -> String {
    let stem = target_path.file_stem().and_then(|s| s.to_str()).unwrap_or("Workbook");
    let ext = match format {
        BackupFormat::Xlk => "xlk",
        BackupFormat::Xlsx => "xlsx",
    };
    crate::shell::i18n::format("_xlsx.backup-of-stem-ext", &[("stem", &stem), ("ext", &ext)])
}

/// 上書き保存前にバックアップ コピーを作成（条件を満たす場合）.
pub(crate) fn create_backup_copy_if_enabled(
    target_path: &Path,
    workbook_override: Option<bool>,
) -> Result<Option<PathBuf>, String> {
    #[cfg(target_family = "wasm")]
    {
        let _ = (target_path, workbook_override);
        Ok(None)
    }
    #[cfg(not(target_family = "wasm"))]
    {
        if !target_path.exists() {
            return Ok(None);
        }
        let config = load_backup_save_config();
        let should_backup = workbook_override.unwrap_or(config.always_create_backup);
        if !should_backup {
            return Ok(None);
        }

        let dest_dir = match &config.backup_location {
            BackupLocation::SameFolder => target_path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")),
            BackupLocation::Custom(dir) => {
                let _ = std::fs::create_dir_all(dir);
                dir.clone()
            }
        };

        let backup_name = generate_backup_file_name(target_path, config.backup_format);
        let backup_path = dest_dir.join(backup_name);

        std::fs::copy(target_path, &backup_path).map_err(|e| crate::shell::i18n::format("_xlsx.could-not-create-the-backup-e", &[("e", &e)]))?;
        Ok(Some(backup_path))
    }
}

// ----------------------------------------------------------------------------
// バックアップと保存先設定パネル (UI Panel)
// ----------------------------------------------------------------------------

pub(crate) struct BackupSaveSettingsPanel {
    pub(crate) always_create_backup: bool,
    pub(crate) backup_format: BackupFormat,
    pub(crate) use_same_folder: bool,
    pub(crate) custom_backup_folder_input: Entity<InputState>,
    pub(crate) default_save_folder_input: Entity<InputState>,
    pub(crate) workbook_override: Option<bool>,
    pub(crate) message: Option<String>,
}

impl SlideEditor {
    /// ツール > バックアップと保存先の設定… を開く.
    pub(crate) fn open_xlsx_backup_save_settings_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sheet_state().is_none() {
            return;
        }
        let config = load_backup_save_config();
        let default_loc_str = config
            .default_save_location
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| effective_default_save_location().display().to_string());
        let (use_same_folder, custom_folder_str) = match &config.backup_location {
            BackupLocation::SameFolder => (true, String::new()),
            BackupLocation::Custom(p) => (false, p.display().to_string()),
        };
        let custom_input = cx.new(|cx| InputState::new(window, cx).default_value(custom_folder_str));
        let default_loc_input = cx.new(|cx| InputState::new(window, cx).default_value(default_loc_str));
        let workbook_override = self.sheet_state().and_then(|s| s.always_create_backup);

        let panel = BackupSaveSettingsPanel {
            always_create_backup: config.always_create_backup,
            backup_format: config.backup_format,
            use_same_folder,
            custom_backup_folder_input: custom_input,
            default_save_folder_input: default_loc_input,
            workbook_override,
            message: None,
        };

        self.wb_mut().close_settings();
        self.wb_mut().panels.backup_save = Some(panel);
        cx.notify();
    }

    /// パネルを閉じる.
    pub(crate) fn close_xlsx_backup_save_settings_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(wb) = self.sheet_state_mut() {
            wb.panels.backup_save = None;
            cx.notify();
        }
    }

    /// 既定の保存場所を OS 既定にリセット.
    pub(crate) fn reset_xlsx_default_save_location_to_default(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let def_str = system_default_save_location().display().to_string();
        if let Some(panel) = self.wb_mut().panels.backup_save.as_mut() {
            panel.default_save_folder_input.update(cx, |input, cx| {
                input.set_value(def_str, window, cx);
            });
            cx.notify();
        }
    }

    /// 設定を保存.
    pub(crate) fn save_xlsx_backup_save_settings(&mut self, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let Some(panel) = self.wb().panels.backup_save.as_ref() else { return };
            let raw_default_loc = panel.default_save_folder_input.read(cx).value().trim().to_string();
            let default_save_location = if raw_default_loc.is_empty() {
                None
            } else {
                let p = PathBuf::from(raw_default_loc);
                if let Err(e) = std::fs::create_dir_all(&p) {
                    if let Some(pnl) = self.wb_mut().panels.backup_save.as_mut() {
                        pnl.message = Some(format!("{}: {e}", text("xlsx.backupsave.error-default-path")));
                        cx.notify();
                    }
                    return;
                }
                Some(p)
            };

            let backup_location = if panel.use_same_folder {
                BackupLocation::SameFolder
            } else {
                let raw_custom = panel.custom_backup_folder_input.read(cx).value().trim().to_string();
                if raw_custom.is_empty() {
                    BackupLocation::SameFolder
                } else {
                    let p = PathBuf::from(raw_custom);
                    if let Err(e) = std::fs::create_dir_all(&p) {
                        if let Some(pnl) = self.wb_mut().panels.backup_save.as_mut() {
                            pnl.message = Some(format!("{}: {e}", text("xlsx.backupsave.error-backup-path")));
                            cx.notify();
                        }
                        return;
                    }
                    BackupLocation::Custom(p)
                }
            };

            let config = BackupSaveConfig {
                always_create_backup: panel.always_create_backup,
                default_save_location,
                backup_format: panel.backup_format,
                backup_location,
            };

            let wb_override = panel.workbook_override;

            if let Err(e) = save_backup_save_config(&config) {
                if let Some(pnl) = self.wb_mut().panels.backup_save.as_mut() {
                    pnl.message = Some(format!("{}: {e}", text("xlsx.backupsave.save-failed")));
                    cx.notify();
                }
                return;
            }

            if let Some(wb) = self.sheet_state_mut() {
                wb.always_create_backup = wb_override;
                if let Some(pnl) = wb.panels.backup_save.as_mut() {
                    pnl.message = Some(text("xlsx.backupsave.saved-msg").to_string());
                }
            }
            cx.notify();
        }
        #[cfg(target_family = "wasm")]
        {
            if let Some(pnl) = self.wb_mut().panels.backup_save.as_mut() {
                pnl.message = Some(text("xlsx.backupsave.wasm-unsupported").to_string());
                cx.notify();
            }
        }
    }

    /// 今すぐバックアップを作成.
    pub(crate) fn manual_create_backup_now(&mut self, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let path_opt = self.path.clone();
            match path_opt {
                Some(path) => {
                    let mut config = load_backup_save_config();
                    if let Some(p) = self.wb().panels.backup_save.as_ref() {
                        config.backup_format = p.backup_format;
                    }
                    let dest_dir = match &config.backup_location {
                        BackupLocation::SameFolder => path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".")),
                        BackupLocation::Custom(dir) => {
                            let _ = std::fs::create_dir_all(dir);
                            dir.clone()
                        }
                    };
                    let backup_name = generate_backup_file_name(&path, config.backup_format);
                    let backup_path = dest_dir.join(backup_name);

                    match std::fs::copy(&path, &backup_path) {
                        Ok(_) => {
                            let fname = backup_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                            if let Some(panel) = self.wb_mut().panels.backup_save.as_mut() {
                                panel.message = Some(format!("{} ({fname})", text("xlsx.backupsave.backup-now-done")));
                            }
                        }
                        Err(err) => {
                            if let Some(panel) = self.wb_mut().panels.backup_save.as_mut() {
                                panel.message = Some(format!("{}: {err}", text("xlsx.backupsave.backup-now-failed")));
                            }
                        }
                    }
                }
                None => {
                    if let Some(panel) = self.wb_mut().panels.backup_save.as_mut() {
                        panel.message = Some(text("xlsx.backupsave.need-save-first").to_string());
                    }
                }
            }
            cx.notify();
        }
        #[cfg(target_family = "wasm")]
        {
            if let Some(panel) = self.wb_mut().panels.backup_save.as_mut() {
                panel.message = Some(text("xlsx.backupsave.wasm-unsupported").to_string());
                cx.notify();
            }
        }
    }

    /// サイドバーパネル描画.
    pub(crate) fn xlsx_backup_save_settings_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.wb().panels.backup_save.as_ref()?;
        let theme = cx.theme();
        let always_create = panel.always_create_backup;
        let is_xlk = panel.backup_format == BackupFormat::Xlk;
        let use_same_folder = panel.use_same_folder;
        let custom_input = panel.custom_backup_folder_input.clone();
        let default_save_input = panel.default_save_folder_input.clone();
        let wb_override = panel.workbook_override;
        let message = panel.message.clone();

        let body = v_flex()
            .p_4()
            .gap_4()
            .w_full()
            .p_0().child(
                // セクション 1: 既定の保存場所
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child(text("xlsx.backupsave.section-default-loc")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(text("xlsx.backupsave.default-loc-desc")),
                    )
                    .child({let target=default_save_input.clone();crate::shell::chrome::text_field(&default_save_input).full_width().path_picker("backup-default-folder",crate::shell::chrome::PathMode::Folder,&[],cx,move |_,path,w,cx|target.update(cx,|i,cx|i.set_value(path.to_string_lossy().to_string(),w,cx)))})
                    .child(
                        h_flex()
                            .justify_end()
                            .child(
                                crate::shell::chrome::action_button("reset-default-save-loc-btn")
                                    .small()
                                    .label(text("xlsx.backupsave.reset-default-loc"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.reset_xlsx_default_save_location_to_default(window, cx);
                                    })),
                            ),
                    ),
            )
            .child(
                // セクション 2: バックアップ設定
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child(text("xlsx.backupsave.section-backup")),
                    )
                    .child(
                        crate::shell::chrome::checkbox_button("toggle-always-backup-btn", text("xlsx.backupsave.always-backup-label"), always_create, false, cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.wb_mut().panels.backup_save.as_mut() {
                                    p.always_create_backup = !p.always_create_backup;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(text("xlsx.backupsave.backup-format-label")),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                crate::shell::chrome::radio_button("fmt-xlk-btn", is_xlk, false, text("xlsx.backupsave.format-xlk"), cx.listener(|this, _, _, cx| {
                                        if let Some(p) = this.wb_mut().panels.backup_save.as_mut() {
                                            p.backup_format = BackupFormat::Xlk;
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(
                                crate::shell::chrome::radio_button("fmt-xlsx-btn", !is_xlk, false, text("xlsx.backupsave.format-xlsx"), cx.listener(|this, _, _, cx| {
                                        if let Some(p) = this.wb_mut().panels.backup_save.as_mut() {
                                            p.backup_format = BackupFormat::Xlsx;
                                            cx.notify();
                                        }
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(text("xlsx.backupsave.backup-location-label")),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                crate::shell::chrome::radio_button("loc-same-btn", use_same_folder, false, text("xlsx.backupsave.loc-same-folder"), cx.listener(|this, _, _, cx| {
                                        if let Some(p) = this.wb_mut().panels.backup_save.as_mut() {
                                            p.use_same_folder = true;
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(
                                crate::shell::chrome::radio_button("loc-custom-btn", !use_same_folder, false, text("xlsx.backupsave.loc-custom-folder"), cx.listener(|this, _, _, cx| {
                                        if let Some(p) = this.wb_mut().panels.backup_save.as_mut() {
                                            p.use_same_folder = false;
                                            cx.notify();
                                        }
                                    })),
                            ),
                    )
                    .when(!use_same_folder, |this| {
                        this.child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.muted_foreground)
                                        .child(text("xlsx.backupsave.custom-folder-label")),
                                )
                                .child({let target=custom_input.clone();crate::shell::chrome::text_field(&custom_input).full_width().path_picker("backup-custom-folder",crate::shell::chrome::PathMode::Folder,&[],cx,move |_,path,w,cx|target.update(cx,|i,cx|i.set_value(path.to_string_lossy().to_string(),w,cx)))}),
                        )
                    }),
            )
            .child(
                // セクション 3: 現在のブックの設定
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child(text("xlsx.backupsave.section-workbook")),
                    )
                    .child(
                        crate::shell::chrome::Checkbox::new("toggle-wb-backup-override-btn")
                            .label(text("xlsx.backupsave.wb-backup-label"))
                            .checked(wb_override == Some(true))
                            .indeterminate(wb_override.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.wb_mut().panels.backup_save.as_mut() {
                                    p.workbook_override = match p.workbook_override {
                                        None => Some(true),
                                        Some(true) => Some(false),
                                        Some(false) => None,
                                    };
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        crate::shell::chrome::action_button("manual-backup-now-btn")
                            .outline()
                            .small()
                            .label(text("xlsx.backupsave.backup-now-btn"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.manual_create_backup_now(cx);
                            })),
                    ),
            )
            .when_some(message, |this, msg| {
                this.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .bg(theme.accent)
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.accent_foreground)
                                .child(msg),
                        ),
                )
            })
;
        let footer=                crate::shell::chrome::action_bar()

                    .action(
                        crate::shell::chrome::ActionDefinition::new("save-backup-save-settings-btn", "", crate::shell::chrome::ActionButtonKind::Primary)
                            .label(text("xlsx.backupsave.save-settings-btn"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.save_xlsx_backup_save_settings(cx);
                            })),
                    )
                    .action(
                        crate::shell::chrome::ActionDefinition::new("cancel-backup-save-settings-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                            .label(text("_shell.cancel"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_xlsx_backup_save_settings_panel(cx);
                            })),
                    );

        Some(SidebarPanel::with_footer(text("xlsx.backupsave.panel-title"),body.into_any_element(),footer))
    }
}
