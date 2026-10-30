// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! テーマの色とフォント (書式 > テーマの色とフォント…): the theme's twelve
//! colours (by their `#RRGGBB`) and its heading and body fonts edited.
//! The shapes, backgrounds and layouts drawn in the theme's colours follow
//! (what the file gives them under the new theme, read back through it);
//! shapes the user has changed since the file was read keep theirs. No
//! colour sets are listed: the colours are the ones the file's theme has.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, FontSet, Rgb, ThemeFonts};

use crate::pptx::SlidePopup;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The scheme's colours in the order PowerPoint lists them, with their names.
pub(crate) const COLORS: [(&'static str, &'static str); 12] = [
    ("dk1", "pptx.theme-panel.text-background-dark-1"),
    ("lt1", "pptx.theme-panel.text-background-light-1"),
    ("dk2", "pptx.theme-panel.text-background-dark-2"),
    ("lt2", "pptx.theme-panel.text-background-light-2"),
    ("accent1", "_xlsx.accent-1"),
    ("accent2", "_xlsx.accent-2"),
    ("accent3", "_xlsx.accent-3"),
    ("accent4", "_xlsx.accent-4"),
    ("accent5", "_xlsx.accent-5"),
    ("accent6", "_xlsx.accent-6"),
    ("hlink", "pptx.links.hyperlink"),
    ("folHlink", "pptx.theme-panel.followed-hyperlink"),
];

pub(crate) struct ThemePalettePreset {
    pub(crate) id: &'static str,
    pub(crate) label_key: &'static str,
    pub(crate) selected_action_id: &'static str,
    pub(crate) colors: [(&'static str, Rgb); 12],
}

pub(crate) const THEME_PALETTES: [ThemePalettePreset; 7] = [
    ThemePalettePreset {
        id: "office",
        label_key: "pptx.theme.palette.office",
        selected_action_id: "pptx.theme.palette-selected.office",
        colors: [
            ("dk1", Rgb(0x000000)), ("lt1", Rgb(0xffffff)),
            ("dk2", Rgb(0x44546a)), ("lt2", Rgb(0xe7e6e6)),
            ("accent1", Rgb(0x4472c4)), ("accent2", Rgb(0xed7d31)),
            ("accent3", Rgb(0xa5a5a5)), ("accent4", Rgb(0xffc000)),
            ("accent5", Rgb(0x5b9bd5)), ("accent6", Rgb(0x70ad47)),
            ("hlink", Rgb(0x0563c1)), ("folHlink", Rgb(0x954f72)),
        ],
    },
    ThemePalettePreset {
        id: "aspect",
        label_key: "pptx.theme.palette.aspect",
        selected_action_id: "pptx.theme.palette-selected.aspect",
        colors: [
            ("dk1", Rgb(0x000000)), ("lt1", Rgb(0xffffff)),
            ("dk2", Rgb(0x242852)), ("lt2", Rgb(0xe3e3e3)),
            ("accent1", Rgb(0xf0a22e)), ("accent2", Rgb(0xa56110)),
            ("accent3", Rgb(0x90a437)), ("accent4", Rgb(0x43896a)),
            ("accent5", Rgb(0x427891)), ("accent6", Rgb(0x4e4273)),
            ("hlink", Rgb(0xd86a14)), ("folHlink", Rgb(0x745a45)),
        ],
    },
    ThemePalettePreset {
        id: "apex",
        label_key: "pptx.theme.palette.apex",
        selected_action_id: "pptx.theme.palette-selected.apex",
        colors: [
            ("dk1", Rgb(0x000000)), ("lt1", Rgb(0xffffff)),
            ("dk2", Rgb(0x494949)), ("lt2", Rgb(0xc9c9c9)),
            ("accent1", Rgb(0xce6533)), ("accent2", Rgb(0x925345)),
            ("accent3", Rgb(0x564e68)), ("accent4", Rgb(0x607b7a)),
            ("accent5", Rgb(0x538a6a)), ("accent6", Rgb(0x8e8d50)),
            ("hlink", Rgb(0xbc542b)), ("folHlink", Rgb(0x7a574e)),
        ],
    },
    ThemePalettePreset {
        id: "slate",
        label_key: "pptx.theme.palette.slate",
        selected_action_id: "pptx.theme.palette-selected.slate",
        colors: [
            ("dk1", Rgb(0x000000)), ("lt1", Rgb(0xffffff)),
            ("dk2", Rgb(0x34495e)), ("lt2", Rgb(0xecf0f1)),
            ("accent1", Rgb(0x5b7085)), ("accent2", Rgb(0x8496a6)),
            ("accent3", Rgb(0xa9b8c5)), ("accent4", Rgb(0x4a5b6c)),
            ("accent5", Rgb(0x687d94)), ("accent6", Rgb(0x8d9faf)),
            ("hlink", Rgb(0x3498db)), ("folHlink", Rgb(0x9b59b6)),
        ],
    },
    ThemePalettePreset {
        id: "median",
        label_key: "pptx.theme.palette.median",
        selected_action_id: "pptx.theme.palette-selected.median",
        colors: [
            ("dk1", Rgb(0x000000)), ("lt1", Rgb(0xffffff)),
            ("dk2", Rgb(0x444444)), ("lt2", Rgb(0xefefef)),
            ("accent1", Rgb(0x775f55)), ("accent2", Rgb(0x8c786e)),
            ("accent3", Rgb(0xa19187)), ("accent4", Rgb(0xb6aaa1)),
            ("accent5", Rgb(0xcbc3bb)), ("accent6", Rgb(0xe0ddd5)),
            ("hlink", Rgb(0x93381e)), ("folHlink", Rgb(0x5f2e23)),
        ],
    },
    ThemePalettePreset {
        id: "orient",
        label_key: "pptx.theme.palette.orient",
        selected_action_id: "pptx.theme.palette-selected.orient",
        colors: [
            ("dk1", Rgb(0x000000)), ("lt1", Rgb(0xffffff)),
            ("dk2", Rgb(0x424242)), ("lt2", Rgb(0xededed)),
            ("accent1", Rgb(0x535455)), ("accent2", Rgb(0x687b89)),
            ("accent3", Rgb(0x9a7b56)), ("accent4", Rgb(0x899573)),
            ("accent5", Rgb(0xac483b)), ("accent6", Rgb(0x736f6e)),
            ("hlink", Rgb(0xb4382b)), ("folHlink", Rgb(0x7e4f49)),
        ],
    },
    ThemePalettePreset {
        id: "blue",
        label_key: "pptx.theme.palette.blue",
        selected_action_id: "pptx.theme.palette-selected.blue",
        colors: [
            ("dk1", Rgb(0x000000)), ("lt1", Rgb(0xffffff)),
            ("dk2", Rgb(0x263859)), ("lt2", Rgb(0xe8eef5)),
            ("accent1", Rgb(0x386fa4)), ("accent2", Rgb(0x59a5d8)),
            ("accent3", Rgb(0x84d2f6)), ("accent4", Rgb(0x91e5f6)),
            ("accent5", Rgb(0x133c55)), ("accent6", Rgb(0x588b8b)),
            ("hlink", Rgb(0x1a759f)), ("folHlink", Rgb(0x184e77)),
        ],
    },
];

pub(crate) struct ThemePanel {
    /// One input per colour of [`COLORS`] the theme has.
    pub(crate) colors: Vec<(&'static str, &'static str, Entity<InputState>)>,
    /// Heading Latin, heading East Asian, body Latin, body East Asian.
    pub(crate) fonts: [Entity<InputState>; 4],
    pub(crate) error: Option<SharedString>,
}

fn parse_hex(text: &str) -> Option<Rgb> {
    let t = text.trim().trim_start_matches('#');
    (t.len() == 6).then(|| u32::from_str_radix(t, 16).ok()).flatten().map(Rgb)
}

impl SlideEditor {
    pub(crate) fn open_theme_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.theme_colors_ui.editing=false;
        self.end_text_edit(window, cx);
        let p = self.presentation();
        let have = p.theme_colors.clone();
        let fonts = p.theme_fonts.clone().unwrap_or_default();
        let colors = COLORS
            .iter()
            .filter_map(|(name, label)| {
                let rgb = have.iter().find(|(n, _)| n == name)?.1;
                let input = cx.new(|cx| InputState::new(window, cx).default_value(format!("#{:06X}", rgb.0 & 0xffffff)));
                Some((*name, *label, input))
            })
            .collect::<Vec<_>>();
        if colors.is_empty() {
            return self.set_status(crate::shell::i18n::text("pptx.theme-panel.file-has-no-theme-colors").to_string(), cx);
        }
        let font = |window: &mut Window, cx: &mut Context<Self>, v: &Option<String>| cx.new(|cx| InputState::new(window, cx).default_value(v.clone().unwrap_or_default()));
        let fonts = [
            font(window, cx, &fonts.major.latin),
            font(window, cx, &fonts.major.east_asian),
            font(window, cx, &fonts.minor.latin),
            font(window, cx, &fonts.minor.east_asian),
        ];
        if let Some(s) = self.pptx_mut() {
            s.theme_panel = Some(ThemePanel { colors, fonts, error: None });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_theme_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.theme_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Apply a predefined theme color palette to the presentation.
    pub(crate) fn apply_theme_palette(&mut self, preset_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preset) = THEME_PALETTES.iter().find(|p| p.id == preset_id) else { return };
        let mut colors = self.presentation().theme_colors.clone();
        for (name, rgb) in preset.colors {
            if let Some(entry) = colors.iter_mut().find(|(n, _)| n == name) {
                entry.1 = rgb;
            } else {
                colors.push((name.to_string(), rgb));
            }
        }
        let fonts = self.presentation().theme_fonts.clone();
        self.end_text_edit(window, cx);
        if colors != self.presentation().theme_colors {
            let commands = match &self.source {
                Some(source) => source.retheme(self.presentation(), colors.clone(), fonts).map_err(|e| e.to_string()),
                None => Ok(vec![Command::SetTheme { colors, fonts }]),
            };
            match commands {
                Ok(commands) => self.run(commands, cx),
                Err(err) => log::warn!("Could not change the theme: {err}"),
            }
        }
        if self.pptx().and_then(|s| s.theme_panel.as_ref()).is_some() {
            self.open_theme_panel(window, cx);
        }
    }

    /// Apply a predefined theme color palette to the specified slides.
    pub(crate) fn apply_theme_palette_to_slides(&mut self, preset_id: &str, slide_ids: &[ibis_doc_model::SlideId], window: &mut Window, cx: &mut Context<Self>) {
        let Some(preset) = THEME_PALETTES.iter().find(|p| p.id == preset_id) else { return };
        let mut colors = self.presentation().theme_colors.clone();
        for (name, rgb) in preset.colors {
            if let Some(entry) = colors.iter_mut().find(|(n, _)| n == name) {
                entry.1 = rgb;
            } else {
                colors.push((name.to_string(), rgb));
            }
        }
        let fonts = self.presentation().theme_fonts.clone();
        self.end_text_edit(window, cx);
        let commands = match &self.source {
            Some(source) => source.retheme_slides(self.presentation(), slide_ids, colors, fonts).map_err(|e| e.to_string()),
            None => Ok(Vec::new()),
        };
        match commands {
            Ok(commands) if !commands.is_empty() => self.run(commands, cx),
            Ok(_) => {}
            Err(err) => log::warn!("Could not apply the theme to the selected slides: {err}"),
        }
    }

    /// Apply a predefined theme color palette to the currently active slide.
    pub(crate) fn apply_theme_palette_to_current_slide(&mut self, preset_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.current_slide;
        self.apply_theme_palette_to_slides(preset_id, &[current], window, cx);
    }

    /// Check if the current presentation matches a given theme palette.
    pub(crate) fn is_theme_palette_active(&self, preset_id: &str) -> bool {
        let Some(preset) = THEME_PALETTES.iter().find(|p| p.id == preset_id) else { return false };
        let colors = &self.presentation().theme_colors;
        preset.colors.iter().all(|(name, rgb)| colors.iter().find(|(n, _)| n == name).is_some_and(|(_, c)| c == rgb))
    }

    /// Apply a predefined theme effect preset to the presentation.
    pub(crate) fn apply_theme_effects(&mut self, preset_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preset) = ibis_pptx::theme_effects::find_effect_preset(preset_id) else { return };
        self.end_text_edit(window, cx);
        let commands = match &self.source {
            Some(source) => source.retheme_effects(self.presentation(), preset.id).map_err(|e| e.to_string()),
            None => Ok(vec![Command::SetThemeEffects { name: Some(preset.id.to_string()) }]),
        };
        match commands {
            Ok(commands) => self.run(commands, cx),
            Err(err) => log::warn!("Could not change the theme effects: {err}"),
        }
        if self.pptx().and_then(|s| s.theme_panel.as_ref()).is_some() {
            self.open_theme_panel(window, cx);
        }
    }

    /// Check if the current presentation matches a given theme effect preset.
    pub(crate) fn is_theme_effects_active(&self, preset_id: &str) -> bool {
        let current = self.presentation().theme_effects.as_deref().unwrap_or("office");
        let Some(preset) = ibis_pptx::theme_effects::find_effect_preset(preset_id) else { return false };
        current.eq_ignore_ascii_case(preset.id) || current.eq_ignore_ascii_case(preset.name)
    }

    pub(crate) fn apply_theme_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.theme_panel.as_ref()) else { return };
        let mut colors = self.presentation().theme_colors.clone();
        for (name, _, input) in &panel.colors {
            let Some(rgb) = parse_hex(input.read(cx).value().as_ref()) else {
                if let Some(p) = self.pptx_mut().and_then(|s| s.theme_panel.as_mut()) {
                    p.error = Some(crate::shell::i18n::text("pptx.theme-panel.enter-colors-rrggbb").into());
                }
                return cx.notify();
            };
            if let Some(entry) = colors.iter_mut().find(|(n, _)| n == name) {
                entry.1 = rgb;
            }
        }
        let text = |i: usize| Some(panel.fonts[i].read(cx).value().trim().to_string()).filter(|t| !t.is_empty());
        let old = self.presentation().theme_fonts.clone();
        let fonts = ThemeFonts {
            major: FontSet { latin: text(0), east_asian: text(1), complex: old.as_ref().and_then(|f| f.major.complex.clone()) },
            minor: FontSet { latin: text(2), east_asian: text(3), complex: old.as_ref().and_then(|f| f.minor.complex.clone()) },
        };
        let fonts = Some(fonts).filter(|f| old.is_some() || *f != ThemeFonts::default());
        self.end_text_edit(window, cx);
        if colors != self.presentation().theme_colors || fonts != old {
            let commands = match &self.source {
                Some(source) => source.retheme(self.presentation(), colors.clone(), fonts.clone()).map_err(|e| e.to_string()),
                None => Ok(vec![Command::SetTheme { colors, fonts }]),
            };
            match commands {
                Ok(commands) => self.run(commands, cx),
                Err(err) => log::warn!("Could not change the theme: {err}"),
            }
        }
        self.close_theme_panel(window, cx);
    }

    pub(crate) fn theme_panel_context_menu_entries(&self, preset_id: Option<&'static str>) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
        if self.pptx().and_then(|s| s.theme_panel.as_ref()).is_none() {
            return Vec::new();
        }
        let mut items = Vec::new();
        if let Some(pid) = preset_id {
            if let Some(preset) = THEME_PALETTES.iter().find(|p| p.id == pid) {
                items.push(
                    MenuItem::tr(preset.label_key)
                        .checked(self.is_theme_palette_active(pid))
                        .on(move |e, w, cx| e.apply_theme_palette(pid, w, cx))
                        .into(),
                );
                items.push(
                    MenuItem::new(
                        preset.selected_action_id,
                        format!("{} ({})", crate::shell::i18n::text("pptx.theme.palette-selected"), crate::shell::i18n::text(preset.label_key)),
                    )
                    .on(move |e, w, cx| e.apply_theme_palette_to_current_slide(pid, w, cx))
                    .into(),
                );
                items.push(MenuEntry::Separator);
            }
        }
        items.push(
            MenuItem::tr("_shell.apply")
                .icon("check")
                .on(|e, w, cx| e.apply_theme_panel(w, cx))
                .into(),
        );
        items.push(MenuEntry::Separator);
        let palette_items: Vec<MenuEntry> = THEME_PALETTES
            .iter()
            .map(|preset| {
                let pid = preset.id;
                MenuItem::tr(preset.label_key)
                    .checked(self.is_theme_palette_active(pid))
                    .on(move |e, w, cx| e.apply_theme_palette(pid, w, cx))
                    .into()
            })
            .collect();
        items.push(Submenu::tr("pptx.theme.palette", "palette", palette_items).into());
        items.push(Submenu::tr("pptx.theme.palette-selected", "palette", super::chrome::theme_palette_selected_items(self)).into());
        items.push(Submenu::tr("pptx.theme.effects", "auto_awesome", super::chrome::theme_effects_items(self)).into());
        items.push(MenuEntry::Separator);
        items.push(
            MenuItem::tr("pptx.theme.save-theme")
                .icon("save")
                .on(|e, w, cx| e.export_thmx(w, cx))
                .into(),
        );
        items.push(MenuEntry::Separator);
        items.push(
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|e, w, cx| e.close_theme_panel(w, cx))
                .into(),
        );
        items
    }

    pub(crate) fn open_theme_panel_context_menu(
        &mut self,
        preset_id: Option<&'static str>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.end_text_edit(window, cx);
        let items = self.theme_panel_context_menu_entries(preset_id);
        if items.is_empty() {
            return;
        }
        self.focus_handle.focus(window, cx);
        let entries = std::rc::Rc::new(items);
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
            crate::shell::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(state) = this.pptx_mut() {
                state.slide_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        if let Some(state) = self.pptx_mut() {
            state.slide_popup = Some(SlidePopup { menu, position, _dismiss: dismiss });
        }
        cx.notify();
    }

    pub(crate) fn theme_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.shared_theme_colors_panel(cx)
    }
}
