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

use gpui_kit::component::input::InputState;
use gpui_kit::*;


/// バックアップ ファイルの形式.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackupFormat {
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
pub enum BackupLocation {
    /// 保存対象ファイルと同一フォルダー
    #[default]
    SameFolder,
    /// 指定されたカスタム フォルダー
    Custom(PathBuf),
}

/// バックアップおよび保存先の設定.
#[derive(Debug, Clone)]
pub struct BackupSaveConfig {
    /// 上書き保存時に常にバックアップを作成するか
    pub always_create_backup: bool,
    /// 新規ブックの既定の保存場所（設定されている場合は優先使用）
    pub default_save_location: Option<PathBuf>,
    /// バックアップの保存形式 (.xlk / .xlsx)
    pub backup_format: BackupFormat,
    /// バックアップの保存場所（同一フォルダー / カスタム）
    pub backup_location: BackupLocation,
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
    let base = crate::app_data::base();
    Some(base?.join("ibisOffice").join("backup-save-config.tsv"))
}

pub fn load_backup_save_config() -> BackupSaveConfig {
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

pub fn save_backup_save_config(config: &BackupSaveConfig) -> std::io::Result<()> {
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
pub fn system_default_save_location() -> PathBuf {
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

/// 設定でユーザーが指定した既定の保存場所（無い・作れないときは `None`）.
pub fn custom_default_save_location() -> Option<PathBuf> {
    let custom = load_backup_save_config().default_save_location?;
    (custom.exists() || std::fs::create_dir_all(&custom).is_ok()).then_some(custom)
}

/// 有効な既定の保存場所（カスタム設定があればそれ、なければ OS 既定）.
pub fn effective_default_save_location() -> PathBuf {
    custom_default_save_location().unwrap_or_else(system_default_save_location)
}

// ----------------------------------------------------------------------------
// バックアップ ファイル生成ロジック
// ----------------------------------------------------------------------------

/// バックアップ ファイル名を生成.
/// Excel 互換形式: `<ファイル名> のバックアップ.xlk` または `<ファイル名>.xlk`
pub fn generate_backup_file_name(target_path: &Path, format: BackupFormat) -> String {
    let stem = target_path.file_stem().and_then(|s| s.to_str()).unwrap_or("Workbook");
    let ext = match format {
        BackupFormat::Xlk => "xlk",
        BackupFormat::Xlsx => "xlsx",
    };
    crate::i18n::format("_xlsx.backup-of-stem-ext", &[("stem", &stem), ("ext", &ext)])
}

/// 上書き保存前にバックアップ コピーを作成（条件を満たす場合）.
pub fn create_backup_copy_if_enabled(
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

        std::fs::copy(target_path, &backup_path).map_err(|e| crate::i18n::format("_xlsx.could-not-create-the-backup-e", &[("e", &e)]))?;
        Ok(Some(backup_path))
    }
}

// ----------------------------------------------------------------------------
// バックアップと保存先設定パネル (UI Panel)
// ----------------------------------------------------------------------------

pub struct BackupSaveSettingsPanel {
    pub always_create_backup: bool,
    pub backup_format: BackupFormat,
    pub use_same_folder: bool,
    pub custom_backup_folder_input: Entity<InputState>,
    pub default_save_folder_input: Entity<InputState>,
    pub workbook_override: Option<bool>,
    pub message: Option<String>,
}
