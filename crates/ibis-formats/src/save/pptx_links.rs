// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ハイパーリンク: the relationships a slide's links need (an external
//! `hyperlink` for a web page or file, a `slide` one for a slide), and the
//! `a:hlinkClick` written for a link of a run or a shape. The writers of
//! runs find the relationship ids here while a slide is written
//! ([`with_link_ids`]).

use std::cell::RefCell;
use std::collections::HashMap;

use ibis_doc_model::{Presentation, Slide, SlideId, SlideLink};
use ibis_ooxml::Package;

use super::pptx::relative;
use super::pptx_new::Prefixes;
use crate::Error;

const REL_HYPERLINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
const REL_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";

thread_local! {
    static LINK_IDS: RefCell<HashMap<SlideLink, String>> = RefCell::new(HashMap::new());
}

/// Run `f` with `ids` the relationship ids of the slide being written.
pub(crate) fn with_link_ids<R>(ids: HashMap<SlideLink, String>, f: impl FnOnce() -> R) -> R {
    let old = LINK_IDS.with(|l| std::mem::replace(&mut *l.borrow_mut(), ids));
    let r = f();
    LINK_IDS.with(|l| *l.borrow_mut() = old);
    r
}

/// The links of `slide`: its runs' (in shapes and table cells) and its shapes'.
pub(crate) fn links_of(slide: &Slide) -> Vec<SlideLink> {
    let mut out: Vec<SlideLink> = slide.shape_links.values().cloned().collect();
    for shape in &slide.shapes {
        let mut bodies: Vec<&ibis_doc_model::TextBody> = shape.text_body().into_iter().collect();
        if let ibis_doc_model::ShapeKind::Table(t) = &shape.kind {
            bodies.extend(t.rows.iter().flat_map(|r| r.cells.iter().map(|c| &c.text)));
        }
        for body in bodies {
            out.extend(body.paragraphs.iter().flat_map(|p| p.runs.iter()).filter_map(|r| r.style.link.clone()));
        }
    }
    out.sort_by_key(|l| format!("{l:?}"));
    out.dedup();
    out
}

/// The relationship ids of slide part `part` for `links` (an existing
/// relationship to the same place is used again), `slides` giving each
/// slide's part.
pub(crate) fn link_rels(package: &mut Package, part: &str, links: &[SlideLink], slides: &HashMap<SlideId, String>) -> Result<HashMap<SlideLink, String>, Error> {
    let mut out = HashMap::new();
    let existing = package.relationships(part)?;
    for link in links {
        let (kind, target, external) = match link {
            SlideLink::Url(url) => (REL_HYPERLINK, url.clone(), true),
            SlideLink::Slide(id) => match slides.get(id) {
                Some(p) => (REL_SLIDE, relative(part, p), false),
                None => continue,
            },
            _ => continue,
        };
        let found = existing.iter().find(|r| {
            r.kind == kind
                && r.external == external
                && match link {
                    SlideLink::Url(url) => r.target == *url,
                    SlideLink::Slide(id) => r.target_part(part).as_ref() == slides.get(id),
                    _ => false,
                }
        });
        let id = match found {
            Some(r) => r.id.clone(),
            None => package.add_relationship(part, kind, &target, external)?,
        };
        out.insert(link.clone(), id);
    }
    Ok(out)
}

/// The slide parts of `current`'s slides, by the targets written.
pub(crate) fn slide_parts(current: &Presentation, parts: &[String]) -> HashMap<SlideId, String> {
    current.slides.iter().zip(parts).filter(|(_, p)| !p.is_empty()).map(|(s, p)| (s.id, p.clone())).collect()
}

/// The `a:hlinkClick` of `link` (its relationship from [`with_link_ids`]),
/// optionally with a tooltip (`tooltip="..."`).
pub(crate) fn hlink_xml(x: &Prefixes, link: &SlideLink, tooltip: Option<&str>) -> Option<String> {
    let (a, r) = (&x.a, &x.r);
    let tip = match tooltip.filter(|s| !s.is_empty()) {
        Some(t) => format!(" tooltip=\"{}\"", super::pptx_new::escape(t)),
        None => String::new(),
    };
    let jump = |j: &str| format!("<{a}hlinkClick {r}id=\"\" action=\"ppaction://hlinkshowjump?jump={j}\"{tip}/>");
    Some(match link {
        SlideLink::NextSlide => jump("nextslide"),
        SlideLink::PreviousSlide => jump("previousslide"),
        SlideLink::FirstSlide => jump("firstslide"),
        SlideLink::LastSlide => jump("lastslide"),
        SlideLink::LastSlideViewed => jump("lastslideviewed"),
        SlideLink::EndShow => jump("endshow"),
        SlideLink::Url(_) => format!("<{a}hlinkClick {r}id=\"{}\"{tip}/>", LINK_IDS.with(|l| l.borrow().get(link).cloned())?),
        SlideLink::Slide(_) => format!("<{a}hlinkClick {r}id=\"{}\" action=\"ppaction://hlinksldjump\"{tip}/>", LINK_IDS.with(|l| l.borrow().get(link).cloned())?),
    })
}
