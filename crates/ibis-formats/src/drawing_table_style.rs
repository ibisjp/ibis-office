// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Built-in PowerPoint table styles (ECMA-376 DrawingML CT_TableStyle, 74 styles).
//!
//! PowerPoint presentations refer to built-in table styles via `<a:tableStyleId>{GUID}</a:tableStyleId>`.
//! The definitions are generated based on ECMA-376 DrawingML and measured reference rendering.

use std::collections::HashMap;
use std::sync::OnceLock;
use roxmltree::{Document, Node};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    NoStyleNoGrid,
    NoStyleTableGrid,
    Themed1,
    Themed2,
    Light1,
    Light2,
    Light3,
    Medium1,
    Medium2,
    Medium3,
    Medium4,
    Dark1,
    Dark2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorScheme {
    Neutral,
    Accent(u8),
    Dual(u8, u8),
}

struct StyleMeta {
    guid: &'static str,
    name: &'static str,
    family: Family,
    color: ColorScheme,
}

const STYLES: &[StyleMeta] = &[
    // No Style (2)
    StyleMeta { guid: "{2D5ABB26-0587-4C30-8999-92F81FD0307C}", name: "No Style, No Grid", family: Family::NoStyleNoGrid, color: ColorScheme::Neutral },
    StyleMeta { guid: "{5940675A-B579-460E-94D1-54222C63F5DA}", name: "No Style, Table Grid", family: Family::NoStyleTableGrid, color: ColorScheme::Neutral },

    // Themed Style 1 (6)
    StyleMeta { guid: "{3C2FFA5D-87B4-456A-9821-1D502468CF0F}", name: "Themed Style 1 - Accent 1", family: Family::Themed1, color: ColorScheme::Accent(1) },
    StyleMeta { guid: "{284E427A-3D55-4303-BF80-6455036E1DE7}", name: "Themed Style 1 - Accent 2", family: Family::Themed1, color: ColorScheme::Accent(2) },
    StyleMeta { guid: "{69C7853C-536D-4A76-A0AE-DD22124D55A5}", name: "Themed Style 1 - Accent 3", family: Family::Themed1, color: ColorScheme::Accent(3) },
    StyleMeta { guid: "{775DCB02-9BB8-47FD-8907-85C794F793BA}", name: "Themed Style 1 - Accent 4", family: Family::Themed1, color: ColorScheme::Accent(4) },
    StyleMeta { guid: "{35758FB7-9AC5-4552-8A53-C91805E547FA}", name: "Themed Style 1 - Accent 5", family: Family::Themed1, color: ColorScheme::Accent(5) },
    StyleMeta { guid: "{08FB837D-C827-4EFA-A057-4D05807E0F7C}", name: "Themed Style 1 - Accent 6", family: Family::Themed1, color: ColorScheme::Accent(6) },

    // Themed Style 2 (6)
    StyleMeta { guid: "{D113A9D2-9D6B-4929-AA2D-F23B5EE8CBE7}", name: "Themed Style 2 - Accent 1", family: Family::Themed2, color: ColorScheme::Accent(1) },
    StyleMeta { guid: "{18603FDC-E32A-4AB5-989C-0864C3EAD2B8}", name: "Themed Style 2 - Accent 2", family: Family::Themed2, color: ColorScheme::Accent(2) },
    StyleMeta { guid: "{306799F8-075E-4A3A-A7F6-7FBC6576F1A4}", name: "Themed Style 2 - Accent 3", family: Family::Themed2, color: ColorScheme::Accent(3) },
    StyleMeta { guid: "{E269D01E-BC32-4049-B463-5C60D7B0CCD2}", name: "Themed Style 2 - Accent 4", family: Family::Themed2, color: ColorScheme::Accent(4) },
    StyleMeta { guid: "{327F97BB-C833-4FB7-BDE5-3F7075034690}", name: "Themed Style 2 - Accent 5", family: Family::Themed2, color: ColorScheme::Accent(5) },
    StyleMeta { guid: "{638B1855-1B75-4FBE-930C-398BA8C253C6}", name: "Themed Style 2 - Accent 6", family: Family::Themed2, color: ColorScheme::Accent(6) },

    // Light Style 1 (7)
    StyleMeta { guid: "{9D7B26C5-4107-4FEC-AEDC-1716B250A1EF}", name: "Light Style 1", family: Family::Light1, color: ColorScheme::Neutral },
    StyleMeta { guid: "{3B4B98B0-60AC-42C2-AFA5-B58CD77FA1E5}", name: "Light Style 1 - Accent 1", family: Family::Light1, color: ColorScheme::Accent(1) },
    StyleMeta { guid: "{0E3FDE45-AF77-4B5C-9715-49D594BDF05E}", name: "Light Style 1 - Accent 2", family: Family::Light1, color: ColorScheme::Accent(2) },
    StyleMeta { guid: "{C083E6E3-FA7D-4D7B-A595-EF9225AFEA82}", name: "Light Style 1 - Accent 3", family: Family::Light1, color: ColorScheme::Accent(3) },
    StyleMeta { guid: "{D27102A9-8310-4765-A935-A1911B00CA55}", name: "Light Style 1 - Accent 4", family: Family::Light1, color: ColorScheme::Accent(4) },
    StyleMeta { guid: "{5FD0F851-EC5A-4D38-B0AD-8093EC10F338}", name: "Light Style 1 - Accent 5", family: Family::Light1, color: ColorScheme::Accent(5) },
    StyleMeta { guid: "{68D230F3-CF80-4859-8CE7-A43EE81993B5}", name: "Light Style 1 - Accent 6", family: Family::Light1, color: ColorScheme::Accent(6) },

    // Light Style 2 (7)
    StyleMeta { guid: "{7E9639D4-E3E2-4D34-9284-5A2195B3D0D7}", name: "Light Style 2", family: Family::Light2, color: ColorScheme::Neutral },
    StyleMeta { guid: "{69012ECD-51FC-41F1-AA8D-1B2483CD663E}", name: "Light Style 2 - Accent 1", family: Family::Light2, color: ColorScheme::Accent(1) },
    StyleMeta { guid: "{72833802-FEF1-4C79-8D5D-14CF1EAF98D9}", name: "Light Style 2 - Accent 2", family: Family::Light2, color: ColorScheme::Accent(2) },
    StyleMeta { guid: "{F2DE63D5-997A-4646-A377-4702673A728D}", name: "Light Style 2 - Accent 3", family: Family::Light2, color: ColorScheme::Accent(3) },
    StyleMeta { guid: "{17292A2E-F333-43FB-9621-5CBBE7FDCDCB}", name: "Light Style 2 - Accent 4", family: Family::Light2, color: ColorScheme::Accent(4) },
    StyleMeta { guid: "{5A111915-BE36-4E01-A7E5-04B1672EAD32}", name: "Light Style 2 - Accent 5", family: Family::Light2, color: ColorScheme::Accent(5) },
    StyleMeta { guid: "{912C8C85-51F0-491E-9774-3900AFEF0FD7}", name: "Light Style 2 - Accent 6", family: Family::Light2, color: ColorScheme::Accent(6) },

    // Light Style 3 (7)
    StyleMeta { guid: "{616DA210-FB5B-4158-B5E0-FEB733F419BA}", name: "Light Style 3", family: Family::Light3, color: ColorScheme::Neutral },
    StyleMeta { guid: "{BC89EF96-8CEA-46FF-86C4-4CE0E7609802}", name: "Light Style 3 - Accent 1", family: Family::Light3, color: ColorScheme::Accent(1) },
    StyleMeta { guid: "{5DA37D80-6434-44D0-A028-1B22A696006F}", name: "Light Style 3 - Accent 2", family: Family::Light3, color: ColorScheme::Accent(2) },
    StyleMeta { guid: "{8799B23B-EC83-4686-B30A-512413B5E67A}", name: "Light Style 3 - Accent 3", family: Family::Light3, color: ColorScheme::Accent(3) },
    StyleMeta { guid: "{ED083AE6-46FA-4A59-8FB0-9F97EB10719F}", name: "Light Style 3 - Accent 4", family: Family::Light3, color: ColorScheme::Accent(4) },
    StyleMeta { guid: "{BDBED569-4797-4DF1-A0F4-6AAB3CD982D8}", name: "Light Style 3 - Accent 5", family: Family::Light3, color: ColorScheme::Accent(5) },
    StyleMeta { guid: "{E8B1032C-EA38-4F05-BA0D-38AFFFC7BED3}", name: "Light Style 3 - Accent 6", family: Family::Light3, color: ColorScheme::Accent(6) },

    // Medium Style 1 (7)
    StyleMeta { guid: "{793D81CF-94F2-401A-BA57-92F5A7B2D0C5}", name: "Medium Style 1", family: Family::Medium1, color: ColorScheme::Neutral },
    StyleMeta { guid: "{B301B821-A1FF-4177-AEE7-76D212191A09}", name: "Medium Style 1 - Accent 1", family: Family::Medium1, color: ColorScheme::Accent(1) },
    StyleMeta { guid: "{9DCAF9ED-07DC-4A11-8D7F-57B35C25682E}", name: "Medium Style 1 - Accent 2", family: Family::Medium1, color: ColorScheme::Accent(2) },
    StyleMeta { guid: "{1FECB4D8-DB02-4DC6-A0A2-4F2EBAE1DC90}", name: "Medium Style 1 - Accent 3", family: Family::Medium1, color: ColorScheme::Accent(3) },
    StyleMeta { guid: "{1E171933-4619-4E11-9A3F-F7608DF75F80}", name: "Medium Style 1 - Accent 4", family: Family::Medium1, color: ColorScheme::Accent(4) },
    StyleMeta { guid: "{FABFCF23-3B69-468F-B69F-88F6DE6A72F2}", name: "Medium Style 1 - Accent 5", family: Family::Medium1, color: ColorScheme::Accent(5) },
    StyleMeta { guid: "{10A1B5D5-9B99-4C35-A422-299274C87663}", name: "Medium Style 1 - Accent 6", family: Family::Medium1, color: ColorScheme::Accent(6) },

    // Medium Style 2 (7) - Default is Accent 1
    StyleMeta { guid: "{073A0DAA-6AF3-43AB-8588-CEC1D06C72B9}", name: "Medium Style 2", family: Family::Medium2, color: ColorScheme::Neutral },
    StyleMeta { guid: "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}", name: "Medium Style 2 - Accent 1", family: Family::Medium2, color: ColorScheme::Accent(1) },
    StyleMeta { guid: "{21E4AEA4-8DFA-4A89-87EB-49C32662AFE0}", name: "Medium Style 2 - Accent 2", family: Family::Medium2, color: ColorScheme::Accent(2) },
    StyleMeta { guid: "{F5AB1C69-6EDB-4FF4-983F-18BD219EF322}", name: "Medium Style 2 - Accent 3", family: Family::Medium2, color: ColorScheme::Accent(3) },
    StyleMeta { guid: "{00A15C55-8517-42AA-B614-E9B94910E393}", name: "Medium Style 2 - Accent 4", family: Family::Medium2, color: ColorScheme::Accent(4) },
    StyleMeta { guid: "{7DF18680-E054-41AD-8BC1-D1AEF772440D}", name: "Medium Style 2 - Accent 5", family: Family::Medium2, color: ColorScheme::Accent(5) },
    StyleMeta { guid: "{93296810-A885-4BE3-A3E7-6D5BEEA58F35}", name: "Medium Style 2 - Accent 6", family: Family::Medium2, color: ColorScheme::Accent(6) },

    // Medium Style 3 (7)
    StyleMeta { guid: "{8EC20E35-A176-4012-BC5E-935CFFF8708E}", name: "Medium Style 3", family: Family::Medium3, color: ColorScheme::Neutral },
    StyleMeta { guid: "{6E25E649-3F16-4E02-A733-19D2CDBF48F0}", name: "Medium Style 3 - Accent 1", family: Family::Medium3, color: ColorScheme::Accent(1) },
    StyleMeta { guid: "{85BE263C-DBD7-4A20-BB59-AAB30ACAA65A}", name: "Medium Style 3 - Accent 2", family: Family::Medium3, color: ColorScheme::Accent(2) },
    StyleMeta { guid: "{EB344D84-9AFB-497E-A393-DC336BA19D2E}", name: "Medium Style 3 - Accent 3", family: Family::Medium3, color: ColorScheme::Accent(3) },
    StyleMeta { guid: "{EB9631B5-78F2-41C9-869B-9F39066F8104}", name: "Medium Style 3 - Accent 4", family: Family::Medium3, color: ColorScheme::Accent(4) },
    StyleMeta { guid: "{74C1A8A3-306A-4EB7-A6B1-4F7E0EB9C5D6}", name: "Medium Style 3 - Accent 5", family: Family::Medium3, color: ColorScheme::Accent(5) },
    StyleMeta { guid: "{2A488322-F2BA-4B5B-9748-0D474271808F}", name: "Medium Style 3 - Accent 6", family: Family::Medium3, color: ColorScheme::Accent(6) },

    // Medium Style 4 (7)
    StyleMeta { guid: "{D7AC3CCA-C797-4891-BE02-D94E43425B78}", name: "Medium Style 4", family: Family::Medium4, color: ColorScheme::Neutral },
    StyleMeta { guid: "{69CF1AB2-1976-4502-BF36-3FF5EA218861}", name: "Medium Style 4 - Accent 1", family: Family::Medium4, color: ColorScheme::Accent(1) },
    StyleMeta { guid: "{8A107856-5554-42FB-B03E-39F5DBC370BA}", name: "Medium Style 4 - Accent 2", family: Family::Medium4, color: ColorScheme::Accent(2) },
    StyleMeta { guid: "{0505E3EF-67EA-436B-97B2-0124C06EBD24}", name: "Medium Style 4 - Accent 3", family: Family::Medium4, color: ColorScheme::Accent(3) },
    StyleMeta { guid: "{C4B1156A-380E-4F78-BDF5-A606A8083BF9}", name: "Medium Style 4 - Accent 4", family: Family::Medium4, color: ColorScheme::Accent(4) },
    StyleMeta { guid: "{22838BEF-8BB2-4498-84A7-C5851F593DF1}", name: "Medium Style 4 - Accent 5", family: Family::Medium4, color: ColorScheme::Accent(5) },
    StyleMeta { guid: "{16D9F66E-5EB9-4882-86FB-DCBF35E3C3E4}", name: "Medium Style 4 - Accent 6", family: Family::Medium4, color: ColorScheme::Accent(6) },

    // Dark Style 1 (7)
    StyleMeta { guid: "{E8034E78-7F5D-4C2E-B375-FC64B27BC917}", name: "Dark Style 1", family: Family::Dark1, color: ColorScheme::Neutral },
    StyleMeta { guid: "{125E5076-3810-47DD-B79F-674D7AD40C01}", name: "Dark Style 1 - Accent 1", family: Family::Dark1, color: ColorScheme::Accent(1) },
    StyleMeta { guid: "{37CE84F3-28C3-443E-9E96-99CF82512B78}", name: "Dark Style 1 - Accent 2", family: Family::Dark1, color: ColorScheme::Accent(2) },
    StyleMeta { guid: "{D03447BB-5D67-496B-8E87-E561075AD55C}", name: "Dark Style 1 - Accent 3", family: Family::Dark1, color: ColorScheme::Accent(3) },
    StyleMeta { guid: "{E929F9F4-4A8F-4326-A1B4-22849713DDAB}", name: "Dark Style 1 - Accent 4", family: Family::Dark1, color: ColorScheme::Accent(4) },
    StyleMeta { guid: "{8FD4443E-F989-4FC4-A0C8-D5A2AF1F390B}", name: "Dark Style 1 - Accent 5", family: Family::Dark1, color: ColorScheme::Accent(5) },
    StyleMeta { guid: "{AF606853-7671-496A-8E4F-DF71F8EC918B}", name: "Dark Style 1 - Accent 6", family: Family::Dark1, color: ColorScheme::Accent(6) },

    // Dark Style 2 (4)
    StyleMeta { guid: "{5202B0CA-FC54-4496-8BCA-5EF66A818D29}", name: "Dark Style 2", family: Family::Dark2, color: ColorScheme::Neutral },
    StyleMeta { guid: "{0660B408-B3CF-4A94-85FC-2B1E0A45F4A2}", name: "Dark Style 2 - Accent 1/Accent 2", family: Family::Dark2, color: ColorScheme::Dual(1, 2) },
    StyleMeta { guid: "{91EBBBCC-DAD2-459C-BE2E-F6DE35CF9A28}", name: "Dark Style 2 - Accent 3/Accent 4", family: Family::Dark2, color: ColorScheme::Dual(3, 4) },
    StyleMeta { guid: "{46F890A9-2807-4EBB-B81D-B2AA78EC7F39}", name: "Dark Style 2 - Accent 5/Accent 6", family: Family::Dark2, color: ColorScheme::Dual(5, 6) },
];

fn scheme_clr(color: ColorScheme, tint: Option<u32>, shade: Option<u32>) -> String {
    let name = match color {
        ColorScheme::Neutral => "dk1",
        ColorScheme::Accent(n) => match n {
            1 => "accent1",
            2 => "accent2",
            3 => "accent3",
            4 => "accent4",
            5 => "accent5",
            _ => "accent6",
        },
        ColorScheme::Dual(a, _) => match a {
            1 => "accent1",
            3 => "accent3",
            _ => "accent5",
        },
    };
    // DrawingML `tint`/`shade` (a blend towards white / black in linear RGB), as PowerPoint's own table styles use.
    if let Some(t) = tint {
        format!(r#"<a:schemeClr val="{name}"><a:tint val="{}"/></a:schemeClr>"#, t * 1000)
    } else if let Some(s) = shade {
        format!(r#"<a:schemeClr val="{name}"><a:shade val="{}"/></a:schemeClr>"#, s * 1000)
    } else {
        format!(r#"<a:schemeClr val="{name}"/>"#)
    }
}

fn scheme_clr_alpha(color: ColorScheme, percent: u32) -> String {
    let plain = scheme_clr(color, None, None);
    plain.replacen("/>", &format!("><a:alpha val=\"{}\"/></a:schemeClr>", percent * 1000), 1)
}

fn scheme_clr_b(color: ColorScheme) -> String {
    let name = match color {
        ColorScheme::Dual(_, b) => match b {
            2 => "accent2",
            4 => "accent4",
            _ => "accent6",
        },
        _ => "dk1",
    };
    format!(r#"<a:schemeClr val="{name}"/>"#)
}

fn border_ln(clr: &str, w: i64) -> String {
    format!(r#"<a:ln w="{w}" cmpd="sng"><a:solidFill>{clr}</a:solidFill></a:ln>"#)
}

fn border_edges(left: Option<&str>, right: Option<&str>, top: Option<&str>, bottom: Option<&str>, inside_h: Option<&str>, inside_v: Option<&str>) -> String {
    let mut s = String::from("<a:tcBdr>");
    if let Some(l) = left { s.push_str(&format!("<a:left>{}</a:left>", border_ln(l, 12700))); }
    if let Some(r) = right { s.push_str(&format!("<a:right>{}</a:right>", border_ln(r, 12700))); }
    if let Some(t) = top { s.push_str(&format!("<a:top>{}</a:top>", border_ln(t, 12700))); }
    if let Some(b) = bottom { s.push_str(&format!("<a:bottom>{}</a:bottom>", border_ln(b, 12700))); }
    if let Some(h) = inside_h { s.push_str(&format!("<a:insideH>{}</a:insideH>", border_ln(h, 12700))); }
    if let Some(v) = inside_v { s.push_str(&format!("<a:insideV>{}</a:insideV>", border_ln(v, 12700))); }
    s.push_str("</a:tcBdr>");
    s
}

/// One edge of a style element, `w` EMU wide.
fn thick_edge_w(edge: &str, clr: &str, w: i64) -> String {
    format!(r#"<a:tcBdr><a:{edge}>{}</a:{edge}></a:tcBdr>"#, border_ln(clr, w))
}

/// `border_edges` with every line `w` EMU wide (left, right, top, bottom, insideH, insideV).
fn border_edges_w(w: i64, e: [Option<&str>; 6]) -> String {
    let mut s = String::from("<a:tcBdr>");
    for (name, c) in ["left", "right", "top", "bottom", "insideH", "insideV"].iter().zip(e) {
        if let Some(c) = c {
            s.push_str(&format!("<a:{name}>{}</a:{name}>", border_ln(c, w)));
        }
    }
    s.push_str("</a:tcBdr>");
    s
}

/// One edge of a style element drawn thicker (3 pt, the header's underline / the total row's overline of the Medium styles).
fn thick_edge(edge: &str, clr: &str) -> String {
    format!(r#"<a:tcBdr><a:{edge}>{}</a:{edge}></a:tcBdr>"#, border_ln(clr, 38100))
}

fn generate_tbl_style(meta: &StyleMeta) -> String {
    let mut xml = format!(r#"<a:tblStyle styleId="{}" styleName="{}">"#, meta.guid, meta.name);
    let clr_base = scheme_clr(meta.color, None, None);
    let clr_tint20 = scheme_clr(meta.color, Some(20), None);
    let clr_tint40 = scheme_clr(meta.color, Some(40), None);
    let clr_shade80 = scheme_clr(meta.color, None, Some(80));
    let clr_shade60 = scheme_clr(meta.color, None, Some(60));
    let clr_shade40 = scheme_clr(meta.color, None, Some(40));
    let clr_alpha20 = scheme_clr_alpha(meta.color, 20);
    let clr_alpha40 = scheme_clr_alpha(meta.color, 40);
    let clr_tint50 = scheme_clr(meta.color, Some(50), None);
    let clr_white = r#"<a:schemeClr val="lt1"/>"#;
    let clr_black = r#"<a:schemeClr val="dk1"/>"#;

    // Measured from PowerPoint's own rendering of each style (test/coverage PTST: the colour of every cell with the
    // header/total row, first/last column and banded rows/columns on and off). A band's alpha is over the table's own
    // fill, and only band 1 of rows and columns paints (the odd rows are the whole table's colour).
    match meta.family {
        Family::NoStyleNoGrid => {
            xml.push_str(r#"<a:wholeTbl><a:tcTxStyle><a:schemeClr val="dk1"/></a:tcTxStyle><a:tcStyle/></a:wholeTbl>"#);
        }
        Family::NoStyleTableGrid => {
            let bdr = border_edges(Some(clr_black), Some(clr_black), Some(clr_black), Some(clr_black), Some(clr_black), Some(clr_black));
            xml.push_str(&format!(r#"<a:wholeTbl><a:tcTxStyle><a:schemeClr val="dk1"/></a:tcTxStyle><a:tcStyle>{bdr}</a:tcStyle></a:wholeTbl>"#));
        }
        Family::Themed1 => {
            let bdr = border_edges_w(6350, [Some(clr_base.as_str()); 6]);
            let head_bdr = thick_edge_w("bottom", clr_white, 12700);
            let total_bdr = format!("<a:tcBdr><a:top>{}</a:top><a:bottom>{}</a:bottom></a:tcBdr>", border_ln(&clr_base, 12700), border_ln(&clr_base, 12700));
            let first_bdr = thick_edge_w("right", &clr_base, 12700);
            let last_bdr = thick_edge_w("left", &clr_base, 12700);
            xml.push_str(&format!(
                r#"<a:wholeTbl><a:tcTxStyle><a:schemeClr val="dk1"/></a:tcTxStyle><a:tcStyle>{bdr}<a:fill><a:solidFill>{clr_tint50}</a:solidFill></a:fill></a:tcStyle></a:wholeTbl>
                <a:band1H><a:tcStyle><a:fill><a:solidFill>{clr_alpha40}</a:solidFill></a:fill></a:tcStyle></a:band1H>
                <a:band1V><a:tcStyle><a:fill><a:solidFill>{clr_alpha40}</a:solidFill></a:fill></a:tcStyle></a:band1V>
                <a:lastCol><a:tcTxStyle b="on"/><a:tcStyle>{last_bdr}</a:tcStyle></a:lastCol>
                <a:firstCol><a:tcTxStyle b="on"/><a:tcStyle>{first_bdr}</a:tcStyle></a:firstCol>
                <a:lastRow><a:tcTxStyle b="on"/><a:tcStyle>{total_bdr}</a:tcStyle></a:lastRow>
                <a:firstRow><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{head_bdr}<a:fill><a:solidFill>{clr_base}</a:solidFill></a:fill></a:tcStyle></a:firstRow>"#
            ));
        }
        Family::Themed2 => {
            let line = scheme_clr(meta.color, Some(50), None);
            let bdr = border_edges_w(6350, [Some(line.as_str()), Some(line.as_str()), Some(line.as_str()), Some(line.as_str()), None, None]);
            let head_bdr = thick_edge_w("bottom", clr_white, 19050);
            let total_bdr = thick_edge_w("top", clr_white, 19050);
            let first_bdr = thick_edge_w("right", clr_white, 19050);
            let last_bdr = thick_edge_w("left", clr_white, 19050);
            xml.push_str(&format!(
                r#"<a:wholeTbl><a:tcTxStyle>{clr_white}</a:tcTxStyle><a:tcStyle>{bdr}<a:fill><a:solidFill>{clr_shade80}</a:solidFill></a:fill></a:tcStyle></a:wholeTbl>
                <a:band1H><a:tcStyle><a:fill><a:solidFill><a:schemeClr val="lt1"><a:alpha val="20000"/></a:schemeClr></a:solidFill></a:fill></a:tcStyle></a:band1H>
                <a:band1V><a:tcStyle><a:fill><a:solidFill><a:schemeClr val="lt1"><a:alpha val="20000"/></a:schemeClr></a:solidFill></a:fill></a:tcStyle></a:band1V>
                <a:lastCol><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{last_bdr}</a:tcStyle></a:lastCol>
                <a:firstCol><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{first_bdr}</a:tcStyle></a:firstCol>
                <a:lastRow><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{total_bdr}</a:tcStyle></a:lastRow>
                <a:firstRow><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{head_bdr}</a:tcStyle></a:firstRow>"#
            ));
        }
        Family::Light1 | Family::Light3 => {
            let light1 = meta.family == Family::Light1;
            let b = clr_base.as_str();
            let outline = if light1 { border_edges_w(12700, [None, None, Some(b), Some(b), None, None]) } else { border_edges_w(12700, [Some(b); 6]) };
            let row_bdr = thick_edge_w("bottom", &clr_base, if light1 { 12700 } else { 25400 });
            let last_bdr = thick_edge_w("top", &clr_base, if light1 { 12700 } else { 25400 });
            xml.push_str(&format!(
                r#"<a:wholeTbl><a:tcTxStyle><a:schemeClr val="dk1"/></a:tcTxStyle><a:tcStyle>{outline}</a:tcStyle></a:wholeTbl>
                <a:band1H><a:tcStyle><a:fill><a:solidFill>{clr_alpha20}</a:solidFill></a:fill></a:tcStyle></a:band1H>
                <a:band1V><a:tcStyle><a:fill><a:solidFill>{clr_alpha20}</a:solidFill></a:fill></a:tcStyle></a:band1V>
                <a:lastCol><a:tcTxStyle b="on"/></a:lastCol>
                <a:firstCol><a:tcTxStyle b="on"/></a:firstCol>
                <a:lastRow><a:tcTxStyle b="on"/><a:tcStyle>{last_bdr}</a:tcStyle></a:lastRow>
                <a:firstRow><a:tcTxStyle b="on"/><a:tcStyle>{row_bdr}</a:tcStyle></a:firstRow>"#
            ));
        }
        Family::Light2 => {
            let b = clr_base.as_str();
            let outline = border_edges_w(6350, [Some(b), Some(b), Some(b), Some(b), None, None]);
            let band_h = border_edges_w(6350, [None, None, Some(b), Some(b), None, None]);
            let band_v = border_edges_w(6350, [Some(b), Some(b), None, None, None, None]);
            let last_bdr = thick_edge_w("top", &clr_base, 38100);
            xml.push_str(&format!(
                r#"<a:wholeTbl><a:tcTxStyle><a:schemeClr val="dk1"/></a:tcTxStyle><a:tcStyle>{outline}</a:tcStyle></a:wholeTbl>
                <a:band1H><a:tcStyle>{band_h}</a:tcStyle></a:band1H>
                <a:band1V><a:tcStyle>{band_v}</a:tcStyle></a:band1V>
                <a:lastCol><a:tcTxStyle b="on"/></a:lastCol>
                <a:firstCol><a:tcTxStyle b="on"/></a:firstCol>
                <a:lastRow><a:tcTxStyle b="on"/><a:tcStyle>{last_bdr}</a:tcStyle></a:lastRow>
                <a:firstRow><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle><a:fill><a:solidFill>{clr_base}</a:solidFill></a:fill></a:tcStyle></a:firstRow>"#
            ));
        }
        Family::Medium1 => {
            let tbl_bdr = border_edges(Some(&clr_base), Some(&clr_base), Some(&clr_base), Some(&clr_base), Some(&clr_base), None);
            xml.push_str(&format!(
                r#"<a:wholeTbl><a:tcTxStyle><a:schemeClr val="dk1"/></a:tcTxStyle><a:tcStyle>{tbl_bdr}<a:fill><a:solidFill>{clr_white}</a:solidFill></a:fill></a:tcStyle></a:wholeTbl>
                <a:band1H><a:tcStyle><a:fill><a:solidFill>{clr_tint20}</a:solidFill></a:fill></a:tcStyle></a:band1H>
                <a:band1V><a:tcStyle><a:fill><a:solidFill>{clr_tint20}</a:solidFill></a:fill></a:tcStyle></a:band1V>
                <a:lastCol><a:tcTxStyle b="on"/></a:lastCol>
                <a:firstCol><a:tcTxStyle b="on"/></a:firstCol>
                <a:lastRow><a:tcTxStyle b="on"/><a:tcStyle><a:fill><a:solidFill>{clr_white}</a:solidFill></a:fill></a:tcStyle></a:lastRow>
                <a:firstRow><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle><a:fill><a:solidFill>{clr_base}</a:solidFill></a:fill></a:tcStyle></a:firstRow>"#
            ));
        }
        Family::Medium2 => {
            let bdr = border_edges(Some(clr_white), Some(clr_white), Some(clr_white), Some(clr_white), Some(clr_white), Some(clr_white));
            let head_bdr = thick_edge("bottom", clr_white);
            let total_bdr = thick_edge("top", clr_white);
            xml.push_str(&format!(
                r#"<a:wholeTbl><a:tcTxStyle><a:schemeClr val="dk1"/></a:tcTxStyle><a:tcStyle>{bdr}<a:fill><a:solidFill>{clr_tint20}</a:solidFill></a:fill></a:tcStyle></a:wholeTbl>
                <a:band1H><a:tcStyle><a:fill><a:solidFill>{clr_tint40}</a:solidFill></a:fill></a:tcStyle></a:band1H>
                <a:band1V><a:tcStyle><a:fill><a:solidFill>{clr_tint40}</a:solidFill></a:fill></a:tcStyle></a:band1V>
                <a:lastCol><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle><a:fill><a:solidFill>{clr_base}</a:solidFill></a:fill></a:tcStyle></a:lastCol>
                <a:firstCol><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle><a:fill><a:solidFill>{clr_base}</a:solidFill></a:fill></a:tcStyle></a:firstCol>
                <a:lastRow><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{total_bdr}<a:fill><a:solidFill>{clr_base}</a:solidFill></a:fill></a:tcStyle></a:lastRow>
                <a:firstRow><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{head_bdr}<a:fill><a:solidFill>{clr_base}</a:solidFill></a:fill></a:tcStyle></a:firstRow>"#
            ));
        }
        Family::Medium3 => {
            let gray_tint = r#"<a:schemeClr val="dk1"><a:tint val="20000"/></a:schemeClr>"#;
            let bdr = format!("<a:tcBdr><a:top>{}</a:top><a:bottom>{}</a:bottom></a:tcBdr>", border_ln(clr_black, 25400), border_ln(clr_black, 25400));
            let head_bdr = thick_edge_w("bottom", clr_black, 25400);
            let total_bdr = thick_edge_w("top", clr_black, 38100);
            xml.push_str(&format!(
                r#"<a:wholeTbl><a:tcTxStyle><a:schemeClr val="dk1"/></a:tcTxStyle><a:tcStyle>{bdr}<a:fill><a:solidFill>{clr_white}</a:solidFill></a:fill></a:tcStyle></a:wholeTbl>
                <a:band1H><a:tcStyle><a:fill><a:solidFill>{gray_tint}</a:solidFill></a:fill></a:tcStyle></a:band1H>
                <a:band1V><a:tcStyle><a:fill><a:solidFill>{gray_tint}</a:solidFill></a:fill></a:tcStyle></a:band1V>
                <a:lastCol><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle><a:fill><a:solidFill>{clr_base}</a:solidFill></a:fill></a:tcStyle></a:lastCol>
                <a:firstCol><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle><a:fill><a:solidFill>{clr_base}</a:solidFill></a:fill></a:tcStyle></a:firstCol>
                <a:lastRow><a:tcTxStyle b="on">{clr_black}</a:tcTxStyle><a:tcStyle>{total_bdr}<a:fill><a:solidFill>{clr_white}</a:solidFill></a:fill></a:tcStyle></a:lastRow>
                <a:firstRow><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{head_bdr}<a:fill><a:solidFill>{clr_base}</a:solidFill></a:fill></a:tcStyle></a:firstRow>"#
            ));
        }
        Family::Medium4 => {
            let b = clr_base.as_str();
            let bdr = border_edges_w(12700, [Some(b); 6]);
            xml.push_str(&format!(
                r#"<a:wholeTbl><a:tcTxStyle><a:schemeClr val="dk1"/></a:tcTxStyle><a:tcStyle>{bdr}<a:fill><a:solidFill>{clr_tint20}</a:solidFill></a:fill></a:tcStyle></a:wholeTbl>
                <a:band1H><a:tcStyle><a:fill><a:solidFill>{clr_tint40}</a:solidFill></a:fill></a:tcStyle></a:band1H>
                <a:band1V><a:tcStyle><a:fill><a:solidFill>{clr_tint40}</a:solidFill></a:fill></a:tcStyle></a:band1V>
                <a:lastCol><a:tcTxStyle b="on"/></a:lastCol>
                <a:firstCol><a:tcTxStyle b="on"/></a:firstCol>
                <a:lastRow><a:tcTxStyle b="on"/></a:lastRow>
                <a:firstRow><a:tcTxStyle b="on"/></a:firstRow>"#
            ));
        }
        Family::Dark1 if meta.color == ColorScheme::Neutral => {
            // The grey one: tints of the text colour rather than shades of an accent.
            let clr_tint60 = scheme_clr(meta.color, Some(60), None);
            let head_bdr = thick_edge_w("bottom", clr_white, 38100);
            let total_bdr = thick_edge_w("top", clr_white, 38100);
            let first_bdr = thick_edge_w("right", clr_white, 38100);
            let last_bdr = thick_edge_w("left", clr_white, 38100);
            xml.push_str(&format!(
                r#"<a:wholeTbl><a:tcTxStyle><a:schemeClr val="dk1"/></a:tcTxStyle><a:tcStyle><a:fill><a:solidFill>{clr_tint20}</a:solidFill></a:fill></a:tcStyle></a:wholeTbl>
                <a:band1H><a:tcStyle><a:fill><a:solidFill>{clr_tint40}</a:solidFill></a:fill></a:tcStyle></a:band1H>
                <a:band1V><a:tcStyle><a:fill><a:solidFill>{clr_tint40}</a:solidFill></a:fill></a:tcStyle></a:band1V>
                <a:lastCol><a:tcTxStyle b="on"/><a:tcStyle>{last_bdr}<a:fill><a:solidFill>{clr_tint60}</a:solidFill></a:fill></a:tcStyle></a:lastCol>
                <a:firstCol><a:tcTxStyle b="on"/><a:tcStyle>{first_bdr}<a:fill><a:solidFill>{clr_tint60}</a:solidFill></a:fill></a:tcStyle></a:firstCol>
                <a:lastRow><a:tcTxStyle b="on"/><a:tcStyle>{total_bdr}<a:fill><a:solidFill>{clr_tint60}</a:solidFill></a:fill></a:tcStyle></a:lastRow>
                <a:firstRow><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{head_bdr}<a:fill><a:solidFill>{clr_black}</a:solidFill></a:fill></a:tcStyle></a:firstRow>"#
            ));
        }
        Family::Dark1 => {
            let head_bdr = thick_edge_w("bottom", clr_white, 38100);
            let total_bdr = thick_edge_w("top", clr_white, 38100);
            let first_bdr = thick_edge_w("right", clr_white, 38100);
            let last_bdr = thick_edge_w("left", clr_white, 38100);
            xml.push_str(&format!(
                r#"<a:wholeTbl><a:tcTxStyle>{clr_white}</a:tcTxStyle><a:tcStyle><a:fill><a:solidFill>{clr_base}</a:solidFill></a:fill></a:tcStyle></a:wholeTbl>
                <a:band1H><a:tcStyle><a:fill><a:solidFill>{clr_shade60}</a:solidFill></a:fill></a:tcStyle></a:band1H>
                <a:band1V><a:tcStyle><a:fill><a:solidFill>{clr_shade60}</a:solidFill></a:fill></a:tcStyle></a:band1V>
                <a:lastCol><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{last_bdr}<a:fill><a:solidFill>{clr_shade60}</a:solidFill></a:fill></a:tcStyle></a:lastCol>
                <a:firstCol><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{first_bdr}<a:fill><a:solidFill>{clr_shade60}</a:solidFill></a:fill></a:tcStyle></a:firstCol>
                <a:lastRow><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{total_bdr}<a:fill><a:solidFill>{clr_shade40}</a:solidFill></a:fill></a:tcStyle></a:lastRow>
                <a:firstRow><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle>{head_bdr}<a:fill><a:solidFill>{clr_black}</a:solidFill></a:fill></a:tcStyle></a:firstRow>"#
            ));
        }
        Family::Dark2 => {
            let clr_b = scheme_clr_b(meta.color);
            let total_bdr = thick_edge_w("top", clr_black, 38100);
            xml.push_str(&format!(
                r#"<a:wholeTbl><a:tcTxStyle><a:schemeClr val="dk1"/></a:tcTxStyle><a:tcStyle><a:fill><a:solidFill>{clr_tint20}</a:solidFill></a:fill></a:tcStyle></a:wholeTbl>
                <a:band1H><a:tcStyle><a:fill><a:solidFill>{clr_tint40}</a:solidFill></a:fill></a:tcStyle></a:band1H>
                <a:band1V><a:tcStyle><a:fill><a:solidFill>{clr_tint40}</a:solidFill></a:fill></a:tcStyle></a:band1V>
                <a:lastCol><a:tcTxStyle b="on"/></a:lastCol>
                <a:firstCol><a:tcTxStyle b="on"/></a:firstCol>
                <a:lastRow><a:tcTxStyle b="on"/><a:tcStyle>{total_bdr}</a:tcStyle></a:lastRow>
                <a:firstRow><a:tcTxStyle b="on">{clr_white}</a:tcTxStyle><a:tcStyle><a:fill><a:solidFill>{clr_b}</a:solidFill></a:fill></a:tcStyle></a:firstRow>"#
            ));
        }
    }
    xml.push_str("</a:tblStyle>");
    xml
}

fn generate_all_styles_xml() -> String {
    let mut xml = String::from(r#"<a:tblStyleLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#);
    for meta in STYLES {
        xml.push_str(&generate_tbl_style(meta));
    }
    xml.push_str("</a:tblStyleLst>");
    xml
}

static BUILTIN_STYLES: OnceLock<HashMap<&'static str, Node<'static, 'static>>> = OnceLock::new();

/// Looks up a built-in table style by its GUID (e.g. `"{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}"`).
pub(crate) fn find_builtin_style(style_id: &str) -> Option<Node<'static, 'static>> {
    let map = BUILTIN_STYLES.get_or_init(|| {
        let xml = generate_all_styles_xml();
        let leaked: &'static str = Box::leak(xml.into_boxed_str());
        let doc: &'static Document<'static> = Box::leak(Box::new(Document::parse(leaked).expect("valid builtin table styles XML")));
        let mut map = HashMap::new();
        for node in doc.root_element().children().filter(|n| n.is_element() && n.tag_name().name() == "tblStyle") {
            if let Some(id) = node.attribute("styleId") {
                map.insert(id, node);
            }
        }
        map
    });
    map.get(style_id).copied().or_else(|| {
        map.iter().find(|(k, _)| k.eq_ignore_ascii_case(style_id)).map(|(_, &v)| v)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_74_styles_resolve() {
        assert_eq!(STYLES.len(), 74);
        for s in STYLES {
            let node = find_builtin_style(s.guid);
            assert!(node.is_some(), "style {} ({}) must resolve", s.guid, s.name);
            let n = node.unwrap();
            assert_eq!(n.attribute("styleId"), Some(s.guid));
            assert_eq!(n.attribute("styleName"), Some(s.name));
        }
    }

    #[test]
    fn default_style_has_expected_parts() {
        let node = find_builtin_style("{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}").unwrap();
        assert!(node.children().any(|c| c.tag_name().name() == "wholeTbl"));
        assert!(node.children().any(|c| c.tag_name().name() == "firstRow"));
        assert!(node.children().any(|c| c.tag_name().name() == "band1H"));
    }
}
