//! Text functions, printf-style formatting and regular expressions.

use regex::{Regex, RegexBuilder};

use super::*;
use crate::display::{fmt_g, num_to_str, trim_g};

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("sprintf", |_, a, _| {
        if a.is_empty() {
            return Err(merr!("Not enough input arguments."));
        }
        let f = a[0].text()?;
        one(Value::chars(&format_printf(&f, &a[1..])?))
    });
    b.insert("num2str", num2str);
    b.insert("int2str", |_, a, _| {
        check_args(&a, 1, 1)?;
        let m = a[0].to_num()?;
        let rounded = Value::mat(m.map(|x| x.round()));
        num2str_value(&rounded, None)
    });
    b.insert("str2num", |it, a, _| {
        check_args(&a, 1, 1)?;
        let t = a[0].text()?;
        let saved = it.frames.len();
        let r = crate::parser::parse_expression(&format!("[{}]", t)).and_then(|e| it.eval(&e));
        it.frames.truncate(saved);
        match r {
            Ok(v) => Ok(vec![v, Value::boolean(true)]),
            Err(_) => Ok(vec![Value::empty(), Value::boolean(false)]),
        }
    });
    b.insert("str2double", |_, a, _| {
        check_args(&a, 1, 1)?;
        let parse = |s: &str| -> f64 {
            let t = s.trim().replace(',', "");
            match t.to_lowercase().as_str() {
                "inf" | "+inf" => f64::INFINITY,
                "-inf" => f64::NEG_INFINITY,
                "nan" => f64::NAN,
                _ => t.parse::<f64>().unwrap_or(f64::NAN),
            }
        };
        match &a[0] {
            Value::Cell(c) => one(Value::mat(Array::new(
                c.rows,
                c.cols,
                c.data
                    .iter()
                    .map(|v| v.text().map(|s| parse(&s)).unwrap_or(f64::NAN))
                    .collect(),
            ))),
            Value::Str(s) => one(Value::mat(s.map(|x| parse(x)))),
            v if v.is_text() => one(Value::num(parse(&v.text()?))),
            _ => one(Value::num(f64::NAN)),
        }
    });
    b.insert("mat2str", mat2str);
    b.insert("string", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(match &a[0] {
            Value::Str(_) => a[0].clone(),
            Value::Char(c) if c.rows <= 1 => Value::string(&a[0].text()?),
            Value::Num(m, NumClass::Logical) => Value::Str(m.map(|x| {
                if *x != 0.0 {
                    "true".into()
                } else {
                    "false".into()
                }
            })),
            Value::Num(m, _) => Value::Str(m.map(|x| num_to_str(*x))),
            Value::Cell(c) => {
                let mut out = Vec::new();
                for v in &c.data {
                    out.push(v.text()?);
                }
                Value::Str(Array::new(c.rows, c.cols, out))
            }
            other => {
                return Err(merr!(
                    "Conversion to string from {} is not possible.",
                    other.class_name()
                ))
            }
        })
    });
    b.insert("strings", |_, a, _| {
        let (r, c, _) = size_args(&a)?;
        one(Value::Str(Array::filled(r, c, String::new())))
    });
    b.insert("char", char_fn);
    b.insert("cellstr", |_, a, _| {
        check_args(&a, 1, 1)?;
        if let Value::Cell(_) = &a[0] {
            return one(a[0].clone());
        }
        let list = cellstr_list(&a[0])?;
        let n = list.len();
        one(Value::Cell(Array::new(
            n,
            1,
            list.iter().map(|s| Value::chars(s)).collect(),
        )))
    });
    b.insert("strcat", strcat);
    b.insert("strvcat", char_fn);
    b.insert("strcmp", |_, a, _| compare(&a, |x, y| x == y));
    b.insert("strcmpi", |_, a, _| {
        compare(&a, |x, y| x.to_lowercase() == y.to_lowercase())
    });
    b.insert("strncmp", |_, a, _| {
        check_args(&a, 3, 3)?;
        let n = a[2].scalar()? as usize;
        compare_n(&a, n, false)
    });
    b.insert("strncmpi", |_, a, _| {
        check_args(&a, 3, 3)?;
        let n = a[2].scalar()? as usize;
        compare_n(&a, n, true)
    });
    b.insert("strfind", |_, a, _| {
        check_args(&a, 2, 2)?;
        let pat: Vec<char> = a[1].text()?.chars().collect();
        let find = |s: &str| -> Value {
            let c: Vec<char> = s.chars().collect();
            let mut out = Vec::new();
            if !pat.is_empty() && pat.len() <= c.len() {
                for i in 0..=c.len() - pat.len() {
                    if c[i..i + pat.len()] == pat[..] {
                        out.push((i + 1) as f64);
                    }
                }
            }
            if out.is_empty() {
                Value::Num(Array::new(1, 0, vec![]), NumClass::Double)
            } else {
                Value::row(out)
            }
        };
        map_text(&a[0], |s| Ok(find(s)))
    });
    b.insert("strrep", |_, a, _| {
        check_args(&a, 3, 3)?;
        let from = a[1].text()?;
        let to = a[2].text()?;
        map_text_keep(&a[0], |s| {
            if from.is_empty() {
                s.to_string()
            } else {
                s.replace(&from, &to)
            }
        })
    });
    b.insert("replace", |_, a, _| {
        check_args(&a, 3, 3)?;
        let from = a[1].text()?;
        let to = a[2].text()?;
        map_text_keep(&a[0], |s| s.replace(&from, &to))
    });
    b.insert("strtrim", |_, a, _| {
        check_args(&a, 1, 1)?;
        map_text_keep(&a[0], |s| {
            s.trim_matches(|c: char| c.is_whitespace() || c == '\0')
                .to_string()
        })
    });
    b.insert("deblank", |_, a, _| {
        check_args(&a, 1, 1)?;
        map_text_keep(&a[0], |s| {
            s.trim_end_matches(|c: char| c.is_whitespace() || c == '\0')
                .to_string()
        })
    });
    b.insert("upper", |_, a, _| {
        check_args(&a, 1, 1)?;
        map_text_keep(&a[0], |s| s.to_uppercase())
    });
    b.insert("lower", |_, a, _| {
        check_args(&a, 1, 1)?;
        map_text_keep(&a[0], |s| s.to_lowercase())
    });
    b.insert("blanks", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::chars(&" ".repeat(a[0].scalar()? as usize)))
    });
    b.insert("newline", |_, _, _| one(Value::chars("\n")));
    b.insert("strsplit", strsplit);
    b.insert("split", |it, a, n| {
        let r = strsplit(it, a.clone(), n)?;
        let Value::Cell(c) = &r[0] else { return Ok(r) };
        let strs: Vec<String> = c
            .data
            .iter()
            .map(|v| v.text().unwrap_or_default())
            .collect();
        let len = strs.len();
        one(Value::Str(Array::new(len, 1, strs)))
    });
    b.insert("strjoin", |_, a, _| {
        check_args(&a, 1, 2)?;
        let list = cellstr_list(&a[0])?;
        let sep = if a.len() > 1 {
            unescape(&a[1].text()?)
        } else {
            " ".into()
        };
        one(Value::chars(&list.join(&sep)))
    });
    b.insert("join", |_, a, _| {
        check_args(&a, 1, 2)?;
        let list = cellstr_list(&a[0])?;
        let sep = if a.len() > 1 {
            a[1].text()?
        } else {
            " ".into()
        };
        one(Value::string(&list.join(&sep)))
    });
    b.insert("contains", |_, a, _| text_pred(&a, |s, p| s.contains(p)));
    b.insert("startsWith", |_, a, _| {
        text_pred(&a, |s, p| s.starts_with(p))
    });
    b.insert("endsWith", |_, a, _| text_pred(&a, |s, p| s.ends_with(p)));
    b.insert("strlength", |_, a, _| {
        check_args(&a, 1, 1)?;
        match &a[0] {
            Value::Str(s) => one(Value::mat(s.map(|x| x.chars().count() as f64))),
            Value::Cell(c) => one(Value::mat(Array::new(
                c.rows,
                c.cols,
                c.data
                    .iter()
                    .map(|v| v.text().map(|t| t.chars().count() as f64).unwrap_or(0.0))
                    .collect(),
            ))),
            v => one(Value::num(v.text()?.chars().count() as f64)),
        }
    });
    b.insert("isspace", |_, a, _| char_pred(&a, |c| c.is_whitespace()));
    b.insert("isletter", |_, a, _| char_pred(&a, |c| c.is_alphabetic()));
    b.insert("isstrprop", |_, a, _| {
        check_args(&a, 2, 2)?;
        let kind = a[1].text()?;
        let f: fn(char) -> bool = match kind.as_str() {
            "alpha" => |c| c.is_alphabetic(),
            "digit" => |c| c.is_ascii_digit(),
            "alphanum" => |c| c.is_alphanumeric(),
            "wspace" => |c| c.is_whitespace(),
            "upper" => |c| c.is_uppercase(),
            "lower" => |c| c.is_lowercase(),
            "punct" => |c| c.is_ascii_punctuation(),
            _ => return Err(merr!("Unsupported category '{}'", kind)),
        };
        char_pred(&a[..1], f)
    });
    b.insert("iscellstr", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(bool_val(matches!(&a[0], Value::Cell(c) if c.data.iter().all(|v| matches!(v, Value::Char(x) if x.rows <= 1)))))
    });
    b.insert("dec2bin", |_, a, _| {
        check_args(&a, 1, 2)?;
        let x = a[0].scalar()? as u64;
        let w = if a.len() > 1 {
            a[1].scalar()? as usize
        } else {
            1
        };
        one(Value::chars(&format!("{:0w$b}", x, w = w)))
    });
    b.insert("bin2dec", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::num(
            u64::from_str_radix(a[0].text()?.trim(), 2)
                .map(|x| x as f64)
                .unwrap_or(f64::NAN),
        ))
    });
    b.insert("dec2hex", |_, a, _| {
        check_args(&a, 1, 2)?;
        one(Value::chars(&format!("{:X}", a[0].scalar()? as u64)))
    });
    b.insert("hex2dec", |_, a, _| {
        check_args(&a, 1, 1)?;
        one(Value::num(
            u64::from_str_radix(a[0].text()?.trim(), 16)
                .map(|x| x as f64)
                .unwrap_or(f64::NAN),
        ))
    });
    b.insert("regexp", |_, a, n| regexp(&a, n, false));
    b.insert("regexpi", |_, a, n| regexp(&a, n, true));
    b.insert("regexprep", regexprep);
    b.insert("lettersPattern", |_, _, _| {
        Err(merr!("Pattern objects are not supported yet"))
    });
}

/// Expand backslash escapes used by sprintf/fprintf format strings.
pub fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('a') => out.push('\x07'),
                Some('b') => out.push('\x08'),
                Some('f') => out.push('\x0c'),
                Some('v') => out.push('\x0b'),
                Some('\\') => out.push('\\'),
                Some('0') => out.push('\0'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[derive(Clone)]
enum PArg {
    Num(f64),
    Text(String),
}

fn flatten_args(args: &[Value]) -> MResult<Vec<PArg>> {
    let mut out = Vec::new();
    for a in args {
        match a {
            Value::Num(m, _) => out.extend(m.data.iter().map(|x| PArg::Num(*x))),
            Value::Char(_) => out.push(PArg::Text(a.text()?)),
            Value::Str(s) => out.extend(s.data.iter().map(|x| PArg::Text(x.clone()))),
            Value::Cell(c) => {
                for v in &c.data {
                    out.extend(flatten_args(std::slice::from_ref(v))?);
                }
            }
            Value::Func(f) => out.push(PArg::Text(f.text())),
            Value::Struct(_) => return Err(merr!("Function is not defined for 'struct' inputs.")),
            Value::Obj(o) => {
                return Err(merr!("Function is not defined for '{}' inputs.", o.class))
            }
        }
    }
    Ok(out)
}

struct Spec {
    flags: String,
    width: Option<usize>,
    prec: Option<usize>,
    conv: char,
}

enum Piece {
    Lit(String),
    Spec(Spec),
}

fn parse_format(fmt: &str) -> Vec<Piece> {
    let chars: Vec<char> = fmt.chars().collect();
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '%' {
            lit.push(c);
            i += 1;
            continue;
        }
        if chars.get(i + 1) == Some(&'%') {
            lit.push('%');
            i += 2;
            continue;
        }
        let mut j = i + 1;
        let mut flags = String::new();
        while j < chars.len() && "-+ 0#".contains(chars[j]) {
            flags.push(chars[j]);
            j += 1;
        }
        let mut width = String::new();
        while j < chars.len() && chars[j].is_ascii_digit() {
            width.push(chars[j]);
            j += 1;
        }
        let mut prec = None;
        if j < chars.len() && chars[j] == '.' {
            j += 1;
            let mut p = String::new();
            while j < chars.len() && chars[j].is_ascii_digit() {
                p.push(chars[j]);
                j += 1;
            }
            prec = Some(p.parse().unwrap_or(0));
        }
        // Skip length modifiers.
        while j < chars.len() && "lhL".contains(chars[j]) {
            j += 1;
        }
        if j >= chars.len() {
            lit.extend(&chars[i..]);
            break;
        }
        let conv = chars[j];
        if !"dioxXucsfeEgGb".contains(conv) {
            lit.extend(&chars[i..=j]);
            i = j + 1;
            continue;
        }
        if !lit.is_empty() {
            out.push(Piece::Lit(std::mem::take(&mut lit)));
        }
        out.push(Piece::Spec(Spec {
            flags,
            width: width.parse().ok(),
            prec,
            conv,
        }));
        i = j + 1;
    }
    if !lit.is_empty() {
        out.push(Piece::Lit(lit));
    }
    out
}

fn pad(s: String, spec: &Spec, numeric: bool) -> String {
    let Some(w) = spec.width else { return s };
    let len = s.chars().count();
    if len >= w {
        return s;
    }
    let fill = w - len;
    if spec.flags.contains('-') {
        format!("{}{}", s, " ".repeat(fill))
    } else if spec.flags.contains('0') && numeric {
        let (sign, digits) = if s.starts_with('-') || s.starts_with('+') {
            s.split_at(1)
        } else {
            ("", s.as_str())
        };
        format!("{}{}{}", sign, "0".repeat(fill), digits)
    } else {
        format!("{}{}", " ".repeat(fill), s)
    }
}

fn sign_prefix(x: f64, spec: &Spec, s: String) -> String {
    if x >= 0.0 && !s.starts_with('-') {
        if spec.flags.contains('+') {
            return format!("+{}", s);
        }
        if spec.flags.contains(' ') {
            return format!(" {}", s);
        }
    }
    s
}

fn fmt_exp(x: f64, prec: usize, upper: bool) -> String {
    let s = format!("{:.*e}", prec, x);
    let (m, e) = s.split_once('e').unwrap();
    let e: i32 = e.parse().unwrap();
    let r = format!("{}e{}{:02}", m, if e < 0 { '-' } else { '+' }, e.abs());
    if upper {
        r.to_uppercase()
    } else {
        r
    }
}

fn fmt_num(x: f64, spec: &Spec) -> String {
    if x.is_nan() || x.is_infinite() {
        let s = if x.is_nan() {
            "NaN".to_string()
        } else if x > 0.0 {
            "Inf".to_string()
        } else {
            "-Inf".to_string()
        };
        return pad(sign_prefix(x, spec, s), spec, false);
    }
    let s = match spec.conv {
        'd' | 'i' | 'u' => {
            if x.fract() != 0.0 {
                // Non-integer under %d behaves like %e.
                fmt_exp(x, spec.prec.unwrap_or(6), false)
            } else {
                format!("{}", x as i64)
            }
        }
        'f' => format!("{:.*}", spec.prec.unwrap_or(6), x),
        'e' | 'E' => fmt_exp(x, spec.prec.unwrap_or(6), spec.conv == 'E'),
        'g' | 'G' => {
            let s = fmt_g(x, spec.prec.unwrap_or(6).max(1), spec.flags.contains('#'));
            if spec.conv == 'G' {
                s.to_uppercase()
            } else {
                s
            }
        }
        'x' => format!("{:x}", x as i64),
        'X' => format!("{:X}", x as i64),
        'o' => format!("{:o}", x as i64),
        'b' => format!("{:b}", x as i64),
        'c' => char::from_u32(x as u32)
            .map(|c| c.to_string())
            .unwrap_or_default(),
        's' => {
            let s = if x.fract() == 0.0 {
                format!("{}", x as i64)
            } else {
                trim_g(x, spec.prec.unwrap_or(15).max(1))
            };
            return pad(s, spec, false);
        }
        _ => String::new(),
    };
    let numeric = spec.conv != 'c';
    pad(sign_prefix(x, spec, s), spec, numeric)
}

fn fmt_text(t: &str, spec: &Spec) -> String {
    match spec.conv {
        's' | 'c' => {
            let s: String = match spec.prec {
                Some(p) => t.chars().take(p).collect(),
                None => t.to_string(),
            };
            pad(s, spec, false)
        }
        // Text under a numeric conversion prints as text, like the reference behavior for strings.
        _ => pad(t.to_string(), spec, false),
    }
}

/// printf-style formatting with argument recycling.
pub fn format_printf(fmt: &str, args: &[Value]) -> MResult<String> {
    let fmt = unescape(fmt);
    let pieces = parse_format(&fmt);
    let flat = flatten_args(args)?;
    let nspecs = pieces
        .iter()
        .filter(|p| matches!(p, Piece::Spec(_)))
        .count();
    let mut out = String::new();
    if nspecs == 0 {
        out.push_str(&fmt);
        return Ok(out);
    }
    let mut k = 0;
    if flat.is_empty() {
        for p in &pieces {
            match p {
                Piece::Lit(s) => out.push_str(s),
                Piece::Spec(spec) => {
                    if matches!(spec.conv, 's' | 'c') {
                        out.push_str(&pad(String::new(), spec, false));
                    } else {
                        break;
                    }
                }
            }
        }
        return Ok(out);
    }
    loop {
        for p in &pieces {
            match p {
                Piece::Lit(s) => out.push_str(s),
                Piece::Spec(spec) => {
                    if k >= flat.len() {
                        return Ok(out);
                    }
                    let arg = flat[k].clone();
                    match (&arg, spec.conv) {
                        (
                            PArg::Text(t),
                            'd' | 'i' | 'u' | 'f' | 'e' | 'E' | 'g' | 'G' | 'x' | 'X' | 'o',
                        ) if t.chars().count() != 1 => {
                            out.push_str(&fmt_text(t, spec));
                        }
                        (
                            PArg::Text(t),
                            'd' | 'i' | 'u' | 'f' | 'e' | 'E' | 'g' | 'G' | 'x' | 'X' | 'o',
                        ) => {
                            let code = t.chars().next().unwrap() as u32 as f64;
                            out.push_str(&fmt_num(code, spec));
                        }
                        (PArg::Text(t), _) => out.push_str(&fmt_text(t, spec)),
                        (PArg::Num(x), _) => out.push_str(&fmt_num(*x, spec)),
                    }
                    k += 1;
                }
            }
        }
        // Each pass consumes at least one argument, so this terminates.
        if k >= flat.len() {
            break;
        }
    }
    Ok(out)
}

fn num2str(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 2)?;
    if a.len() == 2 && a[1].is_text() {
        let f = a[1].text()?;
        return one(Value::chars(&format_printf(&f, &a[..1])?));
    }
    let prec = if a.len() == 2 {
        Some(a[1].scalar()? as usize)
    } else {
        None
    };
    num2str_value(&a[0], prec)
}

fn num2str_value(v: &Value, prec: Option<usize>) -> R {
    match v {
        Value::Char(_) => one(v.clone()),
        Value::Str(_) => one(v.clone()),
        Value::Num(m, class) => {
            if m.is_empty() {
                return one(Value::chars(""));
            }
            let is_int = matches!(class, NumClass::Int(_) | NumClass::Logical)
                || m.data.iter().all(|x| !x.is_finite() || x.fract() == 0.0);
            let fmt1 = |x: f64| -> String {
                match prec {
                    Some(p) => trim_g(x, p),
                    None => {
                        if is_int {
                            num_to_str(x)
                        } else {
                            let digits = if x.abs() >= 1.0 {
                                x.abs().log10().floor() as usize + 1
                            } else {
                                1
                            };
                            trim_g(x, (digits + 4).max(5))
                        }
                    }
                }
            };
            if m.numel() == 1 {
                return one(Value::chars(&fmt1(m.data[0])));
            }
            let cells: Vec<String> = m.data.iter().map(|x| fmt1(*x)).collect();
            let w = cells.iter().map(|s| s.len()).max().unwrap_or(1);
            let gap = if is_int { 2 } else { 4 };
            let mut rows = Vec::new();
            for r in 0..m.rows {
                let mut line = String::new();
                for c in 0..m.cols {
                    let s = &cells[c * m.rows + r];
                    if c == 0 {
                        line.push_str(&format!("{:>w$}", s, w = w));
                    } else {
                        line.push_str(&format!("{:>w$}", s, w = w + gap));
                    }
                }
                rows.push(line);
            }
            // Remove common leading whitespace.
            let strip = rows
                .iter()
                .map(|l| l.len() - l.trim_start().len())
                .min()
                .unwrap_or(0);
            let rows: Vec<String> = rows.iter().map(|l| l[strip..].to_string()).collect();
            let width = rows.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            let mut ch = Array::filled(rows.len(), width, ' ');
            for (r, l) in rows.iter().enumerate() {
                for (c, chv) in l.chars().enumerate() {
                    ch.set(r, c, chv);
                }
            }
            one(Value::Char(ch))
        }
        other => Err(merr!(
            "Input to num2str must be numeric, not {}.",
            other.class_name()
        )),
    }
}

fn mat2str(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 2)?;
    let prec = if a.len() > 1 {
        a[1].scalar()? as usize
    } else {
        15
    };
    match &a[0] {
        Value::Char(_) => one(Value::chars(&format!("'{}'", a[0].text()?))),
        Value::Num(m, class) => {
            let f = |x: f64| -> String {
                if *class == NumClass::Logical {
                    return if x != 0.0 {
                        "true".into()
                    } else {
                        "false".into()
                    };
                }
                if x.is_infinite() {
                    return if x > 0.0 { "Inf".into() } else { "-Inf".into() };
                }
                trim_g(x, prec)
            };
            if m.numel() == 1 {
                return one(Value::chars(&f(m.data[0])));
            }
            let rows: Vec<String> = (0..m.rows)
                .map(|r| {
                    (0..m.cols)
                        .map(|c| f(*m.get(r, c)))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect();
            one(Value::chars(&format!("[{}]", rows.join(";"))))
        }
        other => Err(merr!(
            "Input must be numeric, logical or char, not {}.",
            other.class_name()
        )),
    }
}

fn char_fn(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return one(Value::chars(""));
    }
    let mut rows: Vec<String> = Vec::new();
    for v in &a {
        match v {
            Value::Num(m, _) if a.len() == 1 => {
                return one(Value::Char(
                    m.map(|x| char::from_u32(*x as u32).unwrap_or('\0')),
                ));
            }
            Value::Char(c) if a.len() == 1 => return one(Value::Char(c.clone())),
            Value::Cell(_) | Value::Str(_) => rows.extend(cellstr_list(v)?),
            Value::Num(m, _) => rows.push(
                m.data
                    .iter()
                    .map(|x| char::from_u32(*x as u32).unwrap_or('\0'))
                    .collect(),
            ),
            _ => rows.push(v.text()?),
        }
    }
    let w = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    let mut ch = Array::filled(rows.len(), w, ' ');
    for (r, s) in rows.iter().enumerate() {
        for (c, x) in s.chars().enumerate() {
            ch.set(r, c, x);
        }
    }
    one(Value::Char(ch))
}

fn strcat(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return Err(merr!("Not enough input arguments."));
    }
    if a.iter().any(|v| matches!(v, Value::Cell(_))) {
        let n = a
            .iter()
            .map(|v| if let Value::Cell(c) = v { c.numel() } else { 1 })
            .max()
            .unwrap_or(1);
        let mut out = vec![String::new(); n];
        let mut shape = (1, n);
        for v in &a {
            match v {
                Value::Cell(c) => {
                    shape = (c.rows, c.cols);
                    for (i, x) in c.data.iter().enumerate() {
                        out[i].push_str(&x.text()?);
                    }
                }
                other => {
                    let t = other.text()?;
                    for o in out.iter_mut() {
                        o.push_str(&t);
                    }
                }
            }
        }
        return one(Value::Cell(Array::new(
            shape.0,
            shape.1,
            out.iter().map(|s| Value::chars(s)).collect(),
        )));
    }
    if a.iter().any(|v| matches!(v, Value::Str(_))) {
        let mut s = String::new();
        for v in &a {
            s.push_str(&v.text()?);
        }
        return one(Value::string(&s));
    }
    // Char arrays: trailing whitespace of each input is removed.
    let mut s = String::new();
    for v in &a {
        s.push_str(v.text()?.trim_end_matches([' ', '\t', '\n', '\0']));
    }
    one(Value::chars(&s))
}

fn compare(a: &[Value], f: fn(&str, &str) -> bool) -> R {
    check_args(a, 2, 2)?;
    let as_list = |v: &Value| -> Option<(Vec<Option<String>>, (usize, usize))> {
        match v {
            Value::Cell(c) => Some((
                c.data
                    .iter()
                    .map(|x| if x.is_text() { x.text().ok() } else { None })
                    .collect(),
                (c.rows, c.cols),
            )),
            Value::Str(s) if s.numel() != 1 => Some((
                s.data.iter().map(|x| Some(x.clone())).collect(),
                (s.rows, s.cols),
            )),
            _ => None,
        }
    };
    let single = |v: &Value| -> Option<String> {
        if v.is_text() {
            v.text().ok()
        } else {
            None
        }
    };
    match (as_list(&a[0]), as_list(&a[1])) {
        (None, None) => {
            let r = match (single(&a[0]), single(&a[1])) {
                (Some(x), Some(y)) => f(&x, &y),
                _ => false,
            };
            one(bool_val(r))
        }
        (Some((l, shape)), None) | (None, Some((l, shape))) => {
            let other = if as_list(&a[0]).is_some() {
                single(&a[1])
            } else {
                single(&a[0])
            };
            let data = l
                .iter()
                .map(|x| match (x, &other) {
                    (Some(x), Some(y)) if f(x, y) => 1.0,
                    _ => 0.0,
                })
                .collect();
            one(Value::logical(Array::new(shape.0, shape.1, data)))
        }
        (Some((l1, shape)), Some((l2, _))) => {
            if l1.len() != l2.len() {
                return Err(merr!(
                    "Inputs must be the same size or either one can be a scalar."
                ));
            }
            let data = l1
                .iter()
                .zip(&l2)
                .map(|(x, y)| match (x, y) {
                    (Some(x), Some(y)) if f(x, y) => 1.0,
                    _ => 0.0,
                })
                .collect();
            one(Value::logical(Array::new(shape.0, shape.1, data)))
        }
    }
}

fn compare_n(a: &[Value], n: usize, ci: bool) -> R {
    let norm = |s: String| -> String {
        let t: String = s.chars().take(n).collect();
        if ci {
            t.to_lowercase()
        } else {
            t
        }
    };
    let ok_len = |s: &str| s.chars().count() >= n;
    match (&a[0], &a[1]) {
        (Value::Cell(c), other) | (other, Value::Cell(c)) => {
            let o = other.text()?;
            let data = c
                .data
                .iter()
                .map(|x| {
                    let t = x.text().unwrap_or_default();
                    if ok_len(&t) && ok_len(&o) && norm(t.clone()) == norm(o.clone()) {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect();
            one(Value::logical(Array::new(c.rows, c.cols, data)))
        }
        _ => {
            let x = a[0].text()?;
            let y = a[1].text()?;
            one(bool_val(ok_len(&x) && ok_len(&y) && norm(x) == norm(y)))
        }
    }
}

fn map_text(v: &Value, f: impl Fn(&str) -> MResult<Value>) -> R {
    match v {
        Value::Cell(c) => {
            let mut out = Vec::new();
            for x in &c.data {
                out.push(f(&x.text()?)?);
            }
            one(Value::Cell(Array::new(c.rows, c.cols, out)))
        }
        _ => one(f(&v.text()?)?),
    }
}

fn map_text_keep(v: &Value, f: impl Fn(&str) -> String) -> R {
    match v {
        Value::Cell(c) => {
            let mut out = Vec::new();
            for x in &c.data {
                out.push(Value::chars(&f(&x.text()?)));
            }
            one(Value::Cell(Array::new(c.rows, c.cols, out)))
        }
        Value::Str(s) => one(Value::Str(s.map(|x| f(x)))),
        Value::Char(c) if c.rows > 1 => {
            let mut out = c.clone();
            for r in 0..c.rows {
                let row: String = (0..c.cols).map(|k| *c.get(r, k)).collect();
                let nr: Vec<char> = f(&row).chars().collect();
                for k in 0..c.cols {
                    out.set(r, k, *nr.get(k).unwrap_or(&' '));
                }
            }
            one(Value::Char(out))
        }
        _ => one(Value::chars(&f(&v.text()?))),
    }
}

fn text_pred(a: &[Value], f: fn(&str, &str) -> bool) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let pats = cellstr_list(&a[1])?;
    let ci = a.len() >= 4 && a[2].text()?.eq_ignore_ascii_case("IgnoreCase") && a[3].is_true()?;
    let test = |s: &str| -> bool {
        pats.iter().any(|p| {
            if ci {
                f(&s.to_lowercase(), &p.to_lowercase())
            } else {
                f(s, p)
            }
        })
    };
    match &a[0] {
        Value::Cell(c) => one(Value::logical(Array::new(
            c.rows,
            c.cols,
            c.data
                .iter()
                .map(|x| {
                    if test(&x.text().unwrap_or_default()) {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect(),
        ))),
        Value::Str(s) => one(Value::logical(s.map(|x| if test(x) { 1.0 } else { 0.0 }))),
        v => one(bool_val(test(&v.text()?))),
    }
}

fn char_pred(a: &[Value], f: fn(char) -> bool) -> R {
    check_args(a, 1, 1)?;
    match &a[0] {
        Value::Char(c) => one(Value::logical(c.map(|x| if f(*x) { 1.0 } else { 0.0 }))),
        Value::Str(s) if s.numel() == 1 => {
            let v: Vec<f64> = s.data[0]
                .chars()
                .map(|x| if f(x) { 1.0 } else { 0.0 })
                .collect();
            one(Value::logical(Array::row(v)))
        }
        v => one(Value::logical(v.to_num()?.map(|_| 0.0))),
    }
}

fn strsplit(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    check_args(&a, 1, 4)?;
    let s = a[0].text()?;
    let delims: Vec<String> =
        if a.len() > 1 && !(a[1].is_text() && a[1].text()? == "CollapseDelimiters") {
            cellstr_list(&a[1])?.iter().map(|d| unescape(d)).collect()
        } else {
            vec![" ".into(), "\t".into(), "\n".into()]
        };
    let mut collapse = true;
    let mut i = if a.len() > 1 && !(a[1].text().is_ok_and(|t| t == "CollapseDelimiters")) {
        2
    } else {
        1
    };
    while i + 1 < a.len() {
        if a[i].text()?.eq_ignore_ascii_case("CollapseDelimiters") {
            collapse = a[i + 1].is_true()?;
        }
        i += 2;
    }
    let mut parts = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut k = 0;
    let mut last_was_delim = false;
    while k < chars.len() {
        let rest: String = chars[k..].iter().collect();
        if let Some(d) = delims
            .iter()
            .filter(|d| !d.is_empty() && rest.starts_with(d.as_str()))
            .max_by_key(|d| d.len())
        {
            if !(collapse && last_was_delim) {
                parts.push(std::mem::take(&mut cur));
            }
            k += d.chars().count();
            last_was_delim = true;
            continue;
        }
        cur.push(chars[k]);
        last_was_delim = false;
        k += 1;
    }
    parts.push(cur);
    one(Value::Cell(Array::row(
        parts.iter().map(|p| Value::chars(p)).collect(),
    )))
}

fn build_regex(pat: &str, ci: bool) -> MResult<Regex> {
    // Translate a few common differences: (?<name>...) is accepted by the regex crate as-is.
    RegexBuilder::new(pat)
        .case_insensitive(ci)
        .build()
        .map_err(|e| merr!("Invalid regular expression: {}", e))
}

fn regexp(a: &[Value], nargout: usize, ci_default: bool) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let s = a[0].text()?;
    let pat = a[1].text()?;
    let mut ci = ci_default;
    let mut once = false;
    let mut selectors: Vec<String> = Vec::new();
    for v in &a[2..] {
        let t = v.text()?;
        match t.as_str() {
            "once" => once = true,
            "ignorecase" => ci = true,
            "matchcase" => ci = false,
            "match" | "tokens" | "names" | "start" | "end" | "split" | "tokenExtents" => {
                selectors.push(t)
            }
            _ => {}
        }
    }
    if selectors.is_empty() {
        selectors = vec![
            "start".into(),
            "end".into(),
            "tokenExtents".into(),
            "match".into(),
            "tokens".into(),
            "names".into(),
            "split".into(),
        ];
    }
    let re = build_regex(&pat, ci)?;
    let byte_to_char = |b: usize| s[..b].chars().count();
    let caps: Vec<regex::Captures> = if once {
        re.captures(&s).into_iter().collect()
    } else {
        re.captures_iter(&s).collect()
    };
    let mut outs = Vec::new();
    for sel in selectors.iter().take(nargout.max(1)) {
        let v = match sel.as_str() {
            "match" => {
                let m: Vec<Value> = caps
                    .iter()
                    .map(|c| Value::chars(c.get(0).unwrap().as_str()))
                    .collect();
                if once {
                    m.into_iter().next().unwrap_or_else(|| Value::chars(""))
                } else {
                    Value::cell_row(m)
                }
            }
            "tokens" => {
                let t: Vec<Value> = caps
                    .iter()
                    .map(|c| {
                        let toks: Vec<Value> = if c.len() > 1 {
                            (1..c.len())
                                .map(|i| Value::chars(c.get(i).map_or("", |m| m.as_str())))
                                .collect()
                        } else {
                            vec![Value::chars(c.get(0).unwrap().as_str())]
                        };
                        Value::cell_row(toks)
                    })
                    .collect();
                if once {
                    t.into_iter()
                        .next()
                        .unwrap_or_else(|| Value::Cell(Array::empty()))
                } else {
                    Value::cell_row(t)
                }
            }
            "names" => {
                let names: Vec<&str> = re.capture_names().flatten().collect();
                let mk = |c: &regex::Captures| -> Value {
                    Value::Struct(StructArr::scalar(
                        names
                            .iter()
                            .map(|n| {
                                (
                                    n.to_string(),
                                    Value::chars(c.name(n).map_or("", |m| m.as_str())),
                                )
                            })
                            .collect(),
                    ))
                };
                match caps.first() {
                    Some(c) if once || caps.len() == 1 => mk(c),
                    Some(_) => {
                        let elems: Vec<Vec<Value>> = caps
                            .iter()
                            .map(|c| {
                                names
                                    .iter()
                                    .map(|n| Value::chars(c.name(n).map_or("", |m| m.as_str())))
                                    .collect()
                            })
                            .collect();
                        Value::Struct(StructArr {
                            keys: names.iter().map(|s| s.to_string()).collect(),
                            arr: Array::row(elems),
                        })
                    }
                    None => Value::Struct(StructArr::scalar(
                        names
                            .iter()
                            .map(|n| (n.to_string(), Value::chars("")))
                            .collect(),
                    )),
                }
            }
            "start" => {
                let v: Vec<f64> = caps
                    .iter()
                    .map(|c| (byte_to_char(c.get(0).unwrap().start()) + 1) as f64)
                    .collect();
                if once {
                    v.first().map_or(Value::empty(), |x| Value::num(*x))
                } else if v.is_empty() {
                    Value::Num(Array::new(1, 0, vec![]), NumClass::Double)
                } else {
                    Value::row(v)
                }
            }
            "end" => {
                let v: Vec<f64> = caps
                    .iter()
                    .map(|c| byte_to_char(c.get(0).unwrap().end()) as f64)
                    .collect();
                if once {
                    v.first().map_or(Value::empty(), |x| Value::num(*x))
                } else if v.is_empty() {
                    Value::Num(Array::new(1, 0, vec![]), NumClass::Double)
                } else {
                    Value::row(v)
                }
            }
            "split" => {
                let parts: Vec<Value> = re.split(&s).map(Value::chars).collect();
                Value::cell_row(parts)
            }
            _ => Value::Cell(Array::empty()),
        };
        outs.push(v);
    }
    Ok(outs)
}

fn regexprep(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.len() < 3 {
        return Err(merr!("Not enough input arguments."));
    }
    let pat = a[1].text()?;
    let rep = a[2].text()?;
    let mut ci = false;
    let mut once = false;
    for v in &a[3..] {
        match v.text()?.as_str() {
            "ignorecase" => ci = true,
            "once" => once = true,
            _ => {}
        }
    }
    let re = build_regex(&pat, ci)?;
    // $1 tokens map directly; escape sequences in the replacement are expanded.
    let rep = unescape(&rep).replace("$0", "${0}");
    let rep: String = {
        let r = Regex::new(r"\$(\d+)").unwrap();
        r.replace_all(&rep, "$${${1}}").to_string()
    };
    let apply = |s: &str| -> String {
        if once {
            re.replacen(s, 1, rep.as_str()).to_string()
        } else {
            re.replace_all(s, rep.as_str()).to_string()
        }
    };
    map_text_keep(&a[0], apply)
}
