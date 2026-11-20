// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! テーマ効果 (Theme Effects / `fmtScheme`): the format scheme of a theme
//! defining subtle, moderate, and intense fill styles, line styles,
//! effect styles (shadow, bevel, glow, reflection, soft edge), and
//! background fill styles.

/// A predefined theme effect preset corresponding to PowerPoint's built-in theme effect styles.
#[derive(Clone, Copy, Debug)]
pub struct ThemeEffectPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub label_key: &'static str,
    pub subtle_effect: &'static str,
    pub moderate_effect: &'static str,
    pub intense_effect: &'static str,
}

pub const THEME_EFFECTS: [ThemeEffectPreset; 10] = [
    ThemeEffectPreset {
        id: "office",
        name: "Office",
        label_key: "pptx.theme.effects.office",
        subtle_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="40000" dist="20000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="35000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
        moderate_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="63500" dist="25400" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
        intense_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="100000" dist="38100" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="50000"/></a:srgbClr></a:outerShdw></a:effectLst><a:sp3d><a:bevelT w="19050" h="19050" prst="circle"/></a:sp3d></a:effectStyle>"#,
    },
    ThemeEffectPreset {
        id: "office_2007_2010",
        name: "Office 2007 - 2010",
        label_key: "pptx.theme.effects.office-2007-2010",
        subtle_effect: r#"<a:effectStyle><a:effectLst/><a:sp3d><a:bevelT w="25400" h="19050" prst="circle"/></a:sp3d></a:effectStyle>"#,
        moderate_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="63500" dist="25400" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="35000"/></a:srgbClr></a:outerShdw></a:effectLst><a:sp3d><a:bevelT w="38100" h="25400" prst="circle"/></a:sp3d></a:effectStyle>"#,
        intense_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="63500" dist="25400" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw><a:reflection blurRad="6350" stA="50000" endA="300" dist="12700" dir="5400000" fadeDir="5400000" algn="b"/></a:effectLst><a:sp3d><a:bevelT w="50800" h="38100" prst="circle"/></a:sp3d></a:effectStyle>"#,
    },
    ThemeEffectPreset {
        id: "metro",
        name: "Metro",
        label_key: "pptx.theme.effects.metro",
        subtle_effect: r#"<a:effectStyle><a:effectLst/></a:effectStyle>"#,
        moderate_effect: r#"<a:effectStyle><a:effectLst/></a:effectStyle>"#,
        intense_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="20000" dist="15000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="25000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
    },
    ThemeEffectPreset {
        id: "aspect",
        name: "Aspect",
        label_key: "pptx.theme.effects.aspect",
        subtle_effect: r#"<a:effectStyle><a:effectLst><a:innerShdw blurRad="40000" dist="20000" dir="5400000"><a:srgbClr val="000000"><a:alpha val="35000"/></a:srgbClr></a:innerShdw></a:effectLst></a:effectStyle>"#,
        moderate_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="63500" dist="30000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
        intense_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="80000" dist="40000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="50000"/></a:srgbClr></a:outerShdw></a:effectLst><a:sp3d><a:bevelT w="25400" h="25400" prst="relaxedInset"/></a:sp3d></a:effectStyle>"#,
    },
    ThemeEffectPreset {
        id: "apex",
        name: "Apex",
        label_key: "pptx.theme.effects.apex",
        subtle_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="38100" dist="19050" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="35000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
        moderate_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="50800" dist="25400" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw><a:reflection blurRad="6350" stA="40000" endA="300" dist="12700" dir="5400000" fadeDir="5400000" algn="b"/></a:effectLst></a:effectStyle>"#,
        intense_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="76200" dist="38100" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="50000"/></a:srgbClr></a:outerShdw><a:reflection blurRad="12700" stA="60000" endA="300" dist="19050" dir="5400000" fadeDir="5400000" algn="b"/></a:effectLst></a:effectStyle>"#,
    },
    ThemeEffectPreset {
        id: "slate",
        name: "Slate",
        label_key: "pptx.theme.effects.slate",
        subtle_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="30000" dist="20000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="30000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
        moderate_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="60000" dist="35000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="45000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
        intense_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="120000" dist="50000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="60000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
    },
    ThemeEffectPreset {
        id: "median",
        name: "Median",
        label_key: "pptx.theme.effects.median",
        subtle_effect: r#"<a:effectStyle><a:effectLst><a:softEdge rad="25400"/></a:effectLst></a:effectStyle>"#,
        moderate_effect: r#"<a:effectStyle><a:effectLst><a:softEdge rad="38100"/><a:outerShdw blurRad="50000" dist="25000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="35000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
        intense_effect: r#"<a:effectStyle><a:effectLst><a:glow rad="63500"><a:schemeClr val="phClr"><a:alpha val="40000"/></a:schemeClr></a:glow><a:outerShdw blurRad="70000" dist="35000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="50000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
    },
    ThemeEffectPreset {
        id: "glossy",
        name: "Glossy",
        label_key: "pptx.theme.effects.glossy",
        subtle_effect: r#"<a:effectStyle><a:effectLst/><a:sp3d><a:bevelT w="25400" h="19050" prst="relaxedInset"/></a:sp3d></a:effectStyle>"#,
        moderate_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="63500" dist="25400" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="35000"/></a:srgbClr></a:outerShdw></a:effectLst><a:sp3d><a:bevelT w="38100" h="25400" prst="relaxedInset"/></a:sp3d></a:effectStyle>"#,
        intense_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="80000" dist="35000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="45000"/></a:srgbClr></a:outerShdw><a:reflection blurRad="6350" stA="50000" endA="300" dist="12700" dir="5400000" fadeDir="5400000" algn="b"/></a:effectLst><a:sp3d><a:bevelT w="50800" h="38100" prst="relaxedInset"/></a:sp3d></a:effectStyle>"#,
    },
    ThemeEffectPreset {
        id: "bubbles",
        name: "Bubbles",
        label_key: "pptx.theme.effects.bubbles",
        subtle_effect: r#"<a:effectStyle><a:effectLst/><a:sp3d><a:bevelT w="38100" h="38100" prst="circle"/></a:sp3d></a:effectStyle>"#,
        moderate_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="50000" dist="25000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="35000"/></a:srgbClr></a:outerShdw></a:effectLst><a:sp3d><a:bevelT w="50800" h="50800" prst="circle"/></a:sp3d></a:effectStyle>"#,
        intense_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="75000" dist="35000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="45000"/></a:srgbClr></a:outerShdw><a:reflection blurRad="6350" stA="50000" endA="300" dist="12700" dir="5400000" fadeDir="5400000" algn="b"/></a:effectLst><a:sp3d><a:bevelT w="63500" h="63500" prst="circle"/></a:sp3d></a:effectStyle>"#,
    },
    ThemeEffectPreset {
        id: "shadow",
        name: "Shadow",
        label_key: "pptx.theme.effects.shadow",
        subtle_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="50000" dist="25000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
        moderate_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="80000" dist="40000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="50000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
        intense_effect: r#"<a:effectStyle><a:effectLst><a:outerShdw blurRad="150000" dist="70000" dir="5400000" algn="b" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="65000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle>"#,
    },
];

/// Find preset by ID or name (case-insensitive).
pub fn find_effect_preset(name_or_id: &str) -> Option<&'static ThemeEffectPreset> {
    THEME_EFFECTS.iter().find(|p| p.id.eq_ignore_ascii_case(name_or_id) || p.name.eq_ignore_ascii_case(name_or_id))
}

/// Generate the full `<a:fmtScheme>` XML string for a given preset.
pub fn generate_fmt_scheme_xml(preset: &ThemeEffectPreset) -> String {
    format!(
        r#"<a:fmtScheme name="{}"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:tint val="50000"/><a:satMod val="300000"/></a:schemeClr></a:gs><a:gs pos="35000"><a:schemeClr val="phClr"><a:tint val="37000"/><a:satMod val="220000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:tint val="15000"/><a:satMod val="131000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="16200000" scaled="1"/></a:gradFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:shade val="100000"/><a:satMod val="130000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:tint val="50000"/><a:satMod val="350000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="16200000" scaled="0"/></a:gradFill></a:fillStyleLst><a:lnStyleLst><a:ln w="9525" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln><a:ln w="25400" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln><a:ln w="38100" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln></a:lnStyleLst><a:effectStyleLst>{}{}{}</a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"><a:tint val="95000"/><a:satMod val="170000"/></a:schemeClr></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:tint val="93000"/><a:satMod val="150000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:shade val="98000"/><a:satMod val="130000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill></a:bgFillStyleLst></a:fmtScheme>"#,
        preset.name, preset.subtle_effect, preset.moderate_effect, preset.intense_effect
    )
}
