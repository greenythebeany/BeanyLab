//! Complex numbers: constructors and parts, functions whose real inputs can give
//! complex results (sqrt, log, roots, eig, fft), and complex versions of common
//! functions. The complex versions live in `Interp::complex_builtins` and are used
//! when an argument is complex; real arguments keep the real implementations.

use super::*;
use crate::interp::Builtin;
use crate::ops::broadcast_t;

fn c(re: f64, im: f64) -> C64 {
    C64::new(re, im)
}

const ZERO: C64 = C64::new(0.0, 0.0);

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("i", |_, a, _| unit(a));
    b.insert("j", |_, a, _| unit(a));
    b.insert("complex", |_, a, _| {
        check_args(&a, 1, 2)?;
        let re = a[0].to_num()?;
        let im = if a.len() > 1 {
            a[1].to_num()?
        } else {
            Array::filled(re.rows, re.cols, 0.0)
        };
        // complex() keeps a complex result even when the imaginary part is zero.
        one(Value::Complex(broadcast_t(&re, &im, c, "complex")?))
    });
    b.insert("real", |_, a, _| part(a, |z| z.re));
    b.insert("imag", |_, a, _| part(a, |z| z.im));
    b.insert("abs", |_, a, _| {
        check_args(&a, 1, 1)?;
        if let Value::Complex(z) = &a[0] {
            return one(Value::mat(z.map(|z| z.norm())));
        }
        let class = match a[0].num_class() {
            NumClass::Logical => NumClass::Double,
            c => c,
        };
        one(Value::Num(a[0].to_num()?.map(|x| x.abs()), class))
    });
    b.insert("angle", |_, a, _| part(a, |z| z.arg()));
    b.insert("conj", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(match &a[0] {
            Value::Complex(z) => Value::Complex(z.map(|z| z.conj())),
            v => v.clone(),
        })
    });
    b.insert("isreal", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(!a[0].is_complex()))
    });
    // Functions whose real inputs can leave the real domain.
    b.insert("sqrt", |_, a, _| {
        domain(a, |x| x >= 0.0, f64::sqrt, |z| z.sqrt())
    });
    b.insert("log", |_, a, _| {
        domain(a, |x| x >= 0.0 || x.is_nan(), f64::ln, |z| z.ln())
    });
    b.insert("log10", |_, a, _| {
        domain(
            a,
            |x| x >= 0.0 || x.is_nan(),
            f64::log10,
            |z| z.ln() / std::f64::consts::LN_10,
        )
    });
    b.insert("log2", |_, a, n| {
        if n > 1 {
            return log2_parts(&a);
        }
        domain(
            a,
            |x| x >= 0.0 || x.is_nan(),
            f64::log2,
            |z| z.ln() / std::f64::consts::LN_2,
        )
    });
    b.insert("asin", |_, a, _| {
        domain(a, |x| x.abs() <= 1.0 || x.is_nan(), f64::asin, |z| z.asin())
    });
    b.insert("acos", |_, a, _| {
        domain(a, |x| x.abs() <= 1.0 || x.is_nan(), f64::acos, |z| z.acos())
    });
    b.insert("acosh", |_, a, _| {
        domain(a, |x| x >= 1.0 || x.is_nan(), f64::acosh, |z| z.acosh())
    });
    b.insert("atanh", |_, a, _| {
        domain(
            a,
            |x| x.abs() <= 1.0 || x.is_nan(),
            f64::atanh,
            |z| z.atanh(),
        )
    });
    b.insert("power", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(crate::ops::binary(crate::ast::BinOp::ElPow, &a[0], &a[1])?)
    });
    b.insert("roots", roots);
    b.insert("eig", eig);
    b.insert("fft", |_, a, _| fft_fn(a, false));
    b.insert("ifft", |_, a, _| fft_fn(a, true));
    b.insert("nextpow2", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::mat(a[0].to_num()?.map(|x| {
            if *x == 0.0 {
                0.0
            } else {
                x.abs().log2().ceil()
            }
        })))
    });
    b.insert("unwrap", |_, a, _| {
        check_args(&a, 1, 2)?;
        let tol = if a.len() > 1 {
            a[1].scalar()?
        } else {
            std::f64::consts::PI
        };
        let m = a[0].to_num()?;
        let out = along_dim(&m.map(|x| c(*x, 0.0)), None, |v| {
            let mut out = Vec::with_capacity(v.len());
            let mut shift = 0.0;
            for (k, z) in v.iter().enumerate() {
                if k > 0 {
                    let d = z.re - v[k - 1].re;
                    if d > tol {
                        shift -= 2.0
                            * std::f64::consts::PI
                            * ((d + std::f64::consts::PI) / (2.0 * std::f64::consts::PI)).floor();
                    } else if d < -tol {
                        shift += 2.0
                            * std::f64::consts::PI
                            * ((-d + std::f64::consts::PI) / (2.0 * std::f64::consts::PI)).floor();
                    }
                }
                out.push(c(z.re + shift, 0.0));
            }
            out
        });
        one(Value::mat(out.map(|z| z.re)))
    });

    let cb = &mut it.complex_builtins;
    let elementwise: [(&'static str, Builtin); 15] = [
        ("exp", |_, a, _| cmap(a, |z| z.exp())),
        ("sin", |_, a, _| cmap(a, |z| z.sin())),
        ("cos", |_, a, _| cmap(a, |z| z.cos())),
        ("tan", |_, a, _| cmap(a, |z| z.tan())),
        ("sinh", |_, a, _| cmap(a, |z| z.sinh())),
        ("cosh", |_, a, _| cmap(a, |z| z.cosh())),
        ("tanh", |_, a, _| cmap(a, |z| z.tanh())),
        ("atan", |_, a, _| cmap(a, |z| z.atan())),
        ("asinh", |_, a, _| cmap(a, |z| z.asinh())),
        ("floor", |_, a, _| {
            cmap(a, |z| c(z.re.floor(), z.im.floor()))
        }),
        ("ceil", |_, a, _| cmap(a, |z| c(z.re.ceil(), z.im.ceil()))),
        ("fix", |_, a, _| cmap(a, |z| c(z.re.trunc(), z.im.trunc()))),
        ("round", |_, a, _| {
            cmap(a, |z| c(z.re.round(), z.im.round()))
        }),
        ("sign", |_, a, _| {
            cmap(a, |z| {
                let n = z.norm();
                if n == 0.0 {
                    ZERO
                } else {
                    z / n
                }
            })
        }),
        ("double", |_, a, _| {
            check_args(&a, 1, 1)?;
            one(a[0].clone())
        }),
    ];
    for (name, f) in elementwise {
        cb.insert(name, f);
    }
    cb.insert("isnan", |_, a, _| {
        ctest(a, |z| z.re.is_nan() || z.im.is_nan())
    });
    cb.insert("isinf", |_, a, _| {
        ctest(a, |z| z.re.is_infinite() || z.im.is_infinite())
    });
    cb.insert("isfinite", |_, a, _| {
        ctest(a, |z| z.re.is_finite() && z.im.is_finite())
    });
    cb.insert("isnumeric", |_, _, _| one(bool_val(true)));
    cb.insert("isfloat", |_, _, _| one(bool_val(true)));
    cb.insert("sum", |_, a, _| creduce(a, |v| v.iter().sum()));
    cb.insert("prod", |_, a, _| creduce(a, |v| v.iter().product()));
    cb.insert("mean", |_, a, _| {
        creduce(a, |v| {
            if v.is_empty() {
                c(f64::NAN, 0.0)
            } else {
                v.iter().sum::<C64>() / v.len() as f64
            }
        })
    });
    cb.insert("cumsum", |_, a, _| {
        ccumulate(a, |v| {
            v.iter()
                .scan(ZERO, |acc, z| {
                    *acc += z;
                    Some(*acc)
                })
                .collect()
        })
    });
    cb.insert("cumprod", |_, a, _| {
        ccumulate(a, |v| {
            v.iter()
                .scan(c(1.0, 0.0), |acc, z| {
                    *acc *= z;
                    Some(*acc)
                })
                .collect()
        })
    });
    cb.insert("max", |_, a, n| cminmax(a, n, true));
    cb.insert("min", |_, a, n| cminmax(a, n, false));
    cb.insert("sort", csort);
    cb.insert("find", |it, a, n| {
        let mut a = a;
        a[0] = nonzero_mask(&a[0])?;
        let f = it.builtins["find"];
        f(it, a, n)
    });
    cb.insert("any", |it, a, n| {
        let mut a = a;
        a[0] = nonzero_mask(&a[0])?;
        let f = it.builtins["any"];
        f(it, a, n)
    });
    cb.insert("all", |it, a, n| {
        let mut a = a;
        a[0] = nonzero_mask(&a[0])?;
        let f = it.builtins["all"];
        f(it, a, n)
    });
    cb.insert("nnz", |_, a, _| {
        check_args(&a, 1, 1)?;
        let z = a[0].to_cplx()?;
        one(Value::num(
            z.data.iter().filter(|z| z.re != 0.0 || z.im != 0.0).count() as f64,
        ))
    });
    cb.insert("polyval", |_, a, _| {
        check_args(&a, 2, 2)?;
        let p = a[0].to_cplx()?;
        let x = a[1].to_cplx()?;
        one(Value::complex(
            x.map(|xv| p.data.iter().fold(ZERO, |acc, k| acc * xv + k)),
        ))
    });
    cb.insert("poly", |_, a, _| {
        check_args(&a, 1, 1)?;
        let r = a[0].to_cplx()?;
        let mut p = vec![c(1.0, 0.0)];
        for x in &r.data {
            let mut q = vec![ZERO; p.len() + 1];
            for (i, k) in p.iter().enumerate() {
                q[i] += k;
                q[i + 1] -= k * x;
            }
            p = q;
        }
        // Conjugate pairs give real coefficients up to rounding.
        let scale = p.iter().map(|z| z.norm()).fold(0.0, f64::max).max(1.0);
        if p.iter().all(|z| z.im.abs() <= 1e-12 * scale) {
            return one(Value::row(p.iter().map(|z| z.re).collect()));
        }
        one(Value::complex(Array::row(p)))
    });
    cb.insert("conv", |_, a, _| {
        check_args(&a, 2, 3)?;
        let x = a[0].to_cplx()?;
        let y = a[1].to_cplx()?;
        if x.is_empty() || y.is_empty() {
            return one(Value::empty());
        }
        let mut out = vec![ZERO; x.numel() + y.numel() - 1];
        for (i, p) in x.data.iter().enumerate() {
            for (j, q) in y.data.iter().enumerate() {
                out[i + j] += p * q;
            }
        }
        let col = x.cols == 1 && x.rows > 1;
        one(Value::complex(if col {
            Array::col(out)
        } else {
            Array::row(out)
        }))
    });
    cb.insert("dot", |_, a, _| {
        check_args(&a, 2, 2)?;
        let x = a[0].to_cplx()?;
        let y = a[1].to_cplx()?;
        if x.numel() != y.numel() {
            return Err(merr!("A and B must be the same size."));
        }
        one(Value::complex(Array::scalar(
            x.data.iter().zip(&y.data).map(|(p, q)| p.conj() * q).sum(),
        )))
    });
    cb.insert("trace", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = a[0].to_cplx()?;
        one(Value::complex(Array::scalar(
            (0..m.rows.min(m.cols)).map(|i| *m.get(i, i)).sum(),
        )))
    });
    cb.insert("det", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = a[0].to_cplx()?;
        if m.rows != m.cols {
            return Err(merr!("Matrix must be square."));
        }
        let d = nalgebra::DMatrix::from_column_slice(m.rows, m.cols, &m.data).determinant();
        one(Value::complex(Array::scalar(d)))
    });
    cb.insert("inv", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = a[0].to_cplx()?;
        if m.rows != m.cols {
            return Err(merr!("Matrix must be square."));
        }
        let inv = nalgebra::DMatrix::from_column_slice(m.rows, m.cols, &m.data)
            .try_inverse()
            .ok_or_else(|| merr!("Matrix is singular to working precision."))?;
        one(Value::complex(Array::new(
            m.rows,
            m.cols,
            inv.as_slice().to_vec(),
        )))
    });
    cb.insert("norm", |_, a, _| {
        check_args(&a, 1, 2)?;
        let m = a[0].to_cplx()?;
        let p = if a.len() > 1 {
            if a[1].is_text() {
                match a[1].text()?.as_str() {
                    "inf" | "Inf" => f64::INFINITY,
                    "fro" => -1.0,
                    _ => 2.0,
                }
            } else {
                a[1].scalar()?
            }
        } else {
            2.0
        };
        let vec = m.rows == 1 || m.cols == 1;
        let r = if p == -1.0 || (vec && p == 2.0) {
            m.data.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt()
        } else if vec && p.is_infinite() {
            m.data.iter().map(|z| z.norm()).fold(0.0, f64::max)
        } else if vec {
            m.data
                .iter()
                .map(|z| z.norm().powf(p))
                .sum::<f64>()
                .powf(1.0 / p)
        } else if p == 1.0 {
            (0..m.cols)
                .map(|j| (0..m.rows).map(|i| m.get(i, j).norm()).sum::<f64>())
                .fold(0.0, f64::max)
        } else if p.is_infinite() {
            (0..m.rows)
                .map(|i| (0..m.cols).map(|j| m.get(i, j).norm()).sum::<f64>())
                .fold(0.0, f64::max)
        } else {
            nalgebra::DMatrix::from_column_slice(m.rows, m.cols, &m.data)
                .svd(false, false)
                .singular_values
                .max()
        };
        one(Value::num(r))
    });
    cb.insert("diag", |_, a, _| {
        check_args(&a, 1, 2)?;
        let m = a[0].to_cplx()?;
        let k = if a.len() > 1 {
            a[1].scalar()? as i64
        } else {
            0
        };
        if m.rows == 1 || m.cols == 1 {
            let n = m.numel() + k.unsigned_abs() as usize;
            let mut out = Array::filled(n, n, ZERO);
            for (i, z) in m.data.iter().enumerate() {
                let (r, cc) = if k >= 0 {
                    (i, i + k as usize)
                } else {
                    (i + (-k) as usize, i)
                };
                out.set(r, cc, *z);
            }
            return one(Value::complex(out));
        }
        let mut v = Vec::new();
        for i in 0..m.rows {
            let j = i as i64 + k;
            if j >= 0 && (j as usize) < m.cols {
                v.push(*m.get(i, j as usize));
            }
        }
        one(Value::complex(Array::col(v)))
    });
    cb.insert("fliplr", |_, a, _| {
        check_args(&a, 1, 1)?;
        let (r, cc) = a[0].dims();
        one(a[0]
            .index(&[Idx::All, Idx::List((0..cc).rev().collect(), (1, cc))])?
            .reshape(r, cc)?)
    });
    cb.insert("flipud", |_, a, _| {
        check_args(&a, 1, 1)?;
        let (r, cc) = a[0].dims();
        one(a[0]
            .index(&[Idx::List((0..r).rev().collect(), (r, 1)), Idx::All])?
            .reshape(r, cc)?)
    });
    cb.insert("repmat", |_, a, _| {
        check_args(&a, 2, 3)?;
        let (m, n) = if a.len() == 3 {
            (a[1].scalar()? as usize, a[2].scalar()? as usize)
        } else {
            let v = a[1].to_num()?;
            (
                v.data[0] as usize,
                *v.data.get(1).unwrap_or(&v.data[0]) as usize,
            )
        };
        let z = a[0].to_cplx()?;
        let mut out = Array::filled(z.rows * m, z.cols * n, ZERO);
        for bi in 0..m {
            for bj in 0..n {
                for i in 0..z.rows {
                    for j in 0..z.cols {
                        out.set(bi * z.rows + i, bj * z.cols + j, *z.get(i, j));
                    }
                }
            }
        }
        one(Value::complex(out))
    });
    cb.insert("squeeze", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(a[0].clone())
    });
    cb.insert("transpose", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(a[0].transpose()?)
    });
    cb.insert("ctranspose", |_, a, _| {
        check_args(&a, 1, 1)?;
        let z = a[0].to_cplx()?;
        one(Value::complex(z.transpose().map(|z| z.conj())))
    });
    cb.insert("num2str", |_, a, _| {
        check_args(&a, 1, 2)?;
        let z = a[0].to_cplx()?;
        let rows: Vec<String> = (0..z.rows)
            .map(|r| {
                (0..z.cols)
                    .map(|cc| crate::display::cplx_to_str(*z.get(r, cc)))
                    .collect::<Vec<_>>()
                    .join("  ")
            })
            .collect();
        let w = rows.iter().map(|s| s.chars().count()).max().unwrap_or(0);
        let data: Vec<Vec<char>> = rows
            .iter()
            .map(|s| format!("{:<w$}", s, w = w).chars().collect())
            .collect();
        let mut out = Array::filled(data.len(), w, ' ');
        for (r, line) in data.iter().enumerate() {
            for (cc, ch) in line.iter().enumerate() {
                out.set(r, cc, *ch);
            }
        }
        one(Value::Char(out))
    });
    cb.insert("mat2str", |_, a, _| {
        check_args(&a, 1, 2)?;
        let z = a[0].to_cplx()?;
        if z.numel() == 1 {
            return one(Value::chars(&crate::display::cplx_to_str(z.data[0])));
        }
        let rows: Vec<String> = (0..z.rows)
            .map(|r| {
                (0..z.cols)
                    .map(|cc| crate::display::cplx_to_str(*z.get(r, cc)))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        one(Value::chars(&format!("[{}]", rows.join(";"))))
    });
}

fn unit(a: Vec<Value>) -> R {
    if !a.is_empty() {
        return Err(merr!("Too many input arguments."));
    }
    one(Value::Complex(Array::scalar(c(0.0, 1.0))))
}

fn part(a: Vec<Value>, f: fn(C64) -> f64) -> R {
    check_args(&a, 1, 1)?;
    one(Value::mat(a[0].to_cplx()?.map(|z| f(*z))))
}

/// Real inside `ok`, complex otherwise (or for complex input).
fn domain(a: Vec<Value>, ok: fn(f64) -> bool, real: fn(f64) -> f64, cplx: fn(C64) -> C64) -> R {
    check_args(&a, 1, 1)?;
    if let Value::Complex(z) = &a[0] {
        return one(Value::complex(z.map(|z| cplx(*z))));
    }
    let x = a[0].to_num()?;
    if x.data.iter().all(|v| ok(*v)) {
        return one(Value::mat(x.map(|v| real(*v))));
    }
    one(Value::complex(x.map(|v| {
        if ok(*v) {
            c(real(*v), 0.0)
        } else {
            cplx(c(*v, 0.0))
        }
    })))
}

/// `[f, e] = log2(x)`: mantissa in [0.5, 1) and exponent.
fn log2_parts(a: &[Value]) -> R {
    check_args(a, 1, 1)?;
    let x = a[0].to_num()?;
    let mut f = Vec::with_capacity(x.numel());
    let mut e = Vec::with_capacity(x.numel());
    for v in &x.data {
        if *v == 0.0 || !v.is_finite() {
            f.push(*v);
            e.push(0.0);
        } else {
            let k = v.abs().log2().floor() + 1.0;
            f.push(v / 2f64.powf(k));
            e.push(k);
        }
    }
    Ok(vec![
        Value::mat(Array::new(x.rows, x.cols, f)),
        Value::mat(Array::new(x.rows, x.cols, e)),
    ])
}

fn cmap(a: Vec<Value>, f: fn(C64) -> C64) -> R {
    check_args(&a, 1, 1)?;
    one(Value::complex(a[0].to_cplx()?.map(|z| f(*z))))
}

fn ctest(a: Vec<Value>, f: fn(C64) -> bool) -> R {
    check_args(&a, 1, 1)?;
    one(Value::logical(a[0].to_cplx()?.map(|z| {
        if f(*z) {
            1.0
        } else {
            0.0
        }
    })))
}

fn nonzero_mask(v: &Value) -> MResult<Value> {
    Ok(Value::logical(v.to_cplx()?.map(|z| {
        if z.re != 0.0 || z.im != 0.0 {
            1.0
        } else {
            0.0
        }
    })))
}

/// Dimension to work along: the given one, or the first non-singleton.
fn work_dim(m: &Array<C64>, dim: Option<usize>) -> usize {
    dim.unwrap_or(if m.rows != 1 { 1 } else { 2 })
}

/// Apply `f` to each vector along the dimension; `f` returns a vector of the same length.
fn along_dim(m: &Array<C64>, dim: Option<usize>, f: impl Fn(&[C64]) -> Vec<C64>) -> Array<C64> {
    let mut out = m.clone();
    if work_dim(m, dim) == 1 {
        for j in 0..m.cols {
            let col: Vec<C64> = (0..m.rows).map(|i| *m.get(i, j)).collect();
            for (i, z) in f(&col).into_iter().enumerate() {
                out.set(i, j, z);
            }
        }
    } else {
        for i in 0..m.rows {
            let row: Vec<C64> = (0..m.cols).map(|j| *m.get(i, j)).collect();
            for (j, z) in f(&row).into_iter().enumerate() {
                out.set(i, j, z);
            }
        }
    }
    out
}

fn dim_arg(a: &[Value]) -> MResult<(Option<usize>, bool)> {
    let mut dim = None;
    let mut all = false;
    for x in a {
        if x.is_text() {
            if x.text()? == "all" {
                all = true;
            }
        } else {
            dim = Some(x.scalar()? as usize);
        }
    }
    Ok((dim, all))
}

fn creduce(a: Vec<Value>, f: fn(&[C64]) -> C64) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let m = a[0].to_cplx()?;
    let (dim, all) = dim_arg(&a[1..])?;
    if all {
        return one(Value::complex(Array::scalar(f(&m.data))));
    }
    let out = if work_dim(&m, dim) == 1 {
        Array::row(
            (0..m.cols)
                .map(|j| f(&m.data[j * m.rows..(j + 1) * m.rows]))
                .collect(),
        )
    } else {
        Array::col(
            (0..m.rows)
                .map(|i| f(&(0..m.cols).map(|j| *m.get(i, j)).collect::<Vec<_>>()))
                .collect(),
        )
    };
    one(Value::complex(out))
}

fn ccumulate(a: Vec<Value>, f: fn(&[C64]) -> Vec<C64>) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let m = a[0].to_cplx()?;
    let (dim, _) = dim_arg(&a[1..])?;
    one(Value::complex(along_dim(&m, dim, f)))
}

/// Complex ordering: by magnitude, then by phase angle.
fn cless(x: C64, y: C64) -> std::cmp::Ordering {
    x.norm()
        .partial_cmp(&y.norm())
        .unwrap_or(std::cmp::Ordering::Equal)
        .then(
            x.arg()
                .partial_cmp(&y.arg())
                .unwrap_or(std::cmp::Ordering::Equal),
        )
}

fn cminmax(a: Vec<Value>, nargout: usize, is_max: bool) -> R {
    if a.len() >= 2 && !a[1].is_empty() {
        // Two-argument form: elementwise.
        let x = a[0].to_cplx()?;
        let y = a[1].to_cplx()?;
        let pick = move |p: C64, q: C64| {
            let o = cless(p, q);
            if (o == std::cmp::Ordering::Less) == is_max {
                q
            } else {
                p
            }
        };
        return one(Value::complex(broadcast_t(&x, &y, pick, "max")?));
    }
    let m = a[0].to_cplx()?;
    let (dim, _) = dim_arg(a.get(2..).unwrap_or(&[]))?;
    let best = |v: &[C64]| -> (C64, usize) {
        let mut bi = 0;
        for (k, z) in v.iter().enumerate() {
            if z.re.is_nan() || z.im.is_nan() {
                continue;
            }
            let o = cless(*z, v[bi]);
            if v[bi].re.is_nan()
                || (is_max && o == std::cmp::Ordering::Greater)
                || (!is_max && o == std::cmp::Ordering::Less)
            {
                bi = k;
            }
        }
        (v.get(bi).copied().unwrap_or(c(f64::NAN, 0.0)), bi + 1)
    };
    let (vals, idx): (Vec<C64>, Vec<f64>) = if work_dim(&m, dim) == 1 {
        (0..m.cols)
            .map(|j| {
                let (z, k) = best(&m.data[j * m.rows..(j + 1) * m.rows]);
                (z, k as f64)
            })
            .unzip()
    } else {
        (0..m.rows)
            .map(|i| {
                let (z, k) = best(&(0..m.cols).map(|j| *m.get(i, j)).collect::<Vec<_>>());
                (z, k as f64)
            })
            .unzip()
    };
    let row = work_dim(&m, dim) == 1;
    let shape = |v: Vec<f64>| if row { Array::row(v) } else { Array::col(v) };
    let out = Value::complex(if row {
        Array::row(vals)
    } else {
        Array::col(vals)
    });
    if nargout > 1 {
        return Ok(vec![out, Value::mat(shape(idx))]);
    }
    one(out)
}

fn csort(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let m = a[0].to_cplx()?;
    let mut dim = None;
    let mut desc = false;
    for x in &a[1..] {
        if x.is_text() {
            desc = x.text()?.eq_ignore_ascii_case("descend");
        } else {
            dim = Some(x.scalar()? as usize);
        }
    }
    let mut idx = Array::filled(m.rows, m.cols, 0.0);
    let mut out = m.clone();
    let mut sort_vec = |get: &dyn Fn(usize) -> (usize, usize), n: usize| {
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&p, &q| {
            let (pr, pc) = get(p);
            let (qr, qc) = get(q);
            let o = cless(*m.get(pr, pc), *m.get(qr, qc));
            if desc {
                o.reverse()
            } else {
                o
            }
        });
        for (k, &src) in order.iter().enumerate() {
            let (dr, dc) = get(k);
            let (sr, sc) = get(src);
            out.set(dr, dc, *m.get(sr, sc));
            idx.set(dr, dc, (src + 1) as f64);
        }
    };
    if work_dim(&m, dim) == 1 {
        for j in 0..m.cols {
            sort_vec(&|k| (k, j), m.rows);
        }
    } else {
        for i in 0..m.rows {
            sort_vec(&|k| (i, k), m.cols);
        }
    }
    let v = Value::complex(out);
    if nargout > 1 {
        return Ok(vec![v, Value::mat(idx)]);
    }
    one(v)
}

/// Polynomial roots from the companion matrix; complex roots come back complex.
fn roots(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 1)?;
    let mut p: Vec<C64> = a[0].to_cplx()?.data;
    while p.first().is_some_and(|z| z.re == 0.0 && z.im == 0.0) {
        p.remove(0);
    }
    let mut trailing = 0;
    while p.last().is_some_and(|z| z.re == 0.0 && z.im == 0.0) {
        p.pop();
        trailing += 1;
    }
    let n = p.len().saturating_sub(1);
    let mut out: Vec<C64> = Vec::new();
    if n >= 1 {
        let real = p.iter().all(|z| z.im == 0.0);
        let ev: Vec<C64> = if real {
            let mut m = nalgebra::DMatrix::<f64>::zeros(n, n);
            for j in 0..n {
                m[(0, j)] = -p[j + 1].re / p[0].re;
            }
            for i in 1..n {
                m[(i, i - 1)] = 1.0;
            }
            m.complex_eigenvalues().iter().copied().collect()
        } else {
            let mut m = nalgebra::DMatrix::<C64>::zeros(n, n);
            for j in 0..n {
                m[(0, j)] = -p[j + 1] / p[0];
            }
            for i in 1..n {
                m[(i, i - 1)] = c(1.0, 0.0);
            }
            m.schur()
                .eigenvalues()
                .map(|v| v.iter().copied().collect())
                .unwrap_or_default()
        };
        let mut r = clean_conjugates(ev);
        // Largest magnitude first, conjugate pairs together (positive imaginary first).
        r.sort_by(|x, y| {
            y.norm()
                .partial_cmp(&x.norm())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(y.re.partial_cmp(&x.re).unwrap_or(std::cmp::Ordering::Equal))
                .then(y.im.partial_cmp(&x.im).unwrap_or(std::cmp::Ordering::Equal))
        });
        out.extend(r);
    }
    out.extend(std::iter::repeat_n(ZERO, trailing));
    one(Value::complex(Array::col(out)))
}

/// Snap tiny imaginary parts from rounding to zero.
fn clean_conjugates(v: Vec<C64>) -> Vec<C64> {
    v.into_iter()
        .map(|z| {
            if z.im.abs() <= 1e-12 * z.re.abs().max(1e-300) && z.im.abs() < 1e-10 {
                c(z.re, 0.0)
            } else {
                z
            }
        })
        .collect()
}

/// Eigenvalues (and vectors) of general matrices, complex when needed.
fn eig(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 2)?;
    let m = a[0].to_cplx()?;
    if m.rows != m.cols {
        return Err(merr!("Matrix must be square."));
    }
    let real = m.data.iter().all(|z| z.im == 0.0);
    if real {
        let r = m.map(|z| z.re);
        let ev = crate::ops::to_dmatrix(&r).complex_eigenvalues();
        if ev.iter().all(|z| z.im == 0.0) || super::math::is_symmetric(&r) {
            return super::math::eig(it, vec![Value::mat(r)], nargout);
        }
    }
    let n = m.rows;
    let cm = nalgebra::DMatrix::from_column_slice(n, n, &m.data);
    let vals: Vec<C64> = if real {
        crate::ops::to_dmatrix(&m.map(|z| z.re))
            .complex_eigenvalues()
            .iter()
            .copied()
            .collect()
    } else {
        cm.clone()
            .schur()
            .eigenvalues()
            .map(|v| v.iter().copied().collect())
            .ok_or_else(|| merr!("Eigenvalue computation did not converge."))?
    };
    let mut vals = clean_conjugates(vals);
    // Conjugate pairs adjacent, positive imaginary part first.
    vals.sort_by(|x, y| {
        x.re.partial_cmp(&y.re)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(y.im.partial_cmp(&x.im).unwrap_or(std::cmp::Ordering::Equal))
    });
    if nargout <= 1 {
        return one(Value::complex(Array::col(vals)));
    }
    let mut v = Array::filled(n, n, ZERO);
    for (k, lam) in vals.iter().enumerate() {
        let shifted = &cm - nalgebra::DMatrix::<C64>::identity(n, n) * *lam;
        let svd = shifted.svd(false, true);
        if let Some(vt) = svd.v_t {
            // Right singular vector of the smallest singular value.
            let (mut best, mut bv) = (0, f64::INFINITY);
            for (i, s) in svd.singular_values.iter().enumerate() {
                if *s < bv {
                    bv = *s;
                    best = i;
                }
            }
            let row = vt.row(best);
            let norm = row
                .iter()
                .map(|z| z.norm_sqr())
                .sum::<f64>()
                .sqrt()
                .max(1e-300);
            for r in 0..n {
                v.set(r, k, row[r].conj() / norm);
            }
        }
    }
    let mut d = Array::filled(n, n, ZERO);
    for (k, lam) in vals.iter().enumerate() {
        d.set(k, k, *lam);
    }
    Ok(vec![Value::complex(v), Value::complex(d)])
}

/// In-place FFT of any length (radix-2, or Bluestein's algorithm otherwise).
pub fn fft_vec(x: &[C64], inverse: bool) -> Vec<C64> {
    let n = x.len();
    if n <= 1 {
        return x.to_vec();
    }
    let sign = if inverse { 1.0 } else { -1.0 };
    if n.is_power_of_two() {
        let mut a = x.to_vec();
        radix2(&mut a, sign);
        return a;
    }
    if n <= 32 {
        return (0..n)
            .map(|k| {
                (0..n)
                    .map(|t| {
                        let ang =
                            sign * 2.0 * std::f64::consts::PI * ((k * t) % n) as f64 / n as f64;
                        x[t] * c(ang.cos(), ang.sin())
                    })
                    .sum()
            })
            .collect();
    }
    // Bluestein: express the DFT as a convolution of power-of-two length.
    let m = (2 * n - 1).next_power_of_two();
    let w: Vec<C64> = (0..n)
        .map(|k| {
            let ang = sign * std::f64::consts::PI * ((k * k) % (2 * n)) as f64 / n as f64;
            c(ang.cos(), ang.sin())
        })
        .collect();
    let mut a = vec![ZERO; m];
    for k in 0..n {
        a[k] = x[k] * w[k];
    }
    let mut b = vec![ZERO; m];
    b[0] = w[0].conj();
    for k in 1..n {
        b[k] = w[k].conj();
        b[m - k] = w[k].conj();
    }
    radix2(&mut a, -1.0);
    radix2(&mut b, -1.0);
    for k in 0..m {
        a[k] *= b[k];
    }
    radix2(&mut a, 1.0);
    (0..n).map(|k| a[k] / m as f64 * w[k]).collect()
}

fn radix2(a: &mut [C64], sign: f64) {
    let n = a.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            a.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = sign * 2.0 * std::f64::consts::PI / len as f64;
        let wl = c(ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let mut w = c(1.0, 0.0);
            for k in 0..len / 2 {
                let u = a[start + k];
                let v = a[start + k + len / 2] * w;
                a[start + k] = u + v;
                a[start + k + len / 2] = u - v;
                w *= wl;
            }
        }
        len <<= 1;
    }
}

/// `fft(x)`, `fft(x, n)`, `fft(x, n, dim)` and the inverse.
fn fft_fn(a: Vec<Value>, inverse: bool) -> R {
    check_args(&a, 1, 3)?;
    let m = a[0].to_cplx()?;
    let n_arg = match a.get(1) {
        Some(v) if !v.is_empty() => Some(v.scalar()? as usize),
        _ => None,
    };
    let dim = match a.get(2) {
        Some(v) => Some(v.scalar()? as usize),
        None => None,
    };
    let along_rows = work_dim(&m, dim) == 2;
    let len = if along_rows { m.cols } else { m.rows };
    let n = n_arg.unwrap_or(len);
    let lines = if along_rows { m.rows } else { m.cols };
    let (rows, cols) = if along_rows { (m.rows, n) } else { (n, m.cols) };
    let mut out = Array::filled(rows, cols, ZERO);
    for l in 0..lines {
        let mut v: Vec<C64> = (0..len)
            .map(|k| {
                if along_rows {
                    *m.get(l, k)
                } else {
                    *m.get(k, l)
                }
            })
            .collect();
        v.resize(n, ZERO);
        let mut y = fft_vec(&v, inverse);
        if inverse {
            for z in y.iter_mut() {
                *z /= n as f64;
            }
        }
        for (k, z) in y.into_iter().enumerate() {
            if along_rows {
                out.set(l, k, z);
            } else {
                out.set(k, l, z);
            }
        }
    }
    one(Value::complex(out))
}
