// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A math formula (Office Math, `m:oMath`) as a tree, shared by the formats
//! that carry one (PowerPoint's `a14:m`, Word's `m:oMath`).

pub use crate::latex::parse_latex;

/// One node of a formula.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Math {
    /// Plain characters (`m:r`).
    Text(String),
    /// Nodes side by side.
    Row(Vec<Math>),
    /// A fraction (`m:f`).
    Frac { num: Box<Math>, den: Box<Math>, kind: FracKind },
    /// A superscript, subscript, or both (`m:sSup`, `m:sSub`, `m:sSubSup`).
    Script { base: Box<Math>, sub: Option<Box<Math>>, sup: Option<Box<Math>> },
    /// A root (`m:rad`); `degree` is None for a square root.
    Radical { degree: Option<Box<Math>>, body: Box<Math> },
    /// A big operator with limits (`m:nary`): a sum, an integral, ... The limits are above and below it (`stacked`, `m:limLoc`
    /// `undOvr`) or at its right (`subSup`).
    Nary { op: String, sub: Option<Box<Math>>, sup: Option<Box<Math>>, body: Box<Math>, stacked: bool },
    /// Brackets around items (`m:d`).
    Delimited { open: String, close: String, sep: String, items: Vec<Math> },
    /// A function name and its argument (`m:func`).
    Function { name: Box<Math>, arg: Box<Math> },
    /// A mark over or under the base (`m:acc`, `m:bar`, `m:limUpp`, `m:limLow`, `m:groupChr`).
    Mark { base: Box<Math>, mark: String, over: bool },
    /// A matrix (`m:m`), row by row.
    Matrix(Vec<Vec<Math>>),
    /// An array of equations (`m:eqArr`): the rows are lined up at the `&` in them.
    EqArray(Vec<Math>),
    /// `body` (a run's text, `m:r`) in a colour of its own (`w:rPr/w:color` in the `m:r`): a formula's runs have each their colour (#171). The
    /// others are in the colour of the formula's run in the page.
    Colored { color: crate::drawing::Rgb, body: Box<Math> },
}

/// How a fraction is drawn (`m:f/m:fPr/m:type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FracKind {
    /// With a bar (`bar`).
    Bar,
    /// Stacked without a bar (`noBar`).
    NoBar,
    /// With a slash, the numerator above and the denominator below it (`skw`).
    Skew,
    /// On one line, `a/b` (`lin`).
    Linear,
}

/// Whether a big operator (`op`) has its limits above and below it by default: its limits are at its right for an
/// integral, above and below for the others (ECMA-376 22.1.2.62, `limLoc`).
pub fn stacked_by_default(op: &str) -> bool {
    !matches!(op, "∫" | "∬" | "∭" | "∮" | "∯" | "∰")
}

const SUPER: &str = "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ⁿⁱ";
const SUPER_FROM: &str = "0123456789+-=()ni";
const SUB: &str = "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₙᵢ";
const SUB_FROM: &str = "0123456789+-=()ni";

fn map_chars(text: &str, from: &str, to: &str) -> Option<String> {
    let from: Vec<char> = from.chars().collect();
    let to: Vec<char> = to.chars().collect();
    text.chars().map(|c| from.iter().position(|f| *f == c).map(|i| to[i])).collect()
}

impl Math {
    /// The formula as one line of text (UnicodeMath style), which is what is
    /// shown where the formula cannot be typeset.
    pub fn linear(&self) -> String {
        match self {
            Math::Text(t) => t.clone(),
            Math::Row(items) => items.iter().map(Math::linear).collect(),
            Math::Frac { num, den, kind } => {
                let (n, d) = (num.grouped(), den.grouped());
                match kind {
                    FracKind::Bar => format!("{n}/{d}"),
                    FracKind::NoBar => format!("{n}¦{d}"),
                    FracKind::Skew => format!("{n}⊘{d}"),
                    FracKind::Linear => format!("{n}∕{d}"),
                }
            }
            Math::Script { base, sub, sup } => {
                let mut out = base.grouped();
                if let Some(s) = sub {
                    let t = s.linear();
                    out += &map_chars(&t, SUB_FROM, SUB).unwrap_or_else(|| format!("_{}", s.grouped()));
                }
                if let Some(s) = sup {
                    let t = s.linear();
                    out += &map_chars(&t, SUPER_FROM, SUPER).unwrap_or_else(|| format!("^{}", s.grouped()));
                }
                out
            }
            Math::Radical { degree, body } => match degree.as_deref().map(Math::linear).filter(|d| !d.is_empty() && d != "2") {
                None => format!("√{}", body.grouped()),
                Some(d) if d == "3" => format!("∛{}", body.grouped()),
                Some(d) if d == "4" => format!("∜{}", body.grouped()),
                Some(d) => format!("√({d}&{})", body.linear()),
            },
            Math::Nary { op, sub, sup, body, .. } => {
                let mut out = op.clone();
                if let Some(s) = sub {
                    out += &format!("_{}", s.grouped());
                }
                if let Some(s) = sup {
                    out += &format!("^{}", s.grouped());
                }
                out + &body.grouped_if_row()
            }
            Math::Delimited { open, close, sep, items } => {
                let sep = if sep.is_empty() { "|".to_string() } else { sep.clone() };
                format!("{open}{}{close}", items.iter().map(Math::linear).collect::<Vec<_>>().join(&sep))
            }
            Math::Function { name, arg } => format!("{}{}", name.linear(), arg.grouped_if_row()),
            Math::Mark { base, mark, over } => {
                if *over { format!("{}{mark}", base.linear()) } else { format!("{}_{mark}", base.grouped()) }
            }
            Math::EqArray(rows) => format!("█({})", rows.iter().map(Math::linear).collect::<Vec<_>>().join("@")),
            Math::Colored { body, .. } => body.linear(),
            Math::Matrix(rows) => {
                let body: Vec<String> = rows.iter().map(|r| r.iter().map(Math::linear).collect::<Vec<_>>().join("&")).collect();
                format!("■({})", body.join("@"))
            }
        }
    }

    /// Convert the formula to standard LaTeX math syntax.
    pub fn to_latex(&self) -> String {
        match self {
            Math::Text(t) => {
                let mut out = String::new();
                for c in t.chars() {
                    match c {
                        'α' => out.push_str(r"\alpha "),
                        'β' => out.push_str(r"\beta "),
                        'γ' => out.push_str(r"\gamma "),
                        'δ' => out.push_str(r"\delta "),
                        'ε' => out.push_str(r"\epsilon "),
                        'ζ' => out.push_str(r"\zeta "),
                        'η' => out.push_str(r"\eta "),
                        'θ' => out.push_str(r"\theta "),
                        'ι' => out.push_str(r"\iota "),
                        'κ' => out.push_str(r"\kappa "),
                        'λ' => out.push_str(r"\lambda "),
                        'μ' => out.push_str(r"\mu "),
                        'ν' => out.push_str(r"\nu "),
                        'ξ' => out.push_str(r"\xi "),
                        'π' => out.push_str(r"\pi "),
                        'ρ' => out.push_str(r"\rho "),
                        'σ' => out.push_str(r"\sigma "),
                        'τ' => out.push_str(r"\tau "),
                        'υ' => out.push_str(r"\upsilon "),
                        'φ' => out.push_str(r"\phi "),
                        'χ' => out.push_str(r"\chi "),
                        'ψ' => out.push_str(r"\psi "),
                        'ω' => out.push_str(r"\omega "),
                        'Γ' => out.push_str(r"\Gamma "),
                        'Δ' => out.push_str(r"\Delta "),
                        'Θ' => out.push_str(r"\Theta "),
                        'Λ' => out.push_str(r"\Lambda "),
                        'Ξ' => out.push_str(r"\Xi "),
                        'Π' => out.push_str(r"\Pi "),
                        'Σ' => out.push_str(r"\Sigma "),
                        'Υ' => out.push_str(r"\Upsilon "),
                        'Φ' => out.push_str(r"\Phi "),
                        'Ψ' => out.push_str(r"\Psi "),
                        'Ω' => out.push_str(r"\Omega "),
                        '±' => out.push_str(r"\pm "),
                        '∓' => out.push_str(r"\mp "),
                        '×' => out.push_str(r"\times "),
                        '÷' => out.push_str(r"\div "),
                        '·' => out.push_str(r"\cdot "),
                        '≤' => out.push_str(r"\le "),
                        '≥' => out.push_str(r"\ge "),
                        '≠' => out.push_str(r"\ne "),
                        '≈' => out.push_str(r"\approx "),
                        '≡' => out.push_str(r"\equiv "),
                        '∞' => out.push_str(r"\infty "),
                        '∂' => out.push_str(r"\partial "),
                        '∇' => out.push_str(r"\nabla "),
                        '∈' => out.push_str(r"\in "),
                        '∉' => out.push_str(r"\notin "),
                        '⊂' => out.push_str(r"\subset "),
                        '⊆' => out.push_str(r"\subseteq "),
                        '∪' => out.push_str(r"\cup "),
                        '∩' => out.push_str(r"\cap "),
                        '→' => out.push_str(r"\to "),
                        '←' => out.push_str(r"\leftarrow "),
                        '⇒' => out.push_str(r"\Rightarrow "),
                        '⇐' => out.push_str(r"\Leftarrow "),
                        '⇔' => out.push_str(r"\Leftrightarrow "),
                        '∀' => out.push_str(r"\forall "),
                        '∃' => out.push_str(r"\exists "),
                        '¬' => out.push_str(r"\neg "),
                        _ => out.push(c),
                    }
                }
                out
            }
            Math::Row(items) => items.iter().map(Math::to_latex).collect(),
            Math::Frac { num, den, kind } => match kind {
                FracKind::Bar => format!(r"\frac{{{}}}{{{}}}", num.to_latex().trim(), den.to_latex().trim()),
                FracKind::NoBar => format!(r"\nobarfrac{{{}}}{{{}}}", num.to_latex().trim(), den.to_latex().trim()),
                FracKind::Skew => format!(r"\sfrac{{{}}}{{{}}}", num.to_latex().trim(), den.to_latex().trim()),
                FracKind::Linear => format!(r"\lfrac{{{}}}{{{}}}", num.to_latex().trim(), den.to_latex().trim()),
            },
            Math::Script { base, sub, sup } => {
                let mut out = base.to_latex().trim().to_string();
                if let Some(s) = sub {
                    out.push_str(&format!(r"_{{{}}}", s.to_latex().trim()));
                }
                if let Some(s) = sup {
                    out.push_str(&format!(r"^{{{}}}", s.to_latex().trim()));
                }
                out
            }
            Math::Radical { degree, body } => match degree {
                None => format!(r"\sqrt{{{}}}", body.to_latex().trim()),
                Some(d) => format!(r"\sqrt[{}]{{{}}}", d.to_latex().trim(), body.to_latex().trim()),
            },
            Math::Nary { op, sub, sup, body, .. } => {
                let op_cmd = match op.as_str() {
                    "∑" => r"\sum",
                    "∏" => r"\prod",
                    "∐" => r"\coprod",
                    "∫" => r"\int",
                    "∬" => r"\iint",
                    "∭" => r"\iiint",
                    "∮" => r"\oint",
                    "⋃" => r"\bigcup",
                    "⋂" => r"\bigcap",
                    "⋁" => r"\bigvee",
                    "⋀" => r"\bigwedge",
                    _ => op.as_str(),
                };
                let mut out = op_cmd.to_string();
                if let Some(s) = sub {
                    out.push_str(&format!(r"_{{{}}}", s.to_latex().trim()));
                }
                if let Some(s) = sup {
                    out.push_str(&format!(r"^{{{}}}", s.to_latex().trim()));
                }
                let b = body.to_latex().trim().to_string();
                if !b.is_empty() {
                    out.push(' ');
                    out.push_str(&b);
                }
                out
            }
            Math::Delimited { open, close, items, .. } => {
                if open == "(" && close == ")" && items.len() == 1 {
                    if let Math::Frac { num, den, kind: FracKind::NoBar } = &items[0] {
                        return format!(r"\binom{{{}}}{{{}}}", num.to_latex().trim(), den.to_latex().trim());
                    }
                }
                let inner = items.iter().map(Math::to_latex).collect::<Vec<_>>().join(",");
                let left = match open.as_str() {
                    "{" => r"\left\{",
                    "(" => r"\left(",
                    "[" => r"\left[",
                    "|" => r"\left|",
                    "" => r"\left.",
                    other => other,
                };
                let right = match close.as_str() {
                    "}" => r"\right\}",
                    ")" => r"\right)",
                    "]" => r"\right]",
                    "|" => r"\right|",
                    "" => r"\right.",
                    other => other,
                };
                format!("{left}{inner}{right}")
            }
            Math::Function { name, arg } => {
                let n = name.linear();
                let a = arg.to_latex().trim().to_string();
                if a.starts_with(r"\left(") || a.starts_with('(') {
                    format!(r"\{n}{a}")
                } else if a.is_empty() {
                    format!(r"\{n}")
                } else {
                    format!(r"\{n}{{{a}}}")
                }
            }
            Math::Mark { base, mark, over } => {
                let b = base.to_latex().trim().to_string();
                if *over {
                    match mark.as_str() {
                        "^" => format!(r"\hat{{{b}}}"),
                        "→" => format!(r"\vec{{{b}}}"),
                        "¯" => format!(r"\overline{{{b}}}"),
                        "˙" => format!(r"\dot{{{b}}}"),
                        "¨" => format!(r"\ddot{{{b}}}"),
                        "~" => format!(r"\tilde{{{b}}}"),
                        _ => format!(r"\overline{{{b}}}"),
                    }
                } else {
                    format!(r"\underline{{{b}}}")
                }
            }
            Math::Matrix(rows) => {
                let mut out = String::from(r"\begin{matrix}");
                for (i, r) in rows.iter().enumerate() {
                    if i > 0 {
                        out.push_str(r" \\ ");
                    }
                    out.push_str(&r.iter().map(|c| c.to_latex().trim().to_string()).collect::<Vec<_>>().join(" & "));
                }
                out.push_str(r"\end{matrix}");
                out
            }
            Math::EqArray(rows) => {
                let mut out = String::from(r"\begin{aligned}");
                for (i, r) in rows.iter().enumerate() {
                    if i > 0 {
                        out.push_str(r" \\ ");
                    }
                    out.push_str(r.to_latex().trim());
                }
                out.push_str(r"\end{aligned}");
                out
            }
            Math::Colored { body, .. } => body.to_latex(),
        }
    }

    /// The formula with each of its runs (`Text`, `Colored`) replaced by what `f` gives for it.
    fn map_runs(&self, f: &dyn Fn(&Math) -> Math) -> Math {
        let all = |items: &[Math]| items.iter().map(|m| m.map_runs(f)).collect::<Vec<_>>();
        let boxed = |m: &Math| Box::new(m.map_runs(f));
        let opt = |m: &Option<Box<Math>>| m.as_deref().map(boxed);
        match self {
            Math::Text(_) | Math::Colored { .. } => f(self),
            Math::Row(items) => Math::Row(all(items)),
            Math::Frac { num, den, kind } => Math::Frac { num: boxed(num), den: boxed(den), kind: *kind },
            Math::Script { base, sub, sup } => Math::Script { base: boxed(base), sub: opt(sub), sup: opt(sup) },
            Math::Radical { degree, body } => Math::Radical { degree: opt(degree), body: boxed(body) },
            Math::Nary { op, sub, sup, body, stacked } => Math::Nary { op: op.clone(), sub: opt(sub), sup: opt(sup), body: boxed(body), stacked: *stacked },
            Math::Delimited { open, close, sep, items } => Math::Delimited { open: open.clone(), close: close.clone(), sep: sep.clone(), items: all(items) },
            Math::Function { name, arg } => Math::Function { name: boxed(name), arg: boxed(arg) },
            Math::Mark { base, mark, over } => Math::Mark { base: boxed(base), mark: mark.clone(), over: *over },
            Math::Matrix(rows) => Math::Matrix(rows.iter().map(|r| all(r)).collect()),
            Math::EqArray(rows) => Math::EqArray(all(rows)),
        }
    }

    /// The formula without the colours of its runs (`Colored`): what is left when the whole formula is given a colour, as Word does.
    pub fn without_colors(&self) -> Math {
        self.map_runs(&|run| match run {
            Math::Colored { body, .. } => (**body).clone(),
            other => other.clone(),
        })
    }

    /// The formula with `color` on each run that has no colour of its own: how it is written when its page run has a colour.
    pub fn with_default_color(&self, color: crate::drawing::Rgb) -> Math {
        self.map_runs(&|run| match run {
            Math::Text(t) if !t.is_empty() => Math::Colored { color, body: Box::new(run.clone()) },
            other => other.clone(),
        })
    }

    /// The text of a run (`Text`, or `Colored` holding one): what the neighbouring items look at to space the formula.
    pub fn run_text(&self) -> Option<&str> {
        match self {
            Math::Text(t) => Some(t),
            Math::Colored { body, .. } => body.run_text(),
            _ => None,
        }
    }

    /// The text, in brackets when it is more than one character.
    fn grouped(&self) -> String {
        let t = self.linear();
        let simple = t.chars().count() <= 1
            || t.chars().all(|c| c.is_ascii_digit() || SUPER.contains(c) || SUB.contains(c))
            || (matches!(self, Math::Script { .. }) && t.chars().all(|c| c.is_alphanumeric() || SUPER.contains(c) || SUB.contains(c)));
        if simple || matches!(self, Math::Delimited { .. }) { t } else { format!("({t})") }
    }

    fn grouped_if_row(&self) -> String {
        let t = self.linear();
        if t.chars().count() <= 1 || matches!(self, Math::Delimited { .. }) { t } else { format!("({t})") }
    }
}

const FUNCTIONS: [&str; 16] = ["sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh", "log", "ln", "exp", "lim"];
const NARY: &str = "∑∏∐∫∬∭∮∯∰⋃⋂⋁⋀";

fn unscript(c: char, table_from: &str, table_to: &str) -> Option<char> {
    table_to.chars().position(|t| t == c).and_then(|i| table_from.chars().nth(i))
}

/// A cursor over the characters of a formula typed in UnicodeMath's linear form.
struct Linear {
    chars: Vec<char>,
    at: usize,
}

impl Linear {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    /// Items up to `close` (not taken) or the end.
    fn row(&mut self, close: &[char]) -> Vec<Math> {
        let mut items: Vec<Math> = Vec::new();
        while let Some(c) = self.peek() {
            if close.contains(&c) {
                break;
            }
            match c {
                '/' | '¦' | '⊘' | '∕' => {
                    self.at += 1;
                    let den = self.operand();
                    let num = items.pop().map(unwrap_group).unwrap_or(Math::Row(Vec::new()));
                    let kind = match c {
                        '/' => FracKind::Bar,
                        '¦' => FracKind::NoBar,
                        '⊘' => FracKind::Skew,
                        _ => FracKind::Linear,
                    };
                    items.push(Math::Frac { num: Box::new(num), den: Box::new(unwrap_group(den)), kind });
                }
                '^' | '_' => {
                    self.at += 1;
                    let arg = unwrap_group(self.operand());
                    let base = items.pop().unwrap_or(Math::Row(Vec::new()));
                    items.push(add_script(base, c == '^', arg));
                }
                _ if unscript(c, SUPER_FROM, SUPER).is_some() || unscript(c, SUB_FROM, SUB).is_some() => {
                    let mut sup = String::new();
                    let mut sub = String::new();
                    while let Some(d) = self.peek() {
                        if let Some(n) = unscript(d, SUPER_FROM, SUPER) {
                            sup.push(n);
                        } else if let Some(n) = unscript(d, SUB_FROM, SUB) {
                            sub.push(n);
                        } else {
                            break;
                        }
                        self.at += 1;
                    }
                    let mut base = items.pop().unwrap_or(Math::Row(Vec::new()));
                    if !sub.is_empty() {
                        base = add_script(base, false, Math::Text(sub));
                    }
                    if !sup.is_empty() {
                        base = add_script(base, true, Math::Text(sup));
                    }
                    items.push(base);
                }
                _ => {
                    let item = self.primary();
                    items.push(item);
                }
            }
        }
        merge_text(items)
    }

    /// One operand: a bracketed group, a number, or one character.
    fn operand(&mut self) -> Math {
        match self.peek() {
            Some('(') | Some('√') | Some('■') | Some('█') => self.primary(),
            Some(c) if c.is_ascii_digit() => {
                let mut t = String::new();
                while let Some(d) = self.peek().filter(|d| d.is_ascii_digit() || *d == '.') {
                    t.push(d);
                    self.at += 1;
                }
                Math::Text(t)
            }
            Some(c) if NARY.contains(c) => self.primary(),
            Some(c) => {
                self.at += 1;
                Math::Text(c.to_string())
            }
            None => Math::Row(Vec::new()),
        }
    }

    fn primary(&mut self) -> Math {
        let c = self.peek().expect("a character");
        match c {
            '\\' => {
                self.at += 1;
                let start = self.at;
                while self.peek().is_some_and(|d| d.is_alphabetic()) {
                    self.at += 1;
                }
                let cmd: String = self.chars[start..self.at].iter().collect();
                // If followed by space (Math AutoCorrect delimiter), consume it
                if self.peek() == Some(' ') {
                    self.at += 1;
                }
                match cmd.as_str() {
                    "sqrt" => {
                        let body = self.operand();
                        Math::Radical { degree: None, body: Box::new(unwrap_group(body)) }
                    }
                    "sum" => {
                        let (mut sub, mut sup) = (None, None);
                        while let Some(m @ ('_' | '^')) = self.peek() {
                            self.at += 1;
                            let arg = Box::new(unwrap_group(self.operand()));
                            if m == '_' { sub = Some(arg) } else { sup = Some(arg) }
                        }
                        let body = if self.peek().is_some_and(|d| !")]}⟩".contains(d)) { unwrap_group(self.operand()) } else { Math::Row(Vec::new()) };
                        Math::Nary { op: "∑".into(), sub, sup, body: Box::new(body), stacked: true }
                    }
                    "int" => {
                        let (mut sub, mut sup) = (None, None);
                        while let Some(m @ ('_' | '^')) = self.peek() {
                            self.at += 1;
                            let arg = Box::new(unwrap_group(self.operand()));
                            if m == '_' { sub = Some(arg) } else { sup = Some(arg) }
                        }
                        let body = if self.peek().is_some_and(|d| !")]}⟩".contains(d)) { unwrap_group(self.operand()) } else { Math::Row(Vec::new()) };
                        Math::Nary { op: "∫".into(), sub, sup, body: Box::new(body), stacked: false }
                    }
                    _ => {
                        if let Some(symbol) = crate::latex::lookup_latex_symbol(&cmd) {
                            Math::Text(symbol.into())
                        } else {
                            Math::Text(format!("\\{cmd}"))
                        }
                    }
                }
            }
            '(' => {
                self.at += 1;
                let inner = self.row(&[')']);
                if self.peek() == Some(')') {
                    self.at += 1;
                }
                Math::Delimited { open: "(".into(), close: ")".into(), sep: "|".into(), items: vec![row_of(inner)] }
            }
            '[' | '{' | '⟨' => {
                let close = match c {
                    '[' => ']',
                    '{' => '}',
                    _ => '⟩',
                };
                self.at += 1;
                let inner = self.row(&[close]);
                if self.peek() == Some(close) {
                    self.at += 1;
                }
                Math::Delimited { open: c.to_string(), close: close.to_string(), sep: "|".into(), items: vec![row_of(inner)] }
            }
            '√' | '∛' | '∜' => {
                self.at += 1;
                let body = self.operand();
                let mut degree = match c {
                    '∛' => Some(Box::new(Math::Text("3".into()))),
                    '∜' => Some(Box::new(Math::Text("4".into()))),
                    _ => None,
                };
                // `√(n&x)`: the degree and the body.
                let body = match body {
                    Math::Delimited { items, .. } if matches!(items.first(), Some(Math::Row(r)) if r.iter().any(|m| matches!(m, Math::Text(t) if t.contains('&')))) => {
                        let flat = Math::Row(items).linear();
                        match flat.split_once('&') {
                            Some((d, b)) => {
                                degree = Some(Box::new(parse_linear(d)));
                                parse_linear(b)
                            }
                            None => parse_linear(&flat),
                        }
                    }
                    other => unwrap_group(other),
                };
                Math::Radical { degree, body: Box::new(body) }
            }
            '█' => {
                self.at += 1;
                if self.peek() != Some('(') {
                    return Math::Text("█".into());
                }
                self.at += 1;
                let mut rows: Vec<Math> = Vec::new();
                loop {
                    let part = self.row(&[')', '@']);
                    rows.push(row_of(part));
                    match self.peek() {
                        Some('@') => self.at += 1,
                        Some(')') => {
                            self.at += 1;
                            break;
                        }
                        _ => break,
                    }
                }
                Math::EqArray(rows)
            }
            '■' => {
                self.at += 1;
                if self.peek() != Some('(') {
                    return Math::Text("■".into());
                }
                self.at += 1;
                let mut rows: Vec<Vec<Math>> = vec![vec![]];
                let mut cell: Vec<Math> = Vec::new();
                loop {
                    let part = self.row(&[')', '&', '@']);
                    cell.extend(part);
                    match self.peek() {
                        Some('&') => {
                            self.at += 1;
                            rows.last_mut().expect("a row").push(row_of(std::mem::take(&mut cell)));
                        }
                        Some('@') => {
                            self.at += 1;
                            rows.last_mut().expect("a row").push(row_of(std::mem::take(&mut cell)));
                            rows.push(vec![]);
                        }
                        Some(')') => {
                            self.at += 1;
                            rows.last_mut().expect("a row").push(row_of(std::mem::take(&mut cell)));
                            break;
                        }
                        _ => {
                            rows.last_mut().expect("a row").push(row_of(std::mem::take(&mut cell)));
                            break;
                        }
                    }
                }
                Math::Matrix(rows)
            }
            _ if NARY.contains(c) => {
                self.at += 1;
                let (mut sub, mut sup) = (None, None);
                while let Some(m @ ('_' | '^')) = self.peek() {
                    self.at += 1;
                    let arg = Box::new(unwrap_group(self.operand()));
                    if m == '_' { sub = Some(arg) } else { sup = Some(arg) }
                }
                let body = if self.peek().is_some_and(|d| !")]}⟩".contains(d)) { unwrap_group(self.operand()) } else { Math::Row(Vec::new()) };
                Math::Nary { op: c.to_string(), sub, sup, body: Box::new(body), stacked: stacked_by_default(&c.to_string()) }
            }
            _ if c.is_alphabetic() => {
                let start = self.at;
                while self.peek().is_some_and(|d| d.is_alphabetic()) {
                    self.at += 1;
                }
                let word: String = self.chars[start..self.at].iter().collect();
                for f in FUNCTIONS.iter().rev() {
                    if word == *f {
                        // A function name takes the operand after it.
                        let arg = if self.peek().is_some_and(|d| !")]}⟩/^_&@".contains(d) && d != ' ') { unwrap_group(self.operand()) } else { Math::Row(Vec::new()) };
                        return Math::Function { name: Box::new(Math::Text(word)), arg: Box::new(arg) };
                    }
                }
                // Letters go one at a time (the last may take a script or a fraction).
                if word.chars().count() > 1 {
                    let last = word.chars().last().expect("a letter");
                    self.at -= 1;
                    let head: String = word.chars().take(word.chars().count() - 1).collect();
                    // The last letter is left for the next turn: the head is returned now.
                    let _ = last;
                    return Math::Text(head);
                }
                Math::Text(word)
            }
            _ => {
                self.at += 1;
                Math::Text(c.to_string())
            }
        }
    }
}

fn row_of(mut items: Vec<Math>) -> Math {
    if items.len() == 1 { items.remove(0) } else { Math::Row(items) }
}

/// A bracketed group used as an argument loses its round brackets.
fn unwrap_group(m: Math) -> Math {
    match m {
        Math::Delimited { open, close, mut items, .. } if open == "(" && close == ")" && items.len() == 1 => items.remove(0),
        other => other,
    }
}

fn add_script(base: Math, sup: bool, arg: Math) -> Math {
    match base {
        Math::Script { base, sub, sup: old_sup } if (sup && old_sup.is_none()) || (!sup && sub.is_none()) => {
            if sup { Math::Script { base, sub, sup: Some(Box::new(arg)) } } else { Math::Script { base, sub: Some(Box::new(arg)), sup: old_sup } }
        }
        base if sup => Math::Script { base: Box::new(base), sub: None, sup: Some(Box::new(arg)) },
        base => Math::Script { base: Box::new(base), sub: Some(Box::new(arg)), sup: None },
    }
}

/// Adjacent text joined.
fn merge_text(items: Vec<Math>) -> Vec<Math> {
    let mut out: Vec<Math> = Vec::new();
    for item in items {
        match (out.last_mut(), item) {
            (Some(Math::Text(a)), Math::Text(b)) => a.push_str(&b),
            (_, item) => out.push(item),
        }
    }
    out
}

/// A formula typed in UnicodeMath's linear form (`a/b`, `x^2`, `√(x+1)`, `∑_(i=1)^n i`, `■(a&b@c&d)`)
/// as a tree. What it does not know is kept as text.
pub fn parse_linear(text: &str) -> Math {
    let mut cursor = Linear { chars: text.chars().collect(), at: 0 };
    let mut items = Vec::new();
    while cursor.peek().is_some() {
        items.extend(cursor.row(&[]));
        // A stray closing bracket is text.
        if let Some(c) = cursor.peek() {
            cursor.at += 1;
            items.push(Math::Text(c.to_string()));
        }
    }
    row_of(merge_text(items))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Box<Math> {
        Box::new(Math::Text(s.to_string()))
    }

    #[test]
    fn linear_forms() {
        assert_eq!(Math::Frac { num: t("a"), den: t("b+c"), kind: FracKind::Bar }.linear(), "a/(b+c)");
        assert_eq!(Math::Script { base: t("x"), sub: None, sup: Some(t("2")) }.linear(), "x²");
        assert_eq!(Math::Script { base: t("x"), sub: Some(t("i")), sup: Some(t("n+1")) }.linear(), "xᵢⁿ⁺¹");
        assert_eq!(Math::Script { base: t("x"), sub: None, sup: Some(t("a")) }.linear(), "x^a");
        assert_eq!(Math::Radical { degree: None, body: t("x+1") }.linear(), "√(x+1)");
        assert_eq!(Math::Nary { op: "∑".into(), sub: Some(t("i=1")), sup: Some(t("n")), body: t("i"), stacked: true }.linear(), "∑_(i=1)^ni");
    }

    #[test]
    fn linear_form_is_read_back() {
        for text in ["a/(b+c)", "x²", "x^a", "√(x+1)", "√x", "∑_(i=1)^n x", "a=b+c", "■(1&2@3&4)", "█(x&=1@yy&=2)", "sin x", "(a+b)/2"] {
            let tree = parse_linear(text);
            let back = tree.linear();
            // Reading what was written gives the same tree.
            assert_eq!(parse_linear(&back), tree, "{text} -> {back}");
        }
        assert_eq!(parse_linear("a/b"), Math::Frac { num: t("a"), den: t("b"), kind: FracKind::Bar });
        assert_eq!(parse_linear("a⊘b"), Math::Frac { num: t("a"), den: t("b"), kind: FracKind::Skew });
        assert_eq!(parse_linear("a∕b"), Math::Frac { num: t("a"), den: t("b"), kind: FracKind::Linear });
        assert_eq!(parse_linear("(a+b)/2").linear(), "(a+b)/2");
        assert_eq!(parse_linear("x^2"), Math::Script { base: t("x"), sub: None, sup: Some(t("2")) });
        assert_eq!(parse_linear("x_i^2"), Math::Script { base: t("x"), sub: Some(t("i")), sup: Some(t("2")) });
        assert_eq!(parse_linear("√x"), Math::Radical { degree: None, body: t("x") });
        assert!(matches!(parse_linear("■(a&b@c&d)"), Math::Matrix(rows) if rows.len() == 2 && rows[0].len() == 2));
    }

    #[test]
    fn latex_parse_and_to_latex() {
        // Fraction
        let frac = parse_latex(r"\frac{a}{b+c}");
        assert_eq!(frac, Math::Frac { num: t("a"), den: t("b+c"), kind: FracKind::Bar });
        assert_eq!(frac.to_latex(), r"\frac{a}{b+c}");

        // Scripts
        let scr = parse_latex("x^2 + y_i^3");
        assert_eq!(scr.linear(), "x²+yᵢ³");
        assert_eq!(scr.to_latex(), r"x^{2}+y_{i}^{3}");

        // Square root and nth root
        let sqrt = parse_latex(r"\sqrt{x+1}");
        assert_eq!(sqrt, Math::Radical { degree: None, body: t("x+1") });
        assert_eq!(sqrt.to_latex(), r"\sqrt{x+1}");

        let root3 = parse_latex(r"\sqrt[3]{x+y}");
        assert_eq!(root3, Math::Radical { degree: Some(t("3")), body: t("x+y") });
        assert_eq!(root3.to_latex(), r"\sqrt[3]{x+y}");

        // Big operators
        let sum = parse_latex(r"\sum_{i=1}^{n} x_i");
        assert_eq!(sum.linear(), "∑_(i=1)^nxᵢ");
        assert_eq!(sum.to_latex(), r"\sum_{i=1}^{n}x_{i}");

        // Greek letters & symbols
        let greek = parse_latex(r"\alpha + \beta = \gamma \pm \infty");
        assert_eq!(greek.linear(), "α+β=γ±∞");
        assert_eq!(greek.to_latex(), r"\alpha +\beta =\gamma \pm \infty ");

        // Delimiters
        let delim = parse_latex(r"\left( \frac{a}{b} \right)");
        assert!(matches!(delim, Math::Delimited { ref open, ref close, .. } if open == "(" && close == ")"));

        // Matrix
        let mat = parse_latex(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}");
        assert!(matches!(mat, Math::Delimited { ref open, ref close, ref items, .. } if open == "(" && close == ")" && items.len() == 1 && matches!(items[0], Math::Matrix(_))));

        // Quadratic formula roundtrip
        let quad = parse_latex(r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}");
        assert!(quad.linear().contains("√"));
        assert!(quad.to_latex().contains(r"\frac"));
        assert!(quad.to_latex().contains(r"\sqrt"));

        // Binomial coefficient
        let bin = parse_latex(r"\binom{n}{k}");
        assert_eq!(bin.to_latex(), r"\binom{n}{k}");

        // Dots
        let dots = parse_latex(r"1 + 2 + \dots + n");
        assert!(dots.linear().contains("…"));
    }

    #[test]
    fn linear_math_autocorrect() {
        let tree = parse_linear(r"\alpha+\beta=\gamma\pm\infty");
        assert_eq!(tree.linear(), "α+β=γ±∞");

        let spaced = parse_linear(r"\alpha \beta \gamma");
        assert_eq!(spaced.linear(), "αβγ");

        let sqrt = parse_linear(r"\sqrt(x+1)");
        assert_eq!(sqrt, Math::Radical { degree: None, body: t("x+1") });

        let sum = parse_linear(r"\sum_(i=1)^n x_i");
        assert!(sum.linear().starts_with("∑_(i=1)^n"));
    }
}

