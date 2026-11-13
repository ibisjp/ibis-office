// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A new presentation (ファイル > 新規作成 > プレゼンテーション): the
//! parts of an empty .pptx written from scratch — one slide on the タイトル
//! スライド layout, the layouts タイトルとコンテンツ and 白紙, a master
//! with its text styles, and a theme with the colours and typefaces of
//! PowerPoint's 「Office テーマ」 (2013–2022). It is opened like a file
//! (`crate::open`), so it is saved like one.

use crate::Error;

const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;

/// Wide (16:9) slides, 33.867 × 19.05 cm, as PowerPoint makes a new
/// presentation.
const WIDTH: i64 = 12_192_000;
const HEIGHT: i64 = 6_858_000;

fn rels(items: &[(&str, &str, &str)]) -> String {
    let list: String = items
        .iter()
        .map(|(id, kind, target)| format!(r#"<Relationship Id="{id}" Type="{REL}/{kind}" Target="{target}"/>"#))
        .collect();
    format!(r#"{HEAD}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{list}</Relationships>"#)
}

/// A placeholder of `kind` (`ctrTitle`, `subTitle`, `title`, `body`…, or
/// `None` for the body's object placeholder), with id `id`, at `frame`
/// (EMU) when it says where, and the text body `body`.
fn placeholder(id: u32, name: &str, kind: Option<&str>, idx: Option<u32>, frame: Option<(i64, i64, i64, i64)>, body: &str) -> String {
    let ph = match (kind, idx) {
        (Some(k), Some(i)) => format!(r#"<p:ph type="{k}" idx="{i}"/>"#),
        (Some(k), None) => format!(r#"<p:ph type="{k}"/>"#),
        (None, Some(i)) => format!(r#"<p:ph idx="{i}"/>"#),
        (None, None) => "<p:ph/>".into(),
    };
    let sp_pr = match frame {
        Some((x, y, w, h)) => format!(r#"<p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm></p:spPr>"#),
        None => "<p:spPr/>".into(),
    };
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr>{ph}</p:nvPr></p:nvSpPr>{sp_pr}<p:txBody>{body}</p:txBody></p:sp>"#
    )
}

fn tree(shapes: &str) -> String {
    format!(
        r#"<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree></p:cSld>"#
    )
}

fn layout(name: &str, kind: &str, shapes: &str) -> String {
    let cs = tree(shapes).replacen("<p:cSld>", &format!(r#"<p:cSld name="{name}">"#), 1);
    format!(r#"{HEAD}<p:sldLayout xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}" type="{kind}" preserve="1">{cs}<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#)
}

/// A level of a text style: `size` in hundredths of a point, the left
/// margin and indent in EMU, a bullet or not, the font from the theme's
/// `+mj` (major) or `+mn` (minor) typefaces.
fn level(n: u32, size: u32, margin: i64, indent: i64, bullet: bool, major: bool) -> String {
    let font = if major { "+mj" } else { "+mn" };
    let bu = if bullet { r#"<a:buFont typeface="Arial" panose="020B0604020202020204"/><a:buChar char="•"/>"# } else { "<a:buNone/>" };
    format!(
        r#"<a:lvl{n}pPr marL="{margin}" indent="{indent}" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcBef><a:spcPts val="{}"/></a:spcBef>{bu}<a:defRPr sz="{size}" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="{font}-lt"/><a:ea typeface="{font}-ea"/><a:cs typeface="{font}-cs"/></a:defRPr></a:lvl{n}pPr>"#,
        if major { 0 } else if n == 1 { 1000 } else { 500 }
    )
}

fn master() -> String {
    let title = placeholder(2, "タイトル プレースホルダー 1", Some("title"), None, Some((838_200, 365_125, 10_515_600, 1_325_563)), r#"<a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0" anchor="ctr"><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:r><a:rPr kumimoji="1" lang="ja-JP" altLang="en-US"/><a:t>マスター タイトルの書式設定</a:t></a:r><a:endParaRPr kumimoji="1" lang="ja-JP" altLang="en-US"/></a:p>"#);
    let body = placeholder(3, "テキスト プレースホルダー 2", Some("body"), Some(1), Some((838_200, 1_825_625, 10_515_600, 4_351_338)), r#"<a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0"><a:normAutofit/></a:bodyPr><a:lstStyle/><a:p><a:pPr lvl="0"/><a:r><a:rPr kumimoji="1" lang="ja-JP" altLang="en-US"/><a:t>マスター テキストの書式設定</a:t></a:r></a:p><a:p><a:pPr lvl="1"/><a:r><a:rPr kumimoji="1" lang="ja-JP" altLang="en-US"/><a:t>第 2 レベル</a:t></a:r></a:p><a:p><a:pPr lvl="2"/><a:r><a:rPr kumimoji="1" lang="ja-JP" altLang="en-US"/><a:t>第 3 レベル</a:t></a:r></a:p><a:p><a:pPr lvl="3"/><a:r><a:rPr kumimoji="1" lang="ja-JP" altLang="en-US"/><a:t>第 4 レベル</a:t></a:r></a:p><a:p><a:pPr lvl="4"/><a:r><a:rPr kumimoji="1" lang="ja-JP" altLang="en-US"/><a:t>第 5 レベル</a:t></a:r><a:endParaRPr kumimoji="1" lang="ja-JP" altLang="en-US"/></a:p>"#);
    let title_style = format!(
        r#"<p:titleStyle><a:lvl1pPr algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcBef><a:spcPct val="0"/></a:spcBef><a:buNone/><a:defRPr kumimoji="1" sz="4400" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mj-lt"/><a:ea typeface="+mj-ea"/><a:cs typeface="+mj-cs"/></a:defRPr></a:lvl1pPr></p:titleStyle>"#
    );
    let body_style: String = [(2800, 228_600), (2400, 685_800), (2000, 1_143_000), (1800, 1_600_200), (1800, 2_057_400), (1800, 2_514_600), (1800, 2_971_800), (1800, 3_429_000), (1800, 3_886_200)]
        .iter()
        .enumerate()
        .map(|(i, &(size, margin))| level(i as u32 + 1, size, margin, -228_600, true, false))
        .collect();
    let other_style: String = (0..9).map(|i| level(i + 1, 1800, 457_200 * i as i64, 0, false, false)).collect();
    format!(
        r#"{HEAD}<p:sldMaster xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}">{}<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/><p:sldLayoutId id="2147483650" r:id="rId2"/><p:sldLayoutId id="2147483651" r:id="rId3"/></p:sldLayoutIdLst><p:txStyles>{title_style}<p:bodyStyle>{body_style}</p:bodyStyle><p:otherStyle><a:defPPr><a:defRPr lang="ja-JP"/></a:defPPr>{other_style}</p:otherStyle></p:txStyles></p:sldMaster>"#,
        tree(&format!("{title}{body}")).replacen("<p:cSld>", r#"<p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg>"#, 1)
    )
}

/// The new files' theme colours beside dk1 (black) and lt1 (white).
const THEME_COLOURS: [(&str, &str); 10] = [
    ("dk2", "44546A"), ("lt2", "E7E6E6"), ("accent1", "4472C4"), ("accent2", "ED7D31"), ("accent3", "A5A5A5"),
    ("accent4", "FFC000"), ("accent5", "5B9BD5"), ("accent6", "70AD47"), ("hlink", "0563C1"), ("folHlink", "954F72"),
];

/// The new files' colour scheme, by name (for a book read without a theme).
pub(crate) fn theme_colours() -> Vec<(String, ibis_doc_model::Rgb)> {
    let mut out = vec![("dk1".to_string(), ibis_doc_model::Rgb(0)), ("lt1".to_string(), ibis_doc_model::Rgb(0xFF_FFFF))];
    out.extend(THEME_COLOURS.iter().map(|(n, c)| (n.to_string(), ibis_doc_model::Rgb(u32::from_str_radix(c, 16).unwrap_or(0)))));
    out
}

pub(crate) fn theme() -> String {
    let fonts = |latin: &str, ea: &str| format!(r#"<a:latin typeface="{latin}" panose="020F0302020204030204"/><a:ea typeface="{ea}"/><a:cs typeface=""/>"#);
    theme_of("Office テーマ", &THEME_COLOURS, &fonts("Calibri Light", "游ゴシック Light"), &fonts("Calibri", "游ゴシック"))
}

/// The colours of Office's theme of 2007 to 2010 ("Office 2007 - 2010"), beside dk1 and lt1.
const THEME_2007_COLOURS: [(&str, &str); 10] = [
    ("dk2", "1F497D"), ("lt2", "EEECE1"), ("accent1", "4F81BD"), ("accent2", "C0504D"), ("accent3", "9BBB59"),
    ("accent4", "8064A2"), ("accent5", "4BACC6"), ("accent6", "F79646"), ("hlink", "0000FF"), ("folHlink", "800080"),
];

/// Office's theme of 2007 to 2010, the one an old workbook (.xls) means by the default theme (its `THEME` record's
/// version 124226 with no theme of its own, MS-XLS 2.4.326): Excel 365 saves a workbook in this theme so (the corpus's
/// workbooks in it; those in a later theme carry their theme). Cambria and Calibri, with ＭＳ Ｐゴシック for Japanese.
pub(crate) fn theme_2007() -> String {
    let fonts = |latin: &str| format!(r#"<a:latin typeface="{latin}"/><a:ea typeface=""/><a:cs typeface=""/><a:font script="Jpan" typeface="ＭＳ Ｐゴシック"/>"#);
    theme_of("Office Theme 2007 - 2010", &THEME_2007_COLOURS, &fonts("Cambria"), &fonts("Calibri"))
}

fn theme_of(name: &str, scheme: &[(&str, &str)], major: &str, minor: &str) -> String {
    let colour = |name: &str, rgb: &str| format!(r#"<a:{name}><a:srgbClr val="{rgb}"/></a:{name}>"#);
    let colours: String = scheme.iter().map(|(n, c)| colour(n, c)).collect();
    let solid = r#"<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>"#;
    let line = |w: u32| format!(r#"<a:ln w="{w}" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln>"#);
    format!(
        r#"{HEAD}<a:theme xmlns:a="{A}" name="{name}"><a:themeElements><a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>{colours}</a:clrScheme><a:fontScheme name="Office"><a:majorFont>{}</a:majorFont><a:minorFont>{}</a:minorFont></a:fontScheme><a:fmtScheme name="Office"><a:fillStyleLst>{solid}{solid}{solid}</a:fillStyleLst><a:lnStyleLst>{}{}{}</a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst>{solid}{solid}{solid}</a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#,
        major,
        minor,
        line(6350),
        line(12700),
        line(19050),
    )
}

/// The bytes of a new, empty presentation.
pub fn new_presentation() -> Result<Vec<u8>, Error> {
    let empty_title = r#"<a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr kumimoji="1" lang="ja-JP" altLang="en-US"/></a:p>"#;
    let title_slide = layout(
        "タイトル スライド",
        "title",
        &(placeholder(2, "タイトル 1", Some("ctrTitle"), None, Some((1_524_000, 1_122_363, 9_144_000, 2_387_600)), r#"<a:bodyPr anchor="b"/><a:lstStyle><a:lvl1pPr algn="ctr"><a:defRPr sz="6000"/></a:lvl1pPr></a:lstStyle><a:p><a:r><a:rPr kumimoji="1" lang="ja-JP" altLang="en-US"/><a:t>マスター タイトルの書式設定</a:t></a:r><a:endParaRPr kumimoji="1" lang="ja-JP" altLang="en-US"/></a:p>"#)
            + &placeholder(3, "サブタイトル 2", Some("subTitle"), Some(1), Some((1_524_000, 3_602_038, 9_144_000, 1_655_762)), r#"<a:bodyPr/><a:lstStyle><a:lvl1pPr marL="0" indent="0" algn="ctr"><a:buNone/><a:defRPr sz="2400"/></a:lvl1pPr></a:lstStyle><a:p><a:r><a:rPr kumimoji="1" lang="ja-JP" altLang="en-US"/><a:t>マスター サブタイトルの書式設定</a:t></a:r><a:endParaRPr kumimoji="1" lang="ja-JP" altLang="en-US"/></a:p>"#)),
    );
    let content = layout(
        "タイトルとコンテンツ",
        "obj",
        &(placeholder(2, "タイトル 1", Some("title"), None, None, empty_title) + &placeholder(3, "コンテンツ プレースホルダー 2", None, Some(1), None, empty_title)),
    );
    let blank = layout("白紙", "blank", "");
    let slide = format!(
        r#"{HEAD}<p:sld xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}">{}<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
        tree(&(placeholder(2, "タイトル 1", Some("ctrTitle"), None, None, empty_title) + &placeholder(3, "サブタイトル 2", Some("subTitle"), Some(1), None, empty_title)))
    );
    let presentation = format!(
        r#"{HEAD}<p:presentation xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}" saveSubsetFonts="1"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst><p:sldSz cx="{WIDTH}" cy="{HEIGHT}"/><p:notesSz cx="6858000" cy="9144000"/><p:defaultTextStyle><a:defPPr><a:defRPr lang="ja-JP"/></a:defPPr></p:defaultTextStyle></p:presentation>"#
    );
    let ct = |part: &str, kind: &str| format!(r#"<Override PartName="/{part}" ContentType="application/vnd.openxmlformats-officedocument.{kind}+xml"/>"#);
    let content_types = format!(
        r#"{HEAD}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>{}{}{}{}{}{}{}{}{}</Types>"#,
        ct("ppt/presentation.xml", "presentationml.presentation.main"),
        ct("ppt/slideMasters/slideMaster1.xml", "presentationml.slideMaster"),
        ct("ppt/slideLayouts/slideLayout1.xml", "presentationml.slideLayout"),
        ct("ppt/slideLayouts/slideLayout2.xml", "presentationml.slideLayout"),
        ct("ppt/slideLayouts/slideLayout3.xml", "presentationml.slideLayout"),
        ct("ppt/slides/slide1.xml", "presentationml.slide"),
        ct("ppt/theme/theme1.xml", "theme"),
        ct("ppt/presProps.xml", "presentationml.presProps"),
        ct("ppt/viewProps.xml", "presentationml.viewProps"),
    );
    let parts: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", content_types),
        ("_rels/.rels", rels(&[("rId1", "officeDocument", "ppt/presentation.xml")])),
        ("ppt/presentation.xml", presentation),
        (
            "ppt/_rels/presentation.xml.rels",
            rels(&[
                ("rId1", "slideMaster", "slideMasters/slideMaster1.xml"),
                ("rId2", "slide", "slides/slide1.xml"),
                ("rId3", "presProps", "presProps.xml"),
                ("rId4", "viewProps", "viewProps.xml"),
                ("rId5", "theme", "theme/theme1.xml"),
            ]),
        ),
        ("ppt/slideMasters/slideMaster1.xml", master()),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            rels(&[
                ("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"),
                ("rId2", "slideLayout", "../slideLayouts/slideLayout2.xml"),
                ("rId3", "slideLayout", "../slideLayouts/slideLayout3.xml"),
                ("rId4", "theme", "../theme/theme1.xml"),
            ]),
        ),
        ("ppt/slideLayouts/slideLayout1.xml", title_slide),
        ("ppt/slideLayouts/_rels/slideLayout1.xml.rels", rels(&[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")])),
        ("ppt/slideLayouts/slideLayout2.xml", content),
        ("ppt/slideLayouts/_rels/slideLayout2.xml.rels", rels(&[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")])),
        ("ppt/slideLayouts/slideLayout3.xml", blank),
        ("ppt/slideLayouts/_rels/slideLayout3.xml.rels", rels(&[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")])),
        ("ppt/slides/slide1.xml", slide),
        ("ppt/slides/_rels/slide1.xml.rels", rels(&[("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml")])),
        ("ppt/theme/theme1.xml", theme()),
        ("ppt/presProps.xml", format!(r#"{HEAD}<p:presentationPr xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"/>"#)),
        (
            "ppt/viewProps.xml",
            format!(r#"{HEAD}<p:viewPr xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:normalViewPr><p:restoredLeft sz="15620"/><p:restoredTop sz="94660"/></p:normalViewPr><p:gridSpacing cx="72008" cy="72008"/></p:viewPr>"#),
        ),
    ];
    let mut writer = ibis_ooxml::zip::Writer::new();
    for (name, xml) in parts {
        writer.add(name, ibis_ooxml::zip::EntryMeta::default(), xml.as_bytes()).map_err(|e| Error::new(format!("{e}")))?;
    }
    writer.finish().map_err(|e| Error::new(format!("{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The new file opens, has one slide on its title layout and three
    /// layouts, and saves and reads back the same.
    #[test]
    fn a_new_presentation_opens_and_saves() {
        let bytes = new_presentation().unwrap();
        let opened = crate::open(bytes).unwrap();
        let p = &opened.presentation;
        assert_eq!(p.slides.len(), 1);
        assert_eq!(p.layouts.len(), 3);
        assert_eq!((p.slide_size.width, p.slide_size.height), (WIDTH, HEIGHT));
        let saved = opened.source.save(p, crate::SaveOptions::default()).unwrap();
        let again = crate::read(&saved).unwrap();
        assert_eq!(again.slides.len(), 1);
    }
}
