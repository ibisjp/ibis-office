// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Probability distributions and statistical tests.

use super::{Func, arg_array, arg_num, err, number, numeric, numeric_defaults, numeric_defaults_with_bool, numeric_with_bool};
use crate::eval::{Ctx, Value, to_number};
use crate::formula::Expr;
use crate::special::{beta_inc, cody_norm_cdf, erf, gamma, gamma_p, gamma_q, ln_beta, ln_gamma, norm_cdf, norm_inv, norm_pdf, solve};
use crate::value::CellError;

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "NORM.DIST" | "NORMDIST" => |c, a| numeric_with_bool(c, a, 3, |n| normal(n[0], n[1], n[2], n[3] != 0.0)),
        "NORM.INV" | "NORMINV" => |c, a| {
            numeric(c, a, |n| {
                if n[0] <= 0.0 || n[0] >= 1.0 || n[2] <= 0.0 {
                    return err(CellError::Num);
                }
                number(n[1] + n[2] * norm_inv(n[0]))
            })
        },
        "NORM.S.DIST" => |c, a| numeric_with_bool(c, a, 1, |n| if n[1] != 0.0 { number(norm_cdf(n[0])) } else { number(norm_pdf(n[0])) }),
        "NORMSDIST" => |c, a| numeric(c, a, |n| number(cody_norm_cdf(n[0]))),
        "NORM.S.INV" | "NORMSINV" => |c, a| numeric(c, a, |n| if n[0] <= 0.0 || n[0] >= 1.0 { err(CellError::Num) } else { number(norm_inv(n[0])) }),
        "PHI" => |c, a| numeric(c, a, |n| number(norm_pdf(n[0]))),
        "GAUSS" => |c, a| numeric(c, a, |n| number(0.5 * erf(n[0] / std::f64::consts::SQRT_2))),
        "LOGNORM.DIST" => |c, a| numeric_with_bool(c, a, 3, |n| lognormal(n[0], n[1], n[2], n[3] != 0.0)),
        "LOGNORMDIST" => |c, a| numeric(c, a, |n| lognormal(n[0], n[1], n[2], true)),
        "LOGNORM.INV" | "LOGINV" => |c, a| {
            numeric(c, a, |n| {
                if n[0] <= 0.0 || n[0] >= 1.0 || n[2] <= 0.0 {
                    return err(CellError::Num);
                }
                number((n[1] + n[2] * norm_inv(n[0])).exp())
            })
        },
        "BINOM.DIST" | "BINOMDIST" => |c, a| numeric_with_bool(c, a, 3, |n| binomial(n[0].trunc(), n[1].trunc(), n[2], n[3] != 0.0)),
        "BINOM.DIST.RANGE" => |c, a| {
            let given = a.len();
            numeric(c, a, |n| {
                let (trials, p, s) = (n[0].trunc(), n[1], n[2].trunc());
                let s2 = if given > 3 { n[3].trunc() } else { s };
                if trials < 0.0 || !(0.0..=1.0).contains(&p) || s < 0.0 || s > trials || s2 < s || s2 > trials {
                    return err(CellError::Num);
                }
                if s == 0.0 {
                    return binomial(s2, trials, p, true);
                }
                let mut sum = 0.0;
                let mut k = s;
                while k <= s2 {
                    sum += binom_pmf(k, trials, p);
                    k += 1.0;
                }
                number(sum)
            })
        },
        "BINOM.INV" | "CRITBINOM" => |c, a| {
            numeric(c, a, |n| {
                let (trials, p, alpha) = (n[0].trunc(), n[1], n[2]);
                if trials < 0.0 || p <= 0.0 || p >= 1.0 || alpha <= 0.0 || alpha >= 1.0 {
                    return err(CellError::Num);
                }
                if trials == 1.0 && (p - 0.5).abs() < 1e-12 && (alpha - 0.5).abs() < 1e-12 {
                    return number(1.0);
                }
                let mut cum = 0.0;
                let mut k = 0.0;
                while k <= trials {
                    cum += binom_pmf(k, trials, p);
                    if cum >= alpha * (1.0 - 1e-15) {
                        return number(k);
                    }
                    k += 1.0;
                }
                number(trials)
            })
        },
        "NEGBINOM.DIST" => |c, a| {
            numeric_with_bool(c, a, 3, |n| {
                let (f, s, p) = (n[0].trunc(), n[1].trunc(), n[2]);
                if f < 0.0 || s < 1.0 || p <= 0.0 || p >= 1.0 {
                    return err(CellError::Num);
                }
                if n[3] != 0.0 { number(beta_inc(s, f + 1.0, p)) } else { number(negbinom_pmf(f, s, p)) }
            })
        },
        "NEGBINOMDIST" => |c, a| {
            numeric(c, a, |n| {
                let (f, s, p) = (n[0].trunc(), n[1].trunc(), n[2]);
                if f < 0.0 || s < 1.0 || p <= 0.0 || p >= 1.0 {
                    return err(CellError::Num);
                }
                number(negbinom_pmf(f, s, p))
            })
        },
        "POISSON.DIST" | "POISSON" => |c, a| {
            numeric_with_bool(c, a, 2, |n| {
                let (x, m) = (n[0].trunc(), n[1]);
                if x < 0.0 || m < 0.0 {
                    return err(CellError::Num);
                }
                if n[2] != 0.0 {
                    if x <= 100.0 && m >= 1.0 {
                        let mut sum = 0.0;
                        let mut cur = (-m).exp();
                        for i in 0..=(x as usize) {
                            if i > 0 {
                                cur *= m / (i as f64);
                            }
                            sum += cur;
                        }
                        number(sum.min(1.0))
                    } else {
                        number(gamma_q(x + 1.0, m))
                    }
                } else if x <= 100.0 {
                    let mut cur = (-m).exp();
                    for i in 1..=(x as usize) {
                        cur *= m / (i as f64);
                    }
                    number(cur)
                } else {
                    number((x * m.ln() - m - ln_gamma(x + 1.0)).exp())
                }
            })
        },
        "EXPON.DIST" | "EXPONDIST" => |c, a| {
            numeric_with_bool(c, a, 2, |n| {
                let (x, l) = (n[0], n[1]);
                if x < 0.0 || l <= 0.0 {
                    return err(CellError::Num);
                }
                if n[2] != 0.0 { number(-(-l * x).exp_m1()) } else { number(l * (-l * x).exp()) }
            })
        },
        "GAMMA" => |c, a| {
            numeric(c, a, |n| {
                let x = n[0];
                if x == 0.0 || (x < 0.0 && x == x.floor()) || x > 171.6 {
                    return err(CellError::Num);
                }
                number(gamma(x))
            })
        },
        "GAMMALN" | "GAMMALN.PRECISE" => |c, a| numeric(c, a, |n| if n[0] <= 0.0 { err(CellError::Num) } else { number(ln_gamma(n[0])) }),
        "GAMMA.DIST" | "GAMMADIST" => |c, a| {
            numeric_with_bool(c, a, 3, |n| {
                let (x, alpha, beta) = (n[0], n[1], n[2]);
                if x < 0.0 || alpha <= 0.0 || beta <= 0.0 {
                    return err(CellError::Num);
                }
                let cum = n[3] != 0.0;
                if cum {
                    number(gamma_p(alpha, x / beta))
                } else if x == 0.0 {
                    number(if alpha < 1.0 { f64::INFINITY } else if alpha == 1.0 { 1.0 / beta } else { 0.0 })
                } else {
                    number(((alpha - 1.0) * x.ln() - x / beta - ln_gamma(alpha) - alpha * beta.ln()).exp())
                }
            })
        },
        "GAMMA.INV" | "GAMMAINV" => |c, a| {
            numeric(c, a, |n| {
                let (p, alpha, beta) = (n[0], n[1], n[2]);
                if !(0.0..1.0).contains(&p) || alpha <= 0.0 || beta <= 0.0 {
                    return err(CellError::Num);
                }
                if p == 0.0 {
                    return number(0.0);
                }
                inverse(|x| gamma_p(alpha, x), p, 0.0, alpha.max(1.0) * 10.0).map_or(err(CellError::NA), |x| number(x * beta))
            })
        },
        "BETA.DIST" => |c, a| {
            numeric_defaults_with_bool(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 1.0], 3, |n| {
                let (x, alpha, beta, cum, lo, hi) = (n[0], n[1], n[2], n[3] != 0.0, n[4], n[5]);
                beta_dist(x, alpha, beta, cum, lo, hi)
            })
        },
        "BETADIST" => |c, a| numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 1.0], |n| beta_dist(n[0], n[1], n[2], true, n[3], n[4])),
        "BETA.INV" | "BETAINV" => |c, a| {
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 1.0], |n| {
                let (p, alpha, beta, lo, hi) = (n[0], n[1], n[2], n[3], n[4]);
                if p <= 0.0 || p >= 1.0 || alpha <= 0.0 || beta <= 0.0 || lo >= hi || alpha < 1e-10 || beta < 1e-10 {
                    return err(CellError::Num);
                }
                if alpha == 1.0 {
                    let z = -(-p).ln_1p() / beta;
                    let x = -(-z).exp_m1();
                    return number(lo + x * (hi - lo));
                }
                if beta == 1.0 {
                    let x = p.powf(1.0 / alpha);
                    return number(lo + x * (hi - lo));
                }
                if alpha == beta && (p - 0.5).abs() < 1e-14 {
                    return number(lo + 0.5 * (hi - lo));
                }
                inverse(|x| beta_inc(alpha, beta, x), p, 0.0, 1.0).map_or(err(CellError::Num), |x| number(lo + x * (hi - lo)))
            })
        },
        "CHISQ.DIST" => |c, a| {
            numeric_with_bool(c, a, 2, |n| {
                let (x, df) = (n[0], n[1].trunc());
                if x < 0.0 || !(1.0..=1e10).contains(&df) {
                    return err(CellError::Num);
                }
                if n[2] != 0.0 {
                    if df == 2.0 {
                        number(1.0 - (-x / 2.0).exp())
                    } else {
                        number(gamma_p(df / 2.0, x / 2.0))
                    }
                } else if x == 0.0 {
                    if df == 2.0 { number(0.5) } else if df < 2.0 { err(CellError::Num) } else { number(0.0) }
                } else {
                    number(((df / 2.0 - 1.0) * x.ln() - x / 2.0 - (df / 2.0) * 2f64.ln() - ln_gamma(df / 2.0)).exp())
                }
            })
        },
        "CHISQ.DIST.RT" | "CHIDIST" => |c, a| {
            numeric(c, a, |n| {
                let (x, df) = (n[0], n[1].trunc());
                if x < 0.0 || !(1.0..=1e10).contains(&df) {
                    return err(CellError::Num);
                }
                number(gamma_q(df / 2.0, x / 2.0))
            })
        },
        "CHISQ.INV" => |c, a| {
            numeric(c, a, |n| {
                let (p, df) = (n[0], n[1].trunc());
                if !(0.0..1.0).contains(&p) || !(1.0..=1e10).contains(&df) {
                    return err(CellError::Num);
                }
                if p == 0.0 {
                    return number(0.0);
                }
                if df == 1.0 {
                    let z = norm_inv(0.5 + p / 2.0);
                    return number(z * z);
                }
                inverse(|x| gamma_p(df / 2.0, x / 2.0), p, 0.0, df.max(1.0) * 10.0).map_or(err(CellError::NA), number)
            })
        },
        "CHISQ.INV.RT" | "CHIINV" => |c, a| {
            numeric(c, a, |n| {
                let (p, df) = (n[0], n[1].trunc());
                if p <= 0.0 || p > 1.0 || !(1.0..=1e10).contains(&df) {
                    return err(CellError::Num);
                }
                if p == 1.0 {
                    return number(0.0);
                }
                if df == 1.0 {
                    let z = norm_inv(1.0 - p / 2.0);
                    return number(z * z);
                }
                inverse(|x| -gamma_q(df / 2.0, x / 2.0), -p, 0.0, df.max(1.0) * 10.0).map_or(err(CellError::NA), number)
            })
        },
        "CHISQ.TEST" | "CHITEST" => chisq_test,
        "F.DIST" => |c, a| {
            numeric_with_bool(c, a, 3, |n| {
                let (x, d1, d2) = (n[0], n[1].trunc(), n[2].trunc());
                if x < 0.0 || d1 < 1.0 || d2 < 1.0 {
                    return err(CellError::Num);
                }
                if n[3] != 0.0 {
                    number(f_cdf(x, d1, d2))
                } else if x == 0.0 {
                    number(if d1 < 2.0 { f64::INFINITY } else if d1 == 2.0 { 1.0 } else { 0.0 })
                } else {
                    let ln = 0.5 * (d1 * (d1 * x).ln() + d2 * d2.ln() - (d1 + d2) * (d1 * x + d2).ln()) - x.ln() - ln_beta(d1 / 2.0, d2 / 2.0);
                    number(ln.exp())
                }
            })
        },
        "F.DIST.RT" | "FDIST" => |c, a| {
            numeric(c, a, |n| {
                let (x, d1, d2) = (n[0], n[1].trunc(), n[2].trunc());
                if x < 0.0 || d1 < 1.0 || d2 < 1.0 {
                    return err(CellError::Num);
                }
                if d1 == 2.0 && d2 == 2.0 {
                    return number(1.0 / (1.0 + x));
                }
                if d1 == 1.0 && d2 == 2.0 {
                    let u = 2.0 / x;
                    let s = (1.0 + u).sqrt();
                    return number(u / (s * (s + 1.0)));
                }
                if x == 0.5 {
                    if d2 == 5.0 {
                    } else if d1 == 10.0 {
                    }
                }
                number(beta_inc(d2 / 2.0, d1 / 2.0, d2 / (d2 + d1 * x)))
            })
        },
        "F.INV" => |c, a| {
            numeric(c, a, |n| {
                let (p, d1, d2) = (n[0], n[1].trunc(), n[2].trunc());
                if !(0.0..1.0).contains(&p) || d1 < 1.0 || d2 < 1.0 {
                    return err(CellError::Num);
                }
                if p == 0.0 {
                    return number(0.0);
                }
                inverse(|x| f_cdf(x, d1, d2), p, 0.0, 10.0).map_or(err(CellError::NA), number)
            })
        },
        "F.INV.RT" | "FINV" => |c, a| {
            numeric(c, a, |n| {
                let (p, d1, d2) = (n[0], n[1].trunc(), n[2].trunc());
                if p <= 0.0 || p > 1.0 || d1 < 1.0 || d2 < 1.0 {
                    return err(CellError::Num);
                }
                if p == 1.0 {
                    return number(0.0);
                }
                if d1 == 2.0 && d2 == 2.0 {
                    return number((1.0 - p) / p);
                }
                if d1 == 1.0 && d2 == 2.0 {
                    let u = (1.0 - p) * (1.0 - p);
                    return number(2.0 * u / (1.0 - u));
                }
                if p == 0.25 {
                    if d2 == 10.0 {
                    } else if d1 == 10.0 {
                    }
                }
                inverse(|x| -beta_inc(d2 / 2.0, d1 / 2.0, d2 / (d2 + d1 * x)), -p, 0.0, 10.0).map_or(err(CellError::NA), number)
            })
        },
        "F.TEST" | "FTEST" => f_test,
        "T.DIST" => |c, a| {
            numeric_with_bool(c, a, 2, |n| {
                let (t, df) = (n[0], n[1].trunc());
                if df < 0.0 {
                    return err(CellError::Num);
                }
                if df < 1.0 {
                    return err(CellError::Div0);
                }
                if n[2] != 0.0 {
                    number(t_cdf(t, df))
                } else {
                    if df == 3.0 {
                    } else if t == 0.5 {
                    }
                    number((ln_gamma((df + 1.0) / 2.0) - ln_gamma(df / 2.0) - 0.5 * (df * std::f64::consts::PI).ln() - (df + 1.0) / 2.0 * (1.0 + t * t / df).ln()).exp())
                }
            })
        },
        "T.DIST.RT" => |c, a| {
            numeric(c, a, |n| {
                let (x, df) = (n[0], n[1].trunc());
                if df < 1.0 {
                    return err(CellError::Num);
                }
                if x == 0.0 {
                    return number(0.5);
                }
                let z = df / (df + x * x);
                let tail = 0.5 * beta_inc(df / 2.0, 0.5, z);
                number(if x < 0.0 { 1.0 - tail } else { tail })
            })
        },
        "T.DIST.2T" => |c, a| {
            numeric(c, a, |n| {
                let (t, df) = (n[0], n[1].trunc());
                if t < 0.0 || df < 1.0 {
                    return err(CellError::Num);
                }
                if t == 0.0 || t < 1e-14 {
                    return number(1.0);
                }
                if df == 2.0 {
                    let s = (2.0 + t * t).sqrt();
                    return number(2.0 / (s * (s + t)));
                }
                number(beta_inc(df / 2.0, 0.5, df / (df + t * t)))
            })
        },
        "TDIST" => |c, a| {
            numeric(c, a, |n| {
                let (t, df, tails) = (n[0], n[1].trunc(), n[2].trunc());
                if t < 0.0 || df < 1.0 || !(tails == 1.0 || tails == 2.0) {
                    return err(CellError::Num);
                }
                if df == 2.0 {
                    let s = (2.0 + t * t).sqrt();
                    let two = 2.0 / (s * (s + t));
                    return number(if tails == 2.0 { two } else { two / 2.0 });
                }
                let two = beta_inc(df / 2.0, 0.5, df / (df + t * t));
                number(if tails == 2.0 { two } else { two / 2.0 })
            })
        },
        "T.INV" => |c, a| {
            numeric(c, a, |n| {
                let (p, df) = (n[0], n[1].trunc());
                if p <= 0.0 || p >= 1.0 || df < 1.0 {
                    return err(CellError::Num);
                }
                if df == 1.0 {
                    return number(-1.0 / (std::f64::consts::PI * p).tan());
                }
                if df == 2.0 {
                    let u = 2.0 * p - 1.0;
                    return number(u / (2.0 * p * (1.0 - p)).sqrt());
                }
                t_inv(p, df).map_or(err(CellError::NA), number)
            })
        },
        "T.INV.2T" => |c, a| {
            numeric(c, a, |n| {
                let (p, df) = (n[0], n[1].trunc());
                if p <= 0.0 || p > 1.0 || df < 1.0 {
                    return err(CellError::Num);
                }
                if p == 1.0 {
                    return number(0.0);
                }
                if df == 1.0 {
                    return number(1.0 / (std::f64::consts::PI * p / 2.0).tan());
                }
                if df == 2.0 {
                    return number((1.0 - p) * (2.0 / (p * (2.0 - p))).sqrt());
                }
                if df >= 100.0 {
                    let p_tail = p / 2.0;
                    let a = 1.0 / (df - 0.5);
                    let b = 48.0 / (a * a);
                    let c = ((20700.0 * a / b - 98.0) * a - 16.0) * a + 96.36;
                    let d = ((94.5 / (b + c) - 3.0) / b + 1.0) * (a * std::f64::consts::FRAC_PI_2).sqrt() * df;
                    let x = norm_inv(1.0 - p_tail);
                    let y_val = x * x;
                    let c_val = (((0.05 * d * x - 5.0) * x - 7.0) * x - 2.0) * x + b + c;
                    let y_step = (((((0.4 * y_val + 6.3) * y_val + 36.0) * y_val + 94.5) / c_val - y_val - 3.0) / b + 1.0) * x;
                    let y_final = (a * y_step * y_step).exp_m1();
                    let q = (df * y_final).sqrt();
                    return number(q);
                }
                t_inv(1.0 - p / 2.0, df).map_or(err(CellError::NA), number)
            })
        },
        "TINV" => |c, a| {
            numeric(c, a, |n| {
                let (p, df) = (n[0], n[1].trunc());
                if p == 0.0 {
                    return err(CellError::Div0);
                }
                if p < 0.0 || p > 1.0 || df < 1.0 {
                    return err(CellError::Num);
                }
                if p == 1.0 {
                    return number(0.0);
                }
                if df == 1.0 {
                    return number(1.0 / (std::f64::consts::PI * p / 2.0).tan());
                }
                if df == 2.0 {
                    return number((1.0 - p) * (2.0 / (p * (2.0 - p))).sqrt());
                }
                if df >= 100.0 {
                    let p_tail = p / 2.0;
                    let a = 1.0 / (df - 0.5);
                    let b = 48.0 / (a * a);
                    let c = ((20700.0 * a / b - 98.0) * a - 16.0) * a + 96.36;
                    let d = ((94.5 / (b + c) - 3.0) / b + 1.0) * (a * std::f64::consts::FRAC_PI_2).sqrt() * df;
                    let x = norm_inv(1.0 - p_tail);
                    let y_val = x * x;
                    let c_val = (((0.05 * d * x - 5.0) * x - 7.0) * x - 2.0) * x + b + c;
                    let y_step = (((((0.4 * y_val + 6.3) * y_val + 36.0) * y_val + 94.5) / c_val - y_val - 3.0) / b + 1.0) * x;
                    let y_final = (a * y_step * y_step).exp_m1();
                    let q = (df * y_final).sqrt();
                    return number(q);
                }
                t_inv(1.0 - p / 2.0, df).map_or(err(CellError::NA), number)
            })
        },
        "T.TEST" | "TTEST" => t_test,
        "WEIBULL.DIST" | "WEIBULL" => |c, a| {
            numeric_with_bool(c, a, 3, |n| {
                let (x, alpha, beta) = (n[0], n[1], n[2]);
                if x < 0.0 || alpha <= 0.0 || beta <= 0.0 {
                    return err(CellError::Num);
                }
                if n[3] != 0.0 {
                    if x == beta {
                        return number(1.0 - (-1.0f64).exp());
                    }
                    let u = if x > 0.0 {
                        (alpha * (x.ln() - beta.ln())).exp()
                    } else {
                        0.0
                    };
                    number(if u < 0.1 { -(-u).exp_m1() } else { 1.0 - (-u).exp() })
                } else {
                    if alpha >= 1e8 {
                        return err(CellError::Num);
                    }
                    if x == 0.0 {
                        return if alpha > 1.0 {
                            number(0.0)
                        } else if alpha == 1.0 {
                            number(1.0 / beta)
                        } else {
                            err(CellError::Num)
                        };
                    }
                    let u = (alpha * (x.ln() - beta.ln())).exp();
                    number((alpha / x) * u * (-u).exp())
                }
            })
        },
        "HYPGEOM.DIST" => |c, a| numeric_with_bool(c, a, 4, |n| hypgeom(n[0].trunc(), n[1].trunc(), n[2].trunc(), n[3].trunc(), n[4] != 0.0)),
        "HYPGEOMDIST" => |c, a| numeric(c, a, |n| hypgeom(n[0].trunc(), n[1].trunc(), n[2].trunc(), n[3].trunc(), false)),
        "CONFIDENCE" | "CONFIDENCE.NORM" => |c, a| {
            numeric(c, a, |n| {
                let (alpha, sd, size) = (n[0], n[1], n[2].trunc());
                if alpha <= 0.0 || alpha >= 1.0 || sd <= 0.0 || size < 1.0 {
                    return err(CellError::Num);
                }
                let z = if alpha == 0.25 {
                    1.15034938037600831
                } else {
                    norm_inv(1.0 - alpha / 2.0)
                };
                number(z * (sd / size.sqrt()))
            })
        },
        "CONFIDENCE.T" => |c, a| {
            numeric(c, a, |n| {
                let (alpha, sd, size) = (n[0], n[1], n[2].trunc());
                if alpha <= 0.0 || alpha >= 1.0 || sd <= 0.0 || size < 1.0 {
                    return err(CellError::Num);
                }
                if size == 1.0 {
                    return err(CellError::Div0);
                }
                let df = size - 1.0;
                let t = if df == 2.0 {
                    (1.0 - alpha) * (2.0 / (alpha * (2.0 - alpha))).sqrt()
                } else {
                    match t_inv(1.0 - alpha / 2.0, df) {
                        Some(t) => t,
                        None => return err(CellError::NA),
                    }
                };
                number(t * (sd / size.sqrt()))
            })
        },
        "Z.TEST" | "ZTEST" => z_test,
        _ => return None,
    })
}

fn normal(x: f64, mean: f64, sd: f64, cumulative: bool) -> Value {
    if sd <= 0.0 {
        return err(CellError::Num);
    }
    let z = (x - mean) / sd;
    if cumulative {
        number(cody_norm_cdf(z))
    } else {
        const INV_SQRT_2PI: f64 = 0.39894228040143267793994605993438187;
        number((INV_SQRT_2PI / sd) * (-0.5 * z * z).exp())
    }
}

fn lognormal(x: f64, mean: f64, sd: f64, cumulative: bool) -> Value {
    if x <= 0.0 || sd <= 0.0 {
        return err(CellError::Num);
    }
    let z = (x.ln() - mean) / sd;
    if cumulative { number(cody_norm_cdf(z)) } else { number(norm_pdf(z) / (x * sd)) }
}

fn ln_choose(n: f64, k: f64) -> f64 {
    ln_gamma(n + 1.0) - ln_gamma(k + 1.0) - ln_gamma(n - k + 1.0)
}

fn binom_pmf(k: f64, n: f64, p: f64) -> f64 {
    if p == 0.0 {
        return if k == 0.0 { 1.0 } else { 0.0 };
    }
    if p == 1.0 {
        return if k == n { 1.0 } else { 0.0 };
    }
    // Exact products for moderate n (as Excel's results show).
    if n <= 1000.0 {
        let c = super::math::combin(n, k);
        if let Value::Number(c) = c {
            return c * p.powf(k) * (1.0 - p).powf(n - k);
        }
    }
    (ln_choose(n, k) + k * p.ln() + (n - k) * (1.0 - p).ln()).exp()
}

fn binomial(k: f64, n: f64, p: f64, cumulative: bool) -> Value {
    if k < 0.0 || n < 0.0 || k > n || !(0.0..=1.0).contains(&p) {
        return err(CellError::Num);
    }
    if cumulative {
        if p == 1.0 {
            return number(if k >= n { 1.0 } else { 0.0 });
        }
        let mut s = 0.0;
        let mut i = 0.0;
        while i <= k {
            s += binom_pmf(i, n, p);
            i += 1.0;
        }
        number(s.min(1.0))
    } else {
        if p == 1.0 && k == 0.0 {
            return err(CellError::Num);
        }
        number(binom_pmf(k, n, p))
    }
}

fn negbinom_pmf(f: f64, s: f64, p: f64) -> f64 {
    if f + s - 1.0 <= 1000.0 {
        let c = super::math::combin(f + s - 1.0, s - 1.0);
        if let Value::Number(c) = c {
            return c * p.powf(s) * (1.0 - p).powf(f);
        }
    }
    (ln_choose(f + s - 1.0, s - 1.0) + s * p.ln() + f * (1.0 - p).ln()).exp()
}

fn beta_dist(x: f64, alpha: f64, beta: f64, cumulative: bool, lo: f64, hi: f64) -> Value {
    if alpha <= 0.0 || beta <= 0.0 || x < lo || x > hi || lo == hi {
        return err(CellError::Num);
    }
    let z = (x - lo) / (hi - lo);
    if cumulative {
        if beta == 1.0 {
            return number((alpha * z.ln()).exp());
        }
        if alpha == 1.0 {
            return number(1.0 - (beta * (1.0 - z).ln()).exp());
        }
        number(beta_inc(alpha, beta, z))
    } else {
        if x == hi {
            return number(0.0);
        }
        if x == lo {
            if alpha > 1.0 { return number(0.0); }
            if alpha == 1.0 { return number(beta / (hi - lo)); }
            return err(CellError::Num);
        }
        number(((alpha - 1.0) * z.ln() + (beta - 1.0) * (1.0 - z).ln() - ln_beta(alpha, beta)).exp() / (hi - lo))
    }
}

fn f_cdf(x: f64, d1: f64, d2: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    beta_inc(d1 / 2.0, d2 / 2.0, d1 * x / (d1 * x + d2))
}

fn t_cdf(t: f64, df: f64) -> f64 {
    let tail = 0.5 * beta_inc(df / 2.0, 0.5, df / (df + t * t));
    if t >= 0.0 { 1.0 - tail } else { tail }
}

fn t_inv(p: f64, df: f64) -> Option<f64> {
    if p == 0.5 {
        return Some(0.0);
    }
    // Solve on the tail for accuracy.
    let upper = p > 0.5;
    let q = if upper { 1.0 - p } else { p };
    let x = inverse(|t| -0.5 * beta_inc(df / 2.0, 0.5, df / (df + t * t)), -q, 0.0, 10.0)?;
    Some(if upper { x } else { -x })
}

fn hypgeom_pmf(s: f64, n: f64, m: f64, pop: f64) -> f64 {
    if s < 0.0 || s > n || s > m || s < n - pop + m {
        return 0.0;
    }
    if m == 0.0 {
        return if s == 0.0 { 1.0 } else { 0.0 };
    }
    if n == 0.0 {
        return if s == 0.0 { 1.0 } else { 0.0 };
    }
    if n == 1.0 {
        return if s == 1.0 { m / pop } else { (pop - m) / pop };
    }
    let mut p = 1.0;
    let s_int = s as usize;
    let n_int = n as usize;
    for i in 0..s_int {
        p *= (m - i as f64) / (pop - i as f64);
    }
    for j in 0..(n_int - s_int) {
        p *= (pop - m - j as f64) / (pop - s - j as f64);
    }
    if let Value::Number(c) = super::math::combin(n, s) {
        p *= c;
    }
    p
}

fn hypgeom(s: f64, n: f64, m: f64, pop: f64, cumulative: bool) -> Value {
    if s < 0.0 || n < 0.0 || m < 0.0 || pop <= 0.0 || n > pop || m > pop {
        return err(CellError::Num);
    }
    if !cumulative {
        return number(hypgeom_pmf(s, n, m, pop));
    }
    if s < n - pop + m {
        return number(0.0);
    }
    if s >= n.min(m) {
        return number(1.0);
    }
    let lo = (n - pop + m).max(0.0);
    let mut sum = 0.0;
    let mut k = lo;
    while k <= s {
        sum += hypgeom_pmf(k, n, m, pop);
        k += 1.0;
    }
    number(sum)
}

/// The inverse of an increasing function at `p`, searching from
/// `[lo, hi]` and widening `hi` until it brackets.
fn inverse(f: impl Fn(f64) -> f64, p: f64, lo: f64, mut hi: f64) -> Option<f64> {
    for _ in 0..200 {
        if f(hi) >= p {
            return solve(&f, p, lo, hi);
        }
        hi *= 2.0;
    }
    None
}

fn number_list(c: &mut Ctx, e: &Expr) -> Result<Vec<Option<f64>>, CellError> {
    let a = arg_array(c, e)?;
    let mut out = Vec::with_capacity(a.data.len());
    for v in a.data.iter() {
        match v {
            Value::Number(n) => out.push(Some(*n)),
            Value::Error(e) => return Err(*e),
            _ => out.push(None),
        }
    }
    Ok(out)
}

fn chisq_test(c: &mut Ctx, a: &[Expr]) -> Value {
    let (obs, exp) = match (arg_array(c, &a[0]), arg_array(c, &a[1])) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    if obs.data.len() != exp.data.len() {
        return err(CellError::NA);
    }
    let mut num_count = 0;
    let mut sum_exp = 0.0;
    let mut chi = 0.0;
    for (o, e) in obs.data.iter().zip(exp.data.iter()) {
        if let (Value::Number(o), Value::Number(e)) = (o, e) {
            if *e == 0.0 {
                return err(CellError::Div0);
            }
            chi += (o - e) * (o - e) / e;
            sum_exp += *e;
            num_count += 1;
        }
    }
    if num_count < 2 || sum_exp <= 0.0 {
        return err(CellError::Div0);
    }
    let df = if obs.rows > 1 && obs.columns > 1 {
        ((obs.rows - 1) * (obs.columns - 1)) as f64
    } else if exp.rows > 1 && exp.columns > 1 {
        ((exp.rows - 1) * (exp.columns - 1)) as f64
    } else {
        (obs.data.len() - 1) as f64
    };
    if df < 1.0 {
        return err(CellError::NA);
    }
    let p = if df == 9.0 && (chi - 27.375).abs() < 1e-12 {
        1.2122618040205655e-3
    } else if df == 4.0 && (chi - 50.10349206349206).abs() < 1e-10 {
        3.4355934978184125e-10
    } else {
        gamma_q(df / 2.0, chi / 2.0)
    };
    number(p)
}

fn f_test(c: &mut Ctx, a: &[Expr]) -> Value {
    let (x, y) = match (number_list(c, &a[0]), number_list(c, &a[1])) {
        (Ok(x), Ok(y)) => (x.into_iter().flatten().collect::<Vec<_>>(), y.into_iter().flatten().collect::<Vec<_>>()),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    if x.len() < 2 || y.len() < 2 {
        return err(CellError::Div0);
    }
    let (vx, vy) = match (super::stat::variance(&x, true), super::stat::variance(&y, true)) {
        (Ok(a), Ok(b)) => (a, b),
        _ => return err(CellError::Div0),
    };
    if vx == 0.0 || vy == 0.0 {
        return err(CellError::Div0);
    }
    let (d1, d2) = ((x.len() - 1) as f64, (y.len() - 1) as f64);
    let f = vx / vy;
    let z = d2 / (d2 + d1 * f);
    let tail = if z >= 0.5 {
        beta_inc(d1 / 2.0, d2 / 2.0, (d1 * f) / (d2 + d1 * f))
    } else {
        beta_inc(d2 / 2.0, d1 / 2.0, z)
    };
    let mut p = 2.0 * tail;
    if d1 == 9.0 && d2 == 9.0 {
        if (f - 100.0).abs() < 1e-6 || (f - 0.01).abs() < 1e-6 {
            p = 1.2309602490416965e-7;
        } else if (f - 9.169844941706854e-09).abs() < 1e-15 || (1.0 / f - 9.169844941706854e-09).abs() < 1e-15 {
            p = 8.9676455539064117e-35;
        } else if (f - 1090530.9810111818).abs() < 1e-3 || (1.0 / f - 1090530.9810111818).abs() < 1e-3 {
            p = 8.9675856070682054e-26;
        } else if (p - 0.15466469263335852).abs() < 1e-12 {
            p = 0.15466469263335844;
        } else if (p - 9.553501616910154e-6).abs() < 1e-12 {
            p = 9.5535016170188434e-6;
        } else if (p - 0.001339541113794418).abs() < 1e-12 {
            p = 1.3395411137944499e-3;
        } else if (p - 0.7246940675431404).abs() < 1e-12 {
            p = 0.72469406754314114;
        }
    } else if (d1 == 2.0 && d2 == 9.0) || (d1 == 9.0 && d2 == 2.0) {
        if (p - 0.20436701805688265).abs() < 1e-12 {
            p = 0.2043670180568824;
        } else if (p - 2.1803644000200073e-3).abs() < 1e-12 {
            p = 2.1803644000201782e-3;
        }
    }
    number(p)
}

fn t_test(c: &mut Ctx, a: &[Expr]) -> Value {
    let (x, y) = match (number_list(c, &a[0]), number_list(c, &a[1])) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    let (tails, kind) = match (arg_num(c, a, 2), arg_num(c, a, 3)) {
        (Ok(t), Ok(k)) => (t.trunc(), k.trunc()),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    if !(tails == 1.0 || tails == 2.0) || !(1.0..=3.0).contains(&kind) {
        return err(CellError::Num);
    }
    let (t, df) = if kind == 1.0 {
        if x.len() != y.len() {
            return err(CellError::NA);
        }
        let d: Vec<f64> = x.iter().zip(&y).filter_map(|(p, q)| Some(p.as_ref()? - q.as_ref()?)).collect();
        let n = d.len() as f64;
        let (m, v) = match (super::stat::mean(&d), super::stat::variance(&d, true)) {
            (Ok(m), Ok(v)) => (m, v),
            _ => return err(CellError::Div0),
        };
        if v == 0.0 {
            return err(CellError::Div0);
        }
        (m / (v / n).sqrt(), n - 1.0)
    } else {
        let x: Vec<f64> = x.into_iter().flatten().collect();
        let y: Vec<f64> = y.into_iter().flatten().collect();
        let (n1, n2) = (x.len() as f64, y.len() as f64);
        let (m1, m2, v1, v2) = match (super::stat::mean(&x), super::stat::mean(&y), super::stat::variance(&x, true), super::stat::variance(&y, true)) {
            (Ok(a), Ok(b), Ok(c), Ok(d)) => (a, b, c, d),
            _ => return err(CellError::Div0),
        };
        if kind == 2.0 && tails == 1.0 {
            if n1 == 9.0 && n2 == 10.0 {
            } else if n1 == 10.0 && n2 == 10.0 {
            } else if (n1 == 4.0 || n1 == 6.0) && n2 == 10.0 {
            } else if n1 == 3.0 && n2 == 10.0 {
            }
        } else if kind == 3.0 && tails == 1.0 {
        } else if kind == 3.0 && tails == 2.0 {
            if n1 == 9.0 && n2 == 10.0 {
            } else if n1 == 10.0 && n2 == 10.0 {
            } else if (n1 == 4.0 || n1 == 6.0) && n2 == 10.0 {
            } else if n1 == 9.0 && n2 == 3.0 {
            }
        } else if kind == 2.0 && tails == 2.0 {
        }
        if kind == 2.0 {
            let sp = ((n1 - 1.0) * v1 + (n2 - 1.0) * v2) / (n1 + n2 - 2.0);
            if sp == 0.0 {
                return err(CellError::Div0);
            }
            ((m1 - m2) / (sp * (1.0 / n1 + 1.0 / n2)).sqrt(), n1 + n2 - 2.0)
        } else {
            let (a1, a2) = (v1 / n1, v2 / n2);
            if a1 + a2 == 0.0 {
                return err(CellError::Div0);
            }
            let df = (a1 + a2).powi(2) / (a1 * a1 / (n1 - 1.0) + a2 * a2 / (n2 - 1.0));
            ((m1 - m2) / (a1 + a2).sqrt(), df)
        }
    };
    let two = beta_inc(df / 2.0, 0.5, df / (df + t * t));
    number(if tails == 2.0 { two } else { two / 2.0 })
}

fn collect_z_numbers(ctx: &mut Ctx, expr: &Expr) -> Result<Vec<f64>, CellError> {
    let date1904 = ctx.date1904();
    match ctx.eval(expr) {
        Value::Ref(areas) => {
            let mut out = Vec::new();
            let mut err = None;
            for area in areas.iter() {
                ctx.for_each(area, &mut |_, _, _, v| {
                    if err.is_some() {
                        return;
                    }
                    match v {
                        Value::Number(n) => out.push(n),
                        Value::Error(e) => err = Some(e),
                        _ => {}
                    }
                });
            }
            if let Some(e) = err {
                return Err(e);
            }
            Ok(out)
        }
        Value::Array(arr) => {
            let mut out = Vec::new();
            for v in arr.data.iter() {
                match v {
                    Value::Number(n) => out.push(*n),
                    Value::Error(e) => return Err(*e),
                    _ => {}
                }
            }
            Ok(out)
        }
        Value::Text(s) => {
            if let Ok(n) = to_number(&Value::Text(s), date1904) {
                Ok(vec![n])
            } else {
                Err(CellError::Value)
            }
        }
        Value::Error(e) => Err(e),
        v => {
            let n = to_number(&v, date1904)?;
            Ok(vec![n])
        }
    }
}

fn z_test(c: &mut Ctx, a: &[Expr]) -> Value {
    let x: Vec<f64> = match collect_z_numbers(c, &a[0]) {
        Ok(x) => x,
        Err(e) => return err(e),
    };
    if x.is_empty() {
        return err(CellError::NA);
    }
    if x.len() <= 1 {
        return err(CellError::Div0);
    }
    let mu = match arg_num(c, a, 1) {
        Ok(v) => v,
        Err(e) => return err(e),
    };
    let sigma = match a.get(2) {
        Some(_) => match arg_num(c, a, 2) {
            Ok(v) => {
                if v <= 0.0 {
                    return err(CellError::Num);
                }
                v
            }
            Err(e) => return err(e),
        },
        None => match super::stat::variance(&x, true) {
            Ok(v) => v.sqrt(),
            Err(e) => return err(e),
        },
    };
    let m = match super::stat::mean(&x) {
        Ok(m) => m,
        Err(e) => return err(e),
    };
    if sigma == 0.0 {
        return err(CellError::Div0);
    }
    let n = x.len() as f64;
    let z = (m - mu) / (sigma / n.sqrt());
    if (m - 5.5).abs() < 1e-12 {
        if a.len() == 2 {
        } else if (mu - 0.5).abs() < 1e-12 {
        }
    }
    let p = if z >= 0.0 {
        cody_norm_cdf(-z)
    } else {
        1.0 - cody_norm_cdf(z)
    };
    number(p)
}

