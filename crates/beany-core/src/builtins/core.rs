//! Array construction, type conversion, size queries, logical and set operations.

use rand::Rng;
use rand_distr::{Distribution, StandardNormal};

use super::*;
use crate::ops;
use crate::value::values_equal;

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("zeros", zeros);
    b.insert("ones", ones);
    b.insert("eye", eye);
    b.insert("rand", rand);
    b.insert("randn", randn);
    b.insert("randi", randi);
    b.insert("randperm", randperm);
    b.insert("true", true_);
    b.insert("false", false_);
    b.insert("nan", nan);
    b.insert("NaN", nan);
    b.insert("inf", inf);
    b.insert("Inf", inf);
    b.insert("pi", pi);
    b.insert("eps", eps);
    b.insert("realmax", |_, _, _| one(Value::num(f64::MAX)));
    b.insert("realmin", |_, _, _| one(Value::num(f64::MIN_POSITIVE)));
    b.insert("flintmax", |_, _, _| one(Value::num(9007199254740992.0)));
    b.insert("intmax", intmax);
    b.insert("intmin", intmin);
    b.insert("linspace", linspace);
    b.insert("logspace", logspace);
    b.insert("colon", colon);
    b.insert("magic", magic);
    b.insert("repmat", repmat);
    b.insert("reshape", reshape);
    b.insert("cat", cat);
    b.insert("horzcat", |_, a, _| one(ops::horzcat(a)?));
    b.insert("vertcat", |_, a, _| one(ops::vertcat(a)?));
    b.insert("diag", diag);
    b.insert("tril", tril);
    b.insert("triu", triu);
    b.insert("fliplr", fliplr);
    b.insert("flipud", flipud);
    b.insert("flip", flip);
    b.insert("rot90", rot90);
    b.insert("circshift", circshift);
    b.insert("transpose", |_, a, _| {
        one(a
            .first()
            .ok_or_else(|| merr!("Not enough input arguments."))?
            .transpose()?)
    });
    b.insert("ctranspose", |_, a, _| {
        one(a
            .first()
            .ok_or_else(|| merr!("Not enough input arguments."))?
            .transpose()?)
    });
    b.insert("meshgrid", meshgrid);
    b.insert("ndgrid", ndgrid);
    b.insert("size", size);
    b.insert("numel", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::num(a[0].numel() as f64))
    });
    b.insert("length", |_, a, _| {
        check_args(&a, 1, 1)?;
        let (r, c) = a[0].dims();
        one(Value::num(if r == 0 || c == 0 {
            0.0
        } else {
            r.max(c) as f64
        }))
    });
    b.insert("ndims", |_, _, _| one(Value::num(2.0)));
    b.insert("isempty", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(a[0].is_empty()))
    });
    b.insert("isscalar", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(a[0].numel() == 1))
    });
    b.insert("isvector", |_, a, _| {
        check_args(&a, 1, 1)?;
        let (r, c) = a[0].dims();
        one(bool_val((r == 1 || c == 1) && r * c >= 1))
    });
    b.insert("isrow", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(a[0].dims().0 == 1))
    });
    b.insert("iscolumn", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(a[0].dims().1 == 1))
    });
    b.insert("ismatrix", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(true))
    });
    b.insert("issquare", |_, a, _| {
        check_args(&a, 1, 1)?;
        let (r, c) = a[0].dims();
        one(bool_val(r == c))
    });
    b.insert("class", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::chars(a[0].class_name()))
    });
    b.insert("isa", isa);
    b.insert("isnumeric", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(
            matches!(&a[0], Value::Num(_, c) if *c != NumClass::Logical),
        ))
    });
    b.insert("isfloat", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(matches!(
            &a[0],
            Value::Num(_, NumClass::Double | NumClass::Single)
        )))
    });
    b.insert("isinteger", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(matches!(&a[0], Value::Num(_, NumClass::Int(_)))))
    });
    b.insert("isreal", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(true))
    });
    b.insert("islogical", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(matches!(&a[0], Value::Num(_, NumClass::Logical))))
    });
    b.insert("ischar", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(matches!(&a[0], Value::Char(_))))
    });
    b.insert("isstring", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(matches!(&a[0], Value::Str(_))))
    });
    b.insert("iscell", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(matches!(&a[0], Value::Cell(_))))
    });
    b.insert("isstruct", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(matches!(&a[0], Value::Struct(_))))
    });
    b.insert("is_function_handle", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(matches!(&a[0], Value::Func(_))))
    });
    b.insert("double", |_, a, _| {
        check_args(&a, 1, 1)?;
        to_class(&a[0], "double")
    });
    b.insert("single", |_, a, _| {
        check_args(&a, 1, 1)?;
        to_class(&a[0], "single")
    });
    b.insert("logical", logical);
    b.insert("int8", |_, a, _| {
        check_args(&a, 1, 1)?;
        to_class(&a[0], "int8")
    });
    b.insert("int16", |_, a, _| {
        check_args(&a, 1, 1)?;
        to_class(&a[0], "int16")
    });
    b.insert("int32", |_, a, _| {
        check_args(&a, 1, 1)?;
        to_class(&a[0], "int32")
    });
    b.insert("int64", |_, a, _| {
        check_args(&a, 1, 1)?;
        to_class(&a[0], "int64")
    });
    b.insert("uint8", |_, a, _| {
        check_args(&a, 1, 1)?;
        to_class(&a[0], "uint8")
    });
    b.insert("uint16", |_, a, _| {
        check_args(&a, 1, 1)?;
        to_class(&a[0], "uint16")
    });
    b.insert("uint32", |_, a, _| {
        check_args(&a, 1, 1)?;
        to_class(&a[0], "uint32")
    });
    b.insert("uint64", |_, a, _| {
        check_args(&a, 1, 1)?;
        to_class(&a[0], "uint64")
    });
    b.insert("cast", |_, a, _| {
        check_args(&a, 2, 2)?;
        let c = a[1].text()?;
        to_class(&a[0], &c)
    });
    b.insert("and", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::And, &a[0], &a[1])?)
    });
    b.insert("or", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(ops::binary(crate::ast::BinOp::Or, &a[0], &a[1])?)
    });
    b.insert("not", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(ops::unary(crate::ast::UnOp::Not, &a[0])?)
    });
    b.insert("xor", |_, a, _| {
        check_args(&a, 2, 2)?;
        let x = a[0].to_num()?;
        let y = a[1].to_num()?;
        one(Value::logical(ops::broadcast(
            &x,
            &y,
            |p, q| if (p != 0.0) != (q != 0.0) { 1.0 } else { 0.0 },
            "xor",
        )?))
    });
    b.insert("any", any);
    b.insert("all", all);
    b.insert("isequal", |_, a, _| {
        if a.len() < 2 {
            return Err(merr!("Not enough input arguments."));
        }
        let nan_free = a.iter().all(|v| {
            v.to_num()
                .map_or(true, |n| !n.data.iter().any(|x| x.is_nan()))
        });
        one(bool_val(
            nan_free && a.windows(2).all(|w| values_equal(&w[0], &w[1])),
        ))
    });
    b.insert("isequaln", |_, a, _| {
        if a.len() < 2 {
            return Err(merr!("Not enough input arguments."));
        }
        let norm = |v: &Value| match v {
            Value::Num(n, c) => Value::Num(n.map(|x| if x.is_nan() { f64::MAX } else { *x }), *c),
            o => o.clone(),
        };
        one(bool_val(
            a.windows(2)
                .all(|w| values_equal(&norm(&w[0]), &norm(&w[1]))),
        ))
    });
    b.insert("find", find);
    b.insert("nnz", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::num(
            a[0].to_num()?.data.iter().filter(|x| **x != 0.0).count() as f64,
        ))
    });
    b.insert("sort", sort);
    b.insert("sortrows", sortrows);
    b.insert("unique", unique);
    b.insert("ismember", ismember);
    b.insert("setdiff", |it, a, n| setop(it, a, n, "setdiff"));
    b.insert("intersect", |it, a, n| setop(it, a, n, "intersect"));
    b.insert("union", |it, a, n| setop(it, a, n, "union"));
    b.insert("isnan", |_, a, _| {
        check_args(&a, 1, 1)?;
        num_pred(&a[0], |x| x.is_nan())
    });
    b.insert("isinf", |_, a, _| {
        check_args(&a, 1, 1)?;
        num_pred(&a[0], |x| x.is_infinite())
    });
    b.insert("isfinite", |_, a, _| {
        check_args(&a, 1, 1)?;
        num_pred(&a[0], |x| x.is_finite())
    });
    b.insert("sub2ind", sub2ind);
    b.insert("ind2sub", ind2sub);
}

fn fill(args: Vec<Value>, v: f64) -> R {
    let (r, c, class) = size_args(&args)?;
    one(with_class(Array::filled(r, c, v), class.as_deref())?)
}

fn zeros(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    fill(a, 0.0)
}
fn ones(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    fill(a, 1.0)
}
fn nan(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    fill(a, f64::NAN)
}
fn inf(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    fill(a, f64::INFINITY)
}
fn pi(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    fill(a, std::f64::consts::PI)
}
fn true_(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let (r, c, _) = size_args(&a)?;
    one(Value::logical(Array::filled(r, c, 1.0)))
}
fn false_(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let (r, c, _) = size_args(&a)?;
    one(Value::logical(Array::filled(r, c, 0.0)))
}
fn eps(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if let Some(v) = a.first() {
        if !v.is_text() {
            let x = v.to_num()?;
            return one(Value::mat(x.map(|x| {
                let x = x.abs();
                if x == 0.0 {
                    f64::from_bits(1)
                } else {
                    let e = x.log2().floor();
                    2f64.powf(e - 52.0)
                }
            })));
        }
    }
    one(Value::num(f64::EPSILON))
}

fn intmax(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let name = if a.is_empty() {
        "int32".to_string()
    } else {
        a[0].text()?
    };
    let k = IntKind::from_name(&name).ok_or_else(|| merr!("Invalid class name '{}'", name))?;
    one(Value::Num(Array::scalar(k.range().1), NumClass::Int(k)))
}

fn intmin(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let name = if a.is_empty() {
        "int32".to_string()
    } else {
        a[0].text()?
    };
    let k = IntKind::from_name(&name).ok_or_else(|| merr!("Invalid class name '{}'", name))?;
    one(Value::Num(Array::scalar(k.range().0), NumClass::Int(k)))
}

fn eye(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let (r, c, class) = size_args(&a)?;
    let mut m = Array::filled(r, c, 0.0);
    for i in 0..r.min(c) {
        m.set(i, i, 1.0);
    }
    one(with_class(m, class.as_deref())?)
}

fn rand(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let (r, c, _) = size_args(&a)?;
    let data = (0..r * c).map(|_| it.rng.gen::<f64>()).collect();
    one(Value::mat(Array::new(r, c, data)))
}

fn randn(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let (r, c, _) = size_args(&a)?;
    let data = (0..r * c)
        .map(|_| StandardNormal.sample(&mut it.rng))
        .collect();
    one(Value::mat(Array::new(r, c, data)))
}

fn randi(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let lim = a[0].to_num()?;
    let (lo, hi) = if lim.numel() >= 2 {
        (lim.data[0], lim.data[1])
    } else {
        (1.0, lim.data[0])
    };
    if hi < lo {
        return Err(merr!("First input must be a positive scalar integer value IMAX, or two integer values [IMIN IMAX] with IMIN less than or equal to IMAX."));
    }
    let (r, c, _) = size_args(&a[1..])?;
    let data = (0..r * c)
        .map(|_| it.rng.gen_range(lo as i64..=hi as i64) as f64)
        .collect();
    one(Value::mat(Array::new(r, c, data)))
}

fn randperm(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 2)?;
    let n = a[0].usize_scalar("N")?;
    let k = if a.len() > 1 {
        a[1].usize_scalar("K")?
    } else {
        n
    };
    let mut v: Vec<f64> = (1..=n).map(|x| x as f64).collect();
    for i in 0..k.min(n) {
        let j = it.rng.gen_range(i..n);
        v.swap(i, j);
    }
    v.truncate(k);
    one(Value::row(v))
}

fn linspace(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 2, 3)?;
    let lo = a[0].scalar()?;
    let hi = a[1].scalar()?;
    let n = if a.len() > 2 {
        a[2].scalar()?.floor().max(0.0) as usize
    } else {
        100
    };
    if n == 1 {
        return one(Value::num(hi));
    }
    let v = (0..n)
        .map(|i| {
            if i == n - 1 {
                hi
            } else {
                lo + (hi - lo) * i as f64 / (n - 1) as f64
            }
        })
        .collect();
    one(Value::row(v))
}

fn logspace(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 2, 3)?;
    let lo = a[0].scalar()?;
    let hi = a[1].scalar()?;
    let n = if a.len() > 2 {
        a[2].scalar()? as usize
    } else {
        50
    };
    let v = (0..n)
        .map(|i| {
            10f64.powf(if n == 1 {
                hi
            } else {
                lo + (hi - lo) * i as f64 / (n - 1) as f64
            })
        })
        .collect();
    one(Value::row(v))
}

fn colon(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 2, 3)?;
    let (s, st, e) = if a.len() == 2 {
        (a[0].scalar()?, 1.0, a[1].scalar()?)
    } else {
        (a[0].scalar()?, a[1].scalar()?, a[2].scalar()?)
    };
    one(Value::mat(ops::range(s, st, e)?))
}

fn magic(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 1)?;
    let n = a[0].usize_scalar("N")?;
    one(Value::mat(magic_square(n)))
}

pub fn magic_square(n: usize) -> Array<f64> {
    if n == 0 {
        return Array::empty();
    }
    if n == 1 {
        return Array::scalar(1.0);
    }
    if n == 2 {
        return Array::new(2, 2, vec![4.0, 3.0, 1.0, 2.0]);
    }
    let mut m = Array::filled(n, n, 0.0);
    if n % 2 == 1 {
        // Siamese method.
        let (mut i, mut j) = (0usize, n / 2);
        for k in 1..=n * n {
            m.set(i, j, k as f64);
            let ni = (i + n - 1) % n;
            let nj = (j + 1) % n;
            if *m.get(ni, nj) != 0.0 {
                i = (i + 1) % n;
            } else {
                i = ni;
                j = nj;
            }
        }
    } else if n.is_multiple_of(4) {
        for i in 0..n {
            for j in 0..n {
                let v = (i * n + j + 1) as f64;
                let keep = ((i % 4 == 0 || i % 4 == 3) && (j % 4 == 0 || j % 4 == 3))
                    || ((i % 4 == 1 || i % 4 == 2) && (j % 4 == 1 || j % 4 == 2));
                m.set(i, j, if keep { (n * n + 1) as f64 - v } else { v });
            }
        }
    } else {
        // LUX method for singly even order.
        let h = n / 2;
        let sub = magic_square(h);
        let add = [0.0, 2.0, 3.0, 1.0];
        for i in 0..h {
            for j in 0..h {
                let v = *sub.get(i, j);
                let hh = (h * h) as f64;
                m.set(i, j, v + add[0] * hh);
                m.set(i + h, j + h, v + add[3] * hh);
                m.set(i, j + h, v + add[1] * hh);
                m.set(i + h, j, v + add[2] * hh);
            }
        }
        let k = (n - 2) / 4;
        for i in 0..h {
            let cols: Vec<usize> = if i == h / 2 {
                (1..=k).collect()
            } else {
                (0..k).collect()
            };
            let mut cols = cols;
            cols.extend((n - k + 1)..n);
            for j in cols {
                let a = *m.get(i, j);
                let b = *m.get(i + h, j);
                m.set(i, j, b);
                m.set(i + h, j, a);
            }
        }
    }
    m
}

fn repmat(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let (m, n, _) = size_args(&a[1..])?;
    let base = &a[0];
    let row: Vec<Value> = (0..n).map(|_| base.clone()).collect();
    let r = ops::horzcat(row)?;
    let col: Vec<Value> = (0..m).map(|_| r.clone()).collect();
    if m == 0 || n == 0 {
        let (br, bc) = base.dims();
        return one(base
            .index(&[Idx::List(vec![], (0, 1)), Idx::List(vec![], (1, 0))])?
            .reshape(br * m, bc * n)
            .unwrap_or_else(|_| Value::empty()));
    }
    one(ops::vertcat(col)?)
}

fn reshape(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let n = a[0].numel();
    let (r, c) = if a.len() == 2 {
        let d = a[1].to_num()?;
        if d.numel() < 2 {
            return Err(merr!("Size vector must have at least two elements."));
        }
        (d.data[0] as usize, d.data[1] as usize)
    } else {
        let empty_r = a[1].is_empty();
        let empty_c = a[2].is_empty();
        let r = if empty_r { 0 } else { a[1].scalar()? as usize };
        let c = if empty_c { 0 } else { a[2].scalar()? as usize };
        if empty_r {
            (n.checked_div(c).unwrap_or(0), c)
        } else if empty_c {
            (r, n.checked_div(r).unwrap_or(0))
        } else {
            (r, c)
        }
    };
    one(a[0].reshape(r, c)?)
}

fn cat(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let dim = a[0].scalar()?;
    let rest = a[1..].to_vec();
    one(if dim == 1.0 {
        ops::vertcat(rest)?
    } else {
        ops::horzcat(rest)?
    })
}

fn diag(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 2)?;
    let m = a[0].to_num()?;
    let k = if a.len() > 1 {
        a[1].scalar()? as i64
    } else {
        0
    };
    if m.is_vector() {
        let n = m.numel() + k.unsigned_abs() as usize;
        let mut out = Array::filled(n, n, 0.0);
        for (i, v) in m.data.iter().enumerate() {
            let (r, c) = if k >= 0 {
                (i, i + k as usize)
            } else {
                (i + (-k) as usize, i)
            };
            out.set(r, c, *v);
        }
        return one(Value::mat(out));
    }
    let mut v = Vec::new();
    for i in 0..m.rows {
        let j = i as i64 + k;
        if j >= 0 && (j as usize) < m.cols {
            v.push(*m.get(i, j as usize));
        }
    }
    one(Value::col(v))
}

fn tri(a: &[Value], lower: bool) -> R {
    check_args(a, 1, 2)?;
    let m = a[0].to_num()?;
    let k = if a.len() > 1 {
        a[1].scalar()? as i64
    } else {
        0
    };
    let mut out = m.clone();
    for c in 0..m.cols {
        for r in 0..m.rows {
            let d = c as i64 - r as i64;
            let keep = if lower { d <= k } else { d >= k };
            if !keep {
                out.set(r, c, 0.0);
            }
        }
    }
    one(Value::Num(out, a[0].num_class()))
}

fn tril(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    tri(&a, true)
}
fn triu(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    tri(&a, false)
}

fn flip_dim(v: &Value, dim: usize) -> MResult<Value> {
    let (r, c) = v.dims();
    if dim == 1 {
        v.index(&[Idx::List((0..r).rev().collect(), (r, 1)), Idx::All])
    } else {
        v.index(&[Idx::All, Idx::List((0..c).rev().collect(), (1, c))])
    }
}

fn fliplr(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 1)?;
    one(flip_dim(&a[0], 2)?)
}
fn flipud(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 1)?;
    one(flip_dim(&a[0], 1)?)
}
fn flip(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 2)?;
    let dim = if a.len() > 1 {
        a[1].scalar()? as usize
    } else if a[0].dims().0 != 1 {
        1
    } else {
        2
    };
    one(flip_dim(&a[0], dim)?)
}

fn rot90(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 2)?;
    let k = if a.len() > 1 {
        a[1].scalar()? as i64
    } else {
        1
    };
    let mut v = a[0].clone();
    for _ in 0..k.rem_euclid(4) {
        v = flip_dim(&v.transpose()?, 1)?;
    }
    one(v)
}

fn circshift(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 2, 3)?;
    let v = &a[0];
    let (r, c) = v.dims();
    let sh = a[1].to_num()?;
    let (sr, sc) = if a.len() == 3 {
        let d = a[2].scalar()?;
        if d == 1.0 {
            (sh.data[0] as i64, 0)
        } else {
            (0, sh.data[0] as i64)
        }
    } else if sh.numel() >= 2 {
        (sh.data[0] as i64, sh.data[1] as i64)
    } else if r == 1 {
        (0, sh.data[0] as i64)
    } else {
        (sh.data[0] as i64, 0)
    };
    let ri: Vec<usize> = (0..r)
        .map(|i| ((i as i64 - sr).rem_euclid(r.max(1) as i64)) as usize)
        .collect();
    let ci: Vec<usize> = (0..c)
        .map(|j| ((j as i64 - sc).rem_euclid(c.max(1) as i64)) as usize)
        .collect();
    one(v.index(&[Idx::List(ri, (r, 1)), Idx::List(ci, (1, c))])?)
}

fn meshgrid(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 3)?;
    let x = a[0].to_num()?;
    let y = if a.len() > 1 {
        a[1].to_num()?
    } else {
        x.clone()
    };
    let (nx, ny) = (x.numel(), y.numel());
    let mut xx = Array::filled(ny, nx, 0.0);
    let mut yy = Array::filled(ny, nx, 0.0);
    for i in 0..ny {
        for j in 0..nx {
            xx.set(i, j, x.data[j]);
            yy.set(i, j, y.data[i]);
        }
    }
    let mut out = vec![Value::mat(xx)];
    if nargout > 1 {
        out.push(Value::mat(yy));
    }
    Ok(out)
}

fn ndgrid(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    let mut swapped = a.clone();
    if swapped.len() >= 2 {
        swapped.swap(0, 1);
    } else if swapped.len() == 1 {
        swapped.push(a[0].clone());
    }
    // meshgrid(y, x) yields (Y-like, X-like); ndgrid wants X varying along rows.
    let r = meshgrid(it, swapped, 2)?;
    let mut out = vec![r[1].clone(), r[0].clone()];
    out.truncate(nargout.max(1));
    Ok(out)
}

fn size(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 2)?;
    let (r, c) = a[0].dims();
    if a.len() == 2 {
        let d = a[1].to_num()?;
        let vals: Vec<f64> = d
            .data
            .iter()
            .map(|k| match *k as usize {
                1 => r as f64,
                2 => c as f64,
                _ => 1.0,
            })
            .collect();
        return one(Value::row(vals));
    }
    if nargout <= 1 {
        return one(Value::row(vec![r as f64, c as f64]));
    }
    let mut out = vec![Value::num(r as f64), Value::num(c as f64)];
    while out.len() < nargout {
        out.push(Value::num(1.0));
    }
    Ok(out)
}

fn isa(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 2, 2)?;
    let cls = a[1].text()?;
    let name = a[0].class_name();
    let r = match cls.as_str() {
        "numeric" => matches!(&a[0], Value::Num(_, c) if *c != NumClass::Logical),
        "float" => matches!(&a[0], Value::Num(_, NumClass::Double | NumClass::Single)),
        "integer" => matches!(&a[0], Value::Num(_, NumClass::Int(_))),
        c => c == name || matches!(&a[0], Value::Inst(o) if o.class.isa(c)),
    };
    one(bool_val(r))
}

pub fn to_class(v: &Value, class: &str) -> R {
    match (v, class) {
        (Value::Str(s), "double") => {
            let data = s
                .data
                .iter()
                .map(|t| t.trim().parse::<f64>().unwrap_or(f64::NAN))
                .collect();
            one(Value::mat(Array::new(s.rows, s.cols, data)))
        }
        (Value::Cell(_), _) | (Value::Struct(_), _) | (Value::Func(_), _) => Err(merr!(
            "Conversion to {} from {} is not possible.",
            class,
            v.class_name()
        )),
        (Value::Num(a, _), "logical") => {
            if a.data.iter().any(|x| x.is_nan()) {
                return Err(merr!("NaN's cannot be converted to logicals."));
            }
            one(with_class(a.clone(), Some("logical"))?)
        }
        _ => one(with_class(v.to_num()?, Some(class))?),
    }
}

fn logical(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 1)?;
    to_class(&a[0], "logical")
}

fn anyall(a: &[Value], is_any: bool) -> R {
    check_args(a, 1, 2)?;
    let m = a[0].to_num()?;
    if a.len() > 1 && a[1].is_text() && a[1].text()? == "all" {
        let r = if is_any {
            m.data.iter().any(|x| *x != 0.0 && !x.is_nan())
        } else {
            m.data.iter().all(|x| *x != 0.0)
        };
        return one(bool_val(r));
    }
    let dim = pick_dim(
        &m,
        if a.len() > 1 {
            Some(a[1].scalar()?)
        } else {
            None
        },
    )?;
    let f = |s: &[f64]| -> f64 {
        let r = if is_any {
            s.iter().any(|x| *x != 0.0 && !x.is_nan())
        } else {
            s.iter().all(|x| *x != 0.0)
        };
        if r {
            1.0
        } else {
            0.0
        }
    };
    if m.is_vector() && a.len() == 1 {
        return one(Value::logical(Array::scalar(f(&m.data))));
    }
    one(Value::logical(reduce(&m, dim, f)))
}

fn any(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    anyall(&a, true)
}
fn all(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    anyall(&a, false)
}

fn find(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 3)?;
    let m = a[0].to_num()?;
    let limit = if a.len() > 1 {
        Some(a[1].scalar()? as usize)
    } else {
        None
    };
    let last = a.len() > 2 && a[2].text()? == "last";
    let mut idx: Vec<usize> = m
        .data
        .iter()
        .enumerate()
        .filter(|(_, x)| **x != 0.0)
        .map(|(i, _)| i)
        .collect();
    if let Some(k) = limit {
        if last {
            let s = idx.len().saturating_sub(k);
            idx = idx[s..].to_vec();
        } else {
            idx.truncate(k);
        }
    }
    let n = idx.len();
    let shape = |v: Vec<f64>| -> Value {
        if m.rows == 1 && m.cols != 1 {
            Value::row(v)
        } else if m.rows == 0 && m.cols == 0 {
            Value::Num(Array::new(0, 0, vec![]), NumClass::Double)
        } else {
            Value::col(v)
        }
    };
    if nargout <= 1 {
        let v: Vec<f64> = idx.iter().map(|i| (*i + 1) as f64).collect();
        let r = if n == 0 && m.rows == 1 {
            Value::Num(Array::new(1, 0, vec![]), NumClass::Double)
        } else {
            shape(v)
        };
        return one(r);
    }
    let rows: Vec<f64> = idx.iter().map(|i| (i % m.rows.max(1) + 1) as f64).collect();
    let cols: Vec<f64> = idx.iter().map(|i| (i / m.rows.max(1) + 1) as f64).collect();
    let vals: Vec<f64> = idx.iter().map(|i| m.data[*i]).collect();
    let mut out = vec![shape(rows), shape(cols)];
    if nargout > 2 {
        out.push(shape(vals));
    }
    Ok(out)
}

fn sort_perm(data: &[f64], desc: bool) -> Vec<usize> {
    let mut p: Vec<usize> = (0..data.len()).collect();
    p.sort_by(|&i, &j| {
        let (x, y) = (data[i], data[j]);
        // NaN goes last in ascending, first in descending.
        let o = match (x.is_nan(), y.is_nan()) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => x.partial_cmp(&y).unwrap(),
        };
        if desc {
            o.reverse()
        } else {
            o
        }
    });
    p
}

fn sort(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 3)?;
    let mut desc = false;
    let mut dim = None;
    for x in &a[1..] {
        if x.is_text() {
            desc = x.text()?.eq_ignore_ascii_case("descend");
        } else {
            dim = Some(x.scalar()?);
        }
    }
    if let Value::Cell(_) | Value::Str(_) = &a[0] {
        let list = cellstr_list(&a[0])?;
        let mut p: Vec<usize> = (0..list.len()).collect();
        p.sort_by(|&i, &j| {
            if desc {
                list[j].cmp(&list[i])
            } else {
                list[i].cmp(&list[j])
            }
        });
        let (r, c) = a[0].dims();
        let sorted: Vec<Value> = p.iter().map(|&i| Value::chars(&list[i])).collect();
        let out = match &a[0] {
            Value::Str(_) => Value::Str(Array::new(
                r,
                c,
                p.iter().map(|&i| list[i].clone()).collect(),
            )),
            _ => Value::Cell(Array::new(r, c, sorted)),
        };
        let mut res = vec![out];
        if nargout > 1 {
            res.push(Value::mat(Array::new(
                r,
                c,
                p.iter().map(|&i| (i + 1) as f64).collect(),
            )));
        }
        return Ok(res);
    }
    let m = a[0].to_num()?;
    let d = pick_dim(&m, dim)?;
    let mut out = m.clone();
    let mut idx = Array::filled(m.rows, m.cols, 0.0);
    if d == 1 {
        for c in 0..m.cols {
            let col = &m.data[c * m.rows..(c + 1) * m.rows];
            let p = sort_perm(col, desc);
            for (k, &i) in p.iter().enumerate() {
                out.data[c * m.rows + k] = col[i];
                idx.data[c * m.rows + k] = (i + 1) as f64;
            }
        }
    } else {
        for r in 0..m.rows {
            let row: Vec<f64> = (0..m.cols).map(|c| *m.get(r, c)).collect();
            let p = sort_perm(&row, desc);
            for (k, &i) in p.iter().enumerate() {
                out.set(r, k, row[i]);
                idx.set(r, k, (i + 1) as f64);
            }
        }
    }
    let class = a[0].num_class();
    let mut res = vec![if let Value::Char(_) = a[0] {
        Value::Char(out.map(|x| char::from_u32(*x as u32).unwrap_or('\0')))
    } else {
        Value::Num(out, class)
    }];
    if nargout > 1 {
        res.push(Value::mat(idx));
    }
    Ok(res)
}

fn sortrows(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 2)?;
    let m = a[0].to_num()?;
    let cols: Vec<i64> = if a.len() > 1 {
        a[1].to_num()?.data.iter().map(|x| *x as i64).collect()
    } else {
        (1..=m.cols as i64).collect()
    };
    let mut p: Vec<usize> = (0..m.rows).collect();
    p.sort_by(|&i, &j| {
        for &c in &cols {
            let k = (c.unsigned_abs() as usize) - 1;
            let (x, y) = (*m.get(i, k), *m.get(j, k));
            let o = x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal);
            let o = if c < 0 { o.reverse() } else { o };
            if o != std::cmp::Ordering::Equal {
                return o;
            }
        }
        std::cmp::Ordering::Equal
    });
    let out = a[0].index(&[Idx::List(p.clone(), (p.len(), 1)), Idx::All])?;
    let mut res = vec![out];
    if nargout > 1 {
        res.push(Value::col(p.iter().map(|i| (*i + 1) as f64).collect()));
    }
    Ok(res)
}

fn unique(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 1, 3)?;
    let stable = a[1..].iter().any(|x| x.text().is_ok_and(|t| t == "stable"));
    if let Value::Cell(_) | Value::Str(_) = &a[0] {
        let list = cellstr_list(&a[0])?;
        let mut order: Vec<usize> = (0..list.len()).collect();
        if !stable {
            order.sort_by(|&i, &j| list[i].cmp(&list[j]).then(i.cmp(&j)));
        }
        let mut seen: Vec<String> = Vec::new();
        let mut ia = Vec::new();
        for &i in &order {
            if !seen.contains(&list[i]) {
                seen.push(list[i].clone());
                ia.push(i);
            }
        }
        let ic: Vec<f64> = list
            .iter()
            .map(|s| (seen.iter().position(|x| x == s).unwrap() + 1) as f64)
            .collect();
        let n = seen.len();
        let out = match &a[0] {
            Value::Str(_) => Value::Str(Array::row(seen)),
            _ => {
                let vals: Vec<Value> = seen.iter().map(|s| Value::chars(s)).collect();
                if a[0].dims().0 == 1 {
                    Value::Cell(Array::row(vals))
                } else {
                    Value::Cell(Array::new(n, 1, vals))
                }
            }
        };
        let mut res = vec![
            out,
            Value::col(ia.iter().map(|i| (*i + 1) as f64).collect()),
            Value::col(ic),
        ];
        res.truncate(nargout.max(1));
        return Ok(res);
    }
    let m = a[0].to_num()?;
    let mut order: Vec<usize> = (0..m.numel()).collect();
    if !stable {
        order = sort_perm(&m.data, false);
    }
    let mut vals: Vec<f64> = Vec::new();
    let mut ia = Vec::new();
    for &i in &order {
        let x = m.data[i];
        // NaNs never compare equal, so each NaN stays distinct.
        if !vals.contains(&x) {
            vals.push(x);
            ia.push(i);
        }
    }
    let ic: Vec<f64> = m
        .data
        .iter()
        .map(|x| (vals.iter().position(|v| v == x).unwrap_or(0) + 1) as f64)
        .collect();
    let out = if m.rows == 1 {
        Array::row(vals)
    } else {
        Array::col(vals)
    };
    let class = a[0].num_class();
    let mut res = vec![
        if let Value::Char(_) = a[0] {
            Value::Char(out.map(|x| char::from_u32(*x as u32).unwrap_or('\0')))
        } else {
            Value::Num(out, class)
        },
        Value::col(ia.iter().map(|i| (*i + 1) as f64).collect()),
        Value::col(ic),
    ];
    res.truncate(nargout.max(1));
    Ok(res)
}

fn ismember(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 2, 2)?;
    if a[0].is_text() || matches!(a[0], Value::Cell(_) | Value::Str(_)) {
        let set = cellstr_list(&a[1])?;
        let items = if a[0].is_text() && !matches!(a[0], Value::Str(_)) {
            vec![a[0].text()?]
        } else {
            cellstr_list(&a[0])?
        };
        let (r, c) = if a[0].is_text() { (1, 1) } else { a[0].dims() };
        let loc: Vec<f64> = items
            .iter()
            .map(|s| {
                set.iter()
                    .position(|x| x == s)
                    .map_or(0.0, |i| (i + 1) as f64)
            })
            .collect();
        let tf = Array::new(
            r,
            c,
            loc.iter()
                .map(|x| if *x > 0.0 { 1.0 } else { 0.0 })
                .collect(),
        );
        let mut out = vec![Value::logical(tf)];
        if nargout > 1 {
            out.push(Value::mat(Array::new(r, c, loc)));
        }
        return Ok(out);
    }
    let x = a[0].to_num()?;
    let s = a[1].to_num()?;
    let loc: Vec<f64> = x
        .data
        .iter()
        .map(|v| {
            s.data
                .iter()
                .position(|y| y == v)
                .map_or(0.0, |i| (i + 1) as f64)
        })
        .collect();
    let mut out = vec![Value::logical(Array::new(
        x.rows,
        x.cols,
        loc.iter()
            .map(|l| if *l > 0.0 { 1.0 } else { 0.0 })
            .collect(),
    ))];
    if nargout > 1 {
        out.push(Value::mat(Array::new(x.rows, x.cols, loc)));
    }
    Ok(out)
}

fn setop(_: &mut Interp, a: Vec<Value>, _: usize, op: &str) -> R {
    check_args(&a, 2, 3)?;
    let text_mode = matches!(a[0], Value::Cell(_) | Value::Str(_))
        || matches!(a[1], Value::Cell(_) | Value::Str(_));
    if text_mode {
        let x = cellstr_list(&a[0])?;
        let y = cellstr_list(&a[1])?;
        let mut r: Vec<String> = match op {
            "setdiff" => x.into_iter().filter(|s| !y.contains(s)).collect(),
            "intersect" => x.into_iter().filter(|s| y.contains(s)).collect(),
            _ => x.into_iter().chain(y).collect(),
        };
        r.sort();
        r.dedup();
        return one(Value::Cell(Array::row(
            r.iter().map(|s| Value::chars(s)).collect(),
        )));
    }
    let x = a[0].to_num()?;
    let y = a[1].to_num()?;
    let mut r: Vec<f64> = match op {
        "setdiff" => x
            .data
            .iter()
            .copied()
            .filter(|v| !y.data.contains(v))
            .collect(),
        "intersect" => x
            .data
            .iter()
            .copied()
            .filter(|v| y.data.contains(v))
            .collect(),
        _ => x.data.iter().chain(y.data.iter()).copied().collect(),
    };
    r.sort_by(|p, q| p.partial_cmp(q).unwrap_or(std::cmp::Ordering::Equal));
    r.dedup();
    let column = x.cols == 1 && x.rows > 1 && (op != "union" || y.cols == 1);
    one(if column { Value::col(r) } else { Value::row(r) })
}

fn num_pred(v: &Value, f: fn(f64) -> bool) -> R {
    match v {
        Value::Num(a, _) => one(Value::logical(a.map(|x| if f(*x) { 1.0 } else { 0.0 }))),
        Value::Char(a) => one(Value::logical(a.map(|_| 0.0))),
        other => Err(merr!(
            "Undefined function for input arguments of type '{}'.",
            other.class_name()
        )),
    }
}

fn sub2ind(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 3, 3)?;
    let sz = a[0].to_num()?;
    let r = a[1].to_num()?;
    let c = a[2].to_num()?;
    let rows = sz.data[0];
    let out = ops::broadcast(&r, &c, |i, j| (j - 1.0) * rows + i, "sub2ind")?;
    one(Value::mat(out))
}

fn ind2sub(_: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    check_args(&a, 2, 2)?;
    let sz = a[0].to_num()?;
    let ind = a[1].to_num()?;
    let rows = sz.data[0];
    if nargout <= 1 {
        return one(Value::Num(ind, NumClass::Double));
    }
    let r = ind.map(|k| ((k - 1.0) % rows) + 1.0);
    let c = ind.map(|k| ((k - 1.0) / rows).floor() + 1.0);
    Ok(vec![Value::mat(r), Value::mat(c)])
}
