//! Assorted functions: tokens, bit operations, application data, captured
//! evaluation, argument checks and small numeric helpers.

use super::*;

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("strtok", strtok);
    b.insert("squeeze", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(a[0].clone())
    });
    b.insert("mfilename", |it, a, _| {
        let path = it
            .frames
            .iter()
            .rev()
            .find_map(|f| {
                f.unit
                    .as_ref()
                    .filter(|u| !u.path.is_empty())
                    .map(|u| u.path.clone())
            })
            .unwrap_or_default();
        let p = std::path::Path::new(&path);
        let full = a
            .first()
            .map(|v| v.text())
            .transpose()?
            .is_some_and(|t| t == "fullpath");
        let s = if full {
            p.with_extension("").to_string_lossy().to_string()
        } else {
            p.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default()
        };
        one(Value::chars(&s))
    });
    // This is a current release: version checks against older ones are false.
    b.insert("verLessThan", |_, a, _| {
        check_args(&a, 2, 2)?;
        one(bool_val(false))
    });
    b.insert("nargchk", |_, a, _| {
        check_args(&a, 3, 4)?;
        let (lo, hi, n) = (a[0].scalar()?, a[1].scalar()?, a[2].scalar()?);
        let msg = if n < lo {
            "Not enough input arguments."
        } else if n > hi {
            "Too many input arguments."
        } else {
            ""
        };
        if a.get(3).map(|v| v.text()).transpose()?.as_deref() == Some("struct") {
            return one(Value::Struct(StructArr::scalar(vec![
                ("message".into(), Value::chars(msg)),
                ("identifier".into(), Value::chars("")),
            ])));
        }
        one(Value::chars(msg))
    });
    b.insert("builtin", |it, a, n| {
        if a.is_empty() {
            return Err(merr!("Not enough input arguments."));
        }
        let name = a[0].text()?;
        let f = it
            .builtins
            .get(name.as_str())
            .copied()
            .ok_or_else(|| merr!("Cannot find builtin function '{}'.", name))?;
        f(it, a[1..].to_vec(), n)
    });
    // Argument names are not tracked through calls; an empty name is the documented
    // result when the argument is not a plain variable.
    b.insert("inputname", |_, a, _| {
        check_args(&a, 1, 2)?;
        one(Value::chars(""))
    });
    b.insert("bitand", |_, a, _| bits(&a, "bitand", |x, y| x & y));
    b.insert("bitor", |_, a, _| bits(&a, "bitor", |x, y| x | y));
    b.insert("bitxor", |_, a, _| bits(&a, "bitxor", |x, y| x ^ y));
    b.insert("bitshift", |_, a, _| {
        check_args(&a, 2, 3)?;
        let x = a[0].to_num()?;
        let k = a[1].to_num()?;
        let out = crate::ops::broadcast(
            &x,
            &k,
            |x, k| {
                let v = x as u64;
                if k >= 0.0 {
                    (v << (k as u32).min(63)) as f64
                } else {
                    (v >> ((-k) as u32).min(63)) as f64
                }
            },
            "bitshift",
        )?;
        one(keep_int(&a[0], out))
    });
    b.insert("bitcmp", |_, a, _| {
        check_args(&a, 1, 2)?;
        let bits = match a[0].num_class() {
            NumClass::Int(k) => (k.range().1 + 1.0).log2().round() as u32,
            _ => 53,
        };
        let mask = if bits >= 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        let out = a[0].to_num()?.map(|x| ((!(*x as u64)) & mask) as f64);
        one(keep_int(&a[0], out))
    });
    b.insert("bitget", |_, a, _| {
        check_args(&a, 2, 2)?;
        let x = a[0].to_num()?;
        let k = a[1].to_num()?;
        let out = crate::ops::broadcast(
            &x,
            &k,
            |x, k| ((x as u64 >> ((k as u32).max(1) - 1)) & 1) as f64,
            "bitget",
        )?;
        one(keep_int(&a[0], out))
    });
    b.insert("bitset", |_, a, _| {
        check_args(&a, 2, 3)?;
        let x = a[0].to_num()?;
        let k = a[1].to_num()?;
        let on = a.get(2).map(|v| v.scalar()).transpose()?.unwrap_or(1.0) != 0.0;
        let out = crate::ops::broadcast(
            &x,
            &k,
            |x, k| {
                let m = 1u64 << ((k as u32).max(1) - 1);
                (if on { x as u64 | m } else { x as u64 & !m }) as f64
            },
            "bitset",
        )?;
        one(keep_int(&a[0], out))
    });
    b.insert("pow2", |_, a, _| {
        check_args(&a, 1, 2)?;
        if a.len() == 2 {
            let f = a[0].to_num()?;
            let e = a[1].to_num()?;
            return one(Value::mat(crate::ops::broadcast(
                &f,
                &e,
                |f, e| f * 2f64.powf(e),
                "pow2",
            )?));
        }
        one(Value::mat(a[0].to_num()?.map(|x| 2f64.powf(*x))))
    });
    b.insert("erfinv", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::mat(a[0].to_num()?.map(|x| erfinv(*x))))
    });
    b.insert("erfcinv", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::mat(a[0].to_num()?.map(|x| erfinv(1.0 - *x))))
    });
    b.insert("mag2db", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::mat(a[0].to_num()?.map(|x| 20.0 * x.log10())))
    });
    b.insert("db2mag", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::mat(a[0].to_num()?.map(|x| 10f64.powf(x / 20.0))))
    });
    b.insert("pow2db", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::mat(a[0].to_num()?.map(|x| 10.0 * x.log10())))
    });
    b.insert("db2pow", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::mat(a[0].to_num()?.map(|x| 10f64.powf(x / 10.0))))
    });
    // Implicit expansion already does what bsxfun was for.
    b.insert("bsxfun", |it, a, _| {
        check_args(&a, 3, 3)?;
        one(it.call1(&a[0], vec![a[1].clone(), a[2].clone()])?)
    });
    b.insert("setappdata", |it, a, _| {
        check_args(&a, 3, 3)?;
        let key = appdata_key(&a[0], &a[1].text()?)?;
        it.appdata.insert(key, a[2].clone());
        none()
    });
    b.insert("getappdata", |it, a, _| {
        check_args(&a, 1, 2)?;
        if a.len() == 1 {
            let prefix = appdata_key(&a[0], "")?;
            let fields: Vec<(String, Value)> = it
                .appdata
                .iter()
                .filter_map(|(k, v)| k.strip_prefix(&prefix).map(|n| (n.to_string(), v.clone())))
                .collect();
            return one(Value::Struct(StructArr::scalar(fields)));
        }
        let key = appdata_key(&a[0], &a[1].text()?)?;
        one(it.appdata.get(&key).cloned().unwrap_or_else(Value::empty))
    });
    b.insert("isappdata", |it, a, _| {
        check_args(&a, 2, 2)?;
        let key = appdata_key(&a[0], &a[1].text()?)?;
        one(bool_val(it.appdata.contains_key(&key)))
    });
    b.insert("rmappdata", |it, a, _| {
        check_args(&a, 2, 2)?;
        let key = appdata_key(&a[0], &a[1].text()?)?;
        it.appdata.remove(&key);
        none()
    });
    b.insert("evalc", |it, a, _| {
        check_args(&a, 1, 1)?;
        let code = a[0].text()?;
        let stream = it.stream.take();
        let before = it.take_output();
        let r = it.eval_string(&code);
        let captured = it.take_output();
        it.stream = stream;
        if !before.is_empty() {
            it.print(&before);
        }
        r?;
        one(Value::chars(&captured))
    });
    b.insert("ishandle", |it, a, _| {
        check_args(&a, 1, 1)?;
        let ids: Vec<f64> = it.graphics.figures.iter().map(|f| f.id as f64).collect();
        let x = a[0].to_num()?;
        one(Value::logical(x.map(|v| {
            if ids.contains(v) {
                1.0
            } else {
                0.0
            }
        })))
    });
    b.insert("ishghandle", |it, a, _| {
        let f = it.builtins["ishandle"];
        f(it, a, 1)
    });
    b.insert("prefdir", |_, _, _| {
        one(Value::chars(&settings_dir().to_string_lossy()))
    });
    b.insert("userpath", |it, _, _| {
        one(Value::chars(&it.cwd.to_string_lossy()))
    });
    b.insert("validateattributes", validateattributes);
    b.insert("validatestring", |_, a, _| {
        if a.len() < 2 {
            return Err(merr!("Not enough input arguments."));
        }
        let s = a[0].text()?;
        let options = cellstr_list(&a[1])?;
        let hits: Vec<&String> = options
            .iter()
            .filter(|o| o.to_lowercase().starts_with(&s.to_lowercase()))
            .collect();
        match hits.as_slice() {
            [one_hit] => one(Value::chars(one_hit)),
            [] => Err(merr!(
                "Expected input to match one of these values:\n\n'{}'\n\nThe input, '{}', did not match any of the valid values.",
                options.join("', '"),
                s
            )),
            many => {
                if let Some(exact) = many.iter().find(|o| o.eq_ignore_ascii_case(&s)) {
                    return one(Value::chars(exact));
                }
                Err(merr!("The input, '{}', matched more than one valid value.", s))
            }
        }
    });
}

fn settings_dir() -> std::path::PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(".beanylab")
}

fn appdata_key(h: &Value, name: &str) -> MResult<String> {
    let id = if h.is_empty() { 0.0 } else { h.scalar()? };
    Ok(format!("{}\u{1}{}", id, name))
}

fn keep_int(src: &Value, out: Array<f64>) -> Value {
    match src.num_class() {
        NumClass::Int(k) => Value::Num(out.map(|x| k.cast(*x)), NumClass::Int(k)),
        _ => Value::mat(out),
    }
}

fn bits(a: &[Value], name: &str, f: fn(u64, u64) -> u64) -> R {
    check_args(a, 2, 3)?;
    let x = a[0].to_num()?;
    let y = a[1].to_num()?;
    if x.data
        .iter()
        .chain(&y.data)
        .any(|v| *v < 0.0 || v.fract() != 0.0)
    {
        return Err(merr!(
            "Double inputs must have integer values in the range of ASSUMEDTYPE."
        ));
    }
    let out = crate::ops::broadcast(&x, &y, |p, q| f(p as u64, q as u64) as f64, name)?;
    let src = if matches!(a[0].num_class(), NumClass::Int(_)) {
        &a[0]
    } else {
        &a[1]
    };
    Ok(vec![keep_int(src, out)])
}

/// `[token, remainder] = strtok(str, delimiters)`.
fn strtok(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 2)?;
    let is_string = matches!(a[0], Value::Str(_));
    let s = a[0].text()?;
    let delims: Vec<char> = match a.get(1) {
        Some(d) => d.text()?.chars().collect(),
        None => vec![' ', '\t', '\n', '\r', '\u{b}', '\u{c}', '\0'],
    };
    let chars: Vec<char> = s.chars().collect();
    let start = chars
        .iter()
        .position(|c| !delims.contains(c))
        .unwrap_or(chars.len());
    let end = chars[start..]
        .iter()
        .position(|c| delims.contains(c))
        .map(|k| start + k)
        .unwrap_or(chars.len());
    let tok: String = chars[start..end].iter().collect();
    let rest: String = chars[end..].iter().collect();
    let mk = |t: &str| {
        if is_string {
            Value::string(t)
        } else {
            Value::chars(t)
        }
    };
    Ok(vec![mk(&tok), mk(&rest)])
}

/// Inverse error function: rational first guess refined by Newton steps.
fn erfinv(y: f64) -> f64 {
    if !(-1.0..=1.0).contains(&y) {
        return f64::NAN;
    }
    if y == 1.0 {
        return f64::INFINITY;
    }
    if y == -1.0 {
        return f64::NEG_INFINITY;
    }
    let a = 0.147;
    let ln = (1.0 - y * y).ln();
    let t = 2.0 / (std::f64::consts::PI * a) + ln / 2.0;
    let mut x = y.signum() * ((t * t - ln / a).sqrt() - t).sqrt();
    for _ in 0..3 {
        let err = libm::erf(x) - y;
        x -= err / (2.0 / std::f64::consts::PI.sqrt() * (-x * x).exp());
    }
    x
}

fn validateattributes(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 3 {
        return Err(merr!("Not enough input arguments."));
    }
    let v = &a[0];
    let classes = cellstr_list(&a[1])?;
    let cls = v.class_name();
    let numeric = matches!(v, Value::Num(..) | Value::Complex(_)) && cls != "logical";
    if !classes.is_empty()
        && !classes.iter().any(|c| {
            c == cls
                || (c == "numeric" && numeric)
                || (c == "float" && matches!(cls, "double" | "single"))
                || matches!(v, Value::Inst(o) if o.class.isa(c))
        })
    {
        return Err(merr!(
            "Expected input to be one of these types:\n\n{}\n\nInstead its type was {}.",
            classes.join(", "),
            cls
        ));
    }
    let attrs = match &a[2] {
        Value::Cell(c) => c.data.clone(),
        _ => Vec::new(),
    };
    let x = v.to_num().ok();
    let all = |f: &dyn Fn(f64) -> bool| x.as_ref().is_none_or(|m| m.data.iter().all(|e| f(*e)));
    let (r, c) = v.dims();
    for at in attrs.iter().filter(|x| x.is_text()) {
        let name = at.text()?;
        let ok = match name.as_str() {
            "nonempty" => r * c > 0,
            "scalar" => r * c == 1,
            "vector" => r == 1 || c == 1,
            "row" => r == 1,
            "column" => c == 1,
            "2d" | "nonsparse" => true,
            "square" => r == c,
            "real" => !v.is_complex(),
            "finite" => all(&|e| e.is_finite()),
            "nonnan" => all(&|e| !e.is_nan()),
            "integer" => all(&|e| e.is_finite() && e.fract() == 0.0),
            "positive" => all(&|e| e > 0.0),
            "nonnegative" => all(&|e| e >= 0.0),
            "nonzero" => all(&|e| e != 0.0),
            "binary" => all(&|e| e == 0.0 || e == 1.0),
            "even" => all(&|e| e % 2.0 == 0.0),
            "odd" => all(&|e| e.abs() % 2.0 == 1.0),
            "increasing" => x
                .as_ref()
                .is_none_or(|m| m.data.windows(2).all(|w| w[1] > w[0])),
            "nondecreasing" => x
                .as_ref()
                .is_none_or(|m| m.data.windows(2).all(|w| w[1] >= w[0])),
            "decreasing" => x
                .as_ref()
                .is_none_or(|m| m.data.windows(2).all(|w| w[1] < w[0])),
            _ => true,
        };
        if !ok {
            return Err(merr!("Expected input to be {}.", name));
        }
    }
    none()
}
