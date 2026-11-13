// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The methods a format (docx, xlsx, pptx) writes for the editor are methods of extension traits
//! (`#[ibis_office_macros::editor_ext(Name)] impl SlideEditor { .. }`, doc/rule/implimentationRule.md), so that the
//! format's folder can become a crate of its own: a crate cannot write `impl SlideEditor` for a type of another crate,
//! but it can implement its own trait for it. While the formats are still folders of one crate the compiler accepts
//! the plain `impl` too, so this reads the sources and looks for what the compiler will refuse later, and for the
//! one thing it never refuses (5).
//!
//! 1. Every `impl SlideEditor` of a format's folder has the attribute.
//! 2. `<format>/ext.rs` is what `scripts/gen-editor-ext.py` writes for the blocks as they are.
//! 3. A format does not name the `ext` of another format, and the shell names none.
//! 4. No extension method has the name of a method written with a plain `impl SlideEditor` (the shell's, or those of
//!    a format that is not in `FORMATS` yet): a call would quietly go to the plain one.
//! 5. The traits are brought into scope through `ext` only (`use crate::pptx::ext::*;`), never by their own names.
//!
//! The sources are read the way `scripts/gen-editor-ext.py` reads them: comments, the insides of string literals and
//! character literals are blanked, brackets are counted. The two have to stay the same; (2) fails when they differ.

use std::path::Path;

/// The formats whose `impl SlideEditor` blocks are extension traits. A format enters when its folder gets an `ext.rs`.
const FORMATS: &[&str] = &["docx", "pptx", "xlsx"];
const ALL_FORMATS: [&str; 3] = ["docx", "pptx", "xlsx"];
const ATTRIBUTE: &str = "ibis_office_macros::editor_ext";

/// A file under `src/`: its path from there (with `/`) and its text.
struct Source {
    rel: String,
    text: String,
}

fn read_sources(dir: &Path, prefix: &str, out: &mut Vec<Source>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            read_sources(&path, &format!("{prefix}{name}/"), out);
        } else if name.ends_with(".rs") {
            out.push(Source { rel: format!("{prefix}{name}"), text: std::fs::read_to_string(&path).unwrap_or_default() });
        }
    }
}

fn sources() -> Vec<Source> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    read_sources(&src, "", &mut out);
    // The sources of the crates cut out of this one (the shell, the formats), under the folder they had in this crate (`shell/`,
    // `docx/`, `pptx/`, `xlsx/`; their tests `tests/shell/`, `tests/docx/`, `tests/pptx/`, `tests/xlsx/`), so the rules read as before (tasks/doc/crate-split-plan.md, 12, 14 and 15).
    for (name, crate_src) in [("shell", "../ibis-office-shell/src"), ("docx", "../ibis-office-docx/src"), ("pptx", "../ibis-office-pptx/src"), ("xlsx", "../ibis-office-xlsx/src")] {
        let mut theirs = Vec::new();
        read_sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join(crate_src), "", &mut theirs);
        for mut source in theirs {
            source.rel = match source.rel.strip_prefix("tests/") {
                Some(test) => format!("tests/{name}/{test}"),
                None => format!("{name}/{}", source.rel),
            };
            out.push(source);
        }
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    assert!(out.len() > 500, "found only {} files under {}", out.len(), src.display());
    out
}

/// The first folder of a path from `src/` (`pptx/keys.rs` -> `pptx`; a file of `src/` itself has none).
fn folder(rel: &str) -> &str {
    rel.split_once('/').map_or("", |(first, _)| first)
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn starts(src: &[char], at: usize, what: &str) -> bool {
    what.chars().enumerate().all(|(k, c)| src.get(at + k) == Some(&c))
}

/// `src` with comments, the insides of string literals and character literals turned into spaces (newlines stay):
/// what is left is code. The length does not change.
fn blank(src: &[char]) -> Vec<char> {
    let n = src.len();
    let mut out = src.to_vec();
    let spaces = |out: &mut Vec<char>, a: usize, b: usize| {
        for c in out.iter_mut().take(b.min(n)).skip(a) {
            if *c != '\n' {
                *c = ' ';
            }
        }
    };
    let mut i = 0;
    while i < n {
        let c = src[i];
        if starts(src, i, "//") {
            let j = (i..n).find(|&k| src[k] == '\n').unwrap_or(n);
            spaces(&mut out, i, j);
            i = j;
        } else if starts(src, i, "/*") {
            let (mut depth, mut j) = (1, i + 2);
            while j < n && depth > 0 {
                if starts(src, j, "/*") {
                    depth += 1;
                    j += 2;
                } else if starts(src, j, "*/") {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            spaces(&mut out, i, j);
            i = j;
        } else if is_word(c) {
            let mut j = i;
            while j < n && is_word(src[j]) {
                j += 1;
            }
            let word: String = src[i..j].iter().collect();
            if matches!(word.as_str(), "r" | "br" | "cr") {
                let mut k = j;
                while k < n && src[k] == '#' {
                    k += 1;
                }
                if k < n && src[k] == '"' {
                    let hashes = k - j;
                    let end = (k + 1..n).find(|&e| src[e] == '"' && (1..=hashes).all(|h| src.get(e + h) == Some(&'#'))).unwrap_or(n);
                    spaces(&mut out, k + 1, end);
                    i = end + 1 + hashes;
                    continue;
                }
            }
            i = j;
        } else if c == '"' {
            let mut j = i + 1;
            while j < n && src[j] != '"' {
                j += if src[j] == '\\' { 2 } else { 1 };
            }
            spaces(&mut out, i + 1, j);
            i = j + 1;
        } else if c == '\'' {
            if src.get(i + 1) == Some(&'\\') {
                let mut j = i + 3;
                while j < n && src[j] != '\'' {
                    j += 1;
                }
                spaces(&mut out, i, j + 1);
                i = j + 1;
            } else if src.get(i + 2) == Some(&'\'') {
                spaces(&mut out, i, i + 3);
                i += 3;
            } else {
                i += 1; // a lifetime or a label
            }
        } else {
            i += 1;
        }
    }
    out
}

fn opens(c: char) -> bool {
    matches!(c, '(' | '[' | '{')
}

fn closes(c: char) -> bool {
    matches!(c, ')' | ']' | '}')
}

/// The place of the bracket that closes the one at `i`.
fn closing(code: &[char], i: usize) -> usize {
    let mut depth = 0usize;
    for (j, &c) in code.iter().enumerate().skip(i) {
        if opens(c) {
            depth += 1;
        } else if closes(c) {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return j;
            }
        }
    }
    code.len().saturating_sub(1)
}

/// The ending of the trait's name for the visibility written on a method (the macro's rule).
fn suffix(vis: Option<&str>) -> &'static str {
    match vis {
        None | Some("self") => "Private",
        Some("") => "",
        Some("crate") => "Crate",
        Some("super") => "Super",
        Some(_) => "In",
    }
}

/// An `impl SlideEditor { .. }` of a source.
#[derive(Debug)]
struct Block {
    line: usize,
    /// Not inside a module, a function or another block of the file.
    top: bool,
    /// The name in the attribute.
    name: Option<String>,
    /// The `#[cfg(..)]` lines of the block, as written.
    cfgs: Vec<String>,
    /// The methods: name, the ending of the trait's name, line.
    methods: Vec<(String, &'static str, usize)>,
}

fn line_of(src: &[char], at: usize) -> usize {
    src[..at].iter().filter(|&&c| c == '\n').count() + 1
}

/// Every `impl SlideEditor { .. }` of a source (an inherent impl: `impl Trait for SlideEditor` is not one).
fn blocks_of(text: &str) -> Vec<Block> {
    if !text.contains("SlideEditor") {
        return Vec::new();
    }
    let src: Vec<char> = text.chars().collect();
    let code = blank(&src);
    let n = code.len();
    let mut found = Vec::new();
    let mut depth_at: Option<Vec<i32>> = None;
    let mut at = 0;
    while at + 4 < n {
        // `impl`, a path, `{`
        let here = at;
        at += 1;
        if !starts(&code, here, "impl") || (here > 0 && is_word(code[here - 1])) || !code[here + 4].is_whitespace() {
            continue;
        }
        let mut p = here + 4;
        while p < n && code[p].is_whitespace() {
            p += 1;
        }
        let path_start = p;
        while p < n && (is_word(code[p]) || code[p] == ':') {
            p += 1;
        }
        let path: String = code[path_start..p].iter().collect();
        while p < n && code[p].is_whitespace() {
            p += 1;
        }
        if p >= n || code[p] != '{' || !(path == "SlideEditor" || path.ends_with("::SlideEditor")) {
            continue;
        }
        let open_at = p;
        let depths = depth_at.get_or_insert_with(|| {
            let mut depth = 0;
            code.iter()
                .map(|&c| {
                    let before = depth;
                    if opens(c) {
                        depth += 1;
                    } else if closes(c) {
                        depth -= 1;
                    }
                    before
                })
                .collect()
        });
        let mut block = Block { line: line_of(&src, here), top: depths[here] == 0, name: None, cfgs: Vec::new(), methods: Vec::new() };
        // the attributes above the block
        let mut k = here;
        loop {
            let Some(j) = (0..k).rev().find(|&j| !code[j].is_whitespace()) else { break };
            if code[j] != ']' {
                break;
            }
            let mut depth = 0;
            let mut open = None;
            for o in (0..=j).rev() {
                if closes(code[o]) {
                    depth += 1;
                } else if opens(code[o]) {
                    depth -= 1;
                    if depth == 0 {
                        open = Some(o);
                        break;
                    }
                }
            }
            let Some(o) = open.filter(|&o| o >= 1 && code[o - 1] == '#') else { break };
            let written: String = src[o + 1..j].iter().collect();
            let squeezed: String = written.split_whitespace().collect();
            let named = squeezed.strip_prefix(ATTRIBUTE).and_then(|s| s.strip_prefix('(')).and_then(|s| s.strip_suffix(')'));
            if let Some(name) = named.filter(|s| !s.is_empty() && s.chars().all(is_word) && !s.starts_with(|c: char| c.is_ascii_digit())) {
                block.name = Some(name.to_string());
            } else if squeezed.starts_with("cfg(") {
                block.cfgs.insert(0, format!("#[{}]", written.trim()));
            }
            k = o - 1;
        }
        // the methods: `fn name` at the block's own depth, with the visibility written before it
        let close_at = closing(&code, open_at);
        let mut i = open_at + 1;
        let mut vis: Option<String> = None;
        while i < close_at {
            let c = code[i];
            if opens(c) {
                i = closing(&code, i) + 1;
            } else if is_word(c) {
                let mut j = i;
                while j < close_at && is_word(code[j]) {
                    j += 1;
                }
                let word: String = code[i..j].iter().collect();
                let mut k = j;
                while k < close_at && code[k].is_whitespace() {
                    k += 1;
                }
                if word == "pub" {
                    vis = Some(String::new());
                    if code[k] == '(' {
                        let e = closing(&code, k);
                        let inside: String = code[k + 1..e].iter().collect();
                        let mut parts = inside.split_whitespace();
                        vis = Some(match (parts.next(), parts.next()) {
                            (Some(only), None) => only.to_string(),
                            _ => "in".to_string(),
                        });
                        j = e + 1;
                    }
                } else if word == "fn" && k > j && is_word(code[k]) && !code[k].is_ascii_digit() {
                    let mut e = k;
                    while e < close_at && is_word(code[e]) {
                        e += 1;
                    }
                    block.methods.push((code[k..e].iter().collect(), suffix(vis.as_deref()), line_of(&src, k)));
                    vis = None;
                    j = e;
                }
                i = j;
            } else {
                i += 1;
            }
        }
        found.push(block);
        at = open_at + 1;
    }
    found
}

/// The files of a format's folder that have blocks, in the order of their names, without `ext.rs`.
fn blocks_of_format<'a>(sources: &'a [Source], format: &str) -> Vec<(&'a str, Vec<Block>)> {
    let prefix = format!("{format}/");
    let mut out = Vec::new();
    for s in sources {
        let Some(rel) = s.rel.strip_prefix(&prefix) else { continue };
        if rel == "ext.rs" {
            continue;
        }
        let blocks = blocks_of(&s.text);
        if !blocks.is_empty() {
            out.push((rel, blocks));
        }
    }
    out
}

/// The module of a file of the format's folder, as the path from the folder's module: `a/b.rs` -> `["a", "b"]`.
fn module_of(rel: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = rel.trim_end_matches(".rs").split('/').collect();
    if matches!(parts.last(), Some(&"mod") | Some(&"lib")) {
        parts.pop();
    }
    parts
}

/// The traits other files can see, per block: (the `cfg` lines, the visibility of the `use`, the module's path from
/// the folder, the trait's name).
fn listed_traits(per_file: &[(&str, Vec<Block>)]) -> Vec<(Vec<String>, &'static str, String, String)> {
    let mut out = Vec::new();
    for (rel, blocks) in per_file {
        let module = module_of(rel);
        let path: String = module.iter().map(|p| format!("::{p}")).collect();
        for b in blocks {
            let Some(name) = b.name.as_ref().filter(|_| b.top) else { continue };
            for (ending, vis) in [("", "pub"), ("Crate", "pub(crate)"), ("Super", "pub(super)")] {
                if !b.methods.iter().any(|m| m.1 == ending) || (ending == "Super" && module.len() != 1) {
                    continue;
                }
                out.push((b.cfgs.clone(), vis, path.clone(), format!("{name}{ending}")));
            }
        }
    }
    out
}

/// The lines of `ext.rs` that are not comments, as the script writes them.
fn ext_lines(per_file: &[(&str, Vec<Block>)]) -> Vec<String> {
    let mut lines = vec!["#![allow(unused_imports)]".to_string()];
    for (cfgs, vis, path, name) in listed_traits(per_file) {
        lines.extend(cfgs);
        lines.push(format!("{vis} use super{path}::{name} as _;"));
    }
    lines
}

/// (1) The blocks of the formats' folders that have no attribute.
fn blocks_without_the_attribute(sources: &[Source], formats: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    for format in formats {
        for (rel, blocks) in blocks_of_format(sources, format) {
            found.extend(blocks.iter().filter(|b| b.name.is_none()).map(|b| format!("src/{format}/{rel}:{}", b.line)));
        }
    }
    found
}

/// The words of the code of a source with their places (the index of the first character).
fn words(code: &[char]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < code.len() {
        if is_word(code[i]) {
            let start = i;
            while i < code.len() && is_word(code[i]) {
                i += 1;
            }
            out.push((start, code[start..i].iter().collect()));
        } else {
            i += 1;
        }
    }
    out
}

/// (3) The places where a format's folder names the `ext` of another format, or the shell names one.
fn ext_of_another_format(sources: &[Source]) -> Vec<String> {
    let mut found = Vec::new();
    for s in sources {
        let from = folder(&s.rel);
        if from != "shell" && !ALL_FORMATS.contains(&from) {
            continue;
        }
        if !ALL_FORMATS.iter().any(|f| s.text.contains(f)) || !s.text.contains("ext") {
            continue;
        }
        let src: Vec<char> = s.text.chars().collect();
        let code = blank(&src);
        let all = words(&code);
        for pair in all.windows(2) {
            let ((at, format), (next_at, next)) = (&pair[0], &pair[1]);
            let between: String = code[at + format.len()..*next_at].iter().filter(|c| !c.is_whitespace()).collect();
            if ALL_FORMATS.contains(&format.as_str()) && format != from && between == "::" && next == "ext" {
                found.push(format!("src/{}:{}: {format}::ext", s.rel, line_of(&src, *at)));
            }
        }
    }
    found
}

/// (4) The extension methods of `formats` that have the name of a method of a plain `impl SlideEditor` elsewhere.
fn methods_named_like_a_plain_one(sources: &[Source], formats: &[&str]) -> Vec<String> {
    let mut plain = std::collections::BTreeMap::new();
    for s in sources {
        if formats.contains(&folder(&s.rel)) {
            continue;
        }
        for b in blocks_of(&s.text) {
            for (name, _, line) in b.methods {
                plain.entry(name).or_insert_with(|| format!("src/{}:{line}", s.rel));
            }
        }
    }
    let mut found = Vec::new();
    for format in formats {
        for (rel, blocks) in blocks_of_format(sources, format) {
            for (name, _, line) in blocks.iter().flat_map(|b| &b.methods) {
                if let Some(other) = plain.get(name) {
                    found.push(format!("src/{format}/{rel}:{line}: {name} (the plain one: {other})"));
                }
            }
        }
    }
    found
}

/// (5) The `use` lines that name a trait `ext.rs` lists, outside `ext.rs`.
fn traits_used_by_name(sources: &[Source], formats: &[&str]) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for format in formats {
        names.extend(listed_traits(&blocks_of_format(sources, format)).into_iter().map(|t| t.3));
    }
    let mut found = Vec::new();
    for s in sources {
        if formats.iter().any(|f| s.rel == format!("{f}/ext.rs")) || !names.iter().any(|name| s.text.contains(name.as_str())) {
            continue;
        }
        let src: Vec<char> = s.text.chars().collect();
        let code = blank(&src);
        let mut in_use = false;
        let mut i = 0;
        for (at, word) in words(&code) {
            // a `use` runs to its `;`
            if in_use && code[i..at].contains(&';') {
                in_use = false;
            }
            i = at;
            if word == "use" {
                // `-> impl Trait + use<'a>` is not one
                in_use = code[at + 3..].iter().find(|c| !c.is_whitespace()) != Some(&'<');
            } else if in_use && names.contains(&word) {
                found.push(format!("src/{}:{}: {word}", s.rel, line_of(&src, at)));
            }
        }
    }
    found
}

#[test]
fn the_formats_listed_are_those_with_an_ext_rs() {
    let sources = sources();
    for format in ALL_FORMATS {
        let has = sources.iter().any(|s| s.rel == format!("{format}/ext.rs"));
        assert_eq!(has, FORMATS.contains(&format), "src/{format}/ext.rs and FORMATS of tests/editor_ext_rule.rs do not say the same about {format}");
    }
}

#[test]
fn every_block_of_a_format_has_the_attribute() {
    let found = blocks_without_the_attribute(&sources(), FORMATS);
    assert!(found.is_empty(), "an `impl SlideEditor` of a format without #[{ATTRIBUTE}(..)]. Run: python3 scripts/gen-editor-ext.py\n{}", found.join("\n"));
}

#[test]
fn ext_rs_is_what_the_script_writes() {
    let sources = sources();
    for format in FORMATS {
        let rel = format!("{format}/ext.rs");
        let text = &sources.iter().find(|s| s.rel == rel).unwrap_or_else(|| panic!("no src/{rel}")).text;
        let written: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("//")).collect();
        let expected = ext_lines(&blocks_of_format(&sources, format));
        assert!(written == expected, "src/{rel} is not what the blocks give. Run: python3 scripts/gen-editor-ext.py\nexpected:\n{}", expected.join("\n"));
    }
}

#[test]
fn no_ext_of_another_format() {
    let found = ext_of_another_format(&sources());
    assert!(found.is_empty(), "a format calls its own extension methods and the shell's methods only; the shell reaches a format through the hooks (FormatEditor):\n{}", found.join("\n"));
}

#[test]
fn no_extension_method_has_the_name_of_a_plain_one() {
    let found = methods_named_like_a_plain_one(&sources(), FORMATS);
    assert!(found.is_empty(), "a call of these would go to the plain `impl SlideEditor` method without a word from the compiler. Take another name:\n{}", found.join("\n"));
}

#[test]
fn the_traits_come_through_ext() {
    let found = traits_used_by_name(&sources(), FORMATS);
    assert!(found.is_empty(), "write `use crate::<format>::ext::*;` instead of the trait's own path:\n{}", found.join("\n"));
}

mod the_search {
    use super::{Source, blocks_of, blocks_of_format, blocks_without_the_attribute, ext_lines, ext_of_another_format, methods_named_like_a_plain_one, traits_used_by_name};

    const KEYS: &str = r####"// impl SlideEditor { fn in_a_comment() {} }
const S: &str = "impl SlideEditor { fn in_a_string() {} } \" {";
const R: &str = r#"impl SlideEditor { fn "raw"() {} } {"#;
impl Render for SlideEditor { fn render(&mut self) {} }
#[cfg(feature = "export")]
/// The keys.
#[ibis_office_macros::editor_ext(Keys)]
impl SlideEditor {
 pub(crate) fn one<'a>(&'a self, c: char) -> bool { c == '{' || c == '\'' || c == '"' }
    fn two(&self, f: fn(i32) -> i32) { fn inner() {} let _ = "}"; }
    #[allow(dead_code)]
    pub fn three(&self) -> impl Fn(u8) -> u8 { |x| x }
    pub(super) fn four() {}
    pub(in crate::pptx) fn five() {}
    pub(self) fn six() {}
    /* fn commented() {} /* nested */ fn still_commented() {} */
}
mod tests {
    impl crate::shell::editor::SlideEditor { fn seven(&self) {} }
}
"####;

    fn source(rel: &str, text: &str) -> Source {
        Source { rel: rel.to_string(), text: text.to_string() }
    }

    #[test]
    fn reads_the_blocks() {
        let blocks = blocks_of(KEYS);
        assert_eq!(blocks.len(), 2, "{blocks:?}");
        assert_eq!(blocks[0].name.as_deref(), Some("Keys"));
        assert_eq!(blocks[0].cfgs, ["#[cfg(feature = \"export\")]"]);
        assert!(blocks[0].top);
        assert_eq!(blocks[0].line, 8);
        let methods: Vec<(&str, &str, usize)> = blocks[0].methods.iter().map(|m| (m.0.as_str(), m.1, m.2)).collect();
        assert_eq!(methods, [("one", "Crate", 9), ("two", "Private", 10), ("three", "", 12), ("four", "Super", 13), ("five", "In", 14), ("six", "Private", 15)]);
        assert_eq!(blocks[1].name, None);
        assert!(!blocks[1].top);
        assert_eq!(blocks[1].methods.len(), 1);
    }

    #[test]
    fn writes_the_lines_of_ext_rs() {
        let sources = [
            source("pptx/keys.rs", KEYS),
            source("pptx/mod.rs", "#[ibis_office_macros::editor_ext(Pptx)]\nimpl crate::shell::editor::SlideEditor {\n    pub(crate) fn a(&self) {}\n}\n"),
            source("pptx/panel/inner.rs", "#[ibis_office_macros::editor_ext(Inner)]\nimpl SlideEditor { pub(super) fn b(&self) {} pub(crate) fn c(&self) {} }\n#[ibis_office_macros::editor_ext(Inner2)]\nimpl SlideEditor { fn d(&self) {} }\n"),
            source("pptx/ext.rs", "pub(crate) use super::keys::KeysCrate as _;\n"),
            source("docx/keys.rs", "impl SlideEditor { pub(crate) fn e(&self) {} }\n"),
        ];
        let lines = ext_lines(&blocks_of_format(&sources, "pptx"));
        assert_eq!(
            lines,
            [
                "#![allow(unused_imports)]",
                "#[cfg(feature = \"export\")]",
                "pub use super::keys::Keys as _;",
                "#[cfg(feature = \"export\")]",
                "pub(crate) use super::keys::KeysCrate as _;",
                "#[cfg(feature = \"export\")]",
                "pub(super) use super::keys::KeysSuper as _;",
                "pub(crate) use super::PptxCrate as _;",
                "pub(crate) use super::panel::inner::InnerCrate as _;",
            ]
        );
        // the block inside `mod tests` of keys.rs has no attribute; docx is not asked about
        assert_eq!(blocks_without_the_attribute(&sources, &["pptx"]), ["src/pptx/keys.rs:19"]);
        assert_eq!(blocks_without_the_attribute(&sources, &["docx"]), ["src/docx/keys.rs:1"]);
    }

    #[test]
    fn finds_the_ext_of_another_format() {
        let sources = [
            source("docx/a.rs", "use crate::docx::ext::*;\nuse crate::pptx::ext::*;\n// crate::xlsx::ext in a comment\n"),
            source("shell/b.rs", "fn f() { use crate::xlsx :: ext::*; let _ = \"crate::docx::ext\"; }\n"),
            source("tests/shell/c.rs", "use crate::docx::ext::*;\nuse crate::xlsx::ext::*;\n"),
            source("ui_snapshot.rs", "use crate::pptx::ext::*;\n"),
            source("xlsx/d.rs", "use super::ext::*;\nuse crate::docx::extent;\n"),
        ];
        assert_eq!(ext_of_another_format(&sources), ["src/docx/a.rs:2: pptx::ext", "src/shell/b.rs:1: xlsx::ext"]);
    }

    #[test]
    fn finds_the_method_named_like_a_plain_one() {
        let sources = [
            source("pptx/keys.rs", "#[ibis_office_macros::editor_ext(Keys)]\nimpl SlideEditor {\n    pub(crate) fn key_down(&self) {}\n    fn own(&self) {}\n    pub(crate) fn layout(&self) {}\n}\n"),
            source("shell/keys.rs", "impl SlideEditor {\n    pub(crate) fn key_down(&self) {}\n}\n"),
            source("docx/layout.rs", "impl SlideEditor {\n    fn layout(&self) {}\n}\n"),
        ];
        assert_eq!(
            methods_named_like_a_plain_one(&sources, &["pptx"]),
            ["src/pptx/keys.rs:3: key_down (the plain one: src/shell/keys.rs:2)", "src/pptx/keys.rs:5: layout (the plain one: src/docx/layout.rs:2)"]
        );
        // once docx is a format of extension traits too, the two `layout` are methods of two traits
        assert_eq!(methods_named_like_a_plain_one(&sources, &["docx", "pptx"]), ["src/pptx/keys.rs:3: key_down (the plain one: src/shell/keys.rs:2)"]);
    }

    #[test]
    fn finds_the_trait_used_by_name() {
        let sources = [
            source("pptx/keys.rs", "#[ibis_office_macros::editor_ext(Keys)]\nimpl SlideEditor {\n    pub(crate) fn key_down(&self) {}\n    fn own(&self) {}\n}\nmod tests {\n    use super::KeysPrivate as _;\n}\n"),
            source("pptx/ext.rs", "pub(crate) use super::keys::KeysCrate as _;\n"),
            source("pptx/other.rs", "use crate::pptx::ext::*;\nuse crate::pptx::keys::{\n    KeysCrate as _,\n};\nfn f() -> impl Sized + use<> { let KeysCrate = 1; KeysCrate }\n"),
            source("tests/pptx/t.rs", "// use crate::pptx::keys::KeysCrate;\nuse crate::pptx::ext::*;\n"),
        ];
        assert_eq!(traits_used_by_name(&sources, &["pptx"]), ["src/pptx/other.rs:3: KeysCrate"]);
    }
}
