// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pictures of Word's art page borders, for drawing: ECMA-376's
//! `OfficeOpenXML-WordprocessingMLArtBorders.zip`, shipped unmodified in
//! `data/artborders/` (the macOS bundle's `Contents/Resources/artborders/`,
//! the Linux AppImage's `usr/share/ibisOffice/artborders/`),
//! read when a document first uses an art border.
//!
//! The web version fetches the pictures one by one instead (see `web`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use gpui_kit::{Image, ImageFormat};
use ibis_doc_model::{ArtBorder, Rgb};
#[cfg(not(target_family = "wasm"))]
use ibis_pptx::art_borders::ArtBorderPictures;
use ibis_pptx::art_borders::ArtPicture;

pub use ibis_pptx::art_borders::ArtPart;

#[cfg(not(target_family = "wasm"))]
const ZIP: &str = "OfficeOpenXML-WordprocessingMLArtBorders.zip";

/// The zip's pictures, if the zip is found.
#[cfg(not(target_family = "wasm"))]
fn pictures() -> Option<&'static ArtBorderPictures> {
    static PICTURES: OnceLock<Option<ArtBorderPictures>> = OnceLock::new();
    PICTURES.get_or_init(load).as_ref()
}

#[cfg(not(target_family = "wasm"))]
fn load() -> Option<ArtBorderPictures> {
    let mut places = Vec::new();
    // In the app bundle: Contents/MacOS/ibisOffice → Contents/Resources.
    if let Some(dir) = std::env::current_exe().ok().as_deref().and_then(std::path::Path::parent) {
        places.push(dir.join("../Resources/artborders").join(ZIP));
        // Windows: Resources\artborders\ beside ibisOffice.exe.
        places.push(dir.join("Resources/artborders").join(ZIP));
        // Linux's AppImage: usr/bin/ibisOffice → usr/share/ibisOffice.
        places.push(dir.join("../share/ibisOffice/artborders").join(ZIP));
    }
    // A build run from the source tree (target/<dir>/release/ibisOffice, or
    // the tests' target/release/deps/…), found from the executable so that
    // no path of the build machine goes into it.
    if let Some(dir) = std::env::current_exe().ok().as_deref().and_then(std::path::Path::parent) {
        for up in ["../..", "../../..", "../../../.."] {
            places.push(dir.join(up).join("crates/ibis-pptx/data/artborders").join(ZIP));
        }
    }
    for place in places {
        let Ok(bytes) = std::fs::read(&place) else { continue };
        match ArtBorderPictures::new(bytes.into()) {
            Ok(pictures) => return Some(pictures),
            Err(err) => log::warn!("Cannot read the art border image ({}): {err}", place.display()),
        }
    }
    log::warn!("The art border image ({ZIP}) was not found");
    None
}

/// The web version: the pictures are served one by one, unmodified, under
/// `<base>/assets/artborders/` (scripts/build-web.sh takes them out of the
/// zip), and fetched when a document first uses them; the pages are laid
/// out again when they come (`generation`).
#[cfg(target_family = "wasm")]
mod web {
    use super::*;
    use gpui_kit::http_client::{AsyncBody, HttpClient};
    use std::sync::atomic::AtomicU64;

    pub(super) enum State {
        Pending,
        Ready(Option<Arc<ArtPicture>>),
    }

    static SOURCE: OnceLock<(String, Arc<dyn HttpClient>)> = OnceLock::new();
    pub(super) static GENERATION: AtomicU64 = AtomicU64::new(0);
    static PENDING: AtomicU32 = AtomicU32::new(0);

    pub(super) fn states() -> &'static Mutex<HashMap<(ArtBorder, ArtPart), State>> {
        static STATES: OnceLock<Mutex<HashMap<(ArtBorder, ArtPart), State>>> = OnceLock::new();
        STATES.get_or_init(Default::default)
    }

    pub fn set_source(base: String, client: Arc<dyn HttpClient>) {
        let _ = SOURCE.set((base, client));
    }

    pub(super) fn pending() -> bool {
        PENDING.load(Ordering::Relaxed) > 0
    }

    pub(super) fn picture(art: ArtBorder, part: ArtPart) -> Option<Arc<ArtPicture>> {
        let mut map = states().lock().unwrap_or_else(|e| e.into_inner());
        match map.get(&(art, part)) {
            Some(State::Ready(picture)) => return picture.clone(),
            Some(State::Pending) => return None,
            None => {}
        }
        let (base, client) = SOURCE.get()?;
        let name = ibis_pptx::art_borders::file_name(art, part)?;
        map.insert((art, part), State::Pending);
        drop(map);
        PENDING.fetch_add(1, Ordering::Relaxed);
        let url = format!("{base}/assets/artborders/{name}");
        let request = client.get(&url, AsyncBody::empty(), true);
        wasm_bindgen_futures::spawn_local(async move {
            let picture = async {
                let response = request.await.ok()?;
                if !response.status().is_success() {
                    log::warn!("Cannot read the art border image: {url} ({})", response.status());
                    return None;
                }
                let mut bytes = Vec::new();
                futures::AsyncReadExt::read_to_end(&mut response.into_body(), &mut bytes).await.ok()?;
                ArtPicture::from_png(bytes).map(Arc::new)
            }
            .await;
            states().lock().unwrap_or_else(|e| e.into_inner()).insert((art, part), State::Ready(picture));
            PENDING.fetch_sub(1, Ordering::Relaxed);
            GENERATION.fetch_add(1, Ordering::Relaxed);
        });
        None
    }
}

#[cfg(target_family = "wasm")]
pub use web::set_source as set_web_source;

/// Bumped when pictures arrive (the web version), to lay the pages out again.
pub(crate) fn generation() -> u64 {
    #[cfg(target_family = "wasm")]
    return web::GENERATION.load(Ordering::Relaxed);
    #[cfg(not(target_family = "wasm"))]
    0
}

/// Whether pictures are on their way (the web version): draw again soon.
pub(crate) fn pending() -> bool {
    #[cfg(target_family = "wasm")]
    return web::pending();
    #[cfg(not(target_family = "wasm"))]
    false
}

/// The picture of `art` for `part`, unless there is none or it shows
/// nothing (Word draws birds and tornPaper at the top and bottom only, and
/// certificateBanner at the corners only: their other pictures are blank).
pub(crate) fn picture(art: ArtBorder, part: ArtPart) -> Option<Arc<ArtPicture>> {
    #[cfg(target_family = "wasm")]
    return web::picture(art, part).filter(|p| !p.blank);
    #[cfg(not(target_family = "wasm"))]
    pictures()?.picture(art, part).filter(|p| !p.blank)
}

/// The colour to recolour `art` with for a border of `color`, if any: a
/// border all of whose pictures are grey takes the border's colour (black,
/// which "auto" reads as, leaves it as it is); a coloured one keeps its own,
/// but for the exceptions below.
pub(crate) fn tint(art: ArtBorder, color: Rgb) -> Option<Rgb> {
    use ArtBorder::*;
    if color == Rgb::BLACK {
        return None;
    }
    match art {
        // Grey, but Word draws them grey in any colour (the coverage tests'
        // references in C00000, compared by eye with auto's).
        TornPaper | DiamondsGray | PyramidsAbove | HeartGray | SawtoothGray => return None, // DPGB-00334, 00162, 00288, 00214, 00298
        // The white kinds' faint outline stays near grey in a colour (DPGB-00080
        // in C00000: mostly #bfb1b1 and #dbc0c0, against #bdbdbd to #cdcdcd
        // in auto, DPGB-00079; full C00000 a quarter strong would be #efbfbf).
        BasicWhiteSquares | BasicWhiteDashes | BasicThinLines => return None,
        // In colour, but Word colours their greys (black parts, and
        // southwest's grey) and keeps their other colours.
        DoubleD | Waveline | Southwest => return Some(color), // DPGB-00164, 00374, 00318
        _ => {}
    }
    const PARTS: [ArtPart; 8] =
        [ArtPart::Top, ArtPart::Bottom, ArtPart::Left, ArtPart::Right, ArtPart::TopLeft, ArtPart::TopRight, ArtPart::BottomLeft, ArtPart::BottomRight];
    let grey = PARTS.iter().filter_map(|&part| picture(art, part)).all(|p| p.grey);
    grey.then_some(color)
}

/// A picture to draw, and the device pixels its shorter side is drawn at.
type Key = (ArtBorder, ArtPart, Option<Rgb>, u32);

fn images() -> &'static Mutex<HashMap<Key, Arc<Image>>> {
    static IMAGES: OnceLock<Mutex<HashMap<Key, Arc<Image>>>> = OnceLock::new();
    IMAGES.get_or_init(Default::default)
}

/// Device pixels to a pixel of the window (its scale factor), as last seen
/// where pages are painted; and device pixels to a point where an export
/// makes its pictures ahead (`warm`). Bits of an f32.
static DEVICE_SCALE: AtomicU32 = AtomicU32::new(0x4000_0000); // 2.0
static WARM_PX_PER_PT: AtomicU32 = AtomicU32::new(0x4000_0000); // 2.0

/// The window's scale factor; whether it changed (the pages are then drawn
/// again, their pictures made for it).
pub fn set_device_scale(scale: f32) -> bool {
    DEVICE_SCALE.swap(scale.to_bits(), Ordering::Relaxed) != scale.to_bits()
}

pub(crate) fn device_scale() -> f32 {
    f32::from_bits(DEVICE_SCALE.load(Ordering::Relaxed))
}

/// Device pixels to a point in an export's pages.
#[cfg_attr(target_family = "wasm", allow(dead_code))]
pub fn set_export_px_per_pt(px: f32) {
    WARM_PX_PER_PT.store(px.to_bits(), Ordering::Relaxed);
}

/// Pixels a picture other than line art is rasterized at across (its
/// shorter side): enough for a 31 pt border at 4 pixels a point.
const RASTER: f64 = 128.0;

/// How dark the white kinds' outline is drawn, against black (see `image`).
const OUTLINE_INK: f32 = 0.25;

/// Device pixels for `pt` points of a picture's shorter side.
pub(crate) fn device_px(pt: f32, px_per_pt: f32) -> u32 {
    (pt * px_per_pt).round().clamp(1.0, 4096.0) as u32
}

/// The picture of `art` for `part`, recoloured with `tint`, to draw with its
/// shorter side `px` device pixels: an SVG wrapping the PNG, rasterized at
/// that size. The white kinds (a 1-pixel black outline round white) made
/// smaller keep their outline a device pixel wide, a quarter as dark, as
/// Word draws it: grey, not black, in its PDF at 2 pixels a point, the same
/// at 8 and 24 pt (DPGB-00079: mostly #bdbdbd to #cdcdcd in auto, a fifth
/// to a quarter of black). The PNG is not changed.
pub(crate) fn image(art: ArtBorder, part: ArtPart, tint: Option<Rgb>, px: u32) -> Option<Arc<Image>> {
    let picture = picture(art, part)?;
    // Other pictures are rasterized at one size, RASTER pixels across, and
    // made smaller when drawn (smoother, nearer Word's, than rasterized at
    // the size drawn: DPGB at 2026-10-06, 45 more pages differing more).
    // Only the white kinds' 1-pixel outlines are kept a device pixel wide:
    // other line art (crossStitch, zigZagStitch, people, snowflakes) Word
    // draws faint, made smaller as it is (DPGB-00152・00394 came out far
    // darker than Word's with their lines kept a pixel wide).
    let outline = matches!(art, ArtBorder::BasicWhiteSquares | ArtBorder::BasicWhiteDashes | ArtBorder::BasicThinLines)
        || (art == ArtBorder::BasicWhiteDots && picture.line_art);
    // Black bars or blocks with a 1-pixel outline beside them: Word draws the
    // outline about a fifth as dark, a device pixel wide (DPGB-00085: the
    // line under the bar #cbcbcb, the bar black), the bars as they are.
    let mixed = matches!(art, ArtBorder::BasicWideOutline | ArtBorder::BasicWideInline | ArtBorder::BasicWideMidline | ArtBorder::Marquee);
    let px = if outline || mixed { px } else { 0 };
    let mut images = images().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(found) = images.get(&(art, part, tint, px)) {
        return Some(found.clone());
    }
    let (w, h) = (picture.width as f64, picture.height as f64);
    let short = w.min(h);
    // The texture is at most 16 000 pixels long.
    // Rasterized at RASTER pixels across (made smaller when drawn); the
    // outline spread to as many of the picture's pixels as make a device
    // pixel where it is drawn `px` across.
    // The white kinds are rasterized at the size drawn: GPUI samples a
    // picture made smaller rather than averaging it, which loses or keeps
    // a thin outline whole (2026-10-06, tried at 128 pixels across).
    let k = (if px == 0 { RASTER } else { px as f64 } / short).min(16_000.0 / w.max(h));
    let ratio = if px == 0 { 1.0 } else { short / px as f64 };
    // Spread to a device pixel where drawn, and lightened to a quarter.
    let spread = if outline || mixed { ((ratio - 1.0) / 2.0).round().max(0.0) as u32 } else { 0 };
    let rgb = tint.map(|c| (c.r(), c.g(), c.b()));
    // GPUI and resvg sample a picture made smaller rather than averaging
    // it: a line thinner than a pixel comes out whole or not at all (tried
    // 2026-10-06, the SVG's feMorphology too). So the outline is spread to
    // a pixel and made as light as Word's.
    let spread_png = (spread > 0).then(|| ibis_pptx::art_borders::spread_thin(&picture.png, spread, OUTLINE_INK, mixed)).flatten();
    let source: &[u8] = spread_png.as_deref().unwrap_or(&picture.png);
    // A grey picture is recoloured by the SVG's filter; one in colour has
    // only its grey pixels recoloured, here (the filter cannot tell them).
    let recoloured = rgb.filter(|_| !picture.grey).and_then(|rgb| ibis_pptx::art_borders::recolour_greys(source, rgb));
    let (rw, rh) = ((w * k).round().max(1.0), (h * k).round().max(1.0));
    let svg = match &recoloured {
        Some(png) => crate::render::picture_svg::art_border_picture(png, picture.width, picture.height, rw, rh, None),
        None => crate::render::picture_svg::art_border_picture(source, picture.width, picture.height, rw, rh, rgb),
    };
    let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg));
    images.insert((art, part, tint, px), image.clone());
    Some(image)
}

/// Make the image an export will draw for a picture whose shorter side is
/// `pt` points, so it is rasterized ahead with the other SVG pictures (see
/// `Resources::rasterize_svg_pictures`).
pub(crate) fn warm(art: ArtBorder, part: ArtPart, tint: Option<Rgb>, pt: f32) {
    let _ = image(art, part, tint, device_px(pt, f32::from_bits(WARM_PX_PER_PT.load(Ordering::Relaxed))));
}

/// Every image made so far (for an export).
#[cfg_attr(target_family = "wasm", allow(dead_code))]
pub fn all_images() -> Vec<Arc<Image>> {
    images().lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect()
}

/// How much smaller than the picture's cell Word draws `art` at `sz` points
/// (1 for most). Measured on the coverage tests' references from Word
/// (2026-10-05): the width of the left edge's band in Word's page against
/// ibisOffice's drawn at full size, measured alike, at 8 pt and 24 pt (the
/// two test IDs); between and beyond, along the line through the two.
/// Only the kinds whose pictures repeat at the same spacing in both, or
/// whose two sizes agree within 0.05, are here.
pub(crate) fn scale(art: ArtBorder, sz: f32) -> f32 {
    use ArtBorder::*;
    let (at8, at24) = match art {
        Apples => (1.00, 0.85), // DPGB-00055・00056
        BabyRattle => (0.75, 0.71), // DPGB-00061・00062
        BalloonsHotAir => (0.62, 0.62), // DPGB-00065・00066
        BasicBlackDots => (0.38, 0.17), // DPGB-00069・00070
        BasicBlackSquares => (0.38, 0.35), // DPGB-00071・00072
        BasicWhiteDots => (1.00, 0.56), // DPGB-00077・00078
        Bats => (0.87, 0.87), // DPGB-00087・00088
        CirclesRectangles => (0.19, 0.19), // DPGB-00117・00118
        Confetti => (0.53, 0.50), // DPGB-00125・00126
        CouponCutoutDashes => (0.19, 0.19), // DPGB-00137・00138
        CreaturesLadyBug => (0.81, 0.76), // DPGB-00149・00150
        DecoArch => (0.62, 0.65), // DPGB-00155・00156
        DiamondsGray => (0.81, 0.76), // DPGB-00161・00162
        Firecrackers => (0.62, 0.65), // DPGB-00181・00182
        FlowersRoses => (0.75, 0.74), // DPGB-00195・00196
        GingerbreadMan => (0.88, 0.85), // DPGB-00203・00204
        Handmade1 => (0.19, 0.22), // DPGB-00207・00208
        Handmade2 => (0.44, 0.44), // DPGB-00209・00210
        HeartGray => (0.81, 0.79), // DPGB-00213・00214
        HouseFunky => (0.69, 0.70), // DPGB-00221・00222
        MapPins => (0.69, 0.45), // DPGB-00233・00234
        Moons => (1.00, 0.46), // DPGB-00243・00244
        Northwest => (1.00, 0.52), // DPGB-00249・00250
        Ovals => (1.00, 0.56), // DPGB-00251・00252
        PalmsBlack => (0.88, 0.83), // DPGB-00255・00256
        PalmsColor => (0.88, 0.85), // DPGB-00257・00258
        PaperClips => (0.62, 0.62), // DPGB-00259・00260
        Papyrus => (0.62, 0.56), // DPGB-00261・00262
        PartyGlass => (0.69, 0.70), // DPGB-00265・00266
        Pumpkin1 => (0.88, 0.92), // DPGB-00279・00280
        Skyrocket => (0.93, 0.87), // DPGB-00309・00310
        Stars => (0.69, 0.81), // DPGB-00319・00320
        StarsBlack => (1.00, 0.89), // DPGB-00325・00326
        StarsShadowed => (0.80, 0.78), // DPGB-00327・00328
        Swirligig => (0.50, 0.52), // DPGB-00331・00332
        TornPaperBlack => (0.25, 0.28), // DPGB-00335・00336
        Triangles => (0.56, 0.52), // DPGB-00341・00342
        // Word draws these near one size whatever the width, spaced by it:
        // the band's width (Word's against ibisOffice's, both measured with
        // the same strict ink threshold, 2026-10-06).
        // Re-measured on the top edge (dark pixels only, 2026-10-06): Word's dashes
        // 11.5 pt long at 8 pt and 13 at 24, drawn 8.5 and 11 at 0.44・0.19.
        BasicBlackDashes => (0.60, 0.22), // DPGB-00067・00068
        BasicWideInline => (0.60, 0.17),  // DPGB-00081・00082
        BasicWideMidline => (0.75, 0.27), // DPGB-00083・00084
        BasicWideOutline => (0.60, 0.21), // DPGB-00085・00086
        Marquee => (0.50, 0.19),          // DPGB-00239・00240
        MarqueeToothed => (1.00, 0.33),   // DPGB-00241・00242
        // The white kinds (a 1-pixel black outline round white): as their
        // black counterparts. basicWhiteSquares' band, spacing and squares
        // measure the same as basicBlackSquares' (DPGB-00079・00080 against
        // 00071・00072); basicThinLines' at 8 pt the same as basicWideOutline's
        // (DPGB-00073 against 00085; at 24 pt, DPGB-00074, too faint to measure).
        BasicWhiteSquares => (0.38, 0.35),
        // At 24 pt Word's dashes are 16.5 pt long every 59 pt (DPGB-00076),
        // drawn 11 pt long at 0.19: 0.28 makes them as long (Word's are
        // taller, 11.5 pt against 6.7). At 8 pt (DPGB-00075) Word's outline is
        // too faint to measure: basicBlackDashes' 0.44.
        BasicWhiteDashes => (0.44, 0.28),
        BasicThinLines => (0.60, 0.21),
        // Drawn on the top and bottom only: their top edge's band against sz
        // (the band ibisOffice draws them at), kept to the outer side
        // (`outer`).
        Birds => (0.81, 0.77),     // DPGB-00089・00090
        TornPaper => (0.56, 0.52), // DPGB-00333・00334
        _ => return 1.0,
    };
    (at8 + (at24 - at8) * (sz - 8.0) / 16.0).clamp(at8.min(at24) * 0.5, 1.0)
}

/// Whether Word draws `art` made smaller against the border's outer side
/// (the page's edge), not about its middle: birds' and tornPaper's bands
/// start where the full band would (DPGB-00090: rows 15 to 51 of the page,
/// DPGB-00334: 15 to 39, at 2 pixels a point).
pub(crate) fn outer(art: ArtBorder) -> bool {
    matches!(art, ArtBorder::Birds | ArtBorder::TornPaper)
}

/// How much farther apart than its proportions make them Word sets `art`'s
/// edge pictures at `sz` points (1 for most). Measured on the top edge
/// (dark pixels only, 2026-10-06) at 8 and 24 pt: the spacing matches at
/// 8 pt and is wider at 24; between and beyond, along the line through the
/// two.
pub(crate) fn pitch_scale(art: ArtBorder, sz: f32) -> f32 {
    use ArtBorder::*;
    let (at8, at24) = match art {
        BasicWideMidline => (1.0, 1.165), // DPGB-00083・00084: 25 and 88.5 pt, drawn 25.5 and 76
        Marquee => (1.0, 1.20),           // DPGB-00239・00240: pairs every 31 and 106.5 pt, drawn 31 and 88.5
        MarqueeToothed => (1.0, 1.09),    // DPGB-00241・00242: 16 and 53 pt, drawn 16 and 48.5
        _ => return 1.0,
    };
    (at8 + (at24 - at8) * (sz - 8.0) / 16.0).max(0.5)
}
