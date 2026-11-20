// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Numbers as Word's fields write them: the format switches (`\* ROMAN`,
//! `\* CIRCLENUM`, `\# "000"`…) of `PAGE`, `NUMPAGES`, `SECTION` and the
//! like, and the arithmetic of a formula field (`= { PAGE } + 10`).

/// The words of a field's code, a quoted argument as one word (without its
/// quotes).
pub fn field_words(instruction: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = instruction.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            chars.next();
            out.push(chars.by_ref().take_while(|&c| c != '"').collect());
        } else {
            let mut w = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == '"' {
                    break;
                }
                w.push(c);
                chars.next();
            }
            out.push(w);
        }
    }
    out
}

/// `ST_NumberFormat` of a `\*` switch (`format_page_number`'s names).
fn number_format(switch: &str) -> Option<&'static str> {
    Some(match switch {
        "ROMAN" => "upperRoman",
        "roman" => "lowerRoman",
        "ALPHABETIC" => "upperLetter",
        "alphabetic" => "lowerLetter",
        _ => match switch.to_ascii_lowercase().as_str() {
            "arabic" | "sbchar" => "decimal",
            "arabicdash" => "numberInDash",
            "circlenum" => "decimalEnclosedCircle",
            "dbchar" => "decimalFullWidth",
            "kanjinum1" => "japaneseCounting",
            "kanjinum2" => "japaneseLegal",
            "kanjinum3" => "japaneseDigitalTenThousand",
            "ordinal" => "ordinal",
            "cardtext" => "cardinalText",
            "ordtext" => "ordinalText",
            "hex" => "hex",
            _ => return None,
        },
    })
}

/// Number `n` written by the switches of field `instruction`: a numeric
/// picture (`\#`) or a number format (`\*`), then the case (`\* Upper`…).
/// `None` when it has neither picture nor number format (the field keeps
/// its default: the section's page number format for `PAGE`).
pub fn format_field_number(n: i64, instruction: &str) -> Option<String> {
    let words = field_words(instruction);
    let mut picture = None;
    let mut format = None;
    let mut case = None;
    let mut i = 1;
    while i < words.len() {
        match words[i].as_str() {
            "\\#" => {
                picture = words.get(i + 1).cloned();
                i += 1;
            }
            "\\*" => {
                if let Some(w) = words.get(i + 1) {
                    if let Some(f) = number_format(w) {
                        format = Some(f);
                    } else if matches!(w.to_ascii_lowercase().as_str(), "upper" | "lower" | "caps" | "firstcap") {
                        case = Some(w.to_ascii_lowercase());
                    }
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    let text = match (&picture, format) {
        (Some(p), _) => numeric_picture(n as f64, p),
        (None, Some(f)) if n >= 1 => {
            let s = crate::flow::format_list_number(n as u32, f);
            if f == "cardinalText" || f == "ordinalText" { s.to_lowercase() } else { s }
        }
        (None, Some(_)) => n.to_string(),
        (None, None) if case.is_some() => n.to_string(),
        (None, None) => return None,
    };
    Some(match case.as_deref() {
        Some("upper") => text.to_uppercase(),
        Some("lower") => text.to_lowercase(),
        Some("caps") | Some("firstcap") => {
            let mut c = text.chars();
            c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        }
        _ => text,
    })
}

/// Word's default formatting for a LISTNUM level without an explicit `\*` switch:
/// Level 1: `1)`, Level 2: `a)`, Level 3: `i)`, Level 4: `(1)`, Level 5: `(a)`,
/// Level 6: `(i)`, Level 7: `1.`, Level 8: `a.`, Level 9: `i.`.
pub fn default_listnum_format(n: u32, level: u32) -> String {
    let lvl = if (1..=9).contains(&level) { level } else { 1 };
    match lvl {
        1 => format!("{n})"),
        2 => {
            let letter = crate::flow::format_list_number(n, "lowerLetter");
            format!("{letter})")
        }
        3 => {
            let roman = crate::flow::format_list_number(n, "lowerRoman");
            format!("{roman})")
        }
        4 => format!("({n})"),
        5 => {
            let letter = crate::flow::format_list_number(n, "lowerLetter");
            format!("({letter})")
        }
        6 => {
            let roman = crate::flow::format_list_number(n, "lowerRoman");
            format!("({roman})")
        }
        7 => format!("{n}."),
        8 => {
            let letter = crate::flow::format_list_number(n, "lowerLetter");
            format!("{letter}.")
        }
        _ => {
            let roman = crate::flow::format_list_number(n, "lowerRoman");
            format!("{roman}.")
        }
    }
}

/// Format a LISTNUM field number `n` at `level`, honoring switches (`\*`, `\#`)
/// in `instruction` if present, or falling back to Word's default format.
pub fn format_listnum(n: u32, level: u32, instruction: &str) -> String {
    if let Some(formatted) = format_field_number(n as i64, instruction) {
        formatted
    } else {
        default_listnum_format(n, level)
    }
}

/// Format an AUTONUM, AUTONUMLGL, or AUTONUMOUT field number `n` (ECMA-376 Part 1 §17.16.5.3-5).
/// Honors switches (`\*`, `\#`) if present, or falls back to Word's default:
/// - AUTONUM: `1.`
/// - AUTONUMLGL: `1.`
/// - AUTONUMOUT: Roman numeral `I.`
pub fn format_autonum(n: u32, kind: &str, instruction: &str) -> String {
    if let Some(formatted) = format_field_number(n as i64, instruction) {
        formatted
    } else {
        match kind.to_ascii_uppercase().as_str() {
            "AUTONUMOUT" => {
                let roman = crate::flow::format_list_number(n, "upperRoman");
                format!("{roman}.")
            }
            _ => format!("{n}."),
        }
    }
}

/// Format a FILESIZE field based on document file size in bytes and instruction switches (ECMA-376 Part 1 §17.16.5.23).
/// Switches:
/// - `\k`: Display size in kilobytes (KB)
/// - `\m`: Display size in megabytes (MB)
/// - `\#`: Numeric picture switch
pub fn format_filesize(bytes: u64, instruction: &str) -> String {
    let words = field_words(instruction);
    let is_kb = words.iter().any(|w| w.eq_ignore_ascii_case("\\k"));
    let is_mb = words.iter().any(|w| w.eq_ignore_ascii_case("\\m"));
    let val: f64 = if is_mb {
        (bytes as f64 / (1024.0 * 1024.0)).round()
    } else if is_kb {
        (bytes as f64 / 1024.0).round()
    } else {
        bytes as f64
    };
    if let Some(formatted) = format_field_number(val as i64, instruction) {
        formatted
    } else {
        (val as u64).to_string()
    }
}

/// Format a `DOCVARIABLE` field value from document variables (ECMA-376 Part 1 §17.16.5.18).
/// Looks up the variable name in `variables` (case-insensitively) and applies text switches.
pub fn format_doc_variable(
    instruction: &str,
    variables: &std::collections::BTreeMap<String, String>,
) -> String {
    let words = field_words(instruction);
    // DOCVARIABLE "VarName" or DOCVARIABLE VarName
    let var_name = words.get(1).map(|w| w.trim_matches('"')).unwrap_or("");
    let val = variables
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(var_name))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    apply_instruction_text_switches(val.to_string(), instruction)
}

/// Format a `USERNAME` field value with instruction switches (ECMA-376 Part 1 §17.16.5.65).
pub fn format_user_name(name: &str, instruction: &str) -> String {
    apply_instruction_text_switches(name.to_string(), instruction)
}

/// Format a `USERINITIALS` field value with instruction switches (ECMA-376 Part 1 §17.16.5.64).
pub fn format_user_initials(initials: &str, instruction: &str) -> String {
    apply_instruction_text_switches(initials.to_string(), instruction)
}

/// Format a `USERADDRESS` field value with instruction switches (ECMA-376 Part 1 §17.16.5.66).
pub fn format_user_address(address: &str, instruction: &str) -> String {
    apply_instruction_text_switches(address.to_string(), instruction)
}

/// Format an `AUTOTEXT` field value (ECMA-376 Part 1 §17.16.5.6).
/// Syntax: `AUTOTEXT AutoTextName [Switches]`
pub fn format_autotext(instruction: &str) -> String {
    let words = field_words(instruction);
    let entry_name = words.get(1).map(|w| w.trim_matches('"')).unwrap_or("");
    let text = if entry_name.is_empty() {
        String::new()
    } else {
        entry_name.to_string()
    };
    apply_instruction_text_switches(text, instruction)
}

/// Format an `AUTOTEXTLIST` field value (ECMA-376 Part 1 §17.16.5.7).
/// Syntax: `AUTOTEXTLIST "LiteralText" [\s "StyleName"] [\t "TipText"] [Switches]`
pub fn format_autotextlist(instruction: &str) -> String {
    let words = field_words(instruction);
    let literal_text = words.get(1).map(|w| w.trim_matches('"')).unwrap_or("");
    apply_instruction_text_switches(literal_text.to_string(), instruction)
}

/// Format a `CITATION` field value (ECMA-376 Part 1 §17.16.5.15).
/// Syntax: `CITATION Tag [\s Style] [\l Locale] [\p Pages] [Switches]`
pub fn format_citation(instruction: &str) -> String {
    let words = field_words(instruction);
    let tag = words.get(1).map(|w| w.trim_matches('"')).unwrap_or("");
    let text = if tag.is_empty() {
        String::new()
    } else {
        format!("({tag})")
    };
    apply_instruction_text_switches(text, instruction)
}

/// Format a `BIBLIOGRAPHY` field value (ECMA-376 Part 1 §17.16.5.8).
/// Syntax: `BIBLIOGRAPHY [\l Locale] [Switches]`
pub fn format_bibliography(instruction: &str) -> String {
    let words = field_words(instruction);
    let locale_is_en = words.iter().position(|w| w == "\\l")
        .and_then(|idx| words.get(idx + 1))
        .map(|loc| loc == "1033")
        .unwrap_or(false);
    let heading = if locale_is_en {
        "Bibliography"
    } else {
        "文献目録"
    };
    apply_instruction_text_switches(heading.to_string(), instruction)
}

/// Compute initials from a user's full name.
pub fn initials_from_name(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    if words.len() >= 2 && words.iter().all(|w| w.chars().next().is_some_and(|c| c.is_ascii_alphabetic())) {
        return words.iter().take(2).filter_map(|w| w.chars().next()).flat_map(char::to_uppercase).collect();
    }
    name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

/// Format a `QUOTE` field value (ECMA-376 Part 1 §17.16.5.48).
pub fn format_quote(instruction: &str) -> String {
    let text = extract_quote_text(instruction);
    apply_instruction_text_switches(text, instruction)
}

fn extract_quote_text(instruction: &str) -> String {
    let trimmed = instruction.trim();
    let rest = if let Some(stripped) = trimmed.strip_prefix("QUOTE") {
        stripped.trim()
    } else {
        trimmed
    };
    // スイッチ（\* など）の前を取り出す
    let arg_part = if let Some(idx) = rest.find('\\') {
        rest[..idx].trim()
    } else {
        rest
    };
    if arg_part.is_empty() {
        return String::new();
    }
    // 二重引用符または単一引用符で囲まれている場合
    if (arg_part.starts_with('"') && arg_part.ends_with('"') && arg_part.len() >= 2)
        || (arg_part.starts_with('\'') && arg_part.ends_with('\'') && arg_part.len() >= 2)
    {
        return arg_part[1..arg_part.len() - 1].to_string();
    }
    // 空白区切りの数値列（65 66 67 や 0x41 等）かチェック
    let tokens: Vec<&str> = arg_part.split_whitespace().collect();
    if !tokens.is_empty() && tokens.iter().all(|t| parse_char_code(t).is_some()) {
        let mut result = String::new();
        for t in tokens {
            if let Some(c) = parse_char_code(t).and_then(char::from_u32) {
                result.push(c);
            }
        }
        return result;
    }
    arg_part.to_string()
}

fn parse_char_code(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else if let Some(hex) = s.strip_prefix("U+").or_else(|| s.strip_prefix("u+")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u32>().ok()
    }
}

/// Format a `SYMBOL` field value (ECMA-376 Part 1 §17.16.5.58).
pub fn format_symbol(instruction: &str) -> String {
    let trimmed = instruction.trim();
    let rest = if let Some(stripped) = trimmed.strip_prefix("SYMBOL") {
        stripped.trim()
    } else {
        trimmed
    };
    let words: Vec<&str> = rest.split_whitespace().collect();
    let ch = words.first().and_then(|w| parse_char_code(w)).and_then(char::from_u32).unwrap_or('\u{FFFD}');
    apply_instruction_text_switches(ch.to_string(), instruction)
}

/// Parse a `SET` field (ECMA-376 Part 1 §17.16.5.55): returns (bookmark_name, value).
pub fn parse_set_field(instruction: &str) -> Option<(String, String)> {
    let trimmed = instruction.trim();
    let words = field_words(trimmed);
    if words.is_empty() || !words[0].eq_ignore_ascii_case("SET") {
        return None;
    }
    let name = words.get(1)?.clone();
    if name.is_empty() || name.starts_with('\\') {
        return None;
    }
    let mut val = String::new();
    let mut i = 2;
    while i < words.len() {
        if words[i].starts_with('\\') {
            break;
        }
        if !val.is_empty() {
            val.push(' ');
        }
        val.push_str(&words[i]);
        i += 1;
    }
    Some((name, val))
}

/// Parse an `ASK` field (ECMA-376 Part 1 §17.16.5.4): returns (bookmark_name, value).
/// Syntax: `ASK Bookmark "PromptText" [\d "DefaultResponse"] [\o]`
pub fn parse_ask_field(instruction: &str) -> Option<(String, String)> {
    let trimmed = instruction.trim();
    let words = field_words(trimmed);
    if words.is_empty() || !words[0].eq_ignore_ascii_case("ASK") {
        return None;
    }
    let bookmark = words.get(1)?.clone();
    if bookmark.is_empty() || bookmark.starts_with('\\') {
        return None;
    }
    let mut default_val = None;
    let mut prompt_val = None;
    let mut i = 2;
    while i < words.len() {
        if words[i].eq_ignore_ascii_case("\\d") {
            if let Some(d) = words.get(i + 1) {
                default_val = Some(d.clone());
                i += 1;
            }
        } else if !words[i].starts_with('\\') && prompt_val.is_none() {
            prompt_val = Some(words[i].clone());
        }
        i += 1;
    }
    let value = default_val.or(prompt_val).unwrap_or_default();
    Some((bookmark, value))
}

/// Format an `ASK` field (ECMA-376 Part 1 §17.16.5.4). The field itself displays nothing.
pub fn format_ask(_instruction: &str) -> String {
    String::new()
}

/// Format a `FILLIN` / `FILL-IN` field (ECMA-376 Part 1 §17.16.5.24).
/// Syntax: `FILLIN ["PromptText"] [\d "DefaultResponse"] [\o] [Switches]`
pub fn format_fillin(instruction: &str) -> String {
    let trimmed = instruction.trim();
    let words = field_words(trimmed);
    if words.is_empty() {
        return String::new();
    }
    let first = words[0].to_ascii_uppercase();
    if first != "FILLIN" && first != "FILL-IN" {
        return String::new();
    }
    let mut default_val = None;
    let mut prompt_val = None;
    let mut i = 1;
    while i < words.len() {
        if words[i].eq_ignore_ascii_case("\\d") {
            if let Some(d) = words.get(i + 1) {
                default_val = Some(d.clone());
                i += 1;
            }
        } else if !words[i].starts_with('\\') && prompt_val.is_none() {
            prompt_val = Some(words[i].clone());
        }
        i += 1;
    }
    let val = default_val.or(prompt_val).unwrap_or_default();
    apply_instruction_text_switches(val, instruction)
}

/// Parsed information from a `HYPERLINK` field (ECMA-376 Part 1 §17.16.5.25).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HyperlinkFieldInfo {
    /// URL, file path, or relative address.
    pub target: String,
    /// Document sub-address, such as a bookmark name (`\l`).
    pub sub_address: Option<String>,
    /// Tooltip / screen tip text (`\o`).
    pub screen_tip: Option<String>,
    /// Target frame or window (`\t`).
    pub target_frame: Option<String>,
}

/// Parse a `HYPERLINK` field instruction.
pub fn parse_hyperlink_field(instruction: &str) -> Option<HyperlinkFieldInfo> {
    let trimmed = instruction.trim();
    let words = field_words(trimmed);
    if words.first().map(|s| s.to_ascii_uppercase()).as_deref() != Some("HYPERLINK") {
        return None;
    }
    let mut info = HyperlinkFieldInfo::default();
    let mut iter = words.into_iter().skip(1);
    let mut target_set = false;

    while let Some(tok) = iter.next() {
        if tok.starts_with('\\') {
            let sw = tok.to_ascii_lowercase();
            match sw.as_str() {
                "\\l" => info.sub_address = iter.next(),
                "\\o" => info.screen_tip = iter.next(),
                "\\t" => info.target_frame = iter.next(),
                "\\m" | "\\n" => {}
                _ => {
                    if sw == "\\*" || sw == "\\#" || sw == "\\@" {
                        iter.next();
                    }
                }
            }
        } else if !target_set {
            info.target = tok;
            target_set = true;
        }
    }
    Some(info)
}

/// Format the displayed text for a `HYPERLINK` field.
/// If `current_shown` is non-empty, preserves it (with format switches).
/// Otherwise falls back to `target` or `sub_address`.
pub fn format_hyperlink_field(instruction: &str, current_shown: &str) -> String {
    let raw = if !current_shown.is_empty() {
        current_shown.to_string()
    } else if let Some(info) = parse_hyperlink_field(instruction) {
        if !info.target.is_empty() {
            info.target
        } else if let Some(sub) = info.sub_address {
            sub
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    apply_instruction_text_switches(raw, instruction)
}

/// Format a `TEMPLATE` field value (ECMA-376 Part 1 §17.16.5.60).
pub fn format_template_field(instruction: &str, template_name: Option<&str>) -> String {
    let name = template_name.unwrap_or("Normal.dotm");
    apply_instruction_text_switches(name.to_string(), instruction)
}

/// Parsed offset parameters from an `ADVANCE` field (ECMA-376 Part 1 §17.16.5.2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AdvanceFieldInfo {
    pub down: Option<f32>,
    pub up: Option<f32>,
    pub left: Option<f32>,
    pub right: Option<f32>,
    pub horiz_pos: Option<f32>,
    pub vert_pos: Option<f32>,
}

/// Parse an `ADVANCE` field instruction.
pub fn parse_advance_field(instruction: &str) -> AdvanceFieldInfo {
    let words = field_words(instruction);
    let mut info = AdvanceFieldInfo::default();
    let mut iter = words.into_iter();
    // 最初のトークンは ADVANCE
    iter.next();
    while let Some(tok) = iter.next() {
        let sw = tok.to_ascii_lowercase();
        match sw.as_str() {
            "\\d" => info.down = iter.next().and_then(|v| v.parse::<f32>().ok()),
            "\\u" => info.up = iter.next().and_then(|v| v.parse::<f32>().ok()),
            "\\l" => info.left = iter.next().and_then(|v| v.parse::<f32>().ok()),
            "\\r" => info.right = iter.next().and_then(|v| v.parse::<f32>().ok()),
            "\\x" => info.horiz_pos = iter.next().and_then(|v| v.parse::<f32>().ok()),
            "\\y" => info.vert_pos = iter.next().and_then(|v| v.parse::<f32>().ok()),
            _ => {
                if sw == "\\*" || sw == "\\#" || sw == "\\@" {
                    iter.next();
                }
            }
        }
    }
    info
}

/// Expand nested field expressions `{ ... }` from innermost to outermost.
pub fn expand_nested_fields<F>(code: &str, mut evaluate: F) -> String
where
    F: FnMut(&str) -> Option<String>,
{
    let mut s = code.to_string();
    while let Some(close) = s.find('}') {
        let Some(open) = s[..close].rfind('{') else { break };
        let inner = s[open + 1..close].trim();
        if let Some(val) = evaluate(inner) {
            s.replace_range(open..=close, &val);
        } else {
            break;
        }
    }
    s
}

/// Parse an `INFO` field instruction (ECMA-376 Part 1 §17.16.5.30).
/// Returns the property name if the instruction starts with `INFO`.
pub fn parse_info_field(instruction: &str) -> Option<String> {
    let words = field_words(instruction);
    if words.first().map(|s| s.to_ascii_uppercase()).as_deref() != Some("INFO") {
        return None;
    }
    words.get(1).cloned()
}

/// A numeric picture (`\# "000"`, `\# "#,##0.00"`): `0` a digit always
/// written, `#` a digit when there is one, `,` thousands, `.` decimals;
/// other characters (and text in `'`) as they are.
pub fn numeric_picture(value: f64, picture: &str) -> String {
    // The positive part of a picture `positive;negative`.
    let mut parts = picture.split(';');
    let positive = parts.next().unwrap_or("");
    let negative = parts.next();
    let (picture, value, sign) = match negative {
        Some(neg) if value < 0.0 => (neg, -value, ""),
        _ => (positive, value.abs(), if value < 0.0 { "-" } else { "" }),
    };
    let first = picture.find(['0', '#']).unwrap_or(picture.len());
    let last = picture.rfind(['0', '#', ',', '.']).map_or(first, |i| i + 1).max(first);
    let (prefix, number, suffix) = (&picture[..first], &picture[first..last], &picture[last..]);
    let (int_part, frac_part) = number.split_once('.').unwrap_or((number, ""));
    let decimals = frac_part.chars().filter(|c| matches!(c, '0' | '#')).count();
    let min_frac = frac_part.chars().filter(|&c| c == '0').count();
    let min_int = int_part.chars().filter(|&c| c == '0').count();
    let grouping = int_part.contains(',');
    let rounded = format!("{:.*}", decimals, value);
    let (int_digits, frac_digits) = rounded.split_once('.').unwrap_or((&rounded, ""));
    let mut int_digits = int_digits.trim_start_matches('0').to_string();
    while int_digits.len() < min_int {
        int_digits.insert(0, '0');
    }
    if grouping {
        let mut g = String::new();
        for (i, c) in int_digits.chars().enumerate() {
            if i > 0 && (int_digits.len() - i) % 3 == 0 {
                g.push(',');
            }
            g.push(c);
        }
        int_digits = g;
    }
    let mut frac = frac_digits.to_string();
    while frac.len() > min_frac && frac.ends_with('0') {
        frac.pop();
    }
    let unquote = |s: &str| s.replace('\'', "");
    let mut out = format!("{}{sign}{int_digits}", unquote(prefix));
    if !frac.is_empty() {
        out.push('.');
        out.push_str(&frac);
    }
    out.push_str(&unquote(suffix));
    out
}

/// A resolver that provides numerical values from a document's table cells
/// for formula evaluation (e.g. `SUM(ABOVE)`, `AVERAGE(LEFT)`, `A1:B3`).
pub trait TableCellResolver {
    /// Values from a direction relative to the current cell (`"ABOVE"`, `"BELOW"`, `"LEFT"`, `"RIGHT"`).
    fn resolve_direction(&self, dir: &str) -> Vec<f64>;
    /// Values from a cell reference or range (e.g. `"A1"`, `"B2"`, `"A1:C3"`).
    fn resolve_reference(&self, reference: &str) -> Vec<f64>;
}

/// The value of a formula field's expression (after `=`, its nested fields
/// already replaced by their values): numbers, `+ - * / ^`, parentheses,
/// functions (`SUM`, `AVERAGE`, `COUNT`, `MIN`, `MAX`, `PRODUCT`, `ABS`, `INT`, `ROUND`, `MOD`),
/// and table cell references/directions via `resolver`.
pub fn evaluate_with_resolver(expression: &str, resolver: Option<&dyn TableCellResolver>) -> Option<f64> {
    let tokens: Vec<char> = expression.chars().filter(|c| !c.is_whitespace()).collect();
    let mut at = 0;
    let v = sum_with_resolver(&tokens, &mut at, resolver)?;
    (at == tokens.len()).then_some(v)
}

/// The value of a formula field's expression (after `=`, its nested fields
/// already replaced by their values): numbers, `+ - * / ^`, parentheses.
pub fn evaluate(expression: &str) -> Option<f64> {
    evaluate_with_resolver(expression, None)
}

fn sum_with_resolver(t: &[char], at: &mut usize, res: Option<&dyn TableCellResolver>) -> Option<f64> {
    let mut v = product_with_resolver(t, at, res)?;
    while let Some(&op) = t.get(*at).filter(|c| matches!(c, '+' | '-')) {
        *at += 1;
        let r = product_with_resolver(t, at, res)?;
        v = if op == '+' { v + r } else { v - r };
    }
    Some(v)
}

fn product_with_resolver(t: &[char], at: &mut usize, res: Option<&dyn TableCellResolver>) -> Option<f64> {
    let mut v = power_with_resolver(t, at, res)?;
    while let Some(&op) = t.get(*at).filter(|c| matches!(c, '*' | '/')) {
        *at += 1;
        let r = power_with_resolver(t, at, res)?;
        v = if op == '*' { v * r } else { v / r };
    }
    Some(v)
}

fn power_with_resolver(t: &[char], at: &mut usize, res: Option<&dyn TableCellResolver>) -> Option<f64> {
    let base = unary_with_resolver(t, at, res)?;
    if t.get(*at) == Some(&'^') {
        *at += 1;
        return Some(base.powf(power_with_resolver(t, at, res)?));
    }
    Some(base)
}

fn unary_with_resolver(t: &[char], at: &mut usize, res: Option<&dyn TableCellResolver>) -> Option<f64> {
    let c = *t.get(*at)?;
    match c {
        '-' => {
            *at += 1;
            Some(-unary_with_resolver(t, at, res)?)
        }
        '+' => {
            *at += 1;
            unary_with_resolver(t, at, res)
        }
        '(' => {
            *at += 1;
            let v = sum_with_resolver(t, at, res)?;
            (t.get(*at) == Some(&')')).then(|| *at += 1)?;
            Some(v)
        }
        _ if c.is_ascii_alphabetic() => {
            let start = *at;
            while t.get(*at).is_some_and(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == ':') {
                *at += 1;
            }
            let token: String = t[start..*at].iter().collect();
            let upper = token.to_ascii_uppercase();

            if t.get(*at) == Some(&'(') {
                *at += 1; // skip '('
                let args = parse_arg_list(t, at, res)?;
                (t.get(*at) == Some(&')')).then(|| *at += 1)?;
                evaluate_function(&upper, args)
            } else if let Some(r) = res {
                if matches!(upper.as_str(), "ABOVE" | "BELOW" | "LEFT" | "RIGHT") {
                    let vals = r.resolve_direction(&upper);
                    Some(vals.iter().sum())
                } else {
                    let vals = r.resolve_reference(&upper);
                    Some(vals.first().copied().unwrap_or(0.0))
                }
            } else {
                None
            }
        }
        _ => {
            let start = *at;
            while t.get(*at).is_some_and(|ch| ch.is_ascii_digit() || *ch == '.' || *ch == ',') {
                *at += 1;
            }
            t[start..*at].iter().filter(|&&ch| ch != ',').collect::<String>().parse().ok()
        }
    }
}

fn parse_arg_list(t: &[char], at: &mut usize, res: Option<&dyn TableCellResolver>) -> Option<Vec<f64>> {
    let mut args = Vec::new();
    while t.get(*at).is_some() && t.get(*at) != Some(&')') {
        let arg_start = *at;
        let mut depth = 0usize;
        while let Some(&ch) = t.get(*at) {
            match ch {
                '(' => depth += 1,
                ')' if depth > 0 => depth -= 1,
                ')' => break,
                ',' | ';' if depth == 0 => break,
                _ => {}
            }
            *at += 1;
        }
        let arg_str: String = t[arg_start..*at].iter().collect();
        let trimmed = arg_str.trim();
        let upper = trimmed.to_ascii_uppercase();

        if !trimmed.is_empty() {
            if matches!(upper.as_str(), "ABOVE" | "BELOW" | "LEFT" | "RIGHT") {
                if let Some(r) = res {
                    args.extend(r.resolve_direction(&upper));
                }
            } else if upper.contains(':') && !upper.contains('+') && !upper.contains('-') && !upper.contains('*') && !upper.contains('/') {
                if let Some(r) = res {
                    args.extend(r.resolve_reference(&upper));
                }
            } else if is_pure_cell_ref(&upper) {
                if let Some(r) = res {
                    let vals = r.resolve_reference(&upper);
                    if vals.is_empty() {
                        args.push(0.0);
                    } else {
                        args.extend(vals);
                    }
                } else {
                    args.push(0.0);
                }
            } else if let Some(val) = evaluate_with_resolver(trimmed, res) {
                args.push(val);
            }
        }

        if t.get(*at) == Some(&',') || t.get(*at) == Some(&';') {
            *at += 1;
        }
    }
    Some(args)
}

fn is_pure_cell_ref(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else { return false };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    let mut has_digit = false;
    for c in chars {
        if c.is_ascii_digit() {
            has_digit = true;
        } else if has_digit || !c.is_ascii_alphabetic() {
            return false;
        }
    }
    has_digit
}

fn evaluate_function(func_name: &str, args: Vec<f64>) -> Option<f64> {
    match func_name {
        "SUM" => Some(args.iter().sum()),
        "AVERAGE" => {
            if args.is_empty() {
                Some(0.0)
            } else {
                Some(args.iter().sum::<f64>() / args.len() as f64)
            }
        }
        "COUNT" => Some(args.len() as f64),
        "MIN" => {
            if args.is_empty() {
                Some(0.0)
            } else {
                Some(args.iter().copied().fold(f64::INFINITY, f64::min))
            }
        }
        "MAX" => {
            if args.is_empty() {
                Some(0.0)
            } else {
                Some(args.iter().copied().fold(f64::NEG_INFINITY, f64::max))
            }
        }
        "PRODUCT" => {
            if args.is_empty() {
                Some(0.0)
            } else {
                Some(args.iter().product())
            }
        }
        "ABS" => Some(args.first().copied().unwrap_or(0.0).abs()),
        "INT" => Some(args.first().copied().unwrap_or(0.0).floor()),
        "ROUND" => {
            let val = args.first().copied().unwrap_or(0.0);
            let digits = args.get(1).copied().unwrap_or(0.0) as i32;
            let factor = 10f64.powi(digits);
            Some((val * factor).round() / factor)
        }
        "MOD" => {
            let a = args.first().copied().unwrap_or(0.0);
            let b = args.get(1).copied().unwrap_or(1.0);
            if b == 0.0 { None } else { Some(a % b) }
        }
        _ => None,
    }
}

/// What formula field `instruction` (`= 2 + 3 \# "0.0"`) shows, its nested
/// fields already replaced by their values, optionally resolving table cells via `resolver`.
pub fn formula_text_with_resolver(instruction: &str, resolver: Option<&dyn TableCellResolver>) -> Option<String> {
    let body = instruction.trim_start().strip_prefix('=')?;
    let expression = body.split('\\').next().unwrap_or("");
    let value = evaluate_with_resolver(expression, resolver)?;
    let switches = &body[expression.len()..];
    let picture = field_words(&format!("= {switches}")).windows(2).find(|w| w[0] == "\\#").map(|w| w[1].clone());
    Some(match picture {
        Some(p) => numeric_picture(value, &p),
        None if value.fract() == 0.0 => format!("{}", value as i64),
        None => format!("{}", (value * 100.0).round() / 100.0),
    })
}

/// What formula field `instruction` (`= 2 + 3 \# "0.0"`) shows, its nested
/// fields already replaced by their values.
pub fn formula_text(instruction: &str) -> Option<String> {
    formula_text_with_resolver(instruction, None)
}

/// The arguments of an `EQ \o` field's code (`eq \o\ac(a,b)`) and the
/// switch that follows `\o` (`\ac`, `\ad`, `\al`, `\ar`), each argument as its
/// raw text (any `\s\up n(` around the text still in it).
fn overstrike_arguments(instruction: &str) -> Option<(String, Vec<String>)> {
    let t = instruction.trim();
    let rest = t.strip_prefix("eq").or_else(|| t.strip_prefix("EQ")).or_else(|| t.strip_prefix("Eq"))?;
    let at = rest.find("\\o")?;
    let switch: String = rest[at + 2..].trim_start().strip_prefix('\\').map(|a| format!("\\{}", a.chars().take(2).collect::<String>())).unwrap_or_default();
    let open = at + rest[at..].find('(')?;
    // The arguments up to the matching parenthesis, split at the top level.
    let mut depth = 0;
    let mut parts = vec![String::new()];
    for c in rest[open + 1..].chars() {
        match c {
            '(' => {
                depth += 1;
                parts.last_mut()?.push(c);
            }
            ')' if depth == 0 => break,
            ')' => {
                depth -= 1;
                parts.last_mut()?.push(c);
            }
            ',' if depth == 0 => parts.push(String::new()),
            _ => parts.last_mut()?.push(c),
        }
    }
    Some((switch, parts))
}

/// The text of an argument: inside its innermost parentheses, without
/// switches.
fn overstrike_text(s: &str) -> String {
    let inner = match (s.rfind('('), s.find(')')) {
        (Some(a), Some(b)) if a < b => &s[a + 1..b],
        _ => s,
    };
    // Only ASCII blanks separate the words: an ideographic space is text (the
    // width a `\o\ad` spreads its characters over).
    inner.split(|c: char| c.is_ascii_whitespace()).filter(|w| !w.starts_with('\\')).collect::<Vec<_>>().join("")
}

/// 囲い文字's parts in an `EQ \o\ac` field's code (`eq \o\ac(○,字)`): the
/// enclosure and the character (any `\s\up n( )` and the like around them
/// taken off). The other overstrikes (`\o\ad`, `\o\al`) are
/// [`overstrike`]'s.
pub fn enclosed(instruction: &str) -> Option<(String, String)> {
    let (switch, parts) = overstrike_arguments(instruction)?;
    if switch != "\\ac" {
        return None;
    }
    match parts.as_slice() {
        [a, b, ..] => Some((overstrike_text(a), overstrike_text(b))),
        _ => None,
    }
}

/// How an `EQ \o` field lays its arguments over each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverstrikeAlign {
    /// `\ad`: the first argument spread evenly over the width of the second.
    Distribute,
    /// `\al`: every argument from the left.
    Left,
    /// `\ar`: every argument against the right.
    Right,
    /// `\o` with no switch (Word's 割注-like stack `eq \o(\s\up 5(上),\s\do 2(下))`): every argument in the middle of the widest one.
    Center,
}

/// An `EQ \o\ad`, `\o\al`, `\o\ar` or `\o` (no switch) field's arguments: the text of each and
/// how far it is raised (points; negative: lowered, by `\s\up n` and `\s\do n`).
pub fn overstrike(instruction: &str) -> Option<(OverstrikeAlign, Vec<(String, f32)>)> {
    let (switch, parts) = overstrike_arguments(instruction)?;
    let align = match switch.as_str() {
        "\\ad" => OverstrikeAlign::Distribute,
        "\\al" => OverstrikeAlign::Left,
        "\\ar" => OverstrikeAlign::Right,
        "" => OverstrikeAlign::Center,
        _ => return None,
    };
    let args = parts
        .iter()
        .map(|part| {
            let compact = part.replace(' ', "");
            let mut raise = 0.0f32;
            for (key, sign) in [("\\up", 1.0f32), ("\\do", -1.0f32)] {
                if let Some(i) = compact.find(key) {
                    let digits: String = compact[i + key.len()..].chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
                    if let Ok(n) = digits.parse::<f32>() {
                        raise = sign * n;
                    }
                }
            }
            (overstrike_text(part), raise)
        })
        .collect();
    Some((align, args))
}

/// Compare two values according to Word's field comparison rules (ECMA-376 Part 1 §17.16.5.28).
/// Returns true if the condition holds.
pub fn compare_values(exp1: &str, op: &str, exp2: &str) -> bool {
    let s1 = exp1.trim();
    let s2 = exp2.trim();
    if let (Ok(n1), Ok(n2)) = (s1.parse::<f64>(), s2.parse::<f64>()) {
        match op {
            "=" | "==" => (n1 - n2).abs() < f64::EPSILON,
            "<>" | "!=" => (n1 - n2).abs() >= f64::EPSILON,
            "<" => n1 < n2,
            "<=" => n1 <= n2,
            ">" => n1 > n2,
            ">=" => n1 >= n2,
            _ => false,
        }
    } else {
        match op {
            "=" | "==" => s1.eq_ignore_ascii_case(s2),
            "<>" | "!=" => !s1.eq_ignore_ascii_case(s2),
            "<" => s1.to_ascii_lowercase() < s2.to_ascii_lowercase(),
            "<=" => s1.to_ascii_lowercase() <= s2.to_ascii_lowercase(),
            ">" => s1.to_ascii_lowercase() > s2.to_ascii_lowercase(),
            ">=" => s1.to_ascii_lowercase() >= s2.to_ascii_lowercase(),
            _ => false,
        }
    }
}

/// Apply text case switches (`\* Upper`, `\* Lower`, `\* FirstCap` / `\* Caps`) to a string.
pub fn apply_text_switches(text: String, words: &[String]) -> String {
    let mut case = None;
    let mut i = 0;
    while i < words.len() {
        if words[i] == "\\*" {
            if let Some(w) = words.get(i + 1) {
                if matches!(w.to_ascii_lowercase().as_str(), "upper" | "lower" | "caps" | "firstcap") {
                    case = Some(w.to_ascii_lowercase());
                }
            }
            i += 1;
        }
        i += 1;
    }
    match case.as_deref() {
        Some("upper") => text.to_uppercase(),
        Some("lower") => text.to_lowercase(),
        Some("firstcap") => {
            let mut c = text.chars();
            c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        }
        Some("caps") => {
            let mut res = String::new();
            let mut at_word_start = true;
            for ch in text.chars() {
                if ch.is_alphabetic() {
                    if at_word_start {
                        res.extend(ch.to_uppercase());
                        at_word_start = false;
                    } else {
                        res.push(ch);
                    }
                } else {
                    res.push(ch);
                    at_word_start = true;
                }
            }
            res
        }
        _ => text,
    }
}

/// Applies text format switches (`\* Upper`, `\* Lower`, `\* Caps`, `\* FirstCap`)
/// parsed from an instruction string.
pub fn apply_instruction_text_switches(text: String, instruction: &str) -> String {
    let words: Vec<String> = field_words(instruction).into_iter().map(|w| w.to_string()).collect();
    apply_text_switches(text, &words)
}

/// What an `IF` field (`IF exp1 op exp2 true_text false_text`) evaluates to (ECMA-376 Part 1 §17.16.5.28).
pub fn if_field_text(instruction: &str) -> Option<String> {
    let words = field_words(instruction);
    if words.first().map(|s| s.to_ascii_uppercase()).as_deref() != Some("IF") {
        return None;
    }
    let mut args: Vec<String> = Vec::new();
    let mut i = 1;
    while i < words.len() {
        if words[i] == "\\*" || words[i] == "\\#" || words[i] == "\\@" {
            i += 2;
            continue;
        }
        args.push(words[i].clone());
        i += 1;
    }

    let raw_result = if args.len() >= 3 && matches!(args[1].as_str(), "=" | "==" | "<>" | "!=" | "<" | "<=" | ">" | ">=") {
        let is_true = compare_values(&args[0], &args[1], &args[2]);
        if is_true {
            args.get(3).cloned().unwrap_or_default()
        } else {
            args.get(4).cloned().unwrap_or_default()
        }
    } else if args.len() >= 2 {
        let cond = &args[0];
        let is_true = if let Ok(n) = cond.parse::<f64>() {
            n.abs() >= f64::EPSILON
        } else {
            !cond.is_empty() && !cond.eq_ignore_ascii_case("false") && cond != "0"
        };
        if is_true {
            args.get(1).cloned().unwrap_or_default()
        } else {
            args.get(2).cloned().unwrap_or_default()
        }
    } else {
        return None;
    };

    Some(apply_text_switches(raw_result, &words))
}

/// What a `COMPARE` field (`COMPARE exp1 op exp2`) evaluates to: "1" or "0" (ECMA-376 Part 1 §17.16.5.12).
pub fn compare_field_text(instruction: &str) -> Option<String> {
    let words = field_words(instruction);
    if words.first().map(|s| s.to_ascii_uppercase()).as_deref() != Some("COMPARE") {
        return None;
    }
    let mut args: Vec<String> = Vec::new();
    let mut i = 1;
    while i < words.len() {
        if words[i] == "\\*" || words[i] == "\\#" || words[i] == "\\@" {
            i += 2;
            continue;
        }
        args.push(words[i].clone());
        i += 1;
    }
    if args.len() >= 3 && matches!(args[1].as_str(), "=" | "==" | "<>" | "!=" | "<" | "<=" | ">" | ">=") {
        let is_true = compare_values(&args[0], &args[1], &args[2]);
        let raw = if is_true { "1".to_string() } else { "0".to_string() };
        Some(apply_text_switches(raw, &words))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn enclosed_characters() {
        assert_eq!(super::enclosed("eq \\o\\ac(○,字)"), Some(("○".into(), "字".into())));
        assert_eq!(super::enclosed(" EQ \\o\\ac(□,\\s\\up 1(秘)) "), Some(("□".into(), "秘".into())));
        assert_eq!(super::enclosed("PAGE"), None);
        // The other overstrikes are not enclosures.
        assert_eq!(super::enclosed("eq \\o\\ad(あいう,　　　)"), None);
        assert_eq!(super::enclosed("eq \\o\\al(\\s\\up 6(上),\\s\\up-6(下))"), None);
    }

    #[test]
    fn overstrike_arguments_of_eq() {
        use super::OverstrikeAlign::*;
        assert_eq!(super::overstrike("eq \\o\\ad(あいう,　　　)"), Some((Distribute, vec![("あいう".into(), 0.0), ("　　　".into(), 0.0)])));
        assert_eq!(super::overstrike("eq \\o \\al(\\s \\up 6(上),\\s \\up-6(下))"), Some((Left, vec![("上".into(), 6.0), ("下".into(), -6.0)])));
        assert_eq!(super::overstrike("eq \\o\\al(\\s \\up 6(上),\\s \\up-6(下))"), Some((Left, vec![("上".into(), 6.0), ("下".into(), -6.0)])));
        assert_eq!(super::overstrike("eq \\o\\ac(○,字)"), None);
        // No switch: centred (GitHub issue #180).
        assert_eq!(super::overstrike("eq \\o(\\s\\up 5(かき),\\s\\do 2(くけ))"), Some((Center, vec![("かき".into(), 5.0), ("くけ".into(), -2.0)])));
    }

    use super::*;

    #[test]
    fn switches_of_page_fields() {
        let f = |n, i| format_field_number(n, i);
        assert_eq!(f(3, "PAGE \\* ROMAN"), Some("III".into()));
        assert_eq!(f(4, "PAGE \\* roman"), Some("iv".into()));
        assert_eq!(f(5, "PAGE \\* ALPHABETIC"), Some("E".into()));
        assert_eq!(f(6, "PAGE \\* alphabetic"), Some("f".into()));
        assert_eq!(f(7, "PAGE \\* Arabic \\* MERGEFORMAT"), Some("7".into()));
        assert_eq!(f(8, "PAGE \\* ArabicDash"), Some("- 8 -".into()));
        assert_eq!(f(9, "PAGE \\* CIRCLENUM"), Some("⑨".into()));
        assert_eq!(f(10, "PAGE \\* DBCHAR"), Some("１０".into()));
        assert_eq!(f(1, "PAGE \\* KANJINUM1"), Some("一".into()));
        assert_eq!(f(2, "PAGE \\* Ordinal"), Some("2nd".into()));
        assert_eq!(f(3, "PAGE \\* CardText"), Some("three".into()));
        assert_eq!(f(4, "PAGE \\# \"000\""), Some("004".into()));
        assert_eq!(f(3, "PAGE \\* CardText \\* Upper"), Some("THREE".into()));
        assert_eq!(f(3, "PAGE \\* MERGEFORMAT"), None);
        assert_eq!(f(3, "PAGE"), None);
    }

    #[test]
    fn pictures_and_formulas() {
        assert_eq!(numeric_picture(1234.5, "#,##0.00"), "1,234.50");
        assert_eq!(numeric_picture(7.0, "00"), "07");
        assert_eq!(numeric_picture(-3.0, "0"), "-3");
        assert_eq!(numeric_picture(42.0, "¥#,##0"), "¥42");
        assert_eq!(evaluate("6 + 10"), Some(16.0));
        assert_eq!(evaluate("2*(3+4)^2/7"), Some(14.0));
        assert_eq!(formula_text("= 6 + 10"), Some("16".into()));
        assert_eq!(formula_text("= 1/4 \\# \"0.00\""), Some("0.25".into()));
        assert_eq!(formula_text("= PAGE + 1"), None);
    }

    #[test]
    fn if_and_compare_fields() {
        assert_eq!(if_field_text("IF 1 = 1 \"True\" \"False\""), Some("True".into()));
        assert_eq!(if_field_text("IF 2 > 10 \"Greater\" \"Lesser\""), Some("Lesser".into()));
        assert_eq!(if_field_text("IF 10 >= 10 \"OK\" \"NG\""), Some("OK".into()));
        assert_eq!(if_field_text("IF 5 <> 3 \"Diff\" \"Same\""), Some("Diff".into()));
        assert_eq!(if_field_text("IF \"Apple\" = \"apple\" \"Match\" \"Mismatch\""), Some("Match".into()));
        assert_eq!(if_field_text("IF \"Banana\" = \"Orange\" \"Yes\" \"No\""), Some("No".into()));
        assert_eq!(if_field_text("IF 1 = 1 \"hello\" \"world\" \\* Upper"), Some("HELLO".into()));
        assert_eq!(compare_field_text("COMPARE 10 > 5"), Some("1".into()));
        assert_eq!(compare_field_text("COMPARE 5 > 10"), Some("0".into()));
        assert_eq!(compare_field_text("COMPARE \"A\" = \"a\""), Some("1".into()));
    }

    struct MockResolver;
    impl TableCellResolver for MockResolver {
        fn resolve_direction(&self, dir: &str) -> Vec<f64> {
            match dir {
                "ABOVE" => vec![10.0, 20.0, 30.0],
                "LEFT" => vec![5.0, 15.0],
                _ => Vec::new(),
            }
        }
        fn resolve_reference(&self, reference: &str) -> Vec<f64> {
            match reference {
                "A1" => vec![100.0],
                "B1" => vec![200.0],
                "A1:B2" => vec![1.0, 2.0, 3.0, 4.0],
                _ => Vec::new(),
            }
        }
    }

    #[test]
    fn table_formula_functions_and_resolver() {
        let res = MockResolver;
        assert_eq!(evaluate_with_resolver("SUM(ABOVE)", Some(&res)), Some(60.0));
        assert_eq!(evaluate_with_resolver("AVERAGE(ABOVE)", Some(&res)), Some(20.0));
        assert_eq!(evaluate_with_resolver("COUNT(ABOVE)", Some(&res)), Some(3.0));
        assert_eq!(evaluate_with_resolver("MIN(ABOVE)", Some(&res)), Some(10.0));
        assert_eq!(evaluate_with_resolver("MAX(ABOVE)", Some(&res)), Some(30.0));
        assert_eq!(evaluate_with_resolver("PRODUCT(LEFT)", Some(&res)), Some(75.0));
        assert_eq!(evaluate_with_resolver("SUM(A1:B2)", Some(&res)), Some(10.0));
        assert_eq!(evaluate_with_resolver("A1 + B1", Some(&res)), Some(300.0));
        assert_eq!(evaluate_with_resolver("SUM(ABOVE) * 2 + 10", Some(&res)), Some(130.0));
        assert_eq!(evaluate_with_resolver("ABS(-42)", None), Some(42.0));
        assert_eq!(evaluate_with_resolver("INT(42.7)", None), Some(42.0));
        assert_eq!(evaluate_with_resolver("ROUND(42.345, 2)", None), Some(42.35));
        assert_eq!(evaluate_with_resolver("ROUND(42.345)", None), Some(42.0));
        assert_eq!(evaluate_with_resolver("MOD(10, 3)", None), Some(1.0));

        assert_eq!(formula_text_with_resolver("= SUM(ABOVE) \\# \"#,##0.00\"", Some(&res)), Some("60.00".into()));
    }

    #[test]
    fn test_text_format_switches() {
        let text = "hello world from ibis office".to_string();
        assert_eq!(apply_instruction_text_switches(text.clone(), r#"AUTHOR \* Upper"#), "HELLO WORLD FROM IBIS OFFICE");
        assert_eq!(apply_instruction_text_switches("HELLO WORLD".to_string(), r#"TITLE \* Lower"#), "hello world");
        assert_eq!(apply_instruction_text_switches(text.clone(), r#"DOCPROPERTY "Subject" \* FirstCap"#), "Hello world from ibis office");
        assert_eq!(apply_instruction_text_switches(text.clone(), r#"REF bookmark \* Caps"#), "Hello World From Ibis Office");
        assert_eq!(apply_instruction_text_switches("japanese 日本語 text".to_string(), r#"STYLEREF \* Caps"#), "Japanese 日本語 Text");
    }

    #[test]
    fn test_listnum_formatting() {
        assert_eq!(default_listnum_format(1, 1), "1)");
        assert_eq!(default_listnum_format(2, 2), "b)");
        assert_eq!(default_listnum_format(3, 3), "iii)");
        assert_eq!(default_listnum_format(1, 4), "(1)");
        assert_eq!(default_listnum_format(2, 5), "(b)");
        assert_eq!(default_listnum_format(3, 6), "(iii)");
        assert_eq!(default_listnum_format(1, 7), "1.");
        assert_eq!(default_listnum_format(2, 8), "b.");
        assert_eq!(default_listnum_format(3, 9), "iii.");

        // スイッチ付きフォーマット
        assert_eq!(format_listnum(5, 1, "LISTNUM \\* ARABIC"), "5");
        assert_eq!(format_listnum(4, 2, "LISTNUM \\* ROMAN"), "IV");
        assert_eq!(format_listnum(3, 1, "LISTNUM \\# \"000\""), "003");
        // スイッチ無し（既定）
        assert_eq!(format_listnum(5, 1, "LISTNUM"), "5)");
        assert_eq!(format_listnum(2, 2, "LISTNUM LegalDefault \\l 2"), "b)");
    }

    #[test]
    fn test_autonum_and_filesize_formatting() {
        // AUTONUM
        assert_eq!(format_autonum(1, "AUTONUM", "AUTONUM"), "1.");
        assert_eq!(format_autonum(5, "AUTONUM", "AUTONUM"), "5.");
        assert_eq!(format_autonum(4, "AUTONUM", "AUTONUM \\* ROMAN"), "IV");
        assert_eq!(format_autonum(3, "AUTONUM", "AUTONUM \\* alphabetic"), "c");

        // AUTONUMLGL
        assert_eq!(format_autonum(1, "AUTONUMLGL", "AUTONUMLGL"), "1.");
        assert_eq!(format_autonum(2, "AUTONUMLGL", "AUTONUMLGL \\* ArabicDash"), "- 2 -");

        // AUTONUMOUT
        assert_eq!(format_autonum(1, "AUTONUMOUT", "AUTONUMOUT"), "I.");
        assert_eq!(format_autonum(2, "AUTONUMOUT", "AUTONUMOUT"), "II.");
        assert_eq!(format_autonum(3, "AUTONUMOUT", "AUTONUMOUT"), "III.");
        assert_eq!(format_autonum(4, "AUTONUMOUT", "AUTONUMOUT \\* ALPHABETIC"), "D");

        // FILESIZE
        assert_eq!(format_filesize(2048, "FILESIZE"), "2048");
        assert_eq!(format_filesize(2048, "FILESIZE \\k"), "2");
        assert_eq!(format_filesize(2048, "FILESIZE \\k \\# \"#,##0 KB\""), "2 KB");
        assert_eq!(format_filesize(2097152, "FILESIZE \\m"), "2");
        assert_eq!(format_filesize(2097152, "FILESIZE \\m \\# \"0.0 MB\""), "2.0 MB");
    }

    #[test]
    fn test_doc_variable_and_user_formatting() {
        let mut vars = std::collections::BTreeMap::new();
        vars.insert("Version".to_string(), "2.0.1".to_string());
        vars.insert("ProjectTitle".to_string(), "ibis office docx".to_string());

        assert_eq!(format_doc_variable("DOCVARIABLE Version", &vars), "2.0.1");
        assert_eq!(format_doc_variable("DOCVARIABLE \"Version\"", &vars), "2.0.1");
        assert_eq!(format_doc_variable("DOCVARIABLE version", &vars), "2.0.1");
        assert_eq!(format_doc_variable("DOCVARIABLE ProjectTitle \\* Caps", &vars), "Ibis Office Docx");
        assert_eq!(format_doc_variable("DOCVARIABLE ProjectTitle \\* Upper", &vars), "IBIS OFFICE DOCX");
        assert_eq!(format_doc_variable("DOCVARIABLE UnknownVar", &vars), "");

        // USERNAME / USERINITIALS / USERADDRESS
        assert_eq!(format_user_name("Alice Smith", "USERNAME"), "Alice Smith");
        assert_eq!(format_user_name("Alice Smith", "USERNAME \\* Upper"), "ALICE SMITH");
        assert_eq!(format_user_initials("AS", "USERINITIALS"), "AS");
        assert_eq!(format_user_initials("as", "USERINITIALS \\* Upper"), "AS");
        assert_eq!(format_user_address("Tokyo, Japan", "USERADDRESS"), "Tokyo, Japan");
        assert_eq!(format_user_address("", "USERADDRESS"), "");
    }

    #[test]
    fn test_autotext_and_citation_formatting() {
        // AUTOTEXT
        assert_eq!(format_autotext("AUTOTEXT"), "");
        assert_eq!(format_autotext("AUTOTEXT CompanyName"), "CompanyName");
        assert_eq!(format_autotext("AUTOTEXT \"Company Header\""), "Company Header");
        assert_eq!(format_autotext("AUTOTEXT \"company header\" \\* Upper"), "COMPANY HEADER");
        assert_eq!(format_autotext("AUTOTEXT \"company header\" \\* Caps"), "Company Header");

        // AUTOTEXTLIST
        assert_eq!(format_autotextlist("AUTOTEXTLIST \"Sample Entry\""), "Sample Entry");
        assert_eq!(format_autotextlist("AUTOTEXTLIST \"Sample Entry\" \\s \"Normal\" \\t \"Insert text\""), "Sample Entry");
        assert_eq!(format_autotextlist("AUTOTEXTLIST \"sample entry\" \\* Upper"), "SAMPLE ENTRY");

        // CITATION
        assert_eq!(format_citation("CITATION Knuth1984"), "(Knuth1984)");
        assert_eq!(format_citation("CITATION \"Knuth1984\" \\l 1041"), "(Knuth1984)");
        assert_eq!(format_citation("CITATION knuth1984 \\* Upper"), "(KNUTH1984)");
        assert_eq!(format_citation("CITATION"), "");

        // BIBLIOGRAPHY
        assert_eq!(format_bibliography("BIBLIOGRAPHY"), "文献目録");
        assert_eq!(format_bibliography("BIBLIOGRAPHY \\l 1041"), "文献目録");
        assert_eq!(format_bibliography("BIBLIOGRAPHY \\l 1033"), "Bibliography");
        assert_eq!(format_bibliography("BIBLIOGRAPHY \\l 1033 \\* Upper"), "BIBLIOGRAPHY");
    }

    #[test]
    fn test_quote_symbol_and_set_formatting() {
        // QUOTE
        assert_eq!(format_quote("QUOTE \"Hello World\""), "Hello World");
        assert_eq!(format_quote("QUOTE Hello"), "Hello");
        assert_eq!(format_quote("QUOTE \"hello world\" \\* Upper"), "HELLO WORLD");
        assert_eq!(format_quote("QUOTE \"hello world\" \\* Caps"), "Hello World");
        // QUOTE with character codes
        assert_eq!(format_quote("QUOTE 65 66 67"), "ABC");
        assert_eq!(format_quote("QUOTE 0x41 0x20 0x42"), "A B");

        // SYMBOL
        assert_eq!(format_symbol("SYMBOL 65"), "A");
        assert_eq!(format_symbol("SYMBOL 0x41"), "A");
        assert_eq!(format_symbol("SYMBOL 0x00A9"), "©");
        assert_eq!(format_symbol("SYMBOL 97 \\* Upper"), "A");

        // SET
        assert_eq!(parse_set_field("SET VarName \"Sample Value\""), Some(("VarName".to_string(), "Sample Value".to_string())));
        assert_eq!(parse_set_field("SET VarName SampleText"), Some(("VarName".to_string(), "SampleText".to_string())));
        assert_eq!(parse_set_field("SET VarName \"Sample Value\" \\* Upper"), Some(("VarName".to_string(), "Sample Value".to_string())));
    }

    #[test]
    fn test_hyperlink_template_advance_formatting() {
        // HYPERLINK
        let info = parse_hyperlink_field(r#"HYPERLINK "https://www.example.com" \l "bookmark1" \o "Tip text" \t "_blank""#).unwrap();
        assert_eq!(info.target, "https://www.example.com");
        assert_eq!(info.sub_address.as_deref(), Some("bookmark1"));
        assert_eq!(info.screen_tip.as_deref(), Some("Tip text"));
        assert_eq!(info.target_frame.as_deref(), Some("_blank"));

        let info_anchor = parse_hyperlink_field(r#"HYPERLINK \l "MyBookmark""#).unwrap();
        assert_eq!(info_anchor.target, "");
        assert_eq!(info_anchor.sub_address.as_deref(), Some("MyBookmark"));

        // format_hyperlink_field
        assert_eq!(format_hyperlink_field("HYPERLINK \"https://example.com\"", ""), "https://example.com");
        assert_eq!(format_hyperlink_field("HYPERLINK \"https://example.com\" \\* Upper", ""), "HTTPS://EXAMPLE.COM");
        assert_eq!(format_hyperlink_field("HYPERLINK \"https://example.com\"", "Display Name"), "Display Name");
        assert_eq!(format_hyperlink_field("HYPERLINK \"https://example.com\" \\* Upper", "display text"), "DISPLAY TEXT");
        assert_eq!(format_hyperlink_field("HYPERLINK \\l \"Chapter1\"", ""), "Chapter1");

        // TEMPLATE
        assert_eq!(format_template_field("TEMPLATE", None), "Normal.dotm");
        assert_eq!(format_template_field("TEMPLATE \\* Upper", None), "NORMAL.DOTM");
        assert_eq!(format_template_field("TEMPLATE", Some("CustomTemplate.dotx")), "CustomTemplate.dotx");
        assert_eq!(format_template_field("TEMPLATE \\* Lower", Some("MyTemplate.dotx")), "mytemplate.dotx");

        // ADVANCE
        let adv = parse_advance_field("ADVANCE \\d 4.5 \\r 10 \\u 2");
        assert_eq!(adv.down, Some(4.5));
        assert_eq!(adv.right, Some(10.0));
        assert_eq!(adv.up, Some(2.0));
        assert_eq!(adv.left, None);
    }

    #[test]
    fn test_expand_nested_fields_and_info() {
        // expand_nested_fields
        let res = expand_nested_fields("= { PAGE } + 10", |inner| {
            if inner == "PAGE" {
                Some("5".to_string())
            } else {
                None
            }
        });
        assert_eq!(res, "= 5 + 10");

        // 多重入れ子
        let res2 = expand_nested_fields("IF { = { PAGE } + 1 } > 5 \"Yes\" \"No\"", |inner| {
            if inner == "PAGE" {
                Some("5".to_string())
            } else if inner == "= 5 + 1" {
                Some("6".to_string())
            } else {
                None
            }
        });
        assert_eq!(res2, "IF 6 > 5 \"Yes\" \"No\"");

        // INFO
        assert_eq!(parse_info_field("INFO Author"), Some("Author".to_string()));
        assert_eq!(parse_info_field("INFO \"Title\""), Some("Title".to_string()));
        assert_eq!(parse_info_field("INFO"), None);
    }

    #[test]
    fn test_ask_and_fillin_fields() {
        // ASK
        assert_eq!(parse_ask_field("ASK Name \"Enter name:\" \\d \"Alice\""), Some(("Name".to_string(), "Alice".to_string())));
        assert_eq!(parse_ask_field("ASK Title \"Document Title\""), Some(("Title".to_string(), "Document Title".to_string())));
        assert_eq!(parse_ask_field("ASK Var \\d \"DefaultOnly\""), Some(("Var".to_string(), "DefaultOnly".to_string())));
        assert_eq!(parse_ask_field("ASK"), None);
        assert_eq!(format_ask("ASK Name \"Prompt\""), "");

        // FILLIN
        assert_eq!(format_fillin("FILLIN \"Enter name:\" \\d \"Alice\" \\* Upper"), "ALICE");
        assert_eq!(format_fillin("FILLIN \\d \"hello world\" \\* Caps"), "Hello World");
        assert_eq!(format_fillin("FILLIN \"Prompt Text\""), "Prompt Text");
        assert_eq!(format_fillin("FILLIN"), "");
        assert_eq!(format_fillin("FILL-IN \\d \"test value\" \\* FirstCap"), "Test value");
    }
}


