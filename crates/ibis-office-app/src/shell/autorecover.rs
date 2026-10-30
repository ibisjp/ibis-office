// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 自動回復 (DOCX-FILE-014/015, XLSX-FILE-014..016, PPTX-FILE-010/012), shared by the formats: the settings
//! (how often, where, whether the last version is kept), a snapshot of the open file written to the AutoRecover folder
//! every few minutes while it has unsaved edits, the panel that lists what is there to recover and opens it, and the
//! banner over a recovered file. The folder is `<data>/ibisOffice/AutoRecover` (not in the browser edition).

use std::path::{Path, PathBuf};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use crate::shell::icons::Material;

/// AutoRecover configuration stored persistently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AutoRecoverConfig {
    /// 次の間隔で自動回復用データを保存する (Save AutoRecover info every X minutes).
    pub(crate) enabled: bool,
    /// 保存間隔（分、1〜120分、デフォルト10分）.
    pub(crate) interval_minutes: u32,
    /// 保存しないで終了する場合、最後に自動回復されたバージョンを残す (Keep last unsaved version).
    pub(crate) keep_last_unsaved: bool,
    /// カスタム保存先ディレクトリ（Noneの場合はデフォルト）.
    pub(crate) custom_location: Option<PathBuf>,
}

impl Default for AutoRecoverConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_minutes: 10,
            keep_last_unsaved: true,
            custom_location: None,
        }
    }
}

/// The default directory for AutoRecover files.
pub(crate) fn default_autorecover_dir() -> PathBuf {
    let base = crate::shell::app_data::base();
    let dir = base.unwrap_or_else(|| PathBuf::from(".")).join("ibisOffice").join("AutoRecover");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Where the config file is stored.
fn config_store_path() -> Option<PathBuf> {
    let base = crate::shell::app_data::base();
    Some(base?.join("ibisOffice").join("autorecover-config.tsv"))
}

/// Load configuration from disk, or return default.
pub(crate) fn load_autorecover_config() -> AutoRecoverConfig {
    let Some(path) = config_store_path() else { return AutoRecoverConfig::default() };
    let Ok(content) = std::fs::read_to_string(path) else { return AutoRecoverConfig::default() };
    let mut config = AutoRecoverConfig::default();
    for line in content.lines() {
        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() == 2 {
            match parts[0] {
                "enabled" => config.enabled = parts[1] == "1" || parts[1].eq_ignore_ascii_case("true"),
                "interval_minutes" => {
                    if let Ok(val) = parts[1].parse::<u32>() {
                        config.interval_minutes = val.clamp(1, 120);
                    }
                }
                "keep_last_unsaved" => config.keep_last_unsaved = parts[1] == "1" || parts[1].eq_ignore_ascii_case("true"),
                "custom_location" => {
                    config.custom_location = if parts[1].trim().is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(parts[1].trim()))
                    };
                }
                _ => {}
            }
        }
    }
    config
}

/// Save configuration to disk.
pub(crate) fn save_autorecover_config(config: &AutoRecoverConfig) {
    let Some(path) = config_store_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let loc_str = config.custom_location.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let text = format!(
        "enabled\t{}\ninterval_minutes\t{}\nkeep_last_unsaved\t{}\ncustom_location\t{}\n",
        if config.enabled { "1" } else { "0" },
        config.interval_minutes,
        if config.keep_last_unsaved { "1" } else { "0" },
        loc_str
    );
    let _ = std::fs::write(path, text);
}

/// Get effective directory for AutoRecover files.
pub(crate) fn effective_autorecover_dir(config: &AutoRecoverConfig) -> PathBuf {
    if let Some(custom) = &config.custom_location {
        if custom.is_dir() || std::fs::create_dir_all(custom).is_ok() {
            return custom.clone();
        }
    }
    default_autorecover_dir()
}

/// Format file size for human display.
pub(crate) fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Format timestamp from seconds.
pub(crate) fn format_timestamp_secs(secs: u64) -> String {
    let lt = ibis_doc_model::date_field::LocalTime::from_seconds(secs as i64);
    format!("{:04}-{:02}-{:02} {:02}:{:02}", lt.year, lt.month, lt.day, lt.hour, lt.minute)
}

/// An entry found in the AutoRecover directory.
#[derive(Clone, Debug)]
pub(crate) struct RecoveredFileEntry {
    pub(crate) path: PathBuf,
    pub(crate) file_name: String,
    pub(crate) modified_str: String,
    #[allow(dead_code)]
    pub(crate) size_bytes: u64,
    pub(crate) size_str: String,
}

/// The recovery files (`.docx`, `.xlsx`, `.pptx`) in the AutoRecover folder `dir`, the newest name first.
pub(crate) fn scan_recovered_files(dir: &Path) -> Vec<RecoveredFileEntry> {
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else { return entries };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ["docx", "xlsx", "pptx"].iter().any(|e| ext.eq_ignore_ascii_case(e))) {
            let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let metadata = entry.metadata().ok();
            let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified_str = metadata
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| format_timestamp_secs(d.as_secs()))
                .unwrap_or_else(|| "-".to_string());
            entries.push(RecoveredFileEntry { path, file_name, modified_str, size_bytes, size_str: format_file_size(size_bytes) });
        }
    }
    entries.sort_by(|a, b| b.path.cmp(&a.path));
    entries
}

/// The settings panel's state.
pub(crate) struct AutoRecoverSettingsPanel {
    pub(crate) enabled: bool,
    pub(crate) interval_input: Entity<InputState>,
    pub(crate) keep_last_unsaved: bool,
    pub(crate) location_input: Entity<InputState>,
    pub(crate) disable_for_this_file: bool,
    pub(crate) message: String,
}

/// The recovery panel's state.
pub(crate) struct RecoverUnsavedPanel {
    pub(crate) entries: Vec<RecoveredFileEntry>,
    pub(crate) message: String,
}

/// How often (ms) the timer looks whether a snapshot is due.
const TICK_MS: u64 = 30_000;

impl SlideEditor {
    /// The extension of the snapshot of the open file.
    fn autorecover_extension(&self) -> &'static str {
        if self.is_workbook() {
            "xlsx"
        } else if self.is_document() {
            "docx"
        } else {
            "pptx"
        }
    }

    /// A snapshot of the open file in the AutoRecover folder (`<name>_autorecover_<id>.<ext>`, the same file each time).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_autorecover_snapshot(&mut self) -> Result<PathBuf, String> {
        let (stem, _) = self.name_parts();
        let config = load_autorecover_config();
        let dir = effective_autorecover_dir(&config);
        let _ = std::fs::create_dir_all(&dir);
        let id = if self.is_workbook() { self.workbook_id } else { self.document_id };
        let target = dir.join(format!("{stem}_autorecover_{id}.{}", self.autorecover_extension()));
        self.copy_to(&target)?;
        self.active_autorecover_file = Some(target.clone());
        self.last_autorecover_ms = crate::clock::unix_millis();
        Ok(target)
    }

    /// The timer's look: a snapshot when it is on, this file has edits not saved and the interval has passed. Whether it
    /// wrote one.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn autorecover_tick(&mut self, now_ms: u64) -> bool {
        let config = load_autorecover_config();
        let doc_freq = self.flow().and_then(|f| f.save_frequency);
        if !config.enabled || self.autorecover_off || doc_freq == Some(0) || !self.has_document || self.is_recovered || !self.document.can_undo() {
            return false;
        }
        let interval_minutes = doc_freq.filter(|&f| f > 0).unwrap_or(config.interval_minutes);
        if now_ms.saturating_sub(self.last_autorecover_ms) < u64::from(interval_minutes) * 60_000 {
            return false;
        }
        self.write_autorecover_snapshot().is_ok()
    }

    /// Start the timer of this window (once): every half minute it looks whether a snapshot is due.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn start_autorecover_timer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.autorecover_timer_started || cfg!(feature = "scenarios") {
            return;
        }
        self.autorecover_timer_started = true;
        self.last_autorecover_ms = crate::clock::unix_millis();
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(TICK_MS)).await;
                if this.update(cx, |this, _| this.autorecover_tick(crate::clock::unix_millis())).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// The snapshot goes when the file is saved (or its recovery is dismissed).
    pub(crate) fn cleanup_active_autorecover_file(&mut self) {
        #[cfg(not(target_family = "wasm"))]
        if let Some(path) = self.active_autorecover_file.take() {
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    /// Closed without saving: the snapshot stays as `Unsaved_…` when that is asked for (and there are edits), else it goes.
    pub(crate) fn preserve_or_cleanup_autorecover_on_discard(&mut self) {
        #[cfg(not(target_family = "wasm"))]
        {
            let config = load_autorecover_config();
            if let Some(path) = self.active_autorecover_file.take() {
                if config.keep_last_unsaved && self.document.can_undo() && path.exists() {
                    let dir = path.parent().unwrap_or(Path::new("."));
                    let (stem, _) = self.name_parts();
                    let unsaved = dir.join(format!("Unsaved_{stem}_{}.{}", crate::clock::unix_millis(), self.autorecover_extension()));
                    let _ = std::fs::rename(&path, &unsaved);
                } else if path.exists() {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }

    /// Dismiss the banner: the file is no longer a recovered one, its snapshot goes.
    pub(crate) fn dismiss_recovery_banner(&mut self, cx: &mut Context<Self>) {
        self.is_recovered = false;
        self.cleanup_active_autorecover_file();
        cx.notify();
    }

    /// The band over a recovered file: save it under a name, or let it go.
    pub(crate) fn render_recovery_banner(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.is_recovered {
            return None;
        }
        Some(
            h_flex()
                .id("autorecover-banner")
                .w_full()
                .py_1()
                .px_3()
                .bg(rgb(0xfff8e1))
                .border_b_1()
                .border_color(rgb(0xffd54f))
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(Material("history")).size(px(18.)).text_color(rgb(0xd97706)))
                        .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(rgb(0x5d4037)).child(text("shell.autorecover.banner"))),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(crate::shell::chrome::primary_button("autorecover-banner-save-as").xsmall().label(text("shell.autorecover.save-as")).on_click(cx.listener(|this, _, window, cx| this.save_as(&crate::SaveAs, window, cx))))
                        .child(Button::new("autorecover-banner-dismiss").ghost().xsmall().label(text("shell.autorecover.discard")).on_click(cx.listener(|this, _, _, cx| this.dismiss_recovery_banner(cx)))),
                )
                .into_any_element(),
        )
    }

    // ---- 自動回復の設定 -------------------------------------------------------------------------------------

    /// ツール > 自動回復の設定…: the panel, with the settings now in force.
    pub(crate) fn open_autorecover_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let config = load_autorecover_config();
        let doc_freq = self.flow().and_then(|f| f.save_frequency);
        let interval_val = doc_freq.filter(|&f| f > 0).unwrap_or(config.interval_minutes);
        let location = config.custom_location.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| default_autorecover_dir().to_string_lossy().into_owned());
        let interval_input = cx.new(|cx| InputState::new(window, cx).default_value(interval_val.to_string()));
        let location_input = cx.new(|cx| InputState::new(window, cx).default_value(location));
        let disable_for_this_file = self.autorecover_off || doc_freq == Some(0);
        self.autorecover_settings = Some(AutoRecoverSettingsPanel { enabled: config.enabled, interval_input, keep_last_unsaved: config.keep_last_unsaved, location_input, disable_for_this_file, message: String::new() });
        self.recover_unsaved = None;
        self.sidebar_open = true;
        cx.notify();
    }

    /// Keep the settings in the panel: the file, and this file's own switch.
    pub(crate) fn save_autorecover_settings(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.autorecover_settings.as_ref() else { return };
        let (interval, location) = (panel.interval_input.read(cx).value().trim().to_string(), panel.location_input.read(cx).value().trim().to_string());
        let (enabled, keep_last_unsaved, disable) = (panel.enabled, panel.keep_last_unsaved, panel.disable_for_this_file);
        let message = |this: &mut Self, m: String| {
            if let Some(p) = this.autorecover_settings.as_mut() {
                p.message = m;
            }
        };
        let interval_minutes = match interval.parse::<u32>() {
            Ok(v) if (1..=120).contains(&v) => v,
            _ => {
                message(self, text("shell.autorecover.error-interval").to_string());
                return cx.notify();
            }
        };
        let custom_location = if location.is_empty() || location == default_autorecover_dir().to_string_lossy() {
            None
        } else {
            let path = PathBuf::from(&location);
            if let Err(e) = std::fs::create_dir_all(&path) {
                message(self, format!("{}: {e}", text("shell.autorecover.error-path")));
                return cx.notify();
            }
            Some(path)
        };
        save_autorecover_config(&AutoRecoverConfig { enabled, interval_minutes, keep_last_unsaved, custom_location });
        self.autorecover_off = disable;
        if self.is_document() {
            let freq = if disable { Some(0) } else { Some(interval_minutes) };
            // Setting the interval is not an edit: on an unedited document it does not start an undo history
            // (the document stays unmodified, so the timer has nothing to save).
            if self.document.can_undo() {
                self.run_flow(vec![ibis_doc_model::FlowCommand::SetSaveFrequency(freq)], cx);
            } else if let Some(flow) = self.document.ids().flow.as_mut() {
                std::sync::Arc::make_mut(flow).save_frequency = freq;
            }
        }
        message(self, text("shell.autorecover.saved").to_string());
        cx.notify();
    }

    pub(crate) fn autorecover_settings_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.autorecover_settings.as_ref()?;
        let (enabled, keep, disable, message) = (panel.enabled, panel.keep_last_unsaved, panel.disable_for_this_file, panel.message.clone());
        let (interval_input, location_input) = (panel.interval_input.clone(), panel.location_input.clone());
        let check = |id: &'static str, on: bool, label: &'static str, set: fn(&mut AutoRecoverSettingsPanel)| {
            crate::shell::chrome::checkbox_button(id,text(label),on,false,cx.listener(move |this, _, _, cx| {
                if let Some(p) = this.autorecover_settings.as_mut() {
                    set(p);
                }
                cx.notify();
            }))
        };
        let note = |label: &'static str| div().text_sm().text_color(cx.theme().muted_foreground).child(text(label));
        let body = v_flex()
            .gap(px(10.))
            .p(px(12.))
            .p_0().child(check("autorecover-enabled", enabled, "shell.autorecover.enable", |p| p.enabled = !p.enabled))
            .child(v_flex().gap(px(4.)).child(note("shell.autorecover.minutes")).child(crate::shell::chrome::text_field(&interval_input).unit(text("field.minutes"),px(28.)).spinner("autorecover-interval", crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, 120.0).integer_only(), cx, |_, _, _, _| {})))
            .child(check("autorecover-keep", keep, "shell.autorecover.keep", |p| p.keep_last_unsaved = !p.keep_last_unsaved))
            .child(
                v_flex()
                    .gap(px(4.))
                    .child(note("shell.autorecover.location"))
                    .child(crate::shell::chrome::text_field(&location_input).full_width().path_picker("autorecover-browse",crate::shell::chrome::PathMode::Folder,&[],cx,|_,_,_,_|{}))
                    .child(crate::shell::chrome::action_button("autorecover-reset").small().w(px(160.)).self_start().label(text("shell.autorecover.reset-location")).on_click(cx.listener(|this, _, window, cx| {
                        let default = default_autorecover_dir().to_string_lossy().into_owned();
                        if let Some(p) = this.autorecover_settings.as_ref() {
                            p.location_input.update(cx, |input, cx| input.set_value(default, window, cx));
                        }
                        cx.notify();
                    }))),
            )
            .child(check("autorecover-disable", disable, "shell.autorecover.disable-file", |p| p.disable_for_this_file = !p.disable_for_this_file))
            .child(crate::shell::chrome::action_button("autorecover-now").w(px(160.)).self_start().label(text("shell.autorecover.save-now")).on_click(cx.listener(|this, _, _, cx| {
                #[cfg(not(target_family = "wasm"))]
                let m = match this.write_autorecover_snapshot() {
                    Ok(path) => format!("{} ({})", text("shell.autorecover.now-done"), path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
                    Err(e) => format!("{}: {e}", text("shell.autorecover.now-failed")),
                };
                #[cfg(target_family = "wasm")]
                let m = text("shell.autorecover.wasm").to_string();
                if let Some(p) = this.autorecover_settings.as_mut() {
                    p.message = m;
                }
                cx.notify();
            })))
            .when(!message.is_empty(), |el| el.child(div().p(px(8.)).rounded_md().bg(cx.theme().accent.opacity(0.1)).text_sm().text_color(cx.theme().accent).child(message)))
            ;
        let panel_terminal_actions = crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("autorecover-save", text("shell.autorecover.save"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|this, _, _, cx| this.save_autorecover_settings(cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("autorecover-cancel", text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|this, _, _, cx| {
                        this.autorecover_settings = None;
                        cx.notify();
                    })));
        Some(SidebarPanel::with_footer(text("shell.autorecover.settings-title"), body.into_any_element(), panel_terminal_actions))
    }

    // ---- 未保存のファイルの回復 -----------------------------------------------------------------------------

    /// ファイル > 未保存のファイルの回復…: the panel, with what the AutoRecover folder holds.
    pub(crate) fn open_recover_unsaved(&mut self, cx: &mut Context<Self>) {
        let entries = scan_recovered_files(&effective_autorecover_dir(&load_autorecover_config()));
        self.recover_unsaved = Some(RecoverUnsavedPanel { entries, message: String::new() });
        self.autorecover_settings = None;
        self.sidebar_open = true;
        cx.notify();
    }

    /// Open the recovery file `index` of the panel: untitled (saving asks for a name), with the banner.
    pub(crate) fn open_recovered_entry(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.recover_unsaved.as_ref().and_then(|p| p.entries.get(index)).cloned() else { return };
        #[cfg(not(target_family = "wasm"))]
        match crate::read_file(&entry.path) {
            Ok(loaded) => {
                let name: SharedString = crate::shell::i18n::format("shell.autorecover.recovered-name", &[("file_name", &entry.file_name)]).into();
                self.load(loaded.presentation, name, window, cx);
                self.source = Some(loaded.source);
                self.path = None;
                self.is_recovered = true;
                self.active_autorecover_file = Some(entry.path);
                self.recover_unsaved = None;
                self.set_status(text("shell.autorecover.opened").to_string(), cx);
            }
            Err(err) => {
                if let Some(p) = self.recover_unsaved.as_mut() {
                    p.message = format!("{}: {err}", text("shell.autorecover.open-failed"));
                }
                cx.notify();
            }
        }
        #[cfg(target_family = "wasm")]
        let _ = (entry, window);
    }

    pub(crate) fn delete_recovered_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(path) = self.recover_unsaved.as_ref().and_then(|p| p.entries.get(index)).map(|e| e.path.clone()) {
            #[cfg(not(target_family = "wasm"))]
            let _ = std::fs::remove_file(path);
            #[cfg(target_family = "wasm")]
            let _ = path;
        }
        let entries = scan_recovered_files(&effective_autorecover_dir(&load_autorecover_config()));
        if let Some(p) = self.recover_unsaved.as_mut() {
            p.entries = entries;
            p.message = text("shell.autorecover.deleted").to_string();
        }
        cx.notify();
    }

    pub(crate) fn recover_unsaved_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.recover_unsaved.as_ref()?;
        let (entries, message) = (panel.entries.clone(), panel.message.clone());
        let body = v_flex()
            .gap(px(12.))
            .p(px(12.))
            .p_0().child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_sm().font_weight(FontWeight::BOLD).child(format!("{} ({})", text("shell.autorecover.recover-title"), entries.len())))
,
            )
            .when(!message.is_empty(), |el| el.child(div().p(px(8.)).rounded_md().bg(cx.theme().accent.opacity(0.1)).text_sm().text_color(cx.theme().accent).child(message)))
            .when(entries.is_empty(), |el| el.child(div().p(px(16.)).text_sm().text_color(cx.theme().muted_foreground).child(text("shell.autorecover.empty"))))
            .children(entries.iter().enumerate().map(|(idx, entry)| {
                v_flex()
                    .id(("recovered-item", idx))
                    .p(px(8.))
                    .rounded_md()
                    .border_1()
                    .border_color(cx.theme().border)
                    .gap(px(6.))
                    .child(div().text_sm().font_weight(FontWeight::BOLD).child(entry.file_name.clone()))
                    .child(h_flex().justify_between().text_sm().text_color(cx.theme().muted_foreground).child(entry.modified_str.clone()).child(entry.size_str.clone()))
                    .child(
                        h_flex()
                            .justify_end().w_full()
                            .gap(px(6.))
                            .child(crate::shell::chrome::action_button(("recover-open", idx)).small().label(text("shell.autorecover.open")).on_click(cx.listener(move |this, _, window, cx| this.open_recovered_entry(idx, window, cx))))
                            .child(crate::shell::chrome::destructive_button(("recover-delete", idx),cx).small().label(text("_shell.delete")).on_click(cx.listener(move |this, _, _, cx| this.delete_recovered_entry(idx, cx)))),
                    )
            }));
        let footer = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("recover-refresh", text("shell.autorecover.refresh"), crate::shell::chrome::ActionButtonKind::Normal)
                .on_click(cx.listener(|this, _, _, cx| this.open_recover_unsaved(cx))))

            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("shell.autorecover.recover-title"), body.into_any_element(), footer))
    }
}
