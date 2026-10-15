//! Operators, implicit expansion, linear algebra entry points and concatenation.

use nalgebra::DMatrix;

use crate::ast::{BinOp, UnOp};
use crate::error::MResult;
use crate::merr;
use crate::value::*;

pub fn to_dmatrix(a: &Array<f64>) -> DMatrix<f64> {
    DMatrix::from_column_slice(a.rows, a.cols, &a.data)
}

pub fn from_dmatrix(m: &DMatrix<f64>) -> Array<f64> {
    Array::new(m.nrows(), m.ncols(), m.as_slice().to_vec())
}

/// Elementwise combine with implicit expansion.
pub fn broadcast(
    a: &Array<f64>,
    b: &Array<f64>,
    f: impl Fn(f64, f64) -> f64,
    opname: &str,
) -> MResult<Array<f64>> {
    if a.rows == b.rows && a.cols == b.cols {
        return Ok(Array::new(
            a.rows,
            a.cols,
            a.data.iter().zip(&b.data).map(|(x, y)| f(*x, *y)).collect(),
        ));
    }
    if a.numel() == 1 {
        let x = a.data[0];
        return Ok(b.map(|y| f(x, *y)));
    }
    if b.numel() == 1 {
        let y = b.data[0];
        return Ok(a.map(|x| f(*x, y)));
    }
    let r = dim_join(a.rows, b.rows);
    let c = dim_join(a.cols, b.cols);
    match (r, c) {
        (Some(r), Some(c)) => {
            let mut out = Vec::with_capacity(r * c);
            for j in 0..c {
                for i in 0..r {
                    let x = *a.get(
                        if a.rows == 1 { 0 } else { i },
                        if a.cols == 1 { 0 } else { j },
                    );
                    let y = *b.get(
                        if b.rows == 1 { 0 } else { i },
                        if b.cols == 1 { 0 } else { j },
                    );
                    out.push(f(x, y));
                }
            }
            Ok(Array::new(r, c, out))
        }
        _ => Err(merr!(
            "Arrays have incompatible sizes for this operation ({}). Sizes are {}x{} and {}x{}.",
            opname,
            a.rows,
            a.cols,
            b.rows,
            b.cols
        )),
    }
}

fn dim_join(x: usize, y: usize) -> Option<usize> {
    if x == y {
        Some(x)
    } else if x == 1 {
        Some(y)
    } else if y == 1 {
        Some(x)
    } else {
        None
    }
}

fn result_class(a: NumClass, b: NumClass) -> MResult<NumClass> {
    use NumClass::*;
    Ok(match (a, b) {
        (Int(x), Int(y)) if x != y => {
            return Err(merr!(
                "Integers can only be combined with integers of the same class, or scalar doubles."
            ))
        }
        (Int(x), _) | (_, Int(x)) => Int(x),
        (Single, _) | (_, Single) => Single,
        _ => Double,
    })
}

fn finish(a: Array<f64>, class: NumClass) -> Value {
    match class {
        NumClass::Int(k) => Value::Num(a.map(|x| k.cast(*x)), class),
        NumClass::Single => Value::Num(a.map(|x| (*x as f32) as f64), class),
        _ => Value::Num(a, NumClass::Double),
    }
}

pub fn unary(op: UnOp, v: &Value) -> MResult<Value> {
    if let Value::Str(_) = v {
        return Err(merr!("Unary operators are not supported for string"));
    }
    let a = v.to_num()?;
    Ok(match op {
        UnOp::Neg => finish(a.map(|x| -x), keep_class(v)),
        UnOp::Plus => finish(a, keep_class(v)),
        UnOp::Not => {
            if a.data.iter().any(|x| x.is_nan()) {
                return Err(merr!("NaN's cannot be converted to logicals."));
            }
            Value::logical(a.map(|x| if *x == 0.0 { 1.0 } else { 0.0 }))
        }
    })
}

fn keep_class(v: &Value) -> NumClass {
    match v.num_class() {
        NumClass::Logical => NumClass::Double,
        c => c,
    }
}

pub fn binary(op: BinOp, l: &Value, r: &Value) -> MResult<Value> {
    // String arrays: + concatenates, comparisons compare text.
    if matches!(l, Value::Str(_)) || matches!(r, Value::Str(_)) {
        return string_binary(op, l, r);
    }
    if let (Value::Cell(_), _) | (_, Value::Cell(_)) = (l, r) {
        return Err(merr!(
            "Operator '{}' is not supported for operands of type 'cell'.",
            op_symbol(op)
        ));
    }
    if let (Value::Struct(_), _) | (_, Value::Struct(_)) = (l, r) {
        return Err(merr!(
            "Operator '{}' is not supported for operands of type 'struct'.",
            op_symbol(op)
        ));
    }
    let a = l.to_num()?;
    let b = r.to_num()?;
    let class = result_class(keep_class(l), keep_class(r))?;
    let cmp = |f: fn(f64, f64) -> bool| -> MResult<Value> {
        Ok(Value::logical(broadcast(
            &a,
            &b,
            |x, y| if f(x, y) { 1.0 } else { 0.0 },
            op_symbol(op),
        )?))
    };
    let logic = |f: fn(bool, bool) -> bool| -> MResult<Value> {
        if a.data.iter().chain(b.data.iter()).any(|x| x.is_nan()) {
            return Err(merr!("NaN's cannot be converted to logicals."));
        }
        Ok(Value::logical(broadcast(
            &a,
            &b,
            |x, y| if f(x != 0.0, y != 0.0) { 1.0 } else { 0.0 },
            op_symbol(op),
        )?))
    };
    match op {
        BinOp::Add => Ok(finish(broadcast(&a, &b, |x, y| x + y, "+")?, class)),
        BinOp::Sub => Ok(finish(broadcast(&a, &b, |x, y| x - y, "-")?, class)),
        BinOp::ElMul => Ok(finish(broadcast(&a, &b, |x, y| x * y, ".*")?, class)),
        BinOp::ElDiv => Ok(finish(broadcast(&a, &b, |x, y| x / y, "./")?, class)),
        BinOp::ElLDiv => Ok(finish(broadcast(&a, &b, |x, y| y / x, ".\\")?, class)),
        BinOp::ElPow => Ok(finish(broadcast(&a, &b, pow, ".^")?, class)),
        BinOp::MatMul => {
            if a.is_scalar() || b.is_scalar() {
                return Ok(finish(broadcast(&a, &b, |x, y| x * y, "*")?, class));
            }
            if a.cols != b.rows {
                return Err(merr!("Incorrect dimensions for matrix multiplication. Check that the number of columns in the first matrix matches the number of rows in the second matrix. To operate on each element of the matrix individually, use TIMES (.*) for elementwise multiplication."));
            }
            Ok(finish(matmul(&a, &b), class))
        }
        BinOp::MatDiv => {
            if b.is_scalar() {
                return Ok(finish(broadcast(&a, &b, |x, y| x / y, "/")?, class));
            }
            // a / b = (b' \ a')'
            let x = mldivide(&b.transpose(), &a.transpose())?;
            Ok(finish(x.transpose(), class))
        }
        BinOp::MatLDiv => {
            if a.is_scalar() {
                return Ok(finish(broadcast(&a, &b, |x, y| y / x, "\\")?, class));
            }
            Ok(finish(mldivide(&a, &b)?, class))
        }
        BinOp::MatPow => {
            if a.is_scalar() && b.is_scalar() {
                return Ok(finish(Array::scalar(pow(a.data[0], b.data[0])), class));
            }
            if b.is_scalar() && a.rows == a.cols {
                let p = b.data[0];
                if p.fract() != 0.0 {
                    return Err(merr!("Non-integer matrix powers are not supported yet"));
                }
                return Ok(finish(matpow(&a, p as i64)?, class));
            }
            Err(merr!("Incorrect dimensions for raising a matrix to a power. Check that the matrix is square and the power is a scalar. To operate on each element of the matrix individually, use POWER (.^) for elementwise power."))
        }
        BinOp::Eq => cmp(|x, y| x == y),
        BinOp::Ne => cmp(|x, y| x != y),
        BinOp::Lt => cmp(|x, y| x < y),
        BinOp::Le => cmp(|x, y| x <= y),
        BinOp::Gt => cmp(|x, y| x > y),
        BinOp::Ge => cmp(|x, y| x >= y),
        BinOp::And => logic(|x, y| x && y),
        BinOp::Or => logic(|x, y| x || y),
        BinOp::AndAnd | BinOp::OrOr => unreachable!("short-circuit handled by interpreter"),
    }
}

fn pow(x: f64, y: f64) -> f64 {
    if y.fract() == 0.0 && y.abs() < 1e9 {
        x.powi(y as i32)
    } else {
        x.powf(y)
    }
}

fn string_binary(op: BinOp, l: &Value, r: &Value) -> MResult<Value> {
    let to_strs = |v: &Value| -> MResult<Array<String>> {
        Ok(match v {
            Value::Str(a) => a.clone(),
            Value::Char(_) => Array::scalar(v.text()?),
            Value::Num(a, _) => a.map(|x| crate::display::num_to_str(*x)),
            Value::Cell(c) => {
                let mut out = Vec::new();
                for x in &c.data {
                    out.push(x.text()?);
                }
                Array::new(c.rows, c.cols, out)
            }
            other => return Err(merr!("Operator not supported for {}", other.class_name())),
        })
    };
    let a = to_strs(l)?;
    let b = to_strs(r)?;
    let (n, pick): (usize, Box<dyn Fn(usize) -> (String, String)>) = if a.numel() == b.numel() {
        (
            a.numel(),
            Box::new(|i| (a.data[i].clone(), b.data[i].clone())),
        )
    } else if a.numel() == 1 {
        (
            b.numel(),
            Box::new(|i| (a.data[0].clone(), b.data[i].clone())),
        )
    } else if b.numel() == 1 {
        (
            a.numel(),
            Box::new(|i| (a.data[i].clone(), b.data[0].clone())),
        )
    } else {
        return Err(merr!("Arrays have incompatible sizes for this operation."));
    };
    let (rows, cols) = if a.numel() >= b.numel() {
        (a.rows, a.cols)
    } else {
        (b.rows, b.cols)
    };
    match op {
        BinOp::Add => Ok(Value::Str(Array::new(
            rows,
            cols,
            (0..n)
                .map(|i| {
                    let (x, y) = pick(i);
                    x + &y
                })
                .collect(),
        ))),
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let data = (0..n)
                .map(|i| {
                    let (x, y) = pick(i);
                    let t = match op {
                        BinOp::Eq => x == y,
                        BinOp::Ne => x != y,
                        BinOp::Lt => x < y,
                        BinOp::Le => x <= y,
                        BinOp::Gt => x > y,
                        _ => x >= y,
                    };
                    if t {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect();
            Ok(Value::logical(Array::new(rows, cols, data)))
        }
        _ => Err(merr!(
            "Operator '{}' is not supported for operands of type 'string'.",
            op_symbol(op)
        )),
    }
}

pub fn op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::MatMul => "*",
        BinOp::ElMul => ".*",
        BinOp::MatDiv => "/",
        BinOp::ElDiv => "./",
        BinOp::MatLDiv => "\\",
        BinOp::ElLDiv => ".\\",
        BinOp::MatPow => "^",
        BinOp::ElPow => ".^",
        BinOp::Eq => "==",
        BinOp::Ne => "~=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&",
        BinOp::Or => "|",
        BinOp::AndAnd => "&&",
        BinOp::OrOr => "||",
    }
}

pub fn matmul(a: &Array<f64>, b: &Array<f64>) -> Array<f64> {
    let mut out = vec![0.0; a.rows * b.cols];
    for j in 0..b.cols {
        for k in 0..a.cols {
            let bkj = b.data[j * b.rows + k];
            if bkj == 0.0 {
                continue;
            }
            let col = &a.data[k * a.rows..(k + 1) * a.rows];
            let o = &mut out[j * a.rows..(j + 1) * a.rows];
            for i in 0..a.rows {
                o[i] += col[i] * bkj;
            }
        }
    }
    Array::new(a.rows, b.cols, out)
}

pub fn mldivide(a: &Array<f64>, b: &Array<f64>) -> MResult<Array<f64>> {
    if a.rows != b.rows {
        return Err(merr!("Matrix dimensions must agree."));
    }
    let am = to_dmatrix(a);
    let bm = to_dmatrix(b);
    if a.rows == a.cols {
        let lu = am.clone().lu();
        if let Some(x) = lu.solve(&bm) {
            if x.iter().all(|v| v.is_finite()) {
                return Ok(from_dmatrix(&x));
            }
        }
        // Singular: fall back to least squares and let the caller see Inf/NaN like a warning case.
    }
    let svd = am.svd(true, true);
    let x = svd.solve(&bm, 1e-12).map_err(|e| merr!("{}", e))?;
    Ok(from_dmatrix(&x))
}

fn matpow(a: &Array<f64>, p: i64) -> MResult<Array<f64>> {
    let mut base = if p < 0 { inv(a)? } else { a.clone() };
    let mut e = p.unsigned_abs();
    let mut result = eye(a.rows);
    while e > 0 {
        if e & 1 == 1 {
            result = matmul(&result, &base);
        }
        base = matmul(&base, &base);
        e >>= 1;
    }
    Ok(result)
}

pub fn eye(n: usize) -> Array<f64> {
    let mut a = Array::filled(n, n, 0.0);
    for i in 0..n {
        a.set(i, i, 1.0);
    }
    a
}

pub fn inv(a: &Array<f64>) -> MResult<Array<f64>> {
    if a.rows != a.cols {
        return Err(merr!("Matrix must be square."));
    }
    match to_dmatrix(a).try_inverse() {
        Some(m) => Ok(from_dmatrix(&m)),
        None => Ok(Array::filled(a.rows, a.cols, f64::INFINITY)),
    }
}

pub fn range(start: f64, step: f64, stop: f64) -> MResult<Array<f64>> {
    if step == 0.0
        || (step > 0.0 && start > stop)
        || (step < 0.0 && start < stop)
        || start.is_nan()
        || stop.is_nan()
        || step.is_nan()
    {
        return Ok(Array::new(1, 0, vec![]));
    }
    let n = ((stop - start) / step + 1e-10).floor() as usize + 1;
    if n > 2_000_000_000 {
        return Err(merr!(
            "Maximum variable size allowed by the program is exceeded."
        ));
    }
    Ok(Array::row(
        (0..n).map(|i| start + step * i as f64).collect(),
    ))
}

// ---------- concatenation ----------

pub fn horzcat(vals: Vec<Value>) -> MResult<Value> {
    concat(vals, true)
}

pub fn vertcat(vals: Vec<Value>) -> MResult<Value> {
    concat(vals, false)
}

fn concat(vals: Vec<Value>, horiz: bool) -> MResult<Value> {
    let any_cell = vals.iter().any(|v| matches!(v, Value::Cell(_)));
    let any_str = vals.iter().any(|v| matches!(v, Value::Str(_)));
    let any_struct = vals.iter().any(|v| matches!(v, Value::Struct(_)));
    let any_char = vals.iter().any(|v| matches!(v, Value::Char(_)));
    if vals.iter().any(|v| matches!(v, Value::Func(_))) {
        if vals.len() == 1 {
            return Ok(vals.into_iter().next().unwrap());
        }
        return Err(merr!(
            "Nonscalar arrays of function handles are not allowed; use cell arrays instead."
        ));
    }
    if vals.len() == 1 {
        return Ok(vals.into_iter().next().unwrap());
    }
    // Drop [] (0x0) operands; they only influence the result class.
    let vals: Vec<Value> = {
        let kept: Vec<Value> = vals
            .iter()
            .filter(|v| v.dims() != (0, 0))
            .cloned()
            .collect();
        if kept.is_empty() {
            vec![vals[0].clone()]
        } else {
            kept
        }
    };
    if any_cell {
        let parts: Vec<Array<Value>> = vals
            .into_iter()
            .map(|v| match v {
                Value::Cell(a) => a,
                other => Array::scalar(other),
            })
            .collect();
        return Ok(Value::Cell(join(parts, horiz)?));
    }
    if any_struct {
        let mut keys: Option<Vec<String>> = None;
        let mut parts = Vec::new();
        for v in vals {
            match v {
                Value::Struct(s) => {
                    match &keys {
                        None => keys = Some(s.keys.clone()),
                        Some(k) => {
                            let mut a = k.clone();
                            let mut b = s.keys.clone();
                            a.sort();
                            b.sort();
                            if a != b {
                                return Err(merr!(
                                    "Concatenation of structures requires same field names."
                                ));
                            }
                        }
                    }
                    let k = keys.as_ref().unwrap();
                    let arr = s.arr.map(|e| {
                        k.iter()
                            .map(|key| e[s.key_index(key).unwrap()].clone())
                            .collect::<Vec<Value>>()
                    });
                    parts.push(arr);
                }
                _ => {
                    return Err(merr!(
                        "Cannot concatenate a struct with a non-struct value."
                    ))
                }
            }
        }
        return Ok(Value::Struct(StructArr {
            keys: keys.unwrap(),
            arr: join(parts, horiz)?,
        }));
    }
    if any_str {
        let mut parts = Vec::new();
        for v in vals {
            parts.push(match v {
                Value::Str(a) => a,
                Value::Char(_) => Array::scalar(v.text()?),
                Value::Num(a, _) => a.map(|x| crate::display::num_to_str(*x)),
                other => return Err(merr!("Cannot convert {} to string", other.class_name())),
            });
        }
        return Ok(Value::Str(join(parts, horiz)?));
    }
    if any_char {
        let mut parts = Vec::new();
        for v in vals {
            parts.push(match v {
                Value::Char(a) => a,
                Value::Num(a, _) => a.map(|x| char::from_u32(*x as u32).unwrap_or('\0')),
                other => return Err(merr!("Cannot convert {} to char", other.class_name())),
            });
        }
        return Ok(Value::Char(join(parts, horiz)?));
    }
    let mut class: Option<NumClass> = None;
    let mut parts = Vec::new();
    for v in vals {
        if let Value::Num(a, c) = v {
            class = Some(match (class, c) {
                (None, c) => c,
                (Some(NumClass::Int(k)), _) | (_, NumClass::Int(k)) => NumClass::Int(k),
                (Some(NumClass::Single), _) | (_, NumClass::Single) => NumClass::Single,
                (Some(NumClass::Logical), NumClass::Logical) => NumClass::Logical,
                _ => NumClass::Double,
            });
            parts.push(a);
        }
    }
    let class = class.unwrap_or(NumClass::Double);
    let a = join(parts, horiz)?;
    Ok(match class {
        NumClass::Int(k) => Value::Num(a.map(|x| k.cast(*x)), class),
        c => Value::Num(a, c),
    })
}

fn join<T: Clone>(parts: Vec<Array<T>>, horiz: bool) -> MResult<Array<T>> {
    let parts: Vec<Array<T>> = {
        let non_empty: Vec<Array<T>> = parts.iter().filter(|p| p.numel() > 0).cloned().collect();
        if non_empty.is_empty() {
            return Ok(parts.into_iter().next().unwrap_or_else(Array::empty));
        }
        non_empty
    };
    if horiz {
        let rows = parts[0].rows;
        if parts.iter().any(|p| p.rows != rows) {
            return Err(merr!(
                "Dimensions of arrays being concatenated are not consistent."
            ));
        }
        let cols = parts.iter().map(|p| p.cols).sum();
        let mut data = Vec::with_capacity(rows * cols);
        for p in parts {
            data.extend(p.data);
        }
        Ok(Array::new(rows, cols, data))
    } else {
        let cols = parts[0].cols;
        if parts.iter().any(|p| p.cols != cols) {
            return Err(merr!(
                "Dimensions of arrays being concatenated are not consistent."
            ));
        }
        let rows: usize = parts.iter().map(|p| p.rows).sum();
        let mut data = Vec::with_capacity(rows * cols);
        for c in 0..cols {
            for p in &parts {
                for r in 0..p.rows {
                    data.push(p.get(r, c).clone());
                }
            }
        }
        Ok(Array::new(rows, cols, data))
    }
}
