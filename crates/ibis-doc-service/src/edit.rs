// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What the edit tools share: running commands as one undo step, the result of an edit, and the shapes of the format arguments (design 6.6).

use ibis_doc_model::{Command, Rgb};
use serde_json::{Value, json};

use crate::error::ToolError;
use crate::service::Open;

/// The commands as one step of the undo history (a failure puts the document back as it was).
pub(crate) fn execute(open: &mut Open, commands: Vec<Command>) -> Result<(), ToolError> {
    open.document.execute(commands).map_err(|e| ToolError::new("edit_rejected", format!("{e:?}")))
}

/// The result of an edit tool: the handle, whether there are unsaved changes, the undo state, and what was changed besides what was asked.
pub(crate) fn edited(id: &str, open: &Open, mut extra: Value, warnings: Vec<Value>) -> Value {
    if !extra.is_object() {
        extra = json!({});
    }
    extra["document_id"] = json!(id);
    extra["dirty"] = json!(open.document.is_dirty());
    extra["can_undo"] = json!(open.document.can_undo());
    extra["warnings"] = json!(warnings);
    extra
}

/// A colour argument (design 6.6): `"#RRGGBB"`, or a theme colour `{ "theme": name, "brightness"?: -1.0..1.0 }`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ColorArg {
    Rgb(Rgb),
    Theme { name: String, brightness: f64 },
    /// `null`: back to the colour the styles give.
    Inherit,
}

pub(crate) const THEME_NAMES: [&str; 12] = ["text1", "background1", "text2", "background2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hyperlink", "followed_hyperlink"];

pub(crate) fn parse_color(name: &str, value: &Value) -> Result<ColorArg, ToolError> {
    let bad = || ToolError::invalid(format!("{name} is \"#RRGGBB\" or {{ \"theme\": text1|background1|text2|background2|accent1..accent6|hyperlink|followed_hyperlink, \"brightness\"?: -1..1 }}"));
    match value {
        Value::String(s) => {
            let hex = s.strip_prefix('#').filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit())).ok_or_else(bad)?;
            Ok(ColorArg::Rgb(Rgb(u32::from_str_radix(hex, 16).map_err(|_| bad())?)))
        }
        Value::Object(map) => {
            let theme = map.get("theme").and_then(Value::as_str).filter(|t| THEME_NAMES.contains(t)).ok_or_else(bad)?;
            let brightness = match map.get("brightness") {
                None | Some(Value::Null) => 0.0,
                Some(v) => v.as_f64().filter(|b| (-1.0..=1.0).contains(b)).ok_or_else(bad)?,
            };
            Ok(ColorArg::Theme { name: theme.to_string(), brightness })
        }
        Value::Null => Ok(ColorArg::Inherit),
        _ => Err(bad()),
    }
}

/// The error of a format a screen cannot take back to the inherited one yet (`null`).
pub(crate) fn no_inherit(name: &str) -> ToolError {
    ToolError::new("unsupported_operation", format!("{name}: null (back to the inherited value) is not supported on this screen yet"))
}

/// A size in points (design 6.6) as hundredths of a point: 1 to 409.5 in steps of 0.5, as Office has it.
pub(crate) fn parse_size(name: &str, value: &Value) -> Result<u32, ToolError> {
    let bad = || ToolError::invalid(format!("{name} is a number of points, 1 to 409.5 in steps of 0.5"));
    let pt = value.as_f64().ok_or_else(bad)?;
    if !(1.0..=409.5).contains(&pt) || (pt * 2.0).fract().abs() > 1e-9 {
        return Err(bad());
    }
    Ok((pt * 100.0).round() as u32)
}

/// A typeface argument: a name for every script, `{ latin?, east_asian?, complex_script? }`, or `{ theme: major|minor }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FontArg {
    All(String),
    Split { latin: Option<String>, east_asian: Option<String>, complex_script: Option<String> },
    /// The theme's heading (`major`) or body (`minor`) typeface.
    Theme { major: bool },
    /// `null`: back to the typeface the styles give.
    Inherit,
}

pub(crate) fn parse_font(name: &str, value: &Value) -> Result<FontArg, ToolError> {
    let bad = || ToolError::invalid(format!("{name} is a typeface name or {{ latin?, east_asian?, complex_script? }}"));
    match value {
        Value::String(s) if !s.trim().is_empty() => Ok(FontArg::All(s.trim().to_string())),
        Value::Object(map) if map.contains_key("theme") => match (map.len(), map.get("theme").and_then(Value::as_str)) {
            (1, Some("major")) => Ok(FontArg::Theme { major: true }),
            (1, Some("minor")) => Ok(FontArg::Theme { major: false }),
            _ => Err(ToolError::invalid(format!("{name} is {{ \"theme\": \"major\" | \"minor\" }} for a theme typeface"))),
        },
        Value::Object(map) => {
            let get = |k: &str| -> Result<Option<String>, ToolError> {
                match map.get(k) {
                    None | Some(Value::Null) => Ok(None),
                    Some(Value::String(s)) if !s.trim().is_empty() => Ok(Some(s.trim().to_string())),
                    _ => Err(bad()),
                }
            };
            let split = FontArg::Split { latin: get("latin")?, east_asian: get("east_asian")?, complex_script: get("complex_script")? };
            if split == (FontArg::Split { latin: None, east_asian: None, complex_script: None }) {
                return Err(bad());
            }
            Ok(split)
        }
        Value::Null => Ok(FontArg::Inherit),
        _ => Err(bad()),
    }
}

/// A true or false argument.
pub(crate) fn parse_bool(name: &str, value: &Value) -> Result<bool, ToolError> {
    value.as_bool().ok_or_else(|| ToolError::invalid(format!("{name} is true or false")))
}

/// The arguments that are not a known name (a misspelled `colour` is not ignored).
pub(crate) fn only_known(args: &Value, known: &[&str]) -> Result<(), ToolError> {
    if let Some(map) = args.as_object() {
        for key in map.keys() {
            if !known.contains(&key.as_str()) {
                return Err(ToolError::invalid(format!("{key} is not an argument of this tool")));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_sizes_and_fonts_have_one_shape() {
        assert_eq!(parse_color("color", &json!("#ff8000")).unwrap(), ColorArg::Rgb(Rgb(0xFF8000)));
        assert_eq!(parse_color("color", &json!({ "theme": "accent1", "brightness": 0.4 })).unwrap(), ColorArg::Theme { name: "accent1".into(), brightness: 0.4 });
        for bad in [json!("ff8000"), json!("#ff80"), json!("#gg0000"), json!({ "theme": "accent9" }), json!({ "theme": "accent1", "brightness": 2 }), json!(5)] {
            assert_eq!(parse_color("color", &bad).unwrap_err().code, "invalid_argument", "{bad}");
        }
        assert_eq!(parse_color("color", &Value::Null).unwrap(), ColorArg::Inherit);
        assert_eq!(parse_size("size", &json!(10.5)).unwrap(), 1050);
        for bad in [json!(0.5), json!(410), json!(10.3), json!("12")] {
            assert_eq!(parse_size("size", &bad).unwrap_err().code, "invalid_argument", "{bad}");
        }
        assert_eq!(parse_font("font", &json!("BIZ UDゴシック")).unwrap(), FontArg::All("BIZ UDゴシック".into()));
        assert_eq!(parse_font("font", &json!({ "east_asian": "游明朝" })).unwrap(), FontArg::Split { latin: None, east_asian: Some("游明朝".into()), complex_script: None });
        assert_eq!(parse_font("font", &json!({})).unwrap_err().code, "invalid_argument");
        assert_eq!(parse_font("font", &json!({ "theme": "minor" })).unwrap(), FontArg::Theme { major: false });
        assert_eq!(parse_font("font", &json!({ "theme": "major" })).unwrap(), FontArg::Theme { major: true });
        assert_eq!(parse_font("font", &Value::Null).unwrap(), FontArg::Inherit);
        for bad in [json!({ "theme": "other" }), json!({ "theme": "minor", "latin": "Arial" })] {
            assert_eq!(parse_font("font", &bad).unwrap_err().code, "invalid_argument", "{bad}");
        }
    }
}
