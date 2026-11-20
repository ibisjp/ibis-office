// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `Text.*` functions that do not depend on the culture, and the `Splitter.*` makers. What depends on Excel's rules (the case rules of
//! non-ASCII letters, which blanks `Text.Trim` takes, `Text.Proper`, `Text.Split` of an empty delimiter, what `Text.From` writes) is in the
//! request `tasks/doc/requests/2026-11-17-k0039-m-semantics.md`; here the plain Unicode rules stand until it is answered.
use crate::builtin::{as_number, as_text, list_items};
use crate::error::MError;
use crate::eval::Interp;
use crate::table::{native, reg};
use crate::value::*;
use std::collections::HashMap;
use std::rc::Rc;

type R = Result<Value, MError>;

/// The UTF-16 units of a text or of a list of one-unit texts (the characters of `Text.Remove`, `Text.Select`, a transition): Excel's text
/// is UTF-16, so a character outside the basic plane is two units and is not "a character" there.
fn chars_of(it: &Interp, v: &Value) -> Result<Vec<u16>, MError> {
    match v {
        Value::Text(s) => Ok(s.encode_utf16().collect()),
        v => {
            let mut out = Vec::new();
            for x in list_items(it, v)? {
                let t = as_text(&x)?;
                let u: Vec<u16> = t.encode_utf16().collect();
                if u.len() != 1 {
                    return Err(MError::expr("The value is not a text of one character."));
                }
                out.push(u[0]);
            }
            Ok(out)
        }
    }
}

/// Whether the comparer given (`Comparer.OrdinalIgnoreCase`...) treats "a" and "A" as equal.
pub(crate) fn ignore_case(it: &Interp, cmp: Option<&Value>) -> Result<bool, MError> {
    match cmp {
        None | Some(Value::Null) => Ok(false),
        Some(f) => Ok(matches!(it.call(f, vec![Value::text("a"), Value::text("A")])?, Value::Number(n) if n == 0.0)),
    }
}

/// The text folded for an ordinal comparison that ignores case (the upper simple mapping, as .NET's OrdinalIgnoreCase does).
fn fold(s: &str, ic: bool) -> String {
    if ic { s.chars().map(crate::culture::simple_upper).collect() } else { s.to_string() }
}

fn each_text(a: &[Value], f: impl FnOnce(&str) -> R) -> R {
    match &a[0] {
        Value::Null => Ok(Value::Null),
        v => f(&as_text(v)?),
    }
}

fn count(v: &Value) -> Result<usize, MError> {
    let n = as_number(v)?;
    if n < 0.0 || n.fract() != 0.0 {
        return Err(MError::expr("The count must be a non-negative whole number."));
    }
    Ok(n as usize)
}

fn slice(c: &[u16], a: usize, b: usize) -> String {
    String::from_utf16_lossy(&c[a.min(c.len())..b.min(c.len())])
}

/// Positions of `needle` in `hay` (char offsets), non-overlapping from the left.
fn find_all(hay: &[u16], needle: &[u16]) -> Vec<usize> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()] == *needle {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

/// The `index`th delimiter (0-based, from the start or the end) as a char offset and its length, in `s`.
fn nth_delim(s: &[u16], d: &[u16], spec: Option<&Value>, it: &Interp) -> Result<Option<usize>, MError> {
    let (n, from_end) = match spec {
        None | Some(Value::Null) => (0, false),
        Some(Value::Number(n)) => (*n as usize, false),
        Some(v) => {
            let p = list_items(it, v)?;
            (count(&p[0])?, matches!(p.get(1), Some(Value::Number(x)) if *x == 1.0))
        }
    };
    let all = find_all(s, d);
    Ok(if from_end { all.len().checked_sub(n + 1).map(|i| all[i]) } else { all.get(n).copied() })
}

pub fn register(l: &mut HashMap<&'static str, Value>) {
    for (n, v) in [("Occurrence.First", 0.0), ("Occurrence.Last", 1.0), ("Occurrence.All", 2.0), ("RelativePosition.FromStart", 0.0), ("RelativePosition.FromEnd", 1.0)] {
        l.insert(n, Value::Number(v));
    }
    reg(l, "Comparer.Ordinal", 2, 2, |_, a| {
        Ok(Value::Number(match (as_text(&a[0])?.as_bytes().cmp(as_text(&a[1])?.as_bytes()), 0) {
            (std::cmp::Ordering::Less, _) => -1.0,
            (std::cmp::Ordering::Equal, _) => 0.0,
            _ => 1.0,
        }))
    });
    reg(l, "Comparer.OrdinalIgnoreCase", 2, 2, |_, a| {
        let (x, y) = (as_text(&a[0])?.to_lowercase(), as_text(&a[1])?.to_lowercase());
        Ok(Value::Number(match x.as_bytes().cmp(y.as_bytes()) {
            std::cmp::Ordering::Less => -1.0,
            std::cmp::Ordering::Equal => 0.0,
            _ => 1.0,
        }))
    });
    reg(l, "Text.Contains", 2, 3, |it, a| {
        let ic = ignore_case(it, a.get(2))?;
        each_text(a, |s| Ok(Value::Logical(fold(s, ic).contains(&fold(&as_text(&a[1])?, ic)))))
    });
    reg(l, "Text.StartsWith", 2, 3, |it, a| {
        let ic = ignore_case(it, a.get(2))?;
        each_text(a, |s| Ok(Value::Logical(fold(s, ic).starts_with(&fold(&as_text(&a[1])?, ic)))))
    });
    reg(l, "Text.EndsWith", 2, 3, |it, a| {
        let ic = ignore_case(it, a.get(2))?;
        each_text(a, |s| Ok(Value::Logical(fold(s, ic).ends_with(&fold(&as_text(&a[1])?, ic)))))
    });
    reg(l, "Text.Replace", 3, 3, |_, a| {
        each_text(a, |s| {
            let (old, new) = (as_text(&a[1])?, as_text(&a[2])?);
            Ok(Value::text(if old.is_empty() { s.to_string() } else { s.replace(&*old, &new) }))
        })
    });
    reg(l, "Text.Remove", 2, 2, |it, a| {
        let drop = chars_of(it, &a[1])?;
        each_text(a, |s| Ok(Value::text(String::from_utf16_lossy(&s.encode_utf16().filter(|c| !drop.contains(c)).collect::<Vec<u16>>()))))
    });
    reg(l, "Text.Select", 2, 2, |it, a| {
        let keep = chars_of(it, &a[1])?;
        each_text(a, |s| Ok(Value::text(String::from_utf16_lossy(&s.encode_utf16().filter(|c| keep.contains(c)).collect::<Vec<u16>>()))))
    });
    reg(l, "Text.Combine", 1, 2, |it, a| {
        let sep = match a.get(1) {
            Some(Value::Null) | None => String::new(),
            Some(v) => as_text(v)?.to_string(),
        };
        let mut parts = Vec::new();
        for v in list_items(it, &a[0])? {
            match v {
                Value::Null => {}
                v => parts.push(as_text(&v)?.to_string()),
            }
        }
        Ok(Value::text(parts.join(&sep)))
    });
    reg(l, "Text.Split", 2, 2, |_, a| {
        if matches!(a[0], Value::Null) {
            return Err(MError::expr("We cannot convert the value null to type Text."));
        }
        each_text(a, |s| {
            let d = as_text(&a[1])?;
            if d.is_empty() {
                return Ok(Value::list(vec![Value::text(s)]));
            }
            Ok(Value::list(s.split(&*d).map(Value::text).collect()))
        })
    });
    reg(l, "Text.SplitAny", 2, 2, |_, a| {
        each_text(a, |s| {
            let seps: Vec<u16> = as_text(&a[1])?.encode_utf16().collect();
            let mut out: Vec<String> = Vec::new();
            let mut cur: Vec<u16> = Vec::new();
            for u in s.encode_utf16() {
                if seps.contains(&u) {
                    out.push(String::from_utf16_lossy(&cur));
                    cur.clear();
                } else {
                    cur.push(u);
                }
            }
            out.push(String::from_utf16_lossy(&cur));
            Ok(Value::list(out.into_iter().map(Value::text).collect()))
        })
    });
    reg(l, "Text.Range", 2, 3, |_, a| {
        each_text(a, |s| {
            let c: Vec<u16> = s.encode_utf16().collect();
            let off = count(&a[1])?;
            let end = match a.get(2) {
                Some(Value::Null) | None => {
                    if off > c.len() {
                        return Err(MError::expr("There weren't enough elements in the enumeration to complete the operation."));
                    }
                    c.len()
                }
                Some(n) => off + count(n)?,
            };
            if off > c.len() || end > c.len() {
                return Err(MError::expr("There weren't enough elements in the enumeration to complete the operation."));
            }
            Ok(Value::text(slice(&c, off, end)))
        })
    });
    reg(l, "Text.Start", 2, 2, |_, a| {
        each_text(a, |s| {
            let c: Vec<u16> = s.encode_utf16().collect();
            Ok(Value::text(slice(&c, 0, count(&a[1])?)))
        })
    });
    reg(l, "Text.End", 2, 2, |_, a| {
        each_text(a, |s| {
            let c: Vec<u16> = s.encode_utf16().collect();
            Ok(Value::text(slice(&c, c.len().saturating_sub(count(&a[1])?), c.len())))
        })
    });
    reg(l, "Text.Middle", 2, 3, |_, a| {
        each_text(a, |s| {
            let c: Vec<u16> = s.encode_utf16().collect();
            let off = count(&a[1])?;
            let end = match a.get(2) {
                Some(Value::Null) | None => c.len(),
                Some(n) => off + count(n)?,
            };
            Ok(Value::text(slice(&c, off, end)))
        })
    });
    reg(l, "Text.PadStart", 2, 3, |_, a| pad(a, true));
    reg(l, "Text.PadEnd", 2, 3, |_, a| pad(a, false));
    reg(l, "Text.Reverse", 1, 1, |_, a| each_text(a, |s| Ok(Value::text(String::from_utf16_lossy(&s.encode_utf16().collect::<Vec<u16>>().into_iter().rev().collect::<Vec<u16>>())))));
    reg(l, "Text.ToList", 1, 1, |_, a| each_text(a, |s| Ok(Value::list(s.encode_utf16().map(|u| Value::text(String::from_utf16_lossy(&[u]))).collect()))));
    reg(l, "Text.Repeat", 2, 3, |_, a| {
        each_text(a, |s| {
            let n = count(&a[1])?;
            if s.len().saturating_mul(n) > 100_000_000 {
                return Err(MError::limit("The text is too long."));
            }
            Ok(Value::text(s.repeat(n)))
        })
    });
    reg(l, "Text.Insert", 3, 3, |_, a| {
        each_text(a, |s| {
            let c: Vec<u16> = s.encode_utf16().collect();
            let off = count(&a[1])?;
            if off > c.len() {
                return Err(MError::expr("The offset is beyond the end of the text."));
            }
            Ok(Value::text(format!("{}{}{}", slice(&c, 0, off), as_text(&a[2])?, slice(&c, off, c.len()))))
        })
    });
    reg(l, "Text.At", 2, 2, |_, a| {
        each_text(a, |s| {
            let i = count(&a[1])?;
            s.encode_utf16().nth(i).map(|u| Value::text(String::from_utf16_lossy(&[u]))).ok_or_else(|| MError::expr("The index is beyond the end of the text."))
        })
    });
    reg(l, "Text.TrimStart", 1, 2, |it, a| trim(it, a, true, false));
    reg(l, "Text.TrimEnd", 1, 2, |it, a| trim(it, a, false, true));
    reg(l, "Text.Trim", 1, 2, |it, a| trim(it, a, true, true));
    reg(l, "Text.PositionOf", 2, 4, |it, a| {
        let ic = ignore_case(it, a.get(3))?;
        each_text(a, |s| {
            let hay: Vec<u16> = fold(s, ic).encode_utf16().collect();
            let needle: Vec<u16> = fold(&as_text(&a[1])?, ic).encode_utf16().collect();
            let all = find_all(&hay, &needle);
            Ok(match a.get(2) {
                Some(Value::Number(n)) if *n == 2.0 => Value::list(all.into_iter().map(|i| Value::Number(i as f64)).collect()),
                Some(Value::Number(n)) if *n == 1.0 => Value::Number(all.last().map_or(-1.0, |&i| i as f64)),
                _ => Value::Number(all.first().map_or(-1.0, |&i| i as f64)),
            })
        })
    });
    reg(l, "Text.AfterDelimiter", 2, 3, |it, a| {
        if as_text(&a[1])?.is_empty() {
            return Err(MError::expr("The delimiter must be a text of at least one character."));
        }
        each_text(a, |s| {
            let c: Vec<u16> = s.encode_utf16().collect();
            let d: Vec<u16> = as_text(&a[1])?.encode_utf16().collect();
            Ok(match nth_delim(&c, &d, a.get(2), it)? {
                Some(i) => Value::text(slice(&c, i + d.len(), c.len())),
                None => Value::text(""),
            })
        })
    });
    reg(l, "Text.BeforeDelimiter", 2, 3, |it, a| {
        if as_text(&a[1])?.is_empty() {
            return Err(MError::expr("The delimiter must be a text of at least one character."));
        }
        each_text(a, |s| {
            let c: Vec<u16> = s.encode_utf16().collect();
            let d: Vec<u16> = as_text(&a[1])?.encode_utf16().collect();
            Ok(match nth_delim(&c, &d, a.get(2), it)? {
                Some(i) => Value::text(slice(&c, 0, i)),
                None => Value::text(""),
            })
        })
    });
    reg(l, "Text.BetweenDelimiters", 3, 5, |it, a| {
        each_text(a, |s| {
            let c: Vec<u16> = s.encode_utf16().collect();
            let (d1, d2): (Vec<u16>, Vec<u16>) = (as_text(&a[1])?.encode_utf16().collect(), as_text(&a[2])?.encode_utf16().collect());
            let Some(i) = nth_delim(&c, &d1, a.get(3), it)? else { return Ok(Value::text("")) };
            let rest: Vec<u16> = c[i + d1.len()..].to_vec();
            Ok(match nth_delim(&rest, &d2, a.get(4), it)? {
                Some(j) => Value::text(slice(&rest, 0, j)),
                None => Value::text(""),
            })
        })
    });

    // Splitters (the semantics are those Excel gave: tasks/doc/requests/2026-11-16-k0039-split-semantics-result.md, section A).
    reg(l, "Splitter.SplitTextByAnyDelimiter", 1, 2, |it, a| {
        let ds: Vec<Vec<u16>> = list_items(it, &a[0])?.iter().map(|d| as_text(d).map(|s| s.encode_utf16().collect())).collect::<Result<_, _>>()?;
        Ok(native("Splitter.SplitTextByAnyDelimiter", 1, 1, move |_, args| {
            each_text(args, |s| {
                let c: Vec<u16> = s.encode_utf16().collect();
                let (mut out, mut cur, mut i): (Vec<String>, Vec<u16>, usize) = (Vec::new(), Vec::new(), 0);
                while i < c.len() {
                    match ds.iter().filter(|d| !d.is_empty()).find(|d| c[i..].starts_with(d)) {
                        Some(d) => {
                            out.push(String::from_utf16_lossy(&std::mem::take(&mut cur)));
                            i += d.len();
                        }
                        None => {
                            cur.push(c[i]);
                            i += 1;
                        }
                    }
                }
                out.push(String::from_utf16_lossy(&cur));
                Ok(Value::list(out.into_iter().map(Value::text).collect()))
            })
        }))
    });
    reg(l, "Splitter.SplitTextByEachDelimiter", 1, 3, |it, a| {
        let ds: Vec<String> = list_items(it, &a[0])?.iter().map(|d| as_text(d).map(|s| s.to_string())).collect::<Result<_, _>>()?;
        let from_end = matches!(a.get(2), Some(Value::Logical(true)));
        Ok(native("Splitter.SplitTextByEachDelimiter", 1, 1, move |_, args| {
            each_text(args, |s| {
                let mut pieces = Vec::new();
                let mut rest = s.to_string();
                for d in ds.iter().filter(|d| !d.is_empty()) {
                    let at = if from_end { rest.rfind(d.as_str()) } else { rest.find(d.as_str()) };
                    let Some(at) = at else { break };
                    if from_end {
                        pieces.push(rest[at + d.len()..].to_string());
                        rest.truncate(at);
                    } else {
                        pieces.push(rest[..at].to_string());
                        rest = rest[at + d.len()..].to_string();
                    }
                }
                pieces.push(rest);
                if from_end {
                    pieces.reverse();
                }
                Ok(Value::list(pieces.into_iter().map(Value::text).collect()))
            })
        }))
    });
    reg(l, "Splitter.SplitTextByPositions", 1, 2, |it, a| {
        let pos: Vec<usize> = list_items(it, &a[0])?.iter().map(count).collect::<Result<_, _>>()?;
        let from_end = matches!(a.get(1), Some(Value::Logical(true)));
        Ok(native("Splitter.SplitTextByPositions", 1, 1, move |_, args| {
            each_text(args, |s| {
                let c: Vec<u16> = s.encode_utf16().collect();
                let n = c.len();
                let mut at: Vec<usize> = pos.iter().map(|&p| if from_end { n.saturating_sub(p) } else { p.min(n) }).collect();
                at.sort_unstable();
                at.dedup();
                let mut out: Vec<String> = if from_end {
                    let mut bounds = vec![0];
                    bounds.extend(&at);
                    bounds.windows(2).map(|w| slice(&c, w[0], w[1])).collect()
                } else {
                    at.iter().enumerate().map(|(k, &x)| slice(&c, x, at.get(k + 1).copied().unwrap_or(n))).collect()
                };
                if out.is_empty() {
                    out.push(s.to_string());
                }
                Ok(Value::list(out.into_iter().map(Value::text).collect()))
            })
        }))
    });
    reg(l, "Splitter.SplitTextByLengths", 1, 2, |it, a| {
        let lens: Vec<usize> = list_items(it, &a[0])?.iter().map(count).collect::<Result<_, _>>()?;
        let from_end = matches!(a.get(1), Some(Value::Logical(true)));
        Ok(native("Splitter.SplitTextByLengths", 1, 1, move |_, args| {
            each_text(args, |s| {
                let c: Vec<u16> = s.encode_utf16().collect();
                let mut out = Vec::new();
                if from_end {
                    let mut end = c.len();
                    for &n in &lens {
                        let start = end.saturating_sub(n);
                        out.push(slice(&c, start, end));
                        end = start;
                    }
                    out.reverse();
                } else {
                    let mut start = 0;
                    for &n in &lens {
                        out.push(slice(&c, start, start + n));
                        start += n;
                    }
                }
                Ok(Value::list(out.into_iter().map(Value::text).collect()))
            })
        }))
    });
    reg(l, "Splitter.SplitTextByRepeatedLengths", 1, 2, |_, a| {
        let n = count(&a[0])?;
        let from_end = matches!(a.get(1), Some(Value::Logical(true)));
        if n == 0 {
            return Err(MError::expr("The length must be positive."));
        }
        Ok(native("Splitter.SplitTextByRepeatedLengths", 1, 1, move |_, args| {
            each_text(args, |s| {
                let c: Vec<u16> = s.encode_utf16().collect();
                let mut out: Vec<String> = if from_end {
                    let mut v: Vec<String> = Vec::new();
                    let mut end = c.len();
                    while end > 0 {
                        let start = end.saturating_sub(n);
                        v.push(slice(&c, start, end));
                        end = start;
                    }
                    v.reverse();
                    v
                } else {
                    c.chunks(n).map(|x| String::from_utf16_lossy(x)).collect()
                };
                if out.is_empty() {
                    out.push(String::new());
                }
                Ok(Value::list(out.into_iter().map(Value::text).collect()))
            })
        }))
    });
    reg(l, "Splitter.SplitTextByCharacterTransition", 2, 2, |it, a| {
        // Each side is a list of characters or a function of a character.
        let side = |v: &Value| -> Result<Rc<dyn Fn(&Interp, u16) -> Result<bool, MError>>, MError> {
            match v {
                Value::Function(_) => {
                    let f = v.clone();
                    Ok(Rc::new(move |it: &Interp, c: u16| match it.call(&f, vec![Value::text(String::from_utf16_lossy(&[c]))])? {
                        Value::Logical(b) => Ok(b),
                        _ => Err(MError::expr("The test of a character must give a logical.")),
                    }))
                }
                v => {
                    let set = chars_of(it, v)?;
                    Ok(Rc::new(move |_: &Interp, c: u16| Ok(set.contains(&c))))
                }
            }
        };
        let (from, to) = (side(&a[0])?, side(&a[1])?);
        Ok(native("Splitter.SplitTextByCharacterTransition", 1, 1, move |it, args| {
            each_text(args, |s| {
                let c: Vec<u16> = s.encode_utf16().collect();
                let (mut out, mut cur): (Vec<String>, Vec<u16>) = (Vec::new(), Vec::new());
                for (k, &ch) in c.iter().enumerate() {
                    if k > 0 && from(it, c[k - 1])? && to(it, ch)? {
                        out.push(String::from_utf16_lossy(&std::mem::take(&mut cur)));
                    }
                    cur.push(ch);
                }
                out.push(String::from_utf16_lossy(&cur));
                Ok(Value::list(out.into_iter().map(Value::text).collect()))
            })
        }))
    });
}

fn pad(a: &[Value], start: bool) -> R {
    each_text(a, |s| {
        let n = count(&a[1])?;
        let p = match a.get(2) {
            Some(Value::Null) | None => ' ',
            Some(v) => {
                let t = as_text(v)?;
                let mut it = t.chars();
                match (it.next(), it.next()) {
                    (Some(c), None) if c.len_utf16() == 1 => c,
                    _ => return Err(MError::expr("The value is not a text of one character.")),
                }
            }
        };
        let len = s.encode_utf16().count();
        if len >= n {
            return Ok(Value::text(s));
        }
        let fill: String = std::iter::repeat_n(p, n - len).collect();
        Ok(Value::text(if start { format!("{fill}{s}") } else { format!("{s}{fill}") }))
    })
}

fn trim(it: &Interp, a: &[Value], front: bool, back: bool) -> R {
    let set = match a.get(1) {
        Some(Value::Null) | None => None,
        Some(v) => Some(chars_of(it, v)?),
    };
    each_text(a, |s| {
        let test = |c: char| match &set {
            Some(set) => c.len_utf16() == 1 && set.contains(&(c as u32 as u16)),
            None => c.is_whitespace(),
        };
        let mut t = s;
        if front {
            t = t.trim_start_matches(test);
        }
        if back {
            t = t.trim_end_matches(test);
        }
        Ok(Value::text(t))
    })
}
