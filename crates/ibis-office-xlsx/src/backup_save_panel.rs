// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! XLSX-FILE-019: the workbook's panel of the backup and save location settings. The settings
//! themselves, and the backup copy made at each save, are `shell/backup_save.rs` (every format's save uses them).

#[cfg(not(target_family = "wasm"))]
use std::path::{Path, PathBuf};

use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::shell::backup_save::*;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use crate::ext::*;

#[ibis_office_macros::editor_ext(BackupSavePanel)]
impl SlideEditor {
    /// ツール > バックアップと保存先の設定… を開く.
    pub fn open_xlsx_backup_save_settings_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub fn xlsx_backup_save_settings_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
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
                            .shown(&match wb_override { Some(on) => crate::shell::chrome::Shown::One(on), None => crate::shell::chrome::Shown::Mixed })
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

