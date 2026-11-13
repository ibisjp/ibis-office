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

/// The font files kept in IndexedDB (read at start, before the first document is drawn; an error is no fonts).
pub async fn load_downloaded_fonts() -> Vec<Vec<u8>> {
    let Ok(promise) = fonts_load_js() else { return Vec::new() };
    let Ok(value) = wasm_bindgen_futures::JsFuture::from(promise).await else { return Vec::new() };
    js_sys::Array::from(&value).iter().map(|item| js_sys::Uint8Array::new(&item).to_vec()).collect()
}

/// Opens this page in a new browser tab with `query` (`""` or `"?new=docx"`): the Web version has one editor in a page, so what the desktop
/// opens in another window (ファイル > 新規作成 of an open document, the logo) is another tab. Called inside the click that asks for it, so
/// the browser does not take it for a pop-up.
pub fn open_in_new_tab(query: &str) {
    let open = js_sys::Function::new_with_args("q", "window.open(location.origin + location.pathname + q, '_blank');");
    let _ = open.call1(&JsValue::NULL, &JsValue::from_str(query));
}

/// The kind `?new=<ext>` of this page's address names (`docx`, `xlsx`, `pptx`), if it has one.
pub fn new_kind_of_address() -> Option<ibis_pptx::Kind> {
    let read = js_sys::Function::new_no_args("return new URLSearchParams(location.search).get('new');");
    let ext = read.call0(&JsValue::NULL).ok()?.as_string()?;
    ibis_pptx::Kind::from_extension(&ext)
}

/// The `?new=` query of a new tab for `kind`.
pub fn new_query(kind: ibis_pptx::Kind) -> &'static str {
    match kind {
        ibis_pptx::Kind::Presentation => "?new=pptx",
        ibis_pptx::Kind::Workbook => "?new=xlsx",
        ibis_pptx::Kind::Document => "?new=docx",
    }
}
