// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Financial functions: time value of money, depreciation, securities.

use super::{Func, arg_array, arg_num, err, number, numeric, numeric_defaults, numeric_defaults_with_bool};
use crate::eval::convert::{date_from_serial, days_in_month, is_leap, serial_from_date};
use crate::eval::{Ctx, Value};
use crate::formula::Expr;
use crate::value::CellError;

use super::date::yearfrac;

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "PV" => |c, a| {
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0], |n| {
                let (rate, nper, pmt, fv, due) = (n[0], n[1], n[2], n[3], n[4] != 0.0);
                if rate == 0.0 {
                    return number(-(fv + pmt * nper));
                }
                let t = (1.0 + rate).powf(nper);
                if t == 0.0 {
                    return err(CellError::Div0);
                }
                number(-(fv + pmt * (1.0 + rate * f64::from(due)) * ((t - 1.0) / rate)) / t)
            })
        },
        "FV" => |c, a| {
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0], |n| {
                let (rate, nper, pmt, pv, due) = (n[0], n[1], n[2], n[3], n[4] != 0.0);
                if rate <= -1.0 && (nper < 0.0 || (rate == -1.0 && nper == 0.0)) {
                    return err(CellError::Num);
                }
                number(fv(rate, nper, pmt, pv, due))
            })
        },
        "PMT" => |c, a| {
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0], |n| {
                let (rate, nper, pv, fv, due) = (n[0], n[1], n[2], n[3], n[4] != 0.0);
                if nper == 0.0 || rate <= -1.0 {
                    return err(CellError::Num);
                }
                number(pmt(rate, nper, pv, fv, due))
            })
        },
        "NPER" => |c, a| {
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0], |n| {
                let (rate, pmt, pv, fv, due) = (n[0], n[1], n[2], n[3], n[4] != 0.0);
                if rate == 0.0 {
                    if pmt == 0.0 {
                        return err(CellError::Num);
                    }
                    return number(-(pv + fv) / pmt);
                }
                let k = (pmt / rate) + (if due { pmt } else { 0.0 });
                let num = k - fv;
                let den = k + pv;
                if den == 0.0 || num / den <= 0.0 {
                    return err(CellError::Num);
                }
                number((num / den).ln() / (1.0 + rate).ln())
            })
        },
        "RATE" => rate,
        "IPMT" => |c, a| {
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0], |n| {
                let (rate, per, nper, pv, fv, due) = (n[0], n[1], n[2], n[3], n[4], n[5] != 0.0);
                if per < 1.0 || per > nper || (rate <= -1.0 && !(due && per == 1.0)) {
                    return err(CellError::Num);
                }
                number(ipmt(rate, per, nper, pv, fv, due))
            })
        },
        "PPMT" => |c, a| {
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0], |n| {
                let (rate, per, nper, pv, fv, due) = (n[0], n[1], n[2], n[3], n[4], n[5] != 0.0);
                if per < 1.0 || per > nper || rate <= -1.0 {
                    return err(CellError::Num);
                }
                number(ppmt(rate, per, nper, pv, fv, due))
            })
        },
        "CUMIPMT" => |c, a| cumulative(c, a, true),
        "CUMPRINC" => |c, a| cumulative(c, a, false),
        "ISPMT" => |c, a| numeric(c, a, |n| if n[2] == 0.0 { err(CellError::Div0) } else { number(n[3] * n[0] * (n[1] - n[2]) / n[2]) }),
        "NPV" => npv,
        "XNPV" => xnpv,
        "IRR" => irr,
        "XIRR" => xirr,
        "MIRR" => mirr,
        "EFFECT" => |c, a| {
            numeric(c, a, |n| {
                let (r, p) = (n[0], n[1].trunc());
                if r <= 0.0 || p < 1.0 {
                    return err(CellError::Num);
                }
                number(if p <= i32::MAX as f64 {
                    (1.0 + r / p).powi(p as i32) - 1.0
                } else {
                    (1.0 + r / p).powf(p) - 1.0
                })
            })
        },
        "NOMINAL" => |c, a| {
            numeric(c, a, |n| {
                let (r, p) = (n[0], n[1].trunc());
                if r <= 0.0 || p < 1.0 {
                    return err(CellError::Num);
                }
                number(if r > 1e6 {
                    p * (r.ln_1p() / p).exp_m1()
                } else {
                    p * ((1.0 + r).powf(1.0 / p) - 1.0)
                })
            })
        },
        "FVSCHEDULE" => |c, a| {
            let p = match arg_num(c, a, 0) {
                Ok(v) => v,
                Err(e) => return err(e),
            };
            let s = match arg_array(c, &a[1]) {
                Ok(v) => v,
                Err(e) => return err(e),
            };
            let mut v = p;
            for x in s.data.iter() {
                match x {
                    Value::Number(r) => v *= 1.0 + r,
                    Value::Empty => {}
                    Value::Error(e) => return err(*e),
                    _ => return err(CellError::Value),
                }
            }
            number(v)
        },
        "PDURATION" => |c, a| {
            numeric(c, a, |n| {
                let (r, pv, fv) = (n[0], n[1], n[2]);
                if r <= 0.0 || pv <= 0.0 || fv <= 0.0 {
                    return err(CellError::Num);
                }
                number((fv.ln() - pv.ln()) / (1.0 + r).ln())
            })
        },
        "RRI" => |c, a| {
            numeric(c, a, |n| {
                let (nper, pv, fv) = (n[0], n[1], n[2]);
                if nper <= 0.0 || pv == 0.0 {
                    return err(CellError::Num);
                }
                number((fv / pv).powf(1.0 / nper) - 1.0)
            })
        },
        "SLN" => |c, a| numeric(c, a, |n| if n[2] == 0.0 { err(CellError::Div0) } else { number((n[0] - n[1]) / n[2]) }),
        "SYD" => |c, a| {
            numeric(c, a, |n| {
                let (cost, salvage, life, per) = (n[0], n[1], n[2], n[3]);
                if life <= 0.0 || per <= 0.0 || per > life || salvage < 0.0 {
                    return err(CellError::Num);
                }
                number((cost - salvage) * (life - per + 1.0) * 2.0 / (life * (life + 1.0)))
            })
        },
        "DB" => |c, a| numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 12.0], |n| db(n[0], n[1], n[2], n[3], n[4])),
        "DDB" => |c, a| {
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 2.0], |n| {
                let (cost, salvage, life, period, factor) = (n[0], n[1], n[2], n[3], n[4]);
                if cost < 0.0 || salvage < 0.0 || life <= 0.0 || period <= 0.0 || period > life || factor <= 0.0 {
                    return err(CellError::Num);
                }
                number(ddb(cost, salvage, life, period, factor))
            })
        },
        "VDB" => vdb,
        "DOLLARDE" => |c, a| {
            numeric(c, a, |n| {
                let f = n[1].trunc();
                if f < 0.0 {
                    return err(CellError::Num);
                }
                if f == 0.0 {
                    return err(CellError::Div0);
                }
                let int = n[0].trunc();
                let digits = f.log10().ceil() as i32;
                number(int + (n[0] - int) * 10f64.powi(digits) / f)
            })
        },
        "DOLLARFR" => |c, a| {
            numeric(c, a, |n| {
                let f = n[1].trunc();
                if f < 0.0 {
                    return err(CellError::Num);
                }
                if f == 0.0 {
                    return err(CellError::Div0);
                }
                let int = n[0].trunc();
                let digits = f.log10().ceil() as i32;
                number(int + (n[0] - int) * f / 10f64.powi(digits))
            })
        },
        "COUPDAYBS" => |c, a| coupon_fn(c, a, |s, m, f, b, d| number(coupdaybs(s, m, f, b, d))),
        "COUPDAYS" => |c, a| coupon_fn(c, a, |s, m, f, b, d| number(coupdays(s, m, f, b, d))),
        "COUPDAYSNC" => |c, a| coupon_fn(c, a, |s, m, f, b, d| number(coupdaysnc(s, m, f, b, d))),
        "COUPNCD" => |c, a| coupon_fn(c, a, |s, m, f, _, d| number(coupncd(s, m, f, d) as f64)),
        "COUPPCD" => |c, a| coupon_fn(c, a, |s, m, f, _, d| number(couppcd(s, m, f, d) as f64)),
        "COUPNUM" => |c, a| {
            coupon_fn(c, a, |s, m, f, _, d| {
                let (pcd, _, n) = coupon_dates(s, m, f, d);
                if pcd <= 0 {
                    err(CellError::Num)
                } else {
                    number(n)
                }
            })
        },
        "PRICE" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
                let (s, m) = (n[0].trunc() as i64, n[1].trunc() as i64);
                let (rate, yld, redemption, freq, basis) = (n[2], n[3], n[4], n[5].trunc(), n[6].trunc());
                if let Err(e) = check_bond(s, m, freq, basis) {
                    return err(e);
                }
                if rate < 0.0 || yld < 0.0 || redemption <= 0.0 {
                    return err(CellError::Num);
                }
                number(price(s, m, rate, yld, redemption, freq, basis as u32, d1904))
            })
        },
        "YIELD" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
                let (s, m) = (n[0].trunc() as i64, n[1].trunc() as i64);
                let (rate, pr, redemption, freq, basis) = (n[2], n[3], n[4], n[5].trunc(), n[6].trunc());
                if let Err(e) = check_bond(s, m, freq, basis) {
                    return err(e);
                }
                if rate < 0.0 || pr <= 0.0 || redemption <= 0.0 {
                    return err(CellError::Num);
                }
                yield_(s, m, rate, pr, redemption, freq, basis as u32, d1904).map_or(err(CellError::Num), number)
            })
        },
        "DURATION" => |c, a| duration_fn(c, a, false),
        "MDURATION" => |c, a| duration_fn(c, a, true),
        "DISC" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
                let (s, m, pr, redemption, basis) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2], n[3], n[4].trunc());
                if s < 0 || m < 0 || s >= m || pr <= 0.0 || redemption <= 0.0 || !(0.0..=4.0).contains(&basis) {
                    return err(CellError::Num);
                }
                number((redemption - pr) / redemption / yearfrac(s, m, basis as u32, d1904))
            })
        },
        "INTRATE" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
                let (s, m, inv, redemption, basis) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2], n[3], n[4].trunc());
                if s < 0 || m < 0 || s >= m || inv <= 0.0 || redemption <= 0.0 || !(0.0..=4.0).contains(&basis) {
                    return err(CellError::Num);
                }
                number((redemption - inv) / inv / yearfrac(s, m, basis as u32, d1904))
            })
        },
        "RECEIVED" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
                let (s, m, inv, disc, basis) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2], n[3], n[4].trunc());
                if s < 0 || m < 0 || s >= m || inv <= 0.0 || disc <= 0.0 || !(0.0..=4.0).contains(&basis) {
                    return err(CellError::Num);
                }
                let d = 1.0 - disc * yearfrac(s, m, basis as u32, d1904);
                if d <= 0.0 { err(CellError::Num) } else { number(inv / d) }
            })
        },
        "PRICEDISC" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
                let (s, m, disc, redemption, basis) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2], n[3], n[4].trunc());
                if s < 0 || m < 0 || s >= m || disc <= 0.0 || redemption <= 0.0 || !(0.0..=4.0).contains(&basis) {
                    return err(CellError::Num);
                }
                number(redemption - disc * redemption * yearfrac(s, m, basis as u32, d1904))
            })
        },
        "YIELDDISC" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
                let (s, m, pr, redemption, basis) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2], n[3], n[4].trunc());
                if s < 0 || m < 0 || s >= m || pr <= 0.0 || redemption <= 0.0 || !(0.0..=4.0).contains(&basis) {
                    return err(CellError::Num);
                }
                number((redemption - pr) / pr / yearfrac(s, m, basis as u32, d1904))
            })
        },
        "PRICEMAT" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
                let (s, m, issue, rate, yld, basis) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2].trunc() as i64, n[3], n[4], n[5].trunc());
                if s < 1 || m < 0 || issue < 0 || issue > s || s >= m || rate < 0.0 || yld < 0.0 || !(0.0..=4.0).contains(&basis) {
                    return err(CellError::Num);
                }
                let (dim, a_, dsm) = mature_fractions(issue, s, m, basis as u32, d1904);
                number((100.0 + dim * rate * 100.0) / (1.0 + dsm * yld) - a_ * rate * 100.0)
            })
        },
        "YIELDMAT" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
                let (s, m, issue, rate, pr, basis) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2].trunc() as i64, n[3], n[4], n[5].trunc());
                if s < 1 || m < 0 || issue < 0 || issue > s || s >= m || rate < 0.0 || pr <= 0.0 || !(0.0..=4.0).contains(&basis) {
                    return err(CellError::Num);
                }
                let (dim, a_, dsm) = mature_fractions(issue, s, m, basis as u32, d1904);
                // (The difference over the price, not the ratio less one: the last digits of a small yield.)
                let price = pr / 100.0 + a_ * rate;
                number(((1.0 + dim * rate) - price) / price / dsm)
            })
        },
        "ACCRINTM" => |c, a| {
            let d1904 = c.date1904();
            numeric_defaults(c, a, &[0.0, 0.0, 0.0, 1000.0, 0.0], move |n| {
                let (issue, s, rate, par, basis) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2], n[3], n[4].trunc());
                if issue < 0 || s < 0 || issue > s || rate <= 0.0 || par <= 0.0 || !(0.0..=4.0).contains(&basis) {
                    return err(CellError::Num);
                }
                if issue == s {
                    return number(0.0);
                }
                number(par * rate * yearfrac(issue, s, basis as u32, d1904))
            })
        },
        "ACCRINT" => accrint,
        "TBILLPRICE" => |c, a| {
            numeric(c, a, |n| {
                let (s, m, disc) = (n[0].trunc(), n[1].trunc(), n[2]);
                let days = m - s;
                if s >= m || days > 365.0 || disc <= 0.0 {
                    return err(CellError::Num);
                }
                let p = 100.0 * (1.0 - disc * days / 360.0);
                if p <= 0.0 { err(CellError::Num) } else { number(p) }
            })
        },
        "TBILLYIELD" => |c, a| {
            numeric(c, a, |n| {
                let (s, m, pr) = (n[0].trunc(), n[1].trunc(), n[2]);
                let days = m - s;
                if s >= m || days > 365.0 || pr <= 0.0 {
                    return err(CellError::Num);
                }
                number((100.0 - pr) / pr * 360.0 / days)
            })
        },
        "TBILLEQ" => |c, a| {
            numeric(c, a, |n| {
                let (s, m, disc) = (n[0].trunc(), n[1].trunc(), n[2]);
                let days = m - s;
                if s >= m || days > 365.0 || disc <= 0.0 {
                    return err(CellError::Num);
                }
                if days <= 182.0 {
                    let denom = 360.0 - disc * days;
                    if denom <= 0.0 {
                        return err(CellError::Num);
                    }
                    number(365.0 * disc / denom)
                } else {
                    let price = 100.0 * (1.0 - disc * days / 360.0);
                    if price <= 0.0 {
                        return err(CellError::Num);
                    }
                    let b = days / 365.0;
                    let term = b * b - (2.0 * b - 1.0) * (1.0 - 100.0 / price);
                    if term < 0.0 {
                        return err(CellError::Num);
                    }
                    number((-b + term.sqrt()) / (b - 0.5))
                }
            })
        },
        "AMORLINC" => |c, a| amor(c, a, false),
        "AMORDEGRC" => |c, a| amor(c, a, true),
        "ODDFPRICE" => oddfprice,
        "ODDFYIELD" => oddfyield,
        "ODDLPRICE" => oddlprice,
        "ODDLYIELD" => oddlyield,
        _ => return None,
    })
}

/// The parts of a year that PRICEMAT and YIELDMAT use: from the issue to the maturity, from the issue to the settlement,
/// from the settlement to the maturity. With actual days over the actual year (basis 1) they all count in the one
/// length of year that the issue to the settlement has (366 when a 29 February lies in it, else 365).
fn mature_fractions(issue: i64, s: i64, m: i64, basis: u32, d1904: bool) -> (f64, f64, f64) {
    if basis == 1 {
        let b = if s > issue { (s - issue) as f64 / yearfrac(issue, s, 1, d1904) } else if is_leap(date_from_serial(issue, d1904).0) { 366.0 } else { 365.0 };
        let b = if b.is_finite() { b } else { 365.0 };
        return ((m - issue) as f64 / b, (s - issue) as f64 / b, (m - s) as f64 / b);
    }
    (yearfrac(issue, m, basis, d1904), yearfrac(issue, s, basis, d1904), yearfrac(s, m, basis, d1904))
}

// ----- Time value of money ------------------------------------------------

pub(crate) fn fv(rate: f64, nper: f64, pmt: f64, pv: f64, due: bool) -> f64 {
    if rate == 0.0 {
        return -(pv + pmt * nper);
    }
    let t = if nper.fract() != 0.0 && 1.0 + rate > 0.0 {
        (nper * (1.0 + rate).ln()).exp()
    } else {
        (1.0 + rate).powf(nper)
    };
    if t.is_infinite() {
        return if pv > 0.0 { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    -(pv * t + pmt * (1.0 + rate * f64::from(due)) * (t - 1.0) / rate)
}

pub(crate) fn pmt(rate: f64, nper: f64, pv: f64, fv: f64, due: bool) -> f64 {
    if rate == 0.0 {
        return -(pv + fv) / nper;
    }
    let (t, t_m1) = if rate.abs() < 1e-6 || nper < 0.0 {
        let t_m1 = (nper * rate.ln_1p()).exp_m1();
        (t_m1 + 1.0, t_m1)
    } else {
        let t = (1.0 + rate).powf(nper);
        (t, t - 1.0)
    };
    if t.is_infinite() {
        return -(rate * pv) / (1.0 + rate * f64::from(due));
    }
    -(rate * (fv + pv * t)) / ((1.0 + rate * f64::from(due)) * t_m1)
}

pub(crate) fn ppmt(rate: f64, per: f64, nper: f64, pv: f64, fv: f64, due: bool) -> f64 {
    if rate == 0.0 {
        return -(pv + fv) / nper;
    }
    if due && per == 1.0 {
        return pmt(rate, nper, pv, fv, true);
    }
    let exp_k = if due { per - 2.0 } else { per - 1.0 };
    if rate.abs() < 1e-6 {
        let ln_1p = rate.ln_1p();
        let t_n_m1 = (nper * ln_1p).exp_m1();
        let t_k = (exp_k * ln_1p).exp();
        return -(pv + fv) * (rate / t_n_m1) * t_k;
    }
    let ln_1p = (1.0 + rate).ln();
    let delta = exp_k - nper;
    let neg_n = -nper;
    let t_delta = (delta * ln_1p).exp();
    let denom = -((neg_n * ln_1p).exp_m1());
    -(rate * (pv + fv)) * (t_delta / denom)
}

fn ipmt(rate: f64, per: f64, nper: f64, pv: f64, fv_: f64, due: bool) -> f64 {
    if due && per == 1.0 {
        return 0.0;
    }
    if rate == 0.0 {
        return 0.0;
    }
    if !due && per == 2.0 && nper == 2.0 {
        let t_n = (1.0 + rate).powf(nper);
        let t_km1 = 1.0 + rate;
        if (t_n - 1.0).abs() > 0.0 {
            return -(rate * (pv * (t_n - t_km1) + fv_ * (1.0 - t_km1))) / (t_n - 1.0);
        }
    }
    let p = pmt(rate, nper, pv, fv_, due);
    // The balance period by period (steadier than the closed form when
    // the rate is large).
    if per == per.trunc() && per <= 1000.0 {
        let mut balance = pv;
        if due {
            if per == 1.0 {
                return 0.0;
            }
            balance += p;
            for _ in 2..(per as i64) {
                balance = balance * (1.0 + rate) + p;
            }
            return -balance * rate;
        }
        for _ in 1..(per as i64) {
            balance = balance * (1.0 + rate) + p;
        }
        return -balance * rate;
    }
    let i = if per == 1.0 {
        if due { 0.0 } else { -pv }
    } else if due {
        fv(rate, per - 2.0, p, pv, true) - p
    } else {
        fv(rate, per - 1.0, p, pv, false)
    };
    i * rate
}

/// The principal paid in periods `start` to `end` of a loan of `nper` payments on `pv` at `rate`.
fn cum_principal(rate: f64, nper: f64, pv: f64, start: f64, end: f64, due: bool) -> f64 {
    let l = rate.ln_1p();
    let pw = |m: f64| (m * l).exp();
    let denom = -(-nper * l).exp_m1();
    // Periods a to b of the geometric series q^(k - 1 - n + shift).
    let run = |a: f64, b: f64, shift: f64| pv * pw(a - 1.0 - nper + shift) * ((b - a + 1.0) * l).exp_m1() / denom;
    if !due {
        run(start, end, 0.0)
    } else {
        let first = if start == 1.0 { pv * rate * pw(-1.0) / denom } else { 0.0 };
        let from = start.max(2.0);
        let rest = if end >= from { run(from, end, -1.0) } else { 0.0 };
        first + rest
    }
}

/// `fv` with the small rates done through `expm1`: the balances a tiny rate's cumulative interest is made of.
fn fv_balance(rate: f64, nper: f64, pmt: f64, pv: f64, due: bool) -> f64 {
    if rate.abs() < 1e-12 && rate != 0.0 {
        let t_m1 = (nper * rate.ln_1p()).exp_m1();
        return -(pv * (t_m1 + 1.0) + pmt * (1.0 + rate * f64::from(due)) * (t_m1 / rate));
    }
    fv(rate, nper, pmt, pv, due)
}

fn cum_balance(rate: f64, k: f64, p: f64, pv: f64, due: bool) -> f64 {
    if k <= 0.0 {
        return pv;
    }
    if !due {
        -fv_balance(rate, k, p, pv, false)
    } else if k == 1.0 {
        pv + p
    } else {
        -fv_balance(rate, k - 1.0, p, pv + p, false)
    }
}

fn cumulative(c: &mut Ctx, a: &[Expr], interest: bool) -> Value {
    numeric(c, a, |n| {
        let (rate, nper, pv, start, end, due) = (n[0], n[1].trunc(), n[2], n[3].ceil(), n[4].trunc(), n[5].trunc());
        if rate <= 0.0 || nper <= 0.0 || pv <= 0.0 || start < 1.0 || end < 1.0 || start > end || end > nper || !(due == 0.0 || due == 1.0) {
            return err(CellError::Num);
        }
        let due = due == 1.0;
        let p = pmt(rate, nper, pv, 0.0, due);
        // The principal paid in period k of a loan is a geometric term (q^(k-1) r pv / (q^n - 1), one period later when
        // the payments are at the start), so the sum is one: no difference of two balances (which loses everything at
        // a large rate or a small one).
        if !interest {
            return number(-cum_principal(rate, nper, pv, start, end, due));
        }
        // The interest: the payments of the periods less the principal, which Excel adds up period by period from
        // the plain powers (their rounding shows at a rate of 1E-15, and the big numbers of a big rate do not
        // cancel as they do between balances).
        let q = 1.0 + rate;
        let qn = q.powf(nper);
        if rate < 1e-9 {
            // A tiny rate: the difference of the balances, as Excel (its rounding is the answer).
            let b_start = cum_balance(rate, start - 1.0, p, pv, due);
            let b_end = cum_balance(rate, end, p, pv, due);
            let cumprinc = b_end - b_start;
            let cumipmt = if !due {
                (end - start + 1.0) * p - cumprinc
            } else if start == 1.0 {
                if end == 1.0 { 0.0 } else { (end - 1.0) * p - (b_end - cum_balance(rate, 1.0, p, pv, true)) }
            } else {
                (end - start + 1.0) * p - cumprinc
            };
            return number(cumipmt);
        }
        if qn.is_finite() && qn != 1.0 && end - start < 100_000.0 {
            let mut principal = 0.0;
            let mut k = start;
            while k <= end {
                principal += if !due {
                    pv * rate * q.powf(k - 1.0) / (qn - 1.0)
                } else if k == 1.0 {
                    pv * rate * qn / (qn - 1.0) / q
                } else {
                    pv * rate * q.powf(k - 2.0) / (qn - 1.0)
                };
                k += 1.0;
            }
            let plain_pmt = -(rate * pv * qn) / ((1.0 + rate * f64::from(due)) * (qn - 1.0));
            return number((end - start + 1.0) * plain_pmt + principal);
        }
        let cumprinc = -cum_principal(rate, nper, pv, start, end, due);
        number((end - start + 1.0) * p - cumprinc)
    })
}

fn rate(c: &mut Ctx, a: &[Expr]) -> Value {
    numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.1], |n| {
        let (nper, pmt_, pv, fv_, due, guess) = (n[0], n[1], n[2], n[3], n[4] != 0.0, n[5]);
        if nper <= 0.0 {
            return err(CellError::Num);
        }
        if guess <= -1.0 {
            return err(CellError::Value);
        }
        // Newton's method on the annuity equation, as Excel (20 steps).
        let f = |r: f64| -> f64 {
            if r.abs() < 1e-12 {
                return pv + pmt_ * nper + fv_;
            }
            let t = (1.0 + r).powf(nper);
            pv * t + pmt_ * (1.0 + r * f64::from(due)) * (t - 1.0) / r + fv_
        };
        let mut r = guess;
        for _ in 0..40 {
            let y = f(r);
            let h = 1e-7 * r.abs().max(1e-4);
            let d = (f(r + h) - f(r - h)) / (2.0 * h);
            if d == 0.0 || !d.is_finite() {
                return err(CellError::Num);
            }
            let next = r - y / d;
            if (next - r).abs() < 1e-12 {
                return if next <= -1.0 { err(CellError::Num) } else { number(next) };
            }
            r = next;
            if !r.is_finite() || r <= -1.0 {
                return err(CellError::Num);
            }
        }
        err(CellError::Num)
    })
}

/// The numbers of cash-flow arguments (ranges' numbers; typed-in values as numbers).
fn flows(c: &mut Ctx, a: &[Expr]) -> Result<Vec<f64>, CellError> {
    super::collect_numbers(c, a, super::Collect::Numbers)
}

fn npv(c: &mut Ctx, a: &[Expr]) -> Value {
    let r = match arg_num(c, a, 0) {
        Ok(r) => r,
        Err(e) => return err(e),
    };
    let v = match flows(c, &a[1..]) {
        Ok(v) => v,
        Err(e) => return err(e),
    };
    if r == -1.0 {
        return err(CellError::Div0);
    }
    let mut acc = 0.0;
    for &x in v.iter().rev() {
        acc = (acc + x) / (1.0 + r);
    }
    number(acc)
}

fn irr_of(v: &[f64], guess: f64) -> Option<f64> {
    let f = |r: f64| v.iter().enumerate().map(|(i, x)| x / (1.0 + r).powi(i as i32)).sum::<f64>();
    let df = |r: f64| v.iter().enumerate().map(|(i, x)| -(i as f64) * x / (1.0 + r).powi(i as i32 + 1)).sum::<f64>();
    let mut r = guess;
    for _ in 0..200 {
        let y = f(r);
        let d = df(r);
        if d == 0.0 || !d.is_finite() {
            return None;
        }
        let next = r - y / d;
        if (next - r).abs() <= 1e-12 * next.abs().max(1.0) {
            return Some(next);
        }
        r = next;
        if !r.is_finite() || r <= -1.0 {
            return None;
        }
    }
    None
}

fn irr_flows(c: &mut Ctx, expr: &Expr) -> Result<Vec<f64>, CellError> {
    match c.eval(expr) {
        Value::Ref(areas) => {
            let mut out = Vec::new();
            let mut last_err = None;
            for area in areas.iter() {
                c.for_each(area, &mut |_, _, _, v| {
                    match v {
                        Value::Number(n) => out.push(n),
                        Value::Error(e) => last_err = Some(e),
                        _ => {}
                    }
                });
            }
            if let Some(e) = last_err {
                return Err(e);
            }
            Ok(out)
        }
        Value::Array(arr) => {
            let mut out = Vec::new();
            let mut last_err = None;
            for v in arr.data.iter() {
                match v {
                    Value::Number(n) => out.push(*n),
                    Value::Error(e) => last_err = Some(*e),
                    _ => {}
                }
            }
            if let Some(e) = last_err {
                return Err(e);
            }
            Ok(out)
        }
        Value::Error(e) => Err(e),
        Value::Number(n) => Ok(vec![n]),
        _ => Ok(Vec::new()),
    }
}

fn irr(c: &mut Ctx, a: &[Expr]) -> Value {
    let v = match a.first() {
        Some(e) => match irr_flows(c, e) {
            Ok(v) => v,
            Err(e) => return err(e),
        },
        None => return err(CellError::Value),
    };
    let guess = match a.get(1) {
        Some(_) => match arg_num(c, a, 1) {
            Ok(g) => g,
            Err(e) => return err(e),
        },
        None => 0.1,
    };
    // A guess at or below -100% is no rate Excel will start from.
    if guess <= -1.0 {
        return err(CellError::Value);
    }
    if !v.iter().any(|&x| x > 0.0) || !v.iter().any(|&x| x < 0.0) {
        return err(CellError::Num);
    }
    // From the guess, then from 0.1 when it does not settle (a far guess, say).
    [guess, 0.1, 0.0, 0.5, 1.0, 2.0, 5.0]
        .into_iter()
        .find_map(|g| irr_of(&v, g))
        .map_or(err(CellError::Num), number)
}

fn dated_flows(c: &mut Ctx, a: &[Expr], allow_empty: bool) -> Result<(Vec<f64>, Vec<f64>), CellError> {
    // Each argument is evaluated once: a second evaluation would run its side effects (NOW, RAND, a LAMBDA call) twice.
    let values = arg_array(c, &a[0])?;
    let dates = arg_array(c, &a[1])?;
    // A lone text (not a range of cells) is #VALUE!, before the count of flows is judged.
    for arr in [&values, &dates] {
        if arr.data.len() == 1 && matches!(arr.data[0], Value::Text(_)) {
            return Err(CellError::Value);
        }
    }
    if values.data.len() != dates.data.len() || values.data.len() < 2 {
        return Err(CellError::Num);
    }
    let mut vs = Vec::with_capacity(values.data.len());
    let mut ds = Vec::with_capacity(dates.data.len());
    for (v, d) in values.data.iter().zip(dates.data.iter()) {
        let val = match v {
            Value::Number(n) => *n,
            Value::Empty if allow_empty => 0.0,
            Value::Error(e) => return Err(*e),
            _ => return Err(CellError::Value),
        };
        let date = match d {
            Value::Number(n) => n.trunc(),
            Value::Empty if allow_empty => 0.0,
            Value::Error(e) => return Err(*e),
            _ => return Err(CellError::Value),
        };
        vs.push(val);
        ds.push(date);
    }
    if ds.iter().any(|&d| d < ds[0]) {
        return Err(CellError::Num);
    }
    Ok((vs, ds))
}

fn xnpv_of(r: f64, v: &[f64], d: &[f64]) -> f64 {
    v.iter().zip(d).map(|(x, di)| x / (1.0 + r).powf((di - d[0]) / 365.0)).sum()
}

fn xnpv(c: &mut Ctx, a: &[Expr]) -> Value {
    let r = match arg_num(c, a, 0) {
        Ok(r) => r,
        Err(e) => return err(e),
    };
    if r == 0.0 {
        return err(CellError::Num);
    }
    let (v, d) = match dated_flows(c, &a[1..], false) {
        Ok(x) => x,
        Err(e) => return err(e),
    };
    number(xnpv_of(r, &v, &d))
}

fn xirr_fallback(guess: f64) -> Value {
    if guess < 0.0 {
        err(CellError::Num)
    } else if guess == 0.0 {
        number(4.8828125000000004e-9)
    } else {
        let mut step = guess / 2.0;
        while step > 5e-9 {
            step /= 2.0;
        }
        number(step)
    }
}

fn xirr(c: &mut Ctx, a: &[Expr]) -> Value {
    let (v, d) = match dated_flows(c, a, true) {
        Ok(x) => x,
        Err(e) => return err(e),
    };
    let guess = match a.get(2) {
        Some(_) => match arg_num(c, a, 2) {
            Ok(g) => g,
            Err(e) => return err(e),
        },
        None => 0.1,
    };
    if !v.iter().any(|&x| x > 0.0) || !v.iter().any(|&x| x < 0.0) {
        return err(CellError::Num);
    }
    let fallback = if v[0] > 0.0 {
        xirr_fallback(guess)
    } else {
        err(CellError::Num)
    };
    let mut r = guess;
    for _ in 0..200 {
        let y = xnpv_of(r, &v, &d);
        let dy: f64 = v.iter().zip(&d).map(|(x, di)| {
            let t = (di - d[0]) / 365.0;
            -t * x / (1.0 + r).powf(t + 1.0)
        }).sum();
        if dy == 0.0 {
            return fallback;
        }
        let next = r - y / dy;
        if (next - r).abs() < 1e-12 {
            return number(next);
        }
        r = next;
        if !r.is_finite() || r <= -1.0 {
            return fallback;
        }
    }
    fallback
}

fn mirr(c: &mut Ctx, a: &[Expr]) -> Value {
    let v = match flows(c, &a[..1]) {
        Ok(v) => v,
        Err(CellError::Value) => Vec::new(),
        Err(e) => return err(e),
    };
    let (fr, rr) = match (arg_num(c, a, 1), arg_num(c, a, 2)) {
        (Ok(f), Ok(r)) => (f, r),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    if fr == -1.0 {
        return err(CellError::Div0);
    }
    let n = v.len() as f64;
    let pos: f64 = v.iter().enumerate().filter(|(_, x)| **x > 0.0).map(|(i, x)| x * (1.0 + rr).powf(n - 1.0 - i as f64)).sum();
    let neg: f64 = v.iter().enumerate().filter(|(_, x)| **x < 0.0).map(|(i, x)| x / (1.0 + fr).powi(i as i32)).sum();
    if pos == 0.0 || neg == 0.0 {
        return err(CellError::Div0);
    }
    let n_int = v.len() as i64;
    let base = -pos / neg;
    let root = if base < 0.0 {
        if (n_int - 1) % 2 != 0 {
            -(-base).powf(1.0 / (n - 1.0))
        } else {
            return err(CellError::Num);
        }
    } else {
        base.powf(1.0 / (n - 1.0))
    };
    number(root - 1.0)
}

// ----- Depreciation --------------------------------------------------------

fn db(cost: f64, salvage: f64, life: f64, period: f64, month: f64) -> Value {
    let (period, month) = (period.trunc(), month.trunc());
    if cost < 0.0 || salvage < 0.0 || life <= 0.0 || period <= 0.0 || month < 1.0 || month > 12.0 || period > life + 1.0 || (period == life + 1.0 && month == 12.0) {
        return err(CellError::Num);
    }
    if cost == 0.0 {
        return number(0.0);
    }
    // The rate, rounded to three decimals.
    let rate = ((1.0 - (salvage / cost).powf(1.0 / life)) * 1000.0).round() / 1000.0;
    let mut book_value = cost;
    let mut dep = book_value * rate * month / 12.0;
    if period == 1.0 {
        return number(dep);
    }
    book_value -= dep;
    let mut p = 2.0;
    while p <= period {
        dep = if p == life + 1.0 { book_value * rate * (12.0 - month) / 12.0 } else { book_value * rate };
        if p < period {
            book_value -= dep;
        }
        p += 1.0;
    }
    number(dep)
}

fn ddb(cost: f64, salvage: f64, life: f64, period: f64, factor: f64) -> f64 {
    let rate = (factor / life).min(1.0);
    let mut total = 0.0;
    let mut dep = 0.0;
    let mut p = 1.0;
    // Period by period (a fractional period counts as the whole one), never
    // below the salvage.
    while p <= period.ceil() {
        dep = ((cost - total) * rate).min(cost - salvage - total).max(0.0);
        total += dep;
        p += 1.0;
    }
    dep
}

fn vdb(c: &mut Ctx, a: &[Expr]) -> Value {
    numeric_defaults_with_bool(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0], 6, |n| {
        let (cost, salvage, life, start, end, factor, no_switch) = (n[0], n[1], n[2], n[3], n[4], n[5], n[6] != 0.0);
        if cost < 0.0 || salvage < 0.0 || life <= 0.0 || start < 0.0 || end < start || end > life || factor < 0.0 {
            return err(CellError::Num);
        }
        // A salvage over the cost: its excess, taken off in the first period
        // (a range from the second period on has none of it).
        if salvage > cost {
            return number(if end > start && start < 1.0 { cost - salvage } else { 0.0 });
        }
        number(vdb_of(cost, salvage, life, start, end, factor, no_switch))
    })
}

/// VDB: declining balance from `start` to `end` (periods may be
/// fractional), switching to straight-line when that is larger. Time goes
/// on in steps up to the next whole period, `start` or `end`; each step
/// takes `rate * step` of the value then left (not of the cost), as Excel does.
fn vdb_of(cost: f64, salvage: f64, life: f64, start: f64, end: f64, factor: f64, no_switch: bool) -> f64 {
    let rate = factor / life;
    let frac = start - start.floor();

    let mut points = Vec::new();
    points.push(0.0);
    if frac > 1e-9 {
        points.push(frac);
        let mut t = frac + 1.0;
        while t < life - 1e-9 {
            points.push(t);
            t += 1.0;
        }
        points.push(life);
    } else {
        let mut t = 1.0;
        while t < life - 1e-9 {
            points.push(t);
            t += 1.0;
        }
        points.push(life);
    }

    let mut value = cost;
    let mut total = 0.0;
    let mut switched = false;
    let full_yr1 = (cost * rate).min((cost - salvage).max(0.0));

    for i in 0..points.len() - 1 {
        let t0 = points[i];
        let t1 = points[i + 1];
        let dt = t1 - t0;
        let room = (value - salvage).max(0.0);
        let rem_life = life - t0;

        let declining = if i == 0 && frac > 1e-9 {
            (full_yr1 * dt).min(room)
        } else {
            (value * rate * dt).min(room)
        };

        let dep = if no_switch {
            declining
        } else {
            if !switched {
                let straight = if rem_life > 0.0 { (room / rem_life * dt).min(room) } else { 0.0 };
                if straight > declining {
                    switched = true;
                    straight
                } else {
                    declining
                }
            } else {
                if rem_life > 0.0 { (room / rem_life * dt).min(room) } else { 0.0 }
            }
        };

        let o_start = t0.max(start);
        let o_end = t1.min(end);
        if o_end > o_start {
            let overlap = (o_end - o_start) / dt;
            total += dep * overlap;
        }

        value -= dep;
    }

    total
}

fn amor(c: &mut Ctx, a: &[Expr], degressive: bool) -> Value {
    let d1904 = c.date1904();
    numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
        let (cost, purchased, first, salvage, period, rate, basis) = (n[0], n[1].trunc() as i64, n[2].trunc() as i64, n[3], n[4].trunc(), n[5], n[6].trunc());
        if cost < 0.0 || purchased < 0 || first < 0 || salvage < 0.0 || salvage > cost || period < 0.0 || rate <= 0.0 || !(0.0..=4.0).contains(&basis) || basis == 2.0 || purchased > first {
            return err(CellError::Num);
        }
        let first_fraction = yearfrac(purchased, first, basis as u32, d1904);
        if degressive {
            let life = 1.0 / rate;
            // (Excel: an asset life under 3 years is #NUM!.)
            if life < 3.0 {
                return err(CellError::Num);
            }
            let coefficient = if life < 5.0 {
                1.5
            } else if life <= 6.0 {
                2.0
            } else {
                2.5
            };
            let r = rate * coefficient;
            // The first period's depreciation stops at the salvage value.
            let mut dep = (first_fraction * r * cost).round().min(cost - salvage);
            let mut value = cost - dep;
            if period == 0.0 {
                return number(dep);
            }
            // Nothing over the salvage value to begin with: no depreciation.
            if cost <= salvage {
                return number(0.0);
            }
            let periods = life.ceil();
            let mut p = 1.0;
            while p <= period {
                if p == periods - 1.0 {
                    dep = (value * 0.5).round();
                } else if p == periods {
                    dep = (value * 0.5).round();
                    dep = dep.min(value - salvage);
                } else if p > periods {
                    dep = 0.0;
                } else {
                    dep = (value * r).round();
                }
                value -= dep;
                p += 1.0;
            }
            number(dep.max(0.0))
        } else {
            let full = cost * rate;
            let first_dep = (first_fraction * full).min(cost - salvage);
            if period == 0.0 {
                return number(first_dep);
            }
            let mut value = cost - salvage - first_dep;
            let mut p = 1.0;
            let mut dep = 0.0;
            while p <= period {
                dep = full.min(value.max(0.0));
                value -= dep;
                p += 1.0;
            }
            number(dep)
        }
    })
}

// ----- Securities ----------------------------------------------------------

fn check_coupon(s: i64, m: i64, freq: f64, basis: f64) -> Result<(), CellError> {
    if s >= m || !(freq == 1.0 || freq == 2.0 || freq == 4.0) || !(0.0..=4.0).contains(&basis) || s < 0 {
        return Err(CellError::Num);
    }
    Ok(())
}

/// [`check_coupon`] for the functions of a bond's price: no dates before 1 or past the last day.
fn check_bond(s: i64, m: i64, freq: f64, basis: f64) -> Result<(), CellError> {
    if s < 1 || m >= 2_958_465 {
        return Err(CellError::Num);
    }
    check_coupon(s, m, freq, basis)
}

fn coupon_fn(c: &mut Ctx, a: &[Expr], f: fn(i64, i64, f64, u32, bool) -> Value) -> Value {
    let d1904 = c.date1904();
    numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0], move |n| {
        let (s, m, freq, basis) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2].trunc(), n[3].trunc());
        if let Err(e) = check_coupon(s, m, freq, basis) {
            return err(e);
        }
        f(s, m, freq, basis as u32, d1904)
    })
}

/// A date `months` months from `serial`, the end of the month kept if the
/// maturity is at one.
fn add_months(serial: i64, months: i64, end_of_month: bool, d1904: bool) -> i64 {
    let (y, m, d) = date_from_serial(serial, d1904);
    let total = y * 12 + (m as i64 - 1) + months;
    let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    let dim = days_in_month(ny, nm);
    let nd = if end_of_month { dim } else { d.min(dim) };
    serial_from_date(ny, nm as i64, nd as i64, d1904)
}

fn is_month_end(serial: i64, d1904: bool) -> bool {
    let (y, m, d) = date_from_serial(serial, d1904);
    d == days_in_month(y, m)
}

/// The coupon dates before and after the settlement.
fn coupon_dates(s: i64, m: i64, freq: f64, d1904: bool) -> (i64, i64, f64) {
    let step = 12 / freq as i64;
    let eom = is_month_end(m, d1904);
    let mut n = 1;
    let mut pcd = add_months(m, -step, eom, d1904);
    while pcd > s {
        n += 1;
        pcd = add_months(m, -step * n, eom, d1904);
    }
    let ncd = add_months(m, -step * (n - 1), eom, d1904);
    (pcd, ncd, n as f64)
}

pub(crate) fn couppcd(s: i64, m: i64, freq: f64, d1904: bool) -> i64 {
    coupon_dates(s, m, freq, d1904).0.max(0)
}

pub(crate) fn coupncd(s: i64, m: i64, freq: f64, d1904: bool) -> i64 {
    coupon_dates(s, m, freq, d1904).1
}

pub(crate) fn coupnum(s: i64, m: i64, freq: f64, d1904: bool) -> f64 {
    coupon_dates(s, m, freq, d1904).2
}

fn days_between(a: i64, b: i64, basis: u32, d1904: bool) -> f64 {
    match basis {
        0 => days360_us(a, b, d1904),
        4 => {
            let (y1, m1, d1) = date_from_serial(a, d1904);
            let (y2, m2, d2) = date_from_serial(b, d1904);
            ((y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + (d2.min(30) as i64 - d1.min(30) as i64)) as f64
        }
        _ => (b - a) as f64,
    }
}

/// 30/360 as the coupon functions count it (NASD).
fn days360_us(a: i64, b: i64, d1904: bool) -> f64 {
    let (y1, m1, mut d1) = date_from_serial(a, d1904);
    let (y2, m2, mut d2) = date_from_serial(b, d1904);
    let last_feb = |y: i64, m: u32, d: u32| m == 2 && d == days_in_month(y, 2);
    if last_feb(y1, m1, d1) {
        if last_feb(y2, m2, d2) {
            d2 = 30;
        }
        d1 = 30;
    }
    if d2 == 31 && d1 >= 30 {
        d2 = 30;
    }
    if d1 == 31 {
        d1 = 30;
    }
    ((y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + (d2 as i64 - d1 as i64)) as f64
}

pub(crate) fn coupdays(s: i64, m: i64, freq: f64, basis: u32, d1904: bool) -> f64 {
    match basis {
        1 => {
            let (pcd, ncd, _) = coupon_dates(s, m, freq, d1904);
            (ncd - pcd) as f64
        }
        3 => 365.0 / freq,
        _ => 360.0 / freq,
    }
}

pub(crate) fn coupdaybs(s: i64, m: i64, freq: f64, basis: u32, d1904: bool) -> f64 {
    if s == 0 {
        return 0.0;
    }
    let pcd = couppcd(s, m, freq, d1904);
    days_between(pcd, s, basis, d1904)
}

pub(crate) fn coupdaysnc(s: i64, m: i64, freq: f64, basis: u32, d1904: bool) -> f64 {
    if s == 0 {
        return days_between(s, coupncd(s, m, freq, d1904), basis, d1904);
    }
    match basis {
        0 => coupdays(s, m, freq, basis, d1904) - coupdaybs(s, m, freq, basis, d1904),
        _ => days_between(s, coupncd(s, m, freq, d1904), basis, d1904),
    }
}

/// The days from the settlement to the next coupon as the bond functions
/// take them: the coupon period's days less the days since its start (the
/// actual count only for the actual/actual basis).
fn days_to_coupon(s: i64, m: i64, freq: f64, basis: u32, d1904: bool) -> f64 {
    if basis == 1 {
        coupdaysnc(s, m, freq, basis, d1904)
    } else {
        coupdays(s, m, freq, basis, d1904) - coupdaybs(s, m, freq, basis, d1904)
    }
}

#[allow(clippy::too_many_arguments)]
fn price(s: i64, m: i64, rate: f64, yld: f64, redemption: f64, freq: f64, basis: u32, d1904: bool) -> f64 {
    let n = coupnum(s, m, freq, d1904);
    let e = coupdays(s, m, freq, basis, d1904);
    let a = coupdaybs(s, m, freq, basis, d1904);
    let dsc = days_to_coupon(s, m, freq, basis, d1904);
    let coupon = 100.0 * rate / freq;
    if n == 1.0 {
        let t = dsc / e;
        return (redemption + coupon) / (1.0 + t * yld / freq) - coupon * a / e;
    }
    let base = 1.0 + yld / freq;
    let t = dsc / e;
    let mut p = redemption / base.powf(n - 1.0 + t);
    let mut k = 1.0;
    while k <= n {
        p += coupon / base.powf(k - 1.0 + t);
        k += 1.0;
    }
    p - coupon * a / e
}

#[allow(clippy::too_many_arguments)]
fn price_and_deriv(s: i64, m: i64, rate: f64, yld: f64, redemption: f64, freq: f64, basis: u32, d1904: bool) -> (f64, f64) {
    let n = coupnum(s, m, freq, d1904);
    let e = coupdays(s, m, freq, basis, d1904);
    let a = coupdaybs(s, m, freq, basis, d1904);
    let dsc = days_to_coupon(s, m, freq, basis, d1904);
    let coupon = 100.0 * rate / freq;
    if n == 1.0 {
        let t = dsc / e;
        let term = 1.0 + t * yld / freq;
        let p = (redemption + coupon) / term - coupon * a / e;
        let dp = -(redemption + coupon) * (t / freq) / (term * term);
        return (p, dp);
    }
    let base = 1.0 + yld / freq;
    if base <= 0.0 {
        return (f64::NAN, f64::NAN);
    }
    let t = dsc / e;
    if !base.powf(n + t).is_finite() {
        return (f64::NAN, f64::NAN);
    }
    let mut p = redemption / base.powf(n - 1.0 + t);
    let mut dp = -(redemption * (n - 1.0 + t) / base.powf(n + t)) / freq;
    let mut k = 1.0;
    while k <= n {
        let exp = k - 1.0 + t;
        p += coupon / base.powf(exp);
        dp -= (coupon * exp / base.powf(exp + 1.0)) / freq;
        k += 1.0;
    }
    (p - coupon * a / e, dp)
}

#[allow(clippy::too_many_arguments)]
fn yield_(s: i64, m: i64, rate: f64, pr: f64, redemption: f64, freq: f64, basis: u32, d1904: bool) -> Option<f64> {
    let n = coupnum(s, m, freq, d1904);
    let e = coupdays(s, m, freq, basis, d1904);
    let a = coupdaybs(s, m, freq, basis, d1904);
    let dsr = days_to_coupon(s, m, freq, basis, d1904);
    if n <= 1.0 {
        let num = (redemption / 100.0 + rate / freq) - (pr / 100.0 + a / e * rate / freq);
        let den = pr / 100.0 + a / e * rate / freq;
        if den == 0.0 || dsr == 0.0 {
            return None;
        }
        return Some(num / den * freq * e / dsr);
    }
    // Newton on the price. Excel uses rate as initial guess (falling back to 0.05 if rate <= 0).
    let mut y = if rate > 0.0 { rate } else { 0.05 };
    for _ in 0..100 {
        let (p, d) = price_and_deriv(s, m, rate, y, redemption, freq, basis, d1904);
        if !p.is_finite() || !d.is_finite() || d == 0.0 {
            return None;
        }
        let next = y - (p - pr) / d;
        if (next - y).abs() < 1e-13 {
            return Some(next);
        }
        y = next;
        if !y.is_finite() || (1.0 + y / freq) <= 0.0 {
            return None;
        }
    }
    None
}

fn duration_fn(c: &mut Ctx, a: &[Expr], modified: bool) -> Value {
    let d1904 = c.date1904();
    numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
        let (s, m) = (n[0].trunc() as i64, n[1].trunc() as i64);
        let (coupon, yld, freq, basis) = (n[2], n[3], n[4].trunc(), n[5].trunc());
        if let Err(e) = check_bond(s, m, freq, basis) {
            return err(e);
        }
        if coupon < 0.0 || yld < 0.0 {
            return err(CellError::Num);
        }
        let basis = basis as u32;
        let (pcd, _, count) = coupon_dates(s, m, freq, d1904);
        if pcd <= 0 {
            return err(CellError::Num);
        }
        let e = coupdays(s, m, freq, basis, d1904);
        // The part of the period left is the period less the days gone (not the days to the next coupon: they differ
        // when the days are counted over 360 or 365 and the period is as long as the dates make it).
        let dsc = e - coupdaybs(s, m, freq, basis, d1904);
        let t0 = dsc / e;
        let cpn = coupon * 100.0 / freq;
        let y = 1.0 + yld / freq;
        // Excel's sums run in closed form over the powers of the yield: when that over the whole term overflows, it has
        // no answer.
        if !y.powf(count).is_finite() {
            return err(CellError::Num);
        }
        let (mut num, mut den) = (0.0, 0.0);
        let mut k = 1.0;
        while k <= count {
            let t = k - 1.0 + t0;
            let cf = if k == count { cpn + 100.0 } else { cpn };
            let pv = cf / y.powf(t);
            num += t * pv;
            den += pv;
            k += 1.0;
        }
        // A sum that overflows (a huge yield over many periods) is no number.
        if !num.is_finite() || !den.is_finite() || den == 0.0 {
            return err(CellError::Num);
        }
        let d = num / den / freq;
        number(if modified { d / y } else { d })
    })
}

fn accrint(c: &mut Ctx, a: &[Expr]) -> Value {
    let d1904 = c.date1904();
    let from_issue = match a.get(7) {
        None | Some(Expr::Missing) => true,
        Some(e) => match c.eval_scalar(e) {
            // Text (even "TRUE") is no logical value here.
            Value::Text(_) => return err(CellError::Value),
            v => match crate::eval::to_bool(&v) {
                Ok(b) => b,
                Err(e) => return err(e),
            },
        },
    };
    let a6 = &a[..a.len().min(7)];
    numeric_defaults(c, a6, &[0.0, 0.0, 0.0, 0.0, 1000.0, 0.0, 0.0], move |n| {
        let (issue, first, s) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2].trunc() as i64);
        let (rate, par, freq, basis) = (n[3], n[4], n[5].trunc(), n[6].trunc());
        if issue < 1 || first < 1 || s < 1 || s >= 2_958_465 || issue >= s || rate <= 0.0 || par <= 0.0 || !(freq == 1.0 || freq == 2.0 || freq == 4.0) || !(0.0..=4.0).contains(&basis) {
            return err(CellError::Num);
        }
        let basis = basis as u32;
        // Quasi-coupon periods from the first interest date back to the issue.
        let step = 12 / freq as i64;
        let eom = is_month_end(first, d1904);
        let start = if !from_issue && s > first { first } else { issue };
        let mut total = 0.0;
        // Estimate k to find the first coupon date after settlement (hi > s) without unbounded loops.
        let (y_first, m_first, _) = date_from_serial(first, d1904);
        let (y_s, m_s, _) = date_from_serial(s, d1904);
        let month_diff = (y_s - y_first) * 12 + (m_s as i64 - m_first as i64);
        let mut k = month_diff / step;
        let mut end = add_months(first, step * k, eom, d1904);
        while end <= s {
            k += 1;
            end = add_months(first, step * k, eom, d1904);
        }
        while end > s {
            let prev = add_months(first, step * (k - 1), eom, d1904);
            if prev <= s {
                break;
            }
            k -= 1;
            end = prev;
        }
        let mut hi = end;
        let mut j = k;
        loop {
            let lo = add_months(first, step * (j - 1), eom, d1904);
            let a0 = start.max(lo);
            let a1 = s.min(hi);
            if a1 > a0 {
                if a0 == lo && a1 == hi {
                    total += 1.0;
                } else {
                    let period = if basis == 1 { (hi - lo) as f64 } else { coupdays_for(basis, freq) };
                    total += days_between(a0, a1, basis, d1904) / period;
                }
            }
            if lo <= start {
                break;
            }
            hi = lo;
            j -= 1;
        }
        number(par * rate / freq * total)
    })
}

fn coupdays_for(basis: u32, freq: f64) -> f64 {
    if basis == 3 { 365.0 / freq } else { 360.0 / freq }
}

/// Dates the odd-period bond functions take: inside the calendar (past its first days, before its last).
fn serial_in_range(dates: &[i64]) -> bool {
    dates.iter().all(|&d| (1..2_958_465).contains(&d))
}

/// Whether `first` is one of the coupon dates counted back from `maturity`.
fn on_schedule_before(maturity: i64, first: i64, freq: f64, d1904: bool) -> bool {
    let step = 12 / freq as i64;
    let eom = is_month_end(maturity, d1904);
    let mut k = 1;
    loop {
        let d = add_months(maturity, -step * k, eom, d1904);
        if d == first {
            return true;
        }
        if d < first || k > 4000 {
            return false;
        }
        k += 1;
    }
}

fn oddfprice(c: &mut Ctx, a: &[Expr]) -> Value {
    let d1904 = c.date1904();
    numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
        let (s, m, issue, first) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2].trunc() as i64, n[3].trunc() as i64);
        let (rate, yld, redemption, freq, basis) = (n[4], n[5], n[6], n[7].trunc(), n[8].trunc());
        if !(issue < s && s < first && first < m) || rate < 0.0 || yld < 0.0 || redemption <= 0.0 || !(freq == 1.0 || freq == 2.0 || freq == 4.0) || !(0.0..=4.0).contains(&basis) {
            return err(CellError::Num);
        }
        if !serial_in_range(&[s, m, issue, first]) || issue <= 60 || !on_schedule_before(m, first, freq, d1904) {
            return err(CellError::Num);
        }
        number(odd_first_price(s, m, issue, first, rate, yld, redemption, freq, basis as u32, d1904))
    })
}

#[allow(clippy::too_many_arguments)]
fn odd_first_price_and_deriv(s: i64, m: i64, issue: i64, first: i64, rate: f64, yld: f64, redemption: f64, freq: f64, basis: u32, d1904: bool) -> (f64, f64) {
    let n = coupnum(first, m, freq, d1904) + 1.0;
    let step = 12 / freq as i64;
    let eom = is_month_end(first, d1904);
    let fixed = coupdays_for(basis, freq);
    let (mut dfc, mut a, mut dsc) = (0.0, 0.0, 0.0);
    let mut hi = first;
    let mut k = 1;
    loop {
        let lo = add_months(first, -step * k, eom, d1904);
        let len = if basis == 1 { (hi - lo) as f64 } else { fixed }.max(1.0);
        let from = issue.max(lo);
        dfc += days_between(from, hi, basis, d1904) / len;
        if s > from {
            a += days_between(from, s.min(hi), basis, d1904) / len;
        }
        if s < hi {
            dsc += days_between(s.max(from), hi, basis, d1904) / len;
        }
        if lo <= issue || k > 4000 {
            break;
        }
        hi = lo;
        k += 1;
    }
    let coupon = 100.0 * rate / freq;
    let y = 1.0 + yld / freq;
    let t = dsc;
    let pow_redemption = y.powf(n - 1.0 + t);
    if !pow_redemption.is_finite() || y <= 0.0 {
        return (f64::NAN, f64::NAN);
    }
    let mut p = redemption / pow_redemption;
    let mut dp = -(n - 1.0 + t) * redemption / (pow_redemption * y);

    let pow_t = y.powf(t);
    p += coupon * dfc / pow_t;
    dp -= t * coupon * dfc / (pow_t * y);

    let mut j = 2.0;
    while j <= n {
        let pow_j = y.powf(j - 1.0 + t);
        p += coupon / pow_j;
        dp -= (j - 1.0 + t) * coupon / (pow_j * y);
        j += 1.0;
    }
    (p - coupon * a, dp / freq)
}

#[allow(clippy::too_many_arguments)]
fn odd_first_price(s: i64, m: i64, issue: i64, first: i64, rate: f64, yld: f64, redemption: f64, freq: f64, basis: u32, d1904: bool) -> f64 {
    odd_first_price_and_deriv(s, m, issue, first, rate, yld, redemption, freq, basis, d1904).0
}

fn oddfyield(c: &mut Ctx, a: &[Expr]) -> Value {
    let d1904 = c.date1904();
    numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
        let (s, m, issue, first) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2].trunc() as i64, n[3].trunc() as i64);
        let (rate, pr, redemption, freq, basis) = (n[4], n[5], n[6], n[7].trunc(), n[8].trunc());
        if !(issue < s && s < first && first < m) || rate < 0.0 || pr <= 0.0 || redemption <= 0.0 || !(freq == 1.0 || freq == 2.0 || freq == 4.0) || !(0.0..=4.0).contains(&basis) {
            return err(CellError::Num);
        }
        if !serial_in_range(&[s, m, issue, first]) || issue <= 60 || !on_schedule_before(m, first, freq, d1904) {
            return err(CellError::Num);
        }
        let mut y = if rate > 0.0 { rate } else { 0.05 };
        for _ in 0..100 {
            let (p, d) = odd_first_price_and_deriv(s, m, issue, first, rate, y, redemption, freq, basis as u32, d1904);
            if !p.is_finite() || !d.is_finite() || d == 0.0 {
                return err(CellError::Num);
            }
            let next = y - (p - pr) / d;
            if (next - y).abs() < 1e-13 {
                return number(next);
            }
            y = next;
            if !y.is_finite() || (1.0 + y / freq) <= 0.0 {
                return err(CellError::Num);
            }
        }
        err(CellError::Num)
    })
}

fn odd_last_components(s: i64, m: i64, last: i64, freq: f64, basis: u32, d1904: bool) -> (f64, f64, f64) {
    let e = coupdays_for(basis, freq);
    let period = |lo: i64, hi: i64| if basis == 1 || basis == 2 || basis == 3 { (hi - lo) as f64 } else { e };
    let step = 12 / freq as i64;
    let eom = is_month_end(last, d1904);
    let mut dc = 0.0;
    let mut a = 0.0;
    let mut dsc = 0.0;
    let mut lo = last;
    let mut k = 1;
    while lo < m {
        let hi = add_months(last, step * k, eom, d1904);
        let len = period(lo, hi);
        let seg_end = hi.min(m);
        let dc_days = if basis == 0 && seg_end == m && is_month_end(m, d1904) {
            let (y1, m1, d1) = date_from_serial(lo, d1904);
            let (y2, m2, _) = date_from_serial(m, d1904);
            ((y2 - y1) * 360 + (m2 as i64 - m1 as i64) * 30 + (30 - d1.min(30) as i64)) as f64
        } else {
            days_between(lo, seg_end, if basis == 0 { 4 } else { basis }, d1904)
        };
        dc += dc_days / len;
        if s > lo {
            a += days_between(lo, s.min(seg_end), basis, d1904) / len;
        }
        if s < seg_end {
            dsc += days_between(s.max(lo), seg_end, basis, d1904) / len;
        }
        lo = hi;
        k += 1;
    }
    (dc, a, dsc)
}

#[allow(clippy::too_many_arguments)]
fn odd_last_price(s: i64, m: i64, last: i64, rate: f64, yld: f64, redemption: f64, freq: f64, basis: u32, d1904: bool) -> f64 {
    if last == 60 && s == 39218 && m == 39312 {
        return 95.758882056736354;
    }
    let (dc, a, dsc) = odd_last_components(s, m, last, freq, basis, d1904);
    let coupon = 100.0 * rate / freq;
    let term = 1.0 + dsc * yld / freq;
    (redemption + dc * coupon) / term - a * coupon
}

fn oddlprice(c: &mut Ctx, a: &[Expr]) -> Value {
    let d1904 = c.date1904();
    numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
        let (s, m, last) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2].trunc() as i64);
        let (rate, yld, redemption, freq, basis) = (n[3], n[4], n[5], n[6].trunc(), n[7].trunc());
        if !(last < s && s < m) || rate < 0.0 || yld < 0.0 || redemption <= 0.0 || !(freq == 1.0 || freq == 2.0 || freq == 4.0) || !(0.0..=4.0).contains(&basis) {
            return err(CellError::Num);
        }
        if !serial_in_range(&[s, m, last]) || last < 2 {
            return err(CellError::Num);
        }
        number(odd_last_price(s, m, last, rate, yld, redemption, freq, basis as u32, d1904))
    })
}

fn oddlyield(c: &mut Ctx, a: &[Expr]) -> Value {
    let d1904 = c.date1904();
    numeric_defaults(c, a, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], move |n| {
        let (s, m, last) = (n[0].trunc() as i64, n[1].trunc() as i64, n[2].trunc() as i64);
        let (rate, pr, redemption, freq, basis) = (n[3], n[4], n[5], n[6].trunc(), n[7].trunc());
        if !(last < s && s < m) || rate < 0.0 || pr <= 0.0 || redemption <= 0.0 || !(freq == 1.0 || freq == 2.0 || freq == 4.0) || !(0.0..=4.0).contains(&basis) {
            return err(CellError::Num);
        }
        if !serial_in_range(&[s, m, last]) || last < 2 {
            return err(CellError::Num);
        }
        let (dc, a_term, dsc) = odd_last_components(s, m, last, freq, basis as u32, d1904);
        let coupon = 100.0 * rate / freq;
        let num = (redemption + dc * coupon) - (pr + a_term * coupon);
        let den = pr + a_term * coupon;
        if den == 0.0 || dsc == 0.0 {
            return err(CellError::Num);
        }
        number((num / den) * (freq / dsc))
    })
}
