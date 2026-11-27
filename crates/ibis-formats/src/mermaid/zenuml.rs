// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use super::*;

enum Frame<'a> {
    Call {
        from: &'a str,
        to: &'a str,
    },
    Fragment {
        kind: &'static str,
        statements: usize,
    },
}

fn identifier(text: &str) -> Option<&str> {
    let text = text.trim();
    (!text.is_empty() && text.chars().all(|c| c.is_alphanumeric() || c == '_')).then_some(text)
}

fn invocation(text: &str) -> Option<(&str, &str)> {
    let (target, message) = text.split_once('.')?;
    let method = message
        .split_once('(')
        .map_or(message, |(method, _)| method);
    identifier(method)?;
    if message.contains('(') && !message.ends_with(')') {
        return None;
    }
    Some((identifier(target)?, message))
}

fn declaration(line: &str) -> Option<(&str, &str, Option<&str>)> {
    let (kind, rest) = if line.starts_with('@') {
        let (kind, rest) = line.split_once(char::is_whitespace)?;
        (kind, rest)
    } else {
        ("", line)
    };
    let (name, alias) = rest
        .split_once(" as ")
        .map_or((rest, None), |(name, alias)| (name, Some(alias.trim())));
    Some((kind, identifier(name)?, alias))
}

fn fragment(line: &str) -> Option<(&'static str, &str)> {
    let label = |prefix| {
        line.strip_prefix(prefix)
            .map(str::trim)
            .and_then(|text| text.strip_prefix('(')?.strip_suffix(')'))
    };
    if let Some(condition) = label("if") {
        return Some(("alt", condition));
    }
    if let Some(condition) = label("while").or_else(|| label("for")) {
        return Some(("loop", condition));
    }
    match line {
        "opt" => Some(("opt", "optional")),
        "par" => Some(("par", "parallel")),
        "try" => Some(("critical", "try")),
        _ => None,
    }
}

fn sequence(source: &str) -> Option<String> {
    let mut origin = String::from("__zen_origin");
    while source.contains(&origin) {
        origin.push('_');
    }
    let mut out = String::from("sequenceDiagram\n");
    let mut frames = Vec::new();
    let mut comments = Vec::new();
    let mut reply = false;
    let mut has_message = false;
    for line in source.lines().skip(1).map(str::trim) {
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if let Some(comment) = line.strip_prefix("//") {
            comments.push(comment.trim());
            continue;
        }
        if line.starts_with("title ")
            || line.starts_with("accTitle:")
            || line.starts_with("accDescr:")
        {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if matches!(line, "@return" | "@reply") {
            reply = true;
            continue;
        }
        if let Some(tail) = line.strip_prefix('}') {
            match (frames.last()?, tail.trim()) {
                (Frame::Fragment { kind: "alt", .. }, "else {") => out.push_str("else otherwise\n"),
                (
                    Frame::Fragment {
                        kind: "critical", ..
                    },
                    "catch {",
                ) => out.push_str("option catch\n"),
                (
                    Frame::Fragment {
                        kind: "critical", ..
                    },
                    "finally {",
                ) => out.push_str("option finally\n"),
                (Frame::Fragment { .. }, "") => {
                    frames.pop();
                    out.push_str("end\n");
                }
                (Frame::Call { to, .. }, "") => {
                    out.push_str(&format!("deactivate {to}\n"));
                    frames.pop();
                }
                _ => return None,
            }
            continue;
        }

        let block = line.ends_with('{');
        let stmt = line
            .strip_suffix('{')
            .unwrap_or(line)
            .trim_end()
            .trim_end_matches(';');
        if let Some(Frame::Fragment {
            kind: "par",
            statements,
        }) = frames.last_mut()
        {
            if *statements != 0 {
                out.push_str("and branch\n");
            }
            *statements += 1;
        }
        if block && let Some((kind, condition)) = fragment(stmt) {
            out.push_str(&format!("{kind} {condition}\n"));
            frames.push(Frame::Fragment {
                kind,
                statements: 0,
            });
            continue;
        }

        let caller = frames.iter().rev().find_map(|frame| match frame {
            Frame::Call { from, to } => Some((*from, *to)),
            Frame::Fragment { .. } => None,
        });
        let active = caller.map_or(origin.as_str(), |(_, to)| to);
        let mut message = None;
        if stmt == "return" || stmt.starts_with("return ") {
            let (from, to) = caller?;
            message = Some((
                to,
                from,
                "-->>",
                stmt.strip_prefix("return ").unwrap_or("return"),
            ));
        } else if let Some(rest) = stmt.strip_prefix("new ") {
            let to = identifier(rest.split_once('(').map_or(rest, |(name, _)| name))?;
            out.push_str(&format!("create participant {to}\n"));
            message = Some((active, to, "->>", stmt));
        } else if let Some((from, rest)) = stmt.split_once("->") {
            let from = identifier(from)?;
            if let Some((to, label)) = rest.split_once(':')
                && let Some(to) = identifier(to)
            {
                message = Some((from, to, if reply { "-->>" } else { "->" }, label.trim()));
            } else {
                let (to, label) = invocation(rest.trim())?;
                message = Some((from, to, "->>", label));
            }
        } else {
            let (assignment, expr) = stmt.split_once('=').map_or((None, stmt), |(left, right)| {
                (Some(left.trim()), right.trim())
            });
            if let Some((to, label)) = invocation(expr) {
                if let Some(variable) = assignment {
                    let variable = identifier(variable.split_whitespace().last()?)?;
                    if block {
                        return None;
                    }
                    out.push_str(&format!(
                        "{active}->>{to}: {label}\n{to}-->>{active}: {variable}\n"
                    ));
                    comments.clear();
                    has_message = true;
                    continue;
                }
                message = Some((active, to, "->>", label));
            }
        }
        if let Some((from, to, arrow, label)) = message {
            if !comments.is_empty() {
                out.push_str(&format!("Note over {from}: {}\n", comments.join(" ")));
                comments.clear();
            }
            out.push_str(&format!("{from}{arrow}{to}: {label}\n"));
            if block {
                if arrow != "->>" {
                    return None;
                }
                out.push_str(&format!("activate {to}\n"));
                frames.push(Frame::Call { from, to });
            }
            reply = false;
            has_message = true;
        } else {
            if block || reply {
                return None;
            }
            let (kind, name, alias) = declaration(stmt)?;
            let mut decl = match kind {
                "@Actor" => format!("actor {name}"),
                "" => format!("participant {name}"),
                kind => {
                    let kind = SeqKind::from_name(kind.strip_prefix('@')?)?;
                    format!(r#"participant {name}@{{ "type": "{}" }}"#, kind.name())
                }
            };
            if let Some(alias) = alias {
                decl.push_str(&format!(" as {}", unquote(alias)));
            }
            out.push_str(&decl);
            out.push('\n');
            comments.clear();
        }
    }
    if !frames.is_empty() || reply || !has_message {
        return None;
    }
    if out.contains(&origin) {
        out.insert_str(
            "sequenceDiagram\n".len(),
            &format!("actor {origin} as External\n"),
        );
    }
    Some(out)
}

pub(super) fn render_zenuml_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    render_sequence_svg(&sequence(source)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zenuml_lowering_preserves_calls_returns_and_fragments() {
        let input = "zenuml\n@Actor 利用者\n利用者->A.run() {\nwhile(確認中) {\nB.check()\n}\nreturn 完了\n}";
        let seq = sequence(input).unwrap();
        assert!(seq.contains("actor 利用者\n利用者->>A: run()\nactivate A"));
        assert!(seq.contains("loop 確認中\nA->>B: check()\nend\nA-->>利用者: 完了\ndeactivate A"));
        assert!(sequence("zenuml\nA.run() {").is_none());
    }
}
