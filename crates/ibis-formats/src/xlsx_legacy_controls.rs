// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Form controls of the Excel 2007 form (フォーム コントロール, legacy): the worksheet has no `controls`
//! element, and the controls are only the VML shapes of its `legacyDrawing` (`x:ClientData ObjectType="Radio"`
//! and so on), with a `ctrlProp` part for each, related from the sheet in the order of the shapes.
//!
//! The VML gives the place (`style`: `margin-left`, `margin-top`, `width`, `height` in points, and
//! `x:Anchor`), the text (`v:textbox`) and the state (`x:FmlaLink`, `x:Checked`, `x:Sel`, `x:Val`, ...).

use ibis_doc_model::{ControlKind, FormControl, Rgb};
use roxmltree::{Document, Node};

/// A control found in the VML.
pub(crate) struct LegacyControl {
    pub control: FormControl,
    /// The number of the shape (`1043` of `_x0000_s1043`), as a `control`'s `shapeId` is.
    pub shape_number: String,
    /// Its place from the style, in EMU: x, y, width, height.
    pub frame: Option<(i64, i64, i64, i64)>,
    /// `x:Anchor`: left column, left offset, top row, top offset, right column, right offset, bottom row,
    /// bottom offset (the offsets in pixels).
    pub anchor: Option<[i64; 8]>,
    pub hidden: bool,
}

const EMU_PER_POINT: f64 = 12_700.0;

fn kind_of(object_type: &str) -> Option<ControlKind> {
    Some(match object_type.to_ascii_lowercase().as_str() {
        "button" => ControlKind::Button,
        "checkbox" => ControlKind::CheckBox,
        "radio" => ControlKind::Radio,
        "scroll" => ControlKind::Scroll,
        "spin" => ControlKind::Spin,
        "drop" => ControlKind::Drop,
        "list" => ControlKind::List,
        "gbox" => ControlKind::GroupBox,
        "label" => ControlKind::Label,
        _ => return None,
    })
}

fn local(node: Node, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

fn style_points(style: &str, key: &str) -> Option<f64> {
    style.split(';').find_map(|kv| {
        let (k, v) = kv.split_once(':')?;
        if !k.trim().eq_ignore_ascii_case(key) {
            return None;
        }
        v.trim().strip_suffix("pt")?.trim().parse::<f64>().ok()
    })
}

fn style_value<'a>(style: &'a str, key: &str) -> Option<&'a str> {
    style.split(';').find_map(|kv| {
        let (k, v) = kv.split_once(':')?;
        k.trim().eq_ignore_ascii_case(key).then(|| v.trim())
    })
}

/// The control shapes of `vml`, in the order of the file, each with the `ctrlProp` (its XML) of the same position
/// in `props` (the parts related from the sheet, in the order of their relationship ids) when the two lists have the
/// same kinds in the same order. `props` holds (part name, XML text).
pub(crate) fn read(vml: &Document, props: &[(String, String)]) -> Vec<LegacyControl> {
    struct Raw<'a> {
        shape: Node<'a, 'a>,
        client: Node<'a, 'a>,
        kind: ControlKind,
        object_type: String,
    }
    let mut raws: Vec<Raw> = Vec::new();
    for shape in vml.descendants().filter(|n| local(*n, "shape")) {
        let Some(client) = shape.children().find(|n| local(*n, "ClientData")) else { continue };
        let Some(object_type) = client.attributes().find(|a| a.name() == "ObjectType").map(|a| a.value().to_string()) else { continue };
        let Some(kind) = kind_of(&object_type) else { continue };
        raws.push(Raw { shape, client, kind, object_type });
    }
    // The ctrlProp parts, when they line up with the shapes (same count, same kinds in the same order).
    let parsed: Vec<Option<Document>> = props.iter().map(|(_, xml)| Document::parse(xml).ok()).collect();
    let prop_type = |d: &Option<Document>| d.as_ref().and_then(|d| d.root_element().attribute("objectType").map(str::to_string));
    let lined_up = !raws.is_empty()
        && raws.len() == props.len()
        && raws.iter().zip(&parsed).all(|(r, d)| prop_type(d).is_some_and(|t| t.eq_ignore_ascii_case(&r.object_type)));
    let mut out = Vec::new();
    for (i, raw) in raws.iter().enumerate() {
        let id = raw.shape.attribute("id").unwrap_or("");
        let number = id.rsplit("_s").next().unwrap_or(id).to_string();
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let prop = if lined_up { parsed[i].as_ref().map(|d| d.root_element()) } else { None };
        let vml_tag = |name: &str| raw.client.children().find(|n| local(*n, name));
        let vml_text = |name: &str| vml_tag(name).and_then(|n| n.text()).map(|t| t.trim().to_string());
        // A property: the ctrlProp's attribute, else the VML element.
        let get = |attr: &str, tag: &str| -> Option<String> {
            prop.and_then(|p| p.attribute(attr).map(str::to_string)).or_else(|| vml_text(tag))
        };
        let number_of = |attr: &str, tag: &str, default: i64| get(attr, tag).and_then(|v| v.parse::<i64>().ok()).unwrap_or(default);
        let kind = raw.kind;
        let number_n: i64 = number.parse().unwrap_or(0);
        let english = match kind {
            ControlKind::Button => "Button",
            ControlKind::CheckBox => "Check Box",
            ControlKind::Radio => "Option Button",
            ControlKind::Scroll => "Scroll Bar",
            ControlKind::Spin => "Spin Button",
            ControlKind::Drop => "Drop Down",
            ControlKind::List => "List Box",
            ControlKind::GroupBox => "Group Box",
            ControlKind::Label => "Label",
        };
        // Excel numbers the names from the shape number (1043 is "Option Button 19": 1043 - 1024).
        let mut c = FormControl::new(kind, &format!("{english} {}", number_n % 1024));
        c.vml_id = id.to_string();
        if let Some(i) = lined_up.then_some(i) {
            c.part = props[i].0.clone();
        }
        c.link = get("fmlaLink", "FmlaLink").filter(|s| !s.is_empty());
        c.range = get("fmlaRange", "FmlaRange").filter(|s| !s.is_empty());
        c.min = number_of("min", "Min", 0);
        c.max = number_of("max", "Max", if kind == ControlKind::Scroll { 100 } else { 30000 });
        c.step = number_of("inc", "Inc", 1);
        c.page = number_of("page", "Page", 10);
        c.horizontal = get("horiz", "Horiz").is_some_and(|v| v == "1") || (prop.is_none() && vml_tag("Horiz").is_some());
        c.checked = match prop {
            // The ctrlProp has no `checked` when the control is not.
            Some(p) => match p.attribute("checked") {
                Some("Checked") => 1,
                Some("Mixed") => 2,
                _ => 0,
            },
            None => match vml_text("Checked").as_deref() {
                Some("1") => 1,
                Some("2") => 2,
                _ => 0,
            },
        };
        c.first_button = prop.is_some_and(|p| p.attribute("firstButton") == Some("1")) || vml_tag("FirstButton").is_some();
        c.three_d = match kind {
            ControlKind::Button | ControlKind::Scroll | ControlKind::Spin => true,
            ControlKind::Label => false,
            _ => match prop {
                Some(p) => !matches!(p.attribute("noThreeD"), Some("1" | "true")),
                None => vml_tag("NoThreeD").is_none(),
            },
        };
        c.value = if matches!(kind, ControlKind::Drop | ControlKind::List) { number_of("sel", "Sel", 0) } else { number_of("val", "Val", 0) };
        if kind == ControlKind::Radio {
            c.value = 0;
        }
        // The text and its character properties: the `v:textbox`'s HTML.
        if let Some(textbox) = raw.shape.children().find(|n| local(*n, "textbox")) {
            let joined: String = textbox.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect::<Vec<_>>().join("");
            c.text = joined.split_whitespace().collect::<Vec<_>>().join(" ");
            if let Some(font) = textbox.descendants().find(|n| local(*n, "font")) {
                c.font = font.attribute("face").filter(|s| !s.is_empty()).map(str::to_string);
                // `size` is in twentieths of a point; `sz` in hundredths.
                c.font_size = font.attribute("size").and_then(|v| v.trim().parse::<i32>().ok()).map(|v| v * 5);
                c.text_color = font
                    .attribute("color")
                    .and_then(|v| v.trim().strip_prefix('#'))
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .map(Rgb);
            }
            c.bold = textbox.descendants().any(|n| local(n, "b"));
            c.italic = textbox.descendants().any(|n| local(n, "i"));
        }
        let style = raw.shape.attribute("style").unwrap_or("");
        let frame = match (style_points(style, "margin-left"), style_points(style, "margin-top"), style_points(style, "width"), style_points(style, "height")) {
            (Some(x), Some(y), Some(w), Some(h)) => Some(((x * EMU_PER_POINT).round() as i64, (y * EMU_PER_POINT).round() as i64, (w * EMU_PER_POINT).round() as i64, (h * EMU_PER_POINT).round() as i64)),
            _ => None,
        };
        let anchor = vml_text("Anchor").and_then(|t| {
            let v: Vec<i64> = t.split(',').filter_map(|p| p.trim().parse::<i64>().ok()).collect();
            <[i64; 8]>::try_from(v).ok()
        });
        let hidden = style_value(style, "visibility").is_some_and(|v| v.eq_ignore_ascii_case("hidden"));
        out.push(LegacyControl { control: c, shape_number: number, frame, anchor, hidden });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const VML: &str = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel">
<v:shape id="_x0000_s1043" type="#_x0000_t201" style='position:absolute;margin-left:139.5pt;margin-top:87pt;width:24pt;height:16.5pt;z-index:4;visibility:hidden'>
 <v:textbox><div style='text-align:left'><font face="ＭＳ Ｐゴシック" size="220" color="#112233">中型</font></div></v:textbox>
 <x:ClientData ObjectType="Radio"><x:Anchor>
  6, 6, 5, 5, 7, 9, 5, 27</x:Anchor><x:Checked>1</x:Checked><x:FmlaLink>$AW$9</x:FmlaLink><x:FirstButton/></x:ClientData></v:shape>
<v:shape id="_x0000_s1056" type="#_x0000_t201" style='position:absolute;margin-left:1pt;margin-top:2pt;width:30pt;height:20pt'>
 <x:ClientData ObjectType="Drop"><x:Sel>3</x:Sel><x:FmlaLink>$AW$5</x:FmlaLink><x:FmlaRange>$AX$13:$AX$22</x:FmlaRange></x:ClientData></v:shape>
<v:shape id="_x0000_s1025" type="#_x0000_t202"><x:ClientData ObjectType="Note"><x:Row>1</x:Row></x:ClientData></v:shape>
</xml>"##;

    #[test]
    fn reads_the_control_shapes_and_not_the_notes() {
        let doc = Document::parse(VML).unwrap();
        let found = read(&doc, &[]);
        assert_eq!(found.len(), 2);
        let radio = &found[0];
        assert_eq!(radio.shape_number, "1043");
        assert_eq!(radio.control.kind, ControlKind::Radio);
        assert_eq!(radio.control.text, "中型");
        assert_eq!(radio.control.link.as_deref(), Some("$AW$9"));
        assert_eq!(radio.control.checked, 1);
        assert!(radio.control.first_button);
        assert_eq!(radio.control.font_size, Some(1100));
        assert_eq!(radio.control.text_color, Some(Rgb(0x112233)));
        assert!(radio.hidden);
        assert_eq!(radio.frame, Some((1_771_650, 1_104_900, 304_800, 209_550)));
        assert_eq!(radio.anchor, Some([6, 6, 5, 5, 7, 9, 5, 27]));
        let drop = &found[1];
        assert_eq!(drop.control.kind, ControlKind::Drop);
        assert_eq!(drop.control.value, 3);
        assert_eq!(drop.control.range.as_deref(), Some("$AX$13:$AX$22"));
        assert!(!drop.hidden);
    }

    #[test]
    fn takes_the_state_from_the_ctrl_prop_that_lines_up() {
        let doc = Document::parse(VML).unwrap();
        let props = vec![
            ("xl/ctrlProps/ctrlProp1.xml".to_string(), r#"<formControlPr xmlns="x" objectType="Radio" fmlaLink="$B$1"/>"#.to_string()),
            ("xl/ctrlProps/ctrlProp2.xml".to_string(), r#"<formControlPr xmlns="x" objectType="Drop" sel="2" fmlaLink="$AW$5"/>"#.to_string()),
        ];
        let found = read(&doc, &props);
        assert_eq!(found[0].control.part, "xl/ctrlProps/ctrlProp1.xml");
        assert_eq!(found[0].control.link.as_deref(), Some("$B$1"));
        assert_eq!(found[0].control.checked, 0);
        assert_eq!(found[1].control.value, 2);
        // Kinds out of order: no ctrlProp is taken.
        let wrong = vec![props[1].clone(), props[0].clone()];
        assert!(read(&doc, &wrong).iter().all(|c| c.control.part.is_empty()));
    }
}
