// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GPUI PDF viewer state and rendering bridge over [`ibis_pdf`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gpui_kit::{App, Bounds, Pixels, Point, RenderImage, Window, px, size};
use ibis_ofd::OfdContainer;
use ibis_xps::XpsContainer;
use ibis_pdf::interpret::interpret_page_scene;
use ibis_pdf::scene::{PageInfo, PageScene, collect_pages};
use ibis_pdf::signature::{SignatureRecord, inspect_signatures};
use ibis_pdf::{PdfContainer, Result};

pub mod editor;
pub mod render;
pub use editor::PdfDocument;

enum ViewerSource {
    Empty,
    Pdf(Arc<Mutex<PdfContainer>>),
    Ofd(Arc<OfdContainer>),
    Xps(Arc<XpsContainer>),
}

/// A character of the document: the page and its place in the page's text (the order the text is in the file).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub page: usize,
    pub index: usize,
}

/// The text picked with the pointer: from where the press was to where the pointer is (either may come first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Pos,
    pub focus: Pos,
}

impl Selection {
    pub fn ordered(self) -> (Pos, Pos) {
        (self.anchor.min(self.focus), self.anchor.max(self.focus))
    }
}

/// A place the search found: the page and the characters `[start, end)` of its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FindMatch {
    pub page: usize,
    pub start: usize,
    pub end: usize,
}

/// Viewer state for an open PDF or OFD document (lazy page scene & GPU image cache).
pub struct PdfViewerState {
    source: ViewerSource,
    pub pages: Vec<PageInfo>,
    pub signatures: Vec<SignatureRecord>,
    ofd_signature_status: Option<String>,
    scene_cache: HashMap<usize, Arc<PageScene>>,
    image_cache: HashMap<(usize, usize), Option<Arc<RenderImage>>>,
    pub zoom: f32,
    pub current_page: usize,
    /// The text picked, if any (it is looked at and copied; the file is not changed).
    pub selection: Option<Selection>,
    /// The places the last search found, and the one shown.
    pub find_matches: Vec<FindMatch>,
    pub find_current: Option<usize>,
    /// The text the places were looked for by.
    pub find_query: String,
    /// Whether some page has text (a picked, copied, searched document): found by reading the pages once, and remembered (`has_text`).
    text_found: Option<bool>,
}

impl PdfViewerState {
    /// Empty viewer state used as a non-panicking placeholder while loading.
    pub fn empty() -> Self {
        Self {
            source: ViewerSource::Empty,
            pages: Vec::new(),
            signatures: Vec::new(),
            ofd_signature_status: None,
            scene_cache: HashMap::new(),
            image_cache: HashMap::new(),
            zoom: 1.0,
            current_page: 0,
            selection: None,
            find_matches: Vec::new(),
            find_current: None,
            find_query: String::new(),
            text_found: None,
        }
    }

    /// Open a PDF or OFD container, resolving page metadata and signature counts
    /// without eagerly interpreting page content streams.
    pub fn open(bytes: impl Into<Arc<[u8]>>) -> Result<Self> {
        let raw: Arc<[u8]> = bytes.into();
        if ibis_ofd::is_ofd(&raw) {
            let ofd = OfdContainer::from_bytes(raw).map_err(|e| ibis_pdf::Error::new(e.to_string()))?;
            let pages = ofd.page_infos();
            let ofd_signature_status = ofd.signature_summary().map(|(n, outcome)| signature_text(n, Some(outcome)));
            return Ok(Self {
                source: ViewerSource::Ofd(Arc::new(ofd)),
                pages,
                signatures: Vec::new(),
                ofd_signature_status,
                scene_cache: HashMap::new(),
                image_cache: HashMap::new(),
                zoom: 1.0,
                current_page: 0,
                selection: None,
                find_matches: Vec::new(),
                find_current: None,
            find_query: String::new(),
            text_found: None,
            });
        }
        if ibis_xps::is_xps(&raw) {
            let xps = XpsContainer::from_bytes(raw).map_err(|e| ibis_pdf::Error::new(e.to_string()))?;
            let pages = xps.page_infos();
            let sigs = xps.signature_count();
            let ofd_signature_status = (sigs > 0).then(|| signature_text(sigs, None));
            return Ok(Self {
                source: ViewerSource::Xps(Arc::new(xps)),
                pages,
                signatures: Vec::new(),
                ofd_signature_status,
                scene_cache: HashMap::new(),
                image_cache: HashMap::new(),
                zoom: 1.0,
                current_page: 0,
                selection: None,
                find_matches: Vec::new(),
                find_current: None,
            find_query: String::new(),
            text_found: None,
            });
        }
        let mut container = PdfContainer::from_bytes(raw)?;
        let pages = collect_pages(&mut container)?;
        let signatures = inspect_signatures(&mut container)?;
        Ok(Self {
            source: ViewerSource::Pdf(Arc::new(Mutex::new(container))),
            pages,
            signatures,
            ofd_signature_status: None,
            scene_cache: HashMap::new(),
            image_cache: HashMap::new(),
            zoom: 1.0,
            current_page: 0,
            selection: None,
            find_matches: Vec::new(),
            find_current: None,
            find_query: String::new(),
            text_found: None,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Whether any page has a character that can be picked: the pages are read in order until one is found, and the answer is remembered (a
    /// scanned document of images has none: its Select all and Find stay grey).
    pub fn has_text(&mut self) -> bool {
        if let Some(found) = self.text_found {
            return found;
        }
        let found = (0..self.pages.len()).any(|page| !self.text_index(page).text_index.chars.is_empty());
        self.text_found = Some(found);
        found
    }

    /// The report of the signatures, in the words and the shape of the document screens' (`shell/signature.rs`): one block to a signature, then what was and was not looked at.
    /// A PDF's are compared by structure (the byte range against the revisions), an OFD's by SM3 digests and the SM2 value, an XPS's only counted.
    pub fn signature_report_text(&self) -> Option<String> {
        use ibis_office_shell::i18n::{format, text};
        let mut lines: Vec<String> = Vec::new();
        match &self.source {
            ViewerSource::Pdf(_) if !self.signatures.is_empty() => {
                for (i, record) in self.signatures.iter().enumerate() {
                    let result = match &record.status {
                        ibis_pdf::signature::ByteRangeStatus::CoversWholeFile => text("shell.signature.result.pdf-whole").to_string(),
                        ibis_pdf::signature::ByteRangeStatus::CoversRevisionWithSubsequentUpdates { revision, changed_objects } => {
                            format("shell.signature.result.pdf-later", &[("revision", revision), ("count", &changed_objects.len())])
                        }
                        ibis_pdf::signature::ByteRangeStatus::InvalidOrUnaligned => text("shell.signature.result.pdf-invalid").to_string(),
                    };
                    lines.push(format("shell.signature.report.head", &[("n", &(i + 1)), ("part", &record.field_name), ("result", &result)]));
                    if let Some(name) = &record.signer_name {
                        lines.push(format("shell.signature.report.pdf-signer", &[("name", name)]));
                    }
                    if let Some(reason) = &record.reason {
                        lines.push(format("shell.signature.report.pdf-reason", &[("reason", reason)]));
                    }
                    if let Some(filter) = &record.sub_filter {
                        let name = match filter {
                            ibis_pdf::signature::SubFilter::AdbePkcs7Detached => "adbe.pkcs7.detached".to_string(),
                            ibis_pdf::signature::SubFilter::AdbePkcs7Sha1 => "adbe.pkcs7.sha1".to_string(),
                            ibis_pdf::signature::SubFilter::EtsiCadesDetached => "ETSI.CAdES.detached".to_string(),
                            ibis_pdf::signature::SubFilter::EtsiRfc3161 => "ETSI.RFC3161".to_string(),
                            ibis_pdf::signature::SubFilter::Other(other) => other.clone(),
                        };
                        lines.push(format("shell.signature.report.pdf-format", &[("name", &name)]));
                    }
                }
                lines.push(String::new());
                lines.push(text("shell.signature.report.pdf-notice").to_string());
            }
            ViewerSource::Ofd(ofd) if !ofd.signature_verifications().is_empty() => {
                use ibis_ofd::OfdSignatureCryptoResult as Crypto;
                for (i, v) in ofd.signature_verifications().iter().enumerate() {
                    let result = if !v.all_references_ok() || v.crypto == Crypto::Invalid {
                        text("shell.signature.result.mismatch")
                    } else if v.crypto == Crypto::Valid {
                        text("shell.signature.result.consistent")
                    } else {
                        text("shell.signature.result.uninspectable")
                    };
                    lines.push(format("shell.signature.report.head", &[("n", &(i + 1)), ("part", &v.id), ("result", &result)]));
                    if let Some(signer) = &v.subject_cn {
                        lines.push(format("shell.signature.report.signer", &[("name", signer)]));
                    }
                    let value = match &v.crypto {
                        Crypto::Valid => text("shell.signature.check.match").to_string(),
                        Crypto::Invalid => text("shell.signature.check.mismatch").to_string(),
                        Crypto::NoPublicKey => format("shell.signature.check.uninspectable", &[("reason", &text("shell.signature.reason.no-key"))]),
                        Crypto::Malformed(_) => format("shell.signature.check.uninspectable", &[("reason", &text("shell.signature.reason.bad-key"))]),
                        Crypto::Unsupported(what) => format("shell.signature.check.unsupported", &[("what", what)]),
                    };
                    lines.push(format("shell.signature.report.value", &[("state", &value)]));
                    lines.push(format("shell.signature.report.digests", &[("matched", &v.references_ok), ("total", &v.references_total)]));
                    lines.push(format("shell.signature.report.ofd-algorithms", &[("sig", &v.sig_algorithm), ("check", &v.check_algorithm)]));
                }
                lines.push(String::new());
                lines.push(text("shell.signature.report.notice").to_string());
            }
            ViewerSource::Xps(xps) if xps.signature_count() > 0 => {
                lines.push(format("shell.signature.report.xps-count", &[("count", &xps.signature_count())]));
                lines.push(String::new());
                lines.push(text("shell.signature.report.notice").to_string());
            }
            _ => return None,
        }
        Some(lines.join("\n"))
    }

    pub fn signature_status_text(&self) -> Option<String> {
        if let Some(status) = &self.ofd_signature_status {
            return Some(status.clone());
        }
        if self.signatures.is_empty() {
            None
        } else {
            Some(signature_text(self.signatures.len(), None))
        }
    }

    /// Lazily interpret and cache `page_index`.
    pub fn page_scene(&mut self, page_index: usize) -> Result<Arc<PageScene>> {
        if let Some(scene) = self.scene_cache.get(&page_index) {
            return Ok(scene.clone());
        }
        let scene = match &self.source {
            ViewerSource::Empty => return Err(ibis_pdf::Error::new("empty pdf viewer")),
            ViewerSource::Pdf(container) => {
                let mut guard = container.lock().expect("pdf container lock poisoned");
                Arc::new(interpret_page_scene(&mut guard, page_index)?)
            }
            ViewerSource::Ofd(ofd) => Arc::new(
                ofd.interpret_page_scene(page_index)
                    .map_err(|e| ibis_pdf::Error::new(e.to_string()))?,
            ),
            ViewerSource::Xps(xps) => Arc::new(
                xps.interpret_page_scene(page_index)
                    .map_err(|e| ibis_pdf::Error::new(e.to_string()))?,
            ),
        };
        self.scene_cache.insert(page_index, scene.clone());
        Ok(scene)
    }

    /// The page's text with the place of each character (empty when a page cannot be read).
    fn text_index(&mut self, page: usize) -> Arc<PageScene> {
        self.page_scene(page).unwrap_or_else(|_| {
            let info = self.pages.get(page).cloned().expect("a page of the document");
            Arc::new(PageScene { info, display_list: Vec::new(), text_index: Default::default() })
        })
    }

    /// The character at the point of a page (in page points), the nearest when none is under it.
    pub fn hit(&mut self, page: usize, x: f32, y: f32) -> Option<Pos> {
        if page >= self.pages.len() {
            return None;
        }
        let scene = self.text_index(page);
        scene.text_index.hit(x, y).map(|index| Pos { page, index })
    }

    /// The characters of the selection on `page`: the first and the last (both picked).
    pub fn selected_range(&mut self, page: usize) -> Option<(usize, usize)> {
        let (a, b) = self.selection?.ordered();
        if page < a.page || page > b.page {
            return None;
        }
        let len = self.text_index(page).text_index.chars.len();
        if len == 0 {
            return None;
        }
        let first = if page == a.page { a.index } else { 0 };
        let last = if page == b.page { b.index } else { len - 1 };
        Some((first.min(len - 1), last.min(len - 1)))
    }

    /// The text of the selection, a line break between lines and a blank line between pages.
    pub fn selected_text(&mut self) -> String {
        let Some(sel) = self.selection else { return String::new() };
        let (a, b) = sel.ordered();
        let mut parts = Vec::new();
        for page in a.page..=b.page.min(self.pages.len().saturating_sub(1)) {
            if let Some((first, last)) = self.selected_range(page) {
                parts.push(self.text_index(page).text_index.text_between(first, last));
            }
        }
        parts.join("\n\n")
    }

    /// Everything on the pages is picked.
    pub fn select_all(&mut self) {
        let (mut first, mut last) = (None, None);
        for page in 0..self.pages.len() {
            let n = self.text_index(page).text_index.chars.len();
            if n > 0 {
                first.get_or_insert(Pos { page, index: 0 });
                last = Some(Pos { page, index: n - 1 });
            }
        }
        self.selection = first.zip(last).map(|(anchor, focus)| Selection { anchor, focus });
    }

    /// Look for `query` on every page (without regard to case): the places are kept in the order of the pages. No place is the current one.
    pub fn run_find(&mut self, query: &str) {
        self.find_matches.clear();
        self.find_current = None;
        if query.is_empty() {
            return;
        }
        for page in 0..self.pages.len() {
            let found = self.text_index(page).text_index.find(query, false);
            self.find_matches.extend(found.into_iter().map(|(start, end)| FindMatch { page, start, end }));
        }
    }

    /// The next (or the previous) place the search found, round the end; the index of it.
    pub fn step_find(&mut self, forward: bool) -> Option<usize> {
        let n = self.find_matches.len();
        if n == 0 {
            return None;
        }
        let next = match (self.find_current, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        self.find_current = Some(next);
        Some(next)
    }

    /// The box of a character of a page, in page points.
    pub fn char_rect(&mut self, page: usize, index: usize) -> Option<ibis_pdf::PdfRect> {
        self.text_index(page).text_index.chars.get(index).map(|c| c.rect_pt)
    }

    /// Paint a single page at `origin` using `scale` (pixels per PDF point).
    pub fn paint_page(
        &mut self,
        page_index: usize,
        origin: Point<Pixels>,
        scale: f32,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<()> {
        let scene = self.page_scene(page_index)?;
        let bounds = Bounds::new(
            origin,
            size(
                px(scene.info.crop_box.width() * scale),
                px(scene.info.crop_box.height() * scale),
            ),
        );
        render::paint_page_scene(
            page_index,
            &scene,
            bounds,
            scale,
            &mut self.image_cache,
            window,
            cx,
        );
        Ok(())
    }
}

/// The status bar's words for `n` signatures (and for an OFD's, what they were found to be).
fn signature_text(n: usize, outcome: Option<ibis_ofd::SignatureOutcome>) -> String {
    use ibis_ofd::SignatureOutcome;
    use ibis_office_shell::i18n::format;
    let id = match outcome {
        None => "shell.pdf.signatures",
        Some(SignatureOutcome::Mismatch) => "shell.pdf.signatures-mismatch",
        Some(SignatureOutcome::Verified) => "shell.pdf.signatures-verified",
        Some(SignatureOutcome::DigestsMatch) => "shell.pdf.signatures-digests",
    };
    format(id, &[("count", &n)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};

    /// The characters of a run of an XPS each have a box of their own along it (a drag picks the ones it passes).
    #[test]
    fn the_characters_of_an_xps_run_are_apart() {
        let mut w = Writer::new();
        let meta = EntryMeta::default();
        w.add("FixedDocSeq.fdseq", meta, br#"<?xml version="1.0" encoding="utf-8"?><FixedDocumentSequence xmlns="http://schemas.microsoft.com/xps/2005/06"><DocumentReference Source="Documents/1/FixedDoc.fdoc"/></FixedDocumentSequence>"#).unwrap();
        w.add("Documents/1/FixedDoc.fdoc", meta, br#"<?xml version="1.0" encoding="utf-8"?><FixedDocument xmlns="http://schemas.microsoft.com/xps/2005/06"><PageContent Source="Pages/1.fpage"/></FixedDocument>"#).unwrap();
        w.add("Documents/1/Pages/1.fpage", meta, r##"<?xml version="1.0" encoding="utf-8"?><FixedPage Width="793.7" Height="1122.5" xmlns="http://schemas.microsoft.com/xps/2005/06"><Glyphs OriginX="50" OriginY="80" FontRenderingEmSize="16" Fill="#FF112233" UnicodeString="XPSテスト"/></FixedPage>"##.as_bytes()).unwrap();
        let mut viewer = PdfViewerState::open(w.finish().unwrap()).unwrap();
        let xs: Vec<f32> = (0..6).filter_map(|i| viewer.char_rect(0, i)).map(|r| r.x0).collect();
        assert_eq!(xs.len(), 6);
        assert!(xs.windows(2).all(|p| p[1] > p[0]), "{xs:?}");
    }
}
