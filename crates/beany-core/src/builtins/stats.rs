//! Probability distributions: normal, Student t, chi-square, F.
//! CDFs come from the regularized incomplete gamma/beta functions; inverses use
//! bracketed bisection on the CDF, which is monotone, so they are robust everywhere.

use super::*;
use crate::ops;

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("normcdf", |_, a, _| {
        norm_fn(&a, |x, m, s| norm_cdf((x - m) / s))
    });
    b.insert("normpdf", |_, a, _| {
        norm_fn(&a, |x, m, s| {
            (-0.5 * ((x - m) / s).powi(2)).exp() / (s * (2.0 * std::f64::consts::PI).sqrt())
        })
    });
    b.insert("norminv", |_, a, _| {
        norm_fn(&a, |p, m, s| m + s * norm_inv(p))
    });
    b.insert("tcdf", |_, a, _| two(&a, "tcdf", t_cdf));
    b.insert("tpdf", |_, a, _| two(&a, "tpdf", t_pdf));
    b.insert("tinv", |_, a, _| {
        two(&a, "tinv", |p, v| invert(p, |x| t_cdf(x, v), -1e3, 1e3))
    });
    b.insert("chi2cdf", |_, a, _| {
        two(&a, "chi2cdf", |x, k| {
            if x <= 0.0 {
                0.0
            } else {
                gamma_p(k / 2.0, x / 2.0)
            }
        })
    });
    b.insert("chi2pdf", |_, a, _| two(&a, "chi2pdf", chi2_pdf));
    b.insert("chi2inv", |_, a, _| {
        two(&a, "chi2inv", |p, k| {
            invert(
                p,
                |x| {
                    if x <= 0.0 {
                        0.0
                    } else {
                        gamma_p(k / 2.0, x / 2.0)
                    }
                },
                0.0,
                1e5,
            )
        })
    });
    b.insert("fcdf", |_, a, _| three(&a, "fcdf", f_cdf));
    b.insert("fpdf", |_, a, _| three(&a, "fpdf", f_pdf));
    b.insert("finv", |_, a, _| {
        three(&a, "finv", |p, d1, d2| {
            invert(p, |x| f_cdf(x, d1, d2), 0.0, 1e6)
        })
    });
    b.insert("expcdf", |_, a, _| {
        two(&a, "expcdf", |x, mu| {
            if x < 0.0 {
                0.0
            } else {
                1.0 - (-x / mu).exp()
            }
        })
    });
    b.insert("expinv", |_, a, _| {
        two(&a, "expinv", |p, mu| -mu * (1.0 - p).ln())
    });
    b.insert("unifcdf", |_, a, _| {
        three(&a, "unifcdf", |x, lo, hi| {
            ((x - lo) / (hi - lo)).clamp(0.0, 1.0)
        })
    });
    b.insert("betainc", |_, a, _| {
        three(&a, "betainc", |x, p, q| beta_inc(p, q, x))
    });
    b.insert("gammainc", |_, a, _| {
        two(&a, "gammainc", |x, s| gamma_p(s, x))
    });
    b.insert("beta", |_, a, _| {
        two(&a, "beta", |p, q| {
            (libm::lgamma(p) + libm::lgamma(q) - libm::lgamma(p + q)).exp()
        })
    });
}

fn arg(a: &[Value], i: usize, default: f64) -> MResult<Array<f64>> {
    match a.get(i) {
        Some(v) => v.to_num(),
        None => Ok(Array::scalar(default)),
    }
}

/// Normal functions take (x, mu = 0, sigma = 1).
fn norm_fn(a: &[Value], f: fn(f64, f64, f64) -> f64) -> R {
    check_args(a, 1, 3)?;
    let out = broadcast3(
        arg(a, 0, 0.0)?,
        arg(a, 1, 0.0)?,
        arg(a, 2, 1.0)?,
        "normal",
        |x, m, s| if s <= 0.0 { f64::NAN } else { f(x, m, s) },
    )?;
    one(Value::mat(out))
}

fn two(a: &[Value], name: &str, f: impl Fn(f64, f64) -> f64) -> R {
    check_args(a, 2, 2)?;
    one(Value::mat(ops::broadcast(
        &a[0].to_num()?,
        &a[1].to_num()?,
        f,
        name,
    )?))
}

fn three(a: &[Value], name: &str, f: impl Fn(f64, f64, f64) -> f64) -> R {
    check_args(a, 3, 3)?;
    one(Value::mat(broadcast3(
        a[0].to_num()?,
        a[1].to_num()?,
        a[2].to_num()?,
        name,
        f,
    )?))
}

/// Elementwise f(x, p, q) with implicit expansion across all three inputs.
fn broadcast3(
    x: Array<f64>,
    p: Array<f64>,
    q: Array<f64>,
    name: &str,
    f: impl Fn(f64, f64, f64) -> f64,
) -> MResult<Array<f64>> {
    let pq = ops::broadcast(&p, &q, |u, _| u, name)?;
    let qp = ops::broadcast(&q, &p, |u, _| u, name)?;
    let xs = ops::broadcast(&x, &pq, |u, _| u, name)?;
    let ps = ops::broadcast(&pq, &xs, |u, _| u, name)?;
    let qs = ops::broadcast(&qp, &xs, |u, _| u, name)?;
    // xs may still be smaller than the p/q shape; expand it to match.
    let xs = ops::broadcast(&xs, &ps, |u, _| u, name)?;
    Ok(Array::new(
        xs.rows,
        xs.cols,
        (0..xs.numel())
            .map(|i| f(xs.data[i], ps.data[i], qs.data[i]))
            .collect(),
    ))
}

pub fn norm_cdf(z: f64) -> f64 {
    0.5 * libm::erfc(-z / std::f64::consts::SQRT_2)
}

pub fn norm_inv(p: f64) -> f64 {
    if p.is_nan() || !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    if p == 0.0 {
        return f64::NEG_INFINITY;
    }
    if p == 1.0 {
        return f64::INFINITY;
    }
    // Rational starting guess, then Newton steps on the exact CDF.
    let t = if p < 0.5 {
        (-2.0 * p.ln()).sqrt()
    } else {
        (-2.0 * (1.0 - p).ln()).sqrt()
    };
    let mut x = t
        - (2.515517 + 0.802853 * t + 0.010328 * t * t)
            / (1.0 + 1.432788 * t + 0.189269 * t * t + 0.001308 * t * t * t);
    if p < 0.5 {
        x = -x;
    }
    for _ in 0..6 {
        let pdf = (-0.5 * x * x).exp() / (2.0 * std::f64::consts::PI).sqrt();
        if pdf == 0.0 {
            break;
        }
        x -= (norm_cdf(x) - p) / pdf;
    }
    x
}

fn t_cdf(t: f64, v: f64) -> f64 {
    if v <= 0.0 || t.is_nan() {
        return f64::NAN;
    }
    let x = v / (v + t * t);
    let tail = 0.5 * beta_inc(v / 2.0, 0.5, x);
    if t > 0.0 {
        1.0 - tail
    } else {
        tail
    }
}

fn t_pdf(t: f64, v: f64) -> f64 {
    let ln = libm::lgamma((v + 1.0) / 2.0)
        - libm::lgamma(v / 2.0)
        - 0.5 * (v * std::f64::consts::PI).ln()
        - (v + 1.0) / 2.0 * (1.0 + t * t / v).ln();
    ln.exp()
}

fn chi2_pdf(x: f64, k: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    let h = k / 2.0;
    ((h - 1.0) * x.ln() - x / 2.0 - h * 2f64.ln() - libm::lgamma(h)).exp()
}

fn f_cdf(x: f64, d1: f64, d2: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    beta_inc(d1 / 2.0, d2 / 2.0, d1 * x / (d1 * x + d2))
}

fn f_pdf(x: f64, d1: f64, d2: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    let ln = 0.5 * (d1 * (d1 * x).ln() + d2 * d2.ln() - (d1 + d2) * (d1 * x + d2).ln())
        - x.ln()
        - (libm::lgamma(d1 / 2.0) + libm::lgamma(d2 / 2.0) - libm::lgamma((d1 + d2) / 2.0));
    ln.exp()
}

/// Solve cdf(x) = p by bisection, widening the bracket if needed.
fn invert(p: f64, cdf: impl Fn(f64) -> f64, mut lo: f64, mut hi: f64) -> f64 {
    if p.is_nan() || !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    if p == 0.0 {
        return if lo == 0.0 { 0.0 } else { f64::NEG_INFINITY };
    }
    if p == 1.0 {
        return f64::INFINITY;
    }
    while cdf(hi) < p && hi < 1e300 {
        hi *= 10.0;
    }
    while lo < 0.0 && cdf(lo) > p && lo > -1e300 {
        lo *= 10.0;
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if cdf(mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if (hi - lo).abs() <= 1e-15 * mid.abs().max(1e-300) {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// Regularized lower incomplete gamma P(s, x).
pub fn gamma_p(s: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x < s + 1.0 {
        let mut sum = 1.0 / s;
        let mut term = sum;
        let mut n = s;
        for _ in 0..1000 {
            n += 1.0;
            term *= x / n;
            sum += term;
            if term.abs() < sum.abs() * 1e-16 {
                break;
            }
        }
        (sum.ln() - x + s * x.ln() - libm::lgamma(s)).exp()
    } else {
        // Continued fraction for the upper tail (Lentz).
        let tiny = 1e-300;
        let mut b = x + 1.0 - s;
        let mut c = 1.0 / tiny;
        let mut d = 1.0 / b;
        let mut h = d;
        for i in 1..1000 {
            let an = -(i as f64) * (i as f64 - s);
            b += 2.0;
            d = an * d + b;
            if d.abs() < tiny {
                d = tiny;
            }
            c = b + an / c;
            if c.abs() < tiny {
                c = tiny;
            }
            d = 1.0 / d;
            let del = d * c;
            h *= del;
            if (del - 1.0).abs() < 1e-16 {
                break;
            }
        }
        1.0 - (-x + s * x.ln() - libm::lgamma(s)).exp() * h
    }
}

/// Regularized incomplete beta I_x(a, b).
pub fn beta_inc(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let front =
        (libm::lgamma(a + b) - libm::lgamma(a) - libm::lgamma(b) + a * x.ln() + b * (1.0 - x).ln())
            .exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        front * beta_cf(a, b, x) / a
    } else {
        1.0 - front * beta_cf(b, a, 1.0 - x) / b
    }
}

fn beta_cf(a: f64, b: f64, x: f64) -> f64 {
    let tiny = 1e-300;
    let (qab, qap, qam) = (a + b, a + 1.0, a - 1.0);
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..1000 {
        let m = m as f64;
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-16 {
            break;
        }
    }
    h
}
