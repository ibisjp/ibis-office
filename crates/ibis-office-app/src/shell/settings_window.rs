// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ツール > 設定… (⌘,): the settings window, master–detail as the system's
//! settings are: the categories on the left (一般 for now), the selected
//! one's settings on the right. 一般 > 言語 chooses the UI's language (the
//! system's, English or Japanese); every window shows it at once.
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::scroll::ScrollbarMode;
use gpui_kit::component::theme::Theme;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Root, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::shell::i18n::{self, Language, LanguageSetting};

pub(crate) struct SettingsWindow {
    /// The category shown (0: 一般).
    category: usize,
    focus: FocusHandle,
}

thread_local! {
    /// The settings window, while it is open (one at a time).
    static OPEN: std::cell::RefCell<Option<WindowHandle<Root>>> = const { std::cell::RefCell::new(None) };
}

/// Open the settings window, or bring it to the front.
pub(crate) fn open(cx: &mut App) {
    let parent = cx.active_window().and_then(|w| w.update(cx, |_, window, cx| (window.bounds(), window.display(cx).map(|d| d.id()))).ok());
    open_over(parent.map(|p| p.0), parent.and_then(|p| p.1), cx);
}

/// Open the settings window in the middle of `parent` (the window the user
/// is in, on the same display: a window's bounds are relative to its display,
/// so the display is given too), or bring it to the front.
pub(crate) fn open_over(parent: Option<Bounds<Pixels>>, display: Option<DisplayId>, cx: &mut App) {
    open_at(0, parent, display, cx)
}

/// As `open_over`, with the category `category` shown (when the window is not open yet).
pub(crate) fn open_at(category: usize, parent: Option<Bounds<Pixels>>, display: Option<DisplayId>, cx: &mut App) {
    if let Some(handle) = OPEN.with(|o| *o.borrow())
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    // In the middle of the window the user is in.
    let (width, height) = (px(640.), px(420.));
    let bounds = match parent {
        Some(p) => Bounds::new(point(p.origin.x + (p.size.width - width) / 2.0, p.origin.y + (p.size.height - height) / 2.0), size(width, height)),
        None => Bounds::centered(None, size(width, height), cx),
    };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        display_id: display,
        titlebar: Some(TitlebarOptions { title: Some(i18n::text("settings.title")), ..Default::default() }),
        ..Default::default()
    };
    let opened = cx.open_window(options, |window, cx| {
        crate::shell::palette::watch(window, cx);
        let view = cx.new(|cx| SettingsWindow { category, focus: cx.focus_handle() });
        let focus = view.read(cx).focus.clone();
        focus.focus(window, cx);
        cx.new(|cx| Root::new(view, window, cx))
    });
    if let Ok(handle) = opened {
        OPEN.with(|o| *o.borrow_mut() = Some(handle));
    }
}

/// The settings window, while it is open.
#[allow(dead_code)]
pub(crate) fn handle() -> Option<WindowHandle<Root>> {
    OPEN.with(|o| *o.borrow())
}

/// Whether the settings window is open.
#[allow(dead_code)]
pub(crate) fn is_open() -> bool {
    OPEN.with(|o| o.borrow().is_some())
}

/// Choose the UI's language: saved, and every window drawn again in it (the
/// app's own menu bar too).
pub(crate) fn choose_language(setting: LanguageSetting, cx: &mut App) {
    i18n::set_setting(setting);
    crate::set_app_menus(cx);
    cx.refresh_windows();
}

/// The name of a choice of language, in the UI's language.
fn choice_label(setting: LanguageSetting) -> SharedString {
    match setting {
        LanguageSetting::System => i18n::text("settings.language.system"),
        // A language's own name, as systems list them.
        LanguageSetting::Fixed(Language::English) => "English".into(),
        LanguageSetting::Fixed(Language::Japanese) => "日本語".into(), // i18n-ok（言語の自称（母語での表記）。English と並べて自称のまま。迷った：母語表記）
    }
}

/// The key of 起動時のリンクの確認 (the warning bar of a workbook that links others): off when `0`.
pub(crate) const LINKS_WARNING_KEY: &str = "trust.links-warning";

/// Whether a workbook with links to others shows the warning bar when it is opened.
pub(crate) fn links_warning_enabled() -> bool {
    crate::shell::settings::get(LINKS_WARNING_KEY).as_deref() != Some("0")
}

/// The key of スクロール バーを常に表示: on when `1` (on by default; an explicitly saved 0 still means hover).
pub(crate) const SCROLLBARS_ALWAYS_KEY: &str = "view.scrollbars-always";

/// Whether the scrollbars of the windows are always shown.
pub(crate) fn scrollbars_always() -> bool {
    crate::shell::settings::get(SCROLLBARS_ALWAYS_KEY).as_deref() != Some("0")
}

/// The scrollbars' mode of every window as the setting says (at start, and when it is changed).
pub(crate) fn apply_scrollbar_mode(cx: &mut App) {
    apply_scrollbar_mode_only(cx);
    cx.refresh_windows();
}

/// As `apply_scrollbar_mode`, without redrawing (the theme's change redraws).
pub(crate) fn apply_scrollbar_mode_only(cx: &mut App) {
    // Hidden until the view scrolls or the pointer is over one (not the system's setting, which may show them always), unless asked to stay.
    Theme::set_scrollbar_mode(if scrollbars_always() { ScrollbarMode::Always } else { ScrollbarMode::Hover }, cx);
}

const CATEGORIES: [&str; 4] = ["settings.general", "settings.save", "settings.trust", "settings.qat"];

fn check_row(id: impl Into<ElementId>, label: SharedString, on: bool, action: impl Fn(bool, &mut App) + 'static) -> Stateful<Div> {
    crate::xlsx::toggle::toggle_row(id, on, label, move |_, _, cx| action(!on, cx))
}

impl SettingsWindow {
    fn general(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let current = i18n::setting();
        let choices = [LanguageSetting::System, LanguageSetting::Fixed(Language::English), LanguageSetting::Fixed(Language::Japanese)];
        let language = h_flex()
            .gap_3()
            .items_center()
            .child(div().w(px(80.)).child(i18n::text("settings.language")))
            .child(crate::shell::chrome::Dropdown::application("settings-language",choice_label(current),
                choices.into_iter().map(|choice|crate::shell::chrome::DropdownItem::new(choice_label(choice)).checked(choice==current)
                    .on_app_select(move|_,cx|choose_language(choice,cx))).collect()));
        let detail = v_flex().gap_3().child(language).child(div().text_sm().text_color(theme.muted_foreground).child(i18n::text("settings.language.note")));
        // スクロール バーを常に表示.
        let detail = detail.child(div().mt_3().child(Checkbox::new("settings-scrollbars-always").label(i18n::text("settings.view.scrollbars-always")).checked(scrollbars_always()).on_click(cx.listener(|_, checked: &bool, _, cx| {
            crate::shell::settings::set(SCROLLBARS_ALWAYS_KEY, if *checked { "1" } else { "0" });
            apply_scrollbar_mode(cx);
            cx.notify();
        }))));
        // 起動時に更新を確かめる (no update in the browser).
        #[cfg(not(target_family = "wasm"))]
        let detail = detail
            .child(
                div().mt_3().child(
                    Checkbox::new("settings-check-updates")
                        .label(i18n::text("fix.update.setting"))
                        .checked(crate::shell::update::check_enabled())
                        .on_click(cx.listener(|_, checked: &bool, _, cx| {
                            crate::shell::update::set_check_enabled(*checked);
                            cx.notify();
                        })),
                ),
            )
            .child(div().text_sm().text_color(theme.muted_foreground).child(i18n::text("fix.update.setting.note")));
        detail
    }

    /// 保存: AutoRecover, and the backup copy of a workbook saved over.
    fn saving(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        #[cfg(not(target_family = "wasm"))]
        {
            use crate::shell::autorecover::{load_autorecover_config, save_autorecover_config};
            let config = load_autorecover_config();
            let backup = crate::xlsx::backup_save::load_backup_save_config();
            let entity = cx.entity();
            let refresh = move |cx: &mut App| entity.update(cx, |_, cx| cx.notify());
            let (r1, r2, r3, r_minus, r_plus) = (refresh.clone(), refresh.clone(), refresh.clone(), refresh.clone(), refresh);
            let minutes = config.interval_minutes;
            return v_flex()
                .gap_2()
                .child(check_row("settings-autorecover", i18n::text("settings.save.autorecover"), config.enabled, move |on, cx| {
                    let mut c = load_autorecover_config();
                    c.enabled = on;
                    save_autorecover_config(&c);
                    r1(cx);
                }))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .pl(px(26.))
                        .child(div().text_sm().child(i18n::text("settings.save.interval")))
                        .child(Button::new("settings-interval-minus").small().label("−").on_click(move |_, _, cx| {
                            let mut c = load_autorecover_config();
                            c.interval_minutes = c.interval_minutes.saturating_sub(1).max(1);
                            save_autorecover_config(&c);
                            r_minus(cx);
                        }))
                        .child(div().w(px(36.)).text_center().child(SharedString::from(minutes.to_string())))
                        .child(Button::new("settings-interval-plus").small().label("+").on_click(move |_, _, cx| {
                            let mut c = load_autorecover_config();
                            c.interval_minutes = (c.interval_minutes + 1).min(120);
                            save_autorecover_config(&c);
                            r_plus(cx);
                        }))
                        .child(div().text_sm().child(i18n::text("settings.save.minutes"))),
                )
                .child(check_row("settings-keep-unsaved", i18n::text("settings.save.keep-unsaved"), config.keep_last_unsaved, move |on, cx| {
                    let mut c = load_autorecover_config();
                    c.keep_last_unsaved = on;
                    save_autorecover_config(&c);
                    r2(cx);
                }))
                .child(check_row("settings-backup", i18n::text("settings.save.backup"), backup.always_create_backup, move |on, cx| {
                    let mut c = crate::xlsx::backup_save::load_backup_save_config();
                    c.always_create_backup = on;
                    let _ = crate::xlsx::backup_save::save_backup_save_config(&c);
                    r3(cx);
                }))
                .child(div().text_sm().text_color(theme.muted_foreground).child(i18n::text("settings.save.note")));
        }
        #[cfg(target_family = "wasm")]
        v_flex().gap_2().child(div().text_sm().text_color(theme.muted_foreground).child(i18n::text("settings.save.web")))
    }

    /// セキュリティ センター.
    fn trust(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let entity = cx.entity();
        v_flex()
            .gap_2()
            .child(check_row("settings-links-warning", i18n::text("settings.trust.links-warning"), links_warning_enabled(), move |on, cx| {
                crate::shell::settings::set(LINKS_WARNING_KEY, if on { "1" } else { "0" });
                entity.update(cx, |_, cx| cx.notify());
            }))
            .child(div().text_sm().text_color(theme.muted_foreground).child(i18n::text("settings.trust.links-note")))
            .child(div().mt_3().font_weight(FontWeight::BOLD).child(i18n::text("settings.trust.macros")))
            .child(div().text_sm().text_color(theme.muted_foreground).child(i18n::text("settings.trust.macros-note")))
    }

    /// クイック アクセス ツール バー: the commands of the title row.
    fn quick_access(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let chosen = crate::shell::qat::commands();
        let entity = cx.entity();
        let mut list = v_flex().gap_0();
        for id in crate::shell::qat::CANDIDATES {
            let on = chosen.iter().any(|c| c == id);
            let id: &'static str = id;
            let entity_cb = entity.clone();
            list = list.child(check_row(SharedString::from(format!("settings-qat-{id}")), i18n::text(id), on, move |_, cx| {
                crate::shell::qat::toggle(id);
                cx.refresh_windows();
                entity_cb.update(cx, |_, cx| cx.notify());
            }));
        }

        let mut order_list = v_flex().gap_1();
        if !chosen.is_empty() {
            let total = chosen.len();
            for (idx, id) in chosen.iter().enumerate() {
                let id_clone = id.clone();
                let id_up = id.clone();
                let id_down = id.clone();
                let e_up = entity.clone();
                let e_down = entity.clone();
                let e_del = entity.clone();
                let can_up = idx > 0;
                let can_down = idx + 1 < total;

                let row = h_flex()
                    .id(SharedString::from(format!("settings-qat-order-{id}")))
                    .p_1()
                    .items_center()
                    .justify_between()
                    .rounded_md()
                    .hover(|s| s.bg(theme.accent.opacity(0.1)))
                    .child(div().text_sm().child(format!("{}. {}", idx + 1, i18n::text(&id_clone))))
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child({
                                let mut b = Button::new(SharedString::from(format!("settings-qat-up-{id}")))
                                    .small()
                                    .label("↑");
                                if can_up {
                                    b = b.on_click(move |_, _, cx| {
                                        crate::shell::qat::move_up(&id_up);
                                        cx.refresh_windows();
                                        e_up.update(cx, |_, cx| cx.notify());
                                    });
                                }
                                b
                            })
                            .child({
                                let mut b = Button::new(SharedString::from(format!("settings-qat-down-{id}")))
                                    .small()
                                    .label("↓");
                                if can_down {
                                    b = b.on_click(move |_, _, cx| {
                                        crate::shell::qat::move_down(&id_down);
                                        cx.refresh_windows();
                                        e_down.update(cx, |_, cx| cx.notify());
                                    });
                                }
                                b
                            })
                            .child(
                                Button::new(SharedString::from(format!("settings-qat-del-{id}")))
                                    .small()
                                    .label("✕")
                                    .on_click(move |_, _, cx| {
                                        crate::shell::qat::toggle(&id_clone);
                                        cx.refresh_windows();
                                        e_del.update(cx, |_, cx| cx.notify());
                                    }),
                            ),
                    );
                order_list = order_list.child(row);
            }
        }

        let e_reset = entity.clone();
        let reset_btn = crate::shell::chrome::action_button("settings-qat-reset")
            .small()
            .label(i18n::text("settings.qat.reset"))
            .on_click(move |_, _, cx| {
                crate::shell::qat::reset();
                cx.refresh_windows();
                e_reset.update(cx, |_, cx| cx.notify());
            });

        let mut res = v_flex()
            .gap_2()
            .child(div().text_sm().text_color(theme.muted_foreground).child(i18n::text("settings.qat.note")));

        if !chosen.is_empty() {
            res = res.child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .bg(theme.muted.opacity(0.3))
                    .rounded_md()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(div().font_weight(FontWeight::BOLD).text_sm().child(i18n::text("settings.qat.order")))
                            .child(reset_btn),
                    )
                    .child(div().id("settings-qat-order-list").max_h(px(140.)).overflow_y_scroll().child(order_list)),
            );
        }

        res.child(div().mt_2().font_weight(FontWeight::BOLD).text_sm().child(i18n::text("settings.qat.candidates")))
            .child(div().id("settings-qat-list").max_h(px(160.)).overflow_y_scroll().child(list))
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(&i18n::text("settings.title"));
        let theme = cx.theme().clone();
        let categories: Vec<SharedString> = CATEGORIES.iter().map(|k| i18n::text(k)).collect();
        let list = v_flex().w(px(180.)).h_full().p_2().gap_1().justify_start().items_stretch().bg(theme.sidebar).border_r_1().border_color(theme.border).children(categories.iter().enumerate().map(|(i, name)| {
            let selected = i == self.category;
            div()
                .id(("settings-category", i))
                .px_2()
                .py_1()
                .rounded_md()
                .when(selected, |d| d.bg(theme.accent))
                .child(name.clone())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.category = i;
                    cx.notify();
                }))
        }));
        let category = self.category.min(CATEGORIES.len() - 1);
        let body = match category {
            0 => self.general(cx),
            1 => self.saving(cx),
            2 => self.trust(cx),
            _ => self.quick_access(cx),
        };
        let detail = v_flex().flex_1().min_w_0().p_4().gap_3().child(div().text_lg().child(categories[category].clone())).child(body);
        h_flex().id("settings").track_focus(&self.focus).size_full().items_start().bg(theme.background).text_color(theme.foreground).child(list).child(detail)
    }
}
