//! Built-in object types: transfer functions (`tf`), tables, and tiled chart layouts.

use nalgebra::DMatrix;

use super::*;
use crate::display::{self, Format};
use crate::graphics::UiTable;
use crate::ops;

pub const TF: &str = "tf";
pub const TABLE: &str = "table";
pub const TILED: &str = "TiledChartLayout";

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("tf", tf);
    b.insert("step", step);
    b.insert("dcgain", |_, a, _| {
        check_args(&a, 1, 1)?;
        let (num, den, ts) = tf_parts(&a[0])?;
        let g = if ts > 0.0 {
            num.iter().sum::<f64>() / den.iter().sum::<f64>()
        } else {
            num.last().copied().unwrap_or(0.0) / den.last().copied().unwrap_or(1.0)
        };
        one(Value::num(g))
    });
    b.insert("pole", |it, a, _| {
        check_args(&a, 1, 1)?;
        let (_, den, _) = tf_parts(&a[0])?;
        one(Value::col(poly_roots(it, &den)))
    });
    b.insert("zero", |it, a, _| {
        check_args(&a, 1, 1)?;
        let (num, _, _) = tf_parts(&a[0])?;
        one(Value::col(poly_roots(it, &num)))
    });
    b.insert("tfdata", |_, a, _| {
        check_args(&a, 1, 2)?;
        let (num, den, ts) = tf_parts(&a[0])?;
        Ok(vec![
            Value::cell_row(vec![Value::row(num)]),
            Value::cell_row(vec![Value::row(den)]),
            Value::num(ts),
        ])
    });
    b.insert("table", table);
    b.insert("table2cell", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::Cell(table_cells(table_obj(&a[0])?)?))
    });
    b.insert("table2array", |_, a, _| {
        check_args(&a, 1, 1)?;
        let mut v = brace_get(table_obj(&a[0])?, &[Idx::All, Idx::All])?;
        one(v.swap_remove(0))
    });
    b.insert("height", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::num(a[0].dims().0 as f64))
    });
    b.insert("width", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::num(a[0].dims().1 as f64))
    });
    b.insert("writetable", writetable);
    b.insert("tiledlayout", tiledlayout);
    b.insert("nexttile", nexttile);
    b.insert("uitable", uitable);
}

/// Text shown when an object is displayed.
pub fn object_text(o: &Object, fmt: Format) -> String {
    match o.class {
        TF => tf_text(o),
        TABLE => table_text(o, fmt),
        _ => {
            let mut s = format!("  {} with properties:\n\n", o.class);
            for (i, k) in o.props.keys.iter().enumerate() {
                s.push_str(&format!(
                    "    {}: {}\n",
                    k,
                    display::summary(&o.props.arr.data[0][i], fmt)
                ));
            }
            s
        }
    }
}

// ---------------- generic field / brace access used by the interpreter ----------------

pub fn get_field(o: &Object, name: &str) -> MResult<Value> {
    if o.class == TABLE {
        if name == "Properties" {
            return Ok(Value::Struct(StructArr::scalar(vec![
                ("Description".into(), Value::chars("")),
                (
                    "VariableNames".into(),
                    o.get("VariableNames").cloned().unwrap_or_default(),
                ),
                (
                    "RowNames".into(),
                    o.get("RowNames").cloned().unwrap_or_default(),
                ),
            ])));
        }
        if name == "Row" {
            return Ok(o.get("RowNames").cloned().unwrap_or_default());
        }
        let names = cellstr_list(o.get("VariableNames").unwrap())?;
        if let Some(j) = names.iter().position(|n| n == name) {
            let Some(Value::Cell(d)) = o.get("Data") else {
                unreachable!()
            };
            return Ok(d.data[j].clone());
        }
        return Err(merr!("Unrecognized table variable name '{}'.", name));
    }
    o.get(name)
        .cloned()
        .ok_or_else(|| merr!("Unrecognized property '{}' for class '{}'.", name, o.class))
}

pub fn set_field(o: &mut Object, name: &str, v: Value) -> MResult<()> {
    if o.class == TABLE {
        if name == "Properties" {
            let Value::Struct(s) = &v else {
                return Err(merr!("Properties must be a structure."));
            };
            if let Some(vn) = s.get(0, "VariableNames") {
                let list = cellstr_list(vn)?;
                if list.len() != o.dims().1 {
                    return Err(merr!("The VariableNames property must contain one name for each variable in the table."));
                }
                o.set("VariableNames", names_cell(&list));
            }
            if let Some(rn) = s.get(0, "RowNames") {
                o.set("RowNames", row_names(rn, o.dims().0)?);
            }
            return Ok(());
        }
        let height = o.dims().0;
        if v.dims().0 != height {
            return Err(merr!("To assign to or create a variable in a table, the number of rows must match the height of the table."));
        }
        let mut names = cellstr_list(o.get("VariableNames").unwrap())?;
        let Some(Value::Cell(mut d)) = o.get("Data").cloned() else {
            unreachable!()
        };
        match names.iter().position(|n| n == name) {
            Some(j) => d.data[j] = v,
            None => {
                names.push(name.to_string());
                d.data.push(v);
                d.cols += 1;
            }
        }
        o.set("Data", Value::Cell(d));
        o.set("VariableNames", names_cell(&names));
        return Ok(());
    }
    if o.class == TF {
        return Err(merr!(
            "Transfer function properties are read-only; create a new tf instead."
        ));
    }
    o.set(name, v);
    Ok(())
}

/// `T{rows, vars}`: the selected rows of the selected variables, concatenated.
pub fn brace_get(o: &Object, idx: &[Idx]) -> MResult<Vec<Value>> {
    if o.class != TABLE || idx.len() != 2 {
        return Err(merr!(
            "Brace indexing on a {} needs two subscripts (rows, variables).",
            o.class
        ));
    }
    let cols = o.dims().1;
    let Some(Value::Cell(d)) = o.get("Data") else {
        unreachable!()
    };
    let (cl, _) = idx[1].resolve(cols);
    let mut parts = Vec::new();
    for j in cl {
        let col = d
            .data
            .get(j)
            .ok_or_else(|| merr!("Variable index exceeds table width."))?;
        parts.push(col.index(&[idx[0].clone(), Idx::All])?);
    }
    Ok(vec![ops::horzcat(parts)?])
}

pub fn brace_set(o: &mut Object, idx: &[Idx], rhs: Value) -> MResult<()> {
    if o.class != TABLE || idx.len() != 2 {
        return Err(merr!(
            "Brace assignment on a {} needs two subscripts (rows, variables).",
            o.class
        ));
    }
    let (rows, cols) = o.dims();
    let (rl, _) = idx[0].resolve(rows);
    let (cl, _) = idx[1].resolve(cols);
    let src = rhs.to_num()?;
    let Some(Value::Cell(mut d)) = o.get("Data").cloned() else {
        unreachable!()
    };
    let mut src_col = 0;
    for j in cl {
        let Value::Num(col, class) = &d.data[j] else {
            return Err(merr!(
                "Brace assignment is supported for numeric table variables only."
            ));
        };
        let mut col = col.clone();
        for c in 0..col.cols {
            for (k, &r) in rl.iter().enumerate() {
                let v = if src.numel() == 1 {
                    src.data[0]
                } else {
                    *src.data
                        .get((src_col) * rl.len() + k)
                        .ok_or_else(|| merr!("Right-hand side has too few elements."))?
                };
                col.set(r, c, v);
            }
            src_col += 1;
        }
        d.data[j] = Value::Num(col, *class);
    }
    o.set("Data", Value::Cell(d));
    Ok(())
}

// ---------------- transfer functions ----------------

fn tf_parts(v: &Value) -> MResult<(Vec<f64>, Vec<f64>, f64)> {
    let Value::Obj(o) = v else {
        return Err(merr!("Expected a transfer function (tf) model."));
    };
    if o.class != TF {
        return Err(merr!(
            "Expected a transfer function (tf) model, got {}.",
            o.class
        ));
    }
    let num = o.get("Numerator").unwrap().to_num()?.data;
    let den = o.get("Denominator").unwrap().to_num()?.data;
    let ts = o.get("Ts").unwrap().scalar()?;
    Ok((num, den, ts))
}

fn trim_leading(v: Vec<f64>) -> Vec<f64> {
    let k = v
        .iter()
        .position(|x| *x != 0.0)
        .unwrap_or(v.len().saturating_sub(1));
    v[k..].to_vec()
}

fn tf(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() == 1 && a[0].is_text() {
        return Err(merr!(
            "tf('s') / tf('z') model algebra is not supported yet; use tf(num, den[, Ts])."
        ));
    }
    check_args(&a, 2, 3)?;
    let num = trim_leading(a[0].to_num()?.data);
    let den = trim_leading(a[1].to_num()?.data);
    if den.iter().all(|x| *x == 0.0) {
        return Err(merr!("The denominator must not be zero."));
    }
    let ts = if a.len() == 3 { a[2].scalar()? } else { 0.0 };
    if ts < 0.0 && ts != -1.0 {
        return Err(merr!(
            "The sample time must be a positive scalar, -1, or 0."
        ));
    }
    one(Value::Obj(Box::new(Object::new(
        TF,
        vec![
            ("Numerator".into(), Value::row(num)),
            ("Denominator".into(), Value::row(den)),
            ("Ts".into(), Value::num(ts)),
            (
                "Variable".into(),
                Value::chars(if ts != 0.0 { "z" } else { "s" }),
            ),
        ],
    ))))
}

fn poly_text(p: &[f64], var: &str) -> String {
    let n = p.len();
    let mut s = String::new();
    for (i, &c) in p.iter().enumerate() {
        if c == 0.0 {
            continue;
        }
        let pow = n - 1 - i;
        let mag = c.abs();
        let coef = if (mag - 1.0).abs() < 1e-12 && pow > 0 {
            String::new()
        } else {
            display::trim_g(mag, 4)
        };
        let term = match pow {
            0 => coef,
            1 => format!("{}{}{}", coef, if coef.is_empty() { "" } else { " " }, var),
            _ => format!(
                "{}{}{}^{}",
                coef,
                if coef.is_empty() { "" } else { " " },
                var,
                pow
            ),
        };
        if s.is_empty() {
            s = if c < 0.0 { format!("-{}", term) } else { term };
        } else {
            s.push_str(if c < 0.0 { " - " } else { " + " });
            s.push_str(&term);
        }
    }
    if s.is_empty() {
        "0".into()
    } else {
        s
    }
}

fn tf_text(o: &Object) -> String {
    let num = o
        .get("Numerator")
        .and_then(|v| v.to_num().ok())
        .map(|a| a.data)
        .unwrap_or_default();
    let den = o
        .get("Denominator")
        .and_then(|v| v.to_num().ok())
        .map(|a| a.data)
        .unwrap_or_default();
    let ts = o.get("Ts").and_then(|v| v.scalar().ok()).unwrap_or(0.0);
    let var = if ts != 0.0 { "z" } else { "s" };
    let n = poly_text(&num, var);
    let d = poly_text(&den, var);
    let w = n.chars().count().max(d.chars().count()) + 2;
    let center = |t: &str| format!("{:^w$}", t, w = w);
    let mut s = format!(
        "  {}\n  {}\n  {}\n\n",
        center(&n),
        "-".repeat(w),
        center(&d)
    );
    if ts > 0.0 {
        s.push_str(&format!(
            "Sample time: {} seconds\nDiscrete-time transfer function.\n",
            display::trim_g(ts, 6)
        ));
    } else if ts == -1.0 {
        s.push_str("Sample time: unspecified\nDiscrete-time transfer function.\n");
    } else {
        s.push_str("Continuous-time transfer function.\n");
    }
    s
}

fn poly_roots(it: &mut Interp, p: &[f64]) -> Vec<f64> {
    let p = trim_leading(p.to_vec());
    let n = p.len().saturating_sub(1);
    if n == 0 {
        return vec![];
    }
    let mut c = DMatrix::<f64>::zeros(n, n);
    for j in 0..n {
        c[(0, j)] = -p[j + 1] / p[0];
    }
    for i in 1..n {
        c[(i, i - 1)] = 1.0;
    }
    let ev = c.complex_eigenvalues();
    if ev.iter().any(|z| z.im.abs() > 1e-9) {
        it.warn("The model has complex poles or zeros; complex numbers are not supported yet, returning real parts.");
    }
    let mut r: Vec<f64> = ev.iter().map(|z| z.re).collect();
    r.sort_by(|x, y| y.partial_cmp(x).unwrap_or(std::cmp::Ordering::Equal));
    r
}

/// Discrete step response: den(z) y = num(z) u with u[k] = 1 for k >= 0.
fn discrete_step(num: &[f64], den: &[f64], samples: usize) -> Vec<f64> {
    let n = den.len().max(num.len());
    let mut b = vec![0.0; n - num.len()];
    b.extend_from_slice(num);
    let mut a = vec![0.0; n - den.len()];
    a.extend_from_slice(den);
    let a0 = a[0];
    let mut y = vec![0.0; samples];
    for k in 0..samples {
        let mut acc = 0.0;
        for (i, bi) in b.iter().enumerate() {
            if k >= i {
                acc += bi; // u[k-i] = 1
            }
        }
        for i in 1..n {
            if k >= i {
                acc -= a[i] * y[k - i];
            }
        }
        y[k] = acc / a0;
    }
    y
}

/// Continuous step response sampled every `dt` using an exact zero-order-hold discretization.
fn continuous_step(num: &[f64], den: &[f64], dt: f64, samples: usize) -> Vec<f64> {
    let den = trim_leading(den.to_vec());
    let n = den.len() - 1;
    let a0 = den[0];
    let a: Vec<f64> = den.iter().map(|x| x / a0).collect();
    let mut b = vec![0.0; (n + 1).saturating_sub(num.len())];
    b.extend(num.iter().map(|x| x / a0));
    let b = b[b.len() - (n + 1)..].to_vec();
    if n == 0 {
        return vec![b[0]; samples];
    }
    // Controllable canonical form.
    let mut am = DMatrix::<f64>::zeros(n, n);
    for j in 0..n {
        am[(0, j)] = -a[j + 1];
    }
    for i in 1..n {
        am[(i, i - 1)] = 1.0;
    }
    let c: Vec<f64> = (0..n).map(|j| b[j + 1] - a[j + 1] * b[0]).collect();
    let d = b[0];
    let mut aug = DMatrix::<f64>::zeros(n + 1, n + 1);
    for i in 0..n {
        for j in 0..n {
            aug[(i, j)] = am[(i, j)] * dt;
        }
    }
    aug[(0, n)] = dt;
    let e = aug.exp();
    let mut x = vec![0.0; n];
    let mut y = Vec::with_capacity(samples);
    for _ in 0..samples {
        y.push(c.iter().zip(&x).map(|(p, q)| p * q).sum::<f64>() + d);
        let mut nx = vec![0.0; n];
        for i in 0..n {
            nx[i] = (0..n).map(|j| e[(i, j)] * x[j]).sum::<f64>() + e[(i, n)];
        }
        x = nx;
    }
    y
}

fn step(it: &mut Interp, a: Vec<Value>, nargout: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let (num, den, ts) = tf_parts(&a[0])?;
    let tspec = a.get(1).map(|v| v.to_num()).transpose()?;
    let (t, y): (Vec<f64>, Vec<f64>) = if ts != 0.0 {
        let ts = if ts < 0.0 { 1.0 } else { ts };
        let ks: Vec<usize> = match &tspec {
            Some(t) if t.numel() > 1 => t
                .data
                .iter()
                .map(|x| (x / ts).round().max(0.0) as usize)
                .collect(),
            Some(t) => (0..=((t.data[0] / ts).round() as usize)).collect(),
            None => {
                let probe = discrete_step(&num, &den, 2000);
                let last = settle_index(&probe);
                (0..=last).collect()
            }
        };
        let kmax = ks.iter().copied().max().unwrap_or(0);
        let resp = discrete_step(&num, &den, kmax + 1);
        let t = match &tspec {
            Some(tv) if tv.numel() > 1 => tv.data.clone(),
            _ => ks.iter().map(|k| *k as f64 * ts).collect(),
        };
        (t, ks.iter().map(|k| resp[*k]).collect())
    } else {
        let t: Vec<f64> = match &tspec {
            Some(tv) if tv.numel() > 1 => tv.data.clone(),
            Some(tv) => (0..=200).map(|i| tv.data[0] * i as f64 / 200.0).collect(),
            None => {
                let poles = poly_roots(it, &den);
                let slow = poles
                    .iter()
                    .filter(|p| **p < 0.0)
                    .map(|p| p.abs())
                    .fold(f64::INFINITY, f64::min);
                let tf = if slow.is_finite() && slow > 0.0 {
                    7.0 / slow
                } else {
                    10.0
                };
                (0..=300).map(|i| tf * i as f64 / 300.0).collect()
            }
        };
        // Simulate on a fine uniform grid from 0, then sample at the requested times.
        let tmax = t.iter().copied().fold(0.0f64, f64::max);
        let min_gap = t
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .filter(|d| *d > 0.0)
            .fold(f64::INFINITY, f64::min);
        let dt = min_gap.min(tmax / 2000.0).max(1e-9);
        let steps = (tmax / dt).ceil() as usize + 2;
        let grid = continuous_step(&num, &den, dt, steps);
        let y = t
            .iter()
            .map(|&ti| {
                let p = (ti.max(0.0) / dt).min((steps - 1) as f64);
                let k = (p.floor() as usize).min(steps - 2);
                let f = p - k as f64;
                grid[k] * (1.0 - f) + grid[k + 1] * f
            })
            .collect();
        (t, y)
    };
    if nargout == 0 {
        let ax = it.graphics.plot_axes();
        let color = crate::graphics::DEFAULT_COLORS[ax.color_index % 7].to_string();
        ax.color_index += 1;
        let style = crate::graphics::LineStyle {
            color: Some(color),
            ..Default::default()
        };
        if ts != 0.0 {
            ax.series.push(crate::graphics::Series::Stairs {
                x: t,
                y,
                style,
                name: None,
            });
        } else {
            ax.series.push(crate::graphics::Series::Line {
                x: t,
                y,
                z: None,
                style,
                name: None,
            });
        }
        ax.title.get_or_insert_with(|| "Step Response".into());
        ax.xlabel.get_or_insert_with(|| "Time (seconds)".into());
        ax.ylabel.get_or_insert_with(|| "Amplitude".into());
        ax.grid_on = true;
        return none();
    }
    let mut out = vec![Value::col(y)];
    if nargout > 1 {
        out.push(Value::col(t));
    }
    Ok(out)
}

fn settle_index(y: &[f64]) -> usize {
    let fin = *y.last().unwrap_or(&0.0);
    let tol = 0.002 * fin.abs().max(1e-9);
    let mut last = 0;
    for (k, v) in y.iter().enumerate() {
        if (v - fin).abs() > tol {
            last = k;
        }
    }
    ((last as f64 * 1.3) as usize + 5).clamp(10, y.len() - 1)
}

// ---------------- tables ----------------

fn names_cell(list: &[String]) -> Value {
    Value::Cell(Array::row(list.iter().map(|s| Value::chars(s)).collect()))
}

fn row_names(v: &Value, height: usize) -> MResult<Value> {
    if v.is_empty() {
        return Ok(Value::Cell(Array::new(0, 0, vec![])));
    }
    let list = cellstr_list(v)?;
    if list.len() != height {
        return Err(merr!(
            "The number of row names must equal the number of rows in the table."
        ));
    }
    Ok(Value::Cell(Array::col(
        list.iter().map(|s| Value::chars(s)).collect(),
    )))
}

pub fn table_obj(v: &Value) -> MResult<&Object> {
    match v {
        Value::Obj(o) if o.class == TABLE => Ok(o),
        other => Err(merr!("Expected a table, got {}.", other.class_name())),
    }
}

fn table(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let arg_names = std::mem::take(&mut it.arg_names);
    let mut cols = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut var_names: Option<Vec<String>> = None;
    let mut rows: Option<Value> = None;
    let mut i = 0;
    while i < a.len() {
        if a[i].is_text() && i + 1 < a.len() {
            let key = a[i].text()?;
            match key.to_lowercase().as_str() {
                "variablenames" => {
                    var_names = Some(cellstr_list(&a[i + 1])?);
                    i += 2;
                    continue;
                }
                "rownames" => {
                    rows = Some(a[i + 1].clone());
                    i += 2;
                    continue;
                }
                "size" | "variabletypes" | "dimensionnames" => {
                    return Err(merr!(
                        "table('Size', ...) preallocation is not supported yet."
                    ));
                }
                _ => {}
            }
        }
        names.push(
            arg_names
                .get(i)
                .cloned()
                .flatten()
                .unwrap_or_else(|| format!("Var{}", cols.len() + 1)),
        );
        cols.push(a[i].clone());
        i += 1;
    }
    let height = cols.first().map(|c| c.dims().0).unwrap_or(0);
    for c in &cols {
        if c.dims().0 != height {
            return Err(merr!(
                "All table variables must have the same number of rows."
            ));
        }
    }
    if let Some(v) = var_names {
        if v.len() != cols.len() {
            return Err(merr!(
                "The VariableNames property must contain one name for each variable in the table."
            ));
        }
        names = v;
    }
    let rn = match rows {
        Some(r) => row_names(&r, height)?,
        None => Value::Cell(Array::new(0, 0, vec![])),
    };
    let ncols = cols.len();
    one(Value::Obj(Box::new(Object::new(
        TABLE,
        vec![
            ("Data".into(), Value::Cell(Array::new(1, ncols, cols))),
            ("VariableNames".into(), names_cell(&names)),
            ("RowNames".into(), rn),
            ("Height".into(), Value::num(height as f64)),
        ],
    ))))
}

/// N×M cell with one entry per (row, variable).
fn table_cells(o: &Object) -> MResult<Array<Value>> {
    let (rows, cols) = o.dims();
    let Some(Value::Cell(d)) = o.get("Data") else {
        unreachable!()
    };
    let mut out = Array::filled(rows, cols, Value::empty());
    for (j, col) in d.data.iter().enumerate() {
        for r in 0..rows {
            let v = match col {
                Value::Cell(c) if c.cols == 1 => c.data[r].clone(),
                other => other.index(&[Idx::List(vec![r], (1, 1)), Idx::All])?,
            };
            out.set(r, j, v);
        }
    }
    Ok(out)
}

fn cell_string(v: &Value) -> String {
    match v {
        Value::Num(a, _) if a.numel() == 1 => {
            let x = a.data[0];
            if x.is_nan() {
                "NaN".into()
            } else {
                display::trim_g(x, 7)
            }
        }
        v if v.is_text() => v.text().unwrap_or_default(),
        other => display::summary(other, Format::Short),
    }
}

fn table_text(o: &Object, _fmt: Format) -> String {
    let (rows, cols) = o.dims();
    let names = o
        .get("VariableNames")
        .map(|v| cellstr_list(v).unwrap_or_default())
        .unwrap_or_default();
    let rn = o
        .get("RowNames")
        .map(|v| cellstr_list(v).unwrap_or_default())
        .unwrap_or_default();
    let cells = match table_cells(o) {
        Ok(c) => c,
        Err(_) => return format!("  {}×{} table\n", rows, cols),
    };
    let texts: Vec<Vec<String>> = (0..rows)
        .map(|r| (0..cols).map(|c| cell_string(cells.get(r, c))).collect())
        .collect();
    let widths: Vec<usize> = (0..cols)
        .map(|c| {
            texts
                .iter()
                .map(|row| row[c].chars().count())
                .chain(std::iter::once(
                    names.get(c).map_or(0, |n| n.chars().count()),
                ))
                .max()
                .unwrap_or(1)
        })
        .collect();
    let rw = rn.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let mut s = format!("  {}×{} table\n\n", rows, cols);
    let lead = if rw > 0 {
        format!("    {:rw$}", "", rw = rw)
    } else {
        String::new()
    };
    s.push_str(&lead);
    for (c, w) in widths.iter().enumerate() {
        s.push_str(&format!(
            "    {:^w$}",
            names.get(c).cloned().unwrap_or_default(),
            w = w
        ));
    }
    s.push('\n');
    s.push_str(&lead);
    for w in &widths {
        s.push_str(&format!("    {}", "_".repeat(*w)));
    }
    s.push_str("\n\n");
    for r in 0..rows {
        if rw > 0 {
            s.push_str(&format!(
                "    {:<rw$}",
                rn.get(r).cloned().unwrap_or_default(),
                rw = rw
            ));
        }
        for (c, w) in widths.iter().enumerate() {
            s.push_str(&format!("    {:>w$}", texts[r][c], w = w));
        }
        s.push('\n');
    }
    s
}

fn writetable(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    let o = table_obj(&a[0])?.clone();
    let file = if a.len() > 1 {
        a[1].text()?
    } else {
        "table.txt".into()
    };
    let mut write_rows = false;
    let mut write_names = true;
    let mut delim = ",".to_string();
    let mut i = 2;
    while i + 1 < a.len() {
        match a[i].text()?.to_lowercase().as_str() {
            "writerownames" => write_rows = a[i + 1].is_true()?,
            "writevariablenames" => write_names = a[i + 1].is_true()?,
            "delimiter" => {
                delim = match a[i + 1].text()?.as_str() {
                    "tab" | "\\t" => "\t".into(),
                    "semi" | ";" => ";".into(),
                    "space" | " " => " ".into(),
                    _ => ",".into(),
                }
            }
            _ => {}
        }
        i += 2;
    }
    let path = super::system::resolve(it, &file);
    let (rows, cols) = o.dims();
    let names = cellstr_list(o.get("VariableNames").unwrap())?;
    let rn = cellstr_list(o.get("RowNames").unwrap()).unwrap_or_default();
    let write_rows = write_rows && !rn.is_empty();
    let cells = table_cells(&o)?;
    // Expand multi-column variables into name_1, name_2, ...
    let mut header: Vec<String> = Vec::new();
    if write_rows {
        header.push("Row".into());
    }
    let mut grid: Vec<Vec<Value>> = vec![Vec::new(); rows];
    for c in 0..cols {
        let width = cells.get(0, c).dims().1.max(1);
        let is_text = cells.get(0, c).is_text();
        if is_text || width == 1 {
            header.push(names[c].clone());
        } else {
            for k in 0..width {
                header.push(format!("{}_{}", names[c], k + 1));
            }
        }
        for r in 0..rows {
            let v = cells.get(r, c);
            if is_text || width == 1 {
                grid[r].push(v.clone());
            } else {
                for x in v.to_num()?.data {
                    grid[r].push(Value::num(x));
                }
            }
        }
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext == "xlsx" || ext == "xls" {
        let mut wb = rust_xlsxwriter::Workbook::new();
        let ws = wb.add_worksheet();
        let mut r0 = 0u32;
        if write_names {
            for (c, h) in header.iter().enumerate() {
                ws.write_string(0, c as u16, h)
                    .map_err(|e| merr!("{}", e))?;
            }
            r0 = 1;
        }
        for (r, row) in grid.iter().enumerate() {
            let mut c = 0u16;
            if write_rows {
                ws.write_string(r0 + r as u32, 0, &rn[r])
                    .map_err(|e| merr!("{}", e))?;
                c = 1;
            }
            for v in row {
                match v {
                    Value::Num(a, _) if a.numel() == 1 => {
                        if a.data[0].is_finite() {
                            ws.write_number(r0 + r as u32, c, a.data[0])
                                .map_err(|e| merr!("{}", e))?;
                        }
                    }
                    other => {
                        ws.write_string(r0 + r as u32, c, cell_string(other))
                            .map_err(|e| merr!("{}", e))?;
                    }
                }
                c += 1;
            }
        }
        wb.save(&path)
            .map_err(|e| merr!("Unable to write '{}': {}", path.display(), e))?;
        return none();
    }
    let quote = |s: &str| {
        if s.contains(delim.as_str()) || s.contains('"') {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    let mut text = String::new();
    if write_names {
        text.push_str(
            &header
                .iter()
                .map(|h| quote(h))
                .collect::<Vec<_>>()
                .join(&delim),
        );
        text.push('\n');
    }
    for (r, row) in grid.iter().enumerate() {
        let mut fields: Vec<String> = Vec::new();
        if write_rows {
            fields.push(quote(&rn[r]));
        }
        for v in row {
            fields.push(match v {
                Value::Num(a, _) if a.numel() == 1 => display::trim_g(a.data[0], 15),
                other => quote(&cell_string(other)),
            });
        }
        text.push_str(&fields.join(&delim));
        text.push('\n');
    }
    std::fs::write(&path, text)
        .map_err(|e| merr!("Unable to write '{}': {}", path.display(), e))?;
    none()
}

// ---------------- tiled layouts and uitable ----------------

fn tiledlayout(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let lead: Vec<f64> = a
        .iter()
        .take_while(|v| !v.is_text())
        .map(|v| v.scalar())
        .collect::<MResult<_>>()?;
    let (fig, m, n) = match lead.as_slice() {
        [f, m, n, ..] => (Some(*f as u32), *m as usize, *n as usize),
        [m, n] => (None, *m as usize, *n as usize),
        [f] if a
            .get(1)
            .is_some_and(|v| v.is_text() && v.text().ok().as_deref() == Some("flow")) =>
        {
            (Some(*f as u32), 2, 2)
        }
        _ if a
            .first()
            .is_some_and(|v| v.text().ok().as_deref() == Some("flow")) =>
        {
            (None, 2, 2)
        }
        _ => {
            return Err(merr!(
                "Specify the grid size, for example tiledlayout(2, 2)."
            ))
        }
    };
    if m == 0 || n == 0 {
        return Err(merr!("The grid size must be positive."));
    }
    let id = match fig {
        Some(f) => it.graphics.figure(Some(f)),
        None => it.graphics.current_figure().id,
    };
    let f = it.graphics.figures.iter_mut().find(|f| f.id == id).unwrap();
    f.axes.clear();
    f.current_axes = 0;
    f.tiles = Some((m, n));
    f.next_tile = 0;
    f.area = None;
    it.graphics.current = Some(id);
    it.graphics.mark(id);
    one(Value::Obj(Box::new(Object::new(
        TILED,
        vec![
            ("Figure".into(), Value::num(id as f64)),
            ("GridSize".into(), Value::row(vec![m as f64, n as f64])),
            ("OuterPosition".into(), Value::row(vec![0.0, 0.0, 1.0, 1.0])),
            ("TileSpacing".into(), Value::chars("loose")),
            ("Padding".into(), Value::chars("loose")),
        ],
    ))))
}

fn nexttile(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let mut rest = &a[..];
    let mut layout: Option<Object> = None;
    if let Some(Value::Obj(o)) = a.first() {
        if o.class == TILED {
            layout = Some((**o).clone());
            rest = &a[1..];
        }
    }
    let id = match &layout {
        Some(o) => o.get("Figure").and_then(|v| v.scalar().ok()).unwrap_or(1.0) as u32,
        None => it.graphics.current_figure().id,
    };
    it.graphics.figure(Some(id));
    let f = it.graphics.figures.iter_mut().find(|f| f.id == id).unwrap();
    let (m, n) = match (&layout, f.tiles) {
        (Some(o), _) => {
            let g = o
                .get("GridSize")
                .and_then(|v| v.to_num().ok())
                .map(|a| a.data)
                .unwrap_or(vec![1.0, 1.0]);
            (g[0] as usize, g[1] as usize)
        }
        (None, Some(t)) => t,
        (None, None) => (1, 1),
    };
    f.tiles = Some((m, n));
    if let Some(o) = &layout {
        if let Some(p) = o.get("OuterPosition").and_then(|v| v.to_num().ok()) {
            if p.numel() == 4 && p.data != [0.0, 0.0, 1.0, 1.0] {
                f.area = Some([p.data[0], p.data[1], p.data[2], p.data[3]]);
            }
        }
    }
    let k = match rest.first() {
        Some(v) if !v.is_text() => v.scalar()? as usize,
        _ => f.next_tile + 1,
    };
    if k == 0 || k > m * n {
        return Err(merr!(
            "Tile number {} is outside the {}x{} layout.",
            k,
            m,
            n
        ));
    }
    f.next_tile = k;
    it.graphics.subplot(m, n, vec![k]);
    one(Value::num(k as f64))
}

fn uitable(it: &mut Interp, a: Vec<Value>, _: usize) -> R {
    let mut i = 0;
    let fig = match a.first() {
        Some(v) if !v.is_text() => {
            i = 1;
            Some(v.scalar()? as u32)
        }
        _ => None,
    };
    let mut data: Option<Value> = None;
    let mut columns: Vec<String> = Vec::new();
    let mut rows: Vec<String> = Vec::new();
    let mut position = [0.0, 0.0, 1.0, 0.3];
    let mut pixels = false;
    while i + 1 < a.len() {
        let key = a[i].text()?.to_lowercase();
        let v = &a[i + 1];
        match key.as_str() {
            "data" => data = Some(v.clone()),
            "columnname" => columns = cellstr_list(v)?,
            "rowname" => rows = cellstr_list(v)?,
            "units" => pixels = v.text()?.eq_ignore_ascii_case("pixels"),
            "position" => {
                let p = v.to_num()?.data;
                if p.len() == 4 {
                    position = [p[0], p[1], p[2], p[3]];
                }
            }
            _ => {}
        }
        i += 2;
    }
    let cells: Vec<Vec<String>> = match &data {
        Some(Value::Cell(c)) => (0..c.rows)
            .map(|r| (0..c.cols).map(|k| cell_string(c.get(r, k))).collect())
            .collect(),
        Some(v) => {
            let m = v.to_num()?;
            (0..m.rows)
                .map(|r| {
                    (0..m.cols)
                        .map(|k| cell_string(&Value::num(*m.get(r, k))))
                        .collect()
                })
                .collect()
        }
        None => Vec::new(),
    };
    let id = match fig {
        Some(f) => it.graphics.figure(Some(f)),
        None => it.graphics.current_figure().id,
    };
    let f = it.graphics.figures.iter_mut().find(|f| f.id == id).unwrap();
    if pixels {
        // Convert pixel positions relative to the figure size when it is known.
        let (w, h) = f.size.unwrap_or((560.0, 420.0));
        position = [
            position[0] / w,
            position[1] / h,
            position[2] / w,
            position[3] / h,
        ];
    }
    f.uitables.push(UiTable {
        cells,
        columns,
        rows,
        position,
    });
    it.graphics.mark(id);
    one(Value::num(id as f64))
}
