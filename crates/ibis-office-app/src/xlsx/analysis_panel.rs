// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > データ分析… (the Analysis ToolPak): the tool, its input
//! ranges and options, and the output cell; the tables are made by
//! `ibis_doc_model::analysis` and written in one step (one undo).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::analysis::{self, Block, Eval, Series};
use ibis_doc_model::{CellEdit, CellRange, CellValue, Command, SheetCommand, SlideId, StyleChange};

use super::sheet_ui::parse_reference;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tool {
    AnovaSingle,
    AnovaTwoWith,
    AnovaTwoWithout,
    Correlation,
    Covariance,
    Descriptive,
    ExponentialSmoothing,
    FTest,
    Fourier,
    Histogram,
    MovingAverage,
    Random,
    Rank,
    Regression,
    Sampling,
    TPaired,
    TEqual,
    TUnequal,
    ZTest,
}

impl Tool {
    pub(crate) const ALL: [Tool; 19] = [
        Tool::AnovaSingle,
        Tool::AnovaTwoWith,
        Tool::AnovaTwoWithout,
        Tool::Correlation,
        Tool::Covariance,
        Tool::Descriptive,
        Tool::ExponentialSmoothing,
        Tool::FTest,
        Tool::Fourier,
        Tool::Histogram,
        Tool::MovingAverage,
        Tool::Random,
        Tool::Rank,
        Tool::Regression,
        Tool::Sampling,
        Tool::TPaired,
        Tool::TEqual,
        Tool::TUnequal,
        Tool::ZTest,
    ];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Tool::AnovaSingle => "xlsx.analysis.tool.anova-single",
            Tool::AnovaTwoWith => "xlsx.analysis.tool.anova-two-with",
            Tool::AnovaTwoWithout => "xlsx.analysis.tool.anova-two-without",
            Tool::Correlation => "xlsx.analysis.tool.correlation",
            Tool::Covariance => "xlsx.analysis.tool.covariance",
            Tool::Descriptive => "xlsx.analysis.tool.descriptive",
            Tool::ExponentialSmoothing => "xlsx.analysis.tool.exponential-smoothing",
            Tool::FTest => "xlsx.analysis.tool.f-test",
            Tool::Fourier => "xlsx.analysis.tool.fourier",
            Tool::Histogram => "xlsx.analysis.tool.histogram",
            Tool::MovingAverage => "xlsx.analysis.tool.moving-average",
            Tool::Random => "xlsx.analysis.tool.random",
            Tool::Rank => "xlsx.analysis.tool.rank",
            Tool::Regression => "xlsx.analysis.tool.regression",
            Tool::Sampling => "xlsx.analysis.tool.sampling",
            Tool::TPaired => "xlsx.analysis.tool.t-paired",
            Tool::TEqual => "xlsx.analysis.tool.t-equal",
            Tool::TUnequal => "xlsx.analysis.tool.t-unequal",
            Tool::ZTest => "xlsx.analysis.tool.z-test",
        }
    }

    /// The input range's caption, and the second range's (if the tool has one).
    fn ranges(self) -> (&'static str, Option<&'static str>) {
        match self {
            Tool::Regression => ("xlsx.analysis.field.input-y", Some("xlsx.analysis.field.input-x")),
            Tool::TPaired | Tool::TEqual | Tool::TUnequal | Tool::ZTest | Tool::FTest => ("xlsx.analysis.field.variable-1", Some("xlsx.analysis.field.variable-2")),
            Tool::Histogram => ("xlsx.analysis.field.input", Some("xlsx.analysis.field.bins")),
            Tool::Random => ("", Some("xlsx.analysis.field.discrete-range")),
            _ => ("xlsx.analysis.field.input", None),
        }
    }

    /// Whether the data can be grouped by columns or rows.
    fn grouped(self) -> bool {
        matches!(self, Tool::AnovaSingle | Tool::Correlation | Tool::Covariance | Tool::Descriptive | Tool::Rank)
    }

    /// Whether "labels in the first row" applies.
    fn labels(self) -> bool {
        !matches!(self, Tool::AnovaTwoWith | Tool::Sampling | Tool::Random | Tool::Fourier)
    }

    /// The check boxes (caption ids).
    fn flags(self) -> &'static [&'static str] {
        match self {
            Tool::Histogram => &["xlsx.analysis.flag.pareto", "xlsx.analysis.flag.cumulative", "xlsx.analysis.flag.chart-output"],
            Tool::Regression => &["xlsx.analysis.flag.constant-zero", "xlsx.analysis.flag.residuals", "xlsx.analysis.flag.standard-residuals", "xlsx.analysis.flag.chart-output"],
            Tool::MovingAverage | Tool::ExponentialSmoothing => &["xlsx.analysis.flag.standard-errors", "xlsx.analysis.flag.chart-output"],
            Tool::Fourier => &["xlsx.analysis.flag.inverse"],
            _ => &[],
        }
    }

    /// The numbers to type: caption id and the value to start with.
    fn params(self, dist: Dist) -> Vec<(&'static str, &'static str)> {
        match self {
            Tool::AnovaSingle | Tool::AnovaTwoWithout | Tool::FTest => vec![("xlsx.analysis.field.alpha", "0.05")],
            Tool::AnovaTwoWith => vec![("xlsx.analysis.field.rows-per-sample", "2"), ("xlsx.analysis.field.alpha", "0.05")],
            Tool::Descriptive => vec![("xlsx.analysis.field.confidence-level", ""), ("xlsx.analysis.field.kth-largest", ""), ("xlsx.analysis.field.kth-smallest", "")],
            Tool::Regression => vec![("xlsx.analysis.field.confidence-level", "95")],
            Tool::TPaired | Tool::TEqual | Tool::TUnequal => vec![("xlsx.analysis.field.hypothesized-diff", "0"), ("xlsx.analysis.field.alpha", "0.05")],
            Tool::ZTest => vec![("xlsx.analysis.field.hypothesized-diff", "0"), ("xlsx.analysis.field.alpha", "0.05"), ("xlsx.analysis.field.variance-1", ""), ("xlsx.analysis.field.variance-2", "")],
            Tool::MovingAverage => vec![("xlsx.analysis.field.interval", "3")],
            Tool::ExponentialSmoothing => vec![("xlsx.analysis.field.damping", "0.3")],
            Tool::Sampling => vec![("xlsx.analysis.field.period", ""), ("xlsx.analysis.field.random-count", "")],
            Tool::Random => {
                let mut v = vec![("xlsx.analysis.field.variables", "1"), ("xlsx.analysis.field.count", "10")];
                v.extend(dist.params());
                v.push(("xlsx.analysis.field.seed", ""));
                v
            }
            _ => Vec::new(),
        }
    }
}

/// The ▲▼ of a number box of the tools: counts step by 1, probabilities (α, damping, p) by 0.01, percentages by 1.
/// Free values (means, variances, bounds, seeds…) have none.
fn param_spinner(label: &str) -> Option<crate::shell::chrome::SpinnerSpec> {
    use crate::shell::chrome::SpinnerSpec;
    match label {
        "xlsx.analysis.field.rows-per-sample"
        | "xlsx.analysis.field.kth-largest"
        | "xlsx.analysis.field.kth-smallest"
        | "xlsx.analysis.field.interval"
        | "xlsx.analysis.field.period"
        | "xlsx.analysis.field.random-count"
        | "xlsx.analysis.field.variables"
        | "xlsx.analysis.field.count"
        | "xlsx.analysis.field.trials"
        | "xlsx.analysis.field.repeat-each"
        | "xlsx.analysis.field.repeat-sequence" => Some(SpinnerSpec::new(1.0).min(1.0).integer_only()),
        "xlsx.analysis.field.alpha" | "xlsx.analysis.field.damping" | "xlsx.analysis.field.p-value" => Some(SpinnerSpec::new(0.01).range(0.0, 1.0)),
        "xlsx.analysis.field.confidence-level" => Some(SpinnerSpec::new(1.0).range(0.0, 100.0)),
        _ => None,
    }
}

/// The distribution of 乱数発生.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dist {
    Uniform,
    Normal,
    Bernoulli,
    Binomial,
    Poisson,
    Patterned,
    Discrete,
}

impl Dist {
    pub(crate) const ALL: [Dist; 7] = [Dist::Uniform, Dist::Normal, Dist::Bernoulli, Dist::Binomial, Dist::Poisson, Dist::Patterned, Dist::Discrete];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Dist::Uniform => "xlsx.analysis.dist.uniform",
            Dist::Normal => "xlsx.analysis.dist.normal",
            Dist::Bernoulli => "xlsx.analysis.dist.bernoulli",
            Dist::Binomial => "xlsx.analysis.dist.binomial",
            Dist::Poisson => "xlsx.analysis.dist.poisson",
            Dist::Patterned => "xlsx.analysis.dist.patterned",
            Dist::Discrete => "xlsx.analysis.dist.discrete",
        }
    }

    fn params(self) -> Vec<(&'static str, &'static str)> {
        match self {
            Dist::Uniform => vec![("xlsx.analysis.field.between", "0"), ("xlsx.analysis.field.and", "1")],
            Dist::Normal => vec![("xlsx.analysis.field.mean", "0"), ("xlsx.analysis.field.standard-deviation", "1")],
            Dist::Bernoulli => vec![("xlsx.analysis.field.p-value", "0.5")],
            Dist::Binomial => vec![("xlsx.analysis.field.p-value", "0.5"), ("xlsx.analysis.field.trials", "10")],
            Dist::Poisson => vec![("xlsx.analysis.field.lambda", "1")],
            Dist::Patterned => vec![("xlsx.analysis.field.from", "1"), ("xlsx.analysis.field.to", "10"), ("xlsx.analysis.field.step", "1"), ("xlsx.analysis.field.repeat-each", "1"), ("xlsx.analysis.field.repeat-sequence", "1")],
            Dist::Discrete => Vec::new(),
        }
    }
}

/// The most number boxes any tool shows.
const PARAM_BOXES: usize = 9;

pub(crate) struct AnalysisPanel {
    pub(crate) tool: Tool,
    pub(crate) dist: Dist,
    pub(crate) input: Entity<InputState>,
    pub(crate) input2: Entity<InputState>,
    pub(crate) output: Entity<InputState>,
    pub(crate) params: Vec<Entity<InputState>>,
    pub(crate) by_rows: bool,
    pub(crate) labels: bool,
    pub(crate) new_sheet: bool,
    pub(crate) flags: [bool; 4],
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
    pub(crate) slide: SlideId,
}

/// A range typed as `A1:C10` or `Sheet2!A1:C10`: the sheet's slide and
/// name, the sheet itself and the cells.
pub(super) struct Resolved {
    pub(super) slide: SlideId,
    pub(super) name: String,
    pub(super) sheet: std::sync::Arc<ibis_doc_model::Sheet>,
    pub(super) range: CellRange,
}

fn number_of(text: &str) -> Option<f64> {
    let t = text.trim();
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

pub(super) fn range_text(r: CellRange) -> String {
    format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row))
}

/// One row or column of the range: (cells in order, values).
fn vector(sheet: &ibis_doc_model::Sheet, r: CellRange, labels: bool) -> (Vec<(u32, u32)>, Option<String>) {
    let rowwise = r.first_row == r.last_row && r.last_column > r.first_column;
    let skip = u32::from(labels);
    let cells: Vec<(u32, u32)> = if rowwise { (r.first_column + skip..=r.last_column).map(|c| (r.first_row, c)).collect() } else { (r.first_row + skip..=r.last_row).map(|row| (row, r.first_column)).collect() };
    let name = labels.then(|| match sheet.cell(r.first_row, r.first_column).map(|c| &c.value) {
        Some(CellValue::Text(t)) => Some(t.clone()),
        _ => None,
    });
    (cells, name.flatten())
}

fn numbers_at(sheet: &ibis_doc_model::Sheet, cells: &[(u32, u32)]) -> Vec<Option<f64>> {
    cells
        .iter()
        .map(|(r, c)| match sheet.cell(*r, *c).map(|x| &x.value) {
            Some(CellValue::Number(n)) => Some(*n),
            _ => None,
        })
        .collect()
}

impl SlideEditor {
    /// The sheet a reference names (the current one if none is named), or a Defined Name's target range.
    pub(super) fn resolve_range(&self, text: &str) -> Result<Resolved, String> {
        let reference = parse_reference(text).ok_or_else(|| format("xlsx.analysis.message.bad-range", &[("text", &text.trim())]))?;
        let workbook = self.presentation().workbook.as_ref();
        if let Some(name) = &reference.sheet
            && workbook.and_then(|w| w.sheets.iter().find(|s| s.name.eq_ignore_ascii_case(name)).and_then(|s| s.slide)).is_none()
        {
            return Err(format("xlsx.analysis.message.no-sheet", &[("name", name)]));
        }
        let (slide, range) = self.resolve_reference(&reference).ok_or_else(|| format("xlsx.analysis.message.bad-range", &[("text", &text.trim())]))?;
        let name = workbook
            .and_then(|w| w.sheets.iter().find(|s| s.slide == Some(slide)).map(|s| s.name.clone()))
            .unwrap_or_default();
        let sheet = self.presentation().slide(slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide).ok_or_else(|| text_of("xlsx.analysis.message.no-data"))?;
        // Whole columns or rows reach the data's end.
        let (rows, columns) = sheet.cells.iter().fold((0, 0), |(r, c), x| (r.max(x.row + 1), c.max(x.column + 1)));
        let range = CellRange { last_row: range.last_row.min(rows.saturating_sub(1).max(range.first_row)), last_column: range.last_column.min(columns.saturating_sub(1).max(range.first_column)), ..range };
        Ok(Resolved { slide, name, sheet: sheet.clone(), range })
    }

    pub(crate) fn set_analysis_range_from_selection(&mut self, field: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected_range() else { return };
        let val = if field == "output" {
            ibis_doc_model::cell_name(r.first_column, r.first_row)
        } else {
            range_text(r)
        };
        let input = self.wb().panels.analysis.as_ref().and_then(|p| match field {
            "input" => Some(p.input.clone()),
            "input2" => Some(p.input2.clone()),
            "output" => Some(p.output.clone()),
            _ => None,
        });
        if let Some(input) = input {
            input.update(cx, |i, cx| i.set_value(val, window, cx));
            cx.notify();
        }
    }

    pub(crate) fn open_analysis_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let range = self.selected_range();
        let multi = range.is_some_and(|r| r.first_row != r.last_row || r.first_column != r.last_column);
        let input_text = range.filter(|_| multi).map(range_text).unwrap_or_default();
        let output_text = range.filter(|_| multi).map(|r| ibis_doc_model::cell_name(r.last_column + 2, r.first_row)).unwrap_or_else(|| ibis_doc_model::cell_name(4, 0));
        let make = |value: &str, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            let v = value.to_string();
            state.update(cx, |i, cx| i.set_value(v, window, cx));
            state
        };
        let tool = Tool::Descriptive;
        let dist = Dist::Uniform;
        let mut params = Vec::new();
        for i in 0..PARAM_BOXES {
            let default = tool.params(dist).get(i).map_or("", |p| p.1);
            params.push(make(default, window, cx));
        }
        let panel = AnalysisPanel {
            tool,
            dist,
            input: make(&input_text, window, cx),
            input2: make("", window, cx),
            output: make(&output_text, window, cx),
            params,
            by_rows: false,
            labels: false,
            new_sheet: false,
            flags: [false; 4],
            message: String::new(),
            ui: Default::default(),
            slide: self.current_slide,
        };
        let state = self.wb_mut();
        state.custom_format = None;
        state.page_setup = false;
        state.names = None;
        state.validation = None;
        state.find = None;
        state.link = None;
        state.conditional_edit = None;
        state.sort_edit = None;
        state.close_settings();
        state.panels.analysis = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    fn analysis_message(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.analysis.as_mut() {
            p.message = message;
        }
        cx.notify();
    }

    /// Choose the tool (or the distribution): its number boxes take their
    /// starting values.
    pub(crate) fn analysis_choose(&mut self, tool: Tool, dist: Dist, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.wb_mut().panels.analysis.as_mut() else { return };
        p.tool = tool;
        p.dist = dist;
        p.flags = [false; 4];
        p.message.clear();
        let defaults = tool.params(dist);
        for (i, state) in p.params.iter().enumerate() {
            let value = defaults.get(i).map_or("", |d| d.1).to_string();
            state.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn apply_analysis_panel(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        match self.run_analysis(cx) {
            Ok(done) => self.analysis_message(done, cx),
            Err(message) => self.analysis_message(message, cx),
        }
    }

    fn run_analysis(&mut self, cx: &mut Context<Self>) -> Result<String, String> {
        let p = self.sheet_state().and_then(|s| s.panels.analysis.as_ref()).ok_or_default()?;
        if p.slide != self.current_slide && self.presentation().slide(p.slide).is_some() {
            return Err(text_of("_xlsx.the-sheet-has-changed-reopen-it"));
        }
        let (tool, dist, by_rows, flags, new_sheet) = (p.tool, p.dist, p.by_rows, p.flags, p.new_sheet);
        let labels = p.labels && p.tool.labels();
        let read = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let (input_text, input2_text, output_text) = (read(&p.input), read(&p.input2), read(&p.output));
        let params: Vec<String> = p.params.iter().map(read).collect();
        let param = |i: usize| -> Option<f64> { number_of(&params[i]) };
        let need = |i: usize| -> Result<f64, String> { param(i).ok_or_else(|| format("xlsx.analysis.message.bad-number", &[("name", &text(tool.params(dist).get(i).map_or("", |d| d.0)))])) };
        let blank = |i: usize| params[i].is_empty();

        let column_word = if by_rows { "xlsx.analysis.out.row" } else { "xlsx.analysis.out.column" };
        let name_of = |spaced: bool, id: &str| {
            let id = id.to_string();
            move |i: usize| format!("{}{}{}", text(&id), if spaced { " " } else { "" }, i + 1)
        };
        let l = |id: &str| text(id).to_string();
        let out_labels: &dyn Fn(&str) -> String = &l;

        // Where the table goes: either a cell range on an existing sheet or A1 on a newly inserted worksheet.
        let (out_slide, origin, add_sheet) = if new_sheet {
            if self.workbook_protection().is_some_and(|p| p.lock_structure) {
                return Err(text_of("_xlsx.workbook-is-protected-and-cannot-be-changed"));
            }
            let Some(wb) = self.presentation().workbook.clone() else {
                return Err(text_of("xlsx.analysis.message.no-data"));
            };
            let name = if output_text.is_empty() {
                let mut n = wb.sheets.len() + 1;
                loop {
                    let candidate = format!("Sheet{n}");
                    if ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &candidate, None) {
                        break candidate;
                    }
                    n += 1;
                }
            } else {
                if !ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &output_text, None) {
                    return Err(format("xlsx.analysis.message.bad-range", &[("text", &output_text)]));
                }
                output_text.clone()
            };
            let index = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide)).map_or(wb.sheets.len(), |i| i + 1);
            let (slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
            (slide, (0u32, 0u32), Some((index, name, slide, shape)))
        } else {
            let out_ref = self.resolve_range(&output_text)?;
            (out_ref.slide, (out_ref.range.first_row, out_ref.range.first_column), None)
        };

        let first = if tool == Tool::Random { None } else { Some(self.resolve_range(&input_text)?) };
        let second = || -> Result<Resolved, String> { self.resolve_range(&input2_text) };
        let vec_of = |r: &Resolved, default_id: &str| -> (Vec<Option<f64>>, String) {
            let (cells, name) = vector(&r.sheet, r.range, labels);
            (numbers_at(&r.sheet, &cells), name.unwrap_or_else(|| text(default_id).to_string()))
        };
        let seed_index = if tool == Tool::Random { 2 + dist.params().len() } else { PARAM_BOXES };
        let seed = || -> u64 { params.get(seed_index).and_then(|t| number_of(t)).map_or_else(|| crate::clock::unix_millis() ^ 0x5DEE_CE66D, |v| v as u64) };

        let ev = Eval::new(self.presentation());
        let output = match tool {
            Tool::Descriptive => {
                let r = first.as_ref().ok_or_default()?;
                let block = Block::read(&r.sheet, r.range, by_rows, labels, &name_of(false, column_word));
                let opt = analysis::Descriptive {
                    confidence: if blank(0) { None } else { Some(need(0)?) },
                    kth_largest: if blank(1) { None } else { Some(need(1)? as usize) },
                    kth_smallest: if blank(2) { None } else { Some(need(2)? as usize) },
                };
                analysis::descriptive(&block, &opt, out_labels, &ev)
            }
            Tool::Correlation | Tool::Covariance => {
                let r = first.as_ref().ok_or_default()?;
                let block = Block::read(&r.sheet, r.range, by_rows, labels, &name_of(false, column_word));
                analysis::correlation_or_covariance(&block, tool == Tool::Covariance)
            }
            Tool::Rank => {
                let r = first.as_ref().ok_or_default()?;
                let block = Block::read(&r.sheet, r.range, by_rows, labels, &name_of(false, column_word));
                analysis::rank_percentile(&block, out_labels)
            }
            Tool::Histogram => {
                let r = first.as_ref().ok_or_default()?;
                let block = Block::read(&r.sheet, r.range, false, labels, &name_of(false, column_word));
                let data: Vec<f64> = block.cols.iter().flatten().flatten().copied().collect();
                let bins = if input2_text.is_empty() {
                    None
                } else {
                    let b = second()?;
                    let (cells, _) = vector(&b.sheet, b.range, labels);
                    Some(numbers_at(&b.sheet, &cells).into_iter().flatten().collect::<Vec<f64>>())
                };
                analysis::histogram(&data, &analysis::Histogram { bins, pareto: flags[0], cumulative: flags[1] }, out_labels)
            }
            Tool::Regression => {
                let y = first.as_ref().ok_or_default()?;
                let x = second()?;
                let (ycells, _) = vector(&y.sheet, y.range, labels);
                let ys = numbers_at(&y.sheet, &ycells);
                let xs = Block::read(&x.sheet, x.range, false, labels, &name_of(true, "xlsx.analysis.out.x-variable"));
                let opt = analysis::Regression { constant: !flags[0], confidence: need(0)?, residuals: flags[1] || flags[2] || flags[3], standard_residuals: flags[2] };
                analysis::regression(&ys, &xs, &opt, out_labels, &ev).map_err(|_| text_of("xlsx.analysis.message.regression"))?
            }
            Tool::TPaired | Tool::TEqual | Tool::TUnequal | Tool::ZTest | Tool::FTest => {
                let a = first.as_ref().ok_or_default()?;
                let b = second()?;
                let (xa, na) = vec_of(a, "xlsx.analysis.out.variable-1");
                let (xb, nb) = vec_of(&b, "xlsx.analysis.out.variable-2");
                match tool {
                    Tool::FTest => analysis::f_test(&xa, &xb, (&na, &nb), need(0)?, out_labels, &ev),
                    Tool::ZTest => analysis::z_test(&xa, &xb, (&na, &nb), (need(2)?, need(3)?), need(0)?, need(1)?, out_labels, &ev),
                    _ => {
                        let kind = match tool {
                            Tool::TPaired => analysis::TTest::Paired,
                            Tool::TEqual => analysis::TTest::EqualVariance,
                            _ => analysis::TTest::UnequalVariance,
                        };
                        if kind == analysis::TTest::Paired && xa.iter().flatten().count() != xb.iter().flatten().count() {
                            return Err(text_of("xlsx.analysis.message.paired-size"));
                        }
                        analysis::t_test(&xa, &xb, (&na, &nb), kind, need(0)?, need(1)?, out_labels, &ev)
                    }
                }
            }
            Tool::AnovaSingle => {
                let r = first.as_ref().ok_or_default()?;
                let block = Block::read(&r.sheet, r.range, by_rows, labels, &name_of(true, column_word));
                analysis::anova_single(&block, need(0)?, out_labels, &ev)
            }
            Tool::AnovaTwoWithout => {
                let r = first.as_ref().ok_or_default()?;
                let (skip_r, skip_c) = (u32::from(labels), u32::from(labels));
                let mut rows = Vec::new();
                for row in r.range.first_row + skip_r..=r.range.last_row {
                    let mut v = Vec::new();
                    for col in r.range.first_column + skip_c..=r.range.last_column {
                        match r.sheet.cell(row, col).map(|c| &c.value) {
                            Some(CellValue::Number(n)) => v.push(*n),
                            _ => return Err(text_of("xlsx.analysis.message.anova-blank")),
                        }
                    }
                    rows.push(v);
                }
                if rows.len() < 2 || rows[0].len() < 2 {
                    return Err(text_of("xlsx.analysis.message.anova-size"));
                }
                let label_at = |row: u32, col: u32, i: usize, id: &str| match r.sheet.cell(row, col).map(|c| &c.value) {
                    Some(CellValue::Text(t)) if labels => t.clone(),
                    _ => format!("{} {}", text(id), i + 1),
                };
                let row_names: Vec<String> = (0..rows.len()).map(|i| label_at(r.range.first_row + skip_r + i as u32, r.range.first_column, i, "xlsx.analysis.out.row")).collect();
                let col_names: Vec<String> = (0..rows[0].len()).map(|j| label_at(r.range.first_row, r.range.first_column + skip_c + j as u32, j, "xlsx.analysis.out.column")).collect();
                analysis::anova_two_without(&rows, &row_names, &col_names, need(0)?, out_labels, &ev)
            }
            Tool::AnovaTwoWith => {
                let r = first.as_ref().ok_or_default()?;
                let m = need(0)? as usize;
                let data_rows = (r.range.last_row - r.range.first_row) as usize;
                if m == 0 || data_rows == 0 || data_rows % m != 0 || r.range.last_column <= r.range.first_column {
                    return Err(text_of("xlsx.analysis.message.rows-per-sample"));
                }
                let mut groups = Vec::new();
                let mut names = Vec::new();
                for g in 0..data_rows / m {
                    let top = r.range.first_row + 1 + (g * m) as u32;
                    names.push(match r.sheet.cell(top, r.range.first_column).map(|c| &c.value) {
                        Some(CellValue::Text(t)) => t.clone(),
                        Some(CellValue::Number(n)) => ibis_calc::number_to_xml(*n),
                        _ => format!("{} {}", text("xlsx.analysis.out.sample"), g + 1),
                    });
                    let mut cols = Vec::new();
                    for col in r.range.first_column + 1..=r.range.last_column {
                        let mut cell = Vec::new();
                        for row in top..top + m as u32 {
                            match r.sheet.cell(row, col).map(|c| &c.value) {
                                Some(CellValue::Number(n)) => cell.push(*n),
                                _ => return Err(text_of("xlsx.analysis.message.anova-blank")),
                            }
                        }
                        cols.push(cell);
                    }
                    groups.push(cols);
                }
                let col_names: Vec<String> = (r.range.first_column + 1..=r.range.last_column)
                    .enumerate()
                    .map(|(j, col)| match r.sheet.cell(r.range.first_row, col).map(|c| &c.value) {
                        Some(CellValue::Text(t)) => t.clone(),
                        _ => format!("{} {}", text("xlsx.analysis.out.column"), j + 1),
                    })
                    .collect();
                if groups.len() < 2 || col_names.len() < 2 {
                    return Err(text_of("xlsx.analysis.message.anova-size"));
                }
                analysis::anova_two_with(&groups, &names, &col_names, need(1)?, out_labels, &ev)
            }
            Tool::MovingAverage | Tool::ExponentialSmoothing => {
                let r = first.as_ref().ok_or_default()?;
                let (cells, _) = vector(&r.sheet, r.range, labels);
                if cells.is_empty() {
                    return Err(text_of("xlsx.analysis.message.no-data"));
                }
                let prefix = if r.slide == out_slide { String::new() } else { format!("{}!", ibis_calc::formula::quote_sheet(&r.name)) };
                let series = Series { cells, prefix };
                if tool == Tool::MovingAverage {
                    let interval = need(0)?;
                    if interval < 1.0 || interval.fract() != 0.0 {
                        return Err(text_of("xlsx.analysis.message.interval"));
                    }
                    analysis::moving_average(&series, interval as usize, flags[0], origin)
                } else {
                    let damping = need(0)?;
                    if !(0.0..=1.0).contains(&damping) {
                        return Err(text_of("xlsx.analysis.message.damping"));
                    }
                    analysis::exponential_smoothing(&series, damping, flags[0], origin)
                }
            }
            Tool::Sampling => {
                let r = first.as_ref().ok_or_default()?;
                let block = Block::read(&r.sheet, r.range, false, labels, &name_of(false, column_word));
                let data: Vec<f64> = block.cols.iter().flatten().flatten().copied().collect();
                let how = match (blank(0), blank(1)) {
                    (false, true) => analysis::Sampling::Periodic(need(0)? as usize),
                    (true, false) => analysis::Sampling::Random(need(1)? as usize),
                    _ => return Err(text_of("xlsx.analysis.message.sampling")),
                };
                analysis::sampling(&data, how, seed())
            }
            Tool::Random => {
                let variables = need(0)? as usize;
                let count = need(1)? as usize;
                if variables == 0 || count == 0 || variables > 256 || count > 100_000 {
                    return Err(text_of("xlsx.analysis.message.random-size"));
                }
                let d = match dist {
                    Dist::Uniform => analysis::Distribution::Uniform { low: need(2)?, high: need(3)? },
                    Dist::Normal => analysis::Distribution::Normal { mean: need(2)?, sd: need(3)? },
                    Dist::Bernoulli => analysis::Distribution::Bernoulli { p: need(2)? },
                    Dist::Binomial => analysis::Distribution::Binomial { p: need(2)?, trials: need(3)? as u32 },
                    Dist::Poisson => analysis::Distribution::Poisson { lambda: need(2)? },
                    Dist::Patterned => analysis::Distribution::Patterned { from: need(2)?, to: need(3)?, step: need(4)?, repeat_each: need(5)? as u32, repeat_sequence: need(6)? as u32 },
                    Dist::Discrete => {
                        let r = self.resolve_range(&input2_text)?;
                        let mut values = Vec::new();
                        let mut probabilities = Vec::new();
                        for row in r.range.first_row..=r.range.last_row {
                            let get = |col: u32| match r.sheet.cell(row, col).map(|c| &c.value) {
                                Some(CellValue::Number(n)) => Some(*n),
                                _ => None,
                            };
                            match (get(r.range.first_column), get(r.range.first_column + 1)) {
                                (Some(v), Some(p)) => {
                                    values.push(v);
                                    probabilities.push(p);
                                }
                                _ => return Err(text_of("xlsx.analysis.message.discrete")),
                            }
                        }
                        analysis::Distribution::Discrete { values, probabilities }
                    }
                };
                analysis::random_numbers(variables, count, &d, seed()).map_err(|_| text_of("xlsx.analysis.message.random-parameters"))?
            }
            Tool::Fourier => {
                let r = first.as_ref().ok_or_default()?;
                let (cells, _) = vector(&r.sheet, r.range, labels);
                let data: Vec<f64> = match numbers_at(&r.sheet, &cells).into_iter().collect::<Option<Vec<f64>>>() {
                    Some(d) => d,
                    None => return Err(text_of("xlsx.analysis.message.fourier-blank")),
                };
                analysis::fourier(&data, flags[0]).map_err(|_| text_of("xlsx.analysis.message.fourier-size"))?
            }
        };
        drop(ev);
        if output.cells.is_empty() {
            return Err(text_of("xlsx.analysis.message.no-data"));
        }

        let slide = out_slide;
        let edits: Vec<CellEdit> = output.cells.iter().map(|(r, c, input)| CellEdit { row: origin.0 + r, column: origin.1 + c, input: input.clone() }).collect();
        let mut commands = Vec::new();
        if let Some((index, name, slide, shape)) = add_sheet {
            commands.push(Command::Sheet(SheetCommand::AddSheet { index, name, slide, shape }));
        }
        commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits }));
        for (range, code) in &output.formats {
            let abs = CellRange { first_row: origin.0 + range.first_row, first_column: origin.1 + range.first_column, last_row: origin.0 + range.last_row, last_column: origin.1 + range.last_column };
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![abs], change: StyleChange::NumberFormat(code.clone()) }));
        }
        if let Err(err) = self.document.execute(commands) {
            return Err(format("edit.refused", &[("error", &err)]));
        }
        if new_sheet {
            self.current_slide = slide;
            self.selection = None;
        }
        let (rows, columns) = output.size();

        // Optional chart output beside the generated table, amended into the same undo step.
        let chart_spec = match tool {
            Tool::Histogram if flags[2] && rows > 1 => {
                let out_name = self.presentation().slide(slide).and_then(|s| s.name.clone()).unwrap_or_default();
                let quoted = ibis_pptx::quote_sheet(&out_name);
                let table_col = if flags[0] { origin.1 + if flags[1] { 4 } else { 3 } } else { origin.1 };
                let (h_row, r0, r1) = (origin.0, origin.0 + 1, origin.0 + rows - 1);
                let cats = ibis_pptx::format_ref(&quoted, CellRange { first_row: r0, first_column: table_col, last_row: r1, last_column: table_col });
                let mut refs = vec![ibis_pptx::SeriesRefs {
                    name: Some(ibis_pptx::format_ref(&quoted, CellRange { first_row: h_row, first_column: table_col + 1, last_row: h_row, last_column: table_col + 1 })),
                    categories: Some(cats.clone()),
                    values: Some(ibis_pptx::format_ref(&quoted, CellRange { first_row: r0, first_column: table_col + 1, last_row: r1, last_column: table_col + 1 })),
                    ..Default::default()
                }];
                let kind = if flags[0] || flags[1] {
                    refs.push(ibis_pptx::SeriesRefs {
                        name: Some(ibis_pptx::format_ref(&quoted, CellRange { first_row: h_row, first_column: table_col + 2, last_row: h_row, last_column: table_col + 2 })),
                        categories: Some(cats),
                        values: Some(ibis_pptx::format_ref(&quoted, CellRange { first_row: r0, first_column: table_col + 2, last_row: r1, last_column: table_col + 2 })),
                        ..Default::default()
                    });
                    ibis_pptx::NewChartKind::ComboColumnLineSecondaryAxis
                } else {
                    ibis_pptx::NewChartKind::ClusteredColumn
                };
                Some((kind, refs))
            }
            Tool::Regression if flags[3] => {
                let out_name = self.presentation().slide(slide).and_then(|s| s.name.clone()).unwrap_or_default();
                let quoted = ibis_pptx::quote_sheet(&out_name);
                let obs_label = text("xlsx.analysis.out.observation");
                let obs_row = output.cells.iter().find_map(|(r, c, inp)| {
                    (*c == 0 && matches!(inp, ibis_doc_model::CellInput::Value(CellValue::Text(t)) if *t == obs_label)).then_some(*r)
                });
                obs_row.and_then(|obs_r| {
                    let h_row = origin.0 + obs_r;
                    let r0 = h_row + 1;
                    let r1 = origin.0 + rows.saturating_sub(1);
                    (r1 >= r0).then(|| {
                        let cats = ibis_pptx::format_ref(&quoted, CellRange { first_row: r0, first_column: origin.1, last_row: r1, last_column: origin.1 });
                        let refs = (1..=2)
                            .map(|dc| ibis_pptx::SeriesRefs {
                                name: Some(ibis_pptx::format_ref(&quoted, CellRange { first_row: h_row, first_column: origin.1 + dc, last_row: h_row, last_column: origin.1 + dc })),
                                categories: Some(cats.clone()),
                                values: Some(ibis_pptx::format_ref(&quoted, CellRange { first_row: r0, first_column: origin.1 + dc, last_row: r1, last_column: origin.1 + dc })),
                                ..Default::default()
                            })
                            .collect();
                        (ibis_pptx::NewChartKind::Line, refs)
                    })
                })
            }
            Tool::MovingAverage | Tool::ExponentialSmoothing if flags[1] => {
                first.as_ref().and_then(|r| {
                    let (cells, _) = vector(&r.sheet, r.range, labels);
                    let (&first_cell, &last_cell) = (cells.first()?, cells.last()?);
                    let in_quoted = ibis_pptx::quote_sheet(&r.name);
                    let out_name = self.presentation().slide(slide).and_then(|s| s.name.clone()).unwrap_or_default();
                    let out_quoted = ibis_pptx::quote_sheet(&out_name);
                    let vertical = cells.len() < 2 || cells[0].1 == cells[1].1;
                    let out_range = if vertical {
                        CellRange { first_row: origin.0, first_column: origin.1, last_row: origin.0 + rows.saturating_sub(1), last_column: origin.1 }
                    } else {
                        CellRange { first_row: origin.0, first_column: origin.1, last_row: origin.0, last_column: origin.1 + columns.saturating_sub(1) }
                    };
                    let refs = vec![
                        ibis_pptx::SeriesRefs {
                            name: labels.then(|| ibis_pptx::format_ref(&in_quoted, CellRange { first_row: r.range.first_row, first_column: r.range.first_column, last_row: r.range.first_row, last_column: r.range.first_column })),
                            categories: None,
                            values: Some(ibis_pptx::format_ref(&in_quoted, CellRange { first_row: first_cell.0, first_column: first_cell.1, last_row: last_cell.0, last_column: last_cell.1 })),
                            ..Default::default()
                        },
                        ibis_pptx::SeriesRefs {
                            name: None,
                            categories: None,
                            values: Some(ibis_pptx::format_ref(&out_quoted, out_range)),
                            ..Default::default()
                        },
                    ];
                    Some((ibis_pptx::NewChartKind::Line, refs))
                })
            }
            _ => None,
        };
        if let (Some((kind, refs)), Some(source)) = (chart_spec, self.source.clone()) {
            let current = self.presentation().clone();
            let (x, y) = self
                .presentation()
                .slide(slide)
                .and_then(|s| {
                    let sh = s.shapes.iter().find(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Sheet(_)))?;
                    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(s)?;
                    let left = (origin.1 + columns + 1) as usize;
                    let top = origin.0 as usize;
                    Some((
                        sh.frame.x + sheet.column_offsets().get(left).copied().unwrap_or(0),
                        sh.frame.y + sheet.row_offsets().get(top).copied().unwrap_or(0),
                    ))
                })
                .unwrap_or((952_500, 952_500));
            let id = self.document.ids().new_shape_id();
            if let Ok(chart) = source.new_sheet_chart(&current, slide, id, ibis_doc_model::EmuRect::new(x, y, 4_572_000, 2_743_200), kind, refs) {
                let index = self.presentation().slide(slide).map_or(0, |s| s.shapes.len());
                let _ = self.document.amend_last(vec![Command::InsertShape { slide, index, shape: chart }]);
            }
        }

        let end = CellRange { first_row: origin.0, first_column: origin.1, last_row: origin.0 + rows.saturating_sub(1), last_column: origin.1 + columns.saturating_sub(1) };
        Ok(format("xlsx.analysis.message.done", &[("range", &range_text(end))]))
    }

    pub(crate) fn analysis_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.analysis.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let caption = |id: &str| div().text_sm().child(text(id));
        let mut body = v_flex().gap_2().px(px(16.)).py(px(8.)).p_0();

        // The tool.
        body = body.child(caption("xlsx.analysis.field.tool"));
        let current = p.tool;
        let current_dist = p.dist;
        let choices=Tool::ALL.iter().map(|&t|crate::shell::chrome::MenuItem::new("analysis-tool-choice",text(t.id())).checked(t==current).on(move|this,w,cx|this.analysis_choose(t,Dist::Uniform,w,cx)).into()).collect();
        body=body.child(mark("tool",div().w(px(224.)).child(crate::shell::chrome::Dropdown::from_entries("analysis-tool",text(current.id()),choices,true,0,&cx.weak_entity()).into_any_element()).into_any_element()));

        // Input ranges.
        let (first_caption, second_caption) = current.ranges();
        if !first_caption.is_empty() {
            body = body
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .child(caption(first_caption)),
                )
                .child(mark("input", super::range_field::local_range_field(&p.input,"analysis-input",false,cx).into_any_element()));
        }
        let has_second = second_caption.is_some() && (current != Tool::Random || current_dist == Dist::Discrete);
        if has_second {
            if let Some(c) = second_caption {
                body = body
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(caption(c)),
                    )
                    .child(mark("input2", super::range_field::local_range_field(&p.input2,"analysis-input2",false,cx).into_any_element()));
            }
        }

        let entity = cx.entity().downgrade();
        // Random numbers: the distribution.
        if current == Tool::Random {
            body = body.child(caption("xlsx.analysis.field.distribution"));
            body = body.child(mark(
                "dist",
                crate::shell::chrome::Dropdown::new("analysis-dist",text(current_dist.id()),
                    Dist::ALL.iter().map(|&dist|crate::shell::chrome::DropdownItem::new(text(dist.id()))
                        .checked(dist==current_dist).on_select(std::rc::Rc::new(move|e,w,cx|e.analysis_choose(Tool::Random,dist,w,cx)))).collect(),&cx.weak_entity())
                    .into_any_element(),
            ));
        }

        // Grouped by.
        if current.grouped() {
            body = body.child(crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                .label(text("xlsx.analysis.field.grouped-by"))
                .children([(false,"_xlsx.column","grouped-columns"),(true,"_xlsx.row","grouped-rows")].into_iter().map(|(rows,label,id)| {
                    let ui=p.ui.clone();
                    crate::shell::chrome::segment_control::SegmentItem::label(id,text(label),p.by_rows==rows)
                        .on_bounds(move|b|{ui.borrow_mut().insert(id.to_string(),b);})
                        .on_click(cx.listener(move|this,_,_,cx|{
                            if let Some(p) = this.wb_mut().panels.analysis.as_mut() { p.by_rows=rows; }
                            cx.notify();
                        }))
                }))
            );
        }
        let toggle = |id: String, on: bool, label: &'static str, set: Rc<dyn Fn(&mut AnalysisPanel)>| {
            mark(
                &id.clone(),
                super::toggle::toggle_row(
                    SharedString::from(id),
                    on,
                    text(label),
                    cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.analysis.as_mut() {
                            set(p);
                        }
                        cx.notify();
                    }),
                )
                    .into_any_element(),
            )
        };
        if current.labels() {
            body = body.child(toggle("labels".into(), p.labels, "xlsx.analysis.field.labels", Rc::new(|p| p.labels = !p.labels)));
        }

        // Numbers.
        let params = current.params(current_dist);
        for (i, (label, _)) in params.iter().enumerate() {
            let mut field = crate::shell::chrome::text_field(&p.params[i]);
            if *label == "xlsx.analysis.field.confidence-level" { field = field.unit("%",px(20.)); }
            if let Some(spec) = param_spinner(label) {
                // The tool runs from its OK button, not on Enter: a step only changes the box.
                field = field.spinner(format!("analysis-param-{i}"), spec, cx, |_, _, _, _| {});
            }
            body = body.child(caption(label)).child(mark(&format!("param-{i}"), field.into_any_element()));
        }

        // Options.
        for (i, label) in current.flags().iter().enumerate() {
            body = body.child(toggle(format!("flag-{i}"), p.flags[i], label, Rc::new(move |p| p.flags[i] = !p.flags[i])));
        }

        // The output.
        let new_sheet = p.new_sheet;
        let out_radio = |id: &'static str, on: bool, label: &'static str, sheet: bool| {
            mark(
                id,
                crate::shell::chrome::radio_button(id, on, false, text(label), cx.listener(move |this, _, window, cx| {
                        let out_input = this.wb_mut().panels.analysis.as_mut().map(|p| {
                            p.new_sheet = sheet;
                            p.output.clone()
                        });
                        if let Some(input) = out_input {
                            let val = if sheet { "" } else { "E1" };
                            input.update(cx, |i, cx| i.set_value(val, window, cx));
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        let mut out_header = h_flex()
            .gap_1()
            .items_center()
            .justify_between()
            .child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow).child(out_radio("output-range", !new_sheet, "xlsx.analysis.field.output", false)).child(out_radio("output-new-sheet", new_sheet, "_xlsx.new-worksheet", true)));
        if !new_sheet {
            out_header = out_header;
        }
        body = body
            .child(out_header)
            .child(mark("output", super::range_field::local_range_field(&p.output,"analysis-output",true,cx).into_any_element()));
        if !p.message.is_empty() {
            body = body.children(crate::shell::chrome::panel_message(&p.message));
        }
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("analysis-apply", "", crate::shell::chrome::ActionButtonKind::Primary).label(text("OK")).on_click(cx.listener(|this, _, _, cx| this.apply_analysis_panel(cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("xlsx.analysis.title"), body.into_any_element(), panel_terminal_actions))
    }
}

fn text_of(id: &str) -> String {
    text(id).to_string()
}

/// `Option` → `Result<_, String>` for the lookups that cannot fail while
/// the panel is open.
trait OrDefault<T> {
    fn ok_or_default(self) -> Result<T, String>;
}

impl<T> OrDefault<T> for Option<T> {
    fn ok_or_default(self) -> Result<T, String> {
        self.ok_or_else(|| text_of("xlsx.analysis.message.no-data"))
    }
}

#[cfg(test)]
mod tests {
    use super::{Dist, Tool, analysis};

    #[test]
    fn every_caption_is_in_both_languages() {
        let mut ids: Vec<String> = analysis::LABEL_IDS.iter().map(|s| s.to_string()).collect();
        for t in Tool::ALL {
            ids.push(t.id().into());
            ids.push(t.ranges().0.into());
            ids.extend(t.ranges().1.map(String::from));
            ids.extend(t.flags().iter().map(|s| s.to_string()));
            for d in Dist::ALL {
                ids.extend(t.params(d).iter().map(|p| p.0.to_string()));
                ids.push(d.id().into());
            }
        }
        ids.retain(|id| !id.is_empty());
        for table in [include_str!("../../i18n/ja/xlsx.tsv"), include_str!("../../i18n/en/xlsx.tsv")] {
            for id in &ids {
                assert!(table.lines().any(|l| l.split('\t').next() == Some(id.as_str())), "{id} is missing");
            }
        }
    }
}
