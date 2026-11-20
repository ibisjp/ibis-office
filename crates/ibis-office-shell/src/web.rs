// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The browser's side of the editor (the web build): its platform, its file pickers and the kept CMYK profile. The
//! entry point (`run`, exported through wasm-bindgen) is the app's.

use wasm_bindgen::prelude::*;

/// Whether the browser runs on a Mac (or an iPad, which reports itself as
/// one), where shortcuts use Command. Same test as gpui-pre-web's.
pub fn is_mac() -> bool {
    let navigator = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("navigator")).ok();
    let field = |name: &str| {
        navigator
            .as_ref()
            .and_then(|navigator| js_sys::Reflect::get(navigator, &JsValue::from_str(name)).ok())
            .and_then(|value| value.as_string())
            .unwrap_or_default()
    };
    field("platform").contains("Mac") || field("userAgent").contains("Mac")
}

#[wasm_bindgen]
extern "C" {
    /// Defined by `www/main.ts`: shows the browser's file picker and
    /// resolves to `{ name, bytes }`, or to `null` when cancelled.
    #[wasm_bindgen(catch, js_name = ibisOfficePickFile)]
    fn pick_file_js() -> Result<js_sys::Promise, JsValue>;
}

#[wasm_bindgen]
extern "C" {
    /// Defined by `www/main.ts`: the file picker for an ICC profile (`.icc`, `.icm`); `{ name, bytes }` or `null`.
    #[wasm_bindgen(catch, js_name = ibisOfficePickIcc)]
    fn pick_icc_js() -> Result<js_sys::Promise, JsValue>;
    /// Defined by `www/main.ts`: keeps the CMYK profile the user chose in the browser's IndexedDB (a setting).
    #[wasm_bindgen(js_name = ibisOfficeIccSave)]
    fn icc_save_js(name: &str, bytes: &[u8]);
    /// Defined by `www/main.ts`: forgets the kept profile.
    #[wasm_bindgen(js_name = ibisOfficeIccForget)]
    fn icc_forget_js();
    /// Defined by `www/main.ts`: resolves to the kept profile `{ name, bytes }`, or `null`.
    #[wasm_bindgen(catch, js_name = ibisOfficeIccLoad)]
    fn icc_load_js() -> Result<js_sys::Promise, JsValue>;
}

async fn named_bytes(promise: Result<js_sys::Promise, JsValue>) -> Result<Option<(String, Vec<u8>)>, String> {
    let describe = |err: JsValue| err.as_string().unwrap_or_else(|| format!("{err:?}"));
    let value = wasm_bindgen_futures::JsFuture::from(promise.map_err(describe)?).await.map_err(describe)?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    let field = |name: &str| js_sys::Reflect::get(&value, &JsValue::from_str(name)).map_err(describe);
    let name = field("name")?.as_string().unwrap_or_default();
    let bytes = js_sys::Uint8Array::new(&field("bytes")?).to_vec();
    Ok(Some((name, bytes)))
}

/// Let the user choose an ICC profile file; its name and contents, or `None` if cancelled.
pub(crate) async fn pick_icc_file() -> Result<Option<(String, Vec<u8>)>, String> {
    named_bytes(pick_icc_js()).await
}

/// Keep the CMYK profile `bytes` (a file called `name`) in the browser's IndexedDB.
pub(crate) fn icc_save(name: &str, bytes: &[u8]) {
    icc_save_js(name, bytes);
}

/// Forget the kept CMYK profile.
pub(crate) fn icc_forget() {
    icc_forget_js();
}

/// The kept CMYK profile (name and contents), if there is one.
pub(crate) async fn icc_load() -> Result<Option<(String, Vec<u8>)>, String> {
    named_bytes(icc_load_js()).await
}

/// Let the user choose a file; its name and contents, or `None` if cancelled.
pub async fn pick_file() -> Result<Option<(String, Vec<u8>)>, String> {
    let describe = |err: JsValue| err.as_string().unwrap_or_else(|| format!("{err:?}"));
    let promise = pick_file_js().map_err(describe)?;
    let value = wasm_bindgen_futures::JsFuture::from(promise).await.map_err(describe)?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    let field = |name: &str| js_sys::Reflect::get(&value, &JsValue::from_str(name)).map_err(describe);
    let name = field("name")?.as_string().unwrap_or_default();
    let bytes = js_sys::Uint8Array::new(&field("bytes")?).to_vec();
    Ok(Some((name, bytes)))
}

// ---- The downloadable fonts (セットアップ; shell/font_setup.rs) ------------------------------------------------------------

#[wasm_bindgen]
extern "C" {
    /// Defined by `www/main.ts`: the text at `url` (`fetch`), or a rejection with the reason.
    #[wasm_bindgen(catch, js_name = ibisOfficeFontFetchText)]
    fn font_fetch_text_js(url: &str) -> Result<js_sys::Promise, JsValue>;
    /// Defined by `www/main.ts`: the fonts kept in IndexedDB, as an array of `{ file, size }`.
    #[wasm_bindgen(catch, js_name = ibisOfficeFontsInstalled)]
    fn fonts_installed_js() -> Result<js_sys::Promise, JsValue>;
    /// Defined by `www/main.ts`: downloads `url` and keeps it in IndexedDB in 1 MiB chunks under the font file's name; resolves to the size.
    #[wasm_bindgen(catch, js_name = ibisOfficeFontDownload)]
    fn font_download_js(file: &str, url: &str, size: f64) -> Result<js_sys::Promise, JsValue>;
    /// Defined by `www/main.ts`: every font kept, put back together; an array of `Uint8Array`.
    #[wasm_bindgen(catch, js_name = ibisOfficeFontsLoad)]
    fn fonts_load_js() -> Result<js_sys::Promise, JsValue>;
    /// Defined by `www/main.ts`: the fonts the page's manifest has blocks of (a JSON text; empty when there are none or the host turned font downloads off).
    #[wasm_bindgen(js_name = ibisOfficeSubsetsInfo)]
    fn subsets_info_js() -> String;
    /// Defined by `www/main.ts`: the bytes of one block of a font (`Uint8Array`), checked and kept in IndexedDB by the page.
    #[wasm_bindgen(catch, js_name = ibisOfficeSubsetFetch)]
    fn subset_fetch_js(font_id: &str, key: &str) -> Result<js_sys::Promise, JsValue>;
    /// Defined by `www/main.ts`: the one font kept in IndexedDB under `file` (`Uint8Array`), or `null`.
    #[wasm_bindgen(catch, js_name = ibisOfficeFontLoadOne)]
    fn font_load_one_js(file: &str) -> Result<js_sys::Promise, JsValue>;
}

#[wasm_bindgen]
extern "C" {
    /// Defined by `www/main.ts`: the browser saves the bytes as a file (a Blob behind a temporary link).
    #[wasm_bindgen(js_name = ibisOfficeSaveFile)]
    fn save_file_js(name: &str, mime: &str, bytes: &[u8]);
}

/// The browser saves `bytes` as the file `name` of type `mime` (the browser version's way to export, as the desktop's save dialog).
pub fn download_bytes(name: &str, mime: &str, bytes: &[u8]) {
    save_file_js(name, mime, bytes);
}

fn describe(err: JsValue) -> String {
    err.as_string().or_else(|| js_sys::Reflect::get(&err, &JsValue::from_str("message")).ok().and_then(|m| m.as_string())).unwrap_or_else(|| format!("{err:?}"))
}

/// The text at `url`.
pub(crate) async fn font_fetch_text(url: &str) -> Result<String, String> {
    let value = wasm_bindgen_futures::JsFuture::from(font_fetch_text_js(url).map_err(describe)?).await.map_err(describe)?;
    value.as_string().ok_or_else(|| "no text".to_string())
}

/// The file names of the fonts kept in IndexedDB.
pub(crate) async fn fonts_installed() -> Result<Vec<String>, String> {
    let value = wasm_bindgen_futures::JsFuture::from(fonts_installed_js().map_err(describe)?).await.map_err(describe)?;
    let list = js_sys::Array::from(&value);
    Ok(list.iter().filter_map(|item| js_sys::Reflect::get(&item, &JsValue::from_str("file")).ok()?.as_string()).collect())
}

/// Download the font file at `url` into IndexedDB (in chunks), under `file`; `size` (0 when unknown) is checked.
pub(crate) async fn font_download(file: &str, url: &str, size: u64) -> Result<(), String> {
    wasm_bindgen_futures::JsFuture::from(font_download_js(file, url, size as f64).map_err(describe)?).await.map_err(describe)?;
    Ok(())
}

/// The fonts kept in IndexedDB with their sizes, the small ones first (an error is none).
async fn fonts_installed_by_size() -> Vec<String> {
    let Ok(promise) = fonts_installed_js() else { return Vec::new() };
    let Ok(value) = wasm_bindgen_futures::JsFuture::from(promise).await else { return Vec::new() };
    let mut list: Vec<(String, f64)> = js_sys::Array::from(&value)
        .iter()
        .filter_map(|item| {
            let file = js_sys::Reflect::get(&item, &JsValue::from_str("file")).ok()?.as_string()?;
            let size = js_sys::Reflect::get(&item, &JsValue::from_str("size")).ok().and_then(|s| s.as_f64()).unwrap_or(0.0);
            Some((file, size))
        })
        .collect();
    list.sort_by(|a, b| a.1.total_cmp(&b.1));
    list.into_iter().map(|(file, _)| file).collect()
}

/// GitHub issue #157: the fonts downloaded with セットアップ are read from IndexedDB in the background, one at a time, the small ones first
/// (IPAmj Mincho alone is 46 MB: nothing waits for it, the screen and the document are drawn at once with the viewer's own fonts), and each is
/// given to the text system as soon as it is read (`add_fonts_now`, as セットアップ does for a font just installed); the windows draw again once no other came for 100 ms (`fonts_late`), and
/// a document laid out before is laid out again (the layout caches follow `fonts_generation`). Nothing is read where IndexedDB is not usable
/// (the fonts are then none, as before).
pub fn load_downloaded_fonts_in_background(cx: &mut gpui_kit::App) {
    let arrivals = crate::fonts_late::Arrivals::default();
    cx.spawn(async move |cx: &mut gpui_kit::AsyncApp| {
        for file in fonts_installed_by_size().await {
            let Some(bytes) = font_load_one(&file).await else { continue };
            cx.update(|cx| {
                crate::fonts_late::fonts_arrived(cx, &arrivals, vec![bytes]);
            });
        }
    })
    .detach();
}

/// The font files kept in IndexedDB, all at once (an error is no fonts).
pub async fn load_downloaded_fonts() -> Vec<Vec<u8>> {
    let Ok(promise) = fonts_load_js() else { return Vec::new() };
    let Ok(value) = wasm_bindgen_futures::JsFuture::from(promise).await else { return Vec::new() };
    js_sys::Array::from(&value).iter().map(|item| js_sys::Uint8Array::new(&item).to_vec()).collect()
}

/// The fonts the page has blocks of (empty: none).
fn subsets_info() -> Vec<crate::render::subset_fonts::SubsetFont> {
    use crate::render::subset_fonts::{SubsetBlock, SubsetFont};
    let text = subsets_info_js();
    if text.is_empty() {
        return Vec::new();
    }
    let Ok(root) = js_sys::JSON::parse(&text) else { return Vec::new() };
    let get = |value: &JsValue, key: &str| js_sys::Reflect::get(value, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED);
    js_sys::Array::from(&get(&root, "fonts"))
        .iter()
        .map(|font| SubsetFont {
            id: get(&font, "id").as_string().unwrap_or_default(),
            typefaces: js_sys::Array::from(&get(&font, "typeface")).iter().filter_map(|t| t.as_string()).collect(),
            blocks: js_sys::Array::from(&get(&font, "blocks"))
                .iter()
                .map(|block| SubsetBlock {
                    key: get(&block, "key").as_string().unwrap_or_default(),
                    family: get(&block, "family").as_string().unwrap_or_default(),
                    ranges: js_sys::Array::from(&get(&block, "ranges")).iter().map(|r| (get(&r, "0").as_f64().unwrap_or(0.0) as u32, get(&r, "1").as_f64().unwrap_or(0.0) as u32)).collect(),
                    boot: get(&block, "boot").as_bool().unwrap_or(false),
                })
                .collect(),
        })
        .collect()
}

/// One block's bytes (the page checks and keeps it).
async fn subset_fetch(font_id: &str, key: &str) -> Result<Vec<u8>, String> {
    let value = wasm_bindgen_futures::JsFuture::from(subset_fetch_js(font_id, key).map_err(describe)?).await.map_err(describe)?;
    Ok(js_sys::Uint8Array::new(&value).to_vec())
}

/// The blocks of the fonts a document names (`typefaces`: what its `used_fonts` lists) that have a character of its text (`text`) and were not taken yet are taken now
/// (`subset_fonts::blocks_needed`); a screen that cannot give its text (`None`) takes all the blocks of such a font. The text that is typed or pasted later is followed by the editor (`follow_subset_chunks`).
pub fn request_subset_blocks_for(typefaces: Vec<String>, text: Option<String>, cx: &mut gpui_kit::App) {
    let fonts = crate::render::subset_fonts::fonts();
    let mut requests: Vec<(String, Vec<String>)> = Vec::new();
    for typeface in typefaces {
        let Some(id) = crate::render::subset_fonts::font_for_typeface(&typeface) else { continue };
        if requests.iter().any(|(f, _)| *f == id) {
            continue;
        }
        if let Some(font) = fonts.iter().find(|f| f.id == id) {
            let keys = match &text {
                Some(text) => crate::render::subset_fonts::blocks_needed(&id, text),
                None => font.blocks.iter().map(|b| b.key.clone()).collect(),
            };
            requests.push((id, keys));
        }
    }
    if !requests.is_empty() {
        load_subset_blocks(cx, requests, false);
    }
}

/// GitHub issue #157, stage 2: the page's blocks of fonts are taken (the base block of each font at the start; then, for the typefaces `typefaces` a document names, the
/// blocks that have the characters of its text: `texts` are the texts). Each block is given to the text system as it comes (`fonts_late::fonts_arrived`) and put in the
/// chain of its font (`subset_fonts::mark_loaded`); the windows draw again once. A block that cannot be had is left (its characters are drawn with the viewer's fonts).
pub fn load_subset_blocks(cx: &mut gpui_kit::App, requests: Vec<(String, Vec<String>)>, boot: bool) {
    let arrivals = crate::fonts_late::Arrivals::default();
    let mut wanted: Vec<(String, String)> = Vec::new();
    if boot {
        // The fonts are known at once (a document opened right after asks for the blocks of what it names), the blocks come behind.
        let fonts = subsets_info();
        if fonts.is_empty() {
            return;
        }
        crate::render::subset_fonts::set_fonts(fonts.clone());
        for font in &fonts {
            wanted.extend(font.blocks.iter().filter(|b| b.boot).map(|b| (font.id.clone(), b.key.clone())));
        }
    }
    cx.spawn(async move |cx: &mut gpui_kit::AsyncApp| {
        for (font_id, keys) in requests {
            wanted.extend(keys.into_iter().map(|k| (font_id.clone(), k)));
        }
        for (font_id, key) in wanted {
            if crate::render::subset_fonts::is_loaded(&font_id, &key) {
                continue;
            }
            let Ok(bytes) = subset_fetch(&font_id, &key).await else {
                crate::render::subset_fonts::mark_failed(&font_id, &key);
                continue;
            };
            cx.update(|cx| {
                // In the chain before the windows draw: the blocks of a font are marked as they are added.
                if crate::fonts_late::fonts_arrived(cx, &arrivals, vec![bytes]) {
                    crate::render::subset_fonts::mark_loaded(&font_id, &key);
                }
            });
        }
    })
    .detach();
}

/// The font kept in IndexedDB under `file`.
async fn font_load_one(file: &str) -> Option<Vec<u8>> {
    let value = wasm_bindgen_futures::JsFuture::from(font_load_one_js(file).ok()?).await.ok()?;
    if value.is_null() || value.is_undefined() {
        return None;
    }
    Some(js_sys::Uint8Array::new(&value).to_vec())
}

/// The font file of BIZ UDGothic or BIZ UDMincho (by `families`, see `font_setup::pdf_fallback_for`) for the PDF's characters the
/// viewer's PC has no font for, with the font's id: the copy セットアップ (or an earlier PDF) kept in IndexedDB, else downloaded now
/// from the font list and kept (a host that switched `features.fontDownload` off has no download). `None` when it cannot be had.
pub async fn pdf_fallback_font(families: &[String]) -> Option<(&'static str, Vec<u8>)> {
    let (id, file) = crate::font_setup::pdf_fallback_for(families);
    let bytes = crate::font_setup::obtain_font(
        id,
        file,
        |file| async move { font_load_one(&file).await },
        || async { font_fetch_text(&crate::font_setup::list_url()).await },
        |entry| async move {
            if !crate::host::options().font_download {
                return Err("font downloads are off".to_string());
            }
            font_download(&entry.file, &entry.url, entry.size).await
        },
    )
    .await?;
    Some((id, bytes))
}

/// Opens this page in a new browser tab with `query` (`""` or `"?new=docx"`): the Web version has one editor in a page, so what the desktop
/// opens in another window (ファイル > 新規作成 of an open document, the logo) is another tab. Called inside the click that asks for it, so
/// the browser does not take it for a pop-up.
pub fn open_in_new_tab(query: &str) {
    open_in_new_tab_js(query);
}

/// The kind `?new=<ext>` of this page's address names (`docx`, `xlsx`, `pptx`), if it has one.
pub fn new_kind_of_address() -> Option<ibis_formats::Kind> {
    let ext = new_kind_of_address_js()?;
    ibis_formats::Kind::from_extension(&ext)
}

#[wasm_bindgen(module = "/js/web_scripts.js")]
extern "C" {
    #[wasm_bindgen(js_name = openInNewTab)]
    fn open_in_new_tab_js(query: &str);
    #[wasm_bindgen(js_name = newKindOfAddress)]
    fn new_kind_of_address_js() -> Option<String>;
    #[wasm_bindgen(js_name = copyText)]
    fn copy_text_js(text: &str) -> js_sys::Promise;
}

/// Copies `text` to the clipboard; `false` when the browser refused (the caller tells the user: an unhandled rejection would only reach the console).
pub(crate) async fn copy_text(text: &str) -> bool {
    wasm_bindgen_futures::JsFuture::from(copy_text_js(text)).await.ok().and_then(|v| v.as_bool()).unwrap_or(false)
}

/// The `?new=` query of a new tab for `kind`.
pub fn new_query(kind: ibis_formats::Kind) -> &'static str {
    match kind {
        ibis_formats::Kind::Presentation => "?new=pptx",
        ibis_formats::Kind::Workbook => "?new=xlsx",
        ibis_formats::Kind::Document => "?new=docx",
        ibis_formats::Kind::Pdf => "?new=pdf",
    }
}

#[wasm_bindgen]
extern "C" {
    /// Defined by `www/main.ts`: a blank tab opened at a print click (`null` when the browser refused).
    #[wasm_bindgen(js_name = ibisOfficeBlankTab)]
    fn blank_tab_js() -> JsValue;
    /// Defined by `www/main.ts`: the print's PDF in the tab (or a new one when it is `null`); no bytes closes the tab.
    #[wasm_bindgen(js_name = ibisOfficeShowPdf)]
    fn show_pdf_js(tab: &JsValue, bytes: &[u8]);
}

/// A blank tab for a print's PDF, to be opened while the click still counts (the PDF takes seconds to make, and a tab opened then
/// would be stopped by the popup blocker). Kept by the caller and given to `show_pdf_in_tab`.
pub struct PrintTab(JsValue);

pub fn open_print_tab() -> PrintTab {
    PrintTab(blank_tab_js())
}

/// The print's PDF in the tab opened at the click (a new tab when the browser refused it).
pub fn show_pdf_in_tab(tab: PrintTab, bytes: &[u8]) {
    show_pdf_js(&tab.0, bytes);
}

/// Close a print tab left over (the PDF could not be made).
pub fn close_print_tab(tab: PrintTab) {
    show_pdf_js(&tab.0, &[]);
}

#[wasm_bindgen]
extern "C" {
    /// Defined by `www/main.ts`: asks the browser for the fonts of the viewer's PC (Chrome's `queryLocalFonts`), at a click.
    #[wasm_bindgen(js_name = ibisOfficeLocalFontsStart)]
    fn local_fonts_start_js();
    /// Defined by `www/main.ts`: `{ bytes, postscriptName }` of the PC's face of the family nearest the weight and style, or `null`.
    #[wasm_bindgen(catch, js_name = ibisOfficeLocalFontFace)]
    fn local_font_face_js(family: &str, weight: f64, italic: bool) -> Result<js_sys::Promise, JsValue>;
}

/// Ask for the fonts of the viewer's PC now, while the click that wants a PDF still counts for the permission prompt.
pub fn local_fonts_start() {
    local_fonts_start_js();
}

/// The font file of the viewer's PC for `family` (nearest `weight` and `italic`) and the face's PostScript name, or `None` (no such font,
/// the permission refused, a browser without the API).
pub async fn local_font_face(family: &str, weight: f32, italic: bool) -> Option<(Vec<u8>, String)> {
    let promise = local_font_face_js(family, weight as f64, italic).ok()?;
    let value = wasm_bindgen_futures::JsFuture::from(promise).await.ok()?;
    if value.is_null() || value.is_undefined() {
        return None;
    }
    let bytes = js_sys::Uint8Array::new(&js_sys::Reflect::get(&value, &JsValue::from_str("bytes")).ok()?).to_vec();
    let name = js_sys::Reflect::get(&value, &JsValue::from_str("postscriptName")).ok()?.as_string().unwrap_or_default();
    Some((bytes, name))
}

/// The transition sound `id` (`resource-list.txt`, kind `transition-sound`) for the browser, which carries none: the copy kept in IndexedDB (under `resource/transition-sound/<file>`
/// beside the fonts, the same chunks), else downloaded now from the list and kept; handed to `ibis_formats::transition_sounds::provide`. `true` when it is there.
pub async fn fetch_transition_sound(id: &str) -> bool {
    if ibis_formats::transition_sounds::ogg(id).is_some() {
        return true;
    }
    let list_url = crate::resource_list::list_url();
    let Ok(text) = font_fetch_text(&list_url).await else { return false };
    let Some(entry) = crate::resource_list::parse_list_at(&text, Some(&list_url)).into_iter().find(|e| e.kind == "transition-sound" && e.id == id) else { return false };
    let key = format!("resource/transition-sound/{}", entry.file);
    let bytes = match font_load_one(&key).await {
        Some(bytes) => bytes,
        None => {
            if font_download(&key, &entry.url, entry.size).await.is_err() {
                return false;
            }
            let Some(bytes) = font_load_one(&key).await else { return false };
            bytes
        }
    };
    ibis_formats::transition_sounds::provide(id, bytes);
    true
}
