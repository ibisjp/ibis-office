// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pure-Rust mechanical converter from basic PlantUML sequence diagrams to Mermaid `sequenceDiagram`.
//!
//! Zero external dependencies: converts participants, messages, notes, and basic combined
//! fragments (`alt`/`else`/`opt`/`loop`/`par`/`break`/`critical`) so they can be rendered
//! with the existing Mermaid sequence renderer, while preserving the original PlantUML source
//! verbatim and reporting unsupported constructs (macros, timing diagrams, styling, dividers,
//! etc.) with 1-based line numbers.

use std::collections::HashMap;

/// One unsupported or lossy PlantUML construct encountered during conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlantUmlUnsupported {
    /// 1-based line number in the original PlantUML source.
    pub line: usize,
    /// Machine-readable category (`"macro"`, `"timing"`, `"diagram-type"`, `"styling"`,
    /// `"layout"`, `"divider"`, `"delay"`, `"reference"`, `"fragment"`, `"autonumber"`, `"syntax"`).
    pub kind: &'static str,
    /// Original trimmed line (or short excerpt).
    pub source: String,
    /// Human-readable explanation of why the construct was not faithfully converted.
    pub reason: String,
}

/// Result of converting a PlantUML basic sequence diagram into Mermaid `sequenceDiagram`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlantUmlConversion {
    /// Original PlantUML source preserved verbatim.
    pub original: String,
    /// Generated Mermaid `sequenceDiagram` source, or `None` if the input is not a
    /// convertible sequence diagram (e.g. timing diagram, other UML diagram type, or empty).
    pub mermaid: Option<String>,
    /// Unsupported or lossy constructs found in the original input, with 1-based line numbers.
    pub unsupported: Vec<PlantUmlUnsupported>,
}

impl PlantUmlConversion {
    /// True when conversion produced a Mermaid `sequenceDiagram` with no unsupported constructs.
    pub fn is_lossless(&self) -> bool {
        self.mermaid.is_some() && self.unsupported.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Alt,
    Opt,
    Loop,
    Par,
    Break,
    Critical,
    Group,
    Box,
}

#[derive(Debug, Clone)]
struct PendingNote {
    line: usize,
    header: String,
    lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkipUntil {
    MacroProc,
    StyleTag,
    BraceBlock,
    RefBlock,
    TitleBlock,
    EndKeyword(&'static str),
}

#[derive(Debug, Clone, Copy)]
struct ArrowMatch {
    start: usize,
    end: usize,
    mermaid: &'static str,
    reverse: bool,
    circle_source: bool,
    circle_target: bool,
    styled_lossy: bool,
}

struct OrderedParticipant {
    order: i32,
    had_order: bool,
    seq: usize,
    line_no: usize,
    raw_line: String,
    emitted: String,
}

struct ParticipantRegistry {
    /// Maps a PlantUML participant token (alias or unquoted label) to its Mermaid ID.
    ids: HashMap<String, String>,
    /// Set of Mermaid IDs already declared or allocated.
    used_ids: HashMap<String, usize>,
    next_synth: usize,
}

impl ParticipantRegistry {
    fn new() -> Self {
        Self {
            ids: HashMap::new(),
            used_ids: HashMap::new(),
            next_synth: 1,
        }
    }

    fn is_safe_mermaid_id(name: &str) -> bool {
        !name.is_empty()
            && !name.starts_with(['+', '.'])
            && !name.ends_with('+')
            && !name.contains("()")
            && !name.contains("@{")
            && name.chars().all(|c| {
                !c.is_whitespace()
                    && !matches!(
                        c,
                        '-' | '+' | ':' | ';' | ',' | '"' | '\'' | '<' | '>' | '{' | '}' | '['
                            | ']' | '#'
                    )
            })
    }

    fn allocate_id(&mut self, preferred: &str) -> String {
        if Self::is_safe_mermaid_id(preferred) && !self.used_ids.contains_key(preferred) {
            self.used_ids.insert(preferred.to_string(), 1);
            return preferred.to_string();
        }
        let sanitized: String = preferred
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let base = sanitized.trim_matches('_');
        if !base.is_empty() && Self::is_safe_mermaid_id(base) {
            let count = self.used_ids.entry(base.to_string()).or_insert(0);
            *count += 1;
            if *count == 1 {
                return base.to_string();
            }
            let candidate = format!("{base}_{count}");
            self.used_ids.insert(candidate.clone(), 1);
            return candidate;
        }
        loop {
            let candidate = format!("P{}", self.next_synth);
            self.next_synth += 1;
            if !self.used_ids.contains_key(&candidate) {
                self.used_ids.insert(candidate.clone(), 1);
                return candidate;
            }
        }
    }

    fn register_declared(&mut self, alias: &str, label: Option<&str>) -> String {
        if let Some(existing) = self.ids.get(alias).cloned() {
            if let Some(lbl) = label {
                self.ids.entry(lbl.to_string()).or_insert_with(|| existing.clone());
            }
            return existing;
        }
        let id = self.allocate_id(alias);
        self.ids.insert(alias.to_string(), id.clone());
        if let Some(lbl) = label {
            self.ids.entry(lbl.to_string()).or_insert_with(|| id.clone());
        }
        id
    }

    /// Resolve a participant token used in a message/note/activate statement.
    /// Returns `(mermaid_id, optional_synthetic_declaration)` when a quoted/unsafe name
    /// needs an explicit `participant <id> as <label>` declaration before first use.
    fn resolve(&mut self, raw: &str) -> (String, Option<String>) {
        let trimmed = raw.trim();
        let unquoted = strip_quotes(trimmed);
        if let Some(id) = self.ids.get(unquoted).cloned() {
            return (id, None);
        }
        if Self::is_safe_mermaid_id(unquoted) {
            let id = self.allocate_id(unquoted);
            self.ids.insert(unquoted.to_string(), id.clone());
            return (id, None);
        }
        let id = self.allocate_id(unquoted);
        self.ids.insert(unquoted.to_string(), id.clone());
        let label = sanitize_label(unquoted);
        let decl = format!("  participant {id} as {label}");
        (id, Some(decl))
    }
}

/// Convert a PlantUML basic sequence diagram into Mermaid `sequenceDiagram`.
pub fn convert_plantuml_sequence(source: &str) -> PlantUmlConversion {
    let original = source.to_string();
    let mut unsupported: Vec<PlantUmlUnsupported> = Vec::new();
    let mut out_lines: Vec<String> = vec!["sequenceDiagram".to_string()];
    let mut registry = ParticipantRegistry::new();
    let mut blocks: Vec<(BlockKind, usize)> = Vec::new();
    let mut activation_stack: Vec<(String, String)> = Vec::new();
    let mut last_message: Option<(String, String)> = None;
    let mut last_participant: Option<String> = None;
    let mut first_participant: Option<String> = None;
    let mut top_participants: Vec<OrderedParticipant> = Vec::new();
    let mut seen_top_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut has_any_order = false;
    let mut saw_box = false;
    let mut participant_seq: usize = 0;
    let mut pending_note: Option<PendingNote> = None;
    let mut pending_title: Option<(usize, Vec<String>)> = None;
    let mut skip_until: Option<SkipUntil> = None;
    let mut in_block_comment = false;
    let mut saw_enduml = false;
    let mut non_sequence = false;
    let mut has_sequence_content = false;

    let mut push_unsupported = |line: usize, kind: &'static str, src: &str, reason: &str| {
        unsupported.push(PlantUmlUnsupported {
            line,
            kind,
            source: excerpt(src, 120),
            reason: reason.to_string(),
        });
    };

    for (idx, raw_line) in source.lines().enumerate() {
        let line_no = idx + 1;
        if saw_enduml {
            continue;
        }

        // Strip PlantUML block comments `/' ... '/`.
        let cleaned = strip_block_comments(raw_line, &mut in_block_comment);
        let trimmed = cleaned.trim();

        // Multi-line skip states (macro bodies, <style>, skinparam { ... }, ref, header/footer/legend).
        if let Some(mode) = skip_until {
            let lower = trimmed.to_ascii_lowercase();
            let done = match mode {
                SkipUntil::MacroProc => {
                    lower.starts_with("!endprocedure") || lower.starts_with("!endfunction")
                }
                SkipUntil::StyleTag => lower.contains("</style>"),
                SkipUntil::BraceBlock => trimmed == "}" || trimmed.ends_with('}'),
                SkipUntil::RefBlock => lower == "end ref" || lower == "endref",
                SkipUntil::TitleBlock => lower == "end title" || lower == "endtitle",
                SkipUntil::EndKeyword(end_kw) => lower == end_kw,
            };
            if mode == SkipUntil::TitleBlock {
                if done {
                    if let Some((_, lines)) = pending_title.take() {
                        let joined = lines.join(" / ");
                        if !joined.is_empty() {
                            out_lines.push(format!("  title {}", sanitize_label(&joined)));
                        }
                    }
                    skip_until = None;
                } else if let Some((_, lines)) = pending_title.as_mut() {
                    if !trimmed.is_empty() {
                        lines.push(trimmed.to_string());
                    }
                }
            } else if done {
                skip_until = None;
            }
            continue;
        }

        // Multi-line note accumulation (`note ...` until `end note`).
        if let Some(mut note) = pending_note.take() {
            let lower = trimmed.to_ascii_lowercase();
            if matches!(
                lower.as_str(),
                "end note" | "endnote" | "end hnote" | "end rnote"
            ) {
                let text = sanitize_label(&note.lines.join(" / "));
                out_lines.push(format!("{}{}: {}", indent(&blocks), note.header, text));
                has_sequence_content = true;
            } else {
                if !trimmed.is_empty() {
                    note.lines.push(trimmed.to_string());
                }
                pending_note = Some(note);
            }
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        // Single-line comment `' ...`.
        if let Some(comment) = trimmed.strip_prefix('\'') {
            let comment = comment.trim();
            if !comment.is_empty() {
                out_lines.push(format!("{}%% {}", indent(&blocks), comment));
            }
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();

        // Envelope directives: `@start...` / `@end...`.
        if lower.starts_with("@start") {
            let directive = lower.split_whitespace().next().unwrap_or("");
            if directive != "@startuml" {
                non_sequence = true;
                push_unsupported(
                    line_no,
                    "diagram-type",
                    trimmed,
                    "Only @startuml basic sequence diagrams are supported.",
                );
            }
            continue;
        }
        if lower.starts_with("@end") {
            saw_enduml = true;
            continue;
        }

        // Preprocessor macros (`!include`, `!define`, `!procedure`, `!if`, etc.) and `$macro(...)`.
        if trimmed.starts_with('!') {
            push_unsupported(
                line_no,
                "macro",
                trimmed,
                "PlantUML preprocessor directives and macros are not expanded.",
            );
            if lower.starts_with("!procedure ")
                || lower.starts_with("!unquoted procedure ")
                || lower.starts_with("!function ")
                || lower.starts_with("!unquoted function ")
            {
                skip_until = Some(SkipUntil::MacroProc);
            }
            continue;
        }
        if trimmed.starts_with('$') {
            push_unsupported(
                line_no,
                "macro",
                trimmed,
                "PlantUML macro invocations are not expanded.",
            );
            continue;
        }

        // Timing diagram constructs (`robust`, `concise`, `clock`, `binary`, `analog`, `@0`, `@+10`).
        if is_timing_statement(trimmed, &lower) {
            non_sequence = true;
            push_unsupported(
                line_no,
                "timing",
                trimmed,
                "PlantUML timing diagrams are outside the Mermaid sequence conversion scope.",
            );
            continue;
        }

        // Other non-sequence UML constructs (`class`, `interface`, `state`, `usecase`, etc.).
        if is_non_sequence_uml(trimmed, &lower) {
            non_sequence = true;
            push_unsupported(
                line_no,
                "diagram-type",
                trimmed,
                "Non-sequence PlantUML diagram syntax is not supported.",
            );
            continue;
        }

        // Styling / layout directives that Mermaid sequenceDiagram does not support directly.
        if lower.starts_with("skinparam") {
            push_unsupported(
                line_no,
                "styling",
                trimmed,
                "skinparam styling directives are not converted to Mermaid.",
            );
            if trimmed.ends_with('{') {
                skip_until = Some(SkipUntil::BraceBlock);
            }
            continue;
        }
        if lower.starts_with("<style>") {
            push_unsupported(
                line_no,
                "styling",
                trimmed,
                "<style> blocks are not converted to Mermaid.",
            );
            if !lower.contains("</style>") {
                skip_until = Some(SkipUntil::StyleTag);
            }
            continue;
        }
        if lower.starts_with("hide ") || lower.starts_with("show ") {
            push_unsupported(
                line_no,
                "styling",
                trimmed,
                "hide/show directives are not supported in Mermaid sequenceDiagram.",
            );
            continue;
        }
        if lower.starts_with("scale ")
            || lower == "newpage"
            || lower.starts_with("newpage ")
            || lower.starts_with("autoactivate ")
            || lower.starts_with("caption ")
        {
            push_unsupported(
                line_no,
                "layout",
                trimmed,
                "Layout/pagination directive is not supported in Mermaid sequenceDiagram.",
            );
            continue;
        }
        if let Some(block_end) = multi_line_unsupported_block(&lower) {
            push_unsupported(
                line_no,
                "layout",
                trimmed,
                "header/footer/legend blocks are not supported in Mermaid sequenceDiagram.",
            );
            if !trimmed.contains(' ') || lower == "legend" || lower == "header" || lower == "footer"
            {
                skip_until = Some(SkipUntil::EndKeyword(block_end));
            }
            continue;
        }

        // Dividers (`== ... ==`), delays (`...`), spacing (`|||`, `||45||`).
        if trimmed.starts_with("==") && trimmed.ends_with("==") && trimmed.len() >= 4 {
            push_unsupported(
                line_no,
                "divider",
                trimmed,
                "Sequence dividers (== ... ==) have no direct Mermaid equivalent.",
            );
            continue;
        }
        if trimmed.starts_with("...") && trimmed.ends_with("...") {
            push_unsupported(
                line_no,
                "delay",
                trimmed,
                "Sequence delays (... ... ...) have no direct Mermaid equivalent.",
            );
            continue;
        }
        if trimmed == "|||"
            || (trimmed.starts_with("||") && trimmed.ends_with("||") && trimmed.len() >= 4)
        {
            push_unsupported(
                line_no,
                "layout",
                trimmed,
                "Explicit vertical spacing (|||) is not supported in Mermaid sequenceDiagram.",
            );
            continue;
        }

        // `ref over ...`
        if lower.starts_with("ref over ") {
            push_unsupported(
                line_no,
                "reference",
                trimmed,
                "Interaction-use frames (ref over) are not supported by the Mermaid renderer.",
            );
            if !trimmed.contains(':') {
                skip_until = Some(SkipUntil::RefBlock);
            }
            continue;
        }

        // Teoz parallel prefix `& ...`.
        if trimmed.starts_with("& ") {
            push_unsupported(
                line_no,
                "layout",
                trimmed,
                "Teoz parallel prefix (&) is not supported in Mermaid sequenceDiagram.",
            );
            continue;
        }

        // `autonumber`
        if lower == "autonumber" || lower.starts_with("autonumber ") {
            let arg = lower.strip_prefix("autonumber").unwrap_or("").trim();
            if arg.is_empty() {
                out_lines.push("  autonumber".to_string());
            } else if arg == "stop"
                || arg == "resume"
                || arg.starts_with("resume ")
                || arg.starts_with("inc")
            {
                push_unsupported(
                    line_no,
                    "autonumber",
                    trimmed,
                    "Mid-diagram autonumber stop/resume/inc is not supported in Mermaid.",
                );
            } else {
                out_lines.push("  autonumber".to_string());
                push_unsupported(
                    line_no,
                    "autonumber",
                    trimmed,
                    "Custom autonumber start, increment, or format is simplified to plain autonumber.",
                );
            }
            continue;
        }

        // `url of <who> is [[<url> <title>]]`
        if let Some((who_raw, url, link_title)) = parse_participant_url(trimmed) {
            let (id, synth) = registry.resolve(who_raw);
            if let Some(decl) = synth {
                if seen_top_ids.insert(id.clone()) && !saw_box {
                    participant_seq += 1;
                    top_participants.push(OrderedParticipant {
                        order: 0,
                        had_order: false,
                        seq: participant_seq,
                        line_no,
                        raw_line: trimmed.to_string(),
                        emitted: decl,
                    });
                } else {
                    out_lines.push(decl);
                }
            } else if seen_top_ids.insert(id.clone()) && !saw_box {
                participant_seq += 1;
                top_participants.push(OrderedParticipant {
                    order: 0,
                    had_order: false,
                    seq: participant_seq,
                    line_no,
                    raw_line: trimmed.to_string(),
                    emitted: format!("  participant {id}"),
                });
            }
            let title_text = sanitize_label(link_title.unwrap_or(url));
            out_lines.push(format!("{}link {id}: {title_text} @ {url}", indent(&blocks)));
            has_sequence_content = true;
            continue;
        }

        // `title`
        if lower == "title" {
            pending_title = Some((line_no, Vec::new()));
            skip_until = Some(SkipUntil::TitleBlock);
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "title ") {
            let text = sanitize_label(rest.trim());
            if !text.is_empty() {
                out_lines.push(format!("  title {text}"));
            }
            continue;
        }

        // `box ...` / `end box`
        if lower == "end box" || lower == "endbox" {
            if matches!(blocks.last(), Some((BlockKind::Box, _))) {
                blocks.pop();
                out_lines.push(format!("{}end", indent(&blocks)));
            } else {
                push_unsupported(
                    line_no,
                    "syntax",
                    trimmed,
                    "Unexpected `end box` without a matching `box`.",
                );
            }
            continue;
        }
        if lower == "box" || lower.starts_with("box ") {
            if !saw_box {
                saw_box = true;
                for entry in top_participants.drain(..) {
                    if entry.had_order {
                        push_unsupported(
                            entry.line_no,
                            "layout",
                            &entry.raw_line,
                            "Participant `order` cannot be reordered when `box` groups are present.",
                        );
                    }
                    out_lines.push(entry.emitted);
                }
            }
            let rest = trimmed.get(3..).unwrap_or("").trim();
            let (title, color) = parse_box_args(rest);
            let mut line_out = format!("{}box", indent(&blocks));
            if let Some(c) = color {
                line_out.push(' ');
                line_out.push_str(c);
            }
            if !title.is_empty() {
                line_out.push(' ');
                line_out.push_str(&sanitize_label(&title));
            }
            out_lines.push(line_out);
            blocks.push((BlockKind::Box, line_no));
            continue;
        }

        // Participant declarations (`[create] participant|actor|boundary|control|entity|database|collections|queue ...`)
        if let Some(decl) = parse_participant_declaration(trimmed) {
            if decl.had_styling {
                push_unsupported(
                    line_no,
                    "styling",
                    trimmed,
                    "Participant color or stereotype is not preserved in Mermaid.",
                );
            }
            let id = registry.register_declared(&decl.alias, decl.label.as_deref());
            if first_participant.is_none() {
                first_participant = Some(id.clone());
            }
            last_participant = Some(id.clone());
            let create_prefix = if decl.is_create { "create " } else { "" };
            let emitted = format_mermaid_participant(
                create_prefix,
                decl.kind,
                &id,
                decl.label.as_deref(),
            );
            let in_box = blocks.iter().any(|(k, _)| *k == BlockKind::Box);
            if !decl.is_create && !saw_box && !in_box {
                if decl.order.is_some() {
                    has_any_order = true;
                }
                let line_emitted = format!("  {emitted}");
                if let Some(existing) = top_participants.iter_mut().find(|p| {
                    p.emitted
                        .trim_start()
                        .split_whitespace()
                        .nth(1)
                        .map(|tok| tok.split("@{").next().unwrap_or(tok))
                        == Some(id.as_str())
                }) {
                    existing.emitted = line_emitted;
                    if let Some(ord) = decl.order {
                        existing.order = ord;
                        existing.had_order = true;
                    }
                } else {
                    seen_top_ids.insert(id);
                    participant_seq += 1;
                    top_participants.push(OrderedParticipant {
                        order: decl.order.unwrap_or(0),
                        had_order: decl.order.is_some(),
                        seq: participant_seq,
                        line_no,
                        raw_line: trimmed.to_string(),
                        emitted: line_emitted,
                    });
                }
            } else {
                if decl.order.is_some() {
                    push_unsupported(
                        line_no,
                        "layout",
                        trimmed,
                        "Participant `order` is not preserved inside `box` or on `create`.",
                    );
                }
                out_lines.push(format!("{}{}", indent(&blocks), emitted));
            }
            has_sequence_content = true;
            continue;
        }

        // Bare `create <who>` without keyword.
        if let Some(rest) = strip_prefix_ci(trimmed, "create ") {
            let (id, _) = registry.resolve(rest.trim());
            if first_participant.is_none() {
                first_participant = Some(id.clone());
            }
            last_participant = Some(id.clone());
            out_lines.push(format!("{}create participant {id}", indent(&blocks)));
            has_sequence_content = true;
            continue;
        }

        // `activate <who> [#color]`, `deactivate <who>`, `destroy <who>`
        if let Some(rest) = strip_prefix_ci(trimmed, "activate ") {
            let (who_raw, had_color) = split_trailing_color(rest.trim());
            if had_color {
                push_unsupported(
                    line_no,
                    "styling",
                    trimmed,
                    "Activation bar color is not preserved in Mermaid.",
                );
            }
            let (id, synth) = registry.resolve(who_raw);
            if let Some(decl) = synth {
                if seen_top_ids.insert(id.clone()) && !saw_box {
                    participant_seq += 1;
                    top_participants.push(OrderedParticipant {
                        order: 0,
                        had_order: false,
                        seq: participant_seq,
                        line_no,
                        raw_line: trimmed.to_string(),
                        emitted: decl,
                    });
                } else {
                    out_lines.push(decl);
                }
            } else if seen_top_ids.insert(id.clone()) && !saw_box {
                participant_seq += 1;
                top_participants.push(OrderedParticipant {
                    order: 0,
                    had_order: false,
                    seq: participant_seq,
                    line_no,
                    raw_line: trimmed.to_string(),
                    emitted: format!("  participant {id}"),
                });
            }
            if let Some((ref from, ref to)) = last_message
                && to == &id
            {
                activation_stack.push((from.clone(), id.clone()));
            }
            out_lines.push(format!("{}activate {id}", indent(&blocks)));
            last_participant = Some(id);
            has_sequence_content = true;
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "deactivate ") {
            let (id, synth) = registry.resolve(rest.trim());
            if let Some(decl) = synth {
                out_lines.push(decl);
            }
            if let Some(pos) = activation_stack.iter().rposition(|(_, callee)| callee == &id) {
                activation_stack.remove(pos);
            }
            out_lines.push(format!("{}deactivate {id}", indent(&blocks)));
            has_sequence_content = true;
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "destroy ") {
            let (id, synth) = registry.resolve(rest.trim());
            if let Some(decl) = synth {
                out_lines.push(decl);
            }
            out_lines.push(format!("{}destroy {id}", indent(&blocks)));
            has_sequence_content = true;
            continue;
        }

        // `return [label]`
        if lower == "return" || lower.starts_with("return ") {
            let label = sanitize_label(trimmed.get(6..).unwrap_or("").trim());
            if let Some((caller, callee)) = activation_stack.pop() {
                out_lines.push(format!(
                    "{}{callee}-->>-{caller}: {label}",
                    indent(&blocks)
                ));
                last_message = Some((callee, caller.clone()));
                last_participant = Some(caller);
                has_sequence_content = true;
            } else {
                push_unsupported(
                    line_no,
                    "syntax",
                    trimmed,
                    "`return` requires a preceding activation (`++` or `activate`).",
                );
            }
            continue;
        }

        // Combined fragments (`alt`, `else`, `opt`, `loop`, `par`, `break`, `critical`, `option`, `group`, `end`)
        if let Some((kind, raw_label, had_color)) = parse_fragment_start(trimmed) {
            if had_color {
                push_unsupported(
                    line_no,
                    "styling",
                    trimmed,
                    "Fragment background color is not preserved in Mermaid.",
                );
            }
            if kind == BlockKind::Group {
                push_unsupported(
                    line_no,
                    "fragment",
                    trimmed,
                    "PlantUML `group` has no direct Mermaid keyword and is mapped to `opt`.",
                );
            }
            let kw = match kind {
                BlockKind::Alt => "alt",
                BlockKind::Opt | BlockKind::Group => "opt",
                BlockKind::Loop => "loop",
                BlockKind::Par => "par",
                BlockKind::Break => "break",
                BlockKind::Critical => "critical",
                BlockKind::Box => unreachable!(),
            };
            // `sequence.rs` trims lines and matches `strip_prefix("<kw> ")`, so ensure a label.
            let label = sanitize_label(raw_label);
            let emitted_label = if label.is_empty() { "-" } else { &label };
            out_lines.push(format!("{}{kw} {emitted_label}", indent(&blocks)));
            blocks.push((kind, line_no));
            has_sequence_content = true;
            continue;
        }
        if lower == "else" || lower.starts_with("else ") || lower.starts_with("else#") {
            let (label_clean, had_color) =
                strip_leading_fragment_colors(trimmed.get(4..).unwrap_or(""));
            if had_color {
                push_unsupported(
                    line_no,
                    "styling",
                    trimmed,
                    "Fragment branch background color is not preserved in Mermaid.",
                );
            }
            let label = sanitize_label(label_clean);
            match blocks.last().map(|(k, _)| *k) {
                Some(BlockKind::Par) => {
                    let parent_indent = indent(&blocks[..blocks.len() - 1]);
                    if label.is_empty() {
                        out_lines.push(format!("{parent_indent}and"));
                    } else {
                        out_lines.push(format!("{parent_indent}and {label}"));
                    }
                }
                Some(BlockKind::Alt | BlockKind::Critical | BlockKind::Opt | BlockKind::Group) => {
                    let parent_indent = indent(&blocks[..blocks.len() - 1]);
                    if label.is_empty() {
                        out_lines.push(format!("{parent_indent}else"));
                    } else {
                        out_lines.push(format!("{parent_indent}else {label}"));
                    }
                }
                _ => {
                    push_unsupported(
                        line_no,
                        "syntax",
                        trimmed,
                        "Unexpected `else` outside an `alt`/`par`/`critical` fragment.",
                    );
                }
            }
            continue;
        }
        if (lower == "option" || lower.starts_with("option "))
            && matches!(blocks.last(), Some((BlockKind::Critical, _)))
        {
            let label = sanitize_label(trimmed.get(6..).unwrap_or("").trim());
            let parent_indent = indent(&blocks[..blocks.len() - 1]);
            let emitted_label = if label.is_empty() { "-" } else { &label };
            out_lines.push(format!("{parent_indent}option {emitted_label}"));
            continue;
        }
        if lower == "end" {
            if let Some((kind, _)) = blocks.pop() {
                if kind == BlockKind::Box {
                    push_unsupported(
                        line_no,
                        "syntax",
                        trimmed,
                        "Closed `box` with `end` instead of `end box`.",
                    );
                }
                out_lines.push(format!("{}end", indent(&blocks)));
            } else {
                push_unsupported(
                    line_no,
                    "syntax",
                    trimmed,
                    "Unexpected `end` without an open fragment.",
                );
            }
            continue;
        }

        // Notes (`note`, `hnote`, `rnote`, and aligned `/ note ...`)
        let (aligned_note, note_candidate) = match trimmed.strip_prefix("/ ") {
            Some(rest) => (true, rest.trim_start()),
            None => (false, trimmed),
        };
        if let Some(note_res) = parse_note_line(
            note_candidate,
            &mut registry,
            first_participant.as_deref(),
            last_participant.as_deref(),
        ) {
            if aligned_note {
                push_unsupported(
                    line_no,
                    "layout",
                    trimmed,
                    "Aligned side-by-side note prefix (/) is converted as a sequential note.",
                );
            }
            if note_res.styled_variant {
                push_unsupported(
                    line_no,
                    "styling",
                    trimmed,
                    "Note shape (hnote/rnote) or color is not preserved in Mermaid.",
                );
            }
            if note_res.is_across {
                push_unsupported(
                    line_no,
                    "layout",
                    trimmed,
                    "`note across` is approximated as `Note over` the currently known span.",
                );
            }
            for (id, decl) in note_res.synth_decls {
                if seen_top_ids.insert(id) && !saw_box {
                    participant_seq += 1;
                    top_participants.push(OrderedParticipant {
                        order: 0,
                        had_order: false,
                        seq: participant_seq,
                        line_no,
                        raw_line: trimmed.to_string(),
                        emitted: decl,
                    });
                } else {
                    out_lines.push(decl);
                }
            }
            match note_res.body {
                NoteBody::Inline(text) => {
                    out_lines.push(format!(
                        "{}{}: {}",
                        indent(&blocks),
                        note_res.header,
                        sanitize_label(&text)
                    ));
                    has_sequence_content = true;
                }
                NoteBody::Multiline => {
                    pending_note = Some(PendingNote {
                        line: line_no,
                        header: note_res.header,
                        lines: Vec::new(),
                    });
                }
                NoteBody::MissingParticipant => {
                    push_unsupported(
                        line_no,
                        "syntax",
                        trimmed,
                        "Implicit note position requires a preceding participant or message.",
                    );
                }
            }
            continue;
        }

        // Sequence messages (`A -> B : label`)
        if let Some(msg) = parse_message_line(trimmed) {
            if msg.gate {
                push_unsupported(
                    line_no,
                    "syntax",
                    trimmed,
                    "Incoming/outgoing boundary gate arrows ([-> / ->]) are not supported in Mermaid.",
                );
                continue;
            }
            if msg.styled_lossy {
                push_unsupported(
                    line_no,
                    "styling",
                    trimmed,
                    "Arrow color or thickness styling is not preserved in Mermaid.",
                );
            }
            if msg.activation_colored {
                push_unsupported(
                    line_no,
                    "styling",
                    trimmed,
                    "Shorthand activation color is not preserved in Mermaid.",
                );
            }
            let (raw_from, raw_to) = if msg.reverse {
                (msg.rhs, msg.lhs)
            } else {
                (msg.lhs, msg.rhs)
            };
            let (from_id, from_decl) = registry.resolve(raw_from);
            let (to_id, to_decl) = registry.resolve(raw_to);
            if first_participant.is_none() {
                first_participant = Some(from_id.clone());
            }
            for (pid, maybe_decl, is_created) in [
                (&from_id, from_decl, false),
                (&to_id, to_decl, msg.create_target),
            ] {
                if !is_created {
                    if let Some(decl) = maybe_decl {
                        if seen_top_ids.insert(pid.clone()) && !saw_box {
                            participant_seq += 1;
                            top_participants.push(OrderedParticipant {
                                order: 0,
                                had_order: false,
                                seq: participant_seq,
                                line_no,
                                raw_line: trimmed.to_string(),
                                emitted: decl,
                            });
                        } else {
                            out_lines.push(decl);
                        }
                    } else if seen_top_ids.insert(pid.clone()) && !saw_box {
                        participant_seq += 1;
                        top_participants.push(OrderedParticipant {
                            order: 0,
                            had_order: false,
                            seq: participant_seq,
                            line_no,
                            raw_line: trimmed.to_string(),
                            emitted: format!("  participant {pid}"),
                        });
                    }
                }
            }
            let ind = indent(&blocks);
            if msg.create_target {
                out_lines.push(format!("{ind}create participant {to_id}"));
            }
            if msg.destroy_target {
                out_lines.push(format!("{ind}destroy {to_id}"));
            }
            let source_suffix = if msg.circle_source { "()" } else { "" };
            let target_prefix = if msg.circle_target { "()" } else { "" };
            let label = sanitize_label(msg.label);
            let suffix = match (msg.activate_target, msg.deactivate_source) {
                (true, false) => {
                    activation_stack.push((from_id.clone(), to_id.clone()));
                    "+"
                }
                (false, true) => {
                    if let Some(pos) = activation_stack
                        .iter()
                        .rposition(|(_, callee)| callee == &from_id)
                    {
                        activation_stack.remove(pos);
                    }
                    "-"
                }
                (true, true) => {
                    out_lines.push(format!("{ind}deactivate {from_id}"));
                    if let Some(pos) = activation_stack
                        .iter()
                        .rposition(|(_, callee)| callee == &from_id)
                    {
                        activation_stack.remove(pos);
                    }
                    activation_stack.push((from_id.clone(), to_id.clone()));
                    "+"
                }
                (false, false) => "",
            };
            // `sequence.rs` strips `()` before `+`/`-`, so `()` must precede any `+`/`-` token.
            out_lines.push(format!(
                "{ind}{from_id}{source_suffix}{}{target_prefix}{suffix}{to_id}: {label}",
                msg.arrow
            ));
            last_message = Some((from_id, to_id.clone()));
            last_participant = Some(to_id);
            has_sequence_content = true;
            continue;
        }

        // Anything else is reported with its 1-based line number.
        push_unsupported(
            line_no,
            "syntax",
            trimmed,
            "Unsupported PlantUML sequence syntax.",
        );
    }

    if let Some(note) = pending_note {
        push_unsupported(
            note.line,
            "syntax",
            &note.header,
            "Unclosed multi-line note (missing `end note`).",
        );
    }
    if let Some((line_no, _)) = pending_title {
        push_unsupported(
            line_no,
            "syntax",
            "title",
            "Unclosed multi-line `title` block (missing `end title`).",
        );
    }
    while let Some((kind, open_line)) = blocks.pop() {
        let label = match kind {
            BlockKind::Box => "box",
            BlockKind::Alt => "alt",
            BlockKind::Opt => "opt",
            BlockKind::Loop => "loop",
            BlockKind::Par => "par",
            BlockKind::Break => "break",
            BlockKind::Critical => "critical",
            BlockKind::Group => "group",
        };
        push_unsupported(
            open_line,
            "syntax",
            label,
            "Unclosed fragment or box block (missing `end`).",
        );
        out_lines.push(format!("{}end", indent(&blocks)));
    }

    // Emit buffered top-level participants right after `sequenceDiagram` / header metadata.
    // When `order N` was used (and no `box` group prevents reordering), sort stably by `(order, seq)`.
    if !top_participants.is_empty() {
        if has_any_order {
            top_participants.sort_by_key(|p| (p.order, p.seq));
        }
        let insert_at = out_lines
            .iter()
            .enumerate()
            .skip(1)
            .take_while(|(_, line)| {
                let t = line.trim();
                t == "autonumber" || t.starts_with("title ") || t.starts_with("%% ")
            })
            .last()
            .map_or(1, |(i, _)| i + 1);
        let emitted: Vec<String> = top_participants.into_iter().map(|p| p.emitted).collect();
        out_lines.splice(insert_at..insert_at, emitted);
    }

    let mermaid = if !non_sequence && has_sequence_content {
        let mut text = out_lines.join("\n");
        text.push('\n');
        Some(text)
    } else {
        None
    };

    PlantUmlConversion {
        original,
        mermaid,
        unsupported,
    }
}

fn indent(blocks: &[(BlockKind, usize)]) -> String {
    "  ".repeat(blocks.len() + 1)
}

pub(super) fn excerpt(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let taken: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{taken}...")
    } else {
        taken
    }
}

pub(super) fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    s.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(s)
}

pub(super) fn sanitize_label(raw: &str) -> String {
    raw.trim()
        .replace("\\n", " / ")
        .replace('\r', "")
        .replace('\n', " / ")
}

pub(super) fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Remove PlantUML `/' ... '/` block comments on one line, updating `in_comment`.
pub(super) fn strip_block_comments(line: &str, in_comment: &mut bool) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while !rest.is_empty() {
        if *in_comment {
            if let Some(pos) = rest.find("'/") {
                *in_comment = false;
                rest = &rest[pos + 2..];
            } else {
                return out;
            }
        } else if let Some(pos) = rest.find("/'") {
            out.push_str(&rest[..pos]);
            *in_comment = true;
            rest = &rest[pos + 2..];
        } else {
            out.push_str(rest);
            break;
        }
    }
    out
}

pub(super) fn is_timing_statement(trimmed: &str, lower: &str) -> bool {
    if trimmed.starts_with('@') && !lower.starts_with("@start") && !lower.starts_with("@end") {
        let after = &trimmed[1..];
        if after
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '+')
        {
            return true;
        }
    }
    [
        "robust ",
        "concise ",
        "clock ",
        "binary ",
        "analog ",
        "mode ",
    ]
    .iter()
    .any(|kw| lower.starts_with(kw))
}

fn is_non_sequence_uml(trimmed: &str, lower: &str) -> bool {
    if trimmed.starts_with("[*]") {
        return true;
    }
    [
        "class ",
        "abstract class ",
        "abstract ",
        "interface ",
        "enum ",
        "annotation ",
        "struct ",
        "state ",
        "usecase ",
        "component ",
        "node ",
        "package ",
        "namespace ",
        "object ",
        "folder ",
        "cloud ",
        "frame ",
        "rectangle ",
        "card ",
        "agent ",
        "artifact ",
        "storage ",
    ]
    .iter()
    .any(|kw| lower.starts_with(kw))
}

pub(super) fn multi_line_unsupported_block(lower: &str) -> Option<&'static str> {
    if lower == "header" || lower.starts_with("header ") {
        Some("end header")
    } else if lower == "footer" || lower.starts_with("footer ") {
        Some("end footer")
    } else if lower == "legend" || lower.starts_with("legend ") {
        Some("end legend")
    } else {
        None
    }
}

pub(super) fn split_trailing_color(s: &str) -> (&str, bool) {
    if let Some((head, tail)) = s.rsplit_once('#') {
        let tail = tail.trim();
        let head_trimmed = head.trim_end();
        if !head_trimmed.is_empty()
            && head.ends_with(char::is_whitespace)
            && !tail.is_empty()
            && !tail.contains(char::is_whitespace)
            && !tail.contains('"')
        {
            return (head_trimmed, true);
        }
    }
    (s, false)
}

fn parse_box_args(rest: &str) -> (String, Option<&str>) {
    let rest = rest.trim();
    if rest.is_empty() {
        return (String::new(), None);
    }
    if let Some(after_quote) = rest.strip_prefix('"')
        && let Some(end_quote) = after_quote.find('"')
    {
        let title = after_quote[..end_quote].to_string();
        let tail = after_quote[end_quote + 1..].trim();
        let color = tail.strip_prefix('#').map(|c| {
            let token = c.split_whitespace().next().unwrap_or("");
            &tail[..1 + token.len()]
        });
        return (title, color.filter(|c| c.len() > 1));
    }
    let (head, had_color) = split_trailing_color(rest);
    if had_color {
        let color = rest[head.len()..].trim();
        return (head.to_string(), Some(color));
    }
    (rest.to_string(), None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParticipantKind {
    Participant,
    Actor,
    Boundary,
    Control,
    Entity,
    Database,
    Collections,
    Queue,
}

struct ParsedParticipant {
    is_create: bool,
    kind: ParticipantKind,
    alias: String,
    label: Option<String>,
    had_styling: bool,
    order: Option<i32>,
}

fn parse_participant_url(line: &str) -> Option<(&str, &str, Option<&str>)> {
    let rest = strip_prefix_ci(line, "url of ")?.trim_start();
    let open = rest.find("[[")?;
    let before = rest[..open].trim_end();
    let suffix_start = before.len().saturating_sub(3);
    let who = if before
        .get(suffix_start..)
        .is_some_and(|tail| tail.eq_ignore_ascii_case(" is"))
    {
        before[..suffix_start].trim()
    } else {
        return None;
    };
    let after_open = &rest[open + 2..];
    let close = after_open.find("]]")?;
    let inner = after_open[..close].trim();
    if who.is_empty() || inner.is_empty() {
        return None;
    }
    let (url, title) = match inner.split_once(char::is_whitespace) {
        Some((u, t)) => {
            let t = t.trim();
            (u, (!t.is_empty()).then_some(t))
        }
        None => (inner, None),
    };
    Some((who, url, title))
}

fn parse_participant_declaration(line: &str) -> Option<ParsedParticipant> {
    let (is_create, rest) = match strip_prefix_ci(line, "create ") {
        Some(after) => (true, after.trim_start()),
        None => (false, line),
    };
    let (kind, after_kw) = [
        ("participant ", ParticipantKind::Participant),
        ("actor ", ParticipantKind::Actor),
        ("boundary ", ParticipantKind::Boundary),
        ("control ", ParticipantKind::Control),
        ("entity ", ParticipantKind::Entity),
        ("database ", ParticipantKind::Database),
        ("collections ", ParticipantKind::Collections),
        ("queue ", ParticipantKind::Queue),
    ]
    .iter()
    .find_map(|&(kw, kind)| strip_prefix_ci(rest, kw).map(|r| (kind, r.trim())))?;

    let mut body = after_kw;
    let mut had_styling = false;
    let mut order: Option<i32> = None;

    // Strip trailing `#Color`, `order N`, `<<stereotype>>` in any order.
    loop {
        let before = body;
        let (stripped, color) = split_trailing_color(body);
        if color {
            had_styling = true;
            body = stripped;
        }
        if let Some((head, tail)) = body.rsplit_once(" order ") {
            if let Ok(parsed) = tail.trim().parse::<i32>() {
                order = Some(parsed);
                body = head.trim_end();
            }
        }
        if let Some(open) = body.rfind("<<")
            && body.ends_with(">>")
            && open > 0
            && !is_inside_quotes(&body[..open])
        {
            had_styling = true;
            body = body[..open].trim_end();
        }
        if body == before {
            break;
        }
    }

    let (alias, label) = if let Some((lhs, rhs)) = split_as_outside_quotes(body) {
        let lhs = lhs.trim();
        let rhs = rhs.trim();
        if lhs.starts_with('"') && lhs.ends_with('"') {
            // PlantUML canonical form: `participant "Display Name" as Alias`
            (strip_quotes(rhs).to_string(), Some(strip_quotes(lhs).to_string()))
        } else if rhs.starts_with('"') && rhs.ends_with('"') {
            // Alternate form: `participant Alias as "Display Name"`
            (lhs.to_string(), Some(strip_quotes(rhs).to_string()))
        } else {
            // Unquoted PlantUML form: `participant DisplayName as Alias`
            (rhs.to_string(), Some(lhs.to_string()))
        }
    } else if body.starts_with('"') && body.ends_with('"') {
        let unquoted = strip_quotes(body).to_string();
        (unquoted.clone(), Some(unquoted))
    } else {
        (body.trim().to_string(), None)
    };

    if alias.is_empty() {
        return None;
    }
    Some(ParsedParticipant {
        is_create,
        kind,
        alias,
        label,
        had_styling,
        order,
    })
}

fn is_inside_quotes(prefix: &str) -> bool {
    prefix.chars().filter(|&c| c == '"').count() % 2 == 1
}

pub(super) fn split_as_outside_quotes(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut in_quotes = false;
    for i in 0..bytes.len() {
        if bytes[i] == b'"' {
            in_quotes = !in_quotes;
        } else if !in_quotes
            && i + 4 <= bytes.len()
            && s.is_char_boundary(i)
            && s.is_char_boundary(i + 4)
            && s[i..i + 4].eq_ignore_ascii_case(" as ")
        {
            return Some((&s[..i], &s[i + 4..]));
        }
    }
    None
}

fn format_mermaid_participant(
    create_prefix: &str,
    kind: ParticipantKind,
    id: &str,
    label: Option<&str>,
) -> String {
    let label = label.map(sanitize_label).filter(|l| !l.is_empty() && l != id);
    match kind {
        ParticipantKind::Participant => match label {
            Some(lbl) => format!("{create_prefix}participant {id} as {lbl}"),
            None => format!("{create_prefix}participant {id}"),
        },
        ParticipantKind::Actor => match label {
            Some(lbl) => format!("{create_prefix}actor {id} as {lbl}"),
            None => format!("{create_prefix}actor {id}"),
        },
        ParticipantKind::Boundary
        | ParticipantKind::Control
        | ParticipantKind::Entity
        | ParticipantKind::Database
        | ParticipantKind::Collections
        | ParticipantKind::Queue => {
            let type_name = match kind {
                ParticipantKind::Boundary => "boundary",
                ParticipantKind::Control => "control",
                ParticipantKind::Entity => "entity",
                ParticipantKind::Database => "database",
                ParticipantKind::Collections => "collections",
                ParticipantKind::Queue => "queue",
                _ => unreachable!(),
            };
            match label {
                Some(lbl) => {
                    let escaped = lbl.replace('\\', "\\\\").replace('"', "\\\"");
                    format!(
                        r#"{create_prefix}participant {id}@{{ "type": "{type_name}", "alias": "{escaped}" }}"#
                    )
                }
                None => format!(r#"{create_prefix}participant {id}@{{ "type": "{type_name}" }}"#),
            }
        }
    }
}

/// Strip leading `#Color` tokens from PlantUML fragment headers (`alt#Gold #LightBlue ok`).
fn strip_leading_fragment_colors(rest: &str) -> (&str, bool) {
    let mut s = rest.trim_start();
    let mut had_color = false;
    while let Some(after_hash) = s.strip_prefix('#') {
        let token_len = after_hash
            .find(char::is_whitespace)
            .unwrap_or(after_hash.len());
        let token = &after_hash[..token_len];
        if !token.is_empty()
            && token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/'))
        {
            had_color = true;
            s = after_hash[token_len..].trim_start();
        } else {
            break;
        }
    }
    (s.trim(), had_color)
}

fn parse_fragment_start(line: &str) -> Option<(BlockKind, &str, bool)> {
    let table = [
        ("alt", BlockKind::Alt),
        ("opt", BlockKind::Opt),
        ("loop", BlockKind::Loop),
        ("par", BlockKind::Par),
        ("break", BlockKind::Break),
        ("critical", BlockKind::Critical),
        ("group", BlockKind::Group),
    ];
    for (kw, kind) in table {
        if line.eq_ignore_ascii_case(kw) {
            return Some((kind, "", false));
        }
        if let Some(rest) = strip_prefix_ci(line, kw)
            && (rest.starts_with(char::is_whitespace) || rest.starts_with('#'))
        {
            let (label, had_color) = strip_leading_fragment_colors(rest);
            return Some((kind, label, had_color));
        }
    }
    None
}

enum NoteBody {
    Inline(String),
    Multiline,
    MissingParticipant,
}

struct ParsedNote {
    header: String,
    body: NoteBody,
    synth_decls: Vec<(String, String)>,
    styled_variant: bool,
    is_across: bool,
}

fn split_all_commas_outside_quotes(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut in_quotes = false;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                parts.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(s[start..].trim());
    parts
}

fn parse_note_line(
    line: &str,
    registry: &mut ParticipantRegistry,
    first_participant: Option<&str>,
    last_participant: Option<&str>,
) -> Option<ParsedNote> {
    let (styled_variant, rest) = if let Some(r) = strip_prefix_ci(line, "note ") {
        (false, r.trim_start())
    } else if let Some(r) = strip_prefix_ci(line, "hnote ")
        .or_else(|| strip_prefix_ci(line, "rnote "))
    {
        (true, r.trim_start())
    } else {
        return None;
    };

    let (target_raw, inline_text) = match split_colon_outside_quotes(rest) {
        Some((t, text)) => (t.trim(), Some(text.trim().to_string())),
        None => (rest.trim(), None),
    };

    let (target_clean, had_color) = split_trailing_color(target_raw);
    let styled_variant = styled_variant || had_color;
    let mut synth_decls = Vec::new();

    let (header, is_across) = if let Some(who) = strip_prefix_ci(target_clean, "left of ") {
        let (id, decl) = registry.resolve(who.trim());
        if let Some(d) = decl {
            synth_decls.push((id.clone(), d));
        }
        (format!("Note left of {id}"), false)
    } else if let Some(who) = strip_prefix_ci(target_clean, "right of ") {
        let (id, decl) = registry.resolve(who.trim());
        if let Some(d) = decl {
            synth_decls.push((id.clone(), d));
        }
        (format!("Note right of {id}"), false)
    } else if let Some(who) = strip_prefix_ci(target_clean, "over ") {
        let who = who.trim();
        let targets: Vec<&str> = split_all_commas_outside_quotes(who)
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect();
        if targets.len() >= 2 {
            let mut resolved_ids = Vec::with_capacity(targets.len());
            for part in targets {
                let (id, decl) = registry.resolve(part);
                if let Some(d) = decl {
                    synth_decls.push((id.clone(), d));
                }
                resolved_ids.push(id);
            }
            let first = &resolved_ids[0];
            let last = resolved_ids.last().unwrap();
            (format!("Note over {first},{last}"), false)
        } else {
            let (id, decl) = registry.resolve(who);
            if let Some(d) = decl {
                synth_decls.push((id.clone(), d));
            }
            (format!("Note over {id}"), false)
        }
    } else if target_clean.eq_ignore_ascii_case("left") {
        let Some(id) = last_participant else {
            return Some(ParsedNote {
                header: String::new(),
                body: NoteBody::MissingParticipant,
                synth_decls,
                styled_variant,
                is_across: false,
            });
        };
        (format!("Note left of {id}"), false)
    } else if target_clean.eq_ignore_ascii_case("right") {
        let Some(id) = last_participant else {
            return Some(ParsedNote {
                header: String::new(),
                body: NoteBody::MissingParticipant,
                synth_decls,
                styled_variant,
                is_across: false,
            });
        };
        (format!("Note right of {id}"), false)
    } else if target_clean.eq_ignore_ascii_case("across") {
        match (first_participant, last_participant) {
            (Some(first), Some(last)) => (format!("Note over {first},{last}"), true),
            _ => {
                return Some(ParsedNote {
                    header: String::new(),
                    body: NoteBody::MissingParticipant,
                    synth_decls,
                    styled_variant,
                    is_across: true,
                });
            }
        }
    } else {
        return None;
    };

    let body = match inline_text {
        Some(text) => NoteBody::Inline(text),
        None => NoteBody::Multiline,
    };
    Some(ParsedNote {
        header,
        body,
        synth_decls,
        styled_variant,
        is_across,
    })
}

pub(super) fn split_colon_outside_quotes(s: &str) -> Option<(&str, &str)> {
    let mut in_quotes = false;
    for (i, ch) in s.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => return Some((&s[..i], &s[i + 1..])),
            _ => {}
        }
    }
    None
}

struct ParsedMessage<'a> {
    lhs: &'a str,
    rhs: &'a str,
    arrow: &'static str,
    reverse: bool,
    circle_source: bool,
    circle_target: bool,
    styled_lossy: bool,
    gate: bool,
    activate_target: bool,
    deactivate_source: bool,
    activation_colored: bool,
    create_target: bool,
    destroy_target: bool,
    label: &'a str,
}

fn parse_message_line(line: &str) -> Option<ParsedMessage<'_>> {
    let (expr, label) = match split_colon_outside_quotes(line) {
        Some((e, l)) => (e.trim(), l.trim()),
        None => (line.trim(), ""),
    };

    // Detect incoming/outgoing boundary gates (`[-> A`, `A ->]`, `A <-]`, `[<- A`).
    if expr.starts_with('[') && expr.contains('-') {
        return Some(ParsedMessage {
            lhs: "",
            rhs: "",
            arrow: "",
            reverse: false,
            circle_source: false,
            circle_target: false,
            styled_lossy: false,
            gate: true,
            activate_target: false,
            deactivate_source: false,
            activation_colored: false,
            create_target: false,
            destroy_target: false,
            label,
        });
    }

    let m = find_plantuml_arrow(expr)?;
    let lhs = expr[..m.start].trim();
    let rhs_with_mods = expr[m.end..].trim();
    if rhs_with_mods == "]" || rhs_with_mods.starts_with("] ") {
        return Some(ParsedMessage {
            lhs,
            rhs: "",
            arrow: "",
            reverse: false,
            circle_source: false,
            circle_target: false,
            styled_lossy: false,
            gate: true,
            activate_target: false,
            deactivate_source: false,
            activation_colored: false,
            create_target: false,
            destroy_target: false,
            label,
        });
    }

    let (rhs_clean, activation_colored) = split_trailing_color(rhs_with_mods);
    let (rhs, activate_target, deactivate_source, create_target, destroy_target) =
        strip_message_modifiers(rhs_clean);

    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }

    Some(ParsedMessage {
        lhs,
        rhs,
        arrow: m.mermaid,
        reverse: m.reverse,
        circle_source: m.circle_source,
        circle_target: m.circle_target,
        styled_lossy: m.styled_lossy,
        gate: false,
        activate_target,
        deactivate_source,
        activation_colored,
        create_target,
        destroy_target,
        label,
    })
}

fn strip_message_modifiers(raw: &str) -> (&str, bool, bool, bool, bool) {
    let mut s = raw.trim();
    let mut activate = false;
    let mut deactivate = false;
    let mut create = false;
    let mut destroy = false;

    loop {
        let before = s;
        if let Some(head) = s.strip_suffix("--++") {
            deactivate = true;
            activate = true;
            s = head.trim_end();
        } else if let Some(head) = s.strip_suffix("++--") {
            activate = true;
            deactivate = true;
            s = head.trim_end();
        } else if let Some(head) = s.strip_suffix("++") {
            activate = true;
            s = head.trim_end();
        } else if let Some(head) = s.strip_suffix("--") {
            deactivate = true;
            s = head.trim_end();
        } else if let Some(head) = s.strip_suffix("**") {
            create = true;
            s = head.trim_end();
        } else if let Some(head) = s.strip_suffix("!!") {
            destroy = true;
            s = head.trim_end();
        }
        if s == before {
            break;
        }
    }
    (s, activate, deactivate, create, destroy)
}

fn find_plantuml_arrow(expr: &str) -> Option<ArrowMatch> {
    // First check bracketed PlantUML arrows like `-[dashed]->`, `-[#red,dashed]->`, `<-[#blue]--`.
    if let Some(styled) = find_styled_arrow(expr) {
        return Some(styled);
    }

    // Standard PlantUML sequence arrows ordered longest-first.
    // Tuple: `(pattern, mermaid_arrow, reverse, circle_source, circle_target)` where
    // `circle_source`/`circle_target` are already in logical `(from, to)` order after `reverse`.
    // Note: in PlantUML `->` is a solid filled arrowhead (Mermaid `->>`),
    // while `->>` is an open/thin arrowhead (Mermaid `->`), and half-arrows (`-\`, `-/`, etc.)
    // map to Mermaid async/open arrows (`-)` / `--)`).
    const PRIMARY_ARROWS: &[(&str, &str, bool, bool, bool)] = &[
        ("<<-->>", "<<-->>", false, false, false),
        ("<<->>", "<<->>", false, false, false),
        ("o-->o", "-->>", false, true, true),
        ("o<--o", "-->>", true, true, true),
        ("o->o", "->>", false, true, true),
        ("o<-o", "->>", true, true, true),
        ("<-->", "<<-->>", false, false, false),
        ("<->", "<<->>", false, false, false),
        ("-->x", "--x", false, false, false),
        ("->x", "-x", false, false, false),
        ("x<--", "--x", true, false, false),
        ("x<-", "-x", true, false, false),
        ("-->o", "-->>", false, false, true),
        ("o-->", "-->>", false, true, false),
        ("o<--", "-->>", true, false, true),
        ("<--o", "-->>", true, true, false),
        ("->o", "->>", false, false, true),
        ("o->", "->>", false, true, false),
        ("o<-", "->>", true, false, true),
        ("<-o", "->>", true, true, false),
        ("--\\\\", "--)", false, false, false),
        ("--//", "--)", false, false, false),
        ("\\\\--", "--)", true, false, false),
        ("//--", "--)", true, false, false),
        ("-\\\\", "-)", false, false, false),
        ("-//", "-)", false, false, false),
        ("\\\\-", "-)", true, false, false),
        ("//-", "-)", true, false, false),
        ("--\\", "--)", false, false, false),
        ("--/", "--)", false, false, false),
        ("\\--", "--)", true, false, false),
        ("/--", "--)", true, false, false),
        ("-\\", "-)", false, false, false),
        ("-/", "-)", false, false, false),
        ("\\-", "-)", true, false, false),
        ("/-", "-)", true, false, false),
        ("-->>", "-->", false, false, false),
        ("->>", "->", false, false, false),
        ("<<--", "-->", true, false, false),
        ("<<-", "->", true, false, false),
        ("-->", "-->>", false, false, false),
        ("->", "->>", false, false, false),
        ("<--", "-->>", true, false, false),
        ("<-", "->>", true, false, false),
    ];
    const FALLBACK_ARROWS: &[(&str, &str, bool, bool, bool)] = &[
        ("--x", "--x", false, false, false),
        ("-x", "-x", false, false, false),
    ];

    for table in [PRIMARY_ARROWS, FALLBACK_ARROWS] {
        let mut in_quotes = false;
        let bytes = expr.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'"' {
                in_quotes = !in_quotes;
                i += 1;
                continue;
            }
            if !in_quotes && expr.is_char_boundary(i) {
                let slice = &expr[i..];
                for &(pat, mermaid, reverse, circle_source, circle_target) in table {
                    if slice.starts_with(pat) {
                        // Ensure `x<-` / `o<-` / `o->` isn't splitting the end of an identifier.
                        let starts_alpha = pat.starts_with(['x', 'o']);
                        let prev_ok = if starts_alpha {
                            i == 0
                                || expr[..i]
                                    .chars()
                                    .next_back()
                                    .is_some_and(|c| c.is_whitespace() || c == '"')
                        } else {
                            true
                        };
                        // Ensure `--x` / `-x` / `->x` / `->o` / `<-o` isn't splitting an identifier.
                        let ends_alpha = pat.ends_with(['x', 'o']);
                        let after = &slice[pat.len()..];
                        let next_ok = if ends_alpha {
                            after.is_empty()
                                || after
                                    .chars()
                                    .next()
                                    .is_some_and(|c| c.is_whitespace() || c == '"')
                        } else {
                            true
                        };
                        if prev_ok && next_ok {
                            return Some(ArrowMatch {
                                start: i,
                                end: i + pat.len(),
                                mermaid,
                                reverse,
                                circle_source,
                                circle_target,
                                styled_lossy: false,
                            });
                        }
                    }
                }
            }
            i += 1;
        }
    }
    None
}

/// Match bracketed PlantUML arrows such as `-[dashed]->`, `-[#red]->`, `-[#0000FF,dashed]-->`,
/// `<-[#red]-`, `<-[dashed]--`.
fn find_styled_arrow(expr: &str) -> Option<ArrowMatch> {
    let mut in_quotes = false;
    let bytes = expr.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'"' {
            in_quotes = !in_quotes;
            continue;
        }
        if in_quotes || !expr.is_char_boundary(i) {
            continue;
        }
        let slice = &expr[i..];
        let (reverse, prefix_len) = if slice.starts_with("<-[") {
            (true, 3)
        } else if slice.starts_with("-[") {
            (false, 2)
        } else {
            continue;
        };
        let rest = &slice[prefix_len..];
        let Some(close) = rest.find(']') else {
            continue;
        };
        let style_spec = rest[..close].trim();
        if style_spec.is_empty() {
            continue;
        }
        let after = &rest[close + 1..];
        let (tail_len, tail_dashed) = if reverse {
            if after.starts_with("--") {
                (2, true)
            } else if after.starts_with('-') {
                (1, false)
            } else {
                continue;
            }
        } else if after.starts_with("-->") {
            (3, true)
        } else if after.starts_with("->") {
            (2, false)
        } else {
            continue;
        };
        let mut dashed = tail_dashed;
        let mut styled_lossy = false;
        for token in style_spec.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            if token.eq_ignore_ascii_case("dashed") || token.eq_ignore_ascii_case("dotted") {
                dashed = true;
            } else if token.eq_ignore_ascii_case("plain") {
                dashed = false;
            } else {
                styled_lossy = true;
            }
        }
        return Some(ArrowMatch {
            start: i,
            end: i + prefix_len + close + 1 + tail_len,
            mermaid: if dashed { "-->>" } else { "->>" },
            reverse,
            circle_source: false,
            circle_target: false,
            styled_lossy,
        });
    }
    None
}

/// Result of natively rendering a PlantUML diagram through the shared internal IR and
/// existing SVG renderer, without going through Mermaid text serialization.
#[derive(Debug, Clone, PartialEq)]
pub struct PlantUmlNativeRender {
    /// Original PlantUML source preserved verbatim.
    pub original: String,
    /// Rendered SVG output when the diagram is supported natively.
    pub output: Option<super::MermaidOutput>,
    /// Unsupported constructs remaining even in native IR rendering, with 1-based line numbers.
    pub unsupported: Vec<PlantUmlUnsupported>,
}

impl PlantUmlNativeRender {
    /// True when native rendering produced an output with zero unsupported constructs.
    pub fn is_lossless(&self) -> bool {
        self.output.is_some() && self.unsupported.is_empty()
    }
}

/// Natively render a PlantUML sequence diagram through the shared [`super::sequence::SeqDiagram`]
/// IR and existing sequence SVG renderer, preserving the original source verbatim and
/// reporting only constructs that remain unsupported even natively.
pub fn render_plantuml_sequence_native(source: &str) -> PlantUmlNativeRender {
    let original = source.to_string();
    let (diagram, unsupported) = parse_plantuml_sequence_ir(source);
    let output = diagram
        .as_ref()
        .and_then(super::sequence::render_seq_diagram_svg)
        .map(|(bytes, width_px, height_px)| super::MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        });
    PlantUmlNativeRender {
        original,
        output,
        unsupported,
    }
}

struct NativePendingNote {
    line: usize,
    from: usize,
    to: usize,
    pos: super::sequence::SeqNotePos,
    shape: super::sequence::SeqNoteShape,
    color: Option<String>,
    is_across: bool,
    lines: Vec<String>,
}

struct NativePendingRef {
    line: usize,
    from: usize,
    to: usize,
    lines: Vec<String>,
}

struct ParsedGate<'a> {
    target: &'a str,
    incoming: bool,
    from_left: bool,
    label: &'a str,
    dashed: bool,
    open_arrow: bool,
    cross_end: bool,
}

fn parse_gate_message(line: &str) -> Option<ParsedGate<'_>> {
    let (expr, label) = match split_colon_outside_quotes(line) {
        Some((e, l)) => (e.trim(), l.trim()),
        None => (line.trim(), ""),
    };
    // Left-edge boundary gates: `[-> A`, `[--> A`, `[->> A`, `[-->> A`, `[->x A`, `[-->x A`, `[<- A`, `[<-- A`, `[<<- A`, `[<<-- A`.
    if let Some(after_bracket) = expr.strip_prefix('[') {
        const LEFT_GATES: &[(&str, bool, bool, bool, bool)] = &[
            ("-->x", true, true, false, true),
            ("->x", true, false, false, true),
            ("-->>", true, true, true, false),
            ("->>", true, false, true, false),
            ("<<--", false, true, true, false),
            ("<<-", false, false, true, false),
            ("-->", true, true, false, false),
            ("->", true, false, false, false),
            ("<--", false, true, false, false),
            ("<-", false, false, false, false),
        ];
        for &(pat, incoming, dashed, open_arrow, cross_end) in LEFT_GATES {
            if let Some(rest) = after_bracket.strip_prefix(pat) {
                let (target_clean, _) = split_trailing_color(rest.trim());
                let (target, _, _, _, _) = strip_message_modifiers(target_clean);
                if !target.is_empty() {
                    return Some(ParsedGate {
                        target,
                        incoming,
                        from_left: true,
                        label,
                        dashed,
                        open_arrow,
                        cross_end,
                    });
                }
            }
        }
    }
    // Right-edge boundary gates: `A ->]`, `A -->]`, `A ->>]`, `A -->>]`, `A ->x]`, `A -->x]`, `A <-]`, `A <--]`, `A <<-]`, `A <<--]`.
    if let Some(before_bracket) = expr.strip_suffix(']') {
        let before_bracket = before_bracket.trim_end();
        const RIGHT_GATES: &[(&str, bool, bool, bool, bool)] = &[
            ("-->x", false, true, false, true),
            ("->x", false, false, false, true),
            ("-->>", false, true, true, false),
            ("->>", false, false, true, false),
            ("<<--", true, true, true, false),
            ("<<-", true, false, true, false),
            ("-->", false, true, false, false),
            ("->", false, false, false, false),
            ("<--", true, true, false, false),
            ("<-", true, false, false, false),
        ];
        for &(pat, incoming, dashed, open_arrow, cross_end) in RIGHT_GATES {
            if let Some(target_raw) = before_bracket.strip_suffix(pat) {
                let target = target_raw.trim();
                if !target.is_empty() {
                    return Some(ParsedGate {
                        target,
                        incoming,
                        from_left: false,
                        label,
                        dashed,
                        open_arrow,
                        cross_end,
                    });
                }
            }
        }
    }
    None
}

fn extract_trailing_color(s: &str) -> (&str, Option<String>) {
    let (clean, had_color) = split_trailing_color(s);
    if had_color {
        let color = s[clean.len()..].trim().to_string();
        (clean, (!color.is_empty()).then_some(color))
    } else {
        (s, None)
    }
}

fn extract_leading_fragment_color(rest: &str) -> (&str, Option<String>) {
    let mut s = rest.trim_start();
    let mut first_color: Option<String> = None;
    while let Some(after_hash) = s.strip_prefix('#') {
        let token_len = after_hash
            .find(char::is_whitespace)
            .unwrap_or(after_hash.len());
        let token = &after_hash[..token_len];
        if !token.is_empty()
            && token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/'))
        {
            if first_color.is_none() {
                first_color = Some(format!("#{token}"));
            }
            s = after_hash[token_len..].trim_start();
        } else {
            break;
        }
    }
    (s.trim(), first_color)
}

fn map_seq_kind(kind: ParticipantKind) -> (bool, Option<super::sequence::SeqKind>) {
    use super::sequence::SeqKind;
    match kind {
        ParticipantKind::Participant => (false, None),
        ParticipantKind::Actor => (true, None),
        ParticipantKind::Boundary => (false, Some(SeqKind::Boundary)),
        ParticipantKind::Control => (false, Some(SeqKind::Control)),
        ParticipantKind::Entity => (false, Some(SeqKind::Entity)),
        ParticipantKind::Database => (false, Some(SeqKind::Database)),
        ParticipantKind::Collections => (false, Some(SeqKind::Collections)),
        ParticipantKind::Queue => (false, Some(SeqKind::Queue)),
    }
}

fn parse_autonumber_args(arg: &str) -> Option<super::sequence::SeqAutonumberOp> {
    use super::sequence::SeqAutonumberOp;
    let arg = arg.trim();
    if arg.is_empty() {
        return Some(SeqAutonumberOp::Start { start: 1, step: 1 });
    }
    if arg.eq_ignore_ascii_case("stop") {
        return Some(SeqAutonumberOp::Stop);
    }
    if arg.eq_ignore_ascii_case("resume") {
        return Some(SeqAutonumberOp::Resume);
    }
    let parts: Vec<&str> = arg.split_whitespace().collect();
    if parts.len() == 1
        && let Ok(start) = parts[0].parse::<usize>()
    {
        return Some(SeqAutonumberOp::Start { start, step: 1 });
    }
    if parts.len() == 2
        && let (Ok(start), Ok(step)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>())
    {
        return Some(SeqAutonumberOp::Start { start, step });
    }
    None
}

fn parse_plantuml_sequence_ir(
    source: &str,
) -> (Option<super::sequence::SeqDiagram>, Vec<PlantUmlUnsupported>) {
    use super::sequence::{
        SeqAutonumberOp, SeqBox, SeqDiagram, SeqItem, SeqNotePos, SeqNoteShape, SeqParticipant,
    };

    let mut unsupported: Vec<PlantUmlUnsupported> = Vec::new();
    let mut participants: Vec<SeqParticipant> = Vec::new();
    let mut part_orders: Vec<(i32, usize, bool, usize, String)> = Vec::new();
    let mut alias_map: HashMap<String, usize> = HashMap::new();
    let mut items: Vec<SeqItem> = Vec::new();
    let mut boxes: Vec<SeqBox> = Vec::new();
    let mut blocks: Vec<(BlockKind, usize)> = Vec::new();
    let mut activation_stack: Vec<(usize, usize)> = Vec::new();
    let mut last_message: Option<(usize, usize)> = None;
    let mut last_participant: Option<usize> = None;
    let mut pending_create: Vec<usize> = Vec::new();
    let mut pending_destroy: Vec<usize> = Vec::new();
    let mut created_at: HashMap<usize, usize> = HashMap::new();
    let mut destroyed_at: HashMap<usize, usize> = HashMap::new();
    let mut across_note_indices: Vec<usize> = Vec::new();
    let mut autonumber = false;
    let mut mirror_actors = true;
    let mut title: Option<String> = None;
    let mut has_any_order = false;
    let mut saw_box = false;

    let mut pending_note: Option<NativePendingNote> = None;
    let mut pending_ref: Option<NativePendingRef> = None;
    let mut pending_title: Option<(usize, Vec<String>)> = None;
    let mut skip_until: Option<SkipUntil> = None;
    let mut in_block_comment = false;
    let mut saw_enduml = false;
    let mut non_sequence = false;

    let mut push_unsupported = |line: usize, kind: &'static str, src: &str, reason: &str| {
        unsupported.push(PlantUmlUnsupported {
            line,
            kind,
            source: excerpt(src, 120),
            reason: reason.to_string(),
        });
    };

    let ensure_participant = |raw: &str,
                              label: Option<&str>,
                              is_actor: bool,
                              kind: Option<super::sequence::SeqKind>,
                              order: Option<i32>,
                              line_no: usize,
                              raw_line: &str,
                              participants: &mut Vec<SeqParticipant>,
                              part_orders: &mut Vec<(i32, usize, bool, usize, String)>,
                              alias_map: &mut HashMap<String, usize>,
                              boxes: &mut [SeqBox],
                              in_box: bool|
     -> usize {
        let key = strip_quotes(raw.trim()).to_string();
        let idx = if let Some(&existing) = alias_map.get(&key) {
            if let Some(lbl) = label {
                let clean = sanitize_label(lbl);
                if !clean.is_empty() {
                    participants[existing].label = clean;
                }
                alias_map.entry(lbl.to_string()).or_insert(existing);
            }
            if is_actor {
                participants[existing].is_actor = true;
            }
            if kind.is_some() {
                participants[existing].kind = kind;
            }
            if let Some(ord) = order {
                part_orders[existing].0 = ord;
                part_orders[existing].2 = true;
                part_orders[existing].3 = line_no;
                part_orders[existing].4 = raw_line.to_string();
            }
            existing
        } else {
            let idx = participants.len();
            let display = sanitize_label(label.unwrap_or(&key));
            participants.push(SeqParticipant {
                _id: key.clone(),
                label: if display.is_empty() { key.clone() } else { display },
                is_actor,
                kind,
                links: Vec::new(),
            });
            part_orders.push((
                order.unwrap_or(0),
                idx,
                order.is_some(),
                line_no,
                raw_line.to_string(),
            ));
            alias_map.insert(key, idx);
            if let Some(lbl) = label {
                alias_map.entry(lbl.to_string()).or_insert(idx);
            }
            idx
        };
        if in_box
            && let Some(seq_box) = boxes.last_mut()
            && !seq_box.members.contains(&idx)
        {
            seq_box.members.push(idx);
        }
        idx
    };

    for (idx, raw_line) in source.lines().enumerate() {
        let line_no = idx + 1;
        if saw_enduml {
            continue;
        }

        let cleaned = strip_block_comments(raw_line, &mut in_block_comment);
        let trimmed = cleaned.trim();

        if let Some(mode) = skip_until {
            let lower = trimmed.to_ascii_lowercase();
            let done = match mode {
                SkipUntil::MacroProc => {
                    lower.starts_with("!endprocedure") || lower.starts_with("!endfunction")
                }
                SkipUntil::StyleTag => lower.contains("</style>"),
                SkipUntil::BraceBlock => trimmed == "}" || trimmed.ends_with('}'),
                SkipUntil::RefBlock => lower == "end ref" || lower == "endref",
                SkipUntil::TitleBlock => lower == "end title" || lower == "endtitle",
                SkipUntil::EndKeyword(end_kw) => lower == end_kw,
            };
            if mode == SkipUntil::TitleBlock {
                if done {
                    if let Some((_, lines)) = pending_title.take() {
                        let joined = sanitize_label(&lines.join(" / "));
                        if !joined.is_empty() {
                            title = Some(joined);
                        }
                    }
                    skip_until = None;
                } else if let Some((_, lines)) = pending_title.as_mut()
                    && !trimmed.is_empty()
                {
                    lines.push(trimmed.to_string());
                }
            } else if done {
                skip_until = None;
            }
            continue;
        }

        if let Some(mut note) = pending_note.take() {
            let lower = trimmed.to_ascii_lowercase();
            if matches!(
                lower.as_str(),
                "end note" | "endnote" | "end hnote" | "end rnote"
            ) {
                let text = sanitize_label(&note.lines.join(" / "));
                if note.is_across {
                    across_note_indices.push(items.len());
                }
                items.push(SeqItem::Note {
                    from: note.from,
                    to: note.to,
                    pos: note.pos,
                    text,
                    shape: note.shape,
                    color: note.color,
                });
            } else {
                if !trimmed.is_empty() {
                    note.lines.push(trimmed.to_string());
                }
                pending_note = Some(note);
            }
            continue;
        }

        if let Some(mut r) = pending_ref.take() {
            let lower = trimmed.to_ascii_lowercase();
            if lower == "end ref" || lower == "endref" {
                let label = sanitize_label(&r.lines.join(" / "));
                items.push(SeqItem::Ref {
                    from: r.from,
                    to: r.to,
                    label,
                });
            } else {
                if !trimmed.is_empty() {
                    r.lines.push(trimmed.to_string());
                }
                pending_ref = Some(r);
            }
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with('\'') {
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        let in_box = matches!(blocks.last(), Some((BlockKind::Box, _)));

        if lower.starts_with("@start") {
            let directive = lower.split_whitespace().next().unwrap_or("");
            if directive != "@startuml" {
                non_sequence = true;
                push_unsupported(
                    line_no,
                    "diagram-type",
                    trimmed,
                    "Only @startuml sequence diagrams are supported in Phase 1 native rendering.",
                );
            }
            continue;
        }
        if lower.starts_with("@end") {
            saw_enduml = true;
            continue;
        }

        // Preprocessor macros
        if trimmed.starts_with('!') {
            push_unsupported(
                line_no,
                "macro",
                trimmed,
                "PlantUML preprocessor directives and macros are not expanded.",
            );
            if lower.starts_with("!procedure ")
                || lower.starts_with("!unquoted procedure ")
                || lower.starts_with("!function ")
                || lower.starts_with("!unquoted function ")
            {
                skip_until = Some(SkipUntil::MacroProc);
            }
            continue;
        }
        if trimmed.starts_with('$') {
            push_unsupported(
                line_no,
                "macro",
                trimmed,
                "PlantUML macro invocations are not expanded.",
            );
            continue;
        }

        if is_timing_statement(trimmed, &lower) {
            non_sequence = true;
            push_unsupported(
                line_no,
                "timing",
                trimmed,
                "PlantUML timing diagrams are outside the shared sequence renderer scope.",
            );
            continue;
        }
        if is_non_sequence_uml(trimmed, &lower) {
            non_sequence = true;
            push_unsupported(
                line_no,
                "diagram-type",
                trimmed,
                "Non-sequence PlantUML diagram syntax is not handled by the sequence IR parser.",
            );
            continue;
        }

        // Styling / layout
        if lower.starts_with("skinparam") {
            push_unsupported(
                line_no,
                "styling",
                trimmed,
                "skinparam styling directives are not applied by the native renderer.",
            );
            if trimmed.ends_with('{') {
                skip_until = Some(SkipUntil::BraceBlock);
            }
            continue;
        }
        if lower.starts_with("<style>") {
            push_unsupported(
                line_no,
                "styling",
                trimmed,
                "<style> blocks are not applied by the native renderer.",
            );
            if !lower.contains("</style>") {
                skip_until = Some(SkipUntil::StyleTag);
            }
            continue;
        }
        if lower == "hide footbox" {
            mirror_actors = false;
            continue;
        }
        if lower.starts_with("hide ") || lower.starts_with("show ") {
            push_unsupported(
                line_no,
                "styling",
                trimmed,
                "hide/show directives other than `hide footbox` are not supported.",
            );
            continue;
        }
        if lower.starts_with("scale ")
            || lower == "newpage"
            || lower.starts_with("newpage ")
            || lower.starts_with("autoactivate ")
            || lower.starts_with("caption ")
        {
            push_unsupported(
                line_no,
                "layout",
                trimmed,
                "Layout/pagination directive is not supported.",
            );
            continue;
        }
        if let Some(block_end) = multi_line_unsupported_block(&lower) {
            push_unsupported(
                line_no,
                "layout",
                trimmed,
                "header/footer/legend blocks are not supported.",
            );
            if !trimmed.contains(' ') || lower == "legend" || lower == "header" || lower == "footer"
            {
                skip_until = Some(SkipUntil::EndKeyword(block_end));
            }
            continue;
        }
        if trimmed == "|||"
            || (trimmed.starts_with("||") && trimmed.ends_with("||") && trimmed.len() >= 4)
        {
            push_unsupported(
                line_no,
                "layout",
                trimmed,
                "Explicit vertical spacing (|||) is not supported.",
            );
            continue;
        }
        if trimmed.starts_with("& ") {
            push_unsupported(
                line_no,
                "layout",
                trimmed,
                "Teoz parallel prefix (&) is not supported.",
            );
            continue;
        }

        // Native divider: `== Title ==`
        if trimmed.starts_with("==") && trimmed.ends_with("==") && trimmed.len() >= 4 {
            let label = sanitize_label(trimmed.trim_matches('=').trim());
            items.push(SeqItem::Divider { label });
            continue;
        }

        // Native delay: `...` or `... 5 minutes later ...`
        if trimmed.starts_with("...") && trimmed.ends_with("...") {
            let inner = if trimmed.len() > 6 {
                trimmed[3..trimmed.len() - 3].trim()
            } else {
                ""
            };
            items.push(SeqItem::Delay {
                label: sanitize_label(inner),
            });
            continue;
        }

        // Native interaction-use frame: `ref over A, B : label` or multi-line `ref over A, B\n...\nend ref`
        if let Some(rest) = strip_prefix_ci(trimmed, "ref over ") {
            let (targets_raw, inline_label) = match split_colon_outside_quotes(rest) {
                Some((t, l)) => (t.trim(), Some(sanitize_label(l))),
                None => (rest.trim(), None),
            };
            let targets: Vec<&str> = split_all_commas_outside_quotes(targets_raw)
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect();
            if targets.is_empty() {
                push_unsupported(line_no, "syntax", trimmed, "`ref over` requires at least one participant.");
                continue;
            }
            let mut resolved = Vec::with_capacity(targets.len());
            for t in targets {
                resolved.push(ensure_participant(
                    t,
                    None,
                    false,
                    None,
                    None,
                    line_no,
                    trimmed,
                    &mut participants,
                    &mut part_orders,
                    &mut alias_map,
                    &mut boxes,
                    in_box,
                ));
            }
            let from = resolved[0];
            let to = *resolved.last().unwrap();
            match inline_label {
                Some(label) => items.push(SeqItem::Ref { from, to, label }),
                None => {
                    pending_ref = Some(NativePendingRef {
                        line: line_no,
                        from,
                        to,
                        lines: Vec::new(),
                    });
                }
            }
            continue;
        }

        // Native `autonumber [start] [step]`, `stop`, `resume`
        if lower == "autonumber" || lower.starts_with("autonumber ") {
            let arg = trimmed.get(10..).unwrap_or("").trim();
            if let Some(op) = parse_autonumber_args(arg) {
                if items.is_empty() && matches!(op, SeqAutonumberOp::Start { start: 1, step: 1 }) {
                    autonumber = true;
                } else {
                    items.push(SeqItem::Autonumber(op));
                }
            } else {
                items.push(SeqItem::Autonumber(SeqAutonumberOp::Start {
                    start: 1,
                    step: 1,
                }));
                push_unsupported(
                    line_no,
                    "autonumber",
                    trimmed,
                    "Custom autonumber format string is simplified to numeric autonumber.",
                );
            }
            continue;
        }

        // `url of <who> is [[<url> <title>]]`
        if let Some((who_raw, url, link_title)) = parse_participant_url(trimmed) {
            let pid = ensure_participant(
                who_raw,
                None,
                false,
                None,
                None,
                line_no,
                trimmed,
                &mut participants,
                &mut part_orders,
                &mut alias_map,
                &mut boxes,
                in_box,
            );
            let label = sanitize_label(link_title.unwrap_or(url));
            if !label.is_empty() {
                participants[pid].links.push(label);
            }
            continue;
        }

        // `title`
        if lower == "title" {
            pending_title = Some((line_no, Vec::new()));
            skip_until = Some(SkipUntil::TitleBlock);
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "title ") {
            let t = sanitize_label(rest.trim());
            if !t.is_empty() {
                title = Some(t);
            }
            continue;
        }

        // `box ...` / `end box`
        if lower == "end box" || lower == "endbox" {
            if matches!(blocks.last(), Some((BlockKind::Box, _))) {
                blocks.pop();
            } else {
                push_unsupported(
                    line_no,
                    "syntax",
                    trimmed,
                    "Unexpected `end box` without a matching `box`.",
                );
            }
            continue;
        }
        if lower == "box" || lower.starts_with("box ") {
            saw_box = true;
            let rest = trimmed.get(3..).unwrap_or("").trim();
            let (box_title, _color) = parse_box_args(rest);
            boxes.push(SeqBox {
                title: sanitize_label(&box_title),
                members: Vec::new(),
            });
            blocks.push((BlockKind::Box, line_no));
            continue;
        }

        // Participant declarations
        if let Some(decl) = parse_participant_declaration(trimmed) {
            if decl.had_styling {
                push_unsupported(
                    line_no,
                    "styling",
                    trimmed,
                    "Participant color or stereotype is not preserved.",
                );
            }
            let (is_actor, kind) = map_seq_kind(decl.kind);
            if decl.order.is_some() {
                if decl.is_create || in_box {
                    push_unsupported(
                        line_no,
                        "layout",
                        trimmed,
                        "Participant `order` is not preserved inside `box` or on `create`.",
                    );
                } else {
                    has_any_order = true;
                }
            }
            let pid = ensure_participant(
                &decl.alias,
                decl.label.as_deref(),
                is_actor,
                kind,
                if decl.is_create || in_box { None } else { decl.order },
                line_no,
                trimmed,
                &mut participants,
                &mut part_orders,
                &mut alias_map,
                &mut boxes,
                in_box,
            );
            if decl.is_create {
                pending_create.push(pid);
            }
            last_participant = Some(pid);
            continue;
        }

        // Bare `create <who>`
        if let Some(rest) = strip_prefix_ci(trimmed, "create ") {
            let pid = ensure_participant(
                rest.trim(),
                None,
                false,
                None,
                None,
                line_no,
                trimmed,
                &mut participants,
                &mut part_orders,
                &mut alias_map,
                &mut boxes,
                in_box,
            );
            pending_create.push(pid);
            last_participant = Some(pid);
            continue;
        }

        // `activate` / `deactivate` / `destroy`
        if let Some(rest) = strip_prefix_ci(trimmed, "activate ") {
            let (who_raw, had_color) = split_trailing_color(rest.trim());
            if had_color {
                push_unsupported(
                    line_no,
                    "styling",
                    trimmed,
                    "Activation bar color is not preserved.",
                );
            }
            let pid = ensure_participant(
                who_raw,
                None,
                false,
                None,
                None,
                line_no,
                trimmed,
                &mut participants,
                &mut part_orders,
                &mut alias_map,
                &mut boxes,
                in_box,
            );
            if let Some((from, to)) = last_message
                && to == pid
            {
                activation_stack.push((from, pid));
            }
            items.push(SeqItem::Activate(pid));
            last_participant = Some(pid);
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "deactivate ") {
            let pid = ensure_participant(
                rest.trim(),
                None,
                false,
                None,
                None,
                line_no,
                trimmed,
                &mut participants,
                &mut part_orders,
                &mut alias_map,
                &mut boxes,
                in_box,
            );
            if let Some(pos) = activation_stack.iter().rposition(|&(_, callee)| callee == pid) {
                activation_stack.remove(pos);
            }
            items.push(SeqItem::Deactivate(pid));
            continue;
        }
        if let Some(rest) = strip_prefix_ci(trimmed, "destroy ") {
            let pid = ensure_participant(
                rest.trim(),
                None,
                false,
                None,
                None,
                line_no,
                trimmed,
                &mut participants,
                &mut part_orders,
                &mut alias_map,
                &mut boxes,
                in_box,
            );
            pending_destroy.push(pid);
            continue;
        }

        // `return [label]`
        if lower == "return" || lower.starts_with("return ") {
            let label = sanitize_label(trimmed.get(6..).unwrap_or("").trim());
            if let Some((caller, callee)) = activation_stack.pop() {
                items.push(SeqItem::Message {
                    from: callee,
                    to: caller,
                    label,
                    dashed: true,
                    open_arrow: false,
                    cross_end: false,
                    bidirectional: false,
                    circle_from: false,
                    circle_to: false,
                    activate_from: false,
                    deactivate_from: true,
                    activate_to: false,
                    deactivate_to: false,
                });
                last_message = Some((callee, caller));
                last_participant = Some(caller);
            } else {
                push_unsupported(
                    line_no,
                    "syntax",
                    trimmed,
                    "`return` requires a preceding activation (`++` or `activate`).",
                );
            }
            continue;
        }

        // Combined fragments (`alt`, `opt`, `loop`, `par`, `break`, `critical`, native `group`, with optional `#Color`)
        if let Some((kind, _, _)) = parse_fragment_start(trimmed) {
            let kw_str = match kind {
                BlockKind::Alt => "alt",
                BlockKind::Opt => "opt",
                BlockKind::Loop => "loop",
                BlockKind::Par => "par",
                BlockKind::Break => "break",
                BlockKind::Critical => "critical",
                BlockKind::Group => "group",
                BlockKind::Box => unreachable!(),
            };
            let rest = trimmed.get(kw_str.len()..).unwrap_or("");
            let (label_clean, color) = extract_leading_fragment_color(rest);
            items.push(SeqItem::BlockStart {
                kind: kw_str.to_string(),
                label: sanitize_label(label_clean),
                color,
            });
            blocks.push((kind, line_no));
            continue;
        }
        if lower == "else" || lower.starts_with("else ") || lower.starts_with("else#") {
            let (label_clean, had_color) =
                strip_leading_fragment_colors(trimmed.get(4..).unwrap_or(""));
            if had_color {
                push_unsupported(
                    line_no,
                    "styling",
                    trimmed,
                    "Branch-specific background color on `else` is not preserved.",
                );
            }
            if matches!(
                blocks.last().map(|(k, _)| *k),
                Some(
                    BlockKind::Alt
                        | BlockKind::Par
                        | BlockKind::Critical
                        | BlockKind::Opt
                        | BlockKind::Group
                )
            ) {
                items.push(SeqItem::BlockElse {
                    label: sanitize_label(label_clean),
                });
            } else {
                push_unsupported(
                    line_no,
                    "syntax",
                    trimmed,
                    "Unexpected `else` outside an open fragment.",
                );
            }
            continue;
        }
        if (lower == "option" || lower.starts_with("option "))
            && matches!(blocks.last(), Some((BlockKind::Critical, _)))
        {
            items.push(SeqItem::BlockElse {
                label: sanitize_label(trimmed.get(6..).unwrap_or("").trim()),
            });
            continue;
        }
        if lower == "end" {
            if let Some((kind, _)) = blocks.pop() {
                if kind == BlockKind::Box {
                    push_unsupported(
                        line_no,
                        "syntax",
                        trimmed,
                        "Closed `box` with `end` instead of `end box`.",
                    );
                } else {
                    items.push(SeqItem::BlockEnd);
                }
            } else {
                push_unsupported(
                    line_no,
                    "syntax",
                    trimmed,
                    "Unexpected `end` without an open fragment.",
                );
            }
            continue;
        }

        // Native notes (`note`, `hnote`, `rnote`, `/ note ...`, including `note across`)
        let (aligned_note, note_candidate) = match trimmed.strip_prefix("/ ") {
            Some(rest) => (true, rest.trim_start()),
            None => (false, trimmed),
        };
        let note_prefix = if let Some(r) = strip_prefix_ci(note_candidate, "note ") {
            Some((SeqNoteShape::Rect, r.trim_start()))
        } else if let Some(r) = strip_prefix_ci(note_candidate, "hnote ") {
            Some((SeqNoteShape::Hexagon, r.trim_start()))
        } else if let Some(r) = strip_prefix_ci(note_candidate, "rnote ") {
            Some((SeqNoteShape::Box, r.trim_start()))
        } else {
            None
        };
        if let Some((shape, rest)) = note_prefix {
            if aligned_note {
                push_unsupported(
                    line_no,
                    "layout",
                    trimmed,
                    "Aligned side-by-side note prefix (/) is rendered sequentially.",
                );
            }
            let (target_raw, inline_text) = match split_colon_outside_quotes(rest) {
                Some((t, text)) => (t.trim(), Some(sanitize_label(text))),
                None => (rest.trim(), None),
            };
            let (target_clean, color) = extract_trailing_color(target_raw);
            let resolved_target = if let Some(who) = strip_prefix_ci(target_clean, "left of ") {
                let id = ensure_participant(
                    who.trim(),
                    None,
                    false,
                    None,
                    None,
                    line_no,
                    trimmed,
                    &mut participants,
                    &mut part_orders,
                    &mut alias_map,
                    &mut boxes,
                    in_box,
                );
                Some((id, id, SeqNotePos::Left, false))
            } else if let Some(who) = strip_prefix_ci(target_clean, "right of ") {
                let id = ensure_participant(
                    who.trim(),
                    None,
                    false,
                    None,
                    None,
                    line_no,
                    trimmed,
                    &mut participants,
                    &mut part_orders,
                    &mut alias_map,
                    &mut boxes,
                    in_box,
                );
                Some((id, id, SeqNotePos::Right, false))
            } else if let Some(who) = strip_prefix_ci(target_clean, "over ") {
                let targets: Vec<&str> = split_all_commas_outside_quotes(who.trim())
                    .into_iter()
                    .filter(|p| !p.is_empty())
                    .collect();
                if targets.is_empty() {
                    None
                } else {
                    let mut ids = Vec::with_capacity(targets.len());
                    for part in targets {
                        ids.push(ensure_participant(
                            part,
                            None,
                            false,
                            None,
                            None,
                            line_no,
                            trimmed,
                            &mut participants,
                            &mut part_orders,
                            &mut alias_map,
                            &mut boxes,
                            in_box,
                        ));
                    }
                    Some((ids[0], *ids.last().unwrap(), SeqNotePos::Over, false))
                }
            } else if target_clean.eq_ignore_ascii_case("left") {
                last_participant.map(|id| (id, id, SeqNotePos::Left, false))
            } else if target_clean.eq_ignore_ascii_case("right") {
                last_participant.map(|id| (id, id, SeqNotePos::Right, false))
            } else if target_clean.eq_ignore_ascii_case("across") {
                Some((0, 0, SeqNotePos::Over, true))
            } else {
                None
            };
            let Some((from, to, pos, is_across)) = resolved_target else {
                push_unsupported(
                    line_no,
                    "syntax",
                    trimmed,
                    "Note target could not be resolved.",
                );
                continue;
            };
            match inline_text {
                Some(text) => {
                    if is_across {
                        across_note_indices.push(items.len());
                    }
                    items.push(SeqItem::Note {
                        from,
                        to,
                        pos,
                        text,
                        shape,
                        color,
                    });
                }
                None => {
                    pending_note = Some(NativePendingNote {
                        line: line_no,
                        from,
                        to,
                        pos,
                        shape,
                        color,
                        is_across,
                        lines: Vec::new(),
                    });
                }
            }
            continue;
        }

        // Native boundary gates (`[-> A`, `A ->]`, `[<- A`, `A <-]`)
        if let Some(gate) = parse_gate_message(trimmed) {
            let target = ensure_participant(
                gate.target,
                None,
                false,
                None,
                None,
                line_no,
                trimmed,
                &mut participants,
                &mut part_orders,
                &mut alias_map,
                &mut boxes,
                in_box,
            );
            items.push(SeqItem::Gate {
                target,
                incoming: gate.incoming,
                from_left: gate.from_left,
                label: sanitize_label(gate.label),
                dashed: gate.dashed,
                open_arrow: gate.open_arrow,
                cross_end: gate.cross_end,
            });
            last_participant = Some(target);
            continue;
        }

        // Messages
        if let Some(msg) = parse_message_line(trimmed) {
            if msg.styled_lossy {
                push_unsupported(
                    line_no,
                    "styling",
                    trimmed,
                    "Arrow color or thickness styling is not preserved.",
                );
            }
            if msg.activation_colored {
                push_unsupported(
                    line_no,
                    "styling",
                    trimmed,
                    "Shorthand activation color is not preserved.",
                );
            }
            let (raw_from, raw_to) = if msg.reverse {
                (msg.rhs, msg.lhs)
            } else {
                (msg.lhs, msg.rhs)
            };
            let from = ensure_participant(
                raw_from,
                None,
                false,
                None,
                None,
                line_no,
                trimmed,
                &mut participants,
                &mut part_orders,
                &mut alias_map,
                &mut boxes,
                in_box,
            );
            let to = ensure_participant(
                raw_to,
                None,
                false,
                None,
                None,
                line_no,
                trimmed,
                &mut participants,
                &mut part_orders,
                &mut alias_map,
                &mut boxes,
                in_box,
            );
            if msg.create_target {
                pending_create.push(to);
            }
            if msg.destroy_target {
                pending_destroy.push(to);
            }
            let msg_idx = items.len();
            for who in [to, from] {
                if let Some(pos) = pending_create.iter().position(|&p| p == who) {
                    pending_create.remove(pos);
                    created_at.insert(who, msg_idx);
                    break;
                }
            }
            for who in [from, to] {
                if let Some(pos) = pending_destroy.iter().position(|&p| p == who) {
                    pending_destroy.remove(pos);
                    destroyed_at.insert(who, msg_idx);
                }
            }
            let (dashed, open_arrow, cross_end, bidirectional) = match msg.arrow {
                "<<-->>" => (true, false, false, true),
                "<<->>" => (false, false, false, true),
                "-->>" => (true, false, false, false),
                "->>" => (false, false, false, false),
                "-->" | "--)" => (true, true, false, false),
                "->" | "-)" => (false, true, false, false),
                "--x" => (true, false, true, false),
                "-x" => (false, false, true, false),
                _ => (false, false, false, false),
            };
            if msg.deactivate_source
                && let Some(pos) = activation_stack
                    .iter()
                    .rposition(|&(_, callee)| callee == from)
            {
                activation_stack.remove(pos);
            }
            if msg.activate_target {
                activation_stack.push((from, to));
            }
            items.push(SeqItem::Message {
                from,
                to,
                label: sanitize_label(msg.label),
                dashed,
                open_arrow,
                cross_end,
                bidirectional,
                circle_from: msg.circle_source,
                circle_to: msg.circle_target,
                activate_from: false,
                deactivate_from: msg.deactivate_source,
                activate_to: msg.activate_target,
                deactivate_to: false,
            });
            last_message = Some((from, to));
            last_participant = Some(to);
            continue;
        }

        push_unsupported(
            line_no,
            "syntax",
            trimmed,
            "Unsupported PlantUML sequence syntax.",
        );
    }

    if let Some(note) = pending_note {
        push_unsupported(
            note.line,
            "syntax",
            "note",
            "Unclosed multi-line note (missing `end note`).",
        );
    }
    if let Some(r) = pending_ref {
        push_unsupported(
            r.line,
            "syntax",
            "ref over",
            "Unclosed multi-line `ref` block (missing `end ref`).",
        );
    }
    if let Some((line_no, _)) = pending_title {
        push_unsupported(
            line_no,
            "syntax",
            "title",
            "Unclosed multi-line `title` block (missing `end title`).",
        );
    }
    while let Some((kind, open_line)) = blocks.pop() {
        let label = match kind {
            BlockKind::Box => "box",
            BlockKind::Alt => "alt",
            BlockKind::Opt => "opt",
            BlockKind::Loop => "loop",
            BlockKind::Par => "par",
            BlockKind::Break => "break",
            BlockKind::Critical => "critical",
            BlockKind::Group => "group",
        };
        push_unsupported(
            open_line,
            "syntax",
            label,
            "Unclosed fragment or box block (missing `end`).",
        );
        if kind != BlockKind::Box {
            items.push(SeqItem::BlockEnd);
        }
    }

    if non_sequence || participants.is_empty() {
        return (None, unsupported);
    }

    // Apply top-level participant `order N` stably when no `box` groups prevent reordering.
    if has_any_order {
        if saw_box {
            for &(_, _, had_order, line_no, ref raw) in &part_orders {
                if had_order {
                    push_unsupported(
                        line_no,
                        "layout",
                        raw,
                        "Participant `order` cannot be reordered when `box` groups are present.",
                    );
                }
            }
        } else {
            let mut perm: Vec<usize> = (0..participants.len()).collect();
            perm.sort_by_key(|&i| (part_orders[i].0, part_orders[i].1));
            let mut old_to_new = vec![0usize; participants.len()];
            for (new_idx, &old_idx) in perm.iter().enumerate() {
                old_to_new[old_idx] = new_idx;
            }
            let old_parts = std::mem::take(&mut participants);
            let mut slots: Vec<Option<SeqParticipant>> = old_parts.into_iter().map(Some).collect();
            participants = perm
                .into_iter()
                .map(|old_idx| slots[old_idx].take().unwrap())
                .collect();
            for item in &mut items {
                match item {
                    SeqItem::Message { from, to, .. }
                    | SeqItem::Note { from, to, .. }
                    | SeqItem::Ref { from, to, .. } => {
                        *from = old_to_new[*from];
                        *to = old_to_new[*to];
                    }
                    SeqItem::Activate(who)
                    | SeqItem::Deactivate(who)
                    | SeqItem::Gate { target: who, .. } => {
                        *who = old_to_new[*who];
                    }
                    _ => {}
                }
            }
            created_at = created_at
                .into_iter()
                .map(|(k, v)| (old_to_new[k], v))
                .collect();
            destroyed_at = destroyed_at
                .into_iter()
                .map(|(k, v)| (old_to_new[k], v))
                .collect();
        }
    }

    // Resolve `note across` now that all participants (and final ordering) are known.
    let last_col = participants.len().saturating_sub(1);
    for idx in across_note_indices {
        if let Some(SeqItem::Note { from, to, .. }) = items.get_mut(idx) {
            *from = 0;
            *to = last_col;
        }
    }

    (
        Some(SeqDiagram {
            participants,
            items,
            autonumber,
            mirror_actors,
            boxes,
            title,
            created_at,
            destroyed_at,
        }),
        unsupported,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::{MermaidOutput, render_mermaid};

    #[test]
    fn converts_basic_sequence_and_renders_with_existing_mermaid_renderer() {
        let puml = "\
@startuml
autonumber
title ログイン処理
actor \"利用者\" as U
participant \"API サーバー\" as API
database AuthDB
U -> API ++ : ログイン要求
API -> AuthDB : 認証情報照会
AuthDB --> API : 照会結果
alt 認証成功
  API --> U -- : 200 OK
  note right of U : セッション開始
else 認証失敗
  API -->x U : 401 Unauthorized
end
note over U, API
  完了ログを記録
  監査対象
end note
@enduml
";
        let conv = convert_plantuml_sequence(puml);
        assert_eq!(conv.original, puml);
        assert!(conv.is_lossless(), "unexpected diagnostics: {:?}", conv.unsupported);
        let mermaid = conv.mermaid.as_deref().expect("expected mermaid output");
        assert!(mermaid.starts_with("sequenceDiagram\n"), "{mermaid}");
        assert!(mermaid.contains("actor U as 利用者"), "{mermaid}");
        assert!(mermaid.contains("participant API as API サーバー"), "{mermaid}");
        assert!(mermaid.contains(r#"participant AuthDB@{ "type": "database" }"#), "{mermaid}");
        assert!(mermaid.contains("U->>+API: ログイン要求"), "{mermaid}");
        assert!(mermaid.contains("API-->>-U: 200 OK"), "{mermaid}");
        assert!(mermaid.contains("API--xU: 401 Unauthorized"), "{mermaid}");
        assert!(mermaid.contains("Note over U,API: 完了ログを記録 / 監査対象"), "{mermaid}");

        let Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }) = render_mermaid(mermaid)
        else {
            panic!("existing Mermaid renderer should render converted sequenceDiagram:\n{mermaid}");
        };
        assert!(width_px > 200 && height_px > 200);
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg).is_ok(), "invalid SVG");
        assert!(svg.contains("ログイン処理"), "{svg}");
        assert!(svg.contains("利用者"), "{svg}");
        assert!(svg.contains("API サーバー"), "{svg}");
        assert!(svg.contains(r#"data-shape="seq-database""#), "{svg}");
        assert!(svg.contains("ログイン要求"), "{svg}");
        assert!(svg.contains("200 OK"), "{svg}");
        assert!(svg.contains("401 Unauthorized"), "{svg}");
        assert!(svg.contains("完了ログを記録 / 監査対象"), "{svg}");
    }

    #[test]
    fn converts_left_arrows_par_box_return_and_quoted_participants() {
        let puml = "\
@startuml
box \"外部連携\"
  participant \"Web UI\" as UI
  queue Events
end box
\"Web UI\" -> Events ++ : publish
Events <- \"Web UI\" : ack
par 並列処理
  UI ->> Events : task A
else 並列B
  UI -->> Events : task B
end
return done
@enduml
";
        let conv = convert_plantuml_sequence(puml);
        assert!(conv.is_lossless(), "unexpected diagnostics: {:?}", conv.unsupported);
        let mermaid = conv.mermaid.as_deref().unwrap();
        assert!(mermaid.contains("box 外部連携"), "{mermaid}");
        assert!(mermaid.contains("UI->>+Events: publish"), "{mermaid}");
        assert!(mermaid.contains("UI->>Events: ack"), "{mermaid}");
        assert!(mermaid.contains("par 並列処理"), "{mermaid}");
        assert!(mermaid.contains("and 並列B"), "{mermaid}");
        assert!(mermaid.contains("Events-->>-UI: done"), "{mermaid}");
        let Some(MermaidOutput::Svg { bytes, .. }) = render_mermaid(mermaid) else {
            panic!("failed to render:\n{mermaid}");
        };
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg).is_ok());
        assert!(svg.contains("外部連携"), "{svg}");
        assert!(svg.contains(r#"data-shape="seq-queue""#), "{svg}");
    }

    #[test]
    fn preserves_original_and_reports_unsupported_lines_with_line_numbers() {
        let puml = "\
@startuml
!include common.puml
skinparam backgroundColor #EEEBDC
Alice -> Bob : request
== 後半 ==
... 5分後 ...
ref over Alice, Bob : 共通エラー処理
Bob --> Alice : response
@enduml
";
        let conv = convert_plantuml_sequence(puml);
        assert_eq!(conv.original, puml);
        assert!(!conv.is_lossless());
        let mermaid = conv.mermaid.as_deref().expect("partial sequence still converts");
        assert!(render_mermaid(mermaid).is_some());

        let reported: Vec<(usize, &str)> = conv
            .unsupported
            .iter()
            .map(|u| (u.line, u.kind))
            .collect();
        assert_eq!(
            reported,
            vec![
                (2, "macro"),
                (3, "styling"),
                (5, "divider"),
                (6, "delay"),
                (7, "reference"),
            ]
        );
    }

    #[test]
    fn rejects_timing_and_other_diagram_types_with_line_numbers() {
        let timing = "\
@startuml
robust \"Web Browser\" as WB
concise \"Web User\" as WU
@0
WU is Idle
WB is Idle
@100
WU is Waiting
@enduml
";
        let conv = convert_plantuml_sequence(timing);
        assert_eq!(conv.original, timing);
        assert!(conv.mermaid.is_none());
        assert!(conv.unsupported.iter().any(|u| u.line == 2 && u.kind == "timing"));
        assert!(conv.unsupported.iter().any(|u| u.line == 4 && u.kind == "timing"));

        let class_diagram = "\
@startuml
class Order {
  id: String
}
@enduml
";
        let conv = convert_plantuml_sequence(class_diagram);
        assert_eq!(conv.original, class_diagram);
        assert!(conv.mermaid.is_none());
        assert!(conv.unsupported.iter().any(|u| u.line == 2 && u.kind == "diagram-type"));
    }

    #[test]
    fn handles_block_comments_empty_labels_and_unclosed_blocks() {
        let puml = "\
@startuml
/' multi-line
   comment '/
Alice -> Bob
opt
  Bob --> Alice : ok
@enduml
";
        let conv = convert_plantuml_sequence(puml);
        assert_eq!(conv.original, puml);
        let mermaid = conv.mermaid.as_deref().unwrap();
        assert!(mermaid.contains("Alice->>Bob: "), "{mermaid}");
        assert!(mermaid.contains("opt -"), "{mermaid}");
        assert!(render_mermaid(mermaid).is_some());
        assert_eq!(conv.unsupported.len(), 1);
        assert_eq!(conv.unsupported[0].line, 5);
        assert_eq!(conv.unsupported[0].kind, "syntax");
    }

    #[test]
    fn converts_all_participant_kinds_hyphenated_names_and_special_arrows() {
        let puml = "\
@startuml
boundary \"Web UI\" as UI
control Controller
entity User
collections Cache
queue Jobs
api-x -> auth-svc : verify
auth-svc <-> Controller : sync
Controller ->o Jobs : enqueue
Jobs -> worker ** : spawn
Controller -> worker !! : stop
note right : implicit target note
@enduml
";
        let (conv, rendered) = crate::mermaid::render_plantuml_sequence(puml);
        assert_eq!(conv.original, puml);
        assert!(conv.is_lossless(), "unexpected diagnostics: {:?}", conv.unsupported);
        let mermaid = conv.mermaid.as_deref().unwrap();
        assert!(mermaid.contains("participant api_x as api-x"), "{mermaid}");
        assert!(mermaid.contains("participant auth_svc as auth-svc"), "{mermaid}");
        assert!(mermaid.contains("api_x->>auth_svc: verify"), "{mermaid}");
        assert!(mermaid.contains("auth_svc<<->>Controller: sync"), "{mermaid}");
        assert!(mermaid.contains("Controller->>()Jobs: enqueue"), "{mermaid}");
        assert!(mermaid.contains("create participant worker"), "{mermaid}");
        assert!(mermaid.contains("destroy worker"), "{mermaid}");
        assert!(mermaid.contains("Note right of worker: implicit target note"), "{mermaid}");

        let Some(MermaidOutput::Svg { bytes, .. }) = rendered else {
            panic!("expected SVG from render_plantuml_sequence:\n{mermaid}");
        };
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg).is_ok(), "invalid SVG");
        for kind in ["boundary", "control", "entity", "collections", "queue"] {
            assert!(
                svg.contains(&format!(r#"data-shape="seq-{kind}""#)),
                "missing shape {kind}: {svg}"
            );
        }
        assert!(svg.contains("api-x") && svg.contains("auth-svc"), "{svg}");
        assert!(svg.contains(r#"data-role="message-circle""#), "{svg}");
        assert!(svg.contains(r#"data-shape="seq-destroy""#), "{svg}");
    }

    #[test]
    fn reports_macro_procedures_colored_arrows_gates_and_non_uml_envelopes() {
        let puml = "\
@startuml
!procedure $msg($a, $b)
  $a -> $b : expanded
!endprocedure
$msg(A, B)
[-> Alice : external trigger
Alice -[#red]-> Bob : alert
@enduml
";
        let conv = convert_plantuml_sequence(puml);
        assert_eq!(conv.original, puml);
        let reported: Vec<(usize, &str)> = conv
            .unsupported
            .iter()
            .map(|u| (u.line, u.kind))
            .collect();
        assert_eq!(
            reported,
            vec![(2, "macro"), (5, "macro"), (6, "syntax"), (7, "styling")]
        );
        let mermaid = conv.mermaid.as_deref().unwrap();
        assert!(mermaid.contains("Alice->>Bob: alert"), "{mermaid}");
        assert!(render_mermaid(mermaid).is_some());

        let gantt = "@startgantt\n[Task 1] lasts 4 days\n@endgantt\n";
        let conv_gantt = convert_plantuml_sequence(gantt);
        assert!(conv_gantt.mermaid.is_none());
        assert_eq!(conv_gantt.unsupported[0].line, 1);
        assert_eq!(conv_gantt.unsupported[0].kind, "diagram-type");
    }

    #[test]
    fn converts_order_links_activate_return_circle_and_half_arrows_and_multi_notes() {
        let puml = "\
@startuml
participant Last order 30
participant First order 10
participant Middle order 20
participant DefaultZero
url of First is [[https://example.com/first First Docs]]
First -> Middle : request
activate Middle
return ok
First o->o Last : ping
Last <-o Middle : ack
First -\\ Middle : async push
Middle --/ First : async reply
First -[dashed]-> Last : dashed style
note over DefaultZero, First, Last : spans three
@enduml
";
        let (conv, rendered) = crate::mermaid::render_plantuml_sequence(puml);
        assert_eq!(conv.original, puml);
        assert!(
            conv.is_lossless(),
            "unexpected diagnostics: {:?}",
            conv.unsupported
        );
        let mermaid = conv.mermaid.as_deref().unwrap();
        let pos_zero = mermaid.find("participant DefaultZero").unwrap();
        let pos_first = mermaid.find("participant First").unwrap();
        let pos_mid = mermaid.find("participant Middle").unwrap();
        let pos_last = mermaid.find("participant Last").unwrap();
        assert!(
            pos_zero < pos_first && pos_first < pos_mid && pos_mid < pos_last,
            "order N should sort top-level declarations:\n{mermaid}"
        );
        assert!(
            mermaid.contains("link First: First Docs @ https://example.com/first"),
            "{mermaid}"
        );
        assert!(mermaid.contains("activate Middle"), "{mermaid}");
        assert!(mermaid.contains("Middle-->>-First: ok"), "{mermaid}");
        assert!(mermaid.contains("First()->>()Last: ping"), "{mermaid}");
        assert!(mermaid.contains("Middle()->>Last: ack"), "{mermaid}");
        assert!(mermaid.contains("First-)Middle: async push"), "{mermaid}");
        assert!(mermaid.contains("Middle--)First: async reply"), "{mermaid}");
        assert!(mermaid.contains("First-->>Last: dashed style"), "{mermaid}");
        assert!(
            mermaid.contains("Note over DefaultZero,Last: spans three"),
            "{mermaid}"
        );

        let Some(MermaidOutput::Svg { bytes, .. }) = rendered else {
            panic!("expected SVG:\n{mermaid}");
        };
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg).is_ok(), "invalid SVG");
        assert!(svg.contains(">First Docs<"), "{svg}");
        assert!(svg.matches(r#"data-role="message-circle""#).count() >= 3, "{svg}");
        assert!(svg.contains("spans three"), "{svg}");
    }

    #[test]
    fn handles_colored_fragments_aligned_notes_autonumber_stop_and_styled_arrows() {
        let puml = "\
@startuml
autonumber
Alice -[#blue,dashed]-> Bob : retry
autonumber stop
alt#Gold #LightBlue 成功時
  Bob --> Alice : ok
else #Pink 失敗時
  Bob -->x Alice : err
end
/ note right of Bob : side note
@enduml
";
        let (conv, rendered) = crate::mermaid::render_plantuml_sequence(puml);
        assert_eq!(conv.original, puml);
        let mermaid = conv.mermaid.as_deref().unwrap();
        assert!(mermaid.contains("Alice-->>Bob: retry"), "{mermaid}");
        assert!(mermaid.contains("alt 成功時"), "{mermaid}");
        assert!(mermaid.contains("else 失敗時"), "{mermaid}");
        assert!(!mermaid.contains("#Gold") && !mermaid.contains("#LightBlue") && !mermaid.contains("#Pink"), "{mermaid}");
        assert!(mermaid.contains("Note right of Bob: side note"), "{mermaid}");
        assert_eq!(
            mermaid.matches("autonumber").count(),
            1,
            "autonumber stop must not emit another autonumber:\n{mermaid}"
        );

        let reported: Vec<(usize, &str)> = conv
            .unsupported
            .iter()
            .map(|u| (u.line, u.kind))
            .collect();
        assert_eq!(
            reported,
            vec![
                (3, "styling"),
                (4, "autonumber"),
                (5, "styling"),
                (7, "styling"),
                (10, "layout"),
            ]
        );
        assert!(rendered.is_some());
    }

    #[test]
    fn native_shared_ir_renders_dividers_delays_refs_gates_footbox_hnote_and_autonumber() {
        let puml = "\
@startuml
hide footbox
autonumber 10 5
participant Last order 30
participant First order 10
participant Middle order 20
[-> First : 外部リクエスト
First -> Middle : 処理依頼
== 検証フェーズ ==
hnote over Middle #LightYellow : 六角注記
rnote right of Middle #LightCyan : 角丸なし注記
... 5分待機 ...
autonumber stop
group #LightBlue 共通処理
  ref over First, Last
    認証トークン検証
    監査ログ出力
  end ref
end
autonumber resume
Middle -> Last : 完了通知
Last ->] : 外部コールバック
note across : 全体完了
@enduml
";
        let native = render_plantuml_sequence_native(puml);
        assert_eq!(native.original, puml);
        assert!(
            native.is_lossless(),
            "native IR should handle all constructs losslessly, got: {:?}",
            native.unsupported
        );
        let Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }) = native.output
        else {
            panic!("expected SVG from native shared IR renderer");
        };
        assert!(width_px > 250 && height_px > 300);
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(roxmltree::Document::parse(svg).is_ok(), "invalid SVG");
        // `hide footbox` omits bottom actor boxes.
        assert!(!svg.contains(r#"data-role="actor-bottom""#), "{svg}");
        // `order N` places First < Middle < Last left-to-right.
        assert!(svg.find(">First<").unwrap() < svg.find(">Middle<").unwrap());
        assert!(svg.find(">Middle<").unwrap() < svg.find(">Last<").unwrap());
        // Native PlantUML sequence elements drawn through shared `sequence.rs` renderer.
        assert!(svg.contains(r#"data-shape="seq-divider""#), "{svg}");
        assert!(svg.contains("検証フェーズ"), "{svg}");
        assert!(svg.contains(r#"data-shape="seq-delay""#), "{svg}");
        assert!(svg.contains("5分待機"), "{svg}");
        assert!(svg.contains(r#"data-shape="seq-ref""#), "{svg}");
        assert!(svg.contains("認証トークン検証 / 監査ログ出力"), "{svg}");
        assert_eq!(svg.matches(r#"data-shape="seq-gate""#).count(), 2, "{svg}");
        assert!(svg.contains("外部リクエスト") && svg.contains("外部コールバック"), "{svg}");
        assert!(svg.contains(r#"data-shape="seq-hnote""#), "{svg}");
        assert!(svg.contains(r#"data-shape="seq-rnote""#), "{svg}");
        assert!(svg.contains(">group<") && svg.contains("[共通処理]"), "{svg}");
        assert!(svg.contains("全体完了"), "{svg}");
        // Autonumber 10, 15, stopped, resumed at 20, 25.
        assert!(svg.contains(r#"data-role="seq-autonumber">10</text>"#), "{svg}");
        assert!(svg.contains(r#"data-role="seq-autonumber">15</text>"#), "{svg}");
        assert!(svg.contains(r#"data-role="seq-autonumber">20</text>"#), "{svg}");
        assert!(svg.contains(r#"data-role="seq-autonumber">25</text>"#), "{svg}");
    }

    #[test]
    fn native_shared_ir_still_reports_macros_skinparam_and_timing_with_line_numbers() {
        let puml = "\
@startuml
!include common.puml
skinparam backgroundColor #EEE
Alice -> Bob : hi
@enduml
";
        let native = render_plantuml_sequence_native(puml);
        assert!(native.output.is_some());
        let reported: Vec<(usize, &str)> = native
            .unsupported
            .iter()
            .map(|u| (u.line, u.kind))
            .collect();
        assert_eq!(reported, vec![(2, "macro"), (3, "styling")]);

        let timing = "@startuml\nrobust \"Web\" as W\n@0\nW is Idle\n@enduml\n";
        let native_timing = render_plantuml_sequence_native(timing);
        assert!(native_timing.output.is_none());
        assert!(native_timing.unsupported.iter().any(|u| u.line == 2 && u.kind == "timing"));
    }
}
