// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A presentation as one standalone HTML page (File > Export > Web page, GitHub issue #208): a `<section>` for each slide that is
//! shown (hidden slides are left out), in the slide's proportions, with every shape put where it is by percentages of the slide so
//! the page scales with the window. Text is real text (paragraphs, character formats, bullets, links), basic shapes are SVG,
//! tables are `<table>`s, pictures are embedded as data URIs (written once for the page and shared by the slides that use them).
//! Charts, SmartArt and the other objects the page cannot draw keep their place as a dashed frame with a label.
//! The notes, the animations and the transitions are not written.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ibis_doc_model::{
    Color, Fill, Geometry, LineDash, LineSpacing, MediaFormat, MediaId, Paragraph, Placeholder, Presentation, RunStyle, Shape, ShapeKind, Slide, SlideLink,
    Stroke, TextAlign, TextBody, VerticalAnchor,
};
use ibis_doc_model::shape_geometry::PathFill;

/// The words the page writes itself (the label of an object it cannot draw, the title of a slide), in the language of the UI.
#[derive(Clone, Debug)]
pub struct Labels {
    pub chart: String,
    pub diagram: String,
    pub picture: String,
    pub object: String,
    /// The `aria-label` of a slide: `{n}` is its number.
    pub slide: String,
}

impl Default for Labels {
    fn default() -> Self {
        Self { chart: "Chart".into(), diagram: "SmartArt".into(), picture: "Picture".into(), object: "Object".into(), slide: "Slide {n}".into() }
    }
}

/// How big the page may be written: a presentation with more picture bytes than this is not turned into one page.
pub const MAX_MEDIA_BYTES: usize = 200 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum SlidesHtmlError {
    /// Every slide is hidden, or there is none.
    NoSlides,
    /// The pictures the slides show together take more than [`MAX_MEDIA_BYTES`].
    TooLarge { bytes: usize },
}

/// What draws a shape the page cannot (a chart, SmartArt, the other objects) as a picture, with the shapes attached to it (a SmartArt graphic's shapes are its members, drawn in
/// its picture and left out of the page: [`ibis_doc_model::Slide::attached_to`]; the shapes over a chart stay on the page, over its picture): PNG bytes, or `None` where the shape cannot be drawn (the page keeps its dashed frame and label then).
pub type Raster<'a> = &'a dyn Fn(&Shape, &[Shape]) -> Option<Vec<u8>>;

/// The shapes the page cannot draw itself: the ones [`slides_to_html_with`] asks its [`Raster`] for. Of the slides that are shown, in order.
pub fn shapes_needing_pictures(presentation: &Presentation) -> Vec<(ibis_doc_model::SlideId, ibis_doc_model::ShapeId)> {
    let mut out = Vec::new();
    for slide in presentation.slides.iter().filter(|s| !s.hidden) {
        for shape in &slide.shapes {
            if needs_picture(shape) {
                out.push((slide.id, shape.id));
            }
        }
    }
    out
}

/// Whether the page draws `shape` as a picture the caller made (not as text, SVG, a table or a picture of the file).
fn needs_picture(shape: &Shape) -> bool {
    matches!(shape.kind, ShapeKind::Chart(_) | ShapeKind::Placeholder(_) | ShapeKind::Slicer(_) | ShapeKind::Timeline(_) | ShapeKind::Control(_))
}

/// The presentation as an HTML page. `title` is the page's title, `lang` its language (`ja`, `en`).
pub fn slides_to_html(presentation: &Presentation, title: &str, lang: &str, labels: &Labels) -> Result<String, SlidesHtmlError> {
    slides_to_html_with(presentation, title, lang, labels, &|_, _| None)
}

/// [`slides_to_html`] with the pictures of the shapes the page cannot draw (charts, SmartArt…): each is an `<img>` with its PNG as a data URI where `raster` made one.
pub fn slides_to_html_with(presentation: &Presentation, title: &str, lang: &str, labels: &Labels, raster: Raster<'_>) -> Result<String, SlidesHtmlError> {
    let shown: Vec<usize> = presentation.slides.iter().enumerate().filter(|(_, s)| !s.hidden).map(|(i, _)| i).collect();
    if shown.is_empty() {
        return Err(SlidesHtmlError::NoSlides);
    }
    let mut page = Page { presentation, labels, media: BTreeMap::new(), shown: &shown, defs: 0, raster, skip: Default::default() };
    let mut body = String::new();
    for (n, &index) in shown.iter().enumerate() {
        page.slide(&mut body, &presentation.slides[index], index, n + 1);
    }
    let mut bytes = 0usize;
    let mut styles = String::new();
    for (id, mime) in &page.media {
        if let Some(media) = presentation.media(*id) {
            bytes += media.data.len();
            let _ = write!(styles, ".m{}{{background-image:url(data:{};base64,", id.0, mime);
            base64(&media.data, &mut styles);
            styles.push_str(")}\n");
        }
    }
    if bytes > MAX_MEDIA_BYTES {
        return Err(SlidesHtmlError::TooLarge { bytes });
    }
    let mut out = String::with_capacity(body.len() + styles.len() + 2048);
    let _ = write!(
        out,
        "<!DOCTYPE html>\n<html lang=\"{lang}\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n<title>{title}</title>\n<style>\n{base}{styles}</style>\n</head>\n<body>\n{body}<div id=\"show-number\" aria-hidden=\"true\"></div>\n<script>\n{script}</script>\n</body>\n</html>\n",
        lang = escape(lang, true),
        title = escape(title, false),
        base = BASE_CSS,
        script = SHOW_SCRIPT,
    );
    Ok(out)
}

const BASE_CSS: &str = "\
body{margin:0;padding:24px 0;background:#e8e8ec}
.slide{position:relative;width:min(calc(100% - 32px),1280px);margin:0 auto 24px;container-type:inline-size;overflow:hidden;background:#fff;box-shadow:0 1px 6px rgba(0,0,0,.25);background-size:100% 100%;background-repeat:no-repeat}
.slide *{box-sizing:border-box}
.s{position:absolute}
.s>svg{position:absolute;left:0;top:0;width:100%;height:100%;overflow:visible}
.pic,.fill{position:absolute;left:0;top:0;width:100%;height:100%;background-repeat:no-repeat}
.t{position:absolute;display:flex;flex-direction:column;overflow:visible}
.t p{margin:0;white-space:pre-wrap;overflow-wrap:break-word}
.t a,.tb a{color:inherit;text-decoration:inherit}
.bu{display:inline-block}
table.tb{position:absolute;left:0;top:0;width:100%;border-collapse:collapse;table-layout:fixed}
table.tb td{padding:0;overflow:hidden}
.obj{position:absolute;left:0;top:0;width:100%;height:100%;object-fit:contain}
.ph{position:absolute;left:0;top:0;width:100%;height:100%;display:flex;align-items:center;justify-content:center;border:1px dashed #8a8a8a;color:#6b6b6b;font:12px/1.2 sans-serif;text-align:center;background:rgba(0,0,0,.03)}
@media screen{
html.show,html.show body{height:100%;overflow:hidden;background:#000}
html.show body{padding:0}
html.show .slide{display:none;margin:0;box-shadow:none}
html.show .slide.cur{display:block;position:fixed;left:50%;top:50%;transform:translate(-50%,-50%);width:min(100vw,calc(100vh * var(--r)))}
html:not(.show) .slide{cursor:zoom-in}
}
#show-number{display:none}
@media screen{html.show #show-number{display:block;position:fixed;right:12px;bottom:8px;color:#fff;font:12px/1 sans-serif;opacity:.5;pointer-events:none}}
@media print{#show-number{display:none}body{padding:0;background:#fff}.slide{width:100%;margin:0;box-shadow:none;break-after:page}}
";

/// The slide show (GitHub issue #208): the page opens on the first slide filling the window (the slide's proportions kept, black
/// where it does not reach); the next slide by → ↓ Space Enter PageDown, a click or a tap, a swipe to the left; the previous by
/// ← ↑ Backspace PageUp or a swipe to the right; Home and End go to the first and the last; F asks the browser for its full screen;
/// Esc leaves the show for the list of slides (the page as it was written before), where a click on a slide or Enter starts it again.
/// Without script the page is that list. The script reads nothing from outside the page.
const SHOW_SCRIPT: &str = "\
(function(){
var root=document.documentElement;
var slides=Array.prototype.slice.call(document.querySelectorAll('section.slide'));
var badge=document.getElementById('show-number');
var cur=0;
function clamp(n){return Math.max(0,Math.min(slides.length-1,n));}
function paint(){
slides.forEach(function(s,i){s.classList.toggle('cur',i===cur);});
if(badge){badge.textContent=(cur+1)+' / '+slides.length;}
try{history.replaceState(null,'','#slide-'+(cur+1));}catch(e){}
}
function go(n){cur=clamp(n);paint();}
function inShow(){return root.classList.contains('show');}
function startShow(n){root.classList.add('show');go(n);}
function leaveShow(){
root.classList.remove('show');
if(document.fullscreenElement&&document.exitFullscreen){document.exitFullscreen();}
slides[cur].scrollIntoView();
}
function toggleFullscreen(){
if(document.fullscreenElement){document.exitFullscreen();}
else if(root.requestFullscreen){root.requestFullscreen();}
}
function onKey(e){
if(e.ctrlKey||e.metaKey||e.altKey){return;}
var k=e.key;
if(!inShow()){
if(k==='Enter'){startShow(cur);e.preventDefault();}
return;
}
if(k==='ArrowRight'||k==='ArrowDown'||k===' '||k==='Spacebar'||k==='Enter'||k==='PageDown'){go(cur+1);}
else if(k==='ArrowLeft'||k==='ArrowUp'||k==='Backspace'||k==='PageUp'){go(cur-1);}
else if(k==='Home'){go(0);}
else if(k==='End'){go(slides.length-1);}
else if(k==='Escape'){leaveShow();}
else if(k==='f'||k==='F'){toggleFullscreen();}
else{return;}
e.preventDefault();
}
function onClick(e){
if(e.target.closest&&e.target.closest('a')){return;}
if(inShow()){go(cur+1);return;}
var s=e.target.closest&&e.target.closest('section.slide');
if(s){startShow(slides.indexOf(s));}
}
var sx=0,sy=0,swiped=false;
function onTouchStart(e){var t=e.changedTouches[0];sx=t.clientX;sy=t.clientY;swiped=false;}
function onTouchEnd(e){
if(!inShow()){return;}
var t=e.changedTouches[0],dx=t.clientX-sx,dy=t.clientY-sy;
if(Math.abs(dx)>40&&Math.abs(dx)>Math.abs(dy)){swiped=true;go(dx<0?cur+1:cur-1);e.preventDefault();}
}
document.addEventListener('keydown',onKey);
document.addEventListener('touchstart',onTouchStart,{passive:true});
document.addEventListener('touchend',onTouchEnd,{passive:false});
document.addEventListener('click',function(e){if(swiped){swiped=false;return;}onClick(e);});
var m=/^#slide-(\\d+)$/.exec(location.hash);
startShow(m?parseInt(m[1],10)-1:0);
})();
";

struct Page<'a> {
    presentation: &'a Presentation,
    labels: &'a Labels,
    /// The pictures used, with the media type of each.
    media: BTreeMap<MediaId, &'static str>,
    shown: &'a [usize],
    /// The gradients written so far (for their ids).
    defs: usize,
    raster: Raster<'a>,
    /// The shapes of the slide being written that a SmartArt picture already holds.
    skip: std::collections::HashSet<ibis_doc_model::ShapeId>,
}

/// A slide's size in EMU, to turn lengths into percentages of it.
#[derive(Clone, Copy)]
struct Scale {
    w: f64,
    h: f64,
}

impl Scale {
    fn x(self, emu: i64) -> f64 {
        emu as f64 * 100.0 / self.w
    }
    fn y(self, emu: i64) -> f64 {
        emu as f64 * 100.0 / self.h
    }
    /// A length as `cqw`: percent of the slide's width, which the slide's container makes the unit of lengths (sizes of text, insets).
    fn len(self, emu: i64) -> String {
        format!("{}cqw", num(self.x(emu)))
    }
    fn pt(self, hundredths: f64) -> String {
        self.len((hundredths / 100.0 * 12_700.0).round() as i64)
    }
}

impl Page<'_> {
    fn slide(&mut self, out: &mut String, slide: &Slide, index: usize, number: usize) {
        let size = slide.size.unwrap_or(self.presentation.slide_size);
        let scale = Scale { w: size.width.max(1) as f64, h: size.height.max(1) as f64 };
        let mut style = format!("aspect-ratio:{}/{};--r:{}", size.width.max(1), size.height.max(1), num(scale.w / scale.h));
        match &slide.background {
            Fill::None => {}
            fill => {
                if let Some(media) = fill.media() {
                    if self.use_media(media) {
                        return self.slide_body(out, slide, index, number, scale, style, Some(media));
                    }
                } else if let Some(css) = css_fill(fill) {
                    let _ = write!(style, ";background:{css}");
                }
            }
        }
        self.slide_body(out, slide, index, number, scale, style, None);
    }

    fn slide_body(&mut self, out: &mut String, slide: &Slide, index: usize, number: usize, scale: Scale, style: String, background: Option<MediaId>) {
        let label = self.labels.slide.replace("{n}", &(index + 1).to_string());
        let class = background.map_or("slide".to_string(), |m| format!("slide m{}", m.0));
        let _ = writeln!(out, "<section class=\"{class}\" id=\"slide-{number}\" aria-label=\"{}\" style=\"{style}\">", escape(&label, true));
        if !slide.hide_backdrop {
            for shape in &slide.backdrop {
                self.shape(out, slide, shape, scale, false);
            }
        }
        self.skip.clear();
        for shape in &slide.shapes {
            if slide.hidden_shapes.contains(&shape.id) || self.skip.contains(&shape.id) {
                continue;
            }
            self.shape(out, slide, shape, scale, true);
        }
        out.push_str("</section>\n");
    }

    /// Note a picture as used; `false` when this page cannot show it (a metafile, a video).
    fn use_media(&mut self, id: MediaId) -> bool {
        let Some(media) = self.presentation.media(id) else { return false };
        let Some(mime) = image_mime(media.format) else { return false };
        self.media.insert(id, mime);
        true
    }

    fn shape(&mut self, out: &mut String, slide: &Slide, shape: &Shape, scale: Scale, own: bool) {
        let alt = if own { slide.shape_alt_texts.get(&shape.id).map(|a| a.description.as_str()).unwrap_or("") } else { "" };
        let decorative = own && slide.decorative_shapes.contains(&shape.id);
        let f = shape.frame;
        let mut style = format!("left:{}%;top:{}%;width:{}%;height:{}%", num(scale.x(f.x)), num(scale.y(f.y)), num(scale.x(f.width)), num(scale.y(f.height)));
        if shape.rotation % 21_600_000 != 0 {
            let _ = write!(style, ";transform:rotate({}deg)", num(shape.rotation as f64 / 60_000.0));
        }
        let link = if own { slide.shape_links.get(&shape.id).and_then(|l| self.href(l)) } else { None };
        if let Some(href) = &link {
            let _ = write!(out, "<a href=\"{}\">", escape(href, true));
        }
        let _ = write!(out, "<div class=\"s\" style=\"{style}\"");
        if decorative {
            out.push_str(" aria-hidden=\"true\"");
        }
        out.push('>');
        let flip = flip_transform(shape);
        let (w, h) = (f.width.max(1) as f64, f.height.max(1) as f64);
        match &shape.kind {
            ShapeKind::TextBox(body) => self.text(out, body, scale, (0.0, 0.0, w, h), (w, h), shape),
            ShapeKind::Auto(auto) => {
                self.graphic(out, &auto.geometry, &auto.fill, auto.stroke.as_ref(), (w, h), &flip, slide, shape);
                if let Some(body) = &auto.text {
                    self.text(out, body, scale, auto.geometry.text_rect(w, h), (w, h), shape);
                }
            }
            ShapeKind::Line(stroke) => {
                let (x1, x2) = if shape.flip_h { (w, 0.0) } else { (0.0, w) };
                let (y1, y2) = if shape.flip_v { (h, 0.0) } else { (0.0, h) };
                let _ = write!(out, "<svg viewBox=\"0 0 {} {}\" preserveAspectRatio=\"none\" aria-hidden=\"true\"><line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{}/></svg>", num(w), num(h), num(x1), num(y1), num(x2), num(y2), svg_stroke(stroke));
            }
            ShapeKind::Picture(picture) => {
                if self.use_media(picture.media) {
                    let crop = picture.crop;
                    let (l, t, r, b) = (crop.left as f64, crop.top as f64, crop.right as f64, crop.bottom as f64);
                    let (dw, dh) = ((100_000.0 - l - r).max(1.0), (100_000.0 - t - b).max(1.0));
                    let pos = |a: f64, b: f64| if (a + b).abs() < 1.0 { 0.0 } else { a / (a + b) * 100.0 };
                    let mut s = format!("background-size:{}% {}%;background-position:{}% {}%", num(100_000.0 / dw * 100.0), num(100_000.0 / dh * 100.0), num(pos(l, r)), num(pos(t, b)));
                    if !flip.is_empty() {
                        let _ = write!(s, ";transform:{flip}");
                    }
                    if matches!(picture.geometry, Some(Geometry::Ellipse)) {
                        s.push_str(";border-radius:50%");
                    } else if picture.corner > 0 {
                        let _ = write!(s, ";border-radius:{}", scale.len((f.width.min(f.height) as f64 * picture.corner as f64 / 100_000.0) as i64));
                    }
                    if let Some(stroke) = &picture.stroke {
                        let _ = write!(s, ";box-shadow:inset 0 0 0 {} {}", scale.len(stroke.width), css_color(stroke.color));
                    }
                    let alt = if alt.is_empty() { picture.alt_text.description.as_str() } else { alt };
                    let hidden = if decorative || picture.decorative { " aria-hidden=\"true\"" } else { "" };
                    let _ = write!(out, "<div class=\"pic m{}\" role=\"img\" aria-label=\"{}\" style=\"{s}\"{hidden}></div>", picture.media.0, escape(alt, true));
                } else {
                    self.placeholder(out, if alt.is_empty() { &self.labels.picture } else { alt });
                }
            }
            ShapeKind::Table(table) => self.table(out, table, scale, f.height),
            _ if needs_picture(shape) && self.picture_of(out, slide, shape, alt, own) => {}
            ShapeKind::Chart(_) => self.placeholder(out, if alt.is_empty() { &self.labels.chart } else { alt }),
            ShapeKind::Placeholder(p) => {
                let name = match p {
                    Placeholder::Chart => &self.labels.chart,
                    Placeholder::Diagram => &self.labels.diagram,
                    Placeholder::Table | Placeholder::Picture(_) => &self.labels.object,
                };
                self.placeholder(out, if alt.is_empty() { name } else { alt });
            }
            _ => self.placeholder(out, if alt.is_empty() { &self.labels.object } else { alt }),
        }
        out.push_str("</div>");
        if link.is_some() {
            out.push_str("</a>");
        }
        out.push('\n');
    }

    /// The picture the caller made of `shape` (a chart, SmartArt…) as an `<img>` of the shape's box; `false` where it made none. Its `alt` is the shape's own, else its title or texts
    /// ([`Shape::picture_label`]), else the name of its kind. A SmartArt graphic's picture holds the shapes attached to it, which the page then leaves out.
    fn picture_of(&mut self, out: &mut String, slide: &Slide, shape: &Shape, alt: &str, own: bool) -> bool {
        let is_diagram = matches!(shape.kind, ShapeKind::Placeholder(Placeholder::Diagram));
        let members: Vec<Shape> = if own && is_diagram { slide.attached_to(shape.id).into_iter().filter_map(|id| slide.shape(id).cloned()).collect() } else { Vec::new() };
        let Some(png) = (self.raster)(shape, &members) else { return false };
        self.skip.extend(members.iter().map(|m| m.id));
        let described = shape.picture_label(&members);
        let label = if !alt.is_empty() {
            alt
        } else if let Some(text) = &described {
            text
        } else {
            match &shape.kind {
                ShapeKind::Chart(_) | ShapeKind::Placeholder(Placeholder::Chart) => &self.labels.chart,
                ShapeKind::Placeholder(Placeholder::Diagram) => &self.labels.diagram,
                _ => &self.labels.object,
            }
        };
        let _ = write!(out, "<img class=\"obj\" alt=\"{}\" src=\"data:image/png;base64,", escape(label, true));
        base64(&png, out);
        out.push_str("\">");
        true
    }

    fn placeholder(&self, out: &mut String, label: &str) {
        let _ = write!(out, "<div class=\"ph\" role=\"img\" aria-label=\"{0}\">{0}</div>", escape(label, true));
    }

    /// The shape drawn with SVG (or, for a picture fill, the picture behind it).
    #[allow(clippy::too_many_arguments)]
    fn graphic(&mut self, out: &mut String, geometry: &Geometry, fill: &Fill, stroke: Option<&Stroke>, (w, h): (f64, f64), flip: &str, _slide: &Slide, _shape: &Shape) {
        let mut defs = String::new();
        let mut paint = "none".to_string();
        let mut opacity = String::new();
        match fill {
            Fill::None => {}
            Fill::Solid(c) => {
                paint = format!("#{:06x}", c.rgb.0);
                if c.alpha < 255 {
                    opacity = format!(" fill-opacity=\"{}\"", num(c.alpha as f64 / 255.0));
                }
            }
            Fill::Linear { angle, stops, .. } | Fill::TileLinear { angle, stops, .. } => {
                self.defs += 1;
                let id = format!("g{}", self.defs);
                let a = *angle as f64 / 60_000.0 * std::f64::consts::PI / 180.0;
                let (dx, dy) = (a.cos() / 2.0, a.sin() / 2.0);
                let _ = write!(defs, "<linearGradient id=\"{id}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">", num(0.5 - dx), num(0.5 - dy), num(0.5 + dx), num(0.5 + dy));
                for s in stops {
                    let _ = write!(defs, "<stop offset=\"{}\" stop-color=\"#{:06x}\" stop-opacity=\"{}\"/>", num(s.position as f64 / 100_000.0), s.color.rgb.0, num(s.color.alpha as f64 / 255.0));
                }
                defs.push_str("</linearGradient>");
                paint = format!("url(#{id})");
            }
            Fill::Path { stops, .. } => {
                self.defs += 1;
                let id = format!("g{}", self.defs);
                let _ = write!(defs, "<radialGradient id=\"{id}\">");
                for s in stops {
                    let _ = write!(defs, "<stop offset=\"{}\" stop-color=\"#{:06x}\" stop-opacity=\"{}\"/>", num(s.position as f64 / 100_000.0), s.color.rgb.0, num(s.color.alpha as f64 / 255.0));
                }
                defs.push_str("</radialGradient>");
                paint = format!("url(#{id})");
            }
            Fill::Pattern { foreground, background, .. } => {
                // A pattern's two colours mixed evenly stand in for it.
                let mix = |a: u8, b: u8| ((a as u16 + b as u16) / 2) as u8;
                paint = format!("#{:02x}{:02x}{:02x}", mix(foreground.rgb.r(), background.rgb.r()), mix(foreground.rgb.g(), background.rgb.g()), mix(foreground.rgb.b(), background.rgb.b()));
            }
            Fill::Image(_) | Fill::TileImage { .. } | Fill::Blip(_) => {
                if let Some(media) = fill.media() {
                    if self.use_media(media) {
                        let radius = match geometry {
                            Geometry::Ellipse => ";border-radius:50%".to_string(),
                            Geometry::RoundRect { radius } => {
                                let r = w.min(h) * *radius as f64 / 100_000.0;
                                format!(";border-radius:{}% / {}%", num(r / w * 100.0), num(r / h * 100.0))
                            }
                            _ => String::new(),
                        };
                        let size = if matches!(fill, Fill::TileImage { .. }) { "" } else { "background-size:100% 100%" };
                        let _ = write!(out, "<div class=\"fill m{}\" style=\"{size}{radius}\"></div>", media.0);
                    }
                }
            }
        }
        let stroke_attr = stroke.map(svg_stroke).unwrap_or_default();
        let common = format!(" fill=\"{paint}\"{opacity}{stroke_attr}");
        let mut shapes = String::new();
        match geometry {
            Geometry::Rect => {
                let _ = write!(shapes, "<rect width=\"{}\" height=\"{}\"{common}/>", num(w), num(h));
            }
            Geometry::RoundRect { radius } => {
                let r = w.min(h) * *radius as f64 / 100_000.0;
                let _ = write!(shapes, "<rect width=\"{}\" height=\"{}\" rx=\"{}\"{common}/>", num(w), num(h), num(r));
            }
            Geometry::Ellipse => {
                let _ = write!(shapes, "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"{common}/>", num(w / 2.0), num(h / 2.0), num(w / 2.0), num(h / 2.0));
            }
            Geometry::Formula(formula) => {
                for outline in formula.outline(w, h) {
                    let mut d = String::new();
                    for sub in &outline.subpaths {
                        for (i, (x, y)) in sub.points.iter().enumerate() {
                            let _ = write!(d, "{}{} {} ", if i == 0 { "M" } else { "L" }, num(*x), num(*y));
                        }
                        if sub.closed {
                            d.push('Z');
                        }
                    }
                    if d.is_empty() {
                        continue;
                    }
                    let fill_attr = if outline.fill == PathFill::None { " fill=\"none\"".to_string() } else { format!(" fill=\"{paint}\"{opacity}") };
                    let stroke_part = if outline.stroke { stroke_attr.clone() } else { String::new() };
                    let _ = write!(shapes, "<path d=\"{}\" fill-rule=\"evenodd\"{fill_attr}{stroke_part}/>", d.trim_end());
                }
            }
        }
        let style = if flip.is_empty() { String::new() } else { format!(" style=\"transform:{flip}\"") };
        let _ = write!(out, "<svg viewBox=\"0 0 {} {}\" preserveAspectRatio=\"none\" aria-hidden=\"true\"{style}>", num(w), num(h));
        if !defs.is_empty() {
            let _ = write!(out, "<defs>{defs}</defs>");
        }
        out.push_str(&shapes);
        out.push_str("</svg>");
    }

    /// The text of a shape in the rectangle `(left, top, right, bottom)` of it (EMU).
    fn text(&mut self, out: &mut String, body: &TextBody, scale: Scale, rect: (f64, f64, f64, f64), (w, h): (f64, f64), shape: &Shape) {
        let _ = shape;
        if body.paragraphs.iter().all(|p| p.runs.iter().all(|r| r.text.is_empty())) && body.paragraphs.len() <= 1 {
            return;
        }
        let (l, t, r, b) = rect;
        if r <= l || b <= t {
            return;
        }
        let pad = |emu: i64| scale.len(emu);
        let mut style = format!(
            "left:{}%;top:{}%;width:{}%;height:{}%;padding:{} {} {} {}",
            num(l / w * 100.0),
            num(t / h * 100.0),
            num((r - l) / w * 100.0),
            num((b - t) / h * 100.0),
            pad(body.insets.top),
            pad(body.insets.right),
            pad(body.insets.bottom),
            pad(body.insets.left),
        );
        let justify = match body.anchor {
            VerticalAnchor::Top => "flex-start",
            VerticalAnchor::Middle => "center",
            VerticalAnchor::Bottom => "flex-end",
        };
        let _ = write!(style, ";justify-content:{justify}");
        if body.vertical {
            style.push_str(";writing-mode:vertical-rl");
        }
        if !body.wrap {
            style.push_str(";white-space:nowrap");
        }
        let _ = write!(out, "<div class=\"t\" style=\"{style}\">");
        for (i, paragraph) in body.paragraphs.iter().enumerate() {
            self.paragraph(out, body, paragraph, i, scale);
        }
        out.push_str("</div>");
    }

    fn paragraph(&mut self, out: &mut String, body: &TextBody, p: &Paragraph, index: usize, scale: Scale) {
        let font_scale = body.font_scale as f64 / 100_000.0;
        let size_of = |style: &RunStyle| style.font_size.unwrap_or(body.style.font_size) as f64 * font_scale;
        let first_size = p.runs.first().map_or_else(|| size_of(&p.end_style), |r| size_of(&r.style));
        let mut style = String::new();
        let align = match p.align.unwrap_or(body.style.align) {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
            TextAlign::Justify => "justify",
            TextAlign::Distributed => "justify;text-align-last:justify",
        };
        let _ = write!(style, "text-align:{align}");
        if p.margin_left != 0 {
            let _ = write!(style, ";margin-left:{}", scale.len(p.margin_left));
        }
        if p.indent != 0 {
            let _ = write!(style, ";text-indent:{}", scale.len(p.indent));
        }
        if p.space_before > 0 && (index > 0 || body.space_first_last) {
            let _ = write!(style, ";margin-top:{}", scale.pt(p.space_before as f64));
        }
        if p.space_after > 0 {
            let _ = write!(style, ";margin-bottom:{}", scale.pt(p.space_after as f64));
        }
        match p.line_spacing {
            Some(LineSpacing::Percent(percent)) => {
                let _ = write!(style, ";line-height:{}", num(percent as f64 / 100_000.0 * 1.2));
            }
            Some(LineSpacing::Points(points)) => {
                let _ = write!(style, ";line-height:{}", scale.pt(points as f64));
            }
            None => style.push_str(";line-height:1.2"),
        }
        // The paragraph's own size for the line's strut and the bullet.
        let _ = write!(style, ";font-size:{}", scale.pt(first_size));
        let _ = write!(out, "<p style=\"{style}\">");
        if let Some(bullet) = &p.bullet {
            if !bullet.text.is_empty() {
                let color = bullet.color.or(p.runs.first().and_then(|r| r.style.color)).unwrap_or(body.style.color);
                let mut s = format!("color:#{:06x};font-size:{}", color.0, scale.pt(first_size * bullet.size as f64 / 100_000.0));
                if p.indent < 0 {
                    let _ = write!(s, ";min-width:{}", scale.len(-p.indent));
                }
                let _ = write!(out, "<span class=\"bu\" style=\"{s}\">{}</span>", escape(&bullet.text, false));
            }
        }
        if p.runs.iter().all(|r| r.text.is_empty()) {
            out.push_str("<br>");
        }
        for run in &p.runs {
            if run.text.is_empty() {
                continue;
            }
            let rs = &run.style;
            let mut size = size_of(rs);
            let mut s = String::new();
            let mut decoration = Vec::new();
            if rs.baseline != 0 {
                size *= 0.65;
                let _ = write!(s, "vertical-align:{};", if rs.baseline > 0 { "super" } else { "sub" });
            }
            let _ = write!(s, "font-size:{};color:#{:06x}", scale.pt(size), rs.color.unwrap_or(body.style.color).0);
            if rs.bold.unwrap_or(body.style.bold) {
                s.push_str(";font-weight:700");
            }
            if rs.italic.unwrap_or(body.style.italic) {
                s.push_str(";font-style:italic");
            }
            if rs.underline {
                decoration.push("underline");
            }
            if !matches!(rs.strike, ibis_doc_model::Strike::None) {
                decoration.push("line-through");
            }
            if !decoration.is_empty() {
                let _ = write!(s, ";text-decoration:{}", decoration.join(" "));
            }
            if let Some(h) = rs.highlight {
                let _ = write!(s, ";background:#{:06x}", h.0);
            }
            if rs.spacing != 0 {
                let _ = write!(s, ";letter-spacing:{}", scale.pt(rs.spacing as f64));
            }
            match rs.caps {
                ibis_doc_model::Caps::None => {}
                ibis_doc_model::Caps::All => s.push_str(";text-transform:uppercase"),
                ibis_doc_model::Caps::Small => s.push_str(";font-variant:small-caps"),
            }
            if let Some(family) = self.font_family(rs, body) {
                let _ = write!(s, ";font-family:{family}");
            }
            let href = rs.link.as_ref().and_then(|l| self.href(l));
            if let Some(href) = &href {
                let _ = write!(out, "<a href=\"{}\">", escape(href, true));
            }
            let _ = write!(out, "<span style=\"{s}\">{}</span>", escape(&run.text, false));
            if href.is_some() {
                out.push_str("</a>");
            }
        }
        out.push_str("</p>");
    }

    fn font_family(&self, rs: &RunStyle, body: &TextBody) -> Option<String> {
        let theme = self.presentation.theme_fonts.as_ref();
        let mut names: Vec<&str> = Vec::new();
        for name in [rs.font.as_deref().or(body.style.font.as_deref()), rs.fallback_font.as_deref().or(body.style.fallback_font.as_deref())].into_iter().flatten() {
            names.push(name);
        }
        if let Some(theme) = theme {
            for name in [theme.minor.latin.as_deref(), theme.minor.east_asian.as_deref()].into_iter().flatten() {
                names.push(name);
            }
        }
        let mut out = Vec::new();
        for name in names {
            let clean: String = name.chars().filter(|c| !matches!(c, '\'' | '"' | ';' | '<' | '>' | '&' | '\\' | '{' | '}')).collect();
            if clean.is_empty() || clean.starts_with('+') || clean.starts_with('@') {
                continue;
            }
            let quoted = format!("'{clean}'");
            if !out.contains(&quoted) {
                out.push(quoted);
            }
        }
        if out.is_empty() { None } else { Some(format!("{},sans-serif", out.join(","))) }
    }

    /// Where a link goes in the page, if it can: web, mail and phone links, and the slides of this page.
    fn href(&self, link: &SlideLink) -> Option<String> {
        let slide_no = |index: usize| self.shown.iter().position(|&i| i == index).map(|n| format!("#slide-{}", n + 1));
        match link {
            SlideLink::Url(url) => {
                let lower = url.trim().to_ascii_lowercase();
                (lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:") || lower.starts_with("tel:")).then(|| url.trim().to_string())
            }
            SlideLink::Slide(id) => self.presentation.slide_index(*id).and_then(slide_no),
            SlideLink::FirstSlide => Some("#slide-1".into()),
            SlideLink::LastSlide => Some(format!("#slide-{}", self.shown.len())),
            SlideLink::NextSlide | SlideLink::PreviousSlide | SlideLink::LastSlideViewed | SlideLink::EndShow => None,
        }
    }

    fn table(&mut self, out: &mut String, table: &ibis_doc_model::Table, scale: Scale, frame_height: i64) {
        let total: i64 = table.columns.iter().sum::<i64>().max(1);
        let _ = write!(out, "<table class=\"tb\" style=\"height:{}%\"><colgroup>", num(table.rows.iter().map(|r| r.height).sum::<i64>() as f64 * 100.0 / frame_height.max(1) as f64));
        for c in &table.columns {
            let _ = write!(out, "<col style=\"width:{}%\">", num(*c as f64 * 100.0 / total as f64));
        }
        out.push_str("</colgroup>");
        for row in &table.rows {
            let _ = write!(out, "<tr style=\"height:{}\">", scale.len(row.height));
            for cell in &row.cells {
                if cell.merged {
                    continue;
                }
                let mut s = String::new();
                if let Some(css) = css_fill(&cell.fill) {
                    let _ = write!(s, "background:{css};");
                }
                for (side, stroke) in [("left", &cell.borders.left), ("right", &cell.borders.right), ("top", &cell.borders.top), ("bottom", &cell.borders.bottom)] {
                    if let Some(stroke) = stroke {
                        let _ = write!(s, "border-{side}:max(1px,{}) solid {};", scale.len(stroke.width), css_color(stroke.color));
                    }
                }
                let _ = write!(
                    s,
                    "padding:{} {} {} {};vertical-align:{}",
                    scale.len(cell.text.insets.top),
                    scale.len(cell.text.insets.right),
                    scale.len(cell.text.insets.bottom),
                    scale.len(cell.text.insets.left),
                    match cell.text.anchor {
                        VerticalAnchor::Top => "top",
                        VerticalAnchor::Middle => "middle",
                        VerticalAnchor::Bottom => "bottom",
                    }
                );
                let span = |name: &str, n: u32| if n > 1 { format!(" {name}=\"{n}\"") } else { String::new() };
                let _ = write!(out, "<td{}{} style=\"{s}\">", span("colspan", cell.column_span), span("rowspan", cell.row_span));
                for (i, p) in cell.text.paragraphs.iter().enumerate() {
                    self.paragraph(out, &cell.text, p, i, scale);
                }
                out.push_str("</td>");
            }
            out.push_str("</tr>");
        }
        out.push_str("</table>");
    }
}

fn flip_transform(shape: &Shape) -> String {
    match (shape.flip_h, shape.flip_v) {
        (false, false) => String::new(),
        (true, false) => "scaleX(-1)".into(),
        (false, true) => "scaleY(-1)".into(),
        (true, true) => "scale(-1,-1)".into(),
    }
}

fn svg_stroke(stroke: &Stroke) -> String {
    let width = stroke.width.max(stroke.min_drawn_width).max(1);
    if stroke.color.alpha == 0 && stroke.fill.is_none() {
        return " stroke=\"none\"".into();
    }
    let mut s = format!(" stroke=\"#{:06x}\" stroke-width=\"{}\"", stroke.color.rgb.0, width);
    if stroke.color.alpha < 255 {
        let _ = write!(s, " stroke-opacity=\"{}\"", num(stroke.color.alpha as f64 / 255.0));
    }
    let unit = width as f64;
    let dash: &[f64] = match stroke.dash {
        LineDash::Solid => &[],
        LineDash::Dot | LineDash::SysDot => &[1.0, 3.0],
        LineDash::Dash | LineDash::SysDash => &[4.0, 3.0],
        LineDash::LgDash => &[8.0, 3.0],
        LineDash::DashDot | LineDash::SysDashDot => &[4.0, 3.0, 1.0, 3.0],
        LineDash::LgDashDot => &[8.0, 3.0, 1.0, 3.0],
        LineDash::LgDashDotDot | LineDash::SysDashDotDot => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
    };
    if !dash.is_empty() {
        let list: Vec<String> = dash.iter().map(|d| num(d * unit)).collect();
        let _ = write!(s, " stroke-dasharray=\"{}\"", list.join(" "));
    }
    match stroke.cap {
        ibis_doc_model::LineCap::Round => s.push_str(" stroke-linecap=\"round\""),
        ibis_doc_model::LineCap::Square => s.push_str(" stroke-linecap=\"square\""),
        _ => {}
    }
    match stroke.join {
        ibis_doc_model::LineJoin::Round => s.push_str(" stroke-linejoin=\"round\""),
        ibis_doc_model::LineJoin::Bevel => s.push_str(" stroke-linejoin=\"bevel\""),
        _ => {}
    }
    s
}

fn css_color(c: Color) -> String {
    if c.alpha == 255 { format!("#{:06x}", c.rgb.0) } else { format!("rgba({},{},{},{})", c.rgb.r(), c.rgb.g(), c.rgb.b(), num(c.alpha as f64 / 255.0)) }
}

/// A fill as a CSS `background` value (a colour or a linear gradient); `None` for the fills it cannot say (pictures, patterns).
fn css_fill(fill: &Fill) -> Option<String> {
    match fill {
        Fill::Solid(c) => Some(css_color(*c)),
        Fill::Linear { angle, stops, .. } | Fill::TileLinear { angle, stops, .. } if !stops.is_empty() => {
            let list: Vec<String> = stops.iter().map(|s| format!("{} {}%", css_color(s.color), num(s.position as f64 / 1000.0))).collect();
            // OOXML's angle runs clockwise from left-to-right; CSS's from bottom-to-top.
            Some(format!("linear-gradient({}deg,{})", num(*angle as f64 / 60_000.0 + 90.0), list.join(",")))
        }
        Fill::Pattern { foreground, background, .. } => {
            let mix = |a: u8, b: u8| ((a as u16 + b as u16) / 2) as u8;
            Some(format!("#{:02x}{:02x}{:02x}", mix(foreground.rgb.r(), background.rgb.r()), mix(foreground.rgb.g(), background.rgb.g()), mix(foreground.rgb.b(), background.rgb.b())))
        }
        _ => None,
    }
}

fn image_mime(format: MediaFormat) -> Option<&'static str> {
    Some(match format {
        MediaFormat::Png => "image/png",
        MediaFormat::Jpeg => "image/jpeg",
        MediaFormat::Gif => "image/gif",
        MediaFormat::Bmp => "image/bmp",
        MediaFormat::Webp => "image/webp",
        MediaFormat::Svg => "image/svg+xml",
        _ => return None,
    })
}

/// A number written short (at most 3 decimals, no trailing zeros).
fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.into() }
}

fn escape(text: &str, attribute: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\'' if attribute => out.push_str("&#39;"),
            '\r' => {}
            '\n' if attribute => out.push(' '),
            '\u{b}' => out.push('\n'),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

fn base64(data: &[u8], out: &mut String) {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    out.reserve(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_doc_model::{AutoShape, Color, EmuRect, Rgb, Run, ShapeId, TextStyle};

    fn presentation() -> Presentation {
        Presentation::new()
    }

    fn text_shape(id: u64, x: i64, text: &str) -> Shape {
        Shape::text_box(ShapeId(id), EmuRect::new(x, 914_400, 3_657_600, 914_400), text)
    }

    #[test]
    fn one_section_per_shown_slide_keeps_the_proportions() {
        let mut p = presentation();
        let mut hidden = p.slides[0].clone();
        hidden.id = ibis_doc_model::SlideId(9_999);
        hidden.hidden = true;
        p.slides.push(hidden);
        p.slides[0].shapes.clear();
        p.slides[0].shapes.push(text_shape(1, 0, "Hello <b> & world"));
        let html = slides_to_html(&p, "Deck", "en", &Labels::default()).unwrap();
        assert_eq!(html.matches("<section").count(), 1, "the hidden slide is left out");
        assert!(html.contains(&format!("aspect-ratio:{}/{}", p.slide_size.width, p.slide_size.height)));
        assert!(html.contains("Hello &lt;b&gt; &amp; world"));
        assert!(html.contains("<title>Deck</title>"));
        assert!(html.contains("left:0%;top:13.333%;width:30%"), "{html}");
    }

    #[test]
    fn the_page_is_a_slide_show_with_keys_swipe_and_number() {
        let html = slides_to_html(&presentation(), "Deck", "en", &Labels::default()).unwrap();
        assert!(html.contains("<script>") && html.contains("</script>"));
        assert!(html.contains("function onKey(e)"), "the key handler");
        for key in ["ArrowRight", "ArrowDown", "' '", "Enter", "PageDown", "ArrowLeft", "ArrowUp", "Backspace", "PageUp", "Home", "End", "Escape"] {
            assert!(html.contains(key), "{key}");
        }
        assert!(html.contains("touchend") && html.contains("show-number"));
        assert!(html.contains("html.show .slide.cur"), "the shown slide fills the window");
        assert!(html.contains("--r:"), "the slide's ratio for the fit");
        assert!(!html.contains("src=\"http") && !html.contains("fetch("), "the script reads nothing from outside the page");
    }

    #[test]
    fn nothing_shown_is_an_error() {
        let mut p = presentation();
        p.slides[0].hidden = true;
        assert_eq!(slides_to_html(&p, "", "en", &Labels::default()), Err(SlidesHtmlError::NoSlides));
    }

    #[test]
    fn text_formats_links_and_bullets() {
        let mut p = presentation();
        let mut body = TextBody::plain("", TextStyle::default());
        body.paragraphs = vec![Paragraph {
            runs: vec![
                Run { text: "Bold".into(), style: RunStyle { bold: Some(true), color: Some(Rgb(0xff0000)), ..Default::default() } },
                Run { text: "Link".into(), style: RunStyle { underline: true, link: Some(SlideLink::Url("https://example.com/?a=1&b=2".into())), ..Default::default() } },
                Run { text: "Evil".into(), style: RunStyle { link: Some(SlideLink::Url("javascript:alert(1)".into())), ..Default::default() } },
            ],
            bullet: Some(ibis_doc_model::text::Bullet { text: "\u{2022}".into(), picture: None, size: 100_000, color: None, color_theme: None, auto: None, symbol: None, font: None }),
            ..Default::default()
        }];
        p.slides[0].shapes.push(Shape::new(ShapeId(1), EmuRect::new(0, 0, 3_000_000, 1_000_000), ShapeKind::TextBox(body)));
        let html = slides_to_html(&p, "t", "en", &Labels::default()).unwrap();
        assert!(html.contains("font-weight:700"));
        assert!(html.contains("color:#ff0000"));
        assert!(html.contains("<a href=\"https://example.com/?a=1&amp;b=2\">"));
        assert!(!html.contains("javascript:"));
        assert!(html.contains("class=\"bu\""));
    }

    #[test]
    fn shapes_are_svg_and_other_objects_keep_a_labelled_frame() {
        let mut p = presentation();
        let auto = AutoShape::new(Geometry::Ellipse, Fill::Solid(Color::opaque(Rgb(0x336699))), Some(Stroke::solid(Color::opaque(Rgb(0x112233)), 12_700)));
        p.slides[0].shapes.push(Shape::new(ShapeId(1), EmuRect::new(100_000, 100_000, 2_000_000, 1_000_000), ShapeKind::Auto(auto)));
        p.slides[0].shapes.push(Shape::new(ShapeId(2), EmuRect::new(0, 3_000_000, 2_000_000, 1_000_000), ShapeKind::Placeholder(Placeholder::Diagram)));
        let html = slides_to_html(&p, "t", "en", &Labels::default()).unwrap();
        assert!(html.contains("<ellipse"));
        assert!(html.contains("fill=\"#336699\""));
        assert!(html.contains("stroke=\"#112233\""));
        assert!(html.contains("aria-label=\"SmartArt\""));
    }

    #[test]
    fn a_picture_is_embedded_once_and_shared() {
        let mut p = presentation();
        let media = p.add_media(MediaFormat::Png, vec![0x89u8, b'P', b'N', b'G', 1, 2, 3]);
        for id in [1, 2] {
            let picture = ibis_doc_model::Picture::new(media);
            p.slides[0].shapes.push(Shape::new(ShapeId(id), EmuRect::new(0, 0, 1_000_000, 1_000_000), ShapeKind::Picture(picture)));
        }
        let html = slides_to_html(&p, "t", "en", &Labels::default()).unwrap();
        assert_eq!(html.matches("data:image/png;base64,").count(), 1);
        assert_eq!(html.matches("class=\"pic m").count(), 2);
    }

    #[test]
    fn a_real_deck_writes_a_page() {
        let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/pptx-plan.pptx")).unwrap();
        let p = crate::read(&data).unwrap();
        let html = slides_to_html(&p, "plan", "ja", &Labels::default()).unwrap();
        assert_eq!(html.matches("<section").count(), p.slides.iter().filter(|s| !s.hidden).count());
        assert!(html.len() > 2000);
    }
}
