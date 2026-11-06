// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Engineering functions: number bases, bits, complex numbers, Bessel
//! functions, the error function, units.

use super::{Func, arg_num, arg_text, err, number, numeric, numeric_defaults};
use crate::eval::{Ctx, Value, to_number, to_text};
use crate::formula::Expr;
use crate::special::{erf, erfc};
use crate::value::CellError;

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "BIN2DEC" => |c, a| from_base(c, a, 2, None),
        "OCT2DEC" => |c, a| from_base(c, a, 8, None),
        "HEX2DEC" => |c, a| from_base(c, a, 16, None),
        "BIN2OCT" => |c, a| from_base(c, a, 2, Some(8)),
        "BIN2HEX" => |c, a| from_base(c, a, 2, Some(16)),
        "OCT2BIN" => |c, a| from_base(c, a, 8, Some(2)),
        "OCT2HEX" => |c, a| from_base(c, a, 8, Some(16)),
        "HEX2BIN" => |c, a| from_base(c, a, 16, Some(2)),
        "HEX2OCT" => |c, a| from_base(c, a, 16, Some(8)),
        "DEC2BIN" => |c, a| dec_to(c, a, 2),
        "DEC2OCT" => |c, a| dec_to(c, a, 8),
        "DEC2HEX" => |c, a| dec_to(c, a, 16),
        "BITAND" => |c, a| bits(c, a, |x, y| x & y),
        "BITOR" => |c, a| bits(c, a, |x, y| x | y),
        "BITXOR" => |c, a| bits(c, a, |x, y| x ^ y),
        "BITLSHIFT" => |c, a| shift(c, a, true),
        "BITRSHIFT" => |c, a| shift(c, a, false),
        "DELTA" => |c, a| numeric_defaults(c, a, &[0.0, 0.0], |n| number(if n[0] == n[1] { 1.0 } else { 0.0 })),
        "GESTEP" => |c, a| numeric_defaults(c, a, &[0.0, 0.0], |n| number(if n[0] >= n[1] { 1.0 } else { 0.0 })),
        "ERF" => |c, a| {
            let given = a.len();
            numeric(c, a, move |n| if given > 1 { number(erf(n[1]) - erf(n[0])) } else { number(erf(n[0])) })
        },
        "ERF.PRECISE" => |c, a| numeric(c, a, |n| number(erf(n[0]))),
        "ERFC" | "ERFC.PRECISE" => |c, a| numeric(c, a, |n| number(erfc(n[0]))),
        "BESSELJ" => |c, a| bessel(c, a, besselj),
        "BESSELY" => |c, a| bessel(c, a, bessely),
        "BESSELI" => |c, a| bessel(c, a, besseli),
        "BESSELK" => |c, a| bessel(c, a, besselk),
        "COMPLEX" => complex_fn,
        "IMREAL" => |c, a| im1(c, a, |z| number(z.re)),
        "IMAGINARY" => |c, a| im1(c, a, |z| number(z.im)),
        "IMABS" => |c, a| im1(c, a, |z| number(z.re.hypot(z.im))),
        "IMARGUMENT" => |c, a| im1(c, a, |z| if z.re == 0.0 && z.im == 0.0 { err(CellError::Div0) } else { number(z.im.atan2(z.re)) }),
        "IMCONJUGATE" => |c, a| im1z(c, a, |z| Some(C { re: z.re, im: -z.im })),
        "IMEXP" => |c, a| im1z(c, a, |z| Some(z.exp())),
        "IMLN" => |c, a| im1z(c, a, |z| z.ln()),
        "IMLOG10" => |c, a| im1z(c, a, |z| z.ln().map(|l| l.scale(1.0 / std::f64::consts::LN_10))),
        // Excel's IMLOG2(z) evaluates as IMDIV(IMLN(z), IMLN(2)); IMLN(2) text formats as "0.693147180559945".
        "IMLOG2" => |c, a| im1z(c, a, |z| z.ln().map(|l| l.scale(1.0 / 0.693147180559945))),
        "IMSQRT" => |c, a| im1z(c, a, |z| Some(z.sqrt())),
        "IMSIN" => |c, a| im1z(c, a, |z| Some(z.sin())),
        "IMCOS" => |c, a| im1z(c, a, |z| Some(z.cos())),
        "IMTAN" => |c, a| im1z(c, a, |z| z.sin().div(z.cos())),
        "IMCOT" => |c, a| im1z(c, a, |z| z.cos().div(z.sin())),
        "IMSEC" => |c, a| im1z(c, a, |z| C::ONE.div(z.cos())),
        "IMCSC" => |c, a| im1z(c, a, |z| C::ONE.div(z.sin())),
        "IMSINH" => |c, a| im1z(c, a, |z| Some(z.sinh())),
        "IMCOSH" => |c, a| im1z(c, a, |z| Some(z.cosh())),
        "IMSECH" => |c, a| im1z(c, a, |z| C::ONE.div(z.cosh())),
        "IMCSCH" => |c, a| im1z(c, a, |z| C::ONE.div(z.sinh())),
        "IMPOWER" => impower,
        "IMDIV" => |c, a| im2(c, a, |x, y| x.div(y)),
        "IMSUB" => |c, a| im2(c, a, |x, y| Some(C { re: x.re - y.re, im: x.im - y.im })),
        "IMSUM" => |c, a| im_fold(c, a, C::ZERO, |x, y| C { re: x.re + y.re, im: x.im + y.im }),
        "IMPRODUCT" => |c, a| im_fold(c, a, C::ONE, |x, y| x.mul(y)),
        "CONVERT" => convert,
        _ => return None,
    })
}

// ----- Number bases --------------------------------------------------------

/// Text in `base` (10 digits at most, the top bit negative) as a number.
fn parse_base(t: &str, base: u32) -> Result<i64, CellError> {
    let t = t.trim();
    if t.len() > 10 || t.is_empty() && false {
        return Err(CellError::Num);
    }
    if t.is_empty() {
        return Ok(0);
    }
    let mut v: i64 = 0;
    for ch in t.chars() {
        let d = ch.to_digit(base).ok_or(CellError::Num)?;
        v = v * base as i64 + d as i64;
    }
    // Ten digits: two's complement over 10 digits.
    let full = (base as i64).pow(10);
    if t.len() == 10 && v >= full / 2 {
        v -= full;
    }
    Ok(v)
}

fn base_text(v: Value) -> Result<String, CellError> {
    match v {
        Value::Bool(_) => Err(CellError::Value),
        Value::Number(n) => {
            if n.fract() != 0.0 {
                return Err(CellError::Num);
            }
            Ok(format!("{}", n as i64))
        }
        v => to_text(&v),
    }
}

fn from_base(c: &mut Ctx, a: &[Expr], base: u32, to: Option<u32>) -> Value {
    // The places first: their error comes before the number's.
    let places = match places_arg(c, a, 1) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let text = match base_text(c.eval_scalar(&a[0])) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let v = match parse_base(&text, base) {
        Ok(v) => v,
        Err(e) => return err(e),
    };
    match to {
        None => number(v as f64),
        Some(to) => to_base(v, to, places),
    }
}

fn places_arg(c: &mut Ctx, a: &[Expr], i: usize) -> Result<Option<usize>, CellError> {
    match a.get(i) {
        None | Some(Expr::Missing) => Ok(None),
        Some(e) => {
            let v = c.eval_scalar(e);
            if let Value::Bool(_) = v {
                return Err(CellError::Value);
            }
            let p = to_number(&v, false)?.trunc();
            if !(0.0..=10.0).contains(&p) {
                return Err(CellError::Num);
            }
            Ok(Some(p as usize))
        }
    }
}

/// A number in `base`: two's complement over 10 digits when negative.
fn to_base(v: i64, base: u32, places: Option<usize>) -> Value {
    let full = (base as i64).pow(10);
    if v < -full / 2 || v >= full / 2 {
        return err(CellError::Num);
    }
    let u = if v < 0 { v + full } else { v };
    let mut digits = Vec::new();
    let mut x = u;
    loop {
        digits.push(std::char::from_digit((x % base as i64) as u32, base).unwrap().to_ascii_uppercase());
        x /= base as i64;
        if x == 0 {
            break;
        }
    }
    let mut s: String = digits.into_iter().rev().collect();
    if v >= 0
        && let Some(p) = places
    {
        if s.len() > p {
            return err(CellError::Num);
        }
        s = format!("{}{s}", "0".repeat(p - s.len()));
    }
    Value::Text(s)
}

fn dec_to(c: &mut Ctx, a: &[Expr], base: u32) -> Value {
    let v = c.eval_scalar(&a[0]);
    if let Value::Bool(_) = v {
        return err(CellError::Value);
    }
    let n = match to_number(&v, false) {
        Ok(n) => n.trunc(),
        Err(e) => return err(e),
    };
    let places = match places_arg(c, a, 1) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    to_base(n as i64, base, places)
}

const BIT_LIMIT: f64 = 281_474_976_710_655.0; // 2^48 - 1

fn bits(c: &mut Ctx, a: &[Expr], f: fn(u64, u64) -> u64) -> Value {
    numeric(c, a, move |n| {
        let (x, y) = (n[0], n[1]);
        if x < 0.0 || y < 0.0 || x > BIT_LIMIT || y > BIT_LIMIT || x.fract() != 0.0 || y.fract() != 0.0 {
            return err(CellError::Num);
        }
        number(f(x as u64, y as u64) as f64)
    })
}

fn shift(c: &mut Ctx, a: &[Expr], left: bool) -> Value {
    numeric(c, a, move |n| {
        let (x, s) = (n[0], n[1].trunc());
        if x < 0.0 || x > BIT_LIMIT || x.fract() != 0.0 {
            return err(CellError::Num);
        }
        if x == 0.0 {
            return number(0.0);
        }
        if s.abs() > 53.0 {
            return err(CellError::Num);
        }
        let s = if left { s } else { -s };
        let r = if s >= 0.0 { x * 2f64.powi(s as i32) } else { (x as u64 >> (-s) as u32) as f64 };
        if r > BIT_LIMIT { err(CellError::Num) } else { number(r) }
    })
}

// ----- Bessel --------------------------------------------------------------

fn bessel(c: &mut Ctx, a: &[Expr], f: fn(f64, i32) -> Result<f64, CellError>) -> Value {
    numeric(c, a, move |n| {
        let (x, order) = (n[0], n[1].trunc());
        if order < 0.0 || order > 1e9 {
            return err(CellError::Num);
        }
        f(x, order as i32).map_or_else(err, number)
    })
}

/// Excel's J_n(x) and Y_n(x) use Numerical Recipes / Hart 6825 rational approximations (minimax rational fits)
/// for small |x| < 8, and Hankel asymptotic rational polynomial expansions for |x| >= 8.
fn bessj0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 8.0 {
        let y = x * x;
        let ans1 = 57568490574.0 + y * (-13362590354.0 + y * (651619640.7 + y * (-11214424.18 + y * (77392.33017 + y * (-184.9052456)))));
        let ans2 = 57568490411.0 + y * (1029532985.0 + y * (9494680.718 + y * (59272.64853 + y * (267.8532712 + y * 1.0))));
        ans1 / ans2
    } else {
        let z = 8.0 / ax;
        let y = z * z;
        let xx = ax - 0.785398164;
        let ans1 = 1.0 + y * (-0.1098628627e-2 + y * (0.2734510407e-4 + y * (-0.2073370639e-5 + y * 0.2093887211e-6)));
        let ans2 = -0.1562499995e-1 + y * (0.1430488765e-3 + y * (-0.6911147651e-5 + y * (0.7621095161e-6 - y * 0.934945152e-7)));
        (0.636619772 / ax).sqrt() * (xx.cos() * ans1 - z * xx.sin() * ans2)
    }
}

fn bessj1(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 8.0 {
        let y = x * x;
        let ans1 = x * (72362614232.0 + y * (-7895059235.0 + y * (242396853.1 + y * (-2972611.439 + y * (15704.48260 + y * (-30.16036606))))));
        let ans2 = 144725228442.0 + y * (2300535178.0 + y * (18583304.74 + y * (99447.43394 + y * (376.9991397 + y * 1.0))));
        ans1 / ans2
    } else {
        // Special case for Excel's ToolPak at x=10 if applicable, or standard NR asymptotic
        let z = 8.0 / ax;
        let y = z * z;
        let xx = ax - 2.356194491;
        let ans1 = 1.0 + y * (0.183105e-2 + y * (-0.3516396496e-4 + y * (0.2457520174e-5 + y * (-0.240337019e-6))));
        let ans2 = 0.04687499995 + y * (-0.2002690873e-3 + y * (0.8449199096e-5 + y * (-0.88228987e-6 + y * 0.105787412e-6)));
        let ans = (0.636619772 / ax).sqrt() * (xx.cos() * ans1 - z * xx.sin() * ans2);
        if x < 0.0 { -ans } else { ans }
    }
}

/// J_n(x) using Numerical Recipes / Hart rational minimax fit and recurrence relations.
fn besselj(x: f64, n: i32) -> Result<f64, CellError> {
    if n < 0 || x.abs() >= 1e14 {
        return Err(CellError::Num);
    }
    let ax = x.abs();
    if ax == 0.0 {
        return Ok(if n == 0 { 1.0 } else { 0.0 });
    }
    if n == 0 {
        return Ok(bessj0(x));
    }
    if n == 1 {
        return Ok(bessj1(x));
    }
    let tox = 2.0 / ax;
    let ans = if ax > n as f64 {
        let mut bjm = bessj0(ax);
        let mut bj = bessj1(ax);
        for j in 1..n {
            let bjp = j as f64 * tox * bj - bjm;
            bjm = bj;
            bj = bjp;
        }
        bj
    } else {
        let m = 2 * (n + 3);
        let mut jsum = false;
        let mut sum_val = 0.0;
        let mut bjp = 0.0;
        let mut ans_val = 0.0;
        let mut bj = 1.0;
        for j in (1..=m).rev() {
            let bjm = j as f64 * tox * bj - bjp;
            bjp = bj;
            bj = bjm;
            if bj.abs() > 1e10 {
                bj *= 1e-10;
                bjp *= 1e-10;
                ans_val *= 1e-10;
                sum_val *= 1e-10;
            }
            if jsum {
                sum_val += bj;
            }
            jsum = !jsum;
            if j == n {
                ans_val = bjp;
            }
        }
        sum_val = 2.0 * sum_val - bj;
        ans_val / sum_val
    };
    Ok(if x < 0.0 && n % 2 == 1 { -ans } else { ans })
}


/// Excel's I_n(x) and K_n(x) are not the exact functions: they are Abramowitz & Stegun's polynomial
/// approximations (9.8.1–9.8.8, about 1e-7 off) for n = 0 and 1, then Miller's downward recurrence (I) or the
/// upward one (K) for the orders above; the same approximations here give the same digits.
fn poly(c: &[f64], t: f64) -> f64 {
    c.iter().rev().fold(0.0, |acc, &a| acc * t + a)
}

fn bessi0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 3.75 {
        poly(&[1.0, 3.5156229, 3.0899424, 1.2067492, 0.2659732, 0.0360768, 0.0045813], (x / 3.75).powi(2))
    } else {
        ax.exp() / ax.sqrt() * poly(&[0.39894228, 0.01328592, 0.00225319, -0.00157565, 0.00916281, -0.02057706, 0.02635537, -0.01647633, 0.00392377], 3.75 / ax)
    }
}

fn bessi1(x: f64) -> f64 {
    let ax = x.abs();
    let r = if ax < 3.75 {
        ax * poly(&[0.5, 0.87890594, 0.51498869, 0.15084934, 0.02658733, 0.00301532, 0.00032411], (x / 3.75).powi(2))
    } else {
        ax.exp() / ax.sqrt() * poly(&[0.39894228, -0.03988024, -0.00362018, 0.00163801, -0.01031555, 0.02282967, -0.02895312, 0.01787654, -0.00420059], 3.75 / ax)
    };
    if x < 0.0 { -r } else { r }
}

fn besseli(x: f64, n: i32) -> Result<f64, CellError> {
    if n < 0 {
        return Err(CellError::Num);
    }
    if x != 0.0 && x.abs() < 1e-10 && n >= 10 {
        return Err(CellError::Num);
    }
    let ax = x.abs();
    if ax == 0.0 {
        return Ok(if n == 0 { 1.0 } else { 0.0 });
    }
    let ans = match n {
        0 => return Ok(bessi0(x)),
        1 => return Ok(bessi1(x)),
        _ => {
            let two_over_x = 2.0 / ax;
            let (mut bip, mut ans, mut bi) = (0.0f64, 0.0f64, 1.0f64);
            let m = 2 * (n + (40.0 * n as f64).sqrt() as i32);
            for j in (1..=m).rev() {
                let bim = bip + j as f64 * two_over_x * bi;
                bip = bi;
                bi = bim;
                if bi.abs() > 1e10 {
                    ans *= 1e-10;
                    bi *= 1e-10;
                    bip *= 1e-10;
                }
                if j == n {
                    ans = bip;
                }
            }
            ans * bessi0(ax) / bi
        }
    };
    Ok(if x < 0.0 && n % 2 == 1 { -ans } else { ans })
}

fn bessy0(x: f64) -> f64 {
    if x < 8.0 {
        let y = x * x;
        let ans1 = -2957821389.0 + y * (7062834065.0 + y * (-512359803.6 + y * (10879881.29 + y * (-86327.92757 + y * 228.4622733))));
        let ans2 = 40076544269.0 + y * (745249964.8 + y * (7189466.438 + y * (47447.26470 + y * (226.1030244 + y * 1.0))));
        (ans1 / ans2) + 0.636619772 * bessj0(x) * x.ln()
    } else {
        let z = 8.0 / x;
        let y = z * z;
        let xx = x - 0.785398164;
        let ans1 = 1.0 + y * (-0.1098628627e-2 + y * (0.2734510407e-4 + y * (-0.2073370639e-5 + y * 0.2093887211e-6)));
        let ans2 = -0.1562499995e-1 + y * (0.1430488765e-3 + y * (-0.6911147651e-5 + y * (0.7621095161e-6 - y * 0.934945152e-7)));
        (0.636619772 / x).sqrt() * (xx.sin() * ans1 + z * xx.cos() * ans2)
    }
}

fn bessy1(x: f64) -> f64 {
    if x < 8.0 {
        let y = x * x;
        let ans1 = x * (-0.4900604943e13 + y * (0.1275274390e13 + y * (-0.5153438139e11 + y * (0.7349264551e9 + y * (-0.4237922726e7 + y * 0.8511937935e4)))));
        let ans2 = 0.2499580570e14 + y * (0.4244419664e12 + y * (0.3733650367e10 + y * (0.2245904002e8 + y * (0.1020426050e6 + y * (0.3549632885e3 + y)))));
        (ans1 / ans2) + 0.636619772 * (bessj1(x) * x.ln() - 1.0 / x)
    } else {
        let z = 8.0 / x;
        let y = z * z;
        let xx = x - 2.356194491;
        let ans1 = 1.0 + y * (0.183105e-2 + y * (-0.3516396496e-4 + y * (0.2457520174e-5 + y * (-0.240337019e-6))));
        let ans2 = 0.04687499995 + y * (-0.2002690873e-3 + y * (0.8449199096e-5 + y * (-0.88228987e-6 + y * 0.105787412e-6)));
        (0.636619772 / x).sqrt() * (xx.sin() * ans1 + z * xx.cos() * ans2)
    }
}

/// Y_n(x) using Numerical Recipes / Hart rational minimax fit and recurrence relations.
fn bessely(x: f64, n: i32) -> Result<f64, CellError> {
    if x <= 0.0 || n < 0 || n > 255 || x >= 1e14 {
        return Err(CellError::Num);
    }
    if n == 0 {
        return Ok(bessy0(x));
    }
    if n == 1 {
        return Ok(bessy1(x));
    }
    let tox = 2.0 / x;
    let mut by = bessy1(x);
    let mut bym = bessy0(x);
    for j in 1..n {
        let byp = j as f64 * tox * by - bym;
        bym = by;
        by = byp;
    }
    Ok(by)
}

fn besselk(x: f64, n: i32) -> Result<f64, CellError> {
    if x <= 0.0 || n < 0 {
        return Err(CellError::Num);
    }
    if x >= 700.0 {
        return Ok(0.0);
    }
    let k0 = if x <= 2.0 {
        -(x / 2.0).ln() * bessi0(x) + poly(&[-0.57721566, 0.42278420, 0.23069756, 0.03488590, 0.00262698, 0.00010750, 0.00000740], x * x / 4.0)
    } else {
        (-x).exp() / x.sqrt() * poly(&[1.25331414, -0.07832358, 0.02189568, -0.01062446, 0.00587872, -0.00251540, 0.00053208], 2.0 / x)
    };
    if n == 0 {
        return Ok(k0);
    }
    let k1 = if x <= 2.0 {
        (x / 2.0).ln() * bessi1(x) + poly(&[1.0, 0.15443144, -0.67278579, -0.18156897, -0.01919402, -0.00110404, -0.00004686], x * x / 4.0) / x
    } else {
        (-x).exp() / x.sqrt() * poly(&[1.25331414, 0.23498619, -0.03655620, 0.01504268, -0.00780353, 0.00325614, -0.00068245], 2.0 / x)
    };
    let (mut a, mut b) = (k0, k1);
    for k in 1..n {
        let next = 2.0 * k as f64 / x * b + a;
        a = b;
        b = next;
    }
    Ok(b)
}

// ----- Complex numbers -----------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
struct C {
    re: f64,
    im: f64,
}

impl C {
    const ZERO: C = C { re: 0.0, im: 0.0 };
    const ONE: C = C { re: 1.0, im: 0.0 };

    fn scale(self, k: f64) -> C {
        C { re: self.re * k, im: self.im * k }
    }
    fn mul(self, o: C) -> C {
        C { re: self.re * o.re - self.im * o.im, im: self.re * o.im + self.im * o.re }
    }
    fn div(self, o: C) -> Option<C> {
        // Over an infinitely large number: zero.
        if !o.re.is_finite() || !o.im.is_finite() {
            return Some(C::ZERO);
        }
        let d = o.re * o.re + o.im * o.im;
        if d == 0.0 {
            return None;
        }
        Some(C { re: (self.re * o.re + self.im * o.im) / d, im: (self.im * o.re - self.re * o.im) / d })
    }
    fn exp(self) -> C {
        let e = self.re.exp();
        C { re: e * self.im.cos(), im: e * self.im.sin() }
    }
    fn ln(self) -> Option<C> {
        if self.re == 0.0 && self.im == 0.0 {
            return None;
        }
        Some(C { re: self.re.hypot(self.im).ln(), im: self.im.atan2(self.re) })
    }
    fn sqrt(self) -> C {
        let r = self.re.hypot(self.im).sqrt();
        let t = self.im.atan2(self.re) / 2.0;
        C { re: r * t.cos(), im: r * t.sin() }
    }
    fn sin(self) -> C {
        C { re: self.re.sin() * self.im.cosh(), im: self.re.cos() * self.im.sinh() }
    }
    fn cos(self) -> C {
        C { re: self.re.cos() * self.im.cosh(), im: -self.re.sin() * self.im.sinh() }
    }
    fn sinh(self) -> C {
        C { re: self.re.sinh() * self.im.cos(), im: self.re.cosh() * self.im.sin() }
    }
    fn cosh(self) -> C {
        C { re: self.re.cosh() * self.im.cos(), im: self.re.sinh() * self.im.sin() }
    }
}

/// A complex number as text (`3+4i`, `-2.5j`, `i`, `5`), and its suffix.
fn parse_complex(t: &str) -> Result<(C, Option<char>), CellError> {
    let t = t.trim();
    if t.is_empty() {
        return Err(CellError::Num);
    }
    // A date or time typed as text is its number.
    if let Some(n) = crate::eval::parse_number_text(t, false) {
        return Ok((C { re: n, im: 0.0 }, None));
    }
    let (body, suffix) = match t.chars().last() {
        Some(ch @ ('i' | 'j')) => (&t[..t.len() - 1], Some(ch)),
        _ => (t, None),
    };
    let num = |s: &str| -> Result<f64, CellError> {
        if s.is_empty() || s == "+" {
            return Ok(1.0);
        }
        if s == "-" {
            return Ok(-1.0);
        }
        if s.contains(char::is_whitespace) || s.starts_with("++") {
            return Err(CellError::Num);
        }
        s.parse::<f64>().map_err(|_| CellError::Num)
    };
    match suffix {
        None => Ok((C { re: num(body)?, im: 0.0 }, None)),
        Some(sfx) => {
            // Split the real part off at the last sign that is not an exponent's.
            let bytes = body.as_bytes();
            let mut split = None;
            for i in (1..bytes.len()).rev() {
                if (bytes[i] == b'+' || bytes[i] == b'-') && !matches!(bytes[i - 1], b'e' | b'E') {
                    split = Some(i);
                    break;
                }
            }
            match split {
                Some(i) => Ok((C { re: body[..i].parse::<f64>().map_err(|_| CellError::Num)?, im: num(&body[i..])? }, Some(sfx))),
                None => Ok((C { re: 0.0, im: num(body)? }, Some(sfx))),
            }
        }
    }
}

/// A number as IM* functions write it (up to 15 significant digits).
fn im_number(v: f64) -> String {
    crate::eval::number_to_text(v)
}

fn format_complex(z: C, suffix: char) -> String {
    let (re, im) = (clean_zero(z.re), clean_zero(z.im));
    if im == 0.0 {
        return im_number(re);
    }
    let imag = match im {
        1.0 => String::new(),
        -1.0 => "-".into(),
        v => im_number(v),
    };
    if re == 0.0 {
        return format!("{imag}{suffix}");
    }
    let sign = if im > 0.0 { "+" } else { "" };
    format!("{}{sign}{imag}{suffix}", im_number(re))
}

fn clean_zero(v: f64) -> f64 {
    if v.abs() < 1e-300 { 0.0 } else { v }
}

fn complex_arg(c: &mut Ctx, e: &Expr) -> Result<(C, Option<char>), CellError> {
    match c.eval_scalar(e) {
        Value::Number(n) => Ok((C { re: n, im: 0.0 }, None)),
        Value::Bool(_) => Err(CellError::Value),
        Value::Empty => Ok((C::ZERO, None)),
        v => parse_complex(&to_text(&v)?),
    }
}

fn im1(c: &mut Ctx, a: &[Expr], f: fn(C) -> Value) -> Value {
    match complex_arg(c, &a[0]) {
        Ok((z, _)) => f(z),
        Err(e) => err(e),
    }
}

fn im1z(c: &mut Ctx, a: &[Expr], f: fn(C) -> Option<C>) -> Value {
    match complex_arg(c, &a[0]) {
        Ok((z, s)) => match f(z) {
            Some(r) if r.re.is_finite() && r.im.is_finite() => Value::Text(format_complex(r, s.unwrap_or('i'))),
            _ => err(CellError::Num),
        },
        Err(e) => err(e),
    }
}

fn im2(c: &mut Ctx, a: &[Expr], f: fn(C, C) -> Option<C>) -> Value {
    let (x, y) = match (complex_arg(c, &a[0]), complex_arg(c, &a[1])) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    if let (Some(p), Some(q)) = (x.1, y.1)
        && p != q
    {
        return err(CellError::Value);
    }
    match f(x.0, y.0) {
        Some(r) => Value::Text(format_complex(r, x.1.or(y.1).unwrap_or('i'))),
        None => err(CellError::Num),
    }
}

fn im_fold(c: &mut Ctx, a: &[Expr], start: C, f: fn(C, C) -> C) -> Value {
    let mut acc = start;
    let mut suffix: Option<char> = None;
    for (v, _) in super::collect_values(c, a, true) {
        let (z, s) = match v {
            Value::Empty => (C::ZERO, None),
            Value::Number(n) => (C { re: n, im: 0.0 }, None),
            Value::Error(e) => return err(e),
            Value::Bool(_) => return err(CellError::Value),
            v => match to_text(&v).map_err(|e| e).and_then(|t| parse_complex(&t)) {
                Ok(x) => x,
                Err(e) => return err(e),
            },
        };
        if let (Some(p), Some(q)) = (suffix, s)
            && p != q
        {
            return err(CellError::Value);
        }
        suffix = suffix.or(s);
        acc = f(acc, z);
    }
    Value::Text(format_complex(acc, suffix.unwrap_or('i')))
}

fn complex_fn(c: &mut Ctx, a: &[Expr]) -> Value {
    // The suffix first: a bad one wins over the numbers' errors.
    let suffix = match a.get(2) {
        None | Some(Expr::Missing) => 'i',
        Some(_) => match arg_text(c, a, 2) {
            Ok(s) if s == "i" || s == "j" || s.is_empty() => s.chars().next().unwrap_or('i'),
            Ok(_) => return err(CellError::Value),
            Err(e) => return err(e),
        },
    };
    let (re, im) = match (arg_num(c, a, 0), arg_num(c, a, 1)) {
        (Ok(r), Ok(i)) => (r, i),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    Value::Text(format_complex(C { re, im }, suffix))
}

fn impower(c: &mut Ctx, a: &[Expr]) -> Value {
    let (z, s) = match complex_arg(c, &a[0]) {
        Ok(x) => x,
        Err(e) => return err(e),
    };
    let p = match arg_num(c, a, 1) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    if z == C::ZERO {
        return if p > 0.0 { Value::Text("0".into()) } else { err(CellError::Num) };
    }
    let r = if z.im == 0.0 && z.re > 0.0 && p.fract() != 0.0 {
        (p * z.re.ln()).exp()
    } else {
        z.re.hypot(z.im).powf(p)
    };
    let t = z.im.atan2(z.re) * p;
    let result = C { re: r * t.cos(), im: r * t.sin() };
    if !result.re.is_finite() || !result.im.is_finite() {
        return err(CellError::Num);
    }
    Value::Text(format_complex(result, s.unwrap_or('i')))
}

// ----- Units ----------------------------------------------------------------

/// A unit: its category, its size in the category's base unit, and
/// whether it takes metric (and binary) prefixes.
struct Unit {
    name: &'static str,
    category: &'static str,
    factor: f64,
    prefixes: bool,
}

const UNITS: &[Unit] = &[
    // Weight and mass (gram).
    Unit { name: "g", category: "mass", factor: 1.0, prefixes: true },
    Unit { name: "sg", category: "mass", factor: 14_593.902_937_206_4, prefixes: false },
    Unit { name: "lbm", category: "mass", factor: 453.592_37, prefixes: false },
    Unit { name: "u", category: "mass", factor: 1.660_539_040e-24, prefixes: true },
    Unit { name: "ozm", category: "mass", factor: 28.349_523_125, prefixes: false },
    Unit { name: "grain", category: "mass", factor: 0.064_798_91, prefixes: false },
    Unit { name: "cwt", category: "mass", factor: 45_359.237, prefixes: false },
    Unit { name: "shweight", category: "mass", factor: 45_359.237, prefixes: false },
    Unit { name: "uk_cwt", category: "mass", factor: 50_802.345_44, prefixes: false },
    Unit { name: "lcwt", category: "mass", factor: 50_802.345_44, prefixes: false },
    Unit { name: "hweight", category: "mass", factor: 50_802.345_44, prefixes: false },
    Unit { name: "stone", category: "mass", factor: 6_350.293_18, prefixes: false },
    Unit { name: "ton", category: "mass", factor: 907_184.74, prefixes: false },
    Unit { name: "uk_ton", category: "mass", factor: 1_016_046.908_8, prefixes: false },
    Unit { name: "LTON", category: "mass", factor: 1_016_046.908_8, prefixes: false },
    Unit { name: "brton", category: "mass", factor: 1_016_046.908_8, prefixes: false },
    // Distance (metre).
    Unit { name: "m", category: "distance", factor: 1.0, prefixes: true },
    Unit { name: "mi", category: "distance", factor: 1_609.344, prefixes: false },
    Unit { name: "Nmi", category: "distance", factor: 1_852.0, prefixes: false },
    Unit { name: "in", category: "distance", factor: 0.0254, prefixes: false },
    Unit { name: "ft", category: "distance", factor: 0.3048, prefixes: false },
    Unit { name: "yd", category: "distance", factor: 0.9144, prefixes: false },
    Unit { name: "ang", category: "distance", factor: 1e-10, prefixes: true },
    Unit { name: "ell", category: "distance", factor: 1.143, prefixes: false },
    Unit { name: "ly", category: "distance", factor: 9_460_730_472_580_800.0, prefixes: true },
    Unit { name: "parsec", category: "distance", factor: 30_856_775_812_815_500.0, prefixes: true },
    Unit { name: "pc", category: "distance", factor: 30_856_775_812_815_500.0, prefixes: true },
    Unit { name: "Pica", category: "distance", factor: 0.0254 / 72.0, prefixes: false },
    Unit { name: "Picapt", category: "distance", factor: 0.0254 / 72.0, prefixes: false },
    Unit { name: "pica", category: "distance", factor: 0.0254 / 6.0, prefixes: false },
    Unit { name: "survey_mi", category: "distance", factor: 1_609.347_218_694_437, prefixes: false },
    // Time (second).
    Unit { name: "yr", category: "time", factor: 31_557_600.0, prefixes: false },
    Unit { name: "day", category: "time", factor: 86_400.0, prefixes: false },
    Unit { name: "d", category: "time", factor: 86_400.0, prefixes: false },
    Unit { name: "hr", category: "time", factor: 3_600.0, prefixes: false },
    Unit { name: "mn", category: "time", factor: 60.0, prefixes: false },
    Unit { name: "min", category: "time", factor: 60.0, prefixes: false },
    Unit { name: "sec", category: "time", factor: 1.0, prefixes: true },
    Unit { name: "s", category: "time", factor: 1.0, prefixes: true },
    // Pressure (pascal).
    Unit { name: "Pa", category: "pressure", factor: 1.0, prefixes: true },
    Unit { name: "p", category: "pressure", factor: 1.0, prefixes: true },
    Unit { name: "atm", category: "pressure", factor: 101_325.0, prefixes: true },
    Unit { name: "at", category: "pressure", factor: 101_325.0, prefixes: true },
    Unit { name: "mmHg", category: "pressure", factor: 133.322_368_421_052_6, prefixes: true },
    Unit { name: "psi", category: "pressure", factor: 6_894.757_293_168_36, prefixes: false },
    Unit { name: "Torr", category: "pressure", factor: 133.322_368_421_052_6, prefixes: false },
    // Force (newton).
    Unit { name: "N", category: "force", factor: 1.0, prefixes: true },
    Unit { name: "dyn", category: "force", factor: 1e-5, prefixes: true },
    Unit { name: "dy", category: "force", factor: 1e-5, prefixes: true },
    Unit { name: "lbf", category: "force", factor: 4.448_221_615_260_5, prefixes: false },
    Unit { name: "pond", category: "force", factor: 0.009_806_65, prefixes: true },
    // Energy (joule).
    Unit { name: "J", category: "energy", factor: 1.0, prefixes: true },
    Unit { name: "e", category: "energy", factor: 1e-7, prefixes: true },
    Unit { name: "c", category: "energy", factor: 4.184, prefixes: true },
    Unit { name: "cal", category: "energy", factor: 4.1868, prefixes: true },
    Unit { name: "eV", category: "energy", factor: 1.602_176_487e-19, prefixes: true },
    Unit { name: "ev", category: "energy", factor: 1.602_176_487e-19, prefixes: true },
    Unit { name: "HPh", category: "energy", factor: 2_684_519.537_696_17, prefixes: false },
    Unit { name: "hh", category: "energy", factor: 2_684_519.537_696_17, prefixes: false },
    Unit { name: "Wh", category: "energy", factor: 3_600.0, prefixes: true },
    Unit { name: "wh", category: "energy", factor: 3_600.0, prefixes: true },
    Unit { name: "flb", category: "energy", factor: 0.042_140_110_093_805_4, prefixes: false },
    Unit { name: "BTU", category: "energy", factor: 1_055.055_852_62, prefixes: false },
    Unit { name: "btu", category: "energy", factor: 1_055.055_852_62, prefixes: false },
    // Power (watt).
    Unit { name: "HP", category: "power", factor: 745.699_871_582_270_2, prefixes: false },
    Unit { name: "h", category: "power", factor: 745.699_871_582_270_2, prefixes: false },
    Unit { name: "PS", category: "power", factor: 735.498_75, prefixes: false },
    Unit { name: "W", category: "power", factor: 1.0, prefixes: true },
    Unit { name: "w", category: "power", factor: 1.0, prefixes: true },
    // Magnetism (tesla).
    Unit { name: "T", category: "magnetism", factor: 1.0, prefixes: true },
    Unit { name: "ga", category: "magnetism", factor: 1e-4, prefixes: true },
    // Volume (cubic metre).
    Unit { name: "l", category: "volume", factor: 1e-3, prefixes: true },
    Unit { name: "L", category: "volume", factor: 1e-3, prefixes: true },
    Unit { name: "lt", category: "volume", factor: 1e-3, prefixes: true },
    Unit { name: "tsp", category: "volume", factor: 4.928_921_593_75e-6, prefixes: false },
    Unit { name: "tspm", category: "volume", factor: 5e-6, prefixes: false },
    Unit { name: "tbs", category: "volume", factor: 1.478_676_478_125e-5, prefixes: false },
    Unit { name: "oz", category: "volume", factor: 2.957_352_956_25e-5, prefixes: false },
    Unit { name: "cup", category: "volume", factor: 2.365_882_365e-4, prefixes: false },
    Unit { name: "pt", category: "volume", factor: 4.731_764_73e-4, prefixes: false },
    Unit { name: "us_pt", category: "volume", factor: 4.731_764_73e-4, prefixes: false },
    Unit { name: "uk_pt", category: "volume", factor: 5.682_612_5e-4, prefixes: false },
    Unit { name: "qt", category: "volume", factor: 9.463_529_46e-4, prefixes: false },
    Unit { name: "uk_qt", category: "volume", factor: 1.136_522_5e-3, prefixes: false },
    Unit { name: "gal", category: "volume", factor: 3.785_411_784e-3, prefixes: false },
    Unit { name: "uk_gal", category: "volume", factor: 4.546_09e-3, prefixes: false },
    Unit { name: "m3", category: "volume", factor: 1.0, prefixes: true },
    Unit { name: "ang3", category: "volume", factor: 1e-30, prefixes: true },
    Unit { name: "barrel", category: "volume", factor: 0.158_987_294_928, prefixes: false },
    Unit { name: "bushel", category: "volume", factor: 0.035_239_070_166_88, prefixes: false },
    Unit { name: "ft3", category: "volume", factor: 0.028_316_846_592, prefixes: false },
    Unit { name: "in3", category: "volume", factor: 1.638_706_4e-5, prefixes: false },
    Unit { name: "ly3", category: "volume", factor: 8.467_866_646_237_2e47, prefixes: false },
    Unit { name: "mi3", category: "volume", factor: 4_168_181_825.440_58, prefixes: false },
    Unit { name: "yd3", category: "volume", factor: 0.764_554_857_984, prefixes: false },
    Unit { name: "Nmi3", category: "volume", factor: 6_352_182_208.0, prefixes: false },
    Unit { name: "MTON", category: "volume", factor: 1.132_673_863_68, prefixes: false },
    Unit { name: "GRT", category: "volume", factor: 2.831_684_659_2, prefixes: false },
    Unit { name: "regton", category: "volume", factor: 2.831_684_659_2, prefixes: false },
    // Area (square metre).
    Unit { name: "m2", category: "area", factor: 1.0, prefixes: true },
    Unit { name: "ha", category: "area", factor: 10_000.0, prefixes: false },
    Unit { name: "uk_acre", category: "area", factor: 4_046.856_422_4, prefixes: false },
    Unit { name: "us_acre", category: "area", factor: 4_046.872_609_874_252, prefixes: false },
    Unit { name: "ang2", category: "area", factor: 1e-20, prefixes: true },
    Unit { name: "ar", category: "area", factor: 100.0, prefixes: true },
    Unit { name: "ft2", category: "area", factor: 0.092_903_04, prefixes: false },
    Unit { name: "in2", category: "area", factor: 6.4516e-4, prefixes: false },
    Unit { name: "ly2", category: "area", factor: 8.950_542_107_488_33e31, prefixes: false },
    Unit { name: "mi2", category: "area", factor: 2_589_988.110_336, prefixes: false },
    Unit { name: "Nmi2", category: "area", factor: 3_429_904.0, prefixes: false },
    Unit { name: "yd2", category: "area", factor: 0.836_127_36, prefixes: false },
    Unit { name: "Morgen", category: "area", factor: 2_500.0, prefixes: false },
    // Information (bit).
    Unit { name: "bit", category: "information", factor: 1.0, prefixes: true },
    Unit { name: "byte", category: "information", factor: 8.0, prefixes: true },
    // Speed (m/s).
    Unit { name: "m/s", category: "speed", factor: 1.0, prefixes: true },
    Unit { name: "m/sec", category: "speed", factor: 1.0, prefixes: true },
    Unit { name: "m/h", category: "speed", factor: 1.0 / 3600.0, prefixes: true },
    Unit { name: "m/hr", category: "speed", factor: 1.0 / 3600.0, prefixes: true },
    Unit { name: "mph", category: "speed", factor: 0.447_04, prefixes: false },
    Unit { name: "kn", category: "speed", factor: 1_852.0 / 3600.0, prefixes: false },
    Unit { name: "admkn", category: "speed", factor: 0.514_773_333_333_333_3, prefixes: false },
];

const PREFIXES: &[(&str, f64)] = &[
    ("Y", 1e24), ("Z", 1e21), ("E", 1e18), ("P", 1e15), ("T", 1e12), ("G", 1e9), ("M", 1e6), ("k", 1e3), ("h", 1e2), ("da", 1e1), ("e", 1e1),
    ("d", 1e-1), ("c", 1e-2), ("m", 1e-3), ("u", 1e-6), ("n", 1e-9), ("p", 1e-12), ("f", 1e-15), ("a", 1e-18), ("z", 1e-21), ("y", 1e-24),
];

const BINARY_PREFIXES: &[(&str, f64)] = &[
    ("Yi", 1_208_925_819_614_629_174_706_176.0), ("Zi", 1_180_591_620_717_411_303_424.0), ("Ei", 1_152_921_504_606_846_976.0), ("Pi", 1_125_899_906_842_624.0),
    ("Ti", 1_099_511_627_776.0), ("Gi", 1_073_741_824.0), ("Mi", 1_048_576.0), ("ki", 1_024.0),
];

/// A unit name (with a prefix, perhaps) as its category and size.
fn find_unit(name: &str) -> Option<(&'static str, f64, bool)> {
    if let Some(u) = UNITS.iter().find(|u| u.name == name) {
        return Some((u.category, u.factor, false));
    }
    if name == "C" || name == "cel" {
        return Some(("temperature", 0.0, true));
    }
    for t in ["F", "fah", "K", "kel", "Rank", "Reau"] {
        if name == t {
            return Some(("temperature", 0.0, true));
        }
    }
    for (p, f) in BINARY_PREFIXES.iter().chain(PREFIXES.iter()) {
        if let Some(rest) = name.strip_prefix(p)
            && let Some(u) = UNITS.iter().find(|u| u.name == rest && u.prefixes)
        {
            if BINARY_PREFIXES.iter().any(|(bp, _)| bp == p) && u.category != "information" {
                continue;
            }
            // Squared and cubed units take the prefix squared or cubed.
            let power = if rest.ends_with('2') { 2 } else if rest.ends_with('3') { 3 } else { 1 };
            return Some((u.category, u.factor * f.powi(power), false));
        }
        if let Some(rest) = name.strip_prefix(p)
            && ["K", "kel"].contains(&rest)
        {
            return Some(("temperature-k", *f, true));
        }
    }
    None
}

fn to_kelvin(v: f64, unit: &str) -> f64 {
    match unit {
        "C" | "cel" => v + 273.15,
        "F" | "fah" => (v - 32.0) * 5.0 / 9.0 + 273.15,
        "Rank" => v * 5.0 / 9.0,
        "Reau" => v * 1.25 + 273.15,
        _ => v,
    }
}

fn from_kelvin(k: f64, unit: &str) -> f64 {
    match unit {
        "C" | "cel" => k - 273.15,
        "F" | "fah" => (k - 273.15) * 9.0 / 5.0 + 32.0,
        "Rank" => k * 9.0 / 5.0,
        "Reau" => (k - 273.15) * 0.8,
        _ => k,
    }
}

fn convert(c: &mut Ctx, a: &[Expr]) -> Value {
    let v = match arg_num(c, a, 0) {
        Ok(v) => v,
        Err(e) => return err(e),
    };
    let (from, to) = match (arg_text(c, a, 1), arg_text(c, a, 2)) {
        (Ok(f), Ok(t)) => (f, t),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    let (Some((cf, ff, tf)), Some((ct, ft, tt))) = (find_unit(&from), find_unit(&to)) else {
        return err(CellError::NA);
    };
    if tf || tt {
        if !(tf && tt) {
            return err(CellError::NA);
        }
        // Temperatures (a prefixed kelvin scales).
        let kf = if cf == "temperature-k" { v * ff } else { to_kelvin(v, &from) };
        let r = if ct == "temperature-k" { kf / ft } else { from_kelvin(kf, &to) };
        return number(r);
    }
    if cf != ct {
        return err(CellError::NA);
    }
    number(v * ff / ft)
}
