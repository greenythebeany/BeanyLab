//! MAT-file (Level 5) reader and writer, for interoperability with existing `.mat` data.
//!
//! Reads numeric, logical, char, cell, struct and sparse (as dense) arrays, including
//! zlib-compressed elements. Writes uncompressed Level 5 files that other tools can read.
//! Arrays with more than two dimensions are folded into 2-D (rows × remaining).

use std::io::Read;

use crate::error::MResult;
use crate::merr;
use crate::value::*;

const MI_INT8: u32 = 1;
const MI_UINT8: u32 = 2;
const MI_INT16: u32 = 3;
const MI_UINT16: u32 = 4;
const MI_INT32: u32 = 5;
const MI_UINT32: u32 = 6;
const MI_SINGLE: u32 = 7;
const MI_DOUBLE: u32 = 9;
const MI_INT64: u32 = 12;
const MI_UINT64: u32 = 13;
const MI_MATRIX: u32 = 14;
const MI_COMPRESSED: u32 = 15;
const MI_UTF8: u32 = 16;
const MI_UTF16: u32 = 17;
const MI_UTF32: u32 = 18;

const CLS_CELL: u8 = 1;
const CLS_STRUCT: u8 = 2;
const CLS_CHAR: u8 = 4;
const CLS_SPARSE: u8 = 5;
const CLS_DOUBLE: u8 = 6;
const CLS_SINGLE: u8 = 7;

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
    swap: bool,
}

impl<'a> Reader<'a> {
    fn u32(&mut self) -> MResult<u32> {
        let s = self
            .b
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| merr!("MAT-file is truncated."))?;
        self.pos += 4;
        let a = [s[0], s[1], s[2], s[3]];
        Ok(if self.swap {
            u32::from_be_bytes(a)
        } else {
            u32::from_le_bytes(a)
        })
    }

    /// Next data element: (type, payload bytes). Handles the small-element format and padding.
    fn element(&mut self) -> MResult<(u32, &'a [u8])> {
        let first = self.u32()?;
        if first >> 16 != 0 {
            let ty = first & 0xFFFF;
            let n = (first >> 16) as usize;
            let data = self
                .b
                .get(self.pos..self.pos + n)
                .ok_or_else(|| merr!("MAT-file is truncated."))?;
            self.pos += 4;
            return Ok((ty, data));
        }
        let n = self.u32()? as usize;
        let data = self
            .b
            .get(self.pos..self.pos + n)
            .ok_or_else(|| merr!("MAT-file is truncated."))?;
        self.pos += n;
        if first != MI_COMPRESSED {
            self.pos += (8 - n % 8) % 8;
        }
        Ok((first, data))
    }

    fn done(&self) -> bool {
        self.pos + 8 > self.b.len()
    }
}

fn numbers(ty: u32, d: &[u8], swap: bool) -> MResult<Vec<f64>> {
    macro_rules! conv {
        ($t:ty, $n:expr) => {
            d.chunks_exact($n)
                .map(|c| {
                    let mut a = [0u8; $n];
                    a.copy_from_slice(c);
                    (if swap {
                        <$t>::from_be_bytes(a)
                    } else {
                        <$t>::from_le_bytes(a)
                    }) as f64
                })
                .collect()
        };
    }
    Ok(match ty {
        MI_INT8 => d.iter().map(|x| *x as i8 as f64).collect(),
        MI_UINT8 | MI_UTF8 => d.iter().map(|x| *x as f64).collect(),
        MI_INT16 => conv!(i16, 2),
        MI_UINT16 | MI_UTF16 => conv!(u16, 2),
        MI_INT32 => conv!(i32, 4),
        MI_UINT32 | MI_UTF32 => conv!(u32, 4),
        MI_SINGLE => conv!(f32, 4),
        MI_DOUBLE => conv!(f64, 8),
        MI_INT64 => conv!(i64, 8),
        MI_UINT64 => conv!(u64, 8),
        other => return Err(merr!("Unsupported MAT data type {}.", other)),
    })
}

/// Read all variables from a MAT-file, in file order.
pub fn read(bytes: &[u8]) -> MResult<Vec<(String, Value)>> {
    if bytes.len() < 128 {
        return Err(merr!("File is not a MAT-file."));
    }
    if bytes.starts_with(b"\x89HDF") || bytes.get(512..516) == Some(b"\x89HDF") {
        return Err(merr!(
            "MAT-file version 7.3 (HDF5) is not supported yet. Re-save it with save(..., '-v7')."
        ));
    }
    let swap = match &bytes[126..128] {
        b"IM" => false,
        b"MI" => true,
        _ => return Err(merr!("File is not a Level 5 MAT-file.")),
    };
    let mut r = Reader {
        b: bytes,
        pos: 128,
        swap,
    };
    let mut out = Vec::new();
    while !r.done() {
        let (ty, data) = r.element()?;
        match ty {
            MI_COMPRESSED => {
                let mut buf = Vec::new();
                flate2::read::ZlibDecoder::new(data)
                    .read_to_end(&mut buf)
                    .map_err(|e| merr!("Could not decompress MAT-file data: {}", e))?;
                let mut inner = Reader {
                    b: &buf,
                    pos: 0,
                    swap,
                };
                let (ity, idata) = inner.element()?;
                if ity == MI_MATRIX {
                    if let Some(v) = matrix(idata, swap)? {
                        out.push(v);
                    }
                }
            }
            MI_MATRIX => {
                if let Some(v) = matrix(data, swap)? {
                    out.push(v);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn fold_dims(dims: &[f64]) -> (usize, usize) {
    let r = *dims.first().unwrap_or(&0.0) as usize;
    let c: usize = dims.iter().skip(1).map(|d| *d as usize).product();
    (r, if dims.len() < 2 { 1 } else { c })
}

/// Parse one miMATRIX payload into (name, value). Returns None for unsupported classes (objects).
fn matrix(data: &[u8], swap: bool) -> MResult<Option<(String, Value)>> {
    if data.is_empty() {
        return Ok(Some((String::new(), Value::empty())));
    }
    let mut r = Reader {
        b: data,
        pos: 0,
        swap,
    };
    let (_, flags) = r.element()?;
    let f = numbers(MI_UINT32, flags, swap)?;
    let flag0 = f[0] as u32;
    let class = (flag0 & 0xFF) as u8;
    let complex = flag0 & 0x800 != 0;
    let logical = flag0 & 0x200 != 0;
    let (dty, ddata) = r.element()?;
    let dims = numbers(dty, ddata, swap)?;
    let (rows, cols) = fold_dims(&dims);
    let (_, name) = r.element()?;
    let name = String::from_utf8_lossy(name)
        .trim_end_matches('\0')
        .to_string();
    let value = match class {
        CLS_CELL => {
            let mut items = Vec::with_capacity(rows * cols);
            for _ in 0..rows * cols {
                let (_, d) = r.element()?;
                items.push(matrix(d, swap)?.map(|x| x.1).unwrap_or_else(Value::empty));
            }
            Value::Cell(Array::new(rows, cols, items))
        }
        CLS_STRUCT => {
            let (lty, ld) = r.element()?;
            let len = numbers(lty, ld, swap)?.first().copied().unwrap_or(32.0) as usize;
            let (_, names) = r.element()?;
            let keys: Vec<String> = names
                .chunks(len.max(1))
                .map(|c| {
                    String::from_utf8_lossy(c)
                        .trim_end_matches('\0')
                        .to_string()
                })
                .filter(|s| !s.is_empty())
                .collect();
            let mut elems = Vec::with_capacity(rows * cols);
            for _ in 0..rows * cols {
                let mut e = Vec::with_capacity(keys.len());
                for _ in 0..keys.len() {
                    let (_, d) = r.element()?;
                    e.push(matrix(d, swap)?.map(|x| x.1).unwrap_or_else(Value::empty));
                }
                elems.push(e);
            }
            Value::Struct(StructArr {
                keys,
                arr: Array::new(rows, cols, elems),
            })
        }
        CLS_CHAR => {
            let (ty, d) = r.element()?;
            let codes: Vec<char> = if ty == MI_UTF8 {
                String::from_utf8_lossy(d).chars().collect()
            } else {
                numbers(ty, d, swap)?
                    .iter()
                    .map(|x| char::from_u32(*x as u32).unwrap_or('?'))
                    .collect()
            };
            let mut codes = codes;
            codes.resize(rows * cols, ' ');
            Value::Char(Array::new(rows, cols, codes))
        }
        CLS_SPARSE => {
            let (ity, idata) = r.element()?;
            let ir = numbers(ity, idata, swap)?;
            let (jty, jdata) = r.element()?;
            let jc = numbers(jty, jdata, swap)?;
            let (pty, pdata) = r.element()?;
            let pr = numbers(pty, pdata, swap)?;
            let mut dense = Array::filled(rows, cols, 0.0);
            for c in 0..cols {
                let (s, e) = (
                    jc[c] as usize,
                    jc.get(c + 1).copied().unwrap_or(0.0) as usize,
                );
                for k in s..e.min(pr.len()) {
                    dense.set(ir[k] as usize, c, pr[k]);
                }
            }
            Value::Num(
                dense,
                if logical {
                    NumClass::Logical
                } else {
                    NumClass::Double
                },
            )
        }
        CLS_DOUBLE..=15 => {
            let (ty, d) = r.element()?;
            let mut re = numbers(ty, d, swap)?;
            re.resize(rows * cols, 0.0);
            if complex {
                // Only the real part is kept until complex numbers are supported.
                let _ = r.element();
            }
            let nc = if logical {
                NumClass::Logical
            } else {
                match class {
                    CLS_DOUBLE => NumClass::Double,
                    CLS_SINGLE => NumClass::Single,
                    8 => NumClass::Int(IntKind::I8),
                    9 => NumClass::Int(IntKind::U8),
                    10 => NumClass::Int(IntKind::I16),
                    11 => NumClass::Int(IntKind::U16),
                    12 => NumClass::Int(IntKind::I32),
                    13 => NumClass::Int(IntKind::U32),
                    14 => NumClass::Int(IntKind::I64),
                    _ => NumClass::Int(IntKind::U64),
                }
            };
            Value::Num(Array::new(rows, cols, re), nc)
        }
        _ => return Ok(None),
    };
    Ok(Some((name, value)))
}

// ---------------- writer ----------------

fn pad(buf: &mut Vec<u8>) {
    while !buf.len().is_multiple_of(8) {
        buf.push(0);
    }
}

fn put_element(buf: &mut Vec<u8>, ty: u32, data: &[u8]) {
    buf.extend_from_slice(&ty.to_le_bytes());
    buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
    buf.extend_from_slice(data);
    pad(buf);
}

fn matrix_bytes(name: &str, v: &Value) -> MResult<Vec<u8>> {
    let (rows, cols) = v.dims();
    let mut body = Vec::new();
    let (class, logical, payload): (u8, bool, Box<dyn Fn(&mut Vec<u8>) -> MResult<()>>) = match v {
        Value::Num(a, c) => {
            let a = a.clone();
            match c {
                NumClass::Logical => (
                    9,
                    true,
                    Box::new(move |b: &mut Vec<u8>| {
                        let d: Vec<u8> = a.data.iter().map(|x| (*x != 0.0) as u8).collect();
                        put_element(b, MI_UINT8, &d);
                        Ok(())
                    }),
                ),
                NumClass::Single => (
                    CLS_SINGLE,
                    false,
                    Box::new(move |b: &mut Vec<u8>| {
                        let d: Vec<u8> = a
                            .data
                            .iter()
                            .flat_map(|x| (*x as f32).to_le_bytes())
                            .collect();
                        put_element(b, MI_SINGLE, &d);
                        Ok(())
                    }),
                ),
                NumClass::Int(k) => {
                    let k = *k;
                    let (cls, ty) = match k {
                        IntKind::I8 => (8, MI_INT8),
                        IntKind::U8 => (9, MI_UINT8),
                        IntKind::I16 => (10, MI_INT16),
                        IntKind::U16 => (11, MI_UINT16),
                        IntKind::I32 => (12, MI_INT32),
                        IntKind::U32 => (13, MI_UINT32),
                        IntKind::I64 => (14, MI_INT64),
                        IntKind::U64 => (15, MI_UINT64),
                    };
                    (
                        cls,
                        false,
                        Box::new(move |b: &mut Vec<u8>| {
                            let d: Vec<u8> = a
                                .data
                                .iter()
                                .flat_map(|x| match k {
                                    IntKind::I8 => (*x as i8).to_le_bytes().to_vec(),
                                    IntKind::U8 => (*x as u8).to_le_bytes().to_vec(),
                                    IntKind::I16 => (*x as i16).to_le_bytes().to_vec(),
                                    IntKind::U16 => (*x as u16).to_le_bytes().to_vec(),
                                    IntKind::I32 => (*x as i32).to_le_bytes().to_vec(),
                                    IntKind::U32 => (*x as u32).to_le_bytes().to_vec(),
                                    IntKind::I64 => (*x as i64).to_le_bytes().to_vec(),
                                    IntKind::U64 => (*x as u64).to_le_bytes().to_vec(),
                                })
                                .collect();
                            put_element(b, ty, &d);
                            Ok(())
                        }),
                    )
                }
                NumClass::Double => (
                    CLS_DOUBLE,
                    false,
                    Box::new(move |b: &mut Vec<u8>| {
                        let d: Vec<u8> = a.data.iter().flat_map(|x| x.to_le_bytes()).collect();
                        put_element(b, MI_DOUBLE, &d);
                        Ok(())
                    }),
                ),
            }
        }
        Value::Char(a) => {
            let a = a.clone();
            (
                CLS_CHAR,
                false,
                Box::new(move |b: &mut Vec<u8>| {
                    let d: Vec<u8> = a
                        .data
                        .iter()
                        .flat_map(|c| (*c as u32).min(0xFFFF).to_le_bytes()[..2].to_vec())
                        .collect();
                    put_element(b, MI_UINT16, &d);
                    Ok(())
                }),
            )
        }
        Value::Str(s) if s.numel() == 1 => return matrix_bytes(name, &Value::chars(&s.data[0])),
        Value::Cell(a) => {
            let a = a.clone();
            (
                CLS_CELL,
                false,
                Box::new(move |b: &mut Vec<u8>| {
                    for x in &a.data {
                        let inner = matrix_bytes("", x)?;
                        b.extend_from_slice(&inner);
                    }
                    Ok(())
                }),
            )
        }
        Value::Struct(s) => {
            let s = s.clone();
            (
                CLS_STRUCT,
                false,
                Box::new(move |b: &mut Vec<u8>| {
                    let len = s.keys.iter().map(|k| k.len() + 1).max().unwrap_or(1).max(1);
                    // Field name length as a small element.
                    b.extend_from_slice(&((4u32 << 16) | MI_INT32).to_le_bytes());
                    b.extend_from_slice(&(len as u32).to_le_bytes());
                    let mut names = Vec::new();
                    for k in &s.keys {
                        let mut n = k.as_bytes().to_vec();
                        n.resize(len, 0);
                        names.extend(n);
                    }
                    put_element(b, MI_INT8, &names);
                    for e in &s.arr.data {
                        for v in e {
                            let inner = matrix_bytes("", v)?;
                            b.extend_from_slice(&inner);
                        }
                    }
                    Ok(())
                }),
            )
        }
        other => {
            return Err(merr!(
                "Saving values of class {} is not supported yet.",
                other.class_name()
            ))
        }
    };
    let flags: u32 = class as u32 | if logical { 0x200 } else { 0 };
    let mut fl = Vec::new();
    fl.extend_from_slice(&flags.to_le_bytes());
    fl.extend_from_slice(&0u32.to_le_bytes());
    put_element(&mut body, MI_UINT32, &fl);
    let mut dims = Vec::new();
    dims.extend_from_slice(&(rows as i32).to_le_bytes());
    dims.extend_from_slice(&(cols as i32).to_le_bytes());
    put_element(&mut body, MI_INT32, &dims);
    put_element(&mut body, MI_INT8, name.as_bytes());
    payload(&mut body)?;
    let mut out = Vec::with_capacity(body.len() + 8);
    put_element(&mut out, MI_MATRIX, &body);
    Ok(out)
}

/// Serialize variables as an uncompressed Level 5 MAT-file.
pub fn write(vars: &[(String, Value)]) -> MResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut header = format!(
        "MATLAB 5.0 MAT-file, written by BeanyLab {}",
        env!("CARGO_PKG_VERSION")
    )
    .into_bytes();
    header.resize(116, b' ');
    out.extend(header);
    out.extend([0u8; 8]);
    out.extend(0x0100u16.to_le_bytes());
    out.extend(b"IM");
    for (n, v) in vars {
        out.extend(matrix_bytes(n, v)?);
    }
    Ok(out)
}
