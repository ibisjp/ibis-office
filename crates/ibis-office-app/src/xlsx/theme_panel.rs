// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! テーマの配色とフォント (表示形式 > テーマの配色 / テーマの色とフォント…):
//! ブックのテーマ（12色のカラースキームおよび見出し・本文フォント）の切り替え・編集。

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, FontSet, Rgb, ThemeFonts};

use crate::pptx::theme_panel::{COLORS, THEME_PALETTES};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct XlsxThemePanel {
    pub(crate) colors: Vec<(&'static str, &'static str, Entity<InputState>)>,
    pub(crate) fonts: [Entity<InputState>; 4],
    pub(crate) error: Option<SharedString>,
}



fn parse_hex(text: &str) -> Option<Rgb> {
    let t = text.trim().trim_start_matches('#');
    (t.len() == 6).then(|| u32::from_str_radix(t, 16).ok()).flatten().map(Rgb)
}

impl SlideEditor {
    /// The workbook's 10 palette theme colors (lt1, dk1, lt2, dk2, accent1..6)
    /// based on the current workbook model theme, falling back to source.
    pub(crate) fn current_theme_palette_colors(&self) -> [Rgb; 10] {
        if let Some(w) = self.presentation().workbook.as_ref() {
            if !w.theme_colors.is_empty() {
                return ibis_pptx::workbook_theme_colors_from_slice(&w.theme_colors);
            }
        }
        ibis_pptx::workbook_theme_colors(self.source.as_deref())
    }

    /// Check if the workbook matches a predefined theme palette preset.
    pub(crate) fn is_xlsx_theme_palette_active(&self, preset_id: &str) -> bool {
        let Some(preset) = THEME_PALETTES.iter().find(|p| p.id == preset_id) else { return false };
        let current_colors = self.presentation().workbook.as_ref().map(|w| &w.theme_colors).unwrap_or(&self.presentation().theme_colors);
        if current_colors.is_empty() {
            return preset_id == "office";
        }
        preset.colors.iter().all(|(name, rgb)| current_colors.iter().find(|(n, _)| n == name).is_some_and(|(_, c)| c == rgb))
    }

    /// Apply a predefined theme color palette preset to the workbook.
    pub(crate) fn apply_xlsx_theme_palette(&mut self, preset_id: &str, cx: &mut Context<Self>) {
        let Some(preset) = THEME_PALETTES.iter().find(|p| p.id == preset_id) else { return };
        let mut colors = self.presentation().workbook.as_ref().map(|w| w.theme_colors.clone()).unwrap_or_else(|| self.presentation().theme_colors.clone());
        for (name, rgb) in preset.colors {
            if let Some(entry) = colors.iter_mut().find(|(n, _)| n == name) {
                entry.1 = rgb;
            } else {
                colors.push((name.to_string(), rgb));
            }
        }
        let fonts = self.presentation().workbook.as_ref().and_then(|w| w.theme_fonts.clone()).or_else(|| self.presentation().theme_fonts.clone());
        self.commit_cell_edit(cx);
        self.run_sheet(vec![Command::SetTheme { colors, fonts }], cx);
        if let Some(state) = self.sheet_state_mut() {
            *state.panels.cell_styles.borrow_mut() = None;
        }
        cx.notify();
    }



    /// Open the workbook theme colors and fonts editing sidebar panel.
    pub(crate) fn open_xlsx_theme_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.theme_colors_ui.editing=false;
        self.commit_cell_edit(cx);
        let have = self.presentation().workbook.as_ref().map(|w| w.theme_colors.clone()).unwrap_or_else(|| self.presentation().theme_colors.clone());
        let fonts = self.presentation().workbook.as_ref().and_then(|w| w.theme_fonts.clone()).or_else(|| self.presentation().theme_fonts.clone()).unwrap_or_default();
        let colors = COLORS
            .iter()
            .filter_map(|(name, label)| {
                let rgb = have.iter().find(|(n, _)| n == name).map(|(_, c)| *c).unwrap_or(Rgb(0x000000));
                let input = cx.new(|cx| InputState::new(window, cx).default_value(format!("#{:06X}", rgb.0 & 0xffffff)));
                Some((*name, *label, input))
            })
            .collect::<Vec<_>>();
        let font = |window: &mut Window, cx: &mut Context<Self>, v: &Option<String>| cx.new(|cx| InputState::new(window, cx).default_value(v.clone().unwrap_or_default()));
        let fonts = [
            font(window, cx, &fonts.major.latin),
            font(window, cx, &fonts.major.east_asian),
            font(window, cx, &fonts.minor.latin),
            font(window, cx, &fonts.minor.east_asian),
        ];
        if let Some(s) = self.sheet_state_mut() {
            s.close_settings();
            s.panels.theme = Some(XlsxThemePanel { colors, fonts, error: None });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_xlsx_theme_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.panels.theme = None;
        }
        self.sidebar_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn apply_xlsx_theme_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.sheet_state().and_then(|s| s.panels.theme.as_ref()) else { return };
        let mut colors = self.presentation().workbook.as_ref().map(|w| w.theme_colors.clone()).unwrap_or_else(|| self.presentation().theme_colors.clone());
        for (name, _, input) in &panel.colors {
            let Some(rgb) = parse_hex(input.read(cx).value().as_ref()) else {
                if let Some(p) = self.sheet_state_mut().and_then(|s| s.panels.theme.as_mut()) {
                    p.error = Some(crate::shell::i18n::text("pptx.theme-panel.enter-colors-rrggbb").into());
                }
                return cx.notify();
            };
            if let Some(entry) = colors.iter_mut().find(|(n, _)| n == name) {
                entry.1 = rgb;
            } else {
                colors.push((name.to_string(), rgb));
            }
        }
        let text = |i: usize| Some(panel.fonts[i].read(cx).value().trim().to_string()).filter(|t| !t.is_empty());
        let old = self.presentation().workbook.as_ref().and_then(|w| w.theme_fonts.clone()).or_else(|| self.presentation().theme_fonts.clone());
        let fonts = ThemeFonts {
            major: FontSet { latin: text(0), east_asian: text(1), complex: old.as_ref().and_then(|f| f.major.complex.clone()) },
            minor: FontSet { latin: text(2), east_asian: text(3), complex: old.as_ref().and_then(|f| f.minor.complex.clone()) },
        };
        let fonts = Some(fonts).filter(|f| old.is_some() || *f != ThemeFonts::default());
        self.commit_cell_edit(cx);
        self.run_sheet(vec![Command::SetTheme { colors, fonts }], cx);
        if let Some(state) = self.sheet_state_mut() {
            *state.panels.cell_styles.borrow_mut() = None;
        }
        self.close_xlsx_theme_panel(window, cx);
    }

    pub(crate) fn xlsx_theme_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.shared_theme_colors_panel(cx)
    }

    /// Check if the workbook matches a predefined theme fonts preset.
    pub(crate) fn is_xlsx_theme_fonts_active(&self, preset_id: &str) -> bool {
        let preset_id = if preset_id == "aptos" { "office" } else { preset_id };
        let Some(preset) = THEME_FONTS_PRESETS.iter().find(|p| p.id == preset_id) else { return false };
        let current_fonts = self.presentation().workbook.as_ref().and_then(|w| w.theme_fonts.as_ref()).or_else(|| self.presentation().theme_fonts.as_ref());
        match current_fonts {
            Some(f) => {
                f.major.latin.as_deref() == Some(preset.major_latin) && f.minor.latin.as_deref() == Some(preset.minor_latin)
            }
            None => preset_id == "calibri" || preset_id == "office",
        }
    }

    /// Apply a predefined theme fonts preset to the workbook.
    pub(crate) fn apply_xlsx_theme_fonts(&mut self, preset_id: &str, cx: &mut Context<Self>) {
        let preset_id = if preset_id == "aptos" { "office" } else { preset_id };
        let Some(preset) = THEME_FONTS_PRESETS.iter().find(|p| p.id == preset_id) else { return };
        let colors = self.presentation().workbook.as_ref().map(|w| w.theme_colors.clone()).unwrap_or_else(|| self.presentation().theme_colors.clone());
        let fonts = ThemeFonts {
            major: FontSet {
                latin: Some(preset.major_latin.to_string()),
                east_asian: Some(preset.major_ea.to_string()),
                complex: None,
            },
            minor: FontSet {
                latin: Some(preset.minor_latin.to_string()),
                east_asian: Some(preset.minor_ea.to_string()),
                complex: None,
            },
        };
        self.commit_cell_edit(cx);
        self.run_sheet(vec![Command::SetTheme { colors, fonts: Some(fonts) }], cx);
        if let Some(state) = self.sheet_state_mut() {
            *state.panels.cell_styles.borrow_mut() = None;
        }
        self.status = Some(crate::shell::i18n::text("xlsx.theme-fonts-applied").into());
        cx.notify();
    }

    /// Check if the workbook matches a predefined theme effect preset.
    pub(crate) fn is_xlsx_theme_effects_active(&self, preset_id: &str) -> bool {
        let preset_id = match preset_id {
            "subtle" => "apex",
            "moderate" => "aspect",
            "intense" => "metro",
            other => other,
        };
        let current = self.presentation().theme_effects.as_deref().unwrap_or("office");
        let Some(preset) = ibis_pptx::theme_effects::find_effect_preset(preset_id) else { return false };
        current.eq_ignore_ascii_case(preset.id) || current.eq_ignore_ascii_case(preset.name)
    }

    /// Apply a predefined theme effect preset to the workbook.
    pub(crate) fn apply_xlsx_theme_effects(&mut self, preset_id: &str, cx: &mut Context<Self>) {
        let preset_id = match preset_id {
            "subtle" => "apex",
            "moderate" => "aspect",
            "intense" => "metro",
            other => other,
        };
        let Some(preset) = ibis_pptx::theme_effects::find_effect_preset(preset_id) else { return };
        self.commit_cell_edit(cx);
        self.run_sheet(vec![Command::SetThemeEffects { name: Some(preset.id.to_string()) }], cx);
        self.status = Some(crate::shell::i18n::format("xlsx.theme-effect-applied", &[("name", &crate::shell::i18n::text(preset.label_key))]).into());
        cx.notify();
    }
}

pub(crate) struct ThemeFontsPreset {
    pub(crate) id: &'static str,
    pub(crate) action_id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) major_latin: &'static str,
    pub(crate) major_ea: &'static str,
    pub(crate) minor_latin: &'static str,
    pub(crate) minor_ea: &'static str,
}

pub(crate) const THEME_FONTS_PRESETS: [ThemeFontsPreset; 6] = [
    ThemeFontsPreset {
        id: "office",
        action_id: "xlsx.theme-fonts.office",
        label: "Office (Aptos)",
        major_latin: "Aptos Display",
        major_ea: "Yu Gothic",
        minor_latin: "Aptos",
        minor_ea: "Yu Gothic",
    },
    ThemeFontsPreset {
        id: "calibri",
        action_id: "xlsx.theme-fonts.calibri",
        label: "Office 2007-2022 (Calibri)",
        major_latin: "Calibri Light",
        major_ea: "Yu Gothic Light",
        minor_latin: "Calibri",
        minor_ea: "Yu Gothic",
    },
    ThemeFontsPreset {
        id: "arial",
        action_id: "xlsx.theme-fonts.arial",
        label: "Arial",
        major_latin: "Arial",
        major_ea: "MS Gothic",
        minor_latin: "Arial",
        minor_ea: "MS PGothic",
    },
    ThemeFontsPreset {
        id: "times",
        action_id: "xlsx.theme-fonts.times",
        label: "Times New Roman",
        major_latin: "Times New Roman",
        major_ea: "MS Mincho",
        minor_latin: "Times New Roman",
        minor_ea: "MS PMincho",
    },
    ThemeFontsPreset {
        id: "century-gothic",
        action_id: "xlsx.theme-fonts.century-gothic",
        label: "Century Gothic",
        major_latin: "Century Gothic",
        major_ea: "Yu Gothic",
        minor_latin: "Century Gothic",
        minor_ea: "Yu Gothic",
    },
    ThemeFontsPreset {
        id: "meiryo",
        action_id: "xlsx.theme-fonts.meiryo",
        label: "Meiryo",
        major_latin: "Meiryo UI",
        major_ea: "Meiryo UI",
        minor_latin: "Meiryo",
        minor_ea: "Meiryo",
    },
];

pub(crate) fn theme_effect_action_id(id: &str) -> &'static str {
    match id {
        "office" => "xlsx.theme-effects.office",
        "office_2007_2010" => "xlsx.theme-effects.office-2007-2010",
        "metro" => "xlsx.theme-effects.metro",
        "aspect" => "xlsx.theme-effects.aspect",
        "apex" => "xlsx.theme-effects.apex",
        "flow" => "xlsx.theme-effects.flow",
        "foundry" => "xlsx.theme-effects.foundry",
        "median" => "xlsx.theme-effects.median",
        "solstice" => "xlsx.theme-effects.solstice",
        "trek" => "xlsx.theme-effects.trek",
        _ => "xlsx.theme-effects.office",
    }
}
