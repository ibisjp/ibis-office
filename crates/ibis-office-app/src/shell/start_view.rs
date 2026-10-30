// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The start view (スタート画面), shown while no file is open (at launch,
//! and after ファイル > 閉じる): 新規作成 (an empty document, workbook or
//! presentation), 選択 (開く…) and, on the desktop, the recently opened
//! files.

use gpui_kit::component::ActiveTheme as _;
#[cfg(not(target_family = "wasm"))]
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;

use crate::OpenFile;
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;
use gpui_kit::component::Icon;

/// The colour of the links (開く…), as Office's start screen.
const LINK_COLOR: u32 = 0x0f6cbd;
/// Width of the column of the start view, in pixels.
const COLUMN_WIDTH: f32 = 560.0;
/// Height of one row of the recent files list, in pixels.
#[cfg(not(target_family = "wasm"))]
const RECENT_ROW_HEIGHT: f32 = 44.0;
/// Space left under the recent files list, to the bottom of the window, in
/// pixels; also the space above each section's title.
const BOTTOM_MARGIN: f32 = 24.0;
/// The padding around a page of 新規作成 (where its hover shows), in pixels.
const TILE_PADDING: f32 = 8.0;
/// Space between a section's title and what it shows (開く…, the list), in
/// pixels: the pages' padding, which is all the space between 新規作成 and
/// its pages (their hover reaches the title), so that every title is as far
/// from what it shows.
const TITLE_GAP: f32 = TILE_PADDING;
/// The empty edge of a 20-pixel Material icon (開く…'s folder), in pixels.
const ICON_EDGE: f32 = 3.0;

/// A section's title (新規作成, 選択, 最近開いたファイル), `above` pixels
/// below what is over it and `below` pixels above what it shows; its line as tall as its letters, so that the
/// spaces are those of the letters.
fn section_title(key: &str, above: f32, below: f32) -> Div {
    div()
        .mt(px(above))
        .mb(px(below))
        .text_sm()
        .line_height(px(14.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(crate::shell::i18n::text(key))
}
/// The kinds 新規作成 makes, with the label and the colour of their badge.
const NEW_KINDS: [(ibis_pptx::Kind, &str, u32); 3] = [
    (ibis_pptx::Kind::Document, "docx", 0x2f6fd6),
    (ibis_pptx::Kind::Workbook, "xlsx", 0x1e8e3e),
    (ibis_pptx::Kind::Presentation, "pptx", 0xe8710a),
];
/// A page of 新規作成, in pixels.
const PAGE_WIDTH: f32 = 52.0;
const PAGE_HEIGHT: f32 = 66.0;
/// How far a page's badge reaches out left of the page, in pixels.
const BADGE_OVERHANG: f32 = 6.0;
/// The column's padding on each side (`px_8`), in pixels: also the space
/// between the recent files list and the 協力のお願い panel, and right of it.
const COLUMN_PADDING: f32 = 32.0;
/// The 協力のお願い panel right of the column: as wide as the recent files
/// list, and as narrow as this when the window is (its text wraps), in pixels.
const REQUEST_WIDTH: f32 = COLUMN_WIDTH - 2.0 * COLUMN_PADDING;
const REQUEST_MIN_WIDTH: f32 = 220.0;
/// The narrowest the column gets beside the panel, in pixels.
const COLUMN_MIN_WIDTH: f32 = 320.0;
/// The size of the panel's letters, in pixels: 1.2 times `text_sm`'s.
const REQUEST_TEXT_SIZE: f32 = 14.0 * 1.2;
/// The width a start window opens at: the column, the panel and the space right of it.
pub(crate) const START_WINDOW_WIDTH: f32 = COLUMN_WIDTH + REQUEST_WIDTH + COLUMN_PADDING;
/// Where 協力のお願い asks for reports.
const ISSUES_URL: &str = "https://github.com/ibisjp/ibis-office/issues";

/// The window's title while no file is open: the app and its version.
pub(crate) fn start_title() -> String {
    format!("ibisOffice {}", env!("IBIS_VERSION"))
}

/// 協力のお願い, right of the column: what ibisOffice aims at, and a link to
/// GitHub's issues (opened in the default browser) for reports.
fn request_panel(muted: Hsla) -> Div {
    // The link's words go where the text has {link}.
    let report = crate::shell::i18n::text("start.request-report");
    let link = crate::shell::i18n::text("start.request-link");
    let (before, after) = report.split_once("{link}").unwrap_or((&report, ""));
    let range = before.len()..before.len() + link.len();
    let link_style = HighlightStyle {
        color: Some(rgb(LINK_COLOR).into()),
        underline: Some(UnderlineStyle { thickness: px(1.), color: Some(rgb(LINK_COLOR).into()), wavy: false }),
        ..Default::default()
    };
    let report = InteractiveText::new("start-request-report", StyledText::new(format!("{before}{link}{after}")).with_highlights([(range.clone(), link_style)]))
        .on_click(vec![range], |_, _, cx| cx.open_url(ISSUES_URL));
    v_flex()
        .w(px(REQUEST_WIDTH))
        .min_w(px(REQUEST_MIN_WIDTH))
        .flex_shrink(1.0)
        .mt(px(BOTTOM_MARGIN))
        .mr(px(COLUMN_PADDING))
        .p_4()
        .gap_2()
        .rounded(px(8.))
        .bg(rgb(0xf0f4f9))
        .text_size(px(REQUEST_TEXT_SIZE))
        .child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("start.request")))
        .child(div().text_color(muted).child(crate::shell::i18n::text("start.request-body")))
        .child(div().text_color(muted).child(report))
}

impl SlideEditor {
    /// The white page under the title bar while no file is open.
    pub(crate) fn render_start_view(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let open = div()
            .id("start-open")
            .ml_4()
            .mt(px(-ICON_EDGE))
            .flex()
            .items_center()
            .gap_2()
            .text_color(rgb(LINK_COLOR))
            .text_base()
            .line_height(px(20.))
            .cursor_pointer()
            .hover(|s| s.underline())
            .child(Icon::new(Material("folder_open")).size(px(20.)).text_color(rgb(LINK_COLOR)))
            .child(crate::shell::i18n::text("start.open"))
            .on_click(cx.listener(|this, _, window, cx| this.open_file(&OpenFile, window, cx)));
        // The first badge's left edge under 新規作成's.
        let new_files = div().flex().gap_4().ml(px(BADGE_OVERHANG - TILE_PADDING)).children(NEW_KINDS.iter().map(|&(kind, label, color)| self.new_file_tile(kind, label, color, cx)));

        div()
            .id("start-view")
            .key_context(crate::shell::editor::WORKSPACE_CONTEXT)
            .track_focus(&self.focus_handle)
            .flex_1()
            .min_h_0()
            .w_full()
            .bg(rgb(0xffffff))
            .overflow_hidden()
            .flex()
            .items_start()
            .child(
                v_flex()
                    .w(px(COLUMN_WIDTH))
                    .min_w(px(COLUMN_MIN_WIDTH))
                    .flex_shrink(1.0)
                    .h_full()
                    .px_8()
                    .pb(px(BOTTOM_MARGIN))
                    .child(section_title("start.new", BOTTOM_MARGIN, 0.0))
                    .child(new_files)
                    // The pages' padding (their hover) is part of the space above 選択.
                    .child(section_title("start.select", BOTTOM_MARGIN - TILE_PADDING, TITLE_GAP))
                    .child(div().flex().child(open))
                    .map(|d| self.recent_files_list(d, cx))
                    .when_some(self.status.clone(), |d, status| d.child(div().mt_3().text_sm().text_color(muted).child(status))),
            )
            .child(v_flex().w(px(REQUEST_WIDTH)).min_w(px(REQUEST_MIN_WIDTH)).flex_shrink(1.).mr(px(COLUMN_PADDING)).mt(px(BOTTOM_MARGIN)).gap_4()
                .child(crate::shell::chrome::brand_logo("start-app-logo",REQUEST_WIDTH / crate::shell::chrome::LOGO_ASPECT,true))
                .child(request_panel(muted).w_full().mt_0().mr_0()))
    }

    /// One kind of 新規作成: a blank page with the format's badge on its
    /// lower left; a click makes an empty file of it.
    fn new_file_tile(&self, kind: ibis_pptx::Kind, label: &'static str, color: u32, cx: &mut Context<Self>) -> AnyElement {
        let page = div()
            .w(px(PAGE_WIDTH))
            .h(px(PAGE_HEIGHT))
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(rgb(0xc4c7c5))
            .rounded(px(3.));
        let badge = div()
            .absolute()
            .left(px(-BADGE_OVERHANG))
            .bottom(px(8.))
            .px(px(5.))
            .py(px(1.))
            .rounded(px(3.))
            .bg(rgb(color))
            .text_color(crate::shell::palette::doc(0xffffff))
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .child(label);
        div()
            .id(SharedString::from(format!("start-new-{label}")))
            .p(px(TILE_PADDING))
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0xf0f4f9)))
            .child(div().relative().child(page).child(badge))
            .on_click(cx.listener(move |this, _, window, cx| this.new_file(kind, window, cx)))
            .into_any_element()
    }

    #[cfg(not(target_family="wasm"))]
    pub(crate) fn recent_file_actions(&self,path:std::path::PathBuf)->Vec<crate::shell::chrome::MenuEntry>{
        vec![crate::shell::chrome::MenuItem::tr("start.recent.remove").icon("delete").on(move|e,_,cx|{
            e.recent_files=crate::shell::recent_files::remove(&path);cx.notify();
        }).into()]
    }

    /// 最近開いたファイル and the list of them; a file that is gone is greyed.
    #[cfg(not(target_family = "wasm"))]
    fn recent_files_list(&self, column: Div, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme();
        let (foreground, muted, border, hover, background) =
            (theme.foreground, theme.muted_foreground, theme.border, theme.muted, theme.background);
        let rows: Vec<AnyElement> = self
            .recent_files
            .iter()
            .enumerate()
            .map(|(index, path)| {
                let name: SharedString = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned())
                    .into();
                let dir: SharedString = path.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default().into();
                let exists = path.is_file();
                let path = path.clone();
                let entries=self.recent_file_actions(path.clone());
                let context_entries=entries.clone();
                div()
                    .id(("recent-file", index))
                    .flex_shrink_0()
                    .h(px(RECENT_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .group("row-actions")
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(border)
                    .text_color(if exists { foreground } else { muted })
                    .on_mouse_down(MouseButton::Right,cx.listener(move|e,event:&MouseDownEvent,w,cx|{
                        cx.stop_propagation();e.open_hyperlink_context(event.position,context_entries.clone(),w,cx);
                    }))
                    .when(exists, |d| {
                        d.cursor_pointer()
                            .hover(move |s| s.bg(hover))
                            .on_click(cx.listener(move |this, _, window, cx| this.open_path(path.clone(), None, window, cx)))
                    })
                    .child(div().flex_1().min_w_0().child(div().text_sm().truncate().child(name)).child(div().text_xs().text_color(muted).truncate().child(dir)))
                    .child(crate::shell::chrome::hover_row_actions(index,entries,&cx.weak_entity()))
                    .into_any_element()
            })
            .collect();
        let empty = rows.is_empty();
        // The box reaches down to the bottom margin whatever the number of files,
        // and scrolls when the window is too short for them.
        column.child(section_title("start.recent", BOTTOM_MARGIN - ICON_EDGE, TITLE_GAP)).child(
            div()
                .relative()
                .w_full()
                .flex_1()
                .min_h_0()
                .border_1()
                .border_color(border)
                .rounded_sm()
                .bg(background)
                .overflow_hidden()
                .child(
                    div()
                        .id("recent-files-scroll")
                        .size_full()
                        .flex()
                        .flex_col()
                        .overflow_y_scroll()
                        .track_scroll(&self.recent_scroll)
                        .when(empty, |d| {
                            d.child(
                                div()
                                    .h(px(RECENT_ROW_HEIGHT))
                                    .px_3()
                                    .flex()
                                    .items_center()
                                    .text_sm()
                                    .text_color(muted)
                                    .child(crate::shell::i18n::text("start.recent-none")),
                            )
                        })
                        .children(rows),
                )
                .vertical_scrollbar(&self.recent_scroll),
        )
    }

    /// The browser gives no file paths to reopen, so the web build has no history.
    #[cfg(target_family = "wasm")]
    fn recent_files_list(&self, column: Div, _: &mut Context<Self>) -> Div {
        column
    }
}
