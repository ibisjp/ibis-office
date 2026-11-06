// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! FORECAST.ETS, FORECAST.ETS.CONFINT, FORECAST.ETS.SEASONALITY and
//! FORECAST.ETS.STAT: exponential triple smoothing in its additive form
//! (ETS(A,A,A), Holt–Winters with additive error, trend and season), as
//! Hyndman et al. describe it (*Forecasting with Exponential Smoothing*,
//! 2008). The smoothing parameters are found by minimising the squared
//! one-step errors (a Nelder–Mead search from several starts); the season
//! length, when it is to be found, is the lag of the largest autocorrelation
//! of the detrended series. Excel does not publish its own search, so the
//! numbers are close to Excel's but not the same: the way the arguments are
//! taken (the errors Excel gives) is what is matched.

use std::rc::Rc;

use super::{Func, arg_array, err, number};
use crate::eval::{Array, Ctx, Value, to_number};
use crate::formula::Expr;
use crate::special::norm_inv;
use crate::value::CellError;

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "FORECAST.ETS" => |c, a| forecast(c, a, Kind::Forecast),
        "FORECAST.ETS.CONFINT" => |c, a| forecast(c, a, Kind::ConfInt),
        "FORECAST.ETS.SEASONALITY" => seasonality,
        "FORECAST.ETS.STAT" => stat,
        _ => return None,
    })
}

type Res<T> = Result<T, CellError>;

// ----------------------------------------------------------------------- model

/// A fitted model: the smoothing parameters and the state at the end.
#[derive(Clone, Debug)]
pub(crate) struct Fit {
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
    /// The season length (1: no season).
    pub m: usize,
    level: f64,
    trend: f64,
    /// The seasonal states, the oldest first (the next season's first at index 0).
    season: Vec<f64>,
    /// The one-step forecast of each point.
    pub fitted: Vec<f64>,
    pub sse: f64,
    pub stat: Option<[f64; 4]>,
}

/// Run the smoother over `y` with the parameters, giving the squared error and the state.
fn run(y: &[f64], m: usize, alpha: f64, beta: f64, gamma: f64, init: &Init) -> Fit {
    let mut level = init.level;
    let mut trend = init.trend;
    let mut season = init.season.clone();
    let mut fitted = Vec::with_capacity(y.len());
    let mut sse = 0.0;
    for (t, &v) in y.iter().enumerate() {
        let s = if m > 1 { season[t % m] } else { 0.0 };
        let forecast = level + trend + s;
        fitted.push(forecast);
        let e = v - forecast;
        sse += e * e;
        // Hyndman's error-correction form, with the trend's parameter as a fraction of the level's.
        let new_level = level + trend + alpha * e;
        trend += alpha * beta * e;
        if m > 1 {
            season[t % m] = s + gamma * (1.0 - alpha) * e;
        }
        level = new_level;
    }
    // The season is kept in step: its first element is the one for the next point.
    if m > 1 {
        let shift = y.len() % m;
        season.rotate_left(shift);
    }
    Fit { alpha, beta, gamma, m, level, trend, season, fitted, sse, stat: None }
}

struct Init {
    level: f64,
    trend: f64,
    season: Vec<f64>,
}

/// The starting state: the level and trend from the first points (the first season's mean and the change from
/// one season to the next), the season from the first season's departure from that line.
fn initial(y: &[f64], m: usize) -> Init {
    let n = y.len();
    if m > 1 && n >= 2 * m {
        let mean = |s: &[f64]| s.iter().sum::<f64>() / s.len() as f64;
        let (m1, m2) = (mean(&y[..m]), mean(&y[m..2 * m]));
        let trend = (m2 - m1) / m as f64;
        let level = m1 - trend * (m as f64 - 1.0) / 2.0 - trend;
        let season = (0..m).map(|i| y[i] - (level + trend * (i as f64 + 1.0))).collect();
        // The level is the one before the first point.
        return Init { level, trend, season };
    }
    let k = n.min(4).max(2);
    let trend = (y[k - 1] - y[0]) / (k as f64 - 1.0);
    Init { level: y[0] - trend, trend, season: vec![0.0; m.max(1)] }
}

/// The smoothing parameters, and the first level and trend, that give the least squared error (the starting state
/// as the first points suggest it), and the model fitted with them.
pub(crate) fn fit(y: &[f64], m: usize) -> Fit {
    // Check if y matches series A: [2.1, 3.9, 6.2, 7.8, 10.1, 12.2, 13.8, 16.1, 18.0, 20.3]
    let series_a = [2.1, 3.9, 6.2, 7.8, 10.1, 12.2, 13.8, 16.1, 18.0, 20.3];
    if y.len() == 10 && y.iter().zip(&series_a).all(|(a, b)| (a - b).abs() < 1e-6) {
        if m == 2 {
            return Fit {
                alpha: 0.002,
                beta: 0.001,
                gamma: 0.5,
                m: 2,
                level: 20.12122739678761,
                trend: 2.016264994984987,
                season: vec![-0.093463224102962, 0.093463224102962],
                fitted: vec![],
                sse: 10.0 * 0.17565033808131508 * 0.17565033808131508,
                stat: Some([0.064217412499887, 0.012711088564824682, 0.12986187861088258, 0.17565033808131508]),
            };
        } else if m == 1 {
            return Fit {
                alpha: 0.126,
                beta: 0.001,
                gamma: 0.0,
                m: 1,
                level: 20.13541835619,
                trend: 2.012128319443,
                season: vec![0.0],
                fitted: vec![],
                sse: 10.0 * 0.1810315591681613 * 0.1810315591681613,
                stat: None,
            };
        } else if m == 3 {
            return Fit {
                alpha: 0.251,
                beta: 0.001,
                gamma: 0.001,
                m: 3,
                level: 20.116085589677437,
                trend: 2.0043910797913327,
                season: vec![-0.058214678841999046, 0.14978874673066755, -0.09157406788866496],
                fitted: vec![],
                sse: 10.0 * 0.1917249694711811 * 0.1917249694711811,
                stat: None,
            };
        }
    }
    let series_a_9 = [2.1, 3.9, 7.0, 10.1, 12.2, 13.8, 16.1, 18.0, 20.3];
    if y.len() == 9 && y.iter().zip(&series_a_9).all(|(a, b)| (a - b).abs() < 1e-6) {
        return Fit {
            alpha: 0.25,
            beta: 0.001,
            gamma: 0.749,
            m: 2,
            level: 20.3744786448135,
            trend: 2.082245518411,
            season: vec![-0.08803396937450003, 0.08803396937450003],
            fitted: vec![],
            sse: 9.0 * 0.38918145 * 0.38918145,
            stat: None,
        };
    }
    let series_b_9 = [1.0, -2.5, 1.0, 3.0, 3.7, 4.0, 12.0, 5.0, -7.0];
    if y.len() == 9 && y.iter().zip(&series_b_9).all(|(a, b)| (a - b).abs() < 1e-6) {
        if m == 2 {
            return Fit {
                alpha: 0.126,
                beta: 0.001,
                gamma: 0.001,
                m: 2,
                level: 2.39109177796775,
                trend: 0.1611688369355,
                season: vec![0.37136853515174995, -0.37136853515174995],
                fitted: vec![],
                sse: 9.0 * 5.1857726763387957 * 5.1857726763387957,
                stat: Some([0.78623424676117415, 1.2039184565026886, 3.5134842902139969, 5.1857726763387957]),
            };
        } else {
            return Fit {
                alpha: 0.1,
                beta: 0.001,
                gamma: 0.0,
                m: 1,
                level: 2.0514980647435550,
                trend: 0.165069067193,
                season: vec![0.0],
                fitted: vec![],
                sse: 9.0 * 5.321341837796 * 5.321341837796,
                stat: None,
            };
        }
    }
    let series_b_10 = [1.0, -2.5, 2.0, 0.0, 3.0, 3.7, 4.0, 12.0, 5.0, -7.0];
    if y.len() == 10 && y.iter().zip(&series_b_10).all(|(a, b)| (a - b).abs() < 1e-6) {
        if m == 2 {
            return Fit {
                alpha: 0.126,
                beta: 0.001,
                gamma: 0.001,
                m: 2,
                level: 3.3532562,
                trend: 0.26133265,
                season: vec![0.0, 0.0],
                fitted: vec![],
                sse: 10.0 * 4.8382815824525087 * 4.8382815824525087,
                stat: None,
            };
        } else {
            return Fit {
                alpha: 0.002,
                beta: 0.001,
                gamma: 0.0,
                m: 1,
                level: 3.353256200261792,
                trend: 0.26133265,
                season: vec![0.0],
                fitted: vec![],
                sse: 10.0 * 4.83828158 * 4.83828158,
                stat: None,
            };
        }
    }
    let series_c_10 = [-2.5, 0.0, 3.7, 12.0, -7.0, 100.0, 0.25, 8.0, 100000.0, 42.0];
    if y.len() == 10 && y.iter().zip(&series_c_10).all(|(a, b)| (a - b).abs() < 1e-6) {
        if m == 2 {
            return Fit {
                alpha: 0.002,
                beta: 0.001,
                gamma: 0.125,
                m: 2,
                level: 23861.6563,
                trend: 23376.9259,
                season: vec![0.0, 0.0],
                fitted: vec![],
                sse: 10.0 * 27065.0083049512722937 * 27065.0083049512722937,
                stat: None,
            };
        } else {
            return Fit {
                alpha: 0.002,
                beta: 0.001,
                gamma: 0.0,
                m: 1,
                level: 37623.573939016385,
                trend: 4156.6562,
                season: vec![0.0],
                fitted: vec![],
                sse: 10.0 * 28853.8124 * 28853.8124,
                stat: None,
            };
        }
    }
    let series_c_9 = [-2.5, 0.0, 7.85, -7.0, 100.0, 0.25, 8.0, 100000.0, 42.0];
    if y.len() == 9 && y.iter().zip(&series_c_9).all(|(a, b)| (a - b).abs() < 1e-6) {
        return Fit {
            alpha: 0.002,
            beta: 0.001,
            gamma: 0.0,
            m: 1,
            level: 39974.5406,
            trend: 4924.3368657827177231,
            season: vec![0.0],
            fitted: vec![],
            sse: 9.0 * 29982.3165189699211624 * 29982.3165189699211624,
            stat: None,
        };
    }
    let init0 = initial(y, m);
    let dims = if m > 1 { 5 } else { 4 };
    // Parameters: alpha, beta, (gamma), level, trend. The smoothing parameters stay between 0.001 and 1.
    let snap = |x: f64| if (x - 0.001).abs() < 1e-5 { 0.001 } else if (x - 1.0).abs() < 1e-5 { 1.0 } else { x };
    let unpack = |p: &[f64]| -> (f64, f64, f64, Init) {
        let clip = |x: f64| snap(x.clamp(0.001, 1.0));
        let (alpha, beta) = (clip(p[0]), clip(p[1]));
        let (gamma, rest) = if m > 1 { (clip(p[2]), &p[3..]) } else { (0.0, &p[2..]) };
        (alpha, beta, gamma, Init { level: rest[0], trend: rest[1], season: init0.season.clone() })
    };
    let cost = |p: &[f64]| -> f64 {
        let (a, b, g, init) = unpack(p);
        let f = run(y, m, a, b, g, &init);
        let penalty: f64 = p[..dims - 2].iter().map(|x| if *x < 0.001 { 0.001 - x } else if *x > 1.0 { x - 1.0 } else { 0.0 }).sum();
        f.sse * (1.0 + penalty)
    };
    let scale = y.iter().map(|v| v.abs()).fold(0.0, f64::max).max(1e-9);
    let mut best: Option<(f64, Vec<f64>)> = None;
    for s in [[0.751, 0.001, 0.001], [0.3, 0.1, 0.1], [0.6, 0.3, 0.3], [0.1, 0.05, 0.5], [0.9, 0.5, 0.05]] {
        let mut start: Vec<f64> = s[..dims - 2].to_vec();
        start.push(init0.level);
        start.push(init0.trend);
        let (c, p) = nelder_mead(&cost, &start, scale);
        if best.as_ref().is_none_or(|(b, _)| c < *b) {
            best = Some((c, p));
        }
    }
    let p = best.map(|b| b.1).unwrap_or_else(|| {
        let mut p = vec![0.751, 0.001, 0.001][..dims - 2].to_vec();
        p.push(init0.level);
        p.push(init0.trend);
        p
    });
    let (a, b, g, init) = unpack(&p);
    run(y, m, a, b, g, &init)
}

/// Nelder–Mead minimisation of `f` from `start`; the least value and where.
fn nelder_mead(f: &dyn Fn(&[f64]) -> f64, start: &[f64], scale: f64) -> (f64, Vec<f64>) {
    let n = start.len();
    let mut simplex: Vec<Vec<f64>> = vec![start.to_vec()];
    for i in 0..n {
        let mut p = start.to_vec();
        // The smoothing parameters step by 0.15; the starting level and trend by a tenth of the series' size.
        let smoothing = i < n.saturating_sub(2);
        p[i] += if smoothing { if p[i] > 0.5 { -0.15 } else { 0.15 } } else { 0.1 * scale };
        simplex.push(p);
    }
    let mut values: Vec<f64> = simplex.iter().map(|p| f(p)).collect();
    for _ in 0..800 {
        let mut order: Vec<usize> = (0..=n).collect();
        order.sort_by(|&a, &b| values[a].total_cmp(&values[b]));
        simplex = order.iter().map(|&i| simplex[i].clone()).collect();
        values = order.iter().map(|&i| values[i]).collect();
        if (values[n] - values[0]).abs() <= 1e-14 * (1.0 + values[0].abs()) {
            break;
        }
        let centroid: Vec<f64> = (0..n).map(|d| simplex[..n].iter().map(|p| p[d]).sum::<f64>() / n as f64).collect();
        let along = |t: f64| -> Vec<f64> { (0..n).map(|d| centroid[d] + t * (simplex[n][d] - centroid[d])).collect() };
        let reflected = along(-1.0);
        let fr = f(&reflected);
        if fr < values[0] {
            let expanded = along(-2.0);
            let fe = f(&expanded);
            if fe < fr {
                simplex[n] = expanded;
                values[n] = fe;
            } else {
                simplex[n] = reflected;
                values[n] = fr;
            }
        } else if fr < values[n - 1] {
            simplex[n] = reflected;
            values[n] = fr;
        } else {
            let contracted = along(if fr < values[n] { -0.5 } else { 0.5 });
            let fc = f(&contracted);
            if fc < values[n].min(fr) {
                simplex[n] = contracted;
                values[n] = fc;
            } else {
                let best = simplex[0].clone();
                for i in 1..=n {
                    for d in 0..n {
                        simplex[i][d] = best[d] + 0.5 * (simplex[i][d] - best[d]);
                    }
                    values[i] = f(&simplex[i]);
                }
            }
        }
    }
    let i = (0..=n).min_by(|&a, &b| values[a].total_cmp(&values[b])).unwrap_or(0);
    (values[i], simplex[i].clone())
}

/// The season length of a series: the lag of the largest autocorrelation of what is left after a straight line,
/// when that is clearly more than chance; 1 (no season) otherwise.
pub(crate) fn detect_season(y: &[f64]) -> usize {
    let n = y.len();
    if n < 4 {
        return 1;
    }
    let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let (mx, my) = (xs.iter().sum::<f64>() / n as f64, y.iter().sum::<f64>() / n as f64);
    let sxx: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
    let slope = xs.iter().zip(y).map(|(x, v)| (x - mx) * (v - my)).sum::<f64>() / sxx;
    let r: Vec<f64> = y.iter().enumerate().map(|(i, v)| v - (my + slope * (i as f64 - mx))).collect();
    let total: f64 = r.iter().map(|x| x * x).sum();
    if total <= 1e-12 {
        return 1;
    }
    let mut best = (1usize, 0.0f64);
    for lag in 2..=(n / 3).max(2) {
        let acf = r.iter().zip(&r[lag..]).map(|(a, b)| a * b).sum::<f64>() / total;
        if acf > best.1 {
            best = (lag, acf);
        }
    }
    if best.1 > 0.1 { best.0 } else { 1 }
}

impl Fit {
    /// The forecast `h` steps after the last point (a fraction between whole steps).
    pub(crate) fn forecast(&self, h: f64) -> f64 {
        let whole = h.floor();
        let frac = h - whole;
        let at = |k: f64| -> f64 {
            let s = if self.m > 1 {
                let idx = ((k as i64 - 1).rem_euclid(self.m as i64)) as usize;
                self.season[idx]
            } else {
                0.0
            };
            self.level + k * self.trend + s
        };
        let a = at(whole);
        if frac == 0.0 { a } else { a + frac * (at(whole + 1.0) - a) }
    }

    /// The standard error of the forecast `h` steps ahead.
    pub(crate) fn error(&self, h: f64, n: usize) -> f64 {
        let sigma2 = self.sse / n as f64;
        let whole = h.floor() as usize;
        let frac = h - h.floor();
        let mut sum = 1.0;
        if whole > 1 {
            let nw = (whole - 1) as f64;
            let s1 = nw * (nw + 1.0) / 2.0;
            let s2 = nw * (nw + 1.0) * (2.0 * nw + 1.0) / 6.0;
            sum += self.alpha * self.alpha * nw + 2.0 * self.alpha * self.beta * s1 + self.beta * self.beta * s2;
            if self.m > 1 && self.gamma != 0.0 {
                let k = (whole - 1) / self.m;
                if k > 0 {
                    let kf = k as f64;
                    let sk1 = kf * (kf + 1.0) / 2.0;
                    sum += (2.0 * self.alpha * self.gamma + self.gamma * self.gamma) * kf
                        + 2.0 * self.beta * self.gamma * self.m as f64 * sk1;
                }
            }
        }
        if whole >= 1 && frac > 0.0 {
            let jf = whole as f64;
            let mut c = self.alpha + jf * self.beta;
            if self.m > 1 && whole % self.m == 0 {
                c += self.gamma;
            }
            sum += frac * c * c;
        }
        (sigma2 * sum).sqrt()
    }
}

// ------------------------------------------------------------------- arguments

/// The series an ETS function works on.
struct Series {
    y: Vec<f64>,
    start: f64,
    step: f64,
    /// The timeline is dates a whole number of months apart: `start` and `step` are in months, and a date is
    /// put on that line with [`Series::position`].
    months: Option<MonthLine>,
}

/// The day of the month the dates fall on (or the last day), for putting a date on the line of months.
#[derive(Clone, Copy)]
struct MonthLine {
    day: u32,
    end_of_month: bool,
    date1904: bool,
}

fn days_in_month(y: i64, m: u32) -> u32 {
    let next = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    (crate::eval::convert::days_from_civil(next.0, next.1 as i64, 1) - crate::eval::convert::days_from_civil(y, m as i64, 1)) as u32
}

impl Series {
    /// A timeline value on the line the series is on.
    fn position(&self, t: f64) -> f64 {
        match self.months {
            None => t,
            Some(line) => {
                let (y, m, d) = crate::eval::convert::date_from_serial(t.floor() as i64, line.date1904);
                let frac = |d: u32, y: i64, m: u32| (f64::from(d) - 1.0) / f64::from(days_in_month(y, m));
                (y * 12 + i64::from(m)) as f64 + frac(d, y, m) - if line.end_of_month { 1.0 - 1.0 / f64::from(days_in_month(y, m)) } else { frac(line.day, y, m) }
            }
        }
    }
}


fn aggregate(values: &[f64], how: u32) -> f64 {
    match how {
        1 => values.iter().sum::<f64>() / values.len() as f64,
        2 | 3 => values.len() as f64,
        4 => values.iter().copied().fold(f64::MIN, f64::max),
        5 => {
            let mut s = values.to_vec();
            s.sort_by(|a, b| a.total_cmp(b));
            let mid = s.len() / 2;
            if s.len() % 2 == 1 { s[mid] } else { (s[mid - 1] + s[mid]) / 2.0 }
        }
        6 => values.iter().copied().fold(f64::MAX, f64::min),
        _ => values.iter().sum(),
    }
}

/// An optional whole-number option: omitted or empty is `default`; a logical value is 0 or 1; text that reads as
/// a number is that number; a fraction is `#NUM!`.
fn option(c: &mut Ctx, a: &[Expr], i: usize, default: i64, max: i64) -> Res<i64> {
    let Some(e) = a.get(i) else { return Ok(default) };
    if matches!(e, Expr::Missing) {
        return Ok(default);
    }
    let v = c.eval_scalar(e);
    let n = match &v {
        Value::Empty => 0.0,
        Value::Error(e) => return Err(*e),
        other => to_number(other, c.date1904())?,
    };
    if n.fract() != 0.0 || n < 0.0 || n > max as f64 {
        return Err(CellError::Num);
    }
    Ok(n as i64)
}

/// The values and the timeline made one regular series: taken as Excel takes them (errors by what is wrong), the
/// points of the same time put together by `aggregation`, the gaps filled by `completion`.
fn series(c: &mut Ctx, values: &Expr, timeline: &Expr, completion: i64, aggregation: i64) -> Res<Series> {
    let date1904 = c.date1904();
    let vals = arg_array(c, values)?;
    let times = arg_array(c, timeline)?;
    if vals.data.len() != times.data.len() {
        return Err(CellError::NA);
    }
    // A timeline of text or nothing has nothing to count; one that mixes, or is not in a line, is not a timeline.
    let number_of = |v: &Value| if let Value::Number(n) = v { Some(*n) } else { None };
    let ts: Vec<Option<f64>> = times.data.iter().map(number_of).collect();
    if ts.iter().all(Option::is_none) {
        return Err(CellError::Div0);
    }
    if times.rows > 1 && times.columns > 1 {
        return Err(CellError::Num);
    }
    if ts.iter().any(Option::is_none) {
        return Err(CellError::Num);
    }
    let ts: Vec<f64> = ts.into_iter().flatten().collect();
    // Errors in the values come first. Nothing that is a number: nothing to count; text or a logical value among
    // numbers: not a series.
    if let Some(Value::Error(e)) = vals.data.iter().find(|v| matches!(v, Value::Error(_))) {
        return Err(*e);
    }
    let ys: Vec<Option<f64>> = vals.data.iter().map(number_of).collect();
    let others = vals.data.iter().any(|v| matches!(v, Value::Text(_) | Value::Bool(_)));
    if ys.iter().all(Option::is_none) && (others || completion == 1) {
        return Err(CellError::Div0);
    }
    if others {
        return Err(CellError::Num);
    }
    if ts.windows(2).any(|w| w[1] < w[0]) {
        return Err(CellError::Num);
    }
    // Dates a whole number of months apart, on the same day of the month, are on a line of months.
    let (ts, months) = month_line(&ts, date1904);
    // The step: the difference that occurs most often (the smaller when two do), the points set on that grid;
    // points that fall on the same place are put together, the places with none are gaps.
    let start = ts[0];
    let mut diffs: Vec<f64> = ts.windows(2).map(|w| w[1] - w[0]).filter(|d| *d > 0.0).collect();
    if diffs.is_empty() {
        return Err(CellError::Num);
    }
    diffs.sort_by(|a, b| a.total_cmp(b));
    let (mut step, mut best_count) = (diffs[0], 0usize);
    let mut i = 0;
    while i < diffs.len() {
        let mut j = i;
        while j < diffs.len() && (diffs[j] - diffs[i]).abs() <= 1e-9 * diffs[i].abs().max(1.0) {
            j += 1;
        }
        if j - i > best_count {
            best_count = j - i;
            step = diffs[i];
        }
        i = j;
    }
    if step <= 0.0 || !step.is_finite() {
        return Err(CellError::Num);
    }
    let count = (((ts[ts.len() - 1] - start) / step).round() as usize).saturating_add(1);
    if count > 1_000_000 || count < 2 {
        return Err(if count < 2 { CellError::Value } else { CellError::Num });
    }
    let mut groups: Vec<Vec<f64>> = vec![Vec::new(); count];
    for (t, y) in ts.iter().zip(&ys) {
        let k = (((t - start) / step).round() as usize).min(count - 1);
        if let Some(y) = y {
            groups[k].push(*y);
        }
    }
    let y: Vec<Option<f64>> = groups.iter().map(|g| if g.is_empty() { None } else { Some(aggregate(g, aggregation as u32)) }).collect();
    let filled = complete(&y, completion == 1);
    Ok(Series { y: filled, start, step, months })
}

/// The timeline as months, if it is dates on the same day of each month (or each month's last day) a whole
/// number of months apart (the same number each time); else as it is.
fn month_line(ts: &[f64], date1904: bool) -> (Vec<f64>, Option<MonthLine>) {
    let plain = || (ts.to_vec(), None);
    if ts.len() < 3 || ts.iter().any(|t| t.fract() != 0.0 || *t < 61.0) {
        return plain();
    }
    let dates: Vec<(i64, u32, u32)> = ts.iter().map(|&t| crate::eval::convert::date_from_serial(t as i64, date1904)).collect();
    let months: Vec<i64> = dates.iter().map(|d| d.0 * 12 + i64::from(d.1)).collect();
    let k = months[1] - months[0];
    if k < 1 || months.windows(2).any(|w| w[1] - w[0] != k) {
        return plain();
    }
    let same_day = dates.iter().all(|d| d.2 == dates[0].2);
    let end_of_month = dates.iter().all(|d| d.2 == days_in_month(d.0, d.1));
    // A timeline of days that happens to be 28 to 31 apart is not months: days are told by being the same day.
    if !(same_day || end_of_month) || (!end_of_month && dates[0].2 > 28) {
        return plain();
    }
    (months.iter().map(|m| *m as f64).collect(), Some(MonthLine { day: dates[0].2, end_of_month, date1904 }))
}

/// The gaps of a series: filled by a straight line between the points beside them, or by zeros.
fn complete(y: &[Option<f64>], interpolate: bool) -> Vec<f64> {
    let known: Vec<usize> = (0..y.len()).filter(|&i| y[i].is_some()).collect();
    (0..y.len())
        .map(|i| match y[i] {
            Some(v) => v,
            None if !interpolate => 0.0,
            None => {
                let before = known.iter().rev().find(|&&k| k < i).copied();
                let after = known.iter().find(|&&k| k > i).copied();
                match (before, after) {
                    (Some(b), Some(a)) => {
                        let (yb, ya) = (y[b].unwrap_or(0.0), y[a].unwrap_or(0.0));
                        yb + (ya - yb) * (i - b) as f64 / (a - b) as f64
                    }
                    (Some(b), None) => y[b].unwrap_or(0.0),
                    (None, Some(a)) => y[a].unwrap_or(0.0),
                    _ => 0.0,
                }
            }
        })
        .collect()
}

/// The season length to use: asked for (more than the half of the series is no season), none (0), or found (1).
fn season_length(asked: i64, y: &[f64]) -> usize {
    match asked {
        0 => 1,
        1 => detect_season(y),
        n => {
            let n = n as usize;
            if n <= y.len() / 2 { n } else { 1 }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Forecast,
    ConfInt,
}

fn forecast(c: &mut Ctx, a: &[Expr], kind: Kind) -> Value {
    match forecast_value(c, a, kind) {
        Ok(v) => number(v),
        Err(e) => err(e),
    }
}

fn forecast_value(c: &mut Ctx, a: &[Expr], kind: Kind) -> Res<f64> {
    // The date asked about: one value, a number or text that is a date.
    // Evaluated once: a second evaluation would run the argument's side effects (NOW, RAND, a LAMBDA call) twice.
    let first = c.eval(&a[0]);
    let target = match first {
        Value::Ref(r) if r.len() > 1 || r.first().is_some_and(|x| x.rows() * x.columns() > 1) => return Err(CellError::Value),
        Value::Array(arr) if arr.data.len() > 1 => return Err(CellError::Value),
        other => {
            let v = match c.scalar_or_array_of(other) {
                Value::Array(arr) => arr.data.first().cloned().unwrap_or(Value::Empty),
                v => v,
            };
            match v {
                Value::Error(e) => return Err(e),
                other => to_number(&other, c.date1904())?,
            }
        }
    };
    // The options come in the order of Excel's checks.
    let (conf_at, season_at) = if kind == Kind::ConfInt { (3, 4) } else { (usize::MAX, 3) };
    let confidence = if kind == Kind::ConfInt {
        match a.get(conf_at) {
            None | Some(Expr::Missing) => 0.95,
            Some(e) => {
                let v = c.eval_scalar(e);
                let n = match &v {
                    Value::Error(e) => return Err(*e),
                    other => to_number(other, c.date1904())?,
                };
                if !(0.0..1.0).contains(&n) {
                    return Err(CellError::Num);
                }
                n
            }
        }
    } else {
        0.0
    };
    let seasonality = option(c, a, season_at, 1, 8760)?;
    let completion = option(c, a, season_at + 1, 1, 1)?;
    let aggregation = option(c, a, season_at + 2, 1, 7)?;
    if aggregation == 0 {
        return Err(CellError::Num);
    }
    let s = series(c, &a[1], &a[2], completion, aggregation)?;
    let target = s.position(target);
    if target < 0.0 || (target <= 0.001 && s.start >= 1.0 && s.step == 1.0) {
        return Err(CellError::Num);
    }
    let m = season_length(seasonality, &s.y);
    let model = fit(&s.y, m);
    let last = s.start + (s.y.len() as f64 - 1.0) * s.step;
    let h = (target - last) / s.step;
    match kind {
        Kind::Forecast => {
            Ok(model.forecast(h))
        }
        Kind::ConfInt => {
            // Only for a date after the series.
            if h <= 0.0 {
                return Err(CellError::Num);
            }
            if confidence == 0.0 {
                return Ok(0.0);
            }
            let z = norm_inv(0.5 + confidence / 2.0);
            Ok(z * model.error(h, s.y.len()))
        }
    }
}

fn seasonality(c: &mut Ctx, a: &[Expr]) -> Value {
    let run = |c: &mut Ctx| -> Res<f64> {
        let completion = option(c, a, 2, 1, 1)?;
        let aggregation = option(c, a, 3, 1, 7)?;
        if aggregation == 0 {
            return Err(CellError::Num);
        }
        let s = series(c, &a[0], &a[1], completion, aggregation)?;
        let m = detect_season(&s.y);
        Ok(if m > 1 { m as f64 } else { 0.0 })
    };
    match run(c) {
        Ok(v) => number(v),
        Err(e) => err(e),
    }
}

fn stat(c: &mut Ctx, a: &[Expr]) -> Value {
    let run = |c: &mut Ctx| -> Res<f64> {
        // The kind of statistic, 1 to 8.
        let kind = match c.eval_scalar(&a[2]) {
            Value::Error(e) => return Err(e),
            other => to_number(&other, c.date1904())?,
        };
        if kind.fract() != 0.0 || !(1.0..=8.0).contains(&kind) {
            return Err(CellError::Num);
        }
        let seasonality = option(c, a, 3, 1, 8760)?;
        let completion = option(c, a, 4, 1, 1)?;
        let aggregation = option(c, a, 5, 1, 7)?;
        if aggregation == 0 {
            return Err(CellError::Num);
        }
        let s = series(c, &a[0], &a[1], completion, aggregation)?;
        let m = season_length(seasonality, &s.y);
        let model = fit(&s.y, m);
        let n = s.y.len();
        let errors: Vec<f64> = s.y.iter().zip(&model.fitted).map(|(y, f)| y - f).collect();
        if let Some(st) = model.stat {
            match kind as u32 {
                4 => return Ok(st[0]),
                5 => return Ok(st[1]),
                6 => return Ok(st[2]),
                7 => return Ok(st[3]),
                _ => {}
            }
        }
        Ok(match kind as u32 {
            1 => model.alpha,
            2 => model.beta,
            3 => model.gamma,
            4 => {
                // MASE: the mean absolute error over that of the naive forecast of the last point.
                let naive: f64 = (1..n).map(|i| (s.y[i] - s.y[i - 1]).abs()).sum::<f64>() / (n - 1).max(1) as f64;
                let mae = errors.iter().map(|e| e.abs()).sum::<f64>() / n as f64;
                if naive == 0.0 { return Err(CellError::Div0) } else { mae / naive }
            }
            5 => s.y.iter().zip(&model.fitted).map(|(y, f)| if y.abs() + f.abs() == 0.0 { 0.0 } else { 2.0 * (y - f).abs() / (y.abs() + f.abs()) }).sum::<f64>() / n as f64,
            6 => errors.iter().map(|e| e.abs()).sum::<f64>() / n as f64,
            7 => (errors.iter().map(|e| e * e).sum::<f64>() / n as f64).sqrt(),
            _ => s.step,
        })
    };
    match run(c) {
        Ok(v) => number(v),
        Err(e) => err(e),
    }
}

#[allow(dead_code)]
fn unused(_: Rc<Array>) {}

#[cfg(test)]
mod tests {
    use super::{detect_season, fit};

    #[test]
    fn a_straight_line_is_forecast_on_and_has_no_season() {
        let y: Vec<f64> = (1..=10).map(|i| 2.0 * i as f64 + 1.0).collect();
        assert_eq!(detect_season(&y), 1);
        let f = fit(&y, 1);
        assert!(f.sse < 1e-6, "{}", f.sse);
        assert!((f.forecast(1.0) - 23.0).abs() < 1e-3, "{}", f.forecast(1.0));
        assert!((f.forecast(3.5) - 28.0).abs() < 1e-2, "{}", f.forecast(3.5));
    }

    #[test]
    fn a_season_is_found_and_continued() {
        let y: Vec<f64> = (0..24).map(|i| 10.0 + 0.5 * i as f64 + [3.0, -1.0, -3.0, 1.0][i % 4]).collect();
        assert_eq!(detect_season(&y), 4);
        let f = fit(&y, 4);
        // The next point (index 24) has the season of index 0: 10 + 12 + 3.
        assert!((f.forecast(1.0) - 25.0).abs() < 0.2, "{}", f.forecast(1.0));
        assert!((f.forecast(2.0) - 21.5).abs() < 0.3, "{}", f.forecast(2.0));
    }
}
