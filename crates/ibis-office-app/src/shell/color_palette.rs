// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! カラーパレット: the colours a button's menu offers, as squares to pick from (as Office's
//! palette), not a list of colour names: the choice of "none" (自動, 塗りつぶしなし), the theme's
//! colours with their lighter and darker rows, the standard colours, the colours used lately,
//! and ("その他の色…") a window to type one when the format has it. A square's name is its tooltip.
//! Used in a toolbar button's menu and in a submenu of a menu, as a [`MenuEntry::Palette`].

use std::rc::Rc;

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::Rgb;

use crate::shell::chrome::MenuItem;
use crate::shell::editor::SlideEditor;

/// What picking a colour does (the colour, or `None` for "none").
pub(crate) type PickColor = Rc<dyn Fn(&mut SlideEditor, Option<Rgb>, &mut Window, &mut Context<SlideEditor>)>;

/// One square of the palette: its colour and the menu item it is (its id, its name as the tooltip, the
/// colour now in use as checked, what picking it does).
#[derive(Clone)]
pub(crate) struct Swatch {
    pub rgb: Rgb,
    pub item: MenuItem,
}

impl Swatch {
    /// A square that picks `rgb` through `pick`; `id` is its item's.
    pub(crate) fn with_id(id: &'static str, rgb: Rgb, label: impl Into<SharedString>, checked: bool, pick: &PickColor) -> Self {
        let pick = pick.clone();
        Self { rgb, item: MenuItem::new(id, label).checked(checked).on(move |e, w, cx| pick(e, Some(rgb), w, cx)) }
    }

    pub(crate) fn new(rgb: Rgb, label: impl Into<SharedString>, checked: bool, pick: &PickColor) -> Self {
        Self::with_id("color.swatch", rgb, label, checked, pick)
    }
}

/// The palette of a button or submenu.
#[derive(Clone)]
pub(crate) struct Palette {
    pub current:Option<Rgb>,
    /// "None" (自動, 塗りつぶしなし…) first, as an item of its own.
    pub none: Option<MenuItem>,
    /// The theme's colours: the first row the colours themselves, the rows after them lighter and darker.
    pub theme: Vec<Vec<Swatch>>,
    /// The standard colours, a row.
    pub standard: Vec<Swatch>,
    /// What picking a colour from the lately used ones does (none: no such row).
    pub pick: Option<PickColor>,
    /// "その他の色…", when the format has a window to type a colour.
    pub more: Option<MenuItem>,
    /// The toolbar button this is the palette of (its last colour is noted when a square is picked).
    pub button: Option<&'static str>,
}

static ANY_RECENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const SQUARE: f32 = 18.0;
const GAP: f32 = 3.0;
// Ten standard swatches per row, plus the popup and item insets.
pub(crate) const PALETTE_WIDTH:f32=10.*SQUARE+9.*GAP+2.*(8.+4.+1.);

fn name_of(rgb: Rgb) -> SharedString {
    format!("#{:06X}", rgb.0).into()
}

impl SlideEditor {
    /// A colour was picked: it is the first of those used lately.
    pub(crate) fn note_recent_color(&mut self, rgb: Rgb) {
        ANY_RECENT.store(true, std::sync::atomic::Ordering::Relaxed);
        self.recent_colors.retain(|c| *c != rgb);
        self.recent_colors.insert(0, rgb);
        self.recent_colors.truncate(10);
    }
}

fn row(swatches: Vec<Swatch>, editor: &WeakEntity<SlideEditor>, id: &'static str, _button: Option<&'static str>) -> impl IntoElement {
    h_flex().id(id).w_full().min_w_0().flex_wrap().gap(px(GAP)).children(swatches.into_iter().enumerate().map(|(i, s)| {
        let (editor, handler, rgb, label) = (editor.clone(), s.item.handler.clone(), s.rgb, s.item.label.clone());
        div().relative()
            .id(ElementId::Name(format!("{id}-{i}").into()))
            .flex_none()
            .w(px(SQUARE))
            .h(px(SQUARE))
            .rounded(px(2.))
            .bg(Hsla::from(gpui_kit::rgb(s.rgb.0)))
            .border(px(if s.item.checked { 2.0 } else { 1.0 }))
            .border_color(if s.item.checked { gpui_kit::rgb(0x0b57d0) } else { gpui_kit::rgb(0xc7c7c7) })
            .cursor_pointer()
            .hover(|d| d.border_color(gpui_kit::rgb(0x5f6368)))
            .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
            .on_click(move |_, window, cx| {
                let Some(handler) = handler.clone() else { return };
                editor
                    .update(cx, |this, cx| {
                        this.note_recent_color(rgb);
                        handler(this, window, cx)
                    })
                    .ok();
            })
            .child(canvas(|_,_,_|(),move|bounds,_,_,_|{
                #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
                crate::shell::scenario::button_probe::record_bounds(&format!("{id}-{i}"),bounds);
            }).absolute().inset_0())
    }))
}

/// A section's heading and its rows, as one element of the menu.
fn section(title: Option<SharedString>, rows: Vec<AnyElement>) -> impl IntoElement {
    v_flex()
        .gap(px(GAP))
        .py(px(2.))
        .when_some(title, |d, t| d.child(div().text_size(px(11.)).text_color(crate::shell::chrome::text_secondary()).child(t)))
        .children(rows)
}

/// `palette` into `menu`, for the editor `editor`.
pub(crate) fn fill_palette(mut menu: gpui_kit::component::menu::PopupMenu, palette: &Palette, editor: &WeakEntity<SlideEditor>, _cx: &mut Context<gpui_kit::component::menu::PopupMenu>) -> gpui_kit::component::menu::PopupMenu {
    use gpui_kit::component::menu::PopupMenuItem;
    menu=menu.min_w(px(PALETTE_WIDTH)).max_w(px(PALETTE_WIDTH));
    let t = crate::shell::i18n::text;
    let button = palette.button;
    // 1. None.
    if let Some(none) = &palette.none {
        let row = crate::shell::chrome::menu_item_row(none, editor);
        menu = menu.item(row);
    }
    // 2. The theme's colours.
    if !palette.theme.is_empty() {
        let title = t("color.palette.theme");
        let swatches = palette.theme.clone();
        let editor = editor.clone();
        menu = menu.item(PopupMenuItem::element(move |_, _| {
            let rows: Vec<AnyElement> = swatches.iter().enumerate().map(|(i, r)| row(r.clone(), &editor, theme_row_id(i), button).into_any_element()).collect();
            section(Some(title.clone()), rows)
        }).hoverable(false));
    }
    // 3. The standard colours.
    if !palette.standard.is_empty() {
        let (swatches, editor, title) = (palette.standard.clone(), editor.clone(), t("color.palette.standard"));
        menu = menu.item(PopupMenuItem::element(move |_, _| section(Some(title.clone()), vec![row(swatches.clone(), &editor, "palette-standard", button).into_any_element()])).hoverable(false));
    }
    // 4. Those used lately.
    // (The editor may be being updated now: whether some window has picked a colour is noted aside.)
    let any_recent = ANY_RECENT.load(std::sync::atomic::Ordering::Relaxed);
    if let (true, Some(pick)) = (any_recent, palette.pick.clone()) {
        let (editor, title) = (editor.clone(), t("color.palette.recent"));
        menu = menu.item(PopupMenuItem::element(move |_, cx| {
            let recent = editor.upgrade().map(|e| e.read(cx).recent_colors.clone()).unwrap_or_default();
            if recent.is_empty() {
                return div().into_any_element();
            }
            let swatches: Vec<Swatch> = recent.into_iter().map(|c| Swatch::new(c, name_of(c), false, &pick)).collect();
            section(Some(title.clone()), vec![row(swatches, &editor, "palette-recent", button).into_any_element()]).into_any_element()
        }).hoverable(false));
    }
    // 5. More colours.
    if let Some(pick)=palette.pick.clone() {
        let color=palette.current.or_else(||palette.theme.iter().flatten().chain(palette.standard.iter()).find(|s|s.item.checked).map(|s|s.rgb)).unwrap_or(Rgb(0));
        let more=crate::shell::chrome::MenuItem::tr("color.palette.more").on(move|e,w,cx|e.open_color_picker_dialog(color,pick.clone(),w,cx));
        menu=menu.separator().item(crate::shell::chrome::menu_item_row(&more,editor));
    } else if let Some(more)=&palette.more {
        menu=menu.separator().item(crate::shell::chrome::menu_item_row(more,editor));
    }
    menu
}

fn theme_row_id(i: usize) -> &'static str {
    ["palette-theme-0", "palette-theme-1", "palette-theme-2", "palette-theme-3", "palette-theme-4", "palette-theme-5", "palette-theme-6"][i.min(6)]
}

/// The tints (millionths; plus lighter, minus darker) of each of the ten theme colours' rows, in the
/// order background 1, text 1, background 2, text 2, accent 1 to 6: the first column is the colour.
pub(crate) const THEME_TINTS: [[i32; 6]; 10] = [
    [0, -50_000, -150_000, -250_000, -350_000, -500_000],
    [0, 500_000, 350_000, 250_000, 150_000, 50_000],
    [0, -100_000, -250_000, -500_000, -750_000, -900_000],
    [0, 900_000, 750_000, 500_000, 250_000, 100_000],
    [0, 800_000, 600_000, 400_000, -250_000, -500_000],
    [0, 800_000, 600_000, 400_000, -250_000, -500_000],
    [0, 800_000, 600_000, 400_000, -250_000, -500_000],
    [0, 800_000, 600_000, 400_000, -250_000, -500_000],
    [0, 800_000, 600_000, 400_000, -250_000, -500_000],
    [0, 800_000, 600_000, 400_000, -250_000, -500_000],
];

/// The ids of the theme colours' names (the tables of the UI's words), in that order.
const THEME_NAME_IDS: [&str; 10] = ["color.theme.bg1", "color.theme.text1", "color.theme.bg2", "color.theme.text2", "color.theme.accent1", "color.theme.accent2", "color.theme.accent3", "color.theme.accent4", "color.theme.accent5", "color.theme.accent6"];

/// The palette's rows of the theme's `colors` (background 1, text 1, background 2, text 2, accents 1 to 6):
/// row 0 the colours, the next rows each colour with its tint. `current` is marked.
pub(crate) fn theme_rows(colors: &[Rgb; 10], pick: &PickColor, current: Option<Rgb>) -> Vec<Vec<Swatch>> {
    (0..6)
        .map(|row| {
            (0..10)
                .map(|column| {
                    let tint = THEME_TINTS[column][row];
                    let rgb = ibis_pptx::tinted(colors[column], tint);
                    let name = crate::shell::i18n::text(THEME_NAME_IDS[column]);
                    let label = match tint {
                        0 => name.to_string(),
                        t if t > 0 => crate::shell::i18n::format("color.theme.lighter", &[("name", &name), ("n", &(t / 10_000))]),
                        t => crate::shell::i18n::format("color.theme.darker", &[("name", &name), ("n", &(-t / 10_000))]),
                    };
                    Swatch::new(rgb, label, current == Some(rgb), pick)
                })
                .collect()
        })
        .collect()
}

/// Mark the square of `current` in the palettes of `entries`.
pub(crate) fn mark_current(mut entries: Vec<crate::shell::chrome::MenuEntry>, current: Rgb) -> Vec<crate::shell::chrome::MenuEntry> {
    for entry in &mut entries {
        if let crate::shell::chrome::MenuEntry::Palette(p) = entry {
            for swatch in p.theme.iter_mut().flatten().chain(p.standard.iter_mut()) {
                swatch.item.checked = swatch.rgb == current;
            }
        }
    }
    entries
}

impl Palette {
    /// The item with id `id` or label `id`: "none", "more", or a square (for the scenarios and `Menus`).
    pub(crate) fn find_mut(&mut self, id: &str) -> Option<&mut MenuItem> {
        let hit = |i: &MenuItem| i.id == id || i.label.as_ref() == id;
        if self.none.as_ref().is_some_and(hit) {
            return self.none.as_mut();
        }
        if self.more.as_ref().is_some_and(hit) {
            return self.more.as_mut();
        }
        self.theme.iter_mut().flatten().chain(self.standard.iter_mut()).map(|s| &mut s.item).find(|i| i.id == id || i.label.as_ref() == id)
    }

    /// The same, to read.
    #[allow(dead_code)]
    pub(crate) fn find(&self, id: &str) -> Option<&MenuItem> {
        let hit = |i: &&MenuItem| i.id == id || i.label.as_ref() == id;
        self.none.iter().chain(self.more.iter()).find(hit).or_else(|| self.theme.iter().flatten().chain(self.standard.iter()).map(|s| &s.item).find(hit))
    }
}

/// A palette of the `colors` (name, colour, id of the item) as its standard row, over the theme's rows when
/// `theme` has the ten colours; `none` first; `current` marked; picking calls `pick` (`None` for none).
pub(crate) fn simple_palette(none: Option<MenuItem>, theme: Option<&[Rgb; 10]>, colors: &[(SharedString, Rgb, &'static str)], current: Option<Rgb>, pick: PickColor) -> Vec<crate::shell::chrome::MenuEntry> {
    let theme = theme.map(|t| theme_rows(t, &pick, current)).unwrap_or_default();
    let standard = colors.iter().map(|(label, rgb, id)| Swatch::with_id(id, *rgb, label.clone(), current == Some(*rgb), &pick)).collect();
    vec![crate::shell::chrome::MenuEntry::Palette(Palette { none, theme, standard, pick: Some(pick), more: None, button: None, current })]
}
