//! Built-in function library.

mod core;
mod data;
mod math;
pub mod objects;
mod plot;
mod stats;
mod system;
mod text;

use crate::error::MResult;
use crate::interp::Interp;
use crate::merr;
use crate::value::*;

pub use objects::object_text;
pub use system::bytes_of;
pub use system::resolve as resolve_path;
pub use text::format_printf;

pub fn register(it: &mut Interp) {
    core::register(it);
    math::register(it);
    text::register(it);
    data::register(it);
    system::register(it);
    plot::register(it);
    stats::register(it);
    objects::register(it);
    crate::sim::run::register(it);
    crate::sim::ode::register(it);
}

pub(crate) type R = MResult<Vec<Value>>;

pub(crate) fn one(v: Value) -> R {
    Ok(vec![v])
}

pub(crate) fn none() -> R {
    Ok(vec![])
}

pub(crate) fn check_args(args: &[Value], min: usize, max: usize) -> MResult<()> {
    if args.len() < min {
        return Err(merr!("Not enough input arguments."));
    }
    if args.len() > max {
        return Err(merr!("Too many input arguments."));
    }
    Ok(())
}

pub(crate) fn num_arg(args: &[Value], i: usize) -> MResult<Array<f64>> {
    args.get(i)
        .ok_or_else(|| merr!("Not enough input arguments."))?
        .to_num()
}

pub(crate) fn bool_val(b: bool) -> Value {
    Value::boolean(b)
}

/// Parse size arguments: f(n), f(m,n), f([m n]), with optional trailing class name / 'like'.
pub(crate) fn size_args(args: &[Value]) -> MResult<(usize, usize, Option<String>)> {
    let mut nums = Vec::new();
    let mut class = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a.is_text() {
            let t = a.text()?;
            if t == "like" {
                class = args.get(i + 1).map(|v| v.class_name().to_string());
                i += 2;
                continue;
            }
            class = Some(t);
        } else {
            nums.push(a.to_num()?);
        }
        i += 1;
    }
    let to_n = |x: f64| -> MResult<usize> {
        if x.is_nan() {
            return Err(merr!("Size inputs must be nonnegative integers"));
        }
        Ok(if x < 0.0 { 0 } else { x as usize })
    };
    let (r, c) = match nums.len() {
        0 => (1, 1),
        1 => {
            let a = &nums[0];
            match a.numel() {
                1 => (to_n(a.data[0])?, to_n(a.data[0])?),
                0 => (0, 0),
                _ => {
                    for extra in &a.data[2..] {
                        if *extra != 1.0 {
                            return Err(merr!("N-dimensional arrays are not supported yet"));
                        }
                    }
                    (to_n(a.data[0])?, to_n(a.data[1])?)
                }
            }
        }
        _ => {
            for extra in &nums[2..] {
                if extra.data.first().copied().unwrap_or(1.0) != 1.0 {
                    return Err(merr!("N-dimensional arrays are not supported yet"));
                }
            }
            (to_n(nums[0].scalar_or(0.0))?, to_n(nums[1].scalar_or(0.0))?)
        }
    };
    if r.saturating_mul(c) > 500_000_000 {
        return Err(merr!(
            "Requested {}x{} array exceeds maximum array size preference.",
            r,
            c
        ));
    }
    Ok((r, c, class))
}

pub(crate) trait ScalarOr {
    fn scalar_or(&self, d: f64) -> f64;
}

impl ScalarOr for Array<f64> {
    fn scalar_or(&self, d: f64) -> f64 {
        self.data.first().copied().unwrap_or(d)
    }
}

/// Apply a class name (from zeros(..., 'int32') etc.) to a numeric array.
pub(crate) fn with_class(a: Array<f64>, class: Option<&str>) -> MResult<Value> {
    Ok(match class {
        None | Some("double") => Value::mat(a),
        Some("single") => Value::Num(a.map(|x| (*x as f32) as f64), NumClass::Single),
        Some("logical") => Value::logical(a.map(|x| if *x != 0.0 { 1.0 } else { 0.0 })),
        Some(c) => match IntKind::from_name(c) {
            Some(k) => Value::Num(a.map(|x| k.cast(*x)), NumClass::Int(k)),
            None => return Err(merr!("Invalid class name '{}'", c)),
        },
    })
}

/// Dimension to operate along: explicit `dim` or the first non-singleton dimension.
pub(crate) fn pick_dim(a: &Array<f64>, dim: Option<f64>) -> MResult<usize> {
    match dim {
        Some(d) => {
            if d != 1.0 && d != 2.0 {
                if d >= 3.0 && d.fract() == 0.0 {
                    return Ok(3);
                }
                return Err(merr!(
                    "Dimension argument must be a positive integer scalar within indexing range."
                ));
            }
            Ok(d as usize)
        }
        None => Ok(if a.rows != 1 { 1 } else { 2 }),
    }
}

/// Reduce along a dimension with `f(slice) -> value`.
pub(crate) fn reduce(a: &Array<f64>, dim: usize, f: impl Fn(&[f64]) -> f64) -> Array<f64> {
    match dim {
        1 => {
            if a.rows == 0 && a.cols == 0 {
                return Array::scalar(f(&[]));
            }
            let out = (0..a.cols)
                .map(|c| f(&a.data[c * a.rows..(c + 1) * a.rows]))
                .collect();
            Array::new(1, a.cols, out)
        }
        2 => {
            let mut out = Vec::with_capacity(a.rows);
            let mut buf = Vec::with_capacity(a.cols);
            for r in 0..a.rows {
                buf.clear();
                for c in 0..a.cols {
                    buf.push(*a.get(r, c));
                }
                out.push(f(&buf));
            }
            Array::new(a.rows, 1, out)
        }
        _ => a.map(|x| f(&[*x])),
    }
}

/// Cumulative operation along a dimension.
pub(crate) fn cumulate(a: &Array<f64>, dim: usize, f: impl Fn(&[f64]) -> Vec<f64>) -> Array<f64> {
    let mut out = a.clone();
    match dim {
        1 => {
            for c in 0..a.cols {
                let r = f(&a.data[c * a.rows..(c + 1) * a.rows]);
                out.data[c * a.rows..(c + 1) * a.rows].copy_from_slice(&r);
            }
        }
        2 => {
            for r in 0..a.rows {
                let row: Vec<f64> = (0..a.cols).map(|c| *a.get(r, c)).collect();
                for (c, v) in f(&row).into_iter().enumerate() {
                    out.set(r, c, v);
                }
            }
        }
        _ => {}
    }
    out
}

pub(crate) fn cellstr_list(v: &Value) -> MResult<Vec<String>> {
    match v {
        Value::Cell(c) => c.data.iter().map(|x| x.text()).collect(),
        Value::Str(s) => Ok(s.data.clone()),
        Value::Char(a) if a.rows > 1 => Ok((0..a.rows)
            .map(|r| {
                (0..a.cols)
                    .map(|c| *a.get(r, c))
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()),
        other => Ok(vec![other.text()?]),
    }
}
