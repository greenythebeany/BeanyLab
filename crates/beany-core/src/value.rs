//! Runtime values. All arrays are 2-D and stored column-major.

use std::collections::HashMap;
use std::sync::Arc;

use crate::ast::Expr;
use crate::error::{MError, MResult};
use crate::merr;

/// Complex double.
pub type C64 = nalgebra::Complex<f64>;

#[derive(Debug, Clone, PartialEq)]
pub struct Array<T> {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<T>,
}

impl<T: Clone> Array<T> {
    pub fn new(rows: usize, cols: usize, data: Vec<T>) -> Self {
        debug_assert_eq!(rows * cols, data.len());
        Array { rows, cols, data }
    }
    pub fn filled(rows: usize, cols: usize, v: T) -> Self {
        Array {
            rows,
            cols,
            data: vec![v; rows * cols],
        }
    }
    pub fn scalar(v: T) -> Self {
        Array {
            rows: 1,
            cols: 1,
            data: vec![v],
        }
    }
    pub fn row(v: Vec<T>) -> Self {
        Array {
            rows: 1,
            cols: v.len(),
            data: v,
        }
    }
    pub fn col(v: Vec<T>) -> Self {
        Array {
            rows: v.len(),
            cols: 1,
            data: v,
        }
    }
    pub fn empty() -> Self {
        Array {
            rows: 0,
            cols: 0,
            data: Vec::new(),
        }
    }
    pub fn numel(&self) -> usize {
        self.data.len()
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    pub fn is_scalar(&self) -> bool {
        self.data.len() == 1
    }
    pub fn is_vector(&self) -> bool {
        self.rows == 1 || self.cols == 1
    }
    pub fn get(&self, r: usize, c: usize) -> &T {
        &self.data[c * self.rows + r]
    }
    pub fn set(&mut self, r: usize, c: usize, v: T) {
        let rows = self.rows;
        self.data[c * rows + r] = v;
    }
    pub fn map<U: Clone>(&self, f: impl Fn(&T) -> U) -> Array<U> {
        Array {
            rows: self.rows,
            cols: self.cols,
            data: self.data.iter().map(f).collect(),
        }
    }
    pub fn transpose(&self) -> Self {
        let mut data = Vec::with_capacity(self.data.len());
        for r in 0..self.rows {
            for c in 0..self.cols {
                data.push(self.get(r, c).clone());
            }
        }
        Array {
            rows: self.cols,
            cols: self.rows,
            data,
        }
    }
    pub fn reshape(&self, rows: usize, cols: usize) -> MResult<Self> {
        if rows * cols != self.data.len() {
            return Err(merr!("Number of elements must not change. Use [] as one of the size inputs to automatically calculate the appropriate size for that dimension."));
        }
        Ok(Array {
            rows,
            cols,
            data: self.data.clone(),
        })
    }

    /// Index with resolved 0-based index lists.
    pub fn index(&self, idx: &[Idx]) -> MResult<Self> {
        match idx.len() {
            0 => Ok(self.clone()),
            1 => {
                let n = self.numel();
                let (list, shape) = idx[0].resolve(n);
                for &k in &list {
                    if k >= n {
                        return Err(index_oob(k + 1, n));
                    }
                }
                let data: Vec<T> = list.iter().map(|&k| self.data[k].clone()).collect();
                let len = data.len();
                let (r, c) = match &idx[0] {
                    Idx::All => (len, 1),
                    _ => {
                        let (ir, ic) = shape;
                        let idx_is_vec = ir == 1 || ic == 1;
                        if self.is_vector() && idx_is_vec && !(self.rows == 1 && self.cols == 1) {
                            if self.rows == 1 {
                                (1, len)
                            } else {
                                (len, 1)
                            }
                        } else {
                            (ir, ic)
                        }
                    }
                };
                Ok(Array {
                    rows: r,
                    cols: c,
                    data,
                })
            }
            _ => {
                let rows_idx = idx[0].resolve(self.rows).0;
                // Trailing dimensions collapse: A(i, j, 1) is allowed.
                let cols_n = self.cols;
                let cols_idx = idx[1].resolve(cols_n).0;
                for extra in &idx[2..] {
                    let (l, _) = extra.resolve(1);
                    if l.iter().any(|&k| k != 0) {
                        return Err(merr!("Index in position 3 exceeds array bounds."));
                    }
                }
                for &r in &rows_idx {
                    if r >= self.rows {
                        return Err(merr!(
                            "Index in position 1 exceeds array bounds. Index must not exceed {}.",
                            self.rows
                        ));
                    }
                }
                for &c in &cols_idx {
                    if c >= self.cols {
                        return Err(merr!(
                            "Index in position 2 exceeds array bounds. Index must not exceed {}.",
                            self.cols
                        ));
                    }
                }
                let mut data = Vec::with_capacity(rows_idx.len() * cols_idx.len());
                for &c in &cols_idx {
                    for &r in &rows_idx {
                        data.push(self.get(r, c).clone());
                    }
                }
                Ok(Array {
                    rows: rows_idx.len(),
                    cols: cols_idx.len(),
                    data,
                })
            }
        }
    }

    /// Indexed assignment; grows the array with `fill` when needed.
    pub fn assign(&mut self, idx: &[Idx], src: &Array<T>, fill: &T) -> MResult<()> {
        match idx.len() {
            0 => Err(merr!("Invalid indexing")),
            1 => {
                let n = self.numel();
                let (list, _) = idx[0].resolve(n);
                if !(src.numel() == 1 || src.numel() == list.len()) {
                    return Err(merr!("Unable to perform assignment because the left and right sides have a different number of elements."));
                }
                let max = list.iter().copied().max().map(|m| m + 1).unwrap_or(0);
                if max > n {
                    // Empty arrays and row vectors grow as rows.
                    if self.rows == 1 || (self.rows == 0 && self.cols == 0) {
                        self.resize(1, max, fill);
                    } else if self.cols == 1 {
                        self.resize(max, 1, fill);
                    } else {
                        return Err(merr!("Attempt to grow array along ambiguous dimension."));
                    }
                }
                for (i, &k) in list.iter().enumerate() {
                    let v = if src.numel() == 1 {
                        src.data[0].clone()
                    } else {
                        src.data[i].clone()
                    };
                    self.data[k] = v;
                }
                Ok(())
            }
            _ => {
                let rows_idx = idx[0].resolve(self.rows).0;
                let cols_idx = idx[1].resolve(self.cols).0;
                let count = rows_idx.len() * cols_idx.len();
                if !(src.numel() == 1 || src.numel() == count) {
                    return Err(merr!(
                        "Unable to perform assignment because the size of the left side is {}-by-{} and the size of the right side is {}-by-{}.",
                        rows_idx.len(),
                        cols_idx.len(),
                        src.rows,
                        src.cols
                    ));
                }
                let need_r = rows_idx
                    .iter()
                    .copied()
                    .max()
                    .map(|m| m + 1)
                    .unwrap_or(0)
                    .max(self.rows);
                let need_c = cols_idx
                    .iter()
                    .copied()
                    .max()
                    .map(|m| m + 1)
                    .unwrap_or(0)
                    .max(self.cols);
                if need_r > self.rows || need_c > self.cols {
                    self.resize(need_r, need_c, fill);
                }
                let mut i = 0;
                for &c in &cols_idx {
                    for &r in &rows_idx {
                        let v = if src.numel() == 1 {
                            src.data[0].clone()
                        } else {
                            src.data[i].clone()
                        };
                        self.set(r, c, v);
                        i += 1;
                    }
                }
                Ok(())
            }
        }
    }

    pub fn resize(&mut self, rows: usize, cols: usize, fill: &T) {
        let mut data = vec![fill.clone(); rows * cols];
        for c in 0..self.cols.min(cols) {
            for r in 0..self.rows.min(rows) {
                data[c * rows + r] = self.get(r, c).clone();
            }
        }
        self.rows = rows;
        self.cols = cols;
        self.data = data;
    }

    /// `A(idx) = []`.
    pub fn delete(&mut self, idx: &[Idx]) -> MResult<()> {
        match idx.len() {
            1 => {
                let n = self.numel();
                let (list, _) = idx[0].resolve(n);
                let mut kill = vec![false; n];
                for k in list {
                    if k >= n {
                        return Err(index_oob(k + 1, n));
                    }
                    kill[k] = true;
                }
                let data: Vec<T> = self
                    .data
                    .iter()
                    .zip(kill.iter())
                    .filter(|(_, k)| !**k)
                    .map(|(v, _)| v.clone())
                    .collect();
                let len = data.len();
                if self.cols == 1 && self.rows != 1 {
                    *self = Array {
                        rows: len,
                        cols: 1,
                        data,
                    };
                } else {
                    *self = Array {
                        rows: if len == 0 && self.rows != 1 { 0 } else { 1 },
                        cols: len,
                        data,
                    };
                }
                Ok(())
            }
            2 => {
                let all_r = matches!(idx[0], Idx::All)
                    || idx[0].resolve(self.rows).0.len() == self.rows
                        && is_full(&idx[0], self.rows);
                let all_c = matches!(idx[1], Idx::All) || is_full(&idx[1], self.cols);
                if all_r {
                    let kill: Vec<usize> = idx[1].resolve(self.cols).0;
                    let keep: Vec<usize> = (0..self.cols).filter(|c| !kill.contains(c)).collect();
                    let r = self.index(&[Idx::All, Idx::List(keep.clone(), (1, keep.len()))])?;
                    *self = r;
                    Ok(())
                } else if all_c {
                    let kill: Vec<usize> = idx[0].resolve(self.rows).0;
                    let keep: Vec<usize> = (0..self.rows).filter(|r| !kill.contains(r)).collect();
                    let r = self.index(&[Idx::List(keep.clone(), (keep.len(), 1)), Idx::All])?;
                    *self = r;
                    Ok(())
                } else {
                    Err(merr!(
                        "A null assignment can have only one non-colon index."
                    ))
                }
            }
            _ => Err(merr!("Invalid deletion")),
        }
    }
}

fn is_full(idx: &Idx, n: usize) -> bool {
    match idx {
        Idx::All => true,
        Idx::List(l, _) => {
            let mut seen = vec![false; n];
            for &k in l {
                if k < n {
                    seen[k] = true;
                }
            }
            seen.iter().all(|s| *s)
        }
    }
}

fn index_oob(k: usize, n: usize) -> MError {
    MError::with_id(
        "Beany:badsubscript",
        format!(
            "Index exceeds the number of array elements. Index {} must not exceed {}.",
            k, n
        ),
    )
}

/// Resolved index: either `:` or explicit 0-based positions with the index's own shape.
#[derive(Debug, Clone)]
pub enum Idx {
    All,
    List(Vec<usize>, (usize, usize)),
}

impl Idx {
    pub fn resolve(&self, n: usize) -> (Vec<usize>, (usize, usize)) {
        match self {
            Idx::All => ((0..n).collect(), (n, 1)),
            Idx::List(l, s) => (l.clone(), *s),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntKind {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

impl IntKind {
    pub fn name(self) -> &'static str {
        match self {
            IntKind::I8 => "int8",
            IntKind::I16 => "int16",
            IntKind::I32 => "int32",
            IntKind::I64 => "int64",
            IntKind::U8 => "uint8",
            IntKind::U16 => "uint16",
            IntKind::U32 => "uint32",
            IntKind::U64 => "uint64",
        }
    }
    pub fn range(self) -> (f64, f64) {
        match self {
            IntKind::I8 => (-128.0, 127.0),
            IntKind::I16 => (-32768.0, 32767.0),
            IntKind::I32 => (-2147483648.0, 2147483647.0),
            IntKind::I64 => (-9.223372036854776e18, 9.223372036854776e18),
            IntKind::U8 => (0.0, 255.0),
            IntKind::U16 => (0.0, 65535.0),
            IntKind::U32 => (0.0, 4294967295.0),
            IntKind::U64 => (0.0, 1.8446744073709552e19),
        }
    }
    pub fn from_name(s: &str) -> Option<IntKind> {
        Some(match s {
            "int8" => IntKind::I8,
            "int16" => IntKind::I16,
            "int32" => IntKind::I32,
            "int64" => IntKind::I64,
            "uint8" => IntKind::U8,
            "uint16" => IntKind::U16,
            "uint32" => IntKind::U32,
            "uint64" => IntKind::U64,
            _ => return None,
        })
    }
    /// Round half away from zero and saturate; NaN becomes 0.
    pub fn cast(self, x: f64) -> f64 {
        if x.is_nan() {
            return 0.0;
        }
        let (lo, hi) = self.range();
        x.round().clamp(lo, hi)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumClass {
    Double,
    Single,
    Logical,
    Int(IntKind),
}

impl NumClass {
    pub fn name(self) -> &'static str {
        match self {
            NumClass::Double => "double",
            NumClass::Single => "single",
            NumClass::Logical => "logical",
            NumClass::Int(k) => k.name(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FuncHandle {
    Named(String),
    Anon {
        params: Vec<String>,
        body: Arc<Expr>,
        captured: Arc<HashMap<String, Value>>,
        text: String,
        /// File the handle was created in, so its body can call that file's local functions.
        unit: Option<Arc<crate::ast::FileUnit>>,
    },
}

impl FuncHandle {
    pub fn text(&self) -> String {
        match self {
            FuncHandle::Named(n) => format!("@{}", n),
            FuncHandle::Anon { text, .. } => text.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StructArr {
    pub keys: Vec<String>,
    pub arr: Array<Vec<Value>>,
}

impl StructArr {
    pub fn scalar(fields: Vec<(String, Value)>) -> Self {
        let keys = fields.iter().map(|f| f.0.clone()).collect();
        let vals = fields.into_iter().map(|f| f.1).collect();
        StructArr {
            keys,
            arr: Array::scalar(vals),
        }
    }
    pub fn key_index(&self, k: &str) -> Option<usize> {
        self.keys.iter().position(|x| x == k)
    }
    pub fn add_key(&mut self, k: &str) -> usize {
        if let Some(i) = self.key_index(k) {
            return i;
        }
        self.keys.push(k.to_string());
        for e in self.arr.data.iter_mut() {
            e.push(Value::empty());
        }
        self.keys.len() - 1
    }
    pub fn get(&self, elem: usize, k: &str) -> Option<&Value> {
        self.key_index(k).map(|i| &self.arr.data[elem][i])
    }
    pub fn empty_elem(&self) -> Vec<Value> {
        vec![Value::empty(); self.keys.len()]
    }
}

/// Built-in object (transfer function, table, layout...). Behavior lives in `builtins::objects`.
#[derive(Debug, Clone)]
pub struct Object {
    pub class: &'static str,
    pub props: StructArr,
}

impl Object {
    pub fn new(class: &'static str, props: Vec<(String, Value)>) -> Self {
        Object {
            class,
            props: StructArr::scalar(props),
        }
    }
    pub fn get(&self, k: &str) -> Option<&Value> {
        self.props.get(0, k)
    }
    pub fn set(&mut self, k: &str, v: Value) {
        let i = self.props.add_key(k);
        self.props.arr.data[0][i] = v;
    }
    /// Size as the language reports it: tables are rows × variables, other objects 1×1.
    pub fn dims(&self) -> (usize, usize) {
        if self.class == "table" {
            let rows = self
                .get("Height")
                .and_then(|v| v.scalar().ok())
                .unwrap_or(0.0) as usize;
            let cols = self.get("Data").map(|v| v.numel()).unwrap_or(0);
            return (rows, cols);
        }
        (1, 1)
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Num(Array<f64>, NumClass),
    Char(Array<char>),
    Str(Array<String>),
    Cell(Array<Value>),
    Struct(StructArr),
    Func(FuncHandle),
    Obj(Box<Object>),
    /// Object of a user-defined class.
    Inst(Box<Instance>),
    /// Complex double array (real arrays never use this variant).
    Complex(Array<C64>),
}

/// An object (or object array) of a class defined in `.m` code.
#[derive(Debug, Clone)]
pub struct Instance {
    pub class: Arc<crate::classes::ClassInfo>,
    pub data: InstData,
}

#[derive(Debug, Clone)]
pub enum InstData {
    /// Value class: properties are copied with the object.
    Value(StructArr),
    /// Handle class: copies refer to the same properties.
    Handle(Arc<std::sync::Mutex<StructArr>>),
}

impl Instance {
    /// A copy of the property values.
    pub fn props(&self) -> StructArr {
        match &self.data {
            InstData::Value(s) => s.clone(),
            InstData::Handle(h) => h.lock().unwrap().clone(),
        }
    }
    pub fn dims(&self) -> (usize, usize) {
        match &self.data {
            InstData::Value(s) => (s.arr.rows, s.arr.cols),
            InstData::Handle(_) => (1, 1),
        }
    }
    pub fn with_props(&self, s: StructArr) -> Instance {
        Instance {
            class: self.class.clone(),
            data: InstData::Value(s),
        }
    }
    pub fn is_handle(&self) -> bool {
        matches!(self.data, InstData::Handle(_))
    }
}

impl Default for Value {
    fn default() -> Self {
        Value::empty()
    }
}

impl Value {
    pub fn num(x: f64) -> Value {
        Value::Num(Array::scalar(x), NumClass::Double)
    }
    pub fn boolean(b: bool) -> Value {
        Value::Num(Array::scalar(if b { 1.0 } else { 0.0 }), NumClass::Logical)
    }
    pub fn empty() -> Value {
        Value::Num(Array::empty(), NumClass::Double)
    }
    pub fn mat(a: Array<f64>) -> Value {
        Value::Num(a, NumClass::Double)
    }
    pub fn logical(a: Array<f64>) -> Value {
        Value::Num(a, NumClass::Logical)
    }
    pub fn row(v: Vec<f64>) -> Value {
        Value::mat(Array::row(v))
    }
    pub fn col(v: Vec<f64>) -> Value {
        Value::mat(Array::col(v))
    }
    pub fn chars(s: &str) -> Value {
        let v: Vec<char> = s.chars().collect();
        if v.is_empty() {
            Value::Char(Array::new(0, 0, v))
        } else {
            Value::Char(Array::row(v))
        }
    }
    pub fn string(s: &str) -> Value {
        Value::Str(Array::scalar(s.to_string()))
    }
    pub fn cell_row(v: Vec<Value>) -> Value {
        Value::Cell(Array::row(v))
    }
    /// A complex result, or a real one when every imaginary part is zero.
    pub fn complex(a: Array<C64>) -> Value {
        if a.data.iter().all(|z| z.im == 0.0) {
            Value::mat(a.map(|z| z.re))
        } else {
            Value::Complex(a)
        }
    }
    pub fn is_complex(&self) -> bool {
        matches!(self, Value::Complex(_))
    }
    /// Complex view of a numeric value.
    pub fn to_cplx(&self) -> MResult<Array<C64>> {
        match self {
            Value::Complex(a) => Ok(a.clone()),
            v => Ok(v.to_num()?.map(|x| C64::new(*x, 0.0))),
        }
    }

    pub fn dims(&self) -> (usize, usize) {
        match self {
            Value::Num(a, _) => (a.rows, a.cols),
            Value::Char(a) => (a.rows, a.cols),
            Value::Str(a) => (a.rows, a.cols),
            Value::Cell(a) => (a.rows, a.cols),
            Value::Struct(s) => (s.arr.rows, s.arr.cols),
            Value::Func(_) => (1, 1),
            Value::Obj(o) => o.dims(),
            Value::Inst(o) => o.dims(),
            Value::Complex(a) => (a.rows, a.cols),
        }
    }
    pub fn numel(&self) -> usize {
        let (r, c) = self.dims();
        r * c
    }
    pub fn is_empty(&self) -> bool {
        self.numel() == 0
    }
    pub fn class_name(&self) -> &'static str {
        match self {
            Value::Num(_, c) => c.name(),
            Value::Char(_) => "char",
            Value::Str(_) => "string",
            Value::Cell(_) => "cell",
            Value::Struct(_) => "struct",
            Value::Func(_) => "function_handle",
            Value::Obj(o) => o.class,
            Value::Inst(o) => o.class.name,
            Value::Complex(_) => "double",
        }
    }
    pub fn is_numeric_like(&self) -> bool {
        matches!(self, Value::Num(..) | Value::Char(_))
    }

    /// Numeric view (chars become code points, logicals 0/1).
    pub fn to_num(&self) -> MResult<Array<f64>> {
        match self {
            Value::Num(a, _) => Ok(a.clone()),
            Value::Char(a) => Ok(a.map(|c| *c as u32 as f64)),
            Value::Str(a) if a.numel() == 1 => a.data[0]
                .trim()
                .parse::<f64>()
                .map(Array::scalar)
                .or(Ok(Array::scalar(f64::NAN))),
            Value::Complex(_) => Err(merr!(
                "This function does not support complex values yet; use real, imag or abs first."
            )),
            other => Err(merr!("Expected numeric input, got {}", other.class_name())),
        }
    }
    pub fn num_class(&self) -> NumClass {
        match self {
            Value::Num(_, c) => *c,
            _ => NumClass::Double,
        }
    }
    pub fn scalar(&self) -> MResult<f64> {
        let a = self.to_num()?;
        if a.numel() < 1 {
            return Err(merr!("Expected a scalar value, got an empty array"));
        }
        Ok(a.data[0])
    }
    pub fn usize_scalar(&self, what: &str) -> MResult<usize> {
        let x = self.scalar()?;
        if x < 0.0 || x.fract() != 0.0 || !x.is_finite() {
            return Err(merr!("{} must be a nonnegative integer", what));
        }
        Ok(x as usize)
    }
    /// Text from a char row vector or string scalar.
    pub fn text(&self) -> MResult<String> {
        match self {
            Value::Char(a) => {
                if a.rows <= 1 {
                    Ok(a.data.iter().collect())
                } else {
                    // Multi-row char: read row by row.
                    let mut s = String::new();
                    for r in 0..a.rows {
                        for c in 0..a.cols {
                            s.push(*a.get(r, c));
                        }
                    }
                    Ok(s)
                }
            }
            Value::Str(a) if a.numel() == 1 => Ok(a.data[0].clone()),
            Value::Str(a) if a.numel() == 0 => Ok(String::new()),
            Value::Num(a, _) if a.is_empty() => Ok(String::new()),
            other => Err(merr!("Expected text input, got {}", other.class_name())),
        }
    }
    pub fn is_text(&self) -> bool {
        matches!(self, Value::Char(a) if a.rows <= 1)
            || matches!(self, Value::Str(a) if a.numel() == 1)
    }
    /// Truthiness for `if`/`while`: non-empty and all elements nonzero.
    pub fn is_true(&self) -> MResult<bool> {
        match self {
            Value::Num(a, _) => Ok(!a.is_empty()
                && a.data
                    .iter()
                    .all(|x| if x.is_nan() { false } else { *x != 0.0 })),
            Value::Char(a) => Ok(!a.is_empty() && a.data.iter().all(|c| *c != '\0')),
            Value::Str(_) => Err(merr!("Conversion to logical from string is not possible.")),
            Value::Cell(_) => Err(merr!("Conversion to logical from cell is not possible.")),
            Value::Struct(_) => Err(merr!("Conversion to logical from struct is not possible.")),
            Value::Func(_) => Err(merr!(
                "Conversion to logical from function_handle is not possible."
            )),
            Value::Obj(o) => Err(merr!(
                "Conversion to logical from {} is not possible.",
                o.class
            )),
            Value::Inst(o) => Err(merr!(
                "Conversion to logical from {} is not possible.",
                o.class.name
            )),
            Value::Complex(a) => {
                Ok(!a.is_empty() && a.data.iter().all(|z| z.re != 0.0 || z.im != 0.0))
            }
        }
    }

    pub fn index(&self, idx: &[Idx]) -> MResult<Value> {
        Ok(match self {
            Value::Num(a, c) => Value::Num(a.index(idx)?, *c),
            Value::Char(a) => Value::Char(a.index(idx)?),
            Value::Str(a) => Value::Str(a.index(idx)?),
            Value::Cell(a) => Value::Cell(a.index(idx)?),
            Value::Struct(s) => Value::Struct(StructArr {
                keys: s.keys.clone(),
                arr: s.arr.index(idx)?,
            }),
            Value::Func(_) => {
                return Err(merr!("Function handles cannot be indexed with parentheses"))
            }
            Value::Obj(o) => {
                return Err(merr!(
                    "Parentheses indexing is not supported for {} values yet.",
                    o.class
                ))
            }
            Value::Inst(o) => match &o.data {
                InstData::Value(s) => {
                    let mut s = s.clone();
                    s.arr = s.arr.index(idx)?;
                    Value::Inst(Box::new(o.with_props(s)))
                }
                InstData::Handle(_) => {
                    let one = Array::scalar(0.0).index(idx)?;
                    if one.numel() != 1 {
                        return Err(merr!("Index exceeds the number of array elements (1)."));
                    }
                    self.clone()
                }
            },
            Value::Complex(a) => Value::complex(a.index(idx)?),
        })
    }

    pub fn transpose(&self) -> MResult<Value> {
        Ok(match self {
            Value::Num(a, c) => Value::Num(a.transpose(), *c),
            Value::Char(a) => Value::Char(a.transpose()),
            Value::Str(a) => Value::Str(a.transpose()),
            Value::Cell(a) => Value::Cell(a.transpose()),
            Value::Struct(s) => Value::Struct(StructArr {
                keys: s.keys.clone(),
                arr: s.arr.transpose(),
            }),
            Value::Func(_) => return Err(merr!("Undefined operator ''' for function_handle")),
            Value::Obj(o) => return Err(merr!("Undefined operator ''' for {}", o.class)),
            Value::Inst(o) => match &o.data {
                InstData::Value(s) => {
                    let mut s = s.clone();
                    s.arr = s.arr.transpose();
                    Value::Inst(Box::new(o.with_props(s)))
                }
                InstData::Handle(_) => self.clone(),
            },
            Value::Complex(a) => Value::Complex(a.transpose()),
        })
    }

    pub fn reshape(&self, r: usize, c: usize) -> MResult<Value> {
        Ok(match self {
            Value::Num(a, k) => Value::Num(a.reshape(r, c)?, *k),
            Value::Char(a) => Value::Char(a.reshape(r, c)?),
            Value::Str(a) => Value::Str(a.reshape(r, c)?),
            Value::Cell(a) => Value::Cell(a.reshape(r, c)?),
            Value::Struct(s) => Value::Struct(StructArr {
                keys: s.keys.clone(),
                arr: s.arr.reshape(r, c)?,
            }),
            Value::Func(_) => return Err(merr!("Cannot reshape a function handle")),
            Value::Obj(o) => return Err(merr!("Cannot reshape a {}", o.class)),
            Value::Inst(o) => match &o.data {
                InstData::Value(s) => {
                    let mut s = s.clone();
                    s.arr = s.arr.reshape(r, c)?;
                    Value::Inst(Box::new(o.with_props(s)))
                }
                InstData::Handle(_) if r * c == 1 => self.clone(),
                InstData::Handle(_) => return Err(merr!("Cannot reshape a {}", o.class.name)),
            },
            Value::Complex(a) => Value::Complex(a.reshape(r, c)?),
        })
    }

    /// Elementwise split into scalar values (for cell conversion, arrayfun, for loops).
    pub fn elements(&self) -> Vec<Value> {
        match self {
            Value::Num(a, c) => a
                .data
                .iter()
                .map(|x| Value::Num(Array::scalar(*x), *c))
                .collect(),
            Value::Char(a) => a
                .data
                .iter()
                .map(|x| Value::Char(Array::scalar(*x)))
                .collect(),
            Value::Str(a) => a
                .data
                .iter()
                .map(|x| Value::Str(Array::scalar(x.clone())))
                .collect(),
            Value::Cell(a) => a
                .data
                .iter()
                .map(|x| Value::Cell(Array::scalar(x.clone())))
                .collect(),
            Value::Struct(s) => s
                .arr
                .data
                .iter()
                .map(|e| {
                    Value::Struct(StructArr {
                        keys: s.keys.clone(),
                        arr: Array::scalar(e.clone()),
                    })
                })
                .collect(),
            Value::Func(_) | Value::Obj(_) => vec![self.clone()],
            Value::Inst(o) => match &o.data {
                InstData::Value(s) => s
                    .arr
                    .data
                    .iter()
                    .map(|e| {
                        let mut one = s.clone();
                        one.arr = Array::scalar(e.clone());
                        Value::Inst(Box::new(o.with_props(one)))
                    })
                    .collect(),
                InstData::Handle(_) => vec![self.clone()],
            },
            Value::Complex(a) => a
                .data
                .iter()
                .map(|z| Value::complex(Array::scalar(*z)))
                .collect(),
        }
    }

    /// Column `c` as a value (for `for` loops).
    pub fn column(&self, c: usize) -> MResult<Value> {
        self.index(&[Idx::All, Idx::List(vec![c], (1, 1))])
    }
}

/// Convert a numeric index value to 0-based positions.
pub fn to_idx(v: &Value, dim_len: usize) -> MResult<Idx> {
    match v {
        Value::Num(a, NumClass::Logical) => {
            if a.numel() > dim_len && a.data[dim_len..].iter().any(|x| *x != 0.0) {
                return Err(merr!(
                    "The logical indices contain a true value outside of the array bounds."
                ));
            }
            let list: Vec<usize> = a
                .data
                .iter()
                .enumerate()
                .filter(|(_, x)| **x != 0.0)
                .map(|(i, _)| i)
                .collect();
            let n = list.len();
            let shape = if a.rows == 1 { (1, n) } else { (n, 1) };
            Ok(Idx::List(list, shape))
        }
        Value::Num(a, _) => {
            let mut list = Vec::with_capacity(a.numel());
            for &x in &a.data {
                if x.fract() != 0.0 || x < 1.0 || !x.is_finite() {
                    if x.fract() != 0.0 || !x.is_finite() {
                        return Err(MError::with_id(
                            "Beany:badsubscript",
                            "Array indices must be positive integers or logical values.",
                        ));
                    }
                    return Err(MError::with_id(
                        "Beany:badsubscript",
                        "Index must be a positive integer or logical.",
                    ));
                }
                list.push(x as usize - 1);
            }
            Ok(Idx::List(list, (a.rows, a.cols)))
        }
        Value::Char(a) if a.numel() == 1 && a.data[0] == ':' => Ok(Idx::All),
        Value::Char(a) => {
            let n = a.map(|c| *c as u32 as f64);
            to_idx(&Value::mat(n), dim_len)
        }
        other => Err(merr!(
            "Unable to use a value of type {} as an index.",
            other.class_name()
        )),
    }
}

pub fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Cell(x), Value::Cell(y)) => {
            x.rows == y.rows
                && x.cols == y.cols
                && x.data.iter().zip(&y.data).all(|(p, q)| values_equal(p, q))
        }
        (Value::Struct(x), Value::Struct(y)) => {
            if x.arr.rows != y.arr.rows || x.arr.cols != y.arr.cols || x.keys.len() != y.keys.len()
            {
                return false;
            }
            for (i, e) in x.arr.data.iter().enumerate() {
                for (k, key) in x.keys.iter().enumerate() {
                    match y.get(i, key) {
                        Some(v) if values_equal(&e[k], v) => {}
                        _ => return false,
                    }
                }
            }
            true
        }
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Str(x), Value::Char(_)) if x.numel() == 1 => b.text().is_ok_and(|t| t == x.data[0]),
        (Value::Char(_), Value::Str(y)) if y.numel() == 1 => a.text().is_ok_and(|t| t == y.data[0]),
        (Value::Func(x), Value::Func(y)) => x.text() == y.text(),
        (Value::Complex(_), _) | (_, Value::Complex(_)) => match (a.to_cplx(), b.to_cplx()) {
            (Ok(x), Ok(y)) => x.rows == y.rows && x.cols == y.cols && x.data == y.data,
            _ => false,
        },
        (Value::Inst(x), Value::Inst(y)) => {
            Arc::ptr_eq(&x.class, &y.class)
                && match (&x.data, &y.data) {
                    (InstData::Handle(p), InstData::Handle(q)) => Arc::ptr_eq(p, q),
                    _ => values_equal(&Value::Struct(x.props()), &Value::Struct(y.props())),
                }
        }
        _ => match (a.to_num(), b.to_num()) {
            (Ok(x), Ok(y)) if a.is_numeric_like() && b.is_numeric_like() => {
                x.rows == y.rows && x.cols == y.cols && x.data == y.data
            }
            _ => false,
        },
    }
}
