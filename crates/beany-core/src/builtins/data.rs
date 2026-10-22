//! Cells, structs, function handles and the *fun family.

use super::*;
use crate::ops;

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("cell", |_, a, _| {
        let (r, c, _) = size_args(&a)?;
        one(Value::Cell(Array::filled(r, c, Value::empty())))
    });
    b.insert("struct", struct_fn);
    b.insert("fieldnames", |_, a, _| {
        check_args(&a, 1, 2)?;
        if let Value::Inst(o) = &a[0] {
            let names: Vec<Value> = o
                .class
                .props
                .iter()
                .map(|p| Value::chars(&p.name))
                .collect();
            return one(Value::Cell(Array::new(names.len(), 1, names)));
        }
        let Value::Struct(s) = &a[0] else {
            return Err(merr!(
                "Invalid input argument of type '{}'. Input must be a structure or an object.",
                a[0].class_name()
            ));
        };
        let n = s.keys.len();
        one(Value::Cell(Array::new(
            n,
            1,
            s.keys.iter().map(|k| Value::chars(k)).collect(),
        )))
    });
    b.insert("isfield", |_, a, _| {
        check_args(&a, 2, 2)?;
        let Value::Struct(s) = &a[0] else {
            return one(bool_val(false));
        };
        match &a[1] {
            Value::Cell(c) => one(Value::logical(c.map(|v| {
                if v.text().is_ok_and(|t| s.key_index(&t).is_some()) {
                    1.0
                } else {
                    0.0
                }
            }))),
            v => one(bool_val(s.key_index(&v.text()?).is_some())),
        }
    });
    b.insert("rmfield", |_, a, _| {
        check_args(&a, 2, 2)?;
        let Value::Struct(s) = &a[0] else {
            return Err(merr!("First argument must be a structure."));
        };
        let mut s = s.clone();
        for name in cellstr_list(&a[1])? {
            let Some(k) = s.key_index(&name) else {
                return Err(merr!("A field named '{}' doesn't exist.", name));
            };
            s.keys.remove(k);
            for e in s.arr.data.iter_mut() {
                e.remove(k);
            }
        }
        one(Value::Struct(s))
    });
    b.insert("setfield", |_, a, _| {
        if a.len() < 3 {
            return Err(merr!("Not enough input arguments."));
        }
        let n = a.len();
        one(set_path(
            Some(a[0].clone()),
            &a[1..n - 1],
            a[n - 1].clone(),
        )?)
    });
    b.insert("getfield", |_, a, _| {
        if a.len() < 2 {
            return Err(merr!("Not enough input arguments."));
        }
        let mut cur = a[0].clone();
        for p in &a[1..] {
            cur = match p {
                Value::Cell(c) => cur.index(&cell_idx(c)?)?,
                p => {
                    let name = p.text()?;
                    let Value::Struct(s) = &cur else {
                        return Err(merr!("First argument must be a structure."));
                    };
                    s.get(0, &name)
                        .cloned()
                        .ok_or_else(|| merr!("Unrecognized field name \"{}\".", name))?
                }
            };
        }
        one(cur)
    });
    b.insert("orderfields", |_, a, _| {
        check_args(&a, 1, 1)?;
        let Value::Struct(s) = &a[0] else {
            return Err(merr!("First argument must be a structure."));
        };
        let mut keys = s.keys.clone();
        keys.sort();
        let arr = s.arr.map(|e| {
            keys.iter()
                .map(|k| e[s.key_index(k).unwrap()].clone())
                .collect::<Vec<Value>>()
        });
        one(Value::Struct(StructArr { keys, arr }))
    });
    b.insert("struct2cell", |_, a, _| {
        check_args(&a, 1, 1)?;
        let Value::Struct(s) = &a[0] else {
            return Err(merr!("Input must be a structure."));
        };
        let vals = s.arr.data.first().cloned().unwrap_or_default();
        let n = vals.len();
        one(Value::Cell(Array::new(n, 1, vals)))
    });
    b.insert("cell2struct", |_, a, _| {
        check_args(&a, 2, 3)?;
        let Value::Cell(c) = &a[0] else {
            return Err(merr!("First input must be a cell array."));
        };
        let names = cellstr_list(&a[1])?;
        if names.len() != c.numel() {
            return Err(merr!(
                "Number of field names must match number of fields in new structure."
            ));
        }
        one(Value::Struct(StructArr::scalar(
            names.into_iter().zip(c.data.iter().cloned()).collect(),
        )))
    });
    b.insert("num2cell", |_, a, _| {
        check_args(&a, 1, 1)?;
        let (r, c) = a[0].dims();
        one(Value::Cell(Array::new(r, c, a[0].elements())))
    });
    b.insert("cell2mat", |_, a, _| {
        check_args(&a, 1, 1)?;
        let Value::Cell(c) = &a[0] else {
            return Err(merr!("Input must be a cell array."));
        };
        if c.is_empty() {
            return one(Value::empty());
        }
        let mut rows = Vec::new();
        for r in 0..c.rows {
            let items: Vec<Value> = (0..c.cols).map(|k| c.get(r, k).clone()).collect();
            rows.push(ops::horzcat(items)?);
        }
        one(ops::vertcat(rows)?)
    });
    b.insert("cellfun", |it, a, n| funcall_map(it, a, n, true));
    b.insert("arrayfun", |it, a, n| funcall_map(it, a, n, false));
    b.insert("structfun", |it, a, n| {
        if a.len() < 2 {
            return Err(merr!("Not enough input arguments."));
        }
        let Value::Struct(s) = &a[1] else {
            return Err(merr!("Input must be a scalar structure."));
        };
        let vals: Vec<Value> = s.arr.data.first().cloned().unwrap_or_default();
        let len = vals.len();
        let mut args = vec![a[0].clone(), Value::Cell(Array::new(len, 1, vals))];
        args.extend_from_slice(&a[2..]);
        funcall_map(it, args, n, true)
    });
    b.insert("func2str", |_, a, _| {
        check_args(&a, 1, 1)?;
        let Value::Func(f) = &a[0] else {
            return Err(merr!("Input must be a function handle."));
        };
        one(Value::chars(&match f {
            FuncHandle::Named(n) => n.clone(),
            FuncHandle::Anon { text, .. } => text.clone(),
        }))
    });
    b.insert("str2func", |it, a, _| {
        check_args(&a, 1, 1)?;
        let t = a[0].text()?;
        if t.starts_with('@') {
            let e = crate::parser::parse_expression(&t)?;
            return one(it.eval(&e)?);
        }
        one(Value::Func(FuncHandle::Named(t)))
    });
    b.insert("feval", |it, a, n| {
        if a.is_empty() {
            return Err(merr!("Not enough input arguments."));
        }
        it.call_value(&a[0], a[1..].to_vec(), n)
    });
    b.insert("deal", |_, a, n| {
        if a.len() == 1 {
            return Ok(vec![a[0].clone(); n.max(1)]);
        }
        if a.len() < n {
            return Err(merr!(
                "The number of outputs should match the number of inputs."
            ));
        }
        Ok(a)
    });
    b.insert("isvarname", |_, a, _| {
        check_args(&a, 1, 1)?;
        let s = a[0].text().unwrap_or_default();
        let ok = s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && s.len() <= 63;
        one(bool_val(ok))
    });
    b.insert("matlab.lang.makeValidName", |_, a, _| {
        check_args(&a, 1, 1)?;
        let s = a[0].text()?;
        let mut out: String = s
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if !out.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            out = format!("x{}", out);
        }
        one(Value::chars(&out))
    });
}

fn struct_fn(_: &mut Interp, a: Vec<Value>, _: usize) -> R {
    if a.is_empty() {
        return one(Value::Struct(StructArr::scalar(vec![])));
    }
    match &a[0] {
        Value::Inst(o) if a.len() == 1 => return one(Value::Struct(o.props())),
        Value::Struct(_) if a.len() == 1 => return one(a[0].clone()),
        _ => {}
    }
    if !a.len().is_multiple_of(2) {
        return Err(merr!("Field and value input arguments must come in pairs."));
    }
    // A cell value creates a struct array with one element per cell.
    let mut n = 1;
    let mut shape = (1, 1);
    for pair in a.chunks(2) {
        if let Value::Cell(c) = &pair[1] {
            if c.numel() != 1 {
                n = c.numel();
                shape = (c.rows, c.cols);
            }
        }
    }
    let keys: Vec<String> = a.chunks(2).map(|p| p[0].text()).collect::<MResult<_>>()?;
    let mut elems = Vec::with_capacity(n);
    for i in 0..n {
        let mut e = Vec::with_capacity(keys.len());
        for pair in a.chunks(2) {
            e.push(match &pair[1] {
                Value::Cell(c) if c.numel() == 1 => c.data[0].clone(),
                Value::Cell(c) if c.numel() == 0 => Value::empty(),
                Value::Cell(c) => c.data[i].clone(),
                v => v.clone(),
            });
        }
        elems.push(e);
    }
    if a.chunks(2)
        .any(|p| matches!(&p[1], Value::Cell(c) if c.numel() == 0))
    {
        return one(Value::Struct(StructArr {
            keys,
            arr: Array::new(0, 0, vec![]),
        }));
    }
    one(Value::Struct(StructArr {
        keys,
        arr: Array::new(shape.0, shape.1, elems),
    }))
}

fn funcall_map(it: &mut Interp, a: Vec<Value>, nargout: usize, cells: bool) -> R {
    if a.len() < 2 {
        return Err(merr!("Not enough input arguments."));
    }
    let f = a[0].clone();
    let mut inputs = Vec::new();
    let mut uniform = true;
    let mut error_handler: Option<Value> = None;
    let mut i = 1;
    while i < a.len() {
        if a[i].is_text() && i + 1 < a.len() && !inputs.is_empty() {
            match a[i].text()?.as_str() {
                "UniformOutput" => uniform = a[i + 1].is_true()?,
                "ErrorHandler" => error_handler = Some(a[i + 1].clone()),
                other => return Err(merr!("Unknown parameter '{}'", other)),
            }
            i += 2;
            continue;
        }
        inputs.push(a[i].clone());
        i += 1;
    }
    // Legacy string function names for cellfun.
    if cells && f.is_text() && inputs.len() == 1 {
        let name = f.text()?;
        if let Value::Cell(c) = &inputs[0] {
            let r: Option<Vec<f64>> = match name.as_str() {
                "isempty" => Some(
                    c.data
                        .iter()
                        .map(|v| if v.is_empty() { 1.0 } else { 0.0 })
                        .collect(),
                ),
                "length" => Some(
                    c.data
                        .iter()
                        .map(|v| {
                            let (r, k) = v.dims();
                            if r == 0 || k == 0 {
                                0.0
                            } else {
                                r.max(k) as f64
                            }
                        })
                        .collect(),
                ),
                "isclass" => None,
                _ => None,
            };
            if let Some(r) = r {
                let arr = Array::new(c.rows, c.cols, r);
                return one(if name == "length" {
                    Value::mat(arr)
                } else {
                    Value::logical(arr)
                });
            }
        }
    }
    let (rows, cols) = inputs[0].dims();
    let n = rows * cols;
    for x in &inputs[1..] {
        if x.numel() != n {
            return Err(merr!(
                "All of the input arguments must be of the same size and shape."
            ));
        }
    }
    let elems: Vec<Vec<Value>> = inputs
        .iter()
        .map(|x| match (cells, x) {
            (true, Value::Cell(c)) => Ok(c.data.clone()),
            (true, other) => Err(merr!(
                "Input #2 expected to be a cell array, was {} instead.",
                other.class_name()
            )),
            (false, other) => Ok(other.elements()),
        })
        .collect::<MResult<_>>()?;
    let nout = nargout.max(1);
    let mut results: Vec<Vec<Value>> = vec![Vec::with_capacity(n); nout];
    for k in 0..n {
        let args: Vec<Value> = elems.iter().map(|e| e[k].clone()).collect();
        let r = match it.call_value(
            &f,
            args.clone(),
            if nargout == 0 && !uniform { 1 } else { nout },
        ) {
            Ok(r) => r,
            Err(e) => match &error_handler {
                Some(h) => {
                    let mut hargs = vec![crate::interp::exception_value(&e)];
                    hargs.extend(args);
                    it.call_value(h, hargs, nout)?
                }
                None => return Err(e),
            },
        };
        if r.len() < nout && (nargout > 0 || uniform) {
            if nargout == 0 && r.is_empty() {
                continue;
            }
            return Err(merr!("The function returned fewer outputs than requested."));
        }
        for (j, v) in r.into_iter().take(nout).enumerate() {
            results[j].push(v);
        }
    }
    let mut out = Vec::new();
    for vals in results {
        if vals.is_empty() && n > 0 {
            continue;
        }
        if uniform {
            let mut data = Vec::with_capacity(n);
            let mut class = NumClass::Double;
            let mut is_char = false;
            for v in &vals {
                match v {
                    Value::Num(a, c) if a.numel() == 1 => {
                        class = *c;
                        data.push(a.data[0]);
                    }
                    Value::Char(a) if a.numel() == 1 => {
                        is_char = true;
                        data.push(a.data[0] as u32 as f64);
                    }
                    _ => return Err(merr!("Non-scalar in Uniform output, at index {}, output 1. Set 'UniformOutput' to false.", data.len() + 1)),
                }
            }
            let arr = Array::new(rows, cols, data);
            out.push(if is_char {
                Value::Char(arr.map(|x| char::from_u32(*x as u32).unwrap_or('\0')))
            } else {
                Value::Num(arr, class)
            });
        } else {
            out.push(Value::Cell(Array::new(rows, cols, vals)));
        }
    }
    Ok(out)
}

/// Subscripts given as a cell, like the `{i, j}` arguments of getfield/setfield.
fn cell_idx(c: &Array<Value>) -> MResult<Vec<Idx>> {
    c.data
        .iter()
        .map(|v| {
            if v.is_text() && v.text()? == ":" {
                return Ok(Idx::All);
            }
            let m = v.to_num()?;
            Ok(Idx::List(
                m.data
                    .iter()
                    .map(|x| (*x as usize).saturating_sub(1))
                    .collect(),
                (m.rows, m.cols),
            ))
        })
        .collect()
}

/// Nested `setfield(s, 'a', {2}, 'b', v)`.
fn set_path(cur: Option<Value>, path: &[Value], v: Value) -> MResult<Value> {
    let Some(p) = path.first() else {
        return Ok(v);
    };
    if let Value::Cell(c) = p {
        let idx = cell_idx(c)?;
        let base = cur.unwrap_or_else(|| Value::Struct(StructArr::scalar(vec![])));
        let Value::Struct(mut s) = base else {
            return Err(merr!("Indexed setfield needs a structure array."));
        };
        let (rows, cols) = (s.arr.rows.max(1), s.arr.cols.max(1));
        let k = match idx.as_slice() {
            [Idx::List(l, _)] if l.len() == 1 => l[0],
            [Idx::List(r, _), Idx::List(c, _)] if r.len() == 1 && c.len() == 1 => {
                c[0] * rows + r[0]
            }
            _ => return Err(merr!("setfield supports a single element index.")),
        };
        if s.arr.is_empty() {
            s.arr = Array::scalar(s.empty_elem());
        }
        while s.arr.numel() <= k {
            let e = s.empty_elem();
            s.arr.data.push(e);
            s.arr.rows = 1;
            s.arr.cols = s.arr.data.len();
        }
        let _ = cols;
        let mut one_elem = s.clone();
        one_elem.arr = Array::scalar(s.arr.data[k].clone());
        let Value::Struct(ns) = set_path(Some(Value::Struct(one_elem)), &path[1..], v)? else {
            return Err(merr!("Invalid structure assignment."));
        };
        for key in &ns.keys {
            s.add_key(key);
        }
        s.arr.data[k] = s
            .keys
            .iter()
            .map(|key| ns.get(0, key).cloned().unwrap_or_else(Value::empty))
            .collect();
        return Ok(Value::Struct(s));
    }
    let name = p.text()?;
    let mut s = match cur {
        Some(Value::Struct(s)) => s,
        Some(v) if !v.is_empty() => return Err(merr!("First argument must be a structure.")),
        _ => StructArr::scalar(vec![]),
    };
    let k = s.add_key(&name);
    if s.arr.is_empty() {
        s.arr = Array::scalar(s.empty_elem());
    }
    let child = s.arr.data[0][k].clone();
    let child = if child.is_empty() && !matches!(child, Value::Struct(_)) {
        None
    } else {
        Some(child)
    };
    s.arr.data[0][k] = set_path(child, &path[1..], v)?;
    Ok(Value::Struct(s))
}
