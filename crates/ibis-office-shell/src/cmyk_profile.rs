// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The ICC profile CMYK colours are shown with. Nothing has to be set: until a profile is chosen the open model of
//! `ibis_doc_model::OpenCmykConverter` converts. The profiles installed in the OS (the places the OS keeps them) are found
//! by themselves and listed (CMYK ones only: the profile's colour space is CMYK); the user picks one, or picks an ICC file.
//! The choice is kept in the user's settings (the file's path); a file made with a profile saves only the profile's name
//! and an identifier (never the profile itself), and says so in an information bar when this PC has no such profile.
//! The conversion is the pure Rust `moxcms` (BSD-3-Clause OR Apache-2.0).

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Cmyk, CmykConverter, OpenCmykConverter, Rgb};
use moxcms::{ColorProfile, DataColorSpace, Layout, RenderingIntent, TransformExecutor, TransformOptions};

use super::chrome;
use super::editor::SlideEditor;
use super::i18n::text;

/// The official page where profiles are given out (only opened in the browser: the app neither downloads nor ships a profile).
pub(crate) const PROFILE_SOURCE_URL: &str = "https://www.eci.org/en/downloads";

const KEY_PATH: &str = "cmyk.profile-path";
const KEY_HINT_SHOWN: &str = "cmyk.hint-shown";
const DIALOG_KEY: &str = "cmyk.profile-dialog";

/// The information bar about the profile (`shell/infobar.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmykNotice {
    /// The mode was just turned on and no profile is chosen: the simple conversion shows the colours.
    Hint,
    /// The file was made with a profile this PC does not have.
    Missing,
}

/// A profile found on this PC (or chosen by the user).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileInfo {
    /// The profile's description (its name), or the file's name.
    pub name: String,
    pub path: PathBuf,
    /// The identifier saved in a file: the profile ID of the ICC header when it has one, else a hash of the profile.
    pub id: String,
}

/// The places the OS keeps ICC profiles.
pub(crate) fn search_dirs() -> Vec<PathBuf> {
    if let Some(dir) = std::env::var_os("IBIS_ICC_DIR").filter(|v| !v.is_empty()) {
        return vec![PathBuf::from(dir)];
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs = Vec::new();
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/ColorSync/Profiles"));
        dirs.push(PathBuf::from("/Library/ColorSync/Profiles"));
        dirs.extend(home.map(|h| h.join("Library/ColorSync/Profiles")));
    }
    #[cfg(target_os = "windows")]
    {
        let windir = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        dirs.push(windir.join("System32\\spool\\drivers\\color"));
        let _ = home;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        dirs.push(PathBuf::from("/usr/share/color/icc"));
        dirs.push(PathBuf::from("/usr/local/share/color/icc"));
        if let Some(h) = home {
            dirs.push(h.join(".local/share/color/icc"));
            dirs.push(h.join(".color/icc"));
        }
    }
    dirs
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The profile's description from its `desc` tag (the v2 `desc` and the v4 `mluc` types).
fn description(bytes: &[u8]) -> Option<String> {
    let count = be32(bytes, 128)? as usize;
    for i in 0..count.min(200) {
        let at = 132 + i * 12;
        if bytes.get(at..at + 4)? != b"desc" {
            continue;
        }
        let (offset, size) = (be32(bytes, at + 4)? as usize, be32(bytes, at + 8)? as usize);
        let tag = bytes.get(offset..offset.checked_add(size)?)?;
        match tag.get(..4)? {
            b"desc" => {
                let len = be32(tag, 8)? as usize;
                let s = tag.get(12..12 + len)?;
                return Some(String::from_utf8_lossy(s).trim_end_matches('\0').trim().to_string());
            }
            b"mluc" => {
                // The first record: length, offset (from the tag's start), UTF-16BE text.
                let (len, off) = (be32(tag, 20)? as usize, be32(tag, 24)? as usize);
                let raw = tag.get(off..off + len)?;
                let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                return Some(String::from_utf16_lossy(&units).trim_end_matches('\0').trim().to_string());
            }
            _ => return None,
        }
    }
    None
}

/// Whether `bytes` is an ICC profile whose colour space is CMYK; its name and identifier.
pub fn inspect(bytes: &[u8], file_name: &str) -> Option<(String, String)> {
    if bytes.get(36..40)? != b"acsp" || bytes.get(16..20)? != b"CMYK" {
        return None;
    }
    let name = description(bytes).filter(|n| !n.is_empty()).unwrap_or_else(|| file_name.to_string());
    let header_id = bytes.get(84..100)?;
    let id = if header_id.iter().any(|b| *b != 0) {
        header_id.iter().map(|b| format!("{b:02x}")).collect::<String>()
    } else {
        // No ID in the header: two FNV-1a hashes of the profile (different seeds) stand for it.
        let fnv = |seed: u64| bytes.iter().fold(seed, |h, b| (h ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3));
        format!("fnv-{:016x}{:016x}", fnv(0xcbf2_9ce4_8422_2325), fnv(0x8422_2325_cbf2_9ce4))
    };
    Some((name, id))
}

/// The CMYK profiles in `dirs` (a profile is read for its header only), by name.
pub fn scan(dirs: &[PathBuf]) -> Vec<ProfileInfo> {
    let mut found = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if ext != "icc" && ext != "icm" {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if let Some((name, id)) = inspect(&bytes, &file) {
                found.push(ProfileInfo { name, path, id });
            }
        }
    }
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    found.dedup_by(|a, b| a.id == b.id);
    found
}

/// The conversion of an ICC profile (CMYK to sRGB and back by the profile's own tables).
pub(crate) struct IccConverter {
    name: String,
    to_rgb: Arc<dyn TransformExecutor<f32> + Send + Sync>,
    to_cmyk: Option<Arc<dyn TransformExecutor<f32> + Send + Sync>>,
}

impl IccConverter {
    pub(crate) fn from_bytes(bytes: &[u8], name: String) -> Result<Self, String> {
        let profile = ColorProfile::new_from_slice(bytes).map_err(|e| format!("{e:?}"))?;
        if profile.color_space != DataColorSpace::Cmyk {
            return Err("not a CMYK profile".into());
        }
        let srgb = ColorProfile::new_srgb();
        let options = TransformOptions { rendering_intent: RenderingIntent::RelativeColorimetric, ..Default::default() };
        let to_rgb = profile.create_transform_f32(Layout::Rgba, &srgb, Layout::Rgb, options).map_err(|e| format!("{e:?}"))?;
        let to_cmyk = srgb.create_transform_f32(Layout::Rgb, &profile, Layout::Rgba, options).ok();
        Ok(Self { name, to_rgb, to_cmyk })
    }
}

impl CmykConverter for IccConverter {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn to_rgb(&self, cmyk: Cmyk) -> Rgb {
        self.to_rgb_fractions(cmyk.fractions())
    }

    fn to_rgb_fractions(&self, ink: [f64; 4]) -> Rgb {
        let cmyk = Cmyk::from_fractions(ink);
        let src = ink.map(|v| v.clamp(0.0, 1.0) as f32);
        let mut dst = [0.0f32; 3];
        if self.to_rgb.transform(&src, &mut dst).is_err() {
            return OpenCmykConverter.to_rgb(cmyk);
        }
        let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Rgb::from_rgb(b(dst[0]), b(dst[1]), b(dst[2]))
    }

    fn to_cmyk(&self, rgb: Rgb) -> Cmyk {
        let Some(t) = &self.to_cmyk else { return OpenCmykConverter.to_cmyk(rgb) };
        let src = [rgb.r() as f32 / 255.0, rgb.g() as f32 / 255.0, rgb.b() as f32 / 255.0];
        let mut dst = [0.0f32; 4];
        if t.transform(&src, &mut dst).is_err() {
            return OpenCmykConverter.to_cmyk(rgb);
        }
        Cmyk::from_fractions(dst.map(|v| v as f64))
    }
}

/// The conversion of the profile in `bytes` (an ICC file called `file`); `path` is where it came from (just the file's name in
/// the browser, which has no path).
fn parse_profile(bytes: &[u8], file: &str, path: PathBuf) -> Result<(Arc<dyn CmykConverter>, ProfileInfo), String> {
    let (name, id) = inspect(bytes, file).ok_or_else(|| "not a CMYK ICC profile".to_string())?;
    let converter = IccConverter::from_bytes(bytes, name.clone())?;
    Ok((Arc::new(converter), ProfileInfo { name, path, id }))
}

/// The profile of `path` put in use (and kept in the settings as the user's choice); the open model when `None`.
fn load_converter(path: &Path) -> Result<(Arc<dyn CmykConverter>, ProfileInfo), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    parse_profile(&bytes, &file, path.to_path_buf())
}

/// The profile the user chose before (its path in the settings), when it is still there.
fn saved_profile() -> Option<ProfileInfo> {
    let path = PathBuf::from(super::settings::get(KEY_PATH)?);
    load_converter(&path).ok().map(|(_, info)| info)
}

/// The sheet of the identifier and name the open file's table was made with (`CmykMode::profile`: `name` or `name|id`).
pub fn profile_label(name: &str, id: &str) -> String {
    format!("{name}|{id}")
}

fn split_label(label: &str) -> (&str, Option<&str>) {
    match label.rsplit_once('|') {
        Some((n, i)) => (n, Some(i)),
        None => (label, None),
    }
}

impl SlideEditor {
    /// The name of the profile in use ("the open model" when none).
    pub fn cmyk_profile_name(&self) -> String {
        super::cmyk::converter().name()
    }

    /// Makes the profile `info` the one in use: for the colours made from now on, kept in the settings.
    fn use_profile(&mut self, info: &ProfileInfo, cx: &mut Context<Self>) -> bool {
        match load_converter(&info.path) {
            Ok((converter, _)) => {
                super::settings::set(KEY_PATH, &info.path.to_string_lossy());
                self.activate_profile(converter, info, cx);
                true
            }
            Err(err) => {
                log::warn!("{}: {err}", info.path.display());
                false
            }
        }
    }

    /// The converter of `info` is put in use (the settings keep the choice by the caller).
    fn activate_profile(&mut self, converter: Arc<dyn CmykConverter>, info: &ProfileInfo, cx: &mut Context<Self>) {
        super::cmyk::set_converter(converter);
        self.cmyk_notice = None;
        self.stamp_profile_in_file(Some(info), cx);
        cx.notify();
    }

    /// The simple conversion again (no profile): the default.
    pub(crate) fn use_open_model(&mut self, cx: &mut Context<Self>) {
        super::cmyk::set_converter(Arc::new(OpenCmykConverter));
        super::settings::set(KEY_PATH, "");
        #[cfg(target_family = "wasm")]
        crate::web::icc_forget();
        self.stamp_profile_in_file(None, cx);
        cx.notify();
    }

    /// The file's table keeps the name and identifier of the profile in use (only when the mode is on and it changed).
    fn stamp_profile_in_file(&mut self, info: Option<&ProfileInfo>, cx: &mut Context<Self>) {
        if !self.has_cmyk() || !self.presentation().cmyk.enabled {
            return;
        }
        let mut mode = self.presentation().cmyk.clone();
        mode.profile = info.map(|i| profile_label(&i.name, &i.id));
        if mode != self.presentation().cmyk {
            self.run(vec![ibis_doc_model::Command::SetCmykMode { cmyk: mode }], cx);
        }
    }

    /// The bar's "簡易変換で続ける": the file goes on with the simple conversion (its sRGB values stay as they are).
    pub(crate) fn use_simple_cmyk_conversion(&mut self, cx: &mut Context<Self>) {
        self.cmyk_notice = None;
        cx.notify();
    }

    /// 「あとで」: the bar is closed.
    pub(crate) fn dismiss_cmyk_notice(&mut self, cx: &mut Context<Self>) {
        self.cmyk_notice = None;
        cx.notify();
    }

    /// The CMYK mode was just turned on: the user's saved profile is put in use, or (once) the bar explains the simple conversion.
    pub(crate) fn after_cmyk_enabled(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_family = "wasm")]
        self.restore_saved_profile(cx);
        if let Some(info) = saved_profile() {
            self.use_profile(&info, cx);
            return;
        }
        if super::settings::get(KEY_HINT_SHOWN).as_deref() != Some("1") {
            super::settings::set(KEY_HINT_SHOWN, "1");
            self.cmyk_notice = Some(CmykNotice::Hint);
        }
        cx.notify();
    }

    /// A file was opened: when it was made with a profile, the one in use (or one found on this PC by its name or identifier) is
    /// put in use; when none is found the bar says so.
    pub(crate) fn check_cmyk_profile_on_open(&mut self, cx: &mut Context<Self>) {
        self.cmyk_notice = None;
        if !self.has_cmyk() || !self.presentation().cmyk.enabled {
            return;
        }
        #[cfg(target_family = "wasm")]
        self.restore_saved_profile(cx);
        let Some(label) = self.presentation().cmyk.profile.clone() else {
            return;
        };
        let (name, id) = split_label(&label);
        let in_use = super::cmyk::converter().name();
        if in_use == name {
            return;
        }
        let mut candidates: Vec<ProfileInfo> = saved_profile().into_iter().collect();
        candidates.extend(scan(&search_dirs()));
        let hit = candidates.into_iter().find(|p| id.is_some_and(|i| p.id == i) || p.name == name);
        match hit {
            Some(info) => {
                if let Ok((converter, _)) = load_converter(&info.path) {
                    super::cmyk::set_converter(converter);
                }
            }
            None => self.cmyk_notice = Some(CmykNotice::Missing),
        }
        cx.notify();
    }

    // ----- The window to choose the profile (ファイル > 設定, and the bar's button) --------------------------------

    pub(crate) fn open_cmyk_profile_chooser(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let _ = w;
        let state = Rc::new(std::cell::RefCell::new(ChooserState { found: scan(&search_dirs()), message: None, sidebar: self.sidebar_open }));
        self.document_memory.set(DIALOG_KEY, Some(state));
        self.sidebar_open = true;
        self.dialog = Default::default();
        cx.notify();
    }

    fn chooser_state(&self) -> Option<Rc<std::cell::RefCell<ChooserState>>> {
        self.document_memory.get::<Option<Rc<std::cell::RefCell<ChooserState>>>>(DIALOG_KEY).flatten()
    }

    pub(crate) fn close_cmyk_profile_chooser(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.chooser_state() {
            self.sidebar_open = state.borrow().sidebar;
        }
        self.document_memory.set(DIALOG_KEY, None::<Rc<std::cell::RefCell<ChooserState>>>);
        self.dialog = Default::default();
        self.focus_handle.focus(w, cx);
        cx.notify();
    }

    /// ファイルを選ぶ…: an ICC file is chosen with the file dialog (the browser version cannot open it yet).
    fn choose_icc_file(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: vec!["icc".into(), "icm".into()], multiple: false, prompt: None });
            cx.spawn_in(w, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update_in(cx, |this, _, cx| this.use_icc_path(path, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        cx.spawn_in(w, async move |this, cx| {
            let picked = crate::web::pick_icc_file().await;
            this.update_in(cx, |this, _, cx| match picked {
                Ok(Some((name, bytes))) => this.use_icc_bytes(&bytes, &name, cx),
                Ok(None) => {}
                Err(err) => this.set_status(crate::i18n::format("file.pick-failed", &[("error", &err)]), cx),
            })
            .ok();
        })
        .detach();
    }

    /// The ICC file in `bytes` (chosen in the browser) is put in use when it is a CMYK profile, and kept in the browser's
    /// IndexedDB (as a setting; the settings file's path means nothing there). Else the window says it is not one.
    #[cfg(target_family = "wasm")]
    pub(crate) fn use_icc_bytes(&mut self, bytes: &[u8], file: &str, cx: &mut Context<Self>) {
        let ok = match parse_profile(bytes, file, PathBuf::from(file)) {
            Ok((converter, info)) => {
                crate::web::icc_save(file, bytes);
                self.activate_profile(converter, &info, cx);
                true
            }
            Err(_) => false,
        };
        self.note_profile_result(ok, None, cx);
    }

    /// The browser keeps the profile the user chose in IndexedDB: put it in use, when none is in use yet, where it is wanted: when
    /// the CMYK mode is turned on (`after_cmyk_enabled`), when a file with the mode on is opened (`check_cmyk_profile_on_open`) and
    /// when the settings window shows the CMYK category (`scan_for_settings`). (The page keeps nothing but the bytes until then.)
    #[cfg(target_family = "wasm")]
    pub(crate) fn restore_saved_profile(&mut self, cx: &mut Context<Self>) {
        if super::cmyk::converter().name() != OpenCmykConverter.name() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let Ok(Some((name, bytes))) = crate::web::icc_load().await else { return };
            this.update(cx, |this, cx| {
                if super::cmyk::converter().name() != OpenCmykConverter.name() {
                    return;
                }
                if let Ok((converter, info)) = parse_profile(&bytes, &name, PathBuf::from(&name)) {
                    this.activate_profile(converter, &info, cx);
                    this.check_cmyk_profile_on_open(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// The result of choosing a file is shown in the window that asked (the chooser, and the settings).
    fn note_profile_result(&mut self, ok: bool, info: Option<ProfileInfo>, cx: &mut Context<Self>) {
        let message = (!ok).then(|| text("shell.cmyk.profile.not-cmyk"));
        if let Some(state) = self.chooser_state() {
            let mut s = state.borrow_mut();
            if let Some(info) = info.as_ref().filter(|i| !s.found.iter().any(|p| p.id == i.id)) {
                s.found.push(info.clone());
            }
            s.message = message.clone();
        }
        if let Some(settings) = self.settings.as_mut() {
            if let (Some(found), Some(info)) = (settings.cmyk_found.as_mut(), info.as_ref())
                && !found.iter().any(|p| p.id == info.id)
            {
                found.push(info.clone());
            }
            settings.cmyk_message = message;
        }
        cx.notify();
    }

    /// The ICC file `path` is chosen: in use when it is a CMYK profile, else the window says it is not one.
    pub(crate) fn use_icc_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let (ok, info) = match load_converter(&path) {
            Ok((_, info)) => (self.use_profile(&info, cx), Some(info)),
            Err(_) => (false, None),
        };
        self.note_profile_result(ok, info, cx);
    }

    /// The profiles found in the OS's places, scanned once each time the settings window shows the CMYK category. In the browser
    /// the kept profile is put in use then too (the window shows what the next file will use).
    pub(crate) fn scan_for_settings(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.settings.as_mut()
            && s.cmyk_found.is_none()
        {
            s.cmyk_found = Some(scan(&search_dirs()));
            #[cfg(target_family = "wasm")]
            self.restore_saved_profile(cx);
            #[cfg(not(target_family = "wasm"))]
            let _ = cx;
        }
    }

    /// What the window to choose a profile shows, as the chooser and the settings both do: where the profiles were looked for and
    /// how many were found, the profiles to choose from (the simple conversion is the default), then the buttons to choose a file,
    /// to the page where profiles are given out, and to go back to the default.
    pub(crate) fn cmyk_profile_controls(&self, found: &[ProfileInfo], message: Option<SharedString>, cx: &mut Context<Self>) -> Div {
        let simple_in_use = self.cmyk_profile_name() == OpenCmykConverter.name();
        let current = if simple_in_use { text("shell.cmyk.profile.simple").to_string() } else { self.cmyk_profile_name() };
        let mut list = v_flex().gap_1().w_full();
        list = list.child(chrome::radio_button(
            SharedString::from("cmyk-profile-simple"),
            simple_in_use,
            false,
            text("shell.cmyk.profile.simple"),
            cx.listener(|this, _, _, cx| this.use_open_model(cx)),
        ));
        for (i, info) in found.iter().cloned().enumerate() {
            let checked = !simple_in_use && info.name == self.cmyk_profile_name();
            let label = SharedString::from(info.name.clone());
            list = list.child(chrome::radio_button(
                SharedString::from(format!("cmyk-profile-{i}")),
                checked,
                false,
                label,
                cx.listener(move |this, _, _, cx| {
                    this.use_profile(&info, cx);
                }),
            ));
        }
        let places = search_dirs().iter().map(|d| d.to_string_lossy().into_owned()).collect::<Vec<_>>().join(", ");
        let searched = if cfg!(target_family = "wasm") {
            text("shell.cmyk.profile.web-note").to_string()
        } else if found.is_empty() {
            crate::i18n::format("shell.cmyk.profile.none-found", &[("places", &places)])
        } else {
            crate::i18n::format("shell.cmyk.profile.found", &[("count", &found.len()), ("places", &places)])
        };
        v_flex()
            .gap_3()
            .w_full()
            .child(div().text_sm().child(text("shell.cmyk.profile.steps")))
            .child(div().id("cmyk-profile-searched").text_sm().child(searched))
            .child(div().id("cmyk-profile-in-use").text_sm().child(crate::i18n::format("shell.cmyk.profile.in-use", &[("name", &current)])))
            .child(div().id("cmyk-profile-list").max_h(px(260.)).overflow_y_scroll().child(list))
            .children(message.map(|m| div().id("cmyk-profile-message").text_sm().child(m)))
            .child(h_flex().gap_2().child(chrome::button_group([
                chrome::ButtonGroupItem::new("cmyk-profile-file", text("shell.cmyk.profile.choose-file")).on_click(cx.listener(|this, _, w, cx| this.choose_icc_file(w, cx))),
                chrome::ButtonGroupItem::new("cmyk-profile-source", text("shell.infobar.cmyk.source")).on_click(cx.listener(|_, _, _, cx| crate::external::open_url(PROFILE_SOURCE_URL, cx))),
                chrome::ButtonGroupItem::new("cmyk-profile-reset", text("shell.cmyk.profile.reset")).disabled(simple_in_use).on_click(cx.listener(|this, _, _, cx| this.use_open_model(cx))),
            ])))
    }

    /// The window: the profiles found on this PC to choose from, the file dialog, the page to get one, and the steps.
    pub(crate) fn cmyk_profile_dialog(&mut self, cx: &mut Context<Self>) -> Option<chrome::SidebarPanel> {
        let state = self.chooser_state()?;
        let (found, message) = {
            let s = state.borrow();
            (s.found.clone(), s.message.clone())
        };
        let body = self.cmyk_profile_controls(&found, message, cx);
        let actions = chrome::action_bar().action(
            chrome::ActionDefinition::new("cmyk-profile-close", text("_shell.close"), chrome::ActionButtonKind::Primary)
                .on_click(cx.listener(|e, _, w, cx| e.close_cmyk_profile_chooser(w, cx))),
        );
        Some(chrome::SidebarPanel::with_footer(text("shell.cmyk.profile.title"), body.into_any_element(), actions))
    }
}

struct ChooserState {
    found: Vec<ProfileInfo>,
    message: Option<SharedString>,
    sidebar: bool,
}

#[cfg(test)]
mod tests {
    // (Not `use super::*`: it would bring gpui's `test` attribute in, which expands without end here.)
    use super::{inspect, profile_label, search_dirs, split_label};

    /// A minimal CMYK profile header (128 bytes), a tag table with one `desc` tag.
    fn header(name: &str, space: &[u8; 4], id: [u8; 16]) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[16..20].copy_from_slice(space);
        b[36..40].copy_from_slice(b"acsp");
        b[84..100].copy_from_slice(&id);
        b.extend_from_slice(&1u32.to_be_bytes());
        let tag_at = 132 + 12;
        b.extend_from_slice(b"desc");
        b.extend_from_slice(&(tag_at as u32).to_be_bytes());
        let mut tag = Vec::new();
        tag.extend_from_slice(b"desc\0\0\0\0");
        tag.extend_from_slice(&(name.len() as u32 + 1).to_be_bytes());
        tag.extend_from_slice(name.as_bytes());
        tag.push(0);
        b.extend_from_slice(&(tag.len() as u32).to_be_bytes());
        b.extend_from_slice(&tag);
        b
    }

    #[test]
    fn a_cmyk_profile_is_known_by_its_header_and_its_description() {
        let p = header("Japan Color 2001 Coated", b"CMYK", [7; 16]);
        let (name, id) = inspect(&p, "x.icc").unwrap();
        assert_eq!(name, "Japan Color 2001 Coated");
        assert_eq!(id, "07".repeat(16));
        assert!(inspect(&header("sRGB", b"RGB ", [0; 16]), "srgb.icc").is_none(), "an RGB profile is not listed");
        assert!(inspect(b"not a profile", "x.icc").is_none());
    }

    #[test]
    fn a_profile_without_an_id_is_known_by_a_hash_of_it() {
        let a = inspect(&header("A", b"CMYK", [0; 16]), "a.icc").unwrap().1;
        let b = inspect(&header("B", b"CMYK", [0; 16]), "b.icc").unwrap().1;
        assert!(a.starts_with("fnv-") && a != b);
    }

    #[test]
    fn the_label_in_the_file_has_the_name_and_the_id_only() {
        let label = profile_label("Japan Color 2001 Coated", "abc");
        assert_eq!(split_label(&label), ("Japan Color 2001 Coated", Some("abc")));
        assert_eq!(split_label("Name only"), ("Name only", None));
    }

    #[test]
    fn the_places_of_each_os_are_searched() {
        let dirs = search_dirs();
        #[cfg(target_os = "macos")]
        assert!(dirs.iter().any(|d| d.ends_with("ColorSync/Profiles")));
        #[cfg(target_os = "linux")]
        assert!(dirs.iter().any(|d| d.ends_with("color/icc")));
        assert!(!dirs.is_empty());
    }
}
