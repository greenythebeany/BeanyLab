//! Elementwise math, reductions, linear algebra, polynomials, interpolation and statistics.

use super::*;
use crate::ops::{self, from_dmatrix, to_dmatrix};

macro_rules! elementwise {
    ($reg:ident; keep: $($kname:literal => $kf:expr),* ; float: $($fname:literal => $ff:expr),* $(,)?) => {
        fn $reg(it: &mut Interp) {
            $(
                it.builtins.insert($kname, |_, a, _| {
                    check_args(&a, 1, 1)?;
                    let f: fn(f64) -> f64 = $kf;
                    let class = match a[0].num_class() { NumClass::Logical => NumClass::Double, c => c };
                    let m = a[0].to_num()?.map(|x| f(*x));
                    one(match class { NumClass::Int(k) => Value::Num(m.map(|x| k.cast(*x)), class), c => Value::Num(m, c) })
                });
            )*
            $(
                it.builtins.insert($fname, |_, a, _| {
                    check_args(&a, 1, 1)?;
                    let f: fn(f64) -> f64 = $ff;
                    one(Value::mat(a[0].to_num()?.map(|x| f(*x))))
                });
            )*
        }
    };
}

elementwise! { register_elementwise;
    keep:
        "abs" => |x| x.abs(),
        "floor" => |x| x.floor(),
        "ceil" => |x| x.ceil(),
        "fix" => |x| x.trunc(),
        "sign" => |x| if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else if x == 0.0 { 0.0 } else { f64::NAN },
        "real" => |x| x,
        "conj" => |x| x;
    float:
        "sqrt" => |x| x.sqrt(),
        "exp" => |x| x.exp(),
        "log" => |x| x.ln(),
        "log2" => |x| x.log2(),
        "log10" => |x| x.log10(),
        "log1p" => |x| x.ln_1p(),
        "expm1" => |x| x.exp_m1(),
        "sin" => |x| x.sin(),
        "cos" => |x| x.cos(),
        "tan" => |x| x.tan(),
        "asin" => |x| x.asin(),
        "acos" => |x| x.acos(),
        "atan" => |x| x.atan(),
        "sinh" => |x| x.sinh(),
        "cosh" => |x| x.cosh(),
        "tanh" => |x| x.tanh(),
        "asinh" => |x| x.asinh(),
        "acosh" => |x| x.acosh(),
        "atanh" => |x| x.atanh(),
        "sec" => |x| 1.0 / x.cos(),
        "csc" => |x| 1.0 / x.sin(),
        "cot" => |x| 1.0 / x.tan(),
        "sind" => |x| sind(x),
        "cosd" => |x| sind(x + 90.0),
        "tand" => |x| sind(x) / sind(x + 90.0),
        "asind" => |x| x.asin().to_degrees(),
        "acosd" => |x| x.acos().to_degrees(),
        "atand" => |x| x.atan().to_degrees(),
        "deg2rad" => |x| x.to_radians(),
        "rad2deg" => |x| x.to_degrees(),
        "gamma" => |x| libm::tgamma(x),
        "gammaln" => |x| libm::lgamma(x),
        "erf" => |x| libm::erf(x),
        "erfc" => |x| libm::erfc(x),
        "cbrt" => |x| x.cbrt(),
        "imag" => |_| 0.0,
        "angle" => |x| if x < 0.0 { std::f64::consts::PI } else { 0.0 },
        "isprime" => |x| if is_prime(x) { 1.0 } else { 0.0 },
        "factorial" => |x| if x < 0.0 || x.fract() != 0.0 { f64::NAN } else { libm::tgamma(x + 1.0).round() },
}

fn sind(x: f64) -> f64 {
    // Exact zeros at multiples of 180 degrees.
    let r = x.rem_euclid(360.0);
    if r == 0.0 || r == 180.0 {
        0.0
    } else if r == 90.0 {
        1.0
    } else if r == 270.0 {
        -1.0
    } else {
        x.to_radians().sin()
    }
}

fn is_prime(x: f64) -> bool {
    if x < 2.0 || x.fract() != 0.0 {
        return false;
    }
    let n = x as u64;
    if n < 4 {
        return true;
    }
    if n.is_multiple_of(2) {
        return false;
    }
    let mut i = 3;
    while i * i <= n {
        if n.is_multiple_of(i) {
            return false;
        }
        i += 2;
    }
    true
}

fn binary_num(a: &[Value], name: &str, f: fn(f64, f64) -> f64) -> R {
    check_args(a, 2, 2)?;
    let x = a[0].to_num()?;
    let y = a[1].to_num()?;
    let out = ops::broadcast(&x, &y, f, name)?;
    let class = match (a[0].num_class(), a[1].num_class()) {
        (NumClass::Int(k), _) | (_, NumClass::Int(k)) => NumClass::Int(k),
        _ => NumClass::Double,
    };
    one(match class {
        NumClass::Int(k) => Value::Num(out.map(|v| k.cast(*v)), class),
        _ => Value::mat(out),
    })
}

pub fn register(it: &mut Interp) {
    register_elementwise(it);
    let b = &mut it.builtins;
    b.insert("round", round);
    b.insert("mod", |_, a, _| {
        binary_num(&a, "mod", |x, y| {
            if y == 0.0 {
                x
            } else {
                let r = x % y;
                if r != 0.0 && (r < 0.0) != (y < 0.0) {
                    r + y
                } else {
                    r
                }
            }
        })
    });
    b.insert("rem", |_, a, _| {
        binary_num(&a, "rem", |x, y| if y == 0.0 { f64::NAN } else { x % y })
    });
    b.insert("atan2", |_, a, _| {
        binary_num(&a, "atan2", |y, x| y.atan2(x))
    });
    b.insert("atan2d", |_, a, _| {
        binary_num(&a, "atan2d", |y, x| y.atan2(x).to_degrees())
    });
    b.insert("hypot", |_, a, _| {
        binary_num(&a, "hypot", |x, y| x.hypot(y))
    });
    b.insert("power", |_, a, _| binary_num(&a, "power", |x, y| x.powf(y)));
    b.insert("nthroot", |_, a, _| {
        binary_num(&a, "nthroot", |x, n| {
            if x < 0.0 && n % 2.0 != 0.0 {
                -(-x).powf(1.0 / n)
            } else {
                x.powf(1.0 / n)
            }
        })
    });
    b.insert("gcd", |_, a, _| binary_num(&a, "gcd", gcd));
    b.insert("lcm", |_, a, _| {
        binary_num(&a, "lcm", |x, y| {
            if x == 0.0 || y == 0.0 {
                0.0
            } else {
                (x * y).abs() / gcd(x, y)
            }
        })
    });
    b.insert("plus", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::Add, &a[0], &a[1])?)
    });
    b.insert("minus", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::Sub, &a[0], &a[1])?)
    });
    b.insert("times", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::ElMul, &a[0], &a[1])?)
    });
    b.insert("rdivide", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::ElDiv, &a[0], &a[1])?)
    });
    b.insert("mtimes", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::MatMul, &a[0], &a[1])?)
    });
    b.insert("mldivide", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::MatLDiv, &a[0], &a[1])?)
    });
    b.insert("mrdivide", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::MatDiv, &a[0], &a[1])?)
    });
    b.insert("mpower", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::MatPow, &a[0], &a[1])?)
    });
    b.insert("uminus", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(ops::unary(crate::ast::UnOp::Neg, &a[0])?)
    });
    b.insert("eq", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::Eq, &a[0], &a[1])?)
    });
    b.insert("ne", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::Ne, &a[0], &a[1])?)
    });
    b.insert("lt", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::Lt, &a[0], &a[1])?)
    });
    b.insert("le", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::Le, &a[0], &a[1])?)
    });
    b.insert("gt", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::Gt, &a[0], &a[1])?)
    });
    b.insert("ge", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::Ge, &a[0], &a[1])?)
    });
    b.insert("nchoosek", nchoosek);
    b.insert("primes", |_, a, _| {
        check_args(&a, 1, 1)?;
        let n = a[0].scalar()?;
        one(Value::row(
            (2..=(n.max(0.0) as u64))
                .map(|k| k as f64)
                .filter(|k| is_prime(*k))
                .collect(),
        ))
    });
    b.insert("factor", |_, a, _| {
        check_args(&a, 1, 1)?;
        let mut n = a[0].scalar()? as u64;
        let mut f = Vec::new();
        let mut p = 2;
        while n > 1 && p * p <= n {
            while n.is_multiple_of(p) {
                f.push(p as f64);
                n /= p;
            }
            p += 1;
        }
        if n > 1 {
            f.push(n as f64);
        }
        one(Value::row(f))
    });
    b.insert("sum", |_, a, _| reduction(&a, |s| s.iter().sum(), true));
    b.insert("prod", |_, a, _| {
        reduction(&a, |s| s.iter().product(), true)
    });
    b.insert("mean", |_, a, _| {
        reduction(
            &a,
            |s| {
                if s.is_empty() {
                    f64::NAN
                } else {
                    s.iter().sum::<f64>() / s.len() as f64
                }
            },
            false,
        )
    });
    b.insert("median", |_, a, _| reduction(&a, median, false));
    b.insert("mode", |_, a, _| reduction(&a, mode, false));
    b.insert("var", |_, a, _| var_std(&a, false));
    b.insert("std", |_, a, _| var_std(&a, true));
    b.insert("max", |_, a, n| minmax(&a, n, true));
    b.insert("min", |_, a, n| minmax(&a, n, false));
    b.insert("bounds", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = a[0].to_num()?;
        let lo = m
            .data
            .iter()
            .copied()
            .filter(|x| !x.is_nan())
            .fold(f64::INFINITY, f64::min);
        let hi = m
            .data
            .iter()
            .copied()
            .filter(|x| !x.is_nan())
            .fold(f64::NEG_INFINITY, f64::max);
        Ok(vec![Value::num(lo), Value::num(hi)])
    });
    b.insert("cumsum", |_, a, _| {
        cumulative(&a, |s| {
            s.iter()
                .scan(0.0, |acc, x| {
                    *acc += x;
                    Some(*acc)
                })
                .collect()
        })
    });
    b.insert("cumprod", |_, a, _| {
        cumulative(&a, |s| {
            s.iter()
                .scan(1.0, |acc, x| {
                    *acc *= x;
                    Some(*acc)
                })
                .collect()
        })
    });
    b.insert("cummax", |_, a, _| {
        cumulative(&a, |s| {
            s.iter()
                .scan(f64::NEG_INFINITY, |acc, x| {
                    *acc = acc.max(*x);
                    Some(*acc)
                })
                .collect()
        })
    });
    b.insert("cummin", |_, a, _| {
        cumulative(&a, |s| {
            s.iter()
                .scan(f64::INFINITY, |acc, x| {
                    *acc = acc.min(*x);
                    Some(*acc)
                })
                .collect()
        })
    });
    b.insert("diff", diff);
    b.insert("dot", |_, a, _| {
        check_args(&a, 2, 2)?;
        let x = a[0].to_num()?;
        let y = a[1].to_num()?;
        if x.numel() != y.numel() {
            return Err(merr!("A and B must be the same size."));
        }
        one(Value::num(
            x.data.iter().zip(&y.data).map(|(p, q)| p * q).sum(),
        ))
    });
    b.insert("cross", |_, a, _| {
        check_args(&a, 2, 2)?;
        let x = a[0].to_num()?;
        let y = a[1].to_num()?;
        if x.numel() != 3 || y.numel() != 3 {
            return Err(merr!(
                "A and B must be of length 3 in the dimension in which the cross product is taken."
            ));
        }
        let (u, v) = (&x.data, &y.data);
        let r = vec![
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        one(if x.rows == 1 {
            Value::row(r)
        } else {
            Value::col(r)
        })
    });
    b.insert("kron", |_, a, _| {
        check_args(&a, 2, 2)?;
        let x = a[0].to_num()?;
        let y = a[1].to_num()?;
        let mut out = Array::filled(x.rows * y.rows, x.cols * y.cols, 0.0);
        for i in 0..x.rows {
            for j in 0..x.cols {
                for k in 0..y.rows {
                    for l in 0..y.cols {
                        out.set(i * y.rows + k, j * y.cols + l, x.get(i, j) * y.get(k, l));
                    }
                }
            }
        }
        one(Value::mat(out))
    });
    b.insert("trace", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = a[0].to_num()?;
        one(Value::num(
            (0..m.rows.min(m.cols)).map(|i| *m.get(i, i)).sum(),
        ))
    });
    b.insert("det", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = a[0].to_num()?;
        if m.rows != m.cols {
            return Err(merr!("Matrix must be square."));
        }
        one(Value::num(if m.is_empty() {
            1.0
        } else {
            to_dmatrix(&m).determinant()
        }))
    });
    b.insert("inv", |it, a, _| {
        check_args(&a, 1, 1)?;
        let m = a[0].to_num()?;
        let r = ops::inv(&m)?;
        if r.data.iter().any(|x| x.is_infinite()) {
            it.warn("Matrix is singular to working precision.");
        }
        one(Value::mat(r))
    });
    b.insert("pinv", |_, a, _| {
        check_args(&a, 1, 2)?;
        let m = to_dmatrix(&a[0].to_num()?);
        let tol = if a.len() > 1 {
            a[1].scalar()?
        } else {
            f64::EPSILON
                * m.nrows().max(m.ncols()) as f64
                * m.clone().svd(false, false).singular_values.max()
        };
        let p = m.pseudo_inverse(tol).map_err(|e| merr!("{}", e))?;
        one(Value::mat(from_dmatrix(&p)))
    });
    b.insert("rank", |_, a, _| {
        check_args(&a, 1, 2)?;
        let m = to_dmatrix(&a[0].to_num()?);
        if m.is_empty() {
            return one(Value::num(0.0));
        }
        let s = m.clone().svd(false, false).singular_values;
        let tol = if a.len() > 1 {
            a[1].scalar()?
        } else {
            m.nrows().max(m.ncols()) as f64 * s.max() * f64::EPSILON
        };
        one(Value::num(s.iter().filter(|x| **x > tol).count() as f64))
    });
    b.insert("norm", norm);
    b.insert("cond", |_, a, _| {
        check_args(&a, 1, 1)?;
        let s = to_dmatrix(&a[0].to_num()?)
            .svd(false, false)
            .singular_values;
        one(Value::num(s.max() / s.min()))
    });
    b.insert("eig", eig);
    b.insert("svd", svd);
    b.insert("lu", lu);
    b.insert("qr", qr);
    b.insert("chol", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = to_dmatrix(&a[0].to_num()?);
        match m.cholesky() {
            Some(c) => one(Value::mat(from_dmatrix(&c.l().transpose()))),
            None => Err(merr!("Matrix must be positive definite.")),
        }
    });
    b.insert("expm", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = to_dmatrix(&a[0].to_num()?);
        one(Value::mat(from_dmatrix(&m.exp())))
    });
    b.insert("null", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = to_dmatrix(&a[0].to_num()?);
        let (r, c) = (m.nrows(), m.ncols());
        let svd = m.svd(false, true);
        let v_t = svd.v_t.unwrap();
        let s = &svd.singular_values;
        let tol = r.max(c) as f64 * s.max() * f64::EPSILON;
        let rank = s.iter().filter(|x| **x > tol).count();
        let v = v_t.transpose();
        let cols: Vec<f64> = (rank..c)
            .flat_map(|j| v.column(j).iter().copied().collect::<Vec<_>>())
            .collect();
        one(Value::mat(Array::new(c, c - rank, cols)))
    });
    b.insert("polyfit", polyfit);
    b.insert("polyval", polyval);
    b.insert("roots", roots);
    b.insert("poly", |_, a, _| {
        check_args(&a, 1, 1)?;
        let r = a[0].to_num()?;
        let mut p = vec![1.0];
        for x in &r.data {
            let mut q = vec![0.0; p.len() + 1];
            for (i, c) in p.iter().enumerate() {
                q[i] += c;
                q[i + 1] -= c * x;
            }
            p = q;
        }
        one(Value::row(p))
    });
    b.insert("conv", |_, a, _| {
        check_args(&a, 2, 3)?;
        let x = a[0].to_num()?;
        let y = a[1].to_num()?;
        if x.is_empty() || y.is_empty() {
            return one(Value::empty());
        }
        let mut out = vec![0.0; x.numel() + y.numel() - 1];
        for (i, p) in x.data.iter().enumerate() {
            for (j, q) in y.data.iter().enumerate() {
                out[i + j] += p * q;
            }
        }
        if a.len() == 3 && a[2].text()? == "same" {
            let start = (y.numel() - 1) / 2 + if y.numel() % 2 == 0 { 1 } else { 0 };
            let start = start.min(out.len());
            out = out[start..(start + x.numel()).min(out.len())].to_vec();
        }
        one(if x.cols == 1 && x.rows > 1 {
            Value::col(out)
        } else {
            Value::row(out)
        })
    });
    b.insert("deconv", |_, a, _| {
        check_args(&a, 2, 2)?;
        let mut num = a[0].to_num()?.data;
        let den = a[1].to_num()?.data;
        if den.is_empty() || den[0] == 0.0 {
            return Err(merr!("First coefficient of the divisor must be nonzero."));
        }
        if num.len() < den.len() {
            return Ok(vec![Value::num(0.0), Value::row(num)]);
        }
        let n = num.len() - den.len() + 1;
        let mut q = vec![0.0; n];
        for i in 0..n {
            q[i] = num[i] / den[0];
            for (j, d) in den.iter().enumerate() {
                num[i + j] -= q[i] * d;
            }
        }
        Ok(vec![Value::row(q), Value::row(num)])
    });
    b.insert("trapz", |_, a, _| {
        check_args(&a, 1, 2)?;
        let (x, y) = if a.len() == 2 {
            (Some(a[0].to_num()?), a[1].to_num()?)
        } else {
            (None, a[0].to_num()?)
        };
        let integrate = |ys: &[f64]| -> f64 {
            (1..ys.len())
                .map(|i| {
                    let dx = x.as_ref().map_or(1.0, |x| x.data[i] - x.data[i - 1]);
                    dx * (ys[i] + ys[i - 1]) / 2.0
                })
                .sum()
        };
        if y.is_vector() {
            return one(Value::num(integrate(&y.data)));
        }
        one(Value::mat(reduce(&y, 1, integrate)))
    });
    b.insert("cumtrapz", |_, a, _| {
        check_args(&a, 1, 2)?;
        let (x, y) = if a.len() == 2 {
            (Some(a[0].to_num()?), a[1].to_num()?)
        } else {
            (None, a[0].to_num()?)
        };
        let mut acc = 0.0;
        let mut out = vec![0.0];
        for i in 1..y.numel() {
            let dx = x.as_ref().map_or(1.0, |x| x.data[i] - x.data[i - 1]);
            acc += dx * (y.data[i] + y.data[i - 1]) / 2.0;
            out.push(acc);
        }
        one(if y.rows == 1 {
            Value::row(out)
        } else {
            Value::col(out)
        })
    });
    b.insert("interp1", interp1);
    b.insert("histcounts", histcounts);
    b.insert("cov", |_, a, _| {
        check_args(&a, 1, 2)?;
        let m = if a.len() == 2 {
            let x = a[0].to_num()?;
            let y = a[1].to_num()?;
            Array::new(
                x.numel(),
                2,
                x.data.iter().chain(y.data.iter()).copied().collect(),
            )
        } else {
            let x = a[0].to_num()?;
            if x.rows == 1 {
                x.transpose()
            } else {
                x
            }
        };
        one(Value::mat(covariance(&m)))
    });
    b.insert("corrcoef", |_, a, _| {
        check_args(&a, 1, 2)?;
        let m = if a.len() == 2 {
            let x = a[0].to_num()?;
            let y = a[1].to_num()?;
            Array::new(
                x.numel(),
                2,
                x.data.iter().chain(y.data.iter()).copied().collect(),
            )
        } else {
            a[0].to_num()?
        };
        let c = covariance(&m);
        let mut r = c.clone();
        for i in 0..c.rows {
            for j in 0..c.cols {
                r.set(i, j, c.get(i, j) / (c.get(i, i) * c.get(j, j)).sqrt());
            }
        }
        one(Value::mat(r))
    });
}

fn gcd(a: f64, b: f64) -> f64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b > 0.0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

fn round(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 2)?;
    let digits = if a.len() > 1 { a[1].scalar()? } else { 0.0 };
    let f = 10f64.powf(digits);
    let class = match a[0].num_class() {
        NumClass::Logical => NumClass::Double,
        c => c,
    };
    let m = a[0].to_num()?.map(|x| {
        if digits == 0.0 {
            x.round()
        } else {
            (x * f).round() / f
        }
    });
    one(Value::Num(m, class))
}

fn nchoosek(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 2, 2)?;
    let v = a[0].to_num()?;
    let k = a[1].scalar()? as usize;
    if v.numel() == 1 {
        let n = v.data[0];
        let mut r = 1.0;
        for i in 0..k {
            r = r * (n - i as f64) / (i as f64 + 1.0);
        }
        return one(Value::num(r.round()));
    }
    // All combinations of the vector elements.
    let n = v.numel();
    let mut rows: Vec<Vec<f64>> = Vec::new();
    if k <= n {
        let mut idx: Vec<usize> = (0..k).collect();
        loop {
            rows.push(idx.iter().map(|&i| v.data[i]).collect());
            // Advance the rightmost index that can still move.
            let Some(i) = (0..k).rev().find(|&i| idx[i] < n - k + i) else {
                break;
            };
            idx[i] += 1;
            for j in i + 1..k {
                idx[j] = idx[j - 1] + 1;
            }
        }
    }
    let r = rows.len();
    let mut out = Array::filled(r, k, 0.0);
    for (i, row) in rows.iter().enumerate() {
        for (j, x) in row.iter().enumerate() {
            out.set(i, j, *x);
        }
    }
    one(Value::mat(out))
}

fn reduction(a: &[Value], f: fn(&[f64]) -> f64, keep_int: bool) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let m = a[0].to_num()?;
    let mut dim = None;
    let mut omit_nan = false;
    let mut all = false;
    for x in &a[1..] {
        if x.is_text() {
            match x.text()?.as_str() {
                "omitnan" => omit_nan = true,
                "all" => all = true,
                _ => {}
            }
        } else {
            dim = Some(x.scalar()?);
        }
    }
    let g = |s: &[f64]| -> f64 {
        if omit_nan {
            let v: Vec<f64> = s.iter().copied().filter(|x| !x.is_nan()).collect();
            f(&v)
        } else {
            f(s)
        }
    };
    let out = if all {
        Array::scalar(g(&m.data))
    } else {
        reduce(&m, pick_dim(&m, dim)?, g)
    };
    one(match a[0].num_class() {
        NumClass::Int(k) if keep_int => Value::Num(out.map(|x| k.cast(*x)), NumClass::Int(k)),
        _ => Value::mat(out),
    })
}

fn median(s: &[f64]) -> f64 {
    if s.is_empty() || s.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    let mut v = s.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn mode(s: &[f64]) -> f64 {
    if s.is_empty() {
        return f64::NAN;
    }
    let mut v = s.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let (mut best, mut best_n) = (v[0], 0);
    let mut i = 0;
    while i < v.len() {
        let mut j = i;
        while j < v.len() && v[j] == v[i] {
            j += 1;
        }
        if j - i > best_n {
            best_n = j - i;
            best = v[i];
        }
        i = j.max(i + 1);
    }
    best
}

fn var_std(a: &[Value], sqrt: bool) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let m = a[0].to_num()?;
    let w = if a.len() > 1 && !a[1].is_empty() {
        a[1].scalar()?
    } else {
        0.0
    };
    let dim = if a.len() > 2 {
        Some(a[2].scalar()?)
    } else {
        None
    };
    let f = |s: &[f64]| -> f64 {
        let n = s.len() as f64;
        if n == 0.0 {
            return f64::NAN;
        }
        if n == 1.0 {
            return 0.0;
        }
        let mean = s.iter().sum::<f64>() / n;
        let ss: f64 = s.iter().map(|x| (x - mean).powi(2)).sum();
        let v = if w == 1.0 { ss / n } else { ss / (n - 1.0) };
        if sqrt {
            v.sqrt()
        } else {
            v
        }
    };
    one(Value::mat(reduce(&m, pick_dim(&m, dim)?, f)))
}

fn minmax(a: &[Value], nargout: usize, is_max: bool) -> R {
    check_args(a, 1, 3)?;
    let better = |x: f64, y: f64| if is_max { x > y } else { x < y };
    if a.len() >= 2 && !a[1].is_empty() {
        // Elementwise two-input form.
        let x = a[0].to_num()?;
        let y = a[1].to_num()?;
        let out = ops::broadcast(
            &x,
            &y,
            |p, q| {
                // NaN loses to any number; ties keep the first argument.
                if q.is_nan() || (!p.is_nan() && (better(p, q) || p == q)) {
                    p
                } else {
                    q
                }
            },
            if is_max { "max" } else { "min" },
        )?;
        let class = match (a[0].num_class(), a[1].num_class()) {
            (NumClass::Int(k), _) | (_, NumClass::Int(k)) => NumClass::Int(k),
            _ => NumClass::Double,
        };
        return one(Value::Num(out, class));
    }
    let m = a[0].to_num()?;
    let mut dim = if a.len() == 3 {
        Some(a[2].scalar()?)
    } else {
        None
    };
    let all = a.len() == 3 && a[2].is_text() && a[2].text()? == "all";
    if all {
        dim = None;
    }
    let pick = |s: &[f64]| -> (f64, f64) {
        let mut bi: Option<usize> = None;
        for (i, x) in s.iter().enumerate() {
            if x.is_nan() {
                continue;
            }
            match bi {
                None => bi = Some(i),
                Some(b) if better(*x, s[b]) => bi = Some(i),
                _ => {}
            }
        }
        match bi {
            Some(b) => (s[b], (b + 1) as f64),
            // All-NaN slices report NaN at index 1.
            None => (f64::NAN, if s.is_empty() { f64::NAN } else { 1.0 }),
        }
    };
    if m.is_empty() {
        return Ok(vec![Value::empty(), Value::empty()]);
    }
    let (vals, idx) = if all {
        let (v, i) = pick(&m.data);
        (Array::scalar(v), Array::scalar(i))
    } else {
        let d = pick_dim(&m, dim)?;
        (reduce(&m, d, |s| pick(s).0), reduce(&m, d, |s| pick(s).1))
    };
    let class = match a[0].num_class() {
        NumClass::Logical => NumClass::Double,
        c => c,
    };
    let mut out = vec![Value::Num(vals, class)];
    if nargout > 1 {
        out.push(Value::mat(idx));
    }
    Ok(out)
}

fn cumulative(a: &[Value], f: fn(&[f64]) -> Vec<f64>) -> R {
    check_args(a, 1, 2)?;
    let m = a[0].to_num()?;
    let dim = pick_dim(
        &m,
        if a.len() > 1 {
            Some(a[1].scalar()?)
        } else {
            None
        },
    )?;
    one(Value::mat(cumulate(&m, dim, f)))
}

fn diff(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 3)?;
    let mut m = a[0].to_num()?;
    let order = if a.len() > 1 && !a[1].is_empty() {
        a[1].scalar()? as usize
    } else {
        1
    };
    let dim = pick_dim(
        &m,
        if a.len() > 2 {
            Some(a[2].scalar()?)
        } else {
            None
        },
    )?;
    for _ in 0..order {
        if dim == 1 {
            if m.rows == 0 {
                break;
            }
            let mut out = Array::filled(m.rows - 1, m.cols, 0.0);
            for c in 0..m.cols {
                for r in 0..m.rows - 1 {
                    out.set(r, c, m.get(r + 1, c) - m.get(r, c));
                }
            }
            m = out;
        } else {
            if m.cols == 0 {
                break;
            }
            let mut out = Array::filled(m.rows, m.cols - 1, 0.0);
            for c in 0..m.cols - 1 {
                for r in 0..m.rows {
                    out.set(r, c, m.get(r, c + 1) - m.get(r, c));
                }
            }
            m = out;
        }
    }
    one(Value::mat(m))
}

fn norm(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 2)?;
    let m = a[0].to_num()?;
    let p = if a.len() > 1 {
        if a[1].is_text() {
            match a[1].text()?.as_str() {
                "fro" => -1.0,
                "inf" | "Inf" => f64::INFINITY,
                other => return Err(merr!("Invalid norm type '{}'", other)),
            }
        } else {
            a[1].scalar()?
        }
    } else {
        2.0
    };
    if m.is_vector() || m.is_empty() {
        let v = &m.data;
        let r = if p == f64::INFINITY {
            v.iter().fold(0.0f64, |acc, x| acc.max(x.abs()))
        } else if p == f64::NEG_INFINITY {
            v.iter().fold(f64::INFINITY, |acc, x| acc.min(x.abs()))
        } else if p == -1.0 || p == 2.0 {
            v.iter().map(|x| x * x).sum::<f64>().sqrt()
        } else if p == 1.0 {
            v.iter().map(|x| x.abs()).sum()
        } else {
            v.iter().map(|x| x.abs().powf(p)).sum::<f64>().powf(1.0 / p)
        };
        return one(Value::num(r));
    }
    let r = if p == 2.0 {
        to_dmatrix(&m).svd(false, false).singular_values.max()
    } else if p == 1.0 {
        (0..m.cols)
            .map(|c| (0..m.rows).map(|r| m.get(r, c).abs()).sum::<f64>())
            .fold(0.0, f64::max)
    } else if p == f64::INFINITY {
        (0..m.rows)
            .map(|r| (0..m.cols).map(|c| m.get(r, c).abs()).sum::<f64>())
            .fold(0.0, f64::max)
    } else if p == -1.0 {
        m.data.iter().map(|x| x * x).sum::<f64>().sqrt()
    } else {
        return Err(merr!(
            "Only p = 1, 2, Inf or 'fro' are supported for matrices."
        ));
    };
    one(Value::num(r))
}

pub(crate) fn is_symmetric(m: &Array<f64>) -> bool {
    if m.rows != m.cols {
        return false;
    }
    let scale = m.data.iter().fold(0.0f64, |a, x| a.max(x.abs())).max(1.0);
    for i in 0..m.rows {
        for j in 0..i {
            if (m.get(i, j) - m.get(j, i)).abs() > 1e-12 * scale {
                return false;
            }
        }
    }
    true
}

pub(crate) fn eig(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 1)?;
    let m = a[0].to_num()?;
    if m.rows != m.cols {
        return Err(merr!("Matrix must be square."));
    }
    if m.is_empty() {
        return one(Value::Num(Array::new(0, 1, vec![]), NumClass::Double));
    }
    let dm = to_dmatrix(&m);
    if is_symmetric(&m) {
        let se = dm.symmetric_eigen();
        let mut order: Vec<usize> = (0..m.rows).collect();
        order.sort_by(|&i, &j| se.eigenvalues[i].partial_cmp(&se.eigenvalues[j]).unwrap());
        let vals: Vec<f64> = order.iter().map(|&i| se.eigenvalues[i]).collect();
        if nargout <= 1 {
            return one(Value::col(vals));
        }
        let n = m.rows;
        let mut v = Array::filled(n, n, 0.0);
        for (k, &i) in order.iter().enumerate() {
            let col = se.eigenvectors.column(i);
            // Sign convention: make the largest component positive for stable output.
            let sign =
                if col
                    .iter()
                    .copied()
                    .fold(0.0f64, |acc, x| if x.abs() > acc.abs() { x } else { acc })
                    < 0.0
                {
                    -1.0
                } else {
                    1.0
                };
            for r in 0..n {
                v.set(r, k, col[r] * sign);
            }
        }
        let mut d = Array::filled(n, n, 0.0);
        for (k, val) in vals.iter().enumerate() {
            d.set(k, k, *val);
        }
        return Ok(vec![Value::mat(v), Value::mat(d)]);
    }
    let ce = dm.clone().complex_eigenvalues();
    if ce.iter().any(|c| c.im.abs() > 1e-10 * c.re.abs().max(1.0)) {
        it.warn("Matrix has complex eigenvalues; complex numbers are not supported yet, returning real parts.");
    }
    let mut vals: Vec<f64> = ce.iter().map(|c| c.re).collect();
    vals.sort_by(|x, y| x.partial_cmp(y).unwrap());
    if nargout <= 1 {
        return one(Value::col(vals));
    }
    // Eigenvectors for real eigenvalues via null space of (A - λI).
    let n = m.rows;
    let mut v = Array::filled(n, n, 0.0);
    for (k, lam) in vals.iter().enumerate() {
        let shifted = &dm - nalgebra::DMatrix::<f64>::identity(n, n) * *lam;
        let svd = shifted.svd(false, true);
        if let Some(vt) = svd.v_t {
            let (mut mi, mut mv) = (0, f64::INFINITY);
            for (i, s) in svd.singular_values.iter().enumerate() {
                if *s < mv {
                    mv = *s;
                    mi = i;
                }
            }
            let row = vt.row(mi);
            let nrm = row.norm();
            for r in 0..n {
                v.set(r, k, row[r] / nrm);
            }
        }
    }
    let mut d = Array::filled(n, n, 0.0);
    for (k, val) in vals.iter().enumerate() {
        d.set(k, k, *val);
    }
    Ok(vec![Value::mat(v), Value::mat(d)])
}

fn svd(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 2)?;
    let m = a[0].to_num()?;
    let econ = a.len() > 1;
    let dm = to_dmatrix(&m);
    let (r, c) = (m.rows, m.cols);
    if nargout <= 1 {
        let mut s: Vec<f64> = dm
            .svd(false, false)
            .singular_values
            .iter()
            .copied()
            .collect();
        s.sort_by(|x, y| y.partial_cmp(x).unwrap());
        return one(Value::col(s));
    }
    // Full SVD via eigen-decomposition free route: nalgebra gives thin factors.
    let svd = dm.svd(true, true);
    let u = svd.u.unwrap();
    let vt = svd.v_t.unwrap();
    let s = svd.singular_values;
    let k = s.len();
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|&i, &j| s[j].partial_cmp(&s[i]).unwrap());
    let mut uo = Array::filled(r, k, 0.0);
    let mut vo = Array::filled(c, k, 0.0);
    for (nk, &i) in order.iter().enumerate() {
        for row in 0..r {
            uo.set(row, nk, u[(row, i)]);
        }
        for row in 0..c {
            vo.set(row, nk, vt[(i, row)]);
        }
    }
    let (uo, so, vo) = if econ || r == c {
        let mut sm = Array::filled(k, k, 0.0);
        for (nk, &i) in order.iter().enumerate() {
            sm.set(nk, nk, s[i]);
        }
        (uo, sm, vo)
    } else {
        // Complete the thin factor to a full orthonormal basis with QR.
        let complete = |thin: &Array<f64>, n: usize| -> Array<f64> {
            let mut aug = Array::filled(n, n, 0.0);
            for cc in 0..thin.cols {
                for rr in 0..n {
                    aug.set(rr, cc, *thin.get(rr, cc));
                }
            }
            for cc in thin.cols..n {
                aug.set(cc, cc, 1.0);
                // Fill remaining columns with identity, then orthonormalize below.
            }
            let q = to_dmatrix(&aug).qr().q();
            let mut out = from_dmatrix(&q);
            for cc in 0..thin.cols {
                for rr in 0..n {
                    out.set(rr, cc, *thin.get(rr, cc));
                }
            }
            out
        };
        let uf = if r > k { complete(&uo, r) } else { uo };
        let vf = if c > k { complete(&vo, c) } else { vo };
        let mut sm = Array::filled(r, c, 0.0);
        for (nk, &i) in order.iter().enumerate() {
            sm.set(nk, nk, s[i]);
        }
        (uf, sm, vf)
    };
    Ok(vec![Value::mat(uo), Value::mat(so), Value::mat(vo)])
}

fn lu(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 1)?;
    let m = a[0].to_num()?;
    let dm = to_dmatrix(&m);
    let lu = dm.lu();
    let l = from_dmatrix(&lu.l());
    let u = from_dmatrix(&lu.u());
    let mut p = nalgebra::DMatrix::<f64>::identity(m.rows, m.rows);
    lu.p().permute_rows(&mut p);
    let p = from_dmatrix(&p);
    if nargout <= 2 {
        // L is permuted so that A = L*U.
        let pl = ops::matmul(&p.transpose(), &l);
        return Ok(vec![Value::mat(pl), Value::mat(u)]);
    }
    Ok(vec![Value::mat(l), Value::mat(u), Value::mat(p)])
}

fn qr(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 2)?;
    let m = a[0].to_num()?;
    let qr = to_dmatrix(&m).qr();
    let q = from_dmatrix(&qr.q());
    let r = from_dmatrix(&qr.r());
    if nargout <= 1 {
        return one(Value::mat(r));
    }
    Ok(vec![Value::mat(q), Value::mat(r)])
}

fn polyfit(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 3, 3)?;
    let x = a[0].to_num()?;
    let y = a[1].to_num()?;
    let n = a[2].usize_scalar("N")?;
    if x.numel() != y.numel() {
        return Err(merr!(
            "The first two inputs must have the same number of elements."
        ));
    }
    let m = x.numel();
    let mut v = Array::filled(m, n + 1, 0.0);
    for i in 0..m {
        for j in 0..=n {
            v.set(i, j, x.data[i].powi((n - j) as i32));
        }
    }
    let yc = Array::col(y.data.clone());
    let p = ops::mldivide(&v, &yc)?;
    one(Value::row(p.data))
}

fn polyval(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 2, 2)?;
    let p = a[0].to_num()?;
    let x = a[1].to_num()?;
    one(Value::mat(
        x.map(|xv| p.data.iter().fold(0.0, |acc, c| acc * xv + c)),
    ))
}

fn roots(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 1)?;
    let mut p: Vec<f64> = a[0].to_num()?.data;
    while p.first() == Some(&0.0) {
        p.remove(0);
    }
    let mut trailing = 0;
    while p.last() == Some(&0.0) {
        p.pop();
        trailing += 1;
    }
    let n = p.len().saturating_sub(1);
    let mut out = Vec::new();
    if n >= 1 {
        let mut c = nalgebra::DMatrix::<f64>::zeros(n, n);
        for j in 0..n {
            c[(0, j)] = -p[j + 1] / p[0];
        }
        for i in 1..n {
            c[(i, i - 1)] = 1.0;
        }
        let ev = c.complex_eigenvalues();
        if ev.iter().any(|z| z.im.abs() > 1e-9) {
            it.warn("Polynomial has complex roots; complex numbers are not supported yet, returning real parts.");
        }
        let mut r: Vec<f64> = ev.iter().map(|z| z.re).collect();
        r.sort_by(|x, y| y.partial_cmp(x).unwrap());
        out.extend(r);
    }
    out.extend(std::iter::repeat_n(0.0, trailing));
    one(Value::col(out))
}

fn interp1(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let (x, y, xi, rest) = if a.len() >= 3 && !a[2].is_text() {
        (a[0].to_num()?, a[1].to_num()?, a[2].to_num()?, &a[3..])
    } else {
        let y = a[0].to_num()?;
        (
            Array::row((1..=y.numel()).map(|i| i as f64).collect()),
            y,
            a[1].to_num()?,
            &a[2..],
        )
    };
    let mut method = "linear".to_string();
    let mut extrap: Option<f64> = None;
    let mut do_extrap = false;
    for r in rest {
        if r.is_text() {
            let t = r.text()?.to_lowercase();
            if t == "extrap" {
                do_extrap = true;
            } else {
                method = t;
            }
        } else {
            extrap = Some(r.scalar()?);
        }
    }
    let n = x.numel();
    if n != y.numel() {
        return Err(merr!(
            "The grid vectors must have the same length as the values."
        ));
    }
    let mut pts: Vec<(f64, f64)> = x.data.iter().copied().zip(y.data.iter().copied()).collect();
    pts.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
    let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    let spline = if method == "spline" || method == "pchip" || method == "cubic" {
        Some(natural_spline(&xs, &ys))
    } else {
        None
    };
    let eval = |q: f64| -> f64 {
        if q.is_nan() {
            return f64::NAN;
        }
        let outside = q < xs[0] || q > xs[n - 1];
        if outside && !do_extrap && spline.is_none() {
            return extrap.unwrap_or(f64::NAN);
        }
        let mut k = match xs.binary_search_by(|v| v.partial_cmp(&q).unwrap()) {
            Ok(i) => i.min(n - 2),
            Err(i) => i.saturating_sub(1).min(n - 2),
        };
        if n < 2 {
            k = 0;
        }
        match method.as_str() {
            "nearest" => {
                if n < 2 || (q - xs[k]).abs() <= (xs[k + 1] - q).abs() {
                    ys[k]
                } else {
                    ys[k + 1]
                }
            }
            "previous" => {
                if q >= xs[k + 1] {
                    ys[k + 1]
                } else {
                    ys[k]
                }
            }
            "next" => {
                if q <= xs[k] {
                    ys[k]
                } else {
                    ys[k + 1]
                }
            }
            _ => {
                if let Some(m2) = &spline {
                    let h = xs[k + 1] - xs[k];
                    let t1 = xs[k + 1] - q;
                    let t0 = q - xs[k];
                    return m2[k] * t1.powi(3) / (6.0 * h)
                        + m2[k + 1] * t0.powi(3) / (6.0 * h)
                        + (ys[k] / h - m2[k] * h / 6.0) * t1
                        + (ys[k + 1] / h - m2[k + 1] * h / 6.0) * t0;
                }
                let t = (q - xs[k]) / (xs[k + 1] - xs[k]);
                ys[k] + t * (ys[k + 1] - ys[k])
            }
        }
    };
    one(Value::mat(xi.map(|q| eval(*q))))
}

/// Second derivatives for a not-a-knot-free natural cubic spline.
fn natural_spline(x: &[f64], y: &[f64]) -> Vec<f64> {
    let n = x.len();
    let mut m = vec![0.0; n];
    if n < 3 {
        return m;
    }
    let mut a = vec![0.0; n];
    let mut b = vec![0.0; n];
    let mut c = vec![0.0; n];
    let mut d = vec![0.0; n];
    b[0] = 1.0;
    b[n - 1] = 1.0;
    for i in 1..n - 1 {
        let h0 = x[i] - x[i - 1];
        let h1 = x[i + 1] - x[i];
        a[i] = h0;
        b[i] = 2.0 * (h0 + h1);
        c[i] = h1;
        d[i] = 6.0 * ((y[i + 1] - y[i]) / h1 - (y[i] - y[i - 1]) / h0);
    }
    for i in 1..n {
        let w = a[i] / b[i - 1];
        b[i] -= w * c[i - 1];
        d[i] -= w * d[i - 1];
    }
    m[n - 1] = d[n - 1] / b[n - 1];
    for i in (0..n - 1).rev() {
        m[i] = (d[i] - c[i] * m[i + 1]) / b[i];
    }
    m
}

fn histcounts(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 2)?;
    let x = a[0].to_num()?;
    let data: Vec<f64> = x.data.iter().copied().filter(|v| v.is_finite()).collect();
    let edges: Vec<f64> = if a.len() > 1 && a[1].numel() > 1 {
        a[1].to_num()?.data
    } else {
        let nb = if a.len() > 1 {
            a[1].scalar()? as usize
        } else {
            auto_bins(&data)
        };
        let lo = data.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let (lo, hi) = if data.is_empty() {
            (0.0, 1.0)
        } else if lo == hi {
            (lo - 0.5, hi + 0.5)
        } else {
            (lo, hi)
        };
        (0..=nb)
            .map(|i| lo + (hi - lo) * i as f64 / nb as f64)
            .collect()
    };
    let nb = edges.len().saturating_sub(1);
    let mut counts = vec![0.0; nb];
    for v in &data {
        for i in 0..nb {
            let last = i == nb - 1;
            if *v >= edges[i] && (*v < edges[i + 1] || (last && *v <= edges[i + 1])) {
                counts[i] += 1.0;
                break;
            }
        }
    }
    let mut out = vec![Value::row(counts)];
    if nargout > 1 {
        out.push(Value::row(edges));
    }
    Ok(out)
}

pub fn auto_bins(data: &[f64]) -> usize {
    let n = data.len().max(1) as f64;
    (n.sqrt().ceil() as usize).clamp(1, 100)
}

fn covariance(m: &Array<f64>) -> Array<f64> {
    let n = m.rows as f64;
    let means: Vec<f64> = (0..m.cols)
        .map(|c| (0..m.rows).map(|r| m.get(r, c)).sum::<f64>() / n)
        .collect();
    let mut out = Array::filled(m.cols, m.cols, 0.0);
    for i in 0..m.cols {
        for j in 0..m.cols {
            let s: f64 = (0..m.rows)
                .map(|r| (m.get(r, i) - means[i]) * (m.get(r, j) - means[j]))
                .sum();
            out.set(i, j, s / (n - 1.0));
        }
    }
    if m.cols == 1 {
        return Array::scalar(out.data[0]);
    }
    out
}
