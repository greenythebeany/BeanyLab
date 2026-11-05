//! Numerical methods: optimization and root finding, integration, splines,
//! polynomial algebra, digital filters and signal helpers.

use super::*;

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("optimset", optimset);
    b.insert("optimoptions", |it, a, n| {
        // optimoptions('solver', Name, Value, ...) builds the same option structure.
        let rest = if a.first().is_some_and(|v| v.is_text()) && a.len() % 2 == 1 {
            a[1..].to_vec()
        } else {
            a
        };
        optimset(it, rest, n)
    });
    b.insert("optimget", |_, a, _| {
        check_args(&a, 2, 3)?;
        let name = a[1].text()?;
        if let Value::Struct(s) = &a[0] {
            if let Some(k) = s.keys.iter().position(|k| k.eq_ignore_ascii_case(&name)) {
                let v = &s.arr.data[0][k];
                if !v.is_empty() {
                    return one(v.clone());
                }
            }
        }
        one(a.get(2).cloned().unwrap_or_else(Value::empty))
    });
    b.insert("fzero", fzero);
    b.insert("fminbnd", fminbnd);
    b.insert("fminsearch", fminsearch);
    b.insert("fminunc", fminunc);
    b.insert("lsqnonlin", lsqnonlin);
    b.insert("lsqcurvefit", lsqcurvefit);
    b.insert("fsolve", fsolve);
    b.insert("integral", |it, a, n| integral_fn(it, a, n, false));
    b.insert("quadgk", |it, a, n| integral_fn(it, a, n, true));
    b.insert("quad", quad);
    b.insert("quadl", quad);
    b.insert("integral2", integral2);
    b.insert("dblquad", integral2);
    b.insert("spline", |_, a, _| pp_fn(&a, false));
    b.insert("pchip", |_, a, _| pp_fn(&a, true));
    b.insert("ppval", |_, a, _| {
        check_args(&a, 2, 2)?;
        let pp = Pp::from_value(&a[0])?;
        one(Value::mat(a[1].to_num()?.map(|x| pp.eval(*x))))
    });
    b.insert("mkpp", |_, a, _| {
        check_args(&a, 2, 2)?;
        let breaks = a[0].to_num()?.data;
        let c = a[1].to_num()?;
        let pieces = breaks.len() - 1;
        let order = c.numel() / pieces.max(1);
        let coefs = (0..pieces)
            .map(|i| (0..order).map(|j| *c.get(i, j)).collect())
            .collect();
        one(Pp { breaks, coefs }.to_value())
    });
    b.insert("polyder", polyder);
    b.insert("polyint", |_, a, _| {
        check_args(&a, 1, 2)?;
        let p = a[0].to_num()?.data;
        let k = a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(0.0);
        let n = p.len();
        let mut out: Vec<f64> = p
            .iter()
            .enumerate()
            .map(|(i, c)| c / (n - i) as f64)
            .collect();
        out.push(k);
        one(Value::row(out))
    });
    b.insert("deconv", |_, a, _| {
        check_args(&a, 2, 2)?;
        let (q, r) = deconv(&a[0].to_num()?.data, &a[1].to_num()?.data)?;
        Ok(vec![Value::row(q), Value::row(r)])
    });
    b.insert("residue", residue);
    b.insert("filter", filter);
    b.insert("filtfilt", filtfilt);
    b.insert("butter", butter);
    b.insert("freqz", freqz);
    b.insert("xcorr", xcorr);
    b.insert("sinc", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::mat(a[0].to_num()?.map(|x| {
            if *x == 0.0 {
                1.0
            } else {
                (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
            }
        })))
    });
    b.insert("accumarray", accumarray);
    b.insert("sqrtm", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = crate::ops::to_dmatrix(&a[0].to_num()?);
        let (mut y, mut z) = (m.clone(), nalgebra::DMatrix::identity(m.nrows(), m.ncols()));
        for _ in 0..100 {
            let yi = y
                .clone()
                .try_inverse()
                .ok_or_else(|| merr!("Matrix is singular."))?;
            let zi = z
                .clone()
                .try_inverse()
                .ok_or_else(|| merr!("Matrix is singular."))?;
            let y1 = (&y + &zi) * 0.5;
            let z1 = (&z + &yi) * 0.5;
            let done = (&y1 - &y).norm() <= 1e-15 * y1.norm();
            y = y1;
            z = z1;
            if done {
                break;
            }
        }
        one(Value::mat(crate::ops::from_dmatrix(&y)))
    });
    b.insert("orth", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = crate::ops::to_dmatrix(&a[0].to_num()?);
        let (r, cl) = (m.nrows(), m.ncols());
        let svd = m.svd(true, false);
        let u = svd.u.unwrap();
        let s = &svd.singular_values;
        let tol = r.max(cl) as f64 * s.max() * f64::EPSILON;
        let rank = s.iter().filter(|x| **x > tol).count();
        let data: Vec<f64> = (0..rank)
            .flat_map(|j| u.column(j).iter().copied().collect::<Vec<_>>())
            .collect();
        one(Value::mat(Array::new(r, rank, data)))
    });
    b.insert("normrnd", |it, a, _| {
        if a.len() < 2 {
            return Err(merr!("Not enough input arguments."));
        }
        let mu = a[0].scalar()?;
        let sigma = a[1].scalar()?;
        let (r, c, _) = size_args(&a[2..]).unwrap_or((1, 1, None));
        use rand_distr::Distribution;
        let d = rand_distr::Normal::new(mu, sigma).map_err(|e| merr!("{}", e))?;
        let data = (0..r * c).map(|_| d.sample(&mut it.rng)).collect();
        one(Value::mat(Array::new(r, c, data)))
    });
    b.insert("unifrnd", |it, a, _| {
        if a.len() < 2 {
            return Err(merr!("Not enough input arguments."));
        }
        let (lo, hi) = (a[0].scalar()?, a[1].scalar()?);
        let (r, c, _) = size_args(&a[2..]).unwrap_or((1, 1, None));
        use rand::Rng;
        let data = (0..r * c)
            .map(|_| lo + (hi - lo) * it.rng.gen::<f64>())
            .collect();
        one(Value::mat(Array::new(r, c, data)))
    });
    b.insert("exprnd", |it, a, _| {
        if a.is_empty() {
            return Err(merr!("Not enough input arguments."));
        }
        let mu = a[0].scalar()?;
        let (r, c, _) = size_args(&a[1..]).unwrap_or((1, 1, None));
        use rand::Rng;
        let data = (0..r * c)
            .map(|_| -mu * (1.0 - it.rng.gen::<f64>()).ln())
            .collect();
        one(Value::mat(Array::new(r, c, data)))
    });
    b.insert("sawtooth", |_, a, _| {
        check_args(&a, 1, 2)?;
        let w = a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(1.0);
        one(Value::mat(a[0].to_num()?.map(|t| {
            let x = (t / (2.0 * std::f64::consts::PI)).rem_euclid(1.0);
            if x < w {
                -1.0 + 2.0 * x / w
            } else {
                1.0 - 2.0 * (x - w) / (1.0 - w)
            }
        })))
    });
    b.insert("square", |_, a, _| {
        check_args(&a, 1, 2)?;
        let duty = a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(50.0) / 100.0;
        one(Value::mat(a[0].to_num()?.map(|t| {
            if (t / (2.0 * std::f64::consts::PI)).rem_euclid(1.0) < duty {
                1.0
            } else {
                -1.0
            }
        })))
    });
    b.insert("rectpuls", |_, a, _| {
        check_args(&a, 1, 2)?;
        let w = a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(1.0);
        one(Value::mat(a[0].to_num()?.map(|t| {
            if *t >= -w / 2.0 && *t < w / 2.0 {
                1.0
            } else {
                0.0
            }
        })))
    });
    b.insert("tripuls", |_, a, _| {
        check_args(&a, 1, 2)?;
        let w = a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(1.0);
        one(Value::mat(
            a[0].to_num()?.map(|t| (1.0 - 2.0 * t.abs() / w).max(0.0)),
        ))
    });
    b.insert("chirp", |_, a, _| {
        check_args(&a, 1, 4)?;
        let f0 = a.get(1).map(|v| v.scalar()).transpose()?.unwrap_or(0.0);
        let t1 = a.get(2).map(|v| v.scalar()).transpose()?.unwrap_or(1.0);
        let f1 = a.get(3).map(|v| v.scalar()).transpose()?.unwrap_or(100.0);
        let k = (f1 - f0) / t1;
        one(Value::mat(a[0].to_num()?.map(|t| {
            (2.0 * std::f64::consts::PI * (f0 * t + k / 2.0 * t * t)).cos()
        })))
    });
}

// ---------------------------------------------------------------- options

const OPTION_NAMES: [&str; 16] = [
    "Display",
    "MaxFunEvals",
    "MaxIter",
    "TolFun",
    "TolX",
    "FunValCheck",
    "OutputFcn",
    "PlotFcns",
    "Algorithm",
    "Jacobian",
    "GradObj",
    "MaxFunctionEvaluations",
    "MaxIterations",
    "FunctionTolerance",
    "StepTolerance",
    "OptimalityTolerance",
];

fn optimset(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let mut fields: Vec<(String, Value)> = Vec::new();
    let set = |fields: &mut Vec<(String, Value)>, k: &str, v: Value| {
        let canon = OPTION_NAMES
            .iter()
            .find(|n| n.eq_ignore_ascii_case(k))
            .map(|n| n.to_string())
            .unwrap_or_else(|| k.to_string());
        match fields.iter_mut().find(|(n, _)| *n == canon) {
            Some(slot) => slot.1 = v,
            None => fields.push((canon, v)),
        }
    };
    if a.is_empty() {
        for n in OPTION_NAMES.iter().take(11) {
            fields.push((n.to_string(), Value::empty()));
        }
        return one(Value::Struct(StructArr::scalar(fields)));
    }
    let mut i = 0;
    // Leading structures (old options, new options) are merged in order.
    while i < a.len() {
        match &a[i] {
            Value::Struct(s) => {
                for (k, key) in s.keys.iter().enumerate() {
                    let v = s.arr.data[0][k].clone();
                    if !v.is_empty() || !fields.iter().any(|(n, _)| n == key) {
                        set(&mut fields, key, v);
                    }
                }
                i += 1;
            }
            v if v.is_text() && a.len() == 1 => {
                // optimset('fminsearch'): the solver's defaults.
                for n in OPTION_NAMES.iter().take(11) {
                    fields.push((n.to_string(), Value::empty()));
                }
                i += 1;
            }
            v if v.is_text() => {
                let k = v.text()?;
                let val = a
                    .get(i + 1)
                    .cloned()
                    .ok_or_else(|| merr!("Missing value for option '{}'.", k))?;
                set(&mut fields, &k, val);
                i += 2;
            }
            _ => return Err(merr!("Invalid optimset argument.")),
        }
    }
    one(Value::Struct(StructArr::scalar(fields)))
}

struct Opts {
    tol_x: f64,
    tol_fun: f64,
    max_iter: usize,
    max_fev: usize,
    display: bool,
}

fn opts(v: Option<&Value>, n: usize) -> MResult<Opts> {
    let mut o = Opts {
        tol_x: 1e-4,
        tol_fun: 1e-4,
        max_iter: 200 * n.max(1),
        max_fev: 200 * n.max(1),
        display: false,
    };
    if let Some(Value::Struct(s)) = v {
        for (k, key) in s.keys.iter().enumerate() {
            let val = &s.arr.data[0][k];
            if val.is_empty() {
                continue;
            }
            match key.to_lowercase().as_str() {
                "tolx" | "steptolerance" => o.tol_x = val.scalar()?,
                "tolfun" | "functiontolerance" | "optimalitytolerance" => {
                    o.tol_fun = val.scalar()?
                }
                "maxiter" | "maxiterations" => o.max_iter = val.scalar()? as usize,
                "maxfunevals" | "maxfunctionevaluations" => o.max_fev = val.scalar()? as usize,
                "display" => o.display = matches!(val.text()?.as_str(), "iter" | "final"),
                _ => {}
            }
        }
    }
    Ok(o)
}

fn scalar_call(it: &mut Interp, f: &Value, x: f64, extra: &[Value]) -> MResult<f64> {
    let mut args = vec![Value::num(x)];
    args.extend_from_slice(extra);
    it.call1(f, args)?.scalar()
}

/// Call `f` with a vector shaped like `shape`.
fn vec_call(
    it: &mut Interp,
    f: &Value,
    x: &[f64],
    shape: (usize, usize),
    extra: &[Value],
) -> MResult<Value> {
    let mut args = vec![Value::mat(Array::new(shape.0, shape.1, x.to_vec()))];
    args.extend_from_slice(extra);
    it.call1(f, args)
}

fn output_struct(iters: usize, fev: usize, algorithm: &str, msg: &str) -> Value {
    Value::Struct(StructArr::scalar(vec![
        ("iterations".into(), Value::num(iters as f64)),
        ("funcCount".into(), Value::num(fev as f64)),
        ("algorithm".into(), Value::chars(algorithm)),
        ("message".into(), Value::chars(msg)),
    ]))
}

// ---------------------------------------------------------------- root finding / minimization

fn fzero(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let f = a[0].clone();
    let o = opts(a.get(2), 1)?;
    let extra = a.get(3..).unwrap_or(&[]).to_vec();
    let x0 = a[1].to_num()?;
    let mut fev = 0;
    let mut eval = |it: &mut Interp, x: f64| -> MResult<f64> {
        fev += 1;
        scalar_call(it, &f, x, &extra)
    };
    let (lo, hi, flo, fhi);
    if x0.numel() >= 2 {
        lo = x0.data[0];
        hi = x0.data[1];
        flo = eval(it, lo)?;
        fhi = eval(it, hi)?;
        if flo.signum() == fhi.signum() && flo != 0.0 && fhi != 0.0 {
            return Err(merr!(
                "The function values at the interval endpoints must differ in sign."
            ));
        }
    } else {
        // Search outward from x0 for a sign change.
        let x = x0.data[0];
        let fx = eval(it, x)?;
        if fx == 0.0 {
            return Ok(vec![Value::num(x), Value::num(0.0), Value::num(1.0)]);
        }
        let mut dx = if x == 0.0 { 1.0 / 50.0 } else { x.abs() / 50.0 };
        let mut found = None;
        for _ in 0..200 {
            let (a1, b1) = (x - dx, x + dx);
            let (fa, fb) = (eval(it, a1)?, eval(it, b1)?);
            if fa.is_finite() && fa.signum() != fx.signum() {
                found = Some((a1, x, fa, fx));
                break;
            }
            if fb.is_finite() && fb.signum() != fx.signum() {
                found = Some((x, b1, fx, fb));
                break;
            }
            dx *= std::f64::consts::SQRT_2;
        }
        let (a1, b1, fa, fb) = found.ok_or_else(|| merr!("Exiting fzero: no sign change was found while searching for an interval around {}.", x))?;
        lo = a1;
        hi = b1;
        flo = fa;
        fhi = fb;
    }
    // Brent's method.
    let (mut b_, mut fb) = (hi, fhi);
    let (mut a_, mut fa) = (lo, flo);
    let (mut c_, mut fc) = (a_, fa);
    let mut d = b_ - a_;
    let mut e = d;
    let tol_x = if a.get(2).is_some() {
        o.tol_x
    } else {
        f64::EPSILON
    };
    for _ in 0..o.max_iter.max(100) {
        if fb.signum() == fc.signum() {
            c_ = a_;
            fc = fa;
            d = b_ - a_;
            e = d;
        }
        if fc.abs() < fb.abs() {
            a_ = b_;
            b_ = c_;
            c_ = a_;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let tol = 2.0 * f64::EPSILON * b_.abs() + 0.5 * tol_x;
        let m = 0.5 * (c_ - b_);
        if m.abs() <= tol || fb == 0.0 {
            break;
        }
        if e.abs() >= tol && fa.abs() > fb.abs() {
            let s = fb / fa;
            let (mut p, mut q);
            if a_ == c_ {
                p = 2.0 * m * s;
                q = 1.0 - s;
            } else {
                let qq = fa / fc;
                let r = fb / fc;
                p = s * (2.0 * m * qq * (qq - r) - (b_ - a_) * (r - 1.0));
                q = (qq - 1.0) * (r - 1.0) * (s - 1.0);
            }
            if p > 0.0 {
                q = -q;
            } else {
                p = -p;
            }
            if 2.0 * p < (3.0 * m * q - (tol * q).abs()).min((e * q).abs()) {
                e = d;
                d = p / q;
            } else {
                d = m;
                e = m;
            }
        } else {
            d = m;
            e = m;
        }
        a_ = b_;
        fa = fb;
        b_ += if d.abs() > tol { d } else { tol.copysign(m) };
        fb = eval(it, b_)?;
    }
    let mut out = vec![Value::num(b_), Value::num(fb), Value::num(1.0)];
    if nargout > 3 {
        out.push(output_struct(
            0,
            fev,
            "bisection, interpolation",
            "Zero found in the interval",
        ));
    }
    Ok(out)
}

fn fminbnd(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 3 {
        return Err(merr!("Not enough input arguments."));
    }
    let f = a[0].clone();
    let (lo, hi) = (a[1].scalar()?, a[2].scalar()?);
    let o = opts(a.get(3), 1)?;
    let extra = a.get(4..).unwrap_or(&[]).to_vec();
    // Brent's golden-section search with parabolic steps.
    let cg = 0.5 * (3.0 - 5f64.sqrt());
    let (mut a_, mut b_) = (lo, hi);
    let mut v = a_ + cg * (b_ - a_);
    let (mut w, mut x) = (v, v);
    let (mut e, mut d) = (0.0f64, 0.0f64);
    let mut fx = scalar_call(it, &f, x, &extra)?;
    let (mut fv, mut fw) = (fx, fx);
    let mut fev = 1;
    let mut iters = 0;
    for _ in 0..o.max_iter.max(500) {
        iters += 1;
        let xm = 0.5 * (a_ + b_);
        let tol1 = f64::EPSILON.sqrt() * x.abs() + o.tol_x / 3.0;
        let tol2 = 2.0 * tol1;
        if (x - xm).abs() <= tol2 - 0.5 * (b_ - a_) {
            break;
        }
        let mut golden = true;
        if e.abs() > tol1 {
            let r = (x - w) * (fx - fv);
            let mut q = (x - v) * (fx - fw);
            let mut p = (x - v) * q - (x - w) * r;
            q = 2.0 * (q - r);
            if q > 0.0 {
                p = -p;
            }
            q = q.abs();
            let etemp = e;
            e = d;
            if p.abs() < (0.5 * q * etemp).abs() && p > q * (a_ - x) && p < q * (b_ - x) {
                d = p / q;
                let u = x + d;
                if u - a_ < tol2 || b_ - u < tol2 {
                    d = tol1.copysign(xm - x);
                }
                golden = false;
            }
        }
        if golden {
            e = if x >= xm { a_ - x } else { b_ - x };
            d = cg * e;
        }
        let u = if d.abs() >= tol1 {
            x + d
        } else {
            x + tol1.copysign(d)
        };
        let fu = scalar_call(it, &f, u, &extra)?;
        fev += 1;
        if fu <= fx {
            if u >= x {
                a_ = x;
            } else {
                b_ = x;
            }
            v = w;
            fv = fw;
            w = x;
            fw = fx;
            x = u;
            fx = fu;
        } else {
            if u < x {
                a_ = u;
            } else {
                b_ = u;
            }
            if fu <= fw || w == x {
                v = w;
                fv = fw;
                w = u;
                fw = fu;
            } else if fu <= fv || v == x || v == w {
                v = u;
                fv = fu;
            }
        }
    }
    Ok(vec![
        Value::num(x),
        Value::num(fx),
        Value::num(1.0),
        output_struct(
            iters,
            fev,
            "golden section search, parabolic interpolation",
            "",
        ),
    ])
}

fn fminsearch(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let f = a[0].clone();
    let x0 = a[1].to_num()?;
    let shape = (x0.rows, x0.cols);
    let n = x0.numel();
    let o = opts(a.get(2), n)?;
    let extra = a.get(3..).unwrap_or(&[]).to_vec();
    let fev = std::cell::Cell::new(0usize);
    let call = |it: &mut Interp, x: &[f64]| -> MResult<f64> {
        fev.set(fev.get() + 1);
        vec_call(it, &f, x, shape, &extra)?.scalar()
    };
    // Initial simplex: 5% steps (0.00025 for zero components).
    let mut simplex: Vec<Vec<f64>> = vec![x0.data.clone()];
    for i in 0..n {
        let mut y = x0.data.clone();
        y[i] = if y[i] != 0.0 { 1.05 * y[i] } else { 0.00025 };
        simplex.push(y);
    }
    let mut fs: Vec<f64> = Vec::with_capacity(n + 1);
    for s in &simplex {
        fs.push(call(it, s)?);
    }
    let (rho, chi, psi, sigma) = (1.0, 2.0, 0.5, 0.5);
    let mut iters = 0;
    let mut exitflag = 0.0;
    loop {
        // Sort by function value.
        let mut idx: Vec<usize> = (0..=n).collect();
        idx.sort_by(|&p, &q| {
            fs[p]
                .partial_cmp(&fs[q])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        simplex = idx.iter().map(|&k| simplex[k].clone()).collect();
        fs = idx.iter().map(|&k| fs[k]).collect();
        let fdiff = fs.iter().map(|v| (v - fs[0]).abs()).fold(0.0, f64::max);
        let xdiff = simplex
            .iter()
            .skip(1)
            .map(|s| {
                s.iter()
                    .zip(&simplex[0])
                    .map(|(p, q)| (p - q).abs())
                    .fold(0.0, f64::max)
            })
            .fold(0.0, f64::max);
        if fdiff <= o.tol_fun && xdiff <= o.tol_x {
            exitflag = 1.0;
            break;
        }
        if iters >= o.max_iter || fev.get() >= o.max_fev {
            break;
        }
        iters += 1;
        let xbar: Vec<f64> = (0..n)
            .map(|j| simplex[..n].iter().map(|s| s[j]).sum::<f64>() / n as f64)
            .collect();
        let worst = simplex[n].clone();
        let lin = |t: f64| -> Vec<f64> {
            xbar.iter()
                .zip(&worst)
                .map(|(m, w)| m + t * (m - w))
                .collect()
        };
        let xr = lin(rho);
        let fr = call(it, &xr)?;
        if fr < fs[0] {
            let xe = lin(rho * chi);
            let fe = call(it, &xe)?;
            if fe < fr {
                simplex[n] = xe;
                fs[n] = fe;
            } else {
                simplex[n] = xr;
                fs[n] = fr;
            }
        } else if fr < fs[n - 1] {
            simplex[n] = xr;
            fs[n] = fr;
        } else {
            let (xc, fc, outside) = if fr < fs[n] {
                let xc = lin(psi * rho);
                let fc = call(it, &xc)?;
                (xc, fc, true)
            } else {
                let xc = lin(-psi);
                let fc = call(it, &xc)?;
                (xc, fc, false)
            };
            if (outside && fc <= fr) || (!outside && fc < fs[n]) {
                simplex[n] = xc;
                fs[n] = fc;
            } else {
                // Shrink toward the best point.
                for k in 1..=n {
                    let s: Vec<f64> = simplex[0]
                        .iter()
                        .zip(&simplex[k])
                        .map(|(b, v)| b + sigma * (v - b))
                        .collect();
                    fs[k] = call(it, &s)?;
                    simplex[k] = s;
                }
            }
        }
    }
    if exitflag == 0.0 {
        it.warn("Exiting: Maximum number of iterations or function evaluations has been exceeded.");
    }
    let best = 0;
    Ok(vec![
        Value::mat(Array::new(shape.0, shape.1, simplex[best].clone())),
        Value::num(fs[best]),
        Value::num(exitflag),
        output_struct(iters, fev.get(), "Nelder-Mead simplex direct search", ""),
    ])
}

/// Forward-difference gradient.
fn gradient(
    it: &mut Interp,
    f: &Value,
    x: &[f64],
    fx: f64,
    shape: (usize, usize),
    extra: &[Value],
) -> MResult<Vec<f64>> {
    let mut g = vec![0.0; x.len()];
    for i in 0..x.len() {
        let h = f64::EPSILON.sqrt() * x[i].abs().max(1.0);
        let mut y = x.to_vec();
        y[i] += h;
        g[i] = (vec_call(it, f, &y, shape, extra)?.scalar()? - fx) / h;
    }
    Ok(g)
}

fn fminunc(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let f = a[0].clone();
    let x0 = a[1].to_num()?;
    let shape = (x0.rows, x0.cols);
    let n = x0.numel();
    let o = opts(a.get(2), n)?;
    let tol = if a.get(2).is_some() { o.tol_fun } else { 1e-6 };
    let extra = a.get(3..).unwrap_or(&[]).to_vec();
    let mut x = x0.data.clone();
    let mut fx = vec_call(it, &f, &x, shape, &extra)?.scalar()?;
    let mut g = gradient(it, &f, &x, fx, shape, &extra)?;
    let mut h = nalgebra::DMatrix::<f64>::identity(n, n);
    let mut iters = 0;
    // BFGS with a backtracking line search.
    for _ in 0..o.max_iter.max(400) {
        if g.iter().map(|v| v.abs()).fold(0.0, f64::max) < tol {
            break;
        }
        iters += 1;
        let gv = nalgebra::DVector::from_vec(g.clone());
        let p = -(&h * &gv);
        let mut t = 1.0;
        let slope = gv.dot(&p);
        let (mut xn, mut fnew);
        loop {
            xn = x
                .iter()
                .zip(p.iter())
                .map(|(xi, pi)| xi + t * pi)
                .collect::<Vec<f64>>();
            fnew = vec_call(it, &f, &xn, shape, &extra)?.scalar()?;
            if fnew <= fx + 1e-4 * t * slope || t < 1e-12 {
                break;
            }
            t *= 0.5;
        }
        let gn = gradient(it, &f, &xn, fnew, shape, &extra)?;
        let s = nalgebra::DVector::from_vec(xn.iter().zip(&x).map(|(p, q)| p - q).collect());
        let y = nalgebra::DVector::from_vec(gn.iter().zip(&g).map(|(p, q)| p - q).collect());
        let sy = s.dot(&y);
        if sy > 1e-12 {
            let rho = 1.0 / sy;
            let i = nalgebra::DMatrix::<f64>::identity(n, n);
            h = (&i - &s * y.transpose() * rho) * &h * (&i - &y * s.transpose() * rho)
                + &s * s.transpose() * rho;
        }
        let small = (fx - fnew).abs() < 1e-14 * fx.abs().max(1.0) && s.norm() < 1e-12;
        x = xn;
        fx = fnew;
        g = gn;
        if small {
            break;
        }
    }
    Ok(vec![
        Value::mat(Array::new(shape.0, shape.1, x)),
        Value::num(fx),
        Value::num(1.0),
        output_struct(iters, 0, "quasi-newton", ""),
        Value::col(g),
    ])
}

/// Levenberg–Marquardt on residuals `r(x)`, with simple bounds by projection.
fn levenberg(
    it: &mut Interp,
    r: &dyn Fn(&mut Interp, &[f64]) -> MResult<Vec<f64>>,
    x0: Vec<f64>,
    lb: &[f64],
    ub: &[f64],
    o: &Opts,
) -> MResult<(Vec<f64>, Vec<f64>, usize)> {
    let n = x0.len();
    let clamp = |x: &mut Vec<f64>| {
        for i in 0..n {
            if let Some(l) = lb.get(i) {
                x[i] = x[i].max(*l);
            }
            if let Some(u) = ub.get(i) {
                x[i] = x[i].min(*u);
            }
        }
    };
    let mut x = x0;
    clamp(&mut x);
    let mut res = r(it, &x)?;
    let mut cost: f64 = res.iter().map(|v| v * v).sum();
    let mut lambda = 1e-3;
    let mut iters = 0;
    for _ in 0..o.max_iter.max(400) {
        iters += 1;
        let m = res.len();
        let mut jac = nalgebra::DMatrix::<f64>::zeros(m, n);
        for j in 0..n {
            let h = f64::EPSILON.sqrt() * x[j].abs().max(1.0);
            let mut y = x.clone();
            y[j] += h;
            let ry = r(it, &y)?;
            for i in 0..m {
                jac[(i, j)] = (ry[i] - res[i]) / h;
            }
        }
        let rv = nalgebra::DVector::from_vec(res.clone());
        let jtj = jac.transpose() * &jac;
        let jtr = jac.transpose() * &rv;
        if jtr.amax() < 1e-10 * cost.max(1e-300).sqrt().max(1e-10) {
            break;
        }
        let mut improved = false;
        for _ in 0..30 {
            let mut lhs = jtj.clone();
            for i in 0..n {
                lhs[(i, i)] += lambda * jtj[(i, i)].max(1e-12);
            }
            let Some(step) = lhs.lu().solve(&(-&jtr)) else {
                lambda *= 10.0;
                continue;
            };
            let mut xn: Vec<f64> = x.iter().zip(step.iter()).map(|(a, b)| a + b).collect();
            clamp(&mut xn);
            let rn = r(it, &xn)?;
            let cn: f64 = rn.iter().map(|v| v * v).sum();
            if cn < cost {
                let dx = xn
                    .iter()
                    .zip(&x)
                    .map(|(p, q)| (p - q).abs())
                    .fold(0.0, f64::max);
                let dc = cost - cn;
                x = xn;
                res = rn;
                cost = cn;
                lambda = (lambda / 10.0).max(1e-12);
                improved = true;
                if dx < o.tol_x * 1e-2 * x.iter().map(|v| v.abs()).fold(1.0, f64::max)
                    || dc < o.tol_fun * 1e-2 * cost.max(1e-300)
                {
                    return Ok((x, res, iters));
                }
                break;
            }
            lambda *= 10.0;
        }
        if !improved {
            break;
        }
    }
    Ok((x, res, iters))
}

fn bounds(v: Option<&Value>) -> MResult<Vec<f64>> {
    Ok(match v {
        Some(v) if !v.is_empty() => v.to_num()?.data,
        _ => Vec::new(),
    })
}

fn lsqnonlin(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let f = a[0].clone();
    let x0 = a[1].to_num()?;
    let shape = (x0.rows, x0.cols);
    let lb = bounds(a.get(2))?;
    let ub = bounds(a.get(3))?;
    let o = opts(a.get(4), x0.numel())?;
    let extra = a.get(5..).unwrap_or(&[]).to_vec();
    let r = |it: &mut Interp, x: &[f64]| -> MResult<Vec<f64>> {
        Ok(vec_call(it, &f, x, shape, &extra)?.to_num()?.data)
    };
    let (x, res, iters) = levenberg(it, &r, x0.data.clone(), &lb, &ub, &o)?;
    let resnorm: f64 = res.iter().map(|v| v * v).sum();
    Ok(vec![
        Value::mat(Array::new(shape.0, shape.1, x)),
        Value::num(resnorm),
        Value::col(res),
        Value::num(1.0),
        output_struct(iters, 0, "levenberg-marquardt", ""),
    ])
}

fn lsqcurvefit(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 4 {
        return Err(merr!("Not enough input arguments."));
    }
    let f = a[0].clone();
    let x0 = a[1].to_num()?;
    let shape = (x0.rows, x0.cols);
    let xdata = a[2].clone();
    let ydata = a[3].to_num()?;
    let lb = bounds(a.get(4))?;
    let ub = bounds(a.get(5))?;
    let o = opts(a.get(6), x0.numel())?;
    let r = |it: &mut Interp, x: &[f64]| -> MResult<Vec<f64>> {
        let y = it
            .call1(
                &f,
                vec![
                    Value::mat(Array::new(shape.0, shape.1, x.to_vec())),
                    xdata.clone(),
                ],
            )?
            .to_num()?;
        Ok(y.data.iter().zip(&ydata.data).map(|(p, q)| p - q).collect())
    };
    let (x, res, iters) = levenberg(it, &r, x0.data.clone(), &lb, &ub, &o)?;
    let resnorm: f64 = res.iter().map(|v| v * v).sum();
    Ok(vec![
        Value::mat(Array::new(shape.0, shape.1, x)),
        Value::num(resnorm),
        Value::mat(Array::new(ydata.rows, ydata.cols, res)),
        Value::num(1.0),
        output_struct(iters, 0, "levenberg-marquardt", ""),
    ])
}

fn fsolve(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let f = a[0].clone();
    let x0 = a[1].to_num()?;
    let shape = (x0.rows, x0.cols);
    let mut o = opts(a.get(2), x0.numel())?;
    o.tol_fun = o.tol_fun.min(1e-12);
    o.tol_x = o.tol_x.min(1e-10);
    let extra = a.get(3..).unwrap_or(&[]).to_vec();
    let r = |it: &mut Interp, x: &[f64]| -> MResult<Vec<f64>> {
        Ok(vec_call(it, &f, x, shape, &extra)?.to_num()?.data)
    };
    let (x, res, iters) = levenberg(it, &r, x0.data.clone(), &[], &[], &o)?;
    let fv = vec_call(it, &f, &x, shape, &extra)?;
    let ok = res.iter().map(|v| v.abs()).fold(0.0, f64::max) < 1e-6;
    Ok(vec![
        Value::mat(Array::new(shape.0, shape.1, x)),
        fv,
        Value::num(if ok { 1.0 } else { -2.0 }),
        output_struct(iters, 0, "trust-region-dogleg", ""),
    ])
}

// ---------------------------------------------------------------- integration

/// 15-point Gauss–Kronrod nodes and weights on [-1, 1] (7-point Gauss embedded).
#[allow(clippy::excessive_precision)]
const XGK: [f64; 8] = [
    0.991455371120812639206854697526329,
    0.949107912342758524526189684047851,
    0.864864423359769072789712788640926,
    0.741531185599394439863864773280788,
    0.586087235467691130294144845693013,
    0.405845151377397166906606412076961,
    0.207784955007898467600689403773245,
    0.000000000000000000000000000000000,
];
#[allow(clippy::excessive_precision)]
const WGK: [f64; 8] = [
    0.022935322010529224963732008058970,
    0.063092092629978553290700663189204,
    0.104790010322250183839876322541518,
    0.140653259715525918745189590510238,
    0.169004726639267902826583426598550,
    0.190350578064785409913256402421014,
    0.204432940075298892414161999234649,
    0.209482141084727828012999174891714,
];
#[allow(clippy::excessive_precision)]
const WG: [f64; 4] = [
    0.129484966168869693270611432679082,
    0.279705391489276667901467771423780,
    0.381830050505118944950369775488975,
    0.417959183673469387755102040816327,
];

/// Evaluate `f` at all nodes of the given panels in one vectorized call.
fn gk_panels(
    it: &mut Interp,
    f: &dyn Fn(&mut Interp, &[f64]) -> MResult<Vec<f64>>,
    panels: &[(f64, f64)],
) -> MResult<Vec<(f64, f64)>> {
    let mut xs = Vec::with_capacity(panels.len() * 15);
    for &(a, b) in panels {
        let (c, h) = ((a + b) / 2.0, (b - a) / 2.0);
        for k in 0..7 {
            xs.push(c - h * XGK[k]);
            xs.push(c + h * XGK[k]);
        }
        xs.push(c);
    }
    let ys = f(it, &xs)?;
    if ys.len() != xs.len() {
        return Err(merr!("The integrand must return an output the same size as its input; use 'ArrayValued', true or vectorize it."));
    }
    Ok(panels
        .iter()
        .enumerate()
        .map(|(p, &(a, b))| {
            let h = (b - a) / 2.0;
            let y = &ys[p * 15..(p + 1) * 15];
            let mut k15 = WGK[7] * y[14];
            let mut g7 = WG[3] * y[14];
            for k in 0..7 {
                let s = y[2 * k] + y[2 * k + 1];
                k15 += WGK[k] * s;
                if k % 2 == 1 {
                    g7 += WG[k / 2] * s;
                }
            }
            (k15 * h, ((k15 - g7) * h).abs())
        })
        .collect())
}

/// Adaptive Gauss–Kronrod quadrature on a finite interval.
fn adapt_gk(
    it: &mut Interp,
    f: &dyn Fn(&mut Interp, &[f64]) -> MResult<Vec<f64>>,
    a: f64,
    b: f64,
    abstol: f64,
    reltol: f64,
) -> MResult<(f64, f64)> {
    if a == b {
        return Ok((0.0, 0.0));
    }
    let mut panels: Vec<(f64, f64)> = (0..10)
        .map(|k| {
            (
                a + (b - a) * k as f64 / 10.0,
                a + (b - a) * (k + 1) as f64 / 10.0,
            )
        })
        .collect();
    let mut done_q = 0.0;
    let mut done_e = 0.0;
    for _ in 0..50 {
        let res = gk_panels(it, f, &panels)?;
        let q: f64 = done_q + res.iter().map(|r| r.0).sum::<f64>();
        let tol = abstol.max(reltol * q.abs());
        let mut next = Vec::new();
        let total_err: f64 = done_e + res.iter().map(|r| r.1).sum::<f64>();
        if total_err <= tol {
            return Ok((q, total_err));
        }
        for (p, r) in panels.iter().zip(&res) {
            let width_share = (p.1 - p.0).abs() / (b - a).abs();
            if r.1 <= tol * width_share {
                done_q += r.0;
                done_e += r.1;
            } else {
                let m = (p.0 + p.1) / 2.0;
                next.push((p.0, m));
                next.push((m, p.1));
            }
        }
        if next.is_empty() || next.len() > 20000 {
            let q = done_q
                + next
                    .iter()
                    .zip(gk_panels(it, f, &next)?)
                    .map(|(_, r)| r.0)
                    .sum::<f64>();
            return Ok((q, done_e));
        }
        panels = next;
    }
    Ok((done_q, done_e))
}

fn integral_fn(it: &mut Interp, a: Vec<Value>, nargout: usize, gk_outputs: bool) -> R {
    if a.len() < 3 {
        return Err(merr!("Not enough input arguments."));
    }
    let fv = a[0].clone();
    let (lo, hi) = (a[1].scalar()?, a[2].scalar()?);
    let mut abstol = 1e-10;
    let mut reltol = 1e-6;
    let mut array_valued = false;
    let mut k = 3;
    while k + 1 < a.len() {
        match a[k].text()?.to_lowercase().as_str() {
            "abstol" => abstol = a[k + 1].scalar()?,
            "reltol" => reltol = a[k + 1].scalar()?,
            "arrayvalued" => array_valued = a[k + 1].is_true()?,
            _ => {}
        }
        k += 2;
    }
    let call = |it: &mut Interp, xs: &[f64]| -> MResult<Vec<f64>> {
        if array_valued {
            xs.iter()
                .map(|x| it.call1(&fv, vec![Value::num(*x)])?.scalar())
                .collect()
        } else {
            Ok(it.call1(&fv, vec![Value::row(xs.to_vec())])?.to_num()?.data)
        }
    };
    let (sign, lo2, hi2) = if lo <= hi {
        (1.0, lo, hi)
    } else {
        (-1.0, hi, lo)
    };
    let (q, err) = if lo2.is_finite() && hi2.is_finite() {
        adapt_gk(it, &call, lo2, hi2, abstol, reltol)?
    } else {
        // Map infinite ranges to (-1, 1) with x = t / (1 - t^2).
        let (tl, th) = (
            if lo2.is_finite() { inv_map(lo2) } else { -1.0 },
            if hi2.is_finite() { inv_map(hi2) } else { 1.0 },
        );
        let g = |it: &mut Interp, ts: &[f64]| -> MResult<Vec<f64>> {
            let xs: Vec<f64> = ts.iter().map(|t| t / (1.0 - t * t)).collect();
            let ys = call(it, &xs)?;
            Ok(ts
                .iter()
                .zip(ys)
                .map(|(t, y)| y * (1.0 + t * t) / ((1.0 - t * t) * (1.0 - t * t)))
                .collect())
        };
        adapt_gk(it, &g, tl, th, abstol, reltol)?
    };
    let mut out = vec![Value::num(sign * q)];
    if nargout > 1 || gk_outputs {
        out.push(Value::num(err));
    }
    Ok(out)
}

/// Inverse of x = t / (1 - t^2) on (-1, 1).
fn inv_map(x: f64) -> f64 {
    if x == 0.0 {
        0.0
    } else {
        (-1.0 + (1.0 + 4.0 * x * x).sqrt()) / (2.0 * x)
    }
}

/// Adaptive Simpson quadrature (quad).
fn quad(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 3 {
        return Err(merr!("Not enough input arguments."));
    }
    let fv = a[0].clone();
    let (lo, hi) = (a[1].scalar()?, a[2].scalar()?);
    let tol = match a.get(3) {
        Some(v) if !v.is_empty() => v.scalar()?,
        _ => 1e-6,
    };
    let call = |it: &mut Interp, xs: &[f64]| -> MResult<Vec<f64>> {
        Ok(it.call1(&fv, vec![Value::row(xs.to_vec())])?.to_num()?.data)
    };
    let (q, _) = adapt_gk(it, &call, lo, hi, tol, 0.0)?;
    Ok(vec![Value::num(q), Value::num(0.0)])
}

fn integral2(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 5 {
        return Err(merr!("Not enough input arguments."));
    }
    let fv = a[0].clone();
    let (xa, xb) = (a[1].scalar()?, a[2].scalar()?);
    let ya = a[3].clone();
    let yb = a[4].clone();
    let bound = |it: &mut Interp, v: &Value, x: f64| -> MResult<f64> {
        match v {
            Value::Func(_) => it.call1(v, vec![Value::num(x)])?.scalar(),
            v => v.scalar(),
        }
    };
    let outer = |it: &mut Interp, xs: &[f64]| -> MResult<Vec<f64>> {
        let mut out = Vec::with_capacity(xs.len());
        for &x in xs {
            let (lo, hi) = (bound(it, &ya, x)?, bound(it, &yb, x)?);
            let inner = |it: &mut Interp, ys: &[f64]| -> MResult<Vec<f64>> {
                let xv = Value::row(vec![x; ys.len()]);
                Ok(it
                    .call1(&fv, vec![xv, Value::row(ys.to_vec())])?
                    .to_num()?
                    .data)
            };
            out.push(adapt_gk(it, &inner, lo, hi, 1e-10, 1e-8)?.0);
        }
        Ok(out)
    };
    let (q, _) = adapt_gk(it, &outer, xa, xb, 1e-10, 1e-6)?;
    one(Value::num(q))
}

// ---------------------------------------------------------------- piecewise polynomials

/// Piecewise polynomial: coefficients per piece in descending powers of (x - break).
pub struct Pp {
    breaks: Vec<f64>,
    coefs: Vec<Vec<f64>>,
}

impl Pp {
    pub fn eval(&self, x: f64) -> f64 {
        if x.is_nan() || self.coefs.is_empty() {
            return f64::NAN;
        }
        let n = self.coefs.len();
        let k = match self.breaks[1..n].iter().position(|b| x < *b) {
            Some(i) => i,
            None => n - 1,
        };
        let dx = x - self.breaks[k];
        self.coefs[k].iter().fold(0.0, |acc, c| acc * dx + c)
    }

    fn to_value(&self) -> Value {
        let pieces = self.coefs.len();
        let order = self.coefs.first().map(|c| c.len()).unwrap_or(0);
        let mut c = Array::filled(pieces, order, 0.0);
        for (i, row) in self.coefs.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                c.set(i, j, *v);
            }
        }
        Value::Struct(StructArr::scalar(vec![
            ("form".into(), Value::chars("pp")),
            ("breaks".into(), Value::row(self.breaks.clone())),
            ("coefs".into(), Value::mat(c)),
            ("pieces".into(), Value::num(pieces as f64)),
            ("order".into(), Value::num(order as f64)),
            ("dim".into(), Value::num(1.0)),
        ]))
    }

    fn from_value(v: &Value) -> MResult<Pp> {
        let Value::Struct(s) = v else {
            return Err(merr!("Expected a piecewise polynomial structure."));
        };
        let breaks = s
            .get(0, "breaks")
            .ok_or_else(|| merr!("Missing breaks."))?
            .to_num()?
            .data;
        let c = s
            .get(0, "coefs")
            .ok_or_else(|| merr!("Missing coefs."))?
            .to_num()?;
        let coefs = (0..c.rows)
            .map(|i| (0..c.cols).map(|j| *c.get(i, j)).collect())
            .collect();
        Ok(Pp { breaks, coefs })
    }
}

/// Cubic spline with not-a-knot end conditions (or clamped slopes when given).
pub fn spline_pp(x: &[f64], y: &[f64], ends: Option<(f64, f64)>) -> Pp {
    let n = x.len();
    if n == 2 && ends.is_none() {
        let s = (y[1] - y[0]) / (x[1] - x[0]);
        return Pp {
            breaks: x.to_vec(),
            coefs: vec![vec![0.0, 0.0, s, y[0]]],
        };
    }
    if n == 3 && ends.is_none() {
        // A single parabola through the three points.
        let (h0, h1) = (x[1] - x[0], x[2] - x[1]);
        let (d0, d1) = ((y[1] - y[0]) / h0, (y[2] - y[1]) / h1);
        let c2 = (d1 - d0) / (h0 + h1);
        let b0 = d0 - c2 * h0;
        let b1 = b0 + 2.0 * c2 * h0;
        return Pp {
            breaks: x.to_vec(),
            coefs: vec![vec![0.0, c2, b0, y[0]], vec![0.0, c2, b1, y[1]]],
        };
    }
    let h: Vec<f64> = (0..n - 1).map(|i| x[i + 1] - x[i]).collect();
    let d: Vec<f64> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / h[i]).collect();
    // Solve for slopes s at the knots.
    let mut m = nalgebra::DMatrix::<f64>::zeros(n, n);
    let mut r = nalgebra::DVector::<f64>::zeros(n);
    for i in 1..n - 1 {
        m[(i, i - 1)] = h[i];
        m[(i, i)] = 2.0 * (h[i - 1] + h[i]);
        m[(i, i + 1)] = h[i - 1];
        r[i] = 3.0 * (h[i] * d[i - 1] + h[i - 1] * d[i]);
    }
    match ends {
        Some((s0, s1)) => {
            m[(0, 0)] = 1.0;
            r[0] = s0;
            m[(n - 1, n - 1)] = 1.0;
            r[n - 1] = s1;
        }
        None => {
            // Not-a-knot: third derivative continuous at x[1] and x[n-2].
            m[(0, 0)] = h[1];
            m[(0, 1)] = h[0] + h[1];
            r[0] =
                ((h[0] + 2.0 * (h[0] + h[1])) * h[1] * d[0] + h[0] * h[0] * d[1]) / (h[0] + h[1]);
            let (a, b) = (h[n - 2], h[n - 3]);
            m[(n - 1, n - 2)] = a + b;
            m[(n - 1, n - 1)] = b;
            r[n - 1] = (a * a * d[n - 3] + (2.0 * (a + b) + a) * b * d[n - 2]) / (a + b);
        }
    }
    let s = m
        .lu()
        .solve(&r)
        .unwrap_or_else(|| nalgebra::DVector::zeros(n));
    hermite(x, y, s.as_slice())
}

/// Shape-preserving piecewise cubic (Fritsch–Carlson slopes, as pchip).
pub fn pchip_pp(x: &[f64], y: &[f64]) -> Pp {
    let n = x.len();
    let h: Vec<f64> = (0..n - 1).map(|i| x[i + 1] - x[i]).collect();
    let d: Vec<f64> = (0..n - 1).map(|i| (y[i + 1] - y[i]) / h[i]).collect();
    let mut s = vec![0.0; n];
    if n == 2 {
        s = vec![d[0], d[0]];
        return hermite(x, y, &s);
    }
    for k in 1..n - 1 {
        if d[k - 1] * d[k] > 0.0 {
            let w1 = 2.0 * h[k] + h[k - 1];
            let w2 = h[k] + 2.0 * h[k - 1];
            s[k] = (w1 + w2) / (w1 / d[k - 1] + w2 / d[k]);
        }
    }
    let end = |h0: f64, h1: f64, d0: f64, d1: f64| -> f64 {
        let mut v = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
        if v.signum() != d0.signum() {
            v = 0.0;
        } else if d0.signum() != d1.signum() && v.abs() > (3.0 * d0).abs() {
            v = 3.0 * d0;
        }
        v
    };
    s[0] = end(h[0], h[1], d[0], d[1]);
    s[n - 1] = end(h[n - 2], h[n - 3], d[n - 2], d[n - 3]);
    hermite(x, y, &s)
}

fn hermite(x: &[f64], y: &[f64], s: &[f64]) -> Pp {
    let n = x.len();
    let coefs = (0..n - 1)
        .map(|i| {
            let h = x[i + 1] - x[i];
            let d = (y[i + 1] - y[i]) / h;
            let c2 = (3.0 * d - 2.0 * s[i] - s[i + 1]) / h;
            let c3 = (s[i] + s[i + 1] - 2.0 * d) / (h * h);
            vec![c3, c2, s[i], y[i]]
        })
        .collect();
    Pp {
        breaks: x.to_vec(),
        coefs,
    }
}

fn pp_fn(a: &[Value], shape_preserving: bool) -> R {
    check_args(a, 2, 3)?;
    let x = a[0].to_num()?.data;
    let yv = a[1].to_num()?.data;
    let mut pts: Vec<(f64, f64)> = Vec::new();
    // spline(x, [s0 y s1]) gives the end slopes.
    let ends = if !shape_preserving && yv.len() == x.len() + 2 {
        Some((yv[0], yv[yv.len() - 1]))
    } else {
        None
    };
    let ys = if ends.is_some() {
        &yv[1..yv.len() - 1]
    } else {
        &yv[..]
    };
    if ys.len() != x.len() {
        return Err(merr!("The number of sites and values must be the same."));
    }
    pts.extend(x.iter().copied().zip(ys.iter().copied()));
    pts.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap_or(std::cmp::Ordering::Equal));
    let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let yy: Vec<f64> = pts.iter().map(|p| p.1).collect();
    if xs.len() < 2 {
        return Err(merr!("At least two data points are needed."));
    }
    let pp = if shape_preserving {
        pchip_pp(&xs, &yy)
    } else {
        spline_pp(&xs, &yy, ends)
    };
    match a.get(2) {
        Some(q) => one(Value::mat(q.to_num()?.map(|v| pp.eval(*v)))),
        None => one(pp.to_value()),
    }
}

// ---------------------------------------------------------------- polynomials

fn polyder(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 2)?;
    let der = |p: &[f64]| -> Vec<f64> {
        let n = p.len();
        if n <= 1 {
            return vec![0.0];
        }
        p[..n - 1]
            .iter()
            .enumerate()
            .map(|(i, c)| c * (n - 1 - i) as f64)
            .collect()
    };
    let conv = |a: &[f64], b: &[f64]| -> Vec<f64> {
        let mut out = vec![0.0; a.len() + b.len() - 1];
        for (i, x) in a.iter().enumerate() {
            for (j, y) in b.iter().enumerate() {
                out[i + j] += x * y;
            }
        }
        out
    };
    let sub = |a: &[f64], b: &[f64]| -> Vec<f64> {
        let n = a.len().max(b.len());
        let mut out = vec![0.0; n];
        for (i, x) in a.iter().enumerate() {
            out[n - a.len() + i] += x;
        }
        for (i, x) in b.iter().enumerate() {
            out[n - b.len() + i] -= x;
        }
        out
    };
    let strip = |mut v: Vec<f64>| {
        while v.len() > 1 && v[0] == 0.0 {
            v.remove(0);
        }
        v
    };
    let p = a[0].to_num()?.data;
    if a.len() == 1 {
        return one(Value::row(strip(der(&p))));
    }
    let q = a[1].to_num()?.data;
    if nargout < 2 {
        // Derivative of the product.
        return one(Value::row(strip({
            let t1 = conv(&der(&p), &q);
            let t2 = conv(&p, &der(&q));
            let n = t1.len().max(t2.len());
            let mut out = vec![0.0; n];
            for (i, x) in t1.iter().enumerate() {
                out[n - t1.len() + i] += x;
            }
            for (i, x) in t2.iter().enumerate() {
                out[n - t2.len() + i] += x;
            }
            out
        })));
    }
    // Quotient p/q: (p' q - p q') / q^2.
    let num = sub(&conv(&der(&p), &q), &conv(&p, &der(&q)));
    Ok(vec![Value::row(strip(num)), Value::row(conv(&q, &q))])
}

fn deconv(b: &[f64], a: &[f64]) -> MResult<(Vec<f64>, Vec<f64>)> {
    let a: Vec<f64> = a.iter().copied().skip_while(|x| *x == 0.0).collect();
    if a.is_empty() {
        return Err(merr!(
            "The first coefficient of the divisor must be nonzero."
        ));
    }
    if b.len() < a.len() {
        return Ok((vec![0.0], b.to_vec()));
    }
    let mut r = b.to_vec();
    let nq = b.len() - a.len() + 1;
    let mut q = vec![0.0; nq];
    for i in 0..nq {
        q[i] = r[i] / a[0];
        for (j, ak) in a.iter().enumerate() {
            r[i + j] -= q[i] * ak;
        }
    }
    for v in r.iter_mut().take(nq) {
        *v = 0.0;
    }
    Ok((q, r))
}

/// Partial fractions `[r, p, k] = residue(b, a)`.
fn residue(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 2, 2)?;
    let b = a[0].to_num()?.data;
    let den = a[1].to_num()?.data;
    let (k, rem) = if b.len() >= den.len() {
        deconv(&b, &den)?
    } else {
        (vec![], b.clone())
    };
    let rem: Vec<f64> = rem.iter().copied().skip_while(|x| *x == 0.0).collect();
    let roots_fn = it.builtins["roots"];
    let p = roots_fn(it, vec![Value::row(den.clone())], 1)?
        .remove(0)
        .to_cplx()?
        .data;
    let lead = den.iter().copied().find(|x| *x != 0.0).unwrap_or(1.0);
    let peval = |c: &[f64], z: C64| c.iter().fold(C64::new(0.0, 0.0), |acc, k| acc * z + k);
    // Group repeated poles.
    let mut groups: Vec<(C64, usize)> = Vec::new();
    for z in &p {
        match groups
            .iter_mut()
            .find(|(q, _)| (q - z).norm() < 1e-6 * z.norm().max(1.0))
        {
            Some(g) => g.1 += 1,
            None => groups.push((*z, 1)),
        }
    }
    let mut rs = Vec::new();
    let mut ps = Vec::new();
    for (pole, mult) in &groups {
        // Residues of a pole of multiplicity m: derivatives of (s-p)^m F(s) at p,
        // estimated on a small circle around the pole.
        let others: Vec<C64> = groups
            .iter()
            .filter(|(q, _)| q != pole)
            .flat_map(|(q, m)| std::iter::repeat_n(*q, *m))
            .collect();
        let g = |s: C64| -> C64 {
            let d: C64 = others
                .iter()
                .fold(C64::new(lead, 0.0), |acc, q| acc * (s - q));
            peval(&rem, s) / d
        };
        let radius = 1e-3 * pole.norm().max(1.0);
        let nodes = 64;
        for j in 1..=*mult {
            // coefficient of (s-p)^(m-j) in g's Taylor series = residue for 1/(s-p)^j
            let order = mult - j;
            let mut acc = C64::new(0.0, 0.0);
            for k in 0..nodes {
                let th = 2.0 * std::f64::consts::PI * k as f64 / nodes as f64;
                let e = C64::new(th.cos(), th.sin());
                acc += g(pole + e * radius) * e.powi(-(order as i32));
            }
            let coef = acc / nodes as f64 / radius.powi(order as i32);
            rs.push(coef);
            ps.push(*pole);
        }
    }
    let clean = |v: Vec<C64>| -> Value {
        Value::complex(Array::col(
            v.into_iter()
                .map(|z| {
                    if z.im.abs() < 1e-10 * z.norm().max(1.0) {
                        C64::new(z.re, 0.0)
                    } else {
                        z
                    }
                })
                .collect(),
        ))
    };
    let k = if k.is_empty() {
        Value::empty()
    } else {
        Value::row(k)
    };
    Ok(vec![clean(rs), clean(ps), k])
}

// ---------------------------------------------------------------- filters

/// Direct form II transposed filter of one signal; returns output and final state.
fn filter_vec(b: &[f64], a: &[f64], x: &[f64], zi: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = b.len().max(a.len());
    let a0 = a[0];
    let bb: Vec<f64> = (0..n)
        .map(|i| b.get(i).copied().unwrap_or(0.0) / a0)
        .collect();
    let aa: Vec<f64> = (0..n)
        .map(|i| a.get(i).copied().unwrap_or(0.0) / a0)
        .collect();
    let mut z = vec![0.0; n];
    for (i, v) in zi.iter().enumerate().take(n - 1) {
        z[i] = *v;
    }
    let mut y = Vec::with_capacity(x.len());
    for &xi in x {
        let yi = bb[0] * xi + z[0];
        for k in 1..n {
            z[k - 1] = bb[k] * xi + z[k] - aa[k] * yi;
        }
        y.push(yi);
    }
    z.truncate(n - 1);
    (y, z)
}

fn filter(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 3, 5)?;
    let b = a[0].to_num()?.data;
    let den = a[1].to_num()?.data;
    if den.first().is_none_or(|v| *v == 0.0) {
        return Err(merr!(
            "The first coefficient of the denominator must be nonzero."
        ));
    }
    let x = a[2].to_num()?;
    let zi = match a.get(3) {
        Some(v) if !v.is_empty() => v.to_num()?.data,
        _ => Vec::new(),
    };
    let along_rows = match a.get(4) {
        Some(v) => v.scalar()? == 2.0,
        None => x.rows == 1,
    };
    let mut out = x.clone();
    let mut zf = Vec::new();
    let lines = if along_rows { x.rows } else { x.cols };
    for l in 0..lines {
        let v: Vec<f64> = if along_rows {
            (0..x.cols).map(|j| *x.get(l, j)).collect()
        } else {
            (0..x.rows).map(|i| *x.get(i, l)).collect()
        };
        let (y, z) = filter_vec(&b, &den, &v, &zi);
        for (k, yv) in y.into_iter().enumerate() {
            if along_rows {
                out.set(l, k, yv);
            } else {
                out.set(k, l, yv);
            }
        }
        zf.extend(z);
    }
    let mut res = vec![Value::mat(out)];
    if nargout > 1 {
        res.push(Value::col(zf));
    }
    Ok(res)
}

/// Zero-phase filtering: forward and backward with reflected edges and matched
/// initial conditions.
fn filtfilt(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 3, 3)?;
    let b = a[0].to_num()?.data;
    let den = a[1].to_num()?.data;
    let x = a[2].to_num()?;
    let n = b.len().max(den.len());
    let nfact = 3 * (n - 1);
    let zi = steady_state(&b, &den);
    let run = |v: &[f64]| -> MResult<Vec<f64>> {
        if v.len() <= nfact {
            return Err(merr!(
                "Data must have length more than 3 times filter order."
            ));
        }
        let (first, last) = (v[0], v[v.len() - 1]);
        let mut ext: Vec<f64> = (1..=nfact).rev().map(|k| 2.0 * first - v[k]).collect();
        ext.extend_from_slice(v);
        ext.extend((1..=nfact).map(|k| 2.0 * last - v[v.len() - 1 - k]));
        let z0: Vec<f64> = zi.iter().map(|z| z * ext[0]).collect();
        let (mut y, _) = filter_vec(&b, &den, &ext, &z0);
        y.reverse();
        let z1: Vec<f64> = zi.iter().map(|z| z * y[0]).collect();
        let (mut y2, _) = filter_vec(&b, &den, &y, &z1);
        y2.reverse();
        Ok(y2[nfact..nfact + v.len()].to_vec())
    };
    let mut out = x.clone();
    if x.rows == 1 || x.cols == 1 {
        let y = run(&x.data)?;
        out.data = y;
    } else {
        for j in 0..x.cols {
            let col: Vec<f64> = (0..x.rows).map(|i| *x.get(i, j)).collect();
            for (i, v) in run(&col)?.into_iter().enumerate() {
                out.set(i, j, v);
            }
        }
    }
    one(Value::mat(out))
}

/// Initial state giving a unit-step steady state (lfilter_zi).
fn steady_state(b: &[f64], a: &[f64]) -> Vec<f64> {
    let n = b.len().max(a.len());
    if n < 2 {
        return Vec::new();
    }
    let a0 = a[0];
    let bb: Vec<f64> = (0..n)
        .map(|i| b.get(i).copied().unwrap_or(0.0) / a0)
        .collect();
    let aa: Vec<f64> = (0..n)
        .map(|i| a.get(i).copied().unwrap_or(0.0) / a0)
        .collect();
    let m = n - 1;
    let mut lhs = nalgebra::DMatrix::<f64>::identity(m, m);
    for i in 0..m {
        lhs[(i, 0)] += aa[i + 1];
    }
    for i in 0..m - 1 {
        lhs[(i, i + 1)] -= 1.0;
    }
    let rhs = nalgebra::DVector::from_iterator(m, (0..m).map(|i| bb[i + 1] - aa[i + 1] * bb[0]));
    lhs.lu()
        .solve(&rhs)
        .map(|v| v.iter().copied().collect())
        .unwrap_or_else(|| vec![0.0; m])
}

/// Butterworth design (digital by bilinear transform with prewarping, or analog with 's').
fn butter(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 2, 4)?;
    let n = a[0].scalar()? as usize;
    let wn = a[1].to_num()?.data;
    let mut kind = if wn.len() == 2 {
        "bandpass".to_string()
    } else {
        "low".to_string()
    };
    let mut analog = false;
    for v in &a[2..] {
        let t = v.text()?;
        if t == "s" {
            analog = true;
        } else {
            kind = t;
        }
    }
    // Prewarp to analog frequencies.
    let fs = 2.0;
    let warp = |w: f64| {
        if analog {
            w
        } else {
            2.0 * fs * (std::f64::consts::PI * w / fs).tan()
        }
    };
    let w: Vec<f64> = wn.iter().map(|x| warp(*x)).collect();
    let proto: Vec<C64> = (0..n)
        .map(|k| {
            let th = std::f64::consts::PI * (2 * k + n + 1) as f64 / (2 * n) as f64;
            C64::new(th.cos(), th.sin())
        })
        .collect();
    let (mut z, mut p): (Vec<C64>, Vec<C64>) = (Vec::new(), Vec::new());
    let mut k: f64;
    match kind.as_str() {
        "low" => {
            p = proto.iter().map(|q| q * w[0]).collect();
            k = w[0].powi(n as i32);
        }
        "high" => {
            p = proto.iter().map(|q| C64::new(w[0], 0.0) / q).collect();
            z = vec![C64::new(0.0, 0.0); n];
            k = 1.0;
        }
        "bandpass" | "stop" => {
            let (w1, w2) = (w[0], w[1]);
            let bw = w2 - w1;
            let w0 = (w1 * w2).sqrt();
            for q in &proto {
                let base = if kind == "bandpass" {
                    q * (bw / 2.0)
                } else {
                    C64::new(bw / 2.0, 0.0) / q
                };
                let disc = (base * base - w0 * w0).sqrt();
                p.push(base + disc);
                p.push(base - disc);
            }
            if kind == "bandpass" {
                z = vec![C64::new(0.0, 0.0); n];
                k = bw.powi(n as i32);
            } else {
                for _ in 0..n {
                    z.push(C64::new(0.0, w0));
                    z.push(C64::new(0.0, -w0));
                }
                k = 1.0;
            }
        }
        other => return Err(merr!("Unknown filter type '{}'.", other)),
    }
    if !analog {
        // Bilinear transform: s = 2 fs (z - 1)/(z + 1).
        let c2 = 2.0 * fs;
        let num_gain: C64 = z.iter().fold(C64::new(1.0, 0.0), |acc, q| acc * (c2 - q));
        let den_gain: C64 = p.iter().fold(C64::new(1.0, 0.0), |acc, q| acc * (c2 - q));
        k *= (num_gain / den_gain).re;
        let zd: Vec<C64> = z.iter().map(|q| (c2 + q) / (c2 - q)).collect();
        let pd: Vec<C64> = p.iter().map(|q| (c2 + q) / (c2 - q)).collect();
        let mut zd = zd;
        zd.extend(std::iter::repeat_n(C64::new(-1.0, 0.0), pd.len() - z.len()));
        z = zd;
        p = pd;
    }
    if nargout == 3 {
        return Ok(vec![
            Value::complex(Array::col(z)),
            Value::complex(Array::col(p)),
            Value::num(k),
        ]);
    }
    let poly = |r: &[C64]| -> Vec<f64> {
        let mut c = vec![C64::new(1.0, 0.0)];
        for x in r {
            let mut q = vec![C64::new(0.0, 0.0); c.len() + 1];
            for (i, v) in c.iter().enumerate() {
                q[i] += v;
                q[i + 1] -= v * x;
            }
            c = q;
        }
        c.iter().map(|v| v.re).collect()
    };
    let bnum: Vec<f64> = {
        let mut v = poly(&z).iter().map(|x| x * k).collect::<Vec<f64>>();
        let dlen = p.len() + 1;
        if v.len() < dlen {
            let mut padded = vec![0.0; dlen - v.len()];
            padded.extend(v);
            v = padded;
        }
        v
    };
    Ok(vec![Value::row(bnum), Value::row(poly(&p))])
}

fn freqz(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 4)?;
    let b = a[0].to_num()?.data;
    let den = match a.get(1) {
        Some(v) if !v.is_empty() => v.to_num()?.data,
        _ => vec![1.0],
    };
    let fs = a.get(3).map(|v| v.scalar()).transpose()?;
    let w: Vec<f64> = match a.get(2) {
        Some(v) if v.numel() > 1 => {
            let f = v.to_num()?.data;
            match fs {
                Some(fs) => f
                    .iter()
                    .map(|x| 2.0 * std::f64::consts::PI * x / fs)
                    .collect(),
                None => f,
            }
        }
        Some(v) => {
            let n = v.scalar()? as usize;
            (0..n)
                .map(|k| std::f64::consts::PI * k as f64 / n as f64)
                .collect()
        }
        None => (0..512)
            .map(|k| std::f64::consts::PI * k as f64 / 512.0)
            .collect(),
    };
    let ev = |c: &[f64], z: C64| {
        c.iter()
            .enumerate()
            .fold(C64::new(0.0, 0.0), |acc, (k, v)| {
                acc + v * z.powi(-(k as i32))
            })
    };
    let h: Vec<C64> = w
        .iter()
        .map(|wi| {
            let z = C64::new(0.0, *wi).exp();
            ev(&b, z) / ev(&den, z)
        })
        .collect();
    let wout: Vec<f64> = match fs {
        Some(fs) => w
            .iter()
            .map(|x| x * fs / (2.0 * std::f64::consts::PI))
            .collect(),
        None => w.clone(),
    };
    if nargout > 0 {
        return Ok(vec![Value::complex(Array::col(h)), Value::col(wout)]);
    }
    let xs: Vec<f64> = match fs {
        Some(_) => wout.clone(),
        None => w.iter().map(|x| x / std::f64::consts::PI).collect(),
    };
    let xl = if fs.is_some() {
        "Frequency (Hz)"
    } else {
        "Normalized Frequency (\\times\\pi rad/sample)"
    };
    it.call_named(
        "subplot",
        vec![Value::num(2.0), Value::num(1.0), Value::num(1.0)],
        0,
    )?;
    it.call_named(
        "plot",
        vec![
            Value::row(xs.clone()),
            Value::row(h.iter().map(|z| 20.0 * z.norm().log10()).collect()),
        ],
        0,
    )?;
    {
        let ax = it.graphics.current_axes();
        ax.ylabel = Some("Magnitude (dB)".into());
        ax.xlabel = Some(xl.into());
        ax.grid_on = true;
    }
    it.call_named(
        "subplot",
        vec![Value::num(2.0), Value::num(1.0), Value::num(2.0)],
        0,
    )?;
    let mut ph: Vec<f64> = Vec::new();
    for z in &h {
        let p = z.arg().to_degrees();
        let q = match ph.last() {
            Some(prev) => {
                let mut q = p;
                while q - prev > 180.0 {
                    q -= 360.0;
                }
                while q - prev < -180.0 {
                    q += 360.0;
                }
                q
            }
            None => p,
        };
        ph.push(q);
    }
    it.call_named("plot", vec![Value::row(xs), Value::row(ph)], 0)?;
    let ax = it.graphics.current_axes();
    ax.ylabel = Some("Phase (degrees)".into());
    ax.xlabel = Some(xl.into());
    ax.grid_on = true;
    none()
}

fn xcorr(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 4)?;
    let x = a[0].to_num()?.data;
    let (y, rest) = match a.get(1) {
        Some(v) if !v.is_text() && v.numel() > 1 => (v.to_num()?.data, &a[2..]),
        _ => (x.clone(), &a[1..]),
    };
    let n = x.len().max(y.len());
    let mut maxlag = n - 1;
    let mut scale = String::from("none");
    for v in rest {
        if v.is_text() {
            scale = v.text()?;
        } else {
            maxlag = v.scalar()? as usize;
        }
    }
    let lags: Vec<i64> = (-(maxlag as i64)..=maxlag as i64).collect();
    let mut r: Vec<f64> = lags
        .iter()
        .map(|&l| {
            (0..n)
                .filter_map(|k| {
                    let j = k as i64 - l;
                    if j < 0 {
                        return None;
                    }
                    Some(
                        x.get(k).copied().unwrap_or(0.0)
                            * y.get(j as usize).copied().unwrap_or(0.0),
                    )
                })
                .sum()
        })
        .collect();
    match scale.as_str() {
        "biased" => r.iter_mut().for_each(|v| *v /= n as f64),
        "unbiased" => r
            .iter_mut()
            .zip(&lags)
            .for_each(|(v, l)| *v /= (n as i64 - l.abs()) as f64),
        "normalized" | "coeff" => {
            let ex: f64 = x.iter().map(|v| v * v).sum();
            let ey: f64 = y.iter().map(|v| v * v).sum();
            let d = (ex * ey).sqrt();
            r.iter_mut().for_each(|v| *v /= d);
        }
        _ => {}
    }
    let col = a[0].dims().1 == 1;
    let r = if col { Value::col(r) } else { Value::row(r) };
    let lags = Value::row(lags.iter().map(|l| *l as f64).collect());
    Ok(vec![r, lags])
}

fn accumarray(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 2, 5)?;
    let subs = a[0].to_num()?;
    let vals = a[1].to_num()?;
    let two_d = subs.cols == 2 && subs.rows != 1;
    let n = if two_d { subs.rows } else { subs.numel() };
    let idx: Vec<(usize, usize)> = (0..n)
        .map(|k| {
            if two_d {
                (*subs.get(k, 0) as usize - 1, *subs.get(k, 1) as usize - 1)
            } else {
                (subs.data[k] as usize - 1, 0)
            }
        })
        .collect();
    let (mut rows, mut cols) = (
        idx.iter().map(|p| p.0 + 1).max().unwrap_or(0),
        idx.iter().map(|p| p.1 + 1).max().unwrap_or(1),
    );
    if let Some(sz) = a.get(2).filter(|v| !v.is_empty()) {
        let s = sz.to_num()?.data;
        rows = rows.max(s[0] as usize);
        cols = cols.max(*s.get(1).unwrap_or(&1.0) as usize);
    }
    let fun = a.get(3).filter(|v| !v.is_empty()).cloned();
    let fill = a.get(4).map(|v| v.scalar()).transpose()?.unwrap_or(0.0);
    let mut groups: Vec<Vec<f64>> = vec![Vec::new(); rows * cols];
    for (k, (r, c)) in idx.iter().enumerate() {
        let v = if vals.numel() == 1 {
            vals.data[0]
        } else {
            vals.data[k]
        };
        groups[c * rows + r].push(v);
    }
    let mut out = Array::filled(rows, cols, fill);
    for (k, g) in groups.iter().enumerate() {
        if g.is_empty() {
            continue;
        }
        out.data[k] = match &fun {
            None => g.iter().sum(),
            Some(f) => it.call1(f, vec![Value::col(g.clone())])?.scalar()?,
        };
    }
    one(Value::mat(out))
}
