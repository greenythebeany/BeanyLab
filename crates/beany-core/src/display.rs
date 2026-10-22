//! Text rendering of values for the command window.

use crate::value::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Short,
    Long,
}

const WRAP: usize = 100;

/// Shortest round-trippable-ish text for a number (used by num2str-like conversions).
pub fn num_to_str(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Inf".into() } else { "-Inf".into() };
    }
    if x.fract() == 0.0 && x.abs() < 1e15 {
        return format!("{}", x as i64);
    }
    let digits = if x.abs() >= 1.0 {
        (x.abs().log10().floor() as i32 + 1).max(1)
    } else {
        1
    };
    let prec = (digits + 4).max(5) as usize;
    trim_g(x, prec)
}

/// Emulates C's `%.{prec}g`.
pub fn trim_g(x: f64, prec: usize) -> String {
    fmt_g(x, prec, false)
}

pub fn fmt_g(x: f64, prec: usize, keep_zeros: bool) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Inf".into() } else { "-Inf".into() };
    }
    if x == 0.0 {
        return "0".into();
    }
    let prec = prec.max(1);
    let exp = x.abs().log10().floor() as i32;
    let sci = format!("{:.*e}", prec - 1, x);
    // Exponent after rounding.
    let (mant, e) = sci.split_once('e').unwrap();
    let e: i32 = e.parse().unwrap_or(exp);
    if e < -4 || e >= prec as i32 {
        let mant = if keep_zeros {
            mant.to_string()
        } else {
            strip_zeros(mant)
        };
        let sign = if e < 0 { '-' } else { '+' };
        format!("{}e{}{:02}", mant, sign, e.abs())
    } else {
        let decimals = (prec as i32 - 1 - e).max(0) as usize;
        let s = format!("{:.*}", decimals, x);
        if keep_zeros {
            s
        } else {
            strip_zeros(&s)
        }
    }
}

fn strip_zeros(s: &str) -> String {
    if s.contains('.') {
        let t = s.trim_end_matches('0');
        t.trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

fn is_int_like(a: &[f64]) -> bool {
    a.iter()
        .all(|x| !x.is_finite() || (x.fract() == 0.0 && x.abs() < 1e10))
}

fn fmt_special(x: f64) -> Option<String> {
    if x.is_nan() {
        Some("NaN".into())
    } else if x.is_infinite() {
        Some(if x > 0.0 { "Inf".into() } else { "-Inf".into() })
    } else {
        None
    }
}

/// Format a single real number the way scalars are shown.
pub fn fmt_scalar(x: f64, fmt: Format) -> String {
    if let Some(s) = fmt_special(x) {
        return s;
    }
    if x.fract() == 0.0 && x.abs() < 1e10 {
        return format!("{}", x as i64);
    }
    let decimals = if fmt == Format::Long { 15 } else { 4 };
    let ax = x.abs();
    if ax >= 1e5 || ax < 1e-4 {
        let sig = if fmt == Format::Long { 15 } else { 4 };
        let s = format!("{:.*e}", sig, x);
        let (m, e) = s.split_once('e').unwrap();
        let e: i32 = e.parse().unwrap();
        format!("{}e{}{:02}", m, if e < 0 { '-' } else { '+' }, e.abs())
    } else {
        format!("{:.*}", decimals, x)
    }
}

/// One part of a complex number: fixed decimals, or exponent form for `sci`.
fn fmt_part(x: f64, fmt: Format, sci: bool) -> String {
    if let Some(s) = fmt_special(x.abs()) {
        return s;
    }
    let decimals = if fmt == Format::Long { 15 } else { 4 };
    if sci {
        let s = format!("{:.*e}", decimals, x.abs());
        let (m, e) = s.split_once('e').unwrap();
        let e: i32 = e.parse().unwrap();
        format!("{}e{}{:02}", m, if e < 0 { '-' } else { '+' }, e.abs())
    } else {
        format!("{:.*}", decimals, x.abs())
    }
}

/// Use exponent form when the magnitudes are too large or small for fixed decimals.
fn cplx_sci(zs: &[C64]) -> bool {
    let m = zs
        .iter()
        .flat_map(|z| [z.re.abs(), z.im.abs()])
        .filter(|x| x.is_finite())
        .fold(0.0f64, f64::max);
    m >= 1e5 || (m > 0.0 && m < 1e-4)
}

/// `1.0000 + 2.0000i` style text for a complex number.
pub fn fmt_cplx(z: C64, fmt: Format, sci: bool) -> String {
    let re = fmt_part(z.re, fmt, sci);
    let im = fmt_part(z.im, fmt, sci);
    let re = if z.re < 0.0 { format!("-{}", re) } else { re };
    let sign = if z.im < 0.0 { '-' } else { '+' };
    format!("{} {} {}i", re, sign, im)
}

/// Compact `1+2i` text (num2str and summaries).
pub fn cplx_to_str(z: C64) -> String {
    let im = num_to_str(z.im.abs());
    let sign = if z.im < 0.0 { '-' } else { '+' };
    format!("{}{}{}i", num_to_str(z.re), sign, im)
}

fn cplx_body(a: &Array<C64>, fmt: Format) -> String {
    if a.is_empty() {
        return String::new();
    }
    let sci = cplx_sci(&a.data);
    let cells: Vec<String> = a.data.iter().map(|z| fmt_cplx(*z, fmt, sci)).collect();
    let width = cells.iter().map(|s| s.chars().count()).max().unwrap_or(1) + 3;
    let per_line = (WRAP / width).max(1);
    let mut out = String::new();
    let chunks: Vec<(usize, usize)> = (0..a.cols)
        .step_by(per_line)
        .map(|s| (s, (s + per_line).min(a.cols)))
        .collect();
    for (ci, (c0, c1)) in chunks.iter().enumerate() {
        if chunks.len() > 1 {
            if c1 - c0 == 1 {
                out.push_str(&format!("  Column {}\n\n", c0 + 1));
            } else {
                out.push_str(&format!("  Columns {} through {}\n\n", c0 + 1, c1));
            }
        }
        for r in 0..a.rows {
            for c in *c0..*c1 {
                out.push_str(&format!("{:>w$}", cells[c * a.rows + r], w = width));
            }
            out.push('\n');
        }
        if ci + 1 < chunks.len() {
            out.push('\n');
        }
    }
    out
}

fn dims_str(r: usize, c: usize) -> String {
    format!("{}×{}", r, c)
}

/// Render the body of a numeric matrix (without name header).
fn matrix_body(a: &Array<f64>, class: NumClass, fmt: Format) -> String {
    if a.is_empty() {
        return String::new();
    }
    let ints = is_int_like(&a.data) || matches!(class, NumClass::Int(_) | NumClass::Logical);
    let mut scale_note = String::new();
    let cells: Vec<String> = if ints {
        a.data
            .iter()
            .map(|x| fmt_special(*x).unwrap_or_else(|| format!("{}", *x as i64)))
            .collect()
    } else {
        let maxabs = a
            .data
            .iter()
            .filter(|x| x.is_finite())
            .fold(0.0f64, |m, x| m.max(x.abs()));
        let decimals = if fmt == Format::Long { 15 } else { 4 };
        if maxabs >= 1e5 || (maxabs < 1e-3 && maxabs > 0.0) {
            let p = maxabs.log10().floor() as i32;
            let scale = 10f64.powi(p);
            scale_note = format!(
                "   1.0e{}{:02} *\n\n",
                if p < 0 { '-' } else { '+' },
                p.abs()
            );
            a.data
                .iter()
                .map(|x| fmt_special(*x).unwrap_or_else(|| format!("{:.*}", decimals, x / scale)))
                .collect()
        } else {
            // Exact zeros print as a bare 0 among decimals.
            a.data
                .iter()
                .map(|x| {
                    fmt_special(*x).unwrap_or_else(|| {
                        if *x == 0.0 {
                            "0".into()
                        } else {
                            format!("{:.*}", decimals, x)
                        }
                    })
                })
                .collect()
        }
    };
    let maxw = cells.iter().map(|s| s.chars().count()).max().unwrap_or(1);
    let width = if matches!(class, NumClass::Logical) {
        maxw + 3
    } else if ints {
        (maxw + 3).max(6)
    } else {
        maxw + 4
    };
    let per_line = (WRAP / width).max(1);
    let mut out = scale_note;
    let chunks: Vec<(usize, usize)> = (0..a.cols)
        .step_by(per_line)
        .map(|s| (s, (s + per_line).min(a.cols)))
        .collect();
    for (ci, (c0, c1)) in chunks.iter().enumerate() {
        if chunks.len() > 1 {
            if c1 - c0 == 1 {
                out.push_str(&format!("  Column {}\n\n", c0 + 1));
            } else {
                out.push_str(&format!("  Columns {} through {}\n\n", c0 + 1, c1));
            }
        }
        for r in 0..a.rows {
            for c in *c0..*c1 {
                let s = &cells[c * a.rows + r];
                out.push_str(&format!("{:>w$}", s, w = width));
            }
            out.push('\n');
        }
        if ci + 1 < chunks.len() {
            out.push('\n');
        }
    }
    out
}

/// Short one-line summary used inside cells and struct field listings.
pub fn summary(v: &Value, fmt: Format) -> String {
    match v {
        Value::Num(a, c) => {
            if a.numel() == 1 {
                let s = if *c == NumClass::Logical {
                    format!("{}", a.data[0] as i64)
                } else {
                    fmt_scalar(a.data[0], fmt)
                };
                s
            } else if a.is_empty() {
                if a.rows == 0 && a.cols == 0 {
                    "[]".into()
                } else {
                    format!("[{} {}]", dims_str(a.rows, a.cols), c.name())
                }
            } else if a.rows == 1 && a.cols <= 10 {
                let parts: Vec<String> = a
                    .data
                    .iter()
                    .map(|x| {
                        if is_int_like(&a.data) {
                            fmt_special(*x).unwrap_or_else(|| format!("{}", *x as i64))
                        } else {
                            fmt_scalar(*x, fmt)
                        }
                    })
                    .collect();
                format!("[{}]", parts.join(" "))
            } else {
                format!("[{} {}]", dims_str(a.rows, a.cols), c.name())
            }
        }
        Value::Char(a) if a.rows <= 1 => format!("'{}'", a.data.iter().collect::<String>()),
        Value::Char(a) => format!("[{} char]", dims_str(a.rows, a.cols)),
        Value::Str(a) if a.numel() == 1 => format!("\"{}\"", a.data[0]),
        Value::Str(a) => format!("[{} string]", dims_str(a.rows, a.cols)),
        Value::Cell(a) => format!("{{{} cell}}", dims_str(a.rows, a.cols)),
        Value::Struct(s) => format!("[{} struct]", dims_str(s.arr.rows, s.arr.cols)),
        Value::Func(f) => f.text(),
        Value::Obj(o) => {
            let (r, c) = o.dims();
            format!("[{} {}]", dims_str(r, c), o.class)
        }
        Value::Inst(o) => {
            let (r, c) = o.dims();
            format!("[{} {}]", dims_str(r, c), o.class.name)
        }
        Value::Complex(a) if a.numel() == 1 => fmt_cplx(a.data[0], fmt, cplx_sci(&a.data)),
        Value::Complex(a) => format!("[{} double]", dims_str(a.rows, a.cols)),
    }
}

/// Text that `disp` prints.
pub fn disp(v: &Value, fmt: Format) -> String {
    match v {
        Value::Num(a, c) => {
            if a.is_empty() {
                return String::new();
            }
            if a.numel() == 1 {
                let s = if *c == NumClass::Logical {
                    format!("{}", a.data[0] as i64)
                } else {
                    fmt_scalar(a.data[0], fmt)
                };
                return format!("{:>w$}\n", s, w = s.len() + 3);
            }
            matrix_body(a, *c, fmt)
        }
        Value::Char(a) => {
            let mut out = String::new();
            for r in 0..a.rows {
                for c in 0..a.cols {
                    out.push(*a.get(r, c));
                }
                out.push('\n');
            }
            out
        }
        Value::Str(a) => {
            if a.numel() == 1 {
                format!("{}\n", a.data[0])
            } else {
                let mut out = String::new();
                for r in 0..a.rows {
                    out.push_str("   ");
                    for c in 0..a.cols {
                        out.push_str(&format!(" \"{}\"", a.get(r, c)));
                    }
                    out.push('\n');
                }
                out
            }
        }
        Value::Cell(a) => cell_body(a, fmt),
        Value::Struct(s) => struct_body(s, fmt),
        Value::Func(f) => format!("    {}\n", f.text()),
        Value::Obj(o) => crate::builtins::object_text(o, fmt),
        Value::Inst(o) => instance_body(o, fmt),
        Value::Complex(a) if a.numel() == 1 => {
            let t = fmt_cplx(a.data[0], fmt, cplx_sci(&a.data));
            format!("{:>w$}\n", t, w = t.len() + 3)
        }
        Value::Complex(a) => cplx_body(a, fmt),
    }
}

/// Default text for an object of a user class: its class and properties.
pub fn instance_body(o: &Instance, fmt: Format) -> String {
    let s = o.props();
    let (r, c) = o.dims();
    if r * c != 1 {
        return format!(
            "  {} {} array with properties:\n\n{}",
            dims_str(r, c),
            o.class.name,
            s.keys
                .iter()
                .map(|k| format!("    {}\n", k))
                .collect::<String>()
        );
    }
    if s.keys.is_empty() {
        return format!("  {} with no properties.\n", o.class.name);
    }
    format!(
        "  {} with properties:\n\n{}",
        o.class.name,
        struct_body(&s, fmt)
    )
}

fn cell_body(a: &Array<Value>, fmt: Format) -> String {
    if a.is_empty() {
        return String::new();
    }
    let cells: Vec<String> = a
        .data
        .iter()
        .map(|v| {
            let s = summary(v, fmt);
            match v {
                Value::Num(x, _) if x.numel() == 1 => format!("{{[{}]}}", s),
                Value::Num(x, _) if x.rows == 0 && x.cols == 0 => "{0×0 double}".to_string(),
                Value::Num(..) if s.contains('×') => {
                    format!("{{{}}}", s.trim_start_matches('[').trim_end_matches(']'))
                }
                Value::Num(..) => format!("{{{}}}", s),
                _ => format!("{{{}}}", s),
            }
        })
        .collect();
    let mut widths = vec![0usize; a.cols];
    for c in 0..a.cols {
        for r in 0..a.rows {
            widths[c] = widths[c].max(cells[c * a.rows + r].chars().count());
        }
    }
    let mut out = String::new();
    for r in 0..a.rows {
        out.push_str("  ");
        for c in 0..a.cols {
            out.push_str(&format!("  {:<w$}", cells[c * a.rows + r], w = widths[c]));
        }
        out.push('\n');
    }
    out
}

fn struct_body(s: &StructArr, fmt: Format) -> String {
    if s.arr.numel() != 1 {
        let mut out = format!(
            "  {} struct array with fields:\n\n",
            dims_str(s.arr.rows, s.arr.cols)
        );
        for k in &s.keys {
            out.push_str(&format!("    {}\n", k));
        }
        return out;
    }
    let w = s.keys.iter().map(|k| k.chars().count()).max().unwrap_or(0);
    let mut out = String::new();
    for (i, k) in s.keys.iter().enumerate() {
        out.push_str(&format!(
            "    {:>w$}: {}\n",
            k,
            summary(&s.arr.data[0][i], fmt),
            w = w
        ));
    }
    out
}

/// Full display for `x = ...` echo.
pub fn display(name: &str, v: &Value, fmt: Format) -> String {
    match v {
        Value::Num(a, c) => {
            if a.numel() == 1 {
                if *c == NumClass::Double {
                    return format!("{} = {}\n\n", name, fmt_scalar(a.data[0], fmt));
                }
                // Other classes name the class above the value.
                let s = match c {
                    NumClass::Logical | NumClass::Int(_) => format!("{}", a.data[0] as i64),
                    _ => fmt_scalar(a.data[0], fmt),
                };
                return format!(
                    "{} =\n\n  {}\n\n{:>w$}\n\n",
                    name,
                    c.name(),
                    s,
                    w = s.len() + 3
                );
            }
            if a.is_empty() {
                if a.rows == 0 && a.cols == 0 && *c == NumClass::Double {
                    return format!("{} =\n\n     []\n\n", name);
                }
                return format!(
                    "{} =\n\n  {} empty {} array\n\n",
                    name,
                    dims_str(a.rows, a.cols),
                    c.name()
                );
            }
            let header = match c {
                NumClass::Double => String::new(),
                other => format!("  {} {} array\n\n", dims_str(a.rows, a.cols), other.name()),
            };
            format!("{} =\n\n{}{}\n", name, header, matrix_body(a, *c, fmt))
        }
        Value::Char(a) => {
            if a.rows <= 1 {
                format!(
                    "{} =\n\n    '{}'\n\n",
                    name,
                    a.data.iter().collect::<String>()
                )
            } else {
                let mut out = format!(
                    "{} =\n\n  {} char array\n\n",
                    name,
                    dims_str(a.rows, a.cols)
                );
                for r in 0..a.rows {
                    let row: String = (0..a.cols).map(|c| *a.get(r, c)).collect();
                    out.push_str(&format!("    '{}'\n", row));
                }
                out.push('\n');
                out
            }
        }
        Value::Str(a) => {
            if a.numel() == 1 {
                format!("{} = \n\n    \"{}\"\n\n", name, a.data[0])
            } else {
                format!(
                    "{} = \n\n  {} string array\n\n{}\n",
                    name,
                    dims_str(a.rows, a.cols),
                    disp(v, fmt)
                )
            }
        }
        Value::Cell(a) => {
            if a.is_empty() {
                return format!(
                    "{} =\n\n  {} empty cell array\n\n",
                    name,
                    dims_str(a.rows, a.cols)
                );
            }
            format!(
                "{} =\n\n  {} cell array\n\n{}\n",
                name,
                dims_str(a.rows, a.cols),
                cell_body(a, fmt)
            )
        }
        Value::Struct(s) => {
            if s.arr.numel() == 1 {
                if s.keys.is_empty() {
                    return format!("{} = \n\n  struct with no fields.\n\n", name);
                }
                format!(
                    "{} = \n\n  struct with fields:\n\n{}\n",
                    name,
                    struct_body(s, fmt)
                )
            } else {
                format!("{} = \n\n{}\n", name, struct_body(s, fmt))
            }
        }
        Value::Func(f) => format!(
            "{} =\n\n  function_handle with value:\n\n    {}\n\n",
            name,
            f.text()
        ),
        // Model text starts with its own spacer line.
        Value::Obj(o) if crate::builtins::control::is_lti(v) && o.class != "frd" => {
            format!("{} =\n{}\n", name, crate::builtins::object_text(o, fmt))
        }
        Value::Obj(o) => format!("{} =\n\n{}\n", name, crate::builtins::object_text(o, fmt)),
        Value::Inst(o) => format!("{} = \n\n{}\n", name, instance_body(o, fmt)),
        Value::Complex(a) if a.numel() == 1 => {
            format!(
                "{} = {}\n\n",
                name,
                fmt_cplx(a.data[0], fmt, cplx_sci(&a.data))
            )
        }
        Value::Complex(a) => format!("{} =\n\n{}\n", name, cplx_body(a, fmt)),
    }
}
